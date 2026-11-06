//! `0x00C9`, an object's examination, ported from the ac1999 server in its Release mode: a flag
//! word, then one section per flag in the order below. The later message carries property tables
//! instead, keyed by the same property numbers. A word of all ones means the value is absent.
//!
//! The 1999 message names an item's spells as one line of text rather than by id, so that line goes
//! in the item's long description.

use dereth_primitives::ObjectId;
use dereth_protocol::{
    archive::PackedHash,
    objects::ItemSetAppraiseInfo,
    types::{
        appraisal::{
            flags as later, AppraisalProfile, ArmorProfile, CreatureAppraisalProfile, WeaponProfile,
        },
        qualities::PropertyTables,
    },
    Message, MessageError, Opcode, Reader, Writer,
};

use super::{invalid, read_string, MAX_NEWS};

mod section {
    pub const CREATURE: u32 = 0x1;
    pub const WEAPON: u32 = 0x2;
    pub const ARMOR: u32 = 0x4;
    pub const BURDEN_VALUE: u32 = 0x8;
    pub const INSCRIPTION: u32 = 0x10;
    pub const RESIST_LOCKPICK: u32 = 0x40;
    pub const LOCKPICK_PERCENT: u32 = 0x200;
    pub const USE: u32 = 0x400;
    pub const LOCKED: u32 = 0x800;
    pub const BOOST_VALUE: u32 = 0x1000;
    pub const BOOSTER: u32 = 0x2000;
    pub const ARMOR_LEVEL: u32 = 0x8000;
    pub const PORTAL: u32 = 0x1_0000;
    pub const LONG_DESC: u32 = 0x2_0000;
    pub const SHORT_DESC: u32 = 0x4_0000;
    pub const STRUCTURE: u32 = 0x8_0000;
    pub const ARMOR_ENCHANT: u32 = 0x10_0000;
    pub const WEAPON_ENCHANT: u32 = 0x20_0000;
    pub const RESIST_ENCHANT: u32 = 0x40_0000;
    pub const MAGIC: u32 = 0x80_0000;
    pub const CRAFTSMAN: u32 = 0x400_0000;
}

/// Property numbers, as the ac1999 server counts them.
mod int {
    pub const CREATURE_TYPE: u32 = 2;
    pub const ENCUMBRANCE: u32 = 5;
    pub const MASS: u32 = 8;
    pub const VALUE: u32 = 19;
    pub const LEVEL: u32 = 25;
    pub const ARMOR_LEVEL: u32 = 28;
    pub const ALLEGIANCE_RANK: u32 = 30;
    pub const ALLEGIANCE_FOLLOWERS: u32 = 35;
    pub const RESIST_LOCKPICK: u32 = 38;
    pub const MIN_LEVEL: u32 = 86;
    pub const MAX_LEVEL: u32 = 87;
    pub const BOOSTER: u32 = 89;
    pub const BOOST_VALUE: u32 = 90;
    pub const MAX_STRUCTURE: u32 = 91;
    pub const STRUCTURE: u32 = 92;
    pub const SPELLCRAFT: u32 = 106;
    pub const CUR_MANA: u32 = 107;
    pub const MAX_MANA: u32 = 108;
    pub const DIFFICULTY: u32 = 109;
    pub const RANK_LIMIT: u32 = 110;
    pub const SKILL_LEVEL_LIMIT: u32 = 115;
    pub const LOCKPICK_PERCENT: u32 = 173;
    pub const ITEM_SKILL: u32 = 176;
}
mod string {
    pub const GENDER: u32 = 3;
    pub const HERITAGE: u32 = 4;
    pub const TEMPLATE: u32 = 5;
    pub const INSCRIPTION: u32 = 7;
    pub const SCRIBE: u32 = 8;
    pub const USE: u32 = 14;
    pub const SHORT_DESC: u32 = 15;
    pub const LONG_DESC: u32 = 16;
    pub const HERITAGE_RESTRICTION: u32 = 19;
    pub const SCRIBE_ACCOUNT: u32 = 23;
    pub const CRAFTSMAN: u32 = 25;
    pub const PORTAL_DESTINATION: u32 = 38;
}
const LOCKED_BOOL: u32 = 3;
const WEAPON_DEFENSE_FLOAT: u32 = 29;
const ABSENT: u32 = 0xFFFF_FFFF;

#[derive(Default)]
struct Tables {
    ints: Vec<(u32, i32)>,
    bools: Vec<(u32, i32)>,
    floats: Vec<(u32, f64)>,
    strings: Vec<(u32, String)>,
}

