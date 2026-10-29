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
}

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
        deploy: None,
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
    /// The pistol snaps its primary timer to 0.1 s in the past once its deploy has expired
    /// (measured: -0.01 then -0.115 instead of -0.025); done once per selection.
    pub pistol_relaxed: bool,
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
        }
        self.pistol_relaxed = false;
    }

    /// One server tick for the active weapon `class`.
    pub fn tick(&mut self, class: &str) {
        if class == "weapon_pistol" && self.next_primary < 0.0 && !self.pistol_relaxed {
            self.next_primary = -0.1;
            self.pistol_relaxed = true;
        }
        self.next_primary -= TICK_DT;
        self.next_secondary -= TICK_DT;
    }
}
