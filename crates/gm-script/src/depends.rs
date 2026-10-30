//! Addon dependencies: `depends = ["base", "wire >= 1.2, < 2", "props ^0.3"]` in the manifest.
//! Versions are `major.minor.patch` (missing parts are 0); a requirement is an id followed by
//! comma-separated constraints with `=`, `>`, `>=`, `<`, `<=`, `^` (same major, at least) or
//! `~` (same major.minor, at least); a bare id accepts any version.

use std::collections::BTreeMap;

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub struct Version(pub u64, pub u64, pub u64);

impl Version {
    pub fn parse(s: &str) -> Option<Version> {
        let s = s.trim();
        if s.is_empty() {
            return None;
        }
        let mut parts = [0u64; 3];
        for (i, p) in s.split('.').enumerate() {
            if i >= 3 {
                return None;
            }
            parts[i] = p.trim().parse().ok()?;
        }
        Some(Version(parts[0], parts[1], parts[2]))
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Op {
    Eq,
    Gt,
    Ge,
    Lt,
    Le,
    Caret,
    Tilde,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Requirement {
    pub id: String,
    pub constraints: Vec<(Op, Version)>,
}

impl Requirement {
    pub fn parse(s: &str) -> Result<Requirement, DependsError> {
        let s = s.trim();
        let bad = || DependsError::Syntax(s.to_string());
        let end = s
            .find(|c: char| !(c.is_alphanumeric() || c == '_' || c == '-'))
            .unwrap_or(s.len());
        let id = &s[..end];
        if id.is_empty() {
            return Err(bad());
        }
        let mut constraints = Vec::new();
        let rest = s[end..].trim();
        if !rest.is_empty() {
            for c in rest.split(',') {
                let c = c.trim();
                let (op, v) = if let Some(v) = c.strip_prefix(">=") {
                    (Op::Ge, v)
                } else if let Some(v) = c.strip_prefix("<=") {
                    (Op::Le, v)
                } else if let Some(v) = c.strip_prefix('>') {
                    (Op::Gt, v)
                } else if let Some(v) = c.strip_prefix('<') {
                    (Op::Lt, v)
                } else if let Some(v) = c.strip_prefix('=') {
                    (Op::Eq, v)
                } else if let Some(v) = c.strip_prefix('^') {
                    (Op::Caret, v)
                } else if let Some(v) = c.strip_prefix('~') {
                    (Op::Tilde, v)
                } else {
                    return Err(bad());
                };
                constraints.push((op, Version::parse(v).ok_or_else(bad)?));
            }
        }
        Ok(Requirement {
            id: id.to_string(),
            constraints,
        })
    }

    pub fn matches(&self, v: Version) -> bool {
        self.constraints.iter().all(|&(op, r)| match op {
            Op::Eq => v == r,
            Op::Gt => v > r,
            Op::Ge => v >= r,
            Op::Lt => v < r,
            Op::Le => v <= r,
            Op::Caret => v >= r && v.0 == r.0,
            Op::Tilde => v >= r && v.0 == r.0 && v.1 == r.1,
        })
    }
}

#[derive(Debug, thiserror::Error)]
pub enum DependsError {
    #[error("bad dependency `{0}`")]
    Syntax(String),
    #[error("addon {0} version `{1}` is not major.minor.patch")]
    BadVersion(String, String),
    #[error("addon {0} depends on {1}, which is not loaded")]
    Missing(String, String),
    #[error("addon {0} depends on `{1}`, but {2} is {3}")]
    Mismatch(String, String, String, String),
    #[error("dependency cycle among {0:?}")]
    Cycle(Vec<String>),
    #[error("addon {0} is already loaded")]
    AlreadyLoaded(String),
}

/// Checks one addon's requirements against the loaded versions.
pub fn check(
    id: &str,
    depends: &[String],
    loaded: &BTreeMap<String, String>,
) -> Result<(), DependsError> {
    for d in depends {
        let req = Requirement::parse(d)?;
        let Some(have) = loaded.get(&req.id) else {
            return Err(DependsError::Missing(id.into(), req.id));
        };
        let v = Version::parse(have)
            .ok_or_else(|| DependsError::BadVersion(req.id.clone(), have.clone()))?;
        if !req.matches(v) {
            return Err(DependsError::Mismatch(
                id.into(),
                d.clone(),
                req.id,
                have.clone(),
            ));
        }
    }
    Ok(())
}

/// Load order for `(id, depends)` pairs: dependencies before dependents, the given order
/// otherwise; only dependencies among the given ids are ordered (the rest is checked by `check`).
pub fn order(addons: &[(String, Vec<String>)]) -> Result<Vec<usize>, DependsError> {
    let index: BTreeMap<&str, usize> = addons
        .iter()
        .enumerate()
        .map(|(i, (id, _))| (id.as_str(), i))
        .collect();
    let mut out = Vec::new();
    let mut state = vec![0u8; addons.len()]; // 0 new, 1 on the stack, 2 done
    fn visit(
        i: usize,
        addons: &[(String, Vec<String>)],
        index: &BTreeMap<&str, usize>,
        state: &mut [u8],
        out: &mut Vec<usize>,
        stack: &mut Vec<String>,
    ) -> Result<(), DependsError> {
        match state[i] {
            2 => return Ok(()),
            1 => {
                stack.push(addons[i].0.clone());
                return Err(DependsError::Cycle(std::mem::take(stack)));
            }
            _ => {}
        }
        state[i] = 1;
        stack.push(addons[i].0.clone());
        for d in &addons[i].1 {
            let req = Requirement::parse(d)?;
            if let Some(&j) = index.get(req.id.as_str()) {
                visit(j, addons, index, state, out, stack)?;
            }
        }
        stack.pop();
        state[i] = 2;
        out.push(i);
        Ok(())
    }
    for i in 0..addons.len() {
        visit(i, addons, &index, &mut state, &mut out, &mut Vec::new())?;
    }
    Ok(out)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn requirements_parse_and_match() {
        let r = Requirement::parse("wire >= 1.2, < 2").unwrap();
        assert_eq!(r.id, "wire");
        assert!(r.matches(Version(1, 2, 0)));
        assert!(r.matches(Version(1, 9, 3)));
        assert!(!r.matches(Version(2, 0, 0)));
        assert!(!r.matches(Version(1, 1, 9)));
        let c = Requirement::parse("props ^0.3").unwrap();
        assert!(c.matches(Version(0, 3, 0)) && c.matches(Version(0, 9, 0)));
        assert!(!c.matches(Version(1, 0, 0)) && !c.matches(Version(0, 2, 9)));
        let t = Requirement::parse("base~1.4.2").unwrap();
        assert!(t.matches(Version(1, 4, 7)) && !t.matches(Version(1, 5, 0)));
        assert!(Requirement::parse("base")
            .unwrap()
            .matches(Version(7, 0, 0)));
        assert!(Requirement::parse("base = 1")
            .unwrap()
            .matches(Version(1, 0, 0)));
        assert!(Requirement::parse(">= 1").is_err());
        assert!(Requirement::parse("base >= x").is_err());
        assert!(Requirement::parse("base ! 1").is_err());
        assert_eq!(Version::parse("1.2"), Some(Version(1, 2, 0)));
        assert_eq!(Version::parse("1.2.3.4"), None);
    }

    #[test]
    fn order_puts_dependencies_first_and_rejects_cycles() {
        let a = |id: &str, d: &[&str]| (id.to_string(), d.iter().map(|s| s.to_string()).collect());
        let addons = vec![
            a("hud", &["base >= 1", "wire"]),
            a("wire", &["base"]),
            a("base", &[]),
            a("solo", &["unknown"]),
        ];
        assert_eq!(order(&addons).unwrap(), vec![2, 1, 0, 3]);
        let cyc = vec![a("x", &["y"]), a("y", &["x"])];
        assert!(matches!(order(&cyc), Err(DependsError::Cycle(_))));
        let mut loaded = BTreeMap::new();
        loaded.insert("base".to_string(), "1.4.0".to_string());
        assert!(check("hud", &["base ^1".into()], &loaded).is_ok());
        assert!(matches!(
            check("hud", &["base >= 2".into()], &loaded),
            Err(DependsError::Mismatch(..))
        ));
        assert!(matches!(
            check("hud", &["wire".into()], &loaded),
            Err(DependsError::Missing(..))
        ));
    }
}
