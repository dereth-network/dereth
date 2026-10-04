//! `cargo xtask comment-lint`: comments and messages describe behaviour.
//!
//! Source comments, string literals (log lines, panic and `expect` messages, error text) and TOML
//! comments must say what the code does. They must not carry the scaffolding the code was built
//! with: work-item labels (`unit O-41`, `R6.5b`, `RR-12`, `track G`, `plan 3`), pointers into
//! private working documents (`knowledge/`, `evidence/`, `SHARED-PROPOSALS`, `OPEN_QUESTIONS`),
//! records of who decided what (`owner decision`), or words about how the reference behaviour was
//! studied (`decompil...`, `ghidra`, `vtable`). A reader of the published source has none of those
//! documents, so a sentence that leans on one explains nothing.
//!
//! The scan is textual and deliberately narrow: every pattern is one that almost never occurs in
//! ordinary prose, and the number-and-letter label pattern skips the shapes that are real names
//! (function keys `F1`-`F24`, matrix elements `M11`-`M44`, numeric format specifiers such as `N0`
//! or `F6`, and four-digit opcode shorthand such as `F745`).
//!
//! ## Exceptions
//!
//! `tools/xtask/comment-lint-allowed.txt` lists the findings that are correct as written, one
//! row per `path|rule|matched text|reason`. A row covers every occurrence of that text under that
//! rule in that file. **The list may only shrink**: a row that no longer matches anything fails
//! the lint, so a fixed finding cannot leave a stale exception behind it.
//!
//! ## Test code
//!
//! Test code is `test-lint`'s, not this lint's: files under a `tests/` or `benches/` directory,
//! `#[cfg(test)]` items, and the files a `#[cfg(test)]` item pulls in with `mod name;` or
//! `include!`. The same scan runs over it there, as [`Scope::Test`], with the same rules plus one
//! for test commands that name a test target which no longer exists, and against that lint's own
//! allowlist. [`Scope`] is the one place that divides the two.

use std::collections::BTreeSet;
use std::path::{Path, PathBuf};

use crate::util::{print_table, workspace_root, Outcome, Report};

/// Which half of the workspace a scan reads.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Scope {
    /// Everything but test code: `comment-lint`.
    Production,
    /// Test code only: the comment half of `test-lint`.
    Test,
}

/// The rule only [`Scope::Test`] applies: a `cargo test .. --test <name>` whose target is not one
/// the workspace has. Test code is where such commands are written down, and a test file that
/// moved into a tier binary leaves them behind.
pub const STALE_TEST_COMMAND: &str = "stale test command";

/// The allowlist, beside this crate's manifest.
const ALLOWLIST: &str = "comment-lint-allowed.txt";

/// This file names every pattern it forbids, so it is the one source it cannot judge.
const SELF_PATH: &str = "tools/xtask/src/comment_lint.rs";

/// Where a piece of text came from.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Kind {
    Comment,
    Str,
}

/// One scannable piece of a source file: a comment line or one physical line of a string literal.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Segment {
    pub line: usize,
    pub kind: Kind,
    pub text: String,
    /// Inside a `#[cfg(test)]` item.
    pub test: bool,
    /// The segment is a whole string literal on one line (a format specifier like `"N0"` is
    /// judged by this).
    pub whole_literal: bool,
}

/// A file a `#[cfg(test)]` item brings in.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum TestDecl {
    /// `mod name;`, with the `#[path]` it carried if any.
    Mod { name: String, path: Option<String> },
    /// `include!("path")`.
    Include(String),
}

/// One rule violation.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Finding {
    pub path: String,
    pub line: usize,
    pub rule: &'static str,
    pub matched: String,
    pub context: String,
}

fn is_word(b: u8) -> bool {
    b.is_ascii_alphanumeric() || b == b'_'
}

