//! `scripts/sentences.txt`: the engine's sentence groups (`COMBINE_IDLE0..`, `V_MYNAMES0..`),
//! each a run of words from a directory with `(vNN)` / `(pNN)` volume / pitch modifiers; an
//! uppercase word naming another group splices a random sentence of it (`V_MYNAMES`).

use std::collections::HashMap;

/// One wave of a spoken sentence.
#[derive(Clone, Debug, PartialEq)]
pub struct Word {
    /// `sound/`-relative path (`npc/combine_soldier/vo/on1.wav`).
    pub path: String,
    /// Percent.
    pub pitch: f32,
    /// 0..1.
    pub volume: f32,
}

#[derive(Default, Debug)]
pub struct Sentences {
    /// Group name (the line name without its trailing digits) -> the lines' word runs.
    groups: HashMap<String, Vec<String>>,
}

impl Sentences {
    pub fn load(&mut self, src: &str) {
        for line in src.lines() {
            let line = line.split("//").next().unwrap_or("").trim();
            let Some((name, rest)) = line.split_once(char::is_whitespace) else {
                continue;
            };
            let rest = rest.split('{').next().unwrap_or("").trim();
            if rest.is_empty() {
                continue;
            }
            let group = name.trim_end_matches(|c: char| c.is_ascii_digit());
            self.groups
                .entry(group.to_string())
                .or_default()
                .push(rest.to_string());
        }
    }

    pub fn is_empty(&self) -> bool {
        self.groups.is_empty()
    }

    /// A random sentence of `group` (`!COMBINE_IDLE` without the `!`; a numbered name picks
    /// that line) as words to play back to back.
    pub fn pick(&self, name: &str, rng: &mut dyn FnMut() -> u64) -> Option<Vec<Word>> {
        let mut words = Vec::new();
        self.expand(name, "", 100.0, 100.0, rng, &mut words, 0)?;
        (!words.is_empty()).then_some(words)
    }

    fn line(&self, name: &str, rng: &mut dyn FnMut() -> u64) -> Option<&str> {
        let group = name.trim_end_matches(|c: char| c.is_ascii_digit());
        let lines = self.groups.get(group)?;
        if group.len() < name.len() {
            let i: usize = name[group.len()..].parse().ok()?;
            return lines.get(i).map(String::as_str);
        }
        lines
            .get((rng() % lines.len() as u64) as usize)
            .map(String::as_str)
    }

    #[allow(clippy::too_many_arguments)]
    fn expand(
        &self,
        name: &str,
        dir: &str,
        volume: f32,
        pitch: f32,
        rng: &mut dyn FnMut() -> u64,
        out: &mut Vec<Word>,
        depth: u32,
    ) -> Option<()> {
        let line = self.line(name, rng)?;
        let mut dir = dir.to_string();
        let (mut volume, mut pitch) = (volume, pitch);
        for token in line.split_whitespace() {
            // modifiers: `(v80)`, `(p110)`, `(e..)`, `(t..)`, `(s..)` - attached or alone
            let mut word = String::new();
            let mut rest = token;
            while let Some(i) = rest.find('(') {
                word.push_str(&rest[..i]);
                let Some(j) = rest[i..].find(')') else {
                    rest = "";
                    break;
                };
                let m = &rest[i + 1..i + j];
                let v: Option<f32> = m[1..].parse().ok();
                match (m.chars().next(), v) {
                    (Some('v'), Some(v)) => volume = v,
                    (Some('p'), Some(v)) => pitch = v,
                    _ => {}
                }
                rest = &rest[i + j + 1..];
            }
            word.push_str(rest);
            let word = word.trim_end_matches([',', '.', ';', ':', '!', '?']);
            if word.is_empty() {
                continue;
            }
            if let Some(d) = word.strip_suffix('/') {
                dir = format!("{d}/");
                continue;
            }
            // an uppercase word names another group (`V_MYNAMES`); one the file never
            // defines (`V_IDIEDS`) is silent, as in the engine
            if word.chars().next().is_some_and(|c| c.is_ascii_uppercase()) {
                let known = self
                    .groups
                    .contains_key(word.trim_end_matches(|c: char| c.is_ascii_digit()));
                if known && depth < 4 {
                    self.expand(word, &dir, volume, pitch, rng, out, depth + 1);
                }
                continue;
            }
            out.push(Word {
                path: format!("{dir}{word}.wav"),
                pitch,
                volume: (volume / 100.0).clamp(0.0, 1.0),
            });
        }
        Some(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn combine_sentence_expands_groups_and_modifiers() {
        let mut s = Sentences::default();
        s.load(
            "// HALF-LIFE 2\nV_MYNAMES0 leader {Len 0.4}\nV_MYNAMES1 echo {Len 0.4}\n\
             COMBINE_ANNOUNCE0 npc/combine_soldier/vo/(v100) on1 V_MYNAMES suppressing, off1(p120) {Len 0.96}\n",
        );
        let mut n = 0u64;
        let mut rng = || {
            n += 1;
            n
        };
        let w = s.pick("COMBINE_ANNOUNCE", &mut rng).unwrap();
        let paths: Vec<&str> = w.iter().map(|w| w.path.as_str()).collect();
        assert_eq!(
            paths,
            [
                "npc/combine_soldier/vo/on1.wav",
                "npc/combine_soldier/vo/leader.wav",
                "npc/combine_soldier/vo/suppressing.wav",
                "npc/combine_soldier/vo/off1.wav",
            ]
        );
        assert_eq!(w[3].pitch, 120.0);
        assert_eq!(w[0].volume, 1.0);
        assert!(s.pick("NOPE", &mut rng).is_none());
    }
}
