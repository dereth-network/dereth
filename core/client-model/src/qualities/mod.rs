//! The base qualities' eight typed tables and the AC qualities' twelve sub-objects.
//!
//! This module reconstructs the client's qualities behavior for table storage, updates, and wire
//! I/O.
//!
//! **The client ships no property-id → name table.** The stat-id enum map compiles to nothing but
//! empty static-initialiser thunks and none of the 40 shipped `ENUM_MAPPER` dat files carries stat
//! names. Everything here is keyed by the bare number; the names in comments come from
//! ACE. Never make a code path depend on one.

mod read;
pub mod registrar;
pub mod remove;
pub mod stamper;
pub mod update;

pub use registrar::{QualityNotifications, QualityScope};
pub use remove::{Outcome as RemoveOutcome, QualityRemove};
pub use stamper::PropertySequenceGate;
pub use update::{Outcome as UpdateOutcome, QualityUpdate};

use dereth_primitives::{DataId, ObjectId};
use dereth_protocol::types::qualities::{
    AcQualities, Attribute, AttributeCache, CreationProfile, Enchantment as ProtocolEnchantment,
    PropertyTables, SecondaryAttribute, Skill, SpellBookPage,
};
use dereth_protocol::types::space::PositionWire;
use std::collections::BTreeMap;

/// `StatType` — the tag half of a quality key.
///
/// Values 10, 11 and 12
/// (`BodyDamageValue`, `BodyDamageVariance`, `BodyArmorValue`) have **no** update-stat
/// instantiation in the retail client; they are carried so that a message naming one is accepted
/// and discarded rather than mis-routed.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
#[repr(u32)]
pub enum StatType {
    Int = 1,
    Float = 2,
    Position = 3,
    Skill = 4,
    String = 5,
    Did = 6,
    Iid = 7,
    Attribute = 8,
    Attribute2nd = 9,
    BodyDamageValue = 10,
    BodyDamageVariance = 11,
    BodyArmorValue = 12,
    Bool = 13,
    Int64 = 14,
}

impl StatType {
    #[must_use]
    pub fn from_raw(v: u32) -> Option<Self> {
        Some(match v {
            1 => Self::Int,
            2 => Self::Float,
            3 => Self::Position,
            4 => Self::Skill,
            5 => Self::String,
            6 => Self::Did,
            7 => Self::Iid,
            8 => Self::Attribute,
            9 => Self::Attribute2nd,
            10 => Self::BodyDamageValue,
            11 => Self::BodyDamageVariance,
            12 => Self::BodyArmorValue,
            13 => Self::Bool,
            14 => Self::Int64,
            _ => return None,
        })
    }

    #[must_use]
    pub fn raw(self) -> u32 {
        self as u32
    }
}

/// `(StatType << 16) | propertyId` — the `PropertySequenceGate` key.
///
/// Note what this collapses: *skill*, *skill level* and *skill AC* share one counter, as do
/// *attribute* and *attribute level* (see the `dereth_client_net::client_session::stamper` note on
/// the same key).
#[derive(Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct StatKey(pub u32);

impl StatKey {
    #[must_use]
    pub fn new(t: StatType, property: u32) -> Self {
        Self((t.raw() << 16) | (property & 0xFFFF))
    }

    #[must_use]
    pub fn stat_type(self) -> StatType {
        StatType::from_raw(self.0 >> 16).unwrap_or(StatType::Int)
    }

    #[must_use]
    pub fn property(self) -> u32 {
        self.0 & 0xFFFF
    }
}

impl std::fmt::Debug for StatKey {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "StatKey({:?}, {})", self.stat_type(), self.property())
    }
}

/// One value out of one of the eight typed tables.
#[derive(Debug, Clone, PartialEq)]
pub enum StatValue {
    Int(i32),
    Int64(i64),
    /// The wire form is an `int32`; the client stores 0/1.
    Bool(bool),
    /// The table's own type is `double`. The `f32` narrowing happens in the *enchantment*
    /// pipeline, not here.
    Float(f64),
    Str(String),
    Did(DataId),
    Iid(ObjectId),
    Position(PositionWire),

