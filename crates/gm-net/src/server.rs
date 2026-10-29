//! The server side: clients, their usercmd buffers and reliable commands, snapshots at
//! `snapshot_every` ticks as deltas against the newest snapshot the client acked.

use crate::channel::Channel;
use crate::msg::{
    ClientMsg, Command, EntityState, LocalPlayer, OwnSandbox, PlayerSnap, ServerMsg, UndoEntry,
    PROTOCOL,
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
}

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
        }
    }

    /// Reads everything the transport has for the server (endpoint 0), connecting new clients.
    pub fn receive(&mut self, server: &mut Server<'_>, transport: &mut dyn Transport) {
        for (from, packet) in transport.recv() {
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
                            let mut w = Writer::default();
                            ServerMsg::Welcome {
                                player_id: slot.player,
                                tick_interval: gm_core::tick::DEFAULT_TICK_INTERVAL,
                                map: self.map.clone(),
                                tick: self.tick,
                            }
                            .write(&mut w);
                            slot.channel.send_reliable(w.buf);
                        }
                    }
                    ClientMsg::Cmds { cmds } => {
                        for c in cmds {
                            if c.tick > slot.last_cmd_tick || slot.last_cmd_tick == 0 {
                                slot.cmds.entry(c.tick).or_insert(PlayerCmd {
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
                        if name == "sbox_maxprops" {
                            server.sandbox.limits.max_props = value.parse().unwrap_or(200);
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
                for e in &server.events[n..] {
                    if let gm_sandbox::Event::LimitHit { limit } = e {
                        slot.pending_events.push(format!("LimitHit {limit}"));
                    }
                }
            }
        }
    }

    /// Sends this tick's packets: a snapshot every `snapshot_every` ticks plus pending reliables.
    pub fn send(&mut self, server: &Server<'_>, transport: &mut dyn Transport) {
        let snapshot_tick = self.tick.is_multiple_of(self.snapshot_every);
        let world: BTreeMap<u32, EntityState> = server
            .sandbox
            .props()
            .iter()
            .map(|p| {
                let st = server.prop_state(p.id);
                (
                    p.id,
                    EntityState {
                        id: p.id,
                        model: p.model.clone(),
                        pos: p.pos,
                        angles: p.angles,
                        vel: st.map(|s| s.velocity).unwrap_or_default(),
                        angvel: st.map(|s| s.angular_velocity).unwrap_or_default(),
                        frozen: p.frozen,
                        owner: p.owner,
                    },
                )
            })
            .collect();
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
            let mut unreliable = Vec::new();
            if snapshot_tick {
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
                let entities: Vec<EntityState> = world
                    .values()
                    .filter(|e| base.get(&e.id) != Some(*e))
                    .cloned()
                    .collect();
                let removed: Vec<u32> = base
                    .keys()
                    .filter(|id| !world.contains_key(id))
                    .copied()
                    .collect();
                let own = OwnSandbox {
                    count: server.sandbox.count(slot.player) as u32,
                    constraints: server.sandbox.constraints().len() as u32,
                    undos: server
                        .sandbox
                        .undos(slot.player)
                        .iter()
                        .map(|u| UndoEntry {
                            id: u.id,
                            name: u.name.clone(),
                        })
                        .collect(),
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
                    players: players.clone(),
                    entities,
                    removed,
                    own,
                    local,
                }
                .write(&mut w);
                unreliable.push(w.buf);
                slot.sent.push_back((self.tick, world.clone()));
                if slot.sent.len() > 64 {
                    slot.sent.pop_front();
                }
            }
            if !unreliable.is_empty() || slot.channel.pending_reliable() > 0 {
                let packet = slot.channel.build(&unreliable);
                transport.send(endpoint, packet);
            }
        }
    }

    pub fn stats(&self, endpoint: usize) -> Option<&crate::channel::Stats> {
        self.clients.get(&endpoint).map(|c| &c.channel.stats)
    }
}
