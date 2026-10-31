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
- Fonts: the client ships no Arial/Tahoma/Verdana (system fonts; the game asks the OS, and on
  Linux its `derma/init.lua` uses DejaVu Sans 14 instead of Tahoma 13). `font_file` looks a
  family up in `resource/fonts` first, then in the system font directories (Windows\Fonts,
  fontconfig dirs, macOS Library/Fonts) with the fontconfig substitutions the game gets on
  Linux (Tahoma / Verdana / Trebuchet MS -> DejaVu Sans, Arial / Helvetica -> Liberation Sans);
  Roboto from the content is the last resort. Nothing is bundled in the repo.

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

## Context menu (`contextmenu.lua`, `properties.lua`, `dmenu.lua`)

- Holding C shows the cursor; the active tool's ControlPanel moves into a 320 wide
  DCategoryList at `(ScrW - 320 - 50, ScrH - 50 - tall)`, `tall = min(panel + 10, ScrH * 0.8)`
  (CategoryList.Outer, the DForm header kept). Opening the spawn menu closes it.
- Right click on an entity: `properties.OpenEntityMenu` builds a DMenu of the `properties.Add`
  entries that `Filter` accepts, ordered by `Order` (persist 400, edit bones 500, bodygroups 600,
  skin 601, keep upright 900, ignite/extinguish 999, remove 1000, gravity 1001, drive 1100,
  collisions 1500, statue 1501, biggify 1799 / smallify 1800, motion sensor 2500, edit
  properties 90001); labels `#<MenuLabel>` from `resource/localization/en/context.properties`,
  icons icon16 (`MenuIcon`). DMenu: `MenuBG`, min width 100, options 22 tall with text inset 32
  (icon at 4), width = widest `32 + text + 30`, hover `MenuBG_Hover`, text Button.Normal 82 /
  hover 46 114 178. Not yet: skins as a `Skin` submenu (we list `skin N` flat), submenu arrows.

## Main menu (`html/menu.html`, `template/main.html`, `css/menu/*.css`, `lua/menu/*.lua`)

- The game's menu is a CEF page; we lay its CSS out natively. `DIV.page` fills the screen
  above the 50 px NavBar; `DIV.options` at left 50 / top 50, `UL` margin-left 20 padding 2,
  `LI` padding 0 16 margin-bottom 1: text x = 86. Entries 23 px bold white, letter-spacing -1,
  text-shadow 2px 2px 1px black, hover #ffa, active #ff5; at <= 1280 px wide 16 px (width 250).
  Order: gamemode `logo.png` (288x128), blank, [Resume Game, blank when in game], Start New
  Game, Find Multiplayer Game, blank, Addons, Dupes, Saves, Demos, blank, Options, blank,
  [Disconnect], Quit; strings from `resource/localization/en/main_menu.properties`.
- NavBar: bottom 50 px rgba(0,0,0,.5); buttons 40 tall white radius 4, margin-top 5, right 3,
  padding-left 35, icon at 5,5, span padding 10, text #333 12 px, hover glow; left group
  (Back to main menu, 5 px in), centre group (Disconnect, Back to Game, favourite) flex-filled
  with 10 % margins packed to its end, right group (Problems, Kinect, Games, language flag,
  gamemode icon24 + title). `html/img/*.png` are raw PNG paths (not under materials/).
- `background.lua`: out of game only, a random `backgrounds/*.jpg` scaled to ScrH (plus the
  aspect gap), AngleVel -5/30 deg/s, SizeVel 0.3/30, 30 s per image, the outgoing one fades at
  255/s; `html/img/gradient.png` (512x64) is drawn 1024 x ScrH on the left always.
- Ours: Escape over the game opens the in-game variant (cursor released), Resume / Back to Game
  / Escape return, Quit and Disconnect exit the client; the other pages (new game, servers,
  addons, dupes, saves, demos, options) and the popups are not built yet; fonts are Arial (system) for
  Helvetica/Arial.

