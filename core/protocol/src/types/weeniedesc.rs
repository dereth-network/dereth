//! `PublicWeenieDesc` — an object's game data — plus `RestrictionDB` and the two little inventory
//! records.
//!
//! Source: `docs/networking/messages/02-world-objects.md` §3.3, transcribing the
//! public weenie description's own unpack. The generated page matches it, verified — including
//! the subtlety that `0x04000000` means two different things in the two words: `RestrictionDB` present
//! in `header`, "a second header dword follows" in `_bitfield`.

use crate::archive::{PHash, Reader, Writer};
use crate::error::MessageError;
use dereth_primitives::ObjectId;

/// Base type for an icon DataID (`0x06000000`).
pub const ICON_BASE: u32 = 0x0600_0000;

/// The `header` gates of a [`PublicWeenieDesc`], in bit order. The **wire** order is the order the
/// fields appear in the profile reader, which is not this order.
pub mod header {
    pub const PLURAL_NAME: u32 = 0x0000_0001;
    pub const ITEMS_CAPACITY: u32 = 0x0000_0002;
    pub const CONTAINERS_CAPACITY: u32 = 0x0000_0004;
    pub const VALUE: u32 = 0x0000_0008;
    pub const USEABILITY: u32 = 0x0000_0010;
    pub const USE_RADIUS: u32 = 0x0000_0020;
    pub const MONARCH: u32 = 0x0000_0040;
    pub const EFFECTS: u32 = 0x0000_0080;
    pub const AMMO_TYPE: u32 = 0x0000_0100;
    pub const COMBAT_USE: u32 = 0x0000_0200;
    pub const STRUCTURE: u32 = 0x0000_0400;
    pub const MAX_STRUCTURE: u32 = 0x0000_0800;
    pub const STACK_SIZE: u32 = 0x0000_1000;
    pub const MAX_STACK_SIZE: u32 = 0x0000_2000;
    pub const CONTAINER_ID: u32 = 0x0000_4000;
    pub const WIELDER_ID: u32 = 0x0000_8000;
    pub const VALID_LOCATIONS: u32 = 0x0001_0000;
    pub const LOCATION: u32 = 0x0002_0000;
    pub const PRIORITY: u32 = 0x0004_0000;
    pub const TARGET_TYPE: u32 = 0x0008_0000;
    pub const BLIP_COLOR: u32 = 0x0010_0000;
    pub const BURDEN: u32 = 0x0020_0000;
    pub const SPELL_ID: u32 = 0x0040_0000;
    pub const RADAR_ENUM: u32 = 0x0080_0000;
    pub const WORKMANSHIP: u32 = 0x0100_0000;
    pub const HOUSE_OWNER: u32 = 0x0200_0000;
    /// In `header` this is the `RestrictionDB`; in `_bitfield` it is "header2 follows".
    pub const RESTRICTIONS: u32 = 0x0400_0000;
    pub const PSCRIPT: u32 = 0x0800_0000;
    pub const HOOK_TYPE: u32 = 0x1000_0000;
    pub const HOOK_ITEM_TYPES: u32 = 0x2000_0000;
    pub const ICON_OVERLAY: u32 = 0x4000_0000;
    pub const MATERIAL_TYPE: u32 = 0x8000_0000;
}

/// The `header2` gates, reachable only when `_bitfield & 0x04000000`.
pub mod header2 {
    pub const ICON_UNDERLAY: u32 = 0x0000_0001;
    pub const COOLDOWN_ID: u32 = 0x0000_0002;
    pub const COOLDOWN_DURATION: u32 = 0x0000_0004;
    pub const PET_OWNER: u32 = 0x0000_0008;
}

/// `ObjectDescriptionFlag` bit `0x04000000`: a second header dword follows the alignment pad.
pub const BITFIELD_INCLUDES_SECOND_HEADER: u32 = 0x0400_0000;

/// The access-control list for a dwelling.
///
/// Read directly from the client's own restriction database,
/// which has three shapes:
///
/// * high word of the first dword is zero → that dword **is** the bitmask (the open-house setting), and a
///   legacy hash table follows immediately;
/// * `0 < version <= 0x10000001` → `[version][bitmask][monarch][legacy hash table]`;
/// * `version >= 0x10000002` → `[version][bitmask][monarch][PHashTable]`.
///
/// All shipped data uses the third; the first two are kept because the client still accepts them.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct RestrictionDb {
    /// 0 for the oldest form, where the first dword was the bitmask itself.
    pub version: u32,
    pub bitmask: u32,
    pub monarch_iid: ObjectId,
    pub table: PHash<ObjectId, u32>,
}

