//! The enchantment registry — duelling, culling, the `f32` application pipeline and the
//! **receipt-time rebasing**.
//!
//! Three things here are load-bearing and easy to "improve" by accident:
//!
//! * **Additive is tested before multiplicative** in the value-application step. An
//!   enchantment with both bits set is treated as additive.
//! * Every value goes through the pipeline as a **32-bit `float`**, even for `double` float
//!   properties and `long` int properties. That is where "my buffed skill is one off" comes from.
//!   `f64` here changes displayed numbers.
//! * `_start_time` and `_last_time_degraded` arrive **relative to receipt**, and the client stores
//!   `current_time + value`. The countdown is anchored to the moment the packet was *processed*,
//!   so one-way latency is silently subtracted from every enchantment for its whole life. Do not
//!   "fix" it with a server timestamp.

use dereth_primitives::num::{to_i32, to_i32_f64};
use dereth_primitives::{LocalTime, ObjectId};
use dereth_protocol::types::qualities::{
    Enchantment as ProtocolEnchantment, EnchantmentRegistry as WireRegistry, StatMod,
};

/// The enchantment type bits — `StatMod::kind`.
pub mod ench_type {
    pub const ATTRIBUTE: u32 = 0x0000_0001;
    pub const SECOND_ATT: u32 = 0x0000_0002;
    pub const INT: u32 = 0x0000_0004;
    pub const FLOAT: u32 = 0x0000_0008;
    pub const SKILL: u32 = 0x0000_0010;
    pub const BODY_DAMAGE_VALUE: u32 = 0x0000_0020;
    pub const BODY_DAMAGE_VARIANCE: u32 = 0x0000_0040;
    pub const BODY_ARMOR_VALUE: u32 = 0x0000_0080;
    /// The "which stat family" mask.
    pub const STAT_TYPES: u32 = 0x0000_00FF;
    pub const SINGLE_STAT: u32 = 0x0000_1000;
    /// Affects a *family*; `key` is ignored.
    pub const MULTIPLE_STAT: u32 = 0x0000_2000;
    pub const MULTIPLICATIVE: u32 = 0x0000_4000;
    pub const ADDITIVE: u32 = 0x0000_8000;
    pub const ATTACK_SKILLS: u32 = 0x0001_0000;
    pub const DEFENSE_SKILLS: u32 = 0x0002_0000;
    pub const MULTIPLICATIVE_DEGRADE: u32 = 0x0010_0000;
    pub const ADDITIVE_DEGRADE: u32 = 0x0020_0000;
    pub const VITAE: u32 = 0x0080_0000;
    pub const COOLDOWN: u32 = 0x0100_0000;
    pub const BENEFICIAL: u32 = 0x0200_0000;
}

/// Skill keys affected by attack enchantments.
pub const ATTACK_SKILL_IDS: [u32; 9] = [0x21, 0x22, 0x29, 0x2B, 0x2C, 0x2D, 0x2E, 0x2F, 0x31];
/// Skill keys affected by defense enchantments.
pub const DEFENSE_SKILL_IDS: [u32; 4] = [6, 7, 0x0F, 0x30];

/// One entry of the registry, with its two times already rebased onto the local clock.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Enchantment {
    /// The layered spell id: low 16 = spell id, high 16 = layer.
    pub id: u32,
    /// The category two spells duel within.
    pub spell_category: u16,
    pub power_level: i32,
    /// **Rebased on receipt**: `current_time + wire_offset`.
    pub start_time: f64,
    /// Seconds; `< 0` means permanent.
    pub duration: f64,
    pub caster: ObjectId,
    pub degrade_modifier: f32,
    pub degrade_limit: f32,
    /// Likewise rebased.
    pub last_time_degraded: f64,
    pub smod: StatMod,
    pub spell_set_id: Option<u32>,
}

impl Enchantment {
    /// Applies the two relative-time conversions.
    #[must_use]
    pub fn from_wire(e: &ProtocolEnchantment, now: LocalTime) -> Self {
        let (start, degraded) = e.rebase(now);
        // The low half-word is the spell category; the high half is the "a spell-set id follows"
        // flag, which the wire decoder has already consumed.
        #[allow(clippy::cast_possible_truncation)]
        let spell_category = (e.category_word & 0xFFFF) as u16;
        Self {
            id: e.id,
            spell_category,
            power_level: e.power_level,
            start_time: start.0,
            duration: e.duration,
            caster: e.caster,
            degrade_modifier: e.degrade_modifier,
            degrade_limit: e.degrade_limit,
            last_time_degraded: degraded.0,
            smod: e.smod,
            spell_set_id: e.spell_set_id,
        }
    }

    /// The spell id half of `_id`.
    #[must_use]
    pub fn spell_id(&self) -> u16 {
        #[allow(clippy::cast_possible_truncation)]
        {
            (self.id & 0xFFFF) as u16
        }
    }

    /// The enchantment layer half of `_id`.
    #[must_use]
    pub fn layer(&self) -> u16 {
        #[allow(clippy::cast_possible_truncation)]
        {
            (self.id >> 16) as u16
        }
    }

    /// `(start_time + duration) - now`, anchored to *processing* time.
    ///
    /// A permanent enchantment (`duration < 0`) has no meaningful remaining time; the caller must
    /// check first, exactly as the spell-duration panel does.
    #[must_use]
    pub fn remaining(&self, now: LocalTime) -> f64 {
        (self.start_time + self.duration) - now.0
    }

    #[must_use]
    pub fn is_permanent(&self) -> bool {
        self.duration < 0.0
    }

    /// Behavior: "`self` beats `other`": higher power level wins, and a tie
    /// goes to the **more recently cast**.
    #[must_use]
    pub fn beats(&self, other: &Self) -> bool {
        self.power_level > other.power_level
            || (self.power_level == other.power_level && self.start_time > other.start_time)
    }

    /// Return whether this enchantment affects attack skills.
    #[must_use]
    pub fn affects_attack_skills(&self, key: u32) -> bool {
        self.smod.kind & ench_type::ATTACK_SKILLS != 0 && ATTACK_SKILL_IDS.contains(&key)
    }

    /// Return whether this enchantment affects defense skills.
    #[must_use]
    pub fn affects_defense_skills(&self, key: u32) -> bool {
        self.smod.kind & ench_type::DEFENSE_SKILLS != 0 && DEFENSE_SKILL_IDS.contains(&key)
    }

    /// Behavior: the **only** place a value is modified.
    ///
    /// Additive first. An enchantment with both bits set is treated as additive; that is not a
    /// mistake in the transcription.
    #[must_use]
    pub fn enchant(&self, v: f32) -> Option<f32> {
        let t = self.smod.kind;
        if t & ench_type::ADDITIVE != 0 {
            Some(self.smod.value + v)
        } else if t & ench_type::MULTIPLICATIVE != 0 {
            Some(self.smod.value * v)
        } else {
            None
        }
    }

