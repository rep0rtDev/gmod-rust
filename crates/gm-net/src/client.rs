//! A headless client: sends the last 8 usercmds every tick and reliable sandbox commands,
//! rebuilds the world from delta snapshots.

use crate::channel::Channel;
use crate::msg::{
    ClientMsg, Command, EntityState, NetCmd, OwnSandbox, PlayerSnap, ServerMsg, PROTOCOL,
};
use crate::transport::Transport;
use crate::wire::{Reader, Writer};
use std::collections::{BTreeMap, VecDeque};

#[derive(Default)]
pub struct ClientWorld {
    pub tick: u32,
    pub players: Vec<PlayerSnap>,
    pub entities: BTreeMap<u32, EntityState>,
    pub own: OwnSandbox,
    pub last_cmd_tick: u32,
}

pub struct ClientNet {
    pub endpoint: usize,
    channel: Channel,
    pub player_id: Option<u32>,
    pub server_tick: u32,
    pub tick_interval: f32,
    pub map: String,
    cmd_tick: u32,
    recent: VecDeque<NetCmd>,
    next_command: u32,
    pub acked_commands: u32,
    pub world: ClientWorld,
    /// snapshots kept as possible baselines: tick -> entities
    history: VecDeque<(u32, BTreeMap<u32, EntityState>)>,
    pub events: Vec<String>,
    pub dropped_snapshots: u64,
}

impl ClientNet {
    pub fn new(endpoint: usize, name: &str) -> Self {
        let mut c = ClientNet {
            endpoint,
            channel: Channel::default(),
            player_id: None,
            server_tick: 0,
            tick_interval: gm_core::tick::DEFAULT_TICK_INTERVAL,
            map: String::new(),
            cmd_tick: 0,
            recent: VecDeque::new(),
            next_command: 0,
            acked_commands: 0,
            world: ClientWorld::default(),
            history: VecDeque::new(),
            events: Vec::new(),
            dropped_snapshots: 0,
        };
        let mut w = Writer::default();
        ClientMsg::Hello {
            name: name.to_string(),
            protocol: PROTOCOL,
        }
        .write(&mut w);
        c.channel.send_reliable(w.buf);
        c
    }

    /// Queues a sandbox command (reliable, in order).
    pub fn command(&mut self, cmd: Command) {
        let seq = self.next_command;
        self.next_command += 1;
        let mut w = Writer::default();
        ClientMsg::Command { seq, cmd }.write(&mut w);
        self.channel.send_reliable(w.buf);
    }

    /// One client tick: records the usercmd and sends a packet with the last 8 plus acks.
    pub fn tick(&mut self, mut cmd: NetCmd, transport: &mut dyn Transport) {
        self.cmd_tick += 1;
        cmd.tick = self.cmd_tick;
        self.recent.push_back(cmd);
        while self.recent.len() > 8 {
            self.recent.pop_front();
        }
        let mut unreliable = Vec::new();
        let mut w = Writer::default();
        ClientMsg::Cmds {
            cmds: self.recent.iter().cloned().collect(),
        }
        .write(&mut w);
        unreliable.push(w.buf);
        if self.world.tick > 0 {
            let mut w = Writer::default();
            ClientMsg::AckSnapshot {
                tick: self.world.tick,
            }
            .write(&mut w);
            unreliable.push(w.buf);
        }
        let mut packet = vec![self.endpoint as u8];
        packet.extend(self.channel.build(&unreliable));
        transport.send(0, packet);
    }

    /// Applies everything that arrived.
    pub fn receive(&mut self, transport: &mut dyn Transport) {
        for packet in transport.recv(self.endpoint) {
            let Ok(msgs) = self.channel.receive(&packet[1..]) else {
                continue;
            };
            for m in msgs {
                let Ok(msg) = ServerMsg::read(&mut Reader::new(&m)) else {
                    continue;
                };
                match msg {
                    ServerMsg::Welcome {
                        player_id,
                        tick_interval,
                        map,
                        tick,
                    } => {
                        self.player_id = Some(player_id);
                        self.tick_interval = tick_interval;
                        self.map = map;
                        self.server_tick = tick;
                    }
                    ServerMsg::Snapshot {
                        tick,
                        baseline,
                        last_cmd_tick,
                        players,
                        entities,
                        removed,
                        own,
                    } => {
                        if tick <= self.world.tick {
                            continue;
                        }
                        let mut world = if baseline == 0 {
                            BTreeMap::new()
                        } else if let Some((_, e)) =
                            self.history.iter().find(|(t, _)| *t == baseline)
                        {
                            e.clone()
                        } else {
                            // baseline lost: wait for a snapshot against one we have
                            self.dropped_snapshots += 1;
                            continue;
                        };
                        for id in removed {
                            world.remove(&id);
                        }
                        for e in entities {
                            world.insert(e.id, e);
                        }
                        self.history.push_back((tick, world.clone()));
                        if self.history.len() > 64 {
                            self.history.pop_front();
                        }
                        self.world = ClientWorld {
                            tick,
                            players,
                            entities: world,
                            own,
                            last_cmd_tick,
                        };
                        self.server_tick = tick;
                    }
                    ServerMsg::Event { text, .. } => self.events.push(text),
                    ServerMsg::CommandAck { seq } => {
                        self.acked_commands = self.acked_commands.max(seq + 1);
                    }
                }
            }
        }
    }

    pub fn stats(&self) -> &crate::channel::Stats {
        &self.channel.stats
    }
}
