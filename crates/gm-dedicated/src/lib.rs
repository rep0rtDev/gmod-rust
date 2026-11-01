//! World loading for a server process: the map's brushes and displacements into the collision
//! world (player movement, traces) and the Jolt world (props), and models from MDL/PHY.

use anyhow::{Context, Result};
use glam::Vec3;
use gm_physics::{BodyHandle, Surface, World};

pub struct LoadedWorld {
    pub collision: gm_collision::World,
    pub physics: World,
    pub world_body: BodyHandle,
}

/// Default Jolt solver settings used by all harnesses (`tools/parity/runner` mirrors this).
pub fn tune_solver(_world: &mut World) {}

pub fn load_world(mount: &gm_content::Mount, map: &str) -> Result<LoadedWorld> {
    let data = mount
        .read(&format!("maps/{map}.bsp"))
        .with_context(|| format!("map {map}"))?;
    let bsp = gm_formats::bsp::Bsp::parse(&data)?;
    let collision = gm_collision::World::from_bsp(&bsp);
    let world_hulls: Vec<Vec<Vec3>> = gm_physics::brush::model_hulls(&bsp, 0, gm_physics::REST_GAP)
        .into_iter()
        .map(|h| h.points)
        .collect();
    let table = gm_physics::surface::SurfaceTable::parse(&[&String::from_utf8_lossy(
        &mount.read("scripts/surfaceproperties.txt")?,
    )]);
    let sp = table.get("concrete");
    let surface = Surface {
        friction: sp.friction,
        elasticity: sp.elasticity,
    };
    // 0 worker threads: the impact listener applies impulses from Jolt's contact callbacks,
    // and with workers their order (and the float sums) differs run to run
    let mut physics = World::new(600.0, gm_core::tick::DEFAULT_TICK_INTERVAL, 0);
    tune_solver(&mut physics);
    let world_body = physics.add_static_hulls(&world_hulls, surface)?;
    let (disp_pos, disp_idx) = gm_physics::brush::displacement_mesh(&bsp, gm_physics::REST_GAP);
    if !disp_idx.is_empty() {
        physics.add_static_mesh(&disp_pos, &disp_idx, surface)?;
    }
    Ok(LoadedWorld {
        collision,
        physics,
        world_body,
    })
}

/// `$surfaceprop` of a material (`concrete/concretefloor001a`), through a `patch` shader's
/// include; `default` when the material or the key is missing, as the engine falls back.
/// `name#2` (a displacement blend's second material) reads `$surfaceprop2`, then `$surfaceprop`.
pub fn material_surfaceprop(mount: &gm_content::Mount, material: &str) -> String {
    fn lookup(mount: &gm_content::Mount, material: &str, key: &str, depth: u32) -> Option<String> {
        if depth > 4 {
            return None;
        }
        let path = format!("materials/{}.vmt", material.trim_end_matches(".vmt"));
        let src = String::from_utf8_lossy(&mount.read(&path).ok()?).into_owned();
        let doc = gm_formats::keyvalues::parse(&src).ok()?;
        let root = doc.root()?;
        if root.key.eq_ignore_ascii_case("patch") {
            for block in ["replace", "insert"] {
                if let Some(v) = root.get(block).and_then(|b| b.get_str(key)) {
                    return Some(v.to_string());
                }
            }
            let include = root.get_str("include")?;
            let include = include
                .replace('\\', "/")
                .trim_start_matches("materials/")
                .to_string();
            return lookup(mount, &include, key, depth + 1);
        }
        root.get_str(key).map(str::to_string)
    }
    match material.strip_suffix("#2") {
        Some(m) => {
            lookup(mount, m, "$surfaceprop2", 0).or_else(|| lookup(mount, m, "$surfaceprop", 0))
        }
        None => lookup(mount, material, "$surfaceprop", 0),
    }
    .map(|s| s.to_ascii_lowercase())
    .unwrap_or_else(|| "default".to_string())
}

