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
    /// `undo.SetCustomUndoText`, what the client prints instead of "Undone <name>".
    pub custom: Option<String>,
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

/// `sbox_*` limits (measured defaults: props 200, constraints 2000, ropeconstraints 1000);
/// negative means unlimited, as in `Player:CheckLimit`.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Limits {
    pub max_props: i32,
    pub max_constraints: i32,
    pub max_ropeconstraints: i32,
}

impl Default for Limits {
    fn default() -> Self {
        Limits {
            max_props: 200,
            max_constraints: 2000,
            max_ropeconstraints: 1000,
        }
    }
}

impl Limits {
    fn get(&self, kind: &str) -> i32 {
        match kind {
            "props" => self.max_props,
            "constraints" => self.max_constraints,
            "ropeconstraints" => self.max_ropeconstraints,
            _ => -1,
        }
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

#[derive(Clone, Debug, PartialEq)]
pub enum ConstraintKind {
    /// `constraint.Weld`: `phys_constraint`.
    Weld { forcelimit: f32, nocollide: bool },
    /// `constraint.Rope`: `phys_lengthconstraint`; local attachment points are in the entity
    /// frame (world position for the world).
    Rope {
        lpos1: Vec3,
        lpos2: Vec3,
        length: f32,
        addlength: f32,
        width: f32,
        material: String,
        rigid: bool,
    },
}

/// A constraint entity; `None` stands for the world.
#[derive(Clone, Debug, PartialEq)]
pub struct Constraint {
    pub id: u32,
    pub owner: u32,
    pub ent1: Option<u32>,
    pub ent2: Option<u32>,
    pub kind: ConstraintKind,
}

/// One object the tool gun has selected (`ToolObj:SetObject`): position and normal are kept
/// in the entity frame so they stay valid when it moves; for the world they are world space.
#[derive(Clone, Debug, PartialEq)]
pub struct ToolObject {
    pub ent: Option<u32>,
    pub pos: Vec3,
    pub normal: Vec3,
    pub bone: u32,
}

#[derive(Clone, Debug, Default, PartialEq)]
pub struct ToolGun {
    pub objects: Vec<ToolObject>,
    pub stage: u32,
}

/// Client tool settings the stools read with `GetClientNumber`/`GetClientInfo`.
#[derive(Clone, Debug, PartialEq)]
pub struct ToolSettings {
    pub weld_forcelimit: f32,
    pub weld_nocollide: bool,
    pub rope_forcelimit: f32,
    pub rope_addlength: f32,
    pub rope_width: f32,
    pub rope_material: String,
    pub rope_rigid: bool,
}

impl Default for ToolSettings {
    /// A player without the convars (a bot) reads 0 and "" everywhere.
    fn default() -> Self {
        ToolSettings {
            weld_forcelimit: 0.0,
            weld_nocollide: false,
            rope_forcelimit: 0.0,
            rope_addlength: 0.0,
            rope_width: 0.0,
            rope_material: String::new(),
            rope_rigid: false,
        }
    }
}

/// A prop as the tool trace sees it: pose plus collision hulls as outward planes (entity frame).
#[derive(Clone, Debug, PartialEq)]
pub struct PropBody {
    pub id: u32,
    pub pos: Vec3,
    pub angles: Vec3,
    pub hulls: Vec<Vec<(Vec3, f32)>>,
}

/// What the tool gun's trace hit (`SWEP:DoToolTrace`).
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct ToolHit {
    pub pos: Vec3,
    pub normal: Vec3,
    /// `None` is the world.
    pub ent: Option<u32>,
    pub fraction: f32,
}

/// `MAX_TRACE_LENGTH` of the engine, the reach of `util.GetPlayerTrace`.
pub const MAX_TRACE_LENGTH: f32 = 56756.0;

/// Outward planes of one convex from its triangle soup (PHY ledges).
pub fn hull_planes(points: &[Vec3], triangles: &[[u16; 3]]) -> Vec<(Vec3, f32)> {
    if points.is_empty() {
        return Vec::new();
    }
    let centre = points.iter().sum::<Vec3>() / points.len() as f32;
    let mut planes: Vec<(Vec3, f32)> = Vec::new();
    for t in triangles {
        let (a, b, c) = (
            points[t[0] as usize],
            points[t[1] as usize],
            points[t[2] as usize],
        );
        let mut n = (b - a).cross(c - a);
        if n.length_squared() < 1e-10 {
            continue;
        }
        n = n.normalize();
        let mut d = n.dot(a);
        if n.dot(centre) - d > 0.0 {
            n = -n;
            d = -d;
        }
        if planes
            .iter()
            .any(|(pn, pd)| pn.dot(n) > 0.9999 && (pd - d).abs() < 1e-3)
        {
            continue;
        }
        planes.push((n, d));
    }
    planes
}

fn ray_convex(start: Vec3, end: Vec3, planes: &[(Vec3, f32)]) -> Option<(f32, Vec3)> {
    let (mut t_in, mut t_out) = (0.0f32, 1.0f32);
    let mut normal = Vec3::ZERO;
    for &(n, d) in planes {
        let d1 = n.dot(start) - d;
        let d2 = n.dot(end) - d;
        if d1 > 0.0 && d2 > 0.0 {
            return None;
        }
        if d1 <= 0.0 && d2 <= 0.0 {
            continue;
        }
        let t = d1 / (d1 - d2);
        if d1 > d2 {
            if t > t_in {
                t_in = t;
                normal = n;
            }
        } else if t < t_out {
            t_out = t;
        }
    }
    (t_in <= t_out && normal != Vec3::ZERO).then_some((t_in, normal))
}

/// The tool gun's line trace from the eye: world brushes (and displacements) against the
/// props' collision hulls, nearest hit wins.
pub fn tool_trace(world: &World, props: &[PropBody], eye: Vec3, dir: Vec3) -> Option<ToolHit> {
    let end = eye + dir * MAX_TRACE_LENGTH;
    let tr = world.trace_ray(eye, end, MASK_SOLID);
    let mut best = tr.hit().then_some(ToolHit {
        pos: tr.end,
        normal: tr.normal,
        ent: None,
        fraction: tr.fraction,
    });
    for p in props {
        let rot = rotation(p.angles);
        let inv = rot.inverse();
        let (ls, le) = (inv * (eye - p.pos), inv * (end - p.pos));
        for hull in &p.hulls {
            if let Some((t, n)) = ray_convex(ls, le, hull) {
                if best.is_none_or(|b| t < b.fraction) {
                    best = Some(ToolHit {
                        pos: eye + (end - eye) * t,
                        normal: rot * n,
                        ent: Some(p.id),
                        fraction: t,
                    });
                }
            }
        }
    }
    best
}

#[derive(Default)]
pub struct Sandbox {
    pub limits: Limits,
    pub tool_settings: ToolSettings,
    next_id: u32,
    props: Vec<Prop>,
    constraints: Vec<Constraint>,
    tools: BTreeMap<u32, ToolGun>,
    undo: BTreeMap<u32, Vec<Undo>>,
    cleanup: BTreeMap<u32, BTreeMap<String, Vec<u32>>>,
    counts: BTreeMap<u32, BTreeMap<String, Vec<u32>>>,
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

    /// `Player:GetCount(kind)` for the counted kinds (`props`, `constraints`, `ropeconstraints`):
    /// entries whose entity is gone do not count.
    pub fn count_of(&self, player: u32, kind: &str) -> usize {
        if kind == "props" {
            return self.count(player);
        }
        self.counts
            .get(&player)
            .and_then(|m| m.get(kind))
            .map(|v| v.iter().filter(|&&id| self.exists(id)).count())
            .unwrap_or(0)
    }

    fn exists(&self, id: u32) -> bool {
        self.props.iter().any(|p| p.id == id) || self.constraints.iter().any(|c| c.id == id)
    }

    pub fn constraints(&self) -> &[Constraint] {
        &self.constraints
    }

    pub fn tool(&self, player: u32) -> ToolGun {
        self.tools.get(&player).cloned().unwrap_or_default()
    }

    pub fn undos(&self, player: u32) -> &[Undo] {
        self.undo.get(&player).map(Vec::as_slice).unwrap_or(&[])
    }

    /// `Player:CheckLimit(kind)`: refused once `count > max - 1`; negative max is unlimited.
    fn check_limit(&self, player: u32, kind: &str, events: &mut Vec<Event>) -> bool {
        let max = self.limits.get(kind);
        if max < 0 {
            return true;
        }
        if self.count_of(player, kind) as i32 > max - 1 {
            events.push(Event::LimitHit { limit: kind.into() });
            return false;
        }
        true
    }

    fn push_undo(
        &mut self,
        player: u32,
        name: &str,
        text: String,
        custom: Option<String>,
        entities: Vec<u32>,
    ) {
        let undos = self.undo.entry(player).or_default();
        let undo_id = undos.last().map(|u| u.id + 1).unwrap_or(1);
        undos.push(Undo {
            id: undo_id,
            name: name.into(),
            text,
            custom,
            entities,
        });
    }

    fn add_cleanup(&mut self, player: u32, kind: &str, id: u32) {
        self.cleanup
            .entry(player)
            .or_default()
            .entry(kind.into())
            .or_default()
            .push(id);
    }

    fn add_count(&mut self, player: u32, kind: &str, id: u32) {
        self.counts
            .entry(player)
            .or_default()
            .entry(kind.into())
            .or_default()
            .push(id);
    }

    /// `gm_spawn <model>` for a prop model.
    pub fn spawn_prop(
        &mut self,
        world: &World,
        player: u32,
        req: &SpawnRequest<'_>,
        events: &mut Vec<Event>,
    ) -> Option<u32> {
        if !self.check_limit(player, "props", events) {
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
        self.push_undo(
            player,
            "prop_physics",
            format!("#prop_physics ({model})"),
            None,
            vec![id],
        );
        self.add_cleanup(player, "props", id);
        Some(id)
    }

    /// Props that exist before the player does anything (placed by a scenario or a map): they
    /// get ids and poses but no undo entries or counts.
    pub fn place_existing(&mut self, owner: u32, bodies: &[PropBody], models: &[String]) {
        for (b, model) in bodies.iter().zip(models) {
            self.props.push(Prop {
                id: b.id,
                owner,
                model: model.clone(),
                pos: b.pos,
                angles: b.angles,
            });
            self.next_id = self.next_id.max(b.id + 1);
        }
    }

    /// Entity frame of a prop (`PhysObj:LocalToWorld` / `WorldToLocal`).
    fn prop_pose(&self, id: u32) -> Option<(Vec3, Quat)> {
        self.props
            .iter()
            .find(|p| p.id == id)
            .map(|p| (p.pos, rotation(p.angles)))
    }

    fn object_world_pos(&self, o: &ToolObject) -> Vec3 {
        match o.ent.and_then(|e| self.prop_pose(e)) {
            Some((pos, rot)) => pos + rot * o.pos,
            None => o.pos,
        }
    }

    /// `ToolObj:SetObject`: the hit is stored in the entity frame (world space for the world).
    fn set_object(&mut self, player: u32, hit: &ToolHit) -> usize {
        let obj = match hit.ent.and_then(|e| self.prop_pose(e)) {
            Some((pos, rot)) => {
                let inv = rot.inverse();
                ToolObject {
                    ent: hit.ent,
                    pos: inv * (hit.pos - pos),
                    normal: inv * (hit.normal + hit.pos - pos),
                    bone: 0,
                }
            }
            None => ToolObject {
                ent: None,
                pos: hit.pos,
                normal: hit.normal,
                bone: 0,
            },
        };
        let tool = self.tools.entry(player).or_default();
        tool.objects.push(obj);
        tool.objects.len()
    }

    fn clear_objects(&mut self, player: u32) {
        let tool = self.tools.entry(player).or_default();
        tool.objects.clear();
        tool.stage = 0;
    }

    /// `TOOL:Reload` of the constraint tools: forget the selected objects.
    pub fn tool_reload(&mut self, player: u32) -> bool {
        self.clear_objects(player);
        false
    }

    /// Left click of `weld` or `rope` on what the tool trace hit. Returns what `TOOL:LeftClick`
    /// returns (whether the gun fires its effect).
    pub fn tool_left_click(
        &mut self,
        player: u32,
        tool: &str,
        hit: &ToolHit,
        events: &mut Vec<Event>,
    ) -> bool {
        let n = self.set_object(player, hit);
        if n == 1 {
            self.tools.entry(player).or_default().stage = 1;
            return true;
        }
        let objs = self.tool(player).objects;
        let (o1, o2) = (&objs[0], &objs[1]);
        match tool {
            "weld" => {
                if !self.check_limit(player, "constraints", events) {
                    self.clear_objects(player);
                    return false;
                }
                let s = self.tool_settings.clone();
                if let Some(id) =
                    self.weld(player, o1.ent, o2.ent, s.weld_forcelimit, s.weld_nocollide)
                {
                    self.push_undo(
                        player,
                        "Weld",
                        "#tool.weld.name".into(),
                        Some("Undone #tool.weld.name".into()),
                        vec![id],
                    );
                    self.add_count(player, "constraints", id);
                    self.add_cleanup(player, "constraints", id);
                }
                self.clear_objects(player);
                true
            }
            "rope" => {
                if !self.check_limit(player, "ropeconstraints", events) {
                    self.clear_objects(player);
                    return false;
                }
                let s = self.tool_settings.clone();
                let (w1, w2) = (self.object_world_pos(o1), self.object_world_pos(o2));
                let length = (w1 - w2).length();
                let id = self.rope(player, o1.ent, o2.ent, o1.pos, o2.pos, length, &s);
                if let Some(id) = id {
                    self.push_undo(
                        player,
                        "Rope",
                        "#tool.rope.name".into(),
                        Some("Undone #tool.rope.name".into()),
                        vec![id],
                    );
                    self.add_count(player, "ropeconstraints", id);
                    self.add_cleanup(player, "ropeconstraints", id);
                }
                self.clear_objects(player);
                true
            }
            _ => {
                self.clear_objects(player);
                false
            }
        }
    }

    /// `constraint.Weld`: nothing for the same physics object, nothing when the pair is
    /// already welded, the world always goes second.
    pub fn weld(
        &mut self,
        owner: u32,
        ent1: Option<u32>,
        ent2: Option<u32>,
        forcelimit: f32,
        nocollide: bool,
    ) -> Option<u32> {
        if ent1 == ent2 {
            return None;
        }
        let exists = self.constraints.iter().any(|c| {
            matches!(c.kind, ConstraintKind::Weld { .. })
                && ((c.ent1 == ent1 && c.ent2 == ent2) || (c.ent1 == ent2 && c.ent2 == ent1))
        });
        if exists {
            return None;
        }
        let (ent1, ent2) = if ent1.is_none() {
            (ent2, ent1)
        } else {
            (ent1, ent2)
        };
        let id = self.next_id;
        self.next_id += 1;
        self.constraints.push(Constraint {
            id,
            owner,
            ent1,
            ent2,
            kind: ConstraintKind::Weld {
                forcelimit,
                nocollide,
            },
        });
        Some(id)
    }

    /// `constraint.Rope`: a `phys_lengthconstraint` of the measured length plus `addlength`
    /// between two different physics objects; the same object twice makes no constraint (and at
    /// width 0 no rope either).
    #[allow(clippy::too_many_arguments)]
    pub fn rope(
        &mut self,
        owner: u32,
        ent1: Option<u32>,
        ent2: Option<u32>,
        lpos1: Vec3,
        lpos2: Vec3,
        length: f32,
        s: &ToolSettings,
    ) -> Option<u32> {
        if ent1 == ent2 {
            return None;
        }
        let addlength = s.rope_addlength.clamp(-MAX_TRACE_LENGTH, MAX_TRACE_LENGTH);
        let id = self.next_id;
        self.next_id += 1;
        self.constraints.push(Constraint {
            id,
            owner,
            ent1,
            ent2,
            kind: ConstraintKind::Rope {
                lpos1,
                lpos2,
                length,
                addlength,
                width: s.rope_width,
                material: s.rope_material.clone(),
                rigid: s.rope_rigid,
            },
        });
        Some(id)
    }

    /// Removes an entity; a prop takes its constraints with it.
    fn remove(&mut self, id: u32) -> bool {
        let props_before = self.props.len();
        self.props.retain(|p| p.id != id);
        if self.props.len() != props_before {
            self.constraints
                .retain(|c| c.ent1 != Some(id) && c.ent2 != Some(id));
            return true;
        }
        let before = self.constraints.len();
        self.constraints.retain(|c| c.id != id);
        self.constraints.len() != before
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
        if let Some(lists) = self.cleanup.remove(&player) {
            for id in lists.into_values().flatten() {
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
        let mut sb = Sandbox::new(Limits {
            max_props: 2,
            ..Limits::default()
        });
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

    #[test]
    fn weld_and_rope_through_the_tool() {
        let w = floor();
        let mut sb = Sandbox::new(Limits::default());
        let mut ev = Vec::new();
        let eye = Vec3::new(823.235, -500.0, -79.0);
        let b = crate_bounds();
        let a = sb
            .spawn_prop(
                &w,
                1,
                &SpawnRequest {
                    eye,
                    eye_angles: Vec3::new(30.0, 0.0, 0.0),
                    model: "m",
                    bounds: b,
                },
                &mut ev,
            )
            .unwrap();
        let c = sb
            .spawn_prop(
                &w,
                1,
                &SpawnRequest {
                    eye,
                    eye_angles: Vec3::new(30.0, 40.0, 0.0),
                    model: "m",
                    bounds: b,
                },
                &mut ev,
            )
            .unwrap();
        let hit = |ent: Option<u32>, pos: Vec3| ToolHit {
            pos,
            normal: Vec3::Z,
            ent,
            fraction: 0.5,
        };
        assert!(sb.tool_left_click(
            1,
            "weld",
            &hit(Some(a), Vec3::new(935.0, -500.0, -104.0)),
            &mut ev
        ));
        assert_eq!(sb.tool(1).stage, 1);
        assert!(sb.tool_left_click(
            1,
            "weld",
            &hit(Some(c), Vec3::new(900.0, -440.0, -104.0)),
            &mut ev
        ));
        assert_eq!(sb.tool(1).stage, 0);
        assert_eq!(sb.count_of(1, "constraints"), 1);
        // same pair again: no second weld
        sb.tool_left_click(1, "weld", &hit(Some(c), Vec3::ZERO), &mut ev);
        sb.tool_left_click(1, "weld", &hit(Some(a), Vec3::ZERO), &mut ev);
        assert_eq!(sb.count_of(1, "constraints"), 1);
        // rope to the world: length between the attachment points
        sb.tool_left_click(
            1,
            "rope",
            &hit(Some(a), Vec3::new(935.0, -500.0, -104.0)),
            &mut ev,
        );
        sb.tool_left_click(
            1,
            "rope",
            &hit(None, Vec3::new(935.0, -500.0, -144.0)),
            &mut ev,
        );
        let ConstraintKind::Rope { length, .. } = &sb.constraints().last().unwrap().kind else {
            panic!()
        };
        assert!((length - 40.0).abs() < 1e-3);
        assert_eq!(
            sb.undos(1)
                .iter()
                .map(|u| u.name.as_str())
                .collect::<Vec<_>>(),
            ["prop_physics", "prop_physics", "Weld", "Rope"]
        );
        sb.undo(1, &mut ev);
        assert_eq!(sb.count_of(1, "ropeconstraints"), 0);
        // removing a prop takes its weld along
        sb.undo(1, &mut ev);
        sb.undo(1, &mut ev);
        assert_eq!(sb.count_of(1, "constraints"), 0);
    }
}
