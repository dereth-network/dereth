//! An item the server contains before it is created lands in the slot the server named, and the
//! pack ends up in the server's reply order rather than the order the creates arrived.
//! Fixture: every recording in `fixtures/packet-captures/`, replayed through `ClientNetwork`,
//! `ObjectStream` and `interaction`, entering the world wherever each recording did.
//!
//! `0x0022 Item_ServerSaysContainID` has two branches: an item that exists is moved to
//! `(container, slot)`; an item that does not exist yet is **pre-placed** — its id is inserted at
//! the named slot of the container's item or container list (chosen by `props`), provided the
//! client holds a viewed inventory for that container. When the item's `0xF745` arrives, the
//! create path reads the id's existing position, removes it and adds it back at that index, so
//! the item stays in the server's slot. Without the pre-placement the lookup answers `-1`, the
//! create substitutes `0`, and every such item lands **at the head of the pack**.
//!
//! Samples are taken immediately before and after each single `0x0022`, and on the frame the
//! item's own create lands — never at the end of the replay, because the session-end reset empties
//! the world at log-off and because the claim is about the interval between reply and create.
//! Several replies share one datagram, so `interaction::apply_events` is driven one event at a
//! time, as the client handles one blob at a time.

use crate::common::{
    captures_dir, recorded_enter_world_requests, recorded_sessions, recorded_world_sessions,
    RecordedEntry,
};

use std::collections::{BTreeMap, BTreeSet};
use std::net::SocketAddr;

use dereth_client_net::client_session::SessionEvent;
use dereth_client_runtime::net::ClientNetwork;
use dereth_client_runtime::objects::ObjectStream;
use dereth_primitives::{LocalTime, ObjectId};
use dereth_protocol::{Message, Opcode};
use {dereth_client_runtime::interaction, dereth_client_runtime::interaction::Interaction};

// ---------------------------------------------------------------------------------------------
// Harness. `fixtures/packet-captures` is tracked in the repository, so a missing directory is a
// broken checkout and never a reason to pass. The datagrams come from the shared capture reader.
// ---------------------------------------------------------------------------------------------

use dereth_client_net::client_session::testing::capture::{self, Datagram as Record};

fn corpus_sessions() -> Vec<String> {
    let dir = captures_dir();
    let mut out: Vec<String> = std::fs::read_dir(&dir)
        .unwrap_or_else(|e| panic!("{}: {e}", dir.display()))
        .filter_map(Result::ok)
        .filter(|e| !crate::common::is_unclean_logout_recording(&e.path()))
        .filter_map(|e| {
            let p = e.path();
            if p.extension().and_then(|x| x.to_str()) != Some("jsonl") {
                return None;
            }
            p.file_stem().and_then(|x| x.to_str()).map(str::to_owned)
        })
        .collect();
    out.sort();
    // The number is the corpus index's own rather than an integer written here, so a new
    // recording re-measures this scan instead of failing it.
    assert_eq!(
        out.len(),
        recorded_sessions(),
        "the recorded captures; found {out:?}"
    );
    out
}

fn load(session: &str) -> Vec<Record> {
    capture::shared_session(session).to_vec()
}

fn connection_sequence_number(records: &[Record]) -> u32 {
    dereth_client_net::recording::connection_sequence_number(records).unwrap_or(0)
}

fn addr(pair: u16) -> SocketAddr {
    capture::peer(pair)
}

/// Every enter-world the recorded client performed, keyed by its index in `records`.
fn recorded_entries(records: &[Record]) -> Vec<RecordedEntry> {
    recorded_enter_world_requests(
        records
            .iter()
            .enumerate()
            .filter(|(_, r)| r.c2s)
            .map(|(i, r)| (i, r.raw.as_slice())),
    )
}

/// Enter the world at the client-to-server datagram where the recorded client asked to, as the
/// character it named.
///
/// `requested-death-vitae-salvage` logs off and re-enters the world six times on one connection;
/// a replay that entered only once would leave the client at the character screen for every later
/// cycle, so none of that recording's player descriptions would seed a pack and its replies would
/// name a container the client has no model of.
fn enter_where_the_recording_did(
    net: &mut ClientNetwork,
    entries: &mut std::iter::Peekable<std::vec::IntoIter<RecordedEntry>>,
    record: usize,
) {
    if let Some(e) = entries.next_if(|e| e.record == record) {
        net.enter_world(e.character, &e.account);
    }
}

// ---------------------------------------------------------------------------------------------
// The census: one row per recorded `0x0022`, sampled around that one message.
// ---------------------------------------------------------------------------------------------

/// Where an id sits in one of a container's two ordered lists, and who is on either side of it.
///
/// The neighbours are kept rather than only the index because an index moves for an honest reason:
/// another pre-placement inserting ahead of this one shifts it down by exactly one, and that is not
/// the create having moved it. What must not change is **who this id is behind and who it is in
/// front of**.
#[derive(Debug, Clone)]
struct Placed {
    /// `true` selects the container list; `false` selects the item list.
    containers: bool,
    index: usize,
    len: usize,
    /// How many times the id appears across **both** lists. Anything but 1 is a defect: 2 means
    /// the create left a duplicate instead of preserving exactly one copy.
    occurrences: usize,
    before: Vec<ObjectId>,
    after: Vec<ObjectId>,
}

