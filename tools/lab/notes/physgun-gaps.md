# `physgun` – gap list: Garry's Mod vs gmod-rust (`main`, clone of 2026-11-09)

Rust files referenced (paths relative to the repo root, line numbers from the clone):
- `crates/gm-server/src/lib.rs` – `fn physgun` L21944–22158 (input, pickup, drop, freeze, rotate), `fn hold_targets` L22161–22195, `set_prop_frozen` L22997, `part_body`/`part_frozen` L12103/12111, params default L8791.
- `crates/gm-physics/src/physgun.rs` – `PhysgunParams` L14–35, `Grab` L39–90 (`rotate` L56–60, `new` L62–79, `target` L82–90), `controller` L92–108, `angular_controller` L110–130.
- `crates/gm-physics/src/lib.rs` – `hold` L938–962, `release` L964–972, `freeze` L975–993.
- `crates/gm-client/src/main.rs` – `PHYSGUN_BEAM_COLOR` L41, viewmodel table L472–483, materials L1426–1439, beam/glow/dlight draw L5034–5178, mouse wheel L7581–7623.
- `crates/gm-render/src/world.rs` – `MAX_DLIGHTS` L377, `DLight` L387–394, `DEFAULT_WEAPON_COLOR` L397, `beam_strip` L4577–4624.
- `crates/gm-sandbox/src/lib.rs` – `MAX_TRACE_LENGTH` L1052, `tool_trace` L1256.
- `docs/spec/physgun.md`, `docs/spec/convars-defaults.md` L39–52.

Priority = how visible to a player: **P0** obvious, **P1** noticeable, **P2** subtle. "Game" facts refer to physgun.md (same numbering).

## A. Behaviour-by-behaviour comparison

