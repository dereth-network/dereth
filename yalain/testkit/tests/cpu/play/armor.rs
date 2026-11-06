//! What a player's armor does to a creature's hit: its natural damage rolled from the share its
//! variance leaves up to all of it, what its skill adds, a critical a fifth more, then armor off a
//! point for a point, never below a draw of up to a quarter of the hit. What armor a player has: what
//! its pieces show and what effects on it give, each once. Fixtures: a coat and a hood of made-up
//! ratings, and effects on the armor stat modeled on an armor ward's.

use auberean_protocol::{queue, ServerMessage};
use yalain_content::{Prop, Props};
use yalain_testkit::{Harness, TEST_CREATURE};
use yalain_world::combat;
use yalain_world::effect_defs::{numeric, EffectDef, Kind as EffectKind, Scale};
use yalain_world::effects::{self, Cast, On, ARMOR_RATING};
use yalain_world::rng::Rng;
use yalain_world::rules::{self, Kind, Natural};

const COAT: u32 = 0x4700_7A01;
const HOOD: u32 = 0x4700_7A02;
const PACKED: u32 = 0x4700_7A03;
const WARD: u32 = 0x6F00_7A01;
const WEAKER_WARD: u32 = 0x6F00_7A02;
const OTHER_CLASS: u32 = 0x6F00_7A03;
const HALVES: u32 = 0x6F00_7A04;
const FROM_THE_COAT: u32 = 0x6F00_7A05;

fn plain(damage: f32, variance: f32) -> Natural {
    Natural {
        damage,
        variance,
        added: 0.0,
        factor: 1.0,
    }
}

/// A point of armor takes a point off a hit, on any kind of attack, and what a skill adds is on the
/// hit before armor comes off.
/// Behaviour: server.combat.armor-takes-a-point
#[test]
fn a_point_of_armor_takes_a_point_of_a_creatures_hit() {
    let mut rng = Rng::new(5);
    for kind in [Kind::Melee, Kind::Missile, Kind::Magic] {
        let hit = |rng: &mut Rng, armor| {
            rules::creature_hit_on_player(rng, kind, plain(100.0, 0.0), 10, armor, false)
        };
        assert_eq!(hit(&mut rng, 0.0), 100);
        assert_eq!(hit(&mut rng, 30.0), 70);
        assert_eq!(hit(&mut rng, 31.0), 69);
    }
    let n = Natural {
        added: 8.0,
        factor: 1.5,
        ..plain(100.0, 0.0)
    };
    assert_eq!(
        rules::creature_hit_on_player(&mut rng, Kind::Melee, n, 10, 12.0, false),
        150,
        "(100 + 8) x 1.5 less 12"
    );
}

/// A hit's roll runs from what its variance leaves up to its natural damage, not around it: the
/// spread falls below natural, and armor moves the whole of it down.
/// Behaviour: server.combat.armor-takes-a-point
#[test]
fn a_creatures_hit_rolls_up_to_its_natural_damage_and_no_higher() {
    let mut rng = Rng::new(9);
    let hits: Vec<i32> = (0..4000)
        .map(|_| {
            rules::creature_hit_on_player(&mut rng, Kind::Melee, plain(100.0, 0.4), 10, 20.0, false)
        })
        .collect();
    let (least, most) = (
        *hits.iter().min().expect("hits"),
        *hits.iter().max().expect("hits"),
    );
    assert_eq!((least, most), (40, 80), "100 x (0.6 to 1) less 20");
    #[allow(clippy::cast_precision_loss)]
    let mean = hits.iter().sum::<i32>() as f32 / hits.len() as f32;
    assert!((58.0..62.0).contains(&mean), "mean {mean}, not 100 less 20");
}

/// A critical adds a fifth to the hit, before armor: at a rating that would halve a doubled hit it
/// comes out far short of a share of the plain one.
/// Behaviour: server.combat.armor-takes-a-point
#[test]
fn a_critical_on_a_player_adds_a_fifth_before_armor() {
    let mut rng = Rng::new(3);
    let hit =
        rules::creature_hit_on_player(&mut rng, Kind::Melee, plain(100.0, 0.0), 10, 60.0, true);
    assert_eq!(hit, 60, "120 less 60");
}

