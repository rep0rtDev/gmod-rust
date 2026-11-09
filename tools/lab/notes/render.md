# Garry's Mod `render` subsystem — behaviour note

Scope: client view/FOV, viewmodel placement, flashlight, dynamic lights, HDR/tonemap/bloom, fog, sky, water, lightmaps/light styles, model lighting, entity shadows, bone setup / interpolation / prediction timing.

How to read the tags. Every fact ends with `[src: …; conf: H|M|L]`.
- `pc-server` – pseudo-C of the Linux server binary (real symbols, so function identity is certain).
- `pc-client` / `pc-engine` – pseudo-C of the Windows client/engine binaries (no symbols; identity inferred from strings, vtables and call graph; the first line of each dump file states the selector).
- `SDK` – Source SDK 2013 (`src/game/client`, `src/game/shared`) read-only clone; used for constants that the Windows dumps cannot show (see "Limitations").
- `Lua` – Facepunch/garrysmod gamemodes/base, sandbox.
- `rust-docs` – measurements already recorded in gmod-rust `docs/spec/render.md` / `docs/spec/convars-defaults.md`.
- `inference` – derived by me from the above.

Units: Hammer units (u) for distance, degrees for angles, seconds for time, Source "linear light" for radiance unless stated.

## 0. Limitations of the input (read first)

- The Windows `client.dll`/`engine.dll` pseudo-C loads every float constant from a data address, so the dump shows *where* a constant is used but not its value; only values that happen to be integer-encoded (e.g. `4.0`, `90`, `100`, `128`) are visible. Where a number below comes from the SDK it is marked so; where the GMod flow was verified in pseudo-C but the number only from the SDK, the tag is `pc-client + SDK`. `[src: inference over dump; conf: H]`
- Convar *names* appear in the client dump only when the function registers or looks them up by string. The flashlight, viewmodel-bob, sway, tonemap-rate functions reference their convars through pointers, so their names in this note are the SDK names (the dump contains no functions selected by `cl_wpn_sway_interp`, `cl_bobcycle`, `r_flashlightfov`, `mat_autoexposure_max`, `cl_extrapolate_amount`). `[src: dump index grep; conf: H]`
- The engine's per-pixel dynamic-light code for world faces and the studio-model renderer (dynamic light on models, ambient-cube sampling) are not in the dump (no selected functions). Those laws come from the SDK/public shader knowledge and are marked `inference`. `[src: dump index grep; conf: H]`

## 1. Field of view

### 1.1 Player FOV state (server, authoritative)
- The player's default FOV is stored as a **float** (the SDK stores an int). Reading the default FOV returns `min(stored, 100)`; the SDK clamps to `MAX_FOV = 90`. GMod therefore allows default FOVs up to 100 from the server side. `[src: pc-server default-FOV getter; conf: H]`
- Setting the default FOV to 0 substitutes the game rules' default FOV (sandbox: the player's `fov_desired`, forwarded as userinfo). `[src: pc-server default-FOV setter; conf: H]` `[src: inference for sandbox; conf: M]`
- Client-side convar `fov_desired`: default 75, archive + userinfo. SDK range is 75–90 (hl2mp) / 20–90; GMod widens the upper bound (the options menu offers 75–100 and the cvar accepts up to 120). `[src: SDK for default/flags; GMod range: public GMod behaviour, not in dump; conf: M]`
- Effective FOV on the server:
  1. Base = vehicle view FOV if the player sits in a vehicle whose view FOV is non-zero, else default FOV. `[src: pc-server FOV getter; conf: H]`
  2. If a zoom target FOV (a float in GMod, an int in the SDK) is non-zero it replaces the base. `[src: pc-server; conf: H]`
  3. If the zoom rate is 0 the target is returned immediately. Else `t = (curtime − fov_change_time) / rate`; while `t < 1`: `t` clamped to [0,1], `fov = start + (target − start) · (3t² − 2t³)` (SimpleSpline, identical to SDK). When `t ≥ 1` the start value is overwritten by the target. `[src: pc-server FOV getter (polynomial visible as t²·(3 − 2t)); conf: H]`
  4. The zoom start value can be overridden by the optional 4th argument of the set-FOV call (>0). `[src: pc-server FOV setter; conf: H]`
