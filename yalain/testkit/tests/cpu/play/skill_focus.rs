//! Skills that cost focus (the heroes' skills): a use pays its definition's cost at the user's
//! rating, after its vigor, and one the player hasn't the focus for is refused with the client's
//! reason. Fixture: a hero with a sword in hand, vigor to spare and two skills made up after the
//! data's Tidal Wave line (30 focus at rating 1 to 40 at 150): a heal on itself and a strike.

use auberean_protocol::{queue, ServerMessage};
use auberean_script::args::Arg;
use yalain_testkit::play::server_call;
use yalain_testkit::{Harness, EASE_SUFFERING};
use yalain_world::calls::{self, Value};
use yalain_world::player::quality;
use yalain_world::skill_use::{focus_cost, function, refusal};
use yalain_world::skills::Usage;
use yalain_world::state::SkillState;
use yalain_world::world::Player;
use yalain_world::{focus, quals};

const SWORD: u32 = 0x4700_051C;
const MIND_LASH: u32 = 96;
/// A heal and a strike costing focus, each trained to a rating of 44.
const HEAL: u32 = 9_101;
const STRIKE: u32 = 9_102;
const RATING: u32 = 44;
/// Tidal Wave's line: 30 at rating 1 to 40 at its cap of 150.
const LINE: (f32, f32) = (-30.0, -40.0);
const CAP: u32 = 150;

/// A hero (or not) in the world with the scene drawn, a sword in hand, vigor to spare and `focus`,
/// holding the heal and the strike. Returns the client and the player.
fn hero(is_hero: bool, focus: i32) -> (Harness, usize, u64) {
    let mut h = Harness::new();
    let (b, me) = h.player_in_world("one", "Elara");
    h.bots[b].send(
        true,
        queue::WEENIE,
        &auberean_protocol::ClientMessage::SceneRenderingComplete(auberean_protocol::login::Empty),
    );
    h.run(5);
    h.arm(b, me, SWORD);
    for (id, like) in [(HEAL, EASE_SUFFERING), (STRIKE, MIND_LASH)] {
        let mut d = h.server.skill_table.skills[&like].clone();
        d.id = id;
        d.cap = CAP;
        if let Some(u) = d.usage.as_mut() {
            u.focus_add = LINE;
            // Used with whatever is in hand.
            u.implements_right = -1;
        }
        h.server.skill_table.skills.insert(id, d);
    }
    let p = player(&mut h, me);
    quals::set_bool(&mut p.qualities, focus::HERO, is_hero);
    quals::set_int(&mut p.qualities, quality::VIGOR_MAX, 400);
    quals::set_int(&mut p.qualities, quality::VIGOR, 400);
    quals::set_int(&mut p.qualities, quality::FOCUS, focus);
    for skill in [HEAL, STRIKE] {
        p.play.skills.push(SkillState {
            skill,
            granted: RATING - 1,
            mask: yalain_world::skills::mask::TRAINED,
            cost: 0,
            children: 0,
            dependents: 0,
            spent: 0,
        });
    }
    (h, b, me)
}

fn player(h: &mut Harness, me: u64) -> &mut Player {
    h.server
        .players
        .values_mut()
        .find(|p| p.character == me)
        .expect("in the world")
}

fn focus_of(h: &mut Harness, me: u64) -> Option<i32> {
    quals::int(&player(h, me).qualities, quality::FOCUS)
}

/// Use a skill at a target and let the use end; the messages since the request begin at the index
/// returned.
fn use_skill(h: &mut Harness, b: usize, skill: u32, target: u64) -> usize {
    let from = h.bots[b].inbox.len();
    h.bots[b].send(
        true,
        queue::WEENIE,
        &server_call(
            function::USE,
            &[Arg::Int(7), Arg::Int(skill), Arg::LongInt(target)],
        ),
    );
    h.run(80);
    from
}

/// The vigor and focus sent, in order.
fn vitals_sent(h: &Harness, b: usize, from: usize) -> Vec<(u32, i32)> {
    h.world_messages(b, from)
        .iter()
        .filter_map(|m| match m {
            ServerMessage::UpdateInt(q)
                if q.quality == quality::VIGOR || q.quality == quality::FOCUS =>
            {
                Some((q.quality, q.value))
            }
            _ => None,
        })
        .collect()
}

/// The reasons a use was refused.
fn refusals(h: &Harness, b: usize, from: usize) -> Vec<u32> {
    h.world_messages(b, from)
        .iter()
        .filter_map(|m| match m {
            ServerMessage::PlayerCall(c) if c.function.0 == function::REFUSED => c
                .args
                .as_ref()
                .and_then(|a| calls::values(&a.0).ok())
                .and_then(|v| {
                    v.iter().rev().find_map(|x| match x {
                        Value::Int(n) => Some(*n),
                        _ => None,
                    })
                }),
            _ => None,
        })
        .collect()
}

