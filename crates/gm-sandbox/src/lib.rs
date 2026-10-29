//! Sandbox server core, measured against the real gamemode (`tools/parity/scenarios/sandbox`):
//! where `gm_spawn` puts a prop, how `gmod_undo` and `gmod_cleanup` walk the per-player
//! stacks and how `sbox_maxprops` is enforced. No GLua is executed; the behaviour is
//! re-implemented and checked by the `sandbox_*` harnesses.

use glam::{Quat, Vec3};
use gm_collision::{World, MASK_SOLID};
use std::collections::BTreeMap;

/// Model bounds the game uses for placement (`OBBMins`/`OBBMaxs`, the studio hull).
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct ModelBounds {
    pub min: Vec3,
    pub max: Vec3,
}

#[derive(Clone, Debug, PartialEq)]
pub struct Prop {
    pub id: u32,
    pub owner: u32,
    pub model: String,
    pub pos: Vec3,
    /// Source angles: pitch, yaw, roll in degrees.
    pub angles: Vec3,
}

#[derive(Clone, Debug, PartialEq)]
pub struct Undo {
    pub id: u32,
    pub name: String,
    pub text: String,
    pub entities: Vec<u32>,
}

#[derive(Clone, Debug, PartialEq)]
pub enum Event {
    /// `achievements.SpawnedProp()` is sent to the player; a prop exists now.
    Spawned { prop: u32 },
    /// `Player:LimitHit(name)`: the spawn was refused by `sbox_max<name>`.
    LimitHit { limit: String },
    /// `gmod_undo` removed `removed` entities of undo `id` (0 when the entry was already empty).
    Undone { id: u32, removed: usize },
    /// `gmod_cleanup` without arguments: `hook.Run('OnCleanup','all')`.
    Cleanup { kind: String },
}

/// `sbox_*` limits; negative means unlimited, as in `Player:CheckLimit`.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Limits {
    pub max_props: i32,
}

impl Default for Limits {
    fn default() -> Self {
        Limits { max_props: 200 }
    }
}

/// A player's `gm_spawn`: eye position, view angles (pitch, yaw, roll) and the model.
#[derive(Clone, Debug, PartialEq)]
pub struct SpawnRequest<'a> {
    pub eye: Vec3,
    pub eye_angles: Vec3,
    pub model: &'a str,
    pub bounds: ModelBounds,
}

/// Where the spawn ray hit and the pose the gamemode computed from it.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Placement {
    pub hit: Vec3,
    pub normal: Vec3,
    pub pos: Vec3,
    pub angles: Vec3,
}

#[derive(Default)]
pub struct Sandbox {
    pub limits: Limits,
    next_id: u32,
    props: Vec<Prop>,
    undo: BTreeMap<u32, Vec<Undo>>,
    cleanup: BTreeMap<u32, Vec<u32>>,
}

/// Source `AngleVectors` forward for pitch/yaw in degrees.
pub fn forward(pitch: f32, yaw: f32) -> Vec3 {
    let (sp, cp) = pitch.to_radians().sin_cos();
    let (sy, cy) = yaw.to_radians().sin_cos();
    Vec3::new(cp * cy, cp * sy, -sp)
}

/// Rotation of Source angles (pitch about +y, yaw about +z, roll about +x), applied roll first.
pub fn rotation(angles: Vec3) -> Quat {
    Quat::from_rotation_z(angles.y.to_radians())
        * Quat::from_rotation_y(angles.x.to_radians())
        * Quat::from_rotation_x(angles.z.to_radians())
}

fn normalize_angle(a: f32) -> f32 {
    let mut a = a % 360.0;
    if a > 180.0 {
        a -= 360.0;
    } else if a <= -180.0 {
        a += 360.0;
    }
    a
}

/// `Entity:NearestPoint` on the model box at `pos`/`rot`.
fn nearest_point(pos: Vec3, rot: Quat, b: &ModelBounds, q: Vec3) -> Vec3 {
    let local = rot.inverse() * (q - pos);
    pos + rot * local.clamp(b.min, b.max)
}

/// `DoPlayerEntitySpawn` + `TryFixPropPosition` for a `prop_physics`: the entity is placed at
/// the hit point facing the player (yaw + 180, no pitch or roll), then moved so the point of
/// its box nearest to 512 u behind the surface sits on the hit point, then nudged out of walls
/// along each of its axes.
pub fn place_prop(world: &World, eye: Vec3, eye_angles: Vec3, bounds: &ModelBounds) -> Placement {
    let dir = forward(eye_angles.x, eye_angles.y);
    let tr = world.trace_ray(eye, eye + dir * 2048.0, MASK_SOLID);
    let (hit, normal) = (tr.end, if tr.hit() { tr.normal } else { Vec3::ZERO });
    let angles = Vec3::new(0.0, normalize_angle(eye_angles.y + 180.0), 0.0);
    let rot = rotation(angles);
    let far = hit - normal * 512.0;
    let nearest = nearest_point(hit, rot, bounds, far);
    let mut pos = hit + (hit - nearest);
    for axis in 0..3 {
        let mut lo = Vec3::ZERO;
        let mut hi = Vec3::ZERO;
        lo[axis] = bounds.min[axis];
        hi[axis] = bounds.max[axis];
        let end_d = pos + rot * lo;
        let end_u = pos + rot * hi;
        let tr_d = world.trace_ray(pos, end_d, MASK_SOLID);
        let tr_u = world.trace_ray(pos, end_u, MASK_SOLID);
        if tr_d.hit() && tr_u.hit() {
            continue;
        }
        if tr_d.hit() {
            pos += tr_d.end - end_d;
        }
        if tr_u.hit() {
            pos += tr_u.end - end_u;
        }
    }
    Placement {
        hit,
        normal,
        pos,
        angles,
    }
}

