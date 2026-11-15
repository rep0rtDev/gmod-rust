use std::collections::BTreeMap;

/// A spawned ragdoll (kleiner) can be picked up with the physgun: the beam hits a bone's
/// hull and the player holds that part.
#[test]
fn physgun_picks_up_a_spawned_ragdoll() {
    let Some(inst) = gm_content::find_install() else {
        eprintln!("skipped: no Garry's Mod install");
        return;
    };
    let mount = gm_content::Mount::for_install(&inst).unwrap();
    let world = gm_dedicated::load_world(&mount, "gm_construct").unwrap();
    let mut server = gm_server::Server::new(&world.collision, world.physics, world.world_body);
    let m2 = gm_content::Mount::for_install(&inst).unwrap();
    server.model_loader = Some(Box::new(move |m: &str| {
        gm_dedicated::model_info(&m2, m).ok()
    }));
    let origin = glam::Vec3::new(823.235, -500.0, -143.0);
    server.add_player(1, origin, glam::Vec3::new(30.0, 0.0, 0.0));
    let id = server.spawn_prop(1, "models/kleiner.mdl").unwrap();
    for _ in 0..200 {
        server.tick(&BTreeMap::new());
    }
    let root = server.body_of(id).unwrap();
    let target = server.physics.state(root).position;
    let eye = origin + glam::Vec3::Z * 64.0;
    let d = target - eye;
    let yaw = d.y.atan2(d.x).to_degrees();
    let pitch = (-d.z).atan2(d.truncate().length()).to_degrees();
    let cmd = |buttons: u32| gm_server::PlayerCmd {
        cmd: gm_move::UserCmd {
            buttons,
            view_angles: glam::Vec3::new(pitch, yaw, 0.0),
            ..Default::default()
        },
        physgun: true,
        ..Default::default()
    };
    server.tick(&BTreeMap::from([(1u32, cmd(0))]));
    for _ in 0..5 {
        server.tick(&BTreeMap::from([(1u32, cmd(gm_server::IN_ATTACK))]));
    }
    let held = server.held_point(1);
    assert!(
        held.is_some(),
        "ragdoll at {target} not held from {eye} (pitch {pitch} yaw {yaw}); classes {}",
        server.entity_class(id)
    );
}

/// Another player's death ragdoll (hl2mp_ragdoll) is picked up the same way.
#[test]
fn physgun_picks_up_a_death_ragdoll() {
    let Some(inst) = gm_content::find_install() else {
        eprintln!("skipped: no Garry's Mod install");
        return;
    };
    let mount = gm_content::Mount::for_install(&inst).unwrap();
    let world = gm_dedicated::load_world(&mount, "gm_construct").unwrap();
    let mut server = gm_server::Server::new(&world.collision, world.physics, world.world_body);
    let m2 = gm_content::Mount::for_install(&inst).unwrap();
    server.model_loader = Some(Box::new(move |m: &str| {
        gm_dedicated::model_info(&m2, m).ok()
    }));
    let origin = glam::Vec3::new(823.235, -500.0, -143.0);
    server.add_player(1, origin + glam::Vec3::X * 120.0, glam::Vec3::ZERO);
    server.add_player(2, origin, glam::Vec3::ZERO);
    server.tick(&BTreeMap::new());
    server.kill_player(1);
    for _ in 0..60 {
        server.tick(&BTreeMap::new());
    }
    let rag = server.players[&1].ragdoll_ent.expect("a death ragdoll");
    assert_eq!(server.entity_class(rag), gm_server::RAGDOLL_CLASS);
    let root = server.body_of(rag).expect("ragdoll body");
    let target = server.physics.state(root).position;
    let eye = origin + glam::Vec3::Z * 64.0;
    let d = target - eye;
    let yaw = d.y.atan2(d.x).to_degrees();
    let pitch = (-d.z).atan2(d.truncate().length()).to_degrees();
    let cmd = |buttons: u32| gm_server::PlayerCmd {
        cmd: gm_move::UserCmd {
            buttons,
            view_angles: glam::Vec3::new(pitch, yaw, 0.0),
            ..Default::default()
        },
        physgun: true,
        ..Default::default()
    };
    server.tick(&BTreeMap::from([(2u32, cmd(0))]));
    for _ in 0..5 {
        server.tick(&BTreeMap::from([(2u32, cmd(gm_server::IN_ATTACK))]));
    }
    assert!(
        server.held_point(2).is_some(),
        "death ragdoll at {target} not held from {eye} (pitch {pitch} yaw {yaw})"
    );
}