fn placement(
    world: &dereth_client_model::World,
    container: ObjectId,
    item: ObjectId,
) -> Option<Placed> {
    let inv = world.inventory(container)?;
    let occurrences = inv.items.iter().filter(|x| **x == item).count()
        + inv.containers.iter().filter(|x| **x == item).count();
    if occurrences == 0 {
        return None;
    }
    let (containers, list) = if inv.items.contains(&item) {
        (false, &inv.items)
    } else {
        (true, &inv.containers)
    };
    let index = list.iter().position(|x| *x == item)?;
    Some(Placed {
        containers,
        index,
        len: list.len(),
        occurrences,
        before: list[..index].to_vec(),
        after: list[index + 1..].to_vec(),
    })
}

/// The length of the selected list **before** the reply is applied, and therefore the clamp on the
/// slot the server named.
fn list_len(
    world: &dereth_client_model::World,
    container: ObjectId,
    containers_list: bool,
) -> Option<usize> {
    let inv = world.inventory(container)?;
    Some(if containers_list {
        inv.containers.len()
    } else {
        inv.items.len()
    })
}

/// One recorded `0x0022 Item_ServerSaysContainID` and everything measured around it.
#[derive(Debug, Clone)]
struct Case {
    session: String,
    /// Position in the recording's `0x0022` stream — the **server's** order.
    reply_rank: usize,
    item: ObjectId,
    container: ObjectId,
    slot: u32,
    props: u32,
    /// The item already had a `Weenie` when the reply was applied: the move branch.
    item_existed: bool,
    /// Whether the named container object already existed.
    container_existed: bool,
    /// Whether the container had a viewed-inventory model; the client keeps one only while viewing it.
    container_had_inventory: bool,
    /// Length of the list selected by `props`, immediately before the reply.
    len_before: Option<usize>,
    /// Where the id sat immediately after the reply was applied.
    after_reply: Option<Placed>,
    /// Whether the item still had no `Weenie` immediately after the reply — the whole point.
    still_uncreated_after_reply: bool,
    /// How many `Notice`s the one reply raised. The second branch ends in exactly one movement
    /// notice carrying `(itemId, 0, 0, 0, containerId, slot, 0, 0)` because
    /// there is no object whose attributes could also have changed.
    notice_delta: u64,
    /// Where it sat on the frame its own create landed, and the item's own list predicate then.
    at_create: Option<Placed>,
    /// The whole list it sat in on that frame, for the order test.
    list_at_create: Option<Vec<ObjectId>>,
    goes_in_containers_list: Option<bool>,
    /// Position of this item's first `0xF745 Item_CreateObject` in the recording's create
    /// stream, counted in message order (several creates can share one datagram).
    create_rank: Option<usize>,
    /// On the frame this item's create landed: the other pre-placed ids of the same list that were
    /// no longer in it although no server message had named them since their own reply.
    left_unexplained: Vec<ObjectId>,
    /// How many server messages other than its own create named the item after its reply, over
    /// the whole recording: 0 means nothing but the reply and the create ever placed it.
    touched_after_reply: usize,
}

