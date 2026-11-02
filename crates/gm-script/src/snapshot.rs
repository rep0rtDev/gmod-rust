//! The immutable view of the world every actor reads during the parallel phase.

use glam::Vec3;
use gm_server::Server;
use std::collections::BTreeMap;

#[derive(Clone, Debug, PartialEq)]
pub struct EntitySnap {
    pub id: u32,
    pub model: String,
    pub pos: Vec3,
    pub angles: Vec3,
    pub frozen: bool,
    pub skin: u8,
    pub body: String,
    pub owner: u32,
    /// Class name (`prop_physics`, `prop_ragdoll`, a vehicle / weapon / NPC class).
    pub class: String,
    pub health: i32,
    pub max_health: i32,
    pub velocity: Vec3,
    /// Physics body: angular velocity (deg/s), mass (kg), `(linear, angular)` damping, asleep.
    pub ang_velocity: Vec3,
    pub mass: f32,
    pub damping: (f32, f32),
    pub asleep: bool,
    /// `Entity:GetParent`: 0 when not parented.
    pub parent: u32,
    /// Render state: `Entity:SetColor` rgba, `SetMaterial` override, `RENDERMODE_*`, `kRenderFx*`.
    pub color: [u8; 4],
    pub material: String,
    pub render_mode: u8,
    pub render_fx: u8,
    /// Sequence the entity plays (NPCs; props do not animate on the server).
    pub sequence: String,
    /// The server animator's cycle `[0, 1]`, playback rate, finished flag and gesture layer.
    pub cycle: f32,
    pub playback_rate: f32,
    pub sequence_finished: bool,
    pub gesture: Option<gm_server::GestureLayer>,
    pub pose: Vec<(String, f32)>,
    /// Collision bounds: an NPC's hull, a prop's model box, or a script override.
    pub mins: Vec3,
    pub maxs: Vec3,
    /// Poser state: non-identity bone scales / angles `(bone, value)`, flex weights + scale, eye target.
    pub bone_scales: Vec<(u32, Vec3)>,
    pub bone_angles: Vec<(u32, Vec3)>,
    pub flex: Option<(Vec<f32>, f32)>,
    pub eye_target: Option<Vec3>,
    /// NPC weapon class, empty for none / non-NPCs.
    pub weapon: String,
    /// `GetBloodColor`: the NPC table's blood colour (game enum), -1 for anything else.
    pub blood: i8,
    /// NPC: server tick its current target was last seen (0 = never / not an NPC).
    pub npc_target_seen: u64,
    pub npc_target_first_seen: u64,
    pub npc_enemy_damage: u64,
    pub npc_sight: f32,
    /// Physics surface in use (script override or model PHY).
    pub friction: f32,
    pub elasticity: f32,
    pub on_fire: bool,
    /// NPC only: the player it has as target / chases, and the end of its current path.
    pub npc_target: u32,
    pub npc_chase: u32,
    pub npc_goal: Option<Vec3>,
}

