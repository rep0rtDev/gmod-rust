# gmod-rust `render` — gap list vs the game

Clone: `rep0rtDev/gmod-rust` @ main (shallow clone of 2026-11-09). Paths are relative to the repo root. Each item: what the game does (see `render.md` §), what Rust does, verdict (`match`, `partial`, `missing`, `wrong`), priority by player visibility (P0 obvious, P1 noticeable, P2 subtle).

## A. FOV

| # | Behaviour (render.md §) | Rust today | Verdict | Pri |
|---|---|---|---|---|
| A1 | Zoom blend `start + (target−start)·(3t²−2t³)` with `t = (now − changeTime)/rate`, started from the FOV at the moment of the SetFOV call (§1.1) | `crates/gm-client/src/main.rs` L1541-1557 `camera_fov`: linear, frame-dependent `cur += (target−cur)·(dt/secs)`; it never reaches the target exactly and the curve shape differs (exponential-looking, not S-curve). The server (`crates/gm-server/src/lib.rs` L5711-5713) stores `(fov, secs)` only, no change time / start value. | wrong | P1 |
| A2 | Vehicle view FOV replaces the default while seated (§1.1) | not implemented (no vehicle FOV in `camera_fov`) | missing | P1 (only when vehicles arrive) |
| A3 | Default FOV clamp 100, `fov_desired` 75 default (§1.1) | `crates/gm-client/src/settings.rs` L36-37, L91: 75, range 75..100 | match | – |
| A4 | Width scaling `2·atan(tan(fov/2)·aspect·0.75)` (§1.2) | `main.rs` L618-622 `scale_fov_by_width` same formula (atan form) | match | – |
| A5 | Aspect cap (`sv_restrict_aspect_ratio_fov`, cap 1.85·0.75) applied to the world FOV only (§1.2) | not implemented | missing | P2 (only ultra-wide multiplayer) |
| A6 | World near plane 3 u (§1.2) | check `Camera { near }` in `main.rs` L4565 region — near set by caller; not verified here | unknown | P2 |
| A7 | Zoom mouse sensitivity `zoom_sensitivity_ratio · fov/defaultFov` (§1.2) | not implemented (sensitivity not scaled by FOV in the input path) | missing | P1 when zoomed |
| A8 | `render.GetViewSetup()` / `RenderView` view table (`fov_unscaled`, `bloomtone`, `drawmonitors`…) (§1.2) | GLua surface not checked for these keys in this pass | unknown | P2 |

## B. Viewmodel

| # | Behaviour | Rust today | Verdict | Pri |
|---|---|---|---|---|
| B1 | Viewmodel FOV 54 default, per-weapon `SWEP.ViewModelFOV` (62 default for Lua SWEPs), minus weapon FOV offset (§2.1) | `main.rs` L348 `VIEWMODEL_FOV = 54` for every weapon; no per-SWEP value | partial | P1 (Lua SWEPs look wrong: 62 vs 54) |
| B2 | Viewmodel pass near 1 u, far = world far (§2.1) | `main.rs` L5918-5923: near 1, far **512** | partial (far plane only matters for huge viewmodels) | P2 |
| B3 | HL2 bob: `bobtime += dt·speed/320` using **curtime deltas**, cycles 0.45/0.9 s, `speed·0.005·(0.3+0.7 sin)`, clamp [−7,4], offsets 0.1/0.1/0.5/0.4/0.3/0.8 (§2.2) | `main.rs` L720-749 same constants; `dt` is wall-clock capped at 0.1 s instead of frametime/curtime (equivalent when frame time < 0.1 s) | match | – |
| B4 | Bob only on HL2 C++ weapons; Lua SWEPs have no bob unless `GetViewModelPosition` (§2.2) | bob applied to every viewmodel (`VmView::view` called for all) — needs check for scripted weapons at `main.rs` L3398-3425 (`sync_scripted_weapons`) | partial | P2 |
| B5 | Sway: threshold `g_fMaxViewModelLag = 1.5`, `speed = 5·|diff|/1.5` above it; `origin −= diff·5` (§2.3) | `main.rs` L751-758: threshold **1.0** and `rate = 5·d` (i.e. `5·d/1.0`) → catches up 1.5× faster than the game once behind | wrong (small) | P2 |
| B6 | Pitch offset 0.035/0.03/0.02 (§2.3) | `main.rs` L760-768 same | match | – |
| B7 | Lag computed on predicted angles, history interpolated (§2.3) | not interpolated; computed on the camera angles each frame | partial | P2 |
| B8 | `cl_flipviewmodels` / `SWEP.ViewModelFlip` (§2.3) | no flip path found (`grep flip` in main.rs: none) | missing | P2 |

