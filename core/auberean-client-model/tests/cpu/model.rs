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

/// What an object is told of where it is counts only when stamped later than the newest position
/// stamp heard: leaving the world or being held, stamped earlier than the place it was last given,
/// changes nothing; stamped later, it does. The player itself never leaves.
///
/// Behaviour: model.state.a-stale-leaving-or-holding-changes-nothing
#[test]
fn a_stale_leaving_or_holding_changes_nothing() {
    use auberean_protocol::physics::{Attach, Leave, ObjectEvent};
    use auberean_protocol::types::ObjectStamp;
    let mut m = Model::default();
    let thing = 0x8130_0000_0000_0011;
    let holder = 0x2130_0000_0000_0007;
    let stamp = |id, other_stamp| ObjectStamp {
        id,
        instance_stamp: 1,
        other_stamp,
    };
    m.apply(&create_item(holder, 1, 1));
    m.apply(&create_item(thing, 1, 1));
    let leave = |s| {
        item(
            2,
            0,
            ServerMessage::ObjectLeaves(ObjectEvent {
                sender: stamp(thing, 0),
                action: Leave { stamp: s },
            }),
        )
    };
    let hold = |s| {
        item(
            2,
            0,
            ServerMessage::Parent(ObjectEvent {
                sender: stamp(thing, 0),
                action: Attach {
                    parent: stamp(holder, s),
                    location: 1,
                    orientation: 0,
                },
            }),
        )
    };
    m.apply(&leave(5));
    assert!(!m.object(thing).expect("kept").in_world, "stamped later");
    m.apply(&hold(3));
    assert_eq!(m.object(thing).and_then(|o| o.parent), None, "an older one");
    m.apply(&hold(5));
    assert_eq!(m.object(thing).and_then(|o| o.parent), None, "the same one");
    m.apply(&hold(6));
    assert_eq!(m.object(thing).and_then(|o| o.parent), Some((holder, 1)));
    m.apply(&leave(4));
    assert!(m.object(thing).and_then(|o| o.parent).is_some(), "older");
    m.apply(&leave(0x8008));
    assert!(
        m.object(thing).and_then(|o| o.parent).is_some(),
        "half the circle on is behind"
    );
    m.apply(&leave(7));
    assert!(m.object(thing).and_then(|o| o.parent).is_none(), "later");
}