    // ---- the three AC-qualities sub-object types ------------------------------------------
    //
    // The eight above are the base qualities' generic tables; these seven are the *typed* halves
    // of the AC qualities; without them, skill, attribute, and secondary-attribute updates would
    // have nowhere to land. The client's update-stat table determines which value shape each
    // opcode carries.
    //
    // Note what the **key** does and does not distinguish: skill / skill level / skill AC all
    // carry `StatType::Skill` (tag 4) and attribute / attribute level both carry
    // `StatType::Attribute` (tag 8), because the three (respectively two) messages for one
    // property share a single `PropertySequenceGate` counter. The value variant is what says which form
    // arrived; the key is what says which counter gates it.
    /// The whole-skill update — `0x02DD` / `0x02DE`.
    Skill(Skill),
    /// The skill-level update — `_level_from_pp` alone,
    /// `0x02DF` / `0x02E0`.
    SkillLevel(u32),
    /// The skill-advancement update — `_sac` alone,
    /// `0x02E1` / `0x02E2`.
    SkillAdvancementClass(u32),
    /// The whole-attribute update — `0x02E3` / `0x02E4`.
    Attribute(Attribute),
    /// The primary-attribute-level update — `_level_from_cp` alone,
    /// `0x02E5` / `0x02E6`.
    AttributeLevel(u32),
    /// The whole secondary-attribute update —
    /// `0x02E7` / `0x02E8`.
    Attribute2nd(SecondaryAttribute),
    /// The secondary-attribute-level update — the current level
    /// alone, `0x02E9` / `0x02EA`. This is what a regeneration tick sends.
    Attribute2ndLevel(u32),
}

/// The base qualities — the eight generic tables — plus the AC qualities' twelve sub-objects.
///
/// Every table is `Option`: a table that has never held a value is NULL in the client, and an
/// unpack **without** its header bit *deletes* it. NULL and empty are different and both are
/// observable on the wire.
#[derive(Debug, Clone, Default)]
pub struct Qualities {
    /// The weenie type — ACE's `WeenieType`; a player is `0x0A`.
    pub weenie_type: u32,
    pub ints: Option<BTreeMap<u32, i32>>,
    pub int64s: Option<BTreeMap<u32, i64>>,
    pub bools: Option<BTreeMap<u32, bool>>,
    pub floats: Option<BTreeMap<u32, f64>>,
    pub strings: Option<BTreeMap<u32, String>>,
    pub dids: Option<BTreeMap<u32, DataId>>,
    pub iids: Option<BTreeMap<u32, ObjectId>>,
    pub positions: Option<BTreeMap<u32, PositionWire>>,

    // ---- the twelve AC-qualities sub-objects -------------------------------------------------
    pub attributes: Option<AttributeCache>,
    pub skills: Option<BTreeMap<u32, Skill>>,
    /// `_spell_book` — spell id -> casting likelihood.
    pub spell_book: Option<BTreeMap<u32, SpellBookPage>>,
    pub enchantments: dereth_rules::enchant::EnchantmentRegistry,
    /// `_event_filter` — which world events this object reports. Carried, never read.
    pub event_filter: Option<Vec<u32>>,
    /// `_create_list`. The three anonymous dwords move opaquely, as the client does.
    pub creation_profiles: Option<Vec<CreationProfile>>,
    /// A creation-profile entry's second dword, passed to an unresolved virtual method during
    /// unpacking — almost certainly the database-object DID setter. The community catalogue
    /// names the same field `has_health`; the corpus test compares it against the
    /// `PublicWeenieDesc`'s own wcid.
    pub wcid_dword: u32,
}

impl Qualities {
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// Fill from a decoded `ACQualities` blob. `now` rebases the relative times inside it.
    ///
    /// A cleared header bit **deletes** the corresponding table rather than leaving it alone, which
    /// is why every field is assigned unconditionally from the decoded `Option`.
    pub fn apply_ac_qualities(&mut self, q: &AcQualities, now: dereth_primitives::LocalTime) {
        self.weenie_type = q.base.weenie_type;
        self.apply_property_tables(&q.base.tables);
        self.attributes = q.attribute_cache;
        self.skills = q.skills.as_ref().map(|h| {
            h.entries
                .iter()
                .map(|(k, v)| {
                    let mut s = *v;
                    // `_last_used_time` arrives relative to receipt, and the client
                    // stores `current_time - value`. Latency shifts it, permanently.
                    s.last_used_time = s.rebase_last_used(now).0;
                    (*k, s)
                })
                .collect()
        });
        self.spell_book = q
            .spell_book
            .as_ref()
            .map(|h| h.entries.iter().map(|(k, v)| (*k, *v)).collect());
        self.enchantments = q
            .enchantments
            .as_ref()
            .map_or_else(dereth_rules::enchant::EnchantmentRegistry::default, |r| {
                dereth_rules::enchant::EnchantmentRegistry::from_wire(r, now)
            });
        self.event_filter = q.event_filter.clone();
        self.creation_profiles = q.creation_profiles.clone();
        self.wcid_dword = q.has_health;
    }

