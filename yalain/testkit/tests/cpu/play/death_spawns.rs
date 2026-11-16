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
    c.stats.death_effects = effects.iter().map(|e| ((*e).to_string(), 1)).collect();
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
        ends_at_logout: false,
        ends_at_teleport: false,
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
            event: None,
            template: helper,
            count: 2,
            chance: 1.0,
            offset: [0.0, -3.0],
            lasts: None,
            kingdom: None,
            at: None,
            to: None,
            name: None,
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

/// A creature a death makes holds the effects the world's rows give its template from its first
/// moment, as a camp's does.
/// Behaviour: server.creatures.effects-from-the-first-moment
#[test]
fn a_creature_a_death_makes_holds_the_effects_its_rows_give_its_template() {
    const HELD: u32 = 0x6F00_0124;
    let (mut h, me) = world();
    let helper = beetle_template(&h);
    h.server
        .effect_table
        .effects
        .insert(HELD, lasting(HELD, Kind::Other));
    h.server
        .creature_rows
        .effects
        .push(yalain_content::creature_rows::CreatureEffect {
            templates: vec![helper],
            effect: HELD,
            spellcraft: vec![(0.0, 7.0)],
        });
    h.server.death_spawns.insert(
        EFFECT.into(),
        DeathSpawn {
            event: None,
            template: helper,
            count: 1,
            chance: 1.0,
            offset: [0.0, -3.0],
            lasts: None,
            kingdom: None,
            at: None,
            to: None,
            name: None,
        },
    );
    let before = living(&h);
    kill(&mut h, me, &[EFFECT]);
    let made: Vec<u64> = living(&h)
        .into_iter()
        .filter(|id| !before.contains(id))
        .collect();
    assert_eq!(made.len(), 1);
    let held: Vec<(u32, f32)> =
        yalain_world::effects::records_of(&h.server, yalain_world::effects::On::Creature(made[0]))
            .iter()
            .map(|e| (e.state, e.spellcraft))
            .collect();
    assert_eq!(held, [(HELD, 7.0)]);
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
            event: None,
            template: helper,
            count: 1,
            chance: 1.0,
            offset: [0.0, 2.0],
            lasts: None,
            kingdom: None,
            at: None,
            to: None,
            name: None,
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
            event: None,
            template: helper,
            count: 3,
            chance: 0.0,
            offset: [0.0, 2.0],
            lasts: None,
            kingdom: None,
            at: None,
            to: None,
            name: None,
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
            event: None,
            template: helper,
            count: 5,
            chance: 1.0,
            offset: [0.0, 2.0],
            lasts: None,
            kingdom: None,
            at: None,
            to: None,
            name: None,
        },
    );
    for _ in 0..yalain_world::death_spawn::MOST {
        h.add_creatures(1);
        h.run(1);
        let beetle = *living(&h).first().expect("a beetle");
        let t = h.server.things.get_mut(beetle).expect("the beetle");
        t.health = 1;
        t.creature.as_mut().expect("a creature").stats.death_effects = vec![(EFFECT.into(), 1)];
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

/// A death effect whose row makes a portal makes it at its offset from the killer, and the portals
/// standing count toward how many of the effect's there may be, as creatures living do.
/// Behaviour: server.creatures.death-spawns-limited
#[test]
fn a_death_effects_portals_standing_count_toward_its_limit() {
    use yalain_content::{Prop, Props};
    const PORTAL: u32 = 0x4700_9102;
    let (mut h, me) = world();
    let mut props = Props::default();
    props.set("PortalDestination", Prop::Text("Somewhere".into()));
    let template = yalain_testkit::test_template(PORTAL, 0xF12, props);
    h.server.places_by_name.insert(
        "somewhere".into(),
        yalain_content::Place {
            cell: 0x7000_0001,
            origin: [10.0, 10.0, 50.0],
            rotation: [1.0, 0.0, 0.0, 0.0],
        },
    );
    h.server.death_spawns.insert(
        EFFECT.into(),
        DeathSpawn {
            event: None,
            template: PORTAL,
            count: 20,
            chance: 1.0,
            offset: [0.0, 2.0],
            lasts: None,
            kingdom: None,
            at: None,
            to: None,
            name: None,
        },
    );
    let standing = |h: &Harness| {
        h.server
            .things
            .by_id
            .values()
            .filter(|t| t.template == PORTAL)
            .count()
    };
    for _ in 0..2 {
        // A world's creatures come with a catalog of their own.
        h.add_creatures(1);
        h.server.catalog.insert(template.clone());
        h.run(1);
        kill(&mut h, me, &[EFFECT]);
        h.run(2);
    }
    assert_eq!(standing(&h), yalain_world::death_spawn::MOST);
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
            event: None,
            template: helper,
            count: 1,
            chance: 1.0,
            offset: [0.0, 2.0],
            lasts: None,
            kingdom: None,
            at: None,
            to: None,
            name: None,
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

/// An item whose use names a world's row that makes what this world can't make is refused and
/// kept: a practice gem whose dummy isn't described is not spent, and its quest doesn't move.
/// Behaviour: server.makes.nothing-used-up-for-what-cannot-be-made
#[test]
fn a_gem_whose_row_makes_what_cannot_be_made_is_refused_and_kept() {
    use yalain_content::{Prop, Props};
    const GEM: u32 = 0x4700_1853;
    const SUMMON: &str = "Eff_Summons";
    const MISSING: u32 = 0x4700_2DAE;
    let (mut h, _) = world();
    let me = h.server.players.values().next().expect("in").character;
    h.server.death_spawns.insert(
        SUMMON.into(),
        DeathSpawn {
            event: None,
            template: MISSING,
            count: 1,
            chance: 1.0,
            offset: [0.0, 2.0],
            lasts: None,
            kingdom: None,
            at: None,
            to: None,
            name: None,
        },
    );
    let mut props = Props::default();
    props
        .set("Usage_Effect2", Prop::Text(SUMMON.into()))
        .set("Item_IsDestroyOnUse", Prop::Bool(true));
    h.server
        .catalog
        .insert(yalain_testkit::test_template(GEM, 0x777, props));
    let world = *h.server.players.keys().next().expect("in");
    let gem =
        yalain_world::inventory::give(&mut h.server, &mut h.net, world, GEM, 1).expect("given");
    h.run(2);
    let before = living(&h);
    let from = h.bots[0].inbox.len();
    uses(&mut h, 0, me, gem);
    h.run(20);
    assert_eq!(living(&h), before, "nothing made");
    assert!(
        h.server.players[&world]
            .play
            .items
            .iter()
            .any(|i| i.id == gem),
        "kept"
    );
    assert!(
        h.texts(0, from)
            .iter()
            .any(|(_, t)| *t == yalain_world::pets::text::SUMMONING_FAILS.0),
        "told"
    );
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
            event: None,
            template: helper,
            count: 1,
            chance: 1.0,
            offset: [0.0, 2.0],
            lasts,
            kingdom: None,
            at: None,
            to: None,
            name: None,
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

/// The portals standing in the world: each one's id, where it stands and where it leads.
fn portals(h: &Harness) -> Vec<(u64, [f32; 3], Option<[f32; 3]>)> {
    let mut v: Vec<_> = h
        .server
        .things
        .by_id
        .values()
        .filter(|t| t.kind == yalain_world::things::Kind::Portal)
        .map(|t| {
            (
                t.id,
                t.at.map_or([0.0; 3], |a| a.origin),
                t.destination.map(|d| d.origin),
            )
        })
        .collect();
    v.sort_unstable_by_key(|p| p.0);
    v
}

/// The world with three beetles, a portal template that leads nowhere the world knows, and a
/// player.
fn portal_world() -> (Harness, u64, u32) {
    use yalain_content::{Prop, Props};
    const PORTAL: u32 = 0x4700_9201;
    let mut h = Harness::new();
    h.add_creatures(3);
    let mut props = Props::default();
    props
        .set("PortalDestination", Prop::Text("Nowhere_Known".into()))
        .set("PortalFlags", Prop::Word(0x1000_0000));
    h.server
        .catalog
        .insert(yalain_testkit::test_template(PORTAL, 0xF12, props));
    let (_, me) = h.player_in_world("one", "Kestrel");
    h.run(20);
    (h, me, PORTAL)
}

/// A row of a creature's own, at a place: the death of one of its kind, which names no death effect,
/// stands the row's portal at the row's place, leading to the row's own place, for the row's time;
/// a death while it stands makes no second one; once it is gone, the next death stands another.
/// Behaviour: server.creatures.a-kill-opens-a-portal-for-a-while
#[test]
fn a_creatures_death_stands_its_rows_portal_at_its_place_for_its_time_one_at_a_time() {
    let (mut h, me, portal) = portal_world();
    let platform = yalain_content::Place {
        cell: 0x0299_0109,
        origin: [70.0, 230.0, -44.6],
        rotation: [1.0, 0.0, 0.0, 0.0],
    };
    let below = yalain_content::Place {
        cell: 0x7000_0001,
        origin: [10.0, 10.0, 50.0],
        rotation: [1.0, 0.0, 0.0, 0.0],
    };
    let template = beetle_template(&h);
    h.server.death_spawns.insert(
        DeathSpawn::of_creature(template),
        DeathSpawn {
            event: None,
            template: portal,
            count: 1,
            chance: 1.0,
            offset: [0.0, 0.0],
            lasts: Some(60.0),
            kingdom: None,
            at: Some(platform),
            to: Some(below),
            name: None,
        },
    );
    assert!(portals(&h).is_empty());
    kill(&mut h, me, &[]);
    let stood = portals(&h);
    assert_eq!(stood.len(), 1, "{stood:?}");
    assert_eq!(stood[0].1, platform.origin, "at its place");
    assert_eq!(stood[0].2, Some(below.origin), "leading to its own place");
    kill(&mut h, me, &[]);
    assert_eq!(portals(&h), stood, "one at a time");
    let t = h.now.0;
    while h.now.0 < t + 61.0 {
        h.step();
    }
    assert!(portals(&h).is_empty(), "gone after its time");
    kill(&mut h, me, &[]);
    assert_eq!(portals(&h).len(), 1, "the next death stands another");
}

/// A row of a creature's own with no place stands what it makes at its offset from where the
/// creature died, as it faced, wherever its killer stands.
/// Behaviour: server.creatures.a-kill-opens-a-portal-for-a-while
#[test]
fn a_creatures_own_row_without_a_place_makes_its_creatures_where_it_died_wherever_its_killer_is() {
    let (mut h, me) = world();
    h.add_creatures(1);
    let helper = beetle_template(&h);
    h.server.death_spawns.insert(
        DeathSpawn::of_creature(helper),
        DeathSpawn {
            event: None,
            template: helper,
            count: 1,
            chance: 1.0,
            offset: [0.0, 2.0],
            lasts: None,
            kingdom: None,
            at: None,
            to: None,
            name: None,
        },
    );
    let beetle = *living(&h).last().expect("a beetle");
    let died = {
        let t = h.server.things.get_mut(beetle).expect("the beetle");
        let at = t.at.as_mut().expect("placed");
        at.rotation = [1.0, 0.0, 0.0, 0.0];
        *at
    };
    // The killer far off, in another landblock.
    for p in h.server.players.values_mut() {
        p.position.cell = 0x7000_0001;
    }
    let before = living(&h);
    kill(&mut h, me, &[]);
    let made: Vec<u64> = living(&h)
        .into_iter()
        .filter(|id| !before.contains(id))
        .collect();
    assert_eq!(made.len(), 1, "made though its killer is far");
    let got = h
        .server
        .things
        .get(made[0])
        .and_then(|t| t.at)
        .expect("placed");
    assert!(
        (got.origin[0] - died.origin[0]).abs() < 0.01
            && (got.origin[1] - (died.origin[1] + 2.0)).abs() < 0.01,
        "{got:?} from {died:?}"
    );
}

/// A row made only for killers of one kingdom: a death makes the rows for any killer and those for
/// its killer's own kingdom, and none of another kingdom's; a killer in no kingdom makes the rows
/// for any killer alone.
/// Behaviour: server.creatures.a-death-makes-what-its-killers-kingdom-calls-for
#[test]
fn a_death_makes_the_rows_for_any_killer_and_those_of_its_killers_kingdom_alone() {
    use yalain_world::kingdoms::{kingdom, set_kingdom};
    let (mut h, me) = world();
    h.add_creatures(6);
    let helper = beetle_template(&h);
    let row = |count: u32| DeathSpawn {
        event: None,
        template: helper,
        count,
        chance: 1.0,
        offset: [0.0, 2.0],
        lasts: None,
        kingdom: None,
        at: None,
        to: None,
        name: None,
    };
    let key = DeathSpawn::of_creature(helper);
    let rows = &mut h.server.death_spawns;
    rows.insert(key.clone(), row(1));
    rows.insert(DeathSpawn::for_killers_of(&key, kingdom::ORDER), row(2));
    rows.insert(DeathSpawn::for_killers_of(&key, kingdom::SHADOW), row(4));
    let made = |h: &mut Harness| {
        let before = living(h);
        kill(h, me, &[]);
        living(h)
            .into_iter()
            .filter(|id| !before.contains(id))
            .count()
    };
    assert_eq!(
        made(&mut h),
        1,
        "a killer in no kingdom: the row for any killer"
    );
    for p in h.server.players.values_mut() {
        set_kingdom(&mut p.qualities, kingdom::ORDER);
    }
    assert_eq!(made(&mut h), 1 + 2, "and the Order's, not the Shadow's");
    for p in h.server.players.values_mut() {
        set_kingdom(&mut p.qualities, kingdom::DOMINION);
    }
    assert_eq!(made(&mut h), 1, "a kingdom with no row of its own");
}

/// One creature with several named rows, for any killer and for its killer's kingdom: a death makes
/// every one of them.
/// Behaviour: server.creatures.a-death-makes-each-of-its-rows
#[test]
fn a_death_makes_each_of_its_creatures_named_rows() {
    use yalain_world::kingdoms::{kingdom, set_kingdom};
    let (mut h, me) = world();
    h.add_creatures(4);
    let helper = beetle_template(&h);
    let row = |count: u32| DeathSpawn {
        event: None,
        template: helper,
        count,
        chance: 1.0,
        offset: [0.0, 2.0],
        lasts: None,
        kingdom: None,
        at: None,
        to: None,
        name: None,
    };
    let key = DeathSpawn::of_creature(helper);
    let order = DeathSpawn::for_killers_of(&key, kingdom::ORDER);
    let rows = &mut h.server.death_spawns;
    rows.insert(DeathSpawn::named(&key, "first"), row(1));
    rows.insert(DeathSpawn::named(&key, "second"), row(2));
    rows.insert(DeathSpawn::named(&order, "orders"), row(4));
    for p in h.server.players.values_mut() {
        set_kingdom(&mut p.qualities, kingdom::ORDER);
    }
    let before = living(&h);
    kill(&mut h, me, &[]);
    let made = living(&h)
        .into_iter()
        .filter(|id| !before.contains(id))
        .count();
    assert_eq!(made, 1 + 2 + 4, "each of its rows, the Order's too");
}

/// A creature's row that starts an event: the person placed for the event stands from the next
/// step after the death for the row's time and no longer, and a death while it stands starts the
/// time again from that death.
/// Behaviour: server.creatures.a-death-starts-an-event
#[test]
fn a_death_starts_its_rows_event_and_the_person_placed_for_it_stands_its_time() {
    use yalain_content::events::{Event, EventTable, ObjectGate};
    const PERSON: u64 = 0x4002_9900_0000_0E02;
    let (mut h, me) = world();
    h.add_creatures(2);
    let at = h.server.players.values().next().expect("a player").position;
    h.server.content.objects = vec![yalain_content::ContentObject {
        object: Some(PERSON),
        template: 0x4700_1154,
        description: yalain_content::Description {
            name_text: None,
            package: 0x13C1,
            look: 0x1F00_0ED5,
            enter_fx: 0,
            scale: None,
            name: None,
            numbers: [0; 6],
            source: "authored for a test".into(),
        },
        cell: at.cell,
        origin: at.origin,
        rotation: [1.0, 0.0, 0.0, 0.0],
        kind: yalain_content::Kind::Static,
        label: String::new(),
        placed: "authored for a test".into(),
        destination: None,
        facts: yalain_content::Facts::default(),
        walks: None,
        gate: None,
    }];
    h.server.content.events = EventTable {
        events: vec![Event {
            ended_by: Vec::new(),
            key: "radiant".into(),
            game_event: None,
            windows: Vec::new(),
            on_players: Vec::new(),
            failure: None,
            hides: Vec::new(),
        }],
        camps: Vec::new(),
        objects: vec![ObjectGate {
            object: 0,
            event: "radiant".into(),
        }],
        steps: Vec::new(),
        lines: Vec::new(),
    };
    let template = beetle_template(&h);
    h.server.death_spawns.insert(
        DeathSpawn::of_creature(template),
        DeathSpawn {
            event: Some("radiant".into()),
            template: 0,
            count: 1,
            chance: 1.0,
            offset: [0.0, 0.0],
            lasts: Some(60.0),
            kingdom: None,
            at: None,
            to: None,
            name: None,
        },
    );
    let stands = |h: &Harness| yalain_world::events::content_standing(&h.server, PERSON).is_some();
    let run_to = |h: &mut Harness, unix: i64| {
        while h.unix < unix {
            h.step_by(1.0);
        }
        h.run(2);
    };
    h.run(2);
    assert!(!stands(&h), "not before the death");
    let before = living(&h);
    kill(&mut h, me, &[]);
    let died = h.unix;
    run_to(&mut h, died + 1);
    assert!(stands(&h), "from the step after the death");
    assert_eq!(
        living(&h).len(),
        before.len() - 1,
        "and the row makes nothing"
    );
    run_to(&mut h, died + 30);
    assert!(stands(&h), "for its time");
    kill(&mut h, me, &[]);
    let again = h.unix;
    run_to(&mut h, died + 61);
    assert!(stands(&h), "a death while it stands starts its time again");
    run_to(&mut h, again + 61);
    assert!(!stands(&h), "gone once its time is up");
}

/// A creature's row whose portal template requires a kingdom, and that names none itself, stands a
/// portal for that kingdom alone.
/// Behaviour: server.places.a-made-portal-keeps-its-templates-kingdom
#[test]
fn a_creatures_death_portal_is_for_the_kingdom_its_template_requires() {
    let (mut h, me, portal) = portal_world();
    let mut t = h.server.catalog.get(portal).cloned().expect("the portal");
    t.props
        .set("Usage_RequiredFaction", yalain_content::Prop::Word(4));
    h.server.catalog.insert(t);
    let template = beetle_template(&h);
    h.server.death_spawns.insert(
        DeathSpawn::of_creature(template),
        DeathSpawn {
            event: None,
            template: portal,
            count: 1,
            chance: 1.0,
            offset: [0.0, 0.0],
            lasts: Some(60.0),
            kingdom: None,
            at: None,
            to: Some(yalain_content::Place {
                cell: 0x7000_0001,
                origin: [10.0, 10.0, 50.0],
                rotation: [1.0, 0.0, 0.0, 0.0],
            }),
            name: None,
        },
    );
    kill(&mut h, me, &[]);
    let kingdoms: Vec<Option<u32>> = h
        .server
        .things
        .by_id
        .values()
        .filter(|t| t.kind == yalain_world::things::Kind::Portal)
        .map(|t| t.facts.kingdom)
        .collect();
    assert_eq!(kingdoms, [Some(4)], "the template's kingdom");
}

/// A creature's row whose template names no destination of its own (a plain object) but whose row
/// names a place to lead to stands a portal at its place, which takes its user to the row's place;
/// without that place the same row makes nothing (the template is no creature either); a
/// creature's template on such a row is made a creature.
/// Behaviour: server.creatures.a-death-spawns-row-leads-from-any-template
#[test]
fn a_creatures_death_makes_a_portal_of_a_template_with_no_destination_when_its_row_names_one() {
    use yalain_content::{Place, Props};
    const SHARD: u32 = 0x4700_9202;
    let mut h = Harness::new();
    h.add_creatures(3);
    h.server.catalog.insert(yalain_testkit::test_template(
        SHARD,
        0xF12,
        Props::default(),
    ));
    let (b, me) = h.player_in_world("one", "Kestrel");
    h.run(20);
    let platform = Place {
        cell: 0x0299_0109,
        origin: [70.0, 230.0, -44.6],
        rotation: [1.0, 0.0, 0.0, 0.0],
    };
    let chamber = Place {
        cell: 0x0299_0109,
        origin: [40.0, 200.0, -44.6],
        rotation: [1.0, 0.0, 0.0, 0.0],
    };
    let template = beetle_template(&h);
    let row = |to: Option<Place>| DeathSpawn {
        event: None,
        template: SHARD,
        count: 1,
        chance: 1.0,
        offset: [0.0, 0.0],
        lasts: Some(60.0),
        kingdom: None,
        at: Some(platform),
        to,
        name: None,
    };
    h.server
        .death_spawns
        .insert(DeathSpawn::of_creature(template), row(None));
    let before = h.server.things.by_id.len();
    kill(&mut h, me, &[]);
    assert!(portals(&h).is_empty(), "no place to lead to: no portal");
    assert!(
        h.server.things.by_id.values().all(|t| t.template != SHARD),
        "nor anything else ({before} things before)"
    );
    h.server
        .death_spawns
        .insert(DeathSpawn::of_creature(template), row(Some(chamber)));
    kill(&mut h, me, &[]);
    let stood = portals(&h);
    assert_eq!(stood.len(), 1, "{stood:?}");
    assert_eq!(stood[0].1, platform.origin, "at its place");
    assert_eq!(
        stood[0].2,
        Some(chamber.origin),
        "leading to the row's place"
    );
    let world = *h.server.players.keys().next().expect("in");
    if let Some(p) = h.server.players.get_mut(&world) {
        p.position.cell = platform.cell;
        p.position.origin = [
            platform.origin[0] + 1.0,
            platform.origin[1],
            platform.origin[2],
        ];
    }
    h.bots[b].send(
        true,
        auberean_protocol::queue::WEENIE,
        &yalain_testkit::play::server_call(
            0x8216_0016,
            &[yalain_testkit::play::use_of(stood[0].0, me)],
        ),
    );
    h.run(80);
    let at = h.server.players[&world].position;
    assert_eq!(
        (at.cell, at.origin),
        (chamber.cell, chamber.origin),
        "taken there"
    );
    // A creature's template on a row with a place to lead to is made a creature, never a portal.
    let mut creature_row = row(Some(chamber));
    creature_row.template = template;
    // Where it died: the portal still stands at the row's place, which would hold any other back.
    creature_row.at = None;
    h.server
        .death_spawns
        .insert(DeathSpawn::of_creature(template), creature_row);
    let (portals_before, before) = (portals(&h).len(), living(&h));
    kill(&mut h, me, &[]);
    assert_eq!(portals(&h).len(), portals_before, "no portal of a creature");
    assert!(
        living(&h).iter().any(|id| !before.contains(id)),
        "a creature made"
    );
}

/// The name a client is told an object by as it is made, if its create names one.
fn told_name(
    h: &Harness,
    b: usize,
    from: usize,
    id: u64,
) -> Option<auberean_protocol::text::StringInfo> {
    use auberean_protocol::types::WeenieDesc;
    h.world_messages(b, from).into_iter().find_map(|m| match m {
        auberean_protocol::ServerMessage::CreateObject(o)
            if o.id == id && o.weenie.mask & WeenieDesc::NAME != 0 =>
        {
            Some(o.weenie.name)
        }
        _ => None,
    })
}

/// A creature's row that names what it makes: its portal is told to the killer's client by the
/// row's name, a string reference, in place of its template's, and examined by it; a row naming it
/// in new prose has it told and examined by that text; a row naming nothing leaves its template's.
/// Behaviour: server.places.a-made-thing-takes-its-rows-name
#[test]
fn a_portal_a_row_makes_is_told_and_examined_by_the_rows_name() {
    use yalain_content::Words;
    const ROWS_NAME: (u32, u32) = (0x1B6D_27C1, 0x2500_1B44);
    let template_name = (0x1111_2222, 0x2500_1E91);
    for name in [
        Some(Words::Ref(ROWS_NAME.0, ROWS_NAME.1)),
        Some(Words::Literal("Way Below".into())),
        None,
    ] {
        let (mut h, me, portal) = portal_world();
        let mut named = h.server.catalog.get(portal).cloned().expect("its template");
        named.description.name = Some(template_name);
        named.facts.name = Some(template_name);
        h.server.catalog.insert(named);
        let template = beetle_template(&h);
        h.server.death_spawns.insert(
            DeathSpawn::of_creature(template),
            DeathSpawn {
                event: None,
                template: portal,
                count: 1,
                chance: 1.0,
                offset: [0.0, 2.0],
                lasts: None,
                kingdom: None,
                at: None,
                to: Some(yalain_content::Place {
                    cell: 0x7000_0001,
                    origin: [10.0, 10.0, 50.0],
                    rotation: [1.0, 0.0, 0.0, 0.0],
                }),
                name: name.clone(),
            },
        );
        let from = h.bots[0].inbox.len();
        kill(&mut h, me, &[]);
        h.run(5);
        let made = portals(&h);
        assert_eq!(made.len(), 1, "{name:?}");
        let told = told_name(&h, 0, from, made[0].0).expect("told with a name");
        let examined = examined(&mut h, made[0].0);
        let facts = h.server.things.get(made[0].0).expect("standing").facts;
        // The profile of an object of that name: the request repeated, then its lines.
        let of_name = |n| {
            let name = yalain_world::profile::item_named(n);
            yalain_world::profile::npc(0, name, &facts).packed(&[])
        };
        let want = match &name {
            Some(Words::Ref(s, t)) => {
                assert_eq!((told.string_id, told.table_id), (*s, *t));
                assert_eq!(told.literal, None);
                yalain_world::profile::string(*t, *s)
            }
            Some(Words::Literal(text)) => {
                let shown: Vec<u16> = text.encode_utf16().collect();
                assert_eq!(told.literal.map(|l| l.0), Some(shown));
                yalain_world::profile::literal(text)
            }
            None => {
                assert_eq!((told.string_id, told.table_id), template_name);
                yalain_world::profile::string(template_name.1, template_name.0)
            }
        };
        assert!(examined.ends_with(&of_name(want)), "examined: {name:?}");
    }
}

/// What a client's examination of an object says: its profile's bytes.
fn examined(h: &mut Harness, object: u64) -> Vec<u8> {
    use auberean_script::native_object::NativeObject;
    use yalain_world::calls::{self, Node};
    let mut bytes = Vec::new();
    for w in [0x4000_0002u32, 0] {
        bytes.extend(w.to_le_bytes());
    }
    bytes.extend(object.to_le_bytes());
    bytes.extend(0x3FFu32.to_le_bytes());
    let mut b = yalain_world::graph::Builder::new();
    b.native(154, NativeObject::Packed { ty: 154, bytes });
    let about = auberean_script::args::Arg::Object(b.graph());
    let from = h.bots[0].inbox.len();
    h.bots[0].send(
        true,
        auberean_protocol::queue::WEENIE,
        &yalain_testkit::play::server_call(yalain_world::requests::function::EXAMINE, &[about]),
    );
    h.run(30);
    h.world_messages(0, from)
        .into_iter()
        .filter_map(|m| match m {
            auberean_protocol::ServerMessage::PlayerCall(c)
                if c.function.0 == yalain_world::profile::EXAMINATION_PROFILE =>
            {
                let v = calls::values(&c.args?.0).ok()?;
                match v[0].root() {
                    Some(Node::Native { ty: 153, bytes }) => Some(bytes.clone()),
                    _ => None,
                }
            }
            _ => None,
        })
        .collect::<Vec<_>>()
        .concat()
}
