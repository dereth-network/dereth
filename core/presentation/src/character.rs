//! The character information sheet's rules: the six sections' tokens and variable names, the
//! endurance ladders, the duration breakdown, the burden penalty, and the augmentation and
//! luminance section composed over any string service.
//!
//! The section text is composed out of the end-of-retail string tables, so the builders here take
//! a compose callback (a token and its named values in, the rendered text out) and never reach a
//! string table themselves; an interface hands them whichever string service it has.

use dereth_client_contract::view::CharacterInfo;

/// The `StringInfo` tokens the six sections compose.
pub mod string {
    /// `%BirthDate` — one variable, `strftime("%c")`'d.
    pub const BIRTH: &str = "ID_CharacterInfo_Birth";
    /// `%TimePlayed` — one variable, and it carries a whole nested `StringInfo`
    /// ([`super::query_duration`]) rather than a number.
    pub const PLAYED: &str = "ID_CharacterInfo_Played";
    /// The client's row, on table enum **2**.
    pub const DURATION_FORMAT: &str = "ID_DurationFormat";
    /// `%Mastery`.
    pub const MASTERY_MELEE: &str = "ID_CharacterInfo_Mastery_Melee";
    /// `%Mastery`.
    pub const MASTERY_RANGED: &str = "ID_CharacterInfo_Mastery_Ranged";
    /// `%Mastery`.
    pub const MASTERY_SUMMONING: &str = "ID_CharacterInfo_Mastery_Summoning";
    /// No variable, and **unconditional**.
    pub const LUMINANCE_HEADER: &str = "ID_CharacterInfo_Luminance_Header";
    pub const DEATHS_NONE: &str = "ID_CharacterInfo_Deaths_None";
    pub const DEATHS_ONE: &str = "ID_CharacterInfo_Deaths_One";
    pub const DEATHS_TWO: &str = "ID_CharacterInfo_Deaths_Two";
    /// `%NumberOfDeaths`.
    pub const DEATHS_MANY: &str = "ID_CharacterInfo_Deaths_Many";
    /// `%Enlightenment` — *"Enlightenment Tiers: "* and the count, after the deaths line.
    pub const ENLIGHTENMENT_TIERS: &str = "ID_CharacterInfo_Enlightenment_Tiers";
    /// `%Resists` twice and `%RegenerationBonus` — see the module header's note.
    pub const RESISTS: &str = "ID_CharacterInfo_Resists";
    /// Six `%`-variables, in the display order Strength, Endurance, Coordination, Quickness,
    /// Focus, Self.
    pub const INNATES: &str = "ID_CharacterInfo_Innates";
    /// `%ChessRank`.
    pub const CHESS: &str = "ID_CharacterInfo_Chess";
    /// `%FishingSkill`.
    pub const FISHING: &str = "ID_CharacterInfo_Fishing";
    pub const LOAD_NONE: &str = "ID_CharacterInfo_Load_None";
    /// `%Burden` and `%BurdenPenalty`.
    pub const LOAD_BURDENED: &str = "ID_CharacterInfo_Load_Burdened";
    /// `%NumAugmentations` and `%AdditionalLoad`.
    pub const LOAD_AUGMENTATIONS: &str = "ID_CharacterInfo_Load_Augmentations";
}

