//! The example addon through the real stack: map, server loop, net commands and the script hooks.
use std::collections::BTreeMap;

#[test]
fn freeze_crates_addon_through_the_server() {
    let Some(inst) = gm_content::find_install() else {
        eprintln!("skipped: no Garry's Mod install");
        return;
    };
    let mount = gm_content::Mount::for_install(&inst).unwrap();
    let world = gm_dedicated::load_world(&mount, "gm_construct").unwrap();
    let mut server = gm_server::Server::new(&world.collision, world.physics, world.world_body);
    let m2 = gm_content::Mount::for_install(&inst).unwrap();
    server.model_loader = Some(Box::new(move |m: &str| {
        gm_dedicated::model_info(&m2, m).ok()
    }));
    let mut rt = gm_script::Runtime::new();
    let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../addons/examples/freeze_crates");
    let manifest = rt.load_addon(&root).unwrap();
    assert_eq!(manifest.id, "freeze_crates");
    let mut sn = gm_net::ServerNet::new("gm_construct", glam::Vec3::new(823.235, -500.0, -143.0));
    sn.hooks = Some(Box::new(rt));
    let net = gm_net::Loopback::new(1, 0.0, 7);
    let (mut s_ep, mut c_ep) = (net.endpoint(0), net.endpoint(1));
    let mut client = gm_net::ClientNet::new(1, "bot");
    let mut step = |sn: &mut gm_net::ServerNet,
                    client: &mut gm_net::ClientNet,
                    server: &mut gm_server::Server<'_>| {
        client.tick(
            gm_net::NetCmd {
                view_angles: glam::Vec3::new(30.0, 0.0, 0.0),
                ..Default::default()
            },
            &mut c_ep,
        );
        net.tick();
        sn.receive(server, &mut s_ep);
        sn.tick(server);
        sn.send(server, &mut s_ep);
        client.receive(&mut c_ep);
    };
    for _ in 0..30 {
        step(&mut sn, &mut client, &mut server);
    }
    client.command(gm_net::Command::Spawn {
        model: "models/props_junk/popcan01a.mdl".into(),
    });
    client.command(gm_net::Command::Spawn {
        model: "models/props_junk/wood_crate001a.mdl".into(),
    });
    for _ in 0..30 {
        step(&mut sn, &mut client, &mut server);
    }
    // phys.trace from a second runtime over the same live server: the floor under the player
    let mut rt2 = gm_script::Runtime::new();
    rt2.load_addon(&root).unwrap();
    rt2.post_tick(&mut server);
    let floor_z: f32 = rt2.actors()[0].eval("return state.floor_z").unwrap();
    let floor_ent: u32 = rt2.actors()[0].eval("return state.floor_ent").unwrap();
    // the floor brush under the recordings' spawn point: -143.969 (the engine's 1/32 u
    // pull-back from -144); the player origin -143 is the spawn height, it never moved here
    assert!((floor_z - -143.969).abs() < 0.01, "floor {floor_z}");
    assert_eq!(floor_ent, 0, "the world");
    let props = server.sandbox.props();
    assert_eq!(props.len(), 1, "the can was refused: {props:?}");
    assert_eq!(props[0].model, "models/props_junk/wood_crate001a.mdl");
    assert!(
        props[0].frozen,
        "the addon froze the crate the tick after it spawned"
    );
    let body = server.body_of(props[0].id).unwrap();
    assert!(server.physics.is_frozen(body));
    let seen: BTreeMap<u32, bool> = client
        .world
        .entities
        .iter()
        .map(|(k, e)| (*k, e.frozen))
        .collect();
    assert_eq!(
        seen.values().copied().collect::<Vec<_>>(),
        vec![true],
        "the client sees it frozen: {seen:?}"
    );
}

