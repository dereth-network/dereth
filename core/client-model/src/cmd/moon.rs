//! `@moon`, this client's own command for the moons' phases (CD-moon-command), and the moons'
//! calendar it overrides. Retail has neither.
//!
//! Each moon waxes and wanes over a month of its own on the game's calendar. The command forces
//! how much of either moon's face, or both, the Remastered sky draws lit, so a phase can be looked
//! at without waiting for it. It changes only what this client draws, and nothing is sent. This
//! module holds the calendar and the command's words: what a typed line asks for, what it has
//! asked for so far, and the sentence that answers.

/// Rez'arel's graphics object: the red moon.
pub const REZAREL: u32 = 0x0100_1F67;
/// Alb'arel's graphics object: the pale moon.
pub const ALBAREL: u32 = 0x0100_1F6A;
/// The game's days in its year.
pub const DAYS_PER_YEAR: f64 = 360.0;

/// One of the two moons.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Moon {
    /// Rez'arel, the red moon.
    Rezarel,
    /// Alb'arel, the pale moon.
    Albarel,
}

/// Both moons, in the order the answer names them.
pub const MOONS: [Moon; 2] = [Moon::Rezarel, Moon::Albarel];

impl Moon {
    /// The moon whose graphics object is `gfx`, if it is one of the two.
    #[must_use]
    pub const fn of(gfx: u32) -> Option<Self> {
        match gfx {
            REZAREL => Some(Self::Rezarel),
            ALBAREL => Some(Self::Albarel),
            _ => None,
        }
    }

    /// Its graphics object.
    #[must_use]
    pub const fn gfx(self) -> u32 {
        match self {
            Self::Rezarel => REZAREL,
            Self::Albarel => ALBAREL,
        }
    }

    /// Its name.
    #[must_use]
    pub const fn name(self) -> &'static str {
        match self {
            Self::Rezarel => "Rez'arel",
            Self::Albarel => "Alb'arel",
        }
    }

    /// The words `@moon` takes for it.
    #[must_use]
    pub const fn words(self) -> [&'static str; 3] {
        match self {
            Self::Rezarel => ["rezarel", "rez'arel", "rez"],
            Self::Albarel => ["albarel", "alb'arel", "alb"],
        }
    }

    /// The game's days it takes from full to full, and where in that it stood at the calendar's
    /// start.
    #[must_use]
    pub const fn month(self) -> (f64, f64) {
        match self {
            Self::Rezarel => (11.0, 0.0),
            Self::Albarel => (17.0, 0.37),
        }
    }

    /// How much of its face the calendar shows lit `days` into it: full at the start of each of
    /// its months, new halfway through.
    #[must_use]
    pub fn calendar_phase(self, days: f64) -> f32 {
        let (month, start) = self.month();
        let turn = (days / month + start).rem_euclid(1.0);
        #[allow(clippy::cast_possible_truncation)] // a share
        let lit =
            (0.5 * (1.0 + dereth_primitives::num::math::cos(std::f64::consts::TAU * turn))) as f32;
        lit
    }
}

/// The game's days since its calendar began, at `year`, `day` and `time_of_day`.
#[must_use]
pub fn calendar_days(year: u32, day: u32, time_of_day: f32) -> f64 {
    f64::from(year) * DAYS_PER_YEAR + f64::from(day) + f64::from(time_of_day)
}

/// What `@moon` has asked for: a phase for each moon forced, the calendar's for the others.
#[derive(Debug, Clone, Copy, PartialEq, Default)]
pub struct Moons([Option<f32>; MOONS.len()]);

impl Moons {
    /// The phase forced for `moon`, if one is.
    #[must_use]
    pub const fn get(&self, moon: Moon) -> Option<f32> {
        self.0[moon as usize]
    }

    /// How much of `moon`'s face is drawn lit `days` into the calendar: what is forced, or the
    /// calendar's.
    #[must_use]
    pub fn phase(&self, moon: Moon, days: f64) -> f32 {
        self.get(moon).unwrap_or_else(|| moon.calendar_phase(days))
    }

    /// Carry out `command`.
    pub fn obey(&mut self, command: MoonCommand) {
        match command {
            MoonCommand::Show => {}
            MoonCommand::Set(Some(moon), lit) => self.0[moon as usize] = Some(lit),
            MoonCommand::Set(None, lit) => self.0 = [Some(lit); MOONS.len()],
            MoonCommand::Unset(moon) => self.0[moon as usize] = None,
            MoonCommand::Reset => *self = Self::default(),
        }
    }
}

