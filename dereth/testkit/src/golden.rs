//! Golden files: a whole run's frame log, committed, and diffed.
//!
//! # What a golden replaces
//!
//! A test that asserts "it did all of these things, in this order, once each" by counting needs one
//! accessor per thing counted, each existing only for a test. The frame log collects the events
//! in one place; a golden collapses the assertions into one file. A scenario that ran a login now says *this is every event that
//! login produced*, and a new event, a missing event or a reordered pair all fail in the same
//! place with the same diff.
//!
//! # Rewriting
//!
//! `DERETH_TEST_GOLDEN=1 cargo test ...` writes what the run produced instead of comparing it. **The
//! rewrite is a deliberate act and its output is a diff to read**, not a way to make a red test
//! green: a golden that changed is a behaviour that changed, and the commit that rewrites one says
//! what moved.
//!
//! The file is committed under the crate's own `tests/golden/`. It is not a fixture set: it is not
//! derived from the retail data, it is not hashed into the fixture lock, and it is an *output* of
//! this client rather than an oracle from outside it. What makes it strong is that the run that
//! produces it is driven by a recording that is.
//!
//! # What the frame log can and cannot witness
//!
//! The log records what the *frame* did. A message a scenario delivers between frames -- which is
//! what every `Inbound` step is -- changes the model, not the frame, so it does not appear as a
//! line of its own. That is why the corpus-slice golden reads as fourteen steps repeated: its
//! claim is the negative one, and it is a real claim. Six hundred recorded blobs of a world
//! session arrive at a client with its screens up and **every frame still does exactly its
//! fourteen steps and nothing else** -- no world reset, no lost control, no stream failure, no
//! teleport without a body. A change that made any of those happen under recorded traffic would
//! add a line here, and this is the file it would add it to.

use std::path::{Path, PathBuf};

/// The environment variable that rewrites a golden instead of comparing it.
pub const REWRITE_VAR: &str = "DERETH_TEST_GOLDEN";

/// Where the committed goldens live.
#[must_use]
pub fn golden_dir() -> PathBuf {
    // `CARGO_MANIFEST_DIR` is `dereth/testkit`.
    Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/golden")
}

/// Whether this run rewrites rather than compares.
#[must_use]
pub fn rewriting() -> bool {
    std::env::var_os(REWRITE_VAR).is_some_and(|v| v != "0" && !v.is_empty())
}

/// Compare `actual` against `tests/golden/<name>.txt`, or rewrite it.
///
/// # Panics
/// Panics with the first differing line when the two disagree, and when the golden is absent on a
/// run that is not rewriting -- an absent golden is a missing oracle, never a pass.
pub fn check(name: &str, actual: &str) {
    check_or_rewrite(name, actual, rewriting());
}

/// [`check`], rewriting when `rewrite` says so rather than when the environment does.
fn check_or_rewrite(name: &str, actual: &str, rewrite: bool) {
    let path = golden_dir().join(format!("{name}.txt"));
    // Committed files in this repository are LF, and a golden written on Windows must be too or
    // every line of it differs the first time it is read on another host.
    let actual = normalise(actual);

    if rewrite {
        // `name` may carry a directory -- `ui/<name>` is `UiSnapshot::assert_tree`'s --
        // so the directory made is the file's own parent and not always `golden_dir()`.
        let dir = path.parent().unwrap_or(&path).to_path_buf();
        std::fs::create_dir_all(&dir).expect("the golden directory");
        std::fs::write(&path, actual.as_bytes())
            .unwrap_or_else(|e| panic!("rewriting {}: {e}", path.display()));
        eprintln!(
            "{REWRITE_VAR}: rewrote {} ({} lines)",
            path.display(),
            actual.lines().count()
        );
        return;
    }

    let want = std::fs::read_to_string(&path).unwrap_or_else(|e| {
        panic!(
            "the golden {} is this scenario's oracle and it did not load: {e}\n\
             run the scenario again with {REWRITE_VAR}=1 to write it, and read the diff",
            path.display()
        )
    });
    let want = normalise(&want);
    if want == actual {
        return;
    }

    let mut w = want.lines();
    let mut a = actual.lines();
    let mut line = 0_usize;
    loop {
        line += 1;
        match (w.next(), a.next()) {
            (Some(x), Some(y)) if x == y => {}
            (x, y) => panic!(
                "{} differs at line {line}\n  golden: {}\n  actual: {}\n\
                 ({} golden lines, {} actual lines; {REWRITE_VAR}=1 rewrites it)",
                path.display(),
                x.unwrap_or("<end of file>"),
                y.unwrap_or("<end of file>"),
                want.lines().count(),
                actual.lines().count(),
            ),
        }
    }
}

/// LF, and exactly one trailing newline.
fn normalise(s: &str) -> String {
    let mut out: String = s.replace("\r\n", "\n");
    while out.ends_with('\n') {
        out.pop();
    }
    out.push('\n');
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_golden_is_lf_with_one_trailing_newline_whatever_it_was_given() {
        assert_eq!(normalise("a\r\nb\r\n"), "a\nb\n");
        assert_eq!(normalise("a\nb"), "a\nb\n");
        assert_eq!(normalise("a\nb\n\n\n"), "a\nb\n");
    }

    /// Compared, whatever the environment asks: a rewriting run must neither write the file
    /// nor fail here.
    #[test]
    fn an_absent_golden_is_a_panic_and_not_a_pass() {
        let e = std::panic::catch_unwind(|| {
            check_or_rewrite("no-such-golden-missing-oracle", "anything", false);
        });
        assert!(e.is_err(), "a missing oracle must not read as a pass");
    }

    /// The two committed goldens are files, not an idea. A run in a worktree that resolved the
    /// directory wrongly would otherwise only fail once a scenario ran.
    #[test]
    fn the_committed_goldens_are_where_this_module_looks_for_them() {
        let dir = golden_dir();
        assert!(
            dir.is_dir(),
            "the golden directory is absent at {}",
            dir.display()
        );
        for name in ["headless_login", "corpus_slice"] {
            let p = dir.join(format!("{name}.txt"));
            assert!(
                p.is_file(),
                "the committed golden {} is absent",
                p.display()
            );
        }
    }
}
