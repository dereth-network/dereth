//! Headings, vectors and planes as text.
//! Fixture: every 11.25 degrees round the compass, and the edges of its points.

use auberean_physics::geom::{fixed_three, plane_text, vector_text, Heading, Plane};
use dereth_primitives::Vec3;

/// A heading reads as the nearest of the sixteen points of the compass: within 11.25 degrees of
/// north, south, east or west, the one letter, the edge itself not included; within 11.25 degrees
/// of the middle of a quarter, its two letters, the edges included; else three, the nearer of the
/// two letters first.
///
/// Behaviour: geometry.text.a-heading-reads-as-its-point-of-the-compass
#[test]
fn a_heading_reads_as_its_point_of_the_compass() {
    for (degrees, point) in [
        (0.0, "N"),
        (11.0, "N"),
        (349.0, "N"),
        (11.25, "NNE"),
        (22.5, "NNE"),
        (33.74, "NNE"),
        (33.75, "NE"),
        (45.0, "NE"),
        (56.25, "NE"),
        (56.26, "ENE"),
        (67.5, "ENE"),
        (78.75, "ENE"),
        (90.0, "E"),
        (112.5, "ESE"),
        (135.0, "SE"),
        (157.5, "SSE"),
        (180.0, "S"),
        (202.5, "SSW"),
        (225.0, "SW"),
        (247.5, "WSW"),
        (270.0, "W"),
        (292.5, "WNW"),
        (315.0, "NW"),
        (337.5, "NNW"),
    ] {
        assert_eq!(Heading::new(degrees).compass(), point, "at {degrees}");
    }
}

/// A vector and a plane read with three places, as the client's C library writes them, infinities
/// and not-a-numbers rounded at the same place.
///
/// Behaviour: geometry.text.vectors-and-planes-read-with-three-places
#[test]
fn vectors_and_planes_read_with_three_places() {
    assert_eq!(
        vector_text(Vec3::new(1.0, -2.5, 0.12345)),
        "(1.000, -2.500, 0.123)"
    );
    assert_eq!(
        plane_text(&Plane {
            normal: Vec3::new(0.0, 0.0, 1.0),
            d: -3.5,
        }),
        "N = (0.000, 0.000, 1.000), d = -3.500\n"
    );
    assert_eq!(fixed_three(f32::INFINITY), "1.#IO");
    assert_eq!(fixed_three(f32::NEG_INFINITY), "-1.#IO");
    assert_eq!(fixed_three(f32::NAN), "1.#QO");
}
