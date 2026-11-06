//! Brush collision against a compiled BSP: swept axis-aligned boxes and rays
//! through the node tree, clipping against brush planes (brush sides include
//! bevel planes, so box sweeps only need the plane offset by the box extents).
//! Displacement collision is not implemented yet.

use glam::Vec3;
use gm_formats::bsp::lumps::contents;
use gm_formats::bsp::Bsp;

/// Distance kept between a moving box and the surface it stops at, as the
/// original engine does (1/32 unit).
pub const DIST_EPSILON: f32 = 1.0 / 32.0;

pub const MASK_SOLID: i32 =
    contents::SOLID | contents::MOVEABLE | contents::WINDOW | contents::MONSTER | contents::GRATE;
pub const MASK_PLAYERSOLID: i32 = MASK_SOLID | contents::PLAYERCLIP;
pub const MASK_WATER: i32 = contents::WATER | contents::SLIME;

#[derive(Clone, Copy, Debug)]
struct Plane {
    normal: Vec3,
    dist: f32,
}

#[derive(Clone, Copy, Debug)]
struct Node {
    plane: u32,
    children: [i32; 2],
}

#[derive(Clone, Copy, Debug)]
struct Leaf {
    contents: i32,
    first_brush: u32,
    num_brushes: u32,
    /// Visibility cluster (-1 for solid leafs).
    cluster: i16,
}

#[derive(Clone, Copy, Debug)]
struct Brush {
    first_side: u32,
    num_sides: u32,
    contents: i32,
}

/// Displacement triangles are a thin shell this thick (units) below their face.
const DISP_THICKNESS: f32 = 4.0;

struct DispTri {
    v: [Vec3; 3],
    normal: Vec3,
    dist: f32,
    /// The triangle is the blend's second material (`$surfaceprop2`): the alpha of its three
    /// vertices sums past 382.5 (the SDK's builddisp rule, used as the reference).
    second: bool,
}

/// One displacement surface: its triangles and bounds. The engine traces displacements as
/// one-sided triangle soup on top of the brushes (the brush under a displacement is not solid
/// to the trace; gm_construct's ground at 600, -500 is 4 u above its brush).
struct Disp {
    mins: Vec3,
    maxs: Vec3,
    tris: Vec<DispTri>,
    /// Index into `World::materials` of the displaced face's material, `u32::MAX` for none,
    /// and of the same name with `#2` for the triangles of the blend's second material.
    material: u32,
    material2: u32,
}

/// A solid convex in the world that is not part of the map (a prop's PHY hull at its pose):
/// outward planes `n . x <= d` and its AABB, tagged with the entity it belongs to.
#[derive(Clone, Debug, PartialEq)]
pub struct Convex {
    pub entity: u32,
    /// Ground friction factor of its surface for the player (`min(1, friction * 1.25)` of the
    /// surface property: glass 0.625, plastic/concrete 1).
    pub friction: f32,
    pub planes: Vec<(Vec3, f32)>,
    pub mins: Vec3,
    pub maxs: Vec3,
}

/// Sweeps the box `mins..maxs` from `start` to `end` against `convexes` the way the brush
/// traces do (planes pushed out by the box extents, `DIST_EPSILON` skin): the nearest hit, or
/// a trace with fraction 1 at `end`. `entity` of the hit is returned alongside.
pub fn sweep_convexes(
    start: Vec3,
    end: Vec3,
    mins: Vec3,
    maxs: Vec3,
    convexes: &[Convex],
) -> (Trace, Option<u32>) {
    sweep_convexes_skin(start, end, mins, maxs, convexes, 0.0)
}

/// `sweep_convexes` with every convex's faces pushed out by `skin` for a box that starts
/// outside the pushed-out hull; a box already within the skin sweeps the bare hull.
pub fn sweep_convexes_skin(
    start: Vec3,
    end: Vec3,
    mins: Vec3,
    maxs: Vec3,
    convexes: &[Convex],
    skin: f32,
) -> (Trace, Option<u32>) {
    let center = (mins + maxs) * 0.5;
    let extents = (maxs - mins) * 0.5;
    let (cs, ce) = (start + center, end + center);
    let mut tr = Trace {
        fraction: 1.0,
        end,
        normal: Vec3::ZERO,
        plane_dist: 0.0,
        start_solid: false,
        all_solid: false,
        contents: 0,
        surface: None,
    };
    let mut hit = None;
    let lo = cs.min(ce) - extents;
    let hi = cs.max(ce) + extents;
    for c in convexes {
        if c.mins.cmpgt(hi).any() || c.maxs.cmplt(lo).any() {
            continue;
        }
        let within_skin = skin > 0.0
            && c.planes
                .iter()
                .all(|&(n, d)| n.dot(cs) - (d + n.abs().dot(extents) + skin) <= 0.0);
        let skin = if within_skin { 0.0 } else { skin };
        let mut enter_frac = -1.0f32;
        let mut leave_frac = 1.0f32;
        let mut clip: Option<(Vec3, f32)> = None;
        let mut get_out = false;
        let mut starts_out = false;
        let mut miss = false;
        for &(n, d) in &c.planes {
            let dist = d + n.abs().dot(extents) + skin;
            let d1 = n.dot(cs) - dist;
            let d2 = n.dot(ce) - dist;
            if d2 > 0.0 {
                get_out = true;
            }
            if d1 > 0.0 {
                starts_out = true;
            }
            if d1 > 0.0 && d2 >= d1 {
                miss = true;
                break;
            }
            if d1 <= 0.0 && d2 <= 0.0 {
                continue;
            }
            if d1 > d2 {
                let f = (d1 - DIST_EPSILON) / (d1 - d2);
                if f > enter_frac {
                    enter_frac = f;
                    clip = Some((n, dist));
                }
            } else {
                let f = (d1 + DIST_EPSILON) / (d1 - d2);
                if f < leave_frac {
                    leave_frac = f;
                }
            }
        }
        if miss {
            continue;
        }
        if !starts_out {
            tr.start_solid = true;
            tr.contents |= contents::SOLID;
            if !get_out {
                tr.all_solid = true;
                tr.fraction = 0.0;
                tr.end = start;
                hit = Some(c.entity);
            }
            continue;
        }
        if enter_frac < leave_frac && enter_frac > -1.0 && enter_frac < tr.fraction {
            let (normal, dist) = clip.expect("entering plane");
            tr.fraction = enter_frac.max(0.0);
            tr.normal = normal;
            tr.plane_dist = dist;
            tr.contents = contents::SOLID;
            tr.end = start + (end - start) * tr.fraction;
            hit = Some(c.entity);
        }
    }
    (tr, hit)
}

