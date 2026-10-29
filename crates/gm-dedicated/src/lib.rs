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
    })
}