impl Sandbox {
    pub fn new(limits: Limits) -> Self {
        Sandbox {
            limits,
            next_id: 1,
            ..Default::default()
        }
    }

    pub fn props(&self) -> &[Prop] {
        &self.props
    }

    /// `Player:GetCount("props")`: live props owned by the player.
    pub fn count(&self, player: u32) -> usize {
        self.props.iter().filter(|p| p.owner == player).count()
    }

    pub fn undos(&self, player: u32) -> &[Undo] {
        self.undo.get(&player).map(Vec::as_slice).unwrap_or(&[])
    }

    /// `Player:CheckLimit("props")`: refused once `count > max - 1`; negative max is unlimited.
    fn check_limit(&self, player: u32, events: &mut Vec<Event>) -> bool {
        let max = self.limits.max_props;
        if max < 0 {
            return true;
        }
        if self.count(player) as i32 > max - 1 {
            events.push(Event::LimitHit {
                limit: "props".into(),
            });
            return false;
        }
        true
    }

    /// `gm_spawn <model>` for a prop model.
    pub fn spawn_prop(
        &mut self,
        world: &World,
        player: u32,
        req: &SpawnRequest<'_>,
        events: &mut Vec<Event>,
    ) -> Option<u32> {
        if !self.check_limit(player, events) {
            return None;
        }
        let model = req.model;
        let placement = place_prop(world, req.eye, req.eye_angles, &req.bounds);
        let id = self.next_id;
        self.next_id += 1;
        self.props.push(Prop {
            id,
            owner: player,
            model: model.to_string(),
            pos: placement.pos,
            angles: placement.angles,
        });
        events.push(Event::Spawned { prop: id });
        let undos = self.undo.entry(player).or_default();
        let undo_id = undos.last().map(|u| u.id + 1).unwrap_or(1);
        undos.push(Undo {
            id: undo_id,
            name: "prop_physics".into(),
            text: format!("#prop_physics ({model})"),
            entities: vec![id],
        });
        self.cleanup.entry(player).or_default().push(id);
        Some(id)
    }

    fn remove(&mut self, id: u32) -> bool {
        let before = self.props.len();
        self.props.retain(|p| p.id != id);
        self.props.len() != before
    }

    /// `gmod_undo`: pops the player's last undo; entries whose entities are already gone are
    /// popped too until one removes something.
    pub fn undo(&mut self, player: u32, events: &mut Vec<Event>) {
        loop {
            let Some(u) = self.undo.get_mut(&player).and_then(Vec::pop) else {
                return;
            };
            let removed = u.entities.iter().filter(|&&e| self.remove(e)).count();
            events.push(Event::Undone { id: u.id, removed });
            if removed > 0 {
                return;
            }
        }
    }

    /// `gmod_cleanup` without a type: everything the player spawned goes; undo entries stay.
    pub fn cleanup(&mut self, player: u32, events: &mut Vec<Event>) {
        if let Some(list) = self.cleanup.remove(&player) {
            for id in list {
                self.remove(id);
            }
        }
        events.push(Event::Cleanup { kind: "all".into() });
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn floor() -> World {
        World::from_boxes(&[(
            Vec3::new(-4096.0, -4096.0, -200.0),
            Vec3::new(4096.0, 4096.0, -144.0),
        )])
    }

    fn crate_bounds() -> ModelBounds {
        ModelBounds {
            min: Vec3::new(-20.308, -20.150, -20.227),
            max: Vec3::new(20.231, 20.162, 20.011),
        }
    }

    #[test]
    fn floor_spawn_sits_flush_facing_the_player() {
        let p = place_prop(
            &floor(),
            Vec3::new(823.235, -500.0, -79.0),
            Vec3::new(30.0, 0.0, 0.0),
            &crate_bounds(),
        );
        assert!((p.hit.z + 144.0).abs() < 0.05);
        assert!((p.pos.z - (p.hit.z + 20.227)).abs() < 1e-3);
        assert!((p.pos.x - p.hit.x).abs() < 1e-3);
        assert_eq!(p.angles, Vec3::new(0.0, 180.0, 0.0));
    }

    #[test]
    fn limits_and_undo() {
        let w = floor();
        let mut sb = Sandbox::new(Limits { max_props: 2 });
        let mut ev = Vec::new();
        let eye = Vec3::new(0.0, 0.0, -80.0);
        let b = crate_bounds();
        let req = |yaw: f32| SpawnRequest {
            eye,
            eye_angles: Vec3::new(30.0, yaw, 0.0),
            model: "m",
            bounds: b,
        };
        for yaw in [0.0, 45.0, 90.0] {
            sb.spawn_prop(&w, 1, &req(yaw), &mut ev);
        }
        assert_eq!(sb.count(1), 2);
        assert!(matches!(ev.last(), Some(Event::LimitHit { .. })));
        assert_eq!(sb.undos(1).iter().map(|u| u.id).collect::<Vec<_>>(), [1, 2]);
        sb.undo(1, &mut ev);
        assert_eq!(sb.count(1), 1);
        sb.spawn_prop(&w, 1, &req(135.0), &mut ev);
        assert_eq!(sb.undos(1).iter().map(|u| u.id).collect::<Vec<_>>(), [1, 2]);
        sb.cleanup(1, &mut ev);
        assert_eq!(sb.count(1), 0);
        assert_eq!(sb.undos(1).len(), 2);
        sb.undo(1, &mut ev);
        assert!(sb.undos(1).is_empty());
    }
}
