//! Portal space: its phases and the tunnel the camera rushes along.
//! Fixture: an effect rolling a generator of its own, driven by a clock in seconds.

use auberean_draw::portal::{ring_alpha, Passage, State, RINGS, SPACING, SPEED};
use auberean_primitives::random::Random;

fn close(a: f32, b: f32) -> bool {
    (a - b).abs() < 1e-4
}

/// Asked for, portal space comes in over a second (the world's view narrowing to a fifth, a black
/// veil, the tunnel and the nebula fading in), stays in (no world drawn), and when no longer
/// wanted goes out over a second (the tunnel's view widening, white fading in) and comes out over
/// another (the world's view widening back, the white fading out); asked for at once, it is in at
/// once.
///
/// Behaviour: draw.portal.portal-space-comes-in-and-goes-out-a-second-a-phase
#[test]
fn portal_space_comes_in_and_goes_out_a_second_a_phase() {
    let mut p = Passage::new(Random::new());
    assert!(p.think(0.0).draw_world);
    p.want(true, false, 10.0);
    let l = p.think(10.0);
    assert_eq!(p.state, State::ComingIn);
    assert!(l.draw_world && l.draw_tunnel);
    let l = p.think(10.5);
    assert!(close(l.world_fov, 1.0 - 0.8 * 0.25), "{l:?}");
    assert!(close(l.visibility, 0.5) && close(l.nebula, 0.5), "{l:?}");
    assert_eq!(l.under, Some([0.0, 0.0, 0.0, 0.5]));
    let l = p.think(11.01);
    assert_eq!(p.state, State::In);
    assert!(!l.draw_world && l.draw_tunnel && l.under.is_none() && l.over.is_none());
    p.want(false, false, 12.0);
    p.think(12.0);
    assert_eq!(p.state, State::GoingOut);
    let l = p.think(12.5);
    assert!(
        close(l.tunnel_fov, 1.0 + 1.4 * 0.25) && !l.draw_world,
        "{l:?}"
    );
    assert_eq!(l.over, Some([1.0, 1.0, 1.0, 0.5]));
    p.think(13.01);
    assert_eq!(p.state, State::ComingOut);
    let l = p.think(13.51);
    assert!(l.draw_world && !l.draw_tunnel);
    assert!(close(l.world_fov, 1.0 + 1.4 * 0.25), "{l:?}");
    assert!(close(l.over.map_or(0.0, |c| c[3]), 0.5), "{l:?}");
    p.think(14.02);
    assert_eq!(p.state, State::Off);
    // At once: straight in, the tunnel all there.
    let mut q = Passage::new(Random::new());
    q.want(true, true, 3.0);
    let l = q.think(3.0);
    assert_eq!(q.state, State::In);
    assert!(close(l.visibility, 1.0) && !l.draw_world);
}

/// The tunnel is 150 rings 3 m apart, each with a radius of `10 + 6 sin(0.01 d)` at its distance `d`
/// along the tube; the camera rushes along it at 120 m/s and the rings it passes are made again at
/// the far end, so 150 rings always lie ahead; the rings fade from 1.0 nearest to 0.8 at the
/// sixteenth and then from 0.8 to nothing at the far end.
///
/// Behaviour: draw.portal.the-tunnel-is-rings-three-meters-apart-the-camera-rushes-along
#[test]
fn the_tunnel_is_rings_three_meters_apart_the_camera_rushes_along() {
    let mut p = Passage::new(Random::new());
    p.want(true, true, 0.0);
    p.think(0.0);
    let start = p.distance();
    let rings = p.rings();
    assert_eq!(rings.len(), RINGS);
    for w in rings.windows(2) {
        let (a, _, pa) = w[0];
        let (b, rb, pb) = w[1];
        assert!(close(b - a, SPACING), "{a} {b}");
        let d = ((pb.x - pa.x).powi(2) + (pb.y - pa.y).powi(2) + (pb.z - pa.z).powi(2)).sqrt();
        assert!((d - SPACING).abs() < 1e-3, "rings {d} m apart");
        assert!(
            close(
                rb,
                10.0 + 6.0 * dereth_primitives::num::math::sinf(0.01 * b)
            ),
            "radius {rb} at {b}"
        );
    }
    // Half a second on: 60 m along, every ring still ahead or just passed, the far end made anew.
    p.think(0.25);
    p.think(0.5);
    assert!(
        close(p.distance() - start, SPEED * 0.5),
        "{}",
        p.distance() - start
    );
    let ahead = p.rings();
    assert!(ahead[0].0 <= p.distance() && ahead[1].0 > p.distance());
    assert!(
        ahead[RINGS - 1].0 > p.distance() + 400.0,
        "{:?}",
        ahead[RINGS - 1]
    );
    assert!(close(ring_alpha(0, 0), 1.0) && close(ring_alpha(15, 0), 0.8));
    assert!(close(
        ring_alpha(16, 0),
        0.8 * (1.0 - 16.0 / 149.0f32).powi(2)
    ));
    assert!(close(ring_alpha(149, 0), 0.0) && close(ring_alpha(3, 4), 0.0));
}