/// An object made already held takes the position stamp its description gives, as one in the world
/// does: a let-go older than that changes nothing, and a later one lets it go.
///
/// Behaviour: model.state.an-object-made-held-takes-its-position-stamp
#[test]
fn an_object_made_held_takes_its_position_stamp() {
    use auberean_protocol::physics::ObjectEvent;
    use auberean_protocol::types::ObjectStamp;
    let mut m = Model::default();
    let thing = 0x8130_0000_0000_0011;
    let holder = 0x2130_0000_0000_0007;
    m.apply(&create_item(holder, 1, 1));
    let mut made = create_item(thing, 1, 1);
    let ServerMessage::CreateObject(c) = &mut made.message else {
        panic!("a creation");
    };
    c.physics.parent.parent = holder;
    c.physics.stamps[0] = 5;
    m.apply(&made);
    let let_go = |other_stamp| {
        item(
            2,
            0,
            ServerMessage::DeParent(ObjectEvent {
                sender: ObjectStamp {
                    id: holder,
                    instance_stamp: 1,
                    other_stamp: 0,
                },
                action: ObjectStamp {
                    id: thing,
                    instance_stamp: 1,
                    other_stamp,
                },
            }),
        )
    };
    assert_eq!(m.object(thing).map(|o| o.position_stamp), Some(5));
    m.apply(&let_go(4));
    assert!(m.object(thing).and_then(|o| o.parent).is_some(), "older");
    m.apply(&let_go(6));
    assert!(m.object(thing).and_then(|o| o.parent).is_none(), "later");
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
                parent: ObjectStamp {
                    other_stamp: 1,
                    ..stamp(holder, 1)
                },
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
                action: ObjectStamp {
                    other_stamp: 2,
                    ..stamp(sword, instance)
                },
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

/// The speed and jump scales the server sends for an object are kept with its physics
/// description, as though its creation had carried them, so a body made for it afterwards starts
/// with them; one for an object not made is dropped.
///
/// Behaviour: model.state.an-objects-speed-and-jump-scales-are-kept-with-it
#[test]
fn an_objects_speed_and_jump_scales_are_kept_with_its_description() {
    use auberean_protocol::physics::{ObjectEvent, Scale};
    use auberean_protocol::types::{ObjectStamp, PhysicsDesc};
    let mut m = Model::new();
    let beetle = 0x2130_0000_0000_0007;
    m.apply(&create_item(beetle, 1, 1));
    let event = |id, f: fn(ObjectEvent<Scale>) -> ServerMessage, v| {
        item(
            2,
            0,
            f(ObjectEvent {
                sender: ObjectStamp {
                    id,
                    instance_stamp: 1,
                    other_stamp: 0,
                },
                action: Scale(v),
            }),
        )
    };
    m.apply(&event(beetle, ServerMessage::VelocityScale, 0.5));
    m.apply(&event(beetle, ServerMessage::JumpScale, 1.25));
    m.apply(&event(
        0x2130_0000_0000_0008,
        ServerMessage::VelocityScale,
        2.0,
    ));
    let p = &m.object(beetle).expect("the beetle").physics;
    assert_eq!((p.velocity_scale, p.jump_scale), (0.5, 1.25), "kept: {p:?}");
    assert_eq!(
        p.bits & (PhysicsDesc::VELOCITY_SCALE | PhysicsDesc::JUMP_SCALE),
        PhysicsDesc::VELOCITY_SCALE | PhysicsDesc::JUMP_SCALE
    );
    assert!(m.object(0x2130_0000_0000_0008).is_none());
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

/// A boot ends the login as a refusal for the character booted (reason 20), counted with the
/// others and kept with the server's words; a refusal after it carries none.
///
/// Behaviour: model.state.a-boot-is-kept-as-a-refusal-with-its-words
#[test]
fn a_boot_is_kept_as_a_refusal_with_its_words() {
    use auberean_protocol::login::{CharacterRefused, Text};
    use auberean_protocol::text::StringInfo;
    let mut m = Model::new();
    let line = StringInfo {
        string_id: 0xDA7D_EF7F,
        table_id: 0x2500_0012,
        ..StringInfo::default()
    };
    assert_eq!(
        m.apply(&item(
            3,
            0,
            ServerMessage::AccountBooted(Text(line.clone()))
        )),
        vec![Effect::Refused(CharacterRefused::BOOTED)]
    );
    assert_eq!(m.refused, Some((20, 1)));
    assert_eq!(m.refused_with, Some(line), "the server's words");
    m.apply(&item(
        3,
        1,
        ServerMessage::CharacterRefused(CharacterRefused { reason: 11 }),
    ));
    assert_eq!(m.refused, Some((11, 2)));
    assert_eq!(m.refused_with, None, "an entry refused has none");
}

/// An object is placed where its creation puts it only when that is in a cell: a creation that
/// says it has a place in no cell leaves it unplaced. It keeps a copy of the creation's weenie
/// description, which holds its package only when it has one that is not all ones.
///
/// Behaviour: model.state.an-object-made-in-no-cell-is-not-placed
#[test]
fn an_object_made_in_no_cell_is_not_placed_and_keeps_a_copy_of_its_description() {
    use auberean_protocol::types::{PhysicsDesc, Placement, WeenieDesc};
    let mut m = Model::new();
    let nowhere = 0x2130_0000_0000_0007;
    let here = 0x2130_0000_0000_0008;
    let mut c = crate::create(nowhere, 1, 1);
    c.physics.bits |= PhysicsDesc::POSITION;
    c.weenie = WeenieDesc {
        mask: WeenieDesc::PACKAGE,
        package_id: u32::MAX,
        ..WeenieDesc::default()
    };
    m.apply(&item(3, 0, ServerMessage::CreateObject(c.clone())));
    let o = m.object(nowhere).expect("made");
    assert!(o.place.is_none(), "{:?}", o.place);
    assert_eq!(
        o.weenie.mask & WeenieDesc::PACKAGE,
        0,
        "all ones is no package"
    );

    c.id = here;
    c.physics.position = Placement {
        cell: 0x7D64_0021,
        ..Placement::default()
    };
    c.physics.stamps = [4, 5, 6, 7];
    m.apply(&item(3, 0, ServerMessage::CreateObject(c)));
    let p = m.object(here).and_then(|o| o.place).expect("placed");
    assert_eq!(
        (
            p.cell,
            p.position_stamp,
            p.teleport_stamp,
            p.force_position_stamp
        ),
        (0x7D64_0021, 4, 5, 6)
    );
}

/// The player's id is none (0) until the player's creation names it, and none again once the
/// character leaves the world. An object keeps the whole physics description it was made with,
/// and hangs from another when that description, or the server since, says so.
///
/// Behaviour: model.state.the-player-id-and-what-an-object-hangs-from
#[test]
fn the_players_id_is_known_from_its_creation_and_an_object_keeps_what_it_hangs_from() {
    use auberean_protocol::types::{ParentLink, PhysicsDesc, Placement};
    let mut m = Model::new();
    assert_eq!(m.player_id(), 0);
    m.apply(&player_object());
    assert_eq!(m.player_id(), PLAYER);
    let held = 0x8130_0000_0000_0020;
    let mut c = crate::create(held, 1, 1);
    c.physics = PhysicsDesc {
        bits: PhysicsDesc::POSITION,
        instance_stamp: 1,
        visual_ordering_stamp: 1,
        position: Placement {
            cell: 0x7D64_0021,
            ..Placement::default()
        },
        parent: ParentLink {
            parent: PLAYER,
            location: 3,
            stamp: 1,
        },
        velocity_scale: 1.5,
        jump_scale: 0.5,
        stamps: [1, 2, 3, 4],
        ..PhysicsDesc::default()
    };
    m.apply(&item(3, 0, ServerMessage::CreateObject(c.clone())));
    let o = m.object(held).expect("made");
    assert_eq!(o.physics, c.physics, "the description whole");
    assert!(o.has_parent());
    assert_eq!(o.parent, Some((PLAYER, 3)));
    let loose = 0x8130_0000_0000_0021;
    m.apply(&create_item(loose, 1, 1));
    assert!(!m.object(loose).expect("made").has_parent());
    m.apply(&item(3, 0, ServerMessage::CharExitGame(Empty)));
    assert_eq!(m.player_id(), 0);
}
