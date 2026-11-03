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

## HDR (measured on the mounted gm_construct.bsp, VBSP 20)

- HDR lumps are present and the same size as LDR: lump 53 LIGHTING_HDR 12274916 bytes (v1)
  = lump 8, lump 54 WORLDLIGHTS_HDR 12144 = lump 15, lump 55 LEAF_AMBIENT_LIGHTING_HDR 670516
  (v1) vs lump 56 LDR 665840. The game runs the map in HDR (`mat_hdr_level 2`), so its light
  tone comes from these lumps plus tonemapping, not from lumps 8/15/56.
- `light_environment` `sun_light`: `_light 242 234 218 560`, `_lightHDR -1 -1 -1 300`
  (-1 = keep the LDR colour, HDR brightness 300), `_lightscaleHDR 0.8`, `_ambient 114 136 148
  420`, `_ambientHDR -1 -1 -1 900`, `_AmbientScaleHDR 1.1`, pitch -56, angles 0 306 0,
  SunSpreadAngle 6. `env_sun` at -2845 6842 12256, size 16, `sprites/light_glow02_add_noz`,
  colour 241 240 199, HDRColorScale 1.0. `env_fog_controller`: fogcolor 142 164 178, start
  1024, end 250000, maxdensity 1, enabled.
  Fog law (`gm-render`): the world pass mixes the fog colour (display space, `fogcolor` / 255)
  into every fragment by `min(saturate((depth - fogstart) / (fogend - fogstart)), fogmaxdensity)`
  with depth the view-space distance along the camera forward; the 3D skybox pass uses the
  `sky_camera`'s own fog keys (gm_construct: start -4000, end 320000, same colour) in skybox
  units; the 2D sky is not fogged.
  Translucency (`gm-render`): `$alphatest` surfaces are cut out at alpha 0.5 in the opaque pass
  (no sorting); `$translucent` surfaces are drawn after all opaque batches of their pass with
  alpha blending and no depth write, batches ordered farthest first by the distance from the eye
  to the batch's vertex centroid, output alpha = base alpha (gm_construct: the reflective glass
  and the skybox foliage/antenna sheets). Water is still an opaque placeholder.
  Culling (`gm-render`): BSP faces wind clockwise seen from their front, and the world, 3D
  skybox and translucent passes cull back faces (every water tile on gm_construct is two
  coplanar faces at z -160 - `water_13` facing up and `water_13_beneath` facing down - so
  without culling the last-drawn one won from both sides); `$nocull 1` materials (foliage
  sheets, fences) are drawn two-sided; the 2D sky cube is seen from inside and never culled.
  Detail textures (`$detail`, `$detailscale`, `$detailblendfactor`, `$detailblendmode`): the
  detail texture is sampled at base uv x scale and, in mode 0 (`mod2x`, the only mode on
  gm_construct), the base becomes `base * lerp(1, 2 * detail, factor)` before lighting.
  gm_construct: `detail/noise_detail_01` on concrete / brick (scale 4-18, factor 0.6-0.8),
  `detail/plaster_detail_01` on plaster and the building templates (6.8-8.3, 0.3-1.2),
  `gm_construct/grass_clouds` on the grass (0.005-0.12, 0.5), `maxofs2d/terrain_detail` on
  flatgrass_2. Other blend modes are drawn without detail.
  `WorldVertexTransition` (`gm_construct/grass_13`: grass1 / grass2; `grass-sand_13`: grass1 /
  construct_sand with `$basetexturetransform2 "center .5 .5 scale 1.2 1.2 rotate 30"` and
  `$blendmodulatetexture gm_construct/grass_blendmask`): the base is
  `lerp($basetexture, $basetexture2, a)` with `a` the displacement vertex alpha / 255 (0 shows
  the first texture, 255 the second), interpolated across the triangle. Open: the second
  texture's own transform and the blend-modulate mask (the game sharpens the transition by it).
  Water (`gm_construct/water_13.vmt`, shader `Water`): `$abovewater 1`, `$refracttexture
  _rt_WaterRefraction`, `$reflecttexture _rt_WaterReflection` (`$reflectamount 0.4`,
  `$reflect3dskybox 1`, `$reflectentities 1`), `$bumpmap gm_construct/water_13_dudv`,
  `$normalmap gm_construct/water_13_normal`, scroll `[.01 .01 .05]` / `[-.025 .025 .075]`,
  `$fogenable 1`, `$fogcolor {7 58 66}`, `$fogstart 0`, `$fogend 1024`; `$bottommaterial
  gm_construct/water_13_beneath` (`$abovewater 0`, `$fogcolor {24 64 72}`, `$fogstart -1024`,
  `$fogend 2048`). Done: the surface is drawn flat in its `$fogcolor` (the colour the volume fog
  converges to). Open: refraction/reflection render targets with the dudv/normal distortion,
  the depth-based volume fog through the surface, and the view fog switching to the
  `$bottommaterial` keys when the eye is in a water leaf. On gm_construct the world fog is 0.4 % at 2000 u and 3.6 %
  at 10 000 u - faint, as in the game.
