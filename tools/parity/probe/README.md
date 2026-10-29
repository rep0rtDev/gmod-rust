# Parity probe

A tiny GLua addon that runs inside a real Garry's Mod dedicated server (or client) and writes traces for the parity harnesses. It is the only Lua in this repository that targets the original game; it is never shipped with gmod-rust.

Install: copy `probe/lua` into `<srcds>/garrysmod/addons/parity_probe/lua` (or point the runner at the server directory and let it do that). Scenarios are placed by the runner in `<srcds>/garrysmod/data/parity/scenarios/*.json`; traces come back in `<srcds>/garrysmod/data/parity/out/*.json`.

The probe runs every scenario it finds once the map has loaded (`InitPostEntity`), then shuts the server down when `parity_quit` is set in the scenario list file.

## Running the dedicated server for harnesses

Findings from the first runs (Linux srcds, build 260917):

- An empty GMod server hibernates: no ticks, no timers, so the probe never fires. Start with `+sv_hibernate_think 1` (or set it in `server.cfg`). Console input also advances one frame, which is why typing into the console appears to "wake" it.
- Run `srcds_run` without `-console` piped to a non-tty, or give it a pty; otherwise the console reader blocks. `-condebug` writes `garrysmod/console.log`.
- The dedicated server's `garrysmod_dir.vpk` is not the client's: the client build has files the server lacks (for example `models/props_wasteland/cargo_container01d.mdl`). Server-realm harnesses must run the gmod-rust side against the server directory (`GMOD_DIR=<srcds>`), client-realm ones against the client install.
- Addon folders are read through symlinks on Linux, but copy the probe anyway so the server sees a stable tree.

Full command used:

```
./srcds_run -game garrysmod -norestart -noworkshop -nohltv -condebug -port 27025 +sv_lan 1 +sv_hibernate_think 1 +map gm_construct +maxplayers 2
```
