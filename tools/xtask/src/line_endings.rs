//! `cargo xtask line-endings`: a file's line endings read from its bytes, and the tracked tree
//! gated against `.gitattributes`.
//!
//! ```text
//! cargo xtask line-endings FILE [FILE...]       classify the named files
//! cargo xtask line-endings --tree [ROOT]        gate the tracked tree (default: the workspace)
//! cargo xtask line-endings --normalise FILE...  rewrite to the ending .gitattributes gives it
//! ```
//!
//! Prints one line per file: `CRLF`, `LF`, `MIXED n CRLF / m LF`, `EMPTY` or `NONE` (one line with
//! no terminator). Exits 1 if any file is MIXED, so it can gate a write.
//!
//! **Why this reads bytes.** Shell greps for `\r` have given confident wrong answers here more
//! than once: one measured a file as CRLF that was LF and wrote 57 CRLF lines into it; one
//! reported 581 CRLF lines in a file with none; one, handed an empty file list, recursed into a
//! build tree and reported on artefacts instead of source. A shell grep on Windows can strip or
//! normalise `\r` depending on how the pattern is quoted, whether the file is treated as binary
//! and which grep is on PATH, and none of those failures errors. Counting the bytes has no such
//! failure mode.
//!
//! **One source for the rule.** `.gitattributes` says which ending each file has: `text` with
//! `eol=lf` or `eol=crlf`, or `-text` (and `binary`) for files whose bytes are kept exactly as
//! committed. `--tree` asks git what it says about every tracked file (`git check-attr text eol`,
//! one batched call), so an edit to `.gitattributes` is the only edit a new rule needs. It then
//! checks that the tree obeys it:
//!
//! * no governed text file is MIXED: a file that mixes its endings is one a text tool on Windows
//!   rewrote by accident, and a line-based tool can no longer edit it without changing more than
//!   it meant to;
//! * every governed text file has the ending `.gitattributes` gives it.
//!
//! A file whose bytes are kept (`-text`) carries no expectation; the count of those that are mixed
//! is printed, never failed. A binary file (a NUL in the first 8 KB) is counted and skipped, and a
//! file that cannot be read is reported and fails the gate: an unexamined file is not a clean one.
//!
//! The tree may be any git checkout, not only this workspace: a repository that holds the
//! workspace can be checked whole with `--tree <its root>`.

use std::collections::BTreeMap;
use std::io::Write;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};

use crate::util::workspace_root;

/// How a file ends its lines.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Kind {
    Empty,
    /// One line with no terminator.
    None,
    Lf,
    Crlf,
    Mixed,
}

impl Kind {
    pub fn label(self) -> &'static str {
        match self {
            Kind::Empty => "EMPTY",
            Kind::None => "NONE",
            Kind::Lf => "LF",
            Kind::Crlf => "CRLF",
            Kind::Mixed => "MIXED",
        }
    }
}

/// The ending `.gitattributes` gives a file.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Ending {
    Lf,
    Crlf,
}

impl Ending {
    fn kind(self) -> Kind {
        match self {
            Ending::Lf => Kind::Lf,
            Ending::Crlf => Kind::Crlf,
        }
    }
}

/// `(kind, CRLF count, bare-LF count)` of some bytes. A lone `\r` is not a terminator.
pub fn classify_bytes(b: &[u8]) -> (Kind, usize, usize) {
    if b.is_empty() {
        return (Kind::Empty, 0, 0);
    }
    let crlf = b.windows(2).filter(|w| w == b"\r\n").count();
    let lf = b.iter().filter(|&&c| c == b'\n').count() - crlf;
    match (crlf, lf) {
        (0, 0) => (Kind::None, 0, 0),
        (c, 0) => (Kind::Crlf, c, 0),
        (0, l) => (Kind::Lf, 0, l),
        (c, l) => (Kind::Mixed, c, l),
    }
}

