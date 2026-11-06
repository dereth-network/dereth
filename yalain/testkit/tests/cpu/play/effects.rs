//! Heals and buffs and the effects they put on players: instant heals, records that run out on
//! their own clocks, speed, changes over time, effects kept across sessions, and the effects a
//! quest step gives.

use auberean_protocol::{queue, ServerMessage};
use auberean_script::args::Arg;
use yalain_testkit::play::server_call;
use yalain_testkit::{Harness, COURAGE_EFFECT, DASH, DASH_EFFECT, EASE_SUFFERING, INNER_CALM};
use yalain_world::calls::{self, Node, Value};
use yalain_world::player::quality;
use yalain_world::skill_use::function;
use yalain_world::{effects, quals};

const SWORD: u32 = 0x4700_051C;
const ADD_EFFECT: u32 = 0x8208_0017;
const REMOVE_EFFECT: u32 = 0x8208_0016;
const SKILL_INFO: u32 = 0x8214_000D;
const ATTACK_STATE: u32 = 0x8204_001B;
const SHOW_TEXT: u32 = 0x8205_0011;
const QUEST_UPDATE: u32 = 0x8213_0024;

/// A player in the world with the scene drawn and a sword in hand (a heal needs something in the
/// right hand); returns the client and the player.
fn armed() -> (Harness, usize, u64) {
    let mut h = Harness::new();
    let (b, me) = h.player_in_world("one", "Elara");
    h.bots[b].send(
        true,
        queue::WEENIE,
        &auberean_protocol::ClientMessage::SceneRenderingComplete(auberean_protocol::login::Empty),
    );
    h.run(5);
    h.arm(b, me, SWORD);
    // Vigor enough for a heal's cost (a fresh human's 100 is not).
    set_vital(&mut h, me, quality::VIGOR_MAX, 400);
    set_vital(&mut h, me, quality::VIGOR, 400);
    (h, b, me)
}

fn set_vital(h: &mut Harness, me: u64, q: u32, v: i32) {
    let p = h
        .server
        .players
        .values_mut()
        .find(|p| p.character == me)
        .expect("in the world");
    quals::set_int(&mut p.qualities, q, v);
}

fn use_skill(h: &mut Harness, b: usize, id: u32, skill: u32, target: u64) -> usize {
    let from = h.bots[b].inbox.len();
    h.bots[b].send(
        true,
        queue::WEENIE,
        &server_call(
            function::USE,
            &[Arg::Int(id), Arg::Int(skill), Arg::LongInt(target)],
        ),
    );
    from
}

/// Run the world for so many seconds.
fn wait(h: &mut Harness, seconds: f64) {
    let t = h.now.0 + seconds;
    while h.now.0 < t {
        h.step();
    }
}

/// Run the world for so many seconds, a second a step.
fn wait_coarse(h: &mut Harness, seconds: f64) {
    let t = h.now.0 + seconds;
    while h.now.0 < t {
        h.step_by(1.0);
    }
}

fn calls_of(h: &Harness, b: usize, from: usize) -> Vec<(u32, Vec<Value>)> {
    h.world_messages(b, from)
        .iter()
        .filter_map(|m| match m {
            ServerMessage::PlayerCall(c) => Some((
                c.function.0,
                c.args
                    .as_ref()
                    .and_then(|a| calls::values(&a.0).ok())
                    .unwrap_or_default(),
            )),
            _ => None,
        })
        .collect()
}