    fn apply_property_tables(&mut self, t: &PropertyTables) {
        fn conv<K: Copy + Ord, V, W>(
            h: &dereth_protocol::archive::PackedHash<K, V>,
            f: impl Fn(&V) -> W,
        ) -> BTreeMap<K, W> {
            h.entries.iter().map(|(k, v)| (*k, f(v))).collect()
        }
        self.ints = t.ints.as_ref().map(|h| conv(h, |v| *v));
        self.int64s = t.int64s.as_ref().map(|h| conv(h, |v| *v));
        self.bools = t.bools.as_ref().map(|h| conv(h, |v| *v != 0));
        self.floats = t.floats.as_ref().map(|h| conv(h, |v| *v));
        self.strings = t.strings.as_ref().map(|h| conv(h, Clone::clone));
        self.dids = t.dids.as_ref().map(|h| conv(h, |v| DataId(*v)));
        self.iids = t.iids.as_ref().map(|h| conv(h, |v| *v));
        self.positions = t.positions.as_ref().map(|h| conv(h, |v| *v));
    }

    /// The property lookups, **raw** (unenchanted). The enchanted forms are
    /// [`dereth_rules::quality::QualityRead::inq_int_enchanted`] and
    /// [`dereth_rules::quality::QualityRead::inq_float_enchanted`], because only the integer and
    /// float lookups take the `raw` flag at all — the 64-bit integer, boolean, string, data-id,
    /// instance-id and position lookups are never
    /// enchanted.
    #[must_use]
    pub fn get(&self, k: StatKey) -> Option<StatValue> {
        let p = k.property();
        Some(match k.stat_type() {
            StatType::Int => StatValue::Int(*self.ints.as_ref()?.get(&p)?),
            StatType::Int64 => StatValue::Int64(*self.int64s.as_ref()?.get(&p)?),
            StatType::Bool => StatValue::Bool(*self.bools.as_ref()?.get(&p)?),
            StatType::Float => StatValue::Float(*self.floats.as_ref()?.get(&p)?),
            StatType::String => StatValue::Str(self.strings.as_ref()?.get(&p)?.clone()),
            StatType::Did => StatValue::Did(*self.dids.as_ref()?.get(&p)?),
            StatType::Iid => StatValue::Iid(*self.iids.as_ref()?.get(&p)?),
            StatType::Position => StatValue::Position(*self.positions.as_ref()?.get(&p)?),
            // The three AC-qualities sub-object types read back as their **whole**
            // record: there is one `Skill` behind `0x02DD`/`0x02DF`/`0x02E1` and one `Attribute`
            // behind `0x02E3`/`0x02E5`, and the partial forms are writes, not reads.
            StatType::Skill => StatValue::Skill(*self.skills.as_ref()?.get(&p)?),
            StatType::Attribute => StatValue::Attribute(self.attribute(p)?),
            StatType::Attribute2nd => StatValue::Attribute2nd(self.attribute_2nd(p)?),
            _ => return None,
        })
    }

    /// The generic quality setter and its seven siblings lazily allocate their table, then write.
    ///
    /// **This returns whether the value landed.** The eight generic tables always
    /// take a write, because the table is created on demand; the seven AC-qualities forms
    /// do not. The skill-level, skill-class, attribute-level and
    /// attribute-2nd-level setters each update **one field of a record that must already exist** —
    /// there is no record to hang a bare `_level_from_pp` on — so they answer false when the
    /// property names a skill or attribute this object does not carry. The caller counts that; it
    /// is the difference between "applied" and "silently dropped".
    pub fn set(&mut self, k: StatKey, v: StatValue) -> bool {
        let p = k.property();
        match v {
            StatValue::Int(n) => {
                self.ints.get_or_insert_with(BTreeMap::new).insert(p, n);
            }
            StatValue::Int64(n) => {
                self.int64s.get_or_insert_with(BTreeMap::new).insert(p, n);
            }
            StatValue::Bool(b) => {
                self.bools.get_or_insert_with(BTreeMap::new).insert(p, b);
            }
            StatValue::Float(f) => {
                self.floats.get_or_insert_with(BTreeMap::new).insert(p, f);
            }
            StatValue::Str(s) => {
                self.strings.get_or_insert_with(BTreeMap::new).insert(p, s);
            }
            StatValue::Did(d) => {
                self.dids.get_or_insert_with(BTreeMap::new).insert(p, d);
            }
            StatValue::Iid(i) => {
                self.iids.get_or_insert_with(BTreeMap::new).insert(p, i);
            }
            StatValue::Position(pos) => {
                self.positions
                    .get_or_insert_with(BTreeMap::new)
                    .insert(p, pos);
            }
            // ---- the AC-qualities sub-object types ------------------------------------------
            StatValue::Skill(s) => self.set_skill(p, s),
            StatValue::SkillLevel(n) => return self.set_skill_level(p, n),
            StatValue::SkillAdvancementClass(n) => return self.set_skill_advancement_class(p, n),
            StatValue::Attribute(a) => return self.set_attribute(p, a),
            StatValue::AttributeLevel(n) => return self.set_attribute_level(p, n),
            StatValue::Attribute2nd(a) => return self.set_attribute_2nd(p, a),
            StatValue::Attribute2ndLevel(n) => return self.set_attribute_2nd_level(p, n),
        }
        true
    }

