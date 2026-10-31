//! The main menu's Addons page (`template/addon_list.html`, `css/Workshop.css`,
//! `js/menu/control.Addons.js`) over our addon registry: "Subscribed" are the packages
//! installed under `~/.gmod-rust/addons` (enable / disable = `cfg/addons_disabled.txt`), the
//! Workshop categories list the registry in `cfg/registry.txt` (install = `Registry::install`
//! into the addons folder, uninstall = remove the file).

use std::collections::{BTreeMap, BTreeSet};
use std::path::PathBuf;
use std::sync::mpsc;
use std::time::Instant;

use gm_render::ui::FontMetrics;

use crate::derma::{image, inside, rgba, text, Action, Ctx, Hit, Layout};
use crate::settings::home_dir;
use crate::ugc::{
    fit, rect, say, shadow_text, wrap, BLACK, CONTROL, CONTROL_HOVER, DESC_BG, ICON_BG, LINK,
    LINK_ACTIVE, LINK_HOVER, MESSAGE, NAME_TEXT, PAGE_BTN, PAGE_BTN_HOVER, SUB, WHITE,
};

pub const CATEGORIES: [&str; 3] = ["trending", "popular", "latest"];
pub const CATEGORIES_SECONDARY: [&str; 5] =
    ["followed", "favorite", "friends", "mine", "downloaded_ugc"];
pub const ADDON_TYPES: [&str; 9] = [
    "gamemode", "map", "weapon", "tool", "npc", "entity", "effects", "model", "vehicle",
];

