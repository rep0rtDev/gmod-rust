//! The server animator: an NPC's cycle runs from its model's fps/frames, `SetSequence` restarts
//! it, a one-shot sequence finishes and clamps, rate 0 holds, a gesture layer plays on top and
//! an autokill layer vanishes at its end, a sequence the model lacks is refused.
use glam::Vec3;

#[test]
fn npc_animator_runs_sequences_and_a_gesture_layer() {
    let Some(inst) = gm_content::find_install() else {
        eprintln!("skipped: no Garry's Mod install");
        return;
    };
    let mount = gm_content::Mount::for_install(&inst).unwrap();
    let world = gm_dedicated::load_world(&mount, "gm_construct").unwrap();
    let mut server = gm_server::Server::new(&world.collision, world.physics, world.world_body);
    let model = "models/alyx.mdl";
    server.register_model(model, gm_dedicated::model_info(&mount, model).unwrap());
    let start = Vec3::new(823.235, -500.0, -143.0);
    server.add_player(1, start, Vec3::new(20.0, 0.0, 0.0));
    server.set_weapons(1, &["gmod_tool"]);
    let look = gm_server::PlayerCmd {
        cmd: gm_move::UserCmd {
            view_angles: Vec3::new(20.0, 0.0, 0.0),
            ..Default::default()
        },
        ..Default::default()
    };
    let idle = std::collections::BTreeMap::from([(1, look)]);
    for _ in 0..20 {
        server.tick(&idle);
    }
    let id = server.spawn_npc(1, "npc_alyx", "none").unwrap();
    assert_eq!(server.npcs[&id].model, model);
    for _ in 0..10 {
        server.tick(&idle);
    }
    let a = server.anim(id).expect("an NPC has an animator");
    assert_eq!(a.sequence, server.npcs[&id].sequence);
    assert!(a.cycle > 0.0 && a.cycle < 1.0, "cycle runs: {}", a.cycle);
    assert!(!a.finished);

    // a one-shot sequence of the model: finishes after its duration and clamps at 1
    let script = &server.models[model].script;
    let (oneshot, dur) = script
        .sequences
        .iter()
        .zip(&script.sequence_anim)
        .find(|(_, (fps, frames, flags))| *fps > 0.0 && *frames > 10 && flags & 1 == 0)
        .map(|(n, (fps, frames, _))| (n.clone(), (*frames - 1) as f32 / *fps))
        .unwrap();
    assert!(!server.set_sequence(id, "no_such_sequence"));
    assert!(server.set_sequence(id, &oneshot));
    assert_eq!(server.npcs[&id].sequence, oneshot);
    let ticks = (dur / gm_core::tick::DEFAULT_TICK_INTERVAL).ceil() as usize;
    for _ in 0..ticks / 2 {
        server.tick(&idle);
    }
    let a = server.anim(id).unwrap();
    assert!(
        !a.finished && a.cycle > 0.3 && a.cycle < 0.7,
        "half way: {}",
        a.cycle
    );
    for _ in 0..ticks {
        server.tick(&idle);
    }
    let a = server.anim(id).unwrap();
    assert!(a.finished && a.cycle == 1.0, "finished: {a:?}");

    // rate 0 holds the cycle where SetCycle put it; ResetSequenceInfo clears the flag
    assert!(server.set_playback_rate(id, 0.0));
    assert!(server.set_cycle(id, 0.4));
    for _ in 0..30 {
        server.tick(&idle);
    }
    let a = server.anim(id).unwrap();
    assert!((a.cycle - 0.4).abs() < 1e-6 && !a.finished, "held: {a:?}");
    assert!(server.set_playback_rate(id, 1.0));

    // a gesture layer: autokill vanishes at its end, a kept one finishes at 1 with its weight
    assert!(!server.add_gesture(id, "no_such_sequence", true));
    assert!(server.add_gesture(id, &oneshot, true));
    server.tick(&idle);
    let g = server.anim(id).unwrap().gesture.as_ref().unwrap();
    assert!(g.cycle > 0.0 && g.weight == 1.0);
    for _ in 0..ticks + 2 {
        server.tick(&idle);
    }
    assert!(
        server.anim(id).unwrap().gesture.is_none(),
        "autokill layer gone"
    );
    assert!(server.add_gesture(id, &oneshot, false));
    assert!(server.set_gesture(
        id,
        &gm_server::GesturePatch {
            weight: Some(0.5),
            rate: Some(2.0),
            ..Default::default()
        }
    ));
    for _ in 0..ticks {
        server.tick(&idle);
    }
    let g = server.anim(id).unwrap().gesture.as_ref().unwrap();
    assert!(
        g.finished && g.cycle == 1.0 && g.weight == 0.5 && g.rate == 2.0,
        "{g:?}"
    );
    assert!(server.remove_gesture(id));
    assert!(!server.remove_gesture(id));
    assert!(server.anim(id).unwrap().gesture.is_none());
}

