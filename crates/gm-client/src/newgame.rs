//! The main menu's "Start New Game" page (`template/newgame.html`, `css/menu/NewGame.css`,
//! `js/menu/control.NewGame.js`, `lua/menu/getmaps.lua`): the map list by category with
//! `maps/thumb/<map>.png` icons, favourites, search, and the game settings column (max
//! players, server name / LAN / P2P, the gamemode's `settings` from `sandbox.txt`).

use std::collections::BTreeMap;
use std::path::PathBuf;

use gm_content::Mount;
use gm_render::ui::{FontMetrics, UiItem};

use crate::derma::{image, inside, rgba, text, Action, Ctx, Hit, Layout};
use crate::settings::home_dir;

pub const F10: &str = "Menu10";
pub const F11: &str = "Menu11";
pub const F11B: &str = "Menu11Bold";
pub const F12: &str = "MenuNav";
pub const F13: &str = "Menu13";
pub const F14: &str = "Menu14";
pub const F20: &str = "Menu20";
pub const F32: &str = "Menu32";

const WHITE: [f32; 4] = rgba(255, 255, 255, 255);
const CAT_BG: [f32; 4] = rgba(221, 221, 221, 255);
const CAT_HOVER: [f32; 4] = rgba(204, 204, 204, 255);
const CAT_ACTIVE: [f32; 4] = rgba(153, 204, 255, 255);
const CAT_TEXT: [f32; 4] = rgba(85, 85, 85, 255);
const CAT_TEXT_ACTIVE: [f32; 4] = rgba(51, 51, 51, 255);
const COUNT_TEXT: [f32; 4] = rgba(170, 170, 170, 255);
const HEADER: [f32; 4] = rgba(68, 68, 68, 255);
const HEADER_SMALL: [f32; 4] = rgba(153, 153, 153, 255);
const MAP_SELECTED: [f32; 4] = rgba(151, 197, 255, 255);
const LABEL: [f32; 4] = rgba(102, 102, 102, 255);
const INPUT_BORDER: [f32; 4] = rgba(170, 170, 170, 255);
const BODY_TEXT: [f32; 4] = rgba(0, 0, 0, 255);
const DROPDOWN: [f32; 4] = rgba(68, 196, 68, 255);
const DROPDOWN_HOVER: [f32; 4] = rgba(85, 204, 85, 255);
const DROPDOWN_ITEM_HOVER: [f32; 4] = rgba(170, 204, 255, 255);
const BTN: [f32; 4] = rgba(0, 110, 204, 255);
const BTN_HOVER: [f32; 4] = rgba(34, 153, 230, 255);
const BTN_BORDER: [f32; 4] = rgba(0, 85, 204, 255);
const PLACEHOLDER: [f32; 4] = rgba(117, 117, 117, 255);

pub const PLAYERS: [usize; 8] = [1, 2, 4, 8, 16, 32, 64, 128];

#[derive(Clone, Debug, PartialEq)]
pub enum NgAction {
    Category(String),
    Map(String),
    Fav(String),
    Search,
    Players,
    PlayersPick(usize),
    Check(String),
    Text(String),
    Start,
}

/// One `settings` entry of the gamemode's `.txt` (name / text / type / default).
#[derive(Clone, Debug)]
pub struct GmSetting {
    pub name: String,
    pub text: String,
    pub kind: String,
    pub value: String,
    pub singleplayer: bool,
}

#[derive(Clone, Debug)]
pub struct NewGameState {
    /// Category -> maps, in display order.
    pub categories: Vec<(String, Vec<String>)>,
    pub favourites: Vec<String>,
    pub current: String,
    pub selected: Option<String>,
    pub search: String,
    pub maxplayers: usize,
    pub players_open: bool,
    pub hostname: String,
    pub sv_lan: bool,
    pub p2p_enabled: bool,
    pub p2p_friendsonly: bool,
    pub settings: Vec<GmSetting>,
    /// Focused text field: "search", "hostname" or a setting name.
    pub focus: Option<String>,
    pub scroll: f32,
    pub settings_scroll: f32,
}

