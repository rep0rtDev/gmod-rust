//! Model lighting the way the game lights studio models from the BSP: the leaf's ambient cube
//! (six colours, looked up by the squared normal components) plus the direct world lights that
//! reach the point (point / spot lights with their attenuation, the sun when the point sees the
//! sky), shaded half-Lambert. Static props without baked vertex colours and every dynamic model
//! go through this; props with `sp_<n>.vhv` use vrad's colours.

use glam::Vec3;
use gm_formats::bsp::light::{EmitType, LeafAmbient, WorldLight};
use gm_formats::bsp::Bsp;

/// The ambient lump stores its cubes in linear light (`c * 2^exp`), unlike the lightmaps
/// (`c * 2^exp / 255`): measured on gm_construct, exponents peak at -9 against 0.
fn ambient_linear(c: gm_formats::bsp::lumps::ColorRgbExp32) -> Vec3 {
    Vec3::from(c.to_linear()) * 255.0
}

/// A static prop's baked vertex colour (RGBA8 from `sp_<n>.vhv`) as the linear multiplier the
/// model shader applies; vrad writes them gamma encoded, like the lightmaps' display scale.
pub fn baked_vertex_color(c: [u8; 4]) -> Vec3 {
    Vec3::new(c[0] as f32, c[1] as f32, c[2] as f32) / 255.0
}

pub struct Light {
    pub origin: Vec3,
    pub intensity: Vec3,
    pub normal: Vec3,
    pub emit: EmitType,
    pub stopdot: f32,
    pub stopdot2: f32,
    pub exponent: f32,
    pub radius: f32,
    pub attn: [f32; 3],
}

impl Light {
    fn from_world(l: &WorldLight) -> Self {
        Light {
            origin: Vec3::from(l.origin),
            intensity: Vec3::from(l.intensity),
            normal: Vec3::from(l.normal),
            emit: l.emit,
            stopdot: l.stopdot,
            stopdot2: l.stopdot2,
            exponent: l.exponent,
            radius: l.radius,
            attn: [l.constant_attn, l.linear_attn, l.quadratic_attn],
        }
    }

    /// Irradiance at `pos` ignoring the surface normal: the light's colour over its falloff,
    /// zero outside a spotlight's cone or past its radius. Returns (colour, direction to light).
    fn reach(&self, pos: Vec3) -> Option<(Vec3, Vec3)> {
        match self.emit {
            EmitType::Skylight => Some((self.intensity, -self.normal)),
            EmitType::Point | EmitType::Spotlight => {
                let to = self.origin - pos;
                let d = to.length();
                if d < 1e-3 || (self.radius > 0.0 && d > self.radius) {
                    return None;
                }
                let dir = to / d;
                let [c, l, q] = self.attn;
                let denom = c + l * d + q * d * d;
                if denom <= 0.0 {
                    return None;
                }
                let mut color = self.intensity / denom;
                if self.emit == EmitType::Spotlight {
                    // cone: full inside stopdot, out at stopdot2, `exponent` shaped ramp between
                    let dot = (-dir).dot(self.normal);
                    if dot <= self.stopdot2 {
                        return None;
                    }
                    if dot < self.stopdot {
                        let mut ramp = (dot - self.stopdot2) / (self.stopdot - self.stopdot2);
                        if self.exponent > 0.0 && self.exponent != 1.0 {
                            ramp = ramp.powf(self.exponent);
                        }
                        color *= ramp;
                    }
                }
                Some((color, dir))
            }
            _ => None,
        }
    }
}

/// Everything needed to light a model anywhere on the map (owns what it needs from the BSP).
pub struct ModelLight {
    ambient: LeafAmbient,
    leaf_boxes: Vec<([f32; 3], [f32; 3])>,
    nodes: Vec<gm_formats::bsp::lumps::Node>,
    planes: Vec<gm_formats::bsp::lumps::Plane>,
    head_node: i32,
    lights: Vec<Light>,
    collision: gm_collision::World,
}

/// Lighting prepared for one position: the ambient cube and the lights that reach it.
#[derive(Clone, Debug, Default)]
pub struct LocalLight {
    pub cube: [Vec3; 6],
    /// (colour, direction towards the light)
    pub lights: Vec<(Vec3, Vec3)>,
}

