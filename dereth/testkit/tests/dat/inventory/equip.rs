use std::collections::BTreeMap;

use super::{
    carry_over, clear_requests, doll_tile, gameplay_root, gameplay_screen, open_pack, pack_slot,
    point_of, shipped, strip_lines, Grab, LetGo, Over, DOLL_SHIELD_SLOT, DOLL_WEAPON_SLOT,
};
use dereth_client::app::App;
use dereth_client_model::inventory::equip::{WieldPlan, WIELD_SLOT_ORDER};
use dereth_client_model::inventory::requests::RequestLock;
use dereth_client_model::inventory::SplitState;
use dereth_client_model::Request;
use dereth_client_net::client_session::testing::{Corpus, Direction};
use dereth_primitives::ObjectId;
use dereth_testkit::adapters_inventory::{recorded_equips, EQUIP_SESSIONS};
use dereth_testkit::{ClientSpec, Given, HeadlessClient, Inbound, Player, ScreenPoint, Target};
use dereth_ui::{ElemHandle, ElementId, StateId};
use dereth_ui_screens::view::{DropTarget, UiRequest};
use {
    dereth_rules::slots::loc, dereth_rules::slots::SlotSide, dereth_rules::slots::PAPERDOLL_REGIONS,
};

// -----------------------------------------------------------------------------------------
// The recordings, read once: which things a recorded client asked to put on, where it asked
// for them to go, and everything the recording said about them.
//
// `dereth_testkit::adapters_inventory::recorded_equips` is the shared reader and answers the
// first two facts -- the thing, and the thing's own list of places, with the asks balanced
// against the shard's answers. The rest of what these scenarios need is the recording's own
// description of the thing, which is read here beside it rather than added to the shared
// reader, because no other group wants it.
// -----------------------------------------------------------------------------------------

/// One equip a recording's own client asked for, and what the recording said about the thing.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct Thing {
    /// The thing the recorded client asked to put on.
    item: ObjectId,
    /// That thing's own list of the places it could go.
    places: u32,
    /// The place the recorded client named in the ask.
    asked_for: u32,
    /// How many are in the stack, which is what decides whether a partial move is possible.
    stack: u16,
    /// How this piece of clothing ranks against the others, which is what a second piece in
    /// the same place clashes on.
    priority: u32,
    /// What the thing is for in a fight, and what kind of thing it is: the facts that decide
    /// what a double-click on it means.
    combat_use: u8,
    obj_type: u32,
    bitfield: u32,
}

/// The whole of one recording's equips.
///
/// # Panics
/// Panics when the recording is absent, when it records no equip, and when it never described
/// something its own client asked to put on -- any of which is a broken checkout rather than
/// a client the scenario could be asserting over.
fn recorded_things(session: &'static str) -> Vec<Thing> {
    /// `Inventory_GetAndWieldItem` -- the ask itself.
    const ASK: u32 = 0x001A;
    /// `Item_CreateObject`, which is where everything a recording says about a thing comes
    /// from.
    const DESCRIBED: u32 = 0xF745;

    let corpus = Corpus::load(session)
        .unwrap_or_else(|e| panic!("the recording {session} does not parse: {e}"))
        .unwrap_or_else(|| panic!("the decoded corpus has no scenario {session}"));
    let mut said: BTreeMap<ObjectId, (u16, u32, u8, u32, u32)> = BTreeMap::new();
    for b in corpus
        .blobs
        .iter()
        .filter(|b| b.dir == Direction::ServerToClient)
    {
        if b.opcode != DESCRIBED {
            continue;
        }
        if let Ok(m) = dereth_protocol::read_body_padded::<dereth_protocol::objects::ItemCreateObject>(
            b.payload.get(4..).unwrap_or_default(),
        ) {
            let w = &m.0.wdesc;
            said.insert(
                m.0.id,
                (
                    w.stack_size.unwrap_or(0),
                    w.priority.unwrap_or(0),
                    w.combat_use.unwrap_or(0),
                    w.obj_type,
                    w.bitfield,
                ),
            );
        }
    }

    // The place each ask named, in the order the asks were made -- which is the order the
    // shared reader answers in, so the two line up one for one.
    let places_asked: Vec<u32> = dereth_testkit::Outbound::all(session)
        .into_iter()
        .filter(|s| s.message == ASK)
        .map(|s| s.field(1).unwrap_or_default())
        .collect();

    let equips = recorded_equips(session);
    assert_eq!(
        equips.len(),
        places_asked.len(),
        "{session}: the shared reader and the ask reader must see the same asks"
    );
    equips
        .into_iter()
        .zip(places_asked)
        .map(|(e, asked_for)| {
            let (stack, priority, combat_use, obj_type, bitfield) = *said
                .get(&e.item)
                .unwrap_or_else(|| panic!("{session}: {:?} was never described", e.item));
            Thing {
                item: e.item,
                places: e.places,
                asked_for,
                stack,
                priority,
                combat_use,
                obj_type,
                bitfield,
            }
        })
        .collect()
}

/// Every equip every recording carries.
fn all_recorded_things() -> Vec<Thing> {
    EQUIP_SESSIONS
        .iter()
        .flat_map(|s| recorded_things(s))
        .collect()
}

// -----------------------------------------------------------------------------------------
// The figure, with no screen under it.
//
// A place on the figure is a table in the client's own code, and letting something go on one is
// a request the window raises; neither opens a shipped file. These scenarios drive that request
// rather than a pointer, because the sweeps below make hundreds of drops and a whole client per
// drop would cost minutes rather than seconds. The scenarios whose subject **is** the pointer
// -- the weapons that cannot share the hands -- stand up a whole client and make a real drag
// instead.
// -----------------------------------------------------------------------------------------

const EQUIP_PLAYER: ObjectId = ObjectId(0x5000_00E1);

/// A player with a pack and nothing on the body.
fn a_player_at_the_figure() -> HeadlessClient {
    let mut c = HeadlessClient::model();
    c.given(Given::APlayer(EQUIP_PLAYER));
    c
}

/// The thing in the player's hand, the way a drag off a pack slot leaves it: known to the
/// client, greyed, and carrying the list of places the shard described for it.
fn carrying(c: &mut HeadlessClient, item: ObjectId, places: u32, stack: u16) {
    let player = c.world_mut().player.expect("a player");
    let mut w = dereth_client_model::Weenie::new(item);
    w.valid = true;
    w.pwd.valid_locations = Some(places);
    w.pwd.stack_size = Some(stack);
    w.pwd.container_id = Some(player);
    w.waiting = true;
    c.world_mut().tables.weenies.insert(item, w);
}

/// The same, carrying everything else the recording said about the thing.
fn carrying_the_recorded(c: &mut HeadlessClient, t: &Thing) {
    carrying(c, t.item, t.places, t.stack);
    let w = c
        .world_mut()
        .tables
        .weenies
        .get_mut(t.item)
        .expect("just seeded");
    w.pwd.priority = Some(t.priority);
    w.pwd.combat_use = Some(t.combat_use);
    w.pwd.obj_type = t.obj_type;
    w.pwd.bitfield = t.bitfield;
}

/// The place on the figure a thing with this list of places would naturally be aimed at: the
/// first place on the figure the thing's own list names.
fn the_place_for(places: u32) -> (u32, u32, SlotSide) {
    PAPERDOLL_REGIONS
        .iter()
        .find(|(_, mask, _)| mask & places != 0)
        .map(|(e, m, s)| (*e, *m, *s))
        .unwrap_or_else(|| panic!("no place on the figure covers {places:#010X}"))
}

/// The place on the figure whose own place is exactly `mask`.
fn the_place_drawn_for(mask: u32) -> u32 {
    PAPERDOLL_REGIONS
        .iter()
        .find(|(_, m, _)| *m == mask)
        .map(|(e, _, _)| *e)
        .unwrap_or_else(|| panic!("the figure draws no place for {mask:#010X}"))
}

/// Let `item` go on the place on the figure drawn at `element`, and answer with everything
/// the client asked the shard for because of it.
fn let_go_on(c: &mut HeadlessClient, item: ObjectId, element: u32) -> Vec<Request> {
    let before = c.outbound().len();
    c.when(Player::Ui(vec![UiRequest::DragDrop {
        item,
        target: crate::inventory::equipment_destination(element),
    }]));
    c.outbound()[before..].to_vec()
}

/// Double-click `item` where it lies, and answer with everything that asked the shard for.
fn double_clicked(c: &mut HeadlessClient, item: ObjectId) -> Vec<Request> {
    let before = c.outbound().len();
    c.when(Player::Ui(vec![UiRequest::Use(item)]));
    c.outbound()[before..].to_vec()
}

/// Put the client back where a fresh gesture starts from.
///
/// The one-thing-at-a-time hold has no timeout and in a live client is let go by the shard's
/// own answer; there is no shard here, and the drops in a sweep are alternatives to one
/// another rather than a sequence, so the hold is let go between them. That it was **taken**
/// is asserted at each drop before this runs, so letting it go here cannot hide a hold that
/// was never taken.
fn between_gestures(c: &mut HeadlessClient) {
    c.world_mut().request_lock = RequestLock::default();
    let stats = &mut c.interaction_mut().stats;
    stats.requests_refused = 0;
    stats.wields_requested = 0;
    stats.wears_requested = 0;
}

/// The thing and the place a wield asked for, when a wield is what was asked for.
fn wield_place(r: &Request) -> Option<(ObjectId, u32)> {
    match r {
        Request::GetAndWieldItem(m) => Some((m.item, m.slot)),
        _ => None,
    }
}

/// The thing and the destination of a move into a container.
fn moved_into(r: &Request) -> Option<(ObjectId, ObjectId)> {
    match r {
        Request::PutItemInContainer(m) => Some((m.item, m.container)),
        _ => None,
    }
}

/// The shard saying a thing is now worn at `place`.
fn the_shard_says_worn(item: ObjectId, place: u32) -> Inbound {
    Inbound::message(&dereth_protocol::objects::ItemWearItem { item, slot: place })
}

/// The shard saying a thing has landed inside `container`.
fn the_shard_says_contained(item: ObjectId, container: ObjectId) -> Inbound {
    Inbound::message(&dereth_protocol::objects::ItemServerSaysContainId {
        item,
        container,
        slot: 0,
        container_properties: 0,
    })
}

/// The shard refusing what was asked about `item`.
fn the_shard_refuses(item: ObjectId) -> Inbound {
    Inbound::message(
        &dereth_protocol::objects::CharacterServerSaysAttemptFailed {
            object: item,
            reason: 0,
        },
    )
}

/// Put `blocker` on the body at `place` the way the shard does, so that what the client
/// believes it is wearing, what it draws on the figure and what it will refuse all agree.
fn already_wearing(
    c: &mut HeadlessClient,
    blocker: ObjectId,
    places: u32,
    place: u32,
    priority: u32,
) {
    let player = c.world_mut().player.expect("a player");
    carrying(c, blocker, places, 1);
    {
        let w = c
            .world_mut()
            .tables
            .weenies
            .get_mut(blocker)
            .expect("just seeded");
        w.pwd.priority = Some(priority);
        // A thing in the way is a different thing from the one being let go on it, and
        // exactly one arm of the walk cares: a stack of the very same kind of ammunition is
        // not something to take off, it is something to add to. Any class of its own is the
        // faithful choice, because the recordings do not carry the blocker's.
        w.pwd.wcid = 0xB10C_0000 | (blocker.0 & 0xFFFF);
        w.pwd.wielder_id = Some(player);
        w.pwd.location = Some(place);
        w.pwd.name = format!("obj{:X}", blocker.0 & 0xFFFF);
        w.waiting = false;
    }
    c.when(the_shard_says_worn(blocker, place));
    assert_eq!(
        c.world_mut().inventory_mask & place,
        place,
        "the premise: the shard's own answer records {place:#010X} as taken"
    );
    assert_eq!(
        c.world_mut().inv_slots.item_at(place),
        Some(blocker),
        "and records which thing is in it"
    );
}

/// How many lines the client has said to the player so far.
fn said_so_far(c: &mut HeadlessClient) -> usize {
    c.world_mut().scroll.pending().len()
}

/// Every line the client has said to the player since `from`, with the channel each went out
/// on.
fn said_since(c: &mut HeadlessClient, from: usize) -> Vec<(u32, String)> {
    c.world_mut()
        .scroll
        .pending()
        .iter()
        .skip(from)
        .map(|f| (f.chat_type, f.body.clone()))
        .collect()
}

// -----------------------------------------------------------------------------------------
// inventory.equip.every-recorded-one-let-go-on-its-own-place-asks-for-what-was-recorded
// -----------------------------------------------------------------------------------------

/// Every thing a recording's own client asked to put on, let go on the place on the figure
/// that thing's own list picks out, asks the shard for a place the recording agrees with.
///
/// Three answers are possible and each is counted, because collapsing them is exactly what a
/// wrong fork looks like from a screenshot: a place among the clothes asks with the thing's
/// whole list, which is what the recording carries; a place outside them asks with **one**
/// place, which for a thing that can go in only one place is the recording's own; and for a
/// thing whose list names several places to wear armour in, the two differ -- the recording's
/// own ask did not come from the figure at all, it came from the sort a double-click makes,
/// which reads the thing's list where the figure reads the place's. Both are right, and the
/// one the figure gives is a part of what the recording carries.
pub fn every_recorded_equip_let_go_on_its_own_place_asks_for_what_was_recorded() {
    let all = all_recorded_things();
    let mut c = a_player_at_the_figure();
    let (mut worn, mut wielded, mut differed) = (0usize, 0usize, 0usize);
    let mut inside_its_own_list = true;
    let mut names_the_place_aimed_at = true;

    for t in &all {
        let (element, mask, _) = the_place_for(t.places);
        between_gestures(&mut c);
        carrying_the_recorded(&mut c, t);
        let sent = let_go_on(&mut c, t.item, element);
        let asked: Vec<(ObjectId, u32)> = sent.iter().filter_map(wield_place).collect();
        assert_eq!(
            (sent.len(), asked.len()),
            (1, 1),
            "{:?}: one drop is one ask to put it on; the client asked for {sent:?}",
            t.item
        );
        let (item, place) = asked[0];
        assert_eq!(item, t.item, "the ask names the thing that was let go");
        inside_its_own_list &= place & !t.places == 0;
        names_the_place_aimed_at &= place & mask != 0;

        if mask & loc::CLOTHING != 0 {
            assert_eq!(
                c.interaction_mut().stats.wears_requested,
                1,
                "{:?} is let go among the clothes, so it is a wear",
                t.item
            );
            assert_eq!(
                place, t.asked_for,
                "{:?}: a wear asks with the whole list and the recording asked with it too",
                t.item
            );
            worn += 1;
        } else {
            assert_eq!(
                c.interaction_mut().stats.wields_requested,
                1,
                "{:?} is let go outside the clothes, so it is a wield",
                t.item
            );
            assert_eq!(
                place.count_ones(),
                1,
                "{:?}: a wield asks for one place out of {:#010X}, and it asked for {place:#010X}",
                t.item,
                t.places
            );
            if t.places.count_ones() == 1 {
                assert_eq!(
                    place, t.asked_for,
                    "{:?}: the recording's own place",
                    t.item
                );
                wielded += 1;
            } else {
                assert_eq!(
                    t.asked_for, t.places,
                    "{:?}: a recording that differs must itself carry the whole list",
                    t.item
                );
                assert_ne!(
                    place, t.asked_for,
                    "{:?}: the two answers must differ",
                    t.item
                );
                assert_eq!(
                    place & !t.asked_for,
                    0,
                    "{:?}: and the figure's answer is a part of what the recording carries",
                    t.item
                );
                differed += 1;
            }
        }
    }

    // The denominators, so that a recording that stopped carrying equips reads as a broken
    // checkout rather than as a clean sweep over nothing. Eleven land among the clothes,
    // fourteen go in exactly the one place they can go in, and seven are armour whose list
    // names several places.
    assert_eq!(
        worn + wielded + differed,
        all.len(),
        "every recorded equip was replayed"
    );
    assert_eq!(
        (worn, wielded, differed),
        (11, 14, 7),
        "the recordings' own shape"
    );

    c.assert_behaviour(
        "inventory.equip.every-recorded-one-let-go-on-its-own-place-asks-for-what-was-recorded",
        move |_| inside_its_own_list && names_the_place_aimed_at && worn > 0 && wielded > 0,
    );
}

// -----------------------------------------------------------------------------------------
// inventory.equip.a-place-the-thing-cannot-go-in-refuses-it-and-a-place-it-can-takes-it
// -----------------------------------------------------------------------------------------

