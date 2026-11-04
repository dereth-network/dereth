//! What a creature's death does to the quests its template's effects name: for the one credited
//! with the kill, for its fellows as each quest says, and for no one else. Fixture: the test
//! world's beetles, each given effects by name, and quests of ten phases.

use auberean_script::args::Arg;
use auberean_script::native_object::{native_type, NativeObject};
use yalain_content::quests::{Fellows, QuestDef, Step, Trigger, Update};
use yalain_testkit::Harness;
use yalain_world::calls::Value;
use yalain_world::combat;
use yalain_world::fellowship::function;
use yalain_world::quests::{effect_named, phase_of, refused, status, update, Move, QuestEffect};
use yalain_world::state::QuestState;

const UPDATE_QUEST: u32 = 0x8213_0024;
const QUEST: u32 = 0x4000_0123;
const NAME: &str = "QuestHunt";
const SEEN: i64 = 1_000_000;

fn string_arg(s: &str) -> Arg {
    let mut b = yalain_world::graph::Builder::new();
    #[allow(clippy::cast_possible_truncation)]
    b.native(
        native_type::WP_STRING as u16,
        NativeObject::Text(s.encode_utf16().collect()),
    );
    Arg::Object(b.graph())
}

pub(super) fn quest(phases: u32) -> QuestDef {
    QuestDef {
        id: QUEST,
        name: (0xA860_7CEC, 0x2500_17EE),
        description: (0x6C45_9364, 0x2500_17EE),
        icon: 0x4100_034B,
        play_fx: true,
        journals: (1..=phases).map(|n| (n, 0x2500_17EE)).collect(),
        challenge: 10,
        duration: 0.0,
        timed_phase: 0,
        retry: 0.0,
        xp: 0,
        cancellable: false,
        placed: "authored for a test".into(),
        hidden: false,
        effects: Vec::new(),
        subquests: Vec::new(),
        min_level: 0,
        fails_on_death: false,
        once: false,
        fellows: Fellows::default(),
        last_waits: false,
        hero_only: false,
        pay: yalain_content::quests::CompletionPay::default(),
        required: None,
    }
}

/// A world with this quest, and a player.
fn world(q: QuestDef) -> (Harness, usize, u64) {
    let mut h = Harness::new();
    h.server.quests.quests.push(q);
    h.server.quest_ids.insert(NAME.into(), QUEST);
    h.add_creatures(1);
    let (b, me) = h.player_in_world("one", "Kestrel");
    h.run(20);
    (h, b, me)
}

/// A new beetle naming these effects, hurt to death by this character.
fn kill(h: &mut Harness, by: u64, effects: &[&str]) -> u32 {
    h.add_creatures(1);
    h.run(2);
    let dead = h.server.things.by_id.keys().copied().max();
    let beetle = h
        .server
        .things
        .by_id
        .values()
        .filter(|t| {
            t.creature
                .as_ref()
                .is_some_and(|c| c.mode != yalain_world::things::Mode::Dead)
        })
        .map(|t| t.id)
        .max()
        .or(dead)
        .expect("a beetle");
    let t = h.server.things.get_mut(beetle).expect("the beetle");
    t.health = 10;
    t.max_health = 10;
    let template = t.template;
    let c = t.creature.as_mut().expect("a creature");
    c.stats.attacks_first = false;
    c.stats.death_effects = effects.iter().map(ToString::to_string).collect();
    let clock = h.clock();
    combat::hurt_creature(&mut h.server, &mut h.net, beetle, by, 50, clock);
    h.run(2);
    template
}

fn updates(h: &Harness, b: usize, from: usize) -> Vec<(u32, u32)> {
    h.calls(b, from)
        .into_iter()
        .filter(|(f, _)| *f == UPDATE_QUEST)
        .filter_map(|(_, v)| match (v.first(), v.last()) {
            (Some(Value::Int(result)), Some(Value::Int(kind))) => Some((*result, *kind)),
            _ => None,
        })
        .collect()
}

fn phase(h: &Harness, who: u64) -> u32 {
    let p = h
        .server
        .players
        .values()
        .find(|p| p.character == who)
        .expect("in the world");
    phase_of(&p.play.quests, QUEST)
}

