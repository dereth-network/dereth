//! Device events translated into the messages consumed by the input manager.

use crate::host::HostEvent;
use crate::keys::{Key, KeyCode, MouseButton};
pub use crate::win32::Win32Message;
use dereth_client_contract::window_proc::msg;

/// What the device half of the mapping has to remember between events: the Alt state and the
/// last pointer position.
#[derive(Debug, Default)]
pub struct DeviceMessages {
    /// Which of `ModifiersChanged`'s bits `WM_SYSKEYDOWN` is decided by: Alt is held.
    pub alt_down: bool,
    /// The last cursor position the host reported, in client pixels.
    ///
    /// Windows packs the cursor position into the `lParam` of **every** mouse message, including
    /// the button ones; `winit`'s `MouseInput` carries no position at all, only `CursorMoved`
    /// does. Kept so a `WM_LBUTTONDOWN` this pump builds says where it happened.
    cursor: (f64, f64),
}

impl DeviceMessages {
    /// The messages one device event maps to. A lifecycle event maps to none here: those are the
    /// host lifecycle adapter's responsibility.
    pub fn map_device_event(&mut self, event: &HostEvent, time_ms: u32) -> Vec<Win32Message> {
        let m = |message: u32, wparam: usize, lparam: isize| {
            vec![Win32Message::new(message, wparam, lparam, time_ms)]
        };
        match event {
            HostEvent::ModifiersChanged { alt } => {
                self.alt_down = *alt;
                Vec::new()
            }
            HostEvent::KeyboardInput { key, pressed, text } => {
                let mut out = vec![self.key_message_for_key(*key, *pressed, time_ms)];
                // TranslateMessage's WM_CHAR, which the table routes to ForwardToBrowser and then to the
                // input manager. The host hands the text over already composed.
                out.extend(key_text_messages(
                    *pressed,
                    self.alt_down,
                    text.as_deref(),
                    time_ms,
                ));
                out
            }
            HostEvent::CursorMoved { x, y } => {
                vec![self.mouse_move_message(*x, *y, time_ms)]
            }
            HostEvent::CursorLeft => m(msg::WM_MOUSELEAVE, 0, 0),
            HostEvent::MouseInput { button, pressed } => self
                .button_message(*button, *pressed, time_ms)
                .into_iter()
                .collect(),
            HostEvent::MouseWheel { notches } => {
                // WHEEL_DELTA is 120 and rides in the high word of WPARAM.
                let ticks = dereth_primitives::num::to_i32(notches * 120.0);
                #[allow(clippy::cast_sign_loss)]
                // LINT-OK: the 16-bit field store that follows the conversion above.
                let wparam = ((ticks as u32) << 16) as usize;
                m(msg::WM_MOUSEWHEEL, wparam, 0)
            }
            HostEvent::CloseRequested
            | HostEvent::Destroyed
            | HostEvent::Focused(_)
            | HostEvent::Resized { .. }
            | HostEvent::ScaleFactorChanged => Vec::new(),
        }
    }

    /// The `WM_MOUSEMOVE` a `CursorMoved` maps to, and the position it leaves behind for
    /// the button messages.
    ///
    /// The mouse-message row: the mouse position is set to the signed low and high words of
    /// `lParam` — client coordinates, **stored and not dispatched**; the input poll compares the
    /// stored value with the previous frame's and only then calls the mouse-move handler.
    pub fn mouse_move_message(&mut self, x: f64, y: f64, time_ms: u32) -> Win32Message {
        self.cursor = (x, y);
        Win32Message::new(msg::WM_MOUSEMOVE, 0, make_lparam(x, y), time_ms)
    }

    /// The `WM_*BUTTON*` for one button transition, at the last position
    /// [`Self::mouse_move_message`] saw; `None` for a button the `0x201`-`0x20D` block does not
    /// name — which is what falling through to `DefWindowProcA` means.
    pub fn button_message(
        &mut self,
        button: MouseButton,
        pressed: bool,
        time_ms: u32,
    ) -> Option<Win32Message> {
        let message = mouse_message(button, pressed)?;
        let (x, y) = self.cursor;
        Some(Win32Message::new(message, 0, make_lparam(x, y), time_ms))
    }

