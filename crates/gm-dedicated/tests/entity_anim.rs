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

    // collision bounds: the NPC's hull, settable (sorted), unknown entity refused
    let hull = (server.npcs[&id].mins, server.npcs[&id].maxs);
    assert_eq!(server.collision_bounds(id), Some(hull));
    assert!(hull.1.z > 60.0, "alyx stands {hull:?}");
    assert!(server.set_collision_bounds(id, Vec3::new(4.0, 4.0, 30.0), Vec3::new(-4.0, -4.0, 0.0)));
    assert_eq!(
        server.collision_bounds(id),
        Some((Vec3::new(-4.0, -4.0, 0.0), Vec3::new(4.0, 4.0, 30.0)))
    );
    assert_eq!(server.npcs[&id].maxs.z, 30.0, "the hull itself moved");
    assert!(!server.set_collision_bounds(9999, Vec3::ZERO, Vec3::ONE));

    // the surface table and spawn counts reach the script snapshot
    let text =
        String::from_utf8_lossy(&mount.read("scripts/surfaceproperties.txt").unwrap()).to_string();
    server.surface_table = Some(gm_physics::surface::SurfaceTable::parse(&[&text]));
    let snap = gm_script::Snapshot::of(&server, 0);
    let table = snap.surface_table().expect("surface table in the snapshot");
    let names = table.names();
    assert!(
        names.len() > 50 && names.windows(2).all(|w| w[0] < w[1]),
        "{}",
        names.len()
    );
    assert!(names.iter().any(|n| n == "metal") && table.get("metal").friction > 0.0);
    assert!(
        snap.start_spot.is_none(),
        "no spawn points before the BSP entities are read"
    );
    let bsp_data = mount.read("maps/gm_construct.bsp").unwrap();
    let bsp = gm_formats::bsp::Bsp::parse(&bsp_data).unwrap();
    server.set_spawn_points_from_bsp(&bsp);
    let snap = gm_script::Snapshot::of(&server, 0);
    assert!(snap.start_spot.is_some(), "gm_construct has a spawn point");
    let counts = &snap.players[&1].counts;
    assert_eq!(counts.iter().find(|c| c.0 == "npcs").map(|c| c.1), Some(1));
}

#[test]
fn sequence_movement_bone_props_and_frame_advance() {
    let Some(inst) = gm_content::find_install() else {
        eprintln!("skipped: no Garry's Mod install");
        return;
    };
    let mount = gm_content::Mount::for_install(&inst).unwrap();
    let world = gm_dedicated::load_world(&mount, "gm_construct").unwrap();
    let mut server = gm_server::Server::new(&world.collision, world.physics, world.world_body);
    let model = "models/alyx.mdl";
    let info = gm_dedicated::model_info(&mount, model).unwrap();
    let moving = info
        .script
        .sequence_move
        .iter()
        .filter(|m| glam::Vec3::from_array(**m).length() > 1.0)
        .count();
    assert!(
        moving > 5,
        "alyx has walking sequences with root movement: {moving}"
    );
    assert_eq!(info.script.bone_props.len(), info.script.bones.len());
    assert!(info.script.bone_props.iter().any(|(s, _)| !s.is_empty()));
    server.register_model(model, info);
    server.add_player(
        1,
        Vec3::new(823.235, -500.0, -143.0),
        Vec3::new(20.0, 0.0, 0.0),
    );
    server.set_weapons(1, &["gmod_tool"]);
    let id = server.spawn_npc(1, "npc_alyx", "").unwrap();
    assert!(server.frame_advance(id, 0.5));
    let c = server.anim(id).unwrap().cycle;
    assert!(c > 0.0, "frame advance moved the cycle: {c}");
    assert!(!server.frame_advance(999, 0.5));
}

