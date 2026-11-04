//! How far the lamps light the world and show their glow, Light Draw Distance: an experimental
//! menu after the effects' boxes, a share of the distance they reach with nothing changed, drawn
//! inside the better lighting. It is registered with them ([`super::fidelity::register`]); the
//! profile keeps a share the player chose, and leaves out one still at its default
//! ([`super::fidelity::left_out_of_save`]).

use crate::options::names::fidelity::LIGHT_DRAW_DISTANCE;
use crate::view::PrefValue;

/// The menu's caption.
pub const CAPTION: &str = "Light Draw Distance";

/// What the menu does, for the row's tooltip.
pub const NOTE: &str = "How far lamps, flames and an art pack's lights light the world and show \
                        their glow, against the distance they reach at 1x. They fade out toward \
                        the end of it rather than going out at a line. It changes at once.";

/// The shares the menu offers, in percent, nearest first.
pub const CHOICES: [u32; 8] = [50, 75, 100, 150, 200, 300, 400, 500];

/// The share until the player changes it: the whole distance.
pub const DEFAULT: i32 = 100;

/// The least share.
pub const LEAST: u32 = 50;

/// The greatest share.
pub const MOST: u32 = 500;

/// Whether `name` is the menu.
#[must_use]
pub fn is_light_distance(name: &str) -> bool {
    name.eq_ignore_ascii_case(LIGHT_DRAW_DISTANCE)
}

/// A share in percent as a multiple: `0.5x`, `1x`, `1.5x`.
fn times(share: u32) -> String {
    let whole = share / 100;
    let part = share % 100;
    if part == 0 {
        format!("{whole}x")
    } else if part.is_multiple_of(10) {
        format!("{whole}.{}x", part / 10)
    } else {
        format!("{whole}.{part:02}x")
    }
}

/// `share`'s words, marked where it is the default.
fn label(share: u32) -> String {
    if i32::try_from(share).is_ok_and(|s| s == DEFAULT) {
        format!("{} (Default)", times(share))
    } else {
        times(share)
    }
}

/// The value a word stands for: a multiple with or without its mark, or a percent with or without
/// its sign, between [`LEAST`] and [`MOST`]. `None` for another name or another word, which leaves
/// the option as it was.
#[must_use]
pub fn parse_value(name: &str, raw: &str) -> Option<i32> {
    if !is_light_distance(name) {
        return None;
    }
    let word = raw.trim();
    let word = word.split(" (").next().unwrap_or(word).trim_end();
    let share = if let Some(many) = word.strip_suffix(['x', 'X']) {
        let many: f32 = many.trim_end().parse().ok()?;
        #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
        // LINT-OK: a menu's multiple, a small positive number, brought to percent and rounded.
        let share = (many.is_finite() && many > 0.0).then_some((many * 100.0 + 0.5) as u32)?;
        share
    } else {
        word.trim_end_matches('%').trim_end().parse().ok()?
    };
    i32::try_from(share)
        .ok()
        .filter(|_| (LEAST..=MOST).contains(&share))
}

/// The words for a value: the share as a multiple. `None` for any other name.
#[must_use]
pub fn convert_to_string(name: &str, v: &PrefValue) -> Option<String> {
    if !is_light_distance(name) {
        return None;
    }
    match v {
        PrefValue::Int(i) => u32::try_from(*i).ok().map(label),
        _ => None,
    }
}

/// The shares the menu offers ([`CHOICES`]), each as its words; `None` for any other name.
#[must_use]
pub fn choice_rows(name: &str) -> Option<Vec<super::store::Choice>> {
    if !is_light_distance(name) {
        return None;
    }
    Some(
        CHOICES
            .iter()
            .map(|&share| super::store::Choice {
                label: label(share),
                value: i32::try_from(share).unwrap_or(DEFAULT),
            })
            .collect(),
    )
}

#[cfg(test)]
mod tests {
    //! Behaviour: none (experimental hi-fi lighting)
    use super::*;

    #[test]
    fn the_menu_offers_half_to_five_times_the_distance_with_the_whole_its_default() {
        let rows = choice_rows(LIGHT_DRAW_DISTANCE).expect("the menu");
        let shares: Vec<i32> = rows.iter().map(|c| c.value).collect();
        assert_eq!(shares, [50, 75, 100, 150, 200, 300, 400, 500]);
        assert_eq!(rows[2].label, "1x (Default)");
        assert_eq!(rows[3].label, "1.5x");
        assert!(choice_rows("Fidelity.GodRays").is_none());
    }

    #[test]
    fn a_share_is_read_with_or_without_its_sign_and_refused_outside_the_menu_range() {
        let read = |raw| parse_value(LIGHT_DRAW_DISTANCE, raw);
        assert_eq!(read(" 300 % "), Some(300));
        assert_eq!(read("1.5x"), Some(150));
        assert_eq!(read("1x (Default)"), Some(100));
        assert_eq!(read("6x"), None);
        assert_eq!(read("49"), None);
        assert_eq!(read("501"), None);
        assert_eq!(read("far"), None);
        assert_eq!(parse_value("Fidelity.GodRays", "100"), None);
        assert_eq!(
            convert_to_string(LIGHT_DRAW_DISTANCE, &PrefValue::Int(150)).as_deref(),
            Some("1.5x")
        );
    }
}
