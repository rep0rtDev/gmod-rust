//! The output stream: cpal's default device pulls fixed [`FRAME`] blocks from the
//! Steam Audio [`Engine`] on the audio thread; the game thread talks to it through a
//! command queue (no lock on the render path). Without a device (headless) the engine
//! renders offline on demand ([`Mixer::render`]), which is also how tests listen.

use std::sync::atomic::{AtomicU32, AtomicUsize, Ordering};
use std::sync::mpsc::{channel, Receiver, Sender};
use std::sync::Arc;

use audionimbus::{Context, ContextSettings};
use cpal::traits::{DeviceTrait, HostTrait, StreamTrait};
use glam::Vec3;

use crate::acoustics::{self, Acoustics, Occluder, Shared};
use crate::engine::{Engine, Play, FRAME};
use crate::scene::Geometry;

/// Where sounds are heard from: eye position and the view frame (unit vectors).
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Listener {
    pub position: Vec3,
    pub right: Vec3,
    pub up: Vec3,
    pub ahead: Vec3,
}

impl Default for Listener {
    fn default() -> Self {
        Listener {
            position: Vec3::ZERO,
            right: -Vec3::Y,
            up: Vec3::Z,
            ahead: Vec3::X,
        }
    }
}

impl Listener {
    /// From the view angles (degrees, pitch down positive as the game stores them).
    pub fn from_view(position: Vec3, pitch: f32, yaw: f32) -> Listener {
        let (sp, cp) = pitch.to_radians().sin_cos();
        let (sy, cy) = yaw.to_radians().sin_cos();
        let ahead = Vec3::new(cp * cy, cp * sy, -sp);
        let right = Vec3::new(sy, -cy, 0.0);
        let up = right.cross(ahead).normalize_or_zero();
        Listener {
            position,
            right,
            up,
            ahead,
        }
    }
}

/// Distance gain of a sound with level `level` dB heard `dist` units away. The SDK's
/// `SNDLVL_TO_ATTN` (`20 / (level - 50)`, 4 under 50 dB) gives the attenuation; the gain
/// then falls linearly to silence at `1000 / attenuation` units (GoldSrc's law, which the
/// game's scripts were tuned against: gunfire at 140 dB carries 4500 units, 75 dB 1250).
/// Not measured against the game; the client cannot record its mix.
pub fn distance_gain(level: f32, dist: f32) -> f32 {
    if level <= 0.0 {
        return 1.0;
    }
    let attn = if level > 50.0 {
        20.0 / (level - 50.0)
    } else {
        4.0
    };
    (1.0 - dist * attn / 1000.0).clamp(0.0, 1.0)
}

pub enum Cmd {
    Play(u32, Play),
    Stop(u32),
    StopChannel(u32, Option<u8>, f32),
    Move(u32, Vec3),
    Volume(u32, f32),
    Listener(Listener),
    Master(f32),
    Scene(Arc<Shared>),
    Underwater(bool),
}

fn apply(engine: &mut Engine, cmd: Cmd) {
    match cmd {
        Cmd::Play(id, p) => {
            if let Err(e) = engine.play(id, p) {
                eprintln!("audio: {e}");
            }
        }
        Cmd::Stop(id) => engine.stop(id),
        Cmd::StopChannel(ent, ch, fade) => engine.stop_channel(ent, ch, fade),
        Cmd::Move(id, p) => engine.set_position(id, p),
        Cmd::Volume(id, v) => engine.set_volume(id, v),
        Cmd::Listener(l) => engine.listener = l,
        Cmd::Master(m) => engine.master = m.clamp(0.0, 1.0),
        Cmd::Scene(shared) => engine.shared = Some(shared),
        Cmd::Underwater(u) => engine.underwater = u,
    }
}

/// Audio-thread state: the engine, its queue and the rendered frames not yet consumed.
struct Stream {
    engine: Engine,
    rx: Receiver<Cmd>,
    fifo: Vec<f32>,
    block: Vec<f32>,
    voices: Arc<AtomicUsize>,
}

impl Stream {
    fn drain(&mut self) {
        while let Ok(cmd) = self.rx.try_recv() {
            apply(&mut self.engine, cmd);
        }
    }

