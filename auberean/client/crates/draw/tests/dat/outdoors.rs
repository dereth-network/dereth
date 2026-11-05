//! What the rooms about the camera look out on, drawn under the light outdoors: the human training
//! hall's rooms (hall copy 0x0298 of the September 2013 cell file), canyons open to the sky above;
//! the cave of block 0x6967, whose first room has a doorway out to the land.

use auberean_primitives::Vec3;

use auberean_draw::scene::{Outlook, SceneBuilder, Spot};

use crate::common::install;

fn spot(block: (i32, i32), position: Vec3) -> Spot {
    Spot {
        block,
        position,
        heading: 0.0,
        pitch: 30.0,
        time_of_day: 0.5,
        day: None,
    }
}

/// From a room open to the sky the frame draws the sky, under the light outdoors (its own light
/// stays the day's, for the interior's to replace), and no land; the rooms and what stands in them
/// stay under the frame's own light.
///
/// Behaviour: draw.inside.a-room-open-to-the-sky-shows-the-sky
#[test]
fn a_room_open_to_the_sky_shows_the_sky_and_no_land() {
    let i = install();
    let region = i.records.region().expect("the region");
    let mut builder = SceneBuilder::new(&i.records, &region);
    assert_eq!(
        builder.outlook((0x02, 0x98), 0x0100),
        Outlook {
            sky: true,
            land: false
        }
    );
    let (f, _) = builder.inside_frame(
        &spot((0x02, 0x98), Vec3::new(5.0, 262.0, -43.0)),
        60.0,
        (0x02, 0x98),
        0x0100,
    );
    assert!(!f.sky_before.is_empty(), "the sky");
    assert_eq!(f.outside, Some(f.light), "the sky under the light outdoors");
    assert!(f.terrain.is_empty() && f.water.is_empty(), "no land");
    assert!(f.objects.len() > 50, "the rooms: {}", f.objects.len());
    assert!(
        f.objects.iter().all(|b| !b.outside),
        "the rooms under their own light"
    );
}

/// From the cave's first room, which has a doorway out, the frame draws the land outside through
/// it as a frame outdoors would (its ground, its objects, the sky) under the light outdoors, and the
/// cave's rooms under the frame's own; the room is drawn once, as a room.
///
/// Behaviour: draw.inside.a-doorway-out-shows-the-land-under-the-light-outdoors
#[test]
fn a_doorway_out_shows_the_land_under_the_light_outdoors() {
    let i = install();
    let region = i.records.region().expect("the region");
    let mut builder = SceneBuilder::new(&i.records, &region);
    let block = (0x69, 0x67);
    let outlook = builder.outlook(block, 0x0100);
    assert!(outlook.land, "{outlook:?}");
    let at = Vec3::new(109.75, 112.97, 100.8);
    let outdoors = builder.frame(&spot(block, at), 60.0);
    let (f, _) = builder.inside_frame(&spot(block, at), 60.0, block, 0x0100);
    assert!(f.outside.is_some(), "the light outdoors");
    assert!(f.reflection.is_none(), "no reflection from inside");
    assert!(!f.sky_before.is_empty(), "the sky");
    assert_eq!(
        f.terrain.len(),
        outdoors.terrain.len(),
        "the land as outdoors"
    );
    let out = f.objects.iter().filter(|b| b.outside).count();
    let rooms = f.objects.len() - out;
    println!(
        "{rooms} batches inside, {out} outdoors ({} in the frame outdoors)",
        outdoors.objects.len()
    );
    assert!(rooms > 0 && out > 0);
    assert!(out <= outdoors.objects.len());
}

