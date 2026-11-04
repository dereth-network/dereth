//! Horizon's status bars: the colours of its health, stamina and mana bars, and the size of the
//! numbers on its parameter bar.
//!
//! These are this client's own options, not retail ones. Only the Horizon interface draws the
//! bars they change, so only its Client Options page shows them, under Display, before the
//! Performance Panel. [`STATUS_BAR_COLORS`] holds a value of [`Colors`], registered as an
//! unsigned enumeration whose choice labels are literal text: Horizon's own colours at first, or
//! the original game's red, amber and blue. [`LARGE_STATUS_BAR_NUMBERS`], on at first, draws the
//! parameter bar's numbers large; off, they are smaller and stand on the line of the bars'
//! titles. Both are saved in the profile like any other, and Horizon reads them as it draws, so a
//! change applies at once.

use crate::view::PrefValue;

/// `[Display] StatusBarColors`: the colours of Horizon's health, stamina and mana bars.
pub const STATUS_BAR_COLORS: &str = crate::options::names::DISPLAY_STATUS_BAR_COLORS;

/// `[Display] LargeStatusBarNumbers`: whether Horizon's parameter bar draws its numbers large.
pub const LARGE_STATUS_BAR_NUMBERS: &str = crate::options::names::DISPLAY_LARGE_STATUS_BAR_NUMBERS;

/// The colours option's caption, literal text like its choices.
pub const COLORS_CAPTION: &str = "Status Bar Colors";

/// The numbers option's caption, literal text: no string table has one.
pub const NUMBERS_CAPTION: &str = "Large Status Bar Numbers";

/// The status bars' colours.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default)]
pub enum Colors {
    /// Horizon's own: health green, stamina gold and mana pink.
    #[default]
    Horizon,
    /// The original game's: health red, stamina amber and mana blue.
    Classic,
}

impl Colors {
    /// Every choice, in the order the options list them.
    pub const ALL: [Self; 2] = [Self::Horizon, Self::Classic];

    /// The preference value.
    #[must_use]
    pub const fn value(self) -> i32 {
        match self {
            Self::Horizon => 0,
            Self::Classic => 1,
        }
    }

    /// The choice a preference value names; anything else is Horizon's own.
    #[must_use]
    pub const fn from_value(v: i32) -> Self {
        match v {
            1 => Self::Classic,
            _ => Self::Horizon,
        }
    }

    /// Its name, as the options list it and the preferences file holds it.
    #[must_use]
    pub const fn label(self) -> &'static str {
        match self {
            Self::Horizon => "Horizon",
            Self::Classic => "Classic",
        }
    }

    /// The colours the store holds now: Horizon's own where it holds none.
    #[must_use]
    pub fn chosen() -> Self {
        match super::store::inq_value(STATUS_BAR_COLORS) {
            Some(PrefValue::Int(v)) => Self::from_value(v),
            _ => Self::Horizon,
        }
    }
}

/// Whether the parameter bar draws its numbers large: yes unless the store holds the option off,
/// so a store that has no value for it draws them large.
#[must_use]
pub fn large_numbers() -> bool {
    !matches!(
        super::store::inq_value(LARGE_STATUS_BAR_NUMBERS),
        Some(PrefValue::Bool(false))
    )
}

/// The value a preferences-file string stands for; `None` for another name or a word it cannot
/// read. A plain number is read as the value.
#[must_use]
pub fn parse_value(name: &str, raw: &str) -> Option<i32> {
    if !name.eq_ignore_ascii_case(STATUS_BAR_COLORS) {
        return None;
    }
    if let Ok(v) = raw.trim().parse::<i32>() {
        return Some(Colors::from_value(v).value());
    }
    let word = raw.trim();
    Colors::ALL
        .iter()
        .find(|c| c.label().eq_ignore_ascii_case(word))
        .map(|c| c.value())
}

/// How the store's save writes the value; `None` for any other name.
#[must_use]
pub fn convert_to_string(name: &str, v: &PrefValue) -> Option<String> {
    if !name.eq_ignore_ascii_case(STATUS_BAR_COLORS) {
        return None;
    }
    match v {
        PrefValue::Int(i) => Some(Colors::from_value(*i).label().to_owned()),
        _ => None,
    }
}

/// The choices an options list shows; `None` for any other name.
#[must_use]
pub fn choice_rows(name: &str) -> Option<Vec<super::store::Choice>> {
    if !name.eq_ignore_ascii_case(STATUS_BAR_COLORS) {
        return None;
    }
    Some(
        Colors::ALL
            .iter()
            .map(|c| super::store::Choice {
                label: c.label().to_owned(),
                value: c.value(),
            })
            .collect(),
    )
}