/// Split Rust source into comment and string-literal segments, and collect the files that
/// `#[cfg(test)]` items declare.
#[allow(clippy::too_many_lines)]
pub fn rust_segments(src: &str) -> (Vec<Segment>, Vec<TestDecl>) {
    let c: Vec<char> = src.chars().collect();
    let n = c.len();
    let mut segs = Vec::new();
    let mut decls = Vec::new();
    let mut i = 0usize;
    let mut line = 1usize;
    let mut depth = 0i64;
    let mut test_depth: Option<i64> = None;
    let mut pending_test = false;
    let mut pending_path: Option<String> = None;
    let mut want_path_string = false;
    let mut want_include_string = false;
    let starts =
        |i: usize, s: &str| -> bool { (i..).zip(s.chars()).all(|(k, ch)| k < n && c[k] == ch) };
    let prev_is_word = |i: usize| i > 0 && c[i - 1].is_ascii() && is_word(c[i - 1] as u8);
    while i < n {
        let ch = c[i];
        let in_test = test_depth.is_some();
        // Line comments, including doc comments.
        if starts(i, "//") {
            let mut e = i;
            while e < n && c[e] != '\n' {
                e += 1;
            }
            segs.push(Segment {
                line,
                kind: Kind::Comment,
                text: c[i..e].iter().collect(),
                test: in_test,
                whole_literal: false,
            });
            i = e;
            continue;
        }
        // Block comments, which nest.
        if starts(i, "/*") {
            let mut nest = 0usize;
            let mut e = i;
            let mut cur = String::new();
            let mut cur_line = line;
            while e < n {
                if c[e] == '/' && e + 1 < n && c[e + 1] == '*' {
                    nest += 1;
                    cur.push_str("/*");
                    e += 2;
                    continue;
                }
                if c[e] == '*' && e + 1 < n && c[e + 1] == '/' {
                    nest -= 1;
                    cur.push_str("*/");
                    e += 2;
                    if nest == 0 {
                        break;
                    }
                    continue;
                }
                if c[e] == '\n' {
                    segs.push(Segment {
                        line: cur_line,
                        kind: Kind::Comment,
                        text: std::mem::take(&mut cur),
                        test: in_test,
                        whole_literal: false,
                    });
                    line += 1;
                    cur_line = line;
                } else {
                    cur.push(c[e]);
                }
                e += 1;
            }
            segs.push(Segment {
                line: cur_line,
                kind: Kind::Comment,
                text: cur,
                test: in_test,
                whole_literal: false,
            });
            i = e;
            continue;
        }
        // String literals: "..", b"..", c"..", r"..", r#".."#, br#".."#.
        let mut raw_hashes: Option<usize> = None;
        let mut body_start = None;
        if !prev_is_word(i) {
            let mut k = i;
            if k < n
                && (c[k] == 'b' || c[k] == 'c')
                && k + 1 < n
                && (c[k + 1] == '"' || c[k + 1] == 'r')
            {
                k += 1;
            }
            if k < n && c[k] == 'r' {
                let mut h = k + 1;
                while h < n && c[h] == '#' {
                    h += 1;
                }
                if h < n && c[h] == '"' {
                    raw_hashes = Some(h - k - 1);
                    body_start = Some(h + 1);
                }
            } else if k < n && c[k] == '"' {
                body_start = Some(k + 1);
            }
        }
        if let Some(start) = body_start {
            let mut e = start;
            match raw_hashes {
                Some(h) => loop {
                    if e >= n {
                        break;
                    }
                    if c[e] == '"' && (0..h).all(|j| e + 1 + j < n && c[e + 1 + j] == '#') {
                        break;
                    }
                    e += 1;
                },
                None => {
                    while e < n && c[e] != '"' {
                        if c[e] == '\\' {
                            e += 1;
                        }
                        e += 1;
                    }
                }
            }
            let e = e.min(n);
            let body: String = c[start..e].iter().collect();
            if want_path_string {
                pending_path = Some(body.clone());
                want_path_string = false;
            }
            if want_include_string {
                if in_test || pending_test {
                    decls.push(TestDecl::Include(body.clone()));
                }
                want_include_string = false;
            }
            // `\n` and `\t` escapes read as spaces, so a word after one keeps its boundary.
            let readable = if raw_hashes.is_some() {
                body.clone()
            } else {
                body.replace("\\n", "  ").replace("\\t", "  ")
            };
            let physical: Vec<&str> = readable.split('\n').collect();
            let whole = physical.len() == 1;
            for (k, part) in physical.iter().enumerate() {
                segs.push(Segment {
                    line: line + k,
                    kind: Kind::Str,
                    text: (*part).to_owned(),
                    test: in_test,
                    whole_literal: whole,
                });
            }
            line += body.matches('\n').count();
            i = e + 1 + raw_hashes.unwrap_or(0);
            continue;
        }
        // Character literals versus lifetimes and labels.
        if ch == '\'' {
            if i + 1 < n && c[i + 1] == '\\' {
                let mut e = i + 2;
                while e < n && c[e] != '\'' && c[e] != '\n' {
                    e += 1;
                }
                i = e + 1;
                continue;
            }
            if i + 2 < n && c[i + 2] == '\'' {
                if c[i + 1] == '\n' {
                    line += 1;
                }
                i += 3;
                continue;
            }
            i += 1;
            continue;
        }
        // Attributes that decide what counts as test code.
        if ch == '#' {
            let rest: String = c[i..(i + 20).min(n)]
                .iter()
                .filter(|x| !x.is_whitespace())
                .collect();
            if rest.starts_with("#[cfg(test)]") || rest.starts_with("#[cfg(all(test") {
                pending_test = true;
            } else if rest.starts_with("#[path=") {
                want_path_string = true;
            }
        }
        // `mod name;` and `include!(` in test context.
        if ch.is_ascii_alphabetic() && !prev_is_word(i) {
            let mut e = i;
            while e < n && c[e].is_ascii() && is_word(c[e] as u8) {
                e += 1;
            }
            let word: String = c[i..e].iter().collect();
            if word == "mod" && (pending_test || in_test) {
                let mut k = e;
                while k < n && c[k].is_whitespace() {
                    k += 1;
                }
                let s = k;
                while k < n && c[k].is_ascii() && is_word(c[k] as u8) {
                    k += 1;
                }
                let name: String = c[s..k].iter().collect();
                while k < n && c[k].is_whitespace() {
                    k += 1;
                }
                if k < n && c[k] == ';' && !name.is_empty() {
                    decls.push(TestDecl::Mod {
                        name,
                        path: pending_path.take(),
                    });
                }
            } else if word == "include" && e < n && c[e] == '!' {
                want_include_string = true;
            }
            i = e;
            continue;
        }
        match ch {
            '{' => {
                depth += 1;
                if pending_test && test_depth.is_none() {
                    test_depth = Some(depth);
                }
                pending_test = false;
                pending_path = None;
            }
            '}' => {
                if test_depth == Some(depth) {
                    test_depth = None;
                }
                depth -= 1;
                pending_test = false;
            }
            ';' => {
                pending_test = false;
                pending_path = None;
            }
            '\n' => line += 1,
            _ => {}
        }
        i += 1;
    }
    (segs, decls)
}