/// A cave's own meshes take its sky light alone of the lights it keeps, from inside and from the
/// land: its one sky light, not the point lights their vertices already carry; what stands in its
/// rooms may take those too.
///
/// Behaviour: draw.inside.a-cells-own-mesh-keeps-the-sky-lights-alone
#[test]
fn a_cells_own_mesh_keeps_the_sky_lights_alone() {
    let i = install();
    let region = i.records.region().expect("the region");
    let mut builder = SceneBuilder::new(&i.records, &region);
    let block = (0x69, 0x67);
    let lights = builder.block_lights(block).expect("the cave's lights");
    let sky: Vec<Vec3> = lights
        .lights
        .iter()
        .filter(|l| l.kind == auberean_world::inside::LightKind::Sky)
        .map(|l| l.origin)
        .collect();
    assert_eq!(sky.len(), 1, "the cave has one");
    let at = Vec3::new(109.75, 112.97, 100.8);
    let (inside, _) = builder.inside_frame(&spot(block, at), 60.0, block, 0x0100);
    let outdoors = builder.frame(&spot(block, Vec3::new(109.75, 92.0, 101.0)), 60.0);
    for frame in [&inside, &outdoors] {
        let meshes: Vec<_> = frame
            .objects
            .iter()
            .filter(|b| {
                b.kept_by
                    .is_some_and(|k| k.key.1 == 0 && k.cell & 0xFFFF >= 0x100)
            })
            .collect();
        assert!(!meshes.is_empty());
        for b in meshes {
            assert!(b.points.count <= 1, "{}", b.points.count);
            for l in &b.points.lights[..b.points.count] {
                assert!((l.origin.z - sky[0].z).abs() < 0.01, "{:?}", l.origin);
            }
        }
    }
}

/// A room that sees neither the sky nor a doorway out draws nothing outdoors: no sky, no land, no
/// light outdoors.
///
/// Behaviour: draw.inside.a-closed-room-shows-nothing-outdoors
#[test]
fn a_closed_room_shows_nothing_outdoors() {
    let i = install();
    let region = i.records.region().expect("the region");
    let mut builder = SceneBuilder::new(&i.records, &region);
    // the first block of the map whose rooms are closed
    let (block, low) = (0..256)
        .flat_map(|x| (0..256).map(move |y| (x, y)))
        .find_map(|b| {
            let info = i.records.landblock_info(b.0, b.1)?;
            (info.cell_count > 10).then_some(())?;
            let o = builder.outlook(b, 0x0100);
            (!o.sky && !o.land).then_some((b, 0x0100))
        })
        .expect("a closed room");
    let c = i.records.cell(block.0, block.1, low).expect("the room");
    let (f, _) = builder.inside_frame(&spot(block, c.frame.origin), 60.0, block, low);
    println!("closed room {:02X}{:02X}{low:04X}", block.0, block.1);
    assert_eq!(f.outside, None);
    assert!(f.sky_before.is_empty() && f.sky_after.is_empty() && f.flares.is_empty());
    assert!(f.terrain.is_empty() && f.water.is_empty() && f.shadow.is_none());
    assert!(!f.objects.is_empty() && f.objects.iter().all(|b| !b.outside));
}

/// Outdoors the frills take the lights objects carry too, as the other draws standing there do:
/// by the crystals of `CB94`, a torch carried among the grass lights the frills it reaches beside
/// the sun, and those it does not reach keep the sun alone.
///
/// Behaviour: draw.effects.frills-take-the-carried-lights-that-reach-them
#[test]
fn frills_take_the_carried_lights_that_reach_them() {
    use auberean_assets::property::PropertyCollection;
    use auberean_world::inside::{Light, Outside};
    let r = &install().records;
    let region = r.region().expect("the region");
    let mut builder = SceneBuilder::new(r, &region);
    let at = Vec3::new(127.0, 148.0, 81.0);
    let s = Spot {
        block: (0xCB, 0x94),
        position: at,
        heading: 0.0,
        pitch: 0.0,
        time_of_day: 0.05,
        day: None,
    };
    let mut frame = builder.frame(&s, 60.0);
    let frills: Vec<usize> = (0..frame.objects.len())
        .filter(|&k| frame.objects[k].shade.is_some())
        .collect();
    assert!(frills.len() > 10, "{} frill draws", frills.len());
    assert!(frills.iter().all(|&k| frame.objects[k].kept_by.is_some()));
    let near = frills
        .iter()
        .map(|&k| frame.objects[k].kept_by.expect("kept").at)
        .find(|p| (p.x - at.x).abs() < 20.0 && (p.y - at.y).abs() < 20.0)
        .expect("a frill near the crystals");
    let (mut torch, _, _) = Light::of_properties(0, Vec3::ZERO, &PropertyCollection::default());
    torch.cell = 0xCB94_0038;
    torch.origin = Vec3::new(near.x, near.y, near.z + 1.0);
    torch.intensity = 1.0;
    torch.most_intensity = Some(1.0);
    torch.range = 8.0;
    torch.most_range = 8.0;
    let outside = Outside::of(&frame.light);
    builder.light_carried(&s, &mut frame.objects, &[torch], &outside);
    let lit = frills
        .iter()
        .filter(|&&k| frame.objects[k].points.count > 0)
        .count();
    println!("{} frill draws, {lit} take the torch", frills.len());
    assert!(lit > 0 && lit < frills.len(), "{lit}");
}

