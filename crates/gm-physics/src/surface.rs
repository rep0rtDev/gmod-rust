//! `scripts/surfaceproperties.txt`: per material friction, elasticity and density, with
//! `base` inheritance (`Wood_Crate` -> `Wood`), and the sound script names (`impacthard`...).

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

/// One `surfaceproperties` entry: its `base`, numeric fields and sound script names.
#[derive(Debug, Default, Clone)]
struct Material {
    base: Option<String>,
    nums: HashMap<String, f32>,
    names: HashMap<String, String>,
}

#[derive(Debug, Default, Clone)]
pub struct SurfaceTable {
    raw: HashMap<String, Material>,
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
                let mut names = HashMap::new();
                let mut base = None;
                for child in node.children() {
                    let Some(v) = child.as_str() else { continue };
                    if child.key.eq_ignore_ascii_case("base") {
                        base = Some(v.to_ascii_lowercase());
                    } else if let Ok(f) = v.trim().parse::<f32>() {
                        vals.insert(child.key.to_ascii_lowercase(), f);
                    } else {
                        names.insert(child.key.to_ascii_lowercase(), v.to_string());
                    }
                }
                raw.insert(
                    node.key.to_ascii_lowercase(),
                    Material {
                        base,
                        nums: vals,
                        names,
                    },
                );
            }
        }
        SurfaceTable { raw }
    }

    /// The `base` chain of a material, root first (`["wood", "wood_crate"]`).
    fn chain(&self, name: &str) -> Vec<String> {
        let mut chain = Vec::new();
        let mut cur = Some(name.to_ascii_lowercase());
        while let Some(n) = cur {
            if chain.contains(&n) || chain.len() > 16 {
                break;
            }
            match self.raw.get(&n) {
                Some(m) => {
                    chain.push(n);
                    cur = m.base.clone();
                }
                None => break,
            }
        }
        chain.reverse();
        chain
    }

    /// A sound script name of a material (`impacthard`, `impactsoft`, `scraperough`,
    /// `stepleft`...), the nearest definition up the `base` chain.
    pub fn sound(&self, name: &str, key: &str) -> Option<String> {
        self.chain(name)
            .iter()
            .rev()
            .find_map(|n| self.raw[n].names.get(key).cloned())
    }

    pub fn get(&self, name: &str) -> SurfaceProps {
        let mut out = SurfaceProps::default();
        // apply from the root of the chain down so children override
        for n in self.chain(name) {
            let vals = &self.raw[&n].nums;
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

    #[test]
    fn sound_names_follow_the_base_chain() {
        let t = SurfaceTable::parse(&[r#"
"wood" { "impacthard" "Wood.ImpactHard" "impactsoft" "Wood.ImpactSoft" }
"Wood_Crate" { "base" "Wood" "impacthard" "Wood_Crate.ImpactHard" }
"#]);
        assert_eq!(
            t.sound("wood_crate", "impacthard").as_deref(),
            Some("Wood_Crate.ImpactHard")
        );
        assert_eq!(
            t.sound("Wood_Crate", "impactsoft").as_deref(),
            Some("Wood.ImpactSoft")
        );
        assert_eq!(t.sound("wood_crate", "break"), None);
        assert_eq!(t.sound("nothing", "impacthard"), None);
    }
}
