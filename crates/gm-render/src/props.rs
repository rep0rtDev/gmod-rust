//! Static props baked into world space, lit by a fixed ambient plus the
//! `light_environment` direction (a stand-in for vrad's per-vertex lighting).

use std::collections::HashMap;

use anyhow::Result;
use glam::{Mat3, Quat, Vec3};
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

/// Source `QAngle` (pitch, yaw, roll) to a rotation: yaw about Z, pitch about Y, roll about X.
pub fn angle_matrix(angles: [f32; 3]) -> Mat3 {
    let q = Quat::from_rotation_z(angles[1].to_radians())
        * Quat::from_rotation_y(angles[0].to_radians())
        * Quat::from_rotation_x(angles[2].to_radians());
    Mat3::from_quat(q)
}

pub fn load_model(mount: &Mount, path: &str) -> Result<(Mdl, Vvd, Vtx)> {
    let stem = path.trim_end_matches(".mdl");
    let mdl = Mdl::parse(&mount.read(path)?)?;
    let vvd = Vvd::parse(
        &mount.read(&format!("{stem}.vvd"))?,
        mdl.header.root_lod as usize,
    )?;
    let vtx_bytes = mount
        .read(&format!("{stem}.dx90.vtx"))
        .or_else(|_| mount.read(&format!("{stem}.dx80.vtx")))
        .or_else(|_| mount.read(&format!("{stem}.sw.vtx")))?;
    let vtx = Vtx::parse(&vtx_bytes, &mdl)?;
    Ok((mdl, vvd, vtx))
}

type MaterialMap = HashMap<String, Option<(Material, Rgba)>>;

fn resolve_model_texture(
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
            Some((m, rgba))
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
    sun: Vec3,
    white_uv: [f32; 2],
    vertices: &mut Vec<Vertex>,
    materials: &mut MaterialMap,
    per_material: &mut HashMap<String, Vec<u32>>,
    per_material_sky: &mut HashMap<String, Vec<u32>>,
    skybox_bounds: &Option<([f32; 3], [f32; 3])>,
) -> Result<(usize, usize, usize, Vec<String>)> {
    let props = StaticProps::parse(bsp)?;
    let mut models: HashMap<u16, Option<LoadedModel>> = HashMap::new();
    let mut missing = Vec::new();
    let mut placed = 0;
    let mut triangles = 0;
    for prop in &props.props {
        let entry = models.entry(prop.model).or_insert_with(|| {
            let path = props.models.get(prop.model as usize)?;
            let (mdl, vvd, vtx) = match load_model(mount, path) {
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
                vtx,
                texture_keys,
            })
        });
        let Some(model) = entry else { continue };
        let rot = angle_matrix(prop.angles);
        let origin = Vec3::from(prop.origin);
        let scale = if prop.uniform_scale > 0.0 {
            prop.uniform_scale
        } else {
            1.0
        };
        let in_sky = skybox_bounds.is_some_and(|(mins, maxs)| {
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
            let Some(lod) = vtx_model.lods.first() else {
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
                let Some(Some((_, rgba))) = materials.get(key) else {
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
                        let shade = 0.45 + 0.55 * n.dot(sun).max(0.0);
                        let tint = [
                            prop.diffuse_modulation[0] as f32 / 255.0,
                            prop.diffuse_modulation[1] as f32 / 255.0,
                            prop.diffuse_modulation[2] as f32 / 255.0,
                        ];
                        vertices.push(Vertex {
                            position: p.to_array(),
                            uv: v.uv,
                            lm_uv: white_uv,
                            color: [shade * tint[0], shade * tint[1], shade * tint[2]],
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

#[cfg(test)]
mod tests {
    use super::*;

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
