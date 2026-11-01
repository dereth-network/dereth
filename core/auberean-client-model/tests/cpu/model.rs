//! The model the messages build.

use auberean_client_model::{Effect, Model};
use auberean_protocol::login::{Empty, PlayerQualities};
use auberean_protocol::table::Table;
use auberean_protocol::types::Qualities as Description;
use auberean_protocol::ServerMessage;

use crate::{create_in, create_item, item, player_object, private_int, PLAYER};

/// The player's creation names the player; its description gives its qualities, and private
/// updates change them; objects created inside the player, or inside a container it holds, are its
/// inventory, and out of the world; leaving the world forgets the world's objects.
///
/// Behaviour: model.state.player-qualities-and-inventory-from-the-messages
#[test]
fn the_player_its_qualities_and_its_inventory_come_from_the_messages() {
    let mut m = Model::new();
    assert_eq!(
        m.apply(&player_object()),
        vec![Effect::PlayerCreated(PLAYER)]
    );
    let d = Description {
        did: 0x8100_0530,
        ints: Table::from_inserted(2, [(280u32, 78i32), (290, 1)]),
        ..Description::default()
    };
    m.apply(&item(
        1,
        1,
        ServerMessage::PlayerQualities(PlayerQualities(d)),
    ));
    assert_eq!(
        m.apply(&create_item(PLAYER, 1, 1)),
        vec![Effect::PlayerObject]
    );
    m.apply(&private_int(2, 280));
    let p = m.player_object().expect("the player's object");
    assert_eq!(p.qualities.base, 0x8100_0530);
    assert_eq!(p.qualities.ints.get(&280), Some(&7));
    assert_eq!(p.qualities.ints.get(&290), Some(&1));
    let pack = 0x8130_0000_0000_0010;
    let sword = 0x8130_0000_0000_0011;
    let rock = 0x8130_0000_0000_0012;
    m.apply(&create_in(pack, PLAYER));
    m.apply(&create_in(sword, pack));
    m.apply(&create_item(rock, 1, 1));
    assert_eq!(m.inventory(), vec![pack, sword]);
    assert!(!m.object(sword).expect("the sword").in_world);
    assert_eq!(
        m.surroundings().map(|o| o.id).collect::<Vec<_>>(),
        vec![rock]
    );
    assert_eq!(
        m.apply(&item(3, 0, ServerMessage::CharExitGame(Empty))),
        vec![Effect::LeftWorld]
    );
    assert_eq!(m.objects().count(), 0);
    assert_eq!(m.player, None);
}

/// An instance id's kind comes from its top byte (dynamic, static, player, interface, temporary);
/// a static id carries its landblock; the camera's own ids fall to no known kind, as the client's
/// camera test reads bits they do not set; numbers too wide make no id.
///
/// Behaviour: model.ids.an-ids-kind-comes-from-its-top-byte
#[test]
fn an_ids_kind_comes_from_its_top_byte() {
    use auberean_client_model::ids::{self, IdKind};
    assert_eq!(ids::kind(0x8000_0001_0000_0042), IdKind::Dynamic);
    assert_eq!(ids::kind(crate::PLAYER), IdKind::Player);
    assert_eq!(ids::kind(ids::interface(5)), IdKind::Interface);
    assert_eq!(ids::kind(ids::temporary(5)), IdKind::Temporary);
    assert_eq!(ids::interface(0x1000), ids::INVALID);
    let s = ids::static_object(0x0000_0003_0000_0009, 0xA9B4_0031);
    assert_eq!(s, 0x40A9_B403_0000_0009);
    assert_eq!(ids::kind(s), IdKind::Static);
    assert_eq!(ids::block_of(s), 0xA9B4_FFFF);
    assert!(
        ids::is_static(0x0100_0000_0000_0001),
        "no known kind counts as static"
    );
    assert_eq!(ids::kind(ids::camera_parent()), IdKind::Unknown);
    assert_eq!(ids::kind(0x0048_0000_0000_0001), IdKind::Camera);
    assert!(ids::is_temporary(ids::temporary(1)) && !ids::is_dynamic(ids::temporary(1)));
}

/// The client's own objects take temporary ids from the first up, each given out once until it is
/// taken back, when it is the first given out again.
///
/// Behaviour: model.ids.temporary-ids-are-given-out-from-the-first-free
#[test]
fn temporary_ids_are_given_out_from_the_first_free() {
    use auberean_client_model::ids;
    use auberean_client_model::Model;
    let mut m = Model::new();
    let a = m.allocate_temporary_id().expect("an id");
    let b = m.allocate_temporary_id().expect("an id");
    let c = m.allocate_temporary_id().expect("an id");
    assert_eq!(
        (a, b, c),
        (ids::temporary(0), ids::temporary(1), ids::temporary(2))
    );
    m.deallocate_temporary_id(b);
    assert_eq!(m.allocate_temporary_id(), Some(b), "the first free again");
    assert_eq!(m.allocate_temporary_id(), Some(ids::temporary(3)));
}

