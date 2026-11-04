//! Creatures that shoot: a creature with a basic missile attack engaging from afar stands and
//! shoots, and its missiles arrive as stories its target tells; a special waits 40 s between uses.

use auberean_protocol::types::PhysicsStory;
use auberean_protocol::{queue, ServerMessage};
use yalain_content::Prop;
use yalain_testkit::{test_template, Harness, TEST_PROFILE};

/// A made-up shooting beetle.
const SHOOTER: u32 = 0x4700_2225;
/// The skills it is given: the plain missile, as a basic attack, and the poisoned missile, as a
/// special.
const MISSILE: u32 = 1861;
const POISON: u32 = 1863;

/// A camp of one shooter `distance` meters east of where a character starts, in the start's cell.
fn shooter_at(h: &mut Harness, distance: f32, special: bool) {
    let mut p = yalain_content::Props::default();
    p.set("GameplayStatistics_Level", Prop::Int(3))
        .set("GameplayStatistics_MaxHealth", Prop::Int(60))
        .set("AICombat_NaturalDamage", Prop::Float(10.0))
        .set("AICombat_NaturalVariance", Prop::Float(0.5))
        .set("AI_PerceptionRadius", Prop::Float(30.0))
        .set("AI_HomesickRadius", Prop::Float(40.0))
        .set("NPC_StartingSkill1", Prop::Word(MISSILE))
        .set("NPC_StartingSkill1Level", Prop::Int(1))
        .set("NPC_StartingSkill1_IsBase", Prop::Bool(true))
        .set("NPC_DeathLootProfile", Prop::Word(TEST_PROFILE));
    if special {
        p.set("NPC_StartingSkill2", Prop::Word(POISON))
            .set("NPC_StartingSkill2Level", Prop::Int(1));
    }
    let mut catalog = yalain_testkit::test_catalog();
    catalog.insert(test_template(SHOOTER, 0x1169, p));
    h.server.catalog = catalog;
    h.server.loot = yalain_testkit::test_loot();
    let spot = [59.06 + distance, 240.2, -44.6];
    h.server.spawns.camps.push(yalain_content::Camp {
        id: "shooters".into(),
        cell: 0x0299_0109,
        origin: spot,
        radius: 0.0,
        population: 1,
        regen: 60.0,
        templates: vec![(SHOOTER, 1)],
        spots: vec![(0x0299_0109, spot)],
        placed: "authored for the tests".into(),
        confidence: "authored".into(),
        moves: yalain_content::CampMoves::default(),
    });
    h.server
        .camps
        .push(yalain_world::spawn::CampState::default());
}

fn scene_drawn(h: &mut Harness, b: usize) {
    h.bots[b].send(
        true,
        queue::WEENIE,
        &auberean_protocol::ClientMessage::SceneRenderingComplete(auberean_protocol::login::Empty),
    );
    h.run(5);
}

fn shooter_id(h: &Harness, b: usize) -> u64 {
    h.world_messages(b, 0)
        .iter()
        .find_map(|m| match m {
            ServerMessage::CreateObject(o) if o.weenie.entity_did == SHOOTER => Some(o.id),
            _ => None,
        })
        .expect("the shooter is made")
}

