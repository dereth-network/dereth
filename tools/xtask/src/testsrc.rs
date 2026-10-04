//! The workspace's tests, read from source: every `#[test]` fn, where libtest will name it, what it
//! asserts and a hash of its body.
//!
//! One reader for everything that reasons about tests without building them: `test-lint` checks
//! names and anchors against it, and `test-inventory` writes it out, so that a tree can be compared
//! before and after tests are moved, renamed, merged or split.
//!
//! The walk follows the module tree the way the compiler does: from each target's root file through
//! `mod name;` (with `#[path]`) and inline `mod name { .. }`, so a test's module path is the one
//! `cargo test -- --list` prints. Attributes are not evaluated: a module behind a `cfg` is read like
//! any other, since the point is to see every test that exists.
//!
//! **What the hashes are for.** `hash` covers the body's tokens with comments removed and whitespace
//! collapsed, so a file move or a fn rename leaves it unchanged. `hash_code` also blanks every string
//! literal, so rewriting comments and assertion messages leaves it unchanged too. Each assertion
//! (an `assert*!` macro or a call to an `assert_*` helper) gets its own `hash_code`-style hash, so a
//! merge or split can be checked as a multiset of assertions rather than as a count.

use std::path::{Path, PathBuf};
use std::process::Command;

/// What a token is.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TokKind {
    Ident,
    Punct,
    Str,
    Char,
    Num,
    Lifetime,
    /// `///` or `/** */`: documents the item that follows.
    DocOuter,
    /// `//!` or `/*! */`: documents the enclosing item.
    DocInner,
}

/// One token. Doc comments are tokens, because they belong to the item they document; ordinary
/// comments are dropped.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Tok {
    pub kind: TokKind,
    pub text: String,
    pub line: usize,
}

fn is_ident_start(c: char) -> bool {
    c.is_alphabetic() || c == '_'
}

fn is_ident_char(c: char) -> bool {
    c.is_alphanumeric() || c == '_'
}

/// Split Rust source into tokens.
#[allow(clippy::too_many_lines)]
pub fn lex(src: &str) -> Vec<Tok> {
    let c: Vec<char> = src.chars().collect();
    let n = c.len();
    let mut out = Vec::new();
    let mut i = 0usize;
    let mut line = 1usize;
    let at = |i: usize, s: &str| {
        s.chars()
            .enumerate()
            .all(|(k, ch)| i + k < n && c[i + k] == ch)
    };
    while i < n {
        let ch = c[i];
        if ch == '\n' {
            line += 1;
            i += 1;
            continue;
        }
        if ch.is_whitespace() {
            i += 1;
            continue;
        }
        if at(i, "//") {
            let mut e = i;
            while e < n && c[e] != '\n' {
                e += 1;
            }
            let text: String = c[i..e].iter().collect();
            let kind = if text.starts_with("///") && !text.starts_with("////") {
                Some(TokKind::DocOuter)
            } else if text.starts_with("//!") {
                Some(TokKind::DocInner)
            } else {
                None
            };
            if let Some(kind) = kind {
                out.push(Tok {
                    kind,
                    text: text[3..].to_owned(),
                    line,
                });
            }
            i = e;
            continue;
        }
        if at(i, "/*") {
            let start_line = line;
            let doc = if at(i, "/**") && !at(i, "/**/") && !at(i, "/***") {
                Some(TokKind::DocOuter)
            } else if at(i, "/*!") {
                Some(TokKind::DocInner)
            } else {
                None
            };
            let mut nest = 0usize;
            let mut e = i;
            while e < n {
                if at(e, "/*") {
                    nest += 1;
                    e += 2;
                    continue;
                }
                if at(e, "*/") {
                    nest -= 1;
                    e += 2;
                    if nest == 0 {
                        break;
                    }
                    continue;
                }
                if c[e] == '\n' {
                    line += 1;
                }
                e += 1;
            }
            if let Some(kind) = doc {
                let body: String = c[(i + 3).min(e)..e.saturating_sub(2).max(i + 3)]
                    .iter()
                    .collect();
                for (k, l) in body.split('\n').enumerate() {
                    out.push(Tok {
                        kind,
                        text: l.trim_start().trim_start_matches('*').to_owned(),
                        line: start_line + k,
                    });
                }
            }
            i = e;
            continue;
        }
        // String literals, raw strings and byte/C strings.
        let prefix_len =
            if (ch == 'b' || ch == 'c') && i + 1 < n && (c[i + 1] == '"' || c[i + 1] == 'r') {
                1
            } else {
                0
            };
        let prev_word = i > 0 && is_ident_char(c[i - 1]);
        if !prev_word {
            let k = i + prefix_len;
            if k < n && c[k] == 'r' && k + 1 < n && (c[k + 1] == '"' || c[k + 1] == '#') {
                let mut h = k + 1;
                while h < n && c[h] == '#' {
                    h += 1;
                }
                if h < n && c[h] == '"' {
                    let hashes = h - k - 1;
                    let mut e = h + 1;
                    let start_line = line;
                    while e < n {
                        if c[e] == '"' && (0..hashes).all(|j| e + 1 + j < n && c[e + 1 + j] == '#')
                        {
                            break;
                        }
                        if c[e] == '\n' {
                            line += 1;
                        }
                        e += 1;
                    }
                    let end = (e + 1 + hashes).min(n);
                    out.push(Tok {
                        kind: TokKind::Str,
                        text: c[i..end].iter().collect(),
                        line: start_line,
                    });
                    i = end;
                    continue;
                }
            }
            if k < n && c[k] == '"' {
                let start_line = line;
                let mut e = k + 1;
                while e < n && c[e] != '"' {
                    if c[e] == '\\' {
                        if e + 1 < n && c[e + 1] == '\n' {
                            line += 1;
                        }
                        e += 1;
                    } else if c[e] == '\n' {
                        line += 1;
                    }
                    e += 1;
                }
                let end = (e + 1).min(n);
                out.push(Tok {
                    kind: TokKind::Str,
                    text: c[i..end].iter().collect(),
                    line: start_line,
                });
                i = end;
                continue;
            }
            if ch == 'b' && i + 1 < n && c[i + 1] == '\'' {
                // A byte literal: lexed below as a char literal with its prefix.
                let (end, text) = char_literal(&c, i + 1);
                if let Some(end) = end {
                    out.push(Tok {
                        kind: TokKind::Char,
                        text: format!("b{text}"),
                        line,
                    });
                    i = end;
                    continue;
                }
            }
        }
        if ch == '\'' {
            let (end, text) = char_literal(&c, i);
            if let Some(end) = end {
                out.push(Tok {
                    kind: TokKind::Char,
                    text,
                    line,
                });
                i = end;
                continue;
            }
            // A lifetime or a label.
            let mut e = i + 1;
            while e < n && is_ident_char(c[e]) {
                e += 1;
            }
            out.push(Tok {
                kind: TokKind::Lifetime,
                text: c[i..e].iter().collect(),
                line,
            });
            i = e;
            continue;
        }
        if is_ident_start(ch) {
            let mut e = i;
            // A raw identifier `r#name`.
            if ch == 'r' && i + 2 < n && c[i + 1] == '#' && is_ident_start(c[i + 2]) {
                e = i + 2;
            }
            while e < n && is_ident_char(c[e]) {
                e += 1;
            }
            out.push(Tok {
                kind: TokKind::Ident,
                text: c[i..e].iter().collect(),
                line,
            });
            i = e;
            continue;
        }
        if ch.is_ascii_digit() {
            let mut e = i;
            while e < n
                && (is_ident_char(c[e]) || (c[e] == '.' && e + 1 < n && c[e + 1].is_ascii_digit()))
            {
                e += 1;
            }
            out.push(Tok {
                kind: TokKind::Num,
                text: c[i..e].iter().collect(),
                line,
            });
            i = e;
            continue;
        }
        out.push(Tok {
            kind: TokKind::Punct,
            text: ch.to_string(),
            line,
        });
        i += 1;
    }
    out
}

