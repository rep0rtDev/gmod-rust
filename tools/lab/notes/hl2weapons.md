# GMod `hl2weapons` – behaviour note

Scope: the HL2 player weapons as they exist in Garry's Mod's own binaries (Linux `server_srv.so` with symbols, Windows `client.dll` by strings/vtables): crowbar, stunstick, pistol, .357, SMG1 (+grenade), AR2 (+combine ball), shotgun, crossbow (+bolt), frag grenade (+grenade entity), RPG (+missile, laser dot), bugbait. SLAM: **not present** in GMod's binary (no class, no strings, no script) – only in SDK 2013 HL2MP. Bugbait grenade entity: not in dump (weapon side only).

Source/confidence tags used on every fact:
- `[bin:H]` read directly in the GMod pseudo-C (function logic or literal constant). `[bin-str:H]` string literal/precache/convar name in the binary.
- `[bin≈:M]` logic read in pseudo-C but a constant is a floating literal that the decompiler left as a data reference; value taken from the SDK where the surrounding logic is identical.
- `[sdk:H/M]` Source SDK 2013 (HL2MP shared files unless noted); used where the GMod class derives from an HL2MP base that is *not* in the dump (the HL2MP base classes `CBaseHL2MPCombatWeapon`, `CHL2MPMachineGun` and the HL2MP-specific weapon copies are compiled into GMod but were not exported into the dump; only the HL2-SP bases `CHLMachineGun`, `CBaseHLCombatWeapon`, `CBaseHLBludgeonWeapon` are).
- `[cfg:H]` `garrysmod/cfg/skill.cfg` / `scripts/weapon_*.txt` (GMod ships the HL2MP scripts unchanged – diff checked for the crossbow).
- `[rust-doc:M]` value already measured by the Rust project's harness (docs/spec) – quoted when the binary could not pin the number.
- `[inf:L]` inference.
Where GMod differs from SDK 2013 it is marked **DIFF**.

Units: Hammer units (u), seconds, degrees; angles in Source order (pitch, yaw, roll). Spread values are the *half-angle* of the cone as a 3-vector of sines (Source `Vector(x,y,z)` cone). Server tick in GMod = 66.67 Hz (0.015 s), all timers are compared against `curtime`.

---

## 0. Shared weapon machinery (applies to every weapon)

### 0.1 Button handling per tick (base weapon post-frame) `[bin:H]`
Order each tick while the weapon is active and the player is alive:
1. If the weapon is reloading and the reload end time has passed: finish reload (fill clip from reserve, `reloading=false`).
2. **Alt-fire** (button bit `IN_ATTACK2 = 1<<11`) and `nextSecondaryAttack <= now`: if the weapon uses secondary ammo and the secondary ammo (or clip2) is 0 → play weapon sound slot `EMPTY` and set `nextSecondaryAttack = now + 0.2` (binary literal 0.2 s; the same 0.2 s is used for the primary-empty click) `[bin:H]`; else call the weapon's secondary attack.
3. **Primary** (`IN_ATTACK = 1`) and `nextPrimaryAttack <= now`: if clip-based and clip1 == 0, or not clip-based and reserve == 0 → "fire on empty" handler: `EMPTY` sound, `nextPrimaryAttack = now + 0.2`; else if the player is not in a vehicle without weapon use → primary attack. Weapons that hold `IN_ATTACK` and are semi-auto gate on their own "button must be released" test (pistol, shotgun, 357 – see each).
4. **Reload** (`IN_RELOAD = 1<<13`) and `nextPrimaryAttack <= now` and the weapon uses clips and is not already reloading → reload.
5. If neither attack button is held and the weapon is not reloading: if clip1 is empty and the weapon uses clips → auto-reload or weapon switch ("reload-or-switch": reloads if reserve > 0 and `nextPrimaryAttack <= now`, otherwise switches to the best weapon after 0.5 s via `nextPrimaryAttack = now + 0.5`); then weapon idle animation when the idle timer has elapsed.

### 0.2 Default reload `[bin:H]`
`shells = min(maxClip - clip, reserve)` (and the same for clip2); if 0 → no reload. Sends the reload viewmodel activity; `player.nextAttack = weapon.nextPrimary = weapon.nextSecondary = now + duration(reload sequence)`; `reloading = true`. The actual clip transfer happens at the end (step 1 above). Most HL2 weapons use `ACT_VM_RELOAD`; the reload *sound* is a sequence event in the viewmodel (`Weapon_X.Reload`), not played by code, except where noted.

### 0.3 Weapon sounds `[bin:H]`
Sound slots (index → script key): 0 `empty`, 1 `single_shot`, 2 `single_shot_npc`, 3 `double_shot`, 4 `double_shot_npc`, 5 `burst`, 6 `reload`, 7 `reload_npc`, 8 `melee_miss`, 9 `melee_hit`, 10 `melee_hit_world`, 11 `special1`, 12 `special2`, 13 `special3`. Played at the weapon owner's position with the script's soundlevel (default 75 dB, volume 1, pitch 100) through a prediction filter (the firing client predicts it, others receive it). The `EMPTY` slot additionally inserts a combat sound entry (for NPC hearing) around the owner for 0.2 s. **GMod-specific**: a second dispatcher exists that the weapons call for non-predicted sounds; it uses the same slot table but a recipient filter from the owner's PAS and honours the Lua `EntityEmitSound` hook – audible result is identical to the SDK path `[bin:H]`. Script sound names per weapon are listed in each section `[cfg:H]`.

