//! The bans clippy cannot express.
//!
//! Two rules, both of them things a reimplementation gets wrong silently:
//!
//! 1. **`as i32` on a float in an engine crate.** The original truncates every float toward zero
//!    and yields the x86 integer-indefinite value `0x80000000` for NaN and for anything outside
//!    `i32` range. Rust's `as` saturates instead, so the two disagree by the full width of the
//!    type on NaN and on positive overflow -- a clamp where there should be a wrap, once per
//!    frame, in geometry the player can see. Use `dereth_primitives::num::to_i32` / `floor_to_i32` /
//!    `to_i32_f64`.
//!
//!    The rule is stated as "`as i32` anywhere in an engine crate". Taken literally it
//!    also flags `w(0x34) as i32` on a `u32` dat header field, which the rule was never about, and
//!    a lint that cries wolf gets turned off. So the check fires when the cast appears alongside a
//!    float token — `f32`, `f64`, `.floor()`, `.ceil()`, `.round()`, `.trunc()`, `.sqrt()`,
//!    `.abs()`, or a decimal float literal — on the line or the two lines above it. Clippy's
//!    `cast_possible_truncation`, denied by `cargo xtask clippy`, is the precise enforcement; this
//!    is the backstop that survives an `#[allow]`.
//!
//! 2. **An unannotated `HashMap` in a crate where iteration order is contract.** Several UI panels
//!    are built by walking a hash table and appending with no sort, so the panel order *is* the hash
//!    order; the physics and object-maintenance sweeps enumerate tables where the order decides
//!    which of two simultaneous collisions is reported first; the packet retransmit order and the
//!    ambient sound schedule fall out of a heap's tie-breaking. `std::collections::HashMap`
//!    iterates in a randomised order that changes per process, so a `HashMap` in one of those
//!    crates is either a bug or a deliberate choice that needs saying out loud. The crate list is
//!    [`ORDER_CONTRACT`], derived from the exact-order contract for observable collections.
//!
//! Both are escapable, because a blanket ban with no escape hatch gets disabled. The escape is a
//! comment on the same line or the line above:
//!
//! ```text
//!     // LINT-OK: index arithmetic, both operands already bounded by the 9x9 grid
//!     let i = x as i32;
//!
//!     // ORDER-OK: keyed by DataID and only ever looked up, never iterated
//!     let cache: HashMap<u32, Surface> = HashMap::new();
//! ```
//!
//! The seam checks in [`crate::seams`] are appended to the same table.
//!
//! A third, advisory check counts `unwrap()`/`expect()` outside tests. They are not allowed in
//! library code; it is reported rather than enforced because distinguishing a test helper from
//! library code by text alone produces false positives.
//!
//! ## Acknowledged findings
//!
//! `tools/xtask/lint-acknowledged.txt` lists findings that exist today and belong to someone
//! else to resolve. They print as `ACK` and do not fail the build; anything not on the list does. A
//! known problem is a row naming who owns it and why, not a silently disabled rule. The lint
//! reports the diff; whoever owns the code decides whether to annotate or to change the
//! container. **Do not add a row for your own code** — annotate
//! it instead. A row is for code that is not yours to touch.

use std::path::{Path, PathBuf};

use crate::util::{print_table, workspace_root, Outcome, Report};

/// Crates exempt from the `as i32` ban.
///
/// `dereth-primitives` owns the conversion helpers (`dereth_primitives::num`) and contains the one sanctioned
/// cast, inside the range check that makes it exact. `xtask` is tooling, not engine code.
const CAST_EXEMPT: &[&str] = &["dereth-primitives", "xtask"];

/// Crates where iteration order is observable. The exact-order contract for observable
/// collections requires:
///
/// * UI panels (option groups, property lists, spell tabs) are built by walking a hash
///   table and appending with no sort, so the panel order *is* the hash order;
/// * object maintenance and physics sweeps enumerate hash tables, and the order can
///   decide which of two simultaneous collisions is reported first;
/// * the packet retransmit order and the ambient sound schedule fall out of a priority queue's
///   tie-breaking;
/// * the alpha list and the animation hook list are insertion-ordered.
///
/// Crates not on this list still get `cargo clippy`; they just do not get this ban.
pub const ORDER_CONTRACT: &[&str] = &[
    "dereth-ui",
    "dereth-ui-screens",
    "dereth-physics",
    "dereth-client-model",
    // The fellowship roster and the enchantment lists live here.
    "dereth-rules",
    "dereth-client-net",
    "dereth-transport",
    "dereth-audio",
    "dereth-world-render",
    "dereth-animation",
];

