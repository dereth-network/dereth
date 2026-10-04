use dereth_client_model::weenie::{bitfield, item_type};
use dereth_client_model::Request;
use dereth_primitives::ObjectId;
use dereth_testkit::{ClientSpec, HeadlessClient};

use super::{DropHost, FEEDBACK_CHANNEL};

fn a_client() -> HeadlessClient {
    HeadlessClient::new(ClientSpec::retail())
}

fn a_host(c: &mut HeadlessClient) -> DropHost {
    let store = std::sync::Arc::clone(c.dat_store().expect("a retail client opens the dats"));
    DropHost::new(&store)
}

impl DropHost {
    /// Something lying in the world with a kind and a name of its own.
    fn a_thing_in_the_world(&mut self, id: ObjectId, obj_type: u32, name: &str) -> ObjectId {
        let mut w = dereth_client_model::Weenie::new(id);
        w.pwd.obj_type = obj_type;
        w.pwd.name = name.to_owned();
        self.objects.world.tables.weenies.insert(id, w);
        id
    }

    /// A container lying in the world, openable or not.
    fn a_chest(&mut self, id: ObjectId, openable: bool) -> ObjectId {
        let id = self.a_thing_in_the_world(id, item_type::CONTAINER, "Storage Chest");
        let w = self.objects.world.weenie_mut(id).expect("just seeded");
        w.pwd.items_capacity = Some(10);
        if openable {
            w.pwd.bitfield |= bitfield::OPENABLE;
        }
        id
    }

    /// Whether the thing is still greyed, waiting on the shard.
    fn waiting_on(&self, id: ObjectId) -> bool {
        self.objects.world.weenie(id).expect("seeded").waiting
    }

    /// What the one give this gesture builds names: the recipient, the thing, how many.
    fn given(out: &[Request]) -> Option<(ObjectId, ObjectId, u32)> {
        match out {
            [Request::GiveObjectRequest(m)] => Some((m.target, m.item, m.amount)),
            _ => None,
        }
    }

    /// What the one move this gesture builds names: the thing and the container.
    fn moved(out: &[Request]) -> Option<(ObjectId, ObjectId)> {
        match out {
            [Request::PutItemInContainer(m)] => Some((m.item, m.container)),
            _ => None,
        }
    }
}

// -----------------------------------------------------------------------------------------
// The gift itself.
// -----------------------------------------------------------------------------------------

/// **The central claim.** Something carried, let go over the viewport and picked onto a
/// creature, is offered to that creature -- and nothing at all goes out before the pick has
/// answered.
pub fn a_drop_on_a_creature_asks_to_hand_it_over() {
    let mut c = a_client();
    let mut host = a_host(&mut c);
    let item = host.carry(ObjectId(0x2880_0001));
    let npc = host.a_thing_in_the_world(ObjectId(0x2880_0002), item_type::CREATURE, "Aun Tanua");

    // `DropHost::give` arms the release and only then answers the pick; its own assertions
    // are that the pick went up and that the outbox was empty while it was up.
    let out = host.give(item, npc);
    let offered = DropHost::given(&out) == Some((npc, item, 1));
    // The client predicts nothing: the thing is still in the pack, greyed, and the player is
    // told nothing at all.
    let predicts_nothing = host.waiting_on(item)
        && host
            .objects
            .world
            .weenie(item)
            .expect("seeded")
            .pwd
            .container_id
            == Some(host.player)
        && host.lines().is_empty();

    c.assert_behaviour(
        "inventory.give.a-drop-on-a-creature-asks-to-hand-it-over-and-nothing-goes-first",
        move |_| offered && predicts_nothing,
    );
    c.shutdown();
}

