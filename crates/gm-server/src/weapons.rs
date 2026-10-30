//! The sandbox loadout as the real server reports it: clips, ammo types, reserve ammo on spawn,
//! slots and the deploy time after `SelectWeapon` (recording `weapon_switch`, gm_construct).
//! A switch is immediate: the selected weapon is active in the same tick, with the next
//! primary and secondary fire pushed out by the weapon's deploy time.

use std::collections::BTreeMap;

/// Server tick length the deploy timers count in.
pub const TICK_DT: f32 = 0.015;

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct WeaponDef {
    pub class: &'static str,
    /// Clip size; -1 for weapons without a clip (the game's `Clip1()` / `GetMaxClip1()`).
    pub clip1: i32,
    pub clip2: i32,
    pub ammo1: &'static str,
    pub ammo2: &'static str,
    pub slot: u8,
    pub slot_pos: u8,
    pub weight: u8,
    pub hold_type: &'static str,
    /// Seconds until the next primary/secondary fire after selecting it; `None` not measured yet.
    pub deploy: Option<f32>,
    /// Firing rules measured with a bot (`fire_*`); `None` for weapons not measured yet.
    pub fire: Option<Fire>,
    /// (primary, secondary) timers after a fresh `Give` where they differ from `deploy` (crowbar
    /// 0.2033 against 0.2333 on select; physcannon leaves the secondary at now).
    pub give: Option<(f32, f32)>,
    /// Reserve ammo a fresh `Give` adds on top of the loadout (crossbow 4, RPG 3, frag 1).
    pub give_ammo: i32,
}

/// What a trigger pull launches instead of bullets (`projectile::Projectile`).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Launch {
    Rocket,
    Bolt,
    Grenade,
    Ball,
    /// The SMG1's grenade (`grenade_ar2`).
    Smg1Grenade,
}

impl Launch {
    /// The game's entity class.
    pub fn class(self) -> &'static str {
        match self {
            Launch::Rocket => "rpg_missile",
            Launch::Bolt => "crossbow_bolt",
            Launch::Grenade => "npc_grenade_frag",
            Launch::Ball => "prop_combine_ball",
            Launch::Smg1Grenade => "grenade_ar2",
        }
    }
}

/// How a weapon fires (recordings `fire_*`): every timer is relative to the server tick the
/// usercmd belongs to, so a value set to `x` reads `x - 0.015` in the tick's recording.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Fire {
    /// Shots while the button is held: the primary timer advances by `cycle` from its own expired
    /// value (SMG1, held pistol), not from now.
    pub auto: bool,
    /// Primary timer after a shot (from now for a pressed semi-auto shot and the shotgun).
    pub cycle: f32,
    /// Semi-auto: a fresh press fires once this much time passed since the last shot, even while
    /// `cycle` has not expired (pistol 0.1).
    pub refire_min: Option<f32>,
    pub pellets: u8,
    /// Cone half-angle as the game's spread vector component (sin), at zero penalty.
    pub spread: f32,
    /// Pistol accuracy penalty: spread lerps to this at penalty 1.5, +0.2 per shot, -dt per tick
    /// when the button is not held and `refire_min` has passed.
    pub spread_max: Option<f32>,
    /// Pump after each shot once `cycle` expired, this long (shotgun 0.5333).
    pub pump: Option<f32>,
    /// Reload time after IN_RELOAD with a non-full clip; the clip fills when it expires
    /// (pistol 1.4333, SMG1 1.5, AR2 1.5667, .357 3.6667).
    pub reload: Option<f32>,
    /// Shell-by-shell reload (shotgun): (start, per shell, end) timers. The first shell goes in
    /// when `start` expires, one more per `shell` while the clip can take one, then `end` plays
    /// and the weapon is back; a press at a shell boundary with rounds in the clip fires instead.
    pub shells: Option<(f32, f32, f32)>,
    /// Empty clip and the button down once the primary timer expired: (primary timer, refire
    /// gate) of the dry fire (pistol 0.4, 0.2). An empty clip with the button up reloads.
    pub dry_fire: Option<(f32, f32)>,
    /// Held repeats set the primary timer to `cycle` from now, not from its expired value
    /// (crowbar swings every 27 ticks, physcannon punts every 34).
    pub from_now: bool,
    /// The entity a shot launches (recorded per tick by the probe, `fire_*_proj`).
    pub launch: Option<Launch>,
    /// Secondary timer set with a shot (crowbar 0.6667, crossbow 0.75); untouched otherwise.
    pub secondary_after: Option<f32>,
    /// Reload starting this many ticks after a shot whatever the button, for this long
    /// (crossbow: 7 ticks, 1.8333).
    pub auto_reload: Option<(u32, f32)>,
    /// The tick after a reload refills the clip both timers restart from now (crossbow: -0.0267
    /// at the refill, -0.015 the next tick).
    pub rearm_after_reload: bool,
    /// Pull-back grenade instead of a bullet (frag).
    pub throw: Option<Throw>,
    pub alt: Alt,
    /// A swing that hits players in reach (crowbar: `MELEE_*`).
    pub melee: bool,
}

/// Frag: a press (primary timer expired, reserve left) pulls back and parks the primary timer at
/// `f32::MAX`; the throw animation starts at the release or `drawback_ticks` after the press,
/// whichever is later, the grenade leaves `throw_ticks` later (reserve -1, both timers `cycle`);
/// `redraw` is set on both timers once `cycle` expired. Fitted on five recordings (holds of 1,
/// 2, 5, 20, 90 ticks). The grenade entity itself is not simulated yet.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Throw {
    pub drawback_ticks: u32,
    pub throw_ticks: u32,
    pub cycle: f32,
    pub redraw: f32,
}

/// Secondary fire (IN_ATTACK2) as measured; `None` for weapons whose secondary is unmeasured.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum Alt {
    None,
    /// Shotgun: `n` shells at once when the primary timer expired, primary timer `cycle` then the
    /// usual pump (shot every 70 ticks); `pellets` in one `FireBullets` (12, same cone).
    Shells {
        n: i32,
        pellets: u8,
        cycle: f32,
    },
    /// AR2 ball: a press with both timers expired and alt ammo left sets both to `delay`; when
    /// the primary expires the ball leaves (alt ammo -1) and both timers become `cycle`.
    Ball {
        delay: f32,
        cycle: f32,
    },
    /// SMG1 grenade: with grenades, a press (or hold) with both timers expired launches one the
    /// same tick (alt ammo -1, primary 0.5, secondary 1.0: a hold fires every 67 ticks,
    /// `fire_smg1_grenade*`); with none, a press with the secondary expired sets it to `cycle`.
    GrenadeEmpty {
        cycle: f32,
    },
}

