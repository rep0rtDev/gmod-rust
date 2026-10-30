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
    pub owner: u32,
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
    pub weapons: Vec<String>,
    pub active_weapon: String,
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

#[derive(Clone, Debug, Default, PartialEq)]
pub struct Snapshot {
    pub tick: u64,
    pub world: WorldRef,
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
        Snapshot {
            tick: world.tick as u64,
            world: world_ref,
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
                            owner: e.owner,
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
                            weapons: gm_server::LOADOUT
                                .iter()
                                .enumerate()
                                .filter(|(i, _)| p.weapons & (1 << i) != 0)
                                .map(|(_, w)| w.to_string())
                                .collect(),
                            active_weapon: gm_server::LOADOUT
                                .get(p.active as usize)
                                .map(|w| w.to_string())
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
            props: server.trace_bodies(),
            entities: server
                .sandbox
                .props()
                .iter()
                .map(|p| {
                    (
                        p.id,
                        EntitySnap {
                            id: p.id,
                            model: p.model.clone(),
                            pos: p.pos,
                            angles: p.angles,
                            frozen: p.frozen,
                            owner: p.owner,
                        },
                    )
                })
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
                            weapons: p.weapons.clone(),
                            active_weapon: p.active_weapon.clone(),
                            frags: p.frags,
                            deaths: p.deaths,
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