impl RestrictionDb {
    /// The version every shipped server sends.
    pub const CURRENT_VERSION: u32 = 0x1000_0002;

    pub fn read(r: &mut Reader<'_>) -> Result<Self, MessageError> {
        let first = r.u32()?;
        let version = if first & 0xFFFF_0000 == 0 { 0 } else { first };
        let (bitmask, monarch) = if version == 0 {
            // The oldest form: the dword just read was the bitmask.
            (first, 0)
        } else {
            (r.u32()?, r.u32()?)
        };
        let table = r.phash(|r| Ok((ObjectId(r.u32()?), r.u32()?)))?;
        Ok(Self {
            version,
            bitmask,
            monarch_iid: ObjectId(monarch),
            table,
        })
    }

    pub fn write(&self, w: &mut Writer) -> Result<(), MessageError> {
        if self.version == 0 {
            if self.bitmask & 0xFFFF_0000 != 0 {
                return Err(MessageError::Unencodable {
                    field: "restriction mask",
                    reason: "version 0 stores the bitmask in the version dword, so its high word \
                             must be zero",
                });
            }
            w.u32(self.bitmask);
        } else {
            w.u32(self.version);
            w.u32(self.bitmask);
            w.u32(self.monarch_iid.0);
        }
        w.phash(&self.table, |w, k, v| {
            w.u32(k.0);
            w.u32(*v);
            Ok(())
        })
    }
}

/// The public weenie description.
///
/// Every optional field is `Option` rather than a defaulted value, because the client's *absent*
/// and *present-and-zero* are different: applying a new description compares the incoming
/// container, wielder and location against what it had, and an absent field must not read as zero.
#[derive(Debug, Clone, PartialEq, Default)]
pub struct PublicWeenieDesc {
    pub header: u32,
    pub name: String,
    /// The compressed weenie-class id — **no base**, and it masks with `0x7FFF`, not
    /// `0x3FFF`. Not the same helper as every other packed id in the protocol.
    pub wcid: u32,
    pub icon_id: u32,
    /// `ITEM_TYPE`.
    pub obj_type: u32,
    /// `ObjectDescriptionFlag`.
    pub bitfield: u32,
    pub header2: Option<u32>,

    pub plural_name: Option<String>,
    pub items_capacity: Option<u8>,
    pub containers_capacity: Option<u8>,
    pub ammo_type: Option<u16>,
    pub value: Option<u32>,
    pub useability: Option<u32>,
    pub use_radius: Option<f32>,
    pub target_type: Option<u32>,
    pub effects: Option<u32>,
    pub combat_use: Option<u8>,
    pub structure: Option<u16>,
    pub max_structure: Option<u16>,
    pub stack_size: Option<u16>,
    pub max_stack_size: Option<u16>,
    pub container_id: Option<ObjectId>,
    pub wielder_id: Option<ObjectId>,
    pub valid_locations: Option<u32>,
    pub location: Option<u32>,
    pub priority: Option<u32>,
    pub blip_color: Option<u8>,
    pub radar_enum: Option<u8>,
    pub pscript: Option<u16>,
    pub workmanship: Option<f32>,
    pub burden: Option<u16>,
    pub spell_id: Option<u16>,
    pub house_owner_iid: Option<ObjectId>,
    pub restrictions: Option<RestrictionDb>,
    pub hook_item_types: Option<u32>,
    pub monarch: Option<ObjectId>,
    pub hook_type: Option<u16>,
    pub icon_overlay_id: Option<u32>,
    pub icon_underlay_id: Option<u32>,
    pub material_type: Option<u32>,
    pub cooldown_id: Option<u32>,
    pub cooldown_duration: Option<f64>,
    pub pet_owner: Option<ObjectId>,
}

/// The compressed weenie-class id: like the packed DataID but with no base and a
/// `0x7FFF` mask on the high half.
fn read_wclass_id(r: &mut Reader<'_>) -> Result<u32, MessageError> {
    let v = r.u16()?;
    if v & 0x8000 != 0 {
        let lo = r.u16()?;
        Ok((u32::from(v & 0x7FFF) << 16) | u32::from(lo))
    } else {
        Ok(u32::from(v))
    }
}

fn write_wclass_id(w: &mut Writer, v: u32) -> Result<(), MessageError> {
    // Guarded: each arm masks to 16 bits.
    #[allow(clippy::cast_possible_truncation)]
    if v < 0x8000 {
        w.u16(v as u16);
        Ok(())
    } else if v < 0x8000_0000 {
        w.u16(((v >> 16) as u16) | 0x8000);
        w.u16((v & 0xFFFF) as u16);
        Ok(())
    } else {
        Err(MessageError::Unencodable {
            field: "public object-description class id",
            reason: "weenie class id exceeds 0x7FFFFFFF",
        })
    }
}

