//! A safe passage: the creatures of a camp, or every creature of a template, leave alone a player
//! who holds an item or bears an effect. Fixture: the test world, camps of creatures beside where a
//! character starts, and the rows each test gives.

use auberean_script::args::Arg;
use yalain_content::creature_rows::{Guards, Pass, SafePassage};
use yalain_content::Prop;
use yalain_testkit::play::server_call;
use yalain_testkit::Harness;
use yalain_world::things::Mode;

const GUARD: u32 = 0x4700_2226;
const OTHER: u32 = 0x4700_2227;
const STARTER_SWORD: u32 = 0x4700_051C;
const START_ATTACK: u32 = 0x8204_0008;
/// An effect, and an item, a row lets pass by.
const AMULET_EFFECT: u32 = 0x6F00_1D2C;
const AMULET: u32 = 0x4700_1668;
/// What every creature hates, by the test world's concealing rows.
const HATED_BY_ALL: u32 = 0x7000_1CB3;
/// A friend table of those that attack first, and one of those that never do.
const FIRST: u32 = 0x42;
const NEVER_FIRST: u32 = 0x0E;
const CLASS: u32 = 0x65;

/// A camp of a template at these spots, by its row in the spawn table.
fn camp(h: &mut Harness, template: u32, table: u32, at: &[[f32; 3]]) -> u32 {
    let mut p = yalain_content::Props::default();
    p.set("GameplayStatistics_Level", Prop::Int(2))
        .set("GameplayStatistics_MaxHealth", Prop::Int(5000))
        .set("AICombat_NaturalDamage", Prop::Float(1.0))
        .set("AICombat_NaturalVariance", Prop::Float(0.5))
        .set("AI_PerceptionRadius", Prop::Float(20.0))
        .set("AI_AISuperClass", Prop::Word(CLASS))
        .set("AI_LoveTable", Prop::Word(table));
    h.server
        .catalog
        .insert(yalain_testkit::test_template(template, 0x1169, p));
    let row = u32::try_from(h.server.spawns.camps.len()).expect("a row");
    h.server.spawns.camps.push(yalain_content::Camp {
        id: format!("camp {row}"),
        cell: 0x0299_0109,
        origin: at[0],
        radius: 0.0,
        population: u32::try_from(at.len()).unwrap_or(0),
        regen: 600.0,
        templates: vec![(template, 1)],
        spots: at.iter().map(|s| (0x0299_0109, *s)).collect(),
        placed: "authored for the tests".into(),
        confidence: "authored".into(),
        moves: yalain_content::CampMoves::default(),
        name: None,
    });
    h.server
        .camps
        .push(yalain_world::spawn::CampState::default());
    row
}

fn bearing(p: &mut yalain_world::world::Player, state: u32) {
    let eid = yalain_world::effects::next_eid(p);
    p.play.effects.push(yalain_world::state::EffectState {
        eid,
        state,
        cast: 0.0,
        timeout: f32::MAX,
        caster: 0,
        spellcraft: 1.0,
        pk: false,
        acting: 0,
        skill: 0,
        item: 0,
        flags: 0,
        categories: 1,
    });
}

fn holding(p: &mut yalain_world::world::Player, template: u32, worn: bool) {
    p.play.items.push(yalain_world::state::Held {
        id: 0x8000_0000_0000_0F00,
        template,
        slot: 0,
        location: if worn { 0x8 } else { 0 },
        quantity: 1,
    });
}

/// What the player holds or bears before anything notices it.
#[derive(Clone, Copy)]
enum Has {
    Nothing,
    Effect(u32),
    Item(u32, bool),
    Both(u32, u32),
}

/// A player beside the camps the rows name: creatures of `GUARD` at one spot and of `OTHER` at
/// another, each a camp of its own (`.0` and `.1` of the rows), with what it has.
struct Beside {
    h: Harness,
    b: usize,
    me: u64,
    guards: Vec<u64>,
    others: Vec<u64>,
}

