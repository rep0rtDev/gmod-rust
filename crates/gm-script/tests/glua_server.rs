//! net, ConVar, concommand, file, CTakeDamageInfo and the GM hook bridge.
use glam::Vec3;
use gm_script::api::{Payload, QueuedWrite};
use gm_script::snapshot::PlayerSnap;
use gm_script::{ActorConfig, ActorSpec, Runtime, Snapshot};
use std::sync::Arc;

fn snapshot() -> Arc<Snapshot> {
    let mut s = Snapshot::default();
    s.cvars.insert("sv_gravity".into(), "600".into());
    s.entities.insert(
        5,
        gm_script::snapshot::EntitySnap {
            id: 5,
            model: "models/props_c17/oildrum001.mdl".into(),
            pos: Vec3::new(0.0, 0.0, 0.0),
            angles: Vec3::ZERO,
            frozen: false,
            skin: 0,
            body: String::new(),
            owner: 1,
            class: "prop_physics".into(),
            health: 0,
            max_health: 0,
            velocity: Vec3::ZERO,
            ang_velocity: Vec3::new(1.0, 2.0, 3.0),
            mass: 40.0,
            damping: (0.1, 0.2),
            asleep: false,
            npc_target: 0,
            npc_chase: 0,
            npc_goal: None,
        },
    );
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
            weapons: vec!["weapon_pistol".into(), "weapon_smg1".into()],
            active_weapon: String::new(),
            ammo: vec![("Pistol".into(), 20), ("SMG1".into(), 45)],
            clips: vec![
                ("weapon_pistol".into(), 18, 0),
                ("weapon_smg1".into(), 45, 1),
            ],
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

#[test]
fn gm_hooks_bridge_more_events_and_defaults() {
    let mut rt = runtime(
        "state.log = {}
         hook.Add('PlayerInitialSpawn', 'a', function(ply) table.insert(state.log, 'initial ' .. ply:UserID()) end)
         hook.Add('PlayerTick', 'a', function(ply) table.insert(state.log, 'tick') end)
         hook.Add('EntityNetworkedVarChanged', 'a', function(ent, name, old, new)
             table.insert(state.log, 'nw ' .. name .. ' ' .. tostring(old) .. '>' .. tostring(new))
         end)
         hook.Add('PlayerShouldTakeDamage', 'a', function(ply, att) return false end)
         hook.Add('PreRegisterSENT', 'a', function(ent, class) if class == 'blocked_ent' then return false end end)
         events.on('tick.pre', function()
             Player(1):SetNWInt('score', 7)
             Player(1):SetNWInt('score', 7)
             state.desc = hook.Run('GetGameDescription')
             state.suicide = hook.Run('CanPlayerSuicide', Player(1))
             scripted_ents.Register({ Type = 'anim', Model = 'models/props_c17/oildrum001.mdl', PrintName = 'B' }, 'blocked_ent')
             scripted_ents.Register({ Type = 'anim', Model = 'models/props_c17/oildrum001.mdl', PrintName = 'O' }, 'ok_ent')
             state.reg = { scripted_ents.Get('blocked_ent') == nil, scripted_ents.Get('ok_ent') ~= nil }
         end)",
    );
    let snap = snapshot();
    let a = &mut rt.actors_mut()[0];
    a.dispatch(&snap, "tick.pre", &Payload::default(), 1)
        .unwrap();
    a.dispatch(&snap, "tick.pre", &Payload::default(), 2)
        .unwrap();
    let mut dmg = Payload::default();
    dmg.ints.push(("player", 1));
    dmg.floats.push(("amount", 20.0));
    dmg.ints.push(("type", 2));
    let refused = a.dispatch_reply(&snap, "player.damage", &dmg, 2).unwrap();
    assert!(matches!(refused, gm_script::Reply::Cancel));
    let log: Vec<String> = a.eval("return state.log").unwrap();
    assert_eq!(
        log.iter().filter(|l| l.starts_with("initial")).count(),
        1,
        "PlayerInitialSpawn once per player: {log:?}"
    );
    assert_eq!(log.iter().filter(|l| *l == "tick").count(), 2, "{log:?}");
    assert_eq!(
        log.iter().filter(|l| l.starts_with("nw")).count(),
        1,
        "{log:?}"
    );
    assert!(log.contains(&"nw score nil>7".to_string()), "{log:?}");
    let desc: String = a.eval("return state.desc").unwrap();
    assert_eq!(desc, "Sandbox");
    let suicide: bool = a.eval("return state.suicide").unwrap();
    assert!(suicide);
    let reg: Vec<bool> = a.eval("return state.reg").unwrap();
    assert_eq!(reg, vec![true, true]);
}

#[test]
fn navmesh_over_a_flat_recast_mesh() {
    let mut g = gm_nav::Geometry::default();
    let sq = [
        Vec3::new(-1024.0, -1024.0, 0.0),
        Vec3::new(1024.0, -1024.0, 0.0),
        Vec3::new(1024.0, 1024.0, 0.0),
        Vec3::new(-1024.0, 1024.0, 0.0),
    ];
    g.push_polygon(&sq);
    let mut rev = sq;
    rev.reverse();
    g.push_polygon(&rev);
    g.mins = Vec3::new(-1024.0, -1024.0, -8.0);
    g.maxs = Vec3::new(1024.0, 1024.0, 8.0);
    let mesh = gm_nav::NavMesh::from_geometry(&g, &gm_nav::NavParams::default()).unwrap();
    let mut rt = runtime(
        "events.on('tick.pre', function()
             if not navmesh.IsLoaded() then return end
             local a = navmesh.GetNearestNavArea(Vector(100, 100, 40))
             local found = navmesh.Find(Vector(1000, 1000, 0), 300, 18, 100)
             local gz = navmesh.GetGroundHeight(Vector(500, 500, 900))
             state.z = { a:GetCenter().z, a:GetZ(Vector(300, 300, 500)), gz }
             state.r = {
                 navmesh.IsLoaded(), navmesh.GetNavAreaCount() > 0, a:IsValid(), math.abs(a:GetCenter().z) <= 8,
                 math.abs(a:GetZ(Vector(300, 300, 500))) <= 8, #found > 0, found[1]:GetID() == navmesh.GetNavAreaByID(found[1]:GetID()):GetID(),
                 math.abs(gz) <= 8, a:Contains(a:GetCenter()), navmesh.GetNearestNavArea(Vector(5000, 5000, 0), false, 100) == NULL,
                 #navmesh.GetAllNavAreas() == #navmesh.FindInBox(Vector(-2000, -2000, -10), Vector(2000, 2000, 10)),
             }
         end)",
    );
    let mut snap = (*snapshot()).clone();
    snap.nav = gm_script::snapshot::NavRef(&mesh as *const _);
    let snap = Arc::new(snap);
    let a = &mut rt.actors_mut()[0];
    a.dispatch(&snap, "tick.pre", &Payload::default(), 1)
        .unwrap();
    let z: Vec<f32> = a.eval("return state.z").unwrap();
    let r: Vec<bool> = a.eval("return state.r").unwrap();
    // the mesh surface sits one voxel (cell_height 4) above the geometry
    assert_eq!(r, vec![true; 11], "{r:?} z {z:?}");
    let empty = snapshot();
    a.dispatch(&empty, "tick.pre", &Payload::default(), 2)
        .unwrap();
    let loaded: bool = a.eval("return navmesh.IsLoaded()").unwrap();
    assert!(!loaded);
}