/// Paths inside an [`ORDER_CONTRACT`] crate that the HashMap ban does not cover.
///
/// `core/client-net/src/client_session` is the session layer, which is not itself under the
/// contract: `dereth-client-net` is on [`ORDER_CONTRACT`] for the retransmit order, a transport
/// property shared with `dereth-transport`, and sharing a crate with it does not by itself put the
/// session's per-object tables under the ban. Whether they belong under the contract is a decision
/// for whoever owns them, not a side effect of where the files sit.
const ORDER_CONTRACT_EXEMPT_PATHS: &[&str] = &["core/client-net/src/client_session"];

/// Tokens that mean an `as i32` on this line is a float conversion rather than an integer one.
const FLOAT_TOKENS: &[&str] = &[
    "f32", "f64", ".floor()", ".ceil()", ".round()", ".trunc()", ".sqrt()", ".abs()", ".powi(",
    ".powf(",
];

#[derive(Debug)]
struct Finding {
    file: PathBuf,
    line: usize,
    text: String,
    rule: &'static str,
    acknowledged: bool,
}

/// One acknowledged finding: a path fragment and the rule keyword it excuses.
#[derive(Debug)]
struct Ack {
    path: String,
    rule: String,
}

/// Read `tools/xtask/lint-acknowledged.txt`. One row per line:
/// `<path fragment>|<rule keyword>|<owner> — <reason>`. `#` starts a comment.
fn acknowledged() -> Vec<Ack> {
    let path = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("lint-acknowledged.txt");
    let Ok(text) = std::fs::read_to_string(path) else {
        return Vec::new();
    };
    text.lines()
        .map(str::trim)
        .filter(|l| !l.is_empty() && !l.starts_with('#'))
        .filter_map(|l| {
            let mut parts = l.split('|');
            let p = parts.next()?.trim().to_owned();
            let r = parts.next()?.trim().to_owned();
            Some(Ack { path: p, rule: r })
        })
        .collect()
}

fn rust_files(dir: &Path, out: &mut Vec<PathBuf>) {
    let Ok(entries) = std::fs::read_dir(dir) else {
        return;
    };
    for e in entries.flatten() {
        let p = e.path();
        if p.is_dir() {
            let name = p.file_name().and_then(|s| s.to_str()).unwrap_or("");
            if name == "target" || name.starts_with('.') {
                continue;
            }
            rust_files(&p, out);
        } else if p.extension().and_then(|s| s.to_str()) == Some("rs") {
            out.push(p);
        }
    }
}

/// The package name of the crate `path` is in: `<group>/<short>/...` under the workspace root (see
/// [`crate::util::CRATE_GROUPS`]), or `?` for a path outside it.
fn crate_of(path: &Path) -> String {
    let comps_of = |p: &Path| -> Vec<String> {
        p.components()
            .filter_map(|c| c.as_os_str().to_str().map(str::to_owned))
            .collect()
    };
    // The crate a workspace-relative path is in: the longest group that prefixes it, then the
    // directory below that group.
    let in_workspace = |comps: &[String]| -> Option<String> {
        crate::util::CRATE_GROUPS.iter().find_map(|group| {
            let parts: Vec<&str> = group.split('/').collect();
            let prefix_matches =
                comps.len() > parts.len() && comps.iter().zip(&parts).all(|(c, g)| c == g);
            prefix_matches.then(|| crate::util::crate_name(group, &comps[parts.len()]))
        })
    };
    path.strip_prefix(crate::util::workspace_root())
        .ok()
        .and_then(|rel| in_workspace(&comps_of(rel)))
        .unwrap_or_else(|| "?".to_owned())
}

