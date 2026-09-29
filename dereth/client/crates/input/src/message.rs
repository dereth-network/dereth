//! The window-message arm of the input manager, the whole of stage 2.
//!
//! The pure decoding helpers are in [`crate::win32`]; this is where they meet the manager's state.

use crate::fire::ControlType;
use crate::spec::{ControlCode, SubControlIndex};
use crate::win32::{self, msg, Win32Message};
use crate::{DeviceType, InputManager};

impl InputManager {
    /// Stage 1-2: the window procedure sets its handled flag up
    /// front and clears it only in the default case.
    ///
    /// `m.time_ms` **must** be the message-queue timestamp (`GetMessageTime()`), not a
    /// high-resolution clock: the tap and double-click thresholds are the user's own OS settings
    /// and are read once at start-up. The client's pump already carries it alongside every message.
    ///
    /// `is_dbcs_lead` is the caller's `IsDBCSLeadByte`, which is a function of the process code
    /// page and so is injected rather than assumed.
    pub fn on_window_event(&mut self, m: &Win32Message, is_dbcs_lead: &dyn Fn(u8) -> bool) -> bool {
        // The window procedure decides what reaches the input manager at all.
        match win32::wnd_proc_disposition(m) {
            win32::WndProcDisposition::Swallowed
            | win32::WndProcDisposition::Answered(_)
            | win32::WndProcDisposition::Default => return false,
            win32::WndProcDisposition::Forward
            | win32::WndProcDisposition::ForwardAfterSetFocus => {}
        }

        match m.message {
            msg::WM_SETFOCUS => {
                self.has_focus = true;
                // If mouse-look was wanted, re-enter it.
                if self.mouse.want_mouse_look && !self.mouse.in_mouse_look {
                    self.set_mouse_look_mode(true, m.time_ms);
                }
                true
            }
            msg::WM_KILLFOCUS => {
                self.release_pressed_keys();
                self.has_focus = false;
                if self.mouse.want_mouse_look && self.mouse.in_mouse_look {
                    self.mouse.leave_mouse_look();
                }
                true
            }
            msg::WM_CANCELMODE => {
                // Zero the capture count, release capture, release the pressed keys.
                self.mouse.cancel_capture();
                self.release_pressed_keys();
                true
            }
            msg::WM_KEYDOWN | msg::WM_SYSKEYDOWN | msg::WM_KEYUP | msg::WM_SYSKEYUP => {
                let down = matches!(m.message, msg::WM_KEYDOWN | msg::WM_SYSKEYDOWN);
                if down {
                    // Mark a key-down in progress and clear the ignore-next-char flag.
                    self.text.begin_key_down();
                }
                self.generate_keyboard_event(m.lparam, down, m.time_ms);
                if down {
                    // The key-down-in-progress flag is cleared on every exit path.
                    self.text.end_key_down();
                }
                true
            }
            msg::WM_CHAR | msg::WM_SYSCHAR => {
                // A WM_CHAR carries one byte of the ANSI code page; DBCS pairs before delivery.
                //
                // **The branch above `0xFF` is what keeps an IME commit from vanishing.**
                // The client's character arm is
                //
                // ```text
                //   b = wParam & 0xFF                 ; one ANSI byte
                //   if (a pending lead byte is held) {
                //       wide = MultiByteToWideChar(cp, 1, {lead, b, 0}, -1, 1 unit)
                //       deliver the character wide; clear the pending lead byte
                //   } else if (IsDBCSLeadByte(b)) {
                //       stash b as the pending lead byte and return
                //   } else {
                //       deliver the character (wchar_t)b
                //   }
                // ```
                //
                // so the byte-pair dance exists **only** because retail's window is ANSI
                // (`RegisterClassA` / `CreateWindowExA` / `DefWindowProcA`, and it uses
                // `IsDBCSLeadByte` and never `MultiByteToWideChar`). What it computes, on
                // every path, is the single `wchar_t` the character update takes.
                //
                // This rebuild's window is `winit`'s, which is **Unicode**: `WM_CHAR`'s `wParam`
                // already *is* that `wchar_t`. Narrowing it to a byte therefore threw away exactly
                // what retail's two-message dance exists to reconstruct — an IME- or dead-key
                // committed `U+4E2D` arrived as `0x2D`, a hyphen, silently. Below `0x100` the
                // transcription above is unchanged and the DBCS pairing still runs, so a host that
                // did deliver ANSI bytes behaves exactly as it did.
                if m.wparam > 0xFF {
                    #[allow(clippy::cast_possible_truncation)]
                    let unit = (m.wparam & 0xFFFF) as u16;
                    // A lone surrogate is not a character; the pending half is the host's problem,
                    // and dropping it here is what `char::from_u32` does for retail's pair too.
                    if let Some(ch) = char::from_u32(u32::from(unit)) {
                        if self.text.take_character(self.has_focus) {
                            self.characters.push(ch);
                        }
                    }
                    self.text.end_key_down();
                    return true;
                }
                #[allow(clippy::cast_possible_truncation)]
                let byte = (m.wparam & 0xFF) as u8;
                if let Some(ch) = self.dbcs.feed(byte, is_dbcs_lead) {
                    // The character update.
                    if self.text.take_character(self.has_focus) {
                        self.characters.push(ch);
                    }
                }
                self.text.end_key_down();
                true
            }
            msg::WM_MOUSEMOVE => {
                let (x, y) = signed_point(m.lparam);
                // Stored, **not** dispatched here: the per-frame input poll compares it with the previous
                // frame's.
                self.mouse.pos = (x, y);
                true
            }
            msg::WM_MOUSEWHEEL => {
                // (short)HIWORD(wParam).
                #[allow(clippy::cast_possible_truncation)]
                let delta = ((m.wparam >> 16) & 0xFFFF) as u16 as i16;
                self.generate_mouse_wheel_event(delta, m.time_ms);
                true
            }
            // Returns 1 as the XBUTTON protocol requires; handled either way.
            msg::WM_DEVICECHANGE => m.wparam == msg::DBT_DEVNODES_CHANGED,
            msg::WM_MOUSELEAVE => true,
            _ => match win32::mouse_button(m.message, m.wparam) {
                Some((offset, up)) => {
                    self.generate_mouse_button_event(offset, up, m.time_ms);
                    true
                }
                None => false,
            },
        }
    }

