//! What a concealing effect on a player makes creatures of a class do about it. Fixture: the test
//! world, a camp of creatures of one class beside where a character starts.

use auberean_script::args::Arg;
use yalain_content::Prop;
use yalain_testkit::play::server_call;
use yalain_testkit::Harness;
use yalain_world::things::Mode;

const CREATURE: u32 = 0x4700_2226;
const OTHERS: u32 = 0x4700_2227;
const STARTER_SWORD: u32 = 0x4700_051C;
const START_ATTACK: u32 = 0x8204_0008;
/// The amulet that spares its holder its moarsmen, class 0x17.
const PEACE: u32 = 0x6F00_1C46;
const MOARSMEN: u32 = 0x17;
/// The curse of the drudges, class 0x06.
const CURSE: u32 = 0x6F00_20BC;
const DRUDGES: u32 = 0x06;
/// What every creature hates.
const HATED_BY_ALL: u32 = 0x7000_1CB3;
/// A friend table of those that attack first, and one of those that never do.
const FIRST: u32 = 0x42;
const NEVER_FIRST: u32 = 0x0E;

fn camp(h: &mut Harness, template: u32, class: u32, table: u32, at: &[[f32; 3]]) {
    let mut p = yalain_content::Props::default();
    p.set("GameplayStatistics_Level", Prop::Int(2))
        .set("GameplayStatistics_MaxHealth", Prop::Int(5000))
        .set("AICombat_NaturalDamage", Prop::Float(1.0))
        .set("AICombat_NaturalVariance", Prop::Float(0.5))
        .set("AI_PerceptionRadius", Prop::Float(20.0))
        .set("AI_AISuperClass", Prop::Word(class))
        .set("AI_LoveTable", Prop::Word(table));
    h.server
        .catalog
        .insert(yalain_testkit::test_template(template, 0x1169, p));
    h.server.spawns.camps.push(yalain_content::Camp {
        id: format!("camp {template:08X}"),
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
    });
    h.server
        .camps
        .push(yalain_world::spawn::CampState::default());
}