## Options dialog (`garrysmod/resource/OptionsSub*.res`, `sourceengine/resource/*.res`)

The "Options" entry opens the engine's VGUI PropertyDialog (`#GameUI_Options` = "OPTIONS",
tabs Keyboard / Mouse / Audio / Video / Voice / Multiplayer, OK / Cancel / Apply). The
garrysmod `.res` files override the sourceengine ones; positions are `xpos,ypos wide x tall`
in the 640x480 proportional space of the sheet page.

- Video (garrysmod): Label1 Resolution 36,24 160x24; Resolution ComboBox 36,50 160x24;
  Label4 DisplayMode 254,24; DisplayModeCombo 254,50 182x24 (Full screen / Run in a window);
  AspectRatioLabel 36,100; AspectRatio ComboBox 36,126 160x24 (Normal (4:3) / Widescreen 16:9
  / Widescreen 16:10); Gamma label 257,117 140x24; GammaButton 254,126 182x24 "Adjust
  brightness levels..."; FPSLabel 36,176 168x24 `#GameUI_FPS`; FPSLimitSlider CCvarSlider
  36,196 168x50 `fps_max`; FPSLimitLabel TextEntry 207,196 36x24; AdvancedButton 254,196
  182x24 "Advanced..."; BenchmarkButton 39,230 130x24; ThirdPartyVideoCredits URLButton
  254,254 225x24.
- Keyboard: listpanel_keybindlist SectionedListPanel 8,10 480x258 (rows from
  `sourceengine/scripts/kb_act.lst`: Movement +forward/+back/+moveleft/+moveright/+speed/
  +jump/+duck; Combat +attack/+attack2/+reload/+use/phys_swap/impulse 100/+zoom/impulse 50/
  slot1-6/invnext/invprev/lastinv; Miscellaneous closecaption/jpeg/save quick/load quick/
  pause/quit/toggleconsole; Misc keyboard keys +walk/+left/+right/+moveup/+movedown/+lookup/
  +lookdown/+strafe/+klook; "blank" rows with "====" are section dividers); Defaults 8,276
  134x24 "Use Defaults"; KeyAdvancedButton 148,276 111x24 "Advanced..."; ChangeKeyButton
  272,276 106x24 "Edit key"; ClearKeyButton 384,276 105x24 "Clear Key". Defaults from
  `kb_def.lst`: w/UPARROW +forward, s/DOWNARROW +back, a/, +moveleft, d/. +moveright, SPACE
  +jump, CTRL +duck, TAB +showscores, e +use, v/' +moveup, c impulse 50, / +movedown, r
  +reload, ALT +walk, SHIFT +speed, MOUSE1/ENTER +attack, MOUSE2/\ +attack2, f impulse 100,
  1-6 slotN, MWHEELUP/[ invprev, MWHEELDOWN/] invnext, q lastinv, F5 jpeg, F6/F7 save/load
  quick, F10 quit prompt, PAUSE pause, ESCAPE escape, ~/` toggleconsole, t impulse 201, y
  messagemode, u messagemode2, g dropprimary, z +zoom.
- Mouse (sourceengine): ReverseMouse CCvarNegateCheckButton 28,9 150x28; MouseFilter 200,10
  150x28; Label3 "Mouse sensitivity" 35,40 148x24; Slider CCvarSlider 200,40 200x40;
  SensitivityLabel TextEntry 400,40 50x24; MouseRaw 30,70 150x24; MouseAccelerationCheckbox
  30,100 150x24; MouseAccelerationSlider 200,100 200x40; MouseAccelerationLabel 400,100
  50x24; Divider1 35,140 396x2; Joystick 28,148 226x28; ReverseJoystick 62,176 296x28;
  JoystickSouthpaw 62,202 296x28; joystick yaw/pitch sliders 232,237 / 232,274 186x40.
- Audio (garrysmod): "sfx label" Game volume 36,19; SFXSlider 36,38 393x50; "gmod sfx label"
  `#GameUI_SoundFXVolume` 36,68; GModSFXSlider 36,88 175x50; MusicVolumeLabel 254,68;
  MusicSlider 254,88 175x50; SpeakerConfigLabel 36,120 184x24; SpeakerSetup ComboBox 36,145
  175x24 (Headphones / 2 / 4 / 5.1 / 7.1 Speakers); SoundQualityLabel 254,120; SoundQuality
  254,145 175x24 (Low / Medium / High); AudioSpokenLanguageLabel 36,176; AudioSpokenLanguage
  36,200 175x24; RequiresRestartLabel 36,220; Label2 Captioning 254,176; CloseCaptionCheck
  254,200 175x24 (Disabled / Subtitles (dialog only) / Closed Captions); snd_mute_losefocus
  33,240 430x28; dsp_enhance_stereo 33,265 210x28; ThirdPartySoundCredits 254,270 200x24.
