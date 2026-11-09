# Garry's Mod — `sound` subsystem behaviour note

Scope: soundemitter (soundscripts, channels, randomisation, distance law), DSP presets / room DSP, soundscapes, footsteps, physics impact & scrape sounds, engine mixing (spatialisation, doppler, occlusion), voice chat basics, main-menu / music.

Tags on every fact: **source** = `PC-srv` (pseudo-C of the Linux server binary, symbols present), `PC-cli` (Windows client binary, matched by strings/vtables), `PC-eng` (Windows engine binary, matched by strings), `SDK` (Source SDK 2013 clone), `Lua` (Facepunch/garrysmod clone), `LEAK` (behaviour of the 2007 engine source that the strings/constants in the dump are consistent with — not verifiable from the dump alone), `INF` (inference); **confidence** = H / M / L.
Where the binary differs from SDK 2013 it is marked **≠SDK**. Units: Hammer units (u), seconds (s), milliseconds (ms), dB, percent pitch (100 = recorded rate).

What the dump does *not* contain (so facts there are SDK/LEAK, see §12 blockers): the engine's gain/spatialisation/doppler core, the server physics-impact sound code (it lives in the server binary but was not selected into this dump), player step timer update, music/menu code.

---

## 1. Soundemitter system (server)

### 1.1 Script resolution
- A sound name is a **raw file** when it contains `.wav`, `.mp3` or `.ogg` anywhere, or starts with `!` (sentence), or the engine says it is a sentence/raw name. Raw files play straight with the caller's channel/volume/level/pitch (no script lookup). `.ogg` acceptance is GMod-specific. `[PC-srv H]` **≠SDK** (SDK accepts only `.wav`/`.mp3`).
- Otherwise the name is a **soundscript entry** (`scripts/game_sounds*.txt` via `scripts/game_sounds_manifest.txt`, plus `<map>_level_sounds.txt` per map, plus Lua `sound.Add` entries that live in the virtual script `scripts/sounds/lua.txt`). `[PC-srv H, PC-cli H]`
- Script entry defaults when a key is missing: `channel` CHAN_AUTO (0), `volume` 1.0, `pitch` 100, `soundlevel` 75 dB (SNDLVL_NORM). `[PC-srv H, SDK H]`
- An entry with neither `wave` nor `rndwave` is rejected with a console error. `[PC-srv H]`
- Script keys parsed (server and client): `channel` (CHAN_* name), `volume` (number, `lo,hi` range or `VOL_NORM`), `pitch` (number, `lo,hi`, `PITCH_NORM`=100, `PITCH_LOW`=95, `PITCH_HIGH`=120), `soundlevel` (`SNDLVL_xxdB`, `SNDLVL_NONE`=0, `SNDLVL_IDLE`=60, `SNDLVL_STATIC`=66, `SNDLVL_TALKING`/`SNDLVL_NORM`=75, `SNDLVL_GUNFIRE`=140, a plain number, or a `lo,hi` range), `attenuation` (legacy; converted: level = 50 + 20/attn for attn>0, 0 for attn≤0), `wave` / `rndwave { wave ... }`, `delay_msec`, `operator_stacks` (ignored by GMod: no operator system). `[SDK H, PC-srv M]`
- Wave name prefix characters are stripped and interpreted by the engine: `*` stream, `#` music/no DSP, `@` omnidirectional (no spatialisation), `>` doppler, `<` directional, `^` distance variant, `)` spatial stereo, `(` ... `}` `$` etc. (engine wave-name character set). `[SDK H (soundchars.h) / LEAK M]`

### 1.2 Randomisation
- When an entry has several waves one is picked uniformly (`RandomInt(0, n-1)`). Server and client pick independently; there is no shared seed, so a sound heard by the client that is emitted by the server is chosen **on the server** and sent by name. `[PC-srv H]`
- Pitch range `lo,hi` → `RandomFloat(lo, hi)` cast to int percent. Volume range → `RandomFloat`. Soundlevel range → `RandomFloat` cast to int dB. `[PC-srv H, SDK H]`
- Caller overrides: the emit parameter block carries flag bits "volume overridden" (bit 0) and "pitch overridden" (bit 1); when set, the script value is replaced by the caller's. `[PC-srv H, SDK H]`

### 1.3 Channels
| id | name | notes |
|---|---|---|
| 0 | CHAN_AUTO | never replaces anything; each emit allocates a new engine channel |
| 1 | CHAN_WEAPON | default channel for Lua `Weapon:EmitSound` **≠SDK** `[PC-srv H]` |
| 2 | CHAN_VOICE | voice lines; used by the voice-chat mixer too |
| 3 | CHAN_ITEM | |
| 4 | CHAN_BODY | footsteps, self-collision impacts, friction loops |
| 5 | CHAN_STREAM | |
| 6 | CHAN_STATIC | ambient_generic, physics impacts between two objects, soundscapes |
| 7 | CHAN_VOICE2 | GMod/TF2 addition, listed in `scripts/soundmixers.txt` groups `[PC-eng H]` |
| 8.. | CHAN_VOICE_BASE+n | per-player voice chat slots `[SDK H]` |
A non-AUTO channel on an entity **replaces** the sound already playing on (entity, channel): the engine looks up an existing channel by (entity index, channel, wave) and reuses it; with `SND_CHANGE_VOL`/`SND_CHANGE_PITCH` it only mutates volume/pitch; with `SND_STOP` it stops it. `[PC-eng H]`
A wave that is a voice file (`vo/` or name starting with the voice prefix) emitted on a channel other than VOICE/VOICE2/STREAM only prints a developer warning — it still plays. `[PC-srv H]`

