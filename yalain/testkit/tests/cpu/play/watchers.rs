//! Effects that watch attacks on their holder: what they take off the harm and what they send back
//! to the attacker, on players and on creatures, for strikes, a player's swings and missiles.
//! Fixture: a made-up watcher on a test character or a test creature.

use auberean_protocol::types::StoryHook;
use auberean_protocol::{queue, ServerMessage};
use auberean_script::args::Arg;
use yalain_testkit::play::server_call;
use yalain_testkit::{Harness, TEST_CREATURE};
use yalain_world::effect_defs::{EffectDef, Kind, Scale, Share, Watcher};
use yalain_world::effects::{self, Cast, Harm, On};
use yalain_world::player::quality;
use yalain_world::quals;
use yalain_world::rules::Outcome;

const START_ATTACK: u32 = 0x8204_0008;
const WATCHER: u32 = 0x6F00_7401;
const OTHER_WATCHER: u32 = 0x6F00_7402;
const THIRD_WATCHER: u32 = 0x6F00_7403;
/// The human starting sword (a player attacks with a weapon in hand).
const STARTER_SWORD: u32 = 0x4700_051C;
/// The bits of a hook's target health change, attacker health change and attacker PK damage.
const TARGET_HEALTH: u32 = 0x800;
const ATTACKER_HEALTH: u32 = 0x1000;
const ATTACKER_PK: u32 = 0x4000;

/// Always this share, or nothing.
fn share(amount: f32) -> Share {
    Share {
        chance: 1.0,
        amount: Scale(vec![(0.0, amount)]),
        spread: 0.0,
    }
}

fn watcher(absorbs: f32, sends_back: f32) -> Watcher {
    Watcher {
        absorbs: if absorbs > 0.0 {
            share(absorbs)
        } else {
            Share::default()
        },
        sends_back: if sends_back > 0.0 {
            share(sends_back)
        } else {
            Share::default()
        },
        against_a_player: 1.0,
        shares: true,
        watches: true,
    }
}

fn def(state: u32, class: u32, w: Watcher) -> EffectDef {
    EffectDef {
        state,
        kind: Kind::Watcher(Box::new(w)),
        duration: Scale(vec![(0.0, 100_000.0)]),
        priority: Scale(vec![(0.0, 1.0)]),
        fx: 0,
        external_flags: 0,
        internal_flags: 0x1100_0003,
        enum_value: 0,
        eq_class: class,
        name: None,
    }
}

/// A record of this watcher on a player or a creature.
fn held(h: &mut Harness, on: On, state: u32, class: u32, w: Watcher) {
    h.server
        .effect_table
        .effects
        .insert(state, def(state, class, w));
    let cast = Cast {
        spellcraft: 30.0,
        ..Cast::default()
    };
    let clock = h.clock();
    assert!(effects::apply_on(
        &mut h.server,
        &mut h.net,
        on,
        state,
        cast,
        clock
    ));
}

fn tall(h: &mut Harness, me: u64) {
    with_player(h, me, |p| {
        quals::set_int(&mut p.qualities, quality::HEALTH_MAX, 100_000);
        quals::set_int(&mut p.qualities, quality::HEALTH, 100_000);
    });
}

fn with_player(h: &mut Harness, me: u64, f: impl FnOnce(&mut yalain_world::world::Player)) {
    for p in h.server.players.values_mut() {
        if p.character == me {
            f(p);
            return;
        }
    }
    panic!("in the world");
}

fn health_of(h: &Harness, me: u64) -> i32 {
    h.server
        .players
        .values()
        .find(|p| p.character == me)
        .and_then(|p| quals::int(&p.qualities, quality::HEALTH))
        .expect("in the world")
}

fn world_of(h: &Harness, me: u64) -> yalain_net::ConnId {
    yalain_world::social::world_of(&h.server, me).expect("in the world")
}

/// A test creature, with a great deal of health, beside a player who has drawn its scene; the
/// creature's natural damage is `natural`, so a fight's strikes have sizes to see.
fn beside(natural: f32) -> (Harness, usize, u64, u64) {
    let mut h = Harness::new();
    h.add_creatures(1);
    let (b, me) = h.player_in_world("one", "Elara");
    h.bots[b].send(
        true,
        queue::WEENIE,
        &auberean_protocol::ClientMessage::SceneRenderingComplete(auberean_protocol::login::Empty),
    );
    h.run(5);
    let id = h
        .world_messages(b, 0)
        .iter()
        .find_map(|m| match m {
            ServerMessage::CreateObject(o) if o.weenie.entity_did == TEST_CREATURE => Some(o.id),
            _ => None,
        })
        .expect("told of it");
    let t = h.server.things.get_mut(id).expect("made");
    t.health = 1_000_000;
    t.max_health = 1_000_000;
    t.creature
        .as_mut()
        .expect("a creature")
        .stats
        .natural_damage = natural;
    (h, b, me, id)
}

/// A player's attack run starts.
fn attack(h: &mut Harness, b: usize, target: u64) {
    h.bots[b].send(
        true,
        queue::WEENIE,
        &server_call(START_ATTACK, &[Arg::LongInt(target)]),
    );
}

