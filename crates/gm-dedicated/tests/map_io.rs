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

#[test]
fn the_dark_room_button_toggles_the_colour_room_lights_style() {
    let Some((world, _mount)) = load() else {
        return;
    };
    let mut server = gm_server::Server::new(&world.collision, world.physics, world.world_body);
    server.set_map_entities(world.map_entities.clone(), world.surface);
    assert_eq!(
        server.lightstyles(),
        vec![(32, 1.0)],
        "ColourRoomLights on at spawn"
    );
    let button = server
        .map_io
        .entities
        .iter()
        .find(|e| e.classname == "func_button")
        .cloned()
        .expect("gm_construct has a func_button");
    let idle = std::collections::BTreeMap::new();
    assert!(server.fire_input_id(button.id, "Press", "", 0));
    server.tick(&idle);
    assert_eq!(
        server.lightstyles(),
        vec![(32, 0.0)],
        "OnPressed ColourRoomLights,Toggle"
    );
    server.fire_input("ColourRoomLights", "SetPattern", "z", 0);
    server.fire_input("ColourRoomLights", "TurnOn", "", 0);
    server.tick(&idle);
    let styles = server.lightstyles();
    assert_eq!(styles.len(), 1);
    assert!((styles[0].1 - 25.0 / 12.0).abs() < 1e-5, "{styles:?}");
}

#[test]
fn a_tracktrain_runs_its_path_tracks_and_takes_their_speed() {
    let Some((world, _mount)) = load() else {
        return;
    };
    let mut server = gm_server::Server::new(&world.collision, world.physics, world.world_body);
    let def = |class: &str, kv: &[(&str, &str)], origin: Vec3| gm_server::mapio::MapEntityDef {
        classname: class.into(),
        kv: kv
            .iter()
            .map(|(k, v)| (k.to_string(), v.to_string()))
            .collect(),
        origin,
        angles: Vec3::ZERO,
        hulls: Vec::new(),
        mins: Vec3::ZERO,
        maxs: Vec3::ZERO,
    };
    let defs = vec![
        def(
            "path_track",
            &[("targetname", "t1"), ("target", "t2")],
            Vec3::ZERO,
        ),
        def(
            "path_track",
            &[("targetname", "t2"), ("target", "t3"), ("speed", "100")],
            Vec3::new(100.0, 0.0, 0.0),
        ),
        def(
            "path_track",
            &[("targetname", "t3")],
            Vec3::new(100.0, 100.0, 0.0),
        ),
        def(
            "func_tracktrain",
            &[
                ("targetname", "train"),
                ("target", "t1"),
                ("speed", "200"),
                ("startspeed", "0"),
                ("height", "4"),
            ],
            Vec3::new(-500.0, -500.0, 0.0),
        ),
    ];
    server.set_map_entities(defs, world.surface);
    let idle = std::collections::BTreeMap::new();
    server.tick(&idle);
    let train = |server: &gm_server::Server| {
        server
            .map_io
            .entities
            .iter()
            .find(|e| e.classname == "func_tracktrain")
            .cloned()
            .unwrap()
    };
    assert_eq!(
        train(&server).pos,
        Vec3::new(0.0, 0.0, 4.0),
        "teleports to its first path_track, height above it"
    );
    assert!(!train(&server).moving(), "startspeed 0");
    server.fire_input("train", "StartForward", "", 0);
    let mut ticks = 0;
    while train(&server).pos.x < 100.0 - 1e-3 {
        server.tick(&idle);
        ticks += 1;
        assert!(ticks < 100, "100 u at 200 u/s take ~0.5 s");
        assert!(train(&server).pos.y.abs() < 1e-3);
    }
    assert!((28..=40).contains(&ticks), "{ticks} ticks");
    let t = train(&server);
    assert!(
        (t.rot * Vec3::X - Vec3::Y).length() < 1e-3,
        "faces t3 after t2"
    );
    for _ in 0..33 {
        server.tick(&idle);
    }
    let y = train(&server).pos.y;
    assert!(
        (42.0..=58.0).contains(&y),
        "t2's speed 100 (under the train's max) -> ~50 u in 0.5 s, got {y}"
    );
    for _ in 0..100 {
        server.tick(&idle);
    }
    let t = train(&server);
    assert_eq!(
        t.pos,
        Vec3::new(100.0, 100.0, 4.0),
        "stops at the end of the track"
    );
    assert!(!t.moving());
    server.fire_input("train", "StartBackward", "", 0);
    for _ in 0..20 {
        server.tick(&idle);
    }
    let t = train(&server);
    assert!(
        t.pos.y < 100.0 && t.pos.x == 100.0,
        "runs back towards t2: {:?}",
        t.pos
    );
}

