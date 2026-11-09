//! Soundscapes: the ambience the map asks for. `scripts/soundscapes_manifest.txt` lists
//! the files; each names soundscapes with `playlooping` (a loop at a position or at the
//! listener), `playrandom` (a wave from `rndwave` every `time` seconds) and
//! `playsoundscape` (another soundscape mixed in at a volume, its positions shifted by
//! `positionoverride`). The map's `env_soundscape` entities pick one: the nearest whose
//! `radius` (-1: unbounded) holds the listener and whose origin sees the listener's eyes;
//! it stays until another takes over. A change fades the old loops out and the new ones in
//! over `fadetime` (3 s unless the soundscape says otherwise).
//!
//! `dsp` is parsed and ignored: our room sound is Steam Audio's reflections off the real
//! geometry, not a preset. `position "random"` is a point 36 u from the listener at a
//! random bearing (sound.md §6.2); a random sound first fires at half its interval.

use std::collections::HashMap;
use std::time::Instant;

use glam::Vec3;
use gm_formats::bsp::Bsp;
use gm_formats::keyvalues::{self, Node, Value};

use crate::engine::Play;
use crate::script::{parse_level, parse_range, wave_path};
use crate::Sounds;

/// `soundscape_fadetime`.
pub const FADE_TIME: f32 = 3.0;

/// Where a soundscape sound plays.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum Position {
    /// At the listener, following it.
    Listener,
    /// `position0`..`position7` of the `env_soundscape`.
    Index(u8),
    /// A random spot around the listener each time.
    Random,
}

#[derive(Debug, Clone, PartialEq)]
pub struct Loop {
    pub wave: String,
    pub volume: (f32, f32),
    pub pitch: (f32, f32),
    pub level: f32,
    pub position: Position,
}

#[derive(Debug, Clone, PartialEq)]
pub struct Random {
    pub waves: Vec<String>,
    pub time: (f32, f32),
    pub volume: (f32, f32),
    pub pitch: (f32, f32),
    pub level: f32,
    pub position: Position,
}

#[derive(Debug, Clone, PartialEq)]
pub struct Sub {
    pub name: String,
    pub volume: f32,
    /// Added to the sub-soundscape's position indices.
    pub position_offset: u8,
}

#[derive(Debug, Clone, Default, PartialEq)]
pub struct Definition {
    pub dsp: i32,
    pub fade_time: Option<f32>,
    pub loops: Vec<Loop>,
    pub randoms: Vec<Random>,
    pub subs: Vec<Sub>,
}

fn position_of(node: &Node, key: &str) -> Position {
    match node.get_str(key).map(|s| s.trim().to_ascii_lowercase()) {
        None => Position::Listener,
        Some(s) if s == "random" => Position::Random,
        Some(s) => s
            .parse::<u8>()
            .ok()
            .filter(|&i| i < 8)
            .map_or(Position::Listener, Position::Index),
    }
}

/// `attenuation` (the SDK's `ATTN_*`, 0 = none) or `soundlevel`; 75 dB without either.
fn level_of(node: &Node) -> f32 {
    if let Some(a) = node
        .get_str("attenuation")
        .and_then(|a| a.trim().parse::<f32>().ok())
    {
        return if a <= 0.0 { 0.0 } else { 50.0 + 20.0 / a };
    }
    node.get_str("soundlevel")
        .and_then(parse_level)
        .unwrap_or(crate::script::SNDLVL_NORM)
}

fn range_of(node: &Node, key: &str, default: f32) -> (f32, f32) {
    node.get_str(key)
        .map_or((default, default), |v| parse_range(v, default))
}

