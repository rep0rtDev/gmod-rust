//! The server side: clients, their usercmd buffers and reliable commands, snapshots at
//! `snapshot_every` ticks as deltas against the newest snapshot the client acked.

use crate::channel::Channel;
use crate::msg::{
    auth_message, server_message, ClientMsg, Command, EntityState, LocalPlayer, OwnSandbox,
    PlayerSnap, ServerMsg, SoundEvent, UndoEntry, PROTOCOL,
};
use crate::transport::Transport;
use crate::wire::{Reader, Writer};
use glam::Vec3;
use gm_server::{PlayerCmd, Server};
use std::collections::{BTreeMap, VecDeque};

/// Longest chat line kept (the game's `say` is cut at 127 characters).
pub const MAX_CHAT_CHARS: usize = 127;

pub struct ClientSlot {
    pub player: u32,
    pub name: String,
    channel: Channel,
    /// usercmds by tick, waiting to be run
    cmds: BTreeMap<u32, PlayerCmd>,
    last_cmd: PlayerCmd,
    last_cmd_tick: u32,
    /// sandbox commands by their client sequence; executed in order, once
    commands: BTreeMap<u32, Command>,
    next_command: u32,
    acked_snapshot: u32,
    /// snapshots we sent and still may use as a baseline: tick -> entities
    sent: VecDeque<(u32, BTreeMap<u32, EntityState>)>,
    event_seq: u32,
    pending_events: Vec<String>,
    pending_addon: Vec<(String, Vec<u8>)>,
    addon_seq: u32,
    pending_cvars: Vec<(String, String)>,
    cvar_seq: u32,
    /// published packages being streamed to this client: (index, next offset), in request order
    uploads: VecDeque<(usize, u64)>,
    undo_seq: u32,
    sent_undos: Vec<UndoEntry>,
    /// How many entries of `ServerNet::models` this client has been sent.
    models_sent: usize,
    /// Last snapshot tick each player / entity state went to this client (for the staleness
    /// weight in the budget order).
    last_sent_player: BTreeMap<u32, u32>,
    last_sent_entity: BTreeMap<u32, u32>,
    /// Nonce of the `Challenge` sent after `Hello` (zero before).
    nonce: [u8; 32],
    /// Proven identity (ed25519 public key) of this connection.
    pub identity: Option<[u8; 32]>,
    welcomed: bool,
    eph: crate::crypto::Ephemeral,
    crypto: Option<crate::crypto::Session>,
    /// Server tick of the last packet from this client.
    last_heard: u32,
    /// A `Disconnect` was sent; the slot goes once the channel has flushed or at the timeout.
    closing: bool,
}

/// An addon package the server hands out to clients (its client-realm actors run there).
pub struct PublishedAddon {
    pub info: crate::msg::AddonInfo,
    pub bytes: std::sync::Arc<Vec<u8>>,
}

/// Bytes of a package queued per tick per client while the channel has room.
pub const DEFAULT_INTEREST_RADIUS: f32 = 8192.0;
/// 750 bytes per snapshot at the default snapshot rate is the design's 25 KB/s per client.
pub const DEFAULT_SNAPSHOT_BUDGET: usize = 750;
/// Upper bound of the wire size of one `PlayerSnap` (quantized, positions as varints).
pub const PLAYER_SNAP_SIZE: usize = 44;
/// Upper bound of the wire size of one `EntityState` (the model goes as a table index).
pub const ENTITY_SNAP_SIZE: usize = 44;
pub const ADDON_CHUNK: usize = 16 * 1024;
/// 10 seconds at the default tick rate.
pub const DEFAULT_TIMEOUT_TICKS: u32 = 667;
/// Reliable fragments in flight above which a client's upload waits (other reliable traffic
/// keeps flowing between chunks).
pub const UPLOAD_WINDOW: usize = 24;

pub struct ServerNet {
    pub tick: u32,
    pub snapshot_every: u32,
    pub map: String,
    pub clients: BTreeMap<usize, ClientSlot>,
    next_player: u32,
    pub spawn_origin: Vec3,
    pub hooks: Option<Box<dyn gm_server::ServerHooks>>,
    /// Players allowed to run admin commands (`Command::Reload`).
    pub admins: std::collections::BTreeSet<u32>,
    /// Identities that become admins when they authenticate.
    pub admin_keys: std::collections::BTreeSet<[u8; 32]>,
    /// Banned identities: refused at `Auth`; with `require_auth` that is before the `Welcome`.
    pub bans: std::collections::BTreeSet<[u8; 32]>,
    /// Connections beyond this are refused with `full`.
    pub max_players: usize,
    /// Ticks without a packet before a client is dropped (`timed out`).
    pub timeout_ticks: u32,
    /// World sounds of the last tick, sent with the next snapshots.
    pending_sounds: Vec<SoundEvent>,
    /// Human-readable connection events (joins, leaves, kicks) since the last `take_log`.
    pub log: Vec<String>,
    /// Where the console's `save` / `load` keep `<name>.rdup`.
    pub saves_dir: Option<std::path::PathBuf>,
    pub addons: Vec<PublishedAddon>,
    /// What the server browser is told (`players` and `addons` are filled on send).
    pub info: crate::master::ServerInfo,
    /// The model table every connection shares: `models[i]` is model id `i + 1`.
    pub models: Vec<String>,
    model_ids: BTreeMap<String, u32>,
    /// Interest by PVS: besides the radius, an entity or player must be in a cluster visible
    /// from the player's cluster (`gm_collision::World::visible`).
    pub use_pvs: bool,
    /// Only authenticated connections (`ClientMsg::Auth` with a valid signature) are welcomed.
    pub require_auth: bool,
    /// Long-lived identity: signs every `Challenge` so clients can pin the server
    /// (`ClientNet::with_server_key`); its public key is published in `info.key`.
    pub key: Option<ed25519_dalek::SigningKey>,
    /// Interest: a client gets entities and other players within this distance of its player
    /// (Source units); what leaves the radius is removed on the client, what enters is sent whole.
    pub interest_radius: f32,
    /// Bytes of entity states per snapshot: the nearest changed entities go first, the rest wait
    /// for the next snapshot (they stay different from the acked baseline until sent).
    pub snapshot_budget: usize,
}

