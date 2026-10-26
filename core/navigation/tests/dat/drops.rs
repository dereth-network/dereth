//! A dungeon whose lower floors no walk reaches, only a drop off a ledge: the Black Dominion,
//! whose portal lands a character on a floor its stairs and ramps join to one 48 m down, below
//! which the way goes on only by dropping off a ledge.

use dereth_navigation::NoPath;
use dereth_primitives::{CellId, Frame, Position, Quat, Vec3};

use crate::common::*;
use crate::drive::jump_off;

/// Behaviour: plugins.nav.a-way-only-a-drop-leads-on-is-no-route-and-names-the-drop
#[test]
fn a_route_down_the_black_dominion_past_its_ledge_is_none_and_names_the_drop_a_jump_takes() {
    let mut d = dungeon(0x010A);
    let portal_drop = Position::new(
        CellId(0x010A_0223),
        Frame::new(Vec3::new(0.0, -70.0, 0.0), Quat::IDENTITY),
    );
    let (from, walked, low) = {
        let region = d.nav.region(portal_drop.cell).expect("the dungeon");
        let s = region
            .nearest_place(&portal_drop, 12.0)
            .expect("a place at the drop");
        let l = lowest(region).expect("a lowest place");
        (
            region.position(s),
            region.position(bottom(region, s)),
            region.position(l),
        )
    };
    assert!(
        walked.frame.origin.z > low.frame.origin.z + 6.0,
        "premise: the walk ends above the bottom ({} against {})",
        walked.frame.origin.z,
        low.frame.origin.z
    );
    match d.nav.plan(&from, &low) {
        Err(NoPath::OnlyByDrop {
            stand,
            land,
            height,
        }) => {
            assert!((5.0..7.0).contains(&height), "{height}");
            assert!(
                (stand.frame.origin.z - land.frame.origin.z - height).abs() < 0.01,
                "{stand:?} {land:?}"
            );
            // Checked by the body's own movement: jumping off where it stands, it comes down
            // on the floor below.
            let (across, rise) =
                jump_off(&mut d.world, &d.player, &stand, &land).expect("it stands at the ledge");
            assert!(rise.abs() < 1.0, "down to the floor below: {rise} m off it");
            assert!(
                across < 6.0,
                "near where it was said to land: {across} m off"
            );
        }
        other => panic!("only a drop leads on, and the planner names it: {other:?}"),
    }
}
