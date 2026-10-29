//! Valve Pak (VPK) version 1 and 2 directory trees.
//!
//! Layout (public VDC documentation): a directory file `<name>_dir.vpk` holds a
//! header and a tree of `extension / path / filename` entries. Each entry points
//! into either the directory file itself (archive index `0x7fff`) or into
//! `<name>_NNN.vpk`. Small files may carry a preload block inside the tree.

use std::collections::HashMap;
use std::fs::File;
use std::io::{Read, Seek, SeekFrom};
use std::path::{Path, PathBuf};

use crate::{FormatError, Reader, Result};

pub const SIGNATURE: u32 = 0x55aa_1234;
pub const DIR_ARCHIVE_INDEX: u16 = 0x7fff;

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Header {
    pub version: u32,
    pub tree_size: u32,
    /// v2 only; zero for v1.
    pub file_data_section_size: u32,
    pub archive_md5_section_size: u32,
    pub other_md5_section_size: u32,
    pub signature_section_size: u32,
    /// Byte offset where the tree starts (header length).
    pub tree_offset: u32,
}

impl Header {
    pub fn parse(data: &[u8]) -> Result<Self> {
        let mut r = Reader::new(data);
        let sig = r.u32()?;
        if sig != SIGNATURE {
            return Err(FormatError::BadMagic {
                expected: "VPK 0x55aa1234",
                found: data[..data.len().min(4)].to_vec(),
            });
        }
        let version = r.u32()?;
        let tree_size = r.u32()?;
        let mut h = Header {
            version,
            tree_size,
            file_data_section_size: 0,
            archive_md5_section_size: 0,
            other_md5_section_size: 0,
            signature_section_size: 0,
            tree_offset: 12,
        };
        match version {
            1 => {}
            2 => {
                h.file_data_section_size = r.u32()?;
                h.archive_md5_section_size = r.u32()?;
                h.other_md5_section_size = r.u32()?;
                h.signature_section_size = r.u32()?;
                h.tree_offset = 28;
            }
            v => {
                return Err(FormatError::UnsupportedVersion {
                    what: "VPK",
                    version: v,
                })
            }
        }
        Ok(h)
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Entry {
    pub crc: u32,
    pub preload_len: u16,
    pub archive_index: u16,
    /// Offset inside the archive, or inside the directory file's data section
    /// (relative to the end of the tree) when `archive_index == DIR_ARCHIVE_INDEX`.
    pub entry_offset: u32,
    pub entry_length: u32,
    /// Absolute offset of the preload bytes inside the directory file.
    pub preload_offset: u32,
}

impl Entry {
    pub fn total_len(&self) -> u64 {
        self.preload_len as u64 + self.entry_length as u64
    }
}

/// Parsed directory: full lowercase paths (`materials/models/foo.vmt`) to entries.
#[derive(Debug, Default)]
pub struct Directory {
    pub header: Option<Header>,
    pub entries: HashMap<String, Entry>,
}

impl Directory {
    pub fn parse(data: &[u8]) -> Result<Self> {
        let header = Header::parse(data)?;
        let mut r = Reader::at(data, header.tree_offset as usize);
        let tree_end = header.tree_offset as usize + header.tree_size as usize;
        let mut entries = HashMap::new();

        loop {
            let ext = r.cstr()?;
            if ext.is_empty() {
                break;
            }
            loop {
                let dir = r.cstr()?;
                if dir.is_empty() {
                    break;
                }
                loop {
                    let name = r.cstr()?;
                    if name.is_empty() {
                        break;
                    }
                    let crc = r.u32()?;
                    let preload_len = r.u16()?;
                    let archive_index = r.u16()?;
                    let entry_offset = r.u32()?;
                    let entry_length = r.u32()?;
                    let term = r.u16()?;
                    if term != 0xffff {
                        return Err(FormatError::Malformed {
                            what: "VPK entry",
                            detail: format!("terminator {term:#x} at {}", r.pos()),
                        });
                    }
                    let preload_offset = r.pos() as u32;
                    r.skip(preload_len as usize)?;
                    let full = join_path(dir, name, ext);
                    entries.insert(
                        full,
                        Entry {
                            crc,
                            preload_len,
                            archive_index,
                            entry_offset,
                            entry_length,
                            preload_offset,
                        },
                    );
                }
            }
        }
        if r.pos() > tree_end {
            return Err(FormatError::Malformed {
                what: "VPK tree",
                detail: format!("tree overran declared size: {} > {}", r.pos(), tree_end),
            });
        }
        Ok(Directory {
            header: Some(header),
            entries,
        })
    }

    pub fn len(&self) -> usize {
        self.entries.len()
    }

    pub fn is_empty(&self) -> bool {
        self.entries.is_empty()
    }

    pub fn get(&self, path: &str) -> Option<&Entry> {
        self.entries.get(&normalize(path))
    }

    /// Entries under a directory prefix (lowercase, forward slashes, no trailing slash).
    pub fn iter_prefix<'a>(
        &'a self,
        prefix: &'a str,
    ) -> impl Iterator<Item = (&'a str, &'a Entry)> + 'a {
        let p = normalize(prefix);
        self.entries.iter().filter_map(move |(k, v)| {
            if p.is_empty() || (k.starts_with(&p) && k.as_bytes().get(p.len()) == Some(&b'/')) {
                Some((k.as_str(), v))
            } else {
                None
            }
        })
    }
}

