//! Map entity I/O on gm_construct: the dark room's `func_button` (`movedir 0 90 0`, speed
//! 100, wait 0.01, `OnPressed ColourRoomLights,Toggle`) is a player-trace convex, presses on
//! `Use`, travels along +Y and comes back; `logic_auto` fires `OnMapSpawn` once.

use glam::Vec3;
use gm_server::mapio::{Kind, Toggle, MAP_ENT_ID_BASE};

fn load() -> Option<(gm_dedicated::LoadedWorld, gm_content::Mount)> {
    let inst = gm_content::find_install()?;
    let mount = gm_content::Mount::for_install(&inst).ok()?;
    let world = gm_dedicated::load_world(&mount, "gm_construct").ok()?;
    Some((world, mount))
}

#[test]
fn construct_button_presses_and_returns() {
    let Some((world, _mount)) = load() else {
        return;
    };
    let mut server = gm_server::Server::new(&world.collision, world.physics, world.world_body);
    server.set_map_entities(world.map_entities.clone(), world.surface);
    let button = server
        .map_io
        .entities
        .iter()
        .find(|e| e.classname == "func_button")
        .cloned()
        .expect("gm_construct has a func_button");
    assert!(button.id >= MAP_ENT_ID_BASE);
    assert!(!button.def.hulls.is_empty(), "the button is a brush model");
    let Kind::Mover {
        pos1, pos2, speed, ..
    } = button.kind
    else {
        panic!("{:?}", button.kind)
    };
    assert_eq!(speed, 100.0);
    let travel = pos2 - pos1;
    assert!(
        travel.y > 0.0 && travel.x.abs() < 1e-3 && travel.z.abs() < 1e-3,
        "{travel:?}"
    );
    assert!(
        server
            .map_io
            .convexes()
            .iter()
            .any(|c| c.entity == button.id),
        "the button blocks the player trace"
    );
    assert!(
        !server.map_io.find_by_name("ColourRoomLights").is_empty(),
        "the lights it toggles are map entities too"
    );

    let idle = std::collections::BTreeMap::new();
    server.tick(&idle);
    server.map_input(button.id, "Use", "", 1);
    for _ in 0..3 {
        server.tick(&idle);
    }
    let e = server.map_io.get(button.id).unwrap();
    assert!(e.pos.y > pos1.y + 2.0, "moving out: {:?}", e.pos);
    let ticks_out = (travel.length() / speed / gm_core::tick::DEFAULT_TICK_INTERVAL) as usize;
    for _ in 0..ticks_out + 2 {
        server.tick(&idle);
    }
    let e = server.map_io.get(button.id).unwrap();
    assert!(
        matches!(
            e.kind,
            Kind::Mover {
                state: Toggle::AtTop,
                ..
            } | Kind::Mover {
                state: Toggle::GoingDown,
                ..
            }
        ),
        "{:?}",
        e.kind
    );
    for _ in 0..ticks_out + 4 {
        server.tick(&idle);
    }
    let e = server.map_io.get(button.id).unwrap();
    assert!((e.pos - pos1).length() < 1e-3, "back home: {:?}", e.pos);
    assert!(matches!(
        e.kind,
        Kind::Mover {
            state: Toggle::AtBottom,
            ..
        }
    ));
}

#[test]
fn logic_auto_fires_on_map_spawn() {
    let Some((world, _mount)) = load() else {
        return;
    };
    let mut server = gm_server::Server::new(&world.collision, world.physics, world.world_body);
    server.set_map_entities(world.map_entities.clone(), world.surface);
    let auto = server
        .map_io
        .entities
        .iter()
        .find(|e| e.classname == "logic_auto")
        .cloned()
        .expect("gm_construct has a logic_auto");
    assert!(auto.outputs.iter().any(|o| o.name == "OnMapSpawn"));
    let idle = std::collections::BTreeMap::new();
    server.tick(&idle);
    server.tick(&idle);
    // the outputs with `times -1` stay; nothing panics on the unknown targets
    let e = server.map_io.get(auto.id).unwrap();
    assert_eq!(e.outputs.len(), auto.outputs.len());
    assert!(server.map_io.get(MAP_ENT_ID_BASE).is_some());
    let _ = Vec3::ZERO;
}
