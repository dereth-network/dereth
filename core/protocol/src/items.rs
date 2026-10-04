//! Family: inventory and items — `docs/networking/messages/07-inventory-and-items.md`.
//!
//! The containment messages the client *receives* (`0x0022`, `0x0023`, `0x019A`, `0x0024`,
//! `0x0052`, `0x0196`) and appraisal live in [`crate::objects`], with the other world-object
//! messages. What is here is the request half, the stack-size update and the salvage result.
//!
//! # The byte-packed message
//!
//! [`ItemUpdateStackSize`] (`0x0197`) is **byte-packed** in an otherwise dword-aligned protocol: an
//! 8-bit sequence at offset 4 and then an **unaligned object id at offset 5**. The community
//! catalogue documents it as aligned like the rest. Reading it as aligned dwords corrupts every
//! stack size and — because the misparse shifts the cursor — everything after it in the same blob.
//! `0x0248 House_UpdateRestrictions` has the same shape and lives in [`crate::trade`].

use crate::archive::{Reader, Writer};
use crate::error::MessageError;
use crate::opcodes::Opcode;
use crate::qualities::stat_type;
use crate::Message;
use dereth_primitives::ObjectId;

/// `0x0197 Item_UpdateStackSize` (S2C, UI queue, **unordered**).
///
/// Total 17 (0x11) bytes including the opcode; the dispatcher advances exactly `0x11`.
///
/// | offset | size | field |
/// |---:|---:|---|
/// | 0x00 | 4 | opcode |
/// | 0x04 | 1 | `sequence` |
/// | 0x05 | 4 | `item` — **unaligned** |
/// | 0x09 | 4 | `amount` |
/// | 0x0D | 4 | `new_value` |
///
/// The stack-size update first records `sequence` under timestamp key `0x1000C`, then **rejects the
/// update if
/// `amount > pwd.max_stack_size`** — silently, but the sequence number is still consumed.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct ItemUpdateStackSize {
    pub sequence: u8,
    pub item: ObjectId,
    pub amount: u32,
    pub new_value: u32,
}

impl ItemUpdateStackSize {
    /// The `PropertySequenceGate` key: the integer stat type `<< 16`, ORed with the stack-size
    /// property, = `0x1000C`.
    pub const STAMPER_KEY: u32 =
        stat_type::key(stat_type::INT, crate::qualities::STACK_SIZE_PROPERTY);
}

impl Message for ItemUpdateStackSize {
    const OPCODE: Opcode = Opcode::ITEM_UPDATE_STACK_SIZE;

    fn read(r: &mut Reader<'_>) -> Result<Self, MessageError> {
        // No align after the sequence byte: the object id is read from an odd offset.
        Ok(Self {
            sequence: r.u8()?,
            item: ObjectId(r.u32()?),
            amount: r.u32()?,
            new_value: r.u32()?,
        })
    }

    fn write(&self, w: &mut Writer) -> Result<(), MessageError> {
        w.u8(self.sequence);
        w.u32(self.item.0);
        w.u32(self.amount);
        w.u32(self.new_value);
        Ok(())
    }
}

// ---------------------------------------------------------------------------------------------
// Client → server: the inventory requests
// ---------------------------------------------------------------------------------------------

/// `0x0019 Inventory_PutItemInContainer` — 16 bytes.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct InventoryPutItemInContainer {
    pub item: ObjectId,
    pub container: ObjectId,
    /// 0-based position within the container.
    pub slot: u32,
}

impl Message for InventoryPutItemInContainer {
    const OPCODE: Opcode = Opcode::INVENTORY_PUT_ITEM_IN_CONTAINER;

    fn read(r: &mut Reader<'_>) -> Result<Self, MessageError> {
        Ok(Self {
            item: ObjectId(r.u32()?),
            container: ObjectId(r.u32()?),
            slot: r.u32()?,
        })
    }

