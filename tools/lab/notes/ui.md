# Garry's Mod client UI / HUD - behaviour note (`ui`)

Scope: the C++ side of the GMod client HUD and menus (what the Windows `client.dll` / `engine.dll` do), with the shipped
resource files that parametrise it and the base-gamemode Lua that the C++ hands off to. Written so the Rust rewrite can
reproduce look, timing and state machines without the binary.

Tag legend (every fact ends with one):
- `[pc H/M/L]` - read in the Ghidra pseudo-C of the user's GMod client/engine (strings, constants, control flow). Binary wins over everything else.
- `[res H]` - the shipped GMod resource files (`garrysmod/scripts/HudLayout.res`, `HudAnimations.txt`, `hud_textures.txt`, `resource/ClientScheme.res`) read from the public garrysmod repo. These override compiled defaults at run time.
- `[lua H]` - shipped base/sandbox gamemode Lua (`garrysmod/gamemodes/base`, `lua/includes/modules`) - the C++ calls into these, so they are part of the observable behaviour.
- `[sdk H/M]` - Source SDK 2013 code that the GMod pseudo-C was checked against (same strings/constants/control flow found in the dump unless noted).
- `[inf M/L]` - inference.
H = verified directly, M = verified in one source and consistent with another, L = plausible but not confirmed.

Units: pixels are "layout pixels" on the 640x480 reference unless stated; seconds are game seconds; colours are `R G B A` 0-255.

---

## 1. HUD framework, loading order, scaling

1.1 On client init the HUD loads, in this order: `scripts/HudLayout.res` (panel positions/sizes), `scripts/hud_textures.txt`
and `scripts/mod_textures.txt` (sprite sheet icon definitions), the VGUI scheme `resource/ClientScheme.res`, and the
animation scripts listed in `scripts/hudanimations_manifest.txt` (which lists `scripts/HudAnimations.txt`). `[pc H]`

1.2 Every HUD panel has a `fieldName` in `HudLayout.res`; keys `xpos/ypos/wide/tall` accept the Source prefixes
`r<N>` (N from the right/bottom edge), `c<N>` (offset from screen centre) and `f<N>` (full width minus N, GMod addition used by
`HudMessage`: `wide f0`). All values are 640x480 proportional: the panel is scaled by `screenH/480` vertically and
`screenW/640` horizontally in Source's proportional scaling, but because every panel is placed with proportional scaling
on *both* axes using the same 480-based factor in practice at 4:3, and the SDK HL2 HUD uses `scheme()->GetProportionalScaledValueEx`
which is `value * screenH / 480` for BOTH axes (Source proportional scaling is height based), the effective rule is:
**x and y and sizes all scale by `screenH / 480`**; `r`/`c` anchors are then applied against the real screen width. `[sdk H][pc M]`

1.3 Scheme fonts are *not* proportional: a `tall` in `ClientScheme.res` is absolute screen pixels regardless of
resolution, unless the font definition has `yres "a b"` blocks, in which case the block matching the current vertical
resolution is used (see 2.2). This is why HL2 digits look small at 1080p. `[sdk H][res H]`

1.4 HUD element hiding: every element has hide bits; the server's `m_iHideHUD` (HIDEHUD_* flags: 1 weaponselection, 2 flashlight,
4 all, 8 health, 16 playerdead, 32 needsuit, 64 misc status, 128 chat, 256 crosshair, 512 vehicle crosshair, 1024 in vehicle,
2048 bonus progress) suppress them; additionally GMod asks the Lua hook `GM:HUDShouldDraw(name)` with the element's class name
(`CHudHealth`, `CHudBattery`, `CHudAmmo`, `CHudSecondaryAmmo`, `CHudCrosshair`, `CHudWeaponSelection`, `CHudDamageIndicator`,
`CHudChat`, `CHudHistoryResource`, `CHudZoom`, `CHudPoisonDamageIndicator`, `CHudSuitPower`, `CHudGeiger`, `CHudSquadStatus`,
`CHudHintDisplay`, `CHudCloseCaption`, `CHudMessage`, `CHudMenu`, `CHudTrain`, `CHudVehicle`, `CHudCredits`, `CHUDQuickInfo`, ...)
before painting; a `false` return skips the element for that frame. Convar `cl_drawhud 0` hides the whole HUD and also
refuses to open the chat box (console message "Preventing opening chatbox because its invisible! (cl_drawhud 0)"). `[pc H][lua H]`

1.5 Client user messages registered by the HUD layer (names on the wire): `AchievementEvent`, `Battery`, `CloseCaption`,
`CreditsMsg`, `Damage`, `GameTitle`, `Geiger`, `HudMsg`, `HudText`, `ItemPickup`, `LogoTimeMsg`, `RequestState`, `ResetHUD`,
`Rumble`, `SayText`, `Shake`, `SquadMemberDied`, `TextMsg`, `Train`, `UpdateJalopyRadar`, `VGUIMenu`, `VoiceMask`. `[pc H]`

1.6 Game events the HUD listens to at init: player connect/disconnect/name change/team change, `server_cvar`,
`server_spawn`, `teamplay_broadcast_audio`, achievement events. `[pc H]`

1.7 Panel backgrounds: `PaintBackgroundType 2` means "rounded box" drawn from the four corner sprites
`vgui/hud/800corner1..4` with the panel's `BgColor` (HUD panels use `BgColorHud`); type 0 is a flat rect, type 1 a textured box. `[sdk H][res H]`

---

## 2. Scheme (`resource/ClientScheme.res`)

