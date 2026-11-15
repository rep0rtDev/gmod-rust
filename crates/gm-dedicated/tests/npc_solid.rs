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

/// A prop dropped onto a standing NPC lands on its hull instead of falling through it.
#[test]
fn prop_rests_on_an_npc() {
    let Some(inst) = gm_content::find_install() else {
        eprintln!("skipped: no Garry's Mod install");
        return;
    };
    let (mut server, origin) = server(&inst);
    server.add_player(1, origin, glam::Vec3::ZERO);
    server.tick(&BTreeMap::new());
    let npc = server.spawn_npc(1, "npc_citizen", "").expect("a citizen");
    for _ in 0..10 {
        server.tick(&BTreeMap::new());
    }
    let feet = server.npcs[&npc].pos;
    let top = feet.z + server.npcs[&npc].maxs.z;
    let prop = server
        .spawn_prop(1, "models/props_junk/wood_crate001a.mdl")
        .unwrap();
    let body = server.body_of(prop).unwrap();
    server.physics.teleport(
        body,
        feet + glam::Vec3::Z * (top - feet.z + 60.0),
        glam::Quat::IDENTITY,
    );
    for _ in 0..150 {
        server.tick(&BTreeMap::new());
    }
    let z = server.physics.state(body).position.z;
    assert!(
        z > top - 2.0,
        "crate at z {z} fell through the citizen (hull top {top}, feet {feet:?})"
    );
}

/// The crowbar swing hits an NPC in reach: it takes the melee damage.
#[test]
fn crowbar_hits_an_npc() {
    let Some(inst) = gm_content::find_install() else {
        eprintln!("skipped: no Garry's Mod install");
        return;
    };
    let (mut server, origin) = server(&inst);
    server.add_player(1, origin, glam::Vec3::ZERO);
    server.tick(&BTreeMap::new());
    let npc = server.spawn_npc(1, "npc_citizen", "").expect("a citizen");
    for _ in 0..10 {
        server.tick(&BTreeMap::new());
    }
    let npc_pos = server.npcs[&npc].pos;
    let origin = npc_pos - glam::Vec3::X * 50.0;
    server.players.get_mut(&1).unwrap().state.origin = origin;
    let health = server.npcs[&npc].health;
    let eye = origin + glam::Vec3::Z * 64.0;
    let cmd = |buttons: u32| gm_server::PlayerCmd {
        cmd: gm_move::UserCmd {
            buttons,
            view_angles: aim(eye, npc_pos + glam::Vec3::Z * 40.0),
            ..Default::default()
        },
        ..Default::default()
    };
    assert!(server.select_weapon(1, "weapon_crowbar"));
    server.tick(&BTreeMap::from([(1u32, cmd(0))]));
    for _ in 0..40 {
        server.tick(&BTreeMap::from([(1u32, cmd(gm_server::IN_ATTACK))]));
    }
    let after = server.npcs[&npc].health;
    assert!(
        after < health,
        "citizen health {health} -> {after}: no crowbar hit"
    );
}