fn fav_path() -> PathBuf {
    home_dir().join("cfg").join("favmaps.txt")
}

/// `getmaps.lua`: category of a map by its full name or `prefix_`; `Sandbox` for `gm_`,
/// the big-game tables reduced to what the mounted games can ship.
fn category_of(name: &str) -> &'static str {
    const NAMES: &[(&str, &str)] = &[
        ("gm_", "Sandbox"),
        ("aoc_", "Age of Chivalry"),
        ("infra_", "INFRA"),
        ("ar_", "Counter-Strike"),
        ("cs_", "Counter-Strike"),
        ("de_", "Counter-Strike"),
        ("es_", "Counter-Strike"),
        ("fy_", "Counter-Strike"),
        ("gd_", "Counter-Strike"),
        ("dz_", "Counter-Strike"),
        ("aim_", "Counter-Strike (Custom)"),
        ("awp_", "Counter-Strike (Custom)"),
        ("dod_", "Day Of Defeat"),
        ("ddd_", "Dino D-Day"),
        ("dys_", "Dystopia"),
        ("d1_", "Half-Life 2"),
        ("d2_", "Half-Life 2"),
        ("d3_", "Half-Life 2"),
        ("ep1_", "Half-Life 2: Episode 1"),
        ("ep2_", "Half-Life 2: Episode 2"),
        ("dm_", "Half-Life 2: Deathmatch"),
        ("halls3", "Half-Life 2: Deathmatch"),
        ("lostcoast", "Half-Life 2: Lost Coast"),
        ("c0a", "Half-Life: Source"),
        ("c1a", "Half-Life: Source"),
        ("c2a", "Half-Life: Source"),
        ("c3a", "Half-Life: Source"),
        ("c4a", "Half-Life: Source"),
        ("c5a", "Half-Life: Source"),
        ("t0a", "Half-Life: Source"),
        ("c1m", "Left 4 Dead 2"),
        ("c2m", "Left 4 Dead 2"),
        ("c3m", "Left 4 Dead 2"),
        ("c4m", "Left 4 Dead 2"),
        ("c5m", "Left 4 Dead 2"),
        ("testchmb_", "Portal"),
        ("escape_", "Portal"),
        ("sp_a", "Portal 2"),
        ("mp_coop_", "Portal 2"),
        ("cp_", "Team Fortress 2"),
        ("ctf_", "Team Fortress 2"),
        ("pl_", "Team Fortress 2"),
        ("plr_", "Team Fortress 2"),
        ("koth_", "Team Fortress 2"),
        ("arena_", "Team Fortress 2"),
        ("tc_", "Team Fortress 2"),
        ("pd_", "Team Fortress 2"),
        ("sd_", "Team Fortress 2"),
        ("tr_", "Team Fortress 2"),
        ("mvm_", "Team Fortress 2"),
        ("itemtest", "Team Fortress 2"),
        ("zps_", "Zombie Panic! Source"),
        ("zpo_", "Zombie Panic! Source"),
        ("zpl_", "Zombie Panic! Source"),
        ("zpa_", "Zombie Panic! Source"),
    ];
    let prefix = name.find('_').map(|i| &name[..=i]).unwrap_or("");
    NAMES
        .iter()
        .find(|(k, _)| {
            *k == name
                || (!prefix.is_empty() && *k == prefix)
                || name.starts_with(k) && !k.ends_with('_')
        })
        .map(|(_, c)| *c)
        .unwrap_or("Other")
}

fn ignored(name: &str) -> bool {
    const PATTERNS: [&str; 5] = [
        "background",
        "devtest",
        "ep1_background",
        "ep2_background",
        "styleguide",
    ];
    const PREFIXES: [&str; 3] = ["sdk_", "test_", "vst_"];
    const MAPS: [&str; 8] = [
        "c4a1y",
        "credits",
        "d2_coast_02",
        "d3_c17_02_camera",
        "ep1_citadel_00_demo",
        "c5m1_waterfront_sndscape",
        "intro",
        "test",
    ];
    PATTERNS.iter().any(|p| name.starts_with(p))
        || PREFIXES.iter().any(|p| name.starts_with(p))
        || MAPS.contains(&name)
}