/// Footstep scripts of a brush material for `Server::step_loader`: the material's `$surfaceprop`
/// (or `default` without a material) and its `stepleft`/`stepright` through the base chain.
pub fn step_sounds(
    mount: &gm_content::Mount,
    table: &gm_physics::surface::SurfaceTable,
    material: Option<&str>,
) -> Option<gm_server::StepSounds> {
    let prop = match material {
        Some(m) => match m.strip_prefix("surfaceprop:") {
            Some(p) => p.to_string(),
            None => material_surfaceprop(mount, m),
        },
        None => "default".to_string(),
    };
    let pick = |key: &str| {
        table
            .sound(&prop, key)
            .or_else(|| table.sound("default", key))
    };
    Some(gm_server::StepSounds {
        left: pick("stepleft")?,
        right: pick("stepright")?,
    })
}

/// The surface property table of the mounted game.
pub fn surface_table(mount: &gm_content::Mount) -> Result<gm_physics::surface::SurfaceTable> {
    Ok(gm_physics::surface::SurfaceTable::parse(&[
        &String::from_utf8_lossy(&mount.read("scripts/surfaceproperties.txt")?),
    ]))
}

/// MDL hull box, PHY convexes (points and outward planes) and physics properties of a model.
/// Frame 0 of the model's `ragdoll` sequence (or its first sequence): the pose a `prop_ragdoll`
/// spawns in. Character models carry their facing in the root bone of that frame, not in the
/// bind pose, so a ragdoll built from the bind pose would face 90 degrees off.
fn ragdoll_pose(
    mount: &gm_content::Mount,
    model: &str,
    data: &[u8],
    mdl: &gm_formats::mdl::Mdl,
) -> Option<Vec<([f32; 3], [f32; 4])>> {
    let seqs = gm_formats::mdl::sequences(data, &mdl.header);
    let seq = seqs
        .iter()
        .find(|s| s.name.eq_ignore_ascii_case("ragdoll"))
        .or(seqs.first())?;
    let desc = gm_formats::mdl::anim_descs(data, &mdl.header)
        .into_iter()
        .nth(seq.anim.max(0) as usize)?;
    let delta = desc.flags & 0x04 != 0;
    if desc.block == 0 {
        let start = mdl.header.local_anim_offset as usize + desc.index as usize;
        return gm_formats::mdl::anim_pose(data, start, &mdl.bones, 0, delta);
    }
    let ani = mount.read(&model.replace(".mdl", ".ani")).ok()?;
    let (_, blocks) = gm_formats::mdl::anim_blocks(data);
    let (start, _) = *blocks.get(desc.block as usize)?;
    gm_formats::mdl::anim_pose(&ani, start + desc.index as usize, &mdl.bones, 0, delta)
}

