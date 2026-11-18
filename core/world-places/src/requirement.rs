//! What a portal asks beyond a level range: of the character who uses it (a quest flag, a
//! player-killer status, an account), or of the world for it to be standing at all (an event, the
//! time of day, the moment after something made it).
//!
//! In the portal table each is a term, and a portal's terms are one field, separated by `;`:
//!
//! | Term | The portal… |
//! |---|---|
//! | `closed` | admits no character |
//! | `quest <name>` | admits a character holding the quest flag `<name>` |
//! | `no-pk`, `no-pklite`, `no-npk` | turns away player killers, lite player killers, non-player killers |
//! | `olthoi-only`, `no-olthoi` | admits only Olthoi characters, or none |
//! | `no-vitae` | turns away a character weakened by vitae |
//! | `account-age` | admits a character two weeks old, or of an account that old |
//! | `account <n>` | admits a character whose account's subscription is at least `<n>` |
//! | `advocate` | admits an Advocate |
//! | `event <name> on` or `off` | stands while the event `<name>` runs; `on` when it runs unless switched off |
//! | `night`, `day` | stands at night, or by day, in the world's time |
//! | `temporary` | stands for a while after something makes it, then goes |
//!
//! A name has each `\` and `;` escaped with a `\`. A reader keeps a term it does not know as
//! [`Requirement::Other`], so a later builder's table still reads.

use std::fmt;

/// One thing a portal asks.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum Requirement {
    /// No character may use it.
    Closed,
    /// The character must hold this quest flag.
    Quest(String),
    /// Player killers may not use it.
    NoPk,
    /// Lite player killers may not use it.
    NoPkLite,
    /// Non-player killers may not use it.
    NoNpk,
    /// Only Olthoi characters may use it.
    OlthoiOnly,
    /// Olthoi characters may not use it.
    NoOlthoi,
    /// A character weakened by vitae may not use it.
    NoVitae,
    /// The character, or its account, must be two weeks old.
    AccountAge,
    /// The account's subscription must be at least this.
    Account(u32),
    /// The character must be an Advocate.
    Advocate,
    /// It stands only while the named event runs; whether the event runs unless switched off.
    Event { name: String, on: bool },
    /// It stands only at night.
    Night,
    /// It stands only by day.
    Day,
    /// It stands for a while after something makes it, then goes.
    Temporary,
    /// A term this reader does not know, as written.
    Other(String),
}

impl Requirement {
    /// Whether it is about the world (whether the portal is standing), not the character.
    #[must_use]
    pub const fn of_the_world(&self) -> bool {
        matches!(
            self,
            Self::Event { .. } | Self::Night | Self::Day | Self::Temporary
        )
    }

    /// Reads one term.
    #[must_use]
    pub fn parse(term: &str) -> Self {
        let (word, rest) = term.split_once(' ').unwrap_or((term, ""));
        match (word, rest) {
            ("closed", "") => Self::Closed,
            ("quest", name) if !name.is_empty() => Self::Quest(unescape(name)),
            ("no-pk", "") => Self::NoPk,
            ("no-pklite", "") => Self::NoPkLite,
            ("no-npk", "") => Self::NoNpk,
            ("olthoi-only", "") => Self::OlthoiOnly,
            ("no-olthoi", "") => Self::NoOlthoi,
            ("no-vitae", "") => Self::NoVitae,
            ("account-age", "") => Self::AccountAge,
            ("account", n) if n.parse::<u32>().is_ok() => Self::Account(n.parse().unwrap_or(0)),
            ("advocate", "") => Self::Advocate,
            ("event", rest) => match rest.rsplit_once(' ') {
                Some((name, "on")) if !name.is_empty() => Self::Event {
                    name: unescape(name),
                    on: true,
                },
                Some((name, "off")) if !name.is_empty() => Self::Event {
                    name: unescape(name),
                    on: false,
                },
                _ => Self::Other(term.to_owned()),
            },
            ("night", "") => Self::Night,
            ("day", "") => Self::Day,
            ("temporary", "") => Self::Temporary,
            _ => Self::Other(term.to_owned()),
        }
    }
}

