//! The class behaviours of `gm_server::npc_special` on gm_construct: the floor turret deploys
//! and shoots the player, the stalker's beam burns, the crow takes off when the player is
//! near, the vortigaunt zaps a zombie, the rollermine goes for the player.
use glam::Vec3;

macro_rules! setup {
    ($server:ident, $idle:ident) => {
        let Some(inst) = gm_content::find_install() else {
            eprintln!("skipped: no Garry's Mod install");
            return;
        };
        let mount = gm_content::Mount::for_install(&inst).unwrap();
        let world = gm_dedicated::load_world(&mount, "gm_construct").unwrap();
        let mut $server = gm_server::Server::new(&world.collision, world.physics, world.world_body);
        $server.model_loader = Some(Box::new(move |m: &str| {
            gm_dedicated::model_info(&mount, m).ok()
        }));
        let start = Vec3::new(823.235, -500.0, -143.0);
        $server.add_player(1, start, Vec3::new(20.0, 0.0, 0.0));
        $server.set_weapons(1, &["gmod_tool"]);
        let look = gm_server::PlayerCmd {
            cmd: gm_move::UserCmd {
                view_angles: Vec3::new(20.0, 0.0, 0.0),
                ..Default::default()
            },
            ..Default::default()
        };
        let $idle = std::collections::BTreeMap::from([(1, look)]);
        for _ in 0..20 {
            $server.tick(&$idle);
        }
    };
}

#[test]
fn turret_deploys_and_shoots() {
    setup!(server, idle);
    let id = server.spawn_npc(1, "npc_turret_floor", "none").unwrap();
    for _ in 0..66 {
        server.tick(&idle);
    }
    assert_eq!(server.npcs[&id].special.phase, 1, "deploying");
    for _ in 0..80 {
        server.tick(&idle);
    }
    assert_eq!(server.npcs[&id].special.phase, 2, "firing");
    assert!(
        server.players[&1].health < 100,
        "{}",
        server.players[&1].health
    );
}

#[test]
fn stalker_beam_burns() {
    setup!(server, idle);
    let id = server.spawn_npc(1, "npc_stalker", "none").unwrap();
    for _ in 0..40 {
        server.tick(&idle);
    }
    assert_eq!(server.npcs[&id].special.target, Some(1));
    assert!(server.players[&1].health < 100);
}

#[test]
fn crow_takes_off_from_the_player() {
    setup!(server, idle);
    let id = server.spawn_npc(1, "npc_crow", "none").unwrap();
    for _ in 0..5 {
        server.tick(&idle);
    }
    let z0 = server.npcs[&id].pos.z;
    for _ in 0..100 {
        server.tick(&idle);
    }
    let n = &server.npcs[&id];
    assert!(n.special.phase >= 2, "{}", n.special.phase);
    assert!(n.pos.z > z0 + 20.0, "{} -> {}", z0, n.pos.z);
}

#[test]
fn vortigaunt_zaps_a_zombie() {
    setup!(server, idle);
    let v = server.spawn_npc(1, "npc_vortigaunt", "none").unwrap();
    let z = server.spawn_npc(1, "npc_zombie", "none").unwrap();
    let vpos = server.npcs[&v].pos;
    server.npcs.get_mut(&z).unwrap().pos = vpos + Vec3::new(-250.0, 0.0, 0.0);
    for _ in 0..200 {
        server.tick(&idle);
    }
    let zombie = &server.npcs[&z];
    assert!(
        zombie.health <= 0.0 || zombie.dead_at.is_some(),
        "zapped: {} hp",
        zombie.health
    );
}

#[test]
fn rollermine_goes_for_the_player() {
    setup!(server, idle);
    let id = server.spawn_npc(1, "npc_rollermine", "none").unwrap();
    for _ in 0..66 {
        server.tick(&idle);
    }
    assert_eq!(server.npcs[&id].special.target, Some(1));
}
