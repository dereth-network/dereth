//! Attributes, vitals and their derivations.
//!
//! The client has **no vital regeneration tick of its own.** Current health, stamina and mana change
//! only when the server sends `Qualities_UpdateAttribute2nd`. The bar interpolates its *fill*; the
//! number is never predicted.

use dereth_assets::tables::SkillFormula;
use dereth_primitives::num::to_i32_f64;

/// The six `PropertyAttribute` ids.
pub mod attribute {
    pub const STRENGTH: u32 = 1;
    pub const ENDURANCE: u32 = 2;
    pub const QUICKNESS: u32 = 3;
    pub const COORDINATION: u32 = 4;
    pub const FOCUS: u32 = 5;
    pub const SELF: u32 = 6;
}

/// The six `PropertyAttribute2nd` ids. Odd ids are maxima, even ids are the current values.
pub mod vital {
    pub const MAX_HEALTH: u32 = 1;
    pub const HEALTH: u32 = 2;
    pub const MAX_STAMINA: u32 = 3;
    pub const STAMINA: u32 = 4;
    pub const MAX_MANA: u32 = 5;
    pub const MANA: u32 = 6;
}

/// The skill formula.
///
/// Three details that must be reproduced exactly:
///
/// 1. the division is `float / float`, not `double / double`;
/// 2. the `int → float` conversions treat the value as **unsigned**, so a negative numerator wraps
///    rather than going negative;
/// 3. rounding is `floor(x + 0.5)` then truncation to an integer — round-half-up, not banker's
///    rounding.
///
/// Returns `None` for `_z == 0`, which is the client's failure return that leaves `out` untouched.
#[must_use]
pub fn skill_formula_calculate(f: &SkillFormula, a1: u32, a2: u32) -> Option<u32> {
    if f.z == 0 {
        return None;
    }
    // 32-bit signed intermediate, exactly as the client computes it.
    let n =
        f.x.wrapping_mul(a1)
            .wrapping_add(f.y.wrapping_mul(a2))
            .wrapping_add(f.w);
    // The unsigned conversion: the `+ 4.2949673e+09` for negatives is just "read the bit
    // pattern as unsigned", which `n` already is.
    #[allow(clippy::cast_precision_loss)]
    let num = n as f32;
    #[allow(clippy::cast_precision_loss)]
    let den = f.z as f32;
    let v = f64::from(num / den) + 0.5;
    let t = to_i32_f64(v.floor());
    #[allow(clippy::cast_sign_loss)] // the client's `(unsigned long)` cast
    Some(t as u32)
}

// ---------------------------------------------------------------------------------------------
// The inquiries over an object's qualities (the records need the `proto` feature).
// ---------------------------------------------------------------------------------------------

#[cfg(feature = "proto")]
pub use inquiries::*;

#[cfg(feature = "proto")]
mod inquiries {
    use super::{attribute, skill_formula_calculate, vital};
    use crate::quality::QualityRead;
    use dereth_assets::tables::{Attribute2ndTable, QualityFilter};

    /// Behavior: `_init_level + _level_from_cp`, the **base** value.
    #[must_use]
    pub fn inq_attribute_base<Q: QualityRead + ?Sized>(q: &Q, id: u32) -> Option<u32> {
        let a = match id {
            attribute::STRENGTH
            | attribute::ENDURANCE
            | attribute::QUICKNESS
            | attribute::COORDINATION
            | attribute::FOCUS
            | attribute::SELF => q.attribute(id),
            _ => None,
        }?;
        Some(a.init_level.wrapping_add(a.level_from_cp))
    }

    /// Query a primary attribute's base value, enchanted unless `raw`.
    #[must_use]
    pub fn inq_attribute<Q: QualityRead + ?Sized>(q: &Q, id: u32, raw: bool) -> Option<u32> {
        let base = inq_attribute_base(q, id)?;
        if raw {
            return Some(base);
        }
        let v = q
            .enchantments()
            .enchant_attribute(id, i32::try_from(base).unwrap_or(i32::MAX));
        Some(u32::try_from(v).unwrap_or(0))
    }

    /// Query the stored ranks, or the *current* value for an even id.
    #[must_use]
    pub fn inq_attribute_2nd_stored<Q: QualityRead + ?Sized>(q: &Q, id: u32) -> Option<u32> {
        let s = match id {
            vital::MAX_HEALTH
            | vital::HEALTH
            | vital::MAX_STAMINA
            | vital::STAMINA
            | vital::MAX_MANA
            | vital::MANA => q.attribute_2nd(id),
            _ => None,
        }?;
        Some(if id.is_multiple_of(2) {
            s.current_level
        } else {
            s.attribute
                .init_level
                .wrapping_add(s.attribute.level_from_cp)
        })
    }

    /// Behavior: the formula half, from the dat
    /// `Attribute2ndTable`. Only ids 1, 3 and 5 have a formula; anything else returns `None`.
    #[must_use]
    pub fn inq_attribute_2nd_base_level<Q: QualityRead + ?Sized>(
        q: &Q,
        table: &Attribute2ndTable,
        id: u32,
        raw: bool,
    ) -> Option<u32> {
        let f = match id {
            vital::MAX_HEALTH => table.health,
            vital::MAX_STAMINA => table.stamina,
            vital::MAX_MANA => table.mana,
            _ => return None,
        };
        let a1 = if f.attr1 == 0 {
            0
        } else {
            inq_attribute(q, f.attr1, raw).unwrap_or(0)
        };
        let a2 = if f.attr2 == 0 {
            0
        } else {
            inq_attribute(q, f.attr2, raw).unwrap_or(0)
        };
        skill_formula_calculate(&f, a1, a2)
    }

