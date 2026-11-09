# clientfx: Garry's Mod client effects, behaviour note

Scope: explosions, muzzle flashes, bullet impacts per material, blood, decals, tracers, water
splashes, smoke / fire / dust trails, combine ball, beams, sprite rendering rules.

How to read the tags. Every fact ends with `[src; conf]`:

* `bin` = read in the pseudo-C of the GMod binaries (client.dll, server_srv.so). Numbers quoted
  from `bin` are decoded float constants of the binary; where the binary and the SDK agree the
  tag is `bin=SDK`.
* `SDK` = Source SDK 2013 source (file named). Used where the binary function was not in the
  dump or is a byte-for-byte match of the SDK and the SDK is simply easier to cite.
* `scripts` = GMod's public `garrysmod/scripts/*.txt` or `cfg/*.cfg`.
* `inf` = inference (stated as such).
* `conf`: high / medium / low.

Units: Hammer units (u), seconds (s), u/s, degrees for roll. Colours are 0..255 unless written
as 0..1. `rand(a,b)` is a uniform float, `randint(a,b)` a uniform integer (inclusive).
`rand(±x)^3` means an independent `rand(-x,x)` per axis.

The whole `clientfx` surface in GMod is the Source 2013 code with a handful of GMod edits; the
GMod-only differences found are listed in section 12 and marked **GMOD** in place.

---------------------------------------------------------------------------------------------

## 0. Common building blocks

0.1 Simple sprite particle (used by nearly everything below). Fields: position, velocity,
die time, start/end size (u, byte), start/end alpha (byte), colour rgb (bytes), roll (deg),
roll delta (deg/s). Per frame: `pos += vel*dt`, `roll += rollDelta*dt`, size and alpha lerp
linearly over `lifetime/dieTime`; the quad is camera facing (view-plane aligned), rotated by
roll; drawn with the material's own blend mode (`$additive 1` materials add, others alpha
blend). [SDK particles_simple.cpp; high]

0.2 Trail (streak) particle: a line segment from `pos - vel*length` to `pos`, `width` u wide,
`length` is in seconds of velocity, colour rgba floats; collides with the world using the
emitter's gravity, bounce dampen and (optional) velocity dampen. [SDK particles_simple.cpp
CTrailParticles; high]

0.3 Emitter near clip: particles within `nearClipMin..nearClipMax` u of the eye fade out
(alpha scaled linearly from 1 at max to 0 at min). Explosion core uses 64..128, misc fireballs
32..64, water splash / dust trail / fire trail 64..128. [bin=SDK; high]

0.4 "Particle throttle": every particle count below is multiplied by the particle manager's
throttle scaling (1.0 at normal budget). [SDK; high]

0.5 Explosion flags (network `TE_Explosion.flags`): NOADDITIVE 1, NODLIGHTS 2, NOSOUND 4,
NOPARTICLES 8, DRAWALPHA 16, ROTATE 32, NOFIREBALL 64, NOFIREBALLSMOKE 128. [SDK const.h,
checked in bin; high]

0.6 Game material letters (surfaceprop `gamematerial`): A antlion, B bloody flesh, C concrete,
D dirt, E eggshell, F flesh, G grate, H alien flesh, I clip, L (GMOD: `plastic_barrel` /
`Plastic_Box`), M metal, N sand, O foliage, P computer, S slosh, T tile, U (GMOD: `grass`),
V vent, W wood, X unused (no effects), Y glass, Z warp shield, J (GMOD: `snow`).
[scripts/surfaceproperties.txt + bin; high]

---------------------------------------------------------------------------------------------

## 1. Explosion

### 1.1 Server side: env_explosion (what the client receives)

1.1.1 On `Explode`: trace from origin straight down (origin to origin - 32 u on z) and place
a **`Scorch`** decal at the hit unless spawnflag 0x10 (no decal) is set. [bin server; high]

1.1.2 Sends `TE_Explosion` with: origin, model index 0 (fireball sprite not used; the client
builds the effect procedurally), **scale = magnitude / 100** (0 if spawnflag 0x4 "no fireball"),
frame rate 15, flags, **radius = radius override if > 0 else magnitude * 2.5**, magnitude,
normal (0,0,0), material letter 'C'. [bin server; high]

1.1.3 Flag mapping from spawnflags / rendermode: SF 0x4 -> NOFIREBALL, SF 0x40 -> NOSOUND,
SF 0x80 -> ROTATE, SF 0x100 -> NOFIREBALLSMOKE, SF 0x200 -> NOPARTICLES, SF 0x400 -> NODLIGHTS,
rendermode 4 -> DRAWALPHA, rendermode != 5 -> NOADDITIVE. [bin server = SDK explode.cpp; high]

1.1.4 Radius damage unless SF 0x1 (no damage); `Smoke` think 0.2 s later; when not under water
and SF 0x20 not set, spawns `randint(0,3)` `spark_shower` entities at the origin. [bin server;
high]

1.1.5 Grenades (frag, rpg, etc.) go through the same temp entity with their own magnitude and
in addition call the screen shake: amplitude 25, frequency 150, duration 1.0 s, radius 750 u,
command SHAKE_START (`CBaseGrenade::Explode`, virtuals default to 25 / 750). env_explosion
itself does not shake. [SDK basegrenade_shared.cpp; medium (grenade code not in the dump)]

### 1.2 Client side: receiving TE_Explosion

1.2.1 If `radius` and `magnitude` are non-zero and the "affect ragdolls" flag byte is set,
client ragdolls inside `radius` get an impulse scaled by `magnitude` (ragdoll enumerator).
[bin; high]

1.2.2 If the point contents at origin contain CONTENTS_WATER (0x20): play the **water
explosion** (section 1.6) and return. [bin=SDK; high]

1.2.3 Unless NOFIREBALL: create the **explosion overlay**: one glow-overlay sprite, base
colour (1.0, 0.9, 0.7), horizontal size 0.05, vertical 0.025 (overlay units, see 1.5), lifetime
0.1 s. [bin=SDK c_te_explosion.cpp; high]

1.2.4 Create the base explosion effect at origin with `magnitude` (as float), `scale`, `flags`
(section 1.3). [bin=SDK; high]