/// The comments in a TOML file, one per line.
pub fn toml_segments(src: &str) -> Vec<Segment> {
    let mut out = Vec::new();
    for (k, l) in src.lines().enumerate() {
        let b = l.as_bytes();
        let mut quote: Option<u8> = None;
        let mut j = 0;
        while j < b.len() {
            let x = b[j];
            match quote {
                Some(q) => {
                    if q == b'"' && x == b'\\' {
                        j += 1;
                    } else if x == q {
                        quote = None;
                    }
                }
                None => {
                    if x == b'"' || x == b'\'' {
                        quote = Some(x);
                    } else if x == b'#' {
                        out.push(Segment {
                            line: k + 1,
                            kind: Kind::Comment,
                            text: l[j..].to_owned(),
                            test: false,
                            whole_literal: false,
                        });
                        break;
                    }
                }
            }
            j += 1;
        }
    }
    out
}

fn boundary_before(b: &[u8], i: usize) -> bool {
    i == 0 || !is_word(b[i - 1])
}

fn boundary_after(b: &[u8], i: usize) -> bool {
    i >= b.len() || !is_word(b[i])
}

fn digits(b: &[u8], mut i: usize) -> usize {
    while i < b.len() && b[i].is_ascii_digit() {
        i += 1;
    }
    i
}

/// Every `(start, end)` where `needle` occurs, ASCII case-insensitively when `fold` is set.
fn occurrences(hay: &str, needle: &str, fold: bool) -> Vec<(usize, usize)> {
    let h = hay.as_bytes();
    let nd = needle.as_bytes();
    let mut out = Vec::new();
    if nd.len() > h.len() {
        return out;
    }
    for i in 0..=h.len() - nd.len() {
        let hit = if fold {
            h[i..i + nd.len()].eq_ignore_ascii_case(nd)
        } else {
            &h[i..i + nd.len()] == nd
        };
        if hit {
            out.push((i, i + nd.len()));
        }
    }
    out
}

/// The letters a work-item label starts with.
const LABEL_LETTERS: &[u8] = b"OPRCDFGNLSMIZBE";

/// A work-item label such as `O-41`, `R6.5b`, `E14` or `C24`, or `None` for a real name.
fn label_at(b: &[u8], i: usize, whole_literal: bool) -> Option<usize> {
    if !LABEL_LETTERS.contains(&b[i]) || !boundary_before(b, i) {
        return None;
    }
    let letter = b[i];
    let mut k = i + 1;
    let dashed = k < b.len() && b[k] == b'-';
    if dashed {
        k += 1;
    }
    let ds = k;
    k = digits(b, k);
    if k == ds {
        return None;
    }
    let number = &b[ds..k];
    let mut dotted = false;
    while k + 1 < b.len() && b[k] == b'.' && b[k + 1].is_ascii_digit() {
        k = digits(b, k + 1);
        dotted = true;
        if k < b.len() && b[k].is_ascii_lowercase() {
            k += 1;
        }
    }
    if !dotted && k < b.len() && b[k].is_ascii_lowercase() {
        // `R6b` style: one lowercase suffix letter.
        k += 1;
    }
    if !boundary_after(b, k) {
        return None;
    }
    if dashed || dotted {
        return Some(k);
    }
    let value: u32 = std::str::from_utf8(number).ok()?.parse().ok()?;
    // Function keys.
    if letter == b'F' && (1..=24).contains(&value) && number[0] != b'0' {
        return None;
    }
    // Opcode shorthand: `F745` for `0xF745`.
    if letter == b'F' && number.len() >= 3 {
        return None;
    }
    // Matrix elements `M11`..`M44`.
    if letter == b'M' && number.len() == 2 && number.iter().all(|d| (b'1'..=b'4').contains(d)) {
        return None;
    }
    // Compiler error codes `E0308`, and the `E0` scan-code prefix.
    if letter == b'E' && number[0] == b'0' && (number.len() == 1 || number.len() == 4) {
        return None;
    }
    // Depth formats `D16`/`D24`/`D32` and integer widths `I32`/`I64`.
    if letter == b'D' && matches!(number, b"16" | b"24" | b"32") {
        return None;
    }
    if letter == b'I' && matches!(number, b"8" | b"16" | b"32" | b"64") {
        return None;
    }
    // The end of a character range such as `a-zA-Z0-9`.
    if i >= 2 && b[i - 1] == b'-' && b[i - 2].is_ascii_alphabetic() {
        return None;
    }
    // A numeric format specifier: the whole literal, a quoted `"N0"` in prose, or after the `:`
    // of a format argument.
    let quoted = i > 0 && b[i - 1] == b'"' && k < b.len() && b[k] == b'"';
    if number.len() <= 2
        && (whole_literal && i == 0 && k == b.len() || quoted || i > 0 && b[i - 1] == b':')
    {
        return None;
    }
    Some(k)
}

/// The rules, in report order.
pub const RULES: &[&str] = &[
    "work-item label",
    "unit label",
    "review-row label",
    "track label",
    "plan label",
    "decision record",
    "private document",
    "study method",
    "private provenance",
    "stale source citation",
];

