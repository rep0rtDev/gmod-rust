//! Client settings the Options dialog edits: the cvars of `OptionsSub*.res` and the key
//! binds, kept the way the game keeps them (`cfg/config.cfg`: `bind "key" "command"` and
//! `cvar value` lines, Source key names). Defaults come from the mounted
//! `cfg/config_default.cfg` (GMod's binds) with `scripts/kb_def.lst` underneath.

use std::collections::BTreeMap;
use std::path::PathBuf;

use gm_content::Mount;
use winit::keyboard::KeyCode;

#[derive(Clone, Debug, PartialEq)]
pub struct Settings {
    /// `sensitivity` (default 3): mouse degrees per count = 0.022 * sensitivity.
    pub sensitivity: f32,
    /// `m_pitch` negative: reverse mouse.
    pub m_pitch_invert: bool,
    pub m_rawinput: bool,
    pub m_filter: bool,
    pub m_customaccel: bool,
    pub m_customaccel_exponent: f32,
    /// `volume` 0..1.
    pub volume: f32,
    /// `snd_musicvolume` 0..1.
    pub snd_musicvolume: f32,
    pub snd_mute_losefocus: bool,
    pub dsp_enhance_stereo: bool,
    /// `closecaption` 0 / 1 (subtitles) / 2 (closed captions, `cc_subtitles` 0).
    pub closecaption: u8,
    pub snd_surround_speakers: i32,
    pub snd_pitchquality: u8,
    pub hud_fastswitch: bool,
    pub cl_enable_loadingurl: bool,
    pub cl_downloadfilter: String,
    /// `cl_playermodel` (empty: the server's default), `cl_playercolor` / `cl_weaponcolor`
    /// ("r g b" in 0..1).
    pub cl_playermodel: String,
    pub cl_playercolor: String,
    pub cl_weaponcolor: String,
    pub cl_spraydisable: bool,
    /// `fov_desired` 75..100 (sandbox lets 75..100 through `fov_desired`).
    pub fov_desired: f32,
    /// `fps_max`, 0 = unlimited.
    pub fps_max: f32,
    /// Advanced video (`OptionsSubVideoAdvancedDlg.res`): `mat_picmip` 2..-1 (texture detail
    /// Low..Very High), `r_rootlod` 2..0 (model detail), water / shadow / colour correction,
    /// `mat_antialias` 1/2/4/8, `mat_trilinear` + `mat_forceaniso`, `mat_vsync`,
    /// `mat_hdr_level` 0..2, motion blur, `mat_queue_mode` (-1 multicore), bloom; gamma 1.6..2.6.
    pub mat_picmip: i32,
    pub r_rootlod: i32,
    pub r_waterforceexpensive: bool,
    pub r_waterforcereflectentities: bool,
    pub r_shadowrendertotexture: bool,
    pub mat_colorcorrection: bool,
    pub mat_antialias: u32,
    pub mat_trilinear: bool,
    pub mat_forceaniso: u32,
    pub mat_vsync: bool,
    pub mat_hdr_level: u8,
    pub mat_motion_blur_enabled: bool,
    pub mat_queue_mode: i32,
    pub mat_bloomscale: bool,
    pub mat_monitorgamma: f32,
    /// `con_enable` (Keyboard - Advanced: "Enable developer console").
    pub con_enable: bool,
    pub width: u32,
    pub height: u32,
    pub fullscreen: bool,
    pub voice_modenable: bool,
    pub voice_scale: f32,
    pub voice_loopback: bool,
    /// Source key name -> command.
    pub binds: BTreeMap<String, String>,
}

