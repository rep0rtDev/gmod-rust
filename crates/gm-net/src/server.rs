//! The server side: clients, their usercmd buffers and reliable commands, snapshots at
//! `snapshot_every` ticks as deltas against the newest snapshot the client acked.

use crate::channel::Channel;
use crate::msg::{
    auth_message, ClientMsg, Command, EntityState, LocalPlayer, OwnSandbox, PlayerSnap, ServerMsg,
    UndoEntry, PROTOCOL,
};
use crate::transport::Transport;
use crate::wire::{Reader, Writer};
use glam::Vec3;
use gm_server::{PlayerCmd, Server};
use std::collections::{BTreeMap, VecDeque};

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
    /// Interest: a client gets entities and other players within this distance of its player
    /// (Source units); what leaves the radius is removed on the client, what enters is sent whole.
    pub interest_radius: f32,
    /// Bytes of entity states per snapshot: the nearest changed entities go first, the rest wait
    /// for the next snapshot (they stay different from the acked baseline until sent).
    pub snapshot_budget: usize,
}

impl ServerNet {
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
            addons: Vec::new(),
            models: Vec::new(),
            model_ids: BTreeMap::new(),
            use_pvs: true,
            require_auth: false,
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
            let body = &packet[..];
            if !self.clients.contains_key(&from) {
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
                    },
                );
                server.add_player(player, self.spawn_origin, Vec3::ZERO);
            }
            let slot = self.clients.get_mut(&from).unwrap();
            let Ok(msgs) = slot.channel.receive(body) else {
                continue;
            };
            for m in msgs {
                let Ok(msg) = ClientMsg::read(&mut Reader::new(&m)) else {
                    continue;
                };
                match msg {
                    ClientMsg::Hello { name, protocol } => {
                        if protocol == PROTOCOL {
                            slot.name = name;
                            let mut nonce = [0u8; 32];
                            rand_core::RngCore::fill_bytes(&mut rand_core::OsRng, &mut nonce);
                            slot.nonce = nonce;
                            let mut w = Writer::default();
                            ServerMsg::Challenge { nonce }.write(&mut w);
                            slot.channel.send_reliable(w.buf);
                            if !self.require_auth {
                                welcome(slot, &self.map, self.tick, &self.addons, server);
                            }
                        }
                    }
                    ClientMsg::Auth { pubkey, sig } => {
                        let ok = slot.nonce != [0; 32]
                            && ed25519_dalek::VerifyingKey::from_bytes(&pubkey).is_ok_and(|k| {
                                ed25519_dalek::Signature::from_slice(&sig).is_ok_and(|s| {
                                    k.verify_strict(&auth_message(&slot.nonce), &s).is_ok()
                                })
                            });
                        if ok {
                            slot.identity = Some(pubkey);
                            if let Some(p) = server.players.get_mut(&slot.player) {
                                p.identity = Some(pubkey);
                            }
                            if !slot.welcomed {
                                welcome(slot, &self.map, self.tick, &self.addons, server);
                            }
                        }
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
    pub fn tick(&mut self, server: &mut Server<'_>) {
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
        if let Some(h) = self.hooks.as_mut() {
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
                    Command::Cleanup => server.cleanup(slot.player),
                    Command::ToolClick {
                        tool,
                        right,
                        reload,
                    } => {
                        if reload {
                            server.sandbox.tool_reload(slot.player);
                        } else if let gm_server::ToolClick::Unknown(hit) =
                            server.tool_click_full(slot.player, &tool, right)
                        {
                            if let Some(h) = self.hooks.as_mut() {
                                h.tool_click(server, slot.player, &tool, &hit, right);
                            }
                        }
                    }
                    Command::Cvar { name, value } => {
                        server.set_cvar(&name, &value);
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
                held: p.held.map(|h| h.0),
                alive: p.alive,
                health: p.health.clamp(i16::MIN as i32, i16::MAX as i32) as i16,
                armor: p.armor.clamp(i16::MIN as i32, i16::MAX as i32) as i16,
                weapons: crate::msg::weapon_mask(&p.weapons),
                active: crate::msg::weapon_index(&p.active_weapon),
            })
            .collect();
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
) {
    slot.welcomed = true;
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
