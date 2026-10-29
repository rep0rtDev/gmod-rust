//! `gm-parity`: list, run and compare parity harnesses.
//!
//! A harness has three parts: a scenario (inputs), the trace produced by the
//! real game's probe addon, and the trace produced by gmod-rust. `compare`
//! applies the tolerance from `harnesses.toml` and prints a report.

use std::collections::{BTreeMap, BTreeSet};
use std::path::{Path, PathBuf};

use anyhow::{bail, Context, Result};
use clap::{Parser, Subcommand};
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};

#[derive(Parser)]
#[command(name = "gm-parity", about = "parity harnesses against the real game")]
struct Cli {
    /// Path to harnesses.toml
    #[arg(long, default_value = "tools/parity/harnesses.toml")]
    manifest: PathBuf,
    #[command(subcommand)]
    cmd: Cmd,
}

#[derive(Subcommand)]
enum Cmd {
    /// List harnesses, optionally filtered by name prefix
    List {
        #[arg(default_value = "")]
        prefix: String,
    },
    /// Stage scenarios into a real server's data directory for the probe addon
    Stage {
        /// `<srcds>/garrysmod` directory
        #[arg(long)]
        game: PathBuf,
        #[arg(default_value = "")]
        prefix: String,
    },
    /// Run the gmod-rust side of every scenario matching the prefix and write traces
    RunOurs {
        #[arg(long, default_value = "tools/parity/out/ours")]
        out: PathBuf,
        #[arg(default_value = "")]
        prefix: String,
    },
    /// Replay a phys scenario from the game's recorded state at tick `from` (pose and
    /// velocities of the golden trace) for a few ticks and print ours against the game's
    Replay {
        /// scenario json, e.g. tools/parity/scenarios/phys/spin_crate_x200.json
        scenario: PathBuf,
        #[arg(long)]
        from: usize,
        #[arg(long, default_value_t = 3)]
        ticks: usize,
    },
    /// Compare traces from the real game (`--theirs`, e.g. `<srcds>/garrysmod/data/parity/out`) with ours
    Compare {
        #[arg(long)]
        theirs: PathBuf,
        #[arg(long, default_value = "tools/parity/out/ours")]
        ours: PathBuf,
        #[arg(default_value = "")]
        prefix: String,
    },
}

#[derive(Debug, Deserialize)]
pub struct Manifest {
    pub harness: Vec<Harness>,
}

#[derive(Debug, Deserialize, Clone)]
struct Hit {
    t: u32,
    amount: f32,
    #[serde(default)]
    armor: Option<i32>,
    #[serde(default)]
    health: Option<i32>,
    #[serde(rename = "type", default)]
    dmg_type: Option<u32>,
}

#[derive(Debug, Deserialize)]
pub struct Harness {
    pub name: String,
    pub scenario: String,
    pub compare: String,
    #[serde(default)]
    pub tolerance: toml::Table,
    #[serde(default)]
    pub crates: Vec<String>,
    #[serde(default)]
    pub realm: Option<String>,
}

#[derive(Debug, Deserialize)]
struct Scenario {
    name: String,
    kind: String,
    #[serde(default)]
    globs: Vec<String>,
    #[serde(default)]
    search: Option<String>,
    #[serde(default)]
    map: Option<String>,
    #[serde(default)]
    start: Option<[f32; 3]>,
    #[serde(default)]
    angles: Option<Vec<f32>>,
    #[serde(default)]
    segments: Vec<Segment>,
    #[serde(default)]
    model: Option<String>,
    /// player scenarios: ticks at which the bot is killed
    #[serde(default)]
    kill_at: Vec<u32>,
    #[serde(default)]
    select: Vec<(u32, String)>,
    weapon: Option<String>,
    dist: Option<f32>,
    target_health: Option<i32>,
    target_armor: Option<i32>,
    pitch: Option<f32>,
    #[serde(default)]
    yaw: Option<f32>,
    /// damage scenarios: drop heights every `spacing` ticks, hits, convar and start values
    #[serde(default)]
    heights: Vec<f32>,
    #[serde(default)]
    spacing: Option<u32>,
    #[serde(default)]
    falldamage: Option<i32>,
    #[serde(default)]
    armor: Option<i32>,
    #[serde(default)]
    health: Option<i32>,
    #[serde(default)]
    hits: Vec<Hit>,
    #[serde(default)]
    velocity: Option<[f32; 3]>,
    #[serde(default)]
    angular: Option<[f32; 3]>,
    #[serde(default)]
    ticks: Option<u32>,
    #[serde(default)]
    prop: Option<[f32; 3]>,
    #[serde(default)]
    prop_angles: Option<[f32; 3]>,
    #[serde(default)]
    props: Vec<PropSpec>,
    #[serde(default)]
    constraints: Vec<ConstraintSpec>,
    #[serde(default)]
    ops: Vec<OpSpec>,
    #[serde(default)]
    snapshot_every: Option<u32>,
    /// An addon directory (relative to the repo root) whose scripted stools handle unknown tools.
    #[serde(default)]
    addon: Option<String>,
}

#[derive(Debug, Deserialize, Default)]
struct PropSpec {
    model: String,
    pos: [f32; 3],
    #[serde(default)]
    angles: Option<[f32; 3]>,
    #[serde(default)]
    frozen: bool,
}

#[derive(Debug, Deserialize, Default)]
struct OpSpec {
    op: String,
    #[serde(default)]
    tool: Option<String>,
    #[serde(default)]
    button: Option<String>,
    #[serde(default)]
    model: Option<String>,
    #[serde(default)]
    pitch: f32,
    #[serde(default)]
    yaw: f32,
    #[serde(default)]
    name: Option<String>,
    #[serde(default)]
    value: Option<String>,
    #[serde(default)]
    wait: Option<u32>,
    #[serde(default)]
    ticks: Option<u32>,
}

#[derive(Debug, Deserialize, Default)]
struct ConstraintSpec {
    kind: String,
    a: usize,
    b: usize,
    #[serde(default)]
    local_a: Option<[f32; 3]>,
    #[serde(default)]
    local_b: Option<[f32; 3]>,
    #[serde(default)]
    length: f32,
    #[serde(default)]
    rigid: bool,
}

#[derive(Debug, Deserialize, Default)]
struct Segment {
    ticks: u32,
    #[serde(default)]
    buttons: u32,
    #[serde(default)]
    forward: f32,
    #[serde(default)]
    side: f32,
    #[serde(default)]
    up: f32,
    #[serde(default)]
    pitch: f32,
    #[serde(default)]
    yaw: f32,
    #[serde(default)]
    mouse_x: f32,
    #[serde(default)]
    mouse_y: f32,
}

/// Same shape the probe writes.
#[derive(Debug, Serialize, Deserialize)]
struct Trace {
    scenario: String,
    kind: String,
    ok: bool,
    #[serde(default)]
    result: Value,
    #[serde(default)]
    error: Option<String>,
}

pub fn load_manifest(path: &Path) -> Result<Manifest> {
    let text =
        std::fs::read_to_string(path).with_context(|| format!("reading {}", path.display()))?;
    toml::from_str(&text).context("parsing harnesses.toml")
}

fn scenario_path(manifest: &Path, h: &Harness) -> PathBuf {
    manifest
        .parent()
        .unwrap_or(Path::new("."))
        .join("scenarios")
        .join(format!("{}.json", h.scenario))
}

fn load_scenario(path: &Path) -> Result<Scenario> {
    let text =
        std::fs::read_to_string(path).with_context(|| format!("reading {}", path.display()))?;
    serde_json::from_str(&text).with_context(|| format!("parsing {}", path.display()))
}

fn run_ours(scn: &Scenario) -> Result<Trace> {
    let result = match scn.kind.as_str() {
        "file_find" => {
            let inst = gm_content::find_install()
                .context("no Garry's Mod install found (set GMOD_DIR)")?;
            let mount = gm_content::Mount::for_install(&inst)?;
            let search = scn.search.as_deref().unwrap_or("GAME");
            if search != "GAME" {
                bail!("file_find search path {search} not supported on our side");
            }
            let mut map = serde_json::Map::new();
            for glob in &scn.globs {
                let found: Vec<Value> = mount.find(glob).into_iter().map(Value::String).collect();
                map.insert(glob.clone(), Value::Array(found));
            }
            Value::Object(map)
        }
        "move" => run_move(scn)?,
        "phys" => run_phys(scn)?,
        "player" => run_player(scn)?,
        "weapon" => run_weapon(scn)?,
        "fire" => run_fire(scn)?,
        "hit" => run_hit(scn)?,
        "damage" => run_damage(scn)?,
        "ragdoll" => run_ragdoll(scn)?,
        "physgun" => run_physgun(scn)?,
        "constraint" => run_constraint(scn)?,
        "sandbox" => run_sandbox(scn)?,
        "server" => run_server(scn)?,
        "net" => run_net(scn)?,
        "toolgun" => run_toolgun(scn)?,
        other => bail!("scenario kind {other} has no gmod-rust side yet"),
    };
    Ok(Trace {
        scenario: scn.name.clone(),
        kind: scn.kind.clone(),
        ok: true,
        result,
        error: None,
    })
}

fn run_move(scn: &Scenario) -> Result<Value> {
    use glam::Vec3;
    use gm_move::{Mover, PlayerState, UserCmd};
    let inst = gm_content::find_install().context("no Garry's Mod install found (set GMOD_DIR)")?;
    let mount = gm_content::Mount::for_install(&inst)?;
    let map = scn.map.as_deref().unwrap_or("gm_construct");
    let data = mount.read(&format!("maps/{map}.bsp"))?;
    let bsp = gm_formats::bsp::Bsp::parse(&data)?;
    let world = gm_collision::World::from_bsp(&bsp);
    let mover = Mover::new(gm_move::MoveConfig::default(), &world);
    let start = Vec3::from(scn.start.unwrap_or([0.0; 3]));
    let ang = scn.angles.clone().unwrap_or_default();
    let view = [
        ang.first().copied().unwrap_or(0.0),
        ang.get(1).copied().unwrap_or(0.0),
    ];
    let mut st = PlayerState {
        origin: start,
        ..Default::default()
    };
    // settle onto the ground like the probe's pre-roll
    let settle_cmd = UserCmd {
        view_angles: Vec3::new(view[0], view[1], 0.0),
        ..Default::default()
    };
    // the probe resets origin and velocity at the start of each pre-roll tick, then the tick runs
    for _ in 0..20 {
        st.origin = start;
        st.velocity = Vec3::ZERO;
        mover.tick(&mut st, &settle_cmd);
    }
    let mut ticks = Vec::new();
    let mut t = 0u32;
    for seg in &scn.segments {
        let cmd = UserCmd {
            buttons: seg.buttons,
            view_angles: Vec3::new(seg.pitch, seg.yaw, 0.0),
            forward_move: seg.forward,
            side_move: seg.side,
            up_move: seg.up,
        };
        for _ in 0..seg.ticks {
            mover.tick(&mut st, &cmd);
            ticks.push(serde_json::json!({
                "t": t,
                "pos": [st.origin.x, st.origin.y, st.origin.z],
                "vel": [st.velocity.x, st.velocity.y, st.velocity.z],
                "ground": st.on_ground,
                "ducked": st.ducked,
            }));
            t += 1;
        }
    }
    Ok(serde_json::json!({
        "start": [start.x, start.y, start.z],
        "ticks": ticks,
        "final": [st.origin.x, st.origin.y, st.origin.z],
    }))
}

/// Ray against a convex (triangles with outward normals derived from the centroid): entry distance.
fn ray_convex(
    origin: glam::Vec3,
    dir: glam::Vec3,
    points: &[glam::Vec3],
    tris: &[[u16; 3]],
) -> Option<f32> {
    let centroid = points.iter().sum::<glam::Vec3>() / points.len() as f32;
    let (mut t_in, mut t_out) = (0.0f32, f32::MAX);
    for t in tris {
        let (a, b, c) = (
            points[t[0] as usize],
            points[t[1] as usize],
            points[t[2] as usize],
        );
        let mut n = (b - a).cross(c - a);
        if n.length_squared() < 1e-8 {
            continue;
        }
        n = n.normalize();
        if n.dot(a - centroid) < 0.0 {
            n = -n;
        }
        let dist = n.dot(a);
        let denom = n.dot(dir);
        let num = dist - n.dot(origin);
        if denom.abs() < 1e-8 {
            if num < 0.0 {
                return None;
            }
            continue;
        }
        let t = num / denom;
        if denom < 0.0 {
            t_in = t_in.max(t);
        } else {
            t_out = t_out.min(t);
        }
    }
    (t_in <= t_out).then_some(t_in)
}

fn run_physgun(scn: &Scenario) -> Result<Value> {
    use glam::{Quat, Vec3};
    use gm_physics::physgun::{Grab, PhysgunParams};
    use gm_physics::{PropDesc, Surface, World};
    let inst = gm_content::find_install().context("no Garry's Mod install found (set GMOD_DIR)")?;
    let mount = gm_content::Mount::for_install(&inst)?;
    let map = scn.map.as_deref().unwrap_or("gm_construct");
    let data = mount.read(&format!("maps/{map}.bsp"))?;
    let bsp = gm_formats::bsp::Bsp::parse(&data)?;
    let model = scn
        .model
        .as_deref()
        .context("physgun scenario needs a model")?;
    let phy_path = model
        .strip_suffix(".mdl")
        .map(|m| format!("{m}.phy"))
        .unwrap_or_else(|| model.to_string());
    let phy = gm_formats::phy::Phy::parse(&mount.read(&phy_path)?)?;
    let solid = phy.solids.first().context("PHY has no solids")?;
    let props = phy.props.first().cloned().unwrap_or_default();
    let hulls: Vec<Vec<Vec3>> = solid
        .convexes
        .iter()
        .map(|c| c.points.iter().map(|&p| Vec3::from(p)).collect())
        .collect();
    let world_hulls: Vec<Vec<Vec3>> = gm_physics::brush::model_hulls(&bsp, 0, gm_physics::REST_GAP)
        .into_iter()
        .map(|h| h.points)
        .collect();
    let table = gm_physics::surface::SurfaceTable::parse(&[&String::from_utf8_lossy(
        &mount.read("scripts/surfaceproperties.txt")?,
    )]);
    let (world_sp, prop_sp) = (table.get("concrete"), table.get(&props.surfaceprop));
    let mut world = World::new(600.0, gm_core::tick::DEFAULT_TICK_INTERVAL, 1);
    tune_solver(&mut world);
    world.add_static_hulls(
        &world_hulls,
        Surface {
            friction: world_sp.friction,
            elasticity: world_sp.elasticity,
        },
    )?;
    let (disp_pos, disp_idx) = gm_physics::brush::displacement_mesh(&bsp, gm_physics::REST_GAP);
    if !disp_idx.is_empty() {
        world.add_static_mesh(
            &disp_pos,
            &disp_idx,
            Surface {
                friction: world_sp.friction,
                elasticity: world_sp.elasticity,
            },
        )?;
    }
    let pa = scn.prop_angles.unwrap_or([0.0; 3]);
    let body = world.add_prop(&PropDesc {
        hulls: &hulls,
        mass: if props.mass > 0.0 { props.mass } else { 1.0 },
        volume: props.volume,
        position: Vec3::from(scn.prop.context("physgun scenario needs a prop position")?),
        angles: (pa[0], pa[1], pa[2]),
        velocity: Vec3::ZERO,
        angular_velocity: Vec3::ZERO,
        surface: Surface {
            friction: prop_sp.friction,
            elasticity: prop_sp.elasticity,
        },
        linear_damping: props.damping,
        angular_damping: props.rotdamping,
        drag_axis_areas: Vec3::from(solid.drag_axis_areas),
        inertia: Some(Vec3::from(solid.rotation_inertia)),
    })?;
    world.optimize();
    // the probe lets the prop settle for 60 ticks before the first segment
    for _ in 0..60 {
        world.step();
    }
    // the bot itself moves through gm-move so the eye follows the real player movement
    let collision = gm_collision::World::from_bsp(&bsp);
    let mover = gm_move::Mover::new(gm_move::MoveConfig::default(), &collision);
    let start = Vec3::from(scn.start.unwrap_or([0.0; 3]));
    let view0 = scn.angles.clone().unwrap_or_default();
    let mut player = gm_move::PlayerState {
        origin: start,
        ..Default::default()
    };
    let settle_cmd = gm_move::UserCmd {
        view_angles: Vec3::new(
            view0.first().copied().unwrap_or(0.0),
            view0.get(1).copied().unwrap_or(0.0),
            0.0,
        ),
        ..Default::default()
    };
    for _ in 0..60 {
        player.origin = start;
        player.velocity = Vec3::ZERO;
        mover.tick(&mut player, &settle_cmd);
    }
    let params = PhysgunParams::default();
    let mut grab: Option<Grab> = None;
    let mut ticks = Vec::new();
    let mut t = 0u32;
    for sg in &scn.segments {
        for _ in 0..sg.ticks {
            let eye = player.origin + Vec3::new(0.0, 0.0, mover.eye_height(&player));
            let st = world.state(body);
            ticks.push(serde_json::json!({
                "t": t,
                "pos": [st.position.x, st.position.y, st.position.z],
                "ang": [st.angles.0, st.angles.1, st.angles.2],
                "vel": [st.velocity.x, st.velocity.y, st.velocity.z],
                "angvel": local_angvel(&st),
                "held": grab.is_some(),
                "frozen": world.is_frozen(body),
                "eye": [eye.x, eye.y, eye.z],
                "eye_ang": [sg.pitch, sg.yaw, 0.0],
            }));
            let (sp, cp) = (sg.pitch.to_radians().sin(), sg.pitch.to_radians().cos());
            let (sy, cy) = (sg.yaw.to_radians().sin(), sg.yaw.to_radians().cos());
            let forward = Vec3::new(cp * cy, cp * sy, -sp);
            let right = Vec3::new(sy, -cy, 0.0);
            let attack = sg.buttons & 1 != 0;
            let use_key = sg.buttons & 32 != 0;
            let attack2 = sg.buttons & 2048 != 0;
            if attack && grab.is_none() && !world.is_frozen(body) {
                let inv = st.rotation.inverse();
                let (lo, ld) = (inv * (eye - st.position), inv * forward);
                let hit = solid
                    .convexes
                    .iter()
                    .filter_map(|c| {
                        let pts: Vec<Vec3> = c.points.iter().map(|&p| Vec3::from(p)).collect();
                        ray_convex(lo, ld, &pts, &c.triangles)
                    })
                    .fold(None, |m: Option<f32>, d| Some(m.map_or(d, |x| x.min(d))));
                if let Some(d) = hit {
                    grab = Some(Grab::new(
                        eye,
                        forward,
                        eye + forward * d,
                        st.position,
                        st.rotation,
                        sg.yaw,
                    ));
                }
            } else if !attack && grab.is_some() {
                grab = None;
                world.release(body);
            }
            if grab.is_some() && attack2 {
                grab = None;
                world.freeze(body);
            }
            if let Some(g) = &mut grab {
                if use_key {
                    g.rotate(sg.mouse_x, sg.mouse_y, right);
                }
                let (tp, tr): (Vec3, Quat) = g.target(eye, forward, sg.yaw, st.rotation);
                world.hold(body, tp, tr, &params);
            }
            world.step();
            let cmd = gm_move::UserCmd {
                buttons: sg.buttons & !(1 | 32 | 2048),
                view_angles: Vec3::new(sg.pitch, sg.yaw, 0.0),
                forward_move: sg.forward,
                side_move: sg.side,
                ..Default::default()
            };
            mover.tick(&mut player, &cmd);
            t += 1;
        }
    }
    Ok(
        serde_json::json!({ "start": scn.start.unwrap_or([0.0; 3]), "model": model, "ticks": ticks }),
    )
}

