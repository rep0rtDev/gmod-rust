//! Addon runtime over the server tick. Every actor is its own Luau VM (own globals, GC, budget);
//! a tick has a parallel phase (all actors at once on their own threads, reading an immutable
//! snapshot of the world and queueing writes) and a serial phase (queues applied in addon
//! priority, then actor, then enqueue order; cancelable events run in priority order and the
//! first handler returning `false` cancels). An actor over its CPU budget 5 ticks in a row is
//! throttled to one run in two ticks; an actor that errors is restarted with its declared `state`.

pub mod actor;
pub mod api;
pub mod manifest;
pub mod package;
pub mod snapshot;

pub use actor::{Actor, ActorConfig, ActorStatus, QueuedWrite};
pub use manifest::Manifest;
pub use package::Package;
pub use snapshot::{EntitySnap, PlayerSnap, Snapshot};

use gm_server::Server;
use std::sync::Arc;
use std::time::Duration;

/// One addon's actor declaration (what a manifest will carry; loaded from `.gmr` later).
#[derive(Clone, Debug)]
pub struct ActorSpec {
    pub addon: String,
    pub name: String,
    /// Lower runs first in the serial phase (addon priority).
    pub priority: i32,
    pub source: String,
    pub config: ActorConfig,
}

pub struct Runtime {
    /// `server` (default) or `client`: which actors `load_addon` takes.
    pub realm: String,
    /// Where every loaded addon came from (directory or `.gmr`), for reloads by id.
    pub addon_paths: std::collections::BTreeMap<String, std::path::PathBuf>,
    actors: Vec<Actor>,
    pub tick: u64,
    pub log: Vec<String>,
}

/// What the serial phase did to the server this tick.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Applied {
    pub writes: usize,
    pub rejected: usize,
}

impl Runtime {
    pub fn new() -> Self {
        Runtime {
            realm: "server".into(),
            addon_paths: std::collections::BTreeMap::new(),
            actors: Vec::new(),
            tick: 0,
            log: Vec::new(),
        }
    }

    /// Loads every server actor of an addon: a directory with `manifest.toml` or a `.gmr`.
    pub fn load_addon(
        &mut self,
        path: &std::path::Path,
    ) -> Result<Manifest, Box<dyn std::error::Error>> {
        let (m, specs) = read_addon(path, &self.realm)?;
        for spec in specs {
            self.load(spec)?;
        }
        self.addon_paths.insert(m.id.clone(), path.to_path_buf());
        Ok(m)
    }

    /// A client-realm runtime.
    pub fn client() -> Self {
        let mut rt = Runtime::new();
        rt.realm = "client".into();
        rt
    }

    /// Client realm: every actor sees the client's assembled world (parallel, read-only);
    /// `snapshot.received` when a new server tick arrived, then `tick.client` every frame.
    /// Writes are not applied on the client (the manifest never grants them there).
    pub fn client_tick(&mut self, world: &gm_net::ClientWorld, new_snapshot: bool) {
        let snap = Arc::new(Snapshot::of_client(world));
        self.tick = snap.tick;
        if new_snapshot {
            let mut p = api::Payload::default();
            p.ints.push(("tick", world.tick as i64));
            self.parallel(&snap, "snapshot.received", &p);
        }
        self.parallel(&snap, "tick.client", &api::Payload::default());
        self.release_snapshots();
        for a in self.actors.iter_mut() {
            a.take_queue();
        }
    }

    /// Hot reload of a loaded addon by id, from where it was loaded.
    pub fn reload_addon_by_id(&mut self, id: &str) -> Result<Manifest, Box<dyn std::error::Error>> {
        let path = self
            .addon_paths
            .get(id)
            .cloned()
            .ok_or_else(|| format!("addon {id} is not loaded"))?;
        self.reload_addon(&path)
    }

    /// Hot reload of an addon already loaded: actors keep their ids and `state`, get the new
    /// sources; actors added by the new manifest are loaded, removed ones dropped.
    pub fn reload_addon(
        &mut self,
        path: &std::path::Path,
    ) -> Result<Manifest, Box<dyn std::error::Error>> {
        let (m, specs) = read_addon(path, &self.realm)?;
        let mut keep: Vec<String> = Vec::new();
        for spec in specs {
            keep.push(spec.name.clone());
            match self
                .actors
                .iter_mut()
                .find(|a| a.spec.addon == spec.addon && a.spec.name == spec.name)
            {
                Some(a) => {
                    a.spec.priority = spec.priority;
                    a.spec.config = spec.config;
                    a.reload(spec.source)?;
                }
                None => {
                    self.load(spec)?;
                }
            }
        }
        self.actors
            .retain(|a| a.spec.addon != m.id || keep.contains(&a.spec.name));
        Ok(m)
    }

