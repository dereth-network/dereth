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

/// The word that returns to the day's own weather. `auto`, the word [`Weather::word`] gives it,
/// is taken too.
pub const RESET_WORD: &str = "reset";

/// What `@weather` takes: the tail of every refusal.
pub const USAGE: &str = "@weather takes clear, rain, snow or reset.";

/// The answer when there is no world, and so no sky, yet.
pub const NO_WORLD: &str = "The weather can be shown and set once the world is loaded.";

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
    if word.eq_ignore_ascii_case(RESET_WORD) {
        return Ok(WeatherCommand::Set(Weather::Auto));
    }
    WEATHERS
        .iter()
        .find(|w| w.word().eq_ignore_ascii_case(word))
        .map(|w| WeatherCommand::Set(*w))
        .ok_or_else(|| format!("\"{word}\" is not a weather. {USAGE}"))
}

/// The answer to `@weather`, alone or once a weather is set: the weather drawn, marked forced
/// while it is one set on this client. `days_own` names the day's own weather, which is what is
/// drawn when none is set.
#[must_use]
pub fn describe(asked: Weather, days_own: &str) -> String {
    match asked {
        Weather::Auto => format!("Current weather: {days_own}"),
        w => format!("Current weather: {} (forced)", w.word()),
    }
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
    fn clear_rain_snow_and_reset_set_the_weather_in_any_case() {
        for (word, weather) in [
            ("clear", Weather::Clear),
            ("RAIN", Weather::Rain),
            ("Snow", Weather::Snow),
            ("reset", Weather::Auto),
            ("Reset", Weather::Auto),
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
    fn help_weather_says_each_form_the_command_takes() {
        let want = concat!(
            "@weather - Shows the current weather and if it is forced locally.\n",
            "@weather clear - Forces local weather to clear skies\n",
            "@weather rain - Forces local weather to rainfall\n",
            "@weather snow - Forces local weather to snowfall\n",
            "@weather reset - Restore natural weather set by the world timeline.\n",
        );
        let c = crate::cmd::CommandInterp::new();
        for name in ["weather", "@weather", "/weather"] {
            let lines = c.do_help(&[name.to_owned()]);
            assert_eq!(lines.len(), 2, "{name}: {lines:?}");
            assert_eq!(
                lines[1].0,
                format!("{}{want}", crate::cmd::help::FOR_MORE_INFORMATION),
                "{name}"
            );
        }
    }

    /// Behaviour: chat.commands.weather-is-answered-by-this-client-and-never-sent
    #[test]
    fn the_answer_names_the_weather_and_says_forced_only_when_it_is_set_here() {
        assert_eq!(describe(Weather::Auto, "rain"), "Current weather: rain");
        assert_eq!(describe(Weather::Auto, "cloudy"), "Current weather: cloudy");
        assert_eq!(
            describe(Weather::Clear, "rain"),
            "Current weather: clear (forced)"
        );
        assert_eq!(
            describe(Weather::Rain, "clear"),
            "Current weather: rain (forced)"
        );
        assert_eq!(
            describe(Weather::Snow, "clear"),
            "Current weather: snow (forced)"
        );
    }
}
