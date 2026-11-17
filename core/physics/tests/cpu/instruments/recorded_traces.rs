//! Which required physics trace scenarios the recorded traces hold. The traces are recorded from the
//! retail client and stay out of this tree: they are read only from the folder
//! `DERETH_TEST_PHYSICS_TRACES` names.
#![cfg(feature = "trace")]

use dereth_physics::trace::REQUIRED_SCENARIOS;
use std::path::PathBuf;

/// The folder `DERETH_TEST_PHYSICS_TRACES` names.
fn traces_dir() -> PathBuf {
    match std::env::var_os("DERETH_TEST_PHYSICS_TRACES") {
        Some(dir) if !dir.is_empty() => PathBuf::from(dir),
        _ => panic!(
            "DERETH_TEST_PHYSICS_TRACES is not set: it names the recorded physics traces' folder"
        ),
    }
}

#[test]
#[ignore = "instrument: lists the required scenarios missing from DERETH_TEST_PHYSICS_TRACES; run with --features trace -- --ignored every_required_scenario_has_a_recorded_trace"]
fn every_required_scenario_has_a_recorded_trace() {
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
        "{} of {} required physics trace scenarios have no recording under {}:\n{}\n\
         Each needs a scripted walk recorded as a physics trace with its matching input file.",
        missing.len(),
        REQUIRED_SCENARIOS.len(),
        dir.display(),
        missing.join("\n")
    );
}