/// The hooks told at `target`, by `attacker`.
fn hooks(h: &Harness, b: usize, from: usize, attacker: u64, target: u64) -> Vec<StoryHook> {
    let mut seen = Vec::new();
    for m in h.world_messages(b, from) {
        if let ServerMessage::DoStory(e) = m {
            if e.action.attacker == attacker {
                seen.extend(
                    e.action
                        .hooks
                        .iter()
                        .filter(|k| k.target == target)
                        .cloned(),
                );
            }
        }
    }
    seen
}

fn landed(k: &StoryHook) -> bool {
    k.numbers[0] == Outcome::Hit.code().cast_signed()
        || k.numbers[0] == Outcome::Critical.code().cast_signed()
}

/// The arithmetic the recordings hold: a share of the harm sent back, rounded up, reckoned in
/// single precision: 360 at a fifth is 72, not 73; 5 is 1; a line's value at the record's
/// spellcraft. A share taken is rounded up too and what goes back is of the harm it saw.
/// Behaviour: server.effects.watcher-sends-back
#[test]
fn a_share_sent_back_is_of_the_harm_rounded_up() {
    let (mut h, _, me, _) = beside(1.0);
    let on = On::Player(world_of(&h, me));
    held(&mut h, on, WATCHER, 43, watcher(0.0, 0.2));
    for (harm, back) in [(360, 72), (335, 67), (5, 1), (1, 1), (11, 3)] {
        assert_eq!(
            effects::answer_harm(&mut h.server, on, harm, false),
            Harm { left: harm, back },
            "{harm}"
        );
    }
    // The line of a caster's: 0.08 at 1, 0.35 at 50; at 20, 78 sends back 15 and 42 sends 8.
    let (mut h, _, me, _) = beside(1.0);
    let on = On::Player(world_of(&h, me));
    let line = Watcher {
        sends_back: Share {
            chance: 1.0,
            amount: Scale(vec![(1.0, 0.08), (50.0, 0.35)]),
            spread: 0.0,
        },
        ..watcher(0.0, 0.0)
    };
    h.server
        .effect_table
        .effects
        .insert(OTHER_WATCHER, def(OTHER_WATCHER, 43, line));
    let cast = Cast {
        spellcraft: 20.0,
        ..Cast::default()
    };
    let clock = h.clock();
    effects::apply_on(&mut h.server, &mut h.net, on, OTHER_WATCHER, cast, clock);
    assert_eq!(
        effects::answer_harm(&mut h.server, on, 78, false),
        Harm { left: 78, back: 15 }
    );
    assert_eq!(
        effects::answer_harm(&mut h.server, on, 42, false),
        Harm { left: 42, back: 8 }
    );
}

/// Each of a creature's strikes that lands on a holder tells what goes back, in the same hook as
/// the harm, and the creature loses that; an evaded one tells none.
/// Behaviour: server.effects.watcher-sends-back
#[test]
fn a_strike_on_a_holder_tells_what_goes_back_and_the_creature_loses_it() {
    let (mut h, b, me, id) = beside(40.0);
    tall(&mut h, me);
    let on = On::Player(world_of(&h, me));
    held(&mut h, on, WATCHER, 43, watcher(0.0, 0.25));
    let from = h.bots[b].inbox.len();
    let was = h.server.things.get(id).map_or(0, |t| t.health);
    let done = h.run_until(2000, |h| {
        hooks(h, b, from, id, me)
            .iter()
            .filter(|k| landed(k))
            .count()
            >= 6
    });
    assert!(done, "it strikes");
    // Let the last strike land.
    h.run(40);
    let told = hooks(&h, b, from, id, me);
    let mut sent = 0;
    for k in &told {
        if landed(k) {
            let harm = -k.numbers[3];
            assert!(harm > 0, "{k:?}");
            let back = (harm + 3) / 4;
            assert_eq!(k.bits & ATTACKER_HEALTH, ATTACKER_HEALTH, "{k:?}");
            assert_eq!(k.numbers[4], -back, "{k:?}");
            sent += back;
        } else {
            assert_eq!(k.bits & ATTACKER_HEALTH, 0, "an evaded strike: {k:?}");
        }
    }
    let lost = was - h.server.things.get(id).map_or(0, |t| t.health);
    assert_eq!(lost, sent, "the creature lost what went back");
}

/// A holder's armor of chance answers one strike in five, drawn alone (a 2017 armor's 0.2: 159 of
/// 852), sending nothing back otherwise.
/// Behaviour: server.effects.watcher-sends-back
#[test]
fn what_goes_back_goes_by_its_chance() {
    let (mut h, _, me, _) = beside(1.0);
    let on = On::Player(world_of(&h, me));
    let w = Watcher {
        sends_back: Share {
            chance: 0.2,
            amount: Scale(vec![(0.0, 0.5)]),
            spread: 0.0,
        },
        ..watcher(0.0, 0.0)
    };
    held(&mut h, on, WATCHER, 4013, w);
    let answered = (0..4000)
        .filter(|_| effects::answer_harm(&mut h.server, on, 100, false).back == 50)
        .count();
    assert!((700..900).contains(&answered), "{answered} of 4,000");
}

