//! `Spawn_Vehicle`: the list entry's model stands as a physics prop at the eye trace facing
//! the player; +use seats the player on `vehicle_feet_passenger0`, +use again puts them out
//! 100 u to the vehicle's left (chairs / pods); undo "Vehicle", `sbox_maxvehicles` 4.
use glam::Vec3;

#[test]
fn chair_spawns_seats_and_releases_the_player() {
    let Some(inst) = gm_content::find_install() else {
        eprintln!("skipped: no Garry's Mod install");
        return;
    };
    let mount = gm_content::Mount::for_install(&inst).unwrap();
    let world = gm_dedicated::load_world(&mount, "gm_construct").unwrap();
    let mut server = gm_server::Server::new(&world.collision, world.physics, world.world_body);
    for m in ["models/nova/chair_wood01.mdl", "models/buggy.mdl"] {
        match gm_dedicated::model_info(&mount, m) {
            Ok(info) => {
                eprintln!("{m}: {} attachments", info.attachments.len());
                server.register_model(m, info);
            }
            Err(e) => panic!("{m}: {e}"),
        }
    }
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
    assert!(server.spawn_vehicle(1, "Bogus").is_none());
    let hit = server.tool_hit(1).unwrap();
    server.request_hook = Some(Box::new(|_, req| match req {
        gm_server::Request::Suicide { .. } | gm_server::Request::SwitchWeapon { .. } => {
            gm_server::Verdict::Cancel
        }
        _ => gm_server::Verdict::Allow,
    }));
    server.set_weapons(1, &["gmod_tool", "weapon_physgun"]);
    assert!(
        !server.player_kill_command(1) && server.players[&1].alive,
        "CanPlayerSuicide refuses"
    );
    assert!(
        !server.player_select_weapon(1, "weapon_physgun"),
        "PlayerSwitchWeapon refuses"
    );
    assert_eq!(server.players[&1].active_weapon, "gmod_tool");
    server.request_hook = None;
    assert!(server.player_select_weapon(1, "weapon_physgun"));
    assert_eq!(server.players[&1].active_weapon, "weapon_physgun");
    server.select_weapon(1, "gmod_tool");
    server.request_hook = Some(Box::new(|_, req| match req {
        gm_server::Request::Spawn {
            kind: "vehicle", ..
        } => gm_server::Verdict::Cancel,
        _ => gm_server::Verdict::Allow,
    }));
    assert!(
        server.spawn_vehicle(1, "Chair_Wood").is_none(),
        "PlayerSpawnVehicle refuses"
    );
    server.request_hook = None;
    let id = server.spawn_vehicle(1, "Chair_Wood").expect("chair spawns");
    assert!(matches!(
        server.game_events.last(),
        Some(gm_server::GameEvent::Spawned { player: 1, ent, kind: "vehicle", .. }) if *ent == id
    ));
    let prop = server
        .sandbox
        .props()
        .iter()
        .find(|p| p.id == id)
        .unwrap()
        .clone();
    assert!((prop.pos - hit.pos).length() < 0.01);
    assert!((prop.angles.y - 180.0).abs() < 0.01);
    assert_eq!(server.vehicles[&id].class, "prop_vehicle_prisoner_pod");
    assert_eq!(
        server.vehicles[&id].health, 30,
        "wooden chair prop_data health"
    );
    assert_eq!(server.sandbox.undos(1)[0].name, "Vehicle");
    for _ in 0..30 {
        server.tick(&idle);
    }
    let chair = server
        .sandbox
        .props()
        .iter()
        .find(|p| p.id == id)
        .unwrap()
        .pos;
    // measured `server_vehicle_damage`: bullets neither hurt nor push a vehicle (the chair stays
    // 30/30 and does not move across pistol and .357 hits, even with 500 damage)
    let (eye, _) = server.eye(1).unwrap();
    let centre = chair + Vec3::new(0.0, 0.0, 16.0);
    server.fire_bullets(1, (centre - eye).normalize(), "357", Some(500.0));
    for _ in 0..5 {
        server.tick(&idle);
    }
    assert_eq!(
        server.vehicles[&id].health, 30,
        "vehicles take no bullet damage"
    );
    let after = server
        .sandbox
        .props()
        .iter()
        .find(|p| p.id == id)
        .unwrap()
        .pos;
    assert!(
        (after - chair).length() < 0.05,
        "bullets do not push a vehicle: moved {}",
        (after - chair).length()
    );
    let refusals = std::rc::Rc::new(std::cell::Cell::new(0u32));
    let r2 = refusals.clone();
    server.request_hook = Some(Box::new(move |_, req| match req {
        gm_server::Request::EnterVehicle { player: 1, .. } if r2.get() == 0 => {
            r2.set(1);
            gm_server::Verdict::Cancel
        }
        _ => gm_server::Verdict::Allow,
    }));
    assert!(
        !server.use_enter_vehicle(1, id),
        "CanPlayerEnterVehicle refuses the first +use"
    );
    assert!(server.players[&1].seat.is_none());
    assert!(server.use_enter_vehicle(1, id), "second +use is allowed");
    assert_eq!(refusals.get(), 1);
    assert!(server.use_exit_vehicle(1) && server.players[&1].seat.is_none());
    server.request_hook = None;
    assert!(server.enter_vehicle(1, id));
    let seated = server.players[&1].state.origin;
    // measured: the body sits on `vehicle_feet_passenger0`, (0, -3, 19.5) from the chair's
    // origin at yaw 180 (the root bone's idle-frame quarter turn; 0.5 u of height still open)
    let d = seated - chair;
    assert!(
        (d - Vec3::new(0.0, -3.0, 19.5)).length() < 0.6,
        "seat offset {d:?}"
    );
    assert_eq!(server.vehicles[&id].driver, Some(1));
    for _ in 0..5 {
        server.tick(&idle);
    }
    assert!(
        (server.players[&1].state.origin - seated).length() < 0.5,
        "rides the seat"
    );
    assert!(server.exit_vehicle(1));
    // measured: the first free exit point is 90 u along the chair's axis (behind it) and
    // 11.4 u up, the player faces that way; the per-side law is `server_vehicle_exit_*`
    let out = server.players[&1].state.origin - chair;
    let flat = Vec3::new(out.x, out.y, 0.0);
    assert!(
        (flat.length() - 90.0).abs() < 0.5 && (out.z - 11.4).abs() < 0.1,
        "exit offset {out:?}"
    );
    let want_yaw = flat.y.atan2(flat.x).to_degrees().rem_euclid(360.0);
    assert!((server.players[&1].view_angles.y - want_yaw).abs() < 0.01);
    assert_eq!(server.vehicles[&id].driver, None);
    server.sandbox.limits.max_vehicles = 2;
    assert!(server.spawn_vehicle(1, "Chair_Wood").is_some());
    assert!(
        server.spawn_vehicle(1, "Chair_Wood").is_none(),
        "sbox_maxvehicles"
    );
    server.undo(1);
    server.undo(1);
    assert!(server.vehicles.is_empty());
}

