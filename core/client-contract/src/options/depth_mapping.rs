//! How much relief an art pack gives the world's own pictures: an experimental menu after the
//! effects' boxes, drawn inside the better lighting. It is registered with them
//! ([`super::fidelity::register`]); the profile keeps a level the player chose, and leaves out one
//! still [`DEFAULT`] ([`super::fidelity::left_out_of_save`]).

use crate::options::names::fidelity::DEPTH_MAPPING;
use crate::view::PrefValue;

/// The menu's caption.
pub const CAPTION: &str = "Depth Mapping";

/// What the menu does, for the row's tooltip.
pub const NOTE: &str = "How much relief walls, floors and roofs take from an art pack, on the \
                        original pictures. Low gives bump maps and a damp shine in dungeons; \
                        Medium adds parallax and self-shadows; High deepens them. The higher \
                        levels cost the most. It changes at once.";

/// The level with no relief.
pub const OFF: i32 = 0;

/// The level until the player changes it: off, as every effect is until it is asked for.
pub const DEFAULT: i32 = OFF;

/// The levels the menu offers, least first.
pub const LEVELS: [&str; 4] = ["Off", "Low", "Medium", "High"];

/// The greatest level.
pub const MOST: i32 = 3;

/// Whether `name` is the menu.
#[must_use]
pub fn is_depth_mapping(name: &str) -> bool {
    name.eq_ignore_ascii_case(DEPTH_MAPPING)
}

/// The level a word stands for: its name, its number, or the box it once was, ticked as
/// [`MOST`]. `None` for another name or another word, which leaves the option as it was.
#[must_use]
pub fn parse_value(name: &str, raw: &str) -> Option<i32> {
    if !is_depth_mapping(name) {
        return None;
    }
    let word = raw.trim();
    if let Some(level) = LEVELS.iter().position(|l| l.eq_ignore_ascii_case(word)) {
        return i32::try_from(level).ok();
    }
    if word.eq_ignore_ascii_case("true") {
        return Some(MOST);
    }
    if word.eq_ignore_ascii_case("false") {
        return Some(OFF);
    }
    word.parse().ok().filter(|v| (OFF..=MOST).contains(v))
}

/// The words for a value: the level's name. `None` for any other name.
#[must_use]
pub fn convert_to_string(name: &str, v: &PrefValue) -> Option<String> {
    if !is_depth_mapping(name) {
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
    if !is_depth_mapping(name) {
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
    //! Behaviour: none (experimental hi-fi art packs)
    use super::*;

    #[test]
    fn the_level_is_read_by_name_by_number_and_from_the_box_it_once_was() {
        assert_eq!(parse_value(DEPTH_MAPPING, " medium "), Some(2));
        assert_eq!(parse_value(DEPTH_MAPPING, "3"), Some(3));
        assert_eq!(parse_value(DEPTH_MAPPING, "True"), Some(MOST));
        assert_eq!(parse_value(DEPTH_MAPPING, "False"), Some(OFF));
        assert_eq!(parse_value(DEPTH_MAPPING, "4"), None);
        assert_eq!(parse_value("Fidelity.Sky", "High"), None);
        assert_eq!(
            convert_to_string(DEPTH_MAPPING, &PrefValue::Int(1)).as_deref(),
            Some("Low")
        );
        let rows = choice_rows(DEPTH_MAPPING).expect("the menu's rows");
        assert_eq!(rows.len(), LEVELS.len());
        assert_eq!((rows[3].label.as_str(), rows[3].value), ("High", MOST));
    }
}
