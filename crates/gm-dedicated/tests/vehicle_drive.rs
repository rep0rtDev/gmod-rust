//! Driving a jeep on gm_flatgrass against `server_vehicle_drive` (srcds, driverless jeep
//! steered through the Vehicle API): full throttle gains 5.85 u/s per tick up to 35 mph, steering
//! right at speed turns a degree per tick, the handbrake stops in ~47 ticks, reverse caps at 14 mph.

use glam::Vec3;

fn speed(server: &gm_server::Server, veh: u32) -> f32 {
    server.vehicles[&veh].controls.speed
}

fn yaw(server: &gm_server::Server, veh: u32) -> f32 {
    server
        .sandbox
        .props()
        .iter()
        .find(|p| p.id == veh)
        .map(|p| p.angles.y)
        .unwrap()
}

#[test]
fn jeep_drives_like_the_measured_one() {
    let Some(inst) = gm_content::find_install() else {
        eprintln!("skipped: no Garry's Mod install");
        return;
    };
    let mount = gm_content::Mount::for_install(&inst).unwrap();
    let world = gm_dedicated::load_world(&mount, "gm_flatgrass").unwrap();
    let mut server = gm_server::Server::new(&world.collision, world.physics, world.world_body);
    let info = gm_dedicated::model_info(&mount, "models/buggy.mdl").unwrap();
    server.register_model("models/buggy.mdl", info);
    let start = Vec3::new(1600.0, 384.0, -12799.0);
    server.add_player(1, start, Vec3::new(20.0, 90.0, 0.0));
    server.set_weapons(1, &["gmod_tool"]);
    let look = gm_server::PlayerCmd {
        cmd: gm_move::UserCmd {
            view_angles: Vec3::new(20.0, 90.0, 0.0),
            ..Default::default()
        },
        ..Default::default()
    };
    let idle = std::collections::BTreeMap::from([(1, look)]);
    for _ in 0..20 {
        server.tick(&idle);
    }
    let veh = server.spawn_vehicle(1, "Jeep").expect("jeep spawns");
    for _ in 0..40 {
        server.tick(&idle);
    }
    assert!(
        speed(&server, veh).abs() < 5.0,
        "settled: {}",
        speed(&server, veh)
    );
    let yaw0 = yaw(&server, veh);

    // full throttle: +5.85 u/s per tick, 234 u/s after 40 ticks, 614..640 plateau by 110
    assert!(server.set_vehicle_engine(veh, true));
    assert!(server.set_vehicle_controls(veh, Some(1.0), Some(0.0), Some(false)));
    for _ in 0..40 {
        server.tick(&idle);
    }
    let s40 = speed(&server, veh);
    assert!((s40 - 234.0).abs() < 30.0, "speed after 40 ticks: {s40}");
    for _ in 0..110 {
        server.tick(&idle);
    }
    let s150 = speed(&server, veh);
    assert!((600.0..660.0).contains(&s150), "plateau: {s150}");
    assert!(
        (yaw(&server, veh) - yaw0).abs() < 2.0,
        "straight: {}",
        yaw(&server, veh) - yaw0
    );
    assert_eq!(server.vehicles[&veh].controls.gear, 1);
    let rpm = server.vehicles[&veh].controls.rpm(&gm_server::JEEP_DRIVE);
    assert!((3100.0..3500.0).contains(&rpm), "rpm {rpm}");

    // steering right at speed: 18 deg cone, a degree of yaw per tick (66.7 deg/s)
    assert!(server.set_vehicle_controls(veh, None, Some(1.0), None));
    for _ in 0..60 {
        server.tick(&idle);
    }
    let turned = (yaw0 - yaw(&server, veh)).rem_euclid(360.0);
    assert!(
        (45.0..75.0).contains(&turned),
        "turned {turned} deg in 60 ticks"
    );
    assert!(
        (server.vehicles[&veh].controls.steer_angle - 18.0).abs() < 0.5,
        "steer angle {}",
        server.vehicles[&veh].controls.steer_angle
    );
    let s_turn = speed(&server, veh);
    assert!(
        (560.0..660.0).contains(&s_turn),
        "speed in the turn: {s_turn}"
    );

    // coasting: -1 u/s per tick
    assert!(server.set_vehicle_controls(veh, Some(0.0), Some(0.0), None));
    for i in 0..30 {
        server.tick(&idle);
        let p = server.sandbox.props().iter().find(|p| p.id == veh).unwrap();
        eprintln!(
            "coast {i}: speed {:.1} pitch {:.2} roll {:.2} yaw {:.2} vel {:?} contacts {}",
            speed(&server, veh),
            p.angles.x,
            p.angles.z,
            p.angles.y,
            server.prop_state(veh).map(|st| st.velocity),
            server.vehicles[&veh].controls.wheels_in_contact
        );
    }
    let s_coast = speed(&server, veh);
    assert!(
        (s_turn - 45.0..s_turn - 15.0).contains(&s_coast),
        "coast {s_turn} -> {s_coast}"
    );

    // handbrake: ~-11.8 u/s per tick, standing within 60 ticks
    assert!(server.set_vehicle_controls(veh, None, None, Some(true)));
    for _ in 0..20 {
        server.tick(&idle);
    }
    let s_brake = speed(&server, veh);
    assert!(
        (s_coast - 290.0..s_coast - 180.0).contains(&s_brake),
        "handbrake {s_coast} -> {s_brake}"
    );
    for _ in 0..40 {
        server.tick(&idle);
    }
    assert!(
        speed(&server, veh).abs() < 20.0,
        "stopped: {}",
        speed(&server, veh)
    );

    // reverse: the same rate back to 14 mph, then a creep
    assert!(server.set_vehicle_controls(veh, Some(-1.0), None, Some(false)));
    for _ in 0..60 {
        server.tick(&idle);
    }
    let s_rev = speed(&server, veh);
    assert!((-280.0..-230.0).contains(&s_rev), "reverse {s_rev}");

    // engine off parks the car
    assert!(server.set_vehicle_engine(veh, false));
    for _ in 0..40 {
        server.tick(&idle);
    }
    assert!(
        speed(&server, veh).abs() < 40.0,
        "parked: {}",
        speed(&server, veh)
    );
}

