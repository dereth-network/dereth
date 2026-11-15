//! Creatures that fight other creatures as the world's friend table rows say, a creature players may
//! not harm, and steps moved by a death or a hurt near the player whoever did it. Fixture: the test
//! world's hall beside where a character starts, a camp of made-up attackers and a made-up target
//! that never moves.

use auberean_script::args::Arg;
use yalain_content::quests::{Step, Trigger, Update};
use yalain_content::Prop;
use yalain_testkit::play::server_call;
use yalain_testkit::Harness;
use yalain_world::calls::{self, Value};
use yalain_world::quests::{phase_of, status};
use yalain_world::state::QuestState;
use yalain_world::things::Mode;

const ATTACKER: u32 = 0x4700_2230;
const TARGET: u32 = 0x4700_2231;
/// The attackers' friend table, and the target's.
const ATTACKERS: u32 = 0x4B;
const TARGETS: u32 = 0x16;
const HALL: u32 = 0x0299_0109;
/// Where each stands: the attacker 5 m from where a character starts, the target 4 m past it.
const ATTACKER_AT: [f32; 3] = [64.0, 240.0, -44.6];
const TARGET_AT: [f32; 3] = [68.0, 240.0, -44.6];
const STARTER_SWORD: u32 = 0x4700_051C;
const START_ATTACK: u32 = 0x8204_0008;
const SHOW_TEXT: u32 = 0x8205_0011;
/// The line a player's attack on the target is told.
const KEPT_OUT_LINE: (u32, u32) = (0x383D_0528, 0x2500_25F5);
/// The client's error of a target whose permissions keep players out.
const KEPT_OUT: u32 = 21_000;

fn camp(h: &mut Harness, template: u32, at: [f32; 3], props: yalain_content::Props) {
    h.server
        .catalog
        .insert(yalain_testkit::test_template(template, 0x1169, props));
    h.server.spawns.camps.push(yalain_content::Camp {
        id: format!("camp {template:08X}"),
        cell: HALL,
        origin: at,
        radius: 0.0,
        population: 1,
        regen: 600.0,
        templates: vec![(template, 1)],
        spots: vec![(HALL, at)],
        placed: "authored for the tests".into(),
        confidence: "authored".into(),
        moves: yalain_content::CampMoves::default(),
        name: None,
    });
    h.server
        .camps
        .push(yalain_world::spawn::CampState::default());
}

/// A world with an attacker and a target of these healths, no player in it; with `fights`, the
/// attackers' row fights the target's table. The attacker attacks a player first as `first` says.
fn world(fights: bool, first: bool, target_health: i64) -> (Harness, u64, u64) {
    let mut h = Harness::new();
    h.server.catalog = yalain_testkit::test_catalog();
    let mut a = yalain_content::Props::default();
    a.set("GameplayStatistics_Level", Prop::Int(40))
        .set("GameplayStatistics_MaxHealth", Prop::Int(1_000_000))
        .set("AICombat_NaturalDamage", Prop::Float(80.0))
        .set("AICombat_NaturalVariance", Prop::Float(0.2))
        .set("AI_PerceptionRadius", Prop::Float(20.0))
        .set("AI_LoveTable", Prop::Word(ATTACKERS));
    camp(&mut h, ATTACKER, ATTACKER_AT, a);
    let mut t = yalain_content::Props::default();
    t.set("GameplayStatistics_Level", Prop::Int(1))
        .set("GameplayStatistics_MaxHealth", Prop::Int(target_health))
        .set("AI_MovementType", Prop::Word(0x4000_0001))
        .set("AI_LoveTable", Prop::Word(TARGETS));
    camp(&mut h, TARGET, TARGET_AT, t);
    if fights {
        h.server
            .creature_rows
            .fights
            .insert(ATTACKERS, [TARGETS].into_iter().collect());
    }
    if !first {
        h.server
            .creature_rows
            .attacks_first
            .insert(ATTACKERS, false);
    }
    h.run(4);
    let (a, t) = (of_template(&h, ATTACKER), of_template(&h, TARGET));
    (h, a, t)
}

fn of_template(h: &Harness, template: u32) -> u64 {
    h.server
        .things
        .by_id
        .values()
        .find(|t| t.template == template && t.creature.is_some())
        .map(|t| t.id)
        .expect("made by its camp")
}

/// A player in the world, its scene drawn.
fn enters(h: &mut Harness, account: &str, name: &str) -> (usize, u64) {
    let (b, me) = h.player_in_world(account, name);
    h.bots[b].send(
        true,
        auberean_protocol::queue::WEENIE,
        &auberean_protocol::ClientMessage::SceneRenderingComplete(auberean_protocol::login::Empty),
    );
    h.run(4);
    (b, me)
}

