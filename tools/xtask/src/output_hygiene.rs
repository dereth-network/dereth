//! `cargo xtask output-hygiene [ROOT]`: statically visible Rust output paths must not overwrite
//! checkout files or leave the workspace.
//!
//! The workspace's Rust sources are read from the git index. Read-only fixture paths are allowed. A
//! test or tool writes only inside the workspace: into cargo's target directory
//! (`CARGO_TARGET_TMPDIR` in an integration test) or a temporary directory. A destination that is
//! tracked, or that climbs out of the workspace, fails. Output calls with literal destinations and
//! simple local bindings and joins are checked; helper return values, environment values and
//! runtime path construction remain outside this source instrument, and are counted as unresolved
//! rather than proven safe. It executes no Rust code.
//!
//! The source is read through a small lexer of its own rather than the test reader's or the
//! comment lint's: this check needs every comment blanked **in place** and every string literal
//! replaced by a numbered placeholder at its exact offset, so that the destination expression can
//! be read back out of the code around it.

use std::collections::BTreeSet;
use std::path::{Path, PathBuf};
use std::process::Command;
use std::sync::LazyLock;

use regex::Regex;

use crate::util::workspace_root;

/// What a lexed span is.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Span {
    Comment,
    Str,
}

/// Sorted, non-overlapping `(start, end, kind)` byte spans of the comments and string (and char)
/// literals in `text`.
fn lex_rust(text: &str) -> Vec<(usize, usize, Span)> {
    let b = text.as_bytes();
    let n = b.len();
    let mut spans = Vec::new();
    let mut i = 0;
    let find = |from: usize, pat: &[u8]| -> Option<usize> {
        (from..n.saturating_sub(pat.len() - 1)).find(|&k| b[k..].starts_with(pat))
    };
    while i < n {
        let c = b[i];
        if b[i..].starts_with(b"//") {
            let e = find(i, b"\n").unwrap_or(n);
            spans.push((i, e, Span::Comment));
            i = e;
        } else if b[i..].starts_with(b"/*") {
            let mut depth = 1;
            let mut j = i + 2;
            while j < n && depth > 0 {
                if b[j..].starts_with(b"/*") {
                    depth += 1;
                    j += 2;
                } else if b[j..].starts_with(b"*/") {
                    depth -= 1;
                    j += 2;
                } else {
                    j += 1;
                }
            }
            let e = j.min(n);
            spans.push((i, e, Span::Comment));
            i = e;
        } else if let Some((open, hashes)) = raw_string_open(b, i) {
            let mut close = vec![b'"'];
            close.extend(std::iter::repeat_n(b'#', hashes));
            let e = find(open + 1, &close).map_or(n, |k| k + close.len());
            spans.push((i, e, Span::Str));
            i = e;
        } else if c == b'"' || (c == b'b' && b.get(i + 1) == Some(&b'"')) {
            let mut j = if c == b'b' { i + 2 } else { i + 1 };
            while j < n {
                match b[j] {
                    b'\\' => j += 2,
                    b'"' => {
                        j += 1;
                        break;
                    }
                    _ => j += 1,
                }
            }
            let e = j.min(n);
            spans.push((i, e, Span::Str));
            i = e;
        } else if c == b'\'' {
            // A char literal or a lifetime. A lifetime has no closing quote, so only treat it as a
            // literal when one follows close by.
            let j = i + 1;
            let width = text
                .get(j..)
                .and_then(|t| t.chars().next())
                .map_or(1, char::len_utf8);
            let close = if b.get(j) == Some(&b'\\') {
                find(j + 1, b"'").filter(|&k| k <= j + 4)
            } else if b.get(j + width) == Some(&b'\'') {
                Some(j + width)
            } else {
                None
            };
            match close {
                Some(k) => {
                    spans.push((i, k + 1, Span::Str));
                    i = k + 1;
                }
                None => i = j,
            }
        } else {
            i += 1;
        }
    }
    spans
}

/// `r"`, `r#"`, `br"`, `br#"` at `i`: the offset of the opening quote and the hash count.
fn raw_string_open(b: &[u8], i: usize) -> Option<(usize, usize)> {
    let mut k = i;
    if b.get(k) == Some(&b'b') {
        k += 1;
    }
    if b.get(k) != Some(&b'r') {
        return None;
    }
    k += 1;
    let mut hashes = 0;
    while b.get(k) == Some(&b'#') {
        hashes += 1;
        k += 1;
    }
    (b.get(k) == Some(&b'"')).then_some((k, hashes))
}

