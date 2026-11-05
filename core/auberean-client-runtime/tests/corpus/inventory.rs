//! The inventory the panels show, as the player's scripts answer for it, after every recorded
//! session: against the objects the recorded messages say the player holds; and the items players
//! hold in hand, let go of.

use auberean_client_contract::Action;
use auberean_client_net::LocalTime;
use auberean_client_runtime::Client;
use auberean_protocol::{ClientMessage, ServerMessage};
use auberean_testkit::captures;

use crate::calls::{client, data, PLAY_CHARACTER};

/// The client's log-off: the inventory is read before it goes.
const LOG_OFF: u32 = 0x10027;

/// What the looks at the inventory found.
#[derive(Default, Debug)]
struct Tally {
    looked: u64,
    /// Looks with items in the pack, the items listed and worn.
    with_items: u64,
    listed: u64,
    worn: u64,
    /// Items listed the client has not been sent yet (the panels show them waiting).
    waiting: u64,
    /// Items listed whose holder the recorded messages do not lead back to the player (a
    /// container never sent), and a few of them.
    stray: u64,
    strays: Vec<(i64, u64)>,
    /// Items listed with no name, and a few of them.
    unnamed: u64,
    unnamed_ids: Vec<(i64, u64)>,
    /// Looks at a pack whose segments do not tile the items it lists, in order.
    untiled: u64,
    /// Looks by how many bags hold items.
    bags: std::collections::BTreeMap<usize, u64>,
    /// Looks at a pack holding more than 42 items; the most items one held, the most its main
    /// segment held.
    past_main: u64,
    most: usize,
    most_main: u32,
}

impl Tally {
    fn look(&mut self, c: &mut Client, capture: i64) {
        if c.model.player_object().is_none() {
            return;
        }
        c.refresh_inventory();
        let inv = c.inventory().clone();
        let Some(pack) = inv.containers.get(&inv.player) else {
            return;
        };
        self.looked += 1;
        let held = c.model.inventory();
        // Every segment's answer lists the whole pack in order; a segment is its window of it
        // (its first item and how many), the main pack's first, then each bag's.
        let listed: Vec<u64> = pack
            .segments
            .first()
            .map(|s| s.items.clone())
            .unwrap_or_default();
        let mut at = 0;
        let mut tiled = pack.segments.iter().all(|s| s.items == listed);
        for (first, count) in &inv.segments {
            tiled &= *first == at;
            at += count;
        }
        tiled &= at as usize == listed.len();
        if !tiled {
            self.untiled += 1;
        }
        let bags = inv.segments.iter().skip(1).filter(|(_, n)| *n > 0).count();
        *self.bags.entry(bags).or_default() += 1;
        if listed.len() > 42 {
            self.past_main += 1;
        }
        self.most = self.most.max(listed.len());
        self.most_main = self
            .most_main
            .max(inv.segments.first().map_or(0, |(_, n)| *n));
        if !listed.is_empty() {
            self.with_items += 1;
        }
        self.listed += listed.len() as u64;
        self.worn += inv.worn.len() as u64;
        for id in listed.iter().chain(inv.worn.iter().map(|(_, id)| id)) {
            if c.model.object(*id).is_none() {
                self.waiting += 1;
                continue;
            }
            if !held.contains(id) && !holds(c, *id, inv.player) {
                self.stray += 1;
                if self.strays.len() < 5 {
                    self.strays.push((capture, *id));
                }
            }
            let named = inv
                .items
                .get(id)
                .is_some_and(|f| !f.name.is_empty() && !f.name.starts_with("Error!"));
            if !named {
                self.unnamed += 1;
                if self.unnamed_ids.len() < 5 {
                    self.unnamed_ids.push((capture, *id));
                }
            }
        }
    }
}

/// Whether `holder` holds `id` as the descriptions say: its container (or wielder), or theirs.
fn holds(c: &Client, id: u64, holder: u64) -> bool {
    use auberean_client_runtime::description::instance_id;
    let mut at = id;
    for _ in 0..8 {
        let Some(o) = c.model.object(at) else {
            return false;
        };
        let up = instance_id(&o.weenie, 0x102)
            .or_else(|| instance_id(&o.weenie, 0x105))
            .or(o.container)
            .or(o.wielder);
        match up {
            Some(u) if u == holder => return true,
            Some(u) => at = u,
            None => return false,
        }
    }
    false
}

