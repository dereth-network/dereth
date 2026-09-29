//! `cargo xtask skip-claims --tree [ROOT]`: the retail-dat skip idiom, and the sentence that
//! promises it, as a gate.
//!
//! ## What this is about
//!
//! `let Some(store) = store() else { return };` at the head of a `#[test]` is a test body that
//! disappears the day the environment moves and still prints `ok`. Measured once: 106 tests across
//! 33 suites passed having read nothing at all, with `DERETH_TEST_DAT_DIR` pointed at an empty
//! directory. The retail install is not tracked and a git worktree has none, so that was what every
//! worktree saw.
//!
//! The gate helpers were converted to a loud shape (`open_from_env_or_fail`, `require_from_env`,
//! `Fixtures::require_retail_data`), which makes the silent shape a type error:
//! `open_from_env_or_fail` returns `Self`, so `let Some(_) = ... else` is `E0308` and cannot be
//! written. Forty comment sites in 37 files still told the reader the suite skipped when the dats
//! were absent, and were corrected by hand. This gate keeps both halves true.
//!
//! ## Scoped by behaviour, not by a phrase
//!
//! A census scoped by an exact sentence once missed three helpers printing the same claim with one
//! extra word. So the code half asks *what does the `Option` get discharged into* rather than *does
//! this file contain a phrase*, and the doc half keys on the co-occurrence of a skip verb with an
//! absence-of-retail-data condition, then asks whether the surrounding block says what happens
//! instead.
//!
//! Comment blocks are normalised per block, never per line: a claim split across a line wrap is
//! invisible to a line matcher.
//!
//! ## The three verdicts, and why the middle one exists
//!
//! * `LIE`: a skip claim whose block never says what happens instead. Fails.
//! * `SUSPECT`: a skip claim whose block does say so, but not beside the claim. Counted and
//!   printed, never fatal. This is the honest *could not classify* bucket: an instrument that
//!   cannot examine part of its space reports that part rather than skipping it.
//! * `SILENT`: code where an `Option` from a retail-dat resolver is discharged by a control-flow
//!   exit that is not a hard stop. Fails.
//!
//! And one number that is reported rather than gated: how many test sites still take
//! `RetailDatStore::open_from_env()` at all. Every one of those is a site where the silent shape is
//! still *expressible*, one keystroke from `else { return }`, even though all of them currently
//! panic. It is a printed backlog rather than a red gate, because a gate that is red from birth is
//! a gate nobody reads (the sweep's known-red table, same reasoning).

use std::collections::BTreeSet;
use std::path::Path;
use std::process::Command;
use std::sync::LazyLock;

use regex::Regex;

use crate::util::workspace_root;

/// A skip verb.
static SKIP: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(
        r"(?i)\b(skips?|skipped|skipping|no-?ops?|does nothing|bails? out|returns? early|passes? vacuously|is inert)\b",
    )
    .expect("SKIP")
});

/// An absence-of-retail-data condition -- not merely the word "dats", which appears in hundreds of
/// blocks describing what the client does.
///
/// The second alternative's closing `\b` matters: without it the literal "No dat" inside "No
/// datagram leaves this process." scored as an absence claim, and a sentence the shard-safety
/// rule requires reddened the gate a little more each time someone complied.
static ABSENT: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(concat!(
        r"(?i)((retail |the )?(dats?|dat files?|retail (files?|data)|oracle)[^.;]{0,40}\b",
        r"(are|is|be)?\s*(absent|missing|not (present|there|installed|available|where)|unavailable)",
        r"|(absent|missing|no|without|lacking)\s+(the\s+)?(retail\s+)?(dats?|dat files?|retail (files?|data))\b",
        r"|clean checkout|DERETH_TEST_DAT_DIR is (unset|not set)|no \$?DERETH_TEST_DAT_DIR",
        r"|worktree with no|not where \$?DERETH_TEST_DAT_DIR)",
    ))
    .expect("ABSENT")
});