2.1 Colours (`R G B A`): `[res H]`
| name | value | used by |
|---|---|---|
| `FgColor` | 255 235 20 255 | generic HUD text (poison indicator, labels when a panel doesn't override) |
| `FgColorHud` | 255 220 0 220 | health/suit/ammo digits and labels, weapon selection foreground |
| `BgColor` | 0 0 0 76 | generic panel background |
| `BgColorHud` | 0 0 0 90 | HUD panel rounded background (health/suit/ammo/aux power) |
| `BrightFg` | 255 220 0 255 | health/suit "flash" on damage |
| `BrightDamagedFg` | 255 0 0 255 | |
| `DamagedBg` | 180 0 0 200 | low-health panel background |
| `DamagedFg` | 180 0 0 230 | low-health digits |
| `Normal` | 255 208 64 255 | crosshair glyph colour (unzoomed) |
| `Caution` | 255 48 0 255 | reserved (SDK uses it for empty-ammo cues) |
| `DullWhite` | 190 190 190 255 | chat default line colour |
| `OffWhite` | 221 221 221 255 | |
| `Gray` | 178 178 178 255 | |
| `White` | 255 255 255 255 | |
| `Black` | 46 43 42 255 | (GMod: not pure black) |
| `TransparentBlack` | 0 0 0 128 | |
| `Blank` | 0 0 0 0 | |
| `SelectionBoxBg` | = `BgColorHud` (0 0 0 90) | weapon-selection unselected box |
| `SelectionSelectedBoxBg` | 0 0 0 150 | weapon-selection highlighted box |
| `SelectionEmptyBoxBg` | 0 0 0 70 | weapon-selection empty bucket box |
| `SelectionNumberFg` | 255 220 0 255 | bucket numbers |
| `SelectionTextFg` | = `FgColorHud` | weapon name under icon |
| `ZoomReticleColor` | 255 220 0 255 | zoom overlay |
| `GMod_BG` | 108 111 114 0 | GMod addition |

2.2 Fonts (`name: face, tall px, weight, flags`; `yres` blocks pick by vertical resolution): `[res H]`
- `Default`: Verdana 12, weight 700, antialias. `DefaultSmall`: Verdana 12 w0. `DefaultVerySmall`: Verdana 11.
- `HudNumbers`: HalfLife2 32, additive, custom. `HudNumbersGlow`: HalfLife2 32, blur 4, scanlines 2, additive.
  `HudNumbersSmall`: HalfLife2 (fallback Helvetica) 16, weight 1000, additive.
- `HudDefault`: Verdana, weight 700 - `yres 1..599` -> 9 px, `yres 600..` -> 13 px. (Not proportional beyond that step.)
- `HudSelectionNumbers`: Verdana 11, weight 700, additive. `HudSelectionText`: Verdana weight 700 - 8 px below 600 lines, 10 px at 600+.
- `WeaponIcons`: HalfLife2 64, additive, antialias, custom. `WeaponIconsSelected`: HalfLife2 64, blur 5, scanlines 2, additive.
  `WeaponIconsSmall`: HalfLife2 32, additive.
- `Crosshairs`: HalfLife2 40, additive, no antialias. `QuickInfo`: HL2cross 28, additive.
- `HudHintTextLarge`: Verdana 14, weight 1000, additive. `HudHintTextSmall`: Verdana 11, additive.
- `CloseCaption_Normal`: Tahoma (fallback Verdana) 26, weight 500. `CenterPrintText`: Trebuchet MS (fallback Helvetica) 18, weight 900, additive.
  `CreditsText`: Trebuchet 20, weight 900, additive. `Trebuchet24`: Trebuchet 24, weight 900, additive.
- `ChatFont`: Verdana weight 700, dropshadow - 12 px below 600 lines, 14 px at 600+. `TargetID`: same face/sizes/dropshadow.
- `Marlett`: Marlett 14 (vgui symbols).
- Fonts are system fonts (Verdana/Tahoma/Trebuchet/Arial from the OS) except `HalfLife2`/`HL2cross`/`HL2MP` which come from
  `resource/halflife2.ttf`, `HL2crosshairs.ttf`, `csd.ttf` shipped with the game. `[res H][inf M]`

2.3 Base gamemode Lua fonts created on top of the scheme (used by the HUD pieces the C++ delegates to Lua):
`GModNotify` Arial, weight 500, tall `max(12, ceil(ScreenScaleH(9)))` where `ScreenScaleH(v) = v * screenH / 480`;
`HL2MPTypeDeath` csd.ttf 64 (kill icons); `TargetIDSmall` Verdana 11 bold; `DermaDefaultBold` Tahoma 13 bold (Linux: DejaVu Sans 14). `[lua H]`

---

## 3. Health panel (`HudHealth`, class `CHudHealth`)

3.1 Layout (`HudLayout.res`): x 16, y 432, 102x36, `PaintBackgroundType 2`, label at (8,20) in `HudDefault`, digits at (50,2)
in `HudNumbers` (glow pass in `HudNumbersGlow` with the panel's `Blur` value), colours `FgColorHud` / `BgColorHud`.
Label text is the localised `#Valve_Hud_HEALTH` ("HEALTH"). Compiled defaults are the same text/digit offsets. `[res H][pc H]`

3.2 Data: health comes from the local player's networked health each frame; the `Damage` user message (armor byte,
damage byte, origin vector, damage type int) also drives the health panel: if the damage amount is > 0 the panel fires
the animation event `HealthDamageTaken`. Displayed value is clamped at 0 (negative health shows 0). `[pc H]`

3.3 Threshold state machine (evaluated whenever the value changes; events are `HudAnimations.txt` sequences): `[pc H][res H]`
- new value < 20 and > 0 -> `HealthIncreasedBelow20` (one-shot) and `HealthLow` (looping);
- new value >= 20 -> `HealthIncreasedAbove20` (stops the low loop);
- new value >= 1000 -> `HealthOver999` (panel grows to 116x36); < 1000 -> `HealthBelow999` (back to 102x36).

3.4 Animation sequences (`HudAnimations.txt`, syntax `Animate <property> <value> <interp> <start s> <duration s>`): `[res H]`
- `HealthDamageTaken`: `FgColor BrightFg Linear 0 0.25`, `FgColor FgColorHud Linear 0.3 0.75`, `Blur 3 Linear 0 0.1`,
  `Blur 0 Deaccel 0.1 2.0`, `TextColor BrightFg Linear 0 0.1`, `TextColor FgColorHud Deaccel 0.1 1.2` - digits flash
  bright yellow and glow, settling within ~1 s (glow tail 2 s).
- `HealthLow` (health < 20): stops `HealthDamageTaken/HealthPulse/HealthLoop`; `BgColor DamagedBg Linear 0 0.1`,
  `BgColor BgColorHud Deaccel 0.1 1.75`, `FgColor BrightFg Linear 0 0.2`, `FgColor DamagedFg Linear 0.2 1.2`,
  `TextColor BrightFg Linear 0 0.1`, `TextColor DamagedFg Linear 0.1 1.2`, `Blur 5 Linear 0 0.1`, `Blur 3 Deaccel 0.1 0.9`,
  then `RunEvent HealthPulse 1.0`.
- `HealthPulse`: `Blur 5 Linear 0 0.1`, `Blur 2 Deaccel 0.1 0.8`, `TextColor BrightDamagedFg Linear 0 0.1`,
  `TextColor DamagedFg Deaccel 0.1 0.8`, `BgColor "100 0 0 80" Linear 0 0.1`, `BgColor BgColorHud Deaccel 0.1 0.8`,
  `RunEvent HealthLoop 0.8`; `HealthLoop` re-runs `HealthPulse` - a red pulse with period ~0.9 s while health < 20.
- `HealthIncreasedAbove20`: stops `HealthLoop/HealthPulse/HealthLow`; `BgColor BgColorHud` instantly, `TextColor FgColorHud Linear 0 0.04`,
  `FgColor FgColorHud Linear 0 0.03`, `Blur 3 Linear 0 0.1`, `Blur 0 Deaccel 0.1 2.0`.
- `HealthIncreasedBelow20`: stops the loops, damaged colours, `Blur 5 Deaccel 0 0.4`, `RunEvent HealthPulse 0.4`.
- `HealthOver999`: `Size "116 36" Deaccel 0 0.4`; `HealthBelow999`: `Size "102 36" Deaccel 0 0.4`.
- `ResetHUD` (respawn / `ResetHUD` usermsg) stops the health/suit loops and restores colours, sizes and alpha.

## 4. Suit panel (`HudSuit`, class `CHudBattery`)

4.1 Layout: x 140, y 432, 102x36, rounded background, label `#Valve_Hud_SUIT` ("SUIT") at (8,20), digits at (50,2);
same fonts/colours as health. `[res H]`

4.2 Data: `Battery` user message carries a signed 16-bit armour value. `[pc H]` Transitions: `[pc H][res H]`
- value 0 -> `SuitPowerZero` (`Alpha 0 Linear 0 0.4` - panel fades out, it is not removed);
- value decreased (and > 0) -> `SuitDamageTaken` (identical numbers to `HealthDamageTaken`: `FgColor BrightFg 0/0.25`, back `0.3/0.75`, `Blur 3` then `0` over 2 s,
  `TextColor BrightFg 0/0.1` back Deaccel `0.1/1.2`); if the new value <= 19 also `SuitArmorLow`, which in GMod's file is just the same flash -
  the red background and the pulse loop are commented out, so **the suit never pulses red** (differs from health);
- value increased to >= 20 -> `SuitPowerIncreasedAbove20` (`Alpha 255`, stop low loops, restore colours over 0.4 s);
- value increased but < 20 -> `SuitPowerIncreasedBelow20` (`Alpha 255`, damaged colours, pulse loop);
- >= 1000 -> `SuitOver999` (116x36), < 1000 -> `SuitBelow999` (102x36).
The panel is also hidden when the player has no suit (HIDEHUD_NEEDSUIT) - in GMod every player has the suit. `[sdk H][inf M]`

## 5. Ammo panels (`HudAmmo` class `CHudAmmo`, `HudSecondaryAmmo` class `CHudSecondaryAmmo`)

5.1 `HudAmmo` layout (GMod changed vs SDK, comments in the file say so): x `r150`, y 432, 136x36, rounded background,
label `#Valve_Hud_AMMO` at (8,20), primary digits at **(40,2)** in `HudNumbers`, reserve digits at **(102,16)** in
`HudNumbersSmall`. Compiled defaults are (50,2)/(98,16) and are overridden. `[res H][pc H]`

5.2 `HudSecondaryAmmo` has **no** block in GMod's `HudLayout.res`, so compiled defaults apply; the HL2 defaults (hl2
`HudLayout.res`, mounted content) are x `r76`, y 432, 60x36, label (8,22), digits (36,2). `[sdk M][res H]`