    /// The spell-totals update ignores spell ids `>= 0x8000`.
    #[must_use]
    pub fn counts_toward_spell_totals(&self) -> bool {
        self.spell_id() < 0x8000
    }
}

/// The decoded enchantment registry.
///
/// The three lists are in **head-insertion** order, which is also the order the application
/// pipeline walks them in.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct EnchantmentRegistry {
    pub mult_list: Vec<Enchantment>,
    pub add_list: Vec<Enchantment>,
    pub cooldown_list: Vec<Enchantment>,
    pub vitae: Option<Enchantment>,
    pub helpful_count: u32,
    pub harmful_count: u32,
}

impl EnchantmentRegistry {
    /// Behavior: A **cleared** header bit deletes the existing
    /// list; the wire decoder already reports that as `None`, so an absent list clears here.
    #[must_use]
    pub fn from_wire(r: &WireRegistry, now: LocalTime) -> Self {
        let conv = |l: &Option<Vec<ProtocolEnchantment>>| -> Vec<Enchantment> {
            l.as_ref()
                .map(|v| v.iter().map(|e| Enchantment::from_wire(e, now)).collect())
                .unwrap_or_default()
        };
        Self {
            mult_list: conv(&r.multiplicative),
            add_list: conv(&r.additive),
            cooldown_list: conv(&r.cooldowns),
            vitae: r.vitae.as_ref().map(|e| Enchantment::from_wire(e, now)),
            helpful_count: 0,
            harmful_count: 0,
        }
    }

    /// Behavior: `_vitae._smod.val`, or **1.0** when there is no vitae.
    #[must_use]
    pub fn vitae_value(&self) -> f32 {
        self.vitae.map_or(1.0, |v| v.smod.value)
    }

    /// Behavior: run over `_mult_list` **then** `_add_list`, with the
    /// survivors accumulated into one scratch list.
    ///
    /// Selection is by family mask and key; duelling then keeps at most one entry per spell
    /// category. Because the multiplicative list is culled first, a multiplicative candidate is
    /// *seen* before an additive one of the same category — but only one survives, so the practical
    /// effect is: all surviving multiplicatives in list order, then all surviving additives.
    #[must_use]
    pub fn cull(&self, family_mask: u32, key: u32) -> Vec<Enchantment> {
        let mut out: Vec<Enchantment> = Vec::new();
        for list in [&self.mult_list, &self.add_list] {
            for e in list {
                let t = e.smod.kind;
                if family_mask & t == 0 {
                    continue;
                }
                if t & ench_type::MULTIPLE_STAT == 0
                    && e.smod.key != key
                    && !e.affects_attack_skills(key)
                    && !e.affects_defense_skills(key)
                {
                    continue;
                }
                duel_into(&mut out, *e);
            }
        }
        out
    }

    /// Apply the culled survivors to a value, in list order, in `f32`.
    #[must_use]
    pub fn apply(&self, family_mask: u32, key: u32, value: f32) -> f32 {
        let mut v = value;
        for e in self.cull(family_mask, key) {
            if let Some(next) = e.enchant(v) {
                v = next;
            }
        }
        v
    }

    /// Apply enchantments to a primary attribute.
    ///
    /// No vitae; clamp to 1 when the raw value was below 10, else to 10; then round half up.
    /// Retail adds the final 0.5 at extended precision with no f32 rounding before the integer
    /// conversion: widen only that tail,
    /// preserving the stored f32 enchantment pipeline without rounding the sum a second time.
    #[must_use]
    pub fn enchant_attribute(&self, key: u32, raw: i32) -> i32 {
        #[allow(clippy::cast_precision_loss)]
        let mut f = self.apply(ench_type::ATTRIBUTE, key, raw as f32);
        let floor = if raw < 10 { 1.0f32 } else { 10.0f32 };
        if f < floor {
            f = floor;
        }
        to_i32_f64(f64::from(f) + 0.5)
    }

    /// Apply enchantments to a secondary attribute.
    ///
    /// **Vitae first**, before the spell enchantments; clamp to 1 when the raw value was below 5,
    /// else to 5, then round half up. As above, the final addition stays at extended precision.
    #[must_use]
    pub fn enchant_attribute_2nd(&self, key: u32, raw: i32) -> i32 {
        #[allow(clippy::cast_precision_loss)]
        let mut f = raw as f32 * self.vitae_value();
        f = self.apply(ench_type::SECOND_ATT, key, f);
        let floor = if raw < 5 { 1.0f32 } else { 5.0f32 };
        if f < floor {
            f = floor;
        }
        to_i32_f64(f64::from(f) + 0.5)
    }

    /// Apply enchantments to a skill.
    ///
    /// ```text
    /// f = value as f32;
    /// if vitae: apply vitae to f                            // applied FIRST
    /// cull the multiplicative list for (SKILL, key) into scratch
    /// cull the additive list for (SKILL, key) into scratch
    /// for (e in scratch) apply e to f;
    /// if (!(f > 0.5f)) f = 0.0f;
    /// value = to_int(f + 0.5f);                             // the SAME 0.5f, added
    /// ```
    ///
    /// Two details, both verified against retail:
    ///
    /// * The trailing `+ 0.5` rounds half up; without it the result truncates. This is invisible
    ///   on a character with no vitae and no *multiplicative* enchantment, because then `f` is
    ///   already the integer it started as. With vitae `0.95` and a raw level of 50, the client
    ///   shows **48** and a truncating version shows 47.
    /// * The zero clamp is `<=`, not `<`. Retail skips the clamp only on *greater* or
    ///   *unordered*, so a value of exactly `0.5` is zeroed
    ///   where `f < 0.5` would have kept it and then rounded it up to 1.
    ///
    /// Both constants are the float `0.5`. [`Self::enchant_attribute`] and
    /// [`Self::enchant_attribute_2nd`] add the same `+ 0.5f`.
    #[must_use]
    pub fn enchant_skill(&self, key: u32, raw: i32) -> i32 {
        #[allow(clippy::cast_precision_loss)]
        let mut f = raw as f32 * self.vitae_value();
        f = self.apply(ench_type::SKILL, key, f);
        if f <= 0.5 {
            f = 0.0;
        }
        to_i32(f + 0.5)
    }

