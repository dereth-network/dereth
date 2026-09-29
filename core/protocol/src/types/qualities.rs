//! `ACQualities` and everything under it: the property tables, attributes, skills, the spellbook and
//! the enchantment registry.
//!
//! Sources: `docs/networking/messages/01-login-and-character.md` §5 (transcribing
//! the qualities and base-qualities unpacks) and
//! `docs/networking/messages/03-qualities-and-updates.md` §3, §4 and §5.
//!
//! **Two things the wire order will catch you on.** The optional blocks are *not* in bit order in
//! either structure, and a clear bit **deletes** the client's existing sub-object rather than
//! leaving it alone.
//!
//! **Correction reproduced here:** the generated qualities catalogue
//! assigns DataID to `0x0040` and InstanceID to `0x0008`. The client tests **`0x0008` for the
//! DataID table and `0x0040` for the InstanceID table**; ACE
//! agrees. See `docs/CORRECTIONS.md`.

use super::space::PositionWire;
use crate::archive::{PackedHash, Reader, Writer};
use crate::error::MessageError;
use dereth_primitives::{LocalTime, ObjectId};

/// `ACBaseQualities` flags. The **wire order** is int32, int64, bool, float, string, DID, IID,
/// position — see [`AcBaseQualities::read`].
pub mod base_flags {
    pub const INT: u32 = 0x0000_0001;
    pub const BOOL: u32 = 0x0000_0002;
    pub const FLOAT: u32 = 0x0000_0004;
    /// **DataID**, not InstanceID — the community catalogue has these two swapped.
    pub const DID: u32 = 0x0000_0008;
    pub const STRING: u32 = 0x0000_0010;
    pub const POSITION: u32 = 0x0000_0020;
    /// **InstanceID**, not DataID.
    pub const IID: u32 = 0x0000_0040;
    pub const INT64: u32 = 0x0000_0080;
}

/// `ACQualities` "vector" flags, in the order the client reads them.
pub mod quality_flags {
    pub const ATTRIBUTE_CACHE: u32 = 0x0000_0001;
    pub const SKILLS: u32 = 0x0000_0002;
    pub const BODY: u32 = 0x0000_0004;
    pub const SPELL_BOOK: u32 = 0x0000_0100;
    pub const ENCHANTMENT_REGISTRY: u32 = 0x0000_0200;
    pub const EVENT_FILTER: u32 = 0x0000_0008;
    pub const EMOTE_TABLE: u32 = 0x0000_0010;
    pub const CREATION_PROFILE: u32 = 0x0000_0020;
    pub const PAGE_DATA_LIST: u32 = 0x0000_0040;
    pub const GENERATOR_TABLE: u32 = 0x0000_0080;
    pub const GENERATOR_REGISTRY: u32 = 0x0000_0400;
    pub const GENERATOR_QUEUE: u32 = 0x0000_0800;
}

/// The eight property tables of `ACBaseQualities`.
#[derive(Debug, Clone, PartialEq, Default)]
pub struct PropertyTables {
    pub ints: Option<PackedHash<u32, i32>>,
    pub int64s: Option<PackedHash<u32, i64>>,
    pub bools: Option<PackedHash<u32, i32>>,
    pub floats: Option<PackedHash<u32, f64>>,
    pub strings: Option<PackedHash<u32, String>>,
    pub dids: Option<PackedHash<u32, u32>>,
    pub iids: Option<PackedHash<u32, ObjectId>>,
    pub positions: Option<PackedHash<u32, PositionWire>>,
}

/// The base qualities.
#[derive(Debug, Clone, PartialEq, Default)]
pub struct AcBaseQualities {
    pub flags: u32,
    /// `ObjectType`; a player is `0x0A`.
    pub weenie_type: u32,
    pub tables: PropertyTables,
}

impl AcBaseQualities {
    pub fn read(r: &mut Reader<'_>) -> Result<Self, MessageError> {
        let flags = r.u32()?;
        let weenie_type = r.u32()?;
        let has = |b: u32| flags & b != 0;
        let mut t = PropertyTables::default();
        // Wire order: int32, int64, bool, float, string, DID, IID, position.
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
            t.strings = Some(r.packed_hash(|r| Ok((r.u32()?, r.pstring()?)))?);
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
        Ok(Self {
            flags,
            weenie_type,
            tables: t,
        })
    }

