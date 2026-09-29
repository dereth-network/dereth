//! `AppraisalProfile` and its four sub-profiles — the payload of `0x00C9 Item_SetAppraiseInfo`.
//!
//! Source: `docs/networking/messages/02-world-objects.md` §8, transcribing the
//! appraisal profile's own unpack.
//!
//! Note the property-flag bits here are **not** the same as `ACBaseQualities`': int64 is `0x2000`
//! and DID is `0x1000`.

use super::qualities::PropertyTables;
use crate::archive::{PackedHash, Reader, Writer};
use crate::error::MessageError;

/// The `flags` gates of an [`AppraisalProfile`]. Wire order is the order used by the profile reader,
/// which is not bit order.
pub mod flags {
    pub const INT: u32 = 0x0000_0001;
    pub const BOOL: u32 = 0x0000_0002;
    pub const FLOAT: u32 = 0x0000_0004;
    pub const STRING: u32 = 0x0000_0008;
    pub const SPELL_BOOK: u32 = 0x0000_0010;
    pub const WEAPON_PROFILE: u32 = 0x0000_0020;
    pub const HOOK_PROFILE: u32 = 0x0000_0040;
    pub const ARMOR_PROFILE: u32 = 0x0000_0080;
    pub const CREATURE_PROFILE: u32 = 0x0000_0100;
    pub const ARMOR_ENCHANTMENT: u32 = 0x0000_0200;
    pub const RESIST_ENCHANTMENT: u32 = 0x0000_0400;
    pub const WEAPON_ENCHANTMENT: u32 = 0x0000_0800;
    pub const DID: u32 = 0x0000_1000;
    pub const INT64: u32 = 0x0000_2000;
    pub const BASE_ARMOR: u32 = 0x0000_4000;
}

/// The armour profile — eight floats.
#[derive(Debug, Clone, Copy, PartialEq, Default)]
pub struct ArmorProfile {
    pub mod_vs_slash: f32,
    pub mod_vs_pierce: f32,
    pub mod_vs_bludgeon: f32,
    pub mod_vs_cold: f32,
    pub mod_vs_fire: f32,
    pub mod_vs_acid: f32,
    pub mod_vs_nether: f32,
    pub mod_vs_electric: f32,
}

impl ArmorProfile {
    pub fn read(r: &mut Reader<'_>) -> Result<Self, MessageError> {
        Ok(Self {
            mod_vs_slash: r.f32()?,
            mod_vs_pierce: r.f32()?,
            mod_vs_bludgeon: r.f32()?,
            mod_vs_cold: r.f32()?,
            mod_vs_fire: r.f32()?,
            mod_vs_acid: r.f32()?,
            mod_vs_nether: r.f32()?,
            mod_vs_electric: r.f32()?,
        })
    }

    pub fn write(&self, w: &mut Writer) {
        for f in [
            self.mod_vs_slash,
            self.mod_vs_pierce,
            self.mod_vs_bludgeon,
            self.mod_vs_cold,
            self.mod_vs_fire,
            self.mod_vs_acid,
            self.mod_vs_nether,
            self.mod_vs_electric,
        ] {
            w.f32(f);
        }
    }
}

/// The creature appraisal profile.
///
/// A clear gate **zeroes** the corresponding fields rather than preserving them, which is why the
/// two optional groups are modelled as `Option` and not as defaults.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct CreatureAppraisalProfile {
    pub flags: u32,
    pub health: u32,
    pub max_health: u32,
    /// `flags & 0x08`: ten dwords.
    pub attributes: Option<[u32; 10]>,
    /// `flags & 0x01`.
    pub enchantment_bitfield: Option<u32>,
}

impl CreatureAppraisalProfile {
    pub const HAS_ENCHANTMENTS: u32 = 0x0000_0001;
    pub const HAS_ATTRIBUTES: u32 = 0x0000_0008;

    pub fn read(r: &mut Reader<'_>) -> Result<Self, MessageError> {
        let flags = r.u32()?;
        let mut p = Self {
            flags,
            health: r.u32()?,
            max_health: r.u32()?,
            ..Self::default()
        };
        if flags & Self::HAS_ATTRIBUTES != 0 {
            let mut a = [0u32; 10];
            for slot in &mut a {
                *slot = r.u32()?;
            }
            p.attributes = Some(a);
        }
        if flags & Self::HAS_ENCHANTMENTS != 0 {
            p.enchantment_bitfield = Some(r.u32()?);
        }
        Ok(p)
    }

    pub fn write(&self, w: &mut Writer) {
        w.u32(self.flags);
        w.u32(self.health);
        w.u32(self.max_health);
        if let Some(a) = self.attributes {
            for v in a {
                w.u32(v);
            }
        }
        if let Some(e) = self.enchantment_bitfield {
            w.u32(e);
        }
    }
}

