//! Placed objects: vertex layouts, detail levels and mask turns, how they degrade with distance,
//! the walk through made-up cells, the lights effects place on objects and the dynamic objects'
//! volumes of shadow, and work shared among threads.

mod cull;
mod degrade;
mod layout;
mod lights;
mod volume;
mod workers;

/// How far away a body is drawn doesn't remember its last draw: one left undrawn at a low scale is
/// drawn again when the scale comes back, with the camera where it was.
///
/// Behaviour: draw.mesh.a-bodys-reach-is-worked-out-each-frame
#[test]
fn a_bodys_reach_is_worked_out_each_frame() {
    use auberean_assets::setup::DetailDistances;
    use auberean_draw::mesh::degrade;
    let d = DetailDistances {
        visible: 65.0,
        ..DetailDistances::default()
    };
    let at = 30.41;
    assert!(degrade(&d, at, 1.0, 2).is_some(), "within its reach");
    assert!(degrade(&d, at, 0.1, 2).is_none(), "past the floor of 20 m");
    assert!(degrade(&d, at, 1.0, 2).is_some(), "drawn again");
}
