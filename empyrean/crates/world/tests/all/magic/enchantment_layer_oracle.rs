//! Enchantments are ranked by category and power, then by start time.
//! ACE considers aura and set-spell keys, compares additive and multiplicative entries separately,
//! and keeps the first equal entry. Retail compares both kinds together, multiplicative first,
//! ignores aura and set-spell keys, and keeps the later equal entry (V250, V251, V276).
//! Fixture: independent rankings of the recorded enchantment entries.

use std::cmp::Ordering;

use empyrean_entity::enums::EnchantmentTypeFlags as F;
use empyrean_entity::models::properties_enchantment_registry::PropertiesEnchantmentRegistry as E;

/// An entry with the additive bit counts as additive, whatever else it carries.
pub fn is_additive(e: &E) -> bool {
    e.stat_mod_type.0 & F::Additive.0 != 0
}

/// ACE's ranking keys, in order: power, then its keys `aura` and `set_spell` (a set spell's id
/// replaces its start time), then start time. Retail's order ignores them (V276).
pub struct Keys<'k> {
    pub aura: &'k dyn Fn(i32) -> bool,
    pub set_spell: &'k dyn Fn(i32) -> bool,
}

impl Keys<'_> {
    /// No aura or set spells among the entries.
    pub const NONE: Keys<'static> = Keys {
        aura: &|_| false,
        set_spell: &|_| false,
    };

    fn cmp(&self, a: &E, b: &E) -> Ordering {
        let third = |e: &E| {
            if (self.set_spell)(e.spell_id) {
                f64::from(e.spell_id)
            } else {
                e.start_time
            }
        };
        a.power_level
            .cmp(&b.power_level)
            .then_with(|| (self.aura)(a.spell_id).cmp(&(self.aura)(b.spell_id)))
            .then_with(|| third(a).partial_cmp(&third(b)).unwrap_or(Ordering::Equal))
    }
}

/// The survivor of each category among `entries` (one query's candidates, in registry order), in
/// order of each category's first appearance in the walk.
pub fn survivors<'a>(entries: &[&'a E], retail: bool, keys: &Keys<'_>) -> Vec<&'a E> {
    let keys = if retail { &Keys::NONE } else { keys };
    let walk: Vec<&E> = if retail {
        entries
            .iter()
            .copied()
            .filter(|e| !is_additive(e))
            .chain(entries.iter().copied().filter(|e| is_additive(e)))
            .collect()
    } else {
        entries.to_vec()
    };
    let mut best: Vec<&E> = Vec::new();
    for e in walk {
        match best
            .iter_mut()
            .find(|b| b.spell_category == e.spell_category)
        {
            Some(b) => {
                let ord = keys.cmp(e, b);
                if ord == Ordering::Greater || (retail && ord == Ordering::Equal) {
                    *b = e;
                }
            }
            None => best.push(e),
        }
    }
    best
}

/// The additive sum and multiplicative product the attribute getters form over one query's
/// candidates: ACE's with each kind queried on its own, or retail's with one duel.
pub fn attribute_mods(candidates: &[&E], retail: bool, keys: &Keys<'_>) -> (i32, f32) {
    let (add, mul): (Vec<&E>, Vec<&E>) = if retail {
        survivors(candidates, true, keys)
            .into_iter()
            .partition(|e| is_additive(e))
    } else {
        let adds: Vec<&E> = candidates
            .iter()
            .copied()
            .filter(|e| is_additive(e))
            .collect();
        let muls: Vec<&E> = candidates
            .iter()
            .copied()
            .filter(|e| e.stat_mod_type.0 & F::Multiplicative.0 != 0)
            .collect();
        (survivors(&adds, false, keys), survivors(&muls, false, keys))
    };
    #[allow(clippy::cast_possible_truncation)]
    let sum = add
        .iter()
        .fold(0i32, |s, e| s.wrapping_add(e.stat_mod_value as i32));
    let product = mul.iter().fold(1.0f32, |p, e| p * e.stat_mod_value);
    (sum, product)
}