/// Every one of the twenty-four places on the figure, for one thing of each kind the
/// recordings carry: a place the thing's own list does not name asks the shard for nothing at
/// all, counts the refusal, takes the grey off the icon and takes no hold; a place it does
/// name asks exactly once and leaves the icon grey.
///
/// Both halves are counted with their denominators, because a one-sided measurement passes on
/// a client that refuses **everything** -- which is exactly what a gate placed one step too
/// early would do.
pub fn a_place_the_thing_cannot_go_in_refuses_it_and_a_place_it_can_takes_it() {
    // One thing per distinct list of places, so the sweep over the figure is not repeated for
    // duplicates; the count is required so that a shrinking corpus is visible.
    let mut one_of_each: BTreeMap<u32, Thing> = BTreeMap::new();
    for t in &all_recorded_things() {
        one_of_each.entry(t.places).or_insert(*t);
    }
    assert!(
        one_of_each.len() >= 8,
        "at least eight different lists of places; got {}",
        one_of_each.len()
    );

    let mut c = a_player_at_the_figure();
    let (mut refused, mut refused_of) = (0usize, 0usize);
    let (mut taken, mut taken_of) = (0usize, 0usize);

    for t in one_of_each.values() {
        for (element, mask, _) in PAPERDOLL_REGIONS {
            // A weapon let go on the shield is a third answer and has its own scenario.
            if mask == loc::SHIELD && t.places & loc::MELEE_WEAPON != 0 {
                continue;
            }
            between_gestures(&mut c);
            carrying_the_recorded(&mut c, t);
            let before = c.interaction_mut().stats.requests_refused;
            let sent = let_go_on(&mut c, t.item, element);

            if mask & t.places == 0 {
                refused_of += 1;
                if sent.is_empty()
                    && c.interaction_mut().stats.requests_refused == before + 1
                    && !c.world_mut().weenie(t.item).expect("carried").waiting
                    && c.world_mut().request_lock.is_idle()
                {
                    refused += 1;
                }
            } else {
                taken_of += 1;
                if sent.len() == 1
                    && sent.iter().filter_map(wield_place).count() == 1
                    && c.interaction_mut().stats.requests_refused == before
                    && c.world_mut().weenie(t.item).expect("carried").waiting
                {
                    taken += 1;
                }
            }
        }
    }

    // Both directions really happened: without this a client that answered no to everything
    // would satisfy the refusing half and never reach the other.
    assert!(
        refused_of > 100,
        "the sweep must try the refusal broadly; it tried {refused_of}"
    );
    assert!(
        taken_of >= one_of_each.len(),
        "and at least one place per thing"
    );

    c.assert_behaviour(
        "inventory.equip.a-place-the-thing-cannot-go-in-refuses-it-and-a-place-it-can-takes-it",
        move |_| refused == refused_of && taken == taken_of,
    );
}

// -----------------------------------------------------------------------------------------
// inventory.equip.the-place-let-go-on-decides-whether-the-whole-list-or-one-place-goes-out
// -----------------------------------------------------------------------------------------

/// What decides whether the client asks with the thing's whole list of places or with one of
/// them is **the place it was let go on**, not the thing: the same thing leaves by both
/// answers depending on where the player aimed.
///
/// The two things are the recordings' own -- one whose list names several places to wear
/// armour in, which no single place on the figure equals, and one whose list names several
/// places among the clothes.
pub fn the_place_let_go_on_decides_whether_the_whole_list_or_one_place_goes_out() {
    let all = all_recorded_things();
    let armour = all
        .iter()
        .find(|t| t.places.count_ones() > 1 && t.places & loc::CLOTHING == 0)
        .copied()
        .expect("the recordings carry a piece of armour whose list names several places");
    let clothing = all
        .iter()
        .find(|t| t.places.count_ones() > 1 && t.places & loc::CLOTHING != 0)
        .copied()
        .expect("and a piece of clothing whose list names several places");

    let mut c = a_player_at_the_figure();
    let (element, mask, _) = the_place_for(armour.places);
    assert_eq!(
        mask & loc::CLOTHING,
        0,
        "the place it lands on is outside the clothes"
    );
    carrying_the_recorded(&mut c, &armour);
    let sent = let_go_on(&mut c, armour.item, element);
    let one_place = match sent.as_slice() {
        [r] => wield_place(r).expect("an ask to put it on"),
        _ => panic!("one drop is one ask; the client asked for {sent:?}"),
    };
    let asked_for_one = one_place.1.count_ones() == 1
        && one_place.1 & mask != 0
        && one_place.1 != armour.places
        && c.interaction_mut().stats.wields_requested == 1
        && c.interaction_mut().stats.wears_requested == 0;

    let mut c = a_player_at_the_figure();
    let (element, mask, _) = the_place_for(clothing.places);
    assert_ne!(mask & loc::CLOTHING, 0, "this place is one of the clothes");
    carrying_the_recorded(&mut c, &clothing);
    let sent = let_go_on(&mut c, clothing.item, element);
    let whole = match sent.as_slice() {
        [r] => wield_place(r).expect("an ask to put it on"),
        _ => panic!("one drop is one ask; the client asked for {sent:?}"),
    };
    let asked_for_the_whole_list = whole.1 == clothing.places
        && whole.1 == clothing.asked_for
        && c.interaction_mut().stats.wears_requested == 1
        && c.interaction_mut().stats.wields_requested == 0;

    c.assert_behaviour(
        "inventory.equip.the-place-let-go-on-decides-whether-the-whole-list-or-one-place-goes-out",
        move |_| asked_for_one && asked_for_the_whole_list,
    );
}

// -----------------------------------------------------------------------------------------
// inventory.equip.the-left-and-the-right-of-a-pair-ask-for-different-places
// -----------------------------------------------------------------------------------------

/// The left wrist and the right wrist are two places, and so are the two ring fingers: a ring
/// let go on one of them asks for that one, and the same ring let go on the other asks for
/// the other.
///
/// The thing is valid for both halves of the pair, which is what makes the side load-bearing:
/// with only one half allowed the answer would be forced whatever the client did.
pub fn the_left_and_the_right_of_a_pair_ask_for_different_places() {
    let mut agreed = 0usize;
    let mut pairs = 0usize;
    let mut c = a_player_at_the_figure();
    for pair in [loc::WRIST_WEAR, loc::FINGER_WEAR] {
        let mut asked = Vec::new();
        for (element, mask, _) in PAPERDOLL_REGIONS.iter().filter(|(_, m, _)| m & pair != 0) {
            let item = ObjectId(0x5000_0999);
            between_gestures(&mut c);
            carrying(&mut c, item, pair, 0);
            let sent = let_go_on(&mut c, item, *element);
            let (_, place) = match sent.as_slice() {
                [r] => wield_place(r).expect("an ask to put it on"),
                _ => panic!("one drop is one ask; the client asked for {sent:?}"),
            };
            assert_eq!(
                place, *mask,
                "the place the player aimed at is the place asked for"
            );
            assert_eq!(
                c.interaction_mut().stats.wields_requested,
                1,
                "jewellery is wielded, not worn"
            );
            asked.push(place);
        }
        assert_eq!(asked.len(), 2, "two halves to a pair");
        pairs += 1;
        if asked[0] != asked[1] && asked[0] | asked[1] == pair {
            agreed += 1;
        }
    }

    c.assert_behaviour(
        "inventory.equip.the-left-and-the-right-of-a-pair-ask-for-different-places",
        move |_| pairs == 2 && agreed == 2,
    );
}

// -----------------------------------------------------------------------------------------
// inventory.equip.a-weapon-let-go-on-the-shield-place-is-taken-into-the-off-hand
// -----------------------------------------------------------------------------------------

/// A melee weapon let go on the **shield** is the one place the figure invents an answer of
/// its own: the weapon's list does not name the shield at all, so the gate would refuse it,
/// and instead the client asks for the shield place -- the weapon goes in the off hand.
///
/// Both halves are asserted, because without the widening the drop is silently refused and
/// without the side the weapon is asked into the main hand instead: the same weapon let go on
/// the **weapon** place asks for the main hand, and a shield let go on the shield needs no
/// invention at all.
pub fn a_weapon_let_go_on_the_shield_place_is_taken_into_the_off_hand() {
    let shield_place = the_place_drawn_for(loc::SHIELD);
    let weapon_place = the_place_drawn_for(loc::WEAPON_READY_SLOT);
    let item = ObjectId(0x5000_099E);

    let mut c = a_player_at_the_figure();
    carrying(&mut c, item, loc::MELEE_WEAPON, 0);
    assert_eq!(
        loc::MELEE_WEAPON & loc::SHIELD,
        0,
        "the premise: the gate would refuse this"
    );
    let sent = let_go_on(&mut c, item, shield_place);
    let off_hand = sent.iter().filter_map(wield_place).collect::<Vec<_>>()
        == vec![(item, loc::SHIELD)]
        && c.world_mut().weenie(item).expect("carried").waiting;

    between_gestures(&mut c);
    carrying(&mut c, item, loc::MELEE_WEAPON, 0);
    let sent = let_go_on(&mut c, item, weapon_place);
    let main_hand =
        sent.iter().filter_map(wield_place).collect::<Vec<_>>() == vec![(item, loc::MELEE_WEAPON)];

    between_gestures(&mut c);
    carrying(&mut c, item, loc::SHIELD, 0);
    let sent = let_go_on(&mut c, item, shield_place);
    let a_shield_needs_no_invention =
        sent.iter().filter_map(wield_place).collect::<Vec<_>>() == vec![(item, loc::SHIELD)];

    c.assert_behaviour(
        "inventory.equip.a-weapon-let-go-on-the-shield-place-is-taken-into-the-off-hand",
        move |_| off_hand && main_hand && a_shield_needs_no_invention,
    );
}

// -----------------------------------------------------------------------------------------
// inventory.equip.part-of-a-stack-let-go-on-the-figure-splits-and-a-whole-one-does-not
// -----------------------------------------------------------------------------------------

/// The quantity the player set decides which request goes out: an untouched box moves the
/// whole stack, and a box set to some of them splits that many off instead -- at the same
/// place, for the player's own number.
///
/// Asserted in both directions and on both sides of the fork, because a client that ignored
/// the box and one that always split are equally plausible from a screenshot, and the two
/// sides of the fork hand the quantity over at different places.
pub fn part_of_a_stack_let_go_on_the_figure_splits_and_a_whole_one_does_not() {
    let all = all_recorded_things();
    let ammo = all
        .iter()
        .find(|t| t.places == loc::MISSILE_AMMO)
        .copied()
        .expect("the recordings carry an ammunition equip");
    let (ammo_place, ..) = the_place_for(ammo.places);

    let mut c = a_player_at_the_figure();
    carrying(&mut c, ammo.item, ammo.places, 10);
    assert!(
        c.world_mut().split.is_whole_stack(),
        "the premise: an untouched box is the whole stack"
    );
    let sent = let_go_on(&mut c, ammo.item, ammo_place);
    let the_whole_stack = sent.iter().filter_map(wield_place).collect::<Vec<_>>()
        == vec![(ammo.item, ammo.asked_for)];

    between_gestures(&mut c);
    carrying(&mut c, ammo.item, ammo.places, 10);
    c.when(Player::Ui(vec![UiRequest::StackSliderChanged {
        split: 3,
        max: 10,
    }]));
    assert_eq!(
        c.world_mut().split,
        SplitState {
            split_size: 3,
            max_split_size: 10
        },
        "the premise: the player asked for three of the ten"
    );
    let sent = let_go_on(&mut c, ammo.item, ammo_place);
    let three_of_them = matches!(
        sent.as_slice(),
        [Request::StackableSplitToWield(m)]
            if m.stack == ammo.item && m.slot == ammo.asked_for && m.amount == 3
    );
    // A split takes no hold, which is what lets the next gesture go at once.
    let and_no_hold = c.world_mut().request_lock.is_idle();

    // And the same on the other side of the fork, because the two hand the quantity over at
    // different places and a client that got only one of them right would look right here.
    let clothing = all
        .iter()
        .find(|t| t.places.count_ones() > 1 && t.places & loc::CLOTHING != 0)
        .copied()
        .expect("the recordings carry a piece of clothing whose list names several places");
    let (cloth_place, mask, _) = the_place_for(clothing.places);
    assert_ne!(mask & loc::CLOTHING, 0, "and it lands among the clothes");

    let mut c = a_player_at_the_figure();
    carrying(&mut c, clothing.item, clothing.places, 10);
    let sent = let_go_on(&mut c, clothing.item, cloth_place);
    let worn_whole = sent.iter().filter_map(wield_place).collect::<Vec<_>>()
        == vec![(clothing.item, clothing.places)]
        && c.interaction_mut().stats.wears_requested == 1;

    between_gestures(&mut c);
    carrying(&mut c, clothing.item, clothing.places, 10);
    c.when(Player::Ui(vec![UiRequest::StackSliderChanged {
        split: 4,
        max: 10,
    }]));
    let sent = let_go_on(&mut c, clothing.item, cloth_place);
    let worn_in_part = matches!(
        sent.as_slice(),
        [Request::StackableSplitToWield(m)]
            if m.stack == clothing.item && m.slot == clothing.places && m.amount == 4
    ) && c.interaction_mut().stats.wears_requested == 1;

    c.assert_behaviour(
        "inventory.equip.part-of-a-stack-let-go-on-the-figure-splits-and-a-whole-one-does-not",
        move |_| the_whole_stack && three_of_them && and_no_hold && worn_whole && worn_in_part,
    );
}

// -----------------------------------------------------------------------------------------
// inventory.equip.a-place-that-is-full-with-nothing-recorded-in-it-refuses-without-a-word
// -----------------------------------------------------------------------------------------

/// A place the thing is allowed in, which the client believes is full but has no record of
/// what is in, refuses the drop and says nothing at all: nothing is asked of the shard, the
/// refusal is counted, the grey comes off and no hold is taken.
///
/// The silence is the claim. Every per-place *"you are already wearing one of those"* line
/// belongs to the other way of putting something on, and the figure never shows one: a client
/// that spoke here would be answering a drop with a sentence meant for a shortcut.
///
/// The control is the same drop with the place free, so that what is being measured is the
/// place being full rather than the thing or the place itself. What happens when the client
/// **does** know what is in the way is its own scenario: the thing in the way goes to the
/// pack.
pub fn a_place_that_is_full_with_nothing_recorded_in_it_refuses_without_a_word() {
    let trinket = all_recorded_things()
        .into_iter()
        .find(|t| t.places == loc::TRINKET_ONE)
        .expect("the recordings carry a trinket equip");
    let (element, mask, _) = the_place_for(trinket.places);
    assert_ne!(
        mask & trinket.places,
        0,
        "the premise: the gate lets this through"
    );

    let mut c = a_player_at_the_figure();
    carrying_the_recorded(&mut c, &trinket);
    c.world_mut().inventory_mask = loc::TRINKET_ONE;
    assert_eq!(
        c.world_mut().inv_slots.item_at(loc::TRINKET_ONE),
        None,
        "the premise: the place is full and the client has no record of what is in it"
    );

    let said_before = said_so_far(&mut c);
    let refused_before = c.interaction_mut().stats.requests_refused;
    let sent = let_go_on(&mut c, trinket.item, element);
    let said = said_since(&mut c, said_before);
    let refused_in_silence = sent.is_empty()
        && said.is_empty()
        && c.interaction_mut().stats.requests_refused == refused_before + 1
        && c.interaction_mut().stats.wields_requested == 0
        && c.interaction_mut().stats.wears_requested == 0
        && c.interaction_mut().stats.unblocks_started == 0
        && !c.world_mut().weenie(trinket.item).expect("carried").waiting
        && c.world_mut().request_lock.is_idle();

    let mut c = a_player_at_the_figure();
    carrying_the_recorded(&mut c, &trinket);
    let sent = let_go_on(&mut c, trinket.item, element);
    let the_free_place_takes_it = sent.iter().filter_map(wield_place).collect::<Vec<_>>()
        == vec![(trinket.item, trinket.asked_for)];

    c.assert_behaviour(
        "inventory.equip.a-place-that-is-full-with-nothing-recorded-in-it-refuses-without-a-word",
        move |_| refused_in_silence && the_free_place_takes_it,
    );
}