impl Tables {
    fn word(&mut self, key: u32, r: &mut Reader<'_>) -> Result<(), MessageError> {
        let v = r.u32()?;
        if v != ABSENT {
            self.ints.push((key, v.cast_signed()));
        }
        Ok(())
    }
    fn text(&mut self, key: u32, r: &mut Reader<'_>) -> Result<(), MessageError> {
        let v = read_string(r, MAX_NEWS)?;
        if !v.is_empty() {
            self.strings.push((key, v));
        }
        Ok(())
    }
}

fn table<V>(entries: Vec<(u32, V)>) -> Option<PackedHash<u32, V>> {
    (!entries.is_empty()).then_some(PackedHash {
        table_size: 16,
        entries,
    })
}

fn read_creature(
    r: &mut Reader<'_>,
    t: &mut Tables,
) -> Result<CreatureAppraisalProfile, MessageError> {
    let flags = r.u32()?;
    t.word(int::LEVEL, r)?;
    let mut p = CreatureAppraisalProfile {
        health: r.u32()?,
        max_health: r.u32()?,
        ..CreatureAppraisalProfile::default()
    };
    if flags & 8 != 0 {
        let mut attributes = [0; 10];
        for slot in &mut attributes {
            *slot = r.u32()?;
        }
        p.attributes = Some(attributes);
        p.flags |= CreatureAppraisalProfile::HAS_ATTRIBUTES;
    }
    if flags & 4 != 0 {
        t.word(int::CREATURE_TYPE, r)?;
    }
    if flags & 2 != 0 {
        t.word(int::ALLEGIANCE_RANK, r)?;
        t.word(int::ALLEGIANCE_FOLLOWERS, r)?;
        // Three words the client reads and the server leaves zero.
        for _ in 0..3 {
            r.u32()?;
        }
        t.text(string::GENDER, r)?;
        t.text(string::HERITAGE, r)?;
        t.text(string::TEMPLATE, r)?;
        // The fellowship's and the monarch's names, and the face: the later panel has no place for them.
        read_string(r, MAX_NEWS)?;
        read_string(r, MAX_NEWS)?;
        for _ in 0..6 {
            r.u32()?;
        }
    }
    if flags & 1 != 0 {
        p.enchantment_bitfield = Some(r.u32()?);
        p.flags |= CreatureAppraisalProfile::HAS_ENCHANTMENTS;
    }
    Ok(p)
}

fn read_weapon(r: &mut Reader<'_>, t: &mut Tables) -> Result<WeaponProfile, MessageError> {
    let mut w = WeaponProfile {
        damage_type: r.u32()?,
        weapon_time: r.i32()?,
        weapon_skill: r.u32()?,
        weapon_damage: r.i32()?,
        damage_variance: r.f64()?,
        damage_mod: 1.0,
        weapon_length: r.f64()?,
        max_velocity: r.f64()?,
        ..WeaponProfile::default()
    };
    // The 1999 profile has the melee defense mod where the later one has none, as a property there.
    t.floats.push((WEAPON_DEFENSE_FLOAT, r.f64()?));
    w.weapon_offense = r.f64()?;
    Ok(w)
}

fn read_armor(r: &mut Reader<'_>) -> Result<ArmorProfile, MessageError> {
    Ok(ArmorProfile {
        mod_vs_slash: r.f32()?,
        mod_vs_pierce: r.f32()?,
        mod_vs_bludgeon: r.f32()?,
        mod_vs_cold: r.f32()?,
        mod_vs_fire: r.f32()?,
        mod_vs_acid: r.f32()?,
        mod_vs_electric: r.f32()?,
        ..ArmorProfile::default()
    })
}