impl Default for Settings {
    fn default() -> Self {
        Settings {
            sensitivity: 3.0,
            m_pitch_invert: false,
            m_rawinput: true,
            m_filter: false,
            m_customaccel: false,
            m_customaccel_exponent: 1.0,
            volume: 1.0,
            snd_musicvolume: 1.0,
            snd_mute_losefocus: true,
            dsp_enhance_stereo: false,
            closecaption: 0,
            snd_surround_speakers: 2,
            snd_pitchquality: 1,
            hud_fastswitch: false,
            cl_enable_loadingurl: true,
            cl_downloadfilter: "all".to_string(),
            cl_playermodel: String::new(),
            cl_playercolor: "0.24 0.34 0.41".to_string(),
            cl_weaponcolor: "0.30 1.80 2.10".to_string(),
            cl_spraydisable: false,
            fov_desired: 75.0,
            fps_max: 300.0,
            mat_picmip: 0,
            r_rootlod: 0,
            r_waterforceexpensive: true,
            r_waterforcereflectentities: false,
            r_shadowrendertotexture: true,
            mat_colorcorrection: true,
            mat_antialias: 1,
            mat_trilinear: true,
            mat_forceaniso: 8,
            mat_vsync: true,
            mat_hdr_level: 2,
            mat_motion_blur_enabled: false,
            mat_queue_mode: -1,
            mat_bloomscale: true,
            mat_monitorgamma: 2.2,
            con_enable: true,
            width: 1280,
            height: 720,
            fullscreen: false,
            voice_modenable: true,
            voice_scale: 1.0,
            voice_loopback: false,
            binds: BTreeMap::new(),
        }
    }
}

/// Where the client keeps its own files (`$GMOD_RUST_HOME`, else `~/.gmod-rust`).
pub fn home_dir() -> PathBuf {
    if let Ok(p) = std::env::var("GMOD_RUST_HOME") {
        return PathBuf::from(p);
    }
    let base = std::env::var("HOME")
        .or_else(|_| std::env::var("USERPROFILE"))
        .unwrap_or_else(|_| ".".to_string());
    PathBuf::from(base).join(".gmod-rust")
}

/// `"a" "b"` pairs of a `.lst` / `.cfg` line stream (comments stripped, UTF-8).
fn quoted_pairs(text: &str) -> Vec<(String, String)> {
    let mut out = Vec::new();
    for line in text.lines() {
        let line = line.split("//").next().unwrap_or("").trim();
        let mut parts = Vec::new();
        let mut rest = line;
        while let Some(start) = rest.find('"') {
            let after = &rest[start + 1..];
            let Some(end) = after.find('"') else { break };
            parts.push(after[..end].to_string());
            rest = &after[end + 1..];
        }
        // one pair per line normally; a line like `"Tokens" { "k" "v" }` pairs from the end
        let start = parts.len() % 2;
        for pair in parts[start..].as_chunks::<2>().0 {
            out.push((pair[0].clone(), pair[1].clone()));
        }
    }
    out
}

/// The game's `bind "key" "cmd"` lines of a cfg (other lines are `cvar value`).
fn parse_cfg(
    text: &str,
    binds: &mut BTreeMap<String, String>,
    cvars: &mut BTreeMap<String, String>,
) {
    for line in text.lines() {
        let line = line.split("//").next().unwrap_or("").trim();
        if line.is_empty() {
            continue;
        }
        let toks = tokens(line);
        match toks.as_slice() {
            [b, key, cmd] if b.eq_ignore_ascii_case("bind") => {
                binds.insert(key.clone(), cmd.clone());
            }
            [u, key] if u.eq_ignore_ascii_case("unbind") => {
                binds.remove(key);
            }
            [name, value] => {
                cvars.insert(name.to_ascii_lowercase(), value.clone());
            }
            _ => {}
        }
    }
}

/// Console tokenizer: quoted strings or whitespace-separated words.
fn tokens(line: &str) -> Vec<String> {
    let mut out = Vec::new();
    let mut cur = String::new();
    let mut quoted = false;
    for c in line.chars() {
        match c {
            '"' => {
                if quoted {
                    out.push(std::mem::take(&mut cur));
                }
                quoted = !quoted;
            }
            c if c.is_whitespace() && !quoted => {
                if !cur.is_empty() {
                    out.push(std::mem::take(&mut cur));
                }
            }
            c => cur.push(c),
        }
    }
    if !cur.is_empty() {
        out.push(cur);
    }
    out
}

/// UTF-16LE (BOM) or UTF-8 text of a mounted file.
pub fn read_text(mount: &Mount, path: &str) -> Option<String> {
    let bytes = mount.read(path).ok()?;
    Some(decode_text(&bytes))
}