fn holds(h: &mut Harness, who: u64, status: u32, phase: u32) {
    let p = h
        .server
        .players
        .values_mut()
        .find(|p| p.character == who)
        .expect("in the world");
    p.play.quests.retain(|q| q.quest != QUEST);
    p.play.quests.push(QuestState {
        quest: QUEST,
        status,
        phase,
        started: SEEN,
        updated: SEEN,
    });
}

fn state(h: &Harness, who: u64) -> Option<(u32, u32)> {
    let p = h.server.players.values().find(|p| p.character == who)?;
    p.play
        .quests
        .iter()
        .find(|q| q.quest == QUEST)
        .map(|q| (q.status, q.phase))
}

fn set_level(h: &mut Harness, who: u64, level: i32) {
    let p = h
        .server
        .players
        .values_mut()
        .find(|p| p.character == who)
        .expect("in the world");
    yalain_world::quals::set_int(&mut p.qualities, 302, level);
}

/// A fellowship of two players standing `apart` meters from each other.
fn fellows(h: &mut Harness, (a, aid): (usize, u64), apart: f32) -> (usize, u64) {
    let (b, bid) = h.player_in_world("two", "Haskel");
    h.run(20);
    let at = h
        .server
        .players
        .values()
        .find(|p| p.character == aid)
        .expect("in the world")
        .position;
    let p = h
        .server
        .players
        .values_mut()
        .find(|p| p.character == bid)
        .expect("in the world");
    p.position = at;
    h.ask(
        a,
        function::CREATE,
        &[Arg::Int(1), Arg::Int(0), string_arg("Band")],
        10,
    );
    h.ask(a, function::RECRUIT, &[Arg::LongInt(bid)], 10);
    h.ask(b, function::ACCEPT, &[Arg::LongInt(aid)], 10);
    let p = h
        .server
        .players
        .values_mut()
        .find(|p| p.character == bid)
        .expect("in the world");
    p.position.origin[0] += apart;
    (b, bid)
}

const INCREMENT: &str = "Eff_IncrementQuestHunt";

/// **A kill moves a quest under way on a phase, each kill one, the last phase's completing it**, and
/// its killer is told each move as an advance to the phase, then the completion.
/// Behaviour: server.quests.a-kill-moves-a-quest-under-way-on
#[test]
fn each_kill_moves_a_quest_under_way_on_a_phase_and_the_last_completes_it() {
    let (mut h, b, me) = world(quest(3));
    holds(&mut h, me, status::UNDERWAY, 1);
    let from = h.bots[b].inbox.len();
    kill(&mut h, me, &[INCREMENT]);
    assert_eq!(phase(&h, me), 2);
    kill(&mut h, me, &[INCREMENT]);
    assert_eq!(phase(&h, me), 3);
    kill(&mut h, me, &[INCREMENT]);
    assert_eq!(state(&h, me).map(|s| s.0), Some(status::COMPLETED));
    assert_eq!(
        updates(&h, b, from),
        vec![
            (0, update::ADVANCE | 2),
            (0, update::ADVANCE | 3),
            (0, update::COMPLETE)
        ]
    );
}

/// **A kill's increment moves nothing the killer does not have under way**: never held, done or
/// failed, nothing changes and nothing is told.
/// Behaviour: server.quests.a-kill-moves-no-quest-not-under-way
#[test]
fn a_kill_moves_on_no_quest_its_killer_does_not_have_under_way() {
    let (mut h, b, me) = world(quest(3));
    let from = h.bots[b].inbox.len();
    kill(
        &mut h,
        me,
        &[
            INCREMENT,
            "Eff_AdvanceQuestHuntPhase2",
            "Eff_CompleteQuestHunt",
        ],
    );
    assert_eq!(state(&h, me), None);
    holds(&mut h, me, status::COMPLETED, 0);
    kill(
        &mut h,
        me,
        &[
            INCREMENT,
            "Eff_AdvanceQuestHuntPhase2",
            "Eff_CompleteQuestHunt",
        ],
    );
    assert_eq!(state(&h, me), Some((status::COMPLETED, 0)));
    assert!(updates(&h, b, from).is_empty());
}

