//! The window procedure's state and its lifecycle messages: the window opening, closing, gaining
//! and losing focus, and being minimised.
//!
//! The window procedure itself is a pure `(state, message) -> effects` function
//! ([`dereth_client_contract::window_proc`]); this is the half that feeds it the messages a
//! window's lifecycle produces and keeps the state it writes -- whether the client is the active
//! application (which paces the frame), whether it is minimised, whether a full-screen toggle is
//! pending and whether the client is done. The host resolves its own events into plain
//! `HostEvent`s first, so nothing here names a window system.
//!
//! **Device input is not here.** Keys, mouse buttons and the pointer are the front end's: it turns
//! them into actions for the runtime and, where the retail window procedure has an arm for one of
//! them (the Alt+Enter full-screen toggle, for instance), hands that message to
//! `Pump::dispatch` itself.
//!
//! Two places a host's lifecycle events are not Win32's, and what the mapping does about each:
//!
//! 1. **Focus.** Windows sends `WM_ACTIVATEAPP`, `WM_ACTIVATE` and `WM_SETFOCUS` separately; a
//!    host that reports one `Focused(bool)` has all three re-emitted.
//! 2. **Minimise.** A resize to a zero extent is the only `SIZE_MINIMIZED` signal a host gives, so
//!    it is what maps to it. A non-zero resize maps to `WM_SIZE` with `SIZE_RESTORED`, which the
//!    client's table handles by doing nothing at all.

use dereth_client_contract::window_proc::{
    finish_event_loop, msg, wnd_proc, DeviceState, Effect, WndProcResult,
};

use crate::platform::window::HostEvent;

/// One message for the window procedure: its id and its two parameters.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct WindowMessage {
    pub message: u32,
    pub wparam: usize,
    pub lparam: isize,
}

impl WindowMessage {
    #[must_use]
    pub const fn new(message: u32, wparam: usize, lparam: isize) -> Self {
        Self {
            message,
            wparam,
            lparam,
        }
    }
}

/// The window procedure's state, and what it has asked for.
#[derive(Debug, Default)]
pub struct Pump {
    /// The device-state flags the table reads and writes.
    pub state: DeviceState,
    /// The message time of the last message dispatched, milliseconds.
    pub last_message_time_ms: u32,
    /// Every effect the table asked for during this drain, in call order.
    pub effects: Vec<Effect>,
}

impl Pump {
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// One message through the window procedure, with the time it was sent at; its effects are
    /// accumulated.
    pub fn dispatch(&mut self, m: WindowMessage, time_ms: u32) -> WndProcResult {
        self.last_message_time_ms = time_ms;
        let r = wnd_proc(&mut self.state, m.message, m.wparam, m.lparam);
        self.effects.extend_from_slice(&r.effects);
        r
    }

    /// Map one lifecycle event onto the messages Windows would have sent, and dispatch them.
    ///
    /// Returns the results in order, so a caller can see what the table decided. An event that maps
    /// to no message the client handles yields an empty vector -- the equivalent of falling through
    /// to `DefWindowProcA`.
    pub fn on_window_event(&mut self, event: &HostEvent, time_ms: u32) -> Vec<WndProcResult> {
        Self::map_window_event(event)
            .into_iter()
            .map(|m| self.dispatch(m, time_ms))
            .collect()
    }

    /// The mapping alone, with no dispatch. Split out so it can be inspected, and so a front end
    /// whose own input layer also listens to the lifecycle messages can hand it the same ones.
    #[must_use]
    pub fn map_window_event(event: &HostEvent) -> Vec<WindowMessage> {
        let one = |message: u32, wparam: usize| vec![WindowMessage::new(message, wparam, 0)];
        match event {
            HostEvent::CloseRequested => one(msg::WM_CLOSE, 0),
            HostEvent::Destroyed => one(msg::WM_DESTROY, 0),

            // Windows sends WM_ACTIVATEAPP, WM_ACTIVATE and WM_SETFOCUS separately; a host that
            // collapses all three into one Focused(bool) has all three re-emitted.
            //
            // **WM_ACTIVATE is not optional here.** It is the only message that *clears*
            // `is_minimized` -- it is set to `HIWORD(wParam) != 0` -- and the flag starts
            // true (the device flag's initial value). Without it
            // WM_ACTIVATEAPP's `wParam != 0 && !is_minimized` guard never passes, the client
            // never becomes the active application, and frame pacing caps every frame at 99 ms
            // for the life of the process. Measured without it: 22 frames in 3 seconds.
            //
            // wParam's LOWORD is WA_ACTIVE (1) / WA_INACTIVE (0) and its HIWORD is the minimised
            // flag, which is zero because a host only reports focus for a window that is showing.
            HostEvent::Focused(gained) => vec![
                WindowMessage::new(msg::WM_ACTIVATEAPP, usize::from(*gained), 0),
                WindowMessage::new(msg::WM_ACTIVATE, usize::from(*gained), 0),
                WindowMessage::new(
                    if *gained {
                        msg::WM_SETFOCUS
                    } else {
                        msg::WM_KILLFOCUS
                    },
                    0,
                    0,
                ),
            ],

            // The window is not resizable; a zero extent is the minimise signal.
            HostEvent::Resized { width, height } => {
                let minimized = *width == 0 || *height == 0;
                one(
                    msg::WM_SIZE,
                    if minimized { msg::SIZE_MINIMIZED } else { 0 },
                )
            }
            // `WM_DPICHANGED` reaches no arm of the window message table: the client's window is
            // not resizable and the negotiation happens inside the host. `App` reads it directly.
            HostEvent::ScaleFactorChanged => Vec::new(),
        }
    }

    /// The event-loop epilogue: the Alt+Enter latch, then return the done flag.
    pub fn finish_event_drain(&mut self, renderer_exists: bool) -> bool {
        finish_event_loop(&mut self.state, renderer_exists)
    }

    /// The device-done flag is the single quit switch through which every quit path routes.
    pub fn done(&mut self) {
        self.state.is_done = true;
    }

    /// Drop the effects accumulated so far, which a frame does after handing them on.
    pub fn take_effects(&mut self) -> Vec<Effect> {
        std::mem::take(&mut self.effects)
    }
}
