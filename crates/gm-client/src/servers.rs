//! The main menu's "Find Multiplayer Game" page (`template/servers.html`, `Servers.css`,
//! `control.Servers.js`): the options column (Internet / Favorites / History / Local
//! Network), the gamemode tiles, the server list and the "find servers at address" box.
//! There is no master server: Internet / LAN stay empty, Favorites and History are files
//! under `~/.gmod-rust/cfg`, and an address is probed with our own protocol's handshake.

use std::collections::BTreeMap;
use std::path::PathBuf;
use std::sync::mpsc;
use std::time::{Duration, Instant};

use gm_render::ui::{FontMetrics, UiItem};

use crate::derma::{image, inside, rgba, text, Action, Ctx, Hit, Layout};
use crate::settings::home_dir;

pub const F20B: &str = "Menu20Bold";
pub const F13B: &str = "Menu13Bold";

const WHITE: [f32; 4] = rgba(255, 255, 255, 255);
const BLACK: [f32; 4] = rgba(0, 0, 0, 255);
const TILE: [f32; 4] = rgba(239, 239, 239, 255);
const TILE_HOVER: [f32; 4] = rgba(250, 240, 150, 255);
const STATS: [f32; 4] = rgba(85, 85, 85, 255);
const SUB: [f32; 4] = rgba(136, 136, 136, 255);
const HEADER_BG: [f32; 4] = rgba(35, 135, 237, 255);
const ROW: [f32; 4] = rgba(231, 240, 255, 255);
const ROW_ODD: [f32; 4] = rgba(241, 246, 255, 255);
const ROW_HOVER: [f32; 4] = rgba(183, 221, 253, 255);
const ROW_TEXT: [f32; 4] = rgba(68, 68, 68, 255);
const EMPTY_TEXT: [f32; 4] = rgba(119, 119, 119, 255);
const LINK: [f32; 4] = rgba(255, 255, 255, 255);
const LINK_HOVER: [f32; 4] = rgba(255, 255, 170, 255);
const LINK_ACTIVE: [f32; 4] = rgba(255, 255, 85, 255);
const INPUT_BORDER: [f32; 4] = rgba(170, 170, 170, 255);
const BTN: [f32; 4] = rgba(0, 110, 204, 255);
const BTN_HOVER: [f32; 4] = rgba(34, 153, 230, 255);
const PLACEHOLDER: [f32; 4] = rgba(117, 117, 117, 255);

pub const TYPES: [(&str, &str); 4] = [
    ("internet", "servers_internet"),
    ("favorite", "servers_favorites"),
    ("history", "servers_history"),
    ("lan", "servers_local"),
];

#[derive(Clone, Debug, PartialEq)]
pub enum SvAction {
    Type(&'static str),
    Refresh,
    Gamemode(Option<String>),
    FindText,
    Find,
    Select(String),
    Join(String),
    Fav(String),
}

#[derive(Clone, Debug, PartialEq)]
pub struct ServerEntry {
    pub address: String,
    pub name: String,
    pub map: String,
    pub players: u32,
    pub maxplayers: u32,
    pub ping: u32,
    pub gamemode: String,
    pub last_played: u64,
}

impl ServerEntry {
    fn to_line(&self) -> String {
        format!(
            "{}\t{}\t{}\t{}\t{}\t{}\t{}\t{}",
            self.address,
            self.name,
            self.map,
            self.players,
            self.maxplayers,
            self.ping,
            self.gamemode,
            self.last_played
        )
    }

