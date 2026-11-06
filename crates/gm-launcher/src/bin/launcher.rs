//! `gm-launcher`: the command-line face of the launcher (the window comes with the client).

use anyhow::{bail, Context, Result};
use clap::{Parser, Subcommand};
use std::path::PathBuf;

#[derive(Parser)]
#[command(version, about)]
struct Args {
    #[command(subcommand)]
    cmd: Cmd,
}

#[derive(Subcommand)]
enum Cmd {
    /// Where Garry's Mod is (Steam libraries, `GMOD_DIR`), its build id and branch.
    Find {
        /// Check this directory instead of searching.
        dir: Option<PathBuf>,
    },
    /// Ask a master server for its list and query every server.
    Servers {
        #[arg(long, default_value = "127.0.0.1:27010")]
        master: String,
    },
    /// Make a release signing key: the secret (hex) to `out`, the public key next to it as
    /// `<out>.pub` in minisign's form.
    Keygen { out: PathBuf },
    /// Write `SHA256SUMS` for the files of `dir` and sign it as `SHA256SUMS.minisig`.
    Sign {
        #[arg(long)]
        key: PathBuf,
        dir: PathBuf,
        /// Files to list (default: everything in `dir` except the sums and signature).
        files: Vec<String>,
    },
    /// Check a release directory against the embedded release key (or `--pub`).
    Verify {
        dir: PathBuf,
        #[arg(long = "pub")]
        public: Option<PathBuf>,
    },
    /// Start the client (next to this binary) on a map or against a server.
    Play {
        #[arg(long)]
        connect: Option<String>,
        #[arg(long, default_value = "gm_construct")]
        map: String,
        /// Garry's Mod directory (otherwise searched like `find`).
        #[arg(long)]
        gmod: Option<PathBuf>,
    },
}

fn main() -> Result<()> {
    match Args::parse().cmd {
        Cmd::Find { dir } => {
            let install = match dir {
                Some(d) => gm_launcher::Install::at(&d)?,
                None => gm_launcher::find_install().with_context(|| {
                    format!(
                        "no complete Garry's Mod install in {:?}; set GMOD_DIR",
                        gm_launcher::steam_roots()
                    )
                })?,
            };
            println!("game dir : {}", install.game_dir.display());
            println!(
                "build id : {}",
                install.build_id.as_deref().unwrap_or("unknown")
            );
            println!(
                "branch   : {}",
                install.branch.as_deref().unwrap_or("public")
            );
        }
        Cmd::Servers { master } => {
            let list = gm_launcher::servers::browse(&master)?;
            println!("{} servers", list.len());
            for s in list {
                println!(
                    "{}  \"{}\"  {}  {}/{}  {}  seen {} s ago",
                    s.addr,
                    s.info.name,
                    s.info.map,
                    s.info.players,
                    s.info.max_players,
                    s.ping
                        .map(|p| format!("ping {:.0} ms", p.as_secs_f32() * 1000.0))
                        .unwrap_or_else(|| "no answer".into()),
                    s.seen_s
                );
            }
        }
        Cmd::Keygen { out } => {
            let sk = gm_launcher::minisign::SecretKey::generate();
            std::fs::write(&out, sk.to_hex()).with_context(|| out.display().to_string())?;
            let pub_path = out.with_extension("pub");
            std::fs::write(
                &pub_path,
                sk.public()
                    .to_text("minisign public key of gmod-rust releases"),
            )?;
            println!(
                "secret key {}\npublic key {}",
                out.display(),
                pub_path.display()
            );
        }
        Cmd::Sign { key, dir, files } => {
            let sk = gm_launcher::minisign::SecretKey::from_hex(
                &std::fs::read_to_string(&key).with_context(|| key.display().to_string())?,
            )?;
            let names = if files.is_empty() {
                let mut v: Vec<String> = std::fs::read_dir(&dir)?
                    .filter_map(|e| e.ok())
                    .filter(|e| e.file_type().map(|t| t.is_file()).unwrap_or(false))
                    .map(|e| e.file_name().to_string_lossy().into_owned())
                    .filter(|n| n != gm_launcher::release::SUMS && n != gm_launcher::release::SIG)
                    .collect();
                v.sort();
                v
            } else {
                files
            };
            if names.is_empty() {
                bail!("nothing to sign in {}", dir.display());
            }
            let sums = gm_launcher::release::write_sums(&dir, &names)?;
            let ts = std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .map(|d| d.as_secs())
                .unwrap_or(0);
            let sig = sk.sign(
                sums.as_bytes(),
                &format!("timestamp:{ts}\tfile:{}", gm_launcher::release::SUMS),
                "signature from gm-launcher",
            );
            std::fs::write(dir.join(gm_launcher::release::SIG), sig)?;
            println!("signed {} files in {}", names.len(), dir.display());
        }
        Cmd::Verify { dir, public } => {
            let text = match public {
                Some(p) => std::fs::read_to_string(&p).with_context(|| p.display().to_string())?,
                None => gm_launcher::release::RELEASE_PUBLIC_KEY.to_string(),
            };
            let pk = gm_launcher::minisign::PublicKey::parse(&text)?;
            let files = gm_launcher::release::verify_dir(&dir, &pk)?;
            println!("ok: {} files verified in {}", files.len(), dir.display());
        }
        Cmd::Play { connect, map, gmod } => {
            let game_dir = match gmod {
                Some(d) => gm_launcher::Install::at(&d)?.game_dir,
                None => {
                    gm_launcher::find_install()
                        .context("no complete Garry's Mod install; pass --gmod or set GMOD_DIR")?
                        .game_dir
                }
            };
            let exe = std::env::current_exe()?;
            let client = exe
                .parent()
                .context("launcher path")?
                .join(format!("gm-client{}", std::env::consts::EXE_SUFFIX));
            if !client.exists() {
                bail!("{} is not next to the launcher", client.display());
            }
            let mut cmd = std::process::Command::new(&client);
            cmd.env("GMOD_DIR", &game_dir);
            match connect {
                Some(addr) => {
                    cmd.arg(&map).arg("--connect").arg(addr);
                }
                None => {
                    cmd.arg(&map);
                }
            }
            let status = cmd.status().with_context(|| client.display().to_string())?;
            if !status.success() {
                bail!("client exited with {status}");
            }
        }
    }
    Ok(())
}
