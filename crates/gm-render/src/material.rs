//! Resolving a BSP material name to a VMT and its base texture.

use anyhow::{anyhow, Result};
use gm_content::Mount;
use gm_formats::keyvalues::{self, Node, Value};
use gm_formats::vtf;

#[derive(Clone, Debug, Default)]
pub struct Material {
    pub name: String,
    pub shader: String,
    pub base_texture: Option<String>,
    pub base_texture2: Option<String>,
    /// `eyes` / `EyeRefract`: the iris texture the engine projects on the eyeball.
    pub iris: Option<String>,
    pub translucent: bool,
    /// `$alphatest 1`: the base alpha cuts the surface out (drawn opaque, no sorting).
    pub alpha_test: bool,
    /// `$nocull 1`: drawn two-sided (foliage sheets, fences).
    pub nocull: bool,
    /// `$additive 1`: drawn as src + dst (the UI approximates it by alpha = max(r, g, b)).
    pub additive: bool,
    /// `$halflambert`: models lit with the half-Lambert wrap (characters), else plain Lambert.
    pub half_lambert: bool,
    pub no_draw: bool,
    pub water: bool,
    /// `Water`: `$fogcolor` (display-space 0..1), `$fogstart`, `$fogend` of the volume fog
    /// seen through the surface (`$abovewater 1`) or from inside (`$bottommaterial`).
    pub water_fog: Option<([f32; 3], f32, f32)>,
    /// `Water`: `$bottommaterial`, the material of the same volume seen from inside.
    pub bottom_material: Option<String>,
    /// `Water` with `$abovewater 0`: the volume's underside, drawn only while the eye is below
    /// the surface (seen from above it would paint the refraction copy over the real surface).
    pub below_water: bool,
    /// `$detail`: a tiling texture multiplied into the base.
    pub detail: Option<Detail>,
    /// `$basetexturetransform2` as a 2x3 uv matrix (row-major 2x2 then translation).
    pub base2_transform: Option<[f32; 6]>,
    /// `$blendmodulatetexture`: its r/g sharpen the `WorldVertexTransition` blend.
    pub blend_modulate: Option<String>,
    /// `$bumpmap`: tangent-space normal map lit by the bumped lightmap pages.
    pub bumpmap: Option<String>,
    /// `$ssbump`: the bump map stores the three basis-light weights directly.
    pub ssbump: bool,
    /// Water: `$reflectamount`, `$refractamount` (screen-space offset per unit of normal xy)
    /// and the `$scroll1` xy rate of the normal map, in uv per second.
    pub water_dist: [f32; 4],
    /// Water `$reflectentities`: entities (models) are drawn into the reflection too.
    pub reflect_entities: bool,
    /// A `Subrect` decal: the atlas material, `$Pos` and `$Size` in its texels.
    pub subrect: Option<(String, [f32; 2], [f32; 2])>,
    /// `$decalscale`: world units per texel of a decal.
    pub decal_scale: Option<f32>,
    /// `$envmap env_cubemap`: the nearest baked cubemap reflected, scaled by `$envmaptint`
    /// and masked by the base alpha (`$basealphaenvmapmask`), the normal map alpha
    /// (`$normalmapalphaenvmapmask`) or nothing.
    pub envmap: Option<EnvMap>,
    /// `$envmapmask` texture path.
    pub envmapmask: Option<String>,
    /// `$envmapmaskscale`: uv scale of the mask texture (1 when absent).
    pub envmapmask_scale: f32,
    /// `$selfillum`: the base alpha lerps the lit result towards the unlit base.
    pub selfillum: bool,
    /// `$selfillumtint` (1 1 1 when absent).
    pub selfillum_tint: [f32; 3],
    /// `Proxies { PlayerWeaponColor { resultVar $selfillumtint } }`: the tint is the player's
    /// weapon colour (`cl_weaponcolor`) - the physgun's glowing parts.
    pub weapon_color_tint: bool,
    /// `Proxies { PlayerColor { resultVar $color2 } }`: the model is tinted with the
    /// player's colour (`cl_playercolor`), only where `$blendtintbybasealpha` lets it.
    pub player_color_tint: bool,
    pub blend_tint_by_base_alpha: bool,
    /// `$phong`: the specular highlight of the sun (`VertexLitGeneric` models).
    pub phong: Option<Phong>,
}

