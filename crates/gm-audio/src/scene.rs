//! The map as acoustic geometry: the faces of the world model and of brush entities (sky,
//! triggers and hint/skip faces left out), displacements as their built meshes, each
//! triangle tagged with the acoustic material of its texture's `$surfaceprop`.
//! Everything in game units and axes; [`crate::acoustics`] converts for Steam Audio.
//!
//! Not the engine's model: the game has no acoustic geometry at all (sounds pass through
//! walls; DSP presets per room stand in for it). The material table is Steam Audio's
//! documented presets, assigned to the surface properties by what they are made of.

use audionimbus::geometry::Material;
use glam::Vec3;
use gm_formats::bsp::lumps::surf;
use gm_formats::bsp::Bsp;

#[derive(Debug, Clone, Default)]
pub struct Geometry {
    pub vertices: Vec<Vec3>,
    pub triangles: Vec<[u32; 3]>,
    /// Per triangle: index into `materials`.
    pub material_of: Vec<u32>,
    pub materials: Vec<Material>,
    /// Surface property name per material, same order.
    pub names: Vec<String>,
}

/// Steam Audio's preset materials (absorption low / mid / high, scattering, transmission
/// low / mid / high), with a few of our own for what the presets do not cover.
fn preset(name: &str) -> Material {
    let m = |a: [f32; 3], s: f32, t: [f32; 3]| Material {
        absorption: a,
        scattering: s,
        transmission: t,
    };
    match name {
        "brick" => m([0.03, 0.04, 0.07], 0.05, [0.015, 0.015, 0.015]),
        "concrete" => m([0.05, 0.07, 0.08], 0.05, [0.015, 0.002, 0.001]),
        "ceramic" => m([0.01, 0.02, 0.02], 0.05, [0.060, 0.044, 0.011]),
        "gravel" => m([0.60, 0.70, 0.80], 0.05, [0.031, 0.012, 0.008]),
        "carpet" => m([0.24, 0.69, 0.73], 0.05, [0.020, 0.005, 0.003]),
        "glass" => m([0.06, 0.03, 0.02], 0.05, [0.060, 0.044, 0.011]),
        "plaster" => m([0.12, 0.06, 0.04], 0.05, [0.056, 0.056, 0.004]),
        "wood" => m([0.11, 0.07, 0.06], 0.05, [0.070, 0.014, 0.005]),
        "metal" => m([0.20, 0.07, 0.06], 0.05, [0.200, 0.025, 0.010]),
        "rock" => m([0.13, 0.20, 0.24], 0.05, [0.015, 0.002, 0.001]),
        // ours
        "wood_hollow" => m([0.15, 0.10, 0.08], 0.05, [0.150, 0.050, 0.020]),
        "metal_thin" => m([0.20, 0.07, 0.06], 0.05, [0.300, 0.100, 0.040]),
        "metalgrate" | "chainlink" => m([0.10, 0.10, 0.10], 0.30, [0.800, 0.800, 0.800]),
        "dirt" | "sand" | "snow" => m([0.40, 0.55, 0.70], 0.10, [0.015, 0.002, 0.001]),
        "grass" | "foliage" => m([0.30, 0.50, 0.75], 0.20, [0.300, 0.150, 0.080]),
        "water" => m([0.01, 0.01, 0.02], 0.05, [0.200, 0.100, 0.050]),
        "tile" => m([0.02, 0.02, 0.02], 0.05, [0.015, 0.002, 0.001]),
        "plastic" | "rubber" => m([0.10, 0.10, 0.10], 0.05, [0.100, 0.050, 0.020]),
        "paper" | "cardboard" => m([0.20, 0.30, 0.40], 0.05, [0.400, 0.300, 0.200]),
        "flesh" => m([0.30, 0.40, 0.50], 0.05, [0.050, 0.020, 0.010]),
        _ => m([0.10, 0.20, 0.30], 0.05, [0.100, 0.050, 0.030]),
    }
}

/// The acoustic material of a surface property (lower-case name from `surfaceproperties`).
pub fn material_for(surfaceprop: &str) -> (&'static str, Material) {
    let key = match surfaceprop {
        "brick" => "brick",
        "concrete" | "concrete_block" | "asphalt" => "concrete",
        "gravel" => "gravel",
        "boulder" | "rock" => "rock",
        "mud" | "dirt" => "dirt",
        "quicksand" | "sand" => "sand",
        "snow" => "snow",
        "water" | "slime" | "slipperyslime" | "wade" => "water",
        "wood" | "wood_plank" | "wood_panel" | "wood_solid" | "wood_furniture" => "wood",
        "wood_lowdensity" | "wood_box" | "wood_crate" | "wood_hollow" => "wood_hollow",
        "metal_box" | "metalvent" | "metalpanel" | "metal_barrel" | "metal_thin" => "metal_thin",
        "metalgrate" => "metalgrate",
        "chainlink" | "chain" => "chainlink",
        "metal" | "solidmetal" | "metal_bouncy" | "slime_metal" | "grenade" | "canister"
        | "metal_sand_barrel" | "weapon" => "metal",
        "computer" | "plastic" | "plastic_barrel" | "plastic_box" => "plastic",
        "paper" | "papercup" => "paper",
        "cardboard" => "cardboard",
        "glass" | "combine_glass" | "glassbottle" => "glass",
        "tile" | "ice" => "tile",
        "ceramic" | "porcelain" | "pottery" => "ceramic",
        "carpet" => "carpet",
        "rubbertire" | "rubber" => "rubber",
        "flesh" | "bloodyflesh" | "alienflesh" | "zombieflesh" | "antlion" | "player"
        | "watermelon" => "flesh",
        "foliage" => "foliage",
        "grass" => "grass",
        "plaster" => "plaster",
        _ => "default",
    };
    (key, preset(key))
}