    fn write(&self, w: &mut Writer) -> Result<(), MessageError> {
        w.u32(self.item.0);
        w.u32(self.container.0);
        w.u32(self.slot);
        Ok(())
    }
}

/// `0x001A Inventory_GetAndWieldItem` — 12 bytes.
///
/// The wield attempt sends `0x019B StackableSplitToWield` instead when the item is a stack
/// of more than one and the UI's split size is not the whole stack.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct InventoryGetAndWieldItem {
    pub item: ObjectId,
    /// `INVENTORY_LOC`.
    pub slot: u32,
}

impl Message for InventoryGetAndWieldItem {
    const OPCODE: Opcode = Opcode::INVENTORY_GET_AND_WIELD_ITEM;

    fn read(r: &mut Reader<'_>) -> Result<Self, MessageError> {
        Ok(Self {
            item: ObjectId(r.u32()?),
            slot: r.u32()?,
        })
    }

    fn write(&self, w: &mut Writer) -> Result<(), MessageError> {
        w.u32(self.item.0);
        w.u32(self.slot);
        Ok(())
    }
}

/// A game action whose body is one object id.
macro_rules! object_action {
    ($(#[$m:meta])* $name:ident, $op:ident, $field:ident) => {
        $(#[$m])*
        #[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
        pub struct $name {
            pub $field: ObjectId,
        }

        impl Message for $name {
            const OPCODE: Opcode = Opcode::$op;

            fn read(r: &mut Reader<'_>) -> Result<Self, MessageError> {
                Ok(Self { $field: ObjectId(r.u32()?) })
            }

            fn write(&self, w: &mut Writer) -> Result<(), MessageError> {
                w.u32(self.$field.0);
                Ok(())
            }
        }
    };
}

object_action!(
    /// `0x001B Inventory_DropItem` — 8 bytes.
    InventoryDropItem,
    INVENTORY_DROP_ITEM,
    item
);
object_action!(
    /// `0x0036 Inventory_UseEvent` — 8 bytes.
    InventoryUseEvent,
    INVENTORY_USE_EVENT,
    object
);
object_action!(
    /// `0x0263 Item_QueryItemMana` — 8 bytes.
    ItemQueryItemMana,
    ITEM_QUERY_ITEM_MANA,
    object
);

/// `0x0035 Inventory_UseWithTargetEvent` — 12 bytes.
///
/// Also the *tinkering* message: applying a salvage bag to an item has no dedicated opcode, it is
/// this one guarded by a local confirmation dialog.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct InventoryUseWithTargetEvent {
    /// The item being used.
    pub object: ObjectId,
    pub target: ObjectId,
}

impl Message for InventoryUseWithTargetEvent {
    const OPCODE: Opcode = Opcode::INVENTORY_USE_WITH_TARGET_EVENT;

    fn read(r: &mut Reader<'_>) -> Result<Self, MessageError> {
        Ok(Self {
            object: ObjectId(r.u32()?),
            target: ObjectId(r.u32()?),
        })
    }

    fn write(&self, w: &mut Writer) -> Result<(), MessageError> {
        w.u32(self.object.0);
        w.u32(self.target.0);
        Ok(())
    }
}

/// `0x0054 Inventory_StackableMerge` — 16 bytes.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct InventoryStackableMerge {
    pub merge_from: ObjectId,
    pub merge_to: ObjectId,
    pub amount: i32,
}

impl Message for InventoryStackableMerge {
    const OPCODE: Opcode = Opcode::INVENTORY_STACKABLE_MERGE;

    fn read(r: &mut Reader<'_>) -> Result<Self, MessageError> {
        Ok(Self {
            merge_from: ObjectId(r.u32()?),
            merge_to: ObjectId(r.u32()?),
            amount: r.i32()?,
        })
    }

    fn write(&self, w: &mut Writer) -> Result<(), MessageError> {
        w.u32(self.merge_from.0);
        w.u32(self.merge_to.0);
        w.i32(self.amount);
        Ok(())
    }
}

