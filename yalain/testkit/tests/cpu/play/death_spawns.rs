//! What a creature's death does to its killer beyond its quests: an effect that lasts goes on the
//! killer, and a world's row makes creatures around the killer as it faces. Fixture: the test
//! world's beetles, one player.

use yalain_content::DeathSpawn;
use yalain_testkit::Harness;
use yalain_world::combat;
use yalain_world::effect_defs::{EffectDef, Kind, Scale};
use yalain_world::item_use::UsageEffect;
use yalain_world::things::Mode;

const CURSE: u32 = 0x6F00_0123;
const EFFECT: &str = "Eff_Helpers";

fn world() -> (Harness, u64) {
    let mut h = Harness::new();
    h.add_creatures(1);
    let (_, me) = h.player_in_world("one", "Kestrel");
    h.run(20);
    (h, me)
}

fn living(h: &Harness) -> Vec<u64> {
    let mut v: Vec<u64> = h
        .server
        .things
        .by_id
        .values()
        .filter(|t| t.creature.as_ref().is_some_and(|c| c.mode != Mode::Dead))
        .map(|t| t.id)
        .collect();
    v.sort_unstable();
    v
}

/// A live creature of the world's, hurt to death by this character; its template and its id.
fn kill(h: &mut Harness, by: u64, effects: &[&str]) -> (u32, u64) {
    let beetle = *living(h).last().expect("a beetle");
    let t = h.server.things.get_mut(beetle).expect("the beetle");
    t.health = 10;
    t.max_health = 10;
    let template = t.template;
    let c = t.creature.as_mut().expect("a creature");
    c.stats.attacks_first = false;
    c.stats.death_effects = effects.iter().map(ToString::to_string).collect();
    let clock = h.clock();
    combat::hurt_creature(&mut h.server, &mut h.net, beetle, by, 50, clock);
    (template, beetle)
}

fn beetle_template(h: &Harness) -> u32 {
    let id = *living(h).last().expect("a beetle");
    h.server.things.get(id).expect("the beetle").template
}

fn lasting(state: u32, kind: Kind) -> EffectDef {
    EffectDef {
        state,
        kind,
        duration: Scale(vec![(0.0, 14_400.0)]),
        priority: Scale(vec![(0.0, 1.0)]),
        fx: 0,
        external_flags: 0,
        internal_flags: 0x1100_0003,
        enum_value: 0,
        eq_class: 4174,
        name: None,
    }
}

/// A death effect that lasts goes on the killer alone, from the creature, for its whole duration.
/// Behaviour: server.creatures.death-effect-on-killer
#[test]
fn a_death_effect_that_lasts_goes_on_the_killer_from_the_creature() {
    let (mut h, me) = world();
    let (_, other) = h.player_in_world("two", "Wren");
    h.run(5);
    h.server
        .effect_table
        .effects
        .insert(CURSE, lasting(CURSE, Kind::Other));
    let template = beetle_template(&h);
    h.server.death_effects.insert(
        template,
        vec![UsageEffect {
            state: CURSE,
            spellcraft: 1.0,
        }],
    );
    let (_, beetle) = kill(&mut h, me, &[]);
    let holding = |h: &Harness, who: u64| -> Vec<(u32, u64, f32)> {
        h.server
            .players
            .values()
            .filter(|p| p.character == who)
            .flat_map(|p| p.play.effects.iter())
            .map(|e| (e.state, e.caster, e.timeout))
            .collect()
    };
    assert_eq!(holding(&h, me), vec![(CURSE, beetle, 14_400.0)]);
    assert_eq!(holding(&h, other), vec![], "one who only watched");
}

/// An effect that moves a quest is the quests' business, and is not applied a second time: a kill
/// that names none of its death effects as a quest's moves nothing by this path.
/// Behaviour: server.creatures.death-effect-on-killer
#[test]
fn a_death_effect_that_moves_a_quest_is_not_put_on_the_killer_again() {
    const QUEST: u32 = 0x4000_0123;
    let (mut h, me) = world();
    h.server.quests.quests.push(super::kill_quests::quest(3));
    h.server.effect_table.effects.insert(
        CURSE + 2,
        lasting(
            CURSE + 2,
            Kind::Quest {
                quest: QUEST,
                update: 0x1000_0000,
            },
        ),
    );
    let template = beetle_template(&h);
    h.server.death_effects.insert(
        template,
        vec![UsageEffect {
            state: CURSE + 2,
            spellcraft: 1.0,
        }],
    );
    kill(&mut h, me, &[]);
    let p = h.server.players.values().next().expect("in the world");
    assert!(p.play.quests.is_empty(), "{:?}", p.play.quests);
}