/// The block says what happens instead of skipping. Deliberately generous: over-exempting
/// produces a `SUSPECT`, which is printed, while under-exempting produces a false `LIE`, which
/// stops a build.
///
/// It is not allowed to be generous about work-item citations. An earlier list ended with an
/// alternative matching any work-item id, on the reasoning that a block citing the work that
/// retired the idiom must be talking about history. Nearly every doc block cited one, so that
/// single alternative exempted the entire corpus: the live calibration (the retired sentence
/// pasted into a real loud file) came back green, and the tree-wide zero looked exactly like a
/// working gate. Citation-detection is not instead-detection.
static INSTEAD: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(concat!(
        r"(?i)(\bfails?\b|\bfailing\b|\bfailure\b|\bexpects?\b|\bassert|\bpanics?\b",
        r"|\bhard error\b|refus\w+|not a skip|no skips|never a skip",
        r"|rather than|instead of|half-skip|used to|no longer",
        r"|until \d{4}-\d{2}-\d{2}",
        r"|open_from_env_or_fail|require_from_env|require_retail_data",
        r"|open_store_or_fail|require_dats)",
    ))
    .expect("INSTEAD")
});

/// A retail-dat resolver whose result is an `Option`.
static RESOLVER: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(
        r"(RetailDatStore::open_from_env\(\)|\bopen_from_env\(\)|testing::open_store\(\)|retail_data_available\(\))",
    )
    .expect("RESOLVER")
});

/// A hard stop: the else-arm does not fall through to a green test.
static HARD_STOP: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(
        r"(panic!|unreachable!|todo!|unimplemented!|assert!|assert_eq!|\.expect\(|process::exit)",
    )
    .expect("HARD_STOP")
});

/// A skip token in the past tense is a narrative about what the code used to do, not a promise
/// about what it does. "this skipped on *either* half, so a worktree with no client/ printed ok"
/// is the true history of a defect and must stay. Present tense is the promise, and the promise
/// is what rots.
static PAST_TOKEN: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"(?i)^(skipped|bailed|no-?opped)$").expect("PAST_TOKEN"));

static COMMENT_PREFIX: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"^(//!|///|//)\s?").expect("COMMENT_PREFIX"));
static ELSE_OPEN: LazyLock<Regex> = LazyLock::new(|| Regex::new(r"else\s*\{").expect("ELSE_OPEN"));

/// The gate's own tree test. Its comment blocks are still judged; only the code scan leaves it
/// out, because a probe string there may quote the idiom verbatim.
const SELF: &str = "tools/xtask/tests/cpu/gates/retail_dat_skip_claims.rs";

/// Blocks whose skip claim is true, as `(path, distinctive substring, reason)`. Each entry must
/// still match something: an entry that matches nothing is a hard error, because a stale
/// allow-list is indistinguishable from a working gate.
///
/// Empty. The last two entries left when the defect they described was fixed: they were true
/// claims about a gate that reported a skipped oracle as a pass, and deleting that gate's `Skip`
/// outcome made both sentences false, so they were rewritten at their sites. One thing that
/// retirement measured is worth more than the entries: the second did not go stale when its
/// sentence was rewritten, because the replacement quoted the retired rule in order to say it was
/// wrong, and an exemption keyed on a phrase cannot tell a claim from a citation of that claim. An
/// allow-list entry does not reliably expire when the claim does.
const ALLOW: &[(&str, &str, &str)] = &[];

/// Whitespace runs collapsed to one space, ends trimmed.
fn squash(s: &str) -> String {
    s.split_whitespace().collect::<Vec<_>>().join(" ")
}

/// Contiguous runs of `//`, `///` and `//!` lines, as `(first line, text)`, normalised per block.
pub fn norm_blocks(text: &str) -> Vec<(usize, String)> {
    let mut out = Vec::new();
    let mut cur: Vec<String> = Vec::new();
    let mut start = None;
    for (i, line) in text.lines().enumerate() {
        let s = line.trim();
        if s.starts_with("//") {
            start.get_or_insert(i + 1);
            cur.push(COMMENT_PREFIX.replace(s, "").into_owned());
        } else if let Some(st) = start.take() {
            out.push((st, squash(&cur.join(" "))));
            cur.clear();
        }
    }
    if let Some(st) = start {
        out.push((st, squash(&cur.join(" "))));
    }
    out
}