impl LocalLight {
    /// Lit colour multiplier for a surface with world normal `n`: ambient cube plus the direct
    /// lights, Lambert or (`$halflambert` materials) half-Lambert wrapped.
    pub fn shade(&self, n: Vec3, half_lambert: bool) -> Vec3 {
        let sq = n * n;
        let c = &self.cube;
        let mut out = sq.x * if n.x >= 0.0 { c[0] } else { c[1] }
            + sq.y * if n.y >= 0.0 { c[2] } else { c[3] }
            + sq.z * if n.z >= 0.0 { c[4] } else { c[5] };
        for (color, dir) in &self.lights {
            let d = n.dot(*dir);
            let f = if half_lambert {
                let hl = d * 0.5 + 0.5;
                hl * hl
            } else {
                d.max(0.0)
            };
            out += *color * f;
        }
        out
    }
}

impl ModelLight {
    pub fn new(bsp: &Bsp<'_>) -> Self {
        let ambient = LeafAmbient::parse(bsp, false).unwrap_or_default();
        let lights = gm_formats::bsp::light::world_lights(bsp, false)
            .unwrap_or_default()
            .iter()
            .map(Light::from_world)
            .collect();
        let leaf_boxes = bsp
            .leafs
            .iter()
            .map(|l| (l.mins.map(f32::from), l.maxs.map(f32::from)))
            .collect();
        ModelLight {
            ambient,
            leaf_boxes,
            nodes: bsp.nodes.clone(),
            planes: bsp.planes.clone(),
            head_node: bsp.models.first().map(|m| m.head_node).unwrap_or(0),
            lights,
            collision: gm_collision::World::from_bsp(bsp),
        }
    }

    fn leaf_at(&self, point: Vec3) -> usize {
        let mut node = self.head_node;
        while node >= 0 {
            let Some(n) = self.nodes.get(node as usize) else {
                return 0;
            };
            let Some(p) = self.planes.get(n.plane_num as usize) else {
                return 0;
            };
            let d = Vec3::from(p.normal).dot(point) - p.dist;
            node = if d >= 0.0 {
                n.children[0]
            } else {
                n.children[1]
            };
        }
        (-node - 1) as usize
    }

    /// Ambient cube at `pos`: the nearest sample of its leaf (the game blends the leaf's
    /// samples by distance; the nearest is within its box resolution).
    fn ambient_at(&self, pos: Vec3) -> [Vec3; 6] {
        let leaf = self.leaf_at(pos);
        let samples = self
            .ambient
            .leaves
            .get(leaf)
            .map(Vec::as_slice)
            .unwrap_or(&[]);
        let (mins, maxs) = self
            .leaf_boxes
            .get(leaf)
            .copied()
            .unwrap_or(([0.0; 3], [0.0; 3]));
        let best = samples.iter().min_by(|a, b| {
            let da = Vec3::from(a.position(mins, maxs)).distance_squared(pos);
            let db = Vec3::from(b.position(mins, maxs)).distance_squared(pos);
            da.total_cmp(&db)
        });
        match best {
            Some(s) => s.cube.map(ambient_linear),
            None => [Vec3::splat(0.3); 6],
        }
    }

    /// Whether a straight line from `pos` leaves the map through the sky (no solid in the way).
    fn sees_sky(&self, pos: Vec3, dir: Vec3) -> bool {
        let tr = self
            .collision
            .trace_ray(pos, pos + dir * 16384.0, gm_collision::MASK_SOLID);
        if !tr.hit() {
            return true;
        }
        self.collision.is_sky(tr.end, tr.normal)
    }

    /// Lighting at `pos` for a model; `max_lights` direct lights kept (the game uses up to four).
    pub fn at(&self, pos: Vec3, max_lights: usize) -> LocalLight {
        let cube = self.ambient_at(pos);
        let mut lights: Vec<(Vec3, Vec3)> = Vec::new();
        for l in &self.lights {
            let Some((color, dir)) = l.reach(pos) else {
                continue;
            };
            if color.max_element() < 0.002 {
                continue;
            }
            // shadowed by the world (the sun through the sky only)
            let visible = match l.emit {
                EmitType::Skylight => self.sees_sky(pos, dir),
                _ => {
                    let tr = self
                        .collision
                        .trace_ray(pos, l.origin, gm_collision::MASK_SOLID);
                    !tr.hit()
                }
            };
            if visible {
                lights.push((color, dir));
            }
        }
        lights.sort_by(|a, b| b.0.max_element().total_cmp(&a.0.max_element()));
        lights.truncate(max_lights);
        LocalLight { cube, lights }
    }
}