#[test]
fn env_explosion_explodes_on_its_input() {
    let Some((world, _mount)) = load() else {
        return;
    };
    let mut server = gm_server::Server::new(&world.collision, world.physics, world.world_body);
    let kv = [("targetname", "boom"), ("iMagnitude", "50")];
    let def = gm_server::mapio::MapEntityDef {
        classname: "env_explosion".into(),
        kv: kv
            .iter()
            .map(|(k, v)| (k.to_string(), v.to_string()))
            .collect(),
        origin: Vec3::new(700.0, 100.0, -100.0),
        angles: Vec3::ZERO,
        hulls: Vec::new(),
        mins: Vec3::ZERO,
        maxs: Vec3::ZERO,
    };
    server.set_map_entities(vec![def], world.surface);
    let idle = std::collections::BTreeMap::new();
    server.tick(&idle);
    server.take_effects();
    server.fire_input("boom", "Explode", "", 0);
    server.tick(&idle);
    let effects = server.take_effects();
    assert!(
        effects.iter().any(|e| matches!(
            e,
            gm_server::WorldEffect::Explosion { pos } if (*pos - Vec3::new(700.0, 100.0, -99.0)).length() < 1e-3
        )),
        "{effects:?}"
    );
}

#[test]
fn env_shake_and_env_fade_reach_the_players_in_range() {
    let Some((world, _mount)) = load() else {
        return;
    };
    let mut server = gm_server::Server::new(&world.collision, world.physics, world.world_body);
    let def = |class: &str, kv: &[(&str, &str)], origin: Vec3| gm_server::mapio::MapEntityDef {
        classname: class.into(),
        kv: kv
            .iter()
            .map(|(k, v)| (k.to_string(), v.to_string()))
            .collect(),
        origin,
        angles: Vec3::ZERO,
        hulls: Vec::new(),
        mins: Vec3::ZERO,
        maxs: Vec3::ZERO,
    };
    let defs = vec![
        def(
            "env_shake",
            &[
                ("targetname", "quake"),
                ("amplitude", "8"),
                ("radius", "300"),
                ("duration", "2"),
                ("frequency", "40"),
            ],
            Vec3::new(700.0, 100.0, -100.0),
        ),
        def(
            "env_fade",
            &[
                ("targetname", "dark"),
                ("duration", "1.5"),
                ("holdtime", "0.5"),
                ("renderamt", "200"),
                ("rendercolor", "10 20 30"),
                ("spawnflags", "4"),
            ],
            Vec3::ZERO,
        ),
    ];
    server.set_map_entities(defs, world.surface);
    server.add_player(1, Vec3::new(750.0, 100.0, -100.0), Vec3::ZERO);
    server.add_player(2, Vec3::new(1500.0, 100.0, -100.0), Vec3::ZERO);
    let idle = std::collections::BTreeMap::new();
    server.tick(&idle);
    server.notices.clear();
    server.fire_input("quake", "StartShake", "", 0);
    server.fire_input("dark", "Fade", "", 2);
    server.tick(&idle);
    let notices = std::mem::take(&mut server.notices);
    assert!(
        notices.contains(&(1, "Shake 8 2 40".to_string())),
        "the player 50 u away shakes: {notices:?}"
    );
    assert!(
        !notices
            .iter()
            .any(|(p, t)| *p == 2 && t.starts_with("Shake")),
        "the player 800 u away does not: {notices:?}"
    );
    assert!(
        notices.contains(&(2, "ScreenFade 10 20 30 200 1.5 0.5 2".to_string())),
        "spawnflag 4: the activator only, fading out: {notices:?}"
    );
    assert!(!notices
        .iter()
        .any(|(p, t)| *p == 1 && t.starts_with("ScreenFade")));
}

