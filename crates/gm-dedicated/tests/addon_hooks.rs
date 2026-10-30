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
        config: relaxed(),
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

#[test]
fn addon_net_messages_round_trip_over_the_loopback() {
    let Some(inst) = gm_content::find_install() else {
        eprintln!("skipped: no Garry's Mod install");
        return;
    };
    let mount = gm_content::Mount::for_install(&inst).unwrap();
    let world = gm_dedicated::load_world(&mount, "gm_construct").unwrap();
    let mut server = gm_server::Server::new(&world.collision, world.physics, world.world_body);
    let spec = |name: &str, source: &str| gm_script::ActorSpec {
        addon: "test".into(),
        name: name.into(),
        priority: 0,
        source: source.into(),
        config: relaxed(),
    };
    let mut rt = gm_script::Runtime::new();
    rt.load(spec(
        "echo",
        "net.receive('ping', function(p)
            state.pings = (state.pings or 0) + 1
            state.from = p.player
            net.send('pong', { echo = p.data.n, list = p.data.list, from = p.player, half = 0.5 }, p.player)
            net.send('pong', { echo = -1 }, 99)
        end)",
    ))
    .unwrap();
    let mut sn = gm_net::ServerNet::new("gm_construct", glam::Vec3::new(823.235, -500.0, -143.0));
    sn.hooks = Some(Box::new(rt));
    let mut crt = gm_script::Runtime::client();
    crt.load(spec(
        "hud",
        "events.on('snapshot.received', function()
            if state.sent then return end
            state.sent = true
            net.send('ping', { n = 7, list = { 'a', 'b', nil, 3 } })
        end)
        net.receive('pong', function(p)
            state.pongs = (state.pongs or 0) + 1
            state.echo = p.data.echo
            state.from = p.data.from
            state.half = p.data.half
            state.b = p.data.list[2]
            state.three = p.data.list[4]
        end)",
    ))
    .unwrap();
    let net = gm_net::Loopback::new(1, 0.0, 7);
    let (mut s_ep, mut c_ep) = (net.endpoint(0), net.endpoint(1));
    let mut client = gm_net::ClientNet::new(1, "bot");
    let mut last_tick = 0;
    for _ in 0..40 {
        client.tick(gm_net::NetCmd::default(), &mut c_ep);
        net.tick();
        sn.receive(&mut server, &mut s_ep);
        sn.tick(&mut server);
        sn.send(&server, &mut s_ep);
        client.receive(&mut c_ep);
        let fresh = client.server_tick != last_tick;
        last_tick = client.server_tick;
        crt.client_tick(&client.world, fresh);
        let inbox = std::mem::take(&mut client.addon_msgs);
        crt.client_net(&client.world, inbox);
        for (_, name, data) in crt.drain_net() {
            client.send_addon(&name, &data);
        }
    }
    let (pongs, echo, from, half, b, three): (u64, i64, u32, f64, String, i64) = crt.actors()[0]
        .eval("return state.pongs, state.echo, state.from, state.half, state.b, state.three")
        .unwrap();
    assert_eq!(pongs, 1, "the message to player 99 must not arrive");
    assert_eq!((echo, from, half), (7, client.player_id.unwrap(), 0.5));
    assert_eq!((b.as_str(), three), ("b", 3));
    let srv = sn.hooks.as_mut().unwrap();
    assert!(srv.drain_net().is_empty());
}

#[test]
fn cvars_create_set_clamp_replicate_and_change_events() {
    let Some(inst) = gm_content::find_install() else {
        eprintln!("skipped: no Garry's Mod install");
        return;
    };
    let mount = gm_content::Mount::for_install(&inst).unwrap();
    let world = gm_dedicated::load_world(&mount, "gm_construct").unwrap();
    let mut server = gm_server::Server::new(&world.collision, world.physics, world.world_body);
    let spec = |name: &str, source: &str| gm_script::ActorSpec {
        addon: "test".into(),
        name: name.into(),
        priority: 0,
        source: source.into(),
        config: relaxed(),
    };
    let mut rt = gm_script::Runtime::new();
    rt.load(spec(
        "cfg",
        "cvar.create('myaddon_speed', 10, { replicated = true, min = 0, max = 100, help = 'speed' })
        cvar.create('myaddon_secret', 'hidden')
        state.changes = {}
        events.on('cvar.changed', function(p) table.insert(state.changes, p.name .. '=' .. p.old .. '>' .. p.value) end)
        events.on('tick.pre', function()
            state.ticks = (state.ticks or 0) + 1
            if state.ticks == 5 then
                cvar.set('myaddon_speed', 250)
                cvar.set('myaddon_speed', 100)
                cvar.set('nope', 1)
                cvar.create('myaddon_speed', 999)
            end
            state.speed = cvar.get_number('myaddon_speed')
            state.maxprops = cvar.get_number('sbox_maxprops')
            if state.ticks == 30 then
                net.send('report', { speed = state.speed, maxprops = state.maxprops, changes = state.changes })
            end
        end)",
    ))
    .unwrap();
    let mut sn = gm_net::ServerNet::new("gm_construct", glam::Vec3::new(823.235, -500.0, -143.0));
    sn.admins.insert(1); // the first connection sets the cvar below
    sn.hooks = Some(Box::new(rt));
    let mut crt = gm_script::Runtime::client();
    crt.load(spec(
        "hud",
        "state.changes = {}
        events.on('cvar.changed', function(p)
            if p.name == 'sbox_maxprops' or p.name:sub(1, 7) == 'myaddon' then
                table.insert(state.changes, p.name .. '=' .. p.old .. '>' .. p.value)
            end
        end)
        net.receive('report', function(p) state.report = p.data end)
        events.on('tick.client', function()
            state.speed = cvar.get('myaddon_speed')
            state.secret = cvar.get('myaddon_secret')
            state.maxprops = cvar.get_number('sbox_maxprops')
            state.hurt = cvar.get_bool('sbox_playershurtplayers')
        end)",
    ))
    .unwrap();
    let net = gm_net::Loopback::new(1, 0.0, 7);
    let (mut s_ep, mut c_ep) = (net.endpoint(0), net.endpoint(1));
    let mut client = gm_net::ClientNet::new(1, "bot");
    let mut last_tick = 0;
    for i in 0..40 {
        if i == 20 {
            client.command(gm_net::Command::Cvar {
                name: "sbox_maxprops".into(),
                value: "50".into(),
            });
        }
        client.tick(gm_net::NetCmd::default(), &mut c_ep);
        net.tick();
        sn.receive(&mut server, &mut s_ep);
        sn.tick(&mut server);
        sn.send(&server, &mut s_ep);
        client.receive(&mut c_ep);
        let fresh = client.server_tick != last_tick;
        last_tick = client.server_tick;
        crt.client_tick(&client.world, fresh);
        let inbox = std::mem::take(&mut client.addon_msgs);
        crt.client_net(&client.world, inbox);
    }
    assert_eq!(server.sandbox.limits.max_props, 50);
    assert_eq!(server.cvar("myaddon_speed"), Some("100"));
    assert_eq!(server.cvars["myaddon_speed"].help, "speed");
    assert!(
        server.cvar_changes.is_empty(),
        "the net layer takes the log"
    );
    let (speed, secret, maxprops, hurt, changes): (String, Option<String>, f64, bool, Vec<String>) =
        crt.actors()[0]
            .eval("return state.speed, state.secret, state.maxprops, state.hurt, state.changes")
            .unwrap();
    assert_eq!(
        (speed.as_str(), secret, maxprops, hurt),
        ("100", None, 50.0, true)
    );
    assert_eq!(
        changes,
        vec![
            "myaddon_speed=>10".to_string(),
            "sbox_maxprops=>200".into(),
            "myaddon_speed=10>100".into(),
            "sbox_maxprops=200>50".into()
        ],
        "the client sees the welcome set and the creation as changes from nothing, then each change"
    );
    let (speed, maxprops, srv_changes): (f64, f64, Vec<String>) = crt.actors()[0]
        .eval("return state.report.speed, state.report.maxprops, state.report.changes")
        .unwrap();
    assert_eq!((speed, maxprops), (100.0, 50.0));
    assert_eq!(
        srv_changes,
        vec!["myaddon_speed=10>100".to_string(), "sbox_maxprops=200>50".into()],
        "250 clamps to 100 in one change; set to the same value, an unknown name and a second create are rejected"
    );
}

#[test]
fn sandbox_rules_addon_refuses_at_the_sandbox_limit() {
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
    server.set_cvar("sbox_maxprops", "3");
    let mut rt = gm_script::Runtime::new();
    let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../addons/examples/sandbox_rules");
    rt.load_addon(&root).unwrap();
    let crate_model = "models/props_junk/wood_crate001a.mdl";
    let mut spawned = 0;
    let idle = BTreeMap::new();
    for _ in 0..5 {
        rt.pre_tick(&mut server);
        if rt.prop_spawn_request(&mut server, 1, crate_model) {
            if let Some(ent) = server.spawn_prop(1, crate_model) {
                rt.entity_spawned(&mut server, 1, ent);
                spawned += 1;
            }
        }
        server.tick(&idle);
        rt.post_tick(&mut server);
    }
    assert_eq!(spawned, 3);
    assert_eq!(server.sandbox.props().len(), 3);
    let (hits, tick): (i64, i64) = rt.actors()[0]
        .eval("return state.hits, state.last_refused.tick")
        .unwrap();
    assert_eq!(hits, 2, "the Lua rule refused the 4th and 5th spawn");
    assert_eq!(tick, 5);
    // the Rust sandbox agrees: with the addon out of the way the 4th spawn is refused by it
    assert!(server.spawn_prop(1, crate_model).is_none());
}

#[test]
fn actor_owned_undo_entry_is_undone_by_the_player() {
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
    let undos_before = server.sandbox.undos(1).len();
    let mut rt = gm_script::Runtime::new();
    rt.load(gm_script::ActorSpec {
        addon: "test".into(),
        name: "builder".into(),
        priority: 0,
        source: format!(
            "events.on('tick.pre', function()
                if state.done then return end
                state.done = true
                world.queue('weld', {e1}, {{ ent2 = {e2} }})
                world.queue('undo.push', 1, {{ name = 'Contraption', entities = {{ {e1}, {e2} }} }})
                world.queue('undo.push', 7, {{ name = 'Nope', entities = {{ {e1} }} }})
                world.queue('undo.push', 1, {{ name = 'Gone', entities = {{ 9999 }} }})
            end)"
        ),
        config: relaxed(),
    })
    .unwrap();
    let applied = rt.pre_tick(&mut server);
    assert_eq!((applied.writes, applied.rejected), (2, 2), "{applied:?}");
    assert_eq!(server.sandbox.undos(1).len(), undos_before + 1);
    server.undo(1);
    assert_eq!(
        server.sandbox.props().len(),
        0,
        "the contraption entry removed both crates"
    );
    assert_eq!(server.sandbox.constraints().len(), 0);
}

/// The default 2 ms budget is for a game tick; under the parallel test load it can abort a
/// handler on its first dispatch, which these tests would read as "no writes".
fn relaxed() -> gm_script::ActorConfig {
    gm_script::ActorConfig {
        cpu_budget: std::time::Duration::from_millis(500),
        ..Default::default()
    }
}

#[test]
fn actor_constraints_with_an_owner_follow_the_player_limits_and_undo() {
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
    server.set_cvar("sbox_maxconstraints", "1");
    let undos_before = server.sandbox.undos(1).len();
    let mut rt = gm_script::Runtime::new();
    rt.load(gm_script::ActorSpec {
        addon: "test".into(),
        name: "builder".into(),
        priority: 0,
        source: format!(
            "events.on('tick.pre', function()
                if state.done then return end
                state.done = true
                world.queue('weld', {e1}, {{ ent2 = {e2}, owner = 1 }})
                world.queue('nocollide', {e1}, {{ ent2 = {e2}, owner = 1 }})
                world.queue('nocollide', {e1}, {{ ent2 = {e2}, owner = 7 }})
            end)"
        ),
        config: relaxed(),
    })
    .unwrap();
    let applied = rt.pre_tick(&mut server);
    assert_eq!((applied.writes, applied.rejected), (1, 2), "{applied:?}");
    assert_eq!(server.sandbox.constraints().len(), 1);
    assert_eq!(server.sandbox.undos(1).len(), undos_before + 1);
    assert!(
        server
            .events
            .iter()
            .any(|e| matches!(e, gm_sandbox::Event::LimitHit { limit } if limit == "constraints")),
        "{:?}",
        server.events
    );
    server.undo(1);
    assert_eq!(server.sandbox.constraints().len(), 0);
    assert_eq!(
        server.sandbox.props().len(),
        2,
        "undoing the weld keeps the crates"
    );
}

#[test]
fn remover_tool_in_luau_removes_the_hit_prop_and_fires_entity_removed() {
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
    let ent = server.spawn_prop(1, crate_model).unwrap();
    for _ in 0..10 {
        server.tick(&BTreeMap::new());
    }
    let mut rt = gm_script::Runtime::new();
    let root =
        std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../addons/examples/remover_lua");
    rt.load_addon(&root).unwrap();
    rt.load(gm_script::ActorSpec {
        addon: "test".into(),
        name: "watch".into(),
        priority: 1,
        source: "state.gone = {}
            events.on('entity.removed', function(p) table.insert(state.gone, p.entity) end)"
            .into(),
        config: relaxed(),
    })
    .unwrap();
    let gm_server::ToolClick::Unknown(hit) = server.tool_click_full(1, "remover_lua", false) else {
        panic!("the remover is not a Rust stool");
    };
    assert_eq!(hit.ent, Some(ent), "the crate is under the crosshair");
    let n = server.events.len();
    assert!(rt.tool_click(&mut server, 1, "remover_lua", &hit, false, &[]));
    assert_eq!(server.sandbox.props().len(), 0);
    let removed: Vec<u32> = server.events[n..]
        .iter()
        .filter_map(|e| match e {
            gm_sandbox::Event::Removed { prop } => Some(*prop),
            _ => None,
        })
        .collect();
    assert_eq!(removed, vec![ent]);
    for e in removed {
        rt.entity_removed(&mut server, 1, e);
    }
    let gone: Vec<u32> = rt.actors()[1].eval("return state.gone").unwrap();
    assert_eq!(gone, vec![ent]);
    // a click on the world does nothing
    assert!(matches!(
        server.tool_click_full(1, "remover_lua", false),
        gm_server::ToolClick::Unknown(_)
    ));
}

