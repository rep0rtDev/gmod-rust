# Content mounting

How gmod-rust finds and layers the files of the player's Garry's Mod installation. Target: the same logical file set, in the same precedence order, that the real game sees with no addons installed.

## Install discovery

1. Steam library roots: `<Steam>/steamapps/libraryfolders.vdf` (KeyValues), every `path` entry plus `<Steam>` itself.
2. In each root, `steamapps/appmanifest_4000.acf` with `StateFlags` `4` marks a complete install; `installdir` names the folder under `steamapps/common/` (normally `GarrysMod`).
3. The game directory must contain `garrysmod/gameinfo.txt` and `garrysmod/garrysmod_dir.vpk`. Otherwise the launcher refuses to start.
4. Build id for cache invalidation: `buildid` from the manifest, plus `garrysmod/steam.inf`.

## Mount order (observed on build 26229091, x86-64 branch)

`gameinfo.txt` says mounting is controlled by `cfg/mount.cfg`, and its `SearchPaths` block is informational. The observed on-disk layout:

| Order | Path | Kind | Contents |
|---|---|---|---|
| 1 | `garrysmod/addons/*` | loose dirs or GMA | skipped by gmod-rust (no GLua addons) |
| 2 | `garrysmod/garrysmod_dir.vpk` | VPK v2 | GMod-own materials, models, sounds, settings |
| 3 | `garrysmod/` | loose | `maps/gm_construct.bsp`, `maps/gm_flatgrass.bsp`, `cfg/`, `resource/`, `scripts/`, `settings/`, `particles/`, `scenes/` |
| 4 | `garrysmod/fallbacks_dir.vpk` | VPK v2 | fallback assets used when mounted games are missing (unconfirmed: exact precedence) |
| 5 | `sourceengine/hl2_textures_dir.vpk`, `hl2_misc_dir.vpk`, `hl2_sound_misc_dir.vpk`, `hl2_sound_vo_english_dir.vpk` | VPK v2 | HL2 content |
| 6 | `sourceengine/content_hl2_dir.vpk`, `content_cstrike_dir.vpk` | VPK v2 | additional HL2 / CS:S content shipped with GMod (unconfirmed: what overlaps with 5) |
| 7 | `sourceengine/` | loose | remaining HL2 files |
| 8 | `platform/platform_misc_dir.vpk`, `platform/` | VPK v2 + loose | engine UI resources |

Lookups are case-insensitive and use forward slashes. The first hit in order wins.

`cfg/mount.cfg` and `cfg/mountdepots.txt` add other Source games the player owns (CS:S, TF2, ...). gmod-rust v1 ignores them; a later version mounts them the same way.

## Harnesses

- `fmt_vpk_index_garrysmod`: the probe lists `models/*.mdl` and `materials/**/*.vmt` through `file.Find(..., "GAME")` on a server with `-noaddons -noworkshop`; gmod-rust lists the same globs over its mounted set. Compare as sets. Expected: equal.
- `fmt_vpk_index_hl2_models`: same for `models/props_c17/*.mdl` and `models/props_junk/*.mdl`.

## Client vs dedicated server content

App 4000 (client) and app 4020 (dedicated server) ship different `garrysmod_dir.vpk` builds. Measured on client build 26229091 (x86-64 branch) against server version 2026.09.17: the client has three `models/props_wasteland/cargo_container*` models the server lacks; otherwise the `fmt_vpk_index_*` globs match exactly. Server-realm behaviour is compared against the server install, client-realm against the client install. Harness: `fmt_vpk_index_hl2_models`, `fmt_vpk_index_garrysmod` (green on 2026-10-29 against the server install).