/// A `func_useableladder`: the line the player climbs, bottom to top (mount points).
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Ladder {
    pub bottom: Vec3,
    pub top: Vec3,
}

/// A brush entity's model in the collision world: the head node of its tree and the
/// entity origin its geometry is stored relative to.
#[derive(Clone, Copy, Debug, PartialEq)]
struct BrushModel {
    head_node: i32,
    origin: Vec3,
}

/// Brush entities that are solid to the player and to physics in a fresh map: `(model index,
/// origin)` of every `*N` entity whose class is solid at spawn (`func_brush` unless
/// `Solidity 1` / `StartDisabled 1`, doors, buttons, walls, breakables, movers, glass).
/// `func_illusionary`, `func_vehicleclip`, triggers, areaportals and occluders are not.
/// Brush entity classes whose pose or solidity changes at run time (doors, platforms,
/// toggled walls, buttons): the server poses them itself as convexes, they are not part of
/// the static world.
pub fn movable_brush_class(class: &str) -> bool {
    matches!(
        class,
        "func_brush"
            | "func_wall_toggle"
            | "func_button"
            | "func_rot_button"
            | "func_door"
            | "func_door_rotating"
            | "func_movelinear"
            | "func_rotating"
            | "func_platrot"
            | "func_train"
            | "func_tracktrain"
            | "func_tanktrain"
            | "func_physbox"
            | "func_physbox_multiplayer"
    )
}

/// A brush entity the server never moves or toggles: not a movable class, or a `func_brush`
/// / `func_wall_toggle` without a targetname (nothing can send it an input) that starts
/// solid. These stay in the static world; the rest are the server's (`gm-server::mapio`).
pub fn brush_entity_is_static(e: &gm_formats::bsp::lumps::Entity) -> bool {
    let class = e.classname();
    if !movable_brush_class(class) {
        return true;
    }
    let unnamed = e.get("targetname").is_none_or(|n| n.is_empty());
    match class {
        "func_brush" => {
            unnamed && e.get("Solidity") != Some("1") && e.get("StartDisabled") != Some("1")
        }
        "func_wall_toggle" => {
            unnamed
                && e.get("spawnflags")
                    .and_then(|f| f.parse::<u32>().ok())
                    .unwrap_or(0)
                    & 1
                    == 0
        }
        _ => false,
    }
}

pub fn solid_brush_models(bsp: &Bsp<'_>) -> Vec<(usize, Vec3)> {
    const SOLID: &[&str] = &[
        "func_brush",
        "func_wall",
        "func_wall_toggle",
        "func_breakable",
        "func_button",
        "func_rot_button",
        "func_door",
        "func_door_rotating",
        "func_movelinear",
        "func_rotating",
        "func_platrot",
        "func_train",
        "func_tracktrain",
        "func_tanktrain",
        "func_lod",
        "func_monitor",
        "func_reflective_glass",
        "func_physbox",
        "func_physbox_multiplayer",
        "func_conveyor",
        "func_guntarget",
        "func_detail_blocker",
    ];
    bsp.entities
        .iter()
        .filter(|e| SOLID.contains(&e.classname()) && brush_entity_is_static(e))
        .filter_map(|e| {
            let idx: usize = e.get("model")?.strip_prefix('*')?.parse().ok()?;
            let origin = e
                .get("origin")
                .and_then(gm_formats::bsp::lumps::parse_vec3)
                .map(Vec3::from)
                .unwrap_or(Vec3::ZERO);
            Some((idx, origin))
        })
        .collect()
}

