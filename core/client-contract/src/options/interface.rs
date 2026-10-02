//! Which interface the client shows: the retail interface, or the classic one the game had before
//! its 2005 redesign.
//!
//! This is this client's own option, not a retail one. [`INTERFACE`] holds a value of
//! [`Interface`]. It is registered in the option value store beside the retail options
//! ([`crate::options::store::init`]) as an unsigned enumeration whose choice labels are literal
//! text, so any front end lists it from [`crate::options::store::choice_rows`] and writes it with
//! [`crate::view::UiRequest::SetPreference`], as it does a retail option. The client switches live.
//! The classic interface draws from the early-2005 portal: without those files it is refused, the
//! current interface stays, and the client says why ([`REQUIRES_LEGACY_FILES`]).

use crate::view::PrefValue;

/// `[UI] Interface`: the interface the client shows.
pub const INTERFACE: &str = "UI.Interface";

/// The option's caption, literal text like its choices.
pub const CAPTION: &str = "Interface";

/// What [`INTERFACE`] says when the classic interface cannot be shown for want of its files.
pub const REQUIRES_LEGACY_FILES: &str = "The classic interface requires legacy DATs";

/// What it says when the host cannot draw the classic interface's text.
pub const REQUIRES_FONTS: &str = "The classic interface needs the system's fonts";

/// The two interfaces.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default)]
pub enum Interface {
    /// The end-of-retail interface.
    #[default]
    Retail,
    /// The interface before the 2005 redesign.
    Classic,
}

impl Interface {
    /// Both, in the order the options list them.
    pub const ALL: [Self; 2] = [Self::Retail, Self::Classic];

    /// The preference value.
    #[must_use]
    pub const fn value(self) -> i32 {
        match self {
            Self::Retail => 0,
            Self::Classic => 1,
        }
    }

    /// The interface a preference value names; anything else is the retail one.
    #[must_use]
    pub const fn from_value(v: i32) -> Self {
        match v {
            1 => Self::Classic,
            _ => Self::Retail,
        }
    }

    /// Its name, as the options list it and the preferences file holds it.
    #[must_use]
    pub const fn label(self) -> &'static str {
        match self {
            Self::Retail => "Retail",
            Self::Classic => "Classic",
        }
    }

    /// The interface a stored preference value names.
    #[must_use]
    pub fn of(v: &PrefValue) -> Self {
        match v {
            PrefValue::Int(i) => Self::from_value(*i),
            _ => Self::Retail,
        }
    }

    /// The interface the store holds now.
    #[must_use]
    pub fn chosen() -> Self {
        super::store::inq_value(INTERFACE).map_or(Self::Retail, |v| Self::of(&v))
    }
}

/// The value a preferences-file string stands for; `None` for another name or a word it cannot
/// read. A plain number is read as the value.
#[must_use]
pub fn parse_value(name: &str, raw: &str) -> Option<i32> {
    if !name.eq_ignore_ascii_case(INTERFACE) {
        return None;
    }
    if let Ok(v) = raw.trim().parse::<i32>() {
        return Some(Interface::from_value(v).value());
    }
    let word = raw.trim();
    Interface::ALL
        .iter()
        .find(|i| i.label().eq_ignore_ascii_case(word))
        .map(|i| i.value())
}

/// How the store's save writes the value; `None` for any other name.
#[must_use]
pub fn convert_to_string(name: &str, v: &PrefValue) -> Option<String> {
    if !name.eq_ignore_ascii_case(INTERFACE) {
        return None;
    }
    Some(Interface::of(v).label().to_string())
}

/// The choices an options list shows; `None` for any other name.
#[must_use]
pub fn choice_rows(name: &str) -> Option<Vec<super::store::Choice>> {
    if !name.eq_ignore_ascii_case(INTERFACE) {
        return None;
    }
    Some(
        Interface::ALL
            .iter()
            .map(|i| super::store::Choice {
                label: i.label().to_string(),
                value: i.value(),
            })
            .collect(),
    )
}

/// Register the option in the option value store, holding the retail interface.
pub fn register() -> usize {
    usize::from(super::store::register_preference(
        INTERFACE,
        PrefValue::Int(Interface::Retail.value()),
        super::store::DataType::UInt,
    ))
}

#[cfg(test)]
mod tests {
    //! Behaviour: none (an option's own values; the switch it drives is tested where it happens).
    use super::*;

    #[test]
    fn the_interface_reads_its_file_words_and_numbers_and_writes_its_word() {
        assert_eq!(parse_value("UI.Interface", "classic"), Some(1));
        assert_eq!(parse_value("ui.interface", "Retail"), Some(0));
        assert_eq!(parse_value("UI.Interface", "1"), Some(1));
        assert_eq!(parse_value("UI.Interface", "9"), Some(0));
        assert_eq!(parse_value("UI.Interface", "modern"), None);
        assert_eq!(parse_value("Render.Sky", "classic"), None);
        assert_eq!(
            convert_to_string("UI.Interface", &PrefValue::Int(1)).as_deref(),
            Some("Classic")
        );
        assert_eq!(choice_rows("UI.Interface").map(|r| r.len()), Some(2));
    }
}