#[test]
fn env_sprite_shows_hides_and_recolours_through_its_inputs() {
    let Some((world, _mount)) = load() else {
        return;
    };
    let mut server = gm_server::Server::new(&world.collision, world.physics, world.world_body);
    let kv = [
        ("targetname", "glow"),
        ("model", "sprites/glow01.spr"),
        ("scale", "0.5"),
        ("rendercolor", "255 128 0"),
        ("renderamt", "200"),
        ("rendermode", "9"),
        ("spawnflags", "0"),
    ];
    let def = gm_server::mapio::MapEntityDef {
        classname: "env_sprite".into(),
        kv: kv
            .iter()
            .map(|(k, v)| (k.to_string(), v.to_string()))
            .collect(),
        origin: Vec3::new(10.0, 20.0, 30.0),
        angles: Vec3::ZERO,
        hulls: Vec::new(),
        mins: Vec3::ZERO,
        maxs: Vec3::ZERO,
    };
    server.set_map_entities(vec![def], world.surface);
    let none = std::collections::BTreeMap::new();
    server.tick(&none);
    assert!(
        server.map_sprite_states().is_empty(),
        "a named sprite without Start On starts hidden"
    );
    server.fire_input("glow", "ShowSprite", "", 0);
    server.tick(&none);
    let s = server.map_sprite_states();
    assert_eq!(s.len(), 1, "shown after ShowSprite");
    let (_, model, pos, scale, color, mode) = &s[0];
    assert_eq!(model, "sprites/glow01", "the material without its .spr");
    assert_eq!(*pos, Vec3::new(10.0, 20.0, 30.0));
    assert_eq!((*scale, *color, *mode), (0.5, [255, 128, 0, 200], 9));
    server.fire_input("glow", "ColorGreenValue", "0", 0);
    server.fire_input("glow", "HideSprite", "", 0);
    server.tick(&none);
    assert!(server.map_sprite_states().is_empty(), "hidden again");
    server.fire_input("glow", "ToggleSprite", "", 0);
    server.tick(&none);
    assert_eq!(
        server.map_sprite_states()[0].4,
        [255, 0, 0, 200],
        "ToggleSprite shows it with the green channel set to 0"
    );
}

#[test]
fn a_path_track_alternate_path_branches_the_train_and_a_disabled_node_ends_the_track() {
    let Some((world, _mount)) = load() else {
        return;
    };
    let mut server = gm_server::Server::new(&world.collision, world.physics, world.world_body);
    let def = |class: &str, kv: &[(&str, &str)], origin: Vec3| gm_server::mapio::MapEntityDef {
        classname: class.into(),
        kv: kv
            .iter()
            .map(|(k, v)| (k.to_string(), v.to_string()))
            .collect(),
        origin,
        angles: Vec3::ZERO,
        hulls: Vec::new(),
        mins: Vec3::ZERO,
        maxs: Vec3::ZERO,
    };
    let defs = vec![
        def(
            "path_track",
            &[("targetname", "a"), ("target", "b")],
            Vec3::ZERO,
        ),
        def(
            "path_track",
            &[("targetname", "b"), ("target", "c"), ("altpath", "d")],
            Vec3::new(100.0, 0.0, 0.0),
        ),
        def(
            "path_track",
            &[("targetname", "c")],
            Vec3::new(200.0, 0.0, 0.0),
        ),
        def(
            "path_track",
            &[("targetname", "d")],
            Vec3::new(100.0, 100.0, 0.0),
        ),
        def(
            "func_tracktrain",
            &[
                ("targetname", "train"),
                ("target", "a"),
                ("speed", "200"),
                ("startspeed", "200"),
                ("height", "0"),
            ],
            Vec3::new(-500.0, -500.0, 0.0),
        ),
    ];
    server.set_map_entities(defs, world.surface);
    let idle = std::collections::BTreeMap::new();
    let train = |server: &gm_server::Server| {
        server
            .map_io
            .entities
            .iter()
            .find(|e| e.classname == "func_tracktrain")
            .cloned()
            .unwrap()
    };
    server.fire_input("b", "EnableAlternatePath", "", 0);
    for _ in 0..120 {
        server.tick(&idle);
    }
    let p = train(&server).pos;
    assert!(
        (p.x - 100.0).abs() < 1e-3 && (p.y - 100.0).abs() < 1e-3,
        "the alternate path takes it to d: {p}"
    );
    assert!(!train(&server).moving(), "d ends the track");
    // back to a, then along the main path with c disabled: it stops at b
    server.fire_input("b", "DisableAlternatePath", "", 0);
    server.fire_input("c", "DisablePath", "", 0);
    server.fire_input("train", "StartBackward", "", 0);
    for _ in 0..120 {
        server.tick(&idle);
    }
    assert_eq!(train(&server).pos, Vec3::ZERO, "back at a");
    server.fire_input("train", "StartForward", "", 0);
    for _ in 0..120 {
        server.tick(&idle);
    }
    assert_eq!(
        train(&server).pos,
        Vec3::new(100.0, 0.0, 0.0),
        "a disabled next node ends the track at b"
    );
}

