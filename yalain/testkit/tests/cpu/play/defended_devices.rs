//! Defended devices: a creature of a template a world's row names keeps attackers about it, which
//! go at it, each killed one replaced after the row's time; once it has stood its time it goes and
//! what the row names stands in its place; killed first, it leaves nothing. Fixture: the test hall
//! beside where a character starts, a camp keeping one made-up device, made-up attackers, and a
//! made-up marker the device leaves.

use auberean_protocol::ServerMessage;
use yalain_content::defenses::{Defense, DefenseTable};
use yalain_content::Prop;
use yalain_testkit::Harness;
use yalain_world::calls::{self, Node};
use yalain_world::things::{Kind, Mode};

const DEVICE: u32 = 0x4700_29A1;
const ATTACKER: u32 = 0x4700_2230;
const MARKER: u32 = 0x4700_2231;
const HALL: u32 = 0x0299_0109;
const DEVICE_AT: [f32; 3] = [70.0, 250.0, -44.6];
/// How long the device must stand, and how long a killed attacker takes to come back.
const HOLDS: f64 = 60.0;
const REGEN: f64 = 20.0;
/// Its lines (in its table): as it comes, half way, badly hurt, as it holds.
const TABLE: u32 = 0x2500_26BE;
const COMES: u32 = 0x8B69_133E;
const HUMS: u32 = 0xBB6B_B931;
const HURT: u32 = 0x2FA8_FC8E;
const HOLDS_LINE: u32 = 0xFF1D_266D;
const SAYING: u32 = 0x8205_0016;

/// The hall with a camp keeping the device (of this health), the attackers its row keeps and the
/// marker it leaves; a player in the world: its client.
fn hall(device_health: i64) -> (Harness, usize) {
    let mut h = Harness::new();
    h.server.catalog = yalain_testkit::test_catalog();
    let mut d = yalain_content::Props::default();
    d.set("GameplayStatistics_Level", Prop::Int(1))
        .set("GameplayStatistics_MaxHealth", Prop::Int(device_health))
        .set("AI_MovementType", Prop::Word(0x4000_0002))
        .set("AICombat_NaturalDamage", Prop::Float(1.0));
    h.server
        .catalog
        .insert(yalain_testkit::test_template(DEVICE, 0x2838, d));
    let mut a = yalain_content::Props::default();
    a.set("GameplayStatistics_Level", Prop::Int(40))
        .set("GameplayStatistics_MaxHealth", Prop::Int(1_000))
        .set("AICombat_NaturalDamage", Prop::Float(80.0))
        .set("AICombat_NaturalVariance", Prop::Float(0.2))
        .set("AI_PerceptionRadius", Prop::Float(20.0));
    h.server
        .catalog
        .insert(yalain_testkit::test_template(ATTACKER, 0x1169, a));
    h.server.catalog.insert(yalain_testkit::test_template(
        MARKER,
        0x200,
        yalain_content::Props::default(),
    ));
    h.server.content.defenses = DefenseTable {
        rows: vec![Defense {
            device: DEVICE,
            holds: HOLDS,
            spawns: vec![(ATTACKER, 1)],
            population: 2,
            regen: Some(REGEN),
            radius: 6.0,
            then: Some(MARKER),
            then_lasts: Some(30),
            says: vec![(0.0, (COMES, TABLE)), (HOLDS / 2.0, (HUMS, TABLE))],
            hurt_line: Some((HURT, TABLE)),
            held_line: Some((HOLDS_LINE, TABLE)),
        }],
    };
    // The device comes once the player is in the world, so its client sees it come.
    let (b, _) = h.player_in_world("one", "Elara");
    h.server.spawns.camps.push(yalain_content::Camp {
        id: "device".into(),
        cell: HALL,
        origin: DEVICE_AT,
        radius: 0.0,
        population: 1,
        regen: 100_000.0,
        templates: vec![(DEVICE, 1)],
        spots: vec![(HALL, DEVICE_AT)],
        placed: "authored for the tests".into(),
        confidence: "authored".into(),
        moves: yalain_content::CampMoves::default(),
        name: None,
    });
    h.server
        .camps
        .push(yalain_world::spawn::CampState::default());
    (h, b)
}

/// The world steps until the camp's device has come: it.
fn comes(h: &mut Harness) -> u64 {
    for _ in 0..40 {
        h.step();
        if let [device] = living(h, DEVICE)[..] {
            return device;
        }
    }
    panic!("the device never came");
}

/// The lines a client heard the device say since `from`.
fn said(h: &Harness, b: usize, from: usize) -> Vec<u32> {
    h.world_messages(b, from)
        .iter()
        .filter_map(|m| match m {
            ServerMessage::CellObjectCall(c) if c.call.function.0 == SAYING => c.call.args.as_ref(),
            _ => None,
        })
        .filter_map(|a| calls::values(&a.0).ok())
        .filter_map(|v| {
            v.iter().find_map(|x| match x.root() {
                Some(Node::TextInfo(t)) => Some(t.string_id),
                _ => None,
            })
        })
        .collect()
}

fn run_for(h: &mut Harness, seconds: f64) {
    let t = h.now.0 + seconds;
    while h.now.0 < t {
        h.step();
    }
}

/// The living creatures of a template.
fn living(h: &Harness, template: u32) -> Vec<u64> {
    let mut v: Vec<u64> = h
        .server
        .things
        .by_id
        .values()
        .filter(|t| t.template == template)
        .filter(|t| t.creature.as_ref().is_some_and(|c| c.mode != Mode::Dead))
        .map(|t| t.id)
        .collect();
    v.sort_unstable();
    v
}

