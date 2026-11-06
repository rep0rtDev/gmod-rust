//! `.pcf` particle system definitions: a DMX file (`binary 2` encoding, `pcf 1` format) whose
//! root `DmElement` lists `DmeParticleSystemDefinition`s, each with its `renderers`,
//! `operators`, `initializers`, `emitters`, `forces`, `constraints` and `children` as
//! `DmeParticleOperator` elements (the function in `functionName`, its parameters as the
//! other attributes).
use crate::{FormatError, Reader, Result};

#[derive(Clone, Debug, PartialEq)]
pub enum Value {
    /// Index into `Dmx::elements` (`None`: the null element).
    Element(Option<usize>),
    Int(i32),
    Float(f32),
    Bool(bool),
    Str(String),
    Binary(Vec<u8>),
    Id([u8; 16]),
    Color([u8; 4]),
    Vec2([f32; 2]),
    Vec3([f32; 3]),
    Vec4([f32; 4]),
    Matrix([f32; 16]),
    Array(Vec<Value>),
}

#[derive(Clone, Debug)]
pub struct Element {
    pub type_name: String,
    pub name: String,
    pub id: [u8; 16],
    pub attrs: Vec<(String, Value)>,
}

impl Element {
    pub fn get(&self, name: &str) -> Option<&Value> {
        self.attrs
            .iter()
            .find(|(n, _)| n.eq_ignore_ascii_case(name))
            .map(|(_, v)| v)
    }
    pub fn float(&self, name: &str) -> Option<f32> {
        match self.get(name)? {
            Value::Float(f) => Some(*f),
            Value::Int(i) => Some(*i as f32),
            _ => None,
        }
    }
    pub fn int(&self, name: &str) -> Option<i32> {
        match self.get(name)? {
            Value::Int(i) => Some(*i),
            Value::Float(f) => Some(*f as i32),
            _ => None,
        }
    }
    pub fn bool(&self, name: &str) -> Option<bool> {
        match self.get(name)? {
            Value::Bool(b) => Some(*b),
            Value::Int(i) => Some(*i != 0),
            _ => None,
        }
    }
    pub fn str(&self, name: &str) -> Option<&str> {
        match self.get(name)? {
            Value::Str(s) => Some(s),
            _ => None,
        }
    }
    pub fn vec3(&self, name: &str) -> Option<[f32; 3]> {
        match self.get(name)? {
            Value::Vec3(v) => Some(*v),
            _ => None,
        }
    }
    pub fn color(&self, name: &str) -> Option<[u8; 4]> {
        match self.get(name)? {
            Value::Color(c) => Some(*c),
            _ => None,
        }
    }
    /// The element indices of an element-array attribute (null entries dropped).
    pub fn elements(&self, name: &str) -> Vec<usize> {
        match self.get(name) {
            Some(Value::Array(items)) => items
                .iter()
                .filter_map(|v| match v {
                    Value::Element(e) => *e,
                    _ => None,
                })
                .collect(),
            Some(Value::Element(Some(e))) => vec![*e],
            _ => Vec::new(),
        }
    }
}

/// A parsed DMX file: `elements[0]` is the root.
#[derive(Clone, Debug, Default)]
pub struct Dmx {
    pub encoding_version: u32,
    pub format: String,
    pub format_version: u32,
    pub elements: Vec<Element>,
}

fn malformed(detail: impl Into<String>) -> FormatError {
    FormatError::Malformed {
        what: "dmx",
        detail: detail.into(),
    }
}