5.3 Which numbers are shown (evaluated each frame from the active weapon): `[pc H][sdk H]`
- weapon uses a clip: primary = clip count, secondary small number = reserve ammo of the primary type
  -> animation `WeaponUsesClips` (panel 136 wide, small number visible);
- weapon has no clip (clip value < 0): primary = total reserve, small number hidden -> `WeaponDoesNotUseClips`;
- weapon has secondary ammo -> `WeaponUsesSecondaryAmmo` (second panel alpha 255) else `WeaponDoesNotUseSecondaryAmmo`
  (alpha 0); combined variants (`WeaponUsesClips`/`WeaponDoesNotUseClips` x secondary) exist and set both at once.
- weapon change -> `WeaponChanged`: panel `BgColor "250 220 0 80" Linear 0 0.1` then `BgColorHud Deaccel 0.1 1.0` (yellow background flash; the digit flash lines are commented out);
  ammo decreased -> `AmmoDecreased`: `Blur 7` instantly, `Blur 0 Deaccel 0.1 1.5`, `FgColor FgColorHud` snapped (GMod fix), `TextColor BrightFg Linear 0 0.1`, `TextColor FgColorHud Deaccel 0.1 0.75`;
  ammo increased -> `AmmoIncreased`: `FgColor BrightFg Linear 0 0.15`, `FgColor FgColorHud Deaccel 0.15 1.5`, `Blur 5` instantly, `Blur 0 Accel 0.01 1.5`.
- `HudAmmo` is hidden when there is no active weapon, when the weapon uses no ammo at all, in a vehicle without a weapon,
  and when the weapon's `DrawAmmo` is false.
- GMod addition: scripted weapons (SWEPs) supply the values through their Lua table (`Primary.ClipSize`, `Clip1()`,
  `Ammo1()`, `Secondary`...), and `SWEP.DrawAmmo` (default true) controls the panel. A clip size of -1 means "no clip". `[pc H][lua H]`

## 6. Auxiliary power (`HudSuitPower`, class `CHudSuitPower`)

6.1 Layout: x 16, y 396, 102x26, rounded background; bar inset (8,15), bar 92x4, chunk width 6, chunk gap 3 (so 10 chunks
fill 92 px: 10*6+9*3 = 87 <= 92); label `#Valve_Hud_AUX_POWER` at (8,4) in `HudDefault`; active-item labels at (8,22) with 10 px gap. `[res H]`

6.2 Data: suit power 0..100 (float) and the "active devices" bitmask: 1 sprint, 2 flashlight, 4 oxygen (breath under water).
Number of filled chunks = `round(power/100 * barWidth / (chunkW+chunkGap))`; the rest are drawn at the "disabled" alpha 70. `[pc H][res H]`

