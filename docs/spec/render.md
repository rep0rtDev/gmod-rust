# Client rendering

How the native client draws the world and models, with what was measured on the game's content. Harness group: none yet (visual); numbers below are read from gm_construct's BSP and the mounted models.

## Lightmaps

World faces use `LUMP_LIGHTING` (8) luxels as `c * 2^exp / 255` (linear, 1.0 = white); gm_construct's exponents peak at 0 and -1, mean luxel 0.24. The atlas stores `linear / 2` in 8 bits and the shader doubles it, so the Source overbright range up to 2.0 survives. HDR lumps (53 lightmaps, 54 lights, 55 ambient, 51 index) exist on gm_construct; the game renders it in HDR with tone mapping by default (`mat_hdr_level 2`), which we do not do yet (open: HDR lightmaps + auto exposure + bloom).

## Model lighting (measured scale, `gm_render::light`)

The game lights a studio model from its origin: the leaf's ambient cube (six colours, weighted by the squared normal components) plus the direct world lights reaching it, up to four.

- World lights: `LUMP_WORLDLIGHTS` (15; HDR 54), 88-byte records (version 1: 100). gm_construct: 108 point, 28 spot, 1 skylight (`light_environment`, intensity 1.96 / 1.82 / 1.56 LDR, direction (0.329, -0.452, -0.829)), 1 sky ambient. Point / spot falloff `intensity / (c + l d + q d^2)`, spot cone between `stopdot2` and `stopdot` with the `exponent` ramp; the skylight only when a ray towards the sun leaves through a sky brush; other lights only when the line to them is clear.
- Ambient cubes: `LUMP_LEAF_AMBIENT_INDEX` 52 (u16 count, u16 first per leaf) into `LUMP_LEAF_AMBIENT_LIGHTING` 56 (28 bytes: six `ColorRGBExp32` + x, y, z as fractions of the leaf box, pad). gm_construct: 5002 leaves, 23780 samples. Their exponents peak at -9 / -10 against the lightmaps' 0: the cubes are stored in linear light `c * 2^exp` without the `/ 255` (outdoor top face ≈ 0.26, sides 0.12). We take the nearest sample in the leaf; the game blends by distance.
- Static props: vrad bakes per-vertex colours into `sp_<n>.vhv` in the pakfile (364 on gm_construct): 40-byte header (version 2, checksum, vertex flags, vertex size 4, vertex count, mesh count, 4 unused), 28-byte mesh records (lod, vertex count, offset, 4 unused), RGBA8 per vertex in the mesh's vertex order (mesh-local index). Read as `c / 255` linear: sunlit skybox buildings average 0.24, the lightmap mean. Props without a file are lit as dynamic models from their origin.
- Shading: Lambert for the direct lights, half-Lambert (`(n.l * 0.5 + 0.5)^2`) when the material has `$halflambert 1` (the HL2 characters do, props do not).
- Dynamic models re-light per draw from their origin, cached per 8 u cell.

Open: HDR path; the game's `r_lightaverage`/`r_ambientboost`; `$phong`, `$rimlight`, `$envmap` on models; lightstyles; the 3D skybox models take the sun like world models (the game bakes theirs too, from the skybox's own lights).

## Animation

Sequences are sampled between the two neighbouring frames (position lerp, quaternion slerp), as the game's bone setup does; a nearest frame visibly steps on the 5-15 fps HL2 weapon animations (the measured viewmodel idles are 5-10 fps).
