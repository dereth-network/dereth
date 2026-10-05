use super::*;

/// `Inventory_GiveObjectRequest`, as the recording's own client sent it.
const GIVE_OBJECT_REQUEST: u32 = 0x00CD;
/// The ordered **event** envelope the shard answers in, and the answer's own sub-type.
const ORDERED_EVENT: u32 = 0xF7B0;
const SERVER_SAYS_CONTAIN_ID: u32 = 0x0022;
/// Every recording the corpus holds, so that a count here has a denominator.
pub(super) const EVERY_SESSION: [&str; 7] = [
    "first-login-walk-jump",
    "early-inventory-and-casting",
    "short-second-connection",
    "login-account-booted",
    "ddd-interrogation-only",
    "long-solo-play",
    "short-play-with-training",
];

/// One give the recording's own client sent, and the answer that followed it.
#[derive(Debug, Clone)]
pub(super) struct RecordedGive {
    session: &'static str,
    pub(super) target: ObjectId,
    pub(super) item: ObjectId,
    pub(super) amount: u32,
    /// The answer's own bytes, for the scenarios that replay one.
    reply: Option<Vec<u8>>,
}

/// Every give in one recording, paired with the answer naming the same thing.
///
/// The give's own fields are read through `Outbound`'s `Sent::field`; the answer is read off the
/// recording's server-to-client half, where the corpus is the oracle and the harness has no
/// typed reader.
fn recorded_gives(session: &'static str) -> Vec<RecordedGive> {
    let corpus = Corpus::load(session)
        .expect("the recording parses")
        .expect("the corpus carries this recording");
    let replies: Vec<&dereth_client_net::client_session::testing::CorpusBlob> = corpus
        .blobs
        .iter()
        .filter(|b| {
            b.dir == dereth_client_net::client_session::testing::Direction::ServerToClient
                && b.payload.len() >= 24
                && dword(&b.payload, 0) == ORDERED_EVENT
                && dword(&b.payload, 12) == SERVER_SAYS_CONTAIN_ID
        })
        .collect();

    Outbound::all(session)
        .into_iter()
        .filter(|s| s.message == GIVE_OBJECT_REQUEST)
        .map(|s| {
            let target = ObjectId(s.field(0).expect("a give names its recipient"));
            let item = ObjectId(s.field(1).expect("a give names the thing"));
            let amount = s.field(2).expect("a give names how many");
            // The answer has to come **after** the give and name the same thing into the same
            // recipient -- two fields that could have disagreed.
            let reply = replies
                .iter()
                .find(|b| {
                    b.idx > s.idx
                        && dword(&b.payload, 16) == item.0
                        && dword(&b.payload, 20) == target.0
                })
                .map(|b| b.payload[12..].to_vec());
            RecordedGive {
                session,
                target,
                item,
                amount,
                reply,
            }
        })
        .collect()
}

/// Whether a recording's decoded corpus carries a client half at all.
///
/// **One of the seven does not.** The committed corpus is a decoded, curated form of the
/// recordings, and one of them decodes to a single blob with nothing of the client's in it.
/// `Outbound::all` panics on such a recording rather than answer an empty list -- deliberately,
/// because a reader that answered nothing would let a scenario pass over nothing -- so the
/// census asks first and says how many recordings it really counted.
fn carries_a_client_half(session: &str) -> bool {
    Corpus::load(session)
        .expect("the recording parses")
        .expect("the corpus carries this recording")
        .blobs
        .iter()
        .any(|b| b.dir == dereth_client_net::client_session::testing::Direction::ClientToServer)
}

fn sessions_with_a_client_half() -> Vec<&'static str> {
    EVERY_SESSION
        .into_iter()
        .filter(|s| carries_a_client_half(s))
        .collect()
}

pub(super) fn every_recorded_give() -> Vec<RecordedGive> {
    sessions_with_a_client_half()
        .into_iter()
        .flat_map(recorded_gives)
        .collect()
}