#[test]
fn published_client_addon_is_downloaded_over_a_lossy_loopback_and_runs_on_the_client() {
    let Some(inst) = gm_content::find_install() else {
        eprintln!("skipped: no Garry's Mod install");
        return;
    };
    let mount = gm_content::Mount::for_install(&inst).unwrap();
    let world = gm_dedicated::load_world(&mount, "gm_construct").unwrap();
    let mut server = gm_server::Server::new(&world.collision, world.physics, world.world_body);
    let dir = std::env::temp_dir().join(format!("gmr-dl-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(dir.join("addon/src")).unwrap();
    std::fs::create_dir_all(dir.join("addon/assets")).unwrap();
    std::fs::write(
        dir.join("addon/manifest.toml"),
        "id = \"hud_dl\"\nversion = \"1.2.3\"\nrealms = [\"client\"]\n\
         [[actors]]\nname = \"hud\"\nsource = \"src/hud.luau\"\nrealm = \"client\"\n",
    )
    .unwrap();
    std::fs::write(
        dir.join("addon/src/hud.luau"),
        "events.on('snapshot.received', function() state.snaps = (state.snaps or 0) + 1 end)",
    )
    .unwrap();
    let mut rng = 0x9E37_79B9_7F4A_7C15u64;
    let blob: Vec<u8> = (0..100_000u32)
        .map(|_| {
            rng ^= rng << 13;
            rng ^= rng >> 7;
            rng ^= rng << 17;
            (rng >> 24) as u8
        })
        .collect();
    std::fs::write(dir.join("addon/assets/blob.bin"), &blob).unwrap();
    let bytes = gm_script::package::pack(&dir.join("addon")).unwrap();
    assert!(
        bytes.len() > 60_000,
        "{} bytes: the blob must not shrink away",
        bytes.len()
    );
    let pkg = gm_script::Package::read(&bytes).unwrap();
    let (m, _) = pkg.load_realm("client").unwrap();
    let info = gm_net::AddonInfo {
        id: m.id.clone(),
        version: m.version.clone(),
        sha: pkg.sha256,
        size: bytes.len() as u64,
    };
    let mut sn = gm_net::ServerNet::new("gm_construct", glam::Vec3::new(823.235, -500.0, -143.0));
    sn.publish_addon(info.clone(), bytes.clone());
    sn.publish_addon(info.clone(), bytes.clone());
    assert_eq!(sn.addons.len(), 1, "the same sha is published once");
    let net = gm_net::Loopback::new(1, 0.1, 7);
    let (mut s_ep, mut c_ep) = (net.endpoint(0), net.endpoint(1));
    let mut client = gm_net::ClientNet::new(1, "bot");
    let mut crt = gm_script::Runtime::client();
    let mut asked = false;
    let mut loaded_at = None;
    let mut last_tick = 0;
    for t in 0..600 {
        client.tick(gm_net::NetCmd::default(), &mut c_ep);
        net.tick();
        sn.receive(&mut server, &mut s_ep);
        sn.tick(&mut server);
        sn.send(&server, &mut s_ep);
        client.receive(&mut c_ep);
        if !asked && !client.server_addons.is_empty() {
            assert_eq!(client.server_addons, vec![info.clone()]);
            client.want_addon(info.sha);
            asked = true;
        }
        for (sha, got) in std::mem::take(&mut client.downloaded) {
            assert_eq!(sha, info.sha);
            assert_eq!(got, bytes, "the package arrived whole and in order");
            let p = gm_script::Package::read(&got).unwrap();
            assert_eq!(p.sha256, sha, "the sha verifies the download");
            let path = dir.join(format!("{}.gmr", gm_net::msg::sha_hex(&sha)));
            std::fs::write(&path, &got).unwrap();
            let m = crt.load_addon(&path).unwrap();
            assert_eq!((m.id.as_str(), m.version.as_str()), ("hud_dl", "1.2.3"));
            assert_eq!(
                crt.actors().len(),
                1,
                "the client actor of the package runs"
            );
            loaded_at = Some(t);
        }
        let fresh = client.server_tick != last_tick;
        last_tick = client.server_tick;
        crt.client_tick(&client.world, fresh);
        if loaded_at.is_some_and(|l| t > l + 20) {
            break;
        }
    }
    let loaded_at = loaded_at.expect("the download finished");
    assert!(loaded_at < 200, "download took {loaded_at} ticks");
    assert!(
        !client.world.cvars.is_empty(),
        "replicated cvars flow next to the download"
    );
    assert!(
        net.dropped() > 20,
        "the loopback did lose packets: {}",
        net.dropped()
    );
    let snaps: u64 = crt.actors()[0].eval("return state.snaps").unwrap();
    assert!(snaps >= 5, "{snaps} snapshots");
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn a_hot_reloaded_client_addon_is_republished_and_the_client_reloads_it_keeping_state() {
    let Some(inst) = gm_content::find_install() else {
        eprintln!("skipped: no Garry's Mod install");
        return;
    };
    let mount = gm_content::Mount::for_install(&inst).unwrap();
    let world = gm_dedicated::load_world(&mount, "gm_construct").unwrap();
    let mut server = gm_server::Server::new(&world.collision, world.physics, world.world_body);
    let dir = std::env::temp_dir().join(format!("gmr-repub-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(dir.join("addon/src")).unwrap();
    std::fs::write(
        dir.join("addon/manifest.toml"),
        "id = \"hud_live\"\nversion = \"1.0.0\"\nrealms = [\"client\"]\n\
         [[actors]]\nname = \"hud\"\nsource = \"src/hud.luau\"\nrealm = \"client\"\n",
    )
    .unwrap();
    let hud = dir.join("addon/src/hud.luau");
    std::fs::write(
        &hud,
        "state.gen = 1\nevents.on('snapshot.received', function() state.snaps = (state.snaps or 0) + 1 end)",
    )
    .unwrap();
    let mut rt = gm_script::Runtime::new();
    rt.watch = true;
    rt.load_addon(&dir.join("addon")).unwrap();
    assert_eq!(
        rt.actors().len(),
        0,
        "a client-only addon has no server actors"
    );
    assert_eq!(
        rt.published.len(),
        1,
        "but its package is queued for clients"
    );
    let mut sn = gm_net::ServerNet::new("gm_construct", glam::Vec3::new(823.235, -500.0, -143.0));
    sn.hooks = Some(Box::new(rt));
    let net = gm_net::Loopback::new(1, 0.05, 3);
    let (mut s_ep, mut c_ep) = (net.endpoint(0), net.endpoint(1));
    let mut client = gm_net::ClientNet::new(1, "bot");
    let mut crt = gm_script::Runtime::client();
    let mut seen: std::collections::BTreeSet<[u8; 32]> = Default::default();
    let mut loads = Vec::new();
    let mut last_tick = 0;
    let mut edited_at = None;
    for t in 0..900 {
        client.tick(gm_net::NetCmd::default(), &mut c_ep);
        net.tick();
        sn.receive(&mut server, &mut s_ep);
        sn.tick(&mut server);
        sn.send(&server, &mut s_ep);
        client.receive(&mut c_ep);
        for a in client.server_addons.clone() {
            if seen.insert(a.sha) {
                client.want_addon(a.sha);
            }
        }
        for (sha, bytes) in std::mem::take(&mut client.downloaded) {
            let pkg = gm_script::Package::read(&bytes).unwrap();
            assert_eq!(pkg.sha256, sha);
            let path = dir.join(format!("{}.gmr", gm_net::msg::sha_hex(&sha)));
            std::fs::write(&path, &bytes).unwrap();
            let m = if crt.addon_paths.contains_key("hud_live") {
                crt.reload_addon(&path).unwrap()
            } else {
                crt.load_addon(&path).unwrap()
            };
            loads.push((t, m.version.clone()));
        }
        let fresh = client.server_tick != last_tick;
        last_tick = client.server_tick;
        crt.client_tick(&client.world, fresh);
        if loads.len() == 1 && edited_at.is_none() && t > loads[0].0 + 30 {
            let snaps: u64 = crt.actors()[0].eval("return state.snaps").unwrap();
            assert!(snaps >= 10, "{snaps} snapshots before the edit");
            std::fs::write(
                dir.join("addon/manifest.toml"),
                "id = \"hud_live\"\nversion = \"1.1.0\"\nrealms = [\"client\"]\n\
                 [[actors]]\nname = \"hud\"\nsource = \"src/hud.luau\"\nrealm = \"client\"\n",
            )
            .unwrap();
            std::fs::write(
                &hud,
                "state.gen = 2\nevents.on('snapshot.received', function() state.snaps = (state.snaps or 0) + 1 end)",
            )
            .unwrap();
            let f = std::fs::File::open(&hud).unwrap();
            f.set_modified(std::time::SystemTime::now() + std::time::Duration::from_secs(5))
                .unwrap();
            edited_at = Some(t);
        }
        if loads.len() == 2 && edited_at.is_some_and(|e| t > e + 60) {
            break;
        }
    }
    assert_eq!(loads.len(), 2, "{loads:?}");
    assert_eq!(
        (loads[0].1.as_str(), loads[1].1.as_str()),
        ("1.0.0", "1.1.0")
    );
    assert_eq!(seen.len(), 2, "two distinct packages were announced");
    assert_eq!(sn.addons.len(), 2, "both stay downloadable");
    assert_eq!(sn.addon_list().len(), 1, "the list names the latest only");
    assert_eq!(
        crt.actors().len(),
        1,
        "the reload replaced the actor, not added one"
    );
    let (gen, snaps): (u64, u64) = crt.actors()[0]
        .eval("return state.gen, state.snaps")
        .unwrap();
    assert_eq!(gen, 2, "the new source ran");
    assert!(snaps >= 40, "{snaps}: state survived the reload");
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn a_game_server_answers_browser_queries_and_heartbeats_to_the_master() {
    use gm_net::{MasterMsg, Transport};
    let Some(inst) = gm_content::find_install() else {
        eprintln!("skipped: no Garry's Mod install");
        return;
    };
    let mount = gm_content::Mount::for_install(&inst).unwrap();
    let world = gm_dedicated::load_world(&mount, "gm_construct").unwrap();
    let mut server = gm_server::Server::new(&world.collision, world.physics, world.world_body);
    let mut sn = gm_net::ServerNet::new("gm_construct", glam::Vec3::new(823.235, -500.0, -143.0));
    sn.info.name = "box".into();
    sn.info.port = 27015;
    sn.publish_addon(
        gm_net::AddonInfo {
            id: "hud".into(),
            version: "1.0.0".into(),
            sha: [7; 32],
            size: 3,
        },
        vec![1, 2, 3],
    );
    // endpoints: 0 the game server, 1 a player, 2 a browser, 3 the master
    let net = gm_net::Loopback::new(1, 0.0, 5);
    let (mut s_ep, mut p_ep, mut b_ep, mut m_ep) = (
        net.endpoint(0),
        net.endpoint(1),
        net.endpoint(2),
        net.endpoint(3),
    );
    let mut player = gm_net::ClientNet::new(1, "p1");
    let mut master = gm_net::MasterNet::default();
    let mut infos = Vec::new();
    for t in 0..12 {
        player.tick(gm_net::NetCmd::default(), &mut p_ep);
        if t == 1 || t == 8 {
            b_ep.send(0, MasterMsg::Query { nonce: 42 + t }.encode());
        }
        net.tick();
        sn.receive(&mut server, &mut s_ep);
        sn.tick(&mut server);
        sn.send(&server, &mut s_ep);
        if t == 9 {
            sn.heartbeat(&mut s_ep, 3);
        }
        player.receive(&mut p_ep);
        for (from, p) in b_ep.recv() {
            assert_eq!(from, 0);
            infos.push(MasterMsg::read(&p).unwrap());
        }
        master.receive(&mut m_ep, 1000 + t as u64);
    }
    assert_eq!(infos.len(), 2, "{infos:?}");
    let MasterMsg::Info { nonce, info } = &infos[0] else {
        panic!("{:?}", infos[0]);
    };
    assert_eq!(*nonce, 43);
    assert_eq!(
        (
            info.name.as_str(),
            info.map.as_str(),
            info.max_players,
            info.port
        ),
        ("box", "gm_construct", 16, 27015)
    );
    assert_eq!(info.addons, vec!["hud".to_string()]);
    let MasterMsg::Info { info: later, .. } = &infos[1] else {
        panic!("{:?}", infos[1]);
    };
    assert_eq!(later.players, 1, "the joined player is counted");
    assert_eq!(sn.clients.len(), 1, "a browser query opens no client slot");
    let (key, (hb, seen)) = master.servers.iter().next().expect("the heartbeat arrived");
    assert_eq!(key, "loop0:27015");
    assert_eq!((hb.players, hb.name.as_str()), (1, "box"));
    assert_eq!(*seen, 1010);
}

#[test]
fn snapshots_carry_only_the_props_in_the_players_interest_within_the_byte_budget() {
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
    let spawn = glam::Vec3::new(823.235, -500.0, -143.0);
    let mut sn = gm_net::ServerNet::new("gm_construct", spawn);
    sn.interest_radius = 2000.0;
    sn.snapshot_budget = 400; // ~4 crates per snapshot
    sn.use_pvs = false; // this one measures the radius and the budget alone
    let net = gm_net::Loopback::new(1, 0.0, 11);
    let (mut s_ep, mut c_ep) = (net.endpoint(0), net.endpoint(1));
    let mut client = gm_net::ClientNet::new(1, "p1");
    // connect first, then line crates up along +x: 60 of them 100 u apart, frozen in the air
    for _ in 0..3 {
        client.tick(gm_net::NetCmd::default(), &mut c_ep);
        net.tick();
        sn.receive(&mut server, &mut s_ep);
        sn.tick(&mut server);
        sn.send(&server, &mut s_ep);
        client.receive(&mut c_ep);
    }
    let me = client.player_id.expect("welcomed");
    let crate_model = "models/props_junk/wood_crate001a.mdl";
    let mut by_x: Vec<(u32, f32)> = Vec::new();
    for i in 0..60u32 {
        let ent = server.spawn_prop(me, crate_model).unwrap();
        let x = spawn.x + 100.0 * i as f32;
        assert!(server.teleport_prop(
            ent,
            glam::Vec3::new(x, spawn.y, spawn.z + 200.0),
            glam::Vec3::ZERO
        ));
        server.sandbox.set_frozen(ent, true);
        by_x.push((ent, x));
    }
    let origin = |server: &gm_server::Server<'_>| server.players[&me].state.origin;
    let expected =
        |server: &gm_server::Server<'_>, radius: f32| -> std::collections::BTreeSet<u32> {
            let o = origin(server);
            server
                .sandbox
                .props()
                .iter()
                .filter(|p| o.distance(p.pos) <= radius)
                .map(|p| p.id)
                .collect()
        };
    struct Measured<'a> {
        inner: &'a mut gm_net::LoopbackEndpoint,
        max_packet: usize,
    }
    impl gm_net::Transport for Measured<'_> {
        fn send(&mut self, to: usize, packet: Vec<u8>) {
            self.inner.send(to, packet);
        }
        fn recv(&mut self) -> Vec<(usize, Vec<u8>)> {
            let got = self.inner.recv();
            for (_, p) in &got {
                self.max_packet = self.max_packet.max(p.len());
            }
            got
        }
    }
    let mut c_ep = Measured {
        inner: &mut c_ep,
        max_packet: 0,
    };
    let mut run =
        |ticks: u32, server: &mut gm_server::Server<'_>, client: &mut gm_net::ClientNet| {
            for _ in 0..ticks {
                client.tick(gm_net::NetCmd::default(), &mut c_ep);
                net.tick();
                sn.receive(server, &mut s_ep);
                sn.tick(server);
                sn.send(server, &mut s_ep);
                client.receive(&mut c_ep);
            }
            c_ep.max_packet
        };
    let max_packet = run(120, &mut server, &mut client);
    let want = expected(&server, 2000.0);
    assert!(
        want.len() >= 15 && want.len() <= 25,
        "{} crates in range",
        want.len()
    );
    let got: std::collections::BTreeSet<u32> = client.world.entities.keys().copied().collect();
    assert_eq!(
        got, want,
        "the client holds exactly the crates within 2000 u"
    );
    assert!(
        max_packet <= gm_net::channel::MAX_PACKET,
        "{max_packet} B packet"
    );

    // walk 3000 u down the line: far crates come in, the ones left behind go
    let o = origin(&server);
    server.teleport_player(me, glam::Vec3::new(o.x + 3000.0, o.y, o.z));
    let max_packet = run(120, &mut server, &mut client);
    let want2 = expected(&server, 2000.0);
    assert_ne!(want, want2);
    let got2: std::collections::BTreeSet<u32> = client.world.entities.keys().copied().collect();
    assert_eq!(got2, want2, "interest follows the player");
    assert!(
        max_packet <= gm_net::channel::MAX_PACKET,
        "{max_packet} B packet"
    );
}

#[test]
fn stress_64_clients_and_2000_props_over_the_loopback() {
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
    server.set_cvar("sbox_maxprops", "-1");
    let spawn = glam::Vec3::new(823.235, -500.0, -143.0);
    let mut sn = gm_net::ServerNet::new("gm_construct", spawn);
    // players on a grid land inside buildings too, so the budget is measured without the PVS
    sn.use_pvs = false;
    // the full size in release (`cargo test --release -- stress --nocapture` prints the numbers),
    // a quarter of it in debug so the workspace run stays quick
    let (n_clients, n_props) = if cfg!(debug_assertions) {
        (16usize, 500u32)
    } else {
        (64usize, 2000u32)
    };
    let net = gm_net::Loopback::new(1, 0.0, 3);
    let mut s_ep = net.endpoint(0);
    let mut clients: Vec<(gm_net::ClientNet, gm_net::LoopbackEndpoint)> = (1..=n_clients)
        .map(|i| (gm_net::ClientNet::new(1, &format!("p{i}")), net.endpoint(i)))
        .collect();
    let mut step =
        |server: &mut gm_server::Server<'_>,
         clients: &mut Vec<(gm_net::ClientNet, gm_net::LoopbackEndpoint)>| {
            for (c, ep) in clients.iter_mut() {
                c.tick(gm_net::NetCmd::default(), ep);
            }
            net.tick();
            sn.receive(server, &mut s_ep);
            let t0 = std::time::Instant::now();
            sn.tick(server);
            let tick = t0.elapsed();
            let t0 = std::time::Instant::now();
            sn.send(server, &mut s_ep);
            let send = t0.elapsed();
            for (c, ep) in clients.iter_mut() {
                c.receive(ep);
            }
            (tick, send)
        };
    for _ in 0..5 {
        step(&mut server, &mut clients);
    }
    assert!(
        clients.iter().all(|(c, _)| c.player_id.is_some()),
        "all welcomed"
    );
    // players in a 8 x 8 grid 300 u apart, crates in a 50 x 40 grid 60 u apart over the same area
    for (i, (c, _)) in clients.iter().enumerate() {
        let me = c.player_id.unwrap();
        let p = spawn + glam::Vec3::new((i % 8) as f32 * 300.0, (i / 8) as f32 * 300.0, 0.0);
        server.teleport_player(me, p);
    }
    let crate_model = "models/props_junk/wood_crate001a.mdl";
    let t0 = std::time::Instant::now();
    for i in 0..n_props {
        let ent = server.spawn_prop(1, crate_model).expect("spawned");
        let p = spawn
            + glam::Vec3::new(
                (i % 50) as f32 * 60.0 - 300.0,
                (i / 50) as f32 * 60.0 - 300.0,
                150.0,
            );
        server.teleport_prop(ent, p, glam::Vec3::ZERO);
        server.sandbox.set_frozen(ent, true);
    }
    eprintln!(
        "spawned {n_props} crates in {:.2} s",
        t0.elapsed().as_secs_f32()
    );
    let mut tick_total = std::time::Duration::ZERO;
    let mut send_total = std::time::Duration::ZERO;
    let ticks = 200;
    let rx0: u64 = clients.iter().map(|(c, _)| c.stats().bytes_received).sum();
    for _ in 0..ticks {
        let (t, s) = step(&mut server, &mut clients);
        tick_total += t;
        send_total += s;
    }
    let rx: u64 = clients
        .iter()
        .map(|(c, _)| c.stats().bytes_received)
        .sum::<u64>()
        - rx0;
    let per_client_per_tick = rx as f64 / ticks as f64 / n_clients as f64;
    let known: Vec<usize> = clients
        .iter()
        .map(|(c, _)| c.world.entities.len())
        .collect();
    let (min_known, max_known) = (*known.iter().min().unwrap(), *known.iter().max().unwrap());
    eprintln!(
        "STRESS {n_clients} clients x {n_props} props: tick {:.2} ms, send {:.2} ms (per tick), {per_client_per_tick:.0} B/tick/client ({:.1} KB/s), known entities {min_known}..{max_known}, dropped snapshots {}",
        tick_total.as_secs_f64() * 1000.0 / ticks as f64,
        send_total.as_secs_f64() * 1000.0 / ticks as f64,
        per_client_per_tick * 66.67 / 1024.0,
        clients.iter().map(|(c, _)| c.dropped_snapshots).sum::<u64>()
    );
    assert!(
        per_client_per_tick < 1300.0,
        "{per_client_per_tick} B/tick/client"
    );
    assert!(min_known > 0, "every client sees something");
}

#[test]
fn a_crate_out_of_the_players_pvs_is_not_sent_until_the_player_can_see_it() {
    let Some(inst) = gm_content::find_install() else {
        eprintln!("skipped: no Garry's Mod install");
        return;
    };
    let mount = gm_content::Mount::for_install(&inst).unwrap();
    let world = gm_dedicated::load_world(&mount, "gm_construct").unwrap();
    assert!(
        world.collision.num_clusters() > 100,
        "gm_construct has a PVS"
    );
    let mut server = gm_server::Server::new(&world.collision, world.physics, world.world_body);
    let m2 = gm_content::Mount::for_install(&inst).unwrap();
    server.model_loader = Some(Box::new(move |m: &str| {
        gm_dedicated::model_info(&m2, m).ok()
    }));
    let spawn = glam::Vec3::new(823.235, -500.0, -143.0);
    let eye = spawn + glam::Vec3::new(0.0, 0.0, 64.0);
    let my_cluster = world.collision.cluster_at(eye);
    assert!(my_cluster >= 0);
    // a point in open space within 2 500 u that the spawn cannot see
    let mut hidden = None;
    'search: for dx in (-2500..=2500).step_by(100) {
        for dy in (-2500..=2500).step_by(100) {
            for dz in [0.0f32, 200.0, -200.0] {
                let p = spawn + glam::Vec3::new(dx as f32, dy as f32, 100.0 + dz);
                let c = world.collision.cluster_at(p);
                if c >= 0
                    && world.collision.point_contents(p) == 0
                    && !world.collision.visible(my_cluster, c)
                    && world.collision.visible(c, c)
                {
                    hidden = Some(p);
                    break 'search;
                }
            }
        }
    }
    let hidden = hidden.expect("some open point within 2500 u is out of the spawn's PVS");
    let mut sn = gm_net::ServerNet::new("gm_construct", spawn);
    let net = gm_net::Loopback::new(1, 0.0, 2);
    let (mut s_ep, mut c_ep) = (net.endpoint(0), net.endpoint(1));
    let mut client = gm_net::ClientNet::new(1, "p1");
    let mut run = |n: u32, server: &mut gm_server::Server<'_>, client: &mut gm_net::ClientNet| {
        for _ in 0..n {
            client.tick(gm_net::NetCmd::default(), &mut c_ep);
            net.tick();
            sn.receive(server, &mut s_ep);
            sn.tick(server);
            sn.send(server, &mut s_ep);
            client.receive(&mut c_ep);
        }
    };
    run(3, &mut server, &mut client);
    let me = client.player_id.unwrap();
    let crate_model = "models/props_junk/wood_crate001a.mdl";
    let seen = server.spawn_prop(me, crate_model).unwrap();
    server.teleport_prop(
        seen,
        spawn + glam::Vec3::new(150.0, 0.0, 100.0),
        glam::Vec3::ZERO,
    );
    server.sandbox.set_frozen(seen, true);
    let unseen = server.spawn_prop(me, crate_model).unwrap();
    server.teleport_prop(unseen, hidden, glam::Vec3::ZERO);
    server.sandbox.set_frozen(unseen, true);
    run(30, &mut server, &mut client);
    assert!(client.world.entities.contains_key(&seen));
    assert!(
        !client.world.entities.contains_key(&unseen),
        "the crate at {hidden:?} is within range but out of the PVS"
    );
    server.teleport_player(me, hidden - glam::Vec3::new(0.0, 0.0, 64.0));
    run(30, &mut server, &mut client);
    assert!(
        client.world.entities.contains_key(&unseen),
        "standing next to it, it is sent"
    );
}