/// `"settings"` of `gamemodes/<gm>/<gm>.txt`: the blocks with a `name` and a `type`.
pub fn load_settings(mount: &Mount, gamemode: &str) -> Vec<GmSetting> {
    let Ok(text) = mount.read_string(&format!("gamemodes/{gamemode}/{gamemode}.txt")) else {
        return Vec::new();
    };
    let mut out = Vec::new();
    let mut block: Vec<(String, String)> = Vec::new();
    let mut depth = 0i32;
    for line in text.lines() {
        let line = line.split("//").next().unwrap_or("").trim();
        if line == "{" {
            depth += 1;
            if depth == 3 {
                block.clear();
            }
            continue;
        }
        if line == "}" {
            if depth == 3 {
                let get = |k: &str| {
                    block
                        .iter()
                        .find(|(a, _)| a.eq_ignore_ascii_case(k))
                        .map(|(_, v)| v.clone())
                };
                if let (Some(name), Some(kind)) = (get("name"), get("type")) {
                    out.push(GmSetting {
                        text: get("text").unwrap_or_else(|| name.clone()),
                        value: get("default").unwrap_or_default(),
                        singleplayer: get("singleplayer").is_some_and(|v| v != "0"),
                        name,
                        kind,
                    });
                }
            }
            depth -= 1;
            continue;
        }
        let parts: Vec<&str> = line.split('"').collect();
        if parts.len() >= 5 && depth == 3 {
            block.push((parts[1].to_string(), parts[3].to_string()));
        }
    }
    out
}

impl NewGameState {
    pub fn new(mount: &Mount, current_map: &str, hostname: &str) -> Self {
        let favourites: Vec<String> = std::fs::read_to_string(fav_path())
            .unwrap_or_default()
            .split(';')
            .filter(|s| !s.is_empty())
            .map(str::to_string)
            .collect();
        let mut st = NewGameState {
            categories: Vec::new(),
            favourites,
            current: String::new(),
            selected: Some(current_map.to_string()),
            search: String::new(),
            maxplayers: 0,
            players_open: false,
            hostname: hostname.to_string(),
            sv_lan: false,
            p2p_enabled: false,
            p2p_friendsonly: true,
            settings: load_settings(mount, "sandbox"),
            focus: None,
            scroll: 0.0,
            settings_scroll: 0.0,
        };
        st.refresh(mount);
        st.current = st
            .categories
            .iter()
            .find(|(c, maps)| c == "Sandbox" || maps.iter().any(|m| m == current_map))
            .map(|(c, _)| c.clone())
            .or_else(|| st.categories.first().map(|(c, _)| c.clone()))
            .unwrap_or_default();
        st
    }

    /// `RefreshMaps`: every `maps/*.bsp` of the mount, ignored ones dropped, grouped;
    /// Favourites first, Sandbox second, the rest by name.
    pub fn refresh(&mut self, mount: &Mount) {
        let mut groups: BTreeMap<String, Vec<String>> = BTreeMap::new();
        for path in mount.find("maps/*.bsp") {
            let name = path
                .rsplit('/')
                .next()
                .unwrap_or(&path)
                .trim_end_matches(".bsp")
                .to_ascii_lowercase();
            if ignored(&name) {
                continue;
            }
            groups
                .entry(category_of(&name).to_string())
                .or_default()
                .push(name.clone());
            if self.favourites.contains(&name) {
                groups
                    .entry("Favourites".to_string())
                    .or_default()
                    .push(name);
            }
        }
        let mut cats: Vec<(String, Vec<String>)> = groups.into_iter().collect();
        for (_, maps) in &mut cats {
            maps.sort();
            maps.dedup();
        }
        cats.sort_by_key(|(c, _)| match c.as_str() {
            "Favourites" => "1".to_string(),
            "Sandbox" => "2".to_string(),
            other => other.to_string(),
        });
        self.categories = cats;
    }

