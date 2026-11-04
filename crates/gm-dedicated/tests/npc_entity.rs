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
    // the game's soldier goes after a lost enemy a think later: a line-of-fire point when the
    // navmesh offers one, else the chase
    let mut chasing = None;
    for t in 0..40 {
        server.tick(&idle);
        let n = &server.npcs[&id];
        if n.goal.is_some() && (n.chase == Some(1) || n.flank_until > 0) {
            chasing = Some(t);
            break;
        }
    }
    assert!(chasing.is_some(), "never went after the hidden player");
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

/// Measured on the game's soldier (server_npc_flinch_soldier): no flinch animation; a hit
/// inside a burst ends it (the next burst comes NPC_BURST_GAP_TICKS after the last shot), a
/// hit in the gap delays the next burst by one think. In combat the soldier crouches: eyes at
/// +40, `shootSMG1c` for 7 ticks per shot, `CrouchIdle` for the rest of the gap.
#[test]
fn soldier_hit_mid_burst_ends_it_and_crouches() {
    let world = combat_server!(world);
    let (mut server, idle, _start) = combat_server!(world, server);
    let id = server.spawn_npc(1, "npc_combine_s", "weapon_smg1").unwrap();
    let rest = server.npcs[&id].sequence.clone();
    let mut shots = Vec::new();
    let mut seqs = Vec::new();
    let mut last = 0;
    let mut hit_in_burst = None;
    let mut hit_in_gap = None;
    for tick in 0..400u64 {
        server.tick(&idle);
        let n = &server.npcs[&id];
        if n.shots != last {
            shots.push(tick);
            last = n.shots;
            // the third shot of the second burst: a hit two ticks later
            if n.shots == 8 {
                hit_in_burst = Some(tick + 2);
            }
        }
        if seqs.last().map(|(_, s): &(u64, String)| s.as_str()) != Some(n.sequence.as_str()) {
            seqs.push((tick, n.sequence.clone()));
        }
        if hit_in_burst == Some(tick) || hit_in_gap == Some(tick) {
            assert!(server.damage_npc(id, 1, 10.0, 0));
        }
        // the gap after the third burst: a hit 10 ticks into it
        if last == 13 && hit_in_gap.is_none() && shots.len() == 13 {
            hit_in_gap = Some(shots[12] + 10);
        }
    }
    let gaps: Vec<u64> = shots.windows(2).map(|w| w[1] - w[0]).collect();
    eprintln!("shots {shots:?}\ngaps {gaps:?}\nseqs {seqs:?}");
    // burst 1: 5 shots, burst 2 cut to 3, burst 3: 5 shots, then the delayed burst 4
    assert_eq!(&gaps[..4], &[7, 7, 7, 7]);
    assert_eq!(gaps[4], gm_server::NPC_BURST_GAP_TICKS);
    assert_eq!(&gaps[5..7], &[7, 7], "burst 2 before the hit");
    assert_eq!(
        gaps[7],
        gm_server::NPC_BURST_GAP_TICKS,
        "the hit ended burst 2"
    );
    assert_eq!(&gaps[8..12], &[7, 7, 7, 7], "burst 3");
    assert_eq!(
        gaps[12],
        gm_server::NPC_BURST_GAP_TICKS + 7,
        "the gap hit delays one think"
    );
    // pose: crouch sequences in combat, the shoot one while firing
    assert!(seqs.iter().any(|(_, s)| *s == "shootSMG1c"));
    assert!(seqs.iter().any(|(_, s)| *s == "CrouchIdle"));
    let first_shoot = seqs.iter().find(|(_, s)| *s == "shootSMG1c").unwrap().0;
    assert_eq!(
        first_shoot, shots[0],
        "shoot sequence starts with the first shot"
    );
    // 35 ticks of shooting then CrouchIdle for the first full burst
    let after = seqs
        .iter()
        .find(|(t, s)| *t > shots[0] && *s == "CrouchIdle")
        .unwrap()
        .0;
    assert_eq!(after - shots[0], 35);
    // clearing the target restores the idle sequence
    assert!(server.npc_target(id, None));
    server.tick(&idle);
    assert_eq!(server.npcs[&id].sequence, rest);
}

/// Fast zombie (server_npc_melee_fastzombie): runs in at its measured gait, stops inside
/// 52 u and claws twice per swing, 14 ticks apart, 3 hp each; the next double hit comes
/// after the 133-tick swing and a short run.
#[test]
fn fast_zombie_runs_in_and_claws_twice() {
    let world = combat_server!(world);
    let (mut server, idle, _start) = combat_server!(world, server);
    let id = server.spawn_npc(1, "npc_fastzombie", "none").unwrap();
    let mut hits = Vec::new();
    let mut steps = Vec::new();
    let mut hp = server.players[&1].health;
    let mut last = server.npcs[&id].pos;
    for tick in 0..400u64 {
        server.tick(&idle);
        let n = &server.npcs[&id];
        let s = n.pos.truncate().distance(last.truncate());
        if s > 1e-4 {
            steps.push(s);
        }
        last = n.pos;
        let p = &server.players[&1];
        if p.health != hp {
            hits.push((tick, hp - p.health));
            hp = p.health;
        }
    }
    eprintln!("steps {:?}\nhits {hits:?}", &steps[..steps.len().min(10)]);
    // the game peaks at 21.37 u/think and brakes in two steps; our brake onset curve starts
    // a think earlier on this short run (peak 18.4) - the known softer-run-brake residual
    assert!(
        steps.iter().any(|s| *s > 18.0),
        "never reached the run gait"
    );
    let ramp: Vec<f32> = steps.windows(2).take(5).map(|w| w[1] - w[0]).collect();
    assert!(
        ramp.iter().all(|d| (*d - 2.95).abs() < 0.15),
        "ramp {ramp:?}"
    );
    assert!(hits.len() >= 4, "hits {hits:?}");
    assert!(hits.iter().all(|(_, d)| *d == 3));
    assert_eq!(hits[1].0 - hits[0].0, 14);
    assert_eq!(hits[3].0 - hits[2].0, 14);
    let period = hits[2].0 - hits[0].0;
    assert!((140..=190).contains(&period), "period {period}");
    let d = server.npcs[&id]
        .pos
        .truncate()
        .distance(server.players[&1].state.origin.truncate());
    assert!(d < 60.0, "ended {d} u away");
}

