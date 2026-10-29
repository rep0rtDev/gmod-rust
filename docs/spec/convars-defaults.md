# Console variable defaults (measured)

Values read from a stock Garry's Mod dedicated server (version 2026.09.17, build 260917, Linux, `-noworkshop`, no addons, sandbox gamemode, `gm_construct`) by the `fmt_convars_defaults` harness. Golden trace: `tools/parity/golden/260917/fmt_convars_defaults.json`. These override any number quoted from documentation elsewhere in `docs/`.

## Movement and world

| ConVar | Value | Note |
|---|---|---|
| `sv_gravity` | 600 | GMod default, not the Source 800 |
| `sv_friction` | 8 | GMod default, not the Source 4 |
| `sv_stopspeed` | 10 | GMod default, not the Source 100 |
| `sv_accelerate` | 10 | |
| `sv_airaccelerate` | 10 | |
| `sv_maxspeed` | 10000 | per-player speeds come from the gamemode (walk 200, run 400, unconfirmed here) |
| `sv_maxvelocity` | 3500 | |
| `sv_wateraccelerate` | 10 | |
| `sv_waterfriction` | 1 | |
| `sv_noclipspeed` | 5 | multiplier |
| `sv_noclipaccelerate` | 5 | |
| `sv_specspeed` / `sv_specaccelerate` / `sv_specnoclip` | 3 / 5 / 1 | |
| `sv_rollangle` / `sv_rollspeed` | 0 / 200 | no view roll |
| `sv_sticktoground` | 1 | |
| `sv_footsteps` | 1 | |
| `mp_falldamage` | 0 | fixed 10 damage per fall in sandbox |
| `mp_flashlight` | 1 | |
| tick interval | 0.015 s | `engine.TickInterval()` |
| not a ConVar in GMod | `sv_stepsize`, `sv_waterdist`, `sv_bounce`, `sv_backspeed`, `sv_player_max_safe_fall_speed` | hardcoded; measure with `move_*` harnesses |

## Physics

| ConVar | Value |
|---|---|
| `phys_timescale` | 1 |
| `phys_pushscale` | 1 |
| `phys_impactforcescale` | 1.0 |
| `gmod_physiterations` | 4 |
| `sv_turbophysics` | 0 |

## Physgun

| ConVar | Value |
|---|---|
| `physgun_maxrange` | 4096 |
| `physgun_maxSpeed` | 5000 |
| `physgun_maxAngular` | 5000 |
| `physgun_maxSpeedDamping` | 10000 |
| `physgun_maxAngularDamping` | 10000 |
| `physgun_DampingFactor` | 0.8 |
| `physgun_timeToArrive` | 0.05 |
| `physgun_teleportDistance` | 0 |
| `physgun_limited` | 0 |
| not a ConVar | `physgun_wheelspeed`, `physgun_halo` (client side or absent) |

These are the shadow-controller parameters the physgun passes to the physics engine: the held object is driven toward its target with speed capped at 5000 u/s and angular speed at 5000 deg/s, damping factor 0.8, arrival time 0.05 s, no teleport threshold. Confirmed by `physgun_*` harnesses `(unconfirmed)`.

## Gravity gun (`weapon_physcannon`)

| ConVar | Value |
|---|---|
| `physcannon_tracelength` | 250 |
| `physcannon_pullforce` | 4000 |
| `physcannon_maxforce` | 1500 |
| `physcannon_minforce` | 700 |
| `physcannon_cone` | 0.97 |
| `physcannon_ball_cone` | 0.997 |
| `physcannon_maxmass` | 250 |
| `physcannon_chargetime` | 2 |
| `physcannon_mega_enabled` | 0 |

## Sandbox limits

| ConVar | Value |
|---|---|
| `sbox_maxprops` | 200 |
| `sbox_maxragdolls` | 10 |
| `sbox_maxnpcs` | 10 |
| `sbox_maxvehicles` | 4 |
| `sbox_maxballoons` | 100 |
| `sbox_maxeffects` | 200 |
| `sbox_maxdynamite` | 10 |
| `sbox_maxlamps` | 3 |
| `sbox_maxthrusters` | 50 |
| `sbox_maxwheels` | 50 |
| `sbox_maxhoverballs` | 50 |
| `sbox_maxbuttons` | 50 |
| `sbox_maxemitters` | 20 |
| `sbox_maxlights` | 5 |
| `sbox_maxcameras` | 10 |
| `sbox_maxsents` | 100 |
| `sbox_godmode` | 0 |
| `sbox_noclip` | 1 |
| `sbox_playershurtplayers` | 1 |
| `sbox_weapons` | 1 |
| `sbox_persist` | (empty) |
| `sbox_bonemanip_npc` / `_player` / `_misc` | 1 / 0 / 0 |

## Weapons (player damage per hit, max carry)

| Weapon | `sk_plr_dmg_*` | `sk_max_*` |
|---|---|---|
| pistol | 5 | 150 |
| smg1 | 4 | 225 |
| ar2 | 8 | 60 |
| 357 | 40 | 12 |
| buckshot (shotgun, per pellet) | 8 | 30 |
| crossbow | 100 | 10 |
| crowbar | 10 | |
| stunstick | 10 | |
| rpg_round | 100 | 3 |
| grenade | 150 | 5 |
| smg1_grenade | 100 | 3 |
| ar2_altfire | (not a ConVar) | 3 |
| slam | | (not a ConVar) |

Hitgroup multipliers: `sk_player_head` 3, chest 1, stomach 1, arm 1, leg 1; `sk_npc_head` 3.

## Networking

| ConVar | Value |
|---|---|
| `sv_maxupdaterate` / `sv_minupdaterate` | 66 / 10 |
| `sv_maxcmdrate` / `sv_mincmdrate` | 66 / 10 |
| `sv_maxrate` / `sv_minrate` | 0 (unlimited) / 3500 |
| `sv_client_min_interp_ratio` / `sv_client_max_interp_ratio` | 1 / 5 |
| `sv_maxunlag` | 1.0 s |
| `net_maxfilesize` | 16 MB |

## Misc

`r_decals` 2048, `ai_disabled` 0, `ai_ignoreplayers` 0, `sv_allowcslua` 0, `sv_cheats` 0. Engine reports `jit.version` = LuaJIT 2.0.4 (whether the JIT compiler is enabled is a separate question, `jit.status()`, unconfirmed).
