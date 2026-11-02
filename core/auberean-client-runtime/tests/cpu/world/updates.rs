//! The server's updates of an object's qualities, as the client takes them.

use auberean_client_model::{Item, Model};
use auberean_client_runtime::description;
use auberean_protocol::physics::CreateObject;
use auberean_protocol::quality::VisualQuality;
use auberean_protocol::types::ObjectStamp;
use auberean_protocol::ServerMessage;

const ITEM: u64 = 0x8130_0000_1000_0002;
const BAG: u64 = 0x8130_0000_1000_0001;
const PLAYER: u64 = 0x2130_0000_0000_0001;
/// The container (an object quality the description holds), and one it does not hold.
const CONTAINER: u32 = 0x102;
const OTHER: u32 = 0x103;

fn update(q: u32, v: u64) -> ServerMessage {
    ServerMessage::UpdateInstanceIdVisual(VisualQuality {
        object: ObjectStamp {
            id: ITEM,
            ..ObjectStamp::default()
        },
        quality: q,
        value: v,
    })
}

/// An update of a quality the object's description holds (an item's container) goes to the
/// description, which answers for it first; the object's own qualities are left as they were.
/// An update of one it does not hold is the object's own.
///
/// Behaviour: script.model.the-servers-updates-go-to-the-description-first
#[test]
fn the_servers_updates_go_to_the_description_first() {
    let mut m = Model::new();
    let mut c = CreateObject {
        id: ITEM,
        ..CreateObject::default()
    };
    description::set_instance_id(&mut c.weenie, CONTAINER, BAG);
    m.apply(&Item {
        blob_id: 0,
        message: ServerMessage::CreateObject(c),
    });
    assert!(description::take_update(&mut m, &update(CONTAINER, PLAYER)));
    let o = m.object(ITEM).expect("the item");
    assert_eq!(description::instance_id(&o.weenie, CONTAINER), Some(PLAYER));
    assert!(!o.qualities.instance_ids.contains_key(&CONTAINER));
    assert!(!description::take_update(&mut m, &update(OTHER, PLAYER)));
    // An update for an object the client does not have is not the description's either.
    let mut away = update(CONTAINER, PLAYER);
    if let ServerMessage::UpdateInstanceIdVisual(u) = &mut away {
        u.object.id = BAG;
    }
    assert!(!description::take_update(&mut m, &away));
}

/// A rename (the server's update of the name quality) takes the place of the name the object was
/// made with: the name is the description's, which answers for it first, so the new name is the
/// one read; the object's own qualities are left as they were.
///
/// Behaviour: script.model.a-rename-replaces-the-name-the-object-was-made-with
#[test]
fn a_rename_replaces_the_name_the_object_was_made_with() {
    use auberean_protocol::text::StringInfo;
    /// The name quality.
    const NAME: u32 = 0x100;
    let mut m = Model::new();
    let mut c = CreateObject {
        id: ITEM,
        ..CreateObject::default()
    };
    description::set_string_info(&mut c.weenie, NAME, StringInfo::literal("Drudge"));
    m.apply(&Item {
        blob_id: 0,
        message: ServerMessage::CreateObject(c),
    });
    let rename = ServerMessage::UpdateStringInfoVisual(VisualQuality {
        object: ObjectStamp {
            id: ITEM,
            ..ObjectStamp::default()
        },
        quality: NAME,
        value: StringInfo::literal("Corpse of Drudge"),
    });
    assert!(description::take_update(&mut m, &rename));
    let o = m.object(ITEM).expect("the item");
    assert_eq!(
        description::string_info(&o.weenie, NAME),
        Some(&StringInfo::literal("Corpse of Drudge"))
    );
    assert!(o.qualities.string_infos.is_empty());
}

/// An item of the player's pack is held and out of the world; when the server clears its
/// container it is no longer held, and once a position places it, it stands in the world among the
/// player's surroundings.
///
/// Behaviour: script.model.an-item-put-down-leaves-the-pack-for-the-world
#[test]
fn an_item_put_down_leaves_the_pack_for_the_world() {
    use auberean_protocol::login::PlayerObject;
    use auberean_protocol::physics::ObjectPosition;
    let mut m = Model::new();
    let item = |message| Item {
        blob_id: 0,
        message,
    };
    m.apply(&item(ServerMessage::PlayerObject(PlayerObject {
        id: PLAYER,
        region: 1,
    })));
    m.apply(&item(ServerMessage::CreateObject(CreateObject {
        id: PLAYER,
        ..CreateObject::default()
    })));
    let mut c = CreateObject {
        id: ITEM,
        ..CreateObject::default()
    };
    description::set_instance_id(&mut c.weenie, CONTAINER, PLAYER);
    m.apply(&item(ServerMessage::CreateObject(c)));
    assert_eq!(m.inventory(), vec![ITEM], "held");
    assert!(description::take_update(&mut m, &update(CONTAINER, 0)));
    assert_eq!(m.inventory(), Vec::<u64>::new(), "no longer held");
    m.apply(&item(ServerMessage::Position(ObjectPosition {
        sender: ObjectStamp {
            id: ITEM,
            ..ObjectStamp::default()
        },
        time: 1.0,
        cell: 0x0298_0109,
        origin: auberean_primitives::Vec3::new(59.0, 240.785, -44.49),
        heading_motion: u32::from_be_bytes([0, 127, 127, 127]),
        flags: 4,
        position_stamp: 1,
        ..ObjectPosition::default()
    })));
    let o = m.object(ITEM).expect("the item");
    assert!(o.in_world && o.container.is_none() && o.place.is_some());
    assert_eq!(
        m.surroundings().map(|o| o.id).collect::<Vec<_>>(),
        vec![ITEM],
        "in the world"
    );
}
