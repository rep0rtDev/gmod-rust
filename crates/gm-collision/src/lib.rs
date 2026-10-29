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
}

#[derive(Clone, Copy, Debug)]
struct Brush {
    first_side: u32,
    num_sides: u32,
    contents: i32,
}

pub struct World {
    planes: Vec<Plane>,
    nodes: Vec<Node>,
    leafs: Vec<Leaf>,
    leaf_brushes: Vec<u16>,
    brushes: Vec<Brush>,
    side_planes: Vec<u32>,
    head_node: i32,
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
    checked: Vec<u32>,
}

impl World {
    pub fn from_bsp(bsp: &Bsp<'_>) -> Self {
        World {
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
            head_node: bsp.models.first().map(|m| m.head_node).unwrap_or(0),
        }
    }

    /// A world made of axis-aligned solid boxes with no tree (one leaf), for tests and tools.
    pub fn from_boxes(boxes: &[(Vec3, Vec3)]) -> Self {
        let mut planes = Vec::new();
        let mut side_planes = Vec::new();
        let mut brushes = Vec::new();
        for (mins, maxs) in boxes {
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
                contents: contents::SOLID,
            });
        }
        let leaf_brushes: Vec<u16> = (0..brushes.len() as u16).collect();
        World {
            planes,
            nodes: Vec::new(),
            leafs: vec![Leaf {
                contents: 0,
                first_brush: 0,
                num_brushes: brushes.len() as u32,
            }],
            leaf_brushes,
            brushes,
            side_planes,
            head_node: -1,
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
        c
    }

    fn brush_contains(&self, b: &Brush, point: Vec3) -> bool {
        (0..b.num_sides).all(|i| {
            let p = &self.planes[self.side_planes[(b.first_side + i) as usize] as usize];
            p.normal.dot(point) - p.dist <= 0.0
        })
    }

    /// Ray trace.
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
        }
    }
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
                },
                Leaf {
                    contents: contents::SOLID,
                    first_brush: 0,
                    num_brushes: 1,
                },
            ],
            leaf_brushes: vec![0],
            brushes: vec![Brush {
                first_side: 0,
                num_sides: 6,
                contents: contents::SOLID,
            }],
            side_planes: vec![0, 1, 2, 3, 4, 5],
            head_node: 0,
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
}