    fn from_line(line: &str) -> Option<Self> {
        let f: Vec<&str> = line.split('\t').collect();
        if f.len() < 8 {
            return None;
        }
        Some(ServerEntry {
            address: f[0].to_string(),
            name: f[1].to_string(),
            map: f[2].to_string(),
            players: f[3].parse().unwrap_or(0),
            maxplayers: f[4].parse().unwrap_or(0),
            ping: f[5].parse().unwrap_or(0),
            gamemode: f[6].to_string(),
            last_played: f[7].parse().unwrap_or(0),
        })
    }
}

fn list_path(kind: &str) -> PathBuf {
    home_dir().join("cfg").join(format!("servers_{kind}.txt"))
}

/// The master server from cfg/master.txt, if one is configured.
pub(crate) fn master_address() -> Option<String> {
    std::fs::read_to_string(home_dir().join("cfg").join("master.txt"))
        .ok()
        .map(|s| s.trim().to_string())
        .filter(|s| !s.is_empty())
}

pub fn load_list(kind: &str) -> Vec<ServerEntry> {
    std::fs::read_to_string(list_path(kind))
        .unwrap_or_default()
        .lines()
        .filter_map(ServerEntry::from_line)
        .collect()
}

pub fn save_list(kind: &str, list: &[ServerEntry]) {
    let p = list_path(kind);
    if let Some(dir) = p.parent() {
        let _ = std::fs::create_dir_all(dir);
    }
    let text: Vec<String> = list.iter().map(ServerEntry::to_line).collect();
    let _ = std::fs::write(p, text.join("\n"));
}

#[derive(Debug)]
pub struct ServersState {
    pub kind: &'static str,
    pub gamemode: Option<String>,
    pub find_text: String,
    pub focus: bool,
    pub found: Vec<ServerEntry>,
    pub favorites: Vec<ServerEntry>,
    pub history: Vec<ServerEntry>,
    pub selected: Option<String>,
    pub internet: Vec<ServerEntry>,
    pub lan: Vec<ServerEntry>,
    /// `cfg/master.txt`: the master server the Internet list is asked from.
    pub master: Option<String>,
    pub probing: Option<(mpsc::Receiver<Option<ServerEntry>>, Instant)>,
    pub refreshing: Option<(&'static str, mpsc::Receiver<Vec<ServerEntry>>, Instant)>,
    pub status: String,
}

impl ServersState {
    pub fn new() -> Self {
        ServersState {
            kind: "internet",
            gamemode: None,
            find_text: String::new(),
            focus: false,
            found: Vec::new(),
            favorites: load_list("favorites"),
            history: load_list("history"),
            selected: None,
            internet: Vec::new(),
            lan: Vec::new(),
            master: master_address(),
            probing: None,
            refreshing: None,
            status: String::new(),
        }
    }

    /// `Refresh()`: Internet asks the master for its list, Local Network broadcasts a
    /// `Query`; both on a thread.
    pub fn refresh(&mut self) {
        match self.kind {
            "favorite" => self.favorites = load_list("favorites"),
            "history" => self.history = load_list("history"),
            kind @ ("internet" | "lan") => {
                if self.refreshing.is_some() {
                    return;
                }
                let master = self.master.clone();
                if kind == "internet" && master.is_none() {
                    self.status =
                        "No master server set: put its address in cfg/master.txt".to_string();
                    return;
                }
                let (tx, rx) = mpsc::channel();
                std::thread::spawn(move || {
                    let list = if kind == "internet" {
                        master_list(&master.unwrap_or_default())
                    } else {
                        lan_scan()
                    };
                    let _ = tx.send(list);
                });
                self.status.clear();
                self.refreshing = Some((kind, rx, Instant::now()));
            }
            _ => {}
        }
    }

    pub fn stop_refresh(&mut self) {
        self.refreshing = None;
    }

    /// The servers of the current type.
    pub fn servers(&self) -> &[ServerEntry] {
        match self.kind {
            "favorite" => &self.favorites,
            "history" => &self.history,
            "internet" => &self.internet,
            "lan" => &self.lan,
            _ => &[],
        }
    }

    /// Gamemodes of the current list with players / server counts.
    pub fn gamemodes(&self) -> Vec<(String, u32, u32)> {
        let mut m: BTreeMap<String, (u32, u32)> = BTreeMap::new();
        for s in self.servers() {
            let e = m.entry(s.gamemode.clone()).or_default();
            e.0 += s.players;
            e.1 += 1;
        }
        m.into_iter().map(|(k, (p, n))| (k, p, n)).collect()
    }

