//! The message pump, as the desktop drives it: `winit` events in, Win32 messages out.
//!
//! The pump itself -- the window procedure's state, the device half of the mapping, and the
//! conversions between a window-procedure message and the input manager's -- is the client
//! shell's ([`dereth_client_shell::pump`]), re-exported here. What this module adds is `winit`'s:
//! its key codes and mouse buttons mapped onto the shared identities, on the desktop's [`Pump`].
//!
//! # The three places `winit` is not Win32
//!
//! 1. **Alt+F4.** The client's window procedure swallows every Alt-key system command except two:
//!    `VK_F4` and `VK_RETURN` reach `DefWindowProc`. For Alt+F4 the default procedure answers with
//!    a close, which ends the client. `winit` owns the window procedure and does the same, so the
//!    close arrives as `CloseRequested` and maps to `WM_CLOSE` like the close button's.
//!
//!    Tracing every event a real Alt+F4 produces gives, in order: `ModifiersChanged(ALT)`,
//!    `KeyboardInput(AltLeft, Pressed)`, `CloseRequested`. There is **no `KeyboardInput` for F4 at
//!    all** — `winit`'s key-event builder defers a `WM_SYSKEYDOWN` until it knows whether a
//!    character follows, and `DefWindowProc` has already produced the close by then. Nothing here
//!    needs to see the F4.
//! 2. **Focus.** Windows sends `WM_ACTIVATEAPP`, `WM_ACTIVATE` and `WM_SETFOCUS` separately;
//!    `winit` collapses them into one `Focused(bool)`. The pump re-expands it into the two the
//!    client's table distinguishes: `WM_ACTIVATEAPP` (which activates/deactivates) and
//!    `WM_SETFOCUS`/`WM_KILLFOCUS` (which only forward to the input manager).
//! 3. **Minimise.** `winit` reports `WM_SIZE` as `Resized`, with a zero extent when the window is
//!    minimised. That zero extent is the only `SIZE_MINIMIZED` signal available, so it is what maps
//!    to it. A non-zero resize maps to `WM_SIZE` with `SIZE_RESTORED`, which the client's table
//!    handles by doing nothing at all — **the window is not resizable and the client's window
//!    procedure has no geometry handling**.

use winit::keyboard::KeyCode;

pub use {
    dereth_client_contract::window_proc, dereth_client_shell::pump::from_window,
    dereth_client_shell::pump::window_message, dereth_input::pump::key_lparam,
    dereth_input::pump::key_text_messages, dereth_input::pump::lparam_bits,
    dereth_input::pump::make_lparam, dereth_input::pump::mouse_message,
    dereth_input::pump::DeviceMessages, dereth_input::win32::Win32Message,
};

/// Resolve a host key through the shared physical-key table.
#[must_use]
pub fn scan_code_from_key_code(code: KeyCode) -> Option<u16> {
    dereth_input::pump::scan_code_from_key_code(crate::platform::window::input_key_code(code))
}

/// Resolve a host key through the shared virtual-key table.
#[must_use]
pub fn vk_from_key_code(code: KeyCode) -> Option<usize> {
    dereth_input::pump::vk_from_key_code(crate::platform::window::input_key_code(code))
}

/// The message pump with `winit`'s keys and buttons: the shell's
/// [`dereth_client_shell::pump::Pump`], through `Deref`, plus the two mappings from `winit`'s own
/// types.
#[derive(Debug, Default)]
pub struct Pump(dereth_client_shell::pump::Pump);

impl std::ops::Deref for Pump {
    type Target = dereth_client_shell::pump::Pump;
    fn deref(&self) -> &Self::Target {
        &self.0
    }
}

impl std::ops::DerefMut for Pump {
    fn deref_mut(&mut self) -> &mut Self::Target {
        &mut self.0
    }
}

impl Pump {
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// [`dereth_client_shell::pump::Pump::button_message`] over `winit`'s own button.
    pub fn mouse_button_message(
        &mut self,
        button: winit::event::MouseButton,
        pressed: bool,
        time_ms: u32,
    ) -> Option<Win32Message> {
        self.0.button_message(
            crate::platform::window::mouse_button(button),
            pressed,
            time_ms,
        )
    }