#[derive(Clone, Debug, PartialEq)]
pub enum AdAction {
    Category(&'static str),
    Tag(&'static str),
    Search,
    Page(i32),
    GoToPage(usize),
    Enable(String),
    Disable(String),
    Install(String),
    Uninstall(String),
    OpenWorkshop,
}

#[derive(Clone, Debug, PartialEq)]
pub struct Installed {
    pub id: String,
    pub version: String,
    pub path: PathBuf,
    pub size: u64,
    pub realms: Vec<String>,
    pub depends: Vec<String>,
}

type IndexResult = Result<Vec<gm_script::registry::Entry>, String>;

#[derive(Debug)]
pub struct AddonsState {
    pub category: &'static str,
    pub tag: &'static str,
    pub search: String,
    pub focus: bool,
    pub installed: Vec<Installed>,
    pub disabled: BTreeSet<String>,
    pub registry: Option<String>,
    pub entries: Vec<gm_script::registry::Entry>,
    pub loading: Option<(mpsc::Receiver<IndexResult>, Instant)>,
    pub installing: Option<(String, mpsc::Receiver<Result<PathBuf, String>>)>,
    pub status: String,
    pub offset: usize,
    pub per_page: usize,
}

pub fn addons_dir() -> PathBuf {
    home_dir().join("addons")
}

fn disabled_path() -> PathBuf {
    home_dir().join("cfg").join("addons_disabled.txt")
}

pub fn load_disabled() -> BTreeSet<String> {
    std::fs::read_to_string(disabled_path())
        .unwrap_or_default()
        .lines()
        .map(|l| l.trim().to_string())
        .filter(|l| !l.is_empty())
        .collect()
}

/// The manifest of an installed addon (a `.gmr` or a directory) without loading its actors.
pub fn manifest_of(path: &std::path::Path) -> Option<gm_script::Manifest> {
    if path.is_dir() {
        gm_script::Manifest::load(path).ok().map(|(m, _)| m)
    } else if path.extension().and_then(|e| e.to_str()) == Some("gmr") {
        gm_script::Package::open(path)
            .ok()?
            .load()
            .ok()
            .map(|(m, _)| m)
    } else {
        None
    }
}

fn dir_size(path: &std::path::Path) -> u64 {
    if path.is_dir() {
        std::fs::read_dir(path)
            .map(|rd| rd.flatten().map(|e| dir_size(&e.path())).sum())
            .unwrap_or(0)
    } else {
        std::fs::metadata(path).map(|m| m.len()).unwrap_or(0)
    }
}

/// Every installed addon, by id.
pub fn scan_installed() -> Vec<Installed> {
    let mut out = Vec::new();
    if let Ok(rd) = std::fs::read_dir(addons_dir()) {
        for e in rd.flatten() {
            let path = e.path();
            let Some(m) = manifest_of(&path) else {
                continue;
            };
            out.push(Installed {
                id: m.id,
                version: m.version,
                size: dir_size(&path),
                path,
                realms: m.realms,
                depends: m.depends,
            });
        }
    }
    out.sort_by(|a, b| a.id.cmp(&b.id));
    out
}

/// `GetNiceSize`.
pub fn nice_size(bytes: u64) -> String {
    if bytes >= 1 << 20 {
        format!("{:.1} MB", bytes as f64 / (1u64 << 20) as f64)
    } else if bytes >= 1 << 10 {
        format!("{:.1} KB", bytes as f64 / 1024.0)
    } else {
        format!("{bytes} B")
    }
}

impl AddonsState {
    pub fn new() -> Self {
        let registry = std::fs::read_to_string(home_dir().join("cfg").join("registry.txt"))
            .ok()
            .map(|s| s.trim().to_string())
            .filter(|s| !s.is_empty());
        let mut st = AddonsState {
            category: "subscribed",
            tag: "",
            search: String::new(),
            focus: false,
            installed: scan_installed(),
            disabled: load_disabled(),
            registry,
            entries: Vec::new(),
            loading: None,
            installing: None,
            status: String::new(),
            offset: 0,
            per_page: 1,
        };
        st.fetch_registry();
        st
    }

    pub fn refresh(&mut self) {
        self.installed = scan_installed();
        self.disabled = load_disabled();
    }

    /// Reads the registry index on a thread (it may be http).
    pub fn fetch_registry(&mut self) {
        let Some(reg) = self.registry.clone() else {
            return;
        };
        if self.loading.is_some() {
            return;
        }
        let (tx, rx) = mpsc::channel();
        std::thread::spawn(move || {
            let r = gm_script::Registry::open_any(&reg)
                .map(|r| r.index.packages)
                .map_err(|e| e.to_string());
            let _ = tx.send(r);
        });
        self.loading = Some((rx, Instant::now()));
    }

    pub fn install(&mut self, id: &str) {
        let Some(reg) = self.registry.clone() else {
            return;
        };
        if self.installing.is_some() {
            return;
        }
        let Some(entry) = self.entries.iter().find(|e| e.id == id).cloned() else {
            return;
        };
        let (tx, rx) = mpsc::channel();
        std::thread::spawn(move || {
            let r = gm_script::Registry::open_any(&reg)
                .and_then(|r| r.install(&entry, &addons_dir()))
                .map_err(|e| e.to_string());
            let _ = tx.send(r);
        });
        self.installing = Some((id.to_string(), rx));
    }

    pub fn uninstall(&mut self, id: &str) {
        if let Some(a) = self.installed.iter().find(|a| a.id == id) {
            let _ = if a.path.is_dir() {
                std::fs::remove_dir_all(&a.path)
            } else {
                std::fs::remove_file(&a.path)
            };
        }
        self.refresh();
    }

    pub fn set_enabled(&mut self, id: &str, on: bool) {
        if on {
            self.disabled.remove(id);
        } else {
            self.disabled.insert(id.to_string());
        }
        let p = disabled_path();
        if let Some(dir) = p.parent() {
            let _ = std::fs::create_dir_all(dir);
        }
        let lines: Vec<&str> = self.disabled.iter().map(String::as_str).collect();
        let _ = std::fs::write(p, lines.join("\n"));
    }

    pub fn poll(&mut self) {
        if let Some((rx, _)) = &self.loading {
            match rx.try_recv() {
                Ok(Ok(entries)) => {
                    self.entries = entries;
                    self.status.clear();
                    self.loading = None;
                }
                Ok(Err(e)) => {
                    self.status = format!("Registry: {e}");
                    self.loading = None;
                }
                Err(mpsc::TryRecvError::Disconnected) => self.loading = None,
                Err(_) => {}
            }
        }
        if let Some((id, rx)) = &self.installing {
            match rx.try_recv() {
                Ok(Ok(_)) => {
                    self.status = format!("{id} installed");
                    self.installing = None;
                    self.refresh();
                }
                Ok(Err(e)) => {
                    self.status = format!("{id}: {e}");
                    self.installing = None;
                }
                Err(mpsc::TryRecvError::Disconnected) => self.installing = None,
                Err(_) => {}
            }
        }
    }

    fn is_installed(&self, id: &str) -> bool {
        self.installed.iter().any(|a| a.id == id)
    }

    /// The items of the current category: (id, version, size, description, installed).
    pub fn items(&self) -> Vec<(String, String, u64, String)> {
        match self.category {
            "subscribed" => {
                let q = self.search.to_ascii_lowercase();
                self.installed
                    .iter()
                    .filter(|a| q.is_empty() || a.id.to_ascii_lowercase().contains(&q))
                    .map(|a| {
                        let state = if self.disabled.contains(&a.id) {
                            "disabled"
                        } else {
                            "enabled"
                        };
                        let mut d = format!("{} {} ({state})", a.id, a.version);
                        if !a.realms.is_empty() {
                            d.push_str(&format!("\nrealms: {}", a.realms.join(", ")));
                        }
                        if !a.depends.is_empty() {
                            d.push_str(&format!("\ndepends: {}", a.depends.join(", ")));
                        }
                        (a.id.clone(), a.version.clone(), a.size, d)
                    })
                    .collect()
            }
            "trending" | "popular" | "latest" | "downloaded_ugc" => {
                let mut v: Vec<&gm_script::registry::Entry> = self.entries.iter().collect();
                match self.category {
                    "latest" => v.reverse(),
                    "popular" => v.sort_by(|a, b| a.id.cmp(&b.id)),
                    _ => {}
                }
                v.into_iter()
                    .filter(|e| self.category != "downloaded_ugc" || self.is_installed(&e.id))
                    .map(|e| {
                        let mut d = format!("{} {}", e.id, e.version);
                        if !e.depends.is_empty() {
                            d.push_str(&format!("\ndepends: {}", e.depends.join(", ")));
                        }
                        if !e.signers.is_empty() {
                            d.push_str(&format!("\nsigned by {} key(s)", e.signers.len()));
                        }
                        (e.id.clone(), e.version.clone(), e.size, d)
                    })
                    .collect()
            }
            _ => Vec::new(),
        }
    }
}

pub struct Fonts<'a> {
    pub f11: &'a FontMetrics,
    pub f12: &'a FontMetrics,
    pub f13: &'a FontMetrics,
    pub f14b: &'a FontMetrics,
    pub f32: &'a FontMetrics,
    pub f48b: &'a FontMetrics,
    pub opt: &'a FontMetrics,
    pub opt_font: &'a str,
}

/// Returns the per-page count the grid fits.
pub fn page(
    out: &mut Layout,
    ctx: &Ctx,
    st: &AddonsState,
    lang: &BTreeMap<String, String>,
    fonts: &Fonts,
) -> usize {
    let (w, h) = (ctx.w, ctx.h - 50.0);
    let x = 86.0;
    let line = (fonts.opt.line_height + 1.0).round();
    let small = (fonts.f13.line_height + 1.0).round();
    let mut y = 52.0;
    shadow_text(
        out,
        fonts.opt_font,
        fonts.opt,
        x,
        y,
        line,
        &say(lang, "addons", "Addons"),
        WHITE,
    );
    y += line;
    let link = |out: &mut Layout,
                x: f32,
                y: f32,
                font: &str,
                fm: &FontMetrics,
                lh: f32,
                label: &str,
                active: bool,
                action: Action| {
        let tw = fm.text_width(label);
        let hovered = inside(ctx.cursor, x, y, tw, lh);
        let c = if active {
            LINK_ACTIVE
        } else if hovered {
            LINK_HOVER
        } else {
            LINK
        };
        shadow_text(out, font, fm, x, y, lh, label, c);
        out.hits.push(Hit {
            x,
            y,
            w: tw,
            h: lh,
            action,
        });
    };
    // the tag sub-list under the active category (the subscribed one adds the search box)
    let tags = |out: &mut Layout, y: &mut f32, cat: &'static str| {
        *y += 4.0;
        for t in ADDON_TYPES {
            link(
                out,
                x + 16.0,
                *y,
                "Menu13",
                fonts.f13,
                small,
                &say(lang, &format!("addons.{t}"), t),
                st.tag == t,
                Action::Addons(AdAction::Tag(t)),
            );
            *y += small;
        }
        if cat == "subscribed" {
            let (bw, bh) = (180.0, 22.0);
            *y += 4.0;
            rect(&mut out.items, x + 16.0, *y, bw, bh, WHITE);
            let shown = if st.search.is_empty() && !st.focus {
                say(lang, "addons.search", "Search Subscriptions")
            } else if st.focus {
                format!("{}|", st.search)
            } else {
                st.search.clone()
            };
            text(
                &mut out.items,
                "Menu11",
                fonts.f11,
                x + 20.0,
                *y,
                bh,
                &shown,
                if st.search.is_empty() && !st.focus {
                    rgba(117, 117, 117, 255)
                } else {
                    BLACK
                },
            );
            out.hits.push(Hit {
                x: x + 16.0,
                y: *y,
                w: bw,
                h: bh,
                action: Action::Addons(AdAction::Search),
            });
            *y += bh;
        }
        *y += 15.0;
    };
    link(
        out,
        x,
        y,
        fonts.opt_font,
        fonts.opt,
        line,
        &say(lang, "addons.subscribed", "Subscribed"),
        st.category == "subscribed",
        Action::Addons(AdAction::Category("subscribed")),
    );
    y += line;
    if st.category == "subscribed" {
        tags(out, &mut y, "subscribed");
    }
    y += line;
    for group in [&CATEGORIES[..], &CATEGORIES_SECONDARY[..]] {
        for cat in group {
            link(
                out,
                x,
                y,
                fonts.opt_font,
                fonts.opt,
                line,
                &say(lang, &format!("addons.{cat}"), cat),
                st.category == *cat,
                Action::Addons(AdAction::Category(cat)),
            );
            y += line;
            if st.category == *cat && *cat != "downloaded_ugc" {
                tags(out, &mut y, cat);
            }
        }
        y += line;
    }
    link(
        out,
        x,
        y,
        fonts.opt_font,
        fonts.opt,
        line,
        &say(lang, "addons.openworkshop", "Open Workshop"),
        false,
        Action::Addons(AdAction::OpenWorkshop),
    );

