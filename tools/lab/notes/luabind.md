# Garry's Mod `luabind` subsystem: behaviour note

Scope: the C++ side of the Lua binding in the dedicated server (`server_srv.so`) and the shared Lua
library (`lua_shared_srv.so`): Lua state creation, engine-to-Lua hook dispatch, gamemode loading,
metatable scheme, legacy networked variables ("NWVars") and the GMod additions to the entity data
tables, user messages and the GMod client-to-server message envelope, timers, console variables and
commands created from Lua, file/script search paths and the Lua file cache, error handling, and the
complete engine-to-Lua callback (pooled hook name) table.

Source tags used on every fact: `[PC-srv]` pseudo-C of the server binary, `[PC-lsh]` pseudo-C of the
shared Lua library, `[SDK]` Source SDK 2013, `[Lua]` Facepunch `garrysmod` Lua repository,
`[inf]` inference from the above. Confidence: `high` (read directly in code paths and strings),
`med` (read, but a constant or argument could not be resolved), `low` (inferred).

Important input limitation: the supplied archive contained only `server/` (2,897 functions) and
`luashared/` (179 functions). The brief also mentioned `client/`, `engine/` and `vphysics/`
directories; they were absent. Everything about the engine's `net` bit stream, client-side hook
dispatch, client user message decoding and client Lua error relay is therefore documented from the
server side plus the Lua repository only, and flagged as such.

Facts are numbered F1..Fn so they can be referenced from the gap list.

---

## 1. Lua state and realm bootstrap (shared library)

- F1. A realm's Lua state is created with the stock LuaJIT state constructor, then a panic handler
  is installed and a C error-reporter closure is stored as a registry reference; this reference is
  the message handler for every protected call made by the binding (section 8). `[PC-lsh] high`
- F2. FPU precision check at state creation: a known double is pushed and read back; if it does not
  round-trip, the JIT is switched off for the state and the console warns
  `Lua detected bad FPU precision! Prepare for weirdness!`. `[PC-lsh] high`
- F3. The standard libraries are opened with the stock "open all libs" call; afterwards the
  `debug` table has `setlocal`, `setupvalue`, `upvalueid` and `upvaluejoin` set to nil. The
  remaining `debug` functions stay. `[PC-lsh] high`
- F4. A "pooled string" array is built at state creation: an integer-indexed Lua table whose
  entries 1..N are the engine hook/callback names (section 2.4 gives the complete list in order).
  Engine code pushes hook names by index into this table, so the exact strings are fixed by the
  binary, not by Lua. `[PC-lsh] high`
- F5. Each realm interface carries: the state, the error-handler reference, a per-type metatable
  object cache of up to 255 type ids, a "current path" stack used by `include`, the realm type
  (server = 1, client = 2 in the shared library's `IsServer`/`IsClient` realm predicates; the
  error realm strings are `server`, `client`, `menu`), and a per-frame counter block that is
  zeroed each cycle. `[PC-lsh] high` (numeric realm ids `med`)
- F6. Per-frame cycle of a realm interface: zero the per-frame counters, run the stack leak check
  (F75), then run queued "threaded call" completions (results of background jobs such as HTTP or
  async file reads that must be delivered on the main thread). `[PC-lsh] high`
- F7. Server JIT state: the server Lua interface is created with JIT enabled only when the command
  line contains `-withjit`; without it the server runs the interpreter. `[PC-srv] high`
- F8. Server Lua start-up sequence (once per map load, in this order): create the NWVar manager
  (fatal error if one already exists); create the server Lua interface (fatal if one already
  exists); set the server script path id; set realm type server; set globals `VERSION` (number),
  `VERSIONSTR` (string), `BRANCH` (string); register `lua/send.txt` with the file server; create
  the C library tables; create the C class metatables; set `SERVER = true`, `CLIENT = false`;
  create the shared NULL entity object; run `includes/init.lua`; load (not yet start) the active
  gamemode (section 2.2); run every script in `autorun`; mark for client download every script in
  `matproxy`, `postprocess`, `vgui`, `skins`, `autorun`, `autorun/client`; run every script in
  `autorun/server` and `autorun/server/sensorbones`; with `-systemtest` on the command line also
  run `includes/dev_server_test.lua`. Folder runs use the chunk tags `!RELOAD` (autorun),
  `!RELOAD_SV` (autorun/server), `!RELOAD_CL` (client folders) or `!UNKNOWN`. `[PC-srv] high`
- F9. Scripts in a folder are enumerated through the shared finder which sorts the result list
  (std::sort on the find results, i.e. byte-wise ascending by name) before running. `[PC-lsh] high`
- F10. Server Lua shutdown (level shutdown): the global Lua state id is incremented (so completions
  queued by background jobs for the old state are dropped, F58), class metatables are shut down,
  the interface is closed, the NWVar manager is destroyed. `[PC-srv] high`
- F11. `includes/init.lua` load order (Lua side): `util.lua`, `util/sql.lua`, `extensions/net.lua`,
  then modules `baseclass`, `concommand`, `saverestore`, `hook`, `gamemode`, `weapons`,
  `scripted_ents`, `player_manager`, `numpad`, `team`, `undo`, `cleanup`, `duplicator`,
  `constraint`, `construct`, `usermessage`, `list`, `cvars`, `http`, `properties`, `widget`,
  `cookie`, `utf8`, `drive` (+ drive_base, drive_noclip), server-only `ai_task`, `ai_schedule`,
  client-only `draw`, `markup`, `effects`, `halo`, `killicon`, `spawnmenu`, `controlpanel`,
  `presets`, `menubar`, `matproxy`; then `gmsave.lua` and the `extensions/*` files. `[Lua] high`

## 2. Gamemode object and engine-to-Lua hook dispatch

### 2.1 Dispatch mechanics

