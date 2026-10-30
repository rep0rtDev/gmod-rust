//! A headless client: sends the last 8 usercmds every tick and reliable sandbox commands,
//! rebuilds the world from delta snapshots.

use crate::channel::Channel;
use crate::msg::{
    ClientMsg, Command, EntityState, LocalPlayer, NetCmd, OwnSandbox, PlayerSnap, ServerMsg,
    PROTOCOL,
};
use crate::transport::Transport;
use crate::wire::{Reader, Writer};
use glam::Vec3;
use std::collections::{BTreeMap, VecDeque};

#[derive(Default)]
pub struct ClientWorld {
    pub tick: u32,
    pub players: Vec<PlayerSnap>,
    pub entities: BTreeMap<u32, EntityState>,
    pub own: OwnSandbox,
    pub last_cmd_tick: u32,
    pub local: Option<LocalPlayer>,
    /// Replicated console variables as the server sent them.
    pub cvars: BTreeMap<String, String>,
}

/// Interpolation of other players' and props' poses between the two snapshots around a render
/// time `interp_ticks` behind the newest snapshot (two snapshot intervals by default, 60 ms at
/// 33 Hz); angles go through quaternion slerp. Ahead of the newest snapshot (a late one) the pose
/// is extrapolated from the snapshot's velocity.
pub struct Interpolator {
    pub interp_ticks: f32,
    snapshots: VecDeque<(u32, BTreeMap<u32, EntityState>)>,
}

#[derive(Clone, Debug, PartialEq)]
pub struct InterpPose {
    pub pos: Vec3,
    pub angles: Vec3,
    pub vel: Vec3,
    pub angvel: Vec3,
    pub extrapolated: bool,
}

impl Interpolator {
    pub fn new(interp_ticks: f32) -> Self {
        Interpolator {
            interp_ticks,
            snapshots: VecDeque::new(),
        }
    }

    /// Call with every applied snapshot (the client world after it).
    pub fn push(&mut self, tick: u32, entities: &BTreeMap<u32, EntityState>) {
        if self.snapshots.back().is_some_and(|(t, _)| *t >= tick) {
            return;
        }
        self.snapshots.push_back((tick, entities.clone()));
        while self.snapshots.len() > 32 {
            self.snapshots.pop_front();
        }
    }

    /// The render time for the client's tick `now`.
    pub fn render_tick(&self, now: u32) -> f32 {
        now as f32 - self.interp_ticks
    }

    pub fn sample(&self, id: u32, render_tick: f32) -> Option<InterpPose> {
        let newest = self.snapshots.back()?;
        if render_tick > newest.0 as f32 {
            let e = newest.1.get(&id)?;
            let dt = (render_tick - newest.0 as f32) * gm_core::tick::DEFAULT_TICK_INTERVAL;
            return Some(InterpPose {
                pos: e.pos + e.vel * dt,
                angles: e.angles,
                vel: e.vel,
                angvel: e.angvel,
                extrapolated: true,
            });
        }
        let after = self
            .snapshots
            .iter()
            .position(|(t, _)| *t as f32 >= render_tick)?;
        let (tb, b) = &self.snapshots[after];
        let eb = b.get(&id)?;
        if after == 0 {
            return Some(InterpPose {
                pos: eb.pos,
                angles: eb.angles,
                vel: eb.vel,
                angvel: eb.angvel,
                extrapolated: false,
            });
        }
        let (ta, a) = &self.snapshots[after - 1];
        let Some(ea) = a.get(&id) else {
            return Some(InterpPose {
                pos: eb.pos,
                angles: eb.angles,
                vel: eb.vel,
                angvel: eb.angvel,
                extrapolated: false,
            });
        };
        let f = ((render_tick - *ta as f32) / (*tb as f32 - *ta as f32).max(1.0)).clamp(0.0, 1.0);
        let rot = gm_sandbox::rotation(ea.angles).slerp(gm_sandbox::rotation(eb.angles), f);
        Some(InterpPose {
            pos: ea.pos.lerp(eb.pos, f),
            angles: gm_sandbox::angles_of(rot),
            vel: ea.vel.lerp(eb.vel, f),
            angvel: ea.angvel.lerp(eb.angvel, f),
            extrapolated: false,
        })
    }
}