    /// Fill `out` (interleaved, `channels` wide) from whole engine frames.
    fn fill(&mut self, out: &mut [f32], channels: usize) {
        self.drain();
        let frames = out.len() / channels;
        while self.fifo.len() < frames * 2 {
            self.engine.render(&mut self.block);
            self.fifo.extend_from_slice(&self.block);
        }
        for (o, s) in out
            .chunks_exact_mut(channels)
            .zip(self.fifo.as_chunks::<2>().0)
        {
            match channels {
                1 => o[0] = (s[0] + s[1]) * 0.5,
                _ => {
                    o[0] = s[0];
                    o[1] = s[1];
                    for x in &mut o[2..] {
                        *x = 0.0;
                    }
                }
            }
        }
        self.fifo.drain(..frames * 2);
        self.voices
            .store(self.engine.voices().len(), Ordering::Relaxed);
    }
}

/// The output device and the voices playing on it.
pub struct Mixer {
    context: Option<Context>,
    tx: Sender<Cmd>,
    voices: Arc<AtomicUsize>,
    next_id: AtomicU32,
    pub out_rate: f64,
    _stream: Option<cpal::Stream>,
    /// Without a device the engine stays here and renders on demand.
    offline: Option<Stream>,
    /// Offline: the simulation runs inline in [`Mixer::simulate`].
    acoustics: Option<Acoustics>,
    shared: Option<Arc<Shared>>,
    thread: Option<std::thread::JoinHandle<()>>,
    /// What was opened, or why nothing was.
    pub device: String,
}

impl Mixer {
    /// Open the default output device; without one the mixer renders offline.
    pub fn open() -> Mixer {
        let host = cpal::default_host();
        let Some(device) = host.default_output_device() else {
            return Mixer::offline_because("no output device".into());
        };
        let name = device.name().unwrap_or_else(|_| "?".into());
        let cfg = match device.default_output_config() {
            Ok(c) => c,
            Err(e) => return Mixer::offline_because(format!("{name}: {e}")),
        };
        let channels = cfg.channels() as usize;
        let out_rate = cfg.sample_rate().0;
        let context = match Context::try_new(&ContextSettings::default()) {
            Ok(c) => c,
            Err(e) => return Mixer::offline_because(format!("steam audio context: {e}")),
        };
        let engine = match Engine::new(&context, out_rate) {
            Ok(e) => e,
            Err(e) => return Mixer::offline_because(format!("{e}")),
        };
        let (tx, rx) = channel();
        let voices = Arc::new(AtomicUsize::new(0));
        let mut stream = Stream {
            engine,
            rx,
            fifo: Vec::with_capacity(FRAME * 8),
            block: vec![0.0; FRAME * 2],
            voices: voices.clone(),
        };
        let built = device.build_output_stream(
            &cfg.config(),
            move |out: &mut [f32], _| stream.fill(out, channels),
            |e| eprintln!("audio stream error: {e}"),
            None,
        );
        match built {
            Ok(s) => {
                if let Err(e) = s.play() {
                    return Mixer::offline_because(format!("{name}: {e}"));
                }
                Mixer {
                    context: Some(context),
                    tx,
                    voices,
                    next_id: AtomicU32::new(1),
                    out_rate: out_rate as f64,
                    _stream: Some(s),
                    offline: None,
                    acoustics: None,
                    shared: None,
                    thread: None,
                    device: format!("{name} ({out_rate} Hz, {channels} ch)"),
                }
            }
            Err(e) => Mixer::offline_because(format!("{name}: {e}")),
        }
    }

    /// An engine without a device at `rate` Hz: [`Mixer::render`] produces the mix.
    pub fn offline(rate: u32) -> anyhow::Result<Mixer> {
        let context = Context::try_new(&ContextSettings::default())
            .map_err(|e| anyhow::anyhow!("steam audio context: {e}"))?;
        let engine = Engine::new(&context, rate)?;
        let (tx, rx) = channel();
        let voices = Arc::new(AtomicUsize::new(0));
        Ok(Mixer {
            context: Some(context),
            tx,
            voices: voices.clone(),
            next_id: AtomicU32::new(1),
            out_rate: rate as f64,
            _stream: None,
            offline: Some(Stream {
                engine,
                rx,
                fifo: Vec::new(),
                block: vec![0.0; FRAME * 2],
                voices,
            }),
            acoustics: None,
            shared: None,
            thread: None,
            device: format!("offline ({rate} Hz)"),
        })
    }