/// A char literal starting at the `'` at `i`: its end and text, or `None` for a lifetime.
fn char_literal(c: &[char], i: usize) -> (Option<usize>, String) {
    let n = c.len();
    if i + 1 < n && c[i + 1] == '\\' {
        // Past the escaped character, so an escaped quote does not end the literal.
        let mut e = i + 3;
        while e < n && c[e] != '\'' && c[e] != '\n' {
            e += 1;
        }
        let end = (e + 1).min(n);
        return (Some(end), c[i..end].iter().collect());
    }
    if i + 2 < n && c[i + 2] == '\'' {
        return (Some(i + 3), c[i..i + 3].iter().collect());
    }
    (None, String::new())
}

/// The index of the token that closes the group opened at `open` (a `(`, `[` or `{`).
pub fn close_of(toks: &[Tok], open: usize) -> usize {
    let mut depth = 0i64;
    for (k, t) in toks.iter().enumerate().skip(open) {
        if t.kind != TokKind::Punct {
            continue;
        }
        match t.text.as_str() {
            "(" | "[" | "{" => depth += 1,
            ")" | "]" | "}" => {
                depth -= 1;
                if depth == 0 {
                    return k;
                }
            }
            _ => {}
        }
    }
    toks.len().saturating_sub(1)
}

fn is_p(t: Option<&Tok>, s: &str) -> bool {
    t.is_some_and(|t| t.kind == TokKind::Punct && t.text == s)
}

fn is_i(t: Option<&Tok>, s: &str) -> bool {
    t.is_some_and(|t| t.kind == TokKind::Ident && t.text == s)
}

/// How a test is ignored, from its attributes.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Ignore {
    No,
    /// `#[ignore]` or `#[ignore = ".."]`: the reason, if one is given.
    Always(Option<String>),
    /// `#[cfg_attr(<cond>, ignore ..)]`: ignored unless a feature is on.
    Conditional(Option<String>),
}

/// One `#[test]` fn.
#[derive(Debug, Clone)]
pub struct TestFn {
    pub package: String,
    /// The target: `lib`, `bin.<name>`, or the integration target's name (`cpu`, `dat`, ...).
    pub target: String,
    /// The module path inside the target, as libtest prints it.
    pub module: Vec<String>,
    pub name: String,
    /// The file, relative to the workspace root, `/`-separated.
    pub file: String,
    pub line: usize,
    pub ignore: Ignore,
    /// The `///` lines above it, trimmed.
    pub docs: Vec<String>,
    /// Assertion macros in the body (`assert!`, `assert_eq!`, `debug_assert!`, ...).
    pub assert_macros: usize,
    /// Calls to assertion helpers (`assert_*(..)`).
    pub assert_calls: usize,
    /// Body tokens without comments.
    pub hash: String,
    /// Body tokens without comments, every string literal blanked.
    pub hash_code: String,
    /// One `hash_code`-style hash per assertion site, in source order.
    pub assert_hashes: Vec<String>,
}