    pub fn write(&self, w: &mut Writer) -> Result<(), MessageError> {
        w.u32(self.flags);
        w.u32(self.weenie_type);
        let t = &self.tables;
        if let Some(h) = &t.ints {
            w.packed_hash(h, |w, k, v| {
                w.u32(*k);
                w.i32(*v);
                Ok(())
            })?;
        }
        if let Some(h) = &t.int64s {
            w.packed_hash(h, |w, k, v| {
                w.u32(*k);
                w.i64(*v);
                Ok(())
            })?;
        }
        if let Some(h) = &t.bools {
            w.packed_hash(h, |w, k, v| {
                w.u32(*k);
                w.i32(*v);
                Ok(())
            })?;
        }
        if let Some(h) = &t.floats {
            w.packed_hash(h, |w, k, v| {
                w.u32(*k);
                w.f64(*v);
                Ok(())
            })?;
        }
        if let Some(h) = &t.strings {
            w.packed_hash(h, |w, k, v| {
                w.u32(*k);
                w.pstring(v)
            })?;
        }
        if let Some(h) = &t.dids {
            w.packed_hash(h, |w, k, v| {
                w.u32(*k);
                w.u32(*v);
                Ok(())
            })?;
        }
        if let Some(h) = &t.iids {
            w.packed_hash(h, |w, k, v| {
                w.u32(*k);
                w.u32(v.0);
                Ok(())
            })?;
        }
        if let Some(h) = &t.positions {
            w.packed_hash(h, |w, k, v| {
                w.u32(*k);
                v.write(w);
                Ok(())
            })?;
        }
        Ok(())
    }
}

/// One attribute — three dwords.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct Attribute {
    pub level_from_cp: u32,
    pub init_level: u32,
    pub cp_spent: u32,
}

impl Attribute {
    pub fn read(r: &mut Reader<'_>) -> Result<Self, MessageError> {
        Ok(Self {
            level_from_cp: r.u32()?,
            init_level: r.u32()?,
            cp_spent: r.u32()?,
        })
    }

    pub fn write(&self, w: &mut Writer) {
        w.u32(self.level_from_cp);
        w.u32(self.init_level);
        w.u32(self.cp_spent);
    }
}

/// One secondary attribute — an [`Attribute`] plus the current value. This is what
/// "vital" means on the wire.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct SecondaryAttribute {
    pub attribute: Attribute,
    pub current_level: u32,
}

impl SecondaryAttribute {
    pub fn read(r: &mut Reader<'_>) -> Result<Self, MessageError> {
        Ok(Self {
            attribute: Attribute::read(r)?,
            current_level: r.u32()?,
        })
    }

    pub fn write(&self, w: &mut Writer) {
        self.attribute.write(w);
        w.u32(self.current_level);
    }
}

/// The attribute-cache mask bits. `Full` is `0x01FF` and is what the server always sends.
pub mod attribute_cache_mask {
    pub const STRENGTH: u32 = 0x0001;
    pub const ENDURANCE: u32 = 0x0002;
    pub const QUICKNESS: u32 = 0x0004;
    pub const COORDINATION: u32 = 0x0008;
    pub const FOCUS: u32 = 0x0010;
    pub const SELF: u32 = 0x0020;
    pub const HEALTH: u32 = 0x0040;
    pub const STAMINA: u32 = 0x0080;
    pub const MANA: u32 = 0x0100;
    pub const FULL: u32 = 0x01FF;
}

/// A flag word followed by each present block **in bit order** — the one place in
/// `ACQualities` where bit order and wire order agree.
///
/// The catalogue lists quickness before coordination in its `AttributeCache` page while the
/// knowledge base's mask table has coordination at `0x0008`; both agree that the blocks are read in
/// ascending bit order, which is what this decodes. The *names* differ only in the `0x0004`/`0x0008`
/// pair and neither affects a byte.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct AttributeCache {
    pub flags: u32,
    pub strength: Option<Attribute>,
    pub endurance: Option<Attribute>,
    pub quickness: Option<Attribute>,
    pub coordination: Option<Attribute>,
    pub focus: Option<Attribute>,
    pub self_: Option<Attribute>,
    pub health: Option<SecondaryAttribute>,
    pub stamina: Option<SecondaryAttribute>,
    pub mana: Option<SecondaryAttribute>,
}

