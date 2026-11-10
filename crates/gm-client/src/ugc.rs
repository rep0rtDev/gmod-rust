//! The main menu's Saves / Dupes / Demos pages (`template/{saves,dupes,demos}.html`,
//! `css/Workshop.css`, `js/menu/WorkshopFiles.js`, `lua/menu/menu_{save,dupe,demo}.lua`): the
//! category column, the icon grid of `workshopicon`s and the pagination. Only the local
//! category has files (ours live under `~/.gmod-rust/<kind>`); the Workshop categories show a
//! message, there is no Workshop in this ecosystem.

use std::collections::BTreeMap;
use std::path::PathBuf;

use gm_render::ui::{FontMetrics, UiItem};

use crate::derma::{image, inside, rgba, text, Action, Ctx, Hit, Layout};
use crate::settings::home_dir;

pub const F14B: &str = "Menu14Bold";
pub const F48B: &str = "Menu48Bold";

pub(crate) const WHITE: [f32; 4] = rgba(255, 255, 255, 255);
pub(crate) const BLACK: [f32; 4] = rgba(0, 0, 0, 255);
pub(crate) const ICON_BG: [f32; 4] = rgba(102, 102, 102, 255);
pub(crate) const NAME_TEXT: [f32; 4] = rgba(0, 0, 0, 255);
pub(crate) const DESC_BG: [f32; 4] = rgba(70, 180, 250, 242);
pub(crate) const MESSAGE: [f32; 4] = rgba(153, 153, 153, 255);
pub(crate) const SUB: [f32; 4] = rgba(136, 136, 136, 255);
pub(crate) const CONTROL: [f32; 4] = rgba(245, 255, 128, 255);
pub(crate) const CONTROL_HOVER: [f32; 4] = rgba(255, 255, 255, 255);
pub(crate) const LINK: [f32; 4] = rgba(255, 255, 255, 255);
pub(crate) const LINK_HOVER: [f32; 4] = rgba(255, 255, 170, 255);
pub(crate) const LINK_ACTIVE: [f32; 4] = rgba(255, 255, 85, 255);
pub(crate) const PAGE_BTN: [f32; 4] = rgba(35, 135, 237, 255);
pub(crate) const PAGE_BTN_HOVER: [f32; 4] = rgba(79, 157, 237, 255);

/// `MyCategories`, `Categories`, `CategoriesSecondary` of `control.Saves.js` (dupes / demos
/// share them).
pub const MY_CATEGORIES: [&str; 2] = ["local", "subscribed_ugc"];
pub const CATEGORIES: [&str; 3] = ["trending", "popular", "latest"];
pub const CATEGORIES_SECONDARY: [&str; 4] = ["followed", "favorite", "friends", "mine"];

#[derive(Clone, Debug, PartialEq)]
pub enum UgcAction {
    Category(&'static str),
    OpenFolder,
    Page(i32),
    GoToPage(usize),
    Primary(String),
    Delete(String),
    /// The spawn menu tab's save button (`gm_save` / `dupe_save`).
    Save,
}

#[derive(Clone, Debug, PartialEq)]
pub struct UgcFile {
    pub name: String,
    pub path: PathBuf,
    pub preview: Option<String>,
    pub description: String,
}

#[derive(Debug)]
pub struct UgcState {
    /// `saves` / `dupes` / `demos`: the localization prefix and the folder under home.
    pub kind: &'static str,
    pub category: &'static str,
    pub files: Vec<UgcFile>,
    pub offset: usize,
    pub per_page: usize,
}

impl UgcState {
    pub fn new(kind: &'static str, lang: &BTreeMap<String, String>) -> Self {
        let mut st = UgcState {
            kind,
            category: "local",
            files: Vec::new(),
            offset: 0,
            per_page: 1,
        };
        st.refresh(lang);
        st
    }

    pub fn folder(&self) -> PathBuf {
        home_dir().join(self.kind)
    }

    fn extension(&self) -> &'static str {
        match self.kind {
            "saves" | "dupes" => gm_server::save::EXTENSION,
            _ => "dem",
        }
    }

