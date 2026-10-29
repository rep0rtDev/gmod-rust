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
