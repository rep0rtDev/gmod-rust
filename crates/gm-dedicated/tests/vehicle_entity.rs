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
