//! Derma default skin and the sandbox spawn menu.
//!
//! Numbers come from the installed client's data: `lua/skins/default.lua` (colours and the GWEN
//! atlas pieces), `lua/vgui/dpropertysheet.lua` / `dtab.lua` / `dtree_node.lua` (tab and node
//! sizes) and `gamemodes/sandbox/gamemode/spawnmenu/*.lua` (dividers and their minimums); see
//! docs/spec/ui.md. Immediate mode: [`build`] lays the whole menu out from [`MenuState`] every
//! frame and returns the draw list together with the clickable regions, so hit testing and
//! drawing share one geometry.

use std::collections::BTreeMap;

use gm_content::Mount;
use gm_formats::keyvalues;
use gm_render::ui::{FontMetrics, UiItem};

/// `SKIN.GwenTexture`, a 512 px PNG under materials/.
pub const ATLAS: &str = "gwenskin/GModDefault.png";
const ATLAS_SIZE: f32 = 512.0;
/// `DermaDefault`: Tahoma 13 (Windows) / DejaVu Sans 14 weight 500, a system font like in the
/// game (Roboto from the content when it is missing).
pub const FONT: &str = "DermaDefault";
pub const FONT_BOLD: &str = "DermaDefaultBold";
/// `ContentHeader`: Helvetica 50 weight 1000, bright with a 2 px shadow, on its own 64 px line.
pub const HEADER_FONT: &str = "ContentHeader";
/// SpawnIcon's hover overlay.
pub const HOVER_MAT: &str = "vgui/spawnmenu/hover";
/// icon16 images the menu frame needs (spawnlist icons are loaded with the lists).
pub const IMAGES: [&str; 12] = [
    "icon16/application_view_tile.png",
    "icon16/gun.png",
    "icon16/bricks.png",
    "icon16/monkey.png",
    "icon16/car.png",
    "icon16/picture.png",
    "icon16/page_copy.png",
    "icon16/disk.png",
    "icon16/wrench.png",
    "icon16/page_white_wrench.png",
    "icon16/cog.png",
    "icon16/folder_database.png",
];

/// `GWEN.CreateTextureBorder( x, y, w, h, l, t, r, b )`: a 9-slice piece of the atlas.
#[derive(Clone, Copy)]
pub struct Piece {
    x: f32,
    y: f32,
    w: f32,
    h: f32,
    l: f32,
    t: f32,
    r: f32,
    b: f32,
}

#[allow(clippy::too_many_arguments)]
const fn border4(x: u16, y: u16, w: u16, h: u16, l: u16, t: u16, r: u16, b: u16) -> Piece {
    Piece {
        x: x as f32,
        y: y as f32,
        w: w as f32,
        h: h as f32,
        l: l as f32,
        t: t as f32,
        r: r as f32,
        b: b as f32,
    }
}
const fn border(x: u16, y: u16, w: u16, h: u16, m: u16) -> Piece {
    border4(x, y, w, h, m, m, m, m)
}
const fn normal(x: u16, y: u16, w: u16, h: u16) -> Piece {
    border4(x, y, w, h, 0, 0, 0, 0)
}

pub const TREE: Piece = border(256, 128, 127, 127, 16);
/// `SKIN.tex.Tooltip` (GWEN 384,64 31x31, 8 px borders).
pub const TOOLTIP: Piece = border(384, 64, 31, 31, 8);
pub const TEXTBOX: Piece = border(0, 150, 127, 21, 4);
pub const TAB_CONTROL: Piece = border(0, 256, 127, 127, 8);
pub const TABT_ACTIVE: Piece = border(0, 384, 63, 31, 8);
pub const TABT_INACTIVE: Piece = border(128, 384, 63, 31, 8);
pub const SELECTION: Piece = border(384, 32, 31, 31, 4);
pub const TREE_PLUS: Piece = normal(448, 96, 15, 15);
pub const MENU_CHECK: Piece = normal(448, 112, 15, 15);
pub const MENU_ARROW: Piece = normal(464, 112, 15, 15);
pub const TREE_MINUS: Piece = normal(464, 96, 15, 15);
pub const CHECKBOX: Piece = normal(464, 32, 15, 15);
pub const CATEGORY_OUTER: Piece = border(256, 384, 63, 63, 8);
pub const CATEGORY_INNER_H: Piece = border(320, 384, 63, 20, 8);
pub const CATEGORY_INNER: Piece = border4(320, 405, 63, 42, 8, 0, 8, 8);
pub const CATEGORY_HEADER: Piece = border(320, 352, 63, 31, 8);
pub const SCROLL_TRACK_V: Piece = border(384, 208, 15, 127, 4);
pub const SCROLL_BAR_V: Piece = border(400, 208, 15, 127, 4);
pub const CHECKBOX_CHECKED: Piece = normal(448, 32, 15, 15);
pub const SLIDER_KNOB: Piece = normal(416, 32, 15, 15);
pub const TEXTBOX_FOCUS: Piece = border(0, 172, 127, 21, 4);
pub const MENU_BG: Piece = border(128, 192, 127, 63, 8);
pub const MENU_BG_HOVER: Piece = border(128, 256, 127, 31, 8);

pub const fn rgba(r: u8, g: u8, b: u8, a: u8) -> [f32; 4] {
    [
        r as f32 / 255.0,
        g as f32 / 255.0,
        b as f32 / 255.0,
        a as f32 / 255.0,
    ]
}
pub const WHITE: [f32; 4] = rgba(255, 255, 255, 255);
// `SKIN.Colours.*` are `GWEN.TextureColor( x, y )` samples of the atlas; values read off the PNG.
/// `Colours.Tab.Inactive.Normal`.
pub const TAB_TEXT_INACTIVE: [f32; 4] = rgba(255, 255, 255, 102);
/// `Colours.Category.Header` / `Header_Closed`.
pub const CATEGORY_HEADER_TEXT: [f32; 4] = rgba(255, 255, 255, 230);
pub const CATEGORY_HEADER_CLOSED_TEXT: [f32; 4] = rgba(255, 255, 255, 153);
/// `Colours.Category.Line.*` / `LineAlt.*` (even / odd rows): text, hover text, row, hover row.
const LINE_TEXT: [[f32; 4]; 2] = [rgba(119, 119, 119, 255), rgba(136, 136, 136, 255)];
const LINE_TEXT_HOVER: [[f32; 4]; 2] = [rgba(102, 102, 102, 255), rgba(119, 119, 119, 255)];
const LINE_BUTTON: [[f32; 4]; 2] = [rgba(0, 0, 0, 0), rgba(0, 0, 0, 26)];
const LINE_BUTTON_HOVER: [[f32; 4]; 2] = [rgba(0, 0, 0, 8), rgba(0, 0, 0, 32)];
const LINE_SELECTED: [f32; 4] = rgba(255, 216, 0, 255);
/// `Colours.Tree.Normal` / `Hover` (node label; selected is white on `Selection`).
const TREE_TEXT: [f32; 4] = rgba(94, 102, 110, 255);
const TREE_TEXT_HOVER: [f32; 4] = rgba(51, 148, 240, 255);
/// `SKIN.Colours.TextEntry.Placeholder` 128.
pub const PLACEHOLDER: [f32; 4] = rgba(128, 128, 128, 255);
const HOVER: [f32; 4] = rgba(255, 255, 255, 30);
/// `Colours.Label.Dark` (DLabel default) and `SKIN.colTextEntryText`.
const LABEL_DARK: [f32; 4] = rgba(10, 10, 10, 255);
/// `ControlHelp` text: the DForm's dimmed help label.
const HELP_TEXT: [f32; 4] = rgba(90, 90, 90, 255);
const TEXT_ENTRY: [f32; 4] = rgba(20, 20, 20, 255);
/// `SKIN.colNumSliderNotch`.
const NOTCH: [f32; 4] = rgba(0, 0, 0, 100);
/// `Colours.Button.Normal` / `Hover` (DButton and DMenuOption text).
const BUTTON_TEXT: [f32; 4] = rgba(82, 82, 82, 255);
const BUTTON_TEXT_HOVER: [f32; 4] = rgba(46, 114, 178, 255);
/// SpawnIcon under construction (`vgui/spawnmenu/generating`).
pub const GENERATING_MAT: &str = "vgui/spawnmenu/generating";

/// Draws `p` stretched over the rect: corners as is, edges and the middle stretched.
pub fn nine(items: &mut Vec<UiItem>, p: &Piece, x: f32, y: f32, w: f32, h: f32, color: [f32; 4]) {
    let xs = [0.0, p.l, w - p.r, w];
    let ys = [0.0, p.t, h - p.b, h];
    let us = [p.x, p.x + p.l, p.x + p.w - p.r, p.x + p.w];
    let vs = [p.y, p.y + p.t, p.y + p.h - p.b, p.y + p.h];
    for j in 0..3 {
        for i in 0..3 {
            let (w0, h0) = (xs[i + 1] - xs[i], ys[j + 1] - ys[j]);
            if w0 <= 0.0 || h0 <= 0.0 {
                continue;
            }
            items.push(UiItem::Slice {
                x: x + xs[i],
                y: y + ys[j],
                w: w0,
                h: h0,
                material: ATLAS.to_string(),
                color,
                uv: [
                    us[i] / ATLAS_SIZE,
                    vs[j] / ATLAS_SIZE,
                    us[i + 1] / ATLAS_SIZE,
                    vs[j + 1] / ATLAS_SIZE,
                ],
            });
        }
    }
}

#[allow(clippy::too_many_arguments)]
pub(crate) fn text(
    items: &mut Vec<UiItem>,
    font: &str,
    fm: &FontMetrics,
    x: f32,
    y: f32,
    h: f32,
    s: &str,
    color: [f32; 4],
) {
    items.push(UiItem::Text {
        font: Some(font.to_string()),
        x: x.round(),
        y: (y + (h - fm.line_height) / 2.0).round(),
        text: s.to_string(),
        color,
    });
}

pub(crate) fn image(items: &mut Vec<UiItem>, name: &str, x: f32, y: f32, w: f32, h: f32) {
    items.push(UiItem::Image {
        x,
        y,
        w,
        h,
        material: name.to_string(),
        color: WHITE,
        rot: 0,
    });
}

pub(crate) fn inside((cx, cy): (f32, f32), x: f32, y: f32, w: f32, h: f32) -> bool {
    cx >= x && cx < x + w && cy >= y && cy < y + h
}

/// One `settings/spawnlist_default/NNN-name.txt` (or an addon list the server sent).
pub struct SpawnList {
    /// The file's id (its `NNN-` prefix / `id` key); 0 for addon lists.
    pub id: u32,
    pub name: String,
    pub icon: String,
    pub models: Vec<String>,
    pub addon: bool,
    /// The parent list's id (`parentid`; 0 = top level).
    pub parent: u32,
}

