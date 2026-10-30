//! Client audio: the game's WAV files and soundscripts played through a small mixer.
//!
//! [`Sounds`] loads `scripts/game_sounds_*.txt` from the mounted content and plays a
//! named entry (`Weapon_Pistol.Single`) by picking a wave, volume and pitch as the script
//! says, decoding and caching the WAV, and handing it to the [`Mixer`].

pub mod mixer;
pub mod script;
pub mod wav;

use std::collections::HashMap;
use std::sync::Arc;

use glam::Vec3;

pub use mixer::{Listener, Mixer, Play};
pub use script::{Entry, Scripts};
pub use wav::Pcm;

/// Soundscripts of the base game the client plays from.
pub const SCRIPTS: [&str; 5] = [
    "scripts/game_sounds_weapons.txt",
    "scripts/game_sounds_physics.txt",
    "scripts/game_sounds_player.txt",
    "scripts/game_sounds_items.txt",
    "scripts/game_sounds_world.txt",
];

/// xorshift: picks waves and pitches; seeded from the clock.
struct Rng(u64);

impl Rng {
    fn new() -> Rng {
        let t = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|d| d.as_nanos() as u64)
            .unwrap_or(1);
        Rng(t | 1)
    }

    fn next(&mut self) -> u64 {
        let mut x = self.0;
        x ^= x << 13;
        x ^= x >> 7;
        x ^= x << 17;
        self.0 = x;
        x
    }

    fn unit(&mut self) -> f32 {
        (self.next() >> 40) as f32 / (1u64 << 24) as f32
    }

    fn range(&mut self, (lo, hi): (f32, f32)) -> f32 {
        lo + (hi - lo) * self.unit()
    }
}

pub struct Sounds {
    pub mixer: Mixer,
    pub scripts: Scripts,
    cache: HashMap<String, Option<Arc<Pcm>>>,
    rng: Rng,
}

impl Sounds {
    /// Open the device and load the scripts; missing scripts are skipped.
    pub fn new(mount: &gm_content::Mount) -> Sounds {
        let mut scripts = Scripts::default();
        // the game's manifest (GMod's lists its own `scripts/sounds/*.txt`, with the
        // HL2 ones among them); the base HL2 names as the fallback for a bare install
        let mut paths: Vec<String> = SCRIPTS.iter().map(|s| s.to_string()).collect();
        if let Ok(bytes) = mount.read("scripts/game_sounds_manifest.txt") {
            let text = String::from_utf8_lossy(&bytes);
            paths.extend(
                text.split('"')
                    .filter(|t| t.starts_with("scripts/") && t.ends_with(".txt"))
                    .map(|t| t.to_string()),
            );
        }
        for path in &paths {
            if let Ok(bytes) = mount.read(path) {
                if let Err(e) = scripts.load(&String::from_utf8_lossy(&bytes)) {
                    eprintln!("{path}: {e}");
                }
            }
        }
        Sounds {
            mixer: Mixer::open(),
            scripts,
            cache: HashMap::new(),
            rng: Rng::new(),
        }
    }

    /// Decode `sound/<wave>` once.
    pub fn wave(&mut self, mount: &gm_content::Mount, wave: &str) -> Option<Arc<Pcm>> {
        let key = wave.to_ascii_lowercase();
        if let Some(p) = self.cache.get(&key) {
            return p.clone();
        }
        let pcm = mount
            .read(&format!("sound/{wave}"))
            .ok()
            .and_then(|b| match wav::parse(&b) {
                Ok(p) => Some(Arc::new(p)),
                Err(e) => {
                    eprintln!("sound/{wave}: {e}");
                    None
                }
            });
        self.cache.insert(key, pcm.clone());
        pcm
    }

    /// Play a script entry at `position` (`None`: at the listener). Returns whether the
    /// entry exists and has a decodable wave.
    pub fn emit(&mut self, mount: &gm_content::Mount, name: &str, position: Option<Vec3>) -> bool {
        self.emit_scaled(mount, name, position, 1.0)
    }

    /// `emit` with a factor on the script's volume (a prop impact scales by its speed).
    pub fn emit_scaled(
        &mut self,
        mount: &gm_content::Mount,
        name: &str,
        position: Option<Vec3>,
        scale: f32,
    ) -> bool {
        let Some(e) = self.scripts.get(name).cloned() else {
            return false;
        };
        let wave = &e.waves[(self.rng.next() % e.waves.len() as u64) as usize];
        let Some(pcm) = self.wave(mount, wave) else {
            return false;
        };
        let volume = self.rng.range(e.volume) * scale;
        let pitch = self.rng.range(e.pitch);
        self.mixer.play(Play {
            pcm,
            volume,
            pitch,
            position,
            level: e.level,
        });
        true
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rng_ranges_stay_inside() {
        let mut r = Rng(12345);
        for _ in 0..1000 {
            let p = r.range((98.0, 102.0));
            assert!((98.0..=102.0).contains(&p));
        }
    }
}

#[cfg(test)]
mod content_tests {
    use super::*;

    /// The base game's weapon scripts and waves, when a GMod install is mounted.
    #[test]
    fn weapon_entries_decode_from_the_install() {
        let Some(inst) = gm_content::find_install() else {
            eprintln!("no GMod install, skipped");
            return;
        };
        let mount = gm_content::Mount::for_install(&inst).unwrap();
        let mut s = Sounds::new(&mount);
        assert!(s.scripts.len() > 500, "{} entries", s.scripts.len());
        for name in [
            "Weapon_Pistol.Single",
            "Weapon_357.Single",
            "Weapon_SMG1.Single",
            "Weapon_AR2.Single",
            "Weapon_Shotgun.Single",
            "Weapon_Shotgun.Special1",
            "Weapon_Crossbow.Single",
            "Weapon_RPG.Single",
            "Weapon_Crowbar.Single",
            "Weapon_PhysCannon.Launch",
            "Weapon_Pistol.Empty",
            "Weapon_Pistol.Reload",
            "Weapon_357.Reload",
            "Weapon_SMG1.Reload",
            "Weapon_AR2.Reload",
            "Weapon_Shotgun.Reload",
            "Toolgun.Single",
            "NPC_CScanner.TakePhoto",
        ] {
            let e = s
                .scripts
                .get(name)
                .cloned()
                .unwrap_or_else(|| panic!("{name} missing"));
            for w in &e.waves {
                // the .357 reload entry names a wave the content does not ship: silent
                let Some(pcm) = s.wave(&mount, w) else {
                    assert_eq!(name, "Weapon_357.Reload", "{name}: {w} undecodable");
                    continue;
                };
                assert!(
                    pcm.duration() > 0.01 && pcm.duration() < 10.0,
                    "{w}: {} s",
                    pcm.duration()
                );
            }
        }
        let p = s.scripts.get("Weapon_Pistol.Single").unwrap();
        assert_eq!(
            (p.volume, p.pitch, p.level),
            ((0.55, 0.55), (98.0, 102.0), 140.0)
        );
    }
}
