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
    if !args.addon.is_empty() {
        let mut rt = gm_script::Runtime::new();
        for (i, path) in args.addon.iter().enumerate() {
            if path.is_dir() {
                let m = rt
                    .load_addon(path)
                    .map_err(|e| anyhow::anyhow!("addon {}: {e}", path.display()))?;
                println!("addon {} {} (priority {})", m.id, m.version, m.priority);
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
    let mut udp = gm_net::Udp::server(args.bind.parse().context("bind address")?)?;
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
    loop {
        let start = Instant::now();
        net.receive(&mut server, &mut udp);
        net.tick(&mut server);
        net.send(&server, &mut udp);
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
