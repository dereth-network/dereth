//! The dynamic objects' volumes of shadow from the install's bodies.

use std::collections::HashMap;

use auberean_animation::body::Body as Figure;
use auberean_animation::CachedRecords;
use auberean_assets::property::{PropertyList, PropertyValue};
use auberean_draw::body::BodyDrawer;
use auberean_draw::scene::ObjectBatch;
use auberean_draw::volume::{VolumeMaker, VolumeRoom};
use auberean_physics::world::render::VOLUME_SHADOW_CAST;
use auberean_primitives::{DataId, Vec3};
use auberean_world::math::{normalize, Mat4};
use auberean_world::place::PlaceRecords;

use crate::common::install;

/// The human male's look, and the entity a human male player is made from.
const HUMAN_MALE: DataId = DataId(0x1F00_0023);
const HUMAN_MALE_PLAYER: u32 = 0x4700_00CD;

/// Whether the triangles `indices` over `vertices` close up: each edge, by where its ends stand,
/// run as often one way as the other.
fn closed(vertices: &[[f32; 3]], indices: &[u16]) -> bool {
    #[allow(clippy::cast_possible_truncation)]
    let key = |p: [f32; 3]| p.map(|x| (x * 1000.0).round() as i64);
    let mut count: HashMap<([i64; 3], [i64; 3]), i32> = HashMap::new();
    for t in indices.as_chunks::<3>().0 {
        for k in 0..3 {
            let a = key(vertices[usize::from(t[k])]);
            let b = key(vertices[usize::from(t[(k + 1) % 3])]);
            if a == b {
                continue;
            }
            *count.entry((a, b)).or_default() += 1;
            *count.entry((b, a)).or_default() -= 1;
        }
    }
    count.values().all(|&c| c == 0)
}

/// A player casts a volume of shadow: the entity a human male player is made from says so among
/// its drawing properties, and the human male's posed body casts a closed volume from each of its
/// meshes of more than three triangles (five of its eight: the others are single triangles),
/// within the frame's room, each in its draw's frame.
///
/// Behaviour: draw.volume.a-player-casts-a-closed-volume-of-shadow
#[test]
fn a_player_casts_a_closed_volume_of_shadow() {
    let r = &install().records;
    let list: PropertyList = r.get(0x3400_0000).expect("the property list");
    let lookup = |id: u32| r.entity(DataId(id));
    let player = r
        .entity(DataId(HUMAN_MALE_PLAYER))
        .expect("the player's entity");
    let casts = player
        .total_properties(&lookup)
        .get(&list, VOLUME_SHADOW_CAST)
        .map(|p| p.value.clone());
    assert_eq!(casts, Some(PropertyValue::Bool(true)), "a player casts one");

    let recs = CachedRecords::new(r);
    let figure = Figure::assemble(&recs, HUMAN_MALE).expect("the human male");
    let poses = figure.pose(&recs);
    let place = Mat4::from_parts(
        Vec3::new(10.0, 20.0, 30.0),
        auberean_primitives::Quat::IDENTITY,
        Vec3::new(1.0, 1.0, 1.0),
    );
    let batches = BodyDrawer::default().batches(r, &figure, &poses, &place);
    let meshes = {
        let mut m: Vec<u32> = batches.iter().map(|b| b.mesh).collect();
        m.sort_unstable();
        m.dedup();
        m
    };
    assert!(meshes.len() > 3, "{} meshes", meshes.len());
    let sun = normalize(Vec3::new(0.5, 0.3, -0.8));
    let mut room = VolumeRoom::FRAME;
    let mut volumes = Vec::new();
    VolumeMaker::default().cast(r, &batches, sun, 1.0, 2, &mut room, &mut volumes);
    println!(
        "{} meshes, {} volumes, {} vertices, {} indices",
        meshes.len(),
        volumes.len(),
        VolumeRoom::FRAME.vertices - room.vertices,
        VolumeRoom::FRAME.indices - room.indices
    );
    // the meshes of more than three triangles at the level the volume is made from
    let solid = meshes
        .iter()
        .filter(|&&m| {
            let mesh: auberean_assets::mesh::Mesh = r.get(m).expect("a mesh");
            auberean_draw::volume::volume_level(&mesh, 1.0, 2).is_some_and(|l| {
                mesh.levels[l]
                    .fragments
                    .iter()
                    .map(|f| f.indices.len() / 3)
                    .sum::<usize>()
                    > 3
            })
        })
        .count();
    assert!(solid >= 5, "{solid} meshes with a body");
    assert_eq!(volumes.len(), solid, "one from each");
    for v in &volumes {
        assert!(closed(&v.vertices, &v.indices), "a volume closes up");
        assert_eq!(v.transform, place, "in its draw's frame");
        assert!(v.indices.iter().all(|&i| usize::from(i) < v.vertices.len()));
    }
    assert_eq!(VolumeRoom::FRAME.volumes - room.volumes, volumes.len());
}