- Set-FOV call (requester, fov, rate, start): if another entity owns the zoom (env_zoom owner) and it cannot be overridden the call is refused; otherwise owner := requester (or cleared when fov == 0). Stores target FOV (float), change time (double curtime), rate (float). All three are networked. `[src: pc-server FOV setter; conf: H]`
- The networked FOV helper sends the *start* value while a blend is running toward a larger value (so the client interpolates itself), same as SDK. `[src: pc-server networked-FOV helper; conf: H]`
- FOV distance adjust factor (used for weapon spread / view-model scaling) = `currentFOV / min(defaultFOV, 100)`; if the default FOV is ~0 the factor is 1. The SDK returns exactly 1 when both are equal; GMod always divides (numerically the same). `[src: pc-server FOV distance-adjust helper; conf: H]`
- The weapon class's FOV hook returns its argument unchanged (weapons do not alter FOV in C++; Lua `SWEP:TranslateFOV` does). `[src: pc-server; Lua weapon_base cl_init.lua L134; conf: H]`

### 1.2 Client view setup and aspect scaling
- The client composes the view every frame: origin/angles from the player, `fov` = the player's current FOV (blended as above) and `fovViewmodel` = client-mode viewmodel FOV minus the weapon's FOV offset. `[src: SDK view.cpp L725-729; pc-client render-view function reads the same fields; conf: M]`
- Aspect handling: `aspect = (width/height) · 0.75` (i.e. relative to 4:3). The *limited* aspect is `min(aspect, cap)` and the cap is applied when `sv_restrict_aspect_ratio_fov > 0` and the game is windowed/multi-client, or always when it equals 2; the SDK cap is `1.85 · 0.75` (16:9 + a bit). World FOV is scaled with the limited aspect, viewmodel FOV with the *unlimited* aspect. `[src: pc-client main view-render function (selected by `sv_restrict_aspect_ratio_fov`), flow identical to SDK; cap value from SDK; conf: H for flow, M for 1.85]`
- Width scaling: `scaled(fov, ratio) = 2·atan(tan(fov·π/360)·ratio)·180/π`. `[src: SDK; pc-client arithmetic matches (tan→scale→atan); conf: H]`
- GMod `render.GetViewSetup()` exposes a table with keys `origin, angles, x, y, width, height, znear, zfar, znearviewmodel, zfarviewmodel, fov, fovviewmodel, fov_unscaled, fovviewmodel_unscaled, bloomtone, subrect, aspect, viewid, ortho{left,right,top,bottom}, offcenter{left,right,top,bottom}`. `fov_unscaled = 2·atan(tan(fov/2) · (4/3)/aspect)` – the inverse of the width scaling (so Lua sees the 4:3 value). This table does not exist in the SDK. `[src: pc-client function selected by `fovviewmodel_unscaled`; the 4/3 constant is inferred from the formula shape; conf: H flow / M constant]`
- GMod `render.RenderView` / `CalcView` view table accepts `drawhud, drawviewmodel, dopostprocess, drawmonitors, drawviewer, bloomtone, origin, angles, fov, viewmodelfov, znear, zfar, znearviewmodel, zfarviewmodel, ortho (left/right/top/bottom), offcenter`. `[src: pc-client render-view function strings; conf: H]`
- Base gamemode `GM:CalcView` returns `{origin, angles, fov, znear, zfar, drawviewer=false}`; vehicles go through `CalcVehicleView`, then `drive`, then the player class, then `SWEP:CalcView`. `[src: Lua base cl_init.lua L357-396; conf: H]`
- Near/far planes: world near plane 3 u (SDK `VIEW_NEARZ` 3 for HL2 games; 7 in some mods), viewmodel near plane 1 u, viewmodel far plane = world far plane. `[src: SDK view.cpp L595, L639-643; conf: M for GMod]`
- Zoom: the SDK's `zoom_sensitivity_ratio` (default 1.0) scales mouse sensitivity by `currentFOV/defaultFOV` when zoomed; GMod routes this through `GM:AdjustMouseSensitivity(default, localFOV, defaultFOV)` → `SWEP:AdjustMouseSensitivity`. `[src: SDK; Lua base cl_init.lua L418-425; conf: H]`
- `r_JeepFOV` exists in the client (vehicle view FOV cvar lookup). `[src: pc-client; conf: H]` Default 90 in SDK. `[src: SDK; conf: M]`
- The engine's `ZoomFOV / ZoomIn / ZoomOut / ZoomHold / ZoomSpline / ZoomStayout` strings belong to the demo editor's zoom action dialog, not to gameplay. `[src: pc-engine; conf: H]`

## 2. Viewmodel

