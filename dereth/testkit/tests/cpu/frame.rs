//! The frame event log: the one place everything the client counts about its own frames lives.
//!
//! The subject is the log itself rather than a running client, so the log is built here and the
//! claim is booked through a model client at the end. A scenario that drove a whole client could
//! not reach the third and fourth halves at all: they are about what survives the ring being
//! trimmed, which takes thousands of frames.

use dereth_client::frame::{FrameRecorder, FrameStep};
use dereth_client::frame_events::{
    ActionRoute, ControlLossSite, FrameEvent, FrameEventKind, FrameEvents, NoBodyReason,
    StreamStage, KIND_COUNT,
};
use dereth_client::world::RenderPrefWork;
use dereth_testkit::HeadlessClient;

/// Every scenario in this file, for the census: the name, **the behaviour ids the
/// scenario asserts**, and the function.
pub static ALL: &[dereth_testkit::behaviours::Scenario] = &[(
    "the_frame_log_is_the_counters",
    &["frame.log.is-the-counters-and-keeps-them-whole"],
    the_frame_log_is_the_counters,
)];

/// Run one of this file's scenarios under a recorder, and check that the behaviour ids it
/// asserted are exactly the ones its [`ALL`] entry declares.
fn scenario(name: &str) {
    dereth_testkit::behaviours::run_scenario(ALL, name);
}

/// The fourteen lines a frame that ran to the end produces, in order. This is the golden text a
/// scenario diffs against.
const WHOLE_FRAME: &str = "\
step UiQueueStep
step ClockSample
step ProcessWindowEvents
step NetworkStep
step LoginEvents
step PacketStep
step AssetCacheStep
step UiStep
step WorldViewStep
step PrepareDevice
step BeginFrame
step DrawWorld
step PresentFrame
step PaceFrame";

/// A frame's worth of steps, pushed exactly as the client's own frame pushes them.
fn a_whole_frame() -> FrameEvents {
    let mut log = FrameEvents::new();
    for step in FrameStep::ORDER {
        log.push(FrameEvent::Step(*step));
    }
    log
}

/// One event of every kind, so a change to the set cannot leave a kind untested.
fn one_of_each() -> Vec<FrameEvent> {
    vec![
        FrameEvent::Step(FrameStep::DrawWorld),
        FrameEvent::FrameDrawn,
        FrameEvent::ActionRouted(ActionRoute::Movement),
        FrameEvent::PoseMotionIssued,
        FrameEvent::JumpUseTime,
        FrameEvent::JumpRequested { status: 0 },
        FrameEvent::NewForwardAttackAborted,
        FrameEvent::NewForwardAttackCancelSent,
        FrameEvent::PlayerModuleSavedAtLogout,
        FrameEvent::WorldCameraTurnApplied,
        FrameEvent::WorldReset {
            textures_released: 5,
        },
        FrameEvent::StreamFailed(StreamStage::Landblocks),
        FrameEvent::RenderPreferencesPolled(RenderPrefWork::default()),
        FrameEvent::RenderPreferencesApplied,
        FrameEvent::PositionUseTime,
        FrameEvent::PlayerTeleportUseTime,
        FrameEvent::PlayerTeleportApplied,
        FrameEvent::PlayerTeleportBeforeABody {
            count: 1,
            reason: NoBodyReason::NoScene,
        },
        FrameEvent::ControlTransfer,
        FrameEvent::ServerControlLost(ControlLossSite::Transfer),
        FrameEvent::ServerControlRetaken,
        FrameEvent::EscapeOptionsToggle { answered: true },
        FrameEvent::EscapeStopPerformed,
        FrameEvent::ScreenshotSaved,
        FrameEvent::ScreenshotFailed,
    ]
}