/// **Something that is a creature *and* a container is not given to.** The client asks what
/// kind of thing the target is and compares it for equality, not for a bit -- so a thing that
/// is both falls past the gift arm into the container arm and is refused for being shut.
///
/// This is the most falsifiable claim in the module: written as a bit test it looks right and
/// is wrong for every composite kind. The control is one bit away.
pub fn something_that_is_a_creature_and_a_container_too_is_not_given_to() {
    let mut c = a_client();

    let mut host = a_host(&mut c);
    let item = host.carry(ObjectId(0x2880_0010));
    let both = host.a_thing_in_the_world(
        ObjectId(0x2880_0011),
        item_type::CREATURE | item_type::CONTAINER,
        "Drudge Skulker",
    );
    {
        let w = host.objects.world.weenie_mut(both).expect("just seeded");
        w.pwd.bitfield |= bitfield::OPENABLE;
        w.pwd.items_capacity = Some(10);
    }
    let out = host.give(item, both);
    let not_given = out.is_empty()
            && host.lines()
                == vec![(FEEDBACK_CHANNEL, "You must open the Drudge Skulker first".to_owned())]
            // A refused drop takes the grey off again.
            && !host.waiting_on(item);

    // The control, one bit away: the same thing, a creature and nothing else.
    let mut host = a_host(&mut c);
    let item = host.carry(ObjectId(0x2880_0012));
    let creature =
        host.a_thing_in_the_world(ObjectId(0x2880_0013), item_type::CREATURE, "Drudge Skulker");
    let out = host.give(item, creature);
    let given = DropHost::given(&out) == Some((creature, item, 1));

    c.assert_behaviour(
        "inventory.give.something-that-is-a-creature-and-a-container-too-is-not-given-to",
        move |_| not_given && given,
    );
    c.shutdown();
}

/// How many are handed over is the splitter's own count when the thing is the picked one, and
/// the whole stack otherwise. Both arms, because a client that always sent the whole stack
/// would satisfy a fixture holding one of something.
pub fn how_many_is_the_splitters_count_for_the_picked_thing() {
    use dereth_client_contract::UiRequest;

    let mut c = a_client();

    // Not picked: the whole stack.
    let mut host = a_host(&mut c);
    let item = host.carry_stack(ObjectId(0x2880_0020), 12);
    let npc = host.a_thing_in_the_world(ObjectId(0x2880_0021), item_type::CREATURE, "Ulgrim");
    let whole = DropHost::given(&host.give(item, npc)).map(|(_, _, n)| n) == Some(12);

    // The same stack, picked, with the splitter moved to five **through the splitter's own
    // producer** rather than by writing the number where the client keeps it.
    let mut host = a_host(&mut c);
    let item = host.carry_stack(ObjectId(0x2880_0022), 12);
    let npc = host.a_thing_in_the_world(ObjectId(0x2880_0023), item_type::CREATURE, "Ulgrim");
    host.objects.world.selected = Some(item);
    host.inter.queue(
        Vec::new(),
        vec![UiRequest::StackSliderChanged { split: 5, max: 12 }],
    );
    host.frame();
    let out = host.give(item, npc);
    let part = DropHost::given(&out) == Some((npc, item, 5));

    c.assert_behaviour(
        "inventory.give.how-many-is-the-splitters-count-for-the-picked-thing",
        move |_| whole && part,
    );
    c.shutdown();
}

/// Letting something go over your own body is the pickup and not a gift.
pub fn a_drop_on_yourself_is_the_pickup_and_not_a_gift() {
    let mut c = a_client();
    let mut host = a_host(&mut c);
    let item = host.carry(ObjectId(0x2880_0030));
    let me = host.player;
    let out = host.give(item, me);
    let into_the_pack = DropHost::moved(&out) == Some((item, me));

    c.assert_behaviour(
        "inventory.give.a-drop-on-yourself-is-the-pickup-and-not-a-gift",
        move |_| into_the_pack,
    );
    c.shutdown();
}

/// The two tests the client makes of the **thing**, each with its own sentence: something
/// that is not the player's to give, and something already on the trade table.
pub fn something_not_yours_or_on_the_trade_table_is_refused_in_words() {
    let mut c = a_client();

    let mut host = a_host(&mut c);
    let loose = host.a_thing_in_the_world(ObjectId(0x2880_0040), item_type::MISC, "Rusty Key");
    let npc = host.a_thing_in_the_world(ObjectId(0x2880_0041), item_type::CREATURE, "Nuhmudira");
    host.objects
        .world
        .weenie_mut(loose)
        .expect("just seeded")
        .waiting = true;
    let out = host.give(loose, npc);
    let not_yours = out.is_empty()
        && host.lines()
            == vec![(
                FEEDBACK_CHANNEL,
                "You must first pick up the Rusty Key".to_owned(),
            )]
        && !host.waiting_on(loose);

    let mut host = a_host(&mut c);
    let item = host.carry(ObjectId(0x2880_0042));
    let npc = host.a_thing_in_the_world(ObjectId(0x2880_0043), item_type::CREATURE, "Nuhmudira");
    {
        let w = host.objects.world.weenie_mut(item).expect("just seeded");
        w.pwd.name = "Gem of Enlightenment".to_owned();
        w.trade_state = 1;
    }
    let out = host.give(item, npc);
    let on_the_table = out.is_empty()
        && host.lines()
            == vec![(
                FEEDBACK_CHANNEL,
                "You are trading the Gem of Enlightenment, it cannot be dropped".to_owned(),
            )];

    c.assert_behaviour(
        "inventory.give.something-not-yours-or-on-the-trade-table-is-refused-in-words",
        move |_| not_yours && on_the_table,
    );
    c.shutdown();
}