impl TestFn {
    /// The libtest name: `module::path::fn`.
    #[must_use]
    pub fn libtest_name(&self) -> String {
        let mut s = self.module.join("::");
        if !s.is_empty() {
            s.push_str("::");
        }
        s.push_str(&self.name);
        s
    }

    /// The durable path: `<package>::<target>::<module path>::<fn>`.
    #[must_use]
    pub fn durable(&self) -> String {
        format!("{}::{}::{}", self.package, self.target, self.libtest_name())
    }
}

/// One module file the walk read.
#[derive(Debug, Clone)]
pub struct ModFile {
    pub package: String,
    pub target: String,
    pub module: Vec<String>,
    /// Relative to the workspace root.
    pub file: String,
    /// The `//!` lines, trimmed.
    pub inner_docs: Vec<String>,
    /// The `mod name;` declarations it makes at file level: `(name, line, resolved file)`.
    pub decls: Vec<(String, usize, Option<String>)>,
    /// Whether this is a target's root file.
    pub root: bool,
    /// Its tokens, kept for the checks that need more than the test fns.
    pub toks: Vec<Tok>,
}

/// A target to walk.
#[derive(Debug, Clone)]
pub struct Target {
    pub package: String,
    pub name: String,
    pub root: PathBuf,
}

/// The whole reading.
#[derive(Debug, Default)]
pub struct Inventory {
    pub tests: Vec<TestFn>,
    pub files: Vec<ModFile>,
    /// Named items in test code, including helpers and inline modules.
    pub declarations: Vec<Declaration>,
}

/// A source declaration, not necessarily a compiled test.
#[derive(Debug, Clone)]
pub struct Declaration {
    pub file: String,
    pub line: usize,
    pub name: String,
    pub module: bool,
}

/// FNV-1a 64, as hex.
#[must_use]
pub fn fnv(text: &str) -> String {
    let mut h: u64 = 0xCBF2_9CE4_8422_2325;
    for b in text.bytes() {
        h ^= u64::from(b);
        h = h.wrapping_mul(0x0100_0000_01B3);
    }
    format!("{h:016x}")
}

fn norm(toks: &[Tok], blank_strings: bool) -> String {
    let mut s = String::new();
    for t in toks {
        match t.kind {
            TokKind::DocOuter | TokKind::DocInner => continue,
            TokKind::Str if blank_strings => s.push_str("\"\""),
            _ => s.push_str(&t.text),
        }
        s.push(' ');
    }
    s
}

/// Assertion sites in a body: `(macros, calls, hashes)`.
fn assertions(body: &[Tok]) -> (usize, usize, Vec<String>) {
    let mut macros = 0;
    let mut calls = 0;
    let mut hashes = Vec::new();
    let mut k = 0;
    while k < body.len() {
        let t = &body[k];
        if t.kind == TokKind::Ident {
            let name = t.text.as_str();
            let is_macro_name = name.starts_with("assert")
                || name.starts_with("debug_assert")
                || name.starts_with("prop_assert");
            if is_macro_name && is_p(body.get(k + 1), "!") {
                let open = k + 2;
                if body
                    .get(open)
                    .is_some_and(|o| o.kind == TokKind::Punct && "([{".contains(o.text.as_str()))
                {
                    let close = close_of(body, open);
                    macros += 1;
                    hashes.push(fnv(&norm(&body[k..=close], true))[..8].to_owned());
                    k = close + 1;
                    continue;
                }
            }
            let is_call_name = name.starts_with("assert_");
            let defined_here = k > 0 && is_i(body.get(k - 1), "fn");
            if is_call_name && !defined_here && is_p(body.get(k + 1), "(") {
                let close = close_of(body, k + 1);
                calls += 1;
                hashes.push(fnv(&norm(&body[k..=close], true))[..8].to_owned());
                k = close + 1;
                continue;
            }
        }
        k += 1;
    }
    (macros, calls, hashes)
}

/// The path of an attribute group `#[ path ... ]` starting at the `[`: the idents before the
/// first `(`, `=` or `]`, joined with `::`.
fn attr_path(toks: &[Tok], open: usize, close: usize) -> String {
    let mut s = String::new();
    for t in &toks[open + 1..close] {
        match t.kind {
            TokKind::Ident => s.push_str(&t.text),
            TokKind::Punct if t.text == ":" => s.push(':'),
            _ => break,
        }
    }
    s
}

fn strip_quotes(s: &str) -> String {
    let s = s.trim_start_matches(['b', 'c', 'r']).trim_matches('#');
    s.strip_prefix('"')
        .and_then(|x| x.strip_suffix('"'))
        .unwrap_or(s)
        .to_owned()
}

struct Walk<'a> {
    ws: &'a Path,
    inv: Inventory,
    package: String,
    target: String,
    seen: std::collections::BTreeSet<(String, String)>,
}

fn rel(ws: &Path, p: &Path) -> String {
    let mut parts: Vec<String> = Vec::new();
    for comp in p.strip_prefix(ws).unwrap_or(p).components() {
        match comp {
            std::path::Component::ParentDir => {
                parts.pop();
            }
            std::path::Component::CurDir => {}
            other => parts.push(other.as_os_str().to_string_lossy().into_owned()),
        }
    }
    parts.join("/")
}

