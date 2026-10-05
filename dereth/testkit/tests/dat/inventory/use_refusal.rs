use dereth_client_model::inventory::use_object::{messages, ItemUses, UseOutcome, UseRefusal};
use dereth_client_model::inventory::SplitState;
use dereth_client_model::{Notice, RecordingRequests, RecordingSink, World};
use dereth_primitives::{ObjectId, ServerTime};
use dereth_protocol::types::PublicWeenieDesc;
use dereth_testkit::{ClientSpec, HeadlessClient, Player, Target};
use {dereth_rules::weenie::bitfield, dereth_rules::weenie::item_type};

const ME: ObjectId = ObjectId(0x5000_0001);
const SUBJECT: ObjectId = ObjectId(0x5000_0002);

/// The one kind of use a thing can have that says "only while it is held".
const ONLY_WHILE_HELD: u32 = 0x4;
/// The kind that says "not at all".
const NOT_AT_ALL: u32 = dereth_client_model::weenie::item_useable::NO;

fn a_client() -> HeadlessClient {
    HeadlessClient::new(ClientSpec::retail())
}

fn put(w: &mut World, id: ObjectId, name: &str, pwd: PublicWeenieDesc) {
    let mut it = dereth_client_model::Weenie::new(id);
    it.pwd = pwd;
    it.pwd.name = name.to_owned();
    it.valid = true;
    w.tables.weenies.insert(id, it);
}

/// A world with a player in it and nothing else.
fn a_player() -> World {
    let mut w = World::new();
    put(
        &mut w,
        ME,
        "Larktest",
        PublicWeenieDesc {
            bitfield: bitfield::PLAYER,
            obj_type: item_type::CREATURE,
            items_capacity: Some(102),
            containers_capacity: Some(7),
            ..PublicWeenieDesc::default()
        },
    );
    w.player = Some(ME);
    w.tables
        .inventories
        .insert(ME, dereth_client_model::objects::ObjectInventory::new(ME));
    w
}

/// A world with one thing in it, described as the scenario says.
fn a_player_and(name: &str, pwd: PublicWeenieDesc) -> World {
    let mut w = a_player();
    put(&mut w, SUBJECT, name, pwd);
    w
}

fn use_it(w: &mut World, id: ObjectId) -> (UseOutcome, RecordingSink) {
    let mut req = RecordingRequests::default();
    let mut out = RecordingSink::default();
    let outcome = w.use_object(
        &mut req,
        &mut out,
        id,
        SplitState::default(),
        ServerTime(1.0),
    );
    (outcome, out)
}

/// Every line the use raised on the one channel the bubble strip takes and the scrollback
/// drops. The channel is part of the claim.
fn lines(out: &RecordingSink) -> Vec<String> {
    out.0
        .iter()
        .filter_map(|n| match n {
            Notice::DisplayString {
                channel: 0x1A,
                text,
                ..
            } => Some(text.clone()),
            _ => None,
        })
        .collect()
}

/// The description of a thing that cannot be used at all, and is nailed down.
fn cannot_be_used(obj_type: u32, extra_bits: u32) -> PublicWeenieDesc {
    PublicWeenieDesc {
        obj_type,
        bitfield: bitfield::STUCK | extra_bits,
        useability: Some(NOT_AT_ALL),
        ..PublicWeenieDesc::default()
    }
}

// -----------------------------------------------------------------------------------------
// One arm, one sentence.
// -----------------------------------------------------------------------------------------

/// Something already on the trade table cannot be used, and says so naming itself.
pub fn using_something_on_the_trade_table_says_so() {
    let mut c = a_client();
    let mut w = a_player_and(
        "Shimmering Isparian Sword",
        cannot_be_used(item_type::MELEE_WEAPON, 0),
    );
    w.weenie_mut(SUBJECT).expect("seeded").trade_state = 1;
    let (outcome, out) = use_it(&mut w, SUBJECT);
    let said = outcome == UseOutcome::Refused(UseRefusal::BeingTraded)
        && lines(&out)
            == vec![
                "You cannot use the Shimmering Isparian Sword because you are trading it"
                    .to_owned(),
            ];

    c.assert_behaviour(
        "use.refusal.something-on-the-trade-table-says-so",
        move |_| said,
    );
    c.shutdown();
}

