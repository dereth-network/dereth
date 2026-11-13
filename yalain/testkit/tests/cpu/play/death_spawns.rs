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
            template: helper,
            count: 2,
            chance: 1.0,
            offset: [0.0, -3.0],
            lasts: None,
            kingdom: None,
            at: None,
            to: None,
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
            kingdom: None,
            at: None,
            to: None,
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
            kingdom: None,
            at: None,
            to: None,
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
            kingdom: None,
            at: None,
            to: None,
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
            template: PORTAL,
            count: 20,
            chance: 1.0,
            offset: [0.0, 2.0],
            lasts: None,
            kingdom: None,
            at: None,
            to: None,
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
            template: helper,
            count: 1,
            chance: 1.0,
            offset: [0.0, 2.0],
            lasts: None,
            kingdom: None,
            at: None,
            to: None,
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
            template: MISSING,
            count: 1,
            chance: 1.0,
            offset: [0.0, 2.0],
            lasts: None,
            kingdom: None,
            at: None,
            to: None,
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
            template: helper,
            count: 1,
            chance: 1.0,
            offset: [0.0, 2.0],
            lasts,
            kingdom: None,
            at: None,
            to: None,
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
            template: portal,
            count: 1,
            chance: 1.0,
            offset: [0.0, 0.0],
            lasts: Some(60.0),
            kingdom: None,
            at: Some(platform),
            to: Some(below),
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
            template: helper,
            count: 1,
            chance: 1.0,
            offset: [0.0, 2.0],
            lasts: None,
            kingdom: None,
            at: None,
            to: None,
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