/// The records an effect's call carries: its id, saved state, timeout, spellcraft, flags.
fn added(h: &Harness, b: usize, from: usize) -> Vec<(u32, u32, f32, f32, u32)> {
    calls_of(h, b, from)
        .into_iter()
        .filter(|(f, _)| *f == ADD_EFFECT)
        .filter_map(|(_, v)| {
            let eid = v.iter().find_map(|x| match x {
                Value::Int(e) => Some(*e),
                _ => None,
            })?;
            let g = v.iter().find_map(|x| match x {
                Value::Graph(g) => Some(g),
                _ => None,
            })?;
            let Some(Node::Script { slots, .. }) = g.first().map(|(_, n)| n) else {
                return None;
            };
            let w = |i: usize| slots.get(i).map_or(0, |(_, x)| *x);
            Some((eid, 0, f32::from_bits(w(6)), f32::from_bits(w(8)), w(20)))
        })
        .collect()
}

fn log_off(h: &mut Harness, b: usize, id: u64) {
    use auberean_protocol::login::CharacterRef;
    use auberean_protocol::ClientMessage;
    let before = h.bots[b].inbox.len();
    h.bots[b].send(
        false,
        queue::LOGON,
        &ClientMessage::LogOff(CharacterRef { character: id }),
    );
    assert!(h.run_until(400, |h| h.bots[b].inbox[before..]
        .iter()
        .any(|r| matches!(r.message(), Some(ServerMessage::CharacterList(_))))));
}

fn health_sent(h: &Harness, b: usize, from: usize) -> Vec<i32> {
    h.world_messages(b, from)
        .iter()
        .filter_map(|m| match m {
            ServerMessage::UpdateInt(q) if q.quality == quality::HEALTH => Some(q.value),
            _ => None,
        })
        .collect()
}

/// Ease Suffering aimed at nothing heals its user: the player attacking, in its weapon's mode, not
/// turned; its story of the heal on itself (the hooks that play and its hit, no change of
/// health); at the skill's hook the healed one tells the heal (25 at a rating of 1), then its
/// health; the skill's information, not attacking, and at peace, with no movement order at all.
/// Behaviour: server.effects.heal-at-the-hook
#[test]
fn a_heal_aimed_at_nothing_heals_its_user_at_the_skills_hook() {
    let (mut h, b, me) = armed();
    set_vital(&mut h, me, quality::HEALTH, 50);
    let from = use_skill(&mut h, b, 3, EASE_SUFFERING, 0);
    h.run(60);
    let msgs = h.world_messages(b, from);
    assert!(
        !msgs.iter().any(|m| matches!(m, ServerMessage::MoveTo(_))),
        "no movement order for a use on itself"
    );
    let stories: Vec<_> = msgs
        .iter()
        .filter_map(|m| match m {
            ServerMessage::DoStory(e) if e.sender.id == me => Some(e.action.clone()),
            _ => None,
        })
        .collect();
    assert_eq!(stories.len(), 2, "the use and the heal");
    let used = &stories[0];
    assert_eq!(
        (used.bits, used.skill, used.attack_context),
        (0x1E, EASE_SUFFERING, 3)
    );
    assert_eq!(used.attacker_behavior.behavior, 0x4100_00C2);
    assert_eq!(
        used.hooks.iter().map(|h| h.bits).collect::<Vec<_>>(),
        [0x2, 0x180, 0x781]
    );
    assert_eq!(
        used.hooks[2].numbers[3], 0,
        "the use's story changes no health"
    );
    let healed = &stories[1];
    assert_eq!((healed.bits, healed.attacker), (0x2, me));
    assert_eq!(
        (
            healed.hooks[0].bits,
            healed.hooks[0].target,
            healed.hooks[0].numbers[3]
        ),
        (0x981, me, 25)
    );
    assert_eq!(health_sent(&h, b, from), [75]);
    let calls: Vec<u32> = calls_of(&h, b, from).iter().map(|(f, _)| *f).collect();
    assert!(calls.contains(&SKILL_INFO));
    assert_eq!(calls.last(), Some(&ATTACK_STATE));
    assert!(msgs
        .iter()
        .any(|m| matches!(m, ServerMessage::DoMode(e) if e.action.0 == 0x4000_0001)));
}