impl Walk<'_> {
    /// Read one file as module `module`; `child_dir` is where its `mod name;` files are looked for.
    fn file(&mut self, path: &Path, module: &[String], child_dir: &Path, root: bool, test: bool) {
        let r = rel(self.ws, path);
        if !self.seen.insert((self.target.clone(), r.clone())) {
            return;
        }
        let Ok(text) = std::fs::read_to_string(path) else {
            return;
        };
        let toks = lex(&text);
        let inner_docs = toks
            .iter()
            .take_while(|t| t.kind == TokKind::DocInner || t.kind != TokKind::Ident)
            .filter(|t| t.kind == TokKind::DocInner)
            .map(|t| t.text.trim().to_owned())
            .collect();
        let mut decls = Vec::new();
        let dir = path.parent().unwrap_or(Path::new(".")).to_path_buf();
        self.items(
            &toks,
            0,
            toks.len(),
            module,
            child_dir,
            &dir,
            &r,
            &mut decls,
            true,
            test,
        );
        self.inv.files.push(ModFile {
            package: self.package.clone(),
            target: self.target.clone(),
            module: module.to_vec(),
            file: r,
            inner_docs,
            decls,
            root,
            toks,
        });
    }

    /// Walk the items in `toks[start..end]`.
    #[allow(clippy::too_many_arguments, clippy::too_many_lines)]
    fn items(
        &mut self,
        toks: &[Tok],
        start: usize,
        end: usize,
        module: &[String],
        child_dir: &Path,
        file_dir: &Path,
        file: &str,
        decls: &mut Vec<(String, usize, Option<String>)>,
        file_level: bool,
        test: bool,
    ) {
        let mut k = start;
        let mut attrs: Vec<(usize, usize)> = Vec::new();
        let mut docs: Vec<String> = Vec::new();
        while k < end {
            let t = &toks[k];
            match t.kind {
                TokKind::DocOuter => {
                    docs.push(t.text.trim().to_owned());
                    k += 1;
                    continue;
                }
                TokKind::DocInner => {
                    k += 1;
                    continue;
                }
                _ => {}
            }
            if t.kind == TokKind::Punct && t.text == "#" {
                let mut open = k + 1;
                let inner = is_p(toks.get(open), "!");
                if inner {
                    open += 1;
                }
                if is_p(toks.get(open), "[") {
                    let close = close_of(toks, open);
                    if !inner {
                        attrs.push((open, close));
                    }
                    k = close + 1;
                    continue;
                }
            }
            // Skip visibility and qualifiers.
            let mut j = k;
            loop {
                match toks.get(j) {
                    Some(x) if x.kind == TokKind::Ident && x.text == "pub" => {
                        j += 1;
                        if is_p(toks.get(j), "(") {
                            j = close_of(toks, j) + 1;
                        }
                    }
                    Some(x)
                        if x.kind == TokKind::Ident
                            && matches!(
                                x.text.as_str(),
                                "async" | "unsafe" | "const" | "extern"
                            )
                            && !is_p(toks.get(j + 1), ":")
                            && toks.get(j + 1).is_some_and(|y| {
                                y.kind == TokKind::Ident
                                    && matches!(
                                        y.text.as_str(),
                                        "fn" | "async"
                                            | "unsafe"
                                            | "const"
                                            | "extern"
                                            | "impl"
                                            | "trait"
                                    )
                                    || y.kind == TokKind::Str
                            }) =>
                    {
                        j += 1;
                        if toks.get(j).is_some_and(|y| y.kind == TokKind::Str) {
                            j += 1;
                        }
                    }
                    _ => break,
                }
            }
            let head = toks.get(j);
            let test_item = test
                || attrs.iter().any(|&(o, c)| {
                    let path = attr_path(toks, o, c);
                    path == "test"
                        || path.ends_with("::test")
                        || path == "cfg" && toks[o..c].iter().any(|t| is_i(Some(t), "test"))
                });
            if test_item {
                let name = if is_i(head, "mod")
                    || is_i(head, "fn")
                    || is_i(head, "const")
                    || is_i(head, "static")
                {
                    let at =
                        j + 1 + usize::from(is_i(head, "static") && is_i(toks.get(j + 1), "mut"));
                    toks.get(at).filter(|t| t.kind == TokKind::Ident)
                } else if is_i(head, "macro_rules") && is_p(toks.get(j + 1), "!") {
                    toks.get(j + 2).filter(|t| t.kind == TokKind::Ident)
                } else if head
                    .is_some_and(|t| matches!(t.text.as_str(), "message_roundtrip_test" | "rule3"))
                    && is_p(toks.get(j + 1), "!")
                    && is_p(toks.get(j + 2), "(")
                {
                    // This macro's first argument declares a test. Do not pretend its invocation
                    // is a function body or invent assertion counts for the inventory.
                    let mut at = j + 3;
                    while is_p(toks.get(at), "#") && is_p(toks.get(at + 1), "[") {
                        at = close_of(toks, at + 1) + 1;
                    }
                    toks.get(at).filter(|t| t.kind == TokKind::Ident)
                } else {
                    None
                };
                if let Some(name) = name {
                    self.inv.declarations.push(Declaration {
                        file: file.to_owned(),
                        line: name.line,
                        name: name.text.clone(),
                        module: is_i(head, "mod"),
                    });
                }
            }
            if is_i(head, "mod") {
                let name = toks.get(j + 1).map(|x| x.text.clone()).unwrap_or_default();
                let path_attr = attrs.iter().find_map(|&(o, c)| {
                    if attr_path(toks, o, c) == "path" {
                        toks[o..c]
                            .iter()
                            .find(|x| x.kind == TokKind::Str)
                            .map(|x| strip_quotes(&x.text))
                    } else {
                        None
                    }
                });
                let mut sub = module.to_vec();
                sub.push(name.clone());
                if is_p(toks.get(j + 2), ";") {
                    let candidates: Vec<PathBuf> = match &path_attr {
                        Some(p) => vec![if file_level {
                            file_dir.join(p)
                        } else {
                            child_dir.join(p)
                        }],
                        None => vec![
                            child_dir.join(format!("{}.rs", name.trim_start_matches("r#"))),
                            child_dir.join(name.trim_start_matches("r#")).join("mod.rs"),
                        ],
                    };
                    let found = candidates.into_iter().find(|p| p.is_file());
                    if file_level {
                        decls.push((
                            name.clone(),
                            toks[j].line,
                            found.as_ref().map(|p| rel(self.ws, p)),
                        ));
                    }
                    if let Some(p) = found {
                        let owns_dir = matches!(
                            p.file_name().and_then(|s| s.to_str()),
                            Some("mod.rs" | "lib.rs" | "main.rs")
                        );
                        let next_dir = if owns_dir || path_attr.is_some() {
                            p.parent().unwrap_or(Path::new(".")).to_path_buf()
                        } else {
                            p.parent()
                                .unwrap_or(Path::new("."))
                                .join(p.file_stem().unwrap_or_default())
                        };
                        self.file(&p, &sub, &next_dir, false, test_item);
                    }
                    k = j + 3;
                } else if is_p(toks.get(j + 2), "{") {
                    let close = close_of(toks, j + 2);
                    let inner_dir = match &path_attr {
                        Some(p) if file_level => file_dir.join(p),
                        Some(p) => child_dir.join(p),
                        None => child_dir.join(name.trim_start_matches("r#")),
                    };
                    self.items(
                        toks,
                        j + 3,
                        close,
                        &sub,
                        &inner_dir,
                        file_dir,
                        file,
                        decls,
                        false,
                        test_item,
                    );
                    k = close + 1;
                } else {
                    k = j + 1;
                }
                attrs.clear();
                docs.clear();
                continue;
            }
            if is_i(head, "fn") {
                let name = toks.get(j + 1).map(|x| x.text.clone()).unwrap_or_default();
                // The body: the first `{` outside parentheses and brackets, or a `;`.
                let mut b = j + 2;
                let mut body = None;
                while b < end {
                    let x = &toks[b];
                    if x.kind == TokKind::Punct {
                        match x.text.as_str() {
                            "(" | "[" => {
                                b = close_of(toks, b) + 1;
                                continue;
                            }
                            "{" => {
                                body = Some((b, close_of(toks, b)));
                                break;
                            }
                            ";" => break,
                            _ => {}
                        }
                    }
                    b += 1;
                }
                let is_test = attrs.iter().any(|&(o, c)| {
                    let p = attr_path(toks, o, c);
                    p == "test" || p.ends_with("::test")
                });
                if let (true, Some((open, close))) = (is_test, body) {
                    let mut ignore = Ignore::No;
                    for &(o, c) in &attrs {
                        let p = attr_path(toks, o, c);
                        let reason = || {
                            let at = toks[o..c].iter().position(|x| x.text == "ignore")?;
                            let after = &toks[o + at + 1..c];
                            if is_p(after.first(), "=") {
                                after
                                    .get(1)
                                    .filter(|x| x.kind == TokKind::Str)
                                    .map(|x| strip_quotes(&x.text))
                            } else {
                                None
                            }
                        };
                        if p == "ignore" {
                            ignore = Ignore::Always(reason());
                        } else if p == "cfg_attr"
                            && toks[o..c]
                                .iter()
                                .any(|x| x.kind == TokKind::Ident && x.text == "ignore")
                        {
                            ignore = Ignore::Conditional(reason());
                        }
                    }
                    let body_toks = &toks[open..=close];
                    let (assert_macros, assert_calls, assert_hashes) =
                        assertions(&body_toks[1..body_toks.len() - 1]);
                    self.inv.tests.push(TestFn {
                        package: self.package.clone(),
                        target: self.target.clone(),
                        module: module.to_vec(),
                        name,
                        file: file.to_owned(),
                        line: toks[j].line,
                        ignore,
                        docs: std::mem::take(&mut docs),
                        assert_macros,
                        assert_calls,
                        hash: fnv(&norm(body_toks, false)),
                        hash_code: fnv(&norm(body_toks, true)),
                        assert_hashes,
                    });
                }
                k = match body {
                    Some((_, close)) => close + 1,
                    None => b + 1,
                };
                attrs.clear();
                docs.clear();
                continue;
            }
            // `include!("file.rs");` at item level: the file's items join this module.
            if is_i(head, "include")
                && is_p(toks.get(j + 1), "!")
                && is_p(toks.get(j + 2), "(")
                && toks.get(j + 3).is_some_and(|x| x.kind == TokKind::Str)
            {
                let p = file_dir.join(strip_quotes(&toks[j + 3].text));
                if let Ok(text) = std::fs::read_to_string(&p) {
                    let inc = lex(&text);
                    let r = rel(self.ws, &p);
                    let dir = p.parent().unwrap_or(Path::new(".")).to_path_buf();
                    let mut none = Vec::new();
                    self.items(
                        &inc,
                        0,
                        inc.len(),
                        module,
                        child_dir,
                        &dir,
                        &r,
                        &mut none,
                        false,
                        test_item,
                    );
                }
                k = close_of(toks, j + 2) + 1;
                if is_p(toks.get(k), ";") {
                    k += 1;
                }
                attrs.clear();
                docs.clear();
                continue;
            }
            // Any other item. One that holds an expression (`const`, `static`, `type`, `use`) ends
            // at its `;`, since the expression may hold braces; any other ends at its first brace
            // group or its `;`, whichever comes first (`struct S { .. }`, `impl X { .. }`,
            // `extern "C" { .. }`, `m! { .. }`, `m!(..);`, `struct S(u8);`).
            let to_semicolon = head.is_some_and(|h| {
                h.kind == TokKind::Ident
                    && matches!(h.text.as_str(), "const" | "static" | "type" | "use" | "let")
            });
            let mut e = j;
            while e < end {
                let x = &toks[e];
                if x.kind == TokKind::Punct {
                    match x.text.as_str() {
                        ";" => break,
                        "(" | "[" => {
                            e = close_of(toks, e) + 1;
                            continue;
                        }
                        "{" => {
                            let c = close_of(toks, e);
                            if is_i(head, "impl") || is_i(head, "trait") {
                                self.items(
                                    toks,
                                    e + 1,
                                    c,
                                    module,
                                    child_dir,
                                    file_dir,
                                    file,
                                    decls,
                                    false,
                                    test_item,
                                );
                            }
                            if !to_semicolon {
                                e = c;
                                break;
                            }
                            e = c + 1;
                            continue;
                        }
                        _ => {}
                    }
                }
                e += 1;
            }
            k = e + 1;
            attrs.clear();
            docs.clear();
        }
    }
}