fn run_constraint(scn: &Scenario) -> Result<Value> {
    use glam::Vec3;
    use gm_physics::{PropDesc, Surface, World};
    let inst = gm_content::find_install().context("no Garry's Mod install found (set GMOD_DIR)")?;
    let mount = gm_content::Mount::for_install(&inst)?;
    let map = scn.map.as_deref().unwrap_or("gm_construct");
    let data = mount.read(&format!("maps/{map}.bsp"))?;
    let bsp = gm_formats::bsp::Bsp::parse(&data)?;
    let world_hulls: Vec<Vec<Vec3>> = gm_physics::brush::model_hulls(&bsp, 0, gm_physics::REST_GAP)
        .into_iter()
        .map(|h| h.points)
        .collect();
    let table = gm_physics::surface::SurfaceTable::parse(&[&String::from_utf8_lossy(
        &mount.read("scripts/surfaceproperties.txt")?,
    )]);
    let world_sp = table.get("concrete");
    let mut world = World::new(600.0, gm_core::tick::DEFAULT_TICK_INTERVAL, 1);
    tune_solver(&mut world);
    world.add_static_hulls(
        &world_hulls,
        Surface {
            friction: world_sp.friction,
            elasticity: world_sp.elasticity,
        },
    )?;
    let (disp_pos, disp_idx) = gm_physics::brush::displacement_mesh(&bsp, gm_physics::REST_GAP);
    if !disp_idx.is_empty() {
        world.add_static_mesh(
            &disp_pos,
            &disp_idx,
            Surface {
                friction: world_sp.friction,
                elasticity: world_sp.elasticity,
            },
        )?;
    }
    let mut bodies = Vec::new();
    let mut infos = Vec::new();
    for pr in &scn.props {
        let phy_path = pr
            .model
            .strip_suffix(".mdl")
            .map(|m| format!("{m}.phy"))
            .unwrap_or_else(|| pr.model.clone());
        let phy = gm_formats::phy::Phy::parse(&mount.read(&phy_path)?)?;
        let solid = phy.solids.first().context("PHY has no solids")?;
        let props = phy.props.first().cloned().unwrap_or_default();
        let hulls: Vec<Vec<Vec3>> = solid
            .convexes
            .iter()
            .map(|c| c.points.iter().map(|&p| Vec3::from(p)).collect())
            .collect();
        let sp = table.get(&props.surfaceprop);
        let a = pr.angles.unwrap_or([0.0; 3]);
        let body = world.add_prop(&PropDesc {
            hulls: &hulls,
            mass: if props.mass > 0.0 { props.mass } else { 1.0 },
            volume: props.volume,
            position: Vec3::from(pr.pos),
            angles: (a[0], a[1], a[2]),
            velocity: Vec3::ZERO,
            angular_velocity: Vec3::ZERO,
            surface: Surface {
                friction: sp.friction,
                elasticity: sp.elasticity,
            },
            linear_damping: props.damping,
            angular_damping: props.rotdamping,
            drag_axis_areas: Vec3::from(solid.drag_axis_areas),
            inertia: Some(Vec3::from(solid.rotation_inertia)),
        })?;
        if pr.frozen {
            world.freeze(body);
        }
        bodies.push(body);
        infos.push(serde_json::json!({ "mass": props.mass, "material": props.surfaceprop }));
    }
    let mut made = Vec::new();
    for c in &scn.constraints {
        let (a, b) = (bodies[c.a - 1], bodies[c.b - 1]);
        let (sa, sb) = (world.state(a), world.state(b));
        let ok = match c.kind.as_str() {
            "weld" => world.weld(a, b).is_some(),
            "rope" => {
                let pa = sa.position + sa.rotation * Vec3::from(c.local_a.unwrap_or([0.0; 3]));
                let pb = sb.position + sb.rotation * Vec3::from(c.local_b.unwrap_or([0.0; 3]));
                world.rope(a, b, pa, pb, c.length, c.rigid).is_some()
            }
            _ => false,
        };
        made.push(serde_json::json!({ "kind": c.kind, "ok": ok }));
    }
    world.optimize();
    let total = scn.ticks.unwrap_or(200);
    let mut ticks = Vec::with_capacity(total as usize);
    for t in 0..total {
        let props: Vec<Value> = bodies
            .iter()
            .map(|&b| {
                let st = world.state(b);
                serde_json::json!({
                    "pos": [st.position.x, st.position.y, st.position.z],
                    "ang": [st.angles.0, st.angles.1, st.angles.2],
                    "vel": [st.velocity.x, st.velocity.y, st.velocity.z],
                    "angvel": local_angvel(&st),
                    "asleep": !st.active,
                })
            })
            .collect();
        ticks.push(serde_json::json!({ "t": t, "props": props }));
        world.step();
    }
    Ok(serde_json::json!({ "constraints": made, "info": infos, "ticks": ticks }))
}

/// `GMP_SOLVER=baumgarte,velocity_steps,position_steps,min_restitution_velocity` for experiments.
fn tune_solver(world: &mut gm_physics::World) {
    if std::env::var("GMP_IMPACT").as_deref() == Ok("0") {
        world.set_impact_enabled(false);
    }
    if let Ok(v) = std::env::var("GMP_SOLVER") {
        let f: Vec<f32> = v.split(',').filter_map(|x| x.trim().parse().ok()).collect();
        if f.len() == 4 {
            world.set_solver(f[0], f[1] as u32, f[2] as u32, f[3]);
        }
    }
}

/// The game reports angular velocity in the body's own axes (`GetAngleVelocity`), degrees per second.
fn local_angvel(st: &gm_physics::BodyState) -> [f32; 3] {
    let w = st.rotation.inverse() * st.angular_velocity;
    [w.x, w.y, w.z]
}

fn phys_setup(scn: &Scenario) -> Result<PhysSetup> {
    use glam::Vec3;
    use gm_physics::{PropDesc, Surface, World};
    let inst = gm_content::find_install().context("no Garry's Mod install found (set GMOD_DIR)")?;
    let mount = gm_content::Mount::for_install(&inst)?;
    let map = scn.map.as_deref().unwrap_or("gm_construct");
    let data = mount.read(&format!("maps/{map}.bsp"))?;
    let bsp = gm_formats::bsp::Bsp::parse(&data)?;
    let model = scn
        .model
        .as_deref()
        .context("phys scenario needs a model")?;
    let phy_path = model
        .strip_suffix(".mdl")
        .map(|m| format!("{m}.phy"))
        .unwrap_or_else(|| model.to_string());
    let phy = gm_formats::phy::Phy::parse(&mount.read(&phy_path)?)?;
    let solid = phy.solids.first().context("PHY has no solids")?;
    let props = phy.props.first().cloned().unwrap_or_default();
    let hulls: Vec<Vec<Vec3>> = solid
        .convexes
        .iter()
        .map(|c| c.points.iter().map(|&p| Vec3::from(p)).collect())
        .collect();
    let world_hulls: Vec<Vec<Vec3>> = gm_physics::brush::model_hulls(&bsp, 0, gm_physics::REST_GAP)
        .into_iter()
        .map(|h| h.points)
        .collect();
    let table = gm_physics::surface::SurfaceTable::parse(&[&String::from_utf8_lossy(
        &mount.read("scripts/surfaceproperties.txt")?,
    )]);
    let world_sp = table.get("concrete");
    let prop_sp = table.get(&props.surfaceprop);
    let mut world = World::new(600.0, gm_core::tick::DEFAULT_TICK_INTERVAL, 1);
    tune_solver(&mut world);
    world.add_static_hulls(
        &world_hulls,
        Surface {
            friction: world_sp.friction,
            elasticity: world_sp.elasticity,
        },
    )?;
    let (disp_pos, disp_idx) = gm_physics::brush::displacement_mesh(&bsp, gm_physics::REST_GAP);
    if !disp_idx.is_empty() {
        world.add_static_mesh(
            &disp_pos,
            &disp_idx,
            Surface {
                friction: world_sp.friction,
                elasticity: world_sp.elasticity,
            },
        )?;
    }
    let ang = scn.angles.clone().unwrap_or_default();
    let angles = (
        ang.first().copied().unwrap_or(0.0),
        ang.get(1).copied().unwrap_or(0.0),
        ang.get(2).copied().unwrap_or(0.0),
    );
    let body = world.add_prop(&PropDesc {
        hulls: &hulls,
        mass: if props.mass > 0.0 { props.mass } else { 1.0 },
        volume: props.volume,
        position: Vec3::from(scn.start.unwrap_or([0.0; 3])),
        angles,
        velocity: Vec3::from(scn.velocity.unwrap_or([0.0; 3])),
        angular_velocity: Vec3::from(scn.angular.unwrap_or([0.0; 3])),
        surface: Surface {
            friction: prop_sp.friction,
            elasticity: prop_sp.elasticity,
        },
        linear_damping: props.damping,
        angular_damping: props.rotdamping,
        drag_axis_areas: Vec3::from(solid.drag_axis_areas),
        inertia: Some(Vec3::from(solid.rotation_inertia)),
    })?;
    world.optimize();
    Ok(PhysSetup {
        world,
        body,
        mass: props.mass,
        material: props.surfaceprop.clone(),
        volume: props.volume,
        convexes: solid.convexes.len(),
        model: model.to_string(),
    })
}

struct PhysSetup {
    world: gm_physics::World,
    body: gm_physics::BodyHandle,
    mass: f32,
    material: String,
    volume: f32,
    convexes: usize,
    model: String,
}

fn run_phys(scn: &Scenario) -> Result<Value> {
    let PhysSetup {
        mut world,
        body,
        mass,
        material,
        volume,
        convexes,
        model,
    } = phys_setup(scn)?;
    let total = scn.ticks.unwrap_or(200);
    let mut ticks = Vec::with_capacity(total as usize);
    for t in 0..total {
        let st = world.state(body);
        ticks.push(serde_json::json!({
            "t": t,
            "pos": [st.position.x, st.position.y, st.position.z],
            "ang": [st.angles.0, st.angles.1, st.angles.2],
            "vel": [st.velocity.x, st.velocity.y, st.velocity.z],
            "angvel": local_angvel(&st),
            "asleep": !st.active,
        }));
        world.step();
    }
    Ok(serde_json::json!({
        "start": scn.start.unwrap_or([0.0; 3]),
        "model": model,
        "info": { "mass": mass, "material": material, "volume": volume, "mesh_convexes": convexes },
        "ticks": ticks,
    }))
}

/// Our contact model from the game's own state: the prop is put at the golden pose and
/// velocities of tick `from` and stepped, so a differing impact response shows without the
/// drift of the ticks before it.
fn replay_phys(scenario: &std::path::Path, from: usize, n: usize) -> Result<()> {
    use glam::Vec3;
    let scn: Scenario = serde_json::from_str(&std::fs::read_to_string(scenario)?)?;
    let golden = golden_trace(&scn.name).context("no golden trace for this scenario")?;
    let rows = golden["result"]["ticks"]
        .as_array()
        .context("golden has no ticks")?;
    let PhysSetup {
        mut world, body, ..
    } = phys_setup(&scn)?;
    let v3 = |v: &Value| -> Vec3 {
        let a: Vec<f32> = v
            .as_array()
            .unwrap()
            .iter()
            .map(|x| x.as_f64().unwrap() as f32)
            .collect();
        Vec3::new(a[0], a[1], a[2])
    };
    let g0 = &rows[from];
    let ang = v3(&g0["ang"]);
    let q = gm_physics::quat_from_angles(ang.x, ang.y, ang.z);
    world.teleport(body, v3(&g0["pos"]), q);
    world.set_velocity(body, v3(&g0["vel"]), q * v3(&g0["angvel"]));
    for k in 0..=n {
        let st = world.state(body);
        let w = local_angvel(&st);
        println!(
            "t{} ours pos ({:.2} {:.2} {:.2}) ang ({:.2} {:.2} {:.2}) v ({:.1} {:.1} {:.1}) w ({:.1} {:.1} {:.1})",
            from + k, st.position.x, st.position.y, st.position.z, st.angles.0, st.angles.1, st.angles.2,
            st.velocity.x, st.velocity.y, st.velocity.z, w[0], w[1], w[2]
        );
        if let Some(g) = rows.get(from + k) {
            let (p, a, v, w) = (
                v3(&g["pos"]),
                v3(&g["ang"]),
                v3(&g["vel"]),
                v3(&g["angvel"]),
            );
            println!(
                "t{} game pos ({:.2} {:.2} {:.2}) ang ({:.2} {:.2} {:.2}) v ({:.1} {:.1} {:.1}) w ({:.1} {:.1} {:.1})",
                from + k, p.x, p.y, p.z, a.x, a.y, a.z, v.x, v.y, v.z, w.x, w.y, w.z
            );
        }
        if k < n {
            world.step();
        }
    }
    Ok(())
}

/// Rigid body traces: position, orientation and the tick of first contact. Bit exactness is
/// not expected from a different solver, so the check is "same contact tick within `contact`
/// ticks, position within `pos` for every tick, final pose within `rest`".
/// Collision hulls of a model as outward planes per convex (entity frame), for the spawn and
/// tool traces.
fn model_hull_planes(
    mount: &gm_content::Mount,
    model: &str,
) -> Result<Vec<Vec<(glam::Vec3, f32)>>> {
    use glam::Vec3;
    let phy_path = model
        .strip_suffix(".mdl")
        .map(|m| format!("{m}.phy"))
        .unwrap_or_else(|| model.to_string());
    let phy = gm_formats::phy::Phy::parse(&mount.read(&phy_path)?)?;
    let solid = phy.solids.first().context("PHY has no solids")?;
    Ok(solid
        .convexes
        .iter()
        .map(|c| {
            let pts: Vec<Vec3> = c.points.iter().map(|&p| Vec3::from(p)).collect();
            gm_sandbox::hull_planes(&pts, &c.triangles)
        })
        .collect())
}

/// The recorded trace of this scenario, if any (`tools/parity/golden/*/<name>.json`): the
/// sandbox runner takes the poses of props under physics from it as inputs.
/// The repository root (the runner lives in tools/parity/runner).
fn repo_root() -> std::path::PathBuf {
    std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../..")
        .canonicalize()
        .unwrap_or_else(|_| std::path::PathBuf::from("."))
}

fn golden_trace(name: &str) -> Option<Value> {
    let mut found = None;
    for dir in std::fs::read_dir("tools/parity/golden").ok()?.flatten() {
        let path = dir.path().join(format!("{name}.json"));
        if let Ok(text) = std::fs::read_to_string(&path) {
            if let Ok(v) = serde_json::from_str::<Value>(&text) {
                found = Some(v);
            }
        }
    }
    found
}

