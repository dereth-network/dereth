//! The `cpu` tier's panel scenarios: the claims about panels that open no data file. Those are the
//! census over the house messages a shard can send, the examine pane's highlighted-property tables,
//! and the clearing of a quality -- routed to a receiver, removed from the player's store, and kept
//! off the public marks.
//!
//! A panel claim is usually a claim about the *shipped element tree*, which is built out of the
//! retail data, and there is no fixture set that would build it without the dats; those claims are
//! in `tests/dat/panels.rs`. The quality claims take the character description from the
//! `long-solo-play` recording through `Inbound` and feed every other message through `Peer`, which
//! opens no socket.

use dereth_testkit::HeadlessClient;

/// **The denominator is the point.** Every message about housing a shard can send this client is
/// delivered to both consumers as a real session event and the client is asked whether either of
/// them took an arm -- which is the only thing that tells a decoder that works from a receiver
/// that is called. A decoder alone proves nothing.
///
/// The list is written out rather than shortened as rows close: a census that drops a row when it is fixed cannot tell
/// *closed* from *forgotten*, and the one that follows a message going quiet again is this one.
pub fn every_house_message_the_shard_can_send_reaches_a_receiver() {
    use dereth_protocol::Opcode;

    /// The inbound house messages, in opcode order, each with what it is.
    const INBOUND: [(Opcode, &str); 9] = [
        (
            Opcode::HOUSE_HOUSE_PROFILE,
            "what a dwelling costs, for the purchase window",
        ),
        (
            Opcode::HOUSE_HOUSE_DATA,
            "the player's own house, for the House tab",
        ),
        (Opcode::HOUSE_HOUSE_STATUS, "the player has no house"),
        (Opcode::HOUSE_UPDATE_RENT_TIME, "a new maintenance period"),
        (Opcode::HOUSE_UPDATE_RENT_PAYMENT, "a new maintenance list"),
        (Opcode::HOUSE_UPDATE_RESTRICTIONS, "who may enter"),
        (Opcode::HOUSE_UPDATE_HAR, "the guest list, for the scroll"),
        (
            Opcode::HOUSE_HOUSE_TRANSACTION,
            "the house transaction failed",
        ),
        (
            Opcode::HOUSE_AVAILABLE_HOUSES,
            "which dwellings are for sale",
        ),
    ];

    let mut received = Vec::new();
    let mut dropped = Vec::new();
    for (op, what) in INBOUND {
        dereth_client_runtime::dropped::clear();
        let mut blob = op.0.to_le_bytes().to_vec();
        blob.extend(std::iter::repeat_n(0_u8, 64));
        let events =
            [dereth_client_net::client_session::SessionEvent::UiEvent { opcode: op, blob }];

        let mut world = dereth_client_model::World::new();
        let mut hud = dereth_client_shell::hud::Hud::new();
        hud.apply_events(&events, &mut world);

        let mut world = dereth_client_model::World::new();
        let mut inter = dereth_client_runtime::interaction::Interaction::new();
        dereth_client_runtime::interaction::apply_events(&mut inter, &events, &mut world);

        if dereth_client_runtime::dropped::unreceived(op) {
            dropped.push((op, what));
        } else {
            received.push(op);
        }
    }

    let mut c = HeadlessClient::model();
    c.assert_behaviour(
        "house.messages.every-house-message-the-shard-can-send-reaches-a-receiver",
        move |_| {
            INBOUND.len() == 9
                && received.len() + dropped.len() == 9
                && dropped.is_empty()
                && received.len() == 9
        },
    );
    c.shutdown();
}

dereth_testkit::scenarios! {
    scenario_every_house_message_the_shard_can_send_reaches_a_receiver => every_house_message_the_shard_can_send_reaches_a_receiver ["house.messages.every-house-message-the-shard-can-send-reaches-a-receiver"],
    scenario_every_highlighted_property_is_answered_by_the_table_its_flag_names => every_highlighted_property_is_answered_by_the_table_its_flag_names ["examine.enchanted.every-property-the-pane-highlights-is-answered-by-the-table-its-flag-names"],
    scenario_every_clearing_message_the_shard_can_send_reaches_a_receiver => every_clearing_message_the_shard_can_send_reaches_a_receiver ["qualities.remove.every-one-of-the-sixteen-clearing-messages-reaches-a-receiver"],
    scenario_every_kind_of_clearing_deletes_the_key_and_absent_is_not_zero => every_kind_of_clearing_deletes_the_key_and_absent_is_not_zero ["qualities.remove.each-kind-is-deleted-rather-than-set-back-to-its-own-zero"],
    scenario_a_clearing_older_than_the_change_it_would_undo_is_refused => a_clearing_older_than_the_change_it_would_undo_is_refused ["qualities.remove.one-older-than-the-change-it-would-undo-is-refused"],
    scenario_a_public_clearing_reaches_the_object_it_names_and_runs_no_mirror => a_public_clearing_reaches_the_object_it_names_and_runs_no_mirror ["qualities.remove.one-naming-another-body-reaches-it-and-moves-no-mark-of-its-own"],
}