const BASE: Fire = Fire {
    auto: false,
    cycle: 0.0,
    refire_min: None,
    pellets: 0,
    spread: 0.0,
    spread_max: None,
    pump: None,
    reload: None,
    shells: None,
    dry_fire: None,
    from_now: false,
    melee: false,
    launch: None,
    secondary_after: None,
    auto_reload: None,
    rearm_after_reload: false,
    throw: None,
    alt: Alt::None,
};

pub const IN_ATTACK: u32 = 1;
pub const IN_ATTACK2: u32 = 1 << 11;
pub const IN_RELOAD: u32 = 1 << 13;
const PISTOL: Fire = Fire {
    auto: true,
    cycle: 0.5,
    refire_min: Some(0.1),
    pellets: 1,
    spread: 0.00873,
    spread_max: Some(0.05234),
    pump: None,
    reload: Some(1.4333),
    dry_fire: Some((0.4, 0.2)),
    ..BASE
};
const SMG1: Fire = Fire {
    auto: true,
    cycle: 0.075,
    refire_min: None,
    pellets: 1,
    spread: 0.0436,
    spread_max: None,
    pump: None,
    reload: Some(1.5),
    dry_fire: None,
    alt: Alt::GrenadeEmpty { cycle: 0.5 },
    ..BASE
};
const SHOTGUN: Fire = Fire {
    auto: false,
    cycle: 0.3333,
    refire_min: None,
    pellets: 7,
    spread: 0.0872,
    spread_max: None,
    pump: Some(0.5333),
    reload: None,
    shells: Some((0.5, 0.4, 0.4333)),
    dry_fire: None,
    alt: Alt::Shells {
        n: 2,
        pellets: 12,
        cycle: 0.5,
    },
    ..BASE
};
const REVOLVER: Fire = Fire {
    auto: false,
    cycle: 0.75,
    refire_min: None,
    pellets: 1,
    spread: 0.0,
    spread_max: None,
    pump: None,
    reload: Some(3.6667),
    dry_fire: None,
    ..BASE
};
/// Automatic like the SMG1 at 0.1 s per shot, cone 0.02618 (1.5 degrees); the secondary timer
/// is untouched by shots.
const AR2: Fire = Fire {
    auto: true,
    cycle: 0.1,
    pellets: 1,
    spread: 0.02618,
    reload: Some(1.5667),
    alt: Alt::Ball {
        delay: 0.5,
        cycle: 1.0,
    },
    ..BASE
};
/// One bolt (an entity, not `FireBullets`: no shot recorded; the bolt itself is not simulated
/// yet), both timers 0.75; the reload (1.8333) starts 7 ticks after the shot whatever the button
/// and refills the clip when it expires (expiry not in the recording yet).
const CROSSBOW: Fire = Fire {
    cycle: 0.75,
    launch: Some(Launch::Bolt),
    secondary_after: Some(0.75),
    auto_reload: Some((7, 1.8333)),
    rearm_after_reload: true,
    ..BASE
};
/// Swing into nothing: primary 0.4 from now, secondary 0.6667; repeats while held.
const CROWBAR: Fire = Fire {
    melee: true,
    auto: true,
    from_now: true,
    cycle: 0.4,
    secondary_after: Some(0.6667),
    ..BASE
};
/// Punt into nothing: primary 0.5 from now, secondary untouched; repeats while held.
const PHYSCANNON: Fire = Fire {
    auto: true,
    from_now: true,
    cycle: 0.5,
    ..BASE
};
/// Rocket: reserve -1, primary 0.5, secondary untouched. The rocket entity and the 1.6667 reload
/// of both timers when it dies are not simulated yet.
const RPG: Fire = Fire {
    cycle: 0.5,
    launch: Some(Launch::Rocket),
    ..BASE
};
/// Both timers when the rocket dies (wall or sky), whatever the time since the shot.
pub const RPG_ROCKET_DIED: f32 = 1.6667;
const FRAG: Fire = Fire {
    throw: Some(Throw {
        drawback_ticks: 4,
        throw_ticks: 3,
        cycle: 0.5,
        redraw: 1.2667,
    }),
    ..BASE
};

pub const WEAPONS: [WeaponDef; 13] = [
    WeaponDef {
        class: "gmod_camera",
        clip1: -1,
        clip2: -1,
        ammo1: "",
        ammo2: "",
        slot: 5,
        slot_pos: 100,
        weight: 5,
        hold_type: "camera",
        deploy: None,
        fire: None,
        give: None,
        give_ammo: 0,
    },
    WeaponDef {
        class: "gmod_tool",
        clip1: -1,
        clip2: -1,
        ammo1: "",
        ammo2: "",
        slot: 0,
        slot_pos: 100,
        weight: 5,
        hold_type: "revolver",
        deploy: None,
        fire: None,
        give: None,
        give_ammo: 0,
    },
    WeaponDef {
        class: "weapon_357",
        clip1: 6,
        clip2: -1,
        ammo1: "357",
        ammo2: "",
        slot: 1,
        slot_pos: 1,
        weight: 7,
        hold_type: "revolver",
        deploy: Some(0.25),
        fire: Some(REVOLVER),
        give: None,
        give_ammo: 0,
    },
    WeaponDef {
        class: "weapon_ar2",
        clip1: 30,
        clip2: -1,
        ammo1: "AR2",
        ammo2: "AR2AltFire",
        slot: 2,
        slot_pos: 1,
        weight: 5,
        hold_type: "ar2",
        deploy: Some(0.2083),
        fire: Some(AR2),
        give: None,
        give_ammo: 0,
    },
    WeaponDef {
        class: "weapon_crossbow",
        clip1: 1,
        clip2: -1,
        ammo1: "XBowBolt",
        ammo2: "",
        slot: 3,
        slot_pos: 1,
        weight: 6,
        hold_type: "crossbow",
        deploy: Some(0.25),
        fire: Some(CROSSBOW),
        give: None,
        give_ammo: 4,
    },
    WeaponDef {
        class: "weapon_crowbar",
        clip1: -1,
        clip2: -1,
        ammo1: "",
        ammo2: "",
        slot: 0,
        slot_pos: 0,
        weight: 0,
        hold_type: "melee",
        deploy: Some(0.2333),
        fire: Some(CROWBAR),
        give: Some((0.2033, 0.2033)),
        give_ammo: 0,
    },
    WeaponDef {
        class: "weapon_frag",
        clip1: -1,
        clip2: -1,
        ammo1: "Grenade",
        ammo2: "",
        slot: 4,
        slot_pos: 0,
        weight: 1,
        hold_type: "grenade",
        deploy: Some(0.3167),
        fire: Some(FRAG),
        give: None,
        give_ammo: 1,
    },
    WeaponDef {
        class: "weapon_physcannon",
        clip1: -1,
        clip2: -1,
        ammo1: "",
        ammo2: "",
        slot: 0,
        slot_pos: 2,
        weight: 0,
        hold_type: "physgun",
        deploy: None,
        fire: Some(PHYSCANNON),
        give: Some((0.605, 0.0)),
        give_ammo: 0,
    },
    WeaponDef {
        class: "weapon_physgun",
        clip1: -1,
        clip2: -1,
        ammo1: "",
        ammo2: "",
        slot: 0,
        slot_pos: 5,
        weight: 2,
        hold_type: "physgun",
        deploy: Some(0.2417),
        fire: None,
        give: None,
        give_ammo: 0,
    },
    WeaponDef {
        class: "weapon_pistol",
        clip1: 18,
        clip2: -1,
        ammo1: "Pistol",
        ammo2: "",
        slot: 1,
        slot_pos: 0,
        weight: 2,
        hold_type: "pistol",
        deploy: Some(0.2),
        fire: Some(PISTOL),
        give: None,
        give_ammo: 0,
    },
    WeaponDef {
        class: "weapon_rpg",
        clip1: -1,
        clip2: -1,
        ammo1: "RPG_Round",
        ammo2: "",
        slot: 4,
        slot_pos: 1,
        weight: 0,
        hold_type: "rpg",
        deploy: Some(0.275),
        fire: Some(RPG),
        give: None,
        give_ammo: 3,
    },
    WeaponDef {
        class: "weapon_shotgun",
        clip1: 6,
        clip2: -1,
        ammo1: "Buckshot",
        ammo2: "",
        slot: 3,
        slot_pos: 0,
        weight: 4,
        hold_type: "shotgun",
        deploy: Some(0.1833),
        fire: Some(SHOTGUN),
        give: None,
        give_ammo: 0,
    },
    WeaponDef {
        class: "weapon_smg1",
        clip1: 45,
        clip2: -1,
        ammo1: "SMG1",
        ammo2: "SMG1_Grenade",
        slot: 2,
        slot_pos: 0,
        weight: 3,
        hold_type: "smg",
        deploy: Some(0.2083),
        fire: Some(SMG1),
        give: None,
        give_ammo: 0,
    },
];