- F12. Every engine-originated hook travels through the Lua function `hook.Call` captured at
  gamemode-set time: the binding pushes the captured `hook.Call` object, then the hook name (a C
  string or a pooled string by index), then the gamemode table (`GAMEMODE`), then the arguments,
  and calls with `nargs + 2`. Consequence: the hook library's own ordering/short-circuit rules
  apply to engine hooks exactly as to `hook.Run`. `[PC-srv] high`
- F13. Because `hook.Call` is captured once (when the gamemode is set), replacing `hook.Call`
  later in Lua does not change what the engine calls; replacing `hook.Run` never affects engine
  hooks. `[PC-srv] high`
- F14. Preconditions checked on every engine hook: the gamemode object must be a table and the
  captured `hook.Call` must be a function; otherwise the hook is silently skipped and the caller
  gets "not handled" (false/0). `[PC-srv] high`
- F15. Engine hooks must be called from the main thread; otherwise a fatal engine error
  (`... !ThreadInMainThread] <hookname>`) is raised. `[PC-srv] high`
- F16. Return handling variants used by engine callers: (a) no returns; (b) N returns read back
  as Lua objects; (c) "finish bool with default": one return is requested, and only if it is a
  boolean does it override the caller's default. Non-boolean returns leave the default. `[PC-srv] high`
- F17. Lua side of `hook.Call(name, gm, ...)`: iterates the event's hook table with `pairs`
  (hash order, not insertion order); string identifiers call `fn(...)`, object identifiers call
  `fn(obj, ...)` if `IsValid(obj)` else the entry is deleted; the first hook returning a non-nil
  first value ends dispatch and its up to six values are returned; otherwise `gm[name](gm, ...)`
  is called if present. `hook.Run` caches the gamemode table on first use. `[Lua] high`
- F18. Hooks called with the pooled index path confirmed in the server binary and their arguments:
  - `CheckPassword` (index 11): args `(steamid64 as string, ip string, sv_password, client
    password, client name)`; 2 returns requested. If return 2 is boolean true -> accept. Else if
    return 1 is a string -> it is trimmed and becomes the rejection reason (buffer limited by the
    engine's reason size) and the connection is refused. If the hook returned nothing usable the
    engine fallback is `strncmp(sv_password, client password, strlen(sv_password)) == 0`
    (prefix compare, so an empty sv_password accepts everything). Null strings are replaced
    by "". Default reason text `#GameUI_ServerRejectBadPassword`. `[PC-srv] high`
  - `ClientSignOnStateChanged` (index 12): `(userid, old state, new state)` numbers; only called
    when old != new; no returns. `[PC-srv] high`
  - `NetworkIDValidated` (index 60): `(player name, network id string, steamid64 as string)`;
    three args, no returns. `[PC-srv] high`
  - `ShutDown` (index 159): no args; called as the very first action of level shutdown, before
    game systems shut down, entities are cleared and the Lua state is destroyed. `[PC-srv] high`
  - `OnLuaError` (by name, from the server error callback): `(error string, realm string,
    stack table, addon name, addon workshop id)`; the addon fields are nil/0 when the error
    could not be attributed to an addon. `[PC-srv] high` (arg order `med`)
- F19. Hooks whose dispatch lives in game rules, player and entity classes were not in the dump
  (they are called from classes outside this subsystem's slice), but their exact names and the
  fact that they are called by index are fixed by the pooled table (F4, section 2.4). `[inf] high`

### 2.2 Gamemode loading

- F20. Loading a gamemode by name (`LoadGamemode(name, derived?)`): a global table `GM` is
  created with `GM.Folder = "gamemodes/<name>"` and `GM.FolderName = "<name>"`; the gamemode
  folder is added to the Lua search path; `gamemodes/<name>/gamemode/init.lua` is executed under
  the server path id with error text `Couldn't Load Init Script` if it fails; `derived` is read
  from `GM.DerivedFrom` (default `"base"`); then the Lua `gamemode.Register(GM, name, derived)`
  is called; finally the global `GM` is set to nil. `[PC-srv] high`
- F21. Deriving (`DeriveGamemode(name)` called from Lua): the current `GM` is saved,
  `GM.DerivedFrom = name` is set, the parent is loaded with the same routine (which creates and
  registers its own `GM`), then the saved `GM` is restored as the global. `[PC-srv] high`
- F22. Validity test for a gamemode name: `gamemodes/<name>/gamemode/init.lua` exists in the
  server Lua path. `[PC-srv] high`
- F23. Setting the active gamemode (`SetGamemode(name, runStartup)`): requires global table
  `gamemode` and function `gamemode.Get`, else error messages are printed and nothing is set;
  `GAMEMODE = gamemode.Get(name)`; global `GAMEMODE_NAME = name`; the global `hook` table and
  `hook.Call` are captured (error `hook.Call is not a function!` if missing). If `runStartup`:
  SWEP scripts are loaded (weapons manager), SENT scripts (scripted entities manager), effect
  scripts, then the hooks `CreateTeams`, `PreGamemodeLoaded`, `OnGamemodeLoaded`,
  `PostGamemodeLoaded`, `Initialize` are called in that order. `[PC-srv] high`
- F24. `InitPostEntity` is not part of that sequence; it is called later from the game rules at
  "level init post entity" (after all map entities exist), by pooled index 53. `[SDK]+[inf] med`
- F25. `gamemode.Register(t, name, derived)` (Lua): if the current gamemode's `FolderName` equals
  `name`, the table is merged into the current gamemode (hot reload); if the base class folder
  matches it is merged into the base; a missing derived gamemode prints
  `Warning: Couldn't find derived gamemode (<derived>)`. `[Lua] high`

### 2.3 Server frame ordering relevant to Lua

- F26. Server game frame, per tick (non-restore path): (1) purge entity delete list; (2) game
  systems "pre entity think"; (3) game-rules start frame; (4) nav mesh update; (5) NextBot manager
  update; (6) query cache update; (7) run all entity think functions (players simulated here);
  (8) game systems "post entity think" (game rules `Think` lives here in the SDK, so `GM:Think`
  fires after entity thinks); (9) service the map I/O event queue; (10) purge delete list;
  (11) update all client data; (12) game-rules end-of-frame; (13) networked property timed events.
  Every other tick when "simulate alternate ticks" is on, only (11) runs and the frame time of
  the following full tick is doubled. `[PC-srv] high`, step-8 Lua placement `[SDK] med`
- F27. Measured relative order of Lua callbacks within a tick (from the Rust project's srcds
  captures, consistent with F26/F39): `Think` -> timers (simple then named) -> `Tick`. `[inf] med`
- F28. Level init: wind speed reset; GMod level init; particle manifest parsed; with `sv_autosave`
  on, a `logic_autosave` is spawned with `Save` at 1.0 s and `Kill` at 1.1 s; a missing worldspawn
  gets a default (`skyname sky_wasteland02`, `MaxRange 4096`, bounds +-16384). `[PC-srv] high`
- F29. Server activate: fatal-ish error message if the delete queue is non-empty; precache all
  non-dormant entities; game systems "level init post entity" (where `InitPostEntity` fires);
  precache disabled afterwards; nav mesh load warnings; NextBot init. `[PC-srv] high`
- F30. Client lifecycle (server): on disconnect, unless the game is over, the player's
  "connected" player-resource field is cleared, its sound reset, movement flags adjusted
  (ground flag cleared, two internal flags set, solid flags |= not-solid), then game rules
  "client disconnected" (which calls `PlayerDisconnected`), then touch/ground links and the
  simulation list entry are removed. `[PC-srv] high`
- F31. Client settings changed (server): reads the client's `cl_updaterate` (clamped to
  `sv_minupdaterate..sv_maxupdaterate`), `cl_interpolate`, `cl_interp_ratio` (clamped to
  `sv_client_min_interp_ratio..sv_client_max_interp_ratio` unless the min is -1), `cl_interp`;
  lerp time = `max(cl_interp, ratio / updaterate)`; `cl_predict`, `cl_predictweapons`,
  `cl_lagcompensation`; then calls the game rules' settings-changed (Lua `PlayerSettingsChanged`
  is not in this list, there is no such pooled name). `[PC-srv] high`