- `env_tonemap_controller` "TonemapController" set by `logic_auto` OnMapSpawn:
  SetAutoExposureMax 1.8, SetAutoExposureMin 0.7, SetBloomScale 0.3, SetTonemapRate 0.2.
  Engine defaults otherwise: `mat_autoexposure_min 0.5`, `mat_autoexposure_max 2`,
  `mat_bloomscale 1`, `mat_hdr_tonemapscale 1` (the exposure the auto path converges to),
  bloom threshold via `mat_bloom_scalefactor_scalar`; the tone map is linear: colour x
  tonemap scale, clamped (Source has no filmic curve).
- Ours: `WorldRenderer::new(.., hdr)` with `hdr = mat_hdr_level >= 1 && Bsp::has_hdr()` takes
  lumps 53 / 54 / 55 (`Bsp::face_lightmap(face, hdr)`, `world_lights(bsp, hdr)`,
  `LeafAmbient::parse(bsp, hdr)`); applied on map load. The lightmap atlas is Rgba16Float
  holding linear light as decoded (HDR values pass through; `Camera.lm_scale` stays 1). The
  tonemap scale (`Camera.exposure`) multiplies the shaded colour before the monitor gamma and
  is driven by auto-exposure: every 15th frame the client reads the presented frame back
  (`Gpu::read_texture`, 16 px grid) and takes the brightness the brightest 2 % of pixels reach;
  the goal is `exposure * 0.6 / bright` (the engine's `mat_tonemap_percent_bright_pixels 2` /
  `mat_tonemap_percent_target 60`), clamped into the controller's [AutoExposureMin, Max] and
  approached at `SetTonemapRate` per second, three times faster downwards
  (`mat_accelerate_adjust_exposure_down 3`). Measured on gm_construct: on the sunlit floor the
  exposure settles at the 0.7 floor in ~2 s, turning down to a shaded patch it climbs towards 1.8.
  Bloom (`gm_render::post::Post`, on when the map is HDR and `mat_bloomscale` is set): the
  scene draws into an offscreen target, a bright pass at quarter resolution keeps the part of
  each pixel above a linear luminance of 0.45, a 9-tap separable Gaussian blurs it and the
  composite adds it times the controller's `SetBloomScale` (0.3 on gm_construct). The
  threshold / kernel are ours - the engine's downsample and blur shaders were not consulted;
  the strength follows the map's controller like the exposure bounds do.

## Texture mips and filtering

- World, model and sprite textures upload the whole VTF mip chain below the chosen top
  (`material::load_texture_lod`, `upload_rgba_mips`); before this only the top mip existed,
  so distant surfaces sparkled and the Advanced Video texture settings had nothing to act on.
- `gm_render::world::Quality` carries the Advanced Video values into the renderer:
  `mat_picmip` skips that many top mips on world textures (Texture Detail Low 2 / Medium 1 /
  High 0 / Very High -1 = none), `mat_trilinear` picks a linear mip filter (nearest otherwise),
  `mat_forceaniso` 1..16 sets the base sampler's anisotropy (which forces trilinear, as wgpu
  requires). Applied on map load like the HDR choice.
- `mat_antialias` is MSAA: `Quality.msaa` (1 / 2 / 4 / 8, `pick_msaa` steps down to a sample
  count the adapter supports for the colour and Depth32Float formats). The world, model,
  additive and world-UI pipelines are built with that sample count; the scene passes draw
  into a multisampled colour target (`Gfx::msaa_color`, `offscreen_targets(.., msaa)`) with
  a multisampled depth and resolve into the frame (or the bloom scene texture) at the end of
  each pass (`resolve` on `draw` / `ModelTarget` / `draw_world`); spawn-menu icons draw into
  64 px MSAA targets and resolve into the icon texture. Screen-space UI stays single-sample.
- `r_rootlod` is Model Detail: `Quality.root_lod` (High 0 / Medium 1 / Low 2) is the lowest
  VTX LOD drawn for static props and dynamic models (`props::load_model` takes it, clamps it
  to the LODs the model has and to the MDL's own root LOD, parses the VVD fixups for that
  level and every mesh builder uses that LOD). Distance-based LOD switching
  (`switch_point`) is not done: like the engine with `r_lod` forced, one LOD per model.