/// Inner Calm heals vigor alone: after its power-up its story, then the player's vigor (not past
/// its greatest), and no story of a heal.
/// Behaviour: server.effects.heal-at-the-hook
#[test]
fn a_vigor_heal_sends_the_vigor_alone() {
    let (mut h, b, me) = armed();
    set_vital(&mut h, me, quality::VIGOR, 390);
    let from = use_skill(&mut h, b, 4, INNER_CALM, me);
    h.run(80);
    let msgs = h.world_messages(b, from);
    let vigor: Vec<i32> = msgs
        .iter()
        .filter_map(|m| match m {
            ServerMessage::UpdateInt(q) if q.quality == quality::VIGOR => Some(q.value),
            _ => None,
        })
        .collect();
    assert_eq!(
        vigor.last(),
        Some(&400),
        "healed to its greatest: {vigor:?}"
    );
    assert!(
        !msgs.iter().any(|m| matches!(m, ServerMessage::DoStory(e) if e.action.hooks.iter().any(|k| k.bits == 0x981))),
        "a vigor heal tells no story of it"
    );
}

/// Dash puts a record on its user at its hook: its own id, its timeout and spellcraft at the
/// skill's rating (30 s at 1), timed (flags 66); the player's speed and acceleration scales sent
/// (1.08, 1.05), its appearance shown. Thirty seconds on the record runs out: taken off, the
/// scales back to 1, the appearance stopped.
/// Behaviour: server.effects.buff-record-runs-out
#[test]
fn a_buff_is_a_record_that_runs_out_on_its_own_clock() {
    let (mut h, b, me) = armed();
    let from = use_skill(&mut h, b, 5, DASH, me);
    h.run(60);
    let rec = added(&h, b, from);
    assert_eq!(rec.len(), 1, "one record");
    let (eid, _, timeout, spellcraft, flags) = rec[0];
    assert_eq!((timeout, spellcraft, flags), (30.0, 1.0, 66));
    let msgs = h.world_messages(b, from);
    let scales: Vec<f32> = msgs
        .iter()
        .filter_map(|m| match m {
            ServerMessage::VelocityScale(e) | ServerMessage::AccelerationScale(e)
                if e.sender.id == me =>
            {
                Some(e.action.0)
            }
            _ => None,
        })
        .collect();
    assert_eq!(scales, [1.08, 1.05]);
    assert!(msgs
        .iter()
        .any(|m| matches!(m, ServerMessage::DoFx(e) if e.action.fx == 0x4100_04CB)));
    let later = h.bots[b].inbox.len();
    wait(&mut h, 31.0);
    let gone = calls_of(&h, b, later);
    assert!(
        gone.contains(&(REMOVE_EFFECT, vec![Value::Int(eid)])),
        "{gone:x?}"
    );
    let msgs = h.world_messages(b, later);
    assert!(msgs
        .iter()
        .any(|m| matches!(m, ServerMessage::VelocityScale(e) if e.action.0 == 1.0)));
    assert!(msgs
        .iter()
        .any(|m| matches!(m, ServerMessage::StopFx(e) if e.action.0 == 0x4100_04CB)));
}