/// The workspace's targets, from `cargo metadata`.
///
/// # Errors
/// When cargo cannot be run or its output does not parse.
pub fn targets(ws: &Path) -> Result<Vec<Target>, String> {
    let out = Command::new("cargo")
        .args([
            "metadata",
            "--format-version",
            "1",
            "--no-deps",
            "--offline",
        ])
        .current_dir(ws)
        .output()
        .map_err(|e| format!("cargo metadata: {e}"))?;
    if !out.status.success() {
        return Err(format!(
            "cargo metadata failed: {}",
            String::from_utf8_lossy(&out.stderr)
        ));
    }
    let v: serde_json::Value =
        serde_json::from_slice(&out.stdout).map_err(|e| format!("cargo metadata: {e}"))?;
    let mut result = Vec::new();
    for p in v["packages"]
        .as_array()
        .ok_or("cargo metadata: no packages")?
    {
        let package = p["name"].as_str().unwrap_or_default().to_owned();
        for t in p["targets"].as_array().cloned().unwrap_or_default() {
            let kinds: Vec<&str> = t["kind"]
                .as_array()
                .map(|ks| ks.iter().filter_map(|k| k.as_str()).collect())
                .unwrap_or_default();
            let name = t["name"].as_str().unwrap_or_default();
            let tname = if kinds.contains(&"lib") || kinds.contains(&"proc-macro") {
                "lib".to_owned()
            } else if kinds.contains(&"bin") {
                format!("bin.{name}")
            } else if kinds.contains(&"test") {
                name.to_owned()
            } else {
                continue;
            };
            result.push(Target {
                package: package.clone(),
                name: tname,
                root: PathBuf::from(t["src_path"].as_str().unwrap_or_default()),
            });
        }
    }
    result.sort_by(|a, b| (&a.package, &a.name).cmp(&(&b.package, &b.name)));
    Ok(result)
}