/// No rating stops a hit: armor past a hit leaves a draw of up to a quarter of it, spread across
/// that, never nothing.
/// Behaviour: server.combat.armor-never-stops-a-hit
#[test]
fn no_rating_stops_a_creatures_hit() {
    let mut rng = Rng::new(11);
    for armor in [200.0, 100_000.0] {
        let hits: Vec<i32> = (0..4000)
            .map(|_| {
                rules::creature_hit_on_player(
                    &mut rng,
                    Kind::Melee,
                    plain(100.0, 0.0),
                    40,
                    armor,
                    false,
                )
            })
            .collect();
        assert!(hits.iter().all(|d| (1..=25).contains(d)), "{armor}");
        #[allow(clippy::cast_precision_loss)]
        let mean = hits.iter().sum::<i32>() as f32 / hits.len() as f32;
        assert!((12.0..14.0).contains(&mean), "at armor {armor}: {mean}");
        assert!(hits.iter().any(|d| *d <= 2) && hits.iter().any(|d| *d >= 24));
    }
    // The smallest creature's hit still does something.
    assert!(
        rules::creature_hit_on_player(&mut rng, Kind::Melee, plain(1.0, 0.0), 40, 500.0, false)
            >= 1
    );
}

/// What armor leaves is the greater of what's left and the draw, not the draw alone: a rating just
/// short of a hit's three quarters leaves what it leaves or more, never less.
/// Behaviour: server.combat.armor-never-stops-a-hit
#[test]
fn armor_that_leaves_a_little_leaves_at_least_that() {
    let mut rng = Rng::new(21);
    let hits: Vec<i32> = (0..2000)
        .map(|_| {
            rules::creature_hit_on_player(&mut rng, Kind::Melee, plain(100.0, 0.0), 40, 80.0, false)
        })
        .collect();
    assert!(
        hits.iter().all(|d| (20..=25).contains(d)),
        "20 left, a draw to 25"
    );
    assert!(hits.iter().filter(|d| **d == 20).count() > hits.len() / 2);
}

/// The floor goes with the hit, not with the creature's natural: a hit rolled low draws from less,
/// and a critical's from more.
/// Behaviour: server.combat.armor-never-stops-a-hit
#[test]
fn the_floor_is_a_share_of_the_hit_as_rolled() {
    let mean = |variance: f32, critical: bool| {
        let mut rng = Rng::new(23);
        let total: i32 = (0..8000)
            .map(|_| {
                rules::creature_hit_on_player(
                    &mut rng,
                    Kind::Melee,
                    plain(1000.0, variance),
                    40,
                    100_000.0,
                    critical,
                )
            })
            .sum();
        #[allow(clippy::cast_precision_loss)]
        let m = total as f32 / 8000.0;
        m
    };
    let (still, wide, critical) = (mean(0.0, false), mean(0.6, false), mean(0.0, true));
    assert!((125.0..135.0).contains(&still), "{still}");
    assert!((85.0..96.0).contains(&wide), "0.13 of a mean 700: {wide}");
    assert!(
        (150.0..162.0).contains(&critical),
        "0.13 of 1200: {critical}"
    );
}

/// Armor effects have taken below nothing adds to a hit, a point for a point.
/// Behaviour: server.combat.armor-takes-a-point
#[test]
fn armor_taken_below_nothing_adds_to_a_creatures_hit() {
    let mut rng = Rng::new(13);
    assert_eq!(
        rules::creature_hit_on_player(&mut rng, Kind::Melee, plain(100.0, 0.0), 30, -43.0, false),
        143
    );
}

/// At levels 1 and 2 a hit keeps its wide roll in full, whatever the player has on.
/// Behaviour: server.combat.creature-damage-through-armor
#[test]
fn a_player_of_level_one_or_two_takes_the_wide_roll_whatever_it_wears() {
    for level in [1, 2] {
        let mut rng = Rng::new(17);
        let hits: Vec<i32> = (0..400)
            .map(|_| {
                rules::creature_hit_on_player(
                    &mut rng,
                    Kind::Melee,
                    plain(100.0, 0.5),
                    level,
                    1000.0,
                    false,
                )
            })
            .collect();
        assert!(hits.iter().all(|d| (50..=150).contains(d)));
        assert!(
            hits.iter().any(|d| *d > 120),
            "above natural, as the wide roll goes"
        );
    }
    let mut rng = Rng::new(17);
    assert!(
        rules::creature_hit_on_player(&mut rng, Kind::Melee, plain(100.0, 0.5), 3, 1000.0, false)
            <= 20
    );
}

