//! Regular expressions for plugins (`text.match`), in the dialect VTank metas and loot profiles
//! are written in.
//!
//! The dialect is the common one: classes, groups, named groups (`(?<name>…)`), alternation and
//! repetition. Two differences from this host's matcher are bridged: a backslash before `<` or
//! `>` is a plain angle bracket there (here it would be a word boundary), and a match may be asked
//! for without regard to case.

use std::collections::BTreeMap;

use dereth_plugin_api::Value;
use regex::{Regex, RegexBuilder};

/// The most patterns one plugin keeps compiled; the oldest is dropped past it.
pub const CACHE: usize = 256;

/// `pattern` in this host's syntax.
#[must_use]
pub fn translate(pattern: &str) -> String {
    let mut out = String::with_capacity(pattern.len());
    let mut chars = pattern.chars();
    while let Some(c) = chars.next() {
        if c == '\\' {
            match chars.next() {
                Some(n @ ('<' | '>')) => out.push(n),
                Some(n) => {
                    out.push('\\');
                    out.push(n);
                }
                None => out.push_str("\\\\"),
            }
        } else {
            out.push(c);
        }
    }
    out
}

/// A plugin's compiled patterns.
#[derive(Debug, Default)]
pub struct Patterns {
    compiled: BTreeMap<(String, bool), Regex>,
    order: std::collections::VecDeque<(String, bool)>,
}

impl Patterns {
    /// `pattern` compiled (from the cache when it has been before).
    ///
    /// # Errors
    /// The pattern does not compile.
    pub fn get(&mut self, pattern: &str, ignore_case: bool) -> Result<&Regex, String> {
        let key = (pattern.to_owned(), ignore_case);
        if !self.compiled.contains_key(&key) {
            let re = RegexBuilder::new(&translate(pattern))
                .case_insensitive(ignore_case)
                .size_limit(1 << 20)
                .build()
                .map_err(|e| format!("the pattern {pattern:?} does not compile: {e}"))?;
            if self.order.len() >= CACHE {
                if let Some(old) = self.order.pop_front() {
                    self.compiled.remove(&old);
                }
            }
            self.order.push_back(key.clone());
            self.compiled.insert(key.clone(), re);
        }
        Ok(&self.compiled[&key])
    }
}

/// The first match of `re` in `text`: `{ text, groups = { … }, named = { … } }`, or nil. A group
/// that took part in no match is an empty string, as the metas expect.
#[must_use]
pub fn first_match(re: &Regex, text: &str) -> Value {
    let Some(caps) = re.captures(text) else {
        return Value::Nil;
    };
    let whole = caps.get(0).map_or("", |m| m.as_str());
    let groups = (1..caps.len())
        .map(|i| Value::from(caps.get(i).map_or("", |m| m.as_str()).to_owned()))
        .collect();
    let mut named = Value::map();
    for name in re.capture_names().flatten() {
        named = named.with(name, caps.name(name).map_or("", |m| m.as_str()).to_owned());
    }
    Value::map()
        .with("text", whole.to_owned())
        .with("groups", Value::List(groups))
        .with("named", named)
}

#[cfg(test)]
mod tests {
    //! Behaviour: none (unit checks of the pattern dialect; `automation/met-import` pins its use).
    use super::{first_match, Patterns};

    #[test]
    fn the_metas_dialect_matches_with_named_groups_and_escaped_brackets() {
        let mut p = Patterns::default();
        let re = p
            .get(r#"^\[Fellowship\] .* (say|says)\, \"(?<nav>.*)\"$"#, false)
            .unwrap()
            .clone();
        let m = first_match(&re, "[Fellowship] Bob says, \"go north\"");
        assert_eq!(m.get("named").get("nav").as_str(), Some("go north"));
        assert_eq!(m.get("groups").as_list().unwrap()[0].as_str(), Some("says"));
        let re = p.get(r"\<Tell:(?<n>[^\<]*)\>", false).unwrap().clone();
        assert_eq!(
            first_match(&re, "<Tell:Bob>")
                .get("named")
                .get("n")
                .as_str(),
            Some("Bob")
        );
        let re = p.get("hello", true).unwrap().clone();
        assert!(!first_match(&re, "HELLO there").is_nil());
        assert!(p.get("(", false).is_err());
    }
}