### 2.1 Viewmodel FOV
- Client cvar `viewmodel_fov`, default 54 in GMod (SDK: `viewmodel_fov` 54 as `v_viewmodel_fov`). Lua `SWEP.ViewModelFOV` default 62 overrides it per weapon. `[src: SDK clientmode; Lua weapon_base shared.lua L10; conf: M for GMod default 54]`
- The viewmodel is rendered in its own pass with `fovViewmodel` (width-scaled by the unlimited aspect), znear 1 u, zfar = world zfar, over a cleared depth buffer. `[src: SDK viewrender.cpp L1072-1074; pc-client view flags; conf: H]`

### 2.2 Position: bob (HL2 weapons)
Executed per frame by the HL2 weapons' add-bob step (C++ weapons only; Lua SWEPs get the position via `GM:CalcViewModelView` → `SWEP:GetViewModelPosition` / `SWEP:CalcViewModelView`, with no bob by default). `[src: SDK; Lua base cl_init.lua L555-580; conf: H]`
- Constants: `HL2_BOB_CYCLE_MAX = 0.45 s`, `HL2_BOB_UP = 0.5`. (`cl_bobcycle 0.8`, `cl_bob 0.002`, `cl_bobup 0.5` exist but are unused by the HL2 bob.) `[src: SDK basehlcombatweapon_shared.cpp; conf: H]`
- `speed = clamp(|velocity_2D|, 0, 320)`; `bobtime += (curtime − lastbobtime) · speed/320`.
- Vertical cycle: `c = frac(bobtime / 0.45)`; `c = c < 0.5 ? π·c/0.5 : π + π·(c−0.5)/0.5`; `vBob = speed·0.005; vBob = vBob·0.3 + vBob·0.7·sin(c)`, clamped to [−7, 4].
- Lateral cycle identical with period 0.9 s → `lBob`, same clamp.
- Applied: `origin += forward·(vBob·0.1)`; `origin.z += vBob·0.1`; `roll += vBob·0.5`; `pitch −= vBob·0.4`; `yaw −= lBob·0.3`; `origin += right·(lBob·0.8)`. `[src: SDK; conf: H]` Not verified in pseudo-C (constants invisible, function not selected). `[conf for GMod: M]`

### 2.3 Position: sway / lag
- Convars `cl_wpn_sway_interp` (SDK single-player 0.1; hl2mp 0.0 dev-only) and `cl_wpn_sway_scale` (1.0 / 5.0). The lag threshold `g_fMaxViewModelLag = 1.5`. `[src: SDK baseviewmodel_shared.cpp L54-55, L459; conf: H]`
- Per frame (only if frametime ≠ 0): `diff = forward(angles) − lastFacing`; `speed = 5`; if `|diff| > 1.5` then `speed *= |diff|/1.5`; `lastFacing = normalize(lastFacing + diff·speed·frametime)`; `origin −= diff·5`. `[src: SDK; conf: H]`
- Pitch offset (when `sv_viewmodel_lag_do_angles`, default on): `pitch` wrapped to [−180,180]; `origin += forward·(−pitch·0.035) + right·(−pitch·0.03) + up·(−pitch·0.02)`. `[src: SDK; conf: H]`
- GMod keeps the predicted viewmodel's lag-angle history as an interpolated QAngle variable (as in the SDK's predicted-viewmodel class): the lag is computed on the predicted angles and interpolated. `[src: pc-client predicted-viewmodel constructor (selected by the lag-angles history string); conf: H]`
- `cl_flipviewmodels` (engine) mirrors the viewmodel; Lua `SWEP.ViewModelFlip` flips cull mode to clockwise. `[src: pc-engine string; Lua L611; conf: H]`
- Viewmodel attachment `muzzle` etc. are read from the viewmodel model; the flashlight attachment is separate (§3). `[src: inference; conf: M]`

## 3. Flashlight

### 3.1 Enable / material
- The flashlight effect is created with handle −1 and picks material `effects/flashlight001`; if the material system reports the DX8 capability path it uses `effects/flashlight_border` instead. `[src: pc-client constructor; conf: H]`
- `mat_supportflashlight`, `r_flashlightdepthtexture` (default 1), `r_flashlightdepthres` (SDK 512 sp / 1024 mp), `r_flashlightmodels 1`, `r_flashlightlockposition 0` exist. `[src: pc-engine strings / SDK defaults; conf: H names, M values]`