dereth_testkit::scenarios! {
    scenario_every_recorded_equip_let_go_on_its_own_place_asks_for_what_was_recorded => every_recorded_equip_let_go_on_its_own_place_asks_for_what_was_recorded ["inventory.equip.every-recorded-one-let-go-on-its-own-place-asks-for-what-was-recorded"],
    scenario_a_place_the_thing_cannot_go_in_refuses_it_and_a_place_it_can_takes_it => a_place_the_thing_cannot_go_in_refuses_it_and_a_place_it_can_takes_it ["inventory.equip.a-place-the-thing-cannot-go-in-refuses-it-and-a-place-it-can-takes-it"],
    scenario_the_place_let_go_on_decides_whether_the_whole_list_or_one_place_goes_out => the_place_let_go_on_decides_whether_the_whole_list_or_one_place_goes_out ["inventory.equip.the-place-let-go-on-decides-whether-the-whole-list-or-one-place-goes-out"],
    scenario_the_left_and_the_right_of_a_pair_ask_for_different_places => the_left_and_the_right_of_a_pair_ask_for_different_places ["inventory.equip.the-left-and-the-right-of-a-pair-ask-for-different-places"],
    scenario_a_weapon_let_go_on_the_shield_place_is_taken_into_the_off_hand => a_weapon_let_go_on_the_shield_place_is_taken_into_the_off_hand ["inventory.equip.a-weapon-let-go-on-the-shield-place-is-taken-into-the-off-hand"],
    scenario_part_of_a_stack_let_go_on_the_figure_splits_and_a_whole_one_does_not => part_of_a_stack_let_go_on_the_figure_splits_and_a_whole_one_does_not ["inventory.equip.part-of-a-stack-let-go-on-the-figure-splits-and-a-whole-one-does-not"],
    scenario_a_place_that_is_full_with_nothing_recorded_in_it_refuses_without_a_word => a_place_that_is_full_with_nothing_recorded_in_it_refuses_without_a_word ["inventory.equip.a-place-that-is-full-with-nothing-recorded-in-it-refuses-without-a-word"],
    scenario_what_the_shard_says_is_worn_is_remembered_and_taking_it_off_forgets_only_that => what_the_shard_says_is_worn_is_remembered_and_taking_it_off_forgets_only_that ["inventory.wear.what-the-shard-says-is-worn-is-remembered-and-taking-it-off-forgets-only-that"],
    scenario_every_recorded_one_whose_place_is_taken_moves_the_blocker_or_is_refused_by_name => every_recorded_one_whose_place_is_taken_moves_the_blocker_or_is_refused_by_name ["inventory.unblock.every-recorded-one-whose-place-is-taken-moves-the-blocker-or-is-refused-by-name"],
    scenario_the_thing_in_the_way_goes_to_the_pack_and_the_new_one_goes_on_after => the_thing_in_the_way_goes_to_the_pack_and_the_new_one_goes_on_after ["inventory.unblock.the-thing-in-the-way-goes-to-the-pack-and-the-new-one-goes-on-after"],
    scenario_what_comes_off_is_the_last_full_place_the_walk_looked_at => what_comes_off_is_the_last_full_place_the_walk_looked_at ["inventory.unblock.what-comes-off-is-the-last-full-place-the-walk-looked-at"],
    scenario_it_is_never_begun_twice_and_a_thing_is_not_taken_off_to_put_itself_on => it_is_never_begun_twice_and_a_thing_is_not_taken_off_to_put_itself_on ["inventory.unblock.it-is-never-begun-twice-and-a-thing-is-not-taken-off-to-put-itself-on"],
    scenario_a_refused_move_ungreys_the_thing_that_was_waiting_for_it => a_refused_move_ungreys_the_thing_that_was_waiting_for_it ["inventory.unblock.a-refused-move-ungreys-the-thing-that-was-waiting-for-it"],
    scenario_a_blocker_that_went_somewhere_else_does_not_put_the_new_thing_on => a_blocker_that_went_somewhere_else_does_not_put_the_new_thing_on ["inventory.unblock.a-blocker-that-went-somewhere-else-does-not-put-the-new-thing-on"],
    scenario_a_ring_let_go_on_the_full_hand_takes_that_ring_off_rather_than_the_other => a_ring_let_go_on_the_full_hand_takes_that_ring_off_rather_than_the_other ["inventory.unblock.a-ring-let-go-on-the-full-hand-takes-that-ring-off-rather-than-the-other"],
    scenario_the_figure_says_only_what_it_is_moving_where_the_sort_names_every_full_place => the_figure_says_only_what_it_is_moving_where_the_sort_names_every_full_place ["inventory.unblock.the-figure-says-only-what-it-is-moving-where-the-sort-names-every-full-place"],
    scenario_adding_to_a_stack_on_the_body_that_cannot_take_more_says_so_and_moves_nothing => adding_to_a_stack_on_the_body_that_cannot_take_more_says_so_and_moves_nothing ["inventory.unblock.adding-to-a-stack-on-the-body-that-cannot-take-more-says-so-and-moves-nothing"],
    scenario_a_blocked_double_click_asks_for_the_same_two_things_as_a_blocked_drop => a_blocked_double_click_asks_for_the_same_two_things_as_a_blocked_drop ["inventory.double-click.a-blocked-one-asks-for-the-same-two-things-as-a-blocked-drop"],
    scenario_a_blocked_double_click_says_only_which_thing_is_being_moved_and_says_it_once => a_blocked_double_click_says_only_which_thing_is_being_moved_and_says_it_once ["inventory.double-click.a-blocked-one-says-only-which-thing-is-being-moved-and-says-it-once"],
    scenario_a_ring_may_go_on_the_free_hand_where_a_drop_on_the_full_one_may_not => a_ring_may_go_on_the_free_hand_where_a_drop_on_the_full_one_may_not ["inventory.double-click.a-ring-may-go-on-the-free-hand-where-a-drop-on-the-full-one-may-not"],
    scenario_something_that_can_be_worn_or_held_falls_through_to_the_hand => something_that_can_be_worn_or_held_falls_through_to_the_hand ["inventory.double-click.something-that-can-be-worn-or-held-falls-through-to-the-hand"],
    scenario_a_thing_that_cannot_be_held_in_a_fight_says_so => a_thing_that_cannot_be_held_in_a_fight_says_so ["inventory.double-click.a-thing-that-cannot-be-held-in-a-fight-says-so"],
    scenario_a_piece_of_armour_whose_place_is_taken_is_refused_out_loud => a_piece_of_armour_whose_place_is_taken_is_refused_out_loud ["inventory.double-click.a-piece-of-armour-whose-place-is-taken-is-refused-out-loud"],
    scenario_every_recorded_one_answers_as_the_drop_does_wherever_they_share_a_path => every_recorded_one_answers_as_the_drop_does_wherever_they_share_a_path ["inventory.double-click.every-recorded-one-answers-as-the-drop-does-wherever-they-share-a-path"],
    scenario_a_stack_let_go_on_a_matching_one_on_the_body_merges_what_fits => a_stack_let_go_on_a_matching_one_on_the_body_merges_what_fits ["inventory.equip.a-stack-let-go-on-a-matching-one-on-the-body-merges-what-fits"],
    scenario_the_name_in_that_refusal_is_pluralised_by_the_clients_own_simple_rule => the_name_in_that_refusal_is_pluralised_by_the_clients_own_simple_rule ["inventory.equip.the-name-in-that-refusal-is-pluralised-by-the-clients-own-simple-rule"],
    scenario_a_weapon_that_cannot_share_the_hands_takes_what_is_in_the_way_off_first => a_weapon_that_cannot_share_the_hands_takes_what_is_in_the_way_off_first ["inventory.equip.a-weapon-that-cannot-share-the-hands-takes-what-is-in-the-way-off-first"],
    scenario_a_shield_is_refused_in_words_while_a_weapon_for_both_hands_is_held => a_shield_is_refused_in_words_while_a_weapon_for_both_hands_is_held ["inventory.equip.a-shield-is-refused-in-words-while-a-weapon-for-both-hands-is-held"],
    scenario_the_other_key_for_a_tile_selects_what_is_in_it_instead_of_using_it => the_other_key_for_a_tile_selects_what_is_in_it_instead_of_using_it ["inventory.shortcut-bar.the-other-key-for-a-tile-selects-what-is-in-it-instead-of-using-it"],
    scenario_the_key_that_hides_the_interface_hides_it => the_key_that_hides_the_interface_hides_it ["ui.hot-key.the-key-that-hides-the-interface-hides-it"],
    scenario_the_bound_key_asks_to_leave_and_the_player_stays_until_the_shard_answers => the_bound_key_asks_to_leave_and_the_player_stays_until_the_shard_answers ["shell.logout.the-bound-key-asks-to-leave-and-the-player-stays-until-the-shard-answers"],
    scenario_a_press_on_a_slot_selects_examines_or_uses_the_thing_in_that_slot => a_press_on_a_slot_selects_examines_or_uses_the_thing_in_that_slot ["inventory.pack.a-press-on-a-slot-selects-examines-or-uses-the-thing-in-that-slot"],
    scenario_the_two_buttons_act_on_what_is_selected_or_arm_the_pointer_when_nothing_is => the_two_buttons_act_on_what_is_selected_or_arm_the_pointer_when_nothing_is ["use.button.the-two-buttons-act-on-what-is-selected-or-arm-the-pointer-when-nothing-is"],
    scenario_letting_go_over_the_view_of_the_world_is_a_drop_into_it => letting_go_over_the_view_of_the_world_is_a_drop_into_it ["inventory.world-drop.letting-go-over-the-view-of-the-world-is-a-drop-into-it"],
    scenario_the_shards_refusal_lets_go_of_the_one_at_a_time_hold_as_well_as_the_grey => the_shards_refusal_lets_go_of_the_one_at_a_time_hold_as_well_as_the_grey ["inventory.place.the-shards-refusal-lets-go-of-the-one-at-a-time-hold-as-well-as-the-grey"],
    scenario_turning_the_camera_with_the_right_button_is_not_an_appraisal => turning_the_camera_with_the_right_button_is_not_an_appraisal ["ui.pointer.turning-the-camera-with-the-right-button-is-not-an-appraisal"],
    scenario_typing_a_number_into_the_box_and_leaving_it_sets_how_many_a_drag_moves => typing_a_number_into_the_box_and_leaving_it_sets_how_many_a_drag_moves ["inventory.split.typing-a-number-into-the-box-and-leaving-it-sets-how-many-a-drag-moves"],
    scenario_while_the_number_box_holds_the_keyboard_a_tiles_key_types_instead_of_firing => while_the_number_box_holds_the_keyboard_a_tiles_key_types_instead_of_firing ["inventory.split.while-the-number-box-holds-the-keyboard-a-tiles-key-types-instead-of-firing"],
}

// -----------------------------------------------------------------------------------------
// inventory.wear.what-the-shard-says-is-worn-is-remembered-and-taking-it-off-forgets-only-that
// -----------------------------------------------------------------------------------------

/// What the client believes the player is wearing follows the shard's own word for it, and
/// the two kinds of thing are remembered differently: something wielded takes the one place
/// it went into, and something worn takes the whole of its own list of places and its own
/// rank among the clothes -- so a shirt that covers the chest and the legs is drawn in both.
///
/// Taking one thing off forgets that thing and nothing else, and the list of places the login
/// hands over fills the same memory the shard's later word does.
///
/// Until this was kept, a running client believed every place on the body was empty however
/// much the character was wearing, so no drop could ever be in anybody's way.
pub fn what_the_shard_says_is_worn_is_remembered_and_taking_it_off_forgets_only_that() {
    let trinket = ObjectId(0x5000_9001);
    let shirt = ObjectId(0x5000_9002);
    let helm = ObjectId(0x5000_9003);

    let mut c = a_player_at_the_figure();
    carrying(&mut c, trinket, loc::TRINKET_ONE, 1);
    c.when(the_shard_says_worn(trinket, loc::TRINKET_ONE));
    let a_wielded_thing_takes_its_one_place = c.world_mut().inventory_mask == loc::TRINKET_ONE
        && c.world_mut().inv_slots.item_at(loc::TRINKET_ONE) == Some(trinket)
        && c.world_mut().clothing_priority_mask == 0;

    carrying(&mut c, shirt, loc::CHEST_WEAR | loc::UPPER_LEG_WEAR, 1);
    c.world_mut()
        .tables
        .weenies
        .get_mut(shirt)
        .expect("carried")
        .pwd
        .priority = Some(0x40);
    c.when(the_shard_says_worn(shirt, loc::CHEST_WEAR));
    let a_worn_thing_takes_its_whole_list = c.world_mut().inventory_mask
        == loc::TRINKET_ONE | loc::CHEST_WEAR | loc::UPPER_LEG_WEAR
        && c.world_mut().clothing_priority_mask == 0x40
        && c.world_mut().inv_slots.item_at(loc::CHEST_WEAR) == Some(shirt)
        && c.world_mut().inv_slots.item_at(loc::UPPER_LEG_WEAR) == Some(shirt);

    // The shard taking the trinket off again.
    c.when(Inbound::event(
        dereth_client_net::client_session::SessionEvent::UiEvent {
            opcode: dereth_protocol::Opcode(0x019A),
            blob: {
                let mut b = 0x019A_u32.to_le_bytes().to_vec();
                b.extend_from_slice(&trinket.0.to_le_bytes());
                b
            },
        },
    ));
    let taking_one_off_forgets_only_that = c.world_mut().inventory_mask
        == loc::CHEST_WEAR | loc::UPPER_LEG_WEAR
        && c.world_mut().inv_slots.item_at(loc::TRINKET_ONE).is_none()
        && c.world_mut().clothing_priority_mask == 0x40;

    // And the login's own list of placements fills the same memory.
    let mut c = a_player_at_the_figure();
    carrying(&mut c, helm, loc::HEAD_WEAR, 1);
    c.when(Inbound::event(
        dereth_client_net::client_session::SessionEvent::PlayerDescription(Box::new(
            dereth_protocol::login::LoginPlayerDescription {
                inventory_placements: vec![dereth_protocol::types::InventoryPlacement {
                    iid: helm,
                    location: loc::HEAD_WEAR,
                    priority: 0x4,
                }],
                ..dereth_protocol::login::LoginPlayerDescription::default()
            },
        )),
    ));
    let the_login_fills_it_too = c.world_mut().inventory_mask == loc::HEAD_WEAR
        && c.world_mut().clothing_priority_mask == 0x4
        && c.world_mut().inv_slots.item_at(loc::HEAD_WEAR) == Some(helm);

    c.assert_behaviour(
            "inventory.wear.what-the-shard-says-is-worn-is-remembered-and-taking-it-off-forgets-only-that",
            move |_| {
                a_wielded_thing_takes_its_one_place
                    && a_worn_thing_takes_its_whole_list
                    && taking_one_off_forgets_only_that
                    && the_login_fills_it_too
            },
        );
}

// -----------------------------------------------------------------------------------------
// inventory.unblock.every-recorded-one-whose-place-is-taken-moves-the-blocker-or-is-refused-by-name
// -----------------------------------------------------------------------------------------

/// Every thing the recordings record being put on, let go on its own place with that place
/// already filled by something carrying the recording's own list of places and its own rank
/// among the clothes, gets one of exactly two answers -- and which one depends only on where
/// it was let go.
///
/// Outside the clothes the client asks the shard to move the thing in the way into the
/// player's own pack, remembers what it was doing and leaves the new thing greyed. Among the
/// clothes there is no such move at all: the drop is refused by name, saying which piece has
/// to come off first. The two are counted separately with their own denominators, because a
/// client that grew the move where the clothes are would look right on one total.
pub fn every_recorded_one_whose_place_is_taken_moves_the_blocker_or_is_refused_by_name() {
    let all = all_recorded_things();
    let (mut outside, mut moved) = (0usize, 0usize);
    let (mut among_the_clothes, mut refused_by_name) = (0usize, 0usize);

    for (i, t) in all.iter().enumerate() {
        let (element, mask, _) = the_place_for(t.places);
        let mut c = a_player_at_the_figure();
        let player = c.world_mut().player.expect("a player");
        let blocker = ObjectId(0xB000_0000 + u32::try_from(i).expect("small"));
        // The thing in the way fills **every** place the new one names, which is what a
        // dressed character looks like: filling only the place under the pointer would leave
        // the walk somewhere else to go and the drop would not be blocked at all.
        already_wearing(&mut c, blocker, t.places, t.places, t.priority);
        carrying_the_recorded(&mut c, t);

        let said_before = said_so_far(&mut c);
        let sent = let_go_on(&mut c, t.item, element);
        let said = said_since(&mut c, said_before);

        if mask & loc::CLOTHING == 0 {
            outside += 1;
            let u = c.world_mut().unblock;
            if sent.iter().filter_map(moved_into).collect::<Vec<_>>() == vec![(blocker, player)]
                && sent.len() == 1
                && u.unblock_attempt_num == 1
                && u.blocked_id == Some(t.item)
                && u.blocking_id == Some(blocker)
                && c.world_mut().weenie(t.item).expect("carried").waiting
            {
                moved += 1;
            }
        } else {
            among_the_clothes += 1;
            if sent.is_empty()
                && c.world_mut().unblock.unblock_attempt_num == 0
                && said.iter().any(|(_, t)| {
                    t.starts_with("You must remove your ") && t.ends_with(" to wear that")
                })
            {
                refused_by_name += 1;
            }
        }
    }

    // The recordings' own shape, so that a corpus that stopped carrying equips reads as a
    // broken checkout rather than as a clean sweep over nothing.
    assert_eq!(
        outside + among_the_clothes,
        all.len(),
        "every recorded equip was replayed"
    );
    assert_eq!(
        (outside, among_the_clothes),
        (21, 11),
        "the recordings' own split"
    );

    // Every trial above stood its own client up, because what the client remembers about a
    // half-finished one of these outlives a gesture; this one is only here to carry the
    // claim.
    let mut c = a_player_at_the_figure();
    c.assert_behaviour(
            "inventory.unblock.every-recorded-one-whose-place-is-taken-moves-the-blocker-or-is-refused-by-name",
            move |_| moved == outside && refused_by_name == among_the_clothes,
        );
}

// -----------------------------------------------------------------------------------------
// inventory.unblock.the-thing-in-the-way-goes-to-the-pack-and-the-new-one-goes-on-after
// -----------------------------------------------------------------------------------------

