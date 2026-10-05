//! Family: world objects — `docs/networking/messages/02-world-objects.md`.
//!
//! The **WorldObjects queue (10)** set: object lifecycle, appearance and physics. Sixteen opcodes reach
//! the smart box's event dispatch; the four movement ones are in
//! [`crate::movement`], the twelve here.
//!
//! Two things this family exists to get right:
//!
//! * **The names are back to front.** `0xF745 Item_CreateObject` is the *gentle* one — when the
//!   client already has the object at the same instance sequence it merges the three descriptors
//!   into the live object. `0xF7DB Item_UpdateObject` is the *hard* one — the instance-sequence
//!   branch is skipped entirely and the object is deleted and rebuilt. The two messages are
//!   byte-identical; only the handler's recreate flag differs. See
//!   `docs/CORRECTIONS.md`.
//! * **The instance-sequence gate runs before anything else**, with three outcomes, not two. That
//!   belongs to the dispatcher rather than the codec, so it lives in
//!   `dereth_client_net::client_session::dispatch::world_objects`; [`ObjectDispatchOutcome`] is the vocabulary it uses.

use crate::archive::{Reader, Writer};
use crate::error::MessageError;
use crate::opcodes::Opcode;
use crate::types::{
    AppraisalProfile, ContentProfile, ObjDesc, PhysicsDesc, PhysicsEventStamp, PublicWeenieDesc,
};
use crate::Message;
use dereth_primitives::ObjectId;

/// What the object-message gate returns for a message.
///
/// There are **three** gate outcomes, not two: an object the client does not know yet, or a message
/// whose instance sequence is *newer* than the object's, is `Queued` — parked on a placeholder and
/// replayed when the real object arrives. Dropping "future" messages instead of parking them loses
/// objects.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ObjectDispatchOutcome {
    Undef,
    ProcessedOk,
    /// The object was re-created; this message is stale. Drop it.
    OldInstance,
    /// Bad opcode or short buffer. The WorldObjects dispatcher ignores it, so the blob is dropped.
    Error,
    /// The object is not known yet. Park the blob on it and replay when it arrives.
    Queued,
}

impl ObjectDispatchOutcome {
    /// The numeric values the client's enum uses.
    #[must_use]
    pub fn code(self) -> u32 {
        match self {
            Self::Undef => 0,
            Self::ProcessedOk => 1,
            Self::OldInstance => 2,
            Self::Error => 3,
            Self::Queued => 4,
        }
    }
}

/// The newer-than test — returns true when `b` is newer than `a`, comparing two
/// `u16`s with the `0x8000` half-window.
///
/// The client writes the test as `|b − a| < 0x8000 ? a < b : b < a`. Note this is a *different*
/// shape from [`crate::wrap::newer_u16`], which is the blob-id stamp comparison: this one takes the
/// absolute difference first, so it is symmetric at exactly half the period where the other is not.
/// Both are reproduced separately rather than merged, because the client has both.
#[must_use]
pub fn is_newer(a: u16, b: u16) -> bool {
    let diff = b.abs_diff(a);
    if diff < 0x8000 {
        a < b
    } else {
        b < a
    }
}

/// The three descriptors `0xF745` and `0xF7DB` carry, in this order.
#[derive(Debug, Clone, PartialEq, Default)]
pub struct ObjectCreatePayload {
    pub id: ObjectId,
    pub objdesc: ObjDesc,
    pub physicsdesc: PhysicsDesc,
    pub wdesc: PublicWeenieDesc,
}

impl ObjectCreatePayload {
    fn read(r: &mut Reader<'_>) -> Result<Self, MessageError> {
        Ok(Self {
            id: ObjectId(r.u32()?),
            objdesc: ObjDesc::read(r)?,
            physicsdesc: PhysicsDesc::read(r)?,
            wdesc: PublicWeenieDesc::read(r)?,
        })
    }

    fn write(&self, w: &mut Writer) -> Result<(), MessageError> {
        w.u32(self.id.0);
        self.objdesc.write(w)?;
        self.physicsdesc.write(w)?;
        self.wdesc.write(w)
    }
}

