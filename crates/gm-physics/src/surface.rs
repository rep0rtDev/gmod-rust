//! `scripts/surfaceproperties.txt`: per material friction, elasticity and density, with
//! `base` inheritance (`Wood_Crate` -> `Wood`).

use gm_formats::keyvalues;
use std::collections::HashMap;

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct SurfaceProps {
    pub friction: f32,
    pub elasticity: f32,
    pub density: f32,
    pub dampening: f32,
}

impl Default for SurfaceProps {
    fn default() -> Self {
        // the "default" entry of the shipped file
        SurfaceProps {
            friction: 0.8,
            elasticity: 0.25,
            density: 2000.0,
            dampening: 0.0,
        }
    }
}

#[derive(Debug, Default, Clone)]
pub struct SurfaceTable {
    raw: HashMap<String, (Option<String>, HashMap<String, f32>)>,
}

impl SurfaceTable {
    /// Parse one or more surfaceproperties files (later files override earlier entries).
    pub fn parse(texts: &[&str]) -> Self {
        let mut raw = HashMap::new();
        for text in texts {
            let Ok(doc) = keyvalues::parse(text) else {
                continue;
            };
            for node in &doc.nodes {
                let mut vals = HashMap::new();
                let mut base = None;
                for child in node.children() {
                    let Some(v) = child.as_str() else { continue };
                    if child.key.eq_ignore_ascii_case("base") {
                        base = Some(v.to_ascii_lowercase());
                    } else if let Ok(f) = v.trim().parse::<f32>() {
                        vals.insert(child.key.to_ascii_lowercase(), f);
                    }
                }
                raw.insert(node.key.to_ascii_lowercase(), (base, vals));
            }
        }
        SurfaceTable { raw }
    }

    pub fn get(&self, name: &str) -> SurfaceProps {
        let mut out = SurfaceProps::default();
        let mut chain = Vec::new();
        let mut cur = Some(name.to_ascii_lowercase());
        while let Some(n) = cur {
            if chain.contains(&n) || chain.len() > 16 {
                break;
            }
            match self.raw.get(&n) {
                Some((base, _)) => {
                    chain.push(n);
                    cur = base.clone();
                }
                None => break,
            }
        }
        // apply from the root of the chain down so children override
        for n in chain.iter().rev() {
            let (_, vals) = &self.raw[n];
            if let Some(v) = vals.get("friction") {
                out.friction = *v;
            }
            if let Some(v) = vals.get("elasticity") {
                out.elasticity = *v;
            }
            if let Some(v) = vals.get("density") {
                out.density = *v;
            }
            if let Some(v) = vals.get("dampening") {
                out.dampening = *v;
            }
        }
        out
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn base_inheritance() {
        let t = SurfaceTable::parse(&[r#"
"default" { "density" "2000" "elasticity" "0.25" "friction" "0.8" "dampening" "0.0" }
"wood" { "density" "700" "elasticity" "0.1" "friction" "0.8" }
"Wood_Crate" { "base" "Wood" }
"#]);
        let c = t.get("wood_crate");
        assert_eq!(c.elasticity, 0.1);
        assert_eq!(c.density, 700.0);
        assert_eq!(t.get("nothing").elasticity, 0.25);
    }
}
