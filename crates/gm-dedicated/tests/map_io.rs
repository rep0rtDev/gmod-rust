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
    assert!(!server.fire_input_id(MAP_ENT_ID_BASE + 100_000, "Use", "", 1));
    assert!(server.fire_input_id(button.id, "Use", "", 1));
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

#[test]
fn moving_brushes_are_networked_and_left_out_of_the_baked_world() {
    let Some((world, mount)) = load() else {
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
    let states = server.map_entity_states();
    let (_, model, pos, _) = states
        .iter()
        .find(|s| s.0 == button.id)
        .expect("the button is in the snapshot");
    assert!(model.starts_with('*'), "{model}");
    assert_eq!(*pos, button.pos);
    let bytes = mount.read("maps/gm_construct.bsp").unwrap();
    let bsp = gm_formats::bsp::Bsp::parse(&bytes).unwrap();
    let idx: usize = model[1..].parse().unwrap();
    assert!(
        gm_render::world::movable_brush_models(&bsp).contains(&idx),
        "the client draws it as a dynamic model"
    );
}

#[test]
fn a_pressed_button_plays_its_buttons_snd_sound() {
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
    assert_eq!(button.def.get("sounds"), Some("14"));
    server.take_sounds();
    assert!(server.fire_input_id(button.id, "Press", "", 0));
    let sounds = server.take_sounds();
    let snd = sounds
        .iter()
        .find(|s| s.name == "Buttons.snd14")
        .expect("the press plays Buttons.snd14");
    assert!((snd.pos - button.center()).length() < 1e-3);
    assert!(server.fire_input_id(button.id, "Press", "", 0));
    assert!(
        server
            .take_sounds()
            .iter()
            .all(|s| s.name != "Buttons.snd14"),
        "a button already travelling does not play again"
    );
}

#[test]
fn ambient_generic_plays_revolumes_and_fades_through_its_inputs() {
    let Some((world, _mount)) = load() else {
        return;
    };
    let mut server = gm_server::Server::new(&world.collision, world.physics, world.world_body);
    let kv = [
        ("targetname", "hum"),
        ("message", "ambient/machines/wall_loop1.wav"),
        ("health", "5"),
        ("pitch", "100"),
        ("spawnflags", "16"),
    ];
    let def = gm_server::mapio::MapEntityDef {
        classname: "ambient_generic".into(),
        kv: kv
            .iter()
            .map(|(k, v)| (k.to_string(), v.to_string()))
            .collect(),
        origin: Vec3::new(1.0, 2.0, 3.0),
        angles: Vec3::ZERO,
        hulls: Vec::new(),
        mins: Vec3::ZERO,
        maxs: Vec3::ZERO,
    };
    server.set_map_entities(vec![def], world.surface);
    server.tick(&std::collections::BTreeMap::new());
    assert!(
        server.take_sounds().iter().all(|s| !s.looping),
        "spawnflag 16 starts silent"
    );
    server.fire_input("hum", "PlaySound", "", 0);
    server.tick(&std::collections::BTreeMap::new());
    let sounds = server.take_sounds();
    let snd = sounds
        .iter()
        .find(|s| s.name == "ambient/machines/wall_loop1.wav")
        .expect("PlaySound plays the message");
    assert!(snd.looping, "looped unless spawnflag 32");
    assert!((snd.volume - 0.5).abs() < 1e-6, "health 5 -> 0.5");
    assert_eq!(snd.pos, Vec3::new(1.0, 2.0, 3.0));
    server.fire_input("hum", "Volume", "10", 0);
    server.tick(&std::collections::BTreeMap::new());
    assert!(server
        .take_sounds()
        .iter()
        .any(|s| s.looping && s.ent == snd.ent && (s.volume - 1.0).abs() < 1e-6));
    server.fire_input("hum", "FadeOut", "2", 0);
    server.tick(&std::collections::BTreeMap::new());
    assert!(server
        .take_sounds()
        .iter()
        .any(|s| s.stop && s.ent == snd.ent && (s.fade - 2.0).abs() < 1e-6));
}