impl AttributeCache {
    pub fn read(r: &mut Reader<'_>) -> Result<Self, MessageError> {
        use attribute_cache_mask as m;
        let flags = r.u32()?;
        let has = |b: u32| flags & b != 0;
        let a = |b: u32, r: &mut Reader<'_>| -> Result<Option<Attribute>, MessageError> {
            if has(b) {
                Ok(Some(Attribute::read(r)?))
            } else {
                Ok(None)
            }
        };
        let strength = a(m::STRENGTH, r)?;
        let endurance = a(m::ENDURANCE, r)?;
        let quickness = a(m::QUICKNESS, r)?;
        let coordination = a(m::COORDINATION, r)?;
        let focus = a(m::FOCUS, r)?;
        let self_ = a(m::SELF, r)?;
        let s = |b: u32, r: &mut Reader<'_>| -> Result<Option<SecondaryAttribute>, MessageError> {
            if has(b) {
                Ok(Some(SecondaryAttribute::read(r)?))
            } else {
                Ok(None)
            }
        };
        let health = s(m::HEALTH, r)?;
        let stamina = s(m::STAMINA, r)?;
        let mana = s(m::MANA, r)?;
        Ok(Self {
            flags,
            strength,
            endurance,
            quickness,
            coordination,
            focus,
            self_,
            health,
            stamina,
            mana,
        })
    }

    pub fn write(&self, w: &mut Writer) {
        w.u32(self.flags);
        for a in [
            self.strength,
            self.endurance,
            self.quickness,
            self.coordination,
            self.focus,
            self_opt(self.self_),
        ]
        .into_iter()
        .flatten()
        {
            a.write(w);
        }
        for s in [self.health, self.stamina, self.mana].into_iter().flatten() {
            s.write(w);
        }
    }
}

const fn self_opt(a: Option<Attribute>) -> Option<Attribute> {
    a
}

/// One skill — 28 bytes.
///
/// The first dword packs `_level_from_pp` in the low 16 and a **format version** in the high 16.
/// When the version is zero the client folds `_init_level` into `_pp` with its adjusted-PP helper and zeroes
/// `_init_level`; ACE always writes 1. That fold is a *semantic* step, not a wire step, so it is not
/// applied here — the caller sees what arrived.
///
/// `_last_used_time` is **elapsed seconds**, and the client stores `current_time - value`.
/// The rebasing helper applies that subtraction.
#[derive(Debug, Clone, Copy, PartialEq, Default)]
pub struct Skill {
    pub level_from_pp: u16,
    /// High 16 bits of the first dword. 0 selects the legacy `AdjPP` fold.
    pub format_version: u16,
    /// The skill advancement class.
    pub sac: u32,
    pub pp: u32,
    pub init_level: u32,
    pub resistance_of_last_check: i32,
    /// As received: seconds *before now*.
    pub last_used_time: f64,
}

impl Skill {
    pub fn read(r: &mut Reader<'_>) -> Result<Self, MessageError> {
        let packed = r.u32()?;
        // Both halves are masked explicitly.
        #[allow(clippy::cast_possible_truncation)]
        let (level_from_pp, format_version) = ((packed & 0xFFFF) as u16, (packed >> 16) as u16);
        Ok(Self {
            level_from_pp,
            format_version,
            sac: r.u32()?,
            pp: r.u32()?,
            init_level: r.u32()?,
            resistance_of_last_check: r.i32()?,
            last_used_time: r.f64()?,
        })
    }

    pub fn write(&self, w: &mut Writer) {
        w.u32((u32::from(self.format_version) << 16) | u32::from(self.level_from_pp));
        w.u32(self.sac);
        w.u32(self.pp);
        w.u32(self.init_level);
        w.i32(self.resistance_of_last_check);
        w.f64(self.last_used_time);
    }

    /// The absolute local time the skill was last used, as the client computes it at unpack.
    ///
    /// These times are relative to receipt and the client adds its own clock. Latency
    /// therefore shifts them, permanently. Reproduce that; do not "fix" it.
    #[must_use]
    pub fn rebase_last_used(&self, now: LocalTime) -> LocalTime {
        LocalTime(now.0 - self.last_used_time)
    }
}

/// `StatMod` — the tail of an [`Enchantment`].
#[derive(Debug, Clone, Copy, PartialEq, Default)]
pub struct StatMod {
    /// The enchantment type bits.
    pub kind: u32,
    /// The property, skill or attribute the enchantment modifies.
    pub key: u32,
    pub value: f32,
}