/// Replay one recording through the application's own path and return every `0x0022` in it.
///
/// `ClientNetwork::feed` → `ObjectStream::pump` → `Hud::apply_events` → `interaction::apply_events` is
/// the order `App::frame` runs them in, and the HUD leg is load-bearing: `0x0013`'s
/// `content_profiles` is the only thing that ever seeds the player's own `ObjectInventory`, and
/// pre-placement does nothing at all to a container that has none. Without that leg this
/// test would measure the absence of a pack rather than the placement of an item in one.
fn census(session: &str) -> Vec<Case> {
    let records = load(session);
    let csn = connection_sequence_number(&records);
    let mut net = ClientNetwork::new("127.0.0.1:19000", 7304, "ac01", "pass", csn)
        .expect("a replay client network");
    let mut objects = ObjectStream::new();
    let mut inter = Interaction::new();
    let mut hud = dereth_client_shell::hud::Hud::new();
    let mut entries = recorded_entries(&records).into_iter().peekable();
    let mut cases: Vec<Case> = Vec::new();
    let mut awaiting: BTreeMap<ObjectId, usize> = BTreeMap::new();
    // Every created id, by the position of its first create in message order.
    let mut create_order: BTreeMap<ObjectId, usize> = BTreeMap::new();
    // Per pre-placed id, how many server messages have named it since its own reply.
    let mut named_since_reply: BTreeMap<ObjectId, usize> = BTreeMap::new();
    // The same, less each item's own `0xF745` create.
    let mut touched_after_reply: BTreeMap<ObjectId, usize> = BTreeMap::new();

    for (i, r) in records.iter().enumerate() {
        if r.c2s {
            enter_where_the_recording_did(&mut net, &mut entries, i);
            continue;
        }
        net.feed(&r.raw, addr(r.pair), LocalTime(r.t));
        net.tick(LocalTime(r.t));
        let _ = net.take_outgoing();
        let events = objects.pump(&mut net, LocalTime(r.t));
        for e in &events {
            if let SessionEvent::WorldObject { opcode, body } = e {
                if *opcode == Opcode::ITEM_CREATE_OBJECT {
                    if let Some(b) = body.get(0..4) {
                        let id = ObjectId(u32::from_le_bytes([b[0], b[1], b[2], b[3]]));
                        let next = create_order.len();
                        create_order.entry(id).or_insert(next);
                    }
                }
            }
        }

        // `ObjectStream::pump` has already applied this datagram's creates, so an outstanding
        // pre-placement may have been resolved on this very frame. Sample those first, before any
        // of this frame's `0x0022` arms can touch the same lists.
        let resolved: Vec<(ObjectId, usize)> = awaiting
            .iter()
            .filter(|(id, _)| objects.world.weenie(**id).is_some())
            .map(|(id, i)| (*id, *i))
            .collect();
        for (id, i) in resolved {
            awaiting.remove(&id);
            let container = cases[i].container;
            cases[i].at_create = placement(&objects.world, container, id);
            let containers_list = cases[i].props != 0;
            cases[i].list_at_create = objects.world.inventory(container).map(|inv| {
                if containers_list {
                    inv.containers.clone()
                } else {
                    inv.items.clone()
                }
            });
            cases[i].goes_in_containers_list = objects
                .world
                .weenie(id)
                .map(dereth_client_model::weenie::Weenie::goes_in_containers_list);
            cases[i].create_rank = create_order.get(&id).copied();
            let props = cases[i].props;
            let list = cases[i].list_at_create.clone().unwrap_or_default();
            cases[i].left_unexplained = cases
                .iter()
                .filter(|c| c.container == container && c.props == props && c.item != id)
                .filter(|c| !list.contains(&c.item))
                .filter(|c| named_since_reply.get(&c.item).copied().unwrap_or(0) == 0)
                .map(|c| c.item)
                .collect();
        }

        // Every server message whose first field is an object id counts as naming that object.
        for e in &events {
            let first = match e {
                SessionEvent::UiEvent { .. } => e.ui_body().and_then(|(_, body)| body.get(..4)),
                SessionEvent::WorldObject { body, .. } => body.get(0..4),
                _ => None,
            };
            if let Some(b) = first {
                let id = ObjectId(u32::from_le_bytes([b[0], b[1], b[2], b[3]]));
                if let Some(n) = named_since_reply.get_mut(&id) {
                    *n += 1;
                }
                let own_create = matches!(e, SessionEvent::WorldObject { opcode, .. }
                    if *opcode == Opcode::ITEM_CREATE_OBJECT);
                if !own_create {
                    if let Some(n) = touched_after_reply.get_mut(&id) {
                        *n += 1;
                    }
                }
            }
        }

        let _ = hud.apply_events(&events, &mut objects.world);

        // One event at a time: the interaction handler runs per blob, and three replies in one
        // datagram are three separate inserts into the same list.
        for e in &events {
            let reply = match e {
                SessionEvent::UiEvent { opcode, .. }
                    if *opcode == Opcode::ITEM_SERVER_SAYS_CONTAIN_ID =>
                {
                    e.ui_body().and_then(|(_, b)| {
                        let mut rd = dereth_protocol::archive::Reader::new(b);
                        dereth_protocol::objects::ItemServerSaysContainId::read(&mut rd).ok()
                    })
                }
                _ => None,
            };
            let Some(m) = reply else {
                interaction::apply_events(&mut inter, std::slice::from_ref(e), &mut objects.world);
                continue;
            };
            let w = &objects.world;
            let item_existed = w.weenie(m.item).is_some();
            let container_existed = w.weenie(m.container).is_some();
            let container_had_inventory = w.inventory(m.container).is_some();
            let len_before = list_len(w, m.container, m.container_properties != 0);
            let reply_rank = cases.len() + 1;
            let notices_before = inter.stats.notices;

            interaction::apply_events(&mut inter, std::slice::from_ref(e), &mut objects.world);

            let case = Case {
                session: session.to_string(),
                reply_rank,
                item: m.item,
                container: m.container,
                slot: m.slot,
                props: m.container_properties,
                item_existed,
                container_existed,
                container_had_inventory,
                len_before,
                after_reply: placement(&objects.world, m.container, m.item),
                still_uncreated_after_reply: objects.world.weenie(m.item).is_none(),
                notice_delta: inter.stats.notices - notices_before,
                at_create: None,
                list_at_create: None,
                goes_in_containers_list: None,
                create_rank: None,
                left_unexplained: Vec::new(),
                touched_after_reply: 0,
            };
            if !item_existed {
                awaiting.insert(m.item, cases.len());
            }
            // A pre-placing reply was counted above; only later messages explain a departure. A
            // reply that moves an existing item is itself such a message.
            if !item_existed {
                named_since_reply.insert(m.item, 0);
                touched_after_reply.insert(m.item, 0);
            }
            cases.push(case);
        }
    }
    // The counter the arm itself keeps, checked against the census taken from outside it. Two
    // instruments that could disagree; if they ever do, one of them is lying.
    let hard = cases
        .iter()
        .filter(|c| !c.item_existed && c.container_had_inventory)
        .count();
    assert_eq!(
        inter.stats.contain_ids_preplaced, hard as u64,
        "{session}: the arm counted {} pre-placements and the census found {hard}",
        inter.stats.contain_ids_preplaced
    );
    for c in &mut cases {
        c.touched_after_reply = touched_after_reply.get(&c.item).copied().unwrap_or(0);
    }
    cases
}

fn whole_corpus() -> Vec<Case> {
    let mut all = Vec::new();
    for s in corpus_sessions() {
        all.extend(census(&s));
    }
    assert!(
        !all.is_empty(),
        "the recordings carry no `0x0022 Item_ServerSaysContainID` reply: the scan read nothing"
    );
    all
}