    /// Apply enchantments to an integer quality.
    ///
    /// Every integer quality read that does not ask for the raw value passes through this, and
    /// every float one through [`Self::enchant_float`]:
    /// [`crate::quality::QualityRead::inq_int_enchanted`] and
    /// [`crate::quality::QualityRead::inq_float_enchanted`] are those reads. Each does nothing for
    /// a property the quality filter does not list: the filter is a dat file (database category
    /// `0x10000002`, subtype 3, enum `0x1000000C`, type `QUALITY_FILTER`) read by the asset layer,
    /// and `allowed` is the caller's answer from it.
    ///
    /// No vitae. Rounding: when negatives are allowed the enchanted value is truncated toward zero;
    /// otherwise a value **at or below** 0.5 becomes 0 and anything above is rounded half up, the
    /// 0.5 being added at double precision as in the attribute siblings. So 5 halved is 2 with
    /// negatives allowed and 3 without.
    #[must_use]
    pub fn enchant_int(&self, key: u32, raw: i32, allow_negative: bool, allowed: bool) -> i32 {
        if !allowed {
            return raw;
        }
        #[allow(clippy::cast_precision_loss)]
        let f = self.apply(ench_type::INT, key, raw as f32);
        if allow_negative {
            return to_i32(f);
        }
        if f <= 0.5 {
            return 0;
        }
        to_i32_f64(f64::from(f) + 0.5)
    }

    /// Behavior: the result is assigned back as a `double`,
    /// but the arithmetic happened in `f32`, so the stored value carries `f32` precision.
    #[must_use]
    pub fn enchant_float(&self, key: u32, raw: f64, allowed: bool) -> f64 {
        if !allowed {
            return raw;
        }
        #[allow(clippy::cast_possible_truncation)] // the client's own narrowing
        let f = self.apply(ench_type::FLOAT, key, raw as f32);
        f64::from(f)
    }

    /// [`Self::update_enchantment`] with the client's
    /// spell-totals update (`+1`) on the insert arm.
    pub fn update_enchantment_counted(
        &mut self,
        e: Enchantment,
        beneficial: &dyn Fn(u16) -> Option<bool>,
    ) -> bool {
        let before = self.mult_list.len() + self.add_list.len();
        let ok = self.update_enchantment(e);
        // The client's list insert counts **only** when its replace attempt answered 0 and it
        // therefore inserted at the list head; a replacement of an existing layer changes no
        // counter, and neither does the permanent-replaces-timed arm. Both leave the list length
        // unchanged, which is what this tests.
        if ok && self.mult_list.len() + self.add_list.len() > before {
            let b = beneficial(e.spell_id());
            self.update_spell_totals(&e, 1, b);
        }
        ok
    }

    /// Update one enchantment in the registry.
    ///
    /// The leading test is the client's parity check, verbatim: an enchantment must carry an **odd**
    /// number of `{Cooldown, Additive, Multiplicative}`.
    pub fn update_enchantment(&mut self, e: Enchantment) -> bool {
        let t = e.smod.kind;
        if (((t >> 9) ^ t) >> 1 ^ t) & 0x4000 == 0 {
            return false;
        }
        if t & ench_type::VITAE != 0 {
            self.vitae = Some(e);
            return true;
        }
        if t & ench_type::COOLDOWN != 0 {
            if replace_in_list(&mut self.cooldown_list, &e) {
                return true;
            }
        } else if t & ench_type::MULTIPLICATIVE != 0 {
            if replace_in_list(&mut self.mult_list, &e) {
                return true;
            }
        } else if t & ench_type::ADDITIVE != 0 && replace_in_list(&mut self.add_list, &e) {
            return true;
        }
        // Not present: add.
        let list = if t & ench_type::COOLDOWN != 0 {
            &mut self.cooldown_list
        } else if t & ench_type::MULTIPLICATIVE != 0 {
            &mut self.mult_list
        } else if t & ench_type::ADDITIVE != 0 {
            &mut self.add_list
        } else {
            return false;
        };
        // A permanent enchantment replaces an existing timed
        // entry with the same low-16 spell id; otherwise it is inserted at the head and the
        // helpful/harmful counter is bumped.
        if e.duration <= 0.0 {
            if let Some(slot) = list
                .iter_mut()
                .find(|x| x.spell_id() == e.spell_id() && x.duration > 0.0)
            {
                *slot = e;
                return true;
            }
        }
        list.insert(0, e);
        // Counting this insertion needs the spell table's `_bitfield` bit 2 to choose the counter.
        // Until a caller supplies it, count nothing rather than
        // guess: the counters drive only the buff/debuff indicator.
        true
    }

    /// Behavior: vitae, then cooldown, then mult, then add.
    ///
    /// **The counters are not maintained on this path.** The client's removal updates the spell
    /// totals (`_id & 0xFFFF`, `-1`) on every hit, and doing so needs the spell table; use
    /// [`Self::remove_enchantment_counted`] where the caller has one. This form is kept because
    /// `dereth_client_model::Qualities::remove_enchantment` is on a wire path that does not, and
    /// halving the counter is worse than leaving it.
    pub fn remove_enchantment(&mut self, layered_id: u32) -> bool {
        self.remove_enchantment_counted(layered_id, &|_| None)
    }

    /// [`Self::remove_enchantment`] with the client's
    /// spell-totals update (`-1`).
    pub fn remove_enchantment_counted(
        &mut self,
        layered_id: u32,
        beneficial: &dyn Fn(u16) -> Option<bool>,
    ) -> bool {
        if self.vitae.is_some_and(|v| v.id == layered_id) {
            self.vitae = None;
            return true;
        }
        // The vitae arm above returns without counting, which is the client's: vitae never went
        // through the spell-totals update on the way in either.
        for which in 0..3usize {
            let list = match which {
                0 => &mut self.cooldown_list,
                1 => &mut self.mult_list,
                _ => &mut self.add_list,
            };
            let Some(i) = list.iter().position(|x| x.id == layered_id) else {
                continue;
            };
            let e = list.remove(i);
            // Only the two spell lists were ever counted; a cooldown's removal still calls
            // the spell-totals update, and its `_id & 0xFFFF` is `_cooldown_id + 0x8000`, which the
            // `0x7FFF` gate rejects — so the counter cannot move. Reproduced by passing it through.
            let b = beneficial(e.spell_id());
            self.update_spell_totals(&e, -1, b);
            return true;
        }
        false
    }

    /// Behavior: the **only** writer of the two
    /// counters the buff/debuff indicator reads.
    ///
    /// ```text
    /// if (0x7fff < spellId) return 1;                       // counted as handled, no counter moves
    /// load database category 6, subtype 2, enum `0x10000005`; if absent return 0;
    /// b = the spell's base record;  if (!b) return 0;
    /// if (b.bitfield & 4) helpful_count += delta; else harmful_count += delta;
    /// return 1;
    /// ```
    ///
    /// `beneficial` is the caller's answer to the two middle lines, because the spell table lives
    /// above this crate — see `crate::magic::MagicState::spell_is_beneficial`. `None` is *both*
    /// "no table" and "the table does not know this spell", and on both the client changes no
    /// counter at all; that is why this returns rather than defaulting.
    ///
    /// This is the only writer of [`Self::helpful_count`] and [`Self::harmful_count`]; without it
    /// the indicator that reads them shows 0/0 for every character in every session.
    fn update_spell_totals(
        &mut self,
        e: &Enchantment,
        delta: i32,
        beneficial: Option<bool>,
    ) -> bool {
        if !e.counts_toward_spell_totals() {
            return true;
        }
        let Some(good) = beneficial else { return false };
        let slot = if good {
            &mut self.helpful_count
        } else {
            &mut self.harmful_count
        };
        *slot = slot.saturating_add_signed(delta);
        true
    }

