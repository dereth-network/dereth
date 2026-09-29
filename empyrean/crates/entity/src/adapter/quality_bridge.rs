//! The one mapping between a [`Biota`] and the wire's qualities record (`dereth-protocol`'s
//! [`AcQualities`]): the eight property tables, the attributes and vitals, the skills, the spell
//! book and the enchantment registry.
//!
//! Not an ACE port: ACE has no such adapter. Its own writers (the PlayerDescription and the
//! object-description messages) stay where they are and keep their own rules (which properties go
//! out on login, the hash-bucket order, the spell-derived enchantment fields). This bridge is the
//! storage view: every row the biota holds, as the wire would carry it, and back. It is what the
//! server's tests use wherever they need the client's view of a biota, and what the capture-replay
//! tier uses to rebuild a recorded character.
//!
//! Iteration order differs by design on the two sides (the biota's bags enumerate in insertion
//! order; the wire's tables in the writer's bucket order), so the two are equal **as sets** per
//! table. [`biota_to_ac_qualities`] writes each table in the bag's own order.
//!
//! What does not survive a round trip, by construction:
//! - a bool other than 0 or 1 (the biota stores a `bool`; out it is always 0 or 1);
//! - a skill's format version (the biota has none; out it is 1, as ACE writes) and a negative
//!   resistance read back as a large unsigned one (the biota's field is unsigned; the bits survive);
//! - a legacy 12-byte spell-book page (the biota keeps only the likelihood);
//! - an enchantment category word whose high half is neither 0 nor 1 (the biota keeps only
//!   whether a spell-set id follows), and a spell's `EnchantmentCategory`, which the wire does not
//!   carry: it is read in as [`ENCHANTMENT_CATEGORY_FROM_SPELL`] for the caller to fill in;
//! - the event filter and creation profiles, which the biota does not keep here.

use empyrean_common::dotnet::DotNetDict;

use dereth_primitives::ObjectId;
use dereth_protocol::archive::PackedHash;
use dereth_protocol::types::qualities::{
    attribute_cache_mask, base_flags, quality_flags, AcBaseQualities, AcQualities, Attribute,
    AttributeCache, Enchantment, EnchantmentRegistry, PropertyTables, SecondaryAttribute,
    Skill as WireSkill, SpellBookPage, StatMod,
};
use dereth_protocol::types::space::{Frame, PositionWire, Quat, Vec3};

use crate::enums::{
    EnchantmentTypeFlags, EquipmentSet, PositionType, PropertyAttribute, PropertyAttribute2nd,
    PropertyBool, PropertyDataId, PropertyFloat, PropertyInstanceId, PropertyInt, PropertyInt64,
    PropertyString, Skill, SkillAdvancementClass, SpellCategory, SpellId, WeenieType,
};
use crate::models::{
    PropertiesAttribute, PropertiesAttribute2nd, PropertiesEnchantmentRegistry, PropertiesPosition,
    PropertiesSkill,
};
use crate::Biota;

/// The registry's four lists, as a registry row's `EnchantmentCategory` names them.
pub const ENCHANTMENT_MASK_MULTIPLICATIVE: u32 = 0x1;
/// See [`ENCHANTMENT_MASK_MULTIPLICATIVE`].
pub const ENCHANTMENT_MASK_ADDITIVE: u32 = 0x2;
/// See [`ENCHANTMENT_MASK_MULTIPLICATIVE`].
pub const ENCHANTMENT_MASK_VITAE: u32 = 0x4;
/// See [`ENCHANTMENT_MASK_MULTIPLICATIVE`].
pub const ENCHANTMENT_MASK_COOLDOWN: u32 = 0x8;

/// Marks a registry row read from the wire whose `EnchantmentCategory` is its spell's
/// `MetaSpellType`, which the wire does not carry: the caller, which has the spell table, fills
/// it in.
pub const ENCHANTMENT_CATEGORY_FROM_SPELL: u32 = u32::MAX;