/// At every recorded log-off and at the end of each capture, the player's pack as its scripts
/// list it is there (in nearly every look) and holds what the recorded messages have the player
/// hold, but for items whose container the recordings never sent (under one in twenty); what it
/// wears likewise; every item listed that the client has been sent has its name. The pack is
/// listed whole, in order, its four segments (the main pack, then each of three bags) each a
/// window of the list that follows the one before: 344 of 433 looks hold more than 42 items, up
/// to 78 (a main pack holding 44, then bags of 12, 12 and 10).
///
/// Behaviour: script.model.the-pack-the-scripts-list-is-what-the-player-holds
#[test]
fn the_pack_the_scripts_list_is_what_the_player_holds() {
    let data = data().expect("the September 2013 install opens");
    let index = auberean_testkit::data::require_capture_index();
    let mut types: Vec<u32> = (0x1_0000..0x1_0100)
        .filter(|t| ServerMessage::is_known_type(*t))
        .collect();
    types.extend([PLAY_CHARACTER, LOG_OFF]);
    let mut capture = -1;
    let mut c = client(&data);
    let mut frame = f64::NEG_INFINITY;
    let mut tally = Tally::default();
    captures::messages_in_time_order(&index, &types, &[2013, 2017], |m| {
        if m.capture != capture {
            tally.look(&mut c, capture);
            capture = m.capture;
            c = client(&data);
            frame = f64::NEG_INFINITY;
        }
        if m.time > frame {
            frame = m.time;
            c.set_time(LocalTime(m.time));
            c.upkeep();
        }
        if !m.to_client {
            match ClientMessage::decode(&m.raw) {
                Ok(ClientMessage::PlayCharacter(p)) => c.act(Action::PlayCharacter {
                    character: p.character,
                }),
                Ok(ClientMessage::LogOff(_)) => tally.look(&mut c, capture),
                _ => {}
            }
            return;
        }
        if let Ok(msg) = ServerMessage::decode(&m.raw) {
            c.take_message(m.blob_id, msg);
        }
    })
    .expect("the index reads");
    tally.look(&mut c, capture);
    eprintln!("{tally:?}");
    assert!(tally.with_items >= 400, "{tally:?}");
    assert!(tally.stray * 20 < tally.listed + tally.worn, "{tally:?}");
    assert_eq!(tally.unnamed, 0, "{tally:?}");
    assert_eq!(tally.untiled, 0, "{tally:?}");
    assert!(tally.past_main >= 340 && tally.most > 66, "{tally:?}");
}

/// The server's answer to opening a container.
const OPEN_DONE: u32 = 0x820E_005D;

/// What the containers the recorded players opened showed.
#[derive(Default, Debug)]
struct Opened {
    /// Containers opened, those showing items, and the items shown.
    opened: u64,
    showing: u64,
    shown: u64,
    /// Containers the client had been sent items in that showed none, and a few of them.
    empty: u64,
    empties: Vec<(i64, u64)>,
}