    /// The whole keyboard mapping for one physical key transition: virtual key, scan code, and the
    /// `WM_KEY*` / `WM_SYSKEY*` choice.
    ///
    /// Split out because `winit::event::KeyEvent` carries a private platform field and cannot be
    /// constructed in a test, and this is the half worth testing.
    #[must_use]
    pub fn key_message_for(
        &mut self,
        code: KeyCode,
        pressed: bool,
        time_ms: u32,
    ) -> Option<Win32Message> {
        // A key winit knows and the platform has no scan code for cannot produce a control
        // specification, so it is not a message the input pipeline could read.
        let key = crate::platform::window::key_from_key_code(code)?;
        Some(self.0.key_message_for_key(key, pressed, time_ms))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use dereth_client_contract::window_proc::{msg, Effect};
    use dereth_input::host::HostEvent;

    fn ready() -> Pump {
        let mut p = Pump::new();
        p.state.is_ready = true;
        p
    }

    /// The scripted sequence passes through the real pump.
    #[test]
    fn the_scripted_sequence_passes_through_the_real_pump() {
        let mut p = ready();

        // Activation. winit's one Focused(true) becomes the three messages Windows would send.
        let msgs = p.map_window_event(&HostEvent::Focused(true), 10);
        assert_eq!(
            msgs.iter().map(|m| m.message).collect::<Vec<_>>(),
            vec![msg::WM_ACTIVATEAPP, msg::WM_ACTIVATE, msg::WM_SETFOCUS]
        );
        // is_minimized starts **true**, so WM_ACTIVATEAPP alone cannot activate: WM_ACTIVATE is
        // what clears the flag, and only then does the client become the active application.
        assert!(p.state.is_minimized);
        let r = p.on_window_event(&HostEvent::Focused(true), 10);
        assert!(
            r[0].effects.is_empty(),
            "WM_ACTIVATEAPP is refused while minimised"
        );
        assert!(!p.state.is_minimized, "WM_ACTIVATE cleared it");
        assert_eq!(r[1].effects, vec![Effect::Activate]);
        assert_eq!(r[2].effects, vec![Effect::ForwardToInputManager]);
        assert!(p.state.is_active_app);
        assert_eq!(p.last_message_time_ms, 10);

        // Idempotent: Activate only fires on a transition.
        let r = p.on_window_event(&HostEvent::Focused(true), 11);
        assert!(r.iter().all(|x| x.effects != vec![Effect::Activate]));

        // Deactivation.
        let r = p.on_window_event(&HostEvent::Focused(false), 12);
        assert_eq!(r[0].effects, vec![Effect::Deactivate]);
        assert!(
            r[1].effects.is_empty(),
            "already deactivated by WM_ACTIVATEAPP"
        );
        assert_eq!(r[2].effects, vec![Effect::ForwardToInputManager]);
        assert!(!p.state.is_active_app);

        // Minimise: a zero extent is winit's SIZE_MINIMIZED.
        p.state.is_active_app = true;
        p.state.is_minimized = false;
        let r = p.on_window_event(
            &HostEvent::Resized {
                width: 0,
                height: 0,
            },
            13,
        );
        assert!(p.state.is_minimized);
        assert!(!p.state.is_active_app);
        assert_eq!(r[0].effects, vec![Effect::Deactivate]);

        // A real resize is handled and changes nothing: the window is not resizable and WndProc has
        // no geometry handling at all.
        let before = p.state;
        let r = p.on_window_event(
            &HostEvent::Resized {
                width: 1024,
                height: 768,
            },
            14,
        );
        assert!(r[0].handled);
        assert!(r[0].effects.is_empty());
        assert_eq!(p.state, before);

        // Alt+Enter latches the toggle; the flip happens at the end of the drain.
        let mut p = ready();
        p.state.is_active_app = true;
        p.devices.alt_down = true;
        let m = p.key_message(true, msg::VK_RETURN, 0x001C, 20);
        assert_eq!(m.message, msg::WM_SYSKEYDOWN);
        p.dispatch(m);
        assert!(p.state.toggle_full_screen_mode);
        assert!(!p.finish_event_drain(true), "not done yet");
        assert!(p.state.full_screen, "the latch flipped the preference");
        assert!(!p.state.toggle_full_screen_mode);

        // The close button ends the loop.
        let mut p = ready();
        p.on_window_event(&HostEvent::CloseRequested, 30);
        assert!(p.state.is_done);
        assert!(
            p.finish_event_drain(true),
            "the event loop returns the done flag"
        );
    }

    // Oracle: the retail window procedure's WM_SYSKEYDOWN arm clears its handled flag for
    // `wParam == VK_F4` (0x73), so Alt+F4 reaches the default procedure, whose answer is a close.
    #[test]
    fn alt_f4_reaches_the_default_procedure_and_the_close_it_provokes_ends_the_client() {
        let mut p = ready();
        p.state.is_active_app = true;
        p.devices.alt_down = true;

        let m = p.key_message(true, msg::VK_F4, 0x003E, 40);
        assert_eq!(m.message, msg::WM_SYSKEYDOWN);
        let r = p.dispatch(m);
        assert!(!r.handled, "the default procedure sees Alt+F4");
        assert_eq!(r.effects, vec![Effect::ForwardToInputManager]);
        assert!(!p.state.is_done, "the key itself changes nothing");

        // The default procedure's close arrives as CloseRequested, Alt still held.
        let msgs = p.map_window_event(&HostEvent::CloseRequested, 41);
        assert_eq!(msgs.len(), 1, "the close is not dropped");
        p.on_window_event(&HostEvent::CloseRequested, 41);
        assert!(p.state.is_done, "Alt+F4 ends the client");
        assert!(p.finish_event_drain(true));
    }

    // The event sequence a real Alt+F4 actually produces through winit, transcribed from a traced
    // run of the windowed client: ModifiersChanged(ALT), KeyboardInput(AltLeft), CloseRequested,
    // with no F4 key event at any point. See the module documentation.
    #[test]
    fn the_close_winit_delivers_for_a_real_alt_f4_ends_the_client_without_ever_seeing_f4() {
        let mut p = ready();
        p.state.is_active_app = true;

        // ModifiersChanged(ALT) -- no message, but the pump now knows Alt is down.
        let m = HostEvent::ModifiersChanged { alt: true };
        assert!(p.map_window_event(&m, 90).is_empty());
        assert!(p.devices.alt_down, "the modifier arm is what sets it");

        // The Alt keydown itself arrives as a WM_SYSKEYDOWN and is swallowed by the table because
        // sys_keys_enabled is false.
        let m = p.key_message(true, 0xA4, 0x0038, 91); // VK_LMENU / DIK_LMENU
        assert_eq!(m.message, msg::WM_SYSKEYDOWN);
        assert!(p.dispatch(m).handled);

        // ... and then the default procedure's close, which ends the client.
        p.on_window_event(&HostEvent::CloseRequested, 92);
        assert!(p.state.is_done, "Alt+F4 closes the client");
    }

    // Oracle: `sys_keys_enabled` starts true, then window initialization clears it when the
    // window has no menu. With the flag false, WM_SYSKEYDOWN/UP are swallowed, so Alt-key system
    // commands remain disabled for the life of the client.
    #[test]
    fn alt_key_system_commands_are_disabled_for_the_life_of_the_client() {
        let mut p = ready();
        assert!(!p.state.sys_keys_enabled);
        p.devices.alt_down = true;
        let m = p.key_message(true, 0x41, 0x001E, 70); // Alt+A / DIK_A
        assert_eq!(m.message, msg::WM_SYSKEYDOWN);
        let r = p.dispatch(m);
        assert!(r.handled, "swallowed because sys_keys_enabled is false");
        assert_eq!(r.effects, vec![Effect::ForwardToInputManager]);
    }

    // Oracle: the mouse block gives focus on the first click-type message, arms
    // TrackMouseEvent(TME_LEAVE) on the first mouse message, then dispatches each message to the
    // UI and forwards it to the input manager.
    //
    // Driven through Win32Message rather than through winit's MouseInput because constructing a
    // winit::event::DeviceId needs `unsafe` and this crate forbids it; the winit -> message half is
    // covered by `the_button_mapping_covers_the_0x201_block` below.
    #[test]
    fn the_mouse_path_arms_track_mouse_event_once_per_leave() {
        let mut p = ready();
        p.state.is_active_app = true;
        let r = p.dispatch(Win32Message::new(msg::WM_LBUTTONDOWN, 0, 0, 80));
        assert_eq!(
            r.effects,
            vec![
                Effect::SetFocus,
                Effect::TrackMouseEvent,
                Effect::ForwardToBrowser,
                Effect::ForwardToInputManager
            ]
        );
        // The second message does not re-arm.
        let r = p.dispatch(Win32Message::new(
            msg::WM_MOUSEMOVE,
            0,
            make_lparam(12.0, 34.0),
            81,
        ));
        assert_eq!(
            r.effects,
            vec![Effect::ForwardToBrowser, Effect::ForwardToInputManager]
        );
        // WM_MOUSELEAVE disarms it.
        let r = p.dispatch(Win32Message::new(msg::WM_MOUSELEAVE, 0, 0, 82));
        assert_eq!(r.effects, vec![Effect::ForwardToInputManager]);
        assert!(!p.state.track_leave_called);
    }

    /// The scan-code table is a transcription of `winit`'s **Windows** table, and on Windows that
    /// table is the oracle for it. Walking every code `from_scancode` recognises reaches every
    /// `KeyCode` the backend can name without this test hand-listing them, so a transcription slip
    /// cannot hide in a key the list forgot.
    ///
    /// This is the guard that keeps [`scan_code_from_key_code`] honest now that it no longer
    /// delegates. It runs on Windows only because it is the Windows table that is being checked;
    /// `the_keys_the_shipped_combat_band_binds_reach_their_dik_offsets` below is the arm that runs
    /// everywhere, and it is the one that would have caught the Linux fault.
    #[cfg(windows)]
    #[test]
    fn the_scan_code_table_is_winits_windows_table() {
        use winit::keyboard::PhysicalKey;
        use winit::platform::scancode::PhysicalKeyExtScancode;

        let mut checked = 0_u32;
        for raw in (0x0000..=0x00FF_u32).chain(0xE000..=0xE0FF_u32) {
            let PhysicalKey::Code(code) = PhysicalKey::from_scancode(raw) else {
                continue;
            };
            // `Lang1`/`Lang2` are the one layout-dependent pair; see the doc comment.
            if matches!(code, KeyCode::Lang1 | KeyCode::Lang2) {
                continue;
            }
            let winit = PhysicalKey::Code(code)
                .to_scancode()
                .and_then(|v| u16::try_from(v).ok());
            assert_eq!(
                scan_code_from_key_code(code),
                winit,
                "{code:?} (reached from scan code {raw:#06X})"
            );
            checked += 1;
        }
        // A `from_scancode` that stopped answering would turn this test into a no-op.
        assert!(
            checked > 100,
            "only {checked} key codes were reached; the walk found nothing"
        );
    }
}