/// Enchantment type bits worth naming here.
pub mod enchantment_type {
    pub const STAT_TYPES: u32 = 0x0000_00FF;
    pub const VITAE: u32 = 0x0080_0000;
    pub const COOLDOWN: u32 = 0x0100_0000;
    pub const BENEFICIAL: u32 = 0x0200_0000;
}

/// 0x30 bytes, or 0x34 with the spell-set id.
///
/// `_start_time` and `_last_time_degraded` are **relative to receipt**: the client stores
/// `current_time + value`. The server says "started N seconds ago", so latency permanently
/// shortens every displayed duration. That is the retail behaviour and must be kept.
#[derive(Debug, Clone, Copy, PartialEq, Default)]
pub struct Enchantment {
    /// Spell id in the low 16 bits, enchantment **layer** in the high 16.
    pub id: u32,
    /// Low 16 = spell category; **high 16 non-zero means a spell-set id follows at the end**.
    pub category_word: u32,
    pub power_level: i32,
    /// As received: an offset to add to the local clock.
    pub start_time: f64,
    /// Seconds; `-1` means no duration.
    pub duration: f64,
    pub caster: ObjectId,
    pub degrade_modifier: f32,
    pub degrade_limit: f32,
    /// As received: an offset to add to the local clock.
    pub last_time_degraded: f64,
    pub smod: StatMod,
    /// Present only when `category_word >> 16 != 0`.
    pub spell_set_id: Option<u32>,
}

impl Enchantment {
    pub fn read(r: &mut Reader<'_>) -> Result<Self, MessageError> {
        let id = r.u32()?;
        let category_word = r.u32()?;
        let mut e = Self {
            id,
            category_word,
            power_level: r.i32()?,
            start_time: r.f64()?,
            duration: r.f64()?,
            caster: ObjectId(r.u32()?),
            degrade_modifier: r.f32()?,
            degrade_limit: r.f32()?,
            last_time_degraded: r.f64()?,
            smod: StatMod {
                kind: r.u32()?,
                key: r.u32()?,
                value: r.f32()?,
            },
            spell_set_id: None,
        };
        if category_word >> 16 != 0 {
            e.spell_set_id = Some(r.u32()?);
        }
        Ok(e)
    }

    pub fn write(&self, w: &mut Writer) -> Result<(), MessageError> {
        if (self.category_word >> 16 != 0) != self.spell_set_id.is_some() {
            return Err(MessageError::Unencodable {
                field: "enchantment spell-set id",
                reason: "presence must match the high half-word of the category word",
            });
        }
        w.u32(self.id);
        w.u32(self.category_word);
        w.i32(self.power_level);
        w.f64(self.start_time);
        w.f64(self.duration);
        w.u32(self.caster.0);
        w.f32(self.degrade_modifier);
        w.f32(self.degrade_limit);
        w.f64(self.last_time_degraded);
        w.u32(self.smod.kind);
        w.u32(self.smod.key);
        w.f32(self.smod.value);
        if let Some(s) = self.spell_set_id {
            w.u32(s);
        }
        Ok(())
    }

    /// The spell id half of `_id`.
    #[must_use]
    pub fn spell_id(&self) -> u16 {
        // The mask makes the truncation explicit.
        #[allow(clippy::cast_possible_truncation)]
        {
            (self.id & 0xFFFF) as u16
        }
    }

    /// The enchantment layer half of `_id`.
    #[must_use]
    pub fn layer(&self) -> u16 {
        #[allow(clippy::cast_possible_truncation)]
        {
            (self.id >> 16) as u16
        }
    }

    /// The absolute local times the client derives at unpack: `now + value` for both.
    #[must_use]
    pub fn rebase(&self, now: LocalTime) -> (LocalTime, LocalTime) {
        (
            LocalTime(now.0 + self.start_time),
            LocalTime(now.0 + self.last_time_degraded),
        )
    }
}

/// The enchantment registry.
///
/// Wire order is `0x01`, `0x02`, `0x08`, `0x04` — **not** bit order — and a clear bit deletes the
/// existing list.
#[derive(Debug, Clone, PartialEq, Default)]
pub struct EnchantmentRegistry {
    pub flags: u32,
    pub multiplicative: Option<Vec<Enchantment>>,
    pub additive: Option<Vec<Enchantment>>,
    pub cooldowns: Option<Vec<Enchantment>>,
    /// A single enchantment, not a list.
    pub vitae: Option<Enchantment>,
}

