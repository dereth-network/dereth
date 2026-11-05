//! What is solid to what among the objects of one world, by their physics types and what each
//! passes through, and the touches the bodies keep of what they run into.
//! Fixture: the flat block, bodies and post of the objects tests; the physics types and masks the
//! 2017 servers gave a player, other players, creatures and items; a property list whose flags
//! take the bits the install's do (the new mover's type bit 4, the outdoor scenery's bit 6).

use auberean_assets::property::{PropertyDescriptor, PropertyList};
use auberean_physics::body::Body;
use auberean_physics::land::{Block, Land};
use auberean_physics::solid::Ethereality;
use auberean_physics::world::{World, WorldObject, MOVER_ETHEREAL, STATIC_ETHEREAL};

use crate::contact::objects::{at, body, post, run_north, GROUND_Z};

/// What a player moves through: four of its own kind's bits and three more (never the outdoor
/// scenery's bit 6, nor a creature's bit 22).
const PLAYER_PASSES: u64 = 0x3E20_0010;
/// A player of one race, a player of another, a creature, an item.
const PLAYER: u64 = 0x0480_0020;
const OTHER_PLAYER: u64 = 0x1080_0020;
const CREATURE: u64 = 1 << 22 | 1 << 5;
const CREATURE_PASSES: u64 = 0x4220_0010;
const ITEM: u64 = 0x30;

fn properties() -> PropertyList {
    let flag = |data| PropertyDescriptor {
        value_kind: 0x4000_0001,
        group: 0,
        data,
        report: 0,
        required: false,
    };
    PropertyList::from_descriptors(vec![(MOVER_ETHEREAL, flag(4)), (STATIC_ETHEREAL, flag(6))])
}

fn player(world: &mut World, land: &Land, id: u64, x: f32, y: f32, kind: u64) -> Body {
    let mut b = body(x, y);
    let key = world.insert_body(land, id, &mut b);
    world.set_ethereality(
        key,
        Ethereality {
            physics_type: kind,
            movement: PLAYER_PASSES,
            placement: 0,
        },
    );
    b
}

/// A creature stops a player running into it, and another player does not: a player passes
/// through the physics types its description's movement mask names, and every player's own type
/// carries a bit of that mask while a creature's carries none.
///
/// Behaviour: collision.solid.a-creature-blocks-a-player-and-another-player-does-not
#[test]
fn a_creature_blocks_a_player_and_another_player_does_not() {
    let land = crate::contact::objects::land();
    let mut world = World::new();
    let _other = player(&mut world, &land, 0x51, 70.0, 80.0, OTHER_PLAYER);
    let mut runner = player(&mut world, &land, 0x50, 70.0, 70.0, PLAYER);
    let farthest = run_north(&mut runner, &land, &mut world, 40);
    assert!(farthest > 82.0, "through the other player: {farthest}");

    let mut world = World::new();
    let mut creature = body(70.0, 80.0);
    let key = world.insert_body(&land, 0x60, &mut creature);
    world.set_ethereality(
        key,
        Ethereality {
            physics_type: CREATURE,
            movement: CREATURE_PASSES,
            placement: 0,
        },
    );
    let mut runner = player(&mut world, &land, 0x50, 70.0, 70.0, PLAYER);
    let farthest = run_north(&mut runner, &land, &mut world, 40);
    assert!(
        farthest <= 80.0 - 0.96 + 0.01 && farthest > 77.0,
        "stopped by the creature: {farthest}"
    );
}

/// Two bodies whose descriptions say nothing of what they are pass through nothing, and stop
/// each other.
///
/// Behaviour: collision.solid.bodies-with-no-masks-are-solid-to-each-other
#[test]
fn bodies_with_no_masks_are_solid_to_each_other() {
    let land = crate::contact::objects::land();
    let mut world = World::new();
    world.set_property_list(properties());
    let mut standing = body(70.0, 80.0);
    world.insert_body(&land, 0x51, &mut standing);
    let mut runner = body(70.0, 70.0);
    world.insert_body(&land, 0x50, &mut runner);
    let farthest = run_north(&mut runner, &land, &mut world, 40);
    assert!(farthest <= 80.0 - 0.96 + 0.01, "stopped: {farthest}");
}