6.3 Visibility/colours: the panel is shown while power < 100 **or** any device is active; at 100 and idle it fades out
(`SuitAuxPowerMax`: `BgColor` and `AuxPowerColor` -> transparent over 0.4 s). While draining: `SuitAuxPowerNotMax`
(`BgColor BgColorHud`, `AuxPowerColor 255 220 0 220`). Below 25: `SuitAuxPowerDecreasedBelow25` -> `AuxPowerColor 255 0 0 220`;
back above 25 -> `SuitAuxPowerIncreasedAbove25` (yellow). Resource names: `AuxPowerLowColor 255 0 0 220`,
`AuxPowerHighColor 255 220 0 220`, `AuxPowerDisabledAlpha 70`. `[res H][pc H]`

6.4 Device labels drawn under the bar when active: `#Valve_Hud_SPRINT` "SPRINT", `#Valve_Hud_FLASHLIGHT` "FLASHLIGHT",
`#Valve_Hud_OXYGEN` "OXYGEN" (fallback English strings are compiled in). `[pc H]`

## 7. Crosshair (`HudCrosshair`, class `CHudCrosshair`) and zoom

7.1 Default texture `crosshair_default` = `sprites/crosshairs` sheet, rect x0 y48 24x24, drawn white (255 255 255 255)
centred on the screen centre (plus the crosshair angle offset for vehicles). Convars: `crosshair` (default 1; 0 hides),
`cl_observercrosshair` (default 1). Not drawn when: no active weapon or the weapon says no crosshair, client mode says no,
player dead and not in-eye spectating, HIDEHUD_CROSSHAIR, or in a vehicle without a vehicle crosshair. `[pc H][sdk H]`

7.2 Per-weapon crosshair: each weapon script (`scripts/weapon_*.txt`, `TextureData`) may define glyphs `crosshair`,
`autoaim`, `zoom`, `zoom_autoaim`, `weapon`, `weapon_s`, `ammo`, `ammo2`; GMod's SWEP loader builds the same table from the
Lua `SWEP` fields and uses `selection` as the fallback when `weapon`/`weapon_s` are missing. Glyph fonts: `Crosshairs`
(HalfLife2 40) - all HL2 weapons use character `Q`. While the player's FOV >= 90 the `crosshair` glyph (or `autoaim` when
the auto-aim target is set) is drawn in scheme `Normal` (255 208 64) with alpha 255; when FOV < 90 (zoomed) the `zoom`
glyph (or `zoom_autoaim`) is drawn in white. `[pc H][sdk H]`

7.3 GMod hooks: `GM:HUDShouldDraw("CHudCrosshair")` and `SWEP:DoDrawCrosshair(x, y)` (return true to suppress the C++ one). `[lua H]`

7.4 Quick info (`HUDQuickInfo`, HL2 "sniper" style ammo arcs around the crosshair): textures `crosshair`,
`crosshair_left/_left_empty/_left_full`, `crosshair_right/_right_empty/_right_full` from `hud_textures.txt`; enabled by
convar `hud_quickinfo` (default 0 in GMod -> normally invisible). `[pc H][inf M]`

7.5 Zoom overlay (`HudZoom`, class `CHudZoom`): material `vgui/zoom`, colour `ZoomReticleColor` 255 220 0 255; two circles
radii 66 and 74 (layout px), dashes every 16 px, dash height 4, border thickness 88; fade time 0.4 s: while zooming in
alpha = `t/0.4` (0->1), while zooming out alpha = `(1 - t/0.4) * 0.25` and the circle radii scale by `1 - scale*0.5`; the
final alpha is multiplied by 64 (so max 64/255) and the screen outside the circle is covered by the dark `vgui/zoom`
quad. Active when the player's FOV is below the default (weapon zoom, `+zoom`). `[pc H][res H][sdk H]`

## 8. Weapon selection (`HudWeaponSelection`, class `CHudWeaponSelection`)

8.1 Layout values (GMod `HudLayout.res`, overriding compiled defaults 24/32/108/72/75/50/12/0.1/54): y 16,
`SmallBoxSize` 32, `LargeBoxWide` 112, `LargeBoxTall` 80, `LargeBoxUnselectedTall` 19 (unused in bucket mode),
`BoxGap` **6**, `SelectionNumberXPos/YPos` 4,4, `SelectionGrowTime` 0.4, `TextYPos` 64; `MediumBox` 75x50 and `TextScan`
1.0 compiled. Fonts `WeaponIcons` (64), `WeaponIconsSelected`, `WeaponIconsSmall` (32), numbers `HudSelectionNumbers`,
names `HudSelectionText`. Colours `SelectionBoxBg`, `SelectionSelectedBoxBg`, `SelectionEmptyBoxBg`, `SelectionNumberFg`,
`SelectionTextFg`, foreground `FgColorHud`. `[res H][pc H]`

8.2 Mode: GMod's default `hud_fastswitch` is 0 -> HL2 "bucket" menu (HUDTYPE_BUCKETS). The plus/carousel SDK modes are
not active in the dump. `[pc H][inf M]`

8.3 Bucket geometry (bucket mode; one column per bucket/slot 0..5, left to right): the row is centred horizontally:
`x0 = (screenW - totalWidth) / 2` with `totalWidth = LargeBoxWide + 5*(SmallBoxSize + BoxGap)` when a bucket is open
(all six buckets are always laid out; the open one is wide), y = `ypos` (16). A non-active bucket with weapons is one
`SmallBoxSize` square in `SelectionBoxBg` showing its number at (4,4) in `HudSelectionNumbers` / `SelectionNumberFg`; an
empty bucket is a square in `SelectionEmptyBoxBg` with no number. The active bucket is a column of `LargeBoxWide x
LargeBoxTall` boxes, one per weapon, `BoxGap` apart vertically, each with the weapon's `weapon` glyph (`weapon_s` for the
highlighted one, in `WeaponIconsSelected`) centred in the box, the bucket number at (4,4) on the first box, and the
highlighted weapon's localised print name at `TextYPos` (64) in `HudSelectionText` / `SelectionTextFg`. Highlighted box
background `SelectionSelectedBoxBg`, the others `SelectionBoxBg`. A weapon with no ammo draws its glyph red (255 0 0)
instead of the foreground colour. When the open column changes, the new boxes grow from `SmallBoxSize` to the large size
over `SelectionGrowTime` (0.4 s). `[sdk H][res H][pc M]`

8.4 Ordering inside a bucket: by the weapon's `bucket_position` (SWEP `SlotPos`), ties by order of acquisition. `[pc H][sdk H]`

