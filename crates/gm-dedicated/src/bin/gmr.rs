//! `gmr pack <dir> <out.gmr>` / `gmr inspect <file.gmr>`: the addon package tool.
use anyhow::{Context, Result};
use clap::{Parser, Subcommand};
use std::path::PathBuf;

#[derive(Parser)]
#[command(about = "gmod-rust addon packages (.gmr)")]
struct Args {
    #[command(subcommand)]
    cmd: Cmd,
}

#[derive(Subcommand)]
enum Cmd {
    /// Pack an addon directory (manifest.toml, src/**/*.luau, wasm/, assets/) into a .gmr
    Pack { dir: PathBuf, out: PathBuf },
    /// Print the manifest, files and sha256 of a .gmr
    Inspect { file: PathBuf },
    /// Make a signing key: the secret (hex) goes to `out`, the public key is printed
    Keygen { out: PathBuf },
    /// Sign an addon directory or .gmr with the secret key in `key` (adds a `signature` line)
    Sign { addon: PathBuf, key: PathBuf },
    /// Verify the signatures of an addon directory or .gmr (exit 1 when none verifies)
    Verify { addon: PathBuf },
}

fn files_of(addon: &std::path::Path) -> Result<std::collections::BTreeMap<String, Vec<u8>>> {
    Ok(if addon.is_dir() {
        let bytes = gm_script::package::pack(addon)?;
        gm_script::Package::read(&bytes)?.files
    } else {
        gm_script::Package::open(addon)?.files
    })
}

fn main() -> Result<()> {
    match Args::parse().cmd {
        Cmd::Pack { dir, out } => {
            let bytes =
                gm_script::package::pack(&dir).with_context(|| dir.display().to_string())?;
            std::fs::write(&out, &bytes)?;
            let pkg = gm_script::Package::read(&bytes)?;
            println!(
                "{} ({} bytes, {} files) sha256 {}",
                out.display(),
                bytes.len(),
                pkg.files.len(),
                pkg.sha256_hex()
            );
        }
        Cmd::Keygen { out } => {
            let (sk, pk) = gm_script::package::keygen();
            std::fs::write(&out, gm_script::package::hex(&sk))?;
            println!("public key {}", gm_script::package::hex(&pk));
        }
        Cmd::Sign { addon, key } => {
            let sk: [u8; 32] = gm_script::package::unhex(&std::fs::read_to_string(&key)?)
                .and_then(|v| v.try_into().ok())
                .context("the key file holds 32 hex bytes")?;
            let mut files = files_of(&addon)?;
            let text = gm_script::package::sign(&files, &sk);
            if addon.is_dir() {
                std::fs::write(addon.join(gm_script::package::SIGNATURE), &text)?;
            } else {
                files.insert(gm_script::package::SIGNATURE.to_string(), text.into_bytes());
                std::fs::write(&addon, gm_script::package::pack_files(&files)?)?;
            }
            let files = files_of(&addon)?;
            for k in gm_script::package::signers(&files)? {
                println!("signed by {}", gm_script::package::hex(&k));
            }
        }
        Cmd::Verify { addon } => {
            let files = files_of(&addon)?;
            let signers = gm_script::package::signers(&files)?;
            println!(
                "content sha256 {}",
                gm_script::package::hex(&gm_script::package::content_sha256(&files))
            );
            for k in &signers {
                println!("signed by {}", gm_script::package::hex(k));
            }
            if signers.is_empty() {
                anyhow::bail!("{}: not signed", addon.display());
            }
        }
        Cmd::Inspect { file } => {
            let pkg =
                gm_script::Package::open(&file).with_context(|| file.display().to_string())?;
            let (m, specs) = pkg.load()?;
            println!(
                "{} {} priority {} caps {:?} depends {:?} sha256 {}",
                m.id,
                m.version,
                m.priority,
                m.capabilities,
                m.depends,
                pkg.sha256_hex()
            );
            for s in &specs {
                println!("  actor {} ({} bytes)", s.name, s.source.len());
            }
            for (name, data) in &pkg.files {
                println!("  {name} {} bytes", data.len());
            }
        }
    }
    Ok(())
}
