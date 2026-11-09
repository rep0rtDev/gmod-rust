# `ui` - gap list: GMod client HUD/UI vs the Rust rewrite

Compared against `rep0rtDev/gmod-rust` `main` (shallow clone of 2026-11-09): `crates/gm-client/src/main.rs` (HUD, weapon
selector, menu, fades, notifications), `crates/gm-render/src/ui.rs` (UI items, fonts, images), `docs/spec/ui.md`,
`docs/status.md`. Line numbers are from that checkout. Behaviour references (`ui.md` §n) point to the behaviour note.

Priority: **P0** obvious to any player in the first minute, **P1** noticeable in normal play, **P2** subtle / needs a
side-by-side. Status: `match` / `partial` / `missing` / `wrong`.

## A. Framework, scaling, fonts, scheme

| # | behaviour (ui.md) | Rust today | status | what is missing / wrong | prio |
|---|---|---|---|---|---|
| A1 | 640x480 layout scaled by `screenH/480` on both axes, `r`/`c` anchors against the real width (§1.2) | `main.rs:6529-6540` `s = h/480`, `w - 150*s` for `r150` | match | - | - |
| A2 | Scheme font `tall` is absolute pixels; only `yres` fonts step (9->13, 8->10, 12->14 at 600 lines); HalfLife2 glyph fonts are fixed 32/64/40/16 px at any resolution (§1.3, §2.2) | `main.rs:9314-9445` `register_ui_fonts`: every HUD font multiplied by `fs = height/480` (`HudNumbers 32*fs`, `WeaponIcons 64*fs`, `Crosshairs 40*fs`, `HudSelectionNumbers 11*fs`, `HudSelectionText 8*fs`, `TargetID 12*fs`, `ChatFont 12*fs`, `HudDefault 9*fs`) | wrong | At 1080p the game draws 32 px digits in a 81 px tall panel and 64 px weapon glyphs; Rust draws 72 px digits / 144 px glyphs. Must use fixed px, and the `yres` step (`HudDefault` 13, `HudSelectionText` 10, `ChatFont`/`TargetID` 14 at >= 600 lines). `GModNotify` (`max(12, ceil(9*h/480))`) is the only proportional one and is right (`main.rs:9345`). | P0 |
| A3 | `HudDefault` Verdana weight 700; `HudNumbersGlow` blur 4 / scanlines 2; `WeaponIconsSelected` blur 5; additive blending on all HL2 glyph fonts (§2.2) | `register_ui_fonts` registers plain rasterised fonts; no blur/glow or additive variants (`ui.rs:281` `raster_text` plain alpha) | missing | No glow pass behind digits/icons, no scanlines, no additive blend - the HL2 "CRT" look of the numbers is absent. | P1 |
| A4 | Font families: Verdana/Tahoma/Trebuchet/Arial from the OS, HalfLife2/HL2cross/csd from `resource/*.ttf` (§2.2) | `ui.rs:315` `font_file`: resource/fonts, then system dirs with the fontconfig substitutions, Roboto last | match | Roboto fallback renders noticeably different metrics when the OS font is absent (documented in `docs/spec/ui.md:40-45`). | P2 |
| A5 | `PaintBackgroundType 2` rounded HUD boxes from `vgui/hud/800corner1..4`, colour `BgColorHud 0 0 0 90` (§1.7, §2.1) | `main.rs:6534` `bg = 0 0 0 76`; panel is a flat `UiItem::Rect` (`main.rs:6540-6546`) | wrong | Corners are square and the box is lighter (76 vs 90 alpha). | P1 |
| A6 | HUD foreground `FgColorHud 255 220 0 220` for digits and labels (§2.1) | `main.rs:6533` `fg = 255 235 20 255` (`FgColor`) | wrong | Slightly wrong hue and fully opaque; the game's digits are 220/255 translucent yellow. | P1 |
| A7 | HUD element hiding by `GM:HUDShouldDraw(name)` and `cl_drawhud`, HIDEHUD bits (§1.4) | `main.rs:6517-6522` `hidden(name)` from script `HideHud` draw cmds | partial | No `cl_drawhud`, no server HIDEHUD bits (vehicle, dead, weapon selection, needsuit). | P2 |
| A8 | HUD init loads `HudLayout.res`, `hud_textures.txt`, `ClientScheme.res`, `HudAnimations.txt` (§1.1) | Values are hard-coded in `main.rs`; none of the files are parsed | partial | Fine for 1:1 of the stock files; custom-content servers/skins will not apply. | P2 |