8.5 Input state machine (menu closed / open / fading): `[pc H][sdk H]`
- `slotN` (keys 1..0 -> buckets 0..9): if in a vehicle: ignored. Mode 0: if the bucket is empty -> sound
  `Player.DenyWeaponSelection`; else open (or keep open) the menu with the first weapon of that bucket highlighted - if
  the currently highlighted (or active, when the menu was closed) weapon is already in that bucket, highlight the next one
  in the bucket, wrapping to the first; sound `Player.WeaponSelectionMoveSlot`; opening also plays `Player.WeaponSelectionOpen`
  and starts `OpenWeaponSelectionMenu`.
- `invnext` / `invprev`: walk the flattened (bucket, position) order from the highlighted (or active) weapon, wrapping
  around at the ends (next from the last goes to the first weapon of bucket 0); same open/move behaviour and sounds;
  weapons the player cannot select (no ammo and no "can select with no ammo" flag) are skipped. `[sdk H][pc M]`
- `+attack` while open: select the highlighted weapon (client issues the switch), play `Player.WeaponSelected`,
  run `CloseWeaponSelectionMenu`, close.
- `lastinv` while open or `cancelselect`: close with `Player.WeaponSelectionClose`.
- Timeout: `SELECTION_TIMEOUT_THRESHOLD` 0.5 s after the last input the menu starts `FadeOutWeaponSelectionMenu` and is
  considered closed (highlight dropped) `SELECTION_FADEOUT_TIME` 0.75 s later (in fast-switch modes 1.5 s / 1.5 s).
  Any input during the fade re-opens instantly (`OpenWeaponSelectionMenu`). `[sdk H][pc M]`
- `hud_fastswitch` 1 or 3: `slotN` switches immediately to the first weapon of the bucket (or the next one in the bucket
  if the active weapon is already there, wrapping), no menu; an empty bucket plays `Player.DenyWeaponSelection`;
  `invnext/invprev` switch immediately too. Mode 2 is the console/"plus" mode and keeps its own state. `[pc H]`
- Menu closes automatically when the player dies, when the active weapon changes from outside, or on `ResetHUD`.

8.6 Animation sequences (`HudAnimations.txt`): `[res H]`
- `OpenWeaponSelectionMenu`: `StopEvent CloseWeaponSelectionMenu`, `StopEvent FadeOutWeaponSelectionMenu`,
  `StopEvent WeaponPickup`; `Alpha 255 Linear 0 0.1`, `SelectionAlpha 255 Linear 0 0.1`, `FgColor FgColorHud Linear 0 0.1`,
  `TextColor SelectionTextFg Linear 0 0.1`, `TextScan 1 Linear 0 0.1`.
- `CloseWeaponSelectionMenu`: `FgColor "0 0 0 0"`, `TextColor "0 0 0 0"`, `Alpha 0`, `SelectionAlpha 0` all Linear 0 0.1.
- `FadeOutWeaponSelectionMenu`: the same four to 0, Linear **start 0.5, duration 1.0** (so the game's boxes are only
  half faded when the C++ side hides the panel at 0.75 s after the timeout - visible as an abrupt vanish). `[res H][inf M]`
- `WeaponPickup` (fired when a new weapon is picked up while the menu is closed): shows the menu at `Alpha 255`
  `SelectionAlpha 0` for a moment then `FadeOutWeaponSelectionMenu`-like fade (`Alpha 0 Linear 2.0 1.0`).

8.7 Sounds (all `game_sounds_ui.txt`, channel UI, volume 0.32 default): `Player.WeaponSelectionOpen`,
`Player.WeaponSelectionClose`, `Player.WeaponSelectionMoveSlot`, `Player.WeaponSelected`, `Player.DenyWeaponSelection`. `[pc H][lua H]`

8.8 GMod/SWEP drawing: when the highlighted weapon is a scripted weapon the C++ calls `SWEP:DrawWeaponSelection(x, y, wide, tall, alpha)`
instead of drawing a glyph. The base implementation (`weapon_base/cl_init.lua`): draws `weapons/swep` (white, `alpha`) inside the
box inset 10 px, `wide-20` wide and `wide/2` tall, bouncing by `fsin = sin(CurTime()*10)*5` (rect `(x+fsin, y-fsin, wide-2*fsin, wide/2+fsin)`)
when `SWEP.BounceWeaponIcon` (default true); then `PrintWeaponInfo(x+wide+20, y+tall*0.95, alpha)`: `gui/speech_lid` 128x64 at
`(x, y-69)` and a `draw.RoundedBox(8, x-5, y-5, 260, markupH+18, Color(60,60,60,alpha))` containing markup (width 250) with
"Author:" `<color=230,230,230>` title and `<color=150,150,150>` text lines for `SWEP.Author`, `Contact`, `Purpose`, `Instructions`.
A scripted weapon with `SWEP.DrawWeaponInfoBox = false` skips the bubble. `[lua H][pc M]`

---

## 9. Damage indicator (`HudDamageIndicator`, class `CHudDamageIndicator`) - HL2 style

9.1 Layout: `dmg_xpos` 30, `dmg_ypos` 100, `dmg_wide` 36, `dmg_tall1` 240 (outer), `dmg_tall2` 200 (inner);
`DmgColorLeft/Right` start fully transparent red (255 0 0 0); `DmgFullscreenColor` 255 0 0 0, `DmgHighColorLeft/Right`
255 0 0 0. Compiled defaults (10, 80, 30, 300, 240) are overridden. On resolution change the geometry is recomputed as
y = 0.125*screenH, tall1 = 0.625*screenH, tall2 = 0.4*screenH, wide = 0.1*screenW (that is where 100/240/200/36@640x480 come from). `[res H][pc H]`

9.2 Materials: side bars `vgui/white_additive_linear` (alpha gradient from the edge inwards), full-screen flash
`vgui/white_additive_vignette`. Each side is two quads: the outer `tall1` tall at full colour alpha fading to 0 over `wide`,
the inner `tall2` tall. `[pc H][sdk H]`

9.3 Message: `Damage` usermsg -> armor (byte), damage (byte), origin (vector), damage type (int). If `damage > 0 || armor > 0`:
`[pc H]`
- `damage > 25` selects the "High" variants.
- Direction: delta = origin - player eye origin, normalised; angle = atan2 of the delta against the player's forward
  (view yaw only, pitch ignored) and right vectors, in degrees 0..360 where 0 = in front:
  45..135 -> **left**, 225..315 -> **right**, 135..225 -> **behind** (both sides + fullscreen), else -> front (`HudTakeDamageFront`, which only shakes/animates nothing visible in HL2 scripts).
