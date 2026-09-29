//! Family: qualities, property updates and enchantments —
//! `docs/networking/messages/03-qualities-and-updates.md`.
//!
//! Forty-six small game events -- thirty updates and sixteen removes -- keep the client's copy of
//! every object's qualities up to date: for each quality type a **public** form (carrying an object
//! id) and a **private** form (implicitly the player), plus a **remove** pair for the eight table
//! types. There is no
//! `RemoveSkill` or `RemoveAttribute`: attributes, vitals and skills can only be updated.
//!
//! Two shapes cover all of them, and **neither pads after the sequence byte**:
//!
//! ```text
//! private  [u32 type][u8 seq][u32 property][value…]
//! public   [u32 type][u8 seq][u32 object][u32 property][value…]
//! ```
//!
//! so the property id at offset 5 (private) or the object id at offset 5 (public) is *unaligned*.
//! The client reads it with an unaligned dword load, which x86 permits. The string updates
//! (`0x02D5`/`0x02D6`) are the **only** ones with an alignment step, and it comes immediately
//! before the string.
//!
//! Every one of these carries an 8-bit sequence validated by `PropertySequenceGate`; see
//! [`stat_type`] for the key tag and `dereth_client_net::client_session::stamper` for the gate itself.

use crate::archive::{PHash, Reader, Writer};
use crate::error::MessageError;
use crate::opcodes::Opcode;
use crate::types::qualities::{Attribute, Enchantment, SecondaryAttribute, Skill};
use crate::types::PositionWire;
use crate::Message;
use dereth_primitives::ObjectId;

/// The `StatType` values that tag a `PropertySequenceGate` key: `key = propertyId | (StatType << 16)`.
///
/// Note that **skill, skill level and skill AC share tag 4**, and **attribute and attribute level
/// share tag 8**, so the three (respectively two) messages for one skill or attribute share a single
/// sequence counter. Sending them out of order loses updates.
pub mod stat_type {
    pub const INT: u32 = 1;
    pub const FLOAT: u32 = 2;
    pub const POSITION: u32 = 3;
    pub const SKILL: u32 = 4;
    pub const STRING: u32 = 5;
    pub const DID: u32 = 6;
    pub const IID: u32 = 7;
    pub const ATTRIBUTE: u32 = 8;
    pub const ATTRIBUTE_2ND: u32 = 9;
    pub const BODY_DAMAGE_VALUE: u32 = 10;
    pub const BODY_DAMAGE_VARIANCE: u32 = 11;
    pub const BODY_ARMOR_VALUE: u32 = 12;
    pub const BOOL: u32 = 13;
    pub const INT64: u32 = 14;
    pub const NUM_STAT_TYPES: u32 = 15;

    /// Timestamp key used when updating this quality.
    #[must_use]
    pub const fn key(stat_type: u32, property_id: u32) -> u32 {
        property_id | (stat_type << 16)
    }
}

/// `PropertyInt::StackSize` — 12. The stack-size message shares this property's sequence space, so
/// its `PropertySequenceGate` key is `0x1000C`.
pub const STACK_SIZE_PROPERTY: u32 = 12;

