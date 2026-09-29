// Ported from ACE (ACEmulator), AGPL-3.0: Source/ACE.DatLoader/FileTypes/SpellTable.cs, Source/ACE.DatLoader/Entity/SpellBase.cs, Source/ACE.DatLoader/FileTypes/SpellComponentsTable.cs
//! Spell formulas and spell words: ACE's `SpellTable.ComputeHash`/`GetSpellFormula`,
//! `SpellBase.DecryptFormula`/`GetSpellWords` and `SpellComponentsTable.GetSpellWords`.
//!
//! The shared decoder keeps each spell's eight raw component slots and its own decryption; ACE's
//! `SpellBase.Formula` is re-derived here from the raw slots with ACE's arithmetic, including its
//! `& 0xFF` repair for component ids above 198.

use dereth_assets::tables::SpellBase;
use dereth_assets::{SpellComponentTable, SpellTable};

use super::AceThrow;

/// ACE's `SpellComponentsTable.Type` values. ACE gives `TalismanPea` and `TaperPea` the values of
/// `Talisman` and `PotionPea`.
pub mod component_type {
    pub const SCARAB: u32 = 1;
    pub const HERB: u32 = 2;
    pub const POWDER: u32 = 3;
    pub const POTION: u32 = 4;
    pub const TALISMAN: u32 = 5;
    pub const TAPER: u32 = 6;
    pub const POTION_PEA: u32 = 7;
    pub const TALISMAN_PEA: u32 = 5;
    pub const TAPER_PEA: u32 = 7;
}

/// "Essence of Kemeroi", for Void spells: the highest component id.
const HIGHEST_COMP_ID: u32 = 198;
/// The lowest taper id in the component table (Red Taper).
const LOWEST_TAPER_ID: u32 = 63;

/// A hash of a string's cp1252 bytes taken as **signed** chars. Used to decrypt spell formulas
/// and to derive a player's personal taper choice from the account name.
// ACE: SpellTable.ComputeHash
#[must_use]
pub fn compute_hash(str_to_hash: &str) -> u32 {
    let mut result: i64 = 0;
    if !str_to_hash.is_empty() {
        let str = dereth_primitives::text::cp1252::encode_lossy_chars(str_to_hash);
        for b in str {
            let c = i64::from(b as i8);
            result = c.wrapping_add(result << 4);
            if result & 0xF000_0000 != 0 {
                result = (result ^ ((result & 0xF000_0000) >> 24)) & 0x0FFF_FFFF;
            }
        }
    }
    #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)] // C#'s (uint)long
    let r = result as u32;
    r
}

/// ACE's `SpellBase.DecryptFormula`: subtract the name/description key from each non-zero raw
/// slot, repairing any result above the highest component id with `& 0xFF`.
// ACE: SpellBase.DecryptFormula
#[must_use]
pub fn decrypt_formula(raw_comps: &[u32], name: &str, desc: &str) -> Vec<u32> {
    let mut comps = Vec::new();
    let name_hash = compute_hash(name);
    let desc_hash = compute_hash(desc);
    let key = (name_hash % 0x1210_7680).wrapping_add(desc_hash % 0xBEAD_CF45);
    for raw in raw_comps {
        let mut comp = raw.wrapping_sub(key);
        // This seems to correct issues with certain spells with extended characters.
        if comp > HIGHEST_COMP_ID {
            comp &= 0xFF;
        }
        comps.push(comp);
    }
    comps
}

/// ACE's `SpellBase` members that are not plain fields.
pub trait SpellBaseExt {
    /// ACE's `SpellBase.Formula`: the decrypted non-zero component slots.
    fn formula(&self) -> Vec<u32>;

    /// ACE's `SpellBase.GetSpellWords`: the spoken words for this spell's formula.
    ///
    /// # Errors
    ///
    /// A formula component is not in `comps` (ACE throws `KeyNotFoundException`).
    fn get_spell_words(&self, comps: &SpellComponentTable) -> Result<String, AceThrow>;
}

impl SpellBaseExt for SpellBase {
    fn formula(&self) -> Vec<u32> {
        // SpellBase.Unpack: "We will only add the comp if it is valid" (comp > 0).
        let raw: Vec<u32> = self.raw_comps.iter().copied().filter(|c| *c > 0).collect();
        decrypt_formula(&raw, &self.name, &self.description)
    }

    // ACE: SpellBase.GetSpellWords
    fn get_spell_words(&self, comps: &SpellComponentTable) -> Result<String, AceThrow> {
        // ACE caches the words on the SpellBase; they are a pure function of the formula.
        get_spell_words(comps, Some(&self.formula()))
    }
}