/// Every reply whose item did not exist yet: **this module's subject**, and the denominator every
/// other test here quotes.
fn hard_cases(all: &[Case]) -> Vec<&Case> {
    all.iter().filter(|c| !c.item_existed).collect()
}

// ---------------------------------------------------------------------------------------------
// 0. The harness enters the world where each recording did.
// ---------------------------------------------------------------------------------------------

/// **Which character, how many times.** The census below is only as good as the replayed client's
/// world, and the world is only there if the harness entered it on every cycle the recording did.
///
/// Read from the raw datagrams by the shared recording reader and pinned here against the reassembled
/// message corpus's own `0xF657` blobs, which were read separately. Two recordings enter more than
/// once: `pre-relog-play` twice as the same character, and `requested-death-vitae-salvage` six
/// times on three characters -- a recording an enter-once replay could not follow.
#[test]
fn the_harness_enters_the_world_where_each_recording_did() {
    let mut got: Vec<(String, Vec<u32>)> = Vec::new();
    for s in corpus_sessions() {
        let entries = recorded_entries(&load(&s));
        for e in &entries {
            assert!(!e.account.is_empty(), "{s}: a `0xF657` with no account");
        }
        got.push((s, entries.iter().map(|e| e.character.0).collect()));
    }
    // Every recording that reaches the world enters it at least once, and one recording
    // re-enters on the same connection, so a replay that entered only once would be caught.
    let entering = got.iter().filter(|(_, v)| !v.is_empty()).count();
    assert_eq!(
        entering,
        recorded_world_sessions(),
        "every recording whose client reached the world entered it: {got:?}"
    );
    assert!(
        got.iter().any(|(_, v)| v.len() > 1),
        "no recording re-enters the world, so the re-entry path is not exercised: {got:?}"
    );
}

// ---------------------------------------------------------------------------------------------
// 1. The census.
// ---------------------------------------------------------------------------------------------

/// What the corpus actually contains, printed with its denominators.
///
/// The count is asserted, not merely printed, because a loop that stopped running would otherwise
/// read as a pass in every test below. A mutation that does not apply is the same silent instrument
/// as a test that skips.
#[test]
fn every_item_contained_before_it_was_created_names_a_viewed_container_and_is_created_later() {
    let all = whole_corpus();
    let hard = hard_cases(&all);
    let mut per_session: BTreeMap<&str, (usize, usize)> = BTreeMap::new();
    for c in &all {
        let e = per_session.entry(c.session.as_str()).or_default();
        e.0 += 1;
        if !c.item_existed {
            e.1 += 1;
        }
    }
    for (s, (n, h)) in &per_session {
        eprintln!("census: {s}: {n} `0x0022`, {h} naming an item that did not exist yet");
    }
    for c in &hard {
        eprintln!(
            "hard case: {} #{}: item {:#010X} -> container {:#010X} slot {} props {} \
             len_before {:?} -> index {:?} of {:?}; created #{:?} -> index {:?}, dup {:?}",
            c.session,
            c.reply_rank,
            c.item.0,
            c.container.0,
            c.slot,
            c.props,
            c.len_before,
            c.after_reply.as_ref().map(|p| p.index),
            c.after_reply.as_ref().map(|p| p.len),
            c.create_rank,
            c.at_create.as_ref().map(|p| p.index),
            c.at_create.as_ref().map(|p| p.occurrences),
        );
    }
    let with_container = hard.iter().filter(|c| c.container_existed).count();
    let with_inventory = hard.iter().filter(|c| c.container_had_inventory).count();
    let created = hard.iter().filter(|c| c.at_create.is_some()).count();
    let slots: BTreeSet<u32> = all.iter().map(|c| c.slot).collect();
    eprintln!(
        "{} `0x0022` in the corpus; {} name an item with no weenie yet; {with_container} of \
         those name a known container; {with_inventory} a container the client is viewing; \
         {created} were created before the recording ended. Slots named: {slots:?}",
        all.len(),
        hard.len(),
    );
    // Every recording that carries a `0x0022` is in the census, and no recording names more
    // not-yet-created items than it carries replies.
    for (s, (n, h)) in &per_session {
        assert!(h <= n, "{s}: {h} not-yet-created items out of {n} replies");
    }
    assert!(
        !hard.is_empty(),
        "the corpus carries no item contained before it was created"
    );
    assert_eq!(
        with_container,
        hard.len(),
        "every one names a container the client already had"
    );
    // A replay that entered the world only once would hold no pack model for the containers of
    // a recording that re-enters, so these cases would not be reachable.
    assert_eq!(
        with_inventory,
        hard.len(),
        "and one it was viewing, so the pre-placement is reachable"
    );
    assert_eq!(
        created,
        hard.len(),
        "and every one of those items was created later in the recording"
    );
}

// ---------------------------------------------------------------------------------------------
// 2. The pre-placement itself.
// ---------------------------------------------------------------------------------------------

