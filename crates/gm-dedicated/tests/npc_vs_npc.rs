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

/// A citizen and a Combine soldier in sight of each other fight (hl2_gamerules: the player's
/// allies and the Combine hate one another) with no player involved: both pick the other as
/// their target and one of them is hurt.
#[test]
fn citizen_and_soldier_fight() {
    let Some(inst) = gm_content::find_install() else {
        eprintln!("skipped: no Garry's Mod install");
        return;
    };
    let (mut server, origin) = server(&inst);
    server.add_player(1, origin, glam::Vec3::ZERO);
    server.tick(&BTreeMap::new());
    let citizen = server
        .spawn_npc(1, "npc_citizen", "weapon_smg1")
        .expect("a citizen");
    let soldier = server
        .spawn_npc(1, "npc_combine_s", "weapon_smg1")
        .expect("a soldier");
    for _ in 0..5 {
        server.tick(&BTreeMap::new());
    }
    let feet = server.npcs[&citizen].pos;
    server.npcs.get_mut(&soldier).unwrap().pos = feet + glam::Vec3::X * 300.0;
    // the player stands far off behind the construct building, out of everyone's sight
    server.players.get_mut(&1).unwrap().state.origin = glam::Vec3::new(-2000.0, 1500.0, -143.0);
    let (hc, hs) = (server.npcs[&citizen].health, server.npcs[&soldier].health);
    let mut targeted = (false, false);
    for _ in 0..400 {
        server.tick(&BTreeMap::new());
        if server.npc_current_target(citizen) == Some(soldier | gm_server::NPC_FOE) {
            targeted.0 = true;
        }
        if server.npc_current_target(soldier) == Some(citizen | gm_server::NPC_FOE) {
            targeted.1 = true;
        }
    }
    assert!(targeted.0, "the citizen never targeted the soldier");
    assert!(targeted.1, "the soldier never targeted the citizen");
    let hurt = server.npcs[&citizen].health < hc || server.npcs[&soldier].health < hs;
    assert!(
        hurt,
        "nobody was hurt in 400 ticks (citizen {hc}, soldier {hs})"
    );
}
