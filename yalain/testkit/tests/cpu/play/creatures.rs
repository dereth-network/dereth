//! Creatures: camps keep them alive; they are made for the players whose view holds them.

use auberean_protocol::ServerMessage;
use yalain_testkit::{Harness, TEST_CREATURE};

fn creatures_made(
    h: &Harness,
    b: usize,
    from: usize,
) -> Vec<auberean_protocol::physics::CreateObject> {
    h.world_messages(b, from)
        .into_iter()
        .filter_map(|m| match m {
            ServerMessage::CreateObject(o) if o.weenie.entity_did == TEST_CREATURE => Some(o),
            _ => None,
        })
        .collect()
}

/// A camp keeps its population alive from the start, at its spots; a player entering beside it is
/// told of each of its creatures (already there: no entry effect).
/// Behaviour: server.creatures.camps-keep-their-population
#[test]
fn a_camps_creatures_are_there_from_the_start_and_made_for_a_player_in_view() {
    let mut h = Harness::new();
    h.add_creatures(2);
    let (b, _) = h.player_in_world("one", "Elara");
    let made = creatures_made(&h, b, 0);
    assert_eq!(made.len(), 2, "both of the camp's creatures");
    for o in &made {
        assert_eq!(o.physics.position.cell, 0x0299_0109);
        assert!(
            o.physics.fx.is_empty(),
            "no entry effect for a creature already there"
        );
    }
}

/// A summoned creature's template of the test world: a lasher, with a pet's AI.
const TEST_SUMMON: u32 = 0x4700_0846;

/// Adds a pet-like template to the test world, a level 5 creature with a pet's AI superclass.
fn add_summon(h: &mut Harness) {
    use yalain_content::Prop;
    let mut pet = yalain_content::Props::default();
    pet.set("GameplayStatistics_Level", Prop::Int(5))
        .set("GameplayStatistics_MaxHealth", Prop::Int(50))
        .set("AI_AISuperClass", Prop::Word(yalain_world::spawn::PET_AI));
    h.server
        .catalog
        .insert(yalain_testkit::test_template(TEST_SUMMON, 0x1169, pet));
}

/// A camp never makes a summoned creature: one that lists a pet beside its wild creatures makes
/// only the wild ones, and one of pets alone makes nothing.
/// Behaviour: server.creatures.camps-never-make-summons
#[test]
fn a_camp_makes_its_wild_creatures_and_never_a_summoned_one() {
    let mut h = Harness::new();
    h.add_creatures(4);
    add_summon(&mut h);
    h.server.spawns.camps[0].templates = vec![(TEST_CREATURE, 1), (TEST_SUMMON, 50)];
    let mut pets = yalain_testkit::test_camp(3);
    pets.id = "pets".into();
    pets.templates = vec![(TEST_SUMMON, 1)];
    h.server.spawns.camps.push(pets);
    h.server
        .camps
        .push(yalain_world::spawn::CampState::default());
    let (b, _) = h.player_in_world("one", "Elara");
    let made: Vec<u32> = h
        .world_messages(b, 0)
        .into_iter()
        .filter_map(|m| match m {
            ServerMessage::CreateObject(o)
                if [TEST_CREATURE, TEST_SUMMON].contains(&o.weenie.entity_did) =>
            {
                Some(o.weenie.entity_did)
            }
            _ => None,
        })
        .collect();
    assert_eq!(made, vec![TEST_CREATURE; 4], "four beetles, no pet");
    assert!(
        h.server.camps[1].alive.is_empty(),
        "the camp of pets is empty"
    );
}

fn creature_ids(h: &Harness, b: usize) -> Vec<u64> {
    creatures_made(h, b, 0).iter().map(|o| o.id).collect()
}

fn scene_drawn(h: &mut Harness, b: usize) {
    h.bots[b].send(
        true,
        auberean_protocol::queue::WEENIE,
        &auberean_protocol::ClientMessage::SceneRenderingComplete(auberean_protocol::login::Empty),
    );
    h.run(5);
}