#[test]
fn libs_team_list_numpad_duplicator() {
    let mut rt = runtime(
        "events.on('tick.pre', function()
             team.SetUp(1, 'Red', Color(255, 0, 0))
             Player(1):SetTeam(1)
             list.Set('Desktop Widgets', 'clock', { x = 1 })
             numpad.Register('Toggle', function(ply, ent, on) state.np = (state.np or '') .. ent .. tostring(on) end)
             local id = numpad.OnDown(Player(1), KEY_PAD_1, 'Toggle', 'door', true)
             numpad.OnUp(Player(1), KEY_PAD_1, 'Toggle', 'door', false)
             numpad.Activate(Player(1), KEY_PAD_1)
             numpad.Deactivate(Player(1), KEY_PAD_1)
             numpad.Remove(id)
             numpad.Activate(Player(1), KEY_PAD_1)
             duplicator.StoreEntityModifier(Entity(5), 'colour', { r = 1 })
             local dupe = duplicator.Copy(Entity(5))
             local hit, far = util.IntersectRayWithSphere(Vector(-10, 0, 0), Vector(1, 0, 0), Vector(0, 0, 0), 2)
             player_manager.AddValidModel('kleiner', 'models/player/kleiner.mdl')
             state.r = {
                 team.GetName(1), team.NumPlayers(1), team.GetPlayers(1)[1]:UserID(), list.Get('Desktop Widgets').clock.x,
                 state.np, dupe.Entities[5].Class, dupe.Entities[5].EntityMods.colour.r, hit, far,
                 player_manager.TranslatePlayerModel('kleiner'), math.ease.InQuad(0.5), HexToColor('#ff0080').g, STNDRD(22),
                 system.IsLinux(), CUserCmd ~= nil, Tool ~= nil,
             }
         end)",
    );
    let snap = snapshot();
    let a = &mut rt.actors_mut()[0];
    a.dispatch(&snap, "tick.pre", &Payload::default(), 1)
        .unwrap();
    let r: Vec<String> = a.eval("return { tostring(state.r[1]), tostring(state.r[2]), tostring(state.r[3]), tostring(state.r[4]), tostring(state.r[5]), tostring(state.r[6]), tostring(state.r[7]), tostring(state.r[8]), tostring(state.r[9]), tostring(state.r[10]), tostring(state.r[11]), tostring(state.r[12]), tostring(state.r[13]), tostring(state.r[14]) }").unwrap();
    assert_eq!(
        r,
        vec![
            "Red",
            "1",
            "1",
            "1",
            "doortruedoorfalse",
            "prop_physics",
            "1",
            "8",
            "12",
            "models/player/kleiner.mdl",
            "0.25",
            "0",
            "nd",
            "true"
        ]
        .into_iter()
        .map(String::from)
        .collect::<Vec<_>>()
    );
}

