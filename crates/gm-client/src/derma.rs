//! Derma default skin and the sandbox spawn menu.
//!
//! Numbers come from the installed client's data: `lua/skins/default.lua` (colours and the GWEN
//! atlas pieces), `lua/vgui/dpropertysheet.lua` / `dtab.lua` / `dtree_node.lua` (tab and node
//! sizes) and `gamemodes/sandbox/gamemode/spawnmenu/*.lua` (dividers and their minimums); see
//! docs/spec/ui.md. Immediate mode: [`build`] lays the whole menu out from [`MenuState`] every
//! frame and returns the draw list together with the clickable regions, so hit testing and
//! drawing share one geometry.

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
