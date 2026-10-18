//! `@tod`, this client's own command for the time of day it draws (CD-038). Retail has none.
//!
//! The command moves the sky clock's local adjustment: the offset added to the server's time
//! before the sky, its light and everything that follows them read it. So it changes only what
//! this client draws, and nothing is sent. This module holds the command's words: what a typed
//! line asks for, and the sentences it answers with.

/// What a typed `@tod` asks for.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum TimeOfDayCommand {
    /// Say what time of day the sky is drawn at, and whose clock that is.
    Show,
    /// Draw the sky at this fraction of the day, from 0 (midnight) to 1, and let it run on.
    Set(f32),
    /// Follow the server's clock again.
    Reset,
}

/// The names `@tod` takes, with the fraction of the day each one stands for.
pub const NAMED_TIMES: &[(&str, f32)] = &[
    ("midnight", 0.0),
    ("night", 0.0),
    ("dawn", 0.25),
    ("noon", 0.5),
    ("dusk", 0.75),
];

/// The words that return to the server's clock.
pub const RESET_WORDS: &[&str] = &["reset", "server"];

/// What `@tod` takes: the tail of every refusal.
pub const USAGE: &str = "@tod takes a time of day from 0 to 1 (0 is midnight, 0.25 dawn, \
                         0.5 noon, 0.75 dusk), midnight, dawn, noon, dusk or night, or reset.";

/// The answer when there is no world, and so no sky, yet.
pub const NO_WORLD: &str = "The time of day can be shown and set once the world is loaded.";

/// Read a typed `@tod`'s arguments. `Err` is the one line that says what was wrong.
///
/// # Errors
///
/// More than one word, or a word that is neither a fraction from 0 to 1, a named time nor a
/// reset word.
pub fn parse(args: &[String]) -> Result<TimeOfDayCommand, String> {
    let word = match args {
        [] => return Ok(TimeOfDayCommand::Show),
        [word] => word.as_str(),
        _ => return Err(format!("Too many words after @tod. {USAGE}")),
    };
    if RESET_WORDS.iter().any(|w| w.eq_ignore_ascii_case(word)) {
        return Ok(TimeOfDayCommand::Reset);
    }
    if let Some((_, fraction)) = NAMED_TIMES
        .iter()
        .find(|(name, _)| name.eq_ignore_ascii_case(word))
    {
        return Ok(TimeOfDayCommand::Set(*fraction));
    }
    match word.parse::<f32>() {
        // A NaN or an infinity is outside the range, so the one test refuses them too.
        Ok(fraction) if (0.0..=1.0).contains(&fraction) => {
            Ok(TimeOfDayCommand::Set(fraction.abs()))
        }
        _ => Err(format!("\"{word}\" is not a time of day. {USAGE}")),
    }
}

/// The answer to `@tod`, alone, once a time is set and once it is reset: the region's name for
/// that part of the day where it has one, then the fraction of the day to three places, marked
/// forced while the time is one set on this client.
#[must_use]
pub fn describe(fraction: f32, name: Option<&str>, forced: bool) -> String {
    // The clock's single-precision day boundary can leave midnight a hair below zero; it reads as
    // midnight, not as "-0.000".
    let fraction = if fraction > 0.0 { fraction } else { 0.0 };
    let at = match name.map(str::trim).filter(|n| !n.is_empty()) {
        Some(name) => format!("{name} ({fraction:.3})"),
        None => format!("{fraction:.3}"),
    };
    let forced = if forced { " (forced)" } else { "" };
    format!("The current time of day is {at}{forced}")
}

#[cfg(test)]
mod tests {
    use super::*;

    fn words(line: &str) -> Vec<String> {
        crate::cmd::interp::find_all_words(line)
    }

    /// Behaviour: chat.commands.tod-is-answered-by-this-client-and-never-sent
    #[test]
    fn a_bare_tod_asks_for_the_time_of_day() {
        assert_eq!(parse(&[]), Ok(TimeOfDayCommand::Show));
    }

