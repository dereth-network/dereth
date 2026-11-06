//! A language's writing rules (its string state record): how numbers are written, which numbers
//! take the singular, and the letters generated names carry.

use auberean_assets::strings::{ImeAppearance, StringState};

/// The writing rules the text code consults, from the language's string state record.
#[derive(Clone, Debug, PartialEq, Eq, Default)]
pub struct Language {
    /// The number base a table that names none uses (negative for none).
    pub base: i16,
    /// The least number of digits a number is written with when leading zeros are kept.
    pub decimal_digits: i16,
    /// Whether leading zeros are kept (negative for unset).
    pub leading_zero: i32,
    /// Digits between grouping separators.
    pub grouping_size: u16,
    /// The digits, `0` first.
    pub numerals: Vec<u16>,
    pub decimal_separator: Vec<u16>,
    pub grouping_separator: Vec<u16>,
    /// The pattern a negative number's digits are written into (`%s` for the digits).
    pub negative_format: Vec<u16>,
    /// Whether 0, 1, -1, 2 and up, and -2 and down take the singular.
    pub singular: [bool; 5],
    /// The letters a generated item name's prefix, middle and suffix may carry.
    pub treasure: [Vec<u16>; 3],
    /// The letters `M` and `F` stand for in a variable's flags.
    pub male: Vec<u16>,
    pub female: Vec<u16>,
    /// The highest gore level and blood color the language allows.
    pub top_gore: u32,
    pub top_blood: u32,
    /// How the language's input method draws.
    pub ime: ImeAppearance,
}

/// What the command line may set over the language's gore rules: its top gore level and top blood
/// color (0 for none). They outlast the language: setting them before it is read, or after it is
/// let go, still counts.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct GoreOverrides {
    pub top_gore_level: u32,
    pub top_blood_color: u32,
}

impl GoreOverrides {
    pub fn set_top_gore_level(&mut self, level: u32) {
        self.top_gore_level = level;
    }

    pub fn set_top_blood_color(&mut self, color: u32) {
        self.top_blood_color = color;
    }
}

fn wide(s: &str) -> Vec<u16> {
    s.encode_utf16().collect()
}

impl Language {
    /// The rules a string state record holds.
    #[must_use]
    pub fn from_state(s: &StringState) -> Self {
        let [prefix, middle, suffix, male, female] = &s.name_letters;
        #[allow(clippy::cast_possible_wrap)]
        Self {
            base: s.number_base as i16,
            decimal_digits: s.decimal_digits as i16,
            leading_zero: s.leading_zero as i32,
            grouping_size: s.grouping_size,
            numerals: wide(&s.numerals),
            decimal_separator: wide(&s.decimal_separator),
            grouping_separator: wide(&s.grouping_separator),
            negative_format: wide(&s.negative_format),
            singular: s.singular.map(|v| v != 0),
            treasure: [wide(prefix), wide(middle), wide(suffix)],
            male: wide(male),
            female: wide(female),
            top_gore: s.top_gore_level(),
            top_blood: s.top_blood_color(),
            ime: s.ime_appearance(),
        }
    }

    /// The base numbers are written in when a table names none (negative for none).
    // D2-14178
    #[must_use]
    pub fn base(&self) -> i16 {
        self.base
    }

    /// Whether leading zeros are kept (negative for unset).
    // D2-14177
    #[must_use]
    pub fn leading_zero(&self) -> i32 {
        self.leading_zero
    }

    /// The least number of digits written when leading zeros are kept.
    // D2-14176
    #[must_use]
    pub fn decimal_digits(&self) -> i16 {
        self.decimal_digits
    }

    /// Digits between grouping separators.
    // D2-14181
    #[must_use]
    pub fn grouping_size(&self) -> u16 {
        self.grouping_size
    }

    // D2-14198
    #[must_use]
    pub fn numerals(&self) -> &[u16] {
        &self.numerals
    }

    // D2-14199
    #[must_use]
    pub fn grouping_separator(&self) -> &[u16] {
        &self.grouping_separator
    }

    // D2-14200
    #[must_use]
    pub fn decimal_separator(&self) -> &[u16] {
        &self.decimal_separator
    }

    // D2-14201
    #[must_use]
    pub fn negative_format(&self) -> &[u16] {
        &self.negative_format
    }

    /// The highest gore level allowed: the command line's when it set one, else the language's.
    // D2-14179
    #[must_use]
    pub fn top_gore_level(&self, overrides: GoreOverrides) -> u32 {
        if overrides.top_gore_level != 0 {
            overrides.top_gore_level
        } else {
            self.top_gore
        }
    }

    /// The highest blood color allowed: the command line's when it set one, else the language's.
    #[must_use]
    pub fn top_blood_level(&self, overrides: GoreOverrides) -> u32 {
        if overrides.top_blood_color != 0 {
            overrides.top_blood_color
        } else {
            self.top_blood
        }
    }

    /// How the input method draws.
    // D2-14183
    #[must_use]
    pub fn ime_appearance(&self) -> ImeAppearance {
        self.ime
    }

    /// Whether a number takes the singular.
    // D2-14182
    #[must_use]
    pub fn is_singular(&self, n: i32) -> bool {
        match n {
            0 => self.singular[0],
            1 => self.singular[1],
            -1 => self.singular[2],
            n if n > 1 => self.singular[3],
            _ => self.singular[4],
        }
    }
}

/// Whether a number takes the singular: the language's rule, else only 1.
// D2-14261
#[must_use]
pub fn is_singular(lang: Option<&Language>, n: i32) -> bool {
    lang.map_or(n == 1, |l| l.is_singular(n))
}

/// The letters `M` stands for: the language's, else `mn`.
// D2-14262, D2-14202
#[must_use]
pub fn male_letters(lang: Option<&Language>) -> Vec<u16> {
    lang.map_or_else(|| wide("mn"), |l| l.male.clone())
}

/// The letters `F` stands for: the language's, else `fn`.
// D2-14263, D2-14203
#[must_use]
pub fn female_letters(lang: Option<&Language>) -> Vec<u16> {
    lang.map_or_else(|| wide("fn"), |l| l.female.clone())
}

/// The letters an item name's prefix, middle and suffix may carry: the language's, else `v`,
/// `mfnp` and `s`.
// D2-14264, D2-14204
#[must_use]
pub fn treasure_letters(lang: Option<&Language>) -> [Vec<u16>; 3] {
    lang.map_or_else(
        || [wide("v"), wide("mfnp"), wide("s")],
        |l| l.treasure.clone(),
    )
}