/// A piece of the rating given, for where `place` says.
fn piece(h: &mut Harness, template: u32, place: u32, rating: i64) {
    let mut p = Props::default();
    p.set("Item_ArmorLevel", Prop::Int(rating))
        .set("Item_ValidInventoryLocations", Prop::Word(place))
        .set("Item_PreferredInventoryLocation", Prop::Word(place));
    h.server
        .catalog
        .insert(yalain_testkit::test_template(template, 0x180, p));
}

/// An effect on the armor stat: adding `value`, or multiplying by it.
fn armor_effect(state: u32, class: u32, value: f32, multiplies: bool) -> EffectDef {
    EffectDef {
        state,
        kind: EffectKind::Numeric {
            flags: if multiplies {
                numeric::MULTIPLY
            } else {
                numeric::ADD
            },
            stats: vec![ARMOR_RATING],
            magnitude: Scale(vec![(0.0, value)]),
        },
        duration: Scale(vec![(0.0, 100_000.0)]),
        priority: Scale(vec![(0.0, value.abs())]),
        fx: 0,
        external_flags: 0,
        internal_flags: 0x1100_0003,
        enum_value: 0,
        eq_class: class,
        name: None,
    }
}

fn on(h: &mut Harness, state: u32) {
    let world = *h.server.players.keys().next().expect("in the world");
    let clock = h.clock();
    assert!(effects::apply_on(
        &mut h.server,
        &mut h.net,
        On::Player(world),
        state,
        Cast {
            spellcraft: 10.0,
            ..Cast::default()
        },
        clock,
    ));
}

/// A player's armor is what the pieces it wears show, and what effects on it give, the top of each
/// class: a piece in the pack gives nothing, a piece's own worn effect counts on the wearer and
/// isn't in the piece's rating, and taking a piece off takes its rating.
/// Behaviour: server.combat.armor-from-each-source-once
#[test]
fn a_players_armor_counts_each_piece_and_each_effect_once() {
    let mut h = Harness::new();
    h.server.catalog = yalain_testkit::test_catalog();
    let (b, me) = h.player_in_world("one", "Elara");
    h.run(10);
    let world = *h.server.players.keys().next().expect("in the world");
    piece(&mut h, COAT, 2, 30);
    piece(&mut h, HOOD, 1, 12);
    for d in [
        armor_effect(WARD, 21, 6.0, false),
        armor_effect(WEAKER_WARD, 21, 2.0, false),
        armor_effect(OTHER_CLASS, 115, 10.0, false),
        armor_effect(HALVES, 29, 0.5, true),
        EffectDef {
            internal_flags: 0x0100_0783,
            ..armor_effect(FROM_THE_COAT, 5002, 5.0, false)
        },
    ] {
        h.server.effect_table.effects.insert(d.state, d);
    }
    assert!(combat::armor_on(&h.server, world).abs() < f32::EPSILON);

    // In the pack, nothing.
    piece(&mut h, PACKED, 2, 500);
    let packed = yalain_world::inventory::make_item(&mut h.server, PACKED, 1, 0).expect("made");
    h.server
        .players
        .get_mut(&world)
        .expect("it")
        .play
        .items
        .push(yalain_world::state::Held {
            id: packed,
            template: PACKED,
            slot: 40,
            location: 0,
            quantity: 1,
        });
    assert!(combat::armor_on(&h.server, world).abs() < f32::EPSILON);

    let coat = h.arm(b, me, COAT);
    assert!((combat::armor_on(&h.server, world) - 30.0).abs() < f32::EPSILON);
    let hood = h.arm(b, me, HOOD);
    assert!((combat::armor_on(&h.server, world) - 42.0).abs() < f32::EPSILON);
    // Its own numbers before its template's.
    h.server.things.get_mut(hood).expect("it").own.made = Some(yalain_world::made::Made {
        armor: Some(20),
        delay: Some(5),
        ..Default::default()
    });
    assert!((combat::armor_on(&h.server, world) - 50.0).abs() < f32::EPSILON);

    on(&mut h, WARD);
    assert!((combat::armor_on(&h.server, world) - 56.0).abs() < f32::EPSILON);
    // The same class: the top one only.
    on(&mut h, WEAKER_WARD);
    assert!((combat::armor_on(&h.server, world) - 56.0).abs() < f32::EPSILON);
    on(&mut h, OTHER_CLASS);
    assert!((combat::armor_on(&h.server, world) - 66.0).abs() < f32::EPSILON);

    // A worn effect on the armor stat: on the wearer, and the piece still shows its own rating.
    let clock = h.clock();
    let put = effects::put_on_worn(
        &mut h.server,
        &mut h.net,
        world,
        coat,
        &[(FROM_THE_COAT, 1.0)],
        clock,
    );
    assert_eq!(put.len(), 1, "it comes on");
    assert!((combat::armor_on(&h.server, world) - 71.0).abs() < f32::EPSILON);
    let t = h.server.things.get(coat).expect("the coat").clone();
    let shown = yalain_world::requests::item_view(&h.server, &t)
        .armor
        .map(|a| a.0);
    assert_eq!(
        shown,
        Some(30),
        "the coat's rating doesn't hold its worn effect"
    );

    // An effect that multiplies the stat multiplies all of it.
    on(&mut h, HALVES);
    assert!((combat::armor_on(&h.server, world) - 35.5).abs() < f32::EPSILON);
}

