//! How finely the volumetric clouds are drawn, Cloud Detail: a menu after the effects' boxes, Off,
//! Low, Medium or High, drawn inside the better lighting. It is registered with them
//! ([`super::fidelity::register`]); the profile keeps a level the player chose, and leaves out one
//! still [`DEFAULT`] ([`super::fidelity::left_out_of_save`]).
//!
//! The stored value is 0 at Low, the clouds as they were before the menu, so a capture that never
//! names it draws them so; Off is stored after High. The menu starts at Medium, as the Medium
//! preset has it.

use crate::options::names::fidelity::CLOUD_DETAIL;
use crate::view::PrefValue;

/// The menu's caption.
pub const CAPTION: &str = "Cloud Detail";

/// What the menu does, for the row's tooltip.
pub const NOTE: &str = "How finely the Remastered sky's volumetric clouds are drawn. Medium and \
                        High step through them two and four times as finely, smoothing their \
                        edges at more cost; Off draws retail's flat cloud deck. It changes at \
                        once.";

/// The level the clouds were drawn at before the menu, stored as 0.
pub const LOW: i32 = 0;

/// Twice Low's steps.
pub const MEDIUM: i32 = 1;

/// Four times Low's steps.
pub const HIGH: i32 = 2;

/// Retail's flat cloud deck in place of the volumetric clouds.
pub const OFF: i32 = 3;

/// The level until the player changes it: Medium, as the Medium preset has it.
pub const DEFAULT: i32 = MEDIUM;

/// The greatest stored value.
pub const MOST: i32 = OFF;

/// The levels the menu offers, least first, each with the value it is stored as.
pub const LEVELS: [(&str, i32); 4] = [
    ("Off", OFF),
    ("Low", LOW),
    ("Medium", MEDIUM),
    ("High", HIGH),
];

/// Whether `name` is the menu.
#[must_use]
pub fn is_cloud_detail(name: &str) -> bool {
    name.eq_ignore_ascii_case(CLOUD_DETAIL)
}

/// The stored value a word stands for: a level's name, or a stored value's number. `None` for
/// another name or another word, which leaves the option as it was.
#[must_use]
pub fn parse_value(name: &str, raw: &str) -> Option<i32> {
    if !is_cloud_detail(name) {
        return None;
    }
    let word = raw.trim();
    if let Some((_, v)) = LEVELS.iter().find(|(l, _)| l.eq_ignore_ascii_case(word)) {
        return Some(*v);
    }
    word.parse().ok().filter(|v| (0..=MOST).contains(v))
}

/// The words for a value: the level's name. `None` for any other name.
#[must_use]
pub fn convert_to_string(name: &str, v: &PrefValue) -> Option<String> {
    if !is_cloud_detail(name) {
        return None;
    }
    match v {
        PrefValue::Int(i) => LEVELS
            .iter()
            .find(|(_, v)| v == i)
            .map(|(l, _)| (*l).to_owned()),
        _ => None,
    }
}

/// The levels the menu offers, each as its words; `None` for any other name.
#[must_use]
pub fn choice_rows(name: &str) -> Option<Vec<super::store::Choice>> {
    if !is_cloud_detail(name) {
        return None;
    }
    Some(
        LEVELS
            .iter()
            .map(|(label, value)| super::store::Choice {
                label: (*label).to_owned(),
                value: *value,
            })
            .collect(),
    )
}

/// How many times the cloud steps knob's count the clouds are drawn with at the stored `value`;
/// `None` at Off.
#[must_use]
pub const fn step_factor(value: u32) -> Option<u32> {
    match value {
        0 => Some(1),
        1 => Some(2),
        2 => Some(4),
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    //! Behaviour: none (experimental hi-fi clouds)
    use super::*;

    #[test]
    fn the_levels_read_by_name_or_stored_number_and_low_is_stored_as_zero() {
        assert_eq!(parse_value(CLOUD_DETAIL, "high"), Some(HIGH));
        assert_eq!(parse_value(CLOUD_DETAIL, "Off"), Some(OFF));
        assert_eq!(parse_value(CLOUD_DETAIL, "0"), Some(LOW));
        assert_eq!(parse_value(CLOUD_DETAIL, "9"), None);
        assert_eq!(parse_value("Fidelity.Other", "high"), None);
        assert_eq!(LOW, 0);
        assert_eq!(
            convert_to_string(CLOUD_DETAIL, &PrefValue::Int(MEDIUM)).as_deref(),
            Some("Medium")
        );
        let rows = choice_rows(CLOUD_DETAIL).expect("the menu's rows");
        let labels: Vec<&str> = rows.iter().map(|c| c.label.as_str()).collect();
        assert_eq!(labels, ["Off", "Low", "Medium", "High"]);
    }

    #[test]
    fn low_medium_and_high_draw_one_two_and_four_times_the_steps_and_off_none() {
        let at = |v: i32| step_factor(u32::try_from(v).unwrap());
        assert_eq!(at(LOW), Some(1));
        assert_eq!(at(MEDIUM), Some(2));
        assert_eq!(at(HIGH), Some(4));
        assert_eq!(at(OFF), None);
    }
}