impl ServerNet {
    /// Gives the server a long-lived identity (ed25519 secret); returns its public key, which is
    /// also published to the browser through `info.key`.
    pub fn with_key(&mut self, secret: &[u8; 32]) -> [u8; 32] {
        let key = ed25519_dalek::SigningKey::from_bytes(secret);
        let public = key.verifying_key().to_bytes();
        self.info.key = Some(public);
        self.key = Some(key);
        public
    }

    pub fn new(map: &str, spawn_origin: Vec3) -> Self {
        ServerNet {
            tick: 0,
            snapshot_every: 2,
            map: map.to_string(),
            clients: BTreeMap::new(),
            next_player: 1,
            spawn_origin,
            hooks: None,
            admins: std::collections::BTreeSet::new(),
            admin_keys: std::collections::BTreeSet::new(),
            bans: std::collections::BTreeSet::new(),
            max_players: 64,
            timeout_ticks: DEFAULT_TIMEOUT_TICKS,
            pending_sounds: Vec::new(),
            log: Vec::new(),
            saves_dir: None,
            addons: Vec::new(),
            models: Vec::new(),
            model_ids: BTreeMap::new(),
            use_pvs: true,
            require_auth: false,
            key: None,
            interest_radius: DEFAULT_INTEREST_RADIUS,
            snapshot_budget: DEFAULT_SNAPSHOT_BUDGET,
            info: crate::master::ServerInfo {
                map: map.to_string(),
                max_players: 16,
                version: env!("CARGO_PKG_VERSION").to_string(),
                ..Default::default()
            },
        }
    }

    /// Publishes a `.gmr` for clients to download (announced to clients connecting from now on;
    /// a package with the same sha is published once).
    pub fn publish_addon(&mut self, info: crate::msg::AddonInfo, bytes: Vec<u8>) {
        if self.addons.iter().any(|a| a.info.sha == info.sha) {
            return;
        }
        self.addons.push(PublishedAddon {
            info,
            bytes: std::sync::Arc::new(bytes),
        });
        let list = addon_list_of(&self.addons);
        for slot in self.clients.values_mut().filter(|s| s.player != 0) {
            let mut w = Writer::default();
            ServerMsg::Addons { list: list.clone() }.write(&mut w);
            slot.channel.send_reliable(w.buf);
        }
    }

    /// `info` with the live player count and published addon ids.
    pub fn current_info(&self) -> crate::master::ServerInfo {
        let mut info = self.info.clone();
        info.players = self.clients.values().filter(|c| c.player != 0).count() as u16;
        info.addons = self.addon_list().into_iter().map(|a| a.id).collect();
        info
    }

    /// Sends a heartbeat to the master at this endpoint.
    pub fn heartbeat(&self, transport: &mut dyn Transport, master: usize) {
        let info = self.current_info();
        transport.send(master, crate::master::MasterMsg::Heartbeat(info).encode());
    }

    /// What clients are told to have: the latest publication of every addon id (older packages
    /// stay downloadable so a transfer in flight finishes).
    pub fn addon_list(&self) -> Vec<crate::msg::AddonInfo> {
        addon_list_of(&self.addons)
    }