/// Reserve ammo the sandbox loadout gives on spawn (the player's `GetAmmoCount` per type).
pub const SPAWN_AMMO: [(&str, i32); 8] = [
    ("357", 32),
    ("AR2", 100),
    ("AR2AltFire", 6),
    ("Buckshot", 64),
    ("Grenade", 5),
    ("Pistol", 256),
    ("SMG1", 256),
    ("XBowBolt", 32),
];

pub fn def(class: &str) -> Option<&'static WeaponDef> {
    WEAPONS.iter().find(|w| w.class == class)
}

/// Per-player weapon state beyond the carried list: clips per weapon, reserve ammo per type and
/// the fire timers of the active weapon (seconds from now; negative once expired, as the game's
/// `GetNextPrimaryFire() - CurTime()` reads).
#[derive(Debug, Clone, Default, PartialEq)]
pub struct Inventory {
    pub clips: BTreeMap<String, [i32; 2]>,
    pub ammo: BTreeMap<String, i32>,
    pub next_primary: f32,
    pub next_secondary: f32,
    /// The pistol parks its primary timer 0.1 s in the past once `soonest_primary` has passed
    /// with the button up (measured: -0.01 then -0.115 instead of -0.025); once per arming.
    pub pistol_relaxed: bool,
    /// Semi-auto: time until a fresh press may fire again.
    pub soonest_primary: f32,
    pub penalty: f32,
    pub need_pump: bool,
    pub reloading: bool,
    pub prev_attack: bool,
    /// Deploying or reloading: a fresh press does not shortcut the primary timer.
    pub busy: bool,
    pub shots: u32,
    /// Ticks since the last shot while an `auto_reload` is due.
    pub since_shot: Option<u32>,
    pub ball_pending: bool,
    /// Frag: ticks since the pull-back press; ticks left until the grenade leaves; a redraw due.
    pub throw_press: Option<u32>,
    pub throw_in: Option<u32>,
    pub redraw_pending: bool,
    /// The projectile the last `command` launched (rocket/bolt in the shot tick, the grenade in
    /// its throw tick, the ball when the AR2's 0.5 s charge expires).
    pub launched: Option<Launch>,
    /// A melee swing happened in this command (`Fire::melee`).
    pub swung: bool,
    /// A `rearm_after_reload` refill happened last tick.
    pub rearm: bool,
    /// Last viewmodel event the client cannot predict from its own buttons, with a serial that
    /// changes on every new one (the snapshot carries both).
    pub vm_act: VmAct,
    pub vm_serial: u8,
    /// Shell-by-shell reload stage: 0 none, 1 start, 2 shells, 3 end.
    pub reload_stage: u8,
    /// A held press on an empty clip without a dry fire arms this on its first expired tick; the
    /// reload starts on the next one (SMG1 held empty: reload 6 ticks after the emptying shot).
    pub fire_on_empty: bool,
}

/// Viewmodel events the server decides: a reload starting (manual, empty clip with the button
/// up, the crossbow's automatic one), the shotgun pump, a dry fire on an empty clip.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
#[repr(u8)]
pub enum VmAct {
    #[default]
    None = 0,
    Reload = 1,
    Pump = 2,
    Dry = 3,
    /// One shell going in (shotgun `reload2`) and the end of a shell reload (`reload3`).
    ReloadShell = 4,
    ReloadEnd = 5,
    /// A primary shot, swing or launch left (other players' `range_*` gestures; the owner's
    /// viewmodel plays its fire from the click).
    Fire = 6,
}

fn vm_event(slot: &mut VmAct, serial: &mut u8, act: VmAct) {
    *slot = act;
    *serial = serial.wrapping_add(1);
}

impl Inventory {
    /// The tool gun's accepted click (the stool took it): the game's `DoShootEffect` plays
    /// the sound, the viewmodel's primary attack and the player's attack gesture only then.
    pub fn tool_fired(&mut self) {
        vm_event(&mut self.vm_act, &mut self.vm_serial, VmAct::Fire);
    }
}

/// Bullets fired by one trigger pull.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Shot {
    pub pellets: u8,
    pub spread: f32,
    pub ammo: &'static str,
}

impl Inventory {
    pub fn give_loadout(&mut self) {
        self.clips.clear();
        self.ammo.clear();
        for w in &WEAPONS {
            self.clips.insert(w.class.to_string(), [w.clip1, w.clip2]);
        }
        for (name, n) in SPAWN_AMMO {
            self.ammo.insert(name.to_string(), n);
        }
    }

    pub fn clear(&mut self) {
        self.clips.clear();
        self.ammo.clear();
    }