/// Something that can only be used while it is held says so while it is not.
pub fn using_something_that_must_be_held_says_so() {
    let mut c = a_client();
    let mut w = a_player_and(
        "Acid Wand",
        PublicWeenieDesc {
            obj_type: item_type::CASTER,
            bitfield: bitfield::STUCK,
            useability: Some(ONLY_WHILE_HELD),
            location: Some(0),
            ..PublicWeenieDesc::default()
        },
    );
    let (outcome, out) = use_it(&mut w, SUBJECT);
    let said = outcome == UseOutcome::Refused(UseRefusal::MustBeWielded)
        && lines(&out) == vec!["You must wield the Acid Wand to use it".to_owned()];

    c.assert_behaviour(
        "use.refusal.something-that-must-be-held-says-so",
        move |_| said,
    );
    c.shutdown();
}

/// A use that wants something to be used **on** arms the second click and asks for it in
/// words. It is the one line of the seven that is not a refusal at all.
pub fn a_use_that_wants_a_target_arms_the_second_click_and_asks_for_one() {
    let mut c = a_client();
    let mut w = a_player_and(
        "Flask of Lead Oil",
        PublicWeenieDesc {
            obj_type: item_type::USELESS,
            bitfield: bitfield::STUCK,
            // The kinds a thing may be used **on** are the upper half of what it says about
            // itself, so a lower-half bit alone does not take this arm.
            useability: Some(0x0001_0008),
            ..PublicWeenieDesc::default()
        },
    );
    let (outcome, out) = use_it(&mut w, SUBJECT);
    let armed = outcome == UseOutcome::TargetModeArmed
        && w.targeting_object == SUBJECT
        && lines(&out) == vec!["Choose a target for the Flask of Lead Oil".to_owned()];

    c.assert_behaviour(
        "use.target-mode.a-use-that-wants-a-target-arms-the-second-click-and-asks-for-one",
        move |_| armed,
    );
    c.shutdown();
}

/// Something that could be attacked, clicked while the player is not in a stance, is told to
/// arm himself first rather than told it cannot be used.
pub fn using_something_attackable_out_of_a_stance_says_to_arm_first() {
    let mut c = a_client();
    let mut w = a_player_and(
        "Drudge Skulker",
        cannot_be_used(item_type::CREATURE, bitfield::ATTACKABLE),
    );
    w.combat.combat_mode = dereth_client_model::combat::CombatMode::NonCombat;
    let (outcome, out) = use_it(&mut w, SUBJECT);
    let said = outcome == UseOutcome::Refused(UseRefusal::AttackDoveIcon)
        && lines(&out) == vec!["To attack Drudge Skulker, click on the dove icon first".to_owned()];

    c.assert_behaviour(
        "use.refusal.something-attackable-out-of-a-stance-says-to-arm-first",
        move |_| said,
    );
    c.shutdown();
}

/// Anything else that cannot be used says exactly that, naming itself.
pub fn using_something_that_cannot_be_used_says_so() {
    let mut c = a_client();
    let mut w = a_player_and("Pile of Dirt", cannot_be_used(item_type::MISC, 0));
    let (outcome, out) = use_it(&mut w, SUBJECT);
    let said = outcome == UseOutcome::Refused(UseRefusal::NotUseable)
        && lines(&out) == vec!["The Pile of Dirt cannot be used".to_owned()];

    c.assert_behaviour(
        "use.refusal.something-that-cannot-be-used-says-so",
        move |_| said,
    );
    c.shutdown();
}

