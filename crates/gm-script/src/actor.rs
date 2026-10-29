//! One actor: a Luau VM with a memory limit, a per-tick CPU budget enforced through the VM's
//! interrupt callback, throttling after repeated overruns and restart after an error.

use crate::api::{self, Payload, Shared};
use crate::snapshot::Snapshot;
use crate::ActorSpec;
use mlua::{Lua, Value, VmState};
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

pub use crate::api::QueuedWrite;

#[derive(Clone, Debug, PartialEq)]
pub struct ActorConfig {
    /// CPU per tick before the run is aborted (default 2 ms, servers may raise to 10).
    pub cpu_budget: Duration,
    /// Overruns in a row before throttling to one run in two ticks.
    pub overruns_to_throttle: u32,
    pub memory_limit: usize,
    pub can_write: bool,
}

impl Default for ActorConfig {
    fn default() -> Self {
        ActorConfig {
            cpu_budget: Duration::from_millis(2),
            overruns_to_throttle: 5,
            memory_limit: 256 << 20,
            can_write: true,
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ActorStatus {
    Running,
    Throttled,
}

pub struct Actor {
    id: u32,
    spec: ActorSpec,
    lua: Lua,
    shared: Arc<Shared>,
    deadline: Arc<Mutex<Option<Instant>>>,
    over_budget: Arc<AtomicBool>,
    overruns: u32,
    pub status: ActorStatus,
    pub restarts: u32,
    pub overruns_total: u64,
    cpu_total: Arc<AtomicU64>,
    saved_state: Option<String>,
}

impl Actor {
    pub fn new(id: u32, spec: ActorSpec) -> mlua::Result<Self> {
        let shared = Arc::new(Shared {
            snapshot: std::sync::RwLock::new(Arc::new(Snapshot::default())),
            queue: Mutex::new(Vec::new()),
            queued: Mutex::new(0),
            prints: Mutex::new(Vec::new()),
            can_write: spec.config.can_write,
        });
        let mut a = Actor {
            id,
            spec,
            lua: Lua::new(),
            shared,
            deadline: Arc::new(Mutex::new(None)),
            over_budget: Arc::new(AtomicBool::new(false)),
            overruns: 0,
            status: ActorStatus::Running,
            restarts: 0,
            overruns_total: 0,
            cpu_total: Arc::new(AtomicU64::new(0)),
            saved_state: None,
        };
        a.boot()?;
        Ok(a)
    }

    fn boot(&mut self) -> mlua::Result<()> {
        let lua = Lua::new();
        lua.set_memory_limit(self.spec.config.memory_limit)?;
        let deadline = self.deadline.clone();
        let over = self.over_budget.clone();
        lua.set_interrupt(move |_| {
            if let Some(d) = *deadline.lock().unwrap() {
                if Instant::now() > d {
                    over.store(true, Ordering::Relaxed);
                    return Err(mlua::Error::runtime("cpu budget exceeded"));
                }
            }
            Ok(VmState::Continue)
        });
        api::install(&lua, self.shared.clone())?;
        if let Some(saved) = &self.saved_state {
            lua.load(format!("state = {saved}"))
                .set_name("state")
                .exec()?;
        }
        lua.load(self.spec.source.as_str())
            .set_name(format!("{}/{}", self.spec.addon, self.spec.name))
            .exec()?;
        self.lua = lua;
        Ok(())
    }

    pub fn id(&self) -> u32 {
        self.id
    }

    pub fn priority(&self) -> i32 {
        self.spec.priority
    }

    pub fn label(&self) -> String {
        format!("{}/{}", self.spec.addon, self.spec.name)
    }

    pub fn cpu_total(&self) -> Duration {
        Duration::from_nanos(self.cpu_total.load(Ordering::Relaxed))
    }

    /// Throttled actors run every other tick.
    pub fn may_run(&self, tick: u64) -> bool {
        self.status == ActorStatus::Running || tick.is_multiple_of(2)
    }

    /// Runs the actor's handlers for `event` against `snap` within the budget; `Some(false)`
    /// means a handler cancelled. Errors restart the VM (keeping `state`).
    pub fn dispatch(
        &mut self,
        snap: &Arc<Snapshot>,
        event: &str,
        payload: &Payload,
        _tick: u64,
    ) -> mlua::Result<Option<bool>> {
        *self.shared.snapshot.write().unwrap() = snap.clone();
        let start = Instant::now();
        *self.deadline.lock().unwrap() = Some(start + self.spec.config.cpu_budget);
        self.over_budget.store(false, Ordering::Relaxed);
        let result: mlua::Result<Value> = (|| {
            let dispatch: mlua::Function = self.lua.globals().get("__dispatch")?;
            let p = api::payload_table(&self.lua, payload)?;
            dispatch.call((event, p))
        })();
        *self.deadline.lock().unwrap() = None;
        self.cpu_total
            .fetch_add(start.elapsed().as_nanos() as u64, Ordering::Relaxed);
        if self.over_budget.load(Ordering::Relaxed) {
            self.overruns += 1;
            self.overruns_total += 1;
            if self.overruns >= self.spec.config.overruns_to_throttle {
                self.status = ActorStatus::Throttled;
            }
        } else {
            self.overruns = 0;
            if self.status == ActorStatus::Throttled && result.is_ok() {
                self.status = ActorStatus::Running;
            }
        }
        match result {
            Ok(Value::Boolean(false)) => Ok(Some(false)),
            Ok(_) => Ok(None),
            Err(e) => {
                if !self.over_budget.load(Ordering::Relaxed) {
                    self.restart();
                }
                Err(e)
            }
        }
    }

    /// Serialises `state` (numbers, strings, booleans, nested tables) and boots a fresh VM.
    fn restart(&mut self) {
        self.saved_state = self.serialise_state();
        self.restarts += 1;
        if let Err(e) = self.boot() {
            self.shared
                .prints
                .lock()
                .unwrap()
                .push(format!("restart failed: {e}"));
        }
    }

    fn serialise_state(&self) -> Option<String> {
        let f: mlua::Function = self
            .lua
            .load(
                r#"
                local function ser(v, depth)
                    local t = type(v)
                    if t == "number" or t == "boolean" then return tostring(v) end
                    if t == "string" then return string.format("%q", v) end
                    if t == "table" and depth < 8 then
                        local parts = {}
                        for k, x in pairs(v) do
                            local ks = type(k) == "string" and string.format("[%q]", k) or ("[" .. tostring(k) .. "]")
                            local xs = ser(x, depth + 1)
                            if xs then table.insert(parts, ks .. "=" .. xs) end
                        end
                        return "{" .. table.concat(parts, ",") .. "}"
                    end
                    return nil
                end
                return function() return ser(state, 0) end
                "#,
            )
            .eval()
            .ok()?;
        f.call::<Option<String>>(()).ok().flatten()
    }

    pub fn take_queue(&mut self) -> Vec<(u64, QueuedWrite)> {
        std::mem::take(&mut *self.shared.queue.lock().unwrap())
    }

    pub fn take_prints(&mut self) -> Vec<String> {
        std::mem::take(&mut *self.shared.prints.lock().unwrap())
    }

    /// For tests: evaluate an expression in the actor's VM.
    pub fn eval<T: mlua::FromLuaMulti>(&self, code: &str) -> mlua::Result<T> {
        self.lua.load(code).eval()
    }
}