fn target_of(h: &Harness, id: u64) -> Option<u64> {
    h.server
        .things
        .get(id)
        .and_then(|t| t.creature.as_ref())
        .and_then(|c| c.target)
}

fn markers(h: &Harness) -> usize {
    h.server
        .things
        .by_id
        .values()
        .filter(|t| t.template == MARKER && t.kind == Kind::Placed)
        .count()
}

/// The device, made by its camp, has its row's two attackers about it at the step it comes, each
/// going at it from where it was made, the row's distance away, and fights no one itself; it says
/// its line as it comes. One killed comes back after the row's time and goes at it as it comes; one
/// that has given up goes at it again before anything else. Half way it says its next line; once it
/// has stood its time it says its line for holding, goes, and the marker stands where it stood.
/// Behaviour: server.creatures.a-defended-device-holds-against-its-attackers
#[test]
fn a_device_stands_its_time_against_its_attackers_and_leaves_what_its_row_names() {
    let (mut h, b) = hall(100_000_000);
    let from = h.bots[b].inbox.len();
    let device = comes(&mut h);
    let attackers = living(&h, ATTACKER);
    assert_eq!(attackers.len(), 2, "its population at once");
    for a in &attackers {
        assert_eq!(
            target_of(&h, *a),
            Some(device),
            "each goes at the device as it comes"
        );
        let at = h
            .server
            .things
            .get(*a)
            .and_then(|t| t.creature.as_ref())
            .map(|c| c.home)
            .expect("made");
        let d =
            ((at.origin[0] - DEVICE_AT[0]).powi(2) + (at.origin[1] - DEVICE_AT[1]).powi(2)).sqrt();
        assert!((d - 6.0).abs() < 0.5, "made at the row's distance: {d}");
    }
    assert!(
        !h.server
            .things
            .get(device)
            .unwrap()
            .creature
            .as_ref()
            .unwrap()
            .stats
            .fights,
        "the device fights no one"
    );
    run_for(&mut h, 5.0);
    assert_eq!(said(&h, b, from), [COMES], "its line as it comes");
    assert!(
        h.server.things.get(device).unwrap().health
            < h.server.things.get(device).unwrap().max_health,
        "the attackers strike it"
    );

    // One that has given up goes at it again.
    if let Some(c) = h
        .server
        .things
        .get_mut(attackers[1])
        .and_then(|t| t.creature.as_mut())
    {
        c.mode = Mode::Idle;
        c.target = None;
    }
    run_for(&mut h, 2.0);
    assert_eq!(
        target_of(&h, attackers[1]),
        Some(device),
        "back at the device"
    );

    let clock = h.clock();
    yalain_world::death::creature_dies(&mut h.server, &mut h.net, attackers[0], 0, clock);
    run_for(&mut h, REGEN - 2.0);
    assert_eq!(
        living(&h, ATTACKER).len(),
        1,
        "not back before the row's time"
    );
    let mut fresh = None;
    for _ in 0..400 {
        h.step();
        fresh = living(&h, ATTACKER)
            .into_iter()
            .find(|a| *a != attackers[1]);
        if fresh.is_some() {
            break;
        }
    }
    let fresh = fresh.expect("back after the row's time");
    assert_eq!(
        target_of(&h, fresh),
        Some(device),
        "going at it as it comes"
    );

    run_for(&mut h, HOLDS - 36.0);
    assert_eq!(living(&h, DEVICE), [device], "standing before its time");
    assert_eq!(markers(&h), 0);
    assert_eq!(said(&h, b, from), [COMES, HUMS], "half way, its next line");
    let from = h.bots[b].inbox.len();
    run_for(&mut h, 10.0);
    assert!(living(&h, DEVICE).is_empty(), "gone once it has held");
    assert_eq!(said(&h, b, from), [HOLDS_LINE], "its line as it holds");
    assert_eq!(markers(&h), 1, "the marker in its place");
    let placed = h
        .server
        .things
        .by_id
        .values()
        .find(|t| t.template == MARKER)
        .and_then(|t| t.at)
        .unwrap();
    assert!(
        (placed.origin[0] - DEVICE_AT[0]).abs() < 0.1
            && (placed.origin[1] - DEVICE_AT[1]).abs() < 0.1
    );
    assert_eq!(living(&h, ATTACKER).len(), 2, "its attackers stay");
}

/// A device its attackers wear down says its line for being badly hurt once; killed before its
/// time it leaves nothing in its place, and no more attackers come. One fallen any other way is
/// gone as well.
/// Behaviour: server.creatures.a-defended-device-holds-against-its-attackers
#[test]
fn a_device_killed_before_its_time_leaves_nothing() {
    let (mut h, b) = hall(400);
    let from = h.bots[b].inbox.len();
    run_for(&mut h, 30.0);
    assert!(living(&h, DEVICE).is_empty(), "killed by its attackers");
    assert_eq!(
        said(&h, b, from).iter().filter(|l| **l == HURT).count(),
        1,
        "badly hurt, said once"
    );
    let clock = h.clock();
    for a in living(&h, ATTACKER) {
        yalain_world::death::creature_dies(&mut h.server, &mut h.net, a, 0, clock);
    }
    run_for(&mut h, HOLDS + REGEN);
    assert_eq!(markers(&h), 0, "nothing in its place");
    assert!(living(&h, ATTACKER).is_empty(), "no more come");

    let (mut h, _) = hall(100_000_000);
    let device = comes(&mut h);
    let clock = h.clock();
    yalain_world::death::creature_dies(&mut h.server, &mut h.net, device, 0, clock);
    run_for(&mut h, HOLDS + 5.0);
    assert_eq!(markers(&h), 0, "a fallen device holds nothing");
}