    pub fn toggle_favourite(&mut self, mount: &Mount, map: &str) {
        if let Some(i) = self.favourites.iter().position(|m| m == map) {
            self.favourites.remove(i);
        } else {
            self.favourites.push(map.to_string());
        }
        if let Some(dir) = fav_path().parent() {
            let _ = std::fs::create_dir_all(dir);
        }
        let _ = std::fs::write(fav_path(), self.favourites.join(";"));
        self.refresh(mount);
        if !self.categories.iter().any(|(c, _)| *c == self.current) {
            self.current = self
                .categories
                .first()
                .map(|(c, _)| c.clone())
                .unwrap_or_default();
        }
    }

    fn filtered<'a>(&self, maps: &'a [String]) -> Vec<&'a String> {
        let q = self.search.to_ascii_lowercase();
        maps.iter()
            .filter(|m| q.is_empty() || m.contains(&q))
            .collect()
    }

    /// The text field being edited.
    pub fn text_mut(&mut self) -> Option<&mut String> {
        match self.focus.as_deref()? {
            "search" => Some(&mut self.search),
            "hostname" => Some(&mut self.hostname),
            name => self
                .settings
                .iter_mut()
                .find(|s| s.name == name)
                .map(|s| &mut s.value),
        }
    }

    /// `cfg/server.cfg` of the client's own server: the console commands `StartGame`
    /// runs (`maxplayers`, `hostname`, `sv_lan`, `p2p_*`, the gamemode settings).
    pub fn server_cfg(&self) -> String {
        let players = PLAYERS[self.maxplayers.min(PLAYERS.len() - 1)];
        let on = |b: bool| if b { "1" } else { "0" };
        let mut out = format!("maxplayers \"{players}\"\n");
        if players > 1 {
            out.push_str(&format!("hostname \"{}\"\n", self.hostname));
            out.push_str(&format!("sv_lan \"{}\"\n", on(self.sv_lan)));
            out.push_str(&format!("p2p_enabled \"{}\"\n", on(self.p2p_enabled)));
            out.push_str(&format!(
                "p2p_friendsonly \"{}\"\n",
                on(self.p2p_friendsonly)
            ));
        } else {
            out.push_str("sv_cheats \"0\"\n");
        }
        for s in &self.settings {
            if players > 1 || s.singleplayer {
                let v = if s.kind == "CheckBox" {
                    on(s.value != "0" && !s.value.is_empty()).to_string()
                } else {
                    s.value.clone()
                };
                out.push_str(&format!("{} \"{v}\"\n", s.name));
            }
        }
        out
    }
}

pub struct Fonts<'a> {
    pub f10: &'a FontMetrics,
    pub f11: &'a FontMetrics,
    pub f11b: &'a FontMetrics,
    pub f12: &'a FontMetrics,
    pub f13: &'a FontMetrics,
    pub f14: &'a FontMetrics,
    pub f20: &'a FontMetrics,
    pub f32: &'a FontMetrics,
}

fn rect(items: &mut Vec<UiItem>, x: f32, y: f32, w: f32, h: f32, color: [f32; 4]) {
    items.push(UiItem::Rect { x, y, w, h, color });
}

fn frame(items: &mut Vec<UiItem>, x: f32, y: f32, w: f32, h: f32, color: [f32; 4]) {
    rect(items, x, y, w, 1.0, color);
    rect(items, x, y + h - 1.0, w, 1.0, color);
    rect(items, x, y, 1.0, h, color);
    rect(items, x + w - 1.0, y, 1.0, h, color);
}

fn say(lang: &BTreeMap<String, String>, key: &str, d: &str) -> String {
    lang.get(key)
        .map(|s| s.replace("\\:", ":"))
        .unwrap_or_else(|| d.to_string())
}

