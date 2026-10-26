//! A dais in the Holtburg Redoubt, a crack apart from the steps up to it: the steps reach half
//! its height, and between their tops and the dais's lies a gap narrower than a sampling column
//! or two, where only the floor below is sampled. A body far wider than the gap steps across it.

use dereth_navigation::{PhysicsWalker, WalkerConfig};
use dereth_primitives::{CellId, Frame, Position, Quat, Vec3};

use crate::common::*;

fn at(cell: u32, x: f32, y: f32, z: f32) -> Position {
    Position::new(CellId(cell), Frame::new(Vec3::new(x, y, z), Quat::IDENTITY))
}

/// Behaviour: plugins.nav.a-body-steps-across-a-narrow-gap-between-floors-a-step-apart
#[test]
fn the_holtburg_redoubts_dais_is_walked_onto_from_its_floor_and_off_again() {
    let mut d = dungeon(0x0163);
    // The dais's top, 0.92 m above the room's floor, and the floor beside it.
    let dais = at(0x0163_0137, 47.0, -25.75, -11.08);
    let floor = at(0x0163_0136, 48.75, -23.0, -12.0);
    for (from, to, way) in [(&floor, &dais, "up"), (&dais, &floor, "down")] {
        let mut walker = PhysicsWalker::new(
            &mut d.world,
            std::sync::Arc::clone(&d.player),
            WalkerConfig::default(),
        );
        // One plan, walked as planned by the body through the physics world.
        let (path, report) = d.nav.plan_checked(from, to, &mut walker, 1);
        let path = path.unwrap_or_else(|e| panic!("{way}: no route: {e} ({report:?})"));
        assert!(report.walked, "{way}: the body did not walk it: {report:?}");
        assert!(
            path.length() < 10.0,
            "{way}: by the steps beside it, not round the room: {} m",
            path.length()
        );
    }
}