    /// Generate a keyboard event.
    fn generate_keyboard_event(&mut self, lparam: isize, down: bool, time_ms: u32) {
        let Some(offset) = win32::keyboard_offset(lparam) else {
            // A Windows auto-repeat down, discarded: the repeat you feel is generated internally by
            // the input manager's time-step processing.
            return;
        };
        let Some(idx) = self.device_index(DeviceType::Keyboard) else {
            return;
        };
        let cs = ControlCode::new(idx, SubControlIndex::None, offset);
        // PrintScreen only ever reaches the window as a key-up, so the press is synthesised.
        if !down && offset == win32::DIK_SYSRQ {
            self.fire_input_event(cs, ControlType::Button, 0x80, time_ms);
        }
        let prev = self.previous_control_state(cs, ControlType::Button);
        let data = i32::from(down) * 0x80;
        if prev.data != data {
            self.fire_input_event(cs, ControlType::Button, data, time_ms);
        }
    }

    /// Generate a mouse button event, including the reference-counted capture.
    fn generate_mouse_button_event(&mut self, offset: u16, up: bool, time_ms: u32) {
        if !up {
            self.mouse.add_capture();
        }
        if let Some(idx) = self.device_index(DeviceType::Mouse) {
            let cs = ControlCode::new(idx, SubControlIndex::None, offset);
            let data = if up { 0 } else { 0x80 };
            let prev = self.previous_control_state(cs, ControlType::Button);
            if prev.data != data {
                self.fire_input_event(cs, ControlType::Button, data, time_ms);
            }
        }
        if up {
            self.mouse.release_capture();
        }
    }

