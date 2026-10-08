//! The stack splitter's rules: which stack it splits, and the amounts the slider and the typed
//! box give.

use dereth_client_contract::UiRequest;
use dereth_primitives::ObjectId;

/// The typed box's text-field id.
pub const FIELD: u32 = 80;

/// A split: the stack, how many of it a drag moves, and its size.
pub type Split = (ObjectId, u32, u32);

/// The split, given the stack selected now (its object and size, when it is a stack of more
/// than one) and the split as it was: a newly selected stack, or one whose size changed, is
/// split whole, and with no stack selected there is no split. The game is told of each change.
#[must_use]
pub fn follow(
    stack: Option<(ObjectId, u32)>,
    was: Option<Split>,
) -> (Option<Split>, Option<UiRequest>) {
    match (stack, was) {
        (Some((id, n)), Some((sid, _, max))) if sid == id && max == n => (was, None),
        (Some((id, n)), _) => (
            Some((id, n, n)),
            Some(UiRequest::StackSliderChanged { split: n, max: n }),
        ),
        (None, Some(_)) => (
            None,
            Some(UiRequest::StackSliderChanged { split: 1, max: 1 }),
        ),
        (None, None) => (None, None),
    }
}

/// The amount at `t` along the slider (`0` at its left, `1` at its right): from one to the whole
/// stack, to the nearest whole one.
#[must_use]
pub fn at(t: f32, max: u32) -> u32 {
    let max = max.max(1);
    #[allow(clippy::cast_precision_loss)]
    let span = (max - 1) as f32;
    #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
    // LINT-OK: a point along a stack's size, between zero and that size.
    let n = (t.clamp(0.0, 1.0) * span).round() as u32;
    (1 + n).min(max)
}

/// The amount typed, held to one through the whole stack; `None` while nothing is typed.
#[must_use]
pub fn typed(text: &str, max: u32) -> Option<u32> {
    let digits: String = text.chars().filter(char::is_ascii_digit).collect();
    if digits.is_empty() {
        return None;
    }
    // A number too long for the box is more than any stack.
    let n = digits.parse::<u32>().unwrap_or(u32::MAX);
    Some(n.clamp(1, max.max(1)))
}

#[cfg(test)]
mod tests {
    //! Behaviour: none (experimental Horizon interface)
    use super::*;

    const GEM: ObjectId = ObjectId(0x8000_0001);
    const ORE: ObjectId = ObjectId(0x8000_0002);

    #[test]
    fn a_new_stack_is_split_whole_and_the_game_is_told() {
        let (split, told) = follow(Some((GEM, 25)), None);
        assert_eq!(split, Some((GEM, 25, 25)));
        assert_eq!(
            told,
            Some(UiRequest::StackSliderChanged { split: 25, max: 25 })
        );
        // The same stack keeps its split; another, or the same one resized, starts whole.
        assert_eq!(
            follow(Some((GEM, 25)), Some((GEM, 7, 25))),
            (Some((GEM, 7, 25)), None)
        );
        assert_eq!(
            follow(Some((ORE, 3)), Some((GEM, 7, 25))).0,
            Some((ORE, 3, 3))
        );
        assert_eq!(
            follow(Some((GEM, 18)), Some((GEM, 7, 25))).0,
            Some((GEM, 18, 18))
        );
        // No stack: the split goes, and the game's slider is set to one.
        assert_eq!(
            follow(None, Some((GEM, 7, 25))),
            (
                None,
                Some(UiRequest::StackSliderChanged { split: 1, max: 1 })
            )
        );
        assert_eq!(follow(None, None), (None, None));
    }

    #[test]
    fn the_slider_runs_from_one_to_the_whole_stack() {
        assert_eq!(at(0.0, 25), 1);
        assert_eq!(at(1.0, 25), 25);
        assert_eq!(at(0.5, 25), 13);
        assert_eq!(at(-3.0, 25), 1);
        assert_eq!(at(7.0, 25), 25);
        assert_eq!(at(0.5, 1), 1);
    }

    #[test]
    fn a_typed_amount_is_held_to_one_through_the_whole_stack() {
        assert_eq!(typed("", 25), None);
        assert_eq!(typed("7", 25), Some(7));
        assert_eq!(typed("0", 25), Some(1));
        assert_eq!(typed("400", 25), Some(25));
        assert_eq!(typed("99999999999999", 25), Some(25));
        assert_eq!(typed("1x2", 25), Some(12));
    }
}
