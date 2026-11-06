//! Pets beyond the following class pets' orders: how a pet's hit is reckoned; a pet's missile
//! landing as a story of what it lands on after its flight; an aggressive pet taking on creatures
//! unbidden; the Volcanic Rift (staying where it is made, its fire falling on all near where it
//! lands, gone at its lifetime's end); a turret that cannot be set down indoors; the orbs (four hits
//! a swing, a taunt that harms nothing).
//! Fixture: summoning skills made up for each kind, with the kinds' pet effects, classes and
//! lifetimes as the data defines them; templates made up with the test world's attack skills; the
//! test world's beetles, placed by hand.

use auberean_protocol::{queue, ServerMessage};
use auberean_script::args::Arg;
use yalain_content::{Prop, Props};
use yalain_testkit::play::server_call;
use yalain_testkit::{test_template, Harness};
use yalain_world::calls::Value;
use yalain_world::effect_defs::{EffectDef, Kind, Scale};
use yalain_world::pets::{self, function};
use yalain_world::player::quality;
use yalain_world::quals;
use yalain_world::rng::Rng;
use yalain_world::skills::{SkillDef, Usage};
use yalain_world::state::SkillState;

const SKILL: u32 = 3001;
const SWORD: u32 = 0x4700_051C;
const USE_SKILL: u32 = 0x8204_000B;
const BEETLE: u32 = yalain_testkit::TEST_CREATURE;
/// The test world's plain melee and missile attacks.
const MELEE: u32 = 1845;
const MISSILE: u32 = 1861;

/// A kind of pet as a test summons it: its effect, its template, its class and lifetime (0: none).
struct Summoned {
    effect: u32,
    template: u32,
    class: u32,
    lifetime: f32,
    skill: u32,
}

const LASHER: Summoned = Summoned {
    effect: 0x7000_0805,
    template: 0x4700_0846,
    class: 16,
    lifetime: 0.0,
    skill: MISSILE,
};
const RIFT: Summoned = Summoned {
    effect: 0x7000_0830,
    template: 0x4700_084E,
    class: 128,
    lifetime: 60.0,
    skill: MISSILE,
};
const TURRET: Summoned = Summoned {
    effect: 0x7000_08B7,
    template: 0x4700_0851,
    class: 256,
    lifetime: 600.0,
    skill: MISSILE,
};
const AXE_ORB: Summoned = Summoned {
    effect: 0x7000_1F1D,
    template: 0x4700_083D,
    class: 8,
    lifetime: 0.0,
    skill: MELEE,
};
const TAUNT_ORB: Summoned = Summoned {
    effect: 0x7000_1F1F,
    template: 0x4700_083E,
    class: 8,
    lifetime: 0.0,
    skill: MELEE,
};

/// The test world's beetles first (they bring the test catalog), then the summoning skill, its pet
/// effect and the pet's template.
fn with_kind(h: &mut Harness, k: &Summoned, beetles: u32) {
    h.add_creatures(beetles);
    let usage = Usage {
        min_range: -1.0,
        max_range: 1.0,
        recovery: 1.0,
        min_use: 0.5,
        powerup: 1.0,
        powerup_behavior: 0x4100_000C,
        vigor_add: -1.0,
        min_vigor: 1.0,
        implements_right: -2,
        implements_left: -1,
        valid_targets: 1,
        behavior_other: [0x4100_00ED; 3],
        behavior_self: [0x4100_00ED; 2],
        user_effects: vec![k.effect],
        ui_flags: 0x32C0,
        ..Usage::default()
    };
    h.server.skill_table.skills.insert(
        SKILL,
        SkillDef {
            id: SKILL,
            icon: 0x4100_47E6,
            cap: 150,
            table: 0x6F00_000B,
            flags: 0x41,
            active: true,
            usage: Some(usage),
            ..SkillDef::default()
        },
    );
    let lifetime = if k.lifetime > 0.0 {
        Scale(vec![(1.0, k.lifetime)])
    } else {
        Scale::default()
    };
    h.server.effect_table.effects.insert(
        k.effect,
        EffectDef {
            state: k.effect,
            kind: Kind::Pet {
                class: k.class,
                lifetime,
            },
            duration: Scale::default(),
            priority: Scale::default(),
            fx: 0,
            external_flags: 177,
            internal_flags: 0x4140_8513,
            enum_value: 538,
            eq_class: 0,
            name: None,
        },
    );
    let mut props = Props::default();
    props
        .set("GameplayStatistics_MaxHealth", Prop::Int(560))
        .set("AICombat_NaturalDamage", Prop::Float(45.0))
        .set("AICombat_NaturalVariance", Prop::Float(0.5))
        .set("AI_PerceptionRadius", Prop::Float(40.0))
        .set("NPC_StartingSkill1", Prop::Word(k.skill))
        .set("NPC_StartingSkill1Level", Prop::Int(1))
        .set("NPC_StartingSkill1_IsBase", Prop::Bool(true));
    h.server
        .catalog
        .insert(test_template(k.template, 0x1169, props));
}