pub struct World {
    disps: Vec<Disp>,
    ladders: Vec<Ladder>,
    planes: Vec<Plane>,
    nodes: Vec<Node>,
    leafs: Vec<Leaf>,
    leaf_brushes: Vec<u16>,
    brushes: Vec<Brush>,
    side_planes: Vec<u32>,
    /// Per brush side: its texture is the sky (`SURF_SKY`/`SURF_SKY2D`); empty when unknown.
    side_sky: Vec<bool>,
    /// Per brush side: index into `materials`, `u32::MAX` when it has none; empty when unknown.
    side_material: Vec<u32>,
    /// Material names (`concrete/concretefloor001a`, lower case) the brush sides use.
    materials: Vec<String>,
    head_node: i32,
    /// Solid brush entities (`func_brush`, doors, buttons...): each one's own tree, with its
    /// geometry relative to the entity's `origin`.
    brush_models: Vec<BrushModel>,
    /// PVS: `vis[c]` is a bitset over clusters visible from cluster `c`; empty when the map has
    /// no visibility lump.
    vis: Vec<Vec<u64>>,
    num_clusters: usize,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Trace {
    /// 0..1 along the sweep; 1 means nothing was hit.
    pub fraction: f32,
    pub end: Vec3,
    pub normal: Vec3,
    pub plane_dist: f32,
    /// The start position was inside a solid.
    pub start_solid: bool,
    /// The whole sweep was inside a solid.
    pub all_solid: bool,
    pub contents: i32,
    /// Index into `World::materials` of the displacement triangle hit, when one was.
    pub surface: Option<u32>,
}

impl Trace {
    pub fn hit(&self) -> bool {
        self.fraction < 1.0
    }
}

struct Sweep {
    start: Vec3,
    end: Vec3,
    extents: Vec3,
    is_point: bool,
    mask: i32,
    fraction: f32,
    normal: Vec3,
    plane_dist: f32,
    start_solid: bool,
    all_solid: bool,
    contents: i32,
    surface: Option<u32>,
    checked: Vec<u32>,
}

impl World {
    pub fn from_bsp(bsp: &Bsp<'_>) -> Self {
        let mut materials: Vec<String> = Vec::new();
        let (vis, num_clusters) = match gm_formats::bsp::extra::Visibility::parse(bsp) {
            Ok(v) if v.num_clusters > 0 => {
                let words = v.num_clusters.div_ceil(64);
                let table: Vec<Vec<u64>> = (0..v.num_clusters)
                    .map(|c| {
                        let mut row = vec![0u64; words];
                        if let Ok(bits) = v.pvs(bsp, c) {
                            for (i, b) in bits.iter().enumerate() {
                                if *b {
                                    row[i / 64] |= 1 << (i % 64);
                                }
                            }
                        }
                        row
                    })
                    .collect();
                (table, v.num_clusters)
            }
            _ => (Vec::new(), 0),
        };
        let ladders = bsp
            .entities
            .iter()
            .filter(|e| e.classname() == "func_useableladder")
            .filter_map(|e| {
                // the map's `point0`/`point1` are world positions (the entity's origin is
                // where Hammer placed the helper, not a base for them)
                let a = Vec3::from(gm_formats::bsp::lumps::parse_vec3(e.get("point0")?)?);
                let b = Vec3::from(gm_formats::bsp::lumps::parse_vec3(e.get("point1")?)?);
                Some(if a.z <= b.z {
                    Ladder { bottom: a, top: b }
                } else {
                    Ladder { bottom: b, top: a }
                })
            })
            .collect();
        World {
            ladders,
            disps: displacements(bsp, &mut materials),
            planes: bsp
                .planes
                .iter()
                .map(|p| Plane {
                    normal: Vec3::from(p.normal),
                    dist: p.dist,
                })
                .collect(),
            nodes: bsp
                .nodes
                .iter()
                .map(|n| Node {
                    plane: n.plane_num as u32,
                    children: n.children,
                })
                .collect(),
            leafs: bsp
                .leafs
                .iter()
                .map(|l| Leaf {
                    contents: l.contents,
                    first_brush: l.first_leaf_brush as u32,
                    num_brushes: l.num_leaf_brushes as u32,
                    cluster: l.cluster,
                })
                .collect(),
            leaf_brushes: bsp.leaf_brushes.clone(),
            brushes: bsp
                .brushes
                .iter()
                .map(|b| Brush {
                    first_side: b.first_side.max(0) as u32,
                    num_sides: b.num_sides.max(0) as u32,
                    contents: b.contents,
                })
                .collect(),
            side_planes: bsp.brush_sides.iter().map(|s| s.plane_num as u32).collect(),
            side_material: bsp
                .brush_sides
                .iter()
                .map(|s| {
                    let ti = usize::try_from(s.tex_info)
                        .ok()
                        .and_then(|i| bsp.tex_infos.get(i));
                    let name = ti
                        .and_then(|ti| bsp.tex_datas.get(usize::try_from(ti.tex_data).ok()?))
                        .and_then(|td| {
                            bsp.tex_names
                                .get(usize::try_from(td.name_string_table_id).ok()?)
                        });
                    match name {
                        Some(n) => {
                            let n = n.replace('\\', "/").to_ascii_lowercase();
                            if let Some(i) = materials.iter().position(|m| *m == n) {
                                i as u32
                            } else {
                                materials.push(n);
                                (materials.len() - 1) as u32
                            }
                        }
                        None => u32::MAX,
                    }
                })
                .collect(),
            materials,
            side_sky: bsp
                .brush_sides
                .iter()
                .map(|s| {
                    usize::try_from(s.tex_info)
                        .ok()
                        .and_then(|i| bsp.tex_infos.get(i))
                        .is_some_and(|t| {
                            t.flags
                                & (gm_formats::bsp::lumps::surf::SKY
                                    | gm_formats::bsp::lumps::surf::SKY2D)
                                != 0
                        })
                })
                .collect(),
            head_node: bsp.models.first().map(|m| m.head_node).unwrap_or(0),
            brush_models: solid_brush_models(bsp)
                .into_iter()
                .filter_map(|(idx, origin)| {
                    Some(BrushModel {
                        head_node: bsp.models.get(idx)?.head_node,
                        origin,
                    })
                })
                .collect(),
            vis,
            num_clusters,
        }
    }

    /// A world made of axis-aligned solid boxes with no tree (one leaf), for tests and tools.
    pub fn from_boxes(boxes: &[(Vec3, Vec3)]) -> Self {
        let boxes: Vec<_> = boxes
            .iter()
            .map(|(mins, maxs)| (*mins, *maxs, contents::SOLID))
            .collect();
        Self::from_boxes_contents(&boxes)
    }

