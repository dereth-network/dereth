//! Following: a player follows another, is told so and given one standing movement order toward
//! it; asking again ends the old follow first; the player's own moving ends it, and so do its
//! skills, its trips and the one followed leaving the world, and by this server's own rule a use's
//! request; the trip and the death of the one followed do not. Orders are read in the order the
//! server sent them. Fixture: two players of the test world side by side, on the test clock.

use auberean_primitives::Vec3;
use auberean_protocol::physics::ClientPosition;
use auberean_protocol::{queue, ClientMessage, ServerMessage};
use auberean_script::args::Arg;
use yalain_net::ConnId;
use yalain_testkit::play::server_call;
use yalain_testkit::{Harness, EASE_SUFFERING, LUMBERING_MIGHT, TEST_CREATURE};
use yalain_world::calls::{self, Node, Value};
use yalain_world::follow;
use yalain_world::places;
use yalain_world::skill_use::function as skill;

const SHOW_TEXT: u32 = 0x8205_0011;
const ATTACK_STATE: u32 = 0x8204_001B;
const SWORD: u32 = 0x4700_051C;
const RIPOSTE: u32 = 176;
const BOW: u32 = 0x4700_051B;
const DEADEYE: u32 = 207;

/// The texts a client was shown since `from`, by string id.
fn shown(h: &Harness, b: usize, from: usize) -> Vec<u32> {
    h.sent_messages(b, from)
        .iter()
        .filter_map(text_of)
        .collect()
}

/// The string id of the text a message shows, if it shows one.
fn text_of(m: &ServerMessage) -> Option<u32> {
    let ServerMessage::PlayerCall(c) = m else {
        return None;
    };
    if c.function.0 != SHOW_TEXT {
        return None;
    }
    let v = c.args.as_ref().and_then(|a| calls::values(&a.0).ok())?;
    v.iter().find_map(|x| match x {
        Value::Graph(g) => g.iter().find_map(|(_, n)| match n {
            Node::TextInfo(t) => Some(t.string_id),
            _ => None,
        }),
        _ => None,
    })
}

/// What a follow's end is told by, and what it is told among.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Told {
    OrderStopped,
    Stopped,
    Flash,
    Teleporting,
    Exit,
    Refused,
    Lowered,
    Peace,
}

/// What a client was sent of a follow's end since `from`, in the order the server sent it.
fn told(h: &Harness, b: usize, from: usize, me: u64) -> Vec<Told> {
    h.sent_messages(b, from)
        .iter()
        .filter_map(|m| match m {
            ServerMessage::MoveTo(e) if e.sender.id == me && e.action.params.flags == 0x40 => {
                Some(Told::OrderStopped)
            }
            ServerMessage::DoFx(e) if e.action.fx == places::fx::PORTAL_USED => Some(Told::Flash),
            ServerMessage::DoMode(e)
                if e.sender.id == me && e.action.0 == yalain_world::combat::mode::PEACE =>
            {
                Some(Told::Peace)
            }
            ServerMessage::PlayerCall(c) if c.function.0 == skill::REFUSED => Some(Told::Refused),
            ServerMessage::PlayerCall(c) if c.function.0 == places::function::EXIT_PORTAL_SPACE => {
                Some(Told::Exit)
            }
            ServerMessage::PlayerCall(c) if c.function.0 == ATTACK_STATE => {
                let v = c.args.as_ref().and_then(|a| calls::values(&a.0).ok())?;
                matches!(v.first(), Some(Value::Int(0))).then_some(Told::Lowered)
            }
            _ => match text_of(m)? {
                id if id == follow::STOPPED.0 => Some(Told::Stopped),
                id if id == places::text::TELEPORTING.0 => Some(Told::Teleporting),
                _ => None,
            },
        })
        .collect()
}

