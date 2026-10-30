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
    let mut physics = World::new(600.0, gm_core::tick::DEFAULT_TICK_INTERVAL, 1);
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
pub fn material_surfaceprop(mount: &gm_content::Mount, material: &str) -> String {
    fn lookup(mount: &gm_content::Mount, material: &str, depth: u32) -> Option<String> {
        if depth > 4 {
            return None;
        }
        let path = format!("materials/{}.vmt", material.trim_end_matches(".vmt"));
        let src = String::from_utf8_lossy(&mount.read(&path).ok()?).into_owned();
        let doc = gm_formats::keyvalues::parse(&src).ok()?;
        let root = doc.root()?;
        if root.key.eq_ignore_ascii_case("patch") {
            for block in ["replace", "insert"] {
                if let Some(v) = root.get(block).and_then(|b| b.get_str("$surfaceprop")) {
                    return Some(v.to_string());
                }
            }
            let include = root.get_str("include")?;
            let include = include
                .replace('\\', "/")
                .trim_start_matches("materials/")
                .to_string();
            return lookup(mount, &include, depth + 1);
        }
        root.get_str("$surfaceprop").map(str::to_string)
    }
    lookup(mount, material, 0)
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
        Some(m) => material_surfaceprop(mount, m),
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
pub fn model_info(mount: &gm_content::Mount, model: &str) -> Result<gm_server::ModelInfo> {
    let mdl = gm_formats::mdl::Mdl::parse(&mount.read(model)?)?;
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
    Ok(gm_server::ModelInfo {
        bounds: gm_sandbox::ModelBounds {
            min: Vec3::from(mdl.header.hull_min),
            max: Vec3::from(mdl.header.hull_max),
        },
        hulls,
        hull_planes,
        mass: if props.mass > 0.0 { props.mass } else { 1.0 },
        volume: props.volume,
        surface: Surface {
            friction: sp.friction,
            elasticity: sp.elasticity,
        },
        linear_damping: props.damping,
        angular_damping: props.rotdamping,
        drag_axis_areas: Vec3::from(solid.drag_axis_areas),
        inertia: Some(Vec3::from(solid.rotation_inertia)),
        impact_hard: table.sound(&props.surfaceprop, "impacthard"),
    })
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
