//! The world database's camps taken again while the server runs: a new camp makes its creatures at
//! the world's next step; a camp taken away takes its creatures at rest out of the world at once,
//! and one fighting a player once its fight is over, none replaced; a camp keeping more or fewer
//! keeps its creatures, one moved makes its creatures anew; and a reload never touches what a
//! player has or left lying. Fixture: the test hall with a camp of beetles beside the start (calm
//! ones that notice nobody, unless a test wants them to fight), a player in the world, its scene
//! drawn.

use auberean_protocol::{queue, ServerMessage};
use yalain_content::props::creature_property;
use yalain_content::Prop;
use yalain_testkit::{test_camp, test_catalog, Harness, TEST_CREATURE, TEST_ITEM};
use yalain_world::reload::table;
use yalain_world::things::{Corpse, Kind, Thing};

use super::reload::{reload, tables_of};

/// The hall with a camp of `n` beetles (calm ones, or ones that fight a player near), and a player
/// in the world beside it, its scene drawn.
fn hall(n: u32, calm: bool) -> (Harness, usize, u64) {
    let mut h = Harness::new();
    h.add_creatures(n);
    if calm {
        calm_beetles(&mut h);
    }
    let (b, me) = h.player_in_world("one", "Elara");
    h.bots[b].send(
        true,
        queue::WEENIE,
        &auberean_protocol::ClientMessage::SceneRenderingComplete(auberean_protocol::login::Empty),
    );
    h.run(10);
    (h, b, me)
}

/// Beetles that notice nobody.
fn calm_beetles(h: &mut Harness) {
    if let Some(t) = h.server.catalog.templates.get_mut(&TEST_CREATURE) {
        t.props.set(creature_property::PERCEPTION, Prop::Float(0.1));
    }
}

fn beetles(h: &Harness) -> Vec<u64> {
    h.server
        .things
        .by_id
        .values()
        .filter(|t| t.template == TEST_CREATURE && t.alive())
        .map(|t| t.id)
        .collect()
}

fn made(h: &Harness, b: usize, from: usize) -> Vec<u64> {
    h.world_messages(b, from)
        .iter()
        .filter_map(|m| match m {
            ServerMessage::CreateObject(o) if o.weenie.entity_did == TEST_CREATURE => Some(o.id),
            _ => None,
        })
        .collect()
}

fn forgotten(h: &Harness, b: usize, from: usize) -> Vec<u64> {
    h.world_messages(b, from)
        .iter()
        .filter_map(|m| match m {
            ServerMessage::ForgetObject(o) => Some(o.0.id),
            _ => None,
        })
        .collect()
}

fn sorted(mut v: Vec<u64>) -> Vec<u64> {
    v.sort_unstable();
    v
}

/// A reload whose database adds a camp of two: at the world's next step the camp makes them, and
/// the player sees both made.
/// Behaviour: server.operator.reload-starts-new-camps
#[test]
fn a_new_camp_makes_its_creatures_at_the_worlds_next_step() {
    let mut h = Harness::new();
    h.server.catalog = test_catalog();
    calm_beetles(&mut h);
    let (b, _) = h.player_in_world("one", "Elara");
    h.run(10);
    let from = h.bots[b].inbox.len();
    let mut t = tables_of(&h);
    t.spawns.camps.push(test_camp(2));
    let r = reload(&mut h, t);
    assert_eq!(r.applied, [table::CAMPS]);
    assert!(beetles(&h).is_empty(), "none before the step");
    h.run(10);

    let now = sorted(beetles(&h));
    assert_eq!(now.len(), 2);
    assert_eq!(sorted(made(&h, b, from)), now);
    assert_eq!(h.server.camps[0].alive.len(), 2);
}

/// A reload whose database no longer has the camp: its two beetles, at rest, leave the world at
/// once, forgotten by the player who saw them, and none appear again after the camp's period.
/// Behaviour: server.operator.reload-retires-removed-camps
#[test]
fn a_camp_taken_away_takes_its_creatures_at_rest_out_of_the_world_and_none_come_back() {
    let (mut h, b, _) = hall(2, true);
    let were = sorted(beetles(&h));
    assert_eq!(were.len(), 2);
    let from = h.bots[b].inbox.len();
    let mut t = tables_of(&h);
    t.spawns.camps.clear();
    let r = reload(&mut h, t);
    h.run(5);

    assert_eq!(r.applied, [table::CAMPS]);
    assert_eq!(
        r.details,
        [
            "camps: 0 new, 0 changed, 1 taken away; 2 creatures left the world, 0 leave once their \
             fights are over"
        ]
    );
    assert_eq!(sorted(forgotten(&h, b, from)), were);
    assert!(beetles(&h).is_empty());
    let back = yalain_net::LocalTime(h.now.0 + 70.0);
    h.run_to(back);
    assert!(beetles(&h).is_empty(), "none replaced");
    assert!(made(&h, b, from).is_empty());
}

