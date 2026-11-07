// Rules ported from ac1999 (Adam Plumb), AGPL-3.0: DatInspector/patching/migrations/release/0001-enchantment-durations.py, 0002-it160-spell-fixes.py, 0008-resistable-imperil.py
//! The spell fixes a Launch world plays by, over the 1999 spell table (V512, V513, V514).
//!
//! ac1999 patches its portal file with what the February 29, 2000 spell table changed, and the
//! server pushes the records to its clients. A Launch world here serves the plain 1999 files, so it
//! applies the same numbers where the server reads a spell, and the files stay as they are. The
//! rows are the fields only the server acts on; a spell's formula is left alone, because a client
//! refuses a cast on the formula in its own file.

use dereth_assets::tables::SpellBase;
use empyrean_entity::enums::{MagicSchool, SpellFlags, SpellType};

/// An enchantment's seconds by the level in its name, I to VI. The table's own value is a flat 300
/// (with a few outliers), or 150 for the attribute families, which keep half the ramp.
const ENCHANTMENT_SECONDS: [f64; 6] = [300.0, 360.0, 480.0, 600.0, 780.0, 1020.0];
/// What marks a spell of the attribute families in the 1999 table.
const ATTRIBUTE_FAMILY_SECONDS: f64 = 150.0;

/// The range per point of skill of the shortened life spells, by level, I to VI.
const SHORT_RANGE_PER_SKILL: [f32; 6] = [0.10, 0.09, 0.08, 0.07, 0.06, 0.05];
/// The life spells whose range stops growing so fast with skill, each with its range at no skill.
const SHORT_RANGE_FAMILIES: [(&str, f32); 6] = [
    ("Harm Other", 10.0),
    ("Enfeeble Other", 10.0),
    ("Mana Drain Other", 10.0),
    ("Drain Health Other", 5.0),
    ("Drain Stamina Other", 5.0),
    ("Drain Mana Other", 5.0),
];

/// Spells that are harder to cast. Rejuvenation Self VI's 10 was a slip for 250.
const POWER: [(&str, i32); 6] = [
    ("Portal Tie", 150),
    ("Summon Portal I", 200),
    ("Summon Portal II", 225),
    ("Portal Sending", 125),
    ("Lifestone Sending", 125),
    ("Rejuvenation Self VI", 250),
];

/// Spells a target may now resist with Magic Defense; every level of each.
const RESISTABLE_FAMILIES: [&str; 1] = ["Imperil Other"];

/// A spell's name without its level, and the level, 1 to 6, when the name ends in one.
fn family_and_level(name: &str) -> (&str, Option<usize>) {
    let Some((family, numeral)) = name.rsplit_once(' ') else {
        return (name, None);
    };
    match ["I", "II", "III", "IV", "V", "VI"]
        .iter()
        .position(|n| *n == numeral)
    {
        Some(i) => (family, Some(i + 1)),
        None => (name, None),
    }
}

/// `spell` as a Launch world plays it.
pub fn apply(spell: &mut SpellBase) {
    let (family, level) = family_and_level(&spell.name);

    // Vitae is the one enchantment of these schools with no level: it lasts until it is worked off.
    let enchantment = spell.meta_spell_type == SpellType::Enchantment.0.cast_unsigned()
        && [
            MagicSchool::LifeMagic,
            MagicSchool::ItemEnchantment,
            MagicSchool::CreatureEnchantment,
        ]
        .iter()
        .any(|s| s.0.cast_unsigned() == spell.school);
    if let (true, Some(level), Some((seconds, _, _))) =
        (enchantment, level, spell.duration.as_mut())
    {
        let ramp = ENCHANTMENT_SECONDS[level - 1];
        #[allow(clippy::float_cmp)] // a value stored in the table, not a computed one
        let attribute = *seconds == ATTRIBUTE_FAMILY_SECONDS;
        *seconds = if attribute { ramp / 2.0 } else { ramp };
    }

    if let (Some((_, constant)), Some(level)) = (
        SHORT_RANGE_FAMILIES.iter().find(|(f, _)| *f == family),
        level,
    ) {
        spell.base_range_constant = *constant;
        spell.base_range_mod = SHORT_RANGE_PER_SKILL[level - 1];
    }

    if let Some((_, power)) = POWER.iter().find(|(name, _)| *name == spell.name) {
        spell.power = *power;
    }

    if level.is_some() && RESISTABLE_FAMILIES.contains(&family) {
        spell.bitfield |= SpellFlags::Resistable.0.cast_unsigned();
    }
}