fn join_path(dir: &str, name: &str, ext: &str) -> String {
    let mut s = String::with_capacity(dir.len() + name.len() + ext.len() + 2);
    if dir != " " && !dir.is_empty() {
        s.push_str(dir);
        s.push('/');
    }
    s.push_str(name);
    if ext != " " && !ext.is_empty() {
        s.push('.');
        s.push_str(ext);
    }
    normalize(&s)
}

pub fn normalize(path: &str) -> String {
    let mut out: String = path.replace('\\', "/").to_ascii_lowercase();
    while out.starts_with('/') {
        out.remove(0);
    }
    out
}

/// A directory plus the files needed to read entry data.
pub struct Archive {
    dir_path: PathBuf,
    dir_data: Vec<u8>,
    pub directory: Directory,
}

impl Archive {
    pub fn open(dir_path: impl AsRef<Path>) -> Result<Self> {
        let dir_path = dir_path.as_ref().to_path_buf();
        let dir_data = std::fs::read(&dir_path)?;
        let directory = Directory::parse(&dir_data)?;
        Ok(Self {
            dir_path,
            dir_data,
            directory,
        })
    }

    pub fn dir_path(&self) -> &Path {
        &self.dir_path
    }

    /// `foo_dir.vpk` -> `foo_007.vpk`.
    pub fn archive_path(&self, index: u16) -> PathBuf {
        let stem = self
            .dir_path
            .file_name()
            .and_then(|s| s.to_str())
            .unwrap_or("");
        let base = stem.strip_suffix("_dir.vpk").unwrap_or(stem);
        self.dir_path
            .with_file_name(format!("{base}_{index:03}.vpk"))
    }

    pub fn read(&self, path: &str) -> Result<Vec<u8>> {
        let entry = self
            .directory
            .get(path)
            .ok_or_else(|| FormatError::Malformed {
                what: "VPK lookup",
                detail: format!("no entry {path}"),
            })?;
        self.read_entry(entry)
    }