/// Read every target in `targets`.
#[must_use]
pub fn read(ws: &Path, targets: &[Target]) -> Inventory {
    let mut w = Walk {
        ws,
        inv: Inventory::default(),
        package: String::new(),
        target: String::new(),
        seen: std::collections::BTreeSet::new(),
    };
    for t in targets {
        w.package.clone_from(&t.package);
        w.target.clone_from(&t.name);
        let dir = t.root.parent().unwrap_or(Path::new(".")).to_path_buf();
        let test = t.name != "lib" && !t.name.starts_with("bin.");
        w.file(&t.root, &[], &dir, true, test);
    }
    w.inv
}

/// Read the whole workspace.
///
/// # Errors
/// As [`targets`].
pub fn read_workspace(ws: &Path) -> Result<Inventory, String> {
    Ok(read(ws, &targets(ws)?))
}

/// The inventory as TSV, one row per test fn, sorted.
#[must_use]
pub fn inventory_tsv(inv: &Inventory) -> String {
    let mut rows: Vec<String> = inv
        .tests
        .iter()
        .map(|t| {
            let ignore = match &t.ignore {
                Ignore::No => String::new(),
                Ignore::Always(r) => format!("always:{}", r.as_deref().unwrap_or("")),
                Ignore::Conditional(r) => format!("cfg:{}", r.as_deref().unwrap_or("")),
            };
            let behaviour: Vec<&str> = t.docs.iter().filter_map(|d| behaviour_id(d)).collect();
            [
                t.package.clone(),
                t.target.clone(),
                t.libtest_name(),
                t.file.clone(),
                t.line.to_string(),
                ignore.replace(['\t', '\n'], " "),
                t.assert_macros.to_string(),
                t.assert_calls.to_string(),
                t.hash.clone(),
                t.hash_code.clone(),
                behaviour.join(","),
                t.assert_hashes.join(","),
            ]
            .join("\t")
        })
        .collect();
    rows.sort();
    rows.dedup();
    let mut out = String::from(
        "package\ttarget\ttest\tfile\tline\tignore\tassert_macros\tassert_calls\thash\thash_code\tbehaviour\tassert_hashes\n",
    );
    for r in rows {
        out.push_str(&r);
        out.push('\n');
    }
    out
}