/// **The two that must stay silent**, so that "recovered" never becomes "broadcast": using
/// yourself, and clicking something you are already in a stance against. Both are refusals
/// and neither prints anything.
pub fn using_yourself_or_what_you_are_fighting_says_nothing() {
    let mut c = a_client();

    let mut w = a_player();
    {
        let me = w.weenie_mut(ME).expect("seeded");
        me.pwd.useability = Some(NOT_AT_ALL);
        me.pwd.bitfield |= bitfield::STUCK;
    }
    let (outcome, out) = use_it(&mut w, ME);
    let yourself = outcome == UseOutcome::Refused(UseRefusal::ItIsYou)
        && lines(&out).is_empty()
        && UseRefusal::ItIsYou.text("Larktest").is_none();

    let mut w = a_player_and(
        "Drudge Skulker",
        cannot_be_used(item_type::CREATURE, bitfield::ATTACKABLE),
    );
    w.combat.combat_mode = dereth_client_model::combat::CombatMode::Melee;
    let (outcome, out) = use_it(&mut w, SUBJECT);
    let fighting = outcome == UseOutcome::Refused(UseRefusal::AttackingIt)
        && lines(&out).is_empty()
        && UseRefusal::AttackingIt.text("Drudge Skulker").is_none();

    // The one arm with no producer in this client at all is still recorded, with the place
    // the thing's name goes, so that a client that grows one has the wording waiting.
    let unproduced =
        messages::SELECT_YOUR_TARGET_BEFORE_USING == "Select your target before using the %s";

    c.assert_behaviour(
        "use.refusal.using-yourself-or-what-you-are-fighting-says-nothing",
        move |_| yourself && fighting && unproduced,
    );
    c.shutdown();
}

// -----------------------------------------------------------------------------------------
// Which of the arms the recordings witness.
// -----------------------------------------------------------------------------------------

/// **The honest half.** A sentence read out of the shipped client is a transcription; *"the
/// player will see this line"* is a claim about things that exist. So every thing the
/// recordings ever describe is counted into the arm its own description would take, and three
/// of the arms turn out to have no witness at all -- each for its own reason, and the count
/// is asserted rather than printed so that a recording that changed them would be a red.
pub fn each_arm_is_counted_against_the_recordings_and_three_have_no_witness() {
    use dereth_client_model::inventory::use_object::use_bitfield;
    use dereth_client_net::client_session::testing::{Corpus, Direction};
    use dereth_protocol::objects::ItemCreateObject;
    use dereth_protocol::Message;

    let mut c = a_client();

    let mut total = 0u32;
    let mut door_bit = 0u32;
    let mut door_and_not_useable = 0u32;
    let mut dove = 0u32;
    let mut plain = 0u32;
    let mut held_but_not_held = 0u32;
    let mut targeted = 0u32;
    let mut not_useable = 0u32;
    for s in super::EVERY_SESSION {
        let rows = Corpus::load(s)
            .unwrap_or_else(|e| panic!("{s} decodes: {e}"))
            .unwrap_or_else(|| panic!("{s} is a committed recording"))
            .blobs;
        for row in rows
            .iter()
            .filter(|r| r.dir == Direction::ServerToClient && r.opcode == 0xF745)
        {
            let Ok(o) =
                ItemCreateObject::read(&mut dereth_protocol::Reader::new(&row.payload[4..]))
            else {
                continue;
            };
            total += 1;
            let uses = ItemUses(o.0.wdesc.useability.unwrap_or(0));
            let bits = o.0.wdesc.bitfield;
            if bits & use_bitfield::DOOR != 0 {
                door_bit += 1;
            }
            if uses.is_useable_targeted() {
                targeted += 1;
                continue;
            }
            if uses.is_useable() {
                continue;
            }
            not_useable += 1;
            // The client's own order: a thing that opens, then the two attackable arms, then
            // everything else. A described thing is never the player, so that arm is skipped.
            if bits & use_bitfield::DOOR != 0 {
                door_and_not_useable += 1;
            } else if o.0.wdesc.obj_type & item_type::CREATURE != 0
                && bits & bitfield::ATTACKABLE != 0
            {
                dove += 1;
            } else {
                plain += 1;
            }
            if o.0.wdesc.location.unwrap_or(0) == 0
                && uses.least_limited_source_use()
                    & dereth_client_model::weenie::item_useable::WIELDED
                    != 0
            {
                held_but_not_held += 1;
            }
        }
    }
    eprintln!(
        "use-refusal census: {total} described things; not useable {not_useable} = opens \
             {door_and_not_useable} + attackable {dove} + plain {plain}; the opening bit \
             {door_bit}; wants a target {targeted}; held-but-not-held {held_but_not_held}"
    );

    let counted = total == 837 && not_useable + targeted == 325;
    // Witnessed, and by this many.
    let witnessed = plain == 192 && dove == 110 && targeted == 23;
    // Not witnessed, and each for its own reason: every thing that opens in the recordings
    // *can* be used, which is what a working door is; and nothing described is one that has
    // to be held and is not being held. The trade arm can have no witness from a description
    // at all, because whether a thing is on the trade table is not part of one.
    let unwitnessed = door_bit == 130 && door_and_not_useable == 0 && held_but_not_held == 0;

    c.assert_behaviour(
        "use.refusal.each-arm-is-counted-against-the-recordings-and-three-have-no-witness",
        move |_| counted && witnessed && unwitnessed,
    );
    c.shutdown();
}