#[derive(Clone, Debug, Default, PartialEq)]
pub struct PlayerSnap {
    pub id: u32,
    pub origin: Vec3,
    pub velocity: Vec3,
    pub view_angles: Vec3,
    pub held: Option<u32>,
    /// The prop carried with +use, and the mover's water level (0 dry .. 3 eyes under).
    pub carry: Option<u32>,
    pub water_level: u8,
    /// Connection: ping in ms, loss in percent, seconds since the last packet, peer address.
    pub ping: f32,
    pub loss: f32,
    pub timeout: f32,
    pub addr: String,
    pub alive: bool,
    pub health: i32,
    pub armor: i32,
    pub max_health: i32,
    /// Authenticated identity (ed25519 public key, hex) or `None`.
    pub identity: Option<String>,
    pub model: String,
    pub skin: u8,
    pub body: String,
    pub weapons: Vec<String>,
    pub active_weapon: String,
    pub previous_weapon: String,
    /// Reserve ammo per ammo type and `(class, clip1, clip2)` per carried weapon.
    pub ammo: Vec<(String, i32)>,
    pub clips: Vec<(String, i32, i32)>,
    /// Mover state: buttons of this and the previous processed command, ground / duck flags.
    pub buttons: u32,
    pub buttons_last: u32,
    /// Sandbox spawn counts per kind (`Player:GetCount`), from the server's ledger.
    pub counts: Vec<(String, u32)>,
    /// The processed command: forward/side/up move, mouse deltas, impulse, server tick.
    pub cmd_move: Vec3,
    pub cmd_mouse_x: f32,
    pub cmd_mouse_y: f32,
    pub cmd_impulse: u8,
    pub cmd_tick: u64,
    pub on_ground: bool,
    pub ducked: bool,
    /// Vehicle seat / driven prop / `+use` target entity, 0 for none.
    pub vehicle: u32,
    pub driving: u32,
    pub use_target: u32,
    pub flashlight: bool,
    pub punch: Vec3,
    pub punch_vel: Vec3,
    pub ladder: bool,
    pub suit: bool,
    /// Movement tunables the player moves with (`GetWalkSpeed` and friends).
    pub walk_speed: f32,
    pub run_speed: f32,
    pub jump_power: f32,
    pub step_size: f32,
    pub crouched_walk_speed: f32,
    pub duck_time: f32,
    pub unduck_time: f32,
    pub max_velocity: f32,
    pub stand_eye: f32,
    pub duck_eye: f32,
    pub hull_half: f32,
    pub stand_height: f32,
    pub duck_height: f32,
    pub frags: u32,
    pub deaths: u32,
    pub max_armor: i32,
    pub no_target: bool,
    pub player_color: Vec3,
    pub weapon_color: Vec3,
    pub locked: bool,
    pub joined_tick: u64,
    pub last_hitgroup: u8,
}

/// The server's collision world for traces during a dispatch. The snapshot only lives while the
/// runtime holds `&mut Server`, and the actors' handles are reset to an empty snapshot right
/// after every phase, so the pointer never outlives the world it points at.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct WorldRef(pub *const gm_collision::World);
unsafe impl Send for WorldRef {}
unsafe impl Sync for WorldRef {}

impl Default for WorldRef {
    fn default() -> Self {
        WorldRef(std::ptr::null())
    }
}

/// The server's navmesh, if one was built; same lifetime rule as `WorldRef`.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct NavRef(pub *const gm_nav::NavMesh);

unsafe impl Send for NavRef {}
unsafe impl Sync for NavRef {}

/// The server's `model_scripts` map, same lifetime rule as `WorldRef`.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct ModelsRef(pub *const std::collections::BTreeMap<String, gm_server::ModelScriptData>);

/// The server's surface table, read through the snapshot like the model data.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct SurfacesRef(pub *const Option<gm_server::SurfaceTable>);
unsafe impl Send for SurfacesRef {}
unsafe impl Sync for SurfacesRef {}
impl Default for SurfacesRef {
    fn default() -> Self {
        SurfacesRef(std::ptr::null())
    }
}

unsafe impl Send for ModelsRef {}
unsafe impl Sync for ModelsRef {}

impl Default for ModelsRef {
    fn default() -> Self {
        ModelsRef(std::ptr::null())
    }
}

/// The server's `vehicles` map, same lifetime rule as `WorldRef`.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct VehiclesRef(pub *const std::collections::BTreeMap<u32, gm_server::Vehicle>);

unsafe impl Send for VehiclesRef {}
unsafe impl Sync for VehiclesRef {}

impl Default for VehiclesRef {
    fn default() -> Self {
        VehiclesRef(std::ptr::null())
    }
}

impl Default for NavRef {
    fn default() -> Self {
        NavRef(std::ptr::null())
    }
}

#[derive(Clone, Debug, Default, PartialEq)]
pub struct Snapshot {
    pub tick: u64,
    /// Mounted content provider labels (`IsMounted`).
    pub mounted: Vec<String>,
    pub world: WorldRef,
    pub nav: NavRef,
    pub models: ModelsRef,
    /// The server's `surfaceproperties` table (`util.GetSurfaceData`).
    pub surfaces: SurfacesRef,
    /// First spawn point of the map (`game.StartSpot`): position and angles.
    pub start_spot: Option<(glam::Vec3, glam::Vec3)>,
    pub map_revision: i32,
    pub sun_direction: Option<glam::Vec3>,
    pub vehicles: VehiclesRef,
    /// Prop collision hulls for `phys.trace`.
    pub props: Vec<gm_sandbox::PropBody>,
    pub entities: BTreeMap<u32, EntitySnap>,
    pub players: BTreeMap<u32, PlayerSnap>,
    pub prop_count: usize,
    pub constraint_count: usize,
    /// Console variables (every one on the server, the replicated ones on the client).
    pub cvars: BTreeMap<String, String>,
}