/// Two classes add (a wearer of 0.1 under a caster's 0.19 sent back 0.3); of one class only the
/// top one counts; and what a higher class takes off is off before a lower class sees it.
/// Behaviour: server.effects.watcher-sends-back
#[test]
fn two_classes_add_one_class_counts_once_and_higher_classes_go_first() {
    let (mut h, _, me, _) = beside(1.0);
    let on = On::Player(world_of(&h, me));
    held(&mut h, on, WATCHER, 4013, watcher(0.0, 0.1));
    held(&mut h, on, OTHER_WATCHER, 43, watcher(0.0, 0.2));
    assert_eq!(
        effects::answer_harm(&mut h.server, on, 100, false),
        Harm {
            left: 100,
            back: 30
        },
        "a tenth and a fifth"
    );
    let mut same_class = watcher(0.0, 0.9);
    same_class.against_a_player = 1.0;
    let mut higher = def(THIRD_WATCHER, 43, same_class);
    higher.priority = Scale(vec![(0.0, 2.0)]);
    h.server.effect_table.effects.insert(THIRD_WATCHER, higher);
    let (cast, clock) = (
        Cast {
            spellcraft: 30.0,
            ..Cast::default()
        },
        h.clock(),
    );
    effects::apply_on(&mut h.server, &mut h.net, on, THIRD_WATCHER, cast, clock);
    assert_eq!(
        effects::answer_harm(&mut h.server, on, 100, false),
        Harm {
            left: 100,
            back: 100
        },
        "a tenth, then the class's top alone: 0.9"
    );

    let (mut h, _, me, _) = beside(1.0);
    let on = On::Player(world_of(&h, me));
    held(&mut h, on, WATCHER, 45, watcher(0.5, 0.0));
    held(&mut h, on, OTHER_WATCHER, 43, watcher(0.0, 0.5));
    assert_eq!(
        effects::answer_harm(&mut h.server, on, 100, false),
        Harm { left: 50, back: 25 },
        "class 45 first: half taken, then half of what is left sent back"
    );
}

/// A player's swing at a creature holding a watcher is told with what is left of it and what comes
/// back; the creature loses the one, the player the other.
/// Behaviour: server.effects.watcher-absorbs
#[test]
fn a_swing_at_a_creature_holding_one_is_lessened_and_sent_back_at_the_swinger() {
    let (mut h, b, me, id) = beside(1.0);
    tall(&mut h, me);
    h.arm(b, me, STARTER_SWORD);
    let (worn, keep) = (
        h.server.things.get(id).map_or(0, |t| t.health),
        health_of(&h, me),
    );
    held(&mut h, On::Creature(id), WATCHER, 45, watcher(0.5, 0.0));
    held(
        &mut h,
        On::Creature(id),
        OTHER_WATCHER,
        43,
        watcher(0.0, 1.0),
    );
    // Nothing else hurts the player.
    if let Some(c) = h
        .server
        .things
        .get_mut(id)
        .and_then(|t| t.creature.as_mut())
    {
        c.stats.natural_damage = 0.0;
    }
    let from = h.bots[b].inbox.len();
    attack(&mut h, b, id);
    // A long run of evasions must not let it rest and regain what it lost.
    let mut steps = 0;
    while hooks(&h, b, from, me, id)
        .iter()
        .filter(|k| landed(k))
        .count()
        < 5
    {
        let now = h.now.0;
        if let Some(t) = h.server.things.get_mut(id) {
            t.last_hurt = now;
        }
        h.step();
        steps += 1;
        assert!(steps < 4000, "it swings");
    }
    h.bots[b].send(
        true,
        queue::WEENIE,
        &server_call(yalain_world::combat::function::STOP_ATTACK, &[]),
    );
    h.run(40);
    let told = hooks(&h, b, from, me, id);
    let (mut harm, mut back) = (0, 0);
    for k in told.iter().filter(|k| landed(k)) {
        assert!(k.numbers[3] < 0, "{k:?}");
        assert_eq!(
            k.numbers[4], k.numbers[3],
            "all of what is left goes back: {k:?}"
        );
        harm -= k.numbers[3];
        back -= k.numbers[4];
    }
    assert!(harm > 0, "harm told");
    let lost = worn - h.server.things.get(id).map_or(0, |t| t.health);
    let lost_me = keep - health_of(&h, me);
    // The player's strikes may land after it is told to stop only as far as they were told.
    assert_eq!(lost, harm, "the creature lost what was told");
    assert!(
        lost_me >= back - 1 && lost_me <= back + 200,
        "the player lost {lost_me} of {back}"
    );
}

/// A strike entirely taken tells a hit and no health change.
/// Behaviour: server.effects.watcher-absorbs
#[test]
fn a_strike_entirely_taken_tells_no_health_change() {
    let (mut h, b, me, id) = beside(40.0);
    tall(&mut h, me);
    let keep = health_of(&h, me);
    let on = On::Player(world_of(&h, me));
    held(&mut h, on, WATCHER, 45, watcher(1.0, 0.0));
    let from = h.bots[b].inbox.len();
    let done = h.run_until(2000, |h| {
        hooks(h, b, from, id, me)
            .iter()
            .filter(|k| landed(k))
            .count()
            >= 3
    });
    assert!(done, "it strikes");
    h.run(40);
    for k in hooks(&h, b, from, id, me).iter().filter(|k| landed(k)) {
        assert_eq!(k.bits & TARGET_HEALTH, 0, "{k:?}");
    }
    assert_eq!(health_of(&h, me), keep);
}