impl Dmx {
    pub fn parse(data: &[u8]) -> Result<Self> {
        let mut r = Reader::new(data);
        let header = r.cstr()?;
        let words: Vec<&str> = header.split_whitespace().collect();
        // <!-- dmx encoding binary 2 format pcf 1 -->
        let field = |key: &str| {
            words
                .iter()
                .position(|w| *w == key)
                .and_then(|i| words.get(i + 1).copied())
        };
        if words.get(1) != Some(&"dmx") || field("encoding") != Some("binary") {
            return Err(FormatError::BadMagic {
                expected: "<!-- dmx encoding binary",
                found: data[..data.len().min(16)].to_vec(),
            });
        }
        let ver = |key: &str| -> Result<u32> {
            field(key)
                .and_then(|w| w.parse().ok())
                .ok_or_else(|| malformed(format!("header without {key} version: {header}")))
        };
        let encoding_version = ver("binary")?;
        if !(1..=2).contains(&encoding_version) {
            return Err(FormatError::UnsupportedVersion {
                what: "dmx binary",
                version: encoding_version,
            });
        }
        let format = field("format").unwrap_or("").to_string();
        let format_version = ver(&format).unwrap_or(0);
        let strings = if encoding_version >= 2 {
            // binary 2 and 3 count the string table in 16 bits.
            let n = r.u16()?;
            let mut v = Vec::with_capacity(n as usize);
            for _ in 0..n {
                v.push(r.cstr()?.to_string());
            }
            v
        } else {
            Vec::new()
        };
        let lookup = |i: i16| -> Result<&str> {
            strings
                .get(i as usize)
                .map(String::as_str)
                .ok_or_else(|| malformed(format!("string index {i} of {}", strings.len())))
        };
        let n_elements = r.i32()?;
        if n_elements < 0 {
            return Err(malformed("negative element count"));
        }
        let mut elements = Vec::with_capacity(n_elements as usize);
        for _ in 0..n_elements {
            let type_name = if encoding_version >= 2 {
                lookup(r.u16()? as i16)?.to_string()
            } else {
                r.cstr()?.to_string()
            };
            let name = r.cstr()?.to_string();
            let id = r.array::<16>()?;
            elements.push(Element {
                type_name,
                name,
                id,
                attrs: Vec::new(),
            });
        }
        let n_total = elements.len();
        for element in &mut elements {
            let n_attrs = r.i32()?;
            if n_attrs < 0 {
                return Err(malformed("negative attribute count"));
            }
            let mut attrs = Vec::with_capacity(n_attrs as usize);
            for _ in 0..n_attrs {
                let name = if encoding_version >= 2 {
                    lookup(r.u16()? as i16)?.to_string()
                } else {
                    r.cstr()?.to_string()
                };
                let ty = r.u8()?;
                let value = read_value(&mut r, ty, n_total)?;
                attrs.push((name, value));
            }
            element.attrs = attrs;
        }
        Ok(Dmx {
            encoding_version,
            format,
            format_version,
            elements,
        })
    }

    pub fn root(&self) -> Option<&Element> {
        self.elements.first()
    }
}

fn read_scalar(r: &mut Reader, ty: u8, n_elements: usize) -> Result<Value> {
    Ok(match ty {
        1 => {
            let i = r.i32()?;
            Value::Element((i >= 0 && (i as usize) < n_elements).then_some(i as usize))
        }
        2 => Value::Int(r.i32()?),
        3 => Value::Float(r.f32()?),
        4 => Value::Bool(r.u8()? != 0),
        5 => Value::Str(r.cstr()?.to_string()),
        6 => {
            let n = r.i32()?;
            if n < 0 {
                return Err(malformed("negative binary length"));
            }
            Value::Binary(r.bytes(n as usize)?.to_vec())
        }
        7 => Value::Id(r.array::<16>()?),
        8 => Value::Color(r.array::<4>()?),
        9 => Value::Vec2([r.f32()?, r.f32()?]),
        10 | 12 => Value::Vec3([r.f32()?, r.f32()?, r.f32()?]),
        11 | 13 => Value::Vec4([r.f32()?, r.f32()?, r.f32()?, r.f32()?]),
        14 => {
            let mut m = [0.0; 16];
            for v in &mut m {
                *v = r.f32()?;
            }
            Value::Matrix(m)
        }
        _ => return Err(malformed(format!("attribute type {ty} at {}", r.pos()))),
    })
}

