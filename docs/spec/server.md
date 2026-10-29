# Server tick (`gm-server`)

One fixed step of 0.015 s composes the measured pieces: every player's usercmd goes through
`gm-move` against the BSP/displacement collision world, sandbox commands (`gm_spawn`, tool gun
clicks, `gmod_undo`, `gmod_cleanup`, `sbox_*`) go through `gm-sandbox`, and the props and
constraints the sandbox owns are mirrored into the Jolt world of `gm-physics`:

- a new sandbox prop becomes a dynamic body at its placement pose (PHY hulls, mass, surface,
  damping, drag areas and inertia of the model), frozen props are kinematic;
- a prop the tool gun moved (weld right click, axis) is teleported (`PhysObj:SetPos/SetAngles`,
  velocity cleared) and its frozen flag follows `EnableMotion`;
- weld -> Jolt fixed constraint, rope -> distance constraint between the recorded local points
  (`length + addlength`, rigid as min = max), axis -> hinge through `LPos1` along the axis towards
  `LocalAxis`, nocollide -> the pair is rejected in the Jolt contact validation (a prop dropped onto a no-collided frozen prop falls through it to the floor), re-enabled when the constraint goes; the world is a static
  body, so constraints to the world attach to it;
- undo and cleanup remove constraints first, then bodies;
- after the physics step every body's pose flows back into the sandbox, so the next trace or
  spawn sees the props where physics left them.

Nothing is networked yet; the harnesses drive the loop directly.

## Measured

The sandbox and tool gun recordings (`sandbox_spawn_basic`, `toolgun_weld_rope`, `toolgun_axis`)
replayed through the whole loop, 3 ticks of physics after every op (20 on the explicit waits)
with no poses taken from the recording: `server_spawn_basic` 16 ops within 0.55 u / 1.2 degrees
(a crate spawned 20 u above the floor falls and lands like the game's), `server_weld_rope` 20 ops
within 0.12 u / 0.25 degrees, `server_axis` 8 ops within 0.02 u / 0.06 degrees (frozen crates).

The physgun runs in the loop too (`Player::held`, `PlayerCmd::physgun`): the weapon fires with the
eye before the player's move of the tick (grab by the tool trace against the prop hulls, hold with
the shadow controller, release on attack up, freeze on attack2 also flags the sandbox prop), then
physics steps, then players move. The seven `physgun_*` recordings replayed as `server_physgun_*`
give the same numbers as the standalone physgun harness: 5 of 7 within tolerance (freeze 0.50 u,
off-centre 0.49 u, pitch steps 0.50 u, rotate 1.95 u / 4.45 degrees, yaw steps 0.10 u), walk
hold 3.0 u on the acceleration ticks and hold_crate 8.5 u only after the release landing (the open
contact response of `phys_*`).

## Not yet

Networking and client prediction, multiple players in one recording, spawn menu, the remaining stools.

## Player life cycle (measured, `player_lifecycle`)

Recorded on a real sandbox server with a bot killed four times (`Player:Kill`) and left to the gamemode:

- Spawn: health 100 (max 100), armour 0, model `models/player/kleiner.mdl`, hull (-16, -16, 0) to (16, 16, 72), 13 weapons (`gmod_camera`, `gmod_tool`, `weapon_357`, `weapon_ar2`, `weapon_crossbow`, `weapon_crowbar`, `weapon_frag`, `weapon_physcannon`, `weapon_physgun`, `weapon_pistol`, `weapon_rpg`, `weapon_shotgun`, `weapon_smg1`) with the crowbar out; `PlayerSelectSpawn`, `PlayerSpawn`, `PlayerLoadout`, `PlayerSetModel` fire in that order on the spawn tick.
- Spawn point: one of the map's `info_player_start` (33 on gm_construct) at random (observed sequence 13, 17, 12, 2, 17 of 33: repeats allowed, no visible order), the player placed 1 u above the entity's origin and settling onto the floor over the next ticks.
- Death: the same tick `Alive()` is false, health 0, move type 5, a ragdoll exists, `Deaths` +1; the weapons are still listed that tick (the active one reads `weapon_357` for that tick, a weapon switch artefact) and are gone the next tick; `PlayerDeathThink` starts the tick after.
- Respawn: exactly 135 ticks after the death tick for a bot (2.025 s at 66.67 Hz; deaths at 60, 300, 540, 780 -> spawns at 195, 435, 675, 915), without any input. Convars during the recording: `sbox_godmode 0`, `sbox_playershurtplayers 1`, `sbox_weapons 1`, `mp_falldamage 0`, `sv_gravity 600`.