1.2.5 **GMOD**: if a client convar (name not recoverable from the dump; it is a boolean) is on
and NODLIGHTS is not set, allocate a dynamic light at origin: **radius 350 u, colour
(255,145,30), exponent 5, decay 1166.67 u/s, die = now + 0.3 s** (350 / 1166.67 = 0.3, i.e. it
shrinks to zero exactly at death). The SDK has no explosion dlight at all. [bin; high for
numbers, medium for the 0.3 s (derived from decay)]

1.2.6 Sound: unless NOSOUND, `BaseExplosionEffect.Sound` at origin. [bin=SDK; high]

### 1.3 Base explosion effect (the fireball, smoke, embers, debris)

1.3.1 Force direction: six axis-aligned traces of 128 u? (SDK `GetForceDirection`: traces
along ±x, ±y, ±z, the direction is the sum of the "open" directions weighted by trace
fraction, normalised; `force` is the number of blocked directions scaled). Force is then clamped
to [2,2] (EXPLOSION_FORCE_MIN = EXPLOSION_FORCE_MAX = 2), so **force is always 2** and the
spread `1 - 0.15*force = 0.7` is a constant; the binary has these constants folded (0.7, 1.4,
0.525 appear literally). [bin=SDK fx_explosion.cpp; high]

1.3.2 Order: PlaySound; if scale != 0: CreateCore, CreateDebris, CreateMisc. [SDK; high]

1.3.3 Lighting tint for smoke: sample the world light at `origin + dir*32`, take its
normalised colour `tint` and luminosity `lum`; `tint = tint*0.25 + 0.75`; `lum *= 255`.
Smoke particles use colour `tint * randint(lum*0.5, lum)` per channel. [bin=SDK; high]

1.3.4 CreateCore (skipped when NOFIREBALL). Emitter "exp_smoke" with sort origin at origin,
near clip 64..128, bounding box ±128 u. The smoke material is the dust puff
`particle/particle_smokegrenade` (the first "DustPuff" material); fire materials
`effects/fire_cloud1`, `effects/fire_cloud2`, embers `effects/fire_embers1`,
`effects/fire_embers2`. [bin=SDK; high]

Unless NOFIREBALLSMOKE, three smoke groups:

(a) **4 smoke puffs at origin** [GMOD values]: die time rand(2.0, 3.0) s; velocity =
normalise(rand(±0.7)^3 + dir*rand(1,6)); speed = rand(1,750) * 2 * (0.7*|dot(vel,dir)|);
colour per 1.3.3; **start size randint(32,64), end size 2x; start alpha rand(128,255), end 0;
roll randint(0,360); roll delta rand(-8,8)**. In the SDK this loop has die 0.5..1 s (2..3 only
with NOFIREBALLSMOKE), size 72 -> 144, alpha 255 -> 0, roll delta ±2. **GMOD swapped the look
of groups (a) and (b)** and made the slow puffs live 2..3 s. [bin; high]

(b) **8 smoke puffs at origin + rand(-16,16)^3**: die rand(0.5,1.0); velocity as (a) with speed
rand(1,2000)*2*(0.7*|dot|); colour per 1.3.3; **start size 72, end 144; alpha 255 -> 0; roll
randint(0,360); roll delta rand(-2,2)**. (SDK: size 32..64 -> 2x, alpha 128..255, roll delta
±8.) [bin; high]

(c) **32 ring puffs**: for i in 0..32, angle = i*2pi/32, ringDir = (cos, sin, 0); position =
origin + rand(-4,4)^3 + ringDir*rand(8,16); die rand(0.5,1.5); velocity = ringDir * rand(500,
2000) * 2 * 0.7; colour per 1.3.3; start size randint(16,32), end 4x; start alpha
randint(16,32), end 0; roll randint(0,360); roll delta rand(-8,8). [bin=SDK; high]

(d) **16 embers**: material `effects/fire_embers1` or `_2` (50/50); position origin +
rand(-32,32)^3; die rand(2,3); velocity dir' = normalise(rand(±1.4)^3 + dir); deviation vDev =
0.7*|dot(dir',dir)|; speed = rand(1,400) * 16 * (vDev^2 * 0.5); colour grey randint(192,255);
start size = clamp(randint(8,16)*vDev, 4, 32), end size = start; alpha 255 -> 0; roll
randint(0,360); roll delta rand(-8,8). [bin=SDK; high]

(e) **32 fireballs**: material `effects/fire_cloud2`; position origin + rand(-48,48)^3; die
rand(0.2,0.4); dir' = normalise(rand(±0.525)^3 + dir); vDev = 0.7*|dot|; speed = rand(400,800)
* 16 * (vDev^2*0.5); colour grey randint(128,255); start size = clamp(randint(32,85)*vDev, 32,
85), end size = 1.5x; alpha 255 -> 0; roll randint(0,360); roll delta rand(-16,16). [bin=SDK;
high]

1.3.5 CreateMisc (skipped when NOFIREBALL): a second "exp_smoke" emitter, near clip 32..64,
with **another 32 fireballs identical to (e)**, so 64 `fire_cloud2` sprites in total. [bin=SDK;
high]

1.3.6 CreateDebris (skipped when NOPARTICLES):

(a) Trail emitter (bbox ±128): **randint(8,16) sparks**, material `effects/fire_cloud2`
(yes, the fire cloud as a streak texture): die rand(0.1,0.15); direction normalise(dir +
rand(±1)^3); width rand(2,16) u; length rand(0.05,0.1) s; colour white, alpha 1; velocity =
direction * rand(1500,2500) u/s. [bin=SDK; high]

(b) Fleck emitter (bbox ±128) with collision: gravity 800, bounce 0.5?, collision params (0.9,
512, 1024, 800, 0.5) as the SDK `Setup(mins,maxs,dir,0.9,512,1024,800,0.5)`; **randint(16,32)
flecks** (randint(8,16) on the reduced branch): material `effects/fleck_cement1` or `_2`;
position origin + dir*constant + rand(-8,8)^3; die 3.0 s; direction normalise(dir + rand(±1)^3);
size randint(1,3) u (start = end); speed = rand(64,256) * (4 - size) * 16 * (vDev^2*0.5); roll
rand(0,360), roll delta rand(0,360); colour grey = min(1, 0.25*rand(0.5,1.5))*255 (32..96).
[bin=SDK; high]

### 1.4 Explosion overlay (the screen-space flash)

1.4.1 One sprite, colour (1.0,0.9,0.7) scaled by `1 - t/0.1`; horizontal/vertical size grow
by 16 u/s... in overlay size units from 0.05/0.025; lives 0.1 s. [bin=SDK c_te_explosion.cpp;
high]

1.4.2 Glow-overlay sizing rule (shared with env_sun etc.): size on screen =
remap(dot(viewDir, toOverlay), cos 40deg .. 1 -> 150*size .. 70*size), opacity remap(same,
0 .. 1) times the pixel-visibility fraction (occlusion query, proxy radius 2 u). It is drawn
additive, after the world, no depth test. [SDK glow_overlay.cpp; medium]

### 1.5 Water explosion (origin inside water)

1.5.1 Finds the water surface above the origin (trace up to 128 u). If the surface is more than
128 u above (deep), plays `WaterExplosionEffect.Sound` and only the underwater bubbles;
otherwise plays `Physics.WaterSplash` at the surface point. [bin; high]

1.5.2 Surface effects (unless NOFIREBALL): a `effects/splashwake1` ripple quad lying on the
surface: size 64 -> 256 u, alpha 0.85 -> 0?, yaw rand(0,360), yaw rate rand(-4,4) deg/s, scale
2, lifetime rand(0.8,1.0) s; a water column of `effects/splash3` sprites: first group speed
rand(650,750) u/s upward, size rand(32,64), colour-ramp 50..75; second group size rand(64,128),
velocity ranges 25..100 / 50..100 as in SDK `WaterExplosionEffect::CreateCore`. [bin partially;
SDK fx_explosion.cpp; medium]

1.5.3 Underwater: emitter "waterexp_bubbles", near clip 64..128, materials `effects/splash1`,
`effects/splash2`: bubbles at origin + rand(-32,32)^3, velocity normalise(rand(±0.7)^3 + up) *
rand(1,6)... scaled, roll delta rand(-8,8), count randint(2,3)?-style groups (SDK
`CreateMisc`). [bin=SDK; medium]

---------------------------------------------------------------------------------------------

## 2. Muzzle flashes

### 2.1 Generic flash (`MuzzleFlash` effect / `TE_MuzzleFlash`, used by every Lua SWEP via
`Entity:MuzzleFlash()` and by any weapon without a custom callback)