impl PublicWeenieDesc {
    pub fn read(r: &mut Reader<'_>) -> Result<Self, MessageError> {
        let mut d = Self {
            header: r.u32()?,
            name: r.pstring()?,
            wcid: read_wclass_id(r)?,
            icon_id: r.packed_data_id(ICON_BASE)?,
            obj_type: r.u32()?,
            bitfield: r.u32()?,
            ..Self::default()
        };
        // The pstring already aligned; the two packed ids may not have.
        r.align4()?;
        if d.bitfield & BITFIELD_INCLUDES_SECOND_HEADER != 0 {
            d.header2 = Some(r.u32()?);
        }
        let h = d.header;
        let h2 = d.header2.unwrap_or(0);
        let has = |b: u32| h & b != 0;
        let has2 = |b: u32| h2 & b != 0;

        if has(header::PLURAL_NAME) {
            d.plural_name = Some(r.pstring()?);
        }
        if has(header::ITEMS_CAPACITY) {
            d.items_capacity = Some(r.u8()?);
        }
        if has(header::CONTAINERS_CAPACITY) {
            d.containers_capacity = Some(r.u8()?);
        }
        if has(header::AMMO_TYPE) {
            d.ammo_type = Some(r.u16()?);
        }
        if has(header::VALUE) {
            d.value = Some(r.u32()?);
        }
        if has(header::USEABILITY) {
            d.useability = Some(r.u32()?);
        }
        if has(header::USE_RADIUS) {
            d.use_radius = Some(r.f32()?);
        }
        if has(header::TARGET_TYPE) {
            d.target_type = Some(r.u32()?);
        }
        if has(header::EFFECTS) {
            d.effects = Some(r.u32()?);
        }
        if has(header::COMBAT_USE) {
            d.combat_use = Some(r.u8()?);
        }
        if has(header::STRUCTURE) {
            d.structure = Some(r.u16()?);
        }
        if has(header::MAX_STRUCTURE) {
            d.max_structure = Some(r.u16()?);
        }
        if has(header::STACK_SIZE) {
            d.stack_size = Some(r.u16()?);
        }
        if has(header::MAX_STACK_SIZE) {
            d.max_stack_size = Some(r.u16()?);
        }
        if has(header::CONTAINER_ID) {
            d.container_id = Some(ObjectId(r.u32()?));
        }
        if has(header::WIELDER_ID) {
            d.wielder_id = Some(ObjectId(r.u32()?));
        }
        if has(header::VALID_LOCATIONS) {
            d.valid_locations = Some(r.u32()?);
        }
        if has(header::LOCATION) {
            d.location = Some(r.u32()?);
        }
        if has(header::PRIORITY) {
            d.priority = Some(r.u32()?);
        }
        if has(header::BLIP_COLOR) {
            d.blip_color = Some(r.u8()?);
        }
        if has(header::RADAR_ENUM) {
            d.radar_enum = Some(r.u8()?);
        }
        if has(header::PSCRIPT) {
            d.pscript = Some(r.u16()?);
        }
        if has(header::WORKMANSHIP) {
            d.workmanship = Some(r.f32()?);
        }
        if has(header::BURDEN) {
            d.burden = Some(r.u16()?);
        }
        if has(header::SPELL_ID) {
            d.spell_id = Some(r.u16()?);
        }
        if has(header::HOUSE_OWNER) {
            d.house_owner_iid = Some(ObjectId(r.u32()?));
        }
        if has(header::RESTRICTIONS) {
            d.restrictions = Some(RestrictionDb::read(r)?);
        }
        if has(header::HOOK_ITEM_TYPES) {
            d.hook_item_types = Some(r.u32()?);
        }
        if has(header::MONARCH) {
            d.monarch = Some(ObjectId(r.u32()?));
        }
        if has(header::HOOK_TYPE) {
            d.hook_type = Some(r.u16()?);
        }
        if has(header::ICON_OVERLAY) {
            d.icon_overlay_id = Some(r.packed_data_id(ICON_BASE)?);
        }
        if has2(header2::ICON_UNDERLAY) {
            d.icon_underlay_id = Some(r.packed_data_id(ICON_BASE)?);
        }
        if has(header::MATERIAL_TYPE) {
            d.material_type = Some(r.u32()?);
        }
        if has2(header2::COOLDOWN_ID) {
            d.cooldown_id = Some(r.u32()?);
        }
        if has2(header2::COOLDOWN_DURATION) {
            d.cooldown_duration = Some(r.f64()?);
        }
        if has2(header2::PET_OWNER) {
            d.pet_owner = Some(ObjectId(r.u32()?));
        }
        r.align4()?;
        Ok(d)
    }

