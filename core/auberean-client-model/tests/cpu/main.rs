//! Tests that need no private data: the event queue's holding and ordering rules, and the model the
//! messages build.

mod characters;
mod events;
mod model;

use auberean_client_model::events::Item;
use auberean_protocol::login::PlayerObject;
use auberean_protocol::physics::{CreateObject, Fx, ObjectEvent};
use auberean_protocol::quality::PrivateQuality;
use auberean_protocol::types::{ObjectStamp, PhysicsDesc, WeenieDesc};
use auberean_protocol::ServerMessage;

pub const PLAYER: u64 = 0x2130_0000_0000_0001;

/// A blob id: ordering type, stamp.
pub fn blob(kind: u8, stamp: u16) -> u64 {
    u64::from(kind) << 56 | u64::from(stamp) << 32
}

pub fn item(kind: u8, stamp: u16, message: ServerMessage) -> Item {
    Item {
        blob_id: blob(kind, stamp),
        message,
    }
}

pub fn player_object() -> Item {
    item(
        3,
        1,
        ServerMessage::PlayerObject(PlayerObject {
            id: PLAYER,
            region: 1,
        }),
    )
}

pub fn create(id: u64, instance: u16, visual: u16) -> CreateObject {
    CreateObject {
        id,
        physics: PhysicsDesc {
            instance_stamp: instance,
            visual_ordering_stamp: visual,
            ..PhysicsDesc::default()
        },
        ..CreateObject::default()
    }
}

pub fn create_item(id: u64, instance: u16, visual: u16) -> Item {
    item(
        3,
        0,
        ServerMessage::CreateObject(create(id, instance, visual)),
    )
}

/// A visual effect on an object, stamped.
pub fn fx(id: u64, instance: u16, visual: u16, effect: u32) -> Item {
    item(
        2,
        0,
        ServerMessage::DoFx(ObjectEvent {
            sender: ObjectStamp {
                id,
                instance_stamp: instance,
                other_stamp: visual,
            },
            action: Fx {
                fx: effect,
                scalar: 1.0,
            },
        }),
    )
}

/// A private quality update, stamped.
pub fn private_int(stamp: u16, quality: u32) -> Item {
    item(
        1,
        stamp,
        ServerMessage::UpdateInt(PrivateQuality { quality, value: 7 }),
    )
}

/// An object created inside a container.
pub fn create_in(id: u64, container: u64) -> Item {
    let mut c = create(id, 1, 1);
    c.weenie = WeenieDesc {
        mask: WeenieDesc::OBJECT_BITS[0],
        ..WeenieDesc::default()
    };
    c.weenie.objects[0] = container;
    item(3, 0, ServerMessage::CreateObject(c))
}
