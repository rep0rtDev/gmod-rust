# Server tick (`gm-server`)

One fixed step of 0.015 s composes the measured pieces: every player's usercmd goes through
`gm-move` against the BSP/displacement collision world, sandbox commands (`gm_spawn`, tool gun
clicks, `gmod_undo`, `gmod_cleanup`, `sbox_*`) go through `gm-sandbox`, and the props and
constraints the sandbox owns are mirrored into the Jolt world of `gm-physics`:

- a new sandbox prop becomes a dynamic body at its placement pose (PHY hulls, mass, surface,
  damping, drag areas and inertia of the model), frozen props are kinematic;
- a prop the tool gun moved (weld right click, axis) is teleported (`PhysObj:SetPos/SetAngles`,
  velocity cleared) and its frozen flag follows `EnableMotion`;
- weld -> Jolt fixed constraint, rope -> distance constraint between the recorded local points
  (`length + addlength`, rigid as min = max), axis -> hinge through `LPos1` along the axis towards
  `LocalAxis`, nocollide -> no body constraint (a collision filter later); the world is a static
  body, so constraints to the world attach to it;
- undo and cleanup remove constraints first, then bodies;
- after the physics step every body's pose flows back into the sandbox, so the next trace or
  spawn sees the props where physics left them.

Nothing is networked yet; the harnesses drive the loop directly.

## Measured

The sandbox and tool gun recordings (`sandbox_spawn_basic`, `toolgun_weld_rope`, `toolgun_axis`)
replayed through the whole loop, 3 ticks of physics after every op (20 on the explicit waits)
with no poses taken from the recording: `server_spawn_basic` 16 ops within 0.55 u / 1.2 degrees
(a crate spawned 20 u above the floor falls and lands like the game's), `server_weld_rope` 20 ops
within 0.12 u / 0.25 degrees, `server_axis` 8 ops within 0.02 u / 0.06 degrees (frozen crates).

## Not yet

Networking and client prediction, multiple players, physgun through the loop, nocollide as a
Jolt collision filter, spawn menu, the remaining stools.