/// True when the line, or the line before it, carries the named escape annotation.
fn annotated(lines: &[&str], idx: usize, tag: &str) -> bool {
    if lines[idx].contains(tag) {
        return true;
    }
    idx > 0 && lines[idx - 1].trim_start().starts_with("//") && lines[idx - 1].contains(tag)
}

/// The first physical line of the statement `idx` belongs to.
///
/// **An escape annotation introduces a statement, not a physical line.** A `thread_local!` member
/// can span two lines -- the type on one, `HashMap::new()` on the next -- so its `// ORDER-OK:`
/// comment sits two lines above the initialiser. Judged line by line, [`annotated`] excuses the
/// declaration and flags the initialiser: **the same statement, judged twice, with opposite
/// answers**, and since `ci tier0` runs `lint` as a step, tier 0 could never pass.
///
/// The walk stops at the first preceding line whose code is empty (blank or comment-only) or ends
/// in `;`, `{` or `}`. **The brace half is the part that stops this swallowing the
/// neighbourhood**, which is the failure that would be worse than the one being fixed:
///
/// ```text
///     // ORDER-OK: keyed memo                <- excuses A, and only A
///     static A: RefCell<HashMap<..>> =       <- the statement the comment introduces
///         RefCell::new(HashMap::new());      <- joined to it: an unterminated line above
///     static B: RefCell<HashMap<..>> =       <- NOT excused: `;` above it ends the walk
///         RefCell::new(HashMap::new());
/// ```
///
/// An annotation sitting above a `thread_local! {` or `lazy_static! {` **header** excuses nothing
/// inside the block: the walk stops at the `{`, and the members are then judged on their own
/// lines. That is deliberately the narrow answer -- a comment above a block does not say *which*
/// member it is about -- and the narrow answer is the loud one. A block body is out of reach for
/// the same reason: `Lazy::new(|| {` ends in `{`, so a `HashMap::new()` inside the closure is
/// judged alone. Only an *unterminated continuation* -- a line the parser could not have ended a
/// statement on -- is joined to the line above it.
///
/// Known limit, stated rather than left to be found: `//` inside a string literal (`"http://x"`)
/// truncates the code the same way it does everywhere else in this file, so such a line reads as
/// unterminated and joins to the line above. That is the approximation the whole scanner already
/// makes; it is recorded here because this function is the first place where it could widen an
/// escape rather than narrow a match.
fn statement_start(lines: &[&str], idx: usize) -> usize {
    let mut start = idx;
    while start > 0 {
        let prev = lines[start - 1].split("//").next().unwrap_or("").trim();
        if prev.is_empty() || prev.ends_with(';') || prev.ends_with('{') || prev.ends_with('}') {
            break;
        }
        start -= 1;
    }
    start
}

/// [`annotated`], but the annotation may also introduce the statement this line continues.
fn annotated_stmt(lines: &[&str], idx: usize, tag: &str) -> bool {
    annotated(lines, idx, tag) || annotated(lines, statement_start(lines, idx), tag)
}

/// The lines the `HashMap`/`HashSet` ban would flag, ignoring the crate list and the test cut.
///
/// Extracted from [`lint`]'s loop so that the rule is reachable from a test. `lint()` cannot be
/// called without scanning 811 files, and **an escape hatch whose only exerciser is a whole-
/// workspace run is an escape hatch nobody can calibrate** -- which is how the line-versus-statement
/// defect above could go unnoticed. `lint` calls this rather than re-deriving the predicate beside it, so the test
/// and the gate cannot drift apart into agreeing with themselves.
fn order_findings(lines: &[&str]) -> Vec<usize> {
    (0..lines.len())
        .filter(|&i| {
            let code = lines[i].split("//").next().unwrap_or(lines[i]);
            (code.contains("HashMap") || code.contains("HashSet"))
                && !annotated_stmt(lines, i, "ORDER-OK")
        })
        .collect()
}

/// True when this line follows a `#[cfg(test)]` in the same file. A crude heuristic, used only by
/// the advisory unwrap check.
fn after_cfg_test(lines: &[&str], idx: usize) -> bool {
    lines[..idx].iter().any(|l| l.contains("#[cfg(test)]"))
}