/// `0xF745 Item_CreateObject` — merges into an existing object when the instance sequence matches.
#[derive(Debug, Clone, PartialEq, Default)]
pub struct ItemCreateObject(pub ObjectCreatePayload);

impl Message for ItemCreateObject {
    const OPCODE: Opcode = Opcode::ITEM_CREATE_OBJECT;

    fn read(r: &mut Reader<'_>) -> Result<Self, MessageError> {
        Ok(Self(ObjectCreatePayload::read(r)?))
    }

    fn write(&self, w: &mut Writer) -> Result<(), MessageError> {
        self.0.write(w)
    }
}

/// `0xF7DB Item_UpdateObject` — always deletes and rebuilds. Byte-identical to
/// [`ItemCreateObject`]; the difference is entirely in the handler.
#[derive(Debug, Clone, PartialEq, Default)]
pub struct ItemUpdateObject(pub ObjectCreatePayload);

impl Message for ItemUpdateObject {
    const OPCODE: Opcode = Opcode::ITEM_UPDATE_OBJECT;

    fn read(r: &mut Reader<'_>) -> Result<Self, MessageError> {
        Ok(Self(ObjectCreatePayload::read(r)?))
    }

    fn write(&self, w: &mut Writer) -> Result<(), MessageError> {
        self.0.write(w)
    }
}

/// `0xF746 Login_CreatePlayer`.
///
/// Arrives **before** the player's own `0xF745`, so the create for the player object is one of the
/// blobs the client replays once the player id is known. A second one is ignored.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct LoginCreatePlayer {
    pub player_id: ObjectId,
}

impl Message for LoginCreatePlayer {
    const OPCODE: Opcode = Opcode::LOGIN_CREATE_PLAYER;

    fn read(r: &mut Reader<'_>) -> Result<Self, MessageError> {
        Ok(Self {
            player_id: ObjectId(r.u32()?),
        })
    }

    fn write(&self, w: &mut Writer) -> Result<(), MessageError> {
        w.u32(self.player_id.0);
        Ok(())
    }
}

/// `0xF747 Item_DeleteObject`.
///
/// The trailing `u16` is the **instance** sequence, not a general one; the handler runs the same
/// three-way gate on it. `id == player_id` returns `Error`: the client never deletes itself.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct ItemDeleteObject {
    pub id: ObjectId,
    pub instance_sequence: u16,
}

impl Message for ItemDeleteObject {
    const OPCODE: Opcode = Opcode::ITEM_DELETE_OBJECT;

    fn read(r: &mut Reader<'_>) -> Result<Self, MessageError> {
        let m = Self {
            id: ObjectId(r.u32()?),
            instance_sequence: r.u16()?,
        };
        if r.remaining() > 0 {
            r.align4()?;
        }
        Ok(m)
    }

    fn write(&self, w: &mut Writer) -> Result<(), MessageError> {
        w.u32(self.id.0);
        w.u16(self.instance_sequence);
        // The retail server pads the body to four bytes (every captured instance is 12 bytes); the
        // client reads the fields at fixed offsets and never looks past them.
        w.align4();
        Ok(())
    }
}

/// `0xF625 Item_ObjDescEvent`.
#[derive(Debug, Clone, PartialEq, Default)]
pub struct ItemObjDescEvent {
    pub id: ObjectId,
    pub objdesc: ObjDesc,
    pub timestamps: PhysicsEventStamp,
}

impl Message for ItemObjDescEvent {
    const OPCODE: Opcode = Opcode::ITEM_OBJ_DESC_EVENT;

    fn read(r: &mut Reader<'_>) -> Result<Self, MessageError> {
        Ok(Self {
            id: ObjectId(r.u32()?),
            objdesc: ObjDesc::read(r)?,
            timestamps: PhysicsEventStamp::read(r)?,
        })
    }

