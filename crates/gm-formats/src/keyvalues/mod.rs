//! Valve KeyValues (VDF) text format: `gameinfo.txt`, VMT materials,
//! `surfaceproperties.txt`, sound scripts, spawn lists.
//!
//! Grammar handled: quoted and bare tokens, nested `{}` blocks, `//` comments,
//! `#include` / `#base` directives (returned to the caller, not resolved here),
//! and trailing `[$CONDITION]` tags, which are kept on the node for the caller
//! to evaluate. Duplicate keys are preserved in order, as the engine does.

use crate::{FormatError, Result};

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Value {
    Str(String),
    Block(Vec<Node>),
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Node {
    pub key: String,
    pub value: Value,
    /// `[$WIN32]`-style conditional, without brackets, if present.
    pub condition: Option<String>,
}

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Document {
    pub nodes: Vec<Node>,
    /// `#include` and `#base` paths in file order.
    pub includes: Vec<(String, String)>,
}

impl Node {
    pub fn as_str(&self) -> Option<&str> {
        match &self.value {
            Value::Str(s) => Some(s),
            Value::Block(_) => None,
        }
    }

    pub fn children(&self) -> &[Node] {
        match &self.value {
            Value::Block(b) => b,
            Value::Str(_) => &[],
        }
    }

    /// First child with this key, case-insensitive, like the engine's lookup.
    pub fn get(&self, key: &str) -> Option<&Node> {
        self.children()
            .iter()
            .find(|n| n.key.eq_ignore_ascii_case(key))
    }

    pub fn get_str(&self, key: &str) -> Option<&str> {
        self.get(key).and_then(Node::as_str)
    }

    pub fn get_all<'a>(&'a self, key: &'a str) -> impl Iterator<Item = &'a Node> + 'a {
        self.children()
            .iter()
            .filter(move |n| n.key.eq_ignore_ascii_case(key))
    }
}

impl Document {
    pub fn root(&self) -> Option<&Node> {
        self.nodes.first()
    }

    pub fn get(&self, key: &str) -> Option<&Node> {
        self.nodes.iter().find(|n| n.key.eq_ignore_ascii_case(key))
    }
}

#[derive(Debug, PartialEq)]
enum Tok<'a> {
    Str(&'a str),
    Open,
    Close,
    Cond(&'a str),
}

struct Lexer<'a> {
    src: &'a str,
    pos: usize,
}

impl<'a> Lexer<'a> {
    fn next(&mut self) -> Result<Option<Tok<'a>>> {
        let b = self.src.as_bytes();
        loop {
            while self.pos < b.len() && (b[self.pos] as char).is_ascii_whitespace() {
                self.pos += 1;
            }
            if self.pos >= b.len() {
                return Ok(None);
            }
            if b[self.pos] == b'/' && b.get(self.pos + 1) == Some(&b'/') {
                while self.pos < b.len() && b[self.pos] != b'\n' {
                    self.pos += 1;
                }
                continue;
            }
            break;
        }
        let c = b[self.pos];
        match c {
            b'{' => {
                self.pos += 1;
                Ok(Some(Tok::Open))
            }
            b'}' => {
                self.pos += 1;
                Ok(Some(Tok::Close))
            }
            b'"' => {
                let start = self.pos + 1;
                let mut i = start;
                while i < b.len() && b[i] != b'"' {
                    if b[i] == b'\\' {
                        i += 1;
                    }
                    i += 1;
                }
                if i >= b.len() {
                    return Err(FormatError::Malformed {
                        what: "KeyValues",
                        detail: format!("unterminated string at {start}"),
                    });
                }
                self.pos = i + 1;
                Ok(Some(Tok::Str(&self.src[start..i])))
            }
            b'[' => {
                let start = self.pos + 1;
                let end = self.src[start..]
                    .find(']')
                    .map(|e| start + e)
                    .ok_or_else(|| FormatError::Malformed {
                        what: "KeyValues",
                        detail: format!("unterminated condition at {start}"),
                    })?;
                self.pos = end + 1;
                Ok(Some(Tok::Cond(&self.src[start..end])))
            }
            _ => {
                let start = self.pos;
                let mut i = start;
                while i < b.len() {
                    let ch = b[i];
                    if (ch as char).is_ascii_whitespace()
                        || ch == b'{'
                        || ch == b'}'
                        || ch == b'"'
                        || ch == b'['
                    {
                        break;
                    }
                    if ch == b'/' && b.get(i + 1) == Some(&b'/') {
                        break;
                    }
                    i += 1;
                }
                self.pos = i;
                Ok(Some(Tok::Str(&self.src[start..i])))
            }
        }
    }
}