/// Headcrab (server_npc_melee_headcrab): leaps at the player on a ballistic arc, lands 5 hp
/// on touching the hull about 26 ticks in, drops, runs ~150 u away and leaps again.
#[test]
fn headcrab_leaps_hits_lands_and_leaps_again() {
    let world = combat_server!(world);
    let (mut server, idle, start) = combat_server!(world, server);
    let id = server.spawn_npc(1, "npc_headcrab", "none").unwrap();
    let mut hits = Vec::new();
    let mut leaps = Vec::new();
    let mut landings = Vec::new();
    let mut farthest = 0.0f32;
    let mut hp = server.players[&1].health;
    let mut airborne = false;
    for tick in 0..700u64 {
        server.tick(&idle);
        let n = &server.npcs[&id];
        let now_air = n.flight.is_some();
        if now_air && !airborne {
            leaps.push(tick);
        }
        if !now_air && airborne {
            landings.push((tick, n.pos));
        }
        airborne = now_air;
        let p = &server.players[&1];
        if p.health != hp {
            hits.push((tick, hp - p.health, n.pos - p.state.origin));
            hp = p.health;
        }
        if !now_air && !leaps.is_empty() {
            farthest = farthest.max(n.pos.truncate().distance(p.state.origin.truncate()));
        }
    }
    eprintln!("leaps {leaps:?}\nhits {hits:?}\nlandings {landings:?}\nfarthest {farthest:.1}");
    assert!(leaps.len() >= 2, "leaps {leaps:?}");
    assert!(hits.len() >= 2, "hits {hits:?}");
    assert!(hits.iter().all(|(_, d, _)| *d == 5));
    let flight = hits[0].0 - leaps[0];
    assert!(
        (20..=34).contains(&flight),
        "hit {flight} ticks into the leap"
    );
    assert!(
        hits[0].2.z > 40.0 && hits[0].2.z < 90.0,
        "hit the hull at {:?}",
        hits[0].2
    );
    assert!(!landings.is_empty());
    assert!(
        (landings[0].1.z - start.z).abs() < 4.0,
        "landed at {:?}",
        landings[0].1
    );
    assert!(farthest > 110.0, "ran only {farthest} u away");
    let period = leaps[1] - leaps[0];
    assert!((280..=400).contains(&period), "period {period}");
}

/// Squads (our rule): a soldier that cannot see the player takes the target of a squadmate
/// (same spawn key within NPC_SQUAD_RANGE) and chases it.
#[test]
fn squad_shares_the_target() {
    let world = combat_server!(world);
    let (mut server, idle, _start) = combat_server!(world, server);
    let a = server.spawn_npc(1, "npc_combine_s", "weapon_smg1").unwrap();
    for _ in 0..10 {
        server.tick(&idle);
    }
    assert_eq!(server.npcs[&a].target, Some(1));
    // a second soldier placed behind geometry: a navmesh spot within squad range of the
    // first that the player's eye cannot see
    let extents = Vec3::new(32.0, 32.0, 72.0);
    let eye = server.eye(1).unwrap().0;
    let a_pos = server.npcs[&a].pos;
    let hidden = world
        .nav
        .vertices()
        .iter()
        .filter(|v| {
            let d = v.distance(a_pos);
            (200.0..gm_server::NPC_SQUAD_RANGE - 10.0).contains(&d)
        })
        .filter_map(|v| world.nav.nearest(*v, extents))
        .find(|p| {
            let tr = world.collision.trace_ray(
                eye,
                *p + Vec3::new(0.0, 0.0, 36.0),
                gm_collision::MASK_PLAYERSOLID,
            );
            tr.fraction < 1.0
        })
        .expect("no hidden spot within squad range");
    let b = server.spawn_npc(1, "npc_combine_s", "weapon_smg1").unwrap();
    server.npcs.get_mut(&b).unwrap().pos = hidden;
    assert!(server.npcs[&b].target.is_none());
    let mut shared = None;
    for t in 0..30 {
        server.tick(&idle);
        let n = &server.npcs[&b];
        if n.target == Some(1) && (n.chase == Some(1) || n.goal.is_some()) {
            shared = Some(t);
            break;
        }
    }
    eprintln!("hidden {hidden} shared after {shared:?}");
    assert!(shared.is_some(), "squadmate never took the target");
}