### 3.2 Per-frame update (local player)
The flow below was read in full in the client pseudo-C and matches the SDK's flashlight update, minus the episodic flicker branch (absent in GMod). Numbers are SDK defaults unless marked. `[src: pc-client; conf: H flow]`
1. `onLadder = movetype == 9 (MOVETYPE_LADDER)`.
2. `offsetY = r_flashlightoffsety` (SDK −20 u). If `r_swingflashlight` (SDK 1): `swing = pos + forward·(−12)`; if `swing.z > pos.z` then `offsetY += swing.z − pos.z`. (The 12 is SDK; the dump shows a constant.) `origin = pos + up·offsetY`.
3. A 4 u box hull trace (`±4` visible as integer) from `pos` to `origin` with mask MASK_SOLID minus hitboxes (0x200400B); on a hit the origin is reset to `pos`. Skipped on a ladder.
4. `target = pos + forward·r_flashlightfar` (SDK 750 u). `dir = normalize(target − origin)`; the basis is re-orthogonalised: `up −= dir·(dir·up)`, `right −= dir·(dir·right)`, `right −= up·(up·right)`.
5. Hull trace (4 u box) from `origin` to `target`, mask MASK_OPAQUE_AND_NPCS minus hitboxes plus windows (0x2004083), filter skips the local player and its viewmodel.
6. Pull-back (`distMod`): if hit distance `< 128`: `pullback = onLadder ? r_flashlightladderdist (SDK 40) : 128 − dist`; `distMod += (pullback − distMod)·0.2` (exponential 20 %/frame); if not on a ladder a back trace from `origin` to `origin − dir·(dist − 0.1)` clamps `distMod ≤ |backHit − origin| − 0.1`. Else `distMod *= 0.8` (decays toward 0 at 20 %/frame). `[128 visible as integer; 0.2/0.8 from SDK; conf: H/M]`
7. Light origin `= origin − dir·distMod`; orientation = quaternion of (dir, right, up).
8. State: `constant = r_flashlightconstant (0)`, `linear = r_flashlightlinear (100)`, `quadratic = r_flashlightquadratic (0)`, `hFOV = vFOV = r_flashlightfov`, `near = r_flashlightnear (4) + distMod`, `far = r_flashlightfar (750)`, `ambient = r_flashlightambient (0)`, shadow enable = `r_flashlightdepthtexture`, shadow resolution = `r_flashlightdepthres`, shadow attenuation = `r_flashlightshadowatten (0.35)`, slope-scale / depth bias cvars, texture as §3.1. `[src: pc-client state fill order matches SDK; values SDK; conf: H/M]`
9. **GMod difference – colour**: the flashlight colour is read from three bytes on the local player (per-player RGB, 0–255 → /255), not the SDK's constant white. Default is white. One extra float from a GMod convar is also stored in the state (brightness scale, assumed). `[src: pc-client byte loads from the local player; conf: H that it is per-player bytes; M for semantics]`
10. `r_flashlightfov` default in GMod: 53.7 ° (measured by the Rust project; SDK 45 °). `[src: rust-docs status.md; SDK; conf: M]`
11. Debug: `r_flashlightvisualizetrace` draws the trace; Lua debug table fields `entindex, flashlightHandle, flashlightState`. `[src: pc-client strings; conf: H]`

### 3.3 Other players' flashlights (GMod-specific)
- The light origin is taken from the player model's attachment `gmod_flashlight`; if absent, `anim_attachment_RH`; if neither exists, eye position and eye angles are used. The direction is the player's eye direction, and the attachment origin is moved back along the forward vector by a constant (value not visible). `[src: pc-client function selected by `gmod_flashlight`/`anim_attachment_RH`; conf: H]`

### 3.4 Projection and falloff (engine / shader)
- A flashlight is a projected texture: the world-to-light matrix is perspective with `hFOV/vFOV`, `near`, `far`; the texture is sampled in light space and multiplied by `1 / (constant + linear·d/far·? + quadratic·d²)` as encoded in the attenuation vector; with the defaults (`constant 0, linear 100, quadratic 0`) the intensity law is `≈ 100 / (100·d_norm)` i.e. proportional to `1/d` scaled so the far plane is dark. Shadows: depth texture compared with `r_flashlightdepthres` resolution; shadowed texels are multiplied by `r_flashlightshadowatten` (0.35) rather than black. `[src: SDK flashlighteffect + engine string set (perspective world-to-flashlight matrix, flashlight projection, `r_flashlightdepthtexture`); exact shader law is inference; conf: M]`
- Flashlights also light models (`r_flashlightmodels 1`) and leave flashlight decals/overlays on world geometry (engine flashlight-render and flashlight-decal functions). `[src: pc-engine strings; conf: H existence]`