/// The same effect put on twice is two records: the second does not replace or refresh the first,
/// and each runs out at its own cast time plus its timeout.
/// Behaviour: server.effects.records-stack
#[test]
fn the_same_effect_twice_is_two_records_each_running_out_on_its_own() {
    let (mut h, b, me) = armed();
    let world = *h
        .server
        .players
        .iter()
        .find(|(_, p)| p.character == me)
        .expect("in the world")
        .0;
    let cast = effects::Cast {
        caster: me,
        spellcraft: 1.0,
        pk: true,
        ..effects::Cast::default()
    };
    let from = h.bots[b].inbox.len();
    let clock = h.clock();
    effects::apply(&mut h.server, &mut h.net, world, DASH_EFFECT, cast, clock);
    wait(&mut h, 10.0);
    let clock = h.clock();
    effects::apply(&mut h.server, &mut h.net, world, DASH_EFFECT, cast, clock);
    h.run(2);
    let ids: Vec<u32> = added(&h, b, from).iter().map(|r| r.0).collect();
    assert_eq!(ids.len(), 2);
    assert_eq!(ids[1], ids[0] + 1, "ids count on");
    let mid = h.bots[b].inbox.len();
    wait(&mut h, 21.0);
    let first = calls_of(&h, b, mid);
    assert!(first.contains(&(REMOVE_EFFECT, vec![Value::Int(ids[0])])));
    assert!(
        !first.contains(&(REMOVE_EFFECT, vec![Value::Int(ids[1])])),
        "the second runs on"
    );
    let last = h.bots[b].inbox.len();
    wait(&mut h, 10.0);
    assert!(calls_of(&h, b, last).contains(&(REMOVE_EFFECT, vec![Value::Int(ids[1])])));
}

/// A player is told its speed and acceleration scales whenever a record that changes them comes
/// or goes, changed or not: the same speed effect put on twice, then each running out, sends both
/// scales four times, to the player and to those who see it.
/// Behaviour: server.effects.speed-told-at-each-speed-record
#[test]
fn speed_is_told_at_every_speed_records_coming_and_going_even_unchanged() {
    let (mut h, b, me) = armed();
    let (o, _) = h.player_in_world("two", "Tamsin");
    h.run(10);
    let world = *h
        .server
        .players
        .iter()
        .find(|(_, p)| p.character == me)
        .expect("in the world")
        .0;
    let cast = effects::Cast {
        caster: me,
        spellcraft: 1.0,
        pk: true,
        ..effects::Cast::default()
    };
    let from = (h.bots[b].inbox.len(), h.bots[o].inbox.len());
    let clock = h.clock();
    effects::apply(&mut h.server, &mut h.net, world, DASH_EFFECT, cast, clock);
    wait(&mut h, 10.0);
    let clock = h.clock();
    effects::apply(&mut h.server, &mut h.net, world, DASH_EFFECT, cast, clock);
    wait(&mut h, 35.0);
    for (bot, from) in [(b, from.0), (o, from.1)] {
        let msgs = h.world_messages(bot, from);
        let count = |velocity: bool| {
            msgs.iter()
                .filter(|m| match m {
                    ServerMessage::VelocityScale(e) => velocity && e.sender.id == me,
                    ServerMessage::AccelerationScale(e) => !velocity && e.sender.id == me,
                    _ => false,
                })
                .count()
        };
        assert_eq!(count(true), 4, "the speed at each coming and going");
        assert_eq!(count(false), 4, "the acceleration at each coming and going");
    }
}

/// An effect still running when the player leaves comes back in its next session start, in the
/// registry with the id counter, and runs out on the game's clock; one that ran out while it was
/// away does not come back.
/// Behaviour: server.effects.kept-across-sessions
#[test]
fn an_effect_still_running_comes_back_in_the_session_start_and_runs_out_on_the_games_clock() {
    let (mut h, b, me) = armed();
    let world = *h
        .server
        .players
        .iter()
        .find(|(_, p)| p.character == me)
        .expect("in the world")
        .0;
    let clock = h.clock();
    let cast = effects::Cast {
        caster: me,
        spellcraft: 1.0,
        ..effects::Cast::default()
    };
    effects::apply(
        &mut h.server,
        &mut h.net,
        world,
        COURAGE_EFFECT,
        cast,
        clock,
    );
    h.run(2);
    let eid = h
        .server
        .players
        .get(&world)
        .and_then(|p| p.play.effects.first())
        .map(|e| e.eid)
        .expect("a record");
    log_off(&mut h, b, me);
    wait(&mut h, 100.0);
    let from = h.bots[b].inbox.len();
    h.enter_world(b, "one", me);
    let start = calls_of(&h, b, from)
        .into_iter()
        .find(|(f, _)| *f == 0x8212_0027)
        .expect("the session start");
    let records: Vec<u32> = start
        .1
        .iter()
        .filter_map(|v| match v {
            Value::Graph(g) => Some(g),
            _ => None,
        })
        .flat_map(|g| g.iter())
        .filter_map(|(_, n)| match n {
            Node::Script { package, slots } if *package == 0x14E => slots.get(23).map(|(_, x)| *x),
            _ => None,
        })
        .collect();
    assert_eq!(records, [eid], "the record carried in the registry");
    // Away long enough, it runs out on the way.
    log_off(&mut h, b, me);
    wait_coarse(&mut h, 7300.0);
    let from = h.bots[b].inbox.len();
    h.enter_world(b, "one", me);
    let start = calls_of(&h, b, from)
        .into_iter()
        .find(|(f, _)| *f == 0x8212_0027)
        .expect("the session start");
    let any = start.1.iter().any(|v| match v {
        Value::Graph(g) => g
            .iter()
            .any(|(_, n)| matches!(n, Node::Script { package, .. } if *package == 0x14E)),
        _ => false,
    });
    assert!(!any, "a record that ran out while away is gone");
}