/// True when a line contains a decimal float literal such as `1.5` or `0.0`.
fn has_float_literal(code: &str) -> bool {
    let b: Vec<char> = code.chars().collect();
    if b.len() < 3 {
        return false;
    }
    (1..b.len() - 1).any(|i| b[i] == '.' && b[i - 1].is_ascii_digit() && b[i + 1].is_ascii_digit())
}

/// True when the cast on this line plausibly has a float operand.
fn looks_like_float_cast(lines: &[&str], idx: usize) -> bool {
    let lo = idx.saturating_sub(2);
    lines[lo..=idx].iter().any(|l| {
        let code = l.split("//").next().unwrap_or(l);
        FLOAT_TOKENS.iter().any(|t| code.contains(t)) || has_float_literal(code)
    })
}

fn record(
    out: &mut Vec<Finding>,
    acks: &[Ack],
    path: &Path,
    idx: usize,
    line: &str,
    rule: &'static str,
    keyword: &str,
) {
    let shown = path.to_string_lossy().replace('\\', "/");
    let acknowledged = acks
        .iter()
        .any(|a| shown.contains(&a.path) && a.rule == keyword);
    out.push(Finding {
        file: path.to_path_buf(),
        line: idx + 1,
        text: line.trim().to_owned(),
        rule,
        acknowledged,
    });
}

pub fn lint() -> i32 {
    let ws = workspace_root();
    let mut files = Vec::new();
    // `dereth/` holds the client's own crates (`dereth/client/crates/`) as well.
    for group in ["core", "dereth", "tools"] {
        rust_files(&ws.join(group), &mut files);
    }
    files.sort();

    let acks = acknowledged();
    let mut findings: Vec<Finding> = Vec::new();
    let mut advisory = 0usize;
    let mut scanned = 0usize;

    for path in &files {
        let Ok(text) = std::fs::read_to_string(path) else {
            continue;
        };
        scanned += 1;
        let krate = crate_of(path);
        let unix = path.to_string_lossy().replace('\\', "/");
        let order_contract = ORDER_CONTRACT.contains(&krate.as_str())
            && !ORDER_CONTRACT_EXEMPT_PATHS.iter().any(|p| unix.contains(p));
        // Integration tests under `tests/` are test code; these bans are about engine code.
        let mut in_tests = path.components().any(|c| c.as_os_str() == "tests");
        let lines: Vec<&str> = text.lines().collect();
        // ...and so is an inline `#[cfg(test)] mod tests`, which by convention sits at the end of the
        // file. Without this the textual scan flags integer-to-integer casts in test helpers -- it
        // cannot see types, so `bx as i32` where `bx: u32` looked like a float conversion. A lint that
        // cries wolf gets ignored, and clippy's `cast_possible_truncation` is the type-aware
        // enforcement for engine code anyway.
        let test_mod_start = lines
            .iter()
            .position(|l| l.trim_start().starts_with("#[cfg(test)]"))
            .unwrap_or(usize::MAX);

        // The ORDER-OK half of the scan, computed once per file by the same function the unit
        // tests calibrate. Membership rather than a second copy of the predicate.
        let order_hits = order_findings(&lines);

        for (i, line) in lines.iter().enumerate() {
            if i >= test_mod_start {
                in_tests = true;
            }
            let code = line.split("//").next().unwrap_or(line);

            if !in_tests
                && !CAST_EXEMPT.contains(&krate.as_str())
                && code.contains(" as i32")
                && looks_like_float_cast(&lines, i)
                && !annotated_stmt(&lines, i, "LINT-OK")
            {
                record(
                    &mut findings,
                    &acks,
                    path,
                    i,
                    line,
                    "as i32 on a float -- use dereth_primitives::num::to_i32 / floor_to_i32, or justify with // LINT-OK:",
                    "as i32",
                );
            }

            if !in_tests && order_contract && order_hits.contains(&i) {
                record(
                    &mut findings,
                    &acks,
                    path,
                    i,
                    line,
                    "HashMap/HashSet -- iteration order is contract; use BTreeMap or justify with // ORDER-OK:",
                    "HashMap",
                );
            }

            if !in_tests
                && !after_cfg_test(&lines, i)
                && (code.contains(".unwrap()") || code.contains(".expect("))
            {
                advisory += 1;
            }
        }
    }

    for f in &findings {
        let rel = f.file.strip_prefix(&ws).unwrap_or(&f.file);
        let mark = if f.acknowledged { "ACK " } else { "" };
        println!(
            "{mark}{}:{}: {}\n    {}",
            rel.display(),
            f.line,
            f.rule,
            f.text
        );
    }
    let live = findings.iter().filter(|f| !f.acknowledged).count();
    let acked = findings.len() - live;

    let mut reports = vec![
        Report::new(
            "as i32 / HashMap bans",
            if live == 0 {
                Outcome::Pass
            } else {
                Outcome::Fail
            },
            format!(
                "{live} unresolved finding(s) across {scanned} file(s); \
                 {acked} acknowledged in xtask/lint-acknowledged.txt"
            ),
        ),
        Report::new(
            "unwrap/expect (advisory)",
            Outcome::Pass,
            format!(
                "{advisory} occurrence(s) outside tests; the brief forbids these in library code"
            ),
        ),
    ];
    // The dependency seams (contract / ui-screens / client-runtime / client crates) ride along with the text bans,
    // so every run of `cargo xtask lint` and of CI tier 0 checks them too.
    reports.extend(crate::seams::reports());
    print_table("xtask lint", &reports)
}