/// Lag compensation: the target runs sideways at full speed; the shooter aims where the target
/// stood 10 server ticks ago (the pose its client rendered) and stamps that tick on the command.
/// Without the rewind the bullet passes behind the target (it moved > 20 u, hull half-width 16).
#[test]
fn lag_compensation_hits_the_target_where_the_shooter_saw_it() {
    let Some(inst) = gm_content::find_install() else {
        eprintln!("skipped: no Garry's Mod install");
        return;
    };
    let mount = gm_content::Mount::for_install(&inst).unwrap();
    let world = gm_dedicated::load_world(&mount, "gm_construct").unwrap();
    let mut server = gm_server::Server::new(&world.collision, world.physics, world.world_body);
    let spawn = glam::Vec3::new(823.235, -500.0, -143.0);
    let (shooter, target) = (1u32, 2u32);
    server.add_player(shooter, spawn, glam::Vec3::ZERO);
    // 150 u ahead: open floor at the spawn (a wall stands ~220 u out along +x)
    let target_start = spawn + glam::Vec3::new(150.0, 0.0, 0.0);
    server.add_player(target, target_start, glam::Vec3::new(0.0, 90.0, 0.0));
    assert!(server.give_weapon(shooter, "weapon_pistol"));
    let idle = |view: glam::Vec3, seen: u32, buttons: u32, fwd: f32| gm_server::PlayerCmd {
        seen_tick: seen,
        cmd: gm_move::UserCmd {
            buttons,
            view_angles: view,
            forward_move: fwd,
            side_move: 0.0,
            up_move: 0.0,
        },
        physgun: false,
        mouse_x: 0.0,
        mouse_y: 0.0,
    };
    // settle on the ground, then the target runs along +y (its left) at full speed
    for _ in 0..30 {
        let mut cmds = std::collections::BTreeMap::new();
        cmds.insert(shooter, idle(glam::Vec3::ZERO, 0, 0, 0.0));
        cmds.insert(target, idle(glam::Vec3::new(0.0, 90.0, 0.0), 0, 0, 0.0));
        server.tick(&cmds);
    }
    let mut poses = Vec::new();
    for _ in 0..60 {
        poses.push((
            server.tick_count() as u32,
            server.players[&target].state.origin,
        ));
        let mut cmds = std::collections::BTreeMap::new();
        cmds.insert(shooter, idle(glam::Vec3::ZERO, 0, 0, 0.0));
        cmds.insert(target, idle(glam::Vec3::new(0.0, 90.0, 0.0), 0, 0, 10000.0));
        server.tick(&cmds);
    }
    // the shooter aims at the chest of the pose it saw 10 ticks ago
    let aim_at = |server: &gm_server::Server<'_>, seen_pos: glam::Vec3| {
        let eye = server.players[&shooter].state.origin + glam::Vec3::new(0.0, 0.0, 64.0);
        let d = seen_pos + glam::Vec3::new(0.0, 0.0, 48.0) - eye;
        let yaw = d.y.atan2(d.x).to_degrees();
        let pitch = -(d.z.atan2((d.x * d.x + d.y * d.y).sqrt())).to_degrees();
        glam::Vec3::new(pitch, yaw, 0.0)
    };
    let run = |server: &mut gm_server::Server<'_>,
               poses: &mut Vec<(u32, glam::Vec3)>,
               view,
               seen,
               buttons| {
        poses.push((
            server.tick_count() as u32,
            server.players[&target].state.origin,
        ));
        let mut cmds = std::collections::BTreeMap::new();
        cmds.insert(shooter, idle(view, seen, buttons, 0.0));
        cmds.insert(target, idle(glam::Vec3::new(0.0, 90.0, 0.0), 0, 0, 10000.0));
        server.tick(&cmds);
    };
    // control: the same aim resolved against the current poses (seen_tick 0) misses
    let (_, seen_pos) = poses[poses.len() - 10];
    let now = server.players[&target].state.origin;
    assert!(
        (now - seen_pos).length() > 20.0,
        "the target moved {} u in 10 ticks",
        (now - seen_pos).length()
    );
    let health = server.players[&target].health;
    // the shooter stands 150 u straight behind the seen pose, so the ray crosses the target's
    // path at a right angle and the 30 u it has moved since is a clean miss (hull half-width 16)
    server.teleport_player(shooter, seen_pos - glam::Vec3::new(150.0, 0.0, 0.0));
    let view = aim_at(&server, seen_pos);
    run(&mut server, &mut poses, view, 0, gm_server::IN_ATTACK);
    assert_eq!(
        server.players[&target].health, health,
        "without the rewind the shot passes behind"
    );
    for _ in 0..20 {
        run(&mut server, &mut poses, glam::Vec3::ZERO, 0, 0);
    }
    // with the rewind the same kind of shot lands
    let (seen_tick, seen_pos) = poses[poses.len() - 10];
    let health = server.players[&target].health;
    // the shooter stands 150 u straight behind the seen pose, so the ray crosses the target's
    // path at a right angle and the 30 u it has moved since is a clean miss (hull half-width 16)
    server.teleport_player(shooter, seen_pos - glam::Vec3::new(150.0, 0.0, 0.0));
    let view = aim_at(&server, seen_pos);
    run(
        &mut server,
        &mut poses,
        view,
        seen_tick,
        gm_server::IN_ATTACK,
    );
    assert!(
        server.players[&target].health < health,
        "rewound to tick {seen_tick}, the shot lands ({} -> {})",
        health,
        server.players[&target].health
    );
}

#[test]
fn a_server_requiring_auth_welcomes_only_clients_that_prove_an_identity() {
    let Some(inst) = gm_content::find_install() else {
        eprintln!("skipped: no Garry's Mod install");
        return;
    };
    let mount = gm_content::Mount::for_install(&inst).unwrap();
    let world = gm_dedicated::load_world(&mount, "gm_construct").unwrap();
    let mut server = gm_server::Server::new(&world.collision, world.physics, world.world_body);
    let spawn = glam::Vec3::new(823.235, -500.0, -143.0);
    let mut sn = gm_net::ServerNet::new("gm_construct", spawn);
    sn.require_auth = true;
    let net = gm_net::Loopback::new(2, 0.1, 7);
    let (mut s_ep, mut c1, mut c2) = (net.endpoint(0), net.endpoint(1), net.endpoint(2));
    let (secret, public) = gm_script::package::keygen();
    let mut anon = gm_net::ClientNet::new(1, "anon");
    let mut known = gm_net::ClientNet::new(1, "known").with_key(&secret);
    assert_eq!(known.identity, Some(public));
    for _ in 0..60 {
        anon.tick(gm_net::NetCmd::default(), &mut c1);
        known.tick(gm_net::NetCmd::default(), &mut c2);
        net.tick();
        sn.receive(&mut server, &mut s_ep);
        sn.tick(&mut server);
        sn.send(&server, &mut s_ep);
        anon.receive(&mut c1);
        known.receive(&mut c2);
    }
    assert!(anon.player_id.is_none(), "no key, no Welcome");
    let id = known
        .player_id
        .expect("the signed challenge gets the Welcome");
    assert_eq!(server.players[&id].identity, Some(public));
    // without the requirement everyone is welcomed, and a key still proves the identity
    let mut sn = gm_net::ServerNet::new("gm_construct", spawn);
    let mut server2 = gm_server::Server::new(
        &world.collision,
        gm_physics::World::new(600.0, gm_core::tick::DEFAULT_TICK_INTERVAL, 1),
        world.world_body,
    );
    let net = gm_net::Loopback::new(2, 0.0, 8);
    let (mut s_ep, mut c1, mut c2) = (net.endpoint(0), net.endpoint(1), net.endpoint(2));
    let mut anon = gm_net::ClientNet::new(1, "anon");
    let mut known = gm_net::ClientNet::new(1, "known").with_key(&secret);
    for _ in 0..60 {
        anon.tick(gm_net::NetCmd::default(), &mut c1);
        known.tick(gm_net::NetCmd::default(), &mut c2);
        net.tick();
        sn.receive(&mut server2, &mut s_ep);
        sn.tick(&mut server2);
        sn.send(&server2, &mut s_ep);
        anon.receive(&mut c1);
        known.receive(&mut c2);
    }
    let a = anon.player_id.expect("welcomed without a key");
    let k = known.player_id.expect("welcomed with a key");
    assert_eq!(server2.players[&a].identity, None);
    assert_eq!(server2.players[&k].identity, Some(public));
}

/// Connection lifecycle: a silent client times out (its props stay), an admin kicks and bans
/// another one (its reconnect is refused), a full server refuses, a leaving client frees its slot.
#[test]
fn connections_time_out_get_kicked_banned_refused_when_full_and_leave() {
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
    let spawn = glam::Vec3::new(823.235, -500.0, -143.0);
    let mut sn = gm_net::ServerNet::new("gm_construct", spawn);
    sn.timeout_ticks = 30;
    sn.max_players = 2;
    let (admin_secret, admin_public) = gm_script::package::keygen();
    let (victim_secret, victim_public) = gm_script::package::keygen();
    sn.admin_keys.insert(admin_public);
    let net = gm_net::Loopback::new(1, 0.0, 11);
    let mut s_ep = net.endpoint(0);
    let mut eps: Vec<_> = (1..=4).map(|i| net.endpoint(i)).collect();
    let mut admin = gm_net::ClientNet::new(1, "admin").with_key(&admin_secret);
    let mut victim = gm_net::ClientNet::new(1, "victim").with_key(&victim_secret);
    let run = |n: u32,
               server: &mut gm_server::Server<'_>,
               sn: &mut gm_net::ServerNet,
               s_ep: &mut gm_net::LoopbackEndpoint,
               eps: &mut Vec<gm_net::LoopbackEndpoint>,
               clients: &mut [(&mut gm_net::ClientNet, usize)]| {
        for _ in 0..n {
            for (c, e) in clients.iter_mut() {
                c.tick(gm_net::NetCmd::default(), &mut eps[*e]);
            }
            net.tick();
            sn.receive(server, s_ep);
            sn.tick(server);
            sn.send(server, s_ep);
            for (c, e) in clients.iter_mut() {
                c.receive(&mut eps[*e]);
            }
        }
    };
    run(
        20,
        &mut server,
        &mut sn,
        &mut s_ep,
        &mut eps,
        &mut [(&mut admin, 0), (&mut victim, 1)],
    );
    let admin_id = admin.player_id.expect("admin in");
    let victim_id = victim.player_id.expect("victim in");
    assert_eq!(server.players[&victim_id].identity, Some(victim_public));
    let log = sn.take_log();
    assert!(log.iter().any(|l| l.contains("admin joined")), "{log:?}");
    // full: a third client is refused
    let mut third = gm_net::ClientNet::new(1, "third");
    run(
        20,
        &mut server,
        &mut sn,
        &mut s_ep,
        &mut eps,
        &mut [(&mut admin, 0), (&mut victim, 1), (&mut third, 2)],
    );
    assert_eq!(third.disconnected.as_deref(), Some("full"));
    assert!(third.player_id.is_none());
    run(
        40,
        &mut server,
        &mut sn,
        &mut s_ep,
        &mut eps,
        &mut [(&mut admin, 0), (&mut victim, 1)],
    );
    assert_eq!(
        sn.clients.len(),
        2,
        "the refused slot is gone after its timeout"
    );
    // the victim spawns a crate, then the admin bans it: the crate stays, the player goes
    victim.command(gm_net::Command::Spawn {
        model: "models/props_junk/wood_crate001a.mdl".into(),
    });
    run(
        10,
        &mut server,
        &mut sn,
        &mut s_ep,
        &mut eps,
        &mut [(&mut admin, 0), (&mut victim, 1)],
    );
    assert_eq!(server.sandbox.props().len(), 1);
    admin.command(gm_net::Command::Kick {
        player: victim_id,
        ban: true,
    });
    run(
        20,
        &mut server,
        &mut sn,
        &mut s_ep,
        &mut eps,
        &mut [(&mut admin, 0), (&mut victim, 1)],
    );
    assert_eq!(victim.disconnected.as_deref(), Some("banned"));
    assert!(!server.players.contains_key(&victim_id));
    assert_eq!(server.sandbox.props().len(), 1, "the leaver's crate stays");
    assert!(sn.bans.contains(&victim_public));
    // the banned identity reconnecting is refused
    let mut again = gm_net::ClientNet::new(1, "victim2").with_key(&victim_secret);
    run(
        20,
        &mut server,
        &mut sn,
        &mut s_ep,
        &mut eps,
        &mut [(&mut admin, 0), (&mut again, 3)],
    );
    assert_eq!(again.disconnected.as_deref(), Some("banned"));
    // a non-admin kick is denied
    let mut peer = gm_net::ClientNet::new(1, "peer");
    run(
        20,
        &mut server,
        &mut sn,
        &mut s_ep,
        &mut eps,
        &mut [(&mut admin, 0), (&mut peer, 1)],
    );
    peer.command(gm_net::Command::Kick {
        player: admin_id,
        ban: false,
    });
    run(
        10,
        &mut server,
        &mut sn,
        &mut s_ep,
        &mut eps,
        &mut [(&mut admin, 0), (&mut peer, 1)],
    );
    assert!(server.players.contains_key(&admin_id));
    // the peer leaves: its slot is freed at once; the admin goes silent and times out
    peer.disconnect();
    run(
        5,
        &mut server,
        &mut sn,
        &mut s_ep,
        &mut eps,
        &mut [(&mut admin, 0), (&mut peer, 1)],
    );
    let before = sn.clients.len();
    run(
        40,
        &mut server,
        &mut sn,
        &mut s_ep,
        &mut eps,
        &mut [(&mut peer, 1)],
    );
    assert!(
        sn.clients.len() < before,
        "the admin timed out: {} -> {}",
        before,
        sn.clients.len()
    );
    assert!(!server.players.contains_key(&admin_id));
    let log = sn.take_log();
    assert!(log.iter().any(|l| l.contains("timed out")), "{log:?}");
}

#[test]
fn actors_see_the_players_identity_and_player_disconnected() {
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
    server.players.get_mut(&1).unwrap().identity = Some([0xab; 32]);
    let mut rt = gm_script::Runtime::new();
    rt.load(gm_script::ActorSpec {
        addon: "test".into(),
        name: "watch".into(),
        priority: 1,
        source: "state.gone = {}
            events.on('tick.pre', function() state.identity = world.player(1).identity end)
            events.on('player.disconnected', function(p) table.insert(state.gone, p.player) end)"
            .into(),
        config: relaxed(),
    })
    .unwrap();
    rt.pre_tick(&mut server);
    let identity: String = rt.actors()[0].eval("return state.identity").unwrap();
    assert_eq!(identity, "ab".repeat(32));
    assert!(server.remove_player(1));
    rt.player_disconnected(&mut server, 1);
    let gone: Vec<u32> = rt.actors()[0].eval("return state.gone").unwrap();
    assert_eq!(gone, vec![1]);
}