/// **The key list is falsifiable.** The assessment pane walks its own list of properties worth
/// colouring and asks the client for each one's pair of bits; a property neither of the client's
/// two tables answers for would be skipped in silence and its line would be drawn plain for ever.
///
/// It also asserts that the two tables **disagree** about which properties they answer for, in the
/// direction the client itself does -- so the flag beside each key is load-bearing and not
/// decoration -- and that the raised bit of every pair is sixteen places above the enchanted one,
/// which is what makes a transcription slip in either table visible from here.
///
/// It opens no data file, so it is a `cpu` scenario.
pub fn every_highlighted_property_is_answered_by_the_table_its_flag_names() {
    use dereth_client_model::appraisal::{float_highlight, int_highlight};
    use dereth_ui_screens::panels::examination::HIGHLIGHTED_PROPERTIES;

    let counted = HIGHLIGHTED_PROPERTIES.len();
    let mut every_key_resolves = true;
    let mut the_tables_are_disjoint = true;
    let mut the_raised_bit_is_sixteen_up = true;
    for (key, is_float) in HIGHLIGHTED_PROPERTIES {
        let (right, wrong) = if *is_float {
            (float_highlight(*key), int_highlight(*key))
        } else {
            (int_highlight(*key), float_highlight(*key))
        };
        every_key_resolves &= right.is_some();
        the_tables_are_disjoint &= wrong.is_none();
        if let Some(h) = right {
            the_raised_bit_is_sixteen_up &= h.high == h.low << 16;
        }
    }

    let mut c = HeadlessClient::model();
    c.assert_behaviour(
        "examine.enchanted.every-property-the-pane-highlights-is-answered-by-the-table-its-flag-names",
        move |_| {
            // Three whole numbers and fourteen fractions: the properties the two blocks that draw
            // armour and weapons ask about, plus the three that came with the blocks added later.
            counted == 17
                && every_key_resolves
                && the_tables_are_disjoint
                && the_raised_bit_is_sixteen_up
        },
    );
    c.shutdown();
}

// =============================================================================================
// qualities.remove.*
//
// Four of the quality-removal claims open no data file at all: the routing gate is a census over
// two model consumers, and the other three assert on the player's own quality store, on the
// counters the HUD and the interaction layer keep, and on the public marks a body carries -- none
// of which is the element tree. They are `cpu` scenarios. The fifth, the character sheet, is in
// `tests/dat/panels.rs`.
//
// **The recorded captures are not dats**, so a `cpu` scenario may still replay one, and these do:
// the player's store has to be one the login path could have made, because the delete these claims
// are about is skipped outright on an object that has no store.
//
// # Nothing binds and nothing leaves
//
// Every remove and every update below is built here and fed through the client's **real
// transport** by `dereth_testkit::Peer`, which opens no socket. The one message that does not go
// that way is the recorded character description, and the reason is the harness's own: it is
// nearly two kilobytes, and it is a description rather than a panel notice, so nothing here turns
// on which way it arrived.
// =============================================================================================

/// The recording whose `0x0013` is the quality store these claims act on.
const QUALS_SESSION: &str = "long-solo-play";

/// The player, and the only object in the client that has anywhere to keep a quality.
const QUALS_PLAYER: dereth_primitives::ObjectId = dereth_primitives::ObjectId(0x5000_0001);
/// Any other object -- the subject of the **public** forms, which are the interaction layer's.
const QUALS_OTHER: dereth_primitives::ObjectId = dereth_primitives::ObjectId(0x8000_0ABC);

/// `PropertyInt` **20** `CoinValue`.
const QUALS_COIN_VALUE: u32 = 20;
/// `PropertyInt` **134** `PlayerKillerStatus` -- one of the cases the stat-update path mirrors
/// onto the `PublicWeenieDesc` bitfield, which
/// is what makes it the observable for *"the mirror does not run on a remove"*.
const QUALS_PLAYER_KILLER_STATUS: u32 = 134;