/// The one give these scenarios drive everywhere, with the recording's own answer.
fn a_give_with_an_answer() -> RecordedGive {
    every_recorded_give()
        .into_iter()
        .find(|g| g.reply.is_some())
        .expect("the corpus holds at least one give the shard answered, or this family is blind")
}

/// Every recorded failure the shard sent, in any recording -- the shape the refusal scenario
/// builds its own answer from rather than inventing one.
fn a_recorded_failure() -> Vec<u8> {
    const SERVER_SAYS_ATTEMPT_FAILED: u32 = 0x00A0;
    for session in EVERY_SESSION {
        let corpus = Corpus::load(session)
            .expect("the recording parses")
            .expect("the corpus carries this recording");
        for b in &corpus.blobs {
            if b.dir == dereth_client_net::client_session::testing::Direction::ServerToClient
                && b.payload.len() >= 24
                && dword(&b.payload, 0) == ORDERED_EVENT
                && dword(&b.payload, 12) == SERVER_SAYS_ATTEMPT_FAILED
            {
                return b.payload[12..].to_vec();
            }
        }
    }
    panic!(
        "the corpus carries no recorded failure at all, and this scenario's shape comes from one"
    )
}

// ---------------------------------------------------------------------------------------------
// inventory.give.every-recorded-one-is-answered-by-the-move-and-by-nothing-else
// ---------------------------------------------------------------------------------------------

/// Every give the recordings carry is answered by the shard saying the thing is now inside the
/// recipient, and by nothing else.
///
/// This is a claim about what the shard does, and therefore about what the client may wait for,
/// and it is measured over the whole corpus rather than argued. The two nulls are stated with
/// their denominators in the same breath, because a count of zero means nothing without the
/// space it was taken over: no recorded failure names a given thing, and no recorded give is a
/// part of a stack. The two other answers that *could* release the hold are therefore mechanisms
/// the recordings do not witness, and the scenarios below drive them as such and say so.
pub(super) fn every_recorded_give_is_answered_by_the_move_and_nothing_else() {
    let gives = every_recorded_give();
    assert!(
        !gives.is_empty(),
        "the premise: the corpus carries gives at all"
    );

    let answered = gives.iter().filter(|g| g.reply.is_some()).count();
    let every_one_answered = answered == gives.len();
    let partial = gives.iter().filter(|g| g.amount != 1).count();
    let items: Vec<u32> = gives.iter().map(|g| g.item.0).collect();

    // The recipient of a give the recording also created is a creature, which is the arm the
    // client's own gesture takes.
    let every_recipient_is_a_creature = gives.iter().all(|g| {
        let corpus = Corpus::load(g.session)
            .expect("the recording parses")
            .expect("the corpus carries this recording");
        corpus
            .blobs
            .iter()
            .filter(|b| {
                b.dir == dereth_client_net::client_session::testing::Direction::ServerToClient
                    && b.payload.len() >= 8
                    && dword(&b.payload, 0) == 0xF745
            })
            .filter_map(|b| {
                dereth_protocol::read_body_padded::<dereth_protocol::objects::ItemCreateObject>(
                    &b.payload[4..],
                )
                .ok()
            })
            .find(|m| m.0.id == g.target)
            .is_none_or(|m| m.0.wdesc.obj_type == dereth_rules::weenie::item_type::CREATURE)
    });

    // The first null: no recorded failure names a thing a give named.
    let mut failures_against_a_give = 0usize;
    let mut recorded_failures = 0usize;
    for session in EVERY_SESSION {
        let corpus = Corpus::load(session)
            .expect("the recording parses")
            .expect("the corpus carries this recording");
        for b in &corpus.blobs {
            if b.dir == dereth_client_net::client_session::testing::Direction::ServerToClient
                && b.payload.len() >= 24
                && dword(&b.payload, 0) == ORDERED_EVENT
                && dword(&b.payload, 12) == 0x00A0
            {
                recorded_failures += 1;
                if items.contains(&dword(&b.payload, 16)) {
                    failures_against_a_give += 1;
                }
            }
        }
    }
    // A zero with no denominator says nothing: there have to be recorded failures for "none of
    // them answers a give" to mean anything.
    let there_were_failures_to_look_at = recorded_failures > 0;

    let mut c = a_retail_client();
    c.assert_behaviour(
        "inventory.give.every-recorded-one-is-answered-by-the-move-and-by-nothing-else",
        move |_| {
            every_one_answered
                && every_recipient_is_a_creature
                && there_were_failures_to_look_at
                && failures_against_a_give == 0
                && partial == 0
        },
    );
    c.shutdown();
}

