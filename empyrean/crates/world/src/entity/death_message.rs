// Ported from ACE (ACEmulator), AGPL-3.0: Source/ACE.Server/Entity/DeathMessage.cs
//! Port of `Source/ACE.Server/Entity/DeathMessage.cs`.
//!
//! ACE's instances are the static tables in `Strings` (`entity::strings`), so the fields are
//! `&'static str` and the type is `Copy`.

// ACE: DeathMessage
/// The three texts of a death: to the killer, to the victim, and to everyone nearby. `{0}` is the
/// victim's name and `{1}` the killer's (`string.Format`).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct DeathMessage {
    // ACE: DeathMessage.Killer
    pub killer: &'static str,
    // ACE: DeathMessage.Victim
    pub victim: &'static str,

    // ACE: DeathMessage.Broadcast
    pub broadcast: &'static str,
}

impl DeathMessage {
    // ACE: DeathMessage.DeathMessage
    #[must_use]
    pub const fn new(killer: &'static str, victim: &'static str, broadcast: &'static str) -> Self {
        Self {
            killer,
            victim,
            broadcast,
        }
    }
}

/// Not ACE: `string.Format(format, args)` for the `{n}` items these texts use (no alignment or
/// format strings; `{{`/`}}` escapes).
///
/// # Panics
/// On an item index past `args` (C#: `FormatException`).
#[must_use]
pub fn string_format(format: &str, args: &[&str]) -> String {
    let mut out = String::with_capacity(format.len() + 16);
    let mut chars = format.chars().peekable();
    while let Some(c) = chars.next() {
        match c {
            '{' if chars.peek() == Some(&'{') => {
                chars.next();
                out.push('{');
            }
            '}' if chars.peek() == Some(&'}') => {
                chars.next();
                out.push('}');
            }
            '{' => {
                let mut index = String::new();
                for d in chars.by_ref() {
                    if d == '}' {
                        break;
                    }
                    index.push(d);
                }
                let i: usize = index
                    .trim()
                    .parse()
                    .expect("FormatException: bad format item");
                out.push_str(args.get(i).expect("FormatException: index out of range"));
            }
            _ => out.push(c),
        }
    }
    out
}