#[test]
fn tail_libraries_stack_sound_cookie_gamemode() {
    let mut rt = runtime(
        "events.on('tick.pre', function()
             local st = util.Stack()
             st:Push(1) st:Push(2) st:Push(3)
             local top, size = st:Top(), st:Size()
             local popped = st:Pop()
             local snd = CreateSound(Entity(5), 'ambient/wind1.wav')
             snd:PlayEx(0.5, 90)
             cookie.Set('seen', 3)
             properties.Add('bonk', { MenuLabel = 'Bonk', Order = 1 })
             gamemode.Register({ Name = 'Mine' }, 'mine', 'base')
             local sched = ai_schedule.New('patrol')
             sched:EngTask('TASK_WAIT', 2)
             state.r = { top, size, popped, st:Size(), snd:IsPlaying() and 1 or 0, snd:GetPitch(), cookie.GetNumber('seen', 0),
                 properties.List.bonk.Order, gamemode.Get('mine').Name == 'Mine' and 1 or 0, sched:NumTasks(), properties.CanBeTargeted(Entity(5), Player(1)) and 1 or 0 }
         end)",
    );
    let snap = snapshot();
    let a = &mut rt.actors_mut()[0];
    a.dispatch(&snap, "tick.pre", &Payload::default(), 1)
        .unwrap();
    let r: Vec<f64> = a.eval("return state.r").unwrap();
    assert_eq!(
        r,
        vec![3.0, 3.0, 3.0, 2.0, 1.0, 90.0, 3.0, 1.0, 1.0, 1.0, 1.0]
    );
}

