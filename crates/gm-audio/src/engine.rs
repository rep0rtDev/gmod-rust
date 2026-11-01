//! Steam Audio render: every voice is a mono stream run through a `DirectEffect` (the
//! script's distance gain, plus occlusion / transmission / air absorption when the
//! simulation supplies them) and a `BinauralEffect` (HRTF against the listener's frame),
//! summed into a stereo frame of fixed size. Sounds without a position (the owner's own
//! weapon) are mixed centred without HRTF, as the game does for `CHAN_WEAPON` at the ear.
//!
//! Units: the game's world is inches, x forward, y left, z up; Steam Audio wants meters
//! and a right-handed y-up frame. [`to_sa`] is the same map the physics uses for Jolt.

use std::sync::Arc;

use crate::acoustics::{Shared, REVERB_DURATION, REVERB_ORDER};

use audionimbus::effect::{
    AmbisonicsDecodeEffect, AmbisonicsDecodeEffectParams, AmbisonicsDecodeEffectSettings,
    BinauralEffect, BinauralEffectParams, BinauralEffectSettings, DirectEffect, DirectEffectParams,
    DirectEffectSettings, Parametric, ReflectionEffect, ReflectionEffectParams,
    ReflectionEffectSettings, Rendering, SpeakerLayout,
};
use audionimbus::geometry::{CoordinateSystem, Vector3};
use audionimbus::{
    AudioBufferMut, AudioBufferRef, AudioSettings, Context, Hrtf, HrtfInterpolation, HrtfSettings,
};
use glam::Vec3;

use crate::mixer::{distance_gain, Listener};
use crate::wav::Pcm;

/// Samples per render block; every effect is created for this size.
pub const FRAME: usize = 512;
pub const UNITS_TO_METERS: f32 = 0.0254;

/// Game space (inches, x forward, y left, z up) to Steam Audio space (meters, y up).
pub fn to_sa(v: Vec3) -> Vector3 {
    Vector3::new(
        v.x * UNITS_TO_METERS,
        v.z * UNITS_TO_METERS,
        -v.y * UNITS_TO_METERS,
    )
}

/// A direction (unit, no scale) in Steam Audio axes.
pub fn dir_sa(v: Vec3) -> Vector3 {
    Vector3::new(v.x, v.z, -v.y)
}

/// One sound to start.
#[derive(Debug, Clone)]
pub struct Play {
    pub pcm: Arc<Pcm>,
    /// Linear gain before distance.
    pub volume: f32,
    /// Percent: 100 is the recorded rate.
    pub pitch: f32,
    /// World position; `None` plays at the listener (the owner's own weapon).
    pub position: Option<Vec3>,
    /// Sound level in dB at the source (`SNDLVL_*`); 0 carries without attenuation.
    pub level: f32,
    /// Loop from the WAV's cue point (or the start) back to the end until stopped.
    pub looping: bool,
    /// Owner entity and channel; a new sound on the same pair replaces the old one
    /// (`CHAN_AUTO`, 0, never replaces).
    pub ent: u32,
    pub channel: u8,
}

/// What the simulation thread knows about a voice's path to the listener.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Path {
    pub occlusion: f32,
    pub transmission: [f32; 3],
    pub air_absorption: [f32; 3],
}

impl Default for Path {
    fn default() -> Self {
        Path {
            occlusion: 1.0,
            transmission: [1.0; 3],
            air_absorption: [1.0; 3],
        }
    }
}

pub struct Voice {
    pub id: u32,
    pcm: Arc<Pcm>,
    cursor: f64,
    step: f64,
    gain: f32,
    pub position: Option<Vec3>,
    level: f32,
    looping: bool,
    pub ent: u32,
    pub channel: u8,
    pub path: Path,
    direct: DirectEffect,
    binaural: BinauralEffect,
    done: bool,
}

/// Listener-centric reverb: the parametric reflection effect over the wet sum of the
/// positioned voices, first-order ambisonics decoded binaurally in the listener's frame.
struct Reverb {
    effect: ReflectionEffect<Parametric>,
    decode: AmbisonicsDecodeEffect,
    params: Option<ReflectionEffectParams<Parametric>>,
    wet: Vec<f32>,
    ambi: Vec<f32>,
    out: Vec<f32>,
    channels: usize,
}