#[test]
fn pose_parameters_clamp_or_wrap_into_the_model_range_and_clear() {
    let Some(inst) = gm_content::find_install() else {
        eprintln!("skipped: no Garry's Mod install");
        return;
    };
    let mount = gm_content::Mount::for_install(&inst).unwrap();
    let world = gm_dedicated::load_world(&mount, "gm_construct").unwrap();
    let mut server = gm_server::Server::new(&world.collision, world.physics, world.world_body);
    let model = "models/alyx.mdl";
    server.register_model(model, gm_dedicated::model_info(&mount, model).unwrap());
    let start = Vec3::new(823.235, -500.0, -143.0);
    server.add_player(1, start, Vec3::new(20.0, 0.0, 0.0));
    server.set_weapons(1, &["gmod_tool"]);
    let look = gm_server::PlayerCmd {
        cmd: gm_move::UserCmd {
            view_angles: Vec3::new(20.0, 0.0, 0.0),
            ..Default::default()
        },
        ..Default::default()
    };
    let idle = std::collections::BTreeMap::from([(1, look)]);
    for _ in 0..20 {
        server.tick(&idle);
    }
    let id = server.spawn_npc(1, "npc_alyx", "none").unwrap();
    let params = server.models[model].script.pose_params.clone();
    let acts = &server.models[model].script.sequence_activity;
    assert_eq!(acts.len(), server.models[model].script.sequences.len());
    let tagged = acts.iter().filter(|a| a.0.starts_with("ACT_")).count();
    assert!(tagged > 50, "alyx's sequences carry activities: {tagged}");
    assert!(
        acts.iter().any(|a| a.0 == "ACT_IDLE" && a.1 >= 1),
        "an idle activity with weight"
    );
    assert!(!params.is_empty(), "alyx has pose parameters");
    let (name, lo, hi, wrap) = params
        .iter()
        .find(|p| p.3 == 0.0 && p.2 > p.1)
        .cloned()
        .expect("a clamped parameter");
    assert!(!server.set_pose_parameter(id, "no_such_param", 1.0));
    assert!(server.set_pose_parameter(id, &name.to_uppercase(), (lo + hi) * 0.5));
    assert_eq!(server.pose_parameters(id).unwrap()[&name], (lo + hi) * 0.5);
    assert!(server.set_pose_parameter(id, &name, hi + 1000.0));
    assert_eq!(
        server.pose_parameters(id).unwrap()[&name],
        hi,
        "clamped {wrap}"
    );
    if let Some((wname, wlo, whi, _)) = params.iter().find(|p| p.3 > 0.0 && p.2 > p.1) {
        assert!(server.set_pose_parameter(id, wname, whi + (whi - wlo) * 0.25));
        let v = server.pose_parameters(id).unwrap()[wname];
        assert!(
            (v - (wlo + (whi - wlo) * 0.25)).abs() < 1e-3,
            "wrapped: {v}"
        );
    }
    server.tick(&idle);
    assert!(server.pose_parameters(id).is_some(), "survives the tick");
    assert!(server.clear_pose_parameters(id));
    assert!(server.pose_parameters(id).is_none());
    assert!(!server.clear_pose_parameters(9999));
}
