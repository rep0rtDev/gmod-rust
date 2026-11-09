# clientfx: gap list, gmod-rust vs the game

Rust clone: `rep0rtDev/gmod-rust` @ `280c7450` (main, 2026-11-09). Files:
`crates/gm-client/src/main.rs` (M), `crates/gm-client/src/particles.rs` (P),
`crates/gm-render/src/world.rs` (W), `crates/gm-net/src/msg.rs` (N). Line numbers are from
that commit. "Game" = behaviour in `clientfx.md` (section numbers refer to it).

Ranking: **P0** obvious to any player, **P1** noticeable when looking, **P2** subtle.

Status column: `match`, `partial`, `missing`, `wrong`.

---------------------------------------------------------------------------------------------

## 1. Explosions (clientfx.md section 1)

| # | behaviour | Rust today | status | rank |
|---|---|---|---|---|
| 1.1 | Network payload: origin, magnitude, radius, scale = magnitude/100, flags | `EffectEvent::Explosion { pos }` only (N 550-552; server mapping `gm-net/src/server.rs:1039`) | missing: no magnitude / scale / flags, so nothing below can scale or be disabled | P1 |
| 1.2 | Fireball: 64 `effects/fire_cloud2` sprites (32 at ±48 u + 32 misc), die 0.2..0.4 s, size clamp(randint(32,85)*vDev) -> 1.5x, grey 128..255, roll delta ±16, velocity 400..800*16*(vDev^2/2) | one additive quad of `sprites/zerogxplode` frames, size 96 u * (0.6 + age), colour fading k = 1 - age, life 0.6 s (M 4134, 614-630 `burst_life`, 5684-5700) | wrong: the game never draws `zerogxplode` for TE_Explosion (model index 0); the look is a cloud of 64 small fire sprites, not one 96..154 u billboard | **P0** |
| 1.3 | Smoke: 4 slow puffs (2..3 s, 32..64 -> 2x, alpha 128..255), 8 fast puffs (0.5..1 s, 72 -> 144), 32 ring puffs (16..32 -> 4x, alpha 16..32), light-tinted `particle/particle_smokegrenade` | none | missing | **P0** |
| 1.4 | 16 embers `effects/fire_embers1/2`, 2..3 s, size 4..32 | none | missing | P1 |
| 1.5 | Debris: 8..16 streaks `fire_cloud2` 1500..2500 u/s, 0.1..0.15 s; 16..32 `fleck_cement1/2` flecks 3 s with collision (gravity 800) | none | missing | P1 |
| 1.6 | Screen overlay flash (1.0,0.9,0.7), 0.1 s, grows 16/s | none | missing | P2 |
| 1.7 | Dynamic light (GMOD): radius 350 u, (255,145,30), exp 5, 0.3 s, linear decay 1166.67 u/s | dlight radius 300*k, colour (1,0.6,0.3)*k over 0.6 s (M 5700-5707) | wrong: radius 300 vs 350, colour (255,153,77) vs (255,145,30), duration 0.6 vs 0.3 s, both radius and colour fade (game: only radius shrinks, colour constant) | P1 |
| 1.8 | Sound `BaseExplosionEffect.Sound` unless NOSOUND; water variant `WaterExplosionEffect.Sound` / `Physics.WaterSplash` | sound handled elsewhere (not in fx code read) | unknown | P1 |
| 1.9 | Scorch decal: server trace origin -> origin - 32 u (env_explosion) ; grenade: 8 u above to 32 u below | trace pos + 8 z to pos - 32 z, `decals/scorch{1,2}_subrect` by position hash (M 4135-4153) | match (grenade variant) | P2 |
| 1.10 | Ragdoll push by radius / magnitude | none | missing (physics side) | P2 |
| 1.11 | Water explosion branch (ripple, splash3 column, bubbles, deep variant) | none; same fireball shown under water | missing | P1 |
| 1.12 | Shake 25 / 150 Hz / 1 s / 750 u on grenade explosions | `Shake amplitude duration frequency` event, `calc_shake` (M 4097-4110, 9105-9136) | partial: Rust re-rolls offset every 1/frequency s with linear fade and roll = ±amp*0.25; the game additionally multiplies by `fraction^2 * sin(curtime * frequency/fraction)` (CViewEffects::CalcShake), i.e. the shake oscillates and ramps up in frequency as it dies; radius falloff is server side (unknown whether gm-server applies it) | P1 |
| 1.13 | 0..3 `spark_shower` entities from env_explosion | none | missing | P2 |