pub fn model_info(mount: &gm_content::Mount, model: &str) -> Result<gm_server::ModelInfo> {
    let mdl_data = mount.read(model)?;
    let mdl = gm_formats::mdl::Mdl::parse(&mdl_data)?;
    let phy_path = model
        .strip_suffix(".mdl")
        .map(|m| format!("{m}.phy"))
        .unwrap_or_else(|| model.to_string());
    let phy = gm_formats::phy::Phy::parse(&mount.read(&phy_path)?)?;
    let solid = phy.solids.first().context("PHY has no solids")?;
    let props = phy.props.first().cloned().unwrap_or_default();
    let table = gm_physics::surface::SurfaceTable::parse(&[&String::from_utf8_lossy(
        &mount.read("scripts/surfaceproperties.txt")?,
    )]);
    let sp = table.get(&props.surfaceprop);
    let hulls: Vec<Vec<Vec3>> = solid
        .convexes
        .iter()
        .map(|c| c.points.iter().map(|&p| Vec3::from(p)).collect())
        .collect();
    let hull_planes = solid
        .convexes
        .iter()
        .map(|c| {
            let pts: Vec<Vec3> = c.points.iter().map(|&p| Vec3::from(p)).collect();
            gm_sandbox::hull_planes(&pts, &c.triangles)
        })
        .collect();
    let ragdoll = (phy.solids.len() > 1).then(|| {
        let sp_of = |name: &str| {
            let sp = table.get(name);
            Surface {
                friction: sp.friction,
                elasticity: sp.elasticity,
            }
        };
        gm_server::RagdollInfo {
            solids: phy
                .solids
                .iter()
                .enumerate()
                .map(|(i, s)| {
                    let pr = phy.props.get(i).cloned().unwrap_or_default();
                    gm_server::RagdollSolid {
                        bone: pr.name.clone(),
                        hulls: s
                            .convexes
                            .iter()
                            .map(|c| c.points.iter().map(|&p| Vec3::from(p)).collect())
                            .collect(),
                        planes: s
                            .convexes
                            .iter()
                            .map(|c| {
                                let pts: Vec<Vec3> =
                                    c.points.iter().map(|&p| Vec3::from(p)).collect();
                                gm_sandbox::hull_planes(&pts, &c.triangles)
                            })
                            .collect(),
                        mass: if pr.mass > 0.0 { pr.mass } else { 1.0 },
                        volume: pr.volume,
                        mass_center: Vec3::from(s.mass_center),
                        inertia: Vec3::from(s.rotation_inertia),
                        drag_axis_areas: Vec3::from(s.drag_axis_areas),
                        surface: sp_of(&pr.surfaceprop),
                        surfaceprop: pr.surfaceprop.clone(),
                        linear_damping: pr.damping,
                        angular_damping: pr.rotdamping,
                    }
                })
                .collect(),
            joints: phy.constraints.clone(),
            collision_pairs: phy.collision_pairs.clone(),
            total_mass: phy.edit.total_mass,
            anim_friction: phy.anim_friction.clone(),
            // measured (kleiner / alyx): the hull box widened by the root solid's convex
            // extents in its bone frame (mins.z -5.94 / -5.87 = the pelvis hull's bottom)
            attachments: mdl
                .attachments
                .iter()
                .filter_map(|a| {
                    let m = &a.local;
                    let col = |c: usize| glam::Vec3::new(m[0][c], m[1][c], m[2][c]);
                    Some(gm_server::AttachmentInfo {
                        name: a.name.clone(),
                        bone: usize::try_from(a.bone).ok()?,
                        pos: glam::Vec3::from(a.position()),
                        rot: glam::Quat::from_mat3(&glam::Mat3::from_cols(col(0), col(1), col(2)))
                            .normalize(),
                    })
                })
                .collect(),
            flex_controllers: mdl.flex_controllers.clone(),
            bounds: {
                let mut b = gm_sandbox::ModelBounds {
                    min: Vec3::from(mdl.header.hull_min),
                    max: Vec3::from(mdl.header.hull_max),
                };
                for p in solid.convexes.iter().flat_map(|c| c.points.iter()) {
                    b.min = b.min.min(Vec3::from(*p));
                    b.max = b.max.max(Vec3::from(*p));
                }
                b
            },
            bones: ragdoll_pose(mount, model, &mdl_data, &mdl)
                .unwrap_or_else(|| mdl.bones.iter().map(|b| (b.pos, b.quat)).collect())
                .into_iter()
                .zip(&mdl.bones)
                .map(|((pos, quat), b)| gm_server::BoneInfo {
                    name: b.name.clone(),
                    parent: b.parent,
                    pos: Vec3::from(pos),
                    quat: glam::Quat::from_array(quat),
                })
                .collect(),
        }
    });
    Ok(gm_server::ModelInfo {
        attachments: model_attachments(&mdl),
        ragdoll,
        bounds: gm_sandbox::ModelBounds {
            min: Vec3::from(mdl.header.hull_min),
            max: Vec3::from(mdl.header.hull_max),
        },
        hulls,
        hull_planes,
        mass_center: Vec3::from(solid.mass_center),
        mass: if props.mass > 0.0 { props.mass } else { 1.0 },
        volume: props.volume,
        surfaceprop: props.surfaceprop.clone(),
        surface: Surface {
            friction: sp.friction,
            elasticity: sp.elasticity,
        },
        linear_damping: props.damping,
        angular_damping: props.rotdamping,
        drag_axis_areas: Vec3::from(solid.drag_axis_areas),
        inertia: Some(Vec3::from(solid.rotation_inertia)),
        impact_hard: table.sound(&props.surfaceprop, "impacthard"),
        scrape_rough: table.sound(&props.surfaceprop, "scraperough"),
        prop_data: prop_data(mount, &mdl.key_values),
        break_sound: table.sound(&props.surfaceprop, "break"),
        skins: mdl.skins.len().clamp(1, 255) as u8,
        body_groups: mdl
            .body_parts
            .iter()
            .map(|bp| bp.models.len().min(255) as u8)
            .collect(),
    })
}