fn run_sandbox(scn: &Scenario) -> Result<Value> {
    use glam::Vec3;
    use gm_sandbox::{Event, Limits, ModelBounds, PropBody, Sandbox, SpawnRequest};
    let inst = gm_content::find_install().context("no Garry's Mod install found (set GMOD_DIR)")?;
    let mount = gm_content::Mount::for_install(&inst)?;
    let map = scn.map.as_deref().unwrap_or("gm_construct");
    let data = mount.read(&format!("maps/{map}.bsp"))?;
    let bsp = gm_formats::bsp::Bsp::parse(&data)?;
    let world = gm_collision::World::from_bsp(&bsp);
    let start = Vec3::from(scn.start.unwrap_or([0.0; 3]));
    let eye = start + Vec3::new(0.0, 0.0, 64.0);
    let player = 1u32;
    let mut sb = Sandbox::new(Limits::default());
    let v3 = |v: &Value| -> Vec3 {
        let a = v.as_array().cloned().unwrap_or_default();
        Vec3::new(
            a.first().and_then(Value::as_f64).unwrap_or(0.0) as f32,
            a.get(1).and_then(Value::as_f64).unwrap_or(0.0) as f32,
            a.get(2).and_then(Value::as_f64).unwrap_or(0.0) as f32,
        )
    };
    // props already on the table (spawn targets); their poses under physics come from the recording
    let mut hull_cache: std::collections::HashMap<String, Vec<Vec<(Vec3, f32)>>> =
        Default::default();
    let mut placed = Vec::new();
    for (i, pr) in scn.props.iter().enumerate() {
        let hulls = model_hull_planes(&mount, &pr.model)?;
        hull_cache.insert(pr.model.clone(), hulls.clone());
        placed.push(PropBody {
            id: i as u32 + 1,
            pos: Vec3::from(pr.pos),
            angles: Vec3::from(pr.angles.unwrap_or([0.0; 3])),
            hulls,
        });
    }
    let n_placed = placed.len() as u32;
    sb.place_existing(
        0,
        &placed,
        &scn.props
            .iter()
            .map(|p| p.model.clone())
            .collect::<Vec<_>>(),
    );
    let golden = if scn.props.is_empty() {
        None
    } else {
        golden_trace(&scn.name)
    };
    let mut results = Vec::new();
    let mut messages = Vec::new();
    let snapshot = |sb: &Sandbox| -> Value {
        json!({
            "props": sb.props().iter().filter(|p| p.owner == player).map(|p| json!({
                "model": p.model, "pos": [p.pos.x, p.pos.y, p.pos.z], "ang": [p.angles.x, p.angles.y, p.angles.z],
            })).collect::<Vec<_>>(),
            "count": sb.count(player),
            "undos": sb.undos(player).iter().map(|u| json!({ "id": u.id, "name": u.name, "text": u.text, "n": u.entities.len() })).collect::<Vec<_>>(),
            "maxprops": sb.limits.max_props,
        })
    };
    for (i, op) in scn.ops.iter().enumerate() {
        // where the game had everything right before this op
        if let Some(before) = golden
            .as_ref()
            .map(|g| &g["result"]["results"][i]["before"])
        {
            for (j, p) in before["placed"]
                .as_array()
                .unwrap_or(&Vec::new())
                .iter()
                .enumerate()
            {
                if let Some(body) = placed.get_mut(j) {
                    body.pos = v3(&p["pos"]);
                    body.angles = v3(&p["ang"]);
                    sb.set_prop_pose(body.id, body.pos, body.angles);
                }
            }
            let mine: Vec<u32> = sb
                .props()
                .iter()
                .filter(|p| p.owner == player)
                .map(|p| p.id)
                .collect();
            for (id, p) in mine
                .iter()
                .zip(before["props"].as_array().unwrap_or(&Vec::new()))
            {
                sb.set_prop_pose(*id, v3(&p["pos"]), v3(&p["ang"]));
            }
        }
        let mut bodies = placed.clone();
        for p in sb.props().iter().filter(|p| p.owner == player) {
            if !hull_cache.contains_key(&p.model) {
                hull_cache.insert(p.model.clone(), model_hull_planes(&mount, &p.model)?);
            }
            bodies.push(PropBody {
                id: p.id,
                pos: p.pos,
                angles: p.angles,
                hulls: hull_cache[&p.model].clone(),
            });
        }
        let mut events = Vec::new();
        let mut rec = json!({ "op": op.op, "t": i });
        match op.op.as_str() {
            "spawn" => {
                let model = op.model.clone().unwrap_or_default();
                let mdl = gm_formats::mdl::Mdl::parse(&mount.read(&model)?)?;
                let bounds = ModelBounds {
                    min: Vec3::from(mdl.header.hull_min),
                    max: Vec3::from(mdl.header.hull_max),
                };
                let eye_angles = Vec3::new(op.pitch, op.yaw, 0.0);
                let pl = gm_sandbox::place_prop(&world, &bodies, eye, eye_angles, &bounds);
                let placed_index = pl.hit_ent.filter(|&e| e <= n_placed).unwrap_or(0);
                rec["trace"] = json!({ "pos": [pl.hit.x, pl.hit.y, pl.hit.z], "normal": [pl.normal.x, pl.normal.y, pl.normal.z], "eye": [eye.x, eye.y, eye.z],
                    "ent": if pl.hit_ent.is_some() { "prop_physics" } else { "world" }, "placed": placed_index });
                rec["model"] = json!(model);
                sb.spawn_prop(
                    &world,
                    &bodies,
                    player,
                    &SpawnRequest {
                        eye,
                        eye_angles,
                        model: &model,
                        bounds,
                    },
                    &mut events,
                );
            }
            "undo" => sb.undo(player, &mut events),
            "cleanup" => sb.cleanup(player, &mut events),
            "cvar" => {
                if op.name.as_deref() == Some("sbox_maxprops") {
                    sb.limits.max_props =
                        op.value.as_deref().unwrap_or("200").parse().unwrap_or(200);
                }
            }
            other => bail!("unknown sandbox op {other}"),
        }
        for e in &events {
            match e {
                Event::LimitHit { limit } => messages.push(json!({ "t": i, "limit": limit })),
                Event::Spawned { .. } => {
                    messages.push(json!({ "t": i, "lua": "achievements.SpawnedProp()" }))
                }
                Event::Cleanup { kind } => messages
                    .push(json!({ "t": i, "lua": format!("hook.Run('OnCleanup','{kind}')") })),
                Event::Undone { .. } => {}
            }
        }
        let snap = snapshot(&sb);
        rec["immediate"] = snap.clone();
        rec["after"] = snap;
        rec["t_after"] = json!(i);
        results.push(rec);
    }
    Ok(json!({ "results": results, "messages": messages, "eye": [eye.x, eye.y, eye.z] }))
}

/// Loads what `gm-server` needs to spawn and trace a model: MDL hull box, PHY convexes and
/// physics properties.
fn model_info(mount: &gm_content::Mount, model: &str) -> Result<gm_server::ModelInfo> {
    use glam::Vec3;
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
    Ok(gm_server::ModelInfo {
        bounds: gm_sandbox::ModelBounds {
            min: Vec3::from(mdl.header.hull_min),
            max: Vec3::from(mdl.header.hull_max),
        },
        hulls: solid
            .convexes
            .iter()
            .map(|c| c.points.iter().map(|&p| Vec3::from(p)).collect())
            .collect(),
        hull_planes: model_hull_planes(mount, model)?,
        mass: if props.mass > 0.0 { props.mass } else { 1.0 },
        volume: props.volume,
        surface: gm_physics::Surface {
            friction: sp.friction,
            elasticity: sp.elasticity,
        },
        linear_damping: props.damping,
        angular_damping: props.rotdamping,
        drag_axis_areas: Vec3::from(solid.drag_axis_areas),
        inertia: Some(Vec3::from(solid.rotation_inertia)),
    })
}

/// A bot's life cycle through the server loop: spawned, killed at the scenario's ticks,
/// respawned by the server's rule; recorded like the probe's `player` scenario.
/// Two players through the server loop: the shooter fires at the target `dist` ahead; the
/// target's health and armour per tick and every hit (tick, damage, hit group, position).
fn run_hit(scn: &Scenario) -> Result<Value> {
    use glam::Vec3;
    use gm_physics::{Surface, World};
    let inst = gm_content::find_install().context("no Garry's Mod install found (set GMOD_DIR)")?;
    let mount = gm_content::Mount::for_install(&inst)?;
    let map = scn.map.as_deref().unwrap_or("gm_construct");
    let data = mount.read(&format!("maps/{map}.bsp"))?;
    let bsp = gm_formats::bsp::Bsp::parse(&data)?;
    let collision = gm_collision::World::from_bsp(&bsp);
    let world_hulls: Vec<Vec<Vec3>> = gm_physics::brush::model_hulls(&bsp, 0, gm_physics::REST_GAP)
        .into_iter()
        .map(|h| h.points)
        .collect();
    let mut physics = World::new(600.0, gm_core::tick::DEFAULT_TICK_INTERVAL, 1);
    let world_body = physics.add_static_hulls(
        &world_hulls,
        Surface {
            friction: 0.8,
            elasticity: 0.25,
        },
    )?;
    let mut server = gm_server::Server::new(&collision, physics, world_body);
    server.set_spawn_points_from_bsp(&bsp);
    let mdl_data = mount.read(gm_server::PLAYER_MODEL)?;
    let mdl = gm_formats::mdl::Mdl::parse(&mdl_data)?;
    server.hit_model = Some(gm_server::weapons::HitModel::from_mdl(&mdl_data, &mdl));
    let class = scn.weapon.clone().context("hit scenario needs `weapon`")?;
    let start = Vec3::from(scn.start.unwrap_or([0.0; 3]));
    let tpos = start + Vec3::new(0.0, scn.dist.unwrap_or(200.0), 0.0);
    let aim = Vec3::new(scn.pitch.unwrap_or(0.0), scn.yaw.unwrap_or(90.0), 0.0);
    let back = Vec3::new(0.0, -90.0, 0.0);
    let (shooter, target) = (1u32, 2u32);
    server.add_player(shooter, start, aim);
    server.add_player(target, tpos, back);
    for id in [shooter, target] {
        server.spawn_player(id);
    }
    server.teleport_player(shooter, start);
    server.teleport_player(target, tpos);
    if let Some(h) = scn.target_health {
        server.set_player_health(target, h);
    }
    if let Some(a) = scn.target_armor {
        server.set_player_armor(target, a);
    }
    let mut cmds = std::collections::BTreeMap::new();
    let idle = |ang: Vec3| gm_server::PlayerCmd {
        cmd: gm_move::UserCmd {
            view_angles: ang,
            ..Default::default()
        },
        ..Default::default()
    };
    // the probe spawns at tick -10 and selects the weapon at tick -8
    for pre in -10i32..0 {
        if pre == -8 {
            server.select_weapon(shooter, &class);
        }
        cmds.insert(shooter, idle(aim));
        cmds.insert(target, idle(back));
        server.tick(&cmds);
    }
    let mut ticks = Vec::new();
    let mut hits = Vec::new();
    let mut t = 0u32;
    for seg in &scn.segments {
        for _ in 0..seg.ticks {
            let mut c = idle(aim);
            c.cmd.buttons = seg.buttons;
            cmds.insert(shooter, c);
            cmds.insert(target, idle(back));
            let before = server.hits.len();
            server.tick(&cmds);
            for h in &server.hits[before..] {
                hits.push(json!({ "t": t, "damage": h.damage, "hitgroup": h.hitgroup, "pos": [h.pos.x, h.pos.y, h.pos.z], "ammo": h.ammo }));
            }
            let p = &server.players[&target];
            let sh = &server.players[&shooter];
            ticks.push(json!({ "t": t, "health": p.health, "armor": p.armor, "alive": p.alive, "shots": sh.inventory.shots }));
            t += 1;
        }
    }
    Ok(
        json!({ "weapon": class, "ticks": ticks, "hits": hits, "shots": server.players[&shooter].inventory.shots }),
    )
}

/// Target health, armour and life every tick; every hit on the same tick with the same damage
/// and hit group, position within `pos` (the game hits the model's hit boxes, we hit the hull).
/// With `random = true` (shotgun pellets) only the hit ticks are compared.
fn compare_hit(theirs: &Value, ours: &Value, tol: &toml::Table) -> bool {
    // the game hits the model's hit boxes, a few units inside the hull we hit
    let pos_tol = tol.get("pos").and_then(|v| v.as_float()).unwrap_or(12.0) as f32;
    let random = tol.get("random").and_then(|v| v.as_bool()).unwrap_or(false);
    let empty = Vec::new();
    let tr = theirs["ticks"].as_array().unwrap_or(&empty);
    let or = ours["ticks"].as_array().unwrap_or(&empty);
    let th = theirs["hits"].as_array().unwrap_or(&empty);
    let ts = theirs["scales"].as_array().unwrap_or(&empty);
    let oh = ours["hits"].as_array().unwrap_or(&empty);
    let mut ok = tr.len() == or.len();
    let mut diffs = 0;
    // a spread weapon's later shots miss or land at random: when the game's hit count is below
    // its shot count, rows and hits are compared through the first hit only
    let shots = theirs["shots"].as_u64().unwrap_or(0) as usize;
    let first_only = !random && th.len() < shots;
    let first_hit_t = th.first().and_then(|h| h["t"].as_u64()).unwrap_or(u64::MAX);
    if !random {
        for (a, b) in tr.iter().zip(or) {
            if first_only && a["t"].as_u64().unwrap_or(0) > first_hit_t {
                break;
            }
            if a["health"] != b["health"]
                || a["armor"] != b["armor"]
                || a["alive"] != b["alive"]
                || a["shots"] != b["shots"]
            {
                diffs += 1;
                if diffs <= 4 {
                    println!(
                        "  tick {}: game hp {} ar {} shots {} | ours hp {} ar {} shots {}",
                        a["t"],
                        a["health"],
                        a["armor"],
                        a["shots"],
                        b["health"],
                        b["armor"],
                        b["shots"]
                    );
                }
                ok = false;
            }
        }
    }
    if th.len() != oh.len() && !first_only {
        println!("  hits: game {} ours {}", th.len(), oh.len());
        ok = false;
    }
    if first_only && th.is_empty() != oh.is_empty() {
        println!("  hits: game {} ours {}", th.len(), oh.len());
        ok = false;
    }
    let v3 = |v: &Value| -> Option<[f32; 3]> {
        let a = v.as_array()?;
        Some([
            a.first()?.as_f64()? as f32,
            a.get(1)?.as_f64()? as f32,
            a.get(2)?.as_f64()? as f32,
        ])
    };
    let mut max_pos = 0.0f32;
    for (i, (a, b)) in th.iter().zip(oh).enumerate() {
        if first_only && i > 0 {
            break;
        }
        let group = ts
            .get(i)
            .map(|s| s["hitgroup"].clone())
            .unwrap_or(Value::Null);
        let mut bad = a["t"] != b["t"];
        if !random {
            bad |= a["damage"].as_f64() != b["damage"].as_f64();
            bad |= group != b["hitgroup"];
            if let (Some(pa), Some(pb)) = (v3(&a["pos"]), v3(&b["pos"])) {
                let d =
                    ((pa[0] - pb[0]).powi(2) + (pa[1] - pb[1]).powi(2) + (pa[2] - pb[2]).powi(2))
                        .sqrt();
                max_pos = max_pos.max(d);
                bad |= d > pos_tol;
            }
        }
        if bad {
            println!(
                "  hit {i}: game t {} dmg {} group {} pos {} | ours t {} dmg {} group {} pos {}",
                a["t"], a["damage"], group, a["pos"], b["t"], b["damage"], b["hitgroup"], b["pos"]
            );
            ok = false;
        }
    }
    println!(
        "  {} ticks, {} hits (ours {}), {} rows differing, max hit position err {max_pos:.2} u (tol {pos_tol}){}: {}",
        tr.len(), th.len(), oh.len(), diffs, if random { ", pellets random: ticks only" } else if first_only { ", spread misses: first hit only" } else { "" }, if ok { "equal" } else { "DIFF" }
    );
    ok
}

/// Firing one weapon with a bot's buttons per segment: `Inventory::command` per tick, then the
/// timers; the clip, reserve ammo, timers and shots are the recording's rows.
fn run_fire(scn: &Scenario) -> Result<Value> {
    use gm_server::weapons::Inventory;
    let class = scn.weapon.clone().context("fire scenario needs `weapon`")?;
    let mut inv = Inventory::default();
    inv.give_loadout();
    inv.select(&class);
    // the probe selects the weapon 8 ticks before the first recorded tick: 7 full ticks pass
    // before the first recorded command
    for _ in 0..7 {
        inv.tick(&class);
    }
    let def = gm_server::weapons::def(&class).context("unknown weapon")?;
    let mut ticks = Vec::new();
    let mut t = 0u32;
    for seg in &scn.segments {
        for _ in 0..seg.ticks {
            let shot = inv.command(&class, seg.buttons);
            inv.tick(&class);
            let clip = inv.clip(&class);
            let mut row = json!({
                "t": t, "active": class, "clip1": clip[0], "clip2": clip[1],
                "ammo1": if def.ammo1.is_empty() { -1 } else { inv.ammo.get(def.ammo1).copied().unwrap_or(0) },
                "next_primary": inv.next_primary, "next_secondary": inv.next_secondary, "shots": inv.shots,
            });
            if let Some(s) = shot {
                row["shot"] = json!({ "num": s.pellets, "spread": [s.spread, s.spread, 0.0], "ammo": s.ammo });
            }
            ticks.push(row);
            t += 1;
        }
    }
    Ok(json!({ "weapon": class, "ticks": ticks, "shots": inv.shots }))
}

/// Clip, reserve ammo and shot count every tick, the primary timer within `time`, pellets and
/// spread of every shot within `spread`.
fn compare_fire(theirs: &Value, ours: &Value, tol: &toml::Table) -> bool {
    let time_tol = tol.get("time").and_then(|v| v.as_float()).unwrap_or(0.001) as f32;
    let spread_tol = tol
        .get("spread")
        .and_then(|v| v.as_float())
        .unwrap_or(0.0003) as f32;
    let empty = Vec::new();
    let tr = theirs["ticks"].as_array().unwrap_or(&empty);
    let or = ours["ticks"].as_array().unwrap_or(&empty);
    let mut ok = tr.len() == or.len();
    let (mut diffs, mut max_dt, mut max_ds) = (0, 0.0f32, 0.0f32);
    for (a, b) in tr.iter().zip(or) {
        let mut bad =
            a["clip1"] != b["clip1"] || a["ammo1"] != b["ammo1"] || a["shots"] != b["shots"];
        if let (Some(x), Some(y)) = (a["next_primary"].as_f64(), b["next_primary"].as_f64()) {
            let d = (x - y).abs() as f32;
            max_dt = max_dt.max(d);
            bad |= d > time_tol;
        }
        if a["shot"].is_object() || b["shot"].is_object() {
            let (sa, sb) = (&a["shot"], &b["shot"]);
            bad |= sa["num"] != sb["num"];
            let (x, y) = (
                sa["spread"][0].as_f64().unwrap_or(-1.0),
                sb["spread"][0].as_f64().unwrap_or(-1.0),
            );
            let d = (x - y).abs() as f32;
            max_ds = max_ds.max(d);
            bad |= d > spread_tol;
        }
        if bad {
            diffs += 1;
            if diffs <= 6 {
                println!("  tick {}: game clip {} ammo {} shots {} next {} shot {} | ours clip {} ammo {} shots {} next {} shot {}", a["t"], a["clip1"], a["ammo1"], a["shots"], a["next_primary"], a["shot"], b["clip1"], b["ammo1"], b["shots"], b["next_primary"], b["shot"]);
            }
            ok = false;
        }
    }
    println!(
        "  {} ticks, {} shots (ours {}), {} differing, max timer err {max_dt:.4} s (tol {time_tol}), max spread err {max_ds:.5} (tol {spread_tol}): {}",
        tr.len(), theirs["shots"], ours["shots"], diffs, if ok { "equal" } else { "DIFF" }
    );
    ok
}