## B. Health / suit / ammo / aux power

| # | behaviour | Rust today | status | missing / wrong | prio |
|---|---|---|---|---|---|
| B1 | `HudHealth` 16,432 102x36, label (8,20) `HudDefault`, digits (50,2) `HudNumbers` (§3.1) | `main.rs:6745-6760` panel at `16*s`, wide `102*s`, label `+8,+20`, digits `+50,+2` | match (geometry) | colours/corners per A5/A6 | - |
| B2 | Damage flash `HealthDamageTaken` (bright yellow 0.25 s, blur 3->0 over 2 s), low-health `HealthLow`/`HealthPulse` red pulse ~0.9 s period while < 20, `HealthIncreasedAbove20` recovery, `HealthOver999` panel 116 wide (§3.3-3.4) | none: static colour, `p.health.max(0)` (`main.rs:6749-6758`) | missing | No flash on hit, no red pulsing box below 20 HP, no growth for 4-digit health. The low-health pulse is one of the most visible HL2 HUD cues. | P0 |
| B3 | `HudSuit` 140,432 102x36 (§4.1) | `main.rs:6755-6765` at `140*s` wide `102*s` | match | `docs/spec/ui.md:9` says 108 wide - doc error. | - |
| B4 | Suit shown at alpha 0 after `SuitPowerZero` (0.4 s fade), `SuitDamageTaken` flash, no red pulse (§4.2) | drawn only while `armor > 0` (`main.rs:6755`) - instant disappear | partial | missing 0.4 s fade-out and the flash on armour loss. | P2 |
| B5 | `HudAmmo` `r150` 136x36, digits (40,2), reserve (102,16) `HudNumbersSmall` (§5.1) | `main.rs:7025-7045` `ax = w-150*s`, wide 136, digits `+40,+2`, reserve `+102,+16` | match | - | - |
| B6 | Clip/no-clip logic: clip<0 -> show reserve only, hide small number; hidden when weapon has no ammo at all (§5.3) | `main.rs:7025-7040` `clip = if p.clip>=0 {clip} else {reserve}`; small number only when `clip >= 0`; panel when `clip>=0 || reserve>0` | match | - | - |
| B7 | `WeaponChanged` yellow background flash 250 220 0 80 -> 1.1 s; `AmmoDecreased` blur 7 flash; `AmmoIncreased` (§5.3) | none | missing | Every shot in HL2 makes the digits glow briefly; absent. | P1 |
| B8 | `HudSecondaryAmmo`: separate panel at `r76`,432 60x36 (compiled/HL2 defaults - GMod's res has no block), label (8,22), digits (36,2), no text label (an `ammo2` icon glyph), alpha 0 unless the weapon has secondary ammo (§5.2, §5.3) | `main.rs:7047-7060`: an "ALT" labelled 60-wide panel at `ax - 60*s - 6*s` (left of the ammo panel) | wrong | position (game: right edge 16 px from the screen edge, overlapping the ammo panel's right end - the ammo panel is only 136 wide at `r150` so they touch), the `ALT` text, the icon glyph; effective GMod values are medium confidence (see blockers). | P2 |
| B9 | Aux power panel 16,396 102x26, 10 chunks, SPRINT/FLASHLIGHT/OXYGEN labels, red below 25, hidden at 100 idle (§6) | not found in `main.rs` (no `HudSuitPower`, no sprint/aux drawing) | missing | Sprinting / flashlight / under-water in HL2 shows the AUX POWER bar; absent. | P1 |
| B10 | Poison indicator panel (§10) | missing | missing | rare (poison headcrab/antlion worker) | P2 |

## C. Crosshair and zoom

