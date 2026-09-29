//! Shared virtual-time server fixture and message helpers.

#![allow(unused_imports)]

pub(crate) use dereth_primitives::ObjectId;
pub(crate) use dereth_protocol::comms::{CommunicationTalk, CommunicationTransientString};
pub(crate) use dereth_protocol::items::{
    InventoryGiveObjectRequest, InventoryPutItemInContainer, InventoryUseEvent, ItemQueryItemMana,
    ItemQueryItemManaResponse,
};
pub(crate) use dereth_protocol::objects::ItemDeleteObject;
pub(crate) use empyrean_entity::enums::PropertyInt;
pub(crate) use empyrean_entity::ObjectGuid;

pub(crate) use crate::support::object_use_world::{
    all, at, chats, during, in_pack, join, kinds, obj_mut, on_ground, server, use_dones, ALPHA,
    BOOK, LIFESTONE, WAND,
};

pub(crate) const QUERY_ITEM_MANA_RESPONSE: u32 = 0x0264;
pub(crate) const OBJECT_DELETE: u32 = 0xF747;
pub(crate) const WEENIE_ERROR: u32 = 0x028A;
pub(crate) const TRANSIENT: u32 = 0x02EB;

/// The same repeated Use by a PK character. `Player.FastTick` is `IsPKType` (Player_Tick.cs), so a
/// PK player's Use takes `CreateMoveToChain2` (Player_Move2.cs): the turn runs on the server's
/// physics (`PhysicsObj.TurnToObject` through the MoveToManager) and the Use waits for its
/// completion. The synthetic character has no motion table, so this runs on real content.
#[cfg(feature = "real-content")]
pub(crate) mod real {
    pub(crate) use dereth_primitives::ObjectId;
    pub(crate) use dereth_protocol::comms::CommunicationTextboxString;
    pub(crate) use dereth_protocol::items::InventoryUseEvent;
    pub(crate) use dereth_protocol::login::CharacterLoginCompleteNotification;
    pub(crate) use dereth_protocol::objects::ItemUseDone;
    pub(crate) use empyrean_entity::enums::{PlayerKillerStatus, PropertyInt};

    pub(crate) use dereth_protocol::objects::EffectsPlayerTeleport;
    pub(crate) use empyrean_testkit::decode;

    pub(crate) use crate::support::real_content_bot::real::{create_and_enter, Loop};

    /// Holtburg's life stone.
    pub(crate) const LIFE_STONE: u32 = 509;

    /// An admin teleport, then out of portal space as the client leaves it (LoginComplete a
    /// second after the teleport effect).
    pub(crate) fn teleport(l: &mut Loop, command: &str) {
        let mark = l.mark();
        l.admin_command(command);
        let id = l.id;
        assert!(
            l.ts.run_until(10.0, |ts| !decode::all_of::<EffectsPlayerTeleport>(
                &ts.received_raw(id)[mark..]
            )
            .is_empty()),
            "teleported"
        );
        l.advance(1.0);
        l.action(&CharacterLoginCompleteNotification);
        l.advance(3.0);
    }
}