/// The sandbox loadout and weapon switching: `Inventory` from gm-server driven by the
/// scenario's `select` list, the active weapon, its clips and fire timers recorded every tick.
fn run_weapon(scn: &Scenario) -> Result<Value> {
    use gm_server::weapons::{Inventory, WEAPONS};
    let mut inv = Inventory::default();
    inv.give_loadout();
    let mut active = gm_server::SPAWN_WEAPON.to_string();
    let carried: Vec<&str> = gm_server::LOADOUT.to_vec();
    let total = scn.ticks.unwrap_or(200);
    let mut ticks = Vec::with_capacity(total as usize);
    let mut selected_once = false;
    for t in 0..total {
        if t > 0 {
            inv.tick(&active);
        }
        if let Some((_, class)) = scn.select.iter().find(|(st, _)| *st == t) {
            if carried.contains(&class.as_str()) && *class != active {
                active = class.clone();
                inv.select(class);
                selected_once = true;
            }
        }
        let clip = inv.clip(&active);
        let mut row = json!({ "t": t, "active": active, "clip1": clip[0], "clip2": clip[1] });
        if selected_once && gm_server::weapons::def(&active).is_some_and(|d| d.deploy.is_some()) {
            row["next_primary"] = json!(inv.next_primary);
            row["next_secondary"] = json!(inv.next_secondary);
        }
        ticks.push(row);
    }
    let loadout: Vec<Value> = WEAPONS
        .iter()
        .map(|w| {
            json!({
                "class": w.class, "clip1": w.clip1, "clip2": w.clip2, "maxclip1": w.clip1, "maxclip2": w.clip2,
                "ammo1": w.ammo1, "ammo2": w.ammo2, "slot": w.slot, "slotpos": w.slot_pos, "weight": w.weight,
                "holdtype": w.hold_type,
            })
        })
        .collect();
    let ammo: serde_json::Map<String, Value> = inv
        .ammo
        .iter()
        .map(|(k, v)| (k.clone(), json!(v)))
        .collect();
    Ok(json!({ "loadout": loadout, "ticks": ticks, "ammo": ammo }))
}

/// Loadout table, reserve ammo, the active weapon and clips every tick, the fire timers where
/// we have a deploy time (within `time` seconds, the recording rounds to 0.1 ms).
fn compare_weapon(theirs: &Value, ours: &Value, tol: &toml::Table) -> bool {
    let time_tol = tol.get("time").and_then(|v| v.as_float()).unwrap_or(0.001) as f32;
    let empty = Vec::new();
    let mut ok = true;
    let tl = theirs["loadout"].as_array().unwrap_or(&empty);
    let ol = ours["loadout"].as_array().unwrap_or(&empty);
    if tl.len() != ol.len() {
        println!("  loadout: game {} weapons, ours {}", tl.len(), ol.len());
        ok = false;
    }
    let fields = [
        "class", "clip1", "clip2", "maxclip1", "maxclip2", "ammo1", "ammo2", "slot", "slotpos",
        "weight", "holdtype",
    ];
    for (a, b) in tl.iter().zip(ol) {
        for f in fields {
            if a[f] != b[f] {
                println!("  loadout {}: {f} game {} ours {}", a["class"], a[f], b[f]);
                ok = false;
            }
        }
    }
    let ta = theirs["ammo"].as_object();
    let oa = ours["ammo"].as_object();
    if let (Some(ta), Some(oa)) = (ta, oa) {
        for (k, v) in ta {
            let o = oa.get(k).cloned().unwrap_or(json!(0));
            if v.as_i64().unwrap_or(0) != o.as_i64().unwrap_or(0) {
                println!("  ammo {k}: game {v} ours {o}");
                ok = false;
            }
        }
    }
    let tr = theirs["ticks"].as_array().unwrap_or(&empty);
    let or = ours["ticks"].as_array().unwrap_or(&empty);
    let mut diffs = 0;
    let mut max_dt = 0.0f32;
    for (a, b) in tr.iter().zip(or) {
        let mut bad =
            a["active"] != b["active"] || a["clip1"] != b["clip1"] || a["clip2"] != b["clip2"];
        if let (Some(x), Some(y)) = (a["next_primary"].as_f64(), b["next_primary"].as_f64()) {
            let d = (x - y).abs() as f32;
            max_dt = max_dt.max(d);
            bad |= d > time_tol;
        }
        if bad {
            diffs += 1;
            if diffs <= 5 {
                println!(
                    "  tick {}: game {} {}/{} next {} | ours {} {}/{} next {}",
                    a["t"],
                    a["active"],
                    a["clip1"],
                    a["clip2"],
                    a["next_primary"],
                    b["active"],
                    b["clip1"],
                    b["clip2"],
                    b["next_primary"]
                );
            }
            ok = false;
        }
    }
    println!(
        "  {} weapons, {} ticks, {} differing, max fire-timer err {max_dt:.4} s (tol {time_tol}): {}",
        tl.len(), tr.len(), diffs, if ok { "equal" } else { "DIFF" }
    );
    ok
}

fn run_player(scn: &Scenario) -> Result<Value> {
    use glam::Vec3;
    use gm_physics::{Surface, World};
    let inst = gm_content::find_install().context("no Garry's Mod install found (set GMOD_DIR)")?;
    let mount = gm_content::Mount::for_install(&inst)?;
    let map = scn.map.as_deref().unwrap_or("gm_construct");
    let data = mount.read(&format!("maps/{map}.bsp"))?;
    let bsp = gm_formats::bsp::Bsp::parse(&data)?;
    let collision = gm_collision::World::from_bsp(&bsp);
    let world_hulls: Vec<Vec<Vec3>> = gm_physics::brush::model_hulls(&bsp, 0, gm_physics::REST_GAP)
        .into_iter()
        .map(|h| h.points)
        .collect();
    let mut physics = World::new(600.0, gm_core::tick::DEFAULT_TICK_INTERVAL, 1);
    let world_body = physics.add_static_hulls(
        &world_hulls,
        Surface {
            friction: 0.8,
            elasticity: 0.25,
        },
    )?;
    let mut server = gm_server::Server::new(&collision, physics, world_body);
    server.set_spawn_points_from_bsp(&bsp);
    let player = 1u32;
    server.add_player(player, Vec3::ZERO, Vec3::ZERO);
    server.spawn_player(player);
    let total = scn.ticks.unwrap_or(600);
    let mut ticks = Vec::with_capacity(total as usize);
    let mut last_weapons = String::new();
    let mut cmds = std::collections::BTreeMap::new();
    cmds.insert(player, gm_server::PlayerCmd::default());
    for t in 0..total {
        if scn.kill_at.contains(&t) {
            server.kill_player(player);
        }
        let p = &server.players[&player];
        let weapons = p.weapons.join(",");
        let mut row = json!({
            "t": t,
            "pos": [p.state.origin.x, p.state.origin.y, p.state.origin.z],
            "vel": [p.state.velocity.x, p.state.velocity.y, p.state.velocity.z],
            "alive": p.alive,
            "health": p.health,
            "armor": p.armor,
            "ragdoll": p.ragdoll.is_some(),
            "deaths": p.deaths,
            "frags": p.frags,
            "active": p.active_weapon,
        });
        if weapons != last_weapons || t == 0 {
            row["weapons"] = json!(weapons);
            last_weapons = weapons;
        }
        if t == 0 {
            row["model"] = json!(p.model);
            row["maxhealth"] = json!(p.max_health);
        }
        ticks.push(row);
        server.tick(&cmds);
    }
    Ok(json!({
        "spawns": server.spawn_points.iter().map(|s| json!({ "class": "info_player_start", "pos": [s.pos.x, s.pos.y, s.pos.z], "ang": [s.angles.x, s.angles.y, s.angles.z] })).collect::<Vec<_>>(),
        "ticks": ticks,
    }))
}

/// Life cycle: alive, health, armour, deaths, ragdoll and the weapon list equal every tick,
/// respawns on the same ticks, each spawn position within `pos` of one of the game's spawn
/// points (the game picks one at random, so the point itself may differ).
fn compare_player(theirs: &Value, ours: &Value, tol: &toml::Table) -> bool {
    let pos_tol = tol.get("pos").and_then(|v| v.as_float()).unwrap_or(2.0) as f32;
    let empty = Vec::new();
    let tr = theirs["ticks"].as_array().unwrap_or(&empty);
    let or = ours["ticks"].as_array().unwrap_or(&empty);
    let v3 = |v: &Value| -> glam::Vec3 {
        let a = v.as_array().cloned().unwrap_or_default();
        glam::Vec3::new(
            a.first().and_then(Value::as_f64).unwrap_or(0.0) as f32,
            a.get(1).and_then(Value::as_f64).unwrap_or(0.0) as f32,
            a.get(2).and_then(Value::as_f64).unwrap_or(0.0) as f32,
        )
    };
    let spawns: Vec<glam::Vec3> = theirs["spawns"]
        .as_array()
        .unwrap_or(&empty)
        .iter()
        .map(|s| v3(&s["pos"]))
        .collect();
    let mut ok = tr.len() == or.len();
    if !ok {
        println!("  {} ticks theirs, {} ours", tr.len(), or.len());
    }
    let (mut tw, mut ow) = (String::new(), String::new());
    let mut mismatches = 0;
    let mut spawns_checked = 0;
    let mut worst_spawn = 0.0f32;
    let mut was_alive = true;
    for (i, (a, b)) in tr.iter().zip(or.iter()).enumerate() {
        if let Some(w) = a["weapons"].as_str() {
            tw = w.to_string();
        }
        if let Some(w) = b["weapons"].as_str() {
            ow = w.to_string();
        }
        let mut diff = Vec::new();
        for key in ["alive", "health", "armor", "deaths", "ragdoll"] {
            if a[key] != b[key] {
                diff.push(format!("{key} {} vs {}", a[key], b[key]));
            }
        }
        if tw != ow {
            diff.push(format!("weapons [{tw}] vs [{ow}]"));
        }
        let alive = a["alive"].as_bool().unwrap_or(false);
        if alive && !was_alive {
            // a spawn tick: ours must stand within pos_tol of some spawn point (1 u above it)
            let p = v3(&b["pos"]);
            let d = spawns
                .iter()
                .map(|s| (p - (*s + glam::Vec3::new(0.0, 0.0, 1.0))).length())
                .fold(f32::INFINITY, f32::min);
            spawns_checked += 1;
            worst_spawn = worst_spawn.max(d);
            if d > pos_tol {
                diff.push(format!("spawn {d:.2} u from the nearest spawn point"));
            }
        }
        was_alive = alive;
        if !diff.is_empty() {
            mismatches += 1;
            if mismatches <= 5 {
                println!("  tick {i}: {}", diff.join(", "));
            }
            ok = false;
        }
    }
    println!(
        "  {} ticks, {} differing, {} respawns checked (worst {:.2} u from a spawn point, tol {pos_tol}): {}",
        tr.len(),
        mismatches,
        spawns_checked,
        worst_spawn,
        if ok { "equal" } else { "DIFF" }
    );
    ok
}

/// Fall damage and `TakeDamage` through the server loop, recorded like the probe's `damage`
/// scenario: the bot is dropped from each height every `spacing` ticks and hit at given ticks.
fn run_damage(scn: &Scenario) -> Result<Value> {
    use glam::Vec3;
    use gm_physics::{Surface, World};
    let inst = gm_content::find_install().context("no Garry's Mod install found (set GMOD_DIR)")?;
    let mount = gm_content::Mount::for_install(&inst)?;
    let map = scn.map.as_deref().unwrap_or("gm_construct");
    let data = mount.read(&format!("maps/{map}.bsp"))?;
    let bsp = gm_formats::bsp::Bsp::parse(&data)?;
    let collision = gm_collision::World::from_bsp(&bsp);
    let world_hulls: Vec<Vec<Vec3>> = gm_physics::brush::model_hulls(&bsp, 0, gm_physics::REST_GAP)
        .into_iter()
        .map(|h| h.points)
        .collect();
    let mut physics = World::new(600.0, gm_core::tick::DEFAULT_TICK_INTERVAL, 1);
    let world_body = physics.add_static_hulls(
        &world_hulls,
        Surface {
            friction: 0.8,
            elasticity: 0.25,
        },
    )?;
    let mut server = gm_server::Server::new(&collision, physics, world_body);
    server.set_spawn_points_from_bsp(&bsp);
    server.fall_damage_progressive = scn.falldamage.unwrap_or(0) != 0;
    let player = 1u32;
    let base = Vec3::from(scn.start.unwrap_or([0.0; 3]));
    server.add_player(player, base, Vec3::ZERO);
    server.spawn_player(player);
    let spacing = scn.spacing.unwrap_or(200);
    let total = scn.ticks.unwrap_or(600);
    let mut ticks = Vec::with_capacity(total as usize);
    let mut cmds = std::collections::BTreeMap::new();
    cmds.insert(player, gm_server::PlayerCmd::default());
    for t in 0..total {
        if t % spacing == 0 {
            if let Some(h) = scn.heights.get((t / spacing) as usize) {
                if !server.players[&player].alive {
                    server.spawn_player(player);
                }
                server.set_player_health(player, scn.health.unwrap_or(100));
                server.set_player_armor(player, scn.armor.unwrap_or(0));
                server.teleport_player(player, base + Vec3::new(0.0, 0.0, *h));
            }
        }
        if let Some(hit) = scn.hits.iter().find(|h| h.t == t) {
            if let Some(a) = hit.armor {
                server.set_player_armor(player, a);
            }
            if let Some(h) = hit.health {
                server.set_player_health(player, h);
            }
            server.damage_player(player, hit.amount, hit.dmg_type.unwrap_or(0));
        }
        let p = &server.players[&player];
        ticks.push(json!({
            "t": t,
            "pos": [p.state.origin.x, p.state.origin.y, p.state.origin.z],
            "vel": [p.state.velocity.x, p.state.velocity.y, p.state.velocity.z],
            "health": p.health,
            "armor": p.armor,
            "alive": p.alive,
            "ground": p.state.on_ground,
        }));
        server.tick(&cmds);
    }
    Ok(json!({ "ticks": ticks, "falldamage": scn.falldamage.unwrap_or(0) }))
}

/// Health, armour and life equal every tick; a landing tick is allowed to differ by `contact`
/// ticks (the hit itself is then compared at the game's tick).
fn compare_damage(theirs: &Value, ours: &Value, tol: &toml::Table) -> bool {
    let slack = tol.get("contact").and_then(|v| v.as_integer()).unwrap_or(1) as usize;
    let empty = Vec::new();
    let tr = theirs["ticks"].as_array().unwrap_or(&empty);
    let or = ours["ticks"].as_array().unwrap_or(&empty);
    let mut ok = tr.len() == or.len();
    let mut mismatches = 0;
    for (i, a) in tr.iter().enumerate() {
        // the game's value must appear in ours within the slack window
        let lo = i.saturating_sub(slack);
        let hi = (i + slack).min(or.len().saturating_sub(1));
        let same = |b: &Value| ["health", "armor", "alive"].iter().all(|k| a[*k] == b[*k]);
        let hit = (lo..=hi).any(|j| or.get(j).is_some_and(same));
        if !hit {
            mismatches += 1;
            if mismatches <= 6 {
                let b = or.get(i).cloned().unwrap_or(Value::Null);
                println!(
                    "  tick {i}: theirs hp {} ar {} alive {} | ours hp {} ar {} alive {}",
                    a["health"], a["armor"], a["alive"], b["health"], b["armor"], b["alive"]
                );
            }
            ok = false;
        }
    }
    println!(
        "  {} ticks, {} differing (slack {slack}): {}",
        tr.len(),
        mismatches,
        if ok { "equal" } else { "DIFF" }
    );
    ok
}

/// The server side of a player's death: what entity stands in for the body, where, and how
/// many physics objects it has on the server (recorded by the probe's `ragdoll` scenario).
fn run_ragdoll(scn: &Scenario) -> Result<Value> {
    use glam::Vec3;
    use gm_physics::{Surface, World};
    let inst = gm_content::find_install().context("no Garry's Mod install found (set GMOD_DIR)")?;
    let mount = gm_content::Mount::for_install(&inst)?;
    let map = scn.map.as_deref().unwrap_or("gm_construct");
    let data = mount.read(&format!("maps/{map}.bsp"))?;
    let bsp = gm_formats::bsp::Bsp::parse(&data)?;
    let collision = gm_collision::World::from_bsp(&bsp);
    let world_hulls: Vec<Vec<Vec3>> = gm_physics::brush::model_hulls(&bsp, 0, gm_physics::REST_GAP)
        .into_iter()
        .map(|h| h.points)
        .collect();
    let mut physics = World::new(600.0, gm_core::tick::DEFAULT_TICK_INTERVAL, 1);
    let world_body = physics.add_static_hulls(
        &world_hulls,
        Surface {
            friction: 0.8,
            elasticity: 0.25,
        },
    )?;
    let mut server = gm_server::Server::new(&collision, physics, world_body);
    server.set_spawn_points_from_bsp(&bsp);
    let player = 1u32;
    let start = Vec3::from(scn.start.unwrap_or([0.0; 3]));
    server.add_player(player, start, Vec3::new(0.0, scn.yaw.unwrap_or(0.0), 0.0));
    server.spawn_player(player);
    server.teleport_player(player, start);
    let kill_at = scn.kill_at.first().copied().unwrap_or(20);
    let total = scn.ticks.unwrap_or(300);
    let mut ticks = Vec::with_capacity(total as usize);
    let mut info = Value::Null;
    let mut cmds = std::collections::BTreeMap::new();
    cmds.insert(player, gm_server::PlayerCmd::default());
    for t in 0..total {
        if t == kill_at {
            server.kill_player(player);
        }
        let p = &server.players[&player];
        if info.is_null() {
            if let Some(r) = &p.ragdoll {
                info = json!({
                    "class": r.class,
                    "model": r.model,
                    "count": 0,
                    "origin": [r.origin.x, r.origin.y, r.origin.z],
                    "angles": [r.angles.x, r.angles.y, r.angles.z],
                    "tick": t,
                });
            }
        }
        ticks.push(json!({
            "t": t,
            "pos": [p.state.origin.x, p.state.origin.y, p.state.origin.z],
            "alive": p.alive,
        }));
        server.tick(&cmds);
    }
    Ok(json!({ "ticks": ticks, "ragdoll": info }))
}