fn definition(node: &Node) -> Definition {
    let mut d = Definition::default();
    for child in node.children() {
        let key = child.key.to_ascii_lowercase();
        match (key.as_str(), &child.value) {
            ("dsp", Value::Str(v)) => d.dsp = v.trim().parse().unwrap_or(0),
            ("fadetime", Value::Str(v)) => d.fade_time = v.trim().parse().ok(),
            ("playlooping", Value::Block(_)) => {
                if let Some(w) = child.get_str("wave") {
                    d.loops.push(Loop {
                        wave: wave_path(w).to_string(),
                        volume: range_of(child, "volume", 1.0),
                        pitch: range_of(child, "pitch", 100.0),
                        level: level_of(child),
                        position: position_of(child, "position"),
                    });
                }
            }
            ("playrandom", Value::Block(_)) => {
                let waves: Vec<String> = child
                    .get("rndwave")
                    .map(|r| {
                        r.children()
                            .iter()
                            .filter_map(|w| match &w.value {
                                Value::Str(s) if w.key.eq_ignore_ascii_case("wave") => {
                                    Some(wave_path(s).to_string())
                                }
                                _ => None,
                            })
                            .collect()
                    })
                    .unwrap_or_default();
                if !waves.is_empty() {
                    d.randoms.push(Random {
                        waves,
                        time: range_of(child, "time", 10.0),
                        volume: range_of(child, "volume", 1.0),
                        pitch: range_of(child, "pitch", 100.0),
                        level: level_of(child),
                        position: position_of(child, "position"),
                    });
                }
            }
            ("playsoundscape", Value::Block(_)) => {
                if let Some(name) = child.get_str("name") {
                    d.subs.push(Sub {
                        name: name.to_ascii_lowercase(),
                        volume: child
                            .get_str("volume")
                            .map_or(1.0, |v| parse_range(v, 1.0).0),
                        position_offset: child
                            .get_str("positionoverride")
                            .and_then(|p| p.trim().parse().ok())
                            .unwrap_or(0),
                    });
                }
            }
            _ => {}
        }
    }
    d
}

/// Every soundscape of the loaded files, by lower-cased name.
#[derive(Debug, Default, Clone)]
pub struct Definitions {
    map: HashMap<String, Definition>,
}

impl Definitions {
    /// Parse one soundscape file; the first file to define a name keeps it.
    pub fn load(&mut self, src: &str) -> anyhow::Result<usize> {
        let doc = keyvalues::parse(src)?;
        let mut n = 0;
        for node in &doc.nodes {
            if matches!(node.value, Value::Block(_)) {
                self.map
                    .entry(node.key.to_ascii_lowercase())
                    .or_insert_with(|| definition(node));
                n += 1;
            }
        }
        Ok(n)
    }

    /// The manifest's files (plus `scripts/soundscapes.txt` first) from the mount.
    pub fn from_mount(mount: &gm_content::Mount) -> Definitions {
        let mut defs = Definitions::default();
        let mut files = vec!["scripts/soundscapes.txt".to_string()];
        if let Ok(m) = mount.read_string("scripts/soundscapes_manifest.txt") {
            files.extend(
                m.split('"')
                    .filter(|t| t.starts_with("scripts/") && t.ends_with(".txt"))
                    .map(|t| t.to_string()),
            );
        }
        for f in files {
            if let Ok(src) = mount.read_string(&f) {
                if let Err(e) = defs.load(&src) {
                    eprintln!("{f}: {e}");
                }
            }
        }
        defs
    }

    pub fn get(&self, name: &str) -> Option<&Definition> {
        self.map.get(&name.to_ascii_lowercase())
    }

    pub fn len(&self) -> usize {
        self.map.len()
    }

    pub fn is_empty(&self) -> bool {
        self.map.is_empty()
    }

    /// The soundscape with its `playsoundscape` children folded in (volumes multiplied,
    /// position indices shifted), at most four levels deep.
    pub fn flatten(&self, name: &str) -> Option<Definition> {
        let mut out = Definition::default();
        self.flatten_into(name, 1.0, 0, 0, &mut out)?;
        Some(out)
    }