#[test]
fn reload_over_the_network() {
    let Some(inst) = gm_content::find_install() else {
        eprintln!("skipped: no Garry's Mod install");
        return;
    };
    let mount = gm_content::Mount::for_install(&inst).unwrap();
    let world = gm_dedicated::load_world(&mount, "gm_construct").unwrap();
    let mut server = gm_server::Server::new(&world.collision, world.physics, world.world_body);
    // a copy of the example addon we can edit
    let dir = std::path::Path::new(env!("CARGO_TARGET_TMPDIR")).join("reload_addon");
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(dir.join("src")).unwrap();
    std::fs::write(
        dir.join("manifest.toml"),
        "id = \"counter\"\ncapabilities = [\"world-read\"]\n[[actors]]\nname = \"main\"\nsource = \"src/main.luau\"\n",
    )
    .unwrap();
    std::fs::write(
        dir.join("src/main.luau"),
        "events.on('tick.post', function() state.n = (state.n or 0) + 1 end)",
    )
    .unwrap();
    let mut rt = gm_script::Runtime::new();
    rt.load_addon(&dir).unwrap();
    let mut sn = gm_net::ServerNet::new("gm_construct", glam::Vec3::new(823.235, -500.0, -143.0));
    sn.hooks = Some(Box::new(rt));
    let net = gm_net::Loopback::new(1, 0.0, 7);
    let (mut s_ep, mut c_ep) = (net.endpoint(0), net.endpoint(1));
    let mut client = gm_net::ClientNet::new(1, "bot");
    let mut step = |sn: &mut gm_net::ServerNet,
                    client: &mut gm_net::ClientNet,
                    server: &mut gm_server::Server<'_>| {
        client.tick(gm_net::NetCmd::default(), &mut c_ep);
        net.tick();
        sn.receive(server, &mut s_ep);
        sn.tick(server);
        sn.send(server, &mut s_ep);
        client.receive(&mut c_ep);
    };
    for _ in 0..10 {
        step(&mut sn, &mut client, &mut server);
    }
    // not an admin yet: denied
    client.command(gm_net::Command::Reload {
        addon: "counter".into(),
    });
    for _ in 0..10 {
        step(&mut sn, &mut client, &mut server);
    }
    assert!(
        client.events.iter().any(|e| e == "Reload counter denied"),
        "{:?}",
        client.events
    );
    // new source: the counter keeps its value and counts by 100
    std::fs::write(
        dir.join("src/main.luau"),
        "events.on('tick.post', function() state.n = state.n + 100 end)",
    )
    .unwrap();
    let pid = *server.players.keys().next().unwrap();
    sn.admins.insert(pid);
    client.command(gm_net::Command::Reload {
        addon: "counter".into(),
    });
    for _ in 0..10 {
        step(&mut sn, &mut client, &mut server);
    }
    assert!(
        client
            .events
            .iter()
            .any(|e| e.starts_with("Reload counter ok counter 0.1.0 actors 1 -> 1")),
        "{:?}",
        client.events
    );
    client.command(gm_net::Command::Reload {
        addon: "nope".into(),
    });
    for _ in 0..10 {
        step(&mut sn, &mut client, &mut server);
    }
    assert!(
        client
            .events
            .iter()
            .any(|e| e == "Reload nope failed addon nope is not loaded"),
        "{:?}",
        client.events
    );
}

#[test]
fn script_writes_weld_nocollide_and_velocity() {
    let Some(inst) = gm_content::find_install() else {
        eprintln!("skipped: no Garry's Mod install");
        return;
    };
    let mount = gm_content::Mount::for_install(&inst).unwrap();
    let world = gm_dedicated::load_world(&mount, "gm_construct").unwrap();
    let mut server = gm_server::Server::new(&world.collision, world.physics, world.world_body);
    let m2 = gm_content::Mount::for_install(&inst).unwrap();
    server.model_loader = Some(Box::new(move |m: &str| {
        gm_dedicated::model_info(&m2, m).ok()
    }));
    server.add_player(
        1,
        glam::Vec3::new(823.235, -500.0, -143.0),
        glam::Vec3::new(30.0, 0.0, 0.0),
    );
    let crate_model = "models/props_junk/wood_crate001a.mdl";
    let e1 = server.spawn_prop(1, crate_model).unwrap();
    server.teleport_player(1, glam::Vec3::new(823.235, -400.0, -143.0));
    let e2 = server.spawn_prop(1, crate_model).unwrap();
    assert_ne!(e1, e2);
    let mut rt = gm_script::Runtime::new();
    rt.load(gm_script::ActorSpec {
        addon: "test".into(),
        name: "writer".into(),
        priority: 0,
        source: format!(
            "events.on('tick.pre', function()
                if state.done then return end
                state.done = true
                world.queue('weld', {e1}, {{ ent2 = {e2}, forcelimit = 0 }})
                world.queue('nocollide', {e2}, {{ ent2 = 0 }})
                world.queue('set_velocity', {e1}, {{ x = 0, y = 0, z = 120, angular = {{ x = 0, y = 0, z = 90 }} }})
                world.queue('weld', 9999, {{ ent2 = {e2} }})
            end)"
        ),
        config: gm_script::ActorConfig {
            can_write: true,
            ..Default::default()
        },
    })
    .unwrap();
    let applied = rt.pre_tick(&mut server);
    assert_eq!(applied.writes, 3, "{applied:?}");
    assert_eq!(applied.rejected, 1, "{applied:?}");
    let cons = server.sandbox.constraints();
    assert_eq!(cons.len(), 2, "{cons:?}");
    let st = server.prop_state(e1).unwrap();
    assert!((st.velocity.z - 120.0).abs() < 0.5, "{:?}", st.velocity);
    // The second actor run queues nothing: the weld survives a tick and the server keeps running.
    server.tick(&BTreeMap::new());
    assert_eq!(server.sandbox.constraints().len(), 2);
}