/// What a typed `@moon` asks for.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum MoonCommand {
    /// Say each moon's phase.
    Show,
    /// Draw this moon, or both, with this much of its face lit from now on.
    Set(Option<Moon>, f32),
    /// Return this moon to the calendar's phase.
    Unset(Moon),
    /// Return both to the calendar's.
    Reset,
}

/// The words for phases, with how much of the face each leaves lit.
pub const NAMED_PHASES: [(&str, f32); 5] = [
    ("new", 0.0),
    ("crescent", 0.25),
    ("half", 0.5),
    ("gibbous", 0.75),
    ("full", 1.0),
];

/// The word that returns to the calendar's.
pub const RESET_WORD: &str = "reset";

/// What `@moon` takes: the tail of every refusal.
pub const USAGE: &str = "@moon takes new, crescent, half, gibbous, full, a number from 0 to 1 or \
                         reset, alone for both moons or after rez or alb for one.";

/// The answer when there is no world, and so no sky, yet.
pub const NO_WORLD: &str = "The moons can be shown and set once the world is loaded.";

/// Read a typed `@moon`'s arguments. `Err` is the one line that says what was wrong.
///
/// # Errors
///
/// More than two words, a word that is not a moon or a phase, or a number outside 0 to 1.
pub fn parse(args: &[String]) -> Result<MoonCommand, String> {
    let moon_of = |word: &str| {
        MOONS
            .into_iter()
            .find(|m| m.words().iter().any(|w| w.eq_ignore_ascii_case(word)))
    };
    match args {
        [] => Ok(MoonCommand::Show),
        [word] if word.eq_ignore_ascii_case(RESET_WORD) => Ok(MoonCommand::Reset),
        [word] => match moon_of(word) {
            Some(_) => Err(format!("\"{word}\" needs a phase. {USAGE}")),
            None => phase(word).map(|lit| MoonCommand::Set(None, lit)),
        },
        [moon, word] => {
            let Some(moon) = moon_of(moon) else {
                return Err(format!("\"{moon}\" is not a moon. {USAGE}"));
            };
            if word.eq_ignore_ascii_case(RESET_WORD) {
                Ok(MoonCommand::Unset(moon))
            } else {
                phase(word).map(|lit| MoonCommand::Set(Some(moon), lit))
            }
        }
        _ => Err(format!("Too many words after @moon. {USAGE}")),
    }
}

/// How much of the face `word` leaves lit: a named phase or a number from 0 to 1.
fn phase(word: &str) -> Result<f32, String> {
    if let Some((_, lit)) = NAMED_PHASES
        .iter()
        .find(|(name, _)| name.eq_ignore_ascii_case(word))
    {
        return Ok(*lit);
    }
    match word.parse::<f32>() {
        // A NaN or an infinity is outside the range, so the one test refuses them too.
        Ok(lit) if (0.0..=1.0).contains(&lit) => Ok(lit.abs()),
        _ => Err(format!("\"{word}\" is not a phase. {USAGE}")),
    }
}