/// Scenery placed outside is of the outdoor scenery's physics type, which no player passes
/// through: a player stops against it, and a mover that does pass through that type runs on. An
/// item lying in the way, of a type every player passes through, does not stop a player.
///
/// Behaviour: collision.solid.outdoor-scenery-blocks-a-player-and-an-item-does-not
#[test]
fn outdoor_scenery_blocks_a_player_and_an_item_does_not() {
    let mut block = Block::flat(0x40, 0x40, GROUND_Z);
    block.objects.push((at(70.0, 80.0), post()));
    let land = Land::new();
    land.insert(block);
    let mut world = World::new();
    world.set_property_list(properties());
    let mut runner = player(&mut world, &land, 0x50, 70.0, 70.0, PLAYER);
    let farthest = run_north(&mut runner, &land, &mut world, 40);
    assert!(
        farthest <= 80.0 - 0.5 - 0.48 + 0.01 && farthest > 77.0,
        "stopped by the scenery: {farthest}"
    );
    // A mover that passes through the outdoor scenery's type runs through it.
    let mut b = body(70.0, 70.0);
    world.remove(runner.key().expect("in the world"));
    let key = world.insert_body(&land, 0x52, &mut b);
    world.set_ethereality(
        key,
        Ethereality {
            physics_type: PLAYER,
            movement: PLAYER_PASSES | 0x40,
            placement: 0,
        },
    );
    let through = run_north(&mut b, &land, &mut world, 40);
    assert!(through > 82.0, "through the scenery: {through}");

    let land = crate::contact::objects::land();
    let mut world = World::new();
    world.set_property_list(properties());
    let item = world.insert(
        &land,
        0x70,
        &WorldObject {
            shape: post(),
            position: at(70.0, 76.0),
            is_static: false,
            passable: false,
        },
    );
    world.set_ethereality(
        item,
        Ethereality {
            physics_type: ITEM,
            movement: 0,
            placement: 0,
        },
    );
    let mut runner = player(&mut world, &land, 0x50, 70.0, 70.0, PLAYER);
    let farthest = run_north(&mut runner, &land, &mut world, 40);
    assert!(farthest > 78.0, "over the item: {farthest}");
}

/// A body that runs into another keeps the touch, and the other keeps it back; the runner
/// reports it beginning.
///
/// Behaviour: collision.solid.a-body-and-what-it-runs-into-both-keep-the-touch
#[test]
fn a_body_and_what_it_runs_into_both_keep_the_touch() {
    let land = crate::contact::objects::land();
    let mut world = World::new();
    let mut standing = body(70.0, 80.0);
    let s = world.insert_body(&land, 0x51, &mut standing);
    let mut runner = body(70.0, 70.0);
    let r = world.insert_body(&land, 0x50, &mut runner);
    run_north(&mut runner, &land, &mut world, 40);
    let kept = |of, id| {
        world
            .collisions(of)
            .is_some_and(|c| c.contacts.contains_key(&id))
    };
    assert!(kept(r, 0x51), "the runner keeps the touch");
    assert!(kept(s, 0x50), "the one run into keeps it too");
    world.report_collisions(r, false);
    let reports = world.take_reports(r);
    assert!(
        reports
            .iter()
            .flat_map(|p| p.records.iter())
            .any(|c| c.object == 0x51 && c.begin),
        "the touch begins: {reports:?}"
    );
}

/// An object whose description pushes moves what touches it: a body run into a post that pushes
/// east is carried east for as long as it touches, and one that touches the same post with no
/// push is not.
///
/// Behaviour: collision.solid.what-a-body-touches-pushes-it
#[test]
fn what_a_body_touches_pushes_it() {
    use auberean_assets::property::{Property, PropertyCollection, PropertyGroup, PropertyValue};
    use auberean_physics::collision::manager::push;
    let list = PropertyList::from_descriptors(vec![(
        push::VELOCITY,
        PropertyDescriptor {
            value_kind: 0x4000_0004,
            group: 0x4000_000D,
            data: 0,
            report: 0,
            required: false,
        },
    )]);
    let run = |pushes: bool| {
        let land = crate::contact::objects::land();
        let mut world = World::new();
        world.set_property_list(list.clone());
        let at_post = world.insert(
            &land,
            0x77,
            &WorldObject {
                shape: post(),
                position: at(70.0, 75.0),
                is_static: true,
                passable: false,
            },
        );
        if pushes {
            let mut p = Property::named(&list, push::VELOCITY).expect("in the list");
            p.value = PropertyValue::Vector(auberean_primitives::Vec3::new(4.0, 0.0, 0.0));
            let mut g = PropertyGroup::new(p.group);
            g.properties.push(p);
            let groups = vec![g];
            assert!(world.apply_property_collection(at_post, &PropertyCollection { groups }));
        }
        let mut runner = body(70.0, 70.0);
        world.insert_body(&land, 0x50, &mut runner);
        run_north(&mut runner, &land, &mut world, 40);
        runner.position.frame.origin.x
    };
    assert!(
        (run(false) - 70.0).abs() < 0.5,
        "no push, it stays on its line"
    );
    assert!(run(true) > 71.0, "pushed east: {}", run(true));
}