| # | behaviour | Rust today | status | missing / wrong | prio |
|---|---|---|---|---|---|
| C1 | Weapon `crosshair` glyph `Q` in `Crosshairs` (40 px fixed) colour `Normal 255 208 64 255`, centred (§7.2) | `main.rs:6558-6575` glyph `Q`, colour 255 208 64, centred with `text_width/2`, `line_height/2`; font 40*fs | partial | size scales with resolution (A2); glyph centring uses line height not glyph bounds (HalfLife2 `Q` is a 40 px glyph; SDK centres by `GetCharABCwide`/texture size). | P1 |
| C2 | Default white `crosshair_default` 24x24 sprite when the weapon has no glyph (`sprites/crosshairs` x0 y48) (§7.1) | none; every weapon draws `Q` | missing | SWEPs without a crosshair glyph (most Lua weapons) and the no-weapon case show the white dot/cross in the game. | P1 |
| C3 | Zoomed (FOV < 90): `zoom` glyph in white; `HudZoom` overlay `vgui/zoom`, circles 66/74, 0.4 s fade, alpha max 64 (§7.2, §7.5) | none (`zoom` only as FOV scale `main.rs:616`) | missing | Crossbow/`+zoom` look. | P1 |
| C4 | Not drawn when dead (unless in-eye spectating), in vehicle, `crosshair 0`, `HUDShouldDraw("CHudCrosshair") == false`, `SWEP:DoDrawCrosshair` (§7.1, §7.3) | `main.rs:6560-6565` `p.alive`, `!hidden("CHudCrosshair")`, loadout check | partial | no `crosshair` convar, no vehicle rule, no `DoDrawCrosshair`. | P2 |

## D. Weapon selection