    fn write(&self, w: &mut Writer) -> Result<(), MessageError> {
        w.u32(self.id.0);
        self.objdesc.write(w)?;
        self.timestamps.write(w);
        Ok(())
    }
}

/// `0xF749 Item_ParentEvent`.
///
/// Needs **both** objects; when the creature exists but the item does not, the blob is queued on
/// the *item* id (the handler queues the blob for the object named by the second guid).
///
/// **The guid order matters.** The first is the *holder*, the physics object that becomes
/// `parent`; the second is the item that becomes its child. The argument mapping through this
/// pair of functions:
///
/// * the handler looks up the **first** guid and gates on that object's `update_times[8]`
///   (the instance sequence) against the pack's first `u16`;
/// * it then passes that object as the parent-event handler's **second** argument and the
///   second guid's object as the first, and the handler parents its first argument to its
///   second — so the second guid is the child.
///
/// ACE agrees: `GameMessageParentEvent(creature, wieldedSelectableItem, …)` writes `creature.Guid`
/// first and the creature's `ObjectInstance` sequence as the first `u16`
/// (`Source/ACE.Server/Network/GameMessages/Messages/GameMessageParentEvent.cs`).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct ItemParentEvent {
    /// The holder — ACE's `creature`, and the physics object's `parent`.
    pub creature: ObjectId,
    /// The held object — ACE's `wieldedSelectableItem`, the physics object that gets a parent.
    pub item: ObjectId,
    /// `ParentLocation` — the key is asked for on the **holder's** setup. A miss makes the lookup
    /// return 0 and the attachment is
    /// refused outright.
    pub location: u32,
    /// `Placement` — installed on the **item's own** part array by setting the placement frame,
    /// with no `HasAnims` guard.
    pub placement_frame: u32,
    /// `instance` is the creature's `ObjectInstance` sequence, which the parent-event handler gates on;
    /// `event` is the item's `ObjectPosition` sequence, which applying the parent event gates on against
    /// `update_times[0]`.
    pub timestamps: PhysicsEventStamp,
}

impl Message for ItemParentEvent {
    const OPCODE: Opcode = Opcode::ITEM_PARENT_EVENT;

    fn read(r: &mut Reader<'_>) -> Result<Self, MessageError> {
        Ok(Self {
            creature: ObjectId(r.u32()?),
            item: ObjectId(r.u32()?),
            location: r.u32()?,
            placement_frame: r.u32()?,
            timestamps: PhysicsEventStamp::read(r)?,
        })
    }

    fn write(&self, w: &mut Writer) -> Result<(), MessageError> {
        w.u32(self.creature.0);
        w.u32(self.item.0);
        w.u32(self.location);
        w.u32(self.placement_frame);
        self.timestamps.write(w);
        Ok(())
    }
}

/// `0xF74A Inventory_PickupEvent`.
///
/// The object stays in `object_table`: it has left the 3-D world but the client still knows it.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct InventoryPickupEvent {
    pub id: ObjectId,
    pub timestamps: PhysicsEventStamp,
}

impl Message for InventoryPickupEvent {
    const OPCODE: Opcode = Opcode::INVENTORY_PICKUP_EVENT;

    fn read(r: &mut Reader<'_>) -> Result<Self, MessageError> {
        Ok(Self {
            id: ObjectId(r.u32()?),
            timestamps: PhysicsEventStamp::read(r)?,
        })
    }

    fn write(&self, w: &mut Writer) -> Result<(), MessageError> {
        w.u32(self.id.0);
        self.timestamps.write(w);
        Ok(())
    }
}

/// `0xF74B Item_SetState`.
///
/// Extra rule in the state-update handler: if the object is the player, `waiting_for_teleport` is
/// set, and the new state has `HIDDEN_PS (0x4000)` **clear**, the teleport is complete.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct ItemSetState {
    /// `PhysicsState`.
    pub state: u32,
    pub id: ObjectId,
    pub timestamps: PhysicsEventStamp,
}