/// A text input (`INPUT type=text`, padding 4, 1 px #aaa border), with the caret when focused.
#[allow(clippy::too_many_arguments)]
fn input(
    out: &mut Layout,
    fm: &FontMetrics,
    font: &str,
    x: f32,
    y: f32,
    w: f32,
    h: f32,
    value: &str,
    placeholder: &str,
    focused: bool,
    action: NgAction,
) {
    rect(&mut out.items, x, y, w, h, WHITE);
    frame(&mut out.items, x, y, w, h, INPUT_BORDER);
    if value.is_empty() && !focused {
        text(
            &mut out.items,
            font,
            fm,
            x + 5.0,
            y,
            h,
            placeholder,
            PLACEHOLDER,
        );
    } else {
        let shown = if focused {
            format!("{value}|")
        } else {
            value.to_string()
        };
        text(&mut out.items, font, fm, x + 5.0, y, h, &shown, BODY_TEXT);
    }
    out.hits.push(Hit {
        x,
        y,
        w,
        h,
        action: Action::NewGame(action),
    });
}

fn checkbox(
    out: &mut Layout,
    fm: &FontMetrics,
    x: f32,
    y: f32,
    label: &str,
    on: bool,
    action: NgAction,
) {
    rect(&mut out.items, x, y + 2.0, 13.0, 13.0, WHITE);
    frame(&mut out.items, x, y + 2.0, 13.0, 13.0, INPUT_BORDER);
    if on {
        rect(
            &mut out.items,
            x + 3.0,
            y + 5.0,
            7.0,
            7.0,
            rgba(51, 51, 51, 255),
        );
    }
    text(&mut out.items, F11, fm, x + 18.0, y, 17.0, label, LABEL);
    let w = 18.0 + fm.text_width(label);
    out.hits.push(Hit {
        x,
        y,
        w,
        h: 17.0,
        action: Action::NewGame(action),
    });
}