/// The ordered-event envelope, and the sub-type the character description arrives as.
const QUALS_ORDERED_EVENT: u32 = 0xF7B0;
const QUALS_PLAYER_DESCRIPTION: u32 = 0x0013;

/// The sixteen, in opcode order, paired with the table their removal template belongs to and
/// whether the form carries an object id.
///
/// Sixteen handlers cover **eight** generic property-table removals: integer, 64-bit integer,
/// Boolean, float, string, data id, instance id, and position. Each type has two opcode
/// paths.
/// There are no specialized skill, primary-attribute, or secondary-attribute removal paths,
/// which is why the sixteen opcodes form eight pairs.
const QUALS_SIXTEEN: [(dereth_protocol::Opcode, dereth_client_model::StatType, bool); 16] = [
    (
        dereth_protocol::Opcode::QUALITIES_PRIVATE_REMOVE_INT_EVENT,
        dereth_client_model::StatType::Int,
        false,
    ),
    (
        dereth_protocol::Opcode::QUALITIES_REMOVE_INT_EVENT,
        dereth_client_model::StatType::Int,
        true,
    ),
    (
        dereth_protocol::Opcode::QUALITIES_PRIVATE_REMOVE_BOOL_EVENT,
        dereth_client_model::StatType::Bool,
        false,
    ),
    (
        dereth_protocol::Opcode::QUALITIES_REMOVE_BOOL_EVENT,
        dereth_client_model::StatType::Bool,
        true,
    ),
    (
        dereth_protocol::Opcode::QUALITIES_PRIVATE_REMOVE_FLOAT_EVENT,
        dereth_client_model::StatType::Float,
        false,
    ),
    (
        dereth_protocol::Opcode::QUALITIES_REMOVE_FLOAT_EVENT,
        dereth_client_model::StatType::Float,
        true,
    ),
    (
        dereth_protocol::Opcode::QUALITIES_PRIVATE_REMOVE_STRING_EVENT,
        dereth_client_model::StatType::String,
        false,
    ),
    (
        dereth_protocol::Opcode::QUALITIES_REMOVE_STRING_EVENT,
        dereth_client_model::StatType::String,
        true,
    ),
    (
        dereth_protocol::Opcode::QUALITIES_PRIVATE_REMOVE_DATA_IDEVENT,
        dereth_client_model::StatType::Did,
        false,
    ),
    (
        dereth_protocol::Opcode::QUALITIES_REMOVE_DATA_IDEVENT,
        dereth_client_model::StatType::Did,
        true,
    ),
    (
        dereth_protocol::Opcode::QUALITIES_PRIVATE_REMOVE_INSTANCE_IDEVENT,
        dereth_client_model::StatType::Iid,
        false,
    ),
    (
        dereth_protocol::Opcode::QUALITIES_REMOVE_INSTANCE_IDEVENT,
        dereth_client_model::StatType::Iid,
        true,
    ),
    (
        dereth_protocol::Opcode::QUALITIES_PRIVATE_REMOVE_POSITION_EVENT,
        dereth_client_model::StatType::Position,
        false,
    ),
    (
        dereth_protocol::Opcode::QUALITIES_REMOVE_POSITION_EVENT,
        dereth_client_model::StatType::Position,
        true,
    ),
    (
        dereth_protocol::Opcode::QUALITIES_PRIVATE_REMOVE_INT64_EVENT,
        dereth_client_model::StatType::Int64,
        false,
    ),
    (
        dereth_protocol::Opcode::QUALITIES_REMOVE_INT64_EVENT,
        dereth_client_model::StatType::Int64,
        true,
    ),
];

/// Which blob of `session` is the shard's `0x0013` -- found rather than pinned, so a re-promoted
/// corpus moves the index instead of reddening an arithmetic nobody reads.
fn quals_description_index(session: &str) -> usize {
    dereth_client_net::client_session::testing::Corpus::load(session)
        .unwrap_or_else(|e| panic!("the recording {session} does not parse: {e}"))
        .unwrap_or_else(|| {
            panic!(
                "the decoded corpus has no scenario {session}; it is generated from the committed \
                 recordings and a missing one is a broken checkout"
            )
        })
        .blobs
        .into_iter()
        .find(|b| {
            b.dir == dereth_client_net::client_session::testing::Direction::ServerToClient
                && b.opcode == QUALS_ORDERED_EVENT
                && b.payload
                    .get(12..16)
                    .map(|s| u32::from_le_bytes([s[0], s[1], s[2], s[3]]))
                    == Some(QUALS_PLAYER_DESCRIPTION)
        })
        .map(|b| b.idx)
        .unwrap_or_else(|| panic!("{session} never reached the shard's character description"))
}

