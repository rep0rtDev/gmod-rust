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
    let goto_tick = server.tick_count();
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
    // the game walks these 400 u in 54 thinks (server_npc_walk_citizen*): the first on the
    // NPC's think grid after the goal, then 53 more every 7 ticks
    let spawn_tick = server.npcs[&id].spawn_tick;
    let first = (1..=7)
        .find(|d| (goto_tick + d - spawn_tick) % 7 == gm_server::NPC_THINK_PHASE)
        .unwrap() as i32;
    assert_eq!(
        ticks,
        first + 53 * 7,
        "ticks vs the game's grid for a straight walk"
    );
}

#[test]
fn npc_chases_a_moving_player() {
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
    // the player steps 400 u west; the NPC follows and rests NPC_CHASE_REACH away
    let away = start + Vec3::new(-400.0, 0.0, 0.0);
    server.teleport_player(1, away);
    assert!(server.npc_chase(id, 1, false));
    let mut ticks = 0;
    loop {
        server.tick(&idle);
        ticks += 1;
        let n = &server.npcs[&id];
        if n.goal.is_none() && n.move_speed == 0.0 && ticks > 20 {
            break;
        }
        assert!(ticks < 1500, "never settled: at {}", n.pos);
    }
    let player = server.players[&1].state.origin;
    let d = server.npcs[&id].pos.truncate().distance(player.truncate());
    eprintln!("rested {d:.2} u from the player after {ticks} ticks");
    assert!(
        (d - gm_server::NPC_CHASE_REACH).abs() < 1.0,
        "rested {d} u from the player"
    );
    // the player moves on: the NPC re-paths on its think and walks again without a halt
    server.teleport_player(1, away + Vec3::new(-300.0, 0.0, 0.0));
    let mut last = server.npcs[&id].pos;
    let mut steps = Vec::new();
    for _ in 0..200 {
        server.tick(&idle);
        let p = server.npcs[&id].pos;
        let s = p.truncate().distance(last.truncate());
        if s > 1e-4 {
            steps.push(s);
        }
        last = p;
    }
    eprintln!("steps after the move: {:?}", &steps[..steps.len().min(8)]);
    assert!(steps.len() >= 8, "did not follow: {steps:?}");
    for w in steps[..6].windows(2) {
        assert!(w[1] > w[0], "stalled while following: {steps:?}");
    }
    assert!(server.npcs[&id].chase == Some(1));
}

/// gm_construct, the player at the spawn hub looking 20 deg down: the NPC lands 178 u ahead.
/// `$world` is bound in the caller's scope so the server may borrow it.
macro_rules! combat_server {
    ($world:ident) => {{
        let Some(inst) = gm_content::find_install() else {
            eprintln!("skipped: no Garry's Mod install");
            return;
        };
        let mount = gm_content::Mount::for_install(&inst).unwrap();
        gm_dedicated::load_world(&mount, "gm_construct").unwrap()
    }};
    ($world:ident, server) => {{
        let mut server =
            gm_server::Server::new(&$world.collision, $world.physics, $world.world_body);
        server.set_navmesh($world.nav.clone());
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
        (server, idle, start)
    }};
}

/// Our combat model: a hostile NPC with a hitscan weapon fires at a visible player one shot
/// per think, NPC_BURST_SHOTS per burst, NPC_BURST_GAP_TICKS between bursts; every hit takes
/// the ammo's NPC damage off the player.
#[test]
fn armed_soldier_fires_bursts_at_a_visible_player() {
    let world = combat_server!(world);
    let (mut server, idle, start) = combat_server!(world, server);
    let id = server.spawn_npc(1, "npc_combine_s", "weapon_smg1").unwrap();
    let hp0 = server.players[&1].health;
    let mut shots = Vec::new();
    let mut last = server.npcs[&id].shots;
    for tick in 0..300u64 {
        server.tick(&idle);
        let n = &server.npcs[&id];
        if n.shots != last {
            shots.push(tick);
            last = n.shots;
        }
    }
    let n = &server.npcs[&id];
    assert_eq!(n.target, Some(1));
    assert!(n.goal.is_none(), "stood still while the player is in sight");
    let gaps: Vec<u64> = shots.windows(2).map(|w| w[1] - w[0]).collect();
    eprintln!("shots at {shots:?}\ngaps {gaps:?}");
    assert!(
        server.players[&1].alive,
        "the player died inside the window"
    );
    assert!(shots.len() >= 20, "only {} shots in 300 ticks", shots.len());
    let burst = gm_server::NPC_BURST_SHOTS as usize;
    for (i, g) in gaps.iter().enumerate() {
        let expect = if (i + 1) % burst == 0 {
            gm_server::NPC_BURST_GAP_TICKS
        } else {
            7
        };
        assert_eq!(*g, expect, "gap {i} of {gaps:?}");
    }
    // the player stands 178 u away inside the aim cone: most shots land, 3 hp each
    let hp = server.players[&1].health;
    let lost = hp0 - hp;
    eprintln!("player {hp0} -> {hp} after {} shots", shots.len());
    assert!(lost > 0 && lost % 3 == 0, "lost {lost} hp");
    assert!(
        lost as usize >= shots.len() * 3 / 2,
        "only {lost} hp off {} shots",
        shots.len()
    );
    // facing: the soldier spawned looking at the player, yaw stays on them
    let to = start - server.npcs[&id].pos;
    let want = to.y.atan2(to.x).to_degrees();
    let yaw = server.npcs[&id].angles.y;
    assert!((want - yaw).abs() < 1.0, "yaw {yaw} vs {want}");
}

