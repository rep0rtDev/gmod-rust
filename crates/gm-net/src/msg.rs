//! Messages. A packet carries any number of them; reliable ones are resent until acked.

use crate::wire::{Reader, Truncated, Writer};
use glam::Vec3;

/// One usercmd as sent by the client (66 Hz, the last 8 in every packet).
#[derive(Clone, Debug, Default, PartialEq)]
pub struct NetCmd {
    pub tick: u32,
    pub buttons: u32,
    pub view_angles: Vec3,
    pub forward_move: f32,
    pub side_move: f32,
    pub up_move: f32,
    pub mouse_x: f32,
    pub mouse_y: f32,
    /// Active weapon: 0 none, 1 physgun, 2 tool gun.
    pub weapon: u8,
}

/// Sandbox commands (reliable): console commands of the real game.
#[derive(Clone, Debug, PartialEq)]
pub enum Command {
    Spawn {
        model: String,
    },
    Undo,
    Cleanup,
    ToolClick {
        tool: String,
        right: bool,
        reload: bool,
    },
    Cvar {
        name: String,
        value: String,
    },
    /// Admin: hot reload of a loaded addon by id (its files are re-read on the server).
    Reload {
        addon: String,
    },
}

#[derive(Clone, Debug, PartialEq)]
pub enum ClientMsg {
    Hello {
        name: String,
        protocol: u16,
    },
    Cmds {
        cmds: Vec<NetCmd>,
    },
    Command {
        seq: u32,
        cmd: Command,
    },
    /// The newest snapshot tick the client holds (the delta baseline for the server).
    AckSnapshot {
        tick: u32,
    },
}

#[derive(Clone, Debug, Default, PartialEq)]
pub struct EntityState {
    pub id: u32,
    pub model: String,
    pub pos: Vec3,
    pub angles: Vec3,
    /// Source units per second and body-local degrees per second (for interpolation and the
    /// client's extrapolation when a snapshot is late).
    pub vel: Vec3,
    pub angvel: Vec3,
    pub frozen: bool,
    pub owner: u32,
}

#[derive(Clone, Debug, PartialEq)]
pub struct PlayerSnap {
    pub id: u32,
    pub origin: Vec3,
    pub velocity: Vec3,
    pub view_angles: Vec3,
    pub ducked: bool,
    pub held: Option<u32>,
    pub alive: bool,
    pub health: i16,
    pub armor: i16,
    /// Weapons carried as a bit per entry of `gm_server::LOADOUT` (the weapon registry for
    /// now; a string table comes with scripted weapons).
    pub weapons: u16,
    /// Index into `gm_server::LOADOUT` of the weapon out, 255 for none.
    pub active: u8,
}

/// Weapon class names to the registry bitmask and back.
pub fn weapon_mask(names: &[String]) -> u16 {
    let mut m = 0u16;
    for (i, w) in gm_server::LOADOUT.iter().enumerate() {
        if names.iter().any(|n| n == w) {
            m |= 1 << i;
        }
    }
    m
}

pub fn weapon_names(mask: u16) -> Vec<String> {
    gm_server::LOADOUT
        .iter()
        .enumerate()
        .filter(|(i, _)| mask & (1 << i) != 0)
        .map(|(_, w)| w.to_string())
        .collect()
}

pub fn weapon_index(name: &str) -> u8 {
    gm_server::LOADOUT
        .iter()
        .position(|w| *w == name)
        .map(|i| i as u8)
        .unwrap_or(255)
}

#[derive(Clone, Debug, PartialEq)]
pub struct UndoEntry {
    pub id: u32,
    pub name: String,
}

/// The receiving client's own movement state after its last run usercmd, for reconciliation
/// (all of `gm_move::PlayerState`, so a replay from it is bit-exact).
#[derive(Clone, Debug, PartialEq)]
pub struct LocalPlayer {
    pub state: gm_move::PlayerState,
}

/// What the receiving client's own sandbox looks like (count, undo stack): per client.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct OwnSandbox {
    pub count: u32,
    pub constraints: u32,
    pub undos: Vec<UndoEntry>,
    pub max_props: i32,
}

