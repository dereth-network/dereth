//! Toggled skills and their auras: the use that turns one on, its pulses (each a record of its own
//! on the players it reaches), the use that turns it off, a pulse that misses a player holding an
//! earlier one's record, and portal space turning it off. Fixture: a player with a sword in hand
//! and Espirit as the data defines it.

use auberean_protocol::{queue, ServerMessage};
use auberean_script::args::Arg;
use yalain_content::{ContentObject, ContentPack, Description, Facts, Kind, Place};
use yalain_testkit::play::{server_call, use_of};
use yalain_testkit::{Harness, ESPIRIT, ESPIRIT_AURA, ESPIRIT_PULSE};
use yalain_world::calls::{self, Node, Value};
use yalain_world::effects::{self, function as effect_fn, AURA_FLAGS, PULSE_FLAGS};
use yalain_world::player::quality;
use yalain_world::quals;
use yalain_world::skill_use::function;
use yalain_world::state::{EffectState, SkillState};
use yalain_world::world::Player;

const SWORD: u32 = 0x4700_051C;
const ATTACK_STATE: u32 = 0x8204_001B;
const SKILL_INFO: u32 = 0x8214_000D;
const SHOW_TEXT: u32 = 0x8205_0011;
const PULSE_FX: u32 = 0x4100_038F;
const PORTAL: u64 = 0x4002_9900_0000_0056;

/// A player in the world with the scene drawn, vigor to spare and a sword in hand; returns the
/// client and the player.
fn armed(h: &mut Harness, account: &str, name: &str) -> (usize, u64) {
    let (b, me) = h.player_in_world(account, name);
    h.bots[b].send(
        true,
        queue::WEENIE,
        &auberean_protocol::ClientMessage::SceneRenderingComplete(auberean_protocol::login::Empty),
    );
    h.run(5);
    h.arm(b, me, SWORD);
    let p = player(h, me);
    quals::set_int(&mut p.qualities, quality::VIGOR_MAX, 400);
    quals::set_int(&mut p.qualities, quality::VIGOR, 400);
    // It has trained Espirit.
    p.play.skills.push(SkillState {
        skill: ESPIRIT,
        granted: 0,
        mask: 0x1,
        cost: 1,
        children: 0,
        dependents: 0,
        spent: 0,
    });
    (b, me)
}

