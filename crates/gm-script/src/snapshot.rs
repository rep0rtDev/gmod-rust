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
    /// Reserve ammo per ammo type and `(class, clip1, clip2)` per carried weapon.
    pub ammo: Vec<(String, i32)>,
    pub clips: Vec<(String, i32, i32)>,
    pub frags: u32,
    pub deaths: u32,
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

impl Default for NavRef {
    fn default() -> Self {
        NavRef(std::ptr::null())
    }
}

#[derive(Clone, Debug, Default, PartialEq)]
pub struct Snapshot {
    pub tick: u64,
    pub world: WorldRef,
    pub nav: NavRef,
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

    pub fn nav_loaded(&self) -> bool {
        self.nav().is_some()
    }

    pub fn nav_poly_count(&self) -> usize {
        self.nav().map_or(0, |n| n.poly_count())
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
            world: world_ref,
            nav: NavRef::default(),
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
            world: WorldRef(server.collision as *const _),
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
                            origin: p.state.origin,
                            velocity: p.state.velocity,
                            view_angles: p.view_angles,
                            held: p.held.map(|h| h.0),
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
                            frags: p.frags,
                            deaths: p.deaths,
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