/// A camp as many as its spots fills every one, whatever the dice say, and a creature's replacement
/// takes the spot that is free.
/// Behaviour: server.creatures.spots-hold-one
#[test]
fn a_camp_as_many_as_its_spots_fills_every_one() {
    #[allow(clippy::cast_possible_truncation)]
    let place = |at: yalain_store::StoredPosition| (at.origin[0] * 10.0).round() as i32;
    for seed in 1..40 {
        let mut h = Harness::new();
        h.server.rng = yalain_world::rng::Rng::new(seed);
        h.add_creatures(2);
        let camp = &mut h.server.spawns.camps[0];
        camp.population = 3;
        camp.spots.push((0x0299_0109, [70.0, 248.0, -44.6]));
        h.player_in_world("one", "Elara");
        let mut spots: Vec<i32> = h
            .server
            .things
            .by_id
            .values()
            .filter(|t| t.template == TEST_CREATURE)
            .filter_map(|t| t.creature.as_ref().map(|c| place(c.home)))
            .collect();
        spots.sort_unstable();
        assert_eq!(spots, vec![650, 660, 700], "seed {seed}");
    }
}

/// However many players are in the world, a creature's look reads the state only of those near it:
/// the world's every creature looks once a second, so reading all of them costs a pass of the world
/// about a millisecond a player.
/// Behaviour: server.creatures.look-reads-near-players-only
#[test]
fn a_creature_looking_reads_the_state_of_the_players_near_it_only() {
    let mut h = Harness::new();
    let (_, near) = h.player_in_world("one", "Elara");
    h.player_in_world("two", "Bran");
    h.player_in_world("three", "Coll");
    let here = yalain_store::StoredPosition {
        cell: 0x6FA9_0025,
        origin: [90.0, 90.0, 80.0],
        rotation: [1.0, 0.0, 0.0, 0.0],
    };
    for p in h.server.players.values_mut() {
        p.position = if p.character == near {
            here
        } else {
            // One far in this block, one blocks away.
            let far = p.character % 2 == 0;
            yalain_store::StoredPosition {
                cell: if far { 0x6FA9_0001 } else { 0x20A9_0025 },
                origin: [if far { 5.0 } else { 90.0 }, 5.0, 80.0],
                ..here
            }
        };
    }
    let mut read = Vec::new();
    let noticed = yalain_world::ai::notices_among(&h.server, &here, 20.0, |p| {
        read.push(p.character);
        true
    });
    assert_eq!(noticed, Some(near));
    assert_eq!(read, vec![near], "the far players' states stay unread");
}

/// A creature that attacks first notices a player within its perception and comes at it: its
/// places reach the player's client, each nearer than the last, until it is within reach.
/// Behaviour: server.creatures.notice-and-close-in
#[test]
fn a_creature_notices_a_player_within_its_perception_and_comes_at_it() {
    let mut h = Harness::new();
    h.add_creatures(1);
    let (b, _) = h.player_in_world("one", "Elara");
    scene_drawn(&mut h, b);
    let id = creature_ids(&h, b)[0];
    let from = h.bots[b].inbox.len();
    h.run(80);
    let me = [59.06f32, 240.2];
    let distances: Vec<f32> = h
        .world_messages(b, from)
        .iter()
        .filter_map(|m| match m {
            ServerMessage::Position(p) | ServerMessage::PositionCell(p) if p.sender.id == id => {
                Some(((p.origin.x - me[0]).powi(2) + (p.origin.y - me[1]).powi(2)).sqrt())
            }
            _ => None,
        })
        .collect();
    assert!(distances.len() >= 2, "it moves: {distances:?}");
    assert!(
        distances.windows(2).all(|w| w[1] <= w[0] + 1e-3),
        "each place nearer: {distances:?}"
    );
    assert!(
        *distances.last().unwrap() <= 2.5,
        "within reach: {distances:?}"
    );
}

/// A camp of creatures of one friend table that notice nothing beyond half a meter, `spots` beside where a
/// character starts.
fn kin_camp(h: &mut Harness, template: u32, table: u32, spots: &[[f32; 3]]) {
    use yalain_content::Prop;
    let mut p = yalain_content::Props::default();
    p.set("GameplayStatistics_Level", Prop::Int(2))
        .set("GameplayStatistics_MaxHealth", Prop::Int(460))
        .set("AICombat_NaturalDamage", Prop::Float(1.0))
        .set("AICombat_NaturalVariance", Prop::Float(0.5))
        .set("AI_PerceptionRadius", Prop::Float(0.5))
        .set("AI_LoveTable", Prop::Word(table));
    h.server
        .catalog
        .insert(yalain_testkit::test_template(template, 0x1169, p));
    h.server.spawns.camps.push(yalain_content::Camp {
        id: format!("kin {template:08X}"),
        cell: 0x0299_0109,
        origin: spots[0],
        radius: 0.0,
        population: u32::try_from(spots.len()).unwrap_or(0),
        regen: 600.0,
        templates: vec![(template, 1)],
        spots: spots.iter().map(|s| (0x0299_0109, *s)).collect(),
        placed: "authored for the tests".into(),
        confidence: "authored".into(),
        moves: yalain_content::CampMoves::default(),
    });
    h.server
        .camps
        .push(yalain_world::spawn::CampState::default());
}