impl EnchantmentRegistry {
    pub const MULTIPLICATIVE: u32 = 0x0001;
    pub const ADDITIVE: u32 = 0x0002;
    pub const VITAE: u32 = 0x0004;
    pub const COOLDOWN: u32 = 0x0008;

    pub fn read(r: &mut Reader<'_>) -> Result<Self, MessageError> {
        let flags = r.u32()?;
        let has = |b: u32| flags & b != 0;
        let mut reg = Self {
            flags,
            ..Self::default()
        };
        if has(Self::MULTIPLICATIVE) {
            reg.multiplicative = Some(r.packed_list(Enchantment::read)?);
        }
        if has(Self::ADDITIVE) {
            reg.additive = Some(r.packed_list(Enchantment::read)?);
        }
        if has(Self::COOLDOWN) {
            reg.cooldowns = Some(r.packed_list(Enchantment::read)?);
        }
        if has(Self::VITAE) {
            reg.vitae = Some(Enchantment::read(r)?);
        }
        Ok(reg)
    }

    pub fn write(&self, w: &mut Writer) -> Result<(), MessageError> {
        w.u32(self.flags);
        for l in [&self.multiplicative, &self.additive, &self.cooldowns]
            .into_iter()
            .flatten()
        {
            w.packed_list(l, |w, e| e.write(w))?;
        }
        if let Some(v) = &self.vitae {
            v.write(w)?;
        }
        Ok(())
    }
}

/// One spellbook page, pack and unpack, read directly.
///
/// One float. When it is `>= 2.0` the page is `value − 2.0` and that is all (4 bytes). Otherwise a
/// dword the client skips and a second float follow (12 bytes) and the second float is the value.
///
/// The client's `Pack` **always** writes the 4-byte form, so a legacy page does not survive a round
/// trip through the retail client either. `legacy` keeps the skipped dword so that this crate's
/// round-trip gate can still hold on such a payload.
#[derive(Debug, Clone, Copy, PartialEq, Default)]
pub struct SpellBookPage {
    pub casting_likelihood: f32,
    /// `Some((leading, skipped))` for the 12-byte legacy form.
    pub legacy: Option<(f32, u32)>,
}

impl SpellBookPage {
    pub fn read(r: &mut Reader<'_>) -> Result<Self, MessageError> {
        let first = r.f32()?;
        if first >= 2.0 {
            Ok(Self {
                casting_likelihood: first - 2.0,
                legacy: None,
            })
        } else {
            let skipped = r.u32()?;
            let value = r.f32()?;
            Ok(Self {
                casting_likelihood: value,
                legacy: Some((first, skipped)),
            })
        }
    }

    pub fn write(&self, w: &mut Writer) {
        if let Some((first, skipped)) = self.legacy {
            w.f32(first);
            w.u32(skipped);
            w.f32(self.casting_likelihood);
        } else {
            w.f32(self.casting_likelihood + 2.0);
        }
    }
}

/// `CreationProfile` — the `0x0020` block's element.
#[derive(Debug, Clone, Copy, PartialEq, Default)]
pub struct CreationProfile {
    pub wcid: u32,
    pub palette: u32,
    pub shade: f32,
    pub destination: u32,
    pub stack_size: i32,
    pub try_to_bond: u32,
}

impl CreationProfile {
    pub fn read(r: &mut Reader<'_>) -> Result<Self, MessageError> {
        Ok(Self {
            wcid: r.u32()?,
            palette: r.u32()?,
            shade: r.f32()?,
            destination: r.u32()?,
            stack_size: r.i32()?,
            try_to_bond: r.u32()?,
        })
    }

    pub fn write(&self, w: &mut Writer) {
        w.u32(self.wcid);
        w.u32(self.palette);
        w.f32(self.shade);
        w.u32(self.destination);
        w.i32(self.stack_size);
        w.u32(self.try_to_bond);
    }
}

