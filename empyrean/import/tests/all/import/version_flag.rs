//! Divergence: V376
//! `empyrean-import --version` names the version, the commit, the build time, the target and the
//! source.
//! Fixture: the built importer.

use std::process::Command;

/// `--version` prints the importer's name and version first and the source last, and exits 0.
#[test]
fn version_names_the_importer_and_its_source() {
    let out = Command::new(env!("CARGO_BIN_EXE_empyrean-import"))
        .arg("--version")
        .output()
        .expect("the importer runs");
    assert!(out.status.success(), "{out:?}");
    let text = String::from_utf8(out.stdout).expect("utf-8");
    assert_eq!(
        text,
        empyrean_common::brand::version_text("empyrean-import"),
        "the same text the library formats"
    );
    let first = text.lines().next().expect("a line");
    assert_eq!(
        first,
        format!(
            "empyrean-import {}",
            empyrean_common::server_build_info::VERSION
        )
    );
    assert!(
        text.ends_with(&format!("source: {}\n", empyrean_common::brand::SOURCE_URL)),
        "{text}"
    );
}