/// The whole of it, in order: let something go on a place another thing is in, and the client
/// asks for **that other thing** to go into the player's pack -- nothing else -- and says so
/// by name. The new thing stays greyed, because nothing has been asked about it yet. When the
/// shard says the old thing landed in the pack, and only then, the client asks for the new
/// one to be put on, at the place the old one has just left.
///
/// Each station is asserted on its own: asking only about the second would let a client that
/// sent both at once pass, and asking only about the first would let one that never came back
/// pass.
pub fn the_thing_in_the_way_goes_to_the_pack_and_the_new_one_goes_on_after() {
    let trinket = all_recorded_things()
        .into_iter()
        .find(|t| t.places == loc::TRINKET_ONE)
        .expect("the recordings carry a trinket equip");
    let (element, mask, _) = the_place_for(trinket.places);

    let mut c = a_player_at_the_figure();
    let player = c.world_mut().player.expect("a player");
    let blocker = ObjectId(0x5000_0B01);
    already_wearing(&mut c, blocker, loc::TRINKET_ONE, mask, 0);
    c.world_mut()
        .tables
        .weenies
        .get_mut(blocker)
        .expect("worn")
        .pwd
        .name = "Ring of Chorizite".into();
    carrying_the_recorded(&mut c, &trinket);

    let said_before = said_so_far(&mut c);
    let first = let_go_on(&mut c, trinket.item, element);
    let said = said_since(&mut c, said_before);
    let u = c.world_mut().unblock;
    let the_old_one_goes_to_the_pack = first.len() == 1
        && first.iter().filter_map(moved_into).collect::<Vec<_>>() == vec![(blocker, player)]
        && c.interaction_mut().stats.unblocks_started == 1
        && c.interaction_mut().stats.wields_requested == 0
        && c.interaction_mut().stats.requests_refused == 0
        && u.unblock_attempt_num == 1
        && u.blocking_id == Some(blocker)
        && u.blocked_id == Some(trinket.item)
        && u.blocking_dest_id == Some(player)
        && u.blocked_side == SlotSide::Null
        && c.world_mut().weenie(trinket.item).expect("carried").waiting;
    // The one line, and it is the only one: every per-place sentence is suppressed here.
    let said_by_name = said.len() == 1 && said[0].1 == "Moving Ring of Chorizite to your backpack";

    let before = c.outbound().len();
    c.when(the_shard_says_contained(blocker, player));
    let second = c.outbound()[before..].to_vec();
    let the_new_one_goes_on = second.len() == 1
            && second.iter().filter_map(wield_place).collect::<Vec<_>>()
                == vec![(trinket.item, trinket.asked_for)]
            && c.interaction_mut().stats.unblock_retries == 1
            && c.world_mut().unblock.unblock_attempt_num == 0
            // The old thing really did leave the body, which is why the place was free.
            && c.world_mut().inventory_mask & loc::TRINKET_ONE == 0
            && c.world_mut().inv_slots.item_at(loc::TRINKET_ONE).is_none();

    c.assert_behaviour(
        "inventory.unblock.the-thing-in-the-way-goes-to-the-pack-and-the-new-one-goes-on-after",
        move |_| the_old_one_goes_to_the_pack && said_by_name && the_new_one_goes_on,
    );
}

// -----------------------------------------------------------------------------------------
// inventory.unblock.what-comes-off-is-the-last-full-place-the-walk-looked-at
// -----------------------------------------------------------------------------------------

/// When the thing being put on could go in two places and both are full, the one that comes
/// off is the thing in the place the client looked at **last**, not the first -- and the line
/// the player reads names that same thing.
///
/// The two places are picked out of the client's own order by their position in it rather
/// than written down here, so the claim would still hold if that order changed.
pub fn what_comes_off_is_the_last_full_place_the_walk_looked_at() {
    let early = WIELD_SLOT_ORDER[1].0;
    let late = WIELD_SLOT_ORDER[6].0;
    assert_ne!(early, late, "two different places");

    let mut c = a_player_at_the_figure();
    let item = ObjectId(0x5000_0C00);
    let first_in_the_way = ObjectId(0x5000_0C01);
    let last_in_the_way = ObjectId(0x5000_0C02);
    already_wearing(&mut c, first_in_the_way, early, early, 0);
    c.world_mut()
        .tables
        .weenies
        .get_mut(first_in_the_way)
        .expect("worn")
        .pwd
        .name = "Trinket".into();
    already_wearing(&mut c, last_in_the_way, late, late, 0);
    c.world_mut()
        .tables
        .weenies
        .get_mut(last_in_the_way)
        .expect("worn")
        .pwd
        .name = "Helm".into();
    carrying(&mut c, item, early | late, 1);

    let (element, ..) = the_place_for(early);
    let said_before = said_so_far(&mut c);
    let sent = let_go_on(&mut c, item, element);
    let said = said_since(&mut c, said_before);

    let the_last_one_comes_off = sent.len() == 1
        && sent.iter().filter_map(moved_into).next().map(|(i, _)| i) == Some(last_in_the_way)
        && c.world_mut().unblock.blocking_id == Some(last_in_the_way);
    let and_is_named = said
        .iter()
        .any(|(_, t)| t == "Moving Helm to your backpack");
    let and_the_first_one_stays = c.world_mut().inv_slots.item_at(early) == Some(first_in_the_way)
        && !said.iter().any(|(_, t)| t.contains("Trinket"));

    c.assert_behaviour(
        "inventory.unblock.what-comes-off-is-the-last-full-place-the-walk-looked-at",
        move |_| the_last_one_comes_off && and_is_named && and_the_first_one_stays,
    );
}

// -----------------------------------------------------------------------------------------
// inventory.unblock.it-is-never-begun-twice-and-a-thing-is-not-taken-off-to-put-itself-on
// -----------------------------------------------------------------------------------------

/// Making room for something is a thing the client is either doing or not: a second drop
/// while one is already in the air leaves it doing exactly one, never two, so nothing can
/// pile up behind a shard that is slow to answer.
///
/// And the one case that would otherwise go round for ever -- letting go, on the very place
/// it is already in, the thing that is in the way -- is refused outright: nothing is asked of
/// the shard, nothing is remembered, and the icon comes back un-greyed.
pub fn it_is_never_begun_twice_and_a_thing_is_not_taken_off_to_put_itself_on() {
    let (element, ..) = the_place_for(loc::TRINKET_ONE);

    let mut c = a_player_at_the_figure();
    let item = ObjectId(0x5000_0D00);
    let blocker = ObjectId(0x5000_0D01);
    already_wearing(&mut c, blocker, loc::TRINKET_ONE, loc::TRINKET_ONE, 0);
    carrying(&mut c, item, loc::TRINKET_ONE, 1);
    let nothing_yet = c.world_mut().unblock.unblock_attempt_num == 0;
    let _ = let_go_on(&mut c, item, element);
    let one_after_the_first = c.world_mut().unblock.unblock_attempt_num == 1;

    between_gestures(&mut c);
    carrying(&mut c, item, loc::TRINKET_ONE, 1);
    let _ = let_go_on(&mut c, item, element);
    let still_one_after_the_second = c.world_mut().unblock.unblock_attempt_num == 1;

    // The loop that never happens: the thing in the way is the thing being let go.
    let mut c = a_player_at_the_figure();
    let worn = ObjectId(0x5000_0D10);
    already_wearing(&mut c, worn, loc::TRINKET_ONE, loc::TRINKET_ONE, 0);
    c.world_mut()
        .tables
        .weenies
        .get_mut(worn)
        .expect("worn")
        .waiting = true;
    let refused_before = c.interaction_mut().stats.requests_refused;
    let sent = let_go_on(&mut c, worn, element);
    let putting_it_on_itself_is_refused = sent.is_empty()
        && c.world_mut().unblock.unblock_attempt_num == 0
        && c.interaction_mut().stats.unblocks_started == 0
        && c.interaction_mut().stats.requests_refused == refused_before + 1
        && !c.world_mut().weenie(worn).expect("worn").waiting;

    c.assert_behaviour(
        "inventory.unblock.it-is-never-begun-twice-and-a-thing-is-not-taken-off-to-put-itself-on",
        move |_| {
            nothing_yet
                && one_after_the_first
                && still_one_after_the_second
                && putting_it_on_itself_is_refused
        },
    );
}

// -----------------------------------------------------------------------------------------
// inventory.unblock.a-refused-move-ungreys-the-thing-that-was-waiting-for-it
// -----------------------------------------------------------------------------------------

/// The shard can refuse to move the thing that is in the way, and then there is nothing to
/// come back for: the client forgets what it was doing and takes the grey off the thing the
/// player let go -- which never had a request of its own -- and nothing more goes out.
pub fn a_refused_move_ungreys_the_thing_that_was_waiting_for_it() {
    let (element, ..) = the_place_for(loc::TRINKET_ONE);
    let mut c = a_player_at_the_figure();
    let item = ObjectId(0x5000_0E00);
    let blocker = ObjectId(0x5000_0E01);
    already_wearing(&mut c, blocker, loc::TRINKET_ONE, loc::TRINKET_ONE, 0);
    carrying(&mut c, item, loc::TRINKET_ONE, 1);
    let _ = let_go_on(&mut c, item, element);
    let greyed_while_it_waits = c.world_mut().weenie(item).expect("carried").waiting;

    let before = c.outbound().len();
    c.when(the_shard_refuses(blocker));
    let nothing_more = c.outbound().len() == before;
    let u = c.world_mut().unblock;
    let forgotten = c.interaction_mut().stats.unblocks_abandoned == 1
        && u.unblock_attempt_num == 0
        && u.blocking_id.is_none()
        && u.blocked_id.is_none();
    let ungreyed = !c.world_mut().weenie(item).expect("carried").waiting;

    c.assert_behaviour(
        "inventory.unblock.a-refused-move-ungreys-the-thing-that-was-waiting-for-it",
        move |_| greyed_while_it_waits && nothing_more && forgotten && ungreyed,
    );
}

// -----------------------------------------------------------------------------------------
// inventory.unblock.a-blocker-that-went-somewhere-else-does-not-put-the-new-thing-on
// -----------------------------------------------------------------------------------------

/// The thing in the way can land somewhere other than where the client asked for it -- into a
/// chest, say -- and then the new thing is **not** put on: the client drops what it was doing
/// rather than acting on a move it did not make. And a wholly unrelated thing moving while
/// one of these is in the air is ignored, because otherwise every pickup would cancel it.
pub fn a_blocker_that_went_somewhere_else_does_not_put_the_new_thing_on() {
    let (element, ..) = the_place_for(loc::TRINKET_ONE);

    let mut c = a_player_at_the_figure();
    let player = c.world_mut().player.expect("a player");
    let item = ObjectId(0x5000_0F00);
    let blocker = ObjectId(0x5000_0F01);
    already_wearing(&mut c, blocker, loc::TRINKET_ONE, loc::TRINKET_ONE, 0);
    carrying(&mut c, item, loc::TRINKET_ONE, 1);
    let _ = let_go_on(&mut c, item, element);
    let asked_for_the_pack = c.world_mut().unblock.blocking_dest_id == Some(player);

    let before = c.outbound().len();
    c.when(the_shard_says_contained(blocker, ObjectId(0x5000_FEED)));
    let nothing_followed = c.outbound().len() == before
        && c.interaction_mut().stats.unblock_retries == 0
        && c.interaction_mut().stats.unblocks_abandoned == 1
        && c.world_mut().unblock.unblock_attempt_num == 0;

    // And an unrelated move leaves it alone.
    let mut c = a_player_at_the_figure();
    let player = c.world_mut().player.expect("a player");
    let other = ObjectId(0x5000_0F10);
    let blocker = ObjectId(0x5000_0F11);
    let unrelated = ObjectId(0x5000_0F12);
    already_wearing(&mut c, blocker, loc::TRINKET_ONE, loc::TRINKET_ONE, 0);
    carrying(&mut c, other, loc::TRINKET_ONE, 1);
    let _ = let_go_on(&mut c, other, element);
    carrying(&mut c, unrelated, 0, 1);
    c.when(the_shard_says_contained(unrelated, player));
    let an_unrelated_move_changes_nothing = c.world_mut().unblock.unblock_attempt_num == 1
        && c.interaction_mut().stats.unblocks_abandoned == 0;

    c.assert_behaviour(
        "inventory.unblock.a-blocker-that-went-somewhere-else-does-not-put-the-new-thing-on",
        move |_| asked_for_the_pack && nothing_followed && an_unrelated_move_changes_nothing,
    );
}

// -----------------------------------------------------------------------------------------
// inventory.unblock.a-ring-let-go-on-the-full-hand-takes-that-ring-off-rather-than-the-other
// -----------------------------------------------------------------------------------------

/// A ring let go on the hand that is already wearing one takes **that** ring off, rather than
/// quietly going on the free hand: where the player aimed is where the ring goes. And after
/// the shard has moved the old ring into the pack, the new one goes on the hand the player
/// aimed at, because that side was carried across the round trip.
///
/// This is the case where getting it wrong looks like the feature working: a client that
/// wandered to the other hand would put the ring on, silently, in the wrong place.
pub fn a_ring_let_go_on_the_full_hand_takes_that_ring_off_rather_than_the_other() {
    let (element, mask, side) = PAPERDOLL_REGIONS
        .iter()
        .find(|(_, m, s)| *m == loc::FINGER_WEAR_LEFT && *s == SlotSide::Left)
        .map(|(e, m, s)| (*e, *m, *s))
        .expect("the figure draws a left ring finger");
    assert_eq!((mask, side), (loc::FINGER_WEAR_LEFT, SlotSide::Left));

    let mut c = a_player_at_the_figure();
    let player = c.world_mut().player.expect("a player");
    let on_the_left = ObjectId(0x5000_1001);
    let ring = ObjectId(0x5000_1002);
    already_wearing(
        &mut c,
        on_the_left,
        loc::FINGER_WEAR,
        loc::FINGER_WEAR_LEFT,
        0,
    );
    carrying(&mut c, ring, loc::FINGER_WEAR, 1);
    let the_other_hand_is_free =
        c.world_mut().inventory_mask & loc::FINGER_WEAR == loc::FINGER_WEAR_LEFT;

    let sent = let_go_on(&mut c, ring, element);
    let the_aimed_hand_is_cleared = sent.len() == 1
        && sent.iter().filter_map(moved_into).next().map(|(i, _)| i) == Some(on_the_left)
        && c.world_mut().unblock.blocked_side == SlotSide::Left;

    let before = c.outbound().len();
    c.when(the_shard_says_contained(on_the_left, player));
    let and_goes_back_on_that_hand = c.outbound()[before..]
        .iter()
        .filter_map(wield_place)
        .collect::<Vec<_>>()
        == vec![(ring, loc::FINGER_WEAR_LEFT)];

    c.assert_behaviour(
            "inventory.unblock.a-ring-let-go-on-the-full-hand-takes-that-ring-off-rather-than-the-other",
            move |_| the_other_hand_is_free && the_aimed_hand_is_cleared && and_goes_back_on_that_hand,
        );
}

// -----------------------------------------------------------------------------------------
// inventory.unblock.the-figure-says-only-what-it-is-moving-where-the-sort-names-every-full-place
// -----------------------------------------------------------------------------------------

/// The same blocked equip is spoken about in two entirely different ways depending on the
/// gesture, and both are right.
///
/// Let go on the figure, the player is told **one** thing -- which piece is being moved to
/// the pack to make room -- and none of the fifteen sentences about already wearing one of
/// those. Double-clicked, where the client sorts the thing into whatever place it can find
/// and never moves anything out of the way, the player gets those sentences instead, one for
/// each full place, in the order the client looked at them.
///
/// **It is measured through the two gestures a player can actually make**, against the same
/// world, rather than by calling the decision directly with two argument tuples, which is
/// strictly the stronger reading of the same claim.
pub fn the_figure_says_only_what_it_is_moving_where_the_sort_names_every_full_place() {
    // Two places that are both outside the clothes, so that nothing here is about a wear:
    // the trinket, which the client looks at second, and the first of the aetheria, which it
    // looks at fourth.
    let (early, early_line) = WIELD_SLOT_ORDER[1];
    let (late, late_line) = WIELD_SLOT_ORDER[3];
    assert_eq!(
        (early | late) & loc::WEARABLE,
        0,
        "neither is a place among the clothes"
    );
    let item = ObjectId(0x5000_2000);

    let dress = |c: &mut HeadlessClient| {
        already_wearing(c, ObjectId(0x5000_2010), early, early, 0);
        already_wearing(c, ObjectId(0x5000_2011), late, late, 0);
        carrying(c, item, early | late, 1);
    };

    let mut c = a_player_at_the_figure();
    dress(&mut c);
    let (element, ..) = the_place_for(early);
    let said_before = said_so_far(&mut c);
    let sent = let_go_on(&mut c, item, element);
    let said = said_since(&mut c, said_before);
    let the_figure_says_one_thing = sent.len() == 1
        && said.len() == 1
        && said[0].1.starts_with("Moving ")
        && !WIELD_SLOT_ORDER
            .iter()
            .any(|(_, m)| said.iter().any(|(_, t)| t == m));

    let mut c = a_player_at_the_figure();
    dress(&mut c);
    let said_before = said_so_far(&mut c);
    let sent = double_clicked(&mut c, item);
    let said = said_since(&mut c, said_before);
    let the_sort_names_them_all = sent.is_empty()
        && said.iter().map(|(_, t)| t.clone()).collect::<Vec<_>>()
            == vec![early_line.to_string(), late_line.to_string()]
        && c.world_mut().unblock.unblock_attempt_num == 0;

    c.assert_behaviour(
            "inventory.unblock.the-figure-says-only-what-it-is-moving-where-the-sort-names-every-full-place",
            move |_| the_figure_says_one_thing && the_sort_names_them_all,
        );
}