#[test]
fn a_door_crushes_a_player_blocking_it_and_turns_back() {
    let Some((world, _mount)) = load() else {
        return;
    };
    let mut server = gm_server::Server::new(&world.collision, world.physics, world.world_body);
    let kv = [
        ("targetname", "crusher"),
        ("movedir", "0 0 0"),
        ("speed", "100"),
        ("dmg", "10"),
        ("wait", "-1"),
    ];
    let (lo, hi) = (Vec3::new(-16.0, -16.0, 0.0), Vec3::new(16.0, 16.0, 64.0));
    let mut points = Vec::new();
    for x in [lo.x, hi.x] {
        for y in [lo.y, hi.y] {
            for z in [lo.z, hi.z] {
                points.push(Vec3::new(x, y, z));
            }
        }
    }
    let hull = gm_server::mapio::HullDef {
        planes: vec![
            (Vec3::X, hi.x),
            (-Vec3::X, -lo.x),
            (Vec3::Y, hi.y),
            (-Vec3::Y, -lo.y),
            (Vec3::Z, hi.z),
            (-Vec3::Z, -lo.z),
        ],
        phys_points: points.clone(),
        points,
    };
    let def = gm_server::mapio::MapEntityDef {
        classname: "func_door".into(),
        kv: kv
            .iter()
            .map(|(k, v)| (k.to_string(), v.to_string()))
            .collect(),
        origin: Vec3::ZERO,
        angles: Vec3::ZERO,
        hulls: vec![hull],
        mins: lo,
        maxs: hi,
    };
    server.set_map_entities(vec![def], world.surface);
    // the player stands inside the door's volume: the door cannot clear them
    server.add_player(1, Vec3::ZERO, Vec3::ZERO);
    let idle = std::collections::BTreeMap::new();
    server.tick(&idle);
    let health = server.players[&1].health;
    server.fire_input("crusher", "Open", "", 0);
    server.tick(&idle);
    server.tick(&idle);
    let door = server
        .map_io
        .entities
        .iter()
        .find(|e| e.classname == "func_door")
        .cloned()
        .unwrap();
    let Kind::Mover { state, .. } = door.kind else {
        panic!("a func_door is a mover");
    };
    assert_eq!(state, Toggle::GoingDown, "blocked: it turns back");
    let lost = health - server.players[&1].health;
    assert!(
        (10..=20).contains(&lost),
        "dmg 10 per blocked tick, {lost} lost"
    );
}

#[test]
fn logic_and_math_entities_pass_values_through_their_outputs() {
    let Some((world, _mount)) = load() else {
        return;
    };
    let mut server = gm_server::Server::new(&world.collision, world.physics, world.world_body);
    let def = |class: &str, kv: &[(&str, &str)]| gm_server::mapio::MapEntityDef {
        classname: class.into(),
        kv: kv
            .iter()
            .map(|(k, v)| (k.to_string(), v.to_string()))
            .collect(),
        origin: Vec3::ZERO,
        angles: Vec3::ZERO,
        hulls: Vec::new(),
        mins: Vec3::ZERO,
        maxs: Vec3::ZERO,
    };
    let defs = vec![
        def(
            "math_counter",
            &[
                ("targetname", "c"),
                ("min", "0"),
                ("max", "3"),
                ("OutValue", "b,SetValueTest,,0,-1"),
                ("OnHitMax", "cmp,SetValueCompare,,0,-1"),
            ],
        ),
        def(
            "logic_branch",
            &[("targetname", "b"), ("OnTrue", "case,InValue,,0,-1")],
        ),
        def(
            "logic_compare",
            &[("targetname", "cmp"), ("CompareValue", "3")],
        ),
        def(
            "logic_case",
            &[("targetname", "case"), ("Case01", "1"), ("Case02", "2")],
        ),
    ];
    server.set_map_entities(defs, world.surface);
    let idle = std::collections::BTreeMap::new();
    server.tick(&idle);
    let kind = |server: &gm_server::Server, name: &str| {
        server
            .map_io
            .entities
            .iter()
            .find(|e| e.targetname == name)
            .map(|e| e.kind.clone())
            .unwrap()
    };
    server.fire_input("c", "Add", "2", 0);
    for _ in 0..3 {
        server.tick(&idle);
    }
    assert!(
        matches!(kind(&server, "c"), gm_server::mapio::Kind::Counter { value, .. } if value == 2.0),
        "{:?}",
        kind(&server, "c")
    );
    assert!(
        matches!(
            kind(&server, "b"),
            gm_server::mapio::Kind::Branch { value: true }
        ),
        "OutValue 2 -> SetValueTest -> true: {:?}",
        kind(&server, "b")
    );
    server.fire_input("c", "Add", "5", 0);
    for _ in 0..3 {
        server.tick(&idle);
    }
    assert!(
        matches!(kind(&server, "c"), gm_server::mapio::Kind::Counter { value, at_max: true, .. } if value == 3.0),
        "clamped to max and OnHitMax: {:?}",
        kind(&server, "c")
    );
    assert!(
        matches!(kind(&server, "cmp"), gm_server::mapio::Kind::Compare { value, compare } if value == 3.0 && compare == 3.0),
        "OnHitMax passes the value on: {:?}",
        kind(&server, "cmp")
    );
}