2.1.1 Input: origin, angles, scale, type. Forward from angles. `flScale = clamp(rand(scale -
0.25, scale + 0.25), 0.5, 8)`. [bin=SDK fx.cpp FX_MuzzleEffect; high]

2.1.2 **8 sprites, i = 1..8**: material `effects/muzzleflash{randint(1,4)}`; position = origin +
forward * (2*i*scale); die 0.1 s; velocity 0; colour (255,255,255) or the passed rgb; alpha 255
-> 128; size start = end = `rand(6,9) * (12 - i) / 9 * flScale` u; roll randint(0,360); roll
delta 0. Additive materials. [bin=SDK; high]

2.1.3 Attached variant (HL2 weapon muzzle event, `FX_MuzzleEffectAttached`): same 8 sprites
positioned in the attachment's local frame along its x axis at `2*i*scale`, following the
weapon; die 0.1 s, or 0.0001 s ("one frame") when the caller asks for it; emitter bbox
±scale*? around the attachment; materials `effects/muzzleflash1..4` (and `_noz` variants for
first-person) and `effects/combinemuzzle1..2` (+`_noz`) preloaded. [bin=SDK; high]

2.1.4 Every HL2 weapon muzzle event also allocates an **entity light**: at attachment 1,
radius randint(32,64) u, decay radius/0.05 per s, die now + 0.05 s, colour (255,192,64),
exponent 5, only when convar `muzzleflash_light` (default 1) is on. [SDK c_baseanimating.cpp;
medium (convar string present in bin, function body not in dump)]

### 2.2 Per-weapon flashes (HL2 weapons; client temp-ent callbacks; all bin=SDK
c_te_legacytempents.cpp, high)

Weapon -> effect name: pistol `MuzzleFlash_Pistol_Player` / `_NPC`, .357
`MuzzleFlash_357_Player`, SMG1 `MuzzleFlash_SMG1_Player` / `_NPC`, shotgun
`MuzzleFlash_Shotgun_Player` / `_NPC`, AR2 and Combine `MuzzleFlash_Combine` (attached
generic, one frame), airboat `MuzzleFlash_Airboat`. Binary constants match the SDK exactly:

* Pistol player: 1 smoke puff (`particle/particle_smokegrenade`): die rand(0.25,0.5), vel =
  forward*rand(48,64) + z rand(4,16), grey randint(200,255), alpha randint(64,128) -> 0, size
  * rand(1,1.25)?, roll delta rand(-0.1,0.1); flash core: sprites `effects/muzzleflash{1..4}`
  i = 1..?: die 0.01 s, colour (255,255,200+randint(0,55)), size rand(6,8)*(12-i)/9*scale with
  scale rand(1,1.25).
* .357 player: smoke die rand(0.5,1.0), vel forward*rand(8,64) + z rand(4,16), grey
  randint(200,255), alpha randint(64,128), size *8 (end), roll delta rand(-0.5,0.5); flash
  core as pistol with scale rand(1.25,1.5).
* SMG1 player: flash core die 0.025 s, scale rand(1.25,1.5).
* Shotgun player: flash core die 0.0001 s, scale rand(1.25,1.5).
* Shotgun NPC: generic attached flash scale 0.75; plus "muzzle_embers" (`effects/spark`? no:
  ember sprites): die rand(0.2,0.4), dir forward + rand(±0.05), speed rand(64,256), colour
  (255,128,64), alpha 255 -> 0; plus trail sparks: die rand(0.1,0.2), speed rand(256,1024),
  width 0.05?, length rand(0.25,0.5), colour (255,242,191).
* Combine NPC: scale rand(1,1.5); 4 flash groups of `effects/combinemuzzle1/2` sprites with
  speeds rand(50,150) along forward / sideways, die 0.1 / 0.2 s, sizes rand(6,8)... rand(2,4)
  * scale; a glow sprite die rand(0.3,0.4) size rand(12,16)*scale; dlight (64,128,255) exp 5
  radius randint(32,128), die now + 0.05, when `muzzleflash_light`.
* Combine player (AR2): attached generic flash, scale rand(2,2.25), die 0.025 s, colour
  (255,255,200+randint(0,55)), plus a `effects/combinemuzzle1` core sprite alpha
  randint(64,128), size rand(10,16).

