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

#[derive(Clone, Debug, PartialEq)]
pub struct PlayerSnap {
    pub id: u32,
    pub origin: Vec3,
    pub velocity: Vec3,
    pub view_angles: Vec3,
    pub held: Option<u32>,
}

#[derive(Clone, Debug, Default, PartialEq)]
pub struct Snapshot {
    pub tick: u64,
    pub entities: BTreeMap<u32, EntitySnap>,
    pub players: BTreeMap<u32, PlayerSnap>,
    pub prop_count: usize,
    pub constraint_count: usize,
}

impl Snapshot {
    pub fn of(server: &Server<'_>, tick: u64) -> Self {
        Snapshot {
            tick,
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
                        },
                    )
                })
                .collect(),
            prop_count: server.sandbox.props().len(),
            constraint_count: server.sandbox.constraints().len(),
        }
    }
}