fn read_profile(r: &mut Reader<'_>) -> Result<AppraisalProfile, MessageError> {
    let f = r.u32()?;
    let has = |bit: u32| f & bit != 0;
    let mut p = AppraisalProfile {
        success_flag: r.u32()?,
        ..AppraisalProfile::default()
    };
    let mut t = Tables::default();
    let mut flags = 0;
    if has(section::CREATURE) {
        p.creature_profile = Some(read_creature(r, &mut t)?);
        flags |= later::CREATURE_PROFILE;
    }
    if has(section::WEAPON) {
        p.weapon_profile = Some(read_weapon(r, &mut t)?);
        flags |= later::WEAPON_PROFILE;
    }
    if has(section::ARMOR) {
        p.armor_profile = Some(read_armor(r)?);
        flags |= later::ARMOR_PROFILE;
    }
    if has(section::BURDEN_VALUE) {
        t.word(int::MASS, r)?;
        t.word(int::VALUE, r)?;
        t.word(int::ENCUMBRANCE, r)?;
    }
    if has(section::LOCKPICK_PERCENT) {
        t.word(int::LOCKPICK_PERCENT, r)?;
    }
    if has(section::LOCKED) {
        t.bools.push((LOCKED_BOOL, r.i32()?));
    }
    if has(section::BOOSTER) {
        t.word(int::BOOSTER, r)?;
    }
    if has(section::BOOST_VALUE) {
        t.word(int::BOOST_VALUE, r)?;
    }
    if has(section::ARMOR_LEVEL) {
        t.word(int::ARMOR_LEVEL, r)?;
    }
    if has(section::RESIST_LOCKPICK) {
        t.word(int::RESIST_LOCKPICK, r)?;
    }
    if has(section::CRAFTSMAN) {
        t.text(string::CRAFTSMAN, r)?;
    }
    if has(section::INSCRIPTION) {
        t.text(string::INSCRIPTION, r)?;
        t.text(string::SCRIBE, r)?;
        t.text(string::SCRIBE_ACCOUNT, r)?;
    }
    if has(section::USE) {
        t.text(string::USE, r)?;
    }
    if has(section::LONG_DESC) {
        t.text(string::LONG_DESC, r)?;
    }
    if has(section::SHORT_DESC) {
        t.text(string::SHORT_DESC, r)?;
    }
    if has(section::PORTAL) {
        t.word(int::MIN_LEVEL, r)?;
        t.word(int::MAX_LEVEL, r)?;
        // The server writes one space for no destination.
        let destination = read_string(r, MAX_NEWS)?;
        if !destination.trim().is_empty() {
            t.strings.push((string::PORTAL_DESTINATION, destination));
        }
    }
    if has(section::STRUCTURE) {
        t.word(int::STRUCTURE, r)?;
        t.word(int::MAX_STRUCTURE, r)?;
    }
    if has(section::ARMOR_ENCHANT) {
        p.armor_enchantment = Some(r.u32()?);
        flags |= later::ARMOR_ENCHANTMENT;
    }
    if has(section::WEAPON_ENCHANT) {
        p.weapon_enchantment = Some(r.u32()?);
        flags |= later::WEAPON_ENCHANTMENT;
    }
    if has(section::RESIST_ENCHANT) {
        p.resist_enchantment = Some(r.u32()?);
        flags |= later::RESIST_ENCHANTMENT;
    }
    if has(section::MAGIC) {
        t.word(int::DIFFICULTY, r)?;
        t.word(int::SPELLCRAFT, r)?;
        t.word(int::RANK_LIMIT, r)?;
        t.word(int::CUR_MANA, r)?;
        t.word(int::MAX_MANA, r)?;
        // Two words nothing maps.
        r.u32()?;
        r.u32()?;
        t.text(string::HERITAGE_RESTRICTION, r)?;
        t.word(int::SKILL_LEVEL_LIMIT, r)?;
        let skill = r.u32()?;
        if skill != 0 {
            t.ints.push((int::ITEM_SKILL, skill.cast_signed()));
        }
        let spells = read_string(r, MAX_NEWS)?;
        read_string(r, MAX_NEWS)?;
        r.f64()?;
        if !spells.is_empty() {
            match t.strings.iter_mut().find(|(k, _)| *k == string::LONG_DESC) {
                Some((_, long)) => {
                    long.push('\n');
                    long.push_str(&spells);
                }
                None => t.strings.push((string::LONG_DESC, spells)),
            }
        }
    }
    p.tables = PropertyTables {
        ints: table(t.ints),
        bools: table(t.bools),
        floats: table(t.floats),
        strings: table(t.strings),
        ..PropertyTables::default()
    };
    for (present, bit) in [
        (p.tables.ints.is_some(), later::INT),
        (p.tables.bools.is_some(), later::BOOL),
        (p.tables.floats.is_some(), later::FLOAT),
        (p.tables.strings.is_some(), later::STRING),
    ] {
        if present {
            flags |= bit;
        }
    }
    p.flags = flags;
    Ok(p)
}

/// `0x00C9`, normalized to the later message. A failed examination is the object and a
/// profile with its success word zero.
#[derive(Debug, Clone, PartialEq)]
pub struct AppraiseInfo(pub ItemSetAppraiseInfo);

impl Message for AppraiseInfo {
    const OPCODE: Opcode = Opcode(0x00C9);
    fn read(r: &mut Reader<'_>) -> Result<Self, MessageError> {
        Ok(Self(ItemSetAppraiseInfo {
            object: ObjectId(r.u32()?),
            profile: read_profile(r)?,
        }))
    }
    fn write(&self, _: &mut Writer) -> Result<(), MessageError> {
        Err(invalid("the relay reads this message and never writes it"))
    }
}

#[cfg(test)]
mod tests;
