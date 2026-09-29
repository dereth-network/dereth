//! The skill table, the skill lookup's bonus stack, augmentations, and the two movement formulas
//! skills drive.

/// The skill advancement class.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Default)]
#[repr(u32)]
pub enum Sac {
    #[default]
    Undef = 0,
    Untrained = 1,
    Trained = 2,
    Specialized = 3,
}

impl Sac {
    #[must_use]
    pub fn from_raw(v: u32) -> Self {
        match v {
            1 => Self::Untrained,
            2 => Self::Trained,
            3 => Self::Specialized,
            _ => Self::Undef,
        }
    }
}

/// `SKILL_CATEGORY` — `SkillBase::_category`.
pub mod skill_category {
    pub const UNDEF: u32 = 0;
    pub const WEAPON: u32 = 1;
    pub const NONWEAPON: u32 = 2;
    pub const MAGIC: u32 = 3;
}

/// The skill ids the client names by literal, and the two movement skills.
pub mod skill {
    pub const JUMP: u32 = 0x16;
    pub const RUN: u32 = 0x18;
    pub const CREATURE_ENCHANTMENT: u32 = 0x1F;
    pub const ITEM_ENCHANTMENT: u32 = 0x20;
    pub const LIFE_MAGIC: u32 = 0x21;
    pub const WAR_MAGIC: u32 = 0x22;
    pub const TWO_HANDED_COMBAT: u32 = 0x29;
    pub const VOID_MAGIC: u32 = 0x2B;
    pub const HEAVY_WEAPONS: u32 = 0x2C;
    pub const LIGHT_WEAPONS: u32 = 0x2D;
    pub const FINESSE_WEAPONS: u32 = 0x2E;
    pub const MISSILE_WEAPONS: u32 = 0x2F;
    pub const DUAL_WIELD: u32 = 0x31;
    pub const RECKLESSNESS: u32 = 0x32;
}

/// The five augmentation and luminance int properties the skill query reads.
///
/// The ids match ACE's `PropertyInt` exactly `\[verified\]`.
pub mod aug {
    pub const SKILLED_MELEE: u32 = 300;
    pub const SKILLED_MISSILE: u32 = 301;
    pub const SKILLED_MAGIC: u32 = 302;
    pub const JACK_OF_ALL_TRADES: u32 = 326;
    pub const LUM_SKILLED_SPEC: u32 = 344;
    pub const LUM_ALL_SKILLS: u32 = 365;
    pub const INCREASED_CARRYING_CAPACITY: u32 = 230;
    /// The enlightenment count: while it is above zero, each level adds one to every trained or
    /// specialized skill (Run and Jump included) and two to maximum health. A character who never
    /// enlightened has no such property, so nothing changes for them.
    pub const ENLIGHTENMENT: u32 = 390;
}

/// The skill lookup's augmentation switch: which `"Skilled <family>"` property, if any, grants this
/// skill its flat `+10`.
#[must_use]
pub fn skilled_augmentation_for(skill_id: u32) -> Option<u32> {
    match skill_id {
        skill::CREATURE_ENCHANTMENT
        | skill::ITEM_ENCHANTMENT
        | skill::LIFE_MAGIC
        | skill::WAR_MAGIC
        | skill::VOID_MAGIC => Some(aug::SKILLED_MAGIC),
        skill::TWO_HANDED_COMBAT
        | skill::HEAVY_WEAPONS
        | skill::LIGHT_WEAPONS
        | skill::FINESSE_WEAPONS
        | skill::DUAL_WIELD => Some(aug::SKILLED_MELEE),
        skill::MISSILE_WEAPONS => Some(aug::SKILLED_MISSILE),
        _ => None,
    }
}

pub use crate::movement::{get_run_rate, inq_max_run_rate, jump_velocity};

// ---------------------------------------------------------------------------------------------
// The inquiries over an object's qualities (the records need the `proto` feature).
// ---------------------------------------------------------------------------------------------

#[cfg(feature = "proto")]
pub use inquiries::*;

#[cfg(feature = "proto")]
mod inquiries {
    use super::{aug, get_run_rate, skill, skilled_augmentation_for, Sac};
    use crate::attributes::{inq_attribute, inq_current_vital, skill_formula_calculate, vital};
    use crate::quality::QualityRead;
    use dereth_assets::tables::SkillTable;