/// An amount larger than the harm heals what it exceeds, up to the holder's greatest health (2017:
/// a strike told as 106 health gained).
/// Behaviour: server.effects.watcher-absorbs
#[test]
fn an_amount_larger_than_the_harm_heals_and_not_past_the_greatest_health() {
    let (mut h, _, me, id) = beside(1.0);
    let world = world_of(&h, me);
    let on = On::Player(world);
    let w = Watcher {
        shares: false,
        ..watcher(135.0, 0.0)
    };
    held(&mut h, on, WATCHER, 4, w);
    assert_eq!(
        effects::answer_harm(&mut h.server, on, 29, false),
        Harm {
            left: -106,
            back: 0
        }
    );
    with_player(&mut h, me, |p| {
        quals::set_int(&mut p.qualities, quality::HEALTH_MAX, 500);
        quals::set_int(&mut p.qualities, quality::HEALTH, 300);
    });
    let clock = h.clock();
    let harm = Harm {
        left: -106,
        back: 0,
    };
    yalain_world::combat::creature_strike_lands(&mut h.server, &mut h.net, id, world, harm, clock);
    assert_eq!(health_of(&h, me), 406);
    yalain_world::combat::creature_strike_lands(&mut h.server, &mut h.net, id, world, harm, clock);
    yalain_world::combat::creature_strike_lands(&mut h.server, &mut h.net, id, world, harm, clock);
    assert_eq!(health_of(&h, me), 500);
}

/// A creature's health runs out by what went back: it dies, and the holder has the kill.
/// Behaviour: server.effects.watcher-sends-back
#[test]
fn a_creature_dies_of_what_goes_back_and_the_holder_has_the_kill() {
    let (mut h, b, me, id) = beside(40.0);
    h.server.levels = yalain_world::levels::LevelTable {
        levels: [0, 1_000]
            .map(|xp| yalain_world::levels::Level {
                xp,
                health: 100,
                vigor: 100,
                skill_credits: 0,
                hero_credits: 0,
            })
            .to_vec(),
    };
    tall(&mut h, me);
    let on = On::Player(world_of(&h, me));
    held(&mut h, on, WATCHER, 43, watcher(0.0, 1.0));
    if let Some(t) = h.server.things.get_mut(id) {
        t.health = 5;
        t.max_health = 5;
    }
    let dead = h.run_until(4000, |h| {
        h.world_messages(b, 0).iter().any(|m| {
            matches!(m, ServerMessage::UpdateBoolVisual(q) if q.object.id == id && q.quality == 279 && q.value == 1)
        })
    });
    assert!(dead, "it dies");
    let paid = h.world_messages(b, 0).iter().any(|m| {
        matches!(m, ServerMessage::UpdateLongInt(x) if x.quality == yalain_world::progress::q::TOTAL_XP && x.value > 0)
    });
    assert!(paid, "the kill is the holder's");
}

/// A player's health runs out by what comes back from a creature: it dies, the creature its killer.
/// Behaviour: server.effects.watcher-sends-back
#[test]
fn a_player_can_die_of_what_comes_back() {
    let (mut h, b, me, id) = beside(0.0);
    h.arm(b, me, STARTER_SWORD);
    held(&mut h, On::Creature(id), WATCHER, 43, watcher(0.0, 1.0));
    with_player(&mut h, me, |p| {
        quals::set_int(&mut p.qualities, quality::HEALTH, 1);
    });
    attack(&mut h, b, id);
    let died = h.run_until(4000, |h| health_of(h, me) == 0);
    assert!(died, "it dies of what came back");
}

/// Harm an effect does over time or at once is no attack: nothing goes back and nothing is taken
/// (2017: 58 such changes on wearers of a fifth, none sending back).
/// Behaviour: server.effects.watcher-sends-back
#[test]
fn an_effects_harm_is_no_attack() {
    let (mut h, b, me, id) = beside(0.0);
    tall(&mut h, me);
    let keep = health_of(&h, me);
    let world = world_of(&h, me);
    let on = On::Player(world);
    held(&mut h, on, WATCHER, 43, watcher(0.5, 1.0));
    const HARM: u32 = 0x6F00_7404;
    let mut d = def(HARM, 0, watcher(0.0, 0.0));
    d.kind = Kind::InstantVital {
        flags: 1,
        change: Scale(vec![(0.0, -50.0)]),
    };
    d.duration = Scale::default();
    h.server.effect_table.effects.insert(HARM, d);
    let (from, was, clock) = (
        h.bots[b].inbox.len(),
        h.server.things.get(id).map_or(0, |t| t.health),
        h.clock(),
    );
    let cast = Cast {
        caster: id,
        spellcraft: 30.0,
        ..Cast::default()
    };
    effects::apply_on(&mut h.server, &mut h.net, on, HARM, cast, clock);
    h.run(5);
    assert_eq!(health_of(&h, me), keep - 50);
    assert_eq!(h.server.things.get(id).map_or(0, |t| t.health), was);
    assert!(hooks(&h, b, from, id, me)
        .iter()
        .all(|k| k.bits & ATTACKER_HEALTH == 0));
}