/// One object, created by the shard on the object queue, which is how a real shard introduces one.
fn quals_create(
    c: &mut HeadlessClient,
    shard: &mut dereth_testkit::Peer,
    id: dereth_primitives::ObjectId,
) {
    let mut p = dereth_protocol::objects::ObjectCreatePayload {
        id,
        ..Default::default()
    };
    p.physicsdesc.bitfield |= dereth_protocol::types::physicsdesc::flags::SETUP;
    p.physicsdesc.setup_id = Some(0x0200_0001);
    p.physicsdesc.timestamps.instance = 1;
    shard.send(
        c,
        dereth_testkit::replay::OBJECT_QUEUE,
        dereth_protocol::write_blob(&dereth_protocol::objects::ItemCreateObject(p))
            .expect("the create encodes"),
    );
    c.tick(1);
    assert!(
        c.view().world().weenie(id).is_some(),
        "the shard's create of {id:?} did not reach the world"
    );
}

/// A logged-in player with a real character description, another object beside him, and the shard
/// that made both.
///
/// The description is what allocates the player's quality store: the removal path skips the
/// delete when that store is null, so without the description
/// every claim below would measure an absence and prove nothing.
fn quals_a_character_and_a_shard() -> (HeadlessClient, dereth_testkit::Peer) {
    let at = quals_description_index(QUALS_SESSION);
    let mut c = HeadlessClient::model();
    let mut shard = dereth_testkit::Peer::attach_creating(&mut c, QUALS_PLAYER);
    assert!(
        c.world_mut().set_player(QUALS_PLAYER),
        "the identity is adopted once"
    );
    quals_create(&mut c, &mut shard, QUALS_OTHER);

    c.when(dereth_testkit::Inbound::from_corpus(
        QUALS_SESSION,
        at..at + 1,
    ));
    c.tick(4);
    assert!(
        c.view().world().player_qualities().is_some(),
        "the recorded description has to install a quality store or nothing below tests a delete"
    );
    (c, shard)
}

/// The player's own quality, read off the one store. `None` means the lookup failed, which is a
/// different result from finding a stored zero.
fn quals_player_quality(
    c: &HeadlessClient,
    key: dereth_client_model::StatKey,
) -> Option<dereth_client_model::StatValue> {
    c.view()
        .world()
        .player_qualities()
        .expect("the player's store")
        .get(key)
}

/// The `StatType` a `StatValue` belongs under. Written out here rather than taken off the code
/// under test, so the table in the scenario is the scenario's claim and not the model's.
fn quals_kind_of(v: &dereth_client_model::StatValue) -> dereth_client_model::StatType {
    use dereth_client_model::{StatType, StatValue};
    match v {
        StatValue::Int(_) => StatType::Int,
        StatValue::Int64(_) => StatType::Int64,
        StatValue::Bool(_) => StatType::Bool,
        StatValue::Float(_) => StatType::Float,
        StatValue::Str(_) => StatType::String,
        StatValue::Did(_) => StatType::Did,
        StatValue::Iid(_) => StatType::Iid,
        StatValue::Position(_) => StatType::Position,
        other => panic!("no removal template exists for {other:?}"),
    }
}

/// One private clearing message of `kind`, through the transport, and the frames that consume it.
fn quals_private_remove(
    c: &mut HeadlessClient,
    shard: &mut dereth_testkit::Peer,
    kind: dereth_client_model::StatType,
    sequence: u8,
    property: u32,
) {
    use dereth_client_model::StatType;
    use dereth_protocol::qualities as wire;
    // Every private form has the identical nine-byte layout -- `PrivateRemove` is a *shared*
    // struct, one per shape rather than one per opcode -- so only the message type differs.
    let r = wire::PrivateRemove {
        sequence,
        property_id: property,
    };
    match kind {
        StatType::Int => shard.event(c, &wire::QualitiesPrivateRemoveInt(r)),
        StatType::Int64 => shard.event(c, &wire::QualitiesPrivateRemoveInt64(r)),
        StatType::Bool => shard.event(c, &wire::QualitiesPrivateRemoveBool(r)),
        StatType::Float => shard.event(c, &wire::QualitiesPrivateRemoveFloat(r)),
        StatType::String => shard.event(c, &wire::QualitiesPrivateRemoveString(r)),
        StatType::Did => shard.event(c, &wire::QualitiesPrivateRemoveDataId(r)),
        StatType::Iid => shard.event(c, &wire::QualitiesPrivateRemoveInstanceId(r)),
        StatType::Position => shard.event(c, &wire::QualitiesPrivateRemovePosition(r)),
        other => panic!("no removal template exists for {other:?}"),
    }
    c.tick(4);
}