// -----------------------------------------------------------------------------------------
// The gate: the shipped button, and the bubble the player reads.
// -----------------------------------------------------------------------------------------

/// The shipped toolbar's Use button.
fn use_button() -> Target {
    Target::Element(dereth_ui_screens::toolbar::target_mode::USE_BUTTON)
}

/// The chat scrollback's own text.
fn scrollback(c: &mut HeadlessClient) -> String {
    super::text_of(c, dereth_ui_screens::chat::window::LOG)
}

/// A whole client with the shipped screen up, a player, and one thing selected the way a
/// click in the world selects it.
fn a_client_looking_at(name: &str, pwd: PublicWeenieDesc) -> HeadlessClient {
    let mut c = HeadlessClient::new(ClientSpec::gameplay(4));
    // The client's own world is seeded into rather than replaced: it carries tables the
    // client built for itself when it opened the shipped data, and the name a refusal uses
    // is composed out of one of them.
    seed_into(c.world_mut(), name, pwd);
    let mut sink = RecordingSink::default();
    c.world_mut()
        .set_selected_object(Some(SUBJECT), false, &mut sink);
    // One frame carries the selection to the toolbar.
    c.tick(1);
    c
}

/// The player and the one thing, put into a world the client already has.
fn seed_into(w: &mut World, name: &str, pwd: PublicWeenieDesc) {
    put(
        w,
        ME,
        "Larktest",
        PublicWeenieDesc {
            bitfield: bitfield::PLAYER,
            obj_type: item_type::CREATURE,
            items_capacity: Some(102),
            containers_capacity: Some(7),
            ..PublicWeenieDesc::default()
        },
    );
    w.player = Some(ME);
    w.tables
        .inventories
        .insert(ME, dereth_client_model::objects::ObjectInventory::new(ME));
    put(w, SUBJECT, name, pwd);
}