    /// `FetchLocal`: the folder's files, newest first, with a `.png` preview when present.
    pub fn refresh(&mut self, lang: &BTreeMap<String, String>) {
        let ext = self.extension();
        let description = lang
            .get(&format!("{}.local_description", self.kind))
            .cloned()
            .unwrap_or_default();
        let mut entries: Vec<(std::time::SystemTime, UgcFile)> = Vec::new();
        if let Ok(rd) = std::fs::read_dir(self.folder()) {
            for e in rd.flatten() {
                let path = e.path();
                if path.extension().and_then(|s| s.to_str()) != Some(ext) {
                    continue;
                }
                let Some(name) = path.file_stem().and_then(|s| s.to_str()) else {
                    continue;
                };
                let png = path.with_extension("png");
                let modified = e
                    .metadata()
                    .and_then(|m| m.modified())
                    .unwrap_or(std::time::UNIX_EPOCH);
                entries.push((
                    modified,
                    UgcFile {
                        name: name.to_string(),
                        path: path.clone(),
                        preview: png.exists().then(|| png.to_string_lossy().into_owned()),
                        description: description.clone(),
                    },
                ));
            }
        }
        entries.sort_by_key(|a| std::cmp::Reverse(a.0));
        self.files = entries.into_iter().map(|(_, f)| f).collect();
        if self.offset >= self.files.len() {
            self.offset = 0;
        }
    }

    pub fn delete(&mut self, name: &str, lang: &BTreeMap<String, String>) {
        if let Some(f) = self.files.iter().find(|f| f.name == name) {
            let _ = std::fs::remove_file(&f.path);
            let _ = std::fs::remove_file(f.path.with_extension("png"));
        }
        self.refresh(lang);
    }

    pub fn total(&self) -> usize {
        if self.category == "local" {
            self.files.len()
        } else {
            0
        }
    }
}

pub struct Fonts<'a> {
    pub f11: &'a FontMetrics,
    pub f13: &'a FontMetrics,
    pub f14b: &'a FontMetrics,
    pub f32: &'a FontMetrics,
    pub f48b: &'a FontMetrics,
    pub opt: &'a FontMetrics,
    pub opt_font: &'a str,
}

pub(crate) fn rect(items: &mut Vec<UiItem>, x: f32, y: f32, w: f32, h: f32, color: [f32; 4]) {
    items.push(UiItem::Rect { x, y, w, h, color });
}

pub(crate) fn say(lang: &BTreeMap<String, String>, key: &str, d: &str) -> String {
    lang.get(key)
        .map(|s| s.replace("\\!", "!").replace("\\:", ":"))
        .unwrap_or_else(|| d.to_string())
}

#[allow(clippy::too_many_arguments)]
pub(crate) fn shadow_text(
    out: &mut Layout,
    font: &str,
    fm: &FontMetrics,
    x: f32,
    y: f32,
    h: f32,
    s: &str,
    c: [f32; 4],
) {
    text(&mut out.items, font, fm, x + 2.0, y + 2.0, h, s, BLACK);
    text(&mut out.items, font, fm, x, y, h, s, c);
}

pub(crate) fn fit(fm: &FontMetrics, s: &str, w: f32) -> String {
    let mut t = s.to_string();
    if fm.text_width(&t) <= w {
        return t;
    }
    while fm.text_width(&format!("{t}...")) > w && !t.is_empty() {
        t.pop();
    }
    format!("{t}...")
}

/// Word-wrapped lines of `s` within `w`.
pub(crate) fn wrap(fm: &FontMetrics, s: &str, w: f32) -> Vec<String> {
    let mut lines = Vec::new();
    let mut cur = String::new();
    for word in s.split_whitespace() {
        let cand = if cur.is_empty() {
            word.to_string()
        } else {
            format!("{cur} {word}")
        };
        if fm.text_width(&cand) > w && !cur.is_empty() {
            lines.push(std::mem::take(&mut cur));
            cur = word.to_string();
        } else {
            cur = cand;
        }
    }
    if !cur.is_empty() {
        lines.push(cur);
    }
    lines
}