/// One **public** clearing message for an integer property, naming the object it is about.
fn quals_public_remove_int(
    c: &mut HeadlessClient,
    shard: &mut dereth_testkit::Peer,
    object: dereth_primitives::ObjectId,
    sequence: u8,
    property: u32,
) {
    shard.event(
        c,
        &dereth_protocol::qualities::QualitiesRemoveInt(dereth_protocol::qualities::PublicRemove {
            sequence,
            object,
            property_id: property,
        }),
    );
    c.tick(4);
}

/// A `0x02CD Qualities_PrivateUpdateInt` -- the production writer these scenarios use to *put* a
/// value on the player before removing it, so nothing asserts against a store it seeded by hand.
fn quals_set_int(
    c: &mut HeadlessClient,
    shard: &mut dereth_testkit::Peer,
    sequence: u8,
    property: u32,
    value: i32,
) {
    shard.event(
        c,
        &dereth_protocol::qualities::QualitiesPrivateUpdateInt(
            dereth_protocol::qualities::PrivateUpdate {
                sequence,
                property_id: property,
                value,
            },
        ),
    );
    c.tick(4);
}

/// A `0x02CE Qualities_UpdateInt` for somebody who is not the player.
fn quals_set_object_int(
    c: &mut HeadlessClient,
    shard: &mut dereth_testkit::Peer,
    object: dereth_primitives::ObjectId,
    sequence: u8,
    property: u32,
    value: i32,
) {
    shard.event(
        c,
        &dereth_protocol::qualities::QualitiesUpdateInt(dereth_protocol::qualities::PublicUpdate {
            sequence,
            object,
            property_id: property,
            value,
        }),
    );
    c.tick(4);
}

/// Does this build have a receiver for `opcode` on the UI queue?
///
/// The opcode goes to **both** consumers of a UI event and the answer is whether
/// either took an arm. A body of zeroes is enough -- an arm either decodes it or counts it
/// undecodable, and in neither case does it reach the wildcard.
fn quals_ui_queue_has_a_receiver(opcode: dereth_protocol::Opcode) -> bool {
    dereth_client_runtime::dropped::clear();
    let mut blob = opcode.0.to_le_bytes().to_vec();
    blob.extend(std::iter::repeat_n(0_u8, 64));
    let events = [dereth_client_net::client_session::SessionEvent::UiEvent { opcode, blob }];

    let mut world = dereth_client_model::World::new();
    let mut hud = dereth_client_shell::hud::Hud::new();
    hud.apply_events(&events, &mut world);

    let mut world = dereth_client_model::World::new();
    let mut inter = dereth_client_runtime::interaction::Interaction::new();
    dereth_client_runtime::interaction::apply_events(&mut inter, &events, &mut world);

    !dereth_client_runtime::dropped::unreceived(opcode)
}