2.2.1 Preloaded flash materials also include `sprites/muzzleflash1_hl1..3`,
`sprites/ar2_muzzle1b..4b`, `sprites/muzzleflash_cs` (CS flash, `CS_MuzzleFlash` effect) and
the shell models `models/weapons/shell.mdl`, `rifleshell.mdl`, `shotgun_shell.mdl`,
`models/Shells/shell_9mm/57/12gauge/556/762nato/338mag.mdl`. [bin; high]

---------------------------------------------------------------------------------------------

## 3. Bullet impacts

### 3.1 Dispatch (`Impact` client effect, from the server's `UTIL_ImpactTrace` on every bullet
hit)

3.1.1 Parse: origin, start, material letter, damage type, hitbox, entity, flags. Then:
(1) push client ragdolls along the shot ray (unless the flag says the server already did);
(2) choose the decal (section 4) and apply it to the world / static prop / entity (skipped for
"Flesh" decals on players); (3) if a decal was placed, run the custom particle effects
(3.2); (4) play the impact sound for the surface (`Impact.*` / surfaceprop `bulletimpact`
sound). [bin=SDK fx_impact.cpp; high]

### 3.2 Per-material effects

3.2.1 Nothing when the hit surface has SURF_SKY, SURF_NODRAW, SURF_HINT or SURF_SKIP (mask
0x384). [bin=SDK; high]

3.2.2 If `cl_new_impact_effects` (default 0) is on, a `.pcf` system is used instead
(`impact_concrete[_noflecks]` for C/T, `impact_dirt` D/N, `impact_metal` M/V, `impact_wood
[_noflecks]` W, `impact_computer` P, `impact_glass` Y, `impact_antlion` A,
`warp_shield_impact` Z). Default path is the legacy one below. [bin=SDK; high]

3.2.3 `noFlecks = !r_drawflecks (default 1) || (flags & NOFLECKS)`. Dispatch by letter:

| letter | effect |
|---|---|
| C, T | debris flecks (3.3) with concrete material |
| **L (GMOD)** | debris flecks with `noFlecks` forced **on** (dust only) |
| W | debris flecks with wood material |
| D, N, **U, J (GMOD)** | dust impact (3.4) |
| A | antlion impact (yellow goo, SDK FX_AntlionImpact) |
| M, V | metal sparks (3.5); direction = reflect(shotDir, normal) + rand(±0.2)^3 |
| P | `Sparks` temp effect at origin + normal (magnitude 1, length 1) |
| Z | `.pcf` `warp_shield_impact` with angles from -shotDir |
| everything else (F, B, H, G, Y, S, O, X, E, I) | no particles |

[bin; high. SDK differs: no L/U/J letters.]

### 3.3 Debris flecks (concrete / tile / wood)

Emitter "FX_DebrisFlecks"? (SDK name), gravity 800, collision dampen 0.3?, bbox ±32:
**`numFlecks = iScale * randint(4,8)`** fleck sprites (`effects/fleck_cement1/2` for C/T,
`effects/fleck_wood1/2` for W): position origin, die 3 s, velocity = normalise(normal +
rand(±1)^3) * rand(64,128)? ... (SDK FX_DebrisFlecks) plus a **dust puff** group of
`particle/particle_smokegrenade`: 4 puffs? die rand(0.5,1), velocity normal*rand(1,6) + spread
0.2, speed rand(250,500)*i, colour = surface colour * rand(0.75,1.25), size iScale*randint(3,4)
*(i+1), end 4x, alpha randint(32,255) -> 0, roll delta rand(-8,8) (first puff: die 0.5, roll
delta ±0.1); plus one drifting puff at origin + rand(-8,8) xy, die rand(0.5,1), size
randint(4,8) -> 4x, alpha randint(32,64), roll delta rand(-1,1). [SDK c_impact_effects.cpp
FX_DebrisFlecks; the binary callee was not in the dump; medium]

### 3.4 Dust impact (dirt / sand / grass / snow)

Emitter "FX_DustImpact"; `flScale` = impact scale (1 for bullets). Surface colour from the
surfaceprop (`GetColorForSurface`). Three groups [bin=SDK c_impact_effects.cpp; high]:

(1) `nParticles1` (SDK: 4) puffs, material `particle/particle_smokegrenade`: die rand(0.5,1.0);
velocity = normalise(rand(±0.2)^3 + normal*rand(1,6)) * rand(250,500) * nId * flScale (nId =
index); colour = surfaceColour * rand(0.75,1.25) clamped; start size flScale*randint(3,4)*
(nId+1), end size flScale*start*4; alpha randint(32,255) -> 0; roll randint(0,360); roll delta
rand(-8,8).

(2) `nParticles2` (SDK: 4) puffs, material `particle/particle_noisesphere`: die rand(0.25,
0.75); same velocity law; start size randint(2,4)*(nId+1), end 2x; alpha 255 -> 0; roll delta
rand(-2,2).

(3) `nParticles3` (SDK: 4) drifting puffs, `particle/particle_smokegrenade`: position origin +
rand(-8,8) on x and y; die rand(0.5,1); velocity = normalise(rand(±1)^3 + normal) * rand(0,50);
start size randint(1,4), end 4x; alpha randint(32,64) -> 0; roll delta rand(-16,16).

### 3.5 Metal sparks

Gated by convar `fx_drawmetalspark` (default 1). Trail emitter "FX_MetalSpark 1": velocity
dampen 8, gravity 400, collision dampen 0.25, material `effects/spark`. **numSparks =
randint(4,8) * (iScale*2)** (times throttle). Per spark: die rand(0.05,0.1) s, but when iScale
>= 2 every third spark gets rand(0.15,0.25); `spreadOfs = rand(0,2)`; direction = reflect +
rand(±0.5*spreadOfs) per axis, normalised; width rand(1,4) u; length rand(0.025,0.1) s
(SDK: `length*0.25..length` with length = 0.1); colour white alpha 1; velocity = direction *
rand(128*(2-spreadOfs), 512*(2-spreadOfs)) u/s. Plus one flare quad `effects/yellowflare` at
the impact: size randint(24,28) u, die 0.1 s, alpha 1 -> 0, yaw randint(0,360), white.
[bin=SDK fx_sparks.cpp; high]

### 3.6 Blood

3.6.1 Bullet in flesh: the server's `UTIL_BloodImpact` sends the `bloodimpact` effect (origin,
normal, colour, scale) and `UTIL_BloodDecalTrace` places `Blood` / `YellowBlood` decals
(section 4). Blood is gated server side by `violence_hblood` / `violence_ablood`. [SDK; high]

