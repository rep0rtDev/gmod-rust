//! Server browser plumbing: game servers heartbeat to a master, which answers list requests;
//! a game server answers a direct `Query` with its `Info` (the browser measures the ping). Master
//! packets start with their own magic so they never parse as channel packets.

use crate::transport::Transport;
use crate::wire::{Reader, Truncated, Writer};
use std::collections::BTreeMap;

pub const MASTER_MAGIC: u16 = 0x4D47; // "MG"
/// A `Servers` reply stays under this many bytes (one packet).
pub const LIST_BUDGET: usize = 1100;
/// A server not heard from for this long leaves the list.
pub const TTL_S: u64 = 90;
/// How often a game server heartbeats.
pub const HEARTBEAT_S: u64 = 10;

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct ServerInfo {
    pub name: String,
    pub map: String,
    pub players: u16,
    pub max_players: u16,
    /// The game port (the heartbeat may come from another socket).
    pub port: u16,
    pub version: String,
    /// Ids of the addons clients get.
    pub addons: Vec<String>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ServerEntry {
    /// `host:port` as the master saw the server.
    pub addr: String,
    pub info: ServerInfo,
    pub age_s: u16,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum MasterMsg {
    Heartbeat(ServerInfo),
    List {
        offset: u32,
    },
    Servers {
        total: u32,
        offset: u32,
        entries: Vec<ServerEntry>,
    },
    Query {
        nonce: u32,
    },
    Info {
        nonce: u32,
        info: ServerInfo,
    },
}

pub fn is_master_packet(packet: &[u8]) -> bool {
    let mut r = Reader::new(packet);
    r.u16().is_ok_and(|m| m == MASTER_MAGIC)
}

fn write_info(w: &mut Writer, i: &ServerInfo) {
    w.str(&i.name);
    w.str(&i.map);
    w.u16(i.players);
    w.u16(i.max_players);
    w.u16(i.port);
    w.str(&i.version);
    w.varint(i.addons.len() as u64);
    for a in &i.addons {
        w.str(a);
    }
}

fn read_info(r: &mut Reader) -> Result<ServerInfo, Truncated> {
    let name = r.str()?;
    let map = r.str()?;
    let players = r.u16()?;
    let max_players = r.u16()?;
    let port = r.u16()?;
    let version = r.str()?;
    let n = r.varint()?.min(256);
    let mut addons = Vec::new();
    for _ in 0..n {
        addons.push(r.str()?);
    }
    Ok(ServerInfo {
        name,
        map,
        players,
        max_players,
        port,
        version,
        addons,
    })
}

impl MasterMsg {
    pub fn write(&self, w: &mut Writer) {
        w.u16(MASTER_MAGIC);
        match self {
            MasterMsg::Heartbeat(i) => {
                w.u8(1);
                write_info(w, i);
            }
            MasterMsg::List { offset } => {
                w.u8(2);
                w.u32(*offset);
            }
            MasterMsg::Servers {
                total,
                offset,
                entries,
            } => {
                w.u8(3);
                w.u32(*total);
                w.u32(*offset);
                w.varint(entries.len() as u64);
                for e in entries {
                    w.str(&e.addr);
                    write_info(w, &e.info);
                    w.u16(e.age_s);
                }
            }
            MasterMsg::Query { nonce } => {
                w.u8(4);
                w.u32(*nonce);
            }
            MasterMsg::Info { nonce, info } => {
                w.u8(5);
                w.u32(*nonce);
                write_info(w, info);
            }
        }
    }

    pub fn encode(&self) -> Vec<u8> {
        let mut w = Writer::default();
        self.write(&mut w);
        w.buf
    }

    pub fn read(packet: &[u8]) -> Result<MasterMsg, Truncated> {
        let mut r = Reader::new(packet);
        if r.u16()? != MASTER_MAGIC {
            return Err(Truncated);
        }
        Ok(match r.u8()? {
            1 => MasterMsg::Heartbeat(read_info(&mut r)?),
            2 => MasterMsg::List { offset: r.u32()? },
            3 => {
                let total = r.u32()?;
                let offset = r.u32()?;
                let n = r.varint()?.min(1024);
                let mut entries = Vec::new();
                for _ in 0..n {
                    entries.push(ServerEntry {
                        addr: r.str()?,
                        info: read_info(&mut r)?,
                        age_s: r.u16()?,
                    });
                }
                MasterMsg::Servers {
                    total,
                    offset,
                    entries,
                }
            }
            4 => MasterMsg::Query { nonce: r.u32()? },
            5 => MasterMsg::Info {
                nonce: r.u32()?,
                info: read_info(&mut r)?,
            },
            _ => return Err(Truncated),
        })
    }
}

/// The master's list: servers by `host:port`, the last heartbeat time of each.
#[derive(Default)]
pub struct MasterNet {
    pub servers: BTreeMap<String, (ServerInfo, u64)>,
    pub heartbeats: u64,
    pub lists: u64,
}

impl MasterNet {
    /// Handles heartbeats and list requests that arrived; `now_s` is the master's clock.
    pub fn receive(&mut self, transport: &mut dyn Transport, now_s: u64) {
        for (from, packet) in transport.recv() {
            let Ok(msg) = MasterMsg::read(&packet) else {
                continue;
            };
            match msg {
                MasterMsg::Heartbeat(info) => {
                    let host = transport.peer_name(from);
                    let host = host.rsplit_once(':').map(|(h, _)| h).unwrap_or(&host);
                    let key = format!("{host}:{}", info.port);
                    self.heartbeats += 1;
                    self.servers.insert(key, (info, now_s));
                }
                MasterMsg::List { offset } => {
                    self.lists += 1;
                    transport.send(from, self.page(offset, now_s).encode());
                }
                _ => {}
            }
        }
    }

    /// One `Servers` reply from `offset`, as many entries as fit the budget.
    pub fn page(&self, offset: u32, now_s: u64) -> MasterMsg {
        let mut entries = Vec::new();
        let mut size = 16;
        for (addr, (info, seen)) in self.servers.iter().skip(offset as usize) {
            let e = ServerEntry {
                addr: addr.clone(),
                info: info.clone(),
                age_s: now_s.saturating_sub(*seen).min(u16::MAX as u64) as u16,
            };
            let n = MasterMsg::Servers {
                total: 0,
                offset: 0,
                entries: vec![e.clone()],
            }
            .encode()
            .len();
            if size + n > LIST_BUDGET && !entries.is_empty() {
                break;
            }
            size += n;
            entries.push(e);
        }
        MasterMsg::Servers {
            total: self.servers.len() as u32,
            offset,
            entries,
        }
    }

    /// Drops servers silent for longer than `TTL_S`.
    pub fn expire(&mut self, now_s: u64) -> Vec<String> {
        let gone: Vec<String> = self
            .servers
            .iter()
            .filter(|(_, (_, seen))| now_s.saturating_sub(*seen) > TTL_S)
            .map(|(k, _)| k.clone())
            .collect();
        for k in &gone {
            self.servers.remove(k);
        }
        gone
    }
}