/// **A last phase that waits for something else stays where it is**: a phase that reads "return to"
/// someone, however many more are killed.
/// Behaviour: server.quests.a-last-phase-that-waits-holds
#[test]
fn a_last_phase_that_waits_for_something_else_is_not_ended_by_a_kill() {
    let (mut h, b, me) = world(QuestDef {
        last_waits: true,
        hero_only: false,
        pay: yalain_content::quests::CompletionPay::default(),
        required: None,
        ..quest(3)
    });
    holds(&mut h, me, status::UNDERWAY, 2);
    let from = h.bots[b].inbox.len();
    kill(&mut h, me, &[INCREMENT]);
    kill(&mut h, me, &[INCREMENT]);
    assert_eq!(phase(&h, me), 3);
    assert_eq!(updates(&h, b, from), vec![(0, update::ADVANCE | 3)]);
}

/// **An advance moves a quest only from the phase before; a completion only from the last.**
/// Behaviour: server.quests.a-kill-advances-from-the-phase-before
#[test]
fn an_advance_moves_from_the_phase_before_and_a_completion_from_the_last_phase() {
    let (mut h, b, me) = world(quest(4));
    holds(&mut h, me, status::UNDERWAY, 1);
    let from = h.bots[b].inbox.len();
    kill(&mut h, me, &["Eff_AdvanceQuestHuntPhase3"]);
    assert_eq!(phase(&h, me), 1, "not from two phases back");
    kill(&mut h, me, &["Eff_CompleteQuestHunt"]);
    assert_eq!(phase(&h, me), 1, "not from a phase before the last");
    kill(&mut h, me, &["Eff_AdvanceQuestHuntPhase2"]);
    assert_eq!(phase(&h, me), 2);
    holds(&mut h, me, status::UNDERWAY, 4);
    kill(&mut h, me, &["Eff_CompleteQuestHunt"]);
    assert_eq!(state(&h, me).map(|s| s.0), Some(status::COMPLETED));
    assert_eq!(
        updates(&h, b, from),
        vec![(0, update::ADVANCE | 2), (0, update::COMPLETE)]
    );
}

/// **A kill gives a quest to its killer, at the first phase, when it is free**: never had, done with
/// its wait over or failed.
/// Behaviour: server.quests.a-kill-gives-a-quest
#[test]
fn a_kill_gives_its_quest_to_a_killer_free_to_take_it() {
    let (mut h, b, me) = world(quest(3));
    let from = h.bots[b].inbox.len();
    kill(&mut h, me, &["Eff_BestowQuestHunt"]);
    assert_eq!(state(&h, me), Some((status::UNDERWAY, 1)));
    assert_eq!(updates(&h, b, from), vec![(0, update::BESTOW)]);
    for s in [status::COMPLETED, status::FAILED] {
        holds(&mut h, me, s, 0);
        kill(&mut h, me, &["Eff_BestowQuestHunt"]);
        assert_eq!(state(&h, me), Some((status::UNDERWAY, 1)));
    }
}

/// **A killer whose giving is refused is told why in a record of nothing**: under way, its wait not
/// over, done once; the quest stays as it was.
/// Behaviour: server.quests.a-kills-refused-giving-is-told
#[test]
fn a_killer_is_told_why_a_kills_giving_was_refused_and_its_quest_stays_as_it_was() {
    let (mut h, b, me) = world(QuestDef {
        retry: 100_000_000.0,
        ..quest(3)
    });
    holds(&mut h, me, status::UNDERWAY, 2);
    let from = h.bots[b].inbox.len();
    kill(&mut h, me, &["Eff_BestowQuestHunt"]);
    assert_eq!(
        updates(&h, b, from),
        vec![(refused::UNDER_WAY, update::BESTOW)]
    );
    assert_eq!(state(&h, me), Some((status::UNDERWAY, 2)));
    let now = h.clock().unix;
    holds(&mut h, me, status::COMPLETED, 0);
    for p in h.server.players.values_mut() {
        for q in &mut p.play.quests {
            q.updated = now;
        }
    }
    let from = h.bots[b].inbox.len();
    kill(&mut h, me, &["Eff_BestowQuestHunt"]);
    assert_eq!(
        updates(&h, b, from),
        vec![(refused::WAITING, update::BESTOW)]
    );
    if let Some(q) = h.server.quests.quests.iter_mut().find(|q| q.id == QUEST) {
        q.once = true;
    }
    let from = h.bots[b].inbox.len();
    kill(&mut h, me, &["Eff_BestowQuestHunt"]);
    assert_eq!(updates(&h, b, from), vec![(refused::DONE, update::BESTOW)]);
    assert_eq!(state(&h, me), Some((status::COMPLETED, 0)));
}

