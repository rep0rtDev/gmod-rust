//! The engine's Options dialog (`gamemenucommand OpenOptionsDialog`): a 512x406
//! PropertyDialog with the Keyboard / Mouse / Audio / Video / Voice / Multiplayer pages of
//! `resource/OptionsSub*.res`, drawn with GMod's `SourceScheme.res` colours. Positions are
//! the `.res` values inside the page.

use std::collections::BTreeMap;

use gm_render::ui::{FontMetrics, UiItem};

use crate::derma::{image, inside, rgba, text, Action, Ctx, Hit, Layout};
use crate::settings::{KbRow, Settings};

pub const FONT: &str = "SourceDefault";
pub const FONT_BOLD: &str = "SourceDefaultBold";
pub const FONT_LARGE: &str = "SourceDefaultLarge";

pub const DIALOG_W: f32 = 512.0;
pub const DIALOG_H: f32 = 406.0;
pub const TABS: [&str; 6] = [
    "#GameUI_Keyboard",
    "#GameUI_Mouse",
    "#GameUI_Audio",
    "#GameUI_Video",
    "#GameUI_Voice",
    "#GameUI_Multiplayer",
];

// SourceScheme (GMod)
const FRAME_BG: [f32; 4] = rgba(108, 111, 114, 250);
const TITLE_TEXT: [f32; 4] = rgba(255, 255, 255, 204);
const BORDER_BRIGHT: [f32; 4] = rgba(200, 200, 200, 255);
const BORDER_DARK: [f32; 4] = rgba(40, 40, 40, 198);
const LABEL_TEXT: [f32; 4] = rgba(221, 221, 221, 255);
const BUTTON_BG: [f32; 4] = rgba(227, 227, 227, 255);
const BUTTON_TEXT: [f32; 4] = rgba(82, 82, 82, 255);
const BUTTON_ARMED_BG: [f32; 4] = rgba(240, 240, 240, 255);
const BUTTON_ARMED_TEXT: [f32; 4] = rgba(46, 114, 178, 255);
const SHEET_TEXT: [f32; 4] = rgba(10, 10, 10, 200);
const SHEET_TEXT_SEL: [f32; 4] = rgba(10, 10, 10, 255);
const WHITE_BG: [f32; 4] = rgba(255, 255, 255, 255);
const DARK_TEXT: [f32; 4] = rgba(10, 10, 10, 255);
const ARROW: [f32; 4] = rgba(81, 81, 81, 255);
const LIST_HEADER: [f32; 4] = rgba(50, 50, 50, 255);
const LIST_DIVIDER: [f32; 4] = rgba(0, 0, 0, 150);
const LIST_SEL_BG: [f32; 4] = rgba(0, 168, 255, 204);
const SLIDER_TRACK: [f32; 4] = rgba(184, 184, 184, 229);
const SLIDER_NOB: [f32; 4] = rgba(220, 220, 220, 255);
const MENU_BG: [f32; 4] = rgba(233, 233, 233, 255);
const MENU_TEXT: [f32; 4] = rgba(80, 80, 80, 255);
const MENU_ARMED_BG: [f32; 4] = rgba(132, 183, 241, 255);
const SCROLL_BG: [f32; 4] = rgba(207, 207, 207, 255);
const CHECK: [f32; 4] = rgba(0, 0, 0, 255);

