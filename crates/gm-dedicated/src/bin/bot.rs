//! `gm-bot --server 127.0.0.1:27015`: connects, walks forward for a while, spawns a crate, and
//! prints what it sees; exits after `--ticks`.

use anyhow::{Context, Result};
use clap::Parser;
use std::time::{Duration, Instant};

#[derive(Parser)]
struct Args {
    #[arg(long, default_value = "127.0.0.1:27015")]
    server: String,
    #[arg(long, default_value = "bot")]
    name: String,
    #[arg(long, default_value_t = 400)]
    ticks: u32,
    /// Client-realm addons (directory or .gmr) to run against the assembled world.
    #[arg(long)]
    addon: Vec<std::path::PathBuf>,
    /// Where downloaded server packages are kept (`<sha256>.gmr`); the client-realm actors of
    /// every package the server publishes run from here.
    #[arg(long, default_value = "cache")]
    cache: std::path::PathBuf,
    /// Model to spawn at tick 100.
    #[arg(long, default_value = "models/props_junk/wood_crate001a.mdl")]
    spawn: String,
}

fn main() -> Result<()> {
    let args = Args::parse();
    let mut udp = gm_net::Udp::client(args.server.parse().context("server address")?)?;
    let mut client = gm_net::ClientNet::new(1, &args.name);
    let mut script = gm_script::Runtime::client();
    for a in &args.addon {
        let m = script
            .load_addon(a)
            .unwrap_or_else(|e| panic!("addon {}: {e}", a.display()));
        println!(
            "client addon {} {} ({} actors)",
            m.id,
            m.version,
            script.actors().len()
        );
    }
    let mut last_snapshot_tick = 0u32;
    let mut addons_seen: std::collections::BTreeSet<[u8; 32]> = Default::default();
    let mut run_scripts = !args.addon.is_empty();
    let interval = Duration::from_secs_f32(gm_core::tick::DEFAULT_TICK_INTERVAL);
    let mut next = Instant::now();
    let mut welcomed_at = None;
    let mut first_snapshot_at = None;
    let mut spawned = false;
    for t in 0..args.ticks {
        // walk forward along +y for ticks 20..120 looking down 20 degrees, then stand
        let walking = (20..120).contains(&t);
        client.tick(
            gm_net::NetCmd {
                buttons: if walking { 8 } else { 0 },
                view_angles: glam::Vec3::new(20.0, 90.0, 0.0),
                forward_move: if walking { 200.0 } else { 0.0 },
                weapon: 2,
                ..Default::default()
            },
            &mut udp,
        );
        if t == 100 && !spawned {
            client.command(gm_net::Command::Spawn {
                model: args.spawn.clone(),
            });
            spawned = true;
        }
        client.receive(&mut udp);
        for a in client.server_addons.clone() {
            if !addons_seen.insert(a.sha) {
                continue;
            }
            let path = args
                .cache
                .join(format!("{}.gmr", gm_net::msg::sha_hex(&a.sha)));
            let cached = std::fs::read(&path)
                .ok()
                .and_then(|b| gm_script::Package::read(&b).ok())
                .is_some_and(|p| p.sha256 == a.sha);
            if cached {
                load_package(&mut script, &path, "cached", t);
                run_scripts = true;
            } else {
                println!(
                    "downloading addon {} {} ({} bytes)",
                    a.id, a.version, a.size
                );
                client.want_addon(a.sha);
            }
        }
        for (sha, bytes) in std::mem::take(&mut client.downloaded) {
            let ok = gm_script::Package::read(&bytes).is_ok_and(|p| p.sha256 == sha);
            if !ok {
                println!(
                    "download {} failed verification",
                    gm_net::msg::sha_hex(&sha)
                );
                continue;
            }
            std::fs::create_dir_all(&args.cache)?;
            let path = args
                .cache
                .join(format!("{}.gmr", gm_net::msg::sha_hex(&sha)));
            std::fs::write(&path, &bytes)?;
            load_package(&mut script, &path, "downloaded", t);
            run_scripts = true;
        }
        if run_scripts {
            let fresh = client.server_tick != last_snapshot_tick;
            last_snapshot_tick = client.server_tick;
            script.client_tick(&client.world, fresh);
            let inbox = std::mem::take(&mut client.addon_msgs);
            script.client_net(&client.world, inbox);
            for (_, name, data) in script.drain_net() {
                client.send_addon(&name, &data);
            }
            for line in script.log.drain(..) {
                println!("{line}");
            }
        }
        if let (None, Some(pid)) = (welcomed_at, client.player_id) {
            welcomed_at = Some(t);
            println!(
                "welcome at tick {t}: player {pid} map {} server tick {}",
                client.map, client.server_tick
            );
        }
        if first_snapshot_at.is_none() && client.world.tick > 0 {
            first_snapshot_at = Some(t);
        }
        if t % 100 == 99 {
            let me = client
                .world
                .players
                .iter()
                .find(|p| Some(p.id) == client.player_id);
            println!(
                "tick {t}: snapshot {} me {:?} entities {} count {} undos {} acked cmds {} rx {} B tx {} B",
                client.world.tick,
                me.map(|p| p.origin),
                client.world.entities.len(),
                client.world.own.count,
                client.world.own.undos.len(),
                client.acked_commands,
                client.stats().bytes_received,
                client.stats().bytes_sent
            );
            for e in client.world.entities.values() {
                println!(
                    "  entity {} {} at {:?} frozen {}",
                    e.id, e.model, e.pos, e.frozen
                );
            }
        }
        for ev in client.events.drain(..) {
            println!("event: {ev}");
        }
        next += interval;
        let now = Instant::now();
        if next > now {
            std::thread::sleep(next - now);
        }
    }
    println!(
        "done: welcomed {:?}, first snapshot {:?}, {} entities, {} packets received",
        welcomed_at,
        first_snapshot_at,
        client.world.entities.len(),
        client.stats().packets_received
    );
    Ok(())
}

/// Loads a verified package into the client runtime; a package of an addon already running is
/// a hot reload (actors keep their `state`).
fn load_package(script: &mut gm_script::Runtime, path: &std::path::Path, how: &str, t: u32) {
    let known = gm_script::Package::open(path)
        .ok()
        .and_then(|p| p.load_realm("client").ok())
        .is_some_and(|(m, _)| script.addon_paths.contains_key(&m.id));
    let r = if known {
        script.reload_addon(path)
    } else {
        script.load_addon(path)
    };
    match r {
        Ok(m) => println!(
            "{how} addon {} {} {} at tick {t}",
            m.id,
            m.version,
            if known { "reloaded" } else { "loaded" }
        ),
        Err(e) => println!("{how} addon {}: {e}", path.display()),
    }
}