/// The objects the world places cast volumes of shadow when their entities say so: of the
/// install's placed entities, 63 a landblock places and 799 its cells place. A frame among the
/// fallen arms of block `0xB945` marks each such object's draws to be drawn after the volumes,
/// finds those within forty meters of the camera with the sun's way for their shadow, nearest
/// first, and their meshes make closed volumes; an object farther than forty meters casts none.
///
/// Behaviour: draw.volume.placed-objects-that-say-so-cast-volumes
#[test]
fn placed_objects_that_say_so_cast_volumes() {
    use auberean_draw::scene::{Pass, SceneBuilder, Spot};
    use auberean_draw::volume::placed_casters;
    use auberean_physics::world::{entity_graphics_state, graphics};
    let r = &install().records;
    let lookup = |id: u32| r.entity(DataId(id));
    let casts = |e: &auberean_assets::entity::Entity| {
        entity_graphics_state(e, &lookup) & graphics::VOLUME_SHADOW != 0
    };
    let (mut outdoors, mut inside) = (0, 0);
    for (x, y) in r.landblock_coordinates() {
        let Some(info) = r.landblock_info(x, y) else {
            continue;
        };
        outdoors += info.entities.entities.iter().filter(|e| casts(e)).count();
        for low in 0x100..0x100 + info.cell_count {
            let Some(c) = u16::try_from(low).ok().and_then(|l| r.cell(x, y, l)) else {
                continue;
            };
            inside += c
                .entities
                .iter()
                .flat_map(|g| g.entities.iter())
                .filter(|e| casts(e))
                .count();
        }
    }
    assert_eq!((outdoors, inside), (63, 799));

    let region = r.region().expect("the region");
    let mut builder = SceneBuilder::new(r, &region);
    let spot = Spot {
        block: (0xB9, 0x45),
        position: Vec3::new(86.0, 88.0, 84.0),
        heading: 0.0,
        pitch: -10.0,
        time_of_day: 0.45,
        day: None,
    };
    let frame = builder.frame(&spot, 60.0);
    let sun = frame.light.light_direction;
    let marked: std::collections::BTreeSet<u32> = frame
        .objects
        .iter()
        .filter_map(|b| b.caster.map(|c| c.object))
        .collect();
    // of the block's 24 that cast, those drawn from here (a few small ones are past their
    // visible distance), and none of the scenery
    assert!((20..=24).contains(&marked.len()), "{marked:?}");
    assert!(frame
        .objects
        .iter()
        .filter(|b| b.caster.is_some() && b.layer.depth_write && !b.layer.blends())
        .all(|b| b.pass() == Pass::Casters));
    let casters = placed_casters(&frame, sun, auberean_draw::volume::VOLUME_REACH);
    assert!(
        casters.len() >= 8 && casters.len() < marked.len(),
        "{} within reach",
        casters.len()
    );
    assert!(casters
        .iter()
        .all(|c| c.distance <= 40.0 && c.toward == sun));
    let mut maker = VolumeMaker::default();
    let mut room = VolumeRoom::FRAME;
    let mut volumes = Vec::new();
    for c in &casters {
        maker.cast(
            r,
            &frame.objects[c.draws.clone()],
            c.toward,
            c.detail,
            2,
            &mut room,
            &mut volumes,
        );
    }
    println!(
        "{} placed casters, {} within reach, {} volumes",
        marked.len(),
        casters.len(),
        volumes.len()
    );
    assert!(volumes.len() >= casters.len(), "{} volumes", volumes.len());
    assert!(volumes.iter().all(|v| closed(&v.vertices, &v.indices)));
}