/// **The gate.** A chest in the world that will not open, selected as a click in the world
/// selects it, and the shipped toolbar's own Use button pressed and released. The sentence
/// the player reads is read back out of the live bubble strip -- and the scrollback is
/// asserted **not** to have it, because a client that shouted every refusal everywhere would
/// satisfy the first half on its own.
pub fn the_chest_that_will_not_open_says_so_in_the_bubble_strip() {
    let mut c = a_client_looking_at(
        "Chest",
        cannot_be_used(
            item_type::MISC,
            dereth_client_model::inventory::use_object::use_bitfield::DOOR,
        ),
    );
    let before = super::bubbles(&mut c).len();
    let log_before = scrollback(&mut c);

    c.when(Player::Click(use_button())).tick(4);

    let want = "You can't open or close this Chest that way";
    let after = super::bubbles(&mut c);
    let refused = c.view().interaction().stats.uses_refused == 1
        && after.len() == before + 1
        && after.iter().any(|t| t == want);
    let log_after = scrollback(&mut c);
    let stayed_out_of_the_log = !log_after.contains(want) && log_after == log_before;

    c.assert_behaviour(
        "use.refusal.reaches-the-bubble-strip-and-never-the-scrollback",
        move |_| refused && stayed_out_of_the_log,
    );
    c.shutdown();
}

/// The refusal names the thing the way the player sees it named, which for a thing made of
/// something is that material and then its name.
pub fn the_refusal_names_the_thing_the_way_the_player_sees_it() {
    let mut pwd = cannot_be_used(
        item_type::MISC,
        dereth_client_model::inventory::use_object::use_bitfield::DOOR,
    );
    pwd.material_type = Some(0x3A);
    let mut c = a_client_looking_at("Door", pwd);
    let table_is_installed = c.view().world().material_name(0x3A) == Some("Bronze");

    let before = super::bubbles(&mut c).len();
    let log_before = scrollback(&mut c);
    c.when(Player::Click(use_button())).tick(4);

    let want = "You can't open or close this Bronze Door that way";
    let after = super::bubbles(&mut c);
    let named = c.view().interaction().stats.uses_refused == 1
        && after.len() == before + 1
        && after.iter().any(|t| t == want);
    let log_after = scrollback(&mut c);
    let stayed_out_of_the_log = !log_after.contains(want) && log_after == log_before;

    c.assert_behaviour(
        "use.refusal.names-the-thing-the-way-the-player-sees-it-named",
        move |_| table_is_installed && named && stayed_out_of_the_log,
    );
    c.shutdown();
}

dereth_testkit::scenarios! {
    scenario_using_something_on_the_trade_table_says_so => using_something_on_the_trade_table_says_so ["use.refusal.something-on-the-trade-table-says-so"],
    scenario_using_something_that_must_be_held_says_so => using_something_that_must_be_held_says_so ["use.refusal.something-that-must-be-held-says-so"],
    scenario_a_use_that_wants_a_target_arms_the_second_click_and_asks_for_one => a_use_that_wants_a_target_arms_the_second_click_and_asks_for_one ["use.target-mode.a-use-that-wants-a-target-arms-the-second-click-and-asks-for-one"],
    scenario_using_something_attackable_out_of_a_stance_says_to_arm_first => using_something_attackable_out_of_a_stance_says_to_arm_first ["use.refusal.something-attackable-out-of-a-stance-says-to-arm-first"],
    scenario_using_something_that_cannot_be_used_says_so => using_something_that_cannot_be_used_says_so ["use.refusal.something-that-cannot-be-used-says-so"],
    scenario_using_yourself_or_what_you_are_fighting_says_nothing => using_yourself_or_what_you_are_fighting_says_nothing ["use.refusal.using-yourself-or-what-you-are-fighting-says-nothing"],
    scenario_each_arm_is_counted_against_the_recordings_and_three_have_no_witness => each_arm_is_counted_against_the_recordings_and_three_have_no_witness ["use.refusal.each-arm-is-counted-against-the-recordings-and-three-have-no-witness"],
    scenario_the_chest_that_will_not_open_says_so_in_the_bubble_strip => the_chest_that_will_not_open_says_so_in_the_bubble_strip ["use.refusal.reaches-the-bubble-strip-and-never-the-scrollback"],
    scenario_the_refusal_names_the_thing_the_way_the_player_sees_it => the_refusal_names_the_thing_the_way_the_player_sees_it ["use.refusal.names-the-thing-the-way-the-player-sees-it-named"],
}