/// Cover (our rule): a soldier hit while firing runs to the nearest navmesh vertex within
/// NPC_COVER_RANGE the player cannot see, holds NPC_COVER_TICKS without chasing, then
/// re-engages.
#[test]
fn soldier_takes_cover_when_hit() {
    let world = combat_server!(world);
    let (mut server, idle, start) = combat_server!(world, server);
    let id = server.spawn_npc(1, "npc_combine_s", "weapon_smg1").unwrap();
    let extents = Vec3::new(32.0, 32.0, 72.0);
    let s_pos = server.npcs[&id].pos;
    // a vantage for the player that sees the soldier and leaves it a cover point
    let eye_h = server.eye(1).unwrap().0.z - start.z;
    let candidates: Vec<Vec3> = world
        .nav
        .vertices()
        .iter()
        .filter(|v| {
            let d = v.truncate().distance(s_pos.truncate());
            (48.0..gm_server::NPC_COVER_RANGE).contains(&d)
        })
        .filter_map(|v| world.nav.nearest(*v, extents))
        .collect();
    let sees = |from: Vec3, to: Vec3| {
        world
            .collision
            .trace_ray(from, to, gm_collision::MASK_PLAYERSOLID)
            .fraction
            >= 1.0
    };
    let mut vantage = None;
    'scan: for gx in -8..=8 {
        for gy in -8..=8 {
            let cand = Vec3::new(gx as f32 * 160.0, gy as f32 * 160.0, -143.0);
            let Some(p) = world.nav.nearest(cand, extents) else {
                continue;
            };
            if p.distance(cand) > 64.0 || !(150.0..900.0).contains(&p.distance(s_pos)) {
                continue;
            }
            let eye = p + Vec3::new(0.0, 0.0, eye_h);
            if !sees(eye, s_pos + Vec3::new(0.0, 0.0, 40.0)) {
                continue;
            }
            if candidates
                .iter()
                .any(|c| !sees(eye, *c + Vec3::new(0.0, 0.0, 36.0)))
            {
                vantage = Some(p);
                break 'scan;
            }
        }
    }
    let vantage = vantage.expect("no vantage with a cover point for the soldier");
    server.teleport_player(1, vantage);
    for _ in 0..40 {
        server.tick(&idle);
    }
    assert_eq!(server.npcs[&id].target, Some(1));
    let shots = server.npcs[&id].shots;
    assert!(shots > 0, "no fire from the vantage");
    // 50 -> 15 hp: under NPC_COVER_HEALTH
    assert!(server.damage_npc(id, 1, 35.0, 0));
    let mut covered = None;
    for t in 0..20 {
        server.tick(&idle);
        if let Some(c) = server.npcs[&id].cover_at {
            covered = Some((t, c));
            break;
        }
    }
    let (_, cover) = covered.expect("never took cover");
    let eye = server.eye(1).unwrap().0;
    assert!(
        !sees(eye, cover + Vec3::new(0.0, 0.0, 36.0)),
        "cover {cover} is in view"
    );
    assert!(cover.distance(s_pos) <= gm_server::NPC_COVER_RANGE);
    // runs there without firing, then holds
    let shots_at_hit = server.npcs[&id].shots;
    let mut arrived = None;
    for t in 0..400 {
        server.tick(&idle);
        let n = &server.npcs[&id];
        if n.goal.is_none() && n.move_speed == 0.0 {
            arrived = Some(t);
            break;
        }
        assert_eq!(n.shots, shots_at_hit, "fired on the way to cover");
    }
    let arrived = arrived.expect("never reached cover");
    let n = &server.npcs[&id];
    let at = n.pos;
    eprintln!("cover {cover} reached after {arrived} ticks at {at}");
    assert!(
        at.truncate().distance(cover.truncate()) < 8.0,
        "stopped at {at}"
    );
    for _ in 0..(gm_server::NPC_COVER_TICKS - 20) {
        server.tick(&idle);
    }
    let n = &server.npcs[&id];
    assert_eq!(n.target, Some(1));
    assert!(
        n.chase.is_none() && n.cover_at.is_some(),
        "left cover early"
    );
    // after the stay the usual rules return: unseen target -> chase
    let mut back = false;
    for _ in 0..60 {
        server.tick(&idle);
        let n = &server.npcs[&id];
        if n.chase == Some(1) || n.shots > shots_at_hit {
            back = true;
            break;
        }
    }
    assert!(back, "never re-engaged");
}

/// Antlion (server_npc_melee_antlion): a ~100 u reach swipe for 5 alternating with an 84 u
/// pounce that lands 5 inside 55 u, hits of each kind 168 ticks apart.
#[test]
fn antlion_alternates_swipe_and_pounce() {
    let world = combat_server!(world);
    let (mut server, idle, _start) = combat_server!(world, server);
    let id = server.spawn_npc(1, "npc_antlion", "none").unwrap();
    let mut hits = Vec::new();
    let mut hp = server.players[&1].health;
    for tick in 0..700u64 {
        server.tick(&idle);
        let p = &server.players[&1];
        if p.health != hp {
            let d = server.npcs[&id]
                .pos
                .truncate()
                .distance(p.state.origin.truncate());
            hits.push((tick, hp - p.health, d));
            hp = p.health;
        }
    }
    eprintln!("hits {hits:?}");
    assert!(hits.len() >= 4, "hits {hits:?}");
    assert!(hits.iter().all(|(_, d, _)| *d == 5));
    assert!(
        hits[0].2 > 60.0 && hits[0].2 <= 101.0,
        "swipe from {}",
        hits[0].2
    );
    assert!(hits[1].2 <= 56.0, "pounce landed from {}", hits[1].2);
    let period = hits[2].0 - hits[0].0;
    assert!((140..=200).contains(&period), "period {period}");
}

/// Poison zombie (server_npc_melee_poisonzombie / _long): the throw releases a real poison
/// headcrab (NPC_CRAB_THROW_RANGE, NPC_POISON_CRABS of them, NPC_CRAB_THROW_COOLDOWN apart)
/// whose bite leaves the player at 1 hp ~187 ticks after the throw begins; the claw follows.
#[test]
fn poison_zombie_throws_then_claws() {
    let world = combat_server!(world);
    let (mut server, idle, _start) = combat_server!(world, server);
    let id = server.spawn_npc(1, "npc_poisonzombie", "none").unwrap();
    assert_eq!(server.npcs[&id].crabs, gm_server::NPC_POISON_CRABS);
    let mut events = Vec::new();
    let mut hp = server.players[&1].health;
    let mut throw_at = None;
    let mut crab_at = None;
    let mut claw_at = None;
    for tick in 0..700u64 {
        server.tick(&idle);
        let n = &server.npcs[&id];
        if throw_at.is_none() && n.melee_swing.is_some_and(|m| m.poison) {
            throw_at = Some((
                tick,
                n.pos
                    .truncate()
                    .distance(server.players[&1].state.origin.truncate()),
            ));
        }
        if crab_at.is_none() && server.npcs.len() == 2 {
            let (cid, crab) = server.npcs.iter().find(|(i, _)| **i != id).unwrap();
            assert_eq!(crab.class, "npc_headcrab_black");
            assert!(
                crab.flight.is_some() && crab.thrown,
                "crab {cid} not thrown"
            );
            assert_eq!(n.crabs, gm_server::NPC_POISON_CRABS - 1);
            crab_at = Some(tick);
        }
        if claw_at.is_none() && throw_at.is_some() && n.melee_swing.is_some_and(|m| !m.poison) {
            claw_at = Some(tick);
        }
        let p = &server.players[&1];
        if p.health != hp {
            events.push((tick, p.health));
            hp = p.health;
        }
    }
    eprintln!("throw {throw_at:?} crab {crab_at:?} claw {claw_at:?} events {events:?}");
    let (t0, d0) = throw_at.expect("never threw");
    assert!(
        d0 <= gm_server::NPC_CRAB_THROW_RANGE && d0 > 170.0,
        "threw from {d0}"
    );
    let crab = crab_at.expect("no crab");
    assert!(
        (148..=152).contains(&(crab - t0)),
        "crab released {} after the throw",
        crab - t0
    );
    assert_eq!(events[0].1, 1, "the bite leaves 1 hp: {events:?}");
    assert!(
        (175..=200).contains(&(events[0].0 - t0)),
        "bite {} after the throw",
        events[0].0 - t0
    );
    let claw = claw_at.expect("never clawed");
    assert!(claw > t0 + 168, "claw {claw} before the throw ended");
    assert!(
        events.len() >= 2 && events[1].1 < 1,
        "the claw after the bite: {events:?}"
    );
    // one crab per NPC_CRAB_THROW_COOLDOWN: no second throw inside the run
    assert_eq!(server.npcs[&id].crabs, gm_server::NPC_POISON_CRABS - 1);
    assert_eq!(server.npcs.len(), 2);
}