/// Behaviour: inventory.contain.an-item-contained-before-it-is-created-lands-in-the-named-slot
/// **The row's claim, first half.** A `0x0022` whose item has no weenie yet puts the id in the
/// container's list **at the slot the server named**, while the item still does not exist.
///
/// `min(slot, len)`, with `len` the list's current length, is the original ordered-list insertion
/// arithmetic and not a convenience: with no node at `num` and `num != len`, insertion falls back
/// to appending.
///
/// Removal check: deleting the `server_says_contain_id` call from `interaction.rs`'s second branch
/// fails this on every case with `after_reply = None`.
#[test]
fn an_item_contained_before_it_exists_is_recorded_in_the_slot_the_server_named() {
    let all = whole_corpus();
    let hard = hard_cases(&all);
    let mut checked = 0usize;
    for c in &hard {
        let len_before = c
            .len_before
            .unwrap_or_else(|| panic!("{}: {:#010X} has no pack model", c.session, c.container.0));
        let p = c.after_reply.as_ref().unwrap_or_else(|| {
            panic!(
                "{} #{}: item {:#010X} was not pre-placed into container {:#010X} at slot {}",
                c.session, c.reply_rank, c.item.0, c.container.0, c.slot
            )
        });
        assert!(
            c.still_uncreated_after_reply,
            "{} #{}: {:#010X} must still have no weenie — that is what makes this the hard branch",
            c.session, c.reply_rank, c.item.0
        );
        assert_eq!(
            p.containers,
            c.props != 0,
            "{} #{}: props {} chooses the {} list",
            c.session,
            c.reply_rank,
            c.props,
            if c.props != 0 { "containers" } else { "items" }
        );
        assert_eq!(
            p.index,
            (c.slot as usize).min(len_before),
            "{} #{}: item {:#010X} landed at index {} of container {:#010X}; the server said slot \
             {} and the list held {len_before}",
            c.session,
            c.reply_rank,
            c.item.0,
            p.index,
            c.container.0,
            c.slot
        );
        assert_eq!(
            p.occurrences, 1,
            "{} #{}: pre-placed twice",
            c.session, c.reply_rank
        );
        // The movement-notice helper runs on the way out of the second branch, and it
        // is the only notice that branch can raise. Without it a panel showing the pack has no
        // reason to redraw and the id sits in the model unseen.
        assert_eq!(
            c.notice_delta, 1,
            "{} #{}: the second branch must raise exactly one `ItemMoved` notice, not {}",
            c.session, c.reply_rank, c.notice_delta
        );
        checked += 1;
    }
    eprintln!("{checked} contained-before-created items landed in the server's slot");
    assert!(
        checked > 0,
        "the loop checked no contained-before-created item"
    );
}

/// **The replies that name a slot other than the head.** Most name slot 0, where the head of the
/// pack and the server's slot are the same index and a wrong placement is invisible. Every reply
/// that names a deeper slot must land there, and keep it through its item's create.
///
/// Removal check: without the pre-placement they land at index 0 instead of their slots.
#[test]
fn every_reply_that_names_a_slot_other_than_the_head_is_honoured() {
    let all = whole_corpus();
    let named: Vec<&Case> = hard_cases(&all)
        .into_iter()
        .filter(|c| c.slot != 0)
        .collect();
    let got: Vec<(u32, u32, usize)> = named
        .iter()
        .map(|c| {
            let p = c.after_reply.as_ref().expect("pre-placed");
            (c.item.0, c.slot, p.index)
        })
        .collect();
    for c in &named {
        eprintln!(
            "{} #{}: {:#010X} -> {:#010X} slot {}, landed at {:?}",
            c.session,
            c.reply_rank,
            c.item.0,
            c.container.0,
            c.slot,
            c.after_reply.as_ref().map(|p| p.index)
        );
    }
    assert!(
        !named.is_empty(),
        "no recorded reply names a slot other than the head, so a wrong placement is invisible"
    );
    for (item, slot, index) in &got {
        assert_eq!(
            *index, *slot as usize,
            "{item:#010X} landed at index {index}; the server named slot {slot}"
        );
    }
    for c in &named {
        let at = c.at_create.as_ref().expect("created later");
        assert_eq!(
            at.index, c.slot as usize,
            "{}: {:#010X} was moved off slot {} by its own create",
            c.session, c.item.0, c.slot
        );
    }
}

// ---------------------------------------------------------------------------------------------
// 3. The other end of the seam: the create must not disturb it.
// ---------------------------------------------------------------------------------------------