/// The Steam Audio state of one output stream.
pub struct Engine {
    _context: Context,
    hrtf: Hrtf,
    settings: AudioSettings,
    pub rate: u32,
    pub listener: Listener,
    pub master: f32,
    voices: Vec<Voice>,
    mono: Vec<f32>,
    direct_out: Vec<f32>,
    bin_out: Vec<f32>,
    /// Mailboxes to the acoustics thread, when there is a scene.
    pub shared: Option<Arc<Shared>>,
    reverb: Option<Reverb>,
    /// Listener's eyes under water: the game's `dsp_water` (preset 14) is a muffling
    /// low-pass; ours is a one-pole at [`UNDERWATER_CUTOFF`] over the mix.
    pub underwater: bool,
    lp: [f32; 2],
}

/// Underwater low-pass cutoff (Hz).
pub const UNDERWATER_CUTOFF: f32 = 900.0;

impl Engine {
    pub fn new(context: &Context, rate: u32) -> anyhow::Result<Engine> {
        let context = context.clone();
        let settings = AudioSettings {
            sampling_rate: rate,
            frame_size: FRAME as u32,
        };
        let hrtf = Hrtf::try_new(&context, &settings, &HrtfSettings::default())
            .map_err(|e| anyhow::anyhow!("steam audio hrtf: {e}"))?;
        let channels = ((REVERB_ORDER + 1) * (REVERB_ORDER + 1)) as usize;
        let reverb = Reverb {
            effect: ReflectionEffect::try_new(
                &context,
                &settings,
                &ReflectionEffectSettings {
                    impulse_response_size: (REVERB_DURATION * rate as f32) as u32,
                    num_channels: channels as u32,
                },
            )
            .map_err(|e| anyhow::anyhow!("steam audio reflection effect: {e}"))?,
            decode: AmbisonicsDecodeEffect::try_new(
                &context,
                &settings,
                &AmbisonicsDecodeEffectSettings {
                    speaker_layout: SpeakerLayout::Stereo,
                    hrtf: hrtf.clone(),
                    max_order: REVERB_ORDER,
                    rendering: Rendering::Binaural,
                },
            )
            .map_err(|e| anyhow::anyhow!("steam audio ambisonics decode: {e}"))?,
            params: None,
            wet: vec![0.0; FRAME],
            ambi: vec![0.0; FRAME * channels],
            out: vec![0.0; FRAME * 2],
            channels,
        };
        Ok(Engine {
            _context: context,
            hrtf,
            settings,
            rate,
            listener: Listener::default(),
            master: 1.0,
            voices: Vec::new(),
            mono: vec![0.0; FRAME],
            direct_out: vec![0.0; FRAME],
            bin_out: vec![0.0; FRAME * 2],
            shared: None,
            reverb: Some(reverb),
            underwater: false,
            lp: [0.0; 2],
        })
    }

    pub fn voices(&self) -> &[Voice] {
        &self.voices
    }

    pub fn voices_mut(&mut self) -> &mut [Voice] {
        &mut self.voices
    }

    /// Start voice `id`. A sound on a non-auto channel of the same entity stops the one
    /// playing there.
    pub fn play(&mut self, id: u32, p: Play) -> anyhow::Result<()> {
        if p.channel != 0 {
            self.voices
                .retain(|v| !(v.ent == p.ent && v.channel == p.channel));
        }
        let direct = DirectEffect::try_new(
            &self._context,
            &self.settings,
            &DirectEffectSettings { num_channels: 1 },
        )
        .map_err(|e| anyhow::anyhow!("direct effect: {e}"))?;
        let binaural = BinauralEffect::try_new(
            &self._context,
            &self.settings,
            &BinauralEffectSettings {
                hrtf: self.hrtf.clone(),
            },
        )
        .map_err(|e| anyhow::anyhow!("binaural effect: {e}"))?;
        self.voices.push(Voice {
            id,
            step: p.pitch as f64 / 100.0 * p.pcm.rate as f64 / self.rate as f64,
            pcm: p.pcm,
            cursor: 0.0,
            gain: p.volume,
            position: p.position,
            level: p.level,
            looping: p.looping,
            ent: p.ent,
            channel: p.channel,
            path: Path::default(),
            direct,
            binaural,
            done: false,
        });
        Ok(())
    }

