//! PHY collision models: the IVP compact surfaces studiomdl writes next to an MDL, plus the
//! text section with per solid mass, surface property and break models.
//!
//! Layout (all little endian): a 16 byte file header, then `solid_count` compact surfaces,
//! each a 28 byte `VPHY` header (absent in old files) followed by an IVP compact surface (mass centre, inertia,
//! ledge tree). Ledges are convex pieces: a triangle list over a shared point array. IVP works
//! in metres with Y up, points are converted here to Source units with Z up.

use crate::keyvalues;
use std::fmt;

#[derive(Debug)]
pub enum Error {
    Short(&'static str),
    Magic(&'static str),
    Version(u16),
    Text(crate::FormatError),
}

impl fmt::Display for Error {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Error::Short(what) => write!(f, "phy: truncated {what}"),
            Error::Magic(what) => write!(f, "phy: bad {what} magic"),
            Error::Version(v) => write!(f, "phy: unsupported solid version {v}"),
            Error::Text(e) => write!(f, "phy: text section: {e}"),
        }
    }
}

impl std::error::Error for Error {}

pub type Result<T> = std::result::Result<T, Error>;

/// Metres to Source units, the factor IVP uses (`1 / 0.0254`).
pub const METERS_TO_UNITS: f32 = 39.370_08;

/// One convex piece of a solid.
#[derive(Debug, Clone)]
pub struct Convex {
    /// Source units, Z up, relative to the solid's origin.
    pub points: Vec<[f32; 3]>,
    /// Indices into `points`.
    pub triangles: Vec<[u16; 3]>,
}

/// One rigid solid (a `$collisionmodel` or one `$collisionjoints` part).
#[derive(Debug, Clone)]
pub struct Solid {
    pub version: u16,
    pub model_type: u16,
    pub drag_axis_areas: [f32; 3],
    pub axis_map_size: u32,
    /// Source units, Z up.
    pub mass_center: [f32; 3],
    pub rotation_inertia: [f32; 3],
    pub upper_limit_radius: f32,
    pub convexes: Vec<Convex>,
}

/// The parsed text section (`solid { ... }` blocks and `editparams`).
#[derive(Debug, Clone, Default)]
pub struct SolidProps {
    pub index: u32,
    pub name: String,
    pub mass: f32,
    pub surfaceprop: String,
    pub damping: f32,
    pub rotdamping: f32,
    pub inertia: f32,
    pub volume: f32,
}

#[derive(Debug, Clone)]
pub struct Phy {
    pub checksum: u32,
    pub solids: Vec<Solid>,
    pub props: Vec<SolidProps>,
    pub text: String,
}

fn u16_at(d: &[u8], o: usize, what: &'static str) -> Result<u16> {
    d.get(o..o + 2)
        .map(|b| u16::from_le_bytes([b[0], b[1]]))
        .ok_or(Error::Short(what))
}

fn u32_at(d: &[u8], o: usize, what: &'static str) -> Result<u32> {
    d.get(o..o + 4)
        .map(|b| u32::from_le_bytes([b[0], b[1], b[2], b[3]]))
        .ok_or(Error::Short(what))
}

fn f32_at(d: &[u8], o: usize, what: &'static str) -> Result<f32> {
    u32_at(d, o, what).map(f32::from_bits)
}

fn vec3_at(d: &[u8], o: usize, what: &'static str) -> Result<[f32; 3]> {
    Ok([
        f32_at(d, o, what)?,
        f32_at(d, o + 4, what)?,
        f32_at(d, o + 8, what)?,
    ])
}

/// IVP (x, y up, z) in metres to Source (x, y, z up) in units.
fn ivp_to_source(v: [f32; 3]) -> [f32; 3] {
    [
        v[0] * METERS_TO_UNITS,
        v[2] * METERS_TO_UNITS,
        -v[1] * METERS_TO_UNITS,
    ]
}

impl Phy {
    pub fn parse(data: &[u8]) -> Result<Self> {
        let header_size = u32_at(data, 0, "header")? as usize;
        let solid_count = u32_at(data, 8, "header")? as usize;
        let checksum = u32_at(data, 12, "header")?;
        let mut off = header_size;
        let mut solids = Vec::with_capacity(solid_count);
        for _ in 0..solid_count {
            let size = u32_at(data, off, "solid header")? as usize;
            let body = data
                .get(off + 4..off + 4 + size)
                .ok_or(Error::Short("solid"))?;
            solids.push(parse_solid(body)?);
            off += 4 + size;
        }
        let text_bytes = data.get(off..).unwrap_or(&[]);
        let end = text_bytes
            .iter()
            .position(|&b| b == 0)
            .unwrap_or(text_bytes.len());
        let text = String::from_utf8_lossy(&text_bytes[..end]).into_owned();
        let props = parse_text(&text)?;
        Ok(Phy {
            checksum,
            solids,
            props,
            text,
        })
    }
}