/// Antlion from afar (server_npc_run_antlion): runs at the measured ramp to 37.26 u per
/// think, winds up 42 ticks inside the jump range, flies at 483 u/s to the player, stands
/// 70 ticks where it lands and then starts its melee cycle.
#[test]
fn antlion_runs_and_jumps_from_afar() {
    let world = combat_server!(world);
    let (mut server, idle, _start) = combat_server!(world, server);
    let id = server.spawn_npc(1, "npc_antlion", "none").unwrap();
    let far = Vec3::new(-100.0, -500.0, -143.0);
    server.teleport_player(1, far);
    let mut steps = Vec::new();
    let mut last = server.npcs[&id].pos;
    let mut wound_at = None;
    let mut flew = 0u64;
    let mut landed_at = None;
    let mut land_dist = 0.0f32;
    let mut first_hit = None;
    let hp0 = server.players[&1].health;
    for tick in 0..900u64 {
        server.tick(&idle);
        let n = &server.npcs[&id];
        let d = n
            .pos
            .truncate()
            .distance(server.players[&1].state.origin.truncate());
        if n.flight.is_none() && landed_at.is_none() && n.pos != last {
            steps.push((tick, (n.pos - last).truncate().length(), d));
        }
        if wound_at.is_none() && n.jump_at.is_some() {
            wound_at = Some((tick, d));
        }
        if n.flight.is_some() {
            flew += 1;
        } else if flew > 0 && landed_at.is_none() {
            landed_at = Some(tick);
            land_dist = d;
        }
        if first_hit.is_none() && server.players[&1].health < hp0 {
            first_hit = Some(tick);
        }
        last = n.pos;
    }
    let peak = steps.iter().map(|s| s.1).fold(0.0f32, f32::max);
    eprintln!(
        "steps {:?} peak {peak} wound {wound_at:?} flew {flew} landed {landed_at:?} at {land_dist} hit {first_hit:?}",
        &steps[..steps.len().min(12)]
    );
    let ramp: Vec<f32> = steps.iter().take(4).map(|s| s.1).collect();
    assert!(
        ramp.len() == 4 && (ramp[1] - ramp[0] - 7.8).abs() < 0.3,
        "ramp {ramp:?}"
    );
    assert!((peak - 37.26).abs() < 0.3, "top speed {peak} u/think");
    let (t_wind, d_wind) = wound_at.expect("never wound up");
    assert!((300.0..=600.0).contains(&d_wind), "wound up at {d_wind}");
    let landed = landed_at.expect("never landed");
    assert!(flew >= 50, "flew {flew} ticks");
    assert!(land_dist <= 60.0, "landed {land_dist} u from the player");
    let after_land = steps.iter().find(|s| s.0 > landed).map(|s| s.0 - landed);
    assert!(
        after_land.is_none_or(|t| t >= 63),
        "moved {after_land:?} ticks after landing"
    );
    let hit = first_hit.expect("never hit");
    assert!(hit > t_wind + 42 + flew, "hit {hit} before the jump ended");
}

/// Suppressing fire (our rule): NPC_SUPPRESS_SHOTS bullets passing within NPC_SUPPRESS_RADIUS
/// of an unhurt soldier's chest inside NPC_SUPPRESS_WINDOW ticks send it to cover; a lone
/// near miss does not.
#[test]
fn soldier_takes_cover_under_suppressing_fire() {
    let world = combat_server!(world);
    let (mut server, idle, _start) = combat_server!(world, server);
    let id = server.spawn_npc(1, "npc_combine_s", "weapon_smg1").unwrap();
    for _ in 0..10 {
        server.tick(&idle);
    }
    assert_eq!(server.npcs[&id].target, Some(1));
    let eye = server.eye(1).unwrap().0;
    let chest = server.npcs[&id].pos + Vec3::new(0.0, 0.0, 40.0);
    // aim 30 u beside the chest: a miss inside the suppression radius
    let to = chest - eye;
    let side = Vec3::new(-to.y, to.x, 0.0).normalize() * 30.0;
    let hp = server.npcs[&id].health;
    server.fire_bullets(1, (chest + side - eye).normalize(), "Pistol", None);
    assert_eq!(server.npcs[&id].health, hp, "the near miss hit");
    assert_eq!(server.npcs[&id].suppress_hits, 1);
    assert!(!server.npcs[&id].cover_wanted);
    // the window lapses: the count restarts
    for _ in 0..gm_server::NPC_SUPPRESS_WINDOW + 1 {
        server.tick(&idle);
    }
    server.fire_bullets(1, (chest + side - eye).normalize(), "Pistol", None);
    assert_eq!(server.npcs[&id].suppress_hits, 1);
    server.fire_bullets(1, (chest - side - eye).normalize(), "Pistol", None);
    server.fire_bullets(1, (chest + side - eye).normalize(), "Pistol", None);
    assert_eq!(server.npcs[&id].health, hp, "a near miss hit");
    assert!(
        server.npcs[&id].cover_wanted,
        "three near misses did not call for cover"
    );
    // a bullet far past the soldier never counts
    assert_eq!(server.npcs[&id].suppress_hits, 0);
    server.fire_bullets(1, (chest + side * 4.0 - eye).normalize(), "Pistol", None);
    assert_eq!(server.npcs[&id].suppress_hits, 0);
}