// ---------------------------------------------------------------------------------------------
// inventory.give.the-hold-it-takes-refuses-the-next-gesture-in-the-clients-own-words
// ---------------------------------------------------------------------------------------------

/// Handing something over takes the client's one inventory hold, naming the **thing** and not the
/// recipient -- and while it is held the next gesture is refused where the player is standing,
/// with nothing at all reaching the shard.
///
/// The hold is asserted before any release is measured, because a scenario that only measured
/// releases would be green over a client that never took the hold at all: idle before, idle
/// after. What it names matters too, and is the reason the answer that releases it is the one
/// naming the thing.
pub(super) fn a_give_takes_the_hold_and_the_next_gesture_is_refused_in_words() {
    let g = a_give_with_an_answer();
    let mut c = a_retail_client();
    let store = Arc::clone(c.dat_store().expect("a retail client opens the dats"));
    let mut host = a_host_with_a_recipient(&store, g.target);
    let item = host.carry(g.item);
    let other = host.carry(ObjectId(0x2882_0001));

    let sent = host.give(item, g.target);
    let one_give = sent.len() == 1 && is_give(&sent[0]);
    let holds_the_thing = host.lock() == (InventoryRequest::Give, Some(item));

    let refused = host.give(other, g.target);
    let asked_for_nothing = refused.is_empty();
    let said_so = host.lines() == vec![(FEEDBACK_CHANNEL, BUSY.to_string())];
    let hold_undisturbed = host.lock() == (InventoryRequest::Give, Some(item));

    c.assert_behaviour(
        "inventory.give.the-hold-it-takes-refuses-the-next-gesture-in-the-clients-own-words",
        move |_| one_give && holds_the_thing && asked_for_nothing && said_so && hold_undisturbed,
    );
    c.shutdown();
}

// ---------------------------------------------------------------------------------------------
// inventory.give.the-shards-move-releases-the-hold-and-the-next-one-goes-out
// ---------------------------------------------------------------------------------------------

/// The shard saying the thing is now inside the recipient releases the hold, clears the mark the
/// drag left, and lets the next give go out.
///
/// Driven with the recording's own answer, which is the one the corpus says every give gets.
pub(super) fn the_shards_move_releases_the_give_and_the_next_one_goes_out() {
    let g = a_give_with_an_answer();
    let reply = g.reply.clone().expect("chosen for having one");
    assert!(
        reply.len() >= 8,
        "the premise: the recorded answer has its own two fields"
    );
    assert_eq!(
        dword(&reply, 0),
        SERVER_SAYS_CONTAIN_ID,
        "the premise: the recorded answer is one"
    );
    assert_eq!(
        dword(&reply, 4),
        g.item.0,
        "the premise: and it names the thing being held"
    );

    let mut c = a_retail_client();
    let store = Arc::clone(c.dat_store().expect("a retail client opens the dats"));
    let mut host = a_host_with_a_recipient(&store, g.target);
    let item = host.carry(g.item);

    let sent = host.give(item, g.target);
    let held =
        sent.len() == 1 && is_give(&sent[0]) && host.lock() == (InventoryRequest::Give, Some(item));

    host.shard_says(dereth_protocol::Opcode::ITEM_SERVER_SAYS_CONTAIN_ID, reply);
    let released = host.lock() == (InventoryRequest::None, None);
    let mark_cleared = !host.objects.world.weenie(item).expect("seeded").waiting;

    let second = host.carry(ObjectId(0x2882_0011));
    let again = host.give(second, g.target);
    let usable_again = again.len() == 1
        && is_give(&again[0])
        && host.lock() == (InventoryRequest::Give, Some(second));

    c.assert_behaviour(
        "inventory.give.the-shards-move-releases-the-hold-and-the-next-one-goes-out",
        move |_| held && released && mark_cleared && usable_again,
    );
    c.shutdown();
}