    // ugc_content
    let left = if w > 1024.0 { 320.0 } else { 200.0 };
    let (cx, cy, cw) = (left + 16.0, 16.0, w - left - 32.0);
    let title_h = 40.0;
    let head_key = if st.tag.is_empty() {
        format!("addons.{}", st.category)
    } else {
        format!("addons.{}", st.tag)
    };
    let head = say(lang, &head_key, st.category);
    shadow_text(out, "Menu32", fonts.f32, cx, cy, title_h, &head, WHITE);
    let tw = fonts.f32.text_width(&head);
    text(
        &mut out.items,
        "Menu13",
        fonts.f13,
        cx + tw + 8.0,
        cy + 12.0,
        24.0,
        &say(lang, &format!("{head_key}.subtitle"), ""),
        SUB,
    );
    let (kx, ky, kw, kh) = (cx, cy + 64.0, cw, h - 16.0 - cy - 64.0);
    rect(&mut out.items, kx, ky, kw, kh, WHITE);
    let (iw_all, ih_all) = ((kw - 16.0).max(180.0), (kh - 16.0 - 48.0).max(180.0));
    let icons_wide = ((iw_all / 180.0).floor() as usize).clamp(1, 6);
    let icons_tall = ((ih_all / 180.0).floor() as usize).clamp(1, 4);
    let per_page = icons_wide * icons_tall;
    let icon_w = (iw_all / icons_wide as f32).floor() - 26.0;
    let icon_h = (ih_all / icons_tall as f32).floor() - 26.0;

