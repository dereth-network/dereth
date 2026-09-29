//! The stack splitter: a text box and a slider over one integer.
//!
//! The stack splitter is the only place where a text box and a slider are two views of one
//! integer; both clamp to `1 … max split size` and both emit `StackSliderChanged`.

use dereth_primitives::num::to_i32_f64;
use dereth_ui::ElementId;

use crate::view::UiRequest;

/// The stack-size entry box, whose input filter accepts numbers only.
pub const ENTRY_BOX: ElementId = ElementId(0x1000_01A3);
/// The stack-size slider, a `Scrollbar`.
pub const SLIDER: ElementId = ElementId(0x1000_01A4);

/// The split size and the max split size.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Splitter {
    pub split_size: u32,
    pub max_split_size: u32,
}

/// The client's two unconditional lines before it looks at the selection:
/// split size = 1, max split size = 1.
impl Default for Splitter {
    fn default() -> Self {
        Self {
            split_size: 1,
            max_split_size: 1,
        }
    }
}

impl Splitter {
    /// A splitter over a stack of `max`, starting at the whole stack.
    #[must_use]
    pub fn new(max: u32) -> Self {
        Self {
            split_size: max.max(1),
            max_split_size: max.max(1),
        }
    }

    /// The toolbar's split slider, on element message `0x0A`, takes the message's
    /// unsigned **p1**, not attribute `0x86`. Retail computes `p1 * max * -0.001f` in double
    /// precision, truncates it toward zero to an integer, subtracts that from 1, and clamps the
    /// unsigned result into `1 … max`. The constant is the `f32` `-0.001` (bits `0xBA83126F`),
    /// widened to 53-bit precision.
    /// The scrollbar's own position report produces truncated thousandths of the float position.
    /// Thus the left endpoint is 1, right is the whole stack; there is no rounding-to-nearest.
    pub fn on_slider(&mut self, position_thousandths: u32) -> UiRequest {
        let scaled = f64::from(position_thousandths)
            * f64::from(self.max_split_size)
            * f64::from(f32::from_bits(0xba83_126f));
        let taken = u32::from_ne_bytes(to_i32_f64(scaled).to_ne_bytes());
        let value = 1_u32.wrapping_sub(taken);
        self.split_size = if value > self.max_split_size {
            self.max_split_size
        } else {
            value.max(1)
        };
        self.notice()
    }

    /// Text-box message `0x2F`: parse the text as an unsigned integer (`wcstoul`), clamp it into
    /// `1 … max split size`, write it back if it was clamped, set the slider's float attribute
    /// `0x86` to `split size / max split size` and send the same notice.
    ///
    /// Returns the request and the slider position to write.
    pub fn on_text(&mut self, text: &str) -> (UiRequest, f32) {
        let raw = parse_stack_quantity(text);
        self.split_size = raw.clamp(1, self.max_split_size.max(1));
        (self.notice(), self.slider_position())
    }

    /// The slider's float attribute `0x86`: `split size / max split size`.
    #[must_use]
    pub fn slider_position(&self) -> f32 {
        if self.max_split_size == 0 {
            return 0.0;
        }
        #[allow(clippy::cast_precision_loss)]
        {
            self.split_size as f32 / self.max_split_size as f32
        }
    }

    fn notice(&self) -> UiRequest {
        UiRequest::StackSliderChanged {
            split: self.split_size,
            max: self.max_split_size,
        }
    }
}

/// `wcstoul(text, NULL, 0)`, over the number-only filter's ASCII input domain, as the MSVCR70
/// `wcstoul` behaves: whitespace, optional sign, base 0, stopping at the first invalid digit,
/// unsigned overflow and final two's-complement negation. In this older CRT even overflow is
/// negated for a negative input.
/// The caller ignores errno/endptr. This is not a general locale-dependent wide-character CRT.
pub(crate) fn parse_stack_quantity(text: &str) -> u32 {
    let mut text = text.trim_start_matches(|c: char| c.is_ascii_whitespace());
    let negative = text.starts_with('-');
    if text.starts_with(['+', '-']) {
        text = &text[1..];
    }
    let (radix, digits) =
        if let Some(rest) = text.strip_prefix("0x").or_else(|| text.strip_prefix("0X")) {
            (16_u32, rest)
        } else if text.starts_with('0') {
            (8, text)
        } else {
            (10, text)
        };
    let mut value = 0_u32;
    for c in digits.chars() {
        let Some(digit) = c.to_digit(radix) else {
            break;
        };
        value = value.saturating_mul(radix).saturating_add(digit);
    }
    if negative {
        value.wrapping_neg()
    } else {
        value
    }
}

// ---------------------------------------------------------------------------------------------
// The gate
// ---------------------------------------------------------------------------------------------

