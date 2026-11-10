//! Static props baked into world space, lit by vrad's per-vertex colours from the pakfile
//! (`sp_<n>.vhv`) or, when the map has none for a prop, the model lighting of its origin.

use std::collections::HashMap;

use anyhow::Result;
use glam::{Mat3, Mat4, Quat, Vec3};
use gm_content::Mount;
use gm_formats::bsp::extra::StaticProps;
use gm_formats::bsp::Bsp;
use gm_formats::mdl::{Mdl, Vtx, Vvd};

use crate::material::{self, Material, Rgba};
use crate::world::Vertex;

pub struct LoadedModel {
    pub mdl: Mdl,
    pub vvd: Vvd,
    pub vtx: Vtx,
    /// VTX LOD the VVD was fixed up for (`r_rootlod` clamped to the model).
    pub lod: usize,
    /// Per texture index: material key in the shared material map, or None if missing.
    pub texture_keys: Vec<Option<String>>,
}

/// Direction towards the sun from `light_environment`, or a default from above.
pub fn sun_direction(bsp: &Bsp<'_>) -> Vec3 {
    let Some(e) = bsp
        .entities
        .iter()
        .find(|e| e.classname() == "light_environment")
    else {
        return Vec3::new(0.3, 0.2, 1.0).normalize();
    };
    let angles = e.angles().unwrap_or([0.0, 0.0, 0.0]);
    let pitch = e
        .get("pitch")
        .and_then(|p| p.parse::<f32>().ok())
        .unwrap_or(angles[0]);
    let (p, y) = (pitch.to_radians(), angles[1].to_radians());
    // the light travels along forward(pitch, yaw); the sun is the other way
    -Vec3::new(p.cos() * y.cos(), p.cos() * y.sin(), -p.sin())
}

/// `env_sun`: the glow overlay of the sun - the direction towards it, `size`, `overlaysize`
/// (-1: no overlay), `rendercolor` / `overlaycolor` (0..1) and the sprite `material`.
#[derive(Clone, Debug, PartialEq)]
pub struct EnvSun {
    pub dir: Vec3,
    pub size: f32,
    pub overlay_size: f32,
    pub color: Vec3,
    pub overlay_color: Vec3,
    pub material: String,
}

/// The map's `env_sun`: `sun_dir` when set (what `edit_sun` writes), else the opposite of
/// its angles' forward (`CSun::Activate` with `use_angles`), like `sun_direction`.
pub fn env_sun(bsp: &Bsp<'_>) -> Option<EnvSun> {
    let e = bsp.entities.iter().find(|e| e.classname() == "env_sun")?;
    let num = |k: &str, d: f32| {
        e.get(k)
            .and_then(|v| v.trim().parse::<f32>().ok())
            .unwrap_or(d)
    };
    let vec = |k: &str| {
        let p: Vec<f32> = e
            .get(k)?
            .split_whitespace()
            .filter_map(|x| x.parse().ok())
            .collect();
        (p.len() >= 3).then(|| Vec3::new(p[0], p[1], p[2]))
    };
    let dir = vec("sun_dir")
        .map(|v| v.normalize_or_zero())
        .filter(|v| *v != Vec3::ZERO)
        .unwrap_or_else(|| {
            let angles = e.angles().unwrap_or([0.0, 0.0, 0.0]);
            let (p, y) = (num("pitch", angles[0]).to_radians(), angles[1].to_radians());
            -Vec3::new(p.cos() * y.cos(), p.cos() * y.sin(), -p.sin())
        });
    Some(EnvSun {
        dir,
        size: num("size", 16.0),
        overlay_size: num("overlaysize", -1.0),
        color: vec("rendercolor").unwrap_or(Vec3::new(100.0, 80.0, 80.0)) / 255.0,
        overlay_color: vec("overlaycolor").unwrap_or(Vec3::ZERO) / 255.0,
        material: e
            .get("material")
            .map_or_else(|| "sprites/light_glow02_add_noz".into(), |m| m.to_string()),
    })
}

