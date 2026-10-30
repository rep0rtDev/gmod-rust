//! `gm-dedicated --map gm_construct --port 27015`: loads the map from the local game copy, ticks
//! at 66.67 Hz and serves UDP clients with the `gm-net` protocol.

use anyhow::{Context, Result};
use clap::Parser;
use std::time::{Duration, Instant};

#[derive(Parser)]
struct Args {
    #[arg(long, default_value = "gm_construct")]
    map: String,
    #[arg(long, default_value = "0.0.0.0:27015")]
    bind: String,
    /// Where players appear (x y z).
    #[arg(long, num_args = 3, default_values_t = [823.235, -500.0, -143.0])]
    spawn: Vec<f32>,
    /// Stop after this many ticks (0 = run forever).
    #[arg(long, default_value_t = 0)]
    ticks: u64,
    /// Print a status line every N ticks (0 = never).
    #[arg(long, default_value_t = 0)]
    status_every: u64,
    /// Luau actor sources to load (repeatable); the file name is the actor name.
    #[arg(long)]
    addon: Vec<std::path::PathBuf>,
    /// Where addon storage files live (`<data>/server/<addon>.gms`).
    #[arg(long, default_value = "data")]
    data: std::path::PathBuf,
    /// Hot reload addons whose files change (checked every 33 ticks).
    #[arg(long)]
    watch: bool,
    /// Public key (hex) addons must be signed by; repeatable, none means any addon loads
    #[arg(long)]
    trust: Vec<String>,
    /// Only clients that prove an identity (ed25519 key, `gm-bot --key`) enter the server.
    #[arg(long)]
    require_auth: bool,
    /// Addon registry (directory or `http://` base url): `--addon id@requirement` (e.g.
    /// `wire@^1.2`, or a bare id) is resolved against its index with dependencies and installed
    /// into `<data>/registry`.
    #[arg(long)]
    registry: Option<String>,
    /// Master server to heartbeat to (every 10 s) so browsers list this server.
    #[arg(long)]
    master: Option<String>,
    /// Server name shown in the browser.
    #[arg(long, default_value = "gmod-rust server")]
    name: String,
    #[arg(long, default_value_t = 16)]
    max_players: u16,
}

fn main() -> Result<()> {
    let args = Args::parse();
    let inst = gm_content::find_install().context("no Garry's Mod install found (set GMOD_DIR)")?;
    let mount = gm_content::Mount::for_install(&inst)?;
    let t0 = Instant::now();
    let world = gm_dedicated::load_world(&mount, &args.map)?;
    let mut server = gm_server::Server::new(&world.collision, world.physics, world.world_body);
    let mount_for_models = gm_content::Mount::for_install(&inst)?;
    server.model_loader = Some(Box::new(move |model: &str| {
        gm_dedicated::model_info(&mount_for_models, model)
            .map_err(|e| eprintln!("model {model}: {e}"))
            .ok()
    }));
    let spawn = glam::Vec3::new(args.spawn[0], args.spawn[1], args.spawn[2]);
    let mut net = gm_net::ServerNet::new(&args.map, spawn);
    net.require_auth = args.require_auth;
    if !args.addon.is_empty() {
        let mut rt = gm_script::Runtime::new();
        rt.data_dir = Some(args.data.clone());
        rt.watch = args.watch;
        for t in &args.trust {
            let k: [u8; 32] = gm_script::package::unhex(t)
                .and_then(|v| v.try_into().ok())
                .with_context(|| format!("--trust {t}: a public key is 32 hex bytes"))?;
            rt.trusted.push(k);
        }
        let is_package =
            |p: &std::path::Path| p.is_dir() || p.extension().is_some_and(|e| e == "gmr");
        let mut packages: Vec<std::path::PathBuf> = args
            .addon
            .iter()
            .filter(|p| is_package(p))
            .cloned()
            .collect();
        let wanted: Vec<String> = args
            .addon
            .iter()
            .filter(|p| !p.exists())
            .map(|p| p.to_string_lossy().replace('@', " "))
            .collect();
        if !wanted.is_empty() {
            let reg_dir = args
                .registry
                .as_ref()
                .with_context(|| format!("--registry is needed to install {wanted:?}"))?;
            let reg = gm_script::Registry::open_any(reg_dir)?;
            let cache = args.data.join("registry");
            for e in reg.resolve(&wanted)? {
                let path = reg.install(&e, &cache)?;
                println!("registry: {} {} -> {}", e.id, e.version, path.display());
                packages.push(path);
            }
        }
        for m in rt
            .load_addons(&packages)
            .map_err(|e| anyhow::anyhow!("addons: {e}"))?
        {
            println!(
                "addon {} {} (priority {}, depends {:?})",
                m.id, m.version, m.priority, m.depends
            );
        }
        for (i, path) in args.addon.iter().enumerate() {
            if is_package(path) || !path.exists() {
                continue;
            }
            let source =
                std::fs::read_to_string(path).with_context(|| path.display().to_string())?;
            let name = path
                .file_stem()
                .map(|s| s.to_string_lossy().to_string())
                .unwrap_or_default();
            rt.load(gm_script::ActorSpec {
                addon: name.clone(),
                name,
                priority: i as i32,
                source,
                config: Default::default(),
            })
            .with_context(|| format!("addon {}", path.display()))?;
        }
        println!("gm-dedicated: {} actors loaded", rt.actors().len());
        net.hooks = Some(Box::new(rt));
    }
    let bind: std::net::SocketAddr = args.bind.parse().context("bind address")?;
    let mut udp = gm_net::Udp::server(bind)?;
    net.info.name = args.name.clone();
    net.info.max_players = args.max_players;
    net.info.port = bind.port();
    let master = match &args.master {
        Some(m) => Some(udp.endpoint_for(m.parse().context("master address")?)),
        None => None,
    };
    println!(
        "gm-dedicated: {} loaded in {:.2} s, listening on {}, tick {:.3} s",
        args.map,
        t0.elapsed().as_secs_f32(),
        udp.local_addr()?,
        gm_core::tick::DEFAULT_TICK_INTERVAL
    );
    let interval = Duration::from_secs_f32(gm_core::tick::DEFAULT_TICK_INTERVAL);
    let mut next = Instant::now();
    let mut n = 0u64;
    let mut busy = Duration::ZERO;
    let heartbeat_every =
        (gm_net::master::HEARTBEAT_S as f32 / gm_core::tick::DEFAULT_TICK_INTERVAL) as u64;
    loop {
        let start = Instant::now();
        net.receive(&mut server, &mut udp);
        net.tick(&mut server);
        net.send(&server, &mut udp);
        if let Some(m) = master {
            if n.is_multiple_of(heartbeat_every) {
                net.heartbeat(&mut udp, m);
            }
        }
        busy += start.elapsed();
        n += 1;
        if args.status_every > 0 && n.is_multiple_of(args.status_every) {
            println!(
                "tick {n}: {} clients, {} props, {} constraints, tick time avg {:.3} ms",
                net.clients.len(),
                server.sandbox.props().len(),
                server.sandbox.constraints().len(),
                busy.as_secs_f64() * 1000.0 / args.status_every as f64
            );
            busy = Duration::ZERO;
        }
        if args.ticks > 0 && n >= args.ticks {
            println!(
                "done: {n} ticks, {} clients, {} props",
                net.clients.len(),
                server.sandbox.props().len()
            );
            return Ok(());
        }
        next += interval;
        let now = Instant::now();
        if next > now {
            std::thread::sleep(next - now);
        } else if now - next > Duration::from_millis(200) {
            next = now;
        }
    }
}