    /// `from_boxes` with each box's contents (`contents::WATER` for a pool).
    pub fn from_boxes_contents(boxes: &[(Vec3, Vec3, i32)]) -> Self {
        let mut planes = Vec::new();
        let mut side_planes = Vec::new();
        let mut brushes = Vec::new();
        for (mins, maxs, contents) in boxes {
            let first = side_planes.len() as u32;
            for (n, d) in [
                (Vec3::X, maxs.x),
                (Vec3::NEG_X, -mins.x),
                (Vec3::Y, maxs.y),
                (Vec3::NEG_Y, -mins.y),
                (Vec3::Z, maxs.z),
                (Vec3::NEG_Z, -mins.z),
            ] {
                side_planes.push(planes.len() as u32);
                planes.push(Plane { normal: n, dist: d });
            }
            brushes.push(Brush {
                first_side: first,
                num_sides: 6,
                contents: *contents,
            });
        }
        let leaf_brushes: Vec<u16> = (0..brushes.len() as u16).collect();
        World {
            disps: Vec::new(),
            ladders: Vec::new(),
            planes,
            nodes: Vec::new(),
            leafs: vec![Leaf {
                contents: 0,
                first_brush: 0,
                num_brushes: brushes.len() as u32,
                cluster: 0,
            }],
            leaf_brushes,
            brushes,
            side_planes,
            side_sky: Vec::new(),
            side_material: Vec::new(),
            materials: Vec::new(),
            head_node: -1,
            brush_models: Vec::new(),
            vis: Vec::new(),
            num_clusters: 0,
        }
    }

    pub fn leaf_index(&self, point: Vec3) -> usize {
        let mut node = self.head_node;
        while node >= 0 {
            let n = &self.nodes[node as usize];
            let p = &self.planes[n.plane as usize];
            node = if p.normal.dot(point) - p.dist >= 0.0 {
                n.children[0]
            } else {
                n.children[1]
            };
        }
        (-node - 1) as usize
    }

    /// Visibility cluster of the leaf at `point` (-1 in solid).
    pub fn cluster_at(&self, point: Vec3) -> i32 {
        self.leafs[self.leaf_index(point)].cluster as i32
    }

    /// Whether anything in cluster `to` can be seen from cluster `from` (PVS); true when the map
    /// has no visibility data, false when either point is in solid.
    pub fn visible(&self, from: i32, to: i32) -> bool {
        if self.vis.is_empty() {
            return true;
        }
        if from < 0
            || to < 0
            || from as usize >= self.num_clusters
            || to as usize >= self.num_clusters
        {
            return false;
        }
        self.vis[from as usize][to as usize / 64] >> (to as usize % 64) & 1 == 1
    }

    pub fn num_clusters(&self) -> usize {
        self.num_clusters
    }

    /// Contents of the brushes containing `point` (0 when in open space).
    pub fn point_contents(&self, point: Vec3) -> i32 {
        let leaf = &self.leafs[self.leaf_index(point)];
        if leaf.num_brushes == 0 {
            return leaf.contents;
        }
        let mut c = 0;
        for i in 0..leaf.num_brushes {
            let b = &self.brushes[self.leaf_brushes[(leaf.first_brush + i) as usize] as usize];
            if self.brush_contains(b, point) {
                c |= b.contents;
            }
        }
        for bm in &self.brush_models {
            c |= self.model_contents(bm, point - bm.origin);
        }
        c
    }

    fn model_contents(&self, bm: &BrushModel, point: Vec3) -> i32 {
        let mut node = bm.head_node;
        while node >= 0 {
            let n = &self.nodes[node as usize];
            let p = &self.planes[n.plane as usize];
            node = if p.normal.dot(point) - p.dist >= 0.0 {
                n.children[0]
            } else {
                n.children[1]
            };
        }
        let leaf = &self.leafs[(-node - 1) as usize];
        let mut c = 0;
        for i in 0..leaf.num_brushes {
            let b = &self.brushes[self.leaf_brushes[(leaf.first_brush + i) as usize] as usize];
            if self.brush_contains(b, point) {
                c |= b.contents;
            }
        }
        c
    }

    fn brush_contains(&self, b: &Brush, point: Vec3) -> bool {
        (0..b.num_sides).all(|i| {
            let p = &self.planes[self.side_planes[(b.first_side + i) as usize] as usize];
            p.normal.dot(point) - p.dist <= 0.0
        })
    }

    /// Ray trace.
    /// Whether the surface at `point` (on it, within 0.1 u), facing `normal`, is a sky brush
    /// side (a box trace ends its box extent plus 1/32 before the surface; pass the surface point): the solid brush touching the point whose side plane matches, by its texture flags.
    pub fn is_sky(&self, point: Vec3, normal: Vec3) -> bool {
        if self.side_sky.is_empty() {
            return false;
        }
        self.side_at(point, normal)
            .is_some_and(|si| self.side_sky.get(si).copied().unwrap_or(false))
    }

    /// Material of the brush side at `point` facing `normal` (same contact rule as [`is_sky`]):
    /// what a player standing there steps on.
    pub fn surface_material(&self, point: Vec3, normal: Vec3) -> Option<&str> {
        let mi = match self.side_at(point, normal) {
            Some(si) => *self.side_material.get(si)?,
            None => self.disp_at(point, normal)?,
        };
        self.materials.get(mi as usize).map(String::as_str)
    }

