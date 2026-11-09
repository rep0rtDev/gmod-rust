# `player` subsystem: gap list (GMod behaviour vs gmod-rust)

Rust clone: `https://github.com/rep0rtDev/gmod-rust` branch `main`, read on 2026-11-09. Paths are relative to the repo root; line numbers are from that checkout. Behaviour facts refer to `player.md` (F-numbers).

Severity: P0 = obvious to any player within a minute of play, P1 = noticeable in normal play, P2 = subtle / only in edge cases or with convars changed.

## Summary table

| # | Behaviour | Rust today | Match | Sev |
|---|---|---|---|---|
| G1 | Death screen fade (F34) | `crates/gm-server/src/lib.rs:9018-9019` pushes `ScreenFade 64 0 0 200 0.5 0 10` on death; client `crates/gm-client/src/main.rs:4094-4095`, `8758-8775` draws it, `4644-4648` clears it on respawn | **No**: GMod has no death fade at all (binary "should fade on death" returns false; base/sandbox Lua adds none). The dark-red stay-out fade comes from HL1/CS era code that is not in this binary | P0 |
| G2 | Spawn weapon selected (F45) | `lib.rs:7515` `SPAWN_WEAPON = "weapon_crowbar"`, `lib.rs:9341-9344`, client `main.rs:4649` deploys the crowbar | **No**: sandbox gives the physgun last and then `SwitchToDefaultWeapon` picks the client's `cl_defaultweapon` (unset -> physgun stays out). Crowbar is only right for clients with `cl_defaultweapon weapon_crowbar` | P0 |
| G3 | Head / limb multipliers (F15, F16) | status.md row "Server tick": head x2 implemented; search for 0.25 limb scaling found none (hits carry `hitgroup` into `hurt` at `lib.rs:8896-8900` but only the head path is scaled) | **Partial**: limbs (groups 4..7) and gear (10) must take x0.25 | P1 |
| G4 | Armour formula (F7) | `lib.rs:5812` `ARMOR_SHARE = 0.8`; `lib.rs:8919-8950`: skips armour only for `DMG_FALL \| DMG_DROWN`; `to_armor = 0.8*d` capped at armour, floored | **Partial**: missing `DMG_POISON \| DMG_RADIATION` exemption; missing new-armour rounding (cost < 0.1 -> 0, 0.1..1 -> 1: a 1-damage hit costs 1 armour, not 0.8 floored to 0), missing `player_old_armor` (bonus 0.5: cost = 0.4 d, armour lasts twice as long) | P1 |
| G5 | Suit equipped on spawn (F42, F60) | `lib.rs:9179` `p.suit = false` on spawn; `set_suit` at `lib.rs:11591` only via script | **No**: the game always equips the suit, so the HUD aux-power element, HEV damage sentences (`!HEV_DMG*`, `!HEV_HLTH*`) and the `HEV_DEAD` death sentence are active in stock sandbox | P1 |
| G6 | Death sounds (F33, F27) | `lib.rs:9011-9016` plays `Player.Death` always | **Partial**: `Player.FallGib` for `DMG_FALL` deaths and the `HEV_DEAD` suit sentence are missing; no `PlayerDeathSound` hook equivalent | P1 |
| G7 | Fall-death respawn (F38, F40) | `lib.rs:8981-8985`: fall deaths set `death_tick = None` (never respawn) citing a recording | **Doubtful**: the binary's death think and the Lua hook have no fall-specific branch; the only thing that blocks the `PlayerDeathThink` hook is `FL_FROZEN` on the player. The recorded bot that never respawned was most likely frozen or had FL_FROZEN set by the probe. Recommend re-measuring; expected: respawn like any death (2 s then bot auto / key press) | P1 |
| G8 | Frags on kill (F35) | `lib.rs:5756` field; only `set_player_score` (`lib.rs:11749`) writes `frags`; no increment anywhere on a kill | **No**: `DoPlayerDeath` default gives the attacker +1 (or -1 on suicide) | P1 |
| G9 | Scoreboard columns (F81) | `main.rs:6361` columns Name / Kills / Deaths, sorted by frags then deaths (`main.rs:6373-6375`) | **Partial**: Ping column, avatar, mute button and team grouping / colour missing; name colour (93,93,93) and `ScoreboardDefault` font not checked | P2 |
| G10 | +use sounds and attack gate (F64) | `lib.rs:11081-11190` pickup path; grep for `HL2Player.Use`, `UseDeny`, `TrainUse` in `gm-server` finds nothing | **Match for stock settings** (`gmod_suit 0` -> silent) but no `gmod_suit` convar path; missing: abort `+use` when `IN_ATTACK`/`IN_ATTACK2` is held and the active weapon is not the physgun; missing `HL2Player.TrainUse` on trains | P2 |
| G11 | Pickup gate convar (F66) | `lib.rs:11182` asks `PlayerPickup` (hook) only; no `sv_playerpickupallowed` | **Partial**: convar missing (default 1 so stock play matches) | P2 |
| G12 | Flashlight rules (F48..F50) | `lib.rs:11726-11748`: toggles a bool and plays the on/off sounds; `use_flashlight` `lib.rs:22296` | **Partial**: missing `mp_flashlight` gate, alive check (dead players must not switch it on), `PlayerSwitchFlashlight` hook, forced off on death (check: `hurt` does not clear `p.flashlight`), `OnFlashlightOn/Off` outputs | P1 |
| G13 | Suit power / sprint sounds (F57..F61) | `gm-move` handles sprint speeds (`sprint_enabled` at `lib.rs:2594`); no aux power value, no `HL2Player.SprintStart` / `SprintNoPower` sounds (grep empty) | **Partial**: stock GMod never drains, so the bar stays 100 (fine), but `HL2Player.SprintStart` plays on every sprint start in the game (audible, prediction-filtered so the local player hears it too) and is missing; no `gmod_suit 1` drain model | P1 |
| G14 | Respawn delay (F40) | `lib.rs:7516` `RESPAWN_TICKS = 135`; `lib.rs:22526-22538` requires `tick >= d + 135` and (bot or ATTACK/ATTACK2/JUMP edge) | **Match** (2 s + 0.1 s think granularity measured as 135 ticks). Note the game's rule is time based (`CurTime() + 2` checked on a 0.1 s think), so with a different tick rate the constant must be derived, not hard-coded | ok |
| G15 | Weapons stripped tick after death (F38) | `lib.rs:22527-22531` | **Match** | ok |
| G16 | Spawn point selection (F75..F77) | `lib.rs:9124-9136` random pick of BSP spawn points; `SelectSpawn` hook; `+1 u` (`lib.rs:7517`) | **Partial**: only `info_player_start` from the BSP (`spawn_points_from_bsp` `lib.rs:8285`); the game also uses `info_player_deathmatch`, `gmod_player_start` and the long class list, the "master" spawn flag preference, and the `IsSpawnpointSuitable` occupancy test (random point retried; blocking players killed on the forced last try) | P2 (P1 on maps without `info_player_start`) |
| G17 | Spawn loadout (F45) | `lib.rs:7518-7532` 13 weapons; `inventory.give_loadout()`; no `sbox_weapons` gate; ammo amounts in `weapons.rs` (not re-checked here) | **Partial**: `sbox_weapons 0` must give only tool gun, camera, physgun; ammo given silently (no `AmmoPickup` HUD) matches if the HUD history is not fed on spawn | P2 |
| G18 | Default player model (F70) | `lib.rs:7514` `models/player/kleiner.mdl`; `player_models::translate` | **Match** (kleiner is the Lua fallback). The binary's initial `models/player.mdl` is never visible | ok |
| G19 | Ragdoll creation (F35) | `lib.rs:8970-8978` `Ragdoll {angles: ZERO, velocity}`; `spawn_death_ragdolls` `lib.rs:10941` as `hl2mp_ragdoll` | **Match** in substance; the game's ragdoll force is the accumulated bullet force of the killing hits (F4), not just the player velocity: a shotgun kill flings the body | P1 |
| G20 | `DMG_REMOVENORAGDOLL` (F35) | not handled (grep empty) | **Missing**: `Kill`-type damage with that flag must produce no ragdoll | P2 |
| G21 | Death camera (F47) | `main.rs:4590-4642`: eye on the ragdoll's `eyes` attachment, pulled back 96 u interpolated over 1.5 s, 6 u box trace, angles frozen | **Partial**: the game uses the ragdoll origin + dead view height (14 u), **no time interpolation** (the 96 u pull-back is immediate), and a brush-only trace with `WALL_OFFSET` half-size (SDK value 6). The 1.5 s `DEATH_ANIMTIME` lerp is from CS:S / TF, not HL2MP | P1 |
| G22 | Knock-back on hit (F9) | `weapons.rs:1135` 4.18 u/s per damage (measured) | **Match** (measured) | ok |
| G23 | Fall damage (F25, F26) | `lib.rs:7432-7433`, `22872-22890` | **Match** | ok |
| G24 | Explosion screen shake (F13) | none | **Match** (GMod has none) | ok |
| G25 | Hurt event / `PlayerHurt` hook (F11) | `GameEvent::PlayerDamaged` `lib.rs:8993` | **Partial**: the game passes health remaining and the attacker; `player_hurt` event for the HUD damage indicator direction needs attacker position (`hits` carry `pos`, fine) | P2 |
| G26 | Damage indicator HUD (F87) | not found in `main.rs` (no `DamageIndicator`, no red directional flash) | **Missing**: red side/front/behind flash on damage is very visible | P0 |
| G27 | Health HUD low-health animation (F86) | `main.rs` health drawing not analysed for the < 20 flash | **Unknown**: verify the `HealthLow` pulse below 20 hp | P2 |
| G28 | Hints (F84) | `main.rs:1603-1604`, `3520-3560`: `OpeningContext 20`, `EditingSpawnlists 5`, `ContextClick 20`, Annoy texts; hint notify length 20 s, drip sound? (not checked) | **Partial**: game schedules `OpeningMenu 30`, `Annoy1 5`, `Annoy2 7` at load; verify the `ambient/water/drip1-4.wav` sound and `cl_showhints` | P2 |
| G29 | Notifications (F83) | `main.rs:3568-3610` notify panel, colours (255,220,0) | **Partial**: fade-out thresholds (0.7 s / 0.2 s), anchor at 80 % height and slide-in from x = 200 px not verified; server `Hint` messages shown for 6 s (`main.rs:4037`) where the game's `AddNotify` default is what the caller passes (hints 20 s) | P2 |
| G30 | Buddha / godmode (F6) | `sbox_godmode` convar `lib.rs:8732` | **Partial**: `sbox_godmode` is a Lua `PlayerShouldTakeDamage` rule (sandbox returns false when set); buddha (health never < 1) not present | P2 |
| G31 | Weapon pickup (`WeaponPickup` HUD, `PlayerCanPickupWeapon`) (F24) | `weapon_pickups` `lib.rs:20513` | **Unknown**: hook + HUD history message not verified | P2 |
| G32 | Observer / spectate (F88) | nothing found | **Missing** (only matters for spectator gamemodes) | P2 |
| G33 | Death notice (F36, F82) | `DeathNotice::parse` `main.rs:4057`, `deaths` list | **Partial**: `trigger_hurt` attacker -> victim, invalid inflictor -> attacker, server console line `"<nick> was killed by <class>"`, friendly-attacker flag not verified | P2 |
| G34 | Multiplayer damage table (F19, F20) | `weapons.rs:1074` `bullet_damage(ammo)` single value per ammo | **Partial**: player-vs-NPC uses `sk_npc_dmg_*`? (status says NPC damage measured as `sk_plr_dmg_*` for player bullets, which matches the SDK rule since the attacker is a player). Missing: `sk_dmg_inflict_scale<skill>` on player->NPC damage and the victim-is-player branch for GMod-flagged ammo | P2 |
| G35 | Blast immunity window (F4) | not found | **Missing**: SLAM / tripmine owner immunity only; P2 | P2 |
| G36 | Spawn fade purge (F34) | `main.rs:4644-4648` clears only stay-out fades on respawn | **Partial**: the game purges every fade on spawn (flags 0x11) | P2 |