/// `PhysicsState` bits the protocol layer needs to name.
pub mod physics_state {
    /// Cleared on the player's state to end a teleport.
    pub const HIDDEN_PS: u32 = 0x0000_4000;
}

impl Message for ItemSetState {
    const OPCODE: Opcode = Opcode::ITEM_SET_STATE;

    fn read(r: &mut Reader<'_>) -> Result<Self, MessageError> {
        Ok(Self {
            id: ObjectId(r.u32()?),
            state: r.u32()?,
            timestamps: PhysicsEventStamp::read(r)?,
        })
    }

    fn write(&self, w: &mut Writer) -> Result<(), MessageError> {
        w.u32(self.id.0);
        w.u32(self.state);
        self.timestamps.write(w);
        Ok(())
    }
}

/// `0xF750 Effects_SoundEvent` —. **No sequence check.**
#[derive(Debug, Clone, Copy, PartialEq, Default)]
pub struct EffectsSoundEvent {
    pub id: ObjectId,
    pub sound_type: i32,
    pub volume: f32,
}

impl Message for EffectsSoundEvent {
    const OPCODE: Opcode = Opcode::EFFECTS_SOUND_EVENT;

    fn read(r: &mut Reader<'_>) -> Result<Self, MessageError> {
        Ok(Self {
            id: ObjectId(r.u32()?),
            sound_type: r.i32()?,
            volume: r.f32()?,
        })
    }

    fn write(&self, w: &mut Writer) -> Result<(), MessageError> {
        w.u32(self.id.0);
        w.i32(self.sound_type);
        w.f32(self.volume);
        Ok(())
    }
}

/// `0xF751 Effects_PlayerTeleport`.
///
/// The handler reads only a `ushort` at +4 and **moves nothing**: the actual move arrives as a
/// position event and the teleport ends when a `SetState` without `HIDDEN_PS` arrives.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct EffectsPlayerTeleport {
    pub teleport_sequence: u16,
}

impl Message for EffectsPlayerTeleport {
    const OPCODE: Opcode = Opcode::EFFECTS_PLAYER_TELEPORT;

    fn read(r: &mut Reader<'_>) -> Result<Self, MessageError> {
        let m = Self {
            teleport_sequence: r.u16()?,
        };
        if r.remaining() > 0 {
            r.align4()?;
        }
        Ok(m)
    }

    fn write(&self, w: &mut Writer) -> Result<(), MessageError> {
        w.u16(self.teleport_sequence);
        // The retail server pads the body to four bytes (every captured instance is 8 bytes).
        w.align4();
        Ok(())
    }
}

/// `0xF754 Effects_PlayScriptID` —. No sequence check.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct EffectsPlayScriptId {
    pub id: ObjectId,
    pub script_id: u32,
}

impl Message for EffectsPlayScriptId {
    const OPCODE: Opcode = Opcode::EFFECTS_PLAY_SCRIPT_ID;

    fn read(r: &mut Reader<'_>) -> Result<Self, MessageError> {
        Ok(Self {
            id: ObjectId(r.u32()?),
            script_id: r.u32()?,
        })
    }

    fn write(&self, w: &mut Writer) -> Result<(), MessageError> {
        w.u32(self.id.0);
        w.u32(self.script_id);
        Ok(())
    }
}

/// `0xF755 Effects_PlayScriptType`.
#[derive(Debug, Clone, Copy, PartialEq, Default)]
pub struct EffectsPlayScriptType {
    pub id: ObjectId,
    /// The play-script type.
    pub script_type: i32,
    pub intensity: f32,
}

impl Message for EffectsPlayScriptType {
    const OPCODE: Opcode = Opcode::EFFECTS_PLAY_SCRIPT_TYPE;

    fn read(r: &mut Reader<'_>) -> Result<Self, MessageError> {
        Ok(Self {
            id: ObjectId(r.u32()?),
            script_type: r.i32()?,
            intensity: r.f32()?,
        })
    }

    fn write(&self, w: &mut Writer) -> Result<(), MessageError> {
        w.u32(self.id.0);
        w.i32(self.script_type);
        w.f32(self.intensity);
        Ok(())
    }
}