impl Snapshot {
    /// The collision world behind the snapshot (`Entity:TestPVS`), if the host set one.
    pub fn collision(&self) -> Option<&gm_collision::World> {
        if self.world.0.is_null() {
            return None;
        }
        // SAFETY: the pointer is the server's world, alive while the snapshot is read
        Some(unsafe { &*self.world.0 })
    }

    /// `phys.trace`: world brushes/displacements and prop hulls, like the tool gun's trace.
    pub fn trace(&self, start: glam::Vec3, end: glam::Vec3) -> Option<gm_sandbox::ToolHit> {
        if self.world.0.is_null() {
            return None;
        }
        // see WorldRef: valid while the snapshot is in use
        let world = unsafe { &*self.world.0 };
        gm_sandbox::trace_line(world, &self.props, start, end)
    }

    fn nav(&self) -> Option<&gm_nav::NavMesh> {
        // see NavRef: valid while the snapshot is in use
        (!self.nav.0.is_null()).then(|| unsafe { &*self.nav.0 })
    }

    /// `Entity:GetBodyGroups` and friends: the model's script data when the server loaded it.
    pub fn model_info(&self, name: &str) -> Option<&gm_server::ModelScriptData> {
        (!self.models.0.is_null())
            .then(|| unsafe { &*self.models.0 })
            .and_then(|m| m.get(name))
    }

    /// `Vehicle:GetVehicleParams` and friends: the server's vehicle record.
    pub fn vehicle_info(&self, id: u32) -> Option<&gm_server::Vehicle> {
        (!self.vehicles.0.is_null())
            .then(|| unsafe { &*self.vehicles.0 })
            .and_then(|m| m.get(&id))
    }

    pub fn nav_loaded(&self) -> bool {
        self.nav().is_some()
    }

    pub fn nav_poly_count(&self) -> usize {
        self.nav().map_or(0, |n| n.poly_count())
    }

    pub fn nav_polys(&self) -> &[gm_nav::NavPoly] {
        self.nav().map_or(&[], |n| n.polys())
    }

    pub fn surface_table(&self) -> Option<&gm_server::SurfaceTable> {
        if self.surfaces.0.is_null() {
            return None;
        }
        // SAFETY: the pointer is the server's field, alive while the snapshot is read
        unsafe { (*self.surfaces.0).as_ref() }
    }

    pub fn nav_vertices(&self) -> &[glam::Vec3] {
        self.nav().map_or(&[], |n| n.vertices())
    }

    pub fn nav_nearest(&self, pos: glam::Vec3, extents: glam::Vec3) -> Option<glam::Vec3> {
        self.nav().and_then(|n| n.nearest(pos, extents))
    }

    pub fn nav_path(
        &self,
        a: glam::Vec3,
        b: glam::Vec3,
        extents: glam::Vec3,
    ) -> Option<gm_nav::Path> {
        self.nav().and_then(|n| n.find_path(a, b, extents))
    }

    /// The client realm's view: what the client assembled from snapshots (no world ref, so
    /// `phys.trace` misses; no prop hulls).
    pub fn of_client(world: &gm_net::ClientWorld) -> Self {
        Self::of_client_with(world, WorldRef::default(), Vec::new())
    }

