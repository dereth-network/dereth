use super::*;

const HOLDER: ObjectId = ObjectId(0x7000_0030);
const HELD: ObjectId = ObjectId(0x7000_0031);
const HELD_TOO: ObjectId = ObjectId(0x7000_0032);
const ON_THE_GROUND: ObjectId = ObjectId(0x7000_0033);
/// The part of a creature an item hangs off, and the one thirty-four of the corpus's parent
/// events name: the right hand.
const RIGHT_HAND: u32 = 1;
/// A landblock cell an object placed in the world sits in.
const CELL: u32 = 0x8602_01AD;

fn described(
    id: ObjectId,
    placed: bool,
    parent: Option<(ObjectId, u32)>,
    children: Option<Vec<(ObjectId, u32)>>,
    instance: u16,
) -> dereth_protocol::objects::ItemCreateObject {
    use dereth_protocol::types::physicsdesc::{flags, ChildLink};

    let mut physicsdesc = dereth_protocol::types::PhysicsDesc {
        bitfield: flags::SETUP,
        setup_id: Some(0x0200_0001),
        timestamps: dereth_protocol::types::PhysicsTimestamps {
            instance,
            ..dereth_protocol::types::PhysicsTimestamps::default()
        },
        ..dereth_protocol::types::PhysicsDesc::default()
    };
    if placed {
        physicsdesc.bitfield |= flags::POSITION;
        physicsdesc.position = Some(dereth_protocol::types::PositionWire {
            objcell_id: CELL,
            frame: dereth_protocol::types::Frame::default(),
        });
    }
    if let Some((who, where_on_him)) = parent {
        physicsdesc.bitfield |= flags::PARENT;
        physicsdesc.parent = Some((who, where_on_him));
    }
    if let Some(links) = children {
        physicsdesc.bitfield |= flags::CHILDREN;
        physicsdesc.children = Some(
            links
                .into_iter()
                .map(|(child_id, location_id)| ChildLink {
                    child_id,
                    location_id,
                })
                .collect(),
        );
    }
    dereth_protocol::objects::ItemCreateObject(dereth_protocol::objects::ObjectCreatePayload {
        id,
        objdesc: dereth_protocol::types::ObjDesc::default(),
        physicsdesc,
        wdesc: PublicWeenieDesc::default(),
    })
}

/// The shard describing a holder afresh, which is the arm that re-reads its list of held items.
fn described_afresh(
    id: ObjectId,
    instance: u16,
    children: Option<Vec<(ObjectId, u32)>>,
) -> Inbound {
    let body = dereth_protocol::write_body(&described(id, true, None, children, instance))
        .expect("the description encodes");
    Inbound::event(
        dereth_client_net::client_session::SessionEvent::WorldObject {
            opcode: dereth_protocol::Opcode::ITEM_UPDATE_OBJECT,
            body,
        },
    )
}

fn stow(id: ObjectId, at: u16) -> dereth_protocol::objects::InventoryPickupEvent {
    dereth_protocol::objects::InventoryPickupEvent {
        id,
        timestamps: dereth_protocol::types::PhysicsEventStamp {
            instance: 0,
            event: at,
        },
    }
}

/// An item the shard says has been picked up leaves the hand it was in and the ground it was on.
pub(super) fn a_stowed_item_leaves_the_hand_and_the_world() {
    let mut c = a_client();
    c.when(Inbound::world_view(&described(HOLDER, true, None, None, 0)))
        .when(Inbound::world_view(&described(
            HELD,
            false,
            Some((HOLDER, RIGHT_HAND)),
            None,
            0,
        )));
    let in_the_hand = c
        .view()
        .objects()
        .presence(HELD)
        .is_some_and(|p| p.parent == Some((HOLDER, RIGHT_HAND)));
    let unhandled_before = c.view().objects().stats.unhandled;

    c.when(Inbound::world_view(&stow(HELD, 3)));
    let out_of_the_hand = {
        let o = c.view().objects();
        let item = o
            .presence(HELD)
            .expect("the object is not destroyed by being picked up");
        o.stats.unhandled == unhandled_before
            && o.stats.pickup_events == 1
            && item.parent.is_none()
            && item.position.is_none()
            && o.presence(HOLDER).is_some_and(|h| h.position.is_some())
    };

    // …and one lying on the ground leaves its part of the map, which is the half an item that
    // was already in a hand cannot show.
    let mut ground = a_client();
    ground.when(Inbound::world_view(&described(
        ON_THE_GROUND,
        true,
        None,
        None,
        0,
    )));
    let was_placed = ground
        .view()
        .objects()
        .presence(ON_THE_GROUND)
        .is_some_and(|p| p.position.is_some() && p.parent.is_none());
    ground.when(Inbound::world_view(&stow(ON_THE_GROUND, 3)));
    let left_the_map = ground
        .view()
        .objects()
        .presence(ON_THE_GROUND)
        .is_some_and(|p| p.position.is_none() && p.parent.is_none())
        && ground.view().objects().stats.pickup_events == 1;

    c.assert_behaviour(
        "inventory.pickup.an-item-the-shard-says-was-stowed-leaves-the-hand-and-the-world",
        move |_| in_the_hand && out_of_the_hand && was_placed && left_the_map,
    );
}

