use dereth_primitives::Vec3;

use super::check;
use super::sphere_cases::CASES;
use auberean_physics::geom::sphere::time_of_collision;

/// The time a moving sphere first touches another answers as the September 2013 client's own code
/// does, to the bit of its double: the earlier root ahead of the start, else the later; -1 for
/// spheres already overlapping, a move too short, or no meeting.
///
/// Behaviour: geometry.sphere.the-time-of-collision-answers-as-the-client-does
#[test]
fn the_time_of_collision_answers_as_the_client_does() {
    check(CASES, |label, x, _| {
        if label != "time_of_collision" {
            return None;
        }
        let t = time_of_collision(
            Vec3::new(x[0], x[1], x[2]),
            Vec3::new(x[3], x[4], x[5]),
            x[6],
        );
        let b = t.to_bits();
        #[allow(clippy::cast_possible_truncation)]
        let out = vec![b as u32, (b >> 32) as u32];
        Some(out)
    });
}