// -----------------------------------------------------------------------------------------
// inventory.unblock.adding-to-a-stack-on-the-body-that-cannot-take-more-says-so-and-moves-nothing
// -----------------------------------------------------------------------------------------

/// Letting ammunition go on the quiver when the quiver already holds the same kind and cannot
/// take any more is not a blocked equip at all: the client says *"You cannot wield more"*,
/// naming them, and moves nothing -- the stack on the body is not taken off to make room for
/// the stack in the hand, because they are the same thing.
///
/// Both ways it can happen are measured -- a kind that will not merge at all, and a stack
/// that is simply full -- and the control is one field apart: a **different** kind of thing
/// in the same place is taken off in the ordinary way.
pub fn adding_to_a_stack_on_the_body_that_cannot_take_more_says_so_and_moves_nothing() {
    let (element, mask, _) = the_place_for(loc::MISSILE_AMMO);
    assert_eq!(
        mask,
        loc::MISSILE_AMMO,
        "the quiver is its own place on the figure"
    );
    const SAME_KIND: u32 = 0xA330_0001;

    // (a) the same kind, and nothing will merge.
    let mut c = a_player_at_the_figure();
    let on_the_body = ObjectId(0x5000_0FF1);
    let in_the_hand = ObjectId(0x5000_0FF0);
    already_wearing(&mut c, on_the_body, loc::MISSILE_AMMO, loc::MISSILE_AMMO, 0);
    c.world_mut()
        .tables
        .weenies
        .get_mut(on_the_body)
        .expect("worn")
        .pwd
        .wcid = SAME_KIND;
    carrying(&mut c, in_the_hand, loc::MISSILE_AMMO, 1);
    {
        let w = c
            .world_mut()
            .tables
            .weenies
            .get_mut(in_the_hand)
            .expect("carried");
        w.pwd.wcid = SAME_KIND;
        w.pwd.name = "Arrow".into();
    }
    let said_before = said_so_far(&mut c);
    let sent = let_go_on(&mut c, in_the_hand, element);
    let said = said_since(&mut c, said_before);
    let says_so_and_moves_nothing = sent.is_empty()
        && c.world_mut().unblock.unblock_attempt_num == 0
        && c.world_mut().unblock.blocking_id.is_none()
        && said
            .iter()
            .any(|(_, t)| t == "You cannot wield more Arrows")
        && c.world_mut().inv_slots.item_at(loc::MISSILE_AMMO) == Some(on_the_body);

    // (b) the same kind, and the stack on the body is simply full.
    let mut c = a_player_at_the_figure();
    already_wearing(&mut c, on_the_body, loc::MISSILE_AMMO, loc::MISSILE_AMMO, 0);
    {
        let w = c
            .world_mut()
            .tables
            .weenies
            .get_mut(on_the_body)
            .expect("worn");
        w.pwd.wcid = SAME_KIND;
        w.pwd.stack_size = Some(10);
        w.pwd.max_stack_size = Some(10);
    }
    carrying(&mut c, in_the_hand, loc::MISSILE_AMMO, 7);
    {
        let w = c
            .world_mut()
            .tables
            .weenies
            .get_mut(in_the_hand)
            .expect("carried");
        w.pwd.wcid = SAME_KIND;
        w.pwd.max_stack_size = Some(10);
        w.pwd.name = "Arrow".into();
    }
    let said_before = said_so_far(&mut c);
    let sent = let_go_on(&mut c, in_the_hand, element);
    let said = said_since(&mut c, said_before);
    let a_full_stack_says_the_same = sent.is_empty()
        && said
            .iter()
            .any(|(_, t)| t == "You cannot wield more Arrows")
        && c.world_mut().inv_slots.item_at(loc::MISSILE_AMMO) == Some(on_the_body)
        && c.world_mut().unblock.unblock_attempt_num == 0;

    // The control: a different kind of thing in the same place is taken off as usual.
    let mut c = a_player_at_the_figure();
    already_wearing(&mut c, on_the_body, loc::MISSILE_AMMO, loc::MISSILE_AMMO, 0);
    c.world_mut()
        .tables
        .weenies
        .get_mut(on_the_body)
        .expect("worn")
        .pwd
        .wcid = SAME_KIND;
    carrying(&mut c, in_the_hand, loc::MISSILE_AMMO, 1);
    c.world_mut()
        .tables
        .weenies
        .get_mut(in_the_hand)
        .expect("carried")
        .pwd
        .wcid = SAME_KIND + 1;
    let sent = let_go_on(&mut c, in_the_hand, element);
    let a_different_kind_is_taken_off = sent.len() == 1
        && sent.iter().filter_map(moved_into).next().map(|(i, _)| i) == Some(on_the_body)
        && c.world_mut().unblock.unblock_attempt_num == 1;

    c.assert_behaviour(
            "inventory.unblock.adding-to-a-stack-on-the-body-that-cannot-take-more-says-so-and-moves-nothing",
            move |_| {
                says_so_and_moves_nothing
                    && a_full_stack_says_the_same
                    && a_different_kind_is_taken_off
            },
        );
}

// -----------------------------------------------------------------------------------------
// Double-clicking something in the pack, which is the other way to put it on.
// -----------------------------------------------------------------------------------------

/// The channel the client's own feedback goes out on -- the bright red one every line the
/// client writes for itself uses, refusal or not.
///
/// Pinned as the number rather than read through the name the client writes it with, because
/// a check that reads a constant through the same name it was written through cannot tell a
/// wrong constant from a right one.
const FEEDBACK_CHANNEL: u32 = 26;

/// A weapon in the player's pack: it can go in the hand and nowhere else, and it is for
/// fighting with, which is what makes a double-click on it a request to ready it.
fn a_weapon_in_the_pack(c: &mut HeadlessClient, item: ObjectId, left_handed: bool) {
    use dereth_client_model::inventory::use_object::use_bitfield;

    carrying(c, item, loc::WEAPON_READY_SLOT, 1);
    let w = c.world_mut().tables.weenies.get_mut(item).expect("carried");
    w.pwd.combat_use = Some(1);
    w.pwd.obj_type = dereth_rules::weenie::item_type::MELEE_WEAPON;
    if left_handed {
        w.pwd.bitfield |= use_bitfield::WIELD_LEFT;
    }
}

/// A weapon in the pack with the hand it wants already full.
fn a_weapon_and_a_full_hand(item: ObjectId, blocker: ObjectId) -> HeadlessClient {
    let mut c = a_player_at_the_figure();
    already_wearing(
        &mut c,
        blocker,
        loc::WEAPON_READY_SLOT,
        loc::WEAPON_READY_SLOT,
        0,
    );
    a_weapon_in_the_pack(&mut c, item, false);
    c
}

// -----------------------------------------------------------------------------------------
// inventory.double-click.a-blocked-one-asks-for-the-same-two-things-as-a-blocked-drop
// -----------------------------------------------------------------------------------------

/// Double-clicking a weapon whose hand is full asks the shard for exactly what letting it go
/// on the hand asks for, in the same order: first the weapon already in the hand goes to the
/// pack, then -- once the shard says it landed there -- the new weapon is readied into the
/// place it left.
///
/// The two stations are asserted apart, because asking only about the second would let a
/// client that sent both at once pass and asking only about the first would let one that
/// never came back pass; and the two gestures are compared against each other, which is the
/// claim. Before this, a double-click on a blocked weapon simply did nothing.
///
/// The premise is asserted first: a weapon for fighting with, in the pack, is a thing a
/// double-click readies at all -- into the right hand, or into the left when the thing itself
/// says so.
pub fn a_blocked_double_click_asks_for_the_same_two_things_as_a_blocked_drop() {
    use dereth_client_model::inventory::use_object::UseResult;

    let item = ObjectId(0x5000_7310);
    let blocker = ObjectId(0x5000_7311);
    let (element, mask, _) = the_place_for(loc::WEAPON_READY_SLOT);
    assert_eq!(
        mask,
        loc::WEAPON_READY_SLOT,
        "the place on the figure a weapon is aimed at"
    );

    {
        let mut c = a_player_at_the_figure();
        a_weapon_in_the_pack(&mut c, item, false);
        assert_eq!(
            c.world_mut().determine_use_result(item),
            UseResult::WieldRight,
            "the premise: a double-click on a weapon in the pack readies it"
        );
        let left = ObjectId(0x5000_7312);
        a_weapon_in_the_pack(&mut c, left, true);
        assert_eq!(
            c.world_mut().determine_use_result(left),
            UseResult::WieldLeft,
            "and into the left hand when the thing itself says left"
        );
    }

    let mut both: Vec<Vec<Request>> = Vec::new();
    let mut each_station_held = true;
    for by_drop in [true, false] {
        let mut c = a_weapon_and_a_full_hand(item, blocker);
        let player = c.world_mut().player.expect("a player");

        let first = if by_drop {
            let_go_on(&mut c, item, element)
        } else {
            double_clicked(&mut c, item)
        };
        let u = c.world_mut().unblock;
        each_station_held &= first.len() == 1
            && first.iter().filter_map(moved_into).collect::<Vec<_>>() == vec![(blocker, player)]
            && u.unblock_attempt_num == 1
            && u.blocked_id == Some(item)
            && u.blocking_id == Some(blocker)
            && c.world_mut().weenie(item).expect("carried").waiting;

        let at = c.outbound().len();
        c.when(the_shard_says_contained(blocker, player));
        let second = c.outbound()[at..].to_vec();
        each_station_held &= second.len() == 1
            && second.iter().filter_map(wield_place).collect::<Vec<_>>()
                == vec![(item, loc::WEAPON_READY_SLOT)]
            && c.world_mut().unblock.unblock_attempt_num == 0;

        let mut all = first;
        all.extend(second);
        both.push(all);
    }
    let the_two_gestures_agree = both[0] == both[1];

    let mut c = a_player_at_the_figure();
    c.assert_behaviour(
        "inventory.double-click.a-blocked-one-asks-for-the-same-two-things-as-a-blocked-drop",
        move |_| each_station_held && the_two_gestures_agree,
    );
}

// -----------------------------------------------------------------------------------------
// inventory.double-click.a-blocked-one-says-only-which-thing-is-being-moved-and-says-it-once
// -----------------------------------------------------------------------------------------

/// A blocked double-click says exactly one thing, and it is the same one thing a blocked drop
/// says: which weapon is being moved to the pack to make room. None of the sentences about
/// already wielding something appear, and the line goes out on the client's own red channel
/// -- which is what makes it look like an error when it is not one.
///
/// Both the absence and the presence are measured, because a test of the request alone would
/// pass on a client that also nagged, and a test of the silence alone would pass on a client
/// that did nothing at all.
pub fn a_blocked_double_click_says_only_which_thing_is_being_moved_and_says_it_once() {
    let item = ObjectId(0x5000_7320);
    let blocker = ObjectId(0x5000_7321);
    let (element, ..) = the_place_for(loc::WEAPON_READY_SLOT);

    let mut lines: Vec<Vec<(u32, String)>> = Vec::new();
    let mut moved_it = true;
    for by_drop in [true, false] {
        let mut c = a_weapon_and_a_full_hand(item, blocker);
        c.world_mut()
            .tables
            .weenies
            .get_mut(blocker)
            .expect("worn")
            .pwd
            .name = "Sword of Lost Light".into();
        let said_before = said_so_far(&mut c);
        let sent = if by_drop {
            let_go_on(&mut c, item, element)
        } else {
            double_clicked(&mut c, item)
        };
        moved_it &= sent.len() == 1 && sent.iter().filter_map(moved_into).count() == 1;
        lines.push(said_since(&mut c, said_before));
    }

    let one_line_each = lines.iter().all(|l| {
        l.len() == 1
            && l[0].0 == FEEDBACK_CHANNEL
            && l[0].1 == "Moving Sword of Lost Light to your backpack"
            && !l.iter().any(|(_, t)| t.contains("already wielding"))
    });
    let the_same_line = lines[0] == lines[1];

    let mut c = a_player_at_the_figure();
    c.assert_behaviour(
            "inventory.double-click.a-blocked-one-says-only-which-thing-is-being-moved-and-says-it-once",
            move |_| moved_it && one_line_each && the_same_line,
        );
}

// -----------------------------------------------------------------------------------------
// inventory.double-click.a-ring-may-go-on-the-free-hand-where-a-drop-on-the-full-one-may-not
// -----------------------------------------------------------------------------------------

/// The one place the two gestures are **meant** to disagree. A ring that fits either hand,
/// with the hand the client tries first already wearing one and the other free: a
/// double-click puts it on the free hand and takes nothing off, while letting it go on the
/// full hand takes that ring off instead, because the player aimed there.
///
/// A client that copied either answer to the other gesture would fail here rather than
/// somewhere it looks like the feature working.
pub fn a_ring_may_go_on_the_free_hand_where_a_drop_on_the_full_one_may_not() {
    use dereth_client_model::inventory::use_object::{use_bitfield, UseResult};

    let ring = ObjectId(0x5000_7340);
    let on_the_right = ObjectId(0x5000_7341);
    let dress = |c: &mut HeadlessClient| {
        // **The full hand must be the one the client tries first**, or the side it is willing
        // to wander to is never consulted and the measurement is about nothing.
        already_wearing(
            c,
            on_the_right,
            loc::FINGER_WEAR_RIGHT,
            loc::FINGER_WEAR_RIGHT,
            0,
        );
        carrying(c, ring, loc::FINGER_WEAR, 1);
        let w = c.world_mut().tables.weenies.get_mut(ring).expect("carried");
        w.pwd.obj_type = dereth_rules::weenie::item_type::JEWELRY;
        w.pwd.bitfield |= use_bitfield::WIELD_ON_USE;
    };

    let mut c = a_player_at_the_figure();
    dress(&mut c);
    assert_eq!(
        c.world_mut().determine_use_result(ring),
        UseResult::WieldRight
    );
    assert_eq!(
        c.world_mut().inventory_mask & loc::FINGER_WEAR,
        loc::FINGER_WEAR_RIGHT,
        "the premise: the hand tried first is the full one and the other is free"
    );
    let sent = double_clicked(&mut c, ring);
    let it_takes_the_free_hand = sent.iter().filter_map(wield_place).collect::<Vec<_>>()
        == vec![(ring, loc::FINGER_WEAR_LEFT)]
        && c.world_mut().unblock.unblock_attempt_num == 0;

    let element = the_place_drawn_for(loc::FINGER_WEAR_RIGHT);
    let mut c = a_player_at_the_figure();
    dress(&mut c);
    let sent = let_go_on(&mut c, ring, element);
    let the_drop_clears_the_hand_aimed_at = sent.len() == 1
        && sent.iter().filter_map(moved_into).next().map(|(i, _)| i) == Some(on_the_right);

    c.assert_behaviour(
            "inventory.double-click.a-ring-may-go-on-the-free-hand-where-a-drop-on-the-full-one-may-not",
            move |_| it_takes_the_free_hand && the_drop_clears_the_hand_aimed_at,
        );
}

// -----------------------------------------------------------------------------------------
// inventory.double-click.something-that-can-be-worn-or-held-falls-through-to-the-hand
// -----------------------------------------------------------------------------------------

/// Double-clicking something that could be worn **or** held, with the place on the body it
/// would be worn in already taken, readies it in the hand instead of giving up: the client
/// tries to wear it, is refused, and goes on to try the hand.
///
/// The hand must really be free, or the second leg has nowhere to go and the measurement is
/// about nothing, so that is asserted as the premise.
pub fn something_that_can_be_worn_or_held_falls_through_to_the_hand() {
    use dereth_client_model::inventory::use_object::UseResult;

    let item = ObjectId(0x5000_7380);
    let blocker = ObjectId(0x5000_7381);
    let mut c = a_player_at_the_figure();
    // The thing in the way can only be worn, so it takes the place among the clothes and
    // leaves the hand alone.
    already_wearing(&mut c, blocker, loc::CHEST_WEAR, loc::CHEST_WEAR, 0x40);
    carrying(&mut c, item, loc::CHEST_WEAR | loc::MELEE_WEAPON, 1);
    {
        let w = c.world_mut().tables.weenies.get_mut(item).expect("carried");
        w.pwd.priority = Some(0x40);
        w.pwd.obj_type = dereth_rules::weenie::item_type::ARMOR;
    }
    assert_eq!(
        c.world_mut().determine_use_result(item),
        UseResult::AutoSort,
        "the premise: a double-click on this finds it a place rather than readying it"
    );
    assert_eq!(
        c.world_mut().inventory_mask & loc::WEAPON_READY_SLOT,
        0,
        "and the premise: the hand is free, or there is nowhere to fall through to"
    );

    let sent = double_clicked(&mut c, item);
    let it_goes_in_the_hand = sent.iter().filter_map(wield_place).collect::<Vec<_>>()
        == vec![(item, loc::MELEE_WEAPON)]
        && c.world_mut().unblock.unblock_attempt_num == 0;

    c.assert_behaviour(
        "inventory.double-click.something-that-can-be-worn-or-held-falls-through-to-the-hand",
        move |_| it_goes_in_the_hand,
    );
}

// -----------------------------------------------------------------------------------------
// inventory.double-click.a-thing-that-cannot-be-held-in-a-fight-says-so
// -----------------------------------------------------------------------------------------