/// A stow that is not newer than the item's own last move is refused, and one for an object the
/// client does not have is counted rather than dropped.
pub(super) fn a_stale_or_unknown_stow_changes_nothing() {
    let mut c = a_client();
    c.when(Inbound::world_view(&described(HOLDER, true, None, None, 0)))
        .when(Inbound::world_view(&described(HELD, false, None, None, 0)))
        .when(Inbound::world_view(
            &dereth_protocol::objects::ItemParentEvent {
                creature: HOLDER,
                item: HELD,
                location: RIGHT_HAND,
                placement_frame: 1,
                timestamps: dereth_protocol::types::PhysicsEventStamp {
                    instance: 0,
                    event: 7,
                },
            },
        ));

    // Equal, then older: neither is newer, so neither wins.
    c.when(Inbound::world_view(&stow(HELD, 7)))
        .when(Inbound::world_view(&stow(HELD, 4)));
    let refused = {
        let o = c.view().objects();
        o.stats.pickup_events == 0
            && o.stats.pickup_events_stale == 2
            && o.presence(HELD)
                .is_some_and(|p| p.parent == Some((HOLDER, RIGHT_HAND)))
    };

    c.when(Inbound::world_view(&stow(HELD, 8)));
    let newer_wins = c.view().objects().stats.pickup_events == 1
        && c.view()
            .objects()
            .presence(HELD)
            .is_some_and(|p| p.parent.is_none());

    // An object the client has never had is counted, not silently dropped.
    let mut stranger = a_client();
    stranger.when(Inbound::world_view(&stow(HELD, 3)));
    let counted = stranger.view().objects().stats.pickup_events_unknown == 1
        && stranger.view().objects().stats.pickup_events == 0;

    c.assert_behaviour(
        "inventory.pickup.a-stale-or-unknown-stow-changes-nothing",
        move |_| refused && newer_wins && counted,
    );
}

/// The holder's own list is what decides what hangs off it.
pub(super) fn a_holders_own_list_decides_what_it_holds() {
    // An item already known, attached solely because the holder's list names it -- which is the
    // order nothing else can do, because the item's own description says nothing about a holder.
    let mut c = a_client();
    c.when(Inbound::world_view(&described(HELD, true, None, None, 0)));
    let loose = c
        .view()
        .objects()
        .presence(HELD)
        .is_some_and(|p| p.parent.is_none());
    c.when(Inbound::world_view(&described(
        HOLDER,
        true,
        None,
        Some(vec![(HELD, RIGHT_HAND)]),
        0,
    )));
    let attached = {
        let o = c.view().objects();
        o.stats.children_attached == 1
            && o.stats.children_unknown == 0
            && o.presence(HELD)
                .is_some_and(|p| p.parent == Some((HOLDER, RIGHT_HAND)) && p.position.is_none())
    };

    // A list naming an object the client has never seen is counted rather than invented.
    let mut unknown = a_client();
    unknown.when(Inbound::world_view(&described(
        HOLDER,
        true,
        None,
        Some(vec![(HELD, RIGHT_HAND)]),
        0,
    )));
    let counted = unknown.view().objects().stats.children_unknown == 1
        && unknown.view().objects().stats.children_attached == 0;

    // …and when a holder is described afresh, what its new list leaves out is no longer held.
    let mut afresh = a_client();
    afresh
        .when(Inbound::world_view(&described(HOLDER, true, None, None, 0)))
        .when(Inbound::world_view(&described(
            HELD,
            false,
            Some((HOLDER, RIGHT_HAND)),
            None,
            0,
        )))
        .when(Inbound::world_view(&described(
            HELD_TOO,
            false,
            Some((HOLDER, 2)),
            None,
            0,
        )));
    let both_held = afresh
        .view()
        .objects()
        .presence(HELD)
        .is_some_and(|p| p.parent.is_some())
        && afresh
            .view()
            .objects()
            .presence(HELD_TOO)
            .is_some_and(|p| p.parent.is_some());
    let _ = afresh.objects_mut().take_removed();
    afresh.when(described_afresh(HOLDER, 1, Some(vec![(HELD, RIGHT_HAND)])));
    let retired = afresh.objects_mut().take_removed() == vec![HOLDER];
    let only_the_named_one = {
        let o = afresh.view().objects();
        o.presence(HELD)
            .is_some_and(|p| p.parent == Some((HOLDER, RIGHT_HAND)))
            && o.presence(HELD_TOO).is_some_and(|p| p.parent.is_none())
            && o.stats.recreates == 1
    };

    c.assert_behaviour(
        "inventory.held-item.a-holders-own-list-decides-what-hangs-off-it",
        move |_| loose && attached && counted && both_held && retired && only_the_named_one,
    );
}
