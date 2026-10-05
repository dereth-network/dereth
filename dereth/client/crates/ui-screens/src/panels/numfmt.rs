//! Digit grouping — the client's own, not Rust's.
//!
//! Sources: the client's integer-to-string helper, the language-info record it reads its
//! grouping size, separator and negative format from, and the experience system's separate
//! number formatter.
//!
//! # The client has two number formatters and the panels use the first one
//!
//! Every number a stat-management header or footer shows is written as a **`StringInfo`
//! variable** identified by entries such as the total-experience string id, and
//! the string-info renderer walks each variable through its
//! value formatter. Integer variables use the long-integer variant; its
//! whole body delegates to the numeric formatter, and that
//! function reads **every** formatting decision from the language-info record:
//!
//! * numeric base
//! * decimal digits
//! * leading zero
//! * numeral table
//! * grouping size
//! * grouping separator
//! * decimal separator
//! * negative-number format
//!
//! The language-info record is a **database object** fetched with
//! enum value **1** in group **4**, type `0x30`, so the separator is a
//! shipped datum and never a literal in a call site.
//!
//! The **second** formatter is [`super::statmgmt::xp_to_string`], which does not go
//! through language-info at all: it formats `"%I64d"` and hands the result to
//! `GetNumberFormatA` with an explicit `NUMBERFMTA`. That one is already
//! implemented as [`super::statmgmt::xp_to_string`] and its only caller is the luminance line.
//!
//! # Two independent pins, and they agree
//!
//! * the experience formatter's `NUMBERFMTA`: `NumDigits = 0`, `LeadingZero = 0`,
//!   `Grouping = 3`, `lpDecimalSep = "."`, `lpThousandSep = ","`, `NegativeOrder = 1`.
//!
//! * the shipped language-info record: grouping size 3, grouping separator `","` (U+002C),
//!   decimal separator `"."`, negative-number format `"-%s"`, base 10.
//!
//! Two readings that could have disagreed. **Neither is a hard-coded comma**: the value used at
//! runtime is the dat's, and the `NUMBERFMTA` pair below is the fallback used only when no asset
//! source is installed, which is a headless-test condition and not a shipping one.

pub use dereth_presentation::numfmt::*;

/// Read the shipped language-info grouping rule out of `env` and memoise it for [`shipped`].
///
/// The host calls this where it gives its UI the asset environment (the one place the dat
/// becomes available), and a test calls it after installing its own. A record that cannot be
/// read memoises the fallback, as a missing one always has.
pub fn prime(env: &crate::env::Env) {
    let g = read_language_info(env).unwrap_or_else(fallback);
    set_shipped(g);
}

fn read_language_info(env: &crate::env::Env) -> Option<Grouping> {
    use dereth_assets::Decode;
    let did = env.did_by_enum(LANGUAGE_INFO_GROUP, LANGUAGE_INFO_ENUM)?;
    env.with_assets(|assets| {
        let bytes = assets.read(did).ok()?;
        let li = dereth_assets::LanguageInfo::decode_payload(did, &bytes).ok()?;
        Some(Grouping {
            size: li.grouping_size,
            separator: li.grouping_separator,
            negative_format: li.negative_number_format,
        })
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn english() -> Grouping {
        Grouping {
            size: XPTOSTRING_GROUPING,
            separator: XPTOSTRING_THOUSAND_SEP.to_owned(),
            negative_format: "-%s".to_owned(),
        }
    }

    /// Oracle: the client numeric formatter's grouping loop, with a grouping size of 3 and a
    /// grouping separator of `","` — the pair the shipped language-info carries and the pair
    /// the experience formatter's `NUMBERFMTA` uses.
    ///
    /// **The separator is pinned as a literal here** and nowhere else in this module's own
    /// arithmetic: [`group`] takes it as data. A test that read it back through [`shipped`]
    /// could not detect a wrong constant (the stated testability rule, *"a test that reads a constant
    /// through the same symbol it writes it through"*), so the number below is written out.
    #[test]
    fn the_separator_is_a_comma_every_three_digits_and_the_boundaries_are_the_clients() {
        let g = english();
        assert_eq!(
            XPTOSTRING_THOUSAND_SEP, ",",
            "the thousands separator the client ships"
        );
        assert_eq!(XPTOSTRING_GROUPING, 3, "NUMBERFMTA::Grouping is 3");
        assert_eq!(group(0, &g), "0");
        assert_eq!(group(1, &g), "1");
        assert_eq!(group(999, &g), "999", "three digits carry no separator");
        assert_eq!(group(1_000, &g), "1,000", "the first boundary");
        assert_eq!(group(9_999, &g), "9,999");
        assert_eq!(
            group(1_234_567, &g),
            "1,234,567",
            "seven digits, two separators"
        );
        assert_eq!(group(1_000_000, &g), "1,000,000");
        assert_eq!(group(i64::from(u32::MAX), &g), "4,294,967,295");
        assert_eq!(group(9_007_199_254_740_990, &g), "9,007,199,254,740,990");
        // Even an exactly representable odd value at the edge rounds upward when the digit loop
        // adds 0.5 at the client's configured 53-bit floating-point precision.
        assert_eq!(group(9_007_199_254_740_991, &g), "9,007,199,254,740,992");
        assert_eq!(group(9_007_199_254_740_993, &g), "9,007,199,254,740,992");
        assert_eq!(group(i64::MAX, &g), "9,223,372,036,854,776,028");
        assert_eq!(group(i64::MIN, &g), "-9,223,372,036,854,776,028");
        // The negative-number format's `"-%s"`.
        assert_eq!(group(-1_234, &g), "-1,234");
        assert_eq!(group(-1, &g), "-1");
    }

    /// Oracle: the same function's grouping-size read — a size of 0 divides nothing and a
    /// different separator is a different language, not a different code path.
    #[test]
    fn the_size_and_the_separator_are_data_and_not_constants_of_this_module() {
        let none = Grouping {
            size: 0,
            separator: ",".into(),
            negative_format: "-%s".into(),
        };
        assert_eq!(
            group(1_234_567, &none),
            "1234567",
            "a grouping size of 0 groups nothing"
        );
        let euro = Grouping {
            size: 3,
            separator: ".".into(),
            negative_format: "-%s".into(),
        };
        assert_eq!(group(1_234_567, &euro), "1.234.567");
        let indian = Grouping {
            size: 2,
            separator: ",".into(),
            negative_format: "-%s".into(),
        };
        assert_eq!(group(1_234_567, &indian), "1,23,45,67");
        let paren = Grouping {
            size: 3,
            separator: ",".into(),
            negative_format: "(%s)".into(),
        };
        assert_eq!(group(-1_234, &paren), "(1,234)");
    }

    /// With no asset source the fallback is xptostrings own number format.
    #[test]
    fn with_no_asset_source_the_fallback_is_xptostrings_own_number_format() {
        clear_cache();
        let g = shipped();
        assert_eq!(g.size, XPTOSTRING_GROUPING);
        assert_eq!(g.separator, XPTOSTRING_THOUSAND_SEP);
        assert_eq!(number(1_234_567), "1,234,567");
        clear_cache();
    }
}