fn skip(flags: i32) -> bool {
    flags & (surf::SKY | surf::SKY2D | surf::TRIGGER | surf::HINT | surf::SKIP) != 0
}

impl Geometry {
    fn intern(&mut self, surfaceprop: &str) -> u32 {
        let (key, m) = material_for(surfaceprop);
        if let Some(i) = self.names.iter().position(|n| n == key) {
            return i as u32;
        }
        self.names.push(key.to_string());
        self.materials.push(m);
        (self.materials.len() - 1) as u32
    }

    /// Faces and displacements of `bsp`; `surfaceprop(texture)` names the surface property
    /// of a face's material (the VMT's `$surfaceprop`, "default" without one).
    pub fn from_bsp(bsp: &Bsp<'_>, mut surfaceprop: impl FnMut(&str) -> String) -> Geometry {
        use std::collections::HashMap;
        let mut g = Geometry::default();
        let mut by_texture: HashMap<String, u32> = HashMap::new();
        let mut material = |g: &mut Geometry, texture: Option<&str>| -> u32 {
            let Some(t) = texture else {
                return g.intern("default");
            };
            let key = t.to_ascii_lowercase();
            if let Some(&m) = by_texture.get(&key) {
                return m;
            }
            let m = g.intern(&surfaceprop(&key).to_ascii_lowercase());
            by_texture.insert(key, m);
            m
        };
        // brush entities keep their faces relative to the entity origin
        let mut offsets: HashMap<usize, Vec3> = HashMap::new();
        for e in &bsp.entities {
            let (Some(model), Some(origin)) = (e.get("model"), e.origin()) else {
                continue;
            };
            let Some(m) = model
                .strip_prefix('*')
                .and_then(|n| n.parse::<usize>().ok())
                .and_then(|i| bsp.models.get(i))
            else {
                continue;
            };
            for fi in m.first_face..m.first_face + m.num_faces {
                offsets.insert(fi as usize, Vec3::from(origin));
            }
        }
        for (fi, face) in bsp.faces.iter().enumerate() {
            if face.disp_info >= 0 || face.tex_info < 0 {
                continue;
            }
            let Some(ti) = bsp.tex_infos.get(face.tex_info as usize) else {
                continue;
            };
            if skip(ti.flags) {
                continue;
            }
            let verts = bsp.face_vertices(face);
            if verts.len() < 3 {
                continue;
            }
            let offset = offsets.get(&fi).copied().unwrap_or(Vec3::ZERO);
            let m = material(&mut g, bsp.face_material(face));
            let base = g.vertices.len() as u32;
            g.vertices
                .extend(verts.iter().map(|v| Vec3::from(*v) + offset));
            for i in 1..verts.len() as u32 - 1 {
                g.triangles.push([base, base + i, base + i + 1]);
                g.material_of.push(m);
            }
        }
        if let (Ok(infos), Ok(dverts)) = (
            gm_formats::bsp::extra::disp_infos(bsp),
            gm_formats::bsp::extra::disp_verts(bsp),
        ) {
            for info in &infos {
                let Ok(mesh) = gm_formats::bsp::extra::build_disp_mesh(bsp, info, &dverts) else {
                    continue;
                };
                let m = material(
                    &mut g,
                    bsp.faces
                        .get(info.map_face as usize)
                        .and_then(|f| bsp.face_material(f)),
                );
                let base = g.vertices.len() as u32;
                g.vertices
                    .extend(mesh.positions.iter().map(|p| Vec3::from(*p)));
                for t in mesh.indices.as_chunks::<3>().0 {
                    g.triangles.push([base + t[0], base + t[1], base + t[2]]);
                    g.material_of.push(m);
                }
            }
        }
        g
    }

    /// A prop's collision hull (`.phy`, first solid: its convex pieces as triangle soup in
    /// the model's frame, game units) with the solid's surface property.
    pub fn from_phy(phy: &gm_formats::phy::Phy, surfaceprop: &str) -> Geometry {
        let mut g = Geometry::default();
        let Some(solid) = phy.solids.first() else {
            return g;
        };
        let m = g.intern(surfaceprop);
        for c in &solid.convexes {
            let base = g.vertices.len() as u32;
            g.vertices.extend(c.points.iter().map(|p| Vec3::from(*p)));
            for t in &c.triangles {
                g.triangles
                    .push([base + t[0] as u32, base + t[1] as u32, base + t[2] as u32]);
                g.material_of.push(m);
            }
        }
        g
    }