fn beside(
    table: u32,
    rows: impl FnOnce(u32, u32) -> Vec<SafePassage>,
    has: Has,
    other_template: u32,
) -> Beside {
    let mut h = Harness::new();
    h.server.catalog = yalain_testkit::test_catalog();
    let a = camp(&mut h, GUARD, table, &[[62.0, 240.0, -44.6]]);
    let b_row = camp(&mut h, other_template, table, &[[61.0, 238.0, -44.6]]);
    h.server.creature_rows.safe_passage = rows(a, b_row);
    let (b, me) = h.player_in_world("one", "Elara");
    {
        let p = h.server.players.values_mut().next().expect("the player");
        match has {
            Has::Nothing => {}
            Has::Effect(e) => bearing(p, e),
            Has::Item(t, worn) => holding(p, t, worn),
            Has::Both(e, f) => {
                bearing(p, e);
                bearing(p, f);
            }
        }
    }
    h.bots[b].send(
        true,
        auberean_protocol::queue::WEENIE,
        &auberean_protocol::ClientMessage::SceneRenderingComplete(auberean_protocol::login::Empty),
    );
    let of_camp = |h: &Harness, row: u32| -> Vec<u64> {
        h.server
            .things
            .by_id
            .values()
            .filter(|t| {
                t.creature
                    .as_ref()
                    .and_then(|c| c.camp)
                    .is_some_and(|c| c == row as usize)
            })
            .map(|t| t.id)
            .collect()
    };
    let (guards, others) = (of_camp(&h, a), of_camp(&h, b_row));
    Beside {
        h,
        b,
        me,
        guards,
        others,
    }
}

fn chasing(h: &Harness, id: u64) -> bool {
    h.server
        .things
        .get(id)
        .and_then(|t| t.creature.as_ref())
        .is_some_and(|c| c.mode == Mode::Chasing)
}

fn by_camp(row: u32, pass: Pass) -> SafePassage {
    SafePassage {
        guards: Guards::Camp(row),
        pass,
    }
}

/// A camp's creatures leave alone a player bearing the effect its row names, however long it
/// stands beside them, until the player strikes one; then it fights back. A camp of the same kind
/// the row doesn't name comes at once, and so does the camp itself at one without the effect.
/// Behaviour: server.creatures.a-safe-passage-lets-its-bearer-by
#[test]
fn a_camps_creatures_leave_alone_a_player_bearing_its_passage_until_struck() {
    let mut s = beside(
        FIRST,
        |a, _| vec![by_camp(a, Pass::Effect(AMULET_EFFECT))],
        Has::Effect(AMULET_EFFECT),
        GUARD,
    );
    assert_eq!((s.guards.len(), s.others.len()), (1, 1));
    assert!(
        s.h.run_until(100, |h| chasing(h, s.others[0])),
        "another camp of its kind comes"
    );
    s.h.run(400);
    assert!(!chasing(&s.h, s.guards[0]), "standing beside it");
    s.h.arm(s.b, s.me, STARTER_SWORD);
    s.h.bots[s.b].send(
        true,
        auberean_protocol::queue::WEENIE,
        &server_call(START_ATTACK, &[Arg::LongInt(s.guards[0])]),
    );
    let g = s.guards[0];
    assert!(s.h.run_until(400, |h| chasing(h, g)), "struck, it fights");

    let mut s = beside(
        FIRST,
        |a, _| vec![by_camp(a, Pass::Effect(AMULET_EFFECT))],
        Has::Nothing,
        GUARD,
    );
    let g = s.guards[0];
    assert!(s.h.run_until(100, |h| chasing(h, g)), "not borne: it comes");
    let mut s = beside(
        FIRST,
        |a, _| vec![by_camp(a, Pass::Effect(AMULET_EFFECT))],
        Has::Effect(0x6F00_1C46),
        GUARD,
    );
    let g = s.guards[0];
    assert!(s.h.run_until(100, |h| chasing(h, g)), "another effect");
}

/// A kind's safe passage lets by a player holding its item, in the pack or worn, wherever the
/// kind's creatures stand; creatures of another kind come, and so do its own at one without it.
/// Behaviour: server.creatures.a-safe-passage-lets-its-bearer-by
#[test]
fn a_kinds_creatures_leave_alone_a_player_holding_its_item_in_any_camp() {
    let kind = |_, _| {
        vec![SafePassage {
            guards: Guards::Template(GUARD),
            pass: Pass::Item(AMULET),
        }]
    };
    for worn in [false, true] {
        let mut s = beside(FIRST, kind, Has::Item(AMULET, worn), OTHER);
        let o = s.others[0];
        assert!(s.h.run_until(100, |h| chasing(h, o)), "another kind comes");
        s.h.run(400);
        assert!(!chasing(&s.h, s.guards[0]), "worn {worn}: let by");
    }
    let mut s = beside(FIRST, kind, Has::Item(AMULET, false), GUARD);
    s.h.run(400);
    assert!(
        !s.guards
            .iter()
            .chain(&s.others)
            .any(|id| chasing(&s.h, *id)),
        "both camps are of its kind"
    );
    let mut s = beside(FIRST, kind, Has::Nothing, GUARD);
    let g = s.guards[0];
    assert!(s.h.run_until(100, |h| chasing(h, g)), "not held: it comes");
    let mut s = beside(FIRST, kind, Has::Item(STARTER_SWORD, false), GUARD);
    let g = s.guards[0];
    assert!(s.h.run_until(100, |h| chasing(h, g)), "another item");
}