/// Register both options in the option value store: Horizon's own colours, and large numbers.
pub fn register() -> usize {
    usize::from(super::store::register_preference(
        STATUS_BAR_COLORS,
        PrefValue::Int(Colors::Horizon.value()),
        super::store::DataType::UInt,
    )) + usize::from(super::store::register_preference(
        LARGE_STATUS_BAR_NUMBERS,
        PrefValue::Bool(true),
        super::store::DataType::Bool,
    ))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::options::interface::Interface;
    use crate::options::{sheet, store};
    use crate::persist::preferences::UserPreferences;

    /// The Client Options page of `face`, as the preferences of its rows in order.
    fn client_page(face: Interface) -> Vec<&'static str> {
        sheet::rows_for(sheet::PageId::Client, face)
            .filter_map(sheet::Row::preference)
            .collect()
    }

    /// Horizon's Client Options page has Status Bar Colors and then Large Status Bar Numbers
    /// under Display, just before the Performance Panel, Defaults putting back Horizon's own
    /// colours and the large numbers; the classic and modern interfaces' pages have neither.
    ///
    /// Behaviour: hud.status-bars.horizon-s-page-has-the-two-options-under-display
    #[test]
    fn the_two_rows_are_on_horizon_s_page_alone_just_before_the_performance_panel() {
        use crate::options::config::PrefValueConst::{Bool, Int};
        store::init();
        let page = client_page(Interface::Horizon);
        let at = |p: &str| page.iter().position(|o| *o == p);
        let panel = at(crate::options::performance::PERFORMANCE_PANEL).expect("the panel's row");
        assert_eq!(at(STATUS_BAR_COLORS), Some(panel - 2), "{page:?}");
        assert_eq!(at(LARGE_STATUS_BAR_NUMBERS), Some(panel - 1), "{page:?}");
        let display = sheet::headings_for(sheet::PageId::Client, Interface::Horizon)
            .find(|(h, _)| h.title == "Display")
            .expect("the Display heading");
        assert!(display
            .1
            .iter()
            .any(|r| r.preference() == Some(STATUS_BAR_COLORS)));
        for face in [Interface::Modern, Interface::Classic] {
            let page = client_page(face);
            assert!(!page.contains(&STATUS_BAR_COLORS), "{face:?}");
            assert!(!page.contains(&LARGE_STATUS_BAR_NUMBERS), "{face:?}");
        }

        let colors = sheet::row_of_preference(sheet::PageId::Client, STATUS_BAR_COLORS).unwrap();
        assert_eq!(colors.caption, "Status Bar Colors");
        assert_eq!(colors.value, sheet::Value::Menu(STATUS_BAR_COLORS));
        let numbers =
            sheet::row_of_preference(sheet::PageId::Client, LARGE_STATUS_BAR_NUMBERS).unwrap();
        assert_eq!(numbers.caption, "Large Status Bar Numbers");
        assert_eq!(numbers.value, sheet::Value::Check(LARGE_STATUS_BAR_NUMBERS));
        let defaults = sheet::defaults(sheet::PageId::Client, Interface::Horizon);
        assert!(defaults.contains(&(STATUS_BAR_COLORS, Int(0))));
        assert!(defaults.contains(&(LARGE_STATUS_BAR_NUMBERS, Bool(true))));
        assert_eq!(
            store::choice_rows(STATUS_BAR_COLORS)
                .unwrap()
                .into_iter()
                .map(|c| (c.label, c.value))
                .collect::<Vec<_>>(),
            [("Horizon".to_owned(), 0), ("Classic".to_owned(), 1)]
        );
        store::clear();
    }

    /// Both are at their first values in a fresh store and in one that does not hold them;
    /// changed, they are saved in the profile's `[Display]` section, in words, and a profile read
    /// back brings them back.
    ///
    /// Behaviour: hud.status-bars.horizon-s-page-has-the-two-options-under-display
    #[test]
    fn the_options_start_at_horizon_s_look_and_a_saved_profile_brings_a_change_back() {
        store::clear();
        assert_eq!(Colors::chosen(), Colors::Horizon);
        assert!(
            large_numbers(),
            "a store without the option draws them large"
        );
        store::init();
        assert_eq!(store::inq_value(STATUS_BAR_COLORS), Some(PrefValue::Int(0)));
        assert_eq!(
            store::inq_value(LARGE_STATUS_BAR_NUMBERS),
            Some(PrefValue::Bool(true))
        );
        assert_eq!(register(), 0, "registered once");

        assert!(store::set_value(STATUS_BAR_COLORS, PrefValue::Int(1)));
        assert!(store::set_value(
            LARGE_STATUS_BAR_NUMBERS,
            PrefValue::Bool(false)
        ));
        assert_eq!(Colors::chosen(), Colors::Classic);
        assert!(!large_numbers());
        let text = store::save().to_text();
        let display = text
            .split("[Display]\r\n")
            .nth(1)
            .and_then(|s| s.split("\r\n[").next())
            .unwrap_or_else(|| panic!("no [Display] section: {text}"));
        assert!(display.contains("StatusBarColors=Classic\r\n"), "{display}");
        assert!(
            display.contains("LargeStatusBarNumbers=False\r\n"),
            "{display}"
        );

        store::init();
        assert_eq!(Colors::chosen(), Colors::Horizon);
        assert!(large_numbers());
        let ini = UserPreferences::parse(&text).expect("the saved profile parses");
        store::load(&ini);
        assert_eq!(Colors::chosen(), Colors::Classic);
        assert!(!large_numbers());
        store::clear();
    }

    /// The colours read their words and numbers and write their word.
    ///
    /// Behaviour: none (the option's own spelling).
    #[test]
    fn the_colors_read_their_words_and_numbers_and_write_their_word() {
        for (word, value) in [
            ("Horizon", Some(0)),
            (" horizon ", Some(0)),
            ("CLASSIC", Some(1)),
            ("1", Some(1)),
            ("7", Some(0)),
            ("retail", None),
        ] {
            assert_eq!(
                parse_value("display.statusbarcolors", word),
                value,
                "{word}"
            );
        }
        assert_eq!(parse_value("Display.FullScreen", "Classic"), None);
        assert_eq!(
            convert_to_string(STATUS_BAR_COLORS, &PrefValue::Int(1)).as_deref(),
            Some("Classic")
        );
        assert_eq!(convert_to_string("Other.Name", &PrefValue::Int(1)), None);
        assert_eq!(choice_rows("Other.Name"), None);
    }
}