/// Source `QAngle` (pitch, yaw, roll) to a rotation: yaw about Z, pitch about Y, roll about X.
pub fn angle_matrix(angles: [f32; 3]) -> Mat3 {
    let q = Quat::from_rotation_z(angles[1].to_radians())
        * Quat::from_rotation_y(angles[0].to_radians())
        * Quat::from_rotation_x(angles[2].to_radians());
    Mat3::from_quat(q)
}

/// Load a model's MDL / VVD / VTX. `root_lod` (`r_rootlod`) is the lowest LOD drawn: the
/// VTX LOD used for every mesh and the VVD fixup level, clamped to what the model has;
/// returned as the fourth element.
pub fn load_model(mount: &Mount, path: &str, root_lod: usize) -> Result<(Mdl, Vvd, Vtx, usize)> {
    let stem = path.trim_end_matches(".mdl");
    let mdl = Mdl::parse(&mount.read(path)?)?;
    let vtx_bytes = mount
        .read(&format!("{stem}.dx90.vtx"))
        .or_else(|_| mount.read(&format!("{stem}.dx80.vtx")))
        .or_else(|_| mount.read(&format!("{stem}.sw.vtx")))?;
    let vtx = Vtx::parse(&vtx_bytes, &mdl)?;
    let max_lod = vtx
        .body_parts
        .iter()
        .flat_map(|bp| bp.models.iter())
        .map(|m| m.lods.len().saturating_sub(1))
        .min()
        .unwrap_or(0);
    let lod = root_lod.max(mdl.header.root_lod as usize).min(max_lod);
    let vvd = Vvd::parse(&mount.read(&format!("{stem}.vvd"))?, lod)?;
    Ok((mdl, vvd, vtx, lod))
}

/// Material, its base texture and, for `$normalmapalphaenvmapmask` materials, the normal map
/// whose alpha masks the reflection (the viewmodels' sheets: without it the mask reads 1 and
/// the gun turns to chrome).
pub type MaterialMap = HashMap<String, Option<(Material, Rgba, Option<Rgba>)>>;

pub fn resolve_model_texture(
    mount: &Mount,
    mdl: &Mdl,
    texture: usize,
    materials: &mut MaterialMap,
    missing: &mut Vec<String>,
) -> Option<String> {
    for path in mdl.material_paths(texture) {
        let key = format!("mdl:{}", path.trim_end_matches(".vmt"));
        if let Some(entry) = materials.get(&key) {
            if entry.is_some() {
                return Some(key);
            }
            continue;
        }
        if !mount.exists(&path) {
            continue;
        }
        let loaded = material::load_path(mount, &key, &path).ok().and_then(|m| {
            let tex = m.base_texture.clone()?;
            let rgba = material::load_texture(mount, &tex, 512).ok()?;
            let normal_mask = matches!(
                m.envmap.as_ref().map(|e| e.mask),
                Some(material::EnvMapMask::NormalAlpha)
            );
            let bump = m
                .bumpmap
                .as_ref()
                .filter(|_| normal_mask)
                .and_then(|b| material::load_texture(mount, b, 512).ok());
            Some((m, rgba, bump))
        });
        let ok = loaded.is_some();
        materials.insert(key.clone(), loaded);
        if ok {
            return Some(key);
        }
    }
    missing.push(format!(
        "{} texture {}",
        mdl.header.name,
        mdl.textures
            .get(texture)
            .map(|t| t.name.as_str())
            .unwrap_or("?")
    ));
    None
}

