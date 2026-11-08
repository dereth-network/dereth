//! Resurrection skills: the offer a use at a dead player's corpse makes, taking it up (up where it
//! fell, nothing moved), turning it down, leaving it unanswered, and those it can't be made to.
//! Fixture: a skill and effect shaped as the data defines them, a caster who trained it, a dead
//! player with a sword in its pack.

use auberean_protocol::{queue, ServerMessage};
use auberean_script::args::Arg;
use yalain_testkit::play::server_call;
use yalain_testkit::Harness;
use yalain_world::calls::Value;
use yalain_world::death::Dying;
use yalain_world::effect_defs::{EffectDef, Kind, Scale};
use yalain_world::focus;
use yalain_world::player::quality;
use yalain_world::quals;
use yalain_world::resurrections::function;
use yalain_world::skills::{Hook, SkillDef, Usage};
use yalain_world::state::SkillState;
use yalain_world::world::Player;

const RAISE: u32 = 3101;
const RAISING: u32 = 0x7000_1FCE;
const FX: u32 = 0x4100_03BB;
const USE_SKILL: u32 = 0x8204_000B;
const USE_REFUSED: u32 = 0x8204_001A;
const RESURRECT: u32 = 0x8207_000E;
const DEATH_STATE: u32 = 0x8207_0033;
const MOVEMENT_FROZEN: u32 = 0x8212_0021;
const SWORD: u32 = 0x4700_051C;
const VITAE: u32 = 286;
const LEVEL: u32 = 302;
const FOCUS: u32 = 266;

fn with_raising(h: &mut Harness) {
    let usage = Usage {
        min_range: -1.0,
        max_range: 40.0,
        recovery: 10.0,
        min_use: 0.5,
        powerup: 1.0,
        powerup_behavior: 0x4100_00BF,
        vigor_add: -30.0,
        min_vigor: 1.0,
        implements_right: -2,
        implements_left: -1,
        valid_targets: 0x200_0020,
        behavior_other: [0x4100_00C9; 3],
        target_effects: vec![RAISING],
        ui_flags: 4800,
        hooks: vec![Hook {
            damages: true,
            flags: 8216,
            chance: 1.0,
            anim_hook: 1,
            ..Hook::default()
        }],
        ..Usage::default()
    };
    h.server.skill_table.skills.insert(
        RAISE,
        SkillDef {
            id: RAISE,
            cap: 100,
            table: 0x6F00_000B,
            flags: 0x51,
            active: true,
            usage: Some(usage),
            ..SkillDef::default()
        },
    );
    h.server.effect_table.effects.insert(
        RAISING,
        EffectDef {
            state: RAISING,
            kind: Kind::Resurrect {
                focus: Scale(vec![(1.0, 0.01), (100.0, 0.2)]),
                fx: FX,
            },
            duration: Scale::default(),
            priority: Scale::default(),
            fx: 0,
            external_flags: 18,
            internal_flags: 0x4140_0513,
            enum_value: 6100,
            eq_class: 0,
            name: None,
        },
    );
}

fn in_world(h: &mut Harness, account: &str, name: &str) -> (usize, u64) {
    let (b, me) = h.player_in_world(account, name);
    h.bots[b].send(
        true,
        queue::WEENIE,
        &auberean_protocol::ClientMessage::SceneRenderingComplete(auberean_protocol::login::Empty),
    );
    h.run(5);
    (b, me)
}

fn player(h: &mut Harness, me: u64) -> &mut Player {
    h.server
        .players
        .values_mut()
        .find(|p| p.character == me)
        .expect("in the world")
}

/// A player with the skill trained and vigor to spare.
fn caster(h: &mut Harness, account: &str, name: &str) -> (usize, u64) {
    let (b, me) = in_world(h, account, name);
    h.arm(b, me, SWORD);
    let p = player(h, me);
    quals::set_int(&mut p.qualities, quality::VIGOR_MAX, 400);
    quals::set_int(&mut p.qualities, quality::VIGOR, 400);
    p.play.skills.push(SkillState {
        skill: RAISE,
        granted: 0,
        mask: 0x1,
        cost: 2,
        children: 0,
        dependents: 0,
        spent: 0,
    });
    (b, me)
}

fn wait(h: &mut Harness, seconds: f64) {
    let t = h.now.0 + seconds;
    while h.now.0 < t {
        h.step();
    }
}