/// The ragdoll record: same tick, class and physics object count, origin within `pos`; alive
/// equal every tick.
fn compare_ragdoll(theirs: &Value, ours: &Value, tol: &toml::Table) -> bool {
    let pos_tol = tol.get("pos").and_then(|v| v.as_float()).unwrap_or(1.0) as f32;
    let v3 = |v: &Value| -> glam::Vec3 {
        let a = v.as_array().cloned().unwrap_or_default();
        glam::Vec3::new(
            a.first().and_then(Value::as_f64).unwrap_or(0.0) as f32,
            a.get(1).and_then(Value::as_f64).unwrap_or(0.0) as f32,
            a.get(2).and_then(Value::as_f64).unwrap_or(0.0) as f32,
        )
    };
    let (a, b) = (&theirs["ragdoll"], &ours["ragdoll"]);
    let mut diff = Vec::new();
    for key in ["class", "count", "tick"] {
        if a[key] != b[key] {
            diff.push(format!("{key} {} vs {}", a[key], b[key]));
        }
    }
    let d = (v3(&a["origin"]) - v3(&b["origin"])).length();
    if d > pos_tol {
        diff.push(format!("origin {d:.2} u apart"));
    }
    let empty = Vec::new();
    let tr = theirs["ticks"].as_array().unwrap_or(&empty);
    let or = ours["ticks"].as_array().unwrap_or(&empty);
    let alive_diffs = tr
        .iter()
        .zip(or.iter())
        .filter(|(x, y)| x["alive"] != y["alive"])
        .count();
    if alive_diffs > 0 || tr.len() != or.len() {
        diff.push(format!("alive differs on {alive_diffs} ticks"));
    }
    let ok = diff.is_empty();
    println!(
        "  ragdoll {} with {} server physics objects at tick {}, origin {d:.2} u apart (tol {pos_tol}): {}{}",
        a["class"],
        a["count"],
        a["tick"],
        if ok { "equal" } else { "DIFF " },
        diff.join(", ")
    );
    ok
}

/// The whole server loop: a bot runs sandbox commands and the props live in Jolt between them.
fn run_server(scn: &Scenario) -> Result<Value> {
    use glam::Vec3;
    use gm_physics::{Surface, World};
    let inst = gm_content::find_install().context("no Garry's Mod install found (set GMOD_DIR)")?;
    let mount = gm_content::Mount::for_install(&inst)?;
    let map = scn.map.as_deref().unwrap_or("gm_construct");
    let data = mount.read(&format!("maps/{map}.bsp"))?;
    let bsp = gm_formats::bsp::Bsp::parse(&data)?;
    let collision = gm_collision::World::from_bsp(&bsp);
    let world_hulls: Vec<Vec<Vec3>> = gm_physics::brush::model_hulls(&bsp, 0, gm_physics::REST_GAP)
        .into_iter()
        .map(|h| h.points)
        .collect();
    let table = gm_physics::surface::SurfaceTable::parse(&[&String::from_utf8_lossy(
        &mount.read("scripts/surfaceproperties.txt")?,
    )]);
    let world_sp = table.get("concrete");
    let world_surface = Surface {
        friction: world_sp.friction,
        elasticity: world_sp.elasticity,
    };
    let mut physics = World::new(600.0, gm_core::tick::DEFAULT_TICK_INTERVAL, 1);
    tune_solver(&mut physics);
    let world_body = physics.add_static_hulls(&world_hulls, world_surface)?;
    let (disp_pos, disp_idx) = gm_physics::brush::displacement_mesh(&bsp, gm_physics::REST_GAP);
    if !disp_idx.is_empty() {
        physics.add_static_mesh(&disp_pos, &disp_idx, world_surface)?;
    }
    let mut server = gm_server::Server::new(&collision, physics, world_body);
    let player = 1u32;
    let start = Vec3::from(scn.start.unwrap_or([0.0; 3]));
    let ang = scn.angles.clone().unwrap_or_default();
    let view0 = Vec3::new(
        ang.first().copied().unwrap_or(0.0),
        ang.get(1).copied().unwrap_or(0.0),
        0.0,
    );
    server.add_player(player, start, view0);
    if !scn.segments.is_empty() {
        return run_server_segments(scn, &mount, server, player, start, view0);
    }
    let mut known: std::collections::HashSet<String> = Default::default();
    for pr in &scn.props {
        if known.insert(pr.model.clone()) {
            server.register_model(&pr.model, model_info(&mount, &pr.model)?);
        }
    }
    for op in &scn.ops {
        if let Some(m) = &op.model {
            if known.insert(m.clone()) {
                server.register_model(m, model_info(&mount, m)?);
            }
        }
    }
    let existing: Vec<(String, Vec3, Vec3, bool)> = scn
        .props
        .iter()
        .map(|p| {
            (
                p.model.clone(),
                Vec3::from(p.pos),
                Vec3::from(p.angles.unwrap_or([0.0; 3])),
                p.frozen,
            )
        })
        .collect();
    server.add_existing_props(&existing);
    let all_props = !scn.props.is_empty();
    let mut script = match &scn.addon {
        Some(dir) => {
            let mut rt = gm_script::Runtime::new();
            rt.load_addon(&repo_root().join(dir))
                .map_err(|e| anyhow::anyhow!("addon {dir}: {e}"))?;
            Some(rt)
        }
        None => None,
    };
    // the probe's pre-roll: the bot stands still on the ground
    let idle = std::collections::BTreeMap::from([(player, gm_server::PlayerCmd::default())]);
    for _ in 0..20 {
        server.tick(&idle);
    }
    let snapshot = |server: &gm_server::Server| -> Value {
        let sb = &server.sandbox;
        json!({
            "props": sb.props().iter().filter(|p| all_props || p.owner == player).map(|p| json!({
                "model": p.model, "pos": [p.pos.x, p.pos.y, p.pos.z], "ang": [p.angles.x, p.angles.y, p.angles.z], "frozen": p.frozen,
            })).collect::<Vec<_>>(),
            "count": sb.count(player),
            "undos": sb.undos(player).iter().map(|u| json!({ "id": u.id, "name": u.name, "n": u.entities.len() })).collect::<Vec<_>>(),
            "constraints": sb.constraints().len(),
        })
    };
    let mut results = Vec::new();
    let mut messages = Vec::new();
    for (i, op) in scn.ops.iter().enumerate() {
        let mut rec = json!({ "op": op.op, "t": i });
        let view = Vec3::new(op.pitch, op.yaw, 0.0);
        if let Some(p) = server.players.get_mut(&player) {
            p.view_angles = view;
        }
        let n_before = server.events.len();
        match op.op.as_str() {
            "spawn" => {
                let model = op.model.clone().unwrap_or_default();
                rec["model"] = json!(model);
                server.spawn_prop(player, &model);
            }
            "click" => {
                let tool = op.tool.clone().unwrap_or_default();
                let ret = match op.button.as_deref() {
                    Some("reload") => server.sandbox.tool_reload(player),
                    b => match server.tool_click_full(player, &tool, b == Some("right")) {
                        gm_server::ToolClick::Done(r) => r,
                        gm_server::ToolClick::Miss => false,
                        gm_server::ToolClick::Unknown(hit) => match script.as_mut() {
                            Some(rt) => {
                                rt.tool_click(&mut server, player, &tool, &hit, b == Some("right"))
                            }
                            None => false,
                        },
                    },
                };
                rec["ret"] = json!(ret);
            }
            "undo" => server.undo(player),
            "cleanup" => server.cleanup(player),
            "wait" => {}
            "cvar" => {
                if op.name.as_deref() == Some("sbox_maxprops") {
                    server.sandbox.limits.max_props =
                        op.value.as_deref().unwrap_or("200").parse().unwrap_or(200);
                }
            }
            other => bail!("unknown server op {other}"),
        }
        for e in &server.events[n_before..] {
            if let gm_sandbox::Event::LimitHit { limit } = e {
                messages.push(json!({ "t": i, "limit": limit }));
            }
        }
        rec["immediate"] = snapshot(&server);
        let ticks = if op.op == "wait" {
            op.ticks.unwrap_or(3)
        } else {
            op.wait.unwrap_or(3)
        };
        for _ in 0..ticks {
            server.tick(&idle);
        }
        rec["after"] = snapshot(&server);
        results.push(rec);
    }
    Ok(json!({ "results": results, "messages": messages }))
}

/// A segment scenario (the physgun recordings) through the server loop: one prop on the map, the
/// bot's buttons and view per tick, the prop's state recorded like `run_physgun`.
fn run_server_segments(
    scn: &Scenario,
    mount: &gm_content::Mount,
    mut server: gm_server::Server<'_>,
    player: u32,
    start: glam::Vec3,
    view0: glam::Vec3,
) -> Result<Value> {
    use glam::Vec3;
    let model = scn
        .model
        .as_deref()
        .context("segment scenario needs a model")?;
    server.register_model(model, model_info(mount, model)?);
    server.add_existing_props(&[(
        model.to_string(),
        Vec3::from(scn.prop.unwrap_or([0.0; 3])),
        Vec3::from(scn.prop_angles.unwrap_or([0.0; 3])),
        false,
    )]);
    let body = server.body_of(1).context("prop body")?;
    // the probe's pre-roll: the bot is reset to the start every tick while the prop settles
    let settle = std::collections::BTreeMap::from([(
        player,
        gm_server::PlayerCmd {
            cmd: gm_move::UserCmd {
                view_angles: view0,
                ..Default::default()
            },
            ..Default::default()
        },
    )]);
    for _ in 0..60 {
        if let Some(p) = server.players.get_mut(&player) {
            p.state.origin = start;
            p.state.velocity = Vec3::ZERO;
        }
        server.tick(&settle);
    }
    let mut ticks = Vec::new();
    let mut t = 0u32;
    for sg in &scn.segments {
        for _ in 0..sg.ticks {
            let st = server.physics.state(body);
            let p = &server.players[&player];
            let eye = server.eye(player).map(|e| e.0).unwrap_or_default();
            ticks.push(serde_json::json!({
                "t": t,
                "pos": [st.position.x, st.position.y, st.position.z],
                "ang": [st.angles.0, st.angles.1, st.angles.2],
                "vel": [st.velocity.x, st.velocity.y, st.velocity.z],
                "angvel": local_angvel(&st),
                "held": p.held.is_some(),
                "frozen": server.physics.is_frozen(body),
                "eye": [eye.x, eye.y, eye.z],
                "eye_ang": [sg.pitch, sg.yaw, 0.0],
            }));
            let cmds = std::collections::BTreeMap::from([(
                player,
                gm_server::PlayerCmd {
                    cmd: gm_move::UserCmd {
                        buttons: sg.buttons,
                        view_angles: Vec3::new(sg.pitch, sg.yaw, 0.0),
                        forward_move: sg.forward,
                        side_move: sg.side,
                        ..Default::default()
                    },
                    physgun: true,
                    mouse_x: sg.mouse_x,
                    mouse_y: sg.mouse_y,
                },
            )]);
            server.tick(&cmds);
            t += 1;
        }
    }
    Ok(
        serde_json::json!({ "start": scn.start.unwrap_or([0.0; 3]), "model": model, "ticks": ticks }),
    )
}

/// The server scenario again, but every input travels as packets: a headless client sends
/// usercmds (last 8 per packet) and reliable sandbox commands through a lossy, delayed loopback
/// to `ServerNet`, and the recorded state is what the client rebuilt from delta snapshots.
fn run_net(scn: &Scenario) -> Result<Value> {
    use glam::Vec3;
    use gm_net::{ClientNet, Command, Loopback, NetCmd, ServerNet};
    use gm_physics::{Surface, World};
    let inst = gm_content::find_install().context("no Garry's Mod install found (set GMOD_DIR)")?;
    let mount = gm_content::Mount::for_install(&inst)?;
    let map = scn.map.as_deref().unwrap_or("gm_construct");
    let data = mount.read(&format!("maps/{map}.bsp"))?;
    let bsp = gm_formats::bsp::Bsp::parse(&data)?;
    let collision = gm_collision::World::from_bsp(&bsp);
    let world_hulls: Vec<Vec<Vec3>> = gm_physics::brush::model_hulls(&bsp, 0, gm_physics::REST_GAP)
        .into_iter()
        .map(|h| h.points)
        .collect();
    let table = gm_physics::surface::SurfaceTable::parse(&[&String::from_utf8_lossy(
        &mount.read("scripts/surfaceproperties.txt")?,
    )]);
    let world_sp = table.get("concrete");
    let world_surface = Surface {
        friction: world_sp.friction,
        elasticity: world_sp.elasticity,
    };
    let mut physics = World::new(600.0, gm_core::tick::DEFAULT_TICK_INTERVAL, 1);
    tune_solver(&mut physics);
    let world_body = physics.add_static_hulls(&world_hulls, world_surface)?;
    let (disp_pos, disp_idx) = gm_physics::brush::displacement_mesh(&bsp, gm_physics::REST_GAP);
    if !disp_idx.is_empty() {
        physics.add_static_mesh(&disp_pos, &disp_idx, world_surface)?;
    }
    let mut server = gm_server::Server::new(&collision, physics, world_body);
    let start = Vec3::from(scn.start.unwrap_or([0.0; 3]));
    if !scn.kill_at.is_empty() {
        server.set_spawn_points_from_bsp(&bsp);
        return run_net_lifecycle(scn, server, start);
    }
    if !scn.segments.is_empty() {
        return run_net_predict(scn, &collision, server, start);
    }
    if scn.ops.is_empty() && scn.model.is_some() {
        return run_net_interp(scn, &mount, server, start);
    }
    let mut known: std::collections::HashSet<String> = Default::default();
    for m in scn
        .props
        .iter()
        .map(|p| &p.model)
        .chain(scn.ops.iter().filter_map(|o| o.model.as_ref()))
    {
        if known.insert(m.clone()) {
            server.register_model(m, model_info(&mount, m)?);
        }
    }
    let existing: Vec<(String, Vec3, Vec3, bool)> = scn
        .props
        .iter()
        .map(|p| {
            (
                p.model.clone(),
                Vec3::from(p.pos),
                Vec3::from(p.angles.unwrap_or([0.0; 3])),
                p.frozen,
            )
        })
        .collect();
    server.add_existing_props(&existing);
    let all_props = !scn.props.is_empty();
    // 2 ticks one way (30 ms), 20 % loss, fixed seed
    let net = Loopback::new(2, 0.2, 0x5eed);
    let (mut s_ep, mut c_ep) = (net.endpoint(0), net.endpoint(1));
    let mut sn = ServerNet::new(map, start);
    let mut client = ClientNet::new(1, "bot");
    let mut view = Vec3::ZERO;
    let step = |sn: &mut ServerNet,
                client: &mut ClientNet,
                s_ep: &mut gm_net::LoopbackEndpoint,
                c_ep: &mut gm_net::LoopbackEndpoint,
                server: &mut gm_server::Server<'_>,
                view: Vec3| {
        client.tick(
            NetCmd {
                view_angles: view,
                ..Default::default()
            },
            c_ep,
        );
        net.tick();
        sn.receive(server, s_ep);
        sn.tick(server);
        sn.send(server, s_ep);
        client.receive(c_ep);
    };
    for _ in 0..40 {
        step(
            &mut sn,
            &mut client,
            &mut s_ep,
            &mut c_ep,
            &mut server,
            view,
        );
    }
    let snapshot = |client: &ClientNet| -> Value {
        let me = client.player_id.unwrap_or(0);
        json!({
            "props": client.world.entities.values().filter(|e| all_props || e.owner == me).map(|e| json!({
                "model": e.model, "pos": [e.pos.x, e.pos.y, e.pos.z], "ang": [e.angles.x, e.angles.y, e.angles.z], "frozen": e.frozen,
            })).collect::<Vec<_>>(),
            "count": client.world.own.count,
            "undos": client.world.own.undos.iter().map(|u| json!({ "id": u.id, "name": u.name })).collect::<Vec<_>>(),
            "constraints": client.world.own.constraints,
            "tick": client.world.tick,
        })
    };
    let mut results = Vec::new();
    let mut messages = Vec::new();
    for (i, op) in scn.ops.iter().enumerate() {
        let mut rec = json!({ "op": op.op, "t": i });
        view = Vec3::new(op.pitch, op.yaw, 0.0);
        // the view must be on the server before the command runs: one usercmd ahead
        for _ in 0..4 {
            step(
                &mut sn,
                &mut client,
                &mut s_ep,
                &mut c_ep,
                &mut server,
                view,
            );
        }
        match op.op.as_str() {
            "spawn" => client.command(Command::Spawn {
                model: op.model.clone().unwrap_or_default(),
            }),
            "click" => client.command(Command::ToolClick {
                tool: op.tool.clone().unwrap_or_default(),
                right: op.button.as_deref() == Some("right"),
                reload: op.button.as_deref() == Some("reload"),
            }),
            "undo" => client.command(Command::Undo),
            "cleanup" => client.command(Command::Cleanup),
            "wait" => {}
            "cvar" => client.command(Command::Cvar {
                name: op.name.clone().unwrap_or_default(),
                value: op.value.clone().unwrap_or_default(),
            }),
            other => bail!("unknown net op {other}"),
        }
        // the command needs a round trip (and retries under loss) before the wait starts
        let want = client.acked_commands + if op.op == "wait" { 0 } else { 1 };
        let mut spent = 0;
        while client.acked_commands < want && spent < 200 {
            step(
                &mut sn,
                &mut client,
                &mut s_ep,
                &mut c_ep,
                &mut server,
                view,
            );
            spent += 1;
        }
        rec["round_trip_ticks"] = json!(spent);
        rec["immediate"] = snapshot(&client);
        let ticks = if op.op == "wait" {
            op.ticks.unwrap_or(3)
        } else {
            op.wait.unwrap_or(3)
        };
        for _ in 0..ticks {
            step(
                &mut sn,
                &mut client,
                &mut s_ep,
                &mut c_ep,
                &mut server,
                view,
            );
        }
        // the client sees the state 2 ticks + snapshot interval later: let the snapshot arrive
        for _ in 0..4 {
            step(
                &mut sn,
                &mut client,
                &mut s_ep,
                &mut c_ep,
                &mut server,
                view,
            );
        }
        rec["after"] = snapshot(&client);
        for e in client.events.drain(..) {
            messages.push(json!({ "t": i, "event": e }));
        }
        results.push(rec);
    }
    let st = client.stats().clone();
    let sst = sn.stats(1).cloned().unwrap_or_default();
    Ok(json!({
        "results": results, "messages": messages,
        "net": {
            "client_packets": st.packets_sent, "client_bytes": st.bytes_sent,
            "server_packets": sst.packets_sent, "server_bytes": sst.bytes_sent,
            "dropped": net.dropped(), "reliable_resends": st.reliable_resends + sst.reliable_resends,
            "snapshots_without_baseline": client.dropped_snapshots,
        },
    }))
}