## 2. Muzzle flashes (section 2)

| # | behaviour | Rust today | status | rank |
|---|---|---|---|---|
| 2.1 | Generic flash: 8 sprites `effects/muzzleflash{1..4}` along forward at 2*i*scale, size rand(6,9)*(12-i)/9*flScale, 0.1 s, alpha 255 -> 128, roll random | one quad of `effects/muzzleflash1` 12 u, colour (1,0.85,0.6), drawn in the viewmodel pass for 60 ms (M 5954-5962, 3783-3786, 4410-4413) | wrong: single sprite, fixed size, no roll, colour tint not in game (game is white), 60 ms vs 100 ms; nothing is drawn for other players' shots (`fires_bullets` only gates own flash, M 887-893) | **P0** |
| 2.2 | Per-weapon callbacks (pistol / 357 / SMG1 / shotgun / AR2 / combine) with smoke puff, core flashes, different scales and lifetimes (0.01 .. 0.5 s) | none | missing | P1 |
| 2.3 | Muzzle elight: radius randint(32,64), (255,192,64), 0.05 s, `muzzleflash_light` | dlight radius 100, colour (1,0.7,0.4), for the 60 ms the flash shows (M 5581-5587) | wrong: 2x radius, 1.2x duration, colour off (game 255,192,64), not convar gated | P1 |
| 2.4 | NPC / other player muzzle flashes at their weapon attachment | none | missing | P1 |
| 2.5 | Shell eject models (6001) | `BRASS_MODELS` (M 9140+) | match (list) | P2 |

## 3. Impacts (section 3)

| # | behaviour | Rust today | status | rank |
|---|---|---|---|---|
| 3.1 | Material dispatch (C/T/L flecks, W wood flecks, D/N/U/J dust, M/V sparks, P sparks, A antlion, Z warp) | every impact: one additive `effects/muzzleflash1` quad 5 u at pos + normal*1.5, 0.35 s, orange tint (M 4128, 5684-5699); same for prop hits (M 4320-4331) | wrong: the game has no flash sprite on impacts at all; the per-material particles are absent | **P0** |
| 3.2 | Metal sparks: randint(4,8)*2 `effects/spark` streaks 128..512 u/s, gravity 400, 0.05..0.1 s, + `effects/yellowflare` 24..28 u 0.1 s | none (only the muzzleflash quad) | missing | **P0** |
| 3.3 | Concrete / wood flecks + dust puffs | none | missing | P1 |
| 3.4 | Dust impact (dirt/sand/grass/snow): 3 groups of surface-coloured puffs | none | missing | P1 |
| 3.5 | Impact sound by surface (`Impact.*`) | not in fx code read | unknown | P1 |
| 3.6 | Surface flags SKY/NODRAW/HINT/SKIP suppress effects | not checked (server sends every hit) | missing | P2 |
| 3.7 | `cl_new_impact_effects`, `r_drawflecks`, `fx_drawmetalspark` convars | none | missing | P2 |
| 3.8 | Melee (crowbar) hits use the same `Impact` path with the damage type | `melee` flag in `EffectEvent::Impact` (N 543) unused on the client | partial | P2 |

## 4. Blood (section 3.6)

