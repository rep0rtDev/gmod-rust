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
    let id = server.spawn_vehicle(1, "Chair_Wood").expect("chair spawns");
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
    assert!(server.enter_vehicle(1, id));
    let seated = server.players[&1].state.origin;
    // measured: the body sits on `vehicle_feet_passenger0`, (0, -3, 19.5) from the chair's
    // origin at yaw 180 (the engine's static-prop quarter turn; 0.5 u of height still open)
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
    let out = server.players[&1].state.origin - chair;
    assert!(
        (out - Vec3::new(0.0, -100.0, 12.0)).length() < 1.0,
        "exit offset {out:?}"
    );
    assert!((server.players[&1].view_angles.y - 270.0).abs() < 0.01);
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