    /// [`Self::key_message`] over the plain [`Key`] the host reports — the virtual key and the
    /// scan code already resolved.
    pub fn key_message_for_key(&mut self, key: Key, pressed: bool, time_ms: u32) -> Win32Message {
        self.key_message(pressed, key.virtual_key, key.scan_code, time_ms)
    }

    /// Build the keyboard message for one key transition, choosing between the `WM_KEY*` and
    /// `WM_SYSKEY*` families the way Windows does: the system family is used while Alt is held.
    ///
    /// **`lParam` carries the scan code, and that is not optional.** The window procedure forwards
    /// the raw `MSG`, and keyboard-event generation reads the **`lParam`**: bits 16–23
    /// are the DirectInput scan code, bit 24 is the extended flag, and bits 30–31 mark the Windows
    /// auto-repeat the client discards. Scan code 0 is not a key, so a zero `lParam` would miss
    /// every binding.
    ///
    /// `scan` is the 16-bit PS/2 set-1 scan code, `0xE0xx` for an extended key, which is exactly
    /// what [`scan_code_from_key_code`] yields on every platform.
    pub fn key_message(
        &mut self,
        pressed: bool,
        vk: usize,
        scan: u16,
        time_ms: u32,
    ) -> Win32Message {
        let message = match (self.alt_down, pressed) {
            (true, true) => msg::WM_SYSKEYDOWN,
            (true, false) => msg::WM_SYSKEYUP,
            (false, true) => msg::WM_KEYDOWN,
            (false, false) => msg::WM_KEYUP,
        };
        Win32Message::new(
            message,
            vk,
            key_lparam(scan, self.alt_down, pressed),
            time_ms,
        )
    }
}

/// The `WM_CHAR`/`WM_SYSCHAR` messages `TranslateMessage` produces for a key event.
///
/// The caller supplies the host's text translated with **every** modifier applied, Control
/// included: text with Control removed would turn Ctrl+V into a paste followed by a printable `v`.
#[must_use]
pub fn key_text_messages(
    pressed: bool,
    alt_down: bool,
    text_with_all_modifiers: Option<&str>,
    time_ms: u32,
) -> Vec<Win32Message> {
    if !pressed {
        return Vec::new();
    }
    // `KeyEvent::text` intentionally removes Control before it translates the key. That is useful
    // for shortcut lookup and wrong for `TranslateMessage`: Ctrl+V would become PasteText followed
    // by a printable `v`. The supplement is the host's actual translated text (`U+0016` for
    // Ctrl+V), which retail forwards and the character handler rejects because it is below U+0020.
    let message = if alt_down {
        msg::WM_SYSCHAR
    } else {
        msg::WM_CHAR
    };
    text_with_all_modifiers.map_or_else(Vec::new, |text| {
        text.chars()
            .map(|ch| Win32Message::new(message, ch as usize, 0, time_ms))
            .collect()
    })
}

/// The `lParam` of a `WM_KEYDOWN` / `WM_KEYUP` / `WM_SYSKEY*`, as `winuser.h` defines its fields.
///
/// | bits | field | value here |
/// |---|---|---|
/// | 0–15 | repeat count | **1** — `winit` reports one event per transition, and the input manager
///   generates its own repeat during its time update, so a Windows repeat is
///   something this pump must never claim |
/// | 16–23 | scan code | the low byte of the platform scan code |
/// | 24 | extended | set for the `0xE0` block: the arrow cluster, right Ctrl/Alt, numpad Enter and
///   divide. `generate_keyboard_event` folds it back in as DirectInput's `| 0x80` |
/// | 29 | context | Alt is held, which is what distinguishes `WM_SYSKEY*` from `WM_KEY*` |
/// | 30 | previous state | 1 on a release, 0 on a first press |
/// | 31 | transition | 1 on a release |
///
/// Bits 30 and 31 matter: `keyboard_offset` rejects a message whose top two bits are `01`, which is
/// a Windows auto-repeat down, and a release is `11`, which it accepts.
#[must_use]
pub fn key_lparam(scan: u16, alt_down: bool, pressed: bool) -> isize {
    let extended = u32::from(scan & 0xE000 == 0xE000);
    let mut l: u32 = 1;
    l |= (u32::from(scan) & 0xFF) << 16;
    l |= extended << 24;
    l |= u32::from(alt_down) << 29;
    if !pressed {
        l |= 0b11 << 30;
    }
    lparam_bits(l)
}

