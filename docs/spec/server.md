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

The physgun runs in the loop too (`Player::held`, `PlayerCmd::physgun`): the weapon fires with the
eye before the player's move of the tick (grab by the tool trace against the prop hulls, hold with
the shadow controller, release on attack up, freeze on attack2 also flags the sandbox prop), then
physics steps, then players move. The seven `physgun_*` recordings replayed as `server_physgun_*`
give the same numbers as the standalone physgun harness: 5 of 7 within tolerance (freeze 0.50 u,
off-centre 0.49 u, pitch steps 0.50 u, rotate 1.95 u / 4.45 degrees, yaw steps 0.10 u), walk
hold 3.0 u on the acceleration ticks and hold_crate 8.5 u only after the release landing (the open
contact response of `phys_*`).

## Not yet

Networking and client prediction, multiple players in one recording, nocollide as a Jolt
collision filter, spawn menu, the remaining stools.
