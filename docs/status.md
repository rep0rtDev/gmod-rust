# Status

Scores: 0 absent, 0.25 started, 0.5 playable, 0.75 harnesses green within tolerance, 1.0 harnesses green at target tolerance. Golden traces from the real game live in `tools/parity/golden/<version>/`.

| Area | Score | Harnesses green | Notes |
|---|---|---|---|
| Formats: VPK | 0.75 | 2 / 2 | `fmt_vpk_index_garrysmod`, `fmt_vpk_index_hl2_models` equal to `file.Find` on a real server (version 2026.09.17) |
| Formats: KeyValues | 0.5 | | gameinfo, VMT, appmanifest parse; no dedicated harness yet |
| Formats: BSP | 0.5 | 0 | header, planes, vertices, edges, faces, texinfo/texdata, models, nodes, leafs, brushes, lightmaps, entities; gm_construct parses end to end; static props (sprp v4 to v11), displacements built to meshes, PVS decode and point to leaf. PHY and leaf ambient lighting pending |
| Formats: VTF | 0.5 | 0 | header, resources, mip addressing, decode to RGBA8 for DXT1/3/5, BGR(A/X)8888, BGR888, I8/A8/IA88, 565, UV88, RGBA16F; 300 real HL2 textures decode |
| Formats: MDL/VVD/VTX | 0.5 | 0 | header v44 to v49, bones, body parts, meshes, textures, skins, VVD fixups, VTX strips to triangle lists; 60 props_c17 models verified. PHY pending |
| Content mounting | 0.75 | 2 / 2 | GMod order without addons/workshop; client vs server difference documented |
| Measured facts | | | `fmt_convars_defaults`, `fmt_info` recorded: see `docs/spec/convars-defaults.md` |
| Parity harness tooling | 0.5 | | probe addon with sync and async scenarios (bot driven movement), `gm-parity stage / run-ours / compare` with set-equal and trajectory modes; server launch is manual |
| Research lab | 0.25 | | `prepare.py`, `symbols.py` (unstripped srcds: 64 973 symbols in server, 7 294 in vphysics) |
| Render: world | 0.25 | 0 | `gm-render`: headless wgpu (lavapipe works), BSP faces and displacements with lightmap atlas and base textures, 3D skybox pass, patched material resolution, static props baked with skins and alpha test, `render_map` example writes a PNG. No bump/detail/blend, no fog, no 2D sky, no vertex lighting from vrad, no transparency sorting yet |
| Collision | 0.25 | 0 | `gm-collision`: ray and box sweeps through the brush tree with DIST_EPSILON, point contents; all 33 gm_construct spawns land on their floor. No displacements, no models (PHY), no entity brushes yet |
| Movement | 0.8 | 18 / 18 | `gm-move`: walk, run, friction, air control, jump with forward boost, step, duck/unduck timing, noclip. `move_walk_flat`, `move_run_stop`, `move_jump_single`, `move_jump_forward`, `move_sprint_jump`, `move_strafe_jump`, `move_duck_walk`, `move_fall_200`, `move_crouch_jump`, `move_stairs_up`, `move_stairs_down`, `move_airstrafe`, `move_air_left`, `move_air_right`, `move_air_alt`, `move_walk_disp` (displacement ground and a 4 u step) bit-exact against a real server bot (max position error 0.0000). No ladders, water, moving ground, fall damage, slopes |
| Physics | 0.45 | 1 / 5 | Jolt BSP brush world, PHY convex props and inertia, measured drag, 1 cm rest gap, speculative contact shim, IVP style impact response one hull vertex at a time (material products, tenth pushes, friction tilt); flat crate exact incl. its two corner impacts, barrel first impact within 10 percent, tilted crate first impact right, the tumbles after differ |
| Physgun | 0.55 | 5 / 7 | shadow controller fitted from traces (damping 0.8, timeToArrive 0.05 exact), grab point on the ray, body space offset, yaw follow, rotate mode, freeze, moving player; rotate axis and acceleration lag just outside tolerance |
| Constraints | 0.3 | 1 / 4 | weld and rope on Jolt through the shim, probe records several props; crate dropped on a resting crate within 1 u, the same on displacement ground 2.4 u (just outside), rope swing within 4.6 u, welded pair landing 7 u open |
| Sandbox core | 0.2 | 1 / 1 | `gm-sandbox`: `gm_spawn` placement (hit point, flush offset from the model box, axis nudges), undo stack semantics, cleanup, `sbox_maxprops`; `sandbox_spawn_basic` equal to the real gamemode on 16 ops |
| Everything else | 0 | 0 | see docs/design.md section 8 |