/// A movement recording through the network with client prediction: the client runs every
/// usercmd locally the tick it sends it and reconciles against the server's state in snapshots;
/// the recorded trajectory is the prediction (same shape as `run_move`, so the movement golden
/// applies bit for bit when no correction happens).
fn run_net_predict(
    scn: &Scenario,
    collision: &gm_collision::World,
    mut server: gm_server::Server<'_>,
    start: glam::Vec3,
) -> Result<Value> {
    use glam::Vec3;
    use gm_net::{ClientNet, Loopback, NetCmd, Prediction, ServerNet};
    let loss: f32 = std::env::var("GM_NET_LOSS")
        .ok()
        .and_then(|v| v.parse().ok())
        .unwrap_or(0.0);
    let net = Loopback::new(2, loss, 0x5eed);
    let (mut s_ep, mut c_ep) = (net.endpoint(0), net.endpoint(1));
    let mut sn = ServerNet::new(scn.map.as_deref().unwrap_or("gm_construct"), start);
    let mut client = ClientNet::new(1, "bot");
    let ang = scn.angles.clone().unwrap_or_default();
    let view0 = Vec3::new(
        ang.first().copied().unwrap_or(0.0),
        ang.get(1).copied().unwrap_or(0.0),
        0.0,
    );
    // the probe's pre-roll: origin and velocity reset every tick, then one movement tick;
    // the server player does the same so both sides start from the identical settled state
    let settle = |server: &mut gm_server::Server<'_>, st: &mut gm_move::PlayerState| {
        if let Some(p) = server.players.get_mut(&1) {
            p.state.origin = start;
            p.state.velocity = Vec3::ZERO;
        }
        st.origin = start;
        st.velocity = Vec3::ZERO;
    };
    let mut local = gm_move::PlayerState {
        origin: start,
        ..Default::default()
    };
    let idle_cmd = NetCmd {
        view_angles: view0,
        ..Default::default()
    };
    let mover = gm_move::Mover::new(gm_move::MoveConfig::default(), collision);
    for _ in 0..20 {
        settle(&mut server, &mut local);
        client.tick(idle_cmd.clone(), &mut c_ep);
        net.tick();
        sn.receive(&mut server, &mut s_ep);
        sn.tick(&mut server);
        sn.send(&server, &mut s_ep);
        client.receive(&mut c_ep);
        mover.tick(&mut local, &Prediction::usercmd(&idle_cmd));
    }
    // from here the server only runs what the client sends; wait until the pipe is primed
    let mut pred = Prediction::new(collision, local);
    let mut ticks = Vec::new();
    let mut t = 0u32;
    let mut cmd_tick = 20u32;
    for seg in &scn.segments {
        let cmd = NetCmd {
            buttons: seg.buttons,
            view_angles: Vec3::new(seg.pitch, seg.yaw, 0.0),
            forward_move: seg.forward,
            side_move: seg.side,
            up_move: seg.up,
            ..Default::default()
        };
        for _ in 0..seg.ticks {
            cmd_tick += 1;
            pred.predict(cmd_tick, &cmd);
            client.tick(cmd.clone(), &mut c_ep);
            net.tick();
            sn.receive(&mut server, &mut s_ep);
            sn.tick(&mut server);
            sn.send(&server, &mut s_ep);
            client.receive(&mut c_ep);
            if let Some(l) = &client.world.local {
                pred.reconcile(client.world.last_cmd_tick, &l.state);
            }
            let st = &pred.state;
            ticks.push(serde_json::json!({
                "t": t,
                "pos": [st.origin.x, st.origin.y, st.origin.z],
                "vel": [st.velocity.x, st.velocity.y, st.velocity.z],
                "ground": st.on_ground,
                "ducked": st.ducked,
            }));
            t += 1;
        }
    }
    // drain: the server catches up with the last usercmds and the final states must agree
    for _ in 0..12 {
        net.tick();
        sn.receive(&mut server, &mut s_ep);
        sn.tick(&mut server);
        sn.send(&server, &mut s_ep);
        client.receive(&mut c_ep);
        if let Some(l) = &client.world.local {
            pred.reconcile(client.world.last_cmd_tick, &l.state);
        }
    }
    let server_state = server.players[&1].state;
    let st = pred.state;
    Ok(serde_json::json!({
        "start": [start.x, start.y, start.z],
        "ticks": ticks,
        "final": [st.origin.x, st.origin.y, st.origin.z],
        "server_final": [server_state.origin.x, server_state.origin.y, server_state.origin.z],
        "agree": st == server_state,
        "corrections": pred.corrections,
        "max_correction": pred.max_correction,
        "loss": loss,
        "dropped": net.dropped(),
    }))
}

/// The player life cycle through the network: the server kills the player at the scenario's
/// ticks, the client reads life, health, armour and weapons from the snapshots.
fn run_net_lifecycle(
    scn: &Scenario,
    mut server: gm_server::Server<'_>,
    start: glam::Vec3,
) -> Result<Value> {
    use gm_net::{ClientNet, Loopback, NetCmd, ServerNet};
    let net = Loopback::new(2, 0.0, 0x5eed);
    let (mut s_ep, mut c_ep) = (net.endpoint(0), net.endpoint(1));
    let mut sn = ServerNet::new(scn.map.as_deref().unwrap_or("gm_construct"), start);
    let mut client = ClientNet::new(1, "bot");
    let idle = NetCmd::default();
    let mut step = |server: &mut gm_server::Server<'_>, client: &mut ClientNet| {
        client.tick(idle.clone(), &mut c_ep);
        net.tick();
        sn.receive(server, &mut s_ep);
        sn.tick(server);
        sn.send(server, &mut s_ep);
        client.receive(&mut c_ep);
    };
    for _ in 0..20 {
        step(&mut server, &mut client);
    }
    server.spawn_player(1);
    let total = scn.ticks.unwrap_or(600);
    let mut ticks = Vec::with_capacity(total as usize);
    for t in 0..total {
        if scn.kill_at.contains(&t) {
            server.kill_player(1);
        }
        step(&mut server, &mut client);
        let seen = client.world.players.iter().find(|p| p.id == 1);
        let sp = &server.players[&1];
        ticks.push(json!({
            "t": t,
            "pos": seen.map(|p| [p.origin.x, p.origin.y, p.origin.z]),
            "alive": seen.map(|p| p.alive),
            "health": seen.map(|p| p.health),
            "armor": seen.map(|p| p.armor),
            "weapons": seen.map(|p| gm_net::msg::weapon_names(p.weapons).join(",")),
            "active": seen.map(|p| gm_net::msg::weapon_names(1u16.checked_shl(p.active as u32).unwrap_or(0)).join("")),
            "server": { "alive": sp.alive, "health": sp.health, "armor": sp.armor },
        }));
    }
    Ok(
        json!({ "ticks": ticks, "spawns": server.spawn_points.iter().map(|s| json!({ "pos": [s.pos.x, s.pos.y, s.pos.z] })).collect::<Vec<_>>() }),
    )
}

/// The client's view of the life cycle against the game's recording: life, health, armour and
/// the weapon list must appear within `contact` ticks (snapshot latency), each respawn position
/// within `pos` of a spawn point.
fn compare_net_player(theirs: &Value, ours: &Value, tol: &toml::Table) -> bool {
    let slack = tol.get("contact").and_then(|v| v.as_integer()).unwrap_or(3) as usize;
    let pos_tol = tol.get("pos").and_then(|v| v.as_float()).unwrap_or(2.0) as f32;
    let empty = Vec::new();
    let tr = theirs["ticks"].as_array().unwrap_or(&empty);
    let or = ours["ticks"].as_array().unwrap_or(&empty);
    let v3 = |v: &Value| -> glam::Vec3 {
        let a = v.as_array().cloned().unwrap_or_default();
        glam::Vec3::new(
            a.first().and_then(Value::as_f64).unwrap_or(0.0) as f32,
            a.get(1).and_then(Value::as_f64).unwrap_or(0.0) as f32,
            a.get(2).and_then(Value::as_f64).unwrap_or(0.0) as f32,
        )
    };
    let spawns: Vec<glam::Vec3> = theirs["spawns"]
        .as_array()
        .unwrap_or(&empty)
        .iter()
        .map(|s| v3(&s["pos"]))
        .collect();
    let mut ok = tr.len() == or.len();
    // the game's weapon list per tick (only sent when it changes)
    let mut weapons = Vec::with_capacity(tr.len());
    let mut tw = String::new();
    for a in tr {
        if let Some(w) = a["weapons"].as_str() {
            tw = w.to_string();
        }
        weapons.push(tw.clone());
    }
    let mut mismatches = 0;
    let mut was_alive = true;
    let mut worst_spawn = 0.0f32;
    let mut max_lag = 0usize;
    for (i, a) in tr.iter().enumerate() {
        let tw = &weapons[i];
        // the death tick still lists the weapons for one tick; a snapshot two ticks later
        // cannot show that transient, so the next tick's list is accepted too
        let tw_next = weapons.get(i + 1).unwrap_or(tw);
        let same = |b: &Value| {
            let w = b["weapons"].as_str().unwrap_or("");
            a["alive"] == b["alive"]
                && a["health"] == b["health"]
                && a["armor"] == b["armor"]
                && (w == tw || w == tw_next)
        };
        let found = (i..=(i + slack).min(or.len().saturating_sub(1)))
            .find(|j| or.get(*j).is_some_and(same));
        match found {
            Some(j) => max_lag = max_lag.max(j - i),
            None => {
                mismatches += 1;
                if mismatches <= 5 {
                    let b = or.get(i).cloned().unwrap_or(Value::Null);
                    println!("  tick {i}: game alive {} hp {} ar {} [{tw}] | client alive {} hp {} ar {} [{}]", a["alive"], a["health"], a["armor"], b["alive"], b["health"], b["armor"], b["weapons"]);
                }
                ok = false;
            }
        }
        let alive = a["alive"].as_bool().unwrap_or(false);
        if alive && !was_alive {
            if let Some(b) = or.get((i + slack).min(or.len().saturating_sub(1))) {
                let p = v3(&b["pos"]);
                let d = spawns
                    .iter()
                    .map(|s| (p - (*s + glam::Vec3::new(0.0, 0.0, 1.0))).length())
                    .fold(f32::INFINITY, f32::min);
                worst_spawn = worst_spawn.max(d);
                if d > pos_tol {
                    println!("  tick {i}: respawn seen {d:.2} u from a spawn point");
                    ok = false;
                }
            }
        }
        was_alive = alive;
    }
    println!(
        "  {} ticks, {} differing, max snapshot lag {max_lag} ticks (slack {slack}), respawns within {worst_spawn:.2} u (tol {pos_tol}): {}",
        tr.len(), mismatches, if ok { "equal" } else { "DIFF" }
    );
    ok
}

/// A rigid body recording through the network: the prop falls on the server, the client gets
/// 33 Hz snapshots and interpolates two snapshot intervals behind; the recorded trajectory is the
/// client's interpolated pose indexed by render tick, so the physics golden applies.
fn run_net_interp(
    scn: &Scenario,
    mount: &gm_content::Mount,
    mut server: gm_server::Server<'_>,
    start: glam::Vec3,
) -> Result<Value> {
    use glam::Vec3;
    use gm_net::{ClientNet, Interpolator, Loopback, NetCmd, ServerNet};
    let model = scn
        .model
        .as_deref()
        .context("interp scenario needs a model")?;
    server.register_model(model, model_info(mount, model)?);
    let player_start = Vec3::new(start.x, start.y - 200.0, -143.0);
    let net = Loopback::new(2, 0.0, 0x5eed);
    let (mut s_ep, mut c_ep) = (net.endpoint(0), net.endpoint(1));
    let mut sn = ServerNet::new(scn.map.as_deref().unwrap_or("gm_construct"), player_start);
    if let Some(n) = scn.snapshot_every {
        sn.snapshot_every = n.max(1);
    }
    let mut client = ClientNet::new(1, "bot");
    let mut interp = Interpolator::new(2.0 * sn.snapshot_every as f32);
    let idle = NetCmd::default();
    let angles = scn.angles.clone().unwrap_or_default();
    let mut body = None;
    // the prop appears right before the last warm-up snapshot, so the client has its spawn pose
    for i in 0..20 {
        client.tick(idle.clone(), &mut c_ep);
        net.tick();
        sn.receive(&mut server, &mut s_ep);
        sn.tick(&mut server);
        if i == 19 {
            server.add_existing_props(&[(
                model.to_string(),
                start,
                Vec3::new(
                    angles.first().copied().unwrap_or(0.0),
                    angles.get(1).copied().unwrap_or(0.0),
                    angles.get(2).copied().unwrap_or(0.0),
                ),
                false,
            )]);
            let b = server.body_of(1).context("prop body")?;
            if let Some(v) = scn.velocity {
                server.physics.set_velocity(
                    b,
                    Vec3::from(v),
                    Vec3::from(scn.angular.unwrap_or([0.0; 3])),
                );
            }
            body = Some(b);
        }
        sn.send(&server, &mut s_ep);
        client.receive(&mut c_ep);
    }
    let body = body.context("prop body")?;
    let total = scn.ticks.unwrap_or(200);
    let spawn_tick = sn.tick;
    let mut truth: Vec<(Vec3, glam::Quat)> = Vec::new();
    let mut ticks = Vec::new();
    let (mut max_pos_err, mut max_ang_err, mut extrapolated) = (0.0f32, 0.0f32, 0u32);
    // the prop's tick t is the server tick spawn_tick + 1 + t; the client renders behind
    let lag = interp.interp_ticks as u32 + 3;
    for i in 0..(total + lag) {
        if i < total {
            let st = server.physics.state(body);
            truth.push((
                st.position,
                gm_sandbox::rotation(Vec3::new(st.angles.0, st.angles.1, st.angles.2)),
            ));
        }
        client.tick(idle.clone(), &mut c_ep);
        net.tick();
        sn.receive(&mut server, &mut s_ep);
        sn.tick(&mut server);
        sn.send(&server, &mut s_ep);
        client.receive(&mut c_ep);
        interp.push(client.world.tick, &client.world.entities);
        let render = interp.render_tick(sn.tick);
        let t = render - spawn_tick as f32;
        if t >= 0.0 && (t as u32) < total && t.fract() == 0.0 {
            let t = t as u32;
            if let Some(p) = interp.sample(1, render) {
                let (tp, tr) = truth[t as usize];
                let dp = (p.pos - tp).length();
                let da = gm_sandbox::rotation(p.angles)
                    .angle_between(tr)
                    .to_degrees();
                max_pos_err = max_pos_err.max(dp);
                max_ang_err = max_ang_err.max(da);
                extrapolated += p.extrapolated as u32;
                ticks.push(serde_json::json!({
                    "t": t,
                    "pos": [p.pos.x, p.pos.y, p.pos.z],
                    "ang": [p.angles.x, p.angles.y, p.angles.z],
                    "vel": [p.vel.x, p.vel.y, p.vel.z],
                    "angvel": [p.angvel.x, p.angvel.y, p.angvel.z],
                    "interp_err": dp,
                }));
            }
        }
    }
    Ok(serde_json::json!({
        "start": scn.start.unwrap_or([0.0; 3]), "model": model,
        "info": { "interp_ticks": interp.interp_ticks, "snapshot_every": sn.snapshot_every },
        "ticks": ticks,
        "interp": { "max_pos_err_vs_server": max_pos_err, "max_ang_err_vs_server": max_ang_err, "extrapolated_ticks": extrapolated, "ticks": ticks.len() },
    }))
}