#[test]
fn the_admin_console_answers_status_cvar_say_and_denies_others() {
    let Some(inst) = gm_content::find_install() else {
        eprintln!("skipped: no Garry's Mod install");
        return;
    };
    let mount = gm_content::Mount::for_install(&inst).unwrap();
    let world = gm_dedicated::load_world(&mount, "gm_construct").unwrap();
    let mut server = gm_server::Server::new(&world.collision, world.physics, world.world_body);
    let spawn = glam::Vec3::new(823.235, -500.0, -143.0);
    let mut sn = gm_net::ServerNet::new("gm_construct", spawn);
    let (admin_secret, admin_public) = gm_script::package::keygen();
    sn.admin_keys.insert(admin_public);
    let net = gm_net::Loopback::new(1, 0.0, 13);
    let (mut s_ep, mut e1, mut e2) = (net.endpoint(0), net.endpoint(1), net.endpoint(2));
    let mut admin = gm_net::ClientNet::new(1, "admin").with_key(&admin_secret);
    let mut peer = gm_net::ClientNet::new(1, "peer");
    let mut run = |n: u32,
                   server: &mut gm_server::Server<'_>,
                   admin: &mut gm_net::ClientNet,
                   peer: &mut gm_net::ClientNet| {
        for _ in 0..n {
            admin.tick(gm_net::NetCmd::default(), &mut e1);
            peer.tick(gm_net::NetCmd::default(), &mut e2);
            net.tick();
            sn.receive(server, &mut s_ep);
            sn.tick(server);
            sn.send(server, &mut s_ep);
            admin.receive(&mut e1);
            peer.receive(&mut e2);
        }
    };
    run(20, &mut server, &mut admin, &mut peer);
    let peer_id = peer.player_id.unwrap();
    admin.events.clear();
    peer.events.clear();
    for line in [
        "status",
        "cvar sbox_maxprops 25",
        "cvar sbox_maxprops",
        "say hello all",
        "bogus",
        "kick 999",
    ] {
        admin.command(gm_net::Command::Console { line: line.into() });
    }
    peer.command(gm_net::Command::Console {
        line: "status".into(),
    });
    run(20, &mut server, &mut admin, &mut peer);
    let a = admin.events.join("\n");
    assert!(
        a.contains("gm_construct tick") && a.contains("players 2/"),
        "{a}"
    );
    assert!(a.contains(&format!("{peer_id} peer anonymous")), "{a}");
    assert!(a.contains("admin\n") || a.contains(" admin"), "{a}");
    assert!(a.matches("sbox_maxprops = 25").count() == 2, "{a}");
    assert_eq!(server.sandbox.limits.max_props, 25);
    assert!(a.contains("[server] hello all"), "{a}");
    assert!(a.contains("bogus: unknown command"), "{a}");
    assert!(a.contains("kick 999: no such player"), "{a}");
    let p = peer.events.join("\n");
    assert!(
        p.contains("[server] hello all") && p.contains("Console denied"),
        "{p}"
    );
    assert!(!p.contains("gm_construct tick"), "{p}");
}

#[test]
fn a_saved_world_loads_into_a_fresh_server_with_the_same_poses_and_constraints() {
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
    let spawn = glam::Vec3::new(823.235, -500.0, -143.0);
    server.add_player(1, spawn, glam::Vec3::new(20.0, 0.0, 0.0));
    let crate_model = "models/props_junk/wood_crate001a.mdl";
    let a = server.spawn_prop(1, crate_model).unwrap();
    let b = server.spawn_prop(1, crate_model).unwrap();
    server.teleport_prop(
        b,
        spawn + glam::Vec3::new(150.0, 0.0, 200.0),
        glam::Vec3::new(0.0, 45.0, 0.0),
    );
    server.sandbox.set_frozen(a, true);
    assert!(server
        .sandbox
        .weld(1, Some(a), Some(b), 0.0, false)
        .is_some());
    assert!(server
        .sandbox
        .rope(
            1,
            Some(b),
            None,
            glam::Vec3::ZERO,
            spawn + glam::Vec3::new(150.0, 0.0, 400.0),
            250.0,
            &gm_sandbox::ToolSettings::default()
        )
        .is_some());
    server.sync_after_script();
    for _ in 0..30 {
        server.tick(&BTreeMap::new());
    }
    let save = server.save_world("gm_construct");
    assert_eq!(save.props.len(), 2);
    assert_eq!(save.constraints.len(), 2);
    let json = serde_json::to_string(&save).unwrap();
    let back: gm_server::save::Save = serde_json::from_str(&json).unwrap();
    assert_eq!(back, save);
    // a fresh server on the same map
    let world2 = gm_dedicated::load_world(&mount, "gm_construct").unwrap();
    let mut fresh = gm_server::Server::new(&world2.collision, world2.physics, world2.world_body);
    let m3 = gm_content::Mount::for_install(&inst).unwrap();
    fresh.model_loader = Some(Box::new(move |m: &str| {
        gm_dedicated::model_info(&m3, m).ok()
    }));
    fresh.add_player(7, spawn, glam::Vec3::ZERO);
    assert_eq!(fresh.load_world(&back, 7), (2, 2));
    let before: Vec<_> = server
        .sandbox
        .props()
        .iter()
        .map(|p| (p.pos, p.angles, p.frozen))
        .collect();
    let after: Vec<_> = fresh
        .sandbox
        .props()
        .iter()
        .map(|p| (p.pos, p.angles, p.frozen))
        .collect();
    for (x, y) in before.iter().zip(&after) {
        assert!(
            x.0.distance(y.0) < 0.01 && x.1.distance(y.1) < 0.01 && x.2 == y.2,
            "{x:?} vs {y:?}"
        );
    }
    assert!(fresh.sandbox.props().iter().all(|p| p.owner == 7));
    let kinds: Vec<_> = fresh
        .sandbox
        .constraints()
        .iter()
        .map(|c| std::mem::discriminant(&c.kind))
        .collect();
    let kinds0: Vec<_> = server
        .sandbox
        .constraints()
        .iter()
        .map(|c| std::mem::discriminant(&c.kind))
        .collect();
    assert_eq!(kinds, kinds0);
    // the weld holds: the frozen crate keeps the hanging one in place over a second of physics
    let (fa, fb) = (fresh.sandbox.props()[0].id, fresh.sandbox.props()[1].id);
    let rel0 = fresh.sandbox.props()[1].pos - fresh.sandbox.props()[0].pos;
    for _ in 0..66 {
        fresh.tick(&BTreeMap::new());
    }
    let pa = fresh
        .sandbox
        .props()
        .iter()
        .find(|p| p.id == fa)
        .unwrap()
        .pos;
    let pb = fresh
        .sandbox
        .props()
        .iter()
        .find(|p| p.id == fb)
        .unwrap()
        .pos;
    assert!(
        (pb - pa).distance(rel0) < 2.0,
        "welded pair drifted {} u",
        (pb - pa).distance(rel0)
    );
}

#[test]
fn the_duplicator_copies_a_welded_pair_and_pastes_it_at_the_aim() {
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
    let spawn = glam::Vec3::new(823.235, -500.0, -143.0);
    server.add_player(1, spawn, glam::Vec3::new(20.0, 0.0, 0.0));
    let crate_model = "models/props_junk/wood_crate001a.mdl";
    let a = server.spawn_prop(1, crate_model).unwrap();
    let b = server.spawn_prop(1, crate_model).unwrap();
    let pa = server
        .sandbox
        .props()
        .iter()
        .find(|p| p.id == a)
        .unwrap()
        .pos;
    server.teleport_prop(
        b,
        pa + glam::Vec3::new(0.0, 0.0, 40.0),
        glam::Vec3::new(0.0, 30.0, 0.0),
    );
    server.sandbox.set_frozen(a, true);
    server.sandbox.set_frozen(b, true);
    assert!(server
        .sandbox
        .weld(1, Some(a), Some(b), 0.0, false)
        .is_some());
    server.sync_after_script();
    // copy: aim at the lower crate
    let gm_server::ToolClick::Done(ok) = server.tool_click_full(1, "duplicator", false) else {
        panic!("duplicator is a Rust stool");
    };
    assert!(ok, "the crate under the crosshair was copied");
    let clip = server.clipboards[&1].clone();
    assert_eq!((clip.props.len(), clip.constraints.len()), (2, 1));
    // paste: aim at the floor 300 u to the side
    server.teleport_player(1, spawn + glam::Vec3::new(0.0, 300.0, 0.0));
    let before = server.sandbox.props().len();
    let gm_server::ToolClick::Done(ok) = server.tool_click_full(1, "duplicator", true) else {
        panic!("duplicator is a Rust stool");
    };
    assert!(ok);
    assert_eq!(server.sandbox.props().len(), before + 2);
    assert_eq!(server.sandbox.constraints().len(), 2);
    let new: Vec<_> = server.sandbox.props()[before..].to_vec();
    let rel = new[1].pos - new[0].pos;
    assert!(
        (rel - glam::Vec3::new(0.0, 0.0, 40.0)).length() < 0.01,
        "relative pose kept: {rel:?}"
    );
    assert!(
        (new[1].angles.y - 30.0).abs() < 0.01 && new[0].frozen && new[1].frozen,
        "angles {:?} frozen {} {}",
        new[1].angles,
        new[0].frozen,
        new[1].frozen
    );
    assert!(
        (new[0].pos.y - (spawn.y + 300.0)).abs() < 60.0,
        "pasted near the aim: {:?}",
        new[0].pos
    );
    assert!(server
        .sandbox
        .undos(1)
        .iter()
        .any(|u| u.name == "Duplicator"));
    // undo removes the pasted pair and its weld
    server.undo(1);
    assert_eq!(server.sandbox.props().len(), before);
    assert_eq!(server.sandbox.constraints().len(), 1);
}

/// A server with `--server-key` signs its `Challenge`; a client learns the identity, a client
/// pinning the right key is welcomed, one pinning another key ends the connection before `Hello`
/// is answered, and a pinning client refuses a server without a key.
#[test]
fn server_identity_is_signed_in_challenge_and_pinned_by_the_client() {
    let Some(inst) = gm_content::find_install() else {
        eprintln!("skipped: no Garry's Mod install");
        return;
    };
    let mount = gm_content::Mount::for_install(&inst).unwrap();
    let world = gm_dedicated::load_world(&mount, "gm_construct").unwrap();
    let mut server = gm_server::Server::new(&world.collision, world.physics, world.world_body);
    let spawn = glam::Vec3::new(823.235, -500.0, -143.0);
    let mut sn = gm_net::ServerNet::new("gm_construct", spawn);
    let (server_secret, _) = gm_script::package::keygen();
    let server_public = sn.with_key(&server_secret);
    assert_eq!(sn.info.key, Some(server_public));
    let (_, other_public) = gm_script::package::keygen();
    let net = gm_net::Loopback::new(1, 0.0, 12);
    let mut s_ep = net.endpoint(0);
    let mut eps: Vec<_> = (1..=3).map(|i| net.endpoint(i)).collect();
    let mut plain = gm_net::ClientNet::new(1, "plain");
    let mut pinned = gm_net::ClientNet::new(1, "pinned").with_server_key(server_public);
    let mut wrong = gm_net::ClientNet::new(1, "wrong").with_server_key(other_public);
    for _ in 0..20 {
        for (c, e) in [(&mut plain, 0), (&mut pinned, 1), (&mut wrong, 2)] {
            c.tick(gm_net::NetCmd::default(), &mut eps[e]);
        }
        net.tick();
        sn.receive(&mut server, &mut s_ep);
        sn.tick(&mut server);
        sn.send(&server, &mut s_ep);
        for (c, e) in [(&mut plain, 0), (&mut pinned, 1), (&mut wrong, 2)] {
            c.receive(&mut eps[e]);
        }
    }
    assert_eq!(plain.server_identity, Some(server_public));
    assert!(plain.player_id.is_some());
    assert_eq!(pinned.server_identity, Some(server_public));
    assert!(pinned.player_id.is_some(), "pinning the right key connects");
    assert_eq!(
        wrong.disconnected.as_deref(),
        Some("server identity mismatch")
    );
    assert!(wrong.player_id.is_none());
    assert!(wrong.server_identity.is_none());

    let mut unsigned = gm_net::ServerNet::new("gm_construct", spawn);
    let net2 = gm_net::Loopback::new(1, 0.0, 13);
    let mut s2 = net2.endpoint(0);
    let mut e2 = net2.endpoint(1);
    let mut pinned2 = gm_net::ClientNet::new(1, "pinned").with_server_key(server_public);
    for _ in 0..20 {
        pinned2.tick(gm_net::NetCmd::default(), &mut e2);
        net2.tick();
        unsigned.receive(&mut server, &mut s2);
        unsigned.tick(&mut server);
        unsigned.send(&server, &mut s2);
        pinned2.receive(&mut e2);
    }
    assert_eq!(
        pinned2.disconnected.as_deref(),
        Some("server identity mismatch")
    );
    assert!(pinned2.player_id.is_none());
}

#[test]
fn game_events_reach_actors_and_a_script_plays_a_sound() {
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
    rt.load(gm_script::ActorSpec {
        addon: "test".into(),
        name: "log".into(),
        priority: 1,
        source: "state.log = {}
            state.vetoed = 0
            local function put(s) table.insert(state.log, s) end
            events.on('physgun.pickup_request', function(e)
                if state.vetoed == 0 then state.vetoed = 1 return false end
            end)
            events.on('player.damage', function(e) return e.amount * 0.5 end)
            state.collided = 0
            events.on('entity.collide', function(e)
                state.collided += 1
                state.other = e.other
                state.collide_z = e.pos.z
                state.collide_speed = e.speed
            end)
            events.on('physgun.pickup', function(e) put('pickup ' .. e.player .. ' ' .. e.entity) end)
            events.on('physgun.drop', function(e) put('drop ' .. e.player .. ' ' .. e.entity) end)
            events.on('physgun.freeze', function(e)
                put('freeze ' .. e.player .. ' ' .. e.entity)
                world.queue('sound.play', 0, { name = 'Weapon_Pistol.Single', pos = { x = 823, y = -500, z = -100 }, volume = 0.5 })
            end)
            events.on('player.damaged', function(e)
                put('damaged ' .. e.player .. ' ' .. e.amount .. ' ' .. e.type)
                world.queue('player.kill', e.player, true)
            end)
            events.on('player.death', function(e) put('death ' .. e.player .. ' ' .. e.type) end)
            events.on('player.spawned', function(e)
                put('spawned ' .. e.player)
                net.send('log', { log = state.log, vetoed = state.vetoed, collided = state.collided,
                    other = state.other, collide_z = state.collide_z, collide_speed = state.collide_speed }, e.player)
            end)"
            .into(),
        config: relaxed(),
    })
    .unwrap();
    let mut sn = gm_net::ServerNet::new("gm_construct", glam::Vec3::new(823.235, -500.0, -143.0));
    gm_script::attach(rt, &mut server, &mut sn.hooks);
    let mut crt = gm_script::Runtime::client();
    crt.load(gm_script::ActorSpec {
        addon: "test".into(),
        name: "hud".into(),
        priority: 1,
        source: "net.receive('log', function(p) state.d = p.data end)".into(),
        config: relaxed(),
    })
    .unwrap();
    let net = gm_net::Loopback::new(1, 0.0, 7);
    let (mut s_ep, mut c_ep) = (net.endpoint(0), net.endpoint(1));
    let mut client = gm_net::ClientNet::new(1, "bot");
    let mut step = |sn: &mut gm_net::ServerNet,
                    client: &mut gm_net::ClientNet,
                    server: &mut gm_server::Server<'_>,
                    crt: &mut gm_script::Runtime,
                    buttons: u32,
                    n: usize| {
        for _ in 0..n {
            client.tick(
                gm_net::NetCmd {
                    view_angles: glam::Vec3::new(30.0, 0.0, 0.0),
                    buttons,
                    weapon: 1,
                    ..Default::default()
                },
                &mut c_ep,
            );
            net.tick();
            sn.receive(server, &mut s_ep);
            sn.tick(server);
            sn.send(server, &mut s_ep);
            client.receive(&mut c_ep);
            crt.client_tick(&client.world, true);
            let inbox = std::mem::take(&mut client.addon_msgs);
            crt.client_net(&client.world, inbox);
        }
    };
    step(&mut sn, &mut client, &mut server, &mut crt, 0, 30);
    client.command(gm_net::Command::Spawn {
        model: "models/props_junk/wood_crate001a.mdl".into(),
    });
    step(&mut sn, &mut client, &mut server, &mut crt, 0, 30);
    let crate_id = server.sandbox.props()[0].id;
    use gm_move::buttons::{ATTACK, ATTACK2};
    step(&mut sn, &mut client, &mut server, &mut crt, ATTACK, 5);
    step(&mut sn, &mut client, &mut server, &mut crt, 0, 5);
    step(&mut sn, &mut client, &mut server, &mut crt, ATTACK, 5);
    step(
        &mut sn,
        &mut client,
        &mut server,
        &mut crt,
        ATTACK | ATTACK2,
        1,
    );
    // damage from the server side, as a bullet or a fall deals it: the handler halves it
    assert_eq!(server.damage_player(1, 30.0, 0), 15.0);
    // a second crate dropped from 100 u: a recorded impact against the world
    let dropped = server
        .spawn_prop(1, "models/props_junk/wood_crate001a.mdl")
        .unwrap();
    let pos = server
        .sandbox
        .props()
        .iter()
        .find(|p| p.id == dropped)
        .unwrap()
        .pos;
    assert!(server.teleport_prop(dropped, pos + glam::Vec3::Z * 100.0, glam::Vec3::ZERO));
    step(&mut sn, &mut client, &mut server, &mut crt, 0, 200);
    let log: Vec<String> = crt.actors()[0].eval("return state.d.log").unwrap();
    assert_eq!(
        log,
        vec![
            format!("pickup 1 {crate_id}"),
            format!("drop 1 {crate_id}"),
            format!("pickup 1 {crate_id}"),
            format!("freeze 1 {crate_id}"),
            "damaged 1 15 0".to_string(),
            "death 1 0".to_string(),
            "spawned 1".to_string(),
        ]
    );
    let vetoed: u32 = crt.actors()[0].eval("return state.d.vetoed").unwrap();
    assert_eq!(vetoed, 1, "the first pick-up was refused by the script");
    let collided: u32 = crt.actors()[0].eval("return state.d.collided").unwrap();
    assert!(collided >= 1, "the dropped crate hit the floor");
    let (other, z, speed): (u32, f32, f32) = crt.actors()[0]
        .eval("return state.d.other, state.d.collide_z, state.d.collide_speed")
        .unwrap();
    assert_eq!(other, 0, "against the world");
    assert!((-146.0..-140.0).contains(&z), "contact at the floor: {z}");
    assert!(speed > 0.0, "approach speed {speed}");
    assert!(server.sandbox.props()[0].frozen);
    let shot = client
        .sounds
        .iter()
        .find(|s| s.name == "Weapon_Pistol.Single")
        .unwrap_or_else(|| panic!("the script's sound reached the client: {:?}", client.sounds));
    assert_eq!(shot.pos, glam::Vec3::new(823.0, -500.0, -100.0));
    assert_eq!(shot.volume, 0.5);
}