pub fn decode_text(bytes: &[u8]) -> String {
    if bytes.len() >= 2 && bytes[0] == 0xFF && bytes[1] == 0xFE {
        let units: Vec<u16> = bytes[2..]
            .as_chunks::<2>()
            .0
            .iter()
            .map(|c| u16::from_le_bytes(*c))
            .collect();
        String::from_utf16_lossy(&units)
    } else {
        String::from_utf8_lossy(bytes).into_owned()
    }
}

/// `"Tokens" { "key" "value" }` of a `resource/*_english.txt` file (the game's
/// localization files), keys lower-cased.
pub fn load_tokens(mount: &Mount, path: &str, into: &mut BTreeMap<String, String>) {
    let Some(text) = read_text(mount, path) else {
        return;
    };
    for (k, v) in quoted_pairs(&text) {
        if k.eq_ignore_ascii_case("Language") || k.eq_ignore_ascii_case("Tokens") {
            continue;
        }
        into.insert(k.to_ascii_lowercase(), v);
    }
}

/// A row of the Keyboard tab (`scripts/kb_act.lst`): a section header or a bindable action.
#[derive(Clone, Debug)]
pub struct KbRow {
    /// Empty for a header row.
    pub action: String,
    /// `#Token` of the label.
    pub label: String,
}

/// `kb_act.lst`: `"blank" "===="` rows are dividers, the `"blank" "#Title"` after them is
/// the section header, everything else is `"command" "#Label"`.
pub fn load_kb_act(mount: &Mount) -> Vec<KbRow> {
    let Some(text) = read_text(mount, "scripts/kb_act.lst") else {
        return Vec::new();
    };
    quoted_pairs(&text)
        .into_iter()
        .filter_map(|(cmd, label)| {
            if cmd == "blank" {
                if label.starts_with('=') {
                    None
                } else {
                    Some(KbRow {
                        action: String::new(),
                        label,
                    })
                }
            } else {
                Some(KbRow { action: cmd, label })
            }
        })
        .collect()
}

impl Settings {
    pub fn cfg_path() -> PathBuf {
        home_dir().join("cfg").join("config.cfg")
    }

    /// The game's default binds: `scripts/kb_def.lst` under `cfg/config_default.cfg`.
    pub fn default_binds(mount: &Mount) -> BTreeMap<String, String> {
        let mut binds = BTreeMap::new();
        if let Some(text) = read_text(mount, "scripts/kb_def.lst") {
            for (k, v) in quoted_pairs(&text) {
                binds.insert(k, v);
            }
        }
        if let Some(text) = read_text(mount, "cfg/config_default.cfg") {
            let mut cvars = BTreeMap::new();
            parse_cfg(&text, &mut binds, &mut cvars);
        }
        binds
    }

    /// Defaults, then the user's `config.cfg` over them.
    pub fn load(mount: &Mount) -> Settings {
        let mut s = Settings {
            binds: Self::default_binds(mount),
            ..Settings::default()
        };
        if let Ok(text) = std::fs::read_to_string(Self::cfg_path()) {
            let mut cvars = BTreeMap::new();
            parse_cfg(&text, &mut s.binds, &mut cvars);
            s.apply_cvars(&cvars);
        }
        s
    }

