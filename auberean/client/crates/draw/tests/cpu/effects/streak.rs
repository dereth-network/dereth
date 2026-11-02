//! Streaks on made-up lines: the segments a moving line lays, their fading, the streak's lifespan,
//! and the ribbon drawn from them.

use auberean_assets::material::{MaterialLayer, TextureStage};
use auberean_draw::material::{draw_layer, DrawLayer};
use auberean_draw::scene::Pass;
use auberean_draw::streak::{StreakDesc, Trail, STREAK_ROOM};
use auberean_primitives::Vec3;
use auberean_world::color::Color;
use auberean_world::math::Mat4;

/// A line from 0.1 m to 0.9 m up the object's y, trailing 0.2 s, a new segment past 0.1 m.
fn desc() -> StreakDesc {
    StreakDesc {
        start: Vec3::new(0.0, 0.1, 0.0),
        end: Vec3::new(0.0, 0.9, 0.0),
        lifespan: 0.0,
        trail: 0.2,
        min_time: 0.0,
        min_distance: 0.1,
        color: Color::new(1.0, 0.5, 0.25, 1.0),
        material: 0x3100_0200,
        texture_scale: 2.0,
    }
}

/// A land cell of block (0x80, 0x80).
const CELL: u32 = 0x8080_0001;

/// A plain layer to draw a ribbon in.
fn layer() -> DrawLayer {
    draw_layer(&MaterialLayer {
        stages: vec![TextureStage::default()],
        ..MaterialLayer::default()
    })
}

fn at(x: f32) -> Mat4 {
    Mat4::translation(Vec3::new(x, 0.0, 0.0))
}

/// A streak lays segments only while on: turned on and placed, its line where the object stands
/// is the newest segment, a copy laid after the first; moving within the least distance it moves
/// the newest segment along, adding to the distance run; past it, it lays another. Turned off, it
/// lays none and the trail fades.
///
/// Behaviour: draw.streak.a-moving-line-lays-segments-while-the-streak-is-on
#[test]
fn a_moving_line_lays_segments_while_the_streak_is_on() {
    let mut s = Trail::new(desc(), 0.0);
    assert!(
        !s.active(),
        "one lasting until taken away waits to be turned on"
    );
    s.set_position(1, CELL, at(0.0), 1.0);
    s.think(0.05);
    assert!(s.segments().is_empty(), "none while off");
    s.activate();
    s.set_position(2, CELL, at(0.0), 1.0);
    s.think(0.10);
    let n = s.segments();
    assert_eq!(n.len(), 2);
    assert_eq!(n[0].head, Vec3::new(0.0, 0.1, 0.0));
    assert_eq!(n[0].tail, Vec3::new(0.0, 0.9, 0.0));
    // within the least distance: the newest moves with the line
    s.set_position(3, CELL, at(0.05), 1.0);
    s.think(0.11);
    let n = s.segments();
    assert_eq!(n.len(), 2);
    assert!((n[1].distance - 0.05).abs() < 1e-6, "{}", n[1].distance);
    assert_eq!(n[1].head, Vec3::new(0.05, 0.1, 0.0));
    // past it: another is laid
    s.set_position(4, CELL, at(0.3), 1.0);
    s.think(0.12);
    let n = s.segments();
    assert_eq!(n.len(), 3);
    assert!((n[1].distance - 0.3).abs() < 1e-6, "{}", n[1].distance);
    // the line scales with its object
    s.set_position(5, CELL, at(0.3), 2.0);
    s.think(0.13);
    assert_eq!(s.segments().last().map(|g| g.tail.y), Some(1.8));
    s.deactivate();
    let before = s.segments().len();
    s.set_position(6, CELL, at(1.0), 1.0);
    s.think(0.14);
    assert_eq!(s.segments().len(), before, "off: none laid");
}

