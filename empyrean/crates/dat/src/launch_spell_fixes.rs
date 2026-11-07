// Rules ported from ac1999 (Adam Plumb), AGPL-3.0: DatInspector/patching/migrations/release/0001-enchantment-durations.py, 0002-it160-spell-fixes.py, 0008-resistable-imperil.py
//! The spell fixes a Launch world plays by, over the 1999 spell table (V512, V513, V514).
//!
//! ac1999 patches its portal file with what the February 29, 2000 spell table changed, and the
//! server pushes the records to its clients. A Launch world that serves the plain 1999 files applies
//! the same numbers where the server reads a spell, and the files stay as they are. The rows are the
//! fields only the server acts on; a spell's formula is left alone, because a client refuses a cast
//! on the formula in its own file.
//!
//! A world whose data overlay holds the spell table has its numbers in the files, on both sides, and
//! plays that table as it reads ([`overlay_decides`]). `empyrean-import ac1999 dat-overlay` writes
//! such a table from these same rows.

use dereth_assets::tables::{spell_component_key, SpellBase, SpellTable};
use dereth_assets::Decode;
use dereth_dat::Cursor;
use dereth_primitives::DataId;
use empyrean_entity::enums::{MagicSchool, SpellFlags, SpellType};

use crate::dat_manager::file_id;
use crate::database::DatDatabase;

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

/// Whether the world's data overlay holds the spell table `portal` reads, so the table already says
/// what a world plays, at the commit its overlay was distilled from, and [`apply`] must not run over
/// it. The ramp in particular reads the table's own number to tell the attribute families apart, so
/// it would double those it had already halved.
#[must_use]
pub fn overlay_decides(portal: &DatDatabase) -> bool {
    portal
        .overlay()
        .is_some_and(|layer| layer.touches(DataId(file_id::SPELL_TABLE)))
}

/// `spell` as a Launch world over the plain 1999 table plays it.
pub fn apply(spell: &mut SpellBase) {
    durations(spell);
    range_and_power(spell);
    resistable(spell);
}

/// An enchantment's duration by the level in its name (V512).
pub fn durations(spell: &mut SpellBase) {
    let (_, level) = family_and_level(&spell.name);

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
}

/// The shortened life spells' range and the harder spells' power (V513).
pub fn range_and_power(spell: &mut SpellBase) {
    let (family, level) = family_and_level(&spell.name);

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
}

/// The resistable flag on the spells a target may now resist (V514).
pub fn resistable(spell: &mut SpellBase) {
    let (family, level) = family_and_level(&spell.name);

    if level.is_some() && RESISTABLE_FAMILIES.contains(&family) {
        spell.bitfield |= SpellFlags::Resistable.0.cast_unsigned();
    }
}

/// Which fixes [`patch_table`] writes into the table.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
#[allow(clippy::struct_excessive_bools)] // one switch per fix
pub struct TableFixes {
    /// [`durations`].
    pub durations: bool,
    /// [`range_and_power`].
    pub range_and_power: bool,
    /// The formulas and texts the February 29, 2000 table fixed, which only the client reads.
    pub formulas_and_text: bool,
    /// [`resistable`].
    pub resistable: bool,
}

impl TableFixes {
    /// Every fix.
    pub const ALL: Self = Self {
        durations: true,
        range_and_power: true,
        formulas_and_text: true,
        resistable: true,
    };
}

fn read_u32(bytes: &[u8], at: usize) -> Result<u32, String> {
    bytes
        .get(at..at + 4)
        .map(|b| u32::from_le_bytes([b[0], b[1], b[2], b[3]]))
        .ok_or_else(|| format!("the record ends before offset {at}"))
}

/// A spell whose formula names one component instead of another.
const FORMULA_EDITS: [(&str, u32, u32); 7] = [
    // Took the level one scarab.
    ("Health to Stamina Self II", 1, 2),
    ("Lure Blade I", 48, 46),
    ("Lure Blade II", 48, 46),
    ("Lure Blade III", 48, 46),
    ("Lure Blade IV", 48, 46),
    ("Lure Blade V", 48, 46),
    ("Lure Blade VI", 48, 46),
];