    fn offline_because(why: String) -> Mixer {
        match Mixer::offline(48000) {
            Ok(mut m) => {
                m.device = format!("offline: {why}");
                m
            }
            Err(e) => {
                let (tx, _rx) = channel();
                Mixer {
                    context: None,
                    tx,
                    voices: Arc::new(AtomicUsize::new(0)),
                    next_id: AtomicU32::new(1),
                    out_rate: 48000.0,
                    _stream: None,
                    offline: None,
                    acoustics: None,
                    shared: None,
                    thread: None,
                    device: format!("silent: {why}; {e}"),
                }
            }
        }
    }

    pub fn is_live(&self) -> bool {
        self._stream.is_some()
    }

    /// The map's acoustic geometry: live, the simulation thread starts over it; offline,
    /// [`Mixer::simulate`] steps it. Replaces any previous scene.
    pub fn set_scene(&mut self, geometry: Geometry) -> anyhow::Result<()> {
        let Some(context) = self.context.clone() else {
            anyhow::bail!("no steam audio context ({})", self.device);
        };
        self.stop_acoustics();
        let shared = Arc::new(Shared::default());
        let rate = self.out_rate as u32;
        if self.offline.is_some() {
            self.acoustics = Some(Acoustics::new(&context, rate, &geometry)?);
        } else {
            self.thread = Some(acoustics::spawn(context, rate, geometry, shared.clone())?);
        }
        let _ = self.tx.send(Cmd::Scene(shared.clone()));
        self.shared = Some(shared);
        Ok(())
    }

    fn stop_acoustics(&mut self) {
        if let Some(s) = &self.shared {
            s.stop.store(true, Ordering::Relaxed);
        }
        if let Some(t) = self.thread.take() {
            let _ = t.join();
        }
        self.acoustics = None;
        self.shared = None;
    }

    /// The props at their poses this frame; the simulation thread instances their hulls.
    pub fn set_occluders(&self, occluders: Vec<Occluder>) {
        if let Some(s) = &self.shared {
            if let Ok(mut o) = s.occluders.lock() {
                *o = occluders;
            }
        }
    }

    /// Offline only: one simulation pass over the last rendered frame's voices.
    pub fn simulate(&mut self) -> anyhow::Result<()> {
        let (Some(a), Some(shared)) = (&mut self.acoustics, &self.shared) else {
            return Ok(());
        };
        let occluders = shared
            .occluders
            .lock()
            .map(|o| o.clone())
            .unwrap_or_default();
        a.sync_occluders(&occluders)?;
        let snap = shared
            .snapshot
            .lock()
            .map(|s| s.clone())
            .unwrap_or_default();
        let (paths, reverb) = a.step(&snap)?;
        if let Ok(mut p) = shared.paths.lock() {
            *p = paths;
        }
        if reverb.is_some() {
            if let Ok(mut r) = shared.reverb.lock() {
                *r = reverb;
            }
        }
        Ok(())
    }

    /// Triangles in the acoustic scene (offline only; live reports through the thread).
    pub fn scene_triangles(&self) -> Option<usize> {
        self.acoustics.as_ref().map(|a| a.triangles)
    }

    /// The `volume` cvar: master gain, clamped to 0..1.
    pub fn set_master(&self, volume: f32) {
        let _ = self.tx.send(Cmd::Master(volume));
    }

    pub fn set_listener(&self, listener: Listener) {
        let _ = self.tx.send(Cmd::Listener(listener));
    }

    /// The listener's eyes are under water: low-pass the mix.
    pub fn set_underwater(&self, underwater: bool) {
        let _ = self.tx.send(Cmd::Underwater(underwater));
    }