#[test]
fn script_reads_and_writes_players() {
    let Some(inst) = gm_content::find_install() else {
        eprintln!("skipped: no Garry's Mod install");
        return;
    };
    let mount = gm_content::Mount::for_install(&inst).unwrap();
    let world = gm_dedicated::load_world(&mount, "gm_construct").unwrap();
    let mut server = gm_server::Server::new(&world.collision, world.physics, world.world_body);
    server.add_player(
        1,
        glam::Vec3::new(823.235, -500.0, -143.0),
        glam::Vec3::new(30.0, 0.0, 0.0),
    );
    let mut rt = gm_script::Runtime::new();
    rt.load(gm_script::ActorSpec {
        addon: "test".into(),
        name: "medic".into(),
        priority: 0,
        source: "events.on('tick.pre', function()
                local p = world.player(1)
                state.seen = { p.health, p.armor, p.alive, #p.weapons, p.active_weapon, world.player(7) == nil }
                if state.done then return end
                state.done = true
                world.queue('player.set_health', 1, 42)
                world.queue('player.set_armor', 1, 25)
                world.queue('player.damage', 1, { amount = 10, type = 0 })
                world.queue('player.give_weapon', 1, 'weapon_rpg')
                world.queue('player.select_weapon', 1, 'weapon_357')
                world.queue('player.teleport', 1, { x = 823.235, y = -400, z = -143 })
                world.queue('player.set_health', 7, 1)
                world.queue('player.give_weapon', 1, 'weapon_nope')
            end)"
        .into(),
        config: Default::default(),
    })
    .unwrap();
    let before: (i32, i32, bool, usize, String, bool) = {
        let applied = rt.pre_tick(&mut server);
        assert_eq!((applied.writes, applied.rejected), (6, 2), "{applied:?}");
        rt.actors()[0]
            .eval("return table.unpack(state.seen)")
            .unwrap()
    };
    assert!(before.2);
    assert_eq!(before.0, 100);
    assert!(before.5, "player 7 must be nil");
    let (health, weapons, active, origin_y) = {
        let p = &server.players[&1];
        (
            p.health,
            p.weapons.clone(),
            p.active_weapon.clone(),
            p.state.origin.y,
        )
    };
    assert!(
        (32..42).contains(&health),
        "damage 10 with armor 25: {health}"
    );
    assert!(weapons.iter().any(|w| w == "weapon_rpg"), "{weapons:?}");
    assert_eq!(active, "weapon_357");
    assert!((origin_y - -400.0).abs() < 1.0, "{origin_y}");
    server.tick(&BTreeMap::new());
    let applied = rt.pre_tick(&mut server);
    assert_eq!(applied.writes, 0);
    let after: (i32, i32, bool, usize, String, bool) = rt.actors()[0]
        .eval("return table.unpack(state.seen)")
        .unwrap();
    assert_eq!(after.4, "weapon_357");
    assert_eq!(after.3, before.3, "the loadout is complete after spawn");
    assert_eq!(after.0, health);
}
