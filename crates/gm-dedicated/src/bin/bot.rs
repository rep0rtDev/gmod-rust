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
    /// Addon registry directory: a package the server announces is taken from here by sha256
    /// before asking the server for the bytes.
    #[arg(long)]
    registry: Option<std::path::PathBuf>,
    /// Browse instead of playing: ask this master for its list, query every server and print
    /// name, map, players, addons and ping.
    #[arg(long)]
    list: Option<String>,
    /// Model to spawn at tick 100.
    #[arg(long, default_value = "models/props_junk/wood_crate001a.mdl")]
    spawn: String,
}

fn main() -> Result<()> {
    let args = Args::parse();
    if let Some(master) = &args.list {
        return browse(master);
    }
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
            let from_registry = !cached
                && args.registry.as_ref().is_some_and(|r| {
                    gm_script::Registry::open(r).is_ok_and(|reg| {
                        reg.fetch_by_sha(&gm_net::msg::sha_hex(&a.sha), &args.cache)
                            .is_some()
                    })
                });
            if cached || from_registry {
                load_package(
                    &mut script,
                    &path,
                    if cached { "cached" } else { "registry" },
                    t,
                );
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

fn browse(master: &str) -> Result<()> {
    use gm_net::{MasterMsg, Transport};
    let mut udp = gm_net::Udp::client(master.parse().context("master address")?)?;
    let mut entries = Vec::new();
    let mut offset = 0u32;
    loop {
        udp.send(0, MasterMsg::List { offset }.encode());
        let t0 = Instant::now();
        let mut page = None;
        while t0.elapsed() < Duration::from_secs(2) && page.is_none() {
            for (from, p) in udp.recv() {
                if from == 0 {
                    if let Ok(MasterMsg::Servers { total, entries, .. }) = MasterMsg::read(&p) {
                        page = Some((total, entries));
                    }
                }
            }
            std::thread::sleep(Duration::from_millis(10));
        }
        let Some((total, got)) = page else {
            anyhow::bail!("no answer from the master {master}");
        };
        let n = got.len() as u32;
        entries.extend(got);
        offset += n;
        if n == 0 || offset >= total {
            break;
        }
    }
    println!("{} servers", entries.len());
    for e in &entries {
        let Ok(addr) = e.addr.parse::<std::net::SocketAddr>() else {
            println!("{}  (unparsable address)", e.addr);
            continue;
        };
        let ep = udp.endpoint_for(addr);
        let nonce = 0x5EED ^ e.info.port as u32;
        let t0 = Instant::now();
        udp.send(ep, MasterMsg::Query { nonce }.encode());
        let mut info = None;
        while t0.elapsed() < Duration::from_secs(1) && info.is_none() {
            for (from, p) in udp.recv() {
                if from == ep {
                    if let Ok(MasterMsg::Info { nonce: n, info: i }) = MasterMsg::read(&p) {
                        if n == nonce {
                            info = Some((i, t0.elapsed()));
                        }
                    }
                }
            }
            std::thread::sleep(Duration::from_millis(5));
        }
        match info {
            Some((i, ping)) => println!(
                "{}  \"{}\"  {}  {}/{}  addons {:?}  ping {:.0} ms  (master: seen {} s ago)",
                e.addr,
                i.name,
                i.map,
                i.players,
                i.max_players,
                i.addons,
                ping.as_secs_f32() * 1000.0,
                e.age_s
            ),
            None => println!(
                "{}  \"{}\"  {}  {}/{}  no answer (master: seen {} s ago)",
                e.addr, e.info.name, e.info.map, e.info.players, e.info.max_players, e.age_s
            ),
        }
    }
    Ok(())
}