    pub fn stop(&mut self, id: u32) {
        self.voices.retain(|v| v.id != id);
    }

    /// Stop what plays on an entity's channel (every channel when `channel` is `None`).
    pub fn stop_channel(&mut self, ent: u32, channel: Option<u8>) {
        self.voices
            .retain(|v| !(v.ent == ent && channel.is_none_or(|c| c == v.channel)));
    }

    /// Move a looping voice with its entity.
    pub fn set_position(&mut self, id: u32, position: Vec3) {
        if let Some(v) = self.voices.iter_mut().find(|v| v.id == id) {
            v.position = Some(position);
        }
    }

    /// Render one stereo frame (`FRAME` samples per channel, interleaved) into `out`.
    pub fn render(&mut self, out: &mut [f32]) {
        debug_assert_eq!(out.len(), FRAME * 2);
        out.fill(0.0);
        self.exchange();
        let listener = self.listener;
        let master = self.master;
        if let Some(r) = &mut self.reverb {
            r.wet.fill(0.0);
        }
        for v in &mut self.voices {
            v.fill(&mut self.mono);
            match v.position {
                None => {
                    for (o, s) in out.as_chunks_mut::<2>().0.iter_mut().zip(&self.mono) {
                        let s = s * v.gain * master * 0.707;
                        o[0] += s;
                        o[1] += s;
                    }
                }
                Some(p) => {
                    let d = p - listener.position;
                    let dist = d.length();
                    let params = DirectEffectParams {
                        distance_attenuation: Some(distance_gain(v.level, dist)),
                        air_absorption: Some(audionimbus::Equalizer(v.path.air_absorption)),
                        directivity: None,
                        occlusion: Some(v.path.occlusion),
                        transmission: Some(audionimbus::effect::Transmission::FrequencyDependent(
                            audionimbus::Equalizer(v.path.transmission),
                        )),
                    };
                    let input = AudioBufferRef::try_new(&self.mono, 1).expect("mono frame");
                    let mut dout =
                        AudioBufferMut::try_new(&mut self.direct_out, 1).expect("direct frame");
                    let _ = v.direct.apply(&params, &input, &mut dout);
                    if let Some(r) = &mut self.reverb {
                        let g = v.gain * master;
                        for (w, d) in r.wet.iter_mut().zip(&self.direct_out) {
                            *w += d * g;
                        }
                    }
                    let local = if dist > 1e-3 {
                        let n = d / dist;
                        Vec3::new(
                            n.dot(listener.right),
                            n.dot(listener.up),
                            -n.dot(listener.ahead),
                        )
                    } else {
                        Vec3::new(0.0, 0.0, -1.0)
                    };
                    let bparams = BinauralEffectParams {
                        direction: Vector3::new(local.x, local.y, local.z),
                        interpolation: HrtfInterpolation::Bilinear,
                        spatial_blend: 1.0,
                        hrtf: self.hrtf.clone(),
                        peak_delays: None,
                    };
                    let dinput =
                        AudioBufferRef::try_new(&self.direct_out, 1).expect("direct frame");
                    let mut bout =
                        AudioBufferMut::try_new(&mut self.bin_out, 2).expect("binaural frame");
                    let _ = v.binaural.apply(&bparams, &dinput, &mut bout);
                    let (l, r) = self.bin_out.split_at(FRAME);
                    let g = v.gain * master;
                    for ((o, l), r) in out.as_chunks_mut::<2>().0.iter_mut().zip(l).zip(r) {
                        o[0] += l * g;
                        o[1] += r * g;
                    }
                }
            }
        }
        self.voices.retain(|v| !v.done);
        if let Some(r) = &mut self.reverb {
            if let Some(params) = &r.params {
                let input = AudioBufferRef::try_new(&r.wet, 1).expect("wet frame");
                let mut ambi =
                    AudioBufferMut::try_new(&mut r.ambi, r.channels).expect("ambisonics frame");
                if r.effect.apply(params, &input, &mut ambi).is_ok() {
                    let ambi_in =
                        AudioBufferRef::try_new(&r.ambi, r.channels).expect("ambisonics frame");
                    let mut stereo = AudioBufferMut::try_new(&mut r.out, 2).expect("reverb frame");
                    let dparams = AmbisonicsDecodeEffectParams {
                        order: REVERB_ORDER,
                        hrtf: self.hrtf.clone(),
                        orientation: CoordinateSystem {
                            right: dir_sa(listener.right),
                            up: dir_sa(listener.up),
                            ahead: dir_sa(listener.ahead),
                            origin: to_sa(listener.position),
                        },
                    };
                    if r.decode.apply(&dparams, &ambi_in, &mut stereo).is_ok() {
                        let (l, rr) = r.out.split_at(FRAME);
                        for ((o, l), rr) in out.as_chunks_mut::<2>().0.iter_mut().zip(l).zip(rr) {
                            o[0] += l;
                            o[1] += rr;
                        }
                    }
                }
            }
        }
        if self.underwater {
            let a = 1.0 - (-std::f32::consts::TAU * UNDERWATER_CUTOFF / self.rate as f32).exp();
            for o in out.as_chunks_mut::<2>().0.iter_mut() {
                for (sample, state) in o.iter_mut().zip(self.lp.iter_mut()) {
                    *state += a * (*sample - *state);
                    *sample = *state;
                }
            }
        } else {
            self.lp = [0.0; 2];
        }
        for x in out.iter_mut() {
            *x = x.clamp(-1.0, 1.0);
        }
    }