/// `0x0055 Inventory_StackableSplitToContainer` — 20 bytes.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct InventoryStackableSplitToContainer {
    pub stack: ObjectId,
    pub container: ObjectId,
    pub slot: u32,
    pub amount: i32,
}

impl Message for InventoryStackableSplitToContainer {
    const OPCODE: Opcode = Opcode::INVENTORY_STACKABLE_SPLIT_TO_CONTAINER;

    fn read(r: &mut Reader<'_>) -> Result<Self, MessageError> {
        Ok(Self {
            stack: ObjectId(r.u32()?),
            container: ObjectId(r.u32()?),
            slot: r.u32()?,
            amount: r.i32()?,
        })
    }

    fn write(&self, w: &mut Writer) -> Result<(), MessageError> {
        w.u32(self.stack.0);
        w.u32(self.container.0);
        w.u32(self.slot);
        w.i32(self.amount);
        Ok(())
    }
}

/// `0x0056 Inventory_StackableSplitTo3D` — 12 bytes.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct InventoryStackableSplitTo3d {
    pub stack: ObjectId,
    pub amount: i32,
}

impl Message for InventoryStackableSplitTo3d {
    const OPCODE: Opcode = Opcode::INVENTORY_STACKABLE_SPLIT_TO3_D;

    fn read(r: &mut Reader<'_>) -> Result<Self, MessageError> {
        Ok(Self {
            stack: ObjectId(r.u32()?),
            amount: r.i32()?,
        })
    }

    fn write(&self, w: &mut Writer) -> Result<(), MessageError> {
        w.u32(self.stack.0);
        w.i32(self.amount);
        Ok(())
    }
}

/// `0x019B Inventory_StackableSplitToWield` — 16 bytes.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct InventoryStackableSplitToWield {
    pub stack: ObjectId,
    /// `INVENTORY_LOC`.
    pub slot: u32,
    pub amount: i32,
}

impl Message for InventoryStackableSplitToWield {
    const OPCODE: Opcode = Opcode::INVENTORY_STACKABLE_SPLIT_TO_WIELD;

    fn read(r: &mut Reader<'_>) -> Result<Self, MessageError> {
        Ok(Self {
            stack: ObjectId(r.u32()?),
            slot: r.u32()?,
            amount: r.i32()?,
        })
    }

    fn write(&self, w: &mut Writer) -> Result<(), MessageError> {
        w.u32(self.stack.0);
        w.u32(self.slot);
        w.i32(self.amount);
        Ok(())
    }
}

/// `0x00CD Inventory_GiveObjectRequest` — 16 bytes.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct InventoryGiveObjectRequest {
    /// The recipient.
    pub target: ObjectId,
    pub item: ObjectId,
    pub amount: u32,
}

impl Message for InventoryGiveObjectRequest {
    const OPCODE: Opcode = Opcode::INVENTORY_GIVE_OBJECT_REQUEST;

    fn read(r: &mut Reader<'_>) -> Result<Self, MessageError> {
        Ok(Self {
            target: ObjectId(r.u32()?),
            item: ObjectId(r.u32()?),
            amount: r.u32()?,
        })
    }

    fn write(&self, w: &mut Writer) -> Result<(), MessageError> {
        w.u32(self.target.0);
        w.u32(self.item.0);
        w.u32(self.amount);
        Ok(())
    }
}

/// `0x027D Inventory_CreateTinkeringTool` — the **salvage** request.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct InventoryCreateTinkeringTool {
    pub tool: ObjectId,
    /// The objects to salvage.
    pub items: Vec<ObjectId>,
}

impl Message for InventoryCreateTinkeringTool {
    const OPCODE: Opcode = Opcode::INVENTORY_CREATE_TINKERING_TOOL;

    fn read(r: &mut Reader<'_>) -> Result<Self, MessageError> {
        Ok(Self {
            tool: ObjectId(r.u32()?),
            items: r.packed_list(|r| Ok(ObjectId(r.u32()?)))?,
        })
    }