/// **A quest that shares reaches the killer's fellows, those it limits only near in level and
/// near by; a quest that does not stays the killer's.** A fellow is moved as the killer is, and
/// told nothing when nothing moved.
/// Behaviour: server.quests.a-kill-reaches-fellows-as-its-quest-says
#[test]
fn a_kill_moves_a_fellows_quest_only_as_the_quest_says() {
    let limited = Fellows {
        shared: true,
        near_level: true,
        near_by: true,
    };
    // Shared: both move; a fellow without it is not given anything to move.
    let (mut h, a, me) = world(QuestDef {
        fellows: limited,
        ..quest(5)
    });
    let (b, them) = fellows(&mut h, (a, me), 10.0);
    holds(&mut h, me, status::UNDERWAY, 1);
    holds(&mut h, them, status::UNDERWAY, 3);
    kill(&mut h, me, &[INCREMENT]);
    assert_eq!((phase(&h, me), phase(&h, them)), (2, 4));
    // Ten levels apart is near; eleven is not.
    set_level(&mut h, me, 20);
    set_level(&mut h, them, 30);
    kill(&mut h, me, &[INCREMENT]);
    assert_eq!((phase(&h, me), phase(&h, them)), (3, 5));
    holds(&mut h, them, status::UNDERWAY, 1);
    set_level(&mut h, them, 31);
    kill(&mut h, me, &[INCREMENT]);
    assert_eq!((phase(&h, me), phase(&h, them)), (4, 1));
    // Too far away.
    set_level(&mut h, them, 20);
    let p = h
        .server
        .players
        .values_mut()
        .find(|p| p.character == them)
        .expect("in the world");
    p.position.origin[0] += 500.0;
    kill(&mut h, me, &[INCREMENT]);
    assert_eq!((phase(&h, me), phase(&h, them)), (5, 1));
    // A fellow is never told a refused giving; a free one is given it.
    let from = h.bots[b].inbox.len();
    let p = h
        .server
        .players
        .values_mut()
        .find(|p| p.character == them)
        .expect("in the world");
    p.position.origin[0] -= 500.0;
    kill(&mut h, me, &["Eff_BestowQuestHunt"]);
    assert_eq!(updates(&h, b, from), Vec::<(u32, u32)>::new());
    let p = h
        .server
        .players
        .values_mut()
        .find(|p| p.character == them)
        .expect("in the world");
    p.play.quests.clear();
    kill(&mut h, me, &["Eff_BestowQuestHunt"]);
    assert_eq!(state(&h, them), Some((status::UNDERWAY, 1)));
    assert_eq!(updates(&h, b, from), vec![(0, update::BESTOW)]);

    // Not shared: only the killer moves. Limits that are not named limit nothing.
    let (mut h, a, me) = world(quest(5));
    let (_, them) = fellows(&mut h, (a, me), 10.0);
    holds(&mut h, me, status::UNDERWAY, 1);
    holds(&mut h, them, status::UNDERWAY, 1);
    kill(&mut h, me, &[INCREMENT]);
    assert_eq!((phase(&h, me), phase(&h, them)), (2, 1));
    if let Some(q) = h.server.quests.quests.iter_mut().find(|q| q.id == QUEST) {
        q.fellows = Fellows {
            shared: true,
            ..Fellows::default()
        };
    }
    set_level(&mut h, them, 200);
    let p = h
        .server
        .players
        .values_mut()
        .find(|p| p.character == them)
        .expect("in the world");
    p.position.origin[0] += 500.0;
    kill(&mut h, me, &[INCREMENT]);
    assert_eq!((phase(&h, me), phase(&h, them)), (3, 2));
}