    fn apply_cvars(&mut self, cvars: &BTreeMap<String, String>) {
        let f = |k: &str, d: f32| {
            cvars
                .get(k)
                .and_then(|v| v.parse::<f32>().ok())
                .unwrap_or(d)
        };
        let b = |k: &str, d: bool| cvars.get(k).map_or(d, |v| v.trim() != "0");
        self.sensitivity = f("sensitivity", self.sensitivity);
        self.m_pitch_invert = f("m_pitch", 0.022) < 0.0;
        self.m_rawinput = b("m_rawinput", self.m_rawinput);
        self.m_filter = b("m_filter", self.m_filter);
        self.m_customaccel = b("m_customaccel", self.m_customaccel);
        self.m_customaccel_exponent = f("m_customaccel_exponent", self.m_customaccel_exponent);
        self.volume = f("volume", self.volume);
        self.snd_musicvolume = f("snd_musicvolume", self.snd_musicvolume);
        self.snd_mute_losefocus = b("snd_mute_losefocus", self.snd_mute_losefocus);
        self.dsp_enhance_stereo = b("dsp_enhance_stereo", self.dsp_enhance_stereo);
        self.closecaption = if b("closecaption", false) {
            if b("cc_subtitles", true) {
                1
            } else {
                2
            }
        } else {
            0
        };
        self.snd_surround_speakers = f("snd_surround_speakers", 2.0) as i32;
        self.snd_pitchquality = f("snd_pitchquality", 1.0) as u8;
        self.hud_fastswitch = b("hud_fastswitch", self.hud_fastswitch);
        self.cl_enable_loadingurl = b("cl_enable_loadingurl", self.cl_enable_loadingurl);
        if let Some(v) = cvars.get("cl_downloadfilter") {
            self.cl_downloadfilter = v.clone();
        }
        for (k, slot) in [
            ("cl_playermodel", &mut self.cl_playermodel),
            ("cl_playercolor", &mut self.cl_playercolor),
            ("cl_weaponcolor", &mut self.cl_weaponcolor),
        ] {
            if let Some(v) = cvars.get(k) {
                *slot = v.clone();
            }
        }
        self.cl_spraydisable = b("cl_spraydisable", self.cl_spraydisable);
        self.fov_desired = f("fov_desired", self.fov_desired).clamp(75.0, 100.0);
        self.fps_max = f("fps_max", self.fps_max).max(0.0);
        self.mat_picmip = (f("mat_picmip", self.mat_picmip as f32) as i32).clamp(-1, 2);
        self.r_rootlod = (f("r_rootlod", self.r_rootlod as f32) as i32).clamp(0, 2);
        self.r_waterforceexpensive = b("r_waterforceexpensive", self.r_waterforceexpensive);
        self.r_waterforcereflectentities = b(
            "r_waterforcereflectentities",
            self.r_waterforcereflectentities,
        );
        self.r_shadowrendertotexture = b("r_shadowrendertotexture", self.r_shadowrendertotexture);
        self.mat_colorcorrection = b("mat_colorcorrection", self.mat_colorcorrection);
        self.mat_antialias = (f("mat_antialias", self.mat_antialias as f32) as u32).clamp(1, 8);
        self.mat_trilinear = b("mat_trilinear", self.mat_trilinear);
        self.mat_forceaniso = (f("mat_forceaniso", self.mat_forceaniso as f32) as u32).clamp(1, 16);
        self.mat_vsync = b("mat_vsync", self.mat_vsync);
        self.mat_hdr_level = (f("mat_hdr_level", self.mat_hdr_level as f32) as u8).min(2);
        self.mat_motion_blur_enabled = b("mat_motion_blur_enabled", self.mat_motion_blur_enabled);
        self.mat_queue_mode = (f("mat_queue_mode", self.mat_queue_mode as f32) as i32).clamp(-1, 2);
        self.mat_bloomscale = b("mat_bloomscale", self.mat_bloomscale);
        self.mat_monitorgamma = f("mat_monitorgamma", self.mat_monitorgamma).clamp(1.6, 2.6);
        self.con_enable = b("con_enable", self.con_enable);
        self.width = f("mat_setvideomode_w", self.width as f32).max(320.0) as u32;
        self.height = f("mat_setvideomode_h", self.height as f32).max(240.0) as u32;
        self.fullscreen = b("fullscreen", self.fullscreen);
        self.voice_modenable = b("voice_modenable", self.voice_modenable);
        self.voice_scale = f("voice_scale", self.voice_scale);
        self.voice_loopback = b("voice_loopback", self.voice_loopback);
    }