/// Between players what goes back is multiplied by what the watcher says for that, told as PK
/// damage too. Our choice for the telling: no recording holds one.
/// Behaviour: server.effects.watcher-sends-back-at-a-player
#[test]
fn a_players_attack_sends_back_its_share_for_players_told_as_pk_damage() {
    let (mut h, _, me, id) = beside(1.0);
    let on = On::Player(world_of(&h, me));
    let w = Watcher {
        against_a_player: 1.0 / 3.0,
        ..watcher(0.0, 0.6)
    };
    held(&mut h, on, WATCHER, 43, w);
    assert_eq!(
        effects::answer_harm(&mut h.server, on, 100, true),
        Harm {
            left: 100,
            back: 21
        },
        "0.6 by a third, in single precision"
    );
    assert_eq!(
        effects::answer_harm(&mut h.server, on, 100, false),
        Harm {
            left: 100,
            back: 61
        },
        "a creature's attack, whole"
    );
    let mut told = ServerMessage::DoStory(auberean_protocol::physics::ObjectEvent {
        sender: yalain_world::inventory::visual(&mut h.server, id),
        action: auberean_protocol::types::PhysicsStory {
            hooks: vec![StoryHook {
                bits: TARGET_HEALTH | ATTACKER_HEALTH,
                target: me,
                numbers: [0, 0, 0, -100, -21, 0, 0],
                ..StoryHook::default()
            }],
            ..auberean_protocol::types::PhysicsStory::default()
        },
    });
    yalain_world::combat::with_pk_damage(&mut told, me);
    let ServerMessage::DoStory(e) = told else {
        panic!()
    };
    assert_eq!(e.action.hooks[0].bits & ATTACKER_PK, ATTACKER_PK);
    assert_eq!(e.action.hooks[0].numbers[6], 21);
}

/// A thrown attack's watcher answers as it arrives: its target tells what is left and what comes
/// back, and each side loses it then.
/// Behaviour: server.effects.watcher-absorbs
#[test]
fn a_thrown_attack_is_answered_as_it_arrives() {
    let (mut h, b, me, id) = beside(0.0);
    tall(&mut h, me);
    held(&mut h, On::Creature(id), WATCHER, 45, watcher(0.5, 0.0));
    held(
        &mut h,
        On::Creature(id),
        OTHER_WATCHER,
        43,
        watcher(0.0, 0.5),
    );
    let (from, was, keep, clock) = (
        h.bots[b].inbox.len(),
        h.server.things.get(id).map_or(0, |t| t.health),
        health_of(&h, me),
        h.clock(),
    );
    let thrown = yalain_world::combat::Landing {
        world: world_of(&h, me),
        target: id,
        skill: 690,
        outcome: Outcome::Hit,
        damage: 100,
        back: 0,
        index: 0,
        thrown: true,
        anim_hook: 1,
    };
    yalain_world::combat::lands(&mut h.server, &mut h.net, thrown, clock);
    h.run(2);
    let told = hooks(&h, b, from, me, id);
    assert_eq!(told.len(), 1, "{told:?}");
    assert_eq!((told[0].numbers[3], told[0].numbers[4]), (-50, -25));
    assert_eq!(was - h.server.things.get(id).map_or(0, |t| t.health), 50);
    assert_eq!(keep - health_of(&h, me), 25);
}

/// A special's strike and a special's missile are answered too: the strike as it is told, the
/// missile as it arrives.
/// Behaviour: server.effects.watcher-absorbs
#[test]
fn a_specials_strike_and_a_specials_missile_are_answered() {
    const BOW: u32 = 0x4700_051B;
    const DEADEYE: u32 = 207;
    const RIPOSTE: u32 = 176;
    use yalain_world::skill_use::function::USE;
    for (template, skill) in [(BOW, DEADEYE), (STARTER_SWORD, RIPOSTE)] {
        let (mut h, b, me, id) = beside(0.0);
        tall(&mut h, me);
        if let Some(c) = h
            .server
            .things
            .get_mut(id)
            .and_then(|t| t.creature.as_mut())
        {
            // Beyond the bow's least range, it stays there.
            c.stats.fights = skill == RIPOSTE;
        }
        h.arm(b, me, template);
        held(&mut h, On::Creature(id), WATCHER, 45, watcher(0.5, 0.0));
        held(
            &mut h,
            On::Creature(id),
            OTHER_WATCHER,
            43,
            watcher(0.0, 1.0),
        );
        let from = h.bots[b].inbox.len();
        if skill == RIPOSTE {
            attack(&mut h, b, id);
            let swung = |h: &Harness| !hooks(h, b, from, me, id).is_empty();
            assert!(h.run_until(400, swung), "in reach");
        }
        let told = (1..40).any(|n| {
            h.bots[b].send(
                true,
                queue::WEENIE,
                &server_call(USE, &[Arg::Int(n), Arg::Int(skill), Arg::LongInt(id)]),
            );
            h.run(120);
            h.world_messages(b, from).iter().any(|m| {
                matches!(m, ServerMessage::DoStory(e) if e.action.attacker == me
                    && e.action.skill == skill
                    && e.action.hooks.iter().any(|k| k.target == id && landed(k) && k.numbers[3] < 0))
            })
        });
        assert!(told, "{skill} landed");
        for m in h.world_messages(b, from) {
            let ServerMessage::DoStory(e) = m else {
                continue;
            };
            if e.action.attacker != me || e.action.skill != skill {
                continue;
            }
            for k in e
                .action
                .hooks
                .iter()
                .filter(|k| k.target == id && k.numbers[3] < 0)
            {
                assert_eq!(k.bits & ATTACKER_HEALTH, ATTACKER_HEALTH, "{skill}: {k:?}");
                assert_eq!(k.numbers[4], k.numbers[3], "{skill}: {k:?}");
            }
        }
    }
}