    fn flatten_into(
        &self,
        name: &str,
        volume: f32,
        offset: u8,
        depth: u8,
        out: &mut Definition,
    ) -> Option<()> {
        let d = self.get(name)?;
        if depth == 0 {
            out.dsp = d.dsp;
            out.fade_time = d.fade_time;
        }
        let shift = |p: Position| match p {
            Position::Index(i) => Position::Index((i + offset).min(7)),
            p => p,
        };
        for l in &d.loops {
            out.loops.push(Loop {
                volume: (l.volume.0 * volume, l.volume.1 * volume),
                position: shift(l.position),
                ..l.clone()
            });
        }
        for r in &d.randoms {
            out.randoms.push(Random {
                volume: (r.volume.0 * volume, r.volume.1 * volume),
                position: shift(r.position),
                ..r.clone()
            });
        }
        if depth < 4 {
            for s in &d.subs {
                let _ = self.flatten_into(
                    &s.name,
                    volume * s.volume,
                    offset.saturating_add(s.position_offset),
                    depth + 1,
                    out,
                );
            }
        }
        Some(())
    }
}

/// An `env_soundscape` of the map.
#[derive(Debug, Clone, PartialEq)]
pub struct Env {
    pub origin: Vec3,
    /// Negative: unbounded.
    pub radius: f32,
    pub name: String,
    pub positions: [Option<Vec3>; 8],
}

impl Env {
    /// The map's `env_soundscape` entities with their `position0..7` targets resolved.
    pub fn from_bsp(bsp: &Bsp<'_>) -> Vec<Env> {
        Env::from_entities(&bsp.entities)
    }

    pub fn from_entities(entities: &[gm_formats::bsp::lumps::Entity]) -> Vec<Env> {
        let mut out = Vec::new();
        for e in entities {
            if e.get("classname") != Some("env_soundscape") {
                continue;
            }
            let (Some(name), Some(origin)) = (e.get("soundscape"), e.origin()) else {
                continue;
            };
            let mut positions = [None; 8];
            for (i, slot) in positions.iter_mut().enumerate() {
                let Some(target) = e.get(&format!("position{i}")) else {
                    continue;
                };
                *slot = entities
                    .iter()
                    .find(|t| t.get("targetname") == Some(target))
                    .and_then(|t| t.origin())
                    .map(Vec3::from);
            }
            out.push(Env {
                origin: Vec3::from(origin),
                radius: e
                    .get("radius")
                    .and_then(|r| r.trim().parse().ok())
                    .unwrap_or(-1.0),
                name: name.to_ascii_lowercase(),
                positions,
            });
        }
        out
    }
}

struct LoopVoice {
    id: u32,
    volume: f32,
    position: Position,
}

struct RandomTimer {
    next: f32,
}

/// Which soundscape plays and its voices.
struct Active {
    env: usize,
    def: Definition,
    loops: Vec<LoopVoice>,
    timers: Vec<RandomTimer>,
    /// Fade-in progress 0..1 and its length.
    fade: f32,
    fade_len: f32,
}

/// The running soundscape system of a map.
#[derive(Default)]
pub struct Soundscapes {
    pub defs: Definitions,
    pub envs: Vec<Env>,
    active: Option<Active>,
    clock: Option<Instant>,
    time: f32,
    missing: Vec<String>,
    rng: u64,
    /// The last `env_soundscape` that took over (index into `envs`).
    pub current: Option<usize>,
}

/// Entity id space for soundscape voices (above any networked entity).
const ENT_BASE: u32 = 0xFFFF_0000;

impl Soundscapes {
    pub fn new(defs: Definitions, envs: Vec<Env>) -> Soundscapes {
        Soundscapes {
            defs,
            envs,
            rng: 0x9E37_79B9_7F4A_7C15,
            ..Default::default()
        }
    }

    fn unit(&mut self) -> f32 {
        self.rng ^= self.rng << 13;
        self.rng ^= self.rng >> 7;
        self.rng ^= self.rng << 17;
        (self.rng >> 40) as f32 / (1u64 << 24) as f32
    }

    fn range(&mut self, r: (f32, f32)) -> f32 {
        r.0 + (r.1 - r.0) * self.unit()
    }

    /// Pick the `env_soundscape` for a listener at `eye`: the nearest in radius whose
    /// origin sees the eye (`sees(from, to)`), or `None` to keep the current one.
    pub fn pick(&self, eye: Vec3, sees: &dyn Fn(Vec3, Vec3) -> bool) -> Option<usize> {
        let mut best: Option<(usize, f32)> = None;
        for (i, e) in self.envs.iter().enumerate() {
            let d = e.origin.distance(eye);
            if e.radius >= 0.0 && d > e.radius {
                continue;
            }
            if best.is_some_and(|(_, bd)| bd <= d) {
                continue;
            }
            if sees(e.origin, eye) {
                best = Some((i, d));
            }
        }
        best.map(|(i, _)| i)
    }