    /// Material of the displacement triangle `point` rests on: the one facing `normal` whose
    /// bounds hold the point and whose plane is nearest (a hull on an edge bevel sits up to a
    /// unit above the plane).
    fn disp_at(&self, point: Vec3, normal: Vec3) -> Option<u32> {
        const EPS: f32 = 0.5;
        let mut best: Option<(f32, u32)> = None;
        for d in &self.disps {
            if d.material == u32::MAX
                || point.cmplt(d.mins - EPS).any()
                || point.cmpgt(d.maxs + EPS).any()
            {
                continue;
            }
            for t in &d.tris {
                let off = (t.normal.dot(point) - t.dist).abs();
                if t.normal.dot(normal) < 0.99 || off > DISP_THICKNESS {
                    continue;
                }
                let lo = t.v[0].min(t.v[1]).min(t.v[2]) - EPS;
                let hi = t.v[0].max(t.v[1]).max(t.v[2]) + EPS;
                if point.cmpge(lo).all()
                    && point.cmple(hi).all()
                    && best.is_none_or(|(b, _)| off < b)
                {
                    best = Some((off, if t.second { d.material2 } else { d.material }));
                }
            }
        }
        best.map(|(_, m)| m)
    }

    /// Material under a standing player's feet (`origin` at the feet): a short ray down finds
    /// the floor the player rests on, which can be up to a step below the origin (an idle player
    /// on gm_construct's spawn stands 1 u above the floor brush).
    pub fn ladders(&self) -> &[Ladder] {
        &self.ladders
    }

    /// Adds ladders to a built world (tests, tools).
    pub fn with_ladders(mut self, ladders: &[Ladder]) -> Self {
        self.ladders.extend_from_slice(ladders);
        self
    }

    pub fn ground_material(&self, origin: Vec3) -> Option<&str> {
        // the player hull's footprint, as the game's ground trace: on a displacement the hull
        // rests on the triangle its sweep clipped against, not the one under the origin
        // (`move_water_walk` ticks 68 and 116 on the shore of gm_construct's pond)
        let (mins, maxs) = (Vec3::new(-16.0, -16.0, 0.0), Vec3::new(16.0, 16.0, 36.0));
        let t = self.trace_box(
            origin + Vec3::Z,
            origin - Vec3::Z * 4.0,
            mins,
            maxs,
            MASK_PLAYERSOLID,
        );
        if t.fraction >= 1.0 || t.all_solid {
            return None;
        }
        match t.surface {
            Some(m) => self.materials.get(m as usize).map(String::as_str),
            None => self.surface_material(t.end, t.normal),
        }
    }

    /// Every material the brush sides use.
    pub fn materials(&self) -> &[String] {
        &self.materials
    }

    fn side_at(&self, point: Vec3, normal: Vec3) -> Option<usize> {
        const EPS: f32 = 0.1;
        'brush: for b in &self.brushes {
            if b.contents & contents::SOLID == 0 {
                continue;
            }
            let mut on_side: Option<usize> = None;
            for i in 0..b.num_sides {
                let si = (b.first_side + i) as usize;
                let p = &self.planes[self.side_planes[si] as usize];
                let d = p.normal.dot(point) - p.dist;
                if d > EPS {
                    continue 'brush;
                }
                if d.abs() <= EPS && p.normal.dot(normal) > 0.99 {
                    on_side = Some(si);
                }
            }
            if let Some(si) = on_side {
                return Some(si);
            }
        }
        None
    }

    pub fn trace_ray(&self, start: Vec3, end: Vec3, mask: i32) -> Trace {
        self.trace_box(start, end, Vec3::ZERO, Vec3::ZERO, mask)
    }

    /// Sweep an axis-aligned box from `start` to `end`.
    pub fn trace_box(&self, start: Vec3, end: Vec3, mins: Vec3, maxs: Vec3, mask: i32) -> Trace {
        let is_point = mins == Vec3::ZERO && maxs == Vec3::ZERO;
        let mut sw = Sweep {
            start,
            end,
            extents: (maxs - mins) * 0.5,
            is_point,
            mask,
            fraction: 1.0,
            normal: Vec3::ZERO,
            plane_dist: 0.0,
            start_solid: false,
            all_solid: false,
            contents: 0,
            surface: None,
            checked: Vec::new(),
        };
        // The engine traces the box centre with symmetric extents; shift for asymmetric boxes.
        let offset = (mins + maxs) * 0.5;
        sw.start += offset;
        sw.end += offset;
        if is_point {
            sw.extents = Vec3::ZERO;
        }
        let (s0, e0) = (sw.start, sw.end);
        self.trace_node(&mut sw, self.head_node, 0.0, 1.0, s0, e0);
        for bm in &self.brush_models {
            let before = sw.fraction;
            let (s, e) = (s0 - bm.origin, e0 - bm.origin);
            sw.start = s;
            sw.end = e;
            self.trace_node(&mut sw, bm.head_node, 0.0, 1.0, s, e);
            if sw.fraction < before {
                sw.plane_dist += sw.normal.dot(bm.origin);
            }
        }
        sw.start = s0;
        sw.end = e0;
        if sw.mask & contents::SOLID != 0 {
            self.trace_disps(&mut sw);
        }
        let end = if sw.fraction >= 1.0 {
            end
        } else {
            start + (end - start) * sw.fraction
        };
        Trace {
            fraction: sw.fraction,
            end,
            normal: sw.normal,
            plane_dist: sw.plane_dist,
            start_solid: sw.start_solid,
            all_solid: sw.all_solid,
            contents: sw.contents,
            surface: sw.surface,
        }
    }

    fn trace_disps(&self, sw: &mut Sweep) {
        let pad = sw.extents + Vec3::splat(DIST_EPSILON);
        let lo = sw.start.min(sw.end) - pad;
        let hi = sw.start.max(sw.end) + pad;
        for d in &self.disps {
            if d.mins.cmpgt(hi).any() || d.maxs.cmplt(lo).any() {
                continue;
            }
            for t in &d.tris {
                let (tlo, thi) = (
                    t.v[0].min(t.v[1]).min(t.v[2]),
                    t.v[0].max(t.v[1]).max(t.v[2]),
                );
                if tlo.cmpgt(hi).any() || thi.cmplt(lo).any() {
                    continue;
                }
                self.trace_tri(sw, t, if t.second { d.material2 } else { d.material });
            }
        }
    }

