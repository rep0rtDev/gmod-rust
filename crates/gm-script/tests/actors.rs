use gm_script::{api::Payload, ActorConfig, ActorSpec, ActorStatus, Runtime, Snapshot};
use std::sync::Arc;
use std::time::Duration;

fn spec(name: &str, priority: i32, source: &str) -> ActorSpec {
    spec_budget(name, priority, source, 50)
}

fn spec_budget(name: &str, priority: i32, source: &str, cpu_ms: u64) -> ActorSpec {
    ActorSpec {
        addon: "test".into(),
        name: name.into(),
        priority,
        source: source.into(),
        config: ActorConfig {
            cpu_budget: Duration::from_millis(cpu_ms),
            ..Default::default()
        },
    }
}

#[test]
fn sandbox_has_no_process_access() {
    let mut rt = Runtime::new();
    rt.load(spec("a", 0, "events.on('tick.pre', function() state.seen = {os ~= nil, io ~= nil, require ~= nil, loadstring ~= nil} end)")).unwrap();
    let a = &mut rt.actors_mut()[0];
    a.dispatch(
        &Arc::new(Snapshot::default()),
        "tick.pre",
        &Payload::default(),
        1,
    )
    .unwrap();
    let seen: Vec<bool> = a
        .eval("return {state.seen[1], state.seen[2], state.seen[3], state.seen[4]}")
        .unwrap();
    assert_eq!(
        seen,
        vec![false, false, false, false],
        "os, io, require, loadstring visible: {seen:?}"
    );
}

#[test]
fn cpu_budget_aborts_and_throttles() {
    let mut rt = Runtime::new();
    rt.load(spec_budget(
        "spin",
        0,
        "events.on('tick.pre', function() while true do end end)",
        5,
    ))
    .unwrap();
    rt.load(spec_budget(
        "fine",
        0,
        "events.on('tick.pre', function() state.n = (state.n or 0) + 1 end)",
        5,
    ))
    .unwrap();
    let snap = Arc::new(Snapshot::default());
    for tick in 1..=5u64 {
        for a in rt.actors_mut() {
            let _ = a.dispatch(&snap, "tick.pre", &Payload::default(), tick);
        }
    }
    let spin = &rt.actors()[0];
    assert_eq!(spin.overruns_total, 5);
    assert_eq!(spin.status, ActorStatus::Throttled);
    assert!(
        spin.cpu_total() < Duration::from_millis(100),
        "{:?}",
        spin.cpu_total()
    );
    let n: u64 = rt.actors()[1].eval("return state.n").unwrap();
    assert_eq!(n, 5, "the other actor is unaffected");
    assert!(
        !spin.may_run(7) && spin.may_run(8),
        "throttled: every other tick"
    );
}

#[test]
fn error_restarts_with_state() {
    let mut rt = Runtime::new();
    rt.load(spec(
        "crash",
        0,
        "state.count = (state.count or 0) + 1; events.on('tick.pre', function() error('boom') end)",
    ))
    .unwrap();
    let a = &mut rt.actors_mut()[0];
    let r = a.dispatch(
        &Arc::new(Snapshot::default()),
        "tick.pre",
        &Payload::default(),
        1,
    );
    assert!(r.is_err());
    assert_eq!(a.restarts, 1);
    let count: u64 = a.eval("return state.count").unwrap();
    assert_eq!(
        count, 2,
        "state survived the restart, the source ran again on top of it"
    );
}

#[test]
fn cancel_and_queue_order() {
    let mut rt = Runtime::new();
    rt.load(spec("late", 10, "events.on('prop.spawn_request', function(p) state.model = p.model; return nil end); events.on('tick.pre', function() world.queue('freeze', 1, true) end)")).unwrap();
    rt.load(spec("early", 0, "events.on('prop.spawn_request', function(p) if p.model == 'can' then return false end end); events.on('tick.pre', function() world.queue('set_pos', 1, {x = 1, y = 2, z = 3}) end)")).unwrap();
    let snap = Arc::new(Snapshot::default());
    let mut p = Payload::default();
    p.strs.push(("model", "can".into()));
    // the early actor (priority 0) refuses cans before the late one sees the request
    let mut cancelled = false;
    for i in [1usize, 0] {
        if let Ok(Some(false)) = rt.actors_mut()[i].dispatch(&snap, "prop.spawn_request", &p, 1) {
            cancelled = true;
            break;
        }
    }
    assert!(cancelled);
    let seen: Option<String> = rt.actors()[0].eval("return state.model").unwrap();
    assert_eq!(seen, None);
    for a in rt.actors_mut() {
        a.dispatch(&snap, "tick.pre", &Payload::default(), 2)
            .unwrap();
    }
    let order: Vec<(i32, u32, u64, gm_script::QueuedWrite)> = rt.drain_writes();
    assert_eq!(order.len(), 2);
    assert!(
        matches!(order[0].3, gm_script::QueuedWrite::SetPos { .. }),
        "priority 0 first: {order:?}"
    );
    assert!(matches!(order[1].3, gm_script::QueuedWrite::Freeze { .. }));
}

#[test]
fn hot_reload_keeps_state() {
    let mut rt = Runtime::new();
    rt.load(spec(
        "a",
        0,
        "events.on('tick.pre', function() state.n = (state.n or 0) + 1 end)",
    ))
    .unwrap();
    let snap = Arc::new(Snapshot::default());
    for tick in 1..=3u64 {
        rt.actors_mut()[0]
            .dispatch(&snap, "tick.pre", &Payload::default(), tick)
            .unwrap();
    }
    rt.actors_mut()[0]
        .reload(
            "state.m = state.n * 2; events.on('tick.pre', function() state.n = state.n + 10 end)"
                .into(),
        )
        .unwrap();
    rt.actors_mut()[0]
        .dispatch(&snap, "tick.pre", &Payload::default(), 4)
        .unwrap();
    let (n, m): (u64, u64) = rt.actors()[0].eval("return state.n, state.m").unwrap();
    assert_eq!((n, m), (13, 6));
    assert_eq!(rt.actors()[0].restarts, 0);
}

#[test]
fn gmr_pack_and_load_roundtrip() {
    let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../addons/examples/freeze_crates");
    let bytes = gm_script::package::pack(&root).unwrap();
    let again = gm_script::package::pack(&root).unwrap();
    assert_eq!(bytes, again, "deterministic");
    let pkg = gm_script::Package::read(&bytes).unwrap();
    assert_eq!(
        pkg.files.keys().cloned().collect::<Vec<_>>(),
        vec!["manifest.toml", "src/main.luau"]
    );
    let (m, specs) = pkg.load().unwrap();
    assert_eq!(m.id, "freeze_crates");
    assert_eq!(specs.len(), 1);
    assert!(specs[0].config.can_write);
    let (dm, dspecs) = gm_script::Manifest::load(&root).unwrap();
    assert_eq!(
        (dm.id, dspecs[0].source.clone()),
        (m.id, specs[0].source.clone())
    );
    let out = std::path::Path::new(env!("CARGO_TARGET_TMPDIR")).join("freeze_crates.gmr");
    std::fs::write(&out, &bytes).unwrap();
    let mut rt = Runtime::new();
    let loaded = rt.load_addon(&out).unwrap();
    assert_eq!(loaded.id, "freeze_crates");
    assert_eq!(rt.actors().len(), 1);
}

#[test]
fn client_realm_sees_the_assembled_world() {
    let mut rt = Runtime::client();
    rt.load(spec(
        "hud",
        0,
        "events.on('snapshot.received', function(p) state.snaps = (state.snaps or 0) + 1; state.tick = p.tick end)
         events.on('tick.client', function() state.frames = (state.frames or 0) + 1; state.props = world.count(); local e = world.find('crate')[1]; if e then state.z = e.pos.z end; state.tr = phys.trace({x=0,y=0,z=0},{x=0,y=0,z=-100}).hit end)",
    ))
    .unwrap();
    let mut world = gm_net::ClientWorld {
        tick: 40,
        ..Default::default()
    };
    world.entities.insert(
        3,
        gm_net::EntityState {
            id: 3,
            model: "crate".into(),
            pos: glam::Vec3::new(1.0, 2.0, 3.5),
            ..Default::default()
        },
    );
    world.own.count = 1;
    rt.client_tick(&world, true);
    rt.client_tick(&world, false);
    let (snaps, frames, tick, props, z, tr): (u64, u64, u64, u64, f32, bool) = rt.actors()[0]
        .eval("return state.snaps, state.frames, state.tick, state.props, state.z, state.tr")
        .unwrap();
    assert_eq!(
        (snaps, frames, tick, props, z, tr),
        (1, 2, 40, 1, 3.5, false)
    );
}

#[test]
fn client_actors_hear_the_clients_key_edges() {
    let mut rt = Runtime::client();
    rt.load(spec(
        "keys",
        0,
        "state.seen = {}
         events.on('input.key', function(p) table.insert(state.seen, p.key .. (p.down and '+' or '-')) end)",
    ))
    .unwrap();
    let world = gm_net::ClientWorld::default();
    rt.client_input(&world, &[]);
    rt.client_input(
        &world,
        &[
            ("KeyE".to_string(), true),
            ("MouseLeft".to_string(), true),
            ("KeyE".to_string(), false),
        ],
    );
    let seen: Vec<String> = rt.actors()[0].eval("return state.seen").unwrap();
    assert_eq!(seen, ["KeyE+", "MouseLeft+", "KeyE-"].map(String::from));
}