impl fmt::Display for Requirement {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Closed => f.write_str("closed"),
            Self::Quest(name) => write!(f, "quest {}", escape(name)),
            Self::NoPk => f.write_str("no-pk"),
            Self::NoPkLite => f.write_str("no-pklite"),
            Self::NoNpk => f.write_str("no-npk"),
            Self::OlthoiOnly => f.write_str("olthoi-only"),
            Self::NoOlthoi => f.write_str("no-olthoi"),
            Self::NoVitae => f.write_str("no-vitae"),
            Self::AccountAge => f.write_str("account-age"),
            Self::Account(n) => write!(f, "account {n}"),
            Self::Advocate => f.write_str("advocate"),
            Self::Event { name, on } => {
                write!(
                    f,
                    "event {} {}",
                    escape(name),
                    if *on { "on" } else { "off" }
                )
            }
            Self::Night => f.write_str("night"),
            Self::Day => f.write_str("day"),
            Self::Temporary => f.write_str("temporary"),
            Self::Other(term) => f.write_str(term),
        }
    }
}

/// The field of `requirements`: `-` for none, else the terms separated by `;`.
#[must_use]
pub fn write_field(requirements: &[Requirement]) -> String {
    if requirements.is_empty() {
        return "-".to_owned();
    }
    let terms: Vec<String> = requirements.iter().map(ToString::to_string).collect();
    terms.join(";")
}

/// Reads a field [`write_field`] wrote.
#[must_use]
pub fn read_field(field: &str) -> Vec<Requirement> {
    if field == "-" || field.is_empty() {
        return Vec::new();
    }
    let mut out = Vec::new();
    let mut term = String::new();
    let mut chars = field.chars();
    while let Some(c) = chars.next() {
        match c {
            '\\' => {
                term.push(c);
                if let Some(n) = chars.next() {
                    term.push(n);
                }
            }
            ';' => out.push(Requirement::parse(&std::mem::take(&mut term))),
            c => term.push(c),
        }
    }
    out.push(Requirement::parse(&term));
    out
}

fn escape(name: &str) -> String {
    let mut out = String::with_capacity(name.len());
    for c in name.chars() {
        match c {
            '\\' | ';' => {
                out.push('\\');
                out.push(c);
            }
            '\t' | '\n' | '\r' => out.push(' '),
            c => out.push(c),
        }
    }
    out
}

fn unescape(name: &str) -> String {
    let mut out = String::with_capacity(name.len());
    let mut chars = name.chars();
    while let Some(c) = chars.next() {
        if c == '\\' {
            if let Some(n) = chars.next() {
                out.push(n);
            }
        } else {
            out.push(c);
        }
    }
    out
}

#[cfg(test)]
mod tests {
    //! Behaviour: none (the portal table's requirement field, written and read back).
    use super::*;

    #[test]
    fn every_term_reads_back_as_written_and_an_unknown_one_is_kept() {
        let all = vec![
            Requirement::Closed,
            Requirement::Quest("Gharu'n; \\Temple".into()),
            Requirement::NoPk,
            Requirement::NoPkLite,
            Requirement::NoNpk,
            Requirement::OlthoiOnly,
            Requirement::NoOlthoi,
            Requirement::NoVitae,
            Requirement::AccountAge,
            Requirement::Account(3),
            Requirement::Advocate,
            Requirement::Event {
                name: "Snowreap on Ice".into(),
                on: false,
            },
            Requirement::Event {
                name: "Fair".into(),
                on: true,
            },
            Requirement::Night,
            Requirement::Day,
            Requirement::Temporary,
            Requirement::Other("tide high".into()),
        ];
        let field = write_field(&all);
        assert!(!field.contains('\t'));
        assert_eq!(read_field(&field), all);
        assert_eq!(write_field(&[]), "-");
        assert_eq!(read_field("-"), Vec::new());
        assert_eq!(
            read_field("event"),
            vec![Requirement::Other("event".into())]
        );
        assert!(Requirement::Night.of_the_world() && !Requirement::NoPk.of_the_world());
    }
}