fn provenance_pattern() -> &'static regex::Regex {
    static PATTERN: std::sync::OnceLock<regex::Regex> = std::sync::OnceLock::new();
    PATTERN.get_or_init(|| regex::Regex::new(
        r"(?i)\btrap[ -]+\d+\b|\bcontract[ -]+\d+\.\d+\b|\\?\[(?:verified|inferred)(?:[ :][^\]\r\n]*)?\\?\]|\b(?:[ophr]\d+[a-z]?(?:_\d+[a-z]?)*|astra|fable|rule3)[_-][a-z0-9_-]+\b|\b(?:dere|dereth|emp|serv|ui)-[ophr]\d+(?:[_-]\w+)*\b"
    ).expect("provenance rule"))
}

/// Every violation in one piece of text: `(rule, start, end)`.
pub fn scan_text(text: &str, whole_literal: bool) -> Vec<(&'static str, usize, usize)> {
    let b = text.as_bytes();
    let mut out = Vec::new();
    out.extend(
        provenance_pattern()
            .find_iter(text)
            .filter(|m| {
                !matches!(
                    m.as_str(),
                    "p50_ms" | "p99_ms" | "p99_tick" | "p99_floor_ms"
                )
            })
            .map(|m| ("private provenance", m.start(), m.end())),
    );
    for i in 0..b.len() {
        if let Some(e) = label_at(b, i, whole_literal) {
            out.push(("work-item label", i, e));
        }
    }
    for (s, e) in occurrences(text, "unit ", true) {
        if !boundary_before(b, s) {
            continue;
        }
        let mut k = e;
        if k < b.len() && b[k].is_ascii_uppercase() {
            k += 1;
            if k < b.len() && b[k] == b'-' {
                k += 1;
            }
        }
        if k < b.len() && b[k].is_ascii_digit() {
            out.push(("unit label", s, digits(b, k)));
        }
    }
    for (s, e) in occurrences(text, "RR-", false) {
        if boundary_before(b, s) && e < b.len() && b[e].is_ascii_digit() {
            out.push(("review-row label", s, digits(b, e)));
        }
    }
    for (s, e) in occurrences(text, "track ", true) {
        if boundary_before(b, s)
            && e < b.len()
            && b[e].is_ascii_uppercase()
            && boundary_after(b, e + 1)
        {
            out.push(("track label", s, e + 1));
        }
    }
    for (s, e) in occurrences(text, "plan ", true) {
        if boundary_before(b, s) && e < b.len() && b[e].is_ascii_digit() {
            out.push(("plan label", s, digits(b, e)));
        }
    }
    for needle in ["owner decision", "owner ruling"] {
        for (s, e) in occurrences(text, needle, true) {
            if boundary_before(b, s) {
                out.push(("decision record", s, e));
            }
        }
    }
    for needle in [
        "knowledge/",
        "evidence/",
        "SHARED-PROPOSALS",
        "OPEN_QUESTIONS",
    ] {
        for (s, e) in occurrences(text, needle, false) {
            out.push(("private document", s, e));
        }
    }
    for needle in ["decompil", "ghidra", "vtable"] {
        for (s, e) in occurrences(text, needle, true) {
            out.push(("study method", s, e));
        }
    }
    // An evidence handle (`AC-EVID-O410-HEIGHT`) is an opaque key into the evidence index, not
    // prose, and the work-item label inside it is part of the key.
    let handles: Vec<(usize, usize)> = occurrences(text, "AC-EVID-", false)
        .into_iter()
        .map(|(s, mut e)| {
            while e < b.len() && (is_word(b[e]) || b[e] == b'-') {
                e += 1;
            }
            (s, e)
        })
        .collect();
    out.retain(|&(_, s, e)| !handles.iter().any(|&(hs, he)| hs <= s && e <= he));
    out.sort_by_key(|&(r, s, _)| (s, r));
    out
}

/// One allowlist row.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Allowed {
    pub path: String,
    pub rule: String,
    pub matched: String,
    pub row: usize,
}

/// Parse the allowlist: `path|rule|matched text|reason`, `#` starts a comment line. A row without
/// a reason is an error, returned as the row number.
pub fn parse_allowlist(text: &str) -> Result<Vec<Allowed>, String> {
    let mut out = Vec::new();
    for (k, l) in text.lines().enumerate() {
        let l = l.trim();
        if l.is_empty() || l.starts_with('#') {
            continue;
        }
        let parts: Vec<&str> = l.splitn(4, '|').map(str::trim).collect();
        if parts.len() != 4 || parts.iter().any(|p| p.is_empty()) {
            return Err(format!(
                "{ALLOWLIST} row {}: expected `path|rule|matched text|reason`",
                k + 1
            ));
        }
        if !RULES.contains(&parts[1]) {
            return Err(format!(
                "{ALLOWLIST} row {}: `{}` is not a rule",
                k + 1,
                parts[1]
            ));
        }
        out.push(Allowed {
            path: parts[0].to_owned(),
            rule: parts[1].to_owned(),
            matched: parts[2].to_owned(),
            row: k + 1,
        });
    }
    Ok(out)
}

/// Is this workspace-relative path test code by where it sits?
pub fn is_test_path(rel: &str) -> bool {
    rel.split('/')
        .rev()
        .skip(1)
        .any(|d| d == "tests" || d == "benches")
}