fn player(h: &mut Harness, me: u64) -> &mut Player {
    h.server
        .players
        .values_mut()
        .find(|p| p.character == me)
        .expect("in the world")
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

/// A record's call: its id, timeout, flags and categories.
fn record(v: &[Value]) -> Option<(u32, f32, u32, u32)> {
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
    Some((eid, f32::from_bits(w(6)), w(20), w(24)))
}

fn held(h: &mut Harness, me: u64) -> Vec<EffectState> {
    player(h, me).play.effects.clone()
}

/// Turn the aura on and let its use end; the messages since the request begin at the index
/// returned.
fn turned_on(h: &mut Harness, b: usize) -> usize {
    let from = use_skill(h, b, 1, ESPIRIT, 0);
    h.run(80);
    from
}

/// Espirit's use ends by turning its aura on, in 2017's order: its first pulse's record (three
/// minutes, pulsed), then the aura's own (held until taken off), not attacking, the toggle on, the
/// skill's information marked toggled, the pulse's appearance on the player.
/// Behaviour: server.auras.turned-on
#[test]
fn a_toggled_skill_turns_its_aura_on_as_its_use_ends() {
    let mut h = Harness::new();
    let (b, me) = armed(&mut h, "one", "Elara");
    let from = turned_on(&mut h, b);
    let got = calls_of(&h, b, from);
    let at = got
        .iter()
        .position(|(f, _)| *f == effect_fn::ADD)
        .expect("records added");
    let tail: Vec<u32> = got[at..].iter().map(|(f, _)| *f).take(5).collect();
    assert_eq!(
        tail,
        [
            effect_fn::ADD,
            effect_fn::ADD,
            ATTACK_STATE,
            effect_fn::TOGGLE,
            SKILL_INFO
        ]
    );
    let (pulse_eid, pulse_timeout, pulse_flags, _) = record(&got[at].1).expect("the pulse");
    let (aura_eid, aura_timeout, aura_flags, aura_categories) =
        record(&got[at + 1].1).expect("the aura");
    assert_eq!((pulse_timeout, pulse_flags), (180.0, PULSE_FLAGS));
    assert_eq!(
        (aura_timeout, aura_flags, aura_categories),
        (f32::MAX, AURA_FLAGS, 0x21)
    );
    assert!(
        aura_eid < pulse_eid,
        "the aura's id first, its record second"
    );
    assert_eq!(got[at + 2].1, [Value::Int(0)]);
    assert_eq!(got[at + 3].1, [Value::Int(1), Value::Int(ESPIRIT)]);
    let on = held(&mut h, me);
    assert!(on
        .iter()
        .any(|e| e.eid == aura_eid && e.state == ESPIRIT_AURA));
    assert!(on
        .iter()
        .any(|e| e.eid == pulse_eid && e.state == ESPIRIT_PULSE));
    let msgs = h.world_messages(b, from);
    let info = msgs
        .iter()
        .position(|m| matches!(m, ServerMessage::PlayerCall(c) if c.function.0 == SKILL_INFO))
        .expect("the information");
    let shown = msgs
        .iter()
        .position(
            |m| matches!(m, ServerMessage::DoFx(e) if e.sender.id == me && e.action.fx == PULSE_FX),
        )
        .expect("the appearance");
    assert!(info < shown);
    let mask = got
        .iter()
        .find(|(f, _)| *f == SKILL_INFO)
        .and_then(|(_, v)| match v.first() {
            Some(Value::Graph(g)) => match g.first() {
                Some((_, Node::Script { slots, .. })) => slots.get(7).map(|(_, w)| *w),
                _ => None,
            },
            _ => None,
        })
        .expect("the information's mask");
    assert_eq!(mask & 0x80, 0x80, "marked toggled: {mask:#x}");
}

/// The aura pulses again within one period and then every 121 to 124 seconds: each pulse a record
/// of its own that runs out after its three minutes, so two run at a time; the appearance is shown
/// once and kept while any runs.
/// Behaviour: server.auras.pulses
#[test]
fn an_aura_pulses_every_two_minutes_each_pulse_a_record_of_its_own() {
    let mut h = Harness::new();
    let (b, me) = armed(&mut h, "one", "Elara");
    let from = turned_on(&mut h, b);
    let mut pulses: Vec<f64> = Vec::new();
    let mut seen = h.bots[b].inbox.len();
    for _ in 0..600 {
        h.step_by(1.0);
        for (f, v) in calls_of(&h, b, seen) {
            if f == effect_fn::ADD && record(&v).is_some_and(|r| r.2 == PULSE_FLAGS) {
                pulses.push(h.now.0);
            }
        }
        seen = h.bots[b].inbox.len();
    }
    assert!(pulses.len() >= 4, "{pulses:?}");
    for w in pulses.windows(2).skip(1) {
        let gap = w[1] - w[0];
        assert!((120.0..=125.0).contains(&gap), "gap {gap}");
    }
    let live = held(&mut h, me)
        .iter()
        .filter(|e| e.state == ESPIRIT_PULSE)
        .count();
    assert!((1..=2).contains(&live), "{live} pulse records running");
    let msgs = h.world_messages(b, from);
    assert_eq!(
        msgs.iter()
            .filter(|m| matches!(m, ServerMessage::DoFx(e) if e.sender.id == me && e.action.fx == PULSE_FX))
            .count(),
        1
    );
    assert!(!msgs.iter().any(
        |m| matches!(m, ServerMessage::StopFx(e) if e.sender.id == me && e.action.0 == PULSE_FX)
    ));
}

/// Espirit used again while its aura is on turns it off at once: attacking, the use over, the
/// toggle off, the pulses' records then the aura's taken off, "You stop …", not attacking, the
/// appearance stopped.
/// Behaviour: server.auras.turned-off
#[test]
fn using_a_toggled_skill_again_turns_its_aura_off() {
    let mut h = Harness::new();
    let (b, me) = armed(&mut h, "one", "Elara");
    turned_on(&mut h, b);
    h.run(200);
    let records: Vec<u32> = held(&mut h, me).iter().map(|e| e.eid).collect();
    let from = use_skill(&mut h, b, 2, ESPIRIT, 0);
    h.run(5);
    let got = calls_of(&h, b, from);
    let names: Vec<u32> = got.iter().map(|(f, _)| *f).collect();
    let removes = records.len();
    let mut want = vec![ATTACK_STATE, function::ENDED, effect_fn::TOGGLE];
    want.extend(std::iter::repeat_n(effect_fn::REMOVE, removes));
    want.extend([SHOW_TEXT, ATTACK_STATE]);
    assert_eq!(names, want);
    assert_eq!(got[2].1, [Value::Int(0), Value::Int(ESPIRIT)]);
    let removed: Vec<Value> = got[3..3 + removes]
        .iter()
        .flat_map(|(_, v)| v.clone())
        .collect();
    let aura = records.iter().copied().min().expect("records");
    assert_eq!(
        removed.last(),
        Some(&Value::Int(aura)),
        "the aura's own last"
    );
    assert!(held(&mut h, me).is_empty());
    let msgs = h.world_messages(b, from);
    assert!(msgs.iter().any(
        |m| matches!(m, ServerMessage::StopFx(e) if e.sender.id == me && e.action.0 == PULSE_FX)
    ));
}

/// A pulse that does not reach a player who holds a record of an earlier pulse of the aura tells
/// that player's client so (`Effect_PulseMissed`, the record's id).
/// Behaviour: server.auras.pulse-missed
#[test]
fn a_player_an_auras_pulse_missed_is_told() {
    let mut h = Harness::new();
    let (a, ida) = armed(&mut h, "one", "Elara");
    let (b, idb) = armed(&mut h, "two", "Babyfare");
    turned_on(&mut h, a);
    // The other player holds a record of an earlier pulse of the aura (as a fellow it reached).
    let earlier = held(&mut h, ida)
        .into_iter()
        .find(|e| e.state == ESPIRIT_PULSE)
        .expect("the user's pulse record");
    let eid = 77;
    player(&mut h, idb)
        .play
        .effects
        .push(EffectState { eid, ..earlier });
    let from = h.bots[b].inbox.len();
    h.run(1); // nothing yet
    for _ in 0..130 {
        h.step_by(1.0);
    }
    let missed: Vec<Vec<Value>> = calls_of(&h, b, from)
        .into_iter()
        .filter(|(f, _)| *f == effect_fn::PULSE_MISSED)
        .map(|(_, v)| v)
        .collect();
    assert!(!missed.is_empty(), "told the pulse missed it");
    let Some(Value::Graph(g)) = missed[0].first() else {
        panic!("{missed:?}")
    };
    let Some((_, Node::Native { bytes, .. })) = g.first() else {
        panic!("{g:?}")
    };
    assert_eq!(&bytes[4..8], &eid.to_le_bytes(), "the record's id");
    assert!(calls_of(&h, a, 0)
        .iter()
        .all(|(f, _)| *f != effect_fn::PULSE_MISSED));
}

/// Put players in one fellowship, the first leading.
fn fellows(h: &mut Harness, ids: &[u64]) {
    use yalain_world::fellowship::{Fellow, Fellowship, Vitals};
    h.server.fellowships.list.push(Fellowship {
        name: "The Band".into(),
        leader: ids[0],
        flags: 0x1,
        room: 1,
        fellows: ids
            .iter()
            .map(|id| Fellow {
                id: *id,
                joined: 0.0,
                sent: Vitals::default(),
                next_check: f64::MAX,
            })
            .collect(),
    });
}

/// An aura's pulses reach its user's fellows within its range as they reach its user, each a
/// record of its own with its user as caster; a fellow gone beyond the range holding an earlier
/// pulse's record is told the next pulse missed it, and a player of no fellowship is not reached.
/// Behaviour: server.auras.reach-fellows
#[test]
fn an_auras_pulses_reach_fellows_within_its_range() {
    let mut h = Harness::new();
    let (a, ida) = armed(&mut h, "one", "Elara");
    let (b, idb) = armed(&mut h, "two", "Babyfare");
    let (c, _) = armed(&mut h, "three", "Cedric");
    fellows(&mut h, &[ida, idb]);
    turned_on(&mut h, a);
    let pulsed = |h: &Harness, who: usize, from: usize| -> Vec<u32> {
        calls_of(h, who, from)
            .into_iter()
            .filter(|(f, _)| *f == effect_fn::ADD)
            .filter_map(|(_, v)| record(&v))
            .filter(|r| r.2 == PULSE_FLAGS)
            .map(|r| r.0)
            .collect()
    };
    let from_b = h.bots[b].inbox.len();
    let from_c = h.bots[c].inbox.len();
    for _ in 0..130 {
        h.step_by(1.0);
    }
    assert!(!pulsed(&h, b, from_b).is_empty(), "the fellow is reached");
    assert!(pulsed(&h, c, from_c).is_empty(), "a stranger is not");
    let earlier = held(&mut h, idb)
        .into_iter()
        .find(|e| e.state == ESPIRIT_PULSE)
        .expect("the fellow holds a pulse's record");
    assert_eq!(earlier.caster, ida);
    // The fellow goes 400 m off: the next pulse misses it, and it is told so.
    player(&mut h, idb).position.origin[0] += 400.0;
    let from = h.bots[b].inbox.len();
    for _ in 0..130 {
        h.step_by(1.0);
    }
    assert!(
        pulsed(&h, b, from).is_empty(),
        "beyond the range, not reached"
    );
    assert!(
        calls_of(&h, b, from)
            .iter()
            .any(|(f, _)| *f == effect_fn::PULSE_MISSED),
        "told the pulse missed it"
    );
}

/// Going into portal space turns every aura off: the toggle off, its records taken off, the
/// appearance stopped.
/// Behaviour: server.auras.off-in-portal-space
#[test]
fn going_into_portal_space_turns_auras_off() {
    let mut h = Harness::new();
    let to = Place {
        cell: 0x7000_0001,
        origin: [10.0, 10.0, 50.0],
        rotation: [1.0, 0.0, 0.0, 0.0],
    };
    h.server.content = ContentPack {
        objects: vec![ContentObject {
            object: Some(PORTAL),
            template: 0x4700_1154,
            description: Description {
                name_text: None,
                package: 0x1A00,
                look: 0x1F00_0ED5,
                enter_fx: 0,
                scale: None,
                name: None,
                numbers: [0; 6],
                source: "authored for a test".into(),
            },
            cell: 0x0299_0109,
            origin: [61.0, 240.2, -44.6],
            rotation: [1.0, 0.0, 0.0, 0.0],
            kind: Kind::Static,
            label: String::new(),
            placed: "authored for a test".into(),
            destination: Some(to),
            facts: Facts::default(),
            walks: None,
            gate: None,
        }],
        ..ContentPack::default()
    };
    let (b, me) = armed(&mut h, "one", "Elara");
    turned_on(&mut h, b);
    assert!(effects::aura_held(&h.server, player_conn(&h, me), ESPIRIT_AURA).is_some());
    let from = h.bots[b].inbox.len();
    h.bots[b].send(
        true,
        queue::WEENIE,
        &server_call(yalain_world::requests::function::USE, &[use_of(PORTAL, me)]),
    );
    h.run(10);
    let got: Vec<u32> = calls_of(&h, b, from).iter().map(|(f, _)| *f).collect();
    let entered = got
        .iter()
        .position(|f| *f == yalain_world::places::function::ENTER_PORTAL_SPACE)
        .expect("into portal space");
    assert_eq!(got[entered + 1], effect_fn::TOGGLE);
    assert!(got[entered + 2..].iter().all(|f| *f == effect_fn::REMOVE));
    assert!(held(&mut h, me).is_empty());
    assert!(h.world_messages(b, from).iter().any(
        |m| matches!(m, ServerMessage::StopFx(e) if e.sender.id == me && e.action.0 == PULSE_FX)
    ));
}

/// A resurrection's portal space turns no aura off. Its pulse's record goes at the request, as what a
/// death ends, and three seconds after the player is back the aura pulses again.
/// Behaviour: server.auras.through-a-resurrection
#[test]
fn a_resurrection_leaves_auras_on_and_they_pulse_again_as_it_comes_back() {
    let mut h = Harness::new();
    let (b, me) = armed(&mut h, "one", "Elara");
    turned_on(&mut h, b);
    let world = player_conn(&h, me);
    let clock = h.clock();
    yalain_world::death::player_dies(&mut h.server, &mut h.net, world, None, clock);
    let t = h.now.0 + 11.0;
    while h.now.0 < t {
        h.step();
    }
    let from = h.bots[b].inbox.len();
    h.bots[b].send(
        true,
        queue::WEENIE,
        &server_call(yalain_world::death::player_fn::RESURRECT, &[]),
    );
    assert!(h.run_until(400, |h| h.server.players[&world].dying.is_none()));
    let back = h.now.0;
    assert!(effects::aura_held(&h.server, world, ESPIRIT_AURA).is_some());
    assert!(
        calls_of(&h, b, from)
            .iter()
            .all(|(f, _)| *f != effect_fn::TOGGLE),
        "its toggle stays on"
    );
    assert!(
        h.server.players[&world]
            .play
            .effects
            .iter()
            .all(|e| e.flags != PULSE_FLAGS),
        "its pulses went at the request"
    );
    while h.now.0 < back + 2.9 {
        h.step();
    }
    let mark = h.bots[b].inbox.len();
    while h.now.0 < back + 3.2 {
        h.step();
    }
    assert!(
        calls_of(&h, b, mark)
            .iter()
            .any(|(f, _)| *f == effect_fn::ADD),
        "pulsed again"
    );
}

fn player_conn(h: &Harness, me: u64) -> yalain_net::ConnId {
    h.server
        .players
        .iter()
        .find(|(_, p)| p.character == me)
        .map(|(c, _)| *c)
        .expect("in the world")
}

/// An aura for what is held: its effects, lowering a thing's cost and a weapon's delay, and the
/// aura itself turned on for `me` as its use does, all three made up for the test.
fn aura_on_what_is_held(h: &mut Harness, me: u64, targets: u32) -> u32 {
    use yalain_world::effect_defs::{numeric, Aura, EffectDef, Kind, Scale};
    use yalain_world::made::stat;
    const COST: u32 = 0x6F00_7781;
    const DELAY: u32 = 0x6F00_7782;
    const AURA: u32 = 0x6F00_7783;
    let like: EffectDef = h
        .server
        .effect_table
        .get(ESPIRIT_PULSE)
        .cloned()
        .expect("a pulse");
    let lowers = |state: u32, on: u32, by: f32| EffectDef {
        state,
        kind: Kind::Numeric {
            flags: numeric::MULTIPLY,
            stats: vec![on],
            magnitude: Scale(vec![(1.0, by + 0.1), (50.0, by)]),
        },
        name: Some((0xA860_7CEC, 0x2500_0708)),
        ..like.clone()
    };
    let aura = EffectDef {
        state: AURA,
        kind: Kind::Aura(Aura {
            pulsed: vec![COST, DELAY],
            targets,
            range: 300.0,
        }),
        ..like.clone()
    };
    for d in [
        lowers(COST, stat::VIGOR, 0.72),
        lowers(DELAY, stat::COMBAT_DELAY, 0.8),
        aura,
    ] {
        h.server.effect_table.effects.insert(d.state, d);
    }
    let world = *h
        .server
        .players
        .iter()
        .find(|(_, p)| p.character == me)
        .expect("in the world")
        .0;
    let clock = h.clock();
    effects::aura_on(&mut h.server, &mut h.net, world, AURA, 0, 50.0, clock);
    AURA
}

/// A player in the world holding a weapon whose template names its cost and delay, in the world
/// with the scene drawn; its client, itself and the weapon.
fn holding_a_blade(h: &mut Harness) -> (usize, u64, u64) {
    const BLADE: u32 = 0x4700_77A1;
    let (b, me) = h.player_in_world("one", "Elara");
    h.bots[b].send(
        true,
        queue::WEENIE,
        &auberean_protocol::ClientMessage::SceneRenderingComplete(auberean_protocol::login::Empty),
    );
    h.run(5);
    let mut p = yalain_content::Props::default();
    p.set("Weapon_VigorCost", yalain_content::Prop::Int(18));
    p.set("Weapon_Damage", yalain_content::Prop::Int(100));
    p.set("Weapon_Speed", yalain_content::Prop::Int(18));
    h.server
        .catalog
        .insert(yalain_testkit::test_template(BLADE, 0x1067, p));
    h.server
        .catalog
        .implements
        .insert(BLADE, yalain_world::skill_use::implement::BOW);
    h.arm(b, me, BLADE);
    let blade = player(h, me)
        .play
        .items
        .iter()
        .find(|i| i.template == BLADE)
        .expect("in hand")
        .id;
    (b, me, blade)
}

/// An aura for what its user holds puts no record on the user beyond its own, however long it runs;
/// what it lowers stays lowered between its pulses, and goes when the aura does.
/// Behaviour: server.auras.held-pulses-last-while-held
#[test]
fn an_aura_on_what_is_held_keeps_its_pulses_off_the_player() {
    let mut h = Harness::new();
    let (_, me, _) = holding_a_blade(&mut h);
    let cost = |h: &Harness| yalain_world::skill_use::held_vigor(&h.server, me).weapons;
    assert_eq!(cost(&h), 18);
    let aura = aura_on_what_is_held(&mut h, me, yalain_world::effect_defs::aura_target::ITEM);
    let records = |h: &mut Harness| -> Vec<u32> {
        player(h, me).play.effects.iter().map(|e| e.state).collect()
    };
    assert_eq!(records(&mut h), [aura], "the aura's own record alone");
    assert_eq!(cost(&h), 12, "18 at 0.72, rounded down");
    // Past a pulse's own three minutes.
    #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
    h.run((400.0 / yalain_testkit::STEP) as usize);
    assert_eq!(records(&mut h), [aura], "and no record of a pulse since");
    assert_eq!(cost(&h), 12, "still lowered");
    player(&mut h, me).play.effects.clear();
    assert_eq!(cost(&h), 18, "gone with the aura");
}

/// An aura for weapons holds nothing on a shield, one for items on both; a thing no longer in hand
/// holds nothing.
/// Behaviour: server.auras.held-pulses-last-while-held
#[test]
fn an_aura_names_the_kind_of_thing_it_holds_on() {
    use yalain_world::effect_defs::aura_target as to;
    let mut h = Harness::new();
    let (_, me, blade) = holding_a_blade(&mut h);
    aura_on_what_is_held(&mut h, me, to::WEAPON);
    let on =
        |h: &Harness, item: u64, shield: bool| effects::on_held(&h.server, me, item, shield).len();
    assert_eq!(on(&h, blade, false), 2, "a weapon");
    assert_eq!(on(&h, blade, true), 0, "not a shield");
    player(&mut h, me).play.effects.clear();
    aura_on_what_is_held(&mut h, me, to::SHIELD);
    assert_eq!(on(&h, blade, false), 0);
    assert_eq!(on(&h, blade, true), 2);
    assert_eq!(on(&h, 0x8130_0000_0000_0001, true), 0, "not in hand");
}

/// A weapon's examination lists what an aura holds on it, under its own heading with its spellcraft,
/// and shows its cost and delay as they stand, rounded down; out of the aura, as they were.
/// Behaviour: server.auras.pulses-on-what-is-held
#[test]
fn a_weapons_examination_shows_what_an_aura_holds_on_it() {
    use yalain_world::profile::When;
    let mut h = Harness::new();
    let (_, me, blade) = holding_a_blade(&mut h);
    let view = |h: &Harness| {
        let t = h.server.things.get(blade).expect("the weapon");
        yalain_world::requests::item_view(&h.server, t)
    };
    let plain = view(&h).weapon.expect("a weapon's numbers");
    assert_eq!((plain.2, plain.3), (18, 18));
    aura_on_what_is_held(&mut h, me, yalain_world::effect_defs::aura_target::ITEM);
    let v = view(&h);
    let w = v.weapon.expect("a weapon's numbers");
    assert_eq!((w.2, w.3), (14, 12), "18 at 0.8 and at 0.72");
    assert_eq!(w.0, plain.0, "its damage not its concern");
    let listed: Vec<Option<i32>> = v
        .effects
        .iter()
        .filter(|e| e.when == When::Enchantment)
        .map(|e| e.spellcraft)
        .collect();
    assert_eq!(listed, [Some(50), Some(50)]);
    player(&mut h, me).play.effects.clear();
    let v = view(&h);
    assert!(v.effects.iter().all(|e| e.when != When::Enchantment));
    assert_eq!(v.weapon.map(|w| (w.2, w.3)), Some((18, 18)));
}

/// What an aura holds on a weapon changes the delay its examination shows, not the time scale its
/// holder swings at.
/// Behaviour: server.weapons.time-scale-follows-delay
#[test]
fn an_aura_lowering_a_weapons_delay_does_not_quicken_its_holder() {
    let mut h = Harness::new();
    let (_, me, blade) = holding_a_blade(&mut h);
    let world = *h.server.players.keys().next().expect("in the world");
    aura_on_what_is_held(&mut h, me, yalain_world::effect_defs::aura_target::ITEM);
    let t = h.server.things.get(blade).expect("the weapon");
    let shown = yalain_world::requests::item_view(&h.server, t).weapon;
    assert_eq!(shown.map(|w| w.2), Some(14));
    assert_eq!(yalain_world::combat::delay_worn(&h.server, world), 18);
}

/// An aura whose pulse changes a weapon's damage, its scale running to `at_50`; turned on for
/// `user` at this spellcraft. Made up for the test.
fn aura_on_damage(h: &mut Harness, user: u64, multiply: bool, at_50: f32, spellcraft: f32) -> u32 {
    use yalain_world::effect_defs::{aura_target, numeric, Aura, EffectDef, Kind, Scale};
    const DAMAGE: u32 = 0x6F00_7791;
    const AURA: u32 = 0x6F00_7792;
    let like: EffectDef = h
        .server
        .effect_table
        .get(ESPIRIT_PULSE)
        .cloned()
        .expect("a pulse");
    let changes = EffectDef {
        state: DAMAGE,
        kind: Kind::Numeric {
            flags: if multiply { numeric::MULTIPLY } else { 0 },
            stats: vec![yalain_world::made::stat::DAMAGE],
            magnitude: Scale(vec![(1.0, at_50 / 8.0), (50.0, at_50)]),
        },
        name: Some((0xA860_7CEC, 0x2500_0708)),
        ..like.clone()
    };
    let aura = EffectDef {
        state: AURA,
        kind: Kind::Aura(Aura {
            pulsed: vec![DAMAGE],
            targets: aura_target::WEAPON,
            range: 300.0,
        }),
        ..like
    };
    for d in [changes, aura] {
        h.server.effect_table.effects.insert(d.state, d);
    }
    let world = world_of(h, user);
    let clock = h.clock();
    effects::aura_on(&mut h.server, &mut h.net, world, AURA, 0, spellcraft, clock);
    DAMAGE
}

fn world_of(h: &Harness, id: u64) -> yalain_net::ConnId {
    *h.server
        .players
        .iter()
        .find(|(_, p)| p.character == id)
        .expect("in the world")
        .0
}

/// The blade given damage of its own: `most`, and a quarter of it its span.
fn with_damage(h: &mut Harness, blade: u64, most: i32) {
    h.server.things.get_mut(blade).expect("the weapon").own.made = Some(yalain_world::made::Made {
        damage: Some((most, 0.25)),
        delay: Some(18),
        vigor: Some(18),
        ..Default::default()
    });
}

/// What a blow of the player's reckons and what its examination shows of the blade.
fn reckoned(h: &Harness, me: u64, blade: u64) -> (i32, i32, i32) {
    let p = h
        .server
        .players
        .values()
        .find(|p| p.character == me)
        .expect("in the world");
    let w = yalain_world::combat::hitter(&h.server, p)
        .weapon
        .expect("a weapon");
    let t = h.server.things.get(blade).expect("the weapon");
    let shown = yalain_world::requests::item_view(&h.server, t)
        .weapon
        .expect("a weapon's numbers");
    (w.most, w.least(), shown.0)
}

/// What an aura adds to a weapon's damage counts in its blows and shows in its examination, the
/// least following from the greatest by the weapon's span, while the aura is on and it is in hand.
/// Behaviour: server.auras.damage-held-on-a-weapon
#[test]
fn what_an_aura_adds_to_a_weapons_damage_counts_in_its_blows_and_its_examination() {
    let mut h = Harness::new();
    let (_, me, blade) = holding_a_blade(&mut h);
    with_damage(&mut h, blade, 240);
    assert_eq!(reckoned(&h, me, blade), (240, 180, 240));
    aura_on_damage(&mut h, me, false, 80.0, 50.0);
    assert_eq!(reckoned(&h, me, blade), (320, 240, 320), "80 at 50");
    let p = player(&mut h, me);
    p.play.effects.retain(|e| e.flags != AURA_FLAGS);
    assert_eq!(
        reckoned(&h, me, blade),
        (240, 180, 240),
        "off with the aura"
    );
    aura_on_damage(&mut h, me, false, 80.0, 50.0);
    h.server.things.get_mut(blade).expect("the weapon").wielded = None;
    assert_eq!(
        yalain_world::combat::weapon_damage(&h.server, blade).map(|w| w.most),
        Some(240),
        "out of hand"
    );
}

/// An aura that multiplies a weapon's damage does so, rounded down.
/// Behaviour: server.auras.damage-held-on-a-weapon
#[test]
fn what_an_aura_multiplies_a_weapons_damage_by_counts_rounded_down() {
    let mut h = Harness::new();
    let (_, me, blade) = holding_a_blade(&mut h);
    with_damage(&mut h, blade, 54);
    aura_on_damage(&mut h, me, true, 1.15, 50.0);
    assert_eq!(reckoned(&h, me, blade), (62, 46, 62), "54 at 1.15");
}

/// An aura for weapons holds on what a fellow within its range has in hand, in its blows and its
/// examination; not on a stranger's, and not beyond its range.
/// Behaviour: server.auras.held-reach-fellows
#[test]
fn an_aura_for_weapons_holds_on_a_fellows_weapon_within_its_range() {
    let mut h = Harness::new();
    let (_, me, blade) = holding_a_blade(&mut h);
    with_damage(&mut h, blade, 240);
    let (_, caster) = armed(&mut h, "two", "Babyfare");
    let (_, stranger) = armed(&mut h, "three", "Cedric");
    aura_on_damage(&mut h, caster, false, 80.0, 50.0);
    fellows(&mut h, &[me, caster]);
    assert_eq!(reckoned(&h, me, blade), (320, 240, 320), "a fellow's aura");
    player(&mut h, caster).position.origin[0] += 400.0;
    assert_eq!(reckoned(&h, me, blade), (240, 180, 240), "beyond its range");
    player(&mut h, caster).position.origin[0] -= 400.0;
    assert_eq!(reckoned(&h, me, blade), (320, 240, 320), "back in range");
    player(&mut h, me).scene_drawn = false;
    assert_eq!(
        reckoned(&h, me, blade),
        (240, 180, 240),
        "its holder still coming in"
    );
    player(&mut h, me).scene_drawn = true;
    let trip = yalain_world::places::Trip {
        to: player(&mut h, me).position,
        used: None,
        step: None,
        resurrect: false,
        sent: false,
        said: None,
    };
    player(&mut h, me).trip = Some(trip);
    assert_eq!(
        reckoned(&h, me, blade),
        (240, 180, 240),
        "its holder in portal space"
    );
    player(&mut h, me).trip = None;
    h.server.fellowships.list.clear();
    fellows(&mut h, &[me, stranger]);
    assert_eq!(reckoned(&h, me, blade), (240, 180, 240), "not a fellow");
}

/// Of the same effect from two auras on one weapon, the stronger holds, once.
/// Behaviour: server.auras.held-pulses-last-while-held
#[test]
fn of_one_effect_from_two_auras_the_stronger_holds_once() {
    let mut h = Harness::new();
    let (_, me, blade) = holding_a_blade(&mut h);
    with_damage(&mut h, blade, 240);
    let (_, caster) = armed(&mut h, "two", "Babyfare");
    fellows(&mut h, &[me, caster]);
    aura_on_damage(&mut h, me, false, 80.0, 1.0);
    aura_on_damage(&mut h, caster, false, 80.0, 50.0);
    assert_eq!(effects::on_held(&h.server, me, blade, false).len(), 1);
    assert_eq!(reckoned(&h, me, blade).0, 320, "80 at 50, not 10 at 1");
}

/// A weapon under both kinds of aura is multiplied first, what is added coming after.
/// Behaviour: server.auras.held-pulses-last-while-held
#[test]
fn a_weapon_under_both_kinds_of_aura_is_multiplied_first() {
    let mut h = Harness::new();
    let (_, me, _) = holding_a_blade(&mut h);
    let adds = aura_on_damage(&mut h, me, false, 80.0, 50.0);
    let adding = h.server.effect_table.get(adds).cloned().expect("defined");
    let multiplying = aura_on_damage(&mut h, me, true, 1.5, 50.0);
    let times = h
        .server
        .effect_table
        .get(multiplying)
        .cloned()
        .expect("defined");
    // The helper names one effect; the two kinds under names of their own.
    let (a, m) = (0x6F00_7793, 0x6F00_7794);
    for (state, d) in [(a, adding), (m, times)] {
        h.server
            .effect_table
            .effects
            .insert(state, yalain_world::effect_defs::EffectDef { state, ..d });
    }
    assert_eq!(
        effects::held_damage(&h.server, &[(a, 50.0), (m, 50.0)], 100),
        230,
        "100 x 1.5 + 80"
    );
}