    /// Start a sound; the id stops or moves it later.
    pub fn play(&self, p: Play) -> u32 {
        let id = self.next_id.fetch_add(1, Ordering::Relaxed).max(1);
        let _ = self.tx.send(Cmd::Play(id, p));
        id
    }

    pub fn stop(&self, id: u32) {
        let _ = self.tx.send(Cmd::Stop(id));
    }

    /// Stop an entity's channel (all of them for `None`), fading out over `fade` seconds.
    pub fn stop_channel(&self, ent: u32, channel: Option<u8>, fade: f32) {
        let _ = self.tx.send(Cmd::StopChannel(ent, channel, fade));
    }

    pub fn move_voice(&self, id: u32, position: Vec3) {
        let _ = self.tx.send(Cmd::Move(id, position));
    }

    /// Change a playing voice's gain (soundscape fades).
    pub fn set_volume(&self, id: u32, volume: f32) {
        let _ = self.tx.send(Cmd::Volume(id, volume));
    }

    /// Voices playing after the last rendered block.
    pub fn voices(&self) -> usize {
        self.voices.load(Ordering::Relaxed)
    }

    /// Offline only: apply the queued commands and render `frames` stereo frames.
    pub fn render(&mut self, frames: usize) -> Vec<f32> {
        let mut out = vec![0.0; frames * 2];
        if let Some(s) = &mut self.offline {
            s.fill(&mut out, 2);
        }
        out
    }

    /// Offline only: the engine, for inspection.
    pub fn engine(&mut self) -> Option<&mut Engine> {
        self.offline.as_mut().map(|s| {
            s.drain();
            &mut s.engine
        })
    }
}

