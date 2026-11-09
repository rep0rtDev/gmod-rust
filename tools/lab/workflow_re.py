# Dynamic workflow: one spec agent per game subsystem reads the Ghidra pseudo-C of the real GMod
# binaries (uploaded as attachments by the orchestrator, never committed), writes a behaviour note
# plus a gap list against the Rust crates, and one combine agent ranks everything into a report.
# Run through `run_workflow`; inputs come from .local/re/dump/attachments.json (subsystem -> URLs).
import asyncio
import json
import os

REPO_ROOT = os.path.expanduser("~/repos/gmod-rust")
SUBSYSTEMS = json.load(open(os.path.join(REPO_ROOT, "tools/lab/subsystems.json")))
ATTACH = json.load(open(os.path.join(REPO_ROOT, ".local/re/dump/attachments.json")))
REPO_URL = "https://github.com/rep0rtDev/gmod-rust.git"

SCOPE = {
    "physgun": ("Physics gun (weapon_physgun): pickup trace and range, grab point / shadow controller fit, hold distance, rotate mode (E), wheel distance change, freeze/unfreeze, the beam (start attachment, bezier control points, width, materials, colour from the player's physgun colour, glow sprites at both ends, the dynamic light and its radius), the sounds, the viewmodel animations, what happens for players, ragdolls, constrained groups, frozen props, prop protection hooks", "crates/gm-server (physgun), crates/gm-client/src/main.rs (physgun beam), crates/gm-render/src/world.rs (beam_strip, dlights)"),
    "gravgun": ("Gravity gun (weapon_physcannon in GMod): pickup trace, allowed masses/sizes, hold distance and controller, punt force and its mass law, pull force, the open/closed claws and their sprites/glows (muzzle glow dots), the core sprite, punt/pull/dryfire/pickup/drop sounds, the viewmodel sequences, mega-physcannon absent in GMod, the effects dispatched on punt", "crates/gm-server/src/weapons.rs, crates/gm-server (gravity gun), crates/gm-client/src/main.rs"),
    "toolgun": ("Toolgun / sandbox C++ side (gmod_tool, gmod_camera, scripted entities CSENT/CSWEP, CLuaEffect): toolgun fire rate, trace, the screen texture animation (what drives it, frame rate), the beam/tracer effect, sounds, which tool hooks are called in which order, how Lua SWEPs/SENTs get their callbacks, camera weapon behaviour", "crates/gm-sandbox, crates/gm-server (tools), crates/gm-script (SWEP/SENT/TOOL bridges), crates/gm-client"),
    "hl2weapons": ("HL2 weapons as they exist inside GMod's binaries (they differ from Source SDK 2013): crowbar, pistol, 357, SMG1 (+grenade), AR2 (+combine ball), shotgun, crossbow (bolt flight, stick, ricochet, sounds, electrify on reload), frag grenade (primary throw, secondary lob, roll, timer, bounce sounds), RPG (missile, laser dot, guidance), bugbait, stunstick, slam if present: fire rates, damage, spread cones, recoil/punch, clip sizes and ammo defs, reload timings and sequences, every sound name, every effect dispatched, viewmodel activities, how alt-fire works per weapon", "crates/gm-server/src/weapons.rs, crates/gm-server/src/projectile.rs, crates/gm-client/src/main.rs (viewmodel, fire effects), crates/gm-client/src/particles.rs"),
    "movement": ("Player movement (CGameMovement and GMod changes): walk/run/sprint speeds, acceleration, friction, air control, jump (height, forward boost), step, crouch duck/unduck timing and the eye-offset curve (TIME_TO_DUCK, spline), crouch-jump, ladder, water, noclip, view punch, footstep timing/volume by speed and surface, fall damage thresholds, the duck hull sizes, the view offset values, how the client predicts them", "crates/gm-move, crates/gm-server (player tick), crates/gm-client/src/main.rs (eye height, prediction)"),
    "player": ("Player and game rules (CGMOD_Player/CHL2_Player/CGMODRules): health/armour/damage formulas incl. multiplayer damage table and head multiplier, death flow (sounds, ragdoll creation, screen fade colours/times, death camera, respawn conditions and delay, what weapons are given on spawn), flashlight on/off rules and sounds, suit power, use key (+use pickup of small props: distances, masses, carry), player model/hands, spawn point selection, team/scoreboard data, hints/notifications, ammo definitions", "crates/gm-server/src/lib.rs, crates/gm-server (player, damage, use), crates/gm-client/src/main.rs (death, fade, HUD)"),
    "clientfx": ("Client effects: explosions (every sprite/particle emitted by the explosion effect: counts, sizes, lifetimes, colours, velocities, the dynamic light, the shake), muzzle flashes per weapon, impact effects per material (sparks, dust, blood), decals (which decal per material, sizes, how they are placed and blended, lifetime limits), bullet tracers, water splashes, smoke/fire trails, combine ball sprites, beams in general (width, texture scroll), sprite rendering rules (additive vs alpha, scale by distance, orientation modes)", "crates/gm-client/src/particles.rs, crates/gm-client/src/main.rs (fx_bursts, decals), crates/gm-render/src/world.rs (sprites, decals)"),
    "render": ("Client rendering and view: FOV handling (default fov, fov_desired, aspect scaling, zoom), viewmodel FOV/offset/bob/sway/lag, flashlight (projected texture, cone, falloff, shadows), dynamic lights law on world and models, HDR/tonemap/bloom defaults, fog, 3D skybox and sky paint, water (refract/reflect), lightmaps and light styles, model lighting (ambient cubes), shadows (render-to-texture blob), bone setup / interpolation / extrapolation and cl_interp timing, prediction basics", "crates/gm-render, crates/gm-client/src/main.rs (camera, viewmodel, flashlight)"),
    "sound": ("Sound: soundemitter system (soundscripts, channels, pitch/volume randomisation, SNDLVL distance law), DSP presets and room DSP, soundscapes, footsteps, physics impact sounds (surfaceproperties, speed thresholds, volume law), how the engine mixes (spatialisation, doppler, occlusion if any), voice chat basics, what plays in the main menu, music", "crates/gm-audio, crates/gm-server (WorldSound), crates/gm-client (sounds)"),
    "vphysics": ("VPhysics (IVP based): contact/friction model, restitution, damping (linear/angular), drag coefficients, gravity, buoyancy, sleep thresholds, shadow controllers (teleport distance, max speed, damping, time to arrive), player controller, motion controllers, constraint solver behaviour (weld/hinge/slider/pulley/ballsocket/rope breaking), performance settings (max velocity, max angular velocity), collision rules and groups, ragdoll constraints", "crates/gm-physics, crates/gm-collision, crates/gm-server (physgun shadow, constraints)"),
    "props": ("Props, ragdolls, constraints and brush entities server side: prop_physics spawn/mass/health/breaking (prop data, gibs), prop_ragdoll creation and bone follow, constraint entities (phys_constraint/hinge/slide/pulley/ballsocket/lengthconstraint/fixed/motor/torque/thruster/magnet) with their keyvalues and break logic, +use pickup, vehicles (jeep/airboat physics basics), doors/buttons/triggers, entity I/O, entity factory and class list", "crates/gm-server (props, constraints, mapio), crates/gm-physics"),
    "luabind": ("Lua binding C++ side (lua_shared, CLuaInterface/CLuaObject, gamemode callbacks): how hooks are dispatched and in which order, the entity/player/weapon metatables, network vars and SendTable/DT_ layout, user messages and net library framing, timers, convars/concommands, file library sandbox paths, error handling, the engine->Lua callback list", "crates/gm-script, crates/gm-net/src/msg.rs"),
    "ui": ("Client UI/HUD C++ side: HUD elements and their layout/fonts/colours (health, ammo, suit, crosshair, weapon selection incl. slot order and timeouts, damage indicator, hints, chat, death notices, target id, notifications), screen fades, the main menu/loading screen composition, scheme fonts (resource/*.res), scaling rules by resolution (640x480 proportional), spawn/context menu C++ hooks", "crates/gm-render/src/ui.rs, crates/gm-client/src/main.rs (HUD, weapon selector, menu)"),
}