/// A death effect makes creatures at its offset from the killer, as the killer faces, and they stay
/// in the world for no camp.
/// Behaviour: server.creatures.death-spawn-around-killer
#[test]
fn a_death_spawn_stands_at_its_offset_from_the_killer_as_the_killer_faces() {
    let (mut h, me) = world();
    let helper = beetle_template(&h);
    h.server.death_spawns.insert(
        EFFECT.into(),
        DeathSpawn {
            template: helper,
            count: 2,
            chance: 1.0,
            offset: [0.0, -3.0],
            lasts: None,
        },
    );
    // A quarter turn about the vertical.
    let half = std::f32::consts::FRAC_1_SQRT_2;
    let at = {
        let p = h.server.players.values_mut().next().expect("in the world");
        p.position.rotation = [half, 0.0, 0.0, half];
        p.position
    };
    let before = living(&h);
    kill(&mut h, me, &[EFFECT]);
    let made: Vec<u64> = living(&h)
        .into_iter()
        .filter(|id| !before.contains(id))
        .collect();
    assert_eq!(made.len(), 2);
    for id in made {
        let t = h.server.things.get(id).expect("made");
        let got = t.at.expect("placed");
        assert!(
            (got.origin[0] - (at.origin[0] + 3.0)).abs() < 0.01,
            "{got:?} from {at:?}"
        );
        assert!((got.origin[1] - at.origin[1]).abs() < 0.01, "{got:?}");
        assert_eq!(t.creature.as_ref().expect("a creature").camp, None);
    }
}

/// The kill goes to who did the most damage, wherever it has gone since. What the death makes appears
/// only if that one stands near the death: not beside whoever it now stands among, and not in other
/// cells altogether. One at the edge of how near makes it.
/// Behaviour: server.creatures.death-spawn-near-the-death
#[test]
fn a_killer_far_from_the_death_makes_nothing() {
    let (mut h, me) = world();
    let helper = beetle_template(&h);
    h.server.death_spawns.insert(
        EFFECT.into(),
        DeathSpawn {
            template: helper,
            count: 1,
            chance: 1.0,
            offset: [0.0, 2.0],
            lasts: None,
        },
    );
    let beetle = *living(&h).last().expect("a beetle");
    let there = h
        .server
        .things
        .get(beetle)
        .and_then(|t| t.at)
        .expect("placed");
    let at = |h: &mut Harness, from_it: f32, cell: Option<u32>| -> usize {
        // Another of them, standing where the first stood.
        let template = h.server.catalog.get(helper).expect("its template").clone();
        let stats = yalain_content::CreatureStats::from_props(&template.props).expect("a creature");
        let now = h.clock().local.0;
        yalain_world::spawn::make(&mut h.server, &template, stats, (there, 0), None, None, now);
        for p in h.server.players.values_mut() {
            p.position = there;
            p.position.origin[0] += from_it;
            if let Some(c) = cell {
                p.position.cell = c;
            }
        }
        let before = living(h).len();
        kill(h, me, &[EFFECT]);
        // Less the one killed.
        living(h).len() + 1 - before
    };
    let near = yalain_world::death_spawn::NEAR_THE_DEATH;
    assert_eq!(at(&mut h, near + 1.0, None), 0, "past how near");
    assert_eq!(
        at(&mut h, 0.0, Some(0x0A98_0101)),
        0,
        "in another block's cells"
    );
    assert_eq!(at(&mut h, near - 1.0, None), 1, "within it");
    assert_eq!(at(&mut h, 0.0, None), 1, "beside it");
}

/// Each is made by its chance: none at no chance.
/// Behaviour: server.creatures.death-spawn-around-killer
#[test]
fn a_death_spawn_with_no_chance_makes_nothing() {
    let (mut h, me) = world();
    let helper = beetle_template(&h);
    h.server.death_spawns.insert(
        EFFECT.into(),
        DeathSpawn {
            template: helper,
            count: 3,
            chance: 0.0,
            offset: [0.0, 2.0],
            lasts: None,
        },
    );
    let before = living(&h).len();
    kill(&mut h, me, &[EFFECT]);
    assert_eq!(living(&h).len(), before - 1);
}

