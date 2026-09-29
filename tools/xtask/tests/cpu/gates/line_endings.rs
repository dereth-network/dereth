//! Behaviour: none (checks formats, geometry, numeric contracts or repository tools)
//! Every tracked file has the line ending .gitattributes gives it, and none it governs is mixed,
//! as `cargo xtask line-endings --tree` reads the tree. The checker's own calibrations are its unit
//! tests.
//! Fixture: repository files.

use std::process::Command;

use super::common::repo_root;

/// Read one of the gate's own printed counts, or fail saying which one was missing.
fn count(out: &str, key: &str) -> usize {
    out.split(key)
        .nth(1)
        .and_then(|s| s.split_whitespace().next())
        .and_then(|s| s.parse().ok())
        .unwrap_or_else(|| panic!("the gate printed no `{key}`:\n{out}"))
}

#[test]
fn every_tracked_file_has_the_ending_gitattributes_gives_it() {
    let out = Command::new(env!("CARGO_BIN_EXE_xtask"))
        .args(["line-endings", "--tree"])
        .arg(repo_root())
        .output()
        .expect("the xtask binary runs");
    let ok = out.status.success();
    let out = format!(
        "{}{}",
        String::from_utf8_lossy(&out.stdout),
        String::from_utf8_lossy(&out.stderr)
    );

    // The denominators, asserted rather than trusted. A walk that enumerated nothing reports
    // every rule satisfied, and so does a tree whose `.gitattributes` governs nothing.
    let examined = count(&out, "text examined:");
    let governed = count(&out, "governed by .gitattributes:");
    assert!(
        examined > 2000,
        "the gate examined only {examined} tracked text files; it is broken, not the tree \
         clean. (The workspace had over 4,000 when this was written.)\n{out}"
    );
    assert!(
        governed * 2 > examined,
        "only {governed} of {examined} text files have an ending rule; `.gitattributes` is \
         missing or no longer says `* text=auto eol=lf`:\n{out}"
    );
    assert!(
        out.contains("unreadable: 0"),
        "some tracked files could not be examined, and an unexamined file is not a clean \
         one -- that count is reported on purpose:\n{out}"
    );
    assert!(
        out.contains("MIXED: 0 governed") && out.contains(", 0 with the wrong ending"),
        "a governed file is mixed or has the wrong ending:\n{out}"
    );
    assert!(
        ok,
        "a tracked file breaks the line-ending rule of .gitattributes.\n\
         THE FIX: `cargo xtask line-endings --normalise <path>`; or, if the bytes must be \
         kept as committed, give the file a `-text` rule in .gitattributes.\n{out}"
    );
}