RULES = """Rules of this research lab (follow them exactly):
- The pseudo-C comes from Ghidra on the user's own licensed Garry's Mod binaries. It is an input for understanding only. Do NOT copy code, do NOT quote pseudo-C, do NOT keep original function/class/variable identifiers in your deliverables; describe behaviour in plain language with numbers, formulas, units, state machines, sound/material/convar names (asset names are fine).
- Never commit, push or upload the pseudo-C anywhere; it stays on your VM and is deleted at the end.
- Source SDK 2013 (https://github.com/ValveSoftware/source-sdk-2013) and GMod's Lua (https://github.com/Facepunch/garrysmod) may be cloned read-only as references; where GMod's binary differs from the SDK, the binary wins and you say so.
- Everything you report must be traceable: for each fact say where it came from (pseudo-C function count/strings/convar, SDK, Lua, or inference) and your confidence.
"""

SPEC_SCHEMA = {
    "type": "object",
    "properties": {
        "subsystem": {"type": "string"},
        "note_url": {"type": "string", "description": "attachment URL of the behaviour note markdown"},
        "gap_url": {"type": "string", "description": "attachment URL of the gap list markdown"},
        "summary": {"type": "string", "description": "5-10 lines: the most important facts found and the biggest gaps"},
        "top_gaps": {"type": "array", "items": {"type": "string"}, "description": "up to 8 one-line gaps ranked by how visible they are in game"},
        "facts_count": {"type": "integer"},
        "gaps_count": {"type": "integer"},
        "blockers": {"type": "string", "description": "what could not be derived and why (empty if none)"},
    },
    "required": ["subsystem", "note_url", "gap_url", "summary", "top_gaps", "facts_count", "gaps_count", "blockers"],
}

