//! Recorded trace availability diagnostic.
#![cfg(feature = "trace")]

use dereth_physics::trace::REQUIRED_SCENARIOS;
use std::path::{Path, PathBuf};

fn traces_dir() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../../fixtures/physics/traces")
}

#[test]
#[ignore = "requires recorded physics traces; run explicitly with --features trace -- --ignored trace_suite"]
fn trace_suite() {
    let dir = traces_dir();
    let mut missing = Vec::new();
    for (name, target) in REQUIRED_SCENARIOS {
        let jsonl = dir.join(format!("{name}.jsonl"));
        let input = dir.join(format!("{name}.input.json"));
        if !jsonl.exists() || !input.exists() {
            missing.push(format!("  {name}  (targets {target})"));
        }
    }
    assert!(
        missing.is_empty(),
        "the physics trace fixtures have not been captured. Missing {} of {} scenarios under {}:\n{}\n\
         Capture each scripted walk as a physics trace with its matching input file. \
         End the client session with Shift+Escape, EXIT, Yes.",
        missing.len(),
        REQUIRED_SCENARIOS.len(),
        dir.display(),
        missing.join("\n")
    );
}