/// The stack count at which `Toolbar` shows the splitter at all.
///
/// The selection-changed handler reaches its splitter block having *already*
/// hidden both elements and reset the pair to `1 / 1`, and then branches on one value only: the
/// selected object's stack size. Below 2 it takes the health / mana path and both stay hidden;
/// otherwise it seeds split and maximum from the stack (see `Splitter::for_selection`), writes the
/// box text and the slider position, and shows both.
///
/// **The gate is the object's live stack size, and nothing else** — it is not `WeenieType`, not
/// the max stack size, and not any bitfield bit. The branch never looks at `WeenieType`, so a
/// single arrow (stack size absent, therefore 0) and a stack of exactly one both fall on the
/// *creature* side of the same test. `\[verified\]`
pub const MIN_SPLITTABLE_STACK: u32 = 2;

/// The gate itself: may the split marker be shown for a selection of this stack size?
///
/// `stack_size` is the stack size with an **absent** field read as 0, which is what
/// retail holds when the desc did not carry the optional: the field is left zero-initialised and
/// the selection-changed handler reads it as a plain unsigned value.
#[must_use]
pub fn shows_split_widget(stack_size: u32) -> bool {
    stack_size >= MIN_SPLITTABLE_STACK
}

/// The selection-changed handler's re-entry test, taken from its **other** caller.
///
/// The item-attributes-changed notice is the one that arrives when the selected
/// object's own attributes moved, and it does not re-run the handler unconditionally: only for the
/// selected object, only if the client knows it, and only when its stack size (0 read as 1)
/// differs from the max split size.
///
/// So the seeding runs again exactly when `max(1, stack size) != max split size`. That is what
/// makes a per-frame poll safe rather than merely convenient: a drag moves the split size and
/// leaves the max split size alone, so a player mid-drag is never re-seeded out from under, while a stack the
/// server grew or shrank is.
#[must_use]
pub fn wants_reseed(stack_size: u32, max_split_size: u32) -> bool {
    stack_size.max(1) != max_split_size
}

