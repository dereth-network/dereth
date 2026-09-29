//! Divergence: V376
//! `empyrean-server --version` names the version, the commit, the build time, the target and the
//! source, and starts nothing.
//! Fixture: the built server binary.

use std::process::Command;

/// `--version` prints the five lines and exits 0 without reading a configuration or opening a
/// database: it is what a release's smoke test runs on every platform.
#[test]
fn version_names_the_build_and_its_source_and_starts_nothing() {
    let dir = std::env::temp_dir().join(format!("empyrean-version-flag-{}", std::process::id()));
    std::fs::create_dir_all(&dir).expect("a temporary directory");
    let out = Command::new(env!("CARGO_BIN_EXE_empyrean-server"))
        .arg("--version")
        .current_dir(&dir)
        .output()
        .expect("the server runs");
    assert!(out.status.success(), "{out:?}");
    let text = String::from_utf8(out.stdout).expect("utf-8");
    let lines: Vec<&str> = text.lines().collect();
    assert_eq!(
        lines[0],
        format!(
            "empyrean-server {}",
            empyrean_common::server_build_info::VERSION
        )
    );
    assert_eq!(
        lines[1],
        format!("commit: {}", empyrean_common::server_build_info::COMMIT)
    );
    assert!(lines[2].starts_with("built: "), "{text}");
    assert_eq!(
        lines[3],
        format!("target: {}", empyrean_common::brand::build_target())
    );
    assert_eq!(
        lines[4],
        format!("source: {}", empyrean_common::brand::SOURCE_URL)
    );
    assert_eq!(lines.len(), 5, "{text}");
    let left: Vec<_> = std::fs::read_dir(&dir)
        .expect("readable")
        .map(|e| e.expect("entry").file_name())
        .collect();
    assert!(left.is_empty(), "--version wrote {left:?}");
    std::fs::remove_dir_all(&dir).ok();
}
