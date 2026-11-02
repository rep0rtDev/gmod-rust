//! math.ease, VMatrix and the scripted ENT / SWEP tables.
use gm_script::api::{Payload, QueuedWrite};
use gm_script::{ActorConfig, ActorSpec, Runtime, Snapshot};
use std::sync::Arc;

fn runtime(source: &str) -> Runtime {
    let mut rt = Runtime::new();
    rt.load(ActorSpec {
        addon: "glua".into(),
        name: "cls".into(),
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
fn easing_and_matrices() {
    let rt = runtime("");
    let (a, b, c, d): (f64, f64, f64, f64) = rt.actors()[0]
        .eval("return math.ease.InQuad(0.5), math.ease.OutBounce(1), math.ease.InOutSine(0.5), math.ease.InOutBack(0)")
        .unwrap();
    for (got, want) in [(a, 0.25), (b, 1.0), (c, 0.5), (d, 0.0)] {
        assert!((got - want).abs() < 1e-9, "{got} vs {want}");
    }
    let (t, p, inv_ok, back): (String, String, bool, String) = rt.actors()[0]
        .eval(
            "local m = Matrix()
             m:SetAngles(Angle(0, 90, 0))
             m:SetTranslation(Vector(10, 0, 0))
             local p = m * Vector(1, 0, 0)
             local i = m:GetInverseTR()
             local back = i * p
             return tostring(m:GetTranslation()), string.format('%.3f %.3f %.3f', p.x, p.y, p.z), i ~= nil, string.format('%.3f %.3f %.3f', back.x, back.y, back.z)",
        )
        .unwrap();
    assert_eq!(t, "10.000000 0.000000 0.000000");
    assert_eq!(p, "10.000 1.000 0.000");
    assert!(inv_ok);
    assert_eq!(back, "1.000 0.000 0.000");
}

#[test]
fn scripted_entity_tables_register_and_dispatch() {
    let mut rt = runtime(
        "ENT = { Type = 'anim', Base = 'base_anim', PrintName = 'Crate', Spawnable = true, Model = 'models/props_junk/wood_crate001a.mdl' }
         function ENT:Initialize() state.init = self:GetClass() end
         function ENT:Touch(other) state.touched = other:EntIndex() end
         function ENT:OnTakeDamage(d) state.dmg = d:GetDamage() return true end
         scripted_ents.Register(ENT, 'sent_crate')
         SWEP = { PrintName = 'Zap', Slot = 2, Primary = { ClipSize = 10, DefaultClip = 20, Ammo = 'Pistol', Automatic = false }, Secondary = {} }
         function SWEP:PrimaryAttack() state.fired = self.Owner:UserID() end
         weapons.Register(SWEP, 'weapon_zap')",
    );
    let writes: Vec<QueuedWrite> = rt.drain_writes().into_iter().map(|w| w.3).collect();
    assert!(
        matches!(&writes[0], QueuedWrite::EntityRegister { def } if def.class == "sent_crate" && def.model == "models/props_junk/wood_crate001a.mdl")
    );
    assert!(
        matches!(&writes[1], QueuedWrite::WeaponRegister { def } if def.class == "weapon_zap" && def.slot == 2 && def.clip_size == 10)
    );
    let snap = Arc::new(Snapshot::default());
    let a = &mut rt.actors_mut()[0];
    let mut p = Payload::default();
    p.ints.push(("entity", 4));
    p.strs.push(("class", "sent_crate".into()));
    a.dispatch(&snap, "entity.init", &p, 1).unwrap();
    p.ints.push(("other", 9));
    a.dispatch(&snap, "entity.touch", &p, 1).unwrap();
    p.floats.push(("amount", 12.0));
    a.dispatch(&snap, "entity.damage", &p, 1).unwrap();
    let mut w = Payload::default();
    w.ints.push(("player", 1));
    w.strs.push(("weapon", "weapon_zap".into()));
    a.dispatch(&snap, "weapon.primary", &w, 1).unwrap();
    let (init, touched, dmg, fired, stored, based): (String, i64, f64, i64, String, bool) = rt.actors()[0]
        .eval("return state.init, state.touched, state.dmg, state.fired, scripted_ents.Get('sent_crate').PrintName, weapons.IsBasedOn('weapon_zap', 'weapon_zap')")
        .unwrap();
    assert_eq!(init, "");
    assert_eq!(touched, 9);
    assert_eq!(dmg, 12.0);
    assert_eq!(fired, 1);
    assert_eq!(stored, "Crate");
    assert!(based);
}

#[test]
fn scripted_callbacks_reach_more_events() {
    let mut rt = runtime(
        "ENT = { Type = 'anim', PrintName = 'Kv', Model = 'models/props_junk/wood_crate001a.mdl' }
         function ENT:SetupDataTables() state.dt = (state.dt or 0) + 1 end
         function ENT:Initialize() state.init = state.dt end
         function ENT:KeyValue(k, v) state.kv = k .. '=' .. v end
         function ENT:AcceptInput(name, a, c, d) state.input = name .. ':' .. tostring(d) return true end
         scripted_ents.Register(ENT, 'kv_ent')
         SWEP = { PrintName = 'Gate', Primary = { ClipSize = 1, DefaultClip = 1, Ammo = 'Pistol' }, Secondary = {} }
         function SWEP:CanPrimaryAttack() return false end
         function SWEP:PrimaryAttack() state.fired = true end
         function SWEP:Equip(o) state.equip = o:UserID() end
         weapons.Register(SWEP, 'weapon_gate')
         events.on('tick.pre', function()
             Entity(4):SetKeyValue('speed', 5)
             state.kill = Entity(4):Input('Open', NULL, NULL, 'wide')
         end)",
    );
    let snap = Arc::new(Snapshot::default());
    let a = &mut rt.actors_mut()[0];
    let mut p = Payload::default();
    p.ints.push(("entity", 4));
    p.strs.push(("class", "kv_ent".into()));
    a.dispatch(&snap, "entity.init", &p, 1).unwrap();
    a.dispatch(&snap, "entity.init", &p, 1).unwrap();
    a.dispatch(&snap, "tick.pre", &Payload::default(), 1)
        .unwrap();
    let mut w = Payload::default();
    w.ints.push(("player", 1));
    w.strs.push(("weapon", "weapon_gate".into()));
    a.dispatch(&snap, "weapon.primary", &w, 1).unwrap();
    let mut pk = Payload::default();
    pk.ints.push(("player", 1));
    pk.strs.push(("class", "weapon_gate".into()));
    a.dispatch(&snap, "player.weapon_pickup", &pk, 1).unwrap();
    let (dt, init, kv, input, kill, fired, equip): (i64, i64, String, String, bool, bool, i64) = a
        .eval("return state.dt, state.init, state.kv, state.input, state.kill, state.fired == true, state.equip")
        .unwrap();
    assert_eq!(
        (dt, init),
        (1, 1),
        "SetupDataTables once, before Initialize"
    );
    assert_eq!(kv, "speed=5");
    assert_eq!(input, "Open:wide");
    assert!(kill, "AcceptInput returning true handles the input");
    assert!(!fired, "CanPrimaryAttack false gates PrimaryAttack");
    assert_eq!(equip, 1);
}