    /// Clip state of a carried weapon, `[-1, -1]` for weapons without clips.
    fn start_reload(&mut self, f: Fire) {
        self.reloading = true;
        self.busy = true;
        self.fire_on_empty = false;
        vm_event(&mut self.vm_act, &mut self.vm_serial, VmAct::Reload);
        self.next_primary = match f.shells {
            Some((start, _, _)) => {
                self.reload_stage = 1;
                start
            }
            None => f.reload.unwrap_or(0.0),
        };
        self.next_secondary = self.next_primary;
        self.soonest_primary = self.next_primary;
        self.pistol_relaxed = false;
    }

    pub fn clip(&self, class: &str) -> [i32; 2] {
        self.clips.get(class).copied().unwrap_or([-1, -1])
    }

    /// Selecting a carried weapon makes it active in the same tick and arms its deploy time.
    pub fn select(&mut self, class: &str) {
        if let Some(d) = def(class).and_then(|w| w.deploy) {
            self.next_primary = d;
            self.next_secondary = d;
            self.soonest_primary = d;
        }
        self.pistol_relaxed = false;
        self.need_pump = false;
        self.reloading = false;
        self.reload_stage = 0;
        self.fire_on_empty = false;
        self.busy = true;
    }

    /// A fresh `Give` of a weapon: like `select`, with the measured give timers and reserve.
    pub fn give(&mut self, class: &str) {
        self.select(class);
        if let Some(w) = def(class) {
            if let Some((p, s)) = w.give {
                self.next_primary = p;
                self.next_secondary = s;
                self.soonest_primary = p;
            }
            if w.give_ammo > 0 {
                *self.ammo.entry(w.ammo1.to_string()).or_insert(0) += w.give_ammo;
            }
        }
    }

    /// The usercmd of one tick for the active weapon `class`, before `tick`: reload, pump and
    /// shots as the game resolves them against the timers of the command's tick.
    pub fn command(&mut self, class: &str, buttons: u32) -> Option<Shot> {
        self.launched = None;
        self.swung = false;
        let pressed = buttons & IN_ATTACK != 0;
        let edge = pressed && !self.prev_attack;
        if !pressed {
            self.fire_on_empty = false;
        }
        self.prev_attack = pressed;
        let (Some(w), Some(f)) = (def(class), def(class).and_then(|w| w.fire)) else {
            return None;
        };
        let clip = self
            .clips
            .entry(class.to_string())
            .or_insert([w.clip1, w.clip2]);
        if self.busy && self.next_primary <= 0.0 {
            self.busy = false;
        }
        if self.rearm {
            self.rearm = false;
            self.next_primary = 0.0;
            self.next_secondary = 0.0;
        }
        let pressed2 = buttons & IN_ATTACK2 != 0;
        if let Some(th) = f.throw {
            return self.throw_command(w, th, pressed);
        }
        if let Some(n) = self.since_shot.as_mut() {
            *n += 1;
            if let Some((after, time)) = f.auto_reload {
                if *n >= after {
                    self.since_shot = None;
                    self.reloading = true;
                    vm_event(&mut self.vm_act, &mut self.vm_serial, VmAct::Reload);
                    self.busy = true;
                    self.next_primary = time;
                    self.next_secondary = time;
                    self.soonest_primary = time;
                }
            }
        }
        if let Alt::Ball { delay, cycle } = f.alt {
            let ammo2 = self.ammo.entry(w.ammo2.to_string()).or_insert(0);
            if self.ball_pending {
                if self.next_primary <= 0.0 {
                    self.ball_pending = false;
                    *ammo2 -= 1;
                    self.launched = Some(Launch::Ball);
                    self.next_primary = cycle;
                    self.next_secondary = cycle;
                }
                return None;
            }
            if pressed2 && self.next_primary <= 0.0 && self.next_secondary <= 0.0 && *ammo2 > 0 {
                self.ball_pending = true;
                self.next_primary = delay;
                self.next_secondary = delay;
                return None;
            }
        }
        if let Alt::GrenadeEmpty { cycle } = f.alt {
            if pressed2 && self.next_secondary <= 0.0 {
                let have = self.ammo.entry(w.ammo2.to_string()).or_insert(0);
                if *have == 0 {
                    self.next_secondary = cycle;
                    return None;
                }
                if self.next_primary <= 0.0 {
                    *have -= 1;
                    self.launched = Some(Launch::Smg1Grenade);
                    self.next_primary = SMG1_GRENADE_PRIMARY;
                    self.next_secondary = SMG1_GRENADE_SECONDARY;
                    return None;
                }
            }
        }
        if self.reloading {
            if self.next_primary > 0.0 {
                return None;
            }
            let reserve = self.ammo.entry(w.ammo1.to_string()).or_insert(0);
            if let Some((_, shell, end)) = f.shells {
                match self.reload_stage {
                    3 => {
                        self.reloading = false;
                        self.reload_stage = 0;
                    }
                    2 if pressed && clip[0] > 0 => {
                        // a press at the shell boundary fires instead of loading on
                        self.reloading = false;
                        self.reload_stage = 0;
                    }
                    _ if self.reload_stage == 1 || (clip[0] < w.clip1 && *reserve > 0) => {
                        clip[0] += 1;
                        *reserve -= 1;
                        self.reload_stage = 2;
                        self.next_primary = shell;
                        self.next_secondary = shell;
                        vm_event(&mut self.vm_act, &mut self.vm_serial, VmAct::ReloadShell);
                        return None;
                    }
                    _ => {
                        self.reload_stage = 3;
                        self.next_primary = end;
                        self.next_secondary = end;
                        vm_event(&mut self.vm_act, &mut self.vm_serial, VmAct::ReloadEnd);
                        return None;
                    }
                }
            } else {
                let take = (w.clip1 - clip[0]).min(*reserve).max(0);
                clip[0] += take;
                *reserve -= take;
                self.reloading = false;
                self.rearm = f.rearm_after_reload;
                // both timers restart from the refill (SMG1, AR2, .357: -0.015 on that tick)
                self.next_primary = 0.0;
                self.next_secondary = 0.0;
            }
        }
        if self.need_pump {
            if self.next_primary > 0.0 {
                return None;
            }
            self.need_pump = false;
            self.next_primary = f.pump.unwrap_or(0.0);
            vm_event(&mut self.vm_act, &mut self.vm_serial, VmAct::Pump);
            return None;
        }
        if buttons & IN_RELOAD != 0
            && (f.reload.is_some() || f.shells.is_some())
            && clip[0] < w.clip1
            && self.ammo.get(w.ammo1).copied().unwrap_or(0) > 0
        {
            self.start_reload(f);
            self.next_secondary = self.next_primary;
            self.soonest_primary = self.next_primary;
            self.pistol_relaxed = false;
            return None;
        }
        if clip[0] == 0 {
            if self.next_primary > 0.0 {
                return None;
            }
            // a dry fire while pressed where the weapon has one (pistol); otherwise the empty
            // clip reloads whatever the button (SMG1 held empty reloads after its cycle)
            if let (true, Some((timer, gate))) = (pressed, f.dry_fire) {
                vm_event(&mut self.vm_act, &mut self.vm_serial, VmAct::Dry);
                self.next_primary = timer;
                self.soonest_primary = gate;
                self.pistol_relaxed = false;
            } else if (f.reload.is_some() || f.shells.is_some())
                && self.ammo.get(w.ammo1).copied().unwrap_or(0) > 0
            {
                if pressed && !self.fire_on_empty {
                    self.fire_on_empty = true;
                } else {
                    self.start_reload(f);
                }
            }
            return None;
        }
        if let Alt::Shells { n, pellets, cycle } = f.alt {
            if pressed2 && !pressed && self.next_primary <= 0.0 && clip[0] >= n {
                clip[0] -= n;
                self.next_primary = cycle;
                self.need_pump = f.pump.is_some();
                vm_event(&mut self.vm_act, &mut self.vm_serial, VmAct::Fire);
                self.shots += 1;
                return Some(Shot {
                    pellets,
                    spread: f.spread,
                    ammo: w.ammo1,
                });
            }
        }
        if !pressed {
            return None;
        }
        if w.clip1 < 0 && !w.ammo1.is_empty() && self.ammo.get(w.ammo1).copied().unwrap_or(0) <= 0 {
            return None;
        }
        let fresh_press =
            edge && !self.busy && f.refire_min.is_some_and(|_| self.soonest_primary <= 0.0);
        if !fresh_press && self.next_primary > 0.0 {
            return None;
        }
        // held repeats of an automatic weapon advance from the expired value; a press is from
        // now (SMG1 pressed 4 ticks after its deploy: 0.075 from the press, not from the expiry)
        if fresh_press || edge || !f.auto || f.from_now {
            self.next_primary = f.cycle;
        } else {
            self.next_primary += f.cycle;
        }
        if let Some(r) = f.refire_min {
            self.soonest_primary = r;
        }
        // the penalty accumulates unclamped (18 shots reach 1.65) and is clamped in the cone
        let spread = match f.spread_max {
            Some(m) => f.spread + (m - f.spread) * (self.penalty.min(1.5) / 1.5),
            None => f.spread,
        };
        if f.spread_max.is_some() {
            self.penalty += 0.2;
        }
        if w.clip1 < 0 {
            if !w.ammo1.is_empty() {
                *self.ammo.entry(w.ammo1.to_string()).or_insert(0) -= 1;
            }
        } else {
            clip[0] -= 1;
        }
        if let Some(t) = f.secondary_after {
            self.next_secondary = t;
        }
        if f.auto_reload.is_some() {
            self.since_shot = Some(0);
        }
        self.need_pump = f.pump.is_some();
        self.pistol_relaxed = false;
        self.launched = f.launch;
        self.swung = f.melee;
        vm_event(&mut self.vm_act, &mut self.vm_serial, VmAct::Fire);
        if f.pellets == 0 {
            return None;
        }
        self.shots += 1;
        Some(Shot {
            pellets: f.pellets,
            spread,
            ammo: w.ammo1,
        })
    }

