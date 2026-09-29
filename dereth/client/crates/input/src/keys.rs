//! Physical key identities and portable virtual-key and scan-code tables.

/// One physical key, as the `MSG` the pump builds names it.
///
/// Built by the host from whatever the window system reports; see
/// the host key adapter. The named constants below are the keys the client refers to by name.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct Key {
    /// The Win32 virtual key the `MSG`'s `wParam` carries — `winuser.h`'s `VK_*`.
    pub virtual_key: usize,
    /// The 16-bit PS/2 set-1 scan code the `MSG`'s `lParam` carries, `0xE0xx` for the extended
    /// block. The host owns that table rather than borrowing the window system's, because
    /// `winit`'s `to_scancode` is set 1 only on Windows; see `crate::pump`.
    pub scan_code: u16,
}

impl Key {
    #[must_use]
    pub const fn new(virtual_key: usize, scan_code: u16) -> Self {
        Self {
            virtual_key,
            scan_code,
        }
    }

    /// `VK_ESCAPE` / `DIK_ESCAPE`.
    pub const ESCAPE: Self = Self::new(0x1B, 0x01);
    /// `VK_SPACE` / `DIK_SPACE` — the residual fly-cam's "up".
    pub const SPACE: Self = Self::new(0x20, 0x39);
    /// `VK_C` / `DIK_C` — the residual fly-cam's "down".
    pub const KEY_C: Self = Self::new(0x43, 0x2E);
    /// `VK_W` / `DIK_W`, which the shipped default keymap binds to action 41 `MOVE_FORWARD`.
    pub const KEY_W: Self = Self::new(0x57, 0x11);
    /// `VK_LCONTROL` / `DIK_LCONTROL`.
    pub const CONTROL_LEFT: Self = Self::new(0xA2, 0x1D);
    /// `VK_UP` / `DIK_UP`, one of the extended (`0xE0`) block.
    pub const ARROW_UP: Self = Self::new(0x26, 0xE048);
}

/// A mouse button, as the `0x201`–`0x20D` block of the window procedure's message table names it.
///
/// `Other` is a button the table does not name, which is what falling through to `DefWindowProcA`
/// means.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum MouseButton {
    Left,
    Right,
    Middle,
    /// `XBUTTON1`.
    Back,
    /// `XBUTTON2`.
    Forward,
    Other(u16),
}

/// A physical key supported by the message tables, independent of the window system.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum KeyCode {
    AltLeft,
    AltRight,
    ArrowDown,
    ArrowLeft,
    ArrowRight,
    ArrowUp,
    AudioVolumeDown,
    AudioVolumeMute,
    AudioVolumeUp,
    Backquote,
    Backslash,
    Backspace,
    BracketLeft,
    BracketRight,
    BrowserBack,
    BrowserFavorites,
    BrowserForward,
    BrowserHome,
    BrowserRefresh,
    BrowserSearch,
    BrowserStop,
    CapsLock,
    Comma,
    ContextMenu,
    ControlLeft,
    ControlRight,
    Convert,
    Delete,
    Digit0,
    Digit1,
    Digit2,
    Digit3,
    Digit4,
    Digit5,
    Digit6,
    Digit7,
    Digit8,
    Digit9,
    End,
    Enter,
    Equal,
    Escape,
    F1,
    F10,
    F11,
    F12,
    F13,
    F14,
    F15,
    F16,
    F17,
    F18,
    F19,
    F2,
    F20,
    F21,
    F22,
    F23,
    F24,
    F3,
    F4,
    F5,
    F6,
    F7,
    F8,
    F9,
    Home,
    Insert,
    IntlBackslash,
    IntlRo,
    IntlYen,
    KanaMode,
    KeyA,
    KeyB,
    KeyC,
    KeyD,
    KeyE,
    KeyF,
    KeyG,
    KeyH,
    KeyI,
    KeyJ,
    KeyK,
    KeyL,
    KeyM,
    KeyN,
    KeyO,
    KeyP,
    KeyQ,
    KeyR,
    KeyS,
    KeyT,
    KeyU,
    KeyV,
    KeyW,
    KeyX,
    KeyY,
    KeyZ,
    Lang1,
    Lang2,
    LaunchApp1,
    LaunchApp2,
    LaunchMail,
    MediaPlayPause,
    MediaSelect,
    MediaStop,
    MediaTrackNext,
    MediaTrackPrevious,
    Minus,
    NonConvert,
    NumLock,
    Numpad0,
    Numpad1,
    Numpad2,
    Numpad3,
    Numpad4,
    Numpad5,
    Numpad6,
    Numpad7,
    Numpad8,
    Numpad9,
    NumpadAdd,
    NumpadComma,
    NumpadDecimal,
    NumpadDivide,
    NumpadEnter,
    NumpadEqual,
    NumpadMultiply,
    NumpadSubtract,
    PageDown,
    PageUp,
    Pause,
    Period,
    Power,
    PrintScreen,
    Quote,
    ScrollLock,
    Semicolon,
    ShiftLeft,
    ShiftRight,
    Slash,
    Space,
    SuperLeft,
    SuperRight,
    Tab,
    Unidentified,
}