#[test]
fn player_ammo_and_clips_over_the_inventory() {
    let mut rt = runtime(
        "events.on('tick.pre', function()
             local ply = Player(1)
             local w = ply:GetWeapon('weapon_pistol')
             state.r = { ply:GetAmmoCount('Pistol'), ply:GetAmmoCount('AR2'), ply:GetAmmo().SMG1, w:Clip1(), w:Clip2(), ply:GetWeapon('weapon_smg1'):Clip2() }
             ply:SetAmmo(7, 'Pistol')
             ply:GiveAmmo(5, 'SMG1')
             ply:RemoveAmmo(3, 'SMG1')
             w:SetClip1(2)
             ply:StripWeapon('weapon_smg1')
             ply:StripWeapons()
         end)",
    );
    let snap = snapshot();
    rt.actors_mut()[0]
        .dispatch(&snap, "tick.pre", &Payload::default(), 1)
        .unwrap();
    let r: Vec<i64> = rt.actors()[0].eval("return state.r").unwrap();
    assert_eq!(r, vec![20, 0, 45, 18, 0, 1]);
    let writes: Vec<QueuedWrite> = rt.drain_writes().into_iter().map(|w| w.3).collect();
    assert!(
        matches!(&writes[0], QueuedWrite::PlayerSetAmmo { player: 1, ammo, count: 7, add: false } if ammo == "Pistol")
    );
    assert!(
        matches!(&writes[1], QueuedWrite::PlayerSetAmmo { ammo, count: 5, add: true, .. } if ammo == "SMG1")
    );
    assert!(matches!(
        &writes[2],
        QueuedWrite::PlayerSetAmmo {
            count: -3,
            add: true,
            ..
        }
    ));
    assert!(
        matches!(&writes[3], QueuedWrite::PlayerSetClip { class, slot: 0, count: 2, .. } if class == "weapon_pistol")
    );
    assert!(
        matches!(&writes[4], QueuedWrite::PlayerStripWeapon { class: Some(c), .. } if c == "weapon_smg1")
    );
    assert!(matches!(
        &writes[5],
        QueuedWrite::PlayerStripWeapon { class: None, .. }
    ));
}

#[test]
fn physobj_mass_angular_velocity_damping_over_the_body() {
    let mut rt = runtime(
        "events.on('tick.pre', function()
             local ph = Entity(5):GetPhysicsObject()
             local w = ph:GetAngleVelocity()
             local l, a = ph:GetDamping()
             local f, t = ph:CalculateForceOffset(Vector(0, 0, 10), Entity(5):GetPos() + Vector(2, 0, 0))
             local al = ph:AlignAngles(Angle(0, 0, 0), Angle(0, 90, 0))
             state.r = { ph:GetMass(), w.x, w.y, w.z, l, a, ph:IsAsleep() and 1 or 0, f.z, t.y, math.Round(al.y) }
             ph:SetMass(80)
             ph:SetDamping(0.5, 0.6)
             ph:SetAngleVelocity(Vector(0, 0, 90))
             ph:Sleep()
         end)",
    );
    let snap = snapshot();
    rt.actors_mut()[0]
        .dispatch(&snap, "tick.pre", &Payload::default(), 1)
        .unwrap();
    let r: Vec<f64> = rt.actors()[0].eval("return state.r").unwrap();
    assert_eq!(r[..4], [40.0, 1.0, 2.0, 3.0]);
    assert!((r[4] - 0.1).abs() < 1e-5 && (r[5] - 0.2).abs() < 1e-5);
    assert_eq!(r[6..], [0.0, 10.0, -20.0, 90.0]);
    let writes: Vec<QueuedWrite> = rt.drain_writes().into_iter().map(|w| w.3).collect();
    assert!(matches!(&writes[0], QueuedWrite::SetMass { ent: 5, mass } if *mass == 80.0));
    assert!(
        matches!(&writes[1], QueuedWrite::SetDamping { ent: 5, linear, angular } if *linear == 0.5 && *angular == 0.6)
    );
    assert!(
        matches!(&writes[2], QueuedWrite::SetVelocity { ent: 5, angular, .. } if angular.z == 90.0)
    );
    assert!(matches!(
        &writes[3],
        QueuedWrite::SetAsleep {
            ent: 5,
            asleep: true
        }
    ));
}