    /// `config.cfg` the way the game writes it: `unbindall`, binds, then cvars.
    pub fn to_cfg(&self) -> String {
        let mut out = String::from("unbindall\n");
        for (k, v) in &self.binds {
            out.push_str(&format!("bind \"{k}\" \"{v}\"\n"));
        }
        let on = |b: bool| if b { "1" } else { "0" };
        let lines: Vec<(&str, String)> = vec![
            ("sensitivity", format!("{}", self.sensitivity)),
            (
                "m_pitch",
                if self.m_pitch_invert {
                    "-0.022"
                } else {
                    "0.022"
                }
                .to_string(),
            ),
            ("m_rawinput", on(self.m_rawinput).to_string()),
            ("m_filter", on(self.m_filter).to_string()),
            ("m_customaccel", on(self.m_customaccel).to_string()),
            (
                "m_customaccel_exponent",
                format!("{}", self.m_customaccel_exponent),
            ),
            ("volume", format!("{}", self.volume)),
            ("snd_musicvolume", format!("{}", self.snd_musicvolume)),
            (
                "snd_mute_losefocus",
                on(self.snd_mute_losefocus).to_string(),
            ),
            (
                "dsp_enhance_stereo",
                on(self.dsp_enhance_stereo).to_string(),
            ),
            ("closecaption", on(self.closecaption > 0).to_string()),
            ("cc_subtitles", on(self.closecaption != 2).to_string()),
            (
                "snd_surround_speakers",
                format!("{}", self.snd_surround_speakers),
            ),
            ("snd_pitchquality", format!("{}", self.snd_pitchquality)),
            ("hud_fastswitch", on(self.hud_fastswitch).to_string()),
            (
                "cl_enable_loadingurl",
                on(self.cl_enable_loadingurl).to_string(),
            ),
            ("cl_downloadfilter", self.cl_downloadfilter.clone()),
            ("cl_playermodel", self.cl_playermodel.clone()),
            ("cl_playercolor", self.cl_playercolor.clone()),
            ("cl_weaponcolor", self.cl_weaponcolor.clone()),
            ("cl_spraydisable", on(self.cl_spraydisable).to_string()),
            ("fov_desired", format!("{}", self.fov_desired)),
            ("fps_max", format!("{}", self.fps_max)),
            ("mat_picmip", format!("{}", self.mat_picmip)),
            ("r_rootlod", format!("{}", self.r_rootlod)),
            (
                "r_waterforceexpensive",
                on(self.r_waterforceexpensive).to_string(),
            ),
            (
                "r_waterforcereflectentities",
                on(self.r_waterforcereflectentities).to_string(),
            ),
            (
                "r_shadowrendertotexture",
                on(self.r_shadowrendertotexture).to_string(),
            ),
            (
                "mat_colorcorrection",
                on(self.mat_colorcorrection).to_string(),
            ),
            ("mat_antialias", format!("{}", self.mat_antialias)),
            ("mat_trilinear", on(self.mat_trilinear).to_string()),
            ("mat_forceaniso", format!("{}", self.mat_forceaniso)),
            ("mat_vsync", on(self.mat_vsync).to_string()),
            ("mat_hdr_level", format!("{}", self.mat_hdr_level)),
            (
                "mat_motion_blur_enabled",
                on(self.mat_motion_blur_enabled).to_string(),
            ),
            ("mat_queue_mode", format!("{}", self.mat_queue_mode)),
            ("mat_bloomscale", on(self.mat_bloomscale).to_string()),
            ("mat_monitorgamma", format!("{}", self.mat_monitorgamma)),
            ("con_enable", on(self.con_enable).to_string()),
            ("mat_setvideomode_w", format!("{}", self.width)),
            ("mat_setvideomode_h", format!("{}", self.height)),
            ("fullscreen", on(self.fullscreen).to_string()),
            ("voice_modenable", on(self.voice_modenable).to_string()),
            ("voice_scale", format!("{}", self.voice_scale)),
            ("voice_loopback", on(self.voice_loopback).to_string()),
        ];
        for (k, v) in lines {
            out.push_str(&format!("{k} \"{v}\"\n"));
        }
        out
    }

    pub fn save(&self) -> std::io::Result<()> {
        let path = Self::cfg_path();
        if let Some(dir) = path.parent() {
            std::fs::create_dir_all(dir)?;
        }
        std::fs::write(path, self.to_cfg())
    }

    /// The key(s) bound to a command, in the order the Keyboard tab shows them.
    pub fn keys_for(&self, command: &str) -> Vec<&str> {
        self.binds
            .iter()
            .filter(|(_, c)| c.as_str() == command)
            .map(|(k, _)| k.as_str())
            .collect()
    }

    /// `bind key command`: a key holds one command, the dialog keeps one key per command.
    pub fn bind(&mut self, key: &str, command: &str) {
        self.binds.retain(|_, c| c != command);
        self.binds.insert(key.to_string(), command.to_string());
    }

