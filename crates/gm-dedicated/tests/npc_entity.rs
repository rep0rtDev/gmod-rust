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

/// Locomotion on our navigation mesh: a citizen sent 400 u across the spawn ground walks
/// there corner by corner at its ground speed, stays on the floor and faces its motion.
#[test]
fn npc_walks_the_navmesh_to_a_goal() {
    let Some(inst) = gm_content::find_install() else {
        eprintln!("skipped: no Garry's Mod install");
        return;
    };
    let mount = gm_content::Mount::for_install(&inst).unwrap();
    let world = gm_dedicated::load_world(&mount, "gm_construct").unwrap();
    let mut server = gm_server::Server::new(&world.collision, world.physics, world.world_body);
    server.set_navmesh(world.nav.clone());
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
    let id = server.spawn_npc(1, "npc_citizen", "none").unwrap();
    server.tick(&idle);
    let from = server.npcs[&id].pos;
    let goal = from + Vec3::new(-400.0, 0.0, 0.0);
    assert!(
        server.npc_goto(id, goal, false),
        "no path from {from} to {goal}"
    );
    let path = server.npcs[&id].goal.clone().unwrap();
    assert!(!path.partial, "partial path {path:?}");
    let step_max = gm_server::npc_move_def(&server.npcs[&id].class).walk
        * gm_server::NPC_THINK_TICKS as f32
        * gm_core::tick::DEFAULT_TICK_INTERVAL;
    let mut ticks = 0;
    let mut last = from;
    let (mut zmin, mut zmax) = (from.z, from.z);
    while server.npcs[&id].goal.is_some() {
        server.tick(&idle);
        ticks += 1;
        let n = &server.npcs[&id];
        let moved = n.pos.truncate() - last.truncate();
        assert!(
            moved.length() <= step_max + 0.01,
            "tick {ticks}: moved {moved} > one think at walk speed"
        );
        if moved.length() > 0.1 {
            let yaw = moved.y.atan2(moved.x).to_degrees();
            let d = ((n.angles.y - yaw + 180.0).rem_euclid(360.0) - 180.0).abs();
            assert!(d < 1.0, "tick {ticks}: yaw {} vs motion {yaw}", n.angles.y);
        }
        let lift = Vec3::new(0.0, 0.0, 18.0);
        let tr = world.collision.trace_box(
            n.pos + lift,
            n.pos - Vec3::new(0.0, 0.0, 256.0),
            n.mins,
            n.maxs,
            gm_collision::MASK_PLAYERSOLID,
        );
        assert!(
            !tr.start_solid && (tr.end.z - n.pos.z).abs() < 0.5,
            "tick {ticks}: feet at z={} but the floor is at {}",
            n.pos.z,
            tr.end.z
        );
        assert!(
            (n.pos.z - last.z).abs() <= 18.0,
            "tick {ticks}: dropped {} u",
            last.z - n.pos.z
        );
        zmin = zmin.min(n.pos.z);
        zmax = zmax.max(n.pos.z);
        last = n.pos;
        assert!(ticks < 1000, "never arrived: at {}", n.pos);
    }
    let end = server.npcs[&id].pos;
    eprintln!(
        "walked {from} -> {end} in {ticks} ticks, {} corners",
        path.points.len()
    );
    assert!(
        end.truncate().distance(goal.truncate()) < 0.01,
        "ended {end} for goal {goal}"
    );
    // the game walks these 400 u in 373 ticks after the goal (server_npc_walk_citizen:
    // first think 2 ticks in, then 53 thinks of 7)
    assert_eq!(ticks, 373, "ticks vs the game's 373 for a straight walk");
}