/// However many kills there are with no one to fight what they make, the world holds only so many
/// of one effect's, and more come when those are gone.
/// Behaviour: server.creatures.death-spawns-limited
#[test]
fn only_so_many_of_a_death_effects_creatures_live_at_once() {
    let (mut h, me) = world();
    let helper = beetle_template(&h);
    h.server.death_spawns.insert(
        EFFECT.into(),
        DeathSpawn {
            template: helper,
            count: 5,
            chance: 1.0,
            offset: [0.0, 2.0],
            lasts: None,
        },
    );
    for _ in 0..yalain_world::death_spawn::MOST {
        h.add_creatures(1);
        h.run(1);
        let beetle = *living(&h).first().expect("a beetle");
        let t = h.server.things.get_mut(beetle).expect("the beetle");
        t.health = 1;
        t.creature.as_mut().expect("a creature").stats.death_effects = vec![EFFECT.into()];
        t.creature.as_mut().expect("a creature").stats.attacks_first = false;
        let clock = h.clock();
        combat::hurt_creature(&mut h.server, &mut h.net, beetle, me, 50, clock);
    }
    let live = h.server.death_spawned[EFFECT]
        .iter()
        .filter(|id| {
            h.server
                .things
                .get(**id)
                .and_then(|t| t.creature.as_ref())
                .is_some_and(|c| c.mode != Mode::Dead)
        })
        .count();
    assert!(live <= yalain_world::death_spawn::MOST, "{live}");
    assert!(live >= yalain_world::death_spawn::MOST - 5, "{live}");
}

/// An item whose use names such an effect makes what it makes at its offset from its user, as the
/// use is done, the item spent; used again with no more, it makes no more.
/// Behaviour: server.creatures.an-items-use-makes-what-it-names
#[test]
fn an_items_use_makes_what_its_effect_names_before_its_user() {
    use auberean_protocol::queue;
    use yalain_content::{Prop, Props};
    use yalain_world::graph::{Builder, Slot};
    const GEM: u32 = 0x4700_1853;
    const SUMMON: &str = "Eff_Summons";
    let (mut h, _) = world();
    let (b, me) = (0, h.server.players.values().next().expect("in").character);
    let helper = beetle_template(&h);
    h.server.death_spawns.insert(
        SUMMON.into(),
        DeathSpawn {
            template: helper,
            count: 1,
            chance: 1.0,
            offset: [0.0, 2.0],
            lasts: None,
        },
    );
    let mut props = Props::default();
    props
        .set("Usage_Effect2", Prop::Text(SUMMON.into()))
        .set("Item_IsDestroyOnUse", Prop::Bool(true));
    h.server
        .catalog
        .insert(yalain_testkit::test_template(GEM, 0x777, props));
    h.server.item_uses.insert(
        GEM,
        vec![UsageEffect {
            state: CURSE,
            spellcraft: 1.0,
        }],
    );
    let world = *h.server.players.keys().next().expect("in");
    let gem =
        yalain_world::inventory::give(&mut h.server, &mut h.net, world, GEM, 1).expect("given");
    h.run(2);
    let at = h.server.players[&world].position;
    let before = living(&h);
    let mut use_of = Builder::new();
    use_of.object(
        0x245,
        &[
            Slot::Ref(None),
            Slot::Ref(None),
            Slot::Long(gem),
            Slot::Ref(None),
            Slot::Int(0),
            Slot::Long(me),
            Slot::Int((-1.0f32).to_bits()),
            Slot::Long(0),
            Slot::Int(0),
            Slot::Long(0),
            Slot::Int(0),
            Slot::Ref(None),
            Slot::Int(0),
            Slot::Int(316),
            Slot::Int(0),
            Slot::Int(0),
        ],
    );
    let arg = auberean_script::args::Arg::Object(use_of.graph());
    h.bots[b].send(
        true,
        queue::WEENIE,
        &yalain_testkit::play::server_call(0x8216_0016, &[arg]),
    );
    h.run(20);
    let made: Vec<u64> = living(&h)
        .into_iter()
        .filter(|id| !before.contains(id))
        .collect();
    assert_eq!(made.len(), 1);
    let got = h
        .server
        .things
        .get(made[0])
        .and_then(|t| t.at)
        .expect("placed");
    let dx = got.origin[0] - at.origin[0];
    let dy = got.origin[1] - at.origin[1];
    assert!(
        ((dx * dx + dy * dy) - 4.0).abs() < 0.05,
        "{got:?} from {at:?}"
    );
    assert!(h.server.things.get(gem).is_none(), "spent");
}

/// Sends a use of an item the player holds.
fn uses(h: &mut Harness, b: usize, me: u64, item: u64) {
    use yalain_world::graph::{Builder, Slot};
    let mut use_of = Builder::new();
    use_of.object(
        0x245,
        &[
            Slot::Ref(None),
            Slot::Ref(None),
            Slot::Long(item),
            Slot::Ref(None),
            Slot::Int(0),
            Slot::Long(me),
            Slot::Int((-1.0f32).to_bits()),
            Slot::Long(0),
            Slot::Int(0),
            Slot::Long(0),
            Slot::Int(0),
            Slot::Ref(None),
            Slot::Int(0),
            Slot::Int(316),
            Slot::Int(0),
            Slot::Int(0),
        ],
    );
    let arg = auberean_script::args::Arg::Object(use_of.graph());
    h.bots[b].send(
        true,
        auberean_protocol::queue::WEENIE,
        &yalain_testkit::play::server_call(0x8216_0016, &[arg]),
    );
    h.run(20);
}

