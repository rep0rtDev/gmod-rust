//! World loading for a server process: the map's brushes and displacements into the collision
//! world (player movement, traces) and the Jolt world (props), and models from MDL/PHY.

use anyhow::{Context, Result};
use glam::Vec3;
use gm_physics::{BodyHandle, Surface, World};

pub struct LoadedWorld {
    pub collision: gm_collision::World,
    pub physics: World,
    pub world_body: BodyHandle,
    pub nav: std::sync::Arc<gm_nav::NavMesh>,
    /// The map's `info_player_start` points (`Server::spawn_points`).
    pub spawn_points: Vec<gm_server::SpawnPoint>,
    pub ai_nodes: Vec<gm_server::AiNode>,
    pub func_tanks: Vec<gm_server::FuncTank>,
    /// The compiled node graph `maps/graphs/<map>.ain`, when the install has one.
    pub ai_graph: Option<gm_formats::ain::Ain>,
    /// BSP header revision (`game.GetMapVersion`) and the direction towards the sun from
    /// `light_environment` (`game.GetSunInfo`), if the map has one.
    pub map_revision: i32,
    pub sun_direction: Option<Vec3>,
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
    let nav = gm_nav::NavMesh::build(&bsp, &gm_nav::NavParams::default())
        .map_err(|e| anyhow::anyhow!("navmesh for {map}: {e}"))?;
    Ok(LoadedWorld {
        collision,
        physics,
        world_body,
        nav: std::sync::Arc::new(nav),
        spawn_points: gm_server::spawn_points_from_bsp(&bsp),
        ai_nodes: gm_server::ai_nodes_from_bsp(&bsp),
        func_tanks: gm_server::func_tanks_from_bsp(&bsp),
        ai_graph: mount
            .read(&format!("maps/graphs/{map}.ain"))
            .ok()
            .and_then(|b| gm_formats::ain::Ain::parse(&b).ok()),
        map_revision: bsp.header.map_revision,
        sun_direction: bsp
            .entities
            .iter()
            .find(|e| e.classname() == "light_environment")
            .map(|e| {
                let angles = e.angles().unwrap_or([0.0, 0.0, 0.0]);
                let pitch = e
                    .get("pitch")
                    .and_then(|p| p.parse::<f32>().ok())
                    .unwrap_or(angles[0]);
                let (p, y) = (pitch.to_radians(), angles[1].to_radians());
                -Vec3::new(p.cos() * y.cos(), p.cos() * y.sin(), -p.sin())
            }),
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
    sequence_frame0(mount, model, data, mdl, Some("ragdoll"))
}

/// Frame 0 of the named sequence (or the model's first sequence when absent / `None`): the pose
/// an idle entity of the model stands in.
fn sequence_frame0(
    mount: &gm_content::Mount,
    model: &str,
    data: &[u8],
    mdl: &gm_formats::mdl::Mdl,
    name: Option<&str>,
) -> Option<Vec<([f32; 3], [f32; 4])>> {
    let seqs = gm_formats::mdl::sequences(data, &mdl.header);
    let seq = name
        .and_then(|n| seqs.iter().find(|s| s.name.eq_ignore_ascii_case(n)))
        .or(seqs.first())?;
    let desc = gm_formats::mdl::anim_descs(data, &mdl.header)
        .into_iter()
        .nth(seq.anim.max(0) as usize)?;
    let delta = desc.flags & 0x04 != 0;
    // some models carry a negative animation offset (no frame data for the pose)
    let index = usize::try_from(desc.index).ok()?;
    if desc.block == 0 {
        let start = usize::try_from(mdl.header.local_anim_offset)
            .ok()?
            .checked_add(index)?;
        return gm_formats::mdl::anim_pose(data, start, &mdl.bones, 0, delta);
    }
    let ani = mount.read(&model.replace(".mdl", ".ani")).ok()?;
    let (_, blocks) = gm_formats::mdl::anim_blocks(data);
    let (start, _) = *blocks.get(usize::try_from(desc.block).ok()?)?;
    gm_formats::mdl::anim_pose(&ani, start.checked_add(index)?, &mdl.bones, 0, delta)
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
        script: gm_server::ModelScriptData::from_mdl_with_includes(&mdl, &mdl_data, |p| {
            mount.read(p).ok()
        }),
        attachments: model_attachments(
            &mdl,
            sequence_frame0(mount, model, &mdl_data, &mdl, None).as_deref(),
        ),
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
        // a material without its own soft script falls back to the default block (tile:
        // the game plays Default.ImpactSoft, plastic_box_impact_soft2, `move_prop_step_*`)
        impact_soft: table
            .sound(&props.surfaceprop, "impactsoft")
            .or_else(|| table.sound("default", "impactsoft")),
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

/// Attachments carried into model space through their bone chain in the model's idle pose
/// (frame 0 of its first sequence, like the game's `GetAttachment` on an entity standing in
/// it; the bind pose as fallback). Measured `server_vehicle_seat` / `server_vehicle_spawn`:
/// the idle frame turns a one-bone static prop's root a quarter turn and the jeep's a half
/// turn against the bind pose, so the bind pose alone put the jeep seat 97 u off.
fn model_attachments(
    mdl: &gm_formats::mdl::Mdl,
    idle: Option<&[([f32; 3], [f32; 4])]>,
) -> Vec<gm_server::ModelAttachment> {
    fn bind(
        mdl: &gm_formats::mdl::Mdl,
        idle: Option<&[([f32; 3], [f32; 4])]>,
        bone: usize,
    ) -> glam::Mat4 {
        let b = &mdl.bones[bone];
        let (pos, quat) = idle
            .and_then(|p| p.get(bone).copied())
            .unwrap_or((b.pos, b.quat));
        let local = glam::Mat4::from_rotation_translation(
            glam::Quat::from_array(quat).normalize(),
            glam::Vec3::from_array(pos),
        );
        if b.parent >= 0 && (b.parent as usize) < mdl.bones.len() && b.parent as usize != bone {
            bind(mdl, idle, b.parent as usize) * local
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
            let m = bind(mdl, idle, bone);
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

/// Seconds of a RIFF WAVE file from its `fmt ` and `data` chunks (`SoundDuration`).
pub fn wav_duration_bytes(b: &[u8]) -> Option<f32> {
    if b.len() < 12 || &b[0..4] != b"RIFF" || &b[8..12] != b"WAVE" {
        return None;
    }
    let (mut rate, mut block, mut data) = (0u32, 0u32, None);
    let mut i = 12;
    while i + 8 <= b.len() {
        let id = &b[i..i + 4];
        let size = u32::from_le_bytes(b[i + 4..i + 8].try_into().ok()?) as usize;
        let body = &b[i + 8..(i + 8 + size).min(b.len())];
        if id == b"fmt " && body.len() >= 14 {
            rate = u32::from_le_bytes(body[4..8].try_into().ok()?);
            block = u32::from(u16::from_le_bytes(body[12..14].try_into().ok()?));
        } else if id == b"data" {
            data = Some(size as u32);
        }
        i += 8 + size + (size & 1);
    }
    let data = data?;
    (rate > 0 && block > 0).then(|| data as f32 / (rate * block) as f32)
}

/// `SoundDuration(name)`: the length of `sound/<name>` from the mounted content (WAV only).
pub fn wav_duration(mount: &gm_content::Mount, name: &str) -> Option<f32> {
    let path = format!("sound/{}", name.trim_start_matches('/'));
    wav_duration_bytes(&mount.read(&path).ok()?)
}

#[cfg(test)]
mod wav_tests {
    #[test]
    fn wav_duration_reads_fmt_and_data_chunks() {
        let mut b = Vec::new();
        b.extend_from_slice(b"RIFF");
        b.extend_from_slice(&0u32.to_le_bytes());
        b.extend_from_slice(b"WAVE");
        b.extend_from_slice(b"fmt ");
        b.extend_from_slice(&16u32.to_le_bytes());
        b.extend_from_slice(&1u16.to_le_bytes()); // pcm
        b.extend_from_slice(&1u16.to_le_bytes()); // mono
        b.extend_from_slice(&44100u32.to_le_bytes());
        b.extend_from_slice(&88200u32.to_le_bytes());
        b.extend_from_slice(&2u16.to_le_bytes()); // block align
        b.extend_from_slice(&16u16.to_le_bytes());
        b.extend_from_slice(b"data");
        b.extend_from_slice(&(44100u32 * 2 * 3).to_le_bytes());
        assert_eq!(super::wav_duration_bytes(&b), Some(3.0));
        assert_eq!(super::wav_duration_bytes(b"RIFX"), None);
    }
}