/// Returns the per-page count the grid fits (`RefreshDimensions`), for the state to adopt.
pub fn page(
    out: &mut Layout,
    ctx: &Ctx,
    st: &UgcState,
    lang: &BTreeMap<String, String>,
    fonts: &Fonts,
) -> usize {
    let (w, h) = (ctx.w, ctx.h - 50.0);
    let kind = st.kind;
    let x = 86.0;
    let line = (fonts.opt.line_height + 1.0).round();
    let mut y = 52.0;
    let title = say(lang, kind, kind);
    shadow_text(out, fonts.opt_font, fonts.opt, x, y, line, &title, WHITE);
    y += line;
    let link = |out: &mut Layout, y: f32, label: &str, active: bool, action: Action| {
        let tw = fonts.opt.text_width(label);
        let hovered = inside(ctx.cursor, x, y, tw, line);
        let c = if active {
            LINK_ACTIVE
        } else if hovered {
            LINK_HOVER
        } else {
            LINK
        };
        shadow_text(out, fonts.opt_font, fonts.opt, x, y, line, label, c);
        out.hits.push(Hit {
            x,
            y,
            w: tw,
            h: line,
            action,
        });
    };
    for group in [
        &MY_CATEGORIES[..],
        &CATEGORIES[..],
        &CATEGORIES_SECONDARY[..],
    ] {
        for cat in group {
            let label = say(lang, &format!("{kind}.{cat}"), cat);
            link(
                out,
                y,
                &label,
                st.category == *cat,
                Action::Ugc(UgcAction::Category(cat)),
            );
            y += line;
        }
        y += line;
    }
    link(
        out,
        y,
        &say(lang, &format!("{kind}.openfolder"), "Open Folder"),
        false,
        Action::Ugc(UgcAction::OpenFolder),
    );

    // div.ugc_content
    let left = if w > 1024.0 { 320.0 } else { 200.0 };
    let (cx, cy, cw) = (left + 16.0, 16.0, w - left - 32.0);
    let title_h = 40.0;
    let head = say(lang, &format!("{kind}.{}", st.category), st.category);
    shadow_text(out, "Menu32", fonts.f32, cx, cy, title_h, &head, WHITE);
    let tw = fonts.f32.text_width(&head);
    text(
        &mut out.items,
        "Menu13",
        fonts.f13,
        cx + tw + 8.0,
        cy + 12.0,
        24.0,
        &say(lang, &format!("{kind}.{}.subtitle", st.category), ""),
        SUB,
    );
    // workshopcontainer: top 64, white, padding 8
    let (kx, ky, kw, kh) = (cx, cy + 64.0, cw, h - 16.0 - cy - 64.0);
    grid(out, ctx, st, lang, fonts, kx, ky, kw, kh)
}