    fn write(&self, w: &mut Writer) -> Result<(), MessageError> {
        w.u32(self.tool.0);
        w.packed_list(&self.items, |w, i| {
            w.u32(i.0);
            Ok(())
        })
    }
}

// ---------------------------------------------------------------------------------------------
// Server → client
// ---------------------------------------------------------------------------------------------

/// `0x0264 Item_QueryItemManaResponse`.
#[derive(Debug, Clone, Copy, PartialEq, Default)]
pub struct ItemQueryItemManaResponse {
    pub object: ObjectId,
    /// 0.0–1.0.
    pub mana: f32,
    /// Show the bar: 0/1.
    pub success: u32,
}

impl Message for ItemQueryItemManaResponse {
    const OPCODE: Opcode = Opcode::ITEM_QUERY_ITEM_MANA_RESPONSE;

    fn read(r: &mut Reader<'_>) -> Result<Self, MessageError> {
        Ok(Self {
            object: ObjectId(r.u32()?),
            mana: r.f32()?,
            success: r.u32()?,
        })
    }

    fn write(&self, w: &mut Writer) -> Result<(), MessageError> {
        w.u32(self.object.0);
        w.f32(self.mana);
        w.u32(self.success);
        Ok(())
    }
}

/// `0x00C3 Item_GetInscriptionResponse` — parsed and discarded by the client.
///
// the arm skips **4 bytes twice** before the three strings — the
// first is the object id, the second's meaning is unknown — and then destroys the strings
// immediately. Reproduce the skips exactly and discard, as the client does; do not "fix" it into a
// useful message.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct ItemGetInscriptionResponse {
    pub object: ObjectId,
    /// The second, unnamed dword the client skips.
    pub unknown: u32,
    pub inscription: String,
    pub scribe_name: String,
    pub scribe_account: String,
}

impl Message for ItemGetInscriptionResponse {
    const OPCODE: Opcode = Opcode::ITEM_GET_INSCRIPTION_RESPONSE;

    fn read(r: &mut Reader<'_>) -> Result<Self, MessageError> {
        Ok(Self {
            object: ObjectId(r.u32()?),
            unknown: r.u32()?,
            inscription: r.pstring()?,
            scribe_name: r.pstring()?,
            scribe_account: r.pstring()?,
        })
    }

    fn write(&self, w: &mut Writer) -> Result<(), MessageError> {
        w.u32(self.object.0);
        w.u32(self.unknown);
        w.pstring(&self.inscription)?;
        w.pstring(&self.scribe_name)?;
        w.pstring(&self.scribe_account)
    }
}

/// One salvage result — 16 bytes.
#[derive(Debug, Clone, Copy, PartialEq, Default)]
pub struct SalvageResult {
    pub material: u32,
    pub workmanship: f64,
    pub units: i32,
}

impl SalvageResult {
    pub fn read(r: &mut Reader<'_>) -> Result<Self, MessageError> {
        Ok(Self {
            material: r.u32()?,
            workmanship: r.f64()?,
            units: r.i32()?,
        })
    }

    pub fn write(&self, w: &mut Writer) {
        w.u32(self.material);
        w.f64(self.workmanship);
        w.i32(self.units);
    }
}

/// `0x02B4 Inventory_SalvageOperationsResultData`.
///
/// The payload is the skill used, the two lists, the augmentation bonus.
///
/// There is **no** percent-return double between the results list and the augmentation bonus. It
/// is a client-side field but is **not on the wire**: both the pack and the unpack bracket the
/// payload with the size check `first list + 8 + second list`, where the 8 is the two dwords, and
/// ACE's `GameEventSalvageOperationsResult` writes the same four fields.
#[derive(Debug, Clone, PartialEq, Default)]
pub struct SalvageResultMessage {
    pub skill_used: u32,
    pub not_salvagable: Vec<ObjectId>,
    pub results: Vec<SalvageResult>,
    /// Augmentation bonus percentage.
    pub aug_bonus: i32,
}