    /// Behavior: the number the panel shows.
    ///
    /// `MaxHealth` alone picks up int property 379 (`GearMaxHealth`), negative values included,
    /// and, when it is above zero, twice int property 390 (enlightenment).
    ///
    /// Unless `raw`, the sum then goes through the secondary-attribute enchantments, vitae first,
    /// but only for an id the quality `filter` lists. The shipped filter lists the three maxima
    /// (1, 3, 5) and none of the current values, so an enchantment that reaches every vital scales
    /// the maxima and leaves the current health, stamina and mana as stored; vitae does not touch
    /// them either. With no filter loaded nothing is enchanted, as the client does when the filter
    /// cannot be fetched.
    #[must_use]
    pub fn inq_attribute_2nd<Q: QualityRead + ?Sized>(
        q: &Q,
        table: &Attribute2ndTable,
        id: u32,
        raw: bool,
        filter: Option<&QualityFilter>,
    ) -> Option<u32> {
        let mut base = 0u32;
        if matches!(id, vital::MAX_HEALTH | vital::MAX_STAMINA | vital::MAX_MANA) {
            base = inq_attribute_2nd_base_level(q, table, id, raw)?;
        }
        if id == vital::MAX_HEALTH {
            // Gear max health is added as it is stored, so a negative value lowers the base; the
            // sum is the client's unsigned 32-bit arithmetic.
            let gear = q.inq_int(379);
            if gear != 0 {
                base = base.wrapping_add_signed(gear);
            }
            // Enlightenment: twice its count, on maximum health only, before the enchantment
            // multiplier and vitae. A character who never enlightened has no such property.
            let enlightenment = q.inq_int(crate::skills::aug::ENLIGHTENMENT);
            if enlightenment > 0 {
                base = base.wrapping_add(u32::try_from(enlightenment).unwrap_or(0).wrapping_mul(2));
            }
        }
        let mut v = match inq_attribute_2nd_stored(q, id) {
            Some(stored) => stored.wrapping_add(base),
            None => {
                if base == 0 {
                    return None;
                }
                base
            }
        };
        if !raw {
            v = enchant_attribute_2nd_filtered(q, id, v, filter);
        }
        Some(v)
    }

    /// Behavior: a current vital (health, stamina or mana) as [`inq_attribute_2nd`] reads it
    /// unraw: the stored current level, then the secondary-attribute enchantments only if the
    /// quality `filter` lists the id. A current value has no formula half, so no table is needed.
    /// Any other id answers `None`.
    #[must_use]
    pub fn inq_current_vital<Q: QualityRead + ?Sized>(
        q: &Q,
        id: u32,
        filter: Option<&QualityFilter>,
    ) -> Option<u32> {
        if !matches!(id, vital::HEALTH | vital::STAMINA | vital::MANA) {
            return None;
        }
        let v = inq_attribute_2nd_stored(q, id)?;
        Some(enchant_attribute_2nd_filtered(q, id, v, filter))
    }

    /// The secondary-attribute enchantment, applied only to an id the filter lists; with no filter
    /// the value is returned untouched.
    fn enchant_attribute_2nd_filtered<Q: QualityRead + ?Sized>(
        q: &Q,
        id: u32,
        v: u32,
        filter: Option<&QualityFilter>,
    ) -> u32 {
        if !filter.is_some_and(|f| f.allows_attribute_2nd(id)) {
            return v;
        }
        u32::try_from(
            q.enchantments()
                .enchant_attribute_2nd(id, i32::try_from(v).unwrap_or(i32::MAX)),
        )
        .unwrap_or(0)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn formula(w: u32, x: u32, y: u32, z: u32, a1: u32, a2: u32) -> SkillFormula {
        SkillFormula {
            w,
            x,
            y,
            z,
            attr1: a1,
            attr2: a2,
        }
    }

    /// Oracle: the recovered C implementation of the skill formula. The three rounding and
    /// zero-divisor details are each asserted.
    #[test]
    fn the_skill_formula_rounds_half_up_and_fails_on_a_zero_divisor() {
        // ( (2 x 100) + 100 ) / 3 = 100
        assert_eq!(
            skill_formula_calculate(&formula(0, 2, 1, 3, 4, 3), 100, 100),
            Some(100)
        );
        // 301/3 = 100.333 -> floor(100.833) = 100
        assert_eq!(
            skill_formula_calculate(&formula(1, 2, 1, 3, 4, 3), 100, 100),
            Some(100)
        );
        // 302/3 = 100.667 -> floor(101.167) = 101
        assert_eq!(
            skill_formula_calculate(&formula(2, 2, 1, 3, 4, 3), 100, 100),
            Some(101)
        );
        // Exactly .5 rounds *up*, which banker's rounding would not do.
        // 201/2 = 100.5 -> floor(101.0) = 101
        assert_eq!(
            skill_formula_calculate(&formula(1, 2, 0, 2, 1, 0), 100, 0),
            Some(101)
        );
        assert_eq!(
            skill_formula_calculate(&formula(0, 1, 0, 0, 1, 0), 100, 0),
            None
        );
    }

    /// Oracle: §3.2 detail 2 — the `int → float` conversion treats the value as **unsigned**, so a
    /// negative numerator wraps to just under 2^32 rather than going negative.
    #[test]
    fn a_negative_numerator_wraps_rather_than_going_negative() {
        // w = -1 as u32 with x = y = 0: numerator = 0xFFFFFFFF, denominator = 1.
        let v = skill_formula_calculate(&formula(u32::MAX, 0, 0, 1, 0, 0), 0, 0);
        // 4294967295 as f32 rounds to 4294967296.0; +0.5, floor, then the integer conversion — out of
        // `i32` range and therefore yields the integer-indefinite value.
        assert_eq!(
            v,
            Some(0x8000_0000),
            "the client's out-of-range integer conversion result, not a negative"
        );
    }
}