### 2.4 Complete engine-to-Lua callback name table (pooled strings)

- F32. The pooled table has 315 strings. Indices 0..308 are hook/callback names, 309 is the DT
  receive proxy name `DTVar_ReceiveProxyGL`, 310..314 are the type words `Float`, `Vector`,
  `Angle`, `String`, `Entity` (used to build NW2/DT accessor names). `[PC-lsh] high`
- F33. The table, 0-based index: name (the Lua table is 1-based; engine callers pass index+1 in
  the Lua table, the C side uses the 0-based number):
  0 AcceptInput, 1 AdjustMouseSensitivity, 2 AllowPlayerPickup, 3 CalcMainActivity, 4 CalcView,
  5 CalcViewModelView, 6 CanExitVehicle, 7 CanPlayerEnterVehicle, 8 CanPlayerSuicide, 9 ChatText,
  10 ChatTextChanged, 11 CheckPassword, 12 ClientSignOnStateChanged, 13 CreateClientsideRagdoll,
  14 CreateEntityRagdoll, 15 CreateMove, 16 DoAnimationEvent, 17 DoPlayerDeath, 18 DrawMonitors,
  19 DrawPhysgunBeam, 20 EndEntityDriving, 21 EntityEmitSound, 22 EntityFireBullets,
  23 PostEntityFireBullets, 24 EntityKeyValue, 25 EntityNetworkedVarChanged, 26 EntityRemoved,
  27 EntityTakeDamage, 28 PostEntityTakeDamage, 29 FindUseEntity, 30 FinishChat, 31 FinishMove,
  32 GUIMouseDoublePressed, 33 GUIMousePressed, 34 GUIMouseReleased, 35 GetFallDamage,
  36 GetGameDescription, 37 GetMotionBlurValues, 38 GetPreferredCarryAngles, 39 GetTeamColor,
  40 GravGunOnDropped, 41 GravGunOnPickedUp, 42 GravGunPickupAllowed, 43 GravGunPunt,
  44 HUDAmmoPickedUp, 45 HUDDrawScoreBoard, 46 HUDItemPickedUp, 47 HUDPaint, 48 HUDPaintBackground,
  49 DrawOverlay, 50 CaptureVideo, 51 HUDShouldDraw, 52 HUDWeaponPickedUp, 53 InitPostEntity,
  54 InputMouseApply, 55 KeyPress, 56 KeyRelease, 57 Move, 58 NeedsDepthPass,
  59 NetworkEntityCreated, 60 NetworkIDValidated, 61 NotifyShouldTransmit, 62 OnAchievementAchieved,
  63 OnChatTab, 64 OnCrazyPhysics, 65 OnDamagedByExplosion, 66 OnEntityCreated,
  67 OnEntityWaterLevelChanged, 68 OnNPCKilled, 69 OnNPCDropItem, 70 OnPhysgunFreeze,
  71 OnPhysgunReload, 72 OnPlayerChat, 73 OnPlayerHitGround, 74 PlayerChangedTeam,
  75 OnViewModelChanged, 76 PhysgunDrop, 77 PhysgunPickup, 78 OnPhysgunPickup, 79 PlayerAuthed,
  80 PlayerBindPress, 81 PlayerButtonDown, 82 PlayerButtonUp, 83 PlayerCanHearPlayersVoice,
  84 PlayerCanPickupItem, 85 PlayerCanPickupWeapon, 86 PlayerCanSeePlayersChat, 87 PlayerConnect,
  88 PlayerDeath, 89 PlayerDeathSound, 90 PlayerDeathThink, 91 PlayerDroppedWeapon,
  92 PlayerDisconnected, 93 PlayerDriveAnimate, 94 PlayerEndVoice, 95 PlayerEnteredVehicle,
  96 PlayerFootstep, 97 PlayerHurt, 98 PlayerInitialSpawn, 99 PlayerLeaveVehicle, 100 PlayerNoClip,
  101 PlayerPostThink, 102 PlayerSay, 103 PlayerSelectSpawn, 104 PlayerShouldTakeDamage,
  105 PlayerShouldTaunt, 106 PlayerSilentDeath, 107 PlayerSpawn, 108 PlayerSpray,
  109 PlayerStartTaunt, 110 PlayerStartVoice, 111 PlayerStepSoundTime, 112 PlayerSwitchFlashlight,
  113 PlayerSwitchWeapon, 114 PlayerTick, 115 PlayerTraceAttack, 116 PlayerUse, 117 PostCleanupMap,
  118 PostDraw2DSkyBox, 119 PostDrawEffects, 120 PostDrawHUD, 121 PostDrawOpaqueRenderables,
  122 PostDrawSkyBox, 123 PostDrawTranslucentRenderables, 124 PostDrawViewModel,
  125 PostPlayerDeath, 126 PostPlayerDraw, 127 PostRender, 128 PostRenderVGUI, 129 PreCleanupMap,
  130 PreDrawEffects, 131 PreDrawHUD, 132 PreDrawOpaqueRenderables, 133 PreDrawSkyBox,
  134 PreDrawTranslucentRenderables, 135 PreDrawViewModel, 136 PreDrawViewModels,
  137 PrePlayerDraw, 138 PreRender, 139 PreventScreenClicks, 140 PropBreak, 141 RenderScene,
  142 RenderScreenspaceEffects, 143 Restored, 144 Saved, 145 ScaleNPCDamage, 146 ScalePlayerDamage,
  147 ScoreboardHide, 148 ScoreboardShow, 149 SetupMove, 150 SetupPlayerVisibility,
  151 SetupSkyboxFog, 152 SetupWorldFog, 153 ShouldCollide, 154 ShouldDrawLocalPlayer, 155 ShowHelp,
  156 ShowSpare1, 157 ShowSpare2, 158 ShowTeam, 159 ShutDown, 160 SpawniconGenerated, 161 StartChat,
  162 StartCommand, 163 StartEntityDriving, 164 Think, 165 Tick, 166 TranslateActivity,
  167 UpdateAnimation, 168 VGUIMousePressAllowed, 169 VGUIMousePressed, 170 VehicleMove,
  171 ViewPunch, 172 WeaponEquip, 173 PlayerAmmoChanged, 174 PlayerClassChanged,
  175 PlayerHandleAnimEvent, 176 PlayerFireAnimationEvent, 177 OnPlayerPhysicsPickup,
  178 OnPlayerPhysicsDrop, 179 OnPlayerJump, 180 OnCloseCaptionEmit, 181 HandlePlayerArmorReduction,
  182 OnPauseMenuShow, 183 OnClientLuaError, 184 OnChangeActivity,
  (scripted entity / NextBot / weapon callbacks called on the ENT/SWEP table:)
  185 BehaveUpdate, 186 Blocked, 187 BodyUpdate, 188 Deploy, 189 DoImpactEffect, 190 Draw,
  191 DrawTranslucent, 192 EndTouch, 193 Equip, 194 EquipAmmo, 195 ExpressionFinished,
  196 FireAnimationEvent, 197 FreezeMovement, 198 GetAttackSpread, 199 GetCapabilities,
  200 GetRelationship, 201 GetRenderMesh, 202 HandleAnimEvent, 203 Holster, 204 ImpactTrace,
  205 KeyValue, 206 OnCondition, 207 OnContact, 208 OnIgnite, 209 OnInjured, 210 OnKilled,
  211 OnTraceAttack, 212 OnLandOnGround, 213 OnLeaveGround, 214 OnNavAreaChanged, 215 OnOtherKilled,
  216 OnStuck, 217 OnTakeDamage, 218 OnUnStuck, 219 PassesDamageFilter, 220 PassesFilter,
  221 PassesTriggerFilters, 222 PhysicsCollide, 223 PhysicsSimulate, 224 PhysicsUpdate,
  225 RunEngineTask, 226 StartEngineSchedule, 227 StartEngineTask, 228 StartTouch,
  229 TestCollision, 230 Touch, 231 UpdateTransmitState, 232 Use, 233 IsJumpLegal,
  234 CalcWeaponProficiency, 235 GetShadowCastDirection, 236 OnEntitySight, 237 OnEntitySightLost,
  238 OnMovementFailed, 239 OnMovementComplete, 240 OnActiveWeaponChanged, 241 RunAI,
  242 OnTaskFailed, 243 OverrideMove, 244 OverrideMoveFacing, 245 TranslateSchedule,
  246 TranslateNavGoal, 247 OnStateChange, 248 CustomAmmoDisplay, 249 ResolveCustomFlyCollision,
  250 DrawHUD, 251 DrawHUDBackground, 252 DrawWorldModel, 253 DrawWorldModelTranslucent,
  254 DrawWeaponSelection, 255 DoDrawCrosshair, 256 ShouldDrawViewModel, 257 GetTracerOrigin,
  258 OnDrop, 259 OnRemove, 260 PrimaryAttack, 261 SecondaryAttack, 262 Reload, 263 RenderScreen,
  264 ViewModelDrawn, 265 TranslateFOV, 266 SetWeaponHoldType, 267 CanBePickedUpByNPCs,
  268 NPCShoot_Primary, 269 NPCShoot_Secondary, 270 GetNPCRestTimes, 271 GetNPCBurstSettings,
  272 GetNPCBulletSpread, 273 ShouldDropOnDie,
  (panel callbacks:) 274 ActionSignal, 275 AllowInput, 276 ApplySchemeSettings, 277 ConsoleMessage,
  278 DoClick, 279 OnActivate, 280 OnBeginLoadingDocument, 281 OnCallback, 282 OnChangeAddressBar,
  283 OnChangeTargetURL, 284 OnChangeTitle, 285 OnChildViewCreated, 286 OnCursorEntered,
  287 OnCursorExited, 288 OnCursorMoved, 289 OnDeactivate, 290 OnDeletion, 291 OnDocumentReady,
  292 OnFinishLoadingDocument, 293 OnFocusChanged, 294 OnGetFocus, 295 OnKeyCodePressed,
  296 OnKeyCodeReleased, 297 OnKeyCodeTyped, 298 OnLoseFocus, 299 OnMousePressed,
  300 OnMouseReleased, 301 OnMouseWheeled, 302 OnScreenSizeChanged, 303 OnSizeChanged,
  304 OnTextChanged, 305 OnTextClicked, 306 PerformLayout, 307 ResourceLoaded, 308 TestHover.
  `[PC-lsh] high`
