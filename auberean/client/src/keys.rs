//! The window system's keys and buttons as the shell's.

use auberean_shell::input::{Button, Key};
use winit::event::MouseButton;
use winit::keyboard::KeyCode;

/// The shell's key for a physical key, if it has one.
#[must_use]
pub fn key(code: KeyCode) -> Option<Key> {
    use KeyCode as C;
    Some(match code {
        C::KeyA => Key::A,
        C::KeyB => Key::B,
        C::KeyC => Key::C,
        C::KeyD => Key::D,
        C::KeyE => Key::E,
        C::KeyF => Key::F,
        C::KeyG => Key::G,
        C::KeyH => Key::H,
        C::KeyI => Key::I,
        C::KeyJ => Key::J,
        C::KeyK => Key::K,
        C::KeyL => Key::L,
        C::KeyM => Key::M,
        C::KeyN => Key::N,
        C::KeyO => Key::O,
        C::KeyP => Key::P,
        C::KeyQ => Key::Q,
        C::KeyR => Key::R,
        C::KeyS => Key::S,
        C::KeyT => Key::T,
        C::KeyU => Key::U,
        C::KeyV => Key::V,
        C::KeyW => Key::W,
        C::KeyX => Key::X,
        C::KeyY => Key::Y,
        C::KeyZ => Key::Z,
        C::Digit0 => Key::Digit0,
        C::Digit1 => Key::Digit1,
        C::Digit2 => Key::Digit2,
        C::Digit3 => Key::Digit3,
        C::Digit4 => Key::Digit4,
        C::Digit5 => Key::Digit5,
        C::Digit6 => Key::Digit6,
        C::Digit7 => Key::Digit7,
        C::Digit8 => Key::Digit8,
        C::Digit9 => Key::Digit9,
        C::F1 => Key::F1,
        C::F2 => Key::F2,
        C::F3 => Key::F3,
        C::F4 => Key::F4,
        C::F5 => Key::F5,
        C::F6 => Key::F6,
        C::F7 => Key::F7,
        C::F8 => Key::F8,
        C::F9 => Key::F9,
        C::F10 => Key::F10,
        C::F11 => Key::F11,
        C::F12 => Key::F12,
        C::Numpad0 => Key::Numpad0,
        C::Numpad1 => Key::Numpad1,
        C::Numpad2 => Key::Numpad2,
        C::Numpad3 => Key::Numpad3,
        C::Numpad4 => Key::Numpad4,
        C::Numpad5 => Key::Numpad5,
        C::Numpad6 => Key::Numpad6,
        C::Numpad7 => Key::Numpad7,
        C::Numpad8 => Key::Numpad8,
        C::Numpad9 => Key::Numpad9,
        C::NumpadAdd => Key::NumpadAdd,
        C::NumpadSubtract => Key::NumpadSubtract,
        C::NumpadMultiply => Key::NumpadMultiply,
        C::NumpadDivide => Key::NumpadDivide,
        C::NumpadDecimal => Key::NumpadDecimal,
        C::NumpadEnter => Key::NumpadEnter,
        C::NumLock => Key::NumLock,
        C::Space => Key::Space,
        C::Enter => Key::Enter,
        C::Escape => Key::Escape,
        C::Backspace => Key::Backspace,
        C::Tab => Key::Tab,
        C::ShiftLeft => Key::ShiftLeft,
        C::ShiftRight => Key::ShiftRight,
        C::ControlLeft => Key::ControlLeft,
        C::ControlRight => Key::ControlRight,
        C::AltLeft => Key::AltLeft,
        C::AltRight => Key::AltRight,
        C::ArrowUp => Key::ArrowUp,
        C::ArrowDown => Key::ArrowDown,
        C::ArrowLeft => Key::ArrowLeft,
        C::ArrowRight => Key::ArrowRight,
        C::Home => Key::Home,
        C::End => Key::End,
        C::PageUp => Key::PageUp,
        C::PageDown => Key::PageDown,
        C::Insert => Key::Insert,
        C::Delete => Key::Delete,
        C::Backquote => Key::Backquote,
        C::Minus => Key::Minus,
        C::Equal => Key::Equal,
        C::BracketLeft => Key::BracketLeft,
        C::BracketRight => Key::BracketRight,
        C::Backslash => Key::Backslash,
        C::Semicolon => Key::Semicolon,
        C::Quote => Key::Quote,
        C::Comma => Key::Comma,
        C::Period => Key::Period,
        C::Slash => Key::Slash,
        _ => return None,
    })
}

/// The shell's button for a mouse button, if it has one.
#[must_use]
pub fn button(b: MouseButton) -> Option<Button> {
    match b {
        MouseButton::Left => Some(Button::Left),
        MouseButton::Right => Some(Button::Right),
        MouseButton::Middle => Some(Button::Middle),
        _ => None,
    }
}

/// A key pressed, as the window system read it: its text with every modifier (its first text has
/// Control taken off), and the key with none (the letter it bears on the layout).
#[derive(Clone, Debug, Default)]
pub struct Pressed {
    pub text_with_all_modifiers: Option<String>,
    pub bare: Option<String>,
}

impl Pressed {
    /// The window system's key event.
    #[must_use]
    pub fn of(event: &winit::event::KeyEvent) -> Self {
        use winit::platform::modifier_supplement::KeyEventExtModifierSupplement as _;
        let bare = match event.key_without_modifiers() {
            winit::keyboard::Key::Character(c) => Some(c.to_string()),
            _ => None,
        };
        Self {
            text_with_all_modifiers: event.text_with_all_modifiers().map(str::to_owned),
            bare,
        }
    }
}

