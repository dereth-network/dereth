//! The Win32 stage: the `WindowEvent` seam, the window procedure's decisions that matter
//! to the input manager, and the input manager's own message handler.
//!
//! This covers the client's window procedure, the forward to the input manager, the keyboard,
//! mouse-button and mouse-wheel event generators, and the character update.
//!
//! # Where the seam is
//!
//! The window and the pump belong to the renderer and the client; `dereth_render::window_proc` already
//! implements the window-message table and `dereth-client`'s pump already carries
//! `GetMessageTime()` alongside every message. This module owns the *other* side: the `MSG`
//! quadruple in, control events out. It does not depend on either crate — the seam is the
//! [`Win32Message`] quadruple, which is plain data.
//!
//! The three `WndProc` decisions that change what the input manager sees are reproduced here as
//! [`wnd_proc_disposition`], because the acceptance test asserts them:
//! the system-keys-enabled flag being false and swallowing every unhandled `WM_SYSKEY*`, the power-broadcast
//! refusal, and `SetFocus` on any mouse-down.

/// The Win32 message ids the pipeline cares about.
pub mod msg {
    pub const WM_SETFOCUS: u32 = 0x0007;
    pub const WM_KILLFOCUS: u32 = 0x0008;
    pub const WM_CANCELMODE: u32 = 0x001F;
    pub const WM_KEYDOWN: u32 = 0x0100;
    pub const WM_KEYUP: u32 = 0x0101;
    pub const WM_CHAR: u32 = 0x0102;
    pub const WM_SYSKEYDOWN: u32 = 0x0104;
    pub const WM_SYSKEYUP: u32 = 0x0105;
    pub const WM_SYSCHAR: u32 = 0x0106;
    pub const WM_SYSCOMMAND: u32 = 0x0112;
    pub const WM_MOUSEMOVE: u32 = 0x0200;
    pub const WM_LBUTTONDOWN: u32 = 0x0201;
    pub const WM_LBUTTONUP: u32 = 0x0202;
    pub const WM_LBUTTONDBLCLK: u32 = 0x0203;
    pub const WM_RBUTTONDOWN: u32 = 0x0204;
    pub const WM_RBUTTONUP: u32 = 0x0205;
    pub const WM_RBUTTONDBLCLK: u32 = 0x0206;
    pub const WM_MBUTTONDOWN: u32 = 0x0207;
    pub const WM_MBUTTONUP: u32 = 0x0208;
    pub const WM_MBUTTONDBLCLK: u32 = 0x0209;
    pub const WM_MOUSEWHEEL: u32 = 0x020A;
    pub const WM_XBUTTONDOWN: u32 = 0x020B;
    pub const WM_XBUTTONUP: u32 = 0x020C;
    pub const WM_XBUTTONDBLCLK: u32 = 0x020D;
    pub const WM_POWERBROADCAST: u32 = 0x0218;
    pub const WM_DEVICECHANGE: u32 = 0x0219;
    pub const WM_MOUSELEAVE: u32 = 0x02A3;

    pub const SC_SCREENSAVE: usize = 0xF140;
    pub const SC_MONITORPOWER: usize = 0xF170;
    pub const PBT_APMQUERYSUSPEND: usize = 0x0000;
    pub const DBT_DEVNODES_CHANGED: usize = 0x0007;
    /// The value `WM_POWERBROADCAST`/`PBT_APMQUERYSUSPEND` returns to refuse standby.
    pub const BROADCAST_QUERY_DENY: isize = 0x424D_5144;
}

/// The `MSG` builds: the four `WndProc` parameters
/// **plus `GetMessageTime()`**.
///
/// `time_ms` **must** be the message-queue timestamp. Tap and double-click windows are
/// `GetDoubleClickTime()` and half of it; substituting `QueryPerformanceCounter` breaks agreement
/// with the user's own OS setting. The host's message pump builds these and hands the same value to
/// the input manager.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Win32Message {
    pub message: u32,
    pub wparam: usize,
    pub lparam: isize,
    /// `GetMessageTime()` -- milliseconds, and it wraps.
    pub time_ms: u32,
}

impl Win32Message {
    #[must_use]
    pub const fn new(message: u32, wparam: usize, lparam: isize, time_ms: u32) -> Self {
        Self {
            message,
            wparam,
            lparam,
            time_ms,
        }
    }
}