#[test]
fn airboat_drives_like_the_measured_one() {
    let Some(inst) = gm_content::find_install() else {
        eprintln!("skipped: no Garry's Mod install");
        return;
    };
    let mount = gm_content::Mount::for_install(&inst).unwrap();
    let world = gm_dedicated::load_world(&mount, "gm_flatgrass").unwrap();
    let mut server = gm_server::Server::new(&world.collision, world.physics, world.world_body);
    let info = gm_dedicated::model_info(&mount, "models/airboat.mdl").unwrap();
    server.register_model("models/airboat.mdl", info);
    let start = Vec3::new(1600.0, 384.0, -12799.0);
    server.add_player(1, start, Vec3::new(20.0, 90.0, 0.0));
    server.set_weapons(1, &["gmod_tool"]);
    let look = gm_server::PlayerCmd {
        cmd: gm_move::UserCmd {
            view_angles: Vec3::new(20.0, 90.0, 0.0),
            ..Default::default()
        },
        ..Default::default()
    };
    let idle = std::collections::BTreeMap::from([(1, look)]);
    for _ in 0..20 {
        server.tick(&idle);
    }
    let veh = server.spawn_vehicle(1, "Airboat").expect("airboat spawns");
    for _ in 0..40 {
        server.tick(&idle);
    }
    let yaw0 = yaw(&server, veh);
    // the fan: 111 u/s after 40 ticks, 190 after 100, 228 after 200 (terminal 235)
    assert!(server.set_vehicle_engine(veh, true));
    assert!(server.set_vehicle_controls(veh, Some(1.0), Some(0.0), Some(false)));
    for _ in 0..40 {
        server.tick(&idle);
    }
    let s40 = speed(&server, veh);
    assert!((s40 - 111.0).abs() < 20.0, "speed after 40 ticks: {s40}");
    for _ in 0..60 {
        server.tick(&idle);
    }
    let s100 = speed(&server, veh);
    assert!((s100 - 190.0).abs() < 25.0, "speed after 100 ticks: {s100}");
    for _ in 0..100 {
        server.tick(&idle);
    }
    let s200 = speed(&server, veh);
    assert!((s200 - 228.0).abs() < 20.0, "speed after 200 ticks: {s200}");
    assert_eq!(
        server.vehicles[&veh]
            .controls
            .rpm(&gm_server::AIRBOAT_DRIVE),
        0.0
    );
    // 60 deg of steering: the yaw builds up to 1.5 deg per tick, ~70 deg over 60 ticks
    assert!(server.set_vehicle_controls(veh, None, Some(1.0), None));
    for _ in 0..60 {
        server.tick(&idle);
    }
    let turned = (yaw0 - yaw(&server, veh)).rem_euclid(360.0);
    assert!(
        (40.0..110.0).contains(&turned),
        "turned {turned} deg in 60 ticks"
    );
    // the sliding tyres stop a coasting boat within ~40 ticks
    assert!(server.set_vehicle_controls(veh, Some(0.0), Some(0.0), None));
    for _ in 0..50 {
        server.tick(&idle);
    }
    assert!(
        speed(&server, veh).abs() < 25.0,
        "coasted to: {}",
        speed(&server, veh)
    );
    // reverse: -65 u/s after 50 ticks, approaching -100
    assert!(server.set_vehicle_controls(veh, Some(-1.0), None, None));
    for _ in 0..50 {
        server.tick(&idle);
    }
    let s_rev = speed(&server, veh);
    assert!((s_rev + 65.0).abs() < 15.0, "reverse {s_rev}");
}