/// The spell category cooldowns are kept under (`EnchantmentManager.SpellCategory_Cooldown`).
const SPELL_CATEGORY_COOLDOWN: u32 = 0x8000;

/// The bucket counts ACE's PlayerDescription writes each table with. The wire keeps a table's
/// bucket count; these are what [`biota_to_ac_qualities`] puts there.
pub mod table_size {
    pub const INT: u32 = 64;
    pub const INT64: u32 = 64;
    pub const BOOL: u32 = 32;
    pub const FLOAT: u32 = 32;
    pub const STRING: u32 = 32;
    pub const DATA_ID: u32 = 32;
    pub const INSTANCE_ID: u32 = 32;
    pub const POSITION: u32 = 16;
    pub const SKILL: u32 = 32;
    pub const SPELL: u32 = 64;
}

fn key16(k: u32) -> u16 {
    u16::try_from(k).unwrap_or(u16::MAX)
}

/// A wire position as the biota stores it.
#[must_use]
pub fn position_from_wire(p: &PositionWire) -> PropertiesPosition {
    PropertiesPosition {
        obj_cell_id: p.objcell_id,
        position_x: p.frame.origin.x,
        position_y: p.frame.origin.y,
        position_z: p.frame.origin.z,
        rotation_w: p.frame.orientation.w,
        rotation_x: p.frame.orientation.x,
        rotation_y: p.frame.orientation.y,
        rotation_z: p.frame.orientation.z,
    }
}

/// A stored position in its wire form.
#[must_use]
pub fn position_to_wire(p: &PropertiesPosition) -> PositionWire {
    PositionWire {
        objcell_id: p.obj_cell_id,
        frame: Frame {
            origin: Vec3 {
                x: p.position_x,
                y: p.position_y,
                z: p.position_z,
            },
            orientation: Quat {
                w: p.rotation_w,
                x: p.rotation_x,
                y: p.rotation_y,
                z: p.rotation_z,
            },
        },
    }
}

/// A registry row from its network form, kept in list `mask` (one of the `ENCHANTMENT_MASK_*`):
/// the spell id and layer (vitae is sent as layer 0 and stored as layer 1), the category word (the
/// spell category; a spell-set id follows when its high half is set), and the rest as written.
/// A spell's `EnchantmentCategory` is [`ENCHANTMENT_CATEGORY_FROM_SPELL`]; a cooldown's or a
/// vitae's is its list.
#[must_use]
pub fn enchantment_row_from_wire(e: &Enchantment, mask: u32) -> PropertiesEnchantmentRegistry {
    let spell_id = e.id & 0xFFFF;
    let layer = u16::try_from(e.id >> 16).unwrap_or(0);
    PropertiesEnchantmentRegistry {
        enchantment_category: if mask == ENCHANTMENT_MASK_VITAE || mask == ENCHANTMENT_MASK_COOLDOWN
        {
            mask
        } else {
            ENCHANTMENT_CATEGORY_FROM_SPELL
        },
        spell_id: i32::try_from(spell_id).unwrap_or(0),
        layer_id: if mask == ENCHANTMENT_MASK_VITAE {
            1
        } else {
            layer
        },
        has_spell_set_id: e.category_word >> 16 != 0,
        spell_category: SpellCategory(e.category_word & 0xFFFF),
        power_level: e.power_level.cast_unsigned(),
        start_time: e.start_time,
        duration: e.duration,
        caster_object_id: e.caster.0,
        degrade_modifier: e.degrade_modifier,
        degrade_limit: e.degrade_limit,
        last_time_degraded: e.last_time_degraded,
        stat_mod_type: EnchantmentTypeFlags(e.smod.kind.cast_signed()),
        stat_mod_key: e.smod.key,
        stat_mod_value: e.smod.value,
        spell_set_id: EquipmentSet(e.spell_set_id.map_or(0, u32::cast_signed)),
    }
}