    fn throw_command(&mut self, w: &WeaponDef, th: Throw, pressed: bool) -> Option<Shot> {
        if let Some(n) = self.throw_press.as_mut() {
            *n += 1;
        }
        if let Some(left) = self.throw_in {
            if left > 0 {
                self.throw_in = Some(left - 1);
                return None;
            }
            self.throw_in = None;
            *self.ammo.entry(w.ammo1.to_string()).or_insert(0) -= 1;
            self.launched = Some(Launch::Grenade);
            self.next_primary = th.cycle;
            self.next_secondary = th.cycle;
            self.redraw_pending = true;
            return None;
        }
        if let Some(n) = self.throw_press {
            if !pressed && n >= th.drawback_ticks {
                self.throw_press = None;
                self.throw_in = Some(th.throw_ticks - 1);
            }
            return None;
        }
        if self.redraw_pending {
            if self.next_primary <= 0.0 {
                self.redraw_pending = false;
                self.next_primary = th.redraw;
                self.next_secondary = th.redraw;
            }
            return None;
        }
        if pressed && self.next_primary <= 0.0 && self.ammo.get(w.ammo1).copied().unwrap_or(0) > 0 {
            self.throw_press = Some(0);
            self.next_primary = f32::MAX;
        }
        None
    }

    /// The owner's rocket died: the RPG reloads, both timers to `RPG_ROCKET_DIED`.
    pub fn rocket_died(&mut self) {
        self.next_primary = RPG_ROCKET_DIED;
        self.next_secondary = RPG_ROCKET_DIED;
    }

    /// One server tick for the active weapon `class`.
    pub fn tick(&mut self, class: &str) {
        // the pistol parks its primary timer 0.1 s in the past once the refire gate has passed
        // and the button is up (after a deploy, a reload, a shot or a release), once per arming
        if class == "weapon_pistol"
            && !self.pistol_relaxed
            && !self.reloading
            && self.soonest_primary <= 0.0
            && !self.prev_attack
        {
            self.next_primary = -0.1;
            self.pistol_relaxed = true;
        }
        self.next_primary -= TICK_DT;
        self.next_secondary -= TICK_DT;
        self.soonest_primary -= TICK_DT;
        if self.soonest_primary <= 0.0 && !self.prev_attack {
            self.penalty = (self.penalty - TICK_DT).max(0.0);
        }
    }
}

/// Damage of one bullet of an ammo type on a player (recordings `hit_*`: the multiplayer ammo
/// table, not the `sk_plr_dmg_*` convars the server also carries: pistol 12 against sk 5, SMG1
/// 12 against 4, .357 75 against 40, buckshot 4 per pellet against 8). Unmeasured types are
/// `None` until recorded.
pub fn bullet_damage(ammo: &str) -> Option<f32> {
    match ammo {
        "Pistol" | "SMG1" => Some(12.0),
        "357" => Some(75.0),
        "Buckshot" => Some(4.0),
        _ => None,
    }
}

pub const DMG_BULLET: u32 = 2;
pub const DMG_BUCKSHOT: u32 = 1 << 29;
pub const DMG_ALWAYSGIB: u32 = 1 << 13;

/// Damage type of a bullet by ammo (`hit_*`): pistol and SMG1 4098, .357 8194, buckshot
/// 536875010.
pub fn bullet_dmg_type(ammo: &str) -> u32 {
    match ammo {
        "357" => DMG_BULLET | DMG_ALWAYSGIB,
        "Buckshot" => DMG_BULLET | DMG_NEVERGIB | DMG_BUCKSHOT,
        _ => DMG_BULLET | DMG_NEVERGIB,
    }
}
/// Hit groups as the game numbers them.
pub const HITGROUP_GENERIC: u8 = 0;
pub const HITGROUP_HEAD: u8 = 1;
pub const HITGROUP_CHEST: u8 = 2;
pub const HITGROUP_STOMACH: u8 = 3;