/// The `workshopcontainer` of a Saves / Dupes / Demos page: the white panel of file icons
/// (preview, name bar, hover description, buttons) with the pagination at its bottom, in the
/// rectangle given. Returns the icons per page.
#[allow(clippy::too_many_arguments)]
pub fn grid(
    out: &mut Layout,
    ctx: &Ctx,
    st: &UgcState,
    lang: &BTreeMap<String, String>,
    fonts: &Fonts,
    kx: f32,
    ky: f32,
    kw: f32,
    kh: f32,
) -> usize {
    let kind = st.kind;
    rect(&mut out.items, kx, ky, kw, kh, WHITE);
    let (iw_all, ih_all) = ((kw - 16.0).max(180.0), (kh - 16.0 - 48.0).max(180.0));
    let icons_wide = ((iw_all / 180.0).floor() as usize).clamp(1, 6);
    let icons_tall = ((ih_all / 180.0).floor() as usize).clamp(1, 4);
    let per_page = icons_wide * icons_tall;
    let icon_w = (iw_all / icons_wide as f32).floor() - 26.0;
    let icon_h = (ih_all / icons_tall as f32).floor() - 26.0;
    let icon_max = icon_w.max(icon_h) + 1.0;

    if st.category != "local" {
        let msg = match st.category {
            "subscribed_ugc" => say(lang, "ugc.disabled", "Workshop is disabled"),
            _ => "No Workshop in this ecosystem".to_string(),
        };
        let lines = wrap(fonts.f48b, &msg, kw - 128.0);
        let mut my = ky + 64.0;
        for l in lines {
            let lw = fonts.f48b.text_width(&l);
            text(
                &mut out.items,
                F48B,
                fonts.f48b,
                kx + ((kw - lw) / 2.0).round(),
                my,
                56.0,
                &l,
                MESSAGE,
            );
            my += 56.0;
        }
        return per_page;
    }
    if st.files.is_empty() {
        let msg = say(lang, &format!("{kind}.none"), "Nothing found!");
        let lw = fonts.f48b.text_width(&msg);
        text(
            &mut out.items,
            F48B,
            fonts.f48b,
            kx + ((kw - lw) / 2.0).round(),
            ky + 64.0,
            56.0,
            &msg,
            MESSAGE,
        );
        return per_page;
    }
    let primary = match kind {
        "saves" => say(lang, "saves.load", "load"),
        "dupes" => say(lang, "dupes.spawn", "spawn"),
        _ => say(lang, "demos.play", "play"),
    };
    let delete = say(lang, &format!("{kind}.delete"), "delete");
    let publish = say(lang, &format!("{kind}.publish"), "publish");
    for (i, f) in st
        .files
        .iter()
        .skip(st.offset)
        .take(per_page.max(1))
        .enumerate()
    {
        let col = (i % icons_wide) as f32;
        let row = (i / icons_wide) as f32;
        let ix = kx + 8.0 + 10.0 + col * (icon_w + 20.0);
        let iy = ky + 8.0 + 10.0 + row * (icon_h + 20.0);
        let hovered = inside(ctx.cursor, ix, iy, icon_w, icon_h);
        rect(&mut out.items, ix, iy, icon_w, icon_h, ICON_BG);
        if let Some(p) = &f.preview {
            if (ctx.has_image)(p) {
                // preview: IconMax square centred, cropped to the icon
                let side = icon_max.min(icon_w.max(icon_h));
                let px = ix + ((icon_w - side) / 2.0).round();
                let py = iy + ((icon_h - side) / 2.0).round();
                let vx = px.max(ix);
                let vy = py.max(iy);
                let vw = (px + side).min(ix + icon_w) - vx;
                let vh = (py + side).min(iy + icon_h) - vy;
                image(&mut out.items, p, vx, vy, vw, vh);
            }
        }
        // name bar
        let name_h = (fonts.f14b.line_height + 6.0).round();
        rect(
            &mut out.items,
            ix,
            iy,
            icon_w,
            name_h,
            rgba(255, 255, 255, 252),
        );
        text(
            &mut out.items,
            F14B,
            fonts.f14b,
            ix + 2.0,
            iy + 1.0,
            name_h,
            &fit(fonts.f14b, &f.name, icon_w - 4.0),
            NAME_TEXT,
        );
        if hovered {
            // description overlay
            rect(
                &mut out.items,
                ix,
                iy + name_h,
                icon_w,
                icon_h - name_h,
                DESC_BG,
            );
            let mut dy = iy + 32.0;
            for l in wrap(fonts.f11, &f.description, icon_w - 16.0) {
                text(
                    &mut out.items,
                    "Menu11",
                    fonts.f11,
                    ix + 8.0,
                    dy,
                    14.0,
                    &l,
                    WHITE,
                );
                dy += 14.0;
            }
            // controls: left publish / delete, right primary
            let cy2 = iy + icon_h - 6.0 - 19.0;
            let mut bx = ix + 8.0;
            for (label, action) in [
                (&publish, None),
                (&delete, Some(UgcAction::Delete(f.name.clone()))),
            ] {
                let bw = fonts.f11.text_width(label) + 8.0;
                let bh = inside(ctx.cursor, bx, cy2, bw, 19.0);
                rect(
                    &mut out.items,
                    bx + 1.0,
                    cy2 + 1.0,
                    bw,
                    19.0,
                    rgba(0, 0, 0, 140),
                );
                rect(
                    &mut out.items,
                    bx,
                    cy2,
                    bw,
                    19.0,
                    if action.is_none() {
                        rgba(191, 191, 191, 255)
                    } else if bh {
                        CONTROL_HOVER
                    } else {
                        CONTROL
                    },
                );
                text(
                    &mut out.items,
                    "Menu11",
                    fonts.f11,
                    bx + 4.0,
                    cy2,
                    19.0,
                    label,
                    BLACK,
                );
                if let Some(a) = action {
                    out.hits.push(Hit {
                        x: bx,
                        y: cy2,
                        w: bw,
                        h: 19.0,
                        action: Action::Ugc(a),
                    });
                }
                bx += bw + 4.0;
            }
            let bw = fonts.f11.text_width(&primary) + 8.0;
            let bx = ix + icon_w - 8.0 - bw;
            let bh = inside(ctx.cursor, bx, cy2, bw, 19.0);
            rect(
                &mut out.items,
                bx + 1.0,
                cy2 + 1.0,
                bw,
                19.0,
                rgba(0, 0, 0, 140),
            );
            rect(
                &mut out.items,
                bx,
                cy2,
                bw,
                19.0,
                if bh { CONTROL_HOVER } else { CONTROL },
            );
            text(
                &mut out.items,
                "Menu11",
                fonts.f11,
                bx + 4.0,
                cy2,
                19.0,
                &primary,
                BLACK,
            );
            out.hits.push(Hit {
                x: bx,
                y: cy2,
                w: bw,
                h: 19.0,
                action: Action::Ugc(UgcAction::Primary(f.name.clone())),
            });
        }
    }
    // pagination
    if st.files.len() > per_page {
        let pages = st.files.len().div_ceil(per_page.max(1));
        let pw = (pages as f32 * 20.0 + 2.0 * 52.0 + 60.0).clamp(200.0, 500.0);
        let (px, py) = (kx + ((kw - pw) / 2.0).round(), ky + kh - 4.0 - 44.0);
        rect(&mut out.items, px, py, pw, 44.0, WHITE);
        for (dir, bx, img) in [
            (-1, px + 4.0, "html/img/arrow_left.png"),
            (1, px + pw - 4.0 - 48.0, "html/img/arrow_right.png"),
        ] {
            let hovered = inside(ctx.cursor, bx, py + 4.0, 48.0, 36.0);
            rect(
                &mut out.items,
                bx,
                py + 4.0,
                48.0,
                36.0,
                if hovered { PAGE_BTN_HOVER } else { PAGE_BTN },
            );
            if (ctx.has_image)(img) {
                image(&mut out.items, img, bx + 8.0, py + 4.0 + 2.0, 32.0, 32.0);
            }
            out.hits.push(Hit {
                x: bx,
                y: py + 4.0,
                w: 48.0,
                h: 36.0,
                action: Action::Ugc(UgcAction::Page(dir)),
            });
        }
        let cur = st.offset / per_page.max(1) + 1;
        let dots_w = pages as f32 * 20.0;
        let mut dx = px + ((pw - dots_w) / 2.0).round();
        for p in 1..=pages {
            let img = "html/img/pagination_page.png";
            if (ctx.has_image)(img) {
                let a = if p == cur { 1.0 } else { 0.5 };
                out.items.push(UiItem::Image {
                    x: dx,
                    y: py + 10.0,
                    w: 16.0,
                    h: 16.0,
                    material: img.to_string(),
                    color: [1.0, 1.0, 1.0, a],
                    rot: 0,
                });
            } else {
                rect(
                    &mut out.items,
                    dx + 4.0,
                    py + 14.0,
                    8.0,
                    8.0,
                    if p == cur {
                        PAGE_BTN
                    } else {
                        rgba(35, 135, 237, 128)
                    },
                );
            }
            out.hits.push(Hit {
                x: dx,
                y: py + 4.0,
                w: 20.0,
                h: 36.0,
                action: Action::Ugc(UgcAction::GoToPage(p)),
            });
            dx += 20.0;
        }
        let num = cur.to_string();
        let nw = fonts.f13.text_width(&num);
        text(
            &mut out.items,
            "Menu13",
            fonts.f13,
            px + ((pw - nw) / 2.0).round(),
            py + 26.0,
            18.0,
            &num,
            BLACK,
        );
    }
    per_page
}
