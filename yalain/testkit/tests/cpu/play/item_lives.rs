//! Items that last only a while: an item of a template a world's row gives a lifetime is gone that
//! many seconds after it was made, from a pack, from where it lies, and from a pack kept while its
//! player was away. Fixture: a new character in the test hall, a made-up seal with a lifetime and a
//! made-up stone with none.

use auberean_protocol::{queue, ClientMessage, ServerMessage};
use yalain_net::ConnId;
use yalain_testkit::Harness;
use yalain_world::calls::{self, Value};
use yalain_world::holding::{self, Place, To};
use yalain_world::inventory;

const SEAL: u32 = 0x4700_7B01;
const LONG_SEAL: u32 = 0x4700_7B02;
const STONE: u32 = 0x4700_7B03;
/// The seal's lifetime, and the longer one's, in seconds.
const LIFE: u32 = 100;
const LONG_LIFE: u32 = 300;

/// A player in the hall, its scene drawn, the items' templates in the catalog and the seals'
/// lifetimes among the world's rows: its client, character and world connection.
fn hall() -> (Harness, usize, u64, ConnId) {
    let mut h = Harness::new();
    for t in [SEAL, LONG_SEAL, STONE] {
        h.server.catalog.insert(yalain_testkit::test_template(
            t,
            0x1067,
            yalain_content::Props::default(),
        ));
    }
    h.server.item_rows.lives.insert(SEAL, LIFE);
    h.server.item_rows.lives.insert(LONG_SEAL, LONG_LIFE);
    let (b, me) = h.player_in_world("one", "Elara");
    h.bots[b].send(
        true,
        queue::WEENIE,
        &ClientMessage::SceneRenderingComplete(auberean_protocol::login::Empty),
    );
    h.run(5);
    let world = world_of(&h, me).expect("in the world");
    (h, b, me, world)
}

fn world_of(h: &Harness, me: u64) -> Option<ConnId> {
    h.server
        .players
        .iter()
        .find(|(_, p)| p.character == me)
        .map(|(w, _)| *w)
}

fn holds(h: &Harness, me: u64, item: u64) -> bool {
    world_of(h, me).is_some_and(|w| h.server.players[&w].play.items.iter().any(|i| i.id == item))
}

/// Run the world for so many seconds, a second a step.
fn wait(h: &mut Harness, seconds: f64) {
    let t = h.now.0 + seconds;
    while h.now.0 < t {
        h.step_by(1.0);
    }
}

/// The items the client was told are gone from its pack since `from`.
fn destroyed(h: &Harness, b: usize, from: usize) -> Vec<u64> {
    h.world_messages(b, from)
        .iter()
        .filter_map(|m| match m {
            ServerMessage::PlayerCall(c)
                if c.function.0 == inventory::function::DISCRETE_DESTROY =>
            {
                c.args
                    .as_ref()
                    .and_then(|a| calls::values(&a.0).ok())
                    .and_then(|v| match v.first() {
                        Some(Value::Long(id)) => Some(*id),
                        _ => None,
                    })
            }
            _ => None,
        })
        .collect()
}

/// The objects the client was told to forget since `from`.
fn forgotten(h: &Harness, b: usize, from: usize) -> Vec<u64> {
    h.world_messages(b, from)
        .iter()
        .filter_map(|m| match m {
            ServerMessage::ForgetObject(o) => Some(o.0.id),
            _ => None,
        })
        .collect()
}

fn log_off(h: &mut Harness, b: usize, id: u64) {
    let before = h.bots[b].inbox.len();
    h.bots[b].send(
        false,
        queue::LOGON,
        &ClientMessage::LogOff(auberean_protocol::login::CharacterRef { character: id }),
    );
    assert!(h.run_until(400, |h| h.bots[b].inbox[before..]
        .iter()
        .any(|r| matches!(r.message(), Some(ServerMessage::CharacterList(_))))));
}