/// A markdown file is one block per paragraph, normalised the same way.
pub fn md_blocks(text: &str) -> Vec<(usize, String)> {
    let mut out = Vec::new();
    let mut cur: Vec<&str> = Vec::new();
    let mut start = None;
    for (i, line) in text.lines().enumerate() {
        if line.trim().is_empty() {
            if let Some(st) = start.take() {
                out.push((st, squash(&cur.join(" "))));
                cur.clear();
            }
        } else {
            start.get_or_insert(i + 1);
            cur.push(line.trim());
        }
    }
    if let Some(st) = start {
        out.push((st, squash(&cur.join(" "))));
    }
    out
}

/// A normalised block split into sentences: at whitespace after `.`, `!` or `?`.
///
/// Crude but adequate for doc prose; an abbreviation like `e.g.` merely joins two sentences, and
/// joining is the safe direction (it can only exempt, producing a `SUSPECT`, never a false `LIE`).
fn sentences(body: &str) -> Vec<&str> {
    let mut out = Vec::new();
    let mut start = 0;
    let mut prev = None;
    let mut chars = body.char_indices().peekable();
    while let Some((i, ch)) = chars.next() {
        if ch.is_whitespace() && matches!(prev, Some('.' | '!' | '?')) {
            out.push(&body[start..i]);
            let mut j = i + ch.len_utf8();
            while let Some(&(k, c)) = chars.peek() {
                if !c.is_whitespace() {
                    break;
                }
                j = k + c.len_utf8();
                chars.next();
            }
            start = j;
            prev = None;
            continue;
        }
        prev = Some(ch);
    }
    out.push(&body[start..]);
    out
}

fn snippet(s: &str) -> String {
    s.chars().take(260).collect()
}

/// A block's verdict.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Claim {
    Lie,
    Suspect,
}

/// The verdict on one normalised block, with the sentence that earned it.
///
/// Every skip token is examined, not just the first: an earlier benign one used to hide a later
/// live claim, which is the "instrument stops at the first member of its space" failure this gate
/// exists to prevent.
pub fn judge_block(body: &str) -> (Option<Claim>, String) {
    let sents = sentences(body);
    let mut best: Option<(Claim, String)> = None;
    for (i, sent) in sents.iter().enumerate() {
        for m in SKIP.captures_iter(sent) {
            let whole = m.get(0).expect("a match");
            let token = m.get(1).expect("the token").as_str();
            // A token wrapped in quotes or backticks is a rendering of a printed line or of code
            // (`printed "skipping" line`), not a claim that this file skips. Every doc that
            // explains the retired idiom quotes the word it retired.
            let b = sent.as_bytes();
            let quoted = |c: Option<&u8>| matches!(c, Some(b'"' | b'`'));
            if whole.start() > 0 && quoted(b.get(whole.start() - 1)) && quoted(b.get(whole.end())) {
                continue;
            }
            // The absence condition may sit in the neighbouring sentence ("...are absent. It does
            // nothing."), so look one either side for it.
            let around = sents[i.saturating_sub(1)..(i + 2).min(sents.len())].join(" ");
            if !ABSENT.is_match(&around) {
                continue;
            }
            // What happens instead must be in the same sentence or one immediately either side --
            // the same window as the absence condition, because a claim and its correction are
            // adjacent prose. A character window was measured not to work: at 260 characters the
            // retired sentence pasted at the top of a module doc fell within reach of that doc's
            // own correct "Fails when the retail dats are absent" and was exempted by it.
            if INSTEAD.is_match(sent) {
                continue;
            }
            if INSTEAD.is_match(&around) || PAST_TOKEN.is_match(token) {
                // A correction in the neighbouring sentence, or a past-tense narrative. Reported,
                // never fatal, and never silently dropped: a live calibration put "These tests
                // bail out when the retail data is missing." above a sentence that happened to
                // say "a tampered file is a hard error rather than a warning", and an earlier
                // draft exempted it outright. A claim one sentence from an unrelated correction
                // is this instrument's blind spot, so it is a printed SUSPECT rather than a zero.
                if best.is_none() {
                    best = Some((Claim::Suspect, snippet(sent)));
                }
                continue;
            }
            return (Some(Claim::Lie), snippet(sent));
        }
    }
    match best {
        Some((c, s)) => (Some(c), s),
        None => (None, String::new()),
    }
}

