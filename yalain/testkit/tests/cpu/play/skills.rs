//! Skills: training spends credits, raising spends experience, each by the client's rules, and a
//! refusal is a text alone.

use std::collections::HashMap;

use auberean_protocol::{queue, ServerMessage};
use auberean_script::args::Arg;
use yalain_testkit::play::server_call;
use yalain_testkit::Harness;
use yalain_world::calls::{self, Value};
use yalain_world::skills::{raise_cost, table_level, SkillDef, SkillTable};

const TRAIN: u32 = 0x8214_0002;
const RAISE: u32 = 0x8214_000C;
const UPDATE_INFO: u32 = 0x8214_000D;
const UPDATE_REPOSITORY: u32 = 0x8214_0011;
const SHOW_TEXT: u32 = 0x8205_0011;

/// A made-up skill to train at level 2 for one credit, raised along a table like the masters'.
const SKILL: u32 = 15;
const TABLE: u32 = 0x6F00_000E;

fn table() -> SkillTable {
    let mut skills = HashMap::new();
    skills.insert(
        SKILL,
        SkillDef {
            id: SKILL,
            name: (0xA860_7CEC, 0x2500_085B),
            cost: 1,
            min_level: 2,
            cap: 50,
            table: TABLE,
            flags: 0x1,
            ..SkillDef::default()
        },
    );
    let mut tables = HashMap::new();
    tables.insert(
        TABLE,
        (0..=50u64).map(|n| 2 * n * n.saturating_sub(1)).collect(),
    );
    SkillTable { skills, tables }
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

/// A level-2 player with a credit and 300 experience to spend.
fn trainee(h: &mut Harness) -> usize {
    h.server.skill_table = table();
    let (b, _) = h.player_in_world("one", "Elara");
    for p in h.server.players.values_mut() {
        p.play.credits = 1;
        yalain_world::quals::set_int(&mut p.qualities, 302, 2);
        yalain_world::quals::set_long(&mut p.qualities, 301, 300);
    }
    b
}

fn ask(h: &mut Harness, b: usize, f: u32) -> usize {
    let from = h.bots[b].inbox.len();
    h.bots[b].send(true, queue::WEENIE, &server_call(f, &[Arg::Int(SKILL)]));
    h.run(10);
    from
}

/// Training a skill a player may train: the repository with the credit spent, the skill's
/// information, then its text; training it again is refused with a text alone.
/// Behaviour: server.skills.train
#[test]
fn training_a_skill_spends_its_credits_and_again_is_refused() {
    let mut h = Harness::new();
    let b = trainee(&mut h);
    let from = ask(&mut h, b, TRAIN);
    let fs: Vec<u32> = calls_of(&h, b, from).iter().map(|(f, _)| *f).collect();
    assert_eq!(fs, vec![UPDATE_REPOSITORY, UPDATE_INFO, SHOW_TEXT]);
    let p = h.server.players.values().next().expect("the player");
    assert_eq!(p.play.credits, 0);
    assert!(p
        .play
        .skills
        .iter()
        .any(|s| s.skill == SKILL && s.mask == 0x3 && s.cost == 1));
    let from = ask(&mut h, b, TRAIN);
    let fs: Vec<u32> = calls_of(&h, b, from).iter().map(|(f, _)| *f).collect();
    assert_eq!(fs, vec![SHOW_TEXT], "already trained: a text alone");
}

/// Raising a trained skill costs the experience to its table's next level (4 from none on the
/// masters' table, then 8): the skill's information, the experience left, its text; with too
/// little experience left, a text alone.
/// Behaviour: server.skills.raise
#[test]
fn raising_a_skill_spends_the_experience_to_its_tables_next_level() {
    let mut h = Harness::new();
    let b = trainee(&mut h);
    ask(&mut h, b, TRAIN);
    let from = ask(&mut h, b, RAISE);
    let fs: Vec<u32> = calls_of(&h, b, from).iter().map(|(f, _)| *f).collect();
    assert_eq!(fs, vec![UPDATE_INFO, SHOW_TEXT]);
    let p = h.server.players.values().next().expect("the player");
    assert_eq!(yalain_world::quals::long(&p.qualities, 301), Some(296));
    ask(&mut h, b, RAISE);
    let p = h.server.players.values().next().expect("the player");
    assert_eq!(yalain_world::quals::long(&p.qualities, 301), Some(288));
    for p in h.server.players.values_mut() {
        yalain_world::quals::set_long(&mut p.qualities, 301, 5);
    }
    let from = ask(&mut h, b, RAISE);
    let fs: Vec<u32> = calls_of(&h, b, from).iter().map(|(f, _)| *f).collect();
    assert_eq!(fs, vec![SHOW_TEXT], "short of experience: a text alone");
}

/// A skill's level in its table: the highest level whose experience it holds (none still reaches
/// level 1); a raise costs the experience to the next, none at the top.
/// Behaviour: server.skills.advancement-table
#[test]
fn a_tables_level_and_raise_cost_follow_the_library() {
    let t: Vec<u64> = vec![0, 0, 4, 12, 24];
    assert_eq!(table_level(&t, 0), 1);
    assert_eq!(table_level(&t, 4), 2);
    assert_eq!(table_level(&t, 11), 2);
    assert_eq!(raise_cost(&t, 0), 4);
    assert_eq!(raise_cost(&t, 4), 8);
    assert_eq!(raise_cost(&t, 24), 0);
}