/// **The row's claim, second half.** The item's own `0xF745` must leave it where the pre-placement
/// put it: the create path reads the existing list position before removing and re-adding the id.
///
/// The invariant asserted is *relative*, not the raw index: another pre-placement landing ahead of
/// this one between the reply and the create shifts the index down by one honestly. What may not
/// happen is the item changing sides with anything. Every id that was in front of it stays in
/// front; every id behind it stays behind.
///
/// Removal check: making the create path stop asking the container where the id already is — the
/// `-1 → 0` fallback for every item — reddens this and `every_reply_that_names_a_slot_other_than_the_head_is_honoured` with it, because the create then re-inserts at the head.
///
/// Dropping the add step's already-in-list guard does **not** redden it, and that is a fact about
/// the client rather than a weakness here: removal has already taken the id out by then. See the
/// note on `ObjectInventory::add_content`.
#[test]
fn the_create_that_follows_leaves_the_item_in_its_pre_placed_slot() {
    let all = whole_corpus();
    let hard = hard_cases(&all);
    let mut checked = 0usize;
    for c in &hard {
        let before = c.after_reply.as_ref().expect("pre-placed");
        let at = c.at_create.as_ref().unwrap_or_else(|| {
            panic!(
                "{} #{}: {:#010X} was never created",
                c.session, c.reply_rank, c.item.0
            )
        });
        assert_eq!(
            at.occurrences, 1,
            "{} #{}: {:#010X} appears {} times in {:#010X} after its create; the \
             duplicate-after-create invariant requires exactly one copy",
            c.session, c.reply_rank, c.item.0, at.occurrences, c.container.0
        );
        assert_eq!(
            at.containers, before.containers,
            "{} #{}: {:#010X} changed list across its create",
            c.session, c.reply_rank, c.item.0
        );
        for id in &before.before {
            assert!(
                at.before.contains(id),
                "{} #{}: {:#010X} overtook {:#010X}, which was ahead of it when the server said \
                 slot {}",
                c.session,
                c.reply_rank,
                c.item.0,
                id.0,
                c.slot
            );
        }
        for id in &before.after {
            assert!(
                at.after.contains(id) || !at.before.contains(id),
                "{} #{}: {:#010X} fell behind {:#010X}, which was behind it when the server said \
                 slot {}",
                c.session,
                c.reply_rank,
                c.item.0,
                id.0,
                c.slot
            );
        }
        checked += 1;
    }
    eprintln!("{checked} pre-placements survived their item's create unmoved");
    assert!(checked > 0, "the loop checked no pre-placement");
}

/// **Both ends of the list choice, checked against each other.** The pre-placement picks a list
/// from the server's `containerProperties`; the create selects the container list when the item's
/// bit 23 (`0x800000`, requires a pack slot) is set or either item/container capacity is nonzero.
/// The current `Weenie::goes_in_containers_list` implements this test; it is different from merely asking
/// whether the object is a container.
///
/// The two are read off different things at different times and nothing forces them to agree. If
/// they ever disagreed the client would put the id in one list and then add it to the other, so
/// this is the assertion that says the seam is sound rather than merely that it did not crash.
///
/// Two are side packs (`props = 1`) and the rest plain items, so both directions are
/// exercised — a one-sided check would pass on a build that answered `false` unconditionally.
#[test]
fn the_servers_props_word_agrees_with_the_items_own_list_predicate() {
    let all = whole_corpus();
    let hard = hard_cases(&all);
    let mut side_packs = 0usize;
    let mut plain = 0usize;
    for c in &hard {
        let predicate = c.goes_in_containers_list.unwrap_or_else(|| {
            panic!(
                "{} #{}: {:#010X} was never created",
                c.session, c.reply_rank, c.item.0
            )
        });
        assert_eq!(
            predicate,
            c.props != 0,
            "{} #{}: the server sent props {} for {:#010X}, whose own list-selection predicate says \
             {}",
            c.session,
            c.reply_rank,
            c.props,
            c.item.0,
            predicate
        );
        if predicate {
            side_packs += 1;
        } else {
            plain += 1;
        }
    }
    eprintln!("list choice agreed on {plain} plain items and {side_packs} side packs");
    // Both directions have a natural instance, which is what this test needs.
    assert!(plain > 0, "no plain item exercises the items list");
    assert!(
        side_packs > 0,
        "both directions must be exercised or this proves nothing"
    );
}

// ---------------------------------------------------------------------------------------------
// 4. The two guards the corpus never fires, driven with the corpus's own bytes.
// ---------------------------------------------------------------------------------------------

/// The recording's own `0x0022` bodies, so the two guard tests below use real bytes rather than a
/// message this test wrote for itself to agree with.
fn recorded_contain_id_blobs(session: &str) -> Vec<Vec<u8>> {
    let records = load(session);
    let csn = connection_sequence_number(&records);
    let mut net = ClientNetwork::new("127.0.0.1:19000", 7304, "ac01", "pass", csn)
        .expect("a replay client network");
    let mut objects = ObjectStream::new();
    let mut entries = recorded_entries(&records).into_iter().peekable();
    let mut out = Vec::new();
    for (i, r) in records.iter().enumerate() {
        if r.c2s {
            enter_where_the_recording_did(&mut net, &mut entries, i);
            continue;
        }
        net.feed(&r.raw, addr(r.pair), LocalTime(r.t));
        net.tick(LocalTime(r.t));
        let _ = net.take_outgoing();
        for e in objects.pump(&mut net, LocalTime(r.t)) {
            if let SessionEvent::UiEvent { opcode, blob } = e {
                if opcode == Opcode::ITEM_SERVER_SAYS_CONTAIN_ID {
                    out.push(blob);
                }
            }
        }
    }
    assert!(!out.is_empty(), "{session} carries no `0x0022`");
    out
}

