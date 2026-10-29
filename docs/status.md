# Status

Scores: 0 absent, 0.25 started, 0.5 playable, 0.75 harnesses green within tolerance, 1.0 harnesses green at target tolerance. Golden traces from the real game live in `tools/parity/golden/<version>/`.

| Area | Score | Harnesses green | Notes |
|---|---|---|---|
| Formats: VPK | 0.75 | 2 / 2 | `fmt_vpk_index_garrysmod`, `fmt_vpk_index_hl2_models` equal to `file.Find` on a real server (version 2026.09.17) |
| Formats: KeyValues | 0.5 | | gameinfo, VMT, appmanifest parse; no dedicated harness yet |
| Formats: BSP | 0.25 | 0 | header and lump directory only |
| Formats: VTF | 0.25 | 0 | header only, checked on 200 real textures in integration tests |
| Formats: MDL/VVD/VTX/PHY | 0 | 0 | |
| Content mounting | 0.75 | 2 / 2 | GMod order without addons/workshop; client vs server difference documented |
| Measured facts | | | `fmt_convars_defaults`, `fmt_info` recorded: see `docs/spec/convars-defaults.md` |
| Parity harness tooling | 0.5 | | probe addon, `gm-parity stage / run-ours / compare`; server launch is manual |
| Research lab | 0.25 | | `prepare.py`, `symbols.py` (unstripped srcds: 64 973 symbols in server, 7 294 in vphysics) |
| Everything else | 0 | 0 | see docs/design.md section 8 |