    /// The skill base level — the attribute contribution.
    ///
    /// A skill whose `_sac` is below the `SkillBase::_min_level` evaluates to **0**, not to the
    /// formula's value.
    #[must_use]
    pub fn inq_skill_base_level<Q: QualityRead + ?Sized>(
        q: &Q,
        t: &SkillTable,
        id: u32,
        raw: bool,
    ) -> Option<u32> {
        let base = t.skills.get(&id)?;
        let sac = q.skill(id).map_or(Sac::Untrained, |s| Sac::from_raw(s.sac));
        if (sac as u32) < base.min_level {
            return Some(0);
        }
        let a1 = if base.formula.attr1 == 0 {
            0
        } else {
            inq_attribute(q, base.formula.attr1, raw).unwrap_or(0)
        };
        let a2 = if base.formula.attr2 == 0 {
            0
        } else {
            inq_attribute(q, base.formula.attr2, raw).unwrap_or(0)
        };
        skill_formula_calculate(&base.formula, a1, a2)
    }

    /// Behavior: the number the panel shows.
    ///
    /// The **order** is load-bearing: base formula → `_level_from_pp +
    /// _init_level` → luminance → enlightenment (trained and specialized only; absent on a retail
    /// character) → augmentation `+10` → enchantments → JoAT `+5` → spec luminance `×2`.
    /// A multiplicative enchantment therefore multiplies the augmentation bonuses too.
    #[must_use]
    pub fn inq_skill<Q: QualityRead + ?Sized>(
        q: &Q,
        t: &SkillTable,
        id: u32,
        raw: bool,
    ) -> Option<u32> {
        let mut v = i64::from(inq_skill_base_level(q, t, id, raw)?);
        let s = q.skill(id);
        if let Some(s) = s {
            v += i64::from(s.level_from_pp) + i64::from(s.init_level);
        }
        let lum_all = q.inq_int(aug::LUM_ALL_SKILLS);
        if lum_all > 0 {
            v += i64::from(lum_all);
        }
        // Enlightenment: its count on every trained or specialized skill, part of the base, so
        // the enchantment multiplier and vitae scale it.
        let enlightenment = q.inq_int(aug::ENLIGHTENMENT);
        if enlightenment > 0 && s.is_some_and(|s| Sac::from_raw(s.sac) >= Sac::Trained) {
            v += i64::from(enlightenment);
        }
        if let Some(prop) = skilled_augmentation_for(id) {
            if q.inq_int(prop) > 0 {
                v += 10;
            }
        }
        if !raw {
            v = i64::from(
                q.enchantments()
                    .enchant_skill(id, i32::try_from(v).unwrap_or(i32::MAX)),
            );
            if q.inq_int(aug::JACK_OF_ALL_TRADES) > 0 {
                v += 5;
            }
            let lum = q.inq_int(aug::LUM_SKILLED_SPEC);
            if lum > 0 && s.is_some_and(|s| Sac::from_raw(s.sac) == Sac::Specialized) {
                v += i64::from(lum) * 2;
            }
        }
        Some(u32::try_from(v.max(0)).unwrap_or(u32::MAX))
    }

    /// Behavior: only `_level_from_pp + _init_level`, the "ranks"
    /// number the panel shows next to the total.
    #[must_use]
    pub fn inq_skill_level<Q: QualityRead + ?Sized>(q: &Q, id: u32) -> u32 {
        q.skill(id)
            .map_or(0, |s| u32::from(s.level_from_pp).wrapping_add(s.init_level))
    }

    /// The skill's advancement class.
    #[must_use]
    pub fn inq_skill_advancement_class<Q: QualityRead + ?Sized>(q: &Q, id: u32) -> Sac {
        q.skill(id).map_or(Sac::Untrained, |s| Sac::from_raw(s.sac))
    }

    /// The jump-velocity inquiry's inputs. Both current stamina and the
    /// full Jump skill inquiry must succeed BEFORE zero stamina substitutes skill zero.
    /// The secondary-attribute enchantment's actual quality filter is consulted: the shipped
    /// filter excludes 4.
    #[must_use]
    pub fn inq_jump_skill<Q: QualityRead + ?Sized>(
        q: &Q,
        t: &SkillTable,
        filter: Option<&dereth_assets::tables::QualityFilter>,
    ) -> Option<i32> {
        let stamina = inq_current_vital(q, vital::STAMINA, filter)?;
        let jump = inq_skill(q, t, skill::JUMP, false)?;
        Some(if stamina == 0 {
            0
        } else {
            i32::try_from(jump).unwrap_or(i32::MAX)
        })
    }