/// A torch cannot be held while the player has a weapon out, and double-clicking one says so
/// by name and asks the shard for nothing. The double-click's own refusals are spoken; the
/// way of putting things on that a shortcut uses swallows them, and that is a different
/// silence from the one a blocked drop keeps.
pub fn a_thing_that_cannot_be_held_in_a_fight_says_so() {
    use dereth_client_model::inventory::use_object::UseResult;

    let torch = ObjectId(0x5000_7370);
    let mut c = a_player_at_the_figure();
    carrying(&mut c, torch, loc::HELD, 1);
    {
        let w = c
            .world_mut()
            .tables
            .weenies
            .get_mut(torch)
            .expect("carried");
        w.pwd.combat_use = Some(1);
        w.pwd.obj_type = dereth_rules::weenie::item_type::MELEE_WEAPON;
        w.pwd.name = "Torch".into();
    }
    c.world_mut().combat.combat_mode = dereth_client_model::combat::CombatMode::Melee;
    assert_eq!(
        c.world_mut().determine_use_result(torch),
        UseResult::WieldRight
    );

    let said_before = said_so_far(&mut c);
    let sent = double_clicked(&mut c, torch);
    let said = said_since(&mut c, said_before);
    let refused_by_name = sent.is_empty()
        && said
            == vec![(
                FEEDBACK_CHANNEL,
                "Cannot hold Torch while in combat".to_string(),
            )];

    c.assert_behaviour(
        "inventory.double-click.a-thing-that-cannot-be-held-in-a-fight-says-so",
        move |_| refused_by_name,
    );
}

// -----------------------------------------------------------------------------------------
// inventory.double-click.a-piece-of-armour-whose-place-is-taken-is-refused-out-loud
// -----------------------------------------------------------------------------------------

/// Double-clicking a piece of armour whose place on the body is taken refuses **out loud**,
/// naming the piece that has to come off, and asks the shard for nothing: nothing is moved
/// out of the way for it, unlike a weapon whose hand is full.
///
/// Without the line the gesture is indistinguishable from a dead click, which is what it was.
pub fn a_piece_of_armour_whose_place_is_taken_is_refused_out_loud() {
    use dereth_client_model::inventory::use_object::UseResult;

    let armour = ObjectId(0x5000_7350);
    let blocker = ObjectId(0x5000_7351);
    let mut c = a_player_at_the_figure();
    already_wearing(&mut c, blocker, loc::CHEST_ARMOR, loc::CHEST_ARMOR, 0x40);
    c.world_mut()
        .tables
        .weenies
        .get_mut(blocker)
        .expect("worn")
        .pwd
        .name = "Studded Leather Breastplate".into();
    carrying(&mut c, armour, loc::CHEST_ARMOR, 1);
    {
        let w = c
            .world_mut()
            .tables
            .weenies
            .get_mut(armour)
            .expect("carried");
        w.pwd.priority = Some(0x40);
        w.pwd.obj_type = dereth_rules::weenie::item_type::ARMOR;
    }
    assert_eq!(
        c.world_mut().determine_use_result(armour),
        UseResult::AutoSort,
        "the premise: a double-click on armour finds it a place"
    );

    let said_before = said_so_far(&mut c);
    let sent = double_clicked(&mut c, armour);
    let said = said_since(&mut c, said_before);
    let refused_out_loud = sent.is_empty()
        && said
            == vec![(
                FEEDBACK_CHANNEL,
                "You must remove your Studded Leather Breastplate to wear that".to_string(),
            )]
        && c.world_mut().unblock.unblock_attempt_num == 0;

    c.assert_behaviour(
        "inventory.double-click.a-piece-of-armour-whose-place-is-taken-is-refused-out-loud",
        move |_| refused_out_loud,
    );
}

// -----------------------------------------------------------------------------------------
// inventory.double-click.every-recorded-one-answers-as-the-drop-does-wherever-they-share-a-path
// -----------------------------------------------------------------------------------------

/// Every thing the recordings record being put on, staged with its own place already taken
/// and then both double-clicked and let go on that place, partitioned by what the client
/// decides a double-click on it means.
///
/// The partition is the point: where a double-click readies a thing it does exactly what the
/// drop does and the two put the same bytes on the wire, except for the paired hands, where
/// they are meant to disagree; and where a double-click instead looks for a free place, it
/// asks for nothing at all, because that way of putting something on never moves anything out
/// of the way. Both figures are asserted, and the denominator first, so an empty corpus
/// cannot read as a clean sweep.
pub fn every_recorded_one_answers_as_the_drop_does_wherever_they_share_a_path() {
    use dereth_client_model::inventory::use_object::UseResult;

    let all = all_recorded_things();
    let (mut readied, mut agreed) = (0usize, 0usize);
    let (mut sorted, mut sent_nothing) = (0usize, 0usize);

    for (i, t) in all.iter().enumerate() {
        let blocker = ObjectId(0xB730_0000 + u32::try_from(i).expect("small"));
        let (element, mask, _) = the_place_for(t.places);

        let means = {
            let mut probe = a_player_at_the_figure();
            carrying_the_recorded(&mut probe, t);
            probe.world_mut().determine_use_result(t.item)
        };

        let mut runs: Vec<Vec<Request>> = Vec::new();
        for by_drop in [true, false] {
            let mut c = a_player_at_the_figure();
            already_wearing(&mut c, blocker, t.places, t.places, t.priority);
            carrying_the_recorded(&mut c, t);
            runs.push(if by_drop {
                let_go_on(&mut c, t.item, element)
            } else {
                double_clicked(&mut c, t.item)
            });
        }

        match means {
            UseResult::WieldRight | UseResult::WieldLeft => {
                readied += 1;
                // The two differ only over which hand of a pair they may wander to, so the
                // paired places are left out of the equality and the rest is required to
                // match byte for byte.
                let paired = mask & (loc::WRIST_WEAR | loc::FINGER_WEAR) != 0;
                if (paired || runs[0] == runs[1])
                    && runs[1].len() == 1
                    && runs[1].iter().filter_map(moved_into).count() == 1
                {
                    agreed += 1;
                }
            }
            _ => {
                sorted += 1;
                if runs[1].is_empty() {
                    sent_nothing += 1;
                }
            }
        }
    }

    assert_eq!(
        readied + sorted,
        all.len(),
        "every recorded equip took exactly one path"
    );
    assert!(readied > 0, "a zero denominator is not a clean sweep");
    assert!(sorted > 0, "and both paths must be exercised");

    let mut c = a_player_at_the_figure();
    c.assert_behaviour(
            "inventory.double-click.every-recorded-one-answers-as-the-drop-does-wherever-they-share-a-path",
            move |_| agreed == readied && sent_nothing == sorted,
        );
}

// -----------------------------------------------------------------------------------------
// The weapons that cannot share the hands, on a whole client and a real drag.
//
// These four are about what the player sees happen when they drag a bow onto the weapon place
// with a shield already on, so they are driven by a real pointer over the shipped figure rather
// than by the request the figure raises: a claim about the drag has to cross the hit test, or
// it cannot tell a wired path from one whose place the player cannot reach.
//
// Every list of places, every use in a fight and every kind of ammunition below is read out
// of a recording rather than written here; the two-handed weapon and the equipped stack of
// ammunition are built, because no recording carries one, and each is built out of exactly
// the fields the client reads.
// -----------------------------------------------------------------------------------------

/// The place on the figure the quiver is drawn in.
const DOLL_AMMO_PLACE: ElementId = ElementId(0x1000_01E0);

const ARMOURY_PLAYER: ObjectId = ObjectId(0x5000_0001);
const BOW: ObjectId = ObjectId(0x5000_0021);
const KITE_SHIELD: ObjectId = ObjectId(0x5000_0022);
const SPEAR: ObjectId = ObjectId(0x5000_0023);
const SWORD: ObjectId = ObjectId(0x5000_0024);
const ARROWS_IN_THE_PACK: ObjectId = ObjectId(0x5000_0025);
const ARROWS_ON_THE_BODY: ObjectId = ObjectId(0x5000_0026);
const KNIVES_IN_THE_PACK: ObjectId = ObjectId(0x5000_0027);
const KNIVES_ON_THE_BODY: ObjectId = ObjectId(0x5000_0028);

/// One kind of thing a recording described: everything the client reads when it decides
/// whether this thing can share the hands with that one.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct Kind {
    wcid: u32,
    places: u32,
    combat_use: u8,
    ammo_type: u16,
    obj_type: u32,
}

/// Every kind of thing the recordings describe that can go in a hand at all.
fn recorded_kinds() -> Vec<Kind> {
    let mut out: Vec<Kind> = Vec::new();
    for corpus in Corpus::load_all() {
        for b in corpus
            .blobs
            .iter()
            .filter(|b| b.dir == Direction::ServerToClient)
        {
            if b.opcode != 0xF745 {
                continue;
            }
            let Ok(m) = dereth_protocol::read_body_padded::<
                dereth_protocol::objects::ItemCreateObject,
            >(b.payload.get(4..).unwrap_or_default()) else {
                continue;
            };
            let w = &m.0.wdesc;
            let Some(places) = w.valid_locations else {
                continue;
            };
            if places & loc::WEAPON_READY_SLOT == 0 && places & loc::SHIELD == 0 {
                continue;
            }
            let k = Kind {
                wcid: w.wcid,
                places,
                combat_use: w.combat_use.unwrap_or(0),
                ammo_type: w.ammo_type.unwrap_or(0),
                obj_type: w.obj_type,
            };
            if !out.contains(&k) {
                out.push(k);
            }
        }
    }
    assert!(
        !out.is_empty(),
        "the recordings describe nothing that can go in a hand"
    );
    out
}

/// The first recorded kind matching `f`, or a panic naming what the recordings do hold -- an
/// instrument that cannot look must not report absence.
fn recorded_kind(what: &str, f: impl Fn(&Kind) -> bool) -> Kind {
    let all = recorded_kinds();
    *all.iter()
        .find(|k| f(k))
        .unwrap_or_else(|| panic!("the recordings describe no {what}; they hold {all:#?}"))
}

/// A player carrying a bow, a shield, a spear, a sword and two stacks, with two more stacks
/// to be put on the body by the shard.
fn seed_the_armoury(w: &mut dereth_client_model::World) {
    use dereth_client_model::combat::combat_use;

    let launcher = recorded_kind("thing that fires ammunition", |k| {
        k.combat_use == combat_use::MISSILE && k.ammo_type != 0
    });
    assert_eq!(
        launcher.places,
        loc::MISSILE_WEAPON,
        "a recorded launcher can go in the one place and nowhere else"
    );
    let shield = recorded_kind("shield", |k| k.places == loc::SHIELD);
    assert_eq!(
        shield.places & loc::WEAPON_READY_SLOT,
        0,
        "the premise the whole story turns on: the shield is not one of the places a weapon \
             goes, so looking only at the weapon places cannot see a shield that is on"
    );
    let melee = recorded_kind("one-handed weapon for close fighting", |k| {
        k.places == loc::MELEE_WEAPON && k.combat_use == combat_use::MELEE
    });
    assert_eq!(melee.ammo_type, 0);

    // Built, because the recordings carry neither: a stack of ammunition on the body, which
    // is the launcher's kind in the quiver; and a weapon that needs both hands, which is the
    // recorded close-fighting weapon's kind with the one field the client reads changed.
    let ammo = Kind {
        places: loc::MISSILE_AMMO,
        ..launcher
    };
    let two_handed = Kind {
        places: loc::TWO_HANDED,
        combat_use: combat_use::TWO_HANDED,
        ammo_type: 0,
        ..melee
    };

    w.player = Some(ARMOURY_PLAYER);
    w.tables.inventories.insert(
        ARMOURY_PLAYER,
        dereth_client_model::objects::ObjectInventory::new(ARMOURY_PLAYER),
    );
    let mut me = dereth_client_model::Weenie::new(ARMOURY_PLAYER);
    me.valid = true;
    me.pwd.items_capacity = Some(102);
    me.pwd.containers_capacity = Some(7);
    me.pwd.bitfield |= dereth_rules::weenie::bitfield::OPENABLE;
    w.tables.weenies.insert(ARMOURY_PLAYER, me);

    for (i, (id, kind, name)) in [
        (BOW, launcher, "Bow"),
        (KITE_SHIELD, shield, "Kite Shield"),
        (SPEAR, two_handed, "Spear"),
        (SWORD, melee, "Sword"),
        (ARROWS_IN_THE_PACK, ammo, "Arrow"),
        (ARROWS_ON_THE_BODY, ammo, "Arrow"),
        (KNIVES_IN_THE_PACK, melee, "Throwing Knife"),
        (KNIVES_ON_THE_BODY, melee, "Throwing Knife"),
    ]
    .into_iter()
    .enumerate()
    {
        let mut wn = dereth_client_model::Weenie::new(id);
        wn.valid = true;
        wn.pwd.name = name.into();
        wn.pwd.wcid = kind.wcid;
        wn.pwd.valid_locations = Some(kind.places);
        wn.pwd.combat_use = Some(kind.combat_use);
        wn.pwd.ammo_type = Some(kind.ammo_type);
        wn.pwd.obj_type = kind.obj_type;
        wn.pwd.container_id = Some(ARMOURY_PLAYER);
        let stacked = matches!(
            id,
            ARROWS_IN_THE_PACK | ARROWS_ON_THE_BODY | KNIVES_IN_THE_PACK | KNIVES_ON_THE_BODY
        );
        wn.pwd.stack_size = Some(match id {
            ARROWS_IN_THE_PACK | KNIVES_IN_THE_PACK => 7,
            ARROWS_ON_THE_BODY | KNIVES_ON_THE_BODY => 8,
            _ => 1,
        });
        wn.pwd.max_stack_size = Some(if stacked { 10 } else { 1 });
        w.tables.weenies.insert(id, wn);
        w.tables
            .inventories
            .get_mut(ARMOURY_PLAYER)
            .expect("seeded")
            .add_content(id, false, i);
    }
    w.remake_character_inventory();
}

/// A whole client with the pack open, the figure live, and the armoury in the pack.
fn a_player_with_an_armoury() -> HeadlessClient {
    let mut c = HeadlessClient::new(ClientSpec::gameplay_in_world(4));
    seed_the_armoury(&mut c.app_mut().probe_mut().objects_mut().world);
    open_pack(&mut c);
    c.tick(2);
    c
}

/// Put `item` on the body at `place`, the way the shard does.
fn the_shard_puts_it_on(c: &mut HeadlessClient, item: ObjectId, place: u32) {
    c.when(the_shard_says_worn(item, place)).tick(1);
    assert_eq!(
        c.app_mut()
            .probe_mut()
            .objects_mut()
            .world
            .inv_slots
            .item_at(place),
        Some(item),
        "the premise: the shard's own answer put {item:?} on the body"
    );
}

/// What the client has asked the shard for since `from`, with the two questions a pointer
/// crossing a pack slot asks about the thing under it dropped: they are raised by the hover
/// and have nothing to do with putting anything on.
fn asked_since(c: &HeadlessClient, from: usize) -> Vec<Request> {
    c.outbound()[from..]
        .iter()
        .filter(|r| !matches!(r, Request::Appraise(_) | Request::QueryItemMana(_)))
        .cloned()
        .collect()
}

/// Drag `item` out of the pack and let it go on the place on the figure drawn at `place`,
/// with the pointer really over it, and answer with what the client asked the shard for.
fn drag_onto_the_body(c: &mut HeadlessClient, item: ObjectId, place: ElementId) -> Vec<Request> {
    let from = pack_slot(c, item);
    let to = doll_tile(c, place);
    let start = point_of(c, from);
    let from_here = c.outbound().len();
    c.when(Grab(start));
    let at = carry_over(c, to);
    c.when(LetGo(at));
    c.tick(3);
    clear_requests(c.ui_outbox());
    asked_since(c, from_here)
}

// -----------------------------------------------------------------------------------------
// inventory.equip.a-stack-let-go-on-a-matching-one-on-the-body-merges-what-fits
// -----------------------------------------------------------------------------------------

/// Dragging a stack onto a place on the body where the same kind of thing is already worn
/// adds to it rather than taking it off: the client asks for as many as will fit -- two of
/// the seven in hand, into the eight of a maximum ten -- and asks for nothing else.
///
/// Both places that can hold a stack are measured, the quiver and the hand, because they are
/// two different tests in the client and a build that got one right could get the other
/// wrong.
pub fn a_stack_let_go_on_a_matching_one_on_the_body_merges_what_fits() {
    let mut c = a_player_with_an_armoury();
    the_shard_puts_it_on(&mut c, ARROWS_ON_THE_BODY, loc::MISSILE_AMMO);
    let sent = drag_onto_the_body(&mut c, ARROWS_IN_THE_PACK, DOLL_AMMO_PLACE);
    let the_quiver_takes_two = matches!(
        sent.as_slice(),
        [Request::StackableMerge(m)]
            if m.merge_from == ARROWS_IN_THE_PACK
                && m.merge_to == ARROWS_ON_THE_BODY
                && m.amount == 2
    );
    let and_nothing_came_off = c
        .app_mut()
        .probe_mut()
        .objects_mut()
        .world
        .inv_slots
        .item_at(loc::MISSILE_AMMO)
        == Some(ARROWS_ON_THE_BODY);
    c.shutdown();

    let mut c = a_player_with_an_armoury();
    the_shard_puts_it_on(&mut c, KNIVES_ON_THE_BODY, loc::MELEE_WEAPON);
    let sent = drag_onto_the_body(&mut c, KNIVES_IN_THE_PACK, DOLL_WEAPON_SLOT);
    let the_hand_takes_two = matches!(
        sent.as_slice(),
        [Request::StackableMerge(m)]
            if m.merge_from == KNIVES_IN_THE_PACK
                && m.merge_to == KNIVES_ON_THE_BODY
                && m.amount == 2
    );

    c.assert_behaviour(
        "inventory.equip.a-stack-let-go-on-a-matching-one-on-the-body-merges-what-fits",
        move |_| the_quiver_takes_two && and_nothing_came_off && the_hand_takes_two,
    );
    c.shutdown();
}