/// `0xF6EA Object_SendForceObjdesc` (C2S, **Control queue**) —
/// The force-objdesc send.
///
/// **Three** call sites, not one, as the recorded corpus shows:
///
/// * the object maintainer's weenie-desc setter, on the merge path, when the incoming container,
///   wielder or location differ from what the client had and its toggle latch is clear;
/// * the object maintainer's per-frame tick, **twice** — over `null_object_table` and then
///   `null_weenie_object_table`, restamping and re-asking for every entry older than **20.0 s**.
///
/// All **35** of these in the recorded corpus are the second kind: 31 of 31 consecutive gaps at
/// 20.00 s to within a millisecond, with no `0xF745` for the same object in the 10 s before any of
/// them.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct ObjectSendForceObjdesc {
    pub id: ObjectId,
}

impl Message for ObjectSendForceObjdesc {
    const OPCODE: Opcode = Opcode::OBJECT_SEND_FORCE_OBJDESC;

    fn read(r: &mut Reader<'_>) -> Result<Self, MessageError> {
        Ok(Self {
            id: ObjectId(r.u32()?),
        })
    }

    fn write(&self, w: &mut Writer) -> Result<(), MessageError> {
        w.u32(self.id.0);
        Ok(())
    }
}

// ---------------------------------------------------------------------------------------------
// The UI-queue half: the "server says" inventory messages and appraisal.
// ---------------------------------------------------------------------------------------------

/// `0x0022 Item_ServerSaysContainID`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct ItemServerSaysContainId {
    pub item: ObjectId,
    pub container: ObjectId,
    pub slot: u32,
    /// 0 = plain item list, non-zero = side pack.
    pub container_properties: u32,
}

impl Message for ItemServerSaysContainId {
    const OPCODE: Opcode = Opcode::ITEM_SERVER_SAYS_CONTAIN_ID;

    fn read(r: &mut Reader<'_>) -> Result<Self, MessageError> {
        Ok(Self {
            item: ObjectId(r.u32()?),
            container: ObjectId(r.u32()?),
            slot: r.u32()?,
            container_properties: r.u32()?,
        })
    }

    fn write(&self, w: &mut Writer) -> Result<(), MessageError> {
        w.u32(self.item.0);
        w.u32(self.container.0);
        w.u32(self.slot);
        w.u32(self.container_properties);
        Ok(())
    }
}

/// `0x0023 Item_WearItem`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct ItemWearItem {
    pub item: ObjectId,
    /// `INVENTORY_LOC` — the community catalogue's `EquipMask`.
    pub slot: u32,
}

impl Message for ItemWearItem {
    const OPCODE: Opcode = Opcode::ITEM_WEAR_ITEM;

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

id_message!(
    /// `0x019A Item_ServerSaysMoveItem` — the item leaves your inventory entirely. It is **not**
    /// destroyed; a `0xF745` usually follows for the world instance.
    ItemServerSaysMoveItem,
    ITEM_SERVER_SAYS_MOVE_ITEM,
    item
);

id_message!(
    /// `0x0024 Item_ServerSaysRemove` — the only inventory message on the UI queue that is
    /// **unordered**. Everything inside the object is queued for destruction too.
    ItemServerSaysRemove,
    ITEM_SERVER_SAYS_REMOVE,
    object
);

id_message!(
    /// `0x0052 Item_StopViewingObjectContents`.
    ItemStopViewingObjectContents,
    ITEM_STOP_VIEWING_OBJECT_CONTENTS,
    object
);

id_message!(
    /// `0x00C8 Item_Appraise` (C2S) — an ordered game action.
    ItemAppraise,
    ITEM_APPRAISE,
    target
);

id_message!(
    /// `0x0195 Inventory_NoLongerViewingContents` (C2S).
    InventoryNoLongerViewingContents,
    INVENTORY_NO_LONGER_VIEWING_CONTENTS,
    container
);

/// `0x0196 Item_OnViewContents`.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct ItemOnViewContents {
    pub container: ObjectId,
    pub contents: Vec<ContentProfile>,
}

