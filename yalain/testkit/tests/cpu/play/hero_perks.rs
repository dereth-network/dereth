//! Heroes' combat perks: accuracy by the time of day and against a creature's category, evasion
//! indoors or outdoors and against a category, the chance of a critical, heals received, the damage
//! creatures notice. Fixture: a player beside the test camp's beetle, each perk a made-up skill of
//! its kind; the pets' perks are with the pets.

use auberean_protocol::{queue, ServerMessage};
use auberean_script::args::Arg;
use yalain_content::Prop;
use yalain_net::ConnId;
use yalain_testkit::play::server_call;
use yalain_testkit::{Harness, TEST_CREATURE};
use yalain_world::effect_defs::{vital, EffectDef, Kind, Scale};
use yalain_world::effects::{self, Cast, On};
use yalain_world::player::quality;
use yalain_world::quals;
use yalain_world::ratings::{self, Rater};
use yalain_world::skills::SkillDef;
use yalain_world::state::SkillState;

const SWORD: u32 = 0x4700_051C;
const DEADEYE: u32 = 207;
const HEAL: u32 = 0x6F00_7B01;

/// A player in the world beside the camp's beetle, which never fights back: the harness, the
/// client, the player, its connection and the beetle.
fn beside_a_beetle() -> (Harness, usize, u64, ConnId, u64) {
    let mut h = Harness::new();
    h.add_creatures(1);
    let (b, me) = h.player_in_world("one", "Elara");
    h.bots[b].send(
        true,
        queue::WEENIE,
        &auberean_protocol::ClientMessage::SceneRenderingComplete(auberean_protocol::login::Empty),
    );
    h.run(5);
    let world = *h
        .server
        .players
        .iter()
        .find(|(_, p)| p.character == me)
        .expect("in the world")
        .0;
    let beetle = h
        .server
        .things
        .by_id
        .values()
        .find(|t| t.template == TEST_CREATURE && t.alive())
        .map(|t| t.id)
        .expect("a beetle");
    if let Some(c) = h
        .server
        .things
        .get_mut(beetle)
        .and_then(|t| t.creature.as_mut())
    {
        c.stats.fights = false;
    }
    (h, b, me, world, beetle)
}

/// Give a player a perk of a kind at a value (its own made-up skill).
fn holds_perk(h: &mut Harness, world: ConnId, kind: u32, value: f64) {
    let skill = 9_000 + kind;
    h.server.skill_table.skills.insert(
        skill,
        SkillDef {
            id: skill,
            perk: Some(yalain_world::skills::Perk {
                kind,
                value,
                priority: 1,
            }),
            ..SkillDef::default()
        },
    );
    h.server
        .players
        .get_mut(&world)
        .expect("in the world")
        .play
        .skills
        .push(SkillState {
            skill,
            granted: 0,
            mask: 0x1,
            cost: 0,
            children: 0,
            dependents: 0,
            spent: 0,
        });
}

/// The beetle's template's category.
fn category(h: &mut Harness, of: u32) {
    let mut t = h
        .server
        .catalog
        .get(TEST_CREATURE)
        .expect("its template")
        .clone();
    t.props.set(
        yalain_content::props::creature_property::CATEGORY,
        Prop::Word(of),
    );
    h.server.catalog.insert(t);
}

/// A player's perks add to its ratings against the beetle: Day Stalker's 30 to its accuracy by
/// day and not by night, when Night Stalker's 50 does; Hunter: Nature's 15 against a creature of
/// Nature and not of another category; Defense Against Arcane's 30 to its evasion against a
/// creature of Arcane; Dungeon Crawler's 20 to its evasion indoors (where it stands) and Plains
/// Walker's not.
/// Behaviour: server.perks.ratings
#[test]
fn a_heros_perks_add_to_its_accuracy_and_evasion_where_they_apply() {
    let (mut h, _, _, world, beetle) = beside_a_beetle();
    let (me, it) = (Rater::Player(world), Rater::Thing(beetle));
    let at = |h: &Harness| ratings::of(&h.server, DEADEYE, me, it).accuracy;
    let ev = |h: &Harness| ratings::of(&h.server, DEADEYE, it, me).evasion;
    category(&mut h, 1);
    let (accuracy, evasion) = (at(&h), ev(&h));
    let near = |a: f64, b: f64| (a - b).abs() < 1e-6;

    holds_perk(&mut h, world, 4, 30.0);
    h.server.night = false;
    assert!(near(at(&h), accuracy + 30.0), "by day");
    h.server.night = true;
    assert!(near(at(&h), accuracy), "not by night");
    holds_perk(&mut h, world, 3, 50.0);
    assert!(near(at(&h), accuracy + 50.0), "Night Stalker by night");
    h.server.night = false;

    holds_perk(&mut h, world, 5, 15.0);
    assert!(near(at(&h), accuracy + 30.0 + 15.0), "Hunter: Nature");
    category(&mut h, 4);
    assert!(near(at(&h), accuracy + 30.0), "not against Arcane");

    holds_perk(&mut h, world, 11, 30.0);
    assert!(near(ev(&h), evasion + 30.0), "Defense Against Arcane");
    holds_perk(&mut h, world, 31, 35.0);
    assert!(near(ev(&h), evasion + 30.0), "Plains Walker not indoors");
    holds_perk(&mut h, world, 30, 20.0);
    assert!(
        near(ev(&h), evasion + 30.0 + 20.0),
        "Dungeon Crawler indoors"
    );
}