fn to_lf(b: &[u8]) -> Vec<u8> {
    let mut out = Vec::with_capacity(b.len());
    let mut i = 0;
    while i < b.len() {
        if b[i] == b'\r' && b.get(i + 1) == Some(&b'\n') {
            i += 1;
            continue;
        }
        out.push(b[i]);
        i += 1;
    }
    out
}

fn to_crlf(b: &[u8]) -> Vec<u8> {
    let lf = to_lf(b);
    let mut out = Vec::with_capacity(lf.len() + lf.len() / 16);
    for &c in &lf {
        if c == b'\n' {
            out.push(b'\r');
        }
        out.push(c);
    }
    out
}

/// `{rel: Some(ending) | None}` from `git check-attr -z text eol` output: `None` where the bytes are
/// kept exactly (`text` unset or unspecified), else the ending `eol` names (LF unless it says
/// `crlf`).
pub fn endings_from_attributes(out: &[u8], rels: &[String]) -> BTreeMap<String, Option<Ending>> {
    let parts: Vec<&[u8]> = out.split(|&c| c == 0).collect();
    let mut attrs: BTreeMap<String, BTreeMap<String, String>> = BTreeMap::new();
    let mut i = 0;
    while i + 2 < parts.len() {
        let rel = String::from_utf8_lossy(parts[i]).into_owned();
        attrs.entry(rel).or_default().insert(
            String::from_utf8_lossy(parts[i + 1]).into_owned(),
            String::from_utf8_lossy(parts[i + 2]).into_owned(),
        );
        i += 3;
    }
    rels.iter()
        .map(|rel| {
            let a = attrs.get(rel);
            let get = |k: &str| {
                a.and_then(|m| m.get(k))
                    .map_or("unspecified", String::as_str)
            };
            let ending = match get("text") {
                "unset" | "unspecified" => None,
                _ if get("eol") == "crlf" => Some(Ending::Crlf),
                _ => Some(Ending::Lf),
            };
            (rel.clone(), ending)
        })
        .collect()
}

/// What `.gitattributes` gives each of `rels` (paths relative to `root`), asked of git in one
/// batched call.
pub fn attribute_endings(
    root: &Path,
    rels: &[String],
) -> Result<BTreeMap<String, Option<Ending>>, String> {
    let mut child = Command::new("git")
        .args(["check-attr", "-z", "--stdin", "text", "eol"])
        .current_dir(root)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .map_err(|e| format!("git check-attr: {e}"))?;
    let input = rels.join("\0").into_bytes();
    let mut stdin = child.stdin.take().expect("stdin is piped");
    // Written from a thread so a large answer cannot fill git's stdout while its stdin is still
    // being fed.
    let feeder = std::thread::spawn(move || stdin.write_all(&input));
    let out = child
        .wait_with_output()
        .map_err(|e| format!("git check-attr: {e}"))?;
    feeder
        .join()
        .map_err(|_| "git check-attr: the input writer panicked".to_owned())?
        .map_err(|e| format!("git check-attr: {e}"))?;
    if !out.status.success() {
        return Err(format!(
            "git check-attr failed: {}",
            String::from_utf8_lossy(&out.stderr).trim()
        ));
    }
    Ok(endings_from_attributes(&out.stdout, rels))
}

/// Every tracked file under `root`, relative to it. `--stage` so that submodule gitlinks (mode
/// 160000) can be dropped: they are directories owned by another repository.
pub fn tracked(root: &Path) -> Result<Vec<String>, String> {
    let out = Command::new("git")
        .args(["ls-files", "-z", "--stage"])
        .current_dir(root)
        .output()
        .map_err(|e| format!("git ls-files: {e}"))?;
    if !out.status.success() {
        return Err(format!(
            "git ls-files failed: {}",
            String::from_utf8_lossy(&out.stderr).trim()
        ));
    }
    let mut rels: Vec<String> = out
        .stdout
        .split(|&c| c == 0)
        .filter(|e| !e.is_empty() && !e.starts_with(b"160000 "))
        .filter_map(|e| {
            let tab = e.iter().position(|&c| c == b'\t')?;
            Some(String::from_utf8_lossy(&e[tab + 1..]).into_owned())
        })
        .collect();
    rels.sort();
    Ok(rels)
}

