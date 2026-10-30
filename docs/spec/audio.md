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
- Scripts: every file named in `scripts/game_sounds_manifest.txt` (GMod's, in
  `garrysmod_dir.vpk`, lists its `scripts/sounds/*.txt`: `lua.txt` with `Toolgun.Single`,
  `phx.txt`, the HL2 / episodic / portal / CS / TF2 / L4D sets; files absent from the mount
  are skipped) plus the base `scripts/game_sounds_weapons.txt`, `_physics`, `_player`,
  `_items`, `_world` loose under `sourceengine/` (KeyValues). The first file to define a
  name keeps it, as the SDK's sound emitter skips duplicates: GMod's manifest lists TF2's
  `Weapon_Pistol.Single` (`weapons/pistol_shoot.wav`, not shipped) after HL2's. Per entry: `wave` (one) or `rndwave`
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
| `Fire` (6) | `Weapon_<X>.Single`; `Weapon_PhysCannon.Launch` for the gravity gun; `Toolgun.Single` for the tool gun, whose `Fire` the server raises when a stool takes the click (the game's `DoShootEffect` path: sound, viewmodel attack and the player's attack gesture only on an accepted click) |
| `Reload` (1) | `Weapon_<X>.Reload`; silent for the shotgun (its start stage has no sound) |
| `ReloadShell` (4) | `Weapon_Shotgun.Reload` (per shell) |
| `ReloadEnd` (5), `Pump` (2) | `Weapon_Shotgun.Special1` (cock) |
| `Dry` (3) | `Weapon_<X>.Empty` |

`<X>` is the loadout class: `Pistol`, `357`, `SMG1`, `AR2`, `Shotgun`, `Crossbow`, `RPG`,
`Crowbar`, `PhysCannon`. The physgun is silent on purpose: measured through the probe's
`EntityEmitSound` hook (`physgun_sound_hold`, `physgun_sound_freeze`), the server emits no
sound for pick-up, hold, freeze or release, so whatever the game's physgun plays is
client-only and not measurable from the server (open). The camera's
`NPC_CScanner.TakePhoto` is not wired (the server has no camera click yet).

Measured but not played yet: prop impacts. In `physgun_sound_hold` the released crate's
landing emits `physics/wood/wood_crate_impact_hard5.wav` on `CHAN_STATIC` (6), volume
0.3672, pitch 99, `SNDLVL_NORM` (75), 29 ticks after release (the surface-property impact
sound system: the wave set from `surfaceproperties.txt`, volume from the impact speed). The owner's
viewmodel `fire` sequence also plays on the `Fire` event now, not on the click, so held
automatic fire and dry fire show and sound as the server resolved them. Open: the shotgun's
secondary (double) also arrives as `Fire` and plays `.Single`, not `.Double`. The .357's
`Weapon_357.Reload` names `weapons/357/reload1.wav`, which the content does not ship (the
game's revolver reload sounds come from its viewmodel animation events, `357_reload1/3/4`,
not played yet), so its reload is silent.

## World sounds: prop impacts (measured)

The server plays a prop's impact sound itself and tells every client (`ServerMsg::Sounds`,
unreliable, with the snapshot: script name, world point, volume factor); the client plays the
script at that point (`Sounds::emit_scaled`). Measured on srcds with an `EntityEmitSound`
hook in the physics probe (`phys_sound_drop_crate_3 / 3_5 / 4 / 5 / 7 / 9 / 12 / 27 / 75 /
170 / 300`, `phys_sound_drop_drum_75`: a crate / drum dropped from 3-300 u onto the
gm_construct floor):

- Script: the `impacthard` entry of the model's surface property, through its `base` chain
  (`Wood_Crate` → `Wood_Crate.ImpactHard`, `Metal_Barrel` → `Metal_Barrel.ImpactHard`;
  `SurfaceTable::sound`). `impactsoft` was never heard in these drops.
- Volume: `(speed / 320)^2`, capped at 1, times the script's own volume (0.6 for both):
  80.7 u/s → 0.0381, 107.3 → 0.0674, 177.0 → 0.1835, 292.3 → 0.5003, 422.8 and 529.6 →
  0.5996 (the script's 0.6 at the cap). `speed` is the collision's approach speed, which our
  contact model reproduces exactly on every crate landing (`gm_parity run-ours` writes the
  `impacts` of our trace next to the golden `collisions` / `sounds`).
- Threshold: 62.9 and 69.1 u/s are silent, 71.8 plays (0.0302); we use 70 u/s
  (`IMPACT_SOUND_MIN_SPEED`).
- One sound per prop per tick, its first contact: the crate landing at 116.1 u/s with a
  second 187.7 u/s contact in the same tick plays 116.1 (0.079).
- Timing: the game's sound is recorded 0-3 ticks after the collision tick (15 for the
  collision at 13, 37 for 34, 9 for 9); we play it at the collision tick. The second,
  smaller bounces follow the physics divergence of the drop itself (the drum rolls
  differently after 48 ticks and its second 0.3 impact is not reproduced).
- Pitch 98-100 and `SNDLVL_75dB` come from the script; no server-side override was seen.

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

## World sounds: footsteps (measured)

Measured in `move_step_*` (details and the timing/volume law in [movement.md](movement.md), "Footsteps"): the server emits the surface's `stepleft`/`stepright` wave (gm_construct's spawn floor is `Concrete.StepLeft` = concrete 1/3, `Concrete.StepRight` = concrete 2/4) at soundlevel 75, pitch 100, on the body channel, with volume 0.2 walking, 0.5 running, times 0.65 ducked, 1.0 on a jump and on a hurting landing (with `Player.FallDamage`), 0.85 on a landing from more than half the hurting speed. Here the server sends the script name with that volume as the factor, and the client picks the random wave as it does for prop impacts; the player's own steps come back from the server like everyone else's. Not measured: the client-side mix of one's own steps (the game plays them locally too).