#[cfg(test)]
mod tests {
    use super::*;

    // ORACLE: none needed -- these assert the lint's own text matching, not any client behaviour.
    // They exist because a lint that silently stops firing is worse than no lint.

    #[test]
    fn float_casts_are_distinguished_from_integer_casts() {
        // The case the spec's literal wording gets wrong: a u32 dat header field cast to i32 is not
        // a fidelity question, and flagging it teaches people to disable the rule.
        let integer = vec!["            eng_pack_vnum: w(0x34) as i32,"];
        assert!(!looks_like_float_cast(&integer, 0));

        let float = vec![
            "        let idx = scale as i32;",
            "        let y: f32 = 1.0;",
        ];
        assert!(looks_like_float_cast(&float, 1));

        let literal = vec!["        let n = (x * 0.5) as i32;"];
        assert!(looks_like_float_cast(&literal, 0));

        let floored = vec!["        let n = value.floor() as i32;"];
        assert!(looks_like_float_cast(&floored, 0));
    }

    #[test]
    fn a_float_literal_needs_digits_on_both_sides_of_the_dot() {
        assert!(has_float_literal("let x = 1.5;"));
        assert!(has_float_literal("0.0"));
        assert!(!has_float_literal("self.value"));
        assert!(!has_float_literal("v[0].x"));
        assert!(!has_float_literal(""));
    }

    #[test]
    fn the_escape_hatch_works_on_the_line_and_the_line_above() {
        let same = vec!["let i = x as i32; // LINT-OK: bounded by the 9x9 grid"];
        assert!(annotated(&same, 0, "LINT-OK"));

        let above = vec!["// ORDER-OK: keyed lookup only", "map: HashMap<u32, u32>,"];
        assert!(annotated(&above, 1, "ORDER-OK"));

        let neither = vec!["map: HashMap<u32, u32>,"];
        assert!(!annotated(&neither, 0, "ORDER-OK"));

        // A non-comment line above must not count, or any mention anywhere would excuse the next.
        let bogus = vec!["let s = \"ORDER-OK\";", "map: HashMap<u32, u32>,"];
        assert!(!annotated(&bogus, 1, "ORDER-OK"));
    }