    /// [`Self::set`] for a value that arrived in a quality update, with the client's receipt-time
    /// vital clamp in front of it.
    ///
    /// A received *current* vital (ids 2, 4, 6) is bounded by this object's own computed maximum
    /// before it is stored: the current-only form through [`crate::attributes::bounds_check`] (a
    /// signed-negative value becomes 0, anything else is capped at the enchanted maximum), the
    /// whole-record form through [`crate::attributes::bounds_check_record`] (capped only). The
    /// maximum is computed from the qualities as they stand before the write, and when it cannot be
    /// computed the update is refused (`false`), not stored. Maxima (ids 1, 3, 5) and every other
    /// property pass through unchanged.
    ///
    /// The clamp happens **only here, at receipt.** A later change to the maximum (a debuff landing,
    /// ranks changing) does not re-clamp the stored current value; the next update does.
    ///
    /// `vitals` is the `Attribute2ndTable`. Without it no maximum can be derived, and the value is
    /// stored as received — the state of a host that has not loaded the portal tables (unit tests,
    /// a failed dat read), not of a running client. `filter` is the quality filter the maximum's
    /// enchantments are read through; without it the maximum is unenchanted.
    pub fn set_received(
        &mut self,
        k: StatKey,
        v: StatValue,
        vitals: Option<&dereth_assets::tables::Attribute2ndTable>,
        filter: Option<&dereth_assets::tables::QualityFilter>,
    ) -> bool {
        let v = match (vitals, v) {
            (Some(t), StatValue::Attribute2nd(a)) => {
                match crate::attributes::bounds_check_record(self, t, filter, k.property(), a) {
                    Some(a) => StatValue::Attribute2nd(a),
                    None => return false,
                }
            }
            (Some(t), StatValue::Attribute2ndLevel(n)) => {
                match crate::attributes::bounds_check(self, t, filter, k.property(), n) {
                    Some(n) => StatValue::Attribute2ndLevel(n),
                    None => return false,
                }
            }
            (_, v) => v,
        };
        self.set(k, v)
    }

    /// The `Remove…` family. Removing from a NULL table is a no-op, as in the client.
    pub fn remove(&mut self, k: StatKey) -> bool {
        let p = k.property();
        match k.stat_type() {
            StatType::Int => self.ints.as_mut().is_some_and(|h| h.remove(&p).is_some()),
            StatType::Int64 => self.int64s.as_mut().is_some_and(|h| h.remove(&p).is_some()),
            StatType::Bool => self.bools.as_mut().is_some_and(|h| h.remove(&p).is_some()),
            StatType::Float => self.floats.as_mut().is_some_and(|h| h.remove(&p).is_some()),
            StatType::String => self
                .strings
                .as_mut()
                .is_some_and(|h| h.remove(&p).is_some()),
            StatType::Did => self.dids.as_mut().is_some_and(|h| h.remove(&p).is_some()),
            StatType::Iid => self.iids.as_mut().is_some_and(|h| h.remove(&p).is_some()),
            StatType::Position => self
                .positions
                .as_mut()
                .is_some_and(|h| h.remove(&p).is_some()),
            _ => false,
        }
    }

    /// The stored int property, defaulting to 0: the raw read, and the shape most call sites use.
    ///
    /// The client's ordinary read enchants, but only a property the quality filter lists, and every
    /// property read through this one is outside the shipped filter, so for them the two reads
    /// agree. A listed property (the allegiance rank, the ratings) is read through
    /// [`dereth_rules::quality::QualityRead::inq_int_enchanted`] wherever the client enchants it.
    #[must_use]
    pub fn inq_int(&self, property: u32) -> i32 {
        self.ints
            .as_ref()
            .and_then(|h| h.get(&property))
            .copied()
            .unwrap_or(0)
    }