#[derive(Clone, Debug, PartialEq)]
pub enum ServerMsg {
    Welcome {
        player_id: u32,
        tick_interval: f32,
        map: String,
        tick: u32,
    },
    /// Delta against `baseline` (0 = full); entities changed since, ids removed since.
    Snapshot {
        tick: u32,
        baseline: u32,
        last_cmd_tick: u32,
        players: Vec<PlayerSnap>,
        entities: Vec<EntityState>,
        removed: Vec<u32>,
        own: OwnSandbox,
        local: Option<LocalPlayer>,
    },
    /// Reliable: the server tells the client about a sandbox event (LimitHit etc.).
    Event {
        seq: u32,
        text: String,
    },
    CommandAck {
        seq: u32,
    },
}

pub const PROTOCOL: u16 = 1;

/// Usercmds are coded as deltas against the previous one in the batch (the first against the
/// default): a change mask byte, then only the changed fields. Ticks are consecutive, so only the
/// first tick is sent.
const CMD_BUTTONS: u8 = 1;
const CMD_VIEW: u8 = 2;
const CMD_FORWARD: u8 = 4;
const CMD_SIDE: u8 = 8;
const CMD_UP: u8 = 16;
const CMD_MOUSE: u8 = 32;
const CMD_WEAPON: u8 = 64;

fn w_cmd(w: &mut Writer, c: &NetCmd, prev: &NetCmd) {
    let mut mask = 0u8;
    if c.buttons != prev.buttons {
        mask |= CMD_BUTTONS;
    }
    if c.view_angles != prev.view_angles {
        mask |= CMD_VIEW;
    }
    if c.forward_move != prev.forward_move {
        mask |= CMD_FORWARD;
    }
    if c.side_move != prev.side_move {
        mask |= CMD_SIDE;
    }
    if c.up_move != prev.up_move {
        mask |= CMD_UP;
    }
    if c.mouse_x != prev.mouse_x || c.mouse_y != prev.mouse_y {
        mask |= CMD_MOUSE;
    }
    if c.weapon != prev.weapon {
        mask |= CMD_WEAPON;
    }
    w.u8(mask);
    if mask & CMD_BUTTONS != 0 {
        w.varint(c.buttons as u64);
    }
    if mask & CMD_VIEW != 0 {
        w.vec3(c.view_angles);
    }
    if mask & CMD_FORWARD != 0 {
        w.f32(c.forward_move);
    }
    if mask & CMD_SIDE != 0 {
        w.f32(c.side_move);
    }
    if mask & CMD_UP != 0 {
        w.f32(c.up_move);
    }
    if mask & CMD_MOUSE != 0 {
        w.f32(c.mouse_x);
        w.f32(c.mouse_y);
    }
    if mask & CMD_WEAPON != 0 {
        w.u8(c.weapon);
    }
}

fn r_cmd(r: &mut Reader, prev: &NetCmd, tick: u32) -> Result<NetCmd, Truncated> {
    let mask = r.u8()?;
    let mut c = prev.clone();
    c.tick = tick;
    if mask & CMD_BUTTONS != 0 {
        c.buttons = r.varint()? as u32;
    }
    if mask & CMD_VIEW != 0 {
        c.view_angles = r.vec3()?;
    }
    if mask & CMD_FORWARD != 0 {
        c.forward_move = r.f32()?;
    }
    if mask & CMD_SIDE != 0 {
        c.side_move = r.f32()?;
    }
    if mask & CMD_UP != 0 {
        c.up_move = r.f32()?;
    }
    if mask & CMD_MOUSE != 0 {
        c.mouse_x = r.f32()?;
        c.mouse_y = r.f32()?;
    }
    if mask & CMD_WEAPON != 0 {
        c.weapon = r.u8()?;
    }
    Ok(c)
}

fn w_state(w: &mut Writer, s: &gm_move::PlayerState) {
    w.vec3(s.origin);
    w.vec3(s.velocity);
    w.bool(s.on_ground);
    w.vec3(s.ground_normal);
    w.bool(s.ducked);
    w.bool(s.ducking);
    w.bool(s.unducking);
    w.f32(s.duck_timer);
    w.bool(s.jump_held);
    w.bool(s.boost_pending);
    w.f32(s.boost_forward);
    w.f32(s.boost_yaw);
    w.bool(s.ducked_at_tick_start);
    w.f32(s.surface_friction);
    w.u8(match s.move_type {
        gm_move::MoveType::Walk => 0,
        gm_move::MoveType::Noclip => 1,
    });
}