/// The gamemode's damage scale per hit group: the head doubles (12 -> 24, 75 -> 150), chest and
/// stomach are unscaled; limbs not measured yet (a centre-line shot passes between the legs).
pub fn hitgroup_scale(group: u8) -> f32 {
    if group == HITGROUP_HEAD {
        2.0
    } else {
        1.0
    }
}

/// Hit group from the height of the hit above the player's feet, bands measured on the standing
/// `kleiner` model at 200 u: 64 u head, 51 chest, 37 stomach, 71 and 20 (centre line) miss. An
/// interim stand-in for the model's hit boxes.
pub fn hitgroup_at(rel_z: f32) -> Option<u8> {
    if rel_z > 69.0 || rel_z < 26.0 {
        None
    } else if rel_z >= 58.0 {
        Some(HITGROUP_HEAD)
    } else if rel_z >= 44.0 {
        Some(HITGROUP_CHEST)
    } else {
        Some(HITGROUP_STOMACH)
    }
}

/// Knock-back of a bullet hit on a player, horizontal speed per point of (scaled) damage along
/// the aim: 24 damage -> 100.4 u/s, 12 -> 50.1, 150 -> 636 (recordings `hit_*`; linear in the
/// damage, the .357 and shotgun deviate by +1.5 / -2 %, open). The push shows in the same tick.
pub const KNOCKBACK_PER_DAMAGE: f32 = 4.18;

pub const DMG_CRUSH: u32 = 1;
pub const DMG_BLAST: u32 = 64;
pub const DMG_NEVERGIB: u32 = 1 << 12;
pub const DMG_DISSOLVE: u32 = 1 << 26;

/// Explosions (recordings `hit_rpg_floor*`, `hit_rpg_armor`, `hit_frag_down`,
/// `hit_smg1_grenade`): base damage and radius. The damage falls off linearly with the
/// distance from the explosion to the first point of the target's hull along the ray to the
/// target's eye (0 when the explosion is inside the hull: a rocket that hit the player deals
/// the full 150): rocket 150 over 300 u (111.9 at 76.2 u, 86.5 at 127.1, 19.3 at 261.4), frag
/// grenade 150 over 250 (49.3 at 168.0), SMG1 grenade 100 over 250 (98.2 at 4.6).
pub fn explosion(kind: Launch) -> Option<(f32, f32)> {
    match kind {
        Launch::Rocket => Some((150.0, 300.0)),
        Launch::Grenade => Some((150.0, 250.0)),
        Launch::Smg1Grenade => Some((100.0, 250.0)),
        _ => None,
    }
}

/// A crossbow bolt hitting a player: 100 damage as DMG_BULLET | DMG_NEVERGIB, the bolt removed
/// where it stopped (`hit_crossbow`).
pub const BOLT_DAMAGE: f32 = 100.0;
/// Crowbar on a player (`hit_crowbar*`): a ray from the eye along the aim reaching the hull
/// within 75 u (50 u away hits, 110 misses; aimed 20 deg up at 50 u it passes over the hull),
/// 10 damage as DMG_CLUB with the generic hit group in the tick of the swing, every 27 ticks
/// while held. The push is 17.4 u/s for the 10 damage, 10 deg off the aim to the right in the
/// recording (unexplained; applied along the aim). The physcannon does nothing to players.
pub const MELEE_RANGE: f32 = 75.0;
pub const MELEE_DAMAGE: f32 = 10.0;
pub const MELEE_PUSH_PER_DAMAGE: f32 = 1.74;
pub const DMG_CLUB: u32 = 128;
/// Weapons on props (`shoot_prop_*`): bullets deal the single-player `sk_plr_dmg_*` to props
/// (pistol 5, .357 40, buckshot 8 per pellet) with the damage type without NEVERGIB; the damage
/// force is a per-ammo constant along the shot (pistol 668, .357 10906, buckshot 1309 per
/// pellet, crowbar 3000, bolt 21000), applied at the hit point as an impulse (velocity changes
/// by force / mass). Props with health break at zero (wood crate 30); health 0 is unbreakable.
pub fn prop_bullet_damage(ammo: &str) -> Option<f32> {
    match ammo {
        "Pistol" | "SMG1" => Some(5.0),
        "357" => Some(40.0),
        "Buckshot" => Some(8.0),
        "AR2" => Some(8.0),
        _ => None,
    }
}
pub fn prop_bullet_force(ammo: &str) -> f32 {
    match ammo {
        "Pistol" | "SMG1" => 668.0,
        "357" => 10906.0,
        "Buckshot" => 1309.0,
        _ => 668.0,
    }
}
pub fn prop_bullet_dmg_type(ammo: &str) -> u32 {
    match ammo {
        "Buckshot" => DMG_BULLET | DMG_BUCKSHOT,
        _ => DMG_BULLET,
    }
}
pub const MELEE_PROP_FORCE: f32 = 3000.0;
/// Blast force on a prop: 300 per point of (fallen-off) damage, capped at 30000, times the
/// game's random 0.85-1.15 (we use 1), along the blast-to-prop-center direction at the blast.
pub const BLAST_FORCE_PER_DAMAGE: f32 = 300.0;
pub const BLAST_FORCE_MAX: f32 = 30000.0;
/// A combine ball on a prop: 1000 damage every touch; the game's force depends on the ball's
/// contact (31765-39760 measured), we use the lower.
pub const BALL_PROP_DAMAGE: f32 = 1000.0;
pub const BALL_PROP_FORCE: f32 = 31765.0;
pub const BOLT_PROP_FORCE: f32 = 21000.0;

/// Timers set by an SMG1 grenade launch.
pub const SMG1_GRENADE_PRIMARY: f32 = 0.5;
pub const SMG1_GRENADE_SECONDARY: f32 = 1.0;
/// A combine ball hitting a player kills it (damage equal to its health) as
/// DMG_DISSOLVE | DMG_CRUSH and flies on (`hit_ball`: 1000 against 1000 hp).
pub const BALL_DMG_TYPE: u32 = DMG_DISSOLVE | DMG_CRUSH;
/// Knockback of explosions and projectiles: `PUSH_PER_DAMAGE` u/s per point of damage (before
/// armour) from 10 u below the inflictor's position toward the target's centre (origin + 36),
/// vertical part capped at `PUSH_MAX_Z` (measured 250 twice, 134 and 111 uncapped).
pub const PUSH_PER_DAMAGE: f32 = 5.0;
pub const PUSH_MAX_Z: f32 = 250.0;
pub const PUSH_INFLICTOR_DROP: f32 = 10.0;
pub const PLAYER_CENTER_Z: f32 = 36.0;
pub const PLAYER_EYE_Z: f32 = 64.0;

