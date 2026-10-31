# Client UI (HUD, Derma, spawn menu) - reference facts

Everything here is read from the installed client (`garrysmod/lua`, `sourceengine/`,
`garrysmod/resource`), not measured in-game. Our client reproduces these numbers.

## HUD (done)

- Layout grid: HL2 `hudlayout.res` is a 640x480 layout scaled by `ScrH()/480`.
- Health/suit/ammo panels: `HudHealth` 16,432 102x36, `HudSuit` 140,432 108x36, `HudAmmo`
  right-aligned 44 from the right, 36 tall; fonts `HudNumbers` (HalfLife2 32), `HudNumbersSmall`,
  `HudDefault` (Verdana 12 bold); text 255 235 20, box 0 0 0 76.
- Target ID (`cl_targetid.lua`): `TargetID` Verdana 12 bold + `TargetIDSmall` 11 with two black
  shadows, colour 255 255 100, placed at crosshair + 30 px; health as `Health() .. "%"`.
- Death notices (`cl_deathnotice.lua`): `hud_deathnotice_time` 6 s, fade in the last second, glyphs
  from `HL2MPTypeDeath` (csd.ttf 64, each killicon has its own height scale), `ChatFont` names.
- Pickup history (`cl_hudpickup.lua`): hold 5 s, fade in 0.04, fade out 0.3, `PickupHistoryWide`
  300, weapon colour 255 200 50, item 180 255 180, ammo 180 200 255, `gui/corner8`, `DermaDefaultBold`.
- Notifications (`includes/modules/notification.lua`): `GModNotify` = Arial max(12, ceil(ScreenScaleH 9))
  weight 500, icon = textH*1.25, start at ScrW+200, anchor `ScrH*0.8`, gap `ScrW*0.015`, charge
  left by `ScrW*0.04` when < 0.7 s remain, leave right by 2*w when < 0.2 s; spring
  `vel += (ideal-pos)*dt*15`, `vel *= 0.95 - dt*8`; bg 20 20 20 at 60 %; icons `vgui/notices/*`.
  Sandbox: LimitHit = error 6 s, OnUndo = undo 2 s, OnCleanup = cleanup 5 s.
- Hints (`cl_hints.lua`): one timer per name, once; `OpeningMenu` 30 s, `Annoy1` 5, `Annoy2` 7 at
  load, `OpeningContext` 20 + `EditingSpawnlists` 5 on the first spawn menu, `ContextClick` 20 on the
  first context menu, `PhysgunFreeze`/`PhysgunUse` from the physgun; NOTIFY_HINT 20 s, sound
  `ambient/water/drip{1-4}.wav`.
- Weapon selection: GMod default `hud_fastswitch 0` => HL2 bucket menu. `HudWeaponSelection`:
  ypos 16, SmallBoxSize 32, LargeBoxWide 112, LargeBoxTall 80, BoxGap 8, number at 4,4,
  TextYPos 64, SelectionGrowTime 0.4; fonts `WeaponIcons` HalfLife2 64 additive,
  `HudSelectionNumbers` Verdana 11 bold, `HudSelectionText` Verdana 8 bold; glyphs from
  `scripts/weapon_*.txt` (pistol d, 357 e, smg1 a, ar2 l, shotgun b, crossbow g, crowbar c,
  physcannon/physgun m, rpg i, frag k, bugbait j); buckets: crowbar 0/0, physcannon 0/2,
  physgun 0/5 (gmod_tool 0/100, gmod_camera 5/100 have no script), pistol 1/0, 357 1/1, smg1 2/0,
  ar2 2/1, shotgun 3/0, crossbow 3/1, frag 4/0, rpg 4/1. Scripted weapons (`weapon_base
  cl_init.lua`): `weapons/swep` inset 10, bounce `sin(CurTime*10)*5`, info bubble `gui/speech_lid`
  + Author line. Sounds (`game_sounds_ui.txt`, volume 0.32): `Player.WeaponSelectionOpen/Close`,
  `Player.WeaponSelectionMoveSlot`, `Player.WeaponSelected`, `Player.DenyWeaponSelection`.
- Crosshair: `Crosshairs` HalfLife2 40 additive, glyph `Q` for every HL2 weapon, colour
  ClientScheme `Normal` 255 208 64.
- Fonts: the client ships no Arial/Tahoma/Verdana (Windows system fonts); we fall back to Roboto.

## Derma default skin (`lua/skins/default.lua`)