    #[allow(clippy::too_many_lines)] // one line per wire field; splitting it would hide the order
    pub fn write(&self, w: &mut Writer) -> Result<(), MessageError> {
        w.u32(self.header);
        w.pstring(&self.name)?;
        write_wclass_id(w, self.wcid)?;
        w.packed_data_id(ICON_BASE, self.icon_id)?;
        w.u32(self.obj_type);
        w.u32(self.bitfield);
        w.align4();
        if let Some(h2) = self.header2 {
            w.u32(h2);
        }
        if let Some(s) = &self.plural_name {
            w.pstring(s)?;
        }
        if let Some(v) = self.items_capacity {
            w.u8(v);
        }
        if let Some(v) = self.containers_capacity {
            w.u8(v);
        }
        if let Some(v) = self.ammo_type {
            w.u16(v);
        }
        if let Some(v) = self.value {
            w.u32(v);
        }
        if let Some(v) = self.useability {
            w.u32(v);
        }
        if let Some(v) = self.use_radius {
            w.f32(v);
        }
        if let Some(v) = self.target_type {
            w.u32(v);
        }
        if let Some(v) = self.effects {
            w.u32(v);
        }
        if let Some(v) = self.combat_use {
            w.u8(v);
        }
        if let Some(v) = self.structure {
            w.u16(v);
        }
        if let Some(v) = self.max_structure {
            w.u16(v);
        }
        if let Some(v) = self.stack_size {
            w.u16(v);
        }
        if let Some(v) = self.max_stack_size {
            w.u16(v);
        }
        if let Some(v) = self.container_id {
            w.u32(v.0);
        }
        if let Some(v) = self.wielder_id {
            w.u32(v.0);
        }
        if let Some(v) = self.valid_locations {
            w.u32(v);
        }
        if let Some(v) = self.location {
            w.u32(v);
        }
        if let Some(v) = self.priority {
            w.u32(v);
        }
        if let Some(v) = self.blip_color {
            w.u8(v);
        }
        if let Some(v) = self.radar_enum {
            w.u8(v);
        }
        if let Some(v) = self.pscript {
            w.u16(v);
        }
        if let Some(v) = self.workmanship {
            w.f32(v);
        }
        if let Some(v) = self.burden {
            w.u16(v);
        }
        if let Some(v) = self.spell_id {
            w.u16(v);
        }
        if let Some(v) = self.house_owner_iid {
            w.u32(v.0);
        }
        if let Some(db) = &self.restrictions {
            db.write(w)?;
        }
        if let Some(v) = self.hook_item_types {
            w.u32(v);
        }
        if let Some(v) = self.monarch {
            w.u32(v.0);
        }
        if let Some(v) = self.hook_type {
            w.u16(v);
        }
        if let Some(v) = self.icon_overlay_id {
            w.packed_data_id(ICON_BASE, v)?;
        }
        if let Some(v) = self.icon_underlay_id {
            w.packed_data_id(ICON_BASE, v)?;
        }
        if let Some(v) = self.material_type {
            w.u32(v);
        }
        if let Some(v) = self.cooldown_id {
            w.u32(v);
        }
        if let Some(v) = self.cooldown_duration {
            w.f64(v);
        }
        if let Some(v) = self.pet_owner {
            w.u32(v.0);
        }
        w.align4();
        Ok(())
    }
}

/// `ContentProfile` = `{ ObjectID iid; uint32 container_properties }`.
///
/// The object maintainer's content view puts the id in its items list when the
/// properties word is 0 and in its containers list otherwise.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct ContentProfile {
    pub iid: ObjectId,
    pub container_properties: u32,
}

impl ContentProfile {
    pub fn read(r: &mut Reader<'_>) -> Result<Self, MessageError> {
        Ok(Self {
            iid: ObjectId(r.u32()?),
            container_properties: r.u32()?,
        })
    }

    pub fn write(&self, w: &mut Writer) {
        w.u32(self.iid.0);
        w.u32(self.container_properties);
    }
}

/// `InventoryPlacement` = `{ ObjectID iid; uint32 location (EquipMask); uint32 priority }`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct InventoryPlacement {
    pub iid: ObjectId,
    pub location: u32,
    pub priority: u32,
}

impl InventoryPlacement {
    pub fn read(r: &mut Reader<'_>) -> Result<Self, MessageError> {
        Ok(Self {
            iid: ObjectId(r.u32()?),
            location: r.u32()?,
            priority: r.u32()?,
        })
    }