### 1.4 Emit flags (`SND_*`, engine bit field) `[SDK H, PC-eng H]`
1 CHANGE_VOL, 2 CHANGE_PITCH, 4 STOP, 8 SPAWNING, 16 DELAY, 32 STOP_LOOPING, 64 SPEAKER (re-emitted by env_microphone), 128 SHOULDPAUSE, 256 IGNORE_PHONEMES, 512 IGNORE_NAME, 1024 DO_NOT_OVERWRITE_EXISTING_ON_CHANNEL. Flags are encoded in 11 bits on the wire.
- A **sound level ≥ 256** on the wire means "add 256 to flag bit 'no-attenuation-from-player'": the engine subtracts 256 and sets a channel bit that makes it behave like a sound from the local player (no distance attenuation, not spatialised). `[PC-eng M]`

### 1.5 Server emission pipeline (every server-side sound) `[PC-srv H]`
1. If the entity is a weapon and the call is Lua `Entity:EmitSound`, the sound is re-attributed to the owner (entity index + owner position). **≠SDK**
2. `env_microphone` listeners get first look; a microphone that "hears" the sound (within its radius, LOS, sensitivity) re-emits it through its speaker with flag 64 and may swallow the original.
3. **Lua hook `GM:EntityEmitSound(t)`** is called with a table: `Ambient`, `Entity`, `Channel`, `OriginalSoundName`, `SoundName`, `Volume`, `SoundLevel`, `Flags`, `Pitch`, `DSP`, `SoundTime`, `Pos` (client version also has `SentenceIndex`). Returning `false` drops the sound; otherwise all those fields are read back and used (so Lua can retarget entity, channel, name, volume, level, flags, pitch, dsp, pos, time). Re-entrancy is limited to depth 100; past that the hook is skipped and a "broken" flag is set until the depth unwinds to 0. **≠SDK** (hook does not exist in SDK).
4. The (possibly rewritten) wave is **late-precached** silently if missing.
5. Emitted via the engine as positioned entity sound, or as an *ambient* sound (fixed origin, entity-less) when `Ambient` is set.
6. Close captions: if `closecaption` is on for the recipient the server sends a `CloseCaption` usermessage with duration×10 clamped to 255 and flags (from-player 1<<? / gender). Caption recipients are filtered by attenuation (removed when beyond the level range). Duration is queried from the engine only for non-stop, non-change, non-ambient emits.

### 1.6 Lua API behaviour `[PC-srv H]` (all **≠SDK**, GMod-specific)
- `Entity:EmitSound(name, level=75, pitch=100, volume=1, channel, flags=0, dsp=0, filter)`: name is trimmed; empty → nothing. Channel default CHAN_WEAPON for weapons else CHAN_AUTO. Recipient filter: PAS (potentially audible set) at the emission origin, then **attenuation filter with factor 0.8**: recipients farther than `2 × 1000 / 0.8 = 2500 u` are removed **regardless of the sound level** — unless a `CRecipientFilter` is passed as argument 8/9. Prediction rules apply for predicted entities (player/weapon) so the sound is not doubled.
- Global `EmitSound(name, pos, entIndex, channel=0, volume=1, level=75, flags=0, pitch=100, dsp=0, filter)`: same 2500 u PAS filter. If `name` is a soundscript, a random wave is chosen here and channel/volume/level/pitch default to the script's (randomised) values when the Lua arg is nil.
- `Player:PlayStepSound()`? (server Lua step forcing): sets the force-footstep flag, resets the player's step timer to 400 ms and plays a step on the current surface at volume 1 with force=true (see §4). `[PC-srv M]`
- `sound.Add{name, channel, volume, level|soundlevel, pitch|pitchstart+pitchend, sound (string|table)}` — client and server; `pitchstart/pitchend` become a `lo,hi` pitch range. `[PC-cli H]`
- `CreateSound(ent, name, filter)` → sound patch (§1.7). Patch DSP is settable (controller has set/get DSP). `[PC-srv H]`