    pub fn toggle_favorite(&mut self, address: &str) {
        if let Some(i) = self.favorites.iter().position(|s| s.address == address) {
            self.favorites.remove(i);
        } else if let Some(s) = self
            .found
            .iter()
            .chain(self.history.iter())
            .chain(self.internet.iter())
            .chain(self.lan.iter())
            .find(|s| s.address == address)
            .cloned()
        {
            self.favorites.push(s);
        }
        save_list("favorites", &self.favorites);
    }

    pub fn is_favorite(&self, address: &str) -> bool {
        self.favorites.iter().any(|s| s.address == address)
    }

    /// Joining puts the server at the top of History.
    pub fn record_join(&mut self, address: &str) {
        let now = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map_or(0, |d| d.as_secs());
        let mut entry = self
            .found
            .iter()
            .chain(self.favorites.iter())
            .chain(self.history.iter())
            .chain(self.internet.iter())
            .chain(self.lan.iter())
            .find(|s| s.address == address)
            .cloned()
            .unwrap_or_else(|| ServerEntry {
                address: address.to_string(),
                name: address.to_string(),
                map: String::new(),
                players: 0,
                maxplayers: 0,
                ping: 0,
                gamemode: "sandbox".to_string(),
                last_played: 0,
            });
        entry.last_played = now;
        self.history.retain(|s| s.address != address);
        self.history.insert(0, entry);
        self.history.truncate(50);
        save_list("history", &self.history);
    }

    /// `FindServersAtAddress`: the handshake of our protocol on a thread, 2 s budget; the
    /// Welcome names the map, the first snapshot the players.
    pub fn find(&mut self) {
        let addr = self.find_text.trim().to_string();
        if addr.is_empty() || self.probing.is_some() {
            return;
        }
        let addr = if addr.contains(':') {
            addr
        } else {
            format!("{addr}:27015")
        };
        let (tx, rx) = mpsc::channel();
        self.status = format!("Searching {addr}...");
        std::thread::spawn(move || {
            let result = probe(&addr);
            let _ = tx.send(result);
        });
        self.probing = Some((rx, Instant::now()));
    }