    /// Reads everything the transport has for the server (endpoint 0), connecting new clients.
    pub fn receive(&mut self, server: &mut Server<'_>, transport: &mut dyn Transport) {
        for (from, packet) in transport.recv() {
            if crate::master::is_master_packet(&packet) {
                if let Ok(crate::master::MasterMsg::Query { nonce }) =
                    crate::master::MasterMsg::read(&packet)
                {
                    let info = self.current_info();
                    transport.send(
                        from,
                        crate::master::MasterMsg::Info { nonce, info }.encode(),
                    );
                }
                continue;
            }
            if !self.clients.contains_key(&from) {
                // only a plaintext handshake opens a connection: sealed packets of a dropped
                // client (or anything else) must not create a ghost slot and player
                if packet.first() != Some(&crate::crypto::PLAIN) {
                    continue;
                }
                let player = self.next_player;
                self.next_player += 1;
                self.clients.insert(
                    from,
                    ClientSlot {
                        player,
                        name: String::new(),
                        channel: Channel::default(),
                        cmds: BTreeMap::new(),
                        last_cmd: PlayerCmd::default(),
                        last_cmd_tick: 0,
                        commands: BTreeMap::new(),
                        next_command: 0,
                        acked_snapshot: 0,
                        sent: VecDeque::new(),
                        event_seq: 0,
                        pending_events: Vec::new(),
                        pending_addon: Vec::new(),
                        addon_seq: 0,
                        pending_cvars: Vec::new(),
                        cvar_seq: 0,
                        uploads: VecDeque::new(),
                        undo_seq: 0,
                        sent_undos: Vec::new(),
                        models_sent: 0,
                        last_sent_player: BTreeMap::new(),
                        last_sent_entity: BTreeMap::new(),
                        nonce: [0; 32],
                        identity: None,
                        welcomed: false,
                        eph: crate::crypto::Ephemeral::generate(),
                        crypto: None,
                        last_heard: self.tick,
                        closing: false,
                    },
                );
                server.add_player(player, self.spawn_origin, Vec3::ZERO);
            }
            let open = self.clients.values().filter(|c| !c.closing).count();
            let slot = self.clients.get_mut(&from).unwrap();
            let body = match (packet.first(), &mut slot.crypto) {
                (Some(&crate::crypto::SEALED), Some(c)) => match c.open(&packet) {
                    Some(b) => b,
                    None => continue,
                },
                // plaintext only until the client has shown it holds the key
                (Some(&crate::crypto::PLAIN), c) if c.as_ref().is_none_or(|c| !c.peer_sealed) => {
                    packet[1..].to_vec()
                }
                _ => continue,
            };
            let Ok(msgs) = slot.channel.receive(&body) else {
                continue;
            };
            slot.last_heard = self.tick;
            if slot.closing {
                continue;
            }
            for m in msgs {
                let Ok(msg) = ClientMsg::read(&mut Reader::new(&m)) else {
                    continue;
                };
                match msg {
                    ClientMsg::Hello {
                        name,
                        protocol,
                        pubkey,
                    } => {
                        if protocol == PROTOCOL {
                            slot.name = name;
                            if open > self.max_players && !slot.welcomed {
                                refuse(slot, "full");
                                self.log.push(format!("{} refused: full", slot.name));
                                continue;
                            }
                            let mut nonce = [0u8; 32];
                            rand_core::RngCore::fill_bytes(&mut rand_core::OsRng, &mut nonce);
                            slot.nonce = nonce;
                            if slot.crypto.is_none() {
                                slot.crypto = Some(slot.eph.session(
                                    &pubkey,
                                    &pubkey,
                                    &slot.eph.public,
                                    false,
                                ));
                            }
                            let mut w = Writer::default();
                            ServerMsg::Challenge {
                                nonce,
                                pubkey: slot.eph.public,
                                identity: self.key.as_ref().map(|k| {
                                    let sig = ed25519_dalek::Signer::sign(
                                        k,
                                        &server_message(&nonce, &slot.eph.public),
                                    );
                                    (k.verifying_key().to_bytes(), sig.to_bytes())
                                }),
                            }
                            .write(&mut w);
                            slot.channel.send_reliable(w.buf);
                            if !self.require_auth {
                                welcome(
                                    slot,
                                    &self.map,
                                    self.tick,
                                    &self.addons,
                                    server,
                                    &mut self.log,
                                );
                            }
                        }
                    }
                    ClientMsg::Auth { pubkey, sig } => {
                        let binding = slot.crypto.as_ref().map(|c| c.binding);
                        let ok = slot.nonce != [0; 32]
                            && binding.is_some_and(|b| {
                                ed25519_dalek::VerifyingKey::from_bytes(&pubkey).is_ok_and(|k| {
                                    ed25519_dalek::Signature::from_slice(&sig).is_ok_and(|s| {
                                        k.verify_strict(&auth_message(&slot.nonce, &b), &s).is_ok()
                                    })
                                })
                            });
                        if ok && self.bans.contains(&pubkey) {
                            refuse(slot, "banned");
                            self.log.push(format!(
                                "{} refused: banned {}",
                                slot.name,
                                hex(&pubkey)
                            ));
                            continue;
                        }
                        if ok {
                            slot.identity = Some(pubkey);
                            if let Some(p) = server.players.get_mut(&slot.player) {
                                p.identity = Some(pubkey);
                            }
                            if self.admin_keys.contains(&pubkey) {
                                self.admins.insert(slot.player);
                            }
                            if !slot.welcomed {
                                welcome(
                                    slot,
                                    &self.map,
                                    self.tick,
                                    &self.addons,
                                    server,
                                    &mut self.log,
                                );
                            }
                        }
                    }
                    ClientMsg::Disconnect => {
                        slot.closing = true;
                        self.log.push(format!("{} left", slot.name));
                    }
                    ClientMsg::Cmds { cmds } => {
                        for c in cmds {
                            if c.tick > slot.last_cmd_tick || slot.last_cmd_tick == 0 {
                                slot.cmds.entry(c.tick).or_insert(PlayerCmd {
                                    seen_tick: c.seen_tick,
                                    cmd: gm_move::UserCmd {
                                        buttons: c.buttons,
                                        view_angles: c.view_angles,
                                        forward_move: c.forward_move,
                                        side_move: c.side_move,
                                        up_move: c.up_move,
                                    },
                                    physgun: c.weapon == 1,
                                    mouse_x: c.mouse_x,
                                    mouse_y: c.mouse_y,
                                });
                            }
                        }
                    }
                    ClientMsg::Command { seq, cmd } => {
                        if seq >= slot.next_command {
                            slot.commands.entry(seq).or_insert(cmd);
                        }
                        let mut w = Writer::default();
                        ServerMsg::CommandAck { seq }.write(&mut w);
                        slot.channel.send_reliable(w.buf);
                    }
                    ClientMsg::Addon { name, data } => {
                        if let Some(h) = self.hooks.as_mut() {
                            h.net_message(server, slot.player, &name, &data);
                        }
                    }
                    ClientMsg::WantAddon { sha } => {
                        if let Some(i) = self.addons.iter().position(|a| a.info.sha == sha) {
                            if !slot.uploads.iter().any(|(j, _)| *j == i) {
                                slot.uploads.push_back((i, 0));
                            }
                        }
                    }
                    ClientMsg::AckSnapshot { tick } => {
                        if tick > slot.acked_snapshot {
                            slot.acked_snapshot = tick;
                        }
                    }
                }
            }
        }
    }