### 1.7 Sound patches / envelopes (`CreateSound`, friction loops, thrusters) `[PC-srv M, SDK H]`
- A patch wraps one looping emit on (entity, channel) and keeps volume (0..1) and pitch (int) envelopes. `ChangeVolume(v, dt)` / `ChangePitch(p, dt)` ramp linearly over `dt` seconds; `dt ≤ 0` jumps. The controller thinks every frame and re-sends `SND_CHANGE_VOL`/`SND_CHANGE_PITCH` while an envelope is active. Volume 0 with `FadeOut` stops the sound at the end of the ramp. `Shutdown` sends `SND_STOP`.
- Patch sound level and DSP are stored on the patch and sent on (re)start.

### 1.8 ambient_generic `[PC-srv H (key names and ramp constants match), SDK H]`
- Volume = `health` × 10 → 0..100 (sent as vol/100). Input `Volume x` → `clamp(round(x*10), 0, 100)`.
- Radius → soundlevel: `level = int(40 + 20·log10(radius / 36))` (sound is 40 dB at the 36 u reference); spawnflag 1 "play everywhere" → level 0 (no attenuation); spawnflag 16 start silent; spawnflag 32 not looping.
- Dynamic pitch/volume (`preset` 1..27 table, or keys `pitchstart`, `pitch`, `spinup`, `spindown`, `volstart`, `fadein`/`fadeinsecs`, `fadeout`/`fadeoutsecs`, `lfotype` 0 off/1 square/2 triangle/3 random, `lforate` 0..1000, `lfomodpitch`, `lfomodvol`, `cspinup`): ramp think at **5 Hz (0.2 s)**; pitch/vol step toward target by `spinup`/`fadein` fractions; LFO value `lfomult` 0..255 (random type → `RandomInt(0,255)`), applied as `pitch += (lfomult-128)·lfomodpitch/100`, `vol += (lfomult-128)·lfomodvol/100`. Pitch clamp 1..255, vol clamp 0..100.
- Transmit culling: max audible distance = binary search between radius and 2·radius on the engine's distance-gain function until gain ≤ 1.01e-3. `[PC-srv H]`
- Think delays after inputs: 0.1 s.

### 1.9 Sentences / suit / scanner misc
- `UTIL_EmitAmbientSound` accepts `!N` sentences and `.wav/.mp3/.ogg` raw files **≠SDK** (.ogg). `[PC-srv H]`
- Suit voice: filter attenuation factor 0.8 as above. `[PC-srv M]`
- Random pitch helper used by sentence playback: 50 % chance of `pitch = RandomInt(0,6)+98`. `[SDK H]`

---

## 2. SNDLVL distance law and mixing (engine)

### 2.1 Distance law `[PC-eng M for dist_mult set-up, LEAK/SDK H for the law]`
- Reference: `snd_refdb` = 60 dB, `snd_refdist` = 36 u (engine convars, defaults from the 2007 engine; the 36/60 values are also hard-coded in SDK `sound.cpp`). 
- Per channel: `dist_mult = 10^((refdb - sndlvl)/20) / refdist` = `10^((60 - sndlvl)/20) / 36` (per unit). `sndlvl == 0` → `dist_mult = 0` → no attenuation. 
- Gain: `gain = snd_gain (1.0)`; if `dist_mult > 0`: `d0 = 1/dist_mult`; for `dist ≤ d0` gain stays 1, else `gain = 1/(dist·dist_mult)` (inverse distance, −6 dB per doubling), clamped to `snd_gain_max` (1.0) and floored at `snd_gain_min` (0.01): below the floor the engine fades linearly to 0 so sounds do end (LEAK). Handy values of `d0 = 36·10^((lvl-60)/20)`: 60 dB→36 u, 66→72 u, 70→114 u, 75→202 u, 80→360 u, 90→1138 u, 100→3600 u, 105→6400 u, 110→11 380 u, 120→36 000 u, 140→360 000 u (gunfire is effectively unattenuated on any map).
- **No** "linear to 1000/attn" GoldSrc law anywhere in Source/GMod. `[INF H]`
- `snd_gain`, `snd_gain_max`, `snd_gain_min`, `snd_refdb`, `snd_refdist`, `snd_foliage_db_loss` (4 dB per foliage trace hit), `snd_obscured_gain_dB` (−2.70 dB) exist in the 2007 engine; occlusion (§2.4) uses the last two. `[LEAK M]`
- Volume byte: `vol = int(volume·255)`, clamped at 255 with a console warning when exceeded; pitch 0 is rejected (sound not started, warning). `[PC-eng H]`

### 2.2 Spatialisation `[PC-eng M (debug print of the per-speaker gains dspmix/distmix/dspface/lvol/cvol/rvol/rlvol/rrvol), LEAK M]`
- Mono sources are panned on 5 virtual speakers (front-left, centre, front-right, rear-left, rear-right) from the source direction in listener space; stereo output mixes rear speakers into L/R. Gains are computed per frame (`SND_Spatialize`): `vol·gain·speakermix`. Sounds flagged `@` (omnidirectional) or from the local player get equal L/R.
- `snd_surround_speakers` selects 2/4/5.1 layout; 2 speakers uses the headphone/stereo pan: `lvol = (1 - dot(right,dir))·0.5` style weighting (LEAK).
- Facing-away DSP (`dsp_facingaway`) attenuates highs for sources behind the listener; `dsp_spatial` presets exist (room/facing). `[LEAK M]`
- Source radius: the engine treats sources within 1 u? as centred (no pan). `[LEAK L]`