| # | behaviour | Rust today | status | rank |
|---|---|---|---|---|
| 4.1 | `bloodimpact` legacy effect: `blood_core` 2..4 u -> 8x, `blood_gore` -> 4x, `blood_drop` streaks with gravity 400 | spawns the `.pcf` system `blood_impact_<colour>_01` (M 4197-4215) and 6 `effects/blood_core` quads 4 -> 12 u, 0.35 s, velocity dir*70 + rand*40, gravity 400 (M 4232-4261) | wrong: GMod's default path is the legacy sprite effect, not the `.pcf` (the `.pcf` is used only when `cl_new_impact_effects` is on, and even then `blood_impact_*` is dispatched by `UTIL_BloodImpact` only in Source 2013 MP builds); the sprite set, counts (2 core + gore + drops), sizes and lifetimes differ | P1 |
| 4.2 | Blood colour table (red 72,0,0; yellow 195,195,0) | tints (0.55,0.02,0.02) / (0.85,0.75,0.2) / (0.3,0.75,0.2) (M 4238-4242) | partial: red is lighter than the game's (72,0,0) | P2 |
| 4.3 | Blood decals `Blood` / `YellowBlood` groups, 6 subrects | `decals/{blood,yblood}{1..6}_subrect` by hash (M 4219-4231) | match | |
| 4.4 | `bloodspray` for NPC damage (drops / gore / cloud with scale) | none | missing | P1 |

## 5. Decals (section 4)

| # | behaviour | Rust today | status | rank |
|---|---|---|---|---|
| 5.1 | Translation table by game material letter (C,T,P,L,D concrete; M,V,G metal; W,O wood; Y glass; F/B/H flesh; A/E antlion; N sand; I/S/X none) | `impact_decal` matches surfaceprop *names* (M 9090-9103): metal for 5 names, wood for 2, glass, `antlionsand` -> sand, else concrete | partial: should go through `surfaceproperties.txt` gamematerial letter then the table; e.g. `grate` (G) -> metal, `dirt` (D) -> concrete, `tile` (T) -> concrete, `foliage` (O) -> wood, `default_silent`/`slime` (S, X) -> none, `flesh` -> `Impact.Flesh`, `glass` -> `Impact.Glass` is right but `Impact.Metal` has only 4 subrects (Rust uses 1..5 for every group) | P1 |
| 5.2 | Group sizes: metal shot1..4, concrete shot1..5 | `1 + h % 5` for all groups (M 9102) | wrong for metal (shot5 does not exist) | P2 |
| 5.3 | `DamageDecal` overrides: TransAlpha -> none, `BulletProof` on non-normal rendermode 'G', `ManhackCut` for slash | none | missing | P2 |
| 5.4 | Flesh decals skipped on players | n/a (player hits produce `Blood`) | ok | |
| 5.5 | Lifetime: no time-based expiry, `r_decals` 2048 ring (world), model decal cache per entity | 120 s expiry and 256 cap for world and for prop decals (M 1153-1156, 4123, 4332, 5724-5725, 5738-5739) | wrong: decals vanish after 2 min and only 256 live; the game keeps 2048 with no timeout | P1 |
| 5.6 | Placement: projected along the normal, clipped to the face, aligned to the face texture axes, size = subrect texels * `$decalscale` | flat quad lifted 0.3 u, `right = n x helper` (world Z or X) (W 4771-4808), size from texels * decalscale (W 4395-4443) | partial: no clipping to the face (overhangs edges / corners), orientation uses a world helper axis instead of the face's texture axes | P2 |
| 5.7 | Blend: `DecalModulate` 2x multiply, others lightmapped alpha | modulate pipeline vs alpha, fog to grey for modulate (W 4414-4437) | match (approach) | |
| 5.8 | Scorch decal from explosions, BigShot etc. | scorch only | partial | P2 |

## 6. Tracers (section 5)

