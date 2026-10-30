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
    assert!(rt.tool_click(&mut server, 1, "remover_lua", &hit, false));
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