/// A reload that takes away the camp of a beetle fighting the player: the beetle fights on, no
/// longer its camp's; once the player has left the world and the beetle is at rest again it
/// leaves the world, and nothing replaces it.
/// Behaviour: server.operator.reload-lets-fights-finish
#[test]
fn a_creature_fighting_when_its_camp_goes_leaves_the_world_once_its_fight_is_over() {
    let (mut h, _, me) = hall(1, false);
    let beetle = beetles(&h)[0];
    let target = |h: &Harness| {
        h.server
            .things
            .get(beetle)
            .and_then(|t| t.creature.as_ref())
            .and_then(|c| c.target)
    };
    assert!(h.run_until(200, |h| target(h) == Some(me)), "it fights");
    let mut t = tables_of(&h);
    t.spawns.camps.clear();
    let r = reload(&mut h, t);
    assert!(
        r.details[0].ends_with("0 creatures left the world, 1 leave once their fights are over"),
        "{:?}",
        r.details
    );
    h.run(20);
    let c = h
        .server
        .things
        .get(beetle)
        .and_then(|t| t.creature.as_ref())
        .expect("still in the world");
    assert_eq!(c.camp, None, "no longer its camp's");
    assert_eq!(target(&h), Some(me), "fighting on");

    let conn = *h.server.players.keys().next().expect("in");
    yalain_world::world::leave(&mut h.server, &mut h.net, conn);
    assert!(
        h.run_until(400, |h| h.server.things.get(beetle).is_none()),
        "gone once at rest"
    );
    let back = yalain_net::LocalTime(h.now.0 + 70.0);
    h.run_to(back);
    assert!(beetles(&h).is_empty(), "none replaced");
}

/// A reload whose camp keeps three where it kept two keeps the two and makes one more; a reload
/// that moves the camp takes its beetles out of the world and makes as many anew where it now is.
/// Behaviour: server.operator.reload-changed-camps-keep-or-replace-creatures
#[test]
fn a_camp_keeping_more_keeps_its_creatures_and_a_camp_moved_makes_them_anew() {
    let (mut h, b, _) = hall(2, true);
    let two = sorted(beetles(&h));
    let mut t = tables_of(&h);
    t.spawns.camps[0].population = 3;
    reload(&mut h, t);
    h.run(10);
    let three = sorted(beetles(&h));
    assert_eq!(three.len(), 3);
    assert!(two.iter().all(|id| three.contains(id)), "the two kept");

    let from = h.bots[b].inbox.len();
    let mut t = tables_of(&h);
    let camp = &mut t.spawns.camps[0];
    camp.origin = [70.0, 245.0, -44.6];
    camp.spots = vec![(camp.cell, [70.0, 245.0, -44.6])];
    reload(&mut h, t);
    h.run(10);
    let anew = sorted(beetles(&h));
    assert_eq!(anew.len(), 3);
    assert!(anew.iter().all(|id| !three.contains(id)), "all made anew");
    assert_eq!(sorted(forgotten(&h, b, from)), three);
    assert_eq!(sorted(made(&h, b, from)), anew);
    assert!(h
        .server
        .things
        .by_id
        .values()
        .filter(|t| t.template == TEST_CREATURE)
        .all(|t| t.at.is_some_and(|at| at.origin[0] == 70.0)));
}

/// A reload that takes away every camp and every placed object leaves what is not the world
/// database's: the item in the player's pack, an item lying on the ground, a corpse, and a creature
/// no camp keeps (as a player's pet is) all stay, and the player is told nothing of them.
/// Behaviour: server.operator.reload-never-touches-players-things
#[test]
fn a_reload_never_touches_what_a_player_has_or_left_lying() {
    let (mut h, b, me) = hall(2, true);
    let conn = *h.server.players.keys().next().expect("in");
    yalain_world::inventory::give(&mut h.server, &mut h.net, conn, TEST_ITEM, 1);
    h.run(5);
    let carried = h.server.players[&conn]
        .play
        .items
        .iter()
        .find(|i| i.template == TEST_ITEM)
        .expect("carried")
        .id;
    let at = h.server.players[&conn].position;
    let lying = yalain_world::inventory::make_item(&mut h.server, TEST_ITEM, 1, 0).expect("made");
    if let Some(t) = h.server.things.get_mut(lying) {
        t.at = Some(at);
    }
    let template = h.server.catalog.get(TEST_CREATURE).expect("beetle").clone();
    let corpse = h.server.things.new_id();
    let mut c = Thing::of(corpse, &template, Kind::Corpse);
    c.at = Some(at);
    c.corpse = Some(Corpse {
        killer: me,
        originator: 0,
        contents: Vec::new(),
        creature: TEST_CREATURE,
        butchered: false,
        killer_level: 1,
        expires: h.now.0 + 600.0,
        ..Default::default()
    });
    h.server.things.insert(c);
    // One of the camp's beetles is kept by no camp from now on, as a pet or a summoned one is.
    let pet = beetles(&h)[0];
    h.server.camps[0].alive.retain(|id| *id != pet);
    if let Some(c) = h
        .server
        .things
        .get_mut(pet)
        .and_then(|t| t.creature.as_mut())
    {
        c.camp = None;
    }
    h.run(10);

    let from = h.bots[b].inbox.len();
    let mut t = tables_of(&h);
    t.spawns.camps.clear();
    t.content.objects.clear();
    reload(&mut h, t);
    h.run(10);

    for id in [carried, lying, corpse, pet] {
        assert!(h.server.things.get(id).is_some(), "{id:#x} kept");
        assert!(
            !forgotten(&h, b, from).contains(&id),
            "{id:#x} not forgotten"
        );
    }
    assert_eq!(beetles(&h), [pet], "the camp's own beetle gone");
}
