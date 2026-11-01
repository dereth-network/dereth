//! Untraining a skill: one with no experience in it is untrained at once, its credits back; one with
//! experience waits until the player has earned that much again, then is untrained and its
//! experience given back to spend; an untraining under way can be cancelled. Fixture: a made-up
//! skill trained by a new player, with or without experience put into it.

use std::collections::HashMap;

use auberean_protocol::ServerMessage;
use auberean_script::args::Arg;
use yalain_testkit::Harness;
use yalain_world::calls::Value;
use yalain_world::progress::{self, Award};
use yalain_world::skills::{function, untraining, SkillDef, SkillTable};
use yalain_world::state::SkillState;

/// A made-up skill trained for two credits.
const SKILL: u32 = 15;
const COST: u32 = 2;
const AVAILABLE_XP: u32 = 301;

/// A player that trained the skill, `xp` put into it, with one credit left: its client and
/// connection.
fn trained(xp: u64) -> (Harness, usize, yalain_net::ConnId) {
    let mut h = Harness::new();
    let mut skills = HashMap::new();
    skills.insert(
        SKILL,
        SkillDef {
            id: SKILL,
            name: (0xA860_7CEC, 0x2500_085B),
            cost: COST,
            flags: 0x1,
            ..SkillDef::default()
        },
    );
    h.server.skill_table = SkillTable {
        skills,
        tables: HashMap::new(),
    };
    let (b, _) = h.player_in_world("one", "Elara");
    let world = *h.server.players.keys().next().expect("in");
    let p = h.server.players.get_mut(&world).expect("in");
    p.play.credits = 1;
    p.play.skills.push(SkillState {
        skill: SKILL,
        granted: 0,
        mask: 0x3,
        cost: COST,
        children: 0,
        dependents: 0,
        spent: xp,
    });
    h.run(2);
    (h, b, world)
}

fn holds(h: &Harness, world: yalain_net::ConnId) -> bool {
    h.server.players[&world]
        .play
        .skills
        .iter()
        .any(|s| s.skill == SKILL)
}

/// The untraining texts shown since `from`, in order.
fn texts(h: &Harness, b: usize, from: usize) -> Vec<u32> {
    let ours = [
        untraining::UNTRAINED,
        untraining::UNDER_WAY,
        untraining::CANCELLED,
    ];
    h.texts(b, from)
        .into_iter()
        .map(|(_, id)| id)
        .filter(|id| ours.contains(id))
        .collect()
}

fn ask(h: &mut Harness, b: usize, f: u32, args: &[Arg]) -> usize {
    let from = h.bots[b].inbox.len();
    h.ask(b, f, args, 5);
    from
}

/// A skill with no experience in it is untrained at once: told so, the skill forgotten, its credits
/// back in the repository.
/// Behaviour: server.skills.untrain-at-once
#[test]
fn a_skill_with_no_experience_in_it_is_untrained_at_once_and_its_credits_come_back() {
    let (mut h, b, world) = trained(0);
    let from = ask(&mut h, b, function::UNTRAIN, &[Arg::Int(SKILL)]);
    assert_eq!(texts(&h, b, from), [untraining::UNTRAINED]);
    let calls = h.calls(b, from);
    assert!(
        calls
            .iter()
            .any(|(f, v)| *f == function::REMOVE_INFO && v == &[Value::Int(SKILL)]),
        "the skill forgotten"
    );
    assert!(calls.iter().any(|(f, _)| *f == function::UPDATE_REPOSITORY));
    assert!(!holds(&h, world));
    assert_eq!(h.server.players[&world].play.credits, 1 + COST);
}

/// A skill with experience in it waits: the repository names it with the experience to earn and
/// the player is told; an award short of it leaves it; the award that makes it up untrains it, the
/// skill's experience given back to spend and its credits back.
/// Behaviour: server.skills.untrain-after-earning-its-experience
#[test]
fn a_skill_with_experience_in_it_is_untrained_once_that_much_is_earned_again() {
    let (mut h, b, world) = trained(500);
    let from = ask(&mut h, b, function::UNTRAIN, &[Arg::Int(SKILL)]);
    assert_eq!(texts(&h, b, from), [untraining::UNDER_WAY]);
    assert!(holds(&h, world), "kept until earned");
    let before =
        yalain_world::quals::long(&h.server.players[&world].qualities, AVAILABLE_XP).unwrap_or(0);
    let from = h.bots[b].inbox.len();
    progress::award(&mut h.server, &mut h.net, world, 300, Award::Step);
    h.run(2);
    assert!(texts(&h, b, from).is_empty(), "not yet");
    assert!(holds(&h, world));
    progress::award(&mut h.server, &mut h.net, world, 200, Award::Step);
    h.run(2);
    assert_eq!(texts(&h, b, from), [untraining::UNTRAINED]);
    assert!(!holds(&h, world), "untrained");
    let after =
        yalain_world::quals::long(&h.server.players[&world].qualities, AVAILABLE_XP).unwrap_or(0);
    assert_eq!(
        after,
        before + 300 + 200 + 500,
        "the awards and the skill's own"
    );
    assert_eq!(h.server.players[&world].play.credits, 1 + COST);
}

/// An untraining under way, cancelled: the repository names none, the player is told, the skill
/// is kept, and later awards leave it.
/// Behaviour: server.skills.untrain-cancelled
#[test]
fn an_untraining_under_way_cancelled_keeps_the_skill() {
    let (mut h, b, world) = trained(500);
    ask(&mut h, b, function::UNTRAIN, &[Arg::Int(SKILL)]);
    let from = ask(&mut h, b, function::UNTRAIN_CANCEL, &[]);
    assert!(h
        .calls(b, from)
        .iter()
        .any(|(f, _)| *f == function::UPDATE_REPOSITORY));
    assert_eq!(texts(&h, b, from), [untraining::CANCELLED]);
    progress::award(&mut h.server, &mut h.net, world, 1000, Award::Step);
    h.run(2);
    assert!(holds(&h, world), "kept");
    assert!(h
        .world_messages(b, from)
        .iter()
        .all(|m| !matches!(m, ServerMessage::UpdateTimestamp(_))));
}
