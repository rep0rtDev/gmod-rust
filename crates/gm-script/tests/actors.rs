use gm_script::{api::Payload, ActorConfig, ActorSpec, ActorStatus, Runtime, Snapshot};
use std::sync::Arc;
use std::time::Duration;

fn spec(name: &str, priority: i32, source: &str) -> ActorSpec {
    ActorSpec {
        addon: "test".into(),
        name: name.into(),
        priority,
        source: source.into(),
        config: ActorConfig {
            cpu_budget: Duration::from_millis(5),
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
    rt.load(spec(
        "spin",
        0,
        "events.on('tick.pre', function() while true do end end)",
    ))
    .unwrap();
    rt.load(spec(
        "fine",
        0,
        "events.on('tick.pre', function() state.n = (state.n or 0) + 1 end)",
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
        rt.load(spec("a", 0, src)).unwrap();
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
    rt.load(spec("a", 0, src)).unwrap();
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