    /// The stored float property, defaulting to 0: the raw read. The enchanted read is
    /// [`dereth_rules::quality::QualityRead::inq_float_enchanted`].
    #[must_use]
    pub fn inq_float(&self, property: u32) -> f64 {
        self.floats
            .as_ref()
            .and_then(|h| h.get(&property))
            .copied()
            .unwrap_or(0.0)
    }

    /// The data-id query as its callers use it: the client pre-seeds the out
    /// parameter with `::INVALID_DID` (**0**) and the query leaves it alone
    /// when the property is absent, so "absent" and "invalid" are the **same** answer to every
    /// reader. `None` is that answer. Used by
    /// the combat system's ready-position test, melee arm.
    #[must_use]
    pub fn inq_data_id(&self, property: u32) -> Option<DataId> {
        match self.dids.as_ref().and_then(|h| h.get(&property)).copied() {
            Some(DataId(0)) | None => None,
            Some(d) => Some(d),
        }
    }

    #[must_use]
    pub fn inq_bool(&self, property: u32) -> bool {
        self.bools
            .as_ref()
            .and_then(|h| h.get(&property))
            .copied()
            .unwrap_or(false)
    }

    /// Set a skill.
    pub fn set_skill(&mut self, id: u32, s: Skill) {
        self.skills.get_or_insert_with(BTreeMap::new).insert(id, s);
    }

    #[must_use]
    pub fn skill(&self, id: u32) -> Option<&Skill> {
        self.skills.as_ref()?.get(&id)
    }

    // ---- the partial setters the six skill/attribute opcodes need ---------------------------
    //
    // Each of these is one field of a record. There is no "create a skill from a bare level"
    // path in the client either: the skill-quality level setter looks the id up in the skill
    // table and returns without writing when it is not there.

    /// `_level_from_pp` alone (`0x02DF` / `0x02E0`).
    ///
    /// False when the player carries no `Skill` under that id, which is the one case where the
    /// update passed the sequence gate and still could not land.
    pub fn set_skill_level(&mut self, id: u32, level: u32) -> bool {
        let Some(s) = self.skills.as_mut().and_then(|h| h.get_mut(&id)) else {
            return false;
        };
        // `_level_from_pp` is the low 16 bits of the packed dword; the high half is the format
        // version and is not the server's to change here.
        s.level_from_pp = u16::try_from(level).unwrap_or(u16::MAX);
        true
    }

    /// Set only the skill advancement class (`0x02E1` / `0x02E2`).
    ///
    /// This is the message returned after a successful skill-advancement training request.
    pub fn set_skill_advancement_class(&mut self, id: u32, sac: u32) -> bool {
        let Some(s) = self.skills.as_mut().and_then(|h| h.get_mut(&id)) else {
            return false;
        };
        s.sac = sac;
        true
    }

    /// The attribute query's storage half — the `AttributeCache` slot for one
    /// `PropertyAttribute` id (1..=6), raw: no enchantment and no augmentation.
    #[must_use]
    pub fn attribute(&self, id: u32) -> Option<Attribute> {
        let c = self.attributes.as_ref()?;
        match id {
            1 => c.strength,
            2 => c.endurance,
            3 => c.quickness,
            4 => c.coordination,
            5 => c.focus,
            6 => c.self_,
            _ => None,
        }
    }

    /// The `SecondaryAttribute` slot for one `PropertyAttribute2nd` id.
    ///
    /// **Odd ids are maxima and even ids are current values, and both name the same record**:
    /// `MaxHealth` (1) and `Health` (2) are two views of the health slot, whose
    /// current level is the second and whose formula-derived maximum is the first.
    #[must_use]
    pub fn attribute_2nd(&self, id: u32) -> Option<SecondaryAttribute> {
        let c = self.attributes.as_ref()?;
        match id {
            1 | 2 => c.health,
            3 | 4 => c.stamina,
            5 | 6 => c.mana,
            _ => None,
        }
    }

    /// Set the whole primary-attribute record (`0x02E3` / `0x02E4`).
    ///
    /// Allocates the `AttributeCache` and sets the presence bit, matching the pair that unpacking
    /// establishes: a slot with no bit is not readable, so writing one
    /// without the other would store a value nothing can see.
    pub fn set_attribute(&mut self, id: u32, a: Attribute) -> bool {
        let c = self.attributes.get_or_insert_with(AttributeCache::default);
        let slot = match id {
            1 => &mut c.strength,
            2 => &mut c.endurance,
            3 => &mut c.quickness,
            4 => &mut c.coordination,
            5 => &mut c.focus,
            6 => &mut c.self_,
            _ => return false,
        };
        *slot = Some(a);
        // `attribute_cache_mask`: STRENGTH .. SELF are bits 0..5, in `PropertyAttribute` order.
        c.flags |= 1 << (id - 1);
        true
    }

