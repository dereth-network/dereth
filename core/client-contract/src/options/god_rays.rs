//! How strongly sunlight shows in the air, God Rays: an experimental menu after the effects'
//! boxes, drawn inside the better lighting. It is registered with them
//! ([`super::fidelity::register`]); the profile keeps a level the player chose, and leaves out one
//! still [`DEFAULT`] ([`super::fidelity::left_out_of_save`]).

use crate::options::names::fidelity::GOD_RAYS;
use crate::view::PrefValue;

/// The menu's caption.
pub const CAPTION: &str = "God Rays";

/// What the menu does, for the row's tooltip.
pub const NOTE: &str = "Sunlight shows in the air in shafts where walls, roofs and leaves break \
                        it, most at dawn and dusk, in fog and under trees. Low is a light touch; \
                        High is the boldest. It changes at once.";

/// The level with no shafts.
pub const OFF: i32 = 0;

/// The lightest level, and the one a ticked box stands for.
pub const LOW: i32 = 1;

/// The level until the player changes it: off, as every effect is until it is asked for.
pub const DEFAULT: i32 = OFF;

/// The levels the menu offers, least first.
pub const LEVELS: [&str; 4] = ["Off", "Low", "Medium", "High"];

/// The greatest level.
pub const MOST: i32 = 3;

/// Whether `name` is the menu.
#[must_use]
pub fn is_god_rays(name: &str) -> bool {
    name.eq_ignore_ascii_case(GOD_RAYS)
}

/// The level a word stands for: its name, its number, or the box it once was, ticked as
/// [`LOW`]. `None` for another name or another word, which leaves the option as it was.
#[must_use]
pub fn parse_value(name: &str, raw: &str) -> Option<i32> {
    if !is_god_rays(name) {
        return None;
    }
    let word = raw.trim();
    if let Some(level) = LEVELS.iter().position(|l| l.eq_ignore_ascii_case(word)) {
        return i32::try_from(level).ok();
    }
    if word.eq_ignore_ascii_case("true") {
        return Some(LOW);
    }
    if word.eq_ignore_ascii_case("false") {
        return Some(OFF);
    }
    word.parse().ok().filter(|v| (OFF..=MOST).contains(v))
}

/// The words for a value: the level's name. `None` for any other name.
#[must_use]
pub fn convert_to_string(name: &str, v: &PrefValue) -> Option<String> {
    if !is_god_rays(name) {
        return None;
    }
    match v {
        PrefValue::Int(i) => usize::try_from(*i)
            .ok()
            .and_then(|i| LEVELS.get(i))
            .map(|l| (*l).to_owned()),
        _ => None,
    }
}

/// The levels the menu offers, each as its words; `None` for any other name.
#[must_use]
pub fn choice_rows(name: &str) -> Option<Vec<super::store::Choice>> {
    if !is_god_rays(name) {
        return None;
    }
    Some(
        (OFF..=MOST)
            .zip(LEVELS)
            .map(|(value, label)| super::store::Choice {
                label: label.to_owned(),
                value,
            })
            .collect(),
    )
}

#[cfg(test)]
mod tests {
    //! Behaviour: none (experimental hi-fi lighting)
    use super::*;

    #[test]
    fn the_levels_read_by_name_or_number_and_the_old_box_reads_as_low() {
        assert_eq!(parse_value(GOD_RAYS, "high"), Some(MOST));
        assert_eq!(parse_value(GOD_RAYS, "1"), Some(1));
        assert_eq!(parse_value(GOD_RAYS, "true"), Some(LOW));
        assert_eq!(parse_value(GOD_RAYS, "false"), Some(OFF));
        assert_eq!(parse_value(GOD_RAYS, "9"), None);
        assert_eq!(parse_value("Fidelity.Other", "high"), None);
        assert_eq!(
            convert_to_string(GOD_RAYS, &PrefValue::Int(LOW)).as_deref(),
            Some("Low")
        );
        assert_eq!(choice_rows(GOD_RAYS).map(|c| c.len()), Some(4));
    }
}
