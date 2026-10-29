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
        Cmd::Inspect { file } => {
            let pkg =
                gm_script::Package::open(&file).with_context(|| file.display().to_string())?;
            let (m, specs) = pkg.load()?;
            println!(
                "{} {} priority {} caps {:?} sha256 {}",
                m.id,
                m.version,
                m.priority,
                m.capabilities,
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
