//! `@clouds`, this client's own command for the clouds it draws (CD-cloud-command). Retail has none.
//!
//! The command sets how the Remastered sky draws its clouds, over what the sky's settings give. It
//! changes only what this client draws, and nothing is sent. This module holds the command's
//! words: what a typed line asks for, what it has asked for so far, and the sentence that answers.

/// One thing about the clouds `@clouds` can set.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CloudOption {
    /// How much of the sky they cover.
    Cover,
    /// How fast they drift.
    Speed,
    /// How high over the eye their base lies, meters.
    Height,
    /// How thick they are, meters.
    Depth,
    /// How large they are laid over the sky.
    Size,
    /// How much fine detail breaks them up.
    Detail,
    /// How brightly the sun lights them.
    Light,
    /// How dark their bases are.
    Shade,
    /// How much they shade the land under them.
    Shadows,
}

/// Every option, in the order the answer names them.
pub const OPTIONS: [CloudOption; 9] = [
    CloudOption::Cover,
    CloudOption::Speed,
    CloudOption::Height,
    CloudOption::Depth,
    CloudOption::Size,
    CloudOption::Detail,
    CloudOption::Light,
    CloudOption::Shade,
    CloudOption::Shadows,
];

impl CloudOption {
    /// The word `@clouds` takes for it.
    #[must_use]
    pub const fn word(self) -> &'static str {
        match self {
            Self::Cover => "cover",
            Self::Speed => "speed",
            Self::Height => "height",
            Self::Depth => "depth",
            Self::Size => "size",
            Self::Detail => "detail",
            Self::Light => "light",
            Self::Shade => "shade",
            Self::Shadows => "shadows",
        }
    }

    /// The least and greatest value it takes.
    #[must_use]
    pub const fn range(self) -> (f32, f32) {
        match self {
            Self::Cover | Self::Detail | Self::Shade => (0.0, 2.0),
            Self::Speed => (0.0, 4.0),
            Self::Height => (200.0, 6000.0),
            Self::Depth => (50.0, 4000.0),
            Self::Size => (0.25, 4.0),
            Self::Light => (0.0, 3.0),
            Self::Shadows => (0.0, 1.0),
        }
    }
}

/// What `@clouds` has asked for: a value for each option set, the sky's own for the others.
#[derive(Debug, Clone, Copy, PartialEq, Default)]
pub struct Clouds([Option<f32>; OPTIONS.len()]);

impl Clouds {
    /// The value asked for `option`, if one is.
    #[must_use]
    pub const fn get(&self, option: CloudOption) -> Option<f32> {
        self.0[option as usize]
    }

    /// Carry out `command`.
    pub fn obey(&mut self, command: CloudsCommand) {
        match command {
            CloudsCommand::Show => {}
            CloudsCommand::Set(option, value) => self.0[option as usize] = Some(value),
            CloudsCommand::Unset(option) => self.0[option as usize] = None,
            CloudsCommand::Reset => *self = Self::default(),
        }
    }
}

/// What a typed `@clouds` asks for.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum CloudsCommand {
    /// Say what is set.
    Show,
    /// Draw the clouds with this value from now on.
    Set(CloudOption, f32),
    /// Return this option to the sky's own.
    Unset(CloudOption),
    /// Return every option to the sky's own.
    Reset,
}

/// The words that set the cover alone, with the cover each stands for.
pub const NAMED_COVERS: [(&str, f32); 3] = [("none", 0.0), ("thin", 0.5), ("thick", 2.0)];

/// The word that returns to the sky's own.
pub const RESET_WORD: &str = "reset";

/// What `@clouds` takes: the tail of every refusal.
pub const USAGE: &str = "@clouds takes none, thin, thick or reset, or cover, speed, height, \
                         depth, size, detail, light, shade or shadows with a number or reset.";

/// The answer when there is no world, and so no sky, yet.
pub const NO_WORLD: &str = "The clouds can be shown and set once the world is loaded.";