fn target_of(h: &Harness, id: u64) -> Option<u64> {
    h.server
        .things
        .get(id)
        .and_then(|t| t.creature.as_ref())
        .filter(|c| c.mode == Mode::Chasing)
        .and_then(|c| c.target)
}

fn health(h: &Harness, id: u64) -> i32 {
    h.server.things.get(id).map_or(0, |t| t.health)
}

fn dead(h: &Harness, id: u64) -> bool {
    h.server
        .things
        .get(id)
        .and_then(|t| t.creature.as_ref())
        .is_none_or(|c| c.mode == Mode::Dead)
}

fn calls_of(h: &Harness, b: usize, from: usize) -> Vec<(u32, Vec<Value>)> {
    h.calls(b, from)
}

fn shown(h: &Harness, b: usize, from: usize) -> Vec<(u32, u32)> {
    calls_of(h, b, from)
        .into_iter()
        .filter(|(f, _)| *f == SHOW_TEXT)
        .filter_map(|(_, v)| {
            v.iter().find_map(|x| match x.root() {
                Some(calls::Node::TextInfo(t)) => Some((t.string_id, t.table)),
                _ => None,
            })
        })
        .collect()
}

/// A creature whose friend table the world's rows say fights the target's goes after it once a
/// player is near (none before), strikes it down over time and comes to rest; the player, whom it
/// never attacks first, it leaves alone. Without the row, or with one naming another table, the two
/// leave each other alone.
/// Behaviour: server.creatures.fights-a-friend-table-its-row-names
#[test]
fn a_creature_whose_row_fights_anothers_table_strikes_one_down_once_a_player_is_near() {
    let (mut h, attacker, target) = world(true, false, 2_000);
    h.run(200);
    assert_eq!(target_of(&h, attacker), None, "no player near: at rest");
    let (_, me) = enters(&mut h, "one", "Elara");
    assert!(
        h.run_until(100, |h| target_of(h, attacker) == Some(target)),
        "a player near: it goes after the target"
    );
    let whole = health(&h, target);
    assert!(
        h.run_until(1200, |h| health(h, target) < whole),
        "it strikes it"
    );
    assert!(h.run_until(12_000, |h| dead(h, target)), "struck down");
    assert_ne!(target_of(&h, attacker), Some(me), "never the player");
    assert!(
        h.run_until(400, |h| target_of(h, attacker).is_none()),
        "it gives up once its target is dead"
    );
    for other in [None, Some(0x42)] {
        let (mut h, attacker, target) = world(false, false, 2_000);
        if let Some(table) = other {
            h.server
                .creature_rows
                .fights
                .insert(ATTACKERS, [table].into_iter().collect());
        }
        let whole = health(&h, target);
        enters(&mut h, "one", "Elara");
        h.run(600);
        assert_eq!(target_of(&h, attacker), None, "{other:?}: no fight");
        assert_eq!(health(&h, target), whole);
    }
}

/// A creature that attacks first, looking about with a player in reach of it, takes the nearer of
/// that player and the creature its row fights.
/// Behaviour: server.creatures.fights-a-friend-table-its-row-names
#[test]
fn a_creature_takes_the_nearer_of_a_player_and_the_creature_its_row_fights() {
    for (x, nearer) in [(ATTACKER_AT[0] - 1.0, "the player"), (58.0, "the creature")] {
        let (mut h, attacker, target) = world(true, true, 1_000_000);
        let later = h.now.0 + 1_000.0;
        if let Some(c) = h
            .server
            .things
            .get_mut(attacker)
            .and_then(|t| t.creature.as_mut())
        {
            c.next_look = later;
        }
        let (_, me) = enters(&mut h, "one", "Elara");
        for p in h.server.players.values_mut() {
            p.protected = None;
            p.position.origin = [x, ATTACKER_AT[1], ATTACKER_AT[2]];
        }
        let now = h.now.0;
        if let Some(c) = h
            .server
            .things
            .get_mut(attacker)
            .and_then(|t| t.creature.as_mut())
        {
            c.next_look = now;
        }
        let want = if nearer == "the player" { me } else { target };
        assert!(
            h.run_until(100, |h| target_of(h, attacker).is_some()),
            "{nearer}: it takes on one"
        );
        assert_eq!(target_of(&h, attacker), Some(want), "{nearer}");
    }
}