/// A 32-bit `lParam` field as the message carries it: zero-extended where a pointer is 64 bits,
/// and the same 32 bits where it is 32 (WebAssembly), where a release's top bit makes it negative.
/// Every reader masks the bits back out, so both read the same.
#[must_use]
pub fn lparam_bits(l: u32) -> isize {
    usize::try_from(l).map_or(0, usize::cast_signed)
}

/// `MAKELPARAM(x, y)` for a mouse message, in client coordinates.
pub fn make_lparam(x: f64, y: f64) -> isize {
    let xi = dereth_primitives::num::to_i32_f64(x);
    let yi = dereth_primitives::num::to_i32_f64(y);
    #[allow(clippy::cast_sign_loss)]
    // LINT-OK: the two 16-bit field stores that follow the conversions above.
    let packed = ((yi as u32 & 0xFFFF) << 16) | (xi as u32 & 0xFFFF);
    lparam_bits(packed)
}

/// The `0x201`–`0x20D` block of the message table. X buttons are not distinguished here because the
/// table does not distinguish them either.
pub fn mouse_message(button: MouseButton, pressed: bool) -> Option<u32> {
    Some(match (button, pressed) {
        (MouseButton::Left, true) => msg::WM_LBUTTONDOWN,
        (MouseButton::Left, false) => msg::WM_LBUTTONUP,
        (MouseButton::Right, true) => msg::WM_RBUTTONDOWN,
        (MouseButton::Right, false) => msg::WM_RBUTTONUP,
        (MouseButton::Middle, true) => msg::WM_MBUTTONDOWN,
        (MouseButton::Middle, false) => msg::WM_MBUTTONUP,
        (MouseButton::Back | MouseButton::Forward, true) => msg::WM_XBUTTONDOWN,
        (MouseButton::Back | MouseButton::Forward, false) => msg::WM_XBUTTONUP,
        (MouseButton::Other(_), _) => return None,
    })
}