/// The model's `prop_data` (`$keyvalues`) through the `base` chain of `scripts/propdata.txt`:
/// the crate is `Wooden.Medium`, health 30 (measured, `lua_fire`), the gas can says 20 itself;
/// each key is the first found walking up the chain. No `prop_data` is unbreakable.
pub fn prop_data(mount: &gm_content::Mount, key_values: &str) -> gm_server::PropData {
    let mut out = gm_server::PropData::default();
    let Ok(doc) = gm_formats::keyvalues::parse(key_values) else {
        return out;
    };
    let Some(pd) = doc.root().and_then(|r| r.get("prop_data")) else {
        return out;
    };
    let table = mount
        .read("scripts/propdata.txt")
        .ok()
        .and_then(|b| gm_formats::keyvalues::parse(&String::from_utf8_lossy(&b)).ok());
    let classes = table.as_ref().and_then(|d| d.root());
    let mut chain = Vec::new();
    let mut node = Some(pd);
    for _ in 0..8 {
        let Some(n) = node else { break };
        chain.push(n);
        node = n.get_str("base").and_then(|b| classes?.get(b));
    }
    let first = |key: &str| {
        chain
            .iter()
            .find_map(|n| n.get_str(key).and_then(|v| v.trim().parse::<f32>().ok()))
    };
    out.health = first("health").map_or(0, |h| h as i32);
    out.dmg_bullets = first("dmg.bullets").unwrap_or(1.0);
    out.dmg_club = first("dmg.club").unwrap_or(1.0);
    out.dmg_explosive = first("dmg.explosive").unwrap_or(1.0);
    out
}

/// `gm-dedicated --config server.toml`: the command line's fields as a file, plus `[cvars]`.
/// An explicit command-line flag wins over the file, the file over the built-in default; the
/// lists (`addon`, `trust`, `admin`, `ban`) are merged.
#[derive(Debug, Default, Clone, serde::Deserialize)]
#[serde(deny_unknown_fields, default)]
pub struct ServerConfig {
    pub map: Option<String>,
    pub bind: Option<String>,
    pub name: Option<String>,
    pub max_players: Option<u16>,
    pub spawn: Option<[f32; 3]>,
    pub data: Option<std::path::PathBuf>,
    pub watch: Option<bool>,
    pub require_auth: Option<bool>,
    /// File with the server's ed25519 secret (hex); created when missing. Clients pin its public
    /// key with `gm-bot --server-key`, the browser shows it.
    pub server_key: Option<std::path::PathBuf>,
    pub timeout: Option<f32>,
    pub registry: Option<String>,
    pub master: Option<String>,
    pub addon: Vec<std::path::PathBuf>,
    pub trust: Vec<String>,
    pub admin: Vec<String>,
    pub ban: Vec<String>,
    /// Console variables applied after the map loads (`sbox_maxprops = "150"`, numbers allowed).
    pub cvars: std::collections::BTreeMap<String, toml::Value>,
}

