//! A portal's reach, measured from its setup, against the physics' own test of whether a body
//! standing somewhere touches it.

use std::sync::Arc;

use dereth_navigation::reach::{body_of, reach, HUMAN_BODY, MARGIN};
use dereth_physics::SetupGeometry;
use dereth_primitives::num::math::{cosf, sinf};
use dereth_primitives::{DataId, ObjectId, Position};
use dereth_world_data::env_cells::static_geometry;
use dereth_world_data::setup::SetupPartStats;

use crate::common::{ground, outdoors, player_setup, store, Dungeon};

/// The portal setups most portals are built of, and some others: one sphere (the common
/// portal's, and the tunnel's), a mesh with cylinders beside it, and meshes alone.
const SETUPS: [(u32, f32, &str); 7] = [
    (0x0200_01B3, 1.0, "the common portal"),
    (0x0200_01B3, 0.8, "the common portal, smaller"),
    (0x0200_19E4, 1.0, "a tunnel's portal"),
    (0x0200_1144, 1.5, "a larger portal"),
    (0x0200_164F, 1.0, "a mesh with cylinders"),
    (0x0200_1541, 1.0, "a mesh"),
    (0x0200_0EFC, 1.0, "another mesh"),
];

/// Where the body stands: `across` from `centre`, on the bearing `heading`, on the ground.
fn standing(d: &Dungeon, centre: (f32, f32), across: f32, heading: f32) -> Position {
    ground(
        d,
        0xA9B4,
        centre.0 + across * cosf(heading),
        centre.1 + across * sinf(heading),
    )
}

/// Whether a human standing `across` from the portal `portal` (at `centre`) on `heading`
/// touches it, as the physics' placement test answers; `None` where the body cannot stand there.
fn touched(
    d: &mut Dungeon,
    body: dereth_physics::PhysHandle,
    portal: dereth_physics::PhysHandle,
    centre: (f32, f32),
    across: f32,
    heading: f32,
) -> Option<bool> {
    let at = standing(d, centre, across, heading);
    if !d.world.set_position(body, &at) {
        return None;
    }
    // A body passes through an ethereal portal, so the physics counts no collision with it; it
    // records the touch all the same, and the overlap it records is the one asked of a portal
    // made solid for the question.
    let set = |d: &mut Dungeon, ethereal: bool| {
        if let Some(o) = d.world.get_mut(portal) {
            o.state.set_ethereal_bit(ethereal);
        }
    };
    set(d, false);
    let touched = d.world.check_collision(body, portal);
    set(d, true);
    Some(touched)
}

/// Behaviour: plugins.nav.a-walk-ends-short-of-a-portal-it-is-not-taking
#[test]
fn a_body_at_a_portals_reach_and_its_margin_is_not_taken_and_one_just_inside_is() {
    let store = store();
    let player = player_setup(&store);
    assert_eq!(
        body_of(&player, 1.0),
        HUMAN_BODY,
        "the human body the reach is measured for is setup 0x02000001's"
    );
    let mut d = outdoors(0xA9B4, 0);
    // An open stretch of Holtburg's block, nearly flat.
    let centre = (150.0, 40.0);
    let heights: Vec<f32> = (0..8)
        .map(|k| standing(&d, centre, 4.0, k as f32 * 0.785).frame.origin.z)
        .collect();
    let rise = heights.iter().copied().fold(f32::MIN, f32::max)
        - heights.iter().copied().fold(f32::MAX, f32::min);
    assert!(rise < 0.15, "the ground there is nearly flat: {rise} m");

    let body = d
        .world
        .create(ObjectId(0x5000_0101), Arc::clone(&d.player), true);
    for (k, (setup, scale, what)) in SETUPS.iter().enumerate() {
        let geometry: Arc<SetupGeometry> = Arc::new(
            static_geometry(&store, DataId(*setup), &mut SetupPartStats::default())
                .expect("the setup reads"),
        );
        let r = reach(&geometry, *scale, &HUMAN_BODY);
        let portal = d.world.create(
            ObjectId(0x5000_0200 + u32::try_from(k).expect("few")),
            Arc::clone(&geometry),
            // No portal is static: what touches one is an object collision, which takes the body.
            true,
        );
        d.world.get_mut(portal).expect("made").scale = *scale;
        // The body out of the way, so the portal stands where it is put.
        assert!(d.world.set_position(body, &standing(&d, centre, 20.0, 0.0)));
        let put = standing(&d, centre, 0.0, 0.0);
        assert!(
            d.world.set_position(portal, &put),
            "{what}: the portal stands"
        );
        let stood = d.world.get(portal).expect("made").position;
        // Across, where it was put; its height is the physics' to settle.
        let (a, b) = (
            dereth_navigation::global(&stood),
            dereth_navigation::global(&put),
        );
        assert!(
            (a.x - b.x).abs() < 0.01 && (a.y - b.y).abs() < 0.01,
            "{what}: the portal stands where it was put"
        );
        let _ = d.world.set_ethereal(portal, true, false);

        let headings: Vec<f32> = (0..16)
            .map(|h| h as f32 * std::f32::consts::FRAC_PI_8)
            .collect();
        // Beyond the reach and its margin, no heading touches it.
        for &h in &headings {
            let t = touched(&mut d, body, portal, centre, r + MARGIN, h);
            assert_ne!(
                t,
                Some(true),
                "{what}: touched at its reach and margin, heading {h}"
            );
        }
        // Just beyond the reach itself, neither: the reach is never short.
        for &h in &headings {
            let t = touched(&mut d, body, portal, centre, r + 0.02, h);
            assert_ne!(
                t,
                Some(true),
                "{what}: touched beyond its reach, heading {h}"
            );
        }
        // The nearest any heading is touched, found by halving.
        let contact = headings
            .iter()
            .filter_map(|&h| {
                let (mut lo, mut hi) = (0.0_f32, r + 0.02);
                if touched(&mut d, body, portal, centre, lo, h) != Some(true) {
                    return None;
                }
                for _ in 0..20 {
                    let mid = (lo + hi) * 0.5;
                    if touched(&mut d, body, portal, centre, mid, h) == Some(true) {
                        lo = mid;
                    } else {
                        hi = mid;
                    }
                }
                Some(lo)
            })
            .fold(0.0_f32, f32::max);
        println!(
            "{what} ({setup:#010X} at {scale}): reach {r:.3} m, touched out to {contact:.3} m, \
             kept off at {:.3} m",
            r + MARGIN
        );
        // A sphere or a cylinder is touched right up to its reach; a mesh is bounded by it.
        if geometry.caches_physics_bsp() {
            assert!(contact <= r + 0.02, "{what}: the bound is never short");
        } else {
            assert!(
                (r - contact).abs() < 0.03,
                "{what}: touched up to its reach, {contact} against {r}"
            );
        }
        d.world.destroy(portal);
    }
}