#[test]
fn fire_bullets_and_npc_damage_are_asked() {
    let Some(inst) = gm_content::find_install() else {
        eprintln!("skipped: no Garry's Mod install");
        return;
    };
    let mount = gm_content::Mount::for_install(&inst).unwrap();
    let world = gm_dedicated::load_world(&mount, "gm_construct").unwrap();
    let mut server = gm_server::Server::new(&world.collision, world.physics, world.world_body);
    server.model_loader = Some(Box::new(move |m| gm_dedicated::model_info(&mount, m).ok()));
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
    server.request_hook = Some(Box::new(|_, req| match req {
        gm_server::Request::FireBullets { .. } => gm_server::Verdict::Cancel,
        gm_server::Request::NpcDamage { .. } => gm_server::Verdict::Amount(0.0),
        _ => gm_server::Verdict::Allow,
    }));
    let before = server.game_events.len();
    server.fire_bullets(1, Vec3::new(1.0, 0.0, 0.0), "Pistol", Some(5.0));
    assert!(
        !server.game_events[before..]
            .iter()
            .any(|e| matches!(e, gm_server::GameEvent::FiredBullets { .. })),
        "EntityFireBullets refuses"
    );
    let npc = server
        .spawn_npc(1, "npc_zombie", "none")
        .expect("zombie spawns");
    let hp = server.npcs[&npc].health;
    server.damage_npc(npc, 1, 10.0, 0);
    assert_eq!(
        server.npcs[&npc].health, hp,
        "ScaleNPCDamage zeroed the hit"
    );
    server.request_hook = None;
    server.fire_bullets(1, Vec3::new(1.0, 0.0, 0.0), "Pistol", Some(5.0));
    assert!(matches!(
        server.game_events.last(),
        Some(gm_server::GameEvent::FiredBullets {
            attacker: 1,
            pellets: 1,
            ..
        })
    ));
    server.damage_npc(npc, 1, 10.0, 0);
    assert!(server.npcs[&npc].health < hp);
    server.request_hook = Some(Box::new(|_, req| match req {
        gm_server::Request::CreateUndo { .. } => gm_server::Verdict::Cancel,
        _ => gm_server::Verdict::Allow,
    }));
    let undos = server.sandbox.undos(1).len();
    assert!(server.spawn_npc(1, "npc_zombie", "none").is_some());
    assert_eq!(
        server.sandbox.undos(1).len(),
        undos,
        "CanCreateUndo refused the entry"
    );
    server.request_hook = Some(Box::new(|_, req| match req {
        gm_server::Request::CheckLimit { kind, .. } if kind == "npcs" => gm_server::Verdict::Cancel,
        _ => gm_server::Verdict::Allow,
    }));
    assert!(
        server.spawn_npc(1, "npc_zombie", "none").is_none(),
        "PlayerCheckLimit denies"
    );
    let steps = std::sync::Arc::new(std::sync::atomic::AtomicUsize::new(0));
    let counter = steps.clone();
    server.request_hook = Some(Box::new(move |_, req| match req {
        gm_server::Request::Footstep { player: 1, .. } => {
            counter.fetch_add(1, std::sync::atomic::Ordering::SeqCst);
            gm_server::Verdict::Cancel
        }
        _ => gm_server::Verdict::Allow,
    }));
    let walk = gm_server::PlayerCmd {
        cmd: gm_move::UserCmd {
            buttons: 1 << 3,
            forward_move: 400.0,
            ..Default::default()
        },
        ..Default::default()
    };
    let walking = std::collections::BTreeMap::from([(1, walk)]);
    server.take_sounds();
    for _ in 0..120 {
        server.tick(&walking);
    }
    assert!(
        steps.load(std::sync::atomic::Ordering::SeqCst) >= 1,
        "PlayerFootstep asked while walking"
    );
    assert!(
        !server
            .take_sounds()
            .iter()
            .any(|s| s.name.contains("step") || s.name.contains("Step")),
        "a refused footstep stays silent"
    );
    let asked = std::sync::Arc::new(std::sync::atomic::AtomicUsize::new(0));
    let counter = asked.clone();
    server.request_hook = Some(Box::new(move |_, req| match req {
        gm_server::Request::EmitSound { name, .. } if name == "Zombie.Pain" => {
            counter.fetch_add(1, std::sync::atomic::Ordering::SeqCst);
            gm_server::Verdict::Cancel
        }
        gm_server::Request::EmitSound { name, .. } if name == "Zombie.Idle" => {
            gm_server::Verdict::Amount(0.25)
        }
        _ => gm_server::Verdict::Allow,
    }));
    server.take_sounds();
    assert!(server.emit_sound(npc, "Zombie.Pain".to_owned(), 1.0));
    assert!(server.emit_sound(npc, "Zombie.Idle".to_owned(), 1.0));
    assert!(!server.emit_sound(9999, "Zombie.Idle".to_owned(), 1.0));
    let sounds = server.take_sounds();
    assert_eq!(asked.load(std::sync::atomic::Ordering::SeqCst), 1);
    assert!(
        !sounds.iter().any(|s| s.name == "Zombie.Pain"),
        "EntityEmitSound false drops the sound"
    );
    let idle = sounds
        .iter()
        .find(|s| s.name == "Zombie.Idle")
        .expect("the allowed sound leaves");
    assert!(idle.ent == npc && (idle.volume - 0.25).abs() < 1e-6);
    server.request_hook = Some(Box::new(|_, req| match req {
        gm_server::Request::SelectSpawn { player: 1, .. } => {
            gm_server::Verdict::Text("100 200 -143 0 90 0".to_owned())
        }
        _ => gm_server::Verdict::Allow,
    }));
    server.spawn_player(1);
    let origin = server.players[&1].state.origin;
    assert!(
        (origin - Vec3::new(100.0, 200.0, -143.0 + gm_server::SPAWN_Z_OFFSET)).length() < 0.01,
        "PlayerSelectSpawn placed the player: {origin:?}"
    );
    server.request_hook = Some(Box::new(|_, req| match req {
        gm_server::Request::SelectSpawn { .. } => gm_server::Verdict::Text("nowhere".to_owned()),
        _ => gm_server::Verdict::Allow,
    }));
    server.spawn_player(1);
    let after = server.players[&1].state.origin;
    assert!(
        after.is_finite() && (after - Vec3::new(100.0, 200.0, -142.0)).length() < 0.01,
        "a malformed answer is ignored (no spawn points here, so the player stays): {after:?}"
    );
    server.request_hook = Some(Box::new(|_, req| match req {
        gm_server::Request::SelectSpawn { .. } => {
            gm_server::Verdict::Text("300 400 -143 0 0 0".to_owned())
        }
        _ => gm_server::Verdict::Allow,
    }));
    server.spawn_player(1);
    assert!(
        (server.players[&1].state.origin - Vec3::new(300.0, 400.0, -142.0)).length() < 0.01,
        "a second valid answer moves the player again"
    );
    server.request_hook = None;
}