/// Per op: the sandbox state (count, undo stack) and every prop's pose under physics after the
/// op's wait, against the recording.
fn compare_server(theirs: &Value, ours: &Value, tol: &toml::Table) -> bool {
    let pos_tol = tol.get("pos").and_then(|v| v.as_float()).unwrap_or(2.0) as f32;
    let empty = Vec::new();
    let v3 = |v: &Value| -> glam::Vec3 {
        let a = v.as_array().cloned().unwrap_or_default();
        glam::Vec3::new(
            a.first().and_then(Value::as_f64).unwrap_or(0.0) as f32,
            a.get(1).and_then(Value::as_f64).unwrap_or(0.0) as f32,
            a.get(2).and_then(Value::as_f64).unwrap_or(0.0) as f32,
        )
    };
    let (tr, or) = (
        theirs["results"].as_array().unwrap_or(&empty),
        ours["results"].as_array().unwrap_or(&empty),
    );
    let mut ok = tr.len() == or.len();
    let (mut max_pos, mut max_ang) = (0.0f32, 0.0f32);
    for (i, (t, o)) in tr.iter().zip(or).enumerate() {
        let (ta, oa) = (&t["after"], &o["after"]);
        if !ta["count"].is_null() && ta["count"] != oa["count"] {
            println!("  op {i}: count {} vs {}", ta["count"], oa["count"]);
            ok = false;
        }
        let (tu, ou) = (
            ta["undos"].as_array().unwrap_or(&empty),
            oa["undos"].as_array().unwrap_or(&empty),
        );
        if tu.len() != ou.len()
            || tu
                .iter()
                .zip(ou)
                .any(|(a, b)| a["id"] != b["id"] || a["name"] != b["name"])
        {
            println!("  op {i}: undo stack {} vs {}", tu.len(), ou.len());
            ok = false;
        }
        let (tp, op_) = (
            ta["props"].as_array().unwrap_or(&empty),
            oa["props"].as_array().unwrap_or(&empty),
        );
        if tp.len() != op_.len() {
            println!("  op {i}: {} props vs {}", tp.len(), op_.len());
            ok = false;
            continue;
        }
        for (k, (a, b)) in tp.iter().zip(op_).enumerate() {
            let dp = (v3(&a["pos"]) - v3(&b["pos"])).length();
            let da = gm_sandbox::rotation(v3(&a["ang"]))
                .angle_between(gm_sandbox::rotation(v3(&b["ang"])))
                .to_degrees();
            max_pos = max_pos.max(dp);
            max_ang = max_ang.max(da);
            if dp > pos_tol || da > 5.0 {
                println!(
                    "  op {i}: prop {} pos err {dp:.3} angle err {da:.2} ({:?} vs {:?})",
                    k + 1,
                    v3(&a["pos"]),
                    v3(&b["pos"])
                );
                ok = false;
            }
        }
    }
    println!(
        "  {} ops, max pose err {max_pos:.3} u / {max_ang:.2} deg (tol {pos_tol}): {}",
        tr.len(),
        if ok { "equal" } else { "DIFF" }
    );
    ok
}

fn run_toolgun(scn: &Scenario) -> Result<Value> {
    use glam::Vec3;
    use gm_sandbox::{ConstraintKind, Event, Limits, PropBody, Sandbox};
    let inst = gm_content::find_install().context("no Garry's Mod install found (set GMOD_DIR)")?;
    let mount = gm_content::Mount::for_install(&inst)?;
    let map = scn.map.as_deref().unwrap_or("gm_construct");
    let data = mount.read(&format!("maps/{map}.bsp"))?;
    let bsp = gm_formats::bsp::Bsp::parse(&data)?;
    let world = gm_collision::World::from_bsp(&bsp);
    let start = Vec3::from(scn.start.unwrap_or([0.0; 3]));
    let eye = start + Vec3::new(0.0, 0.0, 64.0);
    let player = 1u32;
    let mut sb = Sandbox::new(Limits::default());
    // props at rest where the scenario put them (the probe lets them settle before clicking)
    let mut bodies = Vec::new();
    for (i, pr) in scn.props.iter().enumerate() {
        bodies.push(PropBody {
            id: i as u32 + 1,
            pos: Vec3::from(pr.pos),
            angles: Vec3::from(pr.angles.unwrap_or([0.0; 3])),
            hulls: model_hull_planes(&mount, &pr.model)?,
        });
    }
    sb.place_existing(
        player,
        &bodies,
        &scn.props
            .iter()
            .map(|p| p.model.clone())
            .collect::<Vec<_>>(),
    );
    for (i, pr) in scn.props.iter().enumerate() {
        sb.set_frozen(i as u32 + 1, pr.frozen);
    }
    let entref = |e: Option<u32>| -> Value {
        match e {
            None => json!({ "index": 0, "class": "worldspawn" }),
            Some(id) => json!({ "prop": id, "class": "prop_physics" }),
        }
    };
    let snapshot = |sb: &Sandbox| -> Value {
        let constraints: Vec<Value> = sb
            .constraints()
            .iter()
            .map(|c| match &c.kind {
                ConstraintKind::Weld { forcelimit, nocollide } => json!({
                    "class": "phys_constraint", "type": "Weld", "ent1": entref(c.ent1), "ent2": entref(c.ent2),
                    "forcelimit": forcelimit, "nocollide": nocollide,
                }),
                ConstraintKind::Axis { lpos1, lpos2, local_axis, forcelimit, torquelimit, friction, nocollide } => json!({
                    "class": "phys_hinge", "type": "Axis", "ent1": entref(c.ent1), "ent2": entref(c.ent2),
                    "lpos1": [lpos1.x, lpos1.y, lpos1.z], "lpos2": [lpos2.x, lpos2.y, lpos2.z], "local_axis": [local_axis.x, local_axis.y, local_axis.z],
                    "forcelimit": forcelimit, "torquelimit": torquelimit, "friction": friction, "nocollide": nocollide,
                }),
                ConstraintKind::NoCollide => json!({
                    "class": "logic_collision_pair", "type": "NoCollide", "ent1": entref(c.ent1), "ent2": entref(c.ent2),
                }),
                ConstraintKind::Rope { lpos1, lpos2, length, addlength, width, material, rigid } => json!({
                    "class": "phys_lengthconstraint", "type": "Rope", "ent1": entref(c.ent1), "ent2": entref(c.ent2),
                    "lpos1": [lpos1.x, lpos1.y, lpos1.z], "lpos2": [lpos2.x, lpos2.y, lpos2.z],
                    "length": length, "addlength": addlength, "width": width, "material": material, "rigid": rigid,
                }),
            })
            .collect();
        json!({
            "constraints": constraints,
            "undos": sb.undos(player).iter().map(|u| json!({ "id": u.id, "name": u.name, "text": u.text, "custom": u.custom, "n": u.entities.len() })).collect::<Vec<_>>(),
            "counts": { "constraints": sb.count_of(player, "constraints"), "ropeconstraints": sb.count_of(player, "ropeconstraints"), "props": sb.count(player) },
            "stage": sb.tool(player).stage,
            "props": sb.props().iter().map(|p| json!({ "pos": [p.pos.x, p.pos.y, p.pos.z], "ang": [p.angles.x, p.angles.y, p.angles.z], "frozen": p.frozen })).collect::<Vec<_>>(),
        })
    };
    let golden = golden_trace(&scn.name);
    let v3 = |v: &Value| -> Vec3 {
        let a = v.as_array().cloned().unwrap_or_default();
        Vec3::new(
            a.first().and_then(Value::as_f64).unwrap_or(0.0) as f32,
            a.get(1).and_then(Value::as_f64).unwrap_or(0.0) as f32,
            a.get(2).and_then(Value::as_f64).unwrap_or(0.0) as f32,
        )
    };
    let mut results = Vec::new();
    let mut messages = Vec::new();
    for (i, op) in scn.ops.iter().enumerate() {
        let mut events = Vec::new();
        let mut rec = json!({ "op": op.op, "t": i });
        // where the game had the props right before this op (they drift and tumble under physics)
        if let Some(before) = golden
            .as_ref()
            .map(|g| &g["result"]["results"][i]["before"])
        {
            let ids: Vec<u32> = sb.props().iter().map(|p| p.id).collect();
            for (id, p) in ids.iter().zip(before.as_array().unwrap_or(&Vec::new())) {
                if p["pos"].is_array() {
                    sb.set_prop_pose(*id, v3(&p["pos"]), v3(&p["ang"]));
                }
            }
        }
        let poses_before: Vec<(Vec3, Vec3)> =
            sb.props().iter().map(|p| (p.pos, p.angles)).collect();
        match op.op.as_str() {
            "click" => {
                for b in bodies.iter_mut() {
                    if let Some(p) = sb.props().iter().find(|p| p.id == b.id) {
                        b.pos = p.pos;
                        b.angles = p.angles;
                    }
                }
                let dir = gm_sandbox::forward(op.pitch, op.yaw);
                let hit = gm_sandbox::tool_trace(&world, &bodies, eye, dir);
                let tool = op.tool.clone().unwrap_or_default();
                rec["tool"] = json!(tool);
                rec["button"] = json!(op.button.clone().unwrap_or_else(|| "left".into()));
                match hit {
                    Some(h) => {
                        rec["trace"] = json!({ "hit": true, "pos": [h.pos.x, h.pos.y, h.pos.z], "normal": [h.normal.x, h.normal.y, h.normal.z], "ent": entref(h.ent), "eye": [eye.x, eye.y, eye.z], "fraction": h.fraction });
                        let ret = match op.button.as_deref() {
                            Some("reload") => sb.tool_reload(player),
                            Some("right") => sb.tool_right_click(player, &tool, &h, &mut events),
                            _ => sb.tool_left_click(player, &tool, &h, &mut events),
                        };
                        rec["ok"] = json!(true);
                        rec["ret"] = json!(ret);
                    }
                    None => {
                        rec["trace"] = json!({ "hit": false });
                        rec["ok"] = json!(false);
                        rec["ret"] = json!("miss");
                    }
                }
            }
            "undo" => sb.undo(player, &mut events),
            "cleanup" => sb.cleanup(player, &mut events),
            "cvar" => match op.name.as_deref() {
                Some("sbox_maxconstraints") => {
                    sb.limits.max_constraints =
                        op.value.as_deref().unwrap_or("100").parse().unwrap_or(100)
                }
                Some("sbox_maxropeconstraints") => {
                    sb.limits.max_ropeconstraints =
                        op.value.as_deref().unwrap_or("100").parse().unwrap_or(100)
                }
                Some("sbox_maxprops") => {
                    sb.limits.max_props =
                        op.value.as_deref().unwrap_or("200").parse().unwrap_or(200)
                }
                _ => {}
            },
            "wait" => {}
            other => bail!("unknown toolgun op {other}"),
        }
        for e in &events {
            match e {
                Event::LimitHit { limit } => messages.push(json!({ "t": i, "limit": limit })),
                Event::Cleanup { kind } => messages
                    .push(json!({ "t": i, "lua": format!("hook.Run('OnCleanup','{kind}')") })),
                Event::Spawned { .. } | Event::Undone { .. } => {}
            }
        }
        let moved: Vec<usize> = sb
            .props()
            .iter()
            .enumerate()
            .filter(|(k, p)| {
                poses_before
                    .get(*k)
                    .is_none_or(|(pp, pa)| *pp != p.pos || *pa != p.angles)
            })
            .map(|(k, _)| k)
            .collect();
        rec["moved"] = json!(moved);
        let snap = snapshot(&sb);
        rec["immediate"] = snap.clone();
        rec["after"] = snap;
        results.push(rec);
    }
    Ok(json!({ "results": results, "messages": messages, "eye": [eye.x, eye.y, eye.z] }))
}

/// Per op: the tool trace (hit point and entity), the tool stage, counts, undo entries by id and
/// name, and every constraint (type, both ends, local attachment points, length).
fn compare_toolgun(theirs: &Value, ours: &Value, tol: &toml::Table) -> bool {
    let pos_tol = tol.get("pos").and_then(|v| v.as_float()).unwrap_or(0.1) as f32;
    let empty = Vec::new();
    let tr = theirs["results"].as_array().unwrap_or(&empty);
    let or = ours["results"].as_array().unwrap_or(&empty);
    let mut ok = tr.len() == or.len();
    if !ok {
        println!("  {} ops recorded, {} ours", tr.len(), or.len());
    }
    let v3 = |v: &Value| -> glam::Vec3 {
        let a = v.as_array().cloned().unwrap_or_default();
        glam::Vec3::new(
            a.first().and_then(Value::as_f64).unwrap_or(0.0) as f32,
            a.get(1).and_then(Value::as_f64).unwrap_or(0.0) as f32,
            a.get(2).and_then(Value::as_f64).unwrap_or(0.0) as f32,
        )
    };
    let ent_key = |e: &Value| -> String {
        if e["index"].as_i64() == Some(0) {
            "world".into()
        } else {
            format!("prop{}", e["prop"].as_i64().unwrap_or(-1))
        }
    };
    let (mut max_trace, mut max_lpos, mut max_len) = (0.0f32, 0.0f32, 0.0f32);
    for (i, (t, o)) in tr.iter().zip(or).enumerate() {
        let op = t["op"].as_str().unwrap_or("");
        if op == "click" {
            let (tt, ot) = (&t["trace"], &o["trace"]);
            if tt["hit"] != ot["hit"]
                || (tt["hit"] == json!(true) && ent_key(&tt["ent"]) != ent_key(&ot["ent"]))
            {
                println!(
                    "  op {i}: trace hit {} {} vs {} {}",
                    tt["hit"],
                    ent_key(&tt["ent"]),
                    ot["hit"],
                    ent_key(&ot["ent"])
                );
                ok = false;
            } else if tt["hit"] == json!(true) {
                max_trace = max_trace.max((v3(&tt["pos"]) - v3(&ot["pos"])).length());
            }
            let ret = |v: &Value| {
                v.as_str()
                    .map(str::to_string)
                    .unwrap_or_else(|| v.to_string())
            };
            if ret(&t["ret"]) != ret(&o["ret"]) {
                println!("  op {i}: tool returned {} vs {}", t["ret"], o["ret"]);
                ok = false;
            }
        }
        let (ta, oa) = (&t["after"], &o["after"]);
        for k in o["moved"]
            .as_array()
            .unwrap_or(&empty)
            .iter()
            .filter_map(Value::as_u64)
        {
            let (tp, op_) = (
                &t["after"]["props"][k as usize],
                &o["immediate"]["props"][k as usize],
            );
            let dp = (v3(&tp["pos"]) - v3(&op_["pos"])).length();
            let da = gm_sandbox::rotation(v3(&tp["ang"]))
                .angle_between(gm_sandbox::rotation(v3(&op_["ang"])))
                .to_degrees();
            println!("  op {i}: prop {} placed by the tool, pos err {dp:.3}, angle err {da:.2}, frozen {} vs {}", k + 1, tp["frozen"], op_["frozen"]);
            if dp > pos_tol || da > 0.5 || tp["frozen"] != op_["frozen"] {
                ok = false;
            }
        }
        if ta["stage"] != oa["stage"] {
            println!("  op {i} {op}: stage {} vs {}", ta["stage"], oa["stage"]);
            ok = false;
        }
        for k in ["constraints", "ropeconstraints"] {
            if ta["counts"][k] != oa["counts"][k] {
                println!(
                    "  op {i} {op}: count {k} {} vs {}",
                    ta["counts"][k], oa["counts"][k]
                );
                ok = false;
            }
        }
        let undos = |s: &Value| -> Vec<(i64, String)> {
            s["undos"]
                .as_array()
                .unwrap_or(&empty)
                .iter()
                .map(|u| {
                    (
                        u["id"].as_i64().unwrap_or(-1),
                        u["name"].as_str().unwrap_or("").to_string(),
                    )
                })
                .collect()
        };
        if undos(ta) != undos(oa) {
            println!("  op {i} {op}: undos {:?} vs {:?}", undos(ta), undos(oa));
            ok = false;
        }
        // the game lists constraints by class, ours by creation: pair them by type and ends
        let sorted = |s: &Value| -> Vec<Value> {
            let mut v = s["constraints"].as_array().cloned().unwrap_or_default();
            v.sort_by_key(|c| {
                format!(
                    "{} {} {}",
                    c["type"],
                    ent_key(&c["ent1"]),
                    ent_key(&c["ent2"])
                )
            });
            v
        };
        let (tc, oc) = (sorted(ta), sorted(oa));
        let (tc, oc) = (&tc, &oc);
        if tc.len() != oc.len() {
            println!(
                "  op {i} {op}: {} constraints in the game, {} ours",
                tc.len(),
                oc.len()
            );
            ok = false;
            continue;
        }
        for (a, b) in tc.iter().zip(oc) {
            if a["type"] != b["type"]
                || ent_key(&a["ent1"]) != ent_key(&b["ent1"])
                || ent_key(&a["ent2"]) != ent_key(&b["ent2"])
            {
                println!(
                    "  op {i} {op}: constraint {} {}-{} vs {} {}-{}",
                    a["type"],
                    ent_key(&a["ent1"]),
                    ent_key(&a["ent2"]),
                    b["type"],
                    ent_key(&b["ent1"]),
                    ent_key(&b["ent2"])
                );
                ok = false;
            }
            if a["type"] == json!("Axis") {
                max_lpos = max_lpos
                    .max((v3(&a["lpos1"]) - v3(&b["lpos1"])).length())
                    .max((v3(&a["lpos2"]) - v3(&b["lpos2"])).length());
            }
            if a["type"] == json!("Rope") {
                max_lpos = max_lpos
                    .max((v3(&a["lpos1"]) - v3(&b["lpos1"])).length())
                    .max((v3(&a["lpos2"]) - v3(&b["lpos2"])).length());
                max_len = max_len.max(
                    (a["length"].as_f64().unwrap_or(0.0) - b["length"].as_f64().unwrap_or(0.0))
                        .abs() as f32,
                );
                let num = |v: &Value| v.as_f64().unwrap_or(0.0);
                if a["rigid"] != b["rigid"] || num(&a["addlength"]) != num(&b["addlength"]) {
                    println!(
                        "  op {i}: rope settings {} {} vs {} {}",
                        a["rigid"], a["addlength"], b["rigid"], b["addlength"]
                    );
                    ok = false;
                }
            }
        }
    }
    if max_trace > pos_tol || max_lpos > pos_tol || max_len > pos_tol {
        ok = false;
    }
    println!(
        "  {} ops, max trace err {:.3}, max attachment err {:.3}, max length err {:.3} (tol {pos_tol}): {}",
        tr.len(), max_trace, max_lpos, max_len, if ok { "equal" } else { "DIFF" }
    );
    ok
}

