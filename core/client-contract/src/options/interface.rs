//! Which interface the client shows: the modern interface, the classic one the game had before its
//! 2005 redesign, or Horizon, the client's own.
//!
//! This is this client's own option, not a retail one. [`INTERFACE`] holds a value of
//! [`Interface`]. It is registered in the option value store beside the retail options
//! ([`crate::options::store::init`]) as an unsigned enumeration whose choice labels are literal
//! text, so any front end lists it from [`crate::options::store::choice_rows`] and writes it with
//! [`crate::view::UiRequest::SetPreference`], as it does a retail option. The client switches live.
//! The classic interface draws from the early-2005 portal: without those files it is refused, the
//! current interface stays, and the client says why ([`REQUIRES_CLASSIC_FILES`]). Horizon draws
//! from its own art, which the host hands the client: while the art is still loading the choice
//! waits, the current interface stays and the client says so ([`HORIZON_ART_LOADING`]), and the
//! choice is followed once the art is in; on a host that cannot get it, Horizon is refused as the
//! classic interface is ([`REQUIRES_HORIZON_ART`]).

use crate::view::PrefValue;

/// `[UI] Interface`: the interface the client shows.
pub const INTERFACE: &str = crate::options::names::INTERFACE;

/// The option's caption, literal text like its choices.
pub const CAPTION: &str = "Interface";

/// What [`INTERFACE`] says when the classic interface cannot be shown for want of its files.
pub const REQUIRES_CLASSIC_FILES: &str = "The classic interface requires classic DATs";

/// What it says when the host cannot draw the classic interface's text.
pub const REQUIRES_FONTS: &str = "The classic interface needs the system's fonts";

/// What it says when the host cannot get the Horizon interface's art.
pub const REQUIRES_HORIZON_ART: &str = "The Horizon interface's art is not available";

/// What it says when Horizon is chosen while its art is still loading.
pub const HORIZON_ART_LOADING: &str =
    "The Horizon interface's art is still loading: it is shown once it has loaded";

/// The interfaces.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default)]
pub enum Interface {
    /// The modern interface.
    #[default]
    Modern,
    /// The interface before the 2005 redesign.
    Classic,
    /// Horizon, the client's own interface.
    Horizon,
}

impl Interface {
    /// Every interface, in the order the options list them.
    pub const ALL: [Self; 3] = [Self::Modern, Self::Classic, Self::Horizon];

    /// The preference value.
    #[must_use]
    pub const fn value(self) -> i32 {
        match self {
            Self::Modern => 0,
            Self::Classic => 1,
            Self::Horizon => 2,
        }
    }

    /// The interface a preference value names; anything else is the modern one.
    #[must_use]
    pub const fn from_value(v: i32) -> Self {
        match v {
            1 => Self::Classic,
            2 => Self::Horizon,
            _ => Self::Modern,
        }
    }

    /// Its name, as the options list it and the preferences file holds it.
    #[must_use]
    pub const fn label(self) -> &'static str {
        match self {
            Self::Modern => "Modern",
            Self::Classic => "Classic",
            Self::Horizon => "Horizon",
        }
    }

    /// The interface a stored preference value names.
    #[must_use]
    pub fn of(v: &PrefValue) -> Self {
        match v {
            PrefValue::Int(i) => Self::from_value(*i),
            _ => Self::Modern,
        }
    }

    /// The interface the store holds now.
    #[must_use]
    pub fn chosen() -> Self {
        super::store::inq_value(INTERFACE).map_or(Self::Modern, |v| Self::of(&v))
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

/// Register the option in the option value store, holding the modern interface.
pub fn register() -> usize {
    usize::from(super::store::register_preference(
        INTERFACE,
        PrefValue::Int(Interface::Modern.value()),
        super::store::DataType::UInt,
    ))
}

#[cfg(test)]
mod tests {
    //! Behaviour: none (an option's own values; the switch it drives is tested where it happens).
    use super::*;

    #[test]
    fn the_interface_reads_its_file_words_and_numbers_and_writes_its_word() {
        for word in ["modern", "Modern", " MODERN "] {
            assert_eq!(parse_value("ui.interface", word), Some(0));
        }
        assert_eq!(parse_value("UI.Interface", "classic"), Some(1));
        assert_eq!(parse_value("UI.Interface", "horizon"), Some(2));
        for word in ["Retail", "retail", " RETAIL ", "unknown"] {
            assert_eq!(parse_value("UI.Interface", word), None);
        }
        for (word, value) in [("1", 1), ("0", 0), ("2", 2), ("9", 0), ("-1", 0), ("+1", 1)] {
            assert_eq!(parse_value("UI.Interface", word), Some(value));
        }
        assert_eq!(parse_value("Render.Sky", "classic"), None);
        assert_eq!(
            convert_to_string("UI.Interface", &PrefValue::Int(1)).as_deref(),
            Some("Classic")
        );
        let rows = choice_rows("UI.Interface").expect("interface choices");
        assert_eq!(
            rows.iter()
                .map(|r| (r.label.as_str(), r.value))
                .collect::<Vec<_>>(),
            [("Modern", 0), ("Classic", 1), ("Horizon", 2)]
        );
    }

    #[test]
    fn stored_modern_round_trips_and_retired_names_leave_the_active_interface_unchanged() {
        use crate::{options::store, persist::preferences::UserPreferences};
        store::init();
        register();
        store::set_value(INTERFACE, PrefValue::Int(1));
        for word in ["Retail", "retail", " RETAIL "] {
            let ini = UserPreferences::parse(&format!("[UI]\nInterface={word}\n")).unwrap();
            assert_eq!(store::load(&ini), (0, 1));
            assert_eq!(store::inq_value(INTERFACE), Some(PrefValue::Int(1)));
        }
        let ini = UserPreferences::parse("[UI]\nInterface=mOdErN\n").unwrap();
        assert_eq!(store::load(&ini), (1, 0));
        assert_eq!(store::inq_value(INTERFACE), Some(PrefValue::Int(0)));
        let saved = store::save().to_text();
        assert!(saved.contains("Interface=Modern"));
        store::set_value(INTERFACE, PrefValue::Int(1));
        let restored = UserPreferences::parse(&saved).unwrap();
        assert_eq!(store::load(&restored).1, 0);
        assert_eq!(store::inq_value(INTERFACE), Some(PrefValue::Int(0)));
    }
}