/// A chest lying in the world takes nothing until it is open, and says which of the two
/// reasons it refused for.
pub fn a_drop_on_a_chest_in_the_world_is_refused_until_it_is_open() {
    let mut c = a_client();

    let mut host = a_host(&mut c);
    let item = host.carry(ObjectId(0x2880_0050));
    let chest = host.a_chest(ObjectId(0x2880_0051), false);
    let out = host.give(item, chest);
    let locked = out.is_empty()
        && host.lines() == vec![(FEEDBACK_CHANNEL, "The Storage Chest is locked".to_owned())];

    let mut host = a_host(&mut c);
    let item = host.carry(ObjectId(0x2880_0052));
    let chest = host.a_chest(ObjectId(0x2880_0053), true);
    let out = host.give(item, chest);
    let shut = out.is_empty()
        && host.lines()
            == vec![(
                FEEDBACK_CHANNEL,
                "You must open the Storage Chest first".to_owned(),
            )];

    let mut host = a_host(&mut c);
    let item = host.carry(ObjectId(0x2880_0054));
    let chest = host.a_chest(ObjectId(0x2880_0055), true);
    host.objects.world.ground_object = Some(chest);
    let out = host.give(item, chest);
    let goes_in = DropHost::moved(&out) == Some((item, chest));

    c.assert_behaviour(
        "container.ground.a-drop-on-one-in-the-world-is-refused-until-it-is-open",
        move |_| locked && shut && goes_in,
    );
    c.shutdown();
}

/// A release over nothing at all puts the thing on the ground -- and a second release, for
/// something that is already out of every container and still the player's, is cancelled
/// instead, in one word.
///
/// This is the one arm that consults the player's own body, so it is driven from a settled
/// one; the same leg with the body in the air is a row of its own.
pub fn a_drop_onto_nothing_lands_on_the_ground_and_a_second_one_is_cancelled() {
    let mut c = a_client();
    let store = std::sync::Arc::clone(c.dat_store().expect("a retail client opens the dats"));
    let body = super::a_settled_body(&store);

    let mut host = a_host(&mut c);
    let item = host.carry(ObjectId(0x2880_0060));
    let out = host.drop_onto(item, ObjectId(0), Some(&body));
    let on_the_ground = matches!(out.as_slice(), [Request::DropItem(_)]);

    // The one shape the cancellation is reachable in at all: in nothing, with no place of its
    // own yet, and still the player's because he is holding it.
    let mut host = a_host(&mut c);
    let item = host.carry(ObjectId(0x2880_0061));
    {
        let me = host.player;
        let w = host.objects.world.weenie_mut(item).expect("just seeded");
        w.pwd.container_id = Some(ObjectId(0));
        w.pwd.wielder_id = Some(me);
        w.pwd.location = Some(0);
        w.determine_position_state();
    }
    let the_premise = host.objects.world.weenie(item).map(|w| w.current_state)
        == Some(dereth_client_model::weenie::PositionState::In3dView)
        && host.objects.world.is_owned_by_player(item);
    let _ = host.inter.take_pending_requests();
    let out = host.drop_onto(item, ObjectId(0), Some(&body));
    let cancelled =
        out.is_empty() && host.lines() == vec![(FEEDBACK_CHANNEL, "Move cancelled".to_owned())];

    c.assert_behaviour(
        "inventory.world-drop.a-drop-onto-nothing-lands-on-the-ground-and-a-second-is-cancelled",
        move |_| on_the_ground && the_premise && cancelled,
    );
    c.shutdown();
}