/// **A stranger's kill moves nothing**: a player who is no fellow of the killer holds the quest and
/// stands beside it.
/// Behaviour: server.quests.a-strangers-kill-moves-nothing
#[test]
fn a_kill_moves_nothing_for_a_player_who_is_no_fellow_of_the_killer() {
    let (mut h, _, me) = world(QuestDef {
        fellows: Fellows {
            shared: true,
            ..Fellows::default()
        },
        ..quest(5)
    });
    let (_, them) = h.player_in_world("two", "Haskel");
    h.run(20);
    holds(&mut h, me, status::UNDERWAY, 1);
    holds(&mut h, them, status::UNDERWAY, 1);
    kill(&mut h, me, &[INCREMENT]);
    assert_eq!((phase(&h, me), phase(&h, them)), (2, 1));
}

/// **A step a kill triggers runs, and its quest does not move twice**: a world's own step on the
/// quest takes the place of the effect for the killer.
/// Behaviour: server.quests.a-kill-step-comes-before-its-effect
#[test]
fn a_kills_step_runs_first_and_its_quest_is_not_moved_again_by_the_effect() {
    let (mut h, b, me) = world(quest(5));
    holds(&mut h, me, status::UNDERWAY, 1);
    let template = kill(&mut h, me, &[]);
    assert_eq!(phase(&h, me), 1);
    h.server.quests.steps.push(Step {
        quest: QUEST,
        needs: 1,
        trigger: Trigger::Kill(template),
        tell: None,
        update: Update::Advance(4),
        xp: 0,
        gives: Vec::new(),
        counts: Vec::new(),
        effects: Vec::new(),
        text: None,
        placed: "authored for a test".into(),
        requires: Vec::new(),
        takes: Vec::new(),
        races: Vec::new(),
        effects_last: false,
        wants: None,
        fellows: false,
        to: None,
    });
    let from = h.bots[b].inbox.len();
    kill(&mut h, me, &[INCREMENT]);
    assert_eq!(phase(&h, me), 4);
    assert_eq!(updates(&h, b, from), vec![(0, update::ADVANCE | 4)]);
}

/// **A kill does not give a quest to a killer under its least level**: the killer is told so in the
/// record of nothing; a fellow is told nothing; at that level it is given.
/// Behaviour: server.quests.a-kills-giving-needs-the-least-level
#[test]
fn a_killer_under_a_quests_least_level_is_refused_it_and_told() {
    let (mut h, b, me) = world(QuestDef {
        min_level: 15,
        fellows: Fellows {
            shared: true,
            near_level: false,
            near_by: false,
        },
        ..quest(3)
    });
    let (c, them) = fellows(&mut h, (b, me), 2.0);
    set_level(&mut h, me, 14);
    set_level(&mut h, them, 14);
    let (from, theirs) = (h.bots[b].inbox.len(), h.bots[c].inbox.len());
    kill(&mut h, me, &["Eff_BestowQuestHunt"]);
    assert_eq!(state(&h, me), None);
    assert_eq!(
        updates(&h, b, from),
        vec![(refused::LEVEL_TOO_LOW, update::BESTOW)]
    );
    assert_eq!(state(&h, them), None);
    assert!(updates(&h, c, theirs).is_empty(), "a fellow is not told");
    set_level(&mut h, me, 15);
    let from = h.bots[b].inbox.len();
    kill(&mut h, me, &["Eff_BestowQuestHunt"]);
    assert_eq!(state(&h, me), Some((status::UNDERWAY, 1)));
    assert_eq!(updates(&h, b, from), vec![(0, update::BESTOW)]);
}

/// **A quest for heroes alone is given to a hero**, and to anyone else nothing is given or said; a
/// step giving it holds back the same.
/// Behaviour: server.quests.a-heros-quest-only-for-a-hero
#[test]
fn a_kill_or_a_step_gives_a_heros_quest_only_to_a_hero() {
    let (mut h, b, me) = world(QuestDef {
        hero_only: true,
        pay: yalain_content::quests::CompletionPay::default(),
        ..quest(3)
    });
    let from = h.bots[b].inbox.len();
    kill(&mut h, me, &["Eff_BestowQuestHunt"]);
    assert_eq!(state(&h, me), None);
    assert!(updates(&h, b, from).is_empty(), "nothing said");
    let step = giving_step();
    assert!(!may_be_given(&h, me, &step), "nor by a step");
    let p = h
        .server
        .players
        .values_mut()
        .find(|p| p.character == me)
        .expect("in the world");
    yalain_world::focus::set_hero_kept(&mut p.qualities, true);
    assert!(may_be_given(&h, me, &step));
    kill(&mut h, me, &["Eff_BestowQuestHunt"]);
    assert_eq!(state(&h, me), Some((status::UNDERWAY, 1)));
}