/// A placed object that casts a volume of shadow throws it the way the lights it keeps throw it,
/// the lights objects carry among them: outdoors, among the fallen arms of block `0xB945`, a torch
/// held beside one turns its shadow from the sun's way to away from the torch, every draw of the
/// object alike, and once the torch is gone the sun's way is back; inside a dungeon of block
/// `0x07F4`, a torch held where an object's shadow falls turns the shadow back away from it, and
/// once it is gone the shadow falls the way the object's own lights throw it again.
///
/// Behaviour: draw.volume.a-placed-objects-shadow-turns-away-from-a-carried-light
#[test]
fn a_placed_objects_shadow_turns_away_from_a_carried_light() {
    use auberean_draw::scene::{KeptBy, ObjectBatch, SceneBuilder, Spot};
    use auberean_physics::world::{entity_graphics_state, graphics};
    use auberean_world::color::Color;
    use auberean_world::inside::{Light, Outside};
    use auberean_world::math::{add, dot, scale, sub};
    let r = &install().records;
    let region = r.region().expect("the region");
    let mut builder = SceneBuilder::new(r, &region);
    let outside = Outside {
        sun: Color::new(0.5, 0.5, 0.5, 1.0),
        brightness: 1.0,
        direction: normalize(Vec3::new(-0.5, 0.3, -0.8)),
        ambient: Color::new(0.2, 0.2, 0.2, 1.0),
    };
    let torch = |cell: u32, at: Vec3| {
        let mut t = Light::of_properties(cell, Vec3::ZERO, &Default::default()).0;
        t.origin = at;
        t.color = Color::new(1.0, 0.5, 0.0, 1.0);
        t.intensity = 1.0;
        t.range = 8.0;
        t.most_range = 8.0;
        t.most_intensity = Some(1.0);
        t
    };
    // The first placed caster among the draws: where it keeps its lights, and the way each of its
    // draws carries.
    let caster = |batches: &[ObjectBatch]| -> (KeptBy, Vec<Option<Vec3>>) {
        let first = batches
            .iter()
            .position(|b| b.caster.is_some() && b.kept_by.is_some())
            .expect("a placed caster");
        let object = batches[first].caster.map(|c| c.object);
        let ways = batches[first..]
            .iter()
            .take_while(|b| b.caster.map(|c| c.object) == object)
            .map(|b| b.caster.and_then(|c| c.toward))
            .collect();
        (batches[first].kept_by.expect("its lights' place"), ways)
    };

    // Outdoors: the sun's way, then away from a torch two meters east of it and one up.
    let spot = Spot {
        block: (0xB9, 0x45),
        position: Vec3::new(86.0, 88.0, 84.0),
        heading: 0.0,
        pitch: -10.0,
        time_of_day: 0.45,
        day: None,
    };
    let mut frame = builder.frame(&spot, 60.0);
    let (kb, ways) = caster(&frame.objects);
    assert!(ways.iter().all(Option::is_none), "the sun's way");
    let east = torch(kb.cell, add(kb.at, Vec3::new(2.0, 0.0, 1.0)));
    builder.light_carried(&spot, &mut frame.objects, &[east], &outside);
    let (_, ways) = caster(&frame.objects);
    let way = ways[0].expect("the torch's way");
    assert!(ways.iter().all(|w| *w == Some(way)), "{ways:?}");
    assert!(
        way.x < -0.5 && way.z < 0.0,
        "west and down, away from the torch: {way:?}"
    );
    builder.light_carried(&spot, &mut frame.objects, &[], &outside);
    let (_, ways) = caster(&frame.objects);
    assert!(ways.iter().all(Option::is_none), "the sun's way again");

    // Inside: the first cell of the dungeon's block with an object that casts one.
    let block = (0x07, 0xF4);
    let lookup = |id: u32| r.entity(DataId(id));
    let info = r.landblock_info(block.0, block.1).expect("the block");
    let (low, cell) = (0x100..0x100 + info.cell_count)
        .filter_map(|low| u16::try_from(low).ok())
        .filter_map(|low| r.cell(block.0, block.1, low).map(|c| (low, c)))
        .find(|(_, c)| {
            c.entities
                .iter()
                .flat_map(|g| g.entities.iter())
                .any(|e| entity_graphics_state(e, &lookup) & graphics::VOLUME_SHADOW != 0)
        })
        .expect("a cell with an object that casts one");
    let spot = Spot {
        block,
        position: add(cell.frame.origin, Vec3::new(0.0, 0.0, 1.0)),
        heading: 0.0,
        pitch: 0.0,
        time_of_day: 0.45,
        day: None,
    };
    let mut batches = builder.interior(&spot, block, low, &outside);
    let (kb, ways) = caster(&batches);
    let own = ways[0].expect("its own lights' way");
    let level = Vec3::new(own.x, own.y, 0.0);
    let ahead = if dot(level, level) > 1e-4 {
        normalize(level)
    } else {
        Vec3::new(1.0, 0.0, 0.0)
    };
    // the torch where the shadow falls, a meter and a half off and one up
    let held = torch(
        kb.cell,
        add(add(kb.at, scale(ahead, 1.5)), Vec3::new(0.0, 0.0, 1.0)),
    );
    let away = normalize(sub(kb.at, held.origin));
    builder.light_carried(&spot, &mut batches, std::slice::from_ref(&held), &outside);
    let (_, ways) = caster(&batches);
    let turned = ways[0].expect("the turned way");
    assert!(ways.iter().all(|w| *w == Some(turned)), "{ways:?}");
    assert!(
        dot(turned, away) > dot(own, away) + 0.3,
        "turned away from the torch: {own:?} to {turned:?}"
    );
    builder.light_carried(&spot, &mut batches, &[], &outside);
    let (_, ways) = caster(&batches);
    assert!(
        ways.iter().all(|w| *w == Some(own)),
        "its own lights' way again"
    );
}