/// The files that `decls` in `file` point at.
fn resolve_decls(file: &Path, decls: &[TestDecl]) -> Vec<PathBuf> {
    let dir = file.parent().unwrap_or(Path::new("."));
    let stem = file.file_stem().and_then(|s| s.to_str()).unwrap_or("");
    let owns_dir = matches!(stem, "lib" | "main" | "mod");
    let mod_dir = if owns_dir {
        dir.to_path_buf()
    } else {
        dir.join(stem)
    };
    let mut out = Vec::new();
    for d in decls {
        match d {
            TestDecl::Mod {
                path: Some(p),
                name: _,
            } => out.push(dir.join(p)),
            TestDecl::Mod { name, path: None } => {
                out.push(mod_dir.join(format!("{name}.rs")));
                out.push(mod_dir.join(name).join("mod.rs"));
            }
            TestDecl::Include(p) => out.push(dir.join(p)),
        }
    }
    out
}

fn walk(dir: &Path, out: &mut Vec<PathBuf>) {
    let Ok(entries) = std::fs::read_dir(dir) else {
        return;
    };
    for e in entries.flatten() {
        let p = e.path();
        let name = p.file_name().and_then(|s| s.to_str()).unwrap_or("");
        if p.is_dir() {
            if name == "target" || name.starts_with('.') {
                continue;
            }
            walk(&p, out);
        } else if name.ends_with(".rs") || name.ends_with(".toml") {
            out.push(p);
        }
    }
}

fn normalise(p: &Path) -> String {
    let mut parts: Vec<String> = Vec::new();
    for c in p.components() {
        match c {
            std::path::Component::ParentDir => {
                parts.pop();
            }
            std::path::Component::CurDir => {}
            other => parts.push(other.as_os_str().to_string_lossy().into_owned()),
        }
    }
    parts.join("/")
}

/// Every `cargo test .. --test <name>` in `text` whose `<name>` is not in `targets`: `(start, end)`
/// of the name.
pub fn stale_test_commands(text: &str, targets: &BTreeSet<String>) -> Vec<(usize, usize)> {
    let b = text.as_bytes();
    let mut out = Vec::new();
    for (s, _) in occurrences(text, "cargo test", false) {
        let rest = &text[s..];
        let mut from = 0;
        while let Some(at) = rest[from..].find("--test") {
            let k = s + from + at + "--test".len();
            from += at + "--test".len();
            if k < b.len() && (is_word(b[k]) || b[k] == b'-') {
                // `--tests` or `--test-threads`, not `--test`.
                continue;
            }
            let mut n = k;
            while n < b.len() && (b[n] == b' ' || b[n] == b'=') {
                n += 1;
            }
            let e = n + b[n..]
                .iter()
                .take_while(|&&x| is_word(x) || x == b'-')
                .count();
            if e > n && !targets.contains(&text[n..e]) {
                out.push((n, e));
            }
        }
    }
    out
}

fn stale_source_citations(root: &Path, text: &str) -> Vec<(usize, usize)> {
    static PATTERN: std::sync::OnceLock<regex::Regex> = std::sync::OnceLock::new();
    let pattern = PATTERN.get_or_init(|| regex::Regex::new(
        r"`((?:(?:dereth/)?(?:core|empyrean|tools)/|(?:dereth/)?dereth/|dereth/(?:client|headless|testkit|web)/)[\w./-]+\.rs)(?::\d+)?`"
    ).expect("source citation rule"));
    pattern
        .captures_iter(text)
        .filter_map(|captures| {
            let path = captures.get(1)?;
            let name = path.as_str();
            let safe = !name.split('/').any(|part| part == "..");
            let exists = safe
                && (root.join(name).is_file()
                    || name
                        .strip_prefix("dereth/")
                        .is_some_and(|p| root.join(p).is_file()));
            (!exists).then_some((path.start(), path.end()))
        })
        .collect()
}

/// Scan the workspace at `root`; every finding, before the allowlist. `targets` is the set of
/// test target names a [`Scope::Test`] scan accepts after `--test`.
pub fn scan_workspace(
    root: &Path,
    scope: Scope,
    targets: &BTreeSet<String>,
) -> (Vec<Finding>, usize) {
    let mut files = Vec::new();
    walk(root, &mut files);
    files.sort();
    let rel = |p: &Path| normalise(p.strip_prefix(root).unwrap_or(p));

    // Pass one: split every Rust file and learn which files test items pull in.
    let mut parsed = Vec::new();
    let mut test_files: BTreeSet<String> = BTreeSet::new();
    for p in &files {
        let Ok(text) = std::fs::read_to_string(p) else {
            continue;
        };
        let r = rel(p);
        if r.ends_with(".toml") {
            parsed.push((r, toml_segments(&text), Vec::new()));
        } else {
            let (segs, decls) = rust_segments(&text);
            for t in resolve_decls(p, &decls) {
                test_files.insert(rel(&t));
            }
            parsed.push((r, segs, decls));
        }
    }
    // A file a test file declares is test code too.
    loop {
        let before = test_files.len();
        for (r, _, decls) in &parsed {
            if test_files.contains(r) || is_test_path(r) {
                for t in resolve_decls(&root.join(r), decls) {
                    test_files.insert(rel(&t));
                }
            }
        }
        if test_files.len() == before {
            break;
        }
    }

    let mut findings = Vec::new();
    let mut scanned = 0usize;
    for (r, segs, _) in &parsed {
        if r == SELF_PATH {
            continue;
        }
        let test_file = is_test_path(r) || test_files.contains(r);
        if scope == Scope::Production && test_file {
            continue;
        }
        let mut any = false;
        for s in segs {
            let test_code = test_file || s.test;
            if test_code != (scope == Scope::Test) {
                continue;
            }
            any = true;
            let mut hits = scan_text(&s.text, s.whole_literal);
            hits.extend(
                stale_source_citations(root, &s.text)
                    .into_iter()
                    .map(|(a, b)| ("stale source citation", a, b)),
            );
            if scope == Scope::Test {
                hits.extend(
                    stale_test_commands(&s.text, targets)
                        .into_iter()
                        .map(|(a, b)| (STALE_TEST_COMMAND, a, b)),
                );
            }
            for (rule, a, b) in hits {
                findings.push(Finding {
                    path: r.clone(),
                    line: s.line,
                    rule,
                    matched: s.text[a..b].to_owned(),
                    context: s.text.trim().chars().take(160).collect(),
                });
            }
        }
        if any || scope == Scope::Production {
            scanned += 1;
        }
    }
    (findings, scanned)
}