/// The gate's decision, as data.
///
/// `kinds` is every text file examined, `expected` the ending each one is given. Returns the
/// governed files that are MIXED, the governed uniform files with the other ending (with what they
/// are and what they should be), and the byte-kept files that are MIXED.
pub type Verdict = (Vec<String>, Vec<(String, Kind, Ending)>, Vec<String>);

pub fn verdict(
    kinds: &BTreeMap<String, Kind>,
    expected: &BTreeMap<String, Option<Ending>>,
) -> Verdict {
    let (mut mixed, mut wrong, mut kept_mixed) = (Vec::new(), Vec::new(), Vec::new());
    for (rel, &kind) in kinds {
        match expected.get(rel).copied().flatten() {
            None => {
                if kind == Kind::Mixed {
                    kept_mixed.push(rel.clone());
                }
            }
            Some(_) if kind == Kind::Mixed => mixed.push(rel.clone()),
            Some(want) if matches!(kind, Kind::Crlf | Kind::Lf) && kind != want.kind() => {
                wrong.push((rel.clone(), kind, want));
            }
            Some(_) => {}
        }
    }
    (mixed, wrong, kept_mixed)
}

fn ending_label(e: Ending) -> &'static str {
    e.kind().label()
}

/// `--tree`: gate every tracked file under `root`. Returns the exit code.
pub fn tree_mode(root: &Path) -> i32 {
    let listed = tracked(root).and_then(|rels| {
        let expected = attribute_endings(root, &rels)?;
        Ok((rels, expected))
    });
    let (rels, expected) = match listed {
        Ok(v) => v,
        Err(e) => {
            println!("no git index to read at {} ({e})", root.display());
            println!("VERDICT: FAIL");
            return 1;
        }
    };
    let mut kinds = BTreeMap::new();
    let mut binary = 0usize;
    let mut unreadable = Vec::new();
    for rel in &rels {
        match std::fs::read(root.join(rel)) {
            Ok(b) => {
                if b[..b.len().min(8192)].contains(&0) {
                    binary += 1;
                    continue;
                }
                kinds.insert(rel.clone(), classify_bytes(&b).0);
            }
            Err(e) => unreadable.push(format!("{rel} ({e})")),
        }
    }
    let (mixed, wrong, kept_mixed) = verdict(&kinds, &expected);
    let governed = kinds
        .keys()
        .filter(|rel| expected.get(*rel).copied().flatten().is_some())
        .count();

    println!(
        "tracked: {}   text examined: {}   binary skipped: {binary}   unreadable: {}",
        rels.len(),
        kinds.len(),
        unreadable.len()
    );
    println!(
        "governed by .gitattributes: {governed}   bytes kept (-text): {}   of those MIXED: {}",
        kinds.len() - governed,
        kept_mixed.len()
    );
    let mut bad = false;
    for u in &unreadable {
        println!("  COULD NOT EXAMINE: {u}");
        bad = true;
    }
    println!("MIXED: {} governed file(s)", mixed.len());
    for rel in &mixed {
        let (_, crlf, lf) = std::fs::read(root.join(rel))
            .map(|b| classify_bytes(&b))
            .unwrap_or((Kind::Mixed, 0, 0));
        println!("  MIXED  {rel:<58} {crlf} CRLF / {lf} LF");
        bad = true;
    }
    println!(
        "endings: {governed} governed file(s), {} with the wrong ending",
        wrong.len()
    );
    for (rel, kind, want) in &wrong {
        println!(
            "  WRONG ENDING  {rel:<58} is {}, .gitattributes says {}",
            kind.label(),
            ending_label(*want)
        );
        bad = true;
    }
    if !mixed.is_empty() || !wrong.is_empty() {
        println!("  THE FIX: `cargo xtask line-endings --normalise <path>` (bytes in, bytes out,");
        println!("  verified). If the file's bytes must be kept as committed, it belongs under a");
        println!("  `-text` rule in .gitattributes.");
    }
    println!(
        "VERDICT: {}",
        if bad {
            "FAIL"
        } else {
            "no governed file is MIXED and every one has its ending"
        }
    );
    i32::from(bad)
}