/// **A quest that requires another is given only with that quest at the status named**: never had,
/// or at another status, nothing is given or said; a step giving it holds back the same.
/// Behaviour: server.quests.a-required-quest-holds-back-a-giving
#[test]
fn a_kill_or_a_step_gives_a_quest_only_with_the_quest_it_requires_at_its_status() {
    const OTHER: u32 = 0x4000_0124;
    let (mut h, b, me) = world(QuestDef {
        required: Some((OTHER, status::UNDERWAY)),
        ..quest(3)
    });
    let hold = |h: &mut Harness, st: u32| {
        let p = h
            .server
            .players
            .values_mut()
            .find(|p| p.character == me)
            .expect("in the world");
        p.play.quests.retain(|q| q.quest != OTHER);
        p.play.quests.push(QuestState {
            quest: OTHER,
            status: st,
            phase: 1,
            started: SEEN,
            updated: SEEN,
        });
    };
    let step = giving_step();
    let from = h.bots[b].inbox.len();
    kill(&mut h, me, &["Eff_BestowQuestHunt"]);
    assert!(!may_be_given(&h, me, &step), "never had");
    hold(&mut h, status::COMPLETED);
    kill(&mut h, me, &["Eff_BestowQuestHunt"]);
    assert!(!may_be_given(&h, me, &step), "at another status");
    assert_eq!(state(&h, me), None);
    assert!(updates(&h, b, from).is_empty(), "nothing said");
    hold(&mut h, status::UNDERWAY);
    assert!(may_be_given(&h, me, &step));
    kill(&mut h, me, &["Eff_BestowQuestHunt"]);
    assert_eq!(state(&h, me), Some((status::UNDERWAY, 1)));
}

/// A step that gives the quest.
fn giving_step() -> Step {
    Step {
        quest: QUEST,
        needs: 0,
        trigger: Trigger::Use(0x4700_0001),
        tell: None,
        update: Update::Bestow,
        xp: 0,
        gives: Vec::new(),
        counts: Vec::new(),
        effects: Vec::new(),
        text: None,
        placed: "authored for a test".into(),
        requires: Vec::new(),
        takes: Vec::new(),
        races: Vec::new(),
        effects_last: false,
        wants: None,
        fellows: false,
        to: None,
    }
}

/// May a step run for this character now?
fn may_be_given(h: &Harness, who: u64, st: &Step) -> bool {
    let world = *h
        .server
        .players
        .iter()
        .find(|(_, p)| p.character == who)
        .expect("in the world")
        .0;
    yalain_world::quests::may_run_now(&h.server, world, st, h.clock().unix)
}

/// **An effect's name says what it does to which quest**: the kind, the quest by the enumeration's
/// name (with or without the separator its name has), and for an advance the phase; a name of
/// any other kind, or of no quest, says nothing.
/// Behaviour: server.quests.an-effects-name-names-its-quest
#[test]
fn an_effects_name_says_what_it_does_to_which_quest() {
    let ids = |n: &str| match n {
        "QuestHunt" => Some(1),
        "Quest_KQ5_Soldiers1" => Some(2),
        "QuestHuntPhase9" => Some(3),
        _ => None,
    };
    let of = |n: &str| effect_named(n, ids);
    let e = |quest, act| Some(QuestEffect { quest, act });
    assert_eq!(of("Eff_IncrementQuestHunt"), e(1, Move::Increment));
    assert_eq!(of("Eff_BestowQuestHunt"), e(1, Move::Bestow));
    assert_eq!(of("Eff_CompleteQuestHunt"), e(1, Move::Complete));
    assert_eq!(of("EFF_CompleteQuestHunt"), e(1, Move::Complete));
    assert_eq!(of("Eff_FailQuestHunt"), e(1, Move::Fail));
    assert_eq!(of("Eff_AdvanceQuestHuntPhase7"), e(1, Move::Advance(7)));
    assert_eq!(
        of("Eff_IncrementQuest_KQ5_Soldiers1"),
        e(2, Move::Increment)
    );
    assert_eq!(of("Eff_IncrementQuestHuntPhase9"), e(3, Move::Increment));
    assert_eq!(of("Eff_SpawnHelpers"), None);
    assert_eq!(of("Eff_IncrementQuestNobody"), None);
    assert_eq!(of("Eff_AdvanceQuestHunt"), None);
}