/// A player model's hit boxes in its bind pose, each as a world transform and an AABB in box
/// space; the model faces -y in that pose (kleiner: toes and face toward -y), so a player with
/// view yaw `yaw` carries it rotated by `yaw + 90` degrees about z. The standing idle pose
/// differs from the bind pose by a few units (measured head hit 5.7 u in front of the bind
/// head box), open until animation is read.
/// A bone-name override from a static animation pose: (bone name, position, quaternion).
pub type BonePoseOverride = (String, Option<[f32; 3]>, Option<[f32; 4]>);

#[derive(Debug, Clone, Default)]
pub struct HitModel {
    pub boxes: Vec<(u8, glam::Mat4, glam::Vec3, glam::Vec3)>,
    /// Degrees added to the player's yaw to face the model forward: 90 for kleiner's bind pose
    /// (it faces -y), 0 for a pose from the shared animation model (its root faces +x).
    pub yaw_offset: f32,
}

impl HitModel {
    pub fn from_mdl(data: &[u8], mdl: &gm_formats::mdl::Mdl) -> Self {
        Self::posed(data, mdl, &[])
    }

    /// The hit boxes in the pose of a static animation: `pose` maps bone names to (position,
    /// quaternion) overrides from `gm_formats::mdl::static_pose` of the shared animation model,
    /// bones without an entry keep the bind pose.
    pub fn posed(data: &[u8], mdl: &gm_formats::mdl::Mdl, pose: &[BonePoseOverride]) -> Self {
        use glam::{Mat4, Quat, Vec3};
        let mut world: Vec<Mat4> = Vec::with_capacity(mdl.bones.len());
        for b in &mdl.bones {
            let (mut pos, mut quat) = (b.pos, b.quat);
            if let Some((_, p, q)) = pose.iter().find(|(n, _, _)| *n == b.name) {
                if let Some(p) = p {
                    pos = *p;
                }
                if let Some(q) = q {
                    quat = *q;
                }
            }
            let local = Mat4::from_rotation_translation(
                Quat::from_xyzw(quat[0], quat[1], quat[2], quat[3]),
                Vec3::from(pos),
            );
            let m = if b.parent >= 0 {
                world[b.parent as usize] * local
            } else {
                local
            };
            world.push(m);
        }
        let boxes = gm_formats::mdl::hitboxes(data, &mdl.header)
            .into_iter()
            .filter_map(|h| {
                let m = *world.get(h.bone as usize)?;
                Some((
                    h.group.clamp(0, 255) as u8,
                    m,
                    Vec3::from(h.mins),
                    Vec3::from(h.maxs),
                ))
            })
            .collect();
        HitModel {
            boxes,
            yaw_offset: if pose.is_empty() { 90.0 } else { 0.0 },
        }
    }

    /// The nearest hit box along the ray from `start` to `end` on a model standing at `origin`
    /// with view yaw `yaw`: (fraction, hit group, world position).
    pub fn trace(
        &self,
        origin: glam::Vec3,
        yaw: f32,
        start: glam::Vec3,
        end: glam::Vec3,
    ) -> Option<(f32, u8, glam::Vec3)> {
        use glam::{Mat4, Quat};
        let place = Mat4::from_rotation_translation(
            Quat::from_rotation_z((yaw + self.yaw_offset).to_radians()),
            origin,
        );
        let mut best: Option<(f32, u8)> = None;
        for (group, m, mins, maxs) in &self.boxes {
            let inv = (place * *m).inverse();
            let (s, e) = (inv.transform_point3(start), inv.transform_point3(end));
            let d = e - s;
            let (mut t0, mut t1) = (0.0f32, 1.0f32);
            let mut hit = true;
            for a in 0..3 {
                if d[a].abs() < 1e-6 {
                    if s[a] < mins[a] || s[a] > maxs[a] {
                        hit = false;
                        break;
                    }
                    continue;
                }
                let (mut ta, mut tb) = ((mins[a] - s[a]) / d[a], (maxs[a] - s[a]) / d[a]);
                if ta > tb {
                    std::mem::swap(&mut ta, &mut tb);
                }
                t0 = t0.max(ta);
                t1 = t1.min(tb);
                if t0 > t1 {
                    hit = false;
                    break;
                }
            }
            if hit && best.is_none_or(|b| t0 < b.0) {
                best = Some((t0, *group));
            }
        }
        best.map(|(t, g)| (t, g, start + (end - start) * t))
    }
}

/// The pose of sequence `seq` of a shared animation model (`anim_mdl` / its `.ani` data) as
/// bone-name overrides for `HitModel::posed`; `None` when the sequence is missing or its
/// animation is not a static raw pose.
pub fn sequence_pose(
    anim_mdl_data: &[u8],
    anim_mdl: &gm_formats::mdl::Mdl,
    ani: &[u8],
    seq: &str,
) -> Option<Vec<BonePoseOverride>> {
    let s = gm_formats::mdl::sequences(anim_mdl_data, &anim_mdl.header)
        .into_iter()
        .find(|s| s.name == seq)?;
    let desc = gm_formats::mdl::anim_descs(anim_mdl_data, &anim_mdl.header)
        .into_iter()
        .nth(s.anim.max(0) as usize)?;
    let pose = if desc.block == 0 {
        gm_formats::mdl::static_pose(
            anim_mdl_data,
            anim_mdl.header.local_anim_offset as usize + desc.index as usize,
        )?
    } else {
        let (_, blocks) = gm_formats::mdl::anim_blocks(anim_mdl_data);
        let (start, _) = *blocks.get(desc.block as usize)?;
        gm_formats::mdl::static_pose(ani, start + desc.index as usize)?
    };
    Some(
        pose.into_iter()
            .filter_map(|b| {
                Some((
                    anim_mdl.bones.get(b.bone as usize)?.name.clone(),
                    b.pos,
                    b.quat,
                ))
            })
            .collect(),
    )
}

#[cfg(test)]
mod vm_event_tests {
    use super::*;

    /// One server tick: the command, then the timers.
    fn step(inv: &mut Inventory, class: &str, buttons: u32) -> Option<Shot> {
        let shot = inv.command(class, buttons);
        inv.tick(class);
        shot
    }

    fn settle(inv: &mut Inventory, class: &str) {
        for _ in 0..200 {
            step(inv, class, 0);
        }
    }