#[test]
fn flex_info_materials_and_npc_took_damage() {
    let Some(inst) = gm_content::find_install() else {
        eprintln!("skipped: no Garry's Mod install");
        return;
    };
    let mount = gm_content::Mount::for_install(&inst).unwrap();
    let world = gm_dedicated::load_world(&mount, "gm_construct").unwrap();
    let mut server = gm_server::Server::new(&world.collision, world.physics, world.world_body);
    let model = "models/alyx.mdl";
    let info = gm_dedicated::model_info(&mount, model).unwrap();
    assert!(info.script.flex_info.iter().any(|(_, lo, hi, _)| hi > lo));
    assert!(
        info.script.materials.iter().any(|m| m.contains("alyx")),
        "{:?}",
        info.script.materials
    );
    server.register_model(model, info);
    server.add_player(
        1,
        Vec3::new(823.235, -500.0, -143.0),
        Vec3::new(20.0, 0.0, 0.0),
    );
    server.set_weapons(1, &["gmod_tool"]);
    let id = server.spawn_npc(1, "npc_alyx", "").unwrap();
    assert_eq!(server.npcs[&id].last_enemy_damage, 0);
    for _ in 0..3 {
        server.tick(&std::collections::BTreeMap::new());
    }
    assert!(server.npc_mark_damaged(id));
    assert!(server.npcs[&id].last_enemy_damage > 0);
    assert!(!server.npc_mark_damaged(999));
}

#[test]
fn player_gesture_layer_runs_and_autokills() {
    let Some(inst) = gm_content::find_install() else {
        eprintln!("skipped: no Garry's Mod install");
        return;
    };
    let mount = gm_content::Mount::for_install(&inst).unwrap();
    let world = gm_dedicated::load_world(&mount, "gm_construct").unwrap();
    let mut server = gm_server::Server::new(&world.collision, world.physics, world.world_body);
    server.add_player(
        1,
        Vec3::new(823.235, -500.0, -143.0),
        Vec3::new(20.0, 0.0, 0.0),
    );
    let model = server.players[&1].model.clone();
    server.register_model(&model, gm_dedicated::model_info(&mount, &model).unwrap());
    assert!(!server.player_add_gesture_by_activity(1, "ACT_NOT_A_THING", true));
    assert!(server.player_add_gesture_by_activity(1, "ACT_GMOD_GESTURE_AGREE", true));
    let g = server.player_anim(1).unwrap().gesture.clone().unwrap();
    assert!(!g.sequence.is_empty() && g.cycle == 0.0);
    for _ in 0..5 {
        server.tick(&std::collections::BTreeMap::new());
    }
    let c = server
        .player_anim(1)
        .unwrap()
        .gesture
        .as_ref()
        .unwrap()
        .cycle;
    assert!(c > 0.0, "gesture advances: {c}");
    assert!(server.player_set_gesture(1, None, Some(0.25)));
    assert_eq!(
        server
            .player_anim(1)
            .unwrap()
            .gesture
            .as_ref()
            .unwrap()
            .weight,
        0.25
    );
    for _ in 0..600 {
        server.tick(&std::collections::BTreeMap::new());
    }
    assert!(
        server.player_anim(1).is_none(),
        "autokill removed the layer"
    );
    assert!(server.player_set_gesture(1, Some(&g.sequence), None));
    assert!(server.player_remove_gesture(1));
    assert!(!server.player_remove_gesture(1));
}

#[test]
fn strip_weapon_reports_weapon_removed() {
    let Some(inst) = gm_content::find_install() else {
        eprintln!("skipped: no Garry's Mod install");
        return;
    };
    let mount = gm_content::Mount::for_install(&inst).unwrap();
    let world = gm_dedicated::load_world(&mount, "gm_construct").unwrap();
    let mut server = gm_server::Server::new(&world.collision, world.physics, world.world_body);
    server.add_player(
        1,
        Vec3::new(823.235, -500.0, -143.0),
        Vec3::new(20.0, 0.0, 0.0),
    );
    server.set_weapons(1, &["weapon_pistol", "weapon_smg1"]);
    server.game_events.clear();
    assert!(server.strip_weapon(1, Some("weapon_pistol")));
    assert!(server.game_events.iter().any(|e| matches!(e, gm_server::GameEvent::WeaponRemoved { player: 1, class } if class == "weapon_pistol")));
    assert!(!server.strip_weapon(1, Some("weapon_pistol")));
}