/// `$phong` parameters.
#[derive(Clone, Debug, PartialEq)]
pub struct Phong {
    /// `$phongexponent` (5 when absent).
    pub exponent: f32,
    /// `$phongboost` (1).
    pub boost: f32,
    /// `$phongalbedotint` (0): how much the highlight takes the base colour.
    pub albedo_tint: f32,
    /// `$phongfresnelranges` [0 0.5 1]: the highlight at (1 - n.v)^5 of 0, 0.5 and 1.
    pub fresnel: [f32; 3],
    /// `$phongexponenttexture`: red = the exponent over 1..150 (replaces `$phongexponent`),
    /// green = the `$phongalbedotint` mask.
    pub exponent_texture: Option<String>,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub enum EnvMapMask {
    None,
    BaseAlpha,
    NormalAlpha,
    /// `$envmapmask` texture (rgb).
    Texture,
}

#[derive(Clone, Copy, Debug)]
pub struct EnvMap {
    pub tint: [f32; 3],
    pub mask: EnvMapMask,
    /// `$envmapcontrast` (0 = linear, 1 = squared) and `$envmapsaturation` (1 = full colour).
    pub contrast: f32,
    pub saturation: f32,
    /// `$envmapfresnel`: `$envmapfresnelminmaxexp` [min max exp] scales the reflection by
    /// min + (max - min)·(1 - n·v)^exp.
    pub fresnel: Option<[f32; 3]>,
}

/// `center cx cy scale sx sy rotate deg translate tx ty` -> uv matrix
/// `T(translate) T(center) R(rotate) S(scale) T(-center)` (Source `TextureTransform`).
pub fn texture_transform(spec: &str) -> Option<[f32; 6]> {
    let w: Vec<&str> = spec.split_whitespace().collect();
    let mut cx = 0.5f32;
    let mut cy = 0.5f32;
    let mut sx = 1.0f32;
    let mut sy = 1.0f32;
    let mut rot = 0.0f32;
    let mut tx = 0.0f32;
    let mut ty = 0.0f32;
    let mut i = 0;
    let num = |k: &str| k.parse::<f32>().ok();
    while i < w.len() {
        match w[i] {
            "center" if i + 2 < w.len() => {
                cx = num(w[i + 1])?;
                cy = num(w[i + 2])?;
                i += 3;
            }
            "scale" if i + 2 < w.len() => {
                sx = num(w[i + 1])?;
                sy = num(w[i + 2])?;
                i += 3;
            }
            "rotate" if i + 1 < w.len() => {
                rot = num(w[i + 1])?;
                i += 2;
            }
            "translate" if i + 2 < w.len() => {
                tx = num(w[i + 1])?;
                ty = num(w[i + 2])?;
                i += 3;
            }
            _ => return None,
        }
    }
    let (sn, cs) = rot.to_radians().sin_cos();
    // M = R * S
    let m = [cs * sx, -sn * sy, sn * sx, cs * sy];
    // t = translate + center - M * center
    let ox = tx + cx - (m[0] * cx + m[1] * cy);
    let oy = ty + cy - (m[2] * cx + m[3] * cy);
    Some([m[0], m[1], m[2], m[3], ox, oy])
}

/// `$detail` / `$detailscale` / `$detailblendfactor` / `$detailblendmode`: the detail
/// texture sampled at `base uv * scale`, blended into the base by `factor`; mode 0 (the
/// default, `mod2x`) gives `base * lerp(1, 2 * detail, factor)`.
#[derive(Clone, Debug)]
pub struct Detail {
    pub texture: String,
    pub scale: f32,
    pub factor: f32,
    pub mode: u32,
}

/// Display-space (sRGB-encoded, 0..1) to linear light: colours given in display space -
/// `fogcolor` of `env_fog_controller` / `sky_camera`, `$fogcolor` of water - are written
/// into the linear-light output that the sRGB target encodes on store.
pub fn srgb_to_linear(c: f32) -> f32 {
    if c <= 0.04045 {
        c / 12.92
    } else {
        ((c + 0.055) / 1.055).powf(2.4)
    }
}

/// `maps/<map>/<mat>_<x>_<y>_<z>` is a per-cubemap patch written by vbsp;
/// the real material is `<mat>`.
pub fn canonical_name(name: &str) -> String {
    let lower = name.to_ascii_lowercase().replace('\\', "/");
    if let Some(rest) = lower.strip_prefix("maps/") {
        if let Some((_, mat)) = rest.split_once('/') {
            let mut parts = mat.rsplitn(4, '_');
            let a = parts.next();
            let b = parts.next();
            let c = parts.next();
            let base = parts.next();
            if let (Some(a), Some(b), Some(c), Some(base)) = (a, b, c, base) {
                if [a, b, c].iter().all(|s| s.parse::<i64>().is_ok()) {
                    return base.to_string();
                }
            }
            return mat.to_string();
        }
    }
    lower
}

pub fn load(mount: &Mount, name: &str) -> Result<Material> {
    let canon = canonical_name(name);
    load_path(mount, &canon, &format!("materials/{canon}.vmt"))
}

/// Load a VMT by full mounted path (`materials/.../x.vmt`), following `patch` includes.
pub fn load_path(mount: &Mount, name: &str, vmt_path: &str) -> Result<Material> {
    let mut mat = Material {
        name: name.to_string(),
        ..Default::default()
    };
    let mut path = vmt_path.to_string();
    for _ in 0..4 {
        let text = mount.read_string(&path)?;
        let doc = keyvalues::parse(&text)?;
        let Some(root) = doc.nodes.first() else {
            return Err(anyhow!("{path}: empty VMT"));
        };
        let shader = root.key.to_ascii_lowercase();
        let Value::Block(params) = &root.value else {
            return Err(anyhow!("{path}: shader is not a block"));
        };
        // DX-level conditional blocks (`">=dx90_20b" { "$baseTexture" ... }`): the engine
        // takes the block matching its shader level, ours is always the top one, so the
        // `>=dx90*` entries override the plain ones and the `<dx90` fallbacks are ignored
        // (combinesoldiersheet.vmt keeps its only $basetexture in such a block)
        let flat: Vec<&keyvalues::Node> = params
            .iter()
            .filter(|n| {
                let k = n.key.to_ascii_lowercase();
                k.starts_with(">=dx9") || k.starts_with(">=dx8") || k == "dx9" || k == "dx90"
            })
            .filter_map(|n| match &n.value {
                Value::Block(b) => Some(b.iter()),
                _ => None,
            })
            .flatten()
            .chain(params.iter())
            .collect();
        let get = |k: &str| -> Option<String> {
            flat.iter().find_map(|n| {
                if !n.key.eq_ignore_ascii_case(k) {
                    return None;
                }
                match &n.value {
                    Value::Str(s) => Some(s.replace('\\', "/").to_ascii_lowercase()),
                    Value::Block(_) => None,
                }
            })
        };
        if shader == "patch" {
            let include = get("include").ok_or_else(|| anyhow!("{path}: patch without include"))?;
            for n in params {
                if n.key.eq_ignore_ascii_case("insert") || n.key.eq_ignore_ascii_case("replace") {
                    if let Value::Block(b) = &n.value {
                        for p in b {
                            if let Value::Str(s) = &p.value {
                                let s = s.replace('\\', "/").to_ascii_lowercase();
                                if p.key.eq_ignore_ascii_case("$basetexture") {
                                    mat.base_texture = Some(s);
                                } else if p.key.eq_ignore_ascii_case("$basetexture2") {
                                    mat.base_texture2 = Some(s);
                                } else if p.key.eq_ignore_ascii_case("$iris") {
                                    mat.iris = Some(s);
                                }
                            }
                        }
                    }
                }
            }
            path = include;
            continue;
        }
        mat.water = shader == "water";
        mat.decal_scale = get("$decalscale").and_then(|v| v.trim().parse().ok());
        if shader == "subrect" {
            let pair = |k: &str| -> Option<[f32; 2]> {
                let v = get(k)?;
                let mut it = v.split_whitespace().map(|x| x.parse::<f32>().ok());
                Some([it.next()??, it.next()??])
            };
            if let (Some(m), Some(pos), Some(size)) =
                (get("$material"), pair("$pos"), pair("$size"))
            {
                mat.subrect = Some((m, pos, size));
            }
        }
        // Refract (sprites/heatwave) reads the same $normalmap / $refractamount
        if mat.water || shader == "refract" {
            mat.bottom_material = get("$bottommaterial");
            mat.below_water = mat.water && matches!(get("$abovewater").as_deref(), Some("0"));
            mat.bumpmap = get("$normalmap").or_else(|| get("$bumpmap"));
            mat.reflect_entities = matches!(get("$reflectentities").as_deref(), Some("1"));
            let amount = |k: &str, d: f32| get(k).and_then(|v| v.parse::<f32>().ok()).unwrap_or(d);
            let scroll: Vec<f32> = get("$scroll1")
                .map(|v| {
                    v.trim_matches(|c| c == '[' || c == ']')
                        .split_whitespace()
                        .filter_map(|x| x.parse::<f32>().ok())
                        .collect()
                })
                .unwrap_or_default();
            let (sx, sy) = if scroll.len() >= 2 {
                (scroll[0], scroll[1])
            } else {
                (0.0, 0.0)
            };
            mat.water_dist = [
                amount("$reflectamount", 0.0),
                amount("$refractamount", 0.0),
                sx,
                sy,
            ];
            // the TextureScroll proxy on $bumptransform scrolls the normal map by
            // texturescrollrate uv/s along texturescrollangle degrees
            if let Some((rate, angle)) = texture_scroll(params, "$bumptransform") {
                let a = angle.to_radians();
                mat.water_dist[2] = rate * a.cos();
                mat.water_dist[3] = rate * a.sin();
            }
            let c: Vec<f32> = get("$fogcolor")
                .map(|v| {
                    v.trim_matches(|c| c == '{' || c == '}' || c == '[' || c == ']')
                        .split_whitespace()
                        .filter_map(|x| x.parse::<f32>().ok())
                        .collect()
                })
                .unwrap_or_default();
            let num = |k: &str, d: f32| get(k).and_then(|v| v.parse::<f32>().ok()).unwrap_or(d);
            if c.len() >= 3 {
                mat.water_fog = Some((
                    [c[0] / 255.0, c[1] / 255.0, c[2] / 255.0],
                    num("$fogstart", 0.0),
                    num("$fogend", 1024.0),
                ));
            }
        }
        mat.shader = shader;
        if mat.base_texture.is_none() {
            mat.base_texture = get("$basetexture");
        }
        if mat.base_texture2.is_none() {
            mat.base_texture2 = get("$basetexture2");
        }
        if mat.iris.is_none() {
            mat.iris = get("$iris");
        }
        mat.half_lambert = matches!(get("$halflambert").as_deref(), Some("1"));
        mat.alpha_test = matches!(get("$alphatest").as_deref(), Some("1"));
        mat.translucent = matches!(get("$translucent").as_deref(), Some("1")) && !mat.alpha_test;
        mat.additive = matches!(get("$additive").as_deref(), Some("1"));
        mat.nocull = matches!(get("$nocull").as_deref(), Some("1"));
        mat.selfillum = matches!(get("$selfillum").as_deref(), Some("1"));
        mat.weapon_color_tint = proxy_result(params, "playerweaponcolor", "$selfillumtint");
        mat.player_color_tint = proxy_result(params, "playercolor", "$color2");
        mat.blend_tint_by_base_alpha = matches!(get("$blendtintbybasealpha").as_deref(), Some("1"));
        mat.ssbump = matches!(get("$ssbump").as_deref(), Some("1"));
        let tint = vec_of(get("$selfillumtint"));
        mat.selfillum_tint = if tint.len() >= 3 {
            [tint[0], tint[1], tint[2]]
        } else {
            [1.0; 3]
        };
        mat.base2_transform = get("$basetexturetransform2")
            .as_deref()
            .and_then(texture_transform);
        mat.blend_modulate = get("$blendmodulatetexture");
        if mat.bumpmap.is_none() {
            mat.bumpmap = get("$bumpmap");
        }
        if matches!(get("$phong").as_deref(), Some("1")) {
            let num = |k: &str, d: f32| get(k).and_then(|v| v.parse::<f32>().ok()).unwrap_or(d);
            let fr = vec_of(get("$phongfresnelranges"));
            mat.phong = Some(Phong {
                exponent: num("$phongexponent", 5.0),
                exponent_texture: get("$phongexponenttexture"),
                boost: num("$phongboost", 1.0),
                albedo_tint: num("$phongalbedotint", 0.0),
                fresnel: if fr.len() >= 3 {
                    [fr[0], fr[1], fr[2]]
                } else {
                    [0.0, 0.5, 1.0]
                },
            });
        }
        if get("$envmap").is_some() {
            let tint: Vec<f32> = get("$envmaptint")
                .map(|v| {
                    v.trim_matches(|c| c == '{' || c == '}' || c == '[' || c == ']')
                        .split_whitespace()
                        .filter_map(|x| x.parse::<f32>().ok())
                        .collect()
                })
                .unwrap_or_default();
            let tint = if tint.len() >= 3 {
                [tint[0], tint[1], tint[2]]
            } else {
                [1.0; 3]
            };
            let one = |k: &str| matches!(get(k).as_deref(), Some("1"));
            mat.envmapmask = get("$envmapmask");
            mat.envmapmask_scale = get("$envmapmaskscale")
                .and_then(|v| v.parse::<f32>().ok())
                .unwrap_or(1.0);
            let numv = |k: &str, d: f32| get(k).and_then(|v| v.parse::<f32>().ok()).unwrap_or(d);
            let mask = if mat.envmapmask.is_some() {
                EnvMapMask::Texture
            } else if one("$basealphaenvmapmask") {
                EnvMapMask::BaseAlpha
            } else if one("$normalmapalphaenvmapmask") {
                EnvMapMask::NormalAlpha
            } else {
                EnvMapMask::None
            };
            mat.envmap = Some(EnvMap {
                tint,
                mask,
                contrast: numv("$envmapcontrast", 0.0),
                saturation: numv("$envmapsaturation", 1.0),
                fresnel: one("$envmapfresnel").then(|| {
                    let v = vec_of(get("$envmapfresnelminmaxexp"));
                    if v.len() >= 3 {
                        [v[0], v[1], v[2]]
                    } else {
                        [0.0, 1.0, 2.0]
                    }
                }),
            });
        }
        if let Some(texture) = get("$detail") {
            let num = |k: &str, d: f32| get(k).and_then(|v| v.parse::<f32>().ok()).unwrap_or(d);
            mat.detail = Some(Detail {
                texture,
                scale: num("$detailscale", 4.0),
                factor: num("$detailblendfactor", 1.0),
                mode: num("$detailblendmode", 0.0) as u32,
            });
        }
        mat.no_draw = matches!(get("%compilenodraw").as_deref(), Some("1"))
            || matches!(get("%compilesky").as_deref(), Some("1"))
            || matches!(get("%compile2dsky").as_deref(), Some("1"))
            || matches!(get("%compiletrigger").as_deref(), Some("1"))
            || matches!(get("%compilehint").as_deref(), Some("1"))
            || matches!(get("%compileskip").as_deref(), Some("1"))
            || matches!(get("%compileclip").as_deref(), Some("1"))
            || matches!(get("%compileplayerclip").as_deref(), Some("1"))
            || matches!(get("%compilenpcclip").as_deref(), Some("1"))
            || matches!(get("%compileinvisible").as_deref(), Some("1"));
        return Ok(mat);
    }
    Err(anyhow!("{name}: patch chain too deep"))
}

pub struct Rgba {
    pub width: u32,
    pub height: u32,
    pub pixels: Vec<u8>,
    /// Lower mip levels, each half the previous (empty when the source carries none).
    pub mips: Vec<Vec<u8>>,
}

/// Decode a JPEG (the main menu's `backgrounds/*.jpg`) to RGBA8.
pub fn load_jpeg(bytes: &[u8]) -> Result<Rgba> {
    let mut decoder = jpeg_decoder::Decoder::new(std::io::Cursor::new(bytes));
    let px = decoder.decode()?;
    let info = decoder
        .info()
        .ok_or_else(|| anyhow::anyhow!("jpeg without a frame header"))?;
    let pixels = match info.pixel_format {
        jpeg_decoder::PixelFormat::RGB24 => px
            .as_chunks::<3>()
            .0
            .iter()
            .flat_map(|c| [c[0], c[1], c[2], 255])
            .collect(),
        jpeg_decoder::PixelFormat::L8 => px.iter().flat_map(|&l| [l, l, l, 255]).collect(),
        jpeg_decoder::PixelFormat::L16 => px
            .as_chunks::<2>()
            .0
            .iter()
            .flat_map(|c| [c[0], c[0], c[0], 255])
            .collect(),
        other => anyhow::bail!("jpeg pixel format {other:?} is not supported"),
    };
    Ok(Rgba {
        width: u32::from(info.width),
        height: u32::from(info.height),
        pixels,
        mips: Vec::new(),
    })
}
/// Decode a PNG (any colour type) to RGBA8.
pub fn load_png(bytes: &[u8]) -> Result<Rgba> {
    let mut decoder = png::Decoder::new(std::io::Cursor::new(bytes));
    decoder.set_transformations(png::Transformations::EXPAND | png::Transformations::STRIP_16);
    let mut reader = decoder.read_info()?;
    let mut buf = vec![0u8; reader.output_buffer_size().unwrap_or(0)];
    let info = reader.next_frame(&mut buf)?;
    let pixels = match info.color_type {
        png::ColorType::Rgba => buf[..info.buffer_size()].to_vec(),
        png::ColorType::Rgb => buf[..info.buffer_size()]
            .as_chunks::<3>()
            .0
            .iter()
            .flat_map(|p| [p[0], p[1], p[2], 255])
            .collect(),
        png::ColorType::GrayscaleAlpha => buf[..info.buffer_size()]
            .as_chunks::<2>()
            .0
            .iter()
            .flat_map(|p| [p[0], p[0], p[0], p[1]])
            .collect(),
        png::ColorType::Grayscale => buf[..info.buffer_size()]
            .iter()
            .flat_map(|&g| [g, g, g, 255])
            .collect(),
        png::ColorType::Indexed => anyhow::bail!("indexed PNG left after EXPAND"),
    };
    Ok(Rgba {
        width: info.width,
        height: info.height,
        pixels,
        mips: Vec::new(),
    })
}

/// Decode a VTF, picking the largest mip whose longer side is at most `max_side`.
pub fn load_texture(mount: &Mount, name: &str, max_side: u32) -> Result<Rgba> {
    load_texture_lod(mount, name, max_side, 0)
}

/// [`load_texture`] skipping `skip` more top mips (`mat_picmip`) and returning the mip
/// chain below the chosen level.
pub fn load_texture_lod(mount: &Mount, name: &str, max_side: u32, skip: u32) -> Result<Rgba> {
    // an absolute path (a save's preview under the user's home) is read from disk
    if std::path::Path::new(name).is_absolute() {
        return load_png(&std::fs::read(name)?);
    }
    let base = name.trim_start_matches('/');
    let path = format!("materials/{base}.vtf");
    let bytes = match mount.read(&path) {
        Ok(b) => b,
        Err(e) => {
            // `Material( "x.png" )`: the game also accepts PNG images under materials/
            let lower = base.to_ascii_lowercase();
            let is_png = lower.ends_with(".png");
            let is_jpeg = lower.ends_with(".jpg") || lower.ends_with(".jpeg");
            let png_path = if is_png {
                format!("materials/{base}")
            } else {
                format!("materials/{base}.png")
            };
            // or a raw path (`html/img/x.png`, `gamemodes/<gm>/logo.png`, `backgrounds/x.jpg`)
            let raw = if is_png || is_jpeg {
                mount.read(base).ok()
            } else {
                None
            };
            return match raw.or_else(|| mount.read(&png_path).ok()) {
                Some(b) if is_jpeg => load_jpeg(&b),
                Some(b) => load_png(&b),
                None => Err(e.into()),
            };
        }
    };
    let h = vtf::Header::parse(&bytes)?;
    let mut mip = 0u32;
    while mip + 1 < h.mipmap_count as u32 {
        let (w, hh, _) = vtf::decode::mip_dims(&h, mip);
        if w.max(hh) <= max_side {
            break;
        }
        mip += 1;
    }
    let mip = (mip + skip).min((h.mipmap_count as u32).saturating_sub(1));
    let (w, hh, _) = vtf::decode::mip_dims(&h, mip);
    let slice = vtf::decode::mip_slice(&h, &bytes, mip, 0, 0, 0)?;
    let pixels = vtf::decode::decode_rgba8(h.high_res_format, w, hh, slice)?;
    let mut mips = Vec::new();
    for m in mip + 1..h.mipmap_count as u32 {
        let (mw, mh, _) = vtf::decode::mip_dims(&h, m);
        let Ok(slice) = vtf::decode::mip_slice(&h, &bytes, m, 0, 0, 0) else {
            break;
        };
        let Ok(px) = vtf::decode::decode_rgba8(h.high_res_format, mw, mh, slice) else {
            break;
        };
        mips.push(px);
    }
    Ok(Rgba {
        width: w,
        height: hh,
        pixels,
        mips,
    })
}

/// Every frame of an animated VTF (`sprites/zerogxplode`), each at the mip that fits
/// `max_side`, with its smaller mips.
pub fn load_texture_frames(mount: &Mount, name: &str, max_side: u32) -> Result<Vec<Rgba>> {
    let base = name.trim_start_matches('/');
    let bytes = mount.read(&format!("materials/{base}.vtf"))?;
    let h = vtf::Header::parse(&bytes)?;
    let mut mip = 0u32;
    while mip + 1 < h.mipmap_count as u32 {
        let (w, hh, _) = vtf::decode::mip_dims(&h, mip);
        if w.max(hh) <= max_side {
            break;
        }
        mip += 1;
    }
    let (w, hh, _) = vtf::decode::mip_dims(&h, mip);
    let mut out = Vec::new();
    for frame in 0..h.frames.max(1) as u32 {
        let slice = vtf::decode::mip_slice(&h, &bytes, mip, frame, 0, 0)?;
        let pixels = vtf::decode::decode_rgba8(h.high_res_format, w, hh, slice)?;
        let mut mips = Vec::new();
        for m in mip + 1..h.mipmap_count as u32 {
            let (mw, mh, _) = vtf::decode::mip_dims(&h, m);
            let Ok(slice) = vtf::decode::mip_slice(&h, &bytes, m, frame, 0, 0) else {
                break;
            };
            let Ok(px) = vtf::decode::decode_rgba8(h.high_res_format, mw, mh, slice) else {
                break;
            };
            mips.push(px);
        }
        out.push(Rgba {
            width: w,
            height: hh,
            pixels,
            mips,
        });
    }
    Ok(out)
}

/// Numbers of a `[x y z]` / `{x y z}` / `x y z` value.
fn vec_of(v: Option<String>) -> Vec<f32> {
    v.map(|v| {
        v.trim_matches(|c| c == '{' || c == '}' || c == '[' || c == ']')
            .split_whitespace()
            .filter_map(|x| x.parse::<f32>().ok())
            .collect()
    })
    .unwrap_or_default()
}

/// Whether the VMT's `Proxies` block has `PlayerWeaponColor` writing `$selfillumtint`.
/// Whether the material's `Proxies` block has `proxy { resultVar var }`.
fn proxy_result(params: &[Node], proxy: &str, var: &str) -> bool {
    let Some(proxies) = params.iter().find_map(|n| match &n.value {
        Value::Block(b) if n.key.eq_ignore_ascii_case("proxies") => Some(b),
        _ => None,
    }) else {
        return false;
    };
    proxies.iter().any(|n| match &n.value {
        Value::Block(b) if n.key.eq_ignore_ascii_case(proxy) => b.iter().any(|m| match &m.value {
            Value::Str(s) if m.key.eq_ignore_ascii_case("resultvar") => s.eq_ignore_ascii_case(var),
            _ => false,
        }),
        _ => false,
    })
}

/// The `Proxies { TextureScroll { texturescrollvar <var> rate angle } }` block of a VMT for
/// `var`: (texturescrollrate, texturescrollangle in degrees).
fn texture_scroll(params: &[Node], var: &str) -> Option<(f32, f32)> {
    let proxies = params.iter().find_map(|n| match &n.value {
        Value::Block(b) if n.key.eq_ignore_ascii_case("proxies") => Some(b),
        _ => None,
    })?;
    proxies.iter().find_map(|n| {
        let Value::Block(b) = &n.value else {
            return None;
        };
        if !n.key.eq_ignore_ascii_case("texturescroll") {
            return None;
        }
        let get = |k: &str| {
            b.iter().find_map(|m| match &m.value {
                Value::Str(s) if m.key.eq_ignore_ascii_case(k) => Some(s.clone()),
                _ => None,
            })
        };
        if !get("texturescrollvar")?.eq_ignore_ascii_case(var) {
            return None;
        }
        let rate = get("texturescrollrate")?.parse::<f32>().ok()?;
        let angle = get("texturescrollangle")
            .and_then(|a| a.parse::<f32>().ok())
            .unwrap_or(0.0);
        Some((rate, angle))
    })
}

#[cfg(test)]
mod tests {
    use super::canonical_name;