    /// Soundscapes the map named that no file defines (reported once each).
    pub fn missing(&self) -> &[String] {
        &self.missing
    }

    /// Per frame: choose, start, fade, fire random sounds, follow the listener.
    pub fn update(
        &mut self,
        sounds: &mut Sounds,
        mount: &gm_content::Mount,
        eye: Vec3,
        sees: &dyn Fn(Vec3, Vec3) -> bool,
    ) {
        let now = Instant::now();
        let dt = match self.clock {
            Some(t) => now.duration_since(t).as_secs_f32().min(0.25),
            None => 0.0,
        };
        self.clock = Some(now);
        self.time += dt;

        if let Some(i) = self.pick(eye, sees) {
            if self.current != Some(i) {
                self.switch(sounds, mount, i, eye);
            }
        }
        let Some(active) = &mut self.active else {
            return;
        };
        if active.fade < 1.0 {
            active.fade = (active.fade + dt / active.fade_len.max(1e-3)).min(1.0);
            let f = active.fade;
            for l in &active.loops {
                sounds.mixer.set_volume(l.id, l.volume * f);
            }
        }
        for l in &active.loops {
            if l.position == Position::Listener {
                sounds.mixer.move_voice(l.id, eye);
            }
        }
        let env = self.envs[active.env].clone();
        let time = self.time;
        let due: Vec<usize> = active
            .timers
            .iter()
            .enumerate()
            .filter(|(_, t)| t.next <= time)
            .map(|(i, _)| i)
            .collect();
        let randoms: Vec<Random> = due.iter().map(|&i| active.def.randoms[i].clone()).collect();
        let fade = active.fade;
        for (i, r) in due.into_iter().zip(randoms) {
            let wave =
                r.waves[(self.unit() * r.waves.len() as f32) as usize % r.waves.len()].clone();
            let volume = self.range(r.volume) * fade;
            let pitch = self.range(r.pitch);
            let position = self.place(r.position, &env, eye);
            if let Some(pcm) = sounds.wave(mount, &wave) {
                sounds.mixer.play(Play {
                    pcm,
                    volume,
                    pitch,
                    position,
                    level: r.level,
                    looping: false,
                    ent: 0,
                    channel: 0,
                    stream: false,
                });
            }
            let next = self.range(r.time);
            if let Some(a) = &mut self.active {
                a.timers[i].next = self.time + next.max(0.1);
            }
        }
    }

    fn place(&mut self, p: Position, env: &Env, eye: Vec3) -> Option<Vec3> {
        match p {
            Position::Listener => None,
            Position::Index(i) => env.positions[i as usize].or(Some(env.origin)),
            Position::Random => {
                let a = self.unit() * std::f32::consts::TAU;
                let r = 36.0;
                Some(eye + Vec3::new(a.cos() * r, a.sin() * r, 0.0))
            }
        }
    }

