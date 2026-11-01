//! Voice chat on the client: `+voicerecord` holds the microphone open (the game's default
//! binds: `x`, `MOUSE5`), 20 ms Opus frames go to the server, other players' frames are
//! decoded into a streaming voice at the speaker's mouth (CHAN_VOICE, 3D like the game's
//! `voice` with `sv_alltalk`), `voice_scale` is the gain, `voice_loopback` plays our own
//! voice back in the head. A speaker whose frames stop for half a second falls silent.

use std::collections::HashMap;
use std::time::{Duration, Instant};

use glam::Vec3;
use gm_audio::voice::{Capture, Decoder, Encoder};

const SILENCE: Duration = Duration::from_millis(500);

#[derive(Default)]
pub struct VoiceChat {
    capture: Option<Capture>,
    encoder: Option<Encoder>,
    /// `+voicerecord` held.
    pub recording: bool,
    no_device_warned: bool,
    /// Speakers by player id: their decoder and when their last frame came.
    speakers: HashMap<u32, (Decoder, Instant)>,
    loopback: Option<Decoder>,
}

pub struct Speaker {
    pub id: u32,
    pub mouth: Vec3,
}

impl VoiceChat {
    pub fn start(&mut self) {
        if self.recording {
            return;
        }
        self.recording = true;
        if self.capture.is_none() {
            self.capture = Capture::start();
            if self.capture.is_none() && !self.no_device_warned {
                self.no_device_warned = true;
                eprintln!("voice: no input device");
            }
        }
        if self.encoder.is_none() {
            self.encoder = Encoder::new().ok();
        }
    }

    pub fn stop(&mut self) {
        self.recording = false;
        if let Some(e) = &mut self.encoder {
            e.flush();
        }
        // the device is released with the key so an unplugged microphone is retried later
        self.capture = None;
    }

    /// Once a frame: our captured audio out as packets, the received packets into the
    /// speakers' streams; `speakers` are the other players at their mouths this frame.
    #[allow(clippy::too_many_arguments)]
    pub fn update(
        &mut self,
        sounds: &mut gm_audio::Sounds,
        received: Vec<(u32, u16, Vec<u8>)>,
        speakers: &[Speaker],
        own: Option<u32>,
        enabled: bool,
        scale: f32,
        loopback: bool,
    ) -> Vec<(u16, Vec<u8>)> {
        let now = Instant::now();
        let mut out = Vec::new();
        if self.recording {
            if let (Some(c), Some(e)) = (&self.capture, &mut self.encoder) {
                out = e.push(&c.drain());
            }
        }
        let own = own.unwrap_or(0);
        if loopback && own != 0 && !out.is_empty() {
            let dec = self
                .loopback
                .get_or_insert_with(|| Decoder::new().expect("opus decoder"));
            let id = match sounds.stream_of(own) {
                Some(id) => id,
                None => sounds.stream(own, None, scale),
            };
            for (seq, data) in &out {
                let pcm = dec.decode(*seq, data);
                sounds.feed(id, &pcm);
            }
            self.speakers
                .entry(own)
                .or_insert_with(|| (Decoder::new().expect("opus decoder"), now))
                .1 = now;
        }
        if enabled {
            for (player, seq, data) in received {
                if player == own {
                    continue;
                }
                let mouth = speakers.iter().find(|s| s.id == player).map(|s| s.mouth);
                let (dec, last) = self
                    .speakers
                    .entry(player)
                    .or_insert_with(|| (Decoder::new().expect("opus decoder"), now));
                *last = now;
                let id = match sounds.stream_of(player) {
                    Some(id) => id,
                    None => sounds.stream(player, mouth, scale),
                };
                let pcm = dec.decode(seq, &data);
                sounds.feed(id, &pcm);
            }
        }
        for s in speakers {
            if self.speakers.contains_key(&s.id) {
                sounds.follow(s.id, s.mouth);
            }
        }
        let gone: Vec<u32> = self
            .speakers
            .iter()
            .filter(|(_, (_, last))| now.duration_since(*last) > SILENCE)
            .map(|(id, _)| *id)
            .collect();
        for id in gone {
            self.speakers.remove(&id);
            sounds.stop_entity(id, 0.05);
        }
        out
    }
}