/// An item of a template with a lifetime stays in its holder's pack until its end, and at the
/// world's step after it is taken out, the client told it is gone from the pack and forgetting it;
/// an item of a template with none stays. One given later joins no stack of an earlier one: each
/// keeps its own end.
/// Behaviour: server.items.an-item-with-a-lifetime-is-gone-from-a-pack-at-its-end
#[test]
fn an_item_with_a_lifetime_stays_in_its_pack_until_its_end_and_is_gone_after() {
    let (mut h, b, me, world) = hall();
    let made = h.server.game_now;
    let seal = inventory::give(&mut h.server, &mut h.net, world, SEAL, 1).expect("given");
    let stone = inventory::give(&mut h.server, &mut h.net, world, STONE, 1).expect("given");
    assert_eq!(
        h.server.things.get(seal).and_then(|t| t.own.ends),
        Some(made + f64::from(LIFE)),
        "its end from when it was made"
    );
    assert_eq!(h.server.things.get(stone).and_then(|t| t.own.ends), None);
    let from = h.bots[b].inbox.len();
    wait(&mut h, f64::from(LIFE) - 2.0);
    assert!(holds(&h, me, seal), "kept before its end");
    assert!(destroyed(&h, b, from).is_empty());
    inventory::give_stacked(&mut h.server, &mut h.net, world, SEAL, 1);
    let later: Vec<u64> = h.server.players[&world]
        .play
        .items
        .iter()
        .filter(|i| i.template == SEAL && i.id != seal)
        .map(|i| i.id)
        .collect();
    assert_eq!(later.len(), 1, "a stack of its own");
    let later = later[0];
    wait(&mut h, 4.0);
    assert!(!holds(&h, me, seal), "gone after its end");
    assert!(h.server.things.get(seal).is_none(), "out of the world");
    assert_eq!(destroyed(&h, b, from), [seal], "told it is gone");
    assert!(forgotten(&h, b, from).contains(&seal), "and forgets it");
    assert!(holds(&h, me, stone), "one with no lifetime stays");
    assert!(holds(&h, me, later), "the later one on its own end");
    wait(&mut h, f64::from(LIFE));
    assert!(!holds(&h, me, later));
    wait(&mut h, 1000.0);
    assert!(holds(&h, me, stone));
}

/// An item with a lifetime lying in the world is gone from it at the world's step after its end,
/// every client that sees it told to forget it.
/// Behaviour: server.items.an-item-with-a-lifetime-is-gone-from-where-it-lies-at-its-end
#[test]
fn an_item_with_a_lifetime_lying_in_the_world_is_gone_from_it_at_its_end() {
    let (mut h, b, _me, world) = hall();
    let at = h.server.players[&world].position;
    let seal = inventory::make_item(&mut h.server, SEAL, 1, 0).expect("made");
    holding::move_item(&mut h.server, seal, Place::Nowhere, To::Ground(at)).expect("down");
    yalain_world::presence::thing_appears(&mut h.server, &mut h.net, seal);
    let from = h.bots[b].inbox.len();
    wait(&mut h, f64::from(LIFE) - 2.0);
    assert_eq!(holding::place_of(&h.server, seal), Some(Place::Ground));
    assert!(!forgotten(&h, b, from).contains(&seal));
    wait(&mut h, 4.0);
    assert!(h.server.things.get(seal).is_none(), "gone");
    assert!(forgotten(&h, b, from).contains(&seal), "forgotten");
}

/// A character entering the world again does not bring in an item whose end passed while it was
/// away: its pack never lists it and its client is never shown it. One whose end has not come
/// comes back with its end as it was and goes at it.
/// Behaviour: server.items.an-item-whose-end-passed-while-away-stays-behind
#[test]
fn an_item_whose_end_passed_while_its_holder_was_away_is_not_brought_back() {
    let (mut h, b, me, world) = hall();
    let seal = inventory::give(&mut h.server, &mut h.net, world, SEAL, 1).expect("given");
    let long = inventory::give(&mut h.server, &mut h.net, world, LONG_SEAL, 1).expect("given");
    let stone = inventory::give(&mut h.server, &mut h.net, world, STONE, 1).expect("given");
    let ends = h.server.things.get(long).and_then(|t| t.own.ends);
    h.run(5);
    log_off(&mut h, b, me);
    wait(&mut h, f64::from(LIFE) + 50.0);
    let from = h.bots[b].inbox.len();
    h.enter_world(b, "one", me);
    h.run(10);
    assert!(!holds(&h, me, seal), "left behind");
    assert!(h.server.things.get(seal).is_none());
    let shown: Vec<u64> = h
        .world_messages(b, from)
        .iter()
        .filter_map(|m| match m {
            ServerMessage::CreateObject(c) => Some(c.id),
            _ => None,
        })
        .collect();
    assert!(!shown.contains(&seal), "never shown");
    assert!(
        shown.contains(&long) && shown.contains(&stone),
        "the others shown"
    );
    assert!(holds(&h, me, long) && holds(&h, me, stone));
    assert_eq!(
        h.server.things.get(long).and_then(|t| t.own.ends),
        ends,
        "its end as it was"
    );
    let left = ends.expect("an end") - h.server.game_now;
    wait(&mut h, left - 2.0);
    assert!(holds(&h, me, long), "kept until its end");
    wait(&mut h, 4.0);
    assert!(!holds(&h, me, long), "gone at it");
    assert!(holds(&h, me, stone));
}