/// A change of health over time heals every few seconds by its rate times the seconds since its
/// last change, each told by the healed one before its health.
/// Behaviour: server.effects.over-time
#[test]
fn a_change_over_time_heals_every_few_seconds() {
    let (mut h, b, me) = armed();
    set_vital(&mut h, me, quality::HEALTH, 10);
    let world = *h
        .server
        .players
        .iter()
        .find(|(_, p)| p.character == me)
        .expect("in the world")
        .0;
    let from = h.bots[b].inbox.len();
    let clock = h.clock();
    let cast = effects::Cast {
        caster: me,
        spellcraft: 1.0,
        ..effects::Cast::default()
    };
    effects::apply(
        &mut h.server,
        &mut h.net,
        world,
        yalain_testkit::TEST_OVER_TIME,
        cast,
        clock,
    );
    wait(&mut h, 20.0);
    let healed = health_sent(&h, b, from);
    assert!(healed.len() >= 2, "at least two ticks in 20 s: {healed:?}");
    assert!(healed.windows(2).all(|w| w[1] > w[0]), "{healed:?}");
    let msgs = h.world_messages(b, from);
    let told: Vec<i32> = msgs
        .iter()
        .filter_map(|m| match m {
            ServerMessage::DoStory(e) if e.sender.id == me => e
                .action
                .hooks
                .first()
                .filter(|k| k.bits == 0x981)
                .map(|k| k.numbers[3]),
            _ => None,
        })
        .collect();
    assert!(
        told.len() >= 2,
        "each change told by the healed one: {told:?}"
    );
    // Three a second at a rating of 1, over waits of 3.5 to 7.5 s (the first within 5.6 s).
    assert!(told.iter().all(|a| (1..=23).contains(a)), "{told:?}");
}