/// The value of a Rust string literal's source text, or `None` when it cannot be read.
fn literal_value(raw: &str) -> Option<String> {
    if raw.starts_with('\'') {
        return None;
    }
    let body = raw.strip_prefix('b').unwrap_or(raw);
    if body.starts_with('r') {
        let (s, e) = (body.find('"')?, body.rfind('"')?);
        return (e > s).then(|| body[s + 1..e].to_owned());
    }
    let inner = body.strip_prefix('"')?.strip_suffix('"')?;
    let mut out = String::new();
    let mut chars = inner.chars().peekable();
    while let Some(ch) = chars.next() {
        if ch != '\\' {
            out.push(ch);
            continue;
        }
        match chars.next()? {
            'n' => out.push('\n'),
            't' => out.push('\t'),
            'r' => out.push('\r'),
            '0' => out.push('\0'),
            '\\' => out.push('\\'),
            '"' => out.push('"'),
            '\'' => out.push('\''),
            'x' => {
                let hex: String = [chars.next()?, chars.next()?].iter().collect();
                out.push(char::from(u8::from_str_radix(&hex, 16).ok()?));
            }
            'u' => {
                if chars.next()? != '{' {
                    return None;
                }
                let hex: String = chars.by_ref().take_while(|&c| c != '}').collect();
                out.push(char::from_u32(u32::from_str_radix(&hex, 16).ok()?)?);
            }
            '\n' => {
                while chars.peek().is_some_and(|c| c.is_whitespace()) {
                    chars.next();
                }
            }
            _ => return None,
        }
    }
    Some(out)
}

/// A string literal: its value (if readable) and its line.
type Literal = (Option<String>, usize);

/// The source with every comment blanked (newlines kept) and every string literal replaced by
/// `__path_literal_N__`, and the literals in order.
fn code_and_literals(source: &str) -> (String, Vec<Literal>) {
    let mut code = String::with_capacity(source.len());
    let mut literals = Vec::new();
    let mut last = 0;
    // The line of `last`, advanced as the spans are walked, so a file of many literals is counted
    // once rather than once per literal.
    let mut line = 1;
    for (start, end, kind) in lex_rust(source) {
        code.push_str(&source[last..start]);
        line += source[last..start].matches('\n').count();
        let piece = &source[start..end];
        match kind {
            Span::Comment => {
                code.extend(piece.chars().map(|c| if c == '\n' { '\n' } else { ' ' }));
            }
            Span::Str => {
                literals.push((literal_value(piece), line));
                code.push_str(&format!("__path_literal_{}__", literals.len() - 1));
                code.extend(std::iter::repeat_n('\n', piece.matches('\n').count()));
            }
        }
        line += piece.matches('\n').count();
        last = end;
    }
    code.push_str(&source[last..]);
    (code, literals)
}

fn line_of(text: &str, offset: usize) -> usize {
    text[..offset].matches('\n').count() + 1
}

static WRITER: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(
        r"\b(?:std::)?fs::write\s*\(|\b(?:std::fs::|fs::)?File::create\s*\(|\.(?:save|save_with_format|save_png|save_tga)\s*\(",
    )
    .expect("WRITER")
});
static LITERAL: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"^__path_literal_(\d+)__$").expect("LITERAL"));
static TEMP_DIR: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"^(?:std::env::|env::)?temp_dir\(\)$").expect("TEMP_DIR"));
static WS_ROOT: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(r"^(?:workspace_root|workspace_root_buf|repo_root)\(\)$").expect("WS_ROOT")
});
static JOIN: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"(?s)^(.*)\.join\((.*)\)$").expect("JOIN"));
static PATH_NEW: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"(?s)^(?:Path|PathBuf)::(?:new|from)\((.*)\)$").expect("PATH"));
static IDENT: LazyLock<Regex> = LazyLock::new(|| Regex::new(r"^[A-Za-z_]\w*$").expect("IDENT"));
static FN_HEAD: LazyLock<Regex> = LazyLock::new(|| Regex::new(r"\bfn\s+\w+").expect("FN_HEAD"));
static DRIVE: LazyLock<Regex> = LazyLock::new(|| Regex::new(r"^[A-Za-z]:/").expect("DRIVE"));
static INSTALL_DECL: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(r"\bpub const DEFAULT_CLIENT_DIR:\s*&str\s*=\s*__path_literal_(\d+)__\s*;")
        .expect("INSTALL_DECL")
});
static INSTALL_FALLBACK: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(
        r"fn client_dir\(\) -> PathBuf\s*\{\s*PathBuf::from\(std::env::var\((__path_literal_\d+__)\)\.unwrap_or_else\(\|_\|\s*__path_literal_(\d+)__\.into\(\)\)\)\s*\}",
    )
    .expect("INSTALL_FALLBACK")
});