| # | Behaviour (game) | Rust today | Match | Gap / fix | Prio |
|---|---|---|---|---|---|
| 1 | **Beam shape**: 3 quadratic Bézier beams start→control→end, control = shootPos + aim·|start−end|·K2; widths 2.0 / rand 2–5 / rand 2–6 u re-rolled per frame; materials `physbeama` while holding, `physbeam` otherwise (§10) | one cubic Bézier, p1 = start + muzzleDir·len·0.5, width **12 u**, colour ×2, 24 segments (`main.rs` L5125–5168); `physbeama` when held else `physbeam` (L5140–5144) | partial | wrong curve family (cubic, bent along the *muzzle* axis instead of the *aim*), one strip instead of three, no per-frame width flicker, width 6× too wide (the 12 u is a work-around for a dim texture – fix the material/HDR scale instead) | **P0** |
| 2 | **End glow**: 7 sprites (2 when idle) of `sprites/physbeam.vmt`, each random 1–16 u, alpha random 100–255, colour = weaponColour×255 (§10) | one `physg_glow1` quad, 12 u, constant colour (L5170–5173) | no | wrong material, wrong count/size/alpha randomisation → the game's end glow flickers and is much brighter when holding | **P0** |
| 3 | **Beam colour** = player's `GetWeaponColor()` (`cl_weaponcolor`, default 0.30 1.80 2.10, over-bright allowed) (§10) | hard-coded `[0.3, 1.0, 1.0]` (L41, used L5111); `DEFAULT_WEAPON_COLOR` (0.3,1.8,2.1) exists in `world.rs` L397 but is not what the beam uses | no | use the player's weapon colour (per player, networked userinfo), keep >1 components (additive) | **P0** |
| 4 | **Beam for other players**: drawn for every player's physgun, start = world-model attachment 1 (§10) | drawn only for the local player while LMB is down (`self.weapon == 0 && self.mouse.contains(&0)`, L5095); no world-model start, no beam state networked to other clients | no | network held entity / grab point / enabled per player; draw from world model attachment 1 for others | **P0** (multiplayer) |
| 5 | **Idle beam**: with the gun out but not holding, the game draws **only the muzzle effect**; no beam to the aim point (§10, "not holding") | a full beam to the aim trace end whenever LMB is held without a target (L5097–5107) | no | the game shows no beam unless the beam entity is enabled (something was picked up or the primary attack hit); remove the idle aim-beam | **P0** |
| 6 | **Dynamic light**: muzzle-effect helper (not in dump) – game has a dlight; radius/colour unknown (§10) | dlights at end (r 160) and muzzle (r 48, ×0.5) (L5112–5124) | unknown | cannot be verified from the supplied dump; keep, mark as unmeasured | P2 (unverifiable) |
| 7 | **Beam start** = viewmodel attachment **1** (§10) | attachment named `"muzzle"` with a hard-coded fallback (L5098–5101) | likely ok | confirm attachment #1 of `c_superphyscannon.mdl` is the one named muzzle | P2 |
| 8 | **Halo** around held entity (sandbox Lua): weaponColour + rand·0.3, size rand 1–2, additive (§10) | none | no | add halo pass when `physgun_halo 1` | P1 |
| 9 | **Pickup trace** length 2×`physgun_maxrange` = 8192 u, filter ignores owner, hit must be a non-world entity (§4) | `tool_trace` to `MAX_TRACE_LENGTH` 56756 u (`lib.rs` L22070, `gm-sandbox` L1052/1256) | no | clamp the pickup ray to 8192 u (configurable), use `MASK_SHOT_HULL`-style contents; today a prop 20 000 u away can be grabbed | P1 |
| 10 | **Hold distance** initial = |hit − shootPos| (§5.4) | `(hit − eye)·forward` (`physgun.rs` L73) | ok (hit is on the ray) | – | – |
| 11 | **Mouse wheel** changes distance by `wheel × physgun_wheelspeed` (10 u), clamped 40..4096 only when the wheel moves (§5.4) | wheel only drives menus/binds (`main.rs` L7581–7623); no distance change at all | no | network the wheel delta in the usercmd (GMod extends usercmd with it), apply on the server | **P0** |
| 12 | **E + W/S** moves distance toward max/min range at K u/s; **E + A/D** spins yaw at `phys_spinspeed` 200 °/s (§5.4, §6) | none | no | implement (K unresolved; SDK value 100 u/s as placeholder) | P1 |
| 13 | **Rotate mode** (E held): mouse → `dPitch = −dy·0.05`, `dYaw = dx·0.05`, applied in the yaw-only eye frame (pitch about view right, yaw about world up) (§6) | `Grab::rotate` L56–60: yaw about world Z, pitch about `right` = (sin yaw, −cos yaw, 0), 0.05 °/unit; applied `pitch*yaw*rot` | ok | order of the two elementary rotations within one tick differs slightly from `AngleMatrix(dPitch,dYaw,0)` (yaw then pitch); sub-degree; the spec's open "mouse y axis" question is answered: view right (yaw-only eye frame) | P2 |
| 14 | **View lock in rotate mode**: player's view does not turn while E is held with an object (`DIROVERRIDE` flag) (§6) | no view lock found client-side (no `IN_USE` handling in `main.rs`); the view keeps turning and the yaw-follow then also turns the prop | no | freeze view angles client-side while E is held and something is held | **P0** |
| 15 | **Angle snap** E+Shift → `gm_snapangles` (45°) increments; **grid snap** `gm_snapgrid` on the target (§6, §5.2) | none | no | implement snap on the target angles when `IN_USE && IN_WALK`; grid snap default 0 so lower priority | P1 |
| 16 | **Yaw follow**: target yaw += Δ eye yaw each tick (§6) | `Grab::target` L82–90, turn by (yaw − grab yaw) | ok | – | – |
| 17 | **Target origin uses body's current orientation** (§5.2) | `eye + fwd·d + body_rot·offset` L87 | ok | – | – |
| 18 | **Controller law**: vphysics shadow control with maxSpeed 5000, maxAngular 5000, maxDampSpeed/Angular 10000, dampFactor 0.8, teleportDistance 0, timeToArrive 0.05 (0.1 ragdoll), re-read every step (§5.3) | fitted law `v' = 0.2v + Δ/T` clamp 5000 (`physgun.rs` L92–130); `time_to_arrive` single value 0.05 (`PhysgunParams`), `max_dv` hack for ragdolls (`lib.rs` L22187–22189) | mostly | add `time_to_arrive_ragdoll = 0.1` (ragdoll entities use it – the measured ragdoll overshoot in docs/spec is likely this), the maxDamp* terms, the jeep angular scaling; convars-defaults.md names (`physgun_maxSpeedDamping`, `physgun_maxAngularDamping`, `physgun_DampingFactor`) are not the binary's (`physgun_maxDampSpeed`, `physgun_maxDampAngular`, `physgun_dampFactor`) | P1 |
| 19 | **Mass while held = 45678 kg** (unless heavier / ragdoll / vehicle), restored on drop (§5.1) | nothing (no mass change) | no | a held crate in the game shoves other props and players aside with ~infinite inertia; ours is pushed back | **P0** (visible whenever a held prop touches anything) |
| 20 | **Pickup unfreezes every physics object of a non-ragdoll; ragdoll: grabbed bone only** (§4) | `set_prop_frozen(ent,false)` for props (whole prop); for ragdoll parts `set_prop_frozen` returns false (not a prop) so a frozen bone may stay frozen while "held" (`lib.rs` L22076–22083, L22997–23003) | partial | unfreeze the grabbed ragdoll bone on pickup | P1 |
| 21 | **Drop**: hook `PhysgunDrop`, keep velocity, gravity back next tick, `SetPhysicsAttacker(ply, curtime)` (§7) | `release` keeps velocity, gravity back (L964–972); `GameEvent::PhysgunDrop` (L22104) | ok | physics attacker credit not tracked (damage attribution) | P2 |
| 22 | **Freeze**: hook `OnPhysgunFreeze(weapon, physobj, ent, ply)` → Lua `EnableMotion(false)` on the grabbed object only, `AddFrozenPhysicsObject`, `PlayerFrozeObject`; viewmodel activity 181; cooldown (§8) | `freeze(b)` kinematic + zero velocity (L975–993), only the held body; ragdoll entity frozen when all parts are (L22106–22127) | ok | no `GetUnFreezable` check, no attack cooldown after freeze (constant unknown) | P2 |
| 23 | **Unfreeze (reload)**: double-press < 0.25 s → all; else eye-trace target + `constraint.GetAllConstrainedEntities`, `CanPlayerUnfreeze` per object, `Wake()` (§8) | reload unfreezes **every frozen prop owned by the player** on every press (L22045–22064) | no | the game only unfreezes what you aim at (and its constrained group) – "all" only on a double tap; ours clears the whole map's frozen props of that player on one tap | **P0** |
| 24 | **Pickup rules** (`PhysgunPickup` Lua): no players, `PhysgunDisabled`, `physgun_limited`, persistent, `ent.PhysgunPickup` override (§9) | `Request::PhysgunPickup` asked of the addon runtime (L22073–22075) | ok (delegated) | sandbox defaults (player refusal etc.) live in Lua; make sure the fallback refuses players when no Lua answers | P2 |
| 25 | **Non-VPHYSICS entities** (NPCs, brushes) are held via the swept-move path (§5.5) | only prop/ragdoll bodies (`part_body`) | no | rare in sandbox play | P2 |
| 26 | **Two players on one entity** (§4) | `held` is per player; physics `hold` would be called twice per tick on the same body (last wins) | partial | the game runs two shadow controllers (sum of velocities set sequentially – effectively also "last wins" within IVP's controller ordering, but both pull); rare | P2 |
| 27 | **Viewmodel anim**: activity 181 (`ACT_VM_SECONDARYATTACK`) on initial press and on freeze; nothing physgun-specific otherwise (§2) | `fire` on attack, `hold_idle` while holding (`main.rs` L472–483) | unknown | map activity 181 → the model's sequence; the `hold_idle` loop is not sent by the weapon code – verify in game whether the claws stay open from another path | P1 |
| 28 | **Sounds**: none (§2) | none (status.md says "hold loop sound not played yet") | ok | drop the "not yet" note: the game plays nothing | – |
| 29 | **Hold/beam state networked** (held entity, local grab point, beam entity) (§2) | client gets `held_pos` for the local player (L5081–5089) | partial | other players need it (see #4) | P0 (with #4) |
| 30 | **Hold target from post-move eye** (§3 order) | `hold_targets` after moves (L22161) | ok (measured) | – | – |
| 31 | **Deploy/holster/death** drop + beam removal (§2) | player leave releases (L10989); holster/switch: weapon slot change clears? not found | partial | drop held on weapon switch and death | P1 |
| 32 | `physgun_drawbeams`, `physgun_halo`, `effects_freeze/unfreeze` client convars (§1) | none | no | add (defaults 1) | P2 |

## B. Things gmod-rust does that the game does not
- Draws an "idle" beam from the muzzle to the aim point while LMB is held with nothing grabbed (`main.rs` L5097–5107). The game draws only the muzzle glow when nothing is held.
- Beam width 12 u with colour ×2 (L5160–5166) to compensate for a dim texture; the game's beams are 2 u / 2–5 u / 2–6 u wide at the player's weapon colour.
- Dynamic lights at both beam ends with chosen radii 160/48 u (L5112–5124) – not verifiable (the game's values are in a function missing from the dump).
- `glow2` sprite at the muzzle 6 u, `glow1` 12 u at the end (L5170–5177): the game's end glow uses the *beam* material as sprites (7 × 1–16 u random), the glow materials belong to the muzzle helper.
- Reload unfreezes every frozen prop of the player at once (L22045–22064); the game needs a double tap for that.
- `max_dv` force limit for ragdolls (`PhysgunParams.max_dv`, `lib.rs` L22187–22189) – not in the game; the game instead uses `physgun_timeToArriveRagdoll = 0.1`.
- Pickup trace reaches 56756 u; the game stops at 8192 u.
- Mouse-y rotation composed `pitch * yaw` (`physgun.rs` L59) – the game composes one `AngleMatrix(dPitch, dYaw, 0)`; negligible.

## C. Ranked gap summary
1. P0 – Beam look: 3 random-width quadratic beams along the aim, `physbeam` end sprites ×7, player weapon colour (#1, #2, #3).
2. P0 – No beam/hold visible for other players; idle aim-beam that the game never draws (#4, #5, #29).
3. P0 – Mouse wheel does not change hold distance (#11).
4. P0 – View keeps turning in rotate mode (#14).
5. P0 – Held mass not set to 45678 kg: held props don't bulldoze (#19).
6. P0 – Reload unfreezes everything instead of the aimed constrained group / double-tap-all (#23).
7. P1 – Pickup ray 56756 u vs 8192 u (#9); E+W/S distance and E+A/D spin (#12); E+Shift angle snap (#15); ragdoll `timeToArrive` 0.1 and maxDamp terms (#18); frozen ragdoll bone not unfrozen on pickup (#20); halo (#8); viewmodel activity mapping (#27); drop on holster/death (#31).
8. P2 – physics attacker, GetUnFreezable, cooldowns, non-physics entities, two holders, convar name spellings in docs, dlight (unverifiable).