    pub fn unbind_command(&mut self, command: &str) {
        self.binds.retain(|_, c| c != command);
    }
}

/// Source key names of winit key codes (`bind` names, `kb_def.lst` spelling).
const KEY_NAMES: &[(KeyCode, &str)] = &[
    (KeyCode::KeyA, "a"),
    (KeyCode::KeyB, "b"),
    (KeyCode::KeyC, "c"),
    (KeyCode::KeyD, "d"),
    (KeyCode::KeyE, "e"),
    (KeyCode::KeyF, "f"),
    (KeyCode::KeyG, "g"),
    (KeyCode::KeyH, "h"),
    (KeyCode::KeyI, "i"),
    (KeyCode::KeyJ, "j"),
    (KeyCode::KeyK, "k"),
    (KeyCode::KeyL, "l"),
    (KeyCode::KeyM, "m"),
    (KeyCode::KeyN, "n"),
    (KeyCode::KeyO, "o"),
    (KeyCode::KeyP, "p"),
    (KeyCode::KeyQ, "q"),
    (KeyCode::KeyR, "r"),
    (KeyCode::KeyS, "s"),
    (KeyCode::KeyT, "t"),
    (KeyCode::KeyU, "u"),
    (KeyCode::KeyV, "v"),
    (KeyCode::KeyW, "w"),
    (KeyCode::KeyX, "x"),
    (KeyCode::KeyY, "y"),
    (KeyCode::KeyZ, "z"),
    (KeyCode::Digit0, "0"),
    (KeyCode::Digit1, "1"),
    (KeyCode::Digit2, "2"),
    (KeyCode::Digit3, "3"),
    (KeyCode::Digit4, "4"),
    (KeyCode::Digit5, "5"),
    (KeyCode::Digit6, "6"),
    (KeyCode::Digit7, "7"),
    (KeyCode::Digit8, "8"),
    (KeyCode::Digit9, "9"),
    (KeyCode::F1, "F1"),
    (KeyCode::F2, "F2"),
    (KeyCode::F3, "F3"),
    (KeyCode::F4, "F4"),
    (KeyCode::F5, "F5"),
    (KeyCode::F6, "F6"),
    (KeyCode::F7, "F7"),
    (KeyCode::F8, "F8"),
    (KeyCode::F9, "F9"),
    (KeyCode::F10, "F10"),
    (KeyCode::F11, "F11"),
    (KeyCode::F12, "F12"),
    (KeyCode::Space, "SPACE"),
    (KeyCode::Enter, "ENTER"),
    (KeyCode::Escape, "ESCAPE"),
    (KeyCode::Tab, "TAB"),
    (KeyCode::Backspace, "BACKSPACE"),
    (KeyCode::CapsLock, "CAPSLOCK"),
    (KeyCode::ShiftLeft, "SHIFT"),
    (KeyCode::ShiftRight, "RSHIFT"),
    (KeyCode::ControlLeft, "CTRL"),
    (KeyCode::ControlRight, "RCTRL"),
    (KeyCode::AltLeft, "ALT"),
    (KeyCode::AltRight, "RALT"),
    (KeyCode::ArrowUp, "UPARROW"),
    (KeyCode::ArrowDown, "DOWNARROW"),
    (KeyCode::ArrowLeft, "LEFTARROW"),
    (KeyCode::ArrowRight, "RIGHTARROW"),
    (KeyCode::Insert, "INS"),
    (KeyCode::Delete, "DEL"),
    (KeyCode::Home, "HOME"),
    (KeyCode::End, "END"),
    (KeyCode::PageUp, "PGUP"),
    (KeyCode::PageDown, "PGDN"),
    (KeyCode::Pause, "PAUSE"),
    (KeyCode::ScrollLock, "SCROLLLOCK"),
    (KeyCode::NumLock, "NUMLOCK"),
    (KeyCode::Backquote, "`"),
    (KeyCode::Minus, "-"),
    (KeyCode::Equal, "="),
    (KeyCode::BracketLeft, "["),
    (KeyCode::BracketRight, "]"),
    (KeyCode::Backslash, "\\"),
    (KeyCode::Semicolon, "SEMICOLON"),
    (KeyCode::Quote, "'"),
    (KeyCode::Comma, ","),
    (KeyCode::Period, "."),
    (KeyCode::Slash, "/"),
    (KeyCode::Numpad0, "KP_INS"),
    (KeyCode::Numpad1, "KP_END"),
    (KeyCode::Numpad2, "KP_DOWNARROW"),
    (KeyCode::Numpad3, "KP_PGDN"),
    (KeyCode::Numpad4, "KP_LEFTARROW"),
    (KeyCode::Numpad5, "KP_5"),
    (KeyCode::Numpad6, "KP_RIGHTARROW"),
    (KeyCode::Numpad7, "KP_HOME"),
    (KeyCode::Numpad8, "KP_UPARROW"),
    (KeyCode::Numpad9, "KP_PGUP"),
    (KeyCode::NumpadDecimal, "KP_DEL"),
    (KeyCode::NumpadDivide, "KP_SLASH"),
    (KeyCode::NumpadMultiply, "KP_MULTIPLY"),
    (KeyCode::NumpadSubtract, "KP_MINUS"),
    (KeyCode::NumpadAdd, "KP_PLUS"),
    (KeyCode::NumpadEnter, "KP_ENTER"),
];