3.6.2 `bloodimpact` (FX_BloodBulletImpact), emitter "bloodgore" gravity 200: `dir = normal *
rand(-0.5,0.5)` per axis; **core**: `effects/blood_core`, die rand(0.25,0.5), velocity dir *
rand(16,32), z -= rand(8,16); colour = rgb * rand(0.75,2.0) clamped; start size randint(2,4),
end 8x; alpha 255 -> 0; roll randint(0,360). **gore** (i = 0..): `effects/blood_gore`, die
rand(0.5,0.75), velocity dir*rand(16,32)*(i+1), z -= rand(32,64)*(i+1); size randint(2,4) ->
4x; alpha 255 -> 0. **drops**: trail emitter "blooddrops" gravity 400, collision (1, 10, 100,
400, 0.2): `effects/blood_drop`, direction normalise(rand(±1)^3), speed rand(64,128), die
rand(0.5,2.0), width rand(0.05,0.15), length rand(0.25,0.5). [bin=SDK fx_blood.cpp; high]

3.6.3 `bloodspray` (NPC damage, FX_BloodSpray, flags DROPS/GORE/CLOUD): "blooddrops" trail
emitter gravity 600, dampen 0.2: drops `effects/blood_drop` at origin ± 0.5*scale along
right/up; dir = normal + rand(±0.3); velocity dir*rand(4*scale,40*scale) + z rand(4,16)*scale;
width rand(0.125,0.275)*scale; length rand(0.02,0.03)*scale; die rand(0.5,1). Second drop set:
dir = normal + rand(±1), z += rand(0,1); speed rand(2*scale,25*scale) + z rand(4,16)*scale;
width rand(0.25,0.375)*scale; length rand(0.0025,0.005)*scale. "bloodgore" simple emitter:
gore `effects/blood_gore` die 0.3, velocity rand(±0.2)^3 + normal*randint(10,100), colour ramp
rand(0.75,1.25), size rand(0.25*scale, scale) -> 2x, alpha randint(200,255) -> 0; puffs
`effects/blood_puff` at ±1*scale, die rand(0.5,0.8), velocity rand(±?)+normal*randint(100,200),
size rand(1.5*scale,2*scale) -> 4x, alpha randint(80,128) -> 0. [bin=SDK; high]

3.6.4 Blood colours (SDK): red (72,0,0)... the effect receives rgb from the blood colour table:
BLOOD_COLOR_RED = (72,0,0), YELLOW = (195,195,0), GREEN = (195,195,0)? (SDK `BloodSprayCallback`
uses r,g,b from data). [SDK; medium]

### 3.7 Underwater bullet impact

Bullet hitting water from inside plays `Underwater.BulletImpact` and small bubbles
(`effects/bubble`, `effects/splash2`). [bin strings; medium]

---------------------------------------------------------------------------------------------

## 4. Decals

4.1 Choice of decal for a bullet: `DamageDecal(damageType, material)` on the hit entity:
rendermode TransAlpha (4) -> no decal; rendermode != normal and material 'G' -> `BulletProof`;
DMG_SLASH -> `ManhackCut`; otherwise **`Impact.Concrete`**, then translated by the engine with
the game material letter via `scripts/decals_subrect.txt` `TranslationData`:
C, T, P, L, D -> `Impact.Concrete`; M, V, G -> `Impact.Metal`; W, O -> `Impact.Wood`; Y ->
`Impact.Glass`; F -> `Impact.Flesh`; B -> `Impact.BloodyFlesh`; H -> `Impact.AlienFlesh`;
A, E -> `Impact.Antlion`; N -> `Impact.Sand`; I, S, X, '-' -> **no decal**. **U (grass) and J
(snow) are not in the table -> the untranslated `Impact.Concrete` is used.** Breakables
(`func_breakable`) override: glass material -> `GlassBreak`? (SDK `CBreakable::DamageDecal`).
[SDK baseentity_shared.cpp + scripts; high]

4.2 Decal groups (random pick inside a group, weight 1 each) [scripts/decals_subrect.txt;
high]:

* `Impact.Concrete`: `decals/concrete/shot1..5_subrect`; `Impact.Metal`:
  `decals/metal/shot1..4_subrect`; `Impact.Wood`: `decals/wood/shot…`; `Impact.Glass`,
  `Impact.Flesh`, `Impact.BloodyFlesh`, `Impact.AlienFlesh`, `Impact.Antlion`, `Impact.Sand`.
* `Scorch`: `decals/scorch1_subrect`, `decals/scorch2_subrect` (explosions). `SmallScorch`,
  `FadingScorch` (fire).
* `Blood`: `decals/blood1..6_subrect`; `YellowBlood`: `decals/yblood1..6_subrect`.
* `Splash.Large` / `Splash.Small`: `decals/splash1_subrect` (**GMOD** removed splash2/3 as
  duplicates); `Splash`: `decals/decalsplash`.
* `BigShot` (beams with decals, `CBeam` returns "BigShot"), `GlassBreak`, `BulletProof`,
  `ManhackCut`, `BeerSplash`, `RedGlowFade`, `CombineBallFade`, `PlasmaGlowFade`,
  `ExplosiveGunshot`, `Fire`, `Extinguish`, `Rollermine.Crater`, `BirdPoop`,
  `PaintSplatBlue/Green/Pink`, `EnergyBall.Impact`, `Antlion.Splat`,
  `SecurityCamera.Detachment`, `Antion.Unburrow`; `Models` group for model decals.

