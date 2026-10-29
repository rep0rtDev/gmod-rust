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
    /// Reload time after IN_RELOAD with a non-full clip; the clip fills when it expires.
    pub reload: Option<f32>,
    /// Empty clip and the button down once the primary timer expired: (primary timer, refire
    /// gate) of the dry fire (pistol 0.4, 0.2). An empty clip with the button up reloads.
    pub dry_fire: Option<(f32, f32)>,
}

pub const IN_ATTACK: u32 = 1;
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
};
const SMG1: Fire = Fire {
    auto: true,
    cycle: 0.075,
    refire_min: None,
    pellets: 1,
    spread: 0.0436,
    spread_max: None,
    pump: None,
    reload: None,
    dry_fire: None,
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
    dry_fire: None,
};
const REVOLVER: Fire = Fire {
    auto: false,
    cycle: 0.75,
    refire_min: None,
    pellets: 1,
    spread: 0.0,
    spread_max: None,
    pump: None,
    reload: None,
    dry_fire: None,
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
        deploy: None,
        fire: None,
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
        deploy: None,
        fire: None,
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
        fire: None,
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
        deploy: None,
        fire: None,
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
        fire: None,
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
        fire: None,
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
        self.busy = true;
    }

    /// The usercmd of one tick for the active weapon `class`, before `tick`: reload, pump and
    /// shots as the game resolves them against the timers of the command's tick.
    pub fn command(&mut self, class: &str, buttons: u32) -> Option<Shot> {
        let pressed = buttons & IN_ATTACK != 0;
        let edge = pressed && !self.prev_attack;
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
        if self.reloading {
            if self.next_primary > 0.0 {
                return None;
            }
            let reserve = self.ammo.entry(w.ammo1.to_string()).or_insert(0);
            let take = (w.clip1 - clip[0]).min(*reserve).max(0);
            clip[0] += take;
            *reserve -= take;
            self.reloading = false;
        }
        if self.need_pump {
            if self.next_primary > 0.0 {
                return None;
            }
            self.need_pump = false;
            self.next_primary = f.pump.unwrap_or(0.0);
            return None;
        }
        if buttons & IN_RELOAD != 0
            && f.reload.is_some()
            && clip[0] < w.clip1
            && self.ammo.get(w.ammo1).copied().unwrap_or(0) > 0
        {
            self.reloading = true;
            self.busy = true;
            self.next_primary = f.reload.unwrap_or(0.0);
            self.next_secondary = self.next_primary;
            self.soonest_primary = self.next_primary;
            self.pistol_relaxed = false;
            return None;
        }
        if clip[0] == 0 {
            if self.next_primary > 0.0 {
                return None;
            }
            if pressed {
                if let Some((timer, gate)) = f.dry_fire {
                    self.next_primary = timer;
                    self.soonest_primary = gate;
                    self.pistol_relaxed = false;
                }
            } else if f.reload.is_some() && self.ammo.get(w.ammo1).copied().unwrap_or(0) > 0 {
                self.reloading = true;
                self.busy = true;
                self.next_primary = f.reload.unwrap_or(0.0);
                self.next_secondary = self.next_primary;
                self.soonest_primary = self.next_primary;
                self.pistol_relaxed = false;
            }
            return None;
        }
        if !pressed {
            return None;
        }
        let fresh_press =
            edge && !self.busy && f.refire_min.is_some_and(|_| self.soonest_primary <= 0.0);
        if !fresh_press && self.next_primary > 0.0 {
            return None;
        }
        if fresh_press || !f.auto {
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
        clip[0] -= 1;
        self.need_pump = f.pump.is_some();
        self.pistol_relaxed = false;
        self.shots += 1;
        Some(Shot {
            pellets: f.pellets,
            spread,
            ammo: w.ammo1,
        })
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