    /// Sweep against one triangle as a one-sided bevelled convex: its plane, the axis planes of
    /// its bounds and the edge-by-axis bevels, each pushed out by the box extents.
    fn trace_tri(&self, sw: &mut Sweep, t: &DispTri, material: u32) {
        let expand = |n: Vec3| {
            if sw.is_point {
                0.0
            } else {
                n.abs().dot(sw.extents)
            }
        };
        // the face is one clip plane among the others, not a gate: a hull resting on an edge
        // bevel sits behind the extended face plane of that triangle (the pond shore of
        // gm_construct, `move_water_walk` tick 61) and its ground trace must still find it.
        // A back plane makes the triangle a thin shell so the space below a displacement
        // stays open
        let mut enter_frac = -1.0f32;
        let mut leave_frac = 1.0f32;
        let mut clip_plane = (t.normal, t.dist + expand(t.normal));
        let mut start_out = false;
        let mut clip = |normal: Vec3, plane_dist: f32| -> bool {
            let dist = plane_dist + expand(normal);
            let d1 = normal.dot(sw.start) - dist;
            let d2 = normal.dot(sw.end) - dist;
            if d1 > 0.0 {
                start_out = true;
            }
            if d1 > 0.0 && d2 >= d1 {
                return false;
            }
            if d1 <= 0.0 && d2 <= 0.0 {
                return true;
            }
            if d1 > d2 {
                let f = (d1 - DIST_EPSILON) / (d1 - d2);
                if f > enter_frac {
                    enter_frac = f;
                    clip_plane = (normal, dist);
                }
            } else {
                let f = (d1 + DIST_EPSILON) / (d1 - d2);
                if f < leave_frac {
                    leave_frac = f;
                }
            }
            true
        };
        if !clip(t.normal, t.dist) || !clip(-t.normal, DISP_THICKNESS - t.dist) {
            return;
        }
        for axis in [Vec3::X, Vec3::Y, Vec3::Z] {
            // the bound slab along the face normal of an axis-aligned triangle has no
            // thickness: as a leave plane it meets the enter fraction to float precision and
            // rejected point rays through flat ground (the face plane already clips it)
            if t.normal.dot(axis).abs() > 0.999 {
                continue;
            }
            let hi = t.v.iter().map(|v| v.dot(axis)).fold(f32::MIN, f32::max);
            let lo = t.v.iter().map(|v| v.dot(axis)).fold(f32::MAX, f32::min);
            if !clip(axis, hi) || !clip(-axis, -lo) {
                return;
            }
        }
        for i in 0..3 {
            let (a, b, c) = (t.v[i], t.v[(i + 1) % 3], t.v[(i + 2) % 3]);
            let e = b - a;
            let mut bevels = [Vec3::X, Vec3::Y, Vec3::Z].map(|axis| e.cross(axis));
            bevels[0] = if bevels[0].length_squared() < 1e-6 {
                e.cross(t.normal)
            } else {
                bevels[0]
            };
            for m in [bevels[0], bevels[1], bevels[2], e.cross(t.normal)] {
                if m.length_squared() < 1e-6 {
                    continue;
                }
                let mut m = m.normalize();
                // an edge bevel parallel to the face normal is the face plane again (an edge in
                // the xy plane crossed with X or Y gives +-Z); as a leave plane it meets the
                // enter fraction to float precision and rejected point rays through flat
                // ground on gm_construct (`move_settle_edge`). A point needs no edge-by-axis
                // bevels at all; a box needs even the nearly parallel ones: on the shore of
                // gm_construct's pond (`move_water_walk` tick 48) a 1 degree bevel is what
                // keeps the player hull off the extended plane of the next triangle, a
                // 0.93 u step the game does not take
                if sw.is_point || m.dot(t.normal).abs() > 0.999_99 {
                    continue;
                }
                if m.dot(c - a) > 0.0 {
                    m = -m;
                }
                if !clip(m, m.dot(a)) {
                    return;
                }
            }
        }
        if !start_out {
            return;
        }
        if enter_frac > -1.0 && enter_frac < leave_frac && enter_frac < sw.fraction {
            sw.fraction = enter_frac.max(0.0);
            sw.normal = clip_plane.0;
            sw.plane_dist = clip_plane.1;
            sw.contents = contents::SOLID;
            sw.surface = (material != u32::MAX).then_some(material);
        }
    }

    fn trace_node(&self, sw: &mut Sweep, node: i32, p1f: f32, p2f: f32, p1: Vec3, p2: Vec3) {
        if sw.fraction <= p1f {
            return;
        }
        if node < 0 {
            self.trace_leaf(sw, (-node - 1) as usize);
            return;
        }
        let n = &self.nodes[node as usize];
        let plane = &self.planes[n.plane as usize];
        let t1 = plane.normal.dot(p1) - plane.dist;
        let t2 = plane.normal.dot(p2) - plane.dist;
        let offset = if sw.is_point {
            0.0
        } else {
            plane.normal.abs().dot(sw.extents)
        };
        if t1 >= offset && t2 >= offset {
            self.trace_node(sw, n.children[0], p1f, p2f, p1, p2);
            return;
        }
        if t1 < -offset && t2 < -offset {
            self.trace_node(sw, n.children[1], p1f, p2f, p1, p2);
            return;
        }
        // straddles: split the segment, near side first
        let (side, frac1, frac2) = if t1 < t2 {
            let idist = 1.0 / (t1 - t2);
            (
                1usize,
                (t1 - offset + DIST_EPSILON) * idist,
                (t1 + offset + DIST_EPSILON) * idist,
            )
        } else if t1 > t2 {
            let idist = 1.0 / (t1 - t2);
            (
                0usize,
                (t1 + offset + DIST_EPSILON) * idist,
                (t1 - offset - DIST_EPSILON) * idist,
            )
        } else {
            (0usize, 1.0, 0.0)
        };
        let frac1 = frac1.clamp(0.0, 1.0);
        let frac2 = frac2.clamp(0.0, 1.0);

        let midf = p1f + (p2f - p1f) * frac1;
        let mid = p1 + (p2 - p1) * frac1;
        self.trace_node(sw, n.children[side], p1f, midf, p1, mid);

        let midf = p1f + (p2f - p1f) * frac2;
        let mid = p1 + (p2 - p1) * frac2;
        self.trace_node(sw, n.children[side ^ 1], midf, p2f, mid, p2);
    }

