//! `InfoRegion` — the label / value / tooltip row the skill, attribute and effect panels share.
//!
//! It lives here rather than in `dereth_ui_screens::panels::inforegion` because its entire
//! production surface is `dereth_primitives::num` and plain arithmetic -- it names no `dereth_ui`
//! type -- and `dereth_client::hud` calls `apply_vitae` and `vitae_modifier` when it composes the
//! skill and vital rows.
//!
//! **Timers are rebased on receipt.** Enchantment and skill timers are rebased when they arrive, so
//! every duration a panel displays is already shortened by one-way latency, permanently. The
//! effects panel and the vitae display must read the client model's rebased values, **not
//! recompute from a server timestamp**. `format_duration` therefore takes seconds
//! remaining and has no timestamp argument at all.

/// The five `InfoRegion` classes and the format each renders.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum InfoRegionKind {
    /// The base class; sets the row's state and handles quality changes.
    Base,
    /// Primary attribute — `"%d"`, or `"???"` when unknown.
    Attribute,
    /// Vital attribute — `"%d/%d"`, `"%d %%"`, `"%d/%d (%d %%)"`, `"???"`.
    Attribute2nd,
    /// Skill — `"%d"`, with the vitae modifier applied.
    Skill,
    /// Effect duration — `"%d:%02d"` or `"%d:%02d:%02d"`.
    Effect,
}

/// The string an unknown value renders as, in every kind that has one.
pub const UNKNOWN: &str = "???";

/// The attribute row: `"%d"`, or `"???"` when unknown.
#[must_use]
pub fn format_attribute(v: Option<i32>) -> String {
    v.map_or_else(|| UNKNOWN.to_owned(), |v| v.to_string())
}

/// The secondary-attribute row — its four shapes.
#[must_use]
pub fn format_attribute_2nd(cur: Option<i32>, max: Option<i32>, percent: Option<i32>) -> String {
    match (cur, max, percent) {
        (Some(c), Some(m), Some(p)) => format!("{c}/{m} ({p} %)"),
        (Some(c), Some(m), None) => format!("{c}/{m}"),
        (None, None, Some(p)) => format!("{p} %"),
        _ => UNKNOWN.to_owned(),
    }
}

/// The effect row's remaining duration — `"%d:%02d"` under an hour, `"%d:%02d:%02d"` at or
/// over one.
///
/// The argument is **seconds remaining**, already rebased by the client model.
#[must_use]
pub fn format_duration(seconds_remaining: i64) -> String {
    let s = seconds_remaining.max(0);
    let (h, m, sec) = (s / 3600, (s % 3600) / 60, s % 60);
    if h > 0 {
        format!("{h}:{m:02}:{sec:02}")
    } else {
        format!("{m}:{sec:02}")
    }
}

/// The vitae multiplier applied to a value, as the skill row computes it.
///
/// ```text
/// if no vitae enchantment is registered      return 0
/// if !(vitae.multiplier < 1.0)               return 0    // a double compare against 1.0
/// raw = skill_level(skill, raw = true)                   // the RAW level, not the enchanted one
/// f   = (float)raw * vitae.multiplier                    // multiplicative
/// return trunc(f + 0.5) - raw
/// ```
///
/// So the answer is the **delta**, and it is **negative** under a penalty. Its two callers
/// subtract it: the skill row compares the raw level against the enchanted total minus this
/// delta, i.e. the enchanted total with the vitae contribution taken back out, so a character
/// carrying vitae and nothing else draws every skill **plain** rather than every skill red. This
/// function is the multiply-and-round half; the subtraction is the caller's.
///
/// **It rounds; it does not truncate.** The client adds **0.5** before the truncation, so
/// `apply_vitae(100, Some(0.985))` is 99, not 98. The same
/// add-a-half ends the secondary-attribute row's version and all three of the enchantment
/// registry's skill, attribute and secondary-attribute enchant calls. It is invisible on every
/// recorded character because with no vitae and no multiplicative enchantment the value is
/// already an integer and `trunc(n + 0.5)` is `n`.
///
/// **One deliberate difference from the secondary-attribute row's version**, which is this
/// function's only other caller shape: that one adds `4294967296.0f` to the float when the raw
/// value is **negative**, because the secondary-attribute query writes a `ulong` and the client
/// is reinterpreting it. The skill row's has no such arm. The fixup cannot fire for a
/// non-negative input and every caller here passes one, so it is documented rather than
/// modelled.
#[must_use]
pub fn apply_vitae(value: i32, vitae: Option<f32>) -> i32 {
    match vitae {
        Some(m) if m < 1.0 => {
            #[allow(clippy::cast_precision_loss)]
            let scaled = f64::from(value as f32 * m) + 0.5;
            #[allow(clippy::cast_possible_truncation)]
            dereth_primitives::num::to_i32(scaled as f32)
        }
        _ => value,
    }
}