    pub fn load(&mut self, spec: ActorSpec) -> Result<usize, mlua::Error> {
        let id = self.actors.len() as u32;
        let actor = Actor::new(id, spec)?;
        self.actors.push(actor);
        Ok(id as usize)
    }

    pub fn actors(&self) -> &[Actor] {
        &self.actors
    }

    pub fn actors_mut(&mut self) -> &mut [Actor] {
        &mut self.actors
    }

    /// Every queued write in serial order (addon priority, actor id, enqueue order) without
    /// applying it.
    pub fn drain_writes(&mut self) -> Vec<(i32, u32, u64, QueuedWrite)> {
        let mut writes: Vec<(i32, u32, u64, QueuedWrite)> = Vec::new();
        for a in self.actors.iter_mut() {
            let p = a.priority();
            for (n, w) in a.take_queue() {
                writes.push((p, a.id(), n, w));
            }
        }
        writes.sort_by_key(|(p, id, n, _)| (*p, *id, *n));
        writes
    }

    fn snapshot(&self, server: &Server<'_>) -> Arc<Snapshot> {
        Arc::new(Snapshot::of(server, self.tick))
    }

    /// Parallel phase for one event (every actor on its own thread with the same snapshot), then
    /// the serial phase applying every queued write in order.
    pub fn run_event(
        &mut self,
        server: &mut Server<'_>,
        event: &str,
        payload: api::Payload,
    ) -> Applied {
        let snap = self.snapshot(server);
        self.parallel(&snap, event, &payload);
        self.serial(server)
    }

    /// Before the server tick: `tick.pre`.
    pub fn pre_tick(&mut self, server: &mut Server<'_>) -> Applied {
        self.tick += 1;
        self.run_event(server, "tick.pre", api::Payload::default())
    }

    /// After the server tick: `tick.post`.
    pub fn post_tick(&mut self, server: &mut Server<'_>) -> Applied {
        self.run_event(server, "tick.post", api::Payload::default())
    }

    /// Cancelable event in the serial phase: actors in priority order, the first `false` cancels.
    pub fn ask(&mut self, server: &mut Server<'_>, event: &str, payload: api::Payload) -> bool {
        let snap = self.snapshot(server);
        let mut order: Vec<usize> = (0..self.actors.len()).collect();
        order.sort_by_key(|&i| (self.actors[i].priority(), i));
        let mut allowed = true;
        for i in order {
            let a = &mut self.actors[i];
            if !a.may_run(self.tick) {
                continue;
            }
            match a.dispatch(&snap, event, &payload, self.tick) {
                Ok(Some(false)) => {
                    allowed = false;
                    break;
                }
                Ok(_) => {}
                Err(e) => self.log.push(format!("[{}] {e}", a.label())),
            }
        }
        self.serial(server);
        allowed
    }

    fn parallel(&mut self, snap: &Arc<Snapshot>, event: &str, payload: &api::Payload) {
        let tick = self.tick;
        let results: Vec<Result<Option<bool>, mlua::Error>> = std::thread::scope(|s| {
            let handles: Vec<_> = self
                .actors
                .iter_mut()
                .map(|a| {
                    let snap = snap.clone();
                    s.spawn(move || {
                        if a.may_run(tick) {
                            a.dispatch(&snap, event, payload, tick)
                        } else {
                            Ok(None)
                        }
                    })
                })
                .collect();
            handles
                .into_iter()
                .map(|h| {
                    h.join()
                        .unwrap_or_else(|_| Err(mlua::Error::runtime("actor thread panicked")))
                })
                .collect()
        });
        for (a, r) in self.actors.iter_mut().zip(results) {
            if let Err(e) = r {
                self.log.push(format!("[{}] {e}", a.label()));
            }
            for line in a.take_prints() {
                self.log.push(format!("[{}] {line}", a.label()));
            }
        }
    }

    /// After a phase: no actor keeps a snapshot that points into the server's world.
    fn release_snapshots(&mut self) {
        let empty = Arc::new(Snapshot::default());
        for a in self.actors.iter_mut() {
            a.set_snapshot(&empty);
        }
    }