/// A registry row in its network form (vitae goes out as layer 0), and the list it goes out in,
/// chosen the way ACE's registry sorts its rows: vitae by spell id, a cooldown by spell id above
/// the cooldown category, a multiplicative stat mod, and everything else additive.
#[must_use]
pub fn enchantment_row_to_wire(row: &PropertiesEnchantmentRegistry) -> (u32, Enchantment) {
    let spell_id = row.spell_id.cast_unsigned() & 0xFFFF;
    let mask = if spell_id == SpellId::Vitae.0 {
        ENCHANTMENT_MASK_VITAE
    } else if spell_id > SPELL_CATEGORY_COOLDOWN {
        ENCHANTMENT_MASK_COOLDOWN
    } else if (row.stat_mod_type & EnchantmentTypeFlags::Multiplicative).0 != 0 {
        ENCHANTMENT_MASK_MULTIPLICATIVE
    } else {
        ENCHANTMENT_MASK_ADDITIVE
    };
    let layer = if mask == ENCHANTMENT_MASK_VITAE {
        0
    } else {
        u32::from(row.layer_id)
    };
    let e = Enchantment {
        id: spell_id | (layer << 16),
        category_word: (row.spell_category.0 & 0xFFFF) | (u32::from(row.has_spell_set_id) << 16),
        power_level: row.power_level.cast_signed(),
        start_time: row.start_time,
        duration: row.duration,
        caster: ObjectId(row.caster_object_id),
        degrade_modifier: row.degrade_modifier,
        degrade_limit: row.degrade_limit,
        last_time_degraded: row.last_time_degraded,
        smod: StatMod {
            kind: row.stat_mod_type.0.cast_unsigned(),
            key: row.stat_mod_key,
            value: row.stat_mod_value,
        },
        spell_set_id: row
            .has_spell_set_id
            .then_some(row.spell_set_id.0.cast_unsigned()),
    };
    (mask, e)
}

