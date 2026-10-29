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
