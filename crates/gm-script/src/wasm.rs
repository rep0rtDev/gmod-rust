//! A WASM component as an actor backend: the guest exports `gm_alloc(len) -> ptr` and
//! `gm_event(name_ptr, name_len, json_ptr, json_len) -> i32` (0 pass, 1 cancel) and imports
//! `gm.queue(op_ptr, op_len, ent, json_ptr, json_len)` and `gm.log(ptr, len)`. Payloads and
//! write arguments travel as JSON. CPU is metered with fuel per dispatch, memory by the store
//! limits; a trap or an exhausted budget ends the dispatch like a Luau error does.

use std::sync::{Arc, Mutex};
use wasmtime::{
    Caller, Config, Engine, Instance, Linker, Module, Store, StoreLimits, StoreLimitsBuilder,
};

/// Fuel units per millisecond of budget (rough: one unit is about one WASM instruction).
pub const FUEL_PER_MS: u64 = 2_000_000;

pub struct Host {
    limits: StoreLimits,
    /// Writes the guest asked for: (op, ent, json), drained by the actor into its Lua `world.queue`.
    pub queue: Arc<Mutex<Vec<(String, u32, String)>>>,
    pub log: Arc<Mutex<Vec<String>>>,
}

pub struct WasmGuest {
    store: Store<Host>,
    instance: Instance,
    pub fuel_per_dispatch: u64,
    pub over_budget: bool,
}

fn read(mem: &[u8], ptr: i32, len: i32) -> Option<String> {
    let (p, l) = (usize::try_from(ptr).ok()?, usize::try_from(len).ok()?);
    let s = mem.get(p..p.checked_add(l)?)?;
    Some(String::from_utf8_lossy(s).into_owned())
}

impl WasmGuest {
    pub fn new(bytes: &[u8], memory_limit: usize, cpu_ms: f32) -> wasmtime::Result<Self> {
        let mut config = Config::new();
        config.consume_fuel(true);
        let engine = Engine::new(&config)?;
        let module = Module::new(&engine, bytes)?;
        let host = Host {
            limits: StoreLimitsBuilder::new()
                .memory_size(memory_limit)
                .instances(1)
                .build(),
            queue: Arc::new(Mutex::new(Vec::new())),
            log: Arc::new(Mutex::new(Vec::new())),
        };
        let mut store = Store::new(&engine, host);
        store.limiter(|h| &mut h.limits);
        let mut linker = Linker::new(&engine);
        linker.func_wrap(
            "gm",
            "queue",
            |mut caller: Caller<'_, Host>, op: i32, op_len: i32, ent: i32, p: i32, l: i32| {
                let Some(mem) = caller.get_export("memory").and_then(|e| e.into_memory()) else {
                    return;
                };
                let data = mem.data(&caller);
                let (Some(op), Some(arg)) = (read(data, op, op_len), read(data, p, l)) else {
                    return;
                };
                caller
                    .data()
                    .queue
                    .lock()
                    .unwrap()
                    .push((op, ent as u32, arg));
            },
        )?;
        linker.func_wrap(
            "gm",
            "log",
            |mut caller: Caller<'_, Host>, p: i32, l: i32| {
                let Some(mem) = caller.get_export("memory").and_then(|e| e.into_memory()) else {
                    return;
                };
                if let Some(s) = read(mem.data(&caller), p, l) {
                    caller.data().log.lock().unwrap().push(s);
                }
            },
        )?;
        let fuel_per_dispatch = (cpu_ms.max(0.1) as f64 * FUEL_PER_MS as f64) as u64;
        store.set_fuel(fuel_per_dispatch)?;
        let instance = linker.instantiate(&mut store, &module)?;
        Ok(WasmGuest {
            store,
            instance,
            fuel_per_dispatch,
            over_budget: false,
        })
    }

    /// Runs `gm_event`; `Ok(true)` when the guest cancels. A trap (budget included) is an error.
    pub fn event(&mut self, name: &str, json: &str) -> wasmtime::Result<bool> {
        self.over_budget = false;
        self.store.set_fuel(self.fuel_per_dispatch)?;
        let alloc = self
            .instance
            .get_typed_func::<i32, i32>(&mut self.store, "gm_alloc")?;
        let event = self
            .instance
            .get_typed_func::<(i32, i32, i32, i32), i32>(&mut self.store, "gm_event")?;
        let memory = self
            .instance
            .get_memory(&mut self.store, "memory")
            .ok_or_else(|| wasmtime::Error::msg("guest exports no memory"))?;
        let mut put = |s: &str| -> wasmtime::Result<(i32, i32)> {
            let ptr = alloc.call(&mut self.store, s.len() as i32)?;
            memory.write(&mut self.store, ptr as usize, s.as_bytes())?;
            Ok((ptr, s.len() as i32))
        };
        let (np, nl) = put(name)?;
        let (jp, jl) = put(json)?;
        match event.call(&mut self.store, (np, nl, jp, jl)) {
            Ok(r) => Ok(r != 0),
            Err(e) => {
                if self.store.get_fuel().map(|f| f == 0).unwrap_or(false) {
                    self.over_budget = true;
                }
                Err(e)
            }
        }
    }

    pub fn take_queue(&mut self) -> Vec<(String, u32, String)> {
        std::mem::take(&mut *self.store.data().queue.lock().unwrap())
    }

    pub fn take_log(&mut self) -> Vec<String> {
        std::mem::take(&mut *self.store.data().log.lock().unwrap())
    }
}