    #[test]
    fn shotgun_pumps_once_the_cycle_expired() {
        let class = "weapon_shotgun";
        let mut inv = Inventory::default();
        inv.give_loadout();
        inv.select(class);
        settle(&mut inv, class);
        assert!(
            step(&mut inv, class, IN_ATTACK).is_some(),
            "the shot leaves"
        );
        assert_eq!(inv.vm_act, VmAct::Fire);
        let ticks = 1 + events_until(&mut inv, class, 0, VmAct::Pump);
        // the cycle (0.3333 s) runs out, then the pump starts: 22-23 ticks
        assert!((22..=24).contains(&ticks), "pump after {ticks} ticks");
        assert_eq!(inv.clip(class)[0], 5);
    }

    fn events_until(inv: &mut Inventory, class: &str, buttons: u32, act: VmAct) -> u32 {
        let serial = inv.vm_serial;
        let mut ticks = 0;
        while inv.vm_serial == serial || inv.vm_act != act {
            step(inv, class, buttons);
            ticks += 1;
            assert!(ticks < 400, "no {act:?} event");
        }
        ticks
    }

    #[test]
    fn shotgun_reloads_shell_by_shell_and_a_press_fires_at_the_boundary() {
        let class = "weapon_shotgun";
        let mut inv = Inventory::default();
        inv.give_loadout();
        inv.select(class);
        settle(&mut inv, class);
        for _ in 0..2 {
            assert!(step(&mut inv, class, IN_ATTACK).is_some());
            settle(&mut inv, class);
        }
        assert_eq!(inv.clip(class)[0], 4);
        assert_eq!(step(&mut inv, class, IN_RELOAD), None);
        assert_eq!(inv.vm_act, VmAct::Reload);
        // start 0.5 s, then the first shell goes in
        let t1 = events_until(&mut inv, class, 0, VmAct::ReloadShell);
        assert!((33..=35).contains(&t1), "first shell after {t1} ticks");
        assert_eq!(inv.clip(class)[0], 5);
        // 0.4 s per shell
        let t2 = events_until(&mut inv, class, 0, VmAct::ReloadShell);
        assert!((26..=28).contains(&t2), "second shell after {t2} ticks");
        assert_eq!(inv.clip(class)[0], 6);
        // full: one more shell period, then the end for 0.4333 s, then the weapon is back
        let t3 = events_until(&mut inv, class, 0, VmAct::ReloadEnd);
        assert!((26..=28).contains(&t3), "end after {t3} ticks");
        let mut back = 0;
        while inv.reloading {
            step(&mut inv, class, 0);
            back += 1;
        }
        assert!((28..=30).contains(&back), "idle after {back} ticks");
        assert_eq!(inv.ammo["Buckshot"], 62);

        // a press at the shell boundary fires instead of loading on
        step(&mut inv, class, IN_ATTACK);
        settle(&mut inv, class);
        assert_eq!(step(&mut inv, class, IN_RELOAD), None);
        events_until(&mut inv, class, 0, VmAct::ReloadShell);
        for _ in 0..26 {
            assert_eq!(step(&mut inv, class, 0), None);
        }
        assert!(
            step(&mut inv, class, IN_ATTACK).is_some(),
            "the shot leaves"
        );
        assert!(!inv.reloading);
        assert_eq!(inv.clip(class)[0], 5);
    }

    #[test]
    fn smg1_held_empty_reloads_six_ticks_after_the_last_shot() {
        let class = "weapon_smg1";
        let mut inv = Inventory::default();
        inv.give_loadout();
        inv.select(class);
        settle(&mut inv, class);
        let mut ticks = 0;
        while inv.clip(class)[0] > 0 {
            step(&mut inv, class, IN_ATTACK);
            ticks += 1;
            assert!(ticks < 400);
        }
        let t = events_until(&mut inv, class, IN_ATTACK, VmAct::Reload);
        assert_eq!(t, 6, "reload after {t} ticks");
        // (the recording releases before the refill; held through it is not measured)
        let mut done = 0;
        while inv.reloading {
            step(&mut inv, class, 0);
            done += 1;
        }
        // 1.5 s, the clip is full and both timers restart from the refill
        assert_eq!(done, 101);
        assert_eq!(inv.clip(class)[0], 45);
        assert!(inv.next_primary <= 0.0 && inv.next_primary > -0.03);
    }

    #[test]
    fn revolver_reloads_in_one_go_after_3_6667_s() {
        let class = "weapon_357";
        let mut inv = Inventory::default();
        inv.give_loadout();
        inv.select(class);
        settle(&mut inv, class);
        assert!(step(&mut inv, class, IN_ATTACK).is_some());
        settle(&mut inv, class);
        assert_eq!(step(&mut inv, class, IN_RELOAD), None);
        let mut done = 0;
        while inv.reloading {
            assert_eq!(inv.clip(class)[0], 5, "no shell goes in before the end");
            step(&mut inv, class, 0);
            done += 1;
        }
        assert_eq!(done, 245);
        assert_eq!(inv.clip(class)[0], 6);
    }

    #[test]
    fn pistol_dry_fires_while_held_and_reloads_on_release() {
        let class = "weapon_pistol";
        let mut inv = Inventory::default();
        inv.give_loadout();
        inv.select(class);
        settle(&mut inv, class);
        for _ in 0..17 {
            assert!(step(&mut inv, class, IN_ATTACK).is_some());
            for _ in 0..10 {
                step(&mut inv, class, 0);
            }
        }
        // the last shot with the button kept down: the clip is empty, the cycle (0.5 s) runs
        // out, then a dry fire, and another when its 0.4 s timer expires
        assert!(step(&mut inv, class, IN_ATTACK).is_some());
        assert_eq!(inv.clip(class)[0], 0);
        let serial = inv.vm_serial;
        let mut ticks = 1;
        while inv.vm_serial == serial {
            step(&mut inv, class, IN_ATTACK);
            ticks += 1;
            assert!(ticks < 100, "no dry fire event");
        }
        assert_eq!(inv.vm_act, VmAct::Dry);
        assert!((33..=35).contains(&ticks), "dry fire after {ticks} ticks");
        for _ in 0..30 {
            step(&mut inv, class, IN_ATTACK);
        }
        assert_eq!(
            (inv.vm_act, inv.vm_serial),
            (VmAct::Dry, serial.wrapping_add(2))
        );
        // released: the empty clip reloads from the reserve once the dry fire timer expired
        let mut ticks = 0;
        while inv.vm_act != VmAct::Reload {
            step(&mut inv, class, 0);
            ticks += 1;
            assert!(ticks < 100, "no reload event");
        }
        assert_eq!(inv.vm_serial, serial.wrapping_add(3));
        assert_eq!(
            inv.clip(class)[0],
            0,
            "the clip fills when the reload ends, not at its start"
        );
        settle(&mut inv, class);
        assert_eq!(inv.clip(class)[0], 18);
        assert_eq!(
            inv.vm_serial,
            serial.wrapping_add(3),
            "the refill is not an event"
        );
    }
}