/// A counter answers a shot that lands as it answers a strike: the shooter takes its effect.
/// Behaviour: server.effects.counter-answers-an-attack
#[test]
fn a_counter_answers_a_shot_that_lands_on_its_holder() {
    use yalain_world::effect_defs::{EffectDef, Kind, Scale};
    use yalain_world::player::quality;
    const COUNTER: u32 = 0x6F00_7311;
    const ANSWER: u32 = 0x6F00_7312;
    let mut h = Harness::new();
    shooter_at(&mut h, 22.0, false);
    let (b, me) = h.player_in_world("one", "Elara");
    scene_drawn(&mut h, b);
    let id = shooter_id(&h, b);
    let def = |state, kind, internal_flags| EffectDef {
        state,
        kind,
        duration: Scale::default(),
        priority: Scale::default(),
        fx: 0,
        external_flags: 0,
        internal_flags,
        enum_value: 0,
        eq_class: 0,
        name: None,
    };
    let harm = Kind::InstantVital {
        flags: 1,
        change: Scale(vec![(0.0, -1.0)]),
    };
    let always = Kind::Counter {
        effect: ANSWER,
        chance: Scale(vec![(0.0, 1.0)]),
    };
    for d in [def(ANSWER, harm, 0), def(COUNTER, always, 0x0100_0783)] {
        h.server.effect_table.effects.insert(d.state, d);
    }
    let world = *h
        .server
        .players
        .iter()
        .find(|(_, p)| p.character == me)
        .expect("in the world")
        .0;
    if let Some(p) = h.server.players.get_mut(&world) {
        yalain_world::quals::set_int(&mut p.qualities, quality::HEALTH_MAX, 100_000);
        yalain_world::quals::set_int(&mut p.qualities, quality::HEALTH, 100_000);
    }
    let clock = h.clock();
    yalain_world::effects::put_on_worn(
        &mut h.server,
        &mut h.net,
        world,
        0x8130_0000_0000_0001,
        &[(COUNTER, 30.0)],
        clock,
    );
    let from = h.bots[b].inbox.len();
    let answered = |h: &Harness| {
        h.world_messages(b, from).iter().any(
            |m| matches!(m, ServerMessage::DoStory(e) if e.sender.id == id && e.action.attacker == me),
        )
    };
    assert!(h.run_until(1000, answered), "it answers");
}

/// A shot that lands on a holder of what sends harm back tells it, and the shooter loses it.
/// Behaviour: server.effects.watcher-sends-back
#[test]
fn a_shot_on_a_holder_tells_what_goes_back_and_the_shooter_loses_it() {
    use yalain_world::effect_defs::{EffectDef, Kind, Scale, Share, Watcher};
    use yalain_world::effects::{self, Cast, On};
    use yalain_world::player::quality;
    const WATCHER: u32 = 0x6F00_7313;
    let mut h = Harness::new();
    shooter_at(&mut h, 22.0, false);
    let (b, me) = h.player_in_world("one", "Elara");
    scene_drawn(&mut h, b);
    let id = shooter_id(&h, b);
    h.server.effect_table.effects.insert(
        WATCHER,
        EffectDef {
            state: WATCHER,
            kind: Kind::Watcher(Box::new(Watcher {
                sends_back: Share {
                    chance: 1.0,
                    amount: Scale(vec![(0.0, 0.5)]),
                    spread: 0.0,
                },
                against_a_player: 1.0,
                shares: true,
                watches: true,
                ..Watcher::default()
            })),
            duration: Scale(vec![(0.0, 100_000.0)]),
            priority: Scale::default(),
            fx: 0,
            external_flags: 0,
            internal_flags: 0x1100_0003,
            enum_value: 0,
            eq_class: 43,
            name: None,
        },
    );
    let world = yalain_world::social::world_of(&h.server, me).expect("in the world");
    if let Some(p) = h.server.players.get_mut(&world) {
        yalain_world::quals::set_int(&mut p.qualities, quality::HEALTH_MAX, 100_000);
        yalain_world::quals::set_int(&mut p.qualities, quality::HEALTH, 100_000);
    }
    let clock = h.clock();
    effects::apply_on(
        &mut h.server,
        &mut h.net,
        On::Player(world),
        WATCHER,
        Cast::default(),
        clock,
    );
    let (from, was) = (
        h.bots[b].inbox.len(),
        h.server.things.get(id).map_or(0, |t| t.health),
    );
    let answered = |h: &Harness| {
        h.world_messages(b, from).iter().any(|m| {
            matches!(m, ServerMessage::DoStory(e) if e.action.attacker == id
                && e.action.hooks.iter().any(|k| k.bits & 0x1000 != 0))
        })
    };
    assert!(h.run_until(1000, answered), "it tells what goes back");
    let told = h.world_messages(b, from);
    let k = told
        .iter()
        .find_map(|m| match m {
            ServerMessage::DoStory(e) if e.action.attacker == id => e
                .action
                .hooks
                .iter()
                .find(|k| k.bits & 0x1000 != 0)
                .cloned(),
            _ => None,
        })
        .expect("told");
    assert_eq!(k.numbers[4], -((-k.numbers[3] + 1) / 2));
    assert!(
        h.server.things.get(id).map_or(0, |t| t.health) < was,
        "it lost it"
    );
}

