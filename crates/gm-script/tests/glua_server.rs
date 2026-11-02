//! net, ConVar, concommand, file, CTakeDamageInfo and the GM hook bridge.
use glam::Vec3;
use gm_script::api::Payload;
use gm_script::snapshot::PlayerSnap;
use gm_script::{ActorConfig, ActorSpec, Runtime, Snapshot};
use std::sync::Arc;

fn snapshot() -> Arc<Snapshot> {
    let mut s = Snapshot::default();
    s.cvars.insert("sv_gravity".into(), "600".into());
    s.players.insert(
        1,
        PlayerSnap {
            id: 1,
            origin: Vec3::ZERO,
            velocity: Vec3::ZERO,
            view_angles: Vec3::ZERO,
            held: None,
            alive: true,
            health: 100,
            armor: 0,
            max_health: 100,
            identity: Some("STEAM_0:1:2".into()),
            model: String::new(),
            skin: 0,
            body: String::new(),
            weapons: vec![],
            active_weapon: String::new(),
            frags: 0,
            deaths: 0,
        },
    );
    Arc::new(s)
}

fn runtime(source: &str) -> Runtime {
    let mut rt = Runtime::new();
    rt.load(ActorSpec {
        addon: "glua".into(),
        name: "srv".into(),
        priority: 0,
        source: source.into(),
        config: ActorConfig {
            cpu_budget: std::time::Duration::from_millis(50),
            ..Default::default()
        },
    })
    .unwrap();
    rt
}

#[test]
fn net_messages_round_trip_through_the_host_transport() {
    let mut rt = runtime(
        "net.Receive('hello', function(len, ply)
             state.got = { net.ReadString(), net.ReadUInt(8), net.ReadBool(), tostring(net.ReadVector()), net.ReadEntity():EntIndex(), net.ReadTable().k, ply:UserID() }
         end)
         events.on('tick.pre', function()
             net.Start('hello')
             net.WriteString('hi') net.WriteUInt(200, 8) net.WriteBool(true) net.WriteVector(Vector(1, 2, 3)) net.WriteEntity(NULL) net.WriteTable({ k = 'v' })
             net.Send(Player(1))
         end)",
    );
    let snap = snapshot();
    rt.actors_mut()[0]
        .dispatch(&snap, "tick.pre", &Payload::default(), 1)
        .unwrap();
    let out = rt.drain_net();
    assert_eq!(out.len(), 1);
    assert_eq!(out[0].0, 1);
    assert_eq!(out[0].1, "hello");
    // deliver it back as an incoming message
    let mut p = Payload::default();
    p.ints.push(("player", 1));
    p.net = Some(("hello".into(), out[0].2.clone()));
    rt.actors_mut()[0]
        .dispatch(&snap, "net.message", &p, 2)
        .unwrap();
    let (s, n, b, v, e, k, who): (String, i64, bool, String, i64, String, i64) =
        rt.actors()[0].eval("return unpack(state.got)").unwrap();
    assert_eq!((s.as_str(), n, b), ("hi", 200, true));
    assert_eq!(v, "1.000000 2.000000 3.000000");
    assert_eq!(e, 0);
    assert_eq!(k, "v");
    assert_eq!(who, 1);
}

#[test]
fn convars_read_the_snapshot_and_hooks_bridge_events() {
    let mut rt = runtime(
        "state.log = {}
         hook.Add('Initialize', 'a', function() table.insert(state.log, 'init') end)
         hook.Add('PlayerHurt', 'a', function(ply, att, hp, dmg) table.insert(state.log, 'hurt ' .. ply:UserID() .. ' ' .. dmg) end)
         hook.Add('EntityTakeDamage', 'a', function(ply, dmg) if dmg:GetDamage() > 50 then return true end dmg:ScaleDamage(0.5) end)
         hook.Add('PlayerSay', 'a', function(ply, text) if text == 'drop' then return '' end return text:upper() end)
         hook.Add('KeyPress', 'a', function(ply, key) table.insert(state.log, 'key ' .. key) end)
         events.on('tick.pre', function()
             local g = GetConVar('sv_gravity')
             state.cv = { g:GetInt(), g:GetFloat(), g:GetBool(), GetConVar('nope') == nil, cvars.Number('sv_gravity', 1) }
             concommand.Add('say_hi', function(ply, cmd, args) state.cmd = cmd .. ':' .. table.concat(args, ',') end)
             RunConsoleCommand('say_hi', 'a', 2)
             local d = DamageInfo() d:SetDamage(10) d:SetDamageType(DMG_BULLET) d:AddDamage(5)
             state.dmg = { d:GetDamage(), d:IsBulletDamage(), d:IsFallDamage() }
         end)",
    );
    let snap = snapshot();
    let a = &mut rt.actors_mut()[0];
    a.dispatch(&snap, "tick.pre", &Payload::default(), 1)
        .unwrap();
    let mut p = Payload::default();
    p.ints.push(("player", 1));
    p.floats.push(("amount", 30.0));
    p.ints.push(("type", 2));
    a.dispatch(&snap, "player.damaged", &p, 1).unwrap();
    let mut k = Payload::default();
    k.ints.push(("player", 1));
    k.strs.push(("key", "IN_JUMP".into()));
    a.dispatch(&snap, "player.key_press", &k, 1).unwrap();
    // requests: EntityTakeDamage blocks over 50, halves otherwise
    let mut big = Payload::default();
    big.ints.push(("player", 1));
    big.floats.push(("amount", 80.0));
    big.ints.push(("type", 2));
    let blocked = a.dispatch_reply(&snap, "player.damage", &big, 1).unwrap();
    let mut small = Payload::default();
    small.ints.push(("player", 1));
    small.floats.push(("amount", 20.0));
    small.ints.push(("type", 2));
    let halved = a.dispatch_reply(&snap, "player.damage", &small, 1).unwrap();
    let mut chat = Payload::default();
    chat.ints.push(("player", 1));
    chat.strs.push(("text", "drop".into()));
    let dropped = a.dispatch_reply(&snap, "player.chat", &chat, 1).unwrap();
    chat.strs.clear();
    chat.strs.push(("text", "hello".into()));
    let replaced = a.dispatch_reply(&snap, "player.chat", &chat, 1).unwrap();
    eprintln!("blocked {blocked:?} halved {halved:?} dropped {dropped:?} replaced {replaced:?}");
    assert!(matches!(blocked, gm_script::Reply::Cancel));
    assert!(matches!(halved, gm_script::Reply::Number(x) if (x - 10.0).abs() < 1e-6));
    assert!(matches!(dropped, gm_script::Reply::Cancel));
    assert!(matches!(replaced, gm_script::Reply::Text(ref t) if t == "HELLO"));
    let (log, cv, cmd, dmg): (String, Vec<mlua::Value>, String, Vec<mlua::Value>) = rt.actors()[0]
        .eval("return table.concat(state.log, '|'), state.cv, state.cmd, state.dmg")
        .unwrap();
    assert_eq!(log, "init|hurt 1 30|key 2");
    let num = |v: &mlua::Value| match v {
        mlua::Value::Integer(i) => *i as f64,
        mlua::Value::Number(n) => *n,
        mlua::Value::Boolean(b) => f64::from(u8::from(*b)),
        _ => f64::NAN,
    };
    assert_eq!(
        cv.iter().map(num).collect::<Vec<_>>(),
        vec![600.0, 600.0, 1.0, 1.0, 600.0]
    );
    assert_eq!(cmd, "say_hi:a,2");
    assert_eq!(
        format!("{dmg:?}"),
        "[Integer(15), Boolean(true), Boolean(false)]"
    );
}
