//! Resolving a BSP material name to a VMT and its base texture.

use anyhow::{anyhow, Result};
use gm_content::Mount;
use gm_formats::keyvalues::{self, Value};
use gm_formats::vtf;

#[derive(Clone, Debug, Default)]
pub struct Material {
    pub name: String,
    pub shader: String,
    pub base_texture: Option<String>,
    pub base_texture2: Option<String>,
    pub translucent: bool,
    /// `$additive 1`: drawn as src + dst (the UI approximates it by alpha = max(r, g, b)).
    pub additive: bool,
    /// `$halflambert`: models lit with the half-Lambert wrap (characters), else plain Lambert.
    pub half_lambert: bool,
    pub no_draw: bool,
    pub water: bool,
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
        let get = |k: &str| -> Option<String> {
            params.iter().find_map(|n| {
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
        mat.shader = shader;
        if mat.base_texture.is_none() {
            mat.base_texture = get("$basetexture");
        }
        if mat.base_texture2.is_none() {
            mat.base_texture2 = get("$basetexture2");
        }
        mat.half_lambert = matches!(get("$halflambert").as_deref(), Some("1"));
        mat.translucent = matches!(get("$translucent").as_deref(), Some("1"))
            || matches!(get("$alphatest").as_deref(), Some("1"));
        mat.additive = matches!(get("$additive").as_deref(), Some("1"));
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
    })
}

/// Decode a VTF, picking the largest mip whose longer side is at most `max_side`.
pub fn load_texture(mount: &Mount, name: &str, max_side: u32) -> Result<Rgba> {
    let base = name.trim_start_matches('/');
    let path = format!("materials/{base}.vtf");
    let bytes = match mount.read(&path) {
        Ok(b) => b,
        Err(e) => {
            // `Material( "x.png" )`: the game also accepts PNG images under materials/
            let png_path = if base.to_ascii_lowercase().ends_with(".png") {
                format!("materials/{base}")
            } else {
                format!("materials/{base}.png")
            };
            return match mount.read(&png_path) {
                Ok(b) => load_png(&b),
                Err(_) => Err(e.into()),
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
    let (w, hh, _) = vtf::decode::mip_dims(&h, mip);
    let slice = vtf::decode::mip_slice(&h, &bytes, mip, 0, 0, 0)?;
    let pixels = vtf::decode::decode_rgba8(h.high_res_format, w, hh, slice)?;
    Ok(Rgba {
        width: w,
        height: hh,
        pixels,
    })
}

#[cfg(test)]
mod tests {
    use super::canonical_name;

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
}