/// The slowest impact a body reports is the one its speed and jump scales set, once it has moved:
/// its collisions hear it from the body.
///
/// Behaviour: collision.solid.a-bodys-collisions-take-its-slowest-impact
#[test]
fn a_bodys_collisions_take_its_slowest_impact() {
    let land = crate::contact::objects::land();
    let mut world = World::new();
    let mut runner = body(70.0, 70.0);
    let key = world.insert_body(&land, 0x50, &mut runner);
    runner.set_velocity_scale(2.0);
    runner.set_jump_scale(3.0);
    let (xy, z) = runner.min_reported_impact();
    assert!(xy >= 0.1 && z >= 0.1);
    run_north(&mut runner, &land, &mut world, 1);
    let current = &world.collisions(key).expect("kept").current;
    assert_eq!((current.min_impact_xy, current.min_impact_z), (xy, z));
}

/// In a cell's collision list what stands still comes first, in the order it came, then what moves:
/// so an object standing still put in beside a body already there is still met first.
///
/// Behaviour: collision.solid.what-stands-still-is-met-before-what-moves
#[test]
fn what_stands_still_is_met_before_what_moves() {
    let land = crate::contact::objects::land();
    let mut world = World::new();
    let mut runner = body(70.0, 70.0);
    let r = world.insert_body(&land, 0x50, &mut runner);
    let cell = world.position(r).expect("placed").cell;
    let stand = |world: &mut World, id, y| {
        world.insert(
            &land,
            id,
            &WorldObject {
                shape: post(),
                position: at(70.0, y),
                is_static: true,
                passable: false,
            },
        );
    };
    stand(&mut world, 0x77, 71.0);
    stand(&mut world, 0x78, 72.0);
    assert_eq!(world.collision_list(cell), vec![0x77, 0x78, 0x50]);
}

/// Two bodies agree whether a touch between them is passed through, whichever keeps it: either
/// finding the other not solid is enough, asked from both sides. A creature that passes through
/// players, run into by a player that does not pass through creatures, is a touch both keep as
/// passed through.
///
/// Behaviour: collision.solid.both-sides-of-a-touch-agree-it-is-passed-through
#[test]
fn both_sides_of_a_touch_agree_it_is_passed_through() {
    let land = crate::contact::objects::land();
    let mut world = World::new();
    world.set_property_list(properties());
    let mut npc = body(70.0, 80.0);
    let n = world.insert_body(&land, 0x60, &mut npc);
    world.set_ethereality(
        n,
        Ethereality {
            physics_type: CREATURE,
            movement: CREATURE_PASSES | PLAYER,
            placement: 0,
        },
    );
    let mut runner = player(&mut world, &land, 0x50, 70.0, 70.0, PLAYER);
    let r = runner.key().expect("in the world");
    run_north(&mut runner, &land, &mut world, 40);
    let passed = |of, id| {
        world
            .collisions(of)
            .and_then(|c| c.contacts.get(&id))
            .map(|c| c.record.ethereal)
    };
    assert_eq!(passed(r, 0x60), Some(true), "the player's touch");
    assert_eq!(passed(n, 0x50), Some(true), "the creature's touch");
}

/// A body running on the ground touches the landscape (its collisions note the plane and the
/// landscape, and it keeps that the plane is the land's); lifted into the air, it falls touching
/// nothing, off the landscape.
///
/// Behaviour: collision.contact.a-body-on-the-ground-is-on-the-landscape
#[test]
fn a_body_on_the_ground_is_on_the_landscape() {
    use auberean_physics::body::transient;
    let land = crate::contact::objects::land();
    let mut world = World::new();
    let mut b = body(70.0, 70.0);
    let key = world.insert_body(&land, 0x50, &mut b);
    run_north(&mut b, &land, &mut world, 5);
    let c = world.collisions(key).expect("a mover keeps its collisions");
    assert!(c.has_contact && c.on_landscape, "standing: {c:?}");
    assert!(!c.in_water);
    assert_ne!(b.transient_state() & transient::LANDSCAPE_CONTACT, 0);

    b.place(at(70.0, 70.0), 0.0);
    b.position.frame.origin.z = GROUND_Z + 5.0;
    b.contact = false;
    b.on_walkable = false;
    b.tick_in(&land, &mut world, 0.1);
    let c = world.collisions(key).expect("collisions");
    assert!(!c.on_landscape, "in the air: {c:?}");
}