fn r_state(r: &mut Reader) -> Result<gm_move::PlayerState, Truncated> {
    Ok(gm_move::PlayerState {
        origin: r.vec3()?,
        velocity: r.vec3()?,
        on_ground: r.bool()?,
        ground_normal: r.vec3()?,
        ducked: r.bool()?,
        ducking: r.bool()?,
        unducking: r.bool()?,
        duck_timer: r.f32()?,
        jump_held: r.bool()?,
        boost_pending: r.bool()?,
        boost_forward: r.f32()?,
        boost_yaw: r.f32()?,
        ducked_at_tick_start: r.bool()?,
        surface_friction: r.f32()?,
        move_type: if r.u8()? == 0 {
            gm_move::MoveType::Walk
        } else {
            gm_move::MoveType::Noclip
        },
    })
}

impl ClientMsg {
    pub fn reliable(&self) -> bool {
        matches!(self, ClientMsg::Hello { .. } | ClientMsg::Command { .. })
    }

    pub fn write(&self, w: &mut Writer) {
        match self {
            ClientMsg::Hello { name, protocol } => {
                w.u8(1);
                w.str(name);
                w.u16(*protocol);
            }
            ClientMsg::Cmds { cmds } => {
                w.u8(2);
                w.u8(cmds.len() as u8);
                w.u32(cmds.first().map(|c| c.tick).unwrap_or(0));
                let mut prev = NetCmd::default();
                for c in cmds {
                    w_cmd(w, c, &prev);
                    prev = c.clone();
                }
            }
            ClientMsg::Command { seq, cmd } => {
                w.u8(3);
                w.u32(*seq);
                match cmd {
                    Command::Spawn { model } => {
                        w.u8(1);
                        w.str(model);
                    }
                    Command::Undo => w.u8(2),
                    Command::Cleanup => w.u8(3),
                    Command::ToolClick {
                        tool,
                        right,
                        reload,
                    } => {
                        w.u8(4);
                        w.str(tool);
                        w.bool(*right);
                        w.bool(*reload);
                    }
                    Command::Cvar { name, value } => {
                        w.u8(5);
                        w.str(name);
                        w.str(value);
                    }
                    Command::Reload { addon } => {
                        w.u8(6);
                        w.str(addon);
                    }
                }
            }
            ClientMsg::AckSnapshot { tick } => {
                w.u8(4);
                w.u32(*tick);
            }
        }
    }

    pub fn read(r: &mut Reader) -> Result<Self, Truncated> {
        Ok(match r.u8()? {
            1 => ClientMsg::Hello {
                name: r.str()?,
                protocol: r.u16()?,
            },
            2 => {
                let n = r.u8()? as usize;
                let first = r.u32()?;
                let mut cmds = Vec::with_capacity(n);
                let mut prev = NetCmd::default();
                for i in 0..n {
                    let c = r_cmd(r, &prev, first + i as u32)?;
                    prev = c.clone();
                    cmds.push(c);
                }
                ClientMsg::Cmds { cmds }
            }
            3 => {
                let seq = r.u32()?;
                let cmd = match r.u8()? {
                    1 => Command::Spawn { model: r.str()? },
                    2 => Command::Undo,
                    3 => Command::Cleanup,
                    4 => Command::ToolClick {
                        tool: r.str()?,
                        right: r.bool()?,
                        reload: r.bool()?,
                    },
                    5 => Command::Cvar {
                        name: r.str()?,
                        value: r.str()?,
                    },
                    6 => Command::Reload { addon: r.str()? },
                    _ => return Err(Truncated),
                };
                ClientMsg::Command { seq, cmd }
            }
            4 => ClientMsg::AckSnapshot { tick: r.u32()? },
            _ => return Err(Truncated),
        })
    }
}

impl ServerMsg {
    pub fn reliable(&self) -> bool {
        matches!(
            self,
            ServerMsg::Welcome { .. } | ServerMsg::Event { .. } | ServerMsg::CommandAck { .. }
        )
    }