/// The weapon profile — 0x3C bytes.
#[derive(Debug, Clone, Copy, PartialEq, Default)]
pub struct WeaponProfile {
    pub damage_type: u32,
    pub weapon_time: i32,
    pub weapon_skill: u32,
    pub weapon_damage: i32,
    pub damage_variance: f64,
    pub damage_mod: f64,
    pub weapon_length: f64,
    pub max_velocity: f64,
    pub weapon_offense: f64,
    pub max_velocity_estimated: i32,
}

impl WeaponProfile {
    pub fn read(r: &mut Reader<'_>) -> Result<Self, MessageError> {
        Ok(Self {
            damage_type: r.u32()?,
            weapon_time: r.i32()?,
            weapon_skill: r.u32()?,
            weapon_damage: r.i32()?,
            damage_variance: r.f64()?,
            damage_mod: r.f64()?,
            weapon_length: r.f64()?,
            max_velocity: r.f64()?,
            weapon_offense: r.f64()?,
            max_velocity_estimated: r.i32()?,
        })
    }

    pub fn write(&self, w: &mut Writer) {
        w.u32(self.damage_type);
        w.i32(self.weapon_time);
        w.u32(self.weapon_skill);
        w.i32(self.weapon_damage);
        w.f64(self.damage_variance);
        w.f64(self.damage_mod);
        w.f64(self.weapon_length);
        w.f64(self.max_velocity);
        w.f64(self.weapon_offense);
        w.i32(self.max_velocity_estimated);
    }
}

/// The hook appraisal profile — three dwords.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct HookAppraisalProfile {
    pub bitfield: u32,
    pub valid_locations: u32,
    pub ammo_type: u32,
}

impl HookAppraisalProfile {
    pub fn read(r: &mut Reader<'_>) -> Result<Self, MessageError> {
        Ok(Self {
            bitfield: r.u32()?,
            valid_locations: r.u32()?,
            ammo_type: r.u32()?,
        })
    }

    pub fn write(&self, w: &mut Writer) {
        w.u32(self.bitfield);
        w.u32(self.valid_locations);
        w.u32(self.ammo_type);
    }
}

/// The appraisal profile.
#[derive(Debug, Clone, PartialEq, Default)]
pub struct AppraisalProfile {
    pub flags: u32,
    pub success_flag: u32,
    /// The property tables, which reuse `ACBaseQualities`' shapes but **not** its flag bits.
    pub tables: PropertyTables,
    /// The `0x0010` block: a counted array of `u32` layered spell ids.
    pub spell_book: Option<Vec<u32>>,
    pub armor_profile: Option<ArmorProfile>,
    pub creature_profile: Option<CreatureAppraisalProfile>,
    pub weapon_profile: Option<WeaponProfile>,
    pub hook_profile: Option<HookAppraisalProfile>,
    pub armor_enchantment: Option<u32>,
    pub weapon_enchantment: Option<u32>,
    pub resist_enchantment: Option<u32>,
    /// The `0x4000` block: head, chest, groin, bicep, wrist, hand, thigh, shin, foot.
    pub base_armor: Option<[u32; 9]>,
}

impl AppraisalProfile {
    pub fn read(r: &mut Reader<'_>) -> Result<Self, MessageError> {
        let flags = r.u32()?;
        let success_flag = r.u32()?;
        let has = |b: u32| flags & b != 0;
        let mut p = Self {
            flags,
            success_flag,
            ..Self::default()
        };
        if has(flags::INT) {
            p.tables.ints = Some(r.packed_hash(|r| Ok((r.u32()?, r.i32()?)))?);
        }
        if has(flags::INT64) {
            p.tables.int64s = Some(r.packed_hash(|r| Ok((r.u32()?, r.i64()?)))?);
        }
        if has(flags::BOOL) {
            p.tables.bools = Some(r.packed_hash(|r| Ok((r.u32()?, r.i32()?)))?);
        }
        if has(flags::FLOAT) {
            p.tables.floats = Some(r.packed_hash(|r| Ok((r.u32()?, r.f64()?)))?);
        }
        if has(flags::STRING) {
            p.tables.strings = Some(r.packed_hash(|r| Ok((r.u32()?, r.pstring()?)))?);
        }
        if has(flags::DID) {
            p.tables.dids = Some(r.packed_hash(|r| Ok((r.u32()?, r.u32()?)))?);
        }
        if has(flags::SPELL_BOOK) {
            p.spell_book = Some(r.packed_list(Reader::u32)?);
        }
        if has(flags::ARMOR_PROFILE) {
            p.armor_profile = Some(ArmorProfile::read(r)?);
        }
        if has(flags::CREATURE_PROFILE) {
            p.creature_profile = Some(CreatureAppraisalProfile::read(r)?);
        }
        if has(flags::WEAPON_PROFILE) {
            p.weapon_profile = Some(WeaponProfile::read(r)?);
        }
        if has(flags::HOOK_PROFILE) {
            p.hook_profile = Some(HookAppraisalProfile::read(r)?);
        }
        if has(flags::ARMOR_ENCHANTMENT) {
            p.armor_enchantment = Some(r.u32()?);
        }
        if has(flags::WEAPON_ENCHANTMENT) {
            p.weapon_enchantment = Some(r.u32()?);
        }
        if has(flags::RESIST_ENCHANTMENT) {
            p.resist_enchantment = Some(r.u32()?);
        }
        if has(flags::BASE_ARMOR) {
            let mut a = [0u32; 9];
            for slot in &mut a {
                *slot = r.u32()?;
            }
            p.base_armor = Some(a);
        }
        Ok(p)
    }

