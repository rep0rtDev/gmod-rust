//! Addon runtime over the server tick. Every actor is its own Luau VM (own globals, GC, budget);
//! a tick has a parallel phase (all actors at once on their own threads, reading an immutable
//! snapshot of the world and queueing writes) and a serial phase (queues applied in addon
//! priority, then actor, then enqueue order; cancelable events run in priority order and the
//! first handler returning `false` cancels). An actor over its CPU budget 5 ticks in a row is
//! throttled to one run in two ticks; an actor that errors is restarted with its declared `state`.

pub mod actor;
pub mod api;
pub mod depends;
pub mod manifest;
pub mod netval;
pub mod package;
pub mod pool;
pub mod registry;
pub mod snapshot;
pub mod storage;

pub use actor::{Actor, ActorConfig, ActorStatus, QueuedWrite};
pub use manifest::Manifest;
pub use package::Package;
pub use registry::Registry;
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
    /// Client-realm packages built by `load_addon`/`check_reloads` on the server, until the
    /// network takes them (`ServerHooks::drain_published`).
    pub published: Vec<gm_server::PublishedPackage>,
    actors: Vec<Actor>,
    pub tick: u64,
    pub log: Vec<String>,
    /// How many of `server.cvar_changes` were already dispatched as `cvar.changed`.
    cvar_seen: usize,
    /// Client realm: the replicated variables as last seen, for `cvar.changed`.
    cvars_last: std::collections::BTreeMap<String, String>,
    /// Where addon stores are kept (`<data_dir>/<realm>/<addon>.gms`); None = memory only.
    pub data_dir: Option<std::path::PathBuf>,
    stores: std::collections::BTreeMap<String, Arc<std::sync::Mutex<storage::Store>>>,
    /// Poll the loaded addons' files every 33 ticks and hot reload the changed ones.
    pub watch: bool,
    addon_stamps: std::collections::BTreeMap<String, u64>,
    /// Public keys addons must be signed by; empty means any addon loads (development).
    pub trusted: Vec<[u8; 32]>,
    /// Version of every loaded addon by id (what `depends` is checked against).
    pub addon_versions: std::collections::BTreeMap<String, String>,
    pool: pool::Pool,
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
            published: Vec::new(),
            actors: Vec::new(),
            tick: 0,
            log: Vec::new(),
            cvar_seen: 0,
            cvars_last: Default::default(),
            data_dir: None,
            stores: Default::default(),
            watch: false,
            addon_stamps: Default::default(),
            addon_versions: Default::default(),
            trusted: Vec::new(),
            pool: pool::Pool::new(
                std::thread::available_parallelism()
                    .map(|n| n.get())
                    .unwrap_or(4)
                    .min(16),
            ),
        }
    }

    /// Loads every server actor of an addon: a directory with `manifest.toml` or a `.gmr`.
    pub fn load_addon(
        &mut self,
        path: &std::path::Path,
    ) -> Result<Manifest, Box<dyn std::error::Error>> {
        let (m, specs) = read_addon(path, &self.realm)?;
        if self.addon_versions.contains_key(&m.id) {
            return Err(depends::DependsError::AlreadyLoaded(m.id).into());
        }
        self.check_trust(&m.id, path)?;
        depends::check(&m.id, &m.depends, &self.addon_versions)?;
        for spec in specs {
            self.load(spec)?;
        }
        self.addon_versions.insert(m.id.clone(), m.version.clone());
        self.addon_paths.insert(m.id.clone(), path.to_path_buf());
        self.addon_stamps.insert(m.id.clone(), file_stamp(path));
        self.publish(&m, path);
        Ok(m)
    }

    /// Loads a set of addons in dependency order (a dependency before what needs it, the given
    /// order otherwise); nothing is loaded when a dependency is missing, has the wrong version or
    /// the set has a cycle. Returns the manifests in load order.
    pub fn load_addons(
        &mut self,
        paths: &[std::path::PathBuf],
    ) -> Result<Vec<Manifest>, Box<dyn std::error::Error>> {
        let mut read = Vec::new();
        for p in paths {
            let (m, _) = read_addon(p, &self.realm)?;
            self.check_trust(&m.id, p)?;
            if self.addon_versions.contains_key(&m.id) || read.iter().any(|(id, _)| *id == m.id) {
                return Err(depends::DependsError::AlreadyLoaded(m.id).into());
            }
            read.push((m.id.clone(), m.depends.clone()));
        }
        let order = depends::order(&read)?;
        let mut versions = self.addon_versions.clone();
        for &i in &order {
            let (m, _) = read_addon(&paths[i], &self.realm)?;
            depends::check(&m.id, &m.depends, &versions)?;
            versions.insert(m.id, m.version);
        }
        let mut out = Vec::new();
        for i in order {
            out.push(self.load_addon(&paths[i])?);
        }
        Ok(out)
    }

    /// With `trusted` keys set, an addon (directory or `.gmr`) must carry a valid signature by
    /// one of them; a broken signature is refused even without a trust list.
    fn check_trust(&self, id: &str, path: &std::path::Path) -> Result<(), package::SignatureError> {
        let files = if path.is_dir() {
            let mut f = std::collections::BTreeMap::new();
            package::collect(path, path, &mut f).map_err(|e| {
                package::SignatureError::Malformed(format!("{}: {e}", path.display()))
            })?;
            f
        } else {
            Package::open(path)
                .map_err(|e| package::SignatureError::Malformed(e.to_string()))?
                .files
        };
        let signers = package::signers(&files)?;
        if self.trusted.is_empty() || signers.iter().any(|k| self.trusted.contains(k)) {
            Ok(())
        } else {
            Err(package::SignatureError::Untrusted(id.to_string()))
        }
    }

    /// On the server, an addon with the `client` realm is packed (a directory deterministically,
    /// a `.gmr` as is) and queued for the network to hand to clients.
    fn publish(&mut self, m: &Manifest, path: &std::path::Path) {
        if self.realm != "server" || !m.realms.iter().any(|r| r == "client") {
            return;
        }
        match Self::package_for_clients(m, path) {
            Ok(p) => self.published.push(p),
            Err(e) => self.log.push(format!("package {}: {e}", m.id)),
        }
    }

    /// The bytes clients get for this addon and their sha256 (the package identity).
    pub fn package_for_clients(
        m: &Manifest,
        path: &std::path::Path,
    ) -> Result<gm_server::PublishedPackage, Box<dyn std::error::Error>> {
        let bytes = if path.is_dir() {
            package::pack(path)?
        } else {
            std::fs::read(path)?
        };
        let pkg = Package::read(&bytes)?;
        Ok(gm_server::PublishedPackage {
            id: m.id.clone(),
            version: m.version.clone(),
            sha: pkg.sha256,
            bytes,
        })
    }

    /// Reloads every loaded addon whose files (directory tree or `.gmr`) changed since it was
    /// loaded or last checked: (id, Ok(manifest) or the error text, which is also logged; the
    /// actors keep running on their old sources after a failed reload).
    pub fn check_reloads(&mut self) -> Vec<(String, Result<Manifest, String>)> {
        let mut out = Vec::new();
        let ids: Vec<(String, std::path::PathBuf)> = self
            .addon_paths
            .iter()
            .map(|(id, p)| (id.clone(), p.clone()))
            .collect();
        for (id, path) in ids {
            let stamp = file_stamp(&path);
            if self.addon_stamps.get(&id) == Some(&stamp) {
                continue;
            }
            self.addon_stamps.insert(id.clone(), stamp);
            let r = self.reload_addon(&path).map_err(|e| e.to_string());
            match &r {
                Ok(m) => {
                    self.log.push(format!("reloaded {} {}", m.id, m.version));
                    self.publish(m, &path);
                }
                Err(e) => self.log.push(format!("reload {id} failed: {e}")),
            }
            out.push((id, r));
        }
        out
    }

    fn watch_tick(&mut self, server: Option<&mut Server<'_>>) {
        if !self.watch || !self.tick.is_multiple_of(33) {
            return;
        }
        let reloaded = self.check_reloads();
        let Some(server) = server else { return };
        for (id, r) in reloaded {
            let mut p = api::Payload::default();
            p.strs.push(("addon", id));
            p.strs.push((
                "version",
                r.as_ref().map(|m| m.version.clone()).unwrap_or_default(),
            ));
            p.strs.push(("error", r.err().unwrap_or_default()));
            self.run_event(server, "addon.reloaded", p);
        }
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
            self.watch_tick(None);
            let mut p = api::Payload::default();
            p.ints.push(("tick", world.tick as i64));
            self.parallel(&snap, "snapshot.received", &p);
        }
        if world.cvars != self.cvars_last {
            for (name, value) in &world.cvars {
                let old = self.cvars_last.get(name).cloned().unwrap_or_default();
                if old != *value {
                    let mut p = api::Payload::default();
                    p.strs.push(("name", name.clone()));
                    p.strs.push(("old", old));
                    p.strs.push(("value", value.clone()));
                    self.parallel(&snap, "cvar.changed", &p);
                }
            }
            self.cvars_last = world.cvars.clone();
        }
        self.parallel(&snap, "tick.client", &api::Payload::default());
        self.release_snapshots();
        for a in self.actors.iter_mut() {
            a.take_queue();
        }
    }

    /// Client realm: addon messages from the server, one `net.message` dispatch each (`name`,
    /// `data`), in arrival order.
    pub fn client_net(&mut self, world: &gm_net::ClientWorld, msgs: Vec<(String, Vec<u8>)>) {
        if msgs.is_empty() {
            return;
        }
        let snap = Arc::new(Snapshot::of_client(world));
        for (name, data) in msgs {
            let p = api::Payload {
                net: Some((name, data)),
                ..Default::default()
            };
            self.parallel(&snap, "net.message", &p);
        }
        self.release_snapshots();
        for a in self.actors.iter_mut() {
            a.take_queue();
        }
    }

    /// Addon messages the actors queued with `net.send` since the last drain, in actor order
    /// then send order: (player or 0, name, payload).
    pub fn drain_net(&mut self) -> Vec<(u32, String, Vec<u8>)> {
        let mut out = Vec::new();
        for a in self.actors.iter_mut() {
            out.extend(a.take_net());
        }
        out
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
        self.check_trust(&m.id, path)?;
        depends::check(&m.id, &m.depends, &self.addon_versions)?;
        let mut versions = self.addon_versions.clone();
        versions.insert(m.id.clone(), m.version.clone());
        for (id, p) in &self.addon_paths {
            if *id == m.id {
                continue;
            }
            let (dm, _) = read_addon(p, &self.realm)?;
            depends::check(&dm.id, &dm.depends, &versions)?;
        }
        self.addon_versions = versions;
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
        let store = spec.config.can_storage.then(|| self.store_of(&spec.addon));
        let actor = Actor::new(id, spec, store)?;
        self.actors.push(actor);
        Ok(id as usize)
    }

    fn store_path(&self, addon: &str) -> Option<std::path::PathBuf> {
        self.data_dir
            .as_ref()
            .map(|d| d.join(&self.realm).join(format!("{addon}.gms")))
    }

    /// The addon's store, loaded from disk the first time (a missing or corrupt file starts
    /// empty; a corrupt one is reported in `log`).
    fn store_of(&mut self, addon: &str) -> Arc<std::sync::Mutex<storage::Store>> {
        if let Some(s) = self.stores.get(addon) {
            return s.clone();
        }
        let store = match self.store_path(addon) {
            Some(p) if p.exists() => match storage::Store::load(&p) {
                Ok(s) => s,
                Err(e) => {
                    self.log.push(format!("storage {}: {e}", p.display()));
                    storage::Store::default()
                }
            },
            _ => storage::Store::default(),
        };
        let store = Arc::new(std::sync::Mutex::new(store));
        self.stores.insert(addon.to_string(), store.clone());
        store
    }

    /// Writes every dirty store to `data_dir`; the number written (0 without a data dir).
    pub fn flush_storage(&mut self) -> usize {
        let mut n = 0;
        let paths: Vec<_> = self
            .stores
            .keys()
            .map(|a| (a.clone(), self.store_path(a)))
            .collect();
        for (addon, path) in paths {
            let Some(path) = path else { continue };
            let mut st = self.stores[&addon].lock().unwrap();
            if st.dirty {
                match st.save(&path) {
                    Ok(()) => n += 1,
                    Err(e) => self.log.push(format!("storage {}: {e}", path.display())),
                }
            }
        }
        n
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
        self.watch_tick(Some(server));
        self.run_event(server, "tick.pre", api::Payload::default())
    }

    /// After the server tick: `tick.post`.
    pub fn post_tick(&mut self, server: &mut Server<'_>) -> Applied {
        self.dispatch_cvar_changes(server);
        let applied = self.run_event(server, "tick.post", api::Payload::default());
        self.dispatch_cvar_changes(server);
        if self.tick.is_multiple_of(67) {
            self.flush_storage();
        }
        applied
    }

    /// After the tick's commands: `cvar.changed` for the variables they set, still in this tick.
    pub fn after_commands(&mut self, server: &mut Server<'_>) {
        self.dispatch_cvar_changes(server);
    }

    /// `cvar.changed` (name, old, value) for every change logged on the server since the last
    /// call; the net layer takes the log after the hooks, which resets the cursor.
    fn dispatch_cvar_changes(&mut self, server: &mut Server<'_>) {
        if server.cvar_changes.len() < self.cvar_seen {
            self.cvar_seen = 0;
        }
        while self.cvar_seen < server.cvar_changes.len() {
            let (name, old, value) = server.cvar_changes[self.cvar_seen].clone();
            self.cvar_seen += 1;
            let mut p = api::Payload::default();
            p.strs.push(("name", name));
            p.strs.push(("old", old));
            p.strs.push(("value", value));
            self.run_event(server, "cvar.changed", p);
        }
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
        let mut results: Vec<Result<Option<bool>, mlua::Error>> = (0..self.actors.len())
            .map(|_| Err(mlua::Error::runtime("actor thread panicked")))
            .collect();
        if self.actors.len() == 1 {
            let a = &mut self.actors[0];
            results[0] = if a.may_run(tick) {
                a.dispatch(snap, event, payload, tick)
            } else {
                Ok(None)
            };
        } else {
            let jobs: Vec<Box<dyn FnOnce() + Send + '_>> = self
                .actors
                .iter_mut()
                .zip(results.iter_mut())
                .map(|(a, slot)| {
                    let snap = snap.clone();
                    Box::new(move || {
                        *slot = if a.may_run(tick) {
                            a.dispatch(&snap, event, payload, tick)
                        } else {
                            Ok(None)
                        };
                    }) as Box<dyn FnOnce() + Send + '_>
                })
                .collect();
            self.pool.run(jobs);
        }
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

    /// `entity.created` (the entity exists, like `OnEntityCreated`) then `entity.spawned` (the
    /// sandbox finished with it, like `PlayerSpawnedProp`), both serial.
    pub fn entity_spawned(&mut self, server: &mut Server<'_>, player: u32, ent: u32) -> Applied {
        let mut p = api::Payload::default();
        p.ints.push(("player", player as i64));
        p.ints.push(("entity", ent as i64));
        self.run_event(server, "entity.created", p.clone());
        self.run_event(server, "entity.spawned", p)
    }

    /// `entity.removed` (`EntityRemoved`): the entity is already gone from the table.
    pub fn entity_removed(&mut self, server: &mut Server<'_>, player: u32, ent: u32) -> Applied {
        let mut p = api::Payload::default();
        p.ints.push(("player", player as i64));
        p.ints.push(("entity", ent as i64));
        self.run_event(server, "entity.removed", p)
    }
}