/// Lays the wire's qualities over `b`, entry by entry and table by table, in the wire's order:
/// the weenie type; the eight property tables (each entry through the biota's own setter, so an
/// unchanged value keeps its slot); the attributes and vitals; the skills; the spell book; and the
/// enchantment registry (appended: multiplicative, additive, cooldowns, then vitae).
///
/// A table the wire leaves out leaves the biota's alone; nothing is removed.
pub fn ac_qualities_into_biota(q: &AcQualities, b: &mut Biota) {
    b.weenie_type = WeenieType(q.base.weenie_type);
    let t = &q.base.tables;
    for (k, v) in t.ints.iter().flat_map(|h| &h.entries) {
        b.set_property(PropertyInt(key16(*k)), *v);
    }
    for (k, v) in t.int64s.iter().flat_map(|h| &h.entries) {
        b.set_property(PropertyInt64(key16(*k)), *v);
    }
    for (k, v) in t.bools.iter().flat_map(|h| &h.entries) {
        b.set_property(PropertyBool(key16(*k)), *v != 0);
    }
    for (k, v) in t.floats.iter().flat_map(|h| &h.entries) {
        b.set_property(PropertyFloat(key16(*k)), *v);
    }
    for (k, v) in t.strings.iter().flat_map(|h| &h.entries) {
        b.set_property(PropertyString(key16(*k)), v.clone());
    }
    for (k, v) in t.dids.iter().flat_map(|h| &h.entries) {
        b.set_property(PropertyDataId(key16(*k)), *v);
    }
    for (k, v) in t.iids.iter().flat_map(|h| &h.entries) {
        b.set_property(PropertyInstanceId(key16(*k)), v.0);
    }
    for (k, v) in t.positions.iter().flat_map(|h| &h.entries) {
        b.set_property_position(PositionType(key16(*k)), position_from_wire(v));
    }

    if let Some(a) = &q.attribute_cache {
        let attributes = b.properties_attribute.get_or_insert_with(DotNetDict::new);
        for (i, v) in [
            a.strength,
            a.endurance,
            a.quickness,
            a.coordination,
            a.focus,
            a.self_,
        ]
        .iter()
        .enumerate()
        {
            if let Some(v) = v {
                let id = PropertyAttribute(u16::try_from(i + 1).unwrap_or(0));
                attributes.insert(
                    id,
                    PropertiesAttribute {
                        init_level: v.init_level,
                        level_from_cp: v.level_from_cp,
                        cp_spent: v.cp_spent,
                    },
                );
            }
        }
        let vitals = b
            .properties_attribute_2nd
            .get_or_insert_with(DotNetDict::new);
        for (id, v) in [(1u16, a.health), (3, a.stamina), (5, a.mana)] {
            if let Some(v) = v {
                vitals.insert(
                    PropertyAttribute2nd(id),
                    PropertiesAttribute2nd {
                        init_level: v.attribute.init_level,
                        level_from_cp: v.attribute.level_from_cp,
                        cp_spent: v.attribute.cp_spent,
                        current_level: v.current_level,
                    },
                );
            }
        }
    }
    if let Some(skills) = &q.skills {
        let bag = b.properties_skill.get_or_insert_with(DotNetDict::new);
        for (k, s) in &skills.entries {
            bag.insert(
                Skill(i32::try_from(*k).unwrap_or(0)),
                PropertiesSkill {
                    level_from_pp: s.level_from_pp,
                    sac: SkillAdvancementClass(s.sac),
                    pp: s.pp,
                    init_level: s.init_level,
                    resistance_at_last_check: s.resistance_of_last_check.cast_unsigned(),
                    last_used_time: s.last_used_time,
                },
            );
        }
    }
    if let Some(book) = &q.spell_book {
        let bag = b.properties_spell_book.get_or_insert_with(DotNetDict::new);
        for (k, page) in &book.entries {
            bag.insert(k.cast_signed(), page.casting_likelihood);
        }
    }
    if let Some(reg) = &q.enchantments {
        let rows = b
            .properties_enchantment_registry
            .get_or_insert_with(Vec::new);
        let lists = [
            (ENCHANTMENT_MASK_MULTIPLICATIVE, &reg.multiplicative),
            (ENCHANTMENT_MASK_ADDITIVE, &reg.additive),
            (ENCHANTMENT_MASK_COOLDOWN, &reg.cooldowns),
        ];
        for (mask, list) in lists {
            for e in list.iter().flatten() {
                rows.push(enchantment_row_from_wire(e, mask));
            }
        }
        if let Some(e) = &reg.vitae {
            rows.push(enchantment_row_from_wire(e, ENCHANTMENT_MASK_VITAE));
        }
    }
}

/// A fresh biota holding exactly what the wire's qualities carry.
#[must_use]
pub fn ac_qualities_to_biota(q: &AcQualities) -> Biota {
    let mut b = Biota::default();
    ac_qualities_into_biota(q, &mut b);
    b
}

fn table<K: Copy + Eq + std::hash::Hash, V: Clone, W>(
    bag: Option<&DotNetDict<K, V>>,
    size: u32,
    key: impl Fn(K) -> u32,
    value: impl Fn(&V) -> W,
) -> Option<PackedHash<u32, W>> {
    bag.map(|d| PackedHash {
        table_size: size,
        entries: d.iter().map(|(k, v)| (key(*k), value(v))).collect(),
    })
}

