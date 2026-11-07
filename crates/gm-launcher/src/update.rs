//! Self-update: a release (GitHub `releases/latest` or a local directory) carries the
//! platform zip, `SHA256SUMS` and `SHA256SUMS.minisig`; the sums are verified against the
//! embedded release key before the zip is checked and unpacked over the install directory.

use anyhow::{bail, Context, Result};
use std::path::{Path, PathBuf};

use crate::release::{self, SIG, SUMS};

/// The zip this build installs from (`.github/workflows/release.yml` names).
pub fn platform_asset() -> &'static str {
    if cfg!(target_os = "windows") {
        "gmod-rust-win-x64.zip"
    } else {
        "gmod-rust-linux-x64.zip"
    }
}

/// Where a release comes from.
pub enum Source {
    /// `owner/repo` on GitHub: the latest release's assets.
    GitHub(String),
    /// A directory holding the zip, `SHA256SUMS` and `SHA256SUMS.minisig` (offline / CI).
    Dir(PathBuf),
}

/// A release that can be fetched: its tag and how to get a named file.
pub struct Release {
    pub tag: String,
    files: Vec<(String, String)>,
}

impl Release {
    fn url(&self, name: &str) -> Option<&str> {
        self.files
            .iter()
            .find(|(n, _)| n == name)
            .map(|(_, u)| u.as_str())
    }
}

/// The latest release of the source.
pub fn latest(source: &Source) -> Result<Release> {
    match source {
        Source::Dir(dir) => {
            let tag = std::fs::read_to_string(dir.join("VERSION"))
                .map(|t| t.trim().to_string())
                .unwrap_or_else(|_| "local".to_string());
            let files = std::fs::read_dir(dir)
                .with_context(|| dir.display().to_string())?
                .flatten()
                .filter(|e| e.path().is_file())
                .map(|e| {
                    let n = e.file_name().to_string_lossy().to_string();
                    (n, e.path().display().to_string())
                })
                .collect();
            Ok(Release { tag, files })
        }
        Source::GitHub(repo) => {
            let url = format!("https://api.github.com/repos/{repo}/releases/latest");
            let body = ureq::get(&url)
                .set("User-Agent", "gm-launcher")
                .set("Accept", "application/vnd.github+json")
                .call()
                .with_context(|| url.clone())?
                .into_string()?;
            let v: serde_json::Value = serde_json::from_str(&body).context("release json")?;
            let tag = v["tag_name"].as_str().context("release tag")?.to_string();
            let files = v["assets"]
                .as_array()
                .map(|a| {
                    a.iter()
                        .filter_map(|x| {
                            Some((
                                x["name"].as_str()?.to_string(),
                                x["browser_download_url"].as_str()?.to_string(),
                            ))
                        })
                        .collect()
                })
                .unwrap_or_default();
            Ok(Release { tag, files })
        }
    }
}

fn fetch(source: &Source, rel: &Release, name: &str) -> Result<Vec<u8>> {
    let at = rel
        .url(name)
        .with_context(|| format!("release {} has no {name}", rel.tag))?;
    match source {
        Source::Dir(_) => std::fs::read(at).with_context(|| at.to_string()),
        Source::GitHub(_) => {
            let mut buf = Vec::new();
            ureq::get(at)
                .set("User-Agent", "gm-launcher")
                .call()
                .with_context(|| at.to_string())?
                .into_reader()
                .read_to_end(&mut buf)?;
            Ok(buf)
        }
    }
}

/// Whether `tag` is newer than this build (`v1.2.3` against CARGO_PKG_VERSION).
pub fn is_newer(tag: &str, current: &str) -> bool {
    let num = |s: &str| -> Vec<u64> {
        s.trim_start_matches('v')
            .split(['.', '-'])
            .map_while(|p| p.parse::<u64>().ok())
            .collect()
    };
    num(tag) > num(current)
}