/// Each step of a frame's objects draws them farthest from the camera first, each object's draws
/// together in their order, and the frills come after the water: at the camp of block `0xC156` the
/// opaque objects' draws come by where their objects stand, never a nearer one before a farther,
/// the parts of each placed object of several parts together and in their order (draws given a
/// step of their own, which are no objects of the world, keep theirs); every frill is
/// drawn in the frills' step and nothing else is; and the layers of the camp's objects that mark
/// nothing are drawn with their objects, not after the water.
///
/// Behaviour: draw.frame.each-step-draws-its-objects-farthest-first
#[test]
fn each_step_draws_its_objects_farthest_first_and_the_frills_after_the_water() {
    use auberean_draw::scene::{in_order, Pass, SceneBuilder, Spot};
    let r = &install().records;
    let region = r.region().expect("the region");
    let mut builder = SceneBuilder::new(r, &region);
    let spot = Spot {
        block: (0xC1, 0x56),
        position: Vec3::new(122.842, 89.758, 79.968),
        heading: 149.65,
        pitch: -10.0,
        time_of_day: 0.45,
        day: None,
    };
    let frame = builder.frame(&spot, 60.0);
    let eye = frame.camera.position;
    let of_step = |p: Pass| -> Vec<&auberean_draw::scene::ObjectBatch> {
        frame.objects.iter().filter(|b| b.pass() == p).collect()
    };
    let opaque = of_step(Pass::Opaque);
    let ordered = in_order(opaque.clone(), Pass::Opaque, eye);
    assert_eq!(ordered.len(), opaque.len());
    let d: Vec<f32> = ordered.iter().map(|b| b.distance_from(eye)).collect();
    assert!(d.windows(2).all(|w| w[0] >= w[1]), "farthest first");
    // The placed object with the most draws: its draws together, in the order they were made.
    let anchored: Vec<_> = opaque.iter().filter_map(|b| b.anchor).collect();
    let several = anchored
        .iter()
        .max_by_key(|a| anchored.iter().filter(|b| *b == *a).count())
        .copied()
        .expect("an anchored object");
    assert!(anchored.iter().filter(|a| **a == several).count() > 1);
    let at: Vec<usize> = (0..ordered.len())
        .filter(|&k| ordered[k].anchor == Some(several))
        .collect();
    assert!(at.windows(2).all(|w| w[1] == w[0] + 1), "together");
    let made: Vec<*const _> = opaque
        .iter()
        .filter(|b| b.anchor == Some(several))
        .map(|b| std::ptr::from_ref(*b))
        .collect();
    let drawn: Vec<*const _> = at.iter().map(|&k| std::ptr::from_ref(ordered[k])).collect();
    assert_eq!(drawn, made, "in their order");
    // Draws given a step of their own (portal space's) keep their order, nearest first or not.
    let mut given: Vec<_> = [ordered[ordered.len() - 1], ordered[0]]
        .into_iter()
        .cloned()
        .collect();
    for b in &mut given {
        b.step = Some(Pass::Opaque);
    }
    let kept = in_order(given.iter().collect(), Pass::Opaque, eye);
    assert!(std::ptr::eq(kept[0], &given[0]) && std::ptr::eq(kept[1], &given[1]));
    // The frills, and nothing else, after the water.
    let frills = of_step(Pass::Frills);
    assert!(frills.len() > 10, "{} frill draws", frills.len());
    assert!(frame
        .objects
        .iter()
        .all(|b| (b.shade.is_some()) == (b.pass() == Pass::Frills)));
    // The objects' layers that mark nothing, drawn with their objects.
    let unmarked = frame
        .objects
        .iter()
        .filter(|b| b.shade.is_none() && !b.layer.depth_write && !b.seen_through)
        .collect::<Vec<_>>();
    println!(
        "{} opaque draws, {} frill draws, {} unmarked object draws",
        opaque.len(),
        frills.len(),
        unmarked.len()
    );
    assert!(!unmarked.is_empty());
    assert!(unmarked
        .iter()
        .all(|b| matches!(b.pass(), Pass::Opaque | Pass::Casters)));
}