COMBINE_SCHEMA = {
    "type": "object",
    "properties": {
        "report_url": {"type": "string"},
        "summary": {"type": "string"},
        "top_gaps": {"type": "array", "items": {"type": "string"}},
    },
    "required": ["report_url", "summary", "top_gaps"],
}


def spec_prompt(name):
    scope, crates = SCOPE[name]
    att = ATTACH[name]
    lines = [
        f"You are reverse-engineering ONE subsystem of Garry's Mod for an independent Rust rewrite that must match the game 1:1 in look, feel and physics.",
        f"Subsystem `{name}`: {scope}.",
        "",
        "Inputs:",
        f"- Pseudo-C dumps (tar.gz; INDEX.txt lists every file; one .c file per function, under `<subsystem>/<binary>/`: `server/` from the unstripped Linux server_srv.so with real C++ symbol names, `vphysics/` from vphysics_srv.so and `luashared/` from lua_shared_srv.so (both with symbols, where present), `client/` from the Windows client.dll and `engine/` from engine.dll, both without symbols - those were selected by the strings each function references, by RTTI vtables of the relevant C_ classes (virtual methods are listed in vtable order, so match them to the server class of the same name) and by direct callers/callees; the first line of each client/engine file says why it was selected):",
    ]
    for label, url in sorted(att.items()):
        lines.append(f'  ATTACHMENT:"{url}"  ({label})')
    lines += [
        f"- The Rust rewrite: clone {REPO_URL} (public, read-only, branch main). Compare against: {crates}. Read docs/status.md and docs/spec/*.md first to see what is already measured.",
        "",
        RULES,
        "Deliverables (two Markdown files, uploaded with the upload_attachment tool; report their URLs in the structured output):",
        f"1. `{name}.md` - the behaviour note: everything the game does in this subsystem, organised by feature, with exact numbers (units!), formulas, timings, state machines, sound/material/effect/convar names and defaults, what differs from Source SDK 2013, and a source+confidence tag per fact. Write it so an engineer can implement the feature without ever seeing the binary.",
        f"2. `{name}-gaps.md` - the gap list: for each behaviour, what the Rust crates do today (with file paths and line numbers from your clone), whether it matches, and if not what exactly is missing or wrong; rank the gaps by how visible they are to a player in game (P0 obvious, P1 noticeable, P2 subtle). Include a short section of things we do that the game does not.",
        "",
        "Method: start from the strings/symbols index to find the relevant functions, read them completely (follow callees inside the dump), cross-check constants against the SDK and Lua where available, then read the Rust side. Spend most of your time on the pseudo-C - it is the only source of truth for what GMod really does. Do not stop at the SDK. Do not write code changes.",
        "When done, delete the extracted pseudo-C from your VM and fill the structured output (summary, top_gaps ranked, counts, blockers).",
    ]
    return "\n".join(lines)