impl ServerConfig {
    pub fn parse(text: &str) -> Result<Self> {
        Ok(toml::from_str(text)?)
    }

    pub fn load(path: &std::path::Path) -> Result<Self> {
        let text = std::fs::read_to_string(path).with_context(|| path.display().to_string())?;
        Self::parse(&text).with_context(|| path.display().to_string())
    }

    /// `[cvars]` as strings the server's `set_cvar` takes.
    pub fn cvar_strings(&self) -> Vec<(String, String)> {
        self.cvars
            .iter()
            .map(|(k, v)| {
                let s = match v {
                    toml::Value::String(s) => s.clone(),
                    toml::Value::Integer(i) => i.to_string(),
                    toml::Value::Float(f) => f.to_string(),
                    toml::Value::Boolean(b) => (*b as u8).to_string(),
                    other => other.to_string(),
                };
                (k.clone(), s)
            })
            .collect()
    }
}

/// Attachments carried into model space through their bone's bind pose (the bone chain's
/// `pos` / `quat`, the pose a static prop is drawn in).
fn model_attachments(mdl: &gm_formats::mdl::Mdl) -> Vec<gm_server::ModelAttachment> {
    fn bind(mdl: &gm_formats::mdl::Mdl, bone: usize) -> glam::Mat4 {
        let b = &mdl.bones[bone];
        let local = glam::Mat4::from_rotation_translation(
            glam::Quat::from_array(b.quat).normalize(),
            glam::Vec3::from_array(b.pos),
        );
        if b.parent >= 0 && (b.parent as usize) < mdl.bones.len() && b.parent as usize != bone {
            bind(mdl, b.parent as usize) * local
        } else {
            local
        }
    }
    mdl.attachments
        .iter()
        .filter_map(|a| {
            let bone = a.bone.max(0) as usize;
            if bone >= mdl.bones.len() {
                return None;
            }
            // measured (`server_vehicle_seat`: nova chairs, jeep seat): on a one-bone static
            // prop the engine's attachment sits a quarter turn further round z than the bone
            // chain gives ((3, 19, 0) in the bone reads (0, 3, 19.5) on the chair), the
            // $staticprop compile convention
            let m = if mdl.bones.len() == 1 {
                glam::Mat4::from_rotation_z(std::f32::consts::FRAC_PI_2) * bind(mdl, bone)
            } else {
                bind(mdl, bone)
            };
            let l = &a.local;
            Some(gm_server::ModelAttachment {
                name: a.name.clone(),
                pos: m.transform_point3(glam::Vec3::new(l[0][3], l[1][3], l[2][3])),
                forward: m.transform_vector3(glam::Vec3::new(l[0][0], l[1][0], l[2][0])),
            })
        })
        .collect()
}

#[cfg(test)]
mod config_tests {
    use super::ServerConfig;

    #[test]
    fn a_server_file_parses_and_rejects_unknown_keys() {
        let c = ServerConfig::parse(
            r#"
map = "gm_flatgrass"
name = "Box"
max_players = 24
require_auth = true
admin = ["00112233445566778899aabbccddeeff00112233445566778899aabbccddeeff"]
addon = ["addons/wire"]
[cvars]
sbox_maxprops = 150
sbox_noclip = false
sbox_godmode = "1"
"#,
        )
        .unwrap();
        assert_eq!(c.map.as_deref(), Some("gm_flatgrass"));
        assert_eq!(c.max_players, Some(24));
        assert_eq!(c.require_auth, Some(true));
        assert_eq!(c.admin.len(), 1);
        assert_eq!(c.bind, None);
        let cv = c.cvar_strings();
        assert_eq!(
            cv,
            vec![
                ("sbox_godmode".to_string(), "1".to_string()),
                ("sbox_maxprops".to_string(), "150".to_string()),
                ("sbox_noclip".to_string(), "0".to_string()),
            ]
        );
        assert!(
            ServerConfig::parse("mpa = \"x\"").is_err(),
            "typos are errors"
        );
    }
}