    /// The wheel is two virtual *buttons* (`DIMOFS_Z[+]` and
    /// `DIMOFS_Z[-]`), each clicked exactly once per message regardless of the delta's magnitude.
    fn generate_mouse_wheel_event(&mut self, delta: i16, time_ms: u32) {
        let Some(idx) = self.device_index(DeviceType::Mouse) else {
            return;
        };
        let sub = if delta > 0 {
            SubControlIndex::PositiveAxis
        } else {
            SubControlIndex::NegativeAxis
        };
        // DIMOFS_Z / DIMOFS_WHEEL is offset 8.
        let cs = ControlCode::new(idx, sub, 8);
        self.fire_input_event(cs, ControlType::Button, 0x80, time_ms);
        self.fire_input_event(cs, ControlType::Button, 0x00, time_ms);
    }

    pub(crate) fn device_index(&self, want: DeviceType) -> Option<u8> {
        self.keymap
            .devices
            .iter()
            .position(|d| d.device_type == want)
            .and_then(|i| u8::try_from(i).ok())
    }

    /// Drain the characters the client accepted. Empty unless text mode is on,
    /// the window has focus, and the ignore-next-char flag was not set.
    pub fn take_characters(&mut self) -> Vec<char> {
        std::mem::take(&mut self.characters)
    }
}

