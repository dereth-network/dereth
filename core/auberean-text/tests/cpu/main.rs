//! The formatter without data: the meta language, numbers, and references over tables made here.

mod meta;
mod names;
mod numbers;
mod places;
mod references;

use auberean_text::Language;

pub(crate) fn wide(s: &str) -> Vec<u16> {
    s.encode_utf16().collect()
}

/// English's writing rules, as its string state record holds them (with plain hexadecimal
/// numerals).
pub(crate) fn english() -> Language {
    Language {
        base: 10,
        decimal_digits: 5,
        leading_zero: 0,
        grouping_size: 3,
        numerals: wide("0123456789ABCDEF"),
        decimal_separator: wide("."),
        grouping_separator: wide(","),
        negative_format: wide("-%s"),
        singular: [false, true, false, false, false],
        treasure: [wide("v"), wide("mfnp"), wide("s")],
        male: wide("mn"),
        female: wide("fn"),
        ..Language::default()
    }
}