/// ACE's `SpellComponentsTable.GetSpellWords`: the herb's word, then the powder's word followed by
/// the lower-cased potion word with its first letter capitalised.
///
/// # Errors
///
/// A formula component is not in `comps` (ACE throws `KeyNotFoundException`).
// ACE: SpellComponentsTable.GetSpellWords
pub fn get_spell_words(
    comps: &SpellComponentTable,
    formula: Option<&[u32]>,
) -> Result<String, AceThrow> {
    let mut first_spell_word = String::new();
    let mut second_spell_word = String::new();
    let mut third_spell_word = String::new();

    let Some(formula) = formula else {
        return Ok(String::new());
    };

    let lookup = |id: u32| comps.components.get(&id).ok_or(AceThrow::KeyNotFound(id));

    // Locate the herb component in the Spell formula
    for &f in formula {
        let c = lookup(f)?;
        if c.component_type == component_type::HERB {
            first_spell_word.clone_from(&c.text);
        }
    }
    // Locate the powder component in the Spell formula
    for &f in formula {
        let c = lookup(f)?;
        if c.component_type == component_type::POWDER {
            second_spell_word.clone_from(&c.text);
        }
    }
    // Locate the potion component in the Spell formula
    for &f in formula {
        let c = lookup(f)?;
        if c.component_type == component_type::POTION {
            third_spell_word.clone_from(&c.text);
        }
    }

    let mut second_spell_word_set = second_spell_word + &third_spell_word.to_lowercase();
    if !second_spell_word_set.is_empty() {
        let mut chars = second_spell_word_set.chars();
        let first_letter: String = chars
            .next()
            .map(|c| c.to_uppercase().collect())
            .unwrap_or_default();
        second_spell_word_set = first_letter + chars.as_str();
    }

    let result = format!("{first_spell_word} {second_spell_word_set}");
    Ok(result.trim().to_owned())
}

/// ACE's `SpellTable.GetSpellFormula`: the formula as this account casts it. Formula versions 1–3
/// replace the taper slots with ones derived from the account name.
///
/// # Errors
///
/// `spell_id` is not in the table ([`AceThrow::KeyNotFound`]), or the formula is too short or the
/// arithmetic divides by zero where ACE throws.
// ACE: SpellTable.GetSpellFormula
pub fn get_spell_formula(
    spell_table: &SpellTable,
    spell_id: u32,
    account_name: &str,
) -> Result<Vec<u32>, AceThrow> {
    let spell = spell_table
        .spells
        .get(&spell_id)
        .ok_or(AceThrow::KeyNotFound(spell_id))?;
    match spell.formula_version {
        1 => randomize_version1(spell, account_name),
        2 => randomize_version2(spell, account_name),
        3 => randomize_version3(spell, account_name),
        _ => Ok(spell.formula()),
    }
}

fn at(comps: &[u32], i: usize) -> Result<u32, AceThrow> {
    comps
        .get(i)
        .copied()
        .ok_or(AceThrow::IndexOutOfRange(i as u64))
}

fn set(comps: &mut [u32], i: usize, v: u32) -> Result<(), AceThrow> {
    *comps
        .get_mut(i)
        .ok_or(AceThrow::IndexOutOfRange(i as u64))? = v;
    Ok(())
}

fn div(a: u32, b: u32) -> Result<u32, AceThrow> {
    a.checked_div(b).ok_or(AceThrow::DivideByZero)
}

// ACE: SpellTable.RandomizeVersion1
fn randomize_version1(spell: &SpellBase, account_name: &str) -> Result<Vec<u32>, AceThrow> {
    let mut comps = spell.formula();
    let mut has_taper1 = false;
    let mut has_taper2 = false;
    let mut has_taper3 = false;

    let key = compute_hash(account_name);
    let seed = key % 0x13_D573;

    let scarab = at(&comps, 0)?;
    let mut herb_index = 1;
    if comps.len() > 5 {
        herb_index = 2;
        has_taper1 = true;
    }
    let herb = at(&comps, herb_index)?;

    let mut powder_index = herb_index + 1;
    if comps.len() > 6 {
        powder_index += 1;
        has_taper2 = true;
    }
    let powder = at(&comps, powder_index)?;

    let potion_index = powder_index + 1;
    let potion = at(&comps, potion_index)?;

    let mut talisman_index = potion_index + 1;
    if comps.len() > 7 {
        talisman_index += 1;
        has_taper3 = true;
    }
    let talisman = at(&comps, talisman_index)?;

    if has_taper1 {
        let v = powder
            .wrapping_add(herb.wrapping_mul(2))
            .wrapping_add(potion)
            .wrapping_add(talisman)
            .wrapping_add(scarab)
            % 0xC
            + LOWEST_TAPER_ID;
        set(&mut comps, 1, v)?;
    }
    if has_taper2 {
        let a = scarab
            .wrapping_add(herb)
            .wrapping_add(talisman)
            .wrapping_add(powder.wrapping_add(potion).wrapping_mul(2));
        let b = div(seed, scarab.wrapping_add(powder.wrapping_add(potion)))?;
        set(&mut comps, 3, a.wrapping_mul(b) % 0xC + LOWEST_TAPER_ID)?;
    }
    if has_taper3 {
        let a = powder
            .wrapping_add(talisman.wrapping_mul(2))
            .wrapping_add(potion)
            .wrapping_add(herb)
            .wrapping_add(scarab);
        let b = div(seed, talisman.wrapping_add(scarab))?;
        set(&mut comps, 6, a.wrapping_mul(b) % 0xC + LOWEST_TAPER_ID)?;
    }
    Ok(comps)
}