    fn serial(&mut self, server: &mut Server<'_>) -> Applied {
        self.release_snapshots();
        let writes = self.drain_writes();
        let mut applied = Applied::default();
        for (_, _, _, w) in writes {
            if api::apply(server, &w) {
                applied.writes += 1;
            } else {
                applied.rejected += 1;
            }
        }
        applied
    }

    pub fn total_cpu(&self) -> Duration {
        self.actors.iter().map(|a| a.cpu_total()).sum()
    }
}

impl Default for Runtime {
    fn default() -> Self {
        Self::new()
    }
}

/// `ServerNet` hook: `prop.spawn_request` before `gm_spawn`, `entity.spawned` after.
impl Runtime {
    pub fn prop_spawn_request(
        &mut self,
        server: &mut Server<'_>,
        player: u32,
        model: &str,
    ) -> bool {
        let mut p = api::Payload::default();
        p.ints.push(("player", player as i64));
        p.strs.push(("model", model.to_string()));
        self.ask(server, "prop.spawn_request", p)
    }

    /// A click of a stool registered by a script (`tool.register`): the first actor in priority
    /// order that has it runs its handler; the returned ops are applied at once.
    pub fn tool_click(
        &mut self,
        server: &mut Server<'_>,
        player: u32,
        tool: &str,
        hit: &gm_sandbox::ToolHit,
        right: bool,
    ) -> bool {
        let snap = self.snapshot(server);
        let mut order: Vec<usize> = (0..self.actors.len()).collect();
        order.sort_by_key(|&i| (self.actors[i].priority(), i));
        let tg = server.sandbox.tool(player);
        let mut payload = api::Payload::default();
        payload.ints.push(("player", player as i64));
        payload.ints.push(("stage", tg.stage as i64));
        payload.hit = Some((hit.ent.unwrap_or(0), hit.pos, hit.normal));
        payload.objects = tg
            .objects
            .iter()
            .map(|o| {
                (
                    o.ent.unwrap_or(0),
                    server.sandbox.object_world_pos(o),
                    server.sandbox.object_world_normal(o),
                )
            })
            .collect();
        let button = if right { "right" } else { "left" };
        for i in order {
            if !self.actors[i].has_tool(tool) {
                continue;
            }
            let label = self.actors[i].label();
            let r = self.actors[i].dispatch_tool(&snap, tool, button, &payload);
            self.release_snapshots();
            return match r {
                Ok(ops) => api::apply_tool_ops(server, player, hit, &ops),
                Err(e) => {
                    self.log.push(format!("[{label}] {e}"));
                    false
                }
            };
        }
        false
    }

    pub fn entity_spawned(&mut self, server: &mut Server<'_>, player: u32, ent: u32) -> Applied {
        let mut p = api::Payload::default();
        p.ints.push(("player", player as i64));
        p.ints.push(("entity", ent as i64));
        self.run_event(server, "entity.spawned", p)
    }
}

fn read_addon(
    path: &std::path::Path,
    realm: &str,
) -> Result<(Manifest, Vec<ActorSpec>), manifest::ManifestError> {
    if path.is_dir() {
        Manifest::load_realm(path, realm)
    } else {
        Package::open(path)?.load_realm(realm)
    }
}

impl gm_server::ServerHooks for Runtime {
    fn prop_spawn_request(&mut self, server: &mut Server<'_>, player: u32, model: &str) -> bool {
        Runtime::prop_spawn_request(self, server, player, model)
    }
    fn entity_spawned(&mut self, server: &mut Server<'_>, player: u32, ent: u32) {
        Runtime::entity_spawned(self, server, player, ent);
    }
    fn tool_click(
        &mut self,
        server: &mut Server<'_>,
        player: u32,
        tool: &str,
        hit: &gm_sandbox::ToolHit,
        right: bool,
    ) -> bool {
        Runtime::tool_click(self, server, player, tool, hit, right)
    }
    fn reload_addon(&mut self, addon: &str) -> Result<String, String> {
        let before = self.actors.len();
        let m = Runtime::reload_addon_by_id(self, addon).map_err(|e| e.to_string())?;
        Ok(format!(
            "{} {} actors {} -> {}",
            m.id,
            m.version,
            before,
            self.actors.len()
        ))
    }
    fn pre_tick(&mut self, server: &mut Server<'_>) {
        Runtime::pre_tick(self, server);
    }
    fn post_tick(&mut self, server: &mut Server<'_>) {
        Runtime::post_tick(self, server);
    }
}