/// A spell whose text says another number of points: its effect always was this one's.
const TEXT_EDITS: [(&str, &str, &str); 5] = [
    ("Swift Killer II", "by 25 points", "by 20 points"),
    ("Swift Killer III", "by 40 points", "by 30 points"),
    ("Swift Killer IV", "by 60 points", "by 40 points"),
    ("Swift Killer V", "by 80 points", "by 50 points"),
    ("Swift Killer VI", "by 100 points", "by 60 points"),
];

/// Where a spell's fields lie in the table's bytes.
#[derive(Debug)]
struct SpellPlace {
    id: u32,
    description: usize,
    fields: usize,
    duration: Option<usize>,
    formula: usize,
}

/// Past a string stored at `at`: a 16-bit length, then the bytes, then zeros to a 4-byte boundary.
fn past_string(bytes: &[u8], at: usize) -> Result<usize, String> {
    let n = bytes
        .get(at..at + 2)
        .map(|b| usize::from(u16::from_le_bytes([b[0], b[1]])))
        .ok_or("the spell table ends inside a string")?;
    Ok((at + 2 + n + 3) & !3)
}

/// Each spell's place, for spell records that end `tail` bytes after their recovery fields.
fn spell_places_with(bytes: &[u8], tail: usize) -> Result<Vec<SpellPlace>, String> {
    let count = usize::try_from(read_u32(bytes, 4)? & 0xFFFF).map_err(|e| e.to_string())?;
    let mut at = 8;
    let mut places = Vec::with_capacity(count);
    for _ in 0..count {
        let id = read_u32(bytes, at)?;
        let description = past_string(bytes, at + 4)?;
        let fields = past_string(bytes, description)?;
        let meta = read_u32(bytes, fields + 44)?;
        let after_meta = fields + 52;
        let (duration, formula) = match meta {
            1 | 12 => (Some(after_meta), after_meta + 16),
            7 => (None, after_meta + 8),
            _ => (None, after_meta),
        };
        places.push(SpellPlace {
            id,
            description,
            fields,
            duration,
            formula,
        });
        at = formula + 32 + 24 + tail;
    }
    if at == bytes.len() {
        Ok(places)
    } else {
        Err(format!("the table has {} bytes, not {at}", bytes.len()))
    }
}

/// Each spell's place. Nothing in a table says how its records end: of the three ways a table from
/// before Throne of Destiny has, the one that ends on its last byte is the one.
fn spell_places(bytes: &[u8]) -> Result<Vec<SpellPlace>, String> {
    let mut last = String::new();
    for tail in [12, 4, 0] {
        match spell_places_with(bytes, tail) {
            Ok(places) => return Ok(places),
            Err(e) => last = e,
        }
    }
    Err(last)
}

fn nibble_swapped(bytes: &[u8]) -> Vec<u8> {
    bytes.iter().map(|b| b.rotate_left(4)).collect()
}

fn decode_table(bytes: &[u8]) -> Result<SpellTable, String> {
    let mut c = Cursor::new(bytes);
    let table = SpellTable::decode_classic(&mut c).map_err(|e| e.to_string())?;
    Ok(table)
}