#[test]
fn player_chat_is_rewritten_or_dropped_by_an_actor_before_it_reaches_the_clients() {
    let Some(inst) = gm_content::find_install() else {
        eprintln!("skipped: no Garry's Mod install");
        return;
    };
    let mount = gm_content::Mount::for_install(&inst).unwrap();
    let world = gm_dedicated::load_world(&mount, "gm_construct").unwrap();
    let mut server = gm_server::Server::new(&world.collision, world.physics, world.world_body);
    let mut rt = gm_script::Runtime::new();
    rt.load(gm_script::ActorSpec {
        addon: "test".into(),
        name: "filter".into(),
        priority: 1,
        source: "events.on('player.chat', function(e)
                if e.text == 'bad' then return false end
                if e.player == 1 then return string.upper(e.text) end
            end)"
            .into(),
        config: relaxed(),
    })
    .unwrap();
    let mut sn = gm_net::ServerNet::new("gm_construct", glam::Vec3::new(823.235, -500.0, -143.0));
    gm_script::attach(rt, &mut server, &mut sn.hooks);
    let net = gm_net::Loopback::new(1, 0.0, 7);
    let (mut s_ep, mut e1, mut e2) = (net.endpoint(0), net.endpoint(1), net.endpoint(2));
    let mut alice = gm_net::ClientNet::new(1, "alice");
    let mut bob = gm_net::ClientNet::new(1, "bob");
    let mut run = |n: u32,
                   server: &mut gm_server::Server<'_>,
                   alice: &mut gm_net::ClientNet,
                   bob: &mut gm_net::ClientNet| {
        for _ in 0..n {
            alice.tick(gm_net::NetCmd::default(), &mut e1);
            bob.tick(gm_net::NetCmd::default(), &mut e2);
            net.tick();
            sn.receive(server, &mut s_ep);
            sn.tick(server);
            sn.send(server, &mut s_ep);
            alice.receive(&mut e1);
            bob.receive(&mut e2);
        }
    };
    run(10, &mut server, &mut alice, &mut bob);
    alice.command(gm_net::Command::Chat {
        text: "hello there".into(),
    });
    alice.command(gm_net::Command::Chat { text: "bad".into() });
    bob.command(gm_net::Command::Chat {
        text: "  hi  ".into(),
    });
    bob.command(gm_net::Command::Chat { text: "   ".into() });
    run(10, &mut server, &mut alice, &mut bob);
    let chat = |c: &gm_net::ClientNet| -> Vec<String> {
        c.events
            .iter()
            .filter(|e| e.contains(": "))
            .cloned()
            .collect()
    };
    assert_eq!(chat(&alice), vec!["alice: HELLO THERE", "bob:   hi  "]);
    assert_eq!(chat(&bob), chat(&alice));
    assert!(sn.take_log().iter().any(|l| l == "chat alice: HELLO THERE"));
}

#[test]
fn tool_before_vetoes_a_click_tool_after_sees_the_result_and_noclip_toggles_with_an_event() {
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
    rt.load(gm_script::ActorSpec {
        addon: "test".into(),
        name: "watch".into(),
        priority: 1,
        source: "state.log = {}
            events.on('tool.before', function(e)
                table.insert(state.log, 'before ' .. e.tool .. ' ' .. e.right .. ' ' .. e.hit.ent)
                if e.tool == 'weld' and e.right == 1 then return false end
            end)
            events.on('tool.after', function(e)
                table.insert(state.log, 'after ' .. e.tool .. ' ' .. e.right .. ' ' .. e.done)
            end)
            events.on('player.noclip', function(e)
                table.insert(state.log, 'noclip ' .. e.player .. ' ' .. e.on)
                net.send('log', { log = state.log }, e.player)
            end)"
            .into(),
        config: relaxed(),
    })
    .unwrap();
    let mut sn = gm_net::ServerNet::new("gm_construct", glam::Vec3::new(823.235, -500.0, -143.0));
    gm_script::attach(rt, &mut server, &mut sn.hooks);
    let mut crt = gm_script::Runtime::client();
    crt.load(gm_script::ActorSpec {
        addon: "test".into(),
        name: "hud".into(),
        priority: 1,
        source: "net.receive('log', function(p) state.log = p.data.log end)".into(),
        config: relaxed(),
    })
    .unwrap();
    let net = gm_net::Loopback::new(1, 0.0, 7);
    let (mut s_ep, mut c_ep) = (net.endpoint(0), net.endpoint(1));
    let mut client = gm_net::ClientNet::new(1, "bot");
    let mut step = |sn: &mut gm_net::ServerNet,
                    client: &mut gm_net::ClientNet,
                    server: &mut gm_server::Server<'_>,
                    crt: &mut gm_script::Runtime,
                    n: usize| {
        for _ in 0..n {
            client.tick(
                gm_net::NetCmd {
                    view_angles: glam::Vec3::new(30.0, 0.0, 0.0),
                    weapon: 2,
                    ..Default::default()
                },
                &mut c_ep,
            );
            net.tick();
            sn.receive(server, &mut s_ep);
            sn.tick(server);
            sn.send(server, &mut s_ep);
            client.receive(&mut c_ep);
            crt.client_tick(&client.world, true);
            let inbox = std::mem::take(&mut client.addon_msgs);
            crt.client_net(&client.world, inbox);
        }
    };
    step(&mut sn, &mut client, &mut server, &mut crt, 10);
    client.command(gm_net::Command::Spawn {
        model: "models/props_junk/wood_crate001a.mdl".into(),
    });
    step(&mut sn, &mut client, &mut server, &mut crt, 20);
    let crate_id = server.sandbox.props()[0].id;
    // weld right click = weld to the world: vetoed by the script; the left click goes through
    let click = |right: bool| gm_net::Command::ToolClick {
        tool: "weld".into(),
        right,
        reload: false,
        settings: Vec::new(),
    };
    client.command(click(true));
    step(&mut sn, &mut client, &mut server, &mut crt, 5);
    client.command(click(false));
    step(&mut sn, &mut client, &mut server, &mut crt, 5);
    assert_eq!(
        server.sandbox.constraints().len(),
        0,
        "the world weld was vetoed"
    );
    assert_eq!(
        server.sandbox.tool(1).stage,
        1,
        "the left click selected the crate"
    );
    client.command(gm_net::Command::Noclip { on: true });
    step(&mut sn, &mut client, &mut server, &mut crt, 5);
    assert_eq!(
        server.players[&1].state.move_type,
        gm_move::MoveType::Noclip
    );
    let log: Vec<String> = crt.actors()[0].eval("return state.log").unwrap();
    assert_eq!(
        log,
        vec![
            format!("before weld 1 {crate_id}"),
            format!("before weld 0 {crate_id}"),
            "after weld 0 1".to_string(),
            "noclip 1 1".to_string(),
        ]
    );
    // the cvar refuses: the toggle is denied and nothing changes
    assert!(server.set_cvar("sbox_noclip", "0"));
    client.command(gm_net::Command::Noclip { on: false });
    step(&mut sn, &mut client, &mut server, &mut crt, 5);
    assert_eq!(
        server.players[&1].state.move_type,
        gm_move::MoveType::Noclip
    );
    assert!(client.events.iter().any(|e| e == "Noclip denied"));
}

#[test]
fn a_client_actor_draws_its_hud_through_render_hud() {
    let mut crt = gm_script::Runtime::client();
    crt.load(gm_script::ActorSpec {
        addon: "test".into(),
        name: "hud".into(),
        priority: 1,
        source: "draw.font('big', { family = 'Roboto Bold', size = 24 })
            hud.register('bar', function(s)
                draw.rect(0, s.h - 20, s.w, 20, { r = 255, g = 0, b = 0, a = 128 })
                draw.text(8, s.h - 18, 'tick ' .. game.tick())
                local w, h = draw.text_size('tick', 'big')
                draw.text(8, 8, 'w ' .. w .. ' h ' .. h, nil, 'big')
                draw.texture(0, 0, 64, 64, 'vgui/hud/icon_locked')
                draw.start3d2d({ x = 100, y = 0, z = 10 }, { x = 0, y = 90, z = 0 }, 0.5)
                draw.text(0, 0, 'on the wall')
                draw.end3d2d()
            end)
            hud.register('second', function(s) draw.rect(1, 2, 3, 4) end)"
            .into(),
        config: relaxed(),
    })
    .unwrap();
    let world = gm_net::ClientWorld {
        tick: 42,
        ..Default::default()
    };
    // the client reports what it rasterised; here a stand-in: 10 px per glyph, 24 px lines
    gm_script::set_font_metrics(
        "big",
        gm_script::TextMetrics {
            line_height: 24.0,
            advances: [('t', 10.0), ('i', 10.0), ('c', 10.0), ('k', 10.0)].into(),
            fallback: 7.0,
        },
    );
    let draws = crt.client_render(&world, 800, 600);
    assert_eq!(
        draws,
        vec![
            gm_script::DrawCmd::Font {
                name: "big".into(),
                family: "Roboto Bold".into(),
                size: 24.0,
            },
            gm_script::DrawCmd::Rect {
                x: 0.0,
                y: 580.0,
                w: 800.0,
                h: 20.0,
                color: [1.0, 0.0, 0.0, 128.0 / 255.0],
            },
            gm_script::DrawCmd::Text {
                x: 8.0,
                y: 582.0,
                text: "tick 42".into(),
                color: [1.0, 1.0, 1.0, 1.0],
                font: None,
            },
            gm_script::DrawCmd::Text {
                x: 8.0,
                y: 8.0,
                text: "w 40 h 24".into(),
                color: [1.0, 1.0, 1.0, 1.0],
                font: Some("big".into()),
            },
            gm_script::DrawCmd::Image {
                x: 0.0,
                y: 0.0,
                w: 64.0,
                h: 64.0,
                material: "vgui/hud/icon_locked".into(),
                color: [1.0, 1.0, 1.0, 1.0],
            },
            gm_script::DrawCmd::Begin3D2D {
                pos: glam::Vec3::new(100.0, 0.0, 10.0),
                angles: glam::Vec3::new(0.0, 90.0, 0.0),
                scale: 0.5,
            },
            gm_script::DrawCmd::Text {
                x: 0.0,
                y: 0.0,
                text: "on the wall".into(),
                color: [1.0, 1.0, 1.0, 1.0],
                font: None,
            },
            gm_script::DrawCmd::End3D2D,
            gm_script::DrawCmd::Rect {
                x: 1.0,
                y: 2.0,
                w: 3.0,
                h: 4.0,
                color: [1.0, 1.0, 1.0, 1.0],
            },
        ]
    );
    // the font declaration ran once at load; the frame's draws come anew
    assert!(
        crt.client_render(&world, 800, 600).len() == 8,
        "every frame anew"
    );
}

#[test]
fn a_client_actor_traces_the_clients_world_and_the_props_it_knows() {
    let Some(inst) = gm_content::find_install() else {
        eprintln!("skipped: no Garry's Mod install");
        return;
    };
    let mount = gm_content::Mount::for_install(&inst).unwrap();
    let world = gm_dedicated::load_world(&mount, "gm_construct").unwrap();
    let mut crt = gm_script::Runtime::client();
    let hulls = gm_content::Mount::for_install(&inst).unwrap();
    crt.set_client_world(
        &world.collision,
        Box::new(move |m: &str| {
            gm_dedicated::model_info(&hulls, m)
                .ok()
                .map(|i| i.hull_planes)
        }),
    );
    crt.load(gm_script::ActorSpec {
        addon: "test".into(),
        name: "probe".into(),
        priority: 1,
        source: "events.on('tick.client', function()
                local floor = phys.trace({ x = 823.235, y = -500, z = -100 }, { x = 823.235, y = -500, z = -300 })
                state.floor_hit, state.floor_z, state.floor_ent = floor.hit, floor.pos.z, floor.ent
                local crate = phys.trace({ x = 900, y = -500, z = -100 }, { x = 900, y = -500, z = -300 })
                state.crate_hit, state.crate_ent, state.crate_z = crate.hit, crate.ent, crate.pos.z
                local air = phys.trace({ x = 823.235, y = -500, z = -100 }, { x = 823.235, y = -500, z = -130 })
                state.air = air.hit
            end)"
            .into(),
        config: relaxed(),
    })
    .unwrap();
    let mut cw = gm_net::ClientWorld::default();
    cw.entities.insert(
        7,
        gm_net::msg::EntityState {
            id: 7,
            model: "models/props_junk/wood_crate001a.mdl".into(),
            pos: glam::Vec3::new(900.0, -500.0, -120.0),
            ..Default::default()
        },
    );
    crt.client_tick(&cw, true);
    let (floor_hit, floor_z, floor_ent): (bool, f32, u32) = crt.actors()[0]
        .eval("return state.floor_hit, state.floor_z, state.floor_ent")
        .unwrap();
    assert!(floor_hit && floor_ent == 0);
    assert!((floor_z - -143.969).abs() < 0.01, "floor {floor_z}");
    let (crate_hit, crate_ent, crate_z): (bool, u32, f32) = crt.actors()[0]
        .eval("return state.crate_hit, state.crate_ent, state.crate_z")
        .unwrap();
    assert!(
        crate_hit && crate_ent == 7,
        "the crate's PHY hull: {crate_ent}"
    );
    assert!(crate_z > -120.0 && crate_z < -90.0, "crate top {crate_z}");
    let air: bool = crt.actors()[0].eval("return state.air").unwrap();
    assert!(!air, "a trace ending above the floor misses");
}

#[test]
fn a_scripted_stool_reads_the_clients_tool_settings_from_the_click() {
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
    rt.load(gm_script::ActorSpec {
        addon: "test".into(),
        name: "tools".into(),
        priority: 1,
        source: "tool.register('weldworld_lua', {
                left = function(ctx)
                    local f = tonumber(ctx.settings and ctx.settings.forcelimit) or 10
                    if ctx.hit.ent == 0 then return {} end
                    return { { op = 'weld', ent1 = ctx.hit.ent, ent2 = 0, forcelimit = f } }
                end,
            })"
        .into(),
        config: relaxed(),
    })
    .unwrap();
    let mut sn = gm_net::ServerNet::new("gm_construct", glam::Vec3::new(823.235, -500.0, -143.0));
    gm_script::attach(rt, &mut server, &mut sn.hooks);
    let net = gm_net::Loopback::new(1, 0.0, 7);
    let (mut s_ep, mut c_ep) = (net.endpoint(0), net.endpoint(1));
    let mut client = gm_net::ClientNet::new(1, "bot");
    let mut step = |sn: &mut gm_net::ServerNet,
                    client: &mut gm_net::ClientNet,
                    server: &mut gm_server::Server<'_>,
                    n: usize| {
        for _ in 0..n {
            client.tick(
                gm_net::NetCmd {
                    view_angles: glam::Vec3::new(30.0, 0.0, 0.0),
                    weapon: 2,
                    ..Default::default()
                },
                &mut c_ep,
            );
            net.tick();
            sn.receive(server, &mut s_ep);
            sn.tick(server);
            sn.send(server, &mut s_ep);
            client.receive(&mut c_ep);
        }
    };
    step(&mut sn, &mut client, &mut server, 10);
    client.command(gm_net::Command::Spawn {
        model: "models/props_junk/wood_crate001a.mdl".into(),
    });
    step(&mut sn, &mut client, &mut server, 20);
    let click = |settings: Vec<(String, String)>| gm_net::Command::ToolClick {
        tool: "weldworld_lua".into(),
        right: false,
        reload: false,
        settings,
    };
    client.command(click(vec![("forcelimit".into(), "64".into())]));
    step(&mut sn, &mut client, &mut server, 2);
    let limits = |server: &gm_server::Server<'_>| -> Vec<f32> {
        server
            .sandbox
            .constraints()
            .iter()
            .map(|c| format!("{c:?}"))
            .map(|d| {
                d.split("forcelimit: ")
                    .nth(1)
                    .and_then(|r| {
                        r.split(|ch: char| !(ch.is_ascii_digit() || ch == '.'))
                            .next()
                    })
                    .and_then(|n| n.parse().ok())
                    .unwrap_or(-1.0)
            })
            .collect()
    };
    assert_eq!(
        limits(&server),
        vec![64.0],
        "the client's setting reached the stool"
    );
    client.command(gm_net::Command::Undo);
    step(&mut sn, &mut client, &mut server, 2);
    client.command(click(Vec::new()));
    step(&mut sn, &mut client, &mut server, 2);
    assert_eq!(
        limits(&server),
        vec![10.0],
        "the stool's default without a setting"
    );
}

#[test]
fn an_actors_spawn_menu_categories_reach_the_client_whole_and_merge() {
    let Some(inst) = gm_content::find_install() else {
        eprintln!("skipped: no Garry's Mod install");
        return;
    };
    let mount = gm_content::Mount::for_install(&inst).unwrap();
    let world = gm_dedicated::load_world(&mount, "gm_construct").unwrap();
    let mut server = gm_server::Server::new(&world.collision, world.physics, world.world_body);
    let mut rt = gm_script::Runtime::new();
    rt.load(gm_script::ActorSpec {
        addon: "test".into(),
        name: "menu".into(),
        priority: 1,
        source: "spawnmenu.add('Crates', { 'models/props_junk/wood_crate001a.mdl', 'models/props_junk/wood_crate002a.mdl' })
            spawnmenu.add('Barrels', { 'models/props_c17/oildrum001.mdl' })
            events.on('tick.post', function()
                if game.tick() == 20 then
                    spawnmenu.add('Crates', { 'models/props_junk/wood_crate001a.mdl', 'models/props_junk/wood_crate001a_damaged.mdl' })
                end
            end)"
            .into(),
        config: relaxed(),
    })
    .unwrap();
    let mut sn = gm_net::ServerNet::new("gm_construct", glam::Vec3::new(823.235, -500.0, -143.0));
    gm_script::attach(rt, &mut server, &mut sn.hooks);
    let net = gm_net::Loopback::new(1, 0.0, 7);
    let (mut s_ep, mut c_ep) = (net.endpoint(0), net.endpoint(1));
    let mut client = gm_net::ClientNet::new(1, "bot");
    let mut step = |sn: &mut gm_net::ServerNet,
                    client: &mut gm_net::ClientNet,
                    server: &mut gm_server::Server<'_>,
                    n: usize| {
        for _ in 0..n {
            client.tick(gm_net::NetCmd::default(), &mut c_ep);
            net.tick();
            sn.receive(server, &mut s_ep);
            sn.tick(server);
            sn.send(server, &mut s_ep);
            client.receive(&mut c_ep);
        }
    };
    step(&mut sn, &mut client, &mut server, 8);
    assert_eq!(
        client.world.spawn_lists,
        vec![
            (
                "Crates".to_string(),
                vec![
                    "models/props_junk/wood_crate001a.mdl".to_string(),
                    "models/props_junk/wood_crate002a.mdl".to_string(),
                ],
            ),
            (
                "Barrels".to_string(),
                vec!["models/props_c17/oildrum001.mdl".to_string()],
            ),
        ],
        "the lists registered at load, in order"
    );
    assert_eq!(server.spawn_lists_version, 2);
    step(&mut sn, &mut client, &mut server, 20);
    assert_eq!(
        server.spawn_lists_version, 3,
        "the merge at tick 20 is one change"
    );
    assert_eq!(
        client.world.spawn_lists[0].1,
        vec![
            "models/props_junk/wood_crate001a.mdl".to_string(),
            "models/props_junk/wood_crate002a.mdl".to_string(),
            "models/props_junk/wood_crate001a_damaged.mdl".to_string(),
        ],
        "a repeated model is dropped, the new one appended"
    );
}

