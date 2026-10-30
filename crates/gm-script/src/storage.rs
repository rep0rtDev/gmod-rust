//! Per-addon persistent key/value store: values in the `netval` wire form, one file per addon
//! and realm (`<data_dir>/<realm>/<addon>.gms`), written atomically when dirty.

use std::collections::BTreeMap;
use std::io::{Read, Write};
use std::path::Path;

pub const MAX_KEY: usize = 64;
/// Keys plus values of one addon.
pub const MAX_BYTES: usize = 1 << 20;
const MAGIC: &[u8; 4] = b"GMS1";

#[derive(Debug, Default)]
pub struct Store {
    pub map: BTreeMap<String, Vec<u8>>,
    pub dirty: bool,
}

impl Store {
    pub fn bytes(&self) -> usize {
        self.map.iter().map(|(k, v)| k.len() + v.len()).sum()
    }

    /// Sets or (empty value) removes a key; false when the addon's 1 MiB would be exceeded.
    pub fn set(&mut self, key: &str, value: Vec<u8>) -> bool {
        if value.is_empty() {
            self.dirty |= self.map.remove(key).is_some();
            return true;
        }
        let old = self.map.get(key).map(|v| v.len()).unwrap_or(0);
        if self.bytes() - old + value.len() + if old == 0 { key.len() } else { 0 } > MAX_BYTES {
            return false;
        }
        if self.map.get(key) != Some(&value) {
            self.map.insert(key.to_string(), value);
            self.dirty = true;
        }
        true
    }

    pub fn load(path: &Path) -> std::io::Result<Self> {
        let mut buf = Vec::new();
        std::fs::File::open(path)?.read_to_end(&mut buf)?;
        let bad = || std::io::Error::new(std::io::ErrorKind::InvalidData, "bad store file");
        if buf.len() < 8 || &buf[..4] != MAGIC {
            return Err(bad());
        }
        let n = u32::from_le_bytes(buf[4..8].try_into().unwrap()) as usize;
        let mut pos = 8;
        let mut map = BTreeMap::new();
        for _ in 0..n {
            let kl = *buf.get(pos).ok_or_else(bad)? as usize;
            pos += 1;
            let key = std::str::from_utf8(buf.get(pos..pos + kl).ok_or_else(bad)?)
                .map_err(|_| bad())?
                .to_string();
            pos += kl;
            let vl = u32::from_le_bytes(buf.get(pos..pos + 4).ok_or_else(bad)?.try_into().unwrap())
                as usize;
            pos += 4;
            let val = buf.get(pos..pos + vl).ok_or_else(bad)?.to_vec();
            pos += vl;
            map.insert(key, val);
        }
        Ok(Store { map, dirty: false })
    }

    pub fn save(&mut self, path: &Path) -> std::io::Result<()> {
        if let Some(dir) = path.parent() {
            std::fs::create_dir_all(dir)?;
        }
        let mut out = Vec::with_capacity(8 + self.bytes() + 5 * self.map.len());
        out.extend_from_slice(MAGIC);
        out.extend_from_slice(&(self.map.len() as u32).to_le_bytes());
        for (k, v) in &self.map {
            out.push(k.len() as u8);
            out.extend_from_slice(k.as_bytes());
            out.extend_from_slice(&(v.len() as u32).to_le_bytes());
            out.extend_from_slice(v);
        }
        let tmp = path.with_extension("gms.tmp");
        std::fs::File::create(&tmp)?.write_all(&out)?;
        std::fs::rename(&tmp, path)?;
        self.dirty = false;
        Ok(())
    }
}