/// Deadly Precision adds its share to the chance of a player's criticals (1.5% at its top rank),
/// with no weapon as with one.
/// Behaviour: server.perks.criticals-heals-notice
#[test]
fn deadly_precision_adds_to_a_players_chance_of_a_critical() {
    let (mut h, _, _, world, _) = beside_a_beetle();
    let chance = |h: &Harness| yalain_world::combat::player_critical(&h.server, world, None);
    assert!(chance(&h).abs() < 1e-9);
    holds_perk(&mut h, world, 23, 0.015);
    assert!((chance(&h) - 0.015).abs() < 1e-9);
}

/// Fortitude makes a heal of a player's health more by its share (0.24 at its top rank: a heal of
/// 100 heals 124).
/// Behaviour: server.perks.criticals-heals-notice
#[test]
fn fortitude_makes_the_heals_a_player_receives_more() {
    let (mut h, _, _, world, _) = beside_a_beetle();
    h.server.effect_table.effects.insert(
        HEAL,
        EffectDef {
            kind: Kind::InstantVital {
                flags: vital::HEALTH,
                change: Scale(vec![(0.0, 100.0)]),
            },
            ..yalain_testkit::plain_effect(HEAL)
        },
    );
    let healed = |h: &mut Harness| -> i32 {
        let p = h.server.players.get_mut(&world).expect("in the world");
        quals::set_int(&mut p.qualities, quality::HEALTH_MAX, 1000);
        quals::set_int(&mut p.qualities, quality::HEALTH, 100);
        let clock = h.clock();
        effects::apply_on(
            &mut h.server,
            &mut h.net,
            On::Player(world),
            HEAL,
            Cast {
                spellcraft: 10.0,
                ..Cast::default()
            },
            clock,
        );
        quals::int(&h.server.players[&world].qualities, quality::HEALTH).expect("health") - 100
    };
    assert_eq!(healed(&mut h), 100);
    holds_perk(&mut h, world, 24, 0.24);
    assert_eq!(healed(&mut h), 124);
}

/// Imposing Facade makes the damage a player does count for more as the creature it hurts
/// chooses whom to fight (1.18 at its top rank): each hit's harm it remembers is the hit's damage
/// times that.
/// Behaviour: server.perks.criticals-heals-notice
#[test]
fn an_imposing_facade_makes_a_creature_notice_more_of_a_players_damage() {
    let (mut h, b, me, world, beetle) = beside_a_beetle();
    holds_perk(&mut h, world, 25, 1.18);
    h.arm(b, me, SWORD);
    // It fights back, so it keeps what it remembers of the harm.
    if let Some(t) = h.server.things.get_mut(beetle) {
        t.max_health = 1_000_000;
        t.health = 1_000_000;
        if let Some(c) = t.creature.as_mut() {
            c.stats.fights = true;
        }
    }
    if let Some(p) = h.server.players.get_mut(&world) {
        quals::set_int(&mut p.qualities, quality::HEALTH, 1_000_000);
    }
    let at = h
        .server
        .things
        .get(beetle)
        .and_then(|t| t.at)
        .expect("placed");
    if let Some(p) = h.server.players.get_mut(&world) {
        p.position.cell = at.cell;
        p.position.origin = [at.origin[0] - 1.0, at.origin[1], at.origin[2]];
    }
    let from = h.bots[b].inbox.len();
    h.bots[b].send(
        true,
        queue::WEENIE,
        &server_call(0x8204_0008, &[Arg::LongInt(beetle)]),
    );
    let hits = |h: &Harness| -> Vec<i64> {
        h.world_messages(b, from)
            .iter()
            .filter_map(|m| match m {
                ServerMessage::DoStory(e) if e.action.attacker == me => Some(e),
                _ => None,
            })
            .flat_map(|e| e.action.hooks.iter())
            .filter(|k| k.target == beetle && k.numbers[3] < 0)
            .map(|k| i64::from(-k.numbers[3]))
            .collect()
    };
    // The hit lands at its swing's hook, after its story is told.
    let remembered = |h: &Harness| {
        h.server
            .things
            .get(beetle)
            .and_then(|t| t.creature.as_ref())
            .and_then(|c| c.harm.first().map(|(_, _, n)| *n))
    };
    assert!(
        h.run_until(2000, |h| remembered(h).is_some()),
        "the harm remembered"
    );
    let hit = hits(&h)[0];
    let noticed = remembered(&h).expect("the harm remembered");
    #[allow(clippy::cast_precision_loss, clippy::cast_possible_truncation)]
    let want = (hit as f64 * 1.18).round() as i64;
    assert_eq!(noticed, want, "{hit}");
}