/// A shooter's shot comes when it is due, not at the world's next step: each gap is its rhythm's over
/// the shot's time scale, within the harness's step.
/// Behaviour: server.creatures.attacks-when-due
#[test]
fn a_shot_comes_when_it_is_due_not_at_the_worlds_next_step() {
    let mut h = Harness::new();
    shooter_at(&mut h, 22.0, false);
    let (b, _me) = h.player_in_world("one", "Elara");
    scene_drawn(&mut h, b);
    let id = shooter_id(&h, b);
    let from = h.bots[b].inbox.len();
    let gap = yalain_world::missiles::rhythm(SHOOTER).gap;
    let mut at: Vec<(f64, f32)> = Vec::new();
    let mut told = 0;
    for _ in 0..1000 {
        h.step();
        let now: Vec<f32> = h
            .world_messages(b, from)
            .iter()
            .filter_map(|m| match m {
                ServerMessage::DoStory(e)
                    if e.sender.id == id && e.action.attacker_behavior.time_scale > 0.0 =>
                {
                    Some(e.action.attacker_behavior.time_scale)
                }
                _ => None,
            })
            .collect();
        if now.len() > told {
            at.push((h.now.0, *now.last().expect("told")));
            told = now.len();
        }
    }
    assert!(at.len() > 10, "{} shots", at.len());
    for pair in at.windows(2) {
        let seen = pair[1].0 - pair[0].0;
        let due = f64::from(gap / pair[0].1);
        assert!(
            (seen - due).abs() <= 0.051 + 0.001,
            "a gap of {seen:.3} where {due:.3} was due"
        );
    }
}

/// A creature with a basic missile attack that notices a player from afar stands where it is and
/// shoots: its story names the skill and a missile hook (kind 3) at the player, and its places
/// keep its spot; each missile arrives as a story the player tells, naming the creature as the
/// attacker and holding one result hook (kind 1).
/// Behaviour: server.creatures.missiles-stand-and-shoot
#[test]
fn a_creature_with_missiles_stands_and_shoots_and_its_target_tells_each_arrival() {
    let mut h = Harness::new();
    shooter_at(&mut h, 22.0, false);
    let (b, me) = h.player_in_world("one", "Elara");
    scene_drawn(&mut h, b);
    let id = shooter_id(&h, b);
    let from = h.bots[b].inbox.len();
    h.run(80);
    let msgs = h.world_messages(b, from);
    let spot = 59.06 + 22.0;
    let moved = msgs.iter().any(|m| {
        matches!(m, ServerMessage::Position(p) | ServerMessage::PositionCell(p)
            if p.sender.id == id && ((p.origin.x - spot).abs() > 0.01 || (p.origin.y - 240.2).abs() > 0.01))
    });
    assert!(
        !moved,
        "a shooter stands where it is (turning to face its target)"
    );
    let launches: Vec<PhysicsStory> = msgs
        .iter()
        .filter_map(|m| match m {
            ServerMessage::DoStory(e) if e.sender.id == id => Some(e.action.clone()),
            _ => None,
        })
        .collect();
    assert!(launches.len() >= 2, "it shoots again and again");
    for l in &launches {
        assert_eq!(l.bits, 0x36, "a basic attack with its behavior");
        assert_eq!(l.skill, MISSILE);
        assert_eq!(l.attacker, id);
        assert_eq!(l.hooks.len(), 2);
        assert_eq!(l.hooks[1].bits, 0x303, "the missile's hook");
        assert_eq!(l.hooks[1].target, me);
    }
    let arrivals: Vec<PhysicsStory> = msgs
        .iter()
        .filter_map(|m| match m {
            ServerMessage::DoStory(e) if e.sender.id == me => Some(e.action.clone()),
            _ => None,
        })
        .collect();
    assert!(!arrivals.is_empty(), "the missiles arrive");
    for a in &arrivals {
        assert_eq!(
            a.bits, 0x32,
            "told by the target: the attacker and the skill"
        );
        assert_eq!(a.attacker, id);
        assert_eq!(a.hooks.len(), 1);
        assert_eq!(a.hooks[0].bits & 0xF, 1, "a result hook");
        assert_eq!(a.hooks[0].target, me);
    }
}