/// A draw's step goes by its object, never by its layer: the human male's layers all blend by
/// their pictures' alpha, and every one of them, those that mark how near they are and the one
/// that marks nothing alike, is drawn with the opaque objects (before the volumes darken the
/// frame; with the casters after them when the body casts one); a body its drawing properties
/// make seen through is drawn with those seen through whole, and an interior cell's mesh before
/// the decals.
///
/// Behaviour: draw.volume.a-draws-step-goes-by-its-object
#[test]
fn a_draws_step_goes_by_its_object() {
    use auberean_draw::scene::{Cull, Pass};
    let r = &install().records;
    let recs = CachedRecords::new(r);
    let figure = Figure::assemble(&recs, HUMAN_MALE).expect("the human male");
    let poses = figure.pose(&recs);
    let batches = BodyDrawer::default().batches(r, &figure, &poses, &Mat4::IDENTITY);
    assert!(
        batches.iter().all(|b| b.layer.blends()),
        "every layer blends"
    );
    let (marks, unmarked): (Vec<_>, Vec<_>) = batches.iter().partition(|b| b.layer.depth_write);
    assert!(marks.len() > 3 && !unmarked.is_empty());
    assert!(batches.iter().all(|b| b.pass() == Pass::Opaque));
    for b in [marks[0], unmarked[0]] {
        let step = |f: &dyn Fn(&mut auberean_draw::scene::ObjectBatch)| {
            let mut b = b.clone();
            f(&mut b);
            b.pass()
        };
        assert_eq!(step(&|b| b.casts_volume = true), Pass::Casters);
        assert_eq!(step(&|b| b.seen_through = true), Pass::Translucent);
        assert_eq!(step(&|b| b.cull = Cull::Cell(0x100)), Pass::Cells);
    }
}