/// Sites as `(line, source)`.
pub type Sites = Vec<(usize, String)>;

/// Silent discharges of a retail-dat `Option`, and every site that takes one, as
/// `(silent, expressible)` lists of `(line, source)`.
pub fn judge_code(text: &str) -> (Sites, Sites) {
    let lines: Vec<&str> = text
        .lines()
        .filter(|l| !l.trim().starts_with("//"))
        .collect();
    let (mut silent, mut expressible) = (Vec::new(), Vec::new());
    for (i, line) in lines.iter().enumerate() {
        if !RESOLVER.is_match(line) {
            continue;
        }
        expressible.push((i + 1, line.trim().to_owned()));
        if !line.contains("let Some") {
            continue;
        }
        // The else-arm: from this line to the next twelve, which covers every shape in this tree
        // (the longest real one is seven lines).
        let arm = lines[i..(i + 12).min(lines.len())].join("\n");
        let Some(m) = ELSE_OPEN.find(&arm) else {
            continue;
        };
        // Up to the matching close brace, crudely but adequately: the first line whose trimmed
        // text is `}` or `};`.
        let body: Vec<&str> = arm[m.end()..]
            .lines()
            .take_while(|t| !matches!(t.trim(), "}" | "};"))
            .collect();
        let body = body.join("\n");
        if !HARD_STOP.is_match(&body) {
            let src: String = format!("{} else {{{}", line.trim(), body.trim())
                .chars()
                .take(160)
                .collect();
            silent.push((i + 1, src));
        }
    }
    (silent, expressible)
}

fn tracked(root: &Path) -> Result<Vec<String>, String> {
    let out = Command::new("git")
        .args(["ls-files", "-z"])
        .current_dir(root)
        .output()
        .map_err(|e| format!("git ls-files: {e}"))?;
    if !out.status.success() {
        return Err(format!(
            "git ls-files failed: {}",
            String::from_utf8_lossy(&out.stderr).trim()
        ));
    }
    Ok(out
        .stdout
        .split(|&c| c == 0)
        .filter(|p| !p.is_empty())
        .map(|p| String::from_utf8_lossy(p).into_owned())
        .collect())
}