#[test]
fn player_use_is_asked_on_the_press_within_reach_and_player_used_follows() {
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
    rt.load(gm_script::ActorSpec {
        addon: "test".into(),
        name: "use".into(),
        priority: 1,
        source: "state.asked, state.used, state.last = 0, 0, 0
            events.on('player.use', function(e)
                state.asked += 1
                if state.asked == 1 then return false end
            end)
            events.on('player.used', function(e)
                state.used += 1
                state.last = e.entity
            end)
            events.on('tick.post', function()
                if game.tick() % 10 == 0 then
                    for _, pl in ipairs(world.players()) do
                        net.send('log', { asked = state.asked, used = state.used, last = state.last }, pl.id)
                    end
                end
            end)"
            .into(),
        config: relaxed(),
    })
    .unwrap();
    let mut sn = gm_net::ServerNet::new("gm_construct", glam::Vec3::new(823.235, -500.0, -143.0));
    gm_script::attach(rt, &mut server, &mut sn.hooks);
    let net = gm_net::Loopback::new(1, 0.0, 7);
    let (mut s_ep, mut c_ep) = (net.endpoint(0), net.endpoint(1));
    let mut client = gm_net::ClientNet::new(1, "bot");
    let mut crt = gm_script::Runtime::client();
    crt.load(gm_script::ActorSpec {
        addon: "test".into(),
        name: "probe".into(),
        priority: 1,
        source: "net.receive('log', function(p) state.d = p.data end)".into(),
        config: relaxed(),
    })
    .unwrap();
    let mut step = |sn: &mut gm_net::ServerNet,
                    client: &mut gm_net::ClientNet,
                    server: &mut gm_server::Server<'_>,
                    crt: &mut gm_script::Runtime,
                    pitch: f32,
                    buttons: u32,
                    n: usize| {
        for _ in 0..n {
            client.tick(
                gm_net::NetCmd {
                    view_angles: glam::Vec3::new(pitch, 0.0, 0.0),
                    buttons,
                    ..Default::default()
                },
                &mut c_ep,
            );
            net.tick();
            sn.receive(server, &mut s_ep);
            sn.tick(server);
            sn.send(server, &mut s_ep);
            client.receive(&mut c_ep);
            crt.client_tick(&client.world, true);
            let inbox = std::mem::take(&mut client.addon_msgs);
            crt.client_net(&client.world, inbox);
        }
    };
    step(&mut sn, &mut client, &mut server, &mut crt, 30.0, 0, 30);
    // spawned where the 30 degree look hits the floor: ~128 u from the eyes, out of reach
    client.command(gm_net::Command::Spawn {
        model: "models/props_junk/wood_crate001a.mdl".into(),
    });
    step(&mut sn, &mut client, &mut server, &mut crt, 30.0, 0, 30);
    let far = server.sandbox.props()[0].id;
    step(
        &mut sn,
        &mut client,
        &mut server,
        &mut crt,
        30.0,
        gm_server::IN_USE,
        5,
    );
    step(&mut sn, &mut client, &mut server, &mut crt, 30.0, 0, 15);
    let asked: i64 = crt.actors()[0].eval("return state.d.asked").unwrap();
    assert_eq!(asked, 0, "the crate 128 u away is beyond PLAYER_USE_RADIUS");
    // a second crate right under the eyes (60 degrees down: ~40 u); held 5 ticks = one press
    // and five asks (the game asks `PlayerUse` every tick the button is held), the first
    // answer vetoes the press, so nothing is picked up
    client.command(gm_net::Command::Spawn {
        model: "models/props_junk/wood_crate001a.mdl".into(),
    });
    step(&mut sn, &mut client, &mut server, &mut crt, 60.0, 0, 30);
    let near = server.sandbox.props()[1].id;
    assert_ne!(near, far);
    step(
        &mut sn,
        &mut client,
        &mut server,
        &mut crt,
        60.0,
        gm_server::IN_USE,
        5,
    );
    // (the probe logs every 10 ticks, so wait for the next log)
    step(&mut sn, &mut client, &mut server, &mut crt, 60.0, 0, 15);
    let (asked, used): (i64, i64) = crt.actors()[0]
        .eval("return state.d.asked, state.d.used")
        .unwrap();
    assert_eq!(
        (asked, used),
        (5, 4),
        "one ask per held tick, the press itself vetoed"
    );
    assert!(
        server.players.values().all(|p| p.carry.is_none()),
        "a vetoed press picks nothing up, the held ticks after it do not either"
    );
    // the second press goes through and picks the crate up; the held ticks keep asking
    step(
        &mut sn,
        &mut client,
        &mut server,
        &mut crt,
        60.0,
        gm_server::IN_USE,
        5,
    );
    step(&mut sn, &mut client, &mut server, &mut crt, 60.0, 0, 25);
    let (asked, used, last): (i64, i64, i64) = crt.actors()[0]
        .eval("return state.d.asked, state.d.used, state.d.last")
        .unwrap();
    assert_eq!((asked, used), (10, 9), "five more asks, all allowed");
    assert_eq!(last as u32, near, "the crate under the eyes");
    assert_eq!(
        server
            .players
            .values()
            .filter(|p| p.carry.is_some())
            .count(),
        1,
        "the press picked the crate up once, the held ticks did not drop or re-pick it"
    );
}

#[test]
fn a_stools_setting_schema_reaches_the_client_and_shapes_the_clicks_settings() {
    let Some(inst) = gm_content::find_install() else {
        eprintln!("skipped: no Garry's Mod install");
        return;
    };
    let mount = gm_content::Mount::for_install(&inst).unwrap();
    let world = gm_dedicated::load_world(&mount, "gm_construct").unwrap();
    let mut server = gm_server::Server::new(&world.collision, world.physics, world.world_body);
    let mut rt = gm_script::Runtime::new();
    rt.load(gm_script::ActorSpec {
        addon: "test".into(),
        name: "schema".into(),
        priority: 1,
        source: "tool.register('weld_lua', {
                settings = {
                    { name = 'forcelimit', type = 'number', min = 0, max = 1000, default = 10 },
                    { name = 'nocollide', type = 'bool', default = false },
                    { name = 'label', type = 'text', default = 'weld' },
                },
                left = function(ctx) return {} end,
            })"
        .into(),
        config: relaxed(),
    })
    .unwrap();
    let mut sn = gm_net::ServerNet::new("gm_construct", glam::Vec3::new(823.235, -500.0, -143.0));
    gm_script::attach(rt, &mut server, &mut sn.hooks);
    let net = gm_net::Loopback::new(1, 0.0, 7);
    let (mut s_ep, mut c_ep) = (net.endpoint(0), net.endpoint(1));
    let mut client = gm_net::ClientNet::new(1, "bot");
    for _ in 0..8 {
        client.tick(gm_net::NetCmd::default(), &mut c_ep);
        net.tick();
        sn.receive(&mut server, &mut s_ep);
        sn.tick(&mut server);
        sn.send(&server, &mut s_ep);
        client.receive(&mut c_ep);
    }
    use gm_server::{ToolSetting, ToolSettingKind as K};
    let expected = vec![
        ToolSetting {
            name: "forcelimit".into(),
            kind: K::Number,
            min: 0.0,
            max: 1000.0,
            default: "10".into(),
        },
        ToolSetting {
            name: "nocollide".into(),
            kind: K::Bool,
            min: f32::MIN,
            max: f32::MAX,
            default: "0".into(),
        },
        ToolSetting {
            name: "label".into(),
            kind: K::Text,
            min: f32::MIN,
            max: f32::MAX,
            default: "weld".into(),
        },
    ];
    assert_eq!(server.tool_schemas_version, 1);
    assert_eq!(
        client.world.tool_schemas,
        vec![("weld_lua".to_string(), expected.clone())]
    );
    let schema = client.world.tool_schemas[0].1.as_slice();
    let mut set = std::collections::BTreeMap::new();
    assert_eq!(
        gm_net::tool_click_settings(Some(schema), &set),
        vec![
            ("forcelimit".to_string(), "10".to_string()),
            ("nocollide".to_string(), "0".to_string()),
            ("label".to_string(), "weld".to_string()),
        ],
        "defaults when the player set nothing"
    );
    set.insert("forcelimit".into(), "5000".into());
    set.insert("nocollide".into(), "true".into());
    set.insert("extra".into(), "x".into());
    assert_eq!(
        gm_net::tool_click_settings(Some(schema), &set),
        vec![
            ("forcelimit".to_string(), "1000".to_string()),
            ("nocollide".to_string(), "1".to_string()),
            ("label".to_string(), "weld".to_string()),
            ("extra".to_string(), "x".to_string()),
        ],
        "clamped to the range, bool normalised, unknown keys pass through"
    );
    set.insert("forcelimit".into(), "lots".into());
    assert_eq!(
        gm_net::tool_click_settings(Some(schema), &set)[0].1,
        "10",
        "an unparsable number falls back to the default"
    );
}

#[test]
fn a_stools_ghost_is_drawn_at_the_clients_trace_hit() {
    let Some(inst) = gm_content::find_install() else {
        eprintln!("skipped: no Garry's Mod install");
        return;
    };
    let mount = gm_content::Mount::for_install(&inst).unwrap();
    let world = gm_dedicated::load_world(&mount, "gm_construct").unwrap();
    let m2 = gm_content::Mount::for_install(&inst).unwrap();
    let mut crt = gm_script::Runtime::client();
    crt.set_client_world(
        &world.collision,
        Box::new(move |m: &str| gm_dedicated::model_info(&m2, m).ok().map(|i| i.hull_planes)),
    );
    crt.load(gm_script::ActorSpec {
        addon: "test".into(),
        name: "ghost".into(),
        priority: 1,
        source: "tool.register('crate_lua', {
                ghost = function(ctx)
                    if ctx.hit.ent ~= 0 then return nil end
                    local rise = tonumber(ctx.settings.rise) or 0
                    return {
                        model = 'models/props_junk/wood_crate001a.mdl',
                        pos = { x = ctx.hit.pos.x, y = ctx.hit.pos.y, z = ctx.hit.pos.z + rise },
                        angles = { x = 0, y = 90, z = 0 },
                        alpha = 102,
                    }
                end,
            })"
        .into(),
        config: relaxed(),
    })
    .unwrap();
    let cw = gm_net::ClientWorld::default();
    let eye = glam::Vec3::new(823.235, -500.0, -143.0 + 64.0);
    let down = glam::Vec3::NEG_Z;
    let settings = vec![("rise".to_string(), "12".to_string())];
    let draws = crt.client_ghost(&cw, "crate_lua", &settings, eye, down);
    assert_eq!(draws.len(), 1, "one ghost for the floor hit: {draws:?}");
    let gm_script::DrawCmd::Model {
        model,
        pos,
        angles,
        alpha,
    } = &draws[0]
    else {
        panic!("{draws:?}");
    };
    assert_eq!(model, "models/props_junk/wood_crate001a.mdl");
    assert!(
        (pos.z - (-143.969 + 12.0)).abs() < 0.01,
        "floor plus rise: {pos}"
    );
    assert!((pos.x - eye.x).abs() < 0.01 && (pos.y - eye.y).abs() < 0.01);
    assert_eq!(*angles, glam::Vec3::new(0.0, 90.0, 0.0));
    assert!((alpha - 0.4).abs() < 0.001, "102/255: {alpha}");
    assert!(
        crt.client_ghost(&cw, "crate_lua", &[], eye, glam::Vec3::Z)
            .is_empty(),
        "nothing above the spawn within reach"
    );
    assert!(
        crt.client_ghost(&cw, "weld", &[], eye, down).is_empty(),
        "a tool without a ghost draws nothing"
    );
}

#[test]
fn sandbox_gamemode_hooks_loadout_undo_unfreeze_and_fall_damage_are_asked() {
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
    rt.load(gm_script::ActorSpec {
        addon: "test".into(),
        name: "gamemode".into(),
        priority: 1,
        source: "state.loadouts, state.undos, state.unfreezes, state.unfroze = 0, 0, 0, 0
            state.falls = {}
            events.on('player.loadout', function(e)
                state.loadouts += 1
                if state.loadouts == 2 then
                    world.queue('player.give_weapon', e.player, 'weapon_pistol')
                    return false
                end
            end)
            events.on('player.undo_request', function(e)
                state.undos += 1
                state.undo_name = e.name
                if state.undos == 1 then return false end
            end)
            events.on('player.unfreeze_request', function(e)
                state.unfreezes += 1
                if state.unfreezes == 1 then return false end
            end)
            events.on('player.unfroze', function(e) state.unfroze = e.count end)
            events.on('player.fall_damage', function(e)
                table.insert(state.falls, { speed = e.speed, amount = e.amount })
                if #state.falls == 1 then return false end
                return e.amount * 2
            end)
            events.on('tick.post', function()
                if game.tick() % 10 == 0 then
                    for _, pl in ipairs(world.players()) do
                        net.send('log', { loadouts = state.loadouts, undos = state.undos, undo_name = state.undo_name,
                            unfreezes = state.unfreezes, unfroze = state.unfroze, falls = state.falls }, pl.id)
                    end
                end
            end)"
            .into(),
        config: relaxed(),
    })
    .unwrap();
    let mut sn = gm_net::ServerNet::new("gm_construct", glam::Vec3::new(823.235, -500.0, -143.0));
    gm_script::attach(rt, &mut server, &mut sn.hooks);
    let net = gm_net::Loopback::new(1, 0.0, 7);
    let (mut s_ep, mut c_ep) = (net.endpoint(0), net.endpoint(1));
    let mut client = gm_net::ClientNet::new(1, "bot");
    let mut crt = gm_script::Runtime::client();
    crt.load(gm_script::ActorSpec {
        addon: "test".into(),
        name: "probe".into(),
        priority: 1,
        source: "net.receive('log', function(p) state.d = p.data end)".into(),
        config: relaxed(),
    })
    .unwrap();
    let mut step = |sn: &mut gm_net::ServerNet,
                    client: &mut gm_net::ClientNet,
                    server: &mut gm_server::Server<'_>,
                    crt: &mut gm_script::Runtime,
                    buttons: u32,
                    weapon: u8,
                    n: usize| {
        for _ in 0..n {
            client.tick(
                gm_net::NetCmd {
                    view_angles: glam::Vec3::new(30.0, 0.0, 0.0),
                    buttons,
                    weapon,
                    ..Default::default()
                },
                &mut c_ep,
            );
            net.tick();
            sn.receive(server, &mut s_ep);
            sn.tick(server);
            sn.send(server, &mut s_ep);
            client.receive(&mut c_ep);
            crt.client_tick(&client.world, true);
            let inbox = std::mem::take(&mut client.addon_msgs);
            crt.client_net(&client.world, inbox);
        }
    };
    step(&mut sn, &mut client, &mut server, &mut crt, 0, 0, 20);
    // first spawn: the default loadout
    assert_eq!(server.players[&1].weapons.len(), gm_server::LOADOUT.len());

    // undo: the first request is vetoed, the second removes the prop
    client.command(gm_net::Command::Spawn {
        model: "models/props_junk/wood_crate001a.mdl".into(),
    });
    step(&mut sn, &mut client, &mut server, &mut crt, 0, 0, 10);
    assert_eq!(server.sandbox.props().len(), 1);
    client.command(gm_net::Command::Undo);
    step(&mut sn, &mut client, &mut server, &mut crt, 0, 0, 10);
    assert_eq!(
        server.sandbox.props().len(),
        1,
        "vetoed undo keeps the prop"
    );
    client.command(gm_net::Command::Undo);
    step(&mut sn, &mut client, &mut server, &mut crt, 0, 0, 10);
    assert_eq!(
        server.sandbox.props().len(),
        0,
        "the allowed undo removes it"
    );

    // unfreeze: a frozen prop, physgun R; the first press is vetoed, the second unfreezes
    client.command(gm_net::Command::Spawn {
        model: "models/props_junk/wood_crate001a.mdl".into(),
    });
    step(&mut sn, &mut client, &mut server, &mut crt, 0, 0, 10);
    let ent = server.sandbox.props()[0].id;
    assert!(server.set_prop_frozen(ent, true));
    let r = gm_server::weapons::IN_RELOAD;
    step(&mut sn, &mut client, &mut server, &mut crt, r, 1, 3);
    step(&mut sn, &mut client, &mut server, &mut crt, 0, 1, 3);
    assert!(
        server.sandbox.props()[0].frozen,
        "vetoed unfreeze keeps it frozen"
    );
    step(&mut sn, &mut client, &mut server, &mut crt, r, 1, 3);
    step(&mut sn, &mut client, &mut server, &mut crt, 0, 1, 3);
    assert!(
        !server.sandbox.props()[0].frozen,
        "the second press unfreezes"
    );

    // fall damage: the first landing is cancelled, the second doubled
    let hp = server.players[&1].health;
    for _ in 0..2 {
        {
            let p = server.players.get_mut(&1).unwrap();
            p.state.origin.z += 700.0;
            p.state.velocity = glam::Vec3::ZERO;
        }
        step(&mut sn, &mut client, &mut server, &mut crt, 0, 0, 120);
    }
    step(&mut sn, &mut client, &mut server, &mut crt, 0, 0, 10);
    let (loadouts, undos, unfreezes, unfroze): (i64, i64, i64, i64) = crt.actors()[0]
        .eval("return state.d.loadouts, state.d.undos, state.d.unfreezes, state.d.unfroze")
        .unwrap();
    let undo_name: String = crt.actors()[0].eval("return state.d.undo_name").unwrap();
    let speeds: Vec<f64> = crt.actors()[0]
        .eval("local t = {} for _, f in ipairs(state.d.falls) do table.insert(t, f.speed) end return t")
        .unwrap();
    let amounts: Vec<f64> = crt.actors()[0]
        .eval("local t = {} for _, f in ipairs(state.d.falls) do table.insert(t, f.amount) end return t")
        .unwrap();
    let falls: Vec<(f64, f64)> = speeds.into_iter().zip(amounts).collect();
    assert_eq!((loadouts, undos, unfreezes, unfroze), (1, 2, 2, 1));
    assert_eq!(undo_name, "prop_physics");
    assert_eq!(falls.len(), 2, "{falls:?}");
    assert!(falls[0].0 > 526.0 && falls[0].1 > 0.0, "{falls:?}");
    let expected = hp - (falls[1].1 * 2.0).round() as i32;
    assert_eq!(
        server.players[&1].health, expected,
        "first landing cancelled, second doubled: {falls:?}"
    );

    // respawn: the second loadout is refused and replaced by the hook's pistol
    server.damage_player(1, 1000.0, 0);
    step(&mut sn, &mut client, &mut server, &mut crt, 0, 0, 160);
    assert!(server.players[&1].alive);
    assert_eq!(
        server.players[&1].weapons,
        vec!["weapon_pistol".to_string()]
    );
    let loadouts: i64 = crt.actors()[0].eval("return state.d.loadouts").unwrap();
    assert_eq!(loadouts, 2);
}