    /// Runs one server tick: the newest due usercmd of every client (the last one again when
    /// none arrived, like the engine's command deficit), then the sandbox commands in order.
    /// Drop a player now: a reliable `Disconnect { reason }`, its entry leaves the server table
    /// (props stay), `ServerHooks::player_disconnected`; `ban` remembers the identity.
    pub fn kick(&mut self, server: &mut Server<'_>, player: u32, reason: &str, ban: bool) -> bool {
        let Some((_, slot)) = self.clients.iter_mut().find(|(_, s)| s.player == player) else {
            return false;
        };
        if ban {
            if let Some(id) = slot.identity {
                self.bans.insert(id);
            }
        }
        let name = slot.name.clone();
        refuse(slot, reason);
        self.log.push(format!("{name} (player {player}) {reason}"));
        self.drop_player(server, player);
        true
    }

    fn drop_player(&mut self, server: &mut Server<'_>, player: u32) {
        if server.remove_player(player) {
            if let Some(h) = self.hooks.as_mut() {
                h.player_disconnected(server, player);
            }
        }
        self.admins.remove(&player);
    }

    /// An admin console line; the reply lines go back to the issuer as events.
    pub fn console(&mut self, server: &mut Server<'_>, admin: u32, line: &str) -> Vec<String> {
        let mut words = line.split_whitespace();
        let verb = words.next().unwrap_or("");
        let rest: Vec<&str> = words.collect();
        let player_arg = |rest: &[&str]| rest.first().and_then(|w| w.parse::<u32>().ok());
        match verb {
            "help" => vec![
                "status | kick <player> [reason] | ban <player> | unban <key> | cvar <name> [value] \
                 | reload <addon> | say <text> | admins | save <name> | load <name>"
                    .to_string(),
            ],
            "save" | "load" => {
                let Some(dir) = self.saves_dir.clone() else {
                    return vec![format!("{verb}: no saves directory")];
                };
                let Some(name) = rest.first().filter(|n| {
                    !n.is_empty() && n.chars().all(|c| c.is_ascii_alphanumeric() || c == '_' || c == '-')
                }) else {
                    return vec![format!("{verb}: name expected (letters, digits, _ -)")];
                };
                let path = dir.join(format!("{name}.{}", gm_server::save::EXTENSION));
                if verb == "save" {
                    let save = server.save_world(&self.map);
                    match std::fs::create_dir_all(&dir)
                        .and_then(|_| serde_json::to_vec_pretty(&save).map_err(std::io::Error::other))
                        .and_then(|bytes| std::fs::write(&path, bytes))
                    {
                        Ok(()) => vec![format!(
                            "save {name}: {} props {} constraints",
                            save.props.len(),
                            save.constraints.len()
                        )],
                        Err(e) => vec![format!("save {name}: {e}")],
                    }
                } else {
                    match std::fs::read(&path)
                        .and_then(|b| serde_json::from_slice::<gm_server::save::Save>(&b).map_err(std::io::Error::other))
                    {
                        Ok(save) => {
                            let (p, c) = server.load_world(&save, admin);
                            vec![format!("load {name}: {p} props {c} constraints (saved on {})", save.map)]
                        }
                        Err(e) => vec![format!("load {name}: {e}")],
                    }
                }
            }
            "status" => {
                let mut out = vec![format!(
                    "{} tick {} players {}/{} props {} constraints {}",
                    self.map,
                    self.tick,
                    self.clients.values().filter(|c| c.welcomed && !c.closing).count(),
                    self.max_players,
                    server.sandbox.props().len(),
                    server.sandbox.constraints().len()
                )];
                for c in self.clients.values().filter(|c| c.welcomed && !c.closing) {
                    out.push(format!(
                        "  {} {} {}{}",
                        c.player,
                        c.name,
                        c.identity.map(|k| hex(&k)).unwrap_or_else(|| "anonymous".into()),
                        if self.admins.contains(&c.player) { " admin" } else { "" }
                    ));
                }
                out
            }
            "kick" | "ban" => match player_arg(&rest) {
                Some(p) if p == admin => vec![format!("{verb} {p}: not yourself")],
                Some(p) => {
                    let ban = verb == "ban";
                    let reason = if ban {
                        "banned".to_string()
                    } else if rest.len() > 1 {
                        rest[1..].join(" ")
                    } else {
                        "kicked".to_string()
                    };
                    if self.kick(server, p, &reason, ban) {
                        vec![format!("{verb} {p}: ok")]
                    } else {
                        vec![format!("{verb} {p}: no such player")]
                    }
                }
                None => vec![format!("{verb}: player id expected")],
            },
            "unban" => match rest.first().and_then(|w| parse_hex(w)) {
                Some(k) if self.bans.remove(&k) => vec!["unban: ok".to_string()],
                Some(_) => vec!["unban: not banned".to_string()],
                None => vec!["unban: public key (32 hex bytes) expected".to_string()],
            },
            "cvar" => match (rest.first(), rest.get(1)) {
                (Some(n), None) => vec![match server.cvar(n) {
                    Some(v) => format!("{n} = {v}"),
                    None => format!("{n}: unknown cvar"),
                }],
                (Some(n), Some(v)) => {
                    if server.set_cvar(n, v) {
                        vec![format!("{n} = {}", server.cvar(n).unwrap_or_default())]
                    } else {
                        vec![format!("{n}: unknown cvar or bad value")]
                    }
                }
                _ => vec!["cvar: name expected".to_string()],
            },
            "reload" => match rest.first() {
                Some(a) => vec![match self.hooks.as_mut() {
                    Some(h) => match h.reload_addon(a) {
                        Ok(info) => format!("reload {a}: ok {info}"),
                        Err(e) => format!("reload {a}: failed {e}"),
                    },
                    None => format!("reload {a}: failed no addon runtime"),
                }],
                None => vec!["reload: addon id expected".to_string()],
            },
            "say" => {
                let text = format!("[server] {}", rest.join(" "));
                for c in self.clients.values_mut() {
                    if c.welcomed && !c.closing {
                        c.pending_events.push(text.clone());
                    }
                }
                Vec::new()
            }
            "admins" => vec![format!(
                "admins: {}",
                self.admins
                    .iter()
                    .map(|a| a.to_string())
                    .collect::<Vec<_>>()
                    .join(" ")
            )],
            "" => Vec::new(),
            other => vec![format!("{other}: unknown command (help)")],
        }
    }

