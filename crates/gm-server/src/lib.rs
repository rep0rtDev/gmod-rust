//! One server tick: usercmds through `gm-move`, sandbox commands through `gm-sandbox`, and the
//! props and constraints the sandbox owns mirrored into the Jolt world of `gm-physics`
//! (weld -> fixed, rope -> distance, axis -> hinge). Nothing here is networked yet.

use glam::Vec3;
use gm_physics::physgun::{Grab, PhysgunParams};
use gm_physics::{BodyHandle, ConstraintHandle, PropDesc, Surface, World};
use gm_sandbox::{ConstraintKind, Event, ModelBounds, PropBody, Sandbox, SpawnRequest, ToolHit};
use std::collections::BTreeMap;

/// Everything the server needs to know about a model to spawn and trace it.
#[derive(Clone, Debug)]
pub struct ModelInfo {
    pub bounds: ModelBounds,
    /// PHY convexes in the entity frame (Source units).
    pub hulls: Vec<Vec<Vec3>>,
    /// The same convexes as outward planes (normal, distance) for the sandbox traces.
    pub hull_planes: Vec<Vec<(Vec3, f32)>>,
    pub mass: f32,
    pub volume: f32,
    pub surface: Surface,
    pub linear_damping: f32,
    pub angular_damping: f32,
    pub drag_axis_areas: Vec3,
    pub inertia: Option<Vec3>,
}

