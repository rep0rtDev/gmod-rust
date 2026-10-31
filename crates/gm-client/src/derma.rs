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
/// `DermaDefault`: Tahoma 13 weight 500 (Roboto when the system font is missing).
pub const FONT: &str = "DermaDefault";
pub const FONT_BOLD: &str = "DermaDefaultBold";
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
pub const TEXTBOX: Piece = border(0, 150, 127, 21, 4);
pub const TAB_CONTROL: Piece = border(0, 256, 127, 127, 8);
pub const TABT_ACTIVE: Piece = border(0, 384, 63, 31, 8);
pub const TABT_INACTIVE: Piece = border(128, 384, 63, 31, 8);
pub const SELECTION: Piece = border(384, 32, 31, 31, 4);
pub const TREE_PLUS: Piece = normal(448, 96, 15, 15);
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
fn text(
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

fn image(items: &mut Vec<UiItem>, name: &str, x: f32, y: f32, w: f32, h: f32) {
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

fn inside((cx, cy): (f32, f32), x: f32, y: f32, w: f32, h: f32) -> bool {
    cx >= x && cx < x + w && cy >= y && cy < y + h
}

/// One `settings/spawnlist_default/NNN-name.txt` (or an addon list the server sent).
pub struct SpawnList {
    pub name: String,
    pub icon: String,
    pub models: Vec<String>,
    pub addon: bool,
}

/// The client's default spawnlists in id order.
pub fn load_spawnlists(mount: &Mount) -> Vec<SpawnList> {
    let mut out = Vec::new();
    for path in mount.find("settings/spawnlist_default/*.txt") {
        let Some(doc) = mount
            .read_string(&path)
            .ok()
            .and_then(|t| keyvalues::parse(&t).ok())
        else {
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
                name,
                icon,
                models,
                addon: false,
            },
        ));
    }
    out.sort_by_key(|(id, _)| *id);
    out.into_iter().map(|(_, l)| l).collect()
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

/// What the player has clicked around in the menu.
#[derive(Default)]
pub struct MenuState {
    pub creation_tab: usize,
    pub tool_tab: usize,
    /// Selected tree node: an index into the spawnlists.
    pub node: usize,
    pub browse_open: bool,
    /// Content scroll in px (a row of icons per wheel notch).
    pub scroll: f32,
    pub collapsed: [bool; 4],
}

#[derive(Clone, Debug, PartialEq)]
pub enum Action {
    CreationTab(usize),
    ToolTab(usize),
    Node(usize),
    Browse,
    Spawn(String),
    Category(usize),
    Tool(&'static str),
    /// A `properties.Add` entry of the right-click menu, by its name.
    Property(String),
    Nothing,
}

pub struct Hit {
    x: f32,
    y: f32,
    w: f32,
    h: f32,
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
    pub bold: &'a FontMetrics,
    pub lists: &'a [SpawnList],
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
    out
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
    nine(&mut out.items, &TEXTBOX, x, y, TREE_W, 20.0, WHITE);
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
    let (ty, th) = (y + 24.0, h - 24.0);
    nine(&mut out.items, &TREE, x, ty, TREE_W, th, WHITE);
    let mut rows: Vec<NodeRow> = ctx
        .lists
        .iter()
        .enumerate()
        .map(|(i, l)| NodeRow {
            label: l.name.clone(),
            icon: l.icon.clone(),
            level: if l.addon { 1.0 } else { 0.0 },
            selected: st.node == i,
            expander: None,
            action: Action::Node(i),
        })
        .collect();
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
        let label = label_of(&s.name);
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

/// The DForm's height for a schema: header 20 + each item's 10 px top padding and height.
pub fn controls_height(schema: &[gm_server::ToolSetting]) -> f32 {
    20.0 + schema
        .iter()
        .map(|s| {
            10.0 + match s.kind {
                gm_server::ToolSettingKind::Number => 32.0,
                gm_server::ToolSettingKind::Bool => 16.0,
                gm_server::ToolSettingKind::Text => 20.0,
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