/// What the window procedure does with a message before the input manager ever sees it.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum WndProcDisposition {
    /// Reaches the input manager's message handler.
    Forward,
    /// Reaches the input manager's message handler, but `SetFocus(hwnd)` runs first (any mouse *down*).
    ForwardAfterSetFocus,
    /// `WndProc` handles it and the input manager never sees it.
    Swallowed,
    /// Handled with a specific return value — the standby refusal.
    Answered(isize),
    /// Falls through to `DefWindowProcA`.
    Default,
}

/// The system-key-enabled flag starts at 1 but is cleared because `GetMenu(hwnd)` is NULL — the
/// game window has no menu. It is therefore **false for the
/// whole run**, and every unhandled `WM_SYSKEY*` is swallowed: pressing Alt never opens the system
/// menu.
pub const SYS_KEYS_ENABLED: bool = false;

/// The subset of the window procedure that decides what reaches the input manager.
///
/// `dereth_render::window_proc` owns the full table including the window-state effects;
/// this is the same decision reproduced without a dependency on that crate, so this crate's
/// tests can assert it directly.
#[must_use]
pub fn wnd_proc_disposition(m: &Win32Message) -> WndProcDisposition {
    use msg::*;
    match m.message {
        WM_SETFOCUS | WM_KILLFOCUS | WM_CANCELMODE | WM_DEVICECHANGE | WM_MOUSELEAVE
        | WM_MOUSEMOVE | WM_KEYDOWN | WM_KEYUP | WM_CHAR | WM_SYSCHAR => {
            WndProcDisposition::Forward
        }
        // Every down message: SetFocus(hwnd) first, then TrackMouseEvent's arm-once, then
        // Keystone, then ForwardToInputManager.
        WM_LBUTTONDOWN | WM_LBUTTONDBLCLK | WM_RBUTTONDOWN | WM_RBUTTONDBLCLK | WM_MBUTTONDOWN
        | WM_MBUTTONDBLCLK | WM_XBUTTONDOWN | WM_XBUTTONDBLCLK | WM_MOUSEWHEEL => {
            WndProcDisposition::ForwardAfterSetFocus
        }
        WM_LBUTTONUP | WM_RBUTTONUP | WM_MBUTTONUP | WM_XBUTTONUP => WndProcDisposition::Forward,
        // The message is forwarded to the input manager, and *then* the system-keys rule decides whether the
        // message is consumed. Alt+F4 and Alt+Enter are special-cased before it.
        WM_SYSKEYDOWN | WM_SYSKEYUP => WndProcDisposition::Forward,
        WM_SYSCOMMAND => match m.wparam {
            SC_SCREENSAVE | SC_MONITORPOWER => WndProcDisposition::Swallowed,
            _ => WndProcDisposition::Default,
        },
        WM_POWERBROADCAST if m.wparam == PBT_APMQUERYSUSPEND => {
            WndProcDisposition::Answered(BROADCAST_QUERY_DENY)
        }
        _ => WndProcDisposition::Default,
    }
}

/// After forwarding to the input manager, a `WM_SYSKEY*` is consumed iff the system-keys-enabled
/// flag is false — which it always is. `VK_F4` falls through to `DefWindowProc` (Alt+F4 closes)
/// and `VK_RETURN` sets the toggle-full-screen request (Alt+Enter).
#[must_use]
pub const fn syskey_is_consumed(wparam: usize) -> bool {
    const VK_F4: usize = 0x73;
    const VK_RETURN: usize = 0x0D;
    match wparam {
        VK_F4 | VK_RETURN => false,
        _ => !SYS_KEYS_ENABLED,
    }
}

/// One decoded control event, the output of stage 2.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum ControlEvent {
    /// A button press or release for `fire_input_event(cs, ControlType::Button, data, time)`.
    Button {
        offset: u16,
        mouse: bool,
        down: bool,
        time_ms: u32,
    },
    /// The wheel: a *pair* of events, one press and one immediate release, on a virtual button
    /// (`DIMOFS_Z` with a positive or negative sub-control). One `WM_MOUSEWHEEL` is exactly one
    /// click regardless of the delta's magnitude.
    Wheel { positive: bool, time_ms: u32 },
    /// The mouse position from the signed low and high words of `lParam` — stored, **not** dispatched.
    MousePos { x: i32, y: i32 },
    /// A character for. Only delivered while in text mode.
    Character(char),
    /// `WM_SETFOCUS` / `WM_KILLFOCUS`.
    Focus(bool),
    /// `WM_CANCELMODE`.
    CancelMode,
    /// `WM_DEVICECHANGE` with `DBT_DEVNODES_CHANGED` — re-enumerate joysticks.
    DeviceChange,
    /// `WM_MOUSELEAVE`.
    MouseLeave,
}