/// A creature that attacks first takes the creature its row fights when it is nearer than the
/// player; it never turns from it to a player that has not hurt it, and turns to one that does.
/// Behaviour: server.creatures.fighting-another-turns-only-to-a-hurter
#[test]
fn a_creature_fighting_another_turns_only_to_a_player_that_hurts_it() {
    let (mut h, attacker, target) = world(true, true, 1_000_000);
    let (_, me) = enters(&mut h, "one", "Elara");
    assert!(
        h.run_until(100, |h| target_of(h, attacker) == Some(target)),
        "the nearer of the two"
    );
    for _ in 0..2_400 {
        h.step();
        assert_eq!(target_of(&h, attacker), Some(target), "unhurt, never turns");
    }
    let mut turned = false;
    for _ in 0..1_200 {
        let clock = h.clock();
        yalain_world::combat::hurt_creature(&mut h.server, &mut h.net, attacker, me, 1, clock);
        h.step();
        if target_of(&h, attacker) == Some(me) {
            turned = true;
            break;
        }
    }
    assert!(turned, "hurt by the player, it turns to it");
}

/// A player's attack on a creature the world's rows keep players from is refused as kept out, its
/// line told, and the creature is not struck; without the row the same attack strikes it.
/// Behaviour: server.creatures.players-kept-out-by-a-row
#[test]
fn a_player_may_not_harm_a_creature_its_row_keeps_players_from() {
    for kept in [true, false] {
        let (mut h, _, target) = world(false, false, 2_000);
        if kept {
            h.server
                .creature_rows
                .kept_out
                .insert(TARGET, Some(KEPT_OUT_LINE));
        }
        let (b, me) = enters(&mut h, "one", "Elara");
        h.arm(b, me, STARTER_SWORD);
        // Beside the target: the client doesn't walk the player in.
        for p in h.server.players.values_mut() {
            p.position.origin = [TARGET_AT[0] - 1.0, TARGET_AT[1], TARGET_AT[2]];
        }
        h.run(10);
        let from = h.bots[b].inbox.len();
        let whole = health(&h, target);
        h.bots[b].send(
            true,
            auberean_protocol::queue::WEENIE,
            &server_call(START_ATTACK, &[Arg::LongInt(target)]),
        );
        let struck = h.run_until(800, |h| health(h, target) < whole);
        let failed = calls_of(&h, b, from).contains(&(
            yalain_world::combat::function::BASIC_ATTACKS_FAILED,
            vec![Value::Int(KEPT_OUT)],
        ));
        if kept {
            assert!(!struck, "not struck");
            assert!(failed, "refused as kept out");
            assert!(shown(&h, b, from).contains(&KEPT_OUT_LINE), "its line told");
            let p = h.server.players.values().next().expect("in");
            assert!(p.attacking.is_none(), "no run");
        } else {
            assert!(struck, "no row: struck");
            assert!(!failed);
        }
    }
}

/// A step moved by a death of the target's template within 30 m moves the quest of a player that
/// near when another creature strikes it down, and not of a player farther off.
/// Behaviour: server.quests.a-death-nearby-moves-a-step
#[test]
fn a_death_within_a_steps_meters_moves_it_for_each_player_that_near_whoever_kills_it() {
    let (mut h, _, target, near, far, quest) = two_players(true, Trigger::KillNear(TARGET, 30));
    assert!(h.run_until(12_000, |h| dead(h, target)), "struck down");
    h.run(4);
    assert_eq!(phase(&h, near, quest), 2, "near: moved");
    assert_eq!(phase(&h, far, quest), 1, "100 m off: not");
}

/// The quest's step on this trigger (a death or a hurt of the target within 30 m), the quest at its
/// first phase for two players, the second 100 m off.
fn two_players(fights: bool, trigger: Trigger) -> (Harness, u64, u64, u64, u64, u32) {
    let q = super::kill_quests::quest(3);
    let quest = q.id;
    let (mut h, attacker, target) = world(fights, false, 2_000);
    h.server.quests.quests.push(q);
    h.server.quests.steps.push(Step {
        out_of_turn: false,
        key: String::new(),
        quest,
        needs: 1,
        trigger,
        tell: None,
        update: Update::Advance(2),
        xp: 0,
        gives: Vec::new(),
        counts: Vec::new(),
        effects: Vec::new(),
        text: None,
        recipes: Vec::new(),
        placed: "authored for a test".into(),
        requires: Vec::new(),
        takes: Vec::new(),
        races: Vec::new(),
        effects_last: false,
        effects_quiet: false,
        wants: None,
        fellows: false,
        levels: None,
        xp_share: 0.0,
        to: None,
        transforms: None,
        chance: None,
        to_fellows: false,
    });
    let (_, near) = enters(&mut h, "one", "Elara");
    let (_, far) = enters(&mut h, "two", "Kestrel");
    for p in h.server.players.values_mut() {
        p.play.quests.push(QuestState {
            quest,
            status: status::UNDERWAY,
            phase: 1,
            started: 1_000_000,
            updated: 1_000_000,
        });
        if p.character == far {
            p.position.origin[1] -= 100.0;
        }
    }
    (h, attacker, target, near, far, quest)
}