fn holding(h: &mut Harness, state: u32) {
    let cast = h.net.game_time(h.now);
    let p = h.server.players.values_mut().next().expect("the player");
    let eid = yalain_world::effects::next_eid(p);
    p.play.effects.push(yalain_world::state::EffectState {
        eid,
        state,
        cast,
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

/// A player beside a camp of a class, holding what's asked (nothing notices it until then), and the
/// camp's creatures.
fn beside(class: u32, table: u32, held: Option<u32>) -> (Harness, usize, u64, Vec<u64>) {
    let mut h = Harness::new();
    h.server.catalog = yalain_testkit::test_catalog();
    camp(&mut h, CREATURE, class, table, &[[62.0, 240.0, -44.6]]);
    let (b, me) = h.player_in_world("one", "Elara");
    if let Some(state) = held {
        holding(&mut h, state);
    }
    h.bots[b].send(
        true,
        auberean_protocol::queue::WEENIE,
        &auberean_protocol::ClientMessage::SceneRenderingComplete(auberean_protocol::login::Empty),
    );
    let ids = of_template(&h, CREATURE);
    (h, b, me, ids)
}

fn of_template(h: &Harness, template: u32) -> Vec<u64> {
    h.server
        .things
        .by_id
        .values()
        .filter(|t| t.template == template)
        .map(|t| t.id)
        .collect()
}

fn chasing(h: &Harness, id: u64) -> bool {
    h.server
        .things
        .get(id)
        .and_then(|t| t.creature.as_ref())
        .is_some_and(|c| c.mode == Mode::Chasing)
}

/// A creature of the class an effect spares leaves its holder alone, however long it stands there,
/// until the player strikes it; then it fights back.
/// Behaviour: server.creatures.an-effect-spares-its-holder
#[test]
fn a_creature_leaves_alone_a_player_it_spares_until_struck() {
    let (mut h, b, me, ids) = beside(MOARSMEN, FIRST, Some(PEACE));
    assert_eq!(ids.len(), 1);
    h.run(400);
    assert!(!chasing(&h, ids[0]), "standing beside it");
    h.arm(b, me, STARTER_SWORD);
    h.bots[b].send(
        true,
        auberean_protocol::queue::WEENIE,
        &server_call(START_ATTACK, &[Arg::LongInt(ids[0])]),
    );
    assert!(
        h.run_until(400, |h| chasing(h, ids[0])),
        "struck, it fights"
    );
}

/// What it holds spares it only creatures of the class it names, and without it they come at once.
/// Behaviour: server.creatures.an-effect-spares-its-holder
#[test]
fn an_effect_spares_its_holder_only_the_class_it_names() {
    let (mut h, _, _, ids) = beside(DRUDGES, FIRST, Some(PEACE));
    assert!(h.run_until(100, |h| chasing(h, ids[0])), "another class");
    let (mut h, _, _, ids) = beside(MOARSMEN, FIRST, None);
    assert!(h.run_until(100, |h| chasing(h, ids[0])), "not held");
}

/// A creature that never attacks first comes at a player of its class's curse, and at one every
/// creature hates; it leaves the rest alone.
/// Behaviour: server.creatures.an-effect-makes-its-holder-hated
#[test]
fn a_creature_that_never_attacks_first_comes_at_a_player_it_hates() {
    let (mut h, _, _, ids) = beside(DRUDGES, NEVER_FIRST, Some(CURSE));
    assert!(h.run_until(100, |h| chasing(h, ids[0])), "cursed");
    let (mut h, _, _, ids) = beside(MOARSMEN, NEVER_FIRST, Some(HATED_BY_ALL));
    assert!(h.run_until(100, |h| chasing(h, ids[0])), "hated by all");
    let (mut h, _, _, ids) = beside(MOARSMEN, NEVER_FIRST, Some(CURSE));
    h.run(400);
    assert!(!chasing(&h, ids[0]), "another class");
    let (mut h, _, _, ids) = beside(DRUDGES, NEVER_FIRST, None);
    h.run(400);
    assert!(!chasing(&h, ids[0]), "not cursed");
}

/// A creature it spares doesn't come to its kin's fight with a player, nor turn to one as it fights
/// another; it may yet turn on one that has hurt it.
/// Behaviour: server.creatures.an-effect-spares-its-holder
#[test]
fn a_creature_neither_joins_a_fight_with_nor_turns_on_a_player_it_spares() {
    let row = |x: f32| -> Vec<[f32; 3]> { (0..8).map(|i| [x, 236.0 + i as f32, -44.6]).collect() };
    let mut h = Harness::new();
    h.server.catalog = yalain_testkit::test_catalog();
    // The same friend table: the one that isn't spared comes, its kin may follow.
    camp(&mut h, CREATURE, MOARSMEN, 0x06, &row(61.0));
    camp(&mut h, OTHERS, DRUDGES, 0x06, &[[63.0, 240.0, -44.6]]);
    let (b, _) = h.player_in_world("one", "Elara");
    holding(&mut h, PEACE);
    h.bots[b].send(
        true,
        auberean_protocol::queue::WEENIE,
        &auberean_protocol::ClientMessage::SceneRenderingComplete(auberean_protocol::login::Empty),
    );
    let (kin, other) = (of_template(&h, CREATURE), of_template(&h, OTHERS));
    assert_eq!((kin.len(), other.len()), (8, 1));
    assert!(h.run_until(100, |h| chasing(h, other[0])));
    h.run(200);
    assert!(!kin.iter().any(|id| chasing(&h, *id)), "its kin stay");

    let (mut h, _, me, ids) = beside(MOARSMEN, FIRST, None);
    let (_, spared) = h.player_in_world("two", "Bran");
    let world = h
        .server
        .players
        .iter()
        .find(|(_, p)| p.character == spared)
        .map(|(c, _)| *c)
        .expect("in");
    let p = h.server.players.get_mut(&world).expect("in");
    let cast = 0.0;
    let eid = yalain_world::effects::next_eid(p);
    p.play.effects.push(yalain_world::state::EffectState {
        eid,
        state: PEACE,
        cast,
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
    p.scene_drawn = true;
    let here = h
        .server
        .players
        .values()
        .find(|p| p.character == me)
        .expect("in")
        .position;
    h.server.players.get_mut(&world).expect("in").position = here;
    let mut turned = (0, 0);
    for round in 0..4000 {
        let now = h.now.0;
        let c = h
            .server
            .things
            .get_mut(ids[0])
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
            c.harm.push((now, spared, 100));
        }
        yalain_world::ai::reconsider_for_test(&mut h.server, ids[0], now);
        let on_it = h
            .server
            .things
            .get(ids[0])
            .and_then(|t| t.creature.as_ref())
            .is_some_and(|c| c.target == Some(spared));
        if round < 2000 {
            turned.0 += i32::from(on_it);
        } else {
            turned.1 += i32::from(on_it);
        }
    }
    assert_eq!(turned.0, 0, "it hasn't hurt it");
    assert!(turned.1 > 100, "it has: {}", turned.1);
}

/// An effect whose words name a class is for that class alone, each of two where it names two.
/// Behaviour: server.creatures.an-effects-words-name-its-class
#[test]
fn an_effect_is_for_the_classes_its_words_name() {
    use yalain_world::ai::{regard, Regard};
    const FANATIC_DISGUISE: u32 = 0x6F00_19F1;
    const SLAVE_SPAWN_HOSTILITY: u32 = 0x6F00_1C48;
    const ARCHON_HATRED: u32 = 0x6F00_1C77;
    const LOYALIST_HATRED: u32 = 0x6F00_1C78;
    const ARCHONS_FAVOR: u32 = 0x6F00_2049;
    const FILINUVEKTA_FAVOR: u32 = 0x6F00_204B;
    let cases: [(u32, &[u32], Regard); 6] = [
        (FANATIC_DISGUISE, &[0x59], Regard::Spares),
        (SLAVE_SPAWN_HOSTILITY, &[0x78], Regard::Hates),
        (ARCHON_HATRED, &[0x46, 0x43], Regard::Hates),
        (LOYALIST_HATRED, &[0x45, 0x44], Regard::Hates),
        (ARCHONS_FAVOR, &[0x46, 0x43], Regard::Spares),
        (FILINUVEKTA_FAVOR, &[0x45, 0x44], Regard::Spares),
    ];
    for (state, classes, how) in cases {
        let mut h = Harness::new();
        h.server.catalog = yalain_testkit::test_catalog();
        h.player_in_world("one", "Elara");
        holding(&mut h, state);
        let p = h.server.players.values().next().expect("the player");
        for class in classes {
            assert_eq!(regard(p, Some(*class)), Some(how), "{state:#x} {class:#x}");
        }
        assert_eq!(
            regard(p, Some(0x06)),
            None,
            "{state:#x}, a class it doesn't name"
        );
        assert_eq!(regard(p, None), None, "{state:#x}, no class");
    }
}

/// The effects whose words say no class or whose recordings disagree do nothing, for any class
/// their words might be read to mean.
/// Behaviour: server.creatures.an-effects-words-name-its-class
#[test]
fn an_effect_that_names_no_class_does_nothing() {
    use yalain_world::ai::regard;
    const CHAOS_WARD: u32 = 0x6F00_1DEA;
    const SHRETH_ALERT: u32 = 0x6F00_1F2C;
    const ULTRA_INTRUDER: u32 = 0x6F00_1F31;
    const RABBIT_DISGUISE: u32 = 0x6F00_20C0;
    const SAFE_PASSAGE: u32 = 0x6F00_1D2C;
    for state in [
        CHAOS_WARD,
        SHRETH_ALERT,
        ULTRA_INTRUDER,
        RABBIT_DISGUISE,
        SAFE_PASSAGE,
    ] {
        let mut h = Harness::new();
        h.server.catalog = yalain_testkit::test_catalog();
        h.player_in_world("one", "Elara");
        holding(&mut h, state);
        let p = h.server.players.values().next().expect("the player");
        for class in [0x08, 0x1C, 0x40, 0x4C, 0x4D, 0x55, 0x75, 0x1E, 0x48, 0x6F] {
            assert_eq!(regard(p, Some(class)), None, "{state:#x} {class:#x}");
        }
    }
}