## C. Flashlight

| # | Behaviour | Rust today | Verdict | Pri |
|---|---|---|---|---|
| C1 | Projected texture `effects/flashlight001` (border variant on DX8), perspective frustum `hFOV = vFOV = r_flashlightfov` (53.7), near 4 + distMod, far 750 (§3.1-3.4) | `main.rs` L5796-5802: a spot `DLight` with cosine cone `cos(26.85°)`, radius 750, soft edge 0.08 in cosine (`crates/gm-render/src/shaders/world.wgsl` L185-193); no texture, so the characteristic rounded-square texture shape and dark rim are absent | partial | **P0** |
| C2 | Falloff `1/(constant + linear·d + quadratic·d²)` with `0/100/0` → ≈ 1/d, plus far-plane cutoff (§3.4) | `world.wgsl` L180-201: `(1 − d/radius)²` (the point-light law) | wrong | P1 |
| C3 | Light origin: eye + `r_flashlightoffsety` (−20) up, swing by forward·(−12) when looking down, 4 u hull trace back to the eye, pull-back `distMod` toward the hit (20 %/frame, cap 128 u, ladder 40 u) (§3.2) | `main.rs` L5783-5784: light at `camera.position`, direction `camera.forward()`; no offset, no traces, no pull-back → walls close to the player are over-saturated and the cone does not swing | wrong | P1 |
| C4 | Shadow attenuation: shadowed texels multiplied by 0.35, depth res `r_flashlightdepthres` (1024/512) (§3.4) | `world.rs` L383-384 2048² map, `world.wgsl` L48 hard compare → black shadows | partial | P1 |
| C5 | Other players: origin from `gmod_flashlight` / `anim_attachment_RH` attachment, eye dir (§3.3) | `main.rs` L5786-5795: `origin + (0,0,64)` along view angles | wrong | P1 (multiplayer) |
| C6 | Per-player flashlight colour (3 bytes, default white) (§3.2 item 9) | `main.rs` L5800: fixed `(1,1,0.95)·1.5` | partial | P2 |
| C7 | Flashlight lights models too (`r_flashlightmodels`) | `world.wgsl` L278-296: dlights (incl. flashlight) applied to models with the same `(1−d/r)²` law | partial (law wrong, see C2) | P1 |
| C8 | Flashlight depth pass includes models (game: yes) | `docs/status.md`: entity models cast into the flashlight depth pass | match | – |

## D. Dynamic lights

| # | Behaviour | Rust today | Verdict | Pri |
|---|---|---|---|---|
| D1 | World law: per-luxel linear `radius − distance`, written into the lightmap (§4.2) | `world.wgsl` L180-201 per-pixel `(1 − d/r)²·facing` | wrong shape (quadratic vs linear; per-pixel vs per-luxel is fine) | P1 |
| D2 | Model law: quadratic with cutoff (§4.3) | `world.wgsl` L278-296 same `(1−d/r)²` | partial (acceptable until the engine law is confirmed) | P2 |
| D3 | `DynamicLight` Lua object keys `Pos/Dir/Size/DieTime/Brightness/Decay/MinLight/Style/Key/InnerAngle/OuterAngle/r/g/b/nomodel/noworld` (§4.1) | `crates/gm-render/src/world.rs` L387-395 `DLight { pos, radius, color, dir, cone }` — no die time / decay / min light / style / flags; `MAX_DLIGHTS = 8` (L377) vs engine 32 | partial | P1 (addon lights) |
| D4 | Decay: `radius −= decay·frametime` until die time (§4.1) | not implemented | missing | P1 |
| D5 | Spot dlights: inner/outer angle ramp (§4.1) | single cosine threshold with 0.08 soft edge | partial | P2 |

## E. HDR / tonemap / bloom