    /// Whether a reverb has been received from the acoustics thread.
    pub fn has_reverb(&self) -> bool {
        self.reverb.as_ref().is_some_and(|r| r.params.is_some())
    }
}

impl Engine {
    /// Tell the acoustics thread where things are and take the paths it found; `try_lock`
    /// only, the render never waits.
    fn exchange(&mut self) {
        let Some(shared) = &self.shared else {
            return;
        };
        if let Ok(mut snap) = shared.snapshot.try_lock() {
            snap.listener = Some(self.listener);
            snap.voices.clear();
            snap.voices.extend(
                self.voices
                    .iter()
                    .filter_map(|v| v.position.map(|p| (v.id, p))),
            );
        }
        if let Ok(paths) = shared.paths.try_lock() {
            for v in &mut self.voices {
                if let Some(p) = paths.get(&v.id) {
                    v.path = *p;
                }
            }
        }
        if let Some(r) = &mut self.reverb {
            if let Ok(mut fresh) = shared.reverb.try_lock() {
                if let Some(p) = fresh.take() {
                    r.params = Some(p);
                }
            }
        }
    }
}

impl Voice {
    /// Next `FRAME` source samples at the voice's pitch (linear interpolation), zero past
    /// the end of a one-shot; a loop wraps to the cue point.
    fn fill(&mut self, mono: &mut [f32]) {
        let s = &self.pcm.samples;
        let n = s.len();
        let loop_start = self.pcm.loop_start.unwrap_or(0).min(n.saturating_sub(1));
        for m in mono.iter_mut() {
            let i = self.cursor as usize;
            if i + 1 >= n {
                if self.looping && n > loop_start + 1 {
                    self.cursor = loop_start as f64 + (self.cursor - i as f64);
                    let i = self.cursor as usize;
                    let f = (self.cursor - i as f64) as f32;
                    *m = s[i] * (1.0 - f) + s[(i + 1).min(n - 1)] * f;
                    self.cursor += self.step;
                    continue;
                }
                self.done = true;
                *m = 0.0;
                continue;
            }
            let f = (self.cursor - i as f64) as f32;
            *m = s[i] * (1.0 - f) + s[i + 1] * f;
            self.cursor += self.step;
        }
    }
}