`gm-server` reproduces this: `Player` carries `alive`, `health`, `armor`, `max_health`, `model`, `weapons`, `active_weapon`, `deaths`, `frags`, `ragdoll`; `Server::spawn_player` picks a spawn point with a deterministic xorshift (the harness only checks membership and the 1 u offset since the game's pick is random), `kill_player` records the death tick, `tick` strips the weapons the tick after and respawns at death + `RESPAWN_TICKS` (135); dead players neither move nor use the physgun. Harness `player_lifecycle`: 1100 ticks equal (alive, health, armour, deaths, ragdoll, weapon list), the four respawns within 0.07 u of a spawn point. Open: the ragdoll as a physics entity, damage (no damage sources yet), fall damage, the human player's respawn on `IN_ATTACK`.

## Damage (measured, `damage_*`)

Recorded with a bot dropped from heights and hit with `TakeDamageInfo`:

- Armour: a hit of `d` with armour `a > 0` takes `min(0.8 d, a)` from the armour and the rest from health (30 at armour 100: armour -24, health -6; 50 at armour 10: armour -10, health -40). `DMG_FALL` ignores armour (an 80.7 fall at armour 100 leaves it at 100); `DMG_BURN`, `DMG_BLAST`, `DMG_CLUB`, `DMG_BULLET` and generic do not.
- Health is whole: the fractional part of every hit is carried over in a per-player accumulator (0.4 then 1.6 takes 0 then 2) which survives `SetHealth` and respawns (on a fresh server a 12.5 fall takes 12; run after other hits carrying 0.68, it takes 13), so recordings must start from a fresh server or account for it.
- Death by a hit: health reads 0 that tick, `PlayerDeath`, ragdoll, respawn 135 ticks later like `Player:Kill`. Death by a fall keeps the negative health (-15, -42, -69 recorded) and the bot did not respawn by itself within the 200 ticks recorded.
- Fall damage (`DMG_FALL`, applied on the landing tick, speed = the downward speed at the start of that tick): nothing up to 522 u/s, damage from 540 u/s; `mp_falldamage 0`: 10 flat; `mp_falldamage 1`: `(v - 526.5) * 25/99` (540 -> 3.41, 693 -> 42.05, 1197 -> 169.32, all within 0.01).

`gm-server`: `damage_player(id, amount, type)` with the armour share, the carried fraction and the two death paths; `fall_damage_progressive` for `mp_falldamage`; the landing is detected from the movement state (`on_ground` false -> true). Harnesses `damage_fall`, `damage_hits`, `damage_fall_armor` equal every tick (health, armour, alive); `damage_fall_progressive` and `damage_fall_threshold` are equal since the ground probe of `categorize` was measured at 1.0-1.32 u with an up-only speed guard (see movement.md: the game grounds a 1089 u/s fall 0.3 u above the floor, which the old 2 u probe plus `|vz|` guard missed by a tick).

## Player ragdoll (measured, `ragdoll_still`)

On the dedicated server a dead player's body is an `hl2mp_ragdoll` entity created on the death tick at the player's origin with zero angles and **no physics objects** (`GetPhysicsObjectCount()` is 0 for all 135 ticks it exists): the ragdoll is simulated on each client from the player's model (`models/player/kleiner.phy`: 15 solids, 14 `ragdollconstraint` blocks and `collisionrules` pairs in the PHY text), the server only carries the record. `gm-server` keeps that record (`Ragdoll`: class, model, origin, angles, velocity, tick) on the dead player; harness `ragdoll_still` equal (class, count 0, tick, origin, alive per tick). The client-side ragdoll simulation (Jolt articulated body from the PHY constraints) is a client realm item.

## Weapons: loadout, ammo, switching

Recording `weapon_switch` (bot with the sandbox loadout, `SelectWeapon` at six ticks): the 13 weapons' clips, ammo types, slots, weights and hold types are in `gm_server::weapons::WEAPONS` (357 6/6, AR2 30/30 + AR2AltFire, crossbow 1/1, pistol 18/18, shotgun 6/6, SMG1 45/45 + SMG1_Grenade; the rest have no clip); reserve ammo on spawn in `SPAWN_AMMO` (357 32, AR2 100, AR2AltFire 6, Buckshot 64, Grenade 5, Pistol 256, SMG1 256, XBowBolt 32; RPG_Round and SMG1_Grenade 0). A switch is immediate: the selected weapon is active in the tick of the call, and both fire timers are set to the weapon's deploy time, then count down 0.015 per tick without clamping (crowbar 0.2333, physgun 0.2417, pistol 0.2, RPG 0.275, shotgun 0.1833, SMG1 0.2083; the others not measured yet). The pistol alone snaps its primary timer to 0.1 s in the past on the first tick after the deploy expired (-0.01, then -0.115 instead of -0.025), once per selection; the secondary timer keeps counting. `Server::select_weapon(id, class)` and `Inventory` carry this; harness `weapon_switch` compares the table, the reserve ammo, the active weapon and clips every tick and the fire timers within 1 ms: equal.

## Firing

Recordings `fire_*` (a bot with one weapon, IN_ATTACK tapped every other tick or held, IN_RELOAD; every tick the clip, reserve ammo, `GetNextPrimaryFire() - CurTime()`, and each `EntityFireBullets`). Every timer is relative to the tick the usercmd belongs to: a value set to x reads x - 0.015 in that tick's row, and a command fires when the previous row's value is <= 0. Rules in `gm_server::weapons::Fire` / `Inventory::command`:

- SMG1: held, a shot every 0.075 s, the timer advancing from its own expired value (reads 0.0583 after each shot: -0.0167 + 0.075); 1 bullet, cone 0.0436 (2.5 degrees); 45-round clip.
- Shotgun: held, 7 pellets, cone 0.0872 (5 degrees), timer 0.3333 from now; once it expires the pump takes 0.5333; a shot every 59 ticks.
- .357: 0.75 from now per shot, no spread; deploy 0.25 (measured here, the switch recording had no .357).
- Pistol: a fresh press fires once 0.1 s passed since the last shot even while the 0.5 cycle runs (taps every other tick give a shot every 8 ticks), timer 0.5 from now; held, it fires when the cycle expires, the timer advancing from its expired value (0.475 / 0.48 reads); not during deploy or reload (`busy`). Cone 1 to 6 degrees (0.00873 to 0.05234) by an accuracy penalty: +0.2 per shot, unclamped (18 shots reach 1.65), clamped to 1.5 in the cone, -0.015 per tick while the button is up and the 0.1 gate has passed (tap: 0.185 net per shot, +0.0054 cone per shot; held: no decay). The pistol parks its primary timer at -0.1 once the gate has passed with the button up (after a deploy, a shot, a dry fire, a reload, a release), once per arming. Empty clip: a press once the timer expired dry-fires (timer 0.4, gate 0.2, no bullet); with the button up it reloads. IN_RELOAD with a non-full clip, or the empty-clip reload: 1.4333 s, the clip fills from the reserve when it expires (18 after 17: reserve 256 -> 255; after 0: -> 238).

Not measured: AR2, crossbow, RPG, frag, crowbar/physcannon melee, secondary fire, the damage per bullet (the recording's `Damage` is 0, the ammo type's damage applies on hit). Harnesses `fire_pistol_tap`, `fire_pistol_hold`, `fire_pistol_empty`, `fire_pistol_reload`, `fire_smg1_hold`, `fire_shotgun_hold`, `fire_357_tap`: clip, reserve, shot count every tick, timers within 1 ms, pellets and cone within 3e-4: equal.

## Bullets on players

Recordings `hit_*` (a bot shoots a second bot standing 200 u ahead with 1000 health; every `EntityTakeDamage` / `ScalePlayerDamage` on the target, its health, armour and position per tick). Damage per bullet is the multiplayer ammo table, not the `sk_plr_dmg_*` convars the server also reports (pistol 12 vs sk 5, SMG1 12 vs 4, .357 75 vs 40, buckshot 4 per pellet vs 8): `weapons::bullet_damage`. The gamemode doubles head hits (12 -> 24, 75 -> 150), chest and stomach are unscaled, limbs unmeasured (a centre-line shot passes between the legs: no hit at knee or foot height, none above the head at 71 u). The hit group comes from the model's hit boxes: `gm_formats::mdl::hitboxes` reads the default set (kleiner: 17 boxes, head / spine / pelvis / arms / thighs / calves / feet / toes), `weapons::HitModel` places them in the bind pose (bone chain of `pos`/`quat`; the model faces -y in it, so a player carries it rotated by view yaw + 90) and traces the bullet against the boxes, the world blocking first. That reproduces the measured groups and misses (the thighs leave a 1.2 u gap at the centre line, so a knee or foot shot on the centre line passes between the legs) with hit positions 1.6 to 8.4 u from the game's. The residual is the pose: the game's standing idle differs from the bind pose (the head hit lands 5.7 u in front of the bind head box, and a shot at 71 u that the game misses hits the bind head whose top is 73.3), open until animation is read: `models/player/kleiner.mdl` carries a single sequence (`ragdoll`) and no include models, the player animations live in the gamemode's shared animation models (`models/m_anm.mdl` / `f_anm.mdl`, applied to the ValveBiped skeleton by bone name), so the idle pose needs their sequence and animation data (`mstudioseqdesc_t`, `mstudioanimdesc_t`, compressed frames) decoded and mapped onto the player model's bones. Measured so far: `m_anm.mdl` has 56 ValveBiped bones, 464 sequences and 1563 animations; a standing player with a weapon plays `idle_<hold type>` (`idle_melee` for the crowbar, 30 frames at 30 fps), the base idle is `idle_all_01` (280 frames, sectioned by 30); their frame data sits in external animation blocks (`models/m_anm.ani`, blocks 2 and 55), not in the `.mdl`, so the reader needs the header's animation block table and the `.ani` file. Without a hit model (`Server::hit_model` None) `weapons::hitgroup_at` falls back to height bands measured on kleiner (head >= 58 u, chest 44-58, stomach 26-44). The hit happens in the tick of the shot. Armour against bullets: `min(0.8 d, armour)` with the armour kept as a float and read truncated (50 - 19.2 -> 30, 30 - 19.2 -> 10), the rest to health with the fraction carried. Knock-back: a horizontal push along the aim of 4.18 u/s per point of scaled damage (24 -> 100.4, 12 -> 50.1, 150 -> 636), added after the target's friction and before its move, so the full push shows in the hit tick and decays by the walk friction (8, stop speed 10) afterwards; the .357 reads +1.5 % and the shotgun -2 % against the linear law (open). Spread is random in the game, so the harness compares the first hit of a spread weapon exactly and later hits only where the game's bullets landed; the shotgun's pellets only by hit tick. Harnesses `hit_pistol`, `hit_pistol_armor`, `hit_pistol_chest`, `hit_pistol_stomach`, `hit_pistol_legs`, `hit_pistol_feet`, `hit_357`, `hit_smg1`, `hit_shotgun` equal; `hit_pistol_high` red (the bind head box reaches a shot the idle pose misses).