#[test]
fn nextbot_mover_approaches_a_point() {
    let Some(inst) = gm_content::find_install() else {
        eprintln!("skipped: no Garry's Mod install");
        return;
    };
    let mount = gm_content::Mount::for_install(&inst).unwrap();
    let world = gm_dedicated::load_world(&mount, "gm_construct").unwrap();
    let mut server = gm_server::Server::new(&world.collision, world.physics, world.world_body);
    let model = "models/props_c17/oildrum001.mdl";
    server.register_model(model, gm_dedicated::model_info(&mount, model).unwrap());
    let start = Vec3::new(823.235, -500.0, -143.0);
    server.add_player(1, start, Vec3::new(20.0, 0.0, 0.0));
    server.register_entity(gm_server::ScriptedEntity {
        class: "my_bot".into(),
        model: model.into(),
        print_name: None,
        category: None,
        think: None,
        use_type: gm_server::UseType::Simple,
    });
    let idle = std::collections::BTreeMap::from([(1, gm_server::PlayerCmd::default())]);
    let origin = start + Vec3::new(80.0, 0.0, 8.0);
    let bot = server
        .script_spawn_entity(1, "my_bot", origin, Vec3::ZERO, true, None)
        .unwrap();
    for _ in 0..20 {
        server.tick(&idle);
    }
    let p0 = server
        .sandbox
        .props()
        .iter()
        .find(|p| p.id == bot)
        .unwrap()
        .pos;
    assert!(server.nextbot_op(bot, gm_server::NextBotOp::DesiredSpeed(100.0)));
    let (mins, maxs) = (Vec3::new(-16.0, -16.0, 0.0), Vec3::new(16.0, 16.0, 72.0));
    let lift = Vec3::new(0.0, 0.0, 18.0);
    let dir = [Vec3::X, Vec3::NEG_X, Vec3::Y, Vec3::NEG_Y]
        .into_iter()
        .find(|d| {
            let tr = world.collision.trace_box(
                p0 + lift,
                p0 + lift + *d * 320.0,
                mins,
                maxs,
                gm_collision::MASK_PLAYERSOLID,
            );
            tr.fraction > 0.999 && !tr.start_solid
        })
        .expect("an open direction");
    let goal = p0 + dir * 300.0;
    for _ in 0..133 {
        server.nextbot_op(bot, gm_server::NextBotOp::Approach { pos: goal });
        server.tick(&idle);
    }
    let p1 = server
        .sandbox
        .props()
        .iter()
        .find(|p| p.id == bot)
        .unwrap()
        .pos;
    let nb = server.nextbot(bot).unwrap();
    eprintln!("bot {p0:?} -> {p1:?} nb {nb:?}");
    let along = (p1 - p0).dot(dir);
    let side = ((p1 - p0) - dir * along).truncate().length();
    assert!(
        along > 120.0 && along < 220.0,
        "{p0:?} -> {p1:?} along {dir:?}"
    );
    assert!(side < 5.0, "{p0:?} -> {p1:?}");
    assert!(nb.on_ground, "{nb:?}");
    assert_eq!(nb.stuck, 0);
    assert!(!server.nextbot_op(999, gm_server::NextBotOp::Jump));
}

#[test]
fn player_fov_and_model_scale_state() {
    let Some(inst) = gm_content::find_install() else {
        eprintln!("skipped: no Garry's Mod install");
        return;
    };
    let mount = gm_content::Mount::for_install(&inst).unwrap();
    let world = gm_dedicated::load_world(&mount, "gm_construct").unwrap();
    let mut server = gm_server::Server::new(&world.collision, world.physics, world.world_body);
    server.add_player(
        1,
        Vec3::new(823.235, -500.0, -143.0),
        Vec3::new(20.0, 0.0, 0.0),
    );
    assert!(server.set_player_fov(1, 100.0, 0.5));
    assert_eq!(server.players[&1].fov, Some((100.0, 0.5)));
    assert!(server.set_player_fov(1, 0.0, 0.0));
    assert_eq!(server.players[&1].fov, None);
    assert!(!server.set_player_fov(2, 90.0, 0.0));
    assert!(!server.set_model_scale(999, 2.0));
    assert_eq!(server.model_scale(999), 1.0);
}