/// A stack let go on a matching stack is merged **before** anything about the target is
/// considered -- the target here is also a creature, so the merge really is being measured
/// ahead of the gift and not merely in its absence.
pub fn a_stack_on_a_matching_stack_merges_before_any_target_is_considered() {
    let mut c = a_client();
    let mut host = a_host(&mut c);
    let item = host.carry_stack(ObjectId(0x2880_0080), 5);
    let onto = host.carry_stack(ObjectId(0x2880_0081), 3);
    for id in [item, onto] {
        let w = host.objects.world.weenie_mut(id).expect("just seeded");
        w.pwd.max_stack_size = Some(10);
        w.pwd.wcid = 273;
    }
    host.objects
        .world
        .weenie_mut(onto)
        .expect("just seeded")
        .pwd
        .obj_type = item_type::CREATURE;

    let out = host.give(item, onto);
    let merged = matches!(
        out.as_slice(),
        [Request::StackableMerge(m)]
            if m.merge_from == item && m.merge_to == onto && m.amount == 5
    );

    c.assert_behaviour(
        "inventory.place.a-stack-on-a-matching-stack-merges-before-any-target-is-considered",
        move |_| merged,
    );
    c.shutdown();
}

/// A drop on a shopkeeper is a sale: the shop and the thing are parked until the shop
/// answers, and no gift goes out -- a shopkeeper is a creature too, and this arm is asked
/// first.
pub fn a_drop_on_a_shopkeeper_is_a_sale_and_parks_the_thing() {
    let mut c = a_client();
    let mut host = a_host(&mut c);
    let item = host.carry(ObjectId(0x2880_0070));
    let vendor = host.a_thing_in_the_world(
        ObjectId(0x2880_0071),
        item_type::CREATURE,
        "Shopkeeper Feng",
    );
    host.objects
        .world
        .weenie_mut(vendor)
        .expect("just seeded")
        .pwd
        .bitfield |= bitfield::VENDOR;
    let nothing_parked = host.objects.world.shop.attempt_open_vendor.is_none();

    let out = host.give(item, vendor);
    let parked = host.objects.world.shop.attempt_open_vendor == Some(vendor)
        && host.objects.world.shop.attempt_sale_object == Some(item)
        && !out
            .iter()
            .any(|r| matches!(r, Request::GiveObjectRequest(_)));

    c.assert_behaviour(
        "vendor.sell.a-drop-on-a-shopkeeper-is-a-sale-and-parks-the-thing",
        move |_| nothing_parked && parked,
    );
    c.shutdown();
}

/// A drop on another player is a gift unless the player has asked for it to open the trade
/// window -- which the shipped default does not.
pub fn a_drop_on_another_player_opens_the_trade_only_if_asked_for() {
    use dereth_client_model::player::options::option::DRAG_ITEM_ON_PLAYER_OPENS_SECURE_TRADE;

    let mut c = a_client();

    let mut host = a_host(&mut c);
    let item = host.carry(ObjectId(0x2880_0072));
    let other =
        host.a_thing_in_the_world(ObjectId(0x2880_0073), item_type::CREATURE, "Someone Else");
    host.objects
        .world
        .weenie_mut(other)
        .expect("just seeded")
        .pwd
        .bitfield |= bitfield::PLAYER;
    let shipped_default_is_off = !host
        .objects
        .world
        .player_system
        .options
        .get(DRAG_ITEM_ON_PLAYER_OPENS_SECURE_TRADE);
    let out = host.give(item, other);
    let a_gift = DropHost::given(&out) == Some((other, item, 1))
        && host.objects.world.trade.attempt_to_player == ObjectId(0);

    let mut host = a_host(&mut c);
    let item = host.carry(ObjectId(0x2880_0074));
    let other =
        host.a_thing_in_the_world(ObjectId(0x2880_0075), item_type::CREATURE, "Someone Else");
    host.objects
        .world
        .weenie_mut(other)
        .expect("just seeded")
        .pwd
        .bitfield |= bitfield::PLAYER;
    host.objects
        .world
        .player_system
        .options
        .set(DRAG_ITEM_ON_PLAYER_OPENS_SECURE_TRADE, true);
    let out = host.give(item, other);
    let a_trade = !out
        .iter()
        .any(|r| matches!(r, Request::GiveObjectRequest(_)))
        && host.objects.world.trade.attempt_to_player == other
        && host.objects.world.trade.attempt_object == item;

    c.assert_behaviour(
        "trade.window.a-drop-on-another-player-opens-it-only-if-the-player-asked-for-that",
        move |_| shipped_default_is_off && a_gift && a_trade,
    );
    c.shutdown();
}