/// The strike of a creature in reach reads what the player wears.
/// Behaviour: server.combat.armor-takes-a-point
#[test]
fn a_creatures_strike_comes_off_the_armor_the_player_wears() {
    fn strikes(rating: i64) -> Vec<i32> {
        let mut h = Harness::new();
        h.add_creatures(1);
        let (b, me) = h.player_in_world("one", "Elara");
        h.bots[b].send(
            true,
            queue::WEENIE,
            &auberean_protocol::ClientMessage::SceneRenderingComplete(
                auberean_protocol::login::Empty,
            ),
        );
        h.run(5);
        if rating != 0 {
            piece(&mut h, COAT, 2, rating);
            h.arm(b, me, COAT);
        }
        for p in h.server.players.values_mut() {
            yalain_world::quals::set_int(&mut p.qualities, yalain_world::examine::LEVEL, 20);
            yalain_world::quals::set_int(&mut p.qualities, 263, 1_000_000);
            yalain_world::quals::set_int(&mut p.qualities, 258, 1_000_000);
        }
        let foe = h
            .world_messages(b, 0)
            .iter()
            .find_map(|m| match m {
                ServerMessage::CreateObject(o) if o.weenie.entity_did == TEST_CREATURE => {
                    Some(o.id)
                }
                _ => None,
            })
            .expect("the creature");
        if let Some(c) = h
            .server
            .things
            .get_mut(foe)
            .and_then(|t| t.creature.as_mut())
        {
            c.stats.natural_damage = 100.0;
            c.stats.natural_variance = 0.0;
        }
        let from = h.bots[b].inbox.len();
        h.run(4000);
        h.world_messages(b, from)
            .iter()
            .filter_map(|m| match m {
                ServerMessage::DoStory(e) if e.action.attacker == foe => Some(e),
                _ => None,
            })
            .flat_map(|e| e.action.hooks.iter())
            .filter(|k| k.target == me && k.numbers[3] < 0)
            .map(|k| -k.numbers[3])
            .collect()
    }
    let bare = strikes(0);
    assert!(!bare.is_empty(), "the creature lands a hit in the run");
    // Plain hits of 100 or more (what its skill adds), a critical a quarter more.
    assert!(bare.iter().all(|d| *d >= 100), "{bare:?}");
    let worn = strikes(60);
    assert!(!worn.is_empty());
    assert!(worn.iter().any(|d| (40..=70).contains(d)), "{worn:?}");
    assert!(
        worn.iter()
            .all(|d| *d < bare.iter().copied().max().expect("hits")),
        "{worn:?}"
    );
}