    fn switch(
        &mut self,
        sounds: &mut Sounds,
        mount: &gm_content::Mount,
        env_index: usize,
        eye: Vec3,
    ) {
        let name = self.envs[env_index].name.clone();
        let Some(def) = self.defs.flatten(&name) else {
            if !self.missing.contains(&name) {
                eprintln!("soundscape {name}: not defined");
                self.missing.push(name);
            }
            self.current = Some(env_index);
            return;
        };
        let fade_len = def.fade_time.unwrap_or(FADE_TIME).max(0.01);
        if let Some(old) = self.active.take() {
            let ent = ENT_BASE + old.env as u32;
            sounds.mixer.stop_channel(ent, None, fade_len);
        }
        let env = self.envs[env_index].clone();
        let ent = ENT_BASE + env_index as u32;
        let mut loops = Vec::new();
        for (k, l) in def.loops.iter().enumerate() {
            let Some(pcm) = sounds.wave(mount, &l.wave) else {
                continue;
            };
            let volume = self.range(l.volume);
            let pitch = self.range(l.pitch);
            let position = match l.position {
                Position::Listener => Some(eye),
                p => self.place(p, &env, eye),
            };
            let id = sounds.mixer.play(Play {
                pcm,
                volume: 0.0,
                pitch,
                position,
                level: if l.position == Position::Listener {
                    0.0
                } else {
                    l.level
                },
                looping: true,
                ent,
                channel: 7 + k as u8,
                stream: false,
            });
            loops.push(LoopVoice {
                id,
                volume,
                position: l.position,
            });
        }
        let timers = def
            .randoms
            .iter()
            .map(|r| RandomTimer {
                next: self.time + 0.5 * self.range(r.time),
            })
            .collect::<Vec<_>>();
        self.active = Some(Active {
            env: env_index,
            def,
            loops,
            timers,
            fade: 0.0,
            fade_len,
        });
        self.current = Some(env_index);
    }

    /// Stop everything (map change).
    pub fn clear(&mut self, sounds: &mut Sounds) {
        if let Some(old) = self.active.take() {
            sounds
                .mixer
                .stop_channel(ENT_BASE + old.env as u32, None, 0.0);
        }
        self.current = None;
    }

