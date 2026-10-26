//! What a plugin wrote that the host's own dialog and panel show (its name, a file request's
//! prompt, its console's lines): put on one line, rid of control and direction characters, and cut
//! to a length, so a plugin's words can neither break their lines nor push the buttons off the
//! screen.

/// The most characters of a plugin's name shown.
pub const NAME: usize = 48;
/// The most characters of a file request's prompt shown.
pub const PROMPT: usize = 200;
/// The most characters of a line of a plugin's console shown.
pub const LINE: usize = 200;

/// The colour a plugin's words are shown in inside the plugin panel, apart from the host's own
/// lines.
pub const PLUGIN_COLOUR: i64 = 0x00C8_B4F0;

/// Whether `c` may stand in a line the host shows: not a control character, a line or
/// paragraph separator, or a mark that turns the direction text runs in.
fn plain(c: char) -> bool {
    !c.is_control()
        && !matches!(
            c,
            '\u{2028}' | '\u{2029}' | '\u{200E}' | '\u{200F}' | '\u{202A}'..='\u{202E}'
                | '\u{2066}'..='\u{2069}'
        )
}

/// `text` as the host's dialog and panel show it: line breaks, control characters and the characters
/// that turn the direction text runs in made spaces, runs of spaces one, trimmed, and at most
/// `max` characters, the last an ellipsis where it was cut.
#[must_use]
pub fn bounded(text: &str, max: usize) -> String {
    let mut out = String::new();
    let mut space = false;
    for c in text.chars() {
        if plain(c) && !c.is_whitespace() {
            if space && !out.is_empty() {
                out.push(' ');
            }
            space = false;
            out.push(c);
        } else {
            space = true;
        }
    }
    if out.chars().count() > max {
        out = out.chars().take(max.saturating_sub(1)).collect();
        out.push('\u{2026}');
    }
    out
}

/// A plugin's own words as the plugin panel shows them, apart from the host's: [`bounded`] to
/// `max`, its double quotes made single, between double quotes.
#[must_use]
pub fn quoted(text: &str, max: usize) -> String {
    format!("\"{}\"", bounded(text, max).replace('"', "'"))
}

/// A plugin's own words as a cell of a list in the plugin panel: [`quoted`], in
/// [`PLUGIN_COLOUR`].
#[must_use]
pub fn plugin_cell(text: &str, max: usize) -> dereth_plugin_api::Value {
    dereth_plugin_api::Value::map()
        .with("text", quoted(text, max))
        .with("color", PLUGIN_COLOUR)
}

#[cfg(test)]
mod tests {
    //! Behaviour: none (the plugin host's own dialog and panel)
    use super::*;

    /// A plugin's words are shown on one line, without control or direction characters, and cut
    /// to their length.
    #[test]
    fn a_plugins_words_are_one_plain_line_cut_to_their_length() {
        assert_eq!(bounded("  Loot\nnow\r\n\tplease ", 48), "Loot now please");
        assert_eq!(bounded("ab\u{202E}cd\u{0007}e", 48), "ab cd e");
        let long = "x".repeat(500);
        let cut = bounded(&long, PROMPT);
        assert_eq!(cut.chars().count(), PROMPT);
        assert!(cut.ends_with('\u{2026}'));
        assert_eq!(bounded("short", NAME), "short");
    }
}