/// One quality value on the wire.
pub trait QualityValue: Sized + std::fmt::Debug + PartialEq + Clone + Default {
    fn read(r: &mut Reader<'_>) -> Result<Self, MessageError>;
    fn write(&self, w: &mut Writer) -> Result<(), MessageError>;
}

macro_rules! scalar_value {
    ($t:ty, $rd:ident, $wr:ident) => {
        impl QualityValue for $t {
            fn read(r: &mut Reader<'_>) -> Result<Self, MessageError> {
                r.$rd()
            }
            fn write(&self, w: &mut Writer) -> Result<(), MessageError> {
                w.$wr(*self);
                Ok(())
            }
        }
    };
}

scalar_value!(i32, i32, i32);
scalar_value!(i64, i64, i64);
scalar_value!(u32, u32, u32);
scalar_value!(f64, f64, f64);

impl QualityValue for ObjectId {
    fn read(r: &mut Reader<'_>) -> Result<Self, MessageError> {
        Ok(Self(r.u32()?))
    }
    fn write(&self, w: &mut Writer) -> Result<(), MessageError> {
        w.u32(self.0);
        Ok(())
    }
}

impl QualityValue for PositionWire {
    fn read(r: &mut Reader<'_>) -> Result<Self, MessageError> {
        Self::read(r)
    }
    fn write(&self, w: &mut Writer) -> Result<(), MessageError> {
        Self::write(self, w);
        Ok(())
    }
}

impl QualityValue for Skill {
    fn read(r: &mut Reader<'_>) -> Result<Self, MessageError> {
        Self::read(r)
    }
    fn write(&self, w: &mut Writer) -> Result<(), MessageError> {
        Self::write(self, w);
        Ok(())
    }
}

impl QualityValue for Attribute {
    fn read(r: &mut Reader<'_>) -> Result<Self, MessageError> {
        Self::read(r)
    }
    fn write(&self, w: &mut Writer) -> Result<(), MessageError> {
        Self::write(self, w);
        Ok(())
    }
}

impl QualityValue for SecondaryAttribute {
    fn read(r: &mut Reader<'_>) -> Result<Self, MessageError> {
        Self::read(r)
    }
    fn write(&self, w: &mut Writer) -> Result<(), MessageError> {
        Self::write(self, w);
        Ok(())
    }
}

/// The string value of `0x02D5`/`0x02D6`, which is preceded by an **align to 4**.
///
/// The string-update handler aligns the read cursor before
/// unpacking the narrow string. This is the only quality event with an alignment step.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct AlignedString(pub String);

impl QualityValue for AlignedString {
    fn read(r: &mut Reader<'_>) -> Result<Self, MessageError> {
        r.align4()?;
        Ok(Self(r.pstring()?))
    }
    fn write(&self, w: &mut Writer) -> Result<(), MessageError> {
        w.align4();
        w.pstring(&self.0)
    }
}

/// The private (player-implicit) update shape.
#[derive(Debug, Clone, PartialEq, Default)]
pub struct PrivateUpdate<V> {
    pub sequence: u8,
    pub property_id: u32,
    pub value: V,
}

/// The public update shape.
#[derive(Debug, Clone, PartialEq, Default)]
pub struct PublicUpdate<V> {
    pub sequence: u8,
    pub object: ObjectId,
    pub property_id: u32,
    pub value: V,
}

/// The private remove shape — 9 bytes with the opcode.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct PrivateRemove {
    pub sequence: u8,
    pub property_id: u32,
}

/// The public remove shape — 13 bytes with the opcode.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct PublicRemove {
    pub sequence: u8,
    pub object: ObjectId,
    pub property_id: u32,
}