    let message = |out: &mut Layout, msg: &str| {
        let mut my = ky + 64.0;
        for l in wrap(fonts.f48b, msg, kw - 128.0) {
            let lw = fonts.f48b.text_width(&l);
            text(
                &mut out.items,
                crate::ugc::F48B,
                fonts.f48b,
                kx + ((kw - lw) / 2.0).round(),
                my,
                56.0,
                &l,
                MESSAGE,
            );
            my += 56.0;
        }
    };
    let items = st.items();
    if !st.status.is_empty() {
        text(
            &mut out.items,
            "Menu11",
            fonts.f11,
            kx + 8.0,
            ky + kh - 24.0,
            18.0,
            &st.status,
            SUB,
        );
    }
    if matches!(st.category, "followed" | "favorite" | "friends" | "mine") {
        message(out, "No Workshop in this ecosystem");
        return per_page;
    }
    if st.category != "subscribed" && st.registry.is_none() {
        message(out, "No registry set: put its address in cfg/registry.txt");
        return per_page;
    }
    if st.category != "subscribed" && st.loading.is_some() {
        message(out, &say(lang, "addons.loading", "Fetching list..."));
        return per_page;
    }
    if items.is_empty() {
        message(out, &say(lang, "addons.none", "Nothing found!"));
        return per_page;
    }
    for (i, (id, version, size, desc)) in items
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
        let installed = st.is_installed(id);
        let enabled = installed && !st.disabled.contains(id);
        // workshopicon.installed / .disabled tint the bars (#d2fc7b / #cedcb1)
        let bar = if installed && enabled {
            rgba(210, 252, 123, 255)
        } else if installed {
            rgba(206, 220, 177, 255)
        } else {
            rgba(255, 255, 255, 252)
        };
        rect(&mut out.items, ix, iy, icon_w, icon_h, ICON_BG);
        if (ctx.has_image)("html/img/downloading.png") && !installed {
            image(
                &mut out.items,
                "html/img/downloading.png",
                ix,
                iy,
                icon_w,
                icon_h,
            );
        }
        let name_h = (fonts.f14b.line_height + 6.0).round();
        rect(&mut out.items, ix, iy, icon_w, name_h, bar);
        text(
            &mut out.items,
            crate::ugc::F14B,
            fonts.f14b,
            ix + 2.0,
            iy + 1.0,
            name_h,
            &fit(fonts.f14b, id, icon_w - 4.0),
            NAME_TEXT,
        );
        // size at top 22 right
        let sz = nice_size(*size);
        let sw = fonts.f11.text_width(&sz) + 8.0;
        rect(&mut out.items, ix + icon_w - sw, iy + name_h, sw, 16.0, bar);
        text(
            &mut out.items,
            "Menu11",
            fonts.f11,
            ix + icon_w - sw + 4.0,
            iy + name_h,
            16.0,
            &sz,
            NAME_TEXT,
        );
        let _ = version;
        if hovered {
            rect(
                &mut out.items,
                ix,
                iy + name_h,
                icon_w,
                icon_h - name_h,
                DESC_BG,
            );
            let mut dy = iy + 32.0;
            for para in desc.split('\n') {
                for l in wrap(fonts.f11, para, icon_w - 16.0) {
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
            }
            let cy2 = iy + icon_h - 6.0 - 19.0;
            let mut controls: Vec<(String, AdAction)> = Vec::new();
            if st.category == "subscribed" || installed {
                if enabled {
                    controls.push((
                        say(lang, "addon.disable", "disable"),
                        AdAction::Disable(id.clone()),
                    ));
                } else if installed {
                    controls.push((
                        say(lang, "addon.enable", "enable"),
                        AdAction::Enable(id.clone()),
                    ));
                }
                controls.push((
                    say(lang, "addon.unsubscribe", "uninstall"),
                    AdAction::Uninstall(id.clone()),
                ));
            } else {
                controls.push((
                    say(lang, "addon.subscribe", "install"),
                    AdAction::Install(id.clone()),
                ));
            }
            let mut bx = ix + icon_w - 8.0;
            for (label, action) in controls.into_iter().rev() {
                let bw = fonts.f11.text_width(&label) + 8.0;
                bx -= bw;
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
                    &label,
                    BLACK,
                );
                out.hits.push(Hit {
                    x: bx,
                    y: cy2,
                    w: bw,
                    h: 19.0,
                    action: Action::Addons(action),
                });
                bx -= 4.0;
            }
        }
    }
    if items.len() > per_page {
        let pages = items.len().div_ceil(per_page.max(1));
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
                image(&mut out.items, img, bx + 8.0, py + 6.0, 32.0, 32.0);
            }
            out.hits.push(Hit {
                x: bx,
                y: py + 4.0,
                w: 48.0,
                h: 36.0,
                action: Action::Addons(AdAction::Page(dir)),
            });
        }
        let cur = st.offset / per_page.max(1) + 1;
        let mut dx = px + ((pw - pages as f32 * 20.0) / 2.0).round();
        for p in 1..=pages {
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
            out.hits.push(Hit {
                x: dx,
                y: py + 4.0,
                w: 20.0,
                h: 36.0,
                action: Action::Addons(AdAction::GoToPage(p)),
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
    let _ = fonts.f12;
    per_page
}
