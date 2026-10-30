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
    /// Start an addon directory from a template: `addon` (server + client actors) or `tool` (a
    /// scripted stool); the directory name is the addon id
    New {
        #[arg(value_parser = ["addon", "tool"])]
        kind: String,
        dir: PathBuf,
    },
    /// Publish a signed addon (directory or .gmr) into a registry directory (index.toml + blobs/)
    Publish { addon: PathBuf, registry: PathBuf },
    /// Resolve `id@requirement`s against a registry and print what would be installed, in order
    Resolve {
        /// Registry directory or `http://` base url
        registry: String,
        #[arg(required = true)]
        wanted: Vec<String>,
    },
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
        Cmd::New { kind, dir } => {
            let id = dir
                .file_name()
                .and_then(|n| n.to_str())
                .filter(|n| {
                    !n.is_empty()
                        && n.chars()
                            .all(|c| c.is_ascii_alphanumeric() || c == '_' || c == '-')
                })
                .context("the directory name is the addon id: letters, digits, _ or -")?
                .to_string();
            anyhow::ensure!(!dir.exists(), "{} exists", dir.display());
            std::fs::create_dir_all(dir.join("src"))?;
            let (manifest, files): (String, Vec<(&str, String)>) = match kind.as_str() {
                "tool" => (
                    format!(
                        "id = {id:?}\nversion = \"0.1.0\"\npriority = 0\nrealms = [\"server\"]\n\
                         capabilities = [\"world-read\", \"world-write\"]\ndepends = []\n\n\
                         [[actors]]\nname = \"tools\"\nsource = \"src/tools.luau\"\n"
                    ),
                    vec![("src/tools.luau", TOOL_TEMPLATE.replace("{id}", &id))],
                ),
                _ => (
                    format!(
                        "id = {id:?}\nversion = \"0.1.0\"\npriority = 0\n\
                         realms = [\"server\", \"client\"]\n\
                         capabilities = [\"world-read\", \"world-write\", \"storage\"]\ndepends = []\n\n\
                         [[actors]]\nname = \"main\"\nsource = \"src/main.luau\"\n\n\
                         [[actors]]\nname = \"hud\"\nsource = \"src/hud.luau\"\nrealm = \"client\"\n"
                    ),
                    vec![
                        ("src/main.luau", SERVER_TEMPLATE.replace("{id}", &id)),
                        ("src/hud.luau", CLIENT_TEMPLATE.replace("{id}", &id)),
                    ],
                ),
            };
            std::fs::write(dir.join("manifest.toml"), manifest)?;
            for (name, text) in &files {
                std::fs::write(dir.join(name), text)?;
            }
            std::fs::write(
                dir.join("README.md"),
                format!(
                    "# {id}\n\nRun against a local server with hot reload:\n\n    gm-dedicated --addon {} --watch\n\n\
                     Pack, sign and publish:\n\n    gmr keygen me.key\n    gmr sign {} me.key\n    gmr publish {} <registry>\n",
                    dir.display(),
                    dir.display(),
                    dir.display()
                ),
            )?;
            gm_script::Manifest::load(&dir)?;
            println!(
                "{} {}: manifest.toml, {}, README.md",
                kind,
                dir.display(),
                files.iter().map(|(n, _)| *n).collect::<Vec<_>>().join(", ")
            );
        }
        Cmd::Publish { addon, registry } => {
            let bytes = if addon.is_dir() {
                gm_script::package::pack(&addon)?
            } else {
                std::fs::read(&addon)?
            };
            let mut reg = gm_script::Registry::open(&registry)?;
            let e = reg.publish(&bytes)?;
            println!(
                "published {} {} ({} bytes) sha256 {} signers {:?} into {}",
                e.id,
                e.version,
                e.size,
                e.sha,
                e.signers,
                registry.display()
            );
        }
        Cmd::Resolve { registry, wanted } => {
            let reg = gm_script::Registry::open_any(&registry)?;
            let wanted: Vec<String> = wanted.iter().map(|w| w.replace('@', " ")).collect();
            for e in reg.resolve(&wanted)? {
                println!(
                    "{} {} sha256 {} depends {:?}",
                    e.id, e.version, e.sha, e.depends
                );
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

const SERVER_TEMPLATE: &str = r#"--!strict
-- {id}: server actor. Runs in parallel with the other actors against an immutable snapshot of
-- the world; writes go through world.queue and are applied in a deterministic order after the
-- tick. `state` survives hot reloads.
state.spawns = state.spawns or 0

events.on("entity.spawned", function(ev)
    state.spawns += 1
    print(`{id}: prop {ev.ent} spawned by player {ev.player} ({state.spawns} so far)`)
end)

events.on("net.message", function(ev)
    if ev.name == "{id}.hello" then
        net.send("{id}.hello", { tick = game.tick(), spawns = state.spawns }, ev.player)
    end
end)
"#;

const CLIENT_TEMPLATE: &str = r#"--!strict
-- {id}: client actor. Sees the client's assembled world (snapshot.received on every server tick,
-- tick.client every frame); writes are not available in this realm.
events.on("snapshot.received", function()
    state.snaps = (state.snaps or 0) + 1
    if state.snaps == 1 then
        net.send("{id}.hello", {})
    end
end)

events.on("net.message", function(ev)
    if ev.name == "{id}.hello" then
        print(`{id}: server tick {ev.data.tick}, {ev.data.spawns} props spawned`)
    end
end)
"#;

const TOOL_TEMPLATE: &str = r##"--!strict
-- {id}: a scripted stool. Left click on two props welds them (first click picks the first prop),
-- right click clears the selection. Return a list of ops; the server applies them with the
-- player's limits, undo entry and cleanup list.
tool.register("{id}", {
    left = function(ctx)
        if ctx.hit.ent == 0 then
            return nil
        end
        if #ctx.objects == 0 then
            return { { op = "set_object" } }
        end
        return {
            { op = "weld", ent1 = ctx.objects[1].ent, ent2 = ctx.hit.ent, forcelimit = 0, nocollide = false, undo = "#tool.{id}.name" },
            { op = "clear" },
        }
    end,
    right = function(ctx)
        return { { op = "clear" } }
    end,
})
"##;