/// A player in the world with the scene drawn, vigor to spare and the skill trained.
fn summoner(h: &mut Harness) -> (usize, u64) {
    let (b, me) = h.player_in_world("one", "Kestrel");
    h.bots[b].send(
        true,
        queue::WEENIE,
        &auberean_protocol::ClientMessage::SceneRenderingComplete(auberean_protocol::login::Empty),
    );
    h.run(5);
    h.arm(b, me, SWORD);
    let p = h
        .server
        .players
        .values_mut()
        .find(|p| p.character == me)
        .expect("in the world");
    quals::set_int(&mut p.qualities, quality::VIGOR_MAX, 400);
    quals::set_int(&mut p.qualities, quality::VIGOR, 400);
    p.play.skills.push(SkillState {
        skill: SKILL,
        granted: 0,
        mask: 0x1,
        cost: 2,
        children: 0,
        dependents: 0,
        spent: 0,
    });
    (b, me)
}

/// Use the skill and let the use end; the messages since begin at the index returned.
fn summon(h: &mut Harness, b: usize) -> usize {
    let from = h.bots[b].inbox.len();
    h.bots[b].send(
        true,
        queue::WEENIE,
        &server_call(USE_SKILL, &[Arg::Int(1), Arg::Int(SKILL), Arg::LongInt(0)]),
    );
    h.run(120);
    from
}

/// The pet a client was told of since a message.
fn pet_added(h: &Harness, b: usize, from: usize) -> Option<u64> {
    h.calls(b, from)
        .iter()
        .find(|(f, _)| *f == function::ADD)
        .and_then(|(_, v)| {
            v.iter().find_map(|x| match x {
                Value::Long(l) => Some(*l),
                _ => None,
            })
        })
}

/// The beetles in the world.
fn beetles(h: &Harness) -> Vec<u64> {
    let mut v: Vec<u64> = h
        .server
        .things
        .by_id
        .values()
        .filter(|t| t.template == BEETLE && t.alive())
        .map(|t| t.id)
        .collect();
    v.sort_unstable();
    v
}

/// Put a thing `dx`, `dy` meters from a player, out of its fights.
fn put_near(h: &mut Harness, id: u64, me: u64, dx: f32, dy: f32) {
    let at = h
        .server
        .players
        .values()
        .find(|p| p.character == me)
        .map(|p| p.position)
        .expect("in the world");
    if let Some(t) = h.server.things.get_mut(id) {
        let mut to = at;
        to.origin[0] += dx;
        to.origin[1] += dy;
        t.at = Some(to);
        if let Some(c) = t.creature.as_mut() {
            c.home = to;
            c.stats.attacks_first = false;
            c.stats.max_health = 100_000;
            c.wander_to = None;
            c.stats.movement.wander_chance = 0.0;
        }
        t.max_health = 100_000;
        t.health = 100_000;
    }
}

fn target_of(h: &Harness, id: u64) -> Option<u64> {
    h.server
        .things
        .get(id)
        .and_then(|t| t.creature.as_ref())
        .and_then(|c| c.target)
}