/// Formation (our rule): two squadmates chasing one player do not stack - the second aims
/// NPC_FORMATION_SPACING beside the player and the two end up apart.
#[test]
fn squad_fans_out_on_the_chase() {
    let world = combat_server!(world);
    let (mut server, idle, start) = combat_server!(world, server);
    let a = server.spawn_npc(1, "npc_combine_s", "weapon_smg1").unwrap();
    let b = server.spawn_npc(1, "npc_combine_s", "weapon_smg1").unwrap();
    // both chase the player (forced, as an addon would)
    assert!(server.npc_chase(a, 1, true));
    assert!(server.npc_chase(b, 1, true));
    server.npcs.get_mut(&a).unwrap().passive = true;
    server.npcs.get_mut(&b).unwrap().passive = true;
    // the player steps 400 u north so the chase has somewhere to go
    server.teleport_player(1, start + Vec3::new(0.0, 400.0, 0.0));
    for _ in 0..400 {
        server.tick(&idle);
    }
    let pa = server.npcs[&a].pos;
    let pb = server.npcs[&b].pos;
    let player = server.players[&1].state.origin;
    let ga = server.npcs[&a]
        .goal
        .as_ref()
        .map(|g| g.points.last().copied());
    let gb = server.npcs[&b]
        .goal
        .as_ref()
        .map(|g| g.points.last().copied());
    eprintln!("a {pa} b {pb} player {player} goals {ga:?} {gb:?}");
    let apart = pa.truncate().distance(pb.truncate());
    assert!(apart >= 64.0, "squadmates stacked: {apart:.1} u apart");
    assert!(pa.truncate().distance(player.truncate()) < 200.0);
    assert!(pb.truncate().distance(player.truncate()) < 200.0);
}

/// Grenade threat (our rule): a live exploding projectile inside NPC_GRENADE_RANGE of an
/// armed soldier sends it to cover from the thrower before it goes off.
#[test]
fn soldier_takes_cover_from_a_live_grenade() {
    let world = combat_server!(world);
    let (mut server, idle, _start) = combat_server!(world, server);
    let id = server.spawn_npc(1, "npc_combine_s", "weapon_smg1").unwrap();
    for _ in 0..10 {
        server.tick(&idle);
    }
    assert_eq!(server.npcs[&id].target, Some(1));
    assert!(!server.npcs[&id].cover_wanted);
    // an AR2 grenade dropped 100 u beside the soldier (no physics body needed)
    let at = server.npcs[&id].pos + Vec3::new(100.0, 0.0, 40.0);
    let mut pr = gm_server::projectile::spawn(
        gm_server::weapons::Launch::Smg1Grenade,
        1,
        at,
        0.0,
        0.0,
        &world.collision,
        None,
    )
    .expect("no grenade");
    pr.velocity = Vec3::ZERO;
    server.projectiles.push(pr);
    server.tick(&idle);
    assert!(
        server.npcs[&id].cover_wanted || server.npcs[&id].cover_at.is_some(),
        "grenade ignored"
    );
    let mut covered = None;
    for t in 0..20 {
        server.tick(&idle);
        if server.npcs[&id].cover_at.is_some() {
            covered = Some(t);
            break;
        }
    }
    eprintln!("cover after {covered:?}");
    assert!(covered.is_some(), "never ran for cover");
}

#[test]
fn blood_colour_comes_from_the_npc_table_and_scripts_can_change_it() {
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
    let _hit = server.tool_hit(1).unwrap();
    let id = server.spawn_npc(1, "npc_alyx", "weapon_alyxgun").unwrap();
    assert_eq!(server.blood_color(id), Some(0), "alyx bleeds red");
    assert!(server.set_blood(id, 2));
    assert_eq!(server.blood_color(id), Some(2));
    assert!(!server.set_blood(99_999, 1), "no such NPC");
    assert_eq!(server.blood_color(99_999), None);
}

#[test]
fn npc_memory_and_wander_goals() {
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
    let _hit = server.tool_hit(1).unwrap();
    let id = server.spawn_npc(1, "npc_alyx", "weapon_alyxgun").unwrap();
    assert_eq!(server.npc_memory(id), Some((0, 0)));
    assert!(server.npc_target(id, Some(1)));
    assert!(server.damage_npc(id, 1, 1.0, 0));
    let (_, hurt) = server.npc_memory(id).unwrap();
    assert!(hurt > 0, "damage from the target is remembered: {hurt}");
    let wandered = server.npc_wander_goal(id, 64.0, 4096.0);
    assert_eq!(
        wandered,
        server.navmesh().is_some(),
        "a goal only with a navmesh"
    );
    assert!(!server.npc_wander_goal(99_999, 0.0, 100.0));
}

#[test]
fn npc_look_distance_and_world_weapon_pickup() {
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
    let _hit = server.tool_hit(1).unwrap();
    let id = server.spawn_npc(1, "npc_alyx", "weapon_alyxgun").unwrap();
    assert_eq!(server.npc_sight(id), Some(2048.0));
    assert!(server.set_npc_sight(id, 512.0));
    assert_eq!(server.npc_sight(id), Some(512.0));
    assert!(!server.set_npc_sight(99_999, 1.0));
    assert!(!server.npc_pickup_weapon(id, 777), "no such world weapon");
    server.world_weapons.insert(777, "weapon_smg1".into());
    assert!(server.npc_pickup_weapon(id, 777));
    assert!(
        !server.world_weapons.contains_key(&777),
        "the world weapon is gone"
    );
    server.world_weapons.insert(778, "weapon_bogus".into());
    assert!(
        !server.npc_pickup_weapon(id, 778),
        "a class the NPC cannot carry stays"
    );
    assert!(server.world_weapons.contains_key(&778));
}