/// Per op: spawn placement (pose at the spawn tick, before physics), prop list after the op
/// (models in order), count, undo ids and the limit hits sent to the player.
fn compare_sandbox(theirs: &Value, ours: &Value, tol: &toml::Table) -> bool {
    let pos_tol = tol.get("pos").and_then(|v| v.as_float()).unwrap_or(0.05) as f32;
    let ang_tol = tol.get("ang").and_then(|v| v.as_float()).unwrap_or(0.1) as f32;
    let empty = Vec::new();
    let tr = theirs["results"].as_array().unwrap_or(&empty);
    let or = ours["results"].as_array().unwrap_or(&empty);
    let mut ok = tr.len() == or.len();
    if !ok {
        println!("  {} ops recorded, {} ours", tr.len(), or.len());
    }
    let v3 = |v: &Value| -> glam::Vec3 {
        let a = v.as_array().cloned().unwrap_or_default();
        glam::Vec3::new(
            a.first().and_then(Value::as_f64).unwrap_or(0.0) as f32,
            a.get(1).and_then(Value::as_f64).unwrap_or(0.0) as f32,
            a.get(2).and_then(Value::as_f64).unwrap_or(0.0) as f32,
        )
    };
    let yaw_diff = |a: f32, b: f32| ((a - b + 540.0).rem_euclid(360.0) - 180.0).abs();
    let (mut max_pos, mut max_ang) = (0.0f32, 0.0f32);
    for (i, (t, o)) in tr.iter().zip(or).enumerate() {
        let op = t["op"].as_str().unwrap_or("");
        let (tsnap, osnap) = (&t["after"], &o["after"]);
        let tprops = t["immediate"]["props"].as_array().unwrap_or(&empty);
        let oprops = o["immediate"]["props"].as_array().unwrap_or(&empty);
        if op == "spawn" {
            let (tt, ot) = (&t["trace"], &o["trace"]);
            let placed = |v: &Value| v["placed"].as_i64().unwrap_or(0);
            if tt["ent"] != ot["ent"] || placed(tt) != placed(ot) {
                println!(
                    "  op {i}: spawn trace hit {} {} vs {} {}",
                    tt["ent"], tt["placed"], ot["ent"], ot["placed"]
                );
                ok = false;
            }
            if tprops.len() != oprops.len() {
                println!(
                    "  op {i} {op}: {} props in the game, {} ours",
                    tprops.len(),
                    oprops.len()
                );
                ok = false;
            } else {
                let prev = i
                    .checked_sub(1)
                    .map(|j| tr[j]["after"]["props"].as_array().map_or(0, Vec::len))
                    .unwrap_or(0);
                // only the prop this op placed is compared: older ones have been under physics
                let pairs = if tprops.len() > prev {
                    tprops.iter().zip(oprops).skip(tprops.len() - 1)
                } else {
                    tprops.iter().zip(oprops).skip(tprops.len())
                };
                for (tp, op_) in pairs {
                    let dp = (v3(&tp["pos"]) - v3(&op_["pos"])).length();
                    let da = yaw_diff(v3(&tp["ang"]).y, v3(&op_["ang"]).y)
                        .max(v3(&tp["ang"]).x.abs() - v3(&op_["ang"]).x.abs())
                        .abs();
                    max_pos = max_pos.max(dp);
                    max_ang = max_ang.max(da);
                    if tp["model"] != op_["model"] {
                        println!("  op {i}: model {} vs {}", tp["model"], op_["model"]);
                        ok = false;
                    }
                }
            }
        }
        let tafter = tsnap["props"].as_array().unwrap_or(&empty);
        let oafter = osnap["props"].as_array().unwrap_or(&empty);
        if tafter.len() != oafter.len() {
            println!(
                "  op {i} {op}: after {} props in the game, {} ours",
                tafter.len(),
                oafter.len()
            );
            ok = false;
        }
        if tsnap["count"] != osnap["count"] {
            println!(
                "  op {i} {op}: count {} vs {}",
                tsnap["count"], osnap["count"]
            );
            ok = false;
        }
        let ids = |s: &Value| -> Vec<i64> {
            s["undos"]
                .as_array()
                .unwrap_or(&empty)
                .iter()
                .filter_map(|u| u["id"].as_i64())
                .collect()
        };
        if ids(tsnap) != ids(osnap) {
            println!(
                "  op {i} {op}: undo ids {:?} vs {:?}",
                ids(tsnap),
                ids(osnap)
            );
            ok = false;
        }
        let tick = t["t"].as_i64().unwrap_or(-1);
        let limits = |m: &Value, key: i64, by_tick: bool| -> Vec<String> {
            m["messages"]
                .as_array()
                .unwrap_or(&empty)
                .iter()
                .filter(|m| m["t"].as_i64() == Some(key) && m.get("limit").is_some())
                .map(|m| m["limit"].as_str().unwrap_or("").to_string())
                .filter(|_| by_tick || true)
                .collect()
        };
        let (tl, ol) = (limits(theirs, tick, true), limits(ours, i as i64, false));
        if tl != ol {
            println!("  op {i} {op}: limit hits {tl:?} vs {ol:?}");
            ok = false;
        }
    }
    if max_pos > pos_tol || max_ang > ang_tol {
        ok = false;
    }
    println!(
        "  {} ops, max spawn pos err {:.3} (tol {pos_tol}), max angle err {:.2} (tol {ang_tol}): {}",
        tr.len(), max_pos, max_ang, if ok { "equal" } else { "DIFF" }
    );
    ok
}

fn compare_rigid(theirs: &Value, ours: &Value, tol: &toml::Table) -> bool {
    let first = theirs
        .get("ticks")
        .and_then(Value::as_array)
        .and_then(|a| a.first());
    if let Some(n) = first
        .and_then(|f| f.get("props"))
        .and_then(Value::as_array)
        .map(Vec::len)
    {
        let split = |v: &Value, i: usize| -> Value {
            let rows: Vec<Value> = v
                .get("ticks")
                .and_then(Value::as_array)
                .map(|a| {
                    a.iter()
                        .filter_map(|r| {
                            r.get("props")
                                .and_then(Value::as_array)
                                .and_then(|p| p.get(i))
                                .cloned()
                        })
                        .collect()
                })
                .unwrap_or_default();
            serde_json::json!({ "ticks": rows })
        };
        let mut ok = true;
        for i in 0..n {
            println!("  prop {}", i + 1);
            ok &= compare_rigid_one(&split(theirs, i), &split(ours, i), tol);
        }
        return ok;
    }
    compare_rigid_one(theirs, ours, tol)
}

fn compare_rigid_one(theirs: &Value, ours: &Value, tol: &toml::Table) -> bool {
    let pos_tol = tol.get("pos").and_then(|v| v.as_float()).unwrap_or(2.0) as f32;
    let rest_tol = tol.get("rest").and_then(|v| v.as_float()).unwrap_or(0.5) as f32;
    let ang_tol = tol.get("ang").and_then(|v| v.as_float()).unwrap_or(5.0) as f32;
    let contact_tol = tol.get("contact").and_then(|v| v.as_integer()).unwrap_or(1);
    let empty = Vec::new();
    let a = theirs
        .get("ticks")
        .and_then(Value::as_array)
        .unwrap_or(&empty);
    let b = ours
        .get("ticks")
        .and_then(Value::as_array)
        .unwrap_or(&empty);
    let n = a.len().min(b.len());
    let v3 = |v: &Value, k: &str| -> [f32; 3] {
        let arr = v.get(k).and_then(Value::as_array);
        let f = |i: usize| {
            arr.and_then(|a| a.get(i))
                .and_then(Value::as_f64)
                .unwrap_or(0.0) as f32
        };
        [f(0), f(1), f(2)]
    };
    // first tick where the vertical speed stops following free fall
    let contact = |t: &[Value]| -> Option<usize> {
        (1..t.len()).find(|&i| {
            let dz = v3(&t[i], "vel")[2] - v3(&t[i - 1], "vel")[2];
            (dz + 9.0).abs() > 2.5
        })
    };
    let (ca, cb) = (contact(a), contact(b));
    let ang_err = |x: f32, y: f32| ((x - y + 540.0).rem_euclid(360.0) - 180.0).abs();
    let mut max_pos = 0f32;
    let mut first_bad = None;
    for i in 0..n {
        let (pa, pb) = (v3(&a[i], "pos"), v3(&b[i], "pos"));
        let dp = (0..3).map(|k| (pa[k] - pb[k]).abs()).fold(0f32, f32::max);
        max_pos = max_pos.max(dp);
        if first_bad.is_none() && dp > pos_tol {
            first_bad = Some(i);
            println!("  first divergence at tick {i}: theirs pos {pa:?}; ours pos {pb:?}");
        }
    }
    let (ra, rb) = (&a[n.saturating_sub(1)], &b[n.saturating_sub(1)]);
    let (pa, pb) = (v3(ra, "pos"), v3(rb, "pos"));
    let rest_err = (0..3).map(|k| (pa[k] - pb[k]).abs()).fold(0f32, f32::max);
    let (aa, ab) = (v3(ra, "ang"), v3(rb, "ang"));
    let rest_ang = (0..3).map(|k| ang_err(aa[k], ab[k])).fold(0f32, f32::max);
    let contact_ok = match (ca, cb) {
        (Some(x), Some(y)) => (x as i64 - y as i64).abs() <= contact_tol,
        (None, None) => true,
        _ => false,
    };
    let ok = first_bad.is_none()
        && a.len() == b.len()
        && rest_err <= rest_tol
        && rest_ang <= ang_tol
        && contact_ok;
    println!(
        "  {n} ticks, contact tick theirs {ca:?} ours {cb:?}, max pos err {max_pos:.3} (tol {pos_tol}), rest err {rest_err:.3} (tol {rest_tol}), rest angle err {rest_ang:.2} (tol {ang_tol}): {}",
        if ok { "equal" } else { "DIFF" }
    );
    ok
}

fn compare_trajectory(theirs: &Value, ours: &Value, tol: &toml::Table) -> bool {
    let pos_tol = tol.get("pos").and_then(|v| v.as_float()).unwrap_or(0.05) as f32;
    let vel_tol = tol.get("vel").and_then(|v| v.as_float()).unwrap_or(0.5) as f32;
    let empty = Vec::new();
    let a = theirs
        .get("ticks")
        .and_then(Value::as_array)
        .unwrap_or(&empty);
    let b = ours
        .get("ticks")
        .and_then(Value::as_array)
        .unwrap_or(&empty);
    let n = a.len().min(b.len());
    if a.len() != b.len() {
        println!("  tick count theirs {} ours {}", a.len(), b.len());
    }
    let v3 = |v: &Value, k: &str| -> [f32; 3] {
        let arr = v.get(k).and_then(Value::as_array);
        let f = |i: usize| {
            arr.and_then(|a| a.get(i))
                .and_then(Value::as_f64)
                .unwrap_or(0.0) as f32
        };
        [f(0), f(1), f(2)]
    };
    let mut max_pos = 0f32;
    let mut max_vel = 0f32;
    let mut first_bad: Option<usize> = None;
    let mut ground_mismatch = 0;
    for i in 0..n {
        let (pa, pb) = (v3(&a[i], "pos"), v3(&b[i], "pos"));
        let (va, vb) = (v3(&a[i], "vel"), v3(&b[i], "vel"));
        let dp = (0..3).map(|k| (pa[k] - pb[k]).abs()).fold(0f32, f32::max);
        let dv = (0..3).map(|k| (va[k] - vb[k]).abs()).fold(0f32, f32::max);
        max_pos = max_pos.max(dp);
        max_vel = max_vel.max(dv);
        let ga = a[i].get("ground").and_then(Value::as_bool);
        let gb = b[i].get("ground").and_then(Value::as_bool);
        if ga != gb {
            ground_mismatch += 1;
        }
        if first_bad.is_none() && (dp > pos_tol || dv > vel_tol || ga != gb) {
            first_bad = Some(i);
            println!(
                "  first divergence at tick {i}: theirs pos {pa:?} vel {va:?} ground {ga:?}; ours pos {pb:?} vel {vb:?} ground {gb:?}"
            );
        }
    }
    let ok = first_bad.is_none() && a.len() == b.len();
    println!(
        "  {n} ticks compared, max pos err {max_pos:.4} (tol {pos_tol}), max vel err {max_vel:.3} (tol {vel_tol}), ground mismatches {ground_mismatch}: {}",
        if ok { "equal" } else { "DIFF" }
    );
    ok
}

#[derive(Default)]
struct Report {
    passed: usize,
    failed: usize,
    skipped: usize,
}

fn compare_set_equal(name: &str, theirs: &Value, ours: &Value) -> bool {
    let (Some(t), Some(o)) = (theirs.as_object(), ours.as_object()) else {
        println!("  {name}: traces are not objects");
        return false;
    };
    let mut ok = true;
    for (glob, tv) in t {
        let tset: BTreeSet<String> = tv
            .as_array()
            .map(|a| {
                a.iter()
                    .filter_map(|v| v.as_str().map(str::to_lowercase))
                    .collect()
            })
            .unwrap_or_default();
        let oset: BTreeSet<String> = o
            .get(glob)
            .and_then(Value::as_array)
            .map(|a| {
                a.iter()
                    .filter_map(|v| v.as_str().map(str::to_lowercase))
                    .collect()
            })
            .unwrap_or_default();
        let missing: Vec<_> = tset.difference(&oset).collect();
        let extra: Vec<_> = oset.difference(&tset).collect();
        if missing.is_empty() && extra.is_empty() {
            println!("  {glob}: {} entries, equal", tset.len());
        } else {
            ok = false;
            println!(
                "  {glob}: theirs {} ours {}, missing {} extra {}",
                tset.len(),
                oset.len(),
                missing.len(),
                extra.len()
            );
            for m in missing.iter().take(10) {
                println!("    - {m}");
            }
            for e in extra.iter().take(10) {
                println!("    + {e}");
            }
        }
    }
    ok
}

fn main() -> Result<()> {
    let cli = Cli::parse();
    let manifest = load_manifest(&cli.manifest)?;
    match cli.cmd {
        Cmd::List { prefix } => {
            for h in manifest
                .harness
                .iter()
                .filter(|h| h.name.starts_with(&prefix))
            {
                println!(
                    "{:<40} {:<8} {}",
                    h.name,
                    h.realm.as_deref().unwrap_or("server"),
                    h.compare
                );
            }
        }
        Cmd::Stage { game, prefix } => {
            let dst = game.join("data/parity/scenarios");
            std::fs::create_dir_all(&dst)?;
            for h in manifest
                .harness
                .iter()
                .filter(|h| h.name.starts_with(&prefix))
            {
                let src = scenario_path(&cli.manifest, h);
                std::fs::copy(&src, dst.join(format!("{}.json", h.name)))
                    .with_context(|| format!("staging {}", src.display()))?;
                println!("staged {}", h.name);
            }
            std::fs::write(
                dst.join("zz_quit.json"),
                r#"{ "name": "quit", "kind": "quit" }"#,
            )?;
        }
        Cmd::RunOurs { out, prefix } => {
            std::fs::create_dir_all(&out)?;
            for h in manifest
                .harness
                .iter()
                .filter(|h| h.name.starts_with(&prefix))
            {
                let scn = load_scenario(&scenario_path(&cli.manifest, h))?;
                match run_ours(&scn) {
                    Ok(trace) => {
                        std::fs::write(
                            out.join(format!("{}.json", h.name)),
                            serde_json::to_vec_pretty(&trace)?,
                        )?;
                        println!("ours {}: ok", h.name);
                    }
                    Err(e) => println!("ours {}: skipped ({e})", h.name),
                }
            }
        }
        Cmd::Replay {
            scenario,
            from,
            ticks,
        } => replay_phys(&scenario, from, ticks)?,
        Cmd::Compare {
            theirs,
            ours,
            prefix,
        } => {
            let mut rep = Report::default();
            let mut by_name: BTreeMap<String, bool> = BTreeMap::new();
            for h in manifest
                .harness
                .iter()
                .filter(|h| h.name.starts_with(&prefix))
            {
                let tp = theirs.join(format!("{}.json", h.name));
                let op = ours.join(format!("{}.json", h.name));
                let (Ok(tt), Ok(ot)) = (std::fs::read_to_string(&tp), std::fs::read_to_string(&op))
                else {
                    println!(
                        "{}: skipped (missing {} or {})",
                        h.name,
                        tp.display(),
                        op.display()
                    );
                    rep.skipped += 1;
                    continue;
                };
                let t: Trace =
                    serde_json::from_str(&tt).with_context(|| tp.display().to_string())?;
                let o: Trace =
                    serde_json::from_str(&ot).with_context(|| op.display().to_string())?;
                if !t.ok {
                    println!("{}: probe error: {}", h.name, t.error.unwrap_or_default());
                    rep.failed += 1;
                    continue;
                }
                println!("{} [{}]", h.name, h.compare);
                let ok = match h.compare.as_str() {
                    "set-equal" => compare_set_equal(&h.name, &t.result, &o.result),
                    "trajectory" => compare_trajectory(&t.result, &o.result, &h.tolerance),
                    "rigid" => compare_rigid(&t.result, &o.result, &h.tolerance),
                    "sandbox" => compare_sandbox(&t.result, &o.result, &h.tolerance),
                    "player" => compare_player(&t.result, &o.result, &h.tolerance),
                    "weapon" => compare_weapon(&t.result, &o.result, &h.tolerance),
                    "fire" => compare_fire(&t.result, &o.result, &h.tolerance),
                    "hit" => compare_hit(&t.result, &o.result, &h.tolerance),
                    "damage" => compare_damage(&t.result, &o.result, &h.tolerance),
                    "ragdoll" => compare_ragdoll(&t.result, &o.result, &h.tolerance),
                    "net_player" => compare_net_player(&t.result, &o.result, &h.tolerance),
                    "server" => compare_server(&t.result, &o.result, &h.tolerance),
                    "toolgun" => compare_toolgun(&t.result, &o.result, &h.tolerance),
                    other => {
                        println!("  unknown compare mode {other}");
                        false
                    }
                };
                by_name.insert(h.name.clone(), ok);
                if ok {
                    rep.passed += 1;
                } else {
                    rep.failed += 1;
                }
            }
            println!(
                "\n{} passed, {} failed, {} skipped",
                rep.passed, rep.failed, rep.skipped
            );
            if rep.failed > 0 {
                std::process::exit(1);
            }
        }
    }
    Ok(())
}