#[test]
fn sandbox_gamemode_package_applies_godmode_bare_loadout_and_prop_limit() {
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
    let root =
        std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../addons/gamemodes/sandbox");
    let m = rt.load_addon(&root).unwrap();
    assert_eq!(m.id, "sandbox");
    assert_eq!(rt.actors().len(), 2);
    let mut sn = gm_net::ServerNet::new("gm_construct", glam::Vec3::new(823.235, -500.0, -143.0));
    gm_script::attach(rt, &mut server, &mut sn.hooks);
    let net = gm_net::Loopback::new(1, 0.0, 7);
    let (mut s_ep, mut c_ep) = (net.endpoint(0), net.endpoint(1));
    let mut client = gm_net::ClientNet::new(1, "bot");
    let mut step = |sn: &mut gm_net::ServerNet,
                    client: &mut gm_net::ClientNet,
                    server: &mut gm_server::Server<'_>,
                    n: usize| {
        for _ in 0..n {
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
        }
    };
    step(&mut sn, &mut client, &mut server, 20);
    assert_eq!(server.players[&1].weapons.len(), gm_server::LOADOUT.len());

    // godmode: no damage while on, damage again once off
    assert!(server.set_cvar("sbox_godmode", "1"));
    step(&mut sn, &mut client, &mut server, 2);
    assert_eq!(server.damage_player(1, 30.0, 0), 0.0);
    assert_eq!(server.players[&1].health, 100);
    assert!(server.set_cvar("sbox_godmode", "0"));
    step(&mut sn, &mut client, &mut server, 2);
    assert_eq!(server.damage_player(1, 30.0, 0), 30.0);
    assert_eq!(server.players[&1].health, 70);

    // sbox_maxprops 1: the second spawn is refused by the package
    assert!(server.set_cvar("sbox_maxprops", "1"));
    for _ in 0..2 {
        client.command(gm_net::Command::Spawn {
            model: "models/props_junk/wood_crate001a.mdl".into(),
        });
        step(&mut sn, &mut client, &mut server, 5);
    }
    assert_eq!(server.sandbox.props().len(), 1);

    // sbox_weapons 0: the respawn holds camera, tool gun and physgun, physgun out
    assert!(server.set_cvar("sbox_weapons", "0"));
    server.damage_player(1, 1000.0, 0);
    step(&mut sn, &mut client, &mut server, 160);
    assert!(server.players[&1].alive);
    assert_eq!(
        server.players[&1].weapons,
        vec![
            "gmod_camera".to_string(),
            "gmod_tool".to_string(),
            "weapon_physgun".to_string()
        ]
    );
    assert_eq!(server.players[&1].active_weapon, "weapon_physgun");
}

#[test]
fn use_pickup_carries_a_light_prop_to_the_hold_point_and_the_second_press_drops_it() {
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
    rt.load(gm_script::ActorSpec {
        addon: "test".into(),
        name: "pickup".into(),
        priority: 1,
        source: "state.asked, state.picked, state.dropped, state.used = 0, 0, 0, 0
            events.on('player.pickup_request', function(e)
                state.asked += 1
                if state.asked == 1 then return false end
            end)
            events.on('player.used', function() state.used += 1 end)
            events.on('player.pickup', function(e) state.picked += 1 state.ent = e.entity end)
            events.on('player.drop', function(e) state.dropped += 1 end)
            events.on('tick.post', function()
                if game.tick() % 5 == 0 then
                    for _, pl in ipairs(world.players()) do
                        net.send('log', { asked = state.asked, picked = state.picked,
                            dropped = state.dropped, used = state.used, ent = state.ent or 0 }, pl.id)
                    end
                end
            end)"
            .into(),
        config: relaxed(),
    })
    .unwrap();
    let mut sn = gm_net::ServerNet::new("gm_construct", glam::Vec3::new(823.235, -600.0, -143.6));
    gm_script::attach(rt, &mut server, &mut sn.hooks);
    let net = gm_net::Loopback::new(1, 0.0, 7);
    let (mut s_ep, mut c_ep) = (net.endpoint(0), net.endpoint(1));
    let mut client = gm_net::ClientNet::new(1, "bot");
    let mut crt = gm_script::Runtime::client();
    crt.load(gm_script::ActorSpec {
        addon: "test".into(),
        name: "probe".into(),
        priority: 1,
        source: "net.receive('log', function(p) state.d = p.data end)".into(),
        config: relaxed(),
    })
    .unwrap();
    let mut step = |sn: &mut gm_net::ServerNet,
                    client: &mut gm_net::ClientNet,
                    server: &mut gm_server::Server<'_>,
                    crt: &mut gm_script::Runtime,
                    pitch: f32,
                    buttons: u32,
                    n: usize| {
        for _ in 0..n {
            client.tick(
                gm_net::NetCmd {
                    view_angles: glam::Vec3::new(pitch, 0.0, 0.0),
                    buttons,
                    ..Default::default()
                },
                &mut c_ep,
            );
            net.tick();
            sn.receive(server, &mut s_ep);
            sn.tick(server);
            sn.send(server, &mut s_ep);
            client.receive(&mut c_ep);
            crt.client_tick(&client.world, true);
            let inbox = std::mem::take(&mut client.addon_msgs);
            crt.client_net(&client.world, inbox);
        }
    };
    let eye = |server: &gm_server::Server<'_>| {
        let p = &server.players[&1];
        p.state.origin + glam::Vec3::new(0.0, 0.0, 64.0)
    };
    // a crate where the 35 degree look hits the floor: its face ~70 u ahead, in reach
    step(&mut sn, &mut client, &mut server, &mut crt, 35.0, 0, 30);
    client.command(gm_net::Command::Spawn {
        model: "models/props_junk/wood_crate001a.mdl".into(),
    });
    step(&mut sn, &mut client, &mut server, &mut crt, 35.0, 0, 40);
    let crate_id = server.sandbox.props()[0].id;
    let rest = server.prop_state(crate_id).unwrap().position;
    // first press (held 3 ticks: three `player.used`): pickup vetoed by the hook, the crate stays
    step(
        &mut sn,
        &mut client,
        &mut server,
        &mut crt,
        30.0,
        gm_server::IN_USE,
        3,
    );
    step(&mut sn, &mut client, &mut server, &mut crt, 30.0, 0, 12);
    let (asked, picked, used): (i64, i64, i64) = crt.actors()[0]
        .eval("return state.d.asked, state.d.picked, state.d.used")
        .unwrap();
    assert_eq!(
        (asked, picked, used),
        (1, 0, 3),
        "vetoed pickup after an allowed use, one use per held tick"
    );
    assert!(server.prop_state(crate_id).unwrap().position.distance(rest) < 0.5);
    // second press: carried to eye + forward * (24 + 22.63 + extent) within a few ticks
    step(
        &mut sn,
        &mut client,
        &mut server,
        &mut crt,
        30.0,
        gm_server::IN_USE,
        3,
    );
    step(&mut sn, &mut client, &mut server, &mut crt, 30.0, 0, 5);
    let e = eye(&server);
    let p = server.prop_state(crate_id).unwrap().position - e;
    let (sp, cp) = (30f32.to_radians().sin(), 30f32.to_radians().cos());
    let want = glam::Vec3::new(cp, 0.0, -sp) * (24.0 + 22.63 + 19.8 * (cp + sp));
    assert!(p.distance(want) < 2.0, "carried {p:?} want {want:?}");
    // looking level: 66.4 u straight ahead at eye height, still carried
    step(&mut sn, &mut client, &mut server, &mut crt, 0.0, 0, 20);
    let e = eye(&server);
    let p = server.prop_state(crate_id).unwrap().position - e;
    assert!(
        p.distance(glam::Vec3::new(66.4, 0.0, 0.0)) < 2.0,
        "level {p:?}"
    );
    let (picked, dropped, ent): (i64, i64, i64) = crt.actors()[0]
        .eval("return state.d.picked, state.d.dropped, state.d.ent")
        .unwrap();
    assert_eq!((picked, dropped, ent as u32), (1, 0, crate_id));
    // third press drops it: it falls to the floor under the hold point
    step(
        &mut sn,
        &mut client,
        &mut server,
        &mut crt,
        0.0,
        gm_server::IN_USE,
        3,
    );
    step(&mut sn, &mut client, &mut server, &mut crt, 0.0, 0, 60);
    let p = server.prop_state(crate_id).unwrap().position;
    assert!(
        (p.z - rest.z).abs() < 1.0 && (p.x - e.x - 66.4).abs() < 3.0,
        "dropped {p:?}"
    );
    let dropped: i64 = crt.actors()[0].eval("return state.d.dropped").unwrap();
    assert_eq!(dropped, 1);
    // a 60 u crate is used but too heavy to carry
    step(&mut sn, &mut client, &mut server, &mut crt, 20.0, 0, 5);
    client.command(gm_net::Command::Spawn {
        model: "models/props_junk/wood_crate002a.mdl".into(),
    });
    step(&mut sn, &mut client, &mut server, &mut crt, 20.0, 0, 40);
    let big = server.sandbox.props()[1].id;
    let rest_big = server.prop_state(big).unwrap().position;
    step(
        &mut sn,
        &mut client,
        &mut server,
        &mut crt,
        20.0,
        gm_server::IN_USE,
        3,
    );
    step(&mut sn, &mut client, &mut server, &mut crt, 20.0, 0, 12);
    let (asked, used, picked): (i64, i64, i64) = crt.actors()[0]
        .eval("return state.d.asked, state.d.used, state.d.picked")
        .unwrap();
    assert_eq!(
        (asked, picked),
        (2, 1),
        "the heavy crate is never asked for pickup"
    );
    assert!(used >= 2, "used {used}");
    assert!(server.prop_state(big).unwrap().position.distance(rest_big) < 0.5);
}

#[test]
fn properties_apply_to_the_prop_under_the_crosshair_after_can_property() {
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
    rt.load(gm_script::ActorSpec {
        addon: "test".into(),
        name: "props".into(),
        priority: 1,
        source: "state.asked, state.applied, state.last = 0, 0, ''
            events.on('player.property_request', function(e)
                state.asked += 1
                if state.asked == 1 then return false end
            end)
                        events.on('player.property', function(e) state.applied += 1 state.last = e.property end)
            events.on('entity.break', function(e) state.broke = e.entity end)
            events.on('tick.post', function()
                if game.tick() % 5 == 0 then
                    for _, pl in ipairs(world.players()) do
                        net.send('log', { asked = state.asked, applied = state.applied, last = state.last, broke = state.broke or 0 }, pl.id)
                    end
                end
            end)"
            .into(),
        config: relaxed(),
    })
    .unwrap();
    let mut sn = gm_net::ServerNet::new("gm_construct", glam::Vec3::new(823.235, -600.0, -143.6));
    gm_script::attach(rt, &mut server, &mut sn.hooks);
    let net = gm_net::Loopback::new(1, 0.0, 7);
    let (mut s_ep, mut c_ep) = (net.endpoint(0), net.endpoint(1));
    let mut client = gm_net::ClientNet::new(1, "bot");
    let mut crt = gm_script::Runtime::client();
    crt.load(gm_script::ActorSpec {
        addon: "test".into(),
        name: "probe".into(),
        priority: 1,
        source: "net.receive('log', function(p) state.d = p.data end)".into(),
        config: relaxed(),
    })
    .unwrap();
    let mut step = |sn: &mut gm_net::ServerNet,
                    client: &mut gm_net::ClientNet,
                    server: &mut gm_server::Server<'_>,
                    crt: &mut gm_script::Runtime,
                    pitch: f32,
                    n: usize| {
        for _ in 0..n {
            client.tick(
                gm_net::NetCmd {
                    view_angles: glam::Vec3::new(pitch, 0.0, 0.0),
                    ..Default::default()
                },
                &mut c_ep,
            );
            net.tick();
            sn.receive(server, &mut s_ep);
            sn.tick(server);
            sn.send(server, &mut s_ep);
            client.receive(&mut c_ep);
            crt.client_tick(&client.world, true);
            let inbox = std::mem::take(&mut client.addon_msgs);
            crt.client_net(&client.world, inbox);
        }
    };
    let prop = |name: &str| gm_net::Command::Property { name: name.into() };
    // a crate where the 30 degree look hits the floor
    step(&mut sn, &mut client, &mut server, &mut crt, 30.0, 30);
    client.command(gm_net::Command::Spawn {
        model: "models/props_junk/wood_crate001a.mdl".into(),
    });
    step(&mut sn, &mut client, &mut server, &mut crt, 30.0, 40);
    let b = server.sandbox.props()[0].id;
    let rest = server.prop_state(b).unwrap().position;
    // CanProperty vetoes the first request: the crate keeps its gravity
    client.command(prop("gravity"));
    step(&mut sn, &mut client, &mut server, &mut crt, 30.0, 10);
    let (asked, applied): (i64, i64) = crt.actors()[0]
        .eval("return state.d.asked, state.d.applied")
        .unwrap();
    assert_eq!((asked, applied), (1, 0));
    assert!(server.sandbox.props()[0].gravity);
    // gravity off: a 50 u/s push upward keeps going instead of falling back
    client.command(prop("gravity"));
    step(&mut sn, &mut client, &mut server, &mut crt, 30.0, 5);
    assert!(
        !server.sandbox.props()[0].gravity,
        "gravity property applied"
    );
    let body = server.body_of(b).unwrap();
    server
        .physics
        .set_velocity(body, glam::Vec3::new(0.0, 0.0, 50.0), glam::Vec3::ZERO);
    step(&mut sn, &mut client, &mut server, &mut crt, 30.0, 30);
    let st = server.prop_state(b).unwrap();
    assert!(
        st.position.z - rest.z > 18.0 && st.velocity.z > 40.0,
        "weightless crate rose {} at {} u/s",
        st.position.z - rest.z,
        st.velocity.z
    );
    // the crate now floats ~25 u up: look at it there (pitch 10) to switch gravity back on
    step(&mut sn, &mut client, &mut server, &mut crt, 10.0, 2);
    client.command(prop("gravity"));
    step(&mut sn, &mut client, &mut server, &mut crt, 10.0, 5);
    assert!(server.sandbox.props()[0].gravity, "gravity back on");
    step(&mut sn, &mut client, &mut server, &mut crt, 30.0, 90);
    assert!(
        (server.prop_state(b).unwrap().position.z - rest.z).abs() < 1.0,
        "back on the floor"
    );
    // collision off: a second crate spawned onto it falls through to the floor
    client.command(prop("collision_off"));
    step(&mut sn, &mut client, &mut server, &mut crt, 30.0, 5);
    assert!(!server.sandbox.props()[0].collide);
    // collision_off is not offered twice
    client.command(prop("collision_off"));
    step(&mut sn, &mut client, &mut server, &mut crt, 30.0, 5);
    let applied: i64 = crt.actors()[0].eval("return state.d.applied").unwrap();
    assert_eq!(applied, 3);
    client.command(gm_net::Command::Spawn {
        model: "models/props_junk/wood_crate001a.mdl".into(),
    });
    step(&mut sn, &mut client, &mut server, &mut crt, 30.0, 120);
    let a = server.sandbox.props()[1].id;
    let az = server.prop_state(a).unwrap().position.z;
    assert!(
        (az - rest.z).abs() < 1.5,
        "the crate fell through to {az} (floor rest {})",
        rest.z
    );
    // the two crates now overlap; take the second away so the crosshair finds the first
    assert!(server.remove_prop(a));
    step(&mut sn, &mut client, &mut server, &mut crt, 30.0, 5);
    client.command(prop("collision_on"));
    step(&mut sn, &mut client, &mut server, &mut crt, 30.0, 5);
    assert!(server.sandbox.props()[0].collide);
    // keep upright: a spin about X is undone within a few ticks
    client.command(prop("keepupright"));
    step(&mut sn, &mut client, &mut server, &mut crt, 30.0, 5);
    assert!(server.sandbox.props()[0].upright.is_some());
    server
        .physics
        .set_velocity(body, glam::Vec3::ZERO, glam::Vec3::new(400.0, 0.0, 0.0));
    step(&mut sn, &mut client, &mut server, &mut crt, 30.0, 10);
    let ang = server.prop_state(b).unwrap().angles;
    assert!(
        ang.0.abs() < 3.0 && ang.2.abs() < 3.0,
        "kept upright, angles {ang:?}"
    );
    // ignite: Ignite(360) on a 30 hp crate, the first point of damage 7 ticks after the
    // flame, then one every 13
    client.command(prop("ignite"));
    step(&mut sn, &mut client, &mut server, &mut crt, 30.0, 3);
    let fire = server.sandbox.props()[0].fire.clone().expect("burning");
    assert_eq!(
        fire.until,
        fire.next_think - gm_server::FIRE_FIRST_DAMAGE_TICKS + 24000
    );
    assert_eq!(server.sandbox.props()[0].health, 30);
    let mut waited = 0;
    while server.sandbox.props()[0].health == 30 && waited < 20 {
        step(&mut sn, &mut client, &mut server, &mut crt, 30.0, 1);
        waited += 1;
    }
    assert_eq!(server.sandbox.props()[0].health, 29);
    step(&mut sn, &mut client, &mut server, &mut crt, 30.0, 12);
    assert_eq!(server.sandbox.props()[0].health, 29);
    step(&mut sn, &mut client, &mut server, &mut crt, 30.0, 1);
    assert_eq!(server.sandbox.props()[0].health, 28);
    // extinguish: the flame is gone at once, the health stays
    client.command(prop("extinguish"));
    step(&mut sn, &mut client, &mut server, &mut crt, 30.0, 5);
    assert!(server.sandbox.props()[0].fire.is_none());
    step(&mut sn, &mut client, &mut server, &mut crt, 30.0, 30);
    assert_eq!(server.sandbox.props()[0].health, 28);
    // remove: the crate is gone
    client.command(prop("remove"));
    step(&mut sn, &mut client, &mut server, &mut crt, 30.0, 10);
    assert!(server.sandbox.props().is_empty());
    // a crate left burning breaks at zero health: 30 points, gone, `entity.break`
    client.command(gm_net::Command::Spawn {
        model: "models/props_junk/wood_crate001a.mdl".into(),
    });
    step(&mut sn, &mut client, &mut server, &mut crt, 30.0, 10);
    let c = server.sandbox.props()[0].id;
    client.command(prop("ignite"));
    step(&mut sn, &mut client, &mut server, &mut crt, 30.0, 5);
    assert!(server.sandbox.props()[0].fire.is_some());
    step(&mut sn, &mut client, &mut server, &mut crt, 30.0, 400);
    assert!(server.sandbox.props().is_empty(), "burnt down");
    let (asked, applied, last, broke): (i64, i64, String, i64) = crt.actors()[0]
        .eval("return state.d.asked, state.d.applied, state.d.last, state.d.broke")
        .unwrap();
    assert_eq!(
        (asked, applied, last.as_str(), broke),
        (10, 9, "ignite", c as i64)
    );
    // `skin`: the crate has two skin families (the game offers the property to it, measured
    // in `lua_properties`), so `skin 1` is asked and applied, an index past the count is not
    // offered (not even asked), and the snapshot carries the family
    client.command(gm_net::Command::Spawn {
        model: "models/props_junk/wood_crate001a.mdl".into(),
    });
    step(&mut sn, &mut client, &mut server, &mut crt, 30.0, 10);
    client.command(prop("skin 2"));
    step(&mut sn, &mut client, &mut server, &mut crt, 30.0, 10);
    assert_eq!(
        server.sandbox.props()[0].skin,
        0,
        "family 2 of 2 does not exist"
    );
    client.command(prop("skin 1"));
    step(&mut sn, &mut client, &mut server, &mut crt, 30.0, 10);
    assert_eq!(server.sandbox.props()[0].skin, 1);
    let snap_skin = client.world.entities.values().next().map(|e| e.skin);
    assert_eq!(snap_skin, Some(1), "the snapshot carries the skin");
    let (asked, applied, last): (i64, i64, String) = crt.actors()[0]
        .eval("return state.d.asked, state.d.applied, state.d.last")
        .unwrap();
    assert_eq!((asked, applied, last.as_str()), (11, 10, "skin"));
    // `bodygroups <part> <model>`: the crate's single body part has one model, so the game's
    // filter (some part with more than one model) refuses it unasked; a model with choices
    // takes a valid pick and refuses a model past the part's count
    client.command(prop("bodygroups 0 1"));
    step(&mut sn, &mut client, &mut server, &mut crt, 30.0, 10);
    assert_eq!(server.sandbox.props()[0].body, "");
    let (asked, applied): (i64, i64) = crt.actors()[0]
        .eval("return state.d.asked, state.d.applied")
        .unwrap();
    assert_eq!((asked, applied), (11, 10), "not offered, not asked");
    // the context menu asks what is offered: the crate's two skins first, then the toggles by
    // state, in the game's order; a toggle's opposite appears once it is applied
    client.command(gm_net::Command::PropertyList);
    step(&mut sn, &mut client, &mut server, &mut crt, 30.0, 10);
    let crate_id = server.sandbox.props()[0].id;
    let names = |c: &gm_net::ClientNet| c.properties.clone().map(|(e, n)| (e, n.join(",")));
    assert_eq!(
        names(&client),
        Some((
            crate_id,
            "skin 0,skin 1,keepupright,ignite,remove,gravity,collision_off".to_string()
        ))
    );
    client.command(prop("collision_off"));
    step(&mut sn, &mut client, &mut server, &mut crt, 30.0, 10);
    client.command(gm_net::Command::PropertyList);
    step(&mut sn, &mut client, &mut server, &mut crt, 30.0, 10);
    assert_eq!(
        names(&client).map(|(_, n)| n),
        Some("skin 0,skin 1,keepupright,ignite,remove,gravity,collision_on".to_string())
    );
    client.command(prop("collision_on"));
    step(&mut sn, &mut client, &mut server, &mut crt, 30.0, 10);
    let (asked, applied): (i64, i64) = crt.actors()[0]
        .eval("return state.d.asked, state.d.applied")
        .unwrap();
    assert_eq!((asked, applied), (13, 12), "listing asks no hook");
    client.command(prop("remove"));
    step(&mut sn, &mut client, &mut server, &mut crt, 30.0, 10);
    let m3 = gm_content::Mount::for_install(&inst).unwrap();
    let choice = [
        "models/props_c17/furniturecouch001a.mdl",
        "models/props_vehicles/car001a_hatchback.mdl",
        "models/props_c17/oildrum001.mdl",
        "models/props_lab/monitor01a.mdl",
        "models/props_combine/combine_barricade_short01a.mdl",
        "models/props_junk/trashdumpster01a.mdl",
        "models/props_wasteland/laundry_washer003.mdl",
        "models/props_c17/doll01.mdl",
        "models/props_junk/garbage_metalcan001a.mdl",
        "models/props_vehicles/van001a_physics.mdl",
    ]
    .into_iter()
    .find_map(|m| {
        let info = gm_dedicated::model_info(&m3, m).ok()?;
        let part = info.body_groups.iter().position(|&n| n > 1)?;
        Some((m, part, info.body_groups[part]))
    });
    let Some((model, part, count)) = choice else {
        eprintln!("skipped the body-group half: no candidate has a part with several models");
        return;
    };
    client.command(gm_net::Command::Spawn {
        model: model.into(),
    });
    step(&mut sn, &mut client, &mut server, &mut crt, 30.0, 40);
    assert_eq!(server.sandbox.props().len(), 1, "{model}");
    client.command(prop(&format!("bodygroups {part} {count}")));
    step(&mut sn, &mut client, &mut server, &mut crt, 30.0, 10);
    assert_eq!(
        server.sandbox.props()[0].body,
        "",
        "model {count} of part {part} does not exist"
    );
    client.command(prop(&format!("bodygroups {part} 1")));
    step(&mut sn, &mut client, &mut server, &mut crt, 30.0, 10);
    let want: String = (0..=part)
        .map(|i| if i == part { '1' } else { '0' })
        .collect();
    assert_eq!(server.sandbox.props()[0].body, want, "{model}");
    let snap_body = client
        .world
        .entities
        .values()
        .next()
        .map(|e| e.body.clone());
    assert_eq!(
        snap_body,
        Some(want),
        "the snapshot carries the body groups"
    );
    let (asked, applied, last): (i64, i64, String) = crt.actors()[0]
        .eval("return state.d.asked, state.d.applied, state.d.last")
        .unwrap();
    assert_eq!((asked, applied, last.as_str()), (12, 11, "bodygroups"));
}