/// Read a typed `@clouds`'s arguments. `Err` is the one line that says what was wrong.
///
/// # Errors
///
/// More than two words, a word that is not one the command takes, or a value out of its option's
/// range.
pub fn parse(args: &[String]) -> Result<CloudsCommand, String> {
    let (word, value) = match args {
        [] => return Ok(CloudsCommand::Show),
        [word] => (word.as_str(), None),
        [word, value] => (word.as_str(), Some(value.as_str())),
        _ => return Err(format!("Too many words after @clouds. {USAGE}")),
    };
    let option = OPTIONS.iter().find(|o| o.word().eq_ignore_ascii_case(word));
    match (option, value) {
        (Some(option), Some(value)) => parse_value(*option, value),
        (Some(option), None) => Err(format!(
            "\"{word}\" needs {}. {USAGE}",
            wanted_number(*option)
        )),
        (None, None) if word.eq_ignore_ascii_case(RESET_WORD) => Ok(CloudsCommand::Reset),
        (None, None) => NAMED_COVERS
            .iter()
            .find(|(name, _)| name.eq_ignore_ascii_case(word))
            .map(|(_, cover)| CloudsCommand::Set(CloudOption::Cover, *cover))
            .ok_or_else(|| format!("\"{word}\" is not a cloud option. {USAGE}")),
        (None, Some(_)) => Err(format!("\"{word}\" is not a cloud option. {USAGE}")),
    }
}

/// A number from an option's least to its greatest, in words.
fn wanted_number(option: CloudOption) -> String {
    let (least, most) = option.range();
    format!("a number from {least} to {most}")
}

/// Read the value typed for `option`.
fn parse_value(option: CloudOption, word: &str) -> Result<CloudsCommand, String> {
    if word.eq_ignore_ascii_case(RESET_WORD) {
        return Ok(CloudsCommand::Unset(option));
    }
    let (least, most) = option.range();
    match word.parse::<f32>() {
        // A NaN or an infinity is outside the range, so the one test refuses them too.
        Ok(value) if (least..=most).contains(&value) => Ok(CloudsCommand::Set(option, value.abs())),
        _ => Err(format!(
            "\"{word}\" is not {} for {}. {USAGE}",
            wanted_number(option),
            option.word()
        )),
    }
}