/// Thrown into a post, a body rebounds off it by 1.005 of its speed into it (a little back the
/// way it came); an inelastic body stops dead.
///
/// Behaviour: collision.contact.an-inelastic-body-stops-dead-where-another-rebounds
#[test]
fn an_inelastic_body_stops_dead_where_another_rebounds() {
    let land = crate::contact::objects::land();
    let hit = |inelastic: bool| {
        let mut world = World::new();
        world.insert(
            &land,
            0x77,
            &WorldObject {
                shape: post(),
                position: at(70.0, 72.0),
                is_static: true,
                passable: false,
            },
        );
        let mut b = body(70.0, 70.0);
        b.set_inelastic(inelastic);
        b.position.frame.origin.z = GROUND_Z + 0.5;
        b.contact = false;
        b.on_walkable = false;
        b.add_impulse(dereth_primitives::Vec3::new(0.0, 10.0, 0.0));
        for _ in 0..10 {
            b.tick_in(&land, &mut world, 0.1);
            if b.velocity.y <= 0.0 {
                return b.velocity;
            }
        }
        panic!("never hit the post: {:?}", b.position);
    };
    let v = hit(false);
    assert!(v.y < 0.0 && v.y > -0.5, "rebounds a little: {v:?}");
    let v = hit(true);
    assert_eq!(v, dereth_primitives::Vec3::ZERO, "stops dead");
}

/// A missile that goes off near its target stops within its reach of the target, short of the
/// target's shape, and fades out; one aimed at another object flies on to the target's shape.
///
/// Behaviour: missile.detonate.a-missile-goes-off-within-its-reach-of-its-target
#[test]
fn a_missile_goes_off_within_its_reach_of_its_target() {
    use auberean_physics::body::missile::NoObjects;
    use auberean_physics::body::Notice;
    use auberean_physics::missile::{aim, Guidance, MissileTargetingParameters};
    let land = crate::contact::objects::land();
    let fly = |target: u64| {
        let mut world = World::new();
        world.insert(
            &land,
            0x77,
            &WorldObject {
                shape: post(),
                position: at(70.0, 75.0),
                is_static: true,
                passable: false,
            },
        );
        let mut b = body(70.0, 60.0);
        b.position.frame.origin.z = GROUND_Z + 0.5;
        b.contact = false;
        b.on_walkable = false;
        let mut g = Guidance::new();
        g.params.set_detonation_proximity(3.0, 1.0);
        g.targeting = MissileTargetingParameters {
            bits: aim::TARGET_OBJECT,
            target,
            ..MissileTargetingParameters::default()
        };
        g.moving = true;
        b.missile = Some(Box::new(g));
        b.velocity = dereth_primitives::Vec3::new(0.0, 10.0, 0.0);
        let mut farthest = f32::MIN;
        for _ in 0..30 {
            b.tick_in(&land, &mut world, 0.1);
            b.update_misc(&land, &mut world, &mut NoObjects, None);
            farthest = farthest.max(b.position.frame.origin.y);
        }
        let faded = b
            .notices
            .iter()
            .any(|n| matches!(n, Notice::FadeOut { fade, .. } if *fade == 0.5));
        (farthest, faded)
    };
    let (fused, faded) = fly(0x77);
    assert!(fused < 71.0, "goes off short of the post: {fused}");
    assert!(faded, "and fades out");
    let (plain, _) = fly(0x78);
    assert!(plain > 73.5, "flies on to the post: {plain}");
}

/// A body's collision settings as they stand are its physics type, what it passes through being
/// placed, and what it would pass through moving (not what its last move let it through).
///
/// Behaviour: collision.solid.a-bodys-collision-state-is-its-type-and-what-it-passes-through
#[test]
fn a_bodys_collision_state_is_its_type_and_what_it_passes_through() {
    use auberean_physics::collision::settings;
    let land = crate::contact::objects::land();
    let mut world = World::new();
    let mut b = body(70.0, 70.0);
    let key = world.insert_body(&land, 0x50, &mut b);
    world.set_ethereality(
        key,
        Ethereality {
            physics_type: PLAYER,
            movement: PLAYER_PASSES,
            placement: ITEM,
        },
    );
    let s = world.collision_state(key).expect("in the table");
    assert_eq!(s.physics_type.mask, PLAYER);
    assert_eq!(s.movement_physics.mask, PLAYER_PASSES);
    assert_eq!(s.placement_physics.mask, ITEM);
    for bit in [
        settings::PHYSICS_TYPE,
        settings::MOVEMENT_PHYSICS,
        settings::PLACEMENT_PHYSICS,
    ] {
        assert!(s.has(bit), "{bit:#x}");
    }
    assert!(world.collision_state(9999).is_none());
}

