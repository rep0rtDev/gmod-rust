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
