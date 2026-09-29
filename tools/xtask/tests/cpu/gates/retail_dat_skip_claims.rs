//! Behaviour: none (checks formats, geometry, numeric contracts or repository tools)
//! The tree fails if a test skips on absent retail dats or a comment claims one does, as
//! `cargo xtask skip-claims --tree` reads the tree. The checker's own calibrations are its unit
//! tests.
//! Fixture: repository files.

use std::process::Command;

use super::common::repo_root;

#[test]
fn no_test_skips_on_absent_retail_dats_and_no_comment_claims_one_does() {
    let out = Command::new(env!("CARGO_BIN_EXE_xtask"))
        .args(["skip-claims", "--tree"])
        .arg(repo_root())
        .output()
        .expect("the xtask binary runs");
    let ok = out.status.success();
    let out = format!(
        "{}{}",
        String::from_utf8_lossy(&out.stdout),
        String::from_utf8_lossy(&out.stderr)
    );

    // The denominator. A walk that enumerated nothing reports every rule satisfied.
    let files: usize = out
        .split("= ")
        .nth(1)
        .and_then(|s| s.split_whitespace().next())
        .and_then(|s| s.parse().ok())
        .unwrap_or_else(|| panic!("the checker printed no denominator:\n{out}"));
    assert!(
        files > 2000,
        "the checker examined only {files} files; it is broken, not the tree clean. \
         (The workspace had over 3,000 tracked .rs and .md files when this was written.)\n{out}"
    );
    assert!(
        out.contains("unreadable 0"),
        "some files could not be examined, and an unexamined file is not a clean one:\n{out}"
    );
    assert!(
        out.contains("LIVENESS: longest path examined is "),
        "the checker printed no LIVENESS line:\n{out}"
    );

    assert!(
        ok,
        "either a retail-dat Option is discharged without a hard stop, or a comment claims \
         this skips when the dats are absent.\n\
         A skipped test and a passing test are the same green line: 106 tests across 33 \
         suites once passed having read nothing at all.\n\
         THE FIX (code): resolve through `dereth_dat::testing::open_store_or_fail()`, which \
         returns Self -- then the silent shape is E0308 and cannot be written.\n\
         THE FIX (docs): say what actually happens -- it FAILS -- or, if the sentence is \
         about the retired idiom, say so in the same sentence.\n{out}"
    );
}
