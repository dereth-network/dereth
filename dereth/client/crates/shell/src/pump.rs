//! The message pump: host events in, Win32 messages out, the window procedure in the middle,
//! and the input manager behind it.
//!
//! The window's lifecycle messages -- close, destroy, focus, size -- are the runtime's window
//! procedure's ([`dereth_client_runtime::pump`]); this module adds the device half: the keyboard,
//! mouse and wheel messages the input manager reads, with the virtual key, the scan code and the
//! pointer position packed the way Windows packs them. A host's own key codes are its own
//! business: the desktop maps its window system's, a browser the page's, onto the shared key
//! identities before an event reaches here.
//!
//! **Nothing here interprets a message.** The mapping stops at the `MSG` quadruple; what a key
//! *means* is the input manager's, and the window procedure's `ForwardToInputManager` effect is
//! where it attaches.

use dereth_client_contract::window_proc::WndProcResult;
use dereth_client_runtime::pump::WindowMessage;

use dereth_input::host::HostEvent;
use {dereth_input::keys::Key, dereth_input::keys::MouseButton};

/// The window procedure model, re-exported for test-support crates that may not name the
/// render crate. A re-export adds no dependency and changes no behaviour.
pub use dereth_render::window_proc;

/// The `MSG` forwarded to the input manager: the four `WndProc` parameters plus
/// `GetMessageTime()`. The input crate owns it; the host builds it here.
pub use dereth_input::win32::Win32Message;

/// A lifecycle message the runtime's window procedure is also sent, with its time.
#[must_use]
pub const fn from_window(m: WindowMessage, time_ms: u32) -> Win32Message {
    Win32Message::new(m.message, m.wparam, m.lparam, time_ms)
}

/// The same message, for the window procedure.
#[must_use]
pub const fn window_message(m: Win32Message) -> WindowMessage {
    WindowMessage::new(m.message, m.wparam, m.lparam)
}

pub use dereth_input::pump::{
    key_lparam, key_text_messages, lparam_bits, make_lparam, mouse_message, DeviceMessages,
};

/// The message pump: the window procedure's state and the device half of the mapping together,
/// for a caller that drives a window procedure of its own rather than the application's.
///
/// Everything about the window's state is [`dereth_client_runtime::pump::Pump`]'s, through
/// `Deref`; what this adds is the host's device events.
#[derive(Debug, Default)]
pub struct Pump {
    /// The window procedure's state.
    pub window: dereth_client_runtime::pump::Pump,
    /// The device half of the mapping.
    pub devices: DeviceMessages,
}

impl std::ops::Deref for Pump {
    type Target = dereth_client_runtime::pump::Pump;
    fn deref(&self) -> &Self::Target {
        &self.window
    }
}

impl std::ops::DerefMut for Pump {
    fn deref_mut(&mut self) -> &mut Self::Target {
        &mut self.window
    }
}

impl Pump {
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// One dispatched message: run the message through and accumulate its effects.
    pub fn dispatch(&mut self, m: Win32Message) -> WndProcResult {
        self.window.dispatch(window_message(m), m.time_ms)
    }

    /// Map one host event onto the messages Windows would have sent, and dispatch them.
    pub fn on_window_event(&mut self, event: &HostEvent, time_ms: u32) -> Vec<WndProcResult> {
        self.map_window_event(event, time_ms)
            .into_iter()
            .map(|m| self.dispatch(m))
            .collect()
    }

    /// The mapping alone, with no dispatch.
    pub fn map_window_event(&mut self, event: &HostEvent, time_ms: u32) -> Vec<Win32Message> {
        match crate::platform::window::lifecycle(event) {
            Some(lifecycle) => dereth_client_runtime::pump::Pump::map_window_event(&lifecycle)
                .into_iter()
                .map(|m| from_window(m, time_ms))
                .collect(),
            None => self.devices.map_device_event(event, time_ms),
        }
    }

    /// The `WM_MOUSEMOVE` for a pointer position. See [`DeviceMessages::mouse_move_message`].
    pub fn mouse_move_message(&mut self, x: f64, y: f64, time_ms: u32) -> Win32Message {
        self.devices.mouse_move_message(x, y, time_ms)
    }

    /// The button message for one transition. See [`DeviceMessages::button_message`].
    pub fn button_message(
        &mut self,
        button: MouseButton,
        pressed: bool,
        time_ms: u32,
    ) -> Option<Win32Message> {
        self.devices.button_message(button, pressed, time_ms)
    }

    /// The key message for one resolved key. See [`DeviceMessages::key_message_for_key`].
    pub fn key_message_for_key(&mut self, key: Key, pressed: bool, time_ms: u32) -> Win32Message {
        self.devices.key_message_for_key(key, pressed, time_ms)
    }

    /// The key message for one key transition. See [`DeviceMessages::key_message`].
    pub fn key_message(
        &mut self,
        pressed: bool,
        vk: usize,
        scan: u16,
        time_ms: u32,
    ) -> Win32Message {
        self.devices.key_message(pressed, vk, scan, time_ms)
    }

    /// The event-loop epilogue: the window's own epilogue (the Alt+Enter latch) runs and the done
    /// flag is returned.
    pub fn finish_event_drain(&mut self, renderer_exists: bool) -> bool {
        self.window.finish_event_drain(renderer_exists)
    }
}