/// The **variable names** those rows file their values under — the key each value is added
/// under, which is a string hash of the name and never a slot number.
///
/// The client walks the *row's* own variable list and looks each id up in the caller's table, so
/// these names are the whole of what places a value and the order this file writes them in is
/// invisible. Every one is confirmed against the shipped rows' own variable lists.
pub mod var {
    /// The birth-date variable — the hash of `"DATE"`.
    pub const DATE: &str = "DATE";
    /// The time-played variable; its value is the whole nested `ID_DurationFormat` string.
    pub const DURATION: &str = "DURATION";
    /// The number-of-deaths variable.
    pub const DEATHS: &str = "DEATHS";
    /// The enlightenment count on [`super::string::ENLIGHTENMENT_TIERS`].
    pub const ENLIGHTENMENT: &str = "ENLIGHTENMENT";
    /// The resistances variable. `ID_CharacterInfo_Resists` names it **twice** while the client
    /// adds it **once**; the lookup simply answers twice, which is why this call site supplies one
    /// value.
    pub const RESIST: &str = "RESIST";
    /// The regeneration-bonus variable.
    pub const REGEN: &str = "REGEN";
    /// The Strength, Endurance, Coordination, Quickness, Focus and Self variables, in the display
    /// order the innates section adds them, which is also
    /// `ID_CharacterInfo_Innates`'s own order \[measured\].
    pub const INNATE: [&str; 6] = [
        "STRENGTH",
        "ENDURANCE",
        "COORDINATION",
        "QUICKNESS",
        "FOCUS",
        "SELF",
    ];
    /// The augmentation-count variable. Every luminance and augmentation row uses this one,
    /// and the 24 of them that ship with no variable at all never look it up.
    pub const NUM_AUGMENTATIONS: &str = "NUM_AUGMENTATIONS";
    /// The chess-rank variable.
    pub const CHESS: &str = "CHESS";
    /// The fishing-skill variable.
    pub const FISHING: &str = "FISHING";
    /// The burden variable.
    pub const BURDEN: &str = "BURDEN";
    /// The burden-penalty variable.
    pub const PENALTY: &str = "PENALTY";
    /// The additional-load variable.
    pub const ADDITIONAL_LOAD: &str = "ADDITIONAL_LOAD";
    /// The mastery variable. All three mastery rows name this same one.
    pub const MASTERY: &str = "MASTERY";
    /// `ID_DurationFormat`'s seven, in the order [`super::query_duration`] returns its terms.
    /// Unlike every other name here these are hashed **inline** where they are used rather than
    /// cached.
    pub const DURATION_TERMS: [&str; 7] = [
        "YEARS", "MONTHS", "WEEKS", "DAYS", "HOURS", "MINUTES", "SECONDS",
    ];
}

/// The six stored rating labels shared by both selection ladders.
pub mod band {
    /// No rating.
    pub const NONE: &str = "None";
    /// Poor rating.
    pub const POOR: &str = "Poor";
    /// Mediocre rating.
    pub const MEDIOCRE: &str = "Mediocre";
    /// Hardy rating.
    pub const HARDY: &str = "Hardy";
    /// Resilient rating.
    pub const RESILIENT: &str = "Resilient";
    /// Indomitable rating.
    pub const INDOMITABLE: &str = "Indomitable";
}

/// The int quality ids `CharacterInfoPanel` reads that are not already fields of
/// [`CharacterInfo`].
///
/// Defined in [`dereth_client_contract::panels::characterinfo::prop`], because
/// the runtime's HUD is what asks the qualities.
pub use dereth_client_contract::panels::characterinfo::prop;

/// The eleven luminance ratings, the eleven augmentation rows and the three mastery names.
///
/// Defined in [`dereth_client_contract::panels::characterinfo`], because
/// the runtime's HUD walks both tables to collect the ids it asks the qualities for, and the
/// three mastery names sit between them and are pure switches over the same three ids.
pub use dereth_client_contract::panels::characterinfo::{
    melee_mastery_name, ranged_mastery_name, summoning_mastery_name, AUGMENTATIONS, LUMINANCE,
};

/// The string table that table enum **2** names.
///
/// Every other row on this sheet comes from `0x10000001` → `0x23000001`; `ID_DurationFormat` is
/// the one row that does not, and `0x23000006` is the **only** one of the fifteen tables the dats
/// ship that holds it [measured against the shipped dats].
pub const DURATION_TABLE: dereth_primitives::DataId = dereth_primitives::DataId(0x2300_0006);

/// The duration breakdown — the seven values `ID_DurationFormat` interleaves, in its own order.
///
/// ```text
/// years   = s / 0x1E13380 ; s %= 0x1E13380      ; 31 536 000 = 365 days
/// months  = s / 0x278D00  ; s %= 0x278D00       ;  2 592 000 =  30 days
/// weeks   = s / 0x93A80   ; s %= 0x93A80        ;    604 800
/// days    = s / 0x15180   ; s %= 0x15180        ;     86 400
/// hours   = s / 0xE10     ; s %= 0xE10          ;      3 600
/// minutes = s / 0x3C      ; seconds = s % 0x3C
/// ```
///
/// The divisions are **unsigned**, which is why this takes a `u32`. The year is 365 days and the
/// month 30, so the terms do not add up to a calendar — that is the client's arithmetic and not a
/// simplification.
///
/// Each term is added under its name as an unsigned number when it is non-zero and as an empty
/// string when it is not.
/// **The empty string is the whole selector**: the metalanguage derives the `b` (blank) flag from
/// it, and `{ …[!b]}` is what deletes the term from the sentence. It is load-bearing, not a
/// placeholder.
#[must_use]
pub fn query_duration(seconds: u32) -> [String; 7] {
    let mut s = seconds;
    let mut term = |div: u32| {
        let n = s / div;
        s %= div;
        if n == 0 {
            String::new()
        } else {
            n.to_string()
        }
    };
    let y = term(0x1E13380);
    let mo = term(0x278D00);
    let w = term(0x93A80);
    let d = term(0x15180);
    let h = term(0xE10);
    let mi = term(0x3C);
    let se = if s == 0 { String::new() } else { s.to_string() };
    [y, mo, w, d, h, mi, se]
}