    #[test]
    fn transform2() {
        let t = super::texture_transform("center .5 .5 scale 2 2 rotate 0 translate 0 0").unwrap();
        // uv (1, 1) about the centre scaled by 2 -> (1.5, 1.5)
        assert!((t[0] * 1.0 + t[1] * 1.0 + t[4] - 1.5).abs() < 1e-5);
        assert!((t[2] * 1.0 + t[3] * 1.0 + t[5] - 1.5).abs() < 1e-5);
        let r = super::texture_transform("center 0 0 scale 1 1 rotate 90 translate 0 0").unwrap();
        // (1, 0) rotated 90 deg -> (0, 1)
        assert!((r[0] - 0.0).abs() < 1e-5 && (r[2] - 1.0).abs() < 1e-5);
    }

    #[test]
    fn patched_names() {
        assert_eq!(
            canonical_name(
                "maps/gm_construct/building_template/building_template007b_-256_-1472_-96"
            ),
            "building_template/building_template007b"
        );
        assert_eq!(
            canonical_name("CONCRETE/ConcreteFloor001a"),
            "concrete/concretefloor001a"
        );
        assert_eq!(
            canonical_name("maps/gm_construct/water_13_beneath"),
            "water_13_beneath"
        );
    }

    #[test]
    fn texture_scroll_proxy() {
        let doc = gm_formats::keyvalues::parse(
            r#""Water" { "$bumpmap" "x" "Proxies" { "TextureScroll" { "texturescrollvar" "$bumptransform" "texturescrollrate" "0.02" "texturescrollangle" "25.00" } } }"#,
        )
        .unwrap();
        let gm_formats::keyvalues::Value::Block(params) = &doc.nodes[0].value else {
            panic!()
        };
        assert_eq!(
            super::texture_scroll(params, "$bumptransform"),
            Some((0.02, 25.0))
        );
        assert_eq!(super::texture_scroll(params, "$basetexturetransform"), None);
    }
}