/// `winit`'s physical key code to the platform scan code the `MSG`'s `lParam` carries.
///
/// **This is a PS/2 set-1 table and it is deliberately not the platform's.** The client's key
/// identity downstream of the pump is the DirectInput scan code the shipped key maps are written
/// in (`dereth_input::names`'s keyboard table), and `DIK_*` *is* set 1:
/// `generate_keyboard_event` reaches it from a `MSG` `lParam` by taking the low byte and
/// folding the extended bit back in as `| 0x80`, which [`crate::win32::keyboard_offset`]
/// does. A set-1 code in `lParam` is therefore the only input from which that arithmetic yields the offset
/// the key maps name.
///
/// It does not delegate to `winit`'s `PhysicalKeyExtScancode::to_scancode`, because
/// `to_scancode` is not one table: it is set 1 only on Windows. On Linux it answers the **evdev**
/// keycode and on macOS the **Apple virtual keycode**, and neither is set 1. Against `winit`
/// 0.29.15, the Windows and Linux tables agree on 85 keys and **disagree on 46** -- the whole
/// `0xE0` block among them, so Linux would send `DIK_DELETE` as `111`, `DIK_NEXT` as `109` and
/// `DIK_END` as `107`. Those are not `DIK_*` values at all: the shipped COMBAT band would do
/// nothing, and rebinding a key to itself would appear to work while the Configure Keyboard box
/// went blank, because the capture and the press agree with each other and with no name in the
/// table. macOS is worse still -- `KeyA` is `0x00` there.
///
/// Owning the table also avoids two further Linux faults: evdev `NumLock` is `69` = `0x45`, which `keyboard_offset`'s NumLock/Pause inversion
/// turns into `DIK_PAUSE`; and evdev `F13`..`F24` are `183`..`194`, which collide with
/// `DIK_SYSRQ` and `DIK_RMENU`.
///
/// Values are `winit`'s Windows table, and the test below holds them to it on Windows by walking
/// every scan code that backend recognises -- so this stays a transcription with an oracle rather
/// than a second opinion. Returns the 16-bit extended form, `0xE0xx` for the extended block.
///
/// The one place `winit`'s Windows table is layout-dependent is `Lang1`/`Lang2`, which answer
/// `0xE0F2`/`0xE0F1` under a Korean keyboard layout and `0x0072`/`0x0071` otherwise. The
/// non-Korean value is taken here and the test skips the pair, for the same reason the choice does
/// not matter: the key maps are written in `DIK_*` and `DIK_*` names neither code.
#[must_use]
#[allow(clippy::too_many_lines)]
pub fn scan_code_from_key_code(code: KeyCode) -> Option<u16> {
    use KeyCode as K;
    Some(match code {
        // The main block. evdev numbers these the same, which is why letters, digits and the
        // function row always worked on Linux and nothing else did.
        K::Escape => 0x0001,
        K::Digit1 => 0x0002,
        K::Digit2 => 0x0003,
        K::Digit3 => 0x0004,
        K::Digit4 => 0x0005,
        K::Digit5 => 0x0006,
        K::Digit6 => 0x0007,
        K::Digit7 => 0x0008,
        K::Digit8 => 0x0009,
        K::Digit9 => 0x000A,
        K::Digit0 => 0x000B,
        K::Minus => 0x000C,
        K::Equal => 0x000D,
        K::Backspace => 0x000E,
        K::Tab => 0x000F,
        K::KeyQ => 0x0010,
        K::KeyW => 0x0011,
        K::KeyE => 0x0012,
        K::KeyR => 0x0013,
        K::KeyT => 0x0014,
        K::KeyY => 0x0015,
        K::KeyU => 0x0016,
        K::KeyI => 0x0017,
        K::KeyO => 0x0018,
        K::KeyP => 0x0019,
        K::BracketLeft => 0x001A,
        K::BracketRight => 0x001B,
        K::Enter => 0x001C,
        K::ControlLeft => 0x001D,
        K::KeyA => 0x001E,
        K::KeyS => 0x001F,
        K::KeyD => 0x0020,
        K::KeyF => 0x0021,
        K::KeyG => 0x0022,
        K::KeyH => 0x0023,
        K::KeyJ => 0x0024,
        K::KeyK => 0x0025,
        K::KeyL => 0x0026,
        K::Semicolon => 0x0027,
        K::Quote => 0x0028,
        K::Backquote => 0x0029,
        K::ShiftLeft => 0x002A,
        K::Backslash => 0x002B,
        K::KeyZ => 0x002C,
        K::KeyX => 0x002D,
        K::KeyC => 0x002E,
        K::KeyV => 0x002F,
        K::KeyB => 0x0030,
        K::KeyN => 0x0031,
        K::KeyM => 0x0032,
        K::Comma => 0x0033,
        K::Period => 0x0034,
        K::Slash => 0x0035,
        // `DIK_RSHIFT`, which `keyboard_offset` folds onto `DIK_LSHIFT`.
        K::ShiftRight => 0x0036,
        K::NumpadMultiply => 0x0037,
        K::AltLeft => 0x0038,
        K::Space => 0x0039,
        K::CapsLock => 0x003A,
        K::F1 => 0x003B,
        K::F2 => 0x003C,
        K::F3 => 0x003D,
        K::F4 => 0x003E,
        K::F5 => 0x003F,
        K::F6 => 0x0040,
        K::F7 => 0x0041,
        K::F8 => 0x0042,
        K::F9 => 0x0043,
        K::F10 => 0x0044,
        // `DIK_PAUSE`, and it is *not* extended: `keyboard_offset` inverts the flag for `0x45`, so
        // this reads as `0xC5` and the extended `0xE045` below reads as `DIK_NUMLOCK` `0x45`.
        K::Pause => 0x0045,
        K::ScrollLock => 0x0046,
        K::Numpad7 => 0x0047,
        K::Numpad8 => 0x0048,
        K::Numpad9 => 0x0049,
        K::NumpadSubtract => 0x004A,
        K::Numpad4 => 0x004B,
        K::Numpad5 => 0x004C,
        K::Numpad6 => 0x004D,
        K::NumpadAdd => 0x004E,
        K::Numpad1 => 0x004F,
        K::Numpad2 => 0x0050,
        K::Numpad3 => 0x0051,
        K::Numpad0 => 0x0052,
        K::NumpadDecimal => 0x0053,
        K::IntlBackslash => 0x0056,
        K::F11 => 0x0057,
        K::F12 => 0x0058,
        K::NumpadEqual => 0x0059,
        K::F13 => 0x0064,
        K::F14 => 0x0065,
        K::F15 => 0x0066,
        K::F16 => 0x0067,
        K::F17 => 0x0068,
        K::F18 => 0x0069,
        K::F19 => 0x006A,
        K::F20 => 0x006B,
        K::F21 => 0x006C,
        K::F22 => 0x006D,
        K::F23 => 0x006E,
        // `DIK_KANA`, the two Korean-ambiguous codes, `DIK_ABNT_C1`, `DIK_CONVERT`,
        // `DIK_NOCONVERT`, `DIK_YEN`, `DIK_ABNT_C2`.
        K::KanaMode => 0x0070,
        K::Lang2 => 0x0071,
        K::Lang1 => 0x0072,
        K::IntlRo => 0x0073,
        K::F24 => 0x0076,
        K::Convert => 0x0079,
        K::NonConvert => 0x007B,
        K::IntlYen => 0x007D,
        K::NumpadComma => 0x007E,
        // The `0xE0` block -- every key evdev renumbers, and where the shipped COMBAT band lives.
        K::MediaTrackPrevious => 0xE010,
        K::MediaTrackNext => 0xE019,
        K::NumpadEnter => 0xE01C,
        K::ControlRight => 0xE01D,
        K::AudioVolumeMute => 0xE020,
        K::LaunchApp2 => 0xE021,
        K::MediaPlayPause => 0xE022,
        K::MediaStop => 0xE024,
        K::AudioVolumeDown => 0xE02E,
        K::AudioVolumeUp => 0xE030,
        K::BrowserHome => 0xE032,
        K::NumpadDivide => 0xE035,
        // `DIK_SYSRQ`, which Windows only reports on the key up; see the host's lifecycle pump.
        K::PrintScreen => 0xE037,
        K::AltRight => 0xE038,
        K::NumLock => 0xE045,
        K::Home => 0xE047,
        K::ArrowUp => 0xE048,
        K::PageUp => 0xE049,
        K::ArrowLeft => 0xE04B,
        K::ArrowRight => 0xE04D,
        K::End => 0xE04F,
        K::ArrowDown => 0xE050,
        K::PageDown => 0xE051,
        K::Insert => 0xE052,
        K::Delete => 0xE053,
        K::SuperLeft => 0xE05B,
        K::SuperRight => 0xE05C,
        K::ContextMenu => 0xE05D,
        K::Power => 0xE05E,
        K::BrowserSearch => 0xE065,
        K::BrowserFavorites => 0xE066,
        K::BrowserRefresh => 0xE067,
        K::BrowserStop => 0xE068,
        K::BrowserForward => 0xE069,
        K::BrowserBack => 0xE06A,
        K::LaunchApp1 => 0xE06B,
        K::LaunchMail => 0xE06C,
        K::MediaSelect => 0xE06D,
        _ => return None,
    })
}