    /// `of_client` with the client's own collision world and the prop hulls it knows, so
    /// `phys.trace` works on the client (`Runtime::set_client_world`).
    pub fn of_client_with(
        world: &gm_net::ClientWorld,
        world_ref: WorldRef,
        props: Vec<gm_sandbox::PropBody>,
    ) -> Self {
        let classes = gm_net::msg::weapon_classes(&world.weapon_defs);
        Snapshot {
            tick: world.tick as u64,
            mounted: Vec::new(),
            world: world_ref,
            nav: NavRef::default(),
            models: ModelsRef::default(),
            surfaces: SurfacesRef::default(),
            start_spot: None,
            map_revision: 0,
            sun_direction: None,
            vehicles: VehiclesRef::default(),
            props,
            entities: world
                .entities
                .values()
                .map(|e| {
                    (
                        e.id,
                        EntitySnap {
                            id: e.id,
                            model: e.model.clone(),
                            pos: e.pos,
                            angles: e.angles,
                            frozen: e.frozen,
                            skin: e.skin,
                            body: e.body.clone(),
                            owner: e.owner,
                            class: if e.bones.is_empty() {
                                "prop_physics".to_owned()
                            } else {
                                "prop_ragdoll".to_owned()
                            },
                            health: 0,
                            max_health: 0,
                            velocity: e.vel,
                            ang_velocity: Vec3::ZERO,
                            mass: 0.0,
                            damping: (0.0, 0.0),
                            asleep: false,
                            parent: 0,
                            color: e.color,
                            material: e.material.clone(),
                            render_mode: e.render_mode,
                            render_fx: e.render_fx,
                            sequence: e.sequence.clone(),
                            cycle: e.cycle,
                            playback_rate: e.playback_rate,
                            sequence_finished: e.sequence_finished,
                            pose: e.pose.clone(),
                            mins: e.mins,
                            maxs: e.maxs,
                            gesture: e.gesture.as_ref().map(|g| gm_server::GestureLayer {
                                sequence: g.sequence.clone(),
                                cycle: g.cycle,
                                rate: g.rate,
                                weight: g.weight,
                                looping: g.looping,
                                autokill: g.autokill,
                                priority: g.priority,
                                finished: g.finished,
                            }),
                            bone_scales: Vec::new(),
                            bone_angles: Vec::new(),
                            flex: e.flex.as_ref().map(|f| (f.weights.clone(), f.scale)),
                            eye_target: e.eye_target,
                            weapon: String::new(),
                            blood: -1,
                            npc_target_seen: 0,
                            npc_target_first_seen: 0,
                            npc_enemy_damage: 0,
                            npc_sight: 0.0,
                            friction: 0.8,
                            elasticity: 0.25,
                            on_fire: false,
                            npc_target: 0,
                            npc_chase: 0,
                            npc_goal: None,
                        },
                    )
                })
                .collect(),
            players: world
                .players
                .iter()
                .map(|p| {
                    (
                        p.id,
                        PlayerSnap {
                            id: p.id,
                            counts: Vec::new(),
                            origin: p.origin,
                            velocity: p.velocity,
                            view_angles: p.view_angles,
                            held: p.held,
                            alive: p.alive,
                            health: p.health as i32,
                            armor: p.armor as i32,
                            max_health: 100,
                            identity: None,
                            weapons: gm_net::msg::weapon_names(p.weapons, &classes),
                            previous_weapon: String::new(),
                            active_weapon: classes
                                .get(p.active as usize)
                                .cloned()
                                .unwrap_or_default(),
                            ..Default::default()
                        },
                    )
                })
                .collect(),
            prop_count: world.own.count as usize,
            constraint_count: 0,
            cvars: world.cvars.clone(),
        }
    }