fn parse_solid(d: &[u8]) -> Result<Solid> {
    // Studiomdl before the VPHY header (older HL2 props such as props_wasteland/bridge_railing)
    // wrote the bare compact surface right after the size field.
    let legacy = d.get(0..4) != Some(b"VPHY");
    let (version, model_type, drag_axis_areas, axis_map_size, s) = if legacy {
        (0, 0, [0.0; 3], 0, d)
    } else {
        let version = u16_at(d, 4, "solid header")?;
        let model_type = u16_at(d, 6, "solid header")?;
        let surface_size = u32_at(d, 8, "solid header")? as usize;
        let drag_axis_areas = vec3_at(d, 12, "solid header")?;
        let axis_map_size = u32_at(d, 24, "solid header")?;
        if version != 0x100 {
            return Err(Error::Version(version));
        }
        let s = d
            .get(28..28 + surface_size)
            .ok_or(Error::Short("compact surface"))?;
        (version, model_type, drag_axis_areas, axis_map_size, s)
    };
    // compactsurface: mass_center, rotation_inertia, upper_limit_radius,
    // max_deviation:8 | byte_size:24, offset_ledgetree_root, dummy[3] ("IVPS" in the last)
    let mass_center = ivp_to_source(vec3_at(s, 0, "compact surface")?);
    let ri = vec3_at(s, 12, "compact surface")?;
    let rotation_inertia = [ri[0], ri[2], ri[1]];
    let upper_limit_radius = f32_at(s, 24, "compact surface")? * METERS_TO_UNITS;
    let byte_size = (u32_at(s, 28, "compact surface")? >> 8) as usize;
    let ledgetree_root = u32_at(s, 32, "compact surface")? as usize;
    if s.get(44..48) != Some(b"IVPS") {
        return Err(Error::Magic("IVPS"));
    }
    // Ledges follow the surface header back to back; each one is a 16 byte header plus 16 bytes
    // per triangle. Points sit either right after a ledge's triangles (then `size_div_16` covers
    // them) or in a shared array after all ledges; `c_point_offset` is relative to the ledge.
    let mut ledge_end = ledgetree_root.min(byte_size).min(s.len());
    let mut convexes = Vec::new();
    let mut off = 48;
    while off + 16 <= ledge_end {
        let point_offset = u32_at(s, off, "ledge")? as usize;
        let n_tris = u16_at(s, off + 12, "ledge")? as usize;
        let tris_end = off + 16 + n_tris * 16;
        if n_tris == 0 || tris_end > s.len() {
            return Err(Error::Short("ledge"));
        }
        let points_base = off + point_offset;
        ledge_end = ledge_end.min(points_base);
        let ledge = &s[off..tris_end];
        let mut raw = Vec::with_capacity(n_tris);
        let mut max_index = 0usize;
        for t in 0..n_tris {
            let to = 16 + t * 16;
            let mut tri = [0u16; 3];
            for (e, idx) in tri.iter_mut().enumerate() {
                let edge = u32_at(ledge, to + 4 + e * 4, "triangle")?;
                *idx = (edge & 0xffff) as u16;
                max_index = max_index.max(*idx as usize);
            }
            raw.push(tri);
        }
        let mut points = Vec::with_capacity(max_index + 1);
        for p in 0..=max_index {
            points.push(ivp_to_source(vec3_at(
                s,
                points_base + p * 16,
                "ledge points",
            )?));
        }
        convexes.push(Convex {
            points,
            triangles: raw,
        });
        off = tris_end;
    }
    Ok(Solid {
        version,
        model_type,
        drag_axis_areas,
        axis_map_size,
        mass_center,
        rotation_inertia,
        upper_limit_radius,
        convexes,
    })
}

fn parse_text(text: &str) -> Result<Vec<SolidProps>> {
    if text.trim().is_empty() {
        return Ok(Vec::new());
    }
    // Some shipped files have typos in `editparams` ("break break {"), so when the whole
    // section fails to parse the solids are read on their own.
    let kv = match keyvalues::parse(text) {
        Ok(kv) => kv,
        Err(e) => {
            let head = text.find("editparams").map(|i| &text[..i]).unwrap_or(text);
            keyvalues::parse(head).map_err(|_| Error::Text(e))?
        }
    };
    let mut out = Vec::new();
    for node in kv
        .nodes
        .iter()
        .filter(|n| n.key.eq_ignore_ascii_case("solid"))
    {
        let get = |k: &str| node.get_str(k).unwrap_or("");
        let num = |k: &str| get(k).trim().parse::<f32>().unwrap_or(0.0);
        out.push(SolidProps {
            index: num("index") as u32,
            name: get("name").to_string(),
            mass: num("mass"),
            surfaceprop: get("surfaceprop").to_string(),
            damping: num("damping"),
            rotdamping: num("rotdamping"),
            inertia: num("inertia"),
            volume: num("volume"),
        });
    }
    Ok(out)
}