/// `--tree`: gate every tracked `.rs` and `.md` file under `root`. Returns the exit code.
#[allow(clippy::too_many_lines)]
pub fn tree_mode(root: &Path) -> i32 {
    let all = match tracked(root) {
        Ok(v) => v,
        Err(e) => {
            println!("no git index to read at {} ({e})", root.display());
            println!("VERDICT: FAIL");
            return 1;
        }
    };
    let rs: Vec<&String> = all.iter().filter(|p| p.ends_with(".rs")).collect();
    let md: Vec<&String> = all.iter().filter(|p| p.ends_with(".md")).collect();
    let abs_root = std::path::absolute(root).unwrap_or_else(|_| root.to_path_buf());

    let (mut lies, mut suspects, mut silent_sites) = (Vec::new(), Vec::new(), Vec::new());
    let mut allow_hits: BTreeSet<(&str, &str)> = BTreeSet::new();
    let (mut expressible, mut nblocks) = (0usize, 0usize);
    let mut unreadable = Vec::new();
    let mut longest = (String::new(), 0usize);
    for rel in rs.iter().chain(md.iter()) {
        let path = abs_root.join(rel.as_str());
        let text = match std::fs::read(&path) {
            Ok(b) => String::from_utf8_lossy(&b).into_owned(),
            Err(e) => {
                unreadable.push(format!("{rel} ({e})"));
                continue;
            }
        };
        let n = path.to_string_lossy().chars().count();
        if n > longest.1 {
            longest = ((*rel).clone(), n);
        }
        let is_rs = rel.ends_with(".rs");
        let blocks = if is_rs {
            norm_blocks(&text)
        } else {
            md_blocks(&text)
        };
        nblocks += blocks.len();
        for (ln, body) in &blocks {
            let (verdict, snip) = judge_block(body);
            let Some(verdict) = verdict else {
                continue;
            };
            if let Some((p, sub, _)) = ALLOW
                .iter()
                .find(|(p, sub, _)| *p == rel.as_str() && body.contains(sub))
            {
                allow_hits.insert((p, sub));
                continue;
            }
            // A markdown paragraph has no gate to join the claim against, and a design document
            // legitimately describes the client skipping things on every other page. So markdown
            // is reported and never fatal; the fatal half is scoped to `.rs`, where the file's own
            // gate can be read.
            if verdict == Claim::Lie && is_rs {
                lies.push(((*rel).clone(), *ln, snip));
            } else {
                suspects.push(((*rel).clone(), *ln, snip));
            }
        }
        if is_rs && rel.contains("/tests/") && !rel.ends_with(SELF) {
            let (s, e) = judge_code(&text);
            expressible += e.len();
            for (ln, src) in s {
                silent_sites.push(((*rel).clone(), ln, src));
            }
        }
    }

    println!(
        "examined: {} .rs + {} .md = {} files, {nblocks} comment/paragraph blocks; unreadable {}",
        rs.len(),
        md.len(),
        rs.len() + md.len(),
        unreadable.len()
    );
    // Liveness is a property of the walk, not of the tree's defects: a probe made of exemptions
    // dies exactly when the tree gets healthy, and takes the evidence for every other number with
    // it. The longest path examined is the probe, because long paths are how a corpus is hard on
    // Windows: a walk that stopped at the easy files shows here.
    println!(
        "LIVENESS: longest path examined is {} chars -- {}",
        longest.1, longest.0
    );
    let mut bad = false;
    for u in &unreadable {
        println!("  COULD NOT EXAMINE: {u}");
        bad = true;
    }
    println!(
        "test sites where the silent shape is still EXPRESSIBLE (they take open_from_env()'s \
         Option and all currently panic): {expressible}"
    );
    println!(
        "COULD NOT CLASSIFY (SUSPECT: a skip claim whose block says what happens instead, but \
         not beside the claim): {}",
        suspects.len()
    );
    for (rel, ln, snip) in suspects.iter().take(12) {
        let s: String = snip.chars().take(110).collect();
        println!("   ? {rel}:{ln}  ...{s}");
    }
    if suspects.len() > 12 {
        println!("   ? ... and {} more", suspects.len() - 12);
    }

    for (rel, sub, why) in ALLOW {
        if !allow_hits.contains(&(*rel, *sub)) {
            println!("  STALE ALLOW ENTRY: {rel} / {sub:?} matched nothing.");
            println!("      Either the sentence was fixed -- delete the entry -- or this");
            println!("      gate stopped reaching that file, in which case NOTHING below");
            println!("      is evidence.  Reason on file: {why}");
            bad = true;
        }
    }
    println!(
        "allow-listed true claims matched: {} of {}",
        allow_hits.len(),
        ALLOW.len()
    );

    for (rel, ln, src) in &silent_sites {
        println!("  SILENT GATE  {rel}:{ln}\n     {src}");
        bad = true;
    }
    if !silent_sites.is_empty() {
        println!();
        println!("An Option from a retail-dat resolver was discharged without a hard stop.");
        println!("A skipped test and a passing test are the same green line: 106 tests across");
        println!("33 suites once passed having read nothing at all.");
        println!("THE FIX: resolve through `RetailDatStore::open_from_env_or_fail()`, which");
        println!("returns Self -- then `let Some(_) = ... else` is E0308 and the silent shape");
        println!("cannot be written here at all. Use `require_from_env()` if you need the");
        println!("files but not the store, or `Fixtures::require_retail_data()`.");
    }
    for (rel, ln, snip) in &lies {
        let s: String = snip.chars().take(220).collect();
        println!("  DOC CLAIMS A SKIP  {rel}:{ln}\n     ...{s}");
        bad = true;
    }
    if !lies.is_empty() {
        println!();
        println!("A comment says this skips when the retail dats are absent. Nothing in this");
        println!("tree skips on absent dats any more -- the gates panic. A doc describing a");
        println!("skip that cannot happen sends the next reader to the wrong conclusion about");
        println!("why a suite is green, and 40 such sentences had to be corrected by hand.");
        println!("THE FIX: say what actually happens -- it FAILS -- and, if the sentence is");
        println!("about the retired idiom, say so in the same block (\"used to\", \"no longer\",");
        println!("\"an expect, never a skip\") so this gate can tell history from a promise.");
    }
    if bad {
        println!("VERDICT: FAIL");
    } else {
        println!(
            "VERDICT: no silent retail-dat gate and no live skip claim; {} suspects and \
             {expressible} expressible sites outstanding.",
            suspects.len()
        );
    }
    i32::from(bad)
}