### 2.3 Doppler `[LEAK M, SDK H for prefix]`
- Only waves whose name starts with `>` are doppler-processed. The engine splits such a wave into "approaching" and "receding" stereo halves and mixes them by the sign/size of the closing velocity (`snd_doppler` … not present in 2013 convars; the doppler is implemented as the "DOPPLER" wave type in the mixer). There is no pitch-shift doppler for ordinary sounds. GMod does not change this. `[INF M]`

### 2.4 Occlusion `[LEAK M, PC-eng L (only `r_occlusion` / COcclusionSystem strings — those are the *render* occlusion system, not audio)]`
- Audio occlusion in Source: every 0.1 s? each positioned channel traces listener→source (`SND_GetGainObscured`): a blocked line applies `snd_obscured_gain_dB` (−2.7 dB) per blocked trace of a small set (centre + offsets), and foliage (`CONTENTS_… surf flag`) adds `snd_foliage_db_loss` 4 dB. Convar `snd_obscured_gain_dB` exists in 2007 engine; it is unknown whether GMod's engine kept it (no string in the dump). Treat as: mild, binary-ish muffling, −2.7 dB per obscured trace, not a full HRTF occlusion. `[LEAK M]`

### 2.5 Channel limits, sample rates, delays `[PC-eng H]`
- Max 128 mixer channels; dynamic channels are indices 0..63, static (CHAN_STATIC, looping ambients, soundscapes) 64..127; `total_channels == MAX_CHANNELS` is logged when exhausted and the new static sound is dropped.
- Accepted sample rates: 44 100, 22 050, 11 025 Hz; others print a warning (still play via resampler).
- Delay: a negative delay on a start means "skip that many seconds into the wave" (modulo length for loops), a positive delay schedules the start. Used for networked sound timing (`SND_DELAY` flag + `SoundTime`).
- Looping: WAV `cue`/`smpl` loop points are honoured; a looped static sound restarts at the cue.

### 2.6 Sound mixers `[PC-eng H]`
- `scripts/soundmixers.txt` defines mix groups per channel class (`CHAN_WEAPON`, `CHAN_VOICE`, `CHAN_VOICE2`, `CHAN_ITEM`, `CHAN_BODY`, `CHAN_STATIC`) and named mixers; the active mixer is `snd_soundmixer` (default `Default_Mix`), set by soundscapes (`Soundmixer` command) and by game code (e.g. freeze-cam uses it on the client: strings `snd_soundmixer`, `FreezeCam_Only`). Mixer group volumes multiply channel gain; a mixer with a lower `priority`? ducks. Default_Mix in HL2 leaves everything 1.0 except voice ducking. `[LEAK M]`
- Master volumes: `volume` (0..1, default 1), `volume_sfx`? (GMod client string — GMod adds a separate SFX volume convar), `snd_musicvolume` (0..1, default 1), `snd_mute_losefocus` (default 1: mute when the window loses focus). `[PC-cli H]`

---

## 3. DSP presets and room DSP

