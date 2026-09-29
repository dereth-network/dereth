//! Fake dats for world tests. Nothing here is ported from ACE.
//!
//! The retail portal dat always has the spell, skill and secondary attribute (vital) tables, and
//! every creature reads them: construction computes its max vitals (`CreatureVital.MaxValue`, the
//! `SecondaryAttributeTable` formulas); every enchanted stat read goes through
//! `EnchantmentManager.GetEnchantments_TopLayer`, which takes `SpellSet.SetSpells` from the spell
//! table; and a moving creature's run rate reads its Run skill (`CreatureSkill.IsUsable` and the
//! formula, from the `SkillTable`). A test that builds creatures but does not care about their
//! stats adds these neutral tables with [`with_stat_tables`].

use dereth_assets::tables::SkillFormula;
use dereth_primitives::DataId;
use empyrean_dat::dat_manager::file_id;
use empyrean_dat::file_types::{SecondaryAttributeTable, SkillTable, SpellTable};
use empyrean_dat::FakeDats;

/// An empty spell table (no spells, no spell sets).
#[must_use]
pub fn empty_spell_table() -> SpellTable {
    SpellTable {
        id: DataId(file_id::SPELL_TABLE),
        spell_buckets: 0,
        spells: std::collections::BTreeMap::new(),
        spellset_bucket_index: 1,
        spellsets: std::collections::BTreeMap::new(),
    }
}

/// An empty skill table (the dat manager still adds ACE's retired skills to it): every skill a
/// creature has not trained is unusable, so an untrained Run skill is 0 (run rate 1.0).
#[must_use]
pub fn empty_skill_table() -> SkillTable {
    SkillTable {
        id: DataId(file_id::SKILL_TABLE),
        buckets: 0,
        skills: std::collections::BTreeMap::new(),
    }
}

/// A vital table whose three formulas are off (`X = 0`): every max vital is its starting value
/// plus its ranks.
#[must_use]
pub fn zero_vital_table() -> SecondaryAttributeTable {
    let off = SkillFormula {
        w: 0,
        x: 0,
        y: 0,
        z: 1,
        attr1: 0,
        attr2: 0,
    };
    SecondaryAttributeTable {
        id: DataId(file_id::SECONDARY_ATTRIBUTE_TABLE),
        health: off,
        stamina: off,
        mana: off,
    }
}

/// `dats` plus [`empty_spell_table`], [`empty_skill_table`] and [`zero_vital_table`] (a test's
/// own tables, inserted later, replace them).
#[must_use]
pub fn with_stat_tables(dats: FakeDats) -> FakeDats {
    dats.with_spell_table(empty_spell_table())
        .with_skill_table(empty_skill_table())
        .with_portal(file_id::SECONDARY_ATTRIBUTE_TABLE, zero_vital_table())
}
