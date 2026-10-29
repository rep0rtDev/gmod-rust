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
