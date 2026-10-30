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
    pack_files(&files)
}

/// The deterministic zip of these entries (sorted names, fixed timestamps).
pub fn pack_files(files: &BTreeMap<String, Vec<u8>>) -> Result<Vec<u8>, ManifestError> {
    let mut buf = std::io::Cursor::new(Vec::new());
    {
        let mut zip = zip::ZipWriter::new(&mut buf);
        let opts = zip::write::SimpleFileOptions::default()
            .compression_method(zip::CompressionMethod::Deflated)
            .last_modified_time(zip::DateTime::default());
        for (name, data) in files {
            zip.start_file(name, opts)?;
            zip.write_all(data)?;
        }
        zip.finish()?;
    }
    Ok(buf.into_inner())
}

pub(crate) fn collect(
    root: &Path,
    dir: &Path,
    out: &mut BTreeMap<String, Vec<u8>>,
) -> std::io::Result<()> {
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
            || rel == SIGNATURE
            || (rel.starts_with("src/") && rel.ends_with(".luau"))
            || rel.starts_with("wasm/")
            || rel.starts_with("assets/");
        if keep {
            out.insert(rel, std::fs::read(&p)?);
        }
    }
    Ok(())
}

/// The entry holding detached signatures: lines `gmr-sig1 <public key hex> <signature hex>`,
/// each an ed25519 signature of the content hash (every other entry, so the same signature
/// holds for the directory and for the `.gmr` packed from it).
pub const SIGNATURE: &str = "signature";

/// Hash of the entries except `signature`: `len(name) name len(data) data` for each, sorted.
pub fn content_sha256(files: &BTreeMap<String, Vec<u8>>) -> [u8; 32] {
    use sha2::Digest;
    let mut h = sha2::Sha256::new();
    for (name, data) in files {
        if name == SIGNATURE {
            continue;
        }
        h.update((name.len() as u64).to_le_bytes());
        h.update(name.as_bytes());
        h.update((data.len() as u64).to_le_bytes());
        h.update(data);
    }
    h.finalize().into()
}

/// Signs the content hash and returns the `signature` entry text with this signature appended
/// to the existing lines (a package can carry several signers).
pub fn sign(files: &BTreeMap<String, Vec<u8>>, secret: &[u8; 32]) -> String {
    use ed25519_dalek::Signer;
    let key = ed25519_dalek::SigningKey::from_bytes(secret);
    let sig = key.sign(&content_sha256(files));
    // existing lines are kept only while they still verify (a re-sign after an edit drops the
    // stale ones instead of leaving a package nobody can load)
    let mut text = String::new();
    if let Some(old) = files.get(SIGNATURE) {
        for line in String::from_utf8_lossy(old).lines() {
            let mut one = BTreeMap::new();
            one.insert(SIGNATURE.to_string(), line.as_bytes().to_vec());
            for (k, v) in files {
                if k != SIGNATURE {
                    one.insert(k.clone(), v.clone());
                }
            }
            if signers(&one).is_ok_and(|s| !s.is_empty()) {
                text.push_str(line);
                text.push('\n');
            }
        }
    }
    text.push_str(&format!(
        "gmr-sig1 {} {}\n",
        hex(key.verifying_key().as_bytes()),
        hex(&sig.to_bytes())
    ));
    text
}

/// A fresh signing key (32 secret bytes) and its public key.
pub fn keygen() -> ([u8; 32], [u8; 32]) {
    let key = ed25519_dalek::SigningKey::generate(&mut rand_core::OsRng);
    (key.to_bytes(), key.verifying_key().to_bytes())
}

pub fn hex(bytes: &[u8]) -> String {
    bytes.iter().map(|b| format!("{b:02x}")).collect()
}

pub fn unhex(s: &str) -> Option<Vec<u8>> {
    let s = s.trim();
    if !s.len().is_multiple_of(2) {
        return None;
    }
    (0..s.len())
        .step_by(2)
        .map(|i| u8::from_str_radix(&s[i..i + 2], 16).ok())
        .collect()
}