#[test]
fn player_model_choice_is_worn_from_the_next_spawn_and_player_set_model_may_replace_it() {
    let Some(inst) = gm_content::find_install() else {
        eprintln!("skipped: no Garry's Mod install");
        return;
    };
    let mount = gm_content::Mount::for_install(&inst).unwrap();
    let world = gm_dedicated::load_world(&mount, "gm_construct").unwrap();
    let mut server = gm_server::Server::new(&world.collision, world.physics, world.world_body);
    let mut rt = gm_script::Runtime::new();
    rt.load(gm_script::ActorSpec {
        addon: "test".into(),
        name: "model".into(),
        priority: 1,
        source: "state.n = 0
            events.on('player.set_model', function(e)
                state.n += 1
                state.last = e.model
                if e.model == 'models/player/alyx.mdl' then return 'models/player/breen.mdl' end
            end)
            events.on('tick.post', function()
                if game.tick() % 5 == 0 then
                    for _, pl in ipairs(world.players()) do
                        net.send('log', { n = state.n, last = state.last or '' }, pl.id)
                    end
                end
            end)"
            .into(),
        config: relaxed(),
    })
    .unwrap();
    let mut sn = gm_net::ServerNet::new("gm_construct", glam::Vec3::new(823.235, -600.0, -143.6));
    gm_script::attach(rt, &mut server, &mut sn.hooks);
    let net = gm_net::Loopback::new(1, 0.0, 7);
    let (mut s_ep, mut c_ep) = (net.endpoint(0), net.endpoint(1));
    let mut client = gm_net::ClientNet::new(1, "bot");
    let mut crt = gm_script::Runtime::client();
    crt.load(gm_script::ActorSpec {
        addon: "test".into(),
        name: "probe".into(),
        priority: 1,
        source: "net.receive('log', function(p) state.d = p.data end)".into(),
        config: relaxed(),
    })
    .unwrap();
    let mut step = |sn: &mut gm_net::ServerNet,
                    client: &mut gm_net::ClientNet,
                    server: &mut gm_server::Server<'_>,
                    crt: &mut gm_script::Runtime,
                    n: usize| {
        for _ in 0..n {
            client.tick(gm_net::NetCmd::default(), &mut c_ep);
            net.tick();
            sn.receive(server, &mut s_ep);
            sn.tick(server);
            sn.send(server, &mut s_ep);
            client.receive(&mut c_ep);
            crt.client_tick(&client.world, true);
            let inbox = std::mem::take(&mut client.addon_msgs);
            crt.client_net(&client.world, inbox);
        }
    };
    step(&mut sn, &mut client, &mut server, &mut crt, 20);
    // a fresh player is kleiner, as a fresh bot in the game
    assert_eq!(server.players[&1].model, "models/player/kleiner.mdl");
    let snap_model = |client: &gm_net::ClientNet| {
        gm_server::player_models::path_of_index(client.world.players[0].model).to_string()
    };
    assert_eq!(snap_model(&client), "models/player/kleiner.mdl");
    // the choice is remembered now and worn after the respawn
    client.command(gm_net::Command::PlayerModel {
        name: "combine".into(),
    });
    step(&mut sn, &mut client, &mut server, &mut crt, 5);
    assert_eq!(server.players[&1].model, "models/player/kleiner.mdl");
    server.kill_player(1);
    step(&mut sn, &mut client, &mut server, &mut crt, 160);
    assert!(server.players[&1].alive, "respawned");
    assert_eq!(
        server.players[&1].model,
        "models/player/combine_soldier.mdl"
    );
    assert_eq!(snap_model(&client), "models/player/combine_soldier.mdl");
    // `cl_playerskin` / `cl_playerbodygroups` ride along from the next spawn, unclamped as the
    // game keeps them (measured `lua_skin_body`: SetSkin(7) on a one-skin model stays 7), the
    // body string cut to its digits; the `kill` console command is the respawn here
    client.command(gm_net::Command::PlayerSkin {
        skin: 7,
        body: "x1y".into(),
    });
    step(&mut sn, &mut client, &mut server, &mut crt, 5);
    assert_eq!(
        (server.players[&1].skin, server.players[&1].body.as_str()),
        (0, "")
    );
    client.command(gm_net::Command::Kill);
    step(&mut sn, &mut client, &mut server, &mut crt, 5);
    assert!(!server.players[&1].alive, "kill command");
    step(&mut sn, &mut client, &mut server, &mut crt, 160);
    assert!(server.players[&1].alive, "respawned after kill");
    assert_eq!(
        (server.players[&1].skin, server.players[&1].body.as_str()),
        (7, "1")
    );
    let snap_skin = |client: &gm_net::ClientNet| {
        let p = client.world.players.iter().find(|p| p.id == 1).unwrap();
        (p.skin, p.body.clone())
    };
    assert_eq!(snap_skin(&client), (7, "1".to_string()));
    client.command(gm_net::Command::PlayerSkin {
        skin: 0,
        body: String::new(),
    });
    // an unknown name falls back to kleiner
    client.command(gm_net::Command::PlayerModel {
        name: "nonsense".into(),
    });
    step(&mut sn, &mut client, &mut server, &mut crt, 5);
    server.kill_player(1);
    step(&mut sn, &mut client, &mut server, &mut crt, 160);
    assert_eq!(server.players[&1].model, "models/player/kleiner.mdl");
    // the hook replaces alyx with breen
    client.command(gm_net::Command::PlayerModel {
        name: "alyx".into(),
    });
    step(&mut sn, &mut client, &mut server, &mut crt, 5);
    server.kill_player(1);
    step(&mut sn, &mut client, &mut server, &mut crt, 160);
    assert_eq!(server.players[&1].model, "models/player/breen.mdl");
    assert_eq!(snap_model(&client), "models/player/breen.mdl");
    let (n, last): (i64, String) = crt.actors()[0]
        .eval("return state.d.n, state.d.last")
        .unwrap();
    assert_eq!(last, "models/player/alyx.mdl");
    assert!(n >= 3, "asked on every spawn, {n}");
}

/// `addons/examples/prop_protection`: another player's prop refuses the physgun, the +use
/// pickup and the context-menu properties; the owner's own touches go through. The game has no
/// protection of its own, the example builds it on the snapshot's `owner`.
#[test]
fn prop_protection_example_keeps_other_players_off_a_prop() {
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
        .join("../../addons/examples/prop_protection");
    rt.load_addon(&root).unwrap();
    let mut sn = gm_net::ServerNet::new("gm_construct", glam::Vec3::new(823.235, -500.0, -143.0));
    gm_script::attach(rt, &mut server, &mut sn.hooks);
    let net = gm_net::Loopback::new(2, 0.0, 9);
    let (mut s_ep, mut e1, mut e2) = (net.endpoint(0), net.endpoint(1), net.endpoint(2));
    let mut owner = gm_net::ClientNet::new(1, "owner");
    let mut other = gm_net::ClientNet::new(1, "other");
    let mut crt = gm_script::Runtime::client();
    crt.load(gm_script::ActorSpec {
        addon: "test".into(),
        name: "probe".into(),
        priority: 1,
        source: "net.receive('prop_protection', function(p) state.d = p.data end)".into(),
        config: relaxed(),
    })
    .unwrap();
    // both look 30 degrees down at the same spot in front of the spawn, the physgun out
    let mut step = |sn: &mut gm_net::ServerNet,
                    server: &mut gm_server::Server<'_>,
                    crt: &mut gm_script::Runtime,
                    owner: &mut gm_net::ClientNet,
                    other: &mut gm_net::ClientNet,
                    owner_buttons: u32,
                    other_buttons: u32,
                    n: usize| {
        for _ in 0..n {
            let cmd = |buttons| gm_net::NetCmd {
                view_angles: glam::Vec3::new(30.0, 0.0, 0.0),
                buttons,
                weapon: 1,
                ..Default::default()
            };
            owner.tick(cmd(owner_buttons), &mut e1);
            other.tick(cmd(other_buttons), &mut e2);
            net.tick();
            sn.receive(server, &mut s_ep);
            sn.tick(server);
            sn.send(server, &mut s_ep);
            owner.receive(&mut e1);
            other.receive(&mut e2);
            crt.client_tick(&other.world, true);
            let inbox = std::mem::take(&mut other.addon_msgs);
            crt.client_net(&other.world, inbox);
        }
    };
    use gm_move::buttons::ATTACK;
    step(
        &mut sn,
        &mut server,
        &mut crt,
        &mut owner,
        &mut other,
        0,
        0,
        30,
    );
    let (me, you) = (owner.player_id.unwrap(), other.player_id.unwrap());
    assert_ne!(me, you);
    // the second player stands where the first one does, so both views hit the same crate
    let origin = server.players[&me].state.origin;
    server.players.get_mut(&you).unwrap().state.origin = origin;
    owner.command(gm_net::Command::Spawn {
        model: "models/props_junk/wood_crate001a.mdl".into(),
    });
    step(
        &mut sn,
        &mut server,
        &mut crt,
        &mut owner,
        &mut other,
        0,
        0,
        30,
    );
    let crate_id = server.sandbox.props()[0].id;
    assert_eq!(server.sandbox.props()[0].owner, me);
    // moved to 60 u ahead at hull height: in +use reach (the spawn point of the 30 degree look,
    // ~128 u out, is beyond PLAYER_USE_RADIUS) and still under both views
    assert!(server.teleport_prop(
        crate_id,
        origin + glam::Vec3::new(60.0, 0.0, 20.0),
        glam::Vec3::ZERO
    ));
    step(
        &mut sn,
        &mut server,
        &mut crt,
        &mut owner,
        &mut other,
        0,
        0,
        20,
    );
    // the other player's physgun: asked and refused, nothing held
    step(
        &mut sn,
        &mut server,
        &mut crt,
        &mut owner,
        &mut other,
        0,
        ATTACK,
        5,
    );
    assert!(
        server.players[&you].held.is_none(),
        "the other player's physgun refused on a foreign prop"
    );
    step(
        &mut sn,
        &mut server,
        &mut crt,
        &mut owner,
        &mut other,
        0,
        0,
        5,
    );
    // the other player's +use on it: refused as well
    step(
        &mut sn,
        &mut server,
        &mut crt,
        &mut owner,
        &mut other,
        0,
        gm_server::IN_USE,
        2,
    );
    assert!(server.players[&you].carry.is_none());
    step(
        &mut sn,
        &mut server,
        &mut crt,
        &mut owner,
        &mut other,
        0,
        0,
        5,
    );
    // a property from the other player: the crate stays
    other.command(gm_net::Command::Property {
        name: "remove".into(),
    });
    step(
        &mut sn,
        &mut server,
        &mut crt,
        &mut owner,
        &mut other,
        0,
        0,
        10,
    );
    assert_eq!(
        server.sandbox.props().len(),
        1,
        "remove refused for the other player"
    );
    // the owner's physgun takes it
    step(
        &mut sn,
        &mut server,
        &mut crt,
        &mut owner,
        &mut other,
        ATTACK,
        0,
        5,
    );
    assert!(
        server.players[&me].held.is_some(),
        "the owner's own physgun goes through"
    );
    step(
        &mut sn,
        &mut server,
        &mut crt,
        &mut owner,
        &mut other,
        0,
        0,
        5,
    );
    // the counter reaches the other player on their next spawn
    server.damage_player(you, 1000.0, 0);
    step(
        &mut sn,
        &mut server,
        &mut crt,
        &mut owner,
        &mut other,
        0,
        0,
        200,
    );
    let (blocked, physgun, pickup, property): (i64, i64, i64, i64) = crt.actors()[0]
        .eval("return state.d.blocked, state.d.physgun, state.d.use, state.d.property")
        .unwrap();
    // the physgun asks on every tick the button is held with nothing in the beam, the +use
    // pickup once per press, the property once
    assert_eq!(
        (blocked, physgun, pickup, property),
        (7, 5, 1, 1),
        "five physgun ticks, one pickup, one property"
    );
    let _ = crate_id;
}