/// After every recorded opening of a container, the container shows what its scripts were told it
/// holds: nearly all show items (a container opened empty shows none), and every container the
/// client was sent items in shows them but one in a hundred.
///
/// Behaviour: script.model.an-opened-container-shows-what-its-scripts-were-told-it-holds
#[test]
fn an_opened_container_shows_what_its_scripts_were_told_it_holds() {
    let data = data().expect("the September 2013 install opens");
    let index = auberean_testkit::data::require_capture_index();
    let types: Vec<u32> = (0x1_0000..0x1_0100)
        .filter(|t| ServerMessage::is_known_type(*t))
        .chain([PLAY_CHARACTER])
        .collect();
    let mut capture = -1;
    let mut c = client(&data);
    let mut frame = f64::NEG_INFINITY;
    let mut tally = Opened::default();
    let mut seen = std::collections::BTreeSet::new();
    captures::messages_in_time_order(&index, &types, &[2013, 2017], |m| {
        if m.capture != capture {
            capture = m.capture;
            c = client(&data);
            c.observe = vec![OPEN_DONE];
            seen.clear();
            frame = f64::NEG_INFINITY;
        }
        if m.time > frame {
            frame = m.time;
            c.set_time(LocalTime(m.time));
            c.upkeep();
        }
        if !m.to_client {
            if let Ok(ClientMessage::PlayCharacter(p)) = ClientMessage::decode(&m.raw) {
                c.act(Action::PlayCharacter {
                    character: p.character,
                });
            }
            return;
        }
        let Ok(msg) = ServerMessage::decode(&m.raw) else {
            return;
        };
        c.take_message(m.blob_id, msg);
        if c.observed.is_empty() {
            return;
        }
        c.observed.clear();
        c.refresh_inventory();
        let inv = c.inventory();
        for (id, held) in inv.containers.iter().filter(|(id, _)| **id != inv.player) {
            if !seen.insert(*id) {
                continue;
            }
            let items: Vec<u64> = held.segments.iter().flat_map(|s| s.items.clone()).collect();
            tally.opened += 1;
            tally.showing += u64::from(!items.is_empty());
            tally.shown += items.len() as u64;
            let sent = c.model.objects().any(|o| o.container == Some(*id));
            if sent && items.is_empty() {
                tally.empty += 1;
                if tally.empties.len() < 5 {
                    tally.empties.push((capture, *id));
                }
            }
        }
    })
    .expect("the index reads");
    eprintln!("{tally:?}");
    assert!(tally.opened >= 4000, "{tally:?}");
    assert!(tally.showing * 10 >= tally.opened * 9, "{tally:?}");
    assert!(tally.empty * 100 < tally.opened, "{tally:?}");
}

/// Every `DeParent` recorded in 2013 and 2017 that finds its item hanging names, in its field, an
/// item the model has hanging from the message's sender, and the item hangs from nothing after it;
/// none names one hanging from anything else. About one in six comes just after the item's own
/// `Contain`, stamped one later, which already took it from its holder's hand, so it has nothing
/// left to do. The few that came after a newer `Parent` of the same item (five in all, the two sent
/// out of their stamps' order) change nothing: the item stays where the newer one hung it.
///
/// Behaviour: model.replay.every-recorded-let-go-names-an-item-its-sender-held
#[test]
fn every_recorded_let_go_names_an_item_its_sender_held() {
    use auberean_client_model::events::{newer, EventQueue, Item};
    use auberean_client_model::Model;
    let index = auberean_testkit::data::require_capture_index();
    let mut types: Vec<u32> = (0x1_0000..0x1_0100)
        .filter(|t| ServerMessage::is_known_type(*t))
        .collect();
    types.push(PLAY_CHARACTER);
    #[derive(Debug, Default)]
    struct Tally {
        taken: u64,
        held_by_sender: u64,
        held_by_other: u64,
        /// Let-gos of an item its `Contain` had already taken from its holder.
        after_contain: u64,
        not_held: u64,
        unknown_item: u64,
        sender_was_held: u64,
        let_go: u64,
        /// Let-gos stamped no later than a hold already taken, which left the item held.
        outrun: u64,
        unknown_made_later: u64,
    }
    let mut t = Tally::default();
    let mut capture = -1;
    let mut unknown = std::collections::BTreeSet::new();
    let mut contained = std::collections::BTreeSet::new();
    let (mut queue, mut model) = (EventQueue::new(), Model::new());
    let mut frame = f64::NEG_INFINITY;
    captures::messages_in_time_order(&index, &types, &[2013, 2017], |m| {
        if m.capture != capture {
            capture = m.capture;
            (queue, model) = (EventQueue::new(), Model::new());
            unknown.clear();
            contained.clear();
            frame = f64::NEG_INFINITY;
        }
        if m.time > frame {
            frame = m.time;
            queue.use_time(m.time);
            queue.take_force_creates();
            for id in queue.take_destroyed() {
                model.forget(id);
            }
        }
        if !m.to_client {
            if let Ok(ClientMessage::PlayCharacter(_)) = ClientMessage::decode(&m.raw) {
                queue.start_entering();
            }
            return;
        }
        let Ok(message) = ServerMessage::decode(&m.raw) else {
            return;
        };
        queue.receive(
            m.time,
            Item {
                blob_id: m.blob_id,
                message,
            },
        );
        while let Some(item) = queue.pop() {
            if let ServerMessage::DeParent(e) = &item.message {
                t.taken += 1;
                let sender_parent = model.object(e.sender.id).and_then(|o| o.parent);
                t.sender_was_held += u64::from(sender_parent.is_some());
                match model.object(e.action.id).map(|o| o.parent) {
                    None => {
                        t.unknown_item += 1;
                        unknown.insert(e.action.id);
                    }
                    Some(None) if contained.contains(&e.action.id) => t.after_contain += 1,
                    Some(None) => t.not_held += 1,
                    Some(Some((p, _))) if p == e.sender.id => t.held_by_sender += 1,
                    Some(Some(_)) => t.held_by_other += 1,
                }
                t.outrun += u64::from(model.object(e.action.id).is_some_and(|o| {
                    o.parent.is_some() && !newer(e.action.other_stamp, o.position_stamp)
                }));
                model.apply(&item);
                t.let_go += u64::from(
                    model
                        .object(e.action.id)
                        .is_some_and(|o| o.parent.is_none()),
                );
                continue;
            }
            if let ServerMessage::CreateObject(c) = &item.message {
                t.unknown_made_later += u64::from(unknown.remove(&c.id));
            }
            let contain = match &item.message {
                ServerMessage::Contain(o) => Some(o.0.id),
                _ => None,
            };
            let held =
                contain.is_some_and(|id| model.object(id).is_some_and(|o| o.parent.is_some()));
            model.apply(&item);
            if let (Some(id), true) = (contain, held) {
                if model.object(id).is_some_and(|o| o.parent.is_none()) {
                    contained.insert(id);
                }
            }
            if let ServerMessage::Parent(e) = &item.message {
                contained.remove(&e.sender.id);
            }
        }
    })
    .expect("the index reads");
    eprintln!("let-gos taken: {t:?}");
    assert_eq!(t.held_by_other, 0, "{t:?}");
    assert_eq!(t.sender_was_held, 0, "{t:?}");
    assert!(t.held_by_sender >= 1_000, "{t:?}");
    assert!(t.after_contain * 20 >= t.taken, "{t:?}");
    assert!(t.not_held * 100 < t.taken, "{t:?}");
    assert_eq!(t.let_go, t.taken - t.unknown_item - t.outrun, "{t:?}");
    assert!(t.outrun * 100 < t.taken, "{t:?}");
}