/// A WASM guest on the flat ABI: counts `tick.pre` in a global, queues `freeze` of entity 7
/// on the third one, logs once, and cancels `prop.spawn_request`.
const WAT_GUEST: &str = r#"(module
  (import "gm" "queue" (func $queue (param i32 i32 i32 i32 i32)))
  (import "gm" "log" (func $log (param i32 i32)))
  (import "gm" "net_send" (func $net_send (param i32 i32 i32 i32 i32)))
  (memory (export "memory") 1)
  (global $ticks (mut i32) (i32.const 0))
  (global $heap (mut i32) (i32.const 1024))
  (data (i32.const 0) "freeze")
  (data (i32.const 16) "true")
  (data (i32.const 32) "hello from wasm")
  (data (i32.const 64) "tick.pre")
  (data (i32.const 80) "prop.spawn_request")
  (data (i32.const 112) "net.message")
  (data (i32.const 128) "echo")
  (data (i32.const 144) "{\"a\":1}")
  (func (export "gm_alloc") (param $len i32) (result i32)
    (local $p i32)
    (local.set $p (global.get $heap))
    (global.set $heap (i32.add (global.get $heap) (local.get $len)))
    (local.get $p))
  (func $streq (param $a i32) (param $alen i32) (param $b i32) (param $blen i32) (result i32)
    (local $i i32)
    (if (i32.ne (local.get $alen) (local.get $blen)) (then (return (i32.const 0))))
    (block $done (loop $l
      (br_if $done (i32.ge_u (local.get $i) (local.get $alen)))
      (if (i32.ne (i32.load8_u (i32.add (local.get $a) (local.get $i)))
                  (i32.load8_u (i32.add (local.get $b) (local.get $i))))
        (then (return (i32.const 0))))
      (local.set $i (i32.add (local.get $i) (i32.const 1)))
      (br $l)))
    (i32.const 1))
  (func (export "gm_event") (param $np i32) (param $nl i32) (param $jp i32) (param $jl i32) (result i32)
    (if (call $streq (local.get $np) (local.get $nl) (i32.const 64) (i32.const 8))
      (then
        (global.set $ticks (i32.add (global.get $ticks) (i32.const 1)))
        (if (i32.eq (global.get $ticks) (i32.const 3))
          (then
            (call $queue (i32.const 0) (i32.const 6) (i32.const 7) (i32.const 16) (i32.const 4))
            (call $log (i32.const 32) (i32.const 15))))))
    (if (call $streq (local.get $np) (local.get $nl) (i32.const 80) (i32.const 18))
      (then (return (i32.const 1))))
    (if (call $streq (local.get $np) (local.get $nl) (i32.const 112) (i32.const 11))
      (then (call $net_send (i32.const 128) (i32.const 4) (i32.const 0) (i32.const 144) (i32.const 7))))
    (i32.const 0))
)"#;

#[test]
fn a_wasm_component_counts_ticks_queues_a_write_and_cancels_a_request() {
    let mut rt = Runtime::new();
    rt.load(ActorSpec {
        addon: "wasm".into(),
        name: "guest".into(),
        priority: 0,
        source: String::new(),
        config: ActorConfig {
            wasm: Some(WAT_GUEST.as_bytes().to_vec()),
            can_write: true,
            ..Default::default()
        },
    })
    .unwrap();
    // the guest counts tick.pre: dispatch the actor directly
    let snap = std::sync::Arc::new(gm_script::snapshot::Snapshot::default());
    let a = &mut rt.actors_mut()[0];
    for _ in 0..3 {
        a.dispatch(&snap, "tick.pre", &gm_script::api::Payload::default(), 1)
            .unwrap();
    }
    let queued = a.take_queue();
    assert_eq!(queued.len(), 1, "{queued:?}");
    assert_eq!(
        queued[0].1,
        gm_script::actor::QueuedWrite::Freeze {
            ent: 7,
            frozen: true
        }
    );
    let cancel = a
        .dispatch(
            &snap,
            "prop.spawn_request",
            &gm_script::api::Payload::default(),
            1,
        )
        .unwrap();
    assert_eq!(cancel, Some(false), "the guest cancels spawn requests");
    // a net message in: the guest answers with net_send("echo", {a = 1}) to everyone
    let data = gm_script::netval::encode(&mlua::Value::Integer(7)).unwrap();
    let p = gm_script::api::Payload {
        net: Some(("ping".into(), data)),
        ..Default::default()
    };
    a.dispatch(&snap, "net.message", &p, 1).unwrap();
    let out = a.take_net();
    assert_eq!(out.len(), 1, "{out:?}");
    assert_eq!((out[0].0, out[0].1.as_str()), (0, "echo"));
    let lua = mlua::Lua::new();
    let v = gm_script::netval::decode(&lua, &out[0].2).unwrap();
    let t = v.as_table().expect("a table");
    assert_eq!(t.get::<i64>("a").unwrap(), 1);
}

fn tick(rt: &mut Runtime, t: u64) {
    let snap = Arc::new(Snapshot {
        tick: t,
        ..Default::default()
    });
    for a in rt.actors_mut() {
        a.dispatch(&snap, "tick.pre", &Payload::default(), t)
            .unwrap();
    }
}

#[test]
fn timers_count_ticks_and_a_failing_one_is_destroyed() {
    let mut rt = Runtime::new();
    rt.load(spec(
        "t",
        0,
        r#"
        state.fired = {}
        local function mark(tag) return function() table.insert(state.fired, tag .. "@" .. game.tick()) end end
        timer.Simple(0.03, mark("simple"))
        timer.Create("rep", 0.015, 3, mark("rep"))
        timer.Create("forever", 0.045, 0, mark("forever"))
        timer.Create("bad", 0.015, 0, function() error("boom") end)
        timer.Create("gone", 0.015, 1, mark("gone"))
        timer.Remove("gone")
        timer.Create("paused", 0.015, 1, mark("paused"))
        timer.Pause("paused")
        "#,
    ))
    .unwrap();
    for t in 1..=7 {
        tick(&mut rt, t);
    }
    let a = &rt.actors()[0];
    let fired: Vec<String> = a.eval("return state.fired").unwrap();
    assert_eq!(
        fired,
        vec![
            "rep@1",
            "simple@2",
            "rep@2",
            "forever@3",
            "rep@3",
            "forever@6"
        ]
    );
    let exists: Vec<bool> = a
        .eval("return {timer.Exists('rep'), timer.Exists('forever'), timer.Exists('bad'), timer.Exists('paused')}")
        .unwrap();
    assert_eq!(exists, vec![false, true, false, true]);
    let prints = rt.actors_mut()[0].take_prints();
    assert!(
        prints
            .iter()
            .any(|p| p.contains("timer bad") && p.contains("boom")),
        "{prints:?}"
    );
}

#[test]
fn netval_round_trips_and_caps() {
    let lua = mlua::Lua::new();
    let v: mlua::Value = lua
        .load(
            "return { a = 1, b = 2.5, c = 'x', d = { true, false, { e = 'deep' } }, [10] = 'ten' }",
        )
        .eval()
        .unwrap();
    let bytes = gm_script::netval::encode(&v).unwrap();
    let back = gm_script::netval::decode(&lua, &bytes).unwrap();
    lua.globals().set("back", back).unwrap();
    let ok: bool = lua
        .load("return back.a == 1 and back.b == 2.5 and back.c == 'x' and back.d[1] == true and back.d[2] == false and back.d[3].e == 'deep' and back[10] == 'ten' and back.zz == nil")
        .eval()
        .unwrap();
    assert!(ok);
    let big: mlua::Value = lua.load("return string.rep('x', 70000)").eval().unwrap();
    assert!(gm_script::netval::encode(&big).is_err(), "over the cap");
    let f: mlua::Value = lua.load("return function() end").eval().unwrap();
    assert!(
        gm_script::netval::encode(&f).is_err(),
        "functions never cross"
    );
    let deep: mlua::Value = lua
        .load("local t = {} local c = t for i = 1, 20 do c.n = {} c = c.n end return t")
        .eval()
        .unwrap();
    assert!(gm_script::netval::encode(&deep).is_err(), "depth cap");
    assert!(
        gm_script::netval::decode(&lua, &bytes[..bytes.len() - 1]).is_err(),
        "truncated"
    );
}

