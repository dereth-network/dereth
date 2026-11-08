//! `0x0013`, the player description, as the ac1999 server sends it: normalized to the later message.
//!
//! The qualities carry 1999 strings, no Int64 table (experience rides two Int keys) and combat modes
//! numbered in sequence. The player module is the short 1999 form: a selector, the options, the
//! shortcuts when the selector says so, and one spell bar.

use dereth_primitives::ObjectId;
use dereth_protocol::{
    login::{player_module_flags, LoginPlayerDescription, PlayerModule, ShortCutData},
    types::{
        qualities::{
            base_flags, quality_flags, AcBaseQualities, AcQualities, AttributeCache,
            EnchantmentRegistry, PropertyTables, Skill, SpellBookPage,
        },
        ContentProfile, InventoryPlacement, PositionWire,
    },
    Message, MessageError, Opcode, Reader, Writer,
};

use super::{invalid, read_string, write_string, MAX_NEWS};

/// The Int keys the 1999 wire carries experience on.
pub const EXPERIENCE_TOTAL_INT: u32 = 0x15;
pub const EXPERIENCE_AVAILABLE_INT: u32 = 0x16;
/// The later Int64 keys of the same values.
pub const EXPERIENCE_TOTAL_INT64: u32 = 1;
pub const EXPERIENCE_AVAILABLE_INT64: u32 = 2;
/// The Int key holding the combat mode.
pub const COMBAT_MODE_INT: u32 = 0x28;
/// The Int key holding the heritage: from zero on the 1999 wire, from one (Aluvian) on the later.
pub const HERITAGE_INT: u32 = 0xBC;

/// A 1999 enchantment id as the later wire's: the 1999 id is the spell alone, and the later one puts
/// a layer above it, starting from one. A character holds one of a spell here, so the layer is fixed.
#[must_use]
pub fn layered(id: u32) -> u32 {
    if id >> 16 == 0 {
        id | (1 << 16)
    } else {
        id
    }
}

fn layer_all(list: &mut Option<Vec<dereth_protocol::types::qualities::Enchantment>>) {
    for e in list.iter_mut().flatten() {
        e.id = layered(e.id);
    }
}

/// A 1999 combat mode (1 peace, 2 melee, 3 missile, 4 magic) as the later bit mask.
#[must_use]
pub fn combat_mode_to_later(mode: i32) -> i32 {
    match mode {
        3 => 4,
        4 => 8,
        other => other,
    }
}

/// A later combat mode bit as the 1999 number. Unknown values pass unchanged.
#[must_use]
pub fn combat_mode_from_later(mode: u32) -> u32 {
    match mode {
        4 => 3,
        8 => 4,
        other => other,
    }
}

fn read_base(r: &mut Reader<'_>) -> Result<AcBaseQualities, MessageError> {
    let flags = r.u32()?;
    let weenie_type = r.u32()?;
    let has = |b: u32| flags & b != 0;
    let mut t = PropertyTables::default();
    if has(base_flags::INT) {
        t.ints = Some(r.packed_hash(|r| Ok((r.u32()?, r.i32()?)))?);
    }
    if has(base_flags::INT64) {
        t.int64s = Some(r.packed_hash(|r| Ok((r.u32()?, r.i64()?)))?);
    }
    if has(base_flags::BOOL) {
        t.bools = Some(r.packed_hash(|r| Ok((r.u32()?, r.i32()?)))?);
    }
    if has(base_flags::FLOAT) {
        t.floats = Some(r.packed_hash(|r| Ok((r.u32()?, r.f64()?)))?);
    }
    if has(base_flags::STRING) {
        t.strings = Some(r.packed_hash(|r| Ok((r.u32()?, read_string(r, MAX_NEWS)?)))?);
    }
    if has(base_flags::DID) {
        t.dids = Some(r.packed_hash(|r| Ok((r.u32()?, r.u32()?)))?);
    }
    if has(base_flags::IID) {
        t.iids = Some(r.packed_hash(|r| Ok((r.u32()?, ObjectId(r.u32()?))))?);
    }
    if has(base_flags::POSITION) {
        t.positions = Some(r.packed_hash(|r| Ok((r.u32()?, PositionWire::read(r)?)))?);
    }
    Ok(AcBaseQualities {
        flags,
        weenie_type,
        tables: t,
    })
}

/// Experience moves to its later Int64 keys, the combat mode to its later bits, and the heritage
/// to its later number.
fn normalize(base: &mut AcBaseQualities) {
    let t = &mut base.tables;
    let Some(ints) = t.ints.as_mut() else {
        return;
    };
    let mut experience = Vec::new();
    for (int, int64) in [
        (EXPERIENCE_TOTAL_INT, EXPERIENCE_TOTAL_INT64),
        (EXPERIENCE_AVAILABLE_INT, EXPERIENCE_AVAILABLE_INT64),
    ] {
        if let Some(at) = ints.entries.iter().position(|(k, _)| *k == int) {
            experience.push((int64, i64::from(ints.entries.remove(at).1)));
        }
    }
    if let Some((_, mode)) = ints.entries.iter_mut().find(|(k, _)| *k == COMBAT_MODE_INT) {
        *mode = combat_mode_to_later(*mode);
    }
    if let Some((_, heritage)) = ints.entries.iter_mut().find(|(k, _)| *k == HERITAGE_INT) {
        *heritage = heritage.saturating_add(1);
    }
    if !experience.is_empty() {
        let table = t.int64s.get_or_insert_with(Default::default);
        table.entries.extend(experience);
        if table.table_size == 0 {
            table.table_size = 16;
        }
        base.flags |= base_flags::INT64;
    }
}

