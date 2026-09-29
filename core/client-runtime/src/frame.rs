//! The per-frame order, and the recorder that asserts it.
//!
//! `FrameStep` transcribes the per-frame order one-for-one. The outer client frame prepends
//! exactly one call.
//!
//! The order is asserted by a recording harness, not by eyeball, and that is what
//! `FrameRecorder` is for: [`crate::app::App::frame`] pushes a step before performing it, and
//! the test compares the recording against `FrameStep::ORDER`.
//!
//! The recorder stores nothing. A step is one
//! [`crate::frame_events::FrameEvent::Step`] in the frame's event log, pushed by the same call
//! that records everything else the frame did, and `FrameRecorder` is the borrowed view of that
//! log's step order. There is one store, not two.
//!
//! A step is recorded whether or not anything performs it yet, so the recording says what a frame
//! *is*, and filling a step in turns a hole into a call without moving anything.

/// One operation in the client's per-frame order.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FrameStep {
    /// The gameplay client prepends UI-queue simulation and chains the base.
    UiQueueStep,
    /// 1. Update the timer, sampled **once** per frame; every consumer reads the same
    ///    value. "Sampling the clock more than once per frame changes physics results."
    ClockSample,
    /// 2. Run the event loop. **If it returns true, the frame returns false
    ///    immediately** — the network is not pumped and no frame is drawn.
    ProcessWindowEvents,
    /// 3. Advance the client network.
    NetworkStep,
    /// 4. Process the login events.
    LoginEvents,
    /// 5. Advance the packet controller.
    PacketStep,
    /// 6. advance the database cache.
    AssetCacheStep,
    /// 7. Advance the UI element manager — **where the phase state machine
    ///    advances**, via a global message broadcast with arguments `(3, 0)`.
    UiStep,
    /// 8. Advance smart-box world simulation.
    WorldViewStep,
    /// 9. Prepare the graphics device — lost-device check plus changed prefs.
    PrepareDevice,
    /// 10. Start the scene frame — advance the scene timestamp, `Clear(7, black, z=1.0)`,
    ///     `BeginScene()`.
    BeginFrame,
    /// 11. Draw the 3D world; a no-op while the world is hidden.
    DrawWorld,
    /// 12. End the frame with the 2D UI overlay, `EndScene`, and `Present`.
    PresentFrame,
    /// 13. Pace the completed frame.
    PaceFrame,
}

impl FrameStep {
    /// The whole frame, in order. The frame loop's step 14 is `return true`, not a call.
    pub const ORDER: &'static [FrameStep] = &[
        FrameStep::UiQueueStep,
        FrameStep::ClockSample,
        FrameStep::ProcessWindowEvents,
        FrameStep::NetworkStep,
        FrameStep::LoginEvents,
        FrameStep::PacketStep,
        FrameStep::AssetCacheStep,
        FrameStep::UiStep,
        FrameStep::WorldViewStep,
        FrameStep::PrepareDevice,
        FrameStep::BeginFrame,
        FrameStep::DrawWorld,
        FrameStep::PresentFrame,
        FrameStep::PaceFrame,
    ];

    /// The name of the client function represented by this step.
    #[must_use]
    pub const fn call(self) -> &'static str {
        match self {
            Self::UiQueueStep => "ui-queue",
            Self::ClockSample => "clock-sample",
            Self::ProcessWindowEvents => "window-events",
            Self::NetworkStep => "network",
            Self::LoginEvents => "login-events",
            Self::PacketStep => "packets",
            Self::AssetCacheStep => "asset-cache",
            Self::UiStep => "ui",
            Self::WorldViewStep => "world-view",
            Self::PrepareDevice => "prepare-device",
            Self::BeginFrame => "begin-frame",
            Self::DrawWorld => "draw-world",
            Self::PresentFrame => "present-frame",
            Self::PaceFrame => "pace-frame",
        }
    }
}

/// The target the frame loop's spans are recorded under, so a filter can ask for them alone
/// (`dereth_client_runtime::frame=trace` next to any other level).
pub const SPAN_TARGET: &str = "dereth_client_runtime::frame";