/// The rooms of a cave seen from the land outside its mouth (block `0x6967`, the frame outdoors at
/// noon) are lit as rooms, not by the sun: each room named in the block's list of rooms seen from
/// outside is drawn once (the list names some twice), its mesh by the light its vertices carry (the
/// lights its record lists, and the sun's share through its openings) and the lights it keeps where
/// it stands; a torch carried inside the first room joins them (and still no sun), while the same
/// torch carried on the land outside does not.
///
/// Behaviour: draw.inside.rooms-seen-from-outside-are-lit-as-rooms
#[test]
fn rooms_seen_from_outside_are_lit_as_rooms() {
    use auberean_assets::property::PropertyCollection;
    use auberean_draw::scene::LIT_IN_VERTICES;
    use auberean_world::inside::{Light, Outside};
    let r = &install().records;
    let region = r.region().expect("the region");
    let block = (0x69, 0x67);
    let info = r.landblock_info(block.0, block.1).expect("the block");
    let mut rooms: Vec<u32> = info
        .outside_visible
        .iter()
        .map(|l| 0x6967_0000 | (l & 0xFFFF))
        .collect();
    let listed = rooms.len();
    rooms.sort_unstable();
    rooms.dedup();
    assert!(rooms.len() < listed, "the list names a room twice");
    let mut builder = SceneBuilder::new(r, &region);
    let s = spot(block, Vec3::new(109.75, 92.0, 101.0));
    let mut frame = builder.frame(&s, 60.0);
    assert_eq!(frame.counts.cells, rooms.len(), "each room once");
    let mouth = 0x6967_0100;
    let walls: Vec<usize> = (0..frame.objects.len())
        .filter(|&k| {
            frame.objects[k]
                .kept_by
                .is_some_and(|kb| kb.key == (mouth, 0))
        })
        .collect();
    assert!(!walls.is_empty(), "the first room's mesh, kept by the room");
    for &k in &walls {
        let b = &frame.objects[k];
        assert_ne!(b.layer.switches & LIT_IN_VERTICES, 0, "lit in its vertices");
        assert!(b.points.inside, "no sun");
        assert!(b.kept_by.expect("kept").own);
    }
    let in_rooms = frame
        .objects
        .iter()
        .filter(|b| b.kept_by.is_some_and(|kb| kb.cell & 0xFFFF >= 0x100))
        .count();
    let rooms_drawn: std::collections::BTreeSet<u32> = frame
        .objects
        .iter()
        .filter_map(|b| b.kept_by.filter(|kb| kb.cell & 0xFFFF >= 0x100))
        .map(|kb| kb.cell)
        .collect();
    assert_eq!(rooms_drawn.len(), rooms.len(), "every room lit as a room");
    let at = frame.objects[walls[0]].kept_by.expect("kept").at;
    let torch_in = |cell: u32| {
        let (mut t, _, _) = Light::of_properties(0, Vec3::ZERO, &PropertyCollection::default());
        t.cell = cell;
        t.origin = Vec3::new(at.x, at.y, at.z + 1.0);
        t.intensity = 1.0;
        t.most_intensity = Some(1.0);
        t.range = 8.0;
        t.most_range = 8.0;
        t
    };
    let outside = Outside::of(&frame.light);
    builder.light_carried(&s, &mut frame.objects, &[torch_in(0x6967_0029)], &outside);
    let took = |f: &auberean_draw::scene::Frame| {
        walls
            .iter()
            .filter(|&&k| f.objects[k].kept_by.is_some_and(|kb| kb.took_carried))
            .count()
    };
    builder.light_carried(&s, &mut frame.objects, &[torch_in(0x6967_0029)], &outside);
    assert_eq!(took(&frame), 0, "a torch on the land reaches no room");
    builder.light_carried(&s, &mut frame.objects, &[torch_in(mouth)], &outside);
    assert!(took(&frame) > 0, "a torch in the room reaches it");
    assert!(walls.iter().all(|&k| frame.objects[k].points.inside));
    println!("{in_rooms} draws in {} rooms", rooms.len());
}