/// Rewrite one file to `want`, and prove it from the bytes, writing through `write`.
///
/// The only change allowed is the terminator: the verification re-reads the file and requires that
/// stripping the CRs of each CRLF from before and after gives identical bytes. `want` of `None`
/// (bytes kept) refuses.
pub fn normalise_with(
    path: &Path,
    want: Option<Ending>,
    write: impl Fn(&Path, &[u8]) -> std::io::Result<()>,
) -> i32 {
    let shown = path.display().to_string();
    let before = match std::fs::read(path) {
        Ok(b) => b,
        Err(e) => {
            println!("{shown:<58} ERROR    {e}");
            return 1;
        }
    };
    let Some(want) = want else {
        println!("{shown:<58} REFUSED  .gitattributes keeps its bytes exactly (-text)");
        return 1;
    };
    if before[..before.len().min(8192)].contains(&0) {
        println!("{shown:<58} SKIP     binary");
        return 0;
    }
    if to_lf(&before).contains(&b'\r') {
        println!("{shown:<58} REFUSED  a lone CR is not a line terminator this tool rewrites");
        return 1;
    }
    let after = match want {
        Ending::Crlf => to_crlf(&before),
        Ending::Lf => to_lf(&before),
    };
    if after == before {
        return 0;
    }
    if let Err(e) = write(path, &after) {
        println!("{shown:<58} WRITE FAILED {e}");
        return 1;
    }
    let back = std::fs::read(path).unwrap_or_default();
    let kind = classify_bytes(&back).0;
    if to_lf(&back) != to_lf(&before)
        || !matches!(kind, Kind::Empty | Kind::None) && kind != want.kind()
    {
        println!("{shown:<58} VERIFY FAILED");
        return 1;
    }
    println!(
        "{shown:<58} NORMALISED {} -> {}",
        classify_bytes(&before).0.label(),
        ending_label(want)
    );
    0
}

/// [`normalise_with`] writing through the file system.
pub fn normalise(path: &Path, want: Option<Ending>) -> i32 {
    normalise_with(path, want, |p, d| std::fs::write(p, d))
}

fn normalise_paths(paths: &[String]) -> i32 {
    let mut bad = 0;
    for path in paths {
        let full = match std::fs::canonicalize(path) {
            Ok(p) => p,
            Err(e) => {
                println!("{path:<58} ERROR    {e}");
                bad = 1;
                continue;
            }
        };
        let dir = full
            .parent()
            .map_or_else(|| PathBuf::from("."), Path::to_path_buf);
        let top = Command::new("git")
            .args(["rev-parse", "--show-toplevel"])
            .current_dir(&dir)
            .output()
            .ok()
            .filter(|o| o.status.success())
            .map(|o| PathBuf::from(String::from_utf8_lossy(&o.stdout).trim()));
        let Some(top) = top.and_then(|t| std::fs::canonicalize(t).ok()) else {
            println!("{path:<58} ERROR    not inside a git checkout");
            bad = 1;
            continue;
        };
        let rel = full
            .strip_prefix(&top)
            .map(|r| r.to_string_lossy().replace('\\', "/"))
            .unwrap_or_default();
        match attribute_endings(&top, std::slice::from_ref(&rel)) {
            Ok(map) => bad |= normalise(Path::new(path), map.get(&rel).copied().flatten()),
            Err(e) => {
                println!("{path:<58} ERROR    {e}");
                bad = 1;
            }
        }
    }
    bad
}

const USAGE: &str = "\
usage: cargo xtask line-endings FILE [FILE...]       classify the named files
       cargo xtask line-endings --tree [ROOT]        gate the tracked tree (default: the workspace)
       cargo xtask line-endings --normalise FILE...  rewrite to the ending .gitattributes gives it";