/// **The gate. All sixteen, and the negative control beside them.**
///
/// This is the pinned list for this family, and it is what a change that deletes an arm trips over.
/// It is kept apart from the census of the messages the recorded shard sends, because that census
/// counts the opcodes **this shard sends** and this shard sends none of these.
///
/// The control is the half that makes the sixteen a measurement: without it a `received` for each
/// is indistinguishable from an oracle that answers true for everything, and the ledger has to
/// name the opcode and the site rather than only a total.
///
/// Falsified by: deleting either remove arm. With both gone all sixteen report dropped.
pub fn every_clearing_message_the_shard_can_send_reaches_a_receiver() {
    // Eight tables, a private and a public form each.
    let counted = QUALS_SIXTEEN.len();

    let mut dropped_rows = Vec::new();
    println!("\n  opcode  name                                             state");
    println!("  {}", "-".repeat(72));
    for (op, _, _) in QUALS_SIXTEEN {
        let has = quals_ui_queue_has_a_receiver(op);
        println!(
            "  {:#06X}  {:<46}  {}",
            op.0,
            op.name().unwrap_or("<not in the opcode table>"),
            if has { "received" } else { "DROPPED" }
        );
        if !has {
            dropped_rows.push(op.0);
        }
    }
    println!("  {}\n", "-".repeat(72));
    let all_received = dropped_rows.is_empty();

    // **The instrument must be able to fail.**
    let control_is_still_unreceived =
        !quals_ui_queue_has_a_receiver(dereth_protocol::Opcode::ADMIN_ENVIRONS);
    dereth_client_runtime::dropped::clear();
    let _ = quals_ui_queue_has_a_receiver(dereth_protocol::Opcode::ADMIN_ENVIRONS);
    // ...and it names the opcode and the site, not just a total.
    let control_is_named = dereth_client_runtime::dropped::snapshot().iter().any(|d| {
        d.opcode == dereth_protocol::Opcode::ADMIN_ENVIRONS
            && d.site == dereth_client_runtime::dropped::Site::UiEvent
    });
    dereth_client_runtime::dropped::clear();

    let mut c = HeadlessClient::model();
    c.assert_behaviour(
        "qualities.remove.every-one-of-the-sixteen-clearing-messages-reaches-a-receiver",
        move |_| counted == 16 && all_received && control_is_still_unreceived && control_is_named,
    );
    c.shutdown();
}

/// **Each of the eight kinds: the quality is gone afterwards, and gone is not zero.**
///
/// The value is seeded through the store the login path fills and then removed **through the
/// wire**, and the reading is the one that can tell an absent key from a present zero -- which is
/// exactly the distinction the lookup's success and failure results draw. Removal deletes the
/// hash-table entry instead of writing back a default value.
///
/// A ninth case is asserted with them, one key over per type: a value deliberately set to the
/// *zero* of its own type is still there afterwards, so an implementation that wrote a default
/// instead of deleting fails here rather than passing by looking the same. Without it the first
/// assertion is also satisfied by a client that never stored anything at all.
///
/// Seeding is through the store rather than the wire because there is no update opcode for a
/// String or a Position this scenario would otherwise have to re-encode, and the subject is the
/// delete.
///
/// Falsified by: making the store's remove write a default instead of deleting; deleting the
/// remove arm from the HUD (the value survives untouched).
pub fn every_kind_of_clearing_deletes_the_key_and_absent_is_not_zero() {
    use dereth_client_model::{StatKey, StatValue};

    let (mut c, mut shard) = quals_a_character_and_a_shard();

    // property, the seeded value, and the zero of the same type.
    let cases: [(u32, StatValue, StatValue); 8] = [
        (0x101, StatValue::Int(42), StatValue::Int(0)),
        (0x102, StatValue::Int64(-9), StatValue::Int64(0)),
        (0x103, StatValue::Bool(true), StatValue::Bool(false)),
        (0x104, StatValue::Float(1.5), StatValue::Float(0.0)),
        (
            0x105,
            StatValue::Str("Ust".to_owned()),
            StatValue::Str(String::new()),
        ),
        (
            0x106,
            StatValue::Did(dereth_primitives::DataId(0x0600_1234)),
            StatValue::Did(dereth_primitives::DataId(0)),
        ),
        (
            0x107,
            StatValue::Iid(dereth_primitives::ObjectId(0x8000_0009)),
            StatValue::Iid(dereth_primitives::ObjectId(0)),
        ),
        (
            0x108,
            StatValue::Position(dereth_protocol::types::space::PositionWire::default()),
            StatValue::Position(dereth_protocol::types::space::PositionWire::default()),
        ),
    ];

    {
        let q = c
            .world_mut()
            .player_qualities_mut()
            .expect("the player's store");
        for (prop, value, zero) in &cases {
            q.set(StatKey::new(quals_kind_of(value), *prop), value.clone());
            q.set(
                StatKey::new(quals_kind_of(value), prop + 0x80),
                zero.clone(),
            );
        }
    }
    let seeded = cases.iter().all(|(prop, value, _)| {
        quals_player_quality(&c, StatKey::new(quals_kind_of(value), *prop)) == Some(value.clone())
    });

    let before = c.view().hud().stats.quality_removes;
    let mut every_kind_went = true;
    let mut every_zero_stayed = true;
    let mut seq = 0_u8;
    for (prop, value, zero) in &cases {
        seq += 1;
        let kind = quals_kind_of(value);
        quals_private_remove(&mut c, &mut shard, kind, seq, *prop);
        every_kind_went &= quals_player_quality(&c, StatKey::new(kind, *prop)).is_none();
        // The control, one key over: a genuine zero is still *there*.
        every_zero_stayed &=
            quals_player_quality(&c, StatKey::new(kind, prop + 0x80)) == Some(zero.clone());
    }
    let eight_deletes = c.view().hud().stats.quality_removes - before == 8;
    // ...and every one of them found a key.
    let none_was_absent = c.view().hud().stats.quality_removes_absent == 0;
    let none_was_stale = c.view().hud().stats.quality_removes_stale == 0;

    c.assert_behaviour(
        "qualities.remove.each-kind-is-deleted-rather-than-set-back-to-its-own-zero",
        move |_| {
            seeded
                && every_kind_went
                && every_zero_stayed
                && eight_deletes
                && none_was_absent
                && none_was_stale
        },
    );
    c.shutdown();
}