impl Message for ItemOnViewContents {
    const OPCODE: Opcode = Opcode::ITEM_ON_VIEW_CONTENTS;

    fn read(r: &mut Reader<'_>) -> Result<Self, MessageError> {
        Ok(Self {
            container: ObjectId(r.u32()?),
            contents: r.packed_list(ContentProfile::read)?,
        })
    }

    fn write(&self, w: &mut Writer) -> Result<(), MessageError> {
        w.u32(self.container.0);
        w.packed_list(&self.contents, |w, c| {
            c.write(w);
            Ok(())
        })
    }
}

/// `0x00C9 Item_SetAppraiseInfo`.
#[derive(Debug, Clone, PartialEq, Default)]
pub struct ItemSetAppraiseInfo {
    pub object: ObjectId,
    pub profile: AppraisalProfile,
}

impl Message for ItemSetAppraiseInfo {
    const OPCODE: Opcode = Opcode::ITEM_SET_APPRAISE_INFO;

    fn read(r: &mut Reader<'_>) -> Result<Self, MessageError> {
        Ok(Self {
            object: ObjectId(r.u32()?),
            profile: AppraisalProfile::read(r)?,
        })
    }

    fn write(&self, w: &mut Writer) -> Result<(), MessageError> {
        w.u32(self.object.0);
        self.profile.write(w)
    }
}

/// `0x01CB Item_AppraiseDone` — one dword the client ignores.
///
/// The appraise-done dispatch is a stub that returns 0.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct ItemAppraiseDone {
    /// Always 0 in practice.
    pub unknown: u32,
}

impl Message for ItemAppraiseDone {
    const OPCODE: Opcode = Opcode::ITEM_APPRAISE_DONE;

    fn read(r: &mut Reader<'_>) -> Result<Self, MessageError> {
        Ok(Self { unknown: r.u32()? })
    }

    fn write(&self, w: &mut Writer) -> Result<(), MessageError> {
        w.u32(self.unknown);
        Ok(())
    }
}

/// `0x01C7 Item_UseDone` — the **universal "action finished" acknowledgement**.
///
/// It also ends spell casts, crafting and salvaging. A server that omits it leaves the client's busy
/// counter above zero for ever. `WeenieError::None` (0) means the use succeeded.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct ItemUseDone {
    pub failure_type: u32,
}

impl Message for ItemUseDone {
    const OPCODE: Opcode = Opcode::ITEM_USE_DONE;

    fn read(r: &mut Reader<'_>) -> Result<Self, MessageError> {
        Ok(Self {
            failure_type: r.u32()?,
        })
    }

    fn write(&self, w: &mut Writer) -> Result<(), MessageError> {
        w.u32(self.failure_type);
        Ok(())
    }
}

/// `0x00A0 Character_ServerSaysAttemptFailed`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct CharacterServerSaysAttemptFailed {
    pub object: ObjectId,
    /// `WeenieError`.
    pub reason: u32,
}

impl CharacterServerSaysAttemptFailed {
    /// The seven codes the arm does **not** forward to
    /// the communication system's failure-event handler, because the client has already printed a message naming
    /// the object.
    pub const SUPPRESSED_CODES: [u32; 7] = [0x01E, 0x02B, 0x3EF, 0x43E, 0x46A, 0x4CE, 0x4CF];

    /// Whether the generic failure text is suppressed for this code.
    #[must_use]
    pub fn suppresses_generic_text(reason: u32) -> bool {
        Self::SUPPRESSED_CODES.contains(&reason)
    }
}

impl Message for CharacterServerSaysAttemptFailed {
    const OPCODE: Opcode = Opcode::CHARACTER_SERVER_SAYS_ATTEMPT_FAILED;

    fn read(r: &mut Reader<'_>) -> Result<Self, MessageError> {
        Ok(Self {
            object: ObjectId(r.u32()?),
            reason: r.u32()?,
        })
    }