/// Split findings into those the allowlist excuses and those it does not, and name the rows that
/// excuse nothing.
pub fn apply_allowlist<'a>(
    findings: &'a [Finding],
    allowed: &'a [Allowed],
) -> (Vec<&'a Finding>, Vec<&'a Allowed>) {
    let covers =
        |a: &Allowed, f: &Finding| a.path == f.path && a.rule == f.rule && a.matched == f.matched;
    let live = findings
        .iter()
        .filter(|f| !allowed.iter().any(|a| covers(a, f)))
        .collect();
    let stale = allowed
        .iter()
        .filter(|a| !findings.iter().any(|f| covers(a, f)))
        .collect();
    (live, stale)
}

pub fn comment_lint() -> i32 {
    let root = workspace_root();
    let list_path = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join(ALLOWLIST);
    let allowed = match std::fs::read_to_string(&list_path)
        .map_err(|e| format!("{}: {e}", list_path.display()))
        .and_then(|t| parse_allowlist(&t))
    {
        Ok(a) => a,
        Err(e) => {
            eprintln!("comment-lint: {e}");
            return 2;
        }
    };
    let (findings, scanned) = scan_workspace(&root, Scope::Production, &BTreeSet::new());
    let (live, stale) = apply_allowlist(&findings, &allowed);
    for f in &live {
        println!(
            "{}:{}: {} `{}`\n    {}",
            f.path, f.line, f.rule, f.matched, f.context
        );
    }
    for a in &stale {
        println!(
            "{ALLOWLIST} row {}: `{}` under `{}` in {} matches nothing; delete the row",
            a.row, a.matched, a.rule, a.path
        );
    }
    let reports = vec![
        Report::new(
            "comments and messages describe behaviour",
            if live.is_empty() {
                Outcome::Pass
            } else {
                Outcome::Fail
            },
            format!(
                "{} finding(s) across {scanned} file(s); {} excused by {} allowlist row(s)",
                live.len(),
                findings.len() - live.len(),
                allowed.len()
            ),
        ),
        Report::new(
            "allowlist only shrinks",
            if stale.is_empty() {
                Outcome::Pass
            } else {
                Outcome::Fail
            },
            format!("{} row(s) that match nothing", stale.len()),
        ),
    ];
    print_table("xtask comment-lint", &reports)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn rules(text: &str) -> Vec<(&'static str, String)> {
        scan_text(text, false)
            .into_iter()
            .map(|(r, a, b)| (r, text[a..b].to_owned()))
            .collect()
    }

    fn flagged(text: &str) -> bool {
        !scan_text(text, false).is_empty()
    }

    #[test]
    fn planted_labels_are_found() {
        for (text, rule, m) in [
            (
                "// placed before unit O-41 landed",
                "work-item label",
                "O-41",
            ),
            ("see R6.5b for the framing", "work-item label", "R6.5b"),
            ("the E14 trace fixtures", "work-item label", "E14"),
            ("C24: landblock unloaded", "work-item label", "C24"),
            ("per P1.82 the sync", "work-item label", "P1.82"),
            ("Found by unit 12 last week", "unit label", "unit 12"),
            ("Unit N12 owns it", "unit label", "Unit N12"),
            ("as RR-94 asked", "review-row label", "RR-94"),
            ("per track G spec", "track label", "track G"),
            ("Track N has not landed", "track label", "Track N"),
            ("this is plan 3's job", "plan label", "plan 3"),
            ("(owner decision D7)", "decision record", "owner decision"),
            ("see knowledge/ first", "private document", "knowledge/"),
            ("the evidence/unit report", "private document", "evidence/"),
            (
                "SHARED-PROPOSALS item",
                "private document",
                "SHARED-PROPOSALS",
            ),
            ("OPEN_QUESTIONS #127", "private document", "OPEN_QUESTIONS"),
            ("read off the Decompiled source", "study method", "Decompil"),
            ("the Ghidra output", "study method", "Ghidra"),
            ("slot 7 of the vtable", "study method", "vtable"),
            (
                "AC-EVID-O410 and then O-41 again",
                "work-item label",
                "O-41",
            ),
        ] {
            let got = rules(text);
            assert!(
                got.iter().any(|(r, s)| *r == rule && s == m),
                "{text:?}: expected {rule} `{m}`, got {got:?}"
            );
        }
    }

    #[test]
    fn real_names_are_not_labels() {
        for text in [
            "press F1 for help, F12 for the console, F24 exists too",
            "a D24S8 depth buffer",
            "the M21 and M44 elements of the matrix",
            "opcode F745 and F7B0",
            "an R8G8B8A8 texture in D3D12",
            "the unit sphere of radius 1",
            "the track the sound plays on",
            "a plan for the frame",
            "the object's owner decides",
            "our own knowledge of the world",
            "0x1000_05EE",
            "Rust 2021",
            "a trap catches creatures; the contract has 12 entries",
            "the signature was verified and the type inferred",
            "p50_ms p99_ms p99_tick p99_floor_ms",
            "evidence AC-EVID-P1-129-COLOURS and AC-EVID-RR78-BOX",
            "fails with E0308 or E0599; the E0-prefixed scan codes",
            "a D32 or D16 depth buffer; %I64d",
            "the class [a-zA-Z0-9_] and ([A-Z1-9])",
            "ToString(\"N0\") and ToString(\"F99\")",
            "client divergence CD-001 and server divergence V336",
        ] {
            assert!(!flagged(text), "{text:?} flagged as {:?}", rules(text));
        }
    }

    #[test]
    fn provenance_markers_are_rejected_in_comments_and_strings() {
        for text in [
            "trap 37",
            "Trap-8",
            "Contract 10.5",
            "[VERIFIED]",
            "[inferred]",
            "[verified against a recording]",
            "[inferred: field meaning]",
            r"\[VERIFIED\]",
            "p1_82_window_modes",
            "o74_swap_census",
            "rule3_packet",
            "dereth-o246",
        ] {
            assert!(
                rules(text)
                    .iter()
                    .any(|(rule, _)| *rule == "private provenance"),
                "{text}"
            );
        }
        for text in [
            "contract 7",
            "a verified checksum",
            "trap handler",
            "rule30_packet",
            "AC-EVID-P1_82_WINDOW",
            "F7",
            "M11",
            "0xF745",
        ] {
            assert!(!flagged(text), "{text}");
        }
    }

    #[test]
    fn rooted_source_citations_fail_when_the_file_is_removed() {
        let root = std::env::temp_dir().join("xtask-comment-source-citation");
        let file = root.join("core/sample/tests/cpu.rs");
        std::fs::create_dir_all(file.parent().expect("parent")).expect("fixture directory");
        std::fs::write(&file, "").expect("fixture source");
        let cited = "core/sample/tests/cpu.rs";
        let text = format!("`{cited}` and `dereth/{cited}:4`");
        assert!(stale_source_citations(&root, &text).is_empty());
        let examples = "`step.rs` `core/sample/*.rs` `https://example.org/core/no.rs`";
        assert!(stale_source_citations(&root, examples).is_empty());
        std::fs::remove_file(&file).expect("remove cited source");
        assert_eq!(stale_source_citations(&root, &text).len(), 2);
        std::fs::remove_dir_all(&root).expect("remove fixture");
    }

    #[test]
    fn format_specifiers_pass_only_as_specifiers() {
        assert!(scan_text("N0", true).is_empty());
        assert!(scan_text("F6", true).is_empty());
        assert!(scan_text("{0:N0} items", false).is_empty());
        assert!(scan_text("{value:G4}", false).is_empty());
        // The same letters in a sentence are a label.
        assert!(!scan_text("guids reused while held (S6)", false).is_empty());
        assert!(!scan_text("N0 of the plan", true).is_empty());
    }

    #[test]
    fn segments_separate_comments_strings_and_test_code() {
        let src = r####"
// a comment O-1
fn f() -> &'static str {
    let c = '"';
    let d = '\'';
    let _l: &'static str = "a string R6.5b";
    let raw = r#"raw "quoted" E14"#;
    /* block
       C24 */
    "x"
}
#[cfg(test)]
mod tests {
    // test comment O-2
    fn g() { let _ = "test string O-3"; }
}
fn after() { let _ = "after O-4"; }
"####;
        let (segs, _) = rust_segments(src);
        let find = |needle: &str| {
            segs.iter()
                .find(|s| s.text.contains(needle))
                .unwrap_or_else(|| panic!("no segment with {needle}"))
                .clone()
        };
        assert_eq!(find("O-1").kind, Kind::Comment);
        assert_eq!(find("O-1").line, 2);
        assert_eq!(find("R6.5b").kind, Kind::Str);
        assert_eq!(find("R6.5b").line, 6);
        assert_eq!(find("E14").kind, Kind::Str);
        assert_eq!(find("C24").line, 9);
        assert!(!find("C24").test);
        assert!(find("O-2").test);
        assert!(find("O-3").test);
        assert!(!find("O-4").test);
        assert_eq!(find("O-4").line, 17);
    }

    #[test]
    fn out_of_line_test_modules_are_found() {
        let src = "#[cfg(test)]\nmod tests;\n#[cfg(test)]\n#[path = \"x_tests.rs\"]\nmod other;\nmod real;\n#[cfg(test)]\nmod inline {\n    include!(\"y_tests.rs\");\n}\n";
        let (_, decls) = rust_segments(src);
        assert_eq!(
            decls,
            vec![
                TestDecl::Mod {
                    name: "tests".into(),
                    path: None
                },
                TestDecl::Mod {
                    name: "other".into(),
                    path: Some("x_tests.rs".into())
                },
                TestDecl::Include("y_tests.rs".into()),
            ]
        );
        let files = resolve_decls(Path::new("src/render.rs"), &decls);
        let files: Vec<String> = files.iter().map(|p| normalise(p)).collect();
        assert!(files.contains(&"src/render/tests.rs".to_owned()));
        assert!(files.contains(&"src/x_tests.rs".to_owned()));
        assert!(files.contains(&"src/y_tests.rs".to_owned()));
    }

    #[test]
    fn a_cfg_test_field_does_not_make_the_next_item_test_code() {
        let src =
            "struct S {\n    #[cfg(test)]\n    x: u8,\n}\nimpl S { fn f() { let _ = \"O-9\"; } }\n";
        let (segs, _) = rust_segments(src);
        let s = segs
            .iter()
            .find(|s| s.text.contains("O-9"))
            .expect("segment");
        assert!(!s.test);
    }

    #[test]
    fn toml_comments_skip_quoted_hashes() {
        let segs = toml_segments("a = \"#not a comment\" # unit O-5\n# plan 2\nb = 1\n");
        assert_eq!(segs.len(), 2);
        assert!(segs[0].text.contains("O-5"));
        assert_eq!(segs[1].line, 2);
    }

    #[test]
    fn a_test_command_naming_a_retired_target_is_stale() {
        let targets: BTreeSet<String> = ["cpu", "dat", "gpu"]
            .iter()
            .map(|s| (*s).to_owned())
            .collect();
        let text = "cargo test -p dereth-ui --test p1_164_cast_button -- --test-threads=1";
        let got: Vec<&str> = stale_test_commands(text, &targets)
            .into_iter()
            .map(|(a, b)| &text[a..b])
            .collect();
        assert_eq!(got, vec!["p1_164_cast_button"]);
        assert!(stale_test_commands("cargo test -p x --test gpu rendering::", &targets).is_empty());
        assert!(stale_test_commands("cargo test --tests", &targets).is_empty());
        assert!(stale_test_commands("run with --test o12 by hand", &targets).is_empty());
    }

    #[test]
    fn paths_identify_test_code() {
        assert!(is_test_path("core/physics/tests/cpu/trace.rs"));
        assert!(is_test_path("tools/pcap/benches/b.rs"));
        assert!(!is_test_path("core/physics/src/trace.rs"));
        assert!(!is_test_path("tests.rs"));
    }

    #[test]
    fn the_allowlist_only_shrinks() {
        let allowed = parse_allowlist(
            "# comment\na.rs|study method|vtable|the COM interface table\nb.rs|plan label|plan 2|gone\n",
        )
        .expect("parses");
        let findings = vec![Finding {
            path: "a.rs".into(),
            line: 3,
            rule: "study method",
            matched: "vtable".into(),
            context: String::new(),
        }];
        let (live, stale) = apply_allowlist(&findings, &allowed);
        assert!(live.is_empty());
        assert_eq!(stale.len(), 1);
        assert_eq!(stale[0].path, "b.rs");
        assert!(
            parse_allowlist("a.rs|study method|vtable\n").is_err(),
            "a row needs a reason"
        );
        assert!(parse_allowlist("a.rs|no such rule|x|y\n").is_err());
    }

    /// The two scopes split the workspace between them: every comment is read by exactly one of
    /// comment-lint and test-lint, so neither lint's exemption of the other's half leaves a
    /// comment unread.
    #[test]
    fn every_comment_is_scanned_by_exactly_one_scope() {
        let dir = std::env::temp_dir().join("xtask-comment-lint-scopes");
        let _ = std::fs::remove_dir_all(&dir);
        let files = [
            (
                "c/src/lib.rs",
                "// O-1 production\nfn f() {}\n#[cfg(test)]\nmod tests {\n    // O-2 inline test\n}\n\
                 #[cfg(test)]\nmod extra;\n",
            ),
            ("c/src/extra.rs", "// O-3 pulled in by a test item\n"),
            ("c/tests/cpu/main.rs", "// O-4 integration test\n"),
            ("c/benches/b.rs", "// O-6 bench\n"),
            ("c/examples/e.rs", "// O-7 example\n"),
            ("c/Cargo.toml", "# O-5 manifest\n"),
        ];
        for (path, text) in files {
            let p = dir.join(path);
            std::fs::create_dir_all(p.parent().expect("parent")).expect("dir");
            std::fs::write(&p, text).expect("write");
        }
        let labels = |scope| -> BTreeSet<String> {
            scan_workspace(&dir, scope, &BTreeSet::new())
                .0
                .into_iter()
                .map(|f| f.matched)
                .collect()
        };
        let production = labels(Scope::Production);
        let test = labels(Scope::Test);
        std::fs::remove_dir_all(&dir).ok();
        let set = |xs: &[&str]| xs.iter().map(|x| (*x).to_owned()).collect::<BTreeSet<_>>();
        assert_eq!(production, set(&["O-1", "O-5", "O-7"]));
        assert_eq!(test, set(&["O-2", "O-3", "O-4", "O-6"]));
        assert!(production.is_disjoint(&test));
    }
}
