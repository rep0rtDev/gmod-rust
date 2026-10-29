//! Convex hulls of BSP brushes: a brush is the intersection of its side half spaces, its
//! vertices are the plane triple intersections that lie inside every other side.

use glam::Vec3;
use gm_formats::bsp::lumps::contents;
use gm_formats::bsp::Bsp;

const EPSILON: f32 = 0.05;
const WELD: f32 = 0.01;

#[derive(Debug, Clone)]
pub struct BrushHull {
    pub contents: i32,
    pub points: Vec<Vec3>,
}

/// Which brushes a rigid body collides with: solid world and (opaque) grates, no clips,
/// water, triggers or skip surfaces.
pub fn is_physics_solid(contents: i32) -> bool {
    contents & (contents::SOLID | contents::WINDOW | contents::GRATE) != 0
}

/// Hulls of every brush of a model (0 = the world), every side pushed out by `inflate` units
/// (the game keeps resting bodies about 1 cm = 0.39 u off the surface, see
/// docs/spec/physics.md). Brushes with fewer than 4 distinct vertices (degenerate) are skipped.
pub fn model_hulls(bsp: &Bsp<'_>, model: usize, inflate: f32) -> Vec<BrushHull> {
    let Some(m) = bsp.models.get(model) else {
        return Vec::new();
    };
    let leafs = model_leafs(bsp, m.head_node);
    let mut seen = vec![false; bsp.brushes.len()];
    let mut out = Vec::new();
    for leaf in leafs {
        let l = &bsp.leafs[leaf];
        for i in 0..l.num_leaf_brushes as usize {
            let b = bsp.leaf_brushes[l.first_leaf_brush as usize + i] as usize;
            if seen[b] {
                continue;
            }
            seen[b] = true;
            let brush = &bsp.brushes[b];
            if !is_physics_solid(brush.contents) {
                continue;
            }
            let planes: Vec<(Vec3, f32)> = (0..brush.num_sides.max(0) as usize)
                .map(|s| {
                    let side = &bsp.brush_sides[brush.first_side as usize + s];
                    let p = &bsp.planes[side.plane_num as usize];
                    (Vec3::from(p.normal), p.dist + inflate)
                })
                .collect();
            let points = hull_points(&planes);
            if points.len() >= 4 {
                out.push(BrushHull {
                    contents: brush.contents,
                    points,
                });
            }
        }
    }
    out
}

fn model_leafs(bsp: &Bsp<'_>, head: i32) -> Vec<usize> {
    let mut stack = vec![head];
    let mut out = Vec::new();
    while let Some(n) = stack.pop() {
        if n < 0 {
            out.push((-1 - n) as usize);
        } else {
            let node = &bsp.nodes[n as usize];
            stack.push(node.children[0]);
            stack.push(node.children[1]);
        }
    }
    out
}

/// Vertices of the convex polytope `n . p <= d` for every plane.
pub fn hull_points(planes: &[(Vec3, f32)]) -> Vec<Vec3> {
    let mut points: Vec<Vec3> = Vec::new();
    let n = planes.len();
    for i in 0..n {
        for j in i + 1..n {
            for k in j + 1..n {
                let (n1, d1) = planes[i];
                let (n2, d2) = planes[j];
                let (n3, d3) = planes[k];
                let denom = n1.dot(n2.cross(n3));
                if denom.abs() < 1e-6 {
                    continue;
                }
                let p = (d1 * n2.cross(n3) + d2 * n3.cross(n1) + d3 * n1.cross(n2)) / denom;
                if !p.is_finite() {
                    continue;
                }
                if planes.iter().any(|(pn, pd)| pn.dot(p) - pd > EPSILON) {
                    continue;
                }
                if !points.iter().any(|q| q.distance_squared(p) < WELD * WELD) {
                    points.push(p);
                }
            }
        }
    }
    points
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn axis_box_has_eight_corners() {
        let planes = [
            (Vec3::X, 16.0),
            (Vec3::NEG_X, 16.0),
            (Vec3::Y, 8.0),
            (Vec3::NEG_Y, 8.0),
            (Vec3::Z, 4.0),
            (Vec3::NEG_Z, 4.0),
        ];
        let pts = hull_points(&planes);
        assert_eq!(pts.len(), 8);
        for p in pts {
            assert!(
                (p.x.abs() - 16.0).abs() < 1e-4
                    && (p.y.abs() - 8.0).abs() < 1e-4
                    && (p.z.abs() - 4.0).abs() < 1e-4
            );
        }
    }

    #[test]
    fn wedge_with_redundant_plane() {
        let s = std::f32::consts::FRAC_1_SQRT_2;
        let planes = [
            (Vec3::X, 10.0),
            (Vec3::NEG_X, 0.0),
            (Vec3::Y, 10.0),
            (Vec3::NEG_Y, 0.0),
            (Vec3::NEG_Z, 0.0),
            (Vec3::new(s, 0.0, s), 10.0 * s),
            (Vec3::Z, 100.0),
        ];
        assert_eq!(hull_points(&planes).len(), 6);
    }
}

/// Displacement surfaces of the map as one triangle soup, wound to face along the plane of
/// the brush face they sit on and pushed out by `inflate` along it (the same rest gap as the
/// brush hulls). The game's brush trace stops on the displacement, not on the brush under it
/// (gm_construct's ground at 600, -500 is 4 u above the brush below it), so props need this.
pub fn displacement_mesh(bsp: &Bsp<'_>, inflate: f32) -> (Vec<Vec3>, Vec<u32>) {
    use gm_formats::bsp::extra::{build_disp_mesh, disp_infos, disp_verts};
    let (mut positions, mut indices) = (Vec::new(), Vec::new());
    let (Ok(infos), Ok(verts)) = (disp_infos(bsp), disp_verts(bsp)) else {
        return (positions, indices);
    };
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
        let mut normal = Vec3::from(plane.normal);
        if face.side != 0 {
            normal = -normal;
        }
        let base = positions.len() as u32;
        for &p in &mesh.positions {
            positions.push(Vec3::from(p) + normal * inflate);
        }
        for t in mesh.indices.chunks_exact(3) {
            let (a, b, c) = (
                Vec3::from(mesh.positions[t[0] as usize]),
                Vec3::from(mesh.positions[t[1] as usize]),
                Vec3::from(mesh.positions[t[2] as usize]),
            );
            let flip = (b - a).cross(c - a).dot(normal) < 0.0;
            let (i1, i2) = if flip { (t[2], t[1]) } else { (t[1], t[2]) };
            indices.extend_from_slice(&[base + t[0], base + i1, base + i2]);
        }
    }
    (positions, indices)
}