impl Drop for Mixer {
    fn drop(&mut self) {
        self.stop_acoustics();
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::wav::Pcm;

    #[test]
    fn gain_falls_linearly_to_the_level_range() {
        assert_eq!(distance_gain(140.0, 0.0), 1.0);
        assert!((distance_gain(140.0, 2250.0) - 0.5).abs() < 1e-4);
        assert_eq!(distance_gain(140.0, 5000.0), 0.0);
        assert!((distance_gain(75.0, 625.0) - 0.5).abs() < 1e-4);
        assert_eq!(distance_gain(0.0, 1e6), 1.0);
        assert_eq!(
            distance_gain(40.0, 250.0),
            0.0,
            "under 50 dB: attenuation 4"
        );
    }

    #[test]
    fn listener_frame_follows_the_view_angles() {
        let l = Listener::from_view(Vec3::ZERO, 0.0, 0.0);
        assert!((l.ahead - Vec3::X).length() < 1e-6);
        assert!((l.right + Vec3::Y).length() < 1e-6);
        assert!((l.up - Vec3::Z).length() < 1e-6);
        let l = Listener::from_view(Vec3::ZERO, 90.0, 0.0);
        assert!((l.ahead + Vec3::Z).length() < 1e-6, "looking down");
        assert!((l.up - Vec3::X).length() < 1e-6);
    }

    fn tone(rate: u32, seconds: f32) -> Arc<Pcm> {
        let n = (rate as f32 * seconds) as usize;
        Arc::new(Pcm {
            rate,
            samples: (0..n)
                .map(|i| (i as f32 * 440.0 / rate as f32 * std::f32::consts::TAU).sin() * 0.5)
                .collect(),
            loop_start: None,
        })
    }

    fn play(pcm: Arc<Pcm>, position: Option<Vec3>) -> Play {
        Play {
            pcm,
            volume: 1.0,
            pitch: 100.0,
            position,
            level: 75.0,
            looping: false,
            ent: 0,
            channel: 0,
        }
    }

    fn peak(buf: &[f32], channel: usize) -> f32 {
        buf.iter()
            .skip(channel)
            .step_by(2)
            .fold(0.0f32, |m, x| m.max(x.abs()))
    }

    #[test]
    fn hrtf_puts_a_sound_on_the_right_in_the_right_ear() {
        let mut m = Mixer::offline(48000).unwrap();
        m.set_listener(Listener::from_view(Vec3::ZERO, 0.0, 0.0));
        // right of the listener (yaw 0: -y is right), 100 u away
        m.play(play(tone(48000, 0.5), Some(Vec3::new(0.0, -100.0, 0.0))));
        let out = m.render(FRAME * 4);
        let (l, r) = (peak(&out, 0), peak(&out, 1));
        assert!(r > l * 1.5, "left {l} right {r}");
        let out = m.render(FRAME * 4);
        assert!(peak(&out, 0) > 0.0, "still playing");
    }

    #[test]
    fn unpositioned_sounds_are_centred_and_end() {
        let mut m = Mixer::offline(48000).unwrap();
        m.play(play(tone(48000, 0.02), None));
        let out = m.render(FRAME);
        let (l, r) = (peak(&out, 0), peak(&out, 1));
        assert!((l - r).abs() < 1e-5 && l > 0.3, "{l} {r}");
        m.render(FRAME);
        assert_eq!(m.voices(), 0, "0.02 s tone ends inside two frames");
    }

    /// Source 200 u ahead; a concrete slab between it and the listener occludes it.
    #[test]
    fn a_wall_occludes_and_the_mix_drops() {
        let source = Some(Vec3::new(200.0, 0.0, 0.0));
        let mut open = Mixer::offline(48000).unwrap();
        open.set_listener(Listener::from_view(Vec3::ZERO, 0.0, 0.0));
        open.set_scene(Geometry::default()).unwrap();
        let mut walled = Mixer::offline(48000).unwrap();
        walled.set_listener(Listener::from_view(Vec3::ZERO, 0.0, 0.0));
        let mut g = Geometry::default();
        g.push_box(
            Vec3::new(90.0, -400.0, -400.0),
            Vec3::new(110.0, 400.0, 400.0),
            "concrete",
        );
        walled.set_scene(g).unwrap();
        assert_eq!(walled.scene_triangles(), Some(12));
        let mut peaks = Vec::new();
        for m in [&mut open, &mut walled] {
            m.play(play(tone(48000, 1.0), source));
            m.render(FRAME); // publishes the voice to the snapshot
            m.simulate().unwrap();
            m.render(FRAME * 16); // the effect ramps its gains over frames
            let out = m.render(FRAME * 4);
            let path = m.engine().unwrap().voices()[0].path;
            peaks.push((peak(&out, 0).max(peak(&out, 1)), path));
        }
        let (open_peak, open_path) = peaks[0];
        let (wall_peak, wall_path) = peaks[1];
        assert!(
            open_path.occlusion > 0.99 && open_path.transmission[0] > 0.99,
            "{open_path:?}"
        );
        assert!(wall_path.occlusion < 0.05, "{wall_path:?}");
        assert!(
            wall_path.transmission[0] < 0.1
                && wall_path.transmission[2] < wall_path.transmission[0],
            "concrete passes little, less at high frequencies: {wall_path:?}"
        );
        assert!(
            wall_peak < open_peak * 0.2,
            "open {open_peak} walled {wall_peak}"
        );
    }

    /// The same slab as an instanced prop between the source and the listener occludes;
    /// moved aside it does not.
    #[test]
    fn a_prop_occludes_until_it_moves() {
        let source = Some(Vec3::new(200.0, 0.0, 0.0));
        let mut m = Mixer::offline(48000).unwrap();
        m.set_listener(Listener::from_view(Vec3::ZERO, 0.0, 0.0));
        m.set_scene(Geometry::default()).unwrap();
        let mut hull = Geometry::default();
        hull.push_box(
            Vec3::new(-10.0, -400.0, -400.0),
            Vec3::new(10.0, 400.0, 400.0),
            "concrete",
        );
        let hull = Arc::new(hull);
        let key: Arc<str> = Arc::from("models/slab.mdl");
        let at = |x: f32| {
            vec![Occluder {
                ent: 5,
                key: key.clone(),
                hull: hull.clone(),
                transform: glam::Mat4::from_translation(Vec3::new(x, 0.0, 0.0)),
            }]
        };
        m.set_occluders(at(100.0));
        m.play(play(tone(48000, 2.0), source));
        m.render(FRAME);
        m.simulate().unwrap();
        m.render(FRAME * 16);
        let blocked = m.engine().unwrap().voices()[0].path;
        assert!(blocked.occlusion < 0.05, "{blocked:?}");
        m.set_occluders(at(2100.0));
        m.simulate().unwrap();
        m.render(FRAME * 16);
        let open = m.engine().unwrap().voices()[0].path;
        assert!(open.occlusion > 0.99, "{open:?}");
        m.set_occluders(Vec::new());
        m.simulate().unwrap();
    }

    /// A closed concrete room keeps ringing after a short click; the open world does not.
    #[test]
    fn a_room_has_a_reverb_tail() {
        let mut g = Geometry::default();
        // 10 x 10 x 4 m room around the listener, walls 0.5 m thick, inward faces count
        let (a, b) = (
            Vec3::new(-200.0, -200.0, -40.0),
            Vec3::new(200.0, 200.0, 120.0),
        );
        g.push_box(a - Vec3::splat(20.0), b + Vec3::splat(20.0), "concrete");
        g.push_box(a, b, "concrete");
        let mut tails = Vec::new();
        for geometry in [Geometry::default(), g] {
            let mut m = Mixer::offline(48000).unwrap();
            m.set_listener(Listener::from_view(Vec3::ZERO, 0.0, 0.0));
            m.set_scene(geometry).unwrap();
            m.play(play(tone(48000, 0.05), Some(Vec3::new(100.0, 0.0, 0.0))));
            m.render(FRAME);
            for _ in 0..acoustics::REVERB_EVERY {
                m.simulate().unwrap();
            }
            m.render(FRAME * 8); // the click and the ramp
            let tail = m.render(FRAME * 8);
            tails.push((
                m.engine().unwrap().has_reverb(),
                peak(&tail, 0).max(peak(&tail, 1)),
            ));
        }
        assert!(!tails[0].0, "no reverb without geometry: {tails:?}");
        assert!(tails[1].0, "the room produces a reverb: {tails:?}");
        assert!(tails[1].1 > 1e-3, "and it rings after the click: {tails:?}");
        assert!(tails[0].1 < 1e-6, "silence outside: {tails:?}");
    }

    #[test]
    fn underwater_muffles() {
        let mut m = Mixer::offline(48000).unwrap();
        let hiss: Vec<f32> = (0..24000)
            .map(|i| if i % 2 == 0 { 0.5 } else { -0.5 })
            .collect();
        let pcm = Arc::new(Pcm {
            rate: 48000,
            samples: hiss,
            loop_start: None,
        });
        m.play(play(pcm.clone(), None));
        let dry = peak(&m.render(FRAME * 2), 0);
        m.set_underwater(true);
        m.play(play(pcm, None));
        let wet = peak(&m.render(FRAME * 2), 0);
        assert!(wet < dry * 0.3, "dry {dry} wet {wet}");
    }

    #[test]
    fn channels_replace_and_loops_wrap() {
        let mut m = Mixer::offline(48000).unwrap();
        let mut p = play(tone(48000, 0.01), None);
        p.looping = true;
        p.ent = 7;
        p.channel = 6;
        m.play(p.clone());
        m.render(FRAME * 4);
        assert_eq!(m.voices(), 1, "the loop keeps playing");
        m.play(p);
        m.render(FRAME);
        assert_eq!(m.voices(), 1, "same entity + channel replaces");
        m.stop_channel(7, None, 0.0);
        m.render(FRAME);
        assert_eq!(m.voices(), 0);
    }

    #[test]
    fn a_fade_out_takes_its_time() {
        let mut m = Mixer::offline(48000).unwrap();
        let mut p = play(tone(48000, 0.01), None);
        p.looping = true;
        p.ent = 3;
        p.channel = 6;
        m.play(p);
        m.render(FRAME);
        m.stop_channel(3, Some(6), 0.1); // 4800 samples: 9.4 frames
        let early = peak(&m.render(FRAME * 2), 0);
        let late = peak(&m.render(FRAME * 6), 0);
        assert!(early > 0.2 && late < early && late > 0.0, "{early} {late}");
        m.render(FRAME * 3);
        assert_eq!(m.voices(), 0, "gone after the fade");
    }
}
