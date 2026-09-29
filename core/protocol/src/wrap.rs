//! The wrap-aware sequence comparisons.
//!
//! The protocol carries sequence numbers at three widths and every one of them is compared with a
//! half-range window rather than with `>`. Keep the 8-bit and 16-bit wrap comparisons exactly as
//! written: naive `>` comparisons break after 256 property updates on one property.

/// The time stamper's acceptance rule, byte-wide.
///
/// The client writes it as: accept when `|new − old| < 0x80 ? new >= old : new <= old`, where the
/// subtraction is on the *unsigned bytes*. Equal always accepts, which is why this is
/// `not_older` and not `newer` — a repeated stamp is not a rejection.
#[must_use]
pub fn not_older_u8(new: u8, old: u8) -> bool {
    let diff = new.abs_diff(old);
    if diff < 0x80 {
        new >= old
    } else {
        new <= old
    }
}

/// The 16-bit form. Strictly newer.
#[must_use]
pub fn newer_u16(lhs: u16, rhs: u16) -> bool {
    let d = lhs.wrapping_sub(rhs);
    d != 0 && d < 0x8000
}

/// The 32-bit form used by `SequenceGate`'s stamps. Strictly newer.
#[must_use]
pub fn newer_u32(lhs: u32, rhs: u32) -> bool {
    let d = lhs.wrapping_sub(rhs);
    d != 0 && d < 0x8000_0000
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Oracle: `docs/networking/messages/00-dispatch-and-queues.md` §5, per-property sequencing.
    /// The rule must accept and reject correctly across a wrap.
    #[test]
    fn the_eight_bit_rule_accepts_and_rejects_across_a_wrap() {
        // Ordinary forward motion.
        assert!(not_older_u8(5, 4));
        assert!(!not_older_u8(4, 5));
        // A repeat is accepted: the rule is "not older", not "newer".
        assert!(not_older_u8(4, 4));
        // Across the wrap: 0x02 after 0xFE is two steps forward, not 252 back.
        assert!(not_older_u8(0x02, 0xFE));
        assert!(!not_older_u8(0xFE, 0x02));
        // At exactly half the range the |diff| < 0x80 test fails, so the second arm decides.
        assert!(
            !not_older_u8(0x80, 0x00),
            "|diff| == 0x80 takes the new <= old arm"
        );
        assert!(not_older_u8(0x00, 0x80));
        assert!(
            not_older_u8(0x7F, 0x00),
            "|diff| == 0x7F stays in the first arm"
        );
    }

    #[test]
    fn the_sixteen_and_thirty_two_bit_forms_agree_in_shape() {
        assert!(newer_u16(1, 0xFFFF));
        assert!(!newer_u16(0xFFFF, 1));
        assert!(!newer_u16(7, 7));
        assert!(newer_u32(1, 0xFFFF_FFFF));
        assert!(!newer_u32(0xFFFF_FFFF, 1));
        assert!(!newer_u32(7, 7));
        // Asymmetric at exactly half the period, in both widths.
        assert!(!newer_u16(0x8000, 0));
        assert!(!newer_u32(0x8000_0000, 0));
    }
}