/// A fingerprint of an addon's files: lengths and modification times of every file under a
/// directory (names included) or of the `.gmr` file.
fn file_stamp(path: &std::path::Path) -> u64 {
    use std::hash::{Hash, Hasher};
    fn visit(p: &std::path::Path, h: &mut std::collections::hash_map::DefaultHasher) {
        if p.is_dir() {
            let mut entries: Vec<_> = std::fs::read_dir(p)
                .map(|d| d.flatten().map(|e| e.path()).collect())
                .unwrap_or_default();
            entries.sort();
            for e in entries {
                visit(&e, h);
            }
        } else if let Ok(md) = std::fs::metadata(p) {
            p.hash(h);
            md.len().hash(h);
            if let Ok(t) = md.modified() {
                t.hash(h);
            }
        }
    }
    let mut h = std::collections::hash_map::DefaultHasher::new();
    visit(path, &mut h);
    h.finish()
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

impl Drop for Runtime {
    fn drop(&mut self) {
        self.flush_storage();
    }
}

impl gm_server::ServerHooks for Runtime {
    fn net_message(&mut self, server: &mut Server<'_>, player: u32, name: &str, data: &[u8]) {
        let mut p = api::Payload::default();
        p.ints.push(("player", player as i64));
        p.net = Some((name.to_string(), data.to_vec()));
        self.run_event(server, "net.message", p);
    }
    fn drain_net(&mut self) -> Vec<(u32, String, Vec<u8>)> {
        Runtime::drain_net(self)
    }
    fn drain_published(&mut self) -> Vec<gm_server::PublishedPackage> {
        std::mem::take(&mut self.published)
    }
    fn entity_removed(&mut self, server: &mut Server<'_>, player: u32, ent: u32) {
        Runtime::entity_removed(self, server, player, ent);
    }
    fn after_commands(&mut self, server: &mut Server<'_>) {
        Runtime::after_commands(self, server);
    }
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