- F34. Names present in the Lua repository but not in this table (e.g. `PlayerSpawnProp`,
  `PlayerSettingsChanged`, `OnReloaded`, `GetDeathNoticeEntityName`) are raised purely from Lua
  (gamemode or library code), never by the engine. `[inf] high`

## 3. Metatables, userdata and classes

- F35. Each bound C++ class is registered as a Lua metatable with a numeric type id (< 255). The
  metatable is created with a typed "new metatable" variant that records the type id; the
  interface keeps one metatable object per type id. `[PC-lsh] high`
- F36. Userdata layout for all engine objects: 8 bytes, `{ pointer-or-handle (4 bytes), type id
  (1 byte) }`. Type checks compare the type byte to the expected id; entities are type id 9,
  Vector 10, Angle 11; Lua primitives use the standard ids (bool 1, number 3, string 4, table 5,
  function 6). `[PC-lsh]+[PC-srv] high`
- F37. Class metatable construction: for a class without a base, `__index = metatable`; all
  registered C functions are set as fields; an optional per-class post-init callback runs.
  For a class with a base: `MetaBaseClass = <base metatable>`, `__index` is a C "derived index"
  function (own metatable, then the `MetaBaseClass` chain); `__gc` and `__newindex` are copied
  from the base when not defined on the class. `[PC-srv] high`
