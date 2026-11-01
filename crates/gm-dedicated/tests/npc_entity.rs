//! `Spawn_NPC`: the list entry's class stands on the floor at the eye trace facing the player
//! with its hull, health and weapon; bullets take `sk_plr_dmg` scaled by the hit group and the
//! class (classic zombies half); a dead NPC lingers `NPC_CORPSE_TICKS` then goes with its undo.
use glam::Vec3;

#[test]
fn npc_spawns_takes_bullets_and_dies_in_place() {
    let Some(inst) = gm_content::find_install() else {
        eprintln!("skipped: no Garry's Mod install");
        return;
    };
    let mount = gm_content::Mount::for_install(&inst).unwrap();
    let world = gm_dedicated::load_world(&mount, "gm_construct").unwrap();
    let mut server = gm_server::Server::new(&world.collision, world.physics, world.world_body);
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
    assert!(server.spawn_npc(1, "npc_bogus", "none").is_none());
    assert!(server.sandbox.undos(1).is_empty());
    let hit = server.tool_hit(1).unwrap();
    let id = server.spawn_npc(1, "npc_alyx", "weapon_alyxgun").unwrap();
    let n = server.npcs[&id].clone();
    assert_eq!(n.class, "npc_alyx");
    assert!(
        (n.pos - hit.pos).length() < 0.1,
        "{:?} vs {:?}",
        n.pos,
        hit.pos
    );
    assert!((n.angles.y - 180.0).abs() < 0.01);
    assert_eq!(n.health, 80.0);
    assert_eq!(n.weapon, "weapon_alyxgun");
    assert_eq!(n.maxs, Vec3::new(13.0, 13.0, 72.0));
    assert_eq!(server.sandbox.undos(1)[0].name, "NPC");
    // the player cannot walk through the hull
    let (eye, _) = server.eye(1).unwrap();
    let centre = n.pos + (n.mins + n.maxs) * 0.5;
    server.fire_bullets(1, (centre - eye).normalize(), "AR2", None);
    assert_eq!(
        server.npcs[&id].health, 72.0,
        "chest hit takes sk_plr_dmg_ar2"
    );
    let head = n.pos + Vec3::new(0.0, 0.0, 66.0);
    server.fire_bullets(1, (head - eye).normalize(), "Pistol", None);
    assert_eq!(server.npcs[&id].health, 62.0, "head hit doubles");
    // a zombie takes half from bullets
    let z = server.spawn_npc(1, "npc_zombie", "none").unwrap();
    assert_eq!(server.npcs[&z].health, 50.0);
    let zc = server.npcs[&z].pos + Vec3::new(0.0, 0.0, 40.0);
    // the alyx hull stands on the same spot: move the zombie aside first
    server.npcs.get_mut(&z).unwrap().pos += Vec3::new(0.0, 60.0, 0.0);
    let zc = zc + Vec3::new(0.0, 60.0, 0.0);
    server.fire_bullets(1, (zc - eye).normalize(), "Pistol", None);
    assert_eq!(server.npcs[&z].health, 47.5);
    // kill alyx: she lingers dead and unarmed, then goes with her undo entry
    server.fire_bullets(1, (centre - eye).normalize(), "357", None);
    server.fire_bullets(1, (centre - eye).normalize(), "357", None);
    let dead = server.npcs[&id].clone();
    assert!(dead.health <= 0.0 && dead.dead_at.is_some());
    assert_eq!(dead.movetype, 0);
    assert_eq!(dead.weapon, "");
    server.fire_bullets(1, (centre - eye).normalize(), "357", None);
    assert_eq!(
        server.npcs[&id].health, dead.health,
        "no damage after death"
    );
    for _ in 0..(gm_server::NPC_CORPSE_TICKS - 1) {
        server.tick(&idle);
    }
    assert!(server.npcs.contains_key(&id));
    server.tick(&idle);
    assert!(!server.npcs.contains_key(&id));
    assert!(server.npcs.contains_key(&z));
    // a save carries the live NPC (key, model, pose, health, weapon) and brings it back
    let save = server.save_world("gm_construct");
    assert_eq!(save.npcs.len(), 1);
    assert_eq!(save.npcs[0].key, "npc_zombie");
    assert_eq!(save.npcs[0].health, 47.5);
    server.cleanup(1);
    assert!(server.npcs.is_empty());
    server.load_world(&save, 1);
    assert_eq!(server.npcs.len(), 1);
    assert_eq!(server.npcs.values().next().unwrap().health, 47.5);
    // the restored zombie stays (counted like a spawned one)
    // sbox_maxnpcs counts the live ones
    server.sandbox.limits.max_npcs = 2;
    assert!(server.spawn_npc(1, "npc_kleiner", "none").is_some());
    assert!(server.spawn_npc(1, "npc_kleiner", "none").is_none());
    // the restored NPC has no undo entry (as in the game); cleanup takes everything
    server.undo(1);
    assert_eq!(server.npcs.len(), 1);
    server.cleanup(1);
    assert!(
        server.npcs.is_empty(),
        "{:?}",
        server.npcs.keys().collect::<Vec<_>>()
    );
}