| # | Behaviour | Rust today | Verdict | Pri |
|---|---|---|---|---|
| E1 | Goal exposure `clamp(0.18 / avg_lum, min, max)`, `avg_lum` from the 2 % bright-pixel location and 50 % percentile with `min_avglum 3 %` floor, 16-bucket histogram (§5) | `world.rs` L4040-4051 `update_exposure`: `goal = exposure·0.6/bright` with `bright` = fraction of bright pixels (`main.rs` L6079-6085, probed every 15 frames); different measure and constant | wrong (approximation) | P1 |
| E2 | Rate: `mat_hdr_manual_tonemap_rate` (controller `SetTonemapRate`) up, ×3 down (§5) | `world.rs` L4045-4050: `tm_rate`, ×3 down — but applied as `(goal−e)·min(rate·dt,1)` on a 15-frame cadence | match (shape) | P2 |
| E3 | Defaults `autoexposure 0.5/2`, `bloomscale 1`, controller overrides (§5) | `world.rs` L1838-1854 reads the four controller inputs, defaults 0.5/2/1/1 | match | – |
| E4 | Bloom: downsample, blur X, blur Y + bloom, `$bloomamount` combine; bloom amount blends at 0.05/s (§5) | `crates/gm-render/src/shaders/post.wgsl`: luminance bright-pass with threshold (L38-41), 5-tap Gaussian (L47), composite; no threshold in the game (it blooms the tonemapped scene's over-bright part via the float path), no 0.05/s blend | partial | P1 |
| E5 | `bloomtone` view flag, `mat_disable_bloom`, DX ≥ 90 gate (§5) | `settings.rs` L57 `mat_bloomscale` as a bool toggle; no per-view flag | partial | P2 |

## F. Fog / sky / water

| # | Behaviour | Rust today | Verdict | Pri |
|---|---|---|---|---|
| F1 | Linear fog, `fogmaxdensity`, 2D sky unfogged (§6) | `world.wgsl` L302-322 | match | – |
| F2 | Underwater fog from the water material (§6) | `world.rs` L3433-3450 `view_fog` | match | – |
| F3 | 3D skybox from `sky_camera` with its own fog (§7) | `world.rs` L604-607 `SkyCamera`, L1918-1948 | match | – |
| F4 | Fallback sky `sky_day03_02` when the named sky fails (§7) | not found (`grep sky_day03_02`: none) | missing | P2 |
| F5 | Sky paint (`env_skypaint` + `edit_sky`) (§7) | `world.rs` L679, L736-738, L3616-3625 `fs_skypaint` | match (not verified numerically) | – |
| F6 | Water expensive/cheap decision, `$forcecheap/$forceexpensive`, `$reflect2dskybox/$reflect3dskybox`, reflect entities (§8) | reflection + refraction passes always when the material has the textures (`world.rs` reflect state L3433-3460, `world.wgsl` L325-375); no cheap/expensive switch, no `$reflect*skybox` honouring, no `r_waterforcereflectentities` | partial | P2 |
| F7 | Water fresnel/fog over refraction (§8) | `world.wgsl` L344-375 Schlick fresnel + fog | match (approx.) | – |

## G. Lightmaps / light styles / model lighting

| # | Behaviour | Rust today | Verdict | Pri |
|---|---|---|---|---|
| G1 | Light styles animate at 10 Hz from the pattern strings; value `(c−'a')·22/256` normalised at `m` (§9) | `crates/gm-render/src/lightmap.rs` L8-23 patterns, L24-32 **mean** brightness per pattern (`(c−'a')/12`), static; runtime changes only via networked `(style, brightness)` pairs (`world.rs` L3897-3943) | partial — flickering lights do not flicker | P1 |
| G2 | Named styles (≥32) toggled by `TurnOn/TurnOff/SetPattern` (§9) | `lightmap.rs` L33-56 initial state from spawnflags; server side toggling present via `lightstyles` messages (`crates/gm-net/src/msg.rs` L409) | partial | P2 |
| G3 | Ambient cube + 4 world lights + half-Lambert (§10) | `crates/gm-render/src/light.rs` L36-75 (attenuation, spot ramp), L100-120 (`shade`) | match | – |
| G4 | Ambient blend over time when an entity moves between leaves (§10) | relit per 8 u cell, no temporal blend (docs) | partial | P2 |

## H. Entity shadows

| # | Behaviour | Rust today | Verdict | Pri |
|---|---|---|---|---|
| H1 | Render-to-texture / blob shadows under every prop, player, NPC; direction from the map's shadow direction, `r_shadowdist`, `r_shadowcolor`, max 32 rendered (§11) | none — only the settings flag `settings.rs` L48, L97 (`r_shadowrendertotexture: true`) is stored; no shadow pass exists in `gm-render` | missing | **P0** |

## I. Interpolation / extrapolation / prediction

| # | Behaviour | Rust today | Verdict | Pri |
|---|---|---|---|---|
| I1 | Interp window `max(cl_interp, cl_interp_ratio/cl_updaterate)` → 0.1 s (6.6 ticks @66), rounded up +1 tick per entity (§12) | `crates/gm-net/src/client.rs` L98-120 `Interpolator::new(interp_ticks)` with a fixed **4 ticks** (`main.rs` L7970-7971); no cvars | partial | P1 (visible as earlier/jerkier remote entities under packet loss; smoothness differs) |
| I2 | Extrapolation capped at `cl_extrapolate_amount` 0.25 s, then hold (§12) | `client.rs` L168-180: `pos + vel·dt` without a cap | partial | P2 |
| I3 | Render clock: game samples at `curtime − interpAmount` (frame time, tick-aligned snapshots) (§12) | `client.rs` L142-163: steered clock (0.3 s time constant, snap at 1 s) — Rust-specific smoothing | extra behaviour (see §J) | P2 |
| I4 | Animation frame blending lerp/slerp, sequence fade 0.2 s (§12) | `world.rs` L1143-1175 `frame_pose` lerp+slerp, L1623-1630 `sequence_fade_out` 0.2 default | match | – |
| I5 | Prediction re-runs all pending usercmds, error smoothed over `cl_smoothtime` 0.1 s (§13) | `main.rs` L61-63, L127, L195 `gm_net::Prediction` re-runs cmds; smoothing not verified | partial | P2 |
| I6 | Interpolated latched vars for cycle / pose parameters / bone controllers (§12) | `InterpPose` (`client.rs` L106-115) carries pos, angles, vel, angvel, ragdoll bones only; no animation-var interpolation | partial | P2 |

## J. Things Rust does that the game does not
1. Steered interpolation clock with a 0.3 s time constant (`client.rs` L142-163): the game samples straight at `curtime − interp` and lets snapshots step. Harmless but not 1:1.
2. Bright-pass **threshold** bloom (`post.wgsl` L38-41): the game has no luminance threshold; bloom comes from the float scene's over-brightness through `dev/floattoscreen_combine`.
3. Flashlight as a 26.85° soft-edged cone light (`world.wgsl` L185-193) with `(1−d/r)²` falloff — a construct the game never has (it is always a projected texture).
4. 2048² flashlight shadow map with hard black shadows (`world.rs` L383, `world.wgsl` L48); the game uses 1024² (mp) / 512² and 0.35 attenuation.
5. Fixed warm flashlight tint `(1,1,0.95)·1.5` (`main.rs` L5800); the game is white at 1.0 unless the player sets a colour.
6. Exposure probe reading the back-buffer bright-pixel fraction every 15 frames (`main.rs` L6079-6085) — the game uses a 16-bucket luminance histogram every frame.
7. Light-style patterns baked at their *mean* brightness (`lightmap.rs` L24-32): the game never shows the mean; it shows the animated value.
8. Viewmodel far plane 512 u (`main.rs` L5921) — the game uses the world far plane.

## Ranked summary
- **P0**: H1 (no entity shadows at all), C1 (flashlight is a cone, not the projected texture).
- **P1**: C2, C3, C5, C7 (flashlight law/placement/others), A1 (zoom curve), A7 (zoom sensitivity), B1 (per-SWEP viewmodel FOV), D1/D3/D4 (dlight law, Lua dlight fields, decay), E1/E4 (exposure measure, bloom threshold), G1 (no light-style animation), I1 (fixed 4-tick interp).
- **P2**: everything else above.