#[test]
fn view_punch_pickup_object_step_sound_and_ladder_are_server_operations() {
    let Some(inst) = gm_content::find_install() else {
        eprintln!("skipped: no Garry's Mod install");
        return;
    };
    let mount = gm_content::Mount::for_install(&inst).unwrap();
    let world = gm_dedicated::load_world(&mount, "gm_construct").unwrap();
    let mut server = gm_server::Server::new(&world.collision, world.physics, world.world_body);
    for m in ["models/nova/chair_wood01.mdl", "models/buggy.mdl"] {
        match gm_dedicated::model_info(&mount, m) {
            Ok(info) => {
                eprintln!("{m}: {} attachments", info.attachments.len());
                server.register_model(m, info);
            }
            Err(e) => panic!("{m}: {e}"),
        }
    }
    let start = Vec3::new(823.235, -500.0, -143.0);
    server.add_player(1, start, Vec3::new(20.0, 0.0, 0.0));
    assert!(server.view_punch(1, Some(Vec3::new(5.0, 0.0, 0.0)), None, true));
    assert!(server.view_punch(1, Some(Vec3::new(1.0, 2.0, 0.0)), None, true));
    assert!((server.players[&1].state.punch - Vec3::new(6.0, 2.0, 0.0)).length() < 1e-4);
    assert!(server.view_punch(1, Some(Vec3::ZERO), Some(Vec3::ZERO), false));
    assert_eq!(server.players[&1].state.punch, Vec3::ZERO);
    assert!(
        !server.pickup_object(1, 99_999),
        "no such entity, nothing carried"
    );
    assert!(server.exit_ladder(1));
    assert!(!server.view_punch(7, None, None, false), "no such player");
}