- A damage origin of (0,0,0) or a damage type with no direction (e.g. drowning) goes to the "behind"/fullscreen branch.
- Damage-type overrides (fire fullscreen events regardless of direction): `DMG_DROWN` -> `HudTakeDamageDrown`,
  `DMG_POISON` -> `HudTakeDamagePoison`, `DMG_BURN` -> `HudTakeDamageBurn`, `DMG_RADIATION` -> `HudTakeDamageRadiation`,
  `DMG_ACID` -> `HudTakeDamageAcid` (the last four exist as events; only Drown has a script body in HL2's file). `[pc H][res M]`

9.4 Animations (`HudAnimations.txt`): `[res H]`
- `HudTakeDamageLeft` / `Right`: `DmgColorLeft "255 88 0 200" Linear 0 0`, `"255 0 0 200" Linear 0 0.3`, `"255 0 0 0" Deaccel 0.3 0.5`
  (orange-red to red in 0.3 s, gone at 0.8 s).
- `HudTakeDamageBehind`: `RunEvent HudTakeDamageLeft 0`, `RunEvent HudTakeDamageRight 0` (both bars).
- `HudTakeDamageHighLeft/Right`: `DmgHighColorLeft "255 88 0 255" Linear 0 0`, `"255 0 0 200" Linear 0 0.4`, `"255 0 0 0" Deaccel 0.4 2.4` (gone at 2.8 s).
- `HudTakeDamageHighBehind` / `HudTakeDamageHighFullscreen`: fullscreen vignette 255 0 0 at alpha 255 held, then Deaccel to 0 (same 0.4 / 2.4 timing).
- `HudTakeDamageDrown`: `DmgFullscreenColor "0 0 200 255" Deaccel 0 0.1`, `"0 0 200 0" Deaccel 0.1 1.0`, `RunEvent HudTakeDamageBehind 0`.
- `HudTakeDamageFront` has no visible script body.

9.5 Other HL2 damage effects that live outside this panel but are player-visible: the `Shake` usermsg (screen shake),
`Rumble`, and the server-sent death `ScreenFade` (HL2: dark red 64 0 0 200, 0.5 s, OUT|STAYOUT - server side, not in this dump) `[sdk M]`.

## 10. Poison indicator (`HudPoisonDamageIndicator`, class `CHudPoisonDamageIndicator`)

Layout: x 16, y 346, 136x38, hidden by default, `text_xpos 8`, `text_ypos 8`, `text_ygap 14`, `TextColor 255 170 0 220`,
rounded background; compiled defaults 8/8/14. Font `HudDefault`, text colour `FgColor`; message token
`#Valve_HudPoisonDamage` ("NEUTRALIZING POISON...") split on `\n` into lines `text_ygap` apart. Shown while the player's
"poisoned" flag is set (health will regenerate), hidden by HIDEHUD bits 0x38 (health|playerdead|needsuit). `[pc H][res H]`

## 11. Hints

11.1 C++ `HudHintDisplay`: x `r120`, y `r340`, 100x200, hidden by default, text inset (8,8), gaps 8, `TextColor 255 170 0 220`,
rounded background; fonts `HudHintTextLarge/Small`; shown by the `KeyHintText`/`HintText` usermsgs for the HL2 single-player
hints (`env_instructor_hint` style). In GMod multiplayer these messages are essentially never sent; **GMod hints are Lua**. `[res H][pc M]`

11.2 Lua hints (`cl_hints.lua`, base gamemode), the ones a player sees: a hint fires once per name after a delay,
as `notification.AddLegacy(text, NOTIFY_HINT, 20)` with the `%+bind%` tokens replaced by the key name, plus sound
`ambient/water/drip{1..4}.wav` (random 1-4). Schedule: `OpeningMenu` 30 s after load, `Annoy1` 5 s, `Annoy2` 7 s,
`OpeningContext` 20 s and `EditingSpawnlists` 5 s on the first spawn-menu open, `ContextClick` 20 s on the first context-menu open,
`PhysgunFreeze`/`PhysgunUse` from the physgun. Server `Player:SendHint(name, delay)` -> the same path. `[lua H]`

## 12. Chat (`HudChat`, class `CHudChat` - GMod's own chat box built on the SDK base)

12.1 Resources: `resource/ChatScheme.res`, `resource/chat_%language%.txt`, `resource/UI/BaseChat.res`,
`resource/UI/ChatFilters.res`. Panels: `HudChat` (in `HudLayout.res` deliberately `visible 0`, 4x4 at (0,0) - the SDK
history box is not shown; GMod draws chat through its own panel and `chat.AddText`/`GM:OnPlayerChat`), `ChatInputLine`,
`HudChatHistory`, `HudChatFilterPanel`. `[pc H][res H]`

12.2 Opening: `messagemode` -> prompt `#chat_say` ("Say :", command `say`), `messagemode2` -> `#chat_say_team`
("Say (TEAM) :", `say_team`); generic prompt "Enter text:". Refused (with the console warning) when `cl_drawhud 0`; sets
`gameui_preventescapetoshow` so Esc closes chat instead of opening the game menu. Lua hooks `StartChat(teamChat)`,
`FinishChat()`, `ChatText(...)`, `ChatTextChanged(text)`, `OnPlayerChat(ply, text, teamChat, isDead)` run around it. `[pc H][lua H]`

12.3 Rendering constants (SDK base, same values in the dump): lines use `ChatFont` (12/14 px by resolution), default line
colour `DullWhite`; `hud_saytext_time` 12 s visible per line; new-line flash window 5 s with 8 flashes; fade-out 1 s;
history panel alpha 127, history fades in/out over 0.25 s, idle history hides after 15 s with a 2.5 s fade; filter bits
`cl_chatfilters` default 63 (1 joinleave, 2 namechange, 4 publicchat, 8 servermsg, 16 teamchange, 32 achievement);
`cl_showtextmsg 1`, `cl_mute_all_comms 1`, `cl_enable_text_chat 1`. Filter check-box names: `chat`, `joinleave`,
`namechange`, `servermsg`, `teamchange`, `Console`. `[sdk H][pc M]`

12.4 Message types on the wire: `SayText` (client index, text, want-chat flag; GMod adds the team-chat flag and the dead
flag -> `*DEAD*` / `(TEAM)` prefixes come from Lua `OnPlayerChat`), `TextMsg` (destination 1 notify, 2 console, 3 chat,
4 center print, 5 hud printtalk; up to 5 `%s` parameters, localised). `[pc H][sdk H]`

## 13. Death notices, target ID, pickup history, notifications (C++ hands off to base-gamemode Lua)

13.1 The C++ side has no death-notice renderer in GMod (only the stub string "GetDeathNoticeName not implemented on client yet");
`GM:HUDPaint` runs `HUDDrawTargetID`, `HUDDrawPickupHistory`, `DrawDeathNotice(0.85, 0.04)` in that order. `[pc H][lua H]`

13.2 Death notices (`cl_deathnotice.lua`): convar `hud_deathnotice_time` 6 s; anchored at x = `0.85*ScrW`, y = `0.04*ScrH`,
one row per notice downwards, kill icon from `killicon` (font `HL2MPTypeDeath`, colour 255 80 0, per-weapon glyph and height
scale: prop_physics `9` 0.52, smg1 `/` 0.55, 357 `.` 0.55, ar2 `2` 0.6, crossbow_bolt `1` 0.5, shotgun `0` 0.45, rpg_missile `3`
0.35, npc_grenade_frag `4` 0.56, pistol `-` 0.52, prop_combine_ball `8` 0.5, grenade_ar2 `7` 0.35, stunstick `!` 0.6,
npc_satchel `*` 0.53, crowbar `6` 0.45, physcannon `,` 0.55; aliases tripmine->satchel, HL1 weapons -> HL2 equivalents),
attacker name right-aligned 16 px left of the icon and victim 16 px right of it in `ChatFont`, team colours for players,
NPC enemy 250 50 50 / friendly 50 200 50; alpha = `clamp((hud_deathnotice_time - age) * 255, 0, 255)` i.e. fades over the last second. `[lua H]`

13.3 Target ID (`cl_targetid.lua`): eye trace; if the hit entity is a player (or has `GetPlayerName`/nick), draw the name
in font `TargetID` at (`ScrW/2`, `ScrH/2 + 30`) centred, with two shadows (`+1,+1` 0 0 0 120 and `+2,+2` 0 0 0 50) and the
team colour (`GM:GetTeamColor`, default 255 255 100 for unassigned), then `Health() .. "%"` in `TargetIDSmall` `h+5` below. `[lua H]`

13.4 Pickup history (`cl_hudpickup.lua`; the C++ `ItemPickup` usermsg and `HudHistoryResource` panel exist but GMod draws
it in Lua; `HudHistoryResource` layout: `r252`, 40, 248x320, `history_gap` 56, icon inset 28, text inset 26, `HudNumbersSmall`):
hold 5 s, fade-in 0.04 and fade-out 0.3 as *fractions of hold*; alpha from `delta = (start + hold - now) / hold`: `delta > 1-fadein`
-> `(1-delta)*255/fadein`, `delta < fadeout` -> `delta*255/fadeout`, else 255; rows 300 wide (`PickupHistoryWide`), coloured
left part (weapon 255 200 50, ammo 180 255 180, item 180 200 255) made of `gui/corner8` corners (8 px) + rects, grey body
`230*colordelta` with `colordelta = clamp(delta, 0.6, 0.7)` (so 138..161 grey); text in
`DermaDefaultBold`, white with a 0 0 0 a/2 shadow at +1,+1, name at `x + rowH + 8`, amount right-aligned at `x + 300`. `[lua H][res H]`

13.5 Notifications (`includes/modules/notification.lua`, `notification.AddLegacy(text, type, length)`): types 0 generic,
1 error, 2 undo, 3 hint, 4 cleanup with materials `vgui/notices/{generic,error,undo,hint,cleanup}`; font `GModNotify`
(2.3), icon `ceil(textH*1.25)`; panel background 20 20 20 at alpha 153 (0.6), label white with 1 px shadow 0 0 0 200,
`DockPadding 3`. Spring motion each frame with `dt = RealFrameTime()`: ideal x = `ScrW - w - 0.015*ScrW` (minus `0.04*ScrW`
in the last 0.7 s, plus `2*w` off-screen right in the last 0.2 s), ideal y = `0.8*ScrH - h - stackedHeightAbove`;
`vel += (ideal - pos) * dt * 15`, `vel *= (0.95 - dt*8)`, snapped when `|dist| < 2 && |vel| < 0.1`; panels start at
x = `ScrW + 200`, y = `ScrH`. Sandbox legacy events: `LimitHit` -> error 6 s (`#hint.hitXlimit`), `OnUndo` -> undo 2 s,
`OnCleanup` -> cleanup 5 s. `[lua H]`

## 14. Screen fades (`ScreenFade` usermsg -> view effects)

14.1 Wire format: duration and hold time as **unsigned shorts in 1/512 s** (`(1<<9)` scale), flags byte, colour RGBA bytes.
Flags: 1 `FFADE_IN` (fade from colour to clear), 2 `FFADE_OUT` (clear to colour), 4 `FFADE_MODULATE` (multiply blend),
8 `FFADE_STAYOUT` (stay at full after fading out), 16 `FFADE_PURGE` (clear all existing fades first). `[sdk H][pc M]`

14.2 Per-fade alpha at time t since receipt: `[sdk H]`
- OUT: `alpha * clamp(t/duration, 0, 1)` while t < duration; then `alpha` while t < duration+hold (or forever with STAYOUT); then removed.
- IN: `alpha` while t < hold; then `alpha * (1 - (t-hold)/duration)` until 0; then removed.
- Several active fades: colour = average of the RGB of all, alpha = max; MODULATE fades multiply the frame instead of blending over it.
14.3 The fade quad is drawn after the 3D view (and after view-model) and **before** the HUD, so HUD text stays visible over a black fade. `[sdk H][pc M]`

## 15. HUD text messages (`HudMsg` / `HudText`, `HudMessage` panel, game_text)

15.1 Panel `HudMessage`: x 0, y 0, `wide f0` (full width), tall 480; up to **16** simultaneous messages, text up to 2048 chars,
default font token `TextMessageDefault` (= `Default`). `HudMsg` carries channel (0..3 wraps, channel 0 = unslotted),
x,y (floats 0..1 normalised, -1 = centred), colour1, colour2, effect, fade-in, fade-out, hold, fx time, text. `HudText`
names an entry of `scripts/titles.txt` (parsed by the engine: `$position x y`, `$effect`, `$color`, `$color2`, `$fadein`,
`$fadeout`, `$holdtime`, `$fxtime`). GMod clamps hold time to **0..3600 s** and warns in the console when it clamps. `[pc H]`

15.2 Effects: 0 fade in/out (alpha ramps linearly over fade-in, holds, ramps down over fade-out); 1 flicker (same but the
alpha jitters: in Source `effect 1` = fade in with a credits-style flicker); 2 write-out: characters appear one at a time
every `fxtime` seconds in colour2 then settle to colour1 after `fadein` each; total life = `fadein*len + fxtime + hold + fadeout`.
Positions: x,y in [0,1] map to `x*screenW`, `y*screenH`; -1 centres on that axis. `[sdk H][pc H]`

## 16. Credits (`HudCredits`, `CreditsMsg` / `LogoTimeMsg`)

Panel full 640x480, `TextFont Default`, `TextColor 255 255 255 192`. `scripts/credits.txt` parameters with compiled defaults:
`scrolltime` 57, `separation` 5, `fadeintime` 1, `fadeholdtime` 3, `fadeouttime` 2, `nextfadetime` 2, `pausebetweenwaves` 2,
`logotime` 2, logo text default "HALF-LIFE'" (the `'` is the lambda glyph in `ClientTitleFont`). `[pc H][res H]`

## 17. Main menu and loading screen composition (C++/engine side)

17.1 Engine startup/loading art: `materials/console/startup_loading.vtf` and background `materials/console/background01.vtf`
(`background01_widescreen.vtf` when aspect > 4:3), created as `UnlitGeneric` with `$ignorez 1 $nofullbright 1 $nocull 1 $nofog 1
$translucent 1`; `-debugstartupscreen` command line keeps it visible. The background is stretched to the full window,
`chapterbackgrounds.txt` chooses the image per map in HL2 but GMod uses the single console background until the Lua/HTML menu
is up. `[pc H][inf M]`

17.2 GameUI loads `Resource/SourceScheme.res` and `Resource/valve_%language%.txt`; the in-engine loading dialog
(`LoadingDialog`) is replaced visually by GMod's Lua `GM:LoadingScreen`/HTML (`html/loading.html`) which shows the server's
loading URL; during workshop download the engine shows "Downloading ... %s" / "Mounting Addon" / "Extracting" strings from
the dump. `[pc H]`

17.3 The main menu itself (background scenes, buttons, `menu.html`) is Lua/HTML (`lua/menu/*.lua`, `html/menu.html`) and is
outside the C++ dump; the C++ contributes the `GameUI` activation (`gameui_activate`, `gameui_hide`, `gameui_preventescapetoshow`,
`gameui_allowescapetoshow`), the background map and the `vgui/spawnmenu/generating` / `vgui/spawnmenu/broken` spawn-icon
placeholders. `[pc H][inf M]`

## 18. Spawn menu / context menu C++ hooks

- Binds: `+menu` (spawn menu, hold) and `+menu_context` (context menu, hold) are engine binds whose press/release the C++
  routes to Lua `GM:OnSpawnMenuOpen`/`OnSpawnMenuClose` and `GM:OnContextMenuOpen`/`OnContextMenuClose`
  (`cl_spawnmenu.lua`); the menus themselves are Lua/Derma. `[lua H][pc M]`
- C++ spawn-icon panel: renders the model icon, shows `vgui/spawnmenu/generating` while the icon is being rendered and
  `vgui/spawnmenu/broken` when the model fails. `[pc H]`
- Spawnlist persistence: categories are written to `settings/spawnlist/<name>.txt` (`DEFAULT_WRITE_PATH`); a non-string category
  name logs "SaveSpawnmenuCategory: Category '%s' isn't string!!!". `[pc H]`
- The "ContextMenu"/"OpenContextMenu" strings in the dump belong to generic vgui `ListPanel` column menus, not the GMod context menu. `[pc H]`

## 19. Differences from Source SDK 2013 (binary/resources win)

1. `HudAmmo` digits at (40,2) and (102,16) (SDK/HL2 and compiled defaults: (50,2) and (98,16)); the GMod file marks both lines as GMod changes. `[res H][pc H]`
2. `HudWeaponSelection` `BoxGap` 6 and `LargeBoxUnselectedTall` 19 (compiled defaults 12 / 24; SDK HL2 res uses 6 as well). `[res H][pc H]`
3. `HudMessage` uses the GMod-only `f0` width prefix. `[res H]`
4. `HudChat` is hidden (4x4, `visible 0`); chat is GMod's own panel with Lua hooks and the `cl_drawhud` refusal. `[res H][pc H]`
5. `HudMsg` hold time clamped to 0..3600 s with a console warning. `[pc H]`
6. Weapon texture data can come from Lua SWEP tables; missing `weapon`/`weapon_s` glyphs fall back to `selection`; SWEPs draw their own selection box via `DrawWeaponSelection`. `[pc H][lua H]`
7. Every HUD element consults `GM:HUDShouldDraw` by class name; `cl_drawhud 0` hides all. `[pc H]`
8. Death notices, target ID, pickup history, hints and notifications are Lua in GMod (SDK: C++ `CHudDeathNotice`, `CHudHistoryResource`, `CHudHintDisplay`). `[pc H][lua H]`
9. `Black` scheme colour is 46 43 42, `GMod_BG` added. `[res H]`
10. `hud_quickinfo` defaults off (SDK HL2: on) - `[inf M]`.


---

## 20. What could not be derived from the dump (blockers / low confidence)

- The HL2 `HudSecondaryAmmo` layout, the `WeaponPickup` sequence body, `HudTakeDamageHighBehind/Fullscreen` exact lines and `hud_quickinfo` default were taken from the SDK / HL2 content rather than the GMod files (GMod's `HudLayout.res` has no secondary-ammo block; the quick-info panel's convar default was not located in the dump). `[sdk M]`
- The weapon-selection paint routine itself (box placement loop) is not in the dump (selected by strings only); geometry is from the SDK bucket renderer plus GMod's resource values and matches the compiled defaults found in the panel constructor. `[sdk H][pc M]`
- The `ScreenFade` client reader is not in the dump; wire units and blend rules are SDK (`view_effects`), consistent with the engine strings present. `[sdk H]`
- Main-menu composition (background scenes, HTML menu) and the Derma spawn/context menus are Lua/HTML and were out of the C++ dump; only their C++ hooks are described. `[inf M]`
- Death-notice / target-ID / notification rendering is Lua in GMod; the C++ only exposes `HUDPaint`. Described from the shipped Lua. `[lua H]`