/// A pet's hit is its kind's number times its level squared, varied either side by its kind's
/// share, less the creature's armor, at least 1; never doubled (the Lasher at level 53: a median
/// about 211, between 116 and 306 on an unarmored creature).
/// Behaviour: server.pets.hits-by-level
#[test]
fn a_pets_hit_is_its_kinds_number_times_its_level_squared_less_the_creatures_armor() {
    let lasher = pets::kind_of(LASHER.effect).expect("a kind");
    let mut rng = Rng::new(7);
    let mut hits: Vec<i32> = (0..2000)
        .map(|_| pets::pet_hit(&mut rng, &lasher, 53, 0))
        .collect();
    hits.sort_unstable();
    let (least, median, most) = (hits[0], hits[1000], hits[1999]);
    assert!((110..=125).contains(&least), "{least}");
    assert!((195..=225).contains(&median), "{median}");
    assert!((295..=310).contains(&most), "{most}");
    // Armor takes a point a point; a hit takes at least 1.
    let mut a = Rng::new(9);
    let mut b = Rng::new(9);
    assert_eq!(
        pets::pet_hit(&mut a, &lasher, 53, 0) - pets::pet_hit(&mut b, &lasher, 53, 50),
        50
    );
    assert_eq!(pets::pet_hit(&mut a, &lasher, 1, 500), 1);
    // A lower level hits for less, as the square.
    let low: i32 = (0..2000)
        .map(|_| pets::pet_hit(&mut rng, &lasher, 29, 0))
        .sum::<i32>()
        / 2000;
    assert!((55..=70).contains(&low), "{low}");
}

/// A pet's missile is thrown in a story of its own (the target named in its missile's part, no
/// result), and lands later as a story told by what it lands on, naming the pet and the result.
/// Behaviour: server.pets.missile-lands-later
#[test]
fn a_pets_missile_lands_after_its_flight_as_a_story_of_what_it_lands_on() {
    let mut h = Harness::new();
    with_kind(&mut h, &LASHER, 1);
    let (b, me) = summoner(&mut h);
    h.run(20);
    let pet = {
        let from = summon(&mut h, b);
        pet_added(&h, b, from)
    }
    .expect("a pet");
    let beetle = beetles(&h)[0];
    put_near(&mut h, beetle, me, 12.0, 0.0);
    let from = h.bots[b].inbox.len();
    h.bots[b].send(
        true,
        queue::WEENIE,
        &server_call(function::ATTACK, &[Arg::LongInt(beetle)]),
    );
    assert!(
        h.run_until(200, |h| h.world_messages(b, from).iter().any(|m| matches!(
            m,
            ServerMessage::DoStory(e) if e.sender.id == beetle && e.action.attacker == pet
        )))
    );
    let msgs = h.world_messages(b, from);
    let thrown = msgs
        .iter()
        .position(|m| {
            matches!(m, ServerMessage::DoStory(e) if e.sender.id == pet
            && e.action.skill == MISSILE)
        })
        .expect("the throw");
    let ServerMessage::DoStory(t) = &msgs[thrown] else {
        unreachable!()
    };
    assert_eq!(t.action.bits & 0x20, 0, "no basic-attack flag");
    assert_eq!(t.action.hooks[1].bits, 0x303, "its missile's part");
    assert_eq!(t.action.hooks[1].target, beetle);
    let landed = msgs
        .iter()
        .position(|m| {
            matches!(m, ServerMessage::DoStory(e) if e.sender.id == beetle
            && e.action.attacker == pet)
        })
        .expect("the landing");
    assert!(thrown < landed);
    let ServerMessage::DoStory(l) = &msgs[landed] else {
        unreachable!()
    };
    assert_eq!(l.action.skill, MISSILE);
    assert_eq!(l.action.hooks.len(), 1);
    assert_eq!(l.action.hooks[0].target, beetle);
    assert!(matches!(
        l.action.hooks[0].numbers[0],
        0x4000_0008 | 0x4000_000B
    ));
}

/// An aggressive pet takes on the nearest creature within its reach unbidden; a supporting pet
/// leaves it be.
/// Behaviour: server.pets.aggressive-unprovoked
#[test]
fn an_aggressive_pet_takes_on_the_nearest_creature_unbidden() {
    let mut h = Harness::new();
    with_kind(&mut h, &LASHER, 2);
    let (b, me) = summoner(&mut h);
    h.run(20);
    let pet = {
        let from = summon(&mut h, b);
        pet_added(&h, b, from)
    }
    .expect("a pet");
    let bs = beetles(&h);
    put_near(&mut h, bs[0], me, 25.0, 0.0);
    put_near(&mut h, bs[1], me, 0.0, 15.0);
    h.run(60);
    assert_eq!(target_of(&h, pet), None, "supporting, it waits");
    h.bots[b].send(
        true,
        queue::WEENIE,
        &server_call(function::MODE, &[Arg::Int(0x20), Arg::LongInt(pet)]),
    );
    assert!(
        h.run_until(30, |h| target_of(h, pet).is_some()),
        "aggressive, it goes"
    );
    assert_eq!(target_of(&h, pet), Some(bs[1]), "the nearest");
}