/// What a workspace-root call resolves to, so a destination built on it is read against the
/// workspace rather than against the crate.
const WORKSPACE: &str = "__workspace__";
/// What a temporary or cargo-scratch directory resolves to.
const TEMPORARY: &str = "__temporary_output__/";

/// The first argument of the call whose `(` ends just before `start`.
fn first_arg(code: &str, start: usize) -> &str {
    let mut depth = 0i32;
    for (i, c) in code[start..].char_indices() {
        match c {
            '(' | '[' | '{' => depth += 1,
            ')' | ']' | '}' => {
                if depth == 0 {
                    return &code[start..start + i];
                }
                depth -= 1;
            }
            ',' if depth == 0 => return &code[start..start + i],
            _ => {}
        }
    }
    ""
}

/// `a` joined with `b`, `/`-separated, the way a path join reads when `b` is relative.
fn posix_join(a: &str, b: &str) -> String {
    if b.starts_with('/') || a.is_empty() {
        b.to_owned()
    } else if a.ends_with('/') {
        format!("{a}{b}")
    } else {
        format!("{a}/{b}")
    }
}

/// A `/`-separated path with `.` and `..` folded, the way `posixpath.normpath` reads it.
fn normpath(p: &str) -> String {
    let absolute = p.starts_with('/');
    let mut parts: Vec<&str> = Vec::new();
    for seg in p.split('/') {
        match seg {
            "" | "." => {}
            ".." => {
                if parts.last().is_some_and(|l| *l != "..") {
                    parts.pop();
                } else if !absolute {
                    parts.push("..");
                }
            }
            s => parts.push(s),
        }
    }
    let joined = parts.join("/");
    match (absolute, joined.is_empty()) {
        (true, _) => format!("/{joined}"),
        (false, true) => ".".to_owned(),
        (false, false) => joined,
    }
}

/// A deliberately limited evaluator of a destination expression; `None` for anything it cannot
/// follow.
fn resolve(
    expr: &str,
    prefix: &str,
    literals: &[Literal],
    krate: &str,
    depth: u32,
) -> Option<String> {
    if depth > 12 {
        return None;
    }
    let expr = expr.trim().trim_start_matches('&').trim();
    if let Some(m) = LITERAL.captures(expr) {
        let k: usize = m[1].parse().ok()?;
        return literals.get(k)?.0.clone();
    }
    if TEMP_DIR.is_match(expr) {
        return Some(TEMPORARY.to_owned());
    }
    if WS_ROOT.is_match(expr) {
        return Some(WORKSPACE.to_owned());
    }
    if let Some(rest) = expr.strip_prefix("env!(") {
        let arg = first_arg(rest, 0);
        match resolve(arg, prefix, literals, krate, depth + 1).as_deref() {
            Some("CARGO_MANIFEST_DIR") => return Some(krate.to_owned()),
            // Cargo's scratch folder for integration tests, inside its target directory.
            Some("CARGO_TARGET_TMPDIR") => return Some(TEMPORARY.to_owned()),
            _ => {}
        }
    }
    if let Some(m) = JOIN.captures(expr) {
        let base = resolve(&m[1], prefix, literals, krate, depth + 1)?;
        let tail = resolve(&m[2], prefix, literals, krate, depth + 1)?;
        return Some(posix_join(
            &base.replace('\\', "/"),
            &tail.replace('\\', "/"),
        ));
    }
    if let Some(m) = PATH_NEW.captures(expr) {
        return resolve(&m[1], prefix, literals, krate, depth + 1);
    }
    if IDENT.is_match(expr) {
        // Only bindings in the current function are candidates: a same-named local of an earlier
        // function is not this one.
        let local = FN_HEAD
            .find_iter(prefix)
            .last()
            .map_or(prefix, |m| &prefix[m.start()..]);
        let binding = Regex::new(&format!(
            r"(?s)\blet\s+(?:mut\s+)?{}\s*(?::[^=;]+)?=\s*([^;]+);",
            regex::escape(expr)
        ))
        .ok()?;
        if let Some(m) = binding.captures_iter(local).last() {
            let whole = m.get(0)?;
            return resolve(&m[1], &local[..whole.start()], literals, krate, depth + 1);
        }
    }
    None
}