- Colours: bg 101 100 105, bg_dark 55 57 61, bg_bright 220 220 220, control 120 120 120
  (highlight 150, active 110 150 250, bright 255 200 100, dark 100); PropertySheet/Tab
  170 170 170, TabInactive 140 140 140, TabShadow 0 0 0 170, TabText white, TabTextInactive
  0 0 0 200, fontTab `DermaDefault`; CollapsibleCategory 255 255 255 20, CategoryText white /
  inactive 200, fontCategoryHeader `TabLarge`; TextEntry text 20 20 20, highlight 20 200 250,
  cursor 0 0 100, placeholder 128; NumSliderNotch 0 0 0 100; Menu bg 255 255 255 200, border
  0 0 0 200; Button text white (disabled alpha 55), border 20 20 20, highlight 255 255 255 50,
  shadow 0 0 0 100.
- Textures: GWEN atlas `materials/gwenskin/GModDefault.png` (512x512 PNG). 9-slice
  `Border(x, y, w, h, l, t, r, b)` and plain `Normal(x, y, w, h)` pieces used by the menus:
  Panels.Normal 256,0 63x63 (16) / Bright 320,0 / Dark 256,64 / Highlight 320,64;
  Button 480,0 31x31 (8), Hovered 480,32, Dead 480,64, Down 480,96; Shadow 448,0 31x31 (8);
  Tree 256,128 127x127 (16); Checkbox 464,32 15x15, Checked 448,32; RadioButton 464,64 /
  Checked 448,64; TreePlus 448,96, TreeMinus 464,96; TextBox 0,150 127x21 (4), Focus 0,172,
  Disabled 0,194; MenuBG_Column 128,128 127x63 (24,8,8,8), MenuBG 128,192 127x63 (8),
  MenuBG_Hover 128,256 127x31 (8), MenuBG_Spacer 128,288 127x3, Menu_Strip 0,128 127x21 (8),
  Menu_Check 448,112 15x15; Tab_Control 0,256 127x127 (8); TabT_Active 0,384 63x31 (8),
  TabT_Inactive 128,384, TabB 0/128,416, TabL 64/192,384 31x63, TabR 96/224,384; Tab_Bar
  128,352 127x31 (4); Window.Normal 0,0 127x127 (8,24,8,8), Inactive 128,0; Window.Close
  32,448 31x24 (Hover 64,448, Down 96,448), Maxi +192, Mini +96, Restore row 480;
  Scroller.TrackV 384,208 15x127 (4), ButtonV Normal/Hover/Down/Disabled at 400/416/432/448,208;
  TrackH 384,128 127x15, ButtonH rows 144/160/176/192; Left/Up/Right/Down buttons 15x15 at
  464/480,208.. (2); Selection 384,32 31x31 (4). Paint: PropertySheet = Tab_Control,
  Tab = TabT_Active/Inactive, Frame = Window.Normal, Panel = Panels.Normal, Button = Button*.

## Spawn menu (`gamemodes/sandbox/gamemode/spawnmenu/*.lua`)

- `SpawnMenu` panel fills the screen; `DHorizontalDivider` with margin
  `clamp((ScrW-1024)*spawnmenu_border, 25, 256)` x `clamp((ScrH-768)*0.1, 25, 256)`
  (`spawnmenu_border` 0.1; the clamp keeps 25 px even below 1024x768), DHorizontalDivider
  (default divider 8) with the CreationMenu left and the ToolMenu right, right min 460 when
  ScrW >= 1024; ToolToggle 16x16 image button.
  Opens on +menu (Q held, `hang open` on a quick tap), closes on release; MakePopup, keyboard
  input off until a text entry asks for it.
- Both halves are `DPropertySheet`: padding 8, tab bar docked top with 3 px side margins, tab
  scroller overlap 5, fade 0.1, sheet panel at (padding, 20 + padding); DTab = `DermaDefault`,
  icon16 left, 20 tall (28 when active, DTab PerformLayout), text 6 px in.
- CreationMenu tabs in order (`spawnmenu.AddCreationTab`, sorted by order then name):
  Spawnlists (-10, `icon16/application_view_tile.png`), Weapons (`icon16/gun.png`), Entities
  (`icon16/bricks.png`), NPCs (`icon16/monkey.png`), Vehicles (`icon16/car.png`), Post Process
  (`icon16/picture.png`), Dupes (`icon16/page_copy.png`, DHTML), Saves (`icon16/disk.png`, DHTML).