/// Scenery a landblock places takes the properties of the entities it is made from, as any
/// object made from an entity does, and is of the outdoor scenery's type besides: a post whose
/// entity makes it of the type every new mover passes through lets a player run through it. Having
/// no object id, it is never among what the body touches; the body keeps its properties instead.
///
/// Behaviour: collision.solid.scenery-takes-its-entitys-properties
#[test]
fn scenery_takes_its_entitys_properties() {
    use std::sync::Arc;

    use auberean_assets::property::{Property, PropertyCollection, PropertyGroup, PropertyValue};
    use auberean_physics::object::Shape;
    const SLIPPERY: u32 = 0x5000_0001;
    let d = |value_kind, group, data| PropertyDescriptor {
        value_kind,
        group,
        data,
        report: 0,
        required: false,
    };
    let list = PropertyList::from_descriptors(vec![
        (MOVER_ETHEREAL, d(0x4000_0001, 0x4000_0003, 4)),
        (STATIC_ETHEREAL, d(0x4000_0001, 0x4000_0003, 6)),
        (SLIPPERY, d(0x4000_0003, 0x4000_0007, 0)),
    ]);
    let named = |id, value| {
        let mut p = Property::named(&list, id).expect("in the list");
        p.value = value;
        let mut g = PropertyGroup::new(p.group);
        g.properties.push(p);
        g
    };
    let entity = PropertyCollection {
        groups: vec![
            named(MOVER_ETHEREAL, PropertyValue::Bool(true)),
            named(SLIPPERY, PropertyValue::Float(0.25)),
        ],
    };
    let mut block = Block::flat(0x40, 0x40, GROUND_Z);
    block.objects.push((
        at(70.0, 80.0),
        Arc::new(Shape {
            properties: vec![Arc::new(entity)],
            ..(*post()).clone()
        }),
    ));
    let land = Land::new();
    land.insert(block);
    let mut world = World::new();
    world.set_property_list(list);
    let mut runner = player(&mut world, &land, 0x50, 70.0, 70.0, PLAYER);
    let through = run_north(&mut runner, &land, &mut world, 40);
    assert!(through > 82.0, "through the post: {through}");
    let (post, _, _) = world
        .entries()
        .find(|(_, id, p)| *id == 0 && p.frame.origin.y == 80.0)
        .expect("the post");
    assert_eq!(
        world.physics_type(post).map(|t| t.mask),
        Some(1 << 4 | 1 << 6)
    );
    let c = world
        .collisions(runner.key().expect("in the world"))
        .expect("collisions");
    assert!(c.contacts.is_empty(), "{:?}", c.contacts);
    assert!(
        c.property_lists.contains_key(&post),
        "{:?}",
        c.property_lists
    );
}

/// An object's description sets its own properties by group: a gameplay property is kept as the
/// object's own, so a body running into it keeps the object's properties among what it touches;
/// the physics property naming a physics material keeps the material's id; an ethereal truth
/// value sets its bit of the object's physics type.
///
/// Behaviour: collision.solid.an-objects-description-sets-what-touching-it-reports
#[test]
fn an_objects_description_sets_what_touching_it_reports() {
    use auberean_assets::property::{Property, PropertyCollection, PropertyGroup, PropertyValue};
    use auberean_primitives::DataId;
    const SLIPPERY: u32 = 0x5000_0001;
    const MATERIAL: u32 = 0x4000_0001;
    const SOLID_TO_ITEMS: u32 = 0x5000_0002;
    let d = |value_kind, group, data| PropertyDescriptor {
        value_kind,
        group,
        data,
        report: 0,
        required: false,
    };
    let list = PropertyList::from_descriptors(vec![
        (MATERIAL, d(0x4000_0008, 0x4000_0001, 0)),
        (SLIPPERY, d(0x4000_0003, 0x4000_0007, 0)),
        (SOLID_TO_ITEMS, d(0x4000_0001, 0x4000_0003, 9)),
    ]);
    let land = crate::contact::objects::land();
    let mut world = World::new();
    world.set_property_list(list.clone());
    let post = world.insert(
        &land,
        0x77,
        &WorldObject {
            shape: post(),
            position: at(70.0, 75.0),
            is_static: true,
            passable: false,
        },
    );
    let named = |id, value| {
        let mut p = Property::named(&list, id).expect("in the list");
        p.value = value;
        p
    };
    let mut groups = Vec::new();
    for p in [
        named(MATERIAL, PropertyValue::DataFile(DataId(0x3A00_0005))),
        named(SLIPPERY, PropertyValue::Float(0.25)),
        named(SOLID_TO_ITEMS, PropertyValue::Bool(true)),
    ] {
        let mut g = PropertyGroup::new(p.group);
        g.properties.push(p);
        groups.push(g);
    }
    assert!(world.apply_property_collection(post, &PropertyCollection { groups }));
    let (material, own) = world.object_properties(post);
    assert_eq!(material, 0x3A00_0005);
    assert!(own.is_some_and(|g| g.iter().any(|g| g.group == 0x4000_0007)));
    assert_eq!(world.physics_type(post).map(|t| t.mask), Some(1 << 9));

    let mut runner = body(70.0, 70.0);
    let r = world.insert_body(&land, 0x50, &mut runner);
    run_north(&mut runner, &land, &mut world, 30);
    let c = world.collisions(r).expect("collisions");
    assert!(
        c.property_lists.contains_key(&post),
        "{:?}",
        c.property_lists
    );
}

