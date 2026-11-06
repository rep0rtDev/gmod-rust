//! A release directory: the artifacts, `SHA256SUMS` (`<hex>  <name>` lines like sha256sum's)
//! and `SHA256SUMS.minisig` by the release key.

use anyhow::{bail, Context, Result};
use sha2::{Digest, Sha256};
use std::path::Path;

pub const SUMS: &str = "SHA256SUMS";
pub const SIG: &str = "SHA256SUMS.minisig";

/// The public key the launcher trusts for releases (`dist/release.pub` in the tree).
pub const RELEASE_PUBLIC_KEY: &str = include_str!("../../../dist/release.pub");

pub fn sha256_hex(data: &[u8]) -> String {
    crate::minisign::encode_hex(&Sha256::digest(data))
}

/// `SHA256SUMS` text for the given files of `dir` (sorted by name).
pub fn write_sums(dir: &Path, names: &[String]) -> Result<String> {
    let mut names = names.to_vec();
    names.sort();
    let mut out = String::new();
    for n in names {
        let data = std::fs::read(dir.join(&n)).with_context(|| n.clone())?;
        out.push_str(&format!("{}  {n}\n", sha256_hex(&data)));
    }
    std::fs::write(dir.join(SUMS), &out).context(SUMS)?;
    Ok(out)
}

/// Every `SHA256SUMS` entry matches the file next to it; returns the checked names.
pub fn check_sums(dir: &Path, sums: &str) -> Result<Vec<String>> {
    let mut checked = Vec::new();
    for line in sums.lines().filter(|l| !l.trim().is_empty()) {
        let (hex, name) = line
            .split_once("  ")
            .with_context(|| format!("bad SHA256SUMS line {line:?}"))?;
        let name = name.trim_start_matches('*');
        let data = std::fs::read(dir.join(name)).with_context(|| name.to_string())?;
        if sha256_hex(&data) != hex.trim() {
            bail!("{name}: sha256 differs from SHA256SUMS");
        }
        checked.push(name.to_string());
    }
    if checked.is_empty() {
        bail!("SHA256SUMS lists no files");
    }
    Ok(checked)
}

/// `SHA256SUMS.minisig` verifies with `public` and every listed file matches.
pub fn verify_dir(dir: &Path, public: &crate::minisign::PublicKey) -> Result<Vec<String>> {
    let sums = std::fs::read(dir.join(SUMS)).context(SUMS)?;
    let sig = std::fs::read_to_string(dir.join(SIG)).context(SIG)?;
    crate::minisign::verify(public, &sig, &sums)?;
    check_sums(dir, std::str::from_utf8(&sums).context("SHA256SUMS utf-8")?)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn signed_release_dir_verifies_until_a_file_changes() {
        let dir = std::env::temp_dir().join(format!("gm-launcher-rel-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        std::fs::write(dir.join("a.zip"), b"aaa").unwrap();
        std::fs::write(dir.join("b.zip"), b"bbb").unwrap();
        let sums = write_sums(&dir, &["b.zip".into(), "a.zip".into()]).unwrap();
        assert!(sums.starts_with(&format!("{}  a.zip\n", sha256_hex(b"aaa"))));
        let sk = crate::minisign::SecretKey::generate();
        std::fs::write(dir.join(SIG), sk.sign(sums.as_bytes(), "t", "u")).unwrap();
        let pk = sk.public();
        assert_eq!(verify_dir(&dir, &pk).unwrap(), vec!["a.zip", "b.zip"]);
        std::fs::write(dir.join("b.zip"), b"bbc").unwrap();
        assert!(verify_dir(&dir, &pk).is_err());
        assert!(crate::minisign::PublicKey::parse(RELEASE_PUBLIC_KEY).is_ok());
        let _ = std::fs::remove_dir_all(&dir);
    }
}