/// The public keys whose signature on this content verifies; a malformed or wrong signature is
/// an error (an unsigned package has none).
pub fn signers(files: &BTreeMap<String, Vec<u8>>) -> Result<Vec<[u8; 32]>, SignatureError> {
    use ed25519_dalek::Verifier;
    let Some(text) = files.get(SIGNATURE) else {
        return Ok(Vec::new());
    };
    let hash = content_sha256(files);
    let mut out = Vec::new();
    for line in String::from_utf8_lossy(text).lines() {
        let line = line.trim();
        if line.is_empty() {
            continue;
        }
        let parts: Vec<&str> = line.split_whitespace().collect();
        let bad = || SignatureError::Malformed(line.to_string());
        if parts.len() != 3 || parts[0] != "gmr-sig1" {
            return Err(bad());
        }
        let pk: [u8; 32] = unhex(parts[1])
            .and_then(|v| v.try_into().ok())
            .ok_or_else(bad)?;
        let sig: [u8; 64] = unhex(parts[2])
            .and_then(|v| v.try_into().ok())
            .ok_or_else(bad)?;
        let key = ed25519_dalek::VerifyingKey::from_bytes(&pk).map_err(|_| bad())?;
        key.verify(&hash, &ed25519_dalek::Signature::from_bytes(&sig))
            .map_err(|_| SignatureError::Invalid(hex(&pk)))?;
        out.push(pk);
    }
    Ok(out)
}

#[derive(Debug, thiserror::Error)]
pub enum SignatureError {
    #[error("malformed signature line `{0}`")]
    Malformed(String),
    #[error("signature by {0} does not match the content")]
    Invalid(String),
    #[error("addon {0} is not signed by a trusted key")]
    Untrusted(String),
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
        hex(&self.sha256)
    }

    pub fn content_sha256(&self) -> [u8; 32] {
        content_sha256(&self.files)
    }

    pub fn signers(&self) -> Result<Vec<[u8; 32]>, SignatureError> {
        signers(&self.files)
    }

    /// The manifest and the server actors, like `Manifest::load` for a directory.
    pub fn load(&self) -> Result<(Manifest, Vec<ActorSpec>), ManifestError> {
        self.load_realm("server")
    }

    pub fn load_realm(&self, realm: &str) -> Result<(Manifest, Vec<ActorSpec>), ManifestError> {
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
            if a.realm != realm && a.realm != "shared" {
                continue;
            }
            let data = self.files.get(&a.source).ok_or_else(|| {
                ManifestError::Escape(m.id.clone(), a.name.clone(), a.source.clone())
            })?;
            let source = String::from_utf8_lossy(data).to_string();
            let mut spec = m.actor_spec(a, source);
            spec.config.can_write &= realm == "server";
            specs.push(spec);
        }
        Ok((m, specs))
    }
}

impl From<zip::result::ZipError> for ManifestError {
    fn from(e: zip::result::ZipError) -> Self {
        ManifestError::Io(std::io::Error::other(e))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn signatures_verify_and_tampering_or_a_wrong_key_is_caught() {
        let mut files = BTreeMap::new();
        files.insert("manifest.toml".to_string(), b"id = \"x\"".to_vec());
        files.insert("src/a.luau".to_string(), b"state.x = 1".to_vec());
        assert!(signers(&files).unwrap().is_empty());
        let (sk, pk) = keygen();
        files.insert(SIGNATURE.to_string(), sign(&files, &sk).into_bytes());
        assert_eq!(signers(&files).unwrap(), vec![pk]);
        let packed = pack_files(&files).unwrap();
        let pkg = Package::read(&packed).unwrap();
        assert_eq!(
            pkg.signers().unwrap(),
            vec![pk],
            "the same signature holds in the zip"
        );
        let (sk2, pk2) = keygen();
        files.insert(SIGNATURE.to_string(), sign(&files, &sk2).into_bytes());
        assert_eq!(
            signers(&files).unwrap(),
            vec![pk, pk2],
            "a second signer is appended"
        );
        files.insert("src/a.luau".to_string(), b"state.x = 2".to_vec());
        assert!(matches!(signers(&files), Err(SignatureError::Invalid(_))));
        files.insert(SIGNATURE.to_string(), sign(&files, &sk2).into_bytes());
        assert_eq!(
            signers(&files).unwrap(),
            vec![pk2],
            "re-signing after an edit drops the stale lines"
        );
        files.insert(SIGNATURE.to_string(), b"gmr-sig1 zz".to_vec());
        assert!(matches!(signers(&files), Err(SignatureError::Malformed(_))));
    }
}
