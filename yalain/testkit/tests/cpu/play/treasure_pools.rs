//! Treasure drawn from pools: a creature's profile that draws from pools leaves the templates its
//! level's windows hold, a killer's race takes its race's pool at that pool's share, and a rare is
//! rolled once a corpse. A creature's own trophies come beside the count, from pools or a
//! profile's own items alike.

use auberean_protocol::{queue, ServerMessage};
use auberean_script::args::Arg;
use yalain_content::pools::{Draws, Entry, Pool, Pools, Target, OPEN_ABOVE};
use yalain_testkit::play::server_call;
use yalain_testkit::{Harness, TEST_CREATURE, TEST_ITEM, TEST_PROFILE_NAME};
use yalain_world::rng::Rng;

const START_ATTACK: u32 = 0x8204_0008;
const CORPSE: u32 = 0x4700_0539;
const STARTER_SWORD: u32 = 0x4700_051C;
/// Two more item templates of the test world's.
const HIGH_ITEM: u32 = 0x4700_2109;
const LISTED_ITEM: u32 = 0x4700_210A;

fn entry(to: Target, weight: f32, least: i32, most: i32) -> Entry {
    Entry {
        to,
        weight,
        least,
        most,
    }
}

fn pool(name: &str, race: u32, own: f32, entries: Vec<Entry>) -> (String, Pool) {
    (
        name.to_string(),
        Pool {
            name: name.to_string(),
            race,
            own,
            entries,
        },
    )
}

/// The test world with the beetle's profile drawing from pools: one pool whose low template is
/// drawn up to level 9 and whose high one, far heavier, from 10; the profile's own items list a
/// third, which a draw from pools passes by.
fn pooled(rare: Option<(String, f32)>) -> Harness {
    let mut h = Harness::new();
    h.add_creatures(1);
    for t in [HIGH_ITEM, LISTED_ITEM] {
        h.server.catalog.insert(yalain_testkit::test_template(
            t,
            0x200,
            yalain_content::Props::default(),
        ));
    }
    h.server.loot.profiles[0].items = vec![(LISTED_ITEM, 1)];
    h.server.loot.pools = Pools {
        pools: [pool(
            "GP_Test",
            0,
            0.0,
            vec![
                entry(Target::Template(TEST_ITEM), 1.0, 0, 9),
                entry(Target::Template(HIGH_ITEM), 1000.0, 10, OPEN_ABOVE),
            ],
        )]
        .into_iter()
        .collect(),
        profiles: [(
            TEST_PROFILE_NAME.to_string(),
            Draws {
                draws: vec![entry(Target::Pool("GP_Test".into()), 1.0, 0, OPEN_ABOVE)],
                rare,
            },
        )]
        .into_iter()
        .collect(),
    };
    h
}

fn scene_drawn(h: &mut Harness, b: usize) {
    h.bots[b].send(
        true,
        queue::WEENIE,
        &auberean_protocol::ClientMessage::SceneRenderingComplete(auberean_protocol::login::Empty),
    );
    h.run(5);
}

/// The beetle (level 2) killed by a player; the templates its corpse holds, with their values.
fn killed(h: Harness) -> Vec<(u32, u32)> {
    killed_with(h, |_| {})
}

/// The beetle killed, its stats first changed as given.
fn killed_with(
    mut h: Harness,
    change: impl FnOnce(&mut yalain_content::CreatureStats),
) -> Vec<(u32, u32)> {
    let (b, me) = h.player_in_world("one", "Elara");
    scene_drawn(&mut h, b);
    h.arm(b, me, STARTER_SWORD);
    let beetle = h
        .world_messages(b, 0)
        .iter()
        .find_map(|m| match m {
            ServerMessage::CreateObject(o) if o.weenie.entity_did == TEST_CREATURE => Some(o.id),
            _ => None,
        })
        .expect("the camp's creature is made");
    if let Some(c) = h
        .server
        .things
        .get_mut(beetle)
        .and_then(|t| t.creature.as_mut())
    {
        c.stats.natural_damage = 1.0;
        change(&mut c.stats);
    }
    h.bots[b].send(
        true,
        queue::WEENIE,
        &server_call(START_ATTACK, &[Arg::LongInt(beetle)]),
    );
    assert!(h.run_until(4000, |h| h.world_messages(b, 0).iter().any(
        |m| matches!(m, ServerMessage::CreateObject(o) if o.weenie.entity_did == CORPSE)
    )));
    let corpse = h
        .server
        .things
        .by_id
        .values()
        .find(|t| t.corpse.is_some())
        .and_then(|t| t.corpse.clone())
        .expect("a corpse");
    corpse
        .contents
        .iter()
        .filter_map(|i| h.server.things.get(*i).map(|t| (t.template, t.value)))
        .collect()
}

/// A creature whose loot profile draws from pools leaves what its level's windows hold: the level 2
/// beetle's one item is the pool's low template, never the far heavier one drawn from level 10 nor
/// the profile's own listed item, worth what treasure of its level is (2017: item kinds climb by
/// the dropper's level, 95.7% of 5,443 recorded items inside the windows fit to them).
/// Behaviour: server.loot.pools-by-level
#[test]
fn a_profile_drawing_from_pools_leaves_the_templates_its_level_holds() {
    let held = killed(pooled(None));
    assert_eq!(held.len(), 1, "the profile's one item: {held:?}");
    let (template, value) = held[0];
    assert_eq!(template, TEST_ITEM, "the low template");
    assert!((1..=4).contains(&value), "a level 2 creature's: {value}");
}