/// The keyboard event generator's scan-code normalisation.
///
/// Returns `None` when the message is a **Windows auto-repeat down**, which the client discards:
/// `(lParam & 0xC0000000) == 0x40000000`. The repeat you feel is generated internally by
/// Input-manager time-step processing.
#[must_use]
pub fn keyboard_offset(lparam: isize) -> Option<u16> {
    // The message's lParam is a 32-bit bitfield on the only platform this ever ran on; the
    // masks below make every field extraction exact, and the top 32 bits of a 64-bit isize carry
    // nothing the client reads.
    #[allow(clippy::cast_possible_truncation)]
    let l = lparam as usize as u32;
    if (l & 0xC000_0000) == 0x4000_0000 {
        return None;
    }
    // The scan code is bits 16-23 and the extended flag bit 24; both masks make the narrowing
    // exact.
    #[allow(clippy::cast_possible_truncation)]
    let mut scan = ((l >> 16) & 0xFF) as u16;
    let mut ext = (l >> 24) & 1 != 0;
    if scan == 0x45 {
        // NumLock/Pause: the extended flag is inverted.
        ext = !ext;
    } else if scan == 0x36 {
        // DIK_RSHIFT folded onto DIK_LSHIFT: left and right shift are one control as far as the
        // map is concerned.
        scan = 0x2A;
    }
    Some(scan | if ext { 0x80 } else { 0 })
}

/// `DIK_SYSRQ` — PrintScreen, which Windows only reports on the key *up*, so
/// `generate_keyboard_event` synthesises the missing press.
pub const DIK_SYSRQ: u16 = 0xB7;

/// The mouse button number for a message, in the order `generate_mouse_button_event` uses:
/// left, right, middle, XBUTTON1, XBUTTON2. `ofs = button + 0xC`.
#[must_use]
pub fn mouse_button(message: u32, wparam: usize) -> Option<(u16, bool)> {
    use msg::*;
    let (button, up) = match message {
        WM_LBUTTONDOWN | WM_LBUTTONDBLCLK => (0u16, false),
        WM_LBUTTONUP => (0, true),
        WM_RBUTTONDOWN | WM_RBUTTONDBLCLK => (1, false),
        WM_RBUTTONUP => (1, true),
        WM_MBUTTONDOWN | WM_MBUTTONDBLCLK => (2, false),
        WM_MBUTTONUP => (2, true),
        // HIWORD(wParam) is 1 for XBUTTON1 and 2 for XBUTTON2; the client adds 2.
        #[allow(clippy::cast_possible_truncation)]
        WM_XBUTTONDOWN | WM_XBUTTONDBLCLK => ((((wparam >> 16) & 0xFFFF) as u16) + 2, false),
        #[allow(clippy::cast_possible_truncation)]
        WM_XBUTTONUP => ((((wparam >> 16) & 0xFFFF) as u16) + 2, true),
        _ => return None,
    };
    Some((button + 0x0C, up))
}

/// `DBCS` lead-byte pairing: the single pending lead byte.
///
/// `WM_CHAR` carries one *byte* of the ANSI code page. If a lead byte is pending the two bytes go
/// through `MultiByteToWideChar(CP_ACP, MB_PRECOMPOSED, …)`; otherwise `IsDBCSLeadByte` decides
/// whether to stash and wait.
#[derive(Debug, Default)]
pub struct DbcsPairing {
    pending: Option<u8>,
}

impl DbcsPairing {
    /// `is_lead` is the caller's `IsDBCSLeadByte` — a function of the process code page, so it is
    /// injected rather than assumed. On a single-byte code page it is always false and this
    /// degenerates to "every byte is a character", which is the western behaviour.
    pub fn feed(&mut self, byte: u8, is_lead: impl Fn(u8) -> bool) -> Option<char> {
        if let Some(lead) = self.pending.take() {
            // The two-byte sequence; without a real MultiByteToWideChar the pair is carried as its
            // raw code units so a DBCS build can substitute a real converter here.
            let combined = (u32::from(lead) << 8) | u32::from(byte);
            return char::from_u32(combined);
        }
        if is_lead(byte) {
            self.pending = Some(byte);
            return None;
        }
        Some(char::from(byte))
    }

