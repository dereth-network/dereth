//! The warm-up the presentation's pipelines are built on: its builds run off the thread that
//! asks for them, it is done once every one is, a slot asked for again for another key builds
//! again, a build that panics ends alone, a held warm-up starts nothing until it is let go,
//! letting it go never waits for a build under way, and every pipeline the crate makes is
//! counted.

use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{mpsc, Arc};
use std::time::Duration;

use dereth_render_hifi::build::{Prebuilt, Warm};

/// A guard against a hang, not a measure.
const HANG: Duration = Duration::from_secs(60);

/// Behaviour: hifi.warm-up.no-frame-builds-a-pipeline-of-the-presentations
#[test]
fn a_build_runs_off_the_asking_thread_and_the_warm_up_is_done_once_it_is() {
    let mut warm = Warm::new();
    assert!(warm.done(), "nothing asked for");
    let (release, held) = mpsc::channel::<()>();
    let mut slot = Prebuilt::default();
    slot.ask(&mut warm, move || {
        held.recv().expect("released");
        std::thread::current().id()
    });
    assert!(slot.is_asked());
    assert_eq!(warm.asked(), 1);
    assert!(!warm.done(), "the build is held, so it cannot be done");
    assert_eq!(slot.take(), None, "nothing built yet");
    release.send(()).expect("the build waits");
    assert!(warm.settle(HANG), "the build never finished");
    assert!(warm.done());
    let built_on = slot.take().expect("built");
    assert_ne!(
        built_on,
        std::thread::current().id(),
        "built on the asking thread"
    );
    assert_eq!(slot.take(), None, "taken once");
}

/// Behaviour: hifi.warm-up.no-frame-builds-a-pipeline-of-the-presentations
#[test]
fn a_slot_asked_again_builds_again_only_for_another_key() {
    let mut warm = Warm::new();
    let mut slot = Prebuilt::default();
    slot.ask_for(1, &mut warm, || 1);
    slot.ask_for(1, &mut warm, || 10);
    assert_eq!(warm.asked(), 1, "asked twice for the same key");
    assert!(warm.settle(HANG));
    slot.ask_for(2, &mut warm, || 2);
    assert_eq!(warm.asked(), 2);
    assert!(warm.settle(HANG));
    assert_eq!(slot.take(), Some(2), "what was built for the last key");
}

/// Behaviour: hifi.warm-up.no-frame-builds-a-pipeline-of-the-presentations
#[test]
fn a_build_that_panics_ends_alone_and_the_warm_up_still_finishes() {
    let mut warm = Warm::new();
    let mut failed: Prebuilt<u32> = Prebuilt::default();
    let mut fine = Prebuilt::default();
    failed.ask(&mut warm, || panic!("the device refused the pipeline"));
    fine.ask(&mut warm, || 7);
    assert!(warm.settle(HANG), "a panicking build held the warm-up");
    assert_eq!(failed.take(), None, "nothing built for the failed build");
    assert!(
        !failed.waits(),
        "the pass is not kept waiting for a build that failed"
    );
    assert_eq!(fine.take(), Some(7));
    assert!(!fine.waits(), "nor for one it has taken");
}

/// Behaviour: hifi.warm-up.no-frame-builds-a-pipeline-of-the-presentations
#[test]
fn a_held_warm_up_starts_no_build_until_it_is_let_go() {
    let mut warm = Warm::new();
    warm.hold(true);
    let let_go = Arc::new(AtomicBool::new(false));
    let mut slot = Prebuilt::default();
    let seen = Arc::clone(&let_go);
    slot.ask(&mut warm, move || seen.load(Ordering::SeqCst));
    assert!(slot.waits(), "held, the build waits");
    assert!(!warm.done());
    let_go.store(true, Ordering::SeqCst);
    warm.hold(false);
    assert!(warm.settle(HANG), "the build never finished once let go");
    assert_eq!(
        slot.take(),
        Some(true),
        "the build ran while the warm-up was held"
    );
}

/// Behaviour: hifi.warm-up.no-frame-builds-a-pipeline-of-the-presentations
#[test]
fn letting_the_warm_up_go_does_not_wait_for_a_build_under_way() {
    let mut warm = Warm::new();
    let (began, beginning) = mpsc::channel::<()>();
    let (release, held) = mpsc::channel::<()>();
    let (ended, ending) = mpsc::channel::<bool>();
    let mut slot = Prebuilt::default();
    slot.ask(&mut warm, move || {
        began.send(()).expect("the test listens");
        // Released only once the warm-up is let go: a drop that waited for the build would wait
        // until this guard ran out.
        let released = held.recv_timeout(HANG).is_ok();
        ended.send(released).expect("the test listens");
        1
    });
    beginning.recv().expect("the build began");
    drop(warm);
    release.send(()).expect("the build is still under way");
    assert!(
        ending.recv().expect("the build ends"),
        "letting the warm-up go waited for the build"
    );
}

/// Behaviour: hifi.warm-up.no-frame-builds-a-pipeline-of-the-presentations
#[test]
fn every_pipeline_the_crate_makes_goes_through_the_counted_builders() {
    let src = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("src");
    let mut stack = vec![src];
    let mut uncounted = Vec::new();
    while let Some(dir) = stack.pop() {
        for entry in std::fs::read_dir(&dir).expect("the source folder reads") {
            let path = entry.expect("an entry").path();
            if path.is_dir() {
                stack.push(path);
                continue;
            }
            if path.extension().is_none_or(|e| e != "rs") || path.ends_with("build.rs") {
                continue;
            }
            let text = std::fs::read_to_string(&path).expect("the source reads");
            for (i, line) in text.lines().enumerate() {
                if line.contains(".create_render_pipeline(")
                    || line.contains(".create_compute_pipeline(")
                {
                    uncounted.push(format!("{}:{}", path.display(), i + 1));
                }
            }
        }
    }
    assert!(
        uncounted.is_empty(),
        "pipelines made outside the counted builders: {uncounted:?}"
    );
}