    fn trace_leaf(&self, sw: &mut Sweep, leaf: usize) {
        let l = self.leafs[leaf];
        for i in 0..l.num_brushes {
            let bi = self.leaf_brushes[(l.first_brush + i) as usize] as u32;
            if sw.checked.contains(&bi) {
                continue;
            }
            sw.checked.push(bi);
            let b = self.brushes[bi as usize];
            if b.contents & sw.mask == 0 || b.num_sides == 0 {
                continue;
            }
            self.trace_brush(sw, &b);
            if sw.all_solid {
                return;
            }
        }
    }

    fn trace_brush(&self, sw: &mut Sweep, b: &Brush) {
        let mut enter_frac = -1.0f32;
        let mut leave_frac = 1.0f32;
        let mut clip_plane: Option<(Vec3, f32)> = None;
        let mut get_out = false;
        let mut starts_out = false;
        for i in 0..b.num_sides {
            let plane = &self.planes[self.side_planes[(b.first_side + i) as usize] as usize];
            let dist = if sw.is_point {
                plane.dist
            } else {
                // push the plane out by the box extents along its normal
                plane.dist + plane.normal.abs().dot(sw.extents)
            };
            let d1 = plane.normal.dot(sw.start) - dist;
            let d2 = plane.normal.dot(sw.end) - dist;
            if d2 > 0.0 {
                get_out = true;
            }
            if d1 > 0.0 {
                starts_out = true;
            }
            if d1 > 0.0 && d2 >= d1 {
                return; // completely in front of this face, no intersection
            }
            if d1 <= 0.0 && d2 <= 0.0 {
                continue;
            }
            if d1 > d2 {
                // entering
                let f = (d1 - DIST_EPSILON) / (d1 - d2);
                if f > enter_frac {
                    enter_frac = f;
                    clip_plane = Some((plane.normal, dist));
                }
            } else {
                // leaving
                let f = (d1 + DIST_EPSILON) / (d1 - d2);
                if f < leave_frac {
                    leave_frac = f;
                }
            }
        }
        if !starts_out {
            sw.start_solid = true;
            sw.contents |= b.contents;
            if !get_out {
                sw.all_solid = true;
                sw.fraction = 0.0;
            }
            return;
        }
        if enter_frac < leave_frac && enter_frac > -1.0 && enter_frac < sw.fraction {
            let (normal, dist) = clip_plane.expect("entering plane");
            sw.fraction = enter_frac.max(0.0);
            sw.normal = normal;
            sw.plane_dist = dist;
            sw.contents = b.contents;
            sw.surface = None;
        }
    }
}