/// The pre-placement path has two early returns, **neither of which the corpus ever takes** — every
/// hard case names a container the client both knows and is viewing.
///
/// Stated out loud rather than left as an unevaluated branch: a zero needs a denominator. The
/// bytes are the recording's; only the world they land in is arranged, and it
/// is arranged to the two states the client's own guards test:
///
/// * no object exists for `containerId`;
/// * a known container has no viewed-inventory model, which is the normal
///   state of every chest in the world.
///
/// Removal check: deleting either guard fails this.
#[test]
fn a_reply_about_a_container_the_client_is_not_viewing_places_nothing() {
    let blobs = recorded_contain_id_blobs("long-solo-play");
    let mut unknown_container = 0usize;
    let mut not_viewing = 0usize;
    let mut opened = 0usize;
    for blob in &blobs {
        let mut rd = dereth_protocol::archive::Reader::new(&blob[4..]);
        let m = dereth_protocol::objects::ItemServerSaysContainId::read(&mut rd)
            .expect("a recorded `0x0022` must decode");

        // (a) No object exists for `containerId`. The world is given an `ObjectInventory` for the
        // id but no object, so the object-existence guard has to refuse. An empty world would be
        // refused by the viewed-inventory guard below and prove nothing about this one.
        let mut world = dereth_client_model::World::new();
        world.view_object_contents(m.container, &[], &mut dereth_client_model::NullSink);
        assert!(world.inventory(m.container).is_some(), "the list exists");
        assert!(world.weenie(m.container).is_none(), "the object does not");
        assert!(
            !world.server_says_contain_id(m.container, m.item, m.slot, m.container_properties),
            "a `0x0022` about an object the client has never heard of must place nothing"
        );
        let inv = world.inventory(m.container).expect("still there");
        assert!(
            inv.items.is_empty() && inv.containers.is_empty(),
            "and must place it nowhere"
        );
        unknown_container += 1;

        // (b) the container exists but the client is not viewing it, so it has no
        // viewed inventory model. Viewing object contents is the only operation that creates one;
        // stopping that view removes it again.
        let mut world = dereth_client_model::World::new();
        world
            .create_or_merge(
                &dereth_protocol::objects::ObjectCreatePayload {
                    id: m.container,
                    ..Default::default()
                },
                dereth_primitives::ServerTime(0.0),
                &mut dereth_client_model::NullSink,
            )
            .expect("the container becomes a known object");
        assert!(
            world.weenie(m.container).is_some(),
            "the container is known"
        );
        assert!(
            world.inventory(m.container).is_none(),
            "and not being viewed"
        );
        assert!(
            !world.server_says_contain_id(m.container, m.item, m.slot, m.container_properties),
            "a `0x0022` about a known container with no inventory list must place nothing"
        );
        not_viewing += 1;

        // And the other direction, because a guard test that only tests the blocked side passes on
        // a function that always refuses. Open the container and the same bytes place the id.
        world.view_object_contents(m.container, &[], &mut dereth_client_model::NullSink);
        assert!(
            world.server_says_contain_id(m.container, m.item, m.slot, m.container_properties),
            "with an inventory list the same reply must pre-place"
        );
        let inv = world.inventory(m.container).expect("viewing it");
        let list = if m.container_properties == 0 {
            &inv.items
        } else {
            &inv.containers
        };
        assert_eq!(
            list,
            &[m.item],
            "into the list `props` names, and nowhere else"
        );
        opened += 1;
    }
    eprintln!(
        "guard (a) unknown container exercised {unknown_container} times, guard (b) known \
         but not viewed {not_viewing} times, and the open direction {opened} times, over the {} \
         recorded `0x0022` in long-solo-play. The corpus replay itself takes neither guard: none of \
         the hard cases named a container the client was not viewing.",
        blobs.len()
    );
    assert_eq!(
        unknown_container,
        blobs.len(),
        "guard (a) ran on every recorded reply"
    );
    assert_eq!(
        not_viewing,
        blobs.len(),
        "guard (b) ran on every recorded reply"
    );
    assert_eq!(opened, blobs.len(), "and so did the open direction");
}

// ---------------------------------------------------------------------------------------------
// 5. The invisible half: whose order the pack ends up in.
// ---------------------------------------------------------------------------------------------