/// Losing sight of the target sends the NPC after the player's position; sight regained
/// stops the chase, and the target is dropped after NPC_TARGET_LOSE_TICKS unseen.
#[test]
fn soldier_chases_a_target_out_of_sight_then_gives_up() {
    let world = combat_server!(world);
    let (mut server, idle, start) = combat_server!(world, server);
    let id = server.spawn_npc(1, "npc_combine_s", "weapon_smg1").unwrap();
    for _ in 0..30 {
        server.tick(&idle);
    }
    assert_eq!(server.npcs[&id].target, Some(1));
    // a reachable spot on the navmesh the soldier's eye cannot see
    let extents = Vec3::new(32.0, 32.0, 72.0);
    let npc_pos = server.npcs[&id].pos;
    let eye = npc_pos + Vec3::new(0.0, 0.0, 64.0);
    let mut hidden = None;
    'scan: for gx in -8..=8 {
        for gy in -8..=8 {
            let cand = Vec3::new(gx as f32 * 192.0, gy as f32 * 192.0, -143.0);
            let Some(p) = world.nav.nearest(cand, extents) else {
                continue;
            };
            let d = p.distance(npc_pos);
            if p.distance(cand) > 64.0 || !(300.0..1500.0).contains(&d) {
                continue;
            }
            let chest = p + Vec3::new(0.0, 0.0, 36.0);
            let tr = world
                .collision
                .trace_ray(eye, chest, gm_collision::MASK_PLAYERSOLID);
            if tr.fraction < 1.0 && world.nav.find_path(npc_pos, p, extents).is_some() {
                hidden = Some(p);
                break 'scan;
            }
        }
    }
    let hidden = hidden.expect("no hidden navmesh spot within 1500 u");
    eprintln!(
        "hidden spot {hidden} ({:.0} u away)",
        hidden.distance(npc_pos)
    );
    server.teleport_player(1, hidden);
    let mut chasing = None;
    for t in 0..40 {
        server.tick(&idle);
        let n = &server.npcs[&id];
        if n.chase == Some(1) && n.goal.is_some() {
            chasing = Some(t);
            break;
        }
    }
    assert!(chasing.is_some(), "never chased the hidden player");
    let shots = server.npcs[&id].shots;
    for _ in 0..21 {
        server.tick(&idle);
    }
    assert_eq!(server.npcs[&id].shots, shots, "fired without sight");
    assert!(server.npcs[&id].move_speed > 0.0, "did not move");
    // the player comes back into view: the chase ends and fire resumes
    server.teleport_player(1, start);
    let mut seen = false;
    for _ in 0..60 {
        server.tick(&idle);
        let n = &server.npcs[&id];
        if n.chase.is_none() && n.shots > shots {
            seen = true;
            break;
        }
    }
    assert!(seen, "did not re-engage");
    // out of sight for good: after NPC_TARGET_LOSE_TICKS the target is dropped
    let far = Vec3::new(-2500.0, 2500.0, -143.0);
    server.teleport_player(1, far);
    let seen_at = server.npcs[&id].target_seen;
    let mut dropped = None;
    for t in 0..(gm_server::NPC_TARGET_LOSE_TICKS + 40) {
        server.tick(&idle);
        let n = &server.npcs[&id];
        if n.target.is_none() {
            dropped = Some(t);
            break;
        }
        assert_eq!(n.target_seen, seen_at, "saw the player at {far}");
    }
    let n = &server.npcs[&id];
    eprintln!(
        "dropped after {dropped:?} ticks: chase {:?} at {}",
        n.chase, n.pos
    );
    assert!(dropped.is_some_and(|t| t >= gm_server::NPC_TARGET_LOSE_TICKS));
    assert!(n.chase.is_none() && n.goal.is_none());
}