/// The id a `Behaviour: <id>` doc line names, if it is one.
#[must_use]
pub fn behaviour_id(doc: &str) -> Option<&str> {
    doc.trim()
        .strip_prefix("Behaviour:")
        .and_then(|r| r.split_whitespace().next())
}

/// `cargo xtask test-inventory [--out <file>]`.
pub fn test_inventory(args: &[String]) -> i32 {
    let ws = crate::util::workspace_root();
    let inv = match read_workspace(&ws) {
        Ok(i) => i,
        Err(e) => {
            eprintln!("test-inventory: {e}");
            return 2;
        }
    };
    let tsv = inventory_tsv(&inv);
    match args.iter().position(|a| a == "--out") {
        Some(k) => {
            let Some(path) = args.get(k + 1) else {
                eprintln!("test-inventory: --out needs a file");
                return 2;
            };
            if let Err(e) = std::fs::write(path, tsv) {
                eprintln!("test-inventory: {path}: {e}");
                return 2;
            }
            eprintln!(
                "test-inventory: {} test fn(s) in {} file(s) -> {path}",
                inv.tests.len(),
                inv.files.len()
            );
        }
        None => print!("{tsv}"),
    }
    0
}

#[cfg(test)]
mod tests {
    use super::*;

    fn kinds(src: &str) -> Vec<(TokKind, String)> {
        lex(src).into_iter().map(|t| (t.kind, t.text)).collect()
    }

    #[test]
    fn comments_vanish_and_doc_comments_are_tokens() {
        let k = kinds("// plain\n/// outer\n//! inner\n/* block */ x");
        assert_eq!(
            k,
            vec![
                (TokKind::DocOuter, " outer".into()),
                (TokKind::DocInner, " inner".into()),
                (TokKind::Ident, "x".into()),
            ]
        );
    }

    #[test]
    fn strings_chars_and_lifetimes_are_told_apart() {
        let k = kinds(r####"'a' '\'' 'b &'static str r#"a "q" b"# b"x" b'y'"####);
        let got: Vec<TokKind> = k.iter().map(|(k, _)| *k).collect();
        assert_eq!(
            got,
            vec![
                TokKind::Char,
                TokKind::Char,
                TokKind::Lifetime,
                TokKind::Punct,
                TokKind::Lifetime,
                TokKind::Ident,
                TokKind::Str,
                TokKind::Str,
                TokKind::Char,
            ]
        );
    }

    fn walk_src(src: &str) -> Inventory {
        walk_target(src, "cpu")
    }

    fn walk_target(src: &str, target: &str) -> Inventory {
        let dir = std::env::temp_dir().join(format!("xtask-testsrc-{}", fnv(src)));
        std::fs::create_dir_all(&dir).expect("temp dir");
        let root = dir.join("main.rs");
        std::fs::write(&root, src).expect("write");
        let inv = read(
            &dir,
            &[Target {
                package: "p".into(),
                name: target.into(),
                root,
            }],
        );
        std::fs::remove_dir_all(&dir).ok();
        inv
    }

    #[test]
    fn declarations_include_helpers_constants_modules_and_named_macro_inputs() {
        let inv = walk_target(
            "fn production() {} const LIVE: u8 = 0;\n#[cfg(test)] mod tests {\n\
             const SAMPLE: u8 = 1; fn helper() {} mod inner { fn setup() {} }\n\
             message_roundtrip_test!(message_reaches_client, Packet);\n\
             message_roundtrip_test!(#[ignore = \"unsupported\"] future_message, Packet);\n\
             #[test] fn ordinary() { assert!(true); }\n}",
            "lib",
        );
        let names: Vec<_> = inv.declarations.iter().map(|d| d.name.as_str()).collect();
        assert_eq!(
            names,
            [
                "tests",
                "SAMPLE",
                "helper",
                "inner",
                "setup",
                "message_reaches_client",
                "future_message",
                "ordinary"
            ]
        );
        assert!(inv.declarations[3].module);
        assert_eq!(
            inv.tests.len(),
            1,
            "macro inputs are declarations, not invented test bodies"
        );
        assert_eq!(inv.tests[0].assert_macros, 1);
    }

    #[test]
    fn declarations_include_test_impl_methods_and_associated_constants() {
        let inv = walk_target(
            "struct Fixture; impl Fixture { fn live() {} #[cfg(test)] fn fixture() {} }\n\
             #[cfg(test)] mod tests { struct Sample; impl Sample {\n\
             const LIMIT: u8 = 1; fn r#helper() {} }\n\
             trait Setup { const DEFAULT: u8 = 2; fn initialize(&self); } }",
            "lib",
        );
        let names: Vec<_> = inv.declarations.iter().map(|d| d.name.as_str()).collect();
        assert_eq!(
            names,
            [
                "fixture",
                "tests",
                "LIMIT",
                "r#helper",
                "DEFAULT",
                "initialize"
            ]
        );
        assert!(inv.tests.is_empty());
    }

    #[test]
    fn declarations_follow_gated_module_files_and_included_helpers() {
        let dir = std::env::temp_dir().join("xtask-testsrc-declarations");
        std::fs::create_dir_all(&dir).expect("temp dir");
        let root = dir.join("lib.rs");
        std::fs::write(&root, "#[cfg(test)] #[path = \"checks.rs\"] mod checks;").expect("root");
        std::fs::write(
            dir.join("checks.rs"),
            "include!(\"helpers.rs\"); #[test] fn works() { assert!(true); }",
        )
        .expect("tests");
        std::fs::write(
            dir.join("helpers.rs"),
            "fn helper() {} const LIMIT: u8 = 1;",
        )
        .expect("helpers");
        let inv = read(
            &dir,
            &[Target {
                package: "sample".into(),
                name: "lib".into(),
                root,
            }],
        );
        std::fs::remove_dir_all(&dir).expect("remove fixture");
        let names: Vec<_> = inv.declarations.iter().map(|d| d.name.as_str()).collect();
        assert_eq!(names, ["checks", "helper", "LIMIT", "works"]);
        assert_eq!(inv.declarations[1].file, "helpers.rs");
        assert_eq!(inv.tests[0].libtest_name(), "checks::works");
    }

    #[test]
    fn inline_path_modules_use_the_containing_file_directory() {
        let scratch = dereth_dat::testing::ScratchDir::new("testsrc-inline-path").unwrap();
        let dir = scratch.path();
        std::fs::create_dir_all(dir.join("backend/nested")).unwrap();
        let root = dir.join("lib.rs");
        std::fs::write(&root, "mod backend;").unwrap();
        std::fs::write(
            dir.join("backend.rs"),
            r#"#[cfg(test)] #[path = "backend"] mod tests {
                #[path = "checks.rs"] mod checks;
                #[path = "nested"] mod inner { mod more; }
            }"#,
        )
        .unwrap();
        std::fs::write(
            dir.join("backend/checks.rs"),
            "#[test] fn works() { assert!(true); }",
        )
        .unwrap();
        std::fs::write(
            dir.join("backend/nested/more.rs"),
            "#[test] fn nested_works() { assert!(true); }",
        )
        .unwrap();
        let inv = read(
            dir,
            &[Target {
                package: "sample".into(),
                name: "lib".into(),
                root,
            }],
        );
        let names: Vec<_> = inv.tests.iter().map(TestFn::durable).collect();
        assert_eq!(
            names,
            [
                "sample::lib::backend::tests::checks::works",
                "sample::lib::backend::tests::inner::more::nested_works",
            ]
        );
        assert_eq!(inv.tests[0].file, "backend/checks.rs");
        assert_eq!(inv.tests[1].file, "backend/nested/more.rs");
    }