/// **A stale clearing is refused, by the counter the matching update consumes.**
///
/// The removal path updates the object's property-sequence gate **before** deleting the value,
/// using the same `prop | 0x10000` key as the matching update. So a `0x02CD`
/// at sequence 5 and a `0x01D1` at sequence 3 for the same property are one conversation, and the
/// older one loses whichever kind it is.
///
/// The newer clearing that does land is the half that makes the refusal a measurement: without it
/// the first assertion is satisfied by a receiver that refuses everything. And the counter is
/// shared in the other direction too, which is the last step: the update at 5 that already landed
/// is stale against the clearing's 6.
///
/// Falsified by: moving the gate after the delete (the stale clearing empties the purse); keying
/// the gate off the opcode rather than the table tag (the clearing gets its own counter and
/// sequence 3 is accepted).
pub fn a_clearing_older_than_the_change_it_would_undo_is_refused() {
    use dereth_client_model::{StatKey, StatType, StatValue};

    let (mut c, mut shard) = quals_a_character_and_a_shard();
    let key = StatKey::new(StatType::Int, QUALS_COIN_VALUE);

    quals_set_int(&mut c, &mut shard, 5, QUALS_COIN_VALUE, 9_995);
    let premise = quals_player_quality(&c, key) == Some(StatValue::Int(9_995))
        && c.view().hud().stats.quality_removes_stale == 0;

    // Three is older than five under the wrap rule, and this is a *clearing* against an *update's*
    // stamp -- the whole point.
    quals_private_remove(&mut c, &mut shard, StatType::Int, 3, QUALS_COIN_VALUE);
    let nothing_was_deleted = quals_player_quality(&c, key) == Some(StatValue::Int(9_995));
    let counted_stale = c.view().hud().stats.quality_removes_stale == 1;
    let not_counted_as_a_delete = c.view().hud().stats.quality_removes == 0;

    // Six is newer, and now it lands.
    quals_private_remove(&mut c, &mut shard, StatType::Int, 6, QUALS_COIN_VALUE);
    let six_landed =
        quals_player_quality(&c, key).is_none() && c.view().hud().stats.quality_removes == 1;

    // One counter per (kind, property): the clearing moved it past the update's stamp.
    quals_set_int(&mut c, &mut shard, 5, QUALS_COIN_VALUE, 1);
    let the_update_is_now_stale =
        quals_player_quality(&c, key).is_none() && c.view().hud().stats.quality_updates_stale == 1;

    c.assert_behaviour(
        "qualities.remove.one-older-than-the-change-it-would-undo-is-refused",
        move |_| {
            premise
                && nothing_was_deleted
                && counted_stale
                && not_counted_as_a_delete
                && six_landed
                && the_update_is_now_stale
        },
    );
    c.shutdown();
}