/// A held item is let go of by a message its holder sends: the item is the one the message names
/// (in the incarnation it names), not the sender; the holder keeps what holds it.
///
/// Behaviour: model.state.a-let-go-names-the-item-and-its-holder-sends-it
#[test]
fn a_let_go_takes_the_named_item_out_of_its_holders_hand() {
    use auberean_protocol::physics::{Attach, ObjectEvent};
    use auberean_protocol::types::ObjectStamp;
    let mut m = Model::new();
    let holder = 0x2130_0000_0000_0007;
    let sword = 0x8130_0000_0000_0011;
    let stamp = |id, instance_stamp| ObjectStamp {
        id,
        instance_stamp,
        other_stamp: 0,
    };
    m.apply(&create_item(holder, 1, 1));
    m.apply(&create_item(sword, 4, 1));
    m.apply(&item(
        2,
        0,
        ServerMessage::Parent(ObjectEvent {
            sender: stamp(sword, 4),
            action: Attach {
                parent: stamp(holder, 1),
                location: 1,
                orientation: 0,
            },
        }),
    ));
    let let_go = |instance| {
        item(
            2,
            0,
            ServerMessage::DeParent(ObjectEvent {
                sender: stamp(holder, 1),
                action: stamp(sword, instance),
            }),
        )
    };
    m.apply(&let_go(3));
    assert_eq!(
        m.object(sword).and_then(|o| o.parent),
        Some((holder, 1)),
        "an older incarnation of the item is not let go of"
    );
    assert_eq!(m.children(holder), vec![sword]);
    m.apply(&let_go(4));
    assert_eq!(m.object(sword).and_then(|o| o.parent), None);
    assert!(m.children(holder).is_empty());
}

/// The player's description message copies its weenie description over the one the player was
/// made with (whichever comes first); the player keeps the package and entity it was made with.
///
/// Behaviour: model.state.the-players-description-replaces-its-weenie-description
#[test]
fn the_players_description_replaces_the_weenie_description_it_was_made_with() {
    use auberean_protocol::types::WeenieDesc;
    let made = WeenieDesc {
        mask: WeenieDesc::PACKAGE | WeenieDesc::ENTITY | WeenieDesc::ICON,
        package_id: 0x0B,
        entity_did: 0x4700_0001,
        icon: 1,
        ..WeenieDesc::default()
    };
    let described = WeenieDesc {
        mask: WeenieDesc::ICON | WeenieDesc::SCALE,
        icon: 2,
        scale: 1.5,
        ..WeenieDesc::default()
    };
    let description = Description {
        header: Description::WEENIE,
        weenie: described.clone(),
        ..Description::default()
    };
    for description_first in [false, true] {
        let mut m = Model::new();
        m.apply(&player_object());
        let mut create = crate::create(PLAYER, 1, 1);
        create.weenie = made.clone();
        let create = item(3, 0, ServerMessage::CreateObject(create));
        let desc = item(
            1,
            1,
            ServerMessage::PlayerQualities(PlayerQualities(description.clone())),
        );
        if description_first {
            m.apply(&desc);
            m.apply(&create);
        } else {
            m.apply(&create);
            m.apply(&desc);
        }
        let w = &m.player_object().expect("the player").weenie;
        assert_eq!((w.icon, w.scale), (2, 1.5), "first: {description_first}");
        assert_eq!((w.package_id, w.entity_did), (0x0B, 0x4700_0001));
    }
}

/// A refused entry is kept with its reason and counted, so the screens see each one, even the same
/// reason twice; the player and the world's objects are left as they were.
///
/// Behaviour: model.state.a-refused-entry-is-kept-with-its-reason
#[test]
fn a_refused_entry_is_kept_with_its_reason_and_counted() {
    use auberean_protocol::login::CharacterRefused;
    let mut m = Model::new();
    assert_eq!(m.refused, None);
    let refused = |reason| {
        item(
            3,
            0,
            ServerMessage::CharacterRefused(CharacterRefused { reason }),
        )
    };
    assert_eq!(
        m.apply(&refused(CharacterRefused::IN_WORLD)),
        vec![Effect::Refused(11)]
    );
    assert_eq!(m.refused, Some((11, 1)));
    assert_eq!(m.apply(&refused(11)), vec![Effect::Refused(11)]);
    assert_eq!(
        m.refused,
        Some((11, 2)),
        "the same reason again is a new one"
    );
    m.apply(&refused(4));
    assert_eq!(m.refused, Some((4, 3)));
    assert_eq!(m.player, None);
    assert_eq!(m.objects().count(), 0);
}