/// **Every gift the recordings hold, reproduced by a drop onto its own recipient.** The
/// recipient, the thing and how many all come out of the recording; a client that named the
/// wrong recipient would be giving somebody else's things away.
pub fn every_recorded_give_is_reproduced_by_a_drop_onto_its_recipient() {
    let mut c = a_client();
    let gives = super::every_recorded_give();
    assert!(
        !gives.is_empty(),
        "the recordings are this scenario's oracle and carry no gift at all"
    );

    let mut ok = true;
    for g in &gives {
        let mut host = a_host(&mut c);
        // How many the recording said, so that what is compared is the gesture and its
        // fields and never a stack size the recording did not state.
        let item = host.carry_stack(
            g.item,
            u16::try_from(g.amount).expect("recorded amounts are small"),
        );
        let npc = host.a_thing_in_the_world(g.target, item_type::CREATURE, "recipient");
        let out = host.give(item, npc);
        ok &= DropHost::given(&out) == Some((g.target, g.item, g.amount));
    }
    let n = gives.len();
    eprintln!("give census: {n} recorded gifts reproduced");

    c.assert_behaviour(
        "inventory.give.every-recorded-one-is-reproduced-by-a-drop-onto-its-recipient",
        move |_| ok,
    );
    c.shutdown();
}

dereth_testkit::scenarios! {
    scenario_a_drop_on_a_creature_asks_to_hand_it_over => a_drop_on_a_creature_asks_to_hand_it_over ["inventory.give.a-drop-on-a-creature-asks-to-hand-it-over-and-nothing-goes-first"],
    scenario_something_that_is_a_creature_and_a_container_too_is_not_given_to => something_that_is_a_creature_and_a_container_too_is_not_given_to ["inventory.give.something-that-is-a-creature-and-a-container-too-is-not-given-to"],
    scenario_how_many_is_the_splitters_count_for_the_picked_thing => how_many_is_the_splitters_count_for_the_picked_thing ["inventory.give.how-many-is-the-splitters-count-for-the-picked-thing"],
    scenario_a_drop_on_yourself_is_the_pickup_and_not_a_gift => a_drop_on_yourself_is_the_pickup_and_not_a_gift ["inventory.give.a-drop-on-yourself-is-the-pickup-and-not-a-gift"],
    scenario_something_not_yours_or_on_the_trade_table_is_refused_in_words => something_not_yours_or_on_the_trade_table_is_refused_in_words ["inventory.give.something-not-yours-or-on-the-trade-table-is-refused-in-words"],
    scenario_a_drop_on_a_chest_in_the_world_is_refused_until_it_is_open => a_drop_on_a_chest_in_the_world_is_refused_until_it_is_open ["container.ground.a-drop-on-one-in-the-world-is-refused-until-it-is-open"],
    scenario_a_drop_onto_nothing_lands_on_the_ground_and_a_second_one_is_cancelled => a_drop_onto_nothing_lands_on_the_ground_and_a_second_one_is_cancelled ["inventory.world-drop.a-drop-onto-nothing-lands-on-the-ground-and-a-second-is-cancelled"],
    scenario_a_stack_on_a_matching_stack_merges_before_any_target_is_considered => a_stack_on_a_matching_stack_merges_before_any_target_is_considered ["inventory.place.a-stack-on-a-matching-stack-merges-before-any-target-is-considered"],
    scenario_a_drop_on_a_shopkeeper_is_a_sale_and_parks_the_thing => a_drop_on_a_shopkeeper_is_a_sale_and_parks_the_thing ["vendor.sell.a-drop-on-a-shopkeeper-is-a-sale-and-parks-the-thing"],
    scenario_a_drop_on_another_player_opens_the_trade_only_if_asked_for => a_drop_on_another_player_opens_the_trade_only_if_asked_for ["trade.window.a-drop-on-another-player-opens-it-only-if-the-player-asked-for-that"],
    scenario_every_recorded_give_is_reproduced_by_a_drop_onto_its_recipient => every_recorded_give_is_reproduced_by_a_drop_onto_its_recipient ["inventory.give.every-recorded-one-is-reproduced-by-a-drop-onto-its-recipient"],
}