/// The server's call that tells a trade's two sides of an offer.
const BE_OFFERED: u32 = 0x8215_0044;

/// What the trade window found of the partner's offers.
#[derive(Default, Debug)]
struct Offers {
    captures: u64,
    /// The partner's offers, those the client was sent, those shown with their name and icon.
    offered: u64,
    sent: u64,
    shown: u64,
    /// Offers sent but not shown, and a few of them.
    hidden: u64,
    hiddens: Vec<(i64, u64)>,
}

/// Every item a recorded trade's partner offered, once the client has been sent it (still in the
/// partner's pack), is shown in the trade window with its name and icon, as the player's own items
/// are: never as an item the client has not been sent.
///
/// Behaviour: script.model.a-trade-partners-offer-is-shown-as-an-item
#[test]
fn a_trade_partners_offer_is_shown_with_its_name_and_icon() {
    use auberean_client_runtime::store::trade;
    let data = data().expect("the September 2013 install opens");
    let index = auberean_testkit::data::require_capture_index();
    let mut traded = std::collections::BTreeSet::new();
    captures::messages(
        &index,
        &[captures::carrier::TO_PLAYER],
        &[2013, 2017],
        |m| {
            if m.script_call().is_some_and(|s| s.function == BE_OFFERED) {
                traded.insert(m.capture);
            }
        },
    )
    .expect("the index reads");
    let types: Vec<u32> = (0x1_0000..0x1_0100)
        .filter(|t| ServerMessage::is_known_type(*t))
        .chain([PLAY_CHARACTER])
        .collect();
    let mut t = Offers::default();
    for capture in traded {
        t.captures += 1;
        let mut c = client(&data);
        let mut frame = f64::NEG_INFINITY;
        // The partner's offers standing, and whether each was found sent yet.
        let mut offers: std::collections::BTreeMap<u64, bool> = std::collections::BTreeMap::new();
        captures::capture_by_id_in_time_order(&index, &types, capture, |m| {
            if m.time > frame {
                frame = m.time;
                c.set_time(LocalTime(m.time));
                c.upkeep();
            }
            if !m.to_client {
                if let Ok(ClientMessage::PlayCharacter(p)) = ClientMessage::decode(&m.raw) {
                    c.act(Action::PlayCharacter {
                        character: p.character,
                    });
                }
                return;
            }
            let Ok(msg) = ServerMessage::decode(&m.raw) else {
                return;
            };
            c.take_message(m.blob_id, msg);
            let player = c.model.player.unwrap_or(0);
            for n in std::mem::take(&mut c.notices) {
                let long = |at: usize| {
                    let w = |i: usize| u64::from(n.words.get(i).copied().unwrap_or(0));
                    w(at) << 32 | w(at + 1)
                };
                match n.native {
                    trade::OFFERED if long(0) != player => {
                        t.offered += 1;
                        offers.insert(long(2), false);
                    }
                    trade::REVOKED => {
                        offers.remove(&long(2));
                    }
                    trade::RESET | trade::CLOSED => offers.clear(),
                    _ => {}
                }
            }
            if offers.values().all(|found| *found) {
                return;
            }
            c.refresh_inventory();
            for (item, found) in &mut offers {
                if *found || c.model.object(*item).is_none() {
                    continue;
                }
                *found = true;
                t.sent += 1;
                let shown = c
                    .inventory()
                    .items
                    .get(item)
                    .is_some_and(|f| f.icon != 0 && !f.name.is_empty());
                if shown {
                    t.shown += 1;
                } else {
                    t.hidden += 1;
                    if t.hiddens.len() < 5 {
                        t.hiddens.push((capture, *item));
                    }
                }
            }
        })
        .expect("the index reads");
    }
    eprintln!("{t:?}");
    assert!(t.sent >= 100, "{t:?}");
    assert_eq!(t.hidden, 0, "{t:?}");
}