| # | behaviour | Rust today | status | missing / wrong | prio |
|---|---|---|---|---|---|
| D1 | `BoxGap` 6 (§8.1) | `main.rs:3597` `gap = 8*s`; `docs/spec/ui.md:28` says 8 | wrong | 2 layout px too wide between every box (also the horizontal gap between buckets). | P2 |
| D2 | Boxes: small 32, large 112x80, y 16, numbers (4,4), name at y 64 (§8.1) | `main.rs:3597-3650` same values | match | - | - |
| D3 | Row centred: `x0 = (screenW - (112 + 5*(32+gap)))/2` (§8.3) | `main.rs:3614` `x = ((w - width)/2).round()` | match | - | - |
| D4 | Box colours `SelectionBoxBg 0 0 0 90`, selected `0 0 0 150`, empty bucket `0 0 0 70` with no number (§8.1, §8.3) | `main.rs:3598` single `box_bg = 0 0 0 80*fade`; selected/unselected/empty distinction absent | wrong | Highlighted weapon box is not darker; empty buckets are drawn like filled ones (or not at all). | P1 |
| D5 | Foreground `FgColorHud 255 220 0 220`, text `SelectionTextFg`, numbers `SelectionNumberFg 255 220 0 255`; glyph red when no ammo (§8.3) | `fg = 255 220 0 fade`, `dim = 0.5*fade` (`main.rs:3595-3596`) | partial | no red no-ammo glyph; `dim` for other buckets is an invention (game uses the same `FgColorHud` for all bucket numbers). | P2 |
| D6 | Open: `Alpha 255` over 0.1 s + `Player.WeaponSelectionOpen`; close: 0.1 s fade + `Player.WeaponSelectionClose`; timeout 0.5 s then `FadeOutWeaponSelectionMenu` (Linear start 0.5 dur 1.0) with the panel hidden at 0.75 s after timeout (§8.5-8.6) | `main.rs:3592-3594` `fade = 1 - (idle-0.5)/0.75` linear from 0.5 s to 1.25 s; hide at 1.25 s (`main.rs:4366-4370`); no open/close fade, no open/close sounds (`grep` shows only MoveSlot/Selected/Deny at `main.rs:3481,3510,3520,7714`) | partial | Fade curve differs (game: no change until 1.0 s after timeout, then 1.0 s linear fade cut off at 1.25 s -> the game's menu stays fully opaque for 1.0 s and vanishes abruptly at 1.25 s having faded only to ~75%). Missing `Player.WeaponSelectionOpen`/`Close`. | P1 |
| D7 | `SelectionGrowTime` 0.4 s box grow when the open column changes; `TextScan` type-on of the name (§8.1, §8.3) | none | missing | Boxes pop instantly. | P2 |
| D8 | Slot keys / invnext / invprev / wrap / `DenyWeaponSelection` / `MoveSlot` (§8.5) | `main.rs:3462-3520` `wsel_slot`, `wsel_step` | match | - | - |
| D9 | `+attack` confirms with `Player.WeaponSelected` (§8.5) | `main.rs:7706-7716` | match | - | - |
| D10 | `lastinv`/`cancelselect` close with `Player.WeaponSelectionClose`; menu closed by death / external weapon change / `ResetHUD` (§8.5) | `main.rs:4366-4370` timeout only | missing | - | P2 |
| D11 | `hud_fastswitch` 0/1/2/3 int; modes 1 and 3 switch directly, "already in slot -> next in slot" (§8.5) | `settings.hud_fastswitch: bool` (`main.rs:3477`, `3507`) | partial | mode 2/3 absent; direct switch logic matches mode 1. | P2 |
| D12 | In a vehicle, selection keys are ignored (§8.5) | none | missing | - | P2 |
| D13 | SWEP `DrawWeaponSelection`: `weapons/swep` bounce `sin(t*10)*5`, icon rect `(x+10+fsin, y+10-fsin, wide-20-2fsin, (wide-20)/2+fsin)`, info bubble at `(x+wide+20, y+tall*0.95)`: `gui/speech_lid` 128x64 at `y-69`, `RoundedBox(8)` 260 wide 60 60 60, markup 250 (§8.8) | `main.rs:3660-3690`: icon rect identical; bubble origin `x + iw + 20 + 10` with `iw = wide-20` = `x + wide + 10` | partial | bubble 10 layout px too far left; check lid offset / rounded corners / grey 230 & 150 text colours. | P2 |
| D14 | Glyph fonts `WeaponIcons` 64 px fixed, `WeaponIconsSelected` blur 5 for the highlighted weapon (§8.1) | `64*fs`, no blurred variant | partial | see A2/A3 | P1 |
| D15 | `WeaponPickup` animation: menu shows briefly on pickup then fades (§8.6) | none | missing | picking up a weapon in HL2 flashes the selection bar. | P2 |

## E. Damage indicator

| # | behaviour | Rust today | status | missing / wrong | prio |
|---|---|---|---|---|---|
| E1 | Direction by angle bands (45-135 left, 225-315 right, 135-225 behind), `damage > 25` -> High variants (§9.3) | `main.rs:1569-1600` `damage_from`: HL2MP/CS style `dot > 0.3` per side, "within 50 u lights all sides" | wrong | Wrong model: front hits (angle < 45) show nothing in the game; Rust lights the side exceeding 0.3. No High/normal distinction. | P1 |
| E2 | Colours/timing: 255 88 0 200 -> 255 0 0 200 over 0.3 s -> 0 Deaccel 0.5 s (0.8 s total); High: 0.4 s + 2.4 s Deaccel (§9.4) | `main.rs:6066-6072` strength decays `2/s`, dropped below 0.4 (SDK HL2MP `Paint`) | wrong | Game bar lasts 0.8 s (normal) / 2.8 s (big hit) with an orange onset; Rust is a 0.5 s linear decay in pure red. | P1 |
| E3 | Geometry 30,100 36 wide, 240/200 tall, gradient `vgui/white_additive_linear` (§9.1-9.2) | `main.rs:6683-6700` same numbers, flat quads "outer full, inner a third", wash 0.3 | partial | no alpha gradient (additive linear ramp); vignette fullscreen for behind/High/drown missing. | P2 |
| E4 | Drown blue fullscreen 0 0 200 255 -> 0 over 1.1 s (§9.4) | none | missing | - | P2 |

## F. Hints, chat, death notices, target ID, pickups, notifications

| # | behaviour | Rust today | status | missing / wrong | prio |
|---|---|---|---|---|---|
| F1 | Lua hints schedule + `NOTIFY_HINT` 20 s + `ambient/water/drip1-4.wav` (§11.2) | `main.rs:3520-3565` `add_hint`/`tick_hints` | match | - | - |
| F2 | Notifications spring (`*15`, `0.95-dt*8`), anchors 0.8/0.015/0.04/200, bg 20 20 20 @0.6, `GModNotify` (§13.5) | `main.rs:8503-8670` `Notices`: `START_X 200`, `CHARGE_X 0.04`, `GAP_X 0.015`, `ANCHOR_Y 0.8`, `damp 0.95-dt*8`, bg 20 20 20 0.6 | match | label 1 px shadow 0 0 0 200 not seen in the label push (`main.rs:8655-8665`) | P2 |
| F3 | Death notices 0.85/0.04, `hud_deathnotice_time` 6, icon 255 80 0, 16 px gaps, `ChatFont`, fade last second (§13.2) | `main.rs:6778-6822` draw (`x=0.85w`, `y=0.04h`, `left = 6 - age`, alpha `left*255`), `main.rs:8251-8318` parse + `kill_icon()` | match | check the 15 glyph/scale pairs of `kill_icon()` (`main.rs:8286`) against §13.2; `hud_deathnotice_time` is a constant, not a convar. | P2 |
| F4 | Target ID: `TargetID` at centre+30, shadows (+1 120, +2 50), team colour, health `%` in `TargetIDSmall` h+5 (§13.3) | `main.rs:6824-6890` | match (per comment) | font sizes per A2 (12/14 fixed, not `*fs`) | P2 |
| F5 | Pickup history 5 s / 0.04 / 0.3, 300 wide, corner8, colours (§13.4) | `main.rs:8320-8500` `PickupHistory`: HOLD 5, FADEIN .04, FADEOUT .3, border 8, `colordelta.clamp(0.6,0.7)`, `230*colordelta` | match | - | - |
| F6 | Chat: `ChatFont` 12/14, `DullWhite`, `hud_saytext_time` 12, prompts "Say :"/"Say (TEAM) :", refusal under `cl_drawhud 0`, Lua `OnPlayerChat` (§12) | no chat box found in `main.rs` (no `say`, `ChatFont` only registered at `main.rs:9430`) | missing | Multiplayer chat is absent entirely. | P0 (multiplayer) |
| F7 | `TextMsg`/`HudMsg` text messages (center print, `game_text`) (§15) | none | missing | map `game_text` entities / `PrintMessage` center text invisible. | P1 |
| F8 | `HudHintDisplay` C++ panel (§11.1) | none | missing | not used in GMod MP; skip. | P2 |

## G. Screen fades, menu, loading, spawn/context hooks

| # | behaviour | Rust today | status | missing / wrong | prio |
|---|---|---|---|---|---|
| G1 | Fade maths IN/OUT/STAYOUT, avg RGB / max alpha across fades, MODULATE (§14.2) | `main.rs:8793-8810` `fade_alpha_at` (IN: hold then ramp; OUT: ramp then hold; flag 8 holds) - unit test `main.rs:9445` | partial | Single fade only (`screen_fade: Option<ScreenFade>` `main.rs:1241`), no MODULATE, no PURGE semantics, no averaging. | P2 |
| G2 | Durations quantised to 1/512 s unsigned shorts on the wire (§14.1) | `main.rs:8766-8780` parses float seconds from `"ScreenFade r g b a time hold flags"` | partial | quantisation (max 127.998 s, 2 ms steps) not modelled - invisible except for very long fades. | P2 |
| G3 | Fade drawn after the view, before the HUD (§14.3) | `main.rs:6140-6150` fade quad pushed before HUD items | match | - | - |
| G4 | Engine loading art `console/background01(_widescreen).vtf`, `startup_loading.vtf` (§17.1) | `main.rs:987-1000` main-menu background slideshow (Lua menu behaviour) | partial | the engine's static console background during the first seconds of startup / map load is not shown. | P2 |
| G5 | `+menu` / `+menu_context` -> `OnSpawnMenuOpen` / `OnContextMenuOpen` hooks and hints (§18) | `main.rs:3841-3880` | match | - | - |
| G6 | Spawn icon placeholders `vgui/spawnmenu/generating` / `broken` (§18) | not found in `main.rs` | missing | icons in the spawn menu appear instantly or blank instead of the "generating" placeholder. | P2 |
| G7 | Spawnlist save to `settings/spawnlist/<name>.txt` (§18) | not checked (server side `gm-script`) | - | - | P2 |

## H. Things the Rust client does that the game does not

1. `main.rs:3596` `dim` colour (half alpha) for non-active bucket numbers in the weapon selector - the game draws every bucket number in `SelectionNumberFg` at full `SelectionAlpha`.
2. `main.rs:3598` one shared box colour `0 0 0 80` for all boxes - no such colour exists in the scheme (90 / 150 / 70).
3. `main.rs:7047-7060` an "ALT" labelled secondary-ammo panel to the left of the ammo panel; the game's secondary panel has no text label and sits at `r76`.
4. `main.rs:6558` the crosshair glyph for *every* weapon including SWEPs and the unarmed state; the game falls back to the white `crosshair_default` sprite.
5. `main.rs:1569` CS/HL2MP-style directional damage model (dot > 0.3, 50 u "all sides") - not GMod's.
6. `register_ui_fonts` proportional scaling of all HL2 fonts and a Roboto fallback family - the game never scales `tall` and never uses Roboto.
7. `main.rs:6755` suit panel removed the instant armour hits 0 - the game fades it over 0.4 s.
8. `docs/spec/ui.md:9-11` records `HudSuit` 108 wide, `HudAmmo` "44 from the right", `HudDefault` Verdana 12, `BoxGap 8` - all four differ from the shipped `HudLayout.res`/`ClientScheme.res` (102, `r150`, 9/13, 6).