/// A skill's focus cost is its definition's line from rating 1 to the skill's cap, the rating held
/// between them, cut toward nothing to a whole point: 2017's Tidal Wave 32 at 44 (the line 32.9),
/// Living Armor 60 at 89 (60.9), Tanua's Gift 62 at 45; a Force's 10 whatever the rating; Portal
/// Recall 70 at 1 down to 1 at its cap of 10; a skill whose definition names none costs none.
/// Behaviour: server.skills.focus-cost-by-rating
#[test]
fn a_skills_focus_cost_follows_its_definitions_line_by_rating_cut_toward_nothing() {
    let line = |first, last| Usage {
        focus_add: (first, last),
        ..Usage::default()
    };
    let tidal_wave = line(-30.0, -40.0);
    assert_eq!(focus_cost(&tidal_wave, 150, 44.0), 32);
    assert_eq!(focus_cost(&tidal_wave, 150, 1.0), 30);
    assert_eq!(focus_cost(&tidal_wave, 150, 150.0), 40);
    assert_eq!(focus_cost(&tidal_wave, 150, 400.0), 40, "held at the cap");
    assert_eq!(focus_cost(&tidal_wave, 150, 0.0), 30, "held at 1");
    assert_eq!(focus_cost(&line(-55.0, -65.0), 150, 89.0), 60);
    assert_eq!(focus_cost(&line(-80.0, -40.0), 100, 45.0), 62);
    let force = line(-10.0, -10.0);
    assert_eq!(
        (focus_cost(&force, 10, 1.0), focus_cost(&force, 10, 7.0)),
        (10, 10)
    );
    let recall = line(-70.0, -1.0);
    assert_eq!(
        (focus_cost(&recall, 10, 1.0), focus_cost(&recall, 10, 10.0)),
        (70, 1)
    );
    assert_eq!(focus_cost(&line(-30.0, -30.0), 1, 1.0), 30, "a cap of 1");
    assert_eq!(focus_cost(&Usage::default(), 50, 20.0), 0);
}

/// A hero's use of a skill that costs focus pays it as the use is carried out, at its rating: the
/// vigor sent, then the focus less the cost (90 less 32 at a rating of 44), and no refusal.
/// Behaviour: server.skills.focus-cost-paid
#[test]
fn a_heros_use_pays_its_skills_focus_cost_at_its_rating_after_the_vigor() {
    let (mut h, b, me) = hero(true, 90);
    let world = *h
        .server
        .players
        .iter()
        .find(|(_, p)| p.character == me)
        .expect("in the world")
        .0;
    assert_eq!(
        yalain_world::skill_use::rating_of_for_test(&h.server, world, HEAL),
        44.0
    );
    let from = use_skill(&mut h, b, HEAL, 0);
    assert_eq!(refusals(&h, b, from), Vec::<u32>::new());
    let sent = vitals_sent(&h, b, from);
    let paid_at = sent
        .iter()
        .position(|&(q, _)| q == quality::FOCUS)
        .expect("its focus sent");
    assert_eq!(sent[paid_at], (quality::FOCUS, 58));
    assert!(
        sent[..paid_at].iter().any(|&(q, _)| q == quality::VIGOR),
        "the vigor first: {sent:?}"
    );
    assert_eq!(focus_of(&mut h, me), Some(58));
    // Used again with 58: 26 left.
    h.run(200);
    let from = use_skill(&mut h, b, HEAL, 0);
    assert!(vitals_sent(&h, b, from).contains(&(quality::FOCUS, 26)));
}

/// A use the player hasn't the focus for is refused with the client's reason ("You don't have
/// enough focus to use that skill."), nothing paid: a hero with 31 of the 32 the heal costs; a
/// strike at nothing, refused for the focus before its target; one who is no hero, whose focus
/// quality says 100, has none to spend.
/// Behaviour: server.skills.focus-cost-refused
#[test]
fn a_use_short_of_its_focus_cost_is_refused_with_the_clients_reason_before_its_target() {
    let (mut h, b, me) = hero(true, 31);
    let from = use_skill(&mut h, b, HEAL, 0);
    assert_eq!(refusals(&h, b, from), [refusal::NOT_ENOUGH_FOCUS]);
    assert_eq!(vitals_sent(&h, b, from), Vec::new(), "nothing paid");
    assert_eq!(focus_of(&mut h, me), Some(31));
    let from = use_skill(&mut h, b, STRIKE, 0);
    assert_eq!(refusals(&h, b, from), [refusal::NOT_ENOUGH_FOCUS]);
    // With focus to spare the same strike is refused for its target.
    quals::set_int(&mut player(&mut h, me).qualities, quality::FOCUS, 100);
    let from = use_skill(&mut h, b, STRIKE, 0);
    assert_eq!(refusals(&h, b, from), [refusal::TARGET_GONE]);

    let (mut h, b, _) = hero(false, 100);
    let from = use_skill(&mut h, b, HEAL, 0);
    assert_eq!(refusals(&h, b, from), [refusal::NOT_ENOUGH_FOCUS]);
}