## 4. Dynamic lights (dlights)

### 4.1 Lua `DynamicLight` object
- Keys accepted: `Pos, Dir, Size (= radius, clamped to [0, max]), DieTime (double), Brightness (byte), Decay, MinLight, Style, Key, InnerAngle, OuterAngle, r, g, b (bytes), nomodel (flag bit 2), noworld (flag bit 1)`. Error text: "Tried to set wrong key '%s' on Dynamic Light object." `[src: pc-client Lua binding; conf: H]`
- Engine dlight record: origin, colour (3 bytes + exponent/brightness), radius, die time, decay (radius units per second), min light, style, key, direction, inner/outer angle for spot dlights, flags (`DLIGHT_NO_WORLD_ILLUMINATION = 1`, `DLIGHT_NO_MODEL_ILLUMINATION = 2`). `[src: pc-client field writes + SDK dlight.h; conf: H]`
- Decay: `radius -= decay · frametime` each frame until die time. `[src: SDK dlight semantics; conf: M]`

### 4.2 Law on world faces (engine)
- Not in the dump. Public Source engine behaviour (Quake lineage): for each lightmap luxel within the sphere, `intensity = radius − distance(luxel, light)` (linear, in units) scaled by `1/radius`-normalised colour and the dlight's brightness exponent, added to the face's lightmap page for that frame; a luxel on the far side of the surface plane (negative plane distance) receives nothing. Faces touched by a dlight are re-uploaded that frame. `[src: inference / public knowledge of the Source dynamic-light lightmap pass; conf: L]`

### 4.3 Law on models (studiorender)
- Not in the dump. Source converts a dlight to a local light with `attenuation0 = 0, attenuation1 = 0, attenuation2 = 1/(r²)`-style quadratic plus cutoff: colour · `max(0, 1 − d/r)`-shaped falloff is applied in the vertex/pixel shader (`r_lightaverage`, `r_worldlights 4`, max 4 local lights per model). `[src: inference; conf: L]`

## 5. HDR, tonemap, bloom

- `mat_hdr_level` default 2 (archive); HDR enabled when the map has HDR lightmaps and level ≥ 2. `[src: SDK; rust-docs convars-defaults; conf: H]`
- Post-processing (client, per view): HDR type 2 (float render target): reads the auto-exposure range (min/max), when `mat_dynamic_tonemapping` (1) sets the goal tonemap scale, draws `dev/floattoscreen_combine` with `$bloomamount` = bloom scale (if the amount is ≤ 0 a no-bloom variant is used), then optional colour-correction. HDR type 1 (integer RT) uses a separate path. Bloom is skipped when `$bloomamount ≤ 0` or DX level < 90. `[src: pc-client post-process function; same flow as SDK; conf: H]`
- Bloom pipeline (SDK, strings present in client): downsample → `BlurX` → `BlurYAndBloom` → combine. `dev/bloomadd`, `pp_bloom` materials exist for Lua `DrawBloom`. `[src: pc-client strings; conf: H]`
- Auto-exposure target (new algorithm, `mat_tonemap_algorithm 1`, histogram of 16 buckets; the old algorithm uses 30 buckets): `loc = location of the mat_tonemap_percent_bright_pixels (2 %) brightest pixels (within target 60 %)`; `scalar = (60/100)/max(loc, 1e-4)`; `avgLoc = location of 50 %`; `scalar2 = (mat_tonemap_min_avglum 3/100)/avgLoc`, `scalar = max(scalar, scalar2)`; `scalar *= lastScale`, `scalar = max(0.001, scalar)`; goal `= clamp(0.18/avg_lum, autoExposureMin, autoExposureMax)` (0.023 for the integer path). `[src: SDK viewpostprocess.cpp L674-700, L2566-2572; bucket counts verified in pc-client; conf: H]`
- Tonemap scale moves toward the goal at `mat_hdr_manual_tonemap_rate` (1.0 /s; the material system rate), accelerated by `mat_accelerate_adjust_exposure_down` (3.0) when going down. `[src: SDK; pc-client prints these two names; conf: H]`
- Defaults: `mat_autoexposure_min 0.5`, `mat_autoexposure_max 2`, `mat_bloomscale 1`, `mat_bloomamount_rate 0.05`, `mat_hdr_tonemapscale 1`, `mat_bloom_scalefactor_scalar 1`, `mat_disable_bloom 0`. `env_tonemap_controller` inputs `SetAutoExposureMin/Max`, `SetBloomScale`, `SetTonemapRate` override them (gm_construct: 0.7 / 1.8 / 0.3 / 0.2). `[src: SDK; rust-docs measured for gm_construct; conf: H]`
- Bloom amount blends toward the controller's value at `mat_bloomamount_rate` (0.05/s). `[src: SDK; conf: M]`
- `bloomtone` flag of a view disables tonemapping/bloom for that view (Lua `render.RenderView`). `[src: pc-client view-table key; conf: H]`