/// A profile's rare is rolled once a corpse: at a chance of 1 the corpse holds the profile's item
/// and one rare beside it, never two.
/// Behaviour: server.loot.pools-rare-once
#[test]
fn a_profiles_rare_is_rolled_once_for_a_corpse() {
    let mut h = pooled(Some(("GP_Rare".into(), 1.0)));
    h.server.loot.pools.pools.extend([pool(
        "GP_Rare",
        0,
        0.0,
        vec![entry(Target::Template(LISTED_ITEM), 1.0, 0, OPEN_ABOVE)],
    )]);
    let held = killed(h);
    let rares = held.iter().filter(|(t, _)| *t == LISTED_ITEM).count();
    assert_eq!(rares, 1, "{held:?}");
    assert_eq!(held.len(), 2, "the item and the rare: {held:?}");
}

/// A pool for a race takes its share of its parent's draws for a killer of that race outright, the
/// rest drawn by weight: a human killer's draws from two even pools come to its own about 0.6 +
/// 0.4 x 0.5 of the time, anyone else's about half (2017: weapons of the killer's race 624 of 893
/// for human killers, 884 of 1,150 for Tumerok killers, 167 of 333 for Lugian ones; fit: the
/// race's own share 0.62, 0.55, 0.41).
/// Behaviour: server.loot.pools-own-race
#[test]
fn a_killers_race_takes_its_races_pool_at_that_pools_share() {
    const HUMAN: u32 = 1;
    const A: u32 = 0x4700_0001;
    const B: u32 = 0x4700_0002;
    let pools = Pools {
        pools: [
            pool(
                "GP_W",
                0,
                0.0,
                vec![
                    entry(Target::Pool("GP_WHuman".into()), 1.0, 0, OPEN_ABOVE),
                    entry(Target::Pool("GP_WLugian".into()), 1.0, 0, OPEN_ABOVE),
                ],
            ),
            pool(
                "GP_WHuman",
                HUMAN,
                0.6,
                vec![entry(Target::Template(A), 1.0, 0, OPEN_ABOVE)],
            ),
            pool(
                "GP_WLugian",
                2,
                0.0,
                vec![entry(Target::Template(B), 1.0, 0, OPEN_ABOVE)],
            ),
        ]
        .into_iter()
        .collect(),
        profiles: Default::default(),
    };
    let share = |race: u32| {
        let mut rng = Rng::new(716);
        let n = 4000;
        let own = (0..n)
            .filter(|_| pools.draw("GP_W", 30, race, &mut rng) == Some(A))
            .count();
        #[allow(clippy::cast_precision_loss)]
        let s = own as f64 / f64::from(n);
        s
    };
    let human = share(HUMAN);
    assert!((0.77..=0.83).contains(&human), "a human killer's: {human}");
    let anyone = share(0);
    assert!((0.47..=0.53).contains(&anyone), "anyone else's: {anyone}");
}

/// A draw gives nothing where no entry's window holds the item level, and a pool that leads back to
/// itself gives nothing rather than drawing on.
/// Behaviour: server.loot.pools-by-level
#[test]
fn a_draw_no_window_holds_and_a_pool_leading_to_itself_give_nothing() {
    let pools = Pools {
        pools: [
            pool(
                "GP_Low",
                0,
                0.0,
                vec![entry(Target::Template(TEST_ITEM), 1.0, 0, 9)],
            ),
            pool(
                "GP_Loop",
                0,
                0.0,
                vec![entry(Target::Pool("GP_Loop".into()), 1.0, 0, OPEN_ABOVE)],
            ),
        ]
        .into_iter()
        .collect(),
        profiles: Default::default(),
    };
    let mut rng = Rng::new(717);
    assert_eq!(pools.draw("GP_Low", 5, 0, &mut rng), Some(TEST_ITEM));
    assert_eq!(pools.draw("GP_Low", 10, 0, &mut rng), None);
    assert_eq!(pools.draw("GP_Loop", 5, 0, &mut rng), None);
    assert_eq!(pools.draw("GP_Missing", 5, 0, &mut rng), None);
}

/// A creature's trophies are left beside the items its profile's count gives, and its template's
/// bounds hold those items, not the trophies: a beetle with a sure trophy, a profile of one item
/// and bounds of 1 leaves the trophy once and the one item, as it does when its profile draws from
/// pools (2017: 1,672 of 1,702 corpses whose templates set bounds held their other items within
/// them, against 1,606 counting the trophies too).
/// Behaviour: server.loot.trophies-beside-the-count
#[test]
fn a_creatures_trophies_come_beside_the_items_its_count_and_bounds_give() {
    let trophy = |c: &mut yalain_content::CreatureStats| {
        c.trophies = vec![(HIGH_ITEM, 1.0)];
        c.loot_min = Some(1);
        c.loot_max = Some(1);
    };
    for pools in [false, true] {
        let mut h = pooled(None);
        if !pools {
            h.server.loot.pools = Pools::default();
        }
        let mut held: Vec<u32> = killed_with(h, trophy).iter().map(|(t, _)| *t).collect();
        held.sort_unstable();
        let item = if pools { TEST_ITEM } else { LISTED_ITEM };
        let mut want = vec![HIGH_ITEM, item];
        want.sort_unstable();
        assert_eq!(held, want, "drawing from pools {pools}");
    }
}