/// `winit`'s physical key code to the Win32 virtual key the `MSG`'s `wParam` carries.
///
/// This belongs to the pump rather than to `dereth-input`: a `MSG` is not a `MSG` without its
/// virtual key, and the input manager consumes the `MSG`. What `dereth-input` owns is everything
/// downstream — the
/// `DIK_*` scan codes the saved key maps are written in, the action bindings, and the tap and
/// double-click timing.
///
/// Values are Windows' `VK_*` constants (`winuser.h`).
#[must_use]
#[allow(clippy::too_many_lines)]
pub fn vk_from_key_code(code: KeyCode) -> Option<usize> {
    use KeyCode as K;
    Some(match code {
        K::Backspace => 0x08,
        K::Tab => 0x09,
        K::Enter | K::NumpadEnter => 0x0D,
        K::Pause => 0x13,
        K::CapsLock => 0x14,
        K::Escape => 0x1B,
        K::Space => 0x20,
        K::PageUp => 0x21,
        K::PageDown => 0x22,
        K::End => 0x23,
        K::Home => 0x24,
        K::ArrowLeft => 0x25,
        K::ArrowUp => 0x26,
        K::ArrowRight => 0x27,
        K::ArrowDown => 0x28,
        K::PrintScreen => 0x2C,
        K::Insert => 0x2D,
        K::Delete => 0x2E,
        K::Digit0 => 0x30,
        K::Digit1 => 0x31,
        K::Digit2 => 0x32,
        K::Digit3 => 0x33,
        K::Digit4 => 0x34,
        K::Digit5 => 0x35,
        K::Digit6 => 0x36,
        K::Digit7 => 0x37,
        K::Digit8 => 0x38,
        K::Digit9 => 0x39,
        K::KeyA => 0x41,
        K::KeyB => 0x42,
        K::KeyC => 0x43,
        K::KeyD => 0x44,
        K::KeyE => 0x45,
        K::KeyF => 0x46,
        K::KeyG => 0x47,
        K::KeyH => 0x48,
        K::KeyI => 0x49,
        K::KeyJ => 0x4A,
        K::KeyK => 0x4B,
        K::KeyL => 0x4C,
        K::KeyM => 0x4D,
        K::KeyN => 0x4E,
        K::KeyO => 0x4F,
        K::KeyP => 0x50,
        K::KeyQ => 0x51,
        K::KeyR => 0x52,
        K::KeyS => 0x53,
        K::KeyT => 0x54,
        K::KeyU => 0x55,
        K::KeyV => 0x56,
        K::KeyW => 0x57,
        K::KeyX => 0x58,
        K::KeyY => 0x59,
        K::KeyZ => 0x5A,
        K::SuperLeft => 0x5B,
        K::SuperRight => 0x5C,
        K::ContextMenu => 0x5D,
        K::Numpad0 => 0x60,
        K::Numpad1 => 0x61,
        K::Numpad2 => 0x62,
        K::Numpad3 => 0x63,
        K::Numpad4 => 0x64,
        K::Numpad5 => 0x65,
        K::Numpad6 => 0x66,
        K::Numpad7 => 0x67,
        K::Numpad8 => 0x68,
        K::Numpad9 => 0x69,
        K::NumpadMultiply => 0x6A,
        K::NumpadAdd => 0x6B,
        K::NumpadSubtract => 0x6D,
        K::NumpadDecimal => 0x6E,
        K::NumpadDivide => 0x6F,
        K::F1 => 0x70,
        K::F2 => 0x71,
        K::F3 => 0x72,
        K::F4 => 0x73,
        K::F5 => 0x74,
        K::F6 => 0x75,
        K::F7 => 0x76,
        K::F8 => 0x77,
        K::F9 => 0x78,
        K::F10 => 0x79,
        K::F11 => 0x7A,
        K::F12 => 0x7B,
        K::NumLock => 0x90,
        K::ScrollLock => 0x91,
        K::ShiftLeft => 0xA0,
        K::ShiftRight => 0xA1,
        K::ControlLeft => 0xA2,
        K::ControlRight => 0xA3,
        K::AltLeft => 0xA4,
        K::AltRight => 0xA5,
        K::Semicolon => 0xBA,
        K::Equal => 0xBB,
        K::Comma => 0xBC,
        K::Minus => 0xBD,
        K::Period => 0xBE,
        K::Slash => 0xBF,
        K::Backquote => 0xC0,
        K::BracketLeft => 0xDB,
        K::Backslash => 0xDC,
        K::BracketRight => 0xDD,
        K::Quote => 0xDE,
        _ => return None,
    })
}