/// The player's own request selecting an object.
const SET_SELECTION: u32 = 0x8212_0016;

/// How many selections the look below makes at most (the 2017 captures in time order).
const SELECTIONS: u64 = 2_000;

/// What the looks at the selection found.
#[derive(Default, Debug)]
struct Picked {
    /// Selections of an object the client held; the inventory naming it as the selection; listing
    /// its facts; with an icon; and a few that were not listed.
    selected: u64,
    named: u64,
    listed: u64,
    with_icon: u64,
    unlisted: Vec<(i64, u64)>,
}

/// Each object the recorded 2017 players selected, once selected again, is the inventory's
/// selection and is listed among the inventory's items with its facts (its icon among them), as
/// the panels need it to drag it from the world into the pack: the first 2,000 selections of an
/// object the client held, every one listed.
///
/// Behaviour: script.model.the-selection-is-listed-with-the-inventory
#[test]
fn the_selection_is_listed_with_the_inventory() {
    let data = data().expect("the September 2013 install opens");
    let lib = &data.scripts.as_ref().expect("the scripts").1.library;
    let index = auberean_testkit::data::require_capture_index();
    let mut types: Vec<u32> = (0x1_0000..0x1_0100)
        .filter(|t| ServerMessage::is_known_type(*t))
        .collect();
    types.extend([PLAY_CHARACTER, captures::carrier::TO_SERVER]);
    let mut capture = -1;
    let mut c = client(&data);
    let mut frame = f64::NEG_INFINITY;
    let mut t = Picked::default();
    captures::messages_in_time_order(&index, &types, &[2017], |m| {
        if t.selected >= SELECTIONS {
            return;
        }
        if m.capture != capture {
            capture = m.capture;
            c = client(&data);
            frame = f64::NEG_INFINITY;
        }
        if m.time > frame {
            frame = m.time;
            c.set_time(LocalTime(m.time));
            c.upkeep();
        }
        c.notices.clear();
        if m.to_client {
            if let Ok(msg) = ServerMessage::decode(&m.raw) {
                c.take_message(m.blob_id, msg);
            }
            return;
        }
        if m.message_type == PLAY_CHARACTER {
            if let Ok(ClientMessage::PlayCharacter(p)) = ClientMessage::decode(&m.raw) {
                c.act(Action::PlayCharacter {
                    character: p.character,
                });
            }
            return;
        }
        let Some(call) = m.script_call() else { return };
        if call.function != SET_SELECTION || c.model.player_object().is_none() {
            return;
        }
        let Ok(v) = auberean_script::args::decode(&call.args, lib) else {
            return;
        };
        let [auberean_script::args::Arg::LongInt(target)] = v[..] else {
            return;
        };
        if target == 0 || Some(target) == c.model.player || c.model.object(target).is_none() {
            return;
        }
        c.act(Action::Select { target });
        c.refresh_inventory();
        t.selected += 1;
        let inv = c.inventory();
        if inv.selection == target {
            t.named += 1;
        }
        match inv.items.get(&target) {
            Some(f) => {
                t.listed += 1;
                if f.icon != 0 {
                    t.with_icon += 1;
                }
            }
            None => {
                if t.unlisted.len() < 5 {
                    t.unlisted.push((capture, target));
                }
            }
        }
    })
    .expect("the index reads");
    eprintln!("{t:?}");
    assert_eq!(t.selected, SELECTIONS, "{t:?}");
    assert_eq!(t.named, t.selected, "{t:?}");
    assert_eq!(t.listed, t.selected, "{t:?}");
}