/// Every segment fades as the square of one less its age over the trail time, and is gone once
/// its age passes the trail time by a tenth of a second; a streak that lasts a while is on at once
/// and goes off once its lifespan has passed.
///
/// Behaviour: draw.streak.a-trail-fades-and-a-streak-ends-with-its-lifespan
#[test]
fn a_trail_fades_and_a_streak_ends_with_its_lifespan() {
    let mut s = Trail::new(desc(), 0.0);
    s.activate();
    s.set_position(1, CELL, at(0.0), 1.0);
    s.think(1.0);
    s.deactivate();
    s.think(1.1);
    let a = s.segments()[0].alpha;
    // laid at 1.0, 0.1 s old of a 0.2 s trail: (1 - 0.5)^2
    assert!((a - 0.25).abs() < 1e-3, "{a}");
    s.think(1.25);
    assert_eq!(s.segments()[0].alpha, 0.0, "past the trail, faded out");
    assert!(!s.segments().is_empty(), "kept a tenth of a second more");
    s.think(1.31);
    assert!(s.segments().is_empty(), "then gone");
    let mut lasting = Trail::new(
        StreakDesc {
            lifespan: 1.0,
            ..desc()
        },
        0.0,
    );
    assert!(lasting.active(), "on at once");
    lasting.think(0.5);
    assert!(lasting.active());
    lasting.think(1.5);
    assert!(!lasting.active(), "off past its lifespan");
}

/// A streak's ribbon: two vertices a segment, oldest first, the line's second point then its first
/// (the picture's v 0 and 1), the picture's u the distance run times the texture scale, each the
/// streak's color with its alpha times the segment's, truncated to bytes; none when its object was
/// not drawn in the frame, with fewer than two segments, or when the frame's streaks have no room
/// left for it; drawn after the particle systems, its segments laid where the trail started.
///
/// Behaviour: draw.streak.a-ribbon-runs-along-the-trail
#[test]
fn a_ribbon_runs_along_the_trail() {
    let mut s = Trail::new(desc(), 0.0);
    s.activate();
    s.set_position(7, CELL, at(0.0), 1.0);
    s.think(0.0);
    s.set_position(7, CELL, at(0.5), 1.0);
    s.think(0.1);
    let mut used = 0;
    assert!(
        s.geometry(6, &mut used).is_none(),
        "not drawn in that frame"
    );
    let v = s.geometry(7, &mut used).expect("a ribbon");
    assert_eq!(v.len(), 2 * s.segments().len());
    assert_eq!(used, v.len());
    assert_eq!(v[0].position, [0.0, 0.9, 0.0], "the second point first");
    assert_eq!(v[1].position, [0.0, 0.1, 0.0]);
    assert_eq!((v[0].uv[0][1], v[1].uv[0][1]), (0.0, 1.0));
    let last = v.len() - 1;
    assert!(
        (v[last].uv[0][0] - 2.0 * 0.5).abs() < 1e-6,
        "u: 0.5 m run, twice"
    );
    // the oldest is half faded (0.1 s of 0.2 s: alpha 0.25): 0.25 * 255 = 63.75
    assert_eq!(v[0].diffuse, (63 << 24) | (255 << 16) | (127 << 8) | 63);
    assert_eq!(v[last].diffuse >> 24, 255);
    let mut full = STREAK_ROOM - 3;
    assert!(s.geometry(7, &mut full).is_none(), "no room left");
    let batches = s.batches(7, &mut 0, &[layer()], CELL);
    assert_eq!(batches.len(), 1);
    assert_eq!(batches[0].pass(), Pass::Streaks);
    assert_eq!(
        batches[0].indices.len(),
        3 * (v.len() - 2),
        "a strip of triangles"
    );
    // drawn in a frame whose world is the next block east: the trail stands a block west
    let east = 0x8180_0001;
    let b = s.batches(7, &mut 0, &[layer()], east);
    assert!(b[0].transform.point(Vec3::ZERO).x < -100.0);
}