/// A player killed, with focus to lose, until its corpse is made: the corpse.
fn killed(h: &mut Harness, me: u64) -> u64 {
    let p = player(h, me);
    quals::set_int(&mut p.qualities, FOCUS, 90);
    let world = h
        .server
        .players
        .iter()
        .find(|(_, p)| p.character == me)
        .map(|(c, _)| *c)
        .expect("in the world");
    let clock = h.clock();
    yalain_world::death::player_dies(&mut h.server, &mut h.net, world, None, clock);
    wait(h, 11.0);
    corpse_of(h, me).expect("a corpse")
}

fn corpse_of(h: &mut Harness, me: u64) -> Option<u64> {
    player(h, me).dying.and_then(|d: Dying| d.corpse)
}

/// Use the skill at something, and let the use run.
fn raise(h: &mut Harness, b: usize, at: u64) -> usize {
    let from = h.bots[b].inbox.len();
    h.bots[b].send(
        true,
        queue::WEENIE,
        &server_call(USE_SKILL, &[Arg::Int(7), Arg::Int(RAISE), Arg::LongInt(at)]),
    );
    wait(h, 4.0);
    from
}

fn offers_since(h: &Harness, b: usize, from: usize) -> usize {
    h.calls(b, from)
        .iter()
        .filter(|(f, _)| *f == function::OFFERED)
        .count()
}

fn refused_with(h: &Harness, b: usize, from: usize) -> Option<u32> {
    h.calls(b, from)
        .iter()
        .find(|(f, _)| *f == USE_REFUSED)
        .and_then(|(_, v)| {
            v.iter().find_map(|x| match x {
                Value::Int(i) if *i != RAISE && *i != 7 => Some(*i),
                _ => None,
            })
        })
}

fn ask(h: &mut Harness, b: usize, function: u32) {
    h.bots[b].send(true, queue::WEENIE, &server_call(function, &[]));
    h.run(5);
}

/// The items a player holds, by id, template, how many, where.
fn held(h: &mut Harness, me: u64) -> Vec<(u64, u32, u32, u32)> {
    let mut v: Vec<_> = player(h, me)
        .play
        .items
        .iter()
        .map(|i| (i.id, i.template, i.quantity, i.slot))
        .collect();
    v.sort_unstable();
    v
}

/// Two players, one with the skill, the other dead with its corpse made and a stack and a sword.
fn scene(h: &mut Harness) -> (usize, u64, usize, u64, u64) {
    with_raising(h);
    let (a, ca) = caster(h, "one", "Elara");
    let (b, dead) = in_world(h, "two", "Babyfare");
    h.arm(b, dead, SWORD);
    let corpse = killed(h, dead);
    (a, ca, b, dead, corpse)
}

/// A use at a dead player's corpse offers it a return, from who, at the share the skill's effect
/// gives at its rating, with its look; nothing else happens until it answers.
/// Behaviour: server.resurrection.offered
#[test]
fn a_use_at_a_dead_players_corpse_offers_it_a_return_and_nothing_more() {
    let mut h = Harness::new();
    let (a, ca, b, dead, corpse) = scene(&mut h);
    let from = h.bots[b].inbox.len();
    let was = player(&mut h, dead).position;
    raise(&mut h, a, corpse);
    assert_eq!(offers_since(&h, b, from), 1, "offered once");
    let (_, v) = h
        .calls(b, from)
        .into_iter()
        .find(|(f, _)| *f == function::OFFERED)
        .expect("the offer");
    assert!(format!("{v:?}").contains("package: 6993"), "{v:?}");
    let p = player(&mut h, dead);
    assert!(p.dying.is_some(), "still dead");
    assert_eq!(p.position, was, "where it fell");
    assert!(p.trip.is_none());
    let o = p.offer.expect("the offer stands");
    assert_eq!((o.by, o.fx), (ca, FX));
    // At its rating of 1: the effect's first point.
    assert!((o.focus_share - 0.01).abs() < 1e-6, "{}", o.focus_share);
    assert!(h.server.things.get(corpse).is_some(), "its corpse stays");
}

