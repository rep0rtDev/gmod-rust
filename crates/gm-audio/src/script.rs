//! Soundscripts (`scripts/game_sounds_*.txt`, KeyValues): a named entry picks one of its
//! `wave`s at random (`rndwave`) and plays it at a volume, a pitch (percent, a fixed value
//! or a `lo,hi` range) and a sound level (dB at the source, which sets how far it carries).

use std::collections::HashMap;

use gm_formats::keyvalues::{self, Node, Value};

#[derive(Debug, Clone, PartialEq)]
pub struct Entry {
    pub waves: Vec<String>,
    pub volume: (f32, f32),
    pub pitch: (f32, f32),
    /// Sound level in dB; 0 means no attenuation with distance.
    pub level: f32,
}

impl Default for Entry {
    fn default() -> Self {
        Entry {
            waves: Vec::new(),
            volume: (1.0, 1.0),
            pitch: (100.0, 100.0),
            level: SNDLVL_NORM,
        }
    }
}

/// `SNDLVL_NORM`, the script default (75 dB).
pub const SNDLVL_NORM: f32 = 75.0;

/// Named sound levels the scripts use (`SNDLVL_*` in the SDK's `soundflags.h`).
fn named_level(s: &str) -> Option<f32> {
    Some(match s.to_ascii_uppercase().as_str() {
        "SNDLVL_NONE" => 0.0,
        "SNDLVL_IDLE" => 60.0,
        "SNDLVL_TALKING" | "SNDLVL_NORM" => 75.0,
        "SNDLVL_STATIC" => 66.0,
        "SNDLVL_GUNFIRE" => 140.0,
        _ => return None,
    })
}

/// `SNDLVL_90dB`, `SNDLVL_105dB`, `85`, `SNDLVL_GUNFIRE`...
pub fn parse_level(s: &str) -> Option<f32> {
    let s = s.trim();
    if let Some(l) = named_level(s) {
        return Some(l);
    }
    let digits = s
        .strip_prefix("SNDLVL_")
        .or_else(|| s.strip_prefix("sndlvl_"))
        .unwrap_or(s);
    let digits = digits.trim_end_matches(['d', 'D', 'b', 'B']);
    digits.parse().ok()
}

/// `0.7` or `0.5,1.0`; names like `PITCH_NORM` / `VOL_NORM` take their SDK values.
pub fn parse_range(s: &str, default: f32) -> (f32, f32) {
    let s = s.trim();
    let named = match s.to_ascii_uppercase().as_str() {
        "VOL_NORM" => Some(1.0),
        "PITCH_NORM" => Some(100.0),
        "PITCH_LOW" => Some(95.0),
        "PITCH_HIGH" => Some(120.0),
        _ => None,
    };
    if let Some(v) = named {
        return (v, v);
    }
    let mut it = s.split(',').map(|p| p.trim().parse::<f32>().ok());
    let lo = it.next().flatten().unwrap_or(default);
    let hi = it.next().flatten().unwrap_or(lo);
    (lo.min(hi), lo.max(hi))
}

/// Strip the engine's sound-character prefixes (`^` distance variant, `*` stream, `)`
/// spatial, `#` no DSP, `>` doppler, `<` ...) from a wave path.
pub fn wave_path(w: &str) -> &str {
    w.trim_start_matches(|c: char| {
        matches!(
            c,
            '^' | '*' | ')' | '(' | '#' | '>' | '<' | '}' | '$' | '!' | '?' | '&' | '~' | '`' | '@'
        )
    })
}

fn entry(node: &Node) -> Option<Entry> {
    let Value::Block(kids) = &node.value else {
        return None;
    };
    let mut e = Entry::default();
    for k in kids {
        let key = k.key.to_ascii_lowercase();
        match (key.as_str(), &k.value) {
            ("wave", Value::Str(w)) => e.waves.push(wave_path(w).to_string()),
            ("rndwave", Value::Block(ws)) => {
                e.waves.extend(ws.iter().filter_map(|w| match &w.value {
                    Value::Str(s) if w.key.eq_ignore_ascii_case("wave") => {
                        Some(wave_path(s).to_string())
                    }
                    _ => None,
                }))
            }
            ("volume", Value::Str(v)) => e.volume = parse_range(v, 1.0),
            ("pitch", Value::Str(v)) => e.pitch = parse_range(v, 100.0),
            ("soundlevel", Value::Str(v)) => e.level = parse_level(v).unwrap_or(SNDLVL_NORM),
            _ => {}
        }
    }
    (!e.waves.is_empty()).then_some(e)
}

/// Every entry of every loaded script, by lower-cased name.
#[derive(Debug, Default, Clone)]
pub struct Scripts {
    entries: HashMap<String, Entry>,
}

impl Scripts {
    pub fn load(&mut self, src: &str) -> anyhow::Result<usize> {
        let doc = keyvalues::parse(src)?;
        let mut n = 0;
        for node in &doc.nodes {
            if let Some(e) = entry(node) {
                self.entries.insert(node.key.to_ascii_lowercase(), e);
                n += 1;
            }
        }
        Ok(n)
    }

    pub fn get(&self, name: &str) -> Option<&Entry> {
        self.entries.get(&name.to_ascii_lowercase())
    }

    pub fn len(&self) -> usize {
        self.entries.len()
    }

    pub fn is_empty(&self) -> bool {
        self.entries.is_empty()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_waves_ranges_and_levels() {
        let mut s = Scripts::default();
        let n = s
            .load(
                r#"
"Weapon_Pistol.Single"
{
    "channel"   "CHAN_WEAPON"
    "volume"    "0.55"
    "soundlevel" "SNDLVL_GUNFIRE"
    "pitch"     "98,102"
    "wave"      "weapons/pistol/pistol_fire2.wav"
}
"Weapon_357.Single"
{
    "volume" "0.93"
    "soundlevel" "SNDLVL_105dB"
    "rndwave"
    {
        "wave" "^weapons/357/357_fire2.wav"
        "wave" "weapons/357/357_fire3.wav"
    }
}
"Empty.Entry" { "volume" "1" }
"#,
            )
            .unwrap();
        assert_eq!(n, 2);
        let p = s.get("weapon_pistol.single").unwrap();
        assert_eq!(p.waves, ["weapons/pistol/pistol_fire2.wav"]);
        assert_eq!(p.volume, (0.55, 0.55));
        assert_eq!(p.pitch, (98.0, 102.0));
        assert_eq!(p.level, 140.0);
        let r = s.get("Weapon_357.Single").unwrap();
        assert_eq!(r.waves.len(), 2);
        assert_eq!(
            r.waves[0], "weapons/357/357_fire2.wav",
            "sound chars stripped"
        );
        assert_eq!(r.level, 105.0);
        assert!(s.get("Empty.Entry").is_none());
    }

    #[test]
    fn level_and_range_forms() {
        assert_eq!(parse_level("SNDLVL_90dB"), Some(90.0));
        assert_eq!(parse_level("85"), Some(85.0));
        assert_eq!(parse_level("SNDLVL_NONE"), Some(0.0));
        assert_eq!(parse_range("0.5, 1.0", 1.0), (0.5, 1.0));
        assert_eq!(parse_range("PITCH_NORM", 100.0), (100.0, 100.0));
        assert_eq!(parse_range("junk", 0.7), (0.7, 0.7));
    }
}