/// A Volcanic Rift stays where it is made and takes on what comes within its missile's reach; its
/// fire lands on every creature near where it falls; at its lifetime's end it is taken off its
/// owner's list and out of the world at once, with no corpse.
/// Behaviour: server.pets.rift
#[test]
fn a_volcanic_rift_stays_its_fire_lands_on_all_near_and_it_goes_at_its_lifetimes_end() {
    let mut h = Harness::new();
    with_kind(&mut h, &RIFT, 2);
    let (b, me) = summoner(&mut h);
    h.run(20);
    let from = summon(&mut h, b);
    let rift = pet_added(&h, b, from).expect("a pet");
    let at = h
        .server
        .things
        .get(rift)
        .and_then(|t| t.at)
        .expect("placed");
    let bs = beetles(&h);
    put_near(&mut h, bs[0], me, 15.0, 0.0);
    put_near(&mut h, bs[1], me, 17.0, 3.0);
    let landed = |h: &Harness| {
        h.world_messages(b, from).iter().any(|m| {
            matches!(m, ServerMessage::DoStory(e) if e.action.attacker == rift
                && e.sender.id != rift && e.action.hooks.len() == 2)
        })
    };
    assert!(h.run_until(200, landed), "its fire falls on both");
    assert_eq!(
        h.server
            .things
            .get(rift)
            .and_then(|t| t.at)
            .map(|a| a.origin),
        Some(at.origin),
        "it never moves"
    );
    // The beetles fight its owner meanwhile, and a player in a fight regains no health: enough of
    // it to outlast the rift.
    if let Some(p) = h.server.players.values_mut().find(|p| p.character == me) {
        quals::set_int(&mut p.qualities, quality::HEALTH_MAX, 100_000);
        quals::set_int(&mut p.qualities, quality::HEALTH, 100_000);
    }
    let from = h.bots[b].inbox.len();
    assert!(h.run_until(1400, |h| !h.server.pets.is_pet(rift)));
    h.run(60);
    assert!(h.functions(b, from).contains(&function::REMOVE));
    assert!(
        h.server.things.get(rift).is_none(),
        "out of the world at once"
    );
    let msgs = h.world_messages(b, from);
    assert!(msgs
        .iter()
        .any(|m| matches!(m, ServerMessage::ForgetObject(o) if o.0.id == rift)));
    assert!(
        !msgs.iter().any(
            |m| matches!(m, ServerMessage::CreateObject(o) if o.id != rift
                && o.weenie.entity_did != BEETLE)
        ),
        "no corpse"
    );
    assert!(h.server.things.get(rift).is_none());
}

/// A turret summoned indoors is told of, then "The summoning fails.", then taken off the list as
/// the use ends; it never appears.
/// Behaviour: server.pets.summoning-fails-indoors
#[test]
fn a_turret_summoned_indoors_fails_and_never_appears() {
    let mut h = Harness::new();
    with_kind(&mut h, &TURRET, 1);
    let (b, me) = summoner(&mut h);
    h.run(20);
    if let Some(p) = h.server.players.values_mut().find(|p| p.character == me) {
        p.position.cell = (p.position.cell & 0xFFFF_0000) | 0x0100;
    }
    let from = summon(&mut h, b);
    let pet = pet_added(&h, b, from).expect("told of");
    let f = h.functions(b, from);
    let add = f.iter().position(|x| *x == function::ADD).expect("added");
    let remove = f
        .iter()
        .position(|x| *x == function::REMOVE)
        .expect("removed");
    assert!(add < remove);
    assert!(h
        .texts(b, from)
        .contains(&(1, pets::text::SUMMONING_FAILS.0)));
    assert!(!h
        .world_messages(b, from)
        .iter()
        .any(|m| matches!(m, ServerMessage::CreateObject(o) if o.id == pet)));
    assert!(!h.server.pets.is_pet(pet));
}