## 6. Fog

- Fog comes from `env_fog_controller` (world) and the `sky_camera`'s fog block (3D skybox), with keys `fogenable, fogstart, fogend, fogmaxdensity, fogcolor, fogcolor2, fogdir, fogblend, farz, HDRColorScale`. Linear in view-space depth: `f = min(saturate((depth − start)/(end − start)), maxdensity)`; 2D sky is unfogged. `[src: rust-docs measured; engine FogEnable/FogEnableSky/FogOverride strings; conf: H]`
- Material-level keys `$fogenable, $fogcolor, $fogstart, $fogend, $nofog` (water volume fog; `$nofog` disables on sprites). `[src: pc-engine strings; conf: H]`
- Underwater: when the eye is inside a water leaf the view fog switches to the water material's fog (colour/start/end) — same as SDK. `[src: pc-client water selection flow + rust-docs; conf: M]`
- Lua `edit_fog` (Sandbox) replaces the controller values live via `SetupWorldFog`; `edit_sky` paints the sky (§7). `[src: Lua sandbox entities; conf: H]`

## 7. Sky

- Skybox name from server cvar `sv_skyname`; materials `skybox/<name><face>` with faces `rt bk lf ft up dn`, a `_hdr` variant when HDR is on; on failure the engine falls back to `sky_day03_02` and prints "Unable to load sky %s". `[src: pc-engine; conf: H]`
- 3D skybox: the `sky_camera` entity's origin and scale (default 16) render a scaled copy of the world first with its own fog, then the main view. `[src: SDK + engine 3D-skybox draw strings; conf: H]`
- Sky paint (`env_skypaint`, GMod only): top colour, bottom colour, fade bias, HDR scale, star layer (texture, scale, fade, speed), sun normal/colour/size, dusk colour/scale/intensity. The default painted sky is used when the map's skybox is missing (`DrawSkyBox`). `[src: not present in the dump selection (no file mentions skypaint); public GMod entity documentation; conf: M]`

## 8. Water