/// The client's default spawnlists in id order.
pub fn load_spawnlists(mount: &Mount) -> Vec<SpawnList> {
    let mut out = Vec::new();
    // the engine reads the player's `settings/spawnlist/`, seeded from `spawnlist_default`
    // the player's edited lists on disk (written by `save_spawnlists`) come first
    let mut docs: Vec<(String, String)> = std::fs::read_dir(spawnlist_dir())
        .map(|rd| {
            rd.filter_map(|e| e.ok())
                .map(|e| e.path())
                .filter(|p| p.extension().is_some_and(|x| x == "txt"))
                .filter_map(|p| {
                    let text = std::fs::read_to_string(&p).ok()?;
                    Some((p.to_string_lossy().replace('\\', "/"), text))
                })
                .collect()
        })
        .unwrap_or_default();
    if docs.is_empty() {
        let mut paths = mount.find("settings/spawnlist/*.txt");
        if paths.is_empty() {
            paths = mount.find("settings/spawnlist_default/*.txt");
        }
        docs = paths
            .into_iter()
            .filter_map(|p| mount.read_string(&p).ok().map(|t| (p, t)))
            .collect();
    }
    for (path, text) in docs {
        let Some(doc) = keyvalues::parse(&text).ok() else {
            continue;
        };
        let Some(root) = doc.root() else { continue };
        let file = path
            .rsplit('/')
            .next()
            .unwrap_or(&path)
            .trim_end_matches(".txt");
        let (num, rest) = file.split_once('-').unwrap_or(("0", file));
        let name = root
            .get_str("name")
            .map(str::to_string)
            .unwrap_or_else(|| title_case(rest));
        let icon = root
            .get_str("icon")
            .unwrap_or("icon16/page.png")
            .to_string();
        let id: u32 = root
            .get_str("id")
            .and_then(|s| s.parse().ok())
            .unwrap_or_else(|| num.parse().unwrap_or(0));
        let parent: u32 = root
            .get_str("parentid")
            .and_then(|s| s.parse().ok())
            .unwrap_or(0);
        let models = root
            .get("contents")
            .map(|c| {
                c.children()
                    .iter()
                    .filter(|n| {
                        n.get_str("type")
                            .is_none_or(|t| t.eq_ignore_ascii_case("model"))
                    })
                    .filter_map(|n| n.get_str("model"))
                    .map(str::to_string)
                    .collect()
            })
            .unwrap_or_default();
        out.push((
            id,
            SpawnList {
                id,
                name,
                icon,
                models,
                addon: false,
                parent,
            },
        ));
    }
    out.sort_by_key(|(id, _)| *id);
    out.into_iter().map(|(_, l)| l).collect()
}

/// The player's `settings/spawnlist/` (the engine's; edits copy the defaults there).
pub fn spawnlist_dir() -> std::path::PathBuf {
    crate::settings::home_dir()
        .join("settings")
        .join("spawnlist")
}

/// Writes every non-addon list as `NNN-name.txt` (`TableToKeyValues`, like the game's
/// `spawnmenu.SaveToTextFiles`), replacing the directory's contents.
pub fn save_spawnlists(lists: &[SpawnList]) -> std::io::Result<()> {
    let dir = spawnlist_dir();
    std::fs::create_dir_all(&dir)?;
    for e in std::fs::read_dir(&dir)?.flatten() {
        if e.path().extension().is_some_and(|x| x == "txt") {
            std::fs::remove_file(e.path())?;
        }
    }
    for (i, l) in lists.iter().filter(|l| !l.addon).enumerate() {
        let id = if l.id > 0 { l.id } else { i as u32 + 1 };
        let mut out = String::from("\"TableToKeyValues\"\n{\n");
        out.push_str(&format!("\t\"name\"\t\t\"{}\"\n", l.name));
        out.push_str(&format!("\t\"parentid\"\t\t\"{}\"\n", l.parent));
        out.push_str(&format!("\t\"icon\"\t\t\"{}\"\n", l.icon));
        out.push_str(&format!("\t\"id\"\t\t\"{id}\"\n"));
        out.push_str("\t\"version\"\t\t\"3\"\n\t\"contents\"\n\t{\n");
        for (n, m) in l.models.iter().enumerate() {
            out.push_str(&format!(
                "\t\t\"{}\"\n\t\t{{\n\t\t\t\"type\"\t\t\"model\"\n\t\t\t\"model\"\t\t\"{m}\"\n\t\t}}\n",
                n + 1
            ));
        }
        out.push_str("\t}\n}\n");
        let file: String = l
            .name
            .to_lowercase()
            .chars()
            .map(|c| {
                if c.is_alphanumeric() || c == ' ' {
                    c
                } else {
                    '_'
                }
            })
            .collect();
        std::fs::write(dir.join(format!("{id:03}-{file}.txt")), out)?;
    }
    Ok(())
}

fn title_case(s: &str) -> String {
    s.split(' ')
        .map(|w| {
            let mut c = w.chars();
            match c.next() {
                Some(f) => f.to_uppercase().collect::<String>() + c.as_str(),
                None => String::new(),
            }
        })
        .collect::<Vec<_>>()
        .join(" ")
}

/// `spawnmenu.AddCreationTab` entries in order (sandbox gamemode).
pub const CREATION_TABS: [(&str, &str); 8] = [
    ("Spawnlists", "icon16/application_view_tile.png"),
    ("Weapons", "icon16/gun.png"),
    ("Entities", "icon16/bricks.png"),
    ("NPCs", "icon16/monkey.png"),
    ("Vehicles", "icon16/car.png"),
    ("Post Process", "icon16/picture.png"),
    ("Dupes", "icon16/page_copy.png"),
    ("Saves", "icon16/disk.png"),
];
/// `spawnmenu.AddToolTab` entries (`cl_spawnmenu.lua`).
pub const TOOL_TABS: [(&str, &str); 2] = [
    ("Tools", "icon16/wrench.png"),
    ("Utilities", "icon16/page_white_wrench.png"),
];
/// `spawnmenu.AddToolCategory` order; tools by `TOOL.Category`, names from `tool.properties`,
/// sorted by name as the ToolPanel does.
pub const TOOL_CATEGORIES: [(&str, &[(&str, &str)]); 4] = [
    (
        "Constraints",
        &[
            ("axis", "Axis"),
            ("ballsocket", "Ball Socket"),
            ("elastic", "Elastic"),
            ("hydraulic", "Hydraulic"),
            ("motor", "Motor"),
            ("muscle", "Muscle"),
            ("pulley", "Pulley"),
            ("rope", "Rope"),
            ("slider", "Slider"),
            ("weld", "Weld"),
            ("winch", "Winch"),
        ],
    ),
    (
        "Construction",
        &[
            ("balloon", "Balloons"),
            ("button", "Button"),
            ("duplicator", "Duplicator"),
            ("dynamite", "Dynamite"),
            ("emitter", "Emitter"),
            ("hoverball", "Hoverball"),
            ("lamp", "Lamps"),
            ("light", "Light"),
            ("nocollide", "No Collide"),
            ("physprop", "Physical Properties"),
            ("remover", "Remover"),
            ("thruster", "Thruster"),
            ("wheel", "Wheel"),
        ],
    ),
    (
        "Posing",
        &[
            ("eyeposer", "Eye Poser"),
            ("faceposer", "Face Poser"),
            ("finger", "Finger Poser"),
            ("inflator", "Inflator"),
        ],
    ),
    (
        "Render",
        &[
            ("camera", "Camera"),
            ("colour", "Color"),
            ("material", "Material"),
            ("paint", "Paint"),
            ("trails", "Trails"),
        ],
    ),
];

/// `search.GetResults` of the typed query: models (64 px SpawnIcons) and Weapons / Entities
/// content (128 px ContentIcons, tab 0 / 1).
#[derive(Clone, Debug, Default)]
pub struct SearchResults {
    pub models: Vec<String>,
    pub content: Vec<(usize, String, String)>,
}

/// A category of the Weapons / Entities tabs (`PopulateFromList`): display names by class.
#[derive(Clone, Debug, Default)]
pub struct ContentList {
    pub category: String,
    pub icon: String,
    pub items: Vec<(String, String)>,
    /// A `ContentHeader` (sub-category) before the item at the index.
    pub headers: Vec<(usize, String)>,
}

/// What the player has clicked around in the menu.
#[derive(Default)]
pub struct MenuState {
    pub creation_tab: usize,
    /// The ContentSearch box: its text and whether it takes the keyboard.
    pub search: String,
    pub search_focus: bool,
    /// Selected category of the Weapons / Entities tabs.
    pub content_node: [usize; 2],
    pub tool_tab: usize,
    /// Selected tree node: an index into the spawnlists.
    pub node: usize,
    pub browse_open: bool,
    /// Content scroll in px (a row of icons per wheel notch).
    pub scroll: f32,
    pub collapsed: [bool; 4],
    /// Spawn icons selected with shift-click (the content container's selection canvas).
    pub selected: Vec<String>,
    /// The icons' right-click `DermaMenu` (cursor position, the models it acts on).
    pub ctx_menu: Option<((f32, f32), Vec<String>)>,
    /// The spawnlist tree's right-click `DermaMenu` (cursor position, the list's index).
    pub list_menu: Option<((f32, f32), usize)>,
    /// A press on a spawn icon that may become a drag onto a tree list: where, which models.
    pub drag: Option<((f32, f32), Vec<String>)>,
    /// A spawnlist being renamed in its tree row (index, the typed name).
    pub rename: Option<(usize, String)>,
}