    /// The client's recount runs over both lists: it zeroes the two counters, then re-counts the
    /// multiplicative and
    /// additive lists. Cooldowns and vitae are **not** counted.
    ///
    /// This is the arm [`Self::from_wire`] could not run, because the spell table reaches this
    /// crate only through the caller.
    pub fn count_spells_in_lists(&mut self, beneficial: &dyn Fn(u16) -> Option<bool>) {
        self.helpful_count = 0;
        self.harmful_count = 0;
        let all: Vec<Enchantment> = self
            .mult_list
            .iter()
            .chain(self.add_list.iter())
            .copied()
            .collect();
        for e in all {
            let b = beneficial(e.spell_id());
            self.update_spell_totals(&e, 1, b);
        }
    }

    /// Run the purge over `_mult_list` and then `_add_list`, returning `a | b`.
    ///
    /// It does **not** clear both lists outright, clear `_vitae`, or zero the two counters. The
    /// client does none of those:
    ///
    /// * it collects the `_id` of every entry whose **`_duration != -1.0`** and removes only those,
    ///   so a **permanent** enchantment survives a purge — the literal is `-1.0`, not "negative",
    ///   and the permanence helper's `< 0.0` is deliberately *not* the test used here;
    /// * `_vitae` is never touched (the purge runs the per-list purge on the two spell lists and
    ///   nothing else) — the vitae penalty surviving a dispel is the point;
    /// * the counters move by a spell-totals update (`-1`) per removed entry, so they end at the
    ///   count of the **survivors**, not at zero.
    ///
    /// Returns whether anything was removed, which is the `int` the client returns.
    pub fn purge_enchantments(&mut self, beneficial: &dyn Fn(u16) -> Option<bool>) -> bool {
        self.purge_matching(beneficial, |_| true)
    }

    /// The harmful-enchantment purge, over both lists.
    ///
    /// The extra clause is `(_smod.type & 0x2000000) == 0` — **not** beneficial — **and** the same
    /// `_duration != -1.0`. Both halves matter: without the permanence half it would purge
    /// permanent debuffs the client keeps.
    pub fn purge_bad_enchantments(&mut self, beneficial: &dyn Fn(u16) -> Option<bool>) -> bool {
        self.purge_matching(beneficial, |e| e.smod.kind & ench_type::BENEFICIAL == 0)
    }

    /// The body both purges share: collect ids from `_mult_list` and then `_add_list`, then remove
    /// them. Each removal also updates the spell totals by `-1`.
    fn purge_matching(
        &mut self,
        beneficial: &dyn Fn(u16) -> Option<bool>,
        extra: impl Fn(&Enchantment) -> bool,
    ) -> bool {
        // `_duration != -1.0`, exactly as written: the client compares against the literal.
        let doomed: Vec<Enchantment> = self
            .mult_list
            .iter()
            .chain(self.add_list.iter())
            .filter(|e| e.duration != -1.0 && extra(e))
            .copied()
            .collect();
        let mut any = false;
        for e in doomed {
            if self.remove_enchantment_counted(e.id, beneficial) {
                any = true;
            }
        }
        any
    }

    /// Returns whether `key` is still running and, if so, for how much longer.
    ///
    /// ```text
    /// for (e = _cooldown_list.head; e; e = e.next)
    ///     if ((e._id & 0xFFFF) == key) {
    ///         *out = (e._duration + e._start_time) - now;
    ///         if (*out <= 0.0) { Remove(e); return 0; }
    ///         return 1;
    ///     }
    /// return 0;
    /// ```
    ///
    /// **It matches on `_id & 0xFFFF`, the low-half spell id, not on `_smod.key`,** as retail
    /// does.
    /// For an item cooldown those are different numbers: the caller passes
    /// `_cooldown_id + 0x8000` and the server lays that value in `_id`, while `_smod.key`
    /// carries the stat a real enchantment modifies and is not meaningful for a cooldown at all
    /// (`kind::COOLDOWN` is its own bit, not a stat family).
    ///
    /// The client's expiry test is `(remaining < 0.0) != (remaining == 0.0)`, which is
    /// `remaining <= 0.0` written as a parity of two flags; the removal it does on that arm is a
    /// cache eviction with no observable effect, so it is not reproduced.
    ///
    /// This one-line correction deliberately crosses the original unit boundary: without it the
    /// cooldown wedge reads a field that is never equal to the key it is asked about and would be
    /// dead on arrival.
    #[must_use]
    pub fn on_cooldown(&self, cooldown_key: u32, now: LocalTime) -> bool {
        self.cooldown_remaining(cooldown_key, now).is_some()
    }

    /// [`Self::on_cooldown`] with the client's **out-parameter** — the seconds left,
    /// which is the only input the cooldown wedge's index has.
    #[must_use]
    pub fn cooldown_remaining(&self, cooldown_key: u32, now: LocalTime) -> Option<f64> {
        let key = cooldown_key & 0xFFFF;
        let e = self
            .cooldown_list
            .iter()
            .find(|e| u32::from(e.spell_id()) == key)?;
        let r = e.remaining(now);
        (r > 0.0).then_some(r)
    }

    /// Behavior: the union of the mult and add lists, in that order.
    #[must_use]
    pub fn enchantments_in_effect(&self) -> Vec<Enchantment> {
        let mut v = self.mult_list.clone();
        v.extend(self.add_list.iter().copied());
        v
    }
}

/// Behavior: matches on the **full layered** `_id`.
fn replace_in_list(list: &mut [Enchantment], e: &Enchantment) -> bool {
    if let Some(slot) = list.iter_mut().find(|x| x.id == e.id) {
        *slot = *e;
        true
    } else {
        false
    }
}