/// `cargo xtask skip-claims`.
pub fn skip_claims(args: &[String]) -> i32 {
    match args.first().map(String::as_str) {
        Some("--tree") => match args.get(1) {
            Some(root) => tree_mode(Path::new(root)),
            None => tree_mode(&workspace_root()),
        },
        _ => {
            eprintln!(
                "usage: cargo xtask skip-claims --tree [ROOT]   gate the tracked tree (default: \
                 the workspace)"
            );
            2
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const RETIRED: &str = "The suite skips with a printed line when the retail dats are absent.";

    fn verdict(s: &str) -> Option<Claim> {
        judge_block(s).0
    }

    /// The retired sentence is a lie, across a line wrap too (blocks are normalised per block),
    /// and a different verb makes the same lie.
    #[test]
    fn the_retired_sentence_is_a_lie_however_it_is_phrased_or_wrapped() {
        assert_eq!(verdict(RETIRED), Some(Claim::Lie));
        let wrapped = norm_blocks(
            "// This module's tests skip with a printed line when the retail\n// dats are absent.\n",
        );
        assert_eq!(wrapped.len(), 1);
        assert_eq!(verdict(&wrapped[0].1), Some(Claim::Lie));
        assert_eq!(
            verdict("It does nothing when the dats are missing."),
            Some(Claim::Lie)
        );
    }

    /// "An expect, never a skip" is never a lie: its correction sits in the sentence before the
    /// skip token, so it is surfaced as a suspect rather than silently exempted.
    #[test]
    fn a_neighbouring_correction_is_a_suspect_never_a_lie() {
        let ok = "**An `expect`, never a skip**. A test that returns early is counted as a pass; \
                  if the retail dats are absent the run is not a pass.";
        assert_eq!(verdict(ok), Some(Claim::Suspect));
    }

    /// The stated blind spot, asserted rather than left to be found: a genuine lie one sentence
    /// from an unrelated correction degrades to a suspect; the same lie beside a neutral sentence
    /// is a lie.
    #[test]
    fn a_lie_beside_an_unrelated_correction_is_only_a_suspect() {
        assert_eq!(
            verdict(
                "These tests bail out when the retail data is missing. Loader behaviour: a \
                 tampered file is a hard error rather than a warning."
            ),
            Some(Claim::Suspect)
        );
        assert_eq!(
            verdict("These tests bail out when the retail data is missing. Loader behaviour."),
            Some(Claim::Lie)
        );
    }

    /// A claim that fails when the files are absent, followed by past-tense history, is not a lie.
    #[test]
    fn a_fails_claim_with_past_tense_history_is_not_a_lie() {
        assert_eq!(
            verdict(
                "**Fails** when the retail files are absent. This skipped with a message until \
                 2026-09-01."
            ),
            None
        );
    }

    /// A skip verb quoted as code is a rendering of the printed word, not a claim; the same
    /// sentence unquoted is a lie.
    #[test]
    fn a_quoted_skip_verb_is_a_rendering_and_an_unquoted_one_is_a_claim() {
        assert_eq!(
            verdict(
                "In a worktree with no DERETH_TEST_DAT_DIR the helper printed `skipping` and \
                 returned None."
            ),
            None
        );
        assert_eq!(
            verdict(
                "In a worktree with no DERETH_TEST_DAT_DIR the helper printed skipping and \
                 returned None."
            ),
            Some(Claim::Lie)
        );
    }

    /// A skip verb with nothing about the retail data beside it is not judged: the client skipping
    /// a zero id, a skip with no absence condition, and a sentence about a datagram.
    #[test]
    fn a_skip_with_no_absence_of_retail_data_is_not_judged() {
        assert_eq!(
            verdict("Static-object initialization skips an id of zero."),
            None
        );
        assert_eq!(
            verdict("The dats are the oracle here and the step is skipped."),
            None
        );
        assert_eq!(
            verdict("Every shard runs alone. No datagram leaves this process. It is skipped."),
            None
        );
    }

    /// `let Some(..) = open_from_env() else { return }` is silent; the same shape with a panic is
    /// not; both still count as expressible; the loud resolver is neither.
    #[test]
    fn an_option_discharged_without_a_hard_stop_is_silent() {
        let silent_src =
            "fn store() -> Option<Arc<RetailDatStore>> { RetailDatStore::open_from_env() }\n\
             #[test]\nfn t() {\n    let Some(_s) = RetailDatStore::open_from_env() else {\n        \
             eprintln!(\"skipping: no dats\");\n        return;\n    };\n}\n";
        let (s, e) = judge_code(silent_src);
        assert_eq!(s.len(), 1);
        let loud_src = "#[test]\nfn t() {\n    let Some(_s) = RetailDatStore::open_from_env() else {\n        \
             panic!(\"the retail dats are this crate's oracle\")\n    };\n}\n";
        let (s2, e2) = judge_code(loud_src);
        assert!(s2.is_empty());
        assert!(!e.is_empty() && e2.len() == 1);
        let (s3, e3) = judge_code("let s = RetailDatStore::open_from_env_or_fail();\n");
        assert!(s3.is_empty() && e3.is_empty());
    }

    /// A path past Windows' MAX_PATH is read and its lie is still seen, and an absent file still
    /// errors: this gate fails the run on any unreadable file, so a reader that swallowed errors
    /// would certify files by omission.
    #[test]
    fn a_path_past_max_path_is_read_and_its_lie_is_still_seen() {
        let mut deep =
            std::env::temp_dir().join(format!("xtask-skip-claims-{}", std::process::id()));
        std::fs::remove_dir_all(&deep).ok();
        for _ in 0..4 {
            deep = deep.join("x".repeat(60));
        }
        std::fs::create_dir_all(&deep).expect("a deep directory");
        let long_path = deep.join("probe.rs");
        let body = format!("//! {RETIRED}\n");
        std::fs::write(&long_path, &body).expect("write");
        assert!(long_path.to_string_lossy().len() > 260);
        let text = std::fs::read_to_string(&long_path).expect("the long path is read");
        assert_eq!(text, body);
        assert_eq!(verdict(&norm_blocks(&text)[0].1), Some(Claim::Lie));
        assert!(std::fs::read(deep.join("there-is-no-such-file.rs")).is_err());
    }

    /// Sentences split after `.`, `!` or `?` and a run of whitespace, and nowhere else.
    #[test]
    fn a_block_splits_into_sentences_at_final_punctuation() {
        assert_eq!(
            sentences("One two. Three!  Four? e.g.five"),
            vec!["One two.", "Three!", "Four?", "e.g.five"]
        );
    }
}