/// A creature that lets a player by doesn't come to its kin's fight with it, nor turn to it as it
/// fights another; it may yet turn on one that has hurt it.
/// Behaviour: server.creatures.a-safe-passage-lets-its-bearer-by
#[test]
fn a_creature_neither_joins_a_fight_with_nor_turns_on_a_player_it_lets_by() {
    // The same friend table: the camp the row doesn't name comes, and its kin may follow.
    let row = |x: f32| -> Vec<[f32; 3]> { (0..8).map(|i| [x, 236.0 + i as f32, -44.6]).collect() };
    let mut h = Harness::new();
    h.server.catalog = yalain_testkit::test_catalog();
    let kin = camp(&mut h, GUARD, 0x06, &row(61.0));
    camp(&mut h, OTHER, 0x06, &[[63.0, 240.0, -44.6]]);
    h.server.creature_rows.safe_passage = vec![by_camp(kin, Pass::Effect(AMULET_EFFECT))];
    let (b, _) = h.player_in_world("one", "Elara");
    bearing(
        h.server.players.values_mut().next().expect("in"),
        AMULET_EFFECT,
    );
    h.bots[b].send(
        true,
        auberean_protocol::queue::WEENIE,
        &auberean_protocol::ClientMessage::SceneRenderingComplete(auberean_protocol::login::Empty),
    );
    let of = |h: &Harness, t: u32| -> Vec<u64> {
        h.server
            .things
            .by_id
            .values()
            .filter(|x| x.template == t)
            .map(|x| x.id)
            .collect()
    };
    let (guards, other) = (of(&h, GUARD), of(&h, OTHER));
    assert_eq!((guards.len(), other.len()), (8, 1));
    assert!(h.run_until(100, |h| chasing(h, other[0])));
    h.run(200);
    assert!(!guards.iter().any(|id| chasing(&h, *id)), "its kin stay");

    // Fighting one player, it never turns to the one it lets by, until that one hurts it.
    let mut s = beside(
        FIRST,
        |a, _| vec![by_camp(a, Pass::Effect(AMULET_EFFECT))],
        Has::Nothing,
        OTHER,
    );
    let (h, me, id) = (&mut s.h, s.me, s.guards[0]);
    let (_, let_by) = h.player_in_world("two", "Bran");
    let here = h
        .server
        .players
        .values()
        .find(|p| p.character == me)
        .expect("in")
        .position;
    let p = h
        .server
        .players
        .values_mut()
        .find(|p| p.character == let_by)
        .expect("in");
    bearing(p, AMULET_EFFECT);
    p.scene_drawn = true;
    p.position = here;
    let mut turned = (0, 0);
    for round in 0..4000 {
        let now = h.now.0;
        let c = h
            .server
            .things
            .get_mut(id)
            .and_then(|t| t.creature.as_mut())
            .expect("a creature");
        c.mode = Mode::Chasing;
        c.target = Some(me);
        c.target_since = now;
        c.next_attack = now;
        c.considered = f64::NEG_INFINITY;
        c.harm.clear();
        c.taunts.clear();
        if round >= 2000 {
            c.harm.push((now, let_by, 100));
        }
        yalain_world::ai::reconsider_for_test(&mut h.server, id, now);
        let on_it = h
            .server
            .things
            .get(id)
            .and_then(|t| t.creature.as_ref())
            .is_some_and(|c| c.target == Some(let_by));
        if round < 2000 {
            turned.0 += i32::from(on_it);
        } else {
            turned.1 += i32::from(on_it);
        }
    }
    assert_eq!(turned.0, 0, "it hasn't hurt it");
    assert!(turned.1 > 100, "it has: {}", turned.1);
}

/// An effect that makes its holder hated wins over a safe passage: even a creature that never
/// attacks first comes at a hated player its camp would let by.
/// Behaviour: server.creatures.a-safe-passage-lets-its-bearer-by
#[test]
fn hate_wins_over_a_safe_passage() {
    let mut s = beside(
        NEVER_FIRST,
        |a, _| vec![by_camp(a, Pass::Effect(AMULET_EFFECT))],
        Has::Both(AMULET_EFFECT, HATED_BY_ALL),
        OTHER,
    );
    let g = s.guards[0];
    assert!(s.h.run_until(100, |h| chasing(h, g)), "hated");
    let mut s = beside(
        FIRST,
        |a, _| vec![by_camp(a, Pass::Effect(AMULET_EFFECT))],
        Has::Effect(AMULET_EFFECT),
        OTHER,
    );
    s.h.run(400);
    assert!(!chasing(&s.h, s.guards[0]), "not hated: let by");
}