/// `(short)LOWORD(lParam), (short)HIWORD(lParam)` — the client coordinates of a mouse message.
fn signed_point(lparam: isize) -> (i32, i32) {
    // The lParam is a pair of packed 16-bit signed coordinates; both masks make the narrowing
    // exact, and the sign is what `(short)` restores.
    #[allow(clippy::cast_possible_truncation)]
    let l = lparam as usize as u32;
    #[allow(clippy::cast_possible_truncation)]
    let x = i32::from((l & 0xFFFF) as u16 as i16);
    #[allow(clippy::cast_possible_truncation)]
    let y = i32::from(((l >> 16) & 0xFFFF) as u16 as i16);
    (x, y)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::actionmap::ToggleType;
    use crate::dispatch::priority;
    use crate::keymap::{DeviceKeyMapEntry, InputMap, GUID_SYS_KEYBOARD, GUID_SYS_MOUSE};
    use crate::spec::{activation, ControlChord};
    use crate::{ActionId, InputMapId, MAP_BLOCK_KEYBOARD};
    use dereth_client_contract::actions::movement as movement_action;

    fn no_lead(_: u8) -> bool {
        false
    }

    /// A manager with the two shipped devices, the Shift meta key, and just enough of the movement
    /// and text-editing maps to exercise the pipeline end to end.
    fn manager() -> InputManager {
        let mut m = InputManager::empty();
        // The toggle types the shipped ActionMap gives these actions: Move Forward is a hold (1),
        // the UI mouse actions and scroll are holds, and Begin Chat Mode is a one-shot (3).
        m.action_map = crate::ActionMap::from_toggles(&[
            (4, movement_action::MOVE_FORWARD.0, ToggleType::Hold),
            (3, 7, ToggleType::Hold),
            (3, 10, ToggleType::Hold),
            (3, 5, ToggleType::Hold),
            (
                0x1000_000A,
                dereth_client_contract::actions::chat_entry::BEGIN_CHAT_MODE.0,
                ToggleType::OneShot,
            ),
        ]);
        m.keymap.devices = vec![
            DeviceKeyMapEntry {
                device_type: DeviceType::Keyboard,
                guid: GUID_SYS_KEYBOARD,
            },
            DeviceKeyMapEntry {
                device_type: DeviceType::Mouse,
                guid: GUID_SYS_MOUSE,
            },
        ];
        m.keymap.meta_keys = vec![(ControlCode(0x002A_0000), 0x8000_0000)];

        let mut movement = InputMap::new(InputMapId(4));
        let w = ControlCode::new(0, SubControlIndex::None, 0x11);
        movement.add_mapping(
            ControlChord::new(w, 0, activation::CLICK),
            movement_action::MOVE_FORWARD,
        );
        m.keymap.sections.push(movement);

        let mut ui = InputMap::new(InputMapId(3));
        let lbutton = ControlCode::new(1, SubControlIndex::None, 0x0C);
        ui.add_mapping(
            ControlChord::new(lbutton, 0, activation::CLICK),
            ActionId(7),
        );
        let wheel_up = ControlCode::new(1, SubControlIndex::PositiveAxis, 8);
        ui.add_mapping(
            ControlChord::new(wheel_up, 0, activation::CLICK),
            ActionId(5),
        );
        m.keymap.sections.push(ui);

        let cb = m.new_callback();
        m.register_input_map(InputMapId(4), priority::GAMEPLAY, cb);
        m.register_input_map(InputMapId(3), priority::LOWEST, cb);
        m
    }

    fn keydown(scan: u32, time: u32) -> Win32Message {
        Win32Message::new(msg::WM_KEYDOWN, 0, (scan << 16 | 1) as isize, time)
    }

    fn keyup(scan: u32, time: u32) -> Win32Message {
        Win32Message::new(msg::WM_KEYUP, 0, (scan << 16 | 0xC000_0001) as isize, time)
    }

    /// ORACLE: the recovered input pipeline §3, §3.1 and §5 — a recorded key-down/key-up pair through the
    /// whole of stages 1 to 4, and the auto-repeat filter in the middle of it.
    #[test]
    fn a_message_stream_produces_the_documented_control_events() {
        let mut m = manager();
        // W down.
        assert!(m.on_window_event(&keydown(0x11, 1000), &no_lead));
        let e = m.take_events();
        assert_eq!(e.len(), 1, "{e:?}");
        assert_eq!(e[0].action, movement_action::MOVE_FORWARD);
        assert!(e[0].start);
        assert!(m.is_action_in_progress(movement_action::MOVE_FORWARD));

        // Windows auto-repeat: bit 30 set, bit 31 clear. Filtered out entirely.
        let repeat = Win32Message::new(msg::WM_KEYDOWN, 0, 0x4011_0001_u32 as isize, 1050);
        assert!(m.on_window_event(&repeat, &no_lead));
        assert!(
            m.take_events().is_empty(),
            "Windows auto-repeat must not reach the action layer"
        );

        // W up.
        assert!(m.on_window_event(&keyup(0x11, 1100), &no_lead));
        let e = m.take_events();
        assert_eq!(e.len(), 1, "{e:?}");
        assert!(!e[0].start);
        assert!(!m.is_action_in_progress(movement_action::MOVE_FORWARD));
    }

    /// ORACLE: the recovered binding behavior §2 and trap 9 — `DIK_RSHIFT` (0x36) is rewritten to
    /// `DIK_LSHIFT` (0x2A) before the map sees it, so right shift sets the Shift meta bit too.
    #[test]
    fn right_shift_is_left_shift() {
        let mut m = manager();
        m.on_window_event(&keydown(0x36, 10), &no_lead);
        assert_eq!(
            m.meta_key_mode, 0x8000_0000,
            "right shift must set the Shift bit"
        );
        m.on_window_event(&keyup(0x36, 20), &no_lead);
        assert_eq!(m.meta_key_mode, 0);
    }

    /// ORACLE: the recovered input pipeline §3.2 and §3.3 — a button down/up pair moves the capture
    /// counter, and one `WM_MOUSEWHEEL` is exactly one click whatever the delta.
    #[test]
    fn mouse_buttons_capture_and_the_wheel_clicks_once() {
        let mut m = manager();
        let down = Win32Message::new(msg::WM_LBUTTONDOWN, 0, 0, 100);
        let up = Win32Message::new(msg::WM_LBUTTONUP, 0, 0, 150);
        m.on_window_event(&down, &no_lead);
        assert_eq!(m.mouse.capture_count(), 1);
        assert_eq!(m.take_events().len(), 1);
        m.on_window_event(&up, &no_lead);
        assert_eq!(m.mouse.capture_count(), 0);

        // A big wheel delta is still one press and one release, i.e. one action start and one stop.
        let wheel = Win32Message::new(msg::WM_MOUSEWHEEL, (600_usize) << 16, 0, 200);
        m.take_events();
        m.on_window_event(&wheel, &no_lead);
        let e = m.take_events();
        assert_eq!(e.len(), 2, "one click = a start and a stop: {e:?}");
        assert_eq!(e[0].action, ActionId(5));
        assert!(e[0].start && !e[1].start);
    }

    /// ORACLE: the recovered input pipeline §3 — `WM_MOUSEMOVE` stores the position in client
    /// coordinates and does **not** dispatch, and the coordinates are signed.
    #[test]
    fn mouse_move_stores_a_signed_client_position() {
        let mut m = manager();
        let neg = (0xFFF6_u32 << 16 | 0xFFFB) as isize; // (-5, -10)
        m.on_window_event(&Win32Message::new(msg::WM_MOUSEMOVE, 0, neg, 1), &no_lead);
        assert_eq!(m.mouse_pos(), (-5, -10));
        assert!(m.take_events().is_empty());
    }

    /// Oracle: the chat key's text-focus path, end to end.
    /// Pressing the chat key focuses a text field, the field registers the keyboard barrier and
    /// turns text mode on inside the same `WM_KEYDOWN`, and the `WM_CHAR` that `TranslateMessage`
    /// produces for that key press is dropped. After that, W no longer walks the character but a
    /// mouse click still reaches the world.
    #[test]
    fn the_chat_key_is_not_typed_into_the_field_and_the_barrier_takes_the_keyboard() {
        let mut m = manager();
        // Bind Return to Begin Chat Mode in the chat map, registered above the gameplay maps.
        let mut chat = InputMap::new(InputMapId(0x1000_000A));
        let ret = ControlCode::new(0, SubControlIndex::None, 0x1C);
        chat.add_mapping(
            ControlChord::new(ret, 0, activation::CLICK),
            dereth_client_contract::actions::chat_entry::BEGIN_CHAT_MODE,
        );
        m.keymap.sections.push(chat);
        let cb = m.new_callback();
        m.register_input_map(InputMapId(0x1000_000A), priority::GAMEPLAY, cb);

        // WM_KEYDOWN for Return. fire_input_event resolves Begin Chat Mode ...
        m.on_window_event(&keydown(0x1C, 500), &no_lead);
        let e = m.take_events();
        assert_eq!(e.len(), 1);
        assert_eq!(
            e[0].action,
            dereth_client_contract::actions::chat_entry::BEGIN_CHAT_MODE
        );
        assert_eq!(e[0].toggle, ToggleType::OneShot);

        // .. and the handler runs inside the dispatch, focusing the field:
        // it takes the keyboard barrier and turns text mode on.
        //
        // The fixture does not hand-write the text-mode bracket (`begin_key_down`,
        // `begin_dispatch`, `end_dispatch`, `end_key_down`): a fixture that did would be the
        // producer rather than simulate it, and would pass even where the ignore-next-char flag
        // cannot be armed in a running client. It drives the production bracket
        // [`InputManager::begin_action_dispatch`] / [`InputManager::end_action_dispatch`] — the
        // same two calls `dereth_client::ui::UiShell::route_input` makes around every event — and the
        // key-down context comes off the event the manager itself produced rather than being
        // asserted into place.
        assert!(
            e[0].from_key_down,
            "the event was produced inside a WM_KEYDOWN, and the action broadcast reads exactly \
             that: {:?}",
            e[0]
        );
        m.begin_action_dispatch(e[0].from_key_down);
        let field = m.new_callback();
        m.register_input_map(MAP_BLOCK_KEYBOARD, priority::FOCUSED_UI, field);
        m.set_text_mode(true);
        m.end_action_dispatch();
        assert!(m.text.ignore_next_char);
        assert!(
            !m.text.processing_key_down && !m.text.processing_action_in_response_to_key_down,
            "the bracket leaves no key-down context behind"
        );

        // TranslateMessage's WM_CHAR for that same press: dropped.
        let ch = Win32Message::new(msg::WM_CHAR, usize::from(b'\r'), 0, 500);
        m.on_window_event(&ch, &no_lead);
        assert!(m.take_characters().is_empty(), "no stray character");

        // The next character is typed normally.
        let ch = Win32Message::new(msg::WM_CHAR, usize::from(b'h'), 0, 520);
        m.on_window_event(&ch, &no_lead);
        assert_eq!(m.take_characters(), vec!['h']);

        // W no longer walks the character ...
        m.on_window_event(&keydown(0x11, 600), &no_lead);
        assert!(
            m.take_events().is_empty(),
            "the barrier stops keyboard input"
        );
        // .. but a mouse click still reaches the world, which is the point of barrier 1.
        m.on_window_event(&Win32Message::new(msg::WM_LBUTTONDOWN, 0, 0, 610), &no_lead);
        let e = m.take_events();
        assert_eq!(e.len(), 1);
        assert_eq!(e[0].action, ActionId(7));
    }

    /// ORACLE: the recovered input pipeline §3 — focus loss releases every held control, so no action
    /// survives an Alt+Tab.
    #[test]
    fn losing_focus_releases_every_held_action() {
        let mut m = manager();
        m.on_window_event(&keydown(0x11, 10), &no_lead);
        m.take_events();
        assert!(m.is_action_in_progress(movement_action::MOVE_FORWARD));

        m.on_window_event(&Win32Message::new(msg::WM_KILLFOCUS, 0, 0, 20), &no_lead);
        let e = m.take_events();
        assert_eq!(e.len(), 1);
        assert!(!e[0].start);
        assert!(!m.is_action_in_progress(movement_action::MOVE_FORWARD));
        assert!(!m.has_focus);
    }

    /// ORACLE: the recovered input pipeline §2, trap 14 and trap 15 — a `WM_SYSKEY*` still reaches the
    /// input manager (so a keymap can bind Alt), the screensaver is swallowed before it, and the
    /// standby query is answered rather than forwarded.
    #[test]
    fn syskeys_reach_the_manager_but_the_power_messages_do_not() {
        let mut m = manager();
        let alt = Win32Message::new(msg::WM_SYSKEYDOWN, 0x12, 0x0038_0001, 10);
        assert!(m.on_window_event(&alt, &no_lead));
        let saver = Win32Message::new(msg::WM_SYSCOMMAND, msg::SC_SCREENSAVE, 0, 20);
        assert!(!m.on_window_event(&saver, &no_lead));
        let standby = Win32Message::new(msg::WM_POWERBROADCAST, msg::PBT_APMQUERYSUSPEND, 0, 30);
        assert!(!m.on_window_event(&standby, &no_lead));
    }

    /// ORACLE: the recovered input pipeline §4 step 4 — a second press inside the double-click time and
    /// inside the double-click rectangle carries `NearbyDown`, so a `MouseDblClick` binding wins
    /// over the `Click` one on the same button.
    #[test]
    fn a_second_click_in_time_and_in_place_is_a_double_click() {
        let mut m = manager();
        let lbutton = ControlCode::new(1, SubControlIndex::None, 0x0C);
        if let Some(ui) = m.keymap.section_mut(InputMapId(3)) {
            ui.add_mapping(
                ControlChord::new(lbutton, 0, activation::MOUSE_DBL_CLICK),
                ActionId(10),
            );
        }
        let down = |t| Win32Message::new(msg::WM_LBUTTONDOWN, 0, 0, t);
        let up = |t| Win32Message::new(msg::WM_LBUTTONUP, 0, 0, t);

        m.on_window_event(&down(1000), &no_lead);
        m.on_window_event(&up(1050), &no_lead);
        m.take_events();
        // The second press, 100 ms later and with the pointer unmoved.
        m.on_window_event(&down(1100), &no_lead);
        let e = m.take_events();
        assert!(
            e.iter().any(|x| x.action == ActionId(10)),
            "MouseDblClick (0x60) must beat Click (0x03): {e:?}"
        );

        // Far apart in time: just a click again.
        m.take_events();
        m.on_window_event(&up(1150), &no_lead);
        m.take_events();
        m.on_window_event(&down(5000), &no_lead);
        let e = m.take_events();
        assert!(e.iter().all(|x| x.action != ActionId(10)), "{e:?}");
    }
}