    pub fn write(&self, w: &mut Writer) -> Result<(), MessageError> {
        w.u32(self.flags);
        w.u32(self.success_flag);
        let t = &self.tables;
        write_table(w, t.ints.as_ref(), |w, v: &i32| {
            w.i32(*v);
            Ok(())
        })?;
        write_table(w, t.int64s.as_ref(), |w, v: &i64| {
            w.i64(*v);
            Ok(())
        })?;
        write_table(w, t.bools.as_ref(), |w, v: &i32| {
            w.i32(*v);
            Ok(())
        })?;
        write_table(w, t.floats.as_ref(), |w, v: &f64| {
            w.f64(*v);
            Ok(())
        })?;
        write_table(w, t.strings.as_ref(), |w, v: &String| w.pstring(v))?;
        write_table(w, t.dids.as_ref(), |w, v: &u32| {
            w.u32(*v);
            Ok(())
        })?;
        if let Some(s) = &self.spell_book {
            w.packed_list(s, |w, v| {
                w.u32(*v);
                Ok(())
            })?;
        }
        if let Some(p) = &self.armor_profile {
            p.write(w);
        }
        if let Some(p) = &self.creature_profile {
            p.write(w);
        }
        if let Some(p) = &self.weapon_profile {
            p.write(w);
        }
        if let Some(p) = &self.hook_profile {
            p.write(w);
        }
        for e in [
            self.armor_enchantment,
            self.weapon_enchantment,
            self.resist_enchantment,
        ]
        .into_iter()
        .flatten()
        {
            w.u32(e);
        }
        if let Some(a) = self.base_armor {
            for v in a {
                w.u32(v);
            }
        }
        Ok(())
    }
}

fn write_table<V, F>(
    w: &mut Writer,
    t: Option<&PackedHash<u32, V>>,
    mut value: F,
) -> Result<(), MessageError>
where
    F: FnMut(&mut Writer, &V) -> Result<(), MessageError>,
{
    if let Some(t) = t {
        w.packed_hash(t, |w, k, v| {
            w.u32(*k);
            value(w, v)
        })?;
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Oracle: the client's appraisal-profile reader
    /// (`docs/networking/messages/02-world-objects.md` §8), which agrees with the community page
    /// for this one message.
    #[test]
    fn appraisal_profile_round_trips() {
        let p = AppraisalProfile {
            flags: flags::INT | flags::WEAPON_PROFILE | flags::BASE_ARMOR,
            success_flag: 1,
            tables: PropertyTables {
                ints: Some(PackedHash {
                    table_size: 8,
                    entries: vec![(19u32, 250i32)],
                }),
                ..PropertyTables::default()
            },
            weapon_profile: Some(WeaponProfile {
                damage_type: 4,
                ..WeaponProfile::default()
            }),
            base_armor: Some([1, 2, 3, 4, 5, 6, 7, 8, 9]),
            ..AppraisalProfile::default()
        };
        let mut w = Writer::new();
        p.write(&mut w).unwrap();
        let bytes = w.into_inner();
        let mut r = Reader::new(&bytes);
        assert_eq!(AppraisalProfile::read(&mut r).unwrap(), p);
        r.expect_exhausted().unwrap();
    }

    /// The appraisal flags are *not* `ACBaseQualities`': here int64 is `0x2000` and DID `0x1000`.
    #[test]
    fn the_appraisal_property_flags_differ_from_the_qualities_ones() {
        use crate::types::qualities::base_flags;
        assert_ne!(flags::INT64, base_flags::INT64);
        assert_ne!(flags::DID, base_flags::DID);
        assert_eq!(flags::INT64, 0x2000);
        assert_eq!(flags::DID, 0x1000);
    }

    /// A weapon profile needs 0x3C bytes — five doubles among the dwords.
    #[test]
    fn weapon_profile_is_sixty_bytes() {
        let mut w = Writer::new();
        WeaponProfile::default().write(&mut w);
        assert_eq!(w.len(), 0x3C);
    }
}