/// The answer to `@moon`: how much of each moon's face is drawn lit, `days` into the calendar
/// when the world's clock is known, each marked forced where it is.
#[must_use]
pub fn describe(moons: &Moons, days: Option<f64>) -> String {
    let each: Vec<String> = MOONS
        .iter()
        .map(|&m| match (moons.get(m), days) {
            (Some(lit), _) => format!("{} {lit} lit (forced)", m.name()),
            (None, Some(days)) => format!("{} {:.2} lit", m.name(), m.calendar_phase(days)),
            (None, None) => format!("{} the calendar's", m.name()),
        })
        .collect();
    format!("Moon phases: {}", each.join(", "))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn words(line: &str) -> Vec<String> {
        crate::cmd::interp::find_all_words(line)
    }

    /// Behaviour: chat.commands.moon-is-answered-by-this-client-and-never-sent
    #[test]
    fn phases_moons_and_reset_read_in_any_case() {
        use Moon as M;
        use MoonCommand as C;
        for (line, want) in [
            ("", C::Show),
            ("new", C::Set(None, 0.0)),
            ("FULL", C::Set(None, 1.0)),
            ("0.3", C::Set(None, 0.3)),
            ("rez half", C::Set(Some(M::Rezarel), 0.5)),
            ("Rez'arel crescent", C::Set(Some(M::Rezarel), 0.25)),
            ("ALBAREL 1", C::Set(Some(M::Albarel), 1.0)),
            ("alb gibbous", C::Set(Some(M::Albarel), 0.75)),
            ("alb reset", C::Unset(M::Albarel)),
            ("Reset", C::Reset),
        ] {
            assert_eq!(parse(&words(line)), Ok(want), "{line:?}");
        }
    }

    /// Behaviour: chat.commands.moon-is-answered-by-this-client-and-never-sent
    #[test]
    fn anything_else_is_refused_in_one_line_that_says_what_moon_takes() {
        for (line, names) in [
            ("waxing", "\"waxing\""),
            ("rez", "needs a phase"),
            ("sun full", "\"sun\""),
            ("rez 1.5", "\"1.5\""),
            ("alb -0.5", "\"-0.5\""),
            ("NaN", "\"NaN\""),
            ("rez full now", "Too many words"),
        ] {
            let refusal = parse(&words(line)).expect_err(line);
            assert!(refusal.contains(names), "{line}: {refusal}");
            assert!(refusal.ends_with(USAGE), "{line}: {refusal}");
            assert!(!refusal.contains('\n'), "one line: {refusal:?}");
        }
    }

    /// Behaviour: chat.commands.moon-is-answered-by-this-client-and-never-sent
    #[test]
    fn help_moon_says_each_form_the_command_takes() {
        let want = concat!(
            "@moon - Shows how much of each moon's face is lit.\n",
            "@moon new, crescent, half, gibbous, full - Forces both moons' phase.\n",
            "@moon <0 to 1> - Forces how much of both moons' faces is lit.\n",
            "@moon rez <phase> - Forces Rez'arel's phase alone.\n",
            "@moon alb <phase> - Forces Alb'arel's phase alone.\n",
            "@moon rez reset, @moon alb reset - Returns one moon to the calendar.\n",
            "@moon reset - Returns both to the calendar.\n",
            "These change the moons of the Remastered sky, which needs Better Lighting and an art \
             pack with pictures of the moons, with Retail Look off.\n",
        );
        let c = crate::cmd::CommandInterp::new();
        for name in ["moon", "@moon", "/moon"] {
            let lines = c.do_help(&[name.to_owned()]);
            assert_eq!(lines.len(), 2, "{name}: {lines:?}");
            assert_eq!(
                lines[1].0,
                format!("{}{want}", crate::cmd::help::FOR_MORE_INFORMATION),
                "{name}"
            );
        }
    }

    /// Behaviour: rendering.sky.moon-phases-asked-for-replace-the-calendars-until-reset
    #[test]
    fn a_forced_phase_replaces_the_calendars_until_reset() {
        use MoonCommand as C;
        let days = calendar_days(10, 4, 0.5);
        let mut moons = Moons::default();
        for m in MOONS {
            assert_eq!(moons.phase(m, days), m.calendar_phase(days));
        }
        moons.obey(C::Set(None, 0.0));
        assert_eq!(moons.phase(Moon::Rezarel, days), 0.0);
        assert_eq!(moons.phase(Moon::Albarel, days), 0.0);
        moons.obey(C::Set(Some(Moon::Albarel), 1.0));
        moons.obey(C::Unset(Moon::Rezarel));
        assert_eq!(
            moons.phase(Moon::Rezarel, days),
            Moon::Rezarel.calendar_phase(days)
        );
        assert_eq!(moons.phase(Moon::Albarel, days), 1.0);
        assert_eq!(
            describe(&moons, Some(days)),
            format!(
                "Moon phases: Rez'arel {:.2} lit, Alb'arel 1 lit (forced)",
                Moon::Rezarel.calendar_phase(days)
            )
        );
        moons.obey(C::Reset);
        assert_eq!(moons, Moons::default());
        assert_eq!(
            describe(&moons, None),
            "Moon phases: Rez'arel the calendar's, Alb'arel the calendar's"
        );
    }

    #[test]
    fn each_moon_waxes_and_wanes_over_a_month_of_its_own() {
        for moon in MOONS {
            let (month, _) = moon.month();
            let at = |d: f64| moon.calendar_phase(1_234.0 * DAYS_PER_YEAR + d);
            assert!(
                (at(0.0) - at(month)).abs() < 1e-4,
                "{moon:?} repeats each month"
            );
            let lits: Vec<f32> = (0..64).map(|i| at(month * f64::from(i) / 64.0)).collect();
            let (lo, hi) = lits
                .iter()
                .fold((1.0f32, 0.0f32), |(l, h), &v| (l.min(v), h.max(v)));
            assert!(
                lo < 0.01 && hi > 0.99,
                "{moon:?} runs new to full: {lo}..{hi}"
            );
        }
        let (rez, alb) = (Moon::Rezarel.month().0, Moon::Albarel.month().0);
        assert!((rez - alb).abs() > 1.0, "the two keep months of their own");
        assert_eq!(Moon::of(REZAREL), Some(Moon::Rezarel));
        assert_eq!(Moon::of(ALBAREL), Some(Moon::Albarel));
        assert_eq!(Moon::of(0x0100_0001), None);
    }
}