/// A quest step's effects are put on the player after its line and gifts, then its text, then its
/// update: a blessing's record (cast by the player itself at spellcraft 1), "... casts ... on
/// you." and the quest moved on.
/// Behaviour: server.effects.quest-rewards
#[test]
fn a_quest_steps_effects_come_after_its_gifts_and_before_its_update() {
    use yalain_content::quests::{QuestDef, Step, Trigger, Update};
    let (mut h, b, me) = armed();
    let quest = 0x4000_0016;
    h.server.quests.quests.push(QuestDef {
        id: quest,
        name: (1, 2),
        description: (3, 4),
        icon: 0,
        play_fx: false,
        journals: vec![(5, 6); 5],
        challenge: 4,
        duration: 0.0,
        timed_phase: 0,
        retry: 0.0,
        xp: 0,
        cancellable: true,
        placed: "made up for the test".into(),
        hidden: false,
        effects: Vec::new(),
        subquests: Vec::new(),
        min_level: 0,
        fails_on_death: false,
        once: false,
        fellows: yalain_content::quests::Fellows::default(),
        last_waits: false,
        hero_only: false,
        pay: yalain_content::quests::CompletionPay::default(),
        required: None,
    });
    let step = Step {
        quest,
        needs: 0,
        trigger: Trigger::Use(0x4700_31ED),
        tell: None,
        update: Update::Bestow,
        xp: 0,
        gives: Vec::new(),
        counts: Vec::new(),
        effects: vec![COURAGE_EFFECT],
        text: Some((0x912E_FF1D, 0x2500_10E8)),
        placed: "made up for the test".into(),
        requires: Vec::new(),
        takes: Vec::new(),
        races: Vec::new(),
        effects_last: false,
        wants: None,
        fellows: false,
        levels: None,
        xp_share: 0.0,
        to: None,
    };
    let world = *h
        .server
        .players
        .iter()
        .find(|(_, p)| p.character == me)
        .expect("in the world")
        .0;
    let from = h.bots[b].inbox.len();
    let clock = h.clock();
    yalain_world::quests::run(&mut h.server, &mut h.net, world, &step, None, clock);
    h.run(2);
    let calls: Vec<u32> = calls_of(&h, b, from).iter().map(|(f, _)| *f).collect();
    let at = |f: u32| calls.iter().position(|x| *x == f);
    assert!(
        at(ADD_EFFECT) < at(SHOW_TEXT) && at(SHOW_TEXT) < at(QUEST_UPDATE),
        "{calls:x?}"
    );
    let rec = added(&h, b, from);
    assert_eq!((rec[0].2, rec[0].3, rec[0].4), (7200.0, 1.0, 66));
    let p = h.server.players.get(&world).expect("in the world");
    assert_eq!(
        effects::max_vital(&h.server, p, quality::HEALTH_MAX),
        130,
        "the blessing raises the greatest health by 30"
    );
}

/// A record that changes the player's jump tells its jump scale to those who see the player as it
/// comes (1.6) and again as it goes (1.0).
/// Behaviour: server.effects.jump-scale-told
#[test]
fn a_record_changing_the_jump_tells_the_jump_scale_as_it_comes_and_goes() {
    use yalain_world::effect_defs::{EffectDef, Kind, Scale};
    const WILD_JUMP: u32 = 0x6F00_2125;
    let (mut h, b, me) = armed();
    let mut d: EffectDef = h
        .server
        .effect_table
        .get(DASH_EFFECT)
        .cloned()
        .expect("dash");
    d.state = WILD_JUMP;
    d.fx = 0;
    d.duration = Scale(vec![(0.0, 300.0)]);
    d.kind = Kind::Numeric {
        flags: 0xA,
        stats: vec![yalain_world::effects::speed::JUMP],
        magnitude: Scale(vec![(0.0, 1.6)]),
    };
    h.server.effect_table.effects.insert(WILD_JUMP, d);
    let world = *h.server.players.keys().next().expect("in the world");
    let from = h.bots[b].inbox.len();
    let clock = h.clock();
    let cast = effects::Cast {
        caster: me,
        spellcraft: 1.0,
        ..effects::Cast::default()
    };
    effects::apply(&mut h.server, &mut h.net, world, WILD_JUMP, cast, clock);
    h.run(10);
    let jumps = |h: &Harness, from: usize| -> Vec<f32> {
        h.world_messages(b, from)
            .iter()
            .filter_map(|m| match m {
                ServerMessage::JumpScale(e) if e.sender.id == me => Some(e.action.0),
                _ => None,
            })
            .collect()
    };
    assert_eq!(jumps(&h, from), [1.6]);
    let later = h.bots[b].inbox.len();
    wait_coarse(&mut h, 301.0);
    h.run(20);
    assert_eq!(jumps(&h, later), [1.0], "{:x?}", calls_of(&h, b, later));
}
