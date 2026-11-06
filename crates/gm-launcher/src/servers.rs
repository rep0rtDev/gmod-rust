//! The server browser's data: the master's list, then each server asked directly.

use anyhow::{Context, Result};
use gm_net::{MasterMsg, Transport};
use std::time::{Duration, Instant};

pub struct Server {
    pub addr: String,
    pub info: gm_net::master::ServerInfo,
    /// Round trip of the direct query; `None` when the server did not answer (the info is
    /// then the master's copy).
    pub ping: Option<Duration>,
    pub seen_s: u16,
}

/// Every server the master lists, with its direct answer.
pub fn browse(master: &str) -> Result<Vec<Server>> {
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
    Ok(entries
        .into_iter()
        .map(|e| {
            let (info, ping) = match e.addr.parse::<std::net::SocketAddr>() {
                Ok(addr) => query(&mut udp, addr, e.info.port),
                Err(_) => (None, None),
            };
            Server {
                addr: e.addr,
                info: info.unwrap_or(e.info),
                ping,
                seen_s: e.age_s,
            }
        })
        .collect())
}

/// One server asked directly (`Query` / `Info`), a second to answer.
pub fn query(
    udp: &mut gm_net::Udp,
    addr: std::net::SocketAddr,
    port: u16,
) -> (Option<gm_net::master::ServerInfo>, Option<Duration>) {
    let ep = udp.endpoint_for(addr);
    let nonce = 0x5EED ^ port as u32;
    let t0 = Instant::now();
    udp.send(ep, MasterMsg::Query { nonce }.encode());
    while t0.elapsed() < Duration::from_secs(1) {
        for (from, p) in udp.recv() {
            if from == ep {
                if let Ok(MasterMsg::Info { nonce: n, info }) = MasterMsg::read(&p) {
                    if n == nonce {
                        return (Some(info), Some(t0.elapsed()));
                    }
                }
            }
        }
        std::thread::sleep(Duration::from_millis(5));
    }
    (None, None)
}