#[test]
fn npc_ignores_a_player_until_the_tick() {
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
    let _hit = server.tool_hit(1).unwrap();
    let id = server.spawn_npc(1, "npc_alyx", "weapon_alyxgun").unwrap();
    assert!(server.npc_target(id, Some(1)));
    assert_eq!(server.npc_current_target(id), Some(1));
    assert!(server.npc_ignore_until(id, 1, 1_000_000));
    assert_eq!(
        server.npc_current_target(id),
        None,
        "the ignored target is dropped"
    );
    assert!(server.npc_ignores(id, 1, 999_999));
    assert!(
        !server.npc_ignores(id, 1, 1_000_000),
        "the ignore expires at the tick"
    );
    assert!(!server.npc_ignores(id, 2, 0));
    assert!(!server.npc_ignore_until(99_999, 1, 1));
}

// A script-owned schedule (NPC:SetSchedule(SCHED_FORCED_GO) after SetLastPosition) walks the
// engine's task list - GET_PATH_TO_SAVEPOSITION, WALK_PATH, WAIT_FOR_MOVEMENT, STOP_MOVING -
// over the native mover, ends with COND_SCHEDULE_DONE and hands the brain back; without a
// script schedule the brain mirrors the native body (IDLE_STAND while standing).
#[test]
fn npc_runs_a_scripted_engine_schedule() {
    use gm_server::ai;
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
    for _ in 0..8 {
        server.tick(&idle);
    }
    let n = &server.npcs[&id];
    assert!(!n.ai.scripted);
    assert_eq!(n.ai.schedule, Some(ai::SCHED_IDLE_STAND), "mirror of a standing body");
    assert_eq!(n.ai.state, ai::NPC_STATE_IDLE);
    let from = n.pos;
    let goal = from + Vec3::new(-400.0, 0.0, 0.0);
    assert!(server.npc_set_save_position(id, goal));
    assert!(server.npc_set_schedule(id, Some(ai::SCHED_FORCED_GO)));
    // GET_PATH / WALK_PATH chain at once (the engine's MaintainSchedule): the path exists
    // right after SetSchedule and the schedule waits for the movement
    assert!(server.npcs[&id].goal.is_some(), "{:?}", server.npcs[&id].ai);
    assert_eq!(server.npcs[&id].ai.current_task().map(|t| t.0), Some(ai::TASK_WAIT_FOR_MOVEMENT));
    let mut ticks = 0;
    let mut seen_tasks = Vec::new();
    while server.npcs[&id].ai.scripted && !server.npcs[&id].ai.schedule_done {
        server.tick(&idle);
        ticks += 1;
        let a = &server.npcs[&id].ai;
        if let Some((t, _)) = a.current_task() {
            if seen_tasks.last() != Some(&t) {
                seen_tasks.push(t);
            }
        }
        assert!(ticks < 1000, "schedule never finished: {a:?}");
    }
    let n = &server.npcs[&id];
    assert!(n.ai.has(ai::COND_SCHEDULE_DONE), "{:?}", n.ai);
    assert!(n.goal.is_none(), "still moving after STOP_MOVING");
    assert!(
        n.pos.truncate().distance(goal.truncate()) < 0.5,
        "ended {} for goal {goal}",
        n.pos
    );
    assert!(seen_tasks.contains(&ai::TASK_WAIT_FOR_MOVEMENT), "tasks seen: {seen_tasks:?}");
    eprintln!("forced go in {ticks} ticks through {seen_tasks:?}");
    assert!(server.npc_set_schedule(id, None));
    for _ in 0..8 {
        server.tick(&idle);
    }
    let n = &server.npcs[&id];
    assert!(!n.ai.scripted && n.ai.schedule == Some(ai::SCHED_IDLE_STAND), "{:?}", n.ai);
}

// Gunfire is a combat sound: an NPC in the weapon's hearing radius gets COND_HEAR_COMBAT and
// remembers the origin as its best sound for ~3 s, then forgets both.
#[test]
fn npc_hears_gunfire_and_forgets_it() {
    use gm_server::ai;
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
    server.set_weapons(1, &["weapon_pistol"]);
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
    for _ in 0..8 {
        server.tick(&idle);
    }
    assert!(!server.npcs[&id].ai.has(ai::COND_HEAR_COMBAT));
    server.fire_bullets(1, Vec3::new(0.0, 0.0, -1.0), "Pistol", None);
    for _ in 0..8 {
        server.tick(&idle);
    }
    let a = &server.npcs[&id].ai;
    assert!(a.has(ai::COND_HEAR_COMBAT), "{a:?}");
    let (pos, kind, _) = a.last_sound.expect("best sound");
    assert_eq!(kind, ai::COND_HEAR_COMBAT);
    assert!(pos.distance(start) < 80.0, "sound at {pos}");
    for _ in 0..(66 * 3 + 8) {
        server.tick(&idle);
    }
    let a = &server.npcs[&id].ai;
    assert!(!a.has(ai::COND_HEAR_COMBAT) && a.last_sound.is_none(), "{a:?}");
}

// UseFollowBehavior: an ally keeps 128..200 u behind its leader - TARGET_CHASE while far,
// TARGET_FACE once close, and it stops rather than crowding the player.
#[test]
fn npc_follow_behaviour_keeps_up_with_the_player() {
    use gm_server::ai;
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
    assert!(server.npc_set_misc(id, None, Some(Some("Follow".into())), None, None, None, None));
    let from = server.npcs[&id].pos;
    server.teleport_player(1, from + Vec3::new(-500.0, 0.0, 0.0));
    let mut ticks = 0;
    let mut chased = false;
    loop {
        server.tick(&idle);
        ticks += 1;
        let n = &server.npcs[&id];
        if n.ai.schedule == Some(ai::SCHED_TARGET_CHASE) {
            chased = true;
        }
        let d = n.pos.truncate().distance(server.players[&1].state.origin.truncate());
        if chased && n.goal.is_none() && n.ai.schedule == Some(ai::SCHED_TARGET_FACE) {
            assert!(d <= 200.0 + 16.0, "stopped {d} u away after {ticks} ticks");
            assert!(d >= 100.0, "crowding the player at {d} u");
            break;
        }
        assert!(ticks < 1500, "never caught up: {d} u, {:?}", n.ai);
    }
    eprintln!("followed in {ticks} ticks");
}