    /// Connection events since the last call (joins, leaves, refusals, kicks, timeouts).
    pub fn take_log(&mut self) -> Vec<String> {
        std::mem::take(&mut self.log)
    }

    pub fn tick(&mut self, server: &mut Server<'_>) {
        // timeouts and closed connections
        let timeout = self.timeout_ticks;
        let tick = self.tick;
        let gone: Vec<(usize, u32, bool, String)> = self
            .clients
            .iter()
            .filter(|(_, s)| {
                tick.wrapping_sub(s.last_heard) > timeout
                    || (s.closing && s.channel.pending_reliable() == 0)
            })
            .map(|(e, s)| (*e, s.player, s.closing, s.name.clone()))
            .collect();
        for (endpoint, player, closing, name) in gone {
            self.clients.remove(&endpoint);
            if !closing {
                self.log.push(format!("{name} (player {player}) timed out"));
            }
            self.drop_player(server, player);
        }
        self.tick += 1;
        let mut cmds = BTreeMap::new();
        for slot in self.clients.values_mut() {
            // like the engine: the backlog after a lost packet is caught up (up to 3 usercmds in
            // one tick, the extra ones move the player only), and with none due the player stands
            let due: Vec<u32> = slot.cmds.keys().copied().take(3).collect();
            let n = due.len();
            for (i, t) in due.into_iter().enumerate() {
                let pc = slot.cmds.remove(&t).unwrap();
                slot.last_cmd_tick = t;
                if i + 1 < n {
                    server.move_player(slot.player, &pc);
                } else {
                    slot.last_cmd = pc.clone();
                    cmds.insert(slot.player, pc);
                }
            }
        }
        if let Some(h) = self.hooks.as_mut() {
            h.pre_tick(server);
        }
        server.tick(&cmds);
        self.pending_sounds
            .extend(server.take_sounds().into_iter().map(|s| SoundEvent {
                name: s.name,
                pos: s.pos,
                volume: s.volume,
            }));
        if let Some(h) = self.hooks.as_mut() {
            let events = std::mem::take(&mut server.game_events);
            if !events.is_empty() {
                h.game_events(server, events);
            }
            h.post_tick(server);
            for p in h.drain_published() {
                let info = crate::msg::AddonInfo {
                    id: p.id,
                    version: p.version,
                    sha: p.sha,
                    size: p.bytes.len() as u64,
                };
                self.publish_addon(info, p.bytes);
            }
        }
        if let Some(h) = self.hooks.as_mut() {
            for (player, name, data) in h.drain_net() {
                for slot in self.clients.values_mut() {
                    if player == 0 || slot.player == player {
                        slot.pending_addon.push((name.clone(), data.clone()));
                    }
                }
            }
        }
        for name in server.take_cvar_new() {
            if let Some(c) = server.cvars.get(&name).filter(|c| c.replicated) {
                for slot in self.clients.values_mut() {
                    slot.pending_cvars.push((name.clone(), c.value.clone()));
                }
            }
        }
        for (name, _, value) in server.take_cvar_changes() {
            if server.cvars.get(&name).is_some_and(|c| c.replicated) {
                for slot in self.clients.values_mut() {
                    slot.pending_cvars.push((name.clone(), value.clone()));
                }
            }
        }
        let mut kicks: Vec<(u32, bool)> = Vec::new();
        let mut console: Vec<(u32, String)> = Vec::new();
        let mut chats: Vec<(u32, String, String)> = Vec::new();
        for slot in self.clients.values_mut() {
            while let Some(cmd) = slot.commands.remove(&slot.next_command) {
                slot.next_command += 1;
                let n = server.events.len();
                match cmd {
                    Command::Spawn { model } => {
                        let allowed = self
                            .hooks
                            .as_mut()
                            .is_none_or(|h| h.prop_spawn_request(server, slot.player, &model));
                        if allowed {
                            if let Some(ent) = server.spawn_prop(slot.player, &model) {
                                if let Some(h) = self.hooks.as_mut() {
                                    h.entity_spawned(server, slot.player, ent);
                                }
                            }
                        }
                    }
                    Command::Undo => server.undo(slot.player),
                    Command::SelectWeapon { class } => {
                        server.select_weapon(slot.player, &class);
                    }
                    Command::Cleanup => server.cleanup(slot.player),
                    Command::ToolClick {
                        tool,
                        right,
                        reload,
                    } => {
                        if reload {
                            server.sandbox.tool_reload(slot.player);
                        } else if let Some(hit) = server.tool_hit(slot.player) {
                            let allowed = self.hooks.as_mut().is_none_or(|h| {
                                h.tool_before(server, slot.player, &tool, right, &hit)
                            });
                            if allowed {
                                let done =
                                    match server.tool_click_at(slot.player, &tool, right, hit) {
                                        gm_server::ToolClick::Done(done) => done,
                                        gm_server::ToolClick::Miss => false,
                                        gm_server::ToolClick::Unknown(hit) => self
                                            .hooks
                                            .as_mut()
                                            .map(|h| {
                                                h.tool_click(
                                                    server,
                                                    slot.player,
                                                    &tool,
                                                    &hit,
                                                    right,
                                                )
                                            })
                                            .unwrap_or(false),
                                    };
                                if done {
                                    server.tool_fired(slot.player);
                                }
                                if let Some(h) = self.hooks.as_mut() {
                                    h.tool_after(server, slot.player, &tool, right, done);
                                }
                            }
                        }
                    }
                    Command::Cvar { name, value } => {
                        // server cvars change through an admin (the game's rcon / sv_cheats path)
                        if !self.admins.contains(&slot.player) {
                            slot.pending_events.push(format!("Cvar {name} denied"));
                        } else if !server.set_cvar(&name, &value) {
                            slot.pending_events.push(format!("Cvar {name} failed"));
                        }
                    }
                    Command::Kick { player, ban } => {
                        let msg = if !self.admins.contains(&slot.player) {
                            format!("Kick {player} denied")
                        } else if player == slot.player {
                            format!("Kick {player} denied self")
                        } else {
                            kicks.push((player, ban));
                            format!("Kick {player} ok")
                        };
                        slot.pending_events.push(msg);
                    }
                    Command::Console { line } => {
                        if self.admins.contains(&slot.player) {
                            console.push((slot.player, line));
                        } else {
                            slot.pending_events.push("Console denied".to_string());
                        }
                    }
                    Command::Noclip { on } => {
                        if !server.set_noclip(slot.player, on) {
                            slot.pending_events.push("Noclip denied".to_string());
                        }
                    }
                    Command::Chat { text } => {
                        let text: String = text.chars().take(MAX_CHAT_CHARS).collect();
                        if !text.trim().is_empty() {
                            chats.push((slot.player, slot.name.clone(), text));
                        }
                    }
                    Command::Reload { addon } => {
                        let msg = if !self.admins.contains(&slot.player) {
                            format!("Reload {addon} denied")
                        } else {
                            match self.hooks.as_mut() {
                                Some(h) => match h.reload_addon(&addon) {
                                    Ok(info) => format!("Reload {addon} ok {info}"),
                                    Err(e) => format!("Reload {addon} failed {e}"),
                                },
                                None => format!("Reload {addon} failed no addon runtime"),
                            }
                        };
                        slot.pending_events.push(msg);
                    }
                }
                let mut removed = Vec::new();
                for e in &server.events[n..] {
                    match e {
                        gm_sandbox::Event::LimitHit { limit } => {
                            slot.pending_events.push(format!("LimitHit {limit}"));
                        }
                        gm_sandbox::Event::Removed { prop } => removed.push(*prop),
                        _ => {}
                    }
                }
                if let Some(h) = self.hooks.as_mut() {
                    for ent in removed {
                        h.entity_removed(server, slot.player, ent);
                    }
                }
            }
        }
        for (player, ban) in kicks {
            self.kick(server, player, if ban { "banned" } else { "kicked" }, ban);
        }
        for (player, line) in console {
            let replies = self.console(server, player, &line);
            if let Some(slot) = self.clients.values_mut().find(|s| s.player == player) {
                slot.pending_events.extend(replies);
            }
        }
        for (player, name, text) in chats {
            let text = match self.hooks.as_mut() {
                Some(h) => h.player_chat(server, player, &text),
                None => Some(text),
            };
            if let Some(text) = text {
                let line = format!("{name}: {text}");
                self.log.push(format!("chat {line}"));
                for c in self.clients.values_mut() {
                    if c.welcomed && !c.closing {
                        c.pending_events.push(line.clone());
                    }
                }
            }
        }
        if let Some(h) = self.hooks.as_mut() {
            h.after_commands(server);
        }
    }