impl Message for SalvageResultMessage {
    const OPCODE: Opcode = Opcode::INVENTORY_SALVAGE_OPERATIONS_RESULT_DATA;

    fn read(r: &mut Reader<'_>) -> Result<Self, MessageError> {
        Ok(Self {
            skill_used: r.u32()?,
            not_salvagable: r.packed_list(|r| Ok(ObjectId(r.u32()?)))?,
            results: r.packed_list(SalvageResult::read)?,
            aug_bonus: r.i32()?,
        })
    }

    fn write(&self, w: &mut Writer) -> Result<(), MessageError> {
        w.u32(self.skill_used);
        w.packed_list(&self.not_salvagable, |w, i| {
            w.u32(i.0);
            Ok(())
        })?;
        w.packed_list(&self.results, |w, s| {
            s.write(w);
            Ok(())
        })?;
        w.i32(self.aug_bonus);
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{read_body, round_trip, write_blob, write_body};

    /// Oracle:
    /// `docs/networking/messages/07-inventory-and-items.md` §1 and
    /// `docs/CORRECTIONS.md`, both transcribing the stack-size update
    /// and the dispatcher's `0x11`-byte advance.
    #[test]
    fn update_stack_size_is_byte_packed_with_an_unaligned_object_id() {
        let m = ItemUpdateStackSize {
            sequence: 0x2A,
            item: ObjectId(0x5000_1234),
            amount: 25,
            new_value: 250,
        };
        let blob = write_blob(&m).unwrap();
        assert_eq!(
            blob.len(),
            0x11,
            "the dispatcher advances exactly 0x11 bytes"
        );
        assert_eq!(blob[4], 0x2A, "the sequence byte is at offset 4");
        assert_eq!(
            &blob[5..9],
            &0x5000_1234u32.to_le_bytes(),
            "the object id starts at offset 5, unaligned"
        );
        let _: ItemUpdateStackSize = round_trip(&blob[4..]);
    }

    /// Reading it as the community catalogue documents it — a pad after the sequence byte, then
    /// aligned dwords — takes the wrong four bytes for the object id and leaves the body short.
    /// This is the concrete failure the correction prevents.
    #[test]
    fn reading_stack_size_as_aligned_takes_the_wrong_object_id() {
        let m = ItemUpdateStackSize {
            sequence: 0x2A,
            item: ObjectId(0x5000_1234),
            amount: 25,
            new_value: 250,
        };
        let body = write_body(&m).unwrap();
        let mut r = Reader::body(&body);
        r.u8().unwrap();
        r.align4().unwrap(); // the mistake: three bytes of pad that are not there
        let wrong_id = r.u32().unwrap();
        assert_ne!(wrong_id, 0x5000_1234, "the aligned read straddles the id");
        // And the body then runs out early.
        r.u32().unwrap();
        assert!(r.u32().is_err(), "the aligned reading overruns");
    }

    /// The `PropertySequenceGate` key this message shares with `PropertyInt::StackSize`.
    #[test]
    fn the_stack_size_stamper_key_is_0x1000c() {
        assert_eq!(ItemUpdateStackSize::STAMPER_KEY, 0x0001_000C);
    }

    /// Oracle: `docs/networking/messages/07-inventory-and-items.md` §3, read against the client's
    /// salvage-result pack and unpack pair and cross-checked against ACE's
    /// `GameEventSalvageOperationsResult`, which writes exactly these four fields.
    #[test]
    fn salvage_result_data_has_no_percent_return_on_the_wire() {
        let m = SalvageResultMessage {
            skill_used: 40,
            not_salvagable: vec![],
            results: vec![SalvageResult {
                material: 60,
                workmanship: 5.5,
                units: 3,
            }],
            aug_bonus: 0,
        };
        let bytes = write_body(&m).unwrap();
        // 4 skill + 4 empty-list count + 4 list count + 16 result + 4 aug = 32. A `percentReturn`
        // double would make it 40.
        assert_eq!(bytes.len(), 32);
        let _: SalvageResultMessage = round_trip(&bytes);
    }

    #[test]
    fn every_inventory_request_round_trips() {
        let _: InventoryPutItemInContainer = round_trip(
            &write_body(&InventoryPutItemInContainer {
                item: ObjectId(1),
                container: ObjectId(2),
                slot: 3,
            })
            .unwrap(),
        );
        let _: InventoryGetAndWieldItem = round_trip(
            &write_body(&InventoryGetAndWieldItem {
                item: ObjectId(1),
                slot: 0x0010_0000,
            })
            .unwrap(),
        );
        let _: InventoryDropItem =
            round_trip(&write_body(&InventoryDropItem { item: ObjectId(1) }).unwrap());
        let _: InventoryUseEvent = round_trip(
            &write_body(&InventoryUseEvent {
                object: ObjectId(1),
            })
            .unwrap(),
        );
        let _: InventoryUseWithTargetEvent = round_trip(
            &write_body(&InventoryUseWithTargetEvent {
                object: ObjectId(1),
                target: ObjectId(2),
            })
            .unwrap(),
        );
        let _: InventoryStackableMerge = round_trip(
            &write_body(&InventoryStackableMerge {
                merge_from: ObjectId(1),
                merge_to: ObjectId(2),
                amount: 5,
            })
            .unwrap(),
        );
        let _: InventoryStackableSplitToContainer = round_trip(
            &write_body(&InventoryStackableSplitToContainer {
                stack: ObjectId(1),
                container: ObjectId(2),
                slot: 0,
                amount: 5,
            })
            .unwrap(),
        );
        let _: InventoryStackableSplitTo3d = round_trip(
            &write_body(&InventoryStackableSplitTo3d {
                stack: ObjectId(1),
                amount: 5,
            })
            .unwrap(),
        );
        let _: InventoryStackableSplitToWield = round_trip(
            &write_body(&InventoryStackableSplitToWield {
                stack: ObjectId(1),
                slot: 0x0010_0000,
                amount: 5,
            })
            .unwrap(),
        );
        let _: InventoryGiveObjectRequest = round_trip(
            &write_body(&InventoryGiveObjectRequest {
                target: ObjectId(1),
                item: ObjectId(2),
                amount: 1,
            })
            .unwrap(),
        );
        let _: InventoryCreateTinkeringTool = round_trip(
            &write_body(&InventoryCreateTinkeringTool {
                tool: ObjectId(1),
                items: vec![ObjectId(2), ObjectId(3)],
            })
            .unwrap(),
        );
        let _: ItemQueryItemMana = round_trip(
            &write_body(&ItemQueryItemMana {
                object: ObjectId(1),
            })
            .unwrap(),
        );
        let _: ItemQueryItemManaResponse = round_trip(
            &write_body(&ItemQueryItemManaResponse {
                object: ObjectId(1),
                mana: 0.5,
                success: 1,
            })
            .unwrap(),
        );
        let _: ItemGetInscriptionResponse = round_trip(
            &write_body(&ItemGetInscriptionResponse {
                object: ObjectId(1),
                unknown: 0,
                inscription: "For Bob".into(),
                scribe_name: "Alice".into(),
                scribe_account: "alice".into(),
            })
            .unwrap(),
        );
    }

    /// Every message in this family must fail on a trailing byte, which is what proves the layout.
    #[test]
    fn a_trailing_byte_is_rejected() {
        let mut bytes = write_body(&InventoryDropItem { item: ObjectId(1) }).unwrap();
        bytes.push(0);
        assert!(matches!(
            read_body::<InventoryDropItem>(&bytes),
            Err(MessageError::TrailingBytes { left: 1 })
        ));
    }
}
