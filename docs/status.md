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
| Parity harness tooling | 0.5 | | probe addon, `gm-parity stage / run-ours / compare`; server launch is manual |
| Research lab | 0.25 | | `prepare.py`, `symbols.py` (unstripped srcds: 64 973 symbols in server, 7 294 in vphysics) |
| Render: world | 0.25 | 0 | `gm-render`: headless wgpu (lavapipe works), BSP faces and displacements with lightmap atlas and base textures, 3D skybox pass, patched material resolution, static props baked with skins and alpha test, `render_map` example writes a PNG. No bump/detail/blend, no fog, no 2D sky, no vertex lighting from vrad, no transparency sorting yet |
| Collision | 0.25 | 0 | `gm-collision`: ray and box sweeps through the brush tree with DIST_EPSILON, point contents; all 33 gm_construct spawns land on their floor. No displacements, no models (PHY), no entity brushes yet |
| Everything else | 0 | 0 | see docs/design.md section 8 |