- F38. Entity classes use a distinct C `__index` that consults the entity's own Lua table (the
  table returned by `Entity:GetTable()`) first and then the metatable chain; this is what makes
  `ent.Foo = x` persist on the entity and methods resolve `Weapon -> Entity`, `Player -> Entity`,
  `NPC -> Entity`, `Vehicle -> Entity`, `NextBot -> Entity`. Errors: `Error initializing class %s`
  and `m_metatable isn't table` are fatal at start-up. `[PC-srv] high`, exact lookup order `med`
- F39. Library tables (`timer`, `file`, `net`, `util`, ...) are created as global tables filled
  with the registered C functions at state init, before `includes/init.lua`. `[PC-srv] high`
- F40. Scripted entity factories exist for `CWeaponSWEP`, `CSENT_anim`, `CSENT_point`,
  `CSENT_brush`, `CSENT_AI`, `CSENT_filter` and a Lua NextBot class (names as seen by the entity
  factory; Lua `ENT.Type` selects them: `anim`, `point`, `brush`, `ai`, `filter`, `nextbot`).
  Engine-created entities may be overridden by a SWEP only when the class is on the
  "allowed to override weapon" list, by a SENT only when on the "allowed to override entity"
  list; Lua-created entities try the SWEP manager first, then the SENT manager. `[PC-srv] high`

## 4. Networked variables

### 4.1 Legacy NWVars (`SetNWString` and friends, user message `NetworkedVar`)

- F41. Storage: a per-entity map keyed by variable name (byte-wise ordered); each record holds the
  Lua value object, an optional proxy function object, a "last sent" time (float seconds, -1 on
  creation) and a network string index (-1 until first send). Entity slots are indexed by entity
  index 1..16383; one extra slot (handle -1 / "world") holds the global NWVars. `[PC-srv] high`
- F42. Variable names are interned in the network string table `networkvars` (capacity 4096;
  adding beyond 4095 prints `Too many NWVar names, could not add name %s`). `[PC-srv] high`
- F43. Setting a value: if the value is a string longer than 200 bytes it is truncated to 200.
  If the new value is Lua-equal to the stored one, nothing happens (no resend, no proxy, no
  hook). Otherwise the value is stored, the last-sent time is reset to 0 (send at the next
  cycle), the name is interned if needed, and if a proxy function is set it is called as
  `proxy(entity, name, oldValue, newValue)` (4 args, no returns). Failure to find/create the
  record prints `Setting Networked Var %s Failed!!`. `[PC-srv] high`
- F44. Wire format: user message `NetworkedVar` (variable size): entity handle, type id as an
  8-bit char, variable name as a string (the network string index is converted back to the
  string before writing), then by type: bool -> 1 bit; number -> 32-bit float (NWInt is sent as
  float32); string -> null-terminated string; Entity -> entity handle; Vector -> coord-compressed
  vec3; Angle -> compressed angles. Any other type prints
  `Error: Trying to network unacceptable type (%s)` and sends an empty payload. `[PC-srv] high`
- F45. Byte budget: console variable `lua_networkvar_bytespertick` (integer; default not visible
  in the dump) limits bytes written per cycle. Each var adds to the counter: bool 1, number 4,
  string strlen, entity 4, vector 12, angle 12. When the counter exceeds the limit the remaining
  non-forced sends are deferred to a later cycle (the counter is set to -1 as a "stop" mark).
  Forced sends (new player catch-up) ignore the budget. `[PC-srv] high`