fn phase(h: &Harness, who: u64, quest: u32) -> u32 {
    let p = h.server.players.values().find(|p| p.character == who);
    p.map_or(0, |p| phase_of(&p.play.quests, quest))
}

/// A player's kill of the target moves the step as another creature's does, for the players near;
/// a kill of a creature of another template there moves nothing.
/// Behaviour: server.quests.a-death-nearby-moves-a-step
#[test]
fn a_players_kill_moves_a_death_nearbys_step_and_another_templates_death_does_not() {
    let (mut h, attacker, target, near, far, quest) =
        two_players(false, Trigger::KillNear(TARGET, 30));
    let clock = h.clock();
    yalain_world::combat::hurt_creature(
        &mut h.server,
        &mut h.net,
        attacker,
        near,
        2_000_000,
        clock,
    );
    h.run(4);
    assert!(dead(&h, attacker));
    assert_eq!(phase(&h, near, quest), 1, "another template's death");
    let clock = h.clock();
    yalain_world::combat::hurt_creature(&mut h.server, &mut h.net, target, near, 5_000, clock);
    h.run(4);
    assert!(dead(&h, target));
    assert_eq!(
        phase(&h, near, quest),
        2,
        "the target's, at the killer's hand"
    );
    assert_eq!(phase(&h, far, quest), 1, "100 m off");
}

/// A step moved by a hurt of the target's template within 30 m moves the quest of a player that
/// near at the first blow another creature lands on it, long before it falls, and not of a player
/// farther off.
/// Behaviour: server.quests.a-hurt-nearby-moves-a-step
#[test]
fn a_hurt_within_a_steps_meters_moves_it_for_each_player_that_near_whoever_hurts_it() {
    let (mut h, _, target, near, far, quest) = two_players(true, Trigger::HurtNear(TARGET, 30));
    let whole = health(&h, target);
    assert!(h.run_until(12_000, |h| health(h, target) < whole), "struck");
    assert_eq!(phase(&h, near, quest), 2, "near: moved at the first hurt");
    assert_eq!(phase(&h, far, quest), 1, "100 m off: not");
    assert!(!dead(&h, target), "moved long before it falls");
}

/// A player's hurt moves a hurt nearby's step as another creature's does, for the players near and
/// only once it takes health: a blow that takes none, and a hurt of a creature of another template
/// there, move nothing. A player at the step's phase only after the first hurt, or come near only
/// after it, moves at the next.
/// Behaviour: server.quests.a-hurt-nearby-moves-a-step
#[test]
fn a_players_hurt_moves_a_hurt_nearbys_step_once_it_takes_health_and_moves_latecomers_at_the_next()
{
    let (mut h, attacker, target, near, far, quest) =
        two_players(false, Trigger::HurtNear(TARGET, 30));
    let hurt = |h: &mut Harness, id: u64, damage: i32| {
        let clock = h.clock();
        yalain_world::combat::hurt_creature(&mut h.server, &mut h.net, id, near, damage, clock);
        h.run(4);
    };
    hurt(&mut h, attacker, 100);
    assert_eq!(phase(&h, near, quest), 1, "another template's hurt");
    hurt(&mut h, target, 0);
    assert_eq!(phase(&h, near, quest), 1, "a blow that takes no health");
    hurt(&mut h, target, 100);
    assert!(!dead(&h, target));
    assert_eq!(
        phase(&h, near, quest),
        2,
        "the target's, at its hurter's hand"
    );
    assert_eq!(phase(&h, far, quest), 1, "100 m off");
    // The one far off comes near, and the one near is put back at the step's phase.
    for p in h.server.players.values_mut() {
        if p.character == far {
            p.position.origin[1] += 100.0;
        }
        if p.character == near {
            p.play.quests.retain(|q| q.quest != quest);
            p.play.quests.push(QuestState {
                quest,
                status: status::UNDERWAY,
                phase: 1,
                started: 1_000_000,
                updated: 1_000_000,
            });
        }
    }
    hurt(&mut h, target, 100);
    assert!(!dead(&h, target));
    assert_eq!(phase(&h, far, quest), 2, "come near after the first hurt");
    assert_eq!(
        phase(&h, near, quest),
        2,
        "at the phase after the first hurt"
    );
}