/// A gem that names only what a row makes, as a real gem does, and the player holding one.
fn gem(h: &mut Harness, lasts: Option<f32>) -> u64 {
    use yalain_content::{Prop, Props};
    const GEM: u32 = 0x4700_1853;
    let helper = beetle_template(h);
    h.server.death_spawns.insert(
        "Eff_Summons".into(),
        DeathSpawn {
            template: helper,
            count: 1,
            chance: 1.0,
            offset: [0.0, 2.0],
            lasts,
        },
    );
    let mut props = Props::default();
    props
        .set("Usage_Effect2", Prop::Text("Eff_Summons".into()))
        .set("Item_IsDestroyOnUse", Prop::Bool(true));
    h.server
        .catalog
        .insert(yalain_testkit::test_template(GEM, 0x777, props));
    let world = *h.server.players.keys().next().expect("in");
    let item =
        yalain_world::inventory::give(&mut h.server, &mut h.net, world, GEM, 1).expect("given");
    h.run(2);
    item
}

/// What a gem makes goes when its time is up, told to those who see it, not before; one killed just
/// before has its death's course.
/// Behaviour: server.creatures.a-gems-dummy-lasts-its-time
#[test]
fn a_gems_dummy_is_gone_when_its_time_is_up() {
    let (mut h, _) = world();
    let me = h.server.players.values().next().expect("in").character;
    let item = gem(&mut h, Some(90.0));
    let before = living(&h);
    uses(&mut h, 0, me, item);
    let made: Vec<u64> = living(&h)
        .into_iter()
        .filter(|id| !before.contains(id))
        .collect();
    assert_eq!(made.len(), 1);
    let dummy = made[0];
    let from = h.bots[0].inbox.len();
    let t = h.now.0;
    while h.now.0 < t + 80.0 {
        h.step();
    }
    assert!(
        h.server.things.get(dummy).is_some(),
        "still standing at 80 s"
    );
    while h.now.0 < t + 91.0 {
        h.step();
    }
    assert!(h.server.things.get(dummy).is_none(), "gone by 91 s");
    let told = h
        .world_messages(0, from)
        .into_iter()
        .any(|m| matches!(m, auberean_protocol::ServerMessage::ForgetObject(o) if o.0.id == dummy));
    assert!(told, "those who see it are told");
    let item = gem(&mut h, Some(90.0));
    let before = living(&h);
    uses(&mut h, 0, me, item);
    let dummy = living(&h)
        .into_iter()
        .find(|id| !before.contains(id))
        .expect("another");
    let t = h.now.0;
    while h.now.0 < t + 88.0 {
        h.step();
    }
    let clock = h.clock();
    h.server.things.get_mut(dummy).expect("it").health = 1;
    yalain_world::combat::hurt_creature(&mut h.server, &mut h.net, dummy, me, 50, clock);
    h.run(2);
    assert!(!living(&h).contains(&dummy), "killed");
    while h.now.0 < t + 89.5 {
        h.step();
    }
    assert!(
        h.server.things.get(dummy).is_some(),
        "its death has its course past its time"
    );
}

/// A gem makes what it names however many of them stand: what others have made takes nothing from
/// its user, nor does the kills' limit reach it.
/// Behaviour: server.creatures.a-gems-dummy-lasts-its-time
#[test]
fn a_gem_makes_its_dummy_however_many_stand() {
    let (mut h, _) = world();
    let me = h.server.players.values().next().expect("in").character;
    let helper = beetle_template(&h);
    let mut standing = Vec::new();
    for _ in 0..=yalain_world::death_spawn::MOST {
        let before = living(&h);
        let item = gem(&mut h, Some(90.0));
        uses(&mut h, 0, me, item);
        assert!(h.server.things.get(item).is_none(), "spent");
        let made: Vec<u64> = living(&h)
            .into_iter()
            .filter(|id| !before.contains(id))
            .collect();
        assert_eq!(made.len(), 1, "each use makes one");
        assert_eq!(
            h.server.things.get(made[0]).map(|t| t.template),
            Some(helper)
        );
        standing.push(made[0]);
    }
    assert!(standing.len() > yalain_world::death_spawn::MOST);
    assert!(
        h.server
            .death_spawned
            .get("Eff_Summons")
            .is_none_or(Vec::is_empty),
        "uses don't count against what kills may make"
    );
}