/// The log is the counters, and it keeps them whole after it has forgotten the entries.
pub fn the_frame_log_is_the_counters() {
    // A whole frame is its fourteen documented lines, in order.
    let log = a_whole_frame();
    let whole = log.last_frame_text() == WHOLE_FRAME
        && log.last_frame().len() == FrameStep::ORDER.len()
        && log.total(FrameEventKind::Step) == FrameStep::ORDER.len() as u64;
    // …and the recorder's view over it is that same order, which is what the `dat` tier reads.
    let mut log = a_whole_frame();
    let recorded = FrameRecorder::new(&log).steps() == FrameStep::ORDER;
    log.drain_frame();
    let next_frame_is_empty = FrameRecorder::new(&log).steps().is_empty();

    // A frame cut short is a shorter list, not a different one.
    let mut short = FrameEvents::new();
    for step in &FrameStep::ORDER[..3] {
        short.push(FrameEvent::Step(*step));
    }
    let truncated = short.last_frame_text()
        == "step UiQueueStep\nstep ClockSample\nstep ProcessWindowEvents"
        && short.total(FrameEventKind::FrameDrawn) == 0;

    // Every kind has its own one-line form, and none of them carries an address.
    let events = one_of_each();
    let mut kinds: Vec<FrameEventKind> = events.iter().map(|e| e.kind()).collect();
    kinds.sort_unstable();
    kinds.dedup();
    let one_line_each = events.len() == KIND_COUNT
        && kinds.len() == KIND_COUNT
        && events.iter().all(|e| {
            let line = e.to_string();
            !line.is_empty()
                && line.lines().count() == 1
                && !line.contains("0x")
                && line.starts_with(e.kind().name())
        });

    // A total counts the events; an amount sums their quantity. This is the mapping the client's
    // old one-number counters were made of.
    let mut sums = FrameEvents::new();
    sums.push(FrameEvent::WorldReset {
        textures_released: 7,
    });
    sums.push(FrameEvent::WorldReset {
        textures_released: 0,
    });
    sums.push(FrameEvent::JumpRequested { status: 0 });
    sums.push(FrameEvent::JumpRequested { status: 0 });
    sums.push(FrameEvent::JumpRequested { status: 9 });
    sums.push(FrameEvent::EscapeOptionsToggle { answered: true });
    sums.push(FrameEvent::EscapeOptionsToggle { answered: false });
    sums.push(FrameEvent::PlayerTeleportBeforeABody {
        count: 3,
        reason: NoBodyReason::JournalDiscarded,
    });
    sums.push(FrameEvent::PlayerTeleportBeforeABody {
        count: 1,
        reason: NoBodyReason::NoCharacter,
    });
    let amounts = sums.total(FrameEventKind::WorldReset) == 2
        && sums.amount(FrameEventKind::WorldReset) == 7
        && sums.total(FrameEventKind::JumpRequested) == 3
        && sums.amount(FrameEventKind::JumpRequested) == 1
        && sums.total(FrameEventKind::EscapeOptionsToggle) == 2
        && sums.amount(FrameEventKind::EscapeOptionsToggle) == 1
        && sums.amount(FrameEventKind::PlayerTeleportBeforeABody) == 4;
    // Every other kind's amount is its total, which is what makes the plain counters one-liners.
    let plain = one_of_each().into_iter().all(|e| {
        let k = e.kind();
        if matches!(
            k,
            FrameEventKind::WorldReset
                | FrameEventKind::JumpRequested
                | FrameEventKind::EscapeOptionsToggle
                | FrameEventKind::PlayerTeleportBeforeABody
        ) {
            return true;
        }
        let mut one = FrameEvents::new();
        one.push(e);
        one.push(e);
        one.total(k) == 2 && one.amount(k) == one.total(k)
    });

    // The ring is bounded and the totals are not: a counter that read the ring would start
    // answering zero after the trim.
    let mut ring = FrameEvents::with_ring_frames(4);
    for _ in 0..1_000 {
        ring.push(FrameEvent::FrameDrawn);
        ring.push(FrameEvent::PositionUseTime);
        ring.drain_frame();
    }
    let bounded = ring.total(FrameEventKind::FrameDrawn) == 1_000
        && ring.total(FrameEventKind::PositionUseTime) == 1_000
        && ring.count(|e| *e == FrameEvent::FrameDrawn) == 4
        && ring.frame_index() == 1_000
        && ring.first_retained_frame() == 996
        && ring.since(998).len() == 4
        && ring.last_frame().is_empty();

    // …and the most recent entry of a kind outlives the ring, because the client reads its
    // payload long after the frame that produced it.
    let mut kept = FrameEvents::with_ring_frames(1);
    let work = RenderPrefWork {
        flushed: true,
        blocks_queued: 2,
        ..RenderPrefWork::default()
    };
    kept.push(FrameEvent::RenderPreferencesPolled(work));
    for _ in 0..8 {
        kept.drain_frame();
    }
    let outlives = kept.count(|e| matches!(e, FrameEvent::RenderPreferencesPolled(_))) == 0
        && kept.last(FrameEventKind::RenderPreferencesPolled)
            == Some(FrameEvent::RenderPreferencesPolled(work))
        && kept
            .last(FrameEventKind::RenderPreferencesPolled)
            .map(|e| e.to_string())
            .unwrap_or_default()
            == "render-preferences-polled flushed=true mid-radius=false detail-texturing=false \
                queued=2 rebuilt=0 surfaces=0";

    // The five routes one old counter had collapsed into a single number.
    let mut routes = FrameEvents::new();
    let named = [
        (
            ActionRoute::EscapeFinishedJump,
            "action-routed escape-finished-jump",
        ),
        (ActionRoute::Jump, "action-routed jump"),
        (ActionRoute::Movement, "action-routed movement"),
        (ActionRoute::Camera, "action-routed camera"),
        (ActionRoute::WorldCamera, "action-routed world-camera"),
    ];
    let mut distinguishable = true;
    for (r, text) in named {
        let e = FrameEvent::ActionRouted(r);
        distinguishable &= e.to_string() == text;
        routes.push(e);
    }
    distinguishable &= routes.total(FrameEventKind::ActionRouted) == 5
        && routes.count(|e| matches!(e, FrameEvent::ActionRouted(ActionRoute::Jump))) == 1;

    let mut c = HeadlessClient::model();
    c.assert_behaviour(
        "frame.log.is-the-counters-and-keeps-them-whole",
        move |_| {
            whole
                && recorded
                && next_frame_is_empty
                && truncated
                && one_line_each
                && amounts
                && plain
                && bounded
                && outlives
                && distinguishable
        },
    );
}

// -------------------------------------------------------------------------------------------

#[test]
fn scenario_the_frame_log_is_the_counters() {
    scenario("the_frame_log_is_the_counters");
}