/// Harm over time of a class a protection names does not come on while the protection is held, and
/// one of another class does; when a chance runs along a line, the record's spellcraft reads it.
/// Behaviour: server.effects.protection-keeps-off-its-classes
#[test]
fn a_protection_keeps_off_what_it_names_by_its_chance_at_its_spellcraft() {
    let (mut h, _, me, _) = beside(0.0);
    tall(&mut h, me);
    let world = world_of(&h, me);
    let on = On::Player(world);
    const FIRE: u32 = 0x6F00_7405;
    const COLD: u32 = 0x6F00_7406;
    const PROTECTION: u32 = 0x6F00_7407;
    let of = |state, class| EffectDef {
        kind: Kind::InstantVital {
            flags: 1,
            change: Scale(vec![(0.0, -1.0)]),
        },
        duration: Scale::default(),
        ..def(state, class, watcher(0.0, 0.0))
    };
    for d in [of(FIRE, 48), of(COLD, 49)] {
        h.server.effect_table.effects.insert(d.state, d);
    }
    h.server.effect_table.effects.insert(
        PROTECTION,
        EffectDef {
            kind: Kind::Ward {
                classes: vec![48, 5013],
                chance: Scale(vec![(1.0, 0.2), (50.0, 0.7)]),
            },
            ..def(PROTECTION, 87, watcher(0.0, 0.0))
        },
    );
    let clock = h.clock();
    let cast = Cast {
        spellcraft: 50.0,
        ..Cast::default()
    };
    assert!(effects::apply_on(
        &mut h.server,
        &mut h.net,
        on,
        PROTECTION,
        cast,
        clock
    ));
    let through = |state: u32, h: &mut Harness| {
        (0..4000)
            .filter(|_| {
                effects::apply_on(&mut h.server, &mut h.net, on, state, Cast::default(), clock)
            })
            .count()
    };
    let fire = through(FIRE, &mut h);
    assert!(
        (1000..1400).contains(&fire),
        "at 50, 0.7 kept off: {fire} of 4,000 through"
    );
    assert_eq!(through(COLD, &mut h), 4000, "of another class");
}

/// A pulse of an aura does not put on what a Blocker it put on before keeps off, and puts on the
/// rest; the first pulse holds no Blocker yet and puts on both.
/// Behaviour: server.effects.protection-keeps-off-its-classes
#[test]
fn a_later_pulse_is_kept_off_by_the_blocker_of_the_first() {
    let (mut h, _, me, _) = beside(0.0);
    let world = world_of(&h, me);
    const SKIN: u32 = 0x6F00_7408;
    const BLOCKER: u32 = 0x6F00_7409;
    const AURA: u32 = 0x6F00_740A;
    h.server
        .effect_table
        .effects
        .insert(SKIN, def(SKIN, 117, watcher(0.0, 0.1)));
    h.server.effect_table.effects.insert(
        BLOCKER,
        EffectDef {
            kind: Kind::Ward {
                classes: vec![117],
                chance: Scale(vec![(0.0, 1.0)]),
            },
            ..def(BLOCKER, 118, watcher(0.0, 0.0))
        },
    );
    h.server.effect_table.effects.insert(
        AURA,
        EffectDef {
            kind: Kind::Aura(yalain_world::effect_defs::Aura {
                pulsed: vec![SKIN, BLOCKER],
                targets: yalain_world::effect_defs::aura_target::PLAYER,
                range: 300.0,
            }),
            ..def(AURA, 1401, watcher(0.0, 0.0))
        },
    );
    let clock = h.clock();
    let count = |h: &Harness, state: u32| {
        effects::records_of(&h.server, On::Player(world))
            .iter()
            .filter(|e| e.state == state)
            .count()
    };
    effects::aura_on(&mut h.server, &mut h.net, world, AURA, 0, 30.0, clock);
    let held = effects::aura_held(&h.server, world, AURA).expect("on");
    assert_eq!(
        (count(&h, SKIN), count(&h, BLOCKER)),
        (1, 1),
        "the first pulse"
    );
    effects::pulse(&mut h.server, &mut h.net, world, held.eid, clock);
    assert_eq!(count(&h, SKIN), 1, "kept off");
    assert_eq!(count(&h, BLOCKER), 2, "the rest comes on");
}