#[cfg(test)]
mod tests {
    //! Behaviour: none (portable device messages retain the host message fields).
    use super::*;
    #[test]
    fn the_button_mapping_covers_the_0x201_block() {
        assert_eq!(
            mouse_message(MouseButton::Left, true),
            Some(msg::WM_LBUTTONDOWN)
        );
        assert_eq!(
            mouse_message(MouseButton::Left, false),
            Some(msg::WM_LBUTTONUP)
        );
        assert_eq!(
            mouse_message(MouseButton::Right, true),
            Some(msg::WM_RBUTTONDOWN)
        );
        assert_eq!(
            mouse_message(MouseButton::Middle, false),
            Some(msg::WM_MBUTTONUP)
        );
        assert_eq!(
            mouse_message(MouseButton::Back, true),
            Some(msg::WM_XBUTTONDOWN)
        );
        assert_eq!(mouse_message(MouseButton::Other(9), true), None);
    }

    #[test]
    fn a_cursor_position_lands_in_lparam_the_way_makelparam_packs_it() {
        let m = make_lparam(12.9, 34.9);
        // Truncation toward zero, then the two 16-bit fields.
        assert_eq!(m & 0xFFFF, 12);
        assert_eq!((m >> 16) & 0xFFFF, 34);
    }

    #[test]
    fn the_virtual_key_table_covers_what_the_message_table_distinguishes() {
        assert_eq!(vk_from_key_code(KeyCode::F4), Some(msg::VK_F4));
        assert_eq!(vk_from_key_code(KeyCode::Enter), Some(msg::VK_RETURN));
        assert_eq!(vk_from_key_code(KeyCode::Escape), Some(0x1B));
        assert_eq!(vk_from_key_code(KeyCode::KeyA), Some(0x41));
        assert_eq!(vk_from_key_code(KeyCode::Digit0), Some(0x30));
    }