### 0.4 Hitscan
Bullets are fired with the shared `FireBullets` info: range 8192 u (`0x46000000` = 8192.0) `[bin:H]`, tracer every 2nd bullet for machine guns / every bullet for pistol-class (`tracer frequency` 2 vs 1) `[bin:H]`, damage from the ammo definition (`sk_plr_dmg_*` convar for players, `sk_npc_dmg_*` for NPCs – ammo defs are in GMod's game DLL, defaults below `[cfg:H]`), damage type `DMG_BULLET` (+`DMG_BUCKSHOT` for Buckshot). Tracer effect name comes from the ammo definition: `Tracer` for everything except AR2 which overrides to `AR2Tracer` `[bin-str:H]`. Tracer visual: 128 u long streak at 5000 u/s, material `effects/spark` (default tracer) / `effects/gunshiptracer` width 5 (AR2) `[sdk:M]`. Impact: `Impact` effect (decal from surface, sparks, dust), AR2 adds `AR2Impact` on every non-sky hit `[bin-str:H]`. Players hit by hitscan get the Source hit-group multipliers (head ×3 … via `sk_player_head`...) – outside this subsystem.

Ammo definitions (GMod `skill.cfg` defaults) `[cfg:H]`:

| ammo | player dmg | NPC dmg | max carry |
|---|---|---|---|
| Pistol | 5 | 3 | 150 |
| 357 | 40 | 30 | 12 |
| SMG1 | 4 | 3 | 225 |
| SMG1_Grenade | 100 | 50 | 3 |
| AR2 | 8 | 3 | 60 |
| AR2AltFire | – | – | 3 |
| Buckshot | 8 (per pellet) | 3 | 30 |
| XBowBolt | 100 | 10 | 10 |
| RPG_Round | 100 | 50 | 3 |
| Grenade | 150 | 75 | 5 |
| (crowbar) | 10 | 5 | – |
| (stunstick) | 10 | 40 | – |

Note: the Rust spec measured 75 dmg for the .357 and 8 for the pistol in GMod sandbox `[rust-doc:M]`; the HL2MP weapon-script `damage` keys (pistol 8, 357 75, smg1 5, ar2 11, buckshot 9, crossbow 100, frag 150, rpg 150) are what GMod uses in multiplayer for *player* bullets, because the HL2MP player `FireBullets` path takes damage from the weapon script (`GetHL2MPWpnData().m_iPlayerDamage`), not from `sk_plr_dmg_*` `[sdk:H]` – the binary confirms this for the crossbow bolt (damage copied from weapon data) and the RPG missile `[bin:H]`. Treat the weapon-script value as authoritative for players; `sk_*` for NPCs.

### 0.5 View punch helper for machine guns `[bin:H]` (SMG1, AR2)
`kick(player, dampEasy, maxVertical, fireDuration, slideLimit)`: reset the punch decay to 10 per second; `duration = min(fireDuration, slideLimit)`; `perc = duration / slideLimit`; punch = (−(0.2 + maxVertical·perc), −(0.2 + maxVertical·perc/3), 0.1 + maxVertical·perc/8); yaw and roll signs flipped randomly (50 %); the result is clipped so the accumulated punch never exceeds (24, 3, 1) in magnitude, then **halved** (×0.5) and added to the player's punch angle `[bin:H]` (constants 24/3/1 and 0.5 are literal in the binary; 0.2/0.2/0.1 are data refs = SDK `[bin≈:M]`).

### 0.6 Viewmodel idle/lower `[bin:H]`
HL2 base: weapon lowers (`ACT_VM_IDLE_LOWERED`, 2-state) while sprinting or when an NPC/friendly is in front; `Lower` sets idle time +… `[sdk:M]`. Viewmodel bob: the HL2 base's bob/sway functions are compiled as empty stubs on the server; the client bob is the standard HL2 one (cycle 0.45 s vertical / 0.9 lateral scaled by speed/320, max 1.0 vertical offset…) `[sdk:M]`.

---

## 1. Crowbar (`weapon_crowbar`)
Script `[cfg:H]`: view `models/weapons/v_crowbar.mdl`, world `models/weapons/w_crowbar.mdl`, no ammo, clip −1, bucket 0. Hold type "melee" `[inf:L]` (GMod hold types are set per weapon from Lua defaults).
- Range 75 u `[bin:H]`; refire 0.4 s `[bin:H]`; damage `sk_plr_dmg_crowbar` (10) for players, `sk_npc_dmg_crowbar` (5) for NPCs `[bin:H]+[cfg:H]`; damage type `DMG_CLUB` `[bin:H]`.
- **Alt-fire: none** (weapon's secondary attack is an empty function) `[bin:H]`.
- Swing (shared bludgeon base) `[bin:H]`: trace a line from eyes along forward for `range`; if nothing hit, trace a 32 u box (±16 u hull) the same way; if that hits, test the 'intersection' choice: pick the hull-trace end only if it is within `sqrt(27.7²)`≈27.7 u (hull corner radius) of the line – else keep the miss. Activity: hit → `ACT_VM_HITCENTER`, miss → `ACT_VM_MISSCENTER`; secondary swing (never triggered for crowbar) would use `ACT_VM_HITCENTER2/MISSCENTER2`. Sounds: miss → slot `melee_miss` (`Weapon_Crowbar.Single`), hit entity → `melee_hit` (`Weapon_Crowbar.Melee_Hit`), hit world → `melee_hit_world` (`Weapon_Crowbar.Melee_HitWorld`) `[bin:H]+[cfg:H]`.
- Timing: the swing is executed at the viewmodel's animation event **3001** (`AE_... melee hit`) – i.e. the hit happens a few frames into the swing, not at button press `[bin:H]`; `nextPrimary = nextSecondary = now + 0.4` on miss; on hit the same (fire rate) `[bin:H]`. Idle time = now + duration of the swing sequence.
- Hit: damage info with the melee force scale (`phys_pushscale`×…) `[bin:H]`; "trigger damage" (`TraceAttackToTriggers`) applied; impact effect = `Impact` trace effect with damage type `DMG_CLUB`; water hit (`watersplash` effect, trace from eye) `[bin-str:H]`; rumble effect on the player `[bin:H]`. View kick on hit: pitch random(1.0, 2.0), yaw random(−2.0, −1.0), roll 0 `[bin≈:M]` (SDK values; logic identical).
- Players: HL2MP adds `PLAYERANIMEVENT_ATTACK_PRIMARY` third-person animation `[sdk:M]`.

## 2. Stunstick (`weapon_stunstick`)
Script `[cfg:H]`: view `v_stunstick.mdl`, world `w_stunbaton.mdl`, no ammo, bucket 0.
- Range 75 u, refire 0.8 s `[bin:H]`. Damage `sk_plr_dmg_stunstick` 10 / `sk_npc_dmg_stunstick` 40 `[bin:H]+[cfg:H]`, type `DMG_CLUB|DMG_SHOCK` `[sdk:M]`.
- Alt-fire: none (empty) `[bin:H]`.
- Deploy → stun state ON: dispatches a `Sparks` effect at viewmodel attachment index 1 and plays `Weapon_StunStick.Activate`; Holster/Drop → OFF and plays the sound whose name in the binary is exactly **`Weapon_StunStick.Deactive`** (sic, not `Deactivate`) `[bin-str:H]`; the stun state is networked for the client glow `[bin:H]`.
- Hit world/entity: dispatches effect **`StunstickImpact`** at the trace end with the surface normal, plus the generic club impact trace `[bin-str:H]`. Sounds from script: swing `Weapon_StunStick.Swing`, miss `Weapon_StunStick.Melee_Miss`, hit `Weapon_StunStick.Melee_Hit`, world `Weapon_StunStick.Melee_HitWorld` `[cfg:H]`.
- Client glow while active `[sdk:M]`: halo sprite `sprites/light_glow02_add` (GMod: same) at the tip, random size 4–6 with colour 0.1–0.2 then 2–3 at 0.9–1.0, random `sprites/physbeam.vmt` arcs (width 0.5–2, amplitude 1–2) 1 in 6 frames.
- NPC event: Metropolice special stun/knockout of the player exists in the NPC animation-event path (not player relevant) `[bin:H]`.

## 3. Pistol (`weapon_pistol`)
Script `[cfg:H]`: `v_pistol.mdl` / `w_pistol.mdl`, clip 18, ammo `Pistol`, bucket 1, script damage 8. Sounds: `Weapon_Pistol.Single`, `.Reload`, `.Empty`, `.Special1`, `.Special2`, `.Burst`, NPC variants.
- Semi-automatic: holding the button fires at a **0.5 s** cadence `[bin≈:M]` (data ref; SDK 0.5; Rust measured 0.5 `[rust-doc:M]`). **Fast tapping**: in the pistol's own post-frame, a *new press* while `nextPrimaryAttack > now` is allowed to fire if at least **0.1 s** have passed since the last shot (the press sets `nextPrimaryAttack = now`) `[bin≈:M]` (logic literal; 0.1 is a data ref matching SDK `PISTOL_FASTEST_REFIRE_TIME`).
- Accuracy penalty `[bin:H]`: each shot adds **0.2 s** to `accuracyPenalty`; every tick (while the owner exists) it decays by `frametime`, clamped to **[0, 1.5]**. Spread cone = lerp(**1°**, **6°**, `penalty/1.5`) as a sine vector: 0.01745 … 0.10453 `[bin:H]` (ramp logic literal; the two cone endpoints are data refs equal to SDK `VECTOR_CONE_1DEGREES/6DEGREES` `[bin≈:M]`).
- Primary activity is `ACT_VM_PRIMARYATTACK` for the first shot, then `ACT_VM_RECOIL1`, `RECOIL2`, `RECOIL3` for shots 2, 3, 4+ of the current burst `[bin:H]`; the shot counter resets to 0 when the attack button is not held and `nextPrimaryAttack` has elapsed `[bin:H]`.
- View kick `[bin:H]`: pitch = −sharedRandom("pistolpax", 0.25, 0.5), yaw = sharedRandom("pistolpay", −0.6, 0.6), roll 0 (the binary uses the *shared* random streams, so client prediction matches).
- Dry fire: `EMPTY` sound + `ACT_VM_DRYFIRE`, `nextPrimary = now + 0.2` (SDK `SequenceDuration`; binary literal 0.2) `[bin:H]`.
- Reload: default reload with `ACT_VM_RELOAD`; on success penalty = 0 and shotsFired = 0 `[bin:H]`. Reload duration = viewmodel sequence (≈1.4333 s measured `[rust-doc:M]`).
- Alt-fire: none.
- Underwater: fires (SDK `m_bFiresUnderwater = true`) `[sdk:M]`.

## 4. .357 Magnum (`weapon_357`)
Script `[cfg:H]`: `v_357.mdl` / `w_357.mdl`, clip 6, ammo `357`, bucket 1, script damage 75; sounds `Weapon_357.Single`, empty `Weapon_Pistol.Empty`.
- Primary `[bin:H]`: one hitscan bullet with a **zero** spread cone for players (**DIFF vs SP SDK**, which uses the weapon cone); the weapon's generic cone getter returns 0.01 for players / 0.04363 for NPCs `[bin:H]` but is not used by the player shot.
- Fire rate: `nextPrimary = nextSecondary = now + 0.75` `[bin≈:M]` (data ref; SDK 0.75; Rust measured 0.75 `[rust-doc:M]`).
- Sound `single_shot`; viewmodel `ACT_VM_PRIMARYATTACK`; player anim event; `!HEV_AMO0` suit message when clip & reserve both 0 `[bin:H]`.
- View punch `[bin:H]`: pitch **−8** (literal), yaw = sharedRandom(stream `357Punch`, −2, 2), roll 0; the shared-random streams `357X`/`357Y` feed the SP-only eye snap below. Implement: `ViewPunch(−8, rand(−2,2), 0)`.
- Eye-angle "snap" (`SnapEyeAngles` of ±1° on pitch/yaw) happens **only in single-player** (binary checks `gpGlobals->maxClients == 1`) → never in GMod MP `[bin:H]`.
- Reload: default `ACT_VM_RELOAD` (3.6667 s measured `[rust-doc:M]`). Alt-fire: none.

## 5. SMG1 (`weapon_smg1`)
Script `[cfg:H]`: `v_smg1.mdl` / `w_smg1.mdl`, clip 45, ammo `SMG1`, secondary `SMG1_Grenade`, bucket 2, script damage 5; sounds `Weapon_SMG1.Single/.Reload/.Empty/.Special1/.Special2/.Double/.Burst`.
- Primary: full-auto via the HL2MP machine-gun base (not in dump) `[sdk:H]`: while held, `nextPrimaryAttack += 0.075` per shot and the loop catches up missed shots (`fireRate = 0.075 s`, 13.33 rps) `[sdk:H]+[rust-doc:M]`; spread cone **0.04362 (= sin 2.5°)** literal in the binary `[bin:H]` (SP SDK uses 0.08716/5° → **DIFF**); tracer every 2nd bullet; `Weapon_SMG1.Single` per shot; `ACT_VM_PRIMARYATTACK`; `!HEV_AMO0` when clip and reserve are empty.
- View kick `[bin:H]`: machine-gun kick with (dampEasy **0.5**, maxVertical **1.0**, fireDuration = time the button has been held, slideLimit **2.0**) → see 0.5. Literal constants in binary 0.5/1.0/2.0.
- Reload `[bin:H]`: default reload with `ACT_VM_RELOAD`; on success the weapon-script `reload` sound is fired by the sequence. (1.5 s measured `[rust-doc:M]`.) Also used as auto-reload when clip empty (0.1 above).
- **Alt-fire – grenade** `[bin:H]`: if no `SMG1_Grenade` ammo **or** the player is underwater (waterlevel 3) → `EMPTY` sound and `nextSecondaryAttack = now + 0.5` `[bin≈:M]`; else:
  - `src = player.weaponPosition (eye)`, `vel = eyeForward * 1000` (literal 1000) `[bin:H]`; creates `grenade_ar2` at `src` with angles (0,0,0)?/`vel`, random angular velocity **(±400, ±400, 0)** deg/s (literal random −400..400 on two axes) `[bin:H]`, thrower = player, damage = **100** (`sk_plr_dmg_smg1_grenade`), damage radius **250** (`sk_smg1_grenade_radius`) `[bin:H]+[cfg:H]`.
  - Sends `ACT_VM_SECONDARYATTACK`, sound `Weapon_SMG1.Double` (slot `double_shot`), player attack anim `[bin:H]`; decrements `SMG1_Grenade`; `nextPrimary = now + 0.5`, `nextSecondary = now + 1.0` `[bin:H]` (SDK SP: 0.5 / 1.0 → same); registers a danger sound (NPC AI) `[bin:H]`.
  - `!HEV_AMO0` when the grenade ammo hits 0 `[bin:H]`.
- Dry fire of alt-fire does *not* consume ammo.

### 5.1 SMG1 grenade entity (`grenade_ar2`) `[bin:H]`
- Model `models/Weapons/ar2_grenade.mdl`; solid bbox ±3 u; movetype `FLYGRAVITY` with gravity scale **400/600 = 0.667** `[sdk:H]` (binary data-ref), friction **0.8** `[bin:H]`; collision group **22** (GMod-specific group; SDK uses `COLLISION_GROUP_PROJECTILE`) `[bin:H]`; danger radius 100 u `[bin:H]`.
- Visuals: glow sprite `sprites/redglow1.vmt` (kRenderGlow 255/255/255/200, scale 0.2) and smoke trail `[sdk:M]` (the binary spawn contains the literals 48, 12, 4, 4, 24, 0.2 for the trail/sprite setup; think every 0.1 s `[bin:H]`).
- **Touch: detonates immediately on any contact** except its own thrower/triggers (no bounce) `[bin:H]` (touch handler is 12 lines → detonate). Think: every 0.1 s; fades / self-destructs after its life (SDK: `m_flDetonateTime` none; grenade just flies).
- Detonate `[bin:H]`: `TE_Explosion` temp-entity (fireball `sprites/zerogxplode`, 15 frames, magnitude from the damage, flags `TE_EXPLFLAG_NONE`), a trace along the velocity to find the surface and decal **`Scorch`** (**`SmallScorch`** if the damage is ≤ 50 i.e. NPC-fired), radius damage (**100**, radius **250**, `DMG_BLAST`), screen shake amplitude **25**, frequency **150**, duration **1.0 s**, radius **750 u**, then removes itself. Explosion sound comes from the TE (`BaseExplosionEffect.Sound`).

## 6. AR2 (`weapon_ar2`)
Script `[cfg:H]`: `v_irifle.mdl` / `w_irifle.mdl`, clip 30, default 60, ammo `AR2`, secondary `AR2AltFire`, bucket 2, script damage 11. Sounds: `Weapon_AR2.Single`, `Weapon_AR2.Reload`, empty `Weapon_IRifle.Empty`, double `Weapon_IRifle.Single` (alt-fire launch), special1 `Weapon_CombineGuard.Special1` (charge-up).
- Primary: machine-gun base, `fireRate = 0.1 s` `[sdk:H]+[rust-doc:M]`; spread cone **0.02618 (= sin 1.5°)** literal `[bin:H]` (SP SDK 0.05234/3° → **DIFF**); tracer `AR2Tracer` every 2nd shot `[bin-str:H]`; `AR2Impact` effect dispatched at every bullet impact that is not sky `[bin:H]`.
- View kick `[bin:H]`: machine-gun kick (§0.5) with dampEasy **0.5**, maxVertical **2.0**, fireDuration = held time, slideLimit **1.0** (literals 0.5/2.0/1.0 present; argument order taken from the SDK) `[bin≈:M]`.
- Viewmodel pose parameter `VentPoses` (0..1 by heat) is set by the weapon `[bin-str:H]` (SDK: not present for HL2MP → **DIFF**/GMod keeps SP behaviour).
- Reload: `ACT_VM_RELOAD`, duration ≈1.5667 s `[rust-doc:M]`; `Weapon_AR2.Reload` from sequence.
- **Alt-fire – combine ball** `[bin:H]`:
  1. On press (needs `AR2AltFire` ammo ≥ 1, `nextSecondary <= now`, not already charging): plays `special1` (`Weapon_CombineGuard.Special1`), sends `ACT_VM_FIDGET` (charge animation), sets `delayedFire = true`, `delayedFireTime = now + 0.5` (literal), `nextPrimary = nextSecondary = delayedFireTime`; the shot fires in the post-frame once `now >= delayedFireTime` `[bin:H]`.
  2. Delayed fire: start = player eye (weapon position), direction = autoaim forward; creates the ball through the shared ball launcher with radius **`sk_weapon_ar2_alt_fire_radius`** (default **10**), mass **`sk_weapon_ar2_alt_fire_mass`** (**150**), lifetime **`sk_weapon_ar2_alt_fire_duration`** (**4 s** in the HL2MP source GMod is built from; SP has 2 – the compiled defaults are not in the dump and skill.cfg does not set them) `[bin-str:H]+[sdk:M]`, speed **1000 u/s** `[bin:H]`, flagged weapon-launched `[bin:H]`.
  3. Effects: `Weapon_IRifle.Single` (slot `double_shot`), `ACT_VM_SECONDARYATTACK`, white screen fade colour (255,255,255) alpha **64**, duration **0.1 s**, fade-out type `[bin:H]`, view punch pitch **−sharedRandom(8, 12)**, yaw **sharedRandom(1, 2)** `[bin:H]`, player secondary attack anim; `AR2AltFire` −1; `nextPrimary = nextSecondary = now + 1.0` `[bin:H]`.
  4. Underwater: binary checks `waterlevel == 3` → `EMPTY` sound and no fire `[bin:H]`.

### 6.1 Combine ball (`prop_combine_ball`) `[bin:H]`
- Model `models/effects/combineball.mdl`; VPhysics sphere of the launch radius, mass **750** when launched (literal), heavy-object + dissolve physics flags, drag/damping 0.5 literal in the physics setup `[bin:H]`; collision group `COLLISION_GROUP_PROJECTILE` while launched, interactive while held by the physgun `[bin:H]`.
- Speed 1000 u/s, keeps its speed after every bounce (velocity re-normalised to the launch speed each collision) `[sdk:H]`; glow trail `sprites/combineball_trail_black_1.vmt` width = radius, fade… `[bin-str:H]`.
- Launch sound `NPC_CombineBall.Launch`; whiz-by `NPC_CombineBall.WhizFlyby` (think every tick; plays when passing within ~? u of a player), impacts `NPC_CombineBall.Impact`, kill `NPC_CombineBall.KillImpact`, held `NPC_CombineBall.HoldingInPhysCannon`, explosion `NPC_CombineBall.Explosion`; episodic variants `NPC_CombineBall_Episodic.*` used when the map/game is episodic (never in GMod) `[bin-str:H]`.
- Bounce: effect `cball_bounce` at impact point + sound; after `max bounces` (weapon-launched: unlimited until lifetime) … lifetime `duration` seconds from launch then **explode** `[bin:H]`.
- Hit entity `[bin:H]`: damage info type `DMG_DISSOLVE (1<<26)` with damage **`sk_npc_dmg_combineball` 15** for players? (SDK; GMod does not override) — for NPCs the damage is set to the NPC's max health (always kills & dissolves) `[sdk:H]` (`sk_combineball_seek_angle 15`, `sk_combineball_guidefactor` **1.0 in GMod skill.cfg vs 0.5 SDK – DIFF**, `sk_combine_ball_search_radius 512`, `sk_combineball_seek_kill 0` `[cfg:H]`). After killing an NPC the ball deflects toward the next enemy within the seek angle `[bin:H]`.
- Explosion `[bin:H]`: three beam rings of `sprites/lgtning.vmt` from the ball's position: ring A start radius = ball radius, end **128 u**, life **0.25 s**, width **48**; rings B/C end radius **384 u**, life **0.2 s** and **0.5 s**, width **64**; colour **(255,255,225) alpha 64**, fade flags 8; effect `cball_explode`; sound `NPC_CombineBall.Explosion`; screen shake amplitude **20**, frequency **150**, duration **1.0 s**, radius **1250 u**; the ball stops, becomes non-solid and is removed. No radius damage (SDK) `[sdk:M]`.

## 7. Shotgun (`weapon_shotgun`)
Script `[cfg:H]`: `v_shotgun.mdl` / `w_shotgun.mdl`, clip 6, ammo `Buckshot`, bucket 3, script damage 9. Sounds: `Weapon_Shotgun.Single`, `.Double`, `.Reload` (per shell), `.Empty`, `.Special1` (pump), NPC variants.
- Spread cone **0.08716 (5°)** (`VECTOR_CONE_10DEGREES` in SDK = 0.17365 → **DIFF**: GMod literal is 0.08716 = sin 5°) `[bin:H]`.
- **Primary** `[bin:H]`: semi-auto (requires the button to be re-pressed); `nextPrimaryAttack = now + duration(primary sequence)` (= **0.3333 s** measured `[rust-doc:M]`); pellets = **`sk_plr_num_shotgun_pellets` (7)** read from the convar `[bin:H]`; `Weapon_Shotgun.Single`; `ACT_VM_PRIMARYATTACK`; view punch pitch sharedRandom(**−2, −1**), yaw sharedRandom(**−2, 2**), roll 0 `[bin:H]`; clip −1; `needPump = true`; `!HEV_AMO0` when clip and reserve are empty.
- **Alt-fire (double)** `[bin:H]`: requires clip ≥ 2 (with 1 shell it fires the primary instead; with 0 it reloads/dry-fires); fires **12 pellets** (literal), `Weapon_Shotgun.Double` (`double_shot`), `ACT_VM_SECONDARYATTACK`, punch pitch sharedRandom(**−5, 5**), yaw 0; clip −2; `nextPrimary = nextSecondary = now + duration(secondary sequence)` (**0.5 s** measured `[rust-doc:M]`); `needPump = true`.
- **Pump** `[bin:H]`: when `needPump` and `nextPrimary <= now` → `ACT_SHOTGUN_PUMP`, sound `Weapon_Shotgun.Special1`, `needPump = false`, `nextPrimary = nextSecondary = now + duration(pump sequence)` = **0.5333 s** (literal/measured) `[bin:H]+[rust-doc:M]`. `delayedFire` flag: if the player pressed attack while a pump was pending the shot fires right after the pump.
- **Reload (shell-by-shell)** `[bin:H]`:
  - Start: requires reserve > 0 and clip < 6 and not pumping; if `needPump` → pump first. Sends `ACT_SHOTGUN_RELOAD_START`; `nextPrimary = nextSecondary = now + duration` (**0.5 s**) `[bin:H]+[rust-doc:M]`; `inReload = true`.
  - Each shell: `ACT_VM_RELOAD` with `Weapon_Shotgun.Reload` sound and **0.4 s** per shell `[bin:H]`; clip +1, reserve −1 after the shell animation; between shells `IN_ATTACK` or `IN_ATTACK2` **interrupts** the reload (finish → pump not needed) and fires at the shell boundary `[bin:H]`.
  - Finish: `ACT_SHOTGUN_RELOAD_FINISH`, **0.4333 s** `[bin:H]+[rust-doc:M]`; `inReload = false`.
  - Viewmodel bodygroups track the loaded shells (shell visibility) `[bin:H]`.
- Dry fire: `EMPTY` sound + `ACT_VM_DRYFIRE`, `nextPrimary = now + duration` `[bin:H]`. Auto-reload when the clip is empty and the button released (0.1 step 5) `[bin:H]`. Underwater: blocked (fires-underwater false) `[bin:H]`.
- Post-frame special: HL2MP shotgun calls the base after its own handling; `IN_ATTACK` held after a reload-start finishes the reload early (`m_bNeedPump`) `[bin:H]`.

## 8. Crossbow (`weapon_crossbow`)
Script `[cfg:H]` (GMod's copy == SDK): `v_crossbow.mdl` / `w_crossbow.mdl`, clip 1, default 5, ammo `XBowBolt`, bucket 3, script damage 100; sounds: `Weapon_Crossbow.Single` (fire), `Weapon_Crossbow.Reload`, special1 `Weapon_Crossbow.BoltElectrify`, special2 `Weapon_Crossbow.BoltFly`.
- Constructor state `[bin:H]`: zoomed false, `mustReload` false, reloads singly, **fires underwater**.
- **Primary** `[bin:H]`: if the clip is empty → reload if reserve > 0, otherwise `EMPTY` sound and `nextPrimaryAttack = now + 0.2`. Otherwise:
  - `src = player.weaponPosition (eye)`, dir = autoaim forward; bolt created at src with angles of dir, owner = player, damage = weapon-script damage (**100**) `[bin:H]`; velocity **3500 u/s** in air, **1500 u/s** when the player's waterlevel is 3 `[bin:H]`.
  - clip −1; view punch **(−2, 0, 0)** `[bin:H]`; sounds `single_shot` (`Weapon_Crossbow.Single`) and `special2` (`Weapon_Crossbow.BoltFly`) `[bin:H]`; `ACT_VM_PRIMARYATTACK`; player anim; `!HEV_AMO0` if clip & reserve 0; `nextPrimary = nextSecondary = now + 0.75` (literals `0x3f400000`) `[bin:H]`; idle time = now + sequence; charger → DISCHARGE (skin 0, sprite brightness 0, sprite off) `[bin:H]`; `mustReload = true`.
- **Reload / "electrify"** `[bin:H]`: `mustReload && idleTimeElapsed` → reload (default reload, `ACT_VM_RELOAD`, `Weapon_Crossbow.Reload` from sequence); reload clears `mustReload`. While the reload animation is playing and the clip is (going to be) > 0, the weapon sets viewmodel **skin 1** (glowing bolt) and charger state **READY**; the charger state machine: `START_LOAD` → plays special1 **`Weapon_Crossbow.BoltElectrify`** + dispatches client effect **`CrossbowLoad`** (arcs on the bolt, SDK client: lightning beams `sprites/lgtning` from the viewmodel bolt attachment) `[bin-str:H]`; `START_CHARGE` → charger sprite (`sprites/blueflare1.vmt`, attached to viewmodel) brightness **32 over 0.5 s**, scale **0.025 over 0.5 s**, turned on; `READY` → brightness **80 over 1.0 s**, scale **0.1 over 1.0 s**; `DISCHARGE`/`OFF` → skin 0, brightness 0, sprite off `[bin:H]`. Deploy with clip > 0 → skin 1 (loaded). Holster/Drop → stop effects (OFF), exit zoom `[bin:H]`. Precached: `sprites/light_glow02_noz.vmt`, `sprites/blueflare1.vmt`, `Weapon_Crossbow.BoltHitBody/BoltHitWorld/BoltSkewer` `[bin-str:H]`.
- **Alt-fire = zoom toggle** `[bin:H]`: on `IN_ATTACK2` *press* (edge): if not zoomed → player FOV **20** blended over **0.1 s**, zoomed = true; else FOV 0 (default) over **0.2 s**; checked every tick including while busy/reloading. No sound in binary (the SDK HL2MP zoom also has none; client draws the scope overlay). The weapon-specific secondary attack function is empty.
- Lower/raise: standard HL2 base.

### 8.1 Crossbow bolt (`crossbow_bolt`) `[bin:H]`
- Spawn: model **`models/crossbow_bolt.mdl`** (**DIFF vs HL2MP SDK** `w_missile_closed.mdl`), movetype `FLYGRAVITY` with custom fly-collision, solid bbox **±1 u**, gravity **0.05** (so ~30 u/s² drop), hitbox-and-solid trace mask without grates; glow sprite `sprites/light_glow02_noz.vmt` (kRenderGlow 255/255/255/128, scale 0.2) created but **off**; bubble think every 0.1 s: while underwater, 5 bubbles along the last 0.1 s of travel `[bin:H]+[sdk:H]`.
- **Hit damageable entity** `[bin:H]`: sound **`Weapon_Crossbow.BoltHitBody`**; damage info `DMG_BULLET|DMG_NEVERGIB`, damage 100, force = melee damage force with scale **0.7**, applied via trace-attack + multidamage; `Impact` trace effect; the multiplayer branch then **removes** the bolt (the SP-only branch would pin ragdolls with `Weapon_Crossbow.BoltSkewer`) `[bin:H]` — **DIFF vs SP SDK**.
- **Hit world/prop** `[bin:H]`: sound **`Weapon_Crossbow.BoltHitWorld`** (on every world contact, including each ricochet); `hitDot = -(surfaceNormal · flightDir)`; if `hitDot < 0.5` (more than 60° off the normal) **and** speed > 100 u/s → **ricochet**: `v' = reflect(dir, n) * speed * 0.75`, gravity scale → **1.0**, keeps flying; else **stick**: velocity 0, the bolt stays where the ±1 u hull stopped (origin = hit end − 1 u along the normal), `Impact` trace effect, client effect **`BoltImpact`** dispatched at the hit point with the surface normal (not dispatched when the thing hit is a physics prop – the bolt is parented to it instead), `Sparks` effect unless the point is in water, the bolt becomes `EF_NODRAW` immediately after the effect is sent, its glow sprite is turned on and faded out over **3 s**, and the entity is removed after **2 s** `[bin:H]`. (SP HL2 keeps the physical bolt for 10 s; GMod/HL2MP replaces it with the client-side `BoltImpact` effect → **DIFF**.)
- Owner-hit and sky (`SURF_SKY`) → remove silently `[bin:H]`.

## 9. Frag grenade (`weapon_frag`)
Script `[cfg:H]`: `v_grenade.mdl` / `w_grenade.mdl`, clip −1, default 1, ammo `grenade` (ammo type name `Grenade`), bucket 4, script damage 150; sounds: special1 **`WeaponFrag.Roll`**, single/double `common/null.wav`. Precached by code: `npc_grenade_frag`, `WeaponFrag.Throw`, `WeaponFrag.Roll` `[bin-str:H]`. Hold type "grenade" `[bin-str:H]`.
- Constants `[bin:H]+[sdk:H]`: fuse **2.5 s**, damage radius **250 u**, throw-position check hull ±6 u (= grenade radius 4 + 2), rethrow delay **0.5 s**, attack timers parked at a huge value (`FLT_MAX`) while the pull-back animation plays.
- **State machine** `[bin:H]`:
  1. `IN_ATTACK` press (needs ammo, `nextPrimary <= now`): `ACT_VM_PULLBACK_HIGH`, `redraw = false`, `attackPaused = PRIMARY`, timers parked; third-person `PLAYERANIMEVENT_ATTACK_GRENADE`? (HL2MP: `PLAYERANIMEVENT_ATTACK_PRIMARY` on throw).
  2. `IN_ATTACK2` press: `ACT_VM_PULLBACK_LOW`, `attackPaused = SECONDARY`.
  3. While paused and the pull-back sequence has finished, on **release** of the button: primary → `ACT_VM_THROW`; secondary → if ducking `ACT_VM_SECONDARYATTACK` (roll) else `ACT_VM_HAULBACK` (lob); `attackPaused = NO`; `nextPrimary = nextSecondary = now + duration(sequence)`. The grenade entity is created when the viewmodel sequence fires its primary-attack animation event (`AE_WPN_PRIMARYATTACK`): the handler throws / lobs / rolls according to the paused mode; after that the ammo is decremented, `redraw = true`, and `nextPrimary = nextSecondary = now + 0.5` `[bin:H]`.
  4. When `redraw` is set and the throw sequence has finished: `redraw = false`; if ammo > 0 send `ACT_VM_DRAW` and set `nextPrimary = nextSecondary = now + duration(draw sequence)` (measured 1.2667 s `[rust-doc:M]`); with no ammo left the HL2MP base switches to the next best weapon and removes the frag `[bin:H]+[sdk:M]`.
  5. If the player dies / weapon holstered mid-pull: timers reset to now `[sdk:M]`.
- **Throw (primary)** `[bin:H]`: `src = eye + forward*18 + right*8` then throw-position check (trace hull ±6 from eye to src, back off to the trace end if blocked); `dir = forward`; `dir.z += 0.1` (literal); `vel = playerVelocity + dir*1200`; angular impulse **(600, random(−1200, 1200), 0)**; grenade angles (0,0,0); timer 2.5 s; combine-spit false; sound **`WeaponFrag.Throw`** (`EmitSound` at the player, not a weapon slot) `[bin:H]`; `ACT_VM_THROW` already playing; damage = weapon script (150), radius 250.
- **Lob (secondary, standing)** `[bin:H]`: `src = eye + forward*18 + right*8 + (0,0,−8)`; `vel = playerVelocity + forward*350 + (0,0,50)`; angular impulse **(200, random(−600, 600), 0)**; sound slot `double_shot` = `common/null.wav` → **silent** `[bin:H]+[cfg:H]`.
- **Roll (secondary, ducking)** `[bin:H]`: `src` = bottom-centre of the player's hull (collision box at normalized (0.5,0.5,0)) raised 4 u, then traced 16 u down (so it sits on the ground) and moved 18 u along the yaw-only facing; `vel = facing*700 + playerVelocity`; angular impulse **(0, 0, 720)** (roll around the forward axis); grenade orientation roll −90 so it lies on its side; sound **`WeaponFrag.Roll`** (slot special1) `[bin:H]+[sdk:H]`.
- Dropping the weapon/no ammo: HL2MP removes the weapon when the last grenade is thrown `[sdk:M]`.

### 9.1 Frag grenade entity (`npc_grenade_frag`) `[bin:H]`
- Model `models/Weapons/w_npcnade.mdl`; VPhysics solid (physics prop), collision group **11 = WEAPON** ; precache `Grenade.Blip`, `sprites/redglow1.vmt`, `sprites/bluelaser1.vmt`; glow sprite `redglow1` kRenderGlow 255/255/255/200 scale **0.2** attached (red blinking light), trail `bluelaser1` kRenderTransAdd red (255,0,0,255) start width **8**, end width **1**, lifetime **0.5 s** `[bin:H]+[sdk:H]`.
- Physics: a plain VPhysics object created from the model (`w_npcnade.mdl`: its own mass/surfaceprop from the .phy) `[bin:H]`; bounces and **bounce sounds** come entirely from the VPhysics impact-sound system for that surfaceprop (`Weapon.ImpactSoft` / `Weapon.ImpactHard`-class physics sounds) – there is no grenade-specific bounce sound code in the GMod binary `[bin:H]+[inf:M]`.
- Timer `[bin:H]`: `SetTimer(detonate = now + 2.5, warn = detonate − 1.5)`: a 'blip' think plays **`Grenade.Blip`** every **1.0 s** until the warn time, then every **0.3 s**; at `detonate` it explodes. Physgun / gravity-gun pickup: the thrower becomes the picking player and the timer is reset with `SetTimer(1.5, 0.75)` (explodes 1.5 s after pickup, fast blips after 0.75 s) `[bin:H]`.
- Explosion (base grenade) `[bin:H]`: trace 32 u down from the origin; `TE_Explosion` (fireball `sprites/zerogxplode`, magnitude from the damage) at the trace end pulled back along the normal; decal **`Scorch`**; sound **`BaseGrenade.Explode`**; radius damage **150** in **250 u** (`DMG_BLAST`, inflictor = grenade, attacker = thrower); screen shake **25 / 150 / 1.0 s / 750 u**; the grenade becomes `EF_NODRAW`, non-solid and is removed `[bin:H]`. Underwater the TE is flagged no-fireball with a water-splash variant `[sdk:M]`.
- Being damaged (shot, another blast) kills the grenade → immediate detonation `[bin:H]`.

## 10. RPG (`weapon_rpg`)
Script `[cfg:H]`: `v_rpg.mdl` / `w_rocket_launcher.mdl`, clip −1, default 3, ammo `rpg_round` (type `RPG_Round`), bucket 4, script damage 150; sounds `Weapon_RPG.Single`, special1 `Weapon_RPG.LaserOn`, special2 `Weapon_RPG.LaserOff`, empty `Weapon_SMG1.Empty`. Precached: `Missile.Ignite`, `Missile.Accelerate`, `sprites/redglow1.vmt`, `effects/laser1.vmt`, `effects/laser1_noz.vmt`, `rpg_missile` `[bin-str:H]`.
- **Primary** `[bin:H]`: refused while a live missile from this launcher exists; with no ammo → `EMPTY`. Otherwise `nextPrimaryAttack = now + 0.5`; the missile is created at `eye + forward*12 + right*6 + up*(−3)` (offsets are data refs equal to the SDK `[bin≈:M]`) with the player's eye angles, owner = player; a short trace forward decides whether the missile starts in 'grace' (no collision with its owner) for **0.3 s** `[bin:H]`; missile damage = weapon-script damage (**150**) `[bin:H]`; ammo −1; `ACT_VM_PRIMARYATTACK`; `Weapon_RPG.Single`; player attack anim. While the missile lives the launcher cannot fire or reload `[bin:H]`.
- **Reload** `[bin:H]`: requires reserve > 0; `nextPrimary = now + duration(ACT_VM_RELOAD)`; reload sound slot `reload`; HL2MP: reload is triggered automatically in post-frame when the missile has died and `nextPrimary` elapsed (`m_bInitialStateUpdate`…) → measured: both timers **1.6667 s** after the rocket dies `[rust-doc:M]`.
- **Alt-fire: nothing** – no secondary-attack function and no `IN_ATTACK2` test anywhere in the RPG's functions `[bin:H]` (**DIFF vs SP SDK** where the laser is always on too; there is no toggle in either).
- **Laser dot / guiding** `[bin:H]`: start guiding when the weapon is deployed/raised → guiding = true, `Weapon_RPG.LaserOn` (special1), an `env_laserdot` entity is created (owner = player, non-solid, no-move, ±4 u hull, hidden with `EF_NODRAW` when off); stop guiding → `Weapon_RPG.LaserOff` (only if this weapon is the active one), the dot is hidden and removed; 'suppress' hides/shows the dot without changing the guiding state (used while lowered/sprinting). Every tick the laser position is updated by a trace from the player's eye along the eye direction (shot mask, 8192 u); the dot sits at the hit point. When the launcher has **no ammo and no live missile**, guiding stops `[bin:H]`. Client: the dot is a `sprites/redglow1.vmt` glow; the viewmodel also draws a **128 u** `effects/laser1_noz.vmt` beam from the muzzle attachment `[sdk:M]`.
- Post-frame: if the player is sprinting/lowered → `SuppressGuiding(true)` else false; `UpdateLaserPosition` each tick `[bin:H]`.

### 10.1 Missile (`rpg_missile`) `[bin:H]`
- Spawn: model `models/weapons/w_missile_launch.mdl` `[sdk:M]`, movetype `FLYGRAVITY` (gravity 1.0) for the first **0.3 s** (ignite think at +0.3) `[bin:H]`; initial velocity = `forward*300 + (0,0,128)` `[bin≈:M]` (300 literal; the up component is a data ref equal to the SDK; the Rust harness measured 119 u/s after the first half-tick of gravity); hull ±4 u `[sdk:M]`.
- **Ignite** at +0.3 s `[bin:H]`: movetype `FLY` (no gravity), model `models/weapons/w_missile.mdl`, looping sound **`Missile.Ignite`**, velocity re-aimed along the current angles; an 'accelerate' think **0.5 s** later plays **`Missile.Accelerate`** and sets the speed to **1500 u/s** (Rust measured ignition at tick 20 and acceleration 33 ticks later `[rust-doc:M]`); a rocket smoke trail entity is attached (spawn rate 100/s, particle life 0.5 s, colour 0.65 grey → black, size 8 → 32, spawn radius 4, speed 2–16, opacity 0.75) `[sdk:M]`; the seek think runs every **0.1 s** `[bin:H]`.
- **Guidance** `[bin:H]`: each seek think picks the visible laser dot (the dot must be on and a shot-mask trace from the missile to it must be clear; the dot whose direction is closest to the current heading wins when several exist); `targetDir = normalize(dot − missile)`, `curDir = normalize(velocity)`, `newDir = normalize(curDir·(1 − 0.125) + targetDir·0.125)` (homing factor is a data ref equal to SDK `0.125` `[bin≈:M]`), `velocity = newDir · |velocity|`, angles = newDir. With no visible dot the missile flies straight. If the dot sits on a living entity, the SDK aims at that entity's body target `[sdk:M]`.
- **Touch** `[bin:H]`: explodes on anything solid (owner excluded during the 0.3 s grace): stops `Missile.Ignite`, creates an `env_explosion` at its position with damage **150** and radius **250 u** (SDK `EXPLOSION_RADIUS` – data ref `[bin≈:M]`), flags no-sparks/no-dlights/no-smoke; the `env_explosion` produces the `Explosion` TE (fireball, `Scorch` decal, `BaseExplosionEffect.Sound`) and the radius damage; the missile is then removed.
- **Shot down** (the missile has health and can be damaged): dispatches effect **`RPGShotDown`**, then 'augers' – tumbles with random angular velocity for `random(1.0, 2.0)` s and explodes `[bin-str:H]+[sdk:H]`.
- Sky hit → removed silently; water → explodes `[sdk:M]`.

## 11. Bugbait (`weapon_bugbait`)
Hold type "grenade" `[bin-str:H]`; precache `npc_grenade_bugbait`, `Weapon_Bugbait.Splat` `[bin-str:H]`; spawn hull ±4 u `[bin:H]`.
- Deploy → spore emitter effect on (`bugbait` sprite spores), holster → off `[bin:H]`.
- Primary `[bin:H]`: `ACT_VM_HAULBACK` on press (pull back), `ACT_VM_THROW` on release; the grenade is created at the throw animation event: `src = eye + forward*18 + right*8` (with the ±6 u hull check), `vel = playerVelocity + forward*1000` (literal 1000 `[bin:H]`), angular impulse **(600, random(−1200,1200), 0)**, entity `npc_grenade_bugbait` `[bin:H]`; `nextPrimary = now + duration(sequence)`; redraw with `ACT_VM_DRAW`; no ammo is consumed.
- Secondary `[bin:H]`: squeeze: `Weapon_Bugbait.Splat` sound, `ACT_VM_SECONDARYATTACK`, `nextSecondary = now + duration(sequence)`, triggers antlion follow; no projectile.
- Drop → spore explosion emitter, removed after a delay `[bin:H]`. The grenade entity (`npc_grenade_bugbait`) is **not in the dump**: SDK: `models/weapons/w_bugbait.mdl`, `GrenadeBugBait.Splat` on touch, splat sprite/`bugbait_splat` effect, radius 512 antlion attraction `[sdk:M]`.

## 12. Differences from Source SDK 2013 (summary)
1. .357 player spread literal 0.01 vs SDK cone 0.01745; fired with **zero** spread anyway (HL2MP). Eye-angle snap only in SP → never in GMod `[bin:H]`.
2. Pistol spread 1°→6° by penalty (SDK HL2MP same); Rust spec assumes 0.5°→3° → binary wins.
3. SMG1 cone 0.04362 (2.5° half) **vs SP 5°** ; AR2 cone 0.02618 (1.5°) **vs SP 3°**; shotgun cone 0.08716 (5°) **vs SP 10°**: GMod uses the HL2MP values (half of SP).
4. Crossbow bolt model `models/crossbow_bolt.mdl` (SDK HL2MP: `w_missile_closed.mdl`); MP bolts are hidden 2 s after sticking (SP: stays 10 s, skewers ragdolls).
5. RPG has no alt-fire at all. AR2 alt-fire duration 4 s (HL2MP) not 2 s (SP). `sk_combineball_guidefactor` 1.0 (GMod cfg) vs 0.5 (SDK code default).
6. Empty-click delay 0.2 s (binary literal) for both buttons.
7. Weapon sounds go through GMod's own dispatcher honouring Lua hooks; same audible result.
8. GMod-only: hold-type strings per weapon, Lua class hooks, `VentPoses` kept on AR2, collision group 22 for SMG grenade.
9. SLAM: absent in GMod.