pub fn parse(src: &str) -> Result<Document> {
    let src = src.strip_prefix('\u{feff}').unwrap_or(src);
    let mut lx = Lexer { src, pos: 0 };
    let mut doc = Document::default();
    let mut stack: Vec<Vec<Node>> = vec![Vec::new()];
    let mut pending_key: Option<String> = None;
    let mut pending_cond: Option<String> = None;

    while let Some(tok) = lx.next()? {
        match tok {
            Tok::Str(s) => {
                if let Some(key) = pending_key.take() {
                    if key.eq_ignore_ascii_case("#include") || key.eq_ignore_ascii_case("#base") {
                        doc.includes.push((key.to_ascii_lowercase(), s.to_string()));
                    } else {
                        let node = Node {
                            key,
                            value: Value::Str(unescape(s)),
                            condition: pending_cond.take(),
                        };
                        stack.last_mut().expect("stack never empty").push(node);
                    }
                } else {
                    pending_key = Some(unescape(s));
                }
            }
            Tok::Cond(c) => {
                if pending_key.is_some() {
                    pending_cond = Some(c.to_string());
                } else if let Some(last) = stack.last_mut().and_then(|v| v.last_mut()) {
                    last.condition = Some(c.to_string());
                }
            }
            Tok::Open => {
                let key = pending_key.take().ok_or_else(|| FormatError::Malformed {
                    what: "KeyValues",
                    detail: format!("block without key at {}", lx.pos),
                })?;
                stack.last_mut().expect("stack never empty").push(Node {
                    key,
                    value: Value::Block(Vec::new()),
                    condition: pending_cond.take(),
                });
                stack.push(Vec::new());
            }
            Tok::Close => {
                let children = stack.pop().ok_or_else(|| FormatError::Malformed {
                    what: "KeyValues",
                    detail: format!("unbalanced '}}' at {}", lx.pos),
                })?;
                if stack.is_empty() {
                    return Err(FormatError::Malformed {
                        what: "KeyValues",
                        detail: format!("unbalanced '}}' at {}", lx.pos),
                    });
                }
                let parent = stack.last_mut().expect("checked non-empty");
                match parent.last_mut() {
                    Some(Node {
                        value: Value::Block(b),
                        ..
                    }) => *b = children,
                    _ => {
                        return Err(FormatError::Malformed {
                            what: "KeyValues",
                            detail: "block close without open".into(),
                        })
                    }
                }
            }
        }
    }
    if stack.len() != 1 {
        return Err(FormatError::Malformed {
            what: "KeyValues",
            detail: format!("{} unclosed block(s)", stack.len() - 1),
        });
    }
    if let Some(k) = pending_key {
        return Err(FormatError::Malformed {
            what: "KeyValues",
            detail: format!("dangling key {k:?}"),
        });
    }
    doc.nodes = stack.pop().unwrap_or_default();
    Ok(doc)
}

fn unescape(s: &str) -> String {
    if !s.contains('\\') {
        return s.to_string();
    }
    let mut out = String::with_capacity(s.len());
    let mut it = s.chars();
    while let Some(c) = it.next() {
        if c == '\\' {
            match it.next() {
                Some('n') => out.push('\n'),
                Some('t') => out.push('\t'),
                Some('"') => out.push('"'),
                Some('\\') => out.push('\\'),
                Some(o) => {
                    out.push('\\');
                    out.push(o);
                }
                None => out.push('\\'),
            }
        } else {
            out.push(c);
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    const GAMEINFO: &str = r#"
"GameInfo"
{
	game	"Garry's Mod"
	type	multiplayer_only   // comment
	FileSystem
	{
		SteamAppId				4000
		SearchPaths
		{
			game+mod			garrysmod/addons/*
			game				|all_source_engine_paths|sourceengine/hl2_textures.vpk
			game				|all_source_engine_paths|sourceengine/hl2_misc.vpk
			platform			|all_source_engine_paths|platform/platform_misc.vpk [$WIN32]
		}
	}
}
"#;

    #[test]
    fn parses_gameinfo_shape() {
        let doc = parse(GAMEINFO).unwrap();
        let root = doc.get("gameinfo").unwrap();
        assert_eq!(root.get_str("game"), Some("Garry's Mod"));
        assert_eq!(root.get_str("TYPE"), Some("multiplayer_only"));
        let fs = root.get("FileSystem").unwrap();
        assert_eq!(fs.get_str("SteamAppId"), Some("4000"));
        let sp = fs.get("SearchPaths").unwrap();
        assert_eq!(sp.children().len(), 4);
        assert_eq!(sp.get_all("game").count(), 2);
        assert_eq!(sp.children()[3].condition.as_deref(), Some("$WIN32"));
    }

    #[test]
    fn vmt_and_includes() {
        let doc = parse(
            r#"#base "shared.vmt"
"VertexLitGeneric" { "$basetexture" "models/props_c17/oildrum001" "$surfaceprop" metal "%keywords" "a,b" }"#,
        )
        .unwrap();
        assert_eq!(
            doc.includes,
            vec![("#base".to_string(), "shared.vmt".to_string())]
        );
        let m = doc.root().unwrap();
        assert_eq!(m.key, "VertexLitGeneric");
        assert_eq!(
            m.get_str("$BASETEXTURE"),
            Some("models/props_c17/oildrum001")
        );
        assert_eq!(m.get_str("$surfaceprop"), Some("metal"));
    }

    #[test]
    fn escapes_and_errors() {
        let doc = parse(r#""k" "a\"b\n""#).unwrap();
        assert_eq!(doc.nodes[0].as_str(), Some("a\"b\n"));
        assert!(parse("\"a\" {").is_err());
        assert!(parse("}").is_err());
        assert!(parse("\"lonely\"").is_err());
    }
}
