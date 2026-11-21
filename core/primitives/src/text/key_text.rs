//! The text a key press types: what a host reports as a key's text, less what a box never takes.
//!
//! Both games' clients read a key's text from the host and type it into a box; this is the one
//! rule for which of its characters are typed, so every host and every box agree.

/// Whether `ch` is the stand-in a host reports as the text of a key that types nothing.
///
/// macOS reports the arrows, Home, End, the page keys, forward Delete and the function row as text
/// in U+F700–U+F8FF, a private-use block it sets aside for those keys; Windows and the browser
/// report no text for them at all. The keys themselves still arrive as key presses, and that is
/// what moves a caret, so the stand-in is never typed, on any platform.
#[must_use]
pub fn is_key_text_stand_in(ch: char) -> bool {
    ('\u{F700}'..='\u{F8FF}').contains(&ch)
}

/// The characters a key's text types into a box: the text without control characters and without
/// [`is_key_text_stand_in`]'s stand-ins.
pub fn typed_characters(text: &str) -> impl Iterator<Item = char> + '_ {
    text.chars()
        .filter(|ch| !ch.is_control() && !is_key_text_stand_in(*ch))
}