fn read_qualities(r: &mut Reader<'_>) -> Result<AcQualities, MessageError> {
    use quality_flags as f;
    let mut base = read_base(r)?;
    normalize(&mut base);
    let flags = r.u32()?;
    let has_health = r.u32()?;
    let has = |b: u32| flags & b != 0;
    let mut q = AcQualities {
        base,
        flags,
        has_health,
        ..AcQualities::default()
    };
    if has(f::ATTRIBUTE_CACHE) {
        q.attribute_cache = Some(AttributeCache::read(r)?);
    }
    if has(f::SKILLS) {
        q.skills = Some(r.packed_hash(|r| {
            let key = r.u32()?;
            let mut skill = Skill::read(r)?;
            skill.format_version = 1;
            Ok((key, skill))
        })?);
    }
    if has(f::BODY) {
        return Err(invalid("ac1999 qualities body"));
    }
    if has(f::SPELL_BOOK) {
        q.spell_book = Some(r.packed_hash(|r| Ok((r.u32()?, SpellBookPage::read(r)?)))?);
    }
    if has(f::ENCHANTMENT_REGISTRY) {
        let mut registry = EnchantmentRegistry::read(r)?;
        layer_all(&mut registry.multiplicative);
        layer_all(&mut registry.additive);
        layer_all(&mut registry.cooldowns);
        if let Some(vitae) = registry.vitae.as_mut() {
            vitae.id = layered(vitae.id);
        }
        q.enchantments = Some(registry);
    }
    if flags
        & (f::EVENT_FILTER
            | f::EMOTE_TABLE
            | f::CREATION_PROFILE
            | f::PAGE_DATA_LIST
            | f::GENERATOR_TABLE
            | f::GENERATOR_REGISTRY
            | f::GENERATOR_QUEUE)
        != 0
    {
        return Err(invalid("ac1999 qualities tables"));
    }
    Ok(q)
}

fn read_module(r: &mut Reader<'_>) -> Result<PlayerModule, MessageError> {
    let selector = r.u32()?;
    let options = r.u32()?;
    let shortcuts = if selector & player_module_flags::SHORTCUT != 0 {
        Some(r.packed_list(ShortCutData::read)?)
    } else {
        None
    };
    let bar = r.packed_list(Reader::u32)?;
    Ok(PlayerModule {
        option_flags: selector & player_module_flags::SHORTCUT,
        options,
        shortcuts,
        spell_bars: vec![bar],
        ..PlayerModule::default()
    })
}

/// `0x0013`, normalized to the later message.
#[derive(Debug, Clone, PartialEq, Default)]
pub struct PlayerDescription(pub LoginPlayerDescription);

impl Message for PlayerDescription {
    const OPCODE: Opcode = Opcode(0x0013);

    fn read(r: &mut Reader<'_>) -> Result<Self, MessageError> {
        Ok(Self(LoginPlayerDescription {
            qualities: read_qualities(r)?,
            player_module: read_module(r)?,
            content_profiles: r.packed_list(ContentProfile::read)?,
            inventory_placements: r.packed_list(InventoryPlacement::read)?,
        }))
    }

    /// The stand-in server's half: only what a test session needs.
    fn write(&self, w: &mut Writer) -> Result<(), MessageError> {
        let q = &self.0.qualities;
        w.u32(q.base.flags);
        w.u32(q.base.weenie_type);
        if let Some(ints) = &q.base.tables.ints {
            w.packed_hash(ints, |w, k, v| {
                w.u32(*k);
                w.i32(*v);
                Ok(())
            })?;
        }
        if let Some(strings) = &q.base.tables.strings {
            w.packed_hash(strings, |w, k, v| {
                w.u32(*k);
                write_string(w, v, MAX_NEWS)
            })?;
        }
        w.u32(q.flags);
        w.u32(q.has_health);
        if let Some(a) = &q.attribute_cache {
            a.write(w);
        }
        if let Some(book) = &q.spell_book {
            w.packed_hash(book, |w, k, v| {
                w.u32(*k);
                v.write(w);
                Ok(())
            })?;
        }
        let m = &self.0.player_module;
        w.u32(m.option_flags);
        w.u32(m.options);
        if let Some(s) = &m.shortcuts {
            w.packed_list(s, |w, v| {
                v.write(w);
                Ok(())
            })?;
        }
        w.packed_list(&m.spell_bars[0], |w, v| {
            w.u32(*v);
            Ok(())
        })?;
        w.packed_list(&self.0.content_profiles, |w, c| {
            c.write(w);
            Ok(())
        })?;
        w.packed_list(&self.0.inventory_placements, |w, p| {
            p.write(w);
            Ok(())
        })
    }
}

#[cfg(test)]
mod tests;