/// Client-side prediction of the local player: every usercmd is run through `gm-move` at once;
/// when a snapshot brings the server's state after usercmd `n`, the prediction recorded for `n`
/// is compared and, if it differs, replaced and the later usercmds replayed from it.
pub struct Prediction<'w> {
    mover: gm_move::Mover<'w>,
    pub state: gm_move::PlayerState,
    history: VecDeque<(u32, gm_move::UserCmd, gm_move::PlayerState)>,
    pub corrections: u64,
    pub max_correction: f32,
    pub reconciled_tick: u32,
}

impl<'w> Prediction<'w> {
    pub fn new(world: &'w gm_collision::World, state: gm_move::PlayerState) -> Self {
        Prediction {
            mover: gm_move::Mover::new(gm_move::MoveConfig::default(), world),
            state,
            history: VecDeque::new(),
            corrections: 0,
            max_correction: 0.0,
            reconciled_tick: 0,
        }
    }

    pub fn usercmd(c: &NetCmd) -> gm_move::UserCmd {
        gm_move::UserCmd {
            buttons: c.buttons & !(1 | 32 | 2048),
            view_angles: c.view_angles,
            forward_move: c.forward_move,
            side_move: c.side_move,
            up_move: c.up_move,
        }
    }

    /// Runs usercmd `tick` locally.
    pub fn predict(&mut self, tick: u32, cmd: &NetCmd) {
        let uc = Self::usercmd(cmd);
        self.mover.tick(&mut self.state, &uc);
        self.history.push_back((tick, uc, self.state));
        while self.history.len() > 128 {
            self.history.pop_front();
        }
    }

    /// The server's state after usercmd `last_cmd_tick` arrived.
    pub fn reconcile(&mut self, last_cmd_tick: u32, authoritative: &gm_move::PlayerState) {
        if last_cmd_tick <= self.reconciled_tick {
            return;
        }
        self.reconciled_tick = last_cmd_tick;
        let Some(pos) = self
            .history
            .iter()
            .position(|(t, _, _)| *t == last_cmd_tick)
        else {
            return;
        };
        let predicted = self.history[pos].2;
        if predicted == *authoritative {
            self.history.drain(..pos);
            return;
        }
        self.corrections += 1;
        self.max_correction = self
            .max_correction
            .max((predicted.origin - authoritative.origin).length());
        self.history[pos].2 = *authoritative;
        let mut st = *authoritative;
        for i in pos + 1..self.history.len() {
            let uc = self.history[i].1;
            self.mover.tick(&mut st, &uc);
            self.history[i].2 = st;
        }
        self.state = st;
        self.history.drain(..pos);
    }
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
    /// Addon messages received since the last drain (name, payload).
    pub addon_msgs: Vec<(String, Vec<u8>)>,
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
            addon_msgs: Vec::new(),
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

    /// Sends an addon message to the server's actors (reliable, in order).
    pub fn send_addon(&mut self, name: &str, data: &[u8]) {
        let mut w = Writer::default();
        ClientMsg::Addon {
            name: name.to_string(),
            data: data.to_vec(),
        }
        .write(&mut w);
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
        let packet = self.channel.build(&unreliable);
        transport.send(0, packet);
    }

    /// Applies everything that arrived.
    pub fn receive(&mut self, transport: &mut dyn Transport) {
        for (_, packet) in transport.recv() {
            let Ok(msgs) = self.channel.receive(&packet) else {
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
                        local,
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
                            local,
                            cvars: std::mem::take(&mut self.world.cvars),
                        };
                        self.server_tick = tick;
                    }
                    ServerMsg::Event { text, .. } => self.events.push(text),
                    ServerMsg::Addon { name, data, .. } => self.addon_msgs.push((name, data)),
                    ServerMsg::Cvar { name, value, .. } => {
                        self.world.cvars.insert(name, value);
                    }
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
