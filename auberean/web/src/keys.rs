//! The page's `KeyboardEvent.code` and button numbers as the shell's keys and buttons. The page
//! names physical keys, as the shell does.

use auberean_shell::input::{Button, Key};

/// The shell's key for a page's key code, if it has one.
#[must_use]
pub fn key(code: &str) -> Option<Key> {
    Some(match code {
        "KeyA" => Key::A,
        "KeyB" => Key::B,
        "KeyC" => Key::C,
        "KeyD" => Key::D,
        "KeyE" => Key::E,
        "KeyF" => Key::F,
        "KeyG" => Key::G,
        "KeyH" => Key::H,
        "KeyI" => Key::I,
        "KeyJ" => Key::J,
        "KeyK" => Key::K,
        "KeyL" => Key::L,
        "KeyM" => Key::M,
        "KeyN" => Key::N,
        "KeyO" => Key::O,
        "KeyP" => Key::P,
        "KeyQ" => Key::Q,
        "KeyR" => Key::R,
        "KeyS" => Key::S,
        "KeyT" => Key::T,
        "KeyU" => Key::U,
        "KeyV" => Key::V,
        "KeyW" => Key::W,
        "KeyX" => Key::X,
        "KeyY" => Key::Y,
        "KeyZ" => Key::Z,
        "Digit0" => Key::Digit0,
        "Digit1" => Key::Digit1,
        "Digit2" => Key::Digit2,
        "Digit3" => Key::Digit3,
        "Digit4" => Key::Digit4,
        "Digit5" => Key::Digit5,
        "Digit6" => Key::Digit6,
        "Digit7" => Key::Digit7,
        "Digit8" => Key::Digit8,
        "Digit9" => Key::Digit9,
        "F1" => Key::F1,
        "F2" => Key::F2,
        "F3" => Key::F3,
        "F4" => Key::F4,
        "F5" => Key::F5,
        "F6" => Key::F6,
        "F7" => Key::F7,
        "F8" => Key::F8,
        "F9" => Key::F9,
        "F10" => Key::F10,
        "F11" => Key::F11,
        "F12" => Key::F12,
        "Numpad0" => Key::Numpad0,
        "Numpad1" => Key::Numpad1,
        "Numpad2" => Key::Numpad2,
        "Numpad3" => Key::Numpad3,
        "Numpad4" => Key::Numpad4,
        "Numpad5" => Key::Numpad5,
        "Numpad6" => Key::Numpad6,
        "Numpad7" => Key::Numpad7,
        "Numpad8" => Key::Numpad8,
        "Numpad9" => Key::Numpad9,
        "NumpadAdd" => Key::NumpadAdd,
        "NumpadSubtract" => Key::NumpadSubtract,
        "NumpadMultiply" => Key::NumpadMultiply,
        "NumpadDivide" => Key::NumpadDivide,
        "NumpadDecimal" => Key::NumpadDecimal,
        "NumpadEnter" => Key::NumpadEnter,
        "NumLock" => Key::NumLock,
        "Space" => Key::Space,
        "Enter" => Key::Enter,
        "Escape" => Key::Escape,
        "Backspace" => Key::Backspace,
        "Tab" => Key::Tab,
        "ShiftLeft" => Key::ShiftLeft,
        "ShiftRight" => Key::ShiftRight,
        "ControlLeft" => Key::ControlLeft,
        "ControlRight" => Key::ControlRight,
        "AltLeft" => Key::AltLeft,
        "AltRight" => Key::AltRight,
        "ArrowUp" => Key::ArrowUp,
        "ArrowDown" => Key::ArrowDown,
        "ArrowLeft" => Key::ArrowLeft,
        "ArrowRight" => Key::ArrowRight,
        "Home" => Key::Home,
        "End" => Key::End,
        "PageUp" => Key::PageUp,
        "PageDown" => Key::PageDown,
        "Insert" => Key::Insert,
        "Delete" => Key::Delete,
        "Backquote" => Key::Backquote,
        "Minus" => Key::Minus,
        "Equal" => Key::Equal,
        "BracketLeft" => Key::BracketLeft,
        "BracketRight" => Key::BracketRight,
        "Backslash" => Key::Backslash,
        "Semicolon" => Key::Semicolon,
        "Quote" => Key::Quote,
        "Comma" => Key::Comma,
        "Period" => Key::Period,
        "Slash" => Key::Slash,
        _ => return None,
    })
}

/// The shell's button for a page's button number, if it has one.
#[must_use]
pub fn button(number: u16) -> Option<Button> {
    match number {
        0 => Some(Button::Left),
        1 => Some(Button::Middle),
        2 => Some(Button::Right),
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    //! Behaviour: none (a browser's key names over the shell's: no retail behavior)
    use super::*;

    #[test]
    fn the_page_s_key_names_are_the_shell_s_physical_keys() {
        assert_eq!(key("KeyW"), Some(Key::W));
        assert_eq!(key("Digit1"), Some(Key::Digit1));
        assert_eq!(key("ArrowUp"), Some(Key::ArrowUp));
        assert_eq!(key("Unidentified"), None);
        assert_eq!(key("keyW"), None);
    }

    #[test]
    fn the_page_numbers_its_right_button_after_its_middle_one() {
        assert_eq!(button(0), Some(Button::Left));
        assert_eq!(button(1), Some(Button::Middle));
        assert_eq!(button(2), Some(Button::Right));
        assert_eq!(button(3), None);
    }
}
