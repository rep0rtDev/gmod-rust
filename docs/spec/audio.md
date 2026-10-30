# Audio (client)

Crate `gm-audio`: the game's sound files and soundscripts, played by the native client.
Nothing here is parity-measured: the client cannot record the game's mix. What is taken
from the game's data is exact (which wave, volume, pitch, level per entry); the mixing laws
are documented approximations and marked so.

## Files

- Waves: `sound/<path>.wav` from the mounted content (`hl2_sound_misc_dir.vpk` for the
  HL2 weapons). RIFF WAVE, format tag 1 (PCM), 8-bit unsigned or 16-bit signed, mono or
  stereo, 22050 or 44100 Hz. Stereo is folded to mono (every emitted sound is spatialized,
  so only the mix of a stereo source matters). Cue/loop chunks are ignored (no weapon sound
  loops). Other formats are rejected with an error and the sound is skipped.
- Scripts: `scripts/game_sounds_weapons.txt`, `_physics`, `_player`, `_items`, `_world`
  (KeyValues, loose files under `sourceengine/`). Per entry: `wave` (one) or `rndwave`
  (several, one picked at random per play), `volume` (`0.7` or `0.5,1.0`: uniform in the
  range), `pitch` (percent, same forms; 100 = recorded rate), `soundlevel` (`SNDLVL_NORM`
  75, `SNDLVL_GUNFIRE` 140, `SNDLVL_105dB`, a bare number; default 75). Leading sound
  characters on a wave path (`^`, `*`, `)`, `#`, `>`...) are stripped and otherwise ignored.
  `channel` is ignored (no per-channel replacement yet).

## Events → entries

Weapon sounds follow the server's viewmodel events (`PlayerSnap::vm_act` / `vm_serial`,
[server.md](server.md)), for the owner (at the listener) and for other players (at their
eye), so the sound and the shot are the same event on every client:

| event (`VmAct`) | entry |
|---|---|
| `Fire` (6) | `Weapon_<X>.Single`; `Weapon_PhysCannon.Launch` for the gravity gun |
| `Reload` (1) | `Weapon_<X>.Reload`; silent for the shotgun (its start stage has no sound) |
| `ReloadShell` (4) | `Weapon_Shotgun.Reload` (per shell) |
| `ReloadEnd` (5), `Pump` (2) | `Weapon_Shotgun.Special1` (cock) |
| `Dry` (3) | `Weapon_<X>.Empty` |

`<X>` is the loadout class: `Pistol`, `357`, `SMG1`, `AR2`, `Shotgun`, `Crossbow`, `RPG`,
`Crowbar`, `PhysCannon`. The physgun, tool gun and camera have no entries yet (GMod's own
sounds for them are not in the HL2 scripts; to be taken from GMod's content). The owner's
viewmodel `fire` sequence also plays on the `Fire` event now, not on the click, so held
automatic fire and dry fire show and sound as the server resolved them. Open: the shotgun's
secondary (double) also arrives as `Fire` and plays `.Single`, not `.Double`. The .357's
`Weapon_357.Reload` names `weapons/357/reload1.wav`, which the content does not ship (the
game's revolver reload sounds come from its viewmodel animation events, `357_reload1/3/4`,
not played yet), so its reload is silent.

## Mixer (approximation)

- Output: cpal default device, its rate and channel count; no device (headless) → sounds
  are accepted and dropped, the game runs the same (`audio: ...` on startup says which).
- Voices: one-shot, linear resampling at `pitch / 100 × source rate / output rate`, gain
  `volume` from the script, summed and hard-clipped to [-1, 1].
- Distance: attenuation from the SDK's public `SNDLVL_TO_ATTN` (`20 / (level - 50)`, 4
  under 50 dB; 0 dB = no attenuation), then gain `1 - dist × attn / 1000` clamped to
  [0, 1] (silence at 4500 u for gunfire, 1250 u for `SNDLVL_NORM`). This linear law is the
  classic one the scripts' levels were tuned around, not the engine's measured curve.
- Pan: constant-power on the listener's horizontal right vector (`(sin yaw, -cos yaw, 0)`);
  sounds at the listener are centered. No occlusion, DSP, doppler or soundscapes.