| # | behaviour | Rust today | status | rank |
|---|---|---|---|---|
| 6.1 | Not drawn when dist < 256 u | drawn whenever total >= 1 u (M 5556-5558) | wrong: close shots show a streak the game hides | P1 |
| 6.2 | Length rand(64,128) u, speed 5000 u/s, life (dist+length)/5000 | fixed 128 u tail, 5000 u/s, kept until head >= total (M 5557-5575) | partial: no random length, tail does not finish its run past the end | P2 |
| 6.3 | Material `effects/spark`, white, additive, width from the discrete line (~ 1.0 .. 1.5?) | `effects/spark`, width 1.5, colour (1,0.9,0.6) (M 5565-5573) | partial: colour should be white | P2 |
| 6.4 | Own shots: `FX_PlayerTracer` static one-frame streak at 128 + rand(-24,64) u, length 0.1..0.6 of the shot, scale 0.5..0.75 (first person uses the viewmodel muzzle) | own tracer uses the moving line from `own_muzzle` (M 5549-5551) | partial | P2 |
| 6.5 | Other players: start at the weapon muzzle attachment | `origin + (0,0,60)` (M 5553) | partial | P2 |
| 6.6 | Tracer frequency (1 in 2 for HL2 weapons), weapon specific tracers (`AR2Tracer` etc.), whiz sound | server decides; no AR2 variant / whiz | unknown / missing | P2 |

## 7. Water splashes (section 6)

| # | behaviour | Rust today | status | rank |
|---|---|---|---|---|
| 7.1 | `FX_GunshotSplash`: 8 `effects/splash2` streaks (gravity 800, 0.25..0.5 s, 150..300 u/s * flScale + z 32..64), 8 `effects/splash1` mist puffs 24*flScale u shrinking by index, ripple `splashwake1` 1.5 s | spawns `.pcf` `water_splash_01` (M 4267-4273) plus a ripple quad 8 -> 56 u, alpha 0.6 -> 0, 1.5 s, and 10 drops 4..8 -> 8..14 u, 0.8..1.2 s, gravity 600 (M 4275-4317) | wrong: the game does not use the `water_splash_01` `.pcf` for bullets; drops are streaks (trail particles) not puffs; lifetimes 0.25..0.5 vs 0.8..1.2; colour should be the water fog colour | P1 |
| 7.2 | Scale by caller (bullet vs physics speed), slime variant | none | missing | P2 |
| 7.3 | `Physics.WaterSplash` sound | not in fx code read | unknown | P1 |

## 8. Trails (section 7)

| # | behaviour | Rust today | status | rank |
|---|---|---|---|---|
| 8.1 | RPG: `RocketTrail` sprite entity (puffs per moved distance, flamelets when damaged, muzzle flare) | `.pcf` `Rocket_Smoke_Trail` on `w_missile` models (M 5364-5377) | wrong source: HL2's missile uses the `RocketTrail` entity, not a `.pcf` (the `Rocket_Smoke_Trail` system exists in the content but is for the EP2 era hunter / strider?); the look (long grey puffs 35 -> 55 u, 5 s) differs | P1 |
| 8.2 | SmokeTrail / FireTrail / DustTrail entities | none | missing | P1 |
| 8.3 | Sprite trails (`util.SpriteTrail`) | none | missing | P2 |

## 9. Combine ball (section 8)

| # | behaviour | Rust today | status | rank |
|---|---|---|---|---|
| 9.1 | Flicker + 8 motion-blur halos + oriented halo with sin(curtime*25) pulse | none | missing (AR2 alt-fire exists?) | P1 |
| 9.2 | `cball_bounce` / `cball_explode` quads and sparks | none | missing | P2 |

## 10. Beams (section 9)

| # | behaviour | Rust today | status | rank |
|---|---|---|---|---|
| 10.1 | Segment count from width (`length/(maxWidth*1.414) + 1`, max 128), width lerp start -> end | `beam_strip` draws one segment per point, constant width (W 4577-4625) | partial | P2 |
| 10.2 | Texture scroll: v = fmod(curtime*scrollSpeed, 1) + length*0.01 per beam | `u0` / `u_per_unit` passed by the caller; the tracer uses `u_per_unit = 1/128` and `u0 = 0` (M 5566-5573); physgun beam passes its own | partial: there is no time-based scroll for env_beam-style beams | P2 |
| 10.3 | SHADEIN / SHADEOUT / fade length, noise, halo | none | missing | P2 |