/// `spell` as `fixes` make it.
fn spell_as(before: &SpellBase, fixes: TableFixes) -> Result<SpellBase, String> {
    let mut after = before.clone();
    if fixes.durations {
        durations(&mut after);
    }
    if fixes.range_and_power {
        range_and_power(&mut after);
    }
    if fixes.resistable {
        resistable(&mut after);
    }
    if !fixes.formulas_and_text {
        return Ok(after);
    }
    let mut components = before.comps.clone();
    for (_, from, to) in FORMULA_EDITS.iter().filter(|(n, _, _)| *n == before.name) {
        let slot = components
            .iter_mut()
            .find(|c| **c == *from)
            .ok_or_else(|| format!("{}'s formula has no component {from}", before.name))?;
        *slot = *to;
    }
    if let Some((_, from, to)) = TEXT_EDITS.iter().find(|(n, _, _)| *n == before.name) {
        if !before.description.contains(from) {
            return Err(format!("{}'s text doesn't say {from:?}", before.name));
        }
        after.description = before.description.replace(from, to);
    }
    if components != before.comps || after.description != before.description {
        // The components are stored under a key made of the spell's name and text.
        if !before.name.is_ascii() || !after.description.is_ascii() {
            return Err(format!("{}'s text isn't plain", before.name));
        }
        after.comp_key = spell_component_key(before.name.as_bytes(), after.description.as_bytes());
        let mut values = components.iter();
        for slot in &mut after.raw_comps {
            if *slot != 0 {
                let value = values.next().ok_or("a formula slot with no component")?;
                *slot = value.wrapping_add(after.comp_key);
            }
        }
        after.comps = components;
    }
    Ok(after)
}

/// The spell table `bytes`, a table from before Throne of Destiny, as `fixes` make it: each field
/// written where it lies, and the whole read back to check it reads as those fields and that
/// nothing else changed. This is the table a world's data overlay carries, so a client reads the
/// same numbers as the server.
///
/// # Errors
/// The table does not read, a fix finds a spell other than as reviewed, or a text would change the
/// table's size.
pub fn patch_table(bytes: &[u8], fixes: TableFixes) -> Result<Vec<u8>, String> {
    let table = decode_table(bytes)?;
    let mut out = bytes.to_vec();
    let mut expected = table.clone();
    for place in spell_places(bytes)? {
        let before = table
            .spells
            .get(&place.id)
            .ok_or_else(|| format!("no spell {}", place.id))?;
        let after = spell_as(before, fixes)?;
        if after == *before {
            continue;
        }
        let f = place.fields;
        out[f + 12..f + 16].copy_from_slice(&after.bitfield.to_le_bytes());
        out[f + 20..f + 24].copy_from_slice(&after.base_range_constant.to_le_bytes());
        out[f + 24..f + 28].copy_from_slice(&after.base_range_mod.to_le_bytes());
        out[f + 28..f + 32].copy_from_slice(&after.power.to_le_bytes());
        if let (Some(at), Some((seconds, _, _))) = (place.duration, after.duration) {
            out[at..at + 8].copy_from_slice(&seconds.to_le_bytes());
        }
        for (i, word) in after.raw_comps.iter().enumerate() {
            let at = place.formula + 4 * i;
            out[at..at + 4].copy_from_slice(&word.to_le_bytes());
        }
        if after.description != before.description {
            write_same_size_string(&mut out, place.description, f, &after.description)?;
        }
        expected.spells.insert(place.id, after);
    }
    let read_back = decode_table(&out)?;
    if read_back != expected {
        return Err("the patched spell table doesn't read as its fields say".into());
    }
    Ok(out)
}

/// `text` as the string stored at `at`, ending at `end` as the old one did: a shorter number leaves
/// the padding to the same boundary, and a text that wouldn't fit would move every later spell.
fn write_same_size_string(out: &mut [u8], at: usize, end: usize, text: &str) -> Result<(), String> {
    let mut raw = text.as_bytes().to_vec();
    raw.push(0);
    let length = u16::try_from(raw.len()).map_err(|e| e.to_string())?;
    let body = nibble_swapped(&raw);
    let finish = at + 2 + body.len();
    if (finish + 3) & !3 != end {
        return Err(format!("{text:?} changes the table's size"));
    }
    out[at..at + 2].copy_from_slice(&length.to_le_bytes());
    out[at + 2..finish].copy_from_slice(&body);
    out[finish..end].fill(0);
    Ok(())
}