    /// `_level_from_cp` alone (`0x02E5` / `0x02E6`).
    pub fn set_attribute_level(&mut self, id: u32, level: u32) -> bool {
        let Some(mut a) = self.attribute(id) else {
            return false;
        };
        a.level_from_cp = level;
        self.set_attribute(id, a)
    }

    /// Set the whole secondary-attribute record (`0x02E7` / `0x02E8`).
    pub fn set_attribute_2nd(&mut self, id: u32, v: SecondaryAttribute) -> bool {
        let c = self.attributes.get_or_insert_with(AttributeCache::default);
        let (slot, bit) = match id {
            1 | 2 => (
                &mut c.health,
                dereth_protocol::types::qualities::attribute_cache_mask::HEALTH,
            ),
            3 | 4 => (
                &mut c.stamina,
                dereth_protocol::types::qualities::attribute_cache_mask::STAMINA,
            ),
            5 | 6 => (
                &mut c.mana,
                dereth_protocol::types::qualities::attribute_cache_mask::MANA,
            ),
            _ => return false,
        };
        *slot = Some(v);
        c.flags |= bit;
        true
    }

    /// The current level alone (`0x02E9` / `0x02EA`).
    ///
    /// This is what a regeneration tick sends, and it is by far the most common quality event on
    /// the wire: 55 of 65 quality events in one recorded session.
    pub fn set_attribute_2nd_level(&mut self, id: u32, level: u32) -> bool {
        let Some(mut v) = self.attribute_2nd(id) else {
            return false;
        };
        v.current_level = level;
        self.set_attribute_2nd(id, v)
    }

    /// Behavior: the wire path for
    /// `Magic_UpdateEnchantment` (0x02C2).
    pub fn update_enchantment(
        &mut self,
        e: &ProtocolEnchantment,
        now: dereth_primitives::LocalTime,
    ) -> bool {
        self.enchantments
            .update_enchantment(dereth_rules::enchant::Enchantment::from_wire(e, now))
    }