- Decision per water material (client, every view; identical flow to the SDK's water-render-info decision): expensive water needs DX ≥ 80; `$forcecheap` forces cheap, `$forceexpensive` or `r_waterforceexpensive` force expensive; otherwise expensive when the eye is closer than the view's water distance threshold or reflections are enabled and `$reflecttexture` is a texture. Refraction requires `$refracttexture`; reflection requires `$reflecttexture`; `$reflect2dskybox` and `$reflect3dskybox` control whether the sky is drawn into the reflection; `r_waterforcereflectentities` draws entities into it. Cheap water = neither refract nor reflect (envmap only). `[src: pc-client; conf: H]`
- Debug: "Water material: %s dist to water: %f / forcecheap / forceexpensive / refract / reflect". `[src: pc-client strings; conf: H]`
- Material params in gm_construct's water: reflection amount 0.4, `$refractamount`, normal map + scroll, `$fogcolor/$fogstart/$fogend`, `_rt_WaterRefraction` / `_rt_WaterReflection` render targets. `[src: rust-docs measured; conf: H]`
- `mat_drawwater` toggles water drawing. `[src: pc-engine; conf: H]`

## 9. Lightmaps and light styles

- LDR lightmap luxel decode `c · 2^exp / 255`; HDR lumps used when `mat_hdr_level ≥ 2`; `worldlights` lump versions checked ("Invalid worldlight lump version!"). `[src: rust-docs measured; pc-engine BSP loader strings; conf: H]`
- Light styles: 64 styles (0–63) with pattern strings `a`..`z` where `m` = normal (1.0), each character = 0.1 s (`r_lightstyle` can force one). Style value `= (c − 'a') · 22 / 256 ≈ (c−'a')/11.6`, i.e. `m` ≈ 1.03 → engine normalises to 1.0 at `m`. Standard patterns 0–12: `m`, `mmnmmommommnonmmonqnmmo` (flicker 1), `abcdefghijklmnopqrstuvwxyzyxwvutsrqponmlkjihgfedcba` (slow pulse), `mmmmmaaaaammmmmaaaaaabcdefgabcdefg` (candle 1), `mamamamamama` (fast strobe), `jklmnopqrstuvwxyzyxwvutsrqponmlkj` (gentle pulse), `nmonqnmomnmomomno` (flicker 2), `mmmaaaabcdefgmmmmaaaammmaamm` (candle 2), `mmmaaammmaaammmabcdefaaaammmmabcdefmmmaaaa` (candle 3), `aaaaaaaazzzzzzzz` (slow strobe), `mmamammmmammamamaaamammma` (fluorescent flicker), `abcdefghijklmnopqrrqponmlkjihgfedcba` (slow pulse no black), `mmnnmmnnnmmnn`. Named lights (style ≥ 32) are toggled by `TurnOn/TurnOff/Toggle` and `SetPattern`. `[src: Quake/Source public tables + SDK `light` entity; the engine table is data, not in the dump; conf: M]`
- Light-style changes are networked as string-table/user-message updates and applied at 10 Hz; faces with changed styles get their lightmaps re-summed (`lightmap · styleValue` per style, up to 4 styles per face). `[src: inference from engine design; conf: M]`

## 10. Model lighting (ambient cubes + local lights)

- Static ambient: per-leaf ambient cube samples (BSP `LEAF_AMBIENT_LIGHTING(_HDR)`), chosen by the nearest sample in the entity's leaf; the cube is six colours (±x, ±y, ±z) and shading = `Σ n_i² · cube[side_i]`. `[src: rust-docs measured; SDK; conf: H]`
- Up to 4 direct world lights per model (`r_worldlights 4`), point/spot falloff `intensity / (c + l·d + q·d²)`, spot cone `stopdot/stopdot2/exponent` ramp, sky light only if a trace toward the sun exits through a sky brush. `[src: rust-docs measured; conf: H]`
- Half-Lambert on models: `((n·l)·0.5 + 0.5)²`. `[src: rust-docs measured; SDK; conf: H]`
- Dynamic models are relit when they move (cached per 8 u cell in the Rust measurements; the engine re-samples every frame for moving entities with `r_lightinterp`-style blending of the ambient cube over 0.1 s). `[src: rust-docs; inference for engine blend; conf: M]`

## 11. Entity shadows (render-to-texture / blob)

- Client shadow manager: a shadow is re-projected only when the entity's origin or angles changed, when forced, or when its fade alpha dropped below a threshold; studio models with the RTT shadow type build a render-to-texture shadow, others a projected blob texture; invisible or non-casting entities get their shadow disabled. Same flow as the SDK. `[src: pc-client shadow-manager update / RTT-build functions (selected by their warning strings); conf: H]`
- Convars: `r_shadows 1`, `r_shadowmaxrendered 32`, `r_shadowrendertotexture` (SDK 0; GMod ships 1 by default — the options menu "Shadow detail" sets it), `r_shadows_gamecontrol −1`; texel budget `TEXEL_SIZE_PER_CASTER_SIZE = 2`, cull tolerance 0.5. `[src: SDK clientshadowmgr.cpp; GMod default inference from the game's options; conf: M]`
- The engine `r_shadowrendertotexture` state is dumped alongside `r_waterforceexpensive` / `r_waterforcereflectentities` in the material-config print. `[src: pc-engine; conf: H]`
- Shadow direction: from the world's `light_environment`-derived shadow direction (`r_shadowangles`/`r_shadowdir` override), distance `r_shadowdist` (SDK 50 u), colour `r_shadowcolor`, blob cut-off. `[src: SDK; conf: M]`

## 12. Bones, interpolation, extrapolation, cl_interp

- Interpolation amount (engine): `cl_interp_amount = max(cl_interp, cl_interp_ratio / cl_updaterate)`, where `cl_interp_ratio` is server-bounded (`sv_client_min_interp_ratio` / `sv_client_max_interp_ratio`). `[src: pc-engine (function selected by both names; the max() of the two terms is explicit); conf: H]`
- Per-entity interpolation window: predictable or client-created entities interpolate 1 tick; in multiplayer `TICKS_TO_TIME(TIME_TO_TICKS(cl_interp_amount) + 1)` (+1 tick when the server runs alternate ticks); single-player entities animated+simulated every tick use 1 tick. `[src: SDK c_baseentity.cpp interpolation-amount routine; conf: H]`
- Defaults: `cl_interp 0`, `cl_interp_ratio 2`, `cl_updaterate 20`, tick interval 1/66 s (sandbox default tickrate 66) → interpolation window `max(0, 2/20) = 0.1 s`, i.e. 6.6 ticks. `[src: rust-docs convars-defaults for ratio/updaterate; engine formula above; conf: M]`
- `cl_interpolate 1` (dev-only userinfo), `cl_interp_npcs 0`, `cl_interp_all 0`. Interpolation is disabled while paused, in demo pause, or when the host frame time exceeds a threshold. `[src: pc-client server-entity interpolation function + SDK; conf: H]`
- Extrapolation: `cl_extrapolate 1`, `cl_extrapolate_amount 0.25 s` — after history runs out, position continues linearly for at most 0.25 s, then holds. `[src: SDK; conf: H]`
- Interpolation runs over all entities in the "interpolation list" each frame; entities whose latch state is unchanged are removed from the list; the interpolated variable groups are `LATCH_SIMULATION_VAR` (origin, angles, velocity) and `LATCH_ANIMATION_VAR` (cycle, pose parameters, encoded controllers, bone controllers). `[src: pc-client server-entity interpolation function + the animating-entity constructor (selected by cycle/controller strings); conf: H]`
- Bone setup: bones are set up on demand with a bone mask (`BONE_USED_BY_ANYTHING`, `_ATTACHMENT`, `_HITBOX`) and cached per frame (bone cache keyed by frame count); ragdoll bones come from the networked physics objects; recorded for the demo tools. `[src: SDK c_baseanimating.cpp; pc-client bone-cache / bone-accessor / bone-recording / draw-model function set; conf: H]`
- Animation between frames: sequence cycle advances `frametime · fps / frames`; frames are blended by linear interpolation of positions and quaternion slerp between neighbouring frames; sequence blends via the sequence transitioner fade over the sequence's `fadeintime/fadeouttime` (0.2 s default). `[src: SDK bone_setup; conf: H]`
- Predicted viewmodel lag angles are a an interpolated angle variable so the viewmodel's sway is interpolated like other latched vars. `[src: pc-client; conf: H]`

## 13. Prediction basics

- `cl_predict` is set by the client at startup from a config value (the startup function emits either `cl_predict 0` or `cl_predict 1`). `[src: pc-client; conf: H]`
- `cl_predictweapons 1`, `cl_lagcompensation 1`, `cl_smooth 1`, `cl_smoothtime 0.1` (error smoothing over 0.1 s), `cl_pred_optimize 2`, `cl_showerror 0`, `cl_idealpitchscale 0.8`. `[src: SDK prediction.cpp; names also present in pc-engine; conf: H names / M GMod values]`
- Prediction re-runs every outstanding usercmd from the last acknowledged server state each frame (the prediction update routine), then view origin smoothing applies the prediction error over `cl_smoothtime`. `[src: pc-client prediction-update function; SDK; conf: H]`
- `cl_interp`, `cl_interp_ratio`, `cl_interpolate`, `cl_updaterate` and `cl_predict` are evaluated in the engine's network-settings path and shown in `net_graph`. `[src: pc-engine; conf: H]`

## 14. Summary of GMod vs Source SDK 2013 differences found in the binary
1. Player default/zoom FOV stored as floats; default FOV clamp 100 (SDK int, 90). `[pc-server; H]`
2. FOV distance adjust factor always divides (no early return). `[pc-server; H]`
3. Flashlight colour per player (3 bytes) + one extra GMod scalar in the flashlight state; no episodic flicker. `[pc-client; H/M]`
4. Flashlight of other players attaches to `gmod_flashlight` / `anim_attachment_RH`. `[pc-client; H]`
5. `render.GetViewSetup()` with unscaled FOVs; `render.RenderView` view table flags (`bloomtone`, `dopostprocess`, `drawmonitors`…). `[pc-client; H]`
6. Lua `DynamicLight` object with `nomodel/noworld` flags. `[pc-client; H]`
7. `r_flashlightfov` 53.7 (SDK 45), `r_shadowrendertotexture` on by default. `[rust-docs/inference; M]`
8. `env_skypaint` painted sky. `[Lua/public; M]`