    /// The run rate.
    ///
    /// The run-skill total is the full `inq_skill` stack, and is **forced to 0 when *current*
    /// stamina is 0** — no stamina, no running.
    ///
    /// # The stamina is this function's own inquiry, not the caller's
    ///
    /// It reads it itself: the attribute cache's secondary-attribute query with ordinal **4**,
    /// then the secondary-attribute enchantment of ordinal 4, and only then the
    /// `if (stam == 0) run = 0`. Ordinal 4's
    /// switch arm returns `_stamina->_current_level`, where the odd ordinals 1/3/5 return
    /// `_init_level + _level_from_cp` — so this is *Stamina*, not *MaxStamina*, and a character drained
    /// to zero is the live case rather than dead code. Taking the number from the caller instead was an
    /// invitation to hand it the maximum; it is read here, beside [`inq_jump_skill`], which reads it
    /// the same way for the same reason.
    #[must_use]
    pub fn inq_run_rate<Q: QualityRead + ?Sized>(
        q: &Q,
        t: &SkillTable,
        filter: Option<&dereth_assets::tables::QualityFilter>,
    ) -> Option<f32> {
        let stamina = inq_current_vital(q, vital::STAMINA, filter)?;
        let load = crate::burden::inq_load(q);
        let run = inq_skill(q, t, skill::RUN, false)?;
        let run = if stamina == 0 {
            0
        } else {
            i32::try_from(run).unwrap_or(i32::MAX)
        };
        Some(get_run_rate(load, run, 1.0))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Oracle: §3.4's switch — five skills each for magic and melee, one for missile.
    #[test]
    fn the_skilled_augmentation_switch_covers_exactly_eleven_skills() {
        for id in [0x1F, 0x20, 0x21, 0x22, 0x2B] {
            assert_eq!(
                skilled_augmentation_for(id),
                Some(aug::SKILLED_MAGIC),
                "0x{id:X}"
            );
        }
        for id in [0x29, 0x2C, 0x2D, 0x2E, 0x31] {
            assert_eq!(
                skilled_augmentation_for(id),
                Some(aug::SKILLED_MELEE),
                "0x{id:X}"
            );
        }
        assert_eq!(skilled_augmentation_for(0x2F), Some(aug::SKILLED_MISSILE));
        assert_eq!(
            skilled_augmentation_for(0x18),
            None,
            "Run gets no family bonus"
        );
    }

    /// Oracle: contract 12.4's three-row table — 799, 800 and 801. The 800 row is the shipped
    /// discontinuity: exactly 800 takes the flat 4.5, not the general expression.
    #[test]
    fn get_run_rate_is_non_monotonic_at_exactly_eight_hundred() {
        assert_eq!(get_run_rate(0.0, 800, 1.0), 4.5);
        // The general expression at 800 would give 3.2.
        assert!((((800.0f32 / 1000.0) * 11.0 + 4.0) / 4.0 - 3.2).abs() < 1e-6);

        let r799 = get_run_rate(0.0, 799, 1.0);
        let r801 = get_run_rate(0.0, 801, 1.0);
        assert!((r799 - 3.199_449).abs() < 5e-6, "799 -> {r799}");
        assert!((r801 - 3.200_549).abs() < 5e-6, "801 -> {r801}");
        assert!(r799 < 4.5 && r801 < 4.5, "800 is a spike, not a step");
        // 41 % faster than 801 (40.6 %, per the audit).
        assert!(((4.5 / r801 - 1.0) - 0.406).abs() < 0.005);
    }

    /// Oracle: the maximum run rate — `inq_max_run_rate() = get_run_rate(0, 9999, 1) = 3.6960733`.
    #[test]
    fn the_maximum_run_rate_matches_the_documented_value() {
        assert!(
            (inq_max_run_rate() - 3.696_073_3).abs() < 1e-6,
            "{}",
            inq_max_run_rate()
        );
        assert!((inq_max_run_rate() * 4.0 - 14.784_293).abs() < 1e-4);
    }

    /// Oracle: the run-rate enquiry — encumbrance enters only through `load_mod`, which multiplies
    /// the skill
    /// term but not the `+ 4.0`, so the rate can never fall below 1.0.
    #[test]
    fn encumbrance_never_takes_the_run_rate_below_one() {
        assert_eq!(get_run_rate(3.0, 400, 1.0), 1.0);
        assert!(get_run_rate(1.5, 400, 1.0) > 1.0);
        assert!(get_run_rate(1.5, 400, 1.0) < get_run_rate(0.0, 400, 1.0));
    }

    /// Oracle: the jump-velocity enquiry is `sqrt(height * 19.6)`, i.e. `v = sqrt(2gh)`;
    /// the floor height 0.35 gives `sqrt(0.35 * 19.6)`.
    #[test]
    fn jump_velocity_is_sqrt_of_two_g_h() {
        #[allow(clippy::cast_possible_truncation)]
        let floor = (f64::from(0.35f32) * 19.6).sqrt() as f32;
        assert_eq!(jump_velocity(0.0, 0, 1.0, 1.0), floor);
        assert_eq!(jump_velocity(0.0, 0, f32::NAN, 1.0), floor);
    }
}