/// Behavior: insert into the accumulator, resolving category
/// conflicts. Only one enchantment per `_spell_category` ever contributes.
fn duel_into(out: &mut Vec<Enchantment>, e: Enchantment) {
    match out
        .iter()
        .position(|x| x.spell_category == e.spell_category)
    {
        None => out.push(e),
        Some(i) => {
            if out[i].beats(&e) {
                // The incumbent wins; the candidate is discarded.
            } else {
                out.remove(i);
                out.push(e);
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn ench(category: u16, power: i32, start: f64, kind: u32, key: u32, val: f32) -> Enchantment {
        Enchantment {
            id: u32::from(category),
            spell_category: category,
            power_level: power,
            start_time: start,
            duration: 100.0,
            caster: ObjectId(1),
            degrade_modifier: 0.0,
            degrade_limit: 0.0,
            last_time_degraded: 0.0,
            smod: StatMod {
                kind,
                key,
                value: val,
            },
            spell_set_id: None,
        }
    }

    /// One enchantment with an explicit id and duration, for the purge tests.
    fn purgeable(id: u32, duration: f64, kind: u32) -> Enchantment {
        let mut e = ench(1, 1, 0.0, kind, 5, 1.0);
        e.id = id;
        e.duration = duration;
        e
    }

    /// Every spell id counts, and every one is beneficial — the simplest non-`None` table.
    fn all_good(_: u16) -> Option<bool> {
        Some(true)
    }

    /// Oracle: retail's rule, verified 2026-09-06 — every enchantment whose duration is not
    /// exactly `-1.0` has its id collected, and the collected ids are removed.
    ///
    /// The literal is `-1.0`. It is written here directly on purpose: the permanence helper uses
    /// `duration < 0.0`, which is a different set, and a test that reached the constant through the
    /// same helper as the production code could not detect the difference.
    #[test]
    fn a_purge_keeps_permanent_enchantments_and_never_touches_vitae() {
        let vitae = purgeable(0xFFFF, -1.0, ench_type::VITAE | ench_type::MULTIPLICATIVE);
        let mut reg = EnchantmentRegistry {
            mult_list: vec![
                purgeable(1, 60.0, ench_type::MULTIPLICATIVE | ench_type::INT),
                purgeable(2, -1.0, ench_type::MULTIPLICATIVE | ench_type::INT),
            ],
            add_list: vec![
                purgeable(3, 60.0, ench_type::ADDITIVE | ench_type::INT),
                // `-2.0` is "permanent" to `is_permanent` and is **not** `-1.0`, so the client
                // purges it. This row is what separates the two readings.
                purgeable(4, -2.0, ench_type::ADDITIVE | ench_type::INT),
            ],
            cooldown_list: Vec::new(),
            vitae: Some(vitae),
            helpful_count: 4,
            harmful_count: 0,
        };
        assert!(reg.purge_enchantments(&all_good));
        assert_eq!(
            reg.mult_list.iter().map(|e| e.id).collect::<Vec<_>>(),
            vec![2]
        );
        assert_eq!(
            reg.add_list.len(),
            0,
            "-2.0 is purged; only the literal -1.0 survives"
        );
        assert!(
            reg.vitae.is_some(),
            "purge_enchantments never purges the vitae"
        );
        assert_eq!(reg.vitae_value(), 1.0, "and the value is still readable");
        assert_eq!(
            (reg.helpful_count, reg.harmful_count),
            (1, 0),
            "update_spell_totals(-1) per removal leaves the count of the survivors, not zero"
        );
        // A second purge removes nothing and says so.
        assert!(!reg.purge_enchantments(&all_good));
    }

    /// Oracle: the harmful-list purge —
    /// `((_smod.type & 0x2000000) == 0) && (_duration != -1.0)`. **Both** clauses.
    ///
    /// `0x2000000` is pinned as a literal here for the same reason as above; `ench_type::BENEFICIAL`
    /// is asserted equal to it so a wrong constant in the module shows up as this test failing
    /// rather than as silence.
    #[test]
    fn a_bad_purge_keeps_the_beneficial_and_the_permanent() {
        assert_eq!(ench_type::BENEFICIAL, 0x0200_0000);
        let good = ench_type::MULTIPLICATIVE | ench_type::INT | 0x0200_0000;
        let bad = ench_type::MULTIPLICATIVE | ench_type::INT;
        let mut reg = EnchantmentRegistry {
            mult_list: vec![
                purgeable(1, 60.0, bad),  // timed debuff  -> goes
                purgeable(2, 60.0, good), // timed buff    -> stays (beneficial)
                purgeable(3, -1.0, bad),  // permanent debuff -> stays (permanent)
            ],
            add_list: vec![purgeable(4, 60.0, ench_type::ADDITIVE | ench_type::INT)],
            cooldown_list: Vec::new(),
            vitae: None,
            helpful_count: 0,
            harmful_count: 4,
        };
        assert!(reg.purge_bad_enchantments(&|_| Some(false)));
        assert_eq!(
            reg.mult_list.iter().map(|e| e.id).collect::<Vec<_>>(),
            vec![2, 3]
        );
        assert_eq!(reg.add_list.len(), 0);
        assert_eq!(
            (reg.helpful_count, reg.harmful_count),
            (0, 2),
            "two removed of four"
        );
    }

    /// Three arms are pinned here, and the third is the one that had never
    /// run: `0x7fff < spellId` returns **1** without touching a counter, a missing table returns
    /// **0** without touching a counter, and only a real spell definition moves one — flag mask 4
    /// choosing which.
    ///
    /// `0x8000` is written as a literal; `counts_toward_spell_totals` is the symbol under test.
    #[test]
    fn the_spell_totals_gate_on_0x8000_and_on_the_table_being_present() {
        let below = purgeable(0x7FFF, 60.0, ench_type::ADDITIVE | ench_type::INT);
        let above = purgeable(0x8000, 60.0, ench_type::ADDITIVE | ench_type::INT);
        assert!(below.counts_toward_spell_totals());
        assert!(
            !above.counts_toward_spell_totals(),
            "0x8000 and up are item cooldowns"
        );

        // No table at all: the client returns 0 and changes nothing.
        let mut reg = EnchantmentRegistry::default();
        assert!(reg.update_enchantment_counted(below, &|_| None));
        assert_eq!(
            (reg.helpful_count, reg.harmful_count),
            (0, 0),
            "no table, no count"
        );

        // A table that says harmful.
        let mut reg = EnchantmentRegistry::default();
        assert!(reg.update_enchantment_counted(below, &|_| Some(false)));
        assert_eq!((reg.helpful_count, reg.harmful_count), (0, 1));
        // The same layered id again is a *replacement*, not an insert, so nothing is counted.
        assert!(reg.update_enchantment_counted(below, &|_| Some(false)));
        assert_eq!(
            (reg.helpful_count, reg.harmful_count),
            (0, 1),
            "a replace does not count"
        );
        // A second, beneficial one.
        let mut other = below;
        other.id = 0x0001_0001;
        assert!(reg.update_enchantment_counted(other, &all_good));
        assert_eq!((reg.helpful_count, reg.harmful_count), (1, 1));
        // Above the gate: inserted, but uncounted.
        assert!(reg.update_enchantment_counted(above, &all_good));
        assert_eq!(
            (reg.helpful_count, reg.harmful_count),
            (1, 1),
            "0x8000 is gated out"
        );
        // And removal takes it back off the right counter.
        assert!(reg.remove_enchantment_counted(0x0001_0001, &all_good));
        assert_eq!((reg.helpful_count, reg.harmful_count), (0, 1));

        // The recount re-derives both counters from the two spell lists.
        reg.count_spells_in_lists(&|id| Some(id == 0x7FFF));
        assert_eq!((reg.helpful_count, reg.harmful_count), (1, 0));
    }

    /// Oracle: §6. The additive test comes first, so an
    /// enchantment carrying both bits is additive.
    #[test]
    fn additive_is_tested_before_multiplicative() {
        let both = ench(
            1,
            1,
            0.0,
            ench_type::ADDITIVE | ench_type::MULTIPLICATIVE | ench_type::INT,
            5,
            3.0,
        );
        assert_eq!(
            both.enchant(10.0),
            Some(13.0),
            "additive wins the both-bits case"
        );

        let mult = ench(
            1,
            1,
            0.0,
            ench_type::MULTIPLICATIVE | ench_type::INT,
            5,
            3.0,
        );
        assert_eq!(mult.enchant(10.0), Some(30.0));

        let neither = ench(1, 1, 0.0, ench_type::INT, 5, 3.0);
        assert_eq!(neither.enchant(10.0), None);
    }

    /// Oracle: §6, — higher power wins; ties go to the later cast.
    #[test]
    fn duelling_keeps_one_per_category_highest_power_then_latest() {
        // Same category, different power. Head insertion, so the later push is first in the list.
        let mut reg = EnchantmentRegistry {
            add_list: vec![
                ench(7, 3, 10.0, ench_type::ADDITIVE | ench_type::INT, 5, 1.0),
                ench(7, 5, 5.0, ench_type::ADDITIVE | ench_type::INT, 5, 2.0),
            ],
            ..Default::default()
        };
        let survivors = reg.cull(ench_type::INT, 5);
        assert_eq!(survivors.len(), 1);
        assert_eq!(
            survivors[0].power_level, 5,
            "highest power wins regardless of list order"
        );

        // A tie is broken by start_time.
        reg.add_list = vec![
            ench(7, 5, 1.0, ench_type::ADDITIVE | ench_type::INT, 5, 1.0),
            ench(7, 5, 9.0, ench_type::ADDITIVE | ench_type::INT, 5, 2.0),
        ];
        let survivors = reg.cull(ench_type::INT, 5);
        assert_eq!(survivors.len(), 1);
        assert!(
            (survivors[0].start_time - 9.0).abs() < f64::EPSILON,
            "latest cast wins the tie"
        );

        // Different categories both survive.
        reg.add_list = vec![
            ench(7, 5, 1.0, ench_type::ADDITIVE | ench_type::INT, 5, 1.0),
            ench(8, 1, 1.0, ench_type::ADDITIVE | ench_type::INT, 5, 2.0),
        ];
        assert_eq!(reg.cull(ench_type::INT, 5).len(), 2);
    }

    /// Oracle: §6's application table — multiplicative survivors first, then additive.
    #[test]
    fn multiplicatives_are_applied_before_additives() {
        let reg = EnchantmentRegistry {
            mult_list: vec![ench(
                1,
                1,
                0.0,
                ench_type::MULTIPLICATIVE | ench_type::INT,
                5,
                2.0,
            )],
            add_list: vec![ench(
                2,
                1,
                0.0,
                ench_type::ADDITIVE | ench_type::INT,
                5,
                3.0,
            )],
            ..Default::default()
        };
        // (10 * 2) + 3, not (10 + 3) * 2.
        assert_eq!(reg.apply(ench_type::INT, 5, 10.0), 23.0);
    }

    /// Oracle: §6's selection rule — the wrong family and the wrong key are both skipped, but a
    /// `MultipleStat` enchantment ignores the key entirely.
    #[test]
    fn culling_filters_by_family_and_key() {
        let reg = EnchantmentRegistry {
            add_list: vec![
                ench(1, 1, 0.0, ench_type::ADDITIVE | ench_type::FLOAT, 5, 1.0), // wrong family
                ench(2, 1, 0.0, ench_type::ADDITIVE | ench_type::INT, 99, 1.0),  // wrong key
                ench(3, 1, 0.0, ench_type::ADDITIVE | ench_type::INT, 5, 1.0),   // match
                ench(
                    4,
                    1,
                    0.0,
                    ench_type::ADDITIVE | ench_type::INT | ench_type::MULTIPLE_STAT,
                    999,
                    1.0,
                ), // family-wide
            ],
            ..Default::default()
        };
        let s = reg.cull(ench_type::INT, 5);
        assert_eq!(s.len(), 2);
        assert_eq!(
            s.iter().map(|e| e.spell_category).collect::<Vec<_>>(),
            vec![3, 4]
        );
    }

    /// Oracle: §6, `affects_attack_skills` / `affects_defense_skills`.
    #[test]
    fn attack_and_defense_families_rescue_a_key_mismatch() {
        let mut reg = EnchantmentRegistry {
            add_list: vec![ench(
                1,
                1,
                0.0,
                ench_type::ADDITIVE | ench_type::SKILL | ench_type::ATTACK_SKILLS,
                0,
                5.0,
            )],
            ..Default::default()
        };
        assert_eq!(
            reg.cull(ench_type::SKILL, 0x2F).len(),
            1,
            "0x2F is an attack skill"
        );
        assert_eq!(
            reg.cull(ench_type::SKILL, 0x06).len(),
            0,
            "0x06 is a defence skill"
        );

        reg.add_list[0].smod.kind =
            ench_type::ADDITIVE | ench_type::SKILL | ench_type::DEFENSE_SKILLS;
        assert_eq!(reg.cull(ench_type::SKILL, 0x06).len(), 1);
        assert_eq!(reg.cull(ench_type::SKILL, 0x2F).len(), 0);
    }

    /// Oracle: the integer path's two roundings -- truncation when negatives are allowed, else a
    /// zero at or below one half and round-half-up above it.
    #[test]
    fn enchant_int_zeroes_at_one_half_rounds_half_up_and_truncates_when_negatives_are_allowed() {
        let scaled = |m: f32| EnchantmentRegistry {
            mult_list: vec![ench(
                1,
                1,
                0.0,
                ench_type::MULTIPLICATIVE | ench_type::INT,
                5,
                m,
            )],
            ..Default::default()
        };
        let half = scaled(0.5);
        assert_eq!(half.enchant_int(5, 5, false, true), 3, "2.5 rounds half up");
        assert_eq!(half.enchant_int(5, 5, true, true), 2, "2.5 truncates");
        assert_eq!(half.enchant_int(5, 1, false, true), 0, "0.5 is zeroed");
        assert_eq!(
            scaled(0.25).enchant_int(5, 3, false, true),
            1,
            "0.75 rounds up"
        );
        assert_eq!(
            scaled(0.25).enchant_int(5, 3, true, true),
            0,
            "0.75 truncates"
        );
        assert_eq!(
            scaled(-0.5).enchant_int(5, 5, true, true),
            -2,
            "a negative truncates toward zero"
        );
        assert_eq!(scaled(-0.5).enchant_int(5, 5, false, true), 0);
    }

    /// Oracle: §6's five entry points. The `f32` pipeline is the point: `f64` here gives a
    /// different integer.
    #[test]
    fn the_pipeline_runs_in_f32_and_clamps_as_documented() {
        let mut reg = EnchantmentRegistry {
            add_list: vec![ench(
                1,
                1,
                0.0,
                ench_type::ADDITIVE | ench_type::FLOAT,
                9,
                0.0,
            )],
            ..Default::default()
        };
        // The pipeline is f32 throughout, so the stored result carries f32 precision even for a
        // *double* float property. 0.1 is representable in neither width, and the f32 value is a
        // different number from the f64 one — which is what "reproduce it with f32, not f64" in
        // §13's rebuild notes actually costs.
        let out = reg.enchant_float(9, 0.1, true);
        assert_eq!(out, f64::from(0.1f32));
        assert_ne!(
            out, 0.1f64,
            "an f64 pipeline would have returned the input unchanged"
        );

        // The int path truncates through the client's own float-to-int helper.
        reg.add_list.clear();
        reg.mult_list = vec![ench(
            1,
            1,
            0.0,
            ench_type::MULTIPLICATIVE | ench_type::INT,
            5,
            0.1,
        )];
        assert_eq!(
            reg.enchant_int(5, 300, true, true),
            to_i32(0.1f32 * 300.0f32)
        );
        // A quality filter that says no leaves the value completely untouched.
        assert_eq!(reg.enchant_int(5, 300, true, false), 300);

        // The attribute clamp: floor 10 when the raw value is >= 10, floor 1 below it.
        reg.mult_list = vec![ench(
            1,
            1,
            0.0,
            ench_type::MULTIPLICATIVE | ench_type::ATTRIBUTE,
            1,
            0.0,
        )];
        assert_eq!(reg.enchant_attribute(1, 100), 10);
        assert_eq!(reg.enchant_attribute(1, 5), 1);

        // The attribute-2nd clamp: 5 / 1.
        reg.mult_list = vec![ench(
            1,
            1,
            0.0,
            ench_type::MULTIPLICATIVE | ench_type::SECOND_ATT,
            1,
            0.0,
        )];
        assert_eq!(reg.enchant_attribute_2nd(1, 100), 5);
        assert_eq!(reg.enchant_attribute_2nd(1, 4), 1);

        // The skill floor is a hard zero below 0.5, not a clamp to 1.
        reg.mult_list = vec![ench(
            1,
            1,
            0.0,
            ench_type::MULTIPLICATIVE | ench_type::SKILL,
            1,
            0.0,
        )];
        assert_eq!(reg.enchant_skill(1, 100), 0);
        // **And the boundary is `<=`, not `<`.** Retail skips the zeroing only on
        // *greater* (or unordered), so a value of exactly 0.5 is zeroed. `0.005 * 100` is 0.5 in f32; with a
        // `<` the value would survive and the trailing `+ 0.5` would then round it up to **1**.
        // The two rules are indistinguishable everywhere except at this one point.
        reg.mult_list = vec![ench(
            1,
            1,
            0.0,
            ench_type::MULTIPLICATIVE | ench_type::SKILL,
            1,
            0.005,
        )];
        assert_eq!(
            100.0f32 * 0.005f32,
            0.5,
            "the product really is the boundary"
        );
        assert_eq!(
            reg.enchant_skill(1, 100),
            0,
            "exactly 0.5 is zeroed, not rounded up to 1"
        );
    }

    /// Oracle: §6 — vitae is applied **first**, before the spell enchantments, for attribute-2nd
    /// and skills, and bypasses the cull entirely. `vitae_value` returns 1.0 with no vitae.
    #[test]
    fn vitae_multiplies_first_and_defaults_to_one() {
        let mut reg = EnchantmentRegistry::default();
        assert_eq!(reg.vitae_value(), 1.0);
        assert_eq!(reg.enchant_skill(1, 200), 200);

        reg.vitae = Some(ench(0, 0, 0.0, ench_type::VITAE, 0, 0.95));
        assert_eq!(reg.vitae_value(), 0.95);
        // 200 * 0.95 = 190 in f32.
        assert_eq!(reg.enchant_skill(1, 200), 190);

        // **The rounding, pinned as a literal.** The client skill path ends with
        // an integer conversion of `f + 0.5f`, so a product with a fractional half rounds **up**: 50 * 0.95 is
        // 47.5 and the client shows 48. A truncating conversion gives 47, and no other case here
        // could see the difference because every other case is exact in f32.
        // Written as literals on purpose -- `to_i32(50.0 * 0.95)` on the right-hand side is the
        // same-rule oracle that would agree with whichever rule the function happened to use.
        assert_eq!(
            reg.enchant_skill(1, 50),
            48,
            "47.5 rounds up, it does not truncate to 47"
        );
        assert_eq!(reg.enchant_skill(1, 51), 48, "48.45 stays 48");
        assert_eq!(reg.enchant_skill(1, 100), 95, "95.0 is exact either way");

        // Vitae first, then an additive: (200 * 0.95) + 10, not (200 + 10) * 0.95.
        reg.add_list = vec![ench(
            1,
            1,
            0.0,
            ench_type::ADDITIVE | ench_type::SKILL,
            1,
            10.0,
        )];
        assert_eq!(reg.enchant_skill(1, 200), 200);
        assert_eq!(
            (200.0f32 + 10.0f32) * 0.95f32,
            199.5,
            "the other order, for contrast"
        );
    }

    /// Both attribute tails load the stored `f32`, add 0.5 at extended precision, then
    /// convert to an integer.
    /// The expected integers are literals, not a second call to the production rounding code.
    #[test]
    #[allow(clippy::field_reassign_with_default)] // each list is set where the case needs it
    fn attribute_rounding_follows_the_native_half_up_tail() {
        let mut reg = EnchantmentRegistry::default();
        reg.mult_list = vec![ench(
            1,
            1,
            0.0,
            ench_type::MULTIPLICATIVE | ench_type::ATTRIBUTE,
            1,
            0.95,
        )];
        assert_eq!(reg.enchant_attribute(1, 50), 48, "47.5 rounds upward");
        assert_eq!(
            reg.enchant_attribute(1, 51),
            48,
            "48.45 stays below the next half"
        );
        assert_eq!(
            reg.enchant_attribute(1, 52),
            49,
            "49.4 also stays below the next half"
        );
        assert_eq!(reg.enchant_attribute(1, 53), 50, "50.35 stays 50");
        assert_eq!(
            reg.enchant_attribute(1, 70),
            67,
            "66.5 must not use ties-to-even"
        );
        reg.vitae = Some(ench(0, 0, 0.0, ench_type::VITAE, 0, 0.5));
        assert_eq!(
            reg.enchant_attribute(1, 50),
            48,
            "primaries do not apply vitae"
        );
    }

    #[test]
    #[allow(clippy::field_reassign_with_default)] // each list is set where the case needs it
    fn attribute_rounding_applies_to_secondary_enchantments_and_vitae() {
        let mut reg = EnchantmentRegistry::default();
        reg.mult_list = vec![ench(
            1,
            1,
            0.0,
            ench_type::MULTIPLICATIVE | ench_type::SECOND_ATT,
            1,
            0.95,
        )];
        assert_eq!(
            reg.enchant_attribute_2nd(1, 50),
            48,
            "secondary enchantment rounds too"
        );
        reg.mult_list.clear();
        reg.vitae = Some(ench(0, 0, 0.0, ench_type::VITAE, 0, 0.95));
        assert_eq!(
            reg.enchant_attribute_2nd(1, 50),
            48,
            "vitae-only 47.5 rounds upward"
        );
        reg.add_list = vec![ench(
            2,
            1,
            0.0,
            ench_type::ADDITIVE | ench_type::SECOND_ATT,
            1,
            10.0,
        )];
        assert_eq!(
            reg.enchant_attribute_2nd(1, 50),
            58,
            "vitae precedes the additive spell"
        );
    }

    /// Retail's final addition is not rounded to f32 before the integer conversion. Rounding that
    /// sum to f32 first changes a large exact integer, even without an enchantment.
    #[test]
    fn attribute_rounding_does_not_narrow_the_final_x87_sum() {
        let reg = EnchantmentRegistry::default();
        assert_eq!(reg.enchant_attribute(1, 16_777_215), 16_777_215);
        assert_eq!(reg.enchant_attribute_2nd(1, 16_777_215), 16_777_215);
    }

    /// Oracle: §6, the client's parity test, transcribed verbatim.
    #[test]
    fn update_enchantment_rejects_an_even_parity_type() {
        let mut reg = EnchantmentRegistry::default();
        // Neither additive, multiplicative nor cooldown: zero of the three, which is even.
        assert!(!reg.update_enchantment(ench(1, 1, 0.0, ench_type::INT, 5, 1.0)));
        // Exactly one: accepted.
        assert!(reg.update_enchantment(ench(
            1,
            1,
            0.0,
            ench_type::ADDITIVE | ench_type::INT,
            5,
            1.0
        )));
        assert_eq!(reg.add_list.len(), 1);
        // Two of the three: even, rejected.
        assert!(!reg.update_enchantment(ench(
            2,
            1,
            0.0,
            ench_type::ADDITIVE | ench_type::MULTIPLICATIVE | ench_type::INT,
            5,
            1.0
        )));
        // All three: odd, accepted.
        assert!(reg.update_enchantment(ench(
            3,
            1,
            0.0,
            ench_type::ADDITIVE | ench_type::MULTIPLICATIVE | ench_type::COOLDOWN,
            5,
            1.0
        )));
    }

    /// Oracle: the update matches the **full layered id** and inserts an unmatched entry at the
    /// head.
    #[test]
    fn update_replaces_by_layered_id_and_otherwise_inserts_at_the_head() {
        let mut reg = EnchantmentRegistry::default();
        let mut a = ench(1, 1, 0.0, ench_type::ADDITIVE | ench_type::INT, 5, 1.0);
        a.id = 0x0001_0064; // layer 1, spell 0x64
        let mut b = a;
        b.id = 0x0002_0064; // layer 2, same spell
        b.smod.value = 9.0;
        assert!(reg.update_enchantment(a));
        assert!(reg.update_enchantment(b));
        assert_eq!(
            reg.add_list.len(),
            2,
            "a different layer is a different entry"
        );
        assert_eq!(reg.add_list[0].id, 0x0002_0064, "head insertion");

        let mut a2 = a;
        a2.smod.value = 42.0;
        assert!(reg.update_enchantment(a2));
        assert_eq!(reg.add_list.len(), 2);
        assert_eq!(
            reg.add_list
                .iter()
                .find(|e| e.id == 0x0001_0064)
                .unwrap()
                .smod
                .value,
            42.0
        );
    }

    /// Oracle: track spec trap 3 / contract 8.17. The client stores `current_time + offset`, so
    /// a packet processed 0.4 s late permanently loses 0.4 s of the enchantment.
    #[test]
    fn remaining_is_anchored_to_processing_time_not_send_time() {
        let wire = ProtocolEnchantment {
            id: 0x0001_0064,
            category_word: 7,
            power_level: 3,
            // "started 2 seconds ago"
            start_time: -2.0,
            duration: 30.0,
            caster: ObjectId(1),
            degrade_modifier: 0.0,
            degrade_limit: 0.0,
            last_time_degraded: -2.0,
            smod: StatMod {
                kind: ench_type::ADDITIVE | ench_type::SKILL,
                key: 1,
                value: 5.0,
            },
            spell_set_id: None,
        };
        // Processed at t = 100.0.
        let e = Enchantment::from_wire(&wire, LocalTime(100.0));
        assert_eq!(e.start_time, 98.0);
        assert_eq!(e.last_time_degraded, 98.0);
        assert_eq!(e.remaining(LocalTime(100.0)), 28.0);
        assert_eq!(e.remaining(LocalTime(110.0)), 18.0);

        // The same packet processed 0.4 s later shows 0.4 s less at the same wall-clock instant.
        let late = Enchantment::from_wire(&wire, LocalTime(100.4));
        assert!(
            (late.remaining(LocalTime(110.0)) - 18.4).abs() < 1e-12,
            "latency is added, not subtracted: the countdown started later so more remains at the \
             same instant — and the *player* sees the loss because the spell also ends later than \
             the server thinks"
        );
        assert_eq!(late.start_time, 100.4 - 2.0);
    }

    /// Oracle: §6's registry-maintenance paragraph — a permanent enchantment replaces a *timed*
    /// entry with the same low-16 spell id.
    #[test]
    fn a_permanent_enchantment_replaces_a_timed_one_of_the_same_spell() {
        let mut reg = EnchantmentRegistry::default();
        let mut timed = ench(1, 1, 0.0, ench_type::ADDITIVE | ench_type::INT, 5, 1.0);
        timed.id = 0x0001_0064;
        timed.duration = 60.0;
        assert!(reg.update_enchantment(timed));

        let mut perm = timed;
        perm.id = 0x0009_0064; // a different layer, so the id match fails
        perm.duration = -1.0;
        perm.smod.value = 7.0;
        assert!(reg.update_enchantment(perm));
        assert_eq!(
            reg.add_list.len(),
            1,
            "the permanent one replaced the timed one"
        );
        assert_eq!(reg.add_list[0].smod.value, 7.0);
    }
}