/// The movement orders a client was sent for its player since `from`: (flags, distance).
fn orders(h: &Harness, b: usize, from: usize, me: u64) -> Vec<(u32, f32)> {
    h.sent_messages(b, from)
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
/// meter of it; following again first stops ("Stopped following.", the order stopped).
/// Behaviour: server.social.follow-starts-and-stops
#[test]
fn a_player_follows_another_until_it_follows_again() {
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
}

/// This server ends a follow as a use of an object is requested (its own rule): told so, the
/// order stopped.
/// Behaviour: server.follow.ends-at-a-use-request
#[test]
fn the_server_ends_a_follow_as_a_use_is_requested() {
    let (mut h, b, me, _, other) = following();
    let used = h.bots[b].inbox.len();
    h.bots[b].send(
        true,
        queue::WEENIE,
        &server_call(0x8216_0016, &[yalain_testkit::play::use_of(other, me)]),
    );
    h.run(4);
    assert!(told_stopped(&h, b, used, me), "{:?}", shown(&h, b, used));
    assert_eq!(follows(&h, me), None);
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
        .sent_messages(b, from)
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

/// Two players side by side, the scene drawn for each, the first following the second: the
/// harness, the follower's client and player, the followed one's client and player.
fn following() -> (Harness, usize, u64, usize, u64) {
    let (mut h, b, me, c, other) = side_by_side(Harness::new());
    follow_now(&mut h, b, me, other);
    (h, b, me, c, other)
}

/// Two players side by side in this harness, the scene drawn for each, following nothing.
fn side_by_side(mut h: Harness) -> (Harness, usize, u64, usize, u64) {
    let (b, me) = h.player_in_world("one", "Elara");
    let (c, other) = h.player_in_world("two", "Torvald");
    for bot in [b, c] {
        h.bots[bot].send(
            true,
            queue::WEENIE,
            &ClientMessage::SceneRenderingComplete(auberean_protocol::login::Empty),
        );
    }
    h.run(5);
    (h, b, me, c, other)
}

fn follow_now(h: &mut Harness, b: usize, me: u64, other: u64) {
    h.bots[b].send(
        true,
        queue::WEENIE,
        &server_call(follow::FOLLOW, &[Arg::LongInt(other)]),
    );
    h.run(4);
    assert_eq!(follows(h, me), Some(other), "following");
}

fn world_of(h: &Harness, character: u64) -> ConnId {
    yalain_world::social::world_of(&h.server, character).expect("in the world")
}

/// What a player follows, if anything.
fn follows(h: &Harness, character: u64) -> Option<u64> {
    h.server.players[&world_of(h, character)].following
}

/// Whether the client was sent the follow's stop since `from`: "Stopped following." once, and the
/// player's order stopped once.
fn told_stopped(h: &Harness, b: usize, from: usize, me: u64) -> bool {
    shown(h, b, from) == vec![follow::STOPPED.0]
        && orders(h, b, from, me)
            .iter()
            .filter(|o| o.0 == 0x40)
            .count()
            == 1
}

/// The follower's report of its place, half a meter on, by its own hand or under the order, with
/// a movement order's stamp.
fn report(h: &Harness, me: u64, by_hand: bool, move_to_stamp: u16) -> ClientMessage {
    let p = &h.server.players[&world_of(h, me)];
    let here = p.position;
    ClientMessage::Position(ClientPosition {
        time: 0.0,
        cell: here.cell,
        origin: Vec3::new(here.origin[0] + 0.5, here.origin[1], here.origin[2]),
        heading_motion: yalain_world::world::IDLE_MOTION,
        flags: if by_hand {
            yalain_world::places::MOVED_BY_HAND
        } else {
            0
        },
        instance_stamp: 1,
        teleport_stamp: p.teleport_stamp,
        force_position_stamp: p.force_position_stamp,
        move_to_stamp,
        jump_velocity: Vec3::ZERO,
    })
}

/// A follower reporting under the order, or by its own hand before it took the order up (an older
/// stamp), keeps following; its first report by hand under the order's stamp ends the follow: told
/// "Stopped following.", the order stopped for it and for the one it followed, who sees it. A use
/// asked after that stops no follow again.
/// Behaviour: server.follow.ends-when-moved-by-hand
#[test]
fn a_follower_moving_by_its_own_hand_stops_following_once() {
    let (mut h, b, me, c, other) = following();
    let stamp = h.server.players[&world_of(&h, me)].move_to_stamp;
    let from = h.bots[b].inbox.len();
    let under = report(&h, me, false, stamp);
    h.bots[b].send(true, queue::WEENIE, &under);
    h.run(4);
    let early = report(&h, me, true, stamp.wrapping_sub(1));
    h.bots[b].send(true, queue::WEENIE, &early);
    h.run(4);
    assert!(shown(&h, b, from).is_empty(), "under the order: nothing");
    assert_eq!(follows(&h, me), Some(other));

    let (seen, theirs) = (h.bots[b].inbox.len(), h.bots[c].inbox.len());
    let moved = report(&h, me, true, stamp);
    h.bots[b].send(true, queue::WEENIE, &moved);
    h.run(4);
    assert!(told_stopped(&h, b, seen, me), "{:?}", shown(&h, b, seen));
    assert_eq!(
        orders(&h, c, theirs, me),
        vec![(0x40, 0.0)],
        "the one followed sees it stop"
    );
    assert_eq!(follows(&h, me), None);

    let used = h.bots[b].inbox.len();
    h.bots[b].send(
        true,
        queue::WEENIE,
        &server_call(0x8216_0016, &[yalain_testkit::play::use_of(other, me)]),
    );
    h.run(4);
    assert!(
        shown(&h, b, used).is_empty(),
        "no stray stop at a later use"
    );
}

/// Move a player out of the other's view by a trip, and let it land.
fn trips_far(h: &mut Harness, who: u64) {
    let w = world_of(h, who);
    let mut to = h.server.players[&w].position;
    to.cell = 0x7000_0001;
    to.origin = [10.0, 10.0, 50.0];
    let clock = h.clock();
    yalain_world::places::sent_away(&mut h.server, &mut h.net, w, to, clock);
    assert!(
        h.run_until(100, |h| h.server.players[&w].trip.is_none()),
        "landed"
    );
    h.run(4);
}

/// The followed player's trip out of the follower's view leaves the follow on: the follower sees it
/// leave and is told nothing, its order standing.
/// Behaviour: server.follow.ends-when-the-followed-leaves
#[test]
fn a_follow_goes_on_through_the_followed_players_trip() {
    let (mut h, b, me, _, other) = following();
    let from = h.bots[b].inbox.len();
    trips_far(&mut h, other);
    assert!(
        h.world_messages(b, from)
            .iter()
            .any(|m| matches!(m, ServerMessage::ObjectLeaves(e) if e.sender.id == other)),
        "seen to leave"
    );
    assert_eq!(follows(&h, me), Some(other), "still following");
    assert!(shown(&h, b, from).is_empty(), "told nothing");
    assert!(orders(&h, b, from, me).is_empty(), "its order standing");
}

/// The one followed logging off leaves the world: the follower stops following, told so and its
/// order stopped, whether it still had it in view or a trip had taken it out of view.
/// Behaviour: server.follow.ends-when-the-followed-leaves
/// Behaviour: server.follow.ends-for-every-follower-as-what-it-follows-leaves
#[test]
fn a_follower_stops_when_the_one_it_follows_leaves_the_world() {
    use auberean_protocol::login::CharacterRef;
    for out_of_view in [false, true] {
        let (mut h, b, me, c, other) = following();
        if out_of_view {
            trips_far(&mut h, other);
        }
        assert_eq!(
            h.server.players[&world_of(&h, me)]
                .known
                .contains_key(&other),
            !out_of_view,
            "in view"
        );
        let from = h.bots[b].inbox.len();
        let before = h.bots[c].inbox.len();
        h.bots[c].send(
            false,
            queue::LOGON,
            &ClientMessage::LogOff(CharacterRef { character: other }),
        );
        assert!(h.run_until(400, |h| h.bots[c].inbox[before..]
            .iter()
            .any(|r| matches!(r.message(), Some(ServerMessage::CharacterList(_))))));
        h.run(4);
        assert!(
            told_stopped(&h, b, from, me),
            "out of view {out_of_view}: {:?}",
            shown(&h, b, from)
        );
        assert_eq!(follows(&h, me), None, "out of view {out_of_view}");
    }
}

/// A followed thing gone from the world ends the follow, whether the follower still had it in view
/// or not.
/// Behaviour: server.follow.ends-for-every-follower-as-what-it-follows-leaves
#[test]
fn a_follower_stops_when_the_thing_it_follows_is_gone_in_view_or_out_of_it() {
    for out_of_view in [false, true] {
        let (mut h, b, me, beetle) = following_beside_a_beetle(SWORD, RIPOSTE);
        follow_now(&mut h, b, me, beetle);
        if out_of_view {
            out_of_sight(&mut h, me, beetle);
        }
        let from = h.bots[b].inbox.len();
        let instance = h.server.things.get(beetle).expect("standing").instance;
        yalain_world::presence::thing_gone(&mut h.server, &mut h.net, beetle, instance);
        assert_eq!(follows(&h, me), None, "out of view {out_of_view}");
        h.run(4);
        assert!(
            told_stopped(&h, b, from, me),
            "out of view {out_of_view}: {:?}",
            shown(&h, b, from)
        );
    }
}

/// The follower's view drops what it follows: the player stands in a far cell, and its view is
/// drawn again.
fn out_of_sight(h: &mut Harness, me: u64, followed: u64) {
    let w = world_of(h, me);
    let p = h.server.players.get_mut(&w).expect("in the world");
    p.position.cell = 0x7000_0001;
    p.position.origin = [10.0, 10.0, 50.0];
    yalain_world::presence::refresh(&mut h.server, &mut h.net, w);
    let p = &h.server.players[&w];
    assert!(!p.known.contains_key(&followed), "out of view");
    assert_eq!(p.following, Some(followed), "still following");
}

/// The followed player dying leaves the follow on, in its death's tick and on past its corpse's
/// making: the follower told nothing, its order standing.
/// Behaviour: server.follow.goes-on-past-the-followed-players-death
#[test]
fn a_follow_goes_on_through_the_followed_players_death() {
    let (mut h, b, me, _, other) = following();
    let from = h.bots[b].inbox.len();
    let clock = h.clock();
    let w = world_of(&h, other);
    yalain_world::death::player_dies(&mut h.server, &mut h.net, w, None, clock);
    assert_eq!(follows(&h, me), Some(other), "in the death's tick");
    h.run_to(yalain_net::LocalTime(
        h.now.0 + yalain_world::death::PLAYER_CORPSE_AFTER + 1.0,
    ));
    assert_eq!(follows(&h, me), Some(other), "past its corpse's making");
    assert!(shown(&h, b, from).is_empty(), "told nothing");
    assert!(orders(&h, b, from, me).is_empty(), "its order standing");
}

/// A follow outlives the death of the creature followed and ends as its body leaves, whether it
/// leaves a corpse or none and whether the follower still has it in view or not: in the death's
/// tick and for as long as the body lies there the order stands and nothing is told; as it leaves,
/// told so once and the order stopped once.
/// Behaviour: server.follow.ends-when-the-followed-creatures-body-leaves
#[test]
fn a_follow_outlives_the_creatures_death_and_ends_when_the_body_leaves() {
    for (leaves_no_corpse, out_of_view) in
        [(false, false), (true, false), (false, true), (true, true)]
    {
        let (mut h, b, me, beetle) = following_beside_a_beetle(SWORD, RIPOSTE);
        if let Some(c) = h
            .server
            .things
            .get_mut(beetle)
            .and_then(|t| t.creature.as_mut())
        {
            c.stats.never_leaves_corpse = leaves_no_corpse;
        }
        follow_now(&mut h, b, me, beetle);
        if out_of_view {
            out_of_sight(&mut h, me, beetle);
        }
        let from = h.bots[b].inbox.len();
        let clock = h.clock();
        yalain_world::death::creature_dies(&mut h.server, &mut h.net, beetle, me, clock);
        assert_eq!(
            follows(&h, me),
            Some(beetle),
            "no corpse {leaves_no_corpse}, out of view {out_of_view}: in the death's tick"
        );
        // Every step the body lies there the follow is on; the step it leaves (after its corpse's
        // making, or hidden), the follow is over.
        #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
        let steps = (20.0 / yalain_testkit::STEP) as usize;
        let mut lay = 0;
        while h.server.things.get(beetle).is_some() && lay < steps {
            assert_eq!(
                follows(&h, me),
                Some(beetle),
                "no corpse {leaves_no_corpse}, out of view {out_of_view}: while the body lies there"
            );
            h.step();
            lay += 1;
        }
        assert!(
            lay > 1 && h.server.things.get(beetle).is_none(),
            "no corpse {leaves_no_corpse}, out of view {out_of_view}: its body lay a while and has left"
        );
        assert_eq!(
            h.server.things.corpses().next().is_none(),
            leaves_no_corpse,
            "no corpse {leaves_no_corpse}, out of view {out_of_view}: what it left"
        );
        assert_eq!(
            follows(&h, me),
            None,
            "no corpse {leaves_no_corpse}, out of view {out_of_view}: as the body leaves"
        );
        h.run(4);
        // Among what the kill shows: "Stopped following." once, and the order stopped once.
        let stops = shown(&h, b, from)
            .iter()
            .filter(|t| **t == follow::STOPPED.0)
            .count();
        assert_eq!(
            stops, 1,
            "no corpse {leaves_no_corpse}, out of view {out_of_view}: told once"
        );
        assert_eq!(
            orders(&h, b, from, me),
            vec![(0x40, 0.0)],
            "no corpse {leaves_no_corpse}, out of view {out_of_view}: the order stopped once"
        );
    }
}

/// A trip told no "Teleporting ..." (a recall, a resurrection, an item's trip, being sent away)
/// leaves the follow on in portal space and ends it as the player comes out: sent the order
/// stopped, "Stopped following.", then the exit from portal space.
/// Behaviour: server.follow.ends-at-a-trip
#[test]
fn a_follower_on_a_trip_with_no_text_stops_following_as_it_comes_out() {
    #[derive(Debug, Clone, Copy, PartialEq, Eq)]
    enum Trip {
        Recall,
        Resurrection,
        Item,
        SentAway,
    }
    for trip in [Trip::Recall, Trip::Resurrection, Trip::Item, Trip::SentAway] {
        let (mut h, b, me, _, other) = side_by_side(Harness::new());
        let item = h.arm(b, me, SWORD);
        follow_now(&mut h, b, me, other);
        let from = h.bots[b].inbox.len();
        let clock = h.clock();
        let w = world_of(&h, me);
        let here = h.server.players[&w].position;
        let mut to = here;
        to.origin[0] += 30.0;
        let (s, net) = (&mut h.server, &mut h.net);
        match trip {
            Trip::Recall => places::recall_trip(s, net, w, to, None, clock),
            Trip::Resurrection => places::resurrection_trip(s, net, w, to, clock),
            Trip::Item => {
                let used = places::walked_in_use(item, me, here);
                assert!(places::item_trip(s, net, w, (to, used, None), clock));
            }
            Trip::SentAway => places::sent_away(s, net, w, to, clock),
        }
        h.run(4);
        assert_eq!(follows(&h, me), Some(other), "{trip:?}: in portal space");
        assert!(
            told(&h, b, from, me).is_empty(),
            "{trip:?}: told nothing yet"
        );
        // A recall is let out by what began it.
        if trip == Trip::Recall {
            let clock = h.clock();
            places::recall_arrives(&mut h.server, &mut h.net, w, clock);
        }
        assert!(
            h.run_until(100, |h| h.server.players[&w].trip.is_none()),
            "{trip:?}: out"
        );
        h.run(4);
        assert_eq!(follows(&h, me), None, "{trip:?}: as it comes out");
        assert_eq!(
            told(&h, b, from, me),
            [Told::OrderStopped, Told::Stopped, Told::Exit],
            "{trip:?}"
        );
    }
}

/// A follower that walks into a placed portal under the follow's order stops following before
/// anything of the trip is told: sent the order stopped, "Stopped following.", the portal's flash,
/// then "Teleporting ...".
/// Behaviour: server.follow.ends-at-a-trip
#[test]
fn a_follower_walking_into_a_portal_stops_following_before_it_is_told_it_is_teleporting() {
    use yalain_content::{ContentObject, ContentPack, Description, Facts, Kind, Place};
    const PACKAGE: u32 = 0x1A01;
    let mut h = Harness::new();
    h.server.content = ContentPack {
        objects: vec![ContentObject {
            object: Some(0x4002_9900_0000_0058),
            template: 0x4700_1154,
            description: Description {
                name_text: None,
                package: PACKAGE,
                look: 0x1F00_0ED5,
                enter_fx: 0,
                scale: None,
                name: None,
                numbers: [0; 6],
                source: "authored for a test".into(),
            },
            cell: 0x0299_0109,
            origin: [70.0, 240.0, -44.6],
            rotation: [1.0, 0.0, 0.0, 0.0],
            kind: Kind::Static,
            label: String::new(),
            placed: "authored for a test".into(),
            destination: Some(Place {
                cell: 0x7000_0001,
                origin: [10.0, 10.0, 50.0],
                rotation: [1.0, 0.0, 0.0, 0.0],
            }),
            facts: Facts::default(),
            walks: None,
            gate: None,
        }],
        ..ContentPack::default()
    };
    h.server.walk_in_packages.insert(PACKAGE);
    let (mut h, b, me, _, other) = side_by_side(h);
    follow_now(&mut h, b, me, other);
    let p = &h.server.players[&world_of(&h, me)];
    let into = ClientMessage::Position(ClientPosition {
        time: 0.0,
        cell: p.position.cell,
        origin: Vec3::new(70.0, 240.0, -44.6),
        heading_motion: yalain_world::world::IDLE_MOTION,
        flags: 0,
        instance_stamp: 1,
        teleport_stamp: p.teleport_stamp,
        force_position_stamp: p.force_position_stamp,
        move_to_stamp: p.move_to_stamp,
        jump_velocity: Vec3::ZERO,
    });
    let from = h.bots[b].inbox.len();
    h.bots[b].send(true, queue::WEENIE, &into);
    h.run(10);
    assert_eq!(
        told(&h, b, from, me),
        [
            Told::OrderStopped,
            Told::Stopped,
            Told::Flash,
            Told::Teleporting
        ],
        "as it goes"
    );
    assert_eq!(follows(&h, me), None, "in portal space");
}

/// A skill's use that gives the follower no movement order (a heal on itself) leaves the follow on
/// while the use is under way; when the use is over the follow ends, sent in this order: the order
/// stopped once, the attack state lowered, peace, then told so.
/// Behaviour: server.follow.ends-as-a-use-is-over
#[test]
fn a_skill_used_where_the_follower_stands_ends_the_follow_when_it_is_over() {
    let (mut h, b, me, _, other) = side_by_side(Harness::new());
    h.arm(b, me, SWORD);
    follow_now(&mut h, b, me, other);
    let w = world_of(&h, me);
    if let Some(p) = h.server.players.get_mut(&w) {
        use yalain_world::player::quality;
        // Vigor enough for the heal's cost (a fresh human's 100 is not).
        yalain_world::quals::set_int(&mut p.qualities, quality::VIGOR_MAX, 400);
        yalain_world::quals::set_int(&mut p.qualities, quality::VIGOR, 400);
    }
    let from = h.bots[b].inbox.len();
    h.bots[b].send(
        true,
        queue::WEENIE,
        &server_call(
            skill::USE,
            &[Arg::Int(3), Arg::Int(EASE_SUFFERING), Arg::LongInt(0)],
        ),
    );
    h.run(3);
    assert!(h.server.players[&w].using.is_some(), "the use is under way");
    assert_eq!(follows(&h, me), Some(other), "still following");
    assert!(shown(&h, b, from).is_empty());
    assert!(h.run_until(200, |h| h.server.players[&w].using.is_none()));
    h.run(2);
    assert!(told_stopped(&h, b, from, me), "{:?}", shown(&h, b, from));
    assert_eq!(told(&h, b, from, me), USE_OVER);
}

/// A use's end that ends a follow, as it is sent.
const USE_OVER: [Told; 4] = [
    Told::OrderStopped,
    Told::Lowered,
    Told::Peace,
    Told::Stopped,
];

/// A standing use cut short by a stop before it is over ends the follow the same way: the order
/// stopped first, then why, the attack state lowered, peace, and told last.
/// Behaviour: server.follow.ends-as-a-use-is-over
#[test]
fn a_skill_cut_short_ends_the_follow_with_the_stop_first_and_the_line_last() {
    let (mut h, b, me, _, other) = side_by_side(Harness::new());
    h.arm(b, me, SWORD);
    follow_now(&mut h, b, me, other);
    let w = world_of(&h, me);
    if let Some(p) = h.server.players.get_mut(&w) {
        use yalain_world::player::quality;
        yalain_world::quals::set_int(&mut p.qualities, quality::VIGOR_MAX, 400);
        yalain_world::quals::set_int(&mut p.qualities, quality::VIGOR, 400);
    }
    if let Some(u) = h
        .server
        .skill_table
        .skills
        .get_mut(&EASE_SUFFERING)
        .and_then(|d| d.usage.as_mut())
    {
        u.ui_flags &= !yalain_world::skill_use::ui_flag::USE_WHILE_MOVING;
    }
    let from = h.bots[b].inbox.len();
    h.bots[b].send(
        true,
        queue::WEENIE,
        &server_call(
            skill::USE,
            &[Arg::Int(3), Arg::Int(EASE_SUFFERING), Arg::LongInt(0)],
        ),
    );
    h.run(3);
    assert!(h.server.players[&w].using.is_some(), "the use is under way");
    h.bots[b].send(
        true,
        queue::WEENIE,
        &server_call(yalain_world::combat::function::STOP_ATTACK, &[]),
    );
    h.run(4);
    assert!(h.server.players[&w].using.is_none(), "cut short");
    assert_eq!(
        told(&h, b, from, me),
        [
            Told::OrderStopped,
            Told::Refused,
            Told::Lowered,
            Told::Peace,
            Told::Stopped
        ]
    );
}

/// A toggled skill used where the follower stands ends the follow as it turns on, as any use's end
/// does: the order stopped, the attack state lowered, peace, then told.
/// Behaviour: server.follow.ends-as-a-use-is-over
#[test]
fn a_skill_toggled_on_ends_the_follow_with_the_stop_first_and_the_line_last() {
    let (mut h, b, me, _, other) = side_by_side(Harness::new());
    h.arm(b, me, SWORD);
    h.all_hold(LUMBERING_MIGHT);
    let w = world_of(&h, me);
    if let Some(p) = h.server.players.get_mut(&w) {
        use yalain_world::player::quality;
        yalain_world::quals::set_int(&mut p.qualities, quality::VIGOR_MAX, 2000);
        yalain_world::quals::set_int(&mut p.qualities, quality::VIGOR, 2000);
    }
    follow_now(&mut h, b, me, other);
    let from = h.bots[b].inbox.len();
    h.bots[b].send(
        true,
        queue::WEENIE,
        &server_call(
            skill::USE,
            &[Arg::Int(1), Arg::Int(LUMBERING_MIGHT), Arg::LongInt(0)],
        ),
    );
    assert!(h.run_until(400, |h| shown(h, b, from).contains(&follow::STOPPED.0)));
    h.run(2);
    assert_eq!(told(&h, b, from, me), USE_OVER);
}

/// A skill's use refused while following (a heal with nothing in hand) ends the follow at the
/// refusal: told so after it, the order stopped.
/// Behaviour: server.follow.ends-at-a-refused-use
#[test]
fn a_refused_skill_use_ends_the_follow_after_its_refusal() {
    let (mut h, b, me, _, _) = following();
    let from = h.bots[b].inbox.len();
    h.bots[b].send(
        true,
        queue::WEENIE,
        &server_call(
            skill::USE,
            &[Arg::Int(3), Arg::Int(EASE_SUFFERING), Arg::LongInt(0)],
        ),
    );
    h.run(4);
    assert!(told_stopped(&h, b, from, me), "{:?}", shown(&h, b, from));
    let calls: Vec<u32> = h
        .sent_messages(b, from)
        .iter()
        .filter_map(|m| match m {
            ServerMessage::PlayerCall(c) => Some(c.function.0),
            _ => None,
        })
        .collect();
    let refused = calls.iter().position(|f| *f == skill::REFUSED);
    let told_at = calls.iter().position(|f| *f == SHOW_TEXT);
    assert!(refused.is_some() && refused < told_at, "{calls:x?}");
    // Refused at its start, the follow's stop comes after the lowering.
    assert_eq!(
        told(&h, b, from, me),
        [
            Told::Refused,
            Told::Lowered,
            Told::OrderStopped,
            Told::Stopped
        ]
    );
}

/// A player beside the test camp's beetle (a practice target, so it stays put), holding a weapon
/// and a skill and following the other player; returns the harness, its client, the player and
/// the beetle.
fn following_beside_a_beetle(weapon: u32, skill: u32) -> (Harness, usize, u64, u64) {
    let mut h = Harness::new();
    h.add_creatures(1);
    let (mut h, b, me, _, other) = side_by_side(h);
    let beetle = h
        .world_messages(b, 0)
        .iter()
        .find_map(|m| match m {
            ServerMessage::CreateObject(o) if o.weenie.entity_did == TEST_CREATURE => Some(o.id),
            _ => None,
        })
        .expect("the camp's creature");
    if let Some(c) = h
        .server
        .things
        .get_mut(beetle)
        .and_then(|t| t.creature.as_mut())
    {
        c.stats.fights = false;
    }
    h.arm(b, me, weapon);
    h.all_hold(skill);
    follow_now(&mut h, b, me, other);
    (h, b, me, beetle)
}

/// The follower's movement-order flags since `from`, after a skill's use on `target` and four
/// steps; and the texts it was shown.
fn used_on(h: &mut Harness, b: usize, me: u64, skill_id: u32, target: u64) -> (Vec<u32>, Vec<u32>) {
    let from = h.bots[b].inbox.len();
    h.bots[b].send(
        true,
        queue::WEENIE,
        &server_call(
            skill::USE,
            &[Arg::Int(1), Arg::Int(skill_id), Arg::LongInt(target)],
        ),
    );
    h.run(4);
    let flags = orders(h, b, from, me).iter().map(|o| o.0).collect();
    (flags, shown(h, b, from))
}

/// A skill that walks to its target (Riposte at a beetle) ends the follow as the walk begins: the
/// follow's stop, then the walk, in one tick.
/// Behaviour: server.follow.ends-as-a-walk-begins
#[test]
fn a_skill_that_walks_to_its_target_ends_the_follow_as_the_walk_begins() {
    let (mut h, b, me, beetle) = following_beside_a_beetle(SWORD, RIPOSTE);
    let (flags, texts) = used_on(&mut h, b, me, RIPOSTE, beetle);
    assert_eq!(texts, vec![follow::STOPPED.0]);
    assert_eq!(
        flags.first(),
        Some(&0x40),
        "the follow's stop first: {flags:x?}"
    );
    assert_eq!(flags.get(1), Some(&0x46B), "then the walk: {flags:x?}");
}

/// A skill that turns the follower to its target (Deadeye at a beetle, from afar) ends the follow
/// as it turns: the follow's stop, then the turn, in one tick; not when the shot is over.
/// Behaviour: server.follow.ends-as-a-use-turns
#[test]
fn a_skill_that_turns_the_follower_to_its_target_ends_the_follow_as_it_turns() {
    let (mut h, b, me, beetle) = following_beside_a_beetle(BOW, DEADEYE);
    let (flags, texts) = used_on(&mut h, b, me, DEADEYE, beetle);
    assert_eq!(texts, vec![follow::STOPPED.0]);
    assert_eq!(
        flags.first(),
        Some(&0x40),
        "the follow's stop first: {flags:x?}"
    );
    assert_eq!(flags.get(1), Some(&0x46A), "then the turn: {flags:x?}");
    let w = world_of(&h, me);
    assert!(
        h.server.players[&w].using.is_some(),
        "the shot is under way"
    );
}