/// Behaviour: inventory.contain.the-pack-follows-the-servers-reply-order
/// **The claim a count cannot make.** Only the **order** distinguishes a pre-placed pack from one
/// filled by the creates: without pre-placement each create lands at the head, so a pack filled
/// that way is in the reverse of the order its `0xF745`s arrived.
///
/// Two checks. Every adjacent pair of slot-0 replies ends up with the later reply in front, as
/// inserting each at the head in reply order leaves it. In the recordings no slot-0 pair's creates
/// arrive in the other order (create order is counted by message, not by frame), so those pairs
/// alone cannot tell the two hypotheses apart. What does is the pack as a whole: the replies also
/// name slots other than the head, and at least one recorded pack ends up in an order that is
/// **not** the reverse of its creates' arrival. That assertion is the one that separates them.
///
/// Removal checks: appending each pre-placed item instead of inserting it at the slot the reply
/// names fails the pairwise check; removing the pre-placement altogether (the reply inserts
/// nothing, so every create lands at the head) fails the whole-pack check.
#[test]
fn the_pack_ends_up_in_the_servers_reply_order_not_the_order_the_creates_arrived() {
    let all = whole_corpus();
    let hard = hard_cases(&all);
    // Group by (session, container, which list), and keep only the head-inserting replies.
    let mut groups: BTreeMap<(String, u32, bool), Vec<&Case>> = BTreeMap::new();
    for c in hard.iter().filter(|c| c.slot == 0) {
        groups
            .entry((c.session.clone(), c.container.0, c.props != 0))
            .or_default()
            .push(c);
    }
    let mut inverted_pairs = 0usize;
    let mut checked_pairs = 0usize;
    let mut unavailable_pairs = 0usize;
    // Every id that had already left its pack by the time its neighbour was created, so the four
    // legitimate departures are named rather than merely counted. See the assertion below.
    let mut departed: Vec<ObjectId> = Vec::new();
    for ((session, container, containers_list), members) in &groups {
        if members.len() < 2 {
            continue;
        }
        for pair in members.windows(2) {
            let (earlier, later) = (pair[0], pair[1]);
            // Sample at the later of the two creates: the first moment both objects exist. Not at
            // the end of the session and not even at the end of the group -- an item picked up and
            // dropped again leaves the pack legitimately, and `early-inventory-and-casting`'s `0x80000684` does
            // exactly that before its group finishes arriving.
            let (at, other) = if earlier.create_rank > later.create_rank {
                (earlier, later)
            } else {
                (later, earlier)
            };
            let list = at.list_at_create.as_ref().expect("created");
            let (Some(i_later), Some(i_earlier)) = (
                list.iter().position(|x| *x == later.item),
                list.iter().position(|x| *x == earlier.item),
            ) else {
                unavailable_pairs += 1;
                departed.push(other.item);
                eprintln!(
                    "{session}/{container:#010X}: {:#010X} was no longer in the pack when \
                     {:#010X} was created; pair not comparable",
                    other.item.0, at.item.0
                );
                continue;
            };
            assert!(
                i_later < i_earlier,
                "{session}/{container:#010X}: {:#010X} replied at #{} and {:#010X} at #{}; each \
                 named slot 0, so insertion at that numbered position puts the later reply in front. The pack has them at \
                 {i_later} and {i_earlier}",
                later.item.0,
                later.reply_rank,
                earlier.item.0,
                earlier.reply_rank,
            );
            checked_pairs += 1;
            if later.create_rank < earlier.create_rank {
                inverted_pairs += 1;
                eprintln!(
                    "{session}/{container:#010X}: {:#010X} (reply #{}, create #{:?}) and \
                     {:#010X} (reply #{}, create #{:?}) arrived in opposite orders -- this pair \
                     is what separates the two hypotheses",
                    earlier.item.0,
                    earlier.reply_rank,
                    earlier.create_rank,
                    later.item.0,
                    later.reply_rank,
                    later.create_rank
                );
            }
        }
        eprintln!(
            "{session}/{container:#010X} ({} list): {} head-inserted pre-placements",
            if *containers_list {
                "containers"
            } else {
                "items"
            },
            members.len()
        );
    }
    eprintln!(
        "{checked_pairs} adjacent reply pairs compared, {inverted_pairs} of them with the \
         create order inverted, {unavailable_pairs} not comparable"
    );
    // **The departures the comparison skips, each explained.** A pair is not comparable when one
    // of its ids had already left the pack by the neighbour's create. That is legitimate only when
    // the server took it out -- rent paid, salvaged, sold, wielded, given, spent -- so every
    // departed id must have been named by a server message after its own reply. An id that
    // vanished with no such message is a defect, and this fails naming it.
    departed.sort_unstable();
    let unexplained: Vec<ObjectId> = hard
        .iter()
        .flat_map(|c| c.left_unexplained.iter().copied())
        .filter(|id| departed.contains(id))
        .collect();
    assert!(
        unexplained.is_empty(),
        "these pre-placed ids left their pack with no server message naming them: {unexplained:?} \
         (departed: {departed:?})"
    );
    assert!(
        checked_pairs > 0,
        "the loop must have run; compared {checked_pairs} pairs"
    );
    // The whole-pack check: per (session, container, list), the pre-placed items that nothing but
    // their reply and their create ever placed, in the pack as it stands when the group's last
    // create lands, against the reverse of their create order. An item a later message moved is
    // left out: its place says nothing about either hypothesis.
    let mut all_groups: BTreeMap<(String, u32, bool), Vec<&Case>> = BTreeMap::new();
    for c in &hard {
        all_groups
            .entry((c.session.clone(), c.container.0, c.props != 0))
            .or_default()
            .push(c);
    }
    let mut packs_compared = 0usize;
    let mut packs_not_in_reverse_create_order = 0usize;
    for ((session, container, _), members) in &all_groups {
        let Some(last) = members
            .iter()
            .filter(|c| c.create_rank.is_some())
            .max_by_key(|c| c.create_rank)
        else {
            continue;
        };
        let list = last.list_at_create.as_ref().expect("created");
        let in_pack: Vec<ObjectId> = list
            .iter()
            .copied()
            .filter(|id| {
                members
                    .iter()
                    .any(|c| c.item == *id && c.touched_after_reply == 0)
            })
            .collect();
        if in_pack.len() < 2 {
            continue;
        }
        let rank = |id: &ObjectId| {
            members
                .iter()
                .find(|c| c.item == *id)
                .and_then(|c| c.create_rank)
        };
        let mut head_inserted = in_pack.clone();
        head_inserted.sort_by_key(|id| std::cmp::Reverse(rank(id)));
        packs_compared += 1;
        if in_pack != head_inserted {
            packs_not_in_reverse_create_order += 1;
            eprintln!(
                "{session}/{container:#010X}: the pack is not in reverse create order -- it is \
                 {in_pack:08X?}"
            );
        }
    }
    eprintln!(
        "{packs_compared} packs compared, {packs_not_in_reverse_create_order} not in reverse \
         create order"
    );
    assert!(
        packs_not_in_reverse_create_order > 0,
        "every recorded pack of pre-placed items is in the reverse of its creates' arrival, which \
         is what a client without pre-placement produces: {packs_compared} packs compared"
    );
}