/// The client's **return value** — `Enchant(raw) - raw`, i.e.
/// zero or negative.
///
/// `raw` is the skill lookup's raw (un-enchanted) level, which is what the client
/// passes: the modifier is *the amount vitae would take off the raw level*, and it is the term
/// the color comparison subtracts.
#[must_use]
pub fn vitae_modifier(raw: i32, vitae: Option<f32>) -> i32 {
    apply_vitae(raw, vitae) - raw
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Oracle: the information panel's duration-formatting rules, evaluated for each shape.
    #[test]
    fn each_info_region_renders_its_documented_format() {
        assert_eq!(format_attribute(Some(150)), "150");
        assert_eq!(format_attribute(None), "???");

        assert_eq!(format_attribute_2nd(Some(50), Some(100), None), "50/100");
        assert_eq!(format_attribute_2nd(None, None, Some(75)), "75 %");
        assert_eq!(
            format_attribute_2nd(Some(50), Some(100), Some(50)),
            "50/100 (50 %)"
        );
        assert_eq!(format_attribute_2nd(None, Some(100), None), "???");
    }

    /// Pinned behavior: the two duration shapes, at the
    /// boundary between them.
    #[test]
    fn a_duration_gains_its_hour_field_at_exactly_one_hour() {
        assert_eq!(format_duration(0), "0:00");
        assert_eq!(format_duration(9), "0:09");
        assert_eq!(format_duration(75), "1:15");
        assert_eq!(format_duration(3599), "59:59");
        assert_eq!(format_duration(3600), "1:00:00");
        assert_eq!(format_duration(3661), "1:01:01");
        // A duration already past zero — which rebasing makes reachable — shows 0:00
        // rather than a negative.
        assert_eq!(format_duration(-5), "0:00");
    }

    /// Oracle: the client's own vitae-modifier arithmetic, step by step — the compare against
    /// the double `1.0`, the multiplicative enchant, the add of the double `0.5`, and the
    /// truncation.
    ///
    /// **The 0.985 case is the whole point of pinning the results as literals.** Plain
    /// truncation gives **98**; the add of a half makes it **99**. Both readings agree on every
    /// other row here: the disagreement needs a multiplier whose product has a fractional part of
    /// at least a half.
    #[test]
    fn the_vitae_penalty_scales_a_skill_and_rounds_half_up() {
        assert_eq!(apply_vitae(100, None), 100, "no vitae registered at all");
        assert_eq!(
            apply_vitae(100, Some(1.0)),
            100,
            "1.0 is not `< 1.0`, so the early return"
        );
        assert_eq!(
            apply_vitae(100, Some(1.05)),
            100,
            "and neither is anything above it"
        );
        assert_eq!(
            apply_vitae(100, Some(0.95)),
            95,
            "95.0 + 0.5 truncates to 95"
        );
        // 0.985 * 100 is 98.5; the client's `+ 0.5` makes that 99.0, not 98.
        assert_eq!(
            apply_vitae(100, Some(0.985)),
            99,
            "the client adds a half before truncating"
        );
        // Two more that separate the two readings, in both directions.
        assert_eq!(
            apply_vitae(50, Some(0.95)),
            48,
            "47.5 + 0.5 = 48, where truncation gives 47"
        );
        assert_eq!(
            apply_vitae(51, Some(0.95)),
            48,
            "48.45 + 0.5 = 48.95, still 48"
        );
        assert_eq!(apply_vitae(0, Some(0.5)), 0);
        // The modifier itself is the delta, and it is negative under a penalty.
        assert_eq!(vitae_modifier(100, Some(0.95)), -5);
        assert_eq!(vitae_modifier(100, Some(1.0)), 0, "no penalty, no term");
        assert_eq!(vitae_modifier(100, None), 0);
        assert_eq!(
            vitae_modifier(50, Some(0.95)),
            -2,
            "48 - 50; the truncating reading gives -3"
        );
    }
}