/// The Axe Orb's swing hits its one target four times, each with its own result; the Taunt Orb's
/// strike harms nothing (a result with no health taken), and the creature may turn on it.
/// Behaviour: server.pets.orbs
#[test]
fn an_axe_orb_hits_four_times_a_swing_and_a_taunt_orbs_strike_harms_nothing() {
    for (kind, hooks) in [(&AXE_ORB, 4), (&TAUNT_ORB, 1)] {
        let mut h = Harness::new();
        with_kind(&mut h, kind, 1);
        let (b, me) = summoner(&mut h);
        h.run(20);
        let orb = {
            let from = summon(&mut h, b);
            pet_added(&h, b, from)
        }
        .expect("a pet");
        let beetle = beetles(&h)[0];
        put_near(&mut h, beetle, me, 3.0, 3.0);
        let from = h.bots[b].inbox.len();
        h.bots[b].send(
            true,
            queue::WEENIE,
            &server_call(function::ATTACK, &[Arg::LongInt(beetle)]),
        );
        let struck = |h: &Harness| {
            h.world_messages(b, from).iter().find_map(|m| match m {
                ServerMessage::DoStory(e) if e.sender.id == orb && e.action.attacker == orb => {
                    Some(e.action.hooks.clone())
                }
                _ => None,
            })
        };
        assert!(h.run_until(300, |h| struck(h).is_some()));
        let parts: Vec<_> = struck(&h)
            .expect("a swing")
            .into_iter()
            .filter(|k| k.target == beetle)
            .collect();
        assert_eq!(parts.len(), hooks);
        if kind.effect == TAUNT_ORB.effect {
            assert_eq!(parts[0].numbers[0], 0x4000_0007, "no harm");
            assert_eq!(parts[0].numbers[3], 0);
            let health = h.server.things.get(beetle).map(|t| t.health);
            assert_eq!(health, Some(100_000));
        }
    }
}

/// An item whose use names a pet no item's use makes (a decoy) is refused and kept, not used up for
/// nothing.
/// Behaviour: server.makes.nothing-used-up-for-what-cannot-be-made
#[test]
fn an_item_summoning_a_pet_no_use_makes_is_refused_and_kept() {
    use yalain_world::item_use::UsageEffect;
    const DECOY: u32 = 0x4700_1C72;
    const SUMMON: u32 = 0x7000_0F00;
    let mut h = Harness::new();
    h.add_creatures(1);
    let (b, me) = h.player_in_world("one", "Kestrel");
    h.run(10);
    h.server.effect_table.effects.insert(
        SUMMON,
        EffectDef {
            state: SUMMON,
            kind: Kind::Pet {
                class: 16,
                lifetime: Scale::default(),
            },
            duration: Scale::default(),
            priority: Scale::default(),
            fx: 0,
            external_flags: 0,
            internal_flags: 0,
            enum_value: 0,
            eq_class: 0,
            name: None,
        },
    );
    let mut props = Props::default();
    props.set("Item_IsDestroyOnUse", Prop::Bool(true));
    h.server
        .catalog
        .insert(yalain_testkit::test_template(DECOY, 0x777, props));
    h.server.item_uses.insert(
        DECOY,
        vec![UsageEffect {
            state: SUMMON,
            spellcraft: 1.0,
        }],
    );
    let world = *h.server.players.keys().next().expect("in");
    let item =
        yalain_world::inventory::give(&mut h.server, &mut h.net, world, DECOY, 1).expect("given");
    h.run(2);
    let from = h.bots[b].inbox.len();
    h.bots[b].send(
        true,
        queue::WEENIE,
        &server_call(0x8216_0016, &[yalain_testkit::play::use_of(item, me)]),
    );
    h.run(20);
    assert!(
        h.server.players[&world]
            .play
            .items
            .iter()
            .any(|i| i.id == item),
        "kept"
    );
    assert!(
        h.texts(b, from)
            .iter()
            .any(|(_, t)| *t == yalain_world::pets::text::SUMMONING_FAILS.0),
        "told"
    );
}