fn def_at(class: &str, kv: &[(&str, &str)], origin: Vec3) -> gm_server::mapio::MapEntityDef {
    gm_server::mapio::MapEntityDef {
        classname: class.into(),
        kv: kv
            .iter()
            .map(|(k, v)| (k.to_string(), v.to_string()))
            .collect(),
        origin,
        angles: Vec3::ZERO,
        hulls: Vec::new(),
        mins: Vec3::ZERO,
        maxs: Vec3::ZERO,
    }
}

#[test]
fn point_template_spawns_its_props_on_force_spawn_and_the_maker_offsets_them() {
    let Some((world, mount)) = load() else {
        return;
    };
    let mut server = gm_server::Server::new(&world.collision, world.physics, world.world_body);
    let model = "models/props_junk/wood_crate001a.mdl";
    server.register_model(model, gm_dedicated::model_info(&mount, model).unwrap());
    let defs = vec![
        def_at(
            "prop_physics",
            &[
                ("targetname", "tpl_crate"),
                ("model", "models/props_junk/wood_crate001a.mdl"),
            ],
            Vec3::new(100.0, 0.0, 100.0),
        ),
        def_at(
            "point_template",
            &[("targetname", "tpl"), ("Template01", "tpl_crate")],
            Vec3::new(0.0, 0.0, 100.0),
        ),
        def_at(
            "env_entity_maker",
            &[
                ("targetname", "maker"),
                ("EntityTemplate", "tpl"),
                ("PostSpawnSpeed", "300"),
                ("PostSpawnDirection", "0 0 0"),
            ],
            Vec3::new(-800.0, -800.0, 300.0),
        ),
    ];
    server.set_map_entities(defs, world.surface);
    let idle = std::collections::BTreeMap::new();
    server.tick(&idle);
    // the templated prop is not in the map (the dedicated loader also leaves it out of
    // map_props); its definition is kept
    assert_eq!(server.sandbox.props().len(), 0);
    assert_eq!(server.map_io.templates.len(), 1);
    let id_of = |server: &gm_server::Server, name: &str| {
        server
            .map_io
            .entities
            .iter()
            .find(|e| e.targetname == name)
            .map(|e| e.id)
            .unwrap()
    };
    let tpl = id_of(&server, "tpl");
    server.map_input(tpl, "ForceSpawn", "", 0);
    server.tick(&idle);
    let props = server.sandbox.props();
    assert_eq!(props.len(), 1, "ForceSpawn spawns the template's prop");
    let p = &props[0];
    assert!(p.model.contains("wood_crate001a"));
    assert!(
        (p.pos - Vec3::new(100.0, 0.0, 100.0)).length() < 2.0,
        "at its own place: {:?}",
        p.pos
    );
    let (class, name) = server.activator_identity(p.id);
    assert_eq!(
        (class.as_str(), name.as_str()),
        ("prop_physics", "tpl_crate")
    );
    // the maker spawns it at its own place plus the template offset, moving at PostSpawnSpeed
    let maker = id_of(&server, "maker");
    server.map_input(maker, "ForceSpawn", "", 0);
    server.tick(&idle);
    let props = server.sandbox.props();
    assert_eq!(props.len(), 2);
    let p = &props[1];
    assert!(
        (p.pos - Vec3::new(-700.0, -800.0, 300.0)).length() < 20.0,
        "maker + offset: {:?}",
        p.pos
    );
    let (id, start) = (p.id, p.pos);
    for _ in 0..10 {
        server.tick(&idle);
    }
    let p = server.sandbox.props().iter().find(|p| p.id == id).unwrap();
    assert!(
        p.pos.x - start.x > 20.0,
        "PostSpawnSpeed along +x: {:?} -> {:?}",
        start,
        p.pos
    );
}