    fn write(&self, w: &mut Writer) -> Result<(), MessageError> {
        w.u32(self.object.0);
        w.u32(self.reason);
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::types::physicsdesc::{flags as pflags, PhysicsTimestamps};
    use crate::types::PositionWire;
    use crate::{round_trip, write_body};

    fn sample_create() -> ObjectCreatePayload {
        ObjectCreatePayload {
            id: ObjectId(0x5000_0001),
            objdesc: ObjDesc {
                ..ObjDesc::default()
            },
            physicsdesc: PhysicsDesc {
                bitfield: pflags::POSITION | pflags::SETUP,
                state: 0x0000_0400,
                position: Some(PositionWire {
                    objcell_id: 0x00A9_0125,
                    ..PositionWire::default()
                }),
                setup_id: Some(0x0200_0001),
                timestamps: PhysicsTimestamps {
                    instance: 3,
                    ..PhysicsTimestamps::default()
                },
                ..PhysicsDesc::default()
            },
            wdesc: PublicWeenieDesc {
                name: "Dagger".into(),
                wcid: 305,
                icon_id: 0x0600_1234,
                obj_type: 1,
                ..PublicWeenieDesc::default()
            },
        }
    }

    /// Oracle: `docs/networking/messages/02-world-objects.md` §1 and §3 — the body of `0xF745` and
    /// `0xF7DB` is `[id][ObjDesc][PhysicsDesc][PublicWeenieDesc]` for both, and the two differ only
    /// in the recreate argument the handler passes. The bytes must be identical.
    #[test]
    fn create_and_update_object_are_byte_identical() {
        let p = sample_create();
        let create = write_body(&ItemCreateObject(p.clone())).unwrap();
        let update = write_body(&ItemUpdateObject(p)).unwrap();
        assert_eq!(
            create, update,
            "the two messages differ only in the handler"
        );
        let _: ItemCreateObject = round_trip(&create);
        let _: ItemUpdateObject = round_trip(&update);
    }

    /// Oracle: the newer-than test of `docs/networking/messages/02-world-objects.md` §2.
    /// `is_newer(object_ts, message_ts)` means "the message is ahead of the object".
    #[test]
    fn is_newer_uses_the_absolute_half_window() {
        assert!(is_newer(1, 2), "the message is ahead");
        assert!(!is_newer(2, 1));
        assert!(!is_newer(2, 2), "equal is not newer");
        // Across the wrap: |b-a| >= 0x8000 flips the comparison.
        assert!(is_newer(0xFFFF, 0x0001));
        assert!(!is_newer(0x0001, 0xFFFF));
    }

    /// Oracle: `docs/networking/messages/02-world-objects.md` §4 — `0xF747`'s trailing `u16` is the
    /// instance sequence, then two zero bytes: every DeleteObject in the retail captures is 12
    /// bytes with its opcode.
    #[test]
    fn delete_object_is_six_bytes_and_two_of_padding() {
        let m = ItemDeleteObject {
            id: ObjectId(0x5000_0002),
            instance_sequence: 3,
        };
        let bytes = write_body(&m).unwrap();
        assert_eq!(bytes, vec![2, 0, 0, 0x50, 3, 0, 0, 0]);
        let _: ItemDeleteObject = round_trip(&bytes);
        let unpadded: ItemDeleteObject = crate::read_body(&bytes[..6]).unwrap();
        assert_eq!(unpadded, m, "a body without the padding still reads");
    }

    /// `PhysicsEventStamp` closes several WorldObjects events, and its trailing align is computed on
    /// the *blob*'s offset, so a message body whose length before the pack is odd pads differently.
    #[test]
    fn the_world_view_events_with_a_timestamp_pack_round_trip() {
        let ts = PhysicsEventStamp {
            instance: 7,
            event: 9,
        };
        let _: InventoryPickupEvent = round_trip(
            &write_body(&InventoryPickupEvent {
                id: ObjectId(1),
                timestamps: ts,
            })
            .unwrap(),
        );
        let _: ItemSetState = round_trip(
            &write_body(&ItemSetState {
                id: ObjectId(1),
                state: 0x400,
                timestamps: ts,
            })
            .unwrap(),
        );
        let _: ItemParentEvent = round_trip(
            &write_body(&ItemParentEvent {
                creature: ObjectId(1),
                item: ObjectId(2),
                location: 3,
                placement_frame: 4,
                timestamps: ts,
            })
            .unwrap(),
        );
        let _: ItemObjDescEvent = round_trip(
            &write_body(&ItemObjDescEvent {
                id: ObjectId(1),
                objdesc: ObjDesc::default(),
                timestamps: ts,
            })
            .unwrap(),
        );
    }

    /// Oracle: `docs/networking/messages/02-world-objects.md` §4 — the `0xF751` handler reads only
    /// a `ushort`; the retail server padded the body to four bytes (every captured instance is 8
    /// with its opcode).
    #[test]
    fn player_teleport_is_two_bytes_and_two_of_padding() {
        let bytes = write_body(&EffectsPlayerTeleport {
            teleport_sequence: 5,
        })
        .unwrap();
        assert_eq!(bytes, vec![5, 0, 0, 0]);
        let _: EffectsPlayerTeleport = round_trip(&bytes);
    }

    /// Oracle: `docs/networking/messages/02-world-objects.md` §8 — seven `0x00A0` codes are
    /// suppressed because the object-specific text has already been printed.
    #[test]
    fn the_seven_suppressed_failure_codes_are_exactly_those() {
        for c in [0x01Eu32, 0x02B, 0x3EF, 0x43E, 0x46A, 0x4CE, 0x4CF] {
            assert!(
                CharacterServerSaysAttemptFailed::suppresses_generic_text(c),
                "0x{c:03X}"
            );
        }
        assert!(!CharacterServerSaysAttemptFailed::suppresses_generic_text(
            0x020
        ));
    }

    #[test]
    fn the_ui_queue_object_messages_round_trip() {
        let _: ItemServerSaysContainId = round_trip(
            &write_body(&ItemServerSaysContainId {
                item: ObjectId(1),
                container: ObjectId(2),
                slot: 3,
                container_properties: 0,
            })
            .unwrap(),
        );
        let _: ItemWearItem = round_trip(
            &write_body(&ItemWearItem {
                item: ObjectId(1),
                slot: 0x0010_0000,
            })
            .unwrap(),
        );
        let _: ItemOnViewContents = round_trip(
            &write_body(&ItemOnViewContents {
                container: ObjectId(1),
                contents: vec![ContentProfile {
                    iid: ObjectId(2),
                    container_properties: 0,
                }],
            })
            .unwrap(),
        );
        let _: ItemUseDone = round_trip(&write_body(&ItemUseDone { failure_type: 0 }).unwrap());
        let _: ItemAppraiseDone =
            round_trip(&write_body(&ItemAppraiseDone { unknown: 0 }).unwrap());
        let _: ObjectSendForceObjdesc =
            round_trip(&write_body(&ObjectSendForceObjdesc { id: ObjectId(9) }).unwrap());
        let _: EffectsSoundEvent = round_trip(
            &write_body(&EffectsSoundEvent {
                id: ObjectId(1),
                sound_type: 2,
                volume: 0.5,
            })
            .unwrap(),
        );
        let _: EffectsPlayScriptId = round_trip(
            &write_body(&EffectsPlayScriptId {
                id: ObjectId(1),
                script_id: 2,
            })
            .unwrap(),
        );
        let _: EffectsPlayScriptType = round_trip(
            &write_body(&EffectsPlayScriptType {
                id: ObjectId(1),
                script_type: 2,
                intensity: 1.0,
            })
            .unwrap(),
        );
        let _: LoginCreatePlayer = round_trip(
            &write_body(&LoginCreatePlayer {
                player_id: ObjectId(0x5000_0001),
            })
            .unwrap(),
        );
    }
}