/// `cargo xtask line-endings`.
pub fn line_endings(args: &[String]) -> i32 {
    match args.first().map(String::as_str) {
        None => {
            eprintln!("{USAGE}");
            2
        }
        Some("--tree") => match args.get(1) {
            Some(root) => tree_mode(Path::new(root)),
            None => tree_mode(&workspace_root()),
        },
        Some("--normalise") => {
            if args.len() < 2 {
                eprintln!("--normalise needs at least one path");
                return 2;
            }
            normalise_paths(&args[1..])
        }
        Some(_) => {
            let mut bad = 0;
            for path in args {
                match std::fs::read(path) {
                    Ok(b) => {
                        let (kind, crlf, lf) = classify_bytes(&b);
                        if kind == Kind::Mixed {
                            println!("{path:<60} MIXED  {crlf} CRLF / {lf} LF");
                            bad = 1;
                        } else if crlf + lf > 0 {
                            println!("{path:<60} {}  {}", kind.label(), crlf + lf);
                        } else {
                            println!("{path:<60} {}", kind.label());
                        }
                    }
                    Err(e) => {
                        println!("{path:<60} ERROR {e}");
                        bad = 1;
                    }
                }
            }
            bad
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const CRLF: &[u8] = b"a\r\nb\r\nc\r\n";
    const LF: &[u8] = b"a\nb\nc\n";

    fn scratch(tag: &str) -> PathBuf {
        let dir =
            std::env::temp_dir().join(format!("xtask-line-endings-{tag}-{}", std::process::id()));
        std::fs::remove_dir_all(&dir).ok();
        std::fs::create_dir_all(&dir).expect("a temp directory");
        dir
    }

    /// A CRLF file with one bare-LF line appended is MIXED, and the counts say so; pure files,
    /// empty files and a lone CR are not.
    #[test]
    fn a_file_is_mixed_only_when_it_holds_both_terminators() {
        let mut mixed = CRLF.to_vec();
        mixed.extend_from_slice(b"d\n");
        assert_eq!(classify_bytes(&mixed), (Kind::Mixed, 3, 1));
        assert_eq!(classify_bytes(CRLF).0, Kind::Crlf);
        assert_eq!(classify_bytes(LF).0, Kind::Lf);
        assert_eq!(classify_bytes(b"").0, Kind::Empty);
        assert_eq!(
            classify_bytes(b"a\rb").0,
            Kind::None,
            "a lone CR is not a terminator"
        );
    }

    /// `text` with `eol=lf` is LF and with `eol=crlf` is CRLF; `-text` and an unspecified `text`
    /// keep their bytes.
    #[test]
    fn git_attributes_are_read_into_the_ending_each_file_is_given() {
        let out = b"a.rs\0text\0auto\0a.rs\0eol\0lf\0\
                    b.cmd\0text\0set\0b.cmd\0eol\0crlf\0\
                    c.sql\0text\0unset\0c.sql\0eol\0unspecified\0\
                    d.txt\0text\0unspecified\0d.txt\0eol\0unspecified\0";
        let rels: Vec<String> = ["a.rs", "b.cmd", "c.sql", "d.txt"]
            .map(str::to_owned)
            .to_vec();
        let got = endings_from_attributes(out, &rels);
        assert_eq!(got["a.rs"], Some(Ending::Lf));
        assert_eq!(got["b.cmd"], Some(Ending::Crlf));
        assert_eq!(got["c.sql"], None);
        assert_eq!(got["d.txt"], None);
    }

    fn map<V: Clone>(pairs: &[(&str, V)]) -> BTreeMap<String, V> {
        pairs
            .iter()
            .map(|(k, v)| ((*k).to_owned(), v.clone()))
            .collect()
    }

    /// A MIXED governed file fails; a uniform file with the other ending has the wrong ending in
    /// either direction; files with their endings, a byte-kept MIXED file (counted, not failed)
    /// and files with no ending at all are quiet.
    #[test]
    fn the_gate_fails_a_mixed_or_wrong_ending_and_stays_quiet_otherwise() {
        let expected = map(&[
            ("x.rs", Some(Ending::Lf)),
            ("y.cmd", Some(Ending::Crlf)),
            ("kept.sql", None),
        ]);
        let (mixed, _, _) = verdict(&map(&[("x.rs", Kind::Mixed)]), &expected);
        assert_eq!(mixed, vec!["x.rs".to_owned()]);
        let (_, wrong, _) = verdict(&map(&[("x.rs", Kind::Crlf)]), &expected);
        assert_eq!(wrong, vec![("x.rs".to_owned(), Kind::Crlf, Ending::Lf)]);
        let (_, wrong, _) = verdict(&map(&[("y.cmd", Kind::Lf)]), &expected);
        assert_eq!(wrong, vec![("y.cmd".to_owned(), Kind::Lf, Ending::Crlf)]);
        let quiet = verdict(
            &map(&[
                ("x.rs", Kind::Lf),
                ("y.cmd", Kind::Crlf),
                ("kept.sql", Kind::Crlf),
            ]),
            &expected,
        );
        assert!(quiet.0.is_empty() && quiet.1.is_empty() && quiet.2.is_empty());
        let kept = verdict(&map(&[("kept.sql", Kind::Mixed)]), &expected);
        assert!(kept.0.is_empty() && kept.1.is_empty());
        assert_eq!(kept.2, vec!["kept.sql".to_owned()]);
        let endless = verdict(
            &map(&[("x.rs", Kind::Empty), ("y.cmd", Kind::None)]),
            &expected,
        );
        assert!(endless.0.is_empty() && endless.1.is_empty());
    }

    /// A path past Windows' MAX_PATH is read, not reported unreadable, and a genuinely absent file
    /// still errors rather than reading as empty: a reader that swallowed errors would turn files
    /// it could not read into files reported as examined.
    #[test]
    fn a_path_past_max_path_is_read_and_an_absent_file_still_errors() {
        let mut deep = scratch("deep");
        for _ in 0..4 {
            deep = deep.join("x".repeat(60));
        }
        std::fs::create_dir_all(&deep).expect("a deep directory");
        let long_path = deep.join("probe.md");
        std::fs::write(&long_path, b"a\r\nb\r\nc\n").expect("write the probe");
        assert!(long_path.to_string_lossy().len() > 260);
        let bytes = std::fs::read(&long_path).expect("the long path is read");
        assert_eq!(classify_bytes(&bytes).0, Kind::Mixed);
        assert!(std::fs::read(deep.join("there-is-no-such-file.md")).is_err());
    }

    /// `--normalise` rewrites terminators and nothing else in both directions, refuses a
    /// byte-kept file and leaves it alone, and a writer that changes a byte fails its own
    /// verification.
    #[test]
    fn normalise_rewrites_terminators_only_and_verifies_from_the_disk() {
        let dir = scratch("normalise");
        let probe = dir.join("probe.md");
        std::fs::write(&probe, b"a\r\nb\nc\r\n").expect("write");
        assert_eq!(normalise(&probe, Some(Ending::Lf)), 0);
        assert_eq!(std::fs::read(&probe).expect("read"), b"a\nb\nc\n");
        assert_eq!(normalise(&probe, Some(Ending::Crlf)), 0);
        assert_eq!(std::fs::read(&probe).expect("read"), b"a\r\nb\r\nc\r\n");

        let kept = dir.join("kept.txt");
        std::fs::write(&kept, b"a\r\nb\n").expect("write");
        assert_eq!(normalise(&kept, None), 1);
        assert_eq!(std::fs::read(&kept).expect("read"), b"a\r\nb\n");

        let sabotage = dir.join("sabotage.md");
        std::fs::write(&sabotage, b"a\r\nb\r\n").expect("write");
        let caught = normalise_with(&sabotage, Some(Ending::Lf), |p, d| {
            let mut d = d.to_vec();
            d.push(b'x');
            std::fs::write(p, d)
        });
        assert_eq!(
            caught, 1,
            "a writer that adds a byte must fail verification"
        );
    }
}
