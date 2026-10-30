//! `gm-dedicated --map gm_construct --port 27015`: loads the map from the local game copy, ticks
//! at 66.67 Hz and serves UDP clients with the `gm-net` protocol.

use anyhow::{Context, Result};
use clap::Parser;
use std::time::{Duration, Instant};

#[derive(Parser)]
struct Args {
    /// Server file (TOML, the same fields as these flags plus `[cvars]`); explicit flags win.
    #[arg(long)]
    config: Option<std::path::PathBuf>,
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
    /// Identity (public key hex) that gets admin commands (`Kick`, `Reload`) once authenticated.
    #[arg(long)]
    admin: Vec<String>,
    /// Banned identity (public key hex); `<data>/bans.txt` (one per line) is loaded and kept too.
    #[arg(long)]
    ban: Vec<String>,
    /// Load `<data>/saves/<name>.rdup` into the world at start (owner: player 0, the server).
    #[arg(long)]
    load: Option<String>,
    /// Seconds without a packet before a client is dropped.
    #[arg(long, default_value_t = 10.0)]
    timeout: f32,
    /// Addon registry (directory or `http://` base url): `--addon id@requirement` (e.g.
    /// `wire@^1.2`, or a bare id) is resolved against its index with dependencies and installed
    /// into `<data>/registry`.
    #[arg(long)]
    registry: Option<String>,
    /// Master server to heartbeat to (every 10 s) so browsers list this server.
    #[arg(long)]
    master: Option<String>,
    /// File with the server's identity (ed25519 secret, hex; created when missing): every
    /// `Challenge` is signed with it, clients can pin the public key (`gm-bot --server-key`).
    #[arg(long)]
    server_key: Option<std::path::PathBuf>,
    /// Server name shown in the browser.
    #[arg(long, default_value = "gmod-rust server")]
    name: String,
    #[arg(long, default_value_t = 16)]
    max_players: u16,
}

fn main() -> Result<()> {
    let matches = <Args as clap::CommandFactory>::command().get_matches();
    let mut args = <Args as clap::FromArgMatches>::from_arg_matches(&matches)?;
    let config = match &args.config {
        Some(p) => gm_dedicated::ServerConfig::load(p)?,
        None => gm_dedicated::ServerConfig::default(),
    };
    // the file fills in what the command line left at its default
    let defaulted =
        |id: &str| matches.value_source(id) != Some(clap::parser::ValueSource::CommandLine);
    macro_rules! take {
        ($field:ident) => {
            if defaulted(stringify!($field)) {
                if let Some(v) = config.$field.clone() {
                    args.$field = v.into();
                }
            }
        };
    }
    take!(map);
    take!(bind);
    take!(name);
    take!(max_players);
    take!(data);
    take!(watch);
    take!(require_auth);
    take!(timeout);
    if defaulted("spawn") {
        if let Some(sp) = config.spawn {
            args.spawn = sp.to_vec();
        }
    }
    if args.registry.is_none() {
        args.registry = config.registry.clone();
    }
    if args.master.is_none() {
        args.master = config.master.clone();
    }
    if args.server_key.is_none() {
        args.server_key = config.server_key.clone();
    }
    args.addon.extend(config.addon.iter().cloned());
    args.trust.extend(config.trust.iter().cloned());
    args.admin.extend(config.admin.iter().cloned());
    args.ban.extend(config.ban.iter().cloned());
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
    for (name, value) in config.cvar_strings() {
        if !server.set_cvar(&name, &value) {
            anyhow::bail!("[cvars] {name} = {value}: unknown cvar or bad value");
        }
    }
    let mut net = gm_net::ServerNet::new(&args.map, spawn);
    net.saves_dir = Some(args.data.join("saves"));
    if let Some(name) = &args.load {
        let path = args
            .data
            .join("saves")
            .join(format!("{name}.{}", gm_server::save::EXTENSION));
        let bytes = std::fs::read(&path).with_context(|| path.display().to_string())?;
        let save: gm_server::save::Save =
            serde_json::from_slice(&bytes).with_context(|| path.display().to_string())?;
        let (p, c) = server.load_world(&save, 0);
        println!(
            "loaded {name}: {p} props, {c} constraints (saved on {})",
            save.map
        );
    }
    net.require_auth = args.require_auth;
    if let Some(path) = &args.server_key {
        let secret = match std::fs::read_to_string(path) {
            Ok(hex) => parse_key(hex.trim()).with_context(|| format!("{}", path.display()))?,
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => {
                let (secret, _) = gm_script::package::keygen();
                std::fs::write(path, hex_of(&secret))
                    .with_context(|| format!("{}", path.display()))?;
                secret
            }
            Err(e) => return Err(e).with_context(|| format!("{}", path.display())),
        };
        let public = net.with_key(&secret);
        println!("server identity {}", hex_of(&public));
    }
    net.max_players = args.max_players as usize;
    net.timeout_ticks = (args.timeout / gm_core::tick::DEFAULT_TICK_INTERVAL).round() as u32;
    for a in &args.admin {
        net.admin_keys
            .insert(parse_key(a).with_context(|| format!("--admin {a}"))?);
    }
    let bans_path = args.data.join("bans.txt");
    for b in &args.ban {
        net.bans
            .insert(parse_key(b).with_context(|| format!("--ban {b}"))?);
    }
    if let Ok(text) = std::fs::read_to_string(&bans_path) {
        for line in text.lines().map(str::trim).filter(|l| !l.is_empty()) {
            net.bans
                .insert(parse_key(line).with_context(|| format!("{}", bans_path.display()))?);
        }
    }
    let mut bans_known = net.bans.len();
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
        for line in net.take_log() {
            println!("{line}");
        }
        if net.bans.len() != bans_known {
            bans_known = net.bans.len();
            let text: String = net.bans.iter().map(|b| hex_of(b) + "\n").collect();
            if let Err(e) =
                std::fs::create_dir_all(&args.data).and_then(|_| std::fs::write(&bans_path, text))
            {
                eprintln!("bans.txt: {e}");
            }
        }
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

fn parse_key(hex: &str) -> Result<[u8; 32]> {
    anyhow::ensure!(hex.len() == 64, "a public key is 32 hex bytes");
    let mut out = [0u8; 32];
    for (i, o) in out.iter_mut().enumerate() {
        *o = u8::from_str_radix(&hex[2 * i..2 * i + 2], 16)?;
    }
    Ok(out)
}

fn hex_of(b: &[u8; 32]) -> String {
    b.iter().map(|x| format!("{x:02x}")).collect()
}
