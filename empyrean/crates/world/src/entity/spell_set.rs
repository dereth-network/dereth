// Ported from ACE (ACEmulator), AGPL-3.0: Source/ACE.Server/Entity/SpellSet.cs
//! Port of `Source/ACE.Server/Entity/SpellSet.cs`.
//!
//! ACE's static `SpellSet.SetSpells` is filled once by the type initializer from
//! `DatManager.PortalDat`. Here it is derived from the world's dats and memoised per
//! `DatManager` instance (tests build many worlds over different fake dats in one process); it is
//! only ever used for membership, so the set's order does not matter.

use std::sync::{Arc, Mutex, Weak};

use empyrean_common::dotnet::DotNetHashSet;
use empyrean_dat::DatManager;
use empyrean_entity::enums::SpellId;

/// The memo: the `DatManager` the set was built from, and the set.
type Memo = Option<(Weak<DatManager>, Arc<DotNetHashSet<i32>>)>;

static MEMO: Mutex<Memo> = Mutex::new(None);

/// Helper collection for spell sorting: every spell of every tier of every equipment set, from
/// `SetCoordination1` upwards (the cutoff for the enchantment manager bug fix sorting).
// ACE: SpellSet.SpellSet
#[must_use]
pub fn set_spells(dats: &Arc<DatManager>) -> Arc<DotNetHashSet<i32>> {
    let mut memo = MEMO
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner);
    if let Some((owner, set)) = memo.as_ref() {
        if owner.upgrade().is_some_and(|d| Arc::ptr_eq(&d, dats)) {
            return Arc::clone(set);
        }
    }

    let mut set_spells = DotNetHashSet::new();
    for spell_set in dats.portal_dat().spell_table().spellsets.values() {
        for tier in spell_set.tiers.values() {
            for &spell in tier {
                // cutoff for enchantment manager bug fix sorting
                if spell >= SpellId::SetCoordination1.0 {
                    set_spells.insert(spell.cast_signed());
                }
            }
        }
    }

    let set = Arc::new(set_spells);
    *memo = Some((Arc::downgrade(dats), Arc::clone(&set)));
    set
}