    /// Picks up a finished probe.
    pub fn poll(&mut self) {
        if let Some((kind, rx, started)) = &self.refreshing {
            match rx.try_recv() {
                Ok(list) => {
                    if *kind == "internet" {
                        self.internet = list;
                    } else {
                        self.lan = list;
                    }
                    self.refreshing = None;
                }
                Err(mpsc::TryRecvError::Empty) if started.elapsed() < Duration::from_secs(8) => {}
                Err(_) => self.refreshing = None,
            }
        }
        let Some((rx, started)) = &self.probing else {
            return;
        };
        match rx.try_recv() {
            Ok(Some(entry)) => {
                self.status.clear();
                self.found.retain(|s| s.address != entry.address);
                self.found.insert(0, entry);
                self.probing = None;
            }
            Ok(None) => {
                self.status = format!(
                    "The server at address {} failed to respond",
                    self.find_text.trim()
                );
                self.probing = None;
            }
            Err(mpsc::TryRecvError::Empty) if started.elapsed() > Duration::from_secs(4) => {
                self.status = format!(
                    "The server at address {} failed to respond",
                    self.find_text.trim()
                );
                self.probing = None;
            }
            Err(mpsc::TryRecvError::Disconnected) => self.probing = None,
            Err(_) => {}
        }
    }
}

const QUERY_TIMEOUT: Duration = Duration::from_secs(2);

fn open_socket(timeout: Duration) -> Option<std::net::UdpSocket> {
    let sock = std::net::UdpSocket::bind("0.0.0.0:0").ok()?;
    sock.set_read_timeout(Some(timeout)).ok()?;
    Some(sock)
}

fn entry_from_info(addr: &str, info: &gm_net::ServerInfo, ping: u32) -> ServerEntry {
    ServerEntry {
        address: addr.to_string(),
        name: info.name.clone(),
        map: info.map.clone(),
        players: u32::from(info.players),
        maxplayers: u32::from(info.max_players),
        ping,
        gamemode: if info.game.is_empty() {
            "sandbox".to_string()
        } else {
            info.game.clone()
        },
        last_played: 0,
    }
}

/// A direct `Query` to a game server: its `Info` and the round trip as the ping.
pub(crate) fn probe(addr: &str) -> Option<ServerEntry> {
    let target: std::net::SocketAddr = addr.parse().ok()?;
    let sock = open_socket(QUERY_TIMEOUT)?;
    let nonce = std::process::id() ^ (Instant::now().elapsed().as_nanos() as u32);
    let start = Instant::now();
    sock.send_to(&gm_net::MasterMsg::Query { nonce }.encode(), target)
        .ok()?;
    let mut buf = [0u8; 2048];
    while start.elapsed() < QUERY_TIMEOUT {
        let Ok((n, from)) = sock.recv_from(&mut buf) else {
            break;
        };
        if from != target {
            continue;
        }
        if let Ok(gm_net::MasterMsg::Info { nonce: got, info }) = gm_net::MasterMsg::read(&buf[..n])
        {
            if got == nonce {
                let ping = start.elapsed().as_millis() as u32;
                return Some(entry_from_info(addr, &info, ping));
            }
        }
    }
    None
}

/// The master's list (`List` pages until `total` is reached), each server then pinged.
pub(crate) fn master_list(master: &str) -> Vec<ServerEntry> {
    let Ok(target) = master.parse::<std::net::SocketAddr>() else {
        return Vec::new();
    };
    let Some(sock) = open_socket(QUERY_TIMEOUT) else {
        return Vec::new();
    };
    let mut entries: Vec<gm_net::ServerEntry> = Vec::new();
    let mut offset = 0u32;
    let mut buf = vec![0u8; 65536];
    loop {
        if sock
            .send_to(&gm_net::MasterMsg::List { offset }.encode(), target)
            .is_err()
        {
            break;
        }
        let Ok((n, _)) = sock.recv_from(&mut buf) else {
            break;
        };
        let Ok(gm_net::MasterMsg::Servers {
            total,
            offset: got,
            entries: page,
        }) = gm_net::MasterMsg::read(&buf[..n])
        else {
            break;
        };
        if got != offset || page.is_empty() {
            break;
        }
        offset += page.len() as u32;
        entries.extend(page);
        if offset >= total {
            break;
        }
    }
    entries
        .iter()
        .map(|e| probe(&e.addr).unwrap_or_else(|| entry_from_info(&e.addr, &e.info, 0)))
        .collect()
}

/// Local Network: a broadcast `Query` on the default port, every `Info` within a second.
pub(crate) fn lan_scan() -> Vec<ServerEntry> {
    let Some(sock) = open_socket(Duration::from_millis(250)) else {
        return Vec::new();
    };
    let _ = sock.set_broadcast(true);
    let nonce = std::process::id();
    let start = Instant::now();
    let q = gm_net::MasterMsg::Query { nonce }.encode();
    let _ = sock.send_to(&q, "255.255.255.255:27015");
    let _ = sock.send_to(&q, "127.0.0.1:27015");
    let mut out: Vec<ServerEntry> = Vec::new();
    let mut buf = [0u8; 2048];
    while start.elapsed() < Duration::from_secs(1) {
        let Ok((n, from)) = sock.recv_from(&mut buf) else {
            continue;
        };
        if let Ok(gm_net::MasterMsg::Info { nonce: got, info }) = gm_net::MasterMsg::read(&buf[..n])
        {
            if got != nonce {
                continue;
            }
            let addr = format!("{}:{}", from.ip(), info.port);
            if !out.iter().any(|s| s.address == addr) {
                let ping = start.elapsed().as_millis() as u32;
                out.push(entry_from_info(&addr, &info, ping));
            }
        }
    }
    out
}

pub struct Fonts<'a> {
    pub f11: &'a FontMetrics,
    pub f12: &'a FontMetrics,
    pub f13: &'a FontMetrics,
    pub f13b: &'a FontMetrics,
    pub f20b: &'a FontMetrics,
    pub f32: &'a FontMetrics,
    pub opt: &'a FontMetrics,
    pub opt_font: &'a str,
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

#[allow(clippy::too_many_arguments)]
fn shadow_text(
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

/// A server row (22 px pitch): name (flex), map, players, ping.
#[allow(clippy::too_many_arguments)]
fn server_row(
    out: &mut Layout,
    ctx: &Ctx,
    fonts: &Fonts,
    st: &ServersState,
    s: &ServerEntry,
    x: f32,
    y: f32,
    w: f32,
    odd: bool,
    wide: bool,
) {
    let hovered = inside(ctx.cursor, x, y, w, 22.0);
    let selected = st.selected.as_deref() == Some(s.address.as_str());
    let bg = if hovered || selected {
        ROW_HOVER
    } else if odd {
        ROW_ODD
    } else {
        ROW
    };
    rect(&mut out.items, x, y, w, 22.0, bg);
    let (map_w, players_w, ping_w) = (if wide { 130.0 } else { 90.0 }, 48.0, 32.0);
    let name_w = w - map_w - players_w - ping_w - 8.0 - 20.0;
    // favourite star
    let fav = st.is_favorite(&s.address);
    let star = if fav {
        "html/img/favourite.png"
    } else {
        "html/img/notfavourite.png"
    };
    if (ctx.has_image)(star) {
        image(&mut out.items, star, x + 4.0, y + 3.0, 16.0, 16.0);
    }
    out.hits.push(Hit {
        x: x + 4.0,
        y: y + 3.0,
        w: 16.0,
        h: 16.0,
        action: Action::Servers(SvAction::Fav(s.address.clone())),
    });
    let (nf, nfm) = if wide {
        (F13B, fonts.f13b)
    } else {
        ("Menu13", fonts.f13)
    };
    let mut name = s.name.clone();
    while nfm.text_width(&name) > name_w && name.len() > 3 {
        name.pop();
    }
    text(&mut out.items, nf, nfm, x + 24.0, y, 22.0, &name, ROW_TEXT);
    let cx = x + 24.0 + name_w;
    let small = if wide { fonts.f13 } else { fonts.f11 };
    let sf = if wide { "Menu13" } else { "Menu11" };
    let empty = if s.players == 0 { EMPTY_TEXT } else { ROW_TEXT };
    text(&mut out.items, sf, small, cx, y, 22.0, &s.map, ROW_TEXT);
    let p = format!("{} / {}", s.players, s.maxplayers);
    let pw = small.text_width(&p);
    text(
        &mut out.items,
        sf,
        small,
        cx + map_w + ((players_w - pw) / 2.0).round(),
        y,
        22.0,
        &p,
        empty,
    );
    let ping = s.ping.to_string();
    let pingw = small.text_width(&ping);
    text(
        &mut out.items,
        sf,
        small,
        cx + map_w + players_w + ((ping_w - pingw) / 2.0).round(),
        y,
        22.0,
        &ping,
        ROW_TEXT,
    );
    out.hits.push(Hit {
        x: x + 24.0,
        y,
        w: w - 24.0,
        h: 22.0,
        action: Action::Servers(SvAction::Select(s.address.clone())),
    });
}

/// The page: options column at 86 / 52 like main.html, the inner page from 320 (250 at
/// <= 1280 wide) to 16 from the right.
pub fn page(
    out: &mut Layout,
    ctx: &Ctx,
    st: &ServersState,
    lang: &BTreeMap<String, String>,
    fonts: &Fonts,
) {
    let (w, h) = (ctx.w, ctx.h - 50.0);
    let wide = w >= 1280.0;
    let x = 86.0;
    let line = (fonts.opt.line_height + 1.0).round();
    let mut y = 52.0;
    // h2 Server List
    shadow_text(
        out,
        fonts.opt_font,
        fonts.opt,
        x,
        y,
        line,
        &say(lang, "server_list", "Server List"),
        WHITE,
    );
    y += line;
    let small_line = (fonts.f11.line_height + 1.0).round();
    let count = st.servers().len();
    shadow_text(
        out,
        "Menu11",
        fonts.f11,
        x,
        y,
        small_line,
        &format!(
            "{count} {}",
            say(lang, "servers_servercount", "server(s) total")
        ),
        WHITE,
    );
    y += small_line;
    shadow_text(
        out,
        "Menu11",
        fonts.f11,
        x,
        y,
        small_line,
        &format!(
            "{} {}",
            st.gamemodes().len(),
            say(lang, "servers_gmcount", "gamemode(s) total")
        ),
        WHITE,
    );
    y += small_line + line;
    for (kind, key) in TYPES {
        let label = say(lang, key, kind);
        let tw = fonts.opt.text_width(&label);
        let hovered = inside(ctx.cursor, x, y, tw, line);
        let c = if st.kind == kind {
            LINK_ACTIVE
        } else if hovered {
            LINK_HOVER
        } else {
            LINK
        };
        shadow_text(out, fonts.opt_font, fonts.opt, x, y, line, &label, c);
        out.hits.push(Hit {
            x,
            y,
            w: tw,
            h: line,
            action: Action::Servers(SvAction::Type(kind)),
        });
        y += line;
    }
    y += line;
    {
        let label = if st.refreshing.is_some() {
            say(lang, "servers_stoprefresh", "Stop Refreshing")
        } else {
            say(lang, "servers_refresh", "Refresh Listings")
        };
        let tw = fonts.opt.text_width(&label);
        let hovered = inside(ctx.cursor, x, y, tw, line);
        shadow_text(
            out,
            fonts.opt_font,
            fonts.opt,
            x,
            y,
            line,
            &label,
            if hovered { LINK_HOVER } else { LINK },
        );
        out.hits.push(Hit {
            x,
            y,
            w: tw,
            h: line,
            action: Action::Servers(SvAction::Refresh),
        });
        y += line * 2.0;
        let label = say(lang, "legacy_browser", "Legacy Browser");
        shadow_text(out, fonts.opt_font, fonts.opt, x, y, line, &label, LINK);
    }

    // innerpage
    let ix = if w > 1280.0 { 320.0 } else { 250.0 };
    let (px, py, pw) = (ix + 16.0, 16.0, w - ix - 32.0);
    let title_h = 40.0;
    match &st.gamemode {
        None => {
            // H1 Gamemodes + small
            let title = say(lang, "servers_gamemodes", "Gamemodes");
            shadow_text(out, "Menu32", fonts.f32, px, py, title_h, &title, WHITE);
            let tw = fonts.f32.text_width(&title);
            text(
                &mut out.items,
                "Menu13",
                fonts.f13,
                px + tw + 8.0,
                py + 12.0,
                24.0,
                &say(
                    lang,
                    "servers_gamemodes.subtitle",
                    "Pick a gamemode from the list below",
                ),
                SUB,
            );
            let gy = py + title_h + 10.0;
            let list_h = if st.kind == "favorite" {
                (h - 16.0 - gy) * 0.45
            } else {
                h - 16.0 - gy
            };
            rect(&mut out.items, px, gy, pw, list_h, WHITE);
            let mut ty = gy + 10.0;
            let gms = st.gamemodes();
            if gms.is_empty() {
                let msg = if st.refreshing.is_some() {
                    "Refreshing...".to_string()
                } else if !st.status.is_empty() && st.kind != "favorite" {
                    st.status.clone()
                } else {
                    match st.kind {
                        "internet" => "No servers listed. Refresh Listings asks the master server."
                            .to_string(),
                        "lan" => {
                            "No servers found on the local network. Refresh Listings looks again."
                                .to_string()
                        }
                        _ => "No servers in this list.".to_string(),
                    }
                };
                text(
                    &mut out.items,
                    "Menu13",
                    fonts.f13,
                    px + 10.0,
                    ty,
                    24.0,
                    &msg,
                    SUB,
                );
            }
            for (gm, players, servers) in gms {
                let hovered = inside(ctx.cursor, px + 10.0, ty, pw - 25.0, 44.0);
                rect(
                    &mut out.items,
                    px + 10.0,
                    ty,
                    pw - 25.0,
                    44.0,
                    if hovered { TILE_HOVER } else { TILE },
                );
                let icon = format!("gamemodes/{gm}/icon24.png");
                if (ctx.has_image)(&icon) {
                    image(&mut out.items, &icon, px + 15.0, ty + 9.0, 24.0, 24.0);
                }
                let name = if gm == "sandbox" {
                    "Sandbox".to_string()
                } else {
                    gm.clone()
                };
                text(
                    &mut out.items,
                    F20B,
                    fonts.f20b,
                    px + 50.0,
                    ty + 3.0,
                    26.0,
                    &name,
                    BLACK,
                );
                let stats = format!(
                    "{players} {} {servers} {}",
                    say(
                        lang,
                        if players == 1 {
                            "servers_player_on"
                        } else {
                            "servers_players_on"
                        },
                        "players on"
                    ),
                    say(
                        lang,
                        if servers == 1 {
                            "servers_server"
                        } else {
                            "servers_servers"
                        },
                        "servers"
                    )
                );
                text(
                    &mut out.items,
                    "Menu11",
                    fonts.f11,
                    px + 53.0,
                    ty + 27.0,
                    14.0,
                    &stats,
                    STATS,
                );
                out.hits.push(Hit {
                    x: px + 10.0,
                    y: ty,
                    w: pw - 25.0,
                    h: 44.0,
                    action: Action::Servers(SvAction::Gamemode(Some(gm.clone()))),
                });
                ty += 49.0;
            }
            if st.kind == "favorite" {
                // .add_fav_server: header, input + button, found servers
                let fy = gy + list_h + 10.0;
                rect(&mut out.items, px, fy, pw, h - 16.0 - fy, WHITE);
                text(
                    &mut out.items,
                    F13B,
                    fonts.f13b,
                    px + 10.0,
                    fy + 6.0,
                    20.0,
                    &say(
                        lang,
                        "servers_find_title",
                        "Search for servers by address to add to favorites",
                    ),
                    BLACK,
                );
                let (bx, by) = (px + 10.0, fy + 30.0);
                let btn_label = say(lang, "servers_find", "Find servers at given address");
                let btn_w = fonts.f12.text_width(&btn_label) + 24.0;
                let in_w = pw - 20.0 - btn_w - 6.0;
                rect(&mut out.items, bx, by, in_w, 24.0, WHITE);
                frame(&mut out.items, bx, by, in_w, 24.0, INPUT_BORDER);
                if st.find_text.is_empty() && !st.focus {
                    text(
                        &mut out.items,
                        "MenuNav",
                        fonts.f12,
                        bx + 5.0,
                        by,
                        24.0,
                        "127.0.0.1:27015",
                        PLACEHOLDER,
                    );
                } else {
                    let shown = if st.focus {
                        format!("{}|", st.find_text)
                    } else {
                        st.find_text.clone()
                    };
                    text(
                        &mut out.items,
                        "MenuNav",
                        fonts.f12,
                        bx + 5.0,
                        by,
                        24.0,
                        &shown,
                        BLACK,
                    );
                }
                out.hits.push(Hit {
                    x: bx,
                    y: by,
                    w: in_w,
                    h: 24.0,
                    action: Action::Servers(SvAction::FindText),
                });
                let bbx = bx + in_w + 6.0;
                let hovered = inside(ctx.cursor, bbx, by, btn_w, 24.0);
                rect(
                    &mut out.items,
                    bbx,
                    by,
                    btn_w,
                    24.0,
                    if hovered { BTN_HOVER } else { BTN },
                );
                text(
                    &mut out.items,
                    "MenuNav",
                    fonts.f12,
                    bbx + 12.0,
                    by,
                    24.0,
                    &btn_label,
                    WHITE,
                );
                out.hits.push(Hit {
                    x: bbx,
                    y: by,
                    w: btn_w,
                    h: 24.0,
                    action: Action::Servers(SvAction::Find),
                });
                let mut ry = by + 30.0;
                if !st.status.is_empty() {
                    text(
                        &mut out.items,
                        "Menu11",
                        fonts.f11,
                        bx,
                        ry,
                        18.0,
                        &st.status,
                        STATS,
                    );
                    ry += 20.0;
                }
                for (i, s) in st.found.iter().enumerate() {
                    if ry + 22.0 > h - 16.0 {
                        break;
                    }
                    server_row(out, ctx, fonts, st, s, bx, ry, pw - 20.0, i % 2 == 1, wide);
                    ry += 22.0;
                }
            }
        }
        Some(gm) => {
            let name = if gm == "sandbox" {
                "Sandbox".to_string()
            } else {
                gm.clone()
            };
            shadow_text(out, "Menu32", fonts.f32, px, py, title_h, &name, WHITE);
            let tw = fonts.f32.text_width(&name);
            text(
                &mut out.items,
                "Menu13",
                fonts.f13,
                px + tw + 8.0,
                py + 12.0,
                24.0,
                &say(lang, "join_a_server", "Join a server"),
                SUB,
            );
            // return link
            let back = say(lang, "return_to_gamemodes", "Return to gamemodes");
            let bw = fonts.f12.text_width(&back) + 20.0;
            let (lx, ly) = (px + pw - bw, py + title_h + 5.0);
            if (ctx.has_image)("html/img/bg_arrow_left.png") {
                image(
                    &mut out.items,
                    "html/img/bg_arrow_left.png",
                    lx,
                    ly + 4.0,
                    16.0,
                    16.0,
                );
            }
            shadow_text(out, "MenuNav", fonts.f12, lx + 20.0, ly, 24.0, &back, WHITE);
            out.hits.push(Hit {
                x: lx,
                y: ly,
                w: bw,
                h: 24.0,
                action: Action::Servers(SvAction::Gamemode(None)),
            });
            // serverlist
            let (ly, lh) = (py + 90.0, h - 16.0 - py - 90.0);
            rect(&mut out.items, px, ly, pw, lh, WHITE);
            let (hx, hy, hw) = (px + 8.0, ly + 8.0, pw - 16.0);
            rect(&mut out.items, hx, hy, hw, 20.0, HEADER_BG);
            let (map_w, players_w, ping_w) = (if wide { 130.0 } else { 90.0 }, 48.0, 32.0);
            let name_w = hw - map_w - players_w - ping_w - 8.0 - 20.0;
            text(
                &mut out.items,
                "Menu11",
                fonts.f11,
                hx + 24.0,
                hy,
                20.0,
                &say(lang, "server_name_header", "Server Name"),
                WHITE,
            );
            text(
                &mut out.items,
                "Menu11",
                fonts.f11,
                hx + 24.0 + name_w,
                hy,
                20.0,
                &say(lang, "server_mapname", "Map Name"),
                WHITE,
            );
            text(
                &mut out.items,
                "Menu11",
                fonts.f11,
                hx + 24.0 + name_w + map_w,
                hy,
                20.0,
                &say(lang, "server_players", "Players"),
                WHITE,
            );
            text(
                &mut out.items,
                "Menu11",
                fonts.f11,
                hx + 24.0 + name_w + map_w + players_w,
                hy,
                20.0,
                &say(lang, "server_ping", "Ping"),
                WHITE,
            );
            let mut ry = hy + 24.0;
            let rows: Vec<&ServerEntry> =
                st.servers().iter().filter(|s| s.gamemode == *gm).collect();
            for (i, s) in rows.iter().enumerate() {
                if ry + 22.0 > ly + lh - 8.0 {
                    break;
                }
                server_row(out, ctx, fonts, st, s, hx, ry, hw, i % 2 == 1, wide);
                ry += 22.0;
            }
            if let Some(sel) = st
                .selected
                .as_ref()
                .filter(|a| rows.iter().any(|s| s.address == **a))
            {
                let label = say(lang, "servers_join_server", "Join Server");
                let bw = fonts.f12.text_width(&label) + 30.0;
                let (bx, by) = (px + pw - 8.0 - bw, ly + lh - 8.0 - 28.0);
                let hovered = inside(ctx.cursor, bx, by, bw, 28.0);
                rect(
                    &mut out.items,
                    bx,
                    by,
                    bw,
                    28.0,
                    if hovered { BTN_HOVER } else { BTN },
                );
                text(
                    &mut out.items,
                    "MenuNav",
                    fonts.f12,
                    bx + 15.0,
                    by,
                    28.0,
                    &label,
                    WHITE,
                );
                out.hits.push(Hit {
                    x: bx,
                    y: by,
                    w: bw,
                    h: 28.0,
                    action: Action::Servers(SvAction::Join(sel.clone())),
                });
            }
        }
    }
}
