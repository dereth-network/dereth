//! Putting the player's body where the server says: beside the start hall's fir it stands where
//! there is room nearest that place, as the September 2013 client stands it.
//! Fixture: hall copy 0x0298 of the September 2013 cell file, its cell 0x0120 beside the fir; the
//! human male's collision spheres and step heights; the place the server gave and where the
//! client stood the player there (read from the running client).

use auberean_physics::interior::add_block_cells;
use auberean_physics::land::Land;
use auberean_physics::mesh::Trees;
use auberean_physics::world::World;
use dereth_primitives::{CellId, Frame, Position, Quat, Vec3};

use crate::common::need_world;
use crate::interior::body;

/// The server's place for the player in the start hall, beside the fir.
const SERVER_PLACE: (f32, f32, f32) = (101.0, 247.5, -44.9);
/// Where the September 2013 client stood the player for that place.
const CLIENT_PLACE: (f32, f32, f32) = (101.385, 247.278, -44.215);

/// A body the server puts inside the start hall's fir stands where there is room nearest that
/// place: lifted onto the floor and stepped 4/9 m aside (the scatter's rings counted by nine
/// tenths of its radius), where the client stands it, within a centimeter.
///
/// Behaviour: transition.place.a-body-put-beside-the-fir-stands-where-the-client-stands-it
#[test]
fn a_body_put_beside_the_fir_stands_where_the_client_stands_it() {
    let w = need_world!();
    let land = Land::new();
    let mut trees = Trees::default();
    land.add_block(&w.records, &w.region, &mut trees, 0x02, 0x98);
    add_block_cells(&land, &w.records, &mut trees, 0x02, 0x98);
    let mut world = World::new();
    if let Some(list) = w
        .records
        .get::<auberean_assets::property::PropertyList>(0x3400_0000)
    {
        world.set_property_list(list);
    }
    let (x, y, z) = SERVER_PLACE;
    let at = Position::new(
        CellId(0x0298_0120),
        Frame::new(Vec3::new(x, y, z), Quat::IDENTITY),
    );
    let mut b = body(w, at, 0.0);
    b.is_player = true;
    world.insert_body(&land, 0x5000_0001, &mut b);
    let r = b.set_position_simple(&land, &mut world, at, true);
    let o = b.position.frame.origin;
    let (cx, cy, cz) = CLIENT_PLACE;
    let off = ((o.x - cx).powi(2) + (o.y - cy).powi(2)).sqrt();
    assert_eq!(r, 0, "placed: {o:?}");
    assert_eq!(b.position.cell, CellId(0x0298_0120));
    assert!(off < 0.01, "{o:?} is {off} m across from {CLIENT_PLACE:?}");
    assert!((o.z - cz).abs() < 0.01, "{o:?}");
}