    #[must_use]
    pub const fn has_pending(&self) -> bool {
        self.pending.is_some()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Oracle: the recovered input pipeline §3.1 — the auto-repeat filter, the NumLock inversion and the
    /// RSHIFT fold.
    #[test]
    fn keyboard_offsets_match_the_documented_normalisation() {
        // A plain W press: scan 0x11, no extended bit, no repeat.
        assert_eq!(keyboard_offset(0x0011_0001), Some(0x11));
        // Windows auto-repeat down: bits 30 set, 31 clear.
        assert_eq!(keyboard_offset(0x4011_0001_u32 as isize), None);
        // Right shift folds onto left shift.
        assert_eq!(keyboard_offset(0x0036_0001), Some(0x2A));
        // NumLock (0x45): the extended flag is inverted, so a non-extended message becomes 0xC5.
        assert_eq!(keyboard_offset(0x0045_0001), Some(0xC5));
        assert_eq!(keyboard_offset(0x0145_0001), Some(0x45));
    }

    /// Oracle: the recovered input pipeline §3.2 — `ofs = button + 0xC`, order L, R, M, X1, X2.
    #[test]
    fn mouse_buttons_are_dimofs_button0_upwards() {
        assert_eq!(mouse_button(msg::WM_LBUTTONDOWN, 0), Some((0x0C, false)));
        assert_eq!(mouse_button(msg::WM_RBUTTONUP, 0), Some((0x0D, true)));
        assert_eq!(mouse_button(msg::WM_MBUTTONDOWN, 0), Some((0x0E, false)));
        assert_eq!(
            mouse_button(msg::WM_XBUTTONDOWN, 1 << 16),
            Some((0x0F, false))
        );
        assert_eq!(mouse_button(msg::WM_XBUTTONUP, 2 << 16), Some((0x10, true)));
    }

    /// Oracle: trap 14 and the recovered input pipeline §2 — Alt alone never opens the system menu,
    /// because the system-keys-enabled flag is false for the whole run.
    #[test]
    fn alt_alone_never_opens_the_system_menu() {
        const VK_MENU: usize = 0x12;
        const VK_F4: usize = 0x73;
        const VK_RETURN: usize = 0x0D;
        let m = Win32Message::new(msg::WM_SYSKEYDOWN, VK_MENU, 0, 0);
        assert_eq!(wnd_proc_disposition(&m), WndProcDisposition::Forward);
        assert!(syskey_is_consumed(VK_MENU), "Alt is swallowed");
        assert!(!syskey_is_consumed(VK_F4), "Alt+F4 reaches DefWindowProc");
        assert!(
            !syskey_is_consumed(VK_RETURN),
            "Alt+Enter toggles full screen"
        );
    }

    /// Oracle: trap 15 and the recovered input pipeline §2 — the client refuses standby and swallows the
    /// screensaver.
    #[test]
    fn the_client_refuses_standby() {
        let m = Win32Message::new(msg::WM_POWERBROADCAST, msg::PBT_APMQUERYSUSPEND, 0, 0);
        assert_eq!(
            wnd_proc_disposition(&m),
            WndProcDisposition::Answered(msg::BROADCAST_QUERY_DENY)
        );
        let m = Win32Message::new(msg::WM_SYSCOMMAND, msg::SC_SCREENSAVE, 0, 0);
        assert_eq!(wnd_proc_disposition(&m), WndProcDisposition::Swallowed);
        let m = Win32Message::new(msg::WM_SYSCOMMAND, msg::SC_MONITORPOWER, 0, 0);
        assert_eq!(wnd_proc_disposition(&m), WndProcDisposition::Swallowed);
    }

    /// Oracle: the recovered input pipeline §2 — every mouse *down* calls `SetFocus(hwnd)` first; an up
    /// does not.
    #[test]
    fn a_mouse_down_takes_focus_first() {
        let down = Win32Message::new(msg::WM_LBUTTONDOWN, 0, 0, 0);
        assert_eq!(
            wnd_proc_disposition(&down),
            WndProcDisposition::ForwardAfterSetFocus
        );
        let up = Win32Message::new(msg::WM_LBUTTONUP, 0, 0, 0);
        assert_eq!(wnd_proc_disposition(&up), WndProcDisposition::Forward);
    }

    /// Oracle: the recovered input pipeline §9 — the DBCS lead byte is stashed and the pair delivered.
    #[test]
    fn dbcs_pairs_a_lead_byte() {
        let mut p = DbcsPairing::default();
        assert_eq!(p.feed(b'A', |_| false), Some('A'));
        assert_eq!(p.feed(0x82, |b| b >= 0x81), None);
        assert!(p.has_pending());
        assert!(p.feed(0xA0, |b| b >= 0x81).is_some());
        assert!(!p.has_pending());
    }
}