/// Returns (props placed, distinct models, triangles, missing material notes).
#[allow(clippy::too_many_arguments)]
pub fn collect_static_props(
    mount: &Mount,
    bsp: &Bsp<'_>,
    light: &crate::light::ModelLight,
    white_uv: [f32; 2],
    vertices: &mut Vec<Vertex>,
    materials: &mut MaterialMap,
    per_material: &mut HashMap<String, Vec<u32>>,
    per_material_sky: &mut HashMap<String, Vec<u32>>,
    skybox_bounds: &Option<([f32; 3], [f32; 3])>,
    skybox_area: Option<i16>,
    root_lod: usize,
    cube_at: &dyn Fn(Vec3) -> f32,
) -> Result<(usize, usize, usize, Vec<String>)> {
    let props = StaticProps::parse(bsp)?;
    let mut models: HashMap<u16, Option<LoadedModel>> = HashMap::new();
    let mut missing = Vec::new();
    let mut placed = 0;
    let mut triangles = 0;
    for (prop_index, prop) in props.props.iter().enumerate() {
        let baked = gm_formats::bsp::light::static_prop_vertex_light(bsp, prop_index, false)
            .ok()
            .flatten();
        let local = if baked.is_none() {
            Some(light.at(Vec3::from(prop.origin), 4))
        } else {
            None
        };
        let entry = models.entry(prop.model).or_insert_with(|| {
            let path = props.models.get(prop.model as usize)?;
            let (mdl, vvd, vtx, lod_index) = match load_model(mount, path, root_lod) {
                Ok(m) => m,
                Err(e) => {
                    missing.push(format!("{path}: {e}"));
                    return None;
                }
            };
            let texture_keys = (0..mdl.textures.len())
                .map(|t| resolve_model_texture(mount, &mdl, t, materials, &mut missing))
                .collect();
            Some(LoadedModel {
                mdl,
                vvd,
                lod: lod_index,
                vtx,
                texture_keys,
            })
        });
        let Some(model) = entry else { continue };
        let rot = angle_matrix(prop.angles);
        let origin = Vec3::from(prop.origin);
        let prop_cube = cube_at(origin);
        let scale = if prop.uniform_scale > 0.0 {
            prop.uniform_scale
        } else {
            1.0
        };
        // a prop is in the 3D skybox when it stands in the sky camera's area (like the
        // engine's area check); the face bounds catch the ones whose origin sits in a solid
        // leaf. Drawn with the world instead, a skybox building at z 11000 shows through
        // the (skipped) sky faces as a thin dark corner high in the sky
        let in_sky = skybox_area.is_some_and(|a| bsp.leafs[bsp.leaf_at(prop.origin)].area() == a)
            || skybox_bounds.is_some_and(|(mins, maxs)| {
                (0..3).all(|k| prop.origin[k] >= mins[k] && prop.origin[k] <= maxs[k])
            });
        let target = if in_sky {
            &mut *per_material_sky
        } else {
            &mut *per_material
        };
        let skin = model
            .mdl
            .skins
            .get(prop.skin.max(0) as usize)
            .or_else(|| model.mdl.skins.first());

        for (bi, bp) in model.vtx.body_parts.iter().enumerate() {
            let Some(mdl_bp) = model.mdl.body_parts.get(bi) else {
                continue;
            };
            // body group 0 only for static props
            let (Some(vtx_model), Some(mdl_model)) = (bp.models.first(), mdl_bp.models.first())
            else {
                continue;
            };
            let vertex_base = (mdl_model.vertex_index / 48).max(0) as usize;
            let Some(lod) = vtx_model.lods.get(model.lod).or(vtx_model.lods.last()) else {
                continue;
            };
            for (mi, mesh) in lod.meshes.iter().enumerate() {
                let Some(mdl_mesh) = mdl_model.meshes.get(mi) else {
                    continue;
                };
                let tex_index = skin
                    .and_then(|s| s.get(mdl_mesh.material.max(0) as usize))
                    .map(|&t| t.max(0) as usize)
                    .unwrap_or(mdl_mesh.material.max(0) as usize);
                let Some(Some(key)) = model.texture_keys.get(tex_index) else {
                    continue;
                };
                let Some(Some((mat, rgba, _))) = materials.get(key) else {
                    continue;
                };
                let (tw, th) = (rgba.width as f32, rgba.height as f32);
                let _ = (tw, th);
                let base = vertices.len() as u32;
                // remap used vertices compactly
                let mut remap: HashMap<u32, u32> = HashMap::new();
                let mut local_indices = Vec::with_capacity(mesh.indices.len());
                for &i in &mesh.indices {
                    let next = remap.len() as u32;
                    let li = *remap.entry(i).or_insert(next);
                    if li == next {
                        let Some(v) = model.vvd.vertices.get(vertex_base + i as usize) else {
                            continue;
                        };
                        let p = origin + rot * (Vec3::from(v.position) * scale);
                        let n = (rot * Vec3::from(v.normal)).normalize_or_zero();
                        // vrad's colour for this vertex (mesh-local index), else lit here
                        let baked_color = baked.as_ref().and_then(|b| {
                            let li = (i as i64 - mdl_mesh.vertex_offset as i64).max(0) as usize;
                            b.meshes.get(mi)?.get(li).copied()
                        });
                        let shade = match (baked_color, &local) {
                            (Some(c), _) => crate::light::baked_vertex_color(c),
                            (None, Some(l)) => l.shade(n, mat.half_lambert),
                            (None, None) => Vec3::splat(0.5),
                        };
                        let tint = Vec3::new(
                            prop.diffuse_modulation[0] as f32 / 255.0,
                            prop.diffuse_modulation[1] as f32 / 255.0,
                            prop.diffuse_modulation[2] as f32 / 255.0,
                        );
                        vertices.push(Vertex {
                            tint: [1.0, 1.0, 1.0, 0.0],
                            position: p.to_array(),
                            uv: v.uv,
                            lm_uv: white_uv,
                            color: (shade * tint).to_array(),
                            alpha: 0.0,
                            lm_page: 0.0,
                            normal: n.to_array(),
                            cube: prop_cube,
                            spec_dir: [0.0; 3],
                            spec_color: [0.0; 3],
                        });
                    }
                    local_indices.push(base + li);
                }
                triangles += local_indices.len() / 3;
                target.entry(key.clone()).or_default().extend(local_indices);
            }
        }
        placed += 1;
    }
    let distinct = models.values().filter(|m| m.is_some()).count();
    Ok((placed, distinct, triangles, missing))
}