- Multiplayer (garrysmod): Label2 Spraypaint image 32,14 150x24; LogoImage 32,38 64x64;
  SpraypaintList 106,38 124x24; ImportSprayImage 106,78 124x24; Label3 server note 246,38
  214x40; DisableSprays 240,74 250x28; DownloadFilterLabel 32,105 260x36; Divider2 32,110
  432x2; DownloadFilterCheck 32,155 300x24; Divider3 32,190 432x2; HTMLLoadingScreen 25,195
  250x22; FastSwitchCheckbox 25,220 250x22 `hud_fastswitch`; CrosshairSettings 32,245 250x24
  `crosshair_setup`; FOVSliderLabel 296,205 176x24; FovSliderSlider 296,225 176x50
  `fov_desired`; Advanced 32,260 144x24.
- Voice (garrysmod): voice_modenable 33,32 216x28; Microphonelabel 42,70 180x24; mic volume
  panel 40,94 160x26; ReceiveLabel 246,70; VoiceReceive CCvarSlider 246,94 160x42; MicMeter
  40,148 158x32; TestMicrophone 40,176 160x24; SteamVoiceSettings 246,176 160x24; MicBoost
  33,212 216x28.
- SourceScheme (platform `sourceschemebase.res`, overridden by `garrysmod/resource/
  SourceScheme.res`): colours White 255, OffWhite 221, DullWhite 190, Orange 255 155 0,
  TransparentBlack 0 0 0 128, GMod_BG 108 111 114; Border.Bright 200 200 200 255, Border.Dark
  40 40 40 198, Button.TextColor 82 82 82, Button.BgColor 227 227 227, Button.ArmedTextColor
  46 114 178, Button.ArmedBgColor 240 240 240, Button.DepressedBgColor 84 178 245 (GMod);
  base: Frame.BgColor 160 160 160 128, Frame.TitleTextInsetX 16, Frame.ClientInsetX 8,
  Label.TextColor OffWhite, CheckButton.Check White, CheckButton.BgColor TransparentBlack,
  ComboBoxButton.ArrowColor DullWhite, ListPanel.BgColor TransparentBlack, selected Orange,
  SectionedListPanel text DullWhite / header White / divider Black, Slider.NobColor 108,
  Slider.TrackColor 31, Slider.TextColor 180, TextEntry bg TransparentBlack text OffWhite,
  Menu.BgColor 160 160 160 64 armed Orange, PropertySheet text OffWhite / selected White,
  transition 0.25 s; borders: Depressed (dark left/top, bright right/bottom), Raised (the
  reverse), inset 0 0 1 1. Fonts: Default Verdana 16 w500, DefaultBold Verdana Bold 16
  w1000, DefaultSmall 13, DefaultVerySmall 12, DefaultLarge 18, Marlett 14 (arrows/checks).