/// Melee: a zombie walks up at its measured gait, swings inside its reach, lands 25 DMG_SLASH
/// 48 ticks into the swing and launches the player away, then closes in and swings again
/// (server_npc_melee_zombie: hits 196 ticks apart).
#[test]
fn zombie_closes_in_and_claws_the_player() {
    let world = combat_server!(world);
    let (mut server, idle, start) = combat_server!(world, server);
    let id = server.spawn_npc(1, "npc_zombie", "none").unwrap();
    let melee = gm_server::npc_melee("npc_zombie").unwrap();
    let mut hits = Vec::new();
    let mut steps = Vec::new();
    let mut swing_dist = Vec::new();
    let mut hp = server.players[&1].health;
    let mut last = server.npcs[&id].pos;
    let mut swinging = false;
    for tick in 0..600u64 {
        server.tick(&idle);
        let n = &server.npcs[&id];
        let p = &server.players[&1];
        let s = n.pos.truncate().distance(last.truncate());
        if s > 1e-4 {
            steps.push(s);
        }
        last = n.pos;
        let now_swinging = n.melee_hit_at.is_some();
        if now_swinging && !swinging {
            swing_dist.push(n.pos.truncate().distance(p.state.origin.truncate()));
        }
        swinging = now_swinging;
        if p.health != hp {
            hits.push((tick, hp - p.health, p.state.velocity.truncate().length()));
            hp = p.health;
        }
    }
    eprintln!(
        "steps {:?}\nswings at {swing_dist:?}\nhits {hits:?}",
        &steps[..steps.len().min(8)]
    );
    assert_eq!(server.npcs[&id].target, Some(1));
    let steady = steps.iter().filter(|s| (**s - 4.73).abs() < 0.1).count();
    assert!(steady >= 10, "zombie gait: {steps:?}");
    assert!(!swing_dist.is_empty(), "never swung");
    for d in &swing_dist {
        assert!(
            *d <= melee.reach && *d > melee.reach - 8.0,
            "swung at {d} u"
        );
    }
    assert!(hits.len() >= 2, "hits {hits:?}");
    for (_, dmg, v) in &hits {
        assert_eq!(*dmg, melee.damage as i32);
        assert!(*v > 250.0 && *v <= melee.punch + 1.0, "launched at {v} u/s");
    }
    let gap = hits[1].0 - hits[0].0;
    assert!((150..=260).contains(&gap), "hits {gap} ticks apart");
    assert!(
        server.players[&1].state.origin.x < start.x - 40.0,
        "player not pushed west"
    );
}

/// Addon control: `npc_target` clear makes an engaged soldier passive (no fire, no chase)
/// until the next `npc_target`, which can also force a farther player over the nearest.
#[test]
fn npc_target_overrides_and_clears_the_combat_target() {
    let world = combat_server!(world);
    let (mut server, idle, start) = combat_server!(world, server);
    server.add_player(
        2,
        start + Vec3::new(0.0, 200.0, 0.0),
        Vec3::new(20.0, 0.0, 0.0),
    );
    server.set_weapons(2, &["gmod_tool"]);
    let mut cmds = idle.clone();
    cmds.insert(2, idle[&1].clone());
    let id = server.spawn_npc(1, "npc_combine_s", "weapon_smg1").unwrap();
    for _ in 0..40 {
        server.tick(&cmds);
    }
    assert_eq!(server.npcs[&id].target, Some(1), "nearest player first");
    assert!(server.npcs[&id].shots > 0);
    // clear: passive
    assert!(server.npc_target(id, None));
    let shots = server.npcs[&id].shots;
    let hp = (server.players[&1].health, server.players[&2].health);
    for _ in 0..100 {
        server.tick(&cmds);
    }
    let n = &server.npcs[&id];
    assert!(n.passive && n.target.is_none() && n.goal.is_none());
    assert_eq!(n.shots, shots, "fired while passive");
    assert_eq!((server.players[&1].health, server.players[&2].health), hp);
    // force the farther player 2
    assert!(server.npc_target(id, Some(2)));
    let hp2 = server.players[&2].health;
    for _ in 0..100 {
        server.tick(&cmds);
    }
    let n = &server.npcs[&id];
    assert_eq!(n.target, Some(2));
    assert!(n.shots > shots, "did not fire at the forced target");
    assert!(server.players[&2].health < hp2, "player 2 untouched");
    assert_eq!(server.players[&1].health, hp.0, "player 1 was hit");
    // rejections
    assert!(!server.npc_target(id, Some(9)));
    assert!(!server.npc_target(999, None));
}