/// A body degrades with its distance from the camera, as a placed object does: nearby every part
/// draws at its fullest, whole; at the edge of its setup's visible distance it fades; past it, it
/// is gone; drawn in by a scale it is gone sooner. In a scene of the interface (no camera) it never
/// degrades.
///
/// Behaviour: draw.mesh.a-body-degrades-with-its-distance-from-the-camera
#[test]
fn a_body_degrades_with_its_distance_from_the_camera() {
    let r = &install().records;
    let recs = CachedRecords::new(r);
    let figure = Figure::assemble(&recs, HUMAN_MALE).expect("the human male");
    let poses = figure.pose(&recs);
    let visible = figure
        .parts
        .iter()
        .map(|p| p.setup.detail[2].visible.max(20.0))
        .fold(0.0_f32, f32::max);
    assert!(visible > 40.0, "{visible}");
    let at = |d: f32, scale: f32| {
        let mut drawer = BodyDrawer::default();
        drawer.eye = Some(Vec3::ZERO);
        drawer.still_scale = scale;
        drawer.next_body(false);
        let place = Mat4::from_parts(
            Vec3::new(0.0, d, 0.0),
            auberean_primitives::Quat::IDENTITY,
            Vec3::new(1.0, 1.0, 1.0),
        );
        drawer.batches(r, &figure, &poses, &place)
    };
    let near = at(1.0, 1.0);
    assert!(!near.is_empty());
    assert!(near.iter().all(|b| b.tint.a == 1.0 && !b.far));
    let whole: usize = BodyDrawer::default()
        .batches(r, &figure, &poses, &Mat4::IDENTITY)
        .iter()
        .map(|b| b.indices.len())
        .sum();
    assert_eq!(near.iter().map(|b| b.indices.len()).sum::<usize>(), whole);
    let edge = at(visible * 0.97, 1.0);
    assert!(
        !edge.is_empty() && edge.iter().all(|b| b.tint.a < 1.0 && b.far),
        "fading at the edge"
    );
    assert!(at(visible + 1.0, 1.0).is_empty(), "gone past it");
    assert!(at(visible * 0.6, 0.4).is_empty(), "sooner, drawn in");
    let far_off = BodyDrawer::default().batches(
        r,
        &figure,
        &poses,
        &Mat4::from_parts(
            Vec3::new(0.0, visible * 10.0, 0.0),
            auberean_primitives::Quat::IDENTITY,
            Vec3::new(1.0, 1.0, 1.0),
        ),
    );
    assert!(!far_off.is_empty(), "never with no camera");
}

/// With a water's reflection in the frame a body is drawn twice where the reflection's choice of
/// level differs from the frame's (it takes the distances at 40%, so its detail falls sooner): one
/// set for the frame alone, one for the reflection alone. Close to the camera both choose the same,
/// and past its reach in the reflection only the frame shows it.
///
/// Behaviour: draw.mesh.the-waters-reflection-sees-a-body-at-its-own-level-of-detail
#[test]
fn the_waters_reflection_sees_a_body_at_its_own_level_of_detail() {
    let r = &install().records;
    let recs = CachedRecords::new(r);
    let figure = Figure::assemble(&recs, HUMAN_MALE).expect("the human male");
    let poses = figure.pose(&recs);
    let at = |d: f32, reflecting: bool| {
        let mut drawer = BodyDrawer::default();
        drawer.eye = Some(Vec3::ZERO);
        drawer.reflecting = reflecting;
        drawer.next_body(false);
        let place = Mat4::from_parts(
            Vec3::new(0.0, d, 0.0),
            auberean_primitives::Quat::IDENTITY,
            Vec3::new(1.0, 1.0, 1.0),
        );
        drawer.batches(r, &figure, &poses, &place)
    };
    let triangles = |b: &[ObjectBatch], pick: &dyn Fn(&ObjectBatch) -> bool| -> usize {
        b.iter().filter(|x| pick(x)).map(|x| x.indices.len()).sum()
    };
    let shown = |x: &ObjectBatch| x.shown();
    let mirrored = |x: &ObjectBatch| x.reflected(Vec3::ZERO);
    let visible = figure
        .parts
        .iter()
        .map(|p| p.setup.detail[2].visible.max(20.0))
        .fold(0.0_f32, f32::max);
    // Near: both the same.
    let near = at(1.0, true);
    assert_eq!(triangles(&near, &shown), triangles(&near, &mirrored));
    // Not in a frame without a reflection: one set.
    let plain = at(visible * 0.3, false);
    let both = at(visible * 0.3, true);
    assert_eq!(triangles(&plain, &shown), triangles(&both, &shown));
    let (frame, there) = (triangles(&both, &shown), triangles(&both, &mirrored));
    println!("at 30% of its reach: {frame} indices in the frame, {there} in the reflection");
    assert!(there < frame, "less detail in the reflection");
    assert!(there > 0, "still in its reach");
    // Past its reach there.
    let far = at(visible * 0.5, true);
    assert!(triangles(&far, &shown) > 0);
    assert_eq!(
        triangles(&far, &mirrored),
        0,
        "out of the reflection's reach"
    );
}