    /// **The Linux keybind fault.** The client's key identity is the `DIK_*` offset, and the whole
    /// point of owning the scan-code table is that a `KeyCode` reaches the same offset on every
    /// platform. This walks the real path -- table, `lParam` pack, `keyboard_offset` -- for the
    /// keys the shipped COMBAT band binds plus the rest of the `0xE0` block, and it runs
    /// everywhere, so the Linux and macOS builds assert it too.
    ///
    /// Before this unit, `winit`'s Linux table answered `111` for `Delete`, `109` for `PageDown`
    /// and `107` for `End`; none is a `DIK_*` code, so every one of these lines was red there
    /// while passing on Windows.
    #[test]
    fn the_keys_the_shipped_combat_band_binds_reach_their_dik_offsets() {
        use crate::win32::keyboard_offset;

        // (key, DIK offset, the `DIK_*` name `crate::names` gives that offset)
        let cases = [
            (KeyCode::Delete, 0xD3_u16, "DIK_DELETE"),
            (KeyCode::PageDown, 0xD1, "DIK_NEXT / DIK_PGDN"),
            (KeyCode::End, 0xCF, "DIK_END"),
            (KeyCode::Home, 0xC7, "DIK_HOME"),
            (KeyCode::PageUp, 0xC9, "DIK_PRIOR / DIK_PGUP"),
            (KeyCode::Insert, 0xD2, "DIK_INSERT"),
            (KeyCode::ArrowUp, 0xC8, "DIK_UP"),
            (KeyCode::ArrowDown, 0xD0, "DIK_DOWN"),
            (KeyCode::ArrowLeft, 0xCB, "DIK_LEFT"),
            (KeyCode::ArrowRight, 0xCD, "DIK_RIGHT"),
            (KeyCode::ControlRight, 0x9D, "DIK_RCONTROL"),
            (KeyCode::AltRight, 0xB8, "DIK_RMENU"),
            (KeyCode::NumpadEnter, 0x9C, "DIK_NUMPADENTER"),
            (KeyCode::NumpadDivide, 0xB5, "DIK_DIVIDE"),
            (KeyCode::PrintScreen, 0xB7, "DIK_SYSRQ"),
            // The inverted pair. evdev numbers `NumLock` `0x45`, which the inversion would have
            // turned into `DIK_PAUSE` -- so this line pins the fix, not just the arithmetic.
            (KeyCode::NumLock, 0x45, "DIK_NUMLOCK"),
            (KeyCode::Pause, 0xC5, "DIK_PAUSE"),
            // The main block, which was already right everywhere, held so it stays that way.
            (KeyCode::KeyW, 0x11, "DIK_W"),
            (KeyCode::Escape, 0x01, "DIK_ESCAPE"),
            (KeyCode::F12, 0x58, "DIK_F12"),
            // Right shift is folded onto `DIK_LSHIFT` by `keyboard_offset`.
            (KeyCode::ShiftRight, 0x2A, "DIK_LSHIFT (folded)"),
        ];

        for (code, expected, name) in cases {
            let scan = scan_code_from_key_code(code)
                .unwrap_or_else(|| panic!("{code:?} has no scan code"));
            let lparam = key_lparam(scan, false, true);
            assert_eq!(
                keyboard_offset(lparam),
                Some(expected),
                "{code:?} must reach {name} ({expected:#04X}); scan code was {scan:#06X}"
            );
        }
    }