## Things the Rust build does that the game does not
- X1. The dark-red death fade (`ScreenFade 64 0 0 200 0.5 0 10`, `lib.rs:9018-9019`, client `main.rs:4094`, `4644-4648`). GMod never fades on death (F34). Remove.
- X2. Crowbar auto-deploy on spawn (`SPAWN_WEAPON`, `lib.rs:7515`; client `main.rs:4649` `pending_select = CROWBAR_SLOT`). The game ends spawn with the physgun selected unless the client's `cl_defaultweapon` names another owned weapon (F45).
- X3. "Fall deaths never respawn" rule (`lib.rs:8981-8985`). No such rule exists in the binary or Lua (F38, F40); treat the recording as an artefact until re-measured.
- X4. Death camera pull-back interpolated over 1.5 s (`main.rs:4626`). HL2MP's death camera jumps straight to 96 u behind (F47).
- X5. `set_suit` defaulting to off (`lib.rs:9179`): the game cannot have the suit off; `gmod_suit` is a drain/sound switch, not a suit toggle (F60).
- X6. Spawn point RNG with a fixed xorshift (`lib.rs:9124-9136`): acceptable (the game's pick is `math.random`), but the game retries occupied points and prefers "master" spawns (F76, F77).

## Ranked list
1. P0 G1 death fade that the game does not have.
2. P0 G26 missing damage indicator flash.
3. P0 G2 wrong weapon out after spawn (crowbar vs physgun / `cl_defaultweapon`).
4. P1 G5 suit never equipped (no aux bar, no HEV voice, no `HEV_DEAD`).
5. P1 G21 death camera interpolation and eye height.
6. P1 G19 ragdoll force from accumulated bullet force.
7. P1 G12 flashlight gates (dead player, `mp_flashlight`, hook, off on death).
8. P1 G13 `HL2Player.SprintStart` missing.
9. P1 G3 limb x0.25 scaling; G4 armour rounding / poison / radiation; G6 fall-death sound; G7 fall-death respawn; G8 frags.
10. P2 the rest (G9..G11, G16, G17, G20, G25, G27..G36).