/// Lays the page out above the NavBar: `.maplist` (left 16, right 256) and `gamesettings`
/// (right 16, width 226). Returns the max scroll of the icon grid.
pub fn page(
    out: &mut Layout,
    ctx: &Ctx,
    st: &NewGameState,
    lang: &BTreeMap<String, String>,
    fonts: &Fonts,
) -> f32 {
    let (w, h) = (ctx.w, ctx.h - 50.0);
    // .maplist
    let (mx, my, mw, mh) = (16.0, 16.0, w - 256.0 - 16.0, h - 32.0);
    rect(&mut out.items, mx, my, mw, mh, WHITE);
    // .controls: 190 wide at 10,10
    let (cx, cy, cw, ch) = (mx + 10.0, my + 10.0, 190.0, mh - 20.0);
    let mut ly = cy;
    for (cat, maps) in &st.categories {
        let n = st.filtered(maps).len();
        if n == 0 {
            continue;
        }
        let active = *cat == st.current;
        let hovered = inside(ctx.cursor, cx, ly, cw, 26.0);
        let bg = if active {
            CAT_ACTIVE
        } else if hovered {
            CAT_HOVER
        } else {
            CAT_BG
        };
        rect(&mut out.items, cx, ly, cw, 26.0, bg);
        let name = match cat.as_str() {
            "Favourites" => say(lang, "newgame_favorite_maps", "Favorites"),
            "Other" => say(lang, "spawnmenu.category.other", "Other"),
            c => c.to_string(),
        };
        text(
            &mut out.items,
            F11B,
            fonts.f11b,
            cx + 6.0,
            ly + 4.0,
            18.0,
            &name,
            if active { CAT_TEXT_ACTIVE } else { CAT_TEXT },
        );
        let count = n.to_string();
        let bw = fonts.f10.text_width(&count) + 10.0;
        rect(
            &mut out.items,
            cx + cw - 5.0 - bw,
            ly + 5.0,
            bw,
            16.0,
            WHITE,
        );
        text(
            &mut out.items,
            F10,
            fonts.f10,
            cx + cw - bw,
            ly + 5.0,
            16.0,
            &count,
            COUNT_TEXT,
        );
        out.hits.push(Hit {
            x: cx,
            y: ly,
            w: cw,
            h: 26.0,
            action: Action::NewGame(NgAction::Category(cat.clone())),
        });
        ly += 26.0 + 5.0;
    }
    // search at the bottom (32 tall, padding 4)
    input(
        out,
        fonts.f12,
        F12,
        cx,
        cy + ch - 32.0 + 4.0,
        cw,
        24.0,
        &st.search,
        &say(lang, "searchbar_placeholder", "Search"),
        st.focus.as_deref() == Some("search"),
        NgAction::Search,
    );
    // .scrollable at left 200 inside the map list
    let (gx, gy, gw, gh) = (mx + 10.0 + 200.0, my + 10.0, mw - 220.0, mh - 20.0);
    let mut max_scroll = 0.0;
    if let Some((cat, maps)) = st.categories.iter().find(|(c, _)| *c == st.current) {
        let maps = st.filtered(maps);
        let scroll = st.scroll;
        let name = match cat.as_str() {
            "Favourites" => say(lang, "newgame_favorite_maps", "Favorites"),
            "Other" => say(lang, "spawnmenu.category.other", "Other"),
            c => c.to_string(),
        };
        // header: padding 5, 32 px #444, small 14 px #999
        let hy = gy - scroll;
        if hy + 48.0 > gy && hy < gy + gh {
            text(
                &mut out.items,
                F32,
                fonts.f32,
                gx + 5.0,
                hy + 5.0,
                38.0,
                &name,
                HEADER,
            );
            let nw = fonts.f32.text_width(&name);
            let small = format!(
                "{} {}",
                maps.len(),
                say(
                    lang,
                    if maps.len() == 1 {
                        "newgame_map"
                    } else {
                        "newgame_maps"
                    },
                    "maps"
                )
            );
            text(
                &mut out.items,
                F14,
                fonts.f14,
                gx + 5.0 + nw + 6.0,
                hy + 5.0 + 14.0,
                24.0,
                &small,
                HEADER_SMALL,
            );
        }
        // icons: 128 + padding 6*2 + margin 2*2 = 144 per column, name 10 px below
        let cell_w = 144.0;
        let cell_h = 128.0 + 12.0 + 12.0 + 4.0;
        let cols = ((gw / cell_w).floor() as usize).max(1);
        let rows = maps.len().div_ceil(cols);
        max_scroll = (48.0 + rows as f32 * cell_h - gh).max(0.0);
        for (i, m) in maps.iter().enumerate() {
            let col = (i % cols) as f32;
            let row = (i / cols) as f32;
            let x = gx + 2.0 + col * cell_w;
            let y = gy + 48.0 + row * cell_h - scroll + 2.0;
            if y + cell_h < gy || y > gy + gh {
                continue;
            }
            let selected = st.selected.as_deref() == Some(m.as_str());
            let hovered = inside(ctx.cursor, x, y, 140.0, cell_h - 4.0);
            if selected {
                rect(&mut out.items, x, y, 140.0, cell_h - 4.0, MAP_SELECTED);
            }
            let thumb = format!("maps/thumb/{m}.png");
            let icon = if (ctx.has_image)(&thumb) {
                thumb
            } else {
                "maps/thumb/noicon.png".to_string()
            };
            if (ctx.has_image)(&icon) {
                image(&mut out.items, &icon, x + 6.0, y + 6.0, 128.0, 128.0);
            } else {
                rect(
                    &mut out.items,
                    x + 6.0,
                    y + 6.0,
                    128.0,
                    128.0,
                    rgba(51, 51, 51, 255),
                );
            }
            let tw = fonts.f10.text_width(m);
            text(
                &mut out.items,
                F10,
                fonts.f10,
                x + ((140.0 - tw) / 2.0).round(),
                y + 6.0 + 128.0,
                14.0,
                m,
                BODY_TEXT,
            );
            out.hits.push(Hit {
                x,
                y,
                w: 140.0,
                h: cell_h - 4.0,
                action: Action::NewGame(NgAction::Map(m.to_string())),
            });
            let fav = st.favourites.contains(m);
            if hovered || fav {
                let fx = x + 140.0 - 8.0 - 16.0;
                let fy = y + 8.0;
                let fav_hover = inside(ctx.cursor, fx, fy, 16.0, 16.0);
                let img = match (fav, fav_hover) {
                    (true, true) => "html/img/favourite_remove.png",
                    (false, true) => "html/img/favourite_add.png",
                    _ => "html/img/favourite.png",
                };
                if (ctx.has_image)(img) {
                    image(&mut out.items, img, fx, fy, 16.0, 16.0);
                }
                out.hits.push(Hit {
                    x: fx,
                    y: fy,
                    w: 16.0,
                    h: 16.0,
                    action: Action::NewGame(NgAction::Fav(m.to_string())),
                });
            }
        }
    }
    // gamesettings
    let (sx, sy, sw, sh) = (w - 16.0 - 226.0, 16.0, 226.0, h - 32.0);
    rect(&mut out.items, sx, sy, sw, sh, WHITE);
    let players = PLAYERS[st.maxplayers.min(PLAYERS.len() - 1)];
    // .scrollable: top 32, bottom 80, margin 10
    let (vx, vy, vw, vh) = (
        sx + 10.0,
        sy + 32.0 + 10.0,
        sw - 20.0,
        sh - 32.0 - 80.0 - 10.0,
    );
    let mut y = vy + 6.0 - st.settings_scroll;
    let control =
        |out: &mut Layout, y: &mut f32, kind: &str, label: &str, name: &str, value: &str| {
            let visible = |yy: f32, hh: f32| yy >= vy - hh && yy + hh <= vy + vh + hh;
            match kind {
                "Text" => {
                    if visible(*y, 40.0) {
                        text(&mut out.items, F11, fonts.f11, vx, *y, 14.0, label, LABEL);
                        input(
                            out,
                            fonts.f12,
                            F12,
                            vx,
                            *y + 16.0,
                            vw,
                            24.0,
                            value,
                            "",
                            st.focus.as_deref() == Some(name),
                            NgAction::Text(name.to_string()),
                        );
                    }
                    *y += 40.0 + 8.0;
                }
                "Numeric" => {
                    if visible(*y, 20.0) {
                        text(&mut out.items, F11, fonts.f11, vx, *y, 20.0, label, LABEL);
                        input(
                            out,
                            fonts.f12,
                            F12,
                            vx + vw - 44.0,
                            *y,
                            44.0,
                            20.0,
                            value,
                            "",
                            st.focus.as_deref() == Some(name),
                            NgAction::Text(name.to_string()),
                        );
                    }
                    *y += 20.0 + 4.0;
                }
                _ => {
                    if visible(*y, 17.0) {
                        let on = !value.is_empty() && value != "0";
                        checkbox(
                            out,
                            fonts.f11,
                            vx,
                            *y,
                            label,
                            on,
                            NgAction::Check(name.to_string()),
                        );
                    }
                    *y += 17.0 + 8.0;
                }
            }
        };
    if players > 1 {
        control(
            out,
            &mut y,
            "Text",
            &say(lang, "server_name", "Server Name:"),
            "hostname",
            &st.hostname,
        );
        control(
            out,
            &mut y,
            "CheckBox",
            &say(lang, "lan_server", "Local Server"),
            "sv_lan",
            if st.sv_lan { "1" } else { "0" },
        );
        control(
            out,
            &mut y,
            "CheckBox",
            &say(lang, "p2p_server", "Peer-To-Peer"),
            "p2p_enabled",
            if st.p2p_enabled { "1" } else { "0" },
        );
    }
    for kind in ["Text", "Numeric", "CheckBox"] {
        for s in st.settings.iter().filter(|s| s.kind == kind) {
            if players > 1 || s.singleplayer {
                control(
                    out,
                    &mut y,
                    kind,
                    &say(lang, &s.text, &s.text),
                    &s.name,
                    &s.value,
                );
            }
        }
    }
    // the settings column clips to its box: paint the margins back over the overflow
    rect(&mut out.items, sx, sy, sw, vy - sy, WHITE);
    rect(&mut out.items, sx, vy + vh, sw, sy + sh - vy - vh, WHITE);
    // max players dropdown: margin 10, 28 tall, green, bold white, caret
    let (dx, dy, dw) = (sx + 10.0, sy + 10.0, sw - 20.0);
    let hovered = inside(ctx.cursor, dx, dy, dw, 28.0);
    rect(
        &mut out.items,
        dx,
        dy,
        dw,
        28.0,
        if hovered { DROPDOWN_HOVER } else { DROPDOWN },
    );
    let label = say(
        lang,
        &format!("maxplayers_{players}"),
        &format!("{players} Players"),
    );
    text(
        &mut out.items,
        F12,
        fonts.f12,
        dx + 10.0,
        dy,
        28.0,
        &label,
        WHITE,
    );
    for i in 0..5 {
        rect(
            &mut out.items,
            dx + dw - 10.0 - 10.0 + i as f32,
            dy + 10.0 + i as f32,
            10.0 - 2.0 * i as f32,
            1.0,
            WHITE,
        );
    }
    out.hits.push(Hit {
        x: dx,
        y: dy,
        w: dw,
        h: 28.0,
        action: Action::NewGame(NgAction::Players),
    });
    // Start Game: left/right 8 + margin 10, padding 10, 20 px, bottom 8 + 10
    let bh = 20.0 + 20.0 + 2.0;
    let (bx, by, bw) = (sx + 18.0, sy + sh - 18.0 - bh, sw - 36.0);
    let hovered = inside(ctx.cursor, bx, by, bw, bh);
    rect(
        &mut out.items,
        bx,
        by,
        bw,
        bh,
        if hovered { BTN_HOVER } else { BTN },
    );
    frame(&mut out.items, bx, by, bw, bh, BTN_BORDER);
    let s = say(lang, "start_game", "Start Game");
    let tw = fonts.f20.text_width(&s);
    text(
        &mut out.items,
        F20,
        fonts.f20,
        bx + ((bw - tw) / 2.0).round(),
        by,
        bh,
        &s,
        WHITE,
    );
    out.hits.push(Hit {
        x: bx,
        y: by,
        w: bw,
        h: bh,
        action: Action::NewGame(NgAction::Start),
    });
    // the open players list (z-index 100): white, border #aaa, items padding 4 15, 13 px
    if st.players_open {
        let row = 24.0;
        let ly = dy + 31.0;
        let lh = row * PLAYERS.len() as f32 + 2.0;
        rect(&mut out.items, dx, ly, dw, lh, WHITE);
        frame(&mut out.items, dx, ly, dw, lh, INPUT_BORDER);
        for (i, n) in PLAYERS.iter().enumerate() {
            let ry = ly + 1.0 + i as f32 * row;
            let hovered = inside(ctx.cursor, dx + 1.0, ry, dw - 2.0, row);
            if hovered {
                rect(
                    &mut out.items,
                    dx + 1.0,
                    ry,
                    dw - 2.0,
                    row,
                    DROPDOWN_ITEM_HOVER,
                );
            }
            let label = say(lang, &format!("maxplayers_{n}"), &format!("{n} Players"));
            text(
                &mut out.items,
                F13,
                fonts.f13,
                dx + 15.0,
                ry,
                row,
                &label,
                BODY_TEXT,
            );
            out.hits.push(Hit {
                x: dx + 1.0,
                y: ry,
                w: dw - 2.0,
                h: row,
                action: Action::NewGame(NgAction::PlayersPick(i)),
            });
        }
    }
    max_scroll
}