/// The full qualities record.
///
/// The five blocks the client and every shipped server actually use are decoded. The seven that
/// ACE never sends and that the community catalogue itself annotates "May not be in prod" are
/// **rejected** rather than guessed: their element layouts (`BodyPart`'s `ArmorCache` and
/// its selection data, `EmoteSetList`, `PageData`, the three generator records) are documented
/// only in the generated catalogue and a wrong guess would desynchronise everything after them.
#[derive(Debug, Clone, PartialEq, Default)]
pub struct AcQualities {
    pub base: AcBaseQualities,
    pub flags: u32,
    pub has_health: u32,
    pub attribute_cache: Option<AttributeCache>,
    pub skills: Option<PackedHash<u32, Skill>>,
    pub spell_book: Option<PackedHash<u32, SpellBookPage>>,
    pub enchantments: Option<EnchantmentRegistry>,
    pub event_filter: Option<Vec<u32>>,
    pub creation_profiles: Option<Vec<CreationProfile>>,
}

impl AcQualities {
    pub fn read(r: &mut Reader<'_>) -> Result<Self, MessageError> {
        use quality_flags as f;
        let base = AcBaseQualities::read(r)?;
        let flags = r.u32()?;
        let has_health = r.u32()?;
        let has = |b: u32| flags & b != 0;
        let mut q = Self {
            base,
            flags,
            has_health,
            ..Self::default()
        };
        // Wire order: 0x01, 0x02, 0x04, 0x0100, 0x0200, 0x08, 0x10, 0x20, 0x40, 0x80, 0x400, 0x800.
        if has(f::ATTRIBUTE_CACHE) {
            q.attribute_cache = Some(AttributeCache::read(r)?);
        }
        if has(f::SKILLS) {
            q.skills = Some(r.packed_hash(|r| Ok((r.u32()?, Skill::read(r)?)))?);
        }
        if has(f::BODY) {
            return Err(unsupported("ACQualities::Body"));
        }
        if has(f::SPELL_BOOK) {
            q.spell_book = Some(r.packed_hash(|r| Ok((r.u32()?, SpellBookPage::read(r)?)))?);
        }
        if has(f::ENCHANTMENT_REGISTRY) {
            q.enchantments = Some(EnchantmentRegistry::read(r)?);
        }
        if has(f::EVENT_FILTER) {
            q.event_filter = Some(r.packed_list(Reader::u32)?);
        }
        if has(f::EMOTE_TABLE) {
            return Err(unsupported("ACQualities::EmoteTable"));
        }
        if has(f::CREATION_PROFILE) {
            q.creation_profiles = Some(r.packed_list(CreationProfile::read)?);
        }
        if has(f::PAGE_DATA_LIST) {
            return Err(unsupported("ACQualities::PageDataList"));
        }
        if has(f::GENERATOR_TABLE) {
            return Err(unsupported("ACQualities::GeneratorTable"));
        }
        if has(f::GENERATOR_REGISTRY) {
            return Err(unsupported("ACQualities generator registry"));
        }
        if has(f::GENERATOR_QUEUE) {
            return Err(unsupported("ACQualities generator queue"));
        }
        Ok(q)
    }

    pub fn write(&self, w: &mut Writer) -> Result<(), MessageError> {
        use quality_flags as f;
        self.base.write(w)?;
        w.u32(self.flags);
        w.u32(self.has_health);
        for b in [
            f::BODY,
            f::EMOTE_TABLE,
            f::PAGE_DATA_LIST,
            f::GENERATOR_TABLE,
            f::GENERATOR_REGISTRY,
            f::GENERATOR_QUEUE,
        ] {
            if self.flags & b != 0 {
                return Err(unsupported("ACQualities: an unimplemented optional block"));
            }
        }
        if let Some(a) = &self.attribute_cache {
            a.write(w);
        }
        if let Some(s) = &self.skills {
            w.packed_hash(s, |w, k, v| {
                w.u32(*k);
                v.write(w);
                Ok(())
            })?;
        }
        if let Some(s) = &self.spell_book {
            w.packed_hash(s, |w, k, v| {
                w.u32(*k);
                v.write(w);
                Ok(())
            })?;
        }
        if let Some(e) = &self.enchantments {
            e.write(w)?;
        }
        if let Some(l) = &self.event_filter {
            w.packed_list(l, |w, v| {
                w.u32(*v);
                Ok(())
            })?;
        }
        if let Some(l) = &self.creation_profiles {
            w.packed_list(l, |w, v| {
                v.write(w);
                Ok(())
            })?;
        }
        Ok(())
    }
}