/// **A public clearing reaches the object it names, and the `PublicWeenieDesc` mirror does not
/// run** -- the second of the stat-removal handler's two divergences from the stat-update
/// handler, and the one a transcription is most likely to get wrong by symmetry.
///
/// An update follows three ordered steps:
///
/// 1. store the integer;
/// 2. report the stat update;
/// 3. call the change handler.
///
/// A removal deletes the integer and calls the remove handler without reporting a stat update.
///
/// The stat-update report is the mirror, and `PropertyInt` **134 `PlayerKillerStatus`** is one of
/// the cases it switches on: it clears and re-sets the `PLAYER_KILLER` / `PK_LITE` /
/// `IMPENETRABLE` bits that determine the public player-killer state. So
/// the update moves the bit and the remove **must not**.
///
/// **The original client gives this fixture's non-player object no quality store**: its object
/// construction allocates one only when the object's id is the player id. The fixture asserts that
/// absence directly. There is therefore no stored value to watch on the other object, and a test
/// that asserted on one would be asserting on `None == None` whether the arm existed or not. That
/// reading is asserted here rather than assumed, because it is what makes the mirror the right
/// observable: it is the premise of this scenario.
///
/// The control that makes the two central assertions a measurement rather than an absence is the
/// update at sequence 1 afterwards: the clearing did reach the object's own stamper and spent
/// sequence 2, so an older update cannot move the bit -- which is only observable because the
/// clearing was routed -- and a fresh one still can, so the object is not simply frozen.
///
/// Falsified by: deleting the interaction layer's arm (the count stays at zero and the stale
/// control then lands); running the mirror from the remove "for symmetry" (the mark clears).
pub fn a_public_clearing_reaches_the_object_it_names_and_runs_no_mirror() {
    use dereth_client_model::{StatKey, StatType, StatValue};

    let (mut c, mut shard) = quals_a_character_and_a_shard();
    let coin = StatKey::new(StatType::Int, QUALS_COIN_VALUE);

    // Only the player has anywhere to keep a quality, and this is the premise the observable
    // below rests on. Its other half -- that the player's store *does* exist -- keeps it from
    // reading "nobody has one".
    quals_set_object_int(&mut c, &mut shard, QUALS_OTHER, 1, QUALS_COIN_VALUE, 77);
    let nowhere_to_store = c
        .view()
        .world()
        .weenie(QUALS_OTHER)
        .expect("the other object exists")
        .qualities
        .is_none()
        && c.view().world().player_qualities().is_some();

    quals_set_int(&mut c, &mut shard, 1, QUALS_COIN_VALUE, 500);
    quals_set_object_int(
        &mut c,
        &mut shard,
        QUALS_OTHER,
        1,
        QUALS_PLAYER_KILLER_STATUS,
        4,
    );
    let is_pk = |client: &HeadlessClient| {
        client
            .view()
            .world()
            .weenie(QUALS_OTHER)
            .expect("the other object exists")
            .is_pk()
    };
    let premise = quals_player_quality(&c, coin) == Some(StatValue::Int(500)) && is_pk(&c);
    let removes_before = c.view().interaction().stats.object_quality_removes;
    let hud_before = c.view().hud().stats.quality_removes;

    quals_public_remove_int(
        &mut c,
        &mut shard,
        QUALS_OTHER,
        2,
        QUALS_PLAYER_KILLER_STATUS,
    );

    // It named the other object, so it is the boundary's and it reached the world's own remove.
    let reached_the_object =
        c.view().interaction().stats.object_quality_removes - removes_before == 1;
    // And the mirror did NOT run, so the mark stands until the shard sends an update clearing it.
    let the_mark_stands = is_pk(&c);
    // The HUD refuses a public form naming somebody else -- retail returns early when the
    // object lookup finds nothing, applied to the store's owner.
    let the_hud_kept_out = c.view().hud().stats.quality_removes == hud_before;
    // The player was not the subject and must be untouched.
    let the_player_is_untouched = quals_player_quality(&c, coin) == Some(StatValue::Int(500));

    // Sequence 1 is older than the 2 the clearing consumed on the SAME counter, so it is refused
    // -- the proof that the clearing reached the stamper.
    quals_set_object_int(
        &mut c,
        &mut shard,
        QUALS_OTHER,
        1,
        QUALS_PLAYER_KILLER_STATUS,
        0,
    );
    let the_stale_update_was_refused = is_pk(&c);
    // ...and a fresh one still works.
    quals_set_object_int(
        &mut c,
        &mut shard,
        QUALS_OTHER,
        3,
        QUALS_PLAYER_KILLER_STATUS,
        0,
    );
    let a_fresh_update_still_lands = !is_pk(&c);

    // The public form that *does* name the player is the player's on both sides, exactly as the
    // public updates are.
    quals_public_remove_int(&mut c, &mut shard, QUALS_PLAYER, 3, QUALS_COIN_VALUE);
    let naming_the_player_clears_it = quals_player_quality(&c, coin).is_none();

    c.assert_behaviour(
        "qualities.remove.one-naming-another-body-reaches-it-and-moves-no-mark-of-its-own",
        move |_| {
            nowhere_to_store
                && premise
                && reached_the_object
                && the_mark_stands
                && the_hud_kept_out
                && the_player_is_untouched
                && the_stale_update_was_refused
                && a_fresh_update_still_lands
                && naming_the_player_clears_it
        },
    );
    c.shutdown();
}