#[test]
fn storage_persists_per_addon_with_caps() {
    let dir = std::env::temp_dir().join(format!("gm-script-storage-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    let src = "events.on('tick.pre', function()
        if state.done then return end
        state.done = true
        storage.set('count', (storage.get('count') or 0) + 1)
        storage.set('cfg', { name = 'x', list = { 1, 2 } })
        storage.set('gone', 1)
        storage.set('gone', nil)
        state.big_ok = pcall(function()
            for i = 1, 20 do storage.set('big' .. i, string.rep('x', 60000)) end
        end)
        state.keys = storage.keys()
    end)";
    {
        let mut rt = Runtime::new();
        rt.data_dir = Some(dir.clone());
        rt.load(spec_budget("a", 0, src, 50)).unwrap();
        let mut other = spec("b", 0, "events.on('tick.pre', function() state.err = select(2, pcall(storage.get, 'count')) end)");
        other.config.can_storage = false;
        rt.load(other).unwrap();
        tick(&mut rt, 1);
        let (count, big_ok, nkeys): (i64, bool, usize) = rt.actors()[0]
            .eval("return storage.get('count'), state.big_ok, #state.keys")
            .unwrap();
        assert_eq!(
            (count, big_ok),
            (1, false),
            "the 1 MiB cap stops the big writes"
        );
        assert_eq!(nkeys, 2 + 17, "count, cfg and the big keys that fit");
        let err: String = rt.actors()[1].eval("return tostring(state.err)").unwrap();
        assert!(err.contains("no storage capability"), "{err}");
        assert_eq!(rt.flush_storage(), 1);
    }
    assert!(dir.join("server").join("test.gms").is_file());
    let mut rt = Runtime::new();
    rt.data_dir = Some(dir.clone());
    rt.load(spec_budget("a", 0, src, 50)).unwrap();
    tick(&mut rt, 1);
    let (count, name, two, gone): (i64, String, i64, Option<i64>) = rt.actors()[0]
        .eval("return storage.get('count'), storage.get('cfg').name, storage.get('cfg').list[2], storage.get('gone')")
        .unwrap();
    assert_eq!((count, name.as_str(), two, gone), (2, "x", 2, None));
    drop(rt);
    let again = Runtime::new();
    let _ = again;
    let mut rt = Runtime::new();
    rt.data_dir = Some(dir.clone());
    rt.load(spec("a", 0, "")).unwrap();
    let count: i64 = rt.actors()[0].eval("return storage.get('count')").unwrap();
    assert_eq!(count, 2, "the drop flushed the second run");
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn file_watch_reloads_changed_addons_and_survives_a_broken_edit() {
    let src = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../addons/examples/freeze_crates");
    let dir = std::path::Path::new(env!("CARGO_TARGET_TMPDIR"))
        .join(format!("watch_{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(dir.join("src")).unwrap();
    std::fs::copy(src.join("manifest.toml"), dir.join("manifest.toml")).unwrap();
    std::fs::copy(src.join("src/main.luau"), dir.join("src/main.luau")).unwrap();
    let mut rt = Runtime::new();
    rt.load_addon(&dir).unwrap();
    assert!(rt.check_reloads().is_empty(), "nothing changed");
    let main = dir.join("src/main.luau");
    let original = std::fs::read_to_string(&main).unwrap();
    std::fs::write(&main, format!("{original}\nstate.marker = 42\n")).unwrap();
    let r = rt.check_reloads();
    assert_eq!(r.len(), 1);
    assert!(r[0].1.is_ok(), "{:?}", r[0].1);
    let marker: i64 = rt.actors()[0].eval("return state.marker").unwrap();
    assert_eq!(marker, 42);
    std::fs::write(
        &main,
        format!("{original}\nstate.marker = 43\nthis is not luau(\n"),
    )
    .unwrap();
    let r = rt.check_reloads();
    assert_eq!(r.len(), 1);
    assert!(r[0].1.is_err());
    assert!(
        rt.log
            .iter()
            .any(|l| l.contains("reload freeze_crates failed")),
        "{:?}",
        rt.log
    );
    let marker: i64 = rt.actors()[0].eval("return state.marker").unwrap();
    assert_eq!(marker, 42, "the old VM keeps running after a broken edit");
    assert!(
        rt.check_reloads().is_empty(),
        "a failed file is not retried until it changes"
    );
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn pool_runs_every_job_and_survives_a_panic() {
    let pool = gm_script::pool::Pool::new(3);
    let hits = std::sync::atomic::AtomicUsize::new(0);
    let mut slots = [0u32; 50];
    {
        let jobs: Vec<Box<dyn FnOnce() + Send + '_>> = slots
            .iter_mut()
            .enumerate()
            .map(|(i, s)| {
                let hits = &hits;
                Box::new(move || {
                    hits.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
                    if i == 7 {
                        panic!("job 7 dies");
                    }
                    *s = i as u32 + 1;
                }) as Box<dyn FnOnce() + Send + '_>
            })
            .collect();
        pool.run(jobs);
    }
    assert_eq!(hits.load(std::sync::atomic::Ordering::Relaxed), 50);
    assert_eq!(slots[7], 0);
    assert!(slots
        .iter()
        .enumerate()
        .all(|(i, &v)| i == 7 || v == i as u32 + 1));
    pool.run(vec![Box::new(|| {}) as Box<dyn FnOnce() + Send + '_>]);
}

fn write_addon(
    dir: &std::path::Path,
    id: &str,
    version: &str,
    depends: &[&str],
) -> std::path::PathBuf {
    let d = dir.join(id);
    std::fs::create_dir_all(d.join("src")).unwrap();
    let deps: Vec<String> = depends.iter().map(|s| format!("{s:?}")).collect();
    std::fs::write(
        d.join("manifest.toml"),
        format!(
            "id = {id:?}\nversion = {version:?}\ndepends = [{}]\n[[actors]]\nname = \"a\"\nsource = \"src/a.luau\"\n",
            deps.join(", ")
        ),
    )
    .unwrap();
    std::fs::write(d.join("src/a.luau"), "state.x = 1").unwrap();
    d
}

#[test]
fn addons_load_in_dependency_order_and_bad_dependencies_refuse_the_whole_set() {
    let dir = std::env::temp_dir().join(format!("gm-deps-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    let base = write_addon(&dir, "base", "1.4.0", &[]);
    let wire = write_addon(&dir, "wire", "0.3.1", &["base ^1.2"]);
    let hud = write_addon(&dir, "hud", "2.0.0", &["wire ~0.3", "base >= 1"]);
    let mut rt = gm_script::Runtime::new();
    let ms = rt
        .load_addons(&[hud.clone(), wire.clone(), base.clone()])
        .unwrap();
    let ids: Vec<&str> = ms.iter().map(|m| m.id.as_str()).collect();
    assert_eq!(ids, ["base", "wire", "hud"]);
    let order: Vec<&str> = rt.actors().iter().map(|a| a.spec.addon.as_str()).collect();
    assert_eq!(
        order,
        ["base", "wire", "hud"],
        "actors run in load order at equal priority"
    );

    let mut rt2 = gm_script::Runtime::new();
    let old = write_addon(&dir, "old", "0.9.0", &["base < 1"]);
    let e = rt2
        .load_addons(&[base.clone(), old.clone()])
        .unwrap_err()
        .to_string();
    assert!(
        e.contains("old depends on `base < 1`, but base is 1.4.0"),
        "{e}"
    );
    assert_eq!(rt2.actors().len(), 0, "nothing of a refused set is loaded");
    let e = rt2
        .load_addons(std::slice::from_ref(&wire))
        .unwrap_err()
        .to_string();
    assert!(
        e.contains("wire depends on base, which is not loaded"),
        "{e}"
    );
    let x = write_addon(&dir, "x", "1.0.0", &["y"]);
    let y = write_addon(&dir, "y", "1.0.0", &["x"]);
    let e = rt2.load_addons(&[x, y]).unwrap_err().to_string();
    assert!(e.contains("cycle"), "{e}");

    // a reload may not break what depends on the addon: wire wants base ^1.2
    write_addon(&dir, "base", "2.0.0", &[]);
    let e = rt.reload_addon(&base).unwrap_err().to_string();
    assert!(
        e.contains("wire depends on `base ^1.2`, but base is 2.0.0"),
        "{e}"
    );
    assert_eq!(
        rt.addon_versions["base"], "1.4.0",
        "the old version stays loaded"
    );
    write_addon(&dir, "base", "1.5.0", &[]);
    assert_eq!(rt.reload_addon(&base).unwrap().version, "1.5.0");
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn a_trusting_runtime_loads_only_addons_signed_by_a_trusted_key() {
    use gm_script::package;
    let dir = std::env::temp_dir().join(format!("gm-sign-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    let plain = write_addon(&dir, "plain", "1.0.0", &[]);
    let signed = write_addon(&dir, "signed", "1.0.0", &[]);
    let (sk, pk) = package::keygen();
    let (_, other) = package::keygen();
    let mut files = std::collections::BTreeMap::new();
    package_collect(&signed, &mut files);
    std::fs::write(signed.join(package::SIGNATURE), package::sign(&files, &sk)).unwrap();
    let gmr = dir.join("signed.gmr");
    std::fs::write(&gmr, package::pack(&signed).unwrap()).unwrap();
    assert_eq!(
        gm_script::Package::open(&gmr).unwrap().signers().unwrap(),
        vec![pk],
        "the directory's signature holds for the package"
    );

    let mut open = gm_script::Runtime::new();
    open.load_addons(&[plain.clone(), gmr.clone()]).unwrap();
    assert_eq!(
        open.actors().len(),
        2,
        "without a trust list everything loads"
    );

    let mut rt = gm_script::Runtime::new();
    rt.trusted.push(other);
    let e = rt.load_addon(&gmr).unwrap_err().to_string();
    assert!(e.contains("signed is not signed by a trusted key"), "{e}");
    rt.trusted.push(pk);
    let e = rt
        .load_addons(&[gmr.clone(), plain.clone()])
        .unwrap_err()
        .to_string();
    assert!(e.contains("plain is not signed by a trusted key"), "{e}");
    assert_eq!(
        rt.actors().len(),
        0,
        "an unsigned addon refuses the whole set"
    );
    rt.load_addon(&gmr).unwrap();
    let e = rt.load_addon(&signed).unwrap_err().to_string();
    assert!(e.contains("signed is already loaded"), "{e}");
    assert_eq!(rt.actors().len(), 1);

    std::fs::write(signed.join("src/a.luau"), "state.x = 2").unwrap();
    let e = gm_script::Runtime::new()
        .load_addon(&signed)
        .unwrap_err()
        .to_string();
    assert!(
        e.contains("does not match the content"),
        "{e}: a tampered signed addon is refused even without a trust list"
    );
    let _ = std::fs::remove_dir_all(&dir);
}

fn package_collect(dir: &std::path::Path, out: &mut std::collections::BTreeMap<String, Vec<u8>>) {
    let bytes = gm_script::package::pack(dir).unwrap();
    *out = gm_script::Package::read(&bytes).unwrap().files;
}

#[test]
fn registry_publishes_signed_packages_and_resolves_the_highest_fitting_versions() {
    use gm_script::package;
    let dir = std::env::temp_dir().join(format!("gm-reg-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    let (sk, pk) = package::keygen();
    let signed = |id: &str, version: &str, depends: &[&str]| {
        let d = write_addon(&dir, &format!("{id}-{version}"), version, depends);
        let mut m = std::fs::read_to_string(d.join("manifest.toml")).unwrap();
        m = m.replacen(
            &format!("id = {:?}", format!("{id}-{version}")),
            &format!("id = {id:?}"),
            1,
        );
        std::fs::write(d.join("manifest.toml"), m).unwrap();
        let mut files = std::collections::BTreeMap::new();
        package_collect(&d, &mut files);
        std::fs::write(d.join(package::SIGNATURE), package::sign(&files, &sk)).unwrap();
        package::pack(&d).unwrap()
    };
    let reg_dir = dir.join("registry");
    let mut reg = gm_script::Registry::open(&reg_dir).unwrap();
    let unsigned = package::pack(&write_addon(&dir, "nosig", "1.0.0", &[])).unwrap();
    assert!(matches!(
        reg.publish(&unsigned),
        Err(gm_script::registry::RegistryError::Unsigned(_))
    ));
    let base14 = signed("base", "1.4.0", &[]);
    let base15 = signed("base", "1.5.0", &[]);
    let base20 = signed("base", "2.0.0", &[]);
    let wire = signed("wire", "0.3.0", &["base ^1.4"]);
    let hud = signed("hud", "1.0.0", &["wire", "base >= 1.5"]);
    let strict = signed("strict", "1.0.0", &["base < 1.5"]);
    for b in [&base14, &base15, &base20, &wire, &hud, &strict] {
        let e = reg.publish(b).unwrap();
        assert_eq!(e.signers, vec![package::hex(&pk)]);
    }
    assert_eq!(reg.index.packages.len(), 6);
    assert!(
        reg.publish(&base14).is_ok(),
        "the same bytes again are fine"
    );
    let other = signed("base", "1.4.0", &["wire"]);
    assert!(matches!(
        reg.publish(&other),
        Err(gm_script::registry::RegistryError::Conflict(..))
    ));

    let reg = gm_script::Registry::open(&reg_dir).unwrap();
    let names = |es: &[gm_script::registry::Entry]| -> Vec<String> {
        es.iter()
            .map(|e| format!("{} {}", e.id, e.version))
            .collect()
    };
    assert_eq!(
        names(&reg.resolve(&["base".into()]).unwrap()),
        ["base 2.0.0"]
    );
    assert_eq!(
        names(&reg.resolve(&["wire".into()]).unwrap()),
        ["base 1.5.0", "wire 0.3.0"],
        "the dependency comes first, the highest fitting version of it"
    );
    assert_eq!(
        names(&reg.resolve(&["hud".into()]).unwrap()),
        ["base 1.5.0", "wire 0.3.0", "hud 1.0.0"]
    );
    assert_eq!(
        names(&reg.resolve(&["strict".into(), "base".into()]).unwrap()),
        ["base 1.4.0", "strict 1.0.0"],
        "a later requirement narrows an earlier pick"
    );
    let e = reg
        .resolve(&["strict".into(), "hud".into()])
        .unwrap_err()
        .to_string();
    assert!(e.contains("base is wanted as"), "{e}");
    let e = reg.resolve(&["base > 3".into()]).unwrap_err().to_string();
    assert!(e.contains("no package satisfies"), "{e}");

    let cache = dir.join("cache");
    let plan = reg.resolve(&["hud".into()]).unwrap();
    let mut rt = gm_script::Runtime::new();
    rt.trusted.push(pk);
    let paths: Vec<_> = plan
        .iter()
        .map(|e| reg.install(e, &cache).unwrap())
        .collect();
    let loaded = rt.load_addons(&paths).unwrap();
    assert_eq!(loaded.len(), 3);
    assert_eq!(rt.addon_versions["base"], "1.5.0");
    assert!(reg.fetch_by_sha(&plan[0].sha, &cache).is_some());
    assert!(reg.fetch_by_sha("00ff", &cache).is_none());

    // the same registry over plain http: read only, same plan, blobs fetched and checked
    let base = gm_script::http::serve_dir_for_tests(reg_dir.clone(), true);
    let mut http = gm_script::Registry::open_any(&base).unwrap();
    assert_eq!(http.index, reg.index);
    assert!(matches!(
        http.publish(&base14),
        Err(gm_script::registry::RegistryError::ReadOnly(_))
    ));
    let http_cache = dir.join("http-cache");
    let got: Vec<_> = http
        .resolve(&["hud".into()])
        .unwrap()
        .iter()
        .map(|e| http.install(e, &http_cache).unwrap())
        .collect();
    assert_eq!(got.len(), 3);
    assert!(gm_script::Runtime::new().load_addons(&got).is_ok());

    std::fs::write(reg.blob_path(&plan[0].sha), b"garbage").unwrap();
    let _ = std::fs::remove_dir_all(&cache);
    assert!(
        reg.install(&plan[0], &cache).is_err(),
        "a damaged blob does not install"
    );
    let _ = std::fs::remove_dir_all(&http_cache);
    assert!(
        http.install(&plan[0], &http_cache).is_err(),
        "nor over http"
    );
    let _ = std::fs::remove_dir_all(&dir);
}

/// Client realm GLua: a `HUDPaint` hook draws through `surface` / `draw`; the frame's
/// `render.hud` runs the game's hook chain and the calls come back as draw commands.
#[test]
fn glua_client_hud_paint_draws_through_surface() {
    let mut rt = Runtime::client();
    rt.load(ActorSpec {
        addon: "hud".into(),
        name: "paint".into(),
        priority: 0,
        source: r#"
            assert(CLIENT and not SERVER, "client realm globals")
            hook.Add("HUDPaint", "test", function()
                surface.SetDrawColor(10, 20, 30, 255)
                surface.DrawRect(5, 6, 70, 80)
                draw.SimpleText("hi", "DermaDefault", ScrW() / 2, 0, Color(255, 255, 255), TEXT_ALIGN_CENTER)
            end)
            hook.Add("HUDShouldDraw", "test", function(name) return name ~= "CHudHealth" end)
            hook.Add("Think", "test", function() THOUGHT = (THOUGHT or 0) + 1 end)
            hook.Add("OnScreenSizeChanged", "test", function(ow, oh, nw, nh) SIZES = { ow, oh, nw, nh } end)
            hook.Add("PostDrawHUD", "test", function()
                local lp = LocalPlayer()
                surface.DrawRect(IsValid(lp) and lp:EntIndex() or -1, 0, 1, 1)
            end)
        "#
        .into(),
        // a frame's hook chain under the test suite's parallel load: not the 2 ms default
        config: ActorConfig {
            cpu_budget: Duration::from_millis(500),
            ..ActorConfig::default()
        },
    })
    .expect("actor");
    let mut world = gm_net::ClientWorld::default();
    let draws = rt.client_render(&world, 1280, 720);
    assert!(
        draws.iter().any(|d| matches!(d, gm_script::DrawCmd::Rect { x, w, h, .. } if *x == -1.0 && *w == 1.0 && *h == 1.0)),
        "LocalPlayer() is NULL before the server accepted us: {draws:?}"
    );
    world.local_id = Some(3);
    world.players.push(gm_net::msg::PlayerSnap {
        id: 3,
        ..Default::default()
    });
    let draws = rt.client_render(&world, 1280, 720);
    assert!(
        draws.iter().any(|d| matches!(d, gm_script::DrawCmd::Rect { x, w, h, .. } if *x == 3.0 && *w == 1.0 && *h == 1.0)),
        "LocalPlayer() is our player once connected: {draws:?}"
    );
    assert!(
        draws.iter().any(|d| matches!(d, gm_script::DrawCmd::Rect { x, y, w, h, .. } if *x == 5.0 && *y == 6.0 && *w == 70.0 && *h == 80.0)),
        "HUDPaint's DrawRect reaches the client: {draws:?}"
    );
    assert!(
        draws
            .iter()
            .any(|d| matches!(d, gm_script::DrawCmd::Text { text, .. } if text == "hi")),
        "draw.SimpleText reaches the client: {draws:?}"
    );
    assert!(
        draws
            .iter()
            .any(|d| matches!(d, gm_script::DrawCmd::HideHud { name } if name == "CHudHealth")),
        "HUDShouldDraw false hides the engine element: {draws:?}"
    );
    assert!(
        !draws
            .iter()
            .any(|d| matches!(d, gm_script::DrawCmd::HideHud { name } if name == "CHudAmmo")),
        "elements not vetoed stay: {draws:?}"
    );
    rt.client_tick(&world, false);
    let _ = rt.client_render(&world, 1920, 1080);
    let (thought, sizes): (f64, String) = rt.actors()[0]
        .eval("return THOUGHT, SIZES and table.concat(SIZES, \",\") or \"\"")
        .expect("eval");
    assert_eq!(thought, 1.0, "Think runs once per client tick");
    // input / gui / killicon state from the key edges and the frame
    rt.client_input(&world, &[("KeyW".into(), true), ("MouseLeft".into(), true)]);
    let (down, pressed_w, name, code, bind, icon): (bool, bool, String, f64, String, bool) = rt.actors()[0]
        .eval(r#"return input.IsKeyDown(KEY_W), input.WasKeyPressed(KEY_W), input.GetKeyName(MOUSE_LEFT), input.GetKeyCode("SPACE"), input.LookupBinding("+jump"), killicon.Exists("weapon_crowbar")"#)
        .expect("eval");
    assert!(
        down && pressed_w,
        "KeyW down is KEY_W down / pressed this frame"
    );
    assert_eq!(name, "MOUSE1");
    assert_eq!(code, 65.0, "SPACE is KEY_SPACE");
    assert_eq!(bind, "space");
    assert!(icon, "the game's default kill icons exist");
    rt.client_input(&world, &[("KeyW".into(), false)]);
    let _ = rt.client_render(&world, 1920, 1080);
    let (down, pressed_w, mouse): (bool, bool, bool) = rt.actors()[0]
        .eval("return input.IsKeyDown(KEY_W), input.WasKeyPressed(KEY_W), input.IsMouseDown(MOUSE_LEFT)")
        .expect("eval");
    assert!(
        !down && !pressed_w && mouse,
        "released key is up, the frame clears the edge, the mouse stays down"
    );
    assert_eq!(
        sizes, "1280,720,1920,1080",
        "OnScreenSizeChanged on a new screen size"
    );
}

/// Derma: a DFrame with a DButton paints through `surface` in panel coordinates, the cursor
/// hovers it, a MouseLeft press / release over it runs `DoClick`, a DTextEntry takes typing.
#[test]
fn glua_derma_frame_button_click_and_typing() {
    let mut rt = Runtime::client();
    rt.load(ActorSpec {
        addon: "ui".into(),
        name: "frame".into(),
        priority: 0,
        source: r#"
            FRAME = vgui.Create("DFrame")
            FRAME:SetPos(100, 100)
            FRAME:SetSize(300, 200)
            FRAME:SetTitle("Test")
            BTN = vgui.Create("DButton", FRAME)
            BTN:SetPos(20, 40)
            BTN:SetSize(100, 30)
            BTN:SetText("Press")
            CLICKS = 0
            BTN.DoClick = function() CLICKS = CLICKS + 1 end
            ENTRY = vgui.Create("DTextEntry", FRAME)
            ENTRY:SetPos(20, 100)
            ENTRY:SetSize(200, 20)
            ENTRY.OnEnter = function(_, v) ENTERED = v end
            DOCKED = vgui.Create("DPanel", FRAME)
            DOCKED:Dock(BOTTOM)
            DOCKED:SetTall(30)
            FRAME:MakePopup()
        "#
        .into(),
        config: ActorConfig {
            cpu_budget: Duration::from_millis(500),
            ..ActorConfig::default()
        },
    })
    .expect("actor");
    let world = gm_net::ClientWorld::default();
    rt.cursor = Some((170.0, 155.0));
    let draws = rt.client_render(&world, 1280, 720);
    // the button's body: frame (100,100) + button (20,40) => a 100x30 rect at (120,140)
    assert!(
        draws.iter().any(|d| matches!(d, gm_script::DrawCmd::Rect { x, y, w, h, .. } if *x == 120.0 && *y == 140.0 && *w == 100.0 && *h == 30.0)),
        "button painted in screen space: {draws:?}"
    );
    assert!(
        draws
            .iter()
            .any(|d| matches!(d, gm_script::DrawCmd::Text { text, .. } if text == "Press")),
        "button label: {draws:?}"
    );
    let (hovered, docked): (bool, String) = rt.actors()[0]
        .eval("return BTN:IsHovered(), DOCKED:GetPos() .. ',' .. select(2, DOCKED:GetPos()) .. ',' .. DOCKED:GetWide()")
        .expect("eval");
    assert!(hovered, "the cursor over the button hovers it");
    assert_eq!(
        docked, "0,170,300",
        "Dock(BOTTOM) lays the panel along the frame's bottom"
    );
    rt.client_input(
        &world,
        &[("MouseLeft".into(), true), ("MouseLeft".into(), false)],
    );
    let clicks: f64 = rt.actors()[0].eval("return CLICKS").expect("eval");
    assert_eq!(clicks, 1.0, "press + release over the button is a click");
    // click the entry, type, enter
    rt.cursor = Some((130.0, 210.0));
    let _ = rt.client_render(&world, 1280, 720);
    rt.client_input(
        &world,
        &[
            ("MouseLeft".into(), true),
            ("MouseLeft".into(), false),
            ("ShiftLeft".into(), true),
            ("KeyH".into(), true),
            ("KeyH".into(), false),
            ("ShiftLeft".into(), false),
            ("KeyI".into(), true),
            ("KeyI".into(), false),
            ("Digit1".into(), true),
            ("Digit1".into(), false),
            ("Backspace".into(), true),
            ("Backspace".into(), false),
            ("Enter".into(), true),
            ("Enter".into(), false),
        ],
    );
    let (value, entered): (String, String) = rt.actors()[0]
        .eval("return ENTRY:GetValue(), ENTERED or ''")
        .expect("eval");
    assert_eq!(value, "Hi", "typed with shift, digit removed by backspace");
    assert_eq!(entered, "Hi", "Enter runs OnEnter with the value");
    // the frame's close button removes it
    rt.actors()[0]
        .eval::<()>("FRAME.btnClose:DoClick()")
        .expect("eval");
    let valid: bool = rt.actors()[0].eval("return IsValid(FRAME)").expect("eval");
    assert!(!valid, "Close() removes a DFrame that deletes on close");
}

/// Derma controls: list view lines / sorting / selection, property sheet tabs, tree nodes,
/// colour mixer and a form all build, lay out and paint without errors.
#[test]
fn glua_derma_controls_build_and_lay_out() {
    let mut rt = Runtime::client();
    rt.load(ActorSpec {
        addon: "ui".into(),
        name: "controls".into(),
        priority: 0,
        source: r#"
            F = vgui.Create("DFrame")
            F:SetSize(600, 500)
            LV = vgui.Create("DListView", F)
            LV:SetPos(10, 30) LV:SetSize(280, 150)
            LV:AddColumn("Name") LV:AddColumn("Size")
            LV:AddLine("b", "20") LV:AddLine("a", "3") LV:AddLine("c", "100")
            LV:SortByColumn(2)
            SEL = nil
            LV.OnRowSelected = function(_, id, line) SEL = line:GetColumnText(1) end
            LV:SelectFirstItem()
            PS = vgui.Create("DPropertySheet", F)
            PS:SetPos(300, 30) PS:SetSize(280, 150)
            P1 = vgui.Create("DPanel") P2 = vgui.Create("DPanel")
            PS:AddSheet("One", P1) PS:AddSheet("Two", P2)
            PS:SwitchToName("Two")
            T = vgui.Create("DTree", F)
            T:SetPos(10, 200) T:SetSize(280, 150)
            N = T:AddNode("root") N2 = N:AddNode("child") N:SetExpanded(true)
            PICKED = nil
            T.OnNodeSelected = function(_, n) PICKED = n:GetText() end
            N2:InternalDoClick(MOUSE_LEFT)
            CM = vgui.Create("DColorMixer", F)
            CM:SetPos(300, 200) CM:SetSize(280, 200)
            CM:SetColor(Color(10, 20, 30, 200))
            FORM = vgui.Create("DForm", F)
            FORM:SetPos(10, 360) FORM:SetSize(280, 130)
            FORM:SetName("Form")
            FORM:CheckBox("box") FORM:NumSlider("slider", nil, 0, 10, 1) FORM:TextEntry("entry") FORM:ComboBox("combo") FORM:Button("btn")
            CB = vgui.Create("DComboBox", F) CB:SetPos(10, 480) CB:AddChoice("x", 1) CB:AddChoice("y", 2, true)
            NW = vgui.Create("DNumberWang", F) NW:SetMinMax(0, 10) NW:SetValue(7)
            F:MakePopup()
        "#
        .into(),
        config: ActorConfig {
            cpu_budget: Duration::from_millis(500),
            ..ActorConfig::default()
        },
    })
    .expect("actor");
    let world = gm_net::ClientWorld::default();
    rt.cursor = Some((5.0, 5.0));
    let draws = rt.client_render(&world, 1280, 720);
    let err: String = rt.actors()[0]
        .eval("local ok, e = pcall(__derma_paint) return ok and '' or tostring(e)")
        .expect("eval");
    assert_eq!(err, "", "the vgui layer paints without a script error");
    assert!(draws.len() > 50, "the controls paint: {}", draws.len());
    let (sorted, sel, active, picked, color, choice, wang, errors): (
        String,
        String,
        String,
        String,
        String,
        String,
        f64,
        String,
    ) = rt.actors()[0]
        .eval(
            r#"
            local s = {}
            for i = 1, 3 do s[#s + 1] = LV.Sorted[i]:GetColumnText(1) end
            local c = CM:GetColor()
            return table.concat(s, ","), SEL or "", PS:GetActiveTab():GetText(), PICKED or "",
                c.r .. "," .. c.g .. "," .. c.b .. "," .. c.a, CB:GetValue(), NW:GetValue(), ""
        "#,
        )
        .expect("eval");
    assert_eq!(sorted, "a,b,c", "numeric sort by the size column");
    assert_eq!(sel, "a", "SelectFirstItem selects the first sorted line");
    assert_eq!(active, "Two", "SwitchToName activates the tab");
    assert_eq!(picked, "child", "clicking a tree node selects it");
    assert_eq!(color, "10,20,30,200", "the mixer keeps the colour set");
    assert_eq!(choice, "y", "AddChoice with select chooses it");
    assert_eq!(wang, 7.0);
    assert_eq!(errors, "");
}

/// spawnmenu registries (tool tabs / categories / options, creation tabs) and the local
/// achievement counters.
#[test]
fn glua_spawnmenu_registry_and_achievements() {
    let mut rt = Runtime::client();
    rt.load(ActorSpec {
        addon: "menu".into(),
        name: "reg".into(),
        priority: 0,
        source: r##"
            spawnmenu.AddToolMenuOption("Main", "Construction", "weld", "#tool.weld.name", "gmod_tool weld", nil, function(cp) end)
            spawnmenu.AddToolMenuOption("Main", "Construction", "rope", "#tool.rope.name", "gmod_tool rope")
            achievements.SpawnedProp() achievements.SpawnedProp()
        "##
        .into(),
        config: ActorConfig::default(),
    })
    .expect("actor");
    let (tools, cat_items, tab_icon, ach): (f64, f64, String, String) = rt.actors()[0]
        .eval(r##"
            local menu = spawnmenu.GetToolMenu("Main")
            return #spawnmenu.GetTools(), #menu[1], spawnmenu.GetCreationTabs()["#spawnmenu.content_tab"].Icon,
                achievements.GetCount(1) .. "/" .. achievements.GetGoal(1) .. "/" .. tostring(achievements.IsAchieved(1))
        "##)
        .expect("eval");
    assert_eq!(tools, 3.0, "the game's three tool tabs");
    assert_eq!(cat_items, 2.0, "two options in the Construction category");
    assert_eq!(tab_icon, "icon16/application_view_tile.png");
    assert_eq!(ach, "2/5000/false");
}

/// render.* world geometry: a box from PostDrawOpaqueRenderables arrives as 12 world
/// triangles, a line as 2, colour modulation applies.
#[test]
fn glua_render_world_geometry_reaches_the_client() {
    let mut rt = Runtime::client();
    rt.load(ActorSpec {
        addon: "fx".into(),
        name: "box".into(),
        priority: 0,
        source: r#"
            hook.Add("PostDrawOpaqueRenderables", "t", function()
                render.SetColorMaterial()
                render.DrawBox(Vector(0, 0, 0), Angle(0, 0, 0), Vector(-8, -8, -8), Vector(8, 8, 8), Color(255, 0, 0))
                render.SetColorModulation(0.5, 1, 1)
                render.DrawLine(Vector(0, 0, 0), Vector(100, 0, 0), Color(200, 200, 200))
            end)
        "#
        .into(),
        config: ActorConfig {
            cpu_budget: Duration::from_millis(500),
            ..ActorConfig::default()
        },
    })
    .expect("actor");
    let world = gm_net::ClientWorld::default();
    let draws = rt.client_render(&world, 1280, 720);
    let tris: Vec<_> = draws
        .iter()
        .filter_map(|d| match d {
            gm_script::DrawCmd::WorldTri { pts, color } => Some((pts, color)),
            _ => None,
        })
        .collect();
    assert_eq!(
        tris.len(),
        14,
        "12 box triangles + 2 line triangles: {draws:?}"
    );
    assert!(tris[..12].iter().all(|(_, c)| c[0] == 1.0 && c[1] == 0.0));
    assert!(
        tris[12].1[0] < 0.5 && tris[12].1[1] > 0.7,
        "colour modulation halves red: {:?}",
        tris[12].1
    );
    assert!(tris[..12]
        .iter()
        .all(|(p, _)| p.iter().all(|v| v.abs().max_element() == 8.0)));
}

/// Client globals: a ClientsideModel is drawn each frame, a particle emitter steps and draws
/// its particles as world quads, Vector:ToScreen projects, AddWorldTip paints a tip.
#[test]
fn glua_client_globals_models_particles_toscreen() {
    let mut rt = Runtime::client();
    rt.load(ActorSpec {
        addon: "fx".into(),
        name: "globals".into(),
        priority: 0,
        source: r#"
            CSM = ClientsideModel("models/props_c17/oildrum001.mdl")
            CSM:SetPos(Vector(100, 0, 0))
            EM = ParticleEmitter(Vector(0, 0, 0))
            local p = EM:Add("effects/yellowflare", Vector(0, 0, 10))
            p:SetDieTime(5) p:SetStartSize(4) p:SetEndSize(0) p:SetVelocity(Vector(0, 0, 50))
            SP = Vector(0, 0, 0):ToScreen()
            PT = ProjectedTexture() PT:SetBrightness(3)
        "#
        .into(),
        config: ActorConfig {
            cpu_budget: Duration::from_millis(500),
            ..ActorConfig::default()
        },
    })
    .expect("actor");
    let world = gm_net::ClientWorld::default();
    let draws = rt.client_render(&world, 1280, 720);
    assert!(
        draws.iter().any(|d| matches!(d, gm_script::DrawCmd::Model { model, pos, .. } if model.ends_with("oildrum001.mdl") && pos.x == 100.0)),
        "the clientside model is drawn: {draws:?}"
    );
    assert_eq!(
        draws
            .iter()
            .filter(|d| matches!(d, gm_script::DrawCmd::WorldTri { .. }))
            .count(),
        2,
        "one particle = one quad = two world triangles"
    );
    let (n, vis, bright, valid): (f64, bool, f64, bool) = rt.actors()[0]
        .eval("CSM:Remove() return EM:GetNumActiveParticles(), SP.visible, PT:GetBrightness(), IsValid(CSM)")
        .expect("eval");
    assert_eq!(n, 1.0);
    assert!(!vis, "the origin sits at the eye: not in front of the view");
    assert_eq!(bright, 3.0);
    assert!(!valid, "Remove invalidates the CSEnt");
    let draws = rt.client_render(&world, 1280, 720);
    assert!(
        !draws
            .iter()
            .any(|d| matches!(d, gm_script::DrawCmd::Model { .. })),
        "removed model no longer drawn"
    );
}

/// The documented enumerations exist with the game's values, and a constant another runtime
/// file defines keeps its value.
#[test]
fn glua_enums_from_the_documented_tables() {
    let mut rt = Runtime::client();
    rt.load(ActorSpec {
        addon: "e".into(),
        name: "e".into(),
        priority: 0,
        source: "".into(),
        config: ActorConfig::default(),
    })
    .expect("actor");
    let (mask, jump, blast, key_space, mt): (f64, f64, f64, f64, f64) = rt.actors()[0]
        .eval("return MASK_SHOT, IN_JUMP, DMG_BLAST, KEY_SPACE, MOVETYPE_VPHYSICS")
        .expect("eval");
    assert_eq!(
        (mask, jump, blast, key_space, mt),
        (1174421507.0, 2.0, 64.0, 65.0, 6.0)
    );
}

/// Screen scale follows the frame size, a Mesh built from triangles draws through the client,
/// `RealFrameTime()` comes from the frame clock, and `require` loads includes/modules.
#[test]
fn glua_client_globals_scale_mesh_require() {
    let mut rt = Runtime::client();
    rt.load(ActorSpec {
        addon: "g".into(),
        name: "g".into(),
        priority: 0,
        source: r##"
            m = Mesh()
            m:BuildFromTriangles({ { pos = Vector(0,0,0) }, { pos = Vector(10,0,0) }, { pos = Vector(0,10,0) } })
            drawn = 0
            hook.Add("PostDrawOpaqueRenderables", "mesh", function()
                m:Draw()
                mesh.Begin(MATERIAL_QUADS, 1)
                mesh.Position(Vector(0,0,0)) mesh.AdvanceVertex()
                mesh.Position(Vector(1,0,0)) mesh.AdvanceVertex()
                mesh.Position(Vector(1,1,0)) mesh.AdvanceVertex()
                mesh.Position(Vector(0,1,0)) mesh.AdvanceVertex()
                mesh.End()
                scale = ScreenScale(10)
                dt = RealFrameTime()
            end)
            language.Add("mytoken", "Hello")
        "##
        .into(),
        config: ActorConfig {
            cpu_budget: Duration::from_millis(500),
            ..ActorConfig::default()
        },
    })
    .expect("actor");
    let world = gm_net::ClientWorld::default();
    rt.client_render(&world, 1280, 720);
    std::thread::sleep(Duration::from_millis(5));
    let draws = rt.client_render(&world, 1280, 720);
    let tris = draws
        .iter()
        .filter(|d| matches!(d, gm_script::DrawCmd::WorldTri { .. }))
        .count();
    assert_eq!(
        tris, 3,
        "one mesh triangle plus two from the quad: {draws:?}"
    );
    let (scale, dt, phrase, loc, err): (f64, f64, String, String, bool) = rt.actors()[0]
        .eval(
            "return scale, dt, language.GetPhrase('#mytoken'), Localize('nothing', 'dflt'), pcall(require, 'no_such_module')",
        )
        .expect("eval");
    assert_eq!(scale, 20.0);
    assert!(dt > 0.0 && dt < 1.0, "{dt}");
    assert_eq!(
        (phrase.as_str(), loc.as_str(), err),
        ("Hello", "dflt", false)
    );
    let skins: f64 = rt.actors()[0]
        .eval("return NumModelSkins('models/props_c17/oildrum001.mdl')")
        .expect("skins");
    assert!(skins >= 0.0);
}

/// `sound.PlayFile` opens a channel on the host (an audio command with the script's channel
/// id), the host's state report fires the callback and feeds the getters, and the channel's
/// methods go back out as commands; a file the host cannot open fails the callback with the
/// BASS code; URLs are not streamed.
#[test]
fn glua_audio_channel_round_trip() {
    let mut rt = Runtime::client();
    rt.load(ActorSpec {
        addon: "a".into(),
        name: "a".into(),
        priority: 0,
        source: r##"
            got = {}
            hook.Add("HUDPaint", "a", function()
                if not got.started then
                    got.started = true
                    sound.PlayFile("sound/music/hl1_song3.wav", "noplay", function(ch, err, name) got.ch, got.err, got.name = ch, err, name end)
                    sound.PlayFile("sound/missing.wav", "", function(ch, err, name) got.err2, got.name2 = err, name end)
                    sound.PlayURL("http://x/y.mp3", "", function(ch, err, name) got.err3 = err end)
                elseif got.ch and not got.done then
                    got.done = true
                    got.ch:Play()
                    got.ch:SetVolume(0.5)
                    got.ch:SetTime(1.25)
                end
            end)
        "##
        .into(),
        config: ActorConfig {
            cpu_budget: Duration::from_millis(500),
            ..ActorConfig::default()
        },
    })
    .expect("actor");
    let world = gm_net::ClientWorld::default();
    let draws = rt.client_render(&world, 800, 600);
    let plays: Vec<(u32, String, bool)> = draws
        .iter()
        .filter_map(|d| match d {
            gm_script::DrawCmd::Audio {
                id,
                op: gm_script::AudioOp::Play { path, paused, .. },
            } => Some((*id, path.clone(), *paused)),
            _ => None,
        })
        .collect();
    assert_eq!(
        plays,
        vec![
            (1, "sound/music/hl1_song3.wav".to_string(), true),
            (2, "sound/missing.wav".to_string(), false)
        ]
    );
    let err3: f64 = rt.actors()[0].eval("return got.err3").expect("err3");
    assert_eq!(err3, 37.0);
    rt.client_audio(
        &world,
        &[
            gm_script::AudioState {
                id: 1,
                state: 2,
                time: 0.0,
                length: 3.5,
                rate: 44100,
                err: 0,
            },
            gm_script::AudioState {
                id: 2,
                state: 0,
                time: 0.0,
                length: 0.0,
                rate: 0,
                err: 2,
            },
        ],
    );
    let (len, state, hz, err2, name2): (f64, f64, f64, f64, String) = rt.actors()[0]
        .eval("return got.ch:GetLength(), got.ch:GetState(), got.ch:GetSamplingRate(), got.err2, got.name2")
        .expect("eval");
    assert_eq!(
        (len, state, hz, err2, name2.as_str()),
        (3.5, 2.0, 44100.0, 2.0, "BASS_ERROR_FILEOPEN")
    );
    let draws = rt.client_render(&world, 800, 600);
    let ops: Vec<String> = draws
        .iter()
        .filter_map(|d| match d {
            gm_script::DrawCmd::Audio { id: 1, op } => Some(format!("{op:?}")),
            _ => None,
        })
        .collect();
    assert_eq!(ops, vec!["Pause(false)", "Volume(0.5)", "Seek(1.25)"]);
    let state: f64 = rt.actors()[0]
        .eval("return got.ch:GetState()")
        .expect("state");
    assert_eq!(state, 1.0);
}

/// Panel hooks the tree raises: a Droppable panel dragged onto a Receiver (OnStartDragging /
/// receiver hover + drop / OnDrop / DroppedOn / OnStopDragging), a RichText click
/// (OnTextClicked), cookies loading on SetCookieName, a scroll bar appearing; plus the menu
/// libraries: permissions grants and a serverlist ping answered by the host.
#[test]
fn glua_panel_hooks_drag_richtext_cookies_servers() {
    let mut rt = Runtime::client();
    rt.load(ActorSpec {
        addon: "p".into(),
        name: "p".into(),
        priority: 0,
        source: r##"
            got = { drops = {} }
            A = vgui.Create("DButton") A:SetPos(0, 0) A:SetSize(100, 100) A:SetText("") A:Droppable("thing")
            A.OnStartDragging = function() got.start = true end
            A.OnStopDragging = function() got.stop = true end
            A.OnDrop = function() got.drop = true end
            R = vgui.Create("DPanel") R:SetPos(300, 0) R:SetSize(100, 100) R.mouse_input = true
            R:Receiver("thing", function(self, panels, dropped) got.drops[#got.drops + 1] = { dropped = dropped, n = #panels } end)
            R.DroppedOn = function(self, p) got.dropped_on = p == A end
            RT = vgui.Create("RichText") RT:SetPos(0, 300) RT:SetSize(400, 100)
            RT:InsertClickableTextStart("sig") RT:AppendText("click me") RT:InsertClickableTextEnd()
            RT.OnTextClicked = function(self, s) got.clicked = s end
            P = vgui.Create("DPanel") P:SetPos(600, 200) P.LoadCookies = function() got.cookies = true end
            P:SetCookieName("t") P:SetCookie("k", "v") got.cookie = P:GetCookie("k")
            S = vgui.Create("DScrollPanel") S:SetPos(600, 0) S:SetSize(100, 100)
            S.OnScrollbarAppear = function() got.bar = true end
            local tall = vgui.Create("DPanel", S) tall:SetSize(50, 300)
            permissions.Grant("connect", true) got.perm = permissions.IsGranted("connect")
            serverlist.PingServer("1.2.3.4", function(ping, name, desc, map, players, maxplayers) got.ping, got.sname, got.players = ping, name, players end)
        "##
        .into(),
        config: ActorConfig {
            cpu_budget: Duration::from_millis(500),
            ..ActorConfig::default()
        },
    })
    .expect("actor");
    let world = gm_net::ClientWorld::default();
    rt.cursor = Some((50.0, 50.0));
    let draws = rt.client_render(&world, 1280, 720);
    let ping: Vec<String> = draws
        .iter()
        .filter_map(|d| match d {
            gm_script::DrawCmd::Servers { id, op } => Some(format!("{id}:{op:?}")),
            _ => None,
        })
        .collect();
    assert_eq!(ping, vec!["1:Ping { addr: \"1.2.3.4\" }"]);
    rt.client_input(&world, &[("MouseLeft".into(), true)]);
    rt.cursor = Some((200.0, 50.0));
    rt.client_render(&world, 1280, 720);
    rt.cursor = Some((350.0, 50.0));
    rt.client_render(&world, 1280, 720);
    let (start, hover_n): (bool, f64) = rt.actors()[0]
        .eval("return got.start == true, #got.drops")
        .expect("eval");
    assert!(
        start && hover_n >= 1.0,
        "the drag started and hovered the receiver: {start} {hover_n}"
    );
    rt.client_input(&world, &[("MouseLeft".into(), false)]);
    let (dropped, drop, dropped_on, stop): (bool, bool, bool, bool) = rt.actors()[0]
        .eval("return got.drops[#got.drops].dropped == true, got.drop == true, got.dropped_on == true, got.stop == true")
        .expect("eval");
    assert_eq!((dropped, drop, dropped_on, stop), (true, true, true, true));
    rt.cursor = Some((6.0, 306.0));
    rt.client_render(&world, 1280, 720);
    rt.client_input(&world, &[("MouseLeft".into(), true)]);
    rt.client_input(&world, &[("MouseLeft".into(), false)]);
    rt.client_servers(
        &world,
        &[
            gm_script::ServerReport::Entry {
                id: 1,
                info: gm_script::ServerInfo {
                    address: "1.2.3.4:27015".into(),
                    name: "srv".into(),
                    players: 3,
                    ping: 40,
                    ..Default::default()
                },
            },
            gm_script::ServerReport::Done { id: 1, count: 1 },
        ],
    );
    let (clicked, cookies, cookie, bar, perm, sname, players, ping): (
        String,
        bool,
        String,
        bool,
        bool,
        String,
        f64,
        f64,
    ) = rt.actors()[0]
        .eval("return tostring(got.clicked), got.cookies == true, tostring(got.cookie), got.bar == true, got.perm == true, tostring(got.sname), got.players or -1, got.ping or -1")
        .expect("eval");
    assert_eq!(
        (
            clicked.as_str(),
            cookies,
            cookie.as_str(),
            bar,
            perm,
            sname.as_str(),
            players,
            ping
        ),
        ("sig", true, "v", true, true, "srv", 3.0, 40.0)
    );
}

/// The tool control surface and the remaining controls: ControlPanel:AddControl builds the
/// legacy controls, presets persist through the preset store, a DHScrollBar scales its grip,
/// a GWEN layout string builds children with their properties, box selection selects the
/// children in the rectangle, DHTML keeps a history and raises the loading hooks, a
/// DFileBrowser lists the data files, chat/notification draw on the HUD.
#[test]
fn glua_controls_remainder() {
    let mut rt = Runtime::client();
    rt.load(ActorSpec {
        addon: "c".into(),
        name: "c".into(),
        priority: 0,
        source: r##"
            got = {}
            CP = vgui.Create("ControlPanel")
            CP:AddControl("Slider", { Label = "Force", Command = "mytool_force", Min = 0, Max = 100 })
            CP:AddControl("CheckBox", { Label = "On", Command = "mytool_on" })
            CP:AddControl("ComboBox", { Label = "Mode", Options = { A = { mytool_mode = "1" } } })
            got.controls = #CP.Items
            presets.Add("mytool", "fast", { mytool_force = "90" })
            got.preset = presets.Exists("mytool", "fast") and presets.GetTable("mytool").fast.mytool_force
            PR = vgui.Create("ControlPresets") PR:SetPreset("mytool") PR:AddConVar("mytool_force")
            got.options = PR.Options.fast ~= nil
            HS = vgui.Create("DHScrollBar") HS:SetSize(200, 15) HS:SetUp(100, 400) HS:SetScroll(150)
            got.hscroll = HS:GetScroll()
            G = vgui.Create("DPanel") G:SetSize(300, 300)
            G:LoadGWENString(util.TableToJSON({ Page = { Children = { { Type = "Button", Properties = { Position = { x = 10, y = 20 }, Size = { w = 80, h = 30 }, Text = "Go", ControlName = "go" } } } } }))
            got.gwen = (#G.children == 1) and G.children[1]:GetClassName() .. ":" .. G.children[1].x .. ":" .. G.children[1].w .. ":" .. G.children[1]:GetName()
            SEL = vgui.Create("DPanel") SEL:SetPos(400, 400) SEL:SetSize(200, 200) SEL:SetSelectionCanvas(true)
            for i = 1, 3 do local c = vgui.Create("DPanel", SEL) c:SetPos(10 + (i - 1) * 60, 10) c:SetSize(40, 40) end
            H = vgui.Create("DHTML") H.OnDocumentReady = function(self, url) got.ready = url end
            H:OpenURL("http://a") H:OpenURL("http://b") H:GoBack()
            got.hist = H.__url .. ":" .. tostring(H:IsLoading())
            file.Write("browse/one.txt", "1") file.Write("browse/two.txt", "2")
            FB = vgui.Create("DFileBrowser") FB:SetPath("DATA") FB:SetBaseFolder("browse") FB:SetFileTypes("*.txt") FB:SetOpen(true)
            got.files = #FB.Files.Lines
            chat.AddText(Color(255, 0, 0), "hello")
            notification.AddLegacy("saved", NOTIFY_GENERIC, 5)
            M = Mesh() got.imesh = tostring(FindMetaTable("IMesh") ~= nil and M.DrawSkinned ~= nil)
        "##
        .into(),
        config: ActorConfig {
            cpu_budget: Duration::from_millis(500),
            ..ActorConfig::default()
        },
    })
    .expect("actor");
    let world = gm_net::ClientWorld::default();
    rt.cursor = Some((405.0, 405.0));
    rt.client_render(&world, 1280, 720);
    rt.actors()[0]
        .eval::<()>("SEL:StartBoxSelection()")
        .expect("start");
    rt.cursor = Some((520.0, 460.0));
    rt.client_render(&world, 1280, 720);
    let selected: f64 = rt.actors()[0]
        .eval("return SEL:EndBoxSelection()")
        .expect("end");
    assert_eq!(selected, 2.0, "two of the three children lie in the box");
    let draws = rt.client_render(&world, 1280, 720);
    let texts: Vec<String> = draws
        .iter()
        .filter_map(|d| match d {
            gm_script::DrawCmd::Text { text, .. } => Some(text.clone()),
            _ => None,
        })
        .collect();
    assert!(
        texts.iter().any(|t| t == "hello") && texts.iter().any(|t| t == "saved"),
        "{texts:?}"
    );
    let (controls, preset, options, hscroll, gwen, ready, hist, files, imesh): (
        f64,
        String,
        bool,
        f64,
        String,
        String,
        String,
        f64,
        String,
    ) = rt.actors()[0]
        .eval("return got.controls, tostring(got.preset), got.options, got.hscroll, tostring(got.gwen), tostring(got.ready), got.hist, got.files, got.imesh")
        .expect("eval");
    assert_eq!(
        (
            controls,
            preset.as_str(),
            options,
            hscroll,
            gwen.as_str(),
            ready.as_str(),
            hist.as_str(),
            files,
            imesh.as_str()
        ),
        (
            3.0,
            "90",
            true,
            150.0,
            "DButton:10:80:go",
            "http://a",
            "http://a:false",
            2.0,
            "true"
        )
    );
}

/// The remaining client libraries: a bf_read decodes little-endian fields, os.date works on
/// UTC, markup parses colour tags into a drawable object, a registered EFFECT runs through
/// Init/Think/Render, render.Model reaches the client, the view matrix sits at the eye.
#[test]
fn glua_client_libs_remainder() {
    let mut rt = Runtime::client();
    rt.load(ActorSpec {
        addon: "l".into(),
        name: "l".into(),
        priority: 0,
        source: r##"
            got = {}
            local bf = __glua_bf_read(string.char(1, 0xFE, 0xFF, 0x2A, 0, 0, 0) .. "hi\0")
            got.bf = tostring(bf:ReadBool()) .. ":" .. bf:ReadShort() .. ":" .. bf:ReadLong() .. ":" .. bf:ReadString()
            got.date = os.date("!%Y-%m-%d %H:%M:%S", 86400 * 365 + 3661)
            got.tt = os.date("*t", 0).year
            M = markup.Parse("<color=255,0,0>Red</color> and <font=Trebuchet18>big</font>", 500)
            got.mw = M:GetWidth() > 0 and M:GetHeight() > 0
            effects.Register({ Init = function(self, data) self.n = 0 got.init = data:GetOrigin().x end, Think = function(self) self.n = self.n + 1 return self.n < 3 end, Render = function(self) got.rendered = (got.rendered or 0) + 1 end }, "my_fx")
            local d = EffectData() d:SetOrigin(Vector(7, 0, 0)) util.Effect("my_fx", d)
            hook.Add("HUDPaint", "l", function() M:Draw(10, 10) render.Model({ model = "models/props_c17/oildrum001.mdl", pos = Vector(5, 0, 0), angle = Angle() }) end)
            got.view = cam.GetViewMatrix() ~= nil and cam.GetProjectionMatrix() ~= nil
            got.phrase = language.FormatPhrase("%d items", 3)
            got.exists = vgui.Exists("DButton") and not vgui.Exists("Nope")
            got.pix = util.PixelVisible(Vector(100, 0, 0), 8, util.GetPixelVisibleHandle())
        "##
        .into(),
        config: ActorConfig {
            cpu_budget: Duration::from_millis(500),
            ..ActorConfig::default()
        },
    })
    .expect("actor");
    let world = gm_net::ClientWorld::default();
    rt.client_render(&world, 1280, 720);
    rt.client_render(&world, 1280, 720);
    let draws = rt.client_render(&world, 1280, 720);
    assert!(
        draws
            .iter()
            .any(|d| matches!(d, gm_script::DrawCmd::Text { text, .. } if text.starts_with("Red"))),
        "{draws:?}"
    );
    assert!(draws
        .iter()
        .any(|d| matches!(d, gm_script::DrawCmd::Model { pos, .. } if pos.x == 5.0)));
    let (bf, date, tt, mw, init, rendered, view, phrase, exists, pix): (String, String, f64, bool, f64, f64, bool, String, bool, f64) = rt.actors()[0]
        .eval("return got.bf, got.date, got.tt, got.mw, got.init, got.rendered, got.view, got.phrase, got.exists, got.pix")
        .expect("eval");
    assert_eq!(bf.as_str(), "true:-2:42:hi");
    assert_eq!(date.as_str(), "1971-01-01 01:01:01");
    assert_eq!(
        (tt, mw, init, rendered, view, phrase.as_str(), exists),
        (1970.0, true, 7.0, 2.0, true, "3 items", true)
    );
    assert!(pix == 0.0 || pix == 1.0);
}

/// Property sheets, PropSelect, GWEN texture draws, controlpanel/search, the clipboard.
#[test]
fn glua_props_fields_remainder() {
    let mut rt = Runtime::client();
    rt.load(ActorSpec {
        addon: "q".into(),
        name: "q".into(),
        priority: 0,
        source: r##"
            got = {}
            local props = vgui.Create("DProperties")
            local row = props:CreateRow("Cat", "Speed")
            row:Setup({ type = "Float", decimals = 1 })
            row:SetValue(5)
            row.DataChanged = function(_, v) got.changed = v end
            row:ValueChanged(7)
            got.row = row:GetRow() .. ":" .. tostring(props:GetCategory("Cat") ~= nil)
            local ps = vgui.Create("PropSelect")
            ps:AddModel("models/a.mdl", { my_cv = "a" })
            ps:AddModel("models/b.mdl", { my_cv = "b" })
            ps.OnSelect = function(_, v) got.sel = v end
            ps:FindAndSelectButton("models/b.mdl")
            got.gwen = type(GWEN.CreateTextureBorder(0, 0, 64, 64, 8, 8, 8, 8)) == "function"
            got.cp = controlpanel.Get("weld") == controlpanel.Get("weld")
            search.AddProvider(function(q) return { { text = "hit:" .. q } } end, "p")
            got.search = search.GetResults("x")[1].text
            SetClipboardText("clip")
            got.clip = __glua_clipboard() .. ":" .. BRANCH .. ":" .. tostring(getfenv ~= nil)
            got.icb = (function() local c = vgui.Create("ImageCheckBox") c:SetChecked(true) return c:GetChecked() end)()
        "##
        .into(),
        config: ActorConfig {
            cpu_budget: Duration::from_millis(500),
            ..ActorConfig::default()
        },
    })
    .expect("actor");
    let (row, changed, sel, gwen, cp, search, clip, icb): (
        String,
        f64,
        String,
        bool,
        bool,
        String,
        String,
        bool,
    ) = rt.actors()[0]
        .eval(
            "return got.row, got.changed, got.sel, got.gwen, got.cp, got.search, got.clip, got.icb",
        )
        .expect("eval");
    assert_eq!(
        (
            row.as_str(),
            changed,
            sel.as_str(),
            gwen,
            cp,
            search.as_str(),
            clip.as_str(),
            icb
        ),
        (
            "Speed:true",
            7.0,
            "models/b.mdl",
            true,
            true,
            "hit:x",
            "clip:x86-64:true",
            true
        )
    );
}

/// A client EFFECT is an entity: it keeps the position / bounds its Init sets, and an effect
/// with collision bounds gets StartTouch / Touch while an entity of the world sits in its box
/// and EndTouch once it has moved off it.
#[test]
fn glua_client_effect_touch_callbacks() {
    let mut rt = Runtime::client();
    rt.load(ActorSpec {
        addon: "fx".into(),
        name: "fx".into(),
        priority: 0,
        source: r##"
            got = { start = 0, touch = 0, stop = 0 }
            effects.Register({
                Init = function(self, data) self:SetPos(data:GetOrigin()) self:SetCollisionBounds(Vector(-8, -8, -8), Vector(8, 8, 8)) self.n = 0 end,
                Think = function(self) self.n = self.n + 1 if self.n == 2 then self:SetPos(self:GetPos() + Vector(100, 0, 0)) end return self.n < 4 end,
                StartTouch = function(self, e) got.start = got.start + 1 got.who = e:EntIndex() end,
                Touch = function(self, e) got.touch = got.touch + 1 end,
                EndTouch = function(self, e) got.stop = got.stop + 1 got.pos = self:GetPos().x end,
            }, "touchy")
            local d = EffectData() d:SetOrigin(Vector(1, 2, 3)) util.Effect("touchy", d)
        "##
        .into(),
        config: ActorConfig {
            cpu_budget: Duration::from_millis(500),
            ..ActorConfig::default()
        },
    })
    .expect("actor");
    let mut world = gm_net::ClientWorld::default();
    world.entities.insert(
        3,
        gm_net::EntityState {
            id: 3,
            model: "crate".into(),
            pos: glam::Vec3::new(1.0, 2.0, 3.5),
            ..Default::default()
        },
    );
    world.own.count = 1;
    rt.client_tick(&world, true);
    for _ in 0..4 {
        rt.client_render(&world, 1280, 720);
    }
    let (start, touch, stop, who, pos): (u64, u64, u64, u64, f64) = rt.actors()[0]
        .eval("return got.start, got.touch, got.stop, got.who, got.pos")
        .expect("eval");
    // one frame in the box (StartTouch + Touch), the move of the second Think leaves it
    assert_eq!((start, touch, stop, who, pos), (1, 1, 1, 3, 101.0));
}