/// Game-logic hooks around server commands (the addon runtime implements them).
pub trait ServerHooks {
    /// `prop.spawn_request`: false refuses the spawn.
    fn prop_spawn_request(&mut self, server: &mut Server<'_>, player: u32, model: &str) -> bool;
    /// `entity.spawned` after a prop was created.
    fn entity_spawned(&mut self, server: &mut Server<'_>, player: u32, ent: u32);
    /// Around every tick.
    fn pre_tick(&mut self, server: &mut Server<'_>);
    fn post_tick(&mut self, server: &mut Server<'_>);
}

/// Loads a model by name on first use.
pub type ModelLoader = Box<dyn FnMut(&str) -> Option<ModelInfo>>;

pub struct Player {
    pub state: gm_move::PlayerState,
    pub view_angles: Vec3,
    /// The prop held by the physgun and how it was grabbed.
    pub held: Option<(u32, Grab)>,
}

/// `IN_ATTACK`, `IN_USE`, `IN_ATTACK2` of a usercmd.
pub const IN_ATTACK: u32 = 1;
pub const IN_USE: u32 = 32;
pub const IN_ATTACK2: u32 = 2048;

/// A player's input for one tick: movement plus what the held weapon needs.
#[derive(Clone, Debug, Default)]
pub struct PlayerCmd {
    pub cmd: gm_move::UserCmd,
    /// The player holds `weapon_physgun`: attack grabs, attack2 freezes, use + mouse rotates.
    pub physgun: bool,
    pub mouse_x: f32,
    pub mouse_y: f32,
}

pub struct Server<'w> {
    pub collision: &'w gm_collision::World,
    pub physics: World,
    /// The static world body constraints to the world attach to.
    world_body: BodyHandle,
    pub sandbox: Sandbox,
    pub players: BTreeMap<u32, Player>,
    mover: gm_move::Mover<'w>,
    bodies: BTreeMap<u32, BodyHandle>,
    constraints: BTreeMap<u32, ConstraintHandle>,
    /// NoCollide pairs by sandbox constraint id.
    nocollides: BTreeMap<u32, (BodyHandle, BodyHandle)>,
    models: BTreeMap<String, ModelInfo>,
    hull_planes: BTreeMap<String, Vec<Vec<(Vec3, f32)>>>,
    pub events: Vec<Event>,
    pub physgun_params: PhysgunParams,
    /// Loads a model the first time something spawns it (the dedicated server reads the MDL/PHY
    /// from the mounted content on demand).
    pub model_loader: Option<ModelLoader>,
}

impl<'w> Server<'w> {
    pub fn new(collision: &'w gm_collision::World, physics: World, world_body: BodyHandle) -> Self {
        Server {
            collision,
            physics,
            world_body,
            sandbox: Sandbox::new(Default::default()),
            players: BTreeMap::new(),
            mover: gm_move::Mover::new(gm_move::MoveConfig::default(), collision),
            bodies: BTreeMap::new(),
            constraints: BTreeMap::new(),
            nocollides: BTreeMap::new(),
            models: BTreeMap::new(),
            hull_planes: BTreeMap::new(),
            events: Vec::new(),
            physgun_params: PhysgunParams::default(),
            model_loader: None,
        }
    }

    pub fn register_model(&mut self, name: &str, info: ModelInfo) {
        self.hull_planes
            .insert(name.to_string(), info.hull_planes.clone());
        self.models.insert(name.to_string(), info);
    }

    pub fn add_player(&mut self, id: u32, origin: Vec3, view_angles: Vec3) {
        self.players.insert(
            id,
            Player {
                state: gm_move::PlayerState {
                    origin,
                    ..Default::default()
                },
                view_angles,
                held: None,
            },
        );
    }

    /// Props already on the map (owner 0): registered models at given poses, optionally frozen.
    pub fn add_existing_props(&mut self, props: &[(String, Vec3, Vec3, bool)]) {
        let bodies: Vec<PropBody> = props
            .iter()
            .enumerate()
            .map(|(i, (model, pos, angles, _))| PropBody {
                id: i as u32 + 1,
                pos: *pos,
                angles: *angles,
                hulls: self.hull_planes.get(model).cloned().unwrap_or_default(),
            })
            .collect();
        let models: Vec<String> = props.iter().map(|p| p.0.clone()).collect();
        self.sandbox.place_existing(0, &bodies, &models);
        for (i, (_, _, _, frozen)) in props.iter().enumerate() {
            self.sandbox.set_frozen(i as u32 + 1, *frozen);
        }
        self.sync_physics();
    }

    /// `GetShootPos()` and view angles of a player.
    pub fn eye(&self, player: u32) -> Option<(Vec3, Vec3)> {
        let p = self.players.get(&player)?;
        let eye = p.state.origin + Vec3::new(0.0, 0.0, self.mover.eye_height(&p.state));
        Some((eye, p.view_angles))
    }

    /// Props as the traces see them, at their current physics poses.
    fn trace_bodies(&self) -> Vec<PropBody> {
        self.sandbox
            .props()
            .iter()
            .filter_map(|p| {
                let hulls = self.hull_planes.get(&p.model)?.clone();
                Some(PropBody {
                    id: p.id,
                    pos: p.pos,
                    angles: p.angles,
                    hulls,
                })
            })
            .collect()
    }

    /// `gm_spawn <model>` by a player.
    pub fn spawn_prop(&mut self, player: u32, model: &str) -> Option<u32> {
        let (eye, eye_angles) = self.eye(player)?;
        if !self.models.contains_key(model) {
            let loaded = self.model_loader.as_mut().and_then(|f| f(model))?;
            self.register_model(model, loaded);
        }
        let info = self.models.get(model)?.clone();
        let bodies = self.trace_bodies();
        let mut events = Vec::new();
        let id = self.sandbox.spawn_prop(
            self.collision,
            &bodies,
            player,
            &SpawnRequest {
                eye,
                eye_angles,
                model,
                bounds: info.bounds,
            },
            &mut events,
        );
        self.events.extend(events);
        self.sync_physics();
        id
    }

    /// A tool gun click of `player` with the current view.
    pub fn tool_click(&mut self, player: u32, tool: &str, right: bool) -> bool {
        let Some((eye, eye_angles)) = self.eye(player) else {
            return false;
        };
        let bodies = self.trace_bodies();
        let dir = gm_sandbox::forward(eye_angles.x, eye_angles.y);
        let Some(hit): Option<ToolHit> = gm_sandbox::tool_trace(self.collision, &bodies, eye, dir)
        else {
            return false;
        };
        let mut events = Vec::new();
        let ret = if right {
            self.sandbox
                .tool_right_click(player, tool, &hit, &mut events)
        } else {
            self.sandbox
                .tool_left_click(player, tool, &hit, &mut events)
        };
        self.events.extend(events);
        self.sync_physics();
        ret
    }

    pub fn undo(&mut self, player: u32) {
        let mut events = Vec::new();
        self.sandbox.undo(player, &mut events);
        self.events.extend(events);
        self.sync_physics();
    }

    pub fn cleanup(&mut self, player: u32) {
        let mut events = Vec::new();
        self.sandbox.cleanup(player, &mut events);
        self.events.extend(events);
        self.sync_physics();
    }

    /// Mirrors the sandbox's props and constraints into Jolt: new props get bodies at their
    /// placement, props the tool moved are teleported, frozen flags follow, gone entities are
    /// removed (constraints first), new constraints are created from their recorded points.
    fn sync_physics(&mut self) {
        let live: Vec<u32> = self.sandbox.props().iter().map(|p| p.id).collect();
        let live_c: Vec<u32> = self.sandbox.constraints().iter().map(|c| c.id).collect();
        for (id, c) in std::mem::take(&mut self.constraints) {
            if live_c.contains(&id) {
                self.constraints.insert(id, c);
            } else {
                self.physics.remove_constraint(c);
            }
        }
        for (id, pair) in std::mem::take(&mut self.nocollides) {
            if live_c.contains(&id) {
                self.nocollides.insert(id, pair);
            } else {
                self.physics.set_pair_collision(pair.0, pair.1, true);
            }
        }
        for (id, b) in std::mem::take(&mut self.bodies) {
            if live.contains(&id) {
                self.bodies.insert(id, b);
            } else {
                self.physics.remove_body(b);
            }
        }
        let props: Vec<_> = self.sandbox.props().to_vec();
        for p in &props {
            match self.bodies.get(&p.id) {
                None => {
                    let Some(info) = self.models.get(&p.model) else {
                        continue;
                    };
                    let Ok(body) = self.physics.add_prop(&PropDesc {
                        hulls: &info.hulls,
                        mass: info.mass,
                        volume: info.volume,
                        position: p.pos,
                        angles: (p.angles.x, p.angles.y, p.angles.z),
                        velocity: Vec3::ZERO,
                        angular_velocity: Vec3::ZERO,
                        surface: info.surface,
                        linear_damping: info.linear_damping,
                        angular_damping: info.angular_damping,
                        drag_axis_areas: info.drag_axis_areas,
                        inertia: info.inertia,
                    }) else {
                        continue;
                    };
                    if p.frozen {
                        self.physics.freeze(body);
                    }
                    self.bodies.insert(p.id, body);
                }
                Some(&body) => {
                    let st = self.physics.state(body);
                    if (st.position - p.pos).length() > 1e-3 {
                        self.physics
                            .teleport(body, p.pos, gm_sandbox::rotation(p.angles));
                    }
                    if p.frozen != self.physics.is_frozen(body) {
                        if p.frozen {
                            self.physics.freeze(body);
                        } else {
                            self.physics.unfreeze(body);
                        }
                    }
                }
            }
        }
        let constraints: Vec<_> = self.sandbox.constraints().to_vec();
        for c in &constraints {
            if self.constraints.contains_key(&c.id) || self.nocollides.contains_key(&c.id) {
                continue;
            }
            let body = |e: Option<u32>| -> Option<BodyHandle> {
                match e {
                    Some(id) => self.bodies.get(&id).copied(),
                    None => Some(self.world_body),
                }
            };
            let (Some(a), Some(b)) = (body(c.ent1), body(c.ent2)) else {
                continue;
            };
            let pose = |e: Option<u32>| {
                props
                    .iter()
                    .find(|p| Some(p.id) == e)
                    .map(|p| (p.pos, gm_sandbox::rotation(p.angles)))
            };
            let world_of = |e: Option<u32>, local: Vec3| match pose(e) {
                Some((pos, rot)) => pos + rot * local,
                None => local,
            };
            let handle = match &c.kind {
                ConstraintKind::Weld { .. } => self.physics.weld(a, b),
                ConstraintKind::Rope {
                    lpos1,
                    lpos2,
                    length,
                    addlength,
                    rigid,
                    ..
                } => self.physics.rope(
                    a,
                    b,
                    world_of(c.ent1, *lpos1),
                    world_of(c.ent2, *lpos2),
                    length + addlength,
                    *rigid,
                ),
                ConstraintKind::Axis {
                    lpos1, local_axis, ..
                } => {
                    let point = world_of(c.ent1, *lpos1);
                    let axis_end = world_of(c.ent1, *local_axis);
                    let axis = (axis_end - point).normalize_or_zero();
                    let axis = if axis == Vec3::ZERO { Vec3::Z } else { axis };
                    self.physics.hinge(a, b, point, axis)
                }
                ConstraintKind::NoCollide => {
                    self.physics.set_pair_collision(a, b, false);
                    self.nocollides.insert(c.id, (a, b));
                    None
                }
            };
            if let Some(h) = handle {
                self.constraints.insert(c.id, h);
            }
        }
    }

    /// The physgun of one player for this tick, with the eye before the player moves (the weapon
    /// fires in the player's think before movement and physics).
    fn physgun(&mut self, player: u32, pc: &PlayerCmd) {
        let Some((eye, _)) = self.eye(player) else {
            return;
        };
        let view = pc.cmd.view_angles;
        let forward = gm_sandbox::forward(view.x, view.y);
        let (sy, cy) = (view.y.to_radians().sin(), view.y.to_radians().cos());
        let right = Vec3::new(sy, -cy, 0.0);
        let attack = pc.cmd.buttons & IN_ATTACK != 0;
        let use_key = pc.cmd.buttons & IN_USE != 0;
        let attack2 = pc.cmd.buttons & IN_ATTACK2 != 0;
        let held = self.players.get(&player).and_then(|p| p.held);
        let held = match held {
            None if attack && pc.physgun => {
                let bodies = self.trace_bodies();
                let hit = gm_sandbox::tool_trace(self.collision, &bodies, eye, forward);
                match hit.and_then(|h| h.ent.map(|e| (e, h))) {
                    Some((ent, h))
                        if !self.sandbox.props().iter().any(|p| p.id == ent && p.frozen) =>
                    {
                        let body = self.bodies.get(&ent).copied();
                        body.map(|b| {
                            let st = self.physics.state(b);
                            (
                                ent,
                                Grab::new(eye, forward, h.pos, st.position, st.rotation, view.y),
                            )
                        })
                    }
                    _ => None,
                }
            }
            Some((ent, _)) if !attack => {
                if let Some(b) = self.bodies.get(&ent).copied() {
                    self.physics.release(b);
                }
                None
            }
            Some((ent, _)) if attack2 => {
                if let Some(b) = self.bodies.get(&ent).copied() {
                    self.physics.freeze(b);
                }
                self.sandbox.set_frozen(ent, true);
                None
            }
            other => other,
        };
        let held = match held {
            Some((ent, mut g)) => {
                if let Some(b) = self.bodies.get(&ent).copied() {
                    if use_key {
                        g.rotate(pc.mouse_x, pc.mouse_y, right);
                    }
                    let st = self.physics.state(b);
                    let (tp, tr) = g.target(eye, forward, view.y, st.rotation);
                    self.physics.hold(b, tp, tr, &self.physgun_params);
                    Some((ent, g))
                } else {
                    None
                }
            }
            None => None,
        };
        if let Some(p) = self.players.get_mut(&player) {
            p.held = held;
        }
    }

    /// Moves one player by one usercmd outside the tick (catching up a usercmd backlog).
    pub fn move_player(&mut self, player: u32, pc: &PlayerCmd) {
        if let Some(p) = self.players.get_mut(&player) {
            p.view_angles = pc.cmd.view_angles;
            let cmd = gm_move::UserCmd {
                buttons: pc.cmd.buttons & !(IN_ATTACK | IN_USE | IN_ATTACK2),
                ..pc.cmd
            };
            self.mover.tick(&mut p.state, &cmd);
        }
    }

    /// One fixed tick: weapons fire with the pre-move eye, physics steps, players move, prop
    /// poses flow back into the sandbox.
    pub fn tick(&mut self, cmds: &BTreeMap<u32, PlayerCmd>) {
        for (id, pc) in cmds {
            if self.players.contains_key(id) {
                self.physgun(*id, pc);
            }
        }
        self.physics.step();
        for (id, p) in self.players.iter_mut() {
            if let Some(pc) = cmds.get(id) {
                p.view_angles = pc.cmd.view_angles;
                let cmd = gm_move::UserCmd {
                    buttons: pc.cmd.buttons & !(IN_ATTACK | IN_USE | IN_ATTACK2),
                    ..pc.cmd
                };
                self.mover.tick(&mut p.state, &cmd);
            }
        }
        for (id, body) in &self.bodies {
            let st = self.physics.state(*body);
            self.sandbox
                .set_prop_pose(*id, st.position, gm_sandbox::angles_of(st.rotation));
        }
    }

    /// `Entity:SetPos/SetAngles` from a script: the sandbox pose and the body move together.
    pub fn teleport_prop(&mut self, id: u32, pos: Vec3, angles: Vec3) -> bool {
        if !self.sandbox.props().iter().any(|p| p.id == id) {
            return false;
        }
        self.sandbox.set_prop_pose(id, pos, angles);
        if let Some(&b) = self.bodies.get(&id) {
            self.physics.teleport(b, pos, gm_sandbox::rotation(angles));
        }
        true
    }

    /// `PhysObj:EnableMotion` from a script.
    pub fn set_prop_frozen(&mut self, id: u32, frozen: bool) -> bool {
        if !self.sandbox.props().iter().any(|p| p.id == id) {
            return false;
        }
        self.sandbox.set_frozen(id, frozen);
        self.sync_physics();
        true
    }

    /// `Entity:Remove` from a script: constraints on it go too, like the sandbox's own removal.
    pub fn remove_prop(&mut self, id: u32) -> bool {
        if !self.sandbox.props().iter().any(|p| p.id == id) {
            return false;
        }
        self.sandbox.remove_entity(id);
        self.sync_physics();
        true
    }

    /// Physics state of a sandbox prop (velocity in Source units, angular body-local deg/s).
    pub fn prop_state(&self, prop: u32) -> Option<gm_physics::BodyState> {
        self.bodies.get(&prop).map(|b| self.physics.state(*b))
    }

    pub fn body_of(&self, prop: u32) -> Option<BodyHandle> {
        self.bodies.get(&prop).copied()
    }
}