    /// Behaviour: chat.commands.tod-is-answered-by-this-client-and-never-sent
    #[test]
    fn a_fraction_from_zero_to_one_sets_the_time_of_day() {
        for (word, fraction) in [
            ("0", 0.0),
            ("0.25", 0.25),
            (".5", 0.5),
            ("0.75", 0.75),
            ("0.9", 0.9),
            ("1", 1.0),
            ("-0", 0.0),
        ] {
            assert_eq!(
                parse(&words(word)),
                Ok(TimeOfDayCommand::Set(fraction)),
                "{word}"
            );
        }
    }

    /// Behaviour: chat.commands.tod-is-answered-by-this-client-and-never-sent
    #[test]
    fn midnight_dawn_noon_dusk_and_night_are_the_quarter_days() {
        for (word, fraction) in [
            ("midnight", 0.0),
            ("Dawn", 0.25),
            ("NOON", 0.5),
            ("dusk", 0.75),
            ("night", 0.0),
        ] {
            assert_eq!(
                parse(&words(word)),
                Ok(TimeOfDayCommand::Set(fraction)),
                "{word}"
            );
        }
    }

    /// Behaviour: chat.commands.tod-is-answered-by-this-client-and-never-sent
    #[test]
    fn reset_and_server_return_to_the_servers_clock() {
        for word in ["reset", "server", "RESET", "Server"] {
            assert_eq!(parse(&words(word)), Ok(TimeOfDayCommand::Reset), "{word}");
        }
    }

    /// Behaviour: chat.commands.tod-is-answered-by-this-client-and-never-sent
    #[test]
    fn anything_else_is_refused_in_one_line_that_says_what_tod_takes() {
        for (line, names) in [
            ("1.5", "\"1.5\""),
            ("-0.25", "\"-0.25\""),
            ("teatime", "\"teatime\""),
            ("NaN", "\"NaN\""),
            ("inf", "\"inf\""),
            ("0.5,", "\"0.5,\""),
            ("noon please", "Too many words"),
            ("0.5 0.6", "Too many words"),
        ] {
            let refusal = parse(&words(line)).expect_err(line);
            assert!(refusal.contains(names), "{line}: {refusal}");
            assert!(refusal.ends_with(USAGE), "{line}: {refusal}");
            assert!(!refusal.contains('\n'), "one line: {refusal:?}");
        }
    }

    /// Behaviour: chat.commands.tod-is-answered-by-this-client-and-never-sent
    #[test]
    fn help_tod_says_each_form_the_command_takes() {
        let want = concat!(
            "@tod - Shows the current time of day and if it is forced locally.\n",
            "@tod <0 to 1> - Forces the time of day and lets it run from there.\n",
            "@tod midnight, dawn, noon, dusk - Forces the time of day and lets it run from there.\n",
            "@tod reset - Restore natural time of day set by the world timeline.\n",
        );
        let c = crate::cmd::CommandInterp::new();
        for name in ["tod", "@tod", "/tod"] {
            let lines = c.do_help(&[name.to_owned()]);
            assert_eq!(lines.len(), 2, "{name}: {lines:?}");
            assert_eq!(
                lines[1].0,
                format!("{}{want}", crate::cmd::help::FOR_MORE_INFORMATION),
                "{name}"
            );
        }
    }

    /// Behaviour: chat.commands.tod-is-answered-by-this-client-and-never-sent
    #[test]
    fn the_answer_names_the_time_and_says_forced_only_when_it_is_set_here() {
        assert_eq!(
            describe(0.5, Some("Midsong"), false),
            "The current time of day is Midsong (0.500)"
        );
        assert_eq!(
            describe(0.25, Some("Dawnsong"), true),
            "The current time of day is Dawnsong (0.250) (forced)"
        );
        // A region with no name for the time, or a blank one, gives the fraction alone.
        assert_eq!(
            describe(0.125, None, false),
            "The current time of day is 0.125"
        );
        assert_eq!(
            describe(0.125, Some(" "), true),
            "The current time of day is 0.125 (forced)"
        );
        // Midnight a hair below zero reads as midnight.
        assert_eq!(
            describe(-1e-7, Some("Darktide"), false),
            "The current time of day is Darktide (0.000)"
        );
    }
}