- GMod `SourceScheme.res` overrides (the light dialog look): Frame.BgColor GMod_BG_Opaque
  108 111 114 250 (out of focus 97 100 102 240), Frame.ClientInsetY 6, FrameTitleBar font
  UiBold/DefaultLarge, title text 255 255 255 204, Label.BgColor GMod_BG (transparent),
  CheckButton.Check Black, disabled bg 180, ComboBoxButton.ArrowColor 81 (armed 110) on a
  white-blank bg, ListPanel bg White text Black selected 0 168 255 204 (out of focus 132 183
  241 100), Menu bg 233 text 80 armed 132 183 241 / White; DefaultSmall is Verdana 16 w0 in
  GMod; HUDNumber Trebuchet MS 40-45 w900, ConsoleText Verdana 14.
- GMod scheme, controls: PropertySheet text 10 10 10 200 / selected 255, ScrollBar.Wide 15
  (button fg 60 bg 207, slider fg 220 bg 184 184 184 229), Slider nob 220 track 184 184 184
  229 text 180, TextEntry bg White text 10 10 10 selected 0 168 255 204 focus edge 0 0 0 196,
  SectionedListPanel bg White header 50 50 50 divider 0 0 0 150 text 10 10 10 selected
  0 168 255 204, Tooltip bg 249 238 181 text 110 102 60.
- Ours: `Options` in the main menu opens the dialog natively (512x406 centred, GMod scheme,
  six pages at the `.res` positions). Settings live in `~/.gmod-rust/cfg/config.cfg` in the
  game's syntax (`bind "key" "cmd"`, `cvar "value"`), defaults from the mounted
  `cfg/config_default.cfg` + `scripts/kb_def.lst`; every key the client reads goes through
  the binds (Source key names), so the Keyboard page's Edit key / Clear Key / Use Defaults
  are live, as are sensitivity / reverse mouse, volume (mixer master), fov_desired,
  fps_max (frame limiter), resolution / display mode (window), hud_fastswitch is stored.
  Not done: editable TextEntry values, gamma / advanced video / crosshair / spray dialogs,
  voice meters, sound quality / speakers / captions have no effect, the scheme font is Tahoma (Windows) / Verdana 13 from the system.

## New Game page (`template/newgame.html`, `css/menu/NewGame.css`, `lua/menu/getmaps.lua`)

- `.maplist`: white, left 16 / top 16 / right 256 / bottom 16, padding 10. `.controls` 190
  wide: category rows 26 tall, bg #ddd (hover #ccc, active #9cf), 11 px bold #555 (#333
  active) at 6,6, count badge white / #aaa 10 px (padding 2 5, margin 5), gap 5; search input
  32 tall at the bottom. `.scrollable` at left 200: header `LI:first-child` padding 5, 32 px
  #444 + `small` 14 px #999 "N maps"; map cells 128 px thumbnails (`asset://mapimage/<map>`
  = `maps/thumb/<map>.png`, `noicon.png` fallback), li 128 wide + padding 6 + margin 2, name
  10 px centred, selected bg #97c5ff, favourite toggle 16 px at top 8 / right 8 on hover
  (`img/favourite*.png`).