/// A practice dummy holds what sends a share of each attack back from its first moment, by its
/// level, and shows it; the dummy of its level on another kind of template holds nothing.
/// Behaviour: server.creatures.practice-dummy-sends-back
#[test]
fn a_practice_dummy_sends_back_a_share_of_each_attack_by_its_level() {
    use yalain_world::spawn::{
        practice_dummy_spellcraft, PRACTICE_DUMMIES, PRACTICE_DUMMY_EFFECT as HOLDS,
    };
    const STURDY: u32 = 0x4700_2DB6;
    /// Level 30, on the plain drudges' table.
    const NOT_ONE: u32 = 0x4700_2DB0;
    assert!(PRACTICE_DUMMIES.contains(&STURDY) && !PRACTICE_DUMMIES.contains(&NOT_ONE));
    assert_eq!(practice_dummy_spellcraft(40), 35.0);
    assert_eq!(practice_dummy_spellcraft(50), 40.0);
    assert_eq!(practice_dummy_spellcraft(60), 45.0);
    assert_eq!(practice_dummy_spellcraft(70), 50.0);
    let dummy = |template: u32| {
        let mut h = Harness::new();
        h.server.catalog = yalain_testkit::test_catalog();
        let mut props = yalain_content::Props::default();
        props
            .set("GameplayStatistics_Level", yalain_content::Prop::Int(40))
            .set(
                "GameplayStatistics_MaxHealth",
                yalain_content::Prop::Int(5_000_000),
            )
            .set("AICombat_NaturalDamage", yalain_content::Prop::Float(0.0))
            .set("AICombat_NaturalVariance", yalain_content::Prop::Float(0.0))
            .set("AI_PerceptionRadius", yalain_content::Prop::Float(20.0));
        h.server
            .catalog
            .insert(yalain_testkit::test_template(template, 0x1169, props));
        let mut w = watcher(0.0, 0.0);
        w.sends_back = Share {
            chance: 1.0,
            amount: Scale(vec![(1.0, 0.01), (500.0, 5.0)]),
            spread: 0.0,
        };
        let mut d = def(HOLDS, 43, w);
        d.fx = 0x4100_0719;
        h.server.effect_table.effects.insert(HOLDS, d);
        let (b, me) = h.player_in_world("one", "Elara");
        let at = h
            .server
            .players
            .values()
            .next()
            .expect("in the world")
            .position;
        let mut spot = at.origin;
        spot[0] += 1.0;
        h.server.spawns.camps.push(yalain_content::Camp {
            id: "made up".into(),
            cell: at.cell,
            origin: spot,
            radius: 0.0,
            population: 1,
            regen: 600.0,
            templates: vec![(template, 1)],
            spots: vec![(at.cell, spot)],
            placed: "authored for the tests".into(),
            confidence: "authored".into(),
            moves: yalain_content::CampMoves::default(),
        });
        h.server
            .camps
            .push(yalain_world::spawn::CampState::default());
        h.bots[b].send(
            true,
            queue::WEENIE,
            &auberean_protocol::ClientMessage::SceneRenderingComplete(
                auberean_protocol::login::Empty,
            ),
        );
        h.run(5);
        let id = h
            .world_messages(b, 0)
            .iter()
            .find_map(|m| match m {
                ServerMessage::CreateObject(o) if o.weenie.entity_did == template => Some(o.id),
                _ => None,
            })
            .expect("made for the player");
        (h, b, me, id)
    };
    let (h, _, _, id) = dummy(NOT_ONE);
    assert!(effects::records_of(&h.server, On::Creature(id)).is_empty());
    let (mut h, b, me, id) = dummy(STURDY);
    tall(&mut h, me);
    let r = effects::records_of(&h.server, On::Creature(id));
    assert_eq!(r.len(), 1);
    assert_eq!((r[0].state, r[0].spellcraft), (HOLDS, 35.0));
    assert!(h.world_messages(b, 0).iter().any(
        |m| matches!(m, ServerMessage::DoFx(e) if e.sender.id == id && e.action.fx == 0x4100_0719)
    ));
    h.arm(b, me, STARTER_SWORD);
    let from = h.bots[b].inbox.len();
    attack(&mut h, b, id);
    assert!(
        h.run_until(4000, |h| {
            hooks(h, b, from, me, id)
                .iter()
                .filter(|k| landed(k))
                .count()
                >= 4
        }),
        "it swings"
    );
    for k in hooks(&h, b, from, me, id).iter().filter(|k| landed(k)) {
        let harm = -k.numbers[3];
        assert!(harm > 0, "{k:?}");
        // Reckoned as the records' single-precision share is.
        let back = dereth_primitives::num::to_i32((harm as f32 * 0.35_f32).ceil());
        assert_eq!(k.numbers[4], -back, "{k:?}");
    }
}

/// The practice dummy of level 30 goes by the same line as the rest, a third of an attack less a
/// little. Our choice: it shows its effect's appearance, and no recording holds an attack on it.
/// Behaviour: server.creatures.practice-dummy-at-30
#[test]
fn the_weakest_practice_dummy_goes_by_the_same_line() {
    use yalain_world::spawn::{practice_dummy_spellcraft, PRACTICE_DUMMIES};
    assert!(PRACTICE_DUMMIES.contains(&0x4700_2DAF));
    assert_eq!(practice_dummy_spellcraft(30), 30.0);
}

/// A record of a harmful effect that lasts, of no use but to be seen.
fn lasting(state: u32, class: u32) -> EffectDef {
    EffectDef {
        kind: Kind::Other,
        internal_flags: 0x1100_0003 | yalain_world::effect_defs::HARMFUL,
        ..def(state, class, watcher(0.0, 0.0))
    }
}