/// An eyeball's frame for the `eyes` shader's iris projection: the vertex goes into the
/// eye bone's space (`pose_to_bone`), the iris uv is its offset from the eye centre along
/// right / up over the eye's diameter, scaled by `iris_scale` (studiorender's eyeball
/// projection at rest, the eye looking down `forward`).
/// An eyeball in rest-pose space (see [`EyeFrame::eye_info`]).
#[derive(Clone, Copy, Debug)]
pub struct EyeInfo {
    pub bone: usize,
    pub org: Vec3,
    pub up: Vec3,
    pub fwd: Vec3,
    pub k: f32,
}

impl EyeInfo {
    /// Iris uv of the rest-pose vertex `position` with the eye turned towards `target` (rest
    /// pose space); the eye deflects at most 45 degrees from its forward.
    pub fn iris_uv_towards(&self, position: Vec3, target: Vec3) -> [f32; 2] {
        let mut fwd = (target - self.org).normalize_or_zero();
        if fwd == Vec3::ZERO {
            fwd = self.fwd;
        }
        let max_cos = std::f32::consts::FRAC_1_SQRT_2;
        let c = fwd.dot(self.fwd);
        if c < max_cos {
            // slide the direction back onto the 45 degree cone
            let side = (fwd - self.fwd * c).normalize_or_zero();
            fwd = (self.fwd * max_cos + side * (1.0 - max_cos * max_cos).sqrt()).normalize();
        }
        let right = fwd.cross(self.up).normalize_or_zero();
        let up = right.cross(fwd).normalize_or_zero();
        let d = position - self.org;
        [0.5 + d.dot(right) * self.k, 0.5 - d.dot(up) * self.k]
    }
}

pub struct EyeFrame {
    pose_to_bone: Mat4,
    org: Vec3,
    right: Vec3,
    up: Vec3,
    radius: f32,
    iris_scale: f32,
}

impl EyeFrame {
    pub fn new(e: &gm_formats::mdl::Eyeball, pose_to_bone: &[[f32; 4]; 3]) -> EyeFrame {
        let m = pose_to_bone;
        let fwd = Vec3::from(e.forward).normalize_or_zero();
        let up = Vec3::from(e.up).normalize_or_zero();
        EyeFrame {
            pose_to_bone: Mat4::from_cols_array_2d(&[
                [m[0][0], m[1][0], m[2][0], 0.0],
                [m[0][1], m[1][1], m[2][1], 0.0],
                [m[0][2], m[1][2], m[2][2], 0.0],
                [m[0][3], m[1][3], m[2][3], 1.0],
            ]),
            org: Vec3::from(e.org),
            right: fwd.cross(up).normalize_or_zero(),
            up,
            radius: e.radius,
            iris_scale: e.iris_scale,
        }
    }

