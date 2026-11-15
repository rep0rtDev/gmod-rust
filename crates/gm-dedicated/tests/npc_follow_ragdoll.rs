use std::collections::BTreeMap;

fn server(inst: &gm_content::Install) -> (gm_server::Server<'static>, glam::Vec3) {
    let mount = gm_content::Mount::for_install(inst).unwrap();
    let world = gm_dedicated::load_world(&mount, "gm_construct").unwrap();
    let collision: &'static _ = Box::leak(Box::new(world.collision));
    let mut server = gm_server::Server::new(collision, world.physics, world.world_body);
    server.set_navmesh(world.nav);
    let m2 = gm_content::Mount::for_install(inst).unwrap();
    server.model_loader = Some(Box::new(move |m: &str| {
        gm_dedicated::model_info(&m2, m).ok()
    }));
    (server, glam::Vec3::new(823.235, -500.0, -143.0))
}

fn aim(from: glam::Vec3, to: glam::Vec3) -> glam::Vec3 {
    let d = to - from;
    glam::Vec3::new(
        (-d.z).atan2(d.truncate().length()).to_degrees(),
        d.y.atan2(d.x).to_degrees(),
        0.0,
    )
}

/// +use on a citizen starts it following: moved away, the player is walked after.
#[test]
fn used_citizen_follows_the_player() {
    let Some(inst) = gm_content::find_install() else {
        eprintln!("skipped: no Garry's Mod install");
        return;
    };
    let (mut server, origin) = server(&inst);
    server.add_player(1, origin, glam::Vec3::ZERO);
    server.tick(&BTreeMap::new());
    let npc = server.spawn_npc(1, "npc_citizen", "").expect("a citizen");
    for _ in 0..30 {
        server.tick(&BTreeMap::new());
    }
    let npc_pos = server.npcs[&npc].pos;
    // within use reach (the NPC spawns ~170 u down the aim)
    let origin = npc_pos - glam::Vec3::X * 60.0;
    server.players.get_mut(&1).unwrap().state.origin = origin;
    let eye = origin + glam::Vec3::Z * 64.0;
    let cmd = |buttons: u32| gm_server::PlayerCmd {
        cmd: gm_move::UserCmd {
            buttons,
            view_angles: aim(eye, npc_pos + glam::Vec3::Z * 36.0),
            ..Default::default()
        },
        ..Default::default()
    };
    server.tick(&BTreeMap::from([(1u32, cmd(0))]));
    server.tick(&BTreeMap::from([(1u32, cmd(gm_server::IN_USE))]));
    server.tick(&BTreeMap::from([(1u32, cmd(0))]));
    assert_eq!(
        server.npcs[&npc].ai.behavior.as_deref(),
        Some("Follow"),
        "+use did not start the follow"
    );
    // the player 350 u back down the open street (the other way the nav path loops round
    // the building and the walk takes longer than the test)
    let far = origin - glam::Vec3::X * 350.0;
    server.players.get_mut(&1).unwrap().state.origin = far;
    let before = server.npcs[&npc].pos.distance(far);
    for _ in 0..400 {
        server.tick(&BTreeMap::new());
    }
    let after = server.npcs[&npc].pos.distance(far);
    assert!(
        after < before - 100.0,
        "citizen did not follow: {before} -> {after} (pos {:?})",
        server.npcs[&npc].pos
    );
}

/// A killed NPC's ragdoll is picked up with the physgun like a spawned one.
#[test]
fn physgun_picks_up_an_npc_death_ragdoll() {
    let Some(inst) = gm_content::find_install() else {
        eprintln!("skipped: no Garry's Mod install");
        return;
    };
    let (mut server, origin) = server(&inst);
    server.add_player(1, origin, glam::Vec3::ZERO);
    server.tick(&BTreeMap::new());
    let npc = server.spawn_npc(1, "npc_zombie", "").expect("a zombie");
    for _ in 0..30 {
        server.tick(&BTreeMap::new());
    }
    assert!(server.damage_npc(npc, 1, 10_000.0, 0));
    for _ in 0..90 {
        server.tick(&BTreeMap::new());
    }
    let rag = server
        .sandbox
        .props()
        .iter()
        .map(|p| p.id)
        .find(|&id| server.entity_class(id) == "prop_ragdoll")
        .expect("the zombie's ragdoll");
    let root = server.body_of(rag).expect("ragdoll body");
    let target = server.physics.state(root).position;
    let eye = origin + glam::Vec3::Z * 64.0;
    let cmd = |buttons: u32| gm_server::PlayerCmd {
        cmd: gm_move::UserCmd {
            buttons,
            view_angles: aim(eye, target),
            ..Default::default()
        },
        physgun: true,
        ..Default::default()
    };
    server.tick(&BTreeMap::from([(1u32, cmd(0))]));
    for _ in 0..5 {
        server.tick(&BTreeMap::from([(1u32, cmd(gm_server::IN_ATTACK))]));
    }
    assert!(
        server.held_point(1).is_some(),
        "ragdoll at {target} not held from {eye}"
    );
}