- F46. Resend cadence: a var is (re)sent when `now - base >= lastSent`, i.e. roughly every
  `base` seconds, where `base` is chosen per entity per cycle: `random(0.5, 1.0) * K1` for
  ordinary entities, a constant `K2` for the world/global slot, `0.5` s for a weapon that is its
  owner's active weapon, and a constant `K3` for players. After a send
  `lastSent = now + random(-0.05, 0.05)`. `K1, K2, K3` are data constants that the pseudo-C does
  not resolve. Nil values are never sent (they only stamp `lastSent = now`). `[PC-srv] high`
  for the structure, `med` for the constants (K1/K2/K3 unknown)
- F47. Cycle (runs when `CurTime >= nextCycleTime`; `nextCycleTime = CurTime + K4`, K4 a data
  constant, likely 0.1 s): byte counter reset; (a) global vars broadcast to all players;
  (b) each entity's vars broadcast; (c) for every connected player with a pending "catch-up"
  cursor: walk the entity list from the cursor, send every var of each entity to that player
  only (reliable, forced), stop for this cycle once more than 512 bytes were written for that
  player; the cursor is cleared when the list is exhausted. `[PC-srv] high`, K4 `low`
- F48. Player insert (player fully connected): the catch-up cursor is set to the first entity;
  the global vars and the player's own vars are sent immediately, reliably and forced. `[PC-srv] high`
- F49. `Entity:GetNWVarTable()` style dump: a Lua table `{[0] = {global vars}, [entIndex] =
  {name = value}}` for every entity with index 1..16383 that has vars. `[PC-srv] high`
- F50. Getters: a typed getter returns the stored value only if non-nil and of the expected
  type (number for float/int, type 10 for Vector, 9 for Entity, ...), otherwise the default
  passed by the caller. `[PC-srv] high`
- F51. The `EntityNetworkedVarChanged` hook (index 25) exists in the pooled table; its C caller
  is outside this dump (NW2/DT variables, client side). The legacy NWVar set path above does not
  call it directly; it calls the per-var proxy. `[PC-srv] high`, hook caller `low`

### 4.2 GMod additions to the entity data tables (NW2 / DT variables)

- F52. `DT_BaseEntity` gains, beyond the SDK: `m_OverrideSubMaterials` (32 ints, 4 bytes each),
  `m_GMOD_bool[32]` (1 byte), `m_GMOD_float[32]` (4), `m_GMOD_int[32]` (4),
  `m_GMOD_Vector[32]` (12), `m_GMOD_QAngle[32]` (12), `m_GMOD_EHANDLE[32]` (4). These are the
  backing arrays of `Entity:SetDTBool/Float/Int/Vector/Angle/Entity(slot, value)` and of the
  `NetworkVar` declarations in `ENT:SetupDataTables`; slot range 0..31 per type. `[PC-srv] high`
- F53. `DT_PlayerResource` carries per-player arrays `m_iPing`, `m_iScore`, `m_iDeaths`,
  `m_bConnected`, `m_iTeam`, `m_bAlive`, `m_iHealth`, `m_iArmor` (team, health and armor are the
  GMod additions over the SDK's resource table). `[PC-srv] high`, SDK diff `med`
- F54. `DT_WeaponSWEP` = base combat weapon table + a `m_ScriptedEntity` sub-table + one string
  (the weapon class name). `DT_SENT_anim/point/AI` = base table + `m_ScriptedEntity`.
  `DT_GMOD_Player` adds to `DT_BasePlayer` a set of ints, entity handles, bools, floats and
  vectors (hands entity, view-model related handles, run/walk speeds, player colour vectors);
  field names are not recoverable from strings in this dump. `[PC-srv] high` (names `low`)
- F55. Changes to any `m_GMOD_*` slot mark the entity's network state changed through the usual
  SDK mechanism (per-slot change notification), so they ride the entity snapshot, not a user
  message, and are therefore delta-compressed and PVS-filtered like other entity properties.
  This is the key behavioural difference between NW2/DT vars (snapshot) and legacy NWVars
  (reliable user message, budgeted, resent periodically). `[PC-srv]+[SDK] high`

## 5. User messages and the GMod client-to-server envelope

- F56. Server user message registry: name -> (size, name), case-insensitive ordering; registering
  a name twice is a fatal engine error naming the duplicate; size -1 means
  variable length. The engine queries name and size by index for the string table. Dispatching or
  hooking a user message on the server is a fatal error (client-only). `NetworkedVar` is
  registered with size -1. `[PC-srv] high`
- F57. Client-to-server GMod envelope (received by the server for every "GMod" engine message):
  the first 8 bits are a message type 0..5; a message shorter than 8 bits or with type > 5 is
  dropped with `Blocking invalid GMod packet - Length: %i Type: %i`. Type 4 = Lua file request
  (data pack, client asks for `send.txt` files), handled without a player entity. Type 1 =
  ignored. Types 0 and 2 require a valid player entity: type 2 = client Lua error report (string,
  max 4096 bytes) -> server-side client-error handler -> `OnClientLuaError`; type 0 = `net`
  library message: the remaining bits (length - 8) are exposed as the current incoming reader and
  Lua `net.Incoming(lengthBits, playerEntity)` is called if `net` is a table and `net.Incoming`
  a function. `[PC-srv] high`
- F58. `net.Incoming` (Lua): reads the 16-bit header (`net.ReadHeader`), maps it through
  `util.NetworkIDToString`, subtracts 16 from the length, and calls the receiver registered with
  `net.Receive(name:lower())`. Unknown names are silently dropped. So the total `net` frame on the
  wire inside the engine message is `[8-bit envelope type = 0][16-bit string-table id][payload]`,
  and `net.WriteEntity` writes the entity index with `MAX_EDICT_BITS` (13) bits, 0 for invalid.
  `net.WriteType`/`ReadType` prefix each value with an 8-bit type id (`TYPE_COLOR = 255`), and
  `WriteTable` is a sequence of `WriteType(key) WriteType(value)` pairs terminated by a nil type.
  `[Lua] high`
- F59. Server-to-client `net` framing, `net.Start` limits (the 64 KB message cap, the per-tick
  reliable budget) and the `umsg` encoding live in the engine/client binaries that were not
  supplied; they are not derivable from this dump. `[inf] high` (as a statement of absence)
- F60. `util.AddNetworkString` must happen server-side; ids are string table indices (16-bit),
  not CRCs. `[Lua]+[PC-srv] high`

## 6. Timers (`timer` library, server)

- F61. Two collections: simple (one-shot, `timer.Simple`) timers in a list in creation order, and
  named timers (`timer.Create`) in a map ordered by name bytes. Each cycle runs the simple list
  first, then the named map. `[PC-srv] high`
- F62. Simple timers created during a cycle (including from inside a timer callback) go to a
  pending queue and are moved onto the live list at the start of the next cycle, so a
  `timer.Simple(0, f)` created inside a timer callback runs no earlier than the next cycle;
  a `timer.Simple(0, f)` created in `Think` runs in the same tick's timer pass (it precedes the
  pass). `[PC-srv] high`