    /// The `thread_local!` block from `dereth/client/crates/ui-screens/src/env.rs` lines 26-32, **verbatim**, which
    /// is the finding `cargo xtask lint` has emitted since `d592296`.
    ///
    /// **Calibration**, and it is hard for the instrument in the way the corpus is hard: the
    /// annotation is two physical lines above the flagged text, separated by a doc comment, and
    /// the flagged text is the *second half of the statement the annotation introduces*. A hatch
    /// tested only on the one-line form passes without ever meeting this shape -- which is exactly
    /// what happened, for the whole life of the repository.
    ///
    /// **The classification, so the next reader does not have to redo it.** `ENUM_DIDS` has five
    /// sites in the workspace, all in `env.rs`: the declaration, two `clear()`, one `get()` and
    /// one `insert()`. There is no `.iter()`, no `.keys()`, no `.values()`, no `for`, and no
    /// `Debug` of the whole map anywhere. `did_by_enum` is a pure function of `(assets, group,
    /// value)` and iterates `DidMapper::enum_to_id`, a different container. So the map's order is
    /// **unobservable**, the `// ORDER-OK:` claim is true, and swapping in a `BTreeMap` would
    /// change nothing a test or a player could see. This is a lint defect, not a code defect --
    /// and the lint defect is the one that would have bitten the next multi-line declaration.
    #[test]
    fn the_escape_hatch_covers_the_statement_it_introduces_not_the_next_physical_line() {
        let env_rs = vec![
            "thread_local! {",
            "    static ENV: RefCell<Option<Env>> = const { RefCell::new(None) };",
            "    /// Memoized asset lookup for [`did_by_enum`]. See its doc comment.",
            "    // ORDER-OK: a memo keyed by (group, value) and only ever looked up.",
            "    static ENUM_DIDS: RefCell<std::collections::HashMap<(u32, u32), Option<dereth_primitives::DataId>>> =",
            "        RefCell::new(std::collections::HashMap::new());",
            "}",
        ];
        assert_eq!(
            statement_start(&env_rs, 5),
            4,
            "line 5 is the continuation of the declaration on line 4"
        );
        assert!(annotated_stmt(&env_rs, 5, "ORDER-OK"));
        assert!(
            order_findings(&env_rs).is_empty(),
            "the one annotated declaration must produce no finding, on either of its two lines"
        );
        // And the old behaviour is pinned as the thing that must not come back: a hatch that looks
        // only at the physical line and its predecessor still flags the initialiser. If this
        // assertion ever fails, `annotated` has been widened and `annotated_stmt` is redundant.
        assert!(
            !annotated(&env_rs, 5, "ORDER-OK"),
            "the physical-line hatch is what missed this; the fix is the statement walk above it"
        );
    }

    /// **Negative control, the one the row asked for**: a `HashMap` three lines below an
    /// unrelated comment must still be flagged. An escape hatch that swallows the neighbourhood is
    /// worse than one that is too narrow, because the narrow one is loud.
    #[test]
    fn an_annotation_three_statements_up_excuses_nothing() {
        let lines = vec![
            "    // ORDER-OK: the vector below is sorted before it is walked",
            "    let mut v = Vec::new();",
            "    v.sort();",
            "    let m: HashMap<u32, u32> = HashMap::new();",
        ];
        assert_eq!(
            statement_start(&lines, 3),
            3,
            "a `;` above ends the statement"
        );
        assert!(!annotated_stmt(&lines, 3, "ORDER-OK"));
        assert_eq!(order_findings(&lines), vec![3]);
    }

