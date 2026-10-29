//! Content mounting. See `docs/spec/content-mounting.md`.
//!
//! A [`Mount`] is an ordered list of providers (VPK archives and loose
//! directories). Lookups are case-insensitive with forward slashes; the first
//! provider that has the path wins, which is how the original filesystem
//! layers its search paths.

use std::collections::{BTreeMap, BTreeSet};
use std::path::{Path, PathBuf};

use gm_formats::vpk;

mod install;
pub use install::{find_install, Install};

#[derive(Debug, thiserror::Error)]
pub enum ContentError {
    #[error("not a Garry's Mod install: {0}")]
    NotAnInstall(PathBuf),
    #[error("file not found in mounted content: {0}")]
    NotFound(String),
    #[error(transparent)]
    Format(#[from] gm_formats::FormatError),
    #[error(transparent)]
    Io(#[from] std::io::Error),
}

pub type Result<T> = std::result::Result<T, ContentError>;

pub enum Provider {
    Vpk(vpk::Archive),
    Dir {
        root: PathBuf,
        index: BTreeMap<String, PathBuf>,
    },
}

impl Provider {
    pub fn label(&self) -> String {
        match self {
            Provider::Vpk(a) => a.dir_path().display().to_string(),
            Provider::Dir { root, .. } => root.display().to_string(),
        }
    }

    fn has(&self, path: &str) -> bool {
        match self {
            Provider::Vpk(a) => a.directory.get(path).is_some(),
            Provider::Dir { index, .. } => index.contains_key(path),
        }
    }

    fn read(&self, path: &str) -> Result<Vec<u8>> {
        match self {
            Provider::Vpk(a) => Ok(a.read(path)?),
            Provider::Dir { root, index } => {
                let rel = index
                    .get(path)
                    .ok_or_else(|| ContentError::NotFound(path.to_string()))?;
                Ok(std::fs::read(root.join(rel))?)
            }
        }
    }

    fn paths(&self) -> Box<dyn Iterator<Item = &str> + '_> {
        match self {
            Provider::Vpk(a) => Box::new(a.directory.entries.keys().map(String::as_str)),
            Provider::Dir { index, .. } => Box::new(index.keys().map(String::as_str)),
        }
    }
}

/// Index a loose directory: every regular file, stored as a normalized relative path.
fn index_dir(root: &Path) -> BTreeMap<String, PathBuf> {
    let mut set = BTreeMap::new();
    for entry in walkdir::WalkDir::new(root)
        .follow_links(false)
        .into_iter()
        .filter_map(|e| e.ok())
    {
        if !entry.file_type().is_file() {
            continue;
        }
        if let Ok(rel) = entry.path().strip_prefix(root) {
            if let Some(s) = rel.to_str() {
                set.insert(vpk::normalize(s), rel.to_path_buf());
            }
        }
    }
    set
}

#[derive(Default)]
pub struct Mount {
    providers: Vec<Provider>,
}

impl Mount {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn push_vpk(&mut self, dir_vpk: impl AsRef<Path>) -> Result<&mut Self> {
        self.providers
            .push(Provider::Vpk(vpk::Archive::open(dir_vpk)?));
        Ok(self)
    }

    pub fn push_dir(&mut self, root: impl AsRef<Path>) -> &mut Self {
        let root = root.as_ref().to_path_buf();
        let index = index_dir(&root);
        self.providers.push(Provider::Dir { root, index });
        self
    }

    /// Mount a Garry's Mod install in the order described in the spec, without
    /// addons, workshop content or extra mounted games.
    pub fn for_install(install: &Install) -> Result<Self> {
        let g = &install.game_dir;
        let mut m = Mount::new();
        m.push_vpk(g.join("garrysmod/garrysmod_dir.vpk"))?;
        m.push_dir(g.join("garrysmod"));
        let fallbacks = g.join("garrysmod/fallbacks_dir.vpk");
        if fallbacks.exists() {
            m.push_vpk(fallbacks)?;
        }
        for name in [
            "sourceengine/hl2_textures_dir.vpk",
            "sourceengine/hl2_misc_dir.vpk",
            "sourceengine/hl2_sound_misc_dir.vpk",
            "sourceengine/hl2_sound_vo_english_dir.vpk",
            "sourceengine/content_hl2_dir.vpk",
            "sourceengine/content_cstrike_dir.vpk",
        ] {
            let p = g.join(name);
            if p.exists() {
                m.push_vpk(p)?;
            }
        }
        m.push_dir(g.join("sourceengine"));
        let platform_vpk = g.join("platform/platform_misc_dir.vpk");
        if platform_vpk.exists() {
            m.push_vpk(platform_vpk)?;
        }
        if g.join("platform").is_dir() {
            m.push_dir(g.join("platform"));
        }
        Ok(m)
    }