/// Every creation of a newer incarnation of an object the client still has, in 2013 and 2017
/// (3,756, nearly all after a hand-off to another server, where the older incarnation came over
/// the connection before), replaces the old one, and the new one is the client's after the next
/// upkeep. (None of them came while the old one's forgetting waited for the upkeep.)
///
/// Behaviour: model.replay.every-newer-incarnation-replaces-the-old-one
#[test]
fn every_recorded_newer_incarnation_replaces_the_old_one() {
    use auberean_client_model::events::{EventQueue, Item};
    use auberean_client_model::Model;
    let index = auberean_testkit::data::require_capture_index();
    let mut types: Vec<u32> = (0x1_0000..0x1_0100)
        .filter(|t| ServerMessage::is_known_type(*t))
        .collect();
    types.push(PLAY_CHARACTER);
    let (mut replaced, mut made, mut kept) = (0u64, 0u64, 0u64);
    let mut capture = -1;
    let (mut queue, mut model) = (EventQueue::new(), Model::new());
    let mut frame = f64::NEG_INFINITY;
    // The newer incarnations made since the last upkeep.
    let mut newer: Vec<(u64, u16)> = Vec::new();
    captures::messages_in_time_order(&index, &types, &[2013, 2017], |m| {
        if m.capture != capture {
            capture = m.capture;
            (queue, model) = (EventQueue::new(), Model::new());
            frame = f64::NEG_INFINITY;
            newer.clear();
        }
        if m.time > frame {
            frame = m.time;
            queue.use_time(m.time);
            queue.take_force_creates();
            for id in queue.take_destroyed() {
                model.forget(id);
            }
            for (id, stamp) in newer.drain(..) {
                kept += u64::from(model.object(id).is_some_and(|o| o.instance_stamp == stamp));
            }
        }
        if !m.to_client {
            if let Ok(ClientMessage::PlayCharacter(_)) = ClientMessage::decode(&m.raw) {
                queue.start_entering();
            }
            return;
        }
        let Ok(message) = ServerMessage::decode(&m.raw) else {
            return;
        };
        let before = queue.stats().replaced;
        queue.receive(
            m.time,
            Item {
                blob_id: m.blob_id,
                message,
            },
        );
        while let Some(item) = queue.pop() {
            match &item.message {
                ServerMessage::CreateObject(c) => {
                    let old = model.object(c.id).map(|o| o.instance_stamp);
                    if old.is_some_and(|s| s != c.physics.instance_stamp) {
                        made += 1;
                        newer.retain(|(id, _)| *id != c.id);
                        newer.push((c.id, c.physics.instance_stamp));
                    }
                }
                ServerMessage::ForgetObject(o) => newer.retain(|(id, _)| *id != o.0.id),
                ServerMessage::CharExitGame(_) => {
                    queue.leave_world();
                    newer.clear();
                }
                _ => {}
            }
            model.apply(&item);
        }
        replaced += u64::from(queue.stats().replaced - before);
    })
    .expect("the index reads");
    eprintln!("{replaced} replaced, {made} made over an older incarnation, {kept} kept after the next upkeep");
    assert_eq!(replaced, made);
    assert_eq!(replaced, 3_756);
    assert_eq!(kept, replaced);
}