- F63. Simple timer fire condition: `due <= CurTime` where `due` is a double stored at creation
  (`CurTime + delay`). All due simple timers fire in list order in one pass; the function
  reference is freed and the entry removed after the call; while it runs, a global "current
  simple timer" marker is set. `[PC-srv] high`
- F64. Named timer record: `due` (double), `delay` (float), `reps` (int, 0 = infinite as created
  by Lua; the C loop erases entries whose `reps` is 0 at visit time only after they have been
  decremented to 0, see below), `paused` flag, `failed` flag and a third flag ("not yet started"
  state used by `timer.Stop`/`Start`). `[PC-srv] high` (third flag semantics `med`)
- F65. Named timer step, per timer per cycle: skip if paused, or flagged not-started, or
  `due > CurTime`. Otherwise: `due = due + delay` (accumulates from the previous due time, not
  from now, so a timer that fell behind catches up one fire per cycle without drifting); if
  `reps > 0` then `reps -= 1`; call the function (protected); on error set `failed`. If after the
  call `reps == 0` or `failed`, free the function reference and erase the timer. At most one fire
  per timer per cycle. `[PC-srv] high`
- F66. Callback failure prints `Timer Failed! [<kind>][<name>]` through the no-halt error
  printer (the full Lua error text itself is printed by the protected-call handler first), where
  kind is `Simple` for simple timers; the timer is removed. `[PC-srv] high`
- F67. `CurTime` used by timers is the server's game time (float); due times are doubles computed
  from it, so delays accumulate without float32 rounding. `[PC-srv] high`
- F68. Where the pass runs: the Lua timer cycle is invoked from the Lua manager's per-frame
  cycle, which runs after the game rules `Think` (and therefore after `GM:Think`) and before
  `GM:Tick`. `[inf] med` (measured by the Rust project's captures; the caller is outside the dump)
- F69. HTTP completions and asynchronous file read completions are delivered in the same
  per-frame cycle (threaded-call queue), each protected, each with the error handler. The async
  file callback receives `(fileName, gamePath, status, data)` with `data` set only when
  `status == 0` and the length is > 0, otherwise an empty string; the callback reference is freed
  after the call; callbacks belonging to a previous Lua state id are discarded. `[PC-srv] high`

## 7. Console variables and commands created from Lua

- F70. `CreateConVar(name, default, flags, help)` from Lua: the flags are masked with
  `0xFF4FFF7F`, i.e. the binding strips `FCVAR_ARCHIVE` (0x80), 0x00100000 (server can execute),
  0x00200000 (server cannot query) and 0x00800000 before handing them to the engine. Archive is
  emulated: if the Lua flags had `FCVAR_ARCHIVE`, the value is persisted by the binding in a
  KeyValues file, `cfg/client.vdf` for `FCVAR_LUA_CLIENT` (0x40000) and `cfg/server.vdf` for
  `FCVAR_LUA_SERVER` (0x80000), path id `MOD`, root key `CVars`; the saved value is applied
  right after creation. Values of `FCVAR_NEVER_AS_STRING` (0x1000) variables are saved as
  `%f` floats. `[PC-lsh] high`
- F71. Convars and concommands carrying `FCVAR_LUA_CLIENT` or `FCVAR_LUA_SERVER` are "managed":
  on Lua shutdown they are unregistered and deleted; the archive files are rewritten at that time
  (only non-empty tables are written). `[PC-lsh] high`
- F72. Pre-set cache: a value set (e.g. by `server.cfg` or `exec`) before a Lua convar exists is
  cached by name; when a Lua convar with `FCVAR_REPLICATED` (0x2000) is created and a cached value
  exists, that value is applied and the cache entry removed. `[PC-lsh] high` (flag identity `med`)
- F73. `concommand.Add` creates a real engine command with a completion callback; server commands
  from clients reach Lua through the game's client-command path (the player entity is resolved
  from the edict; a non-player edict yields a nil player). `[PC-lsh]+[PC-srv] high`
- F74. Convar cvars seen in this subsystem: `lua_networkvar_bytespertick` (NWVar byte budget,
  F45) and `lua_log` (when non-zero every Lua error is appended to `lua_errors_server.txt`).
  `[PC-srv] high`

## 8. Error handling and printing

- F75. Stack leak check: at every Lua cycle, if the stack top is not 0 the server aborts with
  `Server Lua Stack Leak [%i]!` (`Client ...` on the client) followed by a dump of up to 50 stack
  entries `index: name (type)`. `[PC-lsh] high`