#[derive(Clone, Debug, PartialEq)]
pub enum OptAction {
    Tab(usize),
    Ok,
    Cancel,
    Apply,
    Close,
    KbRow(usize),
    EditKey,
    ClearKey,
    Defaults,
    /// Slider pressed: drag starts (name of the cvar).
    Slider(&'static str),
    Check(&'static str),
    /// Combo box opened / closed.
    Combo(&'static str),
    ComboItem(&'static str, usize),
    Button(&'static str),
    /// OK / Cancel of the open sub-dialog (Advanced video, Gamma).
    SubOk,
    SubCancel,
}

/// What the dialog is editing: a copy of the settings until OK / Apply.
#[derive(Clone, Debug)]
pub struct OptionsState {
    pub tab: usize,
    pub pending: Settings,
    pub kb_sel: Option<usize>,
    pub kb_scroll: f32,
    pub waiting_key: bool,
    pub combo: Option<&'static str>,
    pub aspect: Option<usize>,
    /// The open sub-dialog (`VideoAdvanced` / `Gamma`) and the pending settings to restore
    /// on its Cancel.
    pub sub: Option<&'static str>,
    pub sub_backup: Option<Settings>,
}

impl OptionsState {
    pub fn new(settings: &Settings) -> Self {
        OptionsState {
            tab: 0,
            pending: settings.clone(),
            kb_sel: None,
            kb_scroll: 0.0,
            waiting_key: false,
            combo: None,
            aspect: None,
            sub: None,
            sub_backup: None,
        }
    }
}

/// Slider ranges (`CCvarSlider:SetRange`).
pub fn slider_range(name: &str) -> (f32, f32) {
    match name {
        "sensitivity" => (0.1, 20.0),
        "m_customaccel_exponent" => (1.0, 1.4),
        "volume" | "snd_musicvolume" | "voice_scale" => (0.0, 1.0),
        "fov_desired" => (75.0, 100.0),
        "fps_max" => (30.0, 300.0),
        "mat_monitorgamma" => (1.6, 2.6),
        _ => (0.0, 1.0),
    }
}

pub fn slider_get(s: &Settings, name: &str) -> f32 {
    match name {
        "sensitivity" => s.sensitivity,
        "m_customaccel_exponent" => s.m_customaccel_exponent,
        "volume" => s.volume,
        "snd_musicvolume" => s.snd_musicvolume,
        "voice_scale" => s.voice_scale,
        "fov_desired" => s.fov_desired,
        "fps_max" => s.fps_max,
        "mat_monitorgamma" => s.mat_monitorgamma,
        _ => 0.0,
    }
}

pub fn slider_set(s: &mut Settings, name: &str, v: f32) {
    let (lo, hi) = slider_range(name);
    let v = v.clamp(lo, hi);
    match name {
        "sensitivity" => s.sensitivity = (v * 100.0).round() / 100.0,
        "m_customaccel_exponent" => s.m_customaccel_exponent = (v * 100.0).round() / 100.0,
        "volume" => s.volume = (v * 100.0).round() / 100.0,
        "snd_musicvolume" => s.snd_musicvolume = (v * 100.0).round() / 100.0,
        "voice_scale" => s.voice_scale = (v * 100.0).round() / 100.0,
        "fov_desired" => s.fov_desired = v.round(),
        "fps_max" => s.fps_max = v.round(),
        "mat_monitorgamma" => s.mat_monitorgamma = (v * 10.0).round() / 10.0,
        _ => {}
    }
}

pub fn check_get(s: &Settings, name: &str) -> bool {
    match name {
        "m_pitch" => s.m_pitch_invert,
        "m_filter" => s.m_filter,
        "m_rawinput" => s.m_rawinput,
        "m_customaccel" => s.m_customaccel,
        "snd_mute_losefocus" => s.snd_mute_losefocus,
        "dsp_enhance_stereo" => s.dsp_enhance_stereo,
        "hud_fastswitch" => s.hud_fastswitch,
        "cl_enable_loadingurl" => s.cl_enable_loadingurl,
        "cl_spraydisable" => s.cl_spraydisable,
        "voice_modenable" => s.voice_modenable,
        "voice_loopback" => s.voice_loopback,
        "mat_bloomscale" => s.mat_bloomscale,
        _ => false,
    }
}

pub fn check_toggle(s: &mut Settings, name: &str) {
    match name {
        "m_pitch" => s.m_pitch_invert = !s.m_pitch_invert,
        "m_filter" => s.m_filter = !s.m_filter,
        "m_rawinput" => s.m_rawinput = !s.m_rawinput,
        "m_customaccel" => s.m_customaccel = !s.m_customaccel,
        "snd_mute_losefocus" => s.snd_mute_losefocus = !s.snd_mute_losefocus,
        "dsp_enhance_stereo" => s.dsp_enhance_stereo = !s.dsp_enhance_stereo,
        "hud_fastswitch" => s.hud_fastswitch = !s.hud_fastswitch,
        "cl_enable_loadingurl" => s.cl_enable_loadingurl = !s.cl_enable_loadingurl,
        "cl_spraydisable" => s.cl_spraydisable = !s.cl_spraydisable,
        "voice_modenable" => s.voice_modenable = !s.voice_modenable,
        "voice_loopback" => s.voice_loopback = !s.voice_loopback,
        "mat_bloomscale" => s.mat_bloomscale = !s.mat_bloomscale,
        _ => {}
    }
}

/// Aspect ratio class of a mode (the Video page's filter).
fn aspect_of(w: u32, h: u32) -> usize {
    let r = w as f32 / h as f32;
    if (r - 16.0 / 9.0).abs() < 0.05 {
        1
    } else if (r - 16.0 / 10.0).abs() < 0.05 {
        2
    } else {
        0
    }
}

/// Combo box items and the selected index for the pending settings.
pub fn combo_items(
    st: &OptionsState,
    name: &str,
    modes: &[(u32, u32)],
    tok: &dyn Fn(&str) -> String,
) -> (Vec<String>, usize) {
    let s = &st.pending;
    // tokens the shipped gameui_english.txt lacks fall back to the game's visible text
    let tok_or = |key: &str, literal: &str| -> String {
        let t = tok(key);
        if t == key.trim_start_matches('#') {
            literal.to_string()
        } else {
            t
        }
    };
    match name {
        "Resolution" => {
            let aspect = st.aspect.unwrap_or_else(|| aspect_of(s.width, s.height));
            let list: Vec<(u32, u32)> = modes
                .iter()
                .copied()
                .filter(|&(w, h)| aspect_of(w, h) == aspect)
                .collect();
            let sel = list
                .iter()
                .position(|&(w, h)| w == s.width && h == s.height)
                .unwrap_or(0);
            (
                list.iter().map(|(w, h)| format!("{w} x {h}")).collect(),
                sel,
            )
        }
        "DisplayModeCombo" => (
            vec![tok("#GameUI_Fullscreen"), tok("#GameUI_Windowed")],
            usize::from(!s.fullscreen),
        ),
        "AspectRatio" => (
            vec![
                tok("#GameUI_AspectNormal"),
                tok("#GameUI_AspectWide16x9"),
                tok("#GameUI_AspectWide16x10"),
            ],
            st.aspect.unwrap_or_else(|| aspect_of(s.width, s.height)),
        ),
        "SpeakerSetup" => (
            vec![
                tok("#GameUI_Headphones"),
                tok("#GameUI_2Speakers"),
                tok("#GameUI_4Speakers"),
                tok("#GameUI_5Speakers"),
                tok("#GameUI_7Speakers"),
            ],
            match s.snd_surround_speakers {
                0 => 0,
                4 => 2,
                5 => 3,
                7 => 4,
                _ => 1,
            },
        ),
        "SoundQuality" => (
            vec![
                tok("#GameUI_Low"),
                tok("#GameUI_Medium"),
                tok("#GameUI_High"),
            ],
            (s.snd_pitchquality as usize).min(2),
        ),
        "CloseCaptionCheck" => (
            vec![
                tok("#GameUI_NoClosedCaptions"),
                tok("#GameUI_Subtitles"),
                tok("#GameUI_SubtitlesAndSoundEffects"),
            ],
            (s.closecaption as usize).min(2),
        ),
        "AudioSpokenLanguage" => (vec!["English".to_string()], 0),
        "ModelDetail" => (
            vec![
                tok("#GameUI_Low"),
                tok("#GameUI_Medium"),
                tok("#GameUI_High"),
            ],
            (2 - s.r_rootlod.clamp(0, 2)) as usize,
        ),
        "TextureDetail" => (
            vec![
                tok("#GameUI_Low"),
                tok("#GameUI_Medium"),
                tok("#GameUI_High"),
                tok_or("#GameUI_VeryHigh", "Very High"),
            ],
            (2 - s.mat_picmip.clamp(-1, 2)) as usize,
        ),
        "ShaderDetail" => (vec![tok("#GameUI_Low"), tok("#GameUI_High")], 1),
        "WaterDetail" => (
            vec![
                tok_or("#GameUI_Simple", "Simple Reflections"),
                tok_or("#GameUI_Reflect_World", "Reflect World"),
                tok_or("#GameUI_Reflect_All", "Reflect All"),
            ],
            match (s.r_waterforceexpensive, s.r_waterforcereflectentities) {
                (false, _) => 0,
                (true, false) => 1,
                (true, true) => 2,
            },
        ),
        "ShadowDetail" => (
            vec![tok("#GameUI_Low"), tok("#GameUI_High")],
            usize::from(s.r_shadowrendertotexture),
        ),
        "ColorCorrection" => (
            vec![tok("#GameUI_Disabled"), tok("#GameUI_Enabled")],
            usize::from(s.mat_colorcorrection),
        ),
        "AntialiasingMode" => (
            vec![
                tok("#GameUI_None"),
                tok("#GameUI_2X"),
                tok("#GameUI_4X"),
                tok("#GameUI_8X"),
            ],
            match s.mat_antialias {
                2 => 1,
                4 => 2,
                8 => 3,
                _ => 0,
            },
        ),
        "FilteringMode" => (
            vec![
                tok("#GameUI_Bilinear"),
                tok("#GameUI_Trilinear"),
                tok("#GameUI_Anisotropic2X"),
                tok("#GameUI_Anisotropic4X"),
                tok("#GameUI_Anisotropic8X"),
                tok("#GameUI_Anisotropic16X"),
            ],
            match (s.mat_trilinear, s.mat_forceaniso) {
                (false, 0 | 1) => 0,
                (true, 0 | 1) => 1,
                (_, 2) => 2,
                (_, 4) => 3,
                (_, 8) => 4,
                _ => 5,
            },
        ),
        "VSync" => (
            vec![tok("#GameUI_Disabled"), tok("#GameUI_Enabled")],
            usize::from(s.mat_vsync),
        ),
        "MotionBlur" => (
            vec![tok("#GameUI_Disabled"), tok("#GameUI_Enabled")],
            usize::from(s.mat_motion_blur_enabled),
        ),
        "HDR" => (
            vec![
                tok("#GameUI_hdr_level0"),
                tok("#GameUI_hdr_level1"),
                tok("#GameUI_hdr_level2"),
            ],
            (s.mat_hdr_level as usize).min(2),
        ),
        "Multicore" => (
            vec![tok("#GameUI_Disabled"), tok("#GameUI_Enabled")],
            usize::from(s.mat_queue_mode != 0),
        ),
        "D3D9Ex" => (vec![tok("#GameUI_Disabled"), tok("#GameUI_Enabled")], 1),
        "dxlabel" => (vec!["DirectX v9.0".to_string()], 0),
        "DownloadFilterCheck" => (
            vec![
                tok("#GameUI_DownloadFilter_ALL"),
                tok("#GameUI_DownloadFilter_NoSounds"),
                tok("#GameUI_DownloadFilter_MapsOnly"),
                tok("#GameUI_DownloadFilter_None"),
            ],
            match s.cl_downloadfilter.as_str() {
                "nosounds" => 1,
                "mapsonly" => 2,
                "none" => 3,
                _ => 0,
            },
        ),
        _ => (Vec::new(), 0),
    }
}

/// Apply a combo choice to the pending settings.
pub fn combo_pick(st: &mut OptionsState, name: &str, index: usize, modes: &[(u32, u32)]) {
    match name {
        "Resolution" => {
            let aspect = st
                .aspect
                .unwrap_or_else(|| aspect_of(st.pending.width, st.pending.height));
            if let Some(&(w, h)) = modes
                .iter()
                .filter(|&&(w, h)| aspect_of(w, h) == aspect)
                .nth(index)
            {
                st.pending.width = w;
                st.pending.height = h;
            }
        }
        "DisplayModeCombo" => st.pending.fullscreen = index == 0,
        "AspectRatio" => st.aspect = Some(index),
        "SpeakerSetup" => st.pending.snd_surround_speakers = [0, 2, 4, 5, 7][index.min(4)],
        "SoundQuality" => st.pending.snd_pitchquality = index.min(2) as u8,
        "CloseCaptionCheck" => st.pending.closecaption = index.min(2) as u8,
        "DownloadFilterCheck" => {
            st.pending.cl_downloadfilter =
                ["all", "nosounds", "mapsonly", "none"][index.min(3)].to_string()
        }
        "ModelDetail" => st.pending.r_rootlod = 2 - index.min(2) as i32,
        "TextureDetail" => st.pending.mat_picmip = 2 - index.min(3) as i32,
        "WaterDetail" => {
            st.pending.r_waterforceexpensive = index >= 1;
            st.pending.r_waterforcereflectentities = index >= 2;
        }
        "ShadowDetail" => st.pending.r_shadowrendertotexture = index == 1,
        "ColorCorrection" => st.pending.mat_colorcorrection = index == 1,
        "AntialiasingMode" => st.pending.mat_antialias = [1, 2, 4, 8][index.min(3)],
        "FilteringMode" => {
            st.pending.mat_trilinear = index >= 1;
            st.pending.mat_forceaniso = [1, 1, 2, 4, 8, 16][index.min(5)];
        }
        "VSync" => st.pending.mat_vsync = index == 1,
        "MotionBlur" => st.pending.mat_motion_blur_enabled = index == 1,
        "HDR" => st.pending.mat_hdr_level = index.min(2) as u8,
        "Multicore" => st.pending.mat_queue_mode = if index == 1 { -1 } else { 0 },
        _ => {}
    }
}

fn rect(items: &mut Vec<UiItem>, x: f32, y: f32, w: f32, h: f32, color: [f32; 4]) {
    items.push(UiItem::Rect { x, y, w, h, color });
}

/// Scheme border: `raised` = bright left/top and dark right/bottom, else the reverse.
fn border(items: &mut Vec<UiItem>, x: f32, y: f32, w: f32, h: f32, raised: bool) {
    let (a, b) = if raised {
        (BORDER_BRIGHT, BORDER_DARK)
    } else {
        (BORDER_DARK, BORDER_BRIGHT)
    };
    rect(items, x, y, w, 1.0, a);
    rect(items, x, y, 1.0, h, a);
    rect(items, x, y + h - 1.0, w, 1.0, b);
    rect(items, x + w - 1.0, y, 1.0, h, b);
}

pub struct Fonts<'a> {
    pub default: &'a FontMetrics,
    pub bold: &'a FontMetrics,
    pub large: &'a FontMetrics,
}

/// Slider bars laid out this frame, for dragging: `(cvar, x0, x1)`.
pub type SliderBars = Vec<(&'static str, f32, f32)>;

struct Page<'a> {
    x: f32,
    y: f32,
    cursor: (f32, f32),
    fonts: &'a Fonts<'a>,
    st: &'a OptionsState,
    tok: &'a dyn Fn(&str) -> String,
    modes: &'a [(u32, u32)],
    bars: SliderBars,
    /// Open combo list drawn last, over the page.
    popup: Vec<UiItem>,
    popup_hits: Vec<Hit>,
}

impl Page<'_> {
    /// Label: `\n` breaks and word wrap to its width (the game's labels wrap too).
    fn label(&self, out: &mut Layout, x: f32, y: f32, w: f32, h: f32, key: &str) {
        let s = (self.tok)(key);
        let fm = self.fonts.default;
        let mut lines: Vec<String> = Vec::new();
        for para in s.split("\\n") {
            let mut cur = String::new();
            for word in para.split_whitespace() {
                let next = if cur.is_empty() {
                    word.to_string()
                } else {
                    format!("{cur} {word}")
                };
                if !cur.is_empty() && fm.text_width(&next) > w {
                    lines.push(std::mem::take(&mut cur));
                    cur = word.to_string();
                } else {
                    cur = next;
                }
            }
            lines.push(cur);
        }
        let lh = fm.line_height;
        let total = lh * lines.len() as f32;
        let top = self.y + y + ((h - total) / 2.0).max(0.0);
        for (i, line) in lines.iter().enumerate() {
            text(
                &mut out.items,
                FONT,
                fm,
                self.x + x,
                top + i as f32 * lh,
                lh,
                line,
                LABEL_TEXT,
            );
        }
    }

    #[allow(clippy::too_many_arguments)]
    fn button(
        &self,
        out: &mut Layout,
        x: f32,
        y: f32,
        w: f32,
        h: f32,
        key: &str,
        action: OptAction,
    ) {
        let (bx, by) = (self.x + x, self.y + y);
        let hovered = inside(self.cursor, bx, by, w, h);
        rect(
            &mut out.items,
            bx,
            by,
            w,
            h,
            if hovered { BUTTON_ARMED_BG } else { BUTTON_BG },
        );
        border(&mut out.items, bx, by, w, h, true);
        let s = (self.tok)(key);
        let tw = self.fonts.default.text_width(&s);
        text(
            &mut out.items,
            FONT,
            self.fonts.default,
            (bx + (w - tw) / 2.0).round(),
            by,
            h,
            &s,
            if hovered {
                BUTTON_ARMED_TEXT
            } else {
                BUTTON_TEXT
            },
        );
        out.hits.push(Hit {
            x: bx,
            y: by,
            w,
            h,
            action: Action::Opt(action),
        });
    }

    /// CheckButton: 13 px box at the left, label 20 px in.
    #[allow(clippy::too_many_arguments)]
    fn check(
        &self,
        out: &mut Layout,
        x: f32,
        y: f32,
        w: f32,
        h: f32,
        key: &str,
        cvar: &'static str,
    ) {
        let (bx, by) = (self.x + x, self.y + y);
        let box_y = by + ((h - 13.0) / 2.0).floor();
        rect(&mut out.items, bx, box_y, 13.0, 13.0, WHITE_BG);
        border(&mut out.items, bx, box_y, 13.0, 13.0, false);
        if check_get(&self.st.pending, cvar) {
            // the Marlett check glyph, as two strokes
            rect(&mut out.items, bx + 3.0, box_y + 6.0, 2.0, 4.0, CHECK);
            rect(&mut out.items, bx + 5.0, box_y + 8.0, 2.0, 2.0, CHECK);
            rect(&mut out.items, bx + 7.0, box_y + 6.0, 2.0, 2.0, CHECK);
            rect(&mut out.items, bx + 9.0, box_y + 4.0, 2.0, 2.0, CHECK);
            rect(&mut out.items, bx + 5.0, box_y + 7.0, 2.0, 2.0, CHECK);
        }
        let s = (self.tok)(key);
        text(
            &mut out.items,
            FONT,
            self.fonts.default,
            bx + 20.0,
            by,
            h,
            &s,
            LABEL_TEXT,
        );
        out.hits.push(Hit {
            x: bx,
            y: by,
            w,
            h,
            action: Action::Opt(OptAction::Check(cvar)),
        });
    }

    /// CCvarSlider: a 2 px track with the nob (8x16) at the value.
    fn slider(&mut self, out: &mut Layout, x: f32, y: f32, w: f32, h: f32, cvar: &'static str) {
        let (bx, by) = (self.x + x, self.y + y);
        let (lo, hi) = slider_range(cvar);
        let v = slider_get(&self.st.pending, cvar);
        let f = ((v - lo) / (hi - lo)).clamp(0.0, 1.0);
        let (x0, x1) = (bx + 4.0, bx + w - 4.0);
        let track_y = by + (h / 2.0).floor() - 1.0;
        rect(&mut out.items, x0, track_y, x1 - x0, 2.0, SLIDER_TRACK);
        border(
            &mut out.items,
            x0 - 1.0,
            track_y - 1.0,
            x1 - x0 + 2.0,
            4.0,
            false,
        );
        let nob_x = (x0 + f * (x1 - x0) - 4.0).round();
        rect(&mut out.items, nob_x, track_y - 7.0, 8.0, 16.0, SLIDER_NOB);
        border(&mut out.items, nob_x, track_y - 7.0, 8.0, 16.0, true);
        self.bars.push((cvar, x0, x1));
        out.hits.push(Hit {
            x: bx,
            y: by,
            w,
            h,
            action: Action::Opt(OptAction::Slider(cvar)),
        });
    }

    /// TextEntry showing a value (the game's are editable; ours mirror the slider).
    fn entry(&self, out: &mut Layout, x: f32, y: f32, w: f32, h: f32, value: &str) {
        let (bx, by) = (self.x + x, self.y + y);
        rect(&mut out.items, bx, by, w, h, WHITE_BG);
        border(&mut out.items, bx, by, w, h, false);
        text(
            &mut out.items,
            FONT,
            self.fonts.default,
            bx + 4.0,
            by,
            h,
            value,
            DARK_TEXT,
        );
    }

    /// ComboBox: white field, the arrow button at the right, the open list below it.
    fn combo(&mut self, out: &mut Layout, x: f32, y: f32, w: f32, h: f32, name: &'static str) {
        let (bx, by) = (self.x + x, self.y + y);
        let (items, sel) = combo_items(self.st, name, self.modes, self.tok);
        rect(&mut out.items, bx, by, w, h, WHITE_BG);
        border(&mut out.items, bx, by, w, h, false);
        if let Some(s) = items.get(sel) {
            text(
                &mut out.items,
                FONT,
                self.fonts.default,
                bx + 4.0,
                by,
                h,
                s,
                DARK_TEXT,
            );
        }
        // the Marlett down arrow
        let ax = bx + w - 13.0;
        let ay = by + (h / 2.0).floor() - 2.0;
        for i in 0..4 {
            rect(
                &mut out.items,
                ax + i as f32,
                ay + i as f32,
                7.0 - 2.0 * i as f32,
                1.0,
                ARROW,
            );
        }
        out.hits.push(Hit {
            x: bx,
            y: by,
            w,
            h,
            action: Action::Opt(OptAction::Combo(name)),
        });
        if self.st.combo == Some(name) {
            let row = 20.0;
            let lh = row * items.len() as f32;
            let ly = by + h;
            rect(&mut self.popup, bx, ly, w, lh + 2.0, MENU_BG);
            border(&mut self.popup, bx, ly, w, lh + 2.0, true);
            for (i, s) in items.iter().enumerate() {
                let ry = ly + 1.0 + i as f32 * row;
                let hovered = inside(self.cursor, bx + 1.0, ry, w - 2.0, row);
                if hovered {
                    rect(&mut self.popup, bx + 1.0, ry, w - 2.0, row, MENU_ARMED_BG);
                }
                text(
                    &mut self.popup,
                    FONT,
                    self.fonts.default,
                    bx + 6.0,
                    ry,
                    row,
                    s,
                    if hovered { WHITE_BG } else { MENU_TEXT },
                );
                self.popup_hits.push(Hit {
                    x: bx + 1.0,
                    y: ry,
                    w: w - 2.0,
                    h: row,
                    action: Action::Opt(OptAction::ComboItem(name, i)),
                });
            }
        }
    }

    fn divider(&self, out: &mut Layout, x: f32, y: f32, w: f32) {
        rect(&mut out.items, self.x + x, self.y + y, w, 1.0, BORDER_DARK);
        rect(
            &mut out.items,
            self.x + x,
            self.y + y + 1.0,
            w,
            1.0,
            BORDER_BRIGHT,
        );
    }

    /// SectionedListPanel of the Keyboard page: white, 20 px rows, header rows in bold,
    /// the key column at 60 %.
    fn keybind_list(&self, out: &mut Layout, x: f32, y: f32, w: f32, h: f32, rows: &[KbRow]) {
        let (bx, by) = (self.x + x, self.y + y);
        rect(&mut out.items, bx, by, w, h, WHITE_BG);
        border(&mut out.items, bx, by, w, h, false);
        let row_h = 20.0;
        let col2 = bx + (w * 0.6).round();
        let list_w = w - 15.0;
        // column headers
        text(
            &mut out.items,
            FONT_BOLD,
            self.fonts.bold,
            bx + 5.0,
            by + 1.0,
            row_h,
            &(self.tok)("#GameUI_Action"),
            LIST_HEADER,
        );
        text(
            &mut out.items,
            FONT_BOLD,
            self.fonts.bold,
            col2,
            by + 1.0,
            row_h,
            &(self.tok)("#GameUI_KeyButton"),
            LIST_HEADER,
        );
        rect(
            &mut out.items,
            bx + 1.0,
            by + row_h,
            list_w - 1.0,
            1.0,
            LIST_DIVIDER,
        );
        let top = by + row_h + 2.0;
        let view_h = h - row_h - 3.0;
        let total = rows.len() as f32 * row_h;
        let max_scroll = (total - view_h).max(0.0);
        let scroll = self.st.kb_scroll.clamp(0.0, max_scroll);
        for (i, r) in rows.iter().enumerate() {
            let ry = top + i as f32 * row_h - scroll;
            if ry + row_h <= top || ry >= top + view_h {
                continue;
            }
            let label = (self.tok)(&r.label);
            if r.action.is_empty() {
                text(
                    &mut out.items,
                    FONT_BOLD,
                    self.fonts.bold,
                    bx + 5.0,
                    ry,
                    row_h,
                    &label,
                    LIST_HEADER,
                );
                rect(
                    &mut out.items,
                    bx + 1.0,
                    ry + row_h - 1.0,
                    list_w - 1.0,
                    1.0,
                    LIST_DIVIDER,
                );
                continue;
            }
            let selected = self.st.kb_sel == Some(i);
            if selected {
                rect(
                    &mut out.items,
                    bx + 1.0,
                    ry,
                    list_w - 1.0,
                    row_h,
                    LIST_SEL_BG,
                );
            }
            let color = if selected { WHITE_BG } else { DARK_TEXT };
            text(
                &mut out.items,
                FONT,
                self.fonts.default,
                bx + 5.0,
                ry,
                row_h,
                &label,
                color,
            );
            let keys = if selected && self.st.waiting_key {
                "...".to_string()
            } else {
                self.st.pending.keys_for(&r.action).join(", ")
            };
            text(
                &mut out.items,
                FONT,
                self.fonts.default,
                col2,
                ry,
                row_h,
                &keys,
                color,
            );
            out.hits.push(Hit {
                x: bx + 1.0,
                y: ry,
                w: list_w - 1.0,
                h: row_h,
                action: Action::Opt(OptAction::KbRow(i)),
            });
        }
        // scrollbar (ScrollBar.Wide 15)
        let sx = bx + w - 15.0;
        rect(&mut out.items, sx, by + 1.0, 14.0, h - 2.0, SCROLL_BG);
        if max_scroll > 0.0 {
            let frac = view_h / total;
            let nob_h = (frac * (h - 2.0)).max(16.0);
            let nob_y = by + 1.0 + (scroll / max_scroll) * (h - 2.0 - nob_h);
            rect(&mut out.items, sx + 1.0, nob_y, 12.0, nob_h, SLIDER_NOB);
            border(&mut out.items, sx + 1.0, nob_y, 12.0, nob_h, true);
        }
    }
}

/// Lays the dialog out centred on the screen; returns the slider bars for dragging.
#[allow(clippy::too_many_arguments)]
pub fn dialog(
    out: &mut Layout,
    ctx: &Ctx,
    st: &OptionsState,
    rows: &[KbRow],
    tokens: &BTreeMap<String, String>,
    modes: &[(u32, u32)],
    fonts: &Fonts,
) -> SliderBars {
    let tok = |key: &str| -> String {
        let k = key.trim_start_matches('#');
        tokens
            .get(&k.to_ascii_lowercase())
            .cloned()
            .unwrap_or_else(|| k.to_string())
    };
    let (w, h) = (DIALOG_W, DIALOG_H);
    let x = ((ctx.w - w) / 2.0).round();
    let y = ((ctx.h - h) / 2.0).round();
    // modal: swallow clicks outside
    out.hits.push(Hit {
        x: 0.0,
        y: 0.0,
        w: ctx.w,
        h: ctx.h,
        action: Action::Nothing,
    });
    rect(&mut out.items, x, y, w, h, FRAME_BG);
    border(&mut out.items, x, y, w, h, true);
    // title: inset 16, the close button at the right
    text(
        &mut out.items,
        FONT_LARGE,
        fonts.large,
        x + 16.0,
        y + 4.0,
        24.0,
        &tok("#GameUI_Options"),
        TITLE_TEXT,
    );
    {
        let (cx, cy) = (x + w - 8.0 - 18.0, y + 6.0);
        let hovered = inside(ctx.cursor, cx, cy, 18.0, 18.0);
        let c = if hovered { WHITE_BG } else { TITLE_TEXT };
        for i in 0..9 {
            rect(
                &mut out.items,
                cx + 4.0 + i as f32,
                cy + 4.0 + i as f32,
                2.0,
                2.0,
                c,
            );
            rect(
                &mut out.items,
                cx + 12.0 - i as f32,
                cy + 4.0 + i as f32,
                2.0,
                2.0,
                c,
            );
        }
        out.hits.push(Hit {
            x: cx,
            y: cy,
            w: 18.0,
            h: 18.0,
            action: Action::Opt(OptAction::Close),
        });
    }
    // OK / Cancel / Apply, 72x24, bottom right
    let by = y + h - 8.0 - 24.0;
    let mut page = Page {
        x: 0.0,
        y: 0.0,
        cursor: ctx.cursor,
        fonts,
        st,
        tok: &tok,
        modes,
        bars: Vec::new(),
        popup: Vec::new(),
        popup_hits: Vec::new(),
    };
    for (i, (key, act)) in [
        ("#GameUI_OK", OptAction::Ok),
        ("#GameUI_Cancel", OptAction::Cancel),
        ("#GameUI_Apply", OptAction::Apply),
    ]
    .into_iter()
    .enumerate()
    {
        let bx = x + w - 8.0 - 72.0 * (3 - i) as f32 - 8.0 * (2 - i) as f32;
        page.button(out, bx, by, 72.0, 24.0, key, act);
    }
    // PropertySheet: tabs 28 tall, page below with a raised border
    let sx = x + 8.0;
    let sy = y + 32.0;
    let sw = w - 16.0;
    let sh = by - 8.0 - sy;
    let tab_h = 28.0;
    let mut tx = sx;
    for (i, key) in TABS.iter().enumerate() {
        let s = tok(key);
        let tw = fonts.default.text_width(&s) + 20.0;
        let active = i == st.tab;
        let (ty, th) = if active {
            (sy, tab_h + 1.0)
        } else {
            (sy + 2.0, tab_h - 2.0)
        };
        rect(&mut out.items, tx, ty, tw, th, FRAME_BG);
        rect(&mut out.items, tx, ty, tw, 1.0, BORDER_BRIGHT);
        rect(&mut out.items, tx, ty, 1.0, th, BORDER_BRIGHT);
        rect(&mut out.items, tx + tw - 1.0, ty, 1.0, th, BORDER_DARK);
        text(
            &mut out.items,
            FONT,
            fonts.default,
            tx + 10.0,
            ty,
            th - 1.0,
            &s,
            if active { SHEET_TEXT_SEL } else { SHEET_TEXT },
        );
        out.hits.push(Hit {
            x: tx,
            y: ty,
            w: tw,
            h: th,
            action: Action::Opt(OptAction::Tab(i)),
        });
        tx += tw + 1.0;
    }
    let py = sy + tab_h;
    let ph = sh - tab_h;
    border(&mut out.items, sx, py, sw, ph, true);
    // the active tab opens into the page: no top border under it
    page.x = sx + 2.0;
    page.y = py + 2.0;
    match st.tab {
        0 => {
            page.keybind_list(out, 8.0, 10.0, 480.0, 258.0, rows);
            page.button(
                out,
                8.0,
                276.0,
                134.0,
                24.0,
                "#GameUI_UseDefaults",
                OptAction::Defaults,
            );
            page.button(
                out,
                148.0,
                276.0,
                111.0,
                24.0,
                "#GameUI_AdvancedEllipsis",
                OptAction::Button("KeyAdvanced"),
            );
            page.button(
                out,
                272.0,
                276.0,
                106.0,
                24.0,
                "#GameUI_SetNewKey",
                OptAction::EditKey,
            );
            page.button(
                out,
                384.0,
                276.0,
                105.0,
                24.0,
                "#GameUI_ClearKey",
                OptAction::ClearKey,
            );
        }
        1 => {
            page.check(
                out,
                28.0,
                9.0,
                150.0,
                28.0,
                "#GameUI_ReverseMouse",
                "m_pitch",
            );
            page.check(
                out,
                200.0,
                10.0,
                150.0,
                28.0,
                "#GameUI_MouseFilter",
                "m_filter",
            );
            page.label(out, 35.0, 40.0, 148.0, 24.0, "#GameUI_MouseSensitivity");
            page.slider(out, 200.0, 40.0, 200.0, 40.0, "sensitivity");
            page.entry(
                out,
                400.0,
                40.0,
                50.0,
                24.0,
                &format!("{:.2}", st.pending.sensitivity),
            );
            page.check(
                out,
                30.0,
                70.0,
                150.0,
                24.0,
                "#GameUI_MouseRaw",
                "m_rawinput",
            );
            page.check(
                out,
                30.0,
                100.0,
                150.0,
                24.0,
                "#GameUI_MouseAcceleration",
                "m_customaccel",
            );
            page.slider(out, 200.0, 100.0, 200.0, 40.0, "m_customaccel_exponent");
            page.entry(
                out,
                400.0,
                100.0,
                50.0,
                24.0,
                &format!("{:.2}", st.pending.m_customaccel_exponent),
            );
            page.divider(out, 35.0, 140.0, 396.0);
            page.label(out, 28.0, 148.0, 226.0, 28.0, "#GameUI_JoystickLabel");
        }
        2 => {
            page.label(out, 36.0, 19.0, 160.0, 24.0, "#GameUI_SoundEffectVolume");
            page.slider(out, 36.0, 38.0, 393.0, 50.0, "volume");
            page.label(out, 36.0, 68.0, 160.0, 24.0, "#GameUI_SoundFXVolume");
            page.slider(out, 36.0, 88.0, 175.0, 50.0, "volume");
            page.label(out, 254.0, 68.0, 160.0, 24.0, "#GameUI_MusicVolume");
            page.slider(out, 254.0, 88.0, 175.0, 50.0, "snd_musicvolume");
            page.label(
                out,
                36.0,
                120.0,
                184.0,
                24.0,
                "#GameUI_SpeakerConfiguration",
            );
            page.combo(out, 36.0, 145.0, 175.0, 24.0, "SpeakerSetup");
            page.label(out, 254.0, 120.0, 179.0, 24.0, "#GameUI_SoundQuality");
            page.combo(out, 254.0, 145.0, 175.0, 24.0, "SoundQuality");
            page.label(out, 36.0, 176.0, 175.0, 24.0, "#GAMEUI_AudioSpokenLanguage");
            page.combo(out, 36.0, 200.0, 175.0, 24.0, "AudioSpokenLanguage");
            page.label(out, 36.0, 220.0, 175.0, 24.0, "#GAMEUI_RequiresRestart");
            page.label(out, 254.0, 176.0, 175.0, 24.0, "#GameUI_Captioning");
            page.combo(out, 254.0, 200.0, 175.0, 24.0, "CloseCaptionCheck");
            page.check(
                out,
                33.0,
                240.0,
                430.0,
                28.0,
                "#GameUI_SndMuteLoseFocus",
                "snd_mute_losefocus",
            );
            page.check(
                out,
                33.0,
                265.0,
                210.0,
                28.0,
                "#GameUI_DspEnhanceStereo",
                "dsp_enhance_stereo",
            );
        }
        3 => {
            page.label(out, 36.0, 24.0, 160.0, 24.0, "#GameUI_Resolution");
            page.combo(out, 36.0, 50.0, 160.0, 24.0, "Resolution");
            page.label(out, 254.0, 24.0, 160.0, 24.0, "#GameUI_DisplayMode");
            page.combo(out, 254.0, 50.0, 182.0, 24.0, "DisplayModeCombo");
            page.label(out, 36.0, 100.0, 160.0, 24.0, "#GameUI_AspectRatio");
            page.combo(out, 36.0, 126.0, 160.0, 24.0, "AspectRatio");
            page.label(out, 257.0, 117.0, 140.0, 24.0, "#GameUI_Gamma");
            page.button(
                out,
                254.0,
                126.0,
                182.0,
                24.0,
                "#GameUI_AdjustGamma",
                OptAction::Button("Gamma"),
            );
            page.label(out, 36.0, 176.0, 168.0, 24.0, "#GameUI_FPS");
            page.slider(out, 36.0, 196.0, 168.0, 50.0, "fps_max");
            page.entry(
                out,
                207.0,
                196.0,
                36.0,
                24.0,
                &format!("{}", st.pending.fps_max),
            );
            page.button(
                out,
                254.0,
                196.0,
                182.0,
                24.0,
                "#GameUI_AdvancedEllipsis",
                OptAction::Button("VideoAdvanced"),
            );
            page.button(
                out,
                39.0,
                230.0,
                130.0,
                24.0,
                "#GameUI_LaunchBenchmarkVideoPanel",
                OptAction::Button("Benchmark"),
            );
            page.label(
                out,
                254.0,
                254.0,
                225.0,
                24.0,
                "#GameUI_ThirdPartyTechCredits",
            );
        }
        4 => {
            page.check(
                out,
                33.0,
                32.0,
                216.0,
                28.0,
                "#GameUI_EnableVoice",
                "voice_modenable",
            );
            page.label(out, 42.0, 70.0, 180.0, 24.0, "#GameUI_VoiceTransmitVolume");
            page.slider(out, 40.0, 94.0, 160.0, 26.0, "voice_scale");
            page.label(out, 246.0, 70.0, 180.0, 24.0, "#GameUI_VoiceReceiveVolume");
            page.slider(out, 246.0, 94.0, 160.0, 42.0, "voice_scale");
            rect(
                &mut out.items,
                page.x + 40.0,
                page.y + 148.0,
                158.0,
                32.0,
                rgba(0, 0, 0, 128),
            );
            border(
                &mut out.items,
                page.x + 40.0,
                page.y + 148.0,
                158.0,
                32.0,
                false,
            );
            page.button(
                out,
                40.0,
                176.0,
                160.0,
                24.0,
                "#GameUI_TestMicrophone",
                OptAction::Button("TestMicrophone"),
            );
            page.button(
                out,
                246.0,
                176.0,
                160.0,
                24.0,
                "#GameUI_SteamVoiceSettings",
                OptAction::Button("SteamVoiceSettings"),
            );
            page.check(
                out,
                33.0,
                212.0,
                216.0,
                28.0,
                "#GameUI_BoostMicrophone",
                "voice_loopback",
            );
        }
        _ => {
            page.label(out, 32.0, 14.0, 150.0, 24.0, "#GameUI_SpraypaintImage");
            rect(
                &mut out.items,
                page.x + 32.0,
                page.y + 38.0,
                64.0,
                64.0,
                rgba(0, 0, 0, 128),
            );
            border(
                &mut out.items,
                page.x + 32.0,
                page.y + 38.0,
                64.0,
                64.0,
                false,
            );
            if (ctx.has_image)("vgui/logos/garrysmod.png") {
                image(
                    &mut out.items,
                    "vgui/logos/garrysmod.png",
                    page.x + 33.0,
                    page.y + 39.0,
                    62.0,
                    62.0,
                );
            }
            page.button(
                out,
                106.0,
                78.0,
                124.0,
                24.0,
                "#GameUI_ImportSprayEllipsis",
                OptAction::Button("ImportSpray"),
            );
            page.label(
                out,
                246.0,
                38.0,
                214.0,
                24.0,
                "#GameUI_SpraypaintServerNote",
            );
            page.check(
                out,
                240.0,
                74.0,
                250.0,
                28.0,
                "#GameUI_DisableSprays",
                "cl_spraydisable",
            );
            page.divider(out, 32.0, 110.0, 432.0);
            page.label(
                out,
                32.0,
                115.0,
                260.0,
                24.0,
                "#GameUI_DownloadFilter_Title",
            );
            page.combo(out, 32.0, 155.0, 300.0, 24.0, "DownloadFilterCheck");
            page.divider(out, 32.0, 190.0, 432.0);
            page.check(
                out,
                25.0,
                195.0,
                250.0,
                22.0,
                "#GameUI_HTMLLoadingScreen",
                "cl_enable_loadingurl",
            );
            page.check(
                out,
                25.0,
                220.0,
                250.0,
                22.0,
                "#GameUI_FastSwitchCheck",
                "hud_fastswitch",
            );
            page.button(
                out,
                32.0,
                245.0,
                250.0,
                24.0,
                "#GameUI_CrosshairCustomize",
                OptAction::Button("Crosshair"),
            );
            page.label(out, 296.0, 205.0, 176.0, 24.0, "#GameUI_FOV");
            page.slider(out, 296.0, 225.0, 176.0, 50.0, "fov_desired");
            page.entry(
                out,
                440.0,
                262.0,
                36.0,
                20.0,
                &format!("{}", st.pending.fov_desired),
            );
        }
    }
    let Page {
        mut bars,
        popup,
        popup_hits,
        ..
    } = page;
    out.items.extend(popup);
    // the open list is on top: `Layout::hit` takes the last region, so its hits go last
    out.hits.extend(popup_hits);
    if let Some(sub) = st.sub {
        bars.extend(sub_dialog(out, ctx, st, sub, &tok, modes, fonts));
    }
    bars
}

/// A frame like the Options one: background, border, title and the close button.
#[allow(clippy::too_many_arguments)]
fn frame(
    out: &mut Layout,
    ctx: &Ctx,
    fonts: &Fonts,
    x: f32,
    y: f32,
    w: f32,
    h: f32,
    title: &str,
    close: OptAction,
) {
    out.hits.push(Hit {
        x: 0.0,
        y: 0.0,
        w: ctx.w,
        h: ctx.h,
        action: Action::Nothing,
    });
    rect(&mut out.items, x, y, w, h, FRAME_BG);
    border(&mut out.items, x, y, w, h, true);
    text(
        &mut out.items,
        FONT_LARGE,
        fonts.large,
        x + 16.0,
        y + 4.0,
        24.0,
        title,
        TITLE_TEXT,
    );
    let (cx, cy) = (x + w - 8.0 - 18.0, y + 6.0);
    let hovered = inside(ctx.cursor, cx, cy, 18.0, 18.0);
    let c = if hovered { WHITE_BG } else { TITLE_TEXT };
    for i in 0..9 {
        rect(
            &mut out.items,
            cx + 4.0 + i as f32,
            cy + 4.0 + i as f32,
            2.0,
            2.0,
            c,
        );
        rect(
            &mut out.items,
            cx + 12.0 - i as f32,
            cy + 4.0 + i as f32,
            2.0,
            2.0,
            c,
        );
    }
    out.hits.push(Hit {
        x: cx,
        y: cy,
        w: 18.0,
        h: 18.0,
        action: Action::Opt(close),
    });
}

/// `OptionsSubVideoAdvancedDlg.res` (482 wide; the game lays the OK / Cancel row out at the
/// bottom in code) and `optionssubvideogammadlg.res` (290 x 396), centred over Options.
fn sub_dialog(
    out: &mut Layout,
    ctx: &Ctx,
    st: &OptionsState,
    sub: &str,
    tok: &dyn Fn(&str) -> String,
    modes: &[(u32, u32)],
    fonts: &Fonts,
) -> SliderBars {
    let (w, h) = if sub == "Gamma" {
        (290.0, 396.0)
    } else {
        (482.0, 470.0)
    };
    let x = ((ctx.w - w) / 2.0).round();
    let y = ((ctx.h - h) / 2.0).round();
    let title = if sub == "Gamma" {
        tok("#GameUI_AdjustGamma_Title")
    } else {
        tok("#GameUI_VideoAdvanced_Title")
    };
    frame(out, ctx, fonts, x, y, w, h, &title, OptAction::SubCancel);
    let mut page = Page {
        x,
        y,
        cursor: ctx.cursor,
        fonts,
        st,
        tok,
        modes,
        bars: Vec::new(),
        popup: Vec::new(),
        popup_hits: Vec::new(),
    };
    if sub == "Gamma" {
        page.label(out, 16.0, 36.0, 264.0, 53.0, "#GameUI_GammaInfo");
        page.label(out, 16.0, 88.0, 108.0, 24.0, "#GameUI_Gamma");
        page.slider(out, 17.0, 112.0, 200.0, 40.0, "mat_monitorgamma");
        page.entry(
            out,
            228.0,
            114.0,
            44.0,
            24.0,
            &format!("{:.1}", st.pending.mat_monitorgamma),
        );
        // the test image: three lines of text on black at 3 %, 6 % and 10 % grey, the
        // brightness the slider's gamma gives them on screen
        let (ix, iy, iw, ih) = (x + 16.0, y + 156.0, 256.0, 192.0);
        rect(&mut out.items, ix, iy, iw, ih, rgba(0, 0, 0, 255));
        let g = st.pending.mat_monitorgamma;
        let info = tok("#GameUI_GammaInfo");
        let mut line = info.split('.').next().unwrap_or("").to_string();
        while fonts.large.text_width(&line) > iw - 48.0 && !line.is_empty() {
            line.pop();
        }
        for (i, level) in [0.03f32, 0.06, 0.10].iter().enumerate() {
            let v = (level.powf(2.2 / g) * 255.0).round().clamp(0.0, 255.0) as u8;
            let c = [v as f32 / 255.0, v as f32 / 255.0, v as f32 / 255.0, 1.0];
            text(
                &mut out.items,
                FONT_LARGE,
                fonts.large,
                ix + 24.0,
                iy + 40.0 + i as f32 * 44.0,
                24.0,
                &line,
                c,
            );
        }
        page.button(
            out,
            104.0,
            356.0,
            80.0,
            24.0,
            "#GameUI_OK",
            OptAction::SubOk,
        );
        page.button(
            out,
            192.0,
            356.0,
            80.0,
            24.0,
            "#GameUI_Cancel",
            OptAction::SubCancel,
        );
    } else {
        page.label(out, 22.0, 34.0, 112.0, 24.0, "#GameUI_Model_Detail");
        page.combo(out, 22.0, 58.0, 132.0, 24.0, "ModelDetail");
        page.label(out, 175.0, 34.0, 124.0, 24.0, "#GameUI_Texture_Detail");
        page.combo(out, 176.0, 58.0, 132.0, 24.0, "TextureDetail");
        page.label(out, 330.0, 34.0, 132.0, 24.0, "#GameUI_Shader_Detail");
        page.combo(out, 330.0, 58.0, 132.0, 24.0, "ShaderDetail");
        page.label(out, 22.0, 94.0, 132.0, 24.0, "#GameUI_Water_Detail");
        page.combo(out, 22.0, 118.0, 132.0, 24.0, "WaterDetail");
        page.label(out, 176.0, 94.0, 132.0, 24.0, "#GameUI_Shadow_Detail");
        page.combo(out, 176.0, 118.0, 132.0, 24.0, "ShadowDetail");
        page.label(out, 330.0, 94.0, 132.0, 24.0, "#GameUI_Color_Correction");
        page.combo(out, 330.0, 118.0, 132.0, 24.0, "ColorCorrection");
        page.divider(out, 22.0, 160.0, 438.0);
        page.label(out, 22.0, 172.0, 156.0, 24.0, "#GameUI_Antialiasing_Mode");
        page.combo(out, 22.0, 196.0, 132.0, 24.0, "AntialiasingMode");
        page.label(out, 176.0, 172.0, 152.0, 24.0, "#GameUI_Filtering_Mode");
        page.combo(out, 176.0, 196.0, 132.0, 24.0, "FilteringMode");
        page.label(out, 330.0, 172.0, 154.0, 24.0, "#GameUI_Wait_For_VSync");
        page.combo(out, 330.0, 196.0, 132.0, 24.0, "VSync");
        page.label(out, 22.0, 232.0, 138.0, 24.0, "#GameUI_MotionBlur");
        page.combo(out, 22.0, 258.0, 132.0, 24.0, "MotionBlur");
        page.label(out, 176.0, 232.0, 168.0, 24.0, "#GameUI_FOV");
        page.slider(out, 176.0, 252.0, 140.0, 50.0, "fov_desired");
        page.label(out, 330.0, 232.0, 168.0, 24.0, "#GameUI_MulticoreRendering");
        page.combo(out, 330.0, 258.0, 132.0, 24.0, "Multicore");
        page.label(out, 22.0, 292.0, 138.0, 24.0, "#GameUI_HDR");
        page.combo(out, 22.0, 316.0, 132.0, 24.0, "HDR");
        page.check(
            out,
            170.0,
            316.0,
            292.0,
            24.0,
            "#GameUI_Bloom",
            "mat_bloomscale",
        );
        page.divider(out, 22.0, 352.0, 438.0);
        page.label(out, 22.0, 360.0, 288.0, 24.0, "#GameUI_DX_Installed");
        page.label(out, 333.0, 360.0, 130.0, 24.0, "DirectX v9.0");
        page.label(out, 22.0, 385.0, 288.0, 24.0, "#GameUI_DX_Level");
        page.combo(out, 330.0, 385.0, 132.0, 24.0, "dxlabel");
        page.label(out, 22.0, 410.0, 288.0, 24.0, "#GameUI_D3D9Ex");
        page.combo(out, 330.0, 410.0, 132.0, 24.0, "D3D9Ex");
        page.button(
            out,
            290.0,
            h - 34.0,
            80.0,
            24.0,
            "#GameUI_OK",
            OptAction::SubOk,
        );
        page.button(
            out,
            382.0,
            h - 34.0,
            80.0,
            24.0,
            "#GameUI_Cancel",
            OptAction::SubCancel,
        );
    }
    let Page {
        bars,
        popup,
        popup_hits,
        ..
    } = page;
    out.items.extend(popup);
    out.hits.extend(popup_hits);
    bars
}