/// What holds a reply answers a harmful effect from another with its own effect on the sender, at
/// the reply's spellcraft, and takes the effect too. Of a list only those of its classes are
/// answered; a help, one's own effect and an effect from no one are not; and two holders can't go
/// back and forth, an answer not being answered. Our choice: no recording holds one.
/// Behaviour: server.effects.reply-to-an-effect
#[test]
fn a_holder_answers_a_harmful_effect_on_its_sender_and_an_answer_is_not_answered() {
    const REPLY: u32 = 0x6F00_7410;
    const ANSWER: u32 = 0x6F00_7411;
    const HARM: u32 = 0x6F00_7412;
    const HELP: u32 = 0x6F00_7413;
    const OTHER_CLASS_HARM: u32 = 0x6F00_7414;
    const LISTING: u32 = 0x6F00_7415;
    const OTHER_REPLY: u32 = 0x6F00_7416;
    let reply = |state: u32, classes: Vec<u32>| EffectDef {
        kind: Kind::Reply {
            effect: ANSWER,
            chance: Scale(vec![(0.0, 1.0)]),
            classes,
        },
        ..def(state, 20, watcher(0.0, 0.0))
    };
    let (mut h, _, me, id) = beside(1.0);
    let table = &mut h.server.effect_table.effects;
    table.insert(REPLY, reply(REPLY, vec![]));
    table.insert(OTHER_REPLY, reply(OTHER_REPLY, vec![]));
    table.insert(LISTING, reply(LISTING, vec![51]));
    table.insert(ANSWER, lasting(ANSWER, 50));
    table.insert(HARM, lasting(HARM, 51));
    table.insert(OTHER_CLASS_HARM, lasting(OTHER_CLASS_HARM, 52));
    table.insert(
        HELP,
        EffectDef {
            internal_flags: 0x1100_0003,
            ..lasting(HELP, 53)
        },
    );
    let world = *h.server.players.keys().next().expect("in the world");
    let put = |h: &mut Harness, on: On, state: u32, caster: u64, spellcraft: f32| {
        let cast = Cast {
            caster,
            spellcraft,
            ..Cast::default()
        };
        let clock = h.clock();
        effects::apply_on(&mut h.server, &mut h.net, on, state, cast, clock)
    };
    let on_player = On::Player(world);
    let on_creature = On::Creature(id);
    let count = |h: &Harness, on: On, state: u32| {
        effects::records_of(&h.server, on)
            .iter()
            .filter(|e| e.state == state)
            .count()
    };
    // It holds the reply, its spellcraft its own; the player holds another.
    assert!(put(&mut h, on_creature, REPLY, 0, 30.0));
    // A help is not answered, nor one's own, nor one from no one.
    assert!(put(&mut h, on_creature, HELP, me, 5.0));
    assert_eq!(count(&h, on_player, ANSWER), 0, "a help");
    assert!(put(&mut h, on_creature, HARM, id, 5.0));
    assert_eq!(count(&h, on_creature, ANSWER), 0, "its own");
    assert!(put(&mut h, on_creature, HARM, 0, 5.0));
    assert_eq!(count(&h, on_creature, ANSWER), 0, "from no one");
    // Harm from the player is taken, and answered once at the reply's spellcraft.
    assert!(put(&mut h, on_creature, HARM, me, 5.0));
    assert_eq!(count(&h, on_creature, HARM), 3);
    let answers = effects::records_of(&h.server, on_player);
    let a = answers
        .iter()
        .find(|e| e.state == ANSWER)
        .expect("answered");
    assert_eq!((a.caster, a.spellcraft), (id, 30.0));
    assert_eq!(count(&h, on_player, ANSWER), 1);
    // The sender holds a reply too: its answer isn't answered, nor that one in turn.
    assert!(put(&mut h, on_player, OTHER_REPLY, 0, 30.0));
    assert!(put(&mut h, on_creature, HARM, me, 5.0));
    assert_eq!(count(&h, on_player, ANSWER), 2);
    assert_eq!(count(&h, on_creature, ANSWER), 0, "no answer comes back");
    // A list answers what it names.
    let (mut h, _, me, id) = beside(1.0);
    let table = &mut h.server.effect_table.effects;
    table.insert(LISTING, reply(LISTING, vec![51]));
    table.insert(ANSWER, lasting(ANSWER, 50));
    table.insert(HARM, lasting(HARM, 51));
    table.insert(OTHER_CLASS_HARM, lasting(OTHER_CLASS_HARM, 52));
    let world = *h.server.players.keys().next().expect("in the world");
    let (on_player, on_creature) = (On::Player(world), On::Creature(id));
    assert!(put(&mut h, on_creature, LISTING, 0, 30.0));
    assert!(put(&mut h, on_creature, OTHER_CLASS_HARM, me, 5.0));
    assert_eq!(count(&h, on_player, ANSWER), 0, "not of its list");
    assert!(put(&mut h, on_creature, HARM, me, 5.0));
    assert_eq!(count(&h, on_player, ANSWER), 1, "of its list");
    // What a ward keeps off is answered all the same.
    const WARD: u32 = 0x6F00_7417;
    let ward = EffectDef {
        kind: Kind::Ward {
            classes: vec![51],
            chance: Scale(vec![(0.0, 1.0)]),
        },
        ..def(WARD, 21, watcher(0.0, 0.0))
    };
    h.server.effect_table.effects.insert(WARD, ward);
    assert!(put(&mut h, on_creature, WARD, 0, 30.0));
    assert!(!put(&mut h, on_creature, HARM, me, 5.0));
    assert_eq!(count(&h, on_player, ANSWER), 2, "kept off, and answered");
}