- F76. Protected call wrapper (used for hooks, timers, async callbacks): the error-handler
  reference is inserted below the function; after a failed call the error value is classified:
  nil -> a fixed "nil error" text; string -> the string (a stack trace is read into the error
  record); any other type -> `[<type id>]`-style text. The value is popped, the realm name is
  attached, and if the caller asked for reporting the error callback is invoked. `[PC-lsh] high`
- F77. Server error callback: attributes the error to an addon by matching file paths against
  mounted addons; formats the stack as lines `  N. <function or "unknown"> - <file or
  "filename">:<line>`; prints the error block in the error colour; appends `Lua Error: <text>`
  to the engine log when running dedicated; logs to `lua_errors_server.txt` when `lua_log != 0`;
  reports each distinct error text once to the error reporter; then calls hook `OnLuaError`
  (F18). `[PC-srv] high`
- F78. `Error(...)` from Lua -> `luaL_error("%s", msg)` (halts the chunk). `ErrorNoHalt(...)`
  formats into a 4096-byte buffer and prints through the callback without halting; the format
  path strips one trailing newline, reads the stack and attaches the realm; realm numbers used
  internally: server 6, client 4, menu 2. `[PC-lsh] high`
- F79. Script loading errors: `Couldn't include file '%s' - Not a .lua file! (%s)`,
  `Couldn't include file '%s' - File not found or is empty (%s)` (second `%s` is the current
  running file); `Not running script %s - it's too short.` / `Invalid script - or too short.` for
  empty sources; `Cannot run byte code! %x` / `Cannot run byte code!` when the first byte is 0x1B;
  only text chunks are accepted (`luaL_loadbufferx` mode `"t"`). Chunk names are
  `@<path><name>` with a duplicate leading `@` avoided. `[PC-lsh] high`
- F80. Client Lua errors reach the server through envelope type 2 (F57) and fire
  `OnClientLuaError`; argument list not recoverable from this dump. `[PC-srv] high` (args `low`)
- F81. Standard argument errors use `luaL_argerror`/`luaL_typerror` (messages
  `bad argument #n to 'f' (X expected, got Y)`). `[PC-lsh] high`

## 9. Script files, search paths and the Lua cache

- F82. Path ids: server scripts run under path id `lsv` (seen in gamemode loading), client under
  `lcl`, menu under `LuaMenu`. Mounting a Lua root for a realm adds `<root>/lua` and, except for
  `LuaMenu`, `<root>/gamemodes` to that path id. Paths whose id starts with `workshop` get an
  extra "remote/workshop" search-path flag. Adding a non-directory path warns
  `Tried to add search path, but path isn't path(!?) (%s)`. `[PC-lsh] high` (`lcl` `med`)
- F83. `include(path)`: slashes normalised to `/`; must end in `.lua`; the file is first looked up
  relative to the directory of the currently running file, then relative to the Lua roots; during
  execution the file's directory is pushed on the current-path stack and popped afterwards; the
  chunk name is the resolved path with the caller-supplied tag. `[PC-lsh] high`
- F84. File load: the path is normalised (`/`, dot-slashes removed; a path that cannot be
  normalised is rejected); the file's modification time is stored with the cache entry and the
  cache entry is reused only while the time matches; otherwise the file is read and the cache
  entry rebuilt. On the client (data-table path) files arrive from the server by SHA-256 and are
  validated: `hash mismatch: %s`, `File in nosend - but we don't have it! (%s)`. `[PC-lsh] high`
- F85. Lua download cache on disk: `cache/lua/<sha256 hex>` under the writable path; on read the
  content hash is recomputed and `SHA256 in cache file doesn't match!!` rejects a corrupt entry.
  `[PC-srv] high`
- F86. `require(name)`: `_MODULES` global table is created if missing; if `_MODULES[name]` is
  truthy -> return true; otherwise a binary module `lua/bin/gm<sv|cl>_<name>_<platform>.<ext>` is
  looked up (prefix `gmsv_` on the server, `gmcl_` on the client); load errors:
  `Couldn't load module library! (%s)`, `Couldn't find function in library! (%s)`,
  `Couldn't load module!`; on success `_MODULES[name] = true`. Without a binary module the Lua
  module `includes/modules/<name>.lua` is run; if that fails too: `Module not found!`. `[PC-lsh] high`
- F87. Source pre-processing: a macro pass runs over every text chunk before compilation (the
  binary contains a macro expander; its rules are not visible in strings). `[PC-lsh] med`
- F88. The `file` library's sandbox (`DATA` writes only under `data/`, extension whitelist
  `txt dat json xml csv jpg jpeg png vtf vmt mp3 wav ogg`) is implemented by the library's C
  functions, which are not in this dump; the only `file` function present is the async-read
  completion pump (F69). `[inf] high` (as a statement of absence; whitelist from public docs `med`)

## 10. Differences from Source SDK 2013 (summary)

- F89. Hook dispatch through a Lua `hook.Call` with a pooled name table (F4, F12) has no SDK
  counterpart. `[PC] high`
- F90. `DT_BaseEntity` is extended by seven 32-slot arrays (F52); `DT_PlayerResource` adds team,
  health and armour arrays (F53); scripted weapon/entity classes add `m_ScriptedEntity`. `[PC] high`
- F91. The password check is delegated to Lua with a prefix-compare fallback (F18) rather than
  the SDK's exact compare in the engine. `[PC] high`
- F92. Convar archiving for Lua convars is emulated through `cfg/client.vdf` /
  `cfg/server.vdf` instead of `config.cfg` (F70). `[PC] high`
- F93. Byte code is refused and four `debug` functions are removed (F3, F79). `[PC] high`
- F94. The timer scheduler (F61..F66) and the NWVar user-message channel (F41..F48) are GMod
  additions. `[PC] high`