    pub fn write(&self, w: &mut Writer) {
        match self {
            ServerMsg::Welcome {
                player_id,
                tick_interval,
                map,
                tick,
            } => {
                w.u8(1);
                w.u32(*player_id);
                w.f32(*tick_interval);
                w.str(map);
                w.u32(*tick);
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
                w.u8(2);
                w.u32(*tick);
                w.u32(*baseline);
                w.u32(*last_cmd_tick);
                w.varint(players.len() as u64);
                for p in players {
                    w.u32(p.id);
                    w.vec3(p.origin);
                    w.vec3(p.velocity);
                    w.vec3(p.view_angles);
                    w.bool(p.ducked);
                    w.u32(p.held.unwrap_or(0));
                    w.bool(p.alive);
                    w.i16(p.health);
                    w.i16(p.armor);
                    w.u16(p.weapons);
                    w.u8(p.active);
                }
                w.varint(entities.len() as u64);
                for e in entities {
                    w.u32(e.id);
                    w.str(&e.model);
                    w.vec3(e.pos);
                    w.vec3(e.angles);
                    w.vec3(e.vel);
                    w.vec3(e.angvel);
                    w.bool(e.frozen);
                    w.u32(e.owner);
                }
                w.varint(removed.len() as u64);
                for id in removed {
                    w.u32(*id);
                }
                w.u32(own.count);
                w.u32(own.constraints);
                w.u32(own.max_props as u32);
                w.varint(own.undos.len() as u64);
                for u in &own.undos {
                    w.u32(u.id);
                    w.str(&u.name);
                }
                match local {
                    None => w.u8(0),
                    Some(l) => {
                        w.u8(1);
                        w_state(w, &l.state);
                    }
                }
            }
            ServerMsg::Event { seq, text } => {
                w.u8(3);
                w.u32(*seq);
                w.str(text);
            }
            ServerMsg::CommandAck { seq } => {
                w.u8(4);
                w.u32(*seq);
            }
        }
    }

    pub fn read(r: &mut Reader) -> Result<Self, Truncated> {
        Ok(match r.u8()? {
            1 => ServerMsg::Welcome {
                player_id: r.u32()?,
                tick_interval: r.f32()?,
                map: r.str()?,
                tick: r.u32()?,
            },
            2 => {
                let tick = r.u32()?;
                let baseline = r.u32()?;
                let last_cmd_tick = r.u32()?;
                let n = r.varint()? as usize;
                let mut players = Vec::with_capacity(n);
                for _ in 0..n {
                    players.push(PlayerSnap {
                        id: r.u32()?,
                        origin: r.vec3()?,
                        velocity: r.vec3()?,
                        view_angles: r.vec3()?,
                        ducked: r.bool()?,
                        held: match r.u32()? {
                            0 => None,
                            h => Some(h),
                        },
                        alive: r.bool()?,
                        health: r.i16()?,
                        armor: r.i16()?,
                        weapons: r.u16()?,
                        active: r.u8()?,
                    });
                }
                let n = r.varint()? as usize;
                let mut entities = Vec::with_capacity(n);
                for _ in 0..n {
                    entities.push(EntityState {
                        id: r.u32()?,
                        model: r.str()?,
                        pos: r.vec3()?,
                        angles: r.vec3()?,
                        vel: r.vec3()?,
                        angvel: r.vec3()?,
                        frozen: r.bool()?,
                        owner: r.u32()?,
                    });
                }
                let n = r.varint()? as usize;
                let mut removed = Vec::with_capacity(n);
                for _ in 0..n {
                    removed.push(r.u32()?);
                }
                let count = r.u32()?;
                let constraints = r.u32()?;
                let max_props = r.u32()? as i32;
                let n = r.varint()? as usize;
                let mut undos = Vec::with_capacity(n);
                for _ in 0..n {
                    undos.push(UndoEntry {
                        id: r.u32()?,
                        name: r.str()?,
                    });
                }
                let local = if r.u8()? != 0 {
                    Some(LocalPlayer { state: r_state(r)? })
                } else {
                    None
                };
                ServerMsg::Snapshot {
                    tick,
                    baseline,
                    last_cmd_tick,
                    players,
                    entities,
                    removed,
                    own: OwnSandbox {
                        count,
                        constraints,
                        undos,
                        max_props,
                    },
                    local,
                }
            }
            3 => ServerMsg::Event {
                seq: r.u32()?,
                text: r.str()?,
            },
            4 => ServerMsg::CommandAck { seq: r.u32()? },
            _ => return Err(Truncated),
        })
    }
}