    pub fn write(&self, w: &mut Writer) {
        w.u32(self.iid.0);
        w.u32(self.location);
        w.u32(self.priority);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Oracle: the public weenie description's wire order
    /// (`docs/networking/messages/02-world-objects.md` §3.3).
    #[test]
    fn weenie_desc_round_trips_a_typical_item() {
        let d = PublicWeenieDesc {
            header: header::VALUE | header::STACK_SIZE | header::MAX_STACK_SIZE | header::BURDEN,
            name: "Pyreal".to_string(),
            wcid: 273,
            icon_id: ICON_BASE + 0x100E,
            obj_type: 0x0000_0040,
            bitfield: 0,
            value: Some(1000),
            stack_size: Some(25),
            max_stack_size: Some(25_000),
            burden: Some(5),
            ..PublicWeenieDesc::default()
        };
        let mut w = Writer::new();
        d.write(&mut w).unwrap();
        let bytes = w.into_inner();
        let mut r = Reader::new(&bytes);
        assert_eq!(PublicWeenieDesc::read(&mut r).unwrap(), d);
        r.expect_exhausted().unwrap();
    }

    /// `0x04000000` means two different things in the two words. In `_bitfield` it says a second
    /// header dword follows; in `header` it is the `RestrictionDB`. Both at once must work.
    #[test]
    fn the_two_meanings_of_0x04000000_coexist() {
        let d = PublicWeenieDesc {
            header: header::RESTRICTIONS,
            name: "Cottage".to_string(),
            wcid: 9686,
            icon_id: ICON_BASE + 1,
            obj_type: 0x0000_1000,
            bitfield: BITFIELD_INCLUDES_SECOND_HEADER,
            header2: Some(header2::PET_OWNER),
            restrictions: Some(RestrictionDb {
                version: RestrictionDb::CURRENT_VERSION,
                bitmask: 1,
                monarch_iid: ObjectId(0x5000_0001),
                table: PHash::new(vec![(ObjectId(0x5000_0002), 1)]),
            }),
            pet_owner: Some(ObjectId(0x5000_0003)),
            ..PublicWeenieDesc::default()
        };
        let mut w = Writer::new();
        d.write(&mut w).unwrap();
        let bytes = w.into_inner();
        let mut r = Reader::new(&bytes);
        assert_eq!(PublicWeenieDesc::read(&mut r).unwrap(), d);
        r.expect_exhausted().unwrap();
    }

    /// Oracle:, which masks the high half with `0x7FFF`
    /// and has no base — unlike every other packed id in the protocol.
    #[test]
    fn the_weenie_class_id_uses_its_own_packing() {
        for id in [0u32, 1, 0x7FFF, 0x8000, 0x0012_3456] {
            let mut w = Writer::new();
            write_wclass_id(&mut w, id).unwrap();
            assert_eq!(w.len(), if id < 0x8000 { 2 } else { 4 }, "size for {id}");
            let bytes = w.into_inner();
            let mut r = Reader::new(&bytes);
            assert_eq!(read_wclass_id(&mut r).unwrap(), id);
        }
    }

    /// Oracle: retail behaviour. The three
    /// shapes are distinguished by the first dword, not by a length.
    #[test]
    fn restriction_db_has_three_shapes() {
        // Current form.
        let db = RestrictionDb {
            version: RestrictionDb::CURRENT_VERSION,
            bitmask: 1,
            monarch_iid: ObjectId(0x5000_00AA),
            table: PHash::new(vec![(ObjectId(0x5000_00BB), 0), (ObjectId(0x5000_00CC), 1)]),
        };
        let mut w = Writer::new();
        db.write(&mut w).unwrap();
        let bytes = w.into_inner();
        assert_eq!(bytes.len(), 4 + 4 + 4 + 4 + 16);
        let mut r = Reader::new(&bytes);
        assert_eq!(RestrictionDb::read(&mut r).unwrap(), db);
        r.expect_exhausted().unwrap();

        // Oldest form: the first dword's high word is zero, so it is the bitmask itself.
        let old = RestrictionDb {
            version: 0,
            bitmask: 1,
            monarch_iid: ObjectId(0),
            table: PHash::new(vec![]),
        };
        let mut w = Writer::new();
        old.write(&mut w).unwrap();
        assert_eq!(w.len(), 8, "bitmask dword plus an empty table header");
        let bytes = w.into_inner();
        let mut r = Reader::new(&bytes);
        assert_eq!(RestrictionDb::read(&mut r).unwrap(), old);
    }
}
