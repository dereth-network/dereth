//! How many players a world says it has, when it says so.
//!
//! No message tells this client how many players a world has. A world may still say it in a line
//! of its own text when a character enters it, as Icefire's welcome does: "There are currently 2
//! other players logged in." Such a line counts the others, so the world has one more: the player
//! reading it. The count is passed on for the Dereth Network's list of worlds
//! ([`crate::Client::take_world_players`]).
//!
//! Only a line the server writes out itself is read (a literal, with no string of the game's own
//! and nothing filled in), and only when the whole line says that and nothing else.

use auberean_protocol::text::StringInfo;

/// The world's players, when `text` (a line of the server's) says how many others are logged in.
#[must_use]
pub fn said(text: &StringInfo) -> Option<u32> {
    if text.string_id != 0 || !text.variables.is_empty() {
        return None;
    }
    others(&text.literal.as_ref()?.text())?.checked_add(1)
}

/// How many other players `line` says are logged in: "There are currently N other players logged
/// in.", or "is" and "player" for one, "currently" left out, "online" for "logged in", in any case.
fn others(line: &str) -> Option<u32> {
    let line = line.trim().trim_end_matches(['.', '!']).to_lowercase();
    let mut words = line.split_whitespace();
    if words.next()? != "there" || !matches!(words.next()?, "is" | "are") {
        return None;
    }
    let mut word = words.next()?;
    if word == "currently" {
        word = words.next()?;
    }
    if !word.bytes().all(|b| b.is_ascii_digit()) {
        return None;
    }
    let n = word.parse().ok()?;
    if words.next()? != "other" || !matches!(words.next()?, "player" | "players") {
        return None;
    }
    let rest: Vec<&str> = words.collect();
    (rest == ["logged", "in"] || rest == ["online"]).then_some(n)
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Behaviour: session.players.a-worlds-line-of-how-many-others-are-on-counts-its-players
    #[test]
    fn a_line_saying_how_many_others_are_logged_in_counts_them_and_the_player() {
        let line = |s: &str| said(&StringInfo::literal(s));
        assert_eq!(
            line("There are currently 2 other players logged in.\n"),
            Some(3)
        );
        assert_eq!(
            line("There are currently 0 other players logged in."),
            Some(1)
        );
        assert_eq!(
            line("There is currently 1 other player logged in."),
            Some(2)
        );
        assert_eq!(line("there are 14 other players online"), Some(15));
        for not_a_count in [
            "There are currently 2 other players logged in, and a dragon.",
            "There are currently two other players logged in.",
            "There are currently -2 other players logged in.",
            "There are currently +2 other players logged in.",
            "There are currently 99999999999 other players logged in.",
            "There are currently 2 players logged in.",
            "Bob says: There are currently 2 other players logged in.",
            "-- Welcome to the server --",
            "",
        ] {
            assert_eq!(line(not_a_count), None, "{not_a_count:?}");
        }
    }

    /// Behaviour: session.players.a-worlds-line-of-how-many-others-are-on-counts-its-players
    #[test]
    fn only_a_line_the_server_writes_out_itself_is_read() {
        let mut from_a_table =
            StringInfo::literal("There are currently 2 other players logged in.");
        from_a_table.string_id = 0x1234;
        assert_eq!(said(&from_a_table), None);
        let mut nothing = StringInfo::literal("");
        nothing.has_literal = 0;
        nothing.literal = None;
        assert_eq!(said(&nothing), None);
    }
}