/// An item acquired moves the quest its template names on being acquired, from the phase before and
/// no other; a step on its taking runs first, in the effect's place.
/// Behaviour: server.quests.an-item-acquired-moves-its-quest
#[test]
fn an_item_acquired_moves_the_quest_its_template_names() {
    const ITEM: u32 = 0x4700_FFF1;
    let (mut h, b, me) = world(quest(5));
    let mut props = yalain_content::Props::default();
    props.set(
        "Item_AcquireEffect1",
        yalain_content::Prop::Text("Eff_AdvanceQuestHuntPhase3".into()),
    );
    h.server
        .catalog
        .insert(yalain_testkit::test_template(ITEM, 0x3A1, props));
    let world = *h
        .server
        .players
        .iter()
        .find(|(_, p)| p.character == me)
        .expect("in the world")
        .0;
    let take = |h: &mut Harness| {
        let clock = h.clock();
        yalain_world::quests::took(&mut h.server, &mut h.net, world, ITEM, clock);
        h.run(2);
    };
    take(&mut h);
    assert_eq!(phase(&h, me), 0, "not given by an advance");
    holds(&mut h, me, status::UNDERWAY, 1);
    take(&mut h);
    assert_eq!(phase(&h, me), 1, "not from a phase but the one before");
    holds(&mut h, me, status::UNDERWAY, 2);
    let from = h.bots[b].inbox.len();
    take(&mut h);
    assert_eq!(phase(&h, me), 3);
    assert_eq!(updates(&h, b, from), vec![(0, update::ADVANCE | 3)]);
    holds(&mut h, me, status::UNDERWAY, 1);
    h.server.quests.steps.push(Step {
        quest: QUEST,
        needs: 1,
        trigger: Trigger::Take(ITEM),
        tell: None,
        update: Update::Advance(2),
        xp: 0,
        gives: Vec::new(),
        counts: Vec::new(),
        effects: Vec::new(),
        text: None,
        placed: "authored for a test".into(),
        requires: Vec::new(),
        takes: Vec::new(),
        races: Vec::new(),
        effects_last: false,
        wants: None,
        fellows: false,
        to: None,
    });
    let from = h.bots[b].inbox.len();
    take(&mut h);
    assert_eq!(phase(&h, me), 2, "the step, and not the effect after it");
    assert_eq!(updates(&h, b, from), vec![(0, update::ADVANCE | 2)]);
    // What a person gives at a use comes to hand as well.
    const OTHER: u32 = 0x4000_0124;
    const PERSON: u32 = 0x4700_FFF2;
    h.server.quests.quests.push(QuestDef {
        id: OTHER,
        ..quest(5)
    });
    h.server.quests.steps.push(Step {
        quest: OTHER,
        needs: 0,
        trigger: Trigger::Use(PERSON),
        tell: None,
        update: Update::Bestow,
        xp: 0,
        gives: vec![ITEM],
        counts: Vec::new(),
        effects: Vec::new(),
        text: None,
        placed: "authored for a test".into(),
        requires: Vec::new(),
        takes: Vec::new(),
        races: Vec::new(),
        effects_last: false,
        wants: None,
        fellows: false,
        to: None,
    });
    h.server.quests.steps.retain(|st| st.quest != QUEST);
    holds(&mut h, me, status::UNDERWAY, 2);
    let clock = h.clock();
    yalain_world::quests::triggered(
        &mut h.server,
        &mut h.net,
        world,
        Trigger::Use(PERSON),
        None,
        clock,
    );
    h.run(2);
    assert_eq!(phase(&h, me), 3, "by what the person gave");
}