    pub fn read_entry(&self, entry: &Entry) -> Result<Vec<u8>> {
        let mut out = Vec::with_capacity(entry.total_len() as usize);
        let p = entry.preload_offset as usize;
        out.extend_from_slice(&self.dir_data[p..p + entry.preload_len as usize]);
        if entry.entry_length == 0 {
            return Ok(out);
        }
        if entry.archive_index == DIR_ARCHIVE_INDEX {
            let h = self
                .directory
                .header
                .as_ref()
                .expect("parsed directory has header");
            let start = h.tree_offset as usize + h.tree_size as usize + entry.entry_offset as usize;
            let end = start + entry.entry_length as usize;
            let slice = self.dir_data.get(start..end).ok_or(FormatError::Eof {
                offset: start,
                needed: entry.entry_length as usize,
            })?;
            out.extend_from_slice(slice);
        } else {
            let mut f = File::open(self.archive_path(entry.archive_index))?;
            f.seek(SeekFrom::Start(entry.entry_offset as u64))?;
            let mut buf = vec![0u8; entry.entry_length as usize];
            f.read_exact(&mut buf)?;
            out.extend_from_slice(&buf);
        }
        Ok(out)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn cstr(v: &mut Vec<u8>, s: &str) {
        v.extend_from_slice(s.as_bytes());
        v.push(0);
    }

    /// Build a v2 directory with two files: `materials/a.vmt` (preload only) and
    /// `root.txt` (data in the dir file after the tree).
    fn synthetic_v2() -> Vec<u8> {
        let mut tree = Vec::new();
        cstr(&mut tree, "vmt");
        cstr(&mut tree, "materials");
        cstr(&mut tree, "a");
        tree.extend_from_slice(&0xdead_beefu32.to_le_bytes());
        tree.extend_from_slice(&3u16.to_le_bytes()); // preload
        tree.extend_from_slice(&DIR_ARCHIVE_INDEX.to_le_bytes());
        tree.extend_from_slice(&0u32.to_le_bytes());
        tree.extend_from_slice(&0u32.to_le_bytes());
        tree.extend_from_slice(&0xffffu16.to_le_bytes());
        tree.extend_from_slice(b"abc");
        cstr(&mut tree, "");
        cstr(&mut tree, "");
        cstr(&mut tree, "txt");
        cstr(&mut tree, " ");
        cstr(&mut tree, "root");
        tree.extend_from_slice(&1u32.to_le_bytes());
        tree.extend_from_slice(&0u16.to_le_bytes());
        tree.extend_from_slice(&DIR_ARCHIVE_INDEX.to_le_bytes());
        tree.extend_from_slice(&2u32.to_le_bytes()); // offset after tree
        tree.extend_from_slice(&5u32.to_le_bytes());
        tree.extend_from_slice(&0xffffu16.to_le_bytes());
        cstr(&mut tree, "");
        cstr(&mut tree, "");
        cstr(&mut tree, "");

        let data = b"xxhello";
        let mut out = Vec::new();
        out.extend_from_slice(&SIGNATURE.to_le_bytes());
        out.extend_from_slice(&2u32.to_le_bytes());
        out.extend_from_slice(&(tree.len() as u32).to_le_bytes());
        out.extend_from_slice(&(data.len() as u32).to_le_bytes());
        out.extend_from_slice(&0u32.to_le_bytes());
        out.extend_from_slice(&0u32.to_le_bytes());
        out.extend_from_slice(&0u32.to_le_bytes());
        out.extend_from_slice(&tree);
        out.extend_from_slice(data);
        out
    }

    #[test]
    fn parses_v2_tree() {
        let bytes = synthetic_v2();
        let dir = Directory::parse(&bytes).unwrap();
        assert_eq!(dir.len(), 2);
        let a = dir.get("materials/A.vmt").unwrap();
        assert_eq!(a.crc, 0xdead_beef);
        assert_eq!(a.preload_len, 3);
        assert_eq!(a.archive_index, DIR_ARCHIVE_INDEX);
        let root = dir.get("root.txt").unwrap();
        assert_eq!(root.entry_length, 5);
        assert_eq!(dir.iter_prefix("materials").count(), 1);
        assert_eq!(dir.iter_prefix("").count(), 2);
    }

    #[test]
    fn reads_preload_and_dir_data() {
        let bytes = synthetic_v2();
        let tmp =
            std::env::temp_dir().join(format!("gm_formats_test_{}_dir.vpk", std::process::id()));
        std::fs::write(&tmp, &bytes).unwrap();
        let ar = Archive::open(&tmp).unwrap();
        assert_eq!(ar.read("materials/a.vmt").unwrap(), b"abc");
        assert_eq!(ar.read("root.txt").unwrap(), b"hello");
        assert!(ar
            .archive_path(7)
            .ends_with(format!("gm_formats_test_{}_007.vpk", std::process::id())));
        std::fs::remove_file(&tmp).unwrap();
    }

    #[test]
    fn rejects_bad_magic() {
        assert!(matches!(
            Header::parse(b"nope....").unwrap_err(),
            FormatError::BadMagic { .. }
        ));
    }
}