    /// Behavior: the wire path for
    /// `0x02C7 Magic_DispelEnchantment` and `0x02C3 Magic_RemoveEnchantment`.
    ///
    /// The two messages are byte-identical and reach the same removal; the only difference is that
    /// `0x02C7` passes `notify = false`, so a dispel prints no "spell expired" line
    /// (`dereth_protocol::qualities::MagicDispelEnchantment`'s own note). `notify` is the caller's,
    /// because the chat table lives above this crate.
    pub fn remove_enchantment(&mut self, layered_spell_id: u32) -> bool {
        self.enchantments.remove_enchantment(layered_spell_id)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Oracle: §8's `StatType` table and the literal `0x0001000C`,
    /// which is the strongest single confirmation of the key layout in the whole document.
    #[test]
    fn the_stack_size_key_is_the_clients_literal() {
        assert_eq!(StatKey::new(StatType::Int, 12).0, 0x0001_000C);
        let k = StatKey(0x0001_000C);
        assert_eq!(k.stat_type(), StatType::Int);
        assert_eq!(k.property(), 12);
    }

    #[test]
    fn every_stat_type_round_trips_through_a_key() {
        for t in [
            StatType::Int,
            StatType::Float,
            StatType::Position,
            StatType::Skill,
            StatType::String,
            StatType::Did,
            StatType::Iid,
            StatType::Attribute,
            StatType::Attribute2nd,
            StatType::BodyDamageValue,
            StatType::BodyDamageVariance,
            StatType::BodyArmorValue,
            StatType::Bool,
            StatType::Int64,
        ] {
            let k = StatKey::new(t, 0x1234);
            assert_eq!(k.stat_type(), t);
            assert_eq!(k.property(), 0x1234);
        }
    }

    /// Oracle: §2 — every table is lazily allocated, and NULL is not the same as empty.
    #[test]
    fn tables_are_null_until_first_write() {
        let mut q = Qualities::new();
        assert!(q.ints.is_none());
        assert_eq!(q.get(StatKey::new(StatType::Int, 5)), None);
        q.set(StatKey::new(StatType::Int, 5), StatValue::Int(42));
        assert!(q.ints.is_some());
        assert_eq!(
            q.get(StatKey::new(StatType::Int, 5)),
            Some(StatValue::Int(42))
        );
        assert!(q.remove(StatKey::new(StatType::Int, 5)));
        // Removing the last value leaves the table allocated-but-empty, as the client does.
        assert!(q.ints.is_some());
        assert!(q.ints.as_ref().unwrap().is_empty());
    }

    /// Oracle: §2's accessor table — the eight types land in eight separate tables and do not
    /// collide even on the same property number.
    #[test]
    fn the_eight_tables_are_independent() {
        let mut q = Qualities::new();
        q.set(StatKey::new(StatType::Int, 1), StatValue::Int(1));
        q.set(StatKey::new(StatType::Int64, 1), StatValue::Int64(2));
        q.set(StatKey::new(StatType::Bool, 1), StatValue::Bool(true));
        q.set(StatKey::new(StatType::Float, 1), StatValue::Float(3.5));
        q.set(
            StatKey::new(StatType::String, 1),
            StatValue::Str("x".into()),
        );
        q.set(StatKey::new(StatType::Did, 1), StatValue::Did(DataId(4)));
        q.set(StatKey::new(StatType::Iid, 1), StatValue::Iid(ObjectId(5)));
        q.set(
            StatKey::new(StatType::Position, 1),
            StatValue::Position(PositionWire::default()),
        );
        assert_eq!(
            q.get(StatKey::new(StatType::Int, 1)),
            Some(StatValue::Int(1))
        );
        assert_eq!(
            q.get(StatKey::new(StatType::Int64, 1)),
            Some(StatValue::Int64(2))
        );
        assert_eq!(
            q.get(StatKey::new(StatType::Bool, 1)),
            Some(StatValue::Bool(true))
        );
        assert_eq!(
            q.get(StatKey::new(StatType::Float, 1)),
            Some(StatValue::Float(3.5))
        );
        assert_eq!(
            q.get(StatKey::new(StatType::Did, 1)),
            Some(StatValue::Did(DataId(4)))
        );
        assert_eq!(
            q.get(StatKey::new(StatType::Iid, 1)),
            Some(StatValue::Iid(ObjectId(5)))
        );
    }

    /// MaxHealth = Endurance / 2; MaxStamina = Endurance; MaxMana = Self.
    fn vital_table() -> dereth_assets::tables::Attribute2ndTable {
        use dereth_assets::tables::SkillFormula;
        use dereth_rules::attributes::attribute;
        let f = |z, a1| SkillFormula {
            w: 0,
            x: 1,
            y: 0,
            z,
            attr1: a1,
            attr2: 0,
        };
        dereth_assets::tables::Attribute2ndTable {
            id: DataId(0x0E00_0003),
            health: f(2, attribute::ENDURANCE),
            stamina: f(1, attribute::ENDURANCE),
            mana: f(1, attribute::SELF),
        }
    }

    /// Endurance 200 and a health record of 30 ranks: maximum health 130.
    fn vital_player(current: u32) -> Qualities {
        let mut q = Qualities::new();
        q.set_attribute(
            2,
            Attribute {
                init_level: 200,
                level_from_cp: 0,
                cp_spent: 0,
            },
        );
        q.set_attribute(
            6,
            Attribute {
                init_level: 50,
                level_from_cp: 0,
                cp_spent: 0,
            },
        );
        let rec = |ranks| SecondaryAttribute {
            attribute: Attribute {
                init_level: ranks,
                level_from_cp: 0,
                cp_spent: 0,
            },
            current_level: current,
        };
        q.set_attribute_2nd(1, rec(30));
        q.set_attribute_2nd(3, rec(0));
        q.set_attribute_2nd(5, rec(0));
        q
    }

    fn current(q: &Qualities, id: u32) -> u32 {
        q.attribute_2nd(id)
            .map(|v| v.current_level)
            .expect("record")
    }

    /// A received current-only vital is clamped to the character's own computed maximum at
    /// receipt; a signed-negative value is zeroed; maxima and other properties pass untouched.
    #[test]
    fn a_received_current_vital_is_clamped_to_the_computed_maximum() {
        let t = vital_table();
        let mut q = vital_player(100);
        let health = StatKey::new(StatType::Attribute2nd, 2);
        assert!(q.set_received(health, StatValue::Attribute2ndLevel(310), Some(&t), None));
        assert_eq!(
            current(&q, 2),
            130,
            "310 over a maximum of 130 is stored as 130"
        );
        assert!(q.set_received(health, StatValue::Attribute2ndLevel(90), Some(&t), None));
        assert_eq!(current(&q, 2), 90);
        assert!(q.set_received(
            health,
            StatValue::Attribute2ndLevel(u32::MAX),
            Some(&t),
            None
        ));
        assert_eq!(current(&q, 2), 0);
        // Stamina's maximum is Endurance (200), mana's is Self (50).
        assert!(q.set_received(
            StatKey::new(StatType::Attribute2nd, 4),
            StatValue::Attribute2ndLevel(250),
            Some(&t),
            None
        ));
        assert_eq!(current(&q, 4), 200);
        assert!(q.set_received(
            StatKey::new(StatType::Attribute2nd, 6),
            StatValue::Attribute2ndLevel(60),
            Some(&t),
            None
        ));
        assert_eq!(current(&q, 6), 50);

        // Without the table there is no maximum to clamp to: stored as received.
        assert!(q.set_received(health, StatValue::Attribute2ndLevel(310), None, None));
        assert_eq!(current(&q, 2), 310);

        // A non-vital value is untouched.
        assert!(q.set_received(
            StatKey::new(StatType::Int, 25),
            StatValue::Int(9_999),
            Some(&t),
            None
        ));
        assert_eq!(q.inq_int(25), 9_999);
    }

    /// Enlightenment raises the computed maximum, so the clamp follows it: an enlightened
    /// character's 136 health is kept once property 390 is present, and capped at 130 before.
    #[test]
    fn the_clamp_uses_the_maximum_including_enlightenment() {
        let t = vital_table();
        let mut q = vital_player(100);
        let health = StatKey::new(StatType::Attribute2nd, 2);
        assert!(q.set_received(health, StatValue::Attribute2ndLevel(136), Some(&t), None));
        assert_eq!(current(&q, 2), 130);
        q.set(
            StatKey::new(StatType::Int, dereth_rules::skills::aug::ENLIGHTENMENT),
            StatValue::Int(3),
        );
        assert!(q.set_received(health, StatValue::Attribute2ndLevel(136), Some(&t), None));
        assert_eq!(current(&q, 2), 136);
    }

    /// The whole record: under a maximum id (what servers send) it is stored as received; under
    /// a current id its current value is capped at the maximum computed *before* the write, so
    /// the record's own new ranks do not count yet.
    #[test]
    fn a_received_whole_record_is_capped_only_under_a_current_id() {
        let t = vital_table();
        let mut q = vital_player(100);
        let rec = |ranks, current| SecondaryAttribute {
            attribute: Attribute {
                init_level: ranks,
                level_from_cp: 0,
                cp_spent: 0,
            },
            current_level: current,
        };
        assert!(q.set_received(
            StatKey::new(StatType::Attribute2nd, 1),
            StatValue::Attribute2nd(rec(30, 999)),
            Some(&t),
            None
        ));
        assert_eq!(current(&q, 2), 999, "a maximum id is not clamped");
        // 40 ranks would make the maximum 140, but the check reads the old 30: capped at 130.
        assert!(q.set_received(
            StatKey::new(StatType::Attribute2nd, 2),
            StatValue::Attribute2nd(rec(40, 999)),
            Some(&t),
            None
        ));
        assert_eq!(q.attribute_2nd(2), Some(rec(40, 130)));
    }

    /// A maximum that cannot be computed refuses the update and leaves the stored value; a later
    /// change to the maximum does not re-clamp what is already stored.
    #[test]
    fn an_uncomputable_maximum_refuses_and_a_lower_maximum_does_not_reclamp() {
        let mut t = vital_table();
        let mut q = vital_player(40);
        // A mana formula with a zero divisor fails, so the mana maximum cannot be computed.
        t.mana.z = 0;
        let mana = StatKey::new(StatType::Attribute2nd, 6);
        assert!(!q.set_received(mana, StatValue::Attribute2ndLevel(5), Some(&t), None));
        assert_eq!(current(&q, 6), 40, "the refused update is not stored");

        let health = StatKey::new(StatType::Attribute2nd, 2);
        assert!(q.set_received(health, StatValue::Attribute2ndLevel(120), Some(&t), None));
        // Endurance drops to 100: the maximum becomes 80, and the stored 120 stays until the
        // next received value.
        q.set_attribute(
            2,
            Attribute {
                init_level: 100,
                level_from_cp: 0,
                cp_spent: 0,
            },
        );
        assert_eq!(current(&q, 2), 120);
        assert!(q.set_received(health, StatValue::Attribute2ndLevel(120), Some(&t), None));
        assert_eq!(current(&q, 2), 80);
    }
}