    /// Loops playing for the active soundscape.
    pub fn active_loops(&self) -> usize {
        self.active.as_ref().map_or(0, |a| a.loops.len())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const SRC: &str = r#"
"Test.Outside"
{
    "dsp" "19"
    "fadetime" "2"
    "playlooping"
    {
        "volume" "0.25"
        "pitch" "100"
        "attenuation" "0"
        "wave" "ambient/atmosphere/outdoor.wav"
    }
    "playlooping"
    {
        "volume" "0.5,0.8"
        "pitch" "95,105"
        "soundlevel" "SNDLVL_80dB"
        "position" "1"
        "wave" "ambient/water/drip.wav"
    }
    "playrandom"
    {
        "time" "10,20"
        "volume" "0.3"
        "pitch" "100"
        "position" "random"
        "attenuation" "0.8"
        "rndwave"
        {
            "wave" "ambient/animal/bird1.wav"
            "wave" "ambient/animal/bird2.wav"
        }
    }
    "playsoundscape"
    {
        "name" "Test.Sub"
        "volume" "0.5"
        "positionoverride" "2"
    }
}
"Test.Sub"
{
    "playlooping"
    {
        "volume" "1"
        "position" "0"
        "wave" "ambient/machines/hum.wav"
    }
}
"#;

    #[test]
    fn parses_loops_randoms_and_subs() {
        let mut d = Definitions::default();
        assert_eq!(d.load(SRC).unwrap(), 2);
        let s = d.get("test.outside").unwrap();
        assert_eq!((s.dsp, s.fade_time), (19, Some(2.0)));
        assert_eq!(s.loops.len(), 2);
        assert_eq!(s.loops[0].level, 0.0, "attenuation 0 = no falloff");
        assert_eq!(s.loops[0].position, Position::Listener);
        assert_eq!(s.loops[1].position, Position::Index(1));
        assert_eq!((s.loops[1].level, s.loops[1].volume), (80.0, (0.5, 0.8)));
        assert_eq!(s.randoms[0].waves.len(), 2);
        assert_eq!(s.randoms[0].position, Position::Random);
        assert!(
            (s.randoms[0].level - 75.0).abs() < 1e-3,
            "ATTN_NORM 0.8 = 75 dB"
        );
        let f = d.flatten("Test.Outside").unwrap();
        assert_eq!(f.loops.len(), 3);
        assert_eq!(
            f.loops[2].position,
            Position::Index(2),
            "sub shifted by positionoverride"
        );
        assert_eq!(f.loops[2].volume, (0.5, 0.5), "sub volume multiplied");
    }

    fn envs() -> Vec<Env> {
        vec![
            Env {
                origin: Vec3::ZERO,
                radius: -1.0,
                name: "outside".into(),
                positions: [None; 8],
            },
            Env {
                origin: Vec3::new(1000.0, 0.0, 0.0),
                radius: 300.0,
                name: "inside".into(),
                positions: [None; 8],
            },
        ]
    }

    #[test]
    fn picks_the_nearest_in_radius_with_line_of_sight() {
        let s = Soundscapes::new(Definitions::default(), envs());
        let clear = |_: Vec3, _: Vec3| true;
        assert_eq!(
            s.pick(Vec3::new(5000.0, 0.0, 0.0), &clear),
            Some(0),
            "unbounded one"
        );
        assert_eq!(
            s.pick(Vec3::new(1100.0, 0.0, 0.0), &clear),
            Some(1),
            "inside its radius"
        );
        let blocked = |from: Vec3, _: Vec3| from.x < 500.0;
        assert_eq!(
            s.pick(Vec3::new(1100.0, 0.0, 0.0), &blocked),
            Some(0),
            "no line of sight to the near one"
        );
        let none = |_: Vec3, _: Vec3| false;
        assert_eq!(s.pick(Vec3::ZERO, &none), None, "keeps the current one");
    }

    /// The HL2 soundscape files of a mounted install.
    #[test]
    fn hl2_soundscapes_load_from_the_install() {
        let Some(inst) = gm_content::find_install() else {
            eprintln!("no GMod install, skipped");
            return;
        };
        let mount = gm_content::Mount::for_install(&inst).unwrap();
        let defs = Definitions::from_mount(&mount);
        assert!(defs.len() > 100, "{} soundscapes", defs.len());
        let g = defs.get("GenericOutdoor").expect("GenericOutdoor");
        assert_eq!(g.dsp, 19);
        assert!(!g.loops.is_empty());
        let town = defs
            .flatten("d1_town.ScaredPeopleOutside")
            .or_else(|| defs.flatten("d1_town.Street"));
        assert!(town.is_some_and(|t| !t.loops.is_empty() || !t.randoms.is_empty()));
        eprintln!("{} soundscapes", defs.len());
    }

    /// A `GenericOutdoor` env over the listener starts its loop and fades it in.
    #[test]
    fn an_env_starts_its_loops_offline() {
        let Some(inst) = gm_content::find_install() else {
            eprintln!("no GMod install, skipped");
            return;
        };
        let mount = gm_content::Mount::for_install(&inst).unwrap();
        let mut sounds = Sounds::offline(&mount, 48000).unwrap();
        let mut ss = Soundscapes::new(
            Definitions::from_mount(&mount),
            vec![Env {
                origin: Vec3::ZERO,
                radius: -1.0,
                name: "genericoutdoor".into(),
                positions: [None; 8],
            }],
        );
        let clear = |_: Vec3, _: Vec3| true;
        ss.update(&mut sounds, &mount, Vec3::new(0.0, 0.0, 64.0), &clear);
        assert_eq!(ss.current, Some(0));
        assert!(ss.active_loops() >= 1, "{}", ss.active_loops());
        sounds.mixer.render(crate::engine::FRAME);
        assert!(sounds.mixer.voices() >= 1);
        ss.clear(&mut sounds);
        sounds.mixer.render(crate::engine::FRAME);
        assert_eq!(sounds.mixer.voices(), 0);
    }

    #[test]
    fn env_entities_resolve_their_positions() {
        let text = r#"{
"classname" "info_target"
"targetname" "drip_spot"
"origin" "10 20 30"
}
{
"classname" "env_soundscape"
"soundscape" "Test.Outside"
"radius" "512"
"position1" "drip_spot"
"origin" "1 2 3"
}
"#;
        let entities = gm_formats::bsp::lumps::parse_entities(text.as_bytes()).unwrap();
        let envs = Env::from_entities(&entities);
        assert_eq!(envs.len(), 1);
        assert_eq!(envs[0].name, "test.outside");
        assert_eq!(envs[0].radius, 512.0);
        assert_eq!(envs[0].origin, Vec3::new(1.0, 2.0, 3.0));
        assert_eq!(envs[0].positions[1], Some(Vec3::new(10.0, 20.0, 30.0)));
        assert_eq!(envs[0].positions[0], None);
    }
}
