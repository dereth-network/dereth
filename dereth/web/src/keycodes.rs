//! The page's `KeyboardEvent.code` strings as the shared key codes. The two name the same
//! physical keys with the same words, so this is a name lookup over the keys the shared key tables
//! know; a key they do not know has no code here either.

use dereth_input::keys::KeyCode as K;

/// The key code `code` names, or `None` for a key the client's tables do not map.
#[must_use]
pub fn from_dom(code: &str) -> Option<K> {
    K::from_code_name(code)
}
#[cfg(test)]
mod tests {
    use super::*;

    /// A letter, a digit, a function key and the extended block's arrows are found by the name
    /// the page gives them, and an unknown name is not.
    #[test]
    fn the_page_s_key_names_are_the_shared_key_codes() {
        assert_eq!(from_dom("KeyW"), Some(K::KeyW));
        assert_eq!(from_dom("Digit1"), Some(K::Digit1));
        assert_eq!(from_dom("F4"), Some(K::F4));
        assert_eq!(from_dom("ArrowUp"), Some(K::ArrowUp));
        assert_eq!(from_dom("Unidentified"), Some(K::Unidentified));
        assert_eq!(from_dom("NoSuchKey"), None);
        assert_eq!(from_dom("keyW"), None);
        assert_eq!(from_dom(" KeyW"), None);
    }
}