// -----------------------------------------------------------------------------------------
// inventory.equip.the-name-in-that-refusal-is-pluralised-by-the-clients-own-simple-rule
// -----------------------------------------------------------------------------------------

/// When the shard never sent a plural name for a thing, the client makes one by adding a
/// single letter, and it treats only a name already ending in that letter specially -- so a
/// thing called a Lockpix is refused as *"Lockpixs"* and not as *"Lockpixes"*.
///
/// Measured on the same real drag as the ordinary refusal, so the wording reaches the strip
/// the player reads rather than a sink.
pub fn the_name_in_that_refusal_is_pluralised_by_the_clients_own_simple_rule() {
    let mut c = a_player_with_an_armoury();
    {
        let w = c
            .app_mut()
            .probe_mut()
            .objects_mut()
            .world
            .weenie_mut(ARROWS_IN_THE_PACK)
            .expect("seeded");
        w.pwd.name = "Lockpix".into();
        w.pwd.plural_name = None;
    }
    c.app_mut()
        .probe_mut()
        .objects_mut()
        .world
        .weenie_mut(ARROWS_ON_THE_BODY)
        .expect("seeded")
        .pwd
        .stack_size = Some(10);
    the_shard_puts_it_on(&mut c, ARROWS_ON_THE_BODY, loc::MISSILE_AMMO);

    let before = strip_lines(&mut c).len();
    let sent = drag_onto_the_body(&mut c, ARROWS_IN_THE_PACK, DOLL_AMMO_PLACE);
    let said: Vec<String> = strip_lines(&mut c).into_iter().skip(before).collect();

    let one_letter_added = sent.is_empty()
        && said.iter().any(|t| t == "You cannot wield more Lockpixs")
        && !said.iter().any(|t| t.contains("Lockpixes"));
    let and_the_stack_on_the_body_is_untouched = c
        .app_mut()
        .probe_mut()
        .objects_mut()
        .world
        .inv_slots
        .item_at(loc::MISSILE_AMMO)
        == Some(ARROWS_ON_THE_BODY);

    c.assert_behaviour(
        "inventory.equip.the-name-in-that-refusal-is-pluralised-by-the-clients-own-simple-rule",
        move |_| one_letter_added && and_the_stack_on_the_body_is_untouched,
    );
    c.shutdown();
}

// -----------------------------------------------------------------------------------------
// inventory.equip.a-weapon-that-cannot-share-the-hands-takes-what-is-in-the-way-off-first
// -----------------------------------------------------------------------------------------

/// Readying a weapon that cannot share the hands with what is already held takes the thing in
/// the way off first, and only asks for the weapon once the shard says it landed in the pack.
///
/// Three cases, each a real drag onto the weapon place, and they are one story: a bow with a
/// shield on and an empty hand; a spear that needs both hands with a shield on; and a bow
/// with a sword already in the hand. The first two are the failure this guards against -- a
/// client that sends the ask on its own has it silently dropped by the shard, so the weapon
/// never appears.
///
/// The fourth case is the control that stops the fix from over-applying: a one-handed sword
/// beside a shield is the one pairing that is allowed, and it goes straight out with the
/// shield left where it is.
pub fn a_weapon_that_cannot_share_the_hands_takes_what_is_in_the_way_off_first() {
    let mut held = true;
    for (weapon, in_the_way, its_place, asked_for) in [
        (BOW, KITE_SHIELD, loc::SHIELD, loc::MISSILE_WEAPON),
        (SPEAR, KITE_SHIELD, loc::SHIELD, loc::TWO_HANDED),
        (BOW, SWORD, loc::MELEE_WEAPON, loc::MISSILE_WEAPON),
    ] {
        let mut c = a_player_with_an_armoury();
        the_shard_puts_it_on(&mut c, in_the_way, its_place);
        if its_place == loc::SHIELD {
            assert_eq!(
                c.app_mut().probe_mut().objects_mut().world.inventory_mask & loc::WEAPON_READY_SLOT,
                0,
                "the control: the hand a weapon goes in is empty, so only the shield can be \
                     in the way"
            );
        }

        let before = strip_lines(&mut c).len();
        let sent = drag_onto_the_body(&mut c, weapon, DOLL_WEAPON_SLOT);
        let said: Vec<String> = strip_lines(&mut c).into_iter().skip(before).collect();
        held &= sent.len() == 1
            && sent.iter().filter_map(moved_into).collect::<Vec<_>>()
                == vec![(in_the_way, ARMOURY_PLAYER)]
            && said.iter().any(|t| {
                t == &format!(
                    "Moving {} to your backpack",
                    c.app_mut()
                        .probe_mut()
                        .objects_mut()
                        .world
                        .weenie(in_the_way)
                        .expect("worn")
                        .pwd
                        .name
                )
            });

        let at = c.outbound().len();
        c.when(the_shard_says_contained(in_the_way, ARMOURY_PLAYER))
            .tick(2);
        held &= asked_since(&c, at)
            .iter()
            .filter_map(wield_place)
            .collect::<Vec<_>>()
            == vec![(weapon, asked_for)];
        c.shutdown();
    }

    // The control: the one pairing that is allowed goes straight out.
    let mut c = a_player_with_an_armoury();
    the_shard_puts_it_on(&mut c, KITE_SHIELD, loc::SHIELD);
    let sent = drag_onto_the_body(&mut c, SWORD, DOLL_WEAPON_SLOT);
    let a_sword_beside_a_shield_is_allowed = sent.len() == 1
        && sent.iter().filter_map(wield_place).collect::<Vec<_>>()
            == vec![(SWORD, loc::MELEE_WEAPON)]
        && c.app_mut()
            .probe_mut()
            .objects_mut()
            .world
            .inv_slots
            .item_at(loc::SHIELD)
            == Some(KITE_SHIELD);

    c.assert_behaviour(
        "inventory.equip.a-weapon-that-cannot-share-the-hands-takes-what-is-in-the-way-off-first",
        move |_| held && a_sword_beside_a_shield_is_allowed,
    );
    c.shutdown();
}

// -----------------------------------------------------------------------------------------
// inventory.equip.a-shield-is-refused-in-words-while-a-weapon-for-both-hands-is-held
// -----------------------------------------------------------------------------------------

/// The same conflict the other way round is answered differently: a shield dragged onto the
/// shield place while a weapon that needs both hands is held is **refused**, in a sentence
/// naming the weapon, and nothing at all is asked of the shard -- the weapon is not taken off
/// to make room for the shield.
///
/// The asymmetry is the claim: one direction moves the thing in the way, the other says no.
pub fn a_shield_is_refused_in_words_while_a_weapon_for_both_hands_is_held() {
    let mut c = a_player_with_an_armoury();
    the_shard_puts_it_on(&mut c, SPEAR, loc::TWO_HANDED);

    let before = strip_lines(&mut c).len();
    let sent = drag_onto_the_body(&mut c, KITE_SHIELD, DOLL_SHIELD_SLOT);
    let said: Vec<String> = strip_lines(&mut c).into_iter().skip(before).collect();

    let refused_in_words = sent.is_empty()
        && said
            .iter()
            .any(|t| t == "A shield may not be worn with the Spear");
    let and_the_weapon_stays = c
        .app_mut()
        .probe_mut()
        .objects_mut()
        .world
        .inv_slots
        .item_at(loc::TWO_HANDED)
        == Some(SPEAR);

    c.assert_behaviour(
        "inventory.equip.a-shield-is-refused-in-words-while-a-weapon-for-both-hands-is-held",
        move |_| refused_in_words && and_the_weapon_stays,
    );
    c.shutdown();
}

// -----------------------------------------------------------------------------------------
// Eight things the client does, each reached through the gesture that asks for it.
//
// Every one of these has the same shape: make the gesture, and look at the far end of the
// chain. Where it is affordable the answer is read off the world rather than off the request,
// because a handler that raises a request nothing routes is not a feature.
// -----------------------------------------------------------------------------------------

/// A whole client with the screens up and nothing in its world.
fn a_client_in_the_game() -> HeadlessClient {
    HeadlessClient::new(ClientSpec::gameplay(4))
}

// -----------------------------------------------------------------------------------------
// inventory.shortcut-bar.the-other-key-for-a-tile-selects-what-is-in-it-instead-of-using-it
// -----------------------------------------------------------------------------------------

/// Each tile of the shortcut bar answers two keys, and they do different things: one uses
/// what is in the tile, and the other **selects** it, so that the next thing the player does
/// acts on it. The whole chain is the claim -- the key, the broadcast every listener on the
/// screen hears, the bar's own listener and the selection -- and it is read off the world,
/// which no handler that only raises a request could satisfy.
///
/// The thing in the tile comes from the description the shard sends at login, which is where
/// a player's bar comes from.
pub fn the_other_key_for_a_tile_selects_what_is_in_it_instead_of_using_it() {
    const SLOT: i32 = 3;
    let mut c = a_player_with_an_armoury();
    c.when(Inbound::event(
        dereth_client_net::client_session::SessionEvent::PlayerDescription(Box::new(
            dereth_protocol::login::LoginPlayerDescription {
                player_module: dereth_protocol::login::PlayerModule {
                    shortcuts: Some(vec![dereth_protocol::login::ShortCutData {
                        index: SLOT,
                        object_id: BOW,
                        spell_id: 0,
                    }]),
                    ..dereth_protocol::login::PlayerModule::default()
                },
                ..dereth_protocol::login::LoginPlayerDescription::default()
            },
        )),
    ))
    .tick(2);
    {
        let (_ui, screen) = gameplay_screen(c.app_mut());
        assert_eq!(
            screen
                .shortcuts
                .item_at(u32::try_from(SLOT).expect("a slot number")),
            Some(BOW),
            "the premise: the bar is holding the thing the key is about to act on"
        );
    }
    c.app_mut().probe_mut().objects_mut().world.selected = None;
    c.tick(1);
    let nothing_selected_yet = c
        .app_mut()
        .probe_mut()
        .objects_mut()
        .world
        .selected
        .is_none();

    let presses_before = c.app_mut().ui().expect("a shell").stats.key_presses;
    c.when(Player::Press(dereth_input::ActionId(
        0x1000_004E + u32::try_from(SLOT).expect("a slot number"),
    )));
    c.tick(1);

    let stats = c.app_mut().ui().expect("a shell").stats;
    let the_key_reached_the_screens =
        stats.key_presses > presses_before && stats.key_presses_broadcast > 0;
    let and_that_tiles_own_thing_is_selected =
        c.app_mut().probe_mut().objects_mut().world.selected == Some(BOW);

    c.assert_behaviour(
        "inventory.shortcut-bar.the-other-key-for-a-tile-selects-what-is-in-it-instead-of-using-it",
        move |_| {
            nothing_selected_yet
                && the_key_reached_the_screens
                && and_that_tiles_own_thing_is_selected
        },
    );
    c.shutdown();
}

// -----------------------------------------------------------------------------------------
// ui.hot-key.the-key-that-hides-the-interface-hides-it
// -----------------------------------------------------------------------------------------

/// The key that hides the whole interface hides it: the game screen stops drawing and its own
/// root goes invisible with it. It is the other half of the same broadcast the shortcut keys
/// ride on, and the one arm of the game screen's own key handling with a visible effect that
/// does not end the session.
pub fn the_key_that_hides_the_interface_hides_it() {
    let mut c = a_client_in_the_game();
    let up_first = {
        let (_ui, screen) = gameplay_screen(c.app_mut());
        screen.shown
    };
    assert!(up_first, "the premise: the interface is up");

    c.when(Player::Press(dereth_input::ActionId(0x54)));
    c.tick(1);

    let (ui, screen) = gameplay_screen(c.app_mut());
    let hidden = !screen.shown;
    let root = screen.root().expect("the game screen has a root");
    let and_the_root_with_it = !ui.node(root).expect("alive").region.flags.visible;

    c.assert_behaviour(
        "ui.hot-key.the-key-that-hides-the-interface-hides-it",
        move |_| up_first && hidden && and_the_root_with_it,
    );
    c.shutdown();
}

// -----------------------------------------------------------------------------------------
// shell.logout.the-bound-key-asks-to-leave-and-the-player-stays-until-the-shard-answers
// -----------------------------------------------------------------------------------------

/// Pressing the key that leaves for the character list asks to leave and **leaves the player
/// standing in the world**: the client queues no screen of its own, because what ends the
/// session is the shard's own answer arriving later. The whole chain is the claim, and it
/// starts with the shipped keymap really binding a key to that action -- if it binds none,
/// leaving by the keyboard is impossible for a reason no amount of wiring fixes.
///
/// The other half is asserted with it: this is the arm that goes to the character list and
/// not the one that quits, so the epilogue is never reached.
pub fn the_bound_key_asks_to_leave_and_the_player_stays_until_the_shard_answers() {
    const LOGOUT_TO_THE_CHARACTER_LIST: u32 = 0x1000_0026;
    let mut c = a_client_in_the_game();

    let bound = c
        .app_mut()
        .input_manager_mut()
        .expect("an input manager")
        .manager
        .keymap
        .sections
        .iter()
        .flat_map(|s| s.bindings().iter())
        .filter(|(_, a)| a.0 == LOGOUT_TO_THE_CHARACTER_LIST)
        .count();
    assert!(
        bound > 0,
        "the shipped keymap binds a key to leaving for the character list"
    );
    assert!(
        !c.app_mut().teleport().log_off_pending(),
        "the premise: nothing has been asked yet"
    );

    c.when(Player::Press(dereth_input::ActionId(
        LOGOUT_TO_THE_CHARACTER_LIST,
    )));
    c.tick(1);
    let the_screen_took_it = {
        let (_ui, screen) = gameplay_screen(c.app_mut());
        screen.logout_confirmed && !screen.should_quit_on_logout
    };
    // The next frame is where the screen acts on what it recorded.
    c.tick(1);
    let it_asked_to_leave = c.app_mut().teleport().log_off_pending();
    let and_the_player_is_still_in_the_world =
        c.app_mut().ui().expect("a shell").flow.current_mode()
            == Some(dereth_ui::framework::mode::GAME_PLAY)
            && c.app_mut().ui().expect("a shell").flow.current_mode()
                != Some(dereth_ui::framework::mode::EPILOGUE);

    c.assert_behaviour(
        "shell.logout.the-bound-key-asks-to-leave-and-the-player-stays-until-the-shard-answers",
        move |_| {
            bound > 0
                && the_screen_took_it
                && it_asked_to_leave
                && and_the_player_is_still_in_the_world
        },
    );
    c.shutdown();
}

// -----------------------------------------------------------------------------------------
// inventory.pack.a-press-on-a-slot-selects-examines-or-uses-the-thing-in-that-slot
// -----------------------------------------------------------------------------------------

/// A press on a slot of the pack does one of three different things depending on how it was
/// pressed -- select it, look at it, or use it -- and every one of them names **the thing in
/// the slot that was pressed**. Using or examining the wrong thing is invisible and
/// destructive, so which thing is the claim and not how many requests there were.
///
/// A slot with nothing in it does nothing at all, which is not the same as deselecting.
pub fn a_press_on_a_slot_selects_examines_or_uses_the_thing_in_that_slot() {
    /// The three ways the list reports a press: select, select and look, and use.
    const SELECT: u32 = 7;
    const SELECT_AND_LOOK: u32 = 8;
    const USE: u32 = 0x0A;

    let mut c = a_player_with_an_armoury();
    let slot = pack_slot(&mut c, BOW);
    let other_slot = pack_slot(&mut c, SWORD);

    let mut each_press_named_its_own = true;
    for (how, want) in [
        (SELECT, vec![UiRequest::Select(BOW)]),
        (
            SELECT_AND_LOOK,
            vec![UiRequest::Select(BOW), UiRequest::Examine(BOW)],
        ),
        (USE, vec![UiRequest::Use(BOW)]),
    ] {
        clear_requests(c.ui_outbox());
        let (ui, screen) = gameplay_screen(c.app_mut());
        screen.on_item_list_press(ui, slot, how);
        each_press_named_its_own &= super::take_requests(&mut ui.requests) == want;
    }

    // The same press, delivered as the message a real click raises, and followed to the far
    // end: what the world records as selected must be the thing in the slot pressed.
    c.app_mut().probe_mut().objects_mut().world.selected = Some(BOW);
    {
        let (ui, _screen) = gameplay_screen(c.app_mut());
        ui.broadcast_element_message(
            other_slot,
            dereth_ui::msg::element::id::MOUSE_PRESS,
            SELECT,
            0,
        );
    }
    c.tick(1);
    let the_slot_pressed_and_not_the_last_one =
        c.app_mut().probe_mut().objects_mut().world.selected == Some(SWORD);

    let empty = {
        let (_ui, screen) = gameplay_screen(c.app_mut());
        screen
            .inventory
            .item_list
            .as_ref()
            .expect("the pack grid")
            .slots
            .iter()
            .find(|s| s.item.is_none())
            .expect("an empty slot")
            .handle
    };
    clear_requests(c.ui_outbox());
    {
        let (ui, screen) = gameplay_screen(c.app_mut());
        screen.on_item_list_press(ui, empty, SELECT);
        screen.on_item_list_press(ui, empty, USE);
    }
    let an_empty_slot_does_nothing = super::take_requests(c.ui_outbox()).is_empty();

    c.assert_behaviour(
        "inventory.pack.a-press-on-a-slot-selects-examines-or-uses-the-thing-in-that-slot",
        move |_| {
            each_press_named_its_own
                && the_slot_pressed_and_not_the_last_one
                && an_empty_slot_does_nothing
        },
    );
    c.shutdown();
}