async def spec(name):
    try:
        r = await agent(spec_prompt(name), phase="spec", schema=SPEC_SCHEMA, label=f"spec-{name}", soft_time_limit_minutes=60)
        log(f"spec {name}: {r['facts_count']} facts, {r['gaps_count']} gaps")
        return r
    except WorkflowAgentError as e:
        log(f"spec {name} FAILED: {e}")
        return {"subsystem": name, "failed": True, "error": str(e)}


async def main():
    names = [n for n in SUBSYSTEMS if n in ATTACH]
    await register_workflow({
        "name": "gmod-re-gap",
        "description": "Per-subsystem behaviour specs from the real GMod binaries and the gap against the Rust crates",
        "product": "gmod-rust parity",
        "soft_time_limit_minutes": 60,
        "phases": [
            {"title": "spec", "detail": "one agent per subsystem: pseudo-C -> behaviour note + gap list", "labels": [f"spec-{n}" for n in names]},
            {"title": "combine", "detail": "rank every gap into docs/gap-report.md", "count": 1},
        ],
    })
    log(f"fan-out over {len(names)} subsystems")
    results = await parallel([(lambda n=n: spec(n)) for n in names])
    ok = [r for r in results if not r.get("failed")]
    failed = [r["subsystem"] for r in results if r.get("failed")]
    payload = json.dumps(ok, sort_keys=True, indent=1)
    prompt = "\n".join([
        "You combine the results of 13 reverse-engineering agents into one gap report for an independent Rust rewrite of Garry's Mod (https://github.com/rep0rtDev/gmod-rust, public, read-only).",
        "Each agent produced a behaviour note and a gap list (Markdown) for one subsystem; download them with download_attachment from the URLs below and read all of them completely.",
        payload,
        f"Subsystems whose agent failed (report them as missing, do not invent content): {json.dumps(sorted(failed))}",
        "",
        RULES,
        "Deliverable: `gap-report.md` (upload it with upload_attachment and return the URL): 1) a one-page executive summary of how far the rewrite is from the game; 2) a single ranked table of ALL gaps across subsystems (P0 obvious in game first; columns: rank, subsystem, gap, what the game does, what we do, Rust file refs, source/confidence); 3) per-subsystem sections that link to the note and gap files by name; 4) a suggested implementation order with dependencies between gaps (e.g. duck curve before viewmodel bob). Keep every number and formula the agents reported; do not add facts of your own without marking them inference. Also list contradictions between agents' notes if any.",
    ])
    report = await agent(prompt, phase="combine", schema=COMBINE_SCHEMA, label="combine", soft_time_limit_minutes=45)
    log("report: " + report["report_url"])
    log(json.dumps({"ok": [r["subsystem"] for r in ok], "failed": failed, "results": results, "report": report}, sort_keys=True))


asyncio.run(main())