    /// An axis-aligned box of one surface property (tests, stand-ins), faces outward.
    pub fn push_box(&mut self, mins: Vec3, maxs: Vec3, surfaceprop: &str) {
        let m = self.intern(surfaceprop);
        let base = self.vertices.len() as u32;
        for i in 0..8u32 {
            self.vertices.push(Vec3::new(
                if i & 1 != 0 { maxs.x } else { mins.x },
                if i & 2 != 0 { maxs.y } else { mins.y },
                if i & 4 != 0 { maxs.z } else { mins.z },
            ));
        }
        const FACES: [[u32; 4]; 6] = [
            [0, 2, 3, 1],
            [4, 5, 7, 6],
            [0, 1, 5, 4],
            [2, 6, 7, 3],
            [0, 4, 6, 2],
            [1, 3, 7, 5],
        ];
        for f in FACES {
            self.triangles.push([base + f[0], base + f[1], base + f[2]]);
            self.triangles.push([base + f[0], base + f[2], base + f[3]]);
            self.material_of.push(m);
            self.material_of.push(m);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn surface_properties_map_to_presets() {
        assert_eq!(material_for("wood_crate").0, "wood_hollow");
        assert_eq!(material_for("metal_barrel").0, "metal_thin");
        assert_eq!(material_for("concrete").0, "concrete");
        assert_eq!(material_for("gravel").0, "gravel");
        assert_eq!(material_for("boulder").0, "rock");
        assert_eq!(material_for("snow").0, "snow");
        assert_eq!(material_for("whatever").0, "default");
        assert!(
            material_for("chainlink").1.transmission[0] > 0.5,
            "grates let sound through"
        );
    }

    #[test]
    fn a_box_is_twelve_triangles_of_one_material() {
        let mut g = Geometry::default();
        g.push_box(Vec3::ZERO, Vec3::ONE, "concrete");
        g.push_box(Vec3::ONE, Vec3::ONE * 2.0, "concrete");
        assert_eq!((g.vertices.len(), g.triangles.len()), (16, 24));
        assert_eq!(g.materials.len(), 1);
        assert!(g.material_of.iter().all(|&m| m == 0));
    }

    /// gm_construct from a mounted install: faces and displacements make a scene.
    #[test]
    fn gm_construct_builds_a_scene() {
        let Some(inst) = gm_content::find_install() else {
            eprintln!("no GMod install, skipped");
            return;
        };
        let mount = gm_content::Mount::for_install(&inst).unwrap();
        let Ok(data) = mount.read("maps/gm_construct.bsp") else {
            eprintln!("no gm_construct, skipped");
            return;
        };
        let bsp = Bsp::parse(&data).unwrap();
        let t = std::time::Instant::now();
        let g = Geometry::from_bsp(&bsp, |tex| {
            if tex.contains("concrete") {
                "concrete".into()
            } else {
                "default".into()
            }
        });
        let build = t.elapsed();
        assert!(g.triangles.len() > 5000, "{} triangles", g.triangles.len());
        assert_eq!(g.triangles.len(), g.material_of.len());
        assert!(g.names.contains(&"concrete".to_string()), "{:?}", g.names);
        assert!(g
            .material_of
            .iter()
            .all(|&m| (m as usize) < g.materials.len()));
        assert!(g
            .triangles
            .iter()
            .flatten()
            .all(|&i| (i as usize) < g.vertices.len()));
        let context =
            audionimbus::Context::try_new(&audionimbus::ContextSettings::default()).unwrap();
        let t = std::time::Instant::now();
        let mut a = crate::acoustics::Acoustics::new(&context, 48000, &g).unwrap();
        let scene = t.elapsed();
        // a listener at the spawn with one voice: a direct pass and a reverb pass
        let snap = crate::acoustics::Snapshot {
            listener: Some(crate::mixer::Listener::from_view(
                Vec3::new(0.0, 0.0, 64.0),
                0.0,
                0.0,
            )),
            voices: vec![(1, Vec3::new(200.0, 0.0, 64.0))],
        };
        let t = std::time::Instant::now();
        let (paths, reverb) = a.step(&snap).unwrap();
        let first = t.elapsed();
        let t = std::time::Instant::now();
        let (_, none) = a.step(&snap).unwrap();
        let direct_only = t.elapsed();
        assert!(
            reverb.is_some() && none.is_none(),
            "reverb every {} passes",
            crate::acoustics::REVERB_EVERY
        );
        assert_eq!(paths.len(), 1);
        eprintln!(
            "gm_construct: {} triangles, {} materials, build {:?}, scene {:?}, direct+reverb pass {:?}, direct pass {:?}, path {:?}",
            a.triangles,
            g.materials.len(),
            build,
            scene,
            first,
            direct_only,
            paths[&1]
        );
    }
}