    /// Sends this tick's packets: a snapshot every `snapshot_every` ticks plus pending reliables.
    pub fn send(&mut self, server: &Server<'_>, transport: &mut dyn Transport) {
        let snapshot_tick = self.tick.is_multiple_of(self.snapshot_every);
        let mut world: BTreeMap<u32, EntityState> = BTreeMap::new();
        // cluster of every prop once per send, not once per client
        let mut clusters: BTreeMap<u32, i32> = BTreeMap::new();
        for p in server.sandbox.props() {
            if self.use_pvs {
                clusters.insert(p.id, server.collision.cluster_at(p.pos));
            }
            let st = server.prop_state(p.id);
            let model_id = match self.model_ids.get(&p.model) {
                Some(&id) => id,
                None => {
                    self.models.push(p.model.clone());
                    let id = self.models.len() as u32;
                    self.model_ids.insert(p.model.clone(), id);
                    id
                }
            };
            world.insert(
                p.id,
                EntityState {
                    id: p.id,
                    model: p.model.clone(),
                    model_id,
                    pos: p.pos,
                    angles: p.angles,
                    vel: st.map(|s| s.velocity).unwrap_or_default(),
                    angvel: st.map(|s| s.angular_velocity).unwrap_or_default(),
                    frozen: p.frozen,
                    owner: p.owner,
                },
            );
        }
        let players: Vec<PlayerSnap> = server
            .players
            .iter()
            .map(|(id, p)| PlayerSnap {
                id: *id,
                origin: p.state.origin,
                velocity: p.state.velocity,
                view_angles: p.view_angles,
                ducked: p.state.ducked,
                on_ground: p.state.on_ground,
                held: p.held.map(|h| h.0),
                alive: p.alive,
                health: p.health.clamp(i16::MIN as i32, i16::MAX as i32) as i16,
                armor: p.armor.clamp(i16::MIN as i32, i16::MAX as i32) as i16,
                weapons: crate::msg::weapon_mask(&p.weapons),
                active: crate::msg::weapon_index(&p.active_weapon),
                clip: p.inventory.clip(&p.active_weapon)[0].clamp(-1, i16::MAX as i32) as i16,
                reserve: gm_server::weapons::def(&p.active_weapon)
                    .and_then(|d| p.inventory.ammo.get(d.ammo1))
                    .map_or(0, |a| (*a).clamp(0, i16::MAX as i32) as i16),
                alt: gm_server::weapons::def(&p.active_weapon)
                    .filter(|d| !d.ammo2.is_empty())
                    .map_or(-1, |d| {
                        p.inventory
                            .ammo
                            .get(d.ammo2)
                            .map_or(0, |a| (*a).clamp(0, i16::MAX as i32) as i16)
                    }),
                vm_act: p.inventory.vm_act as u8,
                vm_serial: p.inventory.vm_serial,
            })
            .collect();
        // sounds ride with the snapshots: on a tick without one they stay pending
        let sounds = if snapshot_tick {
            std::mem::take(&mut self.pending_sounds)
        } else {
            Vec::new()
        };
        for (&endpoint, slot) in self.clients.iter_mut() {
            for text in std::mem::take(&mut slot.pending_events) {
                slot.event_seq += 1;
                let mut w = Writer::default();
                ServerMsg::Event {
                    seq: slot.event_seq,
                    text,
                }
                .write(&mut w);
                slot.channel.send_reliable(w.buf);
            }
            for (name, value) in std::mem::take(&mut slot.pending_cvars) {
                slot.cvar_seq += 1;
                let mut w = Writer::default();
                ServerMsg::Cvar {
                    seq: slot.cvar_seq,
                    name,
                    value,
                }
                .write(&mut w);
                slot.channel.send_reliable(w.buf);
            }
            for (name, data) in std::mem::take(&mut slot.pending_addon) {
                slot.addon_seq += 1;
                let mut w = Writer::default();
                ServerMsg::Addon {
                    seq: slot.addon_seq,
                    name,
                    data,
                }
                .write(&mut w);
                slot.channel.send_reliable(w.buf);
            }
            let mut unreliable = Vec::new();
            if snapshot_tick {
                if slot.models_sent < self.models.len() {
                    let mut w = Writer::default();
                    ServerMsg::Models {
                        from: slot.models_sent as u32 + 1,
                        names: self.models[slot.models_sent..].to_vec(),
                    }
                    .write(&mut w);
                    slot.channel.send_reliable(w.buf);
                    slot.models_sent = self.models.len();
                }
                slot.sent.retain(|(t, _)| *t >= slot.acked_snapshot);
                let baseline = slot
                    .sent
                    .iter()
                    .find(|(t, _)| *t == slot.acked_snapshot)
                    .map(|(t, e)| (*t, e.clone()));
                let (baseline_tick, base) = match baseline {
                    Some((t, e)) => (t, e),
                    None => (0, BTreeMap::new()),
                };
                let eye = server.players.get(&slot.player).map(|p| p.state.origin);
                let eye_cluster =
                    eye.map(|e| server.collision.cluster_at(e + Vec3::new(0.0, 0.0, 64.0)));
                let near = |pos: Vec3, cluster: Option<i32>| match eye {
                    Some(e) => {
                        e.distance(pos) <= self.interest_radius
                            && (!self.use_pvs
                                || eye_cluster.is_none_or(|c| {
                                    let to =
                                        cluster.unwrap_or_else(|| server.collision.cluster_at(pos));
                                    server.collision.visible(c, to)
                                }))
                    }
                    None => true,
                };
                let near_ent = |e: &EntityState| near(e.pos, clusters.get(&e.id).copied());
                let tick = self.tick;
                let age =
                    |last: Option<&u32>| tick.saturating_sub(last.copied().unwrap_or(0)) as f32;
                let mut changed: Vec<(f32, &EntityState)> = world
                    .values()
                    .filter(|e| near_ent(e) && base.get(&e.id) != Some(*e))
                    .map(|e| {
                        let d = eye.map(|o| o.distance(e.pos)).unwrap_or(0.0);
                        (d / (1.0 + age(slot.last_sent_entity.get(&e.id))), e)
                    })
                    .collect();
                changed.sort_by(|a, b| a.0.total_cmp(&b.0).then(a.1.id.cmp(&b.1.id)));
                let mut near_players: Vec<(f32, &PlayerSnap)> = players
                    .iter()
                    .filter(|p| p.id != slot.player && near(p.origin, None))
                    .map(|p| {
                        let d = eye.map(|o| o.distance(p.origin)).unwrap_or(0.0);
                        (d / (1.0 + age(slot.last_sent_player.get(&p.id))), p)
                    })
                    .collect();
                near_players.sort_by(|a, b| a.0.total_cmp(&b.0).then(a.1.id.cmp(&b.1.id)));
                let mut used = 0usize;
                let mut players: Vec<PlayerSnap> = players
                    .iter()
                    .filter(|p| p.id == slot.player)
                    .cloned()
                    .collect();
                // other players take at most half the budget so entities never starve
                for (_, p) in near_players {
                    if used + PLAYER_SNAP_SIZE > self.snapshot_budget / 2 {
                        break;
                    }
                    used += PLAYER_SNAP_SIZE;
                    slot.last_sent_player.insert(p.id, tick);
                    players.push(p.clone());
                }
                let mut entities: Vec<EntityState> = Vec::new();
                for (_, e) in changed {
                    let size = ENTITY_SNAP_SIZE;
                    if used + size > self.snapshot_budget && !entities.is_empty() {
                        break;
                    }
                    used += size;
                    slot.last_sent_entity.insert(e.id, tick);
                    entities.push(e.clone());
                }
                let removed: Vec<u32> = base
                    .keys()
                    .filter(|id| !world.get(id).is_some_and(near_ent))
                    .copied()
                    .collect();
                let mut sent_world = base.clone();
                for id in &removed {
                    sent_world.remove(id);
                }
                for e in &entities {
                    sent_world.insert(e.id, e.clone());
                }
                let undos: Vec<UndoEntry> = server
                    .sandbox
                    .undos(slot.player)
                    .iter()
                    .map(|u| UndoEntry {
                        id: u.id,
                        name: u.name.clone(),
                    })
                    .collect();
                if undos != slot.sent_undos {
                    slot.undo_seq += 1;
                    let mut w = Writer::default();
                    ServerMsg::Undos {
                        seq: slot.undo_seq,
                        entries: undos.clone(),
                    }
                    .write(&mut w);
                    slot.channel.send_reliable(w.buf);
                    slot.sent_undos = undos;
                }
                let own = OwnSandbox {
                    count: server.sandbox.count(slot.player) as u32,
                    constraints: server.sandbox.constraints().len() as u32,
                    undos: Vec::new(),
                    max_props: server.sandbox.limits.max_props,
                };
                let local = server
                    .players
                    .get(&slot.player)
                    .map(|p| LocalPlayer { state: p.state });
                let mut w = Writer::default();
                ServerMsg::Snapshot {
                    tick: self.tick,
                    baseline: baseline_tick,
                    last_cmd_tick: slot.last_cmd_tick,
                    players,
                    entities,
                    removed,
                    own,
                    local,
                }
                .write(&mut w);
                unreliable.push(w.buf);
                slot.sent.push_back((self.tick, sent_world));
                if slot.sent.len() > 64 {
                    slot.sent.pop_front();
                }
                if !sounds.is_empty() {
                    let mut w = Writer::default();
                    ServerMsg::Sounds {
                        tick: self.tick,
                        list: sounds.clone(),
                    }
                    .write(&mut w);
                    unreliable.push(w.buf);
                }
            }
            if let Some((i, offset)) = slot.uploads.front_mut() {
                if slot.channel.pending_reliable() < UPLOAD_WINDOW {
                    let a = &self.addons[*i];
                    let total = a.bytes.len() as u64;
                    let end = (*offset as usize + ADDON_CHUNK).min(a.bytes.len());
                    let mut w = Writer::default();
                    ServerMsg::AddonChunk {
                        sha: a.info.sha,
                        offset: *offset,
                        total,
                        data: a.bytes[*offset as usize..end].to_vec(),
                    }
                    .write(&mut w);
                    slot.channel.send_reliable(w.buf);
                    *offset = end as u64;
                    if end == a.bytes.len() {
                        slot.uploads.pop_front();
                    }
                }
            }
            if !unreliable.is_empty() || slot.channel.pending_reliable() > 0 {
                for packet in slot.channel.build_burst(&unreliable) {
                    let packet = match &mut slot.crypto {
                        Some(c) if c.peer_sealed => c.seal(&packet),
                        _ => crate::crypto::plain(packet),
                    };
                    transport.send(endpoint, packet);
                }
            }
        }
    }