pub fn key_name(code: KeyCode) -> Option<&'static str> {
    KEY_NAMES.iter().find(|(c, _)| *c == code).map(|(_, n)| *n)
}

pub fn key_code(name: &str) -> Option<KeyCode> {
    let name = if name == ";" { "SEMICOLON" } else { name };
    KEY_NAMES
        .iter()
        .find(|(_, n)| n.eq_ignore_ascii_case(name))
        .map(|(c, _)| *c)
}

/// `MOUSE1`..`MOUSE5` -> the client's button index (0 left, 1 right, 2 middle, 3 back,
/// 4 forward).
pub fn mouse_button(name: &str) -> Option<u8> {
    match name.to_ascii_uppercase().as_str() {
        "MOUSE1" => Some(0),
        "MOUSE2" => Some(1),
        "MOUSE3" => Some(2),
        "MOUSE4" => Some(3),
        "MOUSE5" => Some(4),
        _ => None,
    }
}

pub fn mouse_name(button: u8) -> &'static str {
    ["MOUSE1", "MOUSE2", "MOUSE3", "MOUSE4", "MOUSE5"]
        .get(button as usize)
        .copied()
        .unwrap_or("MOUSE5")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn cfg_round_trips() {
        let mut s = Settings::default();
        s.bind("w", "+forward");
        s.bind("MOUSE1", "+attack");
        s.sensitivity = 2.5;
        s.m_pitch_invert = true;
        s.closecaption = 2;
        s.fov_desired = 90.0;
        let text = s.to_cfg();
        let mut t = Settings::default();
        let mut cvars = BTreeMap::new();
        parse_cfg(&text, &mut t.binds, &mut cvars);
        t.apply_cvars(&cvars);
        assert_eq!(s, t);
    }

    #[test]
    fn bind_keeps_one_key_per_command() {
        let mut s = Settings::default();
        s.bind("w", "+forward");
        s.bind("UPARROW", "+forward");
        assert_eq!(s.keys_for("+forward"), vec!["UPARROW"]);
        s.unbind_command("+forward");
        assert!(s.keys_for("+forward").is_empty());
    }

    #[test]
    fn key_names_are_source_spelling() {
        assert_eq!(key_name(KeyCode::ControlLeft), Some("CTRL"));
        assert_eq!(key_code("mwheelup"), None);
        assert_eq!(key_code("KP_INS"), Some(KeyCode::Numpad0));
        assert_eq!(mouse_button("MOUSE2"), Some(1));
    }

    #[test]
    fn utf16_tokens_decode() {
        let mut bytes = vec![0xFF, 0xFE];
        for u in "\"Tokens\" { \"Valve_Jump\" \"Jump\" }".encode_utf16() {
            bytes.extend_from_slice(&u.to_le_bytes());
        }
        let text = decode_text(&bytes);
        let pairs = quoted_pairs(&text);
        assert_eq!(pairs, vec![("Valve_Jump".to_string(), "Jump".to_string())]);
    }
}
