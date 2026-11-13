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
        name: yalain_content::Words::Ref(0xA860_7CEC, 0x2500_17EE),
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
        fails_on_logout: false,
        fails_on_teleport: false,
        once: false,
        fellows: Fellows::default(),
        ranges: Vec::new(),
        hero_only: false,
        pay: yalain_content::quests::CompletionPay::default(),
        required: None,
        cancel_wait: None,
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
    let at_one: Vec<(&str, u32)> = effects.iter().map(|e| (*e, 1)).collect();
    kill_naming(h, by, &at_one)
}

/// A new beetle naming these effects, each with its stat, hurt to death by this character.
fn kill_naming(h: &mut Harness, by: u64, effects: &[(&str, u32)]) -> u32 {
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
    c.stats.death_effects = effects
        .iter()
        .map(|(e, n)| ((*e).to_string(), *n))
        .collect();
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
        ranges: vec![1, 1, 2],
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

/// **A kill moves only a phase of the range its death names, and ends the quest from that range's
/// last phase**: of a quest of two kills of one kind then two of another (ranges 1, 1, 2, 2), a kill
/// naming the second range moves nothing during the first two phases, one naming the first nothing
/// during the last two, and the second range's last kill completes it.
/// Behaviour: server.quests.a-kill-moves-only-its-ranges-phases
#[test]
fn a_kill_moves_only_the_phases_of_the_range_its_death_names() {
    let (mut h, b, me) = world(QuestDef {
        ranges: vec![1, 1, 2, 2],
        ..quest(4)
    });
    let (first, second) = ([(INCREMENT, 1)], [(INCREMENT, 2)]);
    holds(&mut h, me, status::UNDERWAY, 1);
    let from = h.bots[b].inbox.len();
    kill_naming(&mut h, me, &second);
    assert_eq!(phase(&h, me), 1, "the second range's kill waits its turn");
    kill_naming(&mut h, me, &first);
    kill_naming(&mut h, me, &first);
    assert_eq!(phase(&h, me), 3);
    kill_naming(&mut h, me, &first);
    assert_eq!(phase(&h, me), 3, "the first range is done");
    kill_naming(&mut h, me, &second);
    assert_eq!(phase(&h, me), 4);
    kill_naming(&mut h, me, &second);
    assert_eq!(state(&h, me).map(|s| s.0), Some(status::COMPLETED));
    assert_eq!(
        updates(&h, b, from),
        vec![
            (0, update::ADVANCE | 2),
            (0, update::ADVANCE | 3),
            (0, update::ADVANCE | 4),
            (0, update::COMPLETE)
        ]
    );
}

/// **A phase of a range no kill names waits for something else**: a quest that sends its player to
/// someone, then counts kills, then sends it back (ranges 0, 1, 0) moves on a kill at the kills'
/// phase alone.
/// Behaviour: server.quests.a-kill-moves-only-its-ranges-phases
#[test]
fn a_kill_moves_no_phase_that_waits_for_a_give_or_a_return() {
    let (mut h, b, me) = world(QuestDef {
        ranges: vec![0, 1, 0],
        ..quest(3)
    });
    holds(&mut h, me, status::UNDERWAY, 1);
    let from = h.bots[b].inbox.len();
    kill(&mut h, me, &[INCREMENT]);
    assert_eq!(phase(&h, me), 1, "the first errand is not a kill's");
    holds(&mut h, me, status::UNDERWAY, 2);
    kill(&mut h, me, &[INCREMENT]);
    kill(&mut h, me, &[INCREMENT]);
    assert_eq!(phase(&h, me), 3, "the return is not a kill's");
    assert_eq!(state(&h, me).map(|s| s.0), Some(status::UNDERWAY));
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
        key: String::new(),
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
        key: String::new(),
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
        key: String::new(),
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
        key: String::new(),
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

/// A step of the test quest for a taking of this item, moving the taker's fellows too.
fn fellows_take_step(item: u32, needs: u32) -> Step {
    Step {
        trigger: Trigger::Take(item),
        to: None,
        fellows: true,
        ..leading_step(0, needs, ARENA)
    }
}

/// **An item acquired moves the taker's fellows' quest as a kill does**: a quest that shares
/// reaches them, those it limits only near by; a fellow is moved as the taker is and told nothing
/// it was refused; a quest that does not share stays the taker's. A step on the taking that moved
/// the fellows leaves them where it put them, an increment too.
/// Behaviour: server.quests.an-item-acquired-reaches-fellows-as-its-quest-says
#[test]
fn an_item_acquired_moves_a_fellows_quest_only_as_the_quest_says() {
    const ITEM: u32 = 0x4700_FFF1;
    const GIVING: u32 = 0x4700_FFF3;
    const COUNTED: u32 = 0x4700_FFF4;
    let (mut h, a, me) = world(QuestDef {
        fellows: Fellows {
            shared: true,
            near_level: false,
            near_by: true,
        },
        ..quest(5)
    });
    for (item, effect) in [
        (ITEM, "Eff_AdvanceQuestHuntPhase3"),
        (GIVING, "Eff_BestowQuestHunt"),
        (COUNTED, INCREMENT),
    ] {
        let mut props = yalain_content::Props::default();
        props.set(
            "Item_AcquireEffect1",
            yalain_content::Prop::Text(effect.into()),
        );
        h.server
            .catalog
            .insert(yalain_testkit::test_template(item, 0x3A1, props));
    }
    let (b, them) = fellows(&mut h, (a, me), 10.0);
    let world = *h
        .server
        .players
        .iter()
        .find(|(_, p)| p.character == me)
        .expect("in the world")
        .0;
    let take = |h: &mut Harness, item: u32| {
        let clock = h.clock();
        yalain_world::quests::took(&mut h.server, &mut h.net, world, item, clock);
        h.run(2);
    };
    // Both from the phase before; a fellow told as the taker is.
    holds(&mut h, me, status::UNDERWAY, 2);
    holds(&mut h, them, status::UNDERWAY, 2);
    let from = h.bots[b].inbox.len();
    take(&mut h, ITEM);
    assert_eq!((phase(&h, me), phase(&h, them)), (3, 3));
    assert_eq!(updates(&h, b, from), vec![(0, update::ADVANCE | 3)]);
    // A fellow at another phase stays there.
    holds(&mut h, me, status::UNDERWAY, 2);
    holds(&mut h, them, status::UNDERWAY, 1);
    take(&mut h, ITEM);
    assert_eq!((phase(&h, me), phase(&h, them)), (3, 1));
    // A fellow is never told a refused giving; a free one is given it.
    let from = h.bots[b].inbox.len();
    take(&mut h, GIVING);
    assert!(updates(&h, b, from).is_empty(), "nothing said");
    let p = h
        .server
        .players
        .values_mut()
        .find(|p| p.character == them)
        .expect("in the world");
    p.play.quests.clear();
    take(&mut h, GIVING);
    assert_eq!(state(&h, them), Some((status::UNDERWAY, 1)));
    assert_eq!(updates(&h, b, from), vec![(0, update::BESTOW)]);
    // Too far away.
    holds(&mut h, me, status::UNDERWAY, 2);
    holds(&mut h, them, status::UNDERWAY, 2);
    let p = h
        .server
        .players
        .values_mut()
        .find(|p| p.character == them)
        .expect("in the world");
    p.position.origin[0] += 500.0;
    take(&mut h, ITEM);
    assert_eq!((phase(&h, me), phase(&h, them)), (3, 2));
    let p = h
        .server
        .players
        .values_mut()
        .find(|p| p.character == them)
        .expect("in the world");
    p.position.origin[0] -= 500.0;

    // A step that moves the fellows too: each moved once, by the step.
    h.server.quests.steps = vec![fellows_take_step(ITEM, 2), fellows_take_step(COUNTED, 2)];
    for item in [ITEM, COUNTED] {
        holds(&mut h, me, status::UNDERWAY, 2);
        holds(&mut h, them, status::UNDERWAY, 2);
        let from = h.bots[b].inbox.len();
        take(&mut h, item);
        assert_eq!((phase(&h, me), phase(&h, them)), (3, 3), "{item:#X}");
        assert_eq!(updates(&h, b, from), vec![(0, update::ADVANCE | 3)]);
    }
    h.server.quests.steps.clear();

    // Not shared: only the taker moves.
    if let Some(q) = h.server.quests.quests.iter_mut().find(|q| q.id == QUEST) {
        q.fellows = Fellows::default();
    }
    holds(&mut h, me, status::UNDERWAY, 2);
    holds(&mut h, them, status::UNDERWAY, 2);
    take(&mut h, ITEM);
    assert_eq!((phase(&h, me), phase(&h, them)), (3, 2));
}

/// A kill's step that leads somewhere sends the killer there; one that leads nowhere doesn't.
/// Behaviour: server.quests.kill-step-leads-on
#[test]
fn a_kills_step_that_leads_somewhere_sends_the_killer() {
    let mut q = quest(3);
    q.effects = Vec::new();
    let (mut h, _, me) = world(q);
    let template = kill(&mut h, me, &[]);
    let place = yalain_content::quests::Place {
        cell: 0x049E_0138,
        origin: [1.0, 2.0, 3.0],
        heading: 127,
    };
    let step = |needs: u32, to| Step {
        key: String::new(),
        quest: QUEST,
        needs,
        trigger: Trigger::Kill(template),
        tell: None,
        update: Update::Advance(needs + 1),
        xp: 0,
        gives: Vec::new(),
        counts: Vec::new(),
        effects: Vec::new(),
        text: None,
        recipes: Vec::new(),
        placed: "authored for a test".into(),
        requires: Vec::new(),
        takes: Vec::new(),
        to,
        races: Vec::new(),
        effects_last: false,
        effects_quiet: false,
        fellows: false,
        wants: None,
        levels: None,
        xp_share: 0.0,
        transforms: None,
        chance: None,
        to_fellows: false,
    };
    h.server.quests.steps = vec![step(1, Some(place)), step(2, None)];
    for p in h.server.players.values_mut() {
        p.play.quests.push(QuestState {
            quest: QUEST,
            status: status::UNDERWAY,
            phase: 1,
            started: 0,
            updated: 0,
        });
    }
    let trip_to = |h: &Harness| {
        let p = h.server.players.values().next().expect("in the world");
        (p.trip.as_ref().map(|t| t.to.cell), p.play.quests[0].phase)
    };
    kill(&mut h, me, &[]);
    assert_eq!(
        trip_to(&h),
        (Some(0x049E_0138), 1),
        "sent, the update to come"
    );
    land(&mut h);
    assert_eq!(trip_to(&h), (None, 2));
    kill(&mut h, me, &[]);
    assert_eq!(trip_to(&h), (None, 3), "the last leads nowhere");
}

/// Out of portal space, at wherever the trip was for.
fn land(h: &mut Harness) {
    h.advance(yalain_world::places::IN_PORTAL_SPACE);
    h.run(4);
}

/// A step of the test quest for a kill of this template, leading to this place.
pub(super) fn leading_step(template: u32, needs: u32, to: yalain_content::quests::Place) -> Step {
    Step {
        key: String::new(),
        quest: QUEST,
        needs,
        trigger: Trigger::Kill(template),
        tell: None,
        update: Update::Advance(needs + 1),
        xp: 0,
        gives: Vec::new(),
        counts: Vec::new(),
        effects: Vec::new(),
        text: None,
        recipes: Vec::new(),
        placed: "authored for a test".into(),
        requires: Vec::new(),
        takes: Vec::new(),
        to: Some(to),
        races: Vec::new(),
        effects_last: false,
        effects_quiet: false,
        fellows: false,
        wants: None,
        levels: None,
        xp_share: 0.0,
        transforms: None,
        chance: None,
        to_fellows: false,
    }
}

const ARENA: yalain_content::quests::Place = yalain_content::quests::Place {
    cell: 0x049E_0138,
    origin: [1.0, 2.0, 3.0],
    heading: 127,
};

/// A kill's step that leads somewhere is told as the killer comes out there, once, and the journal
/// doesn't move before: the update isn't in what's sent as the trip starts.
/// Behaviour: server.quests.a-kills-update-comes-on-landing
#[test]
fn a_kills_update_comes_as_the_killer_lands() {
    let mut q = quest(3);
    q.effects = Vec::new();
    let (mut h, b, me) = world(q);
    let template = kill(&mut h, me, &[]);
    h.server.quests.steps = vec![leading_step(template, 1, ARENA)];
    holds(&mut h, me, status::UNDERWAY, 1);
    h.run(2);
    let from = h.calls(b, 0).len();
    kill(&mut h, me, &[]);
    assert!(updates(&h, b, from).is_empty(), "nothing told at the kill");
    assert_eq!(phase(&h, me), 1);
    let p = h.server.players.values().next().expect("in the world");
    assert_eq!(p.trip.as_ref().map(|t| t.to.cell), Some(ARENA.cell));
    assert_ne!(p.position.cell, ARENA.cell, "not there yet");
    land(&mut h);
    assert_eq!(updates(&h, b, from).len(), 1, "told once, on landing");
    assert_eq!(phase(&h, me), 2);
    let p = h.server.players.values().next().expect("in the world");
    assert_eq!(p.position.cell, ARENA.cell);
    assert!(p.trip.is_none());
}

/// A kill's step that takes the fellowship along sends the killer's fellows at the kill, a fellow
/// half a kilometer off too; the killer's quest moves as it lands, and the fellow's, which it doesn't
/// hold, not at all.
/// Behaviour: server.quests.a-step-takes-the-fellowship-along
#[test]
fn a_kills_step_that_takes_the_fellowship_along_sends_the_killers_fellows() {
    let mut q = quest(3);
    q.effects = Vec::new();
    let (mut h, a, me) = world(q);
    let (b, you) = fellows(&mut h, (a, me), 500.0);
    h.bots[b].send(
        true,
        auberean_protocol::queue::WEENIE,
        &auberean_protocol::ClientMessage::SceneRenderingComplete(auberean_protocol::login::Empty),
    );
    let template = kill(&mut h, me, &[]);
    h.server.quests.steps = vec![Step {
        to_fellows: true,
        ..leading_step(template, 1, ARENA)
    }];
    holds(&mut h, me, status::UNDERWAY, 1);
    h.run(2);
    kill(&mut h, me, &[]);
    let trip = |h: &Harness, who: u64| {
        h.server
            .players
            .values()
            .find(|p| p.character == who)
            .and_then(|p| p.trip.as_ref().map(|t| t.to.cell))
    };
    assert_eq!(trip(&h, me), Some(ARENA.cell), "the killer sent");
    assert_eq!(trip(&h, you), Some(ARENA.cell), "the fellow sent with it");
    land(&mut h);
    let at = |h: &Harness, who: u64| {
        h.server
            .players
            .values()
            .find(|p| p.character == who)
            .expect("in the world")
            .position
            .cell
    };
    assert_eq!((at(&h, me), at(&h, you)), (ARENA.cell, ARENA.cell));
    assert_eq!(phase(&h, me), 2);
    assert_eq!(state(&h, you), None, "the fellow's quests as they were");
}

/// A kill's step that takes the fellowship along to a room of a row sends each fellow to a free
/// room of its own choosing, as the killer is, never to the room a player stands in though the row
/// names it.
/// Behaviour: server.quests.a-step-takes-the-fellowship-along
#[test]
fn a_fellowship_taken_along_to_a_row_of_rooms_is_sent_to_free_rooms() {
    use yalain_content::quests::{Place, Room};
    let mut q = quest(3);
    q.effects = Vec::new();
    let (mut h, a, me) = world(q);
    let (b, you) = fellows(&mut h, (a, me), 2.0);
    h.bots[b].send(
        true,
        auberean_protocol::queue::WEENIE,
        &auberean_protocol::ClientMessage::SceneRenderingComplete(auberean_protocol::login::Empty),
    );
    let (_, other) = h.player_in_world("three", "Bran");
    let template = kill(&mut h, me, &[]);
    let room = |i: u32| Room {
        place: Place {
            cell: 0x049E_0138 + i,
            #[allow(clippy::cast_precision_loss)]
            origin: [-120.4 + 60.0 * i as f32, -84.7, 3.06],
            heading: 127,
        },
        holds: 0x049E_0160 + i,
    };
    h.server.quests.rooms = vec![vec![room(0), room(1), room(2)]];
    h.server.quests.steps = vec![Step {
        to_fellows: true,
        update: Update::Advance(1),
        ..leading_step(template, 1, room(1).place)
    }];
    let p = h
        .server
        .players
        .values_mut()
        .find(|p| p.character == other)
        .expect("in the world");
    p.position.cell = room(1).place.cell;
    p.position.origin = [
        room(1).place.origin[0],
        room(1).place.origin[1] - 40.0,
        3.06,
    ];
    holds(&mut h, me, status::UNDERWAY, 1);
    h.run(2);
    for _ in 0..20 {
        kill(&mut h, me, &[]);
        for who in [me, you] {
            let p = h
                .server
                .players
                .values_mut()
                .find(|p| p.character == who)
                .expect("in the world");
            let to = p.trip.take().expect("sent").to.cell;
            // Back as though it had landed and drawn its scene.
            p.scene_drawn = true;
            assert!(
                to == room(0).place.cell || to == room(2).place.cell,
                "a free room, not the one a player stands in: {to:08X}"
            );
        }
        holds(&mut h, me, status::UNDERWAY, 1);
    }
}

/// A fellowship taken along to a row of rooms in one moment is split over the row: a fellow is
/// never sent to the room the killer is already on its way to.
/// Behaviour: server.quests.a-step-takes-the-fellowship-along
#[test]
fn a_fellowship_taken_along_to_a_row_of_rooms_never_shares_a_room() {
    use yalain_content::quests::{Place, Room};
    let mut q = quest(3);
    q.effects = Vec::new();
    let (mut h, a, me) = world(q);
    let (b, you) = fellows(&mut h, (a, me), 2.0);
    h.bots[b].send(
        true,
        auberean_protocol::queue::WEENIE,
        &auberean_protocol::ClientMessage::SceneRenderingComplete(auberean_protocol::login::Empty),
    );
    let template = kill(&mut h, me, &[]);
    let room = |i: u32| Room {
        place: Place {
            cell: 0x049E_0138 + i,
            #[allow(clippy::cast_precision_loss)]
            origin: [-120.4 + 60.0 * i as f32, -84.7, 3.06],
            heading: 127,
        },
        holds: 0x049E_0160 + i,
    };
    h.server.quests.rooms = vec![vec![room(0), room(1), room(2)]];
    h.server.quests.steps = vec![Step {
        to_fellows: true,
        update: Update::Advance(1),
        ..leading_step(template, 1, room(1).place)
    }];
    holds(&mut h, me, status::UNDERWAY, 1);
    h.run(2);
    for _ in 0..20 {
        kill(&mut h, me, &[]);
        let mut sent = Vec::new();
        for who in [me, you] {
            let p = h
                .server
                .players
                .values_mut()
                .find(|p| p.character == who)
                .expect("in the world");
            sent.push(p.trip.take().expect("sent").to.cell);
            // Back as though it had landed and drawn its scene.
            p.scene_drawn = true;
        }
        assert_ne!(
            sent[0], sent[1],
            "the killer and its fellow each to a room of its own"
        );
        holds(&mut h, me, status::UNDERWAY, 1);
    }
}

/// Another kill of the kind while the first's trip is under way moves nothing: the quest moves one
/// phase, on landing, even where the creature's own effects name it.
/// Behaviour: server.quests.a-kills-update-comes-on-landing
#[test]
fn a_second_kill_on_the_way_moves_the_quest_no_further() {
    let mut q = quest(4);
    q.effects = Vec::new();
    let (mut h, b, me) = world(q);
    let template = kill(&mut h, me, &[]);
    h.server.quests.steps = vec![
        leading_step(template, 1, ARENA),
        leading_step(template, 2, ARENA),
    ];
    holds(&mut h, me, status::UNDERWAY, 1);
    h.run(2);
    let from = h.calls(b, 0).len();
    kill(&mut h, me, &[]);
    kill(&mut h, me, &[INCREMENT]);
    assert_eq!(phase(&h, me), 1);
    land(&mut h);
    assert_eq!(phase(&h, me), 2);
    assert_eq!(updates(&h, b, from).len(), 1);
}

/// A killer whose client goes away on the way lands all the same before it leaves, and comes back
/// there with the quest moved on.
/// Behaviour: server.quests.a-kills-update-comes-on-landing
#[test]
fn a_killer_whose_client_goes_away_on_the_way_comes_back_moved_on() {
    let mut q = quest(3);
    q.effects = Vec::new();
    let (mut h, b, me) = world(q);
    let template = kill(&mut h, me, &[]);
    h.server.quests.steps = vec![leading_step(template, 1, ARENA)];
    holds(&mut h, me, status::UNDERWAY, 1);
    h.run(2);
    kill(&mut h, me, &[]);
    h.bots[b].leave();
    assert!(
        h.run_until(2000, |h| h.server.players.is_empty()),
        "it leaves"
    );
    let again = h.at_character_list("one");
    h.enter_world(again, "one", me);
    assert_eq!(phase(&h, me), 2);
    let p = h.server.players.values().next().expect("back");
    assert!(p.trip.is_none());
    assert_eq!(p.position.cell, ARENA.cell);
}

/// A quest that has moved while its trip was under way isn't moved back on landing, and a player
/// still comes out.
/// Behaviour: server.quests.a-kills-update-comes-on-landing
#[test]
fn a_quest_moved_meanwhile_stays_where_it_is_on_landing() {
    let mut q = quest(5);
    q.effects = Vec::new();
    let (mut h, b, me) = world(q);
    let template = kill(&mut h, me, &[]);
    h.server.quests.steps = vec![leading_step(template, 1, ARENA)];
    holds(&mut h, me, status::UNDERWAY, 1);
    h.run(2);
    kill(&mut h, me, &[]);
    holds(&mut h, me, status::UNDERWAY, 4);
    let from = h.calls(b, 0).len();
    land(&mut h);
    assert_eq!(phase(&h, me), 4);
    assert!(updates(&h, b, from).is_empty());
    let p = h.server.players.values().next().expect("in the world");
    assert_eq!(p.position.cell, ARENA.cell);
}

/// A kill's trip takes a player off a walk it is on: its use is answered, and at the far end nothing
/// is left to refuse its next use.
/// Behaviour: server.places.a-walk-ends-with-a-trip-or-a-death
#[test]
fn a_kills_trip_ends_a_walk_and_leaves_no_use_refused_after_it() {
    let mut q = quest(3);
    q.effects = Vec::new();
    let (mut h, b, me) = world(q);
    let template = kill(&mut h, me, &[]);
    h.server.quests.steps = vec![leading_step(template, 1, ARENA)];
    holds(&mut h, me, status::UNDERWAY, 1);
    h.run(2);
    let world = *h.server.players.keys().next().expect("in the world");
    let beetle = h
        .server
        .catalog
        .get(yalain_testkit::TEST_CREATURE)
        .cloned()
        .expect("the beetle");
    let mut at = h.server.players[&world].position;
    at.origin[0] += 100.0;
    let id = h.server.things.new_id();
    let mut corpse =
        yalain_world::things::Thing::of(id, &beetle, yalain_world::things::Kind::Corpse);
    corpse.at = Some(at);
    corpse.corpse = Some(yalain_world::things::Corpse {
        killer: me,
        originator: 0,
        contents: Vec::new(),
        expires: f64::MAX,
        creature: yalain_testkit::TEST_CREATURE,
        butchered: false,
        killer_level: 0,
        ..Default::default()
    });
    h.server.things.insert(corpse);
    h.bots[b].send(
        true,
        auberean_protocol::queue::WEENIE,
        &yalain_testkit::play::server_call(0x8216_0016, &[yalain_testkit::play::use_of(id, me)]),
    );
    h.run(4);
    assert!(h.server.players[&world].walking_to.is_some(), "on its way");
    kill(&mut h, me, &[]);
    land(&mut h);
    assert_eq!(h.server.players[&world].position.cell, ARENA.cell);
    assert!(h.server.players[&world].walking_to.is_none(), "no longer");
    assert!(!yalain_world::places::actions_locked(&h.server, world));
}

/// A step leading to a room of a row of like rooms sends the player to one with no player in it and
/// its creature standing, at random among such; failing that, one with no player; failing that, any.
/// Behaviour: server.quests.a-step-leads-to-one-of-its-rooms
#[test]
fn a_kills_step_sends_the_killer_to_a_free_room_of_its_row() {
    use yalain_content::quests::{Place, Room};
    let mut q = quest(3);
    q.effects = Vec::new();
    let (mut h, _, me) = world(q);
    let template = kill(&mut h, me, &[]);
    let room = |i: u32| Room {
        place: Place {
            cell: 0x049E_0138 + i,
            #[allow(clippy::cast_precision_loss)]
            origin: [-120.4 + 60.0 * i as f32, -84.7, 3.06],
            heading: 127,
        },
        holds: 0x049E_0160 + i,
    };
    h.server.quests.rooms = vec![vec![room(0), room(1), room(2)]];
    h.server.quests.steps = vec![Step {
        key: String::new(),
        quest: QUEST,
        needs: 1,
        trigger: Trigger::Kill(template),
        tell: None,
        update: Update::Advance(1),
        xp: 0,
        gives: Vec::new(),
        counts: Vec::new(),
        effects: Vec::new(),
        text: None,
        recipes: Vec::new(),
        placed: "authored for a test".into(),
        requires: Vec::new(),
        takes: Vec::new(),
        to: Some(room(1).place),
        races: Vec::new(),
        effects_last: false,
        effects_quiet: false,
        fellows: false,
        wants: None,
        levels: None,
        xp_share: 0.0,
        transforms: None,
        chance: None,
        to_fellows: false,
    }];
    for p in h.server.players.values_mut() {
        p.play.quests.push(QuestState {
            quest: QUEST,
            status: status::UNDERWAY,
            phase: 1,
            started: 0,
            updated: 0,
        });
    }
    let (_, other) = h.player_in_world("two", "Bran");
    let sent_to = |h: &mut Harness| {
        let mut seen = std::collections::BTreeSet::new();
        for _ in 0..60 {
            kill(h, me, &[]);
            let p = h
                .server
                .players
                .values_mut()
                .find(|p| p.character == me)
                .expect("in the world");
            let to = p.trip.take().expect("sent").to;
            seen.insert(to.cell - 0x049E_0138);
            let at = room(to.cell - 0x049E_0138).place;
            assert_eq!(to.origin, at.origin, "where it lands");
            p.play.quests[0].phase = 1;
        }
        seen.into_iter().collect::<Vec<u32>>()
    };
    assert_eq!(sent_to(&mut h), [0, 1, 2], "all free, none standing");
    let in_room = |h: &mut Harness, who: u64, i: u32| {
        let p = h
            .server
            .players
            .values_mut()
            .find(|p| p.character == who)
            .expect("in the world");
        p.position.cell = room(i).place.cell;
        p.position.origin = [
            room(i).place.origin[0],
            room(i).place.origin[1] - 40.0,
            3.06,
        ];
    };
    in_room(&mut h, other, 0);
    assert_eq!(sent_to(&mut h), [1, 2], "not where a player is");
    // Kept out of the next beetles' way: the highest id is the next to die.
    let mut standing = h
        .server
        .things
        .by_id
        .values()
        .find(|t| t.creature.is_some() && t.alive())
        .cloned()
        .expect("the camp's");
    standing.id = 1;
    standing.at = Some(yalain_store::StoredPosition {
        cell: room(2).holds,
        origin: [0.0, 0.0, 0.0],
        rotation: [1.0, 0.0, 0.0, 0.0],
    });
    h.server.things.insert(standing);
    assert_eq!(sent_to(&mut h), [2], "where its creature stands");
    in_room(&mut h, other, 2);
    assert_eq!(
        sent_to(&mut h),
        [0, 1],
        "not though it stands, with a player there"
    );
}

/// A kill's trip keeps its step: a recall coming due on the way takes it nowhere else, and the
/// quest moves as it lands.
/// Behaviour: server.quests.a-kills-trip-keeps-its-step
#[test]
fn a_recall_on_a_kills_trip_leaves_it_and_its_step() {
    let mut q = quest(3);
    q.effects = Vec::new();
    let (mut h, _b, me) = world(q);
    let template = kill(&mut h, me, &[]);
    h.server.quests.steps = vec![leading_step(template, 1, ARENA)];
    holds(&mut h, me, status::UNDERWAY, 1);
    h.run(2);
    kill(&mut h, me, &[]);
    let world = *h.server.players.keys().next().expect("in the world");
    let elsewhere = yalain_store::StoredPosition {
        cell: 0x0102_0003,
        origin: [12.5, 34.5, 2.0],
        rotation: [1.0, 0.0, 0.0, 0.0],
    };
    let clock = h.clock();
    yalain_world::places::recall_trip(&mut h.server, &mut h.net, world, elsewhere, None, clock);
    let p = &h.server.players[&world];
    assert_eq!(p.trip.as_ref().map(|t| t.to.cell), Some(ARENA.cell));
    assert!(
        p.trip.as_ref().is_some_and(|t| t.step.is_some()),
        "its step"
    );
    land(&mut h);
    assert_eq!(phase(&h, me), 2);
    assert_eq!(h.server.players[&world].position.cell, ARENA.cell);
}

/// The live creature a camp (by its row in the spawn table) keeps.
fn creature_of(h: &Harness, camp: usize) -> u64 {
    h.server
        .things
        .by_id
        .values()
        .find(|t| {
            t.creature
                .as_ref()
                .is_some_and(|c| c.camp == Some(camp) && c.mode != yalain_world::things::Mode::Dead)
        })
        .map(|t| t.id)
        .expect("the camp's creature")
}

/// Hurt a creature to death by this character.
fn slay(h: &mut Harness, creature: u64, by: u64) {
    let t = h.server.things.get_mut(creature).expect("the creature");
    t.health = 10;
    t.max_health = 10;
    let c = t.creature.as_mut().expect("a creature");
    c.stats.attacks_first = false;
    let clock = h.clock();
    combat::hurt_creature(&mut h.server, &mut h.net, creature, by, 50, clock);
    h.run(2);
}

/// **A creature a camp names is told by that name, and its corpse is named after it**; a step made
/// by a kill of that camp's creature moves its quest, and a kill of a creature of the same template
/// from another camp does not.
/// Behaviour: server.quests.kill-of-one-camps-creature
#[test]
fn a_named_camps_creature_goes_by_its_name_and_only_its_kill_moves_the_camps_step() {
    use auberean_protocol::types::WeenieDesc;
    use auberean_protocol::ServerMessage;
    const NAMED: (u32, u32) = (0xE77B_18D7, 0x2500_284B);
    let (mut h, b, me) = world(quest(3));
    holds(&mut h, me, status::UNDERWAY, 1);
    let from = h.bots[b].inbox.len();
    h.server.spawns.camps.push(yalain_content::Camp {
        id: "named".into(),
        name: Some(NAMED),
        ..yalain_testkit::test_camp(1)
    });
    h.server
        .camps
        .push(yalain_world::spawn::CampState::default());
    h.run(4);
    let (other, named) = (creature_of(&h, 0), creature_of(&h, 1));
    let told = |h: &Harness, from: usize, id: u64| {
        h.world_messages(b, from)
            .into_iter()
            .find_map(|m| match m {
                ServerMessage::CreateObject(o) if o.id == id => Some(o.weenie),
                _ => None,
            })
            .expect("told of it")
    };
    let w = told(&h, from, named);
    assert_ne!(w.mask & WeenieDesc::NAME, 0, "told with a name");
    assert_eq!((w.name.string_id, w.name.table_id), NAMED);
    let template = h.server.things.get(named).expect("named").template;
    assert_eq!(
        h.server.things.get(other).map(|t| t.template),
        Some(template)
    );
    h.server.quests.steps.push(Step {
        key: String::new(),
        quest: QUEST,
        needs: 1,
        trigger: Trigger::KillIn(template, 1),
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
    let from = h.bots[b].inbox.len();
    slay(&mut h, other, me);
    assert_eq!(phase(&h, me), 1, "another camp's creature of the template");
    assert!(updates(&h, b, from).is_empty());
    slay(&mut h, named, me);
    assert_eq!(phase(&h, me), 2);
    assert_eq!(updates(&h, b, from), vec![(0, update::ADVANCE | 2)]);
    // Its corpse is named after it.
    h.run(60);
    let corpses: Vec<_> = h
        .world_messages(b, from)
        .into_iter()
        .filter_map(|m| match m {
            ServerMessage::CreateObject(o)
                if o.weenie.entity_did == yalain_world::death::CORPSE_TEMPLATE =>
            {
                Some(o.weenie.name)
            }
            _ => None,
        })
        .collect();
    assert!(
        corpses.contains(&yalain_world::death::corpse_name(Some(NAMED))),
        "{corpses:?}"
    );
}

/// The item a chance step gives: one the test world's catalog holds.
const GEM: u32 = yalain_testkit::TEST_ITEM;

/// A step on the beetle's kill, its quest at its first phase, that moves no quest and gives the gem,
/// by this chance.
fn chance_step(template: u32, chance: f32) -> Step {
    Step {
        key: String::new(),
        quest: QUEST,
        needs: 1,
        trigger: Trigger::Kill(template),
        tell: None,
        update: Update::Nothing,
        xp: 0,
        gives: vec![GEM],
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
        chance: Some(chance),
        to_fellows: false,
    }
}

/// How many of the gem a character holds.
fn gems(h: &Harness, who: u64) -> u32 {
    h.server
        .players
        .values()
        .find(|p| p.character == who)
        .expect("in the world")
        .play
        .items
        .iter()
        .filter(|i| i.template == GEM)
        .map(|i| i.quantity.max(1))
        .sum()
}

/// **A step with a chance runs only when its chance comes up**: of sixty kills at its phase,
/// each able to run it, some give the gem and some give nothing, and none moves the quest.
/// Behaviour: server.quests.a-step-runs-by-its-chance
#[test]
fn a_kills_step_with_a_chance_runs_on_some_kills_and_not_on_others() {
    let (mut h, _b, me) = world(quest(5));
    holds(&mut h, me, status::UNDERWAY, 1);
    let template = kill(&mut h, me, &[]);
    assert_eq!(gems(&h, me), 0);
    h.server.quests.steps.push(chance_step(template, 0.5));
    for _ in 0..60 {
        kill(&mut h, me, &[]);
    }
    let given = gems(&h, me);
    assert!(
        (15..=45).contains(&given),
        "about half of 60 kills give the gem, not {given}"
    );
    assert_eq!(phase(&h, me), 1, "the step moves no quest");
}

/// **A step whose chance does not come up is passed over as though it could not run**: the
/// kill's own effect then moves the quest the step would have kept it from moving; when it comes up
/// (a chance of 1), the step runs and the effect moves that quest no further.
/// Behaviour: server.quests.a-step-runs-by-its-chance
#[test]
fn a_step_whose_chance_does_not_come_up_leaves_the_kill_to_move_its_quest() {
    let (mut h, _b, me) = world(quest(5));
    holds(&mut h, me, status::UNDERWAY, 1);
    let template = kill(&mut h, me, &[]);
    h.server
        .quests
        .steps
        .push(chance_step(template, f32::MIN_POSITIVE));
    kill(&mut h, me, &[INCREMENT]);
    assert_eq!(
        gems(&h, me),
        0,
        "a chance that never comes up gives nothing"
    );
    assert_eq!(phase(&h, me), 2, "the kill's effect moves the quest");
    holds(&mut h, me, status::UNDERWAY, 1);
    h.server.quests.steps.pop();
    h.server.quests.steps.push(chance_step(template, 1.0));
    kill(&mut h, me, &[INCREMENT]);
    assert_eq!(gems(&h, me), 1, "a chance of 1 always comes up");
    assert_eq!(
        phase(&h, me),
        1,
        "the step moved the quest, so the effect does not"
    );
}

/// The test quest's recall: its first advance's effect, which sends its player to a named place.
const RECALL: u32 = 0x6F00_7701;
/// Its name's table, which holds "You recall to ..." (`RECALLED`).
const RECALL_TABLE: u32 = 0x2500_7701;
const RECALLED: u32 = 0x912E_FF1D;
const SHOW_TEXT: u32 = 0x8205_0011;
/// Places the world holds by their names, and one it lacks.
const FAR: &str = "test_far_room";
const NEAR: &str = "test_near_room";
const MISSING: &str = "test_missing_room";
const FAR_AT: yalain_content::Place = yalain_content::Place {
    cell: 0x049D_0105,
    origin: [10.0, 20.0, 3.0],
    rotation: [1.0, 0.0, 0.0, 0.0],
};
const NEAR_AT: yalain_content::Place = yalain_content::Place {
    cell: 0x049C_0103,
    origin: [30.0, 40.0, 3.0],
    rotation: [1.0, 0.0, 0.0, 0.0],
};

/// A world whose quest's first advance carries a recall to these names, said on arrival or not,
/// and holds the far and the near places; its player holds the quest under way at its start.
fn recall_world(names: &[&str], told: bool) -> (Harness, usize, u64) {
    use yalain_world::effect_defs::{EffectDef, Kind as EffectKind, Recall, Scale};
    let mut q = quest(3);
    q.effects = vec![(update::ADVANCE | 2, RECALL, 1.0)];
    let (mut h, b, me) = world(q);
    h.server.effect_table.effects.insert(
        RECALL,
        EffectDef {
            state: RECALL,
            kind: EffectKind::Recall(Recall::Named {
                names: names.iter().map(ToString::to_string).collect(),
                told,
            }),
            duration: Scale::default(),
            priority: Scale::default(),
            fx: 0,
            external_flags: 0,
            internal_flags: 0,
            enum_value: 0,
            eq_class: 0,
            name: Some((0xA860_7CEC, RECALL_TABLE)),
            ends_at_logout: false,
            ends_at_teleport: false,
        },
    );
    h.server.places_by_name.insert(FAR.into(), FAR_AT);
    h.server.places_by_name.insert(NEAR.into(), NEAR_AT);
    holds(&mut h, me, status::UNDERWAY, 1);
    h.run(2);
    (h, b, me)
}

/// The player's trip under way, if any: where to.
fn trip_cell(h: &Harness) -> Option<u32> {
    let p = h.server.players.values().next().expect("in the world");
    p.trip.as_ref().map(|t| t.to.cell)
}

fn cell(h: &Harness) -> u32 {
    h.server
        .players
        .values()
        .next()
        .expect("in the world")
        .position
        .cell
}

/// Out of portal space, a recall's time after it went in.
fn recall_lands(h: &mut Harness) {
    h.advance(yalain_world::skill_use::RECALL_IN_PORTAL);
    h.run(4);
}

/// Where the quest's update and the player's newer place come among the messages since `from`.
fn update_and_place(h: &Harness, b: usize, from: usize, me: u64) -> (Option<usize>, Option<usize>) {
    use auberean_protocol::ServerMessage;
    let msgs = h.world_messages(b, from);
    let update = msgs
        .iter()
        .position(|m| matches!(m, ServerMessage::PlayerCall(c) if c.function.0 == UPDATE_QUEST));
    let place = msgs.iter().position(
        |m| matches!(m, ServerMessage::Position(p) if p.sender.id == me && p.teleport_stamp > 0),
    );
    (update, place)
}

/// The texts' string ids told since `from`.
fn texts(h: &Harness, b: usize, from: usize) -> Vec<u32> {
    h.calls(b, from)
        .into_iter()
        .filter(|(f, _)| *f == SHOW_TEXT)
        .filter_map(|(_, v)| {
            v.iter().find_map(|x| match x.root() {
                Some(yalain_world::calls::Node::TextInfo(t)) => Some(t.string_id),
                _ => None,
            })
        })
        .collect()
}

/// **An advance whose effects carry a recall takes its player there**: into portal space at the
/// kill, the quest moved on at once but its update not yet told; out at the recall's place a recall's
/// time later, told the update there, before its new place, and the recall's line where it says one.
/// Behaviour: server.quests.an-updates-recall-takes-its-player
#[test]
fn an_advance_carrying_a_recall_takes_its_player_there_and_is_told_as_it_comes_out() {
    let (mut h, b, me) = recall_world(&[FAR], true);
    let from = h.bots[b].inbox.len();
    kill(&mut h, me, &[INCREMENT]);
    assert_eq!(phase(&h, me), 2, "moved at once");
    assert_eq!(trip_cell(&h), Some(FAR_AT.cell), "on its way at the kill");
    assert_ne!(cell(&h), FAR_AT.cell);
    assert!(updates(&h, b, from).is_empty(), "nothing told yet");
    recall_lands(&mut h);
    assert_eq!(cell(&h), FAR_AT.cell, "out at the recall's place");
    assert_eq!(trip_cell(&h), None);
    assert_eq!(updates(&h, b, from), vec![(0, update::ADVANCE | 2)]);
    let (told, placed) = update_and_place(&h, b, from, me);
    assert!(told.is_some() && told < placed, "{told:?} {placed:?}");
    assert!(texts(&h, b, from).contains(&RECALLED), "its line");
    // Its class says nothing, and a later advance with no recall leaves the player where it is.
    let (mut h, b, me) = recall_world(&[FAR], false);
    let from = h.bots[b].inbox.len();
    kill(&mut h, me, &[INCREMENT]);
    recall_lands(&mut h);
    assert_eq!(cell(&h), FAR_AT.cell);
    assert!(!texts(&h, b, from).contains(&RECALLED), "no line");
    kill(&mut h, me, &[INCREMENT]);
    assert_eq!(phase(&h, me), 3);
    assert_eq!(trip_cell(&h), None, "the next advance carries none");
    // A recall among the effects another puts on at once takes the player the same.
    const CHAIN: u32 = 0x6F00_7702;
    let (mut h, _, me) = recall_world(&[FAR], false);
    let mut chain = h.server.effect_table.effects[&RECALL].clone();
    chain.state = CHAIN;
    chain.kind = yalain_world::effect_defs::Kind::Instants(vec![RECALL]);
    h.server.effect_table.effects.insert(CHAIN, chain);
    for q in &mut h.server.quests.quests {
        q.effects = vec![(update::ADVANCE | 2, CHAIN, 1.0)];
    }
    kill(&mut h, me, &[INCREMENT]);
    assert_eq!(trip_cell(&h), Some(FAR_AT.cell), "its chain's recall");
    recall_lands(&mut h);
    assert_eq!(cell(&h), FAR_AT.cell);
}

/// **A recall of several places takes its player to one of those the world holds, at random**:
/// over many advances both of the world's are reached, and never the one it lacks.
/// Behaviour: server.quests.an-updates-recall-takes-its-player
#[test]
fn a_recall_of_several_places_lands_at_one_the_world_holds_chosen_at_random() {
    let (mut h, _, me) = recall_world(&[FAR, MISSING, NEAR], false);
    let mut landed = std::collections::BTreeMap::new();
    for _ in 0..16 {
        holds(&mut h, me, status::UNDERWAY, 1);
        kill(&mut h, me, &[INCREMENT]);
        let to = trip_cell(&h).expect("sent");
        recall_lands(&mut h);
        assert_eq!(cell(&h), to);
        *landed.entry(to).or_insert(0) += 1;
    }
    assert_eq!(
        landed.keys().copied().collect::<Vec<_>>(),
        vec![NEAR_AT.cell, FAR_AT.cell],
        "{landed:x?}"
    );
}

/// **A recall with no place in the world takes its player nowhere**: the quest moves on and is told
/// at once, the player stays where it stood, and the server notes the recall.
/// Behaviour: server.quests.an-updates-recall-with-no-place-stays
#[test]
fn a_recall_to_a_place_the_world_lacks_leaves_its_player_where_it_is() {
    let (mut h, b, me) = recall_world(&[MISSING], false);
    let here = cell(&h);
    let from = h.bots[b].inbox.len();
    kill(&mut h, me, &[INCREMENT]);
    assert_eq!(phase(&h, me), 2);
    assert_eq!(trip_cell(&h), None, "not sent");
    assert_eq!(
        updates(&h, b, from),
        vec![(0, update::ADVANCE | 2)],
        "told at once"
    );
    recall_lands(&mut h);
    assert_eq!(cell(&h), here);
    assert!(
        h.server
            .unhandled
            .counts()
            .keys()
            .any(|k| k.contains(&format!("{RECALL:08X}"))),
        "noted: {:?}",
        h.server.unhandled.counts()
    );
}

/// **A step with a place of its own takes its player there, once**: its update's recall stays put,
/// so the kill's trip goes to the step's place, its update is told on landing, and no second trip
/// follows.
/// Behaviour: server.quests.a-steps-place-wins-over-its-updates-recall
#[test]
fn a_step_leading_somewhere_sends_its_player_once_to_its_own_place_not_the_recalls() {
    let (mut h, b, me) = recall_world(&[FAR], false);
    let template = kill(&mut h, me, &[]);
    h.server.quests.steps = vec![leading_step(template, 1, ARENA)];
    holds(&mut h, me, status::UNDERWAY, 1);
    h.run(2);
    let from = h.bots[b].inbox.len();
    kill(&mut h, me, &[]);
    assert_eq!(trip_cell(&h), Some(ARENA.cell), "the step's place");
    land(&mut h);
    assert_eq!(phase(&h, me), 2);
    assert_eq!(cell(&h), ARENA.cell);
    recall_lands(&mut h);
    h.run(20);
    assert_eq!(trip_cell(&h), None, "no second trip");
    assert_eq!(cell(&h), ARENA.cell);
    assert_eq!(
        updates(&h, b, from),
        vec![(0, update::ADVANCE | 2)],
        "told once"
    );
    let stamps = h
        .world_messages(b, from)
        .iter()
        .filter(|m| {
            matches!(m, auberean_protocol::ServerMessage::Position(p) if p.sender.id == me && p.teleport_stamp > 0)
        })
        .map(|m| match m {
            auberean_protocol::ServerMessage::Position(p) => p.teleport_stamp,
            _ => 0,
        })
        .collect::<std::collections::BTreeSet<_>>();
    assert_eq!(stamps.len(), 1, "one trip: {stamps:?}");
}

/// **A failure whose effects carry a recall takes its player too**, told as it comes out; and a
/// player already on its way somewhere is left to that trip, the update told at once.
/// Behaviour: server.quests.an-updates-recall-takes-its-player
#[test]
fn a_failure_carrying_a_recall_takes_its_player_and_one_on_its_way_is_left_to_it() {
    let (mut h, b, me) = recall_world(&[FAR], false);
    for q in &mut h.server.quests.quests {
        q.effects = vec![(update::FAIL, RECALL, 1.0)];
    }
    let world = *h.server.players.keys().next().expect("in the world");
    let from = h.bots[b].inbox.len();
    let clock = h.clock();
    yalain_world::quests::fail(&mut h.server, &mut h.net, world, QUEST, clock);
    h.run(2);
    assert_eq!(state(&h, me).map(|s| s.0), Some(status::FAILED));
    assert_eq!(trip_cell(&h), Some(FAR_AT.cell));
    assert!(updates(&h, b, from).is_empty(), "nothing told yet");
    recall_lands(&mut h);
    assert_eq!(cell(&h), FAR_AT.cell);
    assert_eq!(updates(&h, b, from), vec![(0, update::FAIL)]);
    // On its way to the arena, the failure's recall leaves it to that trip.
    let (mut h, b, me) = recall_world(&[FAR], false);
    for q in &mut h.server.quests.quests {
        q.effects = vec![(update::FAIL, RECALL, 1.0)];
    }
    let world = *h.server.players.keys().next().expect("in the world");
    let to = yalain_store::StoredPosition {
        cell: ARENA.cell,
        origin: ARENA.origin,
        rotation: [1.0, 0.0, 0.0, 0.0],
    };
    let clock = h.clock();
    yalain_world::places::sent_away(&mut h.server, &mut h.net, world, to, clock);
    let from = h.bots[b].inbox.len();
    yalain_world::quests::fail(&mut h.server, &mut h.net, world, QUEST, clock);
    h.run(2);
    assert_eq!(
        updates(&h, b, from),
        vec![(0, update::FAIL)],
        "told at once"
    );
    assert_eq!(trip_cell(&h), Some(ARENA.cell), "still the trip it was on");
    land(&mut h);
    recall_lands(&mut h);
    h.run(20);
    assert_eq!(cell(&h), ARENA.cell);
    assert_eq!(state(&h, me).map(|s| s.0), Some(status::FAILED));
    assert_eq!(updates(&h, b, from).len(), 1, "told once");
}
