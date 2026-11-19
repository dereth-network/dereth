//! Following: a player follows another, is told so and given one standing movement order toward
//! it; asking again ends the old follow first; a use ends it. Fixture: two players of the test
//! world side by side.

use auberean_protocol::{queue, ServerMessage};
use auberean_script::args::Arg;
use yalain_testkit::play::server_call;
use yalain_testkit::Harness;
use yalain_world::calls::{self, Node, Value};
use yalain_world::follow;

const SHOW_TEXT: u32 = 0x8205_0011;

/// The texts a client was shown since `from`, by string id.
fn shown(h: &Harness, b: usize, from: usize) -> Vec<u32> {
    h.world_messages(b, from)
        .iter()
        .filter_map(|m| match m {
            ServerMessage::PlayerCall(c) if c.function.0 == SHOW_TEXT => {
                let v = c.args.as_ref().and_then(|a| calls::values(&a.0).ok())?;
                v.iter().find_map(|x| match x {
                    Value::Graph(g) => g.iter().find_map(|(_, n)| match n {
                        Node::TextInfo(t) => Some(t.string_id),
                        _ => None,
                    }),
                    _ => None,
                })
            }
            _ => None,
        })
        .collect()
}

/// The movement orders a client was sent for its player since `from`: (flags, distance).
fn orders(h: &Harness, b: usize, from: usize, me: u64) -> Vec<(u32, f32)> {
    h.world_messages(b, from)
        .iter()
        .filter_map(|m| match m {
            ServerMessage::MoveTo(e) if e.sender.id == me => {
                Some((e.action.params.flags, e.action.params.desired_distance))
            }
            _ => None,
        })
        .collect()
}

/// A player follows another: "Started following." and one standing order to keep within a
/// meter of it; following again first stops ("Stopped following.", the order stopped); a use
/// stops it too.
/// Behaviour: server.social.follow-starts-and-stops
#[test]
fn a_player_follows_another_until_it_follows_again_or_uses_something() {
    let mut h = Harness::new();
    let (b, me) = h.player_in_world("one", "Elara");
    let (_, other) = h.player_in_world("two", "Torvald");
    h.run(5);
    let from = h.bots[b].inbox.len();
    h.bots[b].send(
        true,
        queue::WEENIE,
        &server_call(follow::FOLLOW, &[Arg::LongInt(other)]),
    );
    h.run(4);
    assert_eq!(shown(&h, b, from), vec![follow::STARTED.0]);
    assert_eq!(orders(&h, b, from, me), vec![(follow::ORDER_FLAGS, 1.0)]);

    let again = h.bots[b].inbox.len();
    h.bots[b].send(
        true,
        queue::WEENIE,
        &server_call(follow::FOLLOW, &[Arg::LongInt(other)]),
    );
    h.run(4);
    assert_eq!(
        shown(&h, b, again),
        vec![follow::STOPPED.0, follow::STARTED.0]
    );
    assert_eq!(
        orders(&h, b, again, me)
            .iter()
            .map(|o| o.0)
            .collect::<Vec<_>>(),
        vec![0x40, follow::ORDER_FLAGS]
    );

    let used = h.bots[b].inbox.len();
    h.bots[b].send(
        true,
        queue::WEENIE,
        &server_call(0x8216_0016, &[yalain_testkit::play::use_of(other, me)]),
    );
    h.run(4);
    assert!(
        shown(&h, b, used).contains(&follow::STOPPED.0),
        "a use ends it"
    );
}

/// Each follow goes as movement orders to everyone who sees the player: asked again and again they
/// come at a pace, and a follow asked once the pace has filled goes as ever.
/// Behaviour: server.follow.at-a-pace
#[test]
fn a_follow_asked_again_and_again_reaches_those_who_see_it_at_a_pace() {
    let mut h = Harness::new();
    let (a, _) = h.player_in_world("one", "Elara");
    let (b, other) = h.player_in_world("two", "Babyfare");
    h.run(10);
    let from = h.bots[b].inbox.len();
    let orders = |h: &Harness, from: usize| {
        h.world_messages(b, from)
            .iter()
            .filter(|m| matches!(m, ServerMessage::MoveTo(_)))
            .count()
    };
    let asked = 40;
    for _ in 0..asked {
        h.bots[a].send(
            true,
            queue::WEENIE,
            &server_call(follow::FOLLOW, &[Arg::LongInt(other)]),
        );
    }
    h.run(5);
    let seen = orders(&h, from);
    assert!(
        seen > 0 && seen < asked,
        "{seen} orders of {asked} follows, two each"
    );
    h.advance(10.0);
    let from = h.bots[b].inbox.len();
    h.bots[a].send(
        true,
        queue::WEENIE,
        &server_call(follow::FOLLOW, &[Arg::LongInt(other)]),
    );
    h.run(10);
    assert!(orders(&h, from) > 0, "the pace has filled");
}

/// A player that dies while following another is told it stopped following just after its death
/// message, and follows no more: following again only starts. The one it followed is told
/// nothing.
/// Behaviour: server.follow.ends-at-death
#[test]
fn a_player_that_dies_while_following_follows_no_more() {
    const DEATH_MESSAGE: u32 = 0x8207_002B;
    let mut h = Harness::new();
    let (b, me) = h.player_in_world("one", "Elara");
    let (c, other) = h.player_in_world("two", "Torvald");
    h.run(5);
    h.bots[b].send(
        true,
        queue::WEENIE,
        &server_call(follow::FOLLOW, &[Arg::LongInt(other)]),
    );
    h.run(4);
    let world = h
        .server
        .players
        .iter()
        .find(|(_, p)| p.character == me)
        .map(|(w, _)| *w)
        .expect("in the world");
    let (from, theirs) = (h.bots[b].inbox.len(), h.bots[c].inbox.len());
    let clock = h.clock();
    yalain_world::death::player_dies(&mut h.server, &mut h.net, world, None, clock);
    h.run(4);
    assert_eq!(shown(&h, b, from), vec![follow::STOPPED.0]);
    let calls: Vec<u32> = h
        .world_messages(b, from)
        .iter()
        .filter_map(|m| match m {
            ServerMessage::PlayerCall(c) => Some(c.function.0),
            _ => None,
        })
        .collect();
    let died = calls.iter().position(|f| *f == DEATH_MESSAGE);
    assert!(died.is_some(), "its death message");
    assert_eq!(
        calls.get(died.map_or(0, |i| i + 1)),
        Some(&SHOW_TEXT),
        "told just after its death message: {calls:x?}"
    );
    assert!(
        shown(&h, c, theirs).is_empty(),
        "the one followed is told nothing"
    );

    let again = h.bots[b].inbox.len();
    h.bots[b].send(
        true,
        queue::WEENIE,
        &server_call(follow::FOLLOW, &[Arg::LongInt(other)]),
    );
    h.run(4);
    assert_eq!(shown(&h, b, again), vec![follow::STARTED.0]);
}