/// The biota's qualities in their wire form: every row it holds, each table in the bag's order.
///
/// A bag that is present (even empty) is a table that is present, and sets its flag; an absent
/// one is left out. The attribute cache is present when either attribute bag is, with a flag bit
/// for each record present; a vitae or a cooldown row goes in its own list, the rest by stat mod
/// (see [`enchantment_row_to_wire`]), and a list is present only when non-empty. `has_health` is
/// 1, as ACE writes it.
#[must_use]
pub fn biota_to_ac_qualities(b: &Biota) -> AcQualities {
    use table_size as n;
    let tables = PropertyTables {
        ints: table(
            b.properties_int.as_ref(),
            n::INT,
            |k: PropertyInt| u32::from(k.0),
            |v| *v,
        ),
        int64s: table(
            b.properties_int64.as_ref(),
            n::INT64,
            |k: PropertyInt64| u32::from(k.0),
            |v| *v,
        ),
        bools: table(
            b.properties_bool.as_ref(),
            n::BOOL,
            |k: PropertyBool| u32::from(k.0),
            |v| i32::from(*v),
        ),
        floats: table(
            b.properties_float.as_ref(),
            n::FLOAT,
            |k: PropertyFloat| u32::from(k.0),
            |v| *v,
        ),
        strings: table(
            b.properties_string.as_ref(),
            n::STRING,
            |k: PropertyString| u32::from(k.0),
            Clone::clone,
        ),
        dids: table(
            b.properties_did.as_ref(),
            n::DATA_ID,
            |k: PropertyDataId| u32::from(k.0),
            |v| *v,
        ),
        iids: table(
            b.properties_iid.as_ref(),
            n::INSTANCE_ID,
            |k: PropertyInstanceId| u32::from(k.0),
            |v| ObjectId(*v),
        ),
        positions: table(
            b.properties_position.as_ref(),
            n::POSITION,
            |k: PositionType| u32::from(k.0),
            position_to_wire,
        ),
    };
    let t = &tables;
    let base_flag = |present: bool, bit: u32| if present { bit } else { 0 };
    let base = AcBaseQualities {
        flags: base_flag(t.ints.is_some(), base_flags::INT)
            | base_flag(t.int64s.is_some(), base_flags::INT64)
            | base_flag(t.bools.is_some(), base_flags::BOOL)
            | base_flag(t.floats.is_some(), base_flags::FLOAT)
            | base_flag(t.strings.is_some(), base_flags::STRING)
            | base_flag(t.dids.is_some(), base_flags::DID)
            | base_flag(t.iids.is_some(), base_flags::IID)
            | base_flag(t.positions.is_some(), base_flags::POSITION),
        weenie_type: b.weenie_type.0,
        tables,
    };

    let attribute_cache =
        (b.properties_attribute.is_some() || b.properties_attribute_2nd.is_some()).then(|| {
            use attribute_cache_mask as m;
            let attr = |id: u16| {
                b.properties_attribute
                    .as_ref()
                    .and_then(|d| d.get(&PropertyAttribute(id)))
                    .map(|r| Attribute {
                        level_from_cp: r.level_from_cp,
                        init_level: r.init_level,
                        cp_spent: r.cp_spent,
                    })
            };
            let vital = |id: u16| {
                b.properties_attribute_2nd
                    .as_ref()
                    .and_then(|d| d.get(&PropertyAttribute2nd(id)))
                    .map(|r| SecondaryAttribute {
                        attribute: Attribute {
                            level_from_cp: r.level_from_cp,
                            init_level: r.init_level,
                            cp_spent: r.cp_spent,
                        },
                        current_level: r.current_level,
                    })
            };
            let mut c = AttributeCache {
                flags: 0,
                strength: attr(1),
                endurance: attr(2),
                quickness: attr(3),
                coordination: attr(4),
                focus: attr(5),
                self_: attr(6),
                health: vital(1),
                stamina: vital(3),
                mana: vital(5),
            };
            let bit = |present: bool, bit: u32| if present { bit } else { 0 };
            c.flags = bit(c.strength.is_some(), m::STRENGTH)
                | bit(c.endurance.is_some(), m::ENDURANCE)
                | bit(c.quickness.is_some(), m::QUICKNESS)
                | bit(c.coordination.is_some(), m::COORDINATION)
                | bit(c.focus.is_some(), m::FOCUS)
                | bit(c.self_.is_some(), m::SELF)
                | bit(c.health.is_some(), m::HEALTH)
                | bit(c.stamina.is_some(), m::STAMINA)
                | bit(c.mana.is_some(), m::MANA);
            c
        });

    let skills = table(
        b.properties_skill.as_ref(),
        n::SKILL,
        |k: Skill| k.0.cast_unsigned(),
        |s| WireSkill {
            level_from_pp: s.level_from_pp,
            format_version: 1,
            sac: s.sac.0,
            pp: s.pp,
            init_level: s.init_level,
            resistance_of_last_check: s.resistance_at_last_check.cast_signed(),
            last_used_time: s.last_used_time,
        },
    );
    let spell_book = table(
        b.properties_spell_book.as_ref(),
        n::SPELL,
        i32::cast_unsigned,
        |p| SpellBookPage {
            casting_likelihood: *p,
            legacy: None,
        },
    );

    let enchantments = b.properties_enchantment_registry.as_ref().map(|rows| {
        let mut reg = EnchantmentRegistry::default();
        let (mut mult, mut add, mut cool) = (Vec::new(), Vec::new(), Vec::new());
        for row in rows {
            let (mask, e) = enchantment_row_to_wire(row);
            match mask {
                ENCHANTMENT_MASK_MULTIPLICATIVE => mult.push(e),
                ENCHANTMENT_MASK_ADDITIVE => add.push(e),
                ENCHANTMENT_MASK_COOLDOWN => cool.push(e),
                // one vitae: the first, as ACE's writer sends it
                _ => {
                    if reg.vitae.is_none() {
                        reg.vitae = Some(e);
                    }
                }
            }
        }
        let list = |l: Vec<Enchantment>| (!l.is_empty()).then_some(l);
        reg.multiplicative = list(mult);
        reg.additive = list(add);
        reg.cooldowns = list(cool);
        let bit = |present: bool, bit: u32| if present { bit } else { 0 };
        reg.flags = bit(
            reg.multiplicative.is_some(),
            EnchantmentRegistry::MULTIPLICATIVE,
        ) | bit(reg.additive.is_some(), EnchantmentRegistry::ADDITIVE)
            | bit(reg.cooldowns.is_some(), EnchantmentRegistry::COOLDOWN)
            | bit(reg.vitae.is_some(), EnchantmentRegistry::VITAE);
        reg
    });

    let bit = |present: bool, bit: u32| if present { bit } else { 0 };
    let flags = bit(attribute_cache.is_some(), quality_flags::ATTRIBUTE_CACHE)
        | bit(skills.is_some(), quality_flags::SKILLS)
        | bit(spell_book.is_some(), quality_flags::SPELL_BOOK)
        | bit(enchantments.is_some(), quality_flags::ENCHANTMENT_REGISTRY);

    AcQualities {
        base,
        flags,
        has_health: 1,
        attribute_cache,
        skills,
        spell_book,
        enchantments,
        event_filter: None,
        creation_profiles: None,
    }
}

/// `q` with every table's entries sorted by key and every enchantment list by its id word: the
/// form in which two qualities records that differ only in iteration order compare equal.
#[must_use]
pub fn in_key_order(q: &AcQualities) -> AcQualities {
    fn sort<V>(h: &mut Option<PackedHash<u32, V>>) {
        if let Some(h) = h {
            h.entries.sort_by_key(|(k, _)| *k);
        }
    }
    let mut q = q.clone();
    let t = &mut q.base.tables;
    sort(&mut t.ints);
    sort(&mut t.int64s);
    sort(&mut t.bools);
    sort(&mut t.floats);
    sort(&mut t.strings);
    sort(&mut t.dids);
    sort(&mut t.iids);
    sort(&mut t.positions);
    sort(&mut q.skills);
    sort(&mut q.spell_book);
    if let Some(r) = &mut q.enchantments {
        for l in [&mut r.multiplicative, &mut r.additive, &mut r.cooldowns]
            .into_iter()
            .flatten()
        {
            l.sort_by_key(|e| e.id);
        }
    }
    q
}