    #[test]
    fn tests_are_found_with_their_module_path_docs_and_ignore() {
        let inv = walk_src(
            "//! header\nuse x::y;\nconst C: S = S { a: 1 };\nmod inner {\n    /// Behaviour: a.b.c\n    #[test]\n    #[ignore = \"slow\"]\n    fn one() { assert_eq!(1, 1); assert_behaviour(\"x\"); }\n    fn helper() {}\n}\n#[cfg(test)]\nmod tests {\n    #[tokio::test]\n    async fn two() { assert!(true, \"m\"); }\n}\nimpl Foo { fn not_a_test() {} }\n",
        );
        let names: Vec<String> = inv.tests.iter().map(TestFn::libtest_name).collect();
        assert_eq!(names, vec!["inner::one", "tests::two"]);
        let one = &inv.tests[0];
        assert_eq!(one.ignore, Ignore::Always(Some("slow".into())));
        assert_eq!(behaviour_id(&one.docs[0]), Some("a.b.c"));
        assert_eq!((one.assert_macros, one.assert_calls), (1, 1));
        assert_eq!(one.durable(), "p::cpu::inner::one");
        assert_eq!(inv.files[0].inner_docs, vec!["header".to_owned()]);
    }

    #[test]
    fn the_body_hash_ignores_comments_and_layout_and_the_code_hash_ignores_strings() {
        let a = walk_src("#[test]\nfn a() {\n    // why\n    assert_eq!(f(1), 2, \"one\");\n}\n");
        let b = walk_src("#[test]\nfn b() { assert_eq!( f(1),2, \"one\" ); }\n");
        let c = walk_src("#[test]\nfn c() { assert_eq!(f(1), 2, \"two\"); }\n");
        let d = walk_src("#[test]\nfn d() { assert_eq!(f(1), 3, \"one\"); }\n");
        assert_eq!(a.tests[0].hash, b.tests[0].hash);
        assert_ne!(a.tests[0].hash, c.tests[0].hash);
        assert_eq!(a.tests[0].hash_code, c.tests[0].hash_code);
        assert_ne!(a.tests[0].hash_code, d.tests[0].hash_code);
        assert_eq!(a.tests[0].assert_hashes, c.tests[0].assert_hashes);
    }

    #[test]
    fn a_cfg_attr_ignore_is_conditional() {
        let inv = walk_src(
            "#[test]\n#[cfg_attr(not(feature = \"retail-dats\"), ignore = \"reads the dats\")]\nfn t() {}\n#[test]\n#[ignore]\nfn u() {}\n",
        );
        assert_eq!(
            inv.tests[0].ignore,
            Ignore::Conditional(Some("reads the dats".into()))
        );
        assert_eq!(inv.tests[1].ignore, Ignore::Always(None));
    }
}