    pub fn providers(&self) -> &[Provider] {
        &self.providers
    }

    pub fn exists(&self, path: &str) -> bool {
        let p = vpk::normalize(path);
        self.providers.iter().any(|pr| pr.has(&p))
    }

    /// Which provider (index into `providers`) serves this path.
    pub fn resolve(&self, path: &str) -> Option<usize> {
        let p = vpk::normalize(path);
        self.providers.iter().position(|pr| pr.has(&p))
    }

    pub fn read(&self, path: &str) -> Result<Vec<u8>> {
        let p = vpk::normalize(path);
        for pr in &self.providers {
            if pr.has(&p) {
                return pr.read(&p);
            }
        }
        Err(ContentError::NotFound(p))
    }

    pub fn read_string(&self, path: &str) -> Result<String> {
        let bytes = self.read(path)?;
        Ok(String::from_utf8_lossy(&bytes).into_owned())
    }

    /// All distinct paths matching a glob of the form `dir/*.ext` (one level) or
    /// `dir/**/*.ext` (recursive). `*` alone matches any name.
    pub fn find(&self, pattern: &str) -> BTreeSet<String> {
        let pat = vpk::normalize(pattern);
        let (dir, recursive, name_pat) = split_glob(&pat);
        let mut out = BTreeSet::new();
        for pr in &self.providers {
            for p in pr.paths() {
                let rest = if dir.is_empty() {
                    p
                } else if let Some(r) = p.strip_prefix(dir).and_then(|r| r.strip_prefix('/')) {
                    r
                } else {
                    continue;
                };
                let name = match rest.rfind('/') {
                    Some(i) => {
                        if !recursive {
                            continue;
                        }
                        &rest[i + 1..]
                    }
                    None => rest,
                };
                if name_matches(name_pat, name) {
                    out.insert(p.to_string());
                }
            }
        }
        out
    }
}

/// `models/props_c17/*.mdl` -> ("models/props_c17", false, "*.mdl");
/// `materials/**/*.vmt` -> ("materials", true, "*.vmt").
fn split_glob(pat: &str) -> (&str, bool, &str) {
    let (dir, name) = match pat.rfind('/') {
        Some(i) => (&pat[..i], &pat[i + 1..]),
        None => ("", pat),
    };
    if let Some(d) = dir.strip_suffix("/**") {
        (d, true, name)
    } else if dir == "**" {
        ("", true, name)
    } else {
        (dir, false, name)
    }
}

/// Only `*` wildcards, as `file.Find` supports.
fn name_matches(pat: &str, name: &str) -> bool {
    fn rec(p: &[u8], n: &[u8]) -> bool {
        match p.split_first() {
            None => n.is_empty(),
            Some((b'*', rest)) => (0..=n.len()).any(|i| rec(rest, &n[i..])),
            Some((c, rest)) => n.first() == Some(c) && rec(rest, &n[1..]),
        }
    }
    rec(pat.as_bytes(), name.as_bytes())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn glob_split_and_match() {
        assert_eq!(
            split_glob("models/props_c17/*.mdl"),
            ("models/props_c17", false, "*.mdl")
        );
        assert_eq!(
            split_glob("materials/**/*.vmt"),
            ("materials", true, "*.vmt")
        );
        assert_eq!(split_glob("*.txt"), ("", false, "*.txt"));
        assert!(name_matches("*.mdl", "oildrum001.mdl"));
        assert!(!name_matches("*.mdl", "oildrum001.vvd"));
        assert!(name_matches("gm_*", "gm_construct.bsp"));
        assert!(name_matches("*", "anything"));
    }

    #[test]
    fn dir_provider_precedence() {
        let tmp = std::env::temp_dir().join(format!("gm_content_{}", std::process::id()));
        let a = tmp.join("a");
        let b = tmp.join("b");
        std::fs::create_dir_all(a.join("Materials/Models")).unwrap();
        std::fs::create_dir_all(b.join("materials/models")).unwrap();
        std::fs::write(a.join("Materials/Models/X.vmt"), b"from a").unwrap();
        std::fs::write(b.join("materials/models/x.vmt"), b"from b").unwrap();
        std::fs::write(b.join("materials/only_b.vmt"), b"b").unwrap();
        let mut m = Mount::new();
        m.push_dir(&a).push_dir(&b);
        assert_eq!(m.read("materials/models/x.vmt").unwrap(), b"from a");
        assert_eq!(m.resolve("MATERIALS/only_b.vmt"), Some(1));
        assert_eq!(m.find("materials/**/*.vmt").len(), 2);
        assert_eq!(m.find("materials/*.vmt").len(), 1);
        assert!(m.read("nope").is_err());
        std::fs::remove_dir_all(&tmp).unwrap();
    }
}
