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
  addons, dupes, saves, demos, options) and the popups are not built yet; fonts are Roboto for
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