    pub fn stats(&self, endpoint: usize) -> Option<&crate::channel::Stats> {
        self.clients.get(&endpoint).map(|c| &c.channel.stats)
    }
}

fn addon_list_of(addons: &[PublishedAddon]) -> Vec<crate::msg::AddonInfo> {
    let mut latest: BTreeMap<&str, &crate::msg::AddonInfo> = BTreeMap::new();
    for a in addons {
        latest.insert(a.info.id.as_str(), &a.info);
    }
    latest.into_values().cloned().collect()
}

/// `Welcome`, the published addon list and the replicated cvars: the client is on the server.
fn welcome(
    slot: &mut ClientSlot,
    map: &str,
    tick: u32,
    addons: &[PublishedAddon],
    server: &gm_server::Server<'_>,
    log: &mut Vec<String>,
) {
    slot.welcomed = true;
    log.push(format!("{} joined as player {}", slot.name, slot.player));
    let mut w = Writer::default();
    ServerMsg::Welcome {
        player_id: slot.player,
        tick_interval: gm_core::tick::DEFAULT_TICK_INTERVAL,
        map: map.to_string(),
        tick,
    }
    .write(&mut w);
    slot.channel.send_reliable(w.buf);
    if !addons.is_empty() {
        let mut w = Writer::default();
        ServerMsg::Addons {
            list: addon_list_of(addons),
        }
        .write(&mut w);
        slot.channel.send_reliable(w.buf);
    }
    for (n, c) in &server.cvars {
        if c.replicated {
            slot.pending_cvars.push((n.clone(), c.value.clone()));
        }
    }
}

/// A reliable `Disconnect` and the slot closes once it is acked (or at the timeout).
fn refuse(slot: &mut ClientSlot, reason: &str) {
    let mut w = Writer::default();
    ServerMsg::Disconnect {
        reason: reason.to_string(),
    }
    .write(&mut w);
    slot.channel.send_reliable(w.buf);
    slot.closing = true;
}

fn hex(b: &[u8; 32]) -> String {
    b.iter().map(|x| format!("{x:02x}")).collect()
}

fn parse_hex(hex: &str) -> Option<[u8; 32]> {
    if hex.len() != 64 {
        return None;
    }
    let mut out = [0u8; 32];
    for (i, o) in out.iter_mut().enumerate() {
        *o = u8::from_str_radix(&hex[2 * i..2 * i + 2], 16).ok()?;
    }
    Some(out)
}