4.3 Placement (engine): the decal is projected along the hit normal onto the surface
(world brush faces and displacements; static props via the static-prop decal path; dynamic
entities via per-entity model decals in the entity's frame), clipped to the face polygon;
size in world units = `texture width/height (of the subrect) * $decalscale` (subrects
without their own `$decalscale` use the atlas material's); orientation is random roll? No:
the engine aligns the decal with the face's texture axes (no random rotation). Blend:
`DecalModulate` shader -> multiply (2x modulate, grey = no change); `LightmappedGeneric` /
`VertexLitGeneric` with `$decal 1` -> alpha blend lit by the lightmap. [SDK/engine
knowledge; medium; engine decal code is not in the dump]

4.4 Lifetime limits: world decals are a ring of `r_decals` (GMod `config_default.cfg`: 2048)
with the oldest recycled when full, no time-based expiry; model decals per entity are
limited by the model decal cache (`r_maxmodeldecal` 50 default) and also never time out;
flagged decals (`FadingScorch`) fade by their material's proxy. [scripts/cfg + SDK/engine;
medium]

4.5 Blood decals: `UTIL_BloodDecalTrace` traces from the hit along the bullet direction for
up to `172 u`? (SDK `TraceBleed`: 1..3? traces of length `flDamage*2 .. flDamage*4`, random
direction within a cone, each placing one `Blood`/`YellowBlood` decal). [SDK; medium]

---------------------------------------------------------------------------------------------

## 5. Tracers

5.1 Server: `MakeTracer` sends the `Tracer` effect (or the weapon's tracer name: `AR2Tracer`,
`AirboatGunTracer`, `HelicopterTracer`, `GunshipTracer`, `StriderTracer`, `HunterTracer`,
`GaussTracer`) with start, end, flags (USEATTACHMENT, WHIZ). Lua weapons set `Tracer` /
`TracerName` in the bullet table (binary strings `Tracer`, `TracerName`). HL2 pistol/SMG/357
/shotgun use `Tracer`, AR2 `AR2Tracer`. Tracer frequency: 1 in `m_iTracerFreq` shots (SDK
default 2 for HL2 weapons, Lua `TracerName`/`Tracer` field = frequency). [bin strings + SDK;
medium]

5.2 `Tracer` callback: start is the weapon muzzle attachment when USEATTACHMENT (first
person: the viewmodel's `muzzle`), end the hit; velocity **5000 u/s** (TRACER_SPEED); if the
WHIZ flag is set a `TracerSound` whiz is played when the ray passes near the viewer. [SDK
fx_tracer.cpp; high]

5.3 `FX_Tracer(start, end, velocity)` [bin=SDK clientsideeffects_test.cpp; high]: `dist =
|end - start|`; **nothing is drawn if dist < 256 u**; `length = rand(64,128)` u; `life = (dist
+ length) / velocity` s; a *discrete moving line* (FX_AddDiscreetLine) of material
`effects/spark`, width... (SDK: `FX_AddDiscreetLine(start, dir, velocity, length, dist,
rand(0.75,0.9), life, "effects/spark")`: the 7th argument is the line brightness/scale
0.75..0.9); the segment's head travels from `start` at `velocity`, the segment is `length`
long, clipped to `[0, dist]`, dies after `life`. Colour white, additive.

5.4 `FX_PlayerTracer` (first-person own shots) [bin=SDK; high]: nothing if length < 256 u;
start offset along the shot = `TRACER_BASE_OFFSET (SDK 128?) + rand(-24,64)` u; the streak
covers `length * rand(0.1,0.6)` u; static line of `effects/spark`, scale rand(0.5,0.75), life
0.01 s (one frame).

5.5 Other tracers (SDK fx.cpp, materials in bin): `GunshipTracer`: `effects/gunshiptracer`
line, `AR2Tracer`: `effects/ar2_altfire1`? (actually `AR2Tracer` uses the `Tracer` with
colour), `HunterTracer`: `effects/huntertracer`, `StriderTracer` blue, `GaussTracer`:
`effects/spark` with `Weapon_Gauss.Fire`, HL1 gauss wall impacts `HL1GaussWallImpact1/2`.
[bin strings + SDK; medium]

---------------------------------------------------------------------------------------------

## 6. Water splashes

6.1 Entry points: `TE_WaterSplash` / `gunshotsplash` effect (bullet into water: server
`UTIL_ImpactTrace` on a water hit sends `gunshotsplash` with scale = 8? per bullet), physics
objects entering water (`Physics.WaterSplash` sound + `watersplash` effect scaled by speed),
and `TE_WaterLakeSplash`/`waterripple`. Slime (`CONTENTS_SLIME`) uses the slime variant
with `effects/slime1` and `effects/splash4`. [SDK fx_water.cpp + bin; high]

6.2 `FX_GunshotSplash(origin, normal, scale)` [bin=SDK fx_water.cpp; high]:
`flScale = min(scale/8, 4)`. Trail emitter "splash": gravity 800, collision dampen 2,
material `effects/splash2`. **Droplets** (count = 8? per SDK `nParticles`): position origin +
rand(-8,8)*flScale on x,y; die rand(0.25,0.5); direction normal + rand(±0.8)^3; velocity =
direction * rand(SPLASH_MIN_SPEED*flScale*3, SPLASH_MAX_SPEED*flScale*3) with SPLASH_MIN/MAX
= 50 / 100 u/s, plus z += rand(32,64)*flScale; width rand(1,3) u; length rand(0.025,0.05) s;
colour = water fog colour * rand(0.75,1.25). **Mist** ("splish" simple emitter, 8 puffs i =
0..7, material `effects/splash1`): die 2 s; velocity = normalise(rand(±0.2)^3 + normal*
rand(4,6)) * 50*flScale*(8 - i); colour fog colour * rand(0.75,1.25); start size =
24*flScale*remap(i, 7..0 -> 1..0.5), end size start*2?; alpha...; roll randint(0,360), roll
delta rand(-4,4). Then **ripple** `FX_WaterRipple(origin, flScale, colour, 1.5 s, alpha =
luminosity)`.

6.3 Large bullet/explosion splash variant seen in the binary (TE_WaterSplash with bigger
scale): two droplet groups die rand(0.5,1.0): speeds rand(200,400) and rand(300,600) with z +=
rand(32,128), width rand(1,3)/rand(2,3), length rand(0.025,0.1), end size *1.5..2, offsets
rand(-16,16) xy, spread 1.0 and 0.2. [bin; medium (which caller uses it not pinned)]

6.4 `FX_WaterRipple(origin, scale, colour, lifetime, alpha)`: one quad `effects/splashwake1`
flat on the water: start size scale*16?, end size scale*? (SDK: `FX_AddQuad(origin, up,
scale*16?, scale*128?, 0.7, alpha, 0?, 0.25, yaw rand(0,360), yawRate rand(-16,16), colour,
lifetime, "effects/splashwake1", FXQUAD_BIAS_SCALE|FXQUAD_BIAS_ALPHA)`); in the binary the
ripple call carries (scale*A, scale*B, 0.7, alpha, 0, 0.25, yaw, yawRate -16..16) and the
physics splash uses lifetime 1.5 s. [bin=SDK; high for structure, medium for the exact size
multipliers]

6.5 Physics splash (`watersplash` effect from `CSplashEvent`/physics): same as 6.2 with
`Physics.WaterSplash` sound (volume 1) at the splash, droplet offset rand(-8,8), die
rand(0.25,0.5), speed rand(SPLASH*flScale*3) + z rand(32,64)*flScale, width rand(1,3), length
rand(0.025,0.05), colour ramp rand(0.75,1.25); mist as 6.2; ripple lifetime 1.5 s. Only when
the impact speed scaled by a constant exceeds a threshold. [bin; high]

---------------------------------------------------------------------------------------------

## 7. Smoke, fire and dust trails (networked entities)

7.1 `env_smoketrail` / `SmokeTrail` (used by RPG smoke, grenades, burning things): server
parameters: spawnRate (default 10 /s), particleLifetime 5 s, startColor (0.5,0.5,0.5), endColor
(0,0,0), minSpeed 2, maxSpeed 4, minDirectedSpeed = maxDirectedSpeed = 0, startSize 35,
endSize 55, spawnRadius 2, opacity 0.5, emit flag, stopEmitTime. Client, per particle:
position = attachment (or entity origin) + rand(±spawnRadius)^3; material randomly
`particle/particle_smokegrenade` or `particle/particle_noisesphere`; die = particleLifetime;
velocity = rand(±1)^3 * rand(minSpeed,maxSpeed) + attachmentForward * rand(minDirected,
maxDirected); colour = startColor * (1 + rand(-0.2,0.2)) (one value jitter); size start/end;
alpha = rand(opacity*0.75, opacity*1.25)*255, curve `alpha * sin(pi * t/die)` (fades in and
out); roll randint(0,360); roll delta rand(-1,1) with damping `rollDelta += rollDelta *
(-8*dt)` and a floor of ±0.5 deg/s. [bin materials/keys = SDK c_smoke_trail.cpp; high]

7.2 `RocketTrail` (RPG missile): same parameters; `numPuffs = moveLength / (startSize/2)`
spread along the frame's movement; die = lifetime + rand(0.9*lifetime, 1.1*lifetime); velocity
rand(±1)^3 * rand(minSpeed,maxSpeed); colour startColor * rand(0.75,1.25); start size *
rand(0.75,1.25), end size * rand(1,1.25); alpha rand(0.75,1.25)*opacity*255; roll delta
rand(-8,8). When `damaged`: flames `sprites/flamelet1..4` at origin + rand(±8)^3, die 0.25,
velocity rand(±1)^3*rand(32,128), alpha 255, roll delta ±8. Flare at the nozzle: 8 muzzle
sprites `effects/muzzleflash1..4` (i = 1..8), die 0.01 s, size rand(5,6)*(12-i)/9*rand
(flareScale-0.5, flareScale+0.5), roll randint(0,360). [bin keys = SDK; high]

7.3 `FireTrail` (burning props / ragdolls): emitter "FireTrail", materials
`sprites/flamelet1..5`, smoke emitter "FireTrail_Smoke" (`particle/particle_smokegrenade`),
near clip 64..128; flames: die 2 s, alpha randint(128,255) -> 0, size randint(1,2) -> 1,
velocity dir*rand(128,256); smoke: offset rand(±spawnRadius*0.5), die 1 s, roll rand(0,360),
roll delta rand(-2,2), alpha randint(32,64) -> 0, size startSize -> endSize, velocity
dir*rand(64,128). Spawn rate halves after the first half of the lifetime (SDK C_FireTrail).
[bin materials = SDK c_smoke_trail.cpp; high]

7.4 `DustTrail`: emitter "DustTrail", material `particle/smokesprites_0001`, near clip
64..128, same parameter set as SmokeTrail (spawnRate, lifetime, colours, sizes, opacity).
[bin = SDK; high]

7.5 Sprite trails (`env_spritetrail`, used by Lua `util.SpriteTrail`): server params start
width, end width, start width variance, lifetime, min fade length, texture resolution,
skybox flag; the client draws a beam-follow through the entity's last positions over
`lifetime` with the width lerped from start to end. [bin server setters; SDK c_sprite.cpp
C_SpriteTrail; medium]

---------------------------------------------------------------------------------------------

## 8. Combine ball (`prop_combine_ball`)

8.1 Materials: body `effects/ar2_altfire1`, blur `effects/ar2_altfire1b`, flicker
`effects/combinemuzzle1`; bounce/explode quads `effects/combinemuzzle1_nocull`,
`effects/combinemuzzle2_nocull`. [bin=SDK; high]

8.2 Draw each frame (when `emit`): **flicker halo**: `effects/combinemuzzle1`, grey
rand(0.2,0.3), size radius*rand(1.5,2.5), skipped when frametime == 0. If held or launched:
**motion blur**: 8 halos of `effects/ar2_altfire1b` stepping back along -moveDir by
`min(speed*0.5, 4)` u each (speed = |origin - lastOrigin| clamped 0..32), grey = base*(1 -
i/12), base = remap(speed, 4..32 -> 0..1), size = radius. If held: the model is drawn;
otherwise an **oriented halo** `effects/ar2_altfire1`, white, size = radius +
1.0*sin(curtime*25), roll = spawn time (constant per ball). All additive. [SDK
c_prop_combine_ball.cpp, materials confirmed in bin; medium-high]

8.3 `cball_bounce`: quad `effects/combinemuzzle1_nocull` size radius*10, alpha 1 -> 0?,
die 0.25?, yaw randint(0,360); `cball_explode`: `effects/combinemuzzle2_nocull` radius*2 ->
radius*4; in the binary both quads use alpha-bias 0.75..1 and die 0.4 s... (binary args: (0,
360, 0, colour white, 0.25|0.5, material, flags 3) and (64, 0 | 16, 32, 0.75, 1, 0, 0.4)).
Plus `effects/spark` trail sparks: offset rand(±8), speed rand(16,64)?, width rand(2,4),
die rand(0.4,0.6). [bin; medium]

---------------------------------------------------------------------------------------------

## 9. Beams (env_beam, Lua `render.DrawBeam`, weapon beams, physgun)

Beam drawing lives in the client's beam renderer; the pseudo-C for it was not in the dump
(no `sprites/laserbeam` / beam-draw strings selected), so this is SDK beamdraw.cpp
behaviour, which GMod ships unchanged as far as known. [SDK; medium]

9.1 Segment count: `maxWidth = max(startWidth, endWidth)*0.5`; if `length/(segments-1) <
maxWidth*1.414` then `segments = length/(maxWidth*1.414) + 1` (min 2); max 128
(noise_divisions). [SDK; medium]

9.2 Texture: `length *= 0.01` (so one texture repeat per 100 u for an unscaled beam); v
step per segment = `length/(segments-1)` (or `1/(segments-1)` with FBEAM_NOTILE); initial v =
`fmod(freq*speed, 1)` where `freq` is the current time and `speed` the beam's scroll rate
(`TextureScroll` key; 3.5 = 3.5 textures per second). u runs across the beam width (0..1).
[SDK; medium]

9.3 Width: per segment `width = lerp(startWidth, endWidth, fraction) * 2` (the renderer's
width parameter is a half width; `env_beam BoldWidth` is in tenths of units: `width/10`).
Brightness: FBEAM_SHADEIN ramps 0 -> 1 over `fadeLength/beamLength`, SHADEOUT 1 -> 0, both
together 2x ramps; colour = beam colour * brightness * `$hdrcolorscale`. [SDK; medium]

9.4 Noise: `noise` key (0..255 amplitude) displaces segments by a 128-entry sine noise table;
FBEAM_SINENOISE uses `sin` along the beam. `HaloScale` adds a `sprites/light_glow0x` halo at
each end. Default materials: `sprites/laserbeam.vmt` (env_beam), `sprites/physbeam` /
`physbeama` (physgun), `sprites/orangelight1` (gravity gun), `sprites/lgtning` (tesla,
stunstick), `sprites/bluelaser1` (combine sniper/strider). [SDK + bin material strings;
medium-high]

9.5 Beams are drawn additive (`$additive 1` in the beam materials), after opaque geometry,
depth tested, no depth write, camera-facing (each segment's quad is perpendicular to the
eye-to-segment vector). [SDK; medium]

---------------------------------------------------------------------------------------------

## 10. Sprite rendering rules (`env_sprite`, glows, `Entity:SetRenderMode`)

[SDK c_sprite.cpp; the C_Sprite functions in the dump are generic entity methods; medium-high]

10.1 Convar `r_drawsprites` 1. Scale: a sprite is drawn as a quad of `textureWidth*scale`
by `textureHeight*scale` u (`scale` = `m_flSpriteScale`, Lua `SetScale`); with
`m_bWorldSpaceScale` the scale is divided by `min(width,height)` so `scale` becomes the
width in u. Frame from `m_flFrame` (animated by `framerate`).

10.2 Render modes: `kRenderNormal` 0 opaque; `kRenderTransColor` 1, `TransTexture` 2,
`TransAlpha` 4, `TransAdd` 5, `TransAlphaAdd` 8? use the material blend with
`render->SetBlend(alpha/255)`; **`kRenderGlow` 3 and `kRenderWorldGlow` 9**: `blend *=
GlowBlend` where GlowBlend = pixelVisibleFraction (occlusion query with proxy radius
`m_flGlowProxySize`, default 2.0 u, Lua `SetGlowProxySize`) * `clamp(1200^2 / dist^2, 0, 1)`
(the glow fades with distance squared, fully visible inside 1200 u) unless renderfx
`kRenderFxNoDissipation`, in which case `alpha/255 * visible`; for `kRenderGlow` only (not
WorldGlow) **scale *= dist/200** so the glow keeps a constant screen size; rgb *= blend.

10.3 Orientation (`$spriteorientation` in the sprite material, or the .spr type): `vp_parallel`
(default, view-plane aligned; if roll != 0 it becomes `vp_parallel_oriented`),
`vp_parallel_upright` (world up, right = up x viewForward; falls back to view plane when
looking within 1 deg of straight up/down), `facing_upright` (right from the vector to the
viewer, world up), `oriented` (uses the entity angles: forward/right/up from AngleVectors),
`vp_parallel_oriented` (view-plane aligned, rotated by roll).

10.4 Additive vs alpha: decided by the material (`$additive 1` -> src colour added, alpha
ignored; otherwise `$translucent`/`$alphatest` alpha blend) and the render mode (TransAdd
forces additive). `$hdrcolorscale` multiplies colour in HDR.

10.5 Legacy temp-ent sprites (`TE_Sprite`, `TE_GlowSprite`, Lua `effects.*`): `TE_Sprite`
draws a temp sprite of `scale` for its animation then removes; `TE_GlowSprite` fades alpha
over `life` and scales by `scale`.

---------------------------------------------------------------------------------------------

## 11. Dynamic lights summary

| source | radius | colour | exponent | die / decay |
|---|---|---|---|---|
| Explosion (**GMOD only**) | 350 u | (255,145,30) | 5 | 0.3 s, decay 1166.67 u/s |
| HL2 weapon muzzle event (elight, `muzzleflash_light` 1) | randint(32,64) | (255,192,64) | 5 | 0.05 s, decay radius/0.05 |
| Combine NPC flash | randint(32,128) | (64,128,255) | 5 | 0.05 s |
| Combine ball / AR2 alt fire | per SDK c_prop_combine_ball (not in dump) | | | |

[bin for the first; SDK for the rest; high / medium]

---------------------------------------------------------------------------------------------

## 12. What differs from Source SDK 2013 (binary wins)

1. Explosion dynamic light (1.2.5): GMod adds a 350 u orange dlight, 0.3 s, convar gated.
   The SDK has none.
2. Explosion smoke groups (a) and (b) have their size / alpha / roll-delta sets swapped and
   the 4 slow puffs live 2..3 s regardless of NOFIREBALLSMOKE (1.3.4).
3. Impact dispatch has three extra material letters: L (plastic barrel / box: flecks off,
   dust only), U (grass) and J (snow) both using the dirt dust impact (3.2.3, 0.6).
4. `decals_subrect.txt`: `Splash.Large` / `Splash.Small` reduced to one subrect each; U/J have
   no translation entry.
5. Everything else that was read (muzzle flashes, metal sparks, dust impact, blood, tracers,
   water splash, trails, combine ball materials) matched the SDK constants exactly.

## 13. Open items (not recoverable from the dump)

* Name of the explosion-dlight convar (its string was not among the selected functions).
* The engine side of decals (projection, clipping, r_decals recycling, model decal cache)
  and beam drawing were not in the dump; SDK/engine behaviour is assumed.
* Exact sizes of the water-explosion column groups (1.5.2) and the FX_WaterRipple size
  multipliers (6.4) are read as "scale * constant" with the constants not decoded.