#[derive(Clone, Debug, PartialEq)]
pub enum Action {
    CreationTab(usize),
    ToolTab(usize),
    Node(usize),
    Browse,
    Spawn(String),
    /// A category of the Weapons / Entities tab.
    ContentNode(usize),
    /// The search box clicked: it takes the keyboard until Esc / a click elsewhere.
    SearchFocus,
    /// `gm_giveswep` / `gm_spawnsent` from a ContentIcon.
    GiveWeapon(String),
    SpawnSent(String),
    Category(usize),
    Tool(&'static str),
    /// A `properties.Add` entry of the right-click menu, by its name.
    Property(String),
    /// A main menu entry (`template/main.html` ids / NavBar buttons).
    Menu(&'static str),
    /// An Options dialog control.
    Opt(crate::options::OptAction),
    /// A New Game page control.
    NewGame(crate::newgame::NgAction),
    /// A Servers page control.
    Servers(crate::servers::SvAction),
    /// A Saves / Dupes / Demos page control.
    Ugc(crate::ugc::UgcAction),
    /// An Addons page control.
    Addons(crate::addons::AdAction),
    Nothing,
}

pub struct Hit {
    pub x: f32,
    pub y: f32,
    pub w: f32,
    pub h: f32,
    pub action: Action,
}

#[derive(Default)]
pub struct Layout {
    pub items: Vec<UiItem>,
    pub hits: Vec<Hit>,
    /// The tool ControlPanel rect (x, y, w, h); the tool's setting rows go under its header.
    pub control: (f32, f32, f32, f32),
    /// Models whose spawn icons are on screen (to render the missing ones).
    pub visible: Vec<String>,
    pub max_scroll: f32,
    /// Tooltip rects `(x, y, w, h, text)` of the spawn icons (`SpawnIcon:SetTooltip`).
    pub tips: Vec<(f32, f32, f32, f32, String)>,
}

impl Layout {
    /// The topmost clickable region under the cursor.
    pub fn hit(&self, cursor: (f32, f32)) -> Option<&Action> {
        self.hits
            .iter()
            .rev()
            .find(|h| inside(cursor, h.x, h.y, h.w, h.h))
            .map(|h| &h.action)
    }
}

pub struct Ctx<'a> {
    pub w: f32,
    pub h: f32,
    pub cursor: (f32, f32),
    pub font: &'a FontMetrics,
    pub header: &'a FontMetrics,
    pub bold: &'a FontMetrics,
    pub lists: &'a [SpawnList],
    /// Weapons and Entities tab categories.
    pub content: [&'a [ContentList]; 2],
    /// Results of the current search (`MenuState::search`), shown instead of the node.
    pub results: &'a SearchResults,
    pub tool: &'a str,
    pub has_image: &'a dyn Fn(&str) -> bool,
}

/// `SpawnMenu`: margins `clamp((ScrW-1024)*spawnmenu_border, 25, 256)`, a DHorizontalDivider with
/// the CreationMenu left and the ToolMenu right (min 460 on screens >= 1024 wide).
pub fn build(ctx: &Ctx, st: &MenuState) -> Layout {
    let mut out = Layout::default();
    let mx = ((ctx.w - 1024.0) * 0.1).clamp(25.0, 256.0).round();
    let my = ((ctx.h - 768.0) * 0.1).clamp(25.0, 256.0).round();
    let (px, py, pw, ph) = (mx, my, ctx.w - 2.0 * mx, ctx.h - 2.0 * my);
    let div = 8.0;
    let right_w = if ctx.w >= 1024.0 {
        460.0
    } else {
        (pw * 0.4).round()
    };
    let left_w = pw - div - right_w;
    let (cx, cy, cw, ch) = sheet(
        &mut out,
        ctx,
        px,
        py,
        left_w,
        ph,
        &CREATION_TABS,
        st.creation_tab,
        Action::CreationTab,
    );
    if st.creation_tab == 0 {
        spawnlists(&mut out, ctx, st, cx, cy, cw, ch);
    } else if st.creation_tab == 1 || st.creation_tab == 2 {
        content(&mut out, ctx, st, st.creation_tab - 1, cx, cy, cw, ch);
    }
    let (tx, ty, tw, th) = sheet(
        &mut out,
        ctx,
        px + left_w + div,
        py,
        right_w,
        ph,
        &TOOL_TABS,
        st.tool_tab,
        Action::ToolTab,
    );
    if st.tool_tab == 0 {
        tools(&mut out, ctx, st, tx, ty, tw, th);
    }
    // contenticon.lua `OpenGenericSpawnmenuRightClickMenu` / the selection's "Delete %i items"
    if let Some((at, models)) = &st.ctx_menu {
        let n = models.len();
        let addon = ctx.lists.get(st.node).is_some_and(|l| l.addon);
        let opts = if addon {
            // a read-only (addon) list: the icons go to the player's first spawnlist
            vec![(
                "Copy to my spawnlist".to_string(),
                "icon16/page_copy.png",
                "spawnlist_copy",
            )]
        } else {
            let label = if n > 1 {
                format!("Delete {n} items")
            } else {
                "Delete".to_string()
            };
            vec![(label, "icon16/bin_closed.png", "spawnlist_delete")]
        };
        context_menu(&mut out, ctx, *at, &opts);
    }
    // the tree's menu (contentsidebar.lua): New, and Delete for an editable list
    if let Some((at, i)) = &st.list_menu {
        let editable = ctx.lists.iter().filter(|l| !l.addon).count() > 1
            && ctx.lists.get(*i).is_some_and(|l| !l.addon);
        let mut opts = vec![("New".to_string(), "icon16/add.png", "spawnlist_new")];
        if editable {
            opts.push(("Edit".to_string(), "icon16/pencil.png", "spawnlist_edit"));
            opts.push((
                "Add subcategory".to_string(),
                "icon16/folder_add.png",
                "spawnlist_sub",
            ));
            opts.push((
                "Delete".to_string(),
                "icon16/bin_closed.png",
                "spawnlist_remove",
            ));
        }
        context_menu(&mut out, ctx, *at, &opts);
    }
    // DragNDrop: once the press moved far enough to be a drag, the icon follows the cursor
    // translucent (with the count when a shift selection is dragged)
    if let Some((at, models)) = &st.drag {
        let (cx, cy) = ctx.cursor;
        let moved = (cx - at.0).abs() >= 6.0 || (cy - at.1).abs() >= 6.0;
        if let (true, Some(model)) = (moved, models.first()) {
            let key = format!("icon:{model}");
            if (ctx.has_image)(&key) {
                out.items.push(UiItem::Image {
                    x: cx - 32.0,
                    y: cy - 32.0,
                    w: 64.0,
                    h: 64.0,
                    material: key,
                    color: [1.0, 1.0, 1.0, 0.6],
                    rot: 0,
                });
            }
            if models.len() > 1 {
                let n = models.len().to_string();
                out.items.push(UiItem::Rect {
                    x: cx + 12.0,
                    y: cy + 12.0,
                    w: 20.0,
                    h: 20.0,
                    color: [0.0, 0.0, 0.0, 0.7],
                });
                text(
                    &mut out.items,
                    FONT,
                    ctx.font,
                    cx + 16.0,
                    cy + 13.0,
                    16.0,
                    &n,
                    WHITE,
                );
            }
        }
    }
    out
}

/// A `DermaMenu` of options (label, icon, `Action::Menu` id) at `at`, in the dmenu look.
fn context_menu(
    out: &mut Layout,
    ctx: &Ctx,
    at: (f32, f32),
    opts: &[(String, &str, &'static str)],
) {
    let labels: Vec<String> = opts.iter().map(|(l, _, _)| l.clone()).collect();
    let (x, y0) = at;
    let w = dmenu_width(ctx.font, &labels);
    nine(
        &mut out.items,
        &MENU_BG,
        x,
        y0,
        w,
        MENU_OPTION_H * opts.len() as f32,
        WHITE,
    );
    for (i, (label, icon, id)) in opts.iter().enumerate() {
        let y = y0 + i as f32 * MENU_OPTION_H;
        let hovered = inside(ctx.cursor, x, y, w, MENU_OPTION_H);
        if hovered {
            nine(
                &mut out.items,
                &MENU_BG_HOVER,
                x,
                y,
                w,
                MENU_OPTION_H,
                WHITE,
            );
        }
        if (ctx.has_image)(icon) {
            image(&mut out.items, icon, x + 4.0, y + 3.0, 16.0, 16.0);
        }
        let color = if hovered {
            BUTTON_TEXT_HOVER
        } else {
            BUTTON_TEXT
        };
        text(
            &mut out.items,
            FONT,
            ctx.font,
            x + 32.0,
            y,
            MENU_OPTION_H,
            label,
            color,
        );
        out.hits.push(Hit {
            x,
            y,
            w,
            h: MENU_OPTION_H,
            action: Action::Menu(id),
        });
    }
}

/// `DPropertySheet`: padding 8, tabs in a 3 px inset bar overlapping by 5, the active tab 28 tall
/// over the sheet's top edge, inactive ones 20 tall; returns the sheet's content rect.
#[allow(clippy::too_many_arguments)]
fn sheet(
    out: &mut Layout,
    ctx: &Ctx,
    x: f32,
    y: f32,
    w: f32,
    h: f32,
    tabs: &[(&str, &str)],
    active: usize,
    mk: impl Fn(usize) -> Action,
) -> (f32, f32, f32, f32) {
    const PAD: f32 = 8.0;
    const TAB_H: f32 = 20.0;
    const ACTIVE_H: f32 = 28.0;
    const OVERLAP: f32 = 5.0;
    nine(
        &mut out.items,
        &TAB_CONTROL,
        x,
        y + TAB_H,
        w,
        h - TAB_H,
        WHITE,
    );
    let mut tx = x + 3.0;
    let mut active_tab = None;
    for (i, (label, icon)) in tabs.iter().enumerate() {
        let has_icon = (ctx.has_image)(icon);
        let inset = if has_icon { 28.0 } else { 10.0 };
        let tw = (inset + ctx.font.text_width(label) + 10.0).round();
        if i == active {
            active_tab = Some((tx, tw, *label, *icon, has_icon, inset));
        } else {
            let ty = y + ACTIVE_H - TAB_H;
            nine(&mut out.items, &TABT_INACTIVE, tx, ty, tw, TAB_H, WHITE);
            if has_icon {
                image(&mut out.items, icon, tx + 7.0, ty + 2.0, 16.0, 16.0);
            }
            text(
                &mut out.items,
                FONT,
                ctx.font,
                tx + inset,
                ty,
                TAB_H,
                label,
                TAB_TEXT_INACTIVE,
            );
        }
        out.hits.push(Hit {
            x: tx,
            y,
            w: tw,
            h: ACTIVE_H,
            action: mk(i),
        });
        tx += tw - OVERLAP;
    }
    if let Some((ax, aw, label, icon, has_icon, inset)) = active_tab {
        nine(&mut out.items, &TABT_ACTIVE, ax, y, aw, ACTIVE_H, WHITE);
        if has_icon {
            image(&mut out.items, icon, ax + 7.0, y + 4.0, 16.0, 16.0);
        }
        text(
            &mut out.items,
            FONT,
            ctx.font,
            ax + inset,
            y,
            TAB_H + 4.0,
            label,
            WHITE,
        );
    }
    (
        x + PAD,
        y + TAB_H + PAD,
        w - 2.0 * PAD,
        h - TAB_H - 2.0 * PAD,
    )
}

struct NodeRow {
    label: String,
    icon: String,
    level: f32,
    selected: bool,
    expander: Option<bool>,
    action: Action,
}

/// The Spawnlists tab: `ContentSidebar` (search box, DTree of the lists, 192 wide) left of a
/// `ContentContainer` of 64 px SpawnIcons, divider 6.
fn spawnlists(out: &mut Layout, ctx: &Ctx, st: &MenuState, x: f32, y: f32, w: f32, h: f32) {
    const TREE_W: f32 = 192.0;
    const DIV: f32 = 6.0;
    const NODE_H: f32 = 17.0;
    const ICON: f32 = 64.0;
    search_box(out, ctx, st, x, y, TREE_W);
    let (ty, th) = (y + 24.0, h - 24.0);
    nine(&mut out.items, &TREE, x, ty, TREE_W, th, WHITE);
    // the lists nest by `parentid` (custom.lua AddPropsOfParent), depth first
    fn list_rows(rows: &mut Vec<NodeRow>, ctx: &Ctx, st: &MenuState, parent: u32, level: f32) {
        if level > 8.0 {
            return;
        }
        let is_child = |l: &SpawnList| {
            l.parent != 0 && ctx.lists.iter().any(|p| p.id == l.parent && p.id != l.id)
        };
        for (i, l) in ctx.lists.iter().enumerate() {
            let child = is_child(l);
            if (parent == 0 && child) || (parent != 0 && (!child || l.parent != parent)) {
                continue;
            }
            rows.push(NodeRow {
                label: match &st.rename {
                    Some((r, t)) if *r == i => format!("{t}_"),
                    _ => l.name.clone(),
                },
                icon: l.icon.clone(),
                level: level + if l.addon { 1.0 } else { 0.0 },
                selected: st.node == i,
                expander: None,
                action: Action::Node(i),
            });
            list_rows(rows, ctx, st, l.id, level + 1.0);
        }
    }
    let mut rows: Vec<NodeRow> = Vec::new();
    list_rows(&mut rows, ctx, st, 0, 0.0);
    rows.push(NodeRow {
        label: "Browse".into(),
        icon: "icon16/cog.png".into(),
        level: 0.0,
        selected: false,
        expander: Some(st.browse_open),
        action: Action::Browse,
    });
    if st.browse_open {
        for name in ["Games", "Addons"] {
            rows.push(NodeRow {
                label: name.into(),
                icon: "icon16/folder_database.png".into(),
                level: 1.0,
                selected: false,
                expander: None,
                action: Action::Nothing,
            });
        }
    }
    // DTree_Node: 17 tall, expander 15 px, icon16, label; selected = Selection behind the text
    // (PaintTreeNodeButton: 38 px in, text width + 6), label colours Tree.Normal / Hover / white
    let mut ny = ty + 1.0;
    for row in rows {
        if ny + NODE_H > ty + th {
            break;
        }
        let hovered = inside(ctx.cursor, x, ny, TREE_W, NODE_H);
        let x0 = x + 2.0 + row.level * NODE_H;
        let color = if row.selected {
            let tw = ctx.font.text_width(&row.label);
            nine(
                &mut out.items,
                &SELECTION,
                x0 + 36.0,
                ny,
                tw + 6.0,
                NODE_H,
                WHITE,
            );
            WHITE
        } else if hovered {
            TREE_TEXT_HOVER
        } else {
            TREE_TEXT
        };
        if let Some(open) = row.expander {
            let p = if open { TREE_MINUS } else { TREE_PLUS };
            nine(&mut out.items, &p, x0, ny + 1.0, 15.0, 15.0, WHITE);
        }
        let ix = x0 + NODE_H;
        if (ctx.has_image)(&row.icon) {
            image(&mut out.items, &row.icon, ix, ny, 16.0, 16.0);
        }
        text(
            &mut out.items,
            FONT,
            ctx.font,
            ix + 20.0,
            ny,
            NODE_H,
            &row.label,
            color,
        );
        out.hits.push(Hit {
            x,
            y: ny,
            w: TREE_W,
            h: NODE_H,
            action: row.action,
        });
        ny += NODE_H;
    }

    let (cx, cw) = (x + TREE_W + DIV, w - TREE_W - DIV);
    if !st.search.is_empty() {
        search_results(out, ctx, st, cx, y, cw, h);
        return;
    }
    let Some(list) = ctx.lists.get(st.node) else {
        return;
    };
    let cols = ((cw - 15.0 - 4.0) / ICON).floor().max(1.0) as usize;
    let content_h = list.models.len().div_ceil(cols) as f32 * ICON + 4.0;
    out.max_scroll = ((content_h - h) / ICON).ceil().max(0.0) * ICON;
    let scroll = st.scroll.min(out.max_scroll);
    for (i, model) in list.models.iter().enumerate() {
        let (r, c) = (i / cols, i % cols);
        let ix = cx + 2.0 + c as f32 * ICON;
        let iy = y + 2.0 + r as f32 * ICON - scroll;
        if iy < y {
            continue;
        }
        if iy + ICON > y + h {
            break;
        }
        let key = format!("icon:{model}");
        if (ctx.has_image)(&key) {
            image(&mut out.items, &key, ix, iy, ICON, ICON);
        } else if (ctx.has_image)(GENERATING_MAT) {
            image(&mut out.items, GENERATING_MAT, ix, iy, ICON, ICON);
        }
        out.visible.push(model.clone());
        if inside(ctx.cursor, ix, iy, ICON, ICON) {
            if (ctx.has_image)(HOVER_MAT) {
                image(&mut out.items, HOVER_MAT, ix, iy, ICON, ICON);
            } else {
                out.items.push(UiItem::Rect {
                    x: ix,
                    y: iy,
                    w: ICON,
                    h: ICON,
                    color: HOVER,
                });
            }
        }
        if st.selected.iter().any(|m| m == model) {
            out.items.push(UiItem::Rect {
                x: ix,
                y: iy,
                w: ICON,
                h: ICON,
                color: SELECTED_ICON,
            });
        }
        out.tips.push((ix, iy, ICON, ICON, tip_name(model)));
        out.hits.push(Hit {
            x: ix,
            y: iy,
            w: ICON,
            h: ICON,
            action: Action::Spawn(model.clone()),
        });
    }
    if out.max_scroll > 0.0 {
        let sx = cx + cw - 15.0;
        nine(&mut out.items, &SCROLL_TRACK_V, sx, y, 15.0, h, WHITE);
        let bar_h = (h * h / content_h).max(10.0).round();
        let bar_y = y + ((h - bar_h) * (scroll / out.max_scroll)).round();
        nine(&mut out.items, &SCROLL_BAR_V, sx, bar_y, 15.0, bar_h, WHITE);
    }
}

/// The Weapons / Entities tab (`SpawnmenuContentPanel` + `PopulateFromList`): a sidebar tree
/// with one node per category and 128 px `ContentIcon`s (image inset 3, `gui/ContentIcon-normal`
/// / `-hovered` overlay, DermaDefault name with a shadow 9 px above the bottom).
#[allow(clippy::too_many_arguments)]
fn content(
    out: &mut Layout,
    ctx: &Ctx,
    st: &MenuState,
    tab: usize,
    x: f32,
    y: f32,
    w: f32,
    h: f32,
) {
    const TREE_W: f32 = 192.0;
    const DIV: f32 = 6.0;
    const NODE_H: f32 = 17.0;
    const ICON: f32 = 128.0;
    let lists = ctx.content[tab.min(1)];
    search_box(out, ctx, st, x, y, TREE_W);
    let (ty, th) = (y + 24.0, h - 24.0);
    nine(&mut out.items, &TREE, x, ty, TREE_W, th, WHITE);
    let node = st.content_node[tab.min(1)];
    let mut ny = ty + 1.0;
    for (i, l) in lists.iter().enumerate() {
        if ny + NODE_H > ty + th {
            break;
        }
        let hovered = inside(ctx.cursor, x, ny, TREE_W, NODE_H);
        let x0 = x + 2.0;
        let color = if node == i {
            let tw = ctx.font.text_width(&l.category);
            nine(
                &mut out.items,
                &SELECTION,
                x0 + 36.0,
                ny,
                tw + 6.0,
                NODE_H,
                WHITE,
            );
            WHITE
        } else if hovered {
            TREE_TEXT_HOVER
        } else {
            TREE_TEXT
        };
        let ix = x0 + NODE_H;
        if (ctx.has_image)(&l.icon) {
            image(&mut out.items, &l.icon, ix, ny, 16.0, 16.0);
        }
        text(
            &mut out.items,
            FONT,
            ctx.font,
            ix + 20.0,
            ny,
            NODE_H,
            &l.category,
            color,
        );
        out.hits.push(Hit {
            x,
            y: ny,
            w: TREE_W,
            h: NODE_H,
            action: Action::ContentNode(i),
        });
        ny += NODE_H;
    }

    let (cx, cw) = (x + TREE_W + DIV, w - TREE_W - DIV);
    if !st.search.is_empty() {
        search_results(out, ctx, st, cx, y, cw, h);
        return;
    }
    let Some(list) = lists.get(node) else {
        return;
    };
    const HEADER_H: f32 = 64.0;
    let cols = ((cw - 15.0 - 4.0) / ICON).floor().max(1.0) as usize;
    let mut slots: Vec<(f32, f32)> = Vec::with_capacity(list.items.len());
    let mut headers: Vec<(f32, &str)> = Vec::new();
    let (mut cur, mut col) = (0.0f32, 0usize);
    for i in 0..list.items.len() {
        if let Some((_, label)) = list.headers.iter().find(|(at, _)| *at == i) {
            if col != 0 {
                col = 0;
                cur += ICON;
            }
            headers.push((cur, label.as_str()));
            cur += HEADER_H;
        }
        slots.push((cx + 2.0 + col as f32 * ICON, y + 2.0 + cur));
        col += 1;
        if col == cols {
            col = 0;
            cur += ICON;
        }
    }
    if col != 0 {
        cur += ICON;
    }
    let content_h = cur + 4.0;
    out.max_scroll = ((content_h - h) / ICON).ceil().max(0.0) * ICON;
    let scroll = st.scroll.min(out.max_scroll);
    let line_h = ctx.font.line_height;
    for (hy, label) in headers {
        let hy = y + 2.0 + hy - scroll;
        if hy + HEADER_H < y || hy > y + h {
            continue;
        }
        let hx = cx + 2.0 + 8.0;
        text(
            &mut out.items,
            HEADER_FONT,
            ctx.header,
            hx + 2.0,
            hy + 2.0,
            HEADER_H,
            label,
            [0.0, 0.0, 0.0, 130.0 / 255.0],
        );
        text(
            &mut out.items,
            HEADER_FONT,
            ctx.header,
            hx,
            hy,
            HEADER_H,
            label,
            WHITE,
        );
    }
    for (i, (class, name)) in list.items.iter().enumerate() {
        let (ix, iy) = slots[i];
        let iy = iy - scroll;
        if iy < y {
            continue;
        }
        if iy + ICON > y + h {
            break;
        }
        let hovered = inside(ctx.cursor, ix, iy, ICON, ICON);
        let img = format!("entities/{class}.png");
        if (ctx.has_image)(&img) {
            image(
                &mut out.items,
                &img,
                ix + 3.0,
                iy + 3.0,
                ICON - 8.0,
                ICON - 8.0,
            );
        } else if (ctx.has_image)("gui/noicon.png") {
            image(
                &mut out.items,
                "gui/noicon.png",
                ix + 3.0,
                iy + 3.0,
                ICON - 8.0,
                ICON - 8.0,
            );
        }
        let overlay = if hovered {
            "gui/ContentIcon-hovered.png"
        } else {
            "gui/ContentIcon-normal.png"
        };
        if (ctx.has_image)(overlay) {
            image(&mut out.items, overlay, ix, iy, ICON, ICON);
        }
        let tw = ctx.font.text_width(name);
        let tx = ix + (ICON - tw) * 0.5;
        let tyy = iy + ICON - line_h - 9.0;
        text(
            &mut out.items,
            FONT,
            ctx.font,
            tx + 1.0,
            tyy + 1.0,
            line_h,
            name,
            [0.0, 0.0, 0.0, 200.0 / 255.0],
        );
        text(&mut out.items, FONT, ctx.font, tx, tyy, line_h, name, WHITE);
        out.hits.push(Hit {
            x: ix,
            y: iy,
            w: ICON,
            h: ICON,
            action: if tab == 0 {
                Action::GiveWeapon(class.clone())
            } else {
                Action::SpawnSent(class.clone())
            },
        });
    }
    if out.max_scroll > 0.0 {
        let sx = cx + cw - 15.0;
        nine(&mut out.items, &SCROLL_TRACK_V, sx, y, 15.0, h, WHITE);
        let bar_h = (h * h / content_h).max(10.0).round();
        let bar_y = y + ((h - bar_h) * (scroll / out.max_scroll)).round();
        nine(&mut out.items, &SCROLL_BAR_V, sx, bar_y, 15.0, bar_h, WHITE);
    }
}

/// `ContentSearch`: a DTextEntry with the `#spawnmenu.search` placeholder; typed text with a
/// caret while focused.
fn search_box(out: &mut Layout, ctx: &Ctx, st: &MenuState, x: f32, y: f32, w: f32) {
    nine(&mut out.items, &TEXTBOX, x, y, w, 20.0, WHITE);
    if st.search.is_empty() && !st.search_focus {
        text(
            &mut out.items,
            FONT,
            ctx.font,
            x + 4.0,
            y,
            20.0,
            "Search...",
            PLACEHOLDER,
        );
    } else {
        let shown = if st.search_focus {
            format!("{}|", st.search)
        } else {
            st.search.clone()
        };
        text(
            &mut out.items,
            FONT,
            ctx.font,
            x + 4.0,
            y,
            20.0,
            &shown,
            TREE_TEXT,
        );
    }
    out.hits.push(Hit {
        x,
        y,
        w,
        h: 20.0,
        action: Action::SearchFocus,
    });
}

/// The search PropPanel: model results as 64 px SpawnIcons, then Weapons / Entities results as
/// 128 px ContentIcons (a DIconLayout flow, row by row).
fn search_results(out: &mut Layout, ctx: &Ctx, st: &MenuState, x: f32, y: f32, w: f32, h: f32) {
    const SMALL: f32 = 64.0;
    const BIG: f32 = 128.0;
    let res = ctx.results;
    let cols = ((w - 15.0 - 4.0) / SMALL).floor().max(1.0) as usize;
    let big_cols = ((w - 15.0 - 4.0) / BIG).floor().max(1.0) as usize;
    let small_h = res.models.len().div_ceil(cols) as f32 * SMALL;
    let content_h = small_h + res.content.len().div_ceil(big_cols) as f32 * BIG + 4.0;
    out.max_scroll = ((content_h - h) / SMALL).ceil().max(0.0) * SMALL;
    let scroll = st.scroll.min(out.max_scroll);
    for (i, model) in res.models.iter().enumerate() {
        let (r, c) = (i / cols, i % cols);
        let ix = x + 2.0 + c as f32 * SMALL;
        let iy = y + 2.0 + r as f32 * SMALL - scroll;
        if iy + SMALL < y || iy > y + h {
            continue;
        }
        let key = format!("icon:{model}");
        if (ctx.has_image)(&key) {
            image(&mut out.items, &key, ix, iy, SMALL, SMALL);
        } else if (ctx.has_image)(GENERATING_MAT) {
            image(&mut out.items, GENERATING_MAT, ix, iy, SMALL, SMALL);
        }
        out.visible.push(model.clone());
        if inside(ctx.cursor, ix, iy, SMALL, SMALL) && (ctx.has_image)(HOVER_MAT) {
            image(&mut out.items, HOVER_MAT, ix, iy, SMALL, SMALL);
        }
        if st.selected.iter().any(|m| m == model) {
            out.items.push(UiItem::Rect {
                x: ix,
                y: iy,
                w: SMALL,
                h: SMALL,
                color: SELECTED_ICON,
            });
        }
        out.tips.push((ix, iy, SMALL, SMALL, tip_name(model)));
        out.hits.push(Hit {
            x: ix,
            y: iy,
            w: SMALL,
            h: SMALL,
            action: Action::Spawn(model.clone()),
        });
    }
    let line_h = ctx.font.line_height;
    for (i, (tab, class, name)) in res.content.iter().enumerate() {
        let (r, c) = (i / big_cols, i % big_cols);
        let ix = x + 2.0 + c as f32 * BIG;
        let iy = y + 2.0 + small_h + r as f32 * BIG - scroll;
        if iy + BIG < y || iy > y + h {
            continue;
        }
        let hovered = inside(ctx.cursor, ix, iy, BIG, BIG);
        let img = format!("entities/{class}.png");
        if (ctx.has_image)(&img) {
            image(
                &mut out.items,
                &img,
                ix + 3.0,
                iy + 3.0,
                BIG - 8.0,
                BIG - 8.0,
            );
        }
        let overlay = if hovered {
            "gui/ContentIcon-hovered.png"
        } else {
            "gui/ContentIcon-normal.png"
        };
        if (ctx.has_image)(overlay) {
            image(&mut out.items, overlay, ix, iy, BIG, BIG);
        }
        let tw = ctx.font.text_width(name);
        let (tx, tyy) = (ix + (BIG - tw) * 0.5, iy + BIG - line_h - 9.0);
        text(
            &mut out.items,
            FONT,
            ctx.font,
            tx + 1.0,
            tyy + 1.0,
            line_h,
            name,
            [0.0, 0.0, 0.0, 200.0 / 255.0],
        );
        text(&mut out.items, FONT, ctx.font, tx, tyy, line_h, name, WHITE);
        out.hits.push(Hit {
            x: ix,
            y: iy,
            w: BIG,
            h: BIG,
            action: if *tab == 0 {
                Action::GiveWeapon(class.clone())
            } else {
                Action::SpawnSent(class.clone())
            },
        });
    }
    if out.max_scroll > 0.0 {
        let sx = x + w - 15.0;
        nine(&mut out.items, &SCROLL_TRACK_V, sx, y, 15.0, h, WHITE);
        let bar_h = (h * h / content_h).max(10.0).round();
        let bar_y = y + ((h - bar_h) * (scroll / out.max_scroll)).round();
        nine(&mut out.items, &SCROLL_BAR_V, sx, bar_y, 15.0, bar_h, WHITE);
    }
}

/// `ToolPanel`: a 130 px DCategoryList of the tool categories (search box and the hide-disabled
/// checkbox above it) and the selected tool's ControlPanel right of a 6 px divider.
fn tools(out: &mut Layout, ctx: &Ctx, st: &MenuState, x: f32, y: f32, w: f32, h: f32) {
    const LIST_W: f32 = 130.0;
    const DIV: f32 = 6.0;
    const ROW: f32 = 20.0;
    nine(&mut out.items, &TEXTBOX, x, y, LIST_W - 21.0, 21.0, WHITE);
    text(
        &mut out.items,
        FONT,
        ctx.font,
        x + 4.0,
        y,
        21.0,
        "Search...",
        PLACEHOLDER,
    );
    nine(
        &mut out.items,
        &CHECKBOX,
        x + LIST_W - 15.0,
        y + 3.0,
        15.0,
        15.0,
        WHITE,
    );
    let ly = y + 26.0;
    // DCategoryList (CategoryList.Outer) of DCollapsibleCategory: header 20 tall, rows 20 tall,
    // even / odd rows in the Line / LineAlt colours, the selected tool's row yellow (255 216 0)
    nine(
        &mut out.items,
        &CATEGORY_OUTER,
        x,
        ly,
        LIST_W,
        y + h - ly,
        WHITE,
    );
    let mut cy = ly;
    for (ci, (cat, list)) in TOOL_CATEGORIES.iter().enumerate() {
        if cy + ROW > y + h {
            break;
        }
        let shown = if st.collapsed[ci] {
            0
        } else {
            (((y + h - cy - ROW) / ROW).floor().max(0.0) as usize).min(list.len())
        };
        if shown == 0 {
            nine(&mut out.items, &CATEGORY_HEADER, x, cy, LIST_W, ROW, WHITE);
        } else {
            nine(&mut out.items, &CATEGORY_INNER_H, x, cy, LIST_W, ROW, WHITE);
            nine(
                &mut out.items,
                &CATEGORY_INNER,
                x,
                cy + ROW,
                LIST_W,
                shown as f32 * ROW,
                WHITE,
            );
        }
        let header_color = if st.collapsed[ci] {
            CATEGORY_HEADER_CLOSED_TEXT
        } else {
            CATEGORY_HEADER_TEXT
        };
        text(
            &mut out.items,
            FONT_BOLD,
            ctx.bold,
            x + 4.0,
            cy,
            ROW,
            cat,
            header_color,
        );
        out.hits.push(Hit {
            x,
            y: cy,
            w: LIST_W,
            h: ROW,
            action: Action::Category(ci),
        });
        cy += ROW;
        for (k, (name, label)) in list[..shown].iter().enumerate() {
            let alt = k % 2;
            let hovered = inside(ctx.cursor, x, cy, LIST_W, ROW);
            let (bg, fg) = if ctx.tool == *name {
                (LINE_SELECTED, WHITE)
            } else if hovered {
                (LINE_BUTTON_HOVER[alt], LINE_TEXT_HOVER[alt])
            } else {
                (LINE_BUTTON[alt], LINE_TEXT[alt])
            };
            if bg[3] > 0.0 {
                out.items.push(UiItem::Rect {
                    x,
                    y: cy,
                    w: LIST_W,
                    h: ROW,
                    color: bg,
                });
            }
            text(&mut out.items, FONT, ctx.font, x + 6.0, cy, ROW, label, fg);
            out.hits.push(Hit {
                x,
                y: cy,
                w: LIST_W,
                h: ROW,
                action: Action::Tool(name),
            });
            cy += ROW;
        }
    }
    // ControlPanel is a DForm (a DCollapsibleCategory): InnerH header with the tool's name
    let (rx, rw) = (x + LIST_W + DIV, w - LIST_W - DIV);
    let label = TOOL_CATEGORIES
        .iter()
        .flat_map(|(_, l)| l.iter())
        .find(|(n, _)| *n == ctx.tool)
        .map_or(ctx.tool, |(_, l)| l);
    nine(&mut out.items, &CATEGORY_INNER_H, rx, y, rw, ROW, WHITE);
    nine(
        &mut out.items,
        &CATEGORY_INNER,
        rx,
        y + ROW,
        rw,
        h - ROW,
        WHITE,
    );
    text(
        &mut out.items,
        FONT_BOLD,
        ctx.bold,
        rx + 4.0,
        y,
        ROW,
        label,
        CATEGORY_HEADER_TEXT,
    );
    out.control = (rx, y, rw, h);
}

/// `resource/localization/<lang>/*.properties`: `key=value` lines (`\:` escapes a colon).
pub fn load_properties(mount: &Mount, path: &str) -> BTreeMap<String, String> {
    let Ok(text) = mount.read_string(path) else {
        return BTreeMap::new();
    };
    text.lines()
        .filter(|l| !l.starts_with('#'))
        .filter_map(|l| l.split_once('='))
        .map(|(k, v)| (k.trim().to_string(), v.trim().replace("\\:", ":")))
        .collect()
}

/// A laid-out tool setting: its row and the 0..1 span of its control (a slider's track).
#[derive(Clone, Copy, Debug, Default)]
pub struct ControlRow {
    pub x: f32,
    pub y: f32,
    pub w: f32,
    pub h: f32,
    pub bar: (f32, f32),
}

impl ControlRow {
    pub fn contains(&self, c: (f32, f32)) -> bool {
        inside(c, self.x, self.y, self.w, self.h)
    }
    pub fn fraction(&self, c: (f32, f32)) -> f32 {
        ((c.0 - self.bar.0) / self.bar.1.max(1.0)).clamp(0.0, 1.0)
    }
}

/// The tool's settings as a `DForm` under the ControlPanel header: items docked top with
/// `DockPadding( 10, 10, 10, 0 )`, `DNumSlider` 32 tall (label `w / 2.4`, 45 px value box,
/// notch line 8..w-15, 15 px knob), `DCheckBoxLabel` 16 tall (15 px box, label 9 px after),
/// `TextEntry` = 100x20 label + DTextEntry at 110. Labels from `tool.<tool>.<setting>`.
#[allow(clippy::too_many_arguments)]
pub fn controls(
    out: &mut Layout,
    ctx: &Ctx,
    (rx, ry, rw, rh): (f32, f32, f32, f32),
    schema: &[gm_server::ToolSetting],
    value_of: &dyn Fn(&str) -> String,
    focused: Option<(usize, &str)>,
    label_of: &dyn Fn(&str) -> String,
) -> Vec<ControlRow> {
    const PAD: f32 = 10.0;
    let mut rows = Vec::with_capacity(schema.len());
    let (x, w) = (rx + PAD, rw - 2.0 * PAD);
    let mut y = ry + 20.0;
    for (i, s) in schema.iter().enumerate() {
        y += PAD;
        let label = if s.label.is_empty() {
            label_of(&s.name)
        } else {
            label_of(&s.label)
        };
        let cur = value_of(&s.name);
        let (h, bar) = match s.kind {
            gm_server::ToolSettingKind::Number => {
                let h = 32.0;
                let lw = (w / 2.4).round();
                text(&mut out.items, FONT, ctx.font, x, y, h, &label, LABEL_DARK);
                let (sx, sw) = (x + lw, w - lw - 45.0);
                let (lo, hi) = (s.min.max(-1.0e6), s.max.min(1.0e6));
                let v = cur.trim().parse::<f32>().unwrap_or(lo);
                let t = if hi > lo {
                    ((v - lo) / (hi - lo)).clamp(0.0, 1.0)
                } else {
                    0.0
                };
                if y + h <= ry + rh {
                    out.items.push(UiItem::Rect {
                        x: sx + 8.0,
                        y: (y + h / 2.0 - 1.0).round(),
                        w: sw - 23.0,
                        h: 1.0,
                        color: NOTCH,
                    });
                    nine(
                        &mut out.items,
                        &SLIDER_KNOB,
                        (sx + t * (sw - 15.0)).round(),
                        (y + (h - 15.0) / 2.0).round(),
                        15.0,
                        15.0,
                        WHITE,
                    );
                    text(
                        &mut out.items,
                        FONT,
                        ctx.font,
                        sx + sw + 4.0,
                        y,
                        h,
                        &cur,
                        TEXT_ENTRY,
                    );
                }
                (h, (sx, sw - 15.0))
            }
            gm_server::ToolSettingKind::Bool => {
                let h = 16.0;
                if y + h <= ry + rh {
                    let on = matches!(cur.trim(), "1" | "true" | "on");
                    let p = if on { CHECKBOX_CHECKED } else { CHECKBOX };
                    nine(&mut out.items, &p, x, y, 15.0, 15.0, WHITE);
                    text(
                        &mut out.items,
                        FONT,
                        ctx.font,
                        x + 24.0,
                        y,
                        h,
                        &label,
                        LABEL_DARK,
                    );
                }
                (h, (x, w))
            }
            gm_server::ToolSettingKind::Help => {
                let h = 16.0;
                if y + h <= ry + rh {
                    text(&mut out.items, FONT, ctx.font, x, y, h, &label, HELP_TEXT);
                }
                (h, (x, w))
            }
            gm_server::ToolSettingKind::Button => {
                // DButton, 20 tall, its label centred
                let h = 20.0;
                if y + h <= ry + rh {
                    nine(&mut out.items, &TEXTBOX, x, y, w, h, WHITE);
                    let tw = ctx.font.text_width(&label);
                    text(
                        &mut out.items,
                        FONT,
                        ctx.font,
                        x + (w - tw) / 2.0,
                        y,
                        h,
                        &label,
                        BUTTON_TEXT,
                    );
                }
                (h, (x, w))
            }
            gm_server::ToolSettingKind::Presets => {
                // ControlPresets: a DComboBox of the presets (Default first) and the 20 px
                // wrench button that saves the current settings as a new preset
                let h = 20.0;
                if y + h <= ry + rh {
                    let sel = value_of(&format!("__preset:{}", s.name));
                    let shown = if sel.is_empty() { label } else { sel };
                    nine(&mut out.items, &TEXTBOX, x, y, w - 24.0, h, WHITE);
                    text(
                        &mut out.items,
                        FONT,
                        ctx.font,
                        x + 4.0,
                        y,
                        h,
                        &shown,
                        TEXT_ENTRY,
                    );
                    nine(&mut out.items, &TEXTBOX, x + w - 20.0, y, 20.0, h, WHITE);
                    if (ctx.has_image)("icon16/wrench.png") {
                        image(
                            &mut out.items,
                            "icon16/wrench.png",
                            x + w - 18.0,
                            y + 2.0,
                            16.0,
                            16.0,
                        );
                    }
                }
                (h, (x, w))
            }
            gm_server::ToolSettingKind::Key | gm_server::ToolSettingKind::Choice => {
                let h = 20.0;
                if y + h <= ry + rh {
                    text(&mut out.items, FONT, ctx.font, x, y, h, &label, LABEL_DARK);
                    let (ex, ew) = (x + 110.0, w - 110.0);
                    nine(&mut out.items, &TEXTBOX, ex, y, ew, h, WHITE);
                    let shown = if s.kind == gm_server::ToolSettingKind::Key {
                        pad_key_name(&cur)
                    } else {
                        s.options
                            .iter()
                            .find(|(_, v)| *v == cur.trim())
                            .map_or_else(|| cur.clone(), |(l, _)| label_of(l))
                    };
                    text(
                        &mut out.items,
                        FONT,
                        ctx.font,
                        ex + 4.0,
                        y,
                        h,
                        &shown,
                        TEXT_ENTRY,
                    );
                }
                (h, (x, w))
            }
            gm_server::ToolSettingKind::Color => {
                let h = 64.0;
                if y + h <= ry + rh {
                    text(
                        &mut out.items,
                        FONT,
                        ctx.font,
                        x,
                        y,
                        16.0,
                        &label,
                        LABEL_DARK,
                    );
                    let names = [
                        s.name.as_str(),
                        s.options.first().map_or("", |(n, _)| n.as_str()),
                        s.options.get(1).map_or("", |(n, _)| n.as_str()),
                    ];
                    for (k, name) in names.iter().enumerate() {
                        let cy = y + 16.0 + k as f32 * 16.0;
                        let v = value_of(name).trim().parse::<f32>().unwrap_or(255.0);
                        let t = (v / 255.0).clamp(0.0, 1.0);
                        let sw = w - 45.0;
                        out.items.push(UiItem::Rect {
                            x: x + 8.0,
                            y: (cy + 7.0).round(),
                            w: sw - 23.0,
                            h: 1.0,
                            color: NOTCH,
                        });
                        nine(
                            &mut out.items,
                            &SLIDER_KNOB,
                            (x + t * (sw - 15.0)).round(),
                            cy,
                            15.0,
                            15.0,
                            WHITE,
                        );
                        text(
                            &mut out.items,
                            FONT,
                            ctx.font,
                            x + sw + 4.0,
                            cy,
                            16.0,
                            &format!("{v:.0}"),
                            TEXT_ENTRY,
                        );
                    }
                }
                (h, (x, w - 60.0))
            }
            gm_server::ToolSettingKind::Text => {
                let h = 20.0;
                if y + h <= ry + rh {
                    text(&mut out.items, FONT, ctx.font, x, y, h, &label, LABEL_DARK);
                    let (ex, ew) = (x + 110.0, w - 110.0);
                    let focus = focused.filter(|(r, _)| *r == i);
                    let p = if focus.is_some() {
                        TEXTBOX_FOCUS
                    } else {
                        TEXTBOX
                    };
                    nine(&mut out.items, &p, ex, y, ew, h, WHITE);
                    let shown = match focus {
                        Some((_, v)) => format!("{v}_"),
                        None => cur.clone(),
                    };
                    text(
                        &mut out.items,
                        FONT,
                        ctx.font,
                        ex + 4.0,
                        y,
                        h,
                        &shown,
                        TEXT_ENTRY,
                    );
                }
                (h, (x, w))
            }
        };
        rows.push(ControlRow { x, y, w, h, bar });
        y += h;
    }
    rows
}

/// The label of a stool setting as its `BuildCPanel` names it: `tool.<tool>.<key>`, the shared
/// `tool.<key>` (`forcelimit`, `torquelimit`, `nocollide`, `toggle`), with the game's names
/// where our field differs from the convar; else the key itself.
pub fn setting_label(lang: &BTreeMap<String, String>, tool: &str, name: &str) -> String {
    if let Some(k) = name.strip_prefix('#') {
        return lang
            .get(k)
            .cloned()
            .unwrap_or_else(|| title_case(&k.rsplit('.').next().unwrap_or(k).replace('_', " ")));
    }
    let key = name
        .strip_prefix(tool)
        .and_then(|r| r.strip_prefix('_'))
        .unwrap_or(name);
    let alias = match (tool, key) {
        (_, "friction") => "hingefriction",
        ("hydraulic", "key") => "controls",
        ("muscle", "key") => "numpad",
        ("winch", "fwd_key") | ("thruster", "key") | ("wheel", "fwd") => "forward",
        ("winch", "bwd_key") => "backward",
        ("thruster", "key_back") => "back",
        ("wheel", "bck") => "reverse",
        ("hoverball", "key_up") => "up",
        ("hoverball", "key_down") => "down",
        ("hoverball", "key_on") | ("button", "keygroup") => "key",
        ("dynamite", "group") => "explode",
        ("winch", "fwd_speed") => "fspeed",
        ("winch", "bwd_speed") => "bspeed",
        ("muscle", "addlength") => "length",
        (_, k) => k,
    };
    [
        format!("tool.{tool}.{alias}"),
        format!("tool.{alias}"),
        format!("tool.{tool}.{key}"),
        format!("tool.{key}"),
    ]
    .iter()
    .find_map(|k| lang.get(k).cloned())
    .unwrap_or_else(|| title_case(&key.replace('_', " ")))
}

/// `lua/autorun/properties/*.lua`: (name, MenuLabel key in context.properties, MenuIcon, Order).
pub const PROPERTIES: [(&str, &str, &str, u32); 20] = [
    ("persist", "makepersistent", "icon16/link.png", 400),
    (
        "persist_end",
        "stoppersisting",
        "icon16/link_break.png",
        400,
    ),
    ("bone_manipulate", "edit_bones", "icon16/vector.png", 500),
    (
        "bone_manipulate_end",
        "stop_editing_bones",
        "icon16/vector_delete.png",
        500,
    ),
    ("bodygroups", "bodygroups", "icon16/link_edit.png", 600),
    ("skin", "skin", "icon16/picture_edit.png", 601),
    ("keepupright", "keepupright", "icon16/arrow_up.png", 900),
    (
        "keepupright_stop",
        "keepupright_stop",
        "icon16/arrow_rotate_clockwise.png",
        900,
    ),
    ("ignite", "ignite", "icon16/fire.png", 999),
    ("extinguish", "extinguish", "icon16/water.png", 999),
    ("remove", "remove", "icon16/delete.png", 1000),
    ("gravity", "gravity", "", 1001),
    ("drive", "drive", "icon16/joystick.png", 1100),
    (
        "collision_off",
        "collision_off",
        "icon16/collision_off.png",
        1500,
    ),
    (
        "collision_on",
        "collision_on",
        "icon16/collision_on.png",
        1500,
    ),
    ("statue", "makestatue", "icon16/lock.png", 1501),
    ("statue_stop", "unstatue", "icon16/lock_open.png", 1501),
    (
        "npc_bigger",
        "biggify",
        "icon16/magnifier_zoom_in.png",
        1799,
    ),
    (
        "npc_smaller",
        "smallify",
        "icon16/magifier_zoom_out.png",
        1800,
    ),
    ("editentity", "entedit", "icon16/pencil.png", 90001),
];

fn property(name: &str) -> Option<&'static (&'static str, &'static str, &'static str, u32)> {
    PROPERTIES.iter().find(|p| p.0 == name)
}

/// The right-click menu's entries in the game's `Order`, unknown names last as they came.
pub fn sort_properties(names: &mut [String]) {
    names.sort_by_key(|n| property(n).map_or(u32::MAX, |p| p.3));
}

pub fn property_label(lang: &BTreeMap<String, String>, name: &str) -> String {
    let key = property(name).map_or(name, |p| p.1);
    lang.get(key)
        .cloned()
        .unwrap_or_else(|| title_case(&name.replace('_', " ")))
}

const MENU_OPTION_H: f32 = 22.0;

/// A `DMenuOption`: a leaf runs the property `name`; an entry with `children` opens a submenu
/// (`DMenuOption:AddSubMenu`, right arrow); `checked` draws `Menu_Check` (`SetChecked`).
#[derive(Clone, Debug, Default)]
pub struct MenuEntry {
    pub name: String,
    pub label: String,
    pub icon: &'static str,
    pub checked: Option<bool>,
    pub children: Vec<MenuEntry>,
}

/// `string.NiceName`: underscores to spaces, a word split before each capital that does not
/// continue an all-caps run, first letter upper-cased.
pub fn nice_name(name: &str) -> String {
    let mut parts: Vec<String> = Vec::new();
    for word in name.replace('_', " ").split(' ') {
        let chars: Vec<char> = word.chars().collect();
        let mut start = 0;
        for i in 1..chars.len() {
            if chars[i].is_uppercase() {
                let chunk: String = chars[start..i].iter().collect();
                if chunk.to_uppercase() == chunk {
                    continue;
                }
                parts.push(chunk);
                start = i;
            }
        }
        parts.push(chars[start..].iter().collect());
    }
    let joined = parts.join(" ");
    let mut c = joined.chars();
    match c.next() {
        Some(f) => f.to_uppercase().collect::<String>() + c.as_str(),
        None => String::new(),
    }
}

/// The bodygroup option labels of `properties/bodygroups.lua`: the submodel's file name
/// without extension (trimmed of `.`, `/`, `\`), else `Model #i` (1-based).
fn submodel_label(name: &str, i: usize) -> String {
    let n = name.trim_matches(|c| matches!(c, '.' | '/' | '\\'));
    let n = n.rsplit(['/', '\\']).next().unwrap_or(n);
    let n = n.rsplit_once('.').map_or(n, |(stem, _)| stem);
    if n.is_empty() {
        format!("Model #{}", i + 1)
    } else {
        nice_name(n)
    }
}

/// The entries of `ServerMsg::Properties` as the game's menu: `skin N` grouped under Skin
/// (`skinX` options checked at the entity's skin), `bodygroups P M` under Body Groups as in
/// `properties/bodygroups.lua` (a two-state part is one checkable option that toggles, a
/// larger one a submenu of its submodels, names from the MDL via `groups[part]`), the rest
/// flat.
pub fn menu_entries(
    lang: &BTreeMap<String, String>,
    names: &[String],
    skin: u8,
    body: &str,
    groups: &[(String, Vec<String>)],
) -> Vec<MenuEntry> {
    let icon_of = |n: &str| PROPERTIES.iter().find(|p| p.0 == n).map_or("", |p| p.2);
    let skin_fmt = lang
        .get("skinX")
        .cloned()
        .unwrap_or_else(|| "Skin %i".into());
    let mut out: Vec<MenuEntry> = Vec::new();
    for n in names {
        if let Some(i) = n.strip_prefix("skin ") {
            let child = MenuEntry {
                name: n.clone(),
                label: skin_fmt.replace("%i", i).replace("%s", i),
                icon: "",
                checked: Some(i.parse::<u8>().is_ok_and(|v| v == skin)),
                children: Vec::new(),
            };
            match out.iter_mut().find(|e| e.name == "skin") {
                Some(e) => e.children.push(child),
                None => out.push(MenuEntry {
                    name: "skin".into(),
                    label: property_label(lang, "skin"),
                    icon: icon_of("skin"),
                    checked: None,
                    children: vec![child],
                }),
            }
        } else if let Some(rest) = n.strip_prefix("bodygroups ") {
            let (part, model) = rest.split_once(' ').unwrap_or((rest, "0"));
            let current = part
                .parse::<usize>()
                .ok()
                .and_then(|p| body.chars().nth(p))
                .map_or(0, |c| c.to_digit(10).unwrap_or(0));
            let part_idx = part.parse::<usize>().unwrap_or(usize::MAX);
            let model_idx = model.parse::<usize>().unwrap_or(0);
            let group = groups.get(part_idx);
            let part_label =
                group.map_or_else(|| format!("Body Group {part}"), |(name, _)| nice_name(name));
            let states = names
                .iter()
                .filter(|m| {
                    m.strip_prefix("bodygroups ")
                        .and_then(|r| r.split_once(' '))
                        .is_some_and(|(p, _)| p == part)
                })
                .count();
            if !out.iter().any(|e| e.name == "bodygroups") {
                out.push(MenuEntry {
                    name: "bodygroups".into(),
                    label: property_label(lang, "bodygroups"),
                    icon: icon_of("bodygroups"),
                    checked: None,
                    children: Vec::new(),
                });
            }
            let root = out.iter_mut().find(|e| e.name == "bodygroups").unwrap();
            if states == 2 {
                // one checkable option: checked at state 1, a click sets the other state
                if model_idx == 0 {
                    root.children.push(MenuEntry {
                        name: format!("bodygroups {part} {}", if current == 1 { 0 } else { 1 }),
                        label: part_label,
                        icon: "",
                        checked: Some(current == 1),
                        children: Vec::new(),
                    });
                }
                continue;
            }
            let child = MenuEntry {
                name: n.clone(),
                label: group.map_or_else(
                    || format!("Model #{}", model_idx + 1),
                    |(_, subs)| submodel_label(subs.get(model_idx).map_or("", |s| s), model_idx),
                ),
                icon: "",
                checked: Some(model_idx as u32 == current),
                children: Vec::new(),
            };
            let group_name = format!("bodygroups {part}");
            match root.children.iter_mut().find(|g| g.name == group_name) {
                Some(g) => g.children.push(child),
                None => root.children.push(MenuEntry {
                    name: group_name,
                    label: part_label,
                    icon: "",
                    checked: None,
                    children: vec![child],
                }),
            }
        } else {
            out.push(MenuEntry {
                name: n.clone(),
                label: property_label(lang, n),
                icon: icon_of(n),
                checked: None,
                children: Vec::new(),
            });
        }
    }
    out
}

/// Where each open level of a DMenu tree sits: (x, y, entries) for the root and the submenus
/// along `open` (a submenu opens at the right edge of its parent, level with its option).
fn menu_levels<'a>(
    font: &FontMetrics,
    x: f32,
    y: f32,
    entries: &'a [MenuEntry],
    open: &[usize],
) -> Vec<(f32, f32, &'a [MenuEntry])> {
    let mut levels = vec![(x, y, entries)];
    let (mut lx, mut ly, mut cur) = (x, y, entries);
    for &i in open {
        let Some(e) = cur.get(i).filter(|e| !e.children.is_empty()) else {
            break;
        };
        let labels: Vec<String> = cur.iter().map(|e| e.label.clone()).collect();
        let w = dmenu_width(font, &labels);
        lx += w;
        ly += i as f32 * MENU_OPTION_H;
        cur = &e.children;
        levels.push((lx, ly, cur));
    }
    levels
}

/// The (level, row) under the cursor of an open DMenu tree.
fn menu_hit(
    font: &FontMetrics,
    x: f32,
    y: f32,
    entries: &[MenuEntry],
    open: &[usize],
    c: (f32, f32),
) -> Option<(usize, usize)> {
    menu_levels(font, x, y, entries, open)
        .iter()
        .enumerate()
        .rev()
        .find_map(|(lvl, (lx, ly, es))| {
            let labels: Vec<String> = es.iter().map(|e| e.label.clone()).collect();
            dmenu_row(font, *lx, *ly, &labels, c).map(|r| (lvl, r))
        })
}

/// Hovering an option with a submenu opens it (and closes deeper ones); hovering a leaf of a
/// level closes the levels below it; outside the menu nothing changes.
pub fn menu_hover(
    font: &FontMetrics,
    x: f32,
    y: f32,
    entries: &[MenuEntry],
    open: &mut Vec<usize>,
    c: (f32, f32),
) {
    if let Some((lvl, row)) = menu_hit(font, x, y, entries, open, c) {
        open.truncate(lvl);
        let levels = menu_levels(font, x, y, entries, open);
        if levels[lvl]
            .2
            .get(row)
            .is_some_and(|e| !e.children.is_empty())
        {
            open.push(row);
        }
    }
}

/// The leaf property under the cursor, if any.
pub fn menu_leaf(
    font: &FontMetrics,
    x: f32,
    y: f32,
    entries: &[MenuEntry],
    open: &[usize],
    c: (f32, f32),
) -> Option<String> {
    let (lvl, row) = menu_hit(font, x, y, entries, open, c)?;
    let levels = menu_levels(font, x, y, entries, open);
    let e = levels[lvl].2.get(row)?;
    e.children.is_empty().then(|| e.name.clone())
}

/// A DMenu tree: the root and the open submenus, each a `dmenu` with arrows on submenu
/// options and `Menu_Check` on checked ones (`SKIN:PaintMenuOption`).
pub fn dmenu_tree(
    out: &mut Layout,
    ctx: &Ctx,
    x: f32,
    y: f32,
    entries: &[MenuEntry],
    open: &[usize],
) {
    for (lx, ly, es) in menu_levels(ctx.font, x, y, entries, open) {
        let labels: Vec<String> = es.iter().map(|e| e.label.clone()).collect();
        let w = dmenu_width(ctx.font, &labels);
        let names: Vec<String> = es.iter().map(|e| e.name.clone()).collect();
        let icons: Vec<&str> = es.iter().map(|e| e.icon).collect();
        dmenu(out, ctx, lx, ly, &names, &labels, &icons);
        for (i, e) in es.iter().enumerate() {
            let oy = ly + i as f32 * MENU_OPTION_H;
            if e.checked == Some(true) {
                nine(
                    &mut out.items,
                    &MENU_CHECK,
                    lx + 5.0,
                    oy + MENU_OPTION_H * 0.5 - 7.0,
                    15.0,
                    15.0,
                    WHITE,
                );
            }
            if !e.children.is_empty() {
                nine(
                    &mut out.items,
                    &MENU_ARROW,
                    lx + w - 15.0 - 4.0,
                    oy + MENU_OPTION_H * 0.5 - 7.0,
                    15.0,
                    15.0,
                    WHITE,
                );
            }
        }
    }
}

/// `DMenu` width for these labels: at least the minimum 100, options are `32 + text + 30`.
pub fn dmenu_width(font: &FontMetrics, labels: &[String]) -> f32 {
    labels
        .iter()
        .map(|l| (32.0 + font.text_width(l) + 30.0).ceil())
        .fold(100.0_f32, f32::max)
}

/// The option under the cursor of a DMenu at (x, y).
pub fn dmenu_row(
    font: &FontMetrics,
    x: f32,
    y: f32,
    labels: &[String],
    c: (f32, f32),
) -> Option<usize> {
    let w = dmenu_width(font, labels);
    if !inside(c, x, y, w, MENU_OPTION_H * labels.len() as f32) {
        return None;
    }
    Some(((c.1 - y) / MENU_OPTION_H) as usize)
}

/// A `DMenu` of `DMenuOption`s (22 tall, icon at 4, text inset 32): MenuBG, MenuBG_Hover,
/// text in the Button colours.
pub fn dmenu(
    out: &mut Layout,
    ctx: &Ctx,
    x: f32,
    y: f32,
    names: &[String],
    labels: &[String],
    icons: &[&str],
) {
    let w = dmenu_width(ctx.font, labels);
    nine(
        &mut out.items,
        &MENU_BG,
        x,
        y,
        w,
        MENU_OPTION_H * names.len() as f32,
        WHITE,
    );
    for (i, ((name, label), icon)) in names.iter().zip(labels).zip(icons).enumerate() {
        let oy = y + i as f32 * MENU_OPTION_H;
        let hovered = inside(ctx.cursor, x, oy, w, MENU_OPTION_H);
        if hovered {
            nine(
                &mut out.items,
                &MENU_BG_HOVER,
                x,
                oy,
                w,
                MENU_OPTION_H,
                WHITE,
            );
        }
        if !icon.is_empty() && (ctx.has_image)(icon) {
            image(&mut out.items, icon, x + 4.0, oy + 3.0, 16.0, 16.0);
        }
        let color = if hovered {
            BUTTON_TEXT_HOVER
        } else {
            BUTTON_TEXT
        };
        text(
            &mut out.items,
            FONT,
            ctx.font,
            x + 32.0,
            oy,
            MENU_OPTION_H,
            label,
            color,
        );
        out.hits.push(Hit {
            x,
            y: oy,
            w,
            h: MENU_OPTION_H,
            action: Action::Property(name.clone()),
        });
    }
}

/// The name a numpad KeyBinder shows for a key code (`KEY_PAD_0..9` = 37..46; 0 = none).
/// `contenttypes/model.lua`: a spawn icon's tooltip is the model's file name without `.mdl`.
pub fn tip_name(model: &str) -> String {
    model
        .rsplit('/')
        .next()
        .unwrap_or(model)
        .trim_end_matches(".mdl")
        .to_string()
}
/// Overlay of a shift-selected spawn icon (approximation of the selection canvas's highlight).
const SELECTED_ICON: [f32; 4] = rgba(255, 216, 0, 90);
pub fn pad_key_name(code: &str) -> String {
    match code.trim().parse::<i32>().unwrap_or(0) {
        0 => "NONE".into(),
        n @ 37..=46 => format!("KP_{}", n - 37),
        n => format!("KEY {n}"),
    }
}

/// The DForm's height for a schema: header 20 + each item's 10 px top padding and height.
pub fn controls_height(schema: &[gm_server::ToolSetting]) -> f32 {
    20.0 + schema
        .iter()
        .map(|s| {
            10.0 + match s.kind {
                gm_server::ToolSettingKind::Number => 32.0,
                gm_server::ToolSettingKind::Bool => 16.0,
                gm_server::ToolSettingKind::Text
                | gm_server::ToolSettingKind::Key
                | gm_server::ToolSettingKind::Choice
                | gm_server::ToolSettingKind::Presets
                | gm_server::ToolSettingKind::Button => 20.0,
                gm_server::ToolSettingKind::Help => 16.0,
                gm_server::ToolSettingKind::Color => 64.0,
            }
        })
        .sum::<f32>()
}

/// `ContextMenu:PerformLayout`: the active ControlPanel in a 320 wide DCategoryList at
/// `(ScrW - 320 - 50, ScrH - 50 - tall)`, `tall = min(panel + 10, ScrH * 0.8)`; returns the
/// panel rect inside it.
pub fn context_canvas(
    out: &mut Layout,
    ctx: &Ctx,
    schema: &[gm_server::ToolSetting],
) -> (f32, f32, f32, f32) {
    let tall = (controls_height(schema) + 10.0).min(ctx.h * 0.8).round();
    let (x, y) = (ctx.w - 320.0 - 50.0, ctx.h - 50.0 - tall);
    nine(&mut out.items, &CATEGORY_OUTER, x, y, 320.0, tall, WHITE);
    // the ControlPanel (a DForm) keeps its header with the tool's name
    let label = TOOL_CATEGORIES
        .iter()
        .flat_map(|(_, l)| l.iter())
        .find(|(n, _)| *n == ctx.tool)
        .map_or(ctx.tool, |(_, l)| l);
    nine(&mut out.items, &CATEGORY_INNER_H, x, y, 320.0, 20.0, WHITE);
    nine(
        &mut out.items,
        &CATEGORY_INNER,
        x,
        y + 20.0,
        320.0,
        tall - 30.0,
        WHITE,
    );
    text(
        &mut out.items,
        FONT_BOLD,
        ctx.bold,
        x + 4.0,
        y,
        20.0,
        label,
        CATEGORY_HEADER_TEXT,
    );
    (x, y, 320.0, tall - 10.0)
}

/// Main menu fonts: `DIV.options A` 23 px bold (16 px at <= 1280 wide), NavBar body 12 px.
pub const MENU_FONT_23: &str = "MenuOptions23";
pub const MENU_FONT_16: &str = "MenuOptions16";
pub const NAV_FONT: &str = "MenuNav";
/// `html/img` and gamemode images the menu draws (raw PNG paths).
pub const MENU_IMAGES: [&str; 19] = [
    "html/img/gradient.png",
    "html/img/downloading.png",
    "html/img/arrow_left.png",
    "html/img/arrow_right.png",
    "html/img/pagination_page.png",
    "html/img/notfavourite.png",
    "html/img/bg_arrow_left.png",
    "html/img/back_to_main_menu.png",
    "html/img/favourite.png",
    "html/img/favourite_add.png",
    "html/img/favourite_remove.png",
    "maps/thumb/noicon.png",
    "gamemodes/sandbox/logo.png",
    "gamemodes/sandbox/icon24.png",
    "html/img/disconnect.png",
    "html/img/back_to_game.png",
    "html/img/error.png",
    "html/img/games.png",
    "resource/localization/en.png",
];

#[allow(clippy::too_many_arguments)]
fn shadowed(
    out: &mut Layout,
    font: &str,
    fm: &FontMetrics,
    x: f32,
    y: f32,
    h: f32,
    s: &str,
    color: [f32; 4],
    off: f32,
) {
    text(
        out_items(out),
        font,
        fm,
        x + off,
        y + off,
        h,
        s,
        rgba(0, 0, 0, 255),
    );
    text(out_items(out), font, fm, x, y, h, s, color);
}

fn out_items(out: &mut Layout) -> &mut Vec<UiItem> {
    &mut out.items
}

/// `menu.html` + `template/main.html` as the CSS lays them out: the gradient 1024 wide, the
/// options list (logo, entries 23/16 px bold white with a 2 px shadow, hover #ffa) at
/// `50 + 20 + 16` from the left and 50 from the top, the 50 px NavBar (black 50 %) with 40 px
/// white buttons (text #333, icon at 5,5, span padding 10). Backgrounds only out of game.
#[allow(clippy::too_many_arguments)]
pub fn main_menu(
    out: &mut Layout,
    ctx: &Ctx,
    in_game: bool,
    main_page: bool,
    lang: &BTreeMap<String, String>,
    opt: &FontMetrics,
    opt_font: &str,
    nav: &FontMetrics,
) {
    let (w, h) = (ctx.w, ctx.h);
    if (ctx.has_image)("html/img/gradient.png") {
        image(&mut out.items, "html/img/gradient.png", 0.0, 0.0, 1024.0, h);
    }
    let say = |k: &str, d: &str| lang.get(k).cloned().unwrap_or_else(|| d.to_string());
    let x = 50.0 + 20.0 + 16.0;
    let mut y = 50.0 + 2.0;
    let line = (opt.line_height + 1.0).round();
    let hover = rgba(255, 255, 170, 255);
    let mut entries: Vec<Option<(&'static str, String)>> = Vec::new();
    if in_game {
        entries.push(Some(("resume_game", say("resume_game", "Resume Game"))));
        entries.push(None);
    }
    entries.push(Some(("newgame", say("new_game", "Start New Game"))));
    entries.push(Some((
        "servers",
        say("find_mp_game", "Find Multiplayer Game"),
    )));
    entries.push(None);
    entries.push(Some(("addons", say("addons", "Addons"))));
    entries.push(Some(("dupes", say("dupes", "Dupes"))));
    entries.push(Some(("saves", say("saves", "Saves"))));
    entries.push(Some(("demos", say("demos", "Demos"))));
    entries.push(None);
    entries.push(Some(("options", say("options", "Options"))));
    entries.push(None);
    if in_game {
        entries.push(Some(("disconnect", say("disconnect", "Disconnect"))));
    }
    entries.push(Some(("quit", say("quit", "Quit"))));
    if !main_page {
        entries.clear();
    } else if (ctx.has_image)("gamemodes/sandbox/logo.png") {
        image(
            &mut out.items,
            "gamemodes/sandbox/logo.png",
            x,
            y,
            288.0,
            128.0,
        );
    }
    y += 128.0 + line;
    for e in entries {
        if let Some((id, label)) = e {
            let tw = opt.text_width(&label);
            let hovered = inside(ctx.cursor, x, y, tw, line);
            let color = if hovered { hover } else { WHITE };
            shadowed(out, opt_font, opt, x, y, line, &label, color, 2.0);
            out.hits.push(Hit {
                x,
                y,
                w: tw,
                h: line,
                action: Action::Menu(id),
            });
        }
        y += line;
    }

    // NavBar
    let bar_y = h - 50.0;
    out.items.push(UiItem::Rect {
        x: 0.0,
        y: bar_y,
        w,
        h: 50.0,
        color: rgba(0, 0, 0, 128),
    });
    let dark = rgba(51, 51, 51, 255);
    let button = |out: &mut Layout,
                  right_edge: f32,
                  label: Option<&str>,
                  icon: &str,
                  id: &'static str|
     -> f32 {
        let tw = label.map_or(0.0, |l| nav.text_width(l));
        let bw = if label.is_some() {
            35.0 + tw + 10.0
        } else {
            40.0
        }
        .round();
        let bx = right_edge - bw;
        let by = bar_y + 5.0;
        let hovered = inside(ctx.cursor, bx, by, bw, 40.0);
        out.items.push(UiItem::Rect {
            x: bx,
            y: by,
            w: bw,
            h: 40.0,
            color: if hovered {
                rgba(255, 255, 255, 255)
            } else {
                rgba(255, 255, 255, 240)
            },
        });
        if (ctx.has_image)(icon) {
            let (ix, iy, sz) = if label.is_some() {
                (bx + 5.0, by + 5.0, 30.0)
            } else {
                (bx + 7.0, by + 7.0, 26.0)
            };
            image(&mut out.items, icon, ix, iy, sz, sz);
        }
        if let Some(l) = label {
            text(
                &mut out.items,
                NAV_FONT,
                nav,
                bx + 35.0 + 10.0 - 10.0 + 10.0,
                by,
                40.0,
                l,
                dark,
            );
        }
        out.hits.push(Hit {
            x: bx,
            y: by,
            w: bw,
            h: 40.0,
            action: Action::Menu(id),
        });
        bx - 3.0
    };
    if !main_page {
        // #BackToMenu: left group, padding-left 5
        let label = say("back_to_main_menu", "Back to Main Menu");
        let bw = 35.0 + nav.text_width(&label) + 10.0 + 5.0;
        let _ = button(
            out,
            5.0 + bw,
            Some(&label),
            "html/img/back_to_main_menu.png",
            "back",
        );
    }
    let mut right = w - 3.0;
    right = button(
        out,
        right,
        Some("Sandbox"),
        "gamemodes/sandbox/icon24.png",
        "gamemodes",
    );
    right = button(out, right, None, "resource/localization/en.png", "language");
    right = button(out, right, None, "html/img/games.png", "games");
    // the Problems button (`html/img/error.png`) only appears when the menu has problems to
    // list; there is no problems list yet, so it is left out
    // the centre group fills between the groups with 10 % margins and packs to its end
    if in_game {
        let mut c = right - w * 0.1;
        c = button(
            out,
            c,
            Some(&say("back_to_game", "Back to Game")),
            "html/img/back_to_game.png",
            "resume_game",
        );
        let _ = button(
            out,
            c,
            Some(&say("disconnect", "Disconnect")),
            "html/img/disconnect.png",
            "disconnect",
        );
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn nice_name_splits_words_like_the_game() {
        assert_eq!(nice_name("wheel_hub"), "Wheel hub");
        assert_eq!(nice_name("lowerBody"), "Lower Body");
        assert_eq!(nice_name("HDTV_screen"), "HDTV screen");
        assert_eq!(nice_name("studio"), "Studio");
    }

    #[test]
    fn bodygroups_are_a_toggle_at_two_states_and_a_submenu_above() {
        let lang = BTreeMap::new();
        let names: Vec<String> = [
            "bodygroups 0 0",
            "bodygroups 0 1",
            "bodygroups 1 0",
            "bodygroups 1 1",
            "bodygroups 1 2",
        ]
        .iter()
        .map(|s| s.to_string())
        .collect();
        let groups = vec![
            ("hat".to_string(), vec!["cap.smd".into(), "blank".into()]),
            (
                "arms".to_string(),
                vec!["arms_normal.smd".into(), "".into(), "arms_long".into()],
            ),
        ];
        let out = menu_entries(&lang, &names, 0, "01", &groups);
        let root = out.iter().find(|e| e.name == "bodygroups").unwrap();
        assert_eq!(root.children.len(), 2);
        // two states: one checkable option sending the other state (current 0 -> 1)
        assert_eq!(root.children[0].label, "Hat");
        assert_eq!(root.children[0].name, "bodygroups 0 1");
        assert_eq!(root.children[0].checked, Some(false));
        assert!(root.children[0].children.is_empty());
        // three states: a submenu of submodel file names, "Model #2" for the unnamed one
        let arms = &root.children[1];
        assert_eq!(arms.label, "Arms");
        let labels: Vec<&str> = arms.children.iter().map(|c| c.label.as_str()).collect();
        assert_eq!(labels, ["Arms normal", "Model #2", "Arms long"]);
        assert_eq!(arms.children[1].checked, Some(true));
        assert_eq!(arms.children[2].name, "bodygroups 1 2");
    }
}
