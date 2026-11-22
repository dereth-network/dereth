//! An item whose template names a time between uses: used again within it, the use is refused
//! and nothing is done; the wait counts from the last use let through, and goes on while its holder
//! is away.
//! Fixture: a made-up book in a player's pack, its template naming three or 120 seconds between
//! uses, or none.

use auberean_protocol::{queue, ClientMessage, ServerMessage};
use yalain_content::{Prop, Props};
use yalain_net::ConnId;
use yalain_testkit::play::{server_call, use_of};
use yalain_testkit::{test_template, Harness};
use yalain_world::books::USE_BOOK;
use yalain_world::calls::{self, Node, Value};
use yalain_world::places::function::USE_DONE;
use yalain_world::requests::function::USE;

const WAITS: u32 = 0x4700_06CD;
const TABLE: u32 = 0x2500_1CEE;

/// A player in the world holding a book whose template names `between` seconds between uses (none
/// when `None`); the client, the player, the book.
fn holding(between: Option<i64>) -> (Harness, usize, u64, u64) {
    let mut h = Harness::new();
    let mut props = Props::default();
    props.set("Book_Source", Prop::Str(0, TABLE));
    if let Some(s) = between {
        props.set("Item_TimeBetweenUses", Prop::Int(s));
    }
    h.server.catalog.insert(test_template(WAITS, 0x280B, props));
    let (b, id) = h.player_in_world("one", "Reader");
    let world = world_of(&h, id);
    let book =
        yalain_world::inventory::give(&mut h.server, &mut h.net, world, WAITS, 1).expect("given");
    h.run(5);
    (h, b, id, book)
}

fn world_of(h: &Harness, me: u64) -> ConnId {
    h.server
        .players
        .iter()
        .find(|(_, p)| p.character == me)
        .map(|(w, _)| *w)
        .expect("in the world")
}

/// Use the book at `at` seconds on the harness clock: its answer's status and control flags, and
/// whether the book was opened.
fn use_at(h: &mut Harness, b: usize, me: u64, book: u64, at: f64) -> (u32, u32, bool) {
    while h.now.0 < at {
        h.step();
    }
    let from = h.bots[b].inbox.len();
    h.bots[b].send(true, queue::WEENIE, &server_call(USE, &[use_of(book, me)]));
    h.run(4);
    let c = h.calls(b, from);
    let opened = c.iter().any(|(f, _)| *f == USE_BOOK);
    let answer = h
        .world_messages(b, from)
        .iter()
        .find_map(|m| match m {
            ServerMessage::PlayerCall(c) if c.function.0 == USE_DONE => {
                let v = c.args.as_ref().and_then(|a| calls::values(&a.0).ok())?;
                let Some(Value::Graph(g)) = v.first() else {
                    return None;
                };
                g.iter().find_map(|(_, n)| match n {
                    Node::Script { slots, .. } => Some((slots[11].1, slots[17].1)),
                    _ => None,
                })
            }
            _ => None,
        })
        .expect("the use answered");
    (answer.0, answer.1, opened)
}

/// An item of a template naming three seconds between uses, used again two seconds after its use,
/// is refused 10007 with control flags 828 and nothing done; used again 3.2 s after the use let
/// through (1.2 s after the refusal, which does not start the wait again), it is used.
/// Behaviour: server.items.time-between-uses
#[test]
fn an_item_used_again_within_its_time_between_uses_is_refused_and_after_it_used() {
    let (mut h, b, me, book) = holding(Some(3));
    let t0 = h.now.0;
    let (status, _, opened) = use_at(&mut h, b, me, book, t0);
    assert_eq!(status, 0, "the first use let through");
    assert!(opened);
    let (status, flags, opened) = use_at(&mut h, b, me, book, t0 + 2.0);
    assert_eq!(
        (status, flags),
        (yalain_world::item_use::refusal::TOO_SOON, 828),
        "too soon"
    );
    assert!(!opened, "nothing done");
    let (status, _, opened) = use_at(&mut h, b, me, book, t0 + 3.2);
    assert_eq!(status, 0, "the wait over, counted from the use let through");
    assert!(opened);
}

/// An item whose template names no time between uses is used again at once.
/// Behaviour: server.items.time-between-uses
#[test]
fn an_item_naming_no_time_between_uses_is_used_again_at_once() {
    let (mut h, b, me, book) = holding(None);
    let t0 = h.now.0;
    for at in [t0, t0 + 0.1, t0 + 0.2] {
        let (status, _, opened) = use_at(&mut h, b, me, book, at);
        assert_eq!(status, 0, "at {at}");
        assert!(opened);
    }
}

/// An item's last use is kept while its holder is away: a two-minute item used, its holder logged
/// off and back a minute later, is refused; two minutes after its use, it is used.
/// Behaviour: server.items.time-between-uses
#[test]
fn an_items_time_between_uses_goes_on_while_its_holder_is_away() {
    let (mut h, b, me, book) = holding(Some(120));
    let t0 = h.now.0;
    assert_eq!(use_at(&mut h, b, me, book, t0).0, 0, "used");
    let before = h.bots[b].inbox.len();
    h.bots[b].send(
        false,
        queue::LOGON,
        &ClientMessage::LogOff(auberean_protocol::login::CharacterRef { character: me }),
    );
    assert!(h.run_until(400, |h| h.bots[b].inbox[before..]
        .iter()
        .any(|r| matches!(r.message(), Some(ServerMessage::CharacterList(_))))));
    assert!(h.server.things.get(book).is_none(), "gone with its holder");
    while h.now.0 < t0 + 60.0 {
        h.step_by(1.0);
    }
    h.enter_world(b, "one", me);
    h.run(10);
    assert_eq!(
        use_at(&mut h, b, me, book, t0 + 61.0).0,
        yalain_world::item_use::refusal::TOO_SOON,
        "still waiting"
    );
    assert_eq!(use_at(&mut h, b, me, book, t0 + 120.5).0, 0, "used again");
}