    pub fn of(server: &Server<'_>, tick: u64) -> Self {
        Snapshot {
            tick,
            mounted: server.mounted.clone(),
            world: WorldRef(server.collision as *const _),
            models: ModelsRef(&server.model_scripts as *const _),
            surfaces: SurfacesRef(&server.surface_table as *const _),
            start_spot: server.spawn_points.first().map(|s| (s.pos, s.angles)),
            map_revision: server.map_revision,
            sun_direction: server.sun_direction,
            vehicles: VehiclesRef(&server.vehicles as *const _),
            nav: NavRef(server.navmesh().map_or(std::ptr::null(), |n| n as *const _)),
            props: server.trace_bodies(),
            entities: server
                .sandbox
                .props()
                .iter()
                .map(|p| {
                    let phys = server.entity_phys(p.id);
                    (
                        p.id,
                        EntitySnap {
                            id: p.id,
                            model: p.model.clone(),
                            pos: p.pos,
                            angles: p.angles,
                            frozen: p.frozen,
                            skin: p.skin,
                            body: p.body.clone(),
                            owner: server.entity_owner(p.id),
                            class: server.entity_class(p.id),
                            health: server.vehicles.get(&p.id).map_or(p.health, |v| v.health),
                            max_health: server.vehicles.get(&p.id).map_or(0, |v| v.max_health),
                            velocity: server.entity_velocity(p.id),
                            ang_velocity: phys.0,
                            mass: phys.1,
                            damping: phys.2,
                            asleep: phys.3,
                            parent: server.entity_parent(p.id),
                            color: p.color,
                            material: p.material.clone(),
                            render_mode: p.render_mode,
                            render_fx: p.render_fx,
                            sequence: server
                                .anim(p.id)
                                .map_or(String::new(), |a| a.sequence.clone()),
                            cycle: server.anim(p.id).map_or(0.0, |a| a.cycle),
                            playback_rate: server.anim(p.id).map_or(1.0, |a| a.rate),
                            sequence_finished: server.anim(p.id).is_some_and(|a| a.finished),
                            pose: server.pose_parameters(p.id).map_or_else(Vec::new, |m| {
                                m.iter().map(|(k, v)| (k.clone(), *v)).collect()
                            }),
                            mins: server.collision_bounds(p.id).map_or(Vec3::ZERO, |b| b.0),
                            maxs: server.collision_bounds(p.id).map_or(Vec3::ZERO, |b| b.1),
                            gesture: server.anim(p.id).and_then(|a| a.gesture.clone()),
                            bone_scales: server
                                .bone_scales(p.id)
                                .into_iter()
                                .map(|(i, v)| (i as u32, v))
                                .collect(),
                            bone_angles: server
                                .bone_angles(p.id)
                                .into_iter()
                                .map(|(i, v)| (i as u32, v))
                                .collect(),
                            flex: server.flex_state(p.id),
                            eye_target: server.eye_target(p.id),
                            weapon: String::new(),
                            blood: -1,
                            npc_target_seen: 0,
                            npc_target_first_seen: 0,
                            npc_enemy_damage: 0,
                            npc_sight: 0.0,
                            friction: server.entity_surface(p.id).0,
                            elasticity: server.entity_surface(p.id).1,
                            on_fire: p.fire.is_some(),
                            npc_target: 0,
                            npc_chase: 0,
                            npc_goal: None,
                        },
                    )
                })
                .chain(server.npcs.iter().map(|(&id, n)| {
                    (
                        id,
                        EntitySnap {
                            id,
                            model: n.model.clone(),
                            pos: n.pos,
                            angles: n.angles,
                            frozen: false,
                            skin: n.skin,
                            body: String::new(),
                            owner: server.entity_owner(id),
                            class: n.class.clone(),
                            health: n.health.max(0.0) as i32,
                            max_health: n.max_health,
                            velocity: n.flight.unwrap_or(Vec3::ZERO),
                            ang_velocity: Vec3::ZERO,
                            mass: 0.0,
                            damping: (0.0, 0.0),
                            asleep: false,
                            parent: 0,
                            color: [255; 4],
                            material: String::new(),
                            render_mode: 0,
                            render_fx: 0,
                            sequence: server
                                .anim(id)
                                .map_or(n.sequence.as_str(), |a| a.sequence.as_str())
                                .to_string(),
                            cycle: server.anim(id).map_or(0.0, |a| a.cycle),
                            playback_rate: server.anim(id).map_or(1.0, |a| a.rate),
                            sequence_finished: server.anim(id).is_some_and(|a| a.finished),
                            pose: server.pose_parameters(id).map_or_else(Vec::new, |m| {
                                m.iter().map(|(k, v)| (k.clone(), *v)).collect()
                            }),
                            mins: server.collision_bounds(id).map_or(Vec3::ZERO, |b| b.0),
                            maxs: server.collision_bounds(id).map_or(Vec3::ZERO, |b| b.1),
                            gesture: server.anim(id).and_then(|a| a.gesture.clone()),
                            bone_scales: Vec::new(),
                            bone_angles: Vec::new(),
                            flex: None,
                            eye_target: None,
                            weapon: n.weapon.clone(),
                            blood: n.blood,
                            npc_target_seen: n.target_seen,
                            npc_target_first_seen: n.target_first_seen,
                            npc_enemy_damage: n.last_enemy_damage,
                            npc_sight: n.sight,
                            friction: 0.8,
                            elasticity: 0.25,
                            on_fire: false,
                            npc_target: n.target.unwrap_or(0),
                            npc_chase: n.chase.unwrap_or(0),
                            npc_goal: n.goal.as_ref().and_then(|g| g.points.last().copied()),
                        },
                    )
                }))
                .collect(),
            players: server
                .players
                .iter()
                .map(|(id, p)| {
                    (
                        *id,
                        PlayerSnap {
                            id: *id,
                            counts: [
                                "props",
                                "ragdolls",
                                "npcs",
                                "vehicles",
                                "constraints",
                                "ropeconstraints",
                                "thrusters",
                                "hoverballs",
                                "balloons",
                                "wheels",
                                "buttons",
                                "dynamite",
                                "lamps",
                                "lights",
                                "emitters",
                                "cameras",
                                "sents",
                            ]
                            .iter()
                            .map(|k| (k.to_string(), server.sandbox.count_of(*id, k) as u32))
                            .collect(),
                            origin: p.state.origin,
                            velocity: p.state.velocity,
                            view_angles: p.view_angles,
                            held: p.held.map(|h| h.0),
                            carry: p.carry.map(|c| c.0),
                            water_level: p.state.water_level,
                            ping: p.conn.ping_ms,
                            loss: p.conn.loss_pct,
                            timeout: p.conn.timeout_secs,
                            addr: p.conn.addr.clone(),
                            alive: p.alive,
                            health: p.health,
                            armor: p.armor,
                            max_health: p.max_health,
                            identity: p
                                .identity
                                .map(|k| k.iter().map(|x| format!("{x:02x}")).collect()),
                            model: p.model.clone(),
                            skin: p.skin,
                            body: p.body.clone(),
                            weapons: p.weapons.clone(),
                            active_weapon: p.active_weapon.clone(),
                            previous_weapon: p.previous_weapon.clone(),
                            frags: p.frags,
                            deaths: p.deaths,
                            max_armor: p.max_armor,
                            no_target: p.no_target,
                            player_color: p.player_color,
                            weapon_color: p.weapon_color,
                            locked: p.locked,
                            joined_tick: p.joined_tick,
                            last_hitgroup: server.last_hit_group(*id),
                            ammo: p
                                .inventory
                                .ammo
                                .iter()
                                .map(|(k, v)| (k.clone(), *v))
                                .collect(),
                            clips: p
                                .inventory
                                .clips
                                .iter()
                                .map(|(k, v)| (k.clone(), v[0], v[1]))
                                .collect(),
                            buttons: p.buttons,
                            buttons_last: p.buttons_last,
                            cmd_move: p.cmd_move,
                            cmd_mouse_x: p.cmd_mouse.0,
                            cmd_mouse_y: p.cmd_mouse.1,
                            cmd_impulse: p.cmd_impulse,
                            cmd_tick: p.cmd_tick,
                            on_ground: p.state.on_ground,
                            ducked: p.state.ducked,
                            vehicle: p.seat.unwrap_or(0),
                            driving: p.driving.map_or(0, |d| d.prop),
                            use_target: p.use_target.unwrap_or(0),
                            flashlight: p.flashlight,
                            punch: p.state.punch,
                            punch_vel: p.state.punch_vel,
                            ladder: p.state.ladder != 0,
                            suit: p.suit,
                            walk_speed: p.move_override.effective(server.move_cfg()).walk_speed,
                            run_speed: p.move_override.effective(server.move_cfg()).run_speed,
                            jump_power: p.move_override.effective(server.move_cfg()).jump_power,
                            step_size: p.move_override.effective(server.move_cfg()).step_size,
                            crouched_walk_speed: p
                                .move_override
                                .effective(server.move_cfg())
                                .crouched_walk_speed,
                            duck_time: p.move_override.effective(server.move_cfg()).duck_time,
                            unduck_time: p.move_override.effective(server.move_cfg()).unduck_time,
                            max_velocity: p.move_override.effective(server.move_cfg()).max_velocity,
                            stand_eye: p.move_override.effective(server.move_cfg()).stand_eye,
                            duck_eye: p.move_override.effective(server.move_cfg()).duck_eye,
                            hull_half: p.move_override.effective(server.move_cfg()).hull_half,
                            stand_height: p.move_override.effective(server.move_cfg()).stand_height,
                            duck_height: p.move_override.effective(server.move_cfg()).duck_height,
                        },
                    )
                })
                .collect(),
            prop_count: server.sandbox.props().len(),
            constraint_count: server.sandbox.constraints().len(),
            cvars: server
                .cvars
                .iter()
                .map(|(n, c)| (n.clone(), c.value.clone()))
                .collect(),
        }
    }
}