/// The answer to `@clouds`, alone or once something is set: each value set here, marked forced, or
/// the sky's own when none is.
#[must_use]
pub fn describe(clouds: &Clouds) -> String {
    let set: Vec<String> = OPTIONS
        .iter()
        .filter_map(|o| clouds.get(*o).map(|v| format!("{} {v}", o.word())))
        .collect();
    if set.is_empty() {
        "Current clouds: the sky's own".to_owned()
    } else {
        format!("Current clouds: {} (forced)", set.join(", "))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn words(line: &str) -> Vec<String> {
        crate::cmd::interp::find_all_words(line)
    }

    /// Behaviour: chat.commands.clouds-is-answered-by-this-client-and-never-sent
    #[test]
    fn a_bare_clouds_asks_what_is_set() {
        assert_eq!(parse(&[]), Ok(CloudsCommand::Show));
    }

    /// Behaviour: chat.commands.clouds-is-answered-by-this-client-and-never-sent
    #[test]
    fn named_covers_options_and_reset_read_in_any_case() {
        use CloudOption as O;
        use CloudsCommand as C;
        for (line, want) in [
            ("none", C::Set(O::Cover, 0.0)),
            ("THIN", C::Set(O::Cover, 0.5)),
            ("Thick", C::Set(O::Cover, 2.0)),
            ("reset", C::Reset),
            ("cover 1.5", C::Set(O::Cover, 1.5)),
            ("SPEED 0", C::Set(O::Speed, 0.0)),
            ("height 3000", C::Set(O::Height, 3000.0)),
            ("depth 50", C::Set(O::Depth, 50.0)),
            ("size 0.25", C::Set(O::Size, 0.25)),
            ("detail 2", C::Set(O::Detail, 2.0)),
            ("light 3", C::Set(O::Light, 3.0)),
            ("shade 0", C::Set(O::Shade, 0.0)),
            ("shadows 1", C::Set(O::Shadows, 1.0)),
            ("speed Reset", C::Unset(O::Speed)),
        ] {
            assert_eq!(parse(&words(line)), Ok(want), "{line}");
        }
    }

    /// Behaviour: chat.commands.clouds-is-answered-by-this-client-and-never-sent
    #[test]
    fn anything_else_is_refused_in_one_line_that_says_what_clouds_takes() {
        for (line, names) in [
            ("fluffy", "\"fluffy\""),
            ("fluffy 1", "\"fluffy\""),
            ("none 1", "\"none\""),
            ("cover", "a number from 0 to 2"),
            ("cover 2.5", "\"2.5\""),
            ("cover -1", "\"-1\""),
            ("cover NaN", "\"NaN\""),
            ("cover inf", "\"inf\""),
            ("height 100", "a number from 200 to 6000"),
            ("cover 1 now", "Too many words"),
        ] {
            let refusal = parse(&words(line)).expect_err(line);
            assert!(refusal.contains(names), "{line}: {refusal}");
            assert!(refusal.ends_with(USAGE), "{line}: {refusal}");
            assert!(!refusal.contains('\n'), "one line: {refusal:?}");
        }
    }

    /// Behaviour: chat.commands.clouds-is-answered-by-this-client-and-never-sent
    #[test]
    fn help_clouds_says_each_form_the_command_takes() {
        let want = concat!(
            "@clouds - Shows the cloud options forced locally.\n",
            "@clouds none, thin, thick - Forces how much of the sky the clouds cover.\n",
            "@clouds cover <0 to 2> - Forces how much of the sky the clouds cover.\n",
            "@clouds speed <0 to 4> - Forces how fast they drift.\n",
            "@clouds height <200 to 6000> - Forces how high their base lies, in meters.\n",
            "@clouds depth <50 to 4000> - Forces how thick they are, in meters.\n",
            "@clouds size <0.25 to 4> - Forces how large they are.\n",
            "@clouds detail <0 to 2> - Forces how much fine detail breaks them up.\n",
            "@clouds light <0 to 3> - Forces how brightly the sun lights them.\n",
            "@clouds shade <0 to 2> - Forces how dark their bases are.\n",
            "@clouds shadows <0 to 1> - Forces how much they shade the land.\n",
            "@clouds <option> reset - Restores one option.\n",
            "@clouds reset - Restores them all.\n",
            "These change the clouds of the Remastered sky, which needs an art pack and Better \
             Lighting, with Retail Look off.\n",
        );
        let c = crate::cmd::CommandInterp::new();
        for name in ["clouds", "@clouds", "/clouds"] {
            let lines = c.do_help(&[name.to_owned()]);
            assert_eq!(lines.len(), 2, "{name}: {lines:?}");
            assert_eq!(
                lines[1].0,
                format!("{}{want}", crate::cmd::help::FOR_MORE_INFORMATION),
                "{name}"
            );
        }
    }

    /// Behaviour: chat.commands.clouds-is-answered-by-this-client-and-never-sent
    #[test]
    fn the_answer_names_what_is_forced_and_reset_returns_to_the_skys_own() {
        use CloudOption as O;
        let mut clouds = Clouds::default();
        assert_eq!(describe(&clouds), "Current clouds: the sky's own");
        clouds.obey(CloudsCommand::Set(O::Speed, 0.0));
        clouds.obey(CloudsCommand::Set(O::Cover, 2.0));
        clouds.obey(CloudsCommand::Show);
        assert_eq!(
            describe(&clouds),
            "Current clouds: cover 2, speed 0 (forced)"
        );
        clouds.obey(CloudsCommand::Unset(O::Cover));
        assert_eq!(clouds.get(O::Cover), None);
        assert_eq!(describe(&clouds), "Current clouds: speed 0 (forced)");
        clouds.obey(CloudsCommand::Reset);
        assert_eq!(clouds, Clouds::default());
    }

    /// `OPTIONS` is in the enum's order, which is how `Clouds` holds them.
    #[test]
    fn each_option_keeps_its_own_value() {
        for (i, option) in OPTIONS.iter().enumerate() {
            assert_eq!(*option as usize, i, "{option:?}");
            let mut clouds = Clouds::default();
            let (_, most) = option.range();
            clouds.obey(CloudsCommand::Set(*option, most));
            for other in OPTIONS {
                let want = (other == *option).then_some(most);
                assert_eq!(clouds.get(other), want, "{option:?} {other:?}");
            }
        }
    }
}
