//! Healing another's vitae: a drawn vitae restore gives whole points between its least and its most,
//! never above full, and a harmless skill that takes only other players lands on one in a place of
//! peace and is refused at its user. Fixture: two players side by side in the test world, a sword in
//! the healer's hand, a made-up skill with Ease Memories' kinds of target and flags, its effect a
//! drawn restore of 1 to 8 points.

use auberean_protocol::{queue, ServerMessage};
use auberean_script::args::Arg;
use yalain_net::ConnId;
use yalain_testkit::play::server_call;
use yalain_testkit::{Harness, EASE_SUFFERING};
use yalain_world::calls::{self, Value};
use yalain_world::death::vitae_q::VITAE;
use yalain_world::effect_defs::{Kind, VitaeRestore};
use yalain_world::player::quality;
use yalain_world::skill_use::{function, refusal};
use yalain_world::{effects, quals};

const SWORD: u32 = 0x4700_051C;
/// The made-up skill and its effect.
const EASE: u32 = 296;
const RESTORE: u32 = 0x7000_0837;
const FX: u32 = 0x4100_00C0;

/// A drawn restore of 1 to 8 points as the test world's effect `RESTORE`, and a skill `EASE` taking
/// other players alone (no harm marked, doing no damage) whose effect it is.
fn with_the_skill(h: &mut Harness) {
    let tap = h
        .server
        .effect_table
        .effects
        .get(&yalain_testkit::VITAE_TAP)
        .cloned()
        .expect("the test world's tap");
    h.server.effect_table.effects.insert(
        RESTORE,
        yalain_world::effect_defs::EffectDef {
            state: RESTORE,
            kind: Kind::VitaeRestore(VitaeRestore {
                points: 1.0,
                most: Some(8.0),
                fellows: false,
            }),
            fx: FX,
            ..tap
        },
    );
    let mut def = h.server.skill_table.skills[&EASE_SUFFERING].clone();
    def.id = EASE;
    let u = def.usage.as_mut().expect("a use");
    u.valid_targets = 0x100;
    u.ui_flags = 0x12C0;
    u.target_effects = vec![RESTORE];
    h.server.skill_table.skills.insert(EASE, def);
    h.all_hold(EASE);
}

fn player(h: &mut Harness, account: &str, name: &str) -> (usize, u64) {
    let (b, me) = h.player_in_world(account, name);
    h.bots[b].send(
        true,
        queue::WEENIE,
        &auberean_protocol::ClientMessage::SceneRenderingComplete(auberean_protocol::login::Empty),
    );
    h.run(5);
    (b, me)
}

fn world_of(h: &Harness, me: u64) -> ConnId {
    *h.server
        .players
        .iter()
        .find(|(_, p)| p.character == me)
        .map(|(c, _)| c)
        .expect("in the world")
}

fn vitae(h: &Harness, me: u64) -> f32 {
    quals::float(&h.server.players[&world_of(h, me)].qualities, VITAE).unwrap_or(100.0)
}

fn set_vitae(h: &mut Harness, me: u64, v: f32) {
    let w = world_of(h, me);
    let p = h.server.players.get_mut(&w).expect("in the world");
    quals::set_float(&mut p.qualities, VITAE, v);
}

/// Use a skill on a target, as the client packs it; the refusal's code, if one came.
fn use_on(h: &mut Harness, b: usize, id: u32, target: u64) -> Option<u32> {
    let from = h.bots[b].inbox.len();
    h.bots[b].send(
        true,
        queue::WEENIE,
        &server_call(
            function::USE,
            &[Arg::Int(id), Arg::Int(EASE), Arg::LongInt(target)],
        ),
    );
    h.run(60);
    h.world_messages(b, from).iter().find_map(|m| match m {
        ServerMessage::PlayerCall(c) if c.function.0 == function::REFUSED => c
            .args
            .as_ref()
            .and_then(|a| calls::values(&a.0).ok())
            .and_then(|v| match v.last() {
                Some(Value::Int(code)) => Some(*code),
                _ => None,
            }),
        _ => None,
    })
}

/// A restore drawn from 1 to 8 gives a whole number of points each time it is put on, every one
/// of 1 to 8 over many, never more, and never above full: at 99.5 it stops at 100.
/// Behaviour: server.effects.vitae-restore-drawn
#[test]
fn a_drawn_vitae_restore_gives_whole_points_from_its_least_to_its_most_never_above_full() {
    let mut h = Harness::new();
    with_the_skill(&mut h);
    let (_, me) = player(&mut h, "one", "Elara");
    let world = world_of(&h, me);
    let mut seen = std::collections::BTreeSet::new();
    for _ in 0..200 {
        set_vitae(&mut h, me, 50.0);
        let clock = h.clock();
        assert!(effects::apply(
            &mut h.server,
            &mut h.net,
            world,
            RESTORE,
            effects::Cast::default(),
            clock
        ));
        let gained = vitae(&h, me) - 50.0;
        assert!(
            gained.fract().abs() < f32::EPSILON && (1.0..=8.0).contains(&gained),
            "{gained}"
        );
        #[allow(clippy::cast_possible_truncation)]
        seen.insert(gained as i32);
    }
    assert_eq!(seen, (1..=8).collect(), "every amount from 1 to 8");
    set_vitae(&mut h, me, 99.5);
    let clock = h.clock();
    effects::apply(
        &mut h.server,
        &mut h.net,
        world,
        RESTORE,
        effects::Cast::default(),
        clock,
    );
    assert!(
        (vitae(&h, me) - 100.0).abs() < f32::EPSILON,
        "full and no more"
    );
}

/// A skill that does no damage, is marked harmless and takes only other players (as Ease Memories
/// and Calm Vitae do) lands on another player where no one may harm another: its vitae rises by
/// a whole 1 to 8. Aimed at its user it is refused (on yourself), and the user's vitae is untouched.
/// Behaviour: server.skills.harmless-skill-on-others-lands
#[test]
fn a_harmless_skill_for_others_alone_lands_on_another_player_and_is_refused_at_its_user() {
    let mut h = Harness::new();
    with_the_skill(&mut h);
    let (b, me) = player(&mut h, "one", "Elara");
    let (_, you) = player(&mut h, "two", "Bran");
    h.arm(b, me, SWORD);
    let w = world_of(&h, me);
    for (k, v) in [(quality::VIGOR_MAX, 2000), (quality::VIGOR, 2000)] {
        quals::set_int(
            &mut h
                .server
                .players
                .get_mut(&w)
                .expect("in the world")
                .qualities,
            k,
            v,
        );
    }
    set_vitae(&mut h, you, 80.0);
    set_vitae(&mut h, me, 80.0);
    assert_eq!(use_on(&mut h, b, 1, you), None, "no refusal");
    let gained = vitae(&h, you) - 80.0;
    assert!(
        gained.fract().abs() < f32::EPSILON && (1.0..=8.0).contains(&gained),
        "the other's vitae rose: {gained}"
    );
    h.run(400);
    assert_eq!(use_on(&mut h, b, 2, me), Some(refusal::ON_YOURSELF));
    assert!(
        (vitae(&h, me) - 80.0).abs() < f32::EPSILON,
        "its user's vitae untouched"
    );
}