    /// The eyeball in the model's rest-pose space, for re-aiming the iris at a look target
    /// per frame (studiorender's `R_StudioEyeballPosition`): centre, up and forward and the
    /// uv scale per unit.
    pub fn eye_info(&self, bone: usize) -> EyeInfo {
        let b2p = self.pose_to_bone.inverse();
        let fwd = self.up.cross(self.right).normalize_or_zero();
        EyeInfo {
            bone,
            org: b2p.transform_point3(self.org),
            up: b2p.transform_vector3(self.up).normalize_or_zero(),
            fwd: b2p.transform_vector3(fwd).normalize_or_zero(),
            k: if self.radius > 0.0 {
                self.iris_scale.max(0.01) / (2.0 * self.radius)
            } else {
                0.0
            },
        }
    }

    /// Iris texture uv of a rest-pose vertex (outside 0..1 the sclera shows).
    pub fn iris_uv(&self, position: [f32; 3]) -> [f32; 2] {
        if self.radius <= 0.0 {
            return [-1.0, -1.0];
        }
        let d = self.pose_to_bone.transform_point3(Vec3::from(position)) - self.org;
        let k = self.iris_scale.max(0.01) / (2.0 * self.radius);
        [0.5 + d.dot(self.right) * k, 0.5 - d.dot(self.up) * k]
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A citizen's eyeball meshes project their iris around the eye's front.
    #[test]
    fn citizen_eyeballs_project_an_iris() {
        let Some(inst) = gm_content::find_install() else {
            eprintln!("no GMod install, skipped");
            return;
        };
        let mount = Mount::for_install(&inst).unwrap();
        let (mdl, vvd, _vtx, _) =
            load_model(&mount, "models/humans/group01/male_01.mdl", 0).unwrap();
        let mut eyes = 0;
        for bp in &mdl.body_parts {
            for m in &bp.models {
                let base = (m.vertex_index / 48).max(0) as usize;
                for mesh in m.meshes.iter().filter(|k| k.material_type == 1) {
                    let e = &m.eyeballs[mesh.material_param as usize];
                    let bone = &mdl.bones[e.bone as usize];
                    let f = EyeFrame::new(e, &bone.pose_to_bone);
                    let (mut lo, mut hi) = ([f32::MAX; 2], [f32::MIN; 2]);
                    let n = mesh.num_vertices.max(0) as usize;
                    for v in &vvd.vertices[base + mesh.vertex_offset.max(0) as usize..][..n] {
                        let uv = f.iris_uv(v.position);
                        for i in 0..2 {
                            lo[i] = lo[i].min(uv[i]);
                            hi[i] = hi[i].max(uv[i]);
                        }
                    }
                    eprintln!(
                        "{} bone {} org {:?} radius {} iris_scale {} uv {:?}..{:?}",
                        e.name, e.bone, e.org, e.radius, e.iris_scale, lo, hi
                    );
                    assert!(lo[0] < 0.5 && hi[0] > 0.5 && lo[1] < 0.5 && hi[1] > 0.5);
                    assert!(lo[0] > -3.0 && hi[0] < 4.0, "{lo:?} {hi:?}");
                    eyes += 1;
                }
            }
        }
        assert_eq!(eyes, 2);
    }

    #[test]
    fn yaw_rotates_forward_to_left() {
        let m = angle_matrix([0.0, 90.0, 0.0]);
        let v = m * Vec3::X;
        assert!((v - Vec3::Y).length() < 1e-5, "{v}");
        let m = angle_matrix([90.0, 0.0, 0.0]);
        let v = m * Vec3::X;
        assert!((v - Vec3::NEG_Z).length() < 1e-5, "{v}");
    }
}