/// An object's drawing properties say how it is drawn: casting a volume of shadow, casting a
/// shadow on the landscape and being drawn with the objects seen through each set or clear their
/// bit of its graphics state; not being drawn in the game says whether it is; each is kept among
/// the object's own properties. Another drawing property sets nothing.
///
/// Behaviour: draw.volume.an-objects-drawing-properties-say-how-it-is-drawn
#[test]
fn an_objects_drawing_properties_say_how_it_is_drawn() {
    use auberean_assets::property::{Property, PropertyCollection, PropertyGroup, PropertyValue};
    use auberean_physics::world::{graphics, render};
    const RENDER: u32 = 0x4000_0002;
    const INDOOR_SHADOW_CAST: u32 = 0x4000_0017;
    let flag = |id| {
        (
            id,
            PropertyDescriptor {
                value_kind: 0x4000_0001,
                group: RENDER,
                data: 0,
                report: 0,
                required: false,
            },
        )
    };
    let list = PropertyList::from_descriptors(vec![
        flag(render::VOLUME_SHADOW_CAST),
        flag(render::LANDSCAPE_SHADOW_CAST),
        flag(render::TRANSLUCENT),
        flag(render::INVISIBLE_INGAME),
        flag(INDOOR_SHADOW_CAST),
    ]);
    let land = crate::contact::objects::land();
    let mut world = World::new();
    world.set_property_list(list.clone());
    let mut b = body(70.0, 70.0);
    let key = world.insert_body(&land, 0x51, &mut b);
    let drawn = |pairs: &[(u32, bool)]| {
        let mut g = PropertyGroup::new(RENDER);
        for &(id, on) in pairs {
            let mut p = Property::named(&list, id).expect("in the list");
            p.value = PropertyValue::Bool(on);
            g.properties.push(p);
        }
        PropertyCollection { groups: vec![g] }
    };
    assert!(!world.graphics_state(key, graphics::VOLUME_SHADOW));
    assert!(world.graphics_state(key, graphics::TAKES_LAND_SHADOW));
    assert_eq!(world.game_draw(key), None);
    assert!(world.apply_property_collection(
        key,
        &drawn(&[
            (render::VOLUME_SHADOW_CAST, true),
            (render::LANDSCAPE_SHADOW_CAST, true),
            (render::INVISIBLE_INGAME, true),
        ])
    ));
    assert!(world.graphics_state(key, graphics::VOLUME_SHADOW | graphics::LANDSCAPE_SHADOW));
    assert!(!world.graphics_state(key, graphics::TRANSLUCENT));
    assert_eq!(world.game_draw(key), Some(false), "not drawn in the game");
    let (_, own) = world.object_properties(key);
    assert!(
        own.is_some_and(|g| g.iter().any(|g| g.group == RENDER)),
        "kept"
    );
    assert!(world.apply_property_collection(
        key,
        &drawn(&[
            (render::VOLUME_SHADOW_CAST, false),
            (render::TRANSLUCENT, true)
        ])
    ));
    assert!(!world.graphics_state(key, graphics::VOLUME_SHADOW));
    assert!(world.graphics_state(key, graphics::TRANSLUCENT | graphics::LANDSCAPE_SHADOW));
    assert!(world.graphics_state(key, graphics::TAKES_LAND_SHADOW));
    assert!(!world.apply_property_collection(key, &drawn(&[(INDOOR_SHADOW_CAST, true)])));
}