- `gamesettings`: white, right 16 / top 16 / width 226 / bottom 16. Max players dropdown at
  margin 10: 28 tall green (#5C5 to #22bd22), bold white, caret; list 1/2/4/8/16/32/64/128
  (`maxplayers_N`), items padding 4 15 at 13 px, hover #acf. `.scrollable` top 32 bottom 80
  margin 10: labels 11 px #666; MaxPlayers > 1 shows Server Name (text), Local Server,
  Peer-To-Peer (+ friends only); then the gamemode `settings` of `sandbox.txt` (Text /
  Numeric 44 px right-floated input / CheckBox; `singleplayer 1` ones always). Start Game:
  `.btn-primary` #08C to #05C, 20 px white, padding 10, margin 10, left/right 8.
- `getmaps.lua`: `maps/*.bsp` of all mounts, `background*` / `devtest` / `sdk_` / `test_` /
  `vst_` and a few names ignored, category by full name or `prefix_` (`gm_` = Sandbox, `cs_`
  ... = Counter-Strike, `d1_`.. = Half-Life 2, ...), else Other; order Favourites, Sandbox,
  then by name. `StartGame`: `disconnect`, `maxplayers N`, the settings as console
  commands, `map <name>`.
- Ours: the page lives in `newgame.rs`; favourites in `~/.gmod-rust/cfg/favmaps.txt`; Start
  Game writes `~/.gmod-rust/cfg/server.cfg` (the commands the game would run) and relaunches
  the client on the map; the client's own server applies `server.cfg` through
  `Server::set_cvar` at start. maxplayers / hostname / sv_lan / p2p are stored only.

## Servers page (`template/servers.html`, `css/menu/Servers.css`, `js/menu/control.Servers.js`)

- `.options` column (same 23/16 px style as main.html): `h2` Server List, small lines
  "N server(s) total" / "N gamemode(s) total", links Internet / Favorites / History / Local
  Network (`servers_internet` / `servers_favorites` / `servers_history` / `servers_local`,
  active #ff5), Refresh Listings / Stop Refreshing, Legacy Browser, then the filters
  (`addons.sort_by` radio players / servers / name, `addons.filter_by` category checkboxes,
  search; with a gamemode picked: player min-max, max ping, not full / has players / no
  password / outdated).
- `.innerpage` (left 320, top 16, right 16): `H1.menuheader` white with 2 px shadow
  "Gamemodes" + small 50 % #888 "Pick a gamemode from the list below"; `.server_gamemodes
  .whiterounded` with `.gamemode` tiles: #efefef, 34 tall, padding 5 20, radius 3, icon24 at
  5,9, name 20 px bold at left 40 / top 5, stats 11 px #555 bottom 3 / left 43 ("N players on
  M servers"), hover yellow gradient (250 230 150 to 250 250 150). Favorites adds
  `.add_fav_server`: header `servers_find_title`, input placeholder `127.0.0.1:27015`, button
  `servers_find`, found servers list rows.
- With a gamemode: header = gamemode name + `join_a_server`, search box + "Return to
  gamemodes" link, `.serverlist`: white padding 10 radius 4; `.header` #2387ed radius 4 4 0 0
  padding 4 at 8/8/8, 11 px; rows `.server` 16 tall + padding 3 4 (22 px pitch), #E7F0FF /
  odd #F1F6FF, hover #B7DDFD, text #444; columns name (flex, 13 px bold at >= 1280) with
  favourite star / flag / password icon, map 90 (130 at >= 1280), players 48 ("a / b"),
  ping 32, rank 64 (`img/rank1..5.png`); double click joins (`servers_join_server`).
- Ours (`servers.rs`): Internet asks the master in `~/.gmod-rust/cfg/master.txt`
  (`MasterMsg::List` pages, then a `Query` per server for the ping), Local Network broadcasts
  a `Query` to port 27015, Favorites / History are `cfg/servers_{favorites,history}.txt`;
  "Find servers at given address" is a direct `Query` → `Info` (name, map, players, max,
  RTT). Gamemode tiles group the current list (all our servers are `sandbox`); a server row
  clicked twice or Join Server relaunches the client with `--connect host:port` and puts
  the server at the top of History. No sort / filter column controls, no flags / rank /
  password icons yet (our `ServerInfo` has none of those).

## Saves / Dupes / Demos pages (`template/{saves,dupes,demos}.html`, `css/Workshop.css`, `js/menu/WorkshopFiles.js`, `lua/menu/menu_{save,dupe,demo}.lua`)

- Options column: `h2` Saves / Dupes / Demos, then `MyCategories` (local = "My Saves",
  subscribed_ugc), `Categories` (trending, popular, latest), `CategoriesSecondary`
  (followed, favorite, friends, mine), each group followed by a blank line, then "Open
  ... Folder"; labels `saves.<cat>`, subtitles `saves.<cat>.subtitle`.
- `div.ugc_content`: left 320 (200 at <= 1024), top / bottom / right 16, padding 16 (top 0);
  `H1.menuheader` + small; `workshopcontainer` at top 64: white, radius 3, padding 8.
- Grid (`RefreshDimensions`): w = max(180, container width), h = max(180, height - 48),
  iconswide = floor(w / 180) <= 6, iconstall = floor(h / 180) <= 4, PerPage = wide * tall,
  IconWidth = floor(w / wide) - 26, IconHeight = floor(h / tall) - 26, IconMax = max + 1 (the
  preview is an IconMax square centred in the icon, overflow hidden); `workshopicon` margin
  10, bg #666, 1 px shadow; `name` bar white .99 14 px bold letter-spacing -1 (label padding
  2 0 4 2, ellipsis); `author` / `votes` 11 px bold bottom corners (slide out on hover);
  `description` overlay rgba(70,180,250,.95) 11 px white padding 32 8 on hover; `controls`
  slide up on hover (bottom -32 -> 0, 100 ms): `control` #F5FF80 11 px padding 2 4 radius 2
  height 15 shadow 1 1 2 black, hover white, disabled #bfbfbf; local files: publish / delete
  left, load (saves) / spawn (dupes) / play (demos) right.
- `workshopmessage` 48 px bolder #999 centred margin 64 (`saves.none` "No saves found!",
  `saves.loading`); `pagination` bottom 4 white radius 4 padding 4 max 500 wide, back / next
  48x36 #2387ed (hover #4f9ded) with `img/arrow_left.png` / `arrow_right.png`, page dots
  `img/pagination_page.png` (opacity .5, active 1), `pagenum` 14 px.
- `menu_save.lua`: `saves/*.gms` newest first, preview `saves/<name>.jpg`, description
  `saves.local_description`; load = `gm_load <file>` (dupes: `dupes/*.dupe`, demos:
  `demos/*.dem`, `playdemo`).
- Ours (`ugc.rs`): files under `~/.gmod-rust/{saves,dupes}/*.rdup` (our JSON save format) and
  `demos/*.dem` (nothing writes demos yet), preview `<name>.png` beside the file (absolute
  paths load through `material::load_texture`); load sends the in-process server's console
  `load <name>` (the local player is an admin there, `saves_dir` = `~/.gmod-rust/saves`);
  delete removes the file and its preview; Workshop categories show a message; no saving
  from the menu yet (the game saves from the spawnmenu Saves tab).

## Addons page (`template/addon_list.html`, `css/Workshop.css`, `js/menu/control.Addons.js`)

- Options column: Subscribed (with the `AddonTypes` sub-list gamemode / map / weapon / tool /
  npc / entity / effects / model / vehicle and the "Search Subscriptions" box), then
  trending / popular / latest, then followed / favorite / friends / mine / downloaded_ugc,
  each with the type sub-list when active, then "Open Workshop". Header = `addons.<cat>` or
  `addons.<tag>` + subtitle.
- Same `ugc_content` / `workshopcontainer` / grid as Saves; `workshopicon` extras: `size`
  badge top-right under the name (`GetNiceSize`), a checkbox at the top right in Subscribed,
  `.installed` bars #d2fc7b, `.disabled` bars #cedcb1, missing preview `img/downloading.png`;
  controls right: disable / enable (`addon.disable` / `addon.enable`), install / uninstall
  (`addon.subscribe` = "install", `addon.unsubscribe` = "uninstall"), mark unused in
  Downloaded; a settings button opens filter (enabled / disabled only) + sort (title, size,
  updated, subscribed) + select all / page, enable all / disable all.
- Ours (`addons.rs`): Subscribed = packages under `~/.gmod-rust/addons` (`.gmr` or
  directories with `manifest.toml`), disabled ids in `cfg/addons_disabled.txt`; the
  in-process server loads the enabled ones at start. Trending / Top Rated / Latest list
  the registry in `cfg/registry.txt` (`Registry::open_any`, index read on a thread; the three
  are the same index in different orders - there are no ratings), install =
  `Registry::install` into the addons folder (sha + signature checked), uninstall removes
  the file; Downloaded = index entries that are installed. Type tags are drawn but do not
  filter (our manifests have no type); the settings panel, checkboxes and bulk actions are
  not done.

## Advanced video and Gamma dialogs (`OptionsSubVideoAdvancedDlg.res`, `optionssubvideogammadlg.res`)

- Advanced (482 wide; the game places OK / Cancel at the bottom in code): combos 132 x 24 in
  three columns at x 22 / 176 / 330 with labels 24 above: Model detail (`r_rootlod` 2/1/0 =
  Low/Medium/High), Texture detail (`mat_picmip` 2/1/0/-1 = Low..Very High), Shader detail
  (row 58); Water detail (`r_waterforceexpensive` + `r_waterforcereflectentities` = Simple /
  Reflect World / Reflect All), Shadow detail (`r_shadowrendertotexture`), Color Correction
  (`mat_colorcorrection`) (row 118); divider 160; Antialiasing (`mat_antialias` 1/2/4/8),
  Filtering (`mat_trilinear` + `mat_forceaniso` 2..16), Wait for vertical sync (`mat_vsync`)
  (row 196); Motion Blur (`mat_motion_blur_enabled`), FOV slider 176,252 140x50
  (`fov_desired` 75..100), Multicore Rendering (`mat_queue_mode` -1/0) (row 258); HDR
  (`mat_hdr_level` 0/1/2 = None / Bloom / Full) at 22,316 with the Bloom check 170,316
  292x24; divider 352; "Software DirectX level" 360 + "Hardware DirectX level" combo 330,385;
  "Windows Aero extensions" combo 330,410. Titles `GameUI_VideoAdvanced_Title` ("VIDEO -
  ADVANCED"), `GameUI_AdjustGamma_Title`. `GameUI_VeryHigh` / `GameUI_Simple` /
  `GameUI_Reflect_*` tokens are not in the shipped `gameui_english.txt`.
- Gamma (290 x 396): `GameUI_GammaInfo` label 16,36 264x53 ("Adjust the slider until all
  three lines of text below are visible"), Gamma label 16,88, slider 17,112 200x40
  (`mat_monitorgamma` 1.6..2.6), entry 228,114 44x24, image panel 16,156 256x192, OK 104,356,
  Cancel 192,356.
- Ours: both are modal sub-dialogs of Options (`OptionsState::sub`, Cancel / Esc restores the
  pending copy); the values persist in `config.cfg`; applied: `mat_vsync` (surface present
  mode), `fov_desired`, `mat_monitorgamma` (the world shader raises the output to 2.2 /
  gamma, `Camera::gamma`); the gamma test image is three text lines at 3 / 6 / 10 % grey
  (the game's image is not in the mounted content). Also applied (see render.md): Texture Detail (`mat_picmip` top mips skipped), Filtering
  (`mat_trilinear` / `mat_forceaniso` on the base sampler), HDR (`mat_hdr_level` picks the HDR
  lumps + auto-exposure) and Bloom (`mat_bloomscale` post pass). Antialiasing is MSAA (`mat_antialias`,
  see render.md). Model Detail picks the model LOD
  (`r_rootlod`, render.md). Stored only: water / shadow detail, colour correction, motion
  blur, multicore.
- Keyboard - Advanced (`optionssubkeyboardadvanceddlg.res`): "Fast weapon switch"
  (`hud_fastswitch`) check 20,38 240x24, "Enable developer console" (`con_enable`) 20,66
  244x24, OK 95,100, Cancel 182,100; title `GameUI_KeyboardAdvanced_Title`. Multiplayer
  Advanced (`multiplayeradvanceddialog.res`, 540 x 376, a CPanelListPanel 16,56 480x274 of
  `cfg/settings_default.scr` controls) - GMod ships that `.scr` empty ("isn't needed"), so
  the game's dialog is an empty list; ours leaves the button inert.
- TextEntry fields next to the sliders (sensitivity, acceleration exponent, FPS, FOV, gamma)
  take the keyboard on click (digits and '.', caret), Enter / Tab / a click elsewhere commit
  through the slider's clamp and rounding, Esc drops the text.
- Multiplayer page leftovers: the "Customize crosshair" button (`CrosshairSettings`, no
  crosshair cvars or dialog `.res` in the mounted content), "Import spray" (a file dialog that
  converts an image to `materials/vgui/logos/<name>.vtf`; we draw no sprays) and the Voice
  page's "Test microphone" (needs capture; gm-audio has playback only) stay inert buttons.

## Spawn menu: Weapons / Entities tabs

- Game: `spawnmenu.AddCreationTab` Weapons (`PopulateFromList("Weapon")`) and Entities
  (`"SpawnableEntities"`): a sidebar tree with one node per `Category` (icons from
  `ContentCategoryIcons`: Half-Life 2 `games/16/hl2.png`), 128 px `ContentIcon`s (image
  `entities/<class>.png` inset 3, `gui/ContentIcon-normal` / `-hovered` overlay, DermaDefault
  name with a (0,0,0,200) shadow 9 px above the bottom). Left click runs `gm_giveswep <class>`
  (alive, `Spawnable` or admin; `Give` if not carried, then `SelectWeapon`) or
  `gm_spawnsent <class>` (`SpawnFunction` of the scripted entity or `HitPos + HitNormal * 16`,
  `sbox_maxsents`, undo `SENT` "Undone <PrintName>", cleanup `sents`). The HL2 weapon list is
  `ADD_WEAPON` in `lua/autorun/game_hl2.lua`, names `#weapon_x` from `entities.properties`.
- Ours: `ServerMsg::ContentLists` (reliable, versioned like SpawnLists) carries
  `gm_server::ContentEntry` lists: the HL2 spawnable weapons the server implements (10 of the
  game's 13: no stunstick / bugbait / slam), scripted weapons (`print_name`, category Other),
  scripted entities (`print_name` / `category` from `entity.register`). The client groups by
  localised category, draws the ContentIcons and sends `Command::GiveWeapon` /
  `Command::SpawnSent`, which the server runs as `give_weapon_cmd` / `spawn_sent`. Not done:
  HL2 `SpawnableEntities` (ammo / health items; no item entities on the server), middle-click
  `gm_spawnswep` (no world weapon entities), the right-click copy / Creator tool menu, search.

## Spawn menu: search

- Game (`contentsearch.lua`, `cl_search_models.lua`): the sidebar DTextEntry (placeholder
  `#spawnmenu.search`) refreshes on Enter; `search.GetResults(str, type,
  sbox_search_maxresults = 1024)` runs the providers: models = every `models/**/*.mdl` on the
  GAME path matched case-insensitively as a substring of the path without `models/` / `.mdl`
  (unless the query ends in `.mdl`), up to max / 2, shown as 64 px model icons; entities =
  the Weapon / SpawnableEntities / NPC / Vehicles lists matched by localised name or class, up
  to max / 4, shown as ContentIcons. Results replace the content panel in a DIconLayout flow.
- Ours: `MenuState::search` / `search_focus`; the box takes the keyboard (not the key bound to
  `+menu`, which keeps holding the menu; no key repeats), Enter runs `refresh_search`
  (`Mount::find("models/**/*.mdl")` listed once, 512 models max; Weapons / Entities content by
  name or class, 256 max), Esc clears. Results draw as the 64 px icon grid (icons generated
  like the spawnlists') followed by 128 px ContentIcons. Not done: refresh while typing, the
  right-click menu on results, NPC / vehicle providers.
