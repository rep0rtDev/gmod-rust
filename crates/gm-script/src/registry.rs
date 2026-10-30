//! The addon registry as a static layout any file host can serve: `index.toml` lists every
//! published package (id, version, sha256, size, depends, signers), the packages sit in
//! `blobs/<sha256>.gmr`. `gmr publish` adds to it; servers and clients resolve `id@requirement`
//! against the index (highest matching version, dependencies pulled in, one version per id) and
//! install the blobs into a cache after checking the sha256 and the signatures.

use crate::depends::{DependsError, Requirement, Version};
use crate::package::{self, Package, SignatureError};
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

#[derive(Clone, Debug, Default, Serialize, Deserialize, PartialEq, Eq)]
pub struct Index {
    #[serde(default)]
    pub packages: Vec<Entry>,
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
pub struct Entry {
    pub id: String,
    pub version: String,
    /// Hex sha256 of the `.gmr` bytes (the blob name).
    pub sha: String,
    pub size: u64,
    #[serde(default)]
    pub depends: Vec<String>,
    #[serde(default)]
    pub realms: Vec<String>,
    /// Hex public keys whose signatures the package carries.
    #[serde(default)]
    pub signers: Vec<String>,
}

#[derive(Debug, thiserror::Error)]
pub enum RegistryError {
    #[error("{0}")]
    Io(#[from] std::io::Error),
    #[error("index: {0}")]
    Toml(#[from] toml::de::Error),
    #[error("index: {0}")]
    TomlWrite(#[from] toml::ser::Error),
    #[error("{0}")]
    Package(#[from] crate::manifest::ManifestError),
    #[error("{0}")]
    Signature(#[from] SignatureError),
    #[error("{0}")]
    Depends(#[from] DependsError),
    #[error("addon {0} is not signed; the registry takes signed packages only")]
    Unsigned(String),
    #[error("addon {0} {1} is already published with a different content ({2})")]
    Conflict(String, String, String),
    #[error("no package satisfies `{0}`")]
    Unsatisfied(String),
    #[error("{0} is wanted as `{1}` and `{2}`, no version fits both")]
    Incompatible(String, String, String),
    #[error("blob {0}: sha256 mismatch")]
    Corrupt(String),
    #[error("{0}")]
    Http(#[from] crate::http::HttpError),
    #[error("registry {0} is read only (publish into a directory)")]
    ReadOnly(String),
    #[error("addon {0} is {1} bytes, over the {max} byte cap", max = MAX_PACKAGE)]
    TooLarge(String, u64),
}

/// Where a registry lives: a directory (publishable) or a plain-http base url (read only).
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Source {
    Dir(PathBuf),
    Http(String),
}

pub struct Registry {
    pub source: Source,
    pub index: Index,
}

/// Published packages larger than this are refused (the design's cap).
pub const MAX_PACKAGE: u64 = 50 << 20;

impl Registry {
    /// Opens (or starts) the registry at `dir`.
    pub fn open(dir: &Path) -> Result<Registry, RegistryError> {
        let path = dir.join("index.toml");
        let index = if path.exists() {
            toml::from_str(&std::fs::read_to_string(&path)?)?
        } else {
            Index::default()
        };
        Ok(Registry {
            source: Source::Dir(dir.to_path_buf()),
            index,
        })
    }

    /// A directory path or an `http://` url.
    pub fn open_any(where_: &str) -> Result<Registry, RegistryError> {
        if where_.starts_with("http://") {
            Registry::open_http(where_)
        } else {
            Registry::open(Path::new(where_))
        }
    }

    /// Reads the index of a registry served over plain http (`<base>/index.toml`).
    pub fn open_http(base: &str) -> Result<Registry, RegistryError> {
        let base = base.trim_end_matches('/').to_string();
        let bytes = crate::http::get(&format!("{base}/index.toml"))?;
        let index = toml::from_str(&String::from_utf8_lossy(&bytes))?;
        Ok(Registry {
            source: Source::Http(base),
            index,
        })
    }

    fn dir(&self) -> Result<&Path, RegistryError> {
        match &self.source {
            Source::Dir(d) => Ok(d),
            Source::Http(u) => Err(RegistryError::ReadOnly(u.clone())),
        }
    }

    fn save(&self) -> Result<(), RegistryError> {
        let dir = self.dir()?;
        std::fs::create_dir_all(dir.join("blobs"))?;
        let text = toml::to_string(&self.index)?;
        std::fs::write(dir.join("index.toml"), text)?;
        Ok(())
    }

    pub fn blob_path(&self, sha_hex: &str) -> PathBuf {
        match &self.source {
            Source::Dir(d) => d.join("blobs").join(format!("{sha_hex}.gmr")),
            Source::Http(u) => PathBuf::from(format!("{u}/blobs/{sha_hex}.gmr")),
        }
    }

    fn read_blob(&self, sha_hex: &str) -> Result<Vec<u8>, RegistryError> {
        Ok(match &self.source {
            Source::Dir(d) => std::fs::read(d.join("blobs").join(format!("{sha_hex}.gmr")))?,
            Source::Http(u) => crate::http::get(&format!("{u}/blobs/{sha_hex}.gmr"))?,
        })
    }

    /// Publishes a signed package (`.gmr` bytes): a version is immutable, publishing the same
    /// id and version again is fine only with the same bytes.
    pub fn publish(&mut self, bytes: &[u8]) -> Result<Entry, RegistryError> {
        self.dir()?;
        let pkg = Package::read(bytes)?;
        let (m, _) = pkg.load_realm("server")?;
        if bytes.len() as u64 > MAX_PACKAGE {
            return Err(RegistryError::TooLarge(m.id, bytes.len() as u64));
        }
        let signers = pkg.signers()?;
        if signers.is_empty() {
            return Err(RegistryError::Unsigned(m.id));
        }
        Version::parse(&m.version)
            .ok_or_else(|| DependsError::BadVersion(m.id.clone(), m.version.clone()))?;
        for d in &m.depends {
            Requirement::parse(d)?;
        }
        let sha = pkg.sha256_hex();
        if let Some(e) = self
            .index
            .packages
            .iter()
            .find(|e| e.id == m.id && e.version == m.version)
        {
            if e.sha != sha {
                return Err(RegistryError::Conflict(m.id, m.version, e.sha.clone()));
            }
            return Ok(e.clone());
        }
        let entry = Entry {
            id: m.id.clone(),
            version: m.version.clone(),
            sha: sha.clone(),
            size: bytes.len() as u64,
            depends: m.depends.clone(),
            realms: m.realms.clone(),
            signers: signers.iter().map(|k| package::hex(k)).collect(),
        };
        std::fs::create_dir_all(self.dir()?.join("blobs"))?;
        std::fs::write(self.blob_path(&sha), bytes)?;
        self.index.packages.push(entry.clone());
        self.index.packages.sort_by(|a, b| {
            (&a.id, Version::parse(&a.version)).cmp(&(&b.id, Version::parse(&b.version)))
        });
        self.save()?;
        Ok(entry)
    }

    fn versions_of<'a>(&'a self, id: &'a str) -> impl Iterator<Item = (&'a Entry, Version)> + 'a {
        self.index
            .packages
            .iter()
            .filter(move |e| e.id == id)
            .filter_map(|e| Version::parse(&e.version).map(|v| (e, v)))
    }

    /// Picks one version per addon for these requirements and everything they depend on: the
    /// highest version satisfying every requirement placed on the id. Returned in dependency
    /// order (what is needed first).
    pub fn resolve(&self, wanted: &[String]) -> Result<Vec<Entry>, RegistryError> {
        let mut reqs: BTreeMap<String, Vec<Requirement>> = BTreeMap::new();
        let mut queue: Vec<String> = wanted.to_vec();
        let mut chosen: BTreeMap<String, Entry> = BTreeMap::new();
        while let Some(w) = queue.pop() {
            let r = Requirement::parse(&w)?;
            let list = reqs.entry(r.id.clone()).or_default();
            if list.contains(&r) {
                continue;
            }
            list.push(r.clone());
            let pick = self
                .versions_of(&r.id)
                .filter(|(_, v)| list.iter().all(|q| q.matches(*v)))
                .max_by_key(|(_, v)| *v)
                .map(|(e, _)| e.clone());
            let Some(pick) = pick else {
                if list.len() > 1 {
                    let shown: Vec<String> = list.iter().map(show).collect();
                    return Err(RegistryError::Incompatible(
                        r.id.clone(),
                        shown[0].clone(),
                        shown[1..].join(", "),
                    ));
                }
                return Err(RegistryError::Unsatisfied(w));
            };
            let changed = chosen.get(&r.id) != Some(&pick);
            if changed {
                for d in &pick.depends {
                    queue.push(d.clone());
                }
                chosen.insert(r.id.clone(), pick);
            }
        }
        let pairs: Vec<(String, Vec<String>)> = chosen
            .values()
            .map(|e| (e.id.clone(), e.depends.clone()))
            .collect();
        let order = crate::depends::order(&pairs)?;
        let entries: Vec<Entry> = chosen.into_values().collect();
        Ok(order.into_iter().map(|i| entries[i].clone()).collect())
    }

    /// Copies the blob of an entry into `cache/<sha>.gmr` (kept if already there and sound),
    /// checking the sha256 and that every signature verifies.
    pub fn install(&self, e: &Entry, cache: &Path) -> Result<PathBuf, RegistryError> {
        std::fs::create_dir_all(cache)?;
        let out = cache.join(format!("{}.gmr", e.sha));
        if let Ok(bytes) = std::fs::read(&out) {
            if Package::read(&bytes).is_ok_and(|p| p.sha256_hex() == e.sha) {
                return Ok(out);
            }
        }
        let bytes = self.read_blob(&e.sha)?;
        let pkg = Package::read(&bytes)?;
        if pkg.sha256_hex() != e.sha {
            return Err(RegistryError::Corrupt(e.sha.clone()));
        }
        pkg.signers()?;
        std::fs::write(&out, &bytes)?;
        Ok(out)
    }

    /// The cached path of a package by sha, if this registry has it: what a client tries before
    /// asking the server for the bytes.
    pub fn fetch_by_sha(&self, sha_hex: &str, cache: &Path) -> Option<PathBuf> {
        let e = self.index.packages.iter().find(|e| e.sha == sha_hex)?;
        self.install(e, cache).ok()
    }
}

fn show(r: &Requirement) -> String {
    use crate::depends::Op;
    let mut s = r.id.clone();
    for (i, (op, v)) in r.constraints.iter().enumerate() {
        s.push_str(if i == 0 { " " } else { ", " });
        s.push_str(match op {
            Op::Eq => "= ",
            Op::Gt => "> ",
            Op::Ge => ">= ",
            Op::Lt => "< ",
            Op::Le => "<= ",
            Op::Caret => "^",
            Op::Tilde => "~",
        });
        s.push_str(&format!("{}.{}.{}", v.0, v.1, v.2));
    }
    s
}