/// An object made from an entity takes its properties whole: made from an entity that is made from
/// another, it is the inner one first (whose slipperiness and ethereal bit it takes) and then the
/// outer one's own (a physics material), which replace the inner one's properties but leave its
/// ethereal bit set. A record that is not an entity applies nothing.
///
/// Behaviour: collision.solid.an-object-made-from-an-entity-takes-its-properties
#[test]
fn an_object_made_from_an_entity_takes_its_properties() {
    use std::sync::Arc;

    use auberean_assets::entity::Entity;
    use auberean_assets::property::{Property, PropertyCollection, PropertyGroup, PropertyValue};
    use auberean_primitives::DataId;
    const SLIPPERY: u32 = 0x5000_0001;
    const MATERIAL: u32 = 0x4000_0001;
    const SOLID_TO_ITEMS: u32 = 0x5000_0002;
    const INNER: u32 = 0x4700_0001;
    const OUTER: u32 = 0x4700_0002;
    let d = |value_kind, group, data| PropertyDescriptor {
        value_kind,
        group,
        data,
        report: 0,
        required: false,
    };
    let list = PropertyList::from_descriptors(vec![
        (MATERIAL, d(0x4000_0008, 0x4000_0001, 0)),
        (SLIPPERY, d(0x4000_0003, 0x4000_0007, 0)),
        (SOLID_TO_ITEMS, d(0x4000_0001, 0x4000_0003, 9)),
    ]);
    let named = |id, value| {
        let mut p = Property::named(&list, id).expect("in the list");
        p.value = value;
        let mut g = PropertyGroup::new(p.group);
        g.properties.push(p);
        g
    };
    let inner = Arc::new(Entity {
        object: Some(0x1F00_0010),
        properties: Some(PropertyCollection {
            groups: vec![
                named(SLIPPERY, PropertyValue::Float(0.25)),
                named(SOLID_TO_ITEMS, PropertyValue::Bool(true)),
            ],
        }),
        ..Entity::default()
    });
    let outer = Arc::new(Entity {
        object: Some(INNER),
        properties: Some(PropertyCollection {
            groups: vec![named(
                MATERIAL,
                PropertyValue::DataFile(DataId(0x3A00_0005)),
            )],
        }),
        ..Entity::default()
    });
    let lookup = |id: u32| match id {
        INNER => Some(Arc::clone(&inner)),
        OUTER => Some(Arc::clone(&outer)),
        _ => None,
    };
    let land = crate::contact::objects::land();
    let mut world = World::new();
    world.set_property_list(list.clone());
    let place = |world: &mut World, id| {
        world.insert(
            &land,
            id,
            &WorldObject {
                shape: post(),
                position: at(70.0, 75.0),
                is_static: true,
                passable: false,
            },
        )
    };

    let only_inner = place(&mut world, 0x76);
    assert!(world.apply_entity_properties(only_inner, INNER, &lookup));
    let (_, own) = world.object_properties(only_inner);
    assert!(own.is_some_and(|g| g.iter().any(|g| g.group == 0x4000_0007)));

    let post = place(&mut world, 0x77);
    assert!(world.apply_entity_properties(post, OUTER, &lookup));
    let (material, own) = world.object_properties(post);
    assert_eq!(material, 0x3A00_0005);
    assert!(
        own.is_some_and(|g| g.iter().all(|g| g.group != 0x4000_0007)),
        "the inner entity's slipperiness gave way to the outer's own: {own:?}"
    );
    assert_eq!(world.physics_type(post).map(|t| t.mask), Some(1 << 9));

    let plain = place(&mut world, 0x78);
    assert!(!world.apply_entity_properties(plain, 0x1F00_0010, &lookup));
    assert_eq!(world.object_properties(plain), (0, None));
}

/// An object is inside a detection's volume when one of its parts is: here a post's cylinder
/// inside a ball around a place, and not once the ball is moved off; and an object blocks a line
/// of sight unless the line passes through its physics type (or lists it as passed through).
///
/// Behaviour: collision.detect.an-object-answers-for-its-parts-and-its-type
#[test]
fn an_object_answers_for_its_parts_and_its_type() {
    use auberean_physics::detect::{DetectorSettings, Shape, Volume};
    use auberean_physics::ethereal::SightLine;
    use auberean_physics::geom::Sphere;
    let land = crate::contact::objects::land();
    let mut world = World::new();
    let post = world.insert(
        &land,
        0x77,
        &WorldObject {
            shape: post(),
            position: at(70.0, 75.0),
            is_static: true,
            passable: false,
        },
    );
    let mut s = DetectorSettings {
        volume: Volume {
            position: at(70.0, 72.0),
            shape: Shape::Ball(Sphere::new(dereth_primitives::Vec3::ZERO, 3.0)),
        },
        ..DetectorSettings::default()
    };
    assert!(
        world.within_detect_volume(post, &s),
        "the cylinder reaches into the ball"
    );
    s.volume.position = at(70.0, 60.0);
    assert!(!world.within_detect_volume(post, &s), "nothing of it near");

    world.set_ethereality(
        post,
        Ethereality {
            physics_type: ITEM,
            movement: 0,
            placement: 0,
        },
    );
    let mut los = SightLine::default();
    assert!(world.is_los_blocking(post, &los));
    los.physics.mask = ITEM;
    assert!(
        !world.is_los_blocking(post, &los),
        "the line passes through items"
    );
}

