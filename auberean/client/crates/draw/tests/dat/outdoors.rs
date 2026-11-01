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