    /// Behaviour: none (message-field packing is a host adapter invariant).
    /// A key's release names the key it releases at any pointer width: the release's top two
    /// bits make the field too big for a 32-bit signed `lParam`, and the field is carried as its
    /// bits rather than lost to a range check (which made every release on WebAssembly name no
    /// key at all, so movement keys stuck down).
    #[test]
    fn a_key_release_names_its_key_at_any_pointer_width() {
        let w = 0x11;
        for pressed in [true, false] {
            let l = key_lparam(w, false, pressed);
            assert_eq!(crate::win32::keyboard_offset(l), Some(0x11));
        }
        let release = 0xC011_0001_u32;
        assert_eq!(
            lparam_bits(release).cast_unsigned() & 0xFFFF_FFFF,
            usize::try_from(release).unwrap()
        );
        assert_eq!(
            key_lparam(w, false, false).cast_unsigned() & 0xFFFF_FFFF,
            usize::try_from(release).unwrap()
        );
    }
    #[test]
    fn text_is_emitted_on_press_and_mouse_buttons_reuse_the_last_position() {
        let mut messages = DeviceMessages::default();
        messages.map_device_event(&HostEvent::ModifiersChanged { alt: true }, 1);
        let press = messages.map_device_event(
            &HostEvent::KeyboardInput {
                key: Key::KEY_W,
                pressed: true,
                text: Some("w".into()),
            },
            2,
        );
        assert_eq!(
            press.iter().map(|m| m.message).collect::<Vec<_>>(),
            [msg::WM_SYSKEYDOWN, msg::WM_SYSCHAR]
        );
        let release = messages.map_device_event(
            &HostEvent::KeyboardInput {
                key: Key::KEY_W,
                pressed: false,
                text: Some("w".into()),
            },
            3,
        );
        assert_eq!(release.len(), 1);
        assert_eq!(release[0].message, msg::WM_SYSKEYUP);
        messages.map_device_event(&HostEvent::CursorMoved { x: -12.0, y: 34.0 }, 4);
        let button = messages.map_device_event(
            &HostEvent::MouseInput {
                button: MouseButton::Left,
                pressed: true,
            },
            5,
        );
        assert_eq!(button[0].lparam, make_lparam(-12.0, 34.0));
        assert!(messages
            .map_device_event(&HostEvent::Focused(false), 6)
            .is_empty());
    }
}