/// Taking an offer up: the player gets up in that tick where it fell, with no trip and no protection,
/// 5 vitae less, three quarters of its health and vigor, focus no more than ten and the share, the
/// offer's look; and it may move a moment later. Its corpse stays to rot, and nothing it held moved.
/// Behaviour: server.resurrection.rises-where-it-fell
#[test]
fn a_player_taking_an_offer_up_gets_up_where_it_fell_holding_what_it_held() {
    let mut h = Harness::new();
    let (a, _, b, dead, corpse) = scene(&mut h);
    let items = held(&mut h, dead);
    assert!(!items.is_empty());
    quals::set_int(&mut player(&mut h, dead).qualities, LEVEL, 30);
    let fell = player(&mut h, dead).position;
    raise(&mut h, a, corpse);
    let from = h.bots[b].inbox.len();
    ask(&mut h, b, RESURRECT);
    let p = player(&mut h, dead);
    assert!(p.dying.is_none(), "up");
    assert!(p.trip.is_none(), "no trip");
    assert_eq!(p.position, fell, "where it fell");
    assert!(p.protected.is_none(), "no protection");
    assert!(p.offer.is_none());
    let vitae = quals::float(&p.qualities, VITAE).unwrap_or(0.0);
    assert!((vitae - 95.0).abs() < 1e-3, "{vitae}");
    assert_eq!(
        quals::int(&p.qualities, FOCUS),
        Some(11),
        "ten and 1 of 100"
    );
    let max = quals::int(&p.qualities, quality::HEALTH_MAX).unwrap_or(0);
    let health = quals::int(&p.qualities, quality::HEALTH).unwrap_or(0);
    assert!(health > 0 && health <= max * 3 / 4, "{health} of {max}");
    let msgs = h.world_messages(b, from);
    assert!(msgs
        .iter()
        .any(|m| matches!(m, ServerMessage::DoFx(e) if e.sender.id == dead && e.action.fx == FX)));
    assert!(h
        .calls(b, from)
        .iter()
        .any(|(f, v)| *f == DEATH_STATE && matches!(v.first(), Some(Value::Int(0)))));
    assert_eq!(held(&mut h, dead), items, "holds what it held");
    assert!(h.server.things.get(corpse).is_some(), "its corpse stays");
    let c = h.server.things.get(corpse).and_then(|t| t.corpse.as_ref());
    assert!(
        c.is_some_and(|c| c.contents.is_empty()),
        "and holds nothing"
    );
    // Up, asking again changes nothing: it does not come back twice.
    let vitae_then = quals::float(&player(&mut h, dead).qualities, VITAE);
    ask(&mut h, b, RESURRECT);
    wait(&mut h, 3.0);
    assert_eq!(
        quals::float(&player(&mut h, dead).qualities, VITAE),
        vitae_then
    );
    assert_eq!(player(&mut h, dead).position, fell);
    assert!(h
        .calls(b, from)
        .iter()
        .any(|(f, v)| *f == MOVEMENT_FROZEN && matches!(v.first(), Some(Value::Int(0)))));
}

/// An offer turned down is gone: the player stays dead, and its own request takes it to its lifestone
/// as ever, not up where it fell.
/// Behaviour: server.resurrection.turned-down
#[test]
fn a_player_turning_an_offer_down_stays_dead_and_goes_to_its_lifestone_when_it_asks() {
    let mut h = Harness::new();
    let (a, _, b, dead, corpse) = scene(&mut h);
    raise(&mut h, a, corpse);
    ask(&mut h, b, function::DECLINE);
    let p = player(&mut h, dead);
    assert!(p.dying.is_some(), "still dead");
    assert!(p.offer.is_none(), "the offer is gone");
    let vitae = quals::float(&p.qualities, VITAE);
    assert!(vitae.is_none() || vitae == Some(100.0), "{vitae:?}");
    ask(&mut h, b, RESURRECT);
    assert!(player(&mut h, dead).trip.is_some(), "through portal space");
}

/// A player left dead with an offer unanswered is sent back to its lifestone as ever: nobody is got up
/// where it fell who never agreed.
/// Behaviour: server.resurrection.left-unanswered
#[test]
fn an_offer_left_unanswered_is_not_taken_up_for_a_player_sent_back() {
    let mut h = Harness::new();
    let (a, _, _, dead, corpse) = scene(&mut h);
    raise(&mut h, a, corpse);
    assert!(player(&mut h, dead).offer.is_some());
    let world = h
        .server
        .players
        .iter()
        .find(|(_, p)| p.character == dead)
        .map(|(c, _)| *c)
        .expect("in the world");
    let clock = h.clock();
    yalain_world::death::sent_back(&mut h.server, &mut h.net, world, corpse, clock);
    let p = player(&mut h, dead);
    assert!(p.offer.is_none());
    assert!(p.trip.is_some(), "through portal space");
}

