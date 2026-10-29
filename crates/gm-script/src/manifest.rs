//! `manifest.toml` of an addon directory: id, version, priority, capabilities and actors with
//! their Luau source files. A `.gmr` is this directory zipped (packaging comes later).

use crate::{ActorConfig, ActorSpec};
use serde::Deserialize;
use std::path::Path;
use std::time::Duration;

#[derive(Debug, Deserialize)]
pub struct Manifest {
    pub id: String,
    #[serde(default = "default_version")]
    pub version: String,
    #[serde(default)]
    pub priority: i32,
    #[serde(default)]
    pub realms: Vec<String>,
    #[serde(default)]
    pub capabilities: Vec<String>,
    #[serde(default)]
    pub actors: Vec<ActorEntry>,
    #[serde(default)]
    pub depends: Vec<String>,
}

fn default_version() -> String {
    "0.1.0".into()
}

#[derive(Debug, Deserialize)]
pub struct ActorEntry {
    pub name: String,
    pub source: String,
    #[serde(default = "default_realm")]
    pub realm: String,
    /// CPU budget per tick in milliseconds (server clamps to 10).
    #[serde(default)]
    pub cpu_ms: Option<f32>,
}

fn default_realm() -> String {
    "server".into()
}

#[derive(Debug, thiserror::Error)]
pub enum ManifestError {
    #[error("{0}")]
    Io(#[from] std::io::Error),
    #[error("manifest: {0}")]
    Toml(#[from] toml::de::Error),
    #[error("addon {0}: actor {1} source {2} is outside the addon directory")]
    Escape(String, String, String),
}

impl Manifest {
    /// The spec of one actor of this manifest with its source text.
    pub fn actor_spec(&self, a: &ActorEntry, source: String) -> ActorSpec {
        let can_write = self.capabilities.iter().any(|c| c == "world-write");
        let mut config = ActorConfig {
            can_write,
            ..Default::default()
        };
        if let Some(ms) = a.cpu_ms {
            config.cpu_budget = Duration::from_secs_f32(ms.clamp(0.1, 10.0) / 1000.0);
        }
        ActorSpec {
            addon: self.id.clone(),
            name: a.name.clone(),
            priority: self.priority,
            source,
            config,
        }
    }

    pub fn load(dir: &Path) -> Result<(Manifest, Vec<ActorSpec>), ManifestError> {
        let text = std::fs::read_to_string(dir.join("manifest.toml"))?;
        let m: Manifest = toml::from_str(&text)?;
        let mut specs = Vec::new();
        for a in &m.actors {
            if a.realm != "server" {
                continue;
            }
            if a.source.contains("..") || Path::new(&a.source).is_absolute() {
                return Err(ManifestError::Escape(
                    m.id.clone(),
                    a.name.clone(),
                    a.source.clone(),
                ));
            }
            let source = std::fs::read_to_string(dir.join(&a.source))?;
            specs.push(m.actor_spec(a, source));
        }
        Ok((m, specs))
    }
}
