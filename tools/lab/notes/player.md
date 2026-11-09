# Garry's Mod `player` subsystem: behaviour note

Scope: player health / armour / damage, death flow, respawn, spawn loadout, flashlight, suit power, +use and prop pickup, model and hands, spawn point selection, teams and scoreboard data, hints and notifications, ammo definitions.

Sources and tags used on every fact:
- `[bin-server]` pseudo-C of the Linux dedicated server binary (symbols present). This is the authority for what GMod really does.
- `[bin-client]` pseudo-C of the Windows client binary (no symbols, selected by strings / RTTI).
- `[sdk]` Source SDK 2013 (`src/game/...` of the current single-branch repo), used for constants the binary only holds as unresolved float literals, and to state differences.
- `[lua]` GMod's shipped Lua (`garrysmod/gamemodes/base`, `sandbox`, `lua/includes`), which implements a large part of this subsystem through gamemode hooks called from the binary.
- `[inf]` inference. Confidence is `high` / `med` / `low`.

Units: Hammer units (u) for distance, u/s for speed, seconds for time, Source damage points for damage, kg for physics mass. Tick rate of a GMod server is 66.67 Hz (tick 0.015 s).

A general rule that shapes everything below: the server binary delegates most player gameplay decisions to Lua gamemode hooks. Where the binary calls a hook, the hook's name is given (hook names are public GMod API, not binary identifiers), and the default behaviour of the base / sandbox gamemode is given from `[lua]`. A rewrite must reproduce the hook's default outcome, and ideally the hook point itself.

---

## 1. Health, armour, damage formulas

### 1.1 Defaults
- F1. Max health 100, start health 100, max armour 100, start armour 0 (player class `player_default`; sandbox class inherits them). `[lua]` high
- F2. The binary stores max armour in a networked player field that the spawn routine sets to 100 and that `Player:SetMaxArmor` changes; the base engine "armour value at spawn" virtual returns 0. `[bin-server]` high
- F3. Health is networked as an integer; damage accumulation keeps fractions (see F18). `[bin-server]`/`[sdk]` high