/// The endurance section's **first** ladder, over `strength + endurance`.
///
/// ```text
/// <= 0xC8  (200) -> "None"
/// <= 0x104 (260) -> "Poor"
/// <= 0x140 (320) -> "Mediocre"
/// <= 0x17C (380) -> "Hardy"
/// <= 0x1B8 (440) -> "Resilient", else "Indomitable"
/// ```
///
/// The comparisons are **unsigned**, which is what makes a negative sum — impossible
/// from attributes, but free to model — land in `Indomitable` rather than `None`.
#[must_use]
pub fn resist_band(sum: u32) -> &'static str {
    match sum {
        0..=0xC8 => band::NONE,
        0xC9..=0x104 => band::POOR,
        0x105..=0x140 => band::MEDIOCRE,
        0x141..=0x17C => band::HARDY,
        0x17D..=0x1B8 => band::RESILIENT,
        _ => band::INDOMITABLE,
    }
}

/// The endurance section's **second** ladder, over `strength + 2 * endurance`.
///
/// ```text
/// 0xC8 / 0x15A / 0x1D6 / 0x244 / 0x2B2   ; 200 / 346 / 470 / 580 / 690
/// ```
///
/// Same six literals, different bounds — which is why it cannot share [`resist_band`].
#[must_use]
pub fn regeneration_band(sum: u32) -> &'static str {
    match sum {
        0..=0xC8 => band::NONE,
        0xC9..=0x15A => band::POOR,
        0x15B..=0x1D6 => band::MEDIOCRE,
        0x1D7..=0x244 => band::HARDY,
        0x245..=0x2B2 => band::RESILIENT,
        _ => band::INDOMITABLE,
    }
}

/// The encumbrance system's load mod, then `(10 - (int)(mod * 10)) * 10`.
///
/// The percentage `ID_CharacterInfo_Load_Burdened` shows as *"reducing your Run, Jump, Melee
/// Defense and Missile Defense skills by N%"*. A load of exactly `1.0` gives `0`, `1.5` gives
/// `50`, and anything at or over `2.0` gives `100`.
///
/// The load mod is [`dereth_rules::burden::load_mod`], the one the run rate uses. The client
/// multiplies it by ten in double precision and truncates the product without storing it as a
/// float first; the end-of-retail client and the early ones do the same.
#[must_use]
pub fn burden_penalty_percent(load: f32) -> i32 {
    let load_mod = dereth_rules::burden::load_mod(load);
    let tenths = dereth_primitives::num::to_i32_f64(f64::from(load_mod) * 10.0);
    (10 - tenths) * 10
}

/// A number as every integer variable on the sheet renders it: the shipped digit grouping.
#[must_use]
pub fn num(v: impl Into<i128>) -> String {
    let v: i128 = v.into();
    dereth_client_contract::panels::numfmt::number(i64::try_from(v).unwrap_or(if v < 0 {
        i64::MIN
    } else {
        i64::MAX
    }))
}

/// One row composed out of the string tables: its token and its named values in, the rendered
/// row out. An interface supplies it over whatever string service it has.
pub type Compose<'a> = dyn FnMut(&str, &[(&str, &str)]) -> String + 'a;