    /// **Negative control, the expensive one.** A single annotation above a `thread_local!` /
    /// `lazy_static!` block must not excuse every member of it, and a `HashMap` inside a closure
    /// body must not be excused by an annotation above the closure. Both are stopped by the same
    /// clause -- the walk refuses to climb over `{` or `}` -- and if that clause is deleted, one
    /// comment silences a whole block. This is the mutation to point at [`statement_start`].
    #[test]
    fn the_statement_walk_does_not_climb_out_of_a_block() {
        // (i) An annotation above the block HEADER excuses nothing inside it. The walk stops at
        // the `{`, so each member is judged on its own line -- both are flagged.
        let above_header = vec![
            "// ORDER-OK: A is a keyed memo",
            "thread_local! {",
            "    static A: RefCell<HashMap<u32, u32>> = RefCell::new(HashMap::new());",
            "    static B: RefCell<HashMap<u32, u32>> = RefCell::new(HashMap::new());",
            "}",
        ];
        assert_eq!(
            statement_start(&above_header, 2),
            2,
            "an open brace above must end the walk"
        );
        assert_eq!(
            statement_start(&above_header, 3),
            3,
            "a semicolon above must end the walk"
        );
        assert_eq!(
            order_findings(&above_header),
            vec![2, 3],
            "a comment above a block header does not say which member it is about, so it excuses none"
        );

        // (ii) The same block with the annotation on the member. It excuses that member across
        // both of its physical lines and stops dead at the next one. This is the shape of the
        // real site, and it is the assertion that fails if the brace clause is deleted from
        // `statement_start`: without it, line 5's walk climbs past `{` to the comment on line 0
        // and B goes quiet.
        let per_member = vec![
            "thread_local! {",
            "    // ORDER-OK: A is a keyed memo",
            "    static A: RefCell<HashMap<u32, u32>> =",
            "        RefCell::new(HashMap::new());",
            "    static B: RefCell<HashMap<u32, u32>> =",
            "        RefCell::new(HashMap::new());",
            "}",
        ];
        assert_eq!(
            statement_start(&per_member, 3),
            2,
            "A's initialiser continues A's declaration"
        );
        assert_eq!(
            statement_start(&per_member, 5),
            4,
            "B's initialiser continues B's declaration"
        );
        assert_eq!(
            order_findings(&per_member),
            vec![4, 5],
            "exactly one member is excused, on both its lines; the other is flagged on both of its"
        );

        let closure = vec![
            "// ORDER-OK: the outer container is a Vec",
            "static T: Lazy<Vec<u32>> = Lazy::new(|| {",
            "    let m = HashMap::new();",
            "    m.into_values().collect()",
            "});",
        ];
        assert_eq!(
            statement_start(&closure, 2),
            2,
            "a closure body is not a continuation"
        );
        assert_eq!(order_findings(&closure), vec![2]);
    }

    /// A single-line statement must be unchanged by the walk, in both directions. Without this,
    /// `statement_start` returning a constant `0` would pass every other test in this module by
    /// making the file's first line the annotation for everything.
    #[test]
    fn a_single_line_statement_is_its_own_start() {
        let lines = vec![
            "let a = 1;",
            "let m: HashMap<u32, u32> = HashMap::new();",
            "let b = 2;",
        ];
        for i in 0..lines.len() {
            assert_eq!(statement_start(&lines, i), i, "line {i} stands alone");
        }
        assert_eq!(order_findings(&lines), vec![1]);
    }

    #[test]
    fn crate_names_come_out_of_the_path() {
        let ws = crate::util::workspace_root();
        assert_eq!(
            crate_of(&ws.join("core/transport/src/indicator.rs")),
            "dereth-transport"
        );
        assert_eq!(crate_of(&ws.join("tools/xtask/src/lint.rs")), "xtask");
        assert_eq!(
            crate_of(&ws.join("empyrean/crates/world/src/lib.rs")),
            "empyrean-world"
        );
        assert_eq!(
            crate_of(&ws.join("dereth/client/crates/ui/src/lib.rs")),
            "dereth-ui"
        );
        assert_eq!(
            crate_of(&ws.join("dereth/client/src/main.rs")),
            "dereth-client"
        );
        assert_eq!(
            crate_of(&ws.join("empyrean/import/src/main.rs")),
            "empyrean-import"
        );
        assert_eq!(
            crate_of(&ws.join("empyrean/crates/net/src/lib.rs")),
            "empyrean-net"
        );
        // A path outside the workspace names no crate.
        assert_eq!(crate_of(Path::new("/elsewhere/core/net/src/lib.rs")), "?");
    }

    #[test]
    fn the_order_contract_list_matches_the_fidelity_document() {
        // The observable-order contract names UI panels,
        // physics and object-maintenance sweeps (7.3), the packet and ambient-sound paths (7.5),
        // and the alpha and animation-hook lists (7.6, 7.10). If a crate is added to the workspace
        // for one of those, it belongs here too.
        for expected in [
            "dereth-ui",
            "dereth-physics",
            "dereth-client-net",
            "dereth-transport",
            "dereth-audio",
            "dereth-animation",
        ] {
            assert!(
                ORDER_CONTRACT.contains(&expected),
                "{expected} must be order-contract"
            );
        }
        // dereth-dat and dereth-assets are pure decode: nothing there is enumerated into a visible order.
        assert!(!ORDER_CONTRACT.contains(&"dereth-dat"));
    }
}