### 1.2 Damage pipeline on the server (order of operations)
Per incoming damage to a living player:
- F4. Multiplayer-layer filter first: if the damage type contains `DMG_BLAST` (0x40) and the player's blast-immunity timer (set by the SLAM / tripmine code) is still running, the damage is dropped. Otherwise each hit's bullet force is accumulated (per component, clamped) into a "total bullet force" vector that is later used as the ragdoll force on death. `[bin-server]` high (matches `[sdk]` HL2MP)
- F5. HL2-layer: `DMG_FALL` damage is ignored while the "ignore fall damage" timer (map input `IgnoreFallDamage`) is running; blast damage from under water / surface rules as in HL2; last damage time recorded; friendly NPCs are notified of damage; then the skill scaling: if the attacker is an NPC and the type has none of `DMG_CRUSH | DMG_FALL | DMG_DROWN | DMG_POISON | DMG_SNIPER`, damage is multiplied by `sk_dmg_take_scale1/2/3` picked by `skill` (1..3). `[bin-server]` high (SDK applies the same convars)
- F6. Base-layer gate: a damage filter entity on the player may refuse; game rules `PlayerShouldTakeDamage(ply, attacker)` hook (default true) may refuse; with the player's "buddha" bit set the damage is clamped so health never drops below 1; `player_debug_print_damage` prints `"<time>: Player <name> at [x y z] took <dmg> damage from <ent>, type <types>"`. `[bin-server]` high
- F7. Armour: the SDK's C++ armour formula is removed from the binary. Instead the binary calls the Lua hook `HandlePlayerArmorReduction(ply, dmginfo)` exactly where the SDK did the armour math, records the armour difference as the "damage saved" statistic, then continues. `[bin-server]` high. Default Lua implementation `[lua]` high:
  - skipped if armour <= 0 or the type has any of `DMG_FALL | DMG_DROWN | DMG_POISON | DMG_RADIATION`;
  - `ratio = 0.2` (armour absorbs 80 %), `bonus = 1.0`; with convar `player_old_armor 1`: `bonus = 0.5`;
  - `newDamage = dmg * 0.2`; `armourCost = (dmg - newDamage) * bonus`;
  - with `player_old_armor 0`: `armourCost < 0.1 -> 0`, `0.1 <= armourCost < 1 -> 1`;
  - if `armourCost > armour`: `armourCost = armour / bonus`, `newDamage = dmg - armourCost`, armour = 0; else armour -= armourCost;
  - damage set to `newDamage`. Armour is stored as an integer by `SetArmor`, so a 30-damage hit at armour 100 leaves armour 76 and health -6; a 50 hit at armour 10 leaves armour 0, health -40 (matches the Rust team's own measurement). `[lua]`+`[inf]` high
- F8. Then the combat-character layer subtracts the (possibly reduced) damage from health (fraction carried in the entity's float health accumulator, F18), records last damage origin / inflictor, and runs the "alive" damage handler: knock-back impulse, suit speech, hurt event (F9..F12). `[bin-server]` high
- F9. Knock-back: when attacker and inflictor exist, the player's move type is WALK, and the damage is not `DMG_FALL`-style, the player gets `force = dir * damage * (48*48*82 / hullVolume) * scale` (SDK formula, `scale` 1 for damage >= 50 style rules), clamped so the vertical part cannot exceed a fixed limit; direction is from inflictor centre to player centre. `[bin-server]` med (formula shape from SDK, constants unresolved in dump); the Rust team measured 4.18 u/s per damage point for bullets. `[rust-measured]`
- F10. Suit speech on damage (only with the suit equipped, which in GMod is always, F60): the damage type bits map to `!HEV_DMG0..7` / `!HEV_DET0..2` sentences (burn, shock, poison, radiation, acid, chemical, fall...) and health thresholds queue `!HEV_HLTH1` (health < 30 and other conditions), `!HEV_HLTH2`, `!HEV_HLTH3` (critical), `!HEV_MED1`, `!HEV_HEAL7`. These are the SDK's tables; the binary keeps them. `[bin-server]` high. Note: with `gmod_suit 0` (default) the suit still counts as equipped, so these sentences do play in GMod (the HEV voice on damage is audible in stock sandbox). `[bin-server]`+`[inf]` med
- F11. After the health change the Lua hook `PlayerHurt(ply, attacker, healthRemaining, damageTaken)` is called and the game event `player_hurt` (`userid`, `health`, `attacker`, `priority`) is fired. `[bin-server]` high
- F12. If the attacker is an NPC, a combat sound (`SOUND_COMBAT`, radius 512, duration 0.5 s) is inserted at the player for the AI sound system. `[bin-server]` high
- F13. Explosion damage (`DMG_BLAST`) calls the player's "damaged by explosion" handler, which in GMod only calls a Lua hook with (ply, dmginfo); the SDK's screen shake (amplitude 4, freq 1, 0.5 s, radius 1000) and ear-ring are not in the binary. `[bin-server]` high
- F14. Damage taken while dying/dead: the dying handler only accumulates; nothing above runs. `[bin-server]` high

### 1.3 Hit groups and the head multiplier
- F15. The GMod player's trace-attack override stores the hit group in a networked field (`Player:LastHitGroup`), calls the Lua hook `PlayerTraceAttack(ply, dmginfo, dir, trace)` (true = handled, stop), and if the player takes damage calls `ScalePlayerDamage(ply, hitgroup, dmginfo)` (true = stop, no blood), then adds the damage to the multi-damage accumulator and spawns blood (`SpawnBlood` at the hit position pulled back ~4 u along the shot direction, blood colour of the player, then `TraceBleed`). It does NOT call the base player trace-attack, so the SDK convars `sk_player_head/chest/stomach/arm/leg` are never applied to players in GMod, even though `skill.cfg` ships them (`sk_player_head 3` in GMod's cfg; SDK code default 2). `[bin-server]` high, `[lua]` cfg high
- F16. Default `ScalePlayerDamage`: `HITGROUP_HEAD (1)` -> damage x2; `HITGROUP_LEFTARM (4)`, `RIGHTARM (5)`, `LEFTLEG (6)`, `RIGHTLEG (7)`, `GEAR (10)` -> x0.25; chest (2) and stomach (3) x1. `[lua]` high
- F17. Hit group indices on the player: 1 head, 2 chest, 3 stomach, 4/5 arms, 6/7 legs, 10 gear (from the model's hitbox sets). `[bin-server]`/`[sdk]` high

### 1.4 Rounding of health
- F18. Health is subtracted as a float and stored truncated; the base combat character keeps the fractional remainder, so repeated 0.4 hits eventually take a whole point. The Rust team measured this plus "blast damage rounded to nearest" (rockets 111.9 -> 112). The pseudo-C of the generic take-damage path is consistent with a float accumulator; the blast rounding was not located in the dump (it may come from the explosion code, outside this subsystem). `[bin-server]` med, `[rust-measured]` high

### 1.5 Multiplayer damage table (ammo damage to players)
- F19. The ammo damage rule in GMod's game rules: given attacker, victim and ammo index, if the ammo's damage-type bits contain the GMod-specific flag `0x40000000` the rule returns the ammo's **player** damage when the victim is a player and the **NPC** damage otherwise; without that flag it returns the SDK rule (attacker is a player -> player damage value, else NPC damage value). If the attacker is a player and the victim is an NPC the result is additionally multiplied by `sk_dmg_inflict_scale<skill>`. `[bin-server]` high
- F20. Per-ammo player / NPC damage values are convar-backed (`CAmmoDef` entries created with convar names), so the effective multiplayer table is the `sk_plr_dmg_*` / `sk_npc_dmg_*` convars of `skill.cfg`: pistol 5, SMG1 4, AR2 8, buckshot 8 per pellet, .357 40, crossbow bolt 100, RPG round 100, SMG1 grenade 100, frag grenade 125 (`sk_plr_dmg_fraggrenade`; `sk_plr_dmg_grenade` 150 is the thrown HL2 grenade), crowbar 10, stunstick 10, sniper round 20, airboat gun 3. Max carry: pistol 150, SMG1 225, AR2 60, AR2 alt-fire 3, buckshot 30, .357 12, crossbow 10, RPG 3, SMG1 grenade 3, grenade 5, sniper 30. `[lua]` (cfg) high; the binary's ammo-definition constructor is not in the dump, so the exact list of ammo rows and their damage-type flags comes from `[sdk]` HL2MP: AR2 (bullet, `BULLET_IMPULSE(200,1225)`), AR2AltFire (dissolve), Pistol, SMG1, 357 (`BULLET_IMPULSE(800,5000)`), XBowBolt (`BULLET_IMPULSE(800,8000)`), Buckshot (bullet | buckshot, `BULLET_IMPULSE(400,1200)`), RPG_Round (burn), SMG1_Grenade (burn), Grenade (burn), slam (burn). `[sdk]` med (GMod adds Lua `game.AddAmmoType` rows at runtime; `0x40000000` flag rows are GMod additions)
- F21. GMod extension of the ammo table: a per-ammo "GMod max carry" accessor returns the stored max carry, or when it is -1 reads the value from the ammo's associated convar (so `gmod_maxammo` / per-ammo convars override). `[bin-server]` med
- F22. Ammo quantity scaling by skill: `sk_ammo_qty_scale1/2/3` (SDK defaults 1.2 / 1.0 / 0.6) applied to ammo pickups. `[bin-server]` high (convar names), defaults `[sdk]` med
- F23. Giving ammo: the HL2 layer rejects negative ammo indices; the GMod layer then, if anything was added and the "suppress HUD" flag is not set, sends the user message `AmmoPickup(ammoName, amount)` to that player only (reliable), driving the HUD pickup history. `[bin-server]` high
- F24. Weapon pickup: a weapon being equipped is first forced out of any physgun / physcannon hold, the Lua hook `WeaponEquip(weapon, owner)` fires, HL2 equip runs, and unless suppressed the user message `WeaponPickup(ehandle)` goes to the player. `BumpWeapon` (walking into a weapon) first asks `PlayerCanPickupWeapon(ply, weapon)` (default true). `[bin-server]` high, `[lua]` high

### 1.6 Fall damage
- F25. Game rules fall damage asks the Lua hook `GetFallDamage(ply, fallSpeed)`; without a result it returns a constant that the SDK sets to 10. `[bin-server]` high
- F26. Default hook: `mp_falldamage 1` -> `(speed - 526.5) * (100 / 396)` (= 25/99 per u/s above 526.5 u/s), `mp_falldamage 0` -> 10 flat. Threshold for any fall damage is the engine's safe fall speed 526.5 u/s (fatal 922.5 u/s only matters for the old progressive rule). `[lua]` high, `[sdk]` high
- F27. The death sound differs for fall deaths: `Player.FallGib` instead of `Player.Death` (F33). `[bin-server]` high
- F28. Game rules "fall death does screen fade" returns false in GMod (SDK returns true for singleplayer HL2). `[bin-server]` high

### 1.7 Healing / health pickups
- F29. `TakeHealth(amount, typeBits)`: clears the time-based damage bits given (except those the rules keep), then combat-character heal clamps to max health. `[bin-server]` high
- F30. Game rules "NPC should drop health" (companion drop rule): after a timer gate, drop if `random(0,1) > health/maxHealth * K` (K unresolved; SDK 1.0 scaled by skill). `[bin-server]` low

---

## 2. Death flow

### 2.1 Server state machine
- F31. Kill sequence (GMod override of the killed handler), in order: `[bin-server]` high
  1. Damage force replaced by the accumulated bullet force (F4) when finite.
  2. The held +use prop (if any) is dropped (handle cleared).
  3. Life state set to DYING (1). Lua hook `DoPlayerDeath(ply, attacker, dmginfo)` (this is where the ragdoll is created, see F35).
  4. Active weapon: holstered; if `Player:ShouldDropWeapon(true)` was set the weapon is dropped into the world, otherwise the active weapon entity is removed and the active weapon cleared. Other weapons stay in the inventory until the death think packs them (F38).
  5. Tripmines of the player detonate.
  6. HL2 killed handler -> base killed handler (F32), then water death sounds stop, the `PlayerDied` logic_playerproxy output fires, scripted scenes are notified.
  7. If the damage type contains `DMG_DISSOLVE`, the ragdoll handle (if any) is dissolved (normal dissolve type).
  8. Flashlight turned off (again), life state set to DEAD (2), `FL_FROZEN` cleared, transmit state refreshed, zoom stopped, forced buttons cleared, all buttons re-enabled, Lua hook `PostPlayerDeath(ply)`.
- F32. Base killed handler: rules `PlayerKilled`, use entity cleared, health clamped to 0 if below -99, active weapon holstered, dead view offset = rules dead view height scaled by model scale (unless observer), life state DYING, `deadflag`, `FSOLID_NOT_SOLID`, move type FLYGRAVITY (5), ground entity cleared, suit speech queue cleared, FOV reset to 0, flashlight off, death time = now, then the combat-character handler (which fires `Event_Dying`: death sound, leave vehicle, pitch/roll zeroed, death think scheduled in 0.1 s). `[bin-server]` high (matches `[sdk]`)
- F33. Death sound: skipped if the player's ragdoll is dissolving; Lua hook `PlayerDeathSound(ply)` returning true suppresses; otherwise `Player.FallGib` when the damage bits include `DMG_FALL`, else `Player.Death`; with the suit equipped one sentence from the `HEV_DEAD` group is queued (plays as the suit voice). `[bin-server]` high; default hook returns false `[lua]` high
- F34. GMod's base player "should fade on death" virtual returns false: there is **no** screen fade on death from the engine side. Any death fade in GMod comes from Lua, and the base / sandbox gamemodes add none. `[bin-server]` high. On (re)spawn the binary sends a screen fade with duration 0, hold 0 and flags `FFADE_IN | FFADE_PURGE` (0x11), which clears any fade still on screen. `[bin-server]` high
- F35. Ragdoll: the base player "create ragdoll entity" virtual in the binary is empty. The ragdoll is created by Lua: `DoPlayerDeath` calls `ply:CreateRagdoll()` unless the damage type contains `DMG_REMOVENORAGDOLL`, then `AddDeaths(1)`, and if the attacker is a player: `attacker:AddFrags(-1)` for suicide, else `+1`. `[bin-server]` high, `[lua]` high. The resulting entity is `hl2mp_ragdoll` (client-side ragdoll driven from the networked entity; stands at the death origin with zero angles, velocity from the kill force) `[rust-measured]`+`[inf]` med. The previous ragdoll is removed on the next spawn (F42). `[bin-server]` high
- F36. Base gamemode `PlayerDeath(ply, inflictor, attacker)` (called by the binary after the kill): `NextSpawnTime = CurTime() + 2`, `DeathTime = CurTime()`; if the attacker is a `trigger_hurt` it is replaced by the victim; invalid inflictor -> attacker; a death notice is sent (friendly-attacker flag when an NPC attacker likes the victim); server console prints `"<nick> was killed by <attackerClass>"`. `PlayerSilentDeath(ply)` (from `KillSilent`) sets the same two timers. `[lua]` high
- F37. `Player:Kill()`: unfreezes, then runs the full damage path with a self-inflicted zero-damage `DMG_NEVERGIB` (0x1000) info (take-damage, then the killed handler), so ragdoll / sounds / hooks all happen. `Player:KillSilent()`: Lua hook `PlayerSilentDeath`, teleport to the origin with zero angles, ragdoll / sounds skipped. `[bin-server]` high

### 2.2 Death think and respawn
- F38. Death think (GMod override), every 0.1 s after death: `[bin-server]` high
  1. `EF_NODRAW` added to the player.
  2. If the player still has weapons, dead-player items are packed: rules return `GR_PLR_DROP_GUN_ACTIVE` and `GR_PLR_DROP_AMMO_ACTIVE` (same as SDK HL2MP). Since the active weapon was already removed/dropped in F31, in practice the remaining weapons are deleted the tick after death. `[bin-server]` high, `[rust-measured]` consistent
  3. While life state is DYING and the model's death animation has not finished: advance the animation and count frames; return until 60 frames (6 s max) or the sequence ends.
  4. Life state DYING -> DEAD, animation stopped, playback rate 0.
  5. The death-think random seed is advanced (`RandomSeed`).
  6. Unless `FL_FROZEN` is set on the player, Lua hook `PlayerDeathThink(ply)` is called.
  7. If the player is now alive (the hook spawned them) the button state and frame counter are reset and the next think is cleared; otherwise the think repeats in 0.1 s.
  The multiplayer layer skips the whole think while the player is in observer mode. `[bin-server]` high
- F39. Rules: "player can respawn" returns true always; "player spawn time" returns the current time (no engine delay). The whole respawn delay lives in Lua. `[bin-server]` high
- F40. Default `PlayerDeathThink`: return while `NextSpawnTime` (death + 2 s) is in the future; then if the player is a bot, or pressed (`KeyPressed`, edge) `IN_ATTACK`, `IN_ATTACK2` or `IN_JUMP`, call `ply:Spawn()`. Because the think runs every 0.1 s and `KeyPressed` is an edge, a human must press one of those keys after the 2 s; a bot respawns on the first think after 2 s (the Rust team measured 135 ticks = 2.025 s). `[lua]` high, `[rust-measured]` high
- F41. Forced respawn / observer: the SDK's `forcerespawn` / 5 s observer switch in the base death think is not reached because GMod's override replaces the base think entirely. `[bin-server]` high

### 2.3 Spawn
- F42. Spawn routine (GMod override): `[bin-server]` high
  1. Suit equipped (always, regardless of `gmod_suit`).
  2. Move type WALK unless in observer mode.
  3. HL2MP spawn: SDK body (dead flag cleared, `NOT_SOLID` / `NODRAW` removed, `FL_ONGROUND`, default items via `GiveDefaultItems`, model, HUD unhidden). GMod's HL2MP spawn is not in the dump; the loadout observed in GMod is the Lua one (F45), so `GiveDefaultItems` is either empty or overridden in GMod. `[inf]` med
  4. Max health field from the max-health virtual; max armour field set to 100 (unless observer).
  5. Lua hook `PlayerSpawn(ply, transition)`.
  6. The previous death ragdoll handle is removed.
  7. Achievement counters ("spawn" achievements 1 = SP, 2 = MP) bumped.
  8. Vehicle crosshair handling if spawning inside a vehicle.
  9. Collision group set to 10 (the group the GMod player uses for player-vs-player collision rules; SDK's `COLLISION_GROUP_PLAYER` is 5 and `..._PLAYER_MOVEMENT` is 10) `[bin-server]` high, meaning `[inf]` med.
  10. Spawn fade purge (F34).
- F43. Default `PlayerSpawn` (base gamemode): team-based gamemodes spawn spectators / unassigned as spectators; `UnSpectate` unless transition; `player_manager.OnPlayerSpawn`; player class `Spawn`; `PlayerLoadout` (not on level transition); `PlayerSetModel`; `SetupHands`. `[lua]` high
- F44. Initial spawn: the binary sets the model `models/player.mdl`, records the connect time, then the base initial spawn. Lua `PlayerInitialSpawn` sets team `TEAM_UNASSIGNED` (1001) and, in team-based gamemodes, runs `gm_showteam` on the client. `[bin-server]` high, `[lua]` high
- F45. Loadout. Base class: `weapon_pistol` + 255 Pistol ammo. Sandbox class: `RemoveAllAmmo`; if `sbox_weapons` (default 1): ammo Pistol 256, SMG1 256, grenade 5, Buckshot 64, 357 32, XBowBolt 32, AR2AltFire 6, AR2 100 (all given silently: no `AmmoPickup` HUD history), weapons `weapon_crowbar`, `weapon_pistol`, `weapon_smg1`, `weapon_frag`, `weapon_physcannon`, `weapon_crossbow`, `weapon_shotgun`, `weapon_357`, `weapon_rpg`, `weapon_ar2`; always `gmod_tool`, `gmod_camera`, `weapon_physgun`; then `SwitchToDefaultWeapon` selects the weapon named by the client's `cl_defaultweapon` userinfo if owned (so the active weapon after spawn depends on the client; with an unset / unknown value the last given weapon, the physgun, stays selected). Max carry caps apply (F20), so 256 Pistol becomes 150 etc. `[lua]` high, `[inf]` med on the default active weapon
- F46. `impulse 101` ("give all"): crowbar, pistol, 357, smg1, ar2, shotgun, frag, crossbow, rpg, slam, physcannon, physgun. `[bin-server]` high

### 2.4 Death camera (client)
- F47. The GMod server-side view calc is the plain base player one; the death camera is the HL2MP client view: while life state is not ALIVE and not observer, the eye origin becomes the ragdoll's origin plus the dead view height (if a representative ragdoll exists, otherwise the player's eye position), the view angles are the normal (frozen, last) angles, and the camera is pulled back `CHASE_CAM_DISTANCE_MAX = 96 u` along the view direction with a hull trace of half-size `WALL_OFFSET` against brushes only (`MASK_SOLID_BRUSHONLY`); if blocked the camera stops at the trace end. There is no interpolation over time: the pull-back is immediate. `[bin-server]` (server side) high, `[sdk]` for the client body med (the client dump contains the GMod player vtable functions but they are unnamed; no 96-unit float literal was found in them, so a GMod change here cannot be excluded). Dead view height is the rules' view vectors value (HL2: `VEC_DEAD_VIEWHEIGHT` = 14 u) scaled by model scale. `[sdk]` med

---

## 3. Flashlight

- F48. Turning on (HL2MP layer used by GMod): Lua hook `PlayerSwitchFlashlight(ply, true)` returning false aborts. The flashlight suit device is (re)registered. Then if the "legacy flashlight" mode is off or the device was added: the server convar `mp_flashlight` (the binary's flashlight-allow convar, also used by the rules' "allow flashlight" query) must be > 0, the player alive, and the `EF_DIMLIGHT` (4) effect not yet set; then `EF_DIMLIGHT` is added, sound `HL2Player.FlashlightOn` emitted at the player, and the player-proxy output `OnFlashlightOn` fired. `[bin-server]` high (convar identity `mp_flashlight` med: the binary object is named `flashlight`; the SDK name is `mp_flashlight`)
- F49. Turning off: hook `PlayerSwitchFlashlight(ply, false)`; output `OnFlashlightOff`; device removed; in legacy mode, if the device was not active, return; if alive and `EF_DIMLIGHT` set: sound `HL2Player.FlashlightOff`; `EF_DIMLIGHT` cleared. `[bin-server]` high
- F50. "Is on" = the `EF_DIMLIGHT` bit of the effects field (networked; the client renders the projected light from it). `Player:Flashlight(bool)` maps to the on / off routines; `impulse 100` toggles via the same path. `[bin-server]` high
- F51. Lua-side allowance: `Player:AllowFlashlight(bool)` / `CanUseFlashlight()`; the class flag `CanUseFlashlight` is true for the default and sandbox classes. The `impulse 100` handler checks the rules' allow flag and the player's Lua allowance (the latter via the hook). `[lua]` high, `[inf]` med
- F52. The flashlight is forced off on death (both in the base killed handler and again at the end of the GMod killed handler). `[bin-server]` high
- F53. Flashlight-on-NPC check: every 0.25 s while on, NPCs whose eyes are lit get a "player flashlight on my eyes" notification (Alyx, Hunter react). `[bin-server]` high (interval `[sdk]` med)
- F54. Suit power drain of the flashlight: the device rate is the SDK's 2.222 /s (1.111 /s in episodic mode) but in GMod the drain only happens with `gmod_suit 1` (F57). `[bin-server]` high, rates `[sdk]` med

---

## 4. Suit power (aux power) and sprint

- F55. Initialisation: active devices 0, power 100.0, device load 0. `[bin-server]` high
- F56. Charge(amount): power += amount, capped at 100. Charge rate is a constant the SDK sets to 12.5 /s (100 in 8 s). `[bin-server]` high, rate `[sdk]` med
- F57. Drain(amount): if `sv_infinite_aux_power` != 0: nothing, report "has power". If `gmod_suit` == 0 (GMod default): nothing, report "has power". Otherwise power -= amount; if it goes below 0 it is clamped to 0 and "out of power" is reported. Consequence: in stock GMod the aux power bar never moves and sprint / flashlight never run out. `[bin-server]` high
- F58. Should recharge: with `gmod_suit 0` always true (so the update path calls Charge every frame, harmless at 100). With `gmod_suit 1`: no active devices, power < 100, and now >= (time all devices went off + a delay, SDK 0.5 s). `[bin-server]` high, delay `[sdk]` med
- F59. Update (per think): if should-recharge -> charge(rate * frametime). Else sum the active devices' rates (sprint device only counts while the player is actually moving: the absolute velocity is recomputed and checked; flashlight rate as F54 with the episodic halving when `hl2_episodic` and the rule says singleplayer), drain(sum * frametime); when drain reports out of power: stop sprinting if sprinting, and in legacy flashlight mode turn the flashlight off. SDK device rates: sprint 25 /s (100 in 4 s), flashlight 2.222 /s, breather 6.7 /s. `[bin-server]` high, rates `[sdk]` med
- F60. The suit is always equipped (F42 step 1), so the HUD aux power element, suit voice and suit-dependent HL2 behaviours are active; `gmod_suit` only switches the drain, the +use sound/deny (F66) and the Lua side on. `[bin-server]` high
- F61. Sprint start (GMod override): if power < 10 (constant shared with the fall damage default, SDK value 10) and the sprint-enabled flag is set: play `HL2Player.SprintNoPower` through a PAS attenuation filter (attenuation 0.8, prediction rules) and return. Otherwise, with `gmod_suit` != 0, register the sprint device (failure returns); play `HL2Player.SprintStart` the same way; set the networked max speed to the player's run speed; set the sprinting flag. Stop sprinting: remove the device, max speed = walk speed, clear the flag. `[bin-server]` high
- F62. Can sprint = sprint-enabled flag and not ducking (`FL_DUCKING`). Speed changes each command: `IN_SPEED` held and can-sprint -> stop walking, start sprinting; `IN_WALK` held and walk allowed -> stop sprinting, start walking; otherwise stop both. `[bin-server]` high
- F63. Speeds: class `player_default` walk 400, run 600, slow-walk (+walk) 200, crouch factor 0.3, duck/unduck 0.3 s, jump 200; sandbox class walk 200, run 400, slow-walk 100, duck/unduck 0.1 s. `[lua]` high

---

## 5. Use key and prop pickup

- F64. `+use` processing (GMod override of the per-frame use handler), only when `IN_USE` (0x20) is in buttons, pressed or released: `[bin-server]` high
  1. If the active weapon is not the physgun and `IN_ATTACK | IN_ATTACK2` is in any of buttons / pressed / released, abort (prevents use while firing; the physgun exempts this so right-click-drop + use works).
  2. On press: if a use entity is already held (continuous use), it is cleared and the handler returns. On a train (`PFLAG_ONTRAIN`): press plays `HL2Player.TrainUse` and detaches; return.
  3. Observer move type -> return.
  4. Find the use entity (F65) then Lua hook `FindUseEntity(ply, ent)` may replace it (returned entity wins).
  5. Nothing found and the key was pressed this frame and `gmod_suit` != 0: set the deny flag; the item-post-frame emits `HL2Player.UseDeny` once. (Stock GMod: no deny sound.)
  6. Found: Lua hook `PlayerUse(ply, ent)` false -> nothing. On press and `gmod_suit` != 0: sound `HL2Player.Use` (stock GMod: silent). Then the entity's `Use` input is invoked with the player as activator / caller and use type `USE_ON` on press (continuous `USE_SET` while held for entities that accept continuous use, `USE_OFF` on release; same scheme as the SDK). `PFLAG_USING` set.
  (SDK differences: the SDK's "use suspended" timer is not checked here; the SDK plays `HL2Player.Use` whenever the target is not an NPC.) `[bin-server]` high
- F65. Find use entity: the SDK's forward trace from the eyes (`PLAYER_USE_RADIUS` 80 u reach, hull sweeps of the 5 directions, `IsUseableEntity` checks, nearest by dot product) is used unchanged; prop pickup candidates must be a `prop_physics` / `prop_ragdoll` style physics entity. `[sdk]` high (the base function is in the dump unchanged in structure; constants not resolved) med
- F66. Prop pickup through +use (GMod override): `sv_playerpickupallowed` (default 1) must be non-zero; Lua hook `AllowPlayerPickup(ply, ent)` false blocks. Then the HL2 pickup: the entity must not be the player's ground entity; the player's "can pickup object" check with **mass limit 35 kg** and **size limit 128 u** must pass: the entity has VPhysics (move type VPHYSICS), the summed mass of all its physics objects <= 35, no object has `FVPHYSICS_NO_PLAYER_PICKUP`, hinged/constrained-to-world objects are refused, each OBB dimension <= 128 u; bounce bombs are allowed as a special case; no NPC may be standing on it; the player must not already hold something. On success the player-pickup controller (the same one the gravity gun uses for carried objects) takes it: held in front of the player, dropped on `+use` again, when it collides hard, or when out of reach. `[bin-server]` high (numbers 35 / 128 are literal in the call), SDK carry controller details `[sdk]` med; the Rust team measured hold point on the mass centre, 2500 u/s and 3600 deg/s shadow caps, yaw-only turn. `[rust-measured]`
- F67. Held object mass query and use-deny are plain HL2 helpers; "suspend use" stores now + duration (used by vehicles / physcannon to block +use briefly). `[bin-server]` high
- F68. While holding a prop the player cannot attack (the "can attack" virtual returns false while the pickup entity handle is valid). `[bin-server]` high
- F69. Weapon equip forces the weapon out of a physgun / physcannon hold (F24). `[bin-server]` high

---

## 6. Player model, colours, hands, view model

- F70. Initial model `models/player.mdl` (binary), replaced on every spawn by Lua `PlayerSetModel` -> class `SetModel`: `player_manager.TranslatePlayerModel(cl_playermodel userinfo)`, default `models/player/kleiner.mdl` when the name is unknown; sandbox class also applies `cl_playerskin` (skin index) and `cl_playerbodygroups` (space-separated per-group values). `[bin-server]` high, `[lua]` high
- F71. Player colour / weapon colour: networked vectors with Lua setters; sandbox class sets them on spawn from userinfo `cl_playercolor` (default `0.24 0.34 0.41`) and `cl_weaponcolor` (default `0.30 1.80 2.10`), values are 0..1 (weapon colour may exceed 1 for glow). `[bin-server]` high (netvar accessors), `[lua]` high
- F72. Hands: `Player:SetupHands()` (Lua helper) creates a `gmod_hands` entity attached to the view model; `PlayerSetHandsModel(ply, hands)`: class `GetHandsModel` or `player_manager.TranslatePlayerHands(TranslateToPlayerModelName(model))` gives `{model, skin, body, matchBodySkin}`; model, skin (player's skin when `matchBodySkin`) and bodygroups are applied. Classes have `UseVMHands = true`. `[lua]` high
- F73. View models: the binary creates `predicted_viewmodel` entities (indices 0..2) that follow the player. `[bin-server]` high
- F74. Hull: standing 32 x 32 x 72 u, ducked 32 x 32 x 36 u; view height 64 u standing, 28 u ducked, dead 14 u (HL2 view vectors). `[sdk]` high (GMod uses the HL2MP rules view vectors) med

---

## 7. Spawn point selection

- F75. The GMod spawn-point virtual is pure Lua: hook `PlayerSelectSpawn(ply, transition)` must return an entity; with no gamemode loaded it returns null (no C++ fallback in the override; the base-class fallback exists but is not reached). `[bin-server]` high
- F76. Default `PlayerSelectSpawn`: on level transition the player keeps the engine spawn; team-based gamemodes with team spawn points: up to 6 random picks from `team.GetSpawnPoints(team)`, each tested with `IsSpawnpointSuitable(ply, ent, forceOnLast)`. Otherwise the gamemode caches every entity of the classes `info_player_start`, `info_player_deathmatch`, `info_player_combine`, `info_player_rebel`, `info_player_counterterrorist`, `info_player_terrorist`, `info_player_axis`, `info_player_allies`, `gmod_player_start`, `info_player_teamspawn`, `ins_spawnpoint`, `aoc_spawnpoint`, `dys_spawn_point`, `info_player_pirate`, `info_player_viking`, `info_player_knight`, `diprip_start_team_blue`, `diprip_start_team_red`, `info_player_red`, `info_player_blue`, `info_player_coop`, `info_player_human`, `info_player_zombie`, `info_player_zombiemaster`, `info_player_fof`, `info_survivor_rescue` (plus names from `gmod_player_start` keyvalues); if any has spawn flag 1 ("master") it is preferred when suitable; else a uniformly random point is tried up to `count` times with `IsSpawnpointSuitable` (forced on the last try); no points -> console error `[PlayerSelectSpawn] Error! No spawn points!`. `[lua]` high (class list is the shipped Lua list; `[lua]` high)
- F77. `IsSpawnpointSuitable(ply, spawn, makeSuitable)`: the spawn's position must not be blocked by another living player's hull (32 x 32 x 72 box test); when `makeSuitable` is true, blocking players are killed (`Kill`) so the spawn can be used. `[lua]` high
- F78. The player is placed at the spawn entity's origin + 1 u up with the entity's angles; the Rust team measured this. `[rust-measured]` high, `[sdk]` (base spawn applies `+1` z) high

---

## 8. Teams, scoreboard and player resource

- F79. Teams are Lua: `TEAM_CONNECTING 0`, `TEAM_UNASSIGNED 1001`, `TEAM_SPECTATOR 1002` with default colour (255, 255, 100); `team.SetUp(id, name, color, joinable)` adds teams; `Player:SetTeam` writes the networked team number through the base `ChangeTeam(team, silent=false, autoTeam=false)`. The HL2MP C++ team logic (combine/rebels) is not used by GMod rules (`IsTeamplay` of GMod rules is false). `[lua]` high, `[bin-server]` high
- F80. Frags / deaths live on the player entity (networked ints) and are changed only by Lua (`AddFrags`, `AddDeaths` in `DoPlayerDeath`, F35) or the SDK score code for HL2MP rules (not used). The `player_resource` entity networks per-player ping, score, deaths, connected, team, alive, health for the scoreboard. `[bin-server]` high
- F81. Default scoreboard (base `cl_scoreboard.lua`): one row per player: avatar, name (font `ScoreboardDefault`, text colour (93, 93, 93)), optional mute button, then right-docked 50 px columns Kills (`Frags()`), Deaths, Ping; team-based gamemodes group rows by team with the team colour; rows sorted by frags. Sandbox uses the same board. `[lua]` high
- F82. Death notices: `SendDeathNotice(attackerName, inflictorClass, victim, flags)` -> client `cl_deathnotice.lua` draws "attacker [weapon icon] victim" for `hud_deathnotice_time` (default 6 s) at the top right. `[lua]` high

---

## 9. Hints and notifications

- F83. Notification library (client Lua): kinds `NOTIFY_GENERIC 0`, `ERROR 1`, `UNDO 2`, `HINT 3`, `CLEANUP 4`, icons `vgui/notices/{generic,error,undo,hint,cleanup}`; `notification.AddLegacy(text, kind, lengthSeconds)` creates a panel anchored at 80 % screen height, sliding in from x = 200 px; panels fade when less than 0.7 s is left and are removed below 0.2 s; `Kill(uid)` gives 0.8 s; `AddProgress(uid, text, frac)` is a persistent (length -1) progress notice; movement is velocity based (`speed = RealFrameTime * 15`, damping `0.95 - RealFrameTime * 8`). `[lua]` high
- F84. Sandbox hints (`cl_hints.lua`): `GM:AddHint(name, delaySeconds)` schedules a one-shot timer unless the hint was already shown this session; on fire, if `cl_showhints` is on and not a demo: text `#Hint_<name>` with `%bind%` substitution, `GAMEMODE:AddNotify(text, NOTIFY_HINT, 20)` (20 s) and sound `ambient/water/drip<1..4>.wav`. Scheduled at gamemode load: `OpeningMenu` 30 s, `Annoy1` 5 s, `Annoy2` 7 s; event hints (e.g. `OpeningContext`, `EditingSpawnlists`, `ContextClick`, `PhysgunFreeze`, `PhysgunUnfreeze`, `PhysgunReload`, `NoclipUnfreeze`...) are added by their features with their own delays. `GM:SuppressHint(name)` removes a pending one. `[lua]` high
- F85. Pickup HUD: the `AmmoPickup` / `WeaponPickup` user messages (F23, F24) feed the base `cl_hudpickup.lua` history (item icon slides in on the right, fades after a few seconds). `[bin-server]` high, `[lua]` high
- F86. Health HUD (client binary): the health element re-evaluates on each change: health < 20 and > 0 plays animation `HealthIncreasedBelow20` then `HealthLow`; health >= 20 plays `HealthIncreasedAbove20`; `HealthBelow999` / `HealthOver999` switch the digit layout; `HUDQuickInfo.LowHealth` sound exists for the quick-info element. `[bin-client]` high
- F87. Damage indicator (client binary): the red directional damage flash element with the SDK's layout (`dmg_xpos 10`, `dmg_ypos 80`, `dmg_wide 30`, `dmg_tall1 300`, `dmg_tall2 240`, proportional) and the SDK's damage animation table (`HudTakeDamageLeft/Right/Behind/Front/High*`, per-type `Drown/Poison/Burn/Radiation`). `[bin-client]` high (layout), `[sdk]` high (animation table)

---

## 10. Observer mode

- F88. Start observer (GMod override): if mode != 0: when not already an observer the origin is raised by the view offset and velocity zeroed; observer flag set; suit speech cleared; ground entity cleared; `FL_ONGROUND | FL_DUCKING` removed; `FSOLID_NOT_SOLID`; `EF_NODRAW`; observer mode set; take-damage mode set to "no" (8); any held physics flag cleared; move type NONE (0). Mode 0 -> stop observer. Stop observer: HL2MP stop, `NOT_SOLID` and `NODRAW` removed. `[bin-server]` high

---

## 11. Differences from Source SDK 2013 (summary)
- D1. Armour math moved from C++ to the Lua hook `HandlePlayerArmorReduction` with the new-armour rounding rule (cost < 0.1 -> 0, < 1 -> 1); `player_old_armor` restores bonus 0.5. (F7)
- D2. No C++ hit group scaling (`sk_player_*` unused); Lua does head x2, limbs x0.25. (F15, F16)
- D3. No engine ragdoll on death; Lua `CreateRagdoll` (`hl2mp_ragdoll`). (F35)
- D4. No death screen fade (should-fade virtual false, fall-death fade false). (F34, F28)
- D5. Respawn timing entirely in Lua: 2 s then attack / attack2 / jump, bots immediately. (F39, F40)
- D6. Suit power never drains unless `gmod_suit 1`; suit always equipped. (F57, F60)
- D7. `+use` sounds (`HL2Player.Use`, `UseDeny`) only with `gmod_suit 1`; use aborted while attack held unless physgun. (F64)
- D8. Prop pickup gated by `sv_playerpickupallowed` and `AllowPlayerPickup`. (F66)
- D9. Flashlight gated by `PlayerSwitchFlashlight` hook in addition to `mp_flashlight`. (F48)
- D10. Explosion damage has no screen shake. (F13)
- D11. Ammo damage rule has a victim-is-player branch for ammo types flagged 0x40000000; player-vs-NPC damage scaled by `sk_dmg_inflict_scale`. (F19)
- D12. Spawn point selection, teams, scoreboard, hints, death notices are Lua. (F75..F84)
- D13. Weapon equip drops the weapon from physgun holds; `WeaponEquip`, `PlayerCanPickupWeapon`, `AmmoPickup` / `WeaponPickup` user messages gated by a suppress flag. (F24)