macro_rules! quality_messages {
    ($($(#[$m:meta])* $name:ident = $op:ident, $shape:ident $(<$val:ty>)?, $tag:expr;)*) => {
        $(
            $(#[$m])*
            #[derive(Debug, Clone, PartialEq, Default)]
            pub struct $name(pub $shape $(<$val>)?);

            impl $name {
                /// The `StatType` whose sequence space this message shares.
                pub const STAT_TYPE: u32 = $tag;

                /// The `PropertySequenceGate` key this message's sequence byte is checked against.
                #[must_use]
                pub fn stamper_key(&self) -> u32 {
                    stat_type::key(Self::STAT_TYPE, self.0.property_id)
                }
            }

            impl Message for $name {
                const OPCODE: Opcode = Opcode::$op;

                fn read(r: &mut Reader<'_>) -> Result<Self, MessageError> {
                    Ok(Self($shape::read_body(r)?))
                }

                fn write(&self, w: &mut Writer) -> Result<(), MessageError> {
                    self.0.write_body(w)
                }
            }
        )*
    };
}

impl<V: QualityValue> PrivateUpdate<V> {
    fn read_body(r: &mut Reader<'_>) -> Result<Self, MessageError> {
        Ok(Self {
            sequence: r.u8()?,
            property_id: r.u32()?,
            value: V::read(r)?,
        })
    }

    fn write_body(&self, w: &mut Writer) -> Result<(), MessageError> {
        w.u8(self.sequence);
        w.u32(self.property_id);
        self.value.write(w)
    }
}

impl<V: QualityValue> PublicUpdate<V> {
    fn read_body(r: &mut Reader<'_>) -> Result<Self, MessageError> {
        Ok(Self {
            sequence: r.u8()?,
            object: ObjectId(r.u32()?),
            property_id: r.u32()?,
            value: V::read(r)?,
        })
    }

    fn write_body(&self, w: &mut Writer) -> Result<(), MessageError> {
        w.u8(self.sequence);
        w.u32(self.object.0);
        w.u32(self.property_id);
        self.value.write(w)
    }
}

impl PrivateRemove {
    fn read_body(r: &mut Reader<'_>) -> Result<Self, MessageError> {
        Ok(Self {
            sequence: r.u8()?,
            property_id: r.u32()?,
        })
    }

    fn write_body(&self, w: &mut Writer) -> Result<(), MessageError> {
        w.u8(self.sequence);
        w.u32(self.property_id);
        Ok(())
    }
}

impl PublicRemove {
    fn read_body(r: &mut Reader<'_>) -> Result<Self, MessageError> {
        Ok(Self {
            sequence: r.u8()?,
            object: ObjectId(r.u32()?),
            property_id: r.u32()?,
        })
    }

    fn write_body(&self, w: &mut Writer) -> Result<(), MessageError> {
        w.u8(self.sequence);
        w.u32(self.object.0);
        w.u32(self.property_id);
        Ok(())
    }
}

quality_messages! {
    /// `0x02CD`.
    QualitiesPrivateUpdateInt = QUALITIES_PRIVATE_UPDATE_INT, PrivateUpdate<i32>, stat_type::INT;
    /// `0x02CE`.
    QualitiesUpdateInt = QUALITIES_UPDATE_INT, PublicUpdate<i32>, stat_type::INT;
    /// `0x02CF`.
    QualitiesPrivateUpdateInt64 = QUALITIES_PRIVATE_UPDATE_INT64, PrivateUpdate<i64>, stat_type::INT64;
    /// `0x02D0`.
    QualitiesUpdateInt64 = QUALITIES_UPDATE_INT64, PublicUpdate<i64>, stat_type::INT64;
    /// `0x02D1`. The value is an `int32`, not a byte.
    QualitiesPrivateUpdateBool = QUALITIES_PRIVATE_UPDATE_BOOL, PrivateUpdate<i32>, stat_type::BOOL;
    /// `0x02D2`.
    QualitiesUpdateBool = QUALITIES_UPDATE_BOOL, PublicUpdate<i32>, stat_type::BOOL;
    /// `0x02D3`. The value is a **`double`**, not a float.
    QualitiesPrivateUpdateFloat = QUALITIES_PRIVATE_UPDATE_FLOAT, PrivateUpdate<f64>, stat_type::FLOAT;
    /// `0x02D4`.
    QualitiesUpdateFloat = QUALITIES_UPDATE_FLOAT, PublicUpdate<f64>, stat_type::FLOAT;
    /// `0x02D5` — the value is preceded by an align to 4..
    QualitiesPrivateUpdateString = QUALITIES_PRIVATE_UPDATE_STRING, PrivateUpdate<AlignedString>, stat_type::STRING;
    /// `0x02D6` — the other arm of the same helper.
    QualitiesUpdateString = QUALITIES_UPDATE_STRING, PublicUpdate<AlignedString>, stat_type::STRING;
    /// `0x02D7`.
    QualitiesPrivateUpdateDataId = QUALITIES_PRIVATE_UPDATE_DATA_ID, PrivateUpdate<u32>, stat_type::DID;
    /// `0x02D8`.
    QualitiesUpdateDataId = QUALITIES_UPDATE_DATA_ID, PublicUpdate<u32>, stat_type::DID;
    /// `0x02D9`.
    QualitiesPrivateUpdateInstanceId = QUALITIES_PRIVATE_UPDATE_INSTANCE_ID, PrivateUpdate<ObjectId>, stat_type::IID;
    /// `0x02DA`.
    QualitiesUpdateInstanceId = QUALITIES_UPDATE_INSTANCE_ID, PublicUpdate<ObjectId>, stat_type::IID;
    /// `0x02DB`.
    QualitiesPrivateUpdatePosition = QUALITIES_PRIVATE_UPDATE_POSITION, PrivateUpdate<PositionWire>, stat_type::POSITION;
    /// `0x02DC`.
    QualitiesUpdatePosition = QUALITIES_UPDATE_POSITION, PublicUpdate<PositionWire>, stat_type::POSITION;
    /// `0x02DD`.
    QualitiesPrivateUpdateSkill = QUALITIES_PRIVATE_UPDATE_SKILL, PrivateUpdate<Skill>, stat_type::SKILL;
    /// `0x02DE`.
    QualitiesUpdateSkill = QUALITIES_UPDATE_SKILL, PublicUpdate<Skill>, stat_type::SKILL;
    /// `0x02DF`. Shares the skill sequence space with `0x02DD` and `0x02E1`.
    QualitiesPrivateUpdateSkillLevel = QUALITIES_PRIVATE_UPDATE_SKILL_LEVEL, PrivateUpdate<u32>, stat_type::SKILL;
    /// `0x02E0`.
    QualitiesUpdateSkillLevel = QUALITIES_UPDATE_SKILL_LEVEL, PublicUpdate<u32>, stat_type::SKILL;
    /// `0x02E1` — the skill advancement class.
    QualitiesPrivateUpdateSkillAc = QUALITIES_PRIVATE_UPDATE_SKILL_AC, PrivateUpdate<u32>, stat_type::SKILL;
    /// `0x02E2`.
    QualitiesUpdateSkillAc = QUALITIES_UPDATE_SKILL_AC, PublicUpdate<u32>, stat_type::SKILL;
    /// `0x02E3`.
    QualitiesPrivateUpdateAttribute = QUALITIES_PRIVATE_UPDATE_ATTRIBUTE, PrivateUpdate<Attribute>, stat_type::ATTRIBUTE;
    /// `0x02E4`.
    QualitiesUpdateAttribute = QUALITIES_UPDATE_ATTRIBUTE, PublicUpdate<Attribute>, stat_type::ATTRIBUTE;
    /// `0x02E5`. Shares the attribute sequence space with `0x02E3`.
    QualitiesPrivateUpdateAttributeLevel = QUALITIES_PRIVATE_UPDATE_ATTRIBUTE_LEVEL, PrivateUpdate<u32>, stat_type::ATTRIBUTE;
    /// `0x02E6`.
    QualitiesUpdateAttributeLevel = QUALITIES_UPDATE_ATTRIBUTE_LEVEL, PublicUpdate<u32>, stat_type::ATTRIBUTE;
    /// `0x02E7` — a vital: an `Attribute` plus its current value.
    QualitiesPrivateUpdateAttribute2nd = QUALITIES_PRIVATE_UPDATE_ATTRIBUTE2ND, PrivateUpdate<SecondaryAttribute>, stat_type::ATTRIBUTE_2ND;
    /// `0x02E8`.
    QualitiesUpdateAttribute2nd = QUALITIES_UPDATE_ATTRIBUTE2ND, PublicUpdate<SecondaryAttribute>, stat_type::ATTRIBUTE_2ND;
    /// `0x02E9` — what regeneration ticks use.
    QualitiesPrivateUpdateAttribute2ndLevel = QUALITIES_PRIVATE_UPDATE_ATTRIBUTE2ND_LEVEL, PrivateUpdate<u32>, stat_type::ATTRIBUTE_2ND;
    /// `0x02EA`.
    QualitiesUpdateAttribute2ndLevel = QUALITIES_UPDATE_ATTRIBUTE2ND_LEVEL, PublicUpdate<u32>, stat_type::ATTRIBUTE_2ND;

    /// `0x01D1`.
    QualitiesPrivateRemoveInt = QUALITIES_PRIVATE_REMOVE_INT_EVENT, PrivateRemove, stat_type::INT;
    /// `0x01D2`.
    QualitiesRemoveInt = QUALITIES_REMOVE_INT_EVENT, PublicRemove, stat_type::INT;
    /// `0x01D3`.
    QualitiesPrivateRemoveBool = QUALITIES_PRIVATE_REMOVE_BOOL_EVENT, PrivateRemove, stat_type::BOOL;
    /// `0x01D4`.
    QualitiesRemoveBool = QUALITIES_REMOVE_BOOL_EVENT, PublicRemove, stat_type::BOOL;
    /// `0x01D5`.
    QualitiesPrivateRemoveFloat = QUALITIES_PRIVATE_REMOVE_FLOAT_EVENT, PrivateRemove, stat_type::FLOAT;
    /// `0x01D6`.
    QualitiesRemoveFloat = QUALITIES_REMOVE_FLOAT_EVENT, PublicRemove, stat_type::FLOAT;
    /// `0x01D7`.
    QualitiesPrivateRemoveString = QUALITIES_PRIVATE_REMOVE_STRING_EVENT, PrivateRemove, stat_type::STRING;
    /// `0x01D8`.
    QualitiesRemoveString = QUALITIES_REMOVE_STRING_EVENT, PublicRemove, stat_type::STRING;
    /// `0x01D9`.
    QualitiesPrivateRemoveDataId = QUALITIES_PRIVATE_REMOVE_DATA_IDEVENT, PrivateRemove, stat_type::DID;
    /// `0x01DA`.
    QualitiesRemoveDataId = QUALITIES_REMOVE_DATA_IDEVENT, PublicRemove, stat_type::DID;
    /// `0x01DB`.
    QualitiesPrivateRemoveInstanceId = QUALITIES_PRIVATE_REMOVE_INSTANCE_IDEVENT, PrivateRemove, stat_type::IID;
    /// `0x01DC`.
    QualitiesRemoveInstanceId = QUALITIES_REMOVE_INSTANCE_IDEVENT, PublicRemove, stat_type::IID;
    /// `0x01DD`.
    QualitiesPrivateRemovePosition = QUALITIES_PRIVATE_REMOVE_POSITION_EVENT, PrivateRemove, stat_type::POSITION;
    /// `0x01DE`.
    QualitiesRemovePosition = QUALITIES_REMOVE_POSITION_EVENT, PublicRemove, stat_type::POSITION;
    /// `0x02B8`.
    QualitiesPrivateRemoveInt64 = QUALITIES_PRIVATE_REMOVE_INT64_EVENT, PrivateRemove, stat_type::INT64;
    /// `0x02B9`.
    QualitiesRemoveInt64 = QUALITIES_REMOVE_INT64_EVENT, PublicRemove, stat_type::INT64;
}

// ---------------------------------------------------------------------------------------------
// Enchantments and the spellbook
// ---------------------------------------------------------------------------------------------

/// The leading dword of every enchantment message, which is the event type repeated. The client's
/// The magic UI dispatch guard reads it as the double opcode check; [`crate::read_checked`] is
/// the same test, so the field is not carried on these structs.
///
/// `0x02C2 Magic_UpdateEnchantment` — `[Enchantment]`.
#[derive(Debug, Clone, Copy, PartialEq, Default)]
pub struct MagicUpdateEnchantment(pub Enchantment);

impl Message for MagicUpdateEnchantment {
    const OPCODE: Opcode = Opcode::MAGIC_UPDATE_ENCHANTMENT;

    fn read(r: &mut Reader<'_>) -> Result<Self, MessageError> {
        Ok(Self(Enchantment::read(r)?))
    }

    fn write(&self, w: &mut Writer) -> Result<(), MessageError> {
        self.0.write(w)
    }
}

/// `0x02C4 Magic_UpdateMultipleEnchantments` — `PackableList<Enchantment>`.
#[derive(Debug, Clone, PartialEq, Default)]
pub struct MagicUpdateMultipleEnchantments(pub Vec<Enchantment>);

impl Message for MagicUpdateMultipleEnchantments {
    const OPCODE: Opcode = Opcode::MAGIC_UPDATE_MULTIPLE_ENCHANTMENTS;

    fn read(r: &mut Reader<'_>) -> Result<Self, MessageError> {
        Ok(Self(r.packed_list(Enchantment::read)?))
    }

    fn write(&self, w: &mut Writer) -> Result<(), MessageError> {
        w.packed_list(&self.0, |w, e| e.write(w))
    }
}

/// A message whose body is one layered spell id (`spellId | layer << 16`).
macro_rules! layered_spell_message {
    ($(#[$m:meta])* $name:ident, $op:ident) => {
        $(#[$m])*
        #[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
        pub struct $name {
            /// `spellId | (layer << 16)` — the same encoding as an enchantment id.
            pub layered_spell_id: u32,
        }

        impl Message for $name {
            const OPCODE: Opcode = Opcode::$op;

            fn read(r: &mut Reader<'_>) -> Result<Self, MessageError> {
                Ok(Self { layered_spell_id: r.u32()? })
            }

            fn write(&self, w: &mut Writer) -> Result<(), MessageError> {
                w.u32(self.layered_spell_id);
                Ok(())
            }
        }
    };
}

layered_spell_message!(
    /// `0x02C3 Magic_RemoveEnchantment` — removal **with** the "spell expired" chat line.
    MagicRemoveEnchantment,
    MAGIC_REMOVE_ENCHANTMENT
);
layered_spell_message!(
    /// `0x02C7 Magic_DispelEnchantment` — byte-identical to `0x02C3`; the only difference is that
    /// the handler passes `notify = false`, so a dispel is silent.
    MagicDispelEnchantment,
    MAGIC_DISPEL_ENCHANTMENT
);
layered_spell_message!(
    /// `0x02C1 Magic_UpdateSpell` — a spellbook add.
    MagicUpdateSpell,
    MAGIC_UPDATE_SPELL
);
layered_spell_message!(
    /// `0x01A8 Magic_RemoveSpell` — a spellbook removal. The client also *sends* this opcode.
    MagicRemoveSpell,
    MAGIC_REMOVE_SPELL
);

/// A message whose body is a `PackableList<uint32>` of layered spell ids.
macro_rules! layered_spell_list_message {
    ($(#[$m:meta])* $name:ident, $op:ident) => {
        $(#[$m])*
        #[derive(Debug, Clone, PartialEq, Eq, Default)]
        pub struct $name(pub Vec<u32>);

        impl Message for $name {
            const OPCODE: Opcode = Opcode::$op;

            fn read(r: &mut Reader<'_>) -> Result<Self, MessageError> {
                Ok(Self(r.packed_list(Reader::u32)?))
            }

            fn write(&self, w: &mut Writer) -> Result<(), MessageError> {
                w.packed_list(&self.0, |w, v| {
                    w.u32(*v);
                    Ok(())
                })
            }
        }
    };
}

layered_spell_list_message!(
    /// `0x02C5 Magic_RemoveMultipleEnchantments`.
    MagicRemoveMultipleEnchantments,
    MAGIC_REMOVE_MULTIPLE_ENCHANTMENTS
);
layered_spell_list_message!(
    /// `0x02C8 Magic_DispelMultipleEnchantments` — same bytes as `0x02C5`, silent.
    MagicDispelMultipleEnchantments,
    MAGIC_DISPEL_MULTIPLE_ENCHANTMENTS
);

/// A message with no body beyond the event type.
macro_rules! bodyless_message {
    ($(#[$m:meta])* $name:ident, $op:ident) => {
        $(#[$m])*
        #[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
        pub struct $name;

        impl Message for $name {
            const OPCODE: Opcode = Opcode::$op;

            fn read(_: &mut Reader<'_>) -> Result<Self, MessageError> {
                Ok(Self)
            }

            fn write(&self, _: &mut Writer) -> Result<(), MessageError> {
                Ok(())
            }
        }
    };
}

bodyless_message!(
    /// `0x02C6 Magic_PurgeEnchantments`.
    MagicPurgeEnchantments,
    MAGIC_PURGE_ENCHANTMENTS
);
bodyless_message!(
    /// `0x0312 Magic_PurgeBadEnchantments` — drops only the non-`Beneficial` ones.
    MagicPurgeBadEnchantments,
    MAGIC_PURGE_BAD_ENCHANTMENTS
);

// ---------------------------------------------------------------------------------------------
// PropertySequenceGate's own pack format
// ---------------------------------------------------------------------------------------------

/// The time stamper, pack and unpack:
/// `[uchar _house_ts][PHashTable<ulong,uchar>][align 4]`.
///
/// The house timestamp is **separate** from the property table — it is updated by
/// The house-restriction stamp, with the same 8-bit wrap rule but its own byte.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct PropertySequences {
    pub house_ts: u8,
    /// Keyed `propertyId | (StatType << 16)`.
    pub stamps: PHash<u32, u8>,
}

impl PropertySequences {
    pub fn read(r: &mut Reader<'_>) -> Result<Self, MessageError> {
        let house_ts = r.u8()?;
        let stamps = r.phash(|r| Ok((r.u32()?, r.u8()?)))?;
        r.align4()?;
        Ok(Self { house_ts, stamps })
    }

    pub fn write(&self, w: &mut Writer) -> Result<(), MessageError> {
        w.u8(self.house_ts);
        w.phash(&self.stamps, |w, k, v| {
            w.u32(*k);
            w.u8(*v);
            Ok(())
        })?;
        w.align4();
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{round_trip, write_body, OPCODES};

    /// Oracle: `docs/networking/messages/03-qualities-and-updates.md` §1 — there is **no alignment
    /// padding** after the sequence byte, so a private int update is 1 + 4 + 4 = 9 bytes of body
    /// and the property id sits at an unaligned offset. `expect_exhausted` is what proves the
    /// absence of a pad.
    #[test]
    fn a_private_update_has_no_pad_after_the_sequence_byte() {
        let m = QualitiesPrivateUpdateInt(PrivateUpdate {
            sequence: 0x2A,
            property_id: 25,
            value: 126,
        });
        let bytes = write_body(&m).unwrap();
        assert_eq!(bytes.len(), 9);
        assert_eq!(bytes[0], 0x2A);
        assert_eq!(
            &bytes[1..5],
            &25u32.to_le_bytes(),
            "the property id is at offset 5 of the blob"
        );
        let _: QualitiesPrivateUpdateInt = round_trip(&bytes);
    }

    /// The public form inserts the object id and is 13 bytes of body for an int.
    #[test]
    fn a_public_update_carries_the_object_id_before_the_property() {
        let m = QualitiesUpdateInt(PublicUpdate {
            sequence: 1,
            object: ObjectId(0x5000_0001),
            property_id: 25,
            value: 126,
        });
        let bytes = write_body(&m).unwrap();
        assert_eq!(bytes.len(), 13);
        let _: QualitiesUpdateInt = round_trip(&bytes);
    }

    /// Oracle: `docs/networking/messages/03-qualities-and-updates.md` §3 — the float updates carry
    /// a **double**, so a private float update is 1 + 4 + 8 = 13 bytes, not 9. Reading it as an
    /// `f32` would shift everything.
    #[test]
    fn float_updates_carry_a_double() {
        let m = QualitiesPrivateUpdateFloat(PrivateUpdate {
            sequence: 3,
            property_id: 12,
            value: 0.5,
        });
        let bytes = write_body(&m).unwrap();
        assert_eq!(bytes.len(), 13);
        let _: QualitiesPrivateUpdateFloat = round_trip(&bytes);
    }

    /// Oracle: the string-update codecs are the
    /// only quality events with an alignment step, and it is immediately before the string.
    #[test]
    fn only_the_string_updates_align() {
        // Private: opcode(4) + seq(1) + property(4) = blob offset 9, so the align eats 3 bytes.
        let m = QualitiesPrivateUpdateString(PrivateUpdate {
            sequence: 1,
            property_id: 1,
            value: AlignedString("Bob".into()),
        });
        let bytes = write_body(&m).unwrap();
        // 1 + 4 + 3 pad + 2 length + 3 chars + 3 pad = 16
        assert_eq!(bytes.len(), 16);
        assert_eq!(&bytes[5..8], &[0, 0, 0], "the align-to-4 pad");
        let _: QualitiesPrivateUpdateString = round_trip(&bytes);

        // Public: opcode(4) + seq(1) + object(4) + property(4) = blob offset 13, pad 3 again.
        let m = QualitiesUpdateString(PublicUpdate {
            sequence: 1,
            object: ObjectId(2),
            property_id: 1,
            value: AlignedString("Bob".into()),
        });
        let bytes = write_body(&m).unwrap();
        assert_eq!(bytes.len(), 20);
        let _: QualitiesUpdateString = round_trip(&bytes);
    }

    /// Oracle: `docs/networking/messages/03-qualities-and-updates.md` §2 — the key is
    /// `property_id | (stat_type << 16)`, and
    /// skill/skill-level/skill-AC share tag 4 while attribute/attribute-level share tag 8.
    #[test]
    fn the_stamper_key_is_property_or_stat_type_shifted() {
        assert_eq!(
            stat_type::key(stat_type::INT, STACK_SIZE_PROPERTY),
            0x0001_000C
        );

        let skill = QualitiesPrivateUpdateSkill(PrivateUpdate {
            property_id: 22,
            ..PrivateUpdate::default()
        });
        let level = QualitiesPrivateUpdateSkillLevel(PrivateUpdate {
            property_id: 22,
            ..PrivateUpdate::default()
        });
        let ac = QualitiesPrivateUpdateSkillAc(PrivateUpdate {
            property_id: 22,
            ..PrivateUpdate::default()
        });
        assert_eq!(skill.stamper_key(), 0x0004_0016);
        assert_eq!(level.stamper_key(), skill.stamper_key());
        assert_eq!(ac.stamper_key(), skill.stamper_key());

        let attr = QualitiesPrivateUpdateAttribute(PrivateUpdate {
            property_id: 1,
            ..PrivateUpdate::default()
        });
        let attr_level = QualitiesPrivateUpdateAttributeLevel(PrivateUpdate {
            property_id: 1,
            ..PrivateUpdate::default()
        });
        assert_eq!(attr.stamper_key(), 0x0008_0001);
        assert_eq!(attr_level.stamper_key(), attr.stamper_key());
    }

    /// All 44 quality-update events decode. This walks the master opcode
    /// table for the `Qualities_*` family and asserts every one is implemented here.
    #[test]
    fn all_forty_four_quality_events_are_implemented() {
        let implemented: std::collections::BTreeSet<u32> = [
            0x02CDu32, 0x02CE, 0x02CF, 0x02D0, 0x02D1, 0x02D2, 0x02D3, 0x02D4, 0x02D5, 0x02D6,
            0x02D7, 0x02D8, 0x02D9, 0x02DA, 0x02DB, 0x02DC, 0x02DD, 0x02DE, 0x02DF, 0x02E0, 0x02E1,
            0x02E2, 0x02E3, 0x02E4, 0x02E5, 0x02E6, 0x02E7, 0x02E8, 0x02E9, 0x02EA, 0x01D1, 0x01D2,
            0x01D3, 0x01D4, 0x01D5, 0x01D6, 0x01D7, 0x01D8, 0x01D9, 0x01DA, 0x01DB, 0x01DC, 0x01DD,
            0x01DE, 0x02B8, 0x02B9,
        ]
        .into_iter()
        .collect();
        // 30 updates + 16 removes = 46 opcodes. Assert against the table instead of the prose.
        let in_table: std::collections::BTreeSet<u32> = OPCODES
            .iter()
            .filter(|i| i.name.starts_with("Qualities_"))
            .map(|i| i.opcode.0)
            .collect();
        assert_eq!(
            in_table, implemented,
            "every Qualities_* opcode must have a codec"
        );
    }

    #[test]
    fn every_quality_shape_round_trips() {
        let _: QualitiesPrivateUpdateInt64 = round_trip(
            &write_body(&QualitiesPrivateUpdateInt64(PrivateUpdate {
                sequence: 1,
                property_id: 1,
                value: -5,
            }))
            .unwrap(),
        );
        let _: QualitiesUpdatePosition = round_trip(
            &write_body(&QualitiesUpdatePosition(PublicUpdate {
                sequence: 1,
                object: ObjectId(2),
                property_id: 3,
                value: PositionWire::default(),
            }))
            .unwrap(),
        );
        let _: QualitiesPrivateUpdateSkill = round_trip(
            &write_body(&QualitiesPrivateUpdateSkill(PrivateUpdate {
                sequence: 1,
                property_id: 22,
                value: Skill {
                    format_version: 1,
                    ..Skill::default()
                },
            }))
            .unwrap(),
        );
        let _: QualitiesUpdateAttribute2nd = round_trip(
            &write_body(&QualitiesUpdateAttribute2nd(PublicUpdate {
                sequence: 1,
                object: ObjectId(2),
                property_id: 1,
                value: SecondaryAttribute::default(),
            }))
            .unwrap(),
        );
        let _: QualitiesPrivateRemoveInt = round_trip(
            &write_body(&QualitiesPrivateRemoveInt(PrivateRemove {
                sequence: 1,
                property_id: 2,
            }))
            .unwrap(),
        );
        let _: QualitiesRemoveInt = round_trip(
            &write_body(&QualitiesRemoveInt(PublicRemove {
                sequence: 1,
                object: ObjectId(2),
                property_id: 3,
            }))
            .unwrap(),
        );
    }

    /// Oracle: `docs/networking/messages/03-qualities-and-updates.md` §4. Remove and dispel are
    /// byte-identical; only the handler's `notify` flag differs.
    #[test]
    fn remove_and_dispel_are_the_same_bytes() {
        let a = write_body(&MagicRemoveEnchantment {
            layered_spell_id: 0x0002_1234,
        })
        .unwrap();
        let b = write_body(&MagicDispelEnchantment {
            layered_spell_id: 0x0002_1234,
        })
        .unwrap();
        assert_eq!(a, b);
        let _: MagicRemoveEnchantment = round_trip(&a);
        let _: MagicDispelEnchantment = round_trip(&b);

        let a = write_body(&MagicRemoveMultipleEnchantments(vec![1, 2])).unwrap();
        let b = write_body(&MagicDispelMultipleEnchantments(vec![1, 2])).unwrap();
        assert_eq!(a, b);
    }

    #[test]
    fn the_enchantment_messages_round_trip() {
        let _: MagicUpdateEnchantment =
            round_trip(&write_body(&MagicUpdateEnchantment(Enchantment::default())).unwrap());
        let _: MagicUpdateMultipleEnchantments = round_trip(
            &write_body(&MagicUpdateMultipleEnchantments(vec![
                Enchantment::default(),
            ]))
            .unwrap(),
        );
        assert_eq!(write_body(&MagicPurgeEnchantments).unwrap().len(), 0);
        assert_eq!(write_body(&MagicPurgeBadEnchantments).unwrap().len(), 0);
    }

    /// Oracle: `docs/networking/messages/00-dispatch-and-queues.md` §5 — the pack is the
    /// house-restriction byte, then the per-property hash table, then an align to 4, with the house
    /// timestamp kept out of the table.
    #[test]
    fn the_stamper_pack_keeps_the_house_timestamp_separate() {
        let p = PropertySequences {
            house_ts: 7,
            stamps: PHash::new(vec![(0x0001_000C, 3), (0x0002_0001, 200)]),
        };
        let mut w = Writer::new();
        p.write(&mut w).unwrap();
        assert_eq!(w.as_slice()[0], 7);
        // 1 + 4 header + 2 * 5 = 15, padded to 16.
        assert_eq!(w.len(), 16);
        let bytes = w.into_inner();
        let mut r = Reader::new(&bytes);
        assert_eq!(PropertySequences::read(&mut r).unwrap(), p);
        r.expect_exhausted().unwrap();
    }
}
