//! The property → `PublicWeenieDesc` mirror: which property updates the client copies into an
//! object's public description, and into which field.
//!
//! The integer-stat update and its three sibling property handlers apply it. The client
//! references the numbers, not the names; the names in the table's comments are the community's.
//!
//! The client applies this to **every** object, whether or not it has a player description. A
//! server building an object's description from its properties should arrive at the same fields;
//! the table is the one statement of the mapping both sides read.

use crate::weenie::bitfield;

/// Which of the four property tables an entry reads.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum MirrorStat {
    Int,
    DataId,
    InstanceId,
    Bool,
}

/// The public-description field a property lands in.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum PwdField {
    /// `ITEM_TYPE`; the icon is re-rendered when the object is valid.
    ObjType,
    Priority,
    ItemsCapacity,
    ContainersCapacity,
    ValidLocations,
    /// The wield location. Guarded: skipped while the object is the subject of the in-flight
    /// inventory request or is owned by the player.
    Location,
    MaxStackSize,
    StackSize,
    Useability,
    /// The effects word; re-renders the icon.
    Effects,
    Value,
    AmmoType,
    CombatUse,
    MaxStructure,
    Structure,
    BlipColor,
    RadarEnum,
    /// The player-killer status onto three bits of the bitfield ([`set_player_killer_status`]).
    PlayerKillerStatus,
    HookType,
    HookItemTypes,
    /// The icon; re-renders it.
    IconId,
    PScript,
    /// The icon overlay; re-renders the icon.
    IconOverlay,
    /// The icon underlay; re-renders the icon.
    IconUnderlay,
    /// Guarded (see [`PwdField::Location`]); a full move follows.
    Container,
    /// Guarded (see [`PwdField::Location`]); a full move follows.
    Wielder,
    Monarch,
    HouseOwner,
    PetOwner,
    /// One bit of the bitfield, set to the value (or to its inverse).
    Bit {
        mask: u32,
        inverted: bool,
    },
}

/// One row of the mirror.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct PwdMirror {
    pub stat: MirrorStat,
    pub id: u32,
    pub field: PwdField,
}

const fn row(stat: MirrorStat, id: u32, field: PwdField) -> PwdMirror {
    PwdMirror { stat, id, field }
}

/// Every property the client mirrors into the public description, in the order its handlers test
/// them.
pub const PWD_MIRROR: &[PwdMirror] = {
    use MirrorStat::{Bool, DataId, InstanceId, Int};
    use PwdField as F;
    &[
        row(Int, 1, F::ObjType),              // ItemType
        row(Int, 4, F::Priority),             // ClothingPriority
        row(Int, 6, F::ItemsCapacity),        // ItemsCapacity
        row(Int, 7, F::ContainersCapacity),   // ContainersCapacity
        row(Int, 9, F::ValidLocations),       // ValidLocations
        row(Int, 10, F::Location),            // CurrentWieldedLocation
        row(Int, 11, F::MaxStackSize),        // MaxStackSize
        row(Int, 12, F::StackSize),           // StackSize
        row(Int, 16, F::Useability),          // ItemUseable
        row(Int, 18, F::Effects),             // UiEffects
        row(Int, 19, F::Value),               // Value
        row(Int, 50, F::AmmoType),            // AmmoType
        row(Int, 51, F::CombatUse),           // CombatUse
        row(Int, 91, F::MaxStructure),        // MaxStructure
        row(Int, 92, F::Structure),           // Structure
        row(Int, 95, F::BlipColor),           // blip colour
        row(Int, 133, F::RadarEnum),          // radar behaviour
        row(Int, 134, F::PlayerKillerStatus), // PlayerKillerStatus
        row(Int, 151, F::HookType),           // HookType
        row(Int, 152, F::HookItemTypes),      // HookItemType
        row(DataId, 8, F::IconId),
        row(DataId, 44, F::PScript),
        row(DataId, 50, F::IconOverlay),
        row(DataId, 52, F::IconUnderlay),
        row(InstanceId, 2, F::Container),
        row(InstanceId, 3, F::Wielder),
        row(InstanceId, 26, F::Monarch),
        row(InstanceId, 32, F::HouseOwner),
        row(InstanceId, 44, F::PetOwner),
        row(
            Bool,
            1,
            F::Bit {
                mask: bitfield::STUCK,
                inverted: false,
            },
        ),
        // Locked -> openable = !value: the client inverts.
        row(
            Bool,
            3,
            F::Bit {
                mask: bitfield::OPENABLE,
                inverted: true,
            },
        ),
        row(
            Bool,
            22,
            F::Bit {
                mask: bitfield::INSCRIBABLE,
                inverted: false,
            },
        ),
        row(
            Bool,
            24,
            F::Bit {
                mask: bitfield::UI_HIDDEN,
                inverted: false,
            },
        ),
        row(
            Bool,
            25,
            F::Bit {
                mask: bitfield::CELL_BARRIER_IMMUNE,
                inverted: false,
            },
        ),
        row(
            Bool,
            26,
            F::Bit {
                mask: bitfield::HIDDEN_ADMIN,
                inverted: false,
            },
        ),
    ]
};

/// The field property `id` of table `stat` mirrors into, or `None` when the client does not mirror
/// it.
#[must_use]
pub fn pwd_mirror_field(stat: MirrorStat, id: u32) -> Option<PwdField> {
    PWD_MIRROR
        .iter()
        .find(|r| r.stat == stat && r.id == id)
        .map(|r| r.field)
}

