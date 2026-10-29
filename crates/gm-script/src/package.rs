//! `.gmr`: an addon directory as a zip (deflate) with `manifest.toml` at the root, the Luau
//! sources under `src/`, later `wasm/` and `assets/`. Entries are sorted so the same tree packs
//! to the same bytes; `sha256` of the file is the addon's identity for servers and the launcher.

use crate::manifest::{Manifest, ManifestError};
use crate::ActorSpec;
use std::collections::BTreeMap;
use std::io::{Read, Write};
use std::path::{Path, PathBuf};

/// Packs `dir` into `.gmr` bytes.
pub fn pack(dir: &Path) -> Result<Vec<u8>, ManifestError> {
    let mut files: BTreeMap<String, Vec<u8>> = BTreeMap::new();
    collect(dir, dir, &mut files)?;
    if !files.contains_key("manifest.toml") {
        return Err(ManifestError::Io(std::io::Error::new(
            std::io::ErrorKind::NotFound,
            "manifest.toml",
        )));
    }
    let mut buf = std::io::Cursor::new(Vec::new());
    {
        let mut zip = zip::ZipWriter::new(&mut buf);
        let opts = zip::write::SimpleFileOptions::default()
            .compression_method(zip::CompressionMethod::Deflated)
            .last_modified_time(zip::DateTime::default());
        for (name, data) in &files {
            zip.start_file(name, opts)?;
            zip.write_all(data)?;
        }
        zip.finish()?;
    }
    Ok(buf.into_inner())
}

fn collect(root: &Path, dir: &Path, out: &mut BTreeMap<String, Vec<u8>>) -> std::io::Result<()> {
    let mut entries: Vec<PathBuf> = std::fs::read_dir(dir)?
        .map(|e| e.map(|e| e.path()))
        .collect::<Result<_, _>>()?;
    entries.sort();
    for p in entries {
        if p.is_dir() {
            collect(root, &p, out)?;
            continue;
        }
        let rel = p
            .strip_prefix(root)
            .unwrap()
            .to_string_lossy()
            .replace('\\', "/");
        let keep = rel == "manifest.toml"
            || (rel.starts_with("src/") && rel.ends_with(".luau"))
            || rel.starts_with("wasm/")
            || rel.starts_with("assets/");
        if keep {
            out.insert(rel, std::fs::read(&p)?);
        }
    }
    Ok(())
}

/// A `.gmr` read into memory.
pub struct Package {
    pub files: BTreeMap<String, Vec<u8>>,
    pub sha256: [u8; 32],
}

impl Package {
    pub fn read(bytes: &[u8]) -> Result<Package, ManifestError> {
        use sha2::Digest;
        let mut zip = zip::ZipArchive::new(std::io::Cursor::new(bytes))?;
        let mut files = BTreeMap::new();
        for i in 0..zip.len() {
            let mut f = zip.by_index(i)?;
            let Some(name) = f.enclosed_name() else {
                continue;
            };
            let name = name.to_string_lossy().replace('\\', "/");
            let mut data = Vec::new();
            f.read_to_end(&mut data)?;
            files.insert(name, data);
        }
        Ok(Package {
            files,
            sha256: sha2::Sha256::digest(bytes).into(),
        })
    }

    pub fn open(path: &Path) -> Result<Package, ManifestError> {
        Package::read(&std::fs::read(path)?)
    }

    pub fn sha256_hex(&self) -> String {
        self.sha256.iter().map(|b| format!("{b:02x}")).collect()
    }

    /// The manifest and the server actors, like `Manifest::load` for a directory.
    pub fn load(&self) -> Result<(Manifest, Vec<ActorSpec>), ManifestError> {
        let text = self.files.get("manifest.toml").ok_or_else(|| {
            ManifestError::Io(std::io::Error::new(
                std::io::ErrorKind::NotFound,
                "manifest.toml",
            ))
        })?;
        let m: Manifest = toml::from_str(std::str::from_utf8(text).map_err(|e| {
            ManifestError::Io(std::io::Error::new(std::io::ErrorKind::InvalidData, e))
        })?)?;
        let mut specs = Vec::new();
        for a in &m.actors {
            if a.realm != "server" {
                continue;
            }
            let data = self.files.get(&a.source).ok_or_else(|| {
                ManifestError::Escape(m.id.clone(), a.name.clone(), a.source.clone())
            })?;
            let source = String::from_utf8_lossy(data).to_string();
            specs.push(m.actor_spec(a, source));
        }
        Ok((m, specs))
    }
}

impl From<zip::result::ZipError> for ManifestError {
    fn from(e: zip::result::ZipError) -> Self {
        ManifestError::Io(std::io::Error::other(e))
    }
}
