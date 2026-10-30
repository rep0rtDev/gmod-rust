//! `gm-master --bind 0.0.0.0:27010`: the server list. Game servers heartbeat every 10 s
//! (`gm-dedicated --master`), browsers ask for the list (`gm-bot --list`); a server silent for
//! 90 s is dropped.
use anyhow::{Context, Result};
use clap::Parser;
use std::time::{Duration, Instant};

#[derive(Parser)]
struct Args {
    #[arg(long, default_value = "0.0.0.0:27010")]
    bind: String,
    /// Stop after this many seconds (0 = run forever).
    #[arg(long, default_value_t = 0)]
    seconds: u64,
}

fn main() -> Result<()> {
    let args = Args::parse();
    let mut udp = gm_net::Udp::server(args.bind.parse().context("bind address")?)?;
    println!("gm-master: listening on {}", udp.local_addr()?);
    let mut master = gm_net::MasterNet::default();
    let t0 = Instant::now();
    let mut known = std::collections::BTreeSet::new();
    loop {
        let now = t0.elapsed().as_secs();
        master.receive(&mut udp, now);
        for (k, (info, _)) in &master.servers {
            if known.insert(k.clone()) {
                println!(
                    "+ {k} \"{}\" {} {}/{}",
                    info.name, info.map, info.players, info.max_players
                );
            }
        }
        for k in master.expire(now) {
            known.remove(&k);
            println!("- {k} (silent)");
        }
        if args.seconds > 0 && now >= args.seconds {
            return Ok(());
        }
        std::thread::sleep(Duration::from_millis(50));
    }
}