/// What a key press types into the interface: its text with every modifier applied, so Ctrl+C,
/// Ctrl+X and Ctrl+V type the control characters an entry copies, cuts and pastes for (as the
/// original read the system's characters), not the bare letter. With Command held (`command`, on
/// a Mac) the same three keys, by the letter they bear on the layout, type those characters, and
/// any other key types nothing.
#[must_use]
pub fn typed(key: &Pressed, command: bool) -> Option<String> {
    if !command {
        return key.text_with_all_modifiers.clone();
    }
    let control = match key.bare.as_deref().map(str::to_ascii_lowercase).as_deref() {
        Some("c") => '\u{3}',
        Some("v") => '\u{16}',
        Some("x") => '\u{18}',
        _ => return None,
    };
    Some(control.to_string())
}

/// A key by the name a script gives it (the shell's name: `W`, `Numpad5`, `Space`...).
#[must_use]
pub fn named(name: &str) -> Option<Key> {
    use Key as K;
    const ALL: &[Key] = &[
        K::A,
        K::B,
        K::C,
        K::D,
        K::E,
        K::F,
        K::G,
        K::H,
        K::I,
        K::J,
        K::K,
        K::L,
        K::M,
        K::N,
        K::O,
        K::P,
        K::Q,
        K::R,
        K::S,
        K::T,
        K::U,
        K::V,
        K::W,
        K::X,
        K::Y,
        K::Z,
        K::Numpad0,
        K::Numpad1,
        K::Numpad2,
        K::Numpad3,
        K::Numpad4,
        K::Numpad5,
        K::Numpad6,
        K::Numpad7,
        K::Numpad8,
        K::Numpad9,
        K::NumpadAdd,
        K::NumpadSubtract,
        K::NumpadDecimal,
        K::NumpadEnter,
        K::NumLock,
        K::Space,
        K::Enter,
        K::Escape,
        K::ShiftLeft,
        K::ArrowUp,
        K::ArrowDown,
        K::ArrowLeft,
        K::ArrowRight,
        K::BracketLeft,
        K::BracketRight,
        K::Equal,
        K::F1,
        K::F2,
        K::F3,
        K::F4,
        K::F5,
        K::F6,
        K::F7,
        K::F8,
        K::F9,
        K::F10,
        K::F11,
        K::F12,
        K::NumpadDivide,
        K::NumpadMultiply,
        K::ControlLeft,
        K::ControlRight,
        K::AltLeft,
        K::AltRight,
        K::Tab,
        K::Backspace,
        K::Digit0,
        K::Digit1,
        K::Digit2,
        K::Digit3,
        K::Digit4,
        K::Digit5,
        K::Digit6,
        K::Digit7,
        K::Digit8,
        K::Digit9,
        K::Minus,
        K::Backquote,
        K::Slash,
        K::Semicolon,
        K::PageUp,
        K::PageDown,
        K::Home,
        K::End,
    ];
    ALL.iter()
        .copied()
        .find(|k| format!("{k:?}").eq_ignore_ascii_case(name))
}

#[cfg(test)]
mod tests {
    use super::{typed, Pressed};

    fn pressed(all: &str, bare: &str) -> Pressed {
        Pressed {
            text_with_all_modifiers: Some(all.to_owned()),
            bare: Some(bare.to_owned()),
        }
    }

    /// Ctrl+V, Ctrl+C and Ctrl+X type the control characters an entry pastes, copies and cuts for,
    /// as the window system makes them with Control applied; a plain key types its letter.
    ///
    /// Behaviour: desktop.input.ctrl-with-a-letter-types-its-control-character
    #[test]
    fn ctrl_with_a_letter_types_its_control_character() {
        assert_eq!(
            typed(&pressed("\u{16}", "v"), false).as_deref(),
            Some("\u{16}")
        );
        assert_eq!(
            typed(&pressed("\u{3}", "c"), false).as_deref(),
            Some("\u{3}")
        );
        assert_eq!(
            typed(&pressed("\u{18}", "x"), false).as_deref(),
            Some("\u{18}")
        );
        assert_eq!(typed(&pressed("V", "v"), false).as_deref(), Some("V"));
        assert_eq!(typed(&Pressed::default(), false), None);
    }

    /// With Command held on a Mac, V, C and X (by the letter on the layout) type the paste, copy
    /// and cut characters, and any other key types nothing.
    ///
    /// Behaviour: desktop.input.command-with-v-c-or-x-pastes-copies-or-cuts-on-a-mac
    #[test]
    fn command_with_v_c_or_x_pastes_copies_or_cuts_on_a_mac() {
        assert_eq!(typed(&pressed("v", "v"), true).as_deref(), Some("\u{16}"));
        assert_eq!(typed(&pressed("c", "c"), true).as_deref(), Some("\u{3}"));
        assert_eq!(typed(&pressed("X", "X"), true).as_deref(), Some("\u{18}"));
        assert_eq!(
            typed(&pressed("a", "a"), true),
            None,
            "Command+A types nothing"
        );
        assert_eq!(
            typed(&pressed("v", "k"), true),
            None,
            "the letter on the layout decides, not the key's place"
        );
    }
}