/// The frame loop's spans: a `debug` span per frame (`frame`, with its number) and, inside it, a
/// `trace` span per [`FrameStep`] (`step`, with the step's name), entered as the step starts and
/// closed as the next one does.
///
/// A step is a stretch of the frame and not a block of code, so it is held here rather than as a
/// lexical guard: [`Self::step`] closes the step in progress and opens the next, and dropping the
/// value closes the last one, on an early return as on the ordinary end of the frame. Any event
/// logged during the frame carries the frame number and the step it happened in, and a subscriber
/// that times spans gets each step's duration.
///
/// With neither level enabled a span costs one cached check and records nothing.
#[derive(Debug)]
pub struct FrameSpans {
    // Declared after `step`: fields drop in declaration order, so the step closes inside its
    // frame.
    step: Option<tracing::span::EnteredSpan>,
    _frame: tracing::span::EnteredSpan,
}

impl FrameSpans {
    /// Open the span of frame `n` (counted from 1).
    #[must_use]
    pub fn begin(n: u64) -> Self {
        Self {
            step: None,
            _frame: tracing::debug_span!(target: SPAN_TARGET, "frame", n).entered(),
        }
    }

    /// Close the step in progress, if any, and open `step`'s.
    pub fn step(&mut self, step: FrameStep) {
        self.step = None;
        self.step =
            Some(tracing::trace_span!(target: SPAN_TARGET, "step", name = step.call()).entered());
    }
}

/// What a frame actually did, in call order: a **view** of the step order in a
/// [`crate::frame_events::FrameEvents`], not a store of its own.
///
/// [`crate::app::App::last_frame_steps`] is this view over the app's own log.
#[derive(Debug, Clone, Copy)]
pub struct FrameRecorder<'a> {
    events: &'a crate::frame_events::FrameEvents,
}

impl<'a> FrameRecorder<'a> {
    /// View `events`' step order.
    #[must_use]
    pub const fn new(events: &'a crate::frame_events::FrameEvents) -> Self {
        Self { events }
    }

    /// The frame in progress, in call order — which, once `App::frame` has returned, is the
    /// frame that just ran.
    #[must_use]
    pub fn steps(&self) -> &'a [FrameStep] {
        self.events.frame_steps()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    // Oracle: the numbered per-frame order, with the outer frame's prepended call in front.
    #[test]
    fn the_order_is_the_documented_one_and_has_no_gaps() {
        let names: Vec<&str> = FrameStep::ORDER.iter().map(|s| s.call()).collect();
        assert_eq!(
            names,
            vec![
                "ui-queue",
                "clock-sample",
                "window-events",
                "network",
                "login-events",
                "packets",
                "asset-cache",
                "ui",
                "world-view",
                "prepare-device",
                "begin-frame",
                "draw-world",
                "present-frame",
                "pace-frame",
            ]
        );
    }

    // "input and network are consumed **before** simulation, simulation **before** rendering, and
    // the UI tick sits between them because it advances the UI mode flow." (Rebuild notes.)
    #[test]
    fn the_three_orderings_the_rebuild_notes_call_behaviourally_significant_hold() {
        let pos = |s: FrameStep| {
            FrameStep::ORDER
                .iter()
                .position(|x| *x == s)
                .expect("present")
        };
        assert!(pos(FrameStep::ProcessWindowEvents) < pos(FrameStep::WorldViewStep));
        assert!(pos(FrameStep::NetworkStep) < pos(FrameStep::WorldViewStep));
        assert!(pos(FrameStep::WorldViewStep) < pos(FrameStep::BeginFrame));
        assert!(pos(FrameStep::UiStep) > pos(FrameStep::NetworkStep));
        assert!(pos(FrameStep::UiStep) < pos(FrameStep::WorldViewStep));
        // The clock is sampled once, first.
        assert_eq!(pos(FrameStep::ClockSample), 1);
        // The sleep is last.
        assert_eq!(pos(FrameStep::PaceFrame), FrameStep::ORDER.len() - 1);
    }

    #[test]
    fn the_recorder_reports_what_the_log_was_given() {
        use crate::frame_events::{FrameEvent, FrameEvents};
        let mut log = FrameEvents::new();
        log.push(FrameEvent::Step(FrameStep::ClockSample));
        log.push(FrameEvent::FrameDrawn);
        log.push(FrameEvent::Step(FrameStep::ProcessWindowEvents));
        assert_eq!(
            FrameRecorder::new(&log).steps(),
            &[FrameStep::ClockSample, FrameStep::ProcessWindowEvents],
            "the view reports the steps and only the steps"
        );
        log.drain_frame();
        assert!(FrameRecorder::new(&log).steps().is_empty());
    }
}