/// A detection is after an object it does not ignore: one it targets when it has targets, else
/// one its game object says is of a kind it reports. It notes the object once, with what the game
/// object notes, where the object is and its distance from the detection's origin.
///
/// Behaviour: collision.detect.a-detection-notes-an-object-it-is-after-once
#[test]
fn a_detection_notes_an_object_it_is_after_once() {
    use auberean_physics::detect::{DetectorSettings, ReportType, Sighting, Sightings};
    use auberean_physics::world::GameObjects;
    struct Game;
    impl GameObjects for Game {
        fn matches(&self, object: u64, report: &ReportType) -> bool {
            object == 0x77 && report.mask == 2
        }
        fn note(&self, object: u64, s: &mut Sighting) {
            s.entity = u32::try_from(object).unwrap_or(0) + 1;
            s.distance = -1.0;
        }
    }
    let land = crate::contact::objects::land();
    let mut world = World::new();
    let post = world.insert(
        &land,
        0x77,
        &WorldObject {
            shape: post(),
            position: at(70.0, 75.0),
            is_static: true,
            passable: false,
        },
    );
    let mut s = DetectorSettings {
        report: ReportType { mask: 2 },
        ..DetectorSettings::default()
    };
    assert!(world.check_type_match(post, &s, &Game));
    s.report.mask = 1;
    assert!(
        !world.check_type_match(post, &s, &Game),
        "not a kind it reports"
    );
    s.targets.push(0x77);
    assert!(world.check_type_match(post, &s, &Game), "but targeted");
    s.ignored.push(0x77);
    assert!(!world.check_type_match(post, &s, &Game), "ignored first");

    let mut seen = Sightings::new();
    world.add_to_target_list(post, &at(70.0, 72.0), &mut seen, &Game);
    let n = &seen[&0x77];
    assert_eq!(n.entity, 0x78, "the game object's notes");
    assert!(
        (n.distance - 3.0).abs() < 1e-5,
        "its distance, set after: {}",
        n.distance
    );
    assert_eq!(n.position, at(70.0, 75.0));
    world.add_to_target_list(post, &at(70.0, 60.0), &mut seen, &Game);
    assert!((seen[&0x77].distance - 3.0).abs() < 1e-5, "noted once");
}

/// An object a landblock or a cell places takes its drawing properties as any object made from an
/// entity does: the entities it is made from first, innermost first, then the placed entity's
/// own, the last that says deciding each bit. Made from an entity that casts a volume of shadow,
/// it casts one; its own word against it, it casts none; with nothing said, none.
///
/// Behaviour: draw.volume.a-placed-object-takes-its-drawing-properties-from-its-entities
#[test]
fn a_placed_object_takes_its_drawing_properties_from_its_entities() {
    use std::sync::Arc;

    use auberean_assets::entity::Entity;
    use auberean_assets::property::{Property, PropertyCollection, PropertyGroup, PropertyValue};
    use auberean_physics::world::{entity_graphics_state, graphics, render};
    const RENDER: u32 = 0x4000_0002;
    const INNER: u32 = 0x4700_0001;
    const OUTER: u32 = 0x4700_0002;
    let flag = PropertyDescriptor {
        value_kind: 0x4000_0001,
        group: RENDER,
        data: 0,
        report: 0,
        required: false,
    };
    let list = PropertyList::from_descriptors(vec![
        (render::VOLUME_SHADOW_CAST, flag),
        (render::LANDSCAPE_SHADOW_CAST, flag),
    ]);
    let drawn = |pairs: &[(u32, bool)]| {
        let mut g = PropertyGroup::new(RENDER);
        for &(id, on) in pairs {
            let mut p = Property::named(&list, id).expect("in the list");
            p.value = PropertyValue::Bool(on);
            g.properties.push(p);
        }
        Some(PropertyCollection { groups: vec![g] })
    };
    let inner = Arc::new(Entity {
        object: Some(0x1F00_0010),
        properties: drawn(&[
            (render::VOLUME_SHADOW_CAST, true),
            (render::LANDSCAPE_SHADOW_CAST, true),
        ]),
        ..Entity::default()
    });
    let outer = Arc::new(Entity {
        object: Some(INNER),
        properties: drawn(&[(render::LANDSCAPE_SHADOW_CAST, false)]),
        ..Entity::default()
    });
    let lookup = |id: u32| match id {
        INNER => Some(inner.clone()),
        OUTER => Some(outer.clone()),
        _ => None,
    };
    let placed = |object: u32, own: &[(u32, bool)]| Entity {
        object: Some(object),
        properties: (!own.is_empty()).then(|| drawn(own)).flatten(),
        ..Entity::default()
    };
    // made from the outer entity: the inner one's volume stands, its landscape shadow is cleared
    assert_eq!(
        entity_graphics_state(&placed(OUTER, &[]), &lookup),
        graphics::NEW | graphics::VOLUME_SHADOW
    );
    // its own word last
    assert_eq!(
        entity_graphics_state(
            &placed(OUTER, &[(render::VOLUME_SHADOW_CAST, false)]),
            &lookup
        ),
        graphics::NEW
    );
    assert_eq!(
        entity_graphics_state(&placed(INNER, &[]), &lookup),
        graphics::NEW | graphics::VOLUME_SHADOW | graphics::LANDSCAPE_SHADOW
    );
    // made from a visual description alone, nothing said
    assert_eq!(
        entity_graphics_state(&placed(0x1F00_0010, &[]), &lookup),
        graphics::NEW
    );
}