// -----------------------------------------------------------------------------------------
// use.button.the-two-buttons-act-on-what-is-selected-or-arm-the-pointer-when-nothing-is
// -----------------------------------------------------------------------------------------

/// The Use and Examine buttons on the toolbar each do one of two things depending on whether
/// anything is selected: with a selection they act on it, and with none they arm the pointer,
/// so that the next thing clicked is what gets used or looked at. A button that is neither of
/// them does nothing.
///
/// The far end is asserted for a real click: the armed pointer has to reach the part of the
/// client the next click reads, which is the only thing that writes it.
pub fn the_two_buttons_act_on_what_is_selected_or_arm_the_pointer_when_nothing_is() {
    use dereth_client_runtime::interaction::TargetMode as ArmedWith;
    use dereth_ui_screens::toolbar::target_mode::{EXAMINE_BUTTON, USE_BUTTON};
    use dereth_ui_screens::view::TargetMode as Asked;

    let mut c = a_player_with_an_armoury();
    c.app_mut().probe_mut().objects_mut().world.selected = None;
    c.tick(1);
    let with_nothing_selected = {
        let (_ui, screen) = gameplay_screen(c.app_mut());
        screen.on_target_mode_button(USE_BUTTON) == Some(UiRequest::SetTargetMode(Asked::Use))
            && screen.on_target_mode_button(EXAMINE_BUTTON)
                == Some(UiRequest::SetTargetMode(Asked::Examine))
            && screen
                .on_target_mode_button(ElementId(0x1000_0192))
                .is_none()
    };

    c.app_mut().probe_mut().objects_mut().world.selected = Some(BOW);
    c.tick(1);
    let with_a_selection = {
        let (_ui, screen) = gameplay_screen(c.app_mut());
        screen.on_target_mode_button(USE_BUTTON) == Some(UiRequest::Use(BOW))
            && screen.on_target_mode_button(EXAMINE_BUTTON) == Some(UiRequest::Examine(BOW))
    };

    c.app_mut().probe_mut().objects_mut().world.selected = None;
    c.tick(1);
    let nothing_armed_yet = c.app_mut().interaction().target_mode() == ArmedWith::None;
    c.when(Player::click(USE_BUTTON));
    let the_click_armed_the_pointer = c.app_mut().interaction().target_mode() == ArmedWith::Use
        && c.app_mut().interaction().stats.target_modes_armed == 1;

    c.assert_behaviour(
        "use.button.the-two-buttons-act-on-what-is-selected-or-arm-the-pointer-when-nothing-is",
        move |_| {
            with_nothing_selected
                && with_a_selection
                && nothing_armed_yet
                && the_click_armed_the_pointer
        },
    );
    c.shutdown();
}

// -----------------------------------------------------------------------------------------
// inventory.world-drop.letting-go-over-the-view-of-the-world-is-a-drop-into-it
// -----------------------------------------------------------------------------------------

/// Letting something go over the view of the world -- not over a panel, over the world itself
/// -- is a drop into the world, and it arms the search that works out what the thing lands
/// on. Before this the release named no target at all, so nothing was asked and the icon was
/// not even put back.
pub fn letting_go_over_the_view_of_the_world_is_a_drop_into_it() {
    let mut c = a_player_with_an_armoury();
    let view_of_the_world = shipped(
        &mut c,
        dereth_ui_screens::screens::gameplay::window::SMART_BOX,
    );
    let tile = pack_slot(&mut c, BOW);
    let start = point_of(&mut c, tile);

    // A real drag, carried over the world itself: `carry_over` requires that the element the
    // pointer hit-tests to is the one that catches this drag, so the release below is aimed
    // at something the player could really have aimed at.
    c.when(Grab(start));
    let at = carry_over(&mut c, view_of_the_world);
    clear_requests(c.ui_outbox());
    let picks_before = c.app_mut().interaction().stats.picks_requested;

    // What the window answers for a release there. The request the shipped panels raise goes
    // into a queue the client keeps to itself, so this is the one place a scenario can read
    // it -- and the frame that follows is where the far end is.
    let answered = {
        let (ui, screen) = gameplay_screen(c.app_mut());
        screen.handle_drop_release(ui, view_of_the_world, tile)
    };
    let asked_the_world = answered
        == Some(UiRequest::DragDrop {
            item: BOW,
            target: DropTarget::World,
        });
    c.tick(1);
    // Nothing moved the pointer between the two readings, so the search can only have been
    // armed by the drop.
    let and_armed_the_search = c.app_mut().interaction().stats.picks_requested > picks_before;

    c.when(LetGo(at));
    c.tick(1);
    clear_requests(c.ui_outbox());

    c.assert_behaviour(
        "inventory.world-drop.letting-go-over-the-view-of-the-world-is-a-drop-into-it",
        move |_| asked_the_world && and_armed_the_search,
    );
    c.shutdown();
}

// -----------------------------------------------------------------------------------------
// inventory.place.the-shards-refusal-lets-go-of-the-one-at-a-time-hold-as-well-as-the-grey
// -----------------------------------------------------------------------------------------

/// The hold the player is put under while the shard has not answered has no timeout at all,
/// so the shard's own word is the only thing that can end it -- and a **refusal** ends it
/// just as a confirmation does. Until that was wired, the second gesture of a session was
/// refused for ever.
///
/// Both directions are asserted, because a one-sided check passes on a client that never
/// takes the hold in the first place: the second gesture is refused while the hold is on, the
/// refusal arrives, the grey comes off, and the very same gesture then goes out.
pub fn the_shards_refusal_lets_go_of_the_one_at_a_time_hold_as_well_as_the_grey() {
    use dereth_client_model::inventory::requests::InventoryRequest;

    let (element, ..) = the_place_for(loc::TRINKET_ONE);
    let first = ObjectId(0x5000_8500);
    let second = ObjectId(0x5000_8501);

    let mut c = a_player_at_the_figure();
    carrying(&mut c, first, loc::TRINKET_ONE, 1);
    let sent = let_go_on(&mut c, first, element);
    let the_hold_is_taken = sent.len() == 1
        && c.world_mut().request_lock.pending == InventoryRequest::Wield
        && c.world_mut().weenie(first).expect("carried").waiting;

    carrying(&mut c, second, loc::TRINKET_ONE, 1);
    let refused_before = c.interaction_mut().stats.requests_refused;
    let held_out = let_go_on(&mut c, second, element);
    let the_next_gesture_is_refused =
        held_out.is_empty() && c.interaction_mut().stats.requests_refused == refused_before + 1;

    c.when(the_shard_refuses(first));
    let the_grey_comes_off = !c.world_mut().weenie(first).expect("carried").waiting
        && c.world_mut().request_lock.is_idle();

    carrying(&mut c, second, loc::TRINKET_ONE, 1);
    let sent = let_go_on(&mut c, second, element);
    let and_the_same_gesture_goes_out = sent
        .iter()
        .filter_map(wield_place)
        .map(|(i, _)| i)
        .collect::<Vec<_>>()
        == vec![second];

    c.assert_behaviour(
        "inventory.place.the-shards-refusal-lets-go-of-the-one-at-a-time-hold-as-well-as-the-grey",
        move |_| {
            the_hold_is_taken
                && the_next_gesture_is_refused
                && the_grey_comes_off
                && and_the_same_gesture_goes_out
        },
    );
}

// -----------------------------------------------------------------------------------------
// ui.pointer.turning-the-camera-with-the-right-button-is-not-an-appraisal
// -----------------------------------------------------------------------------------------

/// Holding the right button and moving turns the camera, and letting it go appraises
/// **nothing**, even back where it went down; pressing and letting go in the same place is a
/// click, and that does appraise what is under the pointer. Without the distinction every camera
/// turn looked at whatever happened to be under the cursor when the button came up.
///
/// Both directions are the measurement: a check that only asked about the turn would pass on
/// a client where the right button does nothing at all, which is a different and equally
/// wrong answer. The line between them is the client's own -- three pixels of hand-shake is
/// still a click and four is a turn. Where the release lands is all the world sees of a press
/// on its own; a turn let go where it began is told apart by the front end, which saw the
/// pointer go past the line, and its word is read by that release alone.
pub fn turning_the_camera_with_the_right_button_is_not_an_appraisal() {
    use dereth_client_runtime::interaction::SearchReason;
    use dereth_client_shell::ui::UiMouseEvent;

    /// The right button, as the pointer table names it.
    const RIGHT: u32 = 8;

    // Right-button presses let go `dx` pixels from where each went down, all on one client, each
    // said by the front end to have been a drag or not, or left unsaid.
    let presses = |gestures: &[(i32, Option<bool>)]| {
        let mut c = HeadlessClient::model();
        for &(dx, dragged) in gestures {
            for (start, x) in [(true, 400), (false, 400 + dx)] {
                let e = UiMouseEvent {
                    action: RIGHT,
                    start,
                    x,
                    y: 300,
                    over: None,
                };
                let world_click = dereth_client_runtime::interaction::is_world_click(e.over);
                let inter = c.interaction_mut();
                if let (false, Some(dragged)) = (start, dragged) {
                    inter.note_right_release(dragged);
                }
                inter.wrapper_mouse(e, (800, 600), world_click);
            }
        }
        let inter = c.interaction_mut();
        (
            inter.search_reason(),
            inter.stats.picks_requested,
            inter.stats.mouse_look_releases,
        )
    };
    let pointer = |dx: i32| presses(&[(dx, None)]);

    let a_click = pointer(0) == (SearchReason::Examine, 1, 0);
    let a_turn = pointer(120) == (SearchReason::None, 0, 1);
    let three_pixels_is_still_a_click = pointer(3).0 == SearchReason::Examine;
    let four_is_a_turn = pointer(4).0 == SearchReason::None;
    let a_turn_let_go_where_it_began_is_still_a_turn =
        presses(&[(0, Some(true))]) == (SearchReason::None, 0, 1);
    let a_click_said_to_be_one_is_a_click = presses(&[(0, Some(false))]).0 == SearchReason::Examine;
    let the_word_is_read_by_that_release_alone =
        presses(&[(0, Some(true)), (0, None)]) == (SearchReason::Examine, 1, 1);

    let mut c = HeadlessClient::model();
    c.assert_behaviour(
        "ui.pointer.turning-the-camera-with-the-right-button-is-not-an-appraisal",
        move |_| {
            a_click
                && a_turn
                && three_pixels_is_still_a_click
                && four_is_a_turn
                && a_turn_let_go_where_it_began_is_still_a_turn
                && a_click_said_to_be_one_is_a_click
                && the_word_is_read_by_that_release_alone
        },
    );
}

// -----------------------------------------------------------------------------------------
// inventory.split.typing-a-number-into-the-box-and-leaving-it-sets-how-many-a-drag-moves
// -----------------------------------------------------------------------------------------

/// Typing a number into the quantity box on the toolbar and then clicking away sets how many
/// of a stack the next drag moves, at both ends: what the box itself keeps, and what the part
/// of the client a drop reads keeps. A split of the wrong size looks exactly like a split of
/// the right one, so the number is asserted where the drop will read it.
///
/// Both edges of the clamp are asserted too: over the maximum comes back to the maximum and
/// is written back into the box, and under one is lifted to one rather than left as a move of
/// nothing.
pub fn typing_a_number_into_the_box_and_leaving_it_sets_how_many_a_drag_moves() {
    use dereth_ui_screens::toolbar::splitter::{Splitter, ENTRY_BOX};

    let mut c = a_client_in_the_game();
    let entry = shipped(&mut c, ENTRY_BOX);
    let untouched_is_the_whole_stack = c.app_mut().objects().world.split.is_whole_stack();

    {
        let (_ui, screen) = gameplay_screen(c.app_mut());
        // The client seeds the pair from what is selected; a stack of twenty.
        screen.splitter = Splitter::new(20);
    }

    let mut type_and_leave = |c: &mut HeadlessClient, text: &str| {
        {
            let (ui, _screen) = gameplay_screen(c.app_mut());
            ui.set_focus_element(Some(entry));
            if let Some(t) = ui.text_element_mut(entry) {
                t.set_text(text);
            }
            ui.set_focus_element(None);
        }
        c.tick(1);
    };

    type_and_leave(&mut c, "7");
    let the_box_kept_it = {
        let (_ui, screen) = gameplay_screen(c.app_mut());
        screen.splitter.split_size == 7
    };
    let split = c.app_mut().objects().world.split;
    let and_the_drop_will_read_it =
        (split.split_size, split.max_split_size) == (7, 20) && !split.is_whole_stack();

    type_and_leave(&mut c, "999");
    let clamped_down = {
        let (ui, screen) = gameplay_screen(c.app_mut());
        let text: String = ui.text_element_mut(entry).map_or(String::new(), |t| {
            String::from_utf16_lossy(&t.glyphs.glyphs.iter().map(|g| g.data).collect::<Vec<_>>())
        });
        screen.splitter.split_size == 20 && text == "20"
    } && c.app_mut().objects().world.split.split_size == 20;

    type_and_leave(&mut c, "0");
    let lifted_up = c.app_mut().objects().world.split.split_size == 1;

    c.assert_behaviour(
        "inventory.split.typing-a-number-into-the-box-and-leaving-it-sets-how-many-a-drag-moves",
        move |_| {
            untouched_is_the_whole_stack
                && the_box_kept_it
                && and_the_drop_will_read_it
                && clamped_down
                && lifted_up
        },
    );
    c.shutdown();
}

// -----------------------------------------------------------------------------------------
// inventory.split.while-the-number-box-holds-the-keyboard-a-tiles-key-types-instead-of-firing
// -----------------------------------------------------------------------------------------

/// While the quantity box has the keyboard, every action a key would otherwise fire is
/// swallowed: pressing four types a four instead of firing the fourth tile of the shortcut
/// bar. A hotkey that fired while the player was typing a quantity is exactly the accident
/// this exists to prevent, and it became possible only once keys started reaching the screens
/// at all.
///
/// And asking the box to give the keyboard back puts the number the player last settled on
/// into it -- not the maximum -- and lets the focus go, so the next thing typed is a hotkey
/// again.
pub fn while_the_number_box_holds_the_keyboard_a_tiles_key_types_instead_of_firing() {
    use dereth_ui_screens::toolbar::shortcuts::dispatch;
    use dereth_ui_screens::toolbar::splitter::{Splitter, ENTRY_BOX};

    /// The action the fourth tile of the bar answers.
    const A_TILES_KEY: u32 = 0x1000_0042;

    let mut c = a_client_in_the_game();
    let entry = shipped(&mut c, ENTRY_BOX);

    let (unfocused_it_fires, focused_it_is_swallowed, restored, let_go) = {
        let (ui, screen) = gameplay_screen(c.app_mut());
        screen.splitter = Splitter::new(20);
        screen.splitter.split_size = 3;
        assert_eq!(
            ui.focus_element(),
            None,
            "the premise: nothing holds the keyboard"
        );

        let unfocused = dispatch(A_TILES_KEY, false).is_some();
        let swallowed = dispatch(A_TILES_KEY, true).is_none();

        ui.set_focus_element(Some(entry));
        if let Some(t) = ui.text_element_mut(entry) {
            t.set_text("3");
        }
        let swallowed = swallowed && screen.on_input_action(ui, A_TILES_KEY).is_none();

        screen.reset_stack_size_box(ui);
        let text: String = ui.text_element_mut(entry).map_or(String::new(), |t| {
            String::from_utf16_lossy(&t.glyphs.glyphs.iter().map(|g| g.data).collect::<Vec<_>>())
        });
        (
            unfocused,
            swallowed,
            text == "3",
            ui.focus_element() != Some(entry),
        )
    };
    c.tick(1);
    let and_the_box_reads_it_back = {
        let (_ui, screen) = gameplay_screen(c.app_mut());
        screen.splitter.split_size == 3
    };

    c.assert_behaviour(
            "inventory.split.while-the-number-box-holds-the-keyboard-a-tiles-key-types-instead-of-firing",
            move |_| {
                unfocused_it_fires
                    && focused_it_is_swallowed
                    && restored
                    && let_go
                    && and_the_box_reads_it_back
            },
        );
    c.shutdown();
}
