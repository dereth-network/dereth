//! The frame's one extension point: a step a host installs ([`crate::Client::install_frame_hook`]),
//! run once a frame at the frame's end, after the frame's messages have taken effect.
//!
//! The runtime names nothing that plugs in here (a plugin host is one such step). Each frame the
//! hook is handed a [`HookFrame`]: the events since it last ran, each once and in order, the
//! notices the player's scripts sent the panels since then, read, each once and in order, the
//! frame's clocks, where the session stands and the game's view as the host last refreshed it
//! ([`crate::Client::refresh_view`]). It answers with actions, the same actions a front
//! end gives the client; the client carries them out as soon as the step returns, so what they send
//! goes out with the frame's datagrams.
//!
//! The events and the notices the hook sees are copies: the host still takes every event with
//! [`crate::Client::take_events`] and every notice with [`crate::Client::take_notices`], whether
//! the hook ran or not, and when it takes them between two of the hook's frames the hook is
//! handed them at the next. Events and notices from before the hook was installed are not handed
//! to it.

use std::any::Any;

use auberean_client_contract::event::GameEvent;
use auberean_client_contract::notice::Notice;
use auberean_client_contract::session::Phase;
use auberean_client_contract::view::View;
use auberean_client_contract::Action;

/// A step run once a frame (see the module documentation).
pub trait FrameHook: core::fmt::Debug {
    /// This frame's step.
    fn step(&mut self, frame: &mut HookFrame<'_>);

    /// The hook as `Any`, so that the host that installed it can reach its own type again.
    fn as_any(&self) -> &dyn Any;

    /// The hook as `Any`, mutably.
    fn as_any_mut(&mut self) -> &mut dyn Any;
}

/// One frame, as the hook sees it and answers it.
#[derive(Debug)]
pub struct HookFrame<'a> {
    /// What happened since the hook last ran, oldest first.
    pub events: &'a [GameEvent],
    /// What the player's scripts told the panels since the hook last ran, read, in the order sent.
    pub notices: &'a [Notice],
    /// The client's own clock this frame, seconds: the host's time for the frame.
    pub local_time: f64,
    /// The game clock this frame, seconds: the server's time once it has given it.
    pub game_time: f64,
    /// Where the session stands as the frame ends.
    pub phase: Phase,
    /// The game as the host last refreshed its view, when the frame carries it: a host that
    /// refreshes it once a frame hands the hook the last frame's.
    pub view: Option<&'a View>,
    actions: Vec<Action>,
}

impl<'a> HookFrame<'a> {
    /// A frame with these events, clocks and phase, and no answer yet.
    #[must_use]
    pub fn new(events: &'a [GameEvent], local_time: f64, game_time: f64, phase: Phase) -> Self {
        Self {
            events,
            notices: &[],
            local_time,
            game_time,
            phase,
            view: None,
            actions: Vec::new(),
        }
    }

    /// The same frame, carrying the notices told since the hook last ran.
    #[must_use]
    pub const fn with_notices(mut self, notices: &'a [Notice]) -> Self {
        self.notices = notices;
        self
    }

    /// The same frame, carrying `view`.
    #[must_use]
    pub const fn with_view(mut self, view: &'a View) -> Self {
        self.view = Some(view);
        self
    }

    /// Ask the client for something, as a front end would: carried out once the step returns, in
    /// the order asked.
    pub fn act(&mut self, action: Action) {
        self.actions.push(action);
    }

    /// The actions asked so far, taken.
    pub fn take_actions(&mut self) -> Vec<Action> {
        std::mem::take(&mut self.actions)
    }
}

/// The notices the hook has yet to be handed: those the host took since the hook last ran, and
/// where the ones still waiting for the host start.
#[derive(Debug, Default)]
pub(crate) struct HookNotices {
    held: Vec<Notice>,
    from: usize,
}

impl HookNotices {
    /// None yet; the hook is handed the notices from the `from`th still waiting for the host on.
    pub(crate) const fn from(from: usize) -> Self {
        Self {
            held: Vec::new(),
            from,
        }
    }

    /// The host took `notices` (every one waiting): those the hook has not been handed are kept
    /// for it.
    pub(crate) fn taken(&mut self, notices: &[Notice]) {
        if let Some(rest) = notices.get(self.from..) {
            self.held.extend_from_slice(rest);
        }
        self.from = 0;
    }

    /// The notices the hook has not been handed, `waiting` being those still waiting for the host:
    /// in the order sent, each once.
    pub(crate) fn take(&mut self, waiting: &[crate::host::PanelNotice]) -> Vec<Notice> {
        let mut out = std::mem::take(&mut self.held);
        if let Some(rest) = waiting.get(self.from..) {
            out.extend(rest.iter().map(crate::said::read));
        }
        self.from = waiting.len();
        out
    }
}