/// Animation events: the model's sequence events are parsed (a human's walk has its footstep
/// events) and server-side playback raises `GameEvent::AnimEvent` as it passes them.
#[test]
fn npc_sequence_playback_fires_the_models_animation_events() {
    let Some(inst) = gm_content::find_install() else {
        eprintln!("skipped: no Garry's Mod install");
        return;
    };
    let mount = gm_content::Mount::for_install(&inst).unwrap();
    let info = gm_dedicated::model_info(&mount, "models/humans/group01/male_07.mdl").unwrap();
    let walk = info
        .script
        .sequences
        .iter()
        .position(|s| s.eq_ignore_ascii_case("walk_all"))
        .expect("walk_all sequence");
    let events = &info.script.sequence_events[walk];
    // NPC_EVENT_LEFTFOOT / NPC_EVENT_RIGHTFOOT (old-style numbers of npcevent.h, the options
    // string is the footstep sound prefix) or the named new-style AE_NPC_*FOOT
    let is_foot =
        |e: &(f32, i32, i32, String, String)| e.4.contains("FOOT") || e.1 == 6006 || e.1 == 6007;
    assert!(events.iter().any(is_foot), "walk_all events: {events:?}");
    assert!(
        events.iter().all(|e| (0.0..=1.0).contains(&e.0)),
        "{events:?}"
    );

    let world = gm_dedicated::load_world(&mount, "gm_construct").unwrap();
    let mut server = gm_server::Server::new(&world.collision, world.physics, world.world_body);
    server.set_navmesh(world.nav.clone());
    let m2 = gm_content::Mount::for_install(&inst).unwrap();
    server.model_loader = Some(Box::new(move |m: &str| {
        gm_dedicated::model_info(&m2, m).ok()
    }));
    server.add_player(
        1,
        Vec3::new(823.235, -500.0, -143.0),
        Vec3::new(20.0, 0.0, 0.0),
    );
    let idle = std::collections::BTreeMap::new();
    for _ in 0..5 {
        server.tick(&idle);
    }
    let id = server.spawn_npc(1, "npc_citizen", "none").unwrap();
    server.tick(&idle);
    let model = server.npcs[&id].model.clone();
    let npc_info = gm_dedicated::model_info(&mount, &model).unwrap();
    let walk_name = npc_info
        .script
        .sequences
        .iter()
        .find(|s| s.eq_ignore_ascii_case("walk_all"))
        .cloned()
        .expect("npc walk_all");
    server.register_model(&model, npc_info);
    assert!(
        server.set_sequence(id, &walk_name),
        "set_sequence {walk_name} on {model}"
    );
    server.game_events.clear();
    let dt = gm_core::tick::DEFAULT_TICK_INTERVAL;
    let mut fired = Vec::new();
    for _ in 0..200 {
        assert!(server.frame_advance(id, dt));
        fired.extend(server.game_events.drain(..).filter_map(|e| match e {
            gm_server::GameEvent::AnimEvent {
                ent,
                player,
                event,
                name,
                cycle,
                time,
                ..
            } if ent == id => {
                assert!(!player);
                Some((event, name, cycle, time))
            }
            _ => None,
        }));
    }
    let feet: Vec<_> = fired
        .iter()
        .filter(|(event, name, _, _)| name.contains("FOOT") || *event == 6006 || *event == 6007)
        .collect();
    assert!(feet.len() >= 2, "fired {fired:?}");
    // each pass of the (looping) walk fires every event once
    let per_loop = events.iter().filter(|e| is_foot(e)).count();
    assert!(
        feet.len() >= per_loop,
        "{} feet in {} loops: {feet:?}",
        feet.len(),
        200
    );
    for (_, _, cycle, time) in &feet {
        assert!((0.0..=1.0).contains(cycle) && *time >= 0.0);
    }
    // the citizen's HandleAnimEvent voices the footsteps the event names
    let sounds = server.take_sounds();
    let steps: Vec<&str> = sounds
        .iter()
        .filter(|s| s.ent == id && s.name.starts_with("NPC_Citizen.Footstep"))
        .map(|s| s.name.as_str())
        .collect();
    assert!(
        steps.contains(&"NPC_Citizen.FootstepLeft") && steps.contains(&"NPC_Citizen.FootstepRight"),
        "{sounds:?}"
    );
    assert_eq!(steps.len(), feet.len(), "{steps:?}");
}

