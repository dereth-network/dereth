//! `@weather`, this client's own command for the weather it draws (CD-039). Retail has none.
//!
//! The command chooses the kind of day the sky is drawn as, in place of the one the calendar
//! gives: a clear day or a rainy one, and with the Horizon interface's Weather effect, whether its
//! rain falls as rain or as snow. It changes only what this client draws, and nothing is sent.
//! This module holds the command's words: what a typed line asks for, and the sentences it
//! answers with.

/// The weather `@weather` asks for.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum Weather {
    /// The day's own weather: the calendar's day, and with the Weather effect the land round the
    /// viewer saying whether its rain falls as rain or as snow.
    #[default]
    Auto,
    /// A clear day: nothing falls.
    Clear,
    /// A rainy day, its rain falling as rain whatever the land.
    Rain,
    /// A rainy day, its rain falling as snow whatever the land. The game draws no snow of its
    /// own: without the Weather effect the day is a rainy one.
    Snow,
}

impl Weather {
    /// The word `@weather` takes for it.
    #[must_use]
    pub const fn word(self) -> &'static str {
        match self {
            Self::Auto => "auto",
            Self::Clear => "clear",
            Self::Rain => "rain",
            Self::Snow => "snow",
        }
    }
}

/// What a typed `@weather` asks for.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum WeatherCommand {
    /// Say what weather is drawn, and whose choice it is.
    Show,
    /// Draw this weather from now on.
    Set(Weather),
}

/// Every weather `@weather` takes, by its word.
pub const WEATHERS: [Weather; 4] = [Weather::Clear, Weather::Rain, Weather::Snow, Weather::Auto];

/// What `@weather` takes: the tail of every refusal.
pub const USAGE: &str = "@weather takes clear, rain, snow or auto.";

/// The answer when there is no world, and so no sky, yet.
pub const NO_WORLD: &str = "The weather can be shown and set once the world is loaded.";

/// The tail of an answer while the player's options leave the falling weather out.
pub const WEATHER_OFF: &str =
    "Nothing falls while the Disable Most Weather Effects option is ticked.";

/// Read a typed `@weather`'s arguments. `Err` is the one line that says what was wrong.
///
/// # Errors
///
/// More than one word, or a word that is not one of the weathers.
pub fn parse(args: &[String]) -> Result<WeatherCommand, String> {
    let word = match args {
        [] => return Ok(WeatherCommand::Show),
        [word] => word.as_str(),
        _ => return Err(format!("Too many words after @weather. {USAGE}")),
    };
    WEATHERS
        .iter()
        .find(|w| w.word().eq_ignore_ascii_case(word))
        .map(|w| WeatherCommand::Set(*w))
        .ok_or_else(|| format!("\"{word}\" is not a weather. {USAGE}"))
}

/// `sentence`, with the note that nothing falls when the options leave the weather out.
fn noting(sentence: String, weather_off: bool) -> String {
    if weather_off {
        format!("{sentence} {WEATHER_OFF}")
    } else {
        sentence
    }
}

/// The answer to a bare `@weather`: the weather drawn, and whether it is the day's own or one
/// set on this client.
#[must_use]
pub fn describe(asked: Weather, weather_off: bool) -> String {
    let sentence = match asked {
        Weather::Auto => "The weather is the day's own.".to_owned(),
        w => format!(
            "The weather is {}, set on this client. @weather auto returns to the day's own.",
            w.word()
        ),
    };
    noting(sentence, weather_off)
}

/// The answer to `@weather <weather>`, once it is drawn.
#[must_use]
pub fn set_reply(asked: Weather, weather_off: bool) -> String {
    let sentence = match asked {
        Weather::Auto => "The weather is the day's own again.".to_owned(),
        Weather::Snow => "The weather is now snow on this client only. The Horizon interface's \
                          Weather effect draws the snow; without it the day is a rainy one. \
                          @weather auto returns to the day's own."
            .to_owned(),
        w => format!(
            "The weather is now {} on this client only. @weather auto returns to the day's own.",
            w.word()
        ),
    };
    noting(sentence, weather_off)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn words(line: &str) -> Vec<String> {
        crate::cmd::interp::find_all_words(line)
    }

    /// Behaviour: chat.commands.weather-is-answered-by-this-client-and-never-sent
    #[test]
    fn a_bare_weather_asks_what_weather_is_drawn() {
        assert_eq!(parse(&[]), Ok(WeatherCommand::Show));
    }

    /// Behaviour: chat.commands.weather-is-answered-by-this-client-and-never-sent
    #[test]
    fn clear_rain_snow_and_auto_set_the_weather_in_any_case() {
        for (word, weather) in [
            ("clear", Weather::Clear),
            ("RAIN", Weather::Rain),
            ("Snow", Weather::Snow),
            ("auto", Weather::Auto),
        ] {
            assert_eq!(
                parse(&words(word)),
                Ok(WeatherCommand::Set(weather)),
                "{word}"
            );
        }
    }

    /// Behaviour: chat.commands.weather-is-answered-by-this-client-and-never-sent
    #[test]
    fn anything_else_is_refused_in_one_line_that_says_what_weather_takes() {
        for (line, names) in [
            ("hail", "\"hail\""),
            ("sunny", "\"sunny\""),
            ("rain,", "\"rain,\""),
            ("0.5", "\"0.5\""),
            ("rain please", "Too many words"),
            ("rain snow", "Too many words"),
        ] {
            let refusal = parse(&words(line)).expect_err(line);
            assert!(refusal.contains(names), "{line}: {refusal}");
            assert!(refusal.ends_with(USAGE), "{line}: {refusal}");
            assert!(!refusal.contains('\n'), "one line: {refusal:?}");
        }
    }

    /// Behaviour: chat.commands.weather-is-answered-by-this-client-and-never-sent
    #[test]
    fn help_weather_explains_every_form_the_command_takes() {
        let c = crate::cmd::CommandInterp::new();
        for name in ["weather", "@weather", "/weather"] {
            let lines = c.do_help(&[name.to_owned()]);
            assert_eq!(lines.len(), 2, "{name}: {lines:?}");
            let body = &lines[1].0;
            for form in [
                "@weather - ",
                "@weather clear - ",
                "@weather rain - ",
                "@weather snow - ",
                "@weather auto - ",
            ] {
                assert!(body.contains(form), "{name}: {form} is missing from {body}");
            }
        }
    }

    #[test]
    fn the_answers_name_the_weather_and_whose_choice_it_is() {
        assert_eq!(
            describe(Weather::Auto, false),
            "The weather is the day's own."
        );
        assert_eq!(
            describe(Weather::Rain, false),
            "The weather is rain, set on this client. @weather auto returns to the day's own."
        );
        assert_eq!(
            set_reply(Weather::Clear, false),
            "The weather is now clear on this client only. @weather auto returns to the day's \
             own."
        );
        assert!(set_reply(Weather::Snow, false).contains("Weather effect draws the snow"));
        assert_eq!(
            set_reply(Weather::Auto, false),
            "The weather is the day's own again."
        );
        // With the options leaving the weather out, every answer says that nothing falls.
        for w in WEATHERS {
            assert!(describe(w, true).ends_with(WEATHER_OFF), "{w:?}");
            assert!(set_reply(w, true).ends_with(WEATHER_OFF), "{w:?}");
            assert!(!set_reply(w, false).contains(WEATHER_OFF), "{w:?}");
        }
    }
}