## 11. Sprite rendering (section 10)

| # | behaviour | Rust today | status | rank |
|---|---|---|---|---|
| 11.1 | Camera-facing quad: `right = forward x up_view`, `up = right x forward` (view plane) | `sprite_quad`: `right = fwd x Z`, `up = right x fwd` (W 4729-4757): upright, not view-plane aligned (vp_parallel_upright behaviour) | wrong for `vp_parallel` sprites (the default): when pitching the camera the quads lean; also degenerates looking straight down | P1 |
| 11.2 | Glow modes: `kRenderGlow` scale *= dist/200 and alpha *= 1200^2/dist^2, pixel-visibility occlusion | not implemented (no glow sprites drawn yet except via `.pcf`) | missing | P2 |
| 11.3 | `.pcf` renderer: `orientation_type` read (P 431) | rope renderer handled (P 436-470); sprite orientation types 0/1/2 handling not checked here | unknown | P2 |
| 11.4 | Additive = premultiplied colour*alpha, alpha 0 (W additive pipeline) vs alpha blend | matches Source's `$additive` behaviour | match | |
| 11.5 | Near-clip fade (64..128 u etc.) of emitters | none | missing | P2 |

## 12. Things gmod-rust does that the game does not

* An additive `effects/muzzleflash1` flash sprite on every bullet impact (world and prop)
  (M 4128, 4329-4330). The game has no impact flash; metal shows `effects/yellowflare`
  only with sparks.
* `sprites/zerogxplode` fireball frames for TE_Explosion (M 1444, 5688-5693). The game's
  TE_Explosion has model index 0 and never shows that sprite (it belongs to the legacy
  `TE_Explosion` with a model, used by HL1-style callers only).
* Orange tint (1, 0.85, 0.6) on the muzzle flash and (k, k*0.7, k*0.4) on impact flashes
  (M 5696, 5955). Game flashes are white (255,255,255) sprites.
* Decal expiry after 120 s and a 256 cap (M 5724-5739). The game keeps decals until the
  2048 ring recycles them.
* `.pcf` systems `water_splash_01`, `blood_impact_*_01`, `Rocket_Smoke_Trail` for bullet
  splashes, blood and the RPG trail (M 4197-4215, 4267-4273, 5364-5377). The game uses the
  legacy sprite emitters for all three in its default configuration.
* Explosion dlight colour fading with k (M 5703-5705); the game keeps the colour and shrinks
  the radius.
* `entity_remove` sparks (M 4155-4189) are a Lua addon effect (sandbox), fine, but they use
  `(p - center)*5` velocity with gravity 700; the Lua effect (`entity_remove.lua`) should be
  checked separately (not part of this subsystem).

## 13. Suggested order of work

1. P0: replace the single-quad explosion with the 64-fireball + smoke + ember + debris
   emitter set (clientfx.md 1.3), carry magnitude / scale / flags in `EffectEvent::Explosion`.
2. P0: remove the impact flash quad; add metal sparks + yellowflare, concrete/wood flecks and
   dust impact by game material letter; make the server send the letter (or the surfaceprop
   name plus a client lookup in `surfaceproperties.txt`).
3. P0: generic 8-sprite muzzle flash (white, random `muzzleflash1..4`, 0.1 s) for own and
   remote shots; elight 32..64 u, 0.05 s.
4. P1: decal table by letter, no expiry, 2048 ring; dlight numbers; tracer min distance.
5. P1: legacy blood / splash / rocket trail emitters instead of `.pcf`.
6. P2: sprite orientation modes, glow sizing, beam segmenting / scroll, near-clip fades.