/// A corpse whose player can't be offered a return refuses the skill: its player back already, gone
/// from the world, dead again since (the old corpse), one with an offer standing, or no player's at
/// all; beyond the skill's range too. The dead hear nothing.
/// Behaviour: server.resurrection.who-can-be-offered
#[test]
fn a_corpse_refuses_the_skill_when_its_player_cannot_be_offered_a_return() {
    let mut h = Harness::new();
    let (a, ca, b, dead, corpse) = scene(&mut h);
    let d = player(&mut h, ca).position;
    // Too far.
    player(&mut h, ca).position.origin[0] += 300.0;
    let from = raise(&mut h, a, corpse);
    assert_eq!(refused_with(&h, a, from), Some(30_019), "too far");
    player(&mut h, ca).position = d;
    wait(&mut h, 11.0);
    // An offer stands: a second is refused, the first stands.
    raise(&mut h, a, corpse);
    wait(&mut h, 11.0);
    let first = player(&mut h, dead).offer.expect("the first");
    let (e, _) = caster(&mut h, "three", "Cedric");
    let from_b = h.bots[b].inbox.len();
    let from = raise(&mut h, e, corpse);
    assert_eq!(refused_with(&h, e, from), Some(20_002), "one stands");
    assert_eq!(offers_since(&h, b, from_b), 0);
    assert_eq!(player(&mut h, dead).offer, Some(first));
    // Its player back: a corpse left behind no longer raises anyone.
    ask(&mut h, b, RESURRECT);
    wait(&mut h, 11.0);
    let from_b = h.bots[b].inbox.len();
    let from = raise(&mut h, a, corpse);
    assert_eq!(refused_with(&h, a, from), Some(20_002), "already back");
    let again = killed(&mut h, dead);
    assert_eq!(offers_since(&h, b, from_b), 0);
    // Gone from the world.
    let world = h
        .server
        .players
        .iter()
        .find(|(_, p)| p.character == dead)
        .map(|(c, _)| *c)
        .expect("in the world");
    yalain_world::world::leave(&mut h.server, &mut h.net, world);
    wait(&mut h, 11.0);
    let from = raise(&mut h, a, again);
    assert_eq!(refused_with(&h, a, from), Some(20_002), "gone");
}

/// A corpse from an earlier death raises no one: its player, dead again, can only be offered a return
/// at the corpse of this death.
/// Behaviour: server.resurrection.who-can-be-offered
#[test]
fn an_earlier_deaths_corpse_raises_no_one() {
    let mut h = Harness::new();
    let (a, _, b, dead, corpse) = scene(&mut h);
    ask(&mut h, b, RESURRECT);
    wait(&mut h, 3.0);
    let again = killed(&mut h, dead);
    assert_ne!(again, corpse);
    assert!(
        h.server.things.get(corpse).is_some(),
        "the first still lies there"
    );
    let from_b = h.bots[b].inbox.len();
    let from = raise(&mut h, a, corpse);
    assert_eq!(refused_with(&h, a, from), Some(20_002));
    assert_eq!(offers_since(&h, b, from_b), 0);
    assert!(player(&mut h, dead).offer.is_none());
}

/// Who offered may leave, and the offer still stands on what it was: it needs nothing more from them.
/// Behaviour: server.resurrection.outlasts-who-offered
#[test]
fn an_offer_can_be_taken_up_after_who_offered_it_has_gone() {
    let mut h = Harness::new();
    let (a, ca, b, dead, corpse) = scene(&mut h);
    raise(&mut h, a, corpse);
    let world = h
        .server
        .players
        .iter()
        .find(|(_, p)| p.character == ca)
        .map(|(c, _)| *c)
        .expect("in the world");
    yalain_world::world::leave(&mut h.server, &mut h.net, world);
    h.run(3);
    ask(&mut h, b, RESURRECT);
    assert!(player(&mut h, dead).dying.is_none(), "up");
}

/// What a return leaves of focus: ten and the share of the greatest, up to the next whole point.
/// Behaviour: server.focus.resurrection-skill
#[test]
fn a_resurrection_skill_keeps_ten_and_its_share_of_the_greatest_focus() {
    assert_eq!(focus::back_with(100, 0.0), 10);
    assert_eq!(focus::back_with(100, 0.094_444_44), 20);
    assert_eq!(focus::back_with(100, 0.065_656_56), 17);
    assert_eq!(focus::back_with(100, 0.2), 30);
}