/// One finding: the line and the reason.
pub type Issue = (usize, &'static str);

/// Scan one source file, `path` relative to the workspace. `tracked` is the index's file list and
/// `roots` the checkout's absolute roots, lowercased with `/` separators. Returns the findings and
/// how many output calls could not be resolved.
#[cfg(test)]
pub fn scan_source(
    path: &str,
    source: &str,
    tracked: &BTreeSet<String>,
    roots: &BTreeSet<String>,
    case_insensitive: bool,
) -> (Vec<Issue>, usize) {
    // Windows checkout paths alias across case; POSIX membership stays literal.
    let folded = fold_set(tracked, case_insensitive);
    scan_folded(path, source, &folded, roots, case_insensitive)
}

/// `tracked` lowercased when membership is case-insensitive.
fn fold_set(tracked: &BTreeSet<String>, case_insensitive: bool) -> BTreeSet<String> {
    if case_insensitive {
        tracked.iter().map(|p| p.to_lowercase()).collect()
    } else {
        tracked.clone()
    }
}

/// `scan_source` against a tracked set already folded for `case_insensitive`.
fn scan_folded(
    path: &str,
    source: &str,
    tracked: &BTreeSet<String>,
    roots: &BTreeSet<String>,
    case_insensitive: bool,
) -> (Vec<Issue>, usize) {
    let fold = |s: &str| {
        if case_insensitive {
            s.to_lowercase()
        } else {
            s.to_owned()
        }
    };
    let (code, literals) = code_and_literals(source);
    let mut issues = Vec::new();
    let mut unresolved = 0;
    // The crate is the nearest folder with a tracked manifest (the folder `CARGO_MANIFEST_DIR`
    // names), else the first two folders of the path.
    let parts: Vec<&str> = path.split('/').collect();
    let parts = &parts[..parts.len().saturating_sub(1)];
    let mut krate = path.split('/').take(2).collect::<Vec<_>>().join("/");
    for n in (1..=parts.len()).rev() {
        let dir = parts[..n].join("/");
        if tracked.contains(&fold(&format!("{dir}/Cargo.toml"))) {
            krate = dir;
            break;
        }
    }
    for (index, (value, line)) in literals.iter().enumerate() {
        let Some(value) = value else {
            continue;
        };
        let normalized = value.replace('\\', "/").to_lowercase();
        let normalized = normalized.trim_end_matches('/');
        if !roots
            .iter()
            .any(|r| normalized == r || normalized.starts_with(&format!("{r}/")))
        {
            continue;
        }
        // A named constant that is a read-only installation input, not an output directory.
        // Nothing else in its file may carry a checkout path.
        let declared = INSTALL_DECL
            .captures_iter(&code)
            .any(|m| m[1].parse::<usize>().ok() == Some(index));
        let fallback = INSTALL_FALLBACK.captures_iter(&code).any(|m| {
            m[2].parse::<usize>().ok() == Some(index)
                && LITERAL
                    .captures(&m[1])
                    .and_then(|v| v[1].parse::<usize>().ok())
                    .and_then(|k| literals.get(k))
                    .and_then(|l| l.0.as_deref())
                    == Some("DERETH_TEST_DAT_DIR")
        });
        let excepted = (path == "core/dat/src/store.rs" && declared)
            || (path == "dereth/client/tests/gpu/world/house_barrier.rs" && fallback);
        if !excepted {
            issues.push((*line, "absolute checkout path in executable literal"));
        }
    }
    for m in WRITER.find_iter(&code) {
        let expr = first_arg(&code, m.end());
        let Some(value) = resolve(expr, &code[..m.start()], &literals, &krate, 0) else {
            unresolved += 1;
            continue;
        };
        let mut value = value.replace('\\', "/");
        if value.starts_with(TEMPORARY) {
            continue;
        }
        let line = line_of(&code, m.start());
        // Where the destination lands, relative to the workspace: a workspace-root join is already
        // relative to it (and is compared with the index as that relative path), a
        // manifest-directory join starts with the crate, and a bare relative path is relative to
        // the crate (the directory tests run in).
        let within = if let Some(rest) = value.strip_prefix(WORKSPACE) {
            value = rest.trim_start_matches('/').to_owned();
            Some(normpath(if value.is_empty() { "." } else { &value }))
        } else if value.starts_with(&krate) {
            Some(normpath(&value))
        } else if value.starts_with('/') || DRIVE.is_match(&value) {
            None
        } else {
            Some(normpath(&format!("{krate}/{value}")))
        };
        if within
            .as_deref()
            .is_some_and(|w| w == ".." || w.starts_with("../"))
        {
            issues.push((line, "static output destination outside the workspace"));
        }
        let mut candidates: BTreeSet<String> =
            [normpath(&value), normpath(&format!("{krate}/{value}"))]
                .into_iter()
                .collect();
        let lower = value.to_lowercase();
        for root in roots {
            if lower.starts_with(&format!("{root}/")) {
                candidates.insert(value[root.len() + 1..].to_owned());
            }
        }
        if candidates.iter().any(|p| tracked.contains(&fold(p))) {
            issues.push((line, "static output destination is tracked"));
        }
    }
    (issues, unresolved)
}

fn git(root: &Path, args: &[&str]) -> Result<Vec<u8>, String> {
    let out = Command::new("git")
        .arg("-C")
        .arg(root)
        .args(args)
        .output()
        .map_err(|e| format!("git {}: {e}", args.join(" ")))?;
    if !out.status.success() {
        return Err(format!(
            "git {} failed: {}",
            args.join(" "),
            String::from_utf8_lossy(&out.stderr).trim()
        ));
    }
    Ok(out.stdout)
}

/// The checkout's absolute roots as the scan compares them: this checkout and, for a worktree, the
/// main checkout it belongs to.
fn checkout_roots(root: &Path) -> Result<BTreeSet<String>, String> {
    let form = |p: &Path| {
        let p = std::fs::canonicalize(p).unwrap_or_else(|_| p.to_path_buf());
        let s = p.to_string_lossy().replace('\\', "/");
        s.strip_prefix("//?/").unwrap_or(&s).to_lowercase()
    };
    let mut roots = BTreeSet::from([form(root)]);
    if root.join(".git").exists() {
        let out = git(
            root,
            &["rev-parse", "--path-format=absolute", "--git-common-dir"],
        )?;
        let common = PathBuf::from(String::from_utf8_lossy(&out).trim());
        if common.file_name().is_some_and(|n| n == ".git") {
            if let Some(parent) = common.parent() {
                roots.insert(form(parent));
            }
        }
    }
    Ok(roots)
}

/// The result of a tree check.
#[derive(Debug)]
pub struct Checked {
    pub sources: usize,
    pub issues: Vec<(String, usize, &'static str)>,
    pub unresolved: usize,
}

/// Check every tracked Rust source under `root`, the workspace. An empty index is an error, never
/// a clean result.
pub fn check(root: &Path) -> Result<Checked, String> {
    let out = git(root, &["ls-files", "-z"])?;
    let paths: BTreeSet<String> = out
        .split(|&c| c == 0)
        .filter(|p| !p.is_empty())
        .map(|p| String::from_utf8_lossy(p).into_owned())
        .collect();
    let sources: Vec<&String> = paths.iter().filter(|p| p.ends_with(".rs")).collect();
    if sources.is_empty() {
        return Err("no workspace Rust sources in the inventory".to_owned());
    }
    let roots = checkout_roots(root)?;
    let folded = fold_set(&paths, cfg!(windows));
    let mut issues = Vec::new();
    let mut unresolved = 0;
    for path in &sources {
        let text = std::fs::read_to_string(root.join(path.as_str()))
            .map_err(|e| format!("{path}: {e}"))?;
        let (found, unknown) = scan_folded(path, &text, &folded, &roots, cfg!(windows));
        issues.extend(found.into_iter().map(|(l, r)| ((*path).clone(), l, r)));
        unresolved += unknown;
    }
    Ok(Checked {
        sources: sources.len(),
        issues,
        unresolved,
    })
}

/// Run the check over `root` and print it; the exit code.
pub fn run(root: &Path) -> i32 {
    match check(root) {
        Err(e) => {
            eprintln!("output hygiene: {e}");
            2
        }
        Ok(c) => {
            println!(
                "output hygiene: {} Rust source(s), {} issue(s), {} output call(s) unresolved",
                c.sources,
                c.issues.len(),
                c.unresolved
            );
            for (path, line, reason) in &c.issues {
                println!("  FAIL  {path}:{line}  {reason}");
            }
            println!(
                "Unresolved helper and environment destinations need review; this is a static \
                 source guard."
            );
            i32::from(!c.issues.is_empty())
        }
    }
}

/// `cargo xtask output-hygiene [ROOT]`.
pub fn output_hygiene(args: &[String]) -> i32 {
    match args {
        [] => run(&workspace_root()),
        [root] if !root.starts_with('-') => run(Path::new(root)),
        _ => {
            eprintln!("usage: cargo xtask output-hygiene [workspace root]");
            2
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn set(items: &[&str]) -> BTreeSet<String> {
        items.iter().map(|s| (*s).to_owned()).collect()
    }

    /// The tracked set every scan below reads against: a root fixture, the workspace manifest and
    /// the example crate's manifest, which makes `dereth/example` the crate.
    fn tracked() -> BTreeSet<String> {
        set(&[
            "fixtures/expected.png",
            "Cargo.toml",
            "dereth/example/Cargo.toml",
        ])
    }

    fn scan_at(path: &str, source: &str) -> (Vec<Issue>, usize) {
        scan_source(
            path,
            source,
            &tracked(),
            &set(&["c:/checkout", "/checkout"]),
            false,
        )
    }

    fn scan(source: &str) -> (Vec<Issue>, usize) {
        scan_at("dereth/example/tests/cpu/main.rs", source)
    }

    fn reasons(source: &str) -> Vec<&'static str> {
        scan(source).0.into_iter().map(|(_, r)| r).collect()
    }

    /// Writing, creating or saving onto a tracked path fails, whether it is named from the
    /// workspace or from the crate the test runs in; reading it does not.
    #[test]
    fn tracked_output_calls_fail_and_readers_do_not() {
        for expression in [
            r#"std::fs::write("fixtures/expected.png", bytes)"#,
            r#"std::fs::write("../../fixtures/expected.png", bytes)"#,
            r#"std::fs::File::create("../../fixtures/expected.png")"#,
            r#"image.save("../../fixtures/expected.png")"#,
            r#"std::fs::write("Cargo.toml", bytes)"#,
        ] {
            let found = reasons(&format!("fn test() {{ {expression}; }}"));
            assert!(
                found.contains(&"static output destination is tracked"),
                "{expression}: {found:?}"
            );
        }
        assert!(
            scan(r#"fn test() { std::fs::read("../../fixtures/expected.png"); }"#)
                .0
                .is_empty()
        );
    }

    /// A destination reached through local bindings and nested joins is still followed.
    #[test]
    fn local_bindings_and_nested_joins_reach_tracked_output() {
        let source = r#"fn test() {
            let dir = workspace_root().join("fixtures");
            let path = dir.join("expected.png");
            std::fs::write(&path, bytes);
        }"#;
        assert!(!scan(source).0.is_empty());
        assert!(!scan(
            r#"fn test() { File::create(Path::new("../../fixtures").join("expected.png")); }"#
        )
        .0
        .is_empty());
    }

    /// A binding of the same name in an earlier function is not followed.
    #[test]
    fn a_binding_in_an_earlier_function_is_not_borrowed() {
        let source = r#"fn earlier() { let path = workspace_root().join("fixtures/expected.png"); }
            fn test() { std::fs::write(&path, bytes); }"#;
        assert_eq!(scan(source), (vec![], 1));
    }

    /// On Windows the tracked set is matched without regard to case; on POSIX it is literal.
    #[test]
    fn windows_tracked_membership_folds_both_path_cases() {
        let source = r#"fn test() { File::create("Fixtures/Expected.png"); }"#;
        for tracked in [
            set(&["dereth/example/Fixtures/Expected.png"]),
            set(&["DERETH/EXAMPLE/FIXTURES/EXPECTED.PNG"]),
        ] {
            let at = "dereth/example/src/lib.rs";
            assert!(!scan_source(at, source, &tracked, &set(&[]), true)
                .0
                .is_empty());
            let posix = scan_source(at, source, &tracked, &set(&[]), false).0;
            assert_eq!(
                posix.is_empty(),
                tracked.contains("DERETH/EXAMPLE/FIXTURES/EXPECTED.PNG")
            );
        }
    }

    /// Output into a temporary directory is allowed, and a call inside a comment is not code.
    #[test]
    fn temporary_output_and_comments_are_allowed() {
        let source = r#"// std::fs::write("../../../fixtures/expected.png", bytes);
        /* File::create("/checkout/fixtures/expected.png"); */
        fn test() {
            let dir = std::env::temp_dir().join("fixture-test");
            std::fs::write(dir.join("expected.png"), bytes);
        }"#;
        assert!(scan(source).0.is_empty());
    }

    /// An absolute checkout path in a literal fails, raw Windows spellings and other cases
    /// included; a sibling folder whose name merely begins the same way does not.
    #[test]
    fn absolute_checkout_paths_include_raw_windows_literals() {
        for value in [
            r#""/checkout/file""#,
            r#"r"C:\checkout\file""#,
            r#""c:/CHECKOUT/file""#,
        ] {
            assert!(
                !scan(&format!("fn test() {{ let path = {value}; }}"))
                    .0
                    .is_empty(),
                "{value}"
            );
        }
        assert!(scan(r#"fn test() { let path = "/checkout-other/file"; }"#)
            .0
            .is_empty());
    }

    /// The install-directory exception covers one declaration in one file, and nothing beside it.
    #[test]
    fn the_install_input_exception_is_one_declaration_only() {
        let path = "core/dat/src/store.rs";
        let source = r#"pub const DEFAULT_CLIENT_DIR: &str = "c:/checkout/data";"#;
        assert!(scan_at(path, source).0.is_empty());
        let more = format!("{source}\nconst OUTPUT: &str = \"c:/checkout/output\";");
        assert!(!scan_at(path, &more).0.is_empty());
    }

    /// A destination that climbs out of the workspace fails; one that stays inside, a scratch
    /// folder and a read do not.
    #[test]
    fn output_that_climbs_out_of_the_workspace_fails() {
        for expression in [
            r#"File::create(Path::new(env!("CARGO_MANIFEST_DIR")).join("../../../runtime/shot.png"))"#,
            r#"std::fs::write(workspace_root().join("../build/out.tsv"), bytes)"#,
            r#"std::fs::write("../../../out.txt", bytes)"#,
        ] {
            assert!(
                reasons(&format!("fn test() {{ {expression}; }}"))
                    .contains(&"static output destination outside the workspace"),
                "{expression}"
            );
        }
        for expression in [
            r#"File::create(Path::new(env!("CARGO_MANIFEST_DIR")).join("../../target/census.txt"))"#,
            r#"std::fs::write(workspace_root().join("target/out.tsv"), bytes)"#,
            r#"std::fs::write(Path::new(env!("CARGO_TARGET_TMPDIR")).join("../../../x"), bytes)"#,
            r#"std::fs::read(Path::new(env!("CARGO_MANIFEST_DIR")).join("../../../private/x"))"#,
        ] {
            assert!(
                scan(&format!("fn test() {{ {expression}; }}")).0.is_empty(),
                "{expression}"
            );
        }
    }

    /// The crate is the folder with the nearest tracked manifest: three levels up from a crate
    /// two folders deeper than the default reading stays inside.
    #[test]
    fn the_crate_is_the_folder_with_the_nearest_manifest() {
        let path = "dereth/client/crates/render/tests/cpu/main.rs";
        let source = r#"fn test() { File::create(Path::new(env!("CARGO_MANIFEST_DIR")).join("../../../target/x")); }"#;
        let with = set(&["Cargo.toml", "dereth/client/crates/render/Cargo.toml"]);
        assert!(scan_source(path, source, &with, &set(&[]), false)
            .0
            .is_empty());
        assert!(
            !scan_source(path, source, &set(&["Cargo.toml"]), &set(&[]), false)
                .0
                .is_empty()
        );
    }

    /// The integration-test scratch folder is temporary output.
    #[test]
    fn the_integration_test_scratch_folder_is_temporary_output() {
        let source = r#"fn test() { let dir = PathBuf::from(env!("CARGO_TARGET_TMPDIR")).join("shots"); std::fs::write(dir.join("frame.png"), bytes); }"#;
        assert_eq!(scan(source), (vec![], 0));
    }

    /// The dat fallback helper is an exact input shape, not a file exemption.
    #[test]
    fn the_dat_fallback_is_an_exact_input_helper_not_a_file_exemption() {
        let path = "dereth/client/tests/gpu/world/house_barrier.rs";
        let source = r#"fn client_dir() -> PathBuf { PathBuf::from(std::env::var("DERETH_TEST_DAT_DIR").unwrap_or_else(|_| r"C:\checkout\data".into())) }"#;
        assert!(scan_at(path, source).0.is_empty());
        let more = format!("{source}\nfn output() {{ File::create(\"C:/checkout/out\"); }}");
        assert!(!scan_at(path, &more).0.is_empty());
        let renamed = source.replace("DERETH_TEST_DAT_DIR", "OUTPUT_DIR");
        assert!(!scan_at(path, &renamed).0.is_empty());
    }

    /// An output call the evaluator cannot follow is counted as unresolved, never proven safe.
    #[test]
    fn unknown_output_is_reported_as_unresolved_not_proven_safe() {
        assert_eq!(
            scan("fn test() { File::create(destination_from_environment()); }"),
            (vec![], 1)
        );
    }

    /// The index is the inventory: no repository is an error, an empty one is an error, and a
    /// planted tracked destination is found and then cleared by turning the write into a read.
    #[test]
    fn the_index_is_the_inventory_and_an_empty_one_fails_closed() {
        let root =
            std::env::temp_dir().join(format!("xtask-output-hygiene-{}", std::process::id()));
        std::fs::remove_dir_all(&root).ok();
        std::fs::create_dir_all(&root).expect("a temp directory");
        assert!(check(&root).is_err(), "not a repository");
        let git = |args: &[&str]| {
            let ok = Command::new("git")
                .arg("-C")
                .arg(&root)
                .args(args)
                .status()
                .is_ok_and(|s| s.success());
            assert!(ok, "git {args:?}");
        };
        git(&["init", "-q"]);
        let err = check(&root).expect_err("an empty index");
        assert!(err.contains("no workspace Rust"), "{err}");
        let rel = "dereth/example/tests/cpu/main.rs";
        std::fs::create_dir_all(root.join("dereth/example/tests/cpu")).expect("dirs");
        std::fs::write(
            root.join(rel),
            r#"fn test() { std::fs::write("../../../fixtures/expected.png", bytes); }"#,
        )
        .expect("write");
        std::fs::create_dir_all(root.join("fixtures")).expect("dirs");
        std::fs::write(root.join("fixtures/expected.png"), b"x").expect("write");
        git(&["add", rel, "fixtures/expected.png"]);
        let found = check(&root).expect("a check");
        assert_eq!(found.sources, 1);
        assert_eq!(found.issues.len(), 1);
        assert_eq!(run(&root), 1);
        std::fs::write(
            root.join(rel),
            r#"fn test() { std::fs::read("../../../fixtures/expected.png"); }"#,
        )
        .expect("write");
        assert_eq!(run(&root), 0);
        std::fs::remove_dir_all(&root).ok();
    }

    /// Comments are blanked in place and literals replaced where they stood, so line numbers
    /// survive; a lifetime is not a char literal.
    #[test]
    fn the_lexer_keeps_offsets_and_tells_a_lifetime_from_a_char() {
        let (code, lits) =
            code_and_literals("fn f<'a>(x: &'a str) {\n// c\nlet c = 'x'; let s = \"a\\\"b\";\n}");
        assert!(code.contains("fn f<'a>(x: &'a str)"));
        assert_eq!(lits.len(), 2);
        assert_eq!(lits[1], (Some("a\"b".to_owned()), 3));
        assert_eq!(code.lines().count(), 4);
        assert_eq!(literal_value(r##"r#"C:\x"#"##), Some(r"C:\x".to_owned()));
        assert_eq!(normpath("a/b/../../../c"), "../c");
    }
}
