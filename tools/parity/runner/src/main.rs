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
use serde_json::Value;

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
    angles: Option<[f32; 2]>,
    #[serde(default)]
    segments: Vec<Segment>,
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
    let view = scn.angles.unwrap_or([0.0, 0.0]);
    let mut st = PlayerState {
        origin: start,
        ..Default::default()
    };
    // settle onto the ground like the probe's pre-roll
    let settle_cmd = UserCmd {
        view_angles: Vec3::new(view[0], view[1], 0.0),
        ..Default::default()
    };
    for _ in 0..20 {
        mover.tick(&mut st, &settle_cmd);
        st.origin.x = start.x;
        st.origin.y = start.y;
        st.velocity = Vec3::ZERO;
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