fn unsupported(field: &'static str) -> MessageError {
    MessageError::Unsupported {
        field,
        note: "layout known only from the community catalogue; no shipped server sends it.",
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::archive::PackedHash;

    /// Oracle: `docs/CORRECTIONS.md` and `docs/networking/messages/03-qualities-and-updates.md` §5,
    /// both describing the base qualities. The community catalogue swaps these two bits; following
    /// it would read a DataID table as instance ids and vice versa.
    #[test]
    fn the_did_and_iid_flags_are_the_client_way_round() {
        assert_eq!(base_flags::DID, 0x0008);
        assert_eq!(base_flags::IID, 0x0040);

        // A body with only the DID table present must decode as `dids`, not `iids`.
        let q = AcBaseQualities {
            flags: base_flags::DID,
            weenie_type: 0x0A,
            tables: PropertyTables {
                dids: Some(PackedHash {
                    table_size: 8,
                    entries: vec![(1u32, 0x0600_0001u32)],
                }),
                ..PropertyTables::default()
            },
        };
        let mut w = Writer::new();
        q.write(&mut w).unwrap();
        let bytes = w.into_inner();
        let mut r = Reader::new(&bytes);
        let back = AcBaseQualities::read(&mut r).unwrap();
        assert!(back.tables.dids.is_some());
        assert!(back.tables.iids.is_none());
        assert_eq!(back, q);
        r.expect_exhausted().unwrap();
    }

    /// Oracle: `docs/networking/messages/03-qualities-and-updates.md` §5 — the wire order is int32,
    /// int64, bool, float, string, DID, IID, position — again not bit order.
    #[test]
    fn the_property_tables_are_in_wire_order_not_bit_order() {
        let q = AcBaseQualities {
            flags: base_flags::INT | base_flags::INT64 | base_flags::BOOL,
            weenie_type: 0x0A,
            tables: PropertyTables {
                ints: Some(PackedHash {
                    table_size: 8,
                    entries: vec![(12u32, 100i32)],
                }),
                int64s: Some(PackedHash {
                    table_size: 8,
                    entries: vec![(1u32, 5i64)],
                }),
                bools: Some(PackedHash {
                    table_size: 8,
                    entries: vec![(1u32, 1i32)],
                }),
                ..PropertyTables::default()
            },
        };
        let mut w = Writer::new();
        q.write(&mut w).unwrap();
        let bytes = w.into_inner();
        // int (0x0001) is written before int64 (0x0080), which is before bool (0x0002).
        // Header dword then key/value; the int64 value is eight bytes, the others four.
        assert_eq!(
            &bytes[8..12],
            &[0x01, 0x00, 0x08, 0x00],
            "int table header first"
        );
        assert_eq!(
            &bytes[20..24],
            &[0x01, 0x00, 0x08, 0x00],
            "then the int64 table header"
        );
        let mut r = Reader::new(&bytes);
        assert_eq!(AcBaseQualities::read(&mut r).unwrap(), q);
        r.expect_exhausted().unwrap();
    }

    /// Oracle: `docs/networking/messages/03-qualities-and-updates.md` §3 — the skill record is 28
    /// bytes with the format version in the high half of the first dword.
    #[test]
    fn skill_is_twenty_eight_bytes_with_a_version_in_the_high_half() {
        let s = Skill {
            level_from_pp: 40,
            format_version: 1,
            sac: 3,
            pp: 123_456,
            init_level: 10,
            resistance_of_last_check: -1,
            last_used_time: 12.5,
        };
        let mut w = Writer::new();
        s.write(&mut w);
        assert_eq!(w.len(), 28);
        assert_eq!(&w.as_slice()[0..4], &[40, 0, 1, 0]);
        let bytes = w.into_inner();
        let mut r = Reader::new(&bytes);
        assert_eq!(Skill::read(&mut r).unwrap(), s);
        r.expect_exhausted().unwrap();
    }

    /// Skill and enchantment times arrive relative to receipt and the client adds
    /// its own clock, so latency permanently shifts them. This asserts the *direction* of each,
    /// which differs: the skill time is subtracted, the enchantment times added.
    #[test]
    fn the_relative_times_are_rebased_the_way_the_client_does_it() {
        let now = LocalTime(1000.0);
        let s = Skill {
            last_used_time: 30.0,
            ..Skill::default()
        };
        assert_eq!(
            s.rebase_last_used(now).0,
            970.0,
            "timer current time minus value"
        );

        let e = Enchantment {
            start_time: -30.0,
            last_time_degraded: -5.0,
            ..Enchantment::default()
        };
        let (start, degraded) = e.rebase(now);
        assert_eq!(start.0, 970.0, "timer current time plus value");
        assert_eq!(degraded.0, 995.0);
    }

    /// Oracle: the enchantment record's layout. The spell-set id is present
    /// only when the high half of the category word is non-zero.
    #[test]
    fn enchantment_round_trips_with_and_without_a_spell_set() {
        let base = Enchantment {
            id: 0x0002_1234,
            category_word: 0x0000_0056,
            power_level: 5,
            start_time: -1.5,
            duration: 300.0,
            caster: ObjectId(0x5000_0001),
            degrade_modifier: 0.5,
            degrade_limit: 0.25,
            last_time_degraded: -0.5,
            smod: StatMod {
                kind: enchantment_type::BENEFICIAL,
                key: 1,
                value: 20.0,
            },
            spell_set_id: None,
        };
        for e in [
            base,
            Enchantment {
                category_word: 0x0001_0056,
                spell_set_id: Some(7),
                ..base
            },
        ] {
            let mut w = Writer::new();
            e.write(&mut w).unwrap();
            // 60 bytes, or 64 with the spell-set id. The 0x30 in the client is only the *guard*
            // before it starts: the enchantment unpack refuses below 48 bytes and then
            // consumes 60.
            assert_eq!(w.len(), if e.spell_set_id.is_some() { 64 } else { 60 });
            let bytes = w.into_inner();
            let mut r = Reader::new(&bytes);
            assert_eq!(Enchantment::read(&mut r).unwrap(), e);
            r.expect_exhausted().unwrap();
        }
        assert_eq!(base.spell_id(), 0x1234);
        assert_eq!(base.layer(), 2);
    }

    /// Oracle: the enchantment registry uses wire order 0x01, 0x02, 0x08, 0x04, and
    /// vitae is a single enchantment rather than a list.
    #[test]
    fn the_enchantment_registry_reads_cooldowns_before_vitae() {
        let reg = EnchantmentRegistry {
            flags: EnchantmentRegistry::COOLDOWN | EnchantmentRegistry::VITAE,
            cooldowns: Some(vec![Enchantment::default()]),
            vitae: Some(Enchantment {
                power_level: 9,
                ..Enchantment::default()
            }),
            ..EnchantmentRegistry::default()
        };
        let mut w = Writer::new();
        reg.write(&mut w).unwrap();
        let bytes = w.into_inner();
        // flags, then the cooldown list's u32 count, then one 0x30-byte enchantment, then vitae.
        assert_eq!(
            &bytes[4..8],
            &[1, 0, 0, 0],
            "the cooldown list count comes first"
        );
        let mut r = Reader::new(&bytes);
        assert_eq!(EnchantmentRegistry::read(&mut r).unwrap(), reg);
        r.expect_exhausted().unwrap();
    }

    /// Oracle: retail behaviour.
    #[test]
    fn spellbook_page_has_a_four_byte_and_a_twelve_byte_form() {
        let modern = SpellBookPage {
            casting_likelihood: 0.75,
            legacy: None,
        };
        let mut w = Writer::new();
        modern.write(&mut w);
        assert_eq!(w.len(), 4);
        assert_eq!(w.as_slice(), &2.75f32.to_le_bytes());
        let bytes = w.into_inner();
        let mut r = Reader::new(&bytes);
        assert_eq!(SpellBookPage::read(&mut r).unwrap(), modern);

        let legacy = SpellBookPage {
            casting_likelihood: 0.5,
            legacy: Some((1.0, 0xDEAD_BEEF)),
        };
        let mut w = Writer::new();
        legacy.write(&mut w);
        assert_eq!(w.len(), 12);
        let bytes = w.into_inner();
        let mut r = Reader::new(&bytes);
        assert_eq!(SpellBookPage::read(&mut r).unwrap(), legacy);
    }

    /// An optional block whose layout is only in the community catalogue must fail loudly rather
    /// than be guessed: a wrong length desynchronises everything after it.
    #[test]
    fn the_undocumented_optional_blocks_are_rejected_not_guessed() {
        let mut w = Writer::new();
        w.u32(0); // base flags
        w.u32(0x0A); // weenie type
        w.u32(quality_flags::BODY);
        w.u32(1); // has_health
        let bytes = w.into_inner();
        let mut r = Reader::new(&bytes);
        assert!(matches!(
            AcQualities::read(&mut r),
            Err(MessageError::Unsupported { .. })
        ));
    }
}