### 3.1 Presets `[PC-eng H]`
- Loaded from `scripts/dsp_presets.txt`: each preset is a number key with up to N processors; parse errors: "missing } or too many processors in preset #", unknown token. Processor types (2007 engine, strings in dump: filter, chorus, delay, envelope, LFO, pitch shifter, reverb, parameter clamping): `dly` (delay/echo: type plain/lowpass/linear/fade/ramp, delay ms, feedback, gain, tap), `rva` (parallel reverb: size, feedback, gain, numdelays, decay), `dfr` (diffusor), `lfo`, `ptc` (pitch shifter), `env` (envelope), `flt` (filter lowpass/highpass with cutoff, qwidth, quality), `crs` (chorus), `amp` (gain/distortion), `mdy` (modulating delay). Each preset has a global gain and `dsp_mix` etc. Up to 64 presets? (DSP_MAX?) `[LEAK M]`
- Well-known indices (dsp_presets.txt of HL2/GMod, verified by the Rust docs' own citation of preset 14 = water): 0 off, 1 "auto/generic", 14 underwater (`dsp_water` default 14), 2..17 room presets (smallroom..hugehall), 18+ special; the exact table is data in the game files — read `scripts/dsp_presets.txt` from the GMod install; do not hard-code. `[INF H]`

### 3.2 Which DSP is applied `[PC-eng M, LEAK M]`
- Convars: `dsp_room` (current room preset, default 0 = let auto pick), `dsp_player` (preset forced onto the *player* bus: set by the server via the engine for e.g. `player.SetDSP`/damage effects — the server function that sets it sends the console command "dsp_player %i" to the target client; `fastReset` is only valid client-side), `dsp_water` (14), `dsp_facingaway` (default 30?), `dsp_spatial` (40?), `dsp_speaker` (50?), `dsp_automatic` (0 = off; the auto room node system), `dsp_volume` (wet mix 0..1, set by soundscapes' `dsp_volume`), `dsp_off`, `dsp_enhance_stereo` (0), `dsp_slow_cpu`, `dsp_mix_min`/`dsp_mix_max` (0.2/0.8 distance-based wet mix), `dsp_db_min`/`dsp_db_mixdrop`. `[LEAK M; dsp_player and the "fastReset only valid from client" string PC-eng H]`
- **Room DSP (auto)**: when the room-dsp mode value equals the "automatic" setting (an engine global compared to 6 in the dump — the room preset is recomputed by the automatic node system) the engine shoots traces from the listener: 4800 u horizontal in 8 directions (constants 4800/−4800 and direction components 2496/2304/1248/1152), plus up (sky check), accumulates: room size (x,y), height, surface diffusion `dif`, reflectivity `refl`, count of objects `cobj`, sky visible `sky`, and picks a preset (`DSP PRESET` debug print). A new node is created when the listener moves far enough from the previous node ("NEW DSP NODE"); nodes are remembered (array of ~? nodes) and the preset crossfades (`dsp_mix` interpolation over ~0.5 s? LEAK). `[PC-eng M]` 
- Soundscape `dsp N` sets `dsp_room` to N immediately (crossfade is in the DSP mixer: the engine ramps the wet signal between the old and new preset over about 1 s — LEAK L). `dsp_player` from soundscapes sets the player preset. `[PC-cli H for the commands]`
- Wet/dry: positioned sounds get a wet send that increases with distance: `dspmix = clamp((dist - dsp_mix_min_dist)/(dsp_mix_max_dist - …))`, printed as `dspmix`/`distmix`/`dspface` in the debug line found in the dump. Sounds flagged `#` (music) and voice chat bypass room DSP. `[PC-eng M, LEAK M]`
- Underwater: when the listener's eyes are under water `dsp_water` (14) replaces the room preset (lowpass + slight reverb). `[LEAK H, SDK H (player code sets underwater DSP)]`

---

## 4. Footsteps

### 4.1 Player step timer / speed law `[SDK H for law; PC-srv H that GMod keeps the SDK structure; Lua H for intervals]`
- Runs in the shared player move code every tick: `step_time_ms -= 1000·frametime` (clamped ≥0). A step is played when timer ≤ 0 **and** the player is on ground (or on a ladder) **and** horizontal speed ≥ walk threshold.
- Thresholds (u/s): standing walk 90, run 220; ducked or on ladder: walk 60, run 80. `walking = speed < run`.
- Volume: by surface (`surfaceproperties` `gamematerial` letter) — default/concrete/metal/grate/tile/slosh 0.2 walk / 0.5 run; dirt 0.25/0.55; vent 0.4/0.7; ladder 0.5 (uses `ladder` surface); feet in water (water level 1): surface `water`, 0.2/0.5?; **waist-deep** (level 2): surface `wade`, volume 0.65, and only every 4th? step pattern (`m_iStep % 4`? — SDK: skips 3 of 4); ducked multiplies by 0.65. `[SDK H]` Rust parity recordings confirm 0.2/0.5/0.65 for concrete. 
- Interval after a step (GMod Lua `GM:PlayerStepSoundTime(ply, type, walking)` **≠SDK**): `STEPSOUNDTIME_NORMAL`/`WATER_FOOT`: by `Player:GetMaxSpeed()` ≤100 → 400 ms, ≤300 → 350 ms, else 250 ms; `STEPSOUNDTIME_ON_LADDER` → 450 ms; `STEPSOUNDTIME_WATER_KNEE` → 600 ms; crouching adds 50 ms. (SDK: 400 walk/300 run, ladder 350, knee 600, +100 crouch.) `[Lua H]`
- Jumping plays a step (full volume, flagged "force") in the SDK; landing plays `Player.FallDamage`-type sounds separately. `[SDK M]`

### 4.2 Server step emission `[PC-srv H]`
- Multiplayer and `sv_footsteps 0` → no step. `mp_footsteps 0` (multiplayer rules) → the rules' footstep gate returns false; it returns true when on a ladder or 2D speed > 220 u/s, otherwise false — i.e. in MP the *server* only emits running steps / ladder steps for others, walking steps are client-predicted only. `[PC-srv M; 220 is SDK, constant unresolved in dump]`
- Side alternates each step (`stepleft` / `stepright` of the surface's surfaceproperties entry; missing → `Default.StepLeft/Right`). Script params are cached per side when the script has exactly one wave. The script name can be overridden by a virtual (used by GMod's hooks).
- Recipients: PAS at the step origin. In MP, non-forced steps remove everyone in PVS of the origin (they predict their own and see others' via animation events); forced steps (`Player:PlayStepSound` Lua, hook) remove only the player himself unless the GMod force-hack flag is set (then he hears it too).
- **Lua hook `GM:PlayerFootstep(ply, pos, foot, sound, volume, filter)`**: `foot` 0 left / 1 right; return `true` suppresses the step. **≠SDK**
- Emit: channel CHAN_BODY (4), volume = computed step volume, level and pitch from the script (default 75/100), flags 0, origin = feet position.

### 4.3 NPC footsteps `[PC-srv H, PC-cli H (identical client copy)]`
- Material footstep: foot position = attachment `LeftFoot`/`RightFoot`, else bone `ValveBiped.Bip01_L_Foot`/`_R_Foot`, else origin. Trace down 48 u with the shot hull mask; needs a hit with a surface; use that surface's `stepleft`/`stepright`.
- Class overrides (by classname of the animating entity): `npc_dog` → `NPC_dog.FootstepLeft/Right`; `npc_combine_s`, `npc_zombine` when 2D speed² exceeds a run threshold → `NPC_CombineS.RunFootstepLeft/Right`; `npc_metropolice` same rule → `NPC_MetroPolice.RunFootstepLeft/Right`; `npc_stalker` → `NPC_Stalker.FootstepLeft/Right`; `npc_eli` left foot → `NPC_Eli.FootstepLeft`. Zombies via their own virtual: `npc_zombie`, `npc_zombine` → `Zombie.FootstepLeft/Right`; `npc_fastzombie` → `NPC_FastZombie.FootstepLeft/Right`; `npc_poisonzombie` → `NPC_PoisonZombie.FootstepLeft/Right`. Scanner sounds use `"<class>.<event>"` script names.
- Emit with PAS + attenuation filter derived from the script level (`attn = 4` if level ≤ 50 else `20/(level-50)`), CHAN_BODY, volume from the anim event (default 1), pitch/level from script.
- AI "footstep sound" also inserts an **AI sound event** (type combat?/player-footstep, channel 7, with the given volume as radius and duration) for NPC hearing; no audio. `[PC-srv M]`

---

## 5. Physics impact and scrape sounds (server) `[SDK H; not in dump; Rust srcds recordings confirm 70 u/s and (v/320)²]`
- Only entities with vphysics move type generate sounds. Self-hit (same entity, e.g. ragdoll limbs) requires ≥ 0.5 s since the last and uses CHAN_BODY; different entities use CHAN_STATIC.
- Per-collision gate: `deltaCollisionTime < 0.05 s` → skip; `speed < 70 u/s` → skip. `volume = min(1, (speed/320)²)`.
- Queue (per physics frame): entries keyed by (our surfaceprop, other surfaceprop); same key → coalesce: volume summed, max speed kept, origin of the louder. When > 4 entries queued, further ones coalesce into the nearest.
- At the end of the physics frame: for each entry, choose **impactHard** unless (`other.hardnessFactor < our.hardThreshold`) or (`our.hardVelocityThreshold > 0 and speed < our.hardVelocityThreshold`) → **impactSoft** (all from `scripts/surfaceproperties*.txt`: `impacthard`, `impactsoft`, `hardnessfactor`, `hardthreshold`, `hardvelocitythreshold`). Final volume = script volume × queued volume, clamped to 1; emitted from world (entity 0) at the contact position with the script's channel/level/pitch.
- Hidden (`gamematerial` `X`, e.g. `default_silent`, nodraw) and `playerclip` surfaces are silent. Break sounds use the surfaceprop `break` script.
- **Friction/scrape**: every friction callback computes energy = `0.5·m·v_rel²`-ish from the solver (`GetEnergy`); `energy < 75` → nothing. `energy *= 1/15500`; `vol = energy²`. Sound = `scraperough` unless `other.roughnessFactor < our.roughThreshold` → `scrapesmooth`. `vol ≤ 1/128` ignored. A new patch starts only when `scriptVolume·vol > 0.1`; while alive volume and pitch (`pitch = vol·(pitchHigh - pitchLow) + pitchLow` from the script range) are ramped with 0.1 s envelopes every frame; when energy drops the patch fades to 0 over 0.1 s and is removed; patch on CHAN_BODY of the moving object. In multiplayer only one friction effect per 0.5 s is sent.
- Dust / screen shake: dust puffs for `sand`/`dirt` ≥ 200 u/s and `concrete` ≥ 340 u/s; shake when mass ≥ 500 kg hits a static at > 30 u/s with > 0.25 s since last.
- Client Lua `ENT:PhysicsCollide(data, phys)` table (GMod): `HitPos, HitNormal, HitEntity, HitObject, HitSpeed, Speed, DeltaTime, OurOldVelocity, TheirOldVelocity, OurNewVelocity, TheirNewVelocity, OurOldAngularVelocity, TheirOldAngularVelocity, OurSurfaceProps, TheirSurfaceProps, PhysObject`. `[PC-cli H]`

---

## 6. Soundscapes

### 6.1 Server side `[PC-srv H, SDK H]`
- Entities `env_soundscape` (`radius`, `soundscape` name, `position0..7` target names, `StartDisabled`), `env_soundscape_proxy` (points at a main soundscape), `env_soundscape_triggerable` (touch-driven). System thinks every frame post-entity; each player is tested against the soundscapes of his PVS cluster (spatial partition) and against the current one: candidate if `radius > distance || radius == -1`, then LOS trace listener→soundscape with `MASK_SOLID_BRUSHONLY|MASK_WATER` from the player; the nearest visible one wins; the current one is kept until another is *closer* and visible. Enable/Disable inputs; entity Think interval 0.2 s.
- Winner is written to the player's networked audio params: `soundscapeIndex`, `localBits` (which of the 8 positions are valid), `localSound[0..7]` (world positions), `ent` (soundscape entity). `[PC-cli H (netvars)]`

### 6.2 Client side `[PC-cli H strings/commands identical to SDK c_soundscape.cpp, SDK H]`
- Files: `scripts/soundscapes_manifest.txt` → `file` entries; GMod also loads workshop/addon manifests. `cl_soundscape_flush` reloads; `playsoundscape <name> [radius=36]` forces one; `stopsoundscape`; `cl_soundscape_printdebuginfo`. GMod adds a Lua-accessible `Soundscape: %s` debug print.
- Commands inside a soundscape block: `dsp N` (sets `dsp_room`), `dsp_player N`, `dsp_volume f` (default 1), `fadetime f` (overrides `soundscape_fadetime` 3.0 s for that change), `Soundmixer name` (sets `snd_soundmixer`, only if game rules allow), `playlooping { volume, pitch, soundlevel|attenuation, position (index 0..7 | "random"), origin, radius, wave, suppress_on_restore }`, `playrandom { time "lo,hi", volume, pitch, soundlevel|attenuation, position, origin, rndwave { wave … }, suppress_on_restore }`, `playsoundscape { name, volume, position|positionoverride, ambientpositionoverride }` (sub-soundscape; recursion limit 8; `soundlevel` not allowed on sub-soundscapes).
- Interval syntax everywhere: `"a,b"` → `RandomFloat(a,b)`; a single number → fixed. `attenuation` is converted with `50 + 20/attn`.
- Looping sounds: emitted as **ambient sounds** (CHAN_STATIC, `SND_SHOULDPAUSE`?) at `position N` if given (else at the listener → no position: played as 2D ambient; with `position random` a point `36 u` from the listener at a random yaw each time the soundscape starts). Volume target = `masterVolume·RandomInterval(volume)`; pitch `RandomInterval(pitch)`; level from `soundlevel`.
- Crossfade: on soundscape change old loops fade out and new fade in over `fadetime` (3.0 s default) **linearly**, updated every client frame (`vol = target·(1 - elapsed/fade)`), sent as `SND_CHANGE_VOL` updates; loops reaching 0 are stopped.
- Random sounds: first fire at `now + 0.5·RandomInterval(time)`, then `now + RandomInterval(time)` after each; each fire picks `RandomInt` wave, `volume = master·RandomInterval(volume)`, `pitch = int(RandomInterval(pitch))`, `level = int(RandomInterval(soundlevel))`; with a position it is emitted as positioned CHAN_STATIC from entity 0 at that position; without a position as a 2D ambient.
- Entity-less fallback: if the map has no soundscape, nothing plays (no default soundscape). `[SDK H]`

---

## 7. Voice chat `[PC-eng M, SDK H]`
- Transport: Steam voice (the engine pulls compressed voice from the Steam API — "Failed to get voice data from Steam: %i"; codec is Steam's (Opus/SILK inside Steam's container) at 24 kHz? — GMod uses `voice_codec`? n/a: it is Steam voice, sample rate 11 025 → later 24 000 Hz). Packets are sent by the client as `clc_VoiceData`, broadcast by the server to allowed listeners (`voice_*` cvars: `sv_voiceenable` 1, `sv_alltalk` 1 in GMod default? (sandbox sets `sv_alltalk` via Lua? not verified), `voice_inputfromfile`, `voice_loopback` 0, `voice_scale` 1, `voice_overdrive` 2, `voice_overdrivefadetime` 0.4, `voice_fadeouttime` 0.1, `voice_modenable` 1, `voice_enable` 1, `voice_threshold`?). `[SDK/LEAK M]`
- Playback: per-speaker voice channel on `CHAN_VOICE_BASE + slot` at the speaker's position (3D, level ≈ 75? — SDK voice is played with `SNDLVL_TALKING` 75 dB unless `sv_alltalk`-style 2D? In HL2DM voice is 2D; GMod plays voice **positional** by default and Lua can scale it). Client hook **`GM:PlayerVoiceVolume`?** — the engine calls a GMod client callback named "GMOD_VoiceVolume" with (player, volume) for each mixed voice block; it drives the voice volume indicators (`Player:VoiceVolume()` returns 0..1) and the `voice_overdrive` ducking of other sounds (when anyone speaks, other channels are multiplied by `1/voice_overdrive` ramping over `voice_overdrivefadetime`). `[PC-eng M, SDK H for overdrive]`
- Mute: `Player:IsMuted()` / `SetMuted()` client-side; `voice_scale` master; `GM:PlayerCanHearPlayersVoice(listener, talker)` server hook returns (canHear, is3D) per pair each tick (`sv_alltalk` default) — GMod addition. `[Lua H]`
- `voice_loopback` plays the local mic back.

---

## 8. Main menu and music `[Lua H, PC-cli H]`
- GMod's main menu is HTML (`html/menu/menu.html`); its JS calls back into Lua `PlaySound(name)` → `surface.PlaySound(name)` (non-spatial, CHAN_STATIC? `surface.PlaySound` plays a 2D sound at `volume`=1 through the engine's local-sound path). Menu UI click/hover sounds come from the HTML/JS and default GMod menu uses **no background music** (there is no music track in the base game menu; `snd_musicvolume` is passed to the loading screen JS so loading-screen HTML may play audio). `[Lua H, INF M]`
- Sounds used by the menu Lua: `garrysmod/content_downloaded.wav` (icon browser), `buttons/button10.wav` (error notice), `buttons/button15.wav` (notify), `ambient/water/drip1-4.wav` (random drip for generic notices), `npc/roller/mine/rmine_chirp_answer1.wav` (undo/cleanup notice), `garrysmod/save_load1-4.wav`, `garrysmod/ui_click.wav`/`ui_return.wav`/`ui_hover.wav` (spawnmenu/derma controls via `surface.PlaySound`). `[Lua H]`
- Hit marker: engine references `sound/ui/hitsound.wav` (TF2-style `mp_hitsound`? GMod: `cl_hitsound`?) `[PC-eng L]`.
- In-game music: HL2 music is map-triggered `ambient_generic` with `#music/...` waves (the `#` prefix = no DSP, uses `snd_musicvolume`). `[SDK H, LEAK M]`

---

## 9. Client-side extras (GMod) `[PC-cli H]`
- `GM:EntityEmitSound` also runs client-side for client-emitted sounds (table includes `SentenceIndex`); unknown names resolve to `invalid.wav`? (placeholder string present).
- `sound.Add` client table: `name`, `channel`, `volume`, `level|soundlevel`, `pitch` or `pitchstart`/`pitchend`, `sound` (string or table).
- Freeze-cam/`FreezeCam_Only` mixer and `snd_soundmixer` switching exist in the client (TF2 legacy).
- `snd_mute_losefocus`, `volume`, `volume_sfx` handled by the GMod client (`volume_sfx` is GMod's separate SFX slider; applied as a multiplier on the mixer master). `[PC-cli M]`

---

## 10. Networking of sounds (for completeness) `[SDK H, PC-eng H]`
- Server → client `svc_Sounds`: per sound: entity index, channel, sound index (string table `soundprecache`), volume byte (×255), soundlevel (9 bits, +256 flag trick), pitch byte, flags (11 bits), origin (quantised 3×?), delay (float, `SoundTime` relative to server time: the client applies negative delay as a seek), speaker entity, `specialdsp`, `fromserver`, `staticsound`, `updatepositions`, `directionx/y/z` — the field names are visible in the engine (`StartSound` field list). Reliable when flagged (stop/changes). Max 1 sound per entity-channel per frame when not AUTO.

---

## 11. Summary of GMod ≠ SDK 2013 deltas
1. `.ogg` accepted as raw sound everywhere (`EmitSound`, ambient, sentences path). `[PC-srv H]`
2. `GM:EntityEmitSound` (server+client) with full rewrite ability, depth limit 100. `[PC-srv H]`
3. `GM:PlayerFootstep` suppress hook, `GM:PlayerStepSoundTime` intervals 400/350/250 (+50 crouch), ladder 450. `[PC-srv H, Lua H]`
4. Lua `EmitSound` recipient filter hard-capped at 2500 u (attenuation 0.8) independent of level; weapon sounds re-attributed to owner; default CHAN_WEAPON for weapons. `[PC-srv H]`
5. `CHAN_VOICE2` present in mixers. `[PC-eng H]`
6. `volume_sfx` and HTML menu sound bridge, voice-volume callback into Lua, `PlayerCanHearPlayersVoice`. `[PC-cli/eng M]`
7. Lua `sound.Add` with `pitchstart/pitchend`; virtual `scripts/sounds/lua.txt`. `[PC-cli H]`

## 12. Blockers / not derivable from this dump
- Engine gain/spatialise/doppler/occlusion core and the DSP processor maths were not in the dump beyond strings (§2.1–2.4, §3 flagged LEAK/INF).
- Server physics impact/friction sound functions and the player step-timer updater were not selected into the dump (§4.1, §5 are SDK; Rust srcds measurements corroborate thresholds).
- Music/menu: only the Lua/HTML bridge is visible; no engine music code in dump.