// ACE: SpellTable.RandomizeVersion2
fn randomize_version2(spell: &SpellBase, account_name: &str) -> Result<Vec<u32>, AceThrow> {
    let mut comps = spell.formula();

    let key = compute_hash(account_name);
    let seed = key % 0x13_D573;

    let p1 = at(&comps, 0)?;
    let c = at(&comps, 4)?;
    let x = at(&comps, 5)?;
    let a = at(&comps, 7)?;

    let (c0, c1, c2) = (at(&comps, 0)?, at(&comps, 1)?, at(&comps, 2)?);
    let v3 = a
        .wrapping_add(c0.wrapping_mul(2))
        .wrapping_add(c.wrapping_mul(2).wrapping_mul(x))
        .wrapping_add(c0)
        .wrapping_add(c2)
        .wrapping_add(c1)
        % 0xC
        + LOWEST_TAPER_ID;
    set(&mut comps, 3, v3)?;

    let c1 = at(&comps, 1)?;
    let c2 = at(&comps, 2)?;
    let lhs = a
        .wrapping_add(p1.wrapping_mul(2).wrapping_mul(c2))
        .wrapping_add(x.wrapping_mul(2))
        .wrapping_add(p1.wrapping_mul(c2))
        .wrapping_add(c);
    let rhs = div(seed, c1.wrapping_mul(a).wrapping_add(c.wrapping_mul(2)))?;
    set(&mut comps, 6, lhs.wrapping_mul(rhs) % 0xC + LOWEST_TAPER_ID)?;
    Ok(comps)
}

// ACE: SpellTable.RandomizeVersion3
fn randomize_version3(spell: &SpellBase, account_name: &str) -> Result<Vec<u32>, AceThrow> {
    let mut comps = spell.formula();

    let key = compute_hash(account_name);
    let seed1 = key % 0x13_D573;
    let seed2 = key % 0x4_AEFD;
    let seed3 = key % 0x9_6A7F;
    let seed4 = key % 0x10_0A03;
    let seed5 = key % 0xE_B2EF;
    let seed6 = key % 0x12_1E7D;

    let comp_hash0 = seed1.wrapping_add(at(&comps, 0)?) % 0xC;
    let comp_hash1 = seed2.wrapping_add(at(&comps, 1)?) % 0xC;
    let comp_hash2 = seed3.wrapping_add(at(&comps, 2)?) % 0xC;
    let comp_hash4 = seed4.wrapping_add(at(&comps, 4)?) % 0xC;
    let comp_hash5 = seed5.wrapping_add(at(&comps, 5)?) % 0xC;

    // Some spells don't have the full number of comps. 2697 ("Aerfalle's Touch"), is one example.
    let comp_hash7 = if comps.len() < 8 {
        seed6 % 0xC
    } else {
        seed6.wrapping_add(at(&comps, 7)?) % 0xC
    };

    let v3 = (comp_hash0
        + comp_hash1
        + comp_hash2
        + comp_hash4
        + comp_hash5
        + comp_hash2 * comp_hash5
        + comp_hash0 * comp_hash1
        + comp_hash7 * (comp_hash4 + 1))
        % 0xC
        + LOWEST_TAPER_ID;
    set(&mut comps, 3, v3)?;

    let v6 = (comp_hash0
        + comp_hash1
        + comp_hash2
        + comp_hash4
        + key % 0x6_5039 % 0xC
        + comp_hash7 * (comp_hash4 * (comp_hash0 * comp_hash1 * comp_hash2 * comp_hash5 + 7) + 1)
        + comp_hash5
        + 4 * comp_hash0 * comp_hash1
        + comp_hash0 * comp_hash1
        + 11 * comp_hash2 * comp_hash5)
        % 0xC
        + LOWEST_TAPER_ID;
    set(&mut comps, 6, v6)?;
    Ok(comps)
}