/// server_npc_zombie_swat2_crate on the game: a zombie chasing the player past a wooden crate
/// stops 2 s into the chase (schedule 95 = its SWATITEM, CHASE_ENEMY reporting as 93 on a
/// fresh server; idle one think, SCHED_WAKE_ANGRY the next) and 35 ticks later the crate
/// leaves at ~840 u/s toward the player's centre, 23 degrees up.
#[test]
fn zombie_swats_a_crate_at_the_player() {
    use gm_server::ai;
    let Some(inst) = gm_content::find_install() else {
        eprintln!("skipped: no Garry's Mod install");
        return;
    };
    let mount = gm_content::Mount::for_install(&inst).unwrap();
    let world = gm_dedicated::load_world(&mount, "gm_construct").unwrap();
    let mut server = gm_server::Server::new(&world.collision, world.physics, world.world_body);
    let m2 = gm_content::Mount::for_install(&inst).unwrap();
    server.model_loader = Some(Box::new(move |m: &str| gm_dedicated::model_info(&m2, m).ok()));
    server.set_navmesh(world.nav.clone());
    let start = Vec3::new(823.235, -500.0, -143.0);
    server.add_player(1, start, Vec3::new(12.0, 0.0, 0.0));
    server.set_weapons(1, &["gmod_tool"]);
    let look = gm_server::PlayerCmd {
        cmd: gm_move::UserCmd {
            view_angles: Vec3::new(12.0, 0.0, 0.0),
            ..Default::default()
        },
        ..Default::default()
    };
    let idle = std::collections::BTreeMap::from([(1, look)]);
    for _ in 0..20 {
        server.tick(&idle);
    }
    let crate_id = server
        .spawn_prop(1, "models/props_junk/wood_crate001a.mdl")
        .unwrap();
    assert!(server.teleport_prop(crate_id, Vec3::new(865.0, -500.0, -123.7), Vec3::ZERO));
    for _ in 0..40 {
        server.tick(&idle);
    }
    let id = server.spawn_npc(1, "npc_zombie", "none").unwrap();
    let zpos = server.npcs[&id].pos;
    assert!(zpos.x > 950.0, "zombie spawned at {zpos:?}, the crate must lie between it and the player");
    // our zombie walks straight at the player (no steering around the crate), so the player
    // stands back far enough for the crate to be 40..80 u ahead of the zombie at the 2 s scan
    server.teleport_player(1, Vec3::new(760.0, -500.0, -143.0));
    let mut chased_as = 0;
    let mut swat_at = None;
    let mut launched = None;
    for t in 0..700 {
        server.tick(&idle);
        let n = &server.npcs[&id];
        if n.ai.schedule == Some(ai::SCHED_CHASE_ENEMY) && chased_as == 0 {
            chased_as = server.npc_schedule_id(id);
        }
        if n.swat.is_some() && swat_at.is_none() {
            swat_at = Some((t, server.npc_schedule_id(id), n.sequence.clone()));
        }
        let v = server.prop_state(crate_id).unwrap().velocity;
        if v.length() > 300.0 && launched.is_none() {
            launched = Some((t, v));
        }
        if launched.is_some() && n.swat.is_none() {
            break;
        }
    }
    assert_eq!(chased_as, ai::FIRST_CLASS_SCHEDULE as i64, "CHASE_ENEMY reports as the zombie's own id");
    let (swat_tick, swat_id, seq) = swat_at.expect("the zombie never swatted the crate");
    assert_eq!(swat_id, ai::FIRST_CLASS_SCHEDULE as i64 + 2);
    assert!(seq.starts_with("swat") && seq.ends_with("low"), "{seq}");
    let (hit_tick, v) = launched.expect("the crate was never launched");
    assert_eq!(hit_tick - swat_tick, gm_server::ZOMBIE_SWAT_HIT_TICK as usize);
    assert!(v.x < -600.0 && v.z > 250.0 && v.z < 450.0 && v.y.abs() < 60.0, "launch velocity {v:?}");
    assert!((v.length() - 843.0).abs() < 60.0, "launch speed {}", v.length());
}

#[test]
fn soldier_tosses_a_grenade_at_a_visible_player() {
    let Some(inst) = gm_content::find_install() else {
        eprintln!("skipped: no Garry's Mod install");
        return;
    };
    let mount = gm_content::Mount::for_install(&inst).unwrap();
    let world = gm_dedicated::load_world(&mount, "gm_construct").unwrap();
    let mut server = gm_server::Server::new(&world.collision, world.physics, world.world_body);
    server.register_model(
        gm_server::projectile::GRENADE_MODEL,
        gm_dedicated::model_info(&mount, gm_server::projectile::GRENADE_MODEL).unwrap(),
    );
    server.set_navmesh(world.nav.clone());
    let start = Vec3::new(823.235, -500.0, -143.0);
    server.add_player(1, start, Vec3::new(12.0, 0.0, 0.0));
    server.set_weapons(1, &["gmod_tool"]);
    let look = gm_server::PlayerCmd {
        cmd: gm_move::UserCmd {
            view_angles: Vec3::new(12.0, 0.0, 0.0),
            ..Default::default()
        },
        ..Default::default()
    };
    let idle = std::collections::BTreeMap::from([(1, look)]);
    for _ in 0..20 {
        server.tick(&idle);
    }
    let id = server.spawn_npc(1, "npc_combine_s", "weapon_smg1").unwrap();
    let spos = server.npcs[&id].pos;
    // in the open, far enough that the soldier is still 256+ u away when the toss is due
    // (our soldier presses the attack on a passive player before then)
    server.teleport_player(1, Vec3::new(spos.x - 600.0, -500.0, -143.0));
    let mut first_seen = None;
    let mut thrown = None;
    let mut toss = None;
    for t in 0..900 {
        server.tick(&idle);
        let n = &server.npcs[&id];
        if n.target_first_seen > 0 && first_seen.is_none() {
            first_seen = Some(t);
        }
        if n.grenade.is_some() && thrown.is_none() {
            thrown = Some((t, n.sequence.clone()));
        }
        if let Some(pr) = server
            .projectiles
            .iter()
            .find(|p| p.kind == gm_server::weapons::Launch::Grenade)
        {
            toss = Some((t, pr.origin, pr.velocity));
            break;
        }
    }
    let first_seen = first_seen.expect("never saw the player");
    let n = &server.npcs[&id];
    let (throw_tick, seq) = thrown.unwrap_or_else(|| {
        panic!(
            "never started a toss: sched {} goal {:?} cover {:?} next_grenade {} first_seen {} dist {:.0}",
            server.npc_schedule_id(id),
            n.goal,
            n.cover_at,
            n.next_grenade,
            n.target_first_seen,
            n.pos.distance(server.players[&1].state.origin)
        )
    });
    assert_eq!(seq, "grenThrow");
    let late = throw_tick - first_seen;
    let first = gm_server::NPC_GRENADE_FIRST_TICKS as usize;
    assert!((first..=first + 14).contains(&late), "toss {late} ticks after the sighting");
    let (hit_tick, origin, v) = toss.expect("no grenade was released");
    assert_eq!(hit_tick - throw_tick, gm_server::NPC_GRENADE_RELEASE_TICK as usize);
    let n = &server.npcs[&id];
    assert!(
        (origin.x - (n.pos.x - 35.0)).abs() < 2.0 && (origin.z - (n.pos.z + 42.0)).abs() < 2.0,
        "released at {origin} for {}",
        n.pos
    );
    let range = (server.players[&1].state.origin - origin).truncate().length();
    assert!((256.0..=1024.0).contains(&range), "released {range:.0} u from the player");
    // apex 255 u: vz 553, 1.84 s of flight, the horizontal speed lands it on the player
    let flight = 2.0 * 553.0 / 600.0;
    assert!(
        v.x < 0.0 && v.y.abs() < 10.0 && (v.z - 553.0).abs() < 10.0 && (v.x.abs() - range / flight).abs() < 10.0,
        "toss velocity {v:?} for {range:.0} u"
    );
}