/// Downloads the latest release into `dir/.update/<tag>`, verifies the sums' signature with
/// `public` and the zip against the sums, then unpacks the zip over `dir` (the running
/// launcher is renamed `.old` first). Returns the installed tag.
pub fn install(
    source: &Source,
    dir: &Path,
    public: &crate::minisign::PublicKey,
    running: Option<&Path>,
) -> Result<String> {
    let rel = latest(source)?;
    let stage = dir.join(".update").join(&rel.tag);
    std::fs::create_dir_all(&stage)?;
    let asset = platform_asset();
    for name in [SUMS, SIG, asset] {
        std::fs::write(stage.join(name), fetch(source, &rel, name)?)?;
    }
    let verified = release::verify_dir(&stage, public)?;
    if !verified.iter().any(|f| f == asset) {
        bail!("{asset} is not covered by the signed {SUMS}");
    }
    let zip = std::fs::File::open(stage.join(asset))?;
    let mut archive = zip::ZipArchive::new(zip).context("release zip")?;
    let mut installed = 0;
    for i in 0..archive.len() {
        let mut entry = archive.by_index(i)?;
        let Some(path) = entry.enclosed_name() else {
            continue;
        };
        // the zip holds one top directory (gmod-rust-<platform>/...)
        let rel_path: PathBuf = path.components().skip(1).collect();
        if rel_path.as_os_str().is_empty() || entry.is_dir() {
            continue;
        }
        let out = dir.join(&rel_path);
        if let Some(parent) = out.parent() {
            std::fs::create_dir_all(parent)?;
        }
        if running.is_some_and(|r| r == out) {
            let old = out.with_extension("old");
            let _ = std::fs::remove_file(&old);
            std::fs::rename(&out, &old).with_context(|| out.display().to_string())?;
        }
        let mut data = Vec::new();
        std::io::Read::read_to_end(&mut entry, &mut data)?;
        std::fs::write(&out, data).with_context(|| out.display().to_string())?;
        #[cfg(unix)]
        if let Some(mode) = entry.unix_mode() {
            use std::os::unix::fs::PermissionsExt;
            std::fs::set_permissions(&out, std::fs::Permissions::from_mode(mode))?;
        }
        installed += 1;
    }
    if installed == 0 {
        bail!("{asset} holds no files under its top directory");
    }
    std::fs::write(dir.join("VERSION"), format!("{}\n", rel.tag))?;
    let _ = std::fs::remove_dir_all(dir.join(".update"));
    Ok(rel.tag)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Write;

    #[test]
    fn newer_tags_compare_numerically() {
        assert!(is_newer("v0.2.0", "0.1.9"));
        assert!(!is_newer("v0.1.0", "0.1.0"));
        assert!(is_newer("v1.0.0", "0.10.3"));
    }

    #[test]
    fn a_signed_local_release_installs_over_the_directory() {
        let tmp = std::env::temp_dir().join(format!("gm-launcher-update-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&tmp);
        let src = tmp.join("release");
        let dst = tmp.join("install");
        std::fs::create_dir_all(&src).unwrap();
        std::fs::create_dir_all(&dst).unwrap();
        // the release zip: one top directory with the client "binary"
        let f = std::fs::File::create(src.join(platform_asset())).unwrap();
        let mut w = zip::ZipWriter::new(f);
        let opts = zip::write::SimpleFileOptions::default();
        w.start_file("gmod-rust-x/gm-client", opts).unwrap();
        w.write_all(b"new client").unwrap();
        w.start_file("gmod-rust-x/gm-launcher", opts).unwrap();
        w.write_all(b"new launcher").unwrap();
        w.finish().unwrap();
        std::fs::write(src.join("VERSION"), "v9.9.9\n").unwrap();
        let sums = release::write_sums(&src, &[platform_asset().to_string()]).unwrap();
        let key = crate::minisign::SecretKey::generate();
        std::fs::write(src.join(SIG), key.sign(sums.as_bytes(), "test", "test")).unwrap();
        std::fs::write(dst.join("gm-launcher"), b"old launcher").unwrap();
        let tag = install(
            &Source::Dir(src.clone()),
            &dst,
            &key.public(),
            Some(&dst.join("gm-launcher")),
        )
        .unwrap();
        assert_eq!(tag, "v9.9.9");
        assert_eq!(std::fs::read(dst.join("gm-client")).unwrap(), b"new client");
        assert_eq!(
            std::fs::read(dst.join("gm-launcher")).unwrap(),
            b"new launcher"
        );
        assert_eq!(
            std::fs::read(dst.join("gm-launcher.old")).unwrap(),
            b"old launcher"
        );
        assert_eq!(
            std::fs::read_to_string(dst.join("VERSION")).unwrap(),
            "v9.9.9\n"
        );
        // a tampered zip is refused
        std::fs::write(src.join(platform_asset()), b"evil").unwrap();
        assert!(install(&Source::Dir(src), &dst, &key.public(), None).is_err());
        let _ = std::fs::remove_dir_all(&tmp);
    }
}