// ---------------------------------------------------------------------------------------------
// inventory.give.the-shards-refusal-releases-the-hold-and-says-which-thing
// ---------------------------------------------------------------------------------------------

/// The shard refusing releases the hold too, names the thing where the player is standing, and
/// leaves the client able to try again.
///
/// **The recordings carry no refusal of a give** -- the census scenario above asserts that zero
/// with its denominator -- so the answer here is built into the shape a recorded refusal has,
/// with the held thing's own id in it. That is said rather than implied: the mechanism is the
/// claim, and the corpus is not being made to say something it does not.
pub(super) fn the_shards_refusal_releases_the_give_and_says_which_thing() {
    let g = a_give_with_an_answer();
    let mut blob = a_recorded_failure();
    assert_eq!(
        blob.len(),
        12,
        "the premise: a refusal is its sub-type, the thing and a reason"
    );
    blob[4..8].copy_from_slice(&g.item.0.to_le_bytes());

    let mut c = a_retail_client();
    let store = Arc::clone(c.dat_store().expect("a retail client opens the dats"));
    let mut host = a_host_with_a_recipient(&store, g.target);
    let item = host.carry(g.item);

    let sent = host.give(item, g.target);
    let held =
        sent.len() == 1 && is_give(&sent[0]) && host.lock() == (InventoryRequest::Give, Some(item));

    host.shard_says(
        dereth_protocol::Opcode::CHARACTER_SERVER_SAYS_ATTEMPT_FAILED,
        blob,
    );
    let released = host.lock() == (InventoryRequest::None, None);
    let the_arm_ran = host.inter.stats.attempts_failed == 1;
    let named_the_thing = host
        .lines()
        .iter()
        .any(|(channel, text)| *channel == FEEDBACK_CHANNEL && text.contains("thing"));

    let second = host.carry(ObjectId(0x2882_0021));
    let usable_again = host.give(second, g.target).len() == 1;

    c.assert_behaviour(
        "inventory.give.the-shards-refusal-releases-the-hold-and-says-which-thing",
        move |_| held && released && the_arm_ran && named_the_thing && usable_again,
    );
    c.shutdown();
}

// ---------------------------------------------------------------------------------------------
// inventory.give.part-of-a-stack-is-released-by-the-stacks-new-count
// ---------------------------------------------------------------------------------------------

/// Handing over part of a stack sends the split size and holds the **source** stack, so the
/// answer that releases it is the shard saying what the source stack is now.
///
/// This is the release whose absence wedged a real session after a split, and it is the one the
/// recordings cannot witness -- the census above asserts that every recorded give is a whole
/// thing. So the count is driven through the client's own writer against the stack this
/// scenario seeded, and the zero denominator is said out loud.
pub(super) fn part_of_a_stack_is_released_by_the_stacks_new_count() {
    let mut c = a_retail_client();
    let store = Arc::clone(c.dat_store().expect("a retail client opens the dats"));
    let recipient = ObjectId(0x2882_0031);
    let mut host = a_host_with_a_recipient(&store, recipient);
    let item = host.carry_stack(ObjectId(0x2882_0030), 10);

    // How much of a stack a gesture hands over is read off the splitter for the **selected**
    // thing only, so both halves are driven: the selection, and the splitter's own producer.
    host.objects.world.selected = Some(item);
    host.inter.queue(
        Vec::new(),
        vec![UiRequest::StackSliderChanged { split: 3, max: 10 }],
    );
    host.frame();

    let sent = host.give(item, recipient);
    let split_size = matches!(
        sent.first(),
        Some(dereth_client_model::Request::GiveObjectRequest(m)) if m.amount == 3
    );
    let holds_the_source = host.lock() == (InventoryRequest::Give, Some(item));

    // The shard's new count for the source stack.
    let mut blob = 0x0197u32.to_le_bytes().to_vec();
    blob.push(1);
    blob.extend_from_slice(&item.0.to_le_bytes());
    blob.extend_from_slice(&7u32.to_le_bytes());
    blob.extend_from_slice(&70u32.to_le_bytes());
    host.shard_says(dereth_protocol::Opcode::ITEM_UPDATE_STACK_SIZE, blob);

    let the_arm_ran = host.inter.stats.stack_sizes_applied == 1;
    let counted_down = host
        .objects
        .world
        .weenie(item)
        .and_then(|w| w.pwd.stack_size)
        == Some(7);
    let released = host.lock() == (InventoryRequest::None, None);

    c.assert_behaviour(
        "inventory.give.part-of-a-stack-is-released-by-the-stacks-new-count",
        move |_| split_size && holds_the_source && the_arm_ran && counted_down && released,
    );
    c.shutdown();
}