impl Splitter {
    /// The client's splitter block, as one value.
    ///
    /// `None` is the not-a-stack arm — both elements hidden and the pair left at `1 / 1`, which is
    /// [`Splitter::default`]. `Some` is the stack arm, seeded at the whole stack.
    ///
    /// **Not modelled: the vendor arm.** The client skips seeding the split from the stack size when
    /// a vendor is open, the item's container is that vendor, and the item's type mask
    /// `& 0xDC41CB0` is non-zero, so an item being *sold by an open vendor* opens at a split of 1
    /// rather than at the whole stack. The open vendor's id has no counterpart in this rebuild
    /// (`dereth_client_model::vendor` holds the list, not the open panel's id), so the left half is always
    /// false here and every stack opens at the whole stack. **The widget's visibility is
    /// unaffected** — the gate above sits outside that expression.
    #[must_use]
    pub fn for_selection(stack_size: u32) -> Option<Self> {
        shows_split_widget(stack_size).then(|| Self::new(stack_size))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Oracle: §2.2's two element ids.
    #[test]
    fn the_splitter_is_the_documented_text_box_and_slider() {
        assert_eq!(ENTRY_BOX, ElementId(0x1000_01A3));
        assert_eq!(SLIDER, ElementId(0x1000_01A4));
        let spec = crate::panels::catalogue::spec("Toolbar").unwrap();
        assert!(spec.children.iter().any(|c| c.id == ENTRY_BOX));
        assert!(spec.children.iter().any(|c| c.id == SLIDER));
    }

    /// Retail's slider arithmetic: both endpoints and discriminating interior positions.
    #[test]
    fn the_slider_consumes_integer_thousandths_from_one_to_the_whole_stack() {
        let mut s = Splitter::new(100);
        assert_eq!(
            s.on_slider(0),
            UiRequest::StackSliderChanged { split: 1, max: 100 },
            "p1=0 is one item"
        );
        s.on_slider(250);
        assert_eq!(s.split_size, 26);
        s.on_slider(500);
        assert_eq!(s.split_size, 51);
        s.on_slider(1000);
        assert_eq!(s.split_size, 100);
        s.on_slider(2000);
        assert_eq!(s.split_size, 100, "unsigned high clamp");
        let mut s = Splitter::new(9988);
        s.on_slider(123);
        assert_eq!(
            s.split_size, 1229,
            "message quantization precedes toolbar multiplication"
        );
        let mut s = Splitter::new(99);
        s.on_slider(5);
        assert_eq!(
            s.split_size, 1,
            "the float-to-int step truncates, does not round"
        );
    }

    /// Oracle: §2.2's text-box rule — clamp into `1 … max_split_size`, then drive the slider.
    #[test]
    fn the_text_box_clamps_into_range_and_writes_the_slider_back() {
        let mut s = Splitter::new(20);
        let (r, pos) = s.on_text("7");
        assert_eq!(r, UiRequest::StackSliderChanged { split: 7, max: 20 });
        assert!((pos - 0.35).abs() < 1e-6);
        // Over the maximum clamps down…
        let (r, pos) = s.on_text("999");
        assert_eq!(r, UiRequest::StackSliderChanged { split: 20, max: 20 });
        assert_eq!(pos, 1.0);
        // …and zero, empty and non-numeric all clamp up to one, because `wcstoul` yields 0.
        for t in ["0", "", "abc"] {
            let (r, _) = s.on_text(t);
            assert_eq!(
                r,
                UiRequest::StackSliderChanged { split: 1, max: 20 },
                "{t:?}"
            );
        }
        // `wcstoul` stops at the first non-digit.
        s.on_text("12x");
        assert_eq!(s.split_size, 12);
    }

    #[test]
    fn stack_text_obeys_the_imported_32_bit_crt_not_rust_decimal_parse() {
        for (text, want) in [
            ("0010", 8),
            ("09", 0),
            ("0109", 8),
            ("0x10rest", 16),
            ("0Xf", 15),
            (" \t\r\n+12tail", 12),
            ("-4", u32::MAX - 3),
            ("-0x10", u32::MAX - 15),
            ("4294967295", u32::MAX),
            ("4294967296", u32::MAX),
            ("9999999999999999999999999", u32::MAX),
            ("-4294967296", 1),
            ("", 0),
            ("x12", 0),
            ("0x", 0),
            ("+", 0),
        ] {
            assert_eq!(parse_stack_quantity(text), want, "{text:?}");
        }
    }

    /// The split marker is gated on the stack count and on nothing else.
    #[test]
    fn the_split_marker_is_gated_on_the_stack_count_and_on_nothing_else() {
        assert_eq!(MIN_SPLITTABLE_STACK, 2);
        // The creature side of the branch: absent (0) and a stack of exactly one.
        assert!(
            !shows_split_widget(0),
            "an absent stack size is 0 and 0 < 2"
        );
        assert!(
            !shows_split_widget(1),
            "a stack of one is not splittable either"
        );
        assert_eq!(Splitter::for_selection(0), None);
        assert_eq!(Splitter::for_selection(1), None);
        // The stack side, from the first value that clears the edge upwards.
        for n in [2u32, 3, 20, 250, 10_000, u32::from(u16::MAX)] {
            assert!(shows_split_widget(n), "{n}");
            let s = Splitter::for_selection(n).unwrap_or_else(|| panic!("{n} is a stack"));
            assert_eq!(
                s,
                Splitter {
                    split_size: n,
                    max_split_size: n
                },
                "the stack arm opens at the whole stack"
            );
            // …and the seeded splitter really splits: the whole range is reachable from the box.
            let mut s = s;
            s.on_text("2");
            assert_eq!(
                s.split_size, 2,
                "{n}: a seeded splitter takes a smaller quantity"
            );
        }
        // The not-a-stack arm leaves the client's reset value, `split_size = max_split_size = 1`.
        assert_eq!(
            Splitter::default(),
            Splitter {
                split_size: 1,
                max_split_size: 1
            }
        );
    }

    /// Oracle: the item-attributes-changed notice re-runs the selection handler when the stack
    /// size, with 0 read as 1, differs from `max_split_size`.
    ///
    /// Reading 0 as 1 is load-bearing and is the reason this is not simply
    /// `stack_size != max_split_size`: a non-stack selection settles at `max_split_size == 1` and
    /// must **stop** re-seeding, or the box would be rewritten on every frame of a poll.
    #[test]
    fn the_reseed_test_settles_on_both_arms_and_survives_a_drag() {
        // A creature: 0 -> 1, which is what the reset left behind, so it settles at once.
        assert!(
            !wants_reseed(0, 1),
            "a non-stack settles against the reset value"
        );
        assert!(!wants_reseed(1, 1));
        assert!(
            wants_reseed(0, 20),
            "a creature selected after a stack does re-seed"
        );
        // A stack: re-seeds until `max_split_size` agrees, then stops.
        assert!(wants_reseed(20, 1));
        assert!(!wants_reseed(20, 20));
        // A drag moves `split_size` only, so the test still says "no".
        let mut s = Splitter::new(20);
        s.on_slider(500);
        assert_ne!(s.split_size, 20, "the drag moved the quantity");
        assert!(
            !wants_reseed(20, s.max_split_size),
            "and did not arm a re-seed"
        );
        // The server growing the stack does arm one.
        assert!(wants_reseed(25, s.max_split_size));
    }

    /// Oracle: §7's rebuild note — "both clamp to `1 … max_split_size` and **both emit
    /// `StackSliderChanged`**", i.e. the two views round-trip to the same integer.
    #[test]
    fn the_two_views_agree_on_the_same_integer() {
        let mut s = Splitter::new(40);
        s.on_slider(250);
        let split = s.split_size;
        let (_, pos) = s.on_text(&split.to_string());
        let mut t = Splitter::new(40);
        t.split_size = 0;
        t.on_text(&split.to_string());
        assert_eq!(t.split_size, split);
        assert!((pos - t.slider_position()).abs() < 1e-6);
    }
}