/// A missile arrives after the creature's windup and its flight: from 22 m at 35 m/s with the
/// beetles' windup (0.55 s at the median time scale), between about 0.9 and 1.4 s after the shot.
/// Behaviour: server.creatures.missiles-arrive-after-flight
#[test]
fn a_missile_arrives_after_the_windup_and_its_flight() {
    let mut h = Harness::new();
    shooter_at(&mut h, 22.0, false);
    let (b, me) = h.player_in_world("one", "Elara");
    scene_drawn(&mut h, b);
    let id = shooter_id(&h, b);
    let mut shot_at = None;
    let mut arrived_at = None;
    for _ in 0..200 {
        let from = h.bots[b].inbox.len();
        h.step();
        let now = h.clock().local.0;
        for m in h.world_messages(b, from) {
            if let ServerMessage::DoStory(e) = m {
                if e.sender.id == id && shot_at.is_none() {
                    shot_at = Some(now);
                } else if e.sender.id == me && shot_at.is_some() && arrived_at.is_none() {
                    arrived_at = Some(now);
                }
            }
        }
        if arrived_at.is_some() {
            break;
        }
    }
    let flight = arrived_at.expect("it arrives") - shot_at.expect("it shoots");
    assert!((0.8..=1.5).contains(&flight), "flight {flight} s");
}

/// A hit lowers the player's health, sent before the story of the arrival.
/// Behaviour: server.creatures.missiles-health-before-the-story
#[test]
fn a_missiles_hit_sends_the_players_health_before_the_story() {
    let mut h = Harness::new();
    shooter_at(&mut h, 22.0, false);
    let (b, me) = h.player_in_world("one", "Elara");
    scene_drawn(&mut h, b);
    let from = h.bots[b].inbox.len();
    h.run(400);
    let msgs = h.world_messages(b, from);
    let hit = msgs.iter().position(|m| {
        matches!(m, ServerMessage::DoStory(e) if e.sender.id == me
            && e.action.hooks.first().is_some_and(|k| k.bits == 0xB81))
    });
    let hit = hit.expect("a missile hits within the run");
    let health_before = msgs[..hit]
        .iter()
        .rev()
        .take(3)
        .any(|m| matches!(m, ServerMessage::UpdateInt(q) if q.quality == 258));
    assert!(
        health_before,
        "the health comes just before the hit's story"
    );
}

/// A special is told without the basic-attack flag (bits 0x16) and arrives told the same way (bits
/// 0x12); a use bars it for 40 s, so the creature uses it again only after that.
/// Behaviour: server.creatures.specials-barred-forty-seconds
#[test]
fn a_creatures_special_waits_forty_seconds_before_it_is_used_again() {
    let mut h = Harness::new();
    shooter_at(&mut h, 22.0, true);
    let (b, _) = h.player_in_world("one", "Elara");
    // The player outlasts the fight.
    for p in h.server.players.values_mut() {
        yalain_world::quals::set_int(&mut p.qualities, 263, 1_000_000);
        yalain_world::quals::set_int(&mut p.qualities, 258, 1_000_000);
    }
    scene_drawn(&mut h, b);
    let id = shooter_id(&h, b);
    let from = h.bots[b].inbox.len();
    let specials = |h: &Harness| -> Vec<PhysicsStory> {
        h.world_messages(b, from)
            .into_iter()
            .filter_map(|m| match m {
                ServerMessage::DoStory(e) if e.sender.id == id && e.action.skill == POISON => {
                    Some(e.action)
                }
                _ => None,
            })
            .collect()
    };
    // When each use reached the player, on the harness's clock.
    let mut used_at = Vec::new();
    for _ in 0..3000 {
        h.step();
        if specials(&h).len() > used_at.len() {
            used_at.push(h.now.0);
        }
    }
    let all = specials(&h);
    assert_eq!(all[0].bits, 0x16, "without the basic-attack flag");
    let arrival = h.world_messages(b, from).into_iter().any(|m| {
        matches!(m, ServerMessage::DoStory(e)
            if e.action.skill == POISON && e.sender.id != id && e.action.bits == 0x12)
    });
    assert!(arrival, "its arrival is told without the basic-attack flag");
    assert!(used_at.len() >= 2, "used again later: {used_at:?}");
    assert!(
        used_at.windows(2).all(|w| w[1] - w[0] >= 40.0),
        "never within 40 s: {used_at:?}"
    );
}