fn displacements(bsp: &Bsp<'_>, materials: &mut Vec<String>) -> Vec<Disp> {
    use gm_formats::bsp::extra::{build_disp_mesh, disp_infos, disp_verts};
    let (Ok(infos), Ok(verts)) = (disp_infos(bsp), disp_verts(bsp)) else {
        return Vec::new();
    };
    let mut out = Vec::new();
    for info in &infos {
        let Ok(mesh) = build_disp_mesh(bsp, info, &verts) else {
            continue;
        };
        let Some(face) = bsp.faces.get(info.map_face as usize) else {
            continue;
        };
        let Some(plane) = bsp.planes.get(face.plane_num as usize) else {
            continue;
        };
        let material = usize::try_from(face.tex_info)
            .ok()
            .and_then(|i| bsp.tex_infos.get(i))
            .and_then(|ti| bsp.tex_datas.get(usize::try_from(ti.tex_data).ok()?))
            .and_then(|td| {
                bsp.tex_names
                    .get(usize::try_from(td.name_string_table_id).ok()?)
            })
            .map(|n| n.replace('\\', "/").to_ascii_lowercase());
        let mut intern = |n: String| match materials.iter().position(|m| *m == n) {
            Some(i) => i as u32,
            None => {
                materials.push(n);
                (materials.len() - 1) as u32
            }
        };
        let (material, material2) = match material {
            Some(n) => (intern(n.clone()), intern(format!("{n}#2"))),
            None => (u32::MAX, u32::MAX),
        };
        let mut up = Vec3::from(plane.normal);
        if face.side != 0 {
            up = -up;
        }
        let (mut mins, mut maxs) = (Vec3::splat(f32::MAX), Vec3::splat(f32::MIN));
        let mut tris = Vec::new();
        for t in mesh.indices.as_chunks::<3>().0 {
            let mut v = t.map(|i| Vec3::from(mesh.positions[i as usize]));
            let mut normal = (v[1] - v[0]).cross(v[2] - v[0]);
            if normal.length_squared() < 1e-8 {
                continue;
            }
            if normal.dot(up) < 0.0 {
                v.swap(1, 2);
                normal = -normal;
            }
            let normal = normal.normalize();
            for p in v {
                mins = mins.min(p);
                maxs = maxs.max(p);
            }
            let alpha: f32 = t.iter().map(|i| mesh.alphas[*i as usize]).sum();
            tris.push(DispTri {
                v,
                normal,
                dist: normal.dot(v[0]),
                second: alpha > 382.5,
            });
        }
        if !tris.is_empty() {
            out.push(Disp {
                mins,
                maxs,
                tris,
                material,
                material2,
            });
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    /// One axis-aligned box brush from (-64,-64,-16) to (64,64,0), six sides, in a two-leaf tree.
    fn slab() -> World {
        let planes = vec![
            Plane {
                normal: Vec3::X,
                dist: 64.0,
            },
            Plane {
                normal: Vec3::NEG_X,
                dist: 64.0,
            },
            Plane {
                normal: Vec3::Y,
                dist: 64.0,
            },
            Plane {
                normal: Vec3::NEG_Y,
                dist: 64.0,
            },
            Plane {
                normal: Vec3::Z,
                dist: 0.0,
            },
            Plane {
                normal: Vec3::NEG_Z,
                dist: 16.0,
            },
        ];
        World {
            disps: Vec::new(),
            ladders: Vec::new(),
            planes,
            // single node splitting on z=0: above is leaf 0 (empty), below leaf 1 (the brush)
            nodes: vec![Node {
                plane: 4,
                children: [-1, -2],
            }],
            leafs: vec![
                Leaf {
                    contents: 0,
                    first_brush: 0,
                    num_brushes: 0,
                    cluster: 0,
                },
                Leaf {
                    contents: contents::SOLID,
                    first_brush: 0,
                    num_brushes: 1,
                    cluster: 0,
                },
            ],
            leaf_brushes: vec![0],
            brushes: vec![Brush {
                first_side: 0,
                num_sides: 6,
                contents: contents::SOLID,
            }],
            side_planes: vec![0, 1, 2, 3, 4, 5],
            side_sky: Vec::new(),
            side_material: Vec::new(),
            materials: Vec::new(),
            head_node: 0,
            brush_models: Vec::new(),
            vis: Vec::new(),
            num_clusters: 0,
        }
    }

    #[test]
    fn ray_hits_top_of_slab() {
        let w = slab();
        let t = w.trace_ray(
            Vec3::new(0.0, 0.0, 100.0),
            Vec3::new(0.0, 0.0, -100.0),
            MASK_SOLID,
        );
        assert!(t.hit());
        assert_eq!(t.normal, Vec3::Z);
        assert!((t.end.z - DIST_EPSILON).abs() < 1e-4, "{}", t.end.z);
        assert!((t.fraction - (100.0 - DIST_EPSILON) / 200.0).abs() < 1e-5);
        let miss = w.trace_ray(
            Vec3::new(200.0, 0.0, 100.0),
            Vec3::new(200.0, 0.0, -100.0),
            MASK_SOLID,
        );
        assert!(!miss.hit());
    }

    #[test]
    fn box_stops_above_slab_and_point_contents() {
        let w = slab();
        let mins = Vec3::new(-16.0, -16.0, 0.0);
        let maxs = Vec3::new(16.0, 16.0, 72.0);
        let t = w.trace_box(
            Vec3::new(0.0, 0.0, 50.0),
            Vec3::new(0.0, 0.0, -50.0),
            mins,
            maxs,
            MASK_PLAYERSOLID,
        );
        assert!(t.hit());
        assert!((t.end.z - DIST_EPSILON).abs() < 1e-4, "{}", t.end.z);
        assert_eq!(w.point_contents(Vec3::new(0.0, 0.0, -8.0)), contents::SOLID);
        assert_eq!(w.point_contents(Vec3::new(0.0, 0.0, 8.0)), 0);
        let inside = w.trace_box(
            Vec3::new(0.0, 0.0, -8.0),
            Vec3::new(0.0, 0.0, -9.0),
            mins,
            maxs,
            MASK_SOLID,
        );
        assert!(inside.start_solid && inside.all_solid);
        // sliding along the top does not hit
        let slide = w.trace_box(
            Vec3::new(0.0, 0.0, 0.1),
            Vec3::new(30.0, 0.0, 0.1),
            mins,
            maxs,
            MASK_SOLID,
        );
        assert!(!slide.hit(), "{slide:?}");
    }
    #[test]
    fn sweep_skin_stops_outside_the_hull_unless_already_within_it() {
        let slab = Convex {
            entity: 7,
            friction: 1.0,
            planes: vec![
                (Vec3::X, 16.0),
                (Vec3::NEG_X, 16.0),
                (Vec3::Y, 16.0),
                (Vec3::NEG_Y, 16.0),
                (Vec3::Z, 0.0),
                (Vec3::NEG_Z, 4.0),
            ],
            mins: Vec3::new(-16.0, -16.0, -4.0),
            maxs: Vec3::new(16.0, 16.0, 0.0),
        };
        let (mins, maxs) = (Vec3::new(-1.0, -1.0, 0.0), Vec3::new(1.0, 1.0, 2.0));
        let (t, ent) = sweep_convexes_skin(
            Vec3::Z * 10.0,
            Vec3::Z * -10.0,
            mins,
            maxs,
            std::slice::from_ref(&slab),
            0.5,
        );
        assert_eq!(ent, Some(7));
        assert!(
            (t.end.z - 0.5).abs() < 2.0 * DIST_EPSILON,
            "skin stop {}",
            t.end.z
        );
        // a box already within the skin (standing on the bare hull) sweeps the bare hull
        let (t, _) = sweep_convexes_skin(Vec3::Z * 0.1, Vec3::Z * 10.0, mins, maxs, &[slab], 0.5);
        assert!(!t.start_solid && t.fraction == 1.0, "left freely {:?}", t);
    }
}