fn read_value(r: &mut Reader, ty: u8, n_elements: usize) -> Result<Value> {
    if (15..=28).contains(&ty) {
        let n = r.i32()?;
        if n < 0 {
            return Err(malformed("negative array length"));
        }
        let mut items = Vec::with_capacity((n as usize).min(1 << 16));
        for _ in 0..n {
            items.push(read_scalar(r, ty - 14, n_elements)?);
        }
        Ok(Value::Array(items))
    } else {
        read_scalar(r, ty, n_elements)
    }
}

/// The particle systems of a `.pcf`.
#[derive(Clone, Debug, Default)]
pub struct Pcf {
    pub dmx: Dmx,
    /// Indices of the `DmeParticleSystemDefinition` elements.
    pub systems: Vec<usize>,
}

/// The operator lists of a particle system definition.
pub const OPERATOR_LISTS: [&str; 7] = [
    "renderers",
    "operators",
    "initializers",
    "emitters",
    "forces",
    "constraints",
    "children",
];

impl Pcf {
    pub fn parse(data: &[u8]) -> Result<Self> {
        let dmx = Dmx::parse(data)?;
        let mut systems: Vec<usize> = dmx
            .root()
            .map(|r| r.elements("particleSystemDefinitions"))
            .unwrap_or_default();
        for (i, e) in dmx.elements.iter().enumerate() {
            if e.type_name == "DmeParticleSystemDefinition" && !systems.contains(&i) {
                systems.push(i);
            }
        }
        Ok(Pcf { dmx, systems })
    }

    pub fn system(&self, name: &str) -> Option<&Element> {
        self.systems
            .iter()
            .map(|&i| &self.dmx.elements[i])
            .find(|e| e.name.eq_ignore_ascii_case(name))
    }

    pub fn system_names(&self) -> impl Iterator<Item = &str> {
        self.systems
            .iter()
            .map(|&i| self.dmx.elements[i].name.as_str())
    }

    /// The `DmeParticleOperator`s of one list (`operators`, `initializers`, ...) of a system.
    pub fn operators<'a>(&'a self, system: &Element, list: &str) -> Vec<&'a Element> {
        system
            .elements(list)
            .into_iter()
            .filter_map(|i| self.dmx.elements.get(i))
            .collect()
    }

    /// A child system reference (`children` entries point at a `DmeParticleChild` whose
    /// `child` is the definition) resolved to the definition.
    pub fn child_definition<'a>(&'a self, child: &Element) -> Option<&'a Element> {
        match child.get("child") {
            Some(Value::Element(Some(i))) => self.dmx.elements.get(*i),
            _ => None,
        }
    }
}

/// The function of a `DmeParticleOperator`: `functionName`, falling back to the element's
/// name (`C_OP_Decay`, `C_INIT_RandomLifeTime`, `render_animated_sprites`, ...).
pub fn function_name(op: &Element) -> &str {
    op.str("functionName").unwrap_or(op.name.as_str())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn gmod_effects_lists_its_systems_and_operators() {
        let Ok(dir) = std::env::var("GMOD_DIR") else {
            eprintln!("skipped: GMOD_DIR unset");
            return;
        };
        let path = std::path::Path::new(&dir).join("garrysmod/particles/gmod_effects.pcf");
        let Ok(data) = std::fs::read(&path) else {
            eprintln!("skipped: {} missing", path.display());
            return;
        };
        let pcf = Pcf::parse(&data).unwrap();
        assert_eq!(pcf.dmx.format, "pcf");
        assert!(!pcf.systems.is_empty(), "no systems");
        let mut ops = 0;
        for &i in &pcf.systems {
            let s = &pcf.dmx.elements[i];
            assert_eq!(s.type_name, "DmeParticleSystemDefinition");
            assert!(s.str("material").is_some(), "{} without material", s.name);
            for list in OPERATOR_LISTS {
                for op in pcf.operators(s, list) {
                    assert!(
                        op.type_name == "DmeParticleOperator" || op.type_name == "DmeParticleChild",
                        "{list} of {} holds a {}",
                        s.name,
                        op.type_name
                    );
                    ops += 1;
                }
            }
        }
        assert!(ops > 0, "no operators");
        let names: Vec<&str> = pcf.system_names().collect();
        eprintln!("{} systems, {ops} operators: {names:?}", names.len());
    }
}