#[test]
fn trigger_filtername_rejects_the_filtered_activator() {
    let Some((world, _mount)) = load() else {
        return;
    };
    let mut server = gm_server::Server::new(&world.collision, world.physics, world.world_body);
    // a trigger for everything (0x40) at the player's spawn, filtered to NPCs only, chained
    // to a relay so the touch leaves a trace; a filter_multi with Negated inverts it
    let spawn = Vec3::new(823.235, -500.0, -143.0);
    let mut trig = def_at(
        "trigger_multiple",
        &[
            ("targetname", "trig"),
            ("spawnflags", "64"),
            ("filtername", "only_npc"),
            ("OnStartTouch", "relay,Trigger,,0,-1"),
        ],
        spawn,
    );
    trig.mins = Vec3::splat(-64.0);
    trig.maxs = Vec3::new(64.0, 64.0, 128.0);
    // a box hull (entity space) so the trigger has a volume
    let (lo, hi) = (trig.mins, trig.maxs);
    trig.hulls = vec![gm_server::mapio::HullDef {
        planes: vec![
            (Vec3::X, hi.x),
            (Vec3::NEG_X, -lo.x),
            (Vec3::Y, hi.y),
            (Vec3::NEG_Y, -lo.y),
            (Vec3::Z, hi.z),
            (Vec3::NEG_Z, -lo.z),
        ],
        points: (0..8)
            .map(|i| {
                Vec3::new(
                    if i & 1 == 0 { lo.x } else { hi.x },
                    if i & 2 == 0 { lo.y } else { hi.y },
                    if i & 4 == 0 { lo.z } else { hi.z },
                )
            })
            .collect(),
        phys_points: Vec::new(),
    }];
    let mut trig2 = trig.clone();
    trig2
        .kv
        .retain(|(k, _)| k != "filtername" && k != "targetname" && k != "OnStartTouch");
    trig2.kv.push(("targetname".into(), "trig2".into()));
    trig2.kv.push(("filtername".into(), "not_npc".into()));
    trig2
        .kv
        .push(("OnStartTouch".into(), "relay2,Trigger,,0,-1".into()));
    let defs = vec![
        trig,
        trig2,
        def_at(
            "filter_activator_class",
            &[("targetname", "only_npc"), ("filterclass", "npc_*")],
            Vec3::ZERO,
        ),
        def_at(
            "filter_multi",
            &[
                ("targetname", "not_npc"),
                ("Filter01", "only_npc"),
                ("Negated", "1"),
            ],
            Vec3::ZERO,
        ),
        def_at(
            "logic_relay",
            &[
                ("targetname", "relay"),
                ("OnTrigger", "relay,Disable,,0,-1"),
            ],
            Vec3::ZERO,
        ),
        def_at(
            "logic_relay",
            &[
                ("targetname", "relay2"),
                ("OnTrigger", "relay2,Disable,,0,-1"),
            ],
            Vec3::ZERO,
        ),
    ];
    server.set_map_entities(defs, world.surface);
    assert!(server.map_io.passes_filter("not_npc", "player", ""));
    assert!(!server.map_io.passes_filter("only_npc", "player", ""));
    assert!(server.map_io.passes_filter("only_npc", "npc_citizen", ""));
    assert!(server.map_io.passes_filter("nonexistent", "player", ""));
    server.add_player(1, spawn, Vec3::ZERO);
    let idle = std::collections::BTreeMap::from([(1, gm_server::PlayerCmd::default())]);
    for _ in 0..5 {
        server.tick(&idle);
    }
    let relay_enabled = |server: &gm_server::Server, name: &str| {
        let e = server
            .map_io
            .entities
            .iter()
            .find(|e| e.targetname == name)
            .unwrap();
        match e.kind {
            gm_server::mapio::Kind::Relay { enabled, .. } => enabled,
            _ => panic!("relay"),
        }
    };
    assert!(
        relay_enabled(&server, "relay"),
        "the npc-only trigger ignored the player"
    );
    assert!(
        !relay_enabled(&server, "relay2"),
        "the negated filter let the player through"
    );
}