/// Behavior: int property 134 onto three bits of the bitfield. 4 is player killer, `0x40` PK-lite,
/// `0x20` impenetrable; any other value clears all three.
pub fn set_player_killer_status(b: &mut u32, value: i32) {
    *b &= !(bitfield::PLAYER_KILLER | bitfield::IMPENETRABLE | bitfield::PK_LITE);
    match value {
        4 => *b |= bitfield::PLAYER_KILLER,
        0x40 => *b |= bitfield::PK_LITE,
        0x20 => *b |= bitfield::IMPENETRABLE,
        _ => {}
    }
}

/// A mirrored property's value.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MirrorValue {
    Int(i32),
    DataId(u32),
    InstanceId(dereth_primitives::ObjectId),
    Bool(bool),
}

#[cfg(feature = "proto")]
pub use apply::apply_pwd_field;

#[cfg(feature = "proto")]
mod apply {
    use super::{set_player_killer_status, MirrorValue, PwdField};
    use dereth_protocol::types::weeniedesc::PublicWeenieDesc;

    /// Writes `v` into `field` of `pwd` as the client stores it: an int narrowed with a plain cast
    /// to the field's width, a data id to a script id's 16 bits, a bit set or cleared. Returns false
    /// (and writes nothing) when the value's kind is not the field's.
    #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
    pub fn apply_pwd_field(pwd: &mut PublicWeenieDesc, field: PwdField, v: MirrorValue) -> bool {
        use MirrorValue as V;
        use PwdField as F;
        match (field, v) {
            (F::ObjType, V::Int(n)) => pwd.obj_type = n as u32,
            (F::Priority, V::Int(n)) => pwd.priority = Some(n as u32),
            (F::ItemsCapacity, V::Int(n)) => pwd.items_capacity = Some(n as u8),
            (F::ContainersCapacity, V::Int(n)) => pwd.containers_capacity = Some(n as u8),
            (F::ValidLocations, V::Int(n)) => pwd.valid_locations = Some(n as u32),
            (F::Location, V::Int(n)) => pwd.location = Some(n as u32),
            (F::MaxStackSize, V::Int(n)) => pwd.max_stack_size = Some(n as u16),
            (F::StackSize, V::Int(n)) => pwd.stack_size = Some(n as u16),
            (F::Useability, V::Int(n)) => pwd.useability = Some(n as u32),
            (F::Effects, V::Int(n)) => pwd.effects = Some(n as u32),
            (F::Value, V::Int(n)) => pwd.value = Some(n as u32),
            (F::AmmoType, V::Int(n)) => pwd.ammo_type = Some(n as u16),
            (F::CombatUse, V::Int(n)) => pwd.combat_use = Some(n as u8),
            (F::MaxStructure, V::Int(n)) => pwd.max_structure = Some(n as u16),
            (F::Structure, V::Int(n)) => pwd.structure = Some(n as u16),
            (F::BlipColor, V::Int(n)) => pwd.blip_color = Some(n as u8),
            (F::RadarEnum, V::Int(n)) => pwd.radar_enum = Some(n as u8),
            (F::PlayerKillerStatus, V::Int(n)) => set_player_killer_status(&mut pwd.bitfield, n),
            (F::HookType, V::Int(n)) => pwd.hook_type = Some(n as u16),
            (F::HookItemTypes, V::Int(n)) => pwd.hook_item_types = Some(n as u32),
            (F::IconId, V::DataId(d)) => pwd.icon_id = d,
            (F::PScript, V::DataId(d)) => pwd.pscript = Some(d as u16),
            (F::IconOverlay, V::DataId(d)) => pwd.icon_overlay_id = Some(d),
            (F::IconUnderlay, V::DataId(d)) => pwd.icon_underlay_id = Some(d),
            (F::Container, V::InstanceId(id)) => pwd.container_id = Some(id),
            (F::Wielder, V::InstanceId(id)) => pwd.wielder_id = Some(id),
            (F::Monarch, V::InstanceId(id)) => pwd.monarch = Some(id),
            (F::HouseOwner, V::InstanceId(id)) => pwd.house_owner_iid = Some(id),
            (F::PetOwner, V::InstanceId(id)) => pwd.pet_owner = Some(id),
            (F::Bit { mask, inverted }, V::Bool(b)) => {
                if b != inverted {
                    pwd.bitfield |= mask;
                } else {
                    pwd.bitfield &= !mask;
                }
            }
            _ => return false,
        }
        true
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_row_is_unique_and_found_by_its_key() {
        for (i, r) in PWD_MIRROR.iter().enumerate() {
            assert!(
                PWD_MIRROR[..i]
                    .iter()
                    .all(|o| (o.stat, o.id) != (r.stat, r.id)),
                "{r:?} twice"
            );
            assert_eq!(pwd_mirror_field(r.stat, r.id), Some(r.field));
        }
        assert_eq!(PWD_MIRROR.len(), 35);
        assert_eq!(
            pwd_mirror_field(MirrorStat::Int, 5),
            None,
            "burden is not mirrored"
        );
    }

    #[cfg(feature = "proto")]
    #[test]
    fn locked_is_written_inverted_into_openable() {
        let mut pwd = dereth_protocol::types::weeniedesc::PublicWeenieDesc::default();
        let f = pwd_mirror_field(MirrorStat::Bool, 3).unwrap();
        assert!(apply_pwd_field(&mut pwd, f, MirrorValue::Bool(false)));
        assert_eq!(pwd.bitfield & bitfield::OPENABLE, bitfield::OPENABLE);
        assert!(apply_pwd_field(&mut pwd, f, MirrorValue::Bool(true)));
        assert_eq!(pwd.bitfield & bitfield::OPENABLE, 0);
        assert!(
            !apply_pwd_field(&mut pwd, f, MirrorValue::Int(1)),
            "a value of the wrong kind"
        );
    }
}