/// The augmentation and luminance section: three mastery lines, the luminance header,
/// [`LUMINANCE`] and [`AUGMENTATIONS`], in that order, each row composed by `compose` (a token and
/// its named values in, the rendered row out).
///
/// - Melee (int `0x162`), ranged (`0x163`) and summoning (`0x16A`) mastery each show one line
///   when above zero, with `%Mastery` named from the value.
/// - The luminance header, unconditional and with no variable, when `luminance` is set (the
///   world's era has luminance); without it neither the header nor the eleven ratings show.
/// - The four rated pairs: a value above 5 shows the base line with `%NumAugmentations = 5` and
///   the specialised line with the excess; 1..=5 shows only the base line; zero or less shows
///   nothing. The specialised count starts from zero for every pair, so one pair's excess never
///   reaches the next pair's line.
/// - The seven single ratings and the 43 augmentations: each shows its line, with
///   `%NumAugmentations` the value, when the value is above zero. The single ratings are never
///   split at five.
///
/// Each row's text carries its own trailing line break, so the rows are simply appended.
pub fn augmentation_section(
    c: &CharacterInfo,
    features: dereth_primitives::EraFeatures,
    compose: &mut Compose<'_>,
) -> String {
    if !(features.luminance || features.innate_augmentations) {
        return String::new();
    }
    let mut s = String::new();
    if c.melee_mastery > 0 {
        let name = melee_mastery_name(c.melee_mastery);
        s.push_str(&compose(string::MASTERY_MELEE, &[(var::MASTERY, name)]));
    }
    if c.ranged_mastery > 0 {
        let name = ranged_mastery_name(c.ranged_mastery);
        s.push_str(&compose(string::MASTERY_RANGED, &[(var::MASTERY, name)]));
    }
    if c.summoning_mastery > 0 {
        let name = summoning_mastery_name(c.summoning_mastery);
        s.push_str(&compose(string::MASTERY_SUMMONING, &[(var::MASTERY, name)]));
    }
    // No gate and no variable: the header is on the sheet even for a character with no luminance
    // at all, which is what a new character sees first -- in an era that has luminance.
    let ratings: &[_] = if features.luminance {
        s.push_str(&compose(string::LUMINANCE_HEADER, &[]));
        LUMINANCE
    } else {
        &[]
    };
    for (id, base, spec) in ratings {
        let v = c.aug_ints.get(id).copied().unwrap_or(0);
        match spec {
            // The rated pairs: five splits the value, and both halves can show.
            Some(sp) => {
                let (base_v, spec_v) = if v > 5 { (5, v - 5) } else { (v, 0) };
                if base_v <= 0 {
                    continue;
                }
                let n = num(base_v);
                s.push_str(&compose(base, &[(var::NUM_AUGMENTATIONS, n.as_str())]));
                if spec_v > 0 {
                    let n = num(spec_v);
                    s.push_str(&compose(sp, &[(var::NUM_AUGMENTATIONS, n.as_str())]));
                }
            }
            None => {
                if v > 0 {
                    let n = num(v);
                    s.push_str(&compose(base, &[(var::NUM_AUGMENTATIONS, n.as_str())]));
                }
            }
        }
    }
    for (id, token) in AUGMENTATIONS {
        let v = c.aug_ints.get(id).copied().unwrap_or(0);
        if v > 0 {
            let n = num(v);
            s.push_str(&compose(token, &[(var::NUM_AUGMENTATIONS, n.as_str())]));
        }
    }
    s
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Oracle: the client's two ladders, at every bound.
    ///
    /// The bounds are **inclusive** because each step tests *above*, so `0xC8` itself
    /// is still `"None"` and `0xC9` is the first `"Poor"`. That off-by-one is the whole reason to
    /// pin every boundary rather than a value in the middle of each band.
    #[test]
    fn the_two_endurance_ladders_have_different_bounds_and_the_same_six_words() {
        assert_eq!(resist_band(0), band::NONE);
        assert_eq!(
            resist_band(200),
            band::NONE,
            "0xC8 is not above the first bound"
        );
        assert_eq!(resist_band(201), band::POOR);
        assert_eq!(resist_band(260), band::POOR);
        assert_eq!(resist_band(261), band::MEDIOCRE);
        assert_eq!(resist_band(320), band::MEDIOCRE);
        assert_eq!(resist_band(321), band::HARDY);
        assert_eq!(resist_band(380), band::HARDY);
        assert_eq!(resist_band(381), band::RESILIENT);
        assert_eq!(resist_band(440), band::RESILIENT);
        assert_eq!(resist_band(441), band::INDOMITABLE);

        assert_eq!(regeneration_band(200), band::NONE);
        assert_eq!(regeneration_band(201), band::POOR);
        assert_eq!(regeneration_band(346), band::POOR, "0x15A, not 0x104");
        assert_eq!(regeneration_band(347), band::MEDIOCRE);
        assert_eq!(regeneration_band(470), band::MEDIOCRE);
        assert_eq!(regeneration_band(471), band::HARDY);
        assert_eq!(regeneration_band(580), band::HARDY);
        assert_eq!(regeneration_band(581), band::RESILIENT);
        assert_eq!(regeneration_band(690), band::RESILIENT);
        assert_eq!(regeneration_band(691), band::INDOMITABLE);

        // The two ladders disagree over most of their range, which is what stops one standing in
        // for the other: 300 is Mediocre on the first and Poor on the second.
        assert_eq!(resist_band(300), band::MEDIOCRE);
        assert_eq!(regeneration_band(300), band::POOR);
    }

    /// Oracle: the client's `(10 - trunc(load_mod(load) * 10.0)) * 10`, the product in double
    /// precision, and the load modifier's three branches.
    #[test]
    fn the_burden_penalty_is_ten_minus_the_load_mod_in_tenths_times_ten() {
        assert_eq!(
            burden_penalty_percent(0.5),
            0,
            "under capacity: the load mod is 1.0"
        );
        assert_eq!(
            burden_penalty_percent(1.0),
            0,
            "exactly at capacity, and still 0%"
        );
        assert_eq!(burden_penalty_percent(1.5), 50, "0.5 is exact in f32");
        // **The truncation is the client's and it is visible.** `2.0f - 1.2f` is `0.79999995`,
        // `* 10.0f` is `7.9999995`, and the truncation takes **7** -- so a load of 1.2 costs 30%, not
        // the 20% an `f64` implementation would print. Pinned because it is the one place this
        // function can silently disagree with retail.
        assert_eq!(burden_penalty_percent(1.2), 30);
        // **The product is a double.** A load of 110% is `1.1f`, its mod `0.9f` is `0.89999998`,
        // and ten of those is `8.9999998` in double precision: the truncation takes **8**, so the
        // sheet says 20%. Rounded to a float first, the product would be `9.0` and the sheet 10%.
        // Of every float load from 1 to 2 this is the one where the two widths disagree.
        assert_eq!(
            burden_penalty_percent(dereth_rules::burden::load(100, 110)),
            20
        );
        assert_eq!(
            burden_penalty_percent(1.9),
            90,
            "2.0f - 1.9f is 0.100000024, so (int)1"
        );
        assert_eq!(
            burden_penalty_percent(2.0),
            100,
            "the load mod clamps to 0 at twice capacity"
        );
        assert_eq!(burden_penalty_percent(5.0), 100);
    }

    /// The token form of a composed row, so a test can see which rows showed with which values.
    fn token_form(token: &str, values: &[(&str, &str)]) -> String {
        let vals: Vec<&str> = values.iter().map(|(_, v)| *v).collect();
        format!("{token}[{}]\n", vals.join("|"))
    }

    /// A rated pair splits at five and shows both halves; a single rating never splits; without
    /// luminance the header and every rating go, and the masteries and augmentations stay.
    #[test]
    fn the_augmentation_section_splits_rated_pairs_at_five_and_drops_luminance_by_era() {
        let mut c = CharacterInfo {
            melee_mastery: 1,
            ..CharacterInfo::default()
        };
        c.aug_ints.insert(0x14D, 7);
        c.aug_ints.insert(0x14E, 3);
        c.aug_ints.insert(0x152, 9);
        c.aug_ints.insert(0xDA, 1);
        let melee = format!("ID_CharacterInfo_Mastery_Melee[{}]", melee_mastery_name(1));
        let with = augmentation_section(
            &c,
            dereth_primitives::EraFeatures::END_OF_RETAIL,
            &mut token_form,
        );
        assert_eq!(
            with.lines().collect::<Vec<_>>(),
            [
                melee.as_str(),
                "ID_CharacterInfo_Luminance_Header[]",
                "ID_CharacterInfo_Luminance_Base_Damage[5]",
                "ID_CharacterInfo_Luminance_Spec_Damage[2]",
                "ID_CharacterInfo_Luminance_Base_Reduction[3]",
                "ID_CharacterInfo_Luminance_Base_Surge_Chance[9]",
                "ID_CharacterInfo_Augmentation_Attribute_Strength[1]",
            ]
        );
        let without = augmentation_section(
            &c,
            dereth_primitives::EraFeatures {
                luminance: false,
                ..Default::default()
            },
            &mut token_form,
        );
        assert_eq!(
            without.lines().collect::<Vec<_>>(),
            [
                melee.as_str(),
                "ID_CharacterInfo_Augmentation_Attribute_Strength[1]",
            ]
        );
    }
    /// Behaviour: presentation.era.shared-facts-follow-the-world-profile
    #[test]
    fn the_augmentation_section_requires_luminance_or_innate_augmentations() {
        let mut c = CharacterInfo {
            melee_mastery: 1,
            ..Default::default()
        };
        c.aug_ints.insert(0xDA, 1);
        for luminance in [false, true] {
            for innate_augmentations in [false, true] {
                let text = augmentation_section(
                    &c,
                    dereth_primitives::EraFeatures {
                        luminance,
                        innate_augmentations,
                        ..Default::default()
                    },
                    &mut token_form,
                );
                assert_eq!(text.is_empty(), !luminance && !innate_augmentations);
                if !text.is_empty() {
                    assert!(text.contains("Mastery_Melee"));
                    assert!(text.contains("Augmentation_Attribute_Strength"));
                    assert_eq!(text.contains("Luminance_Header"), luminance);
                }
            }
        }
    }
}