/// When a creature goes after a player, resting creatures of its own friend table nearby come
/// too, though the player is beyond their own perception; creatures of a table that does not come
/// to its kin's fights stay where they are.
/// Behaviour: server.creatures.kin-join-a-fight
#[test]
fn resting_kin_join_a_fight_and_strangers_do_not() {
    const DRUDGES: u32 = 0x4700_2226;
    const LONERS: u32 = 0x4700_2227;
    let mut h = Harness::new();
    h.server.catalog = yalain_testkit::test_catalog();
    let row = |x: f32| -> Vec<[f32; 3]> { (0..8).map(|i| [x, 236.0 + i as f32, -44.6]).collect() };
    kin_camp(&mut h, DRUDGES, 0x06, &row(61.0));
    kin_camp(&mut h, LONERS, 0x42, &row(66.0));
    let (b, me) = h.player_in_world("one", "Elara");
    scene_drawn(&mut h, b);
    h.arm(b, me, 0x4700_051C);
    // Each made, nearest the player first.
    let made = |h: &Harness, template: u32| -> Vec<u64> {
        let mut v: Vec<(f32, u64)> = h
            .world_messages(b, 0)
            .iter()
            .filter_map(|m| match m {
                ServerMessage::CreateObject(o) if o.weenie.entity_did == template => {
                    Some(((o.physics.position.frame.origin.y - 240.2).abs(), o.id))
                }
                _ => None,
            })
            .collect();
        v.sort_by(|a, b| a.0.total_cmp(&b.0));
        v.into_iter().map(|(_, id)| id).collect()
    };
    let drudges = made(&h, DRUDGES);
    let loners = made(&h, LONERS);
    assert_eq!((drudges.len(), loners.len()), (8, 8));
    let from = h.bots[b].inbox.len();
    h.bots[b].send(
        true,
        auberean_protocol::queue::WEENIE,
        &yalain_testkit::play::server_call(
            0x8204_0008,
            &[auberean_script::args::Arg::LongInt(drudges[0])],
        ),
    );
    h.run(80);
    let fighting = |ids: &[u64]| {
        ids.iter()
            .filter(|id| {
                h.world_messages(b, from).iter().any(|m| {
                    matches!(m, ServerMessage::DoMode(e) if e.sender.id == **id && e.action.0 == 0x4100_0012)
                })
            })
            .count()
    };
    assert!(fighting(&drudges[1..]) >= 1, "kin come");
    assert_eq!(fighting(&loners), 0, "strangers stay");
}

/// A creature gives up once the player has been beyond its perception for a second, wherever
/// beyond it the player is: a player two meters past a 20 m radius is still chased half a second
/// on, and given up on a second later.
/// Behaviour: server.creatures.give-up-a-second-beyond-perception
#[test]
fn a_creature_gives_up_a_second_after_the_player_leaves_its_perception() {
    let mut h = Harness::new();
    h.add_creatures(1);
    let (b, _) = h.player_in_world("one", "Elara");
    scene_drawn(&mut h, b);
    let id = creature_ids(&h, b)[0];
    let chasing = h.run_until(200, |h| {
        h.server
            .things
            .get(id)
            .and_then(|t| t.creature.as_ref())
            .is_some_and(|c| c.mode == yalain_world::things::Mode::Chasing)
    });
    assert!(chasing, "it goes after the player");
    // The player keeps 22 m from it, beyond its 20 m (as a player outrunning it does).
    let from = h.bots[b].inbox.len();
    let peace = |h: &Harness| {
        h.world_messages(b, from).iter().any(
            |m| matches!(m, ServerMessage::DoMode(e) if e.sender.id == id && e.action.0 == 0x4000_0001),
        )
    };
    let mut gave_up_after = None;
    for step in 1..=60 {
        let at = h.server.things.get(id).and_then(|t| t.at).expect("placed");
        for p in h.server.players.values_mut() {
            p.position.origin = [at.origin[0] - 22.0, at.origin[1], at.origin[2]];
            p.position.cell = at.cell;
        }
        h.step();
        if gave_up_after.is_none() && peace(&h) {
            gave_up_after = Some(f64::from(step) * yalain_testkit::STEP);
        }
    }
    let t = gave_up_after.expect("it gives up");
    assert!(
        (0.9..=1.6).contains(&t),
        "about a second beyond its perception: {t} s"
    );
}