- Spawnlists tab: `DHorizontalDivider` left 192 (min 192 / right min 400 at >= 1024), divider 6,
  `ContentSidebar` = search box + `DTree`; the tree has `#spawnmenu.category.browse` (icon16/cog)
  with Games/Addons, "Your Spawnlists" nodes from `settings/spawnlist_default/NNN-name.txt`
  (KeyValues `TableToKeyValues { parentid, icon, id, contents { n { type model, model } } }`,
  name = file name without the number prefix). Content = `ContentContainer` (DIconLayout,
  64x64 `SpawnIcon`s, spacing 1 (DIconLayout default), 4 px padding) rendered from the model
  (we render icons ourselves at 64 px, `spawnicon` pose) and `ContentHeader` rows.
- ToolMenu tabs (`cl_spawnmenu.lua`): Main = Tools (`icon16/wrench.png`), Utilities
  (`icon16/page_white_wrench.png`); tool panel = `DCategoryList` of `DCollapsibleCategory`
  (header `TabLarge`-font, 20 tall) with `DCategoryButton`-like rows (`DermaDefault`, 16 tall?)
  and a `ControlPanel` (DForm) for the selected tool on the right/under. Categories in order:
  Constraints (axis, ballsocket, elastic, hydraulic, motor, muscle, pulley, rope, slider, weld,
  winch), Construction (balloon, button, duplicator, dynamite, editentity, emitter, hoverball,
  lamp, light, nocollide, physprop, remover, thruster, wheel), Poser (eyeposer, faceposer,
  finger, inflator), Render (camera, colour, material, paint, trails); names from
  `resource/localization/en/tool.properties` (`tool.<name>.name`), categories from
  `spawnmenu.properties` (`spawnmenu.tools.*`).

## ControlPanel (`controlpanel.lua`, `dform.lua`, stools' `BuildCPanel`)

- ControlPanel is a DForm (DCollapsibleCategory: InnerH header 20 with the tool's name, Inner
  body): items docked top with `DockPadding( 10, 10, 10, 0 )`, spacing 4. `NumSlider` 32 tall:
  label `w / 2.4` (Label.Dark 10 10 10), slider with the notch line (`colNumSliderNotch`
  0 0 0 100) from 8 to w - 15, 15 px knob `Input.Slider.H.Normal`, 45 px value text
  (`colTextEntryText` 20 20 20); `CheckBox` = DCheckBoxLabel 16 tall, 15 px Checkbox piece, label
  9 px after it; `TextEntry` = 100x20 label + DTextEntry at 110 (TextBox / TextBox_Focus).
- Built-in stools' panels (`gm_server::builtin_tool_schema`) follow each `BuildCPanel`: NumSlider
  ranges (weld forcelimit 0..1000; rope forcelimit 0..1000, addlength -500..500, width 0..10;
  axis/ballsocket force/torque 0..50000, hinge friction 0..200; elastic constant 0..4000, damping
  0..50, rdamping 0..1, width 0..20; hydraulic addlength -1000..1000, speed 0..128, width 0..5;
  winch speeds 0..1000, width 0..10; muscle length -1000..1000, period 0..10, width 0..5; pulley
  forcelimit 0..1000, width 0..10; thruster force 1..10000; hoverball speed 0..20, resistance
  0..10, strength 0.1..10; balloon rope 5..1000, force -1000..2000; wheel torque 10..10000,
  forcelimit 0..50000, friction 0..100; dynamite damage 0..500, delay 0..10), KeyBinders as
  numbers KEY_PAD_0..9 = 37..46, RopeSelect/PropSelect/ColorPicker as text or omitted. Defaults
  = the stools' `TOOL.ClientConVar` tables (rope width 2 cable/rope, elastic 500/3/0.01
  stretch-only, hydraulic 100/64 fixed toggle, winch 64/64 keys 44/41, muscle 100/1 fixed,
  thruster 1500 keys 45/42, hoverball 5/5/10 keys 46/43/40, balloon 64/500, wheel 3000/1
  nocollide keys 45/42, button key 37 toggle, dynamite 200/0 remove).
- Labels: `tool.<tool>.<key>`, else the shared `tool.forcelimit` / `torquelimit` / `nocollide` /
  `toggle` / `hingefriction`; our field names map to the game's where they differ
  (`derma::setting_label`).
- Wire: the client sends its current values with every `ToolClick` (`tool <name> <key> <value>`
  pairs); the server applies them to the sandbox's one `ToolSettings` table before the stool runs
  (not per player yet; the measured harnesses send none and keep the sandbox defaults).