// ---------------------------------------------------------------------------------------------
// inventory.give.the-contents-of-a-container-being-viewed-do-not-release-one
// ---------------------------------------------------------------------------------------------

/// The shard listing the contents of a container the player has open does **not** release a
/// give, because that answer names a container and a give holds a thing.
///
/// The route has to be reachable or the claim is about the wrong thing, so the container is
/// really opened first and the answer really is the one for it; the arm is asserted to have run
/// and to have reached its own tail. Then the answer that *can* release the give is made
/// afterwards and does, so what is being shown is the route and not a hold this scenario has
/// broken.
pub(super) fn a_container_being_viewed_does_not_release_a_give() {
    let g = a_give_with_an_answer();
    let mut c = a_retail_client();
    let store = Arc::clone(c.dat_store().expect("a retail client opens the dats"));
    let mut host = a_host_with_a_recipient(&store, g.target);
    let item = host.carry(g.item);

    let chest = ObjectId(0x2882_0040);
    {
        let mut w = dereth_client_model::Weenie::new(chest);
        w.pwd.obj_type = dereth_rules::weenie::item_type::CONTAINER;
        w.pwd.bitfield = dereth_rules::weenie::bitfield::OPENABLE;
        w.pwd.items_capacity = Some(10);
        w.pwd.useability = Some(0x20);
        w.pwd.name = "Storage Chest".to_owned();
        host.objects.world.tables.weenies.insert(chest, w);
    }
    host.open_ground_container(chest);
    assert_eq!(
        host.objects.world.requested_ground_object,
        Some(chest),
        "the premise: the container really is the one the player asked about"
    );

    let sent = host.give(item, g.target);
    let held =
        sent.len() == 1 && is_give(&sent[0]) && host.lock() == (InventoryRequest::Give, Some(item));

    let mut blob = 0x0196u32.to_le_bytes().to_vec();
    blob.extend_from_slice(&chest.0.to_le_bytes());
    blob.extend_from_slice(&0u32.to_le_bytes());
    host.shard_says(dereth_protocol::Opcode::ITEM_ON_VIEW_CONTENTS, blob);

    let the_arm_ran =
        host.inter.stats.contents_viewed == 1 && host.inter.stats.ground_panels_opened == 1;
    let hold_untouched = host.lock() == (InventoryRequest::Give, Some(item));

    host.shard_says(
        dereth_protocol::Opcode::ITEM_SERVER_SAYS_CONTAIN_ID,
        g.reply.clone().expect("chosen for having one"),
    );
    let and_the_one_that_can_still_does = host.lock() == (InventoryRequest::None, None);

    c.assert_behaviour(
        "inventory.give.the-contents-of-a-container-being-viewed-do-not-release-one",
        move |_| held && the_arm_ran && hold_untouched && and_the_one_that_can_still_does,
    );
    c.shutdown();
}
