//! The camera's view, the views through doorways, and the walk through made-up cells.

use std::collections::HashMap;

use auberean_draw::cull::{Beyond, Doorway, Frustum, Relation, Room, Rooms};
use auberean_draw::scene::Camera;
use auberean_primitives::Vec3;
use auberean_world::math::Mat4;
use dereth_primitives::num::math::{cosf, sinf};
use dereth_primitives::Vec3 as V;

/// A box of half side `r` about `c`.
fn cube(c: V, r: f32) -> (V, V) {
    (
        V::new(c.x - r, c.y - r, c.z - r),
        V::new(c.x + r, c.y + r, c.z + r),
    )
}

/// The point `d` meters from the origin along a direction `a` degrees off north toward the east
/// and `e` degrees above the horizon.
fn toward(a: f32, e: f32, d: f32) -> V {
    let (a, e) = (a.to_radians(), e.to_radians());
    V::new(sinf(a) * cosf(e) * d, cosf(a) * cosf(e) * d, sinf(e) * d)
}

/// The camera's view at the origin looking north, 75 degrees high in a picture four thirds as wide.
fn north() -> Frustum {
    Frustum::camera(
        V::new(0.0, 0.0, 0.0),
        V::new(1.0, 0.0, 0.0),
        V::new(0.0, 1.0, 0.0),
        V::new(0.0, 0.0, 1.0),
        75.0 * 4.0 / 3.0,
        75.0,
        0.1,
    )
}

/// The camera's view reaches across half the field of view times the picture's shape (50 degrees
/// either way for 75 degrees at four thirds), and up and down to ninety degrees less half the
/// field of view (52.5 degrees): looser than the picture, whose top lies at 37.5 degrees.
///
/// Behaviour: draw.inside.the-cameras-view-is-looser-than-its-picture
#[test]
fn the_cameras_view_is_looser_than_its_picture() {
    let f = north();
    let at = |a: f32, e: f32| {
        let (lo, hi) = cube(toward(a, e, 50.0), 0.1);
        f.relation(lo, hi)
    };
    assert_eq!(at(0.0, 0.0), Relation::Inside);
    assert_eq!(at(48.0, 0.0), Relation::Inside);
    assert_eq!(at(-48.0, 0.0), Relation::Inside);
    assert_eq!(at(53.0, 0.0), Relation::Outside);
    assert_eq!(at(0.0, 45.0), Relation::Inside, "past the picture's top");
    assert_eq!(at(0.0, -50.0), Relation::Inside);
    assert_eq!(at(0.0, 55.0), Relation::Outside);
    assert_eq!(at(180.0, 0.0), Relation::Outside, "behind");
    let (lo, hi) = cube(toward(0.0, 0.0, 0.05), 0.01);
    assert_eq!(
        f.relation(lo, hi),
        Relation::Outside,
        "nearer than the near plane"
    );
    let (lo, hi) = cube(toward(53.0, 0.0, 50.0), 3.0);
    assert_eq!(f.relation(lo, hi), Relation::Partly);
}

/// A doorway 2 m wide and 2 m high, 5 m north of the origin, its corners running clockwise as seen
/// from the south.
fn door(y: f32) -> Vec<V> {
    vec![
        V::new(-1.0, y, 0.0),
        V::new(-1.0, y, 2.0),
        V::new(1.0, y, 2.0),
        V::new(1.0, y, 0.0),
    ]
}

/// The view through a doorway holds what lies beyond it as the camera sees it, and nothing to
/// its side; a doorway partly in view is clipped to the view, one out of view shows nothing, and
/// one whose plane the camera stands within a centimeter of shows whole.
///
/// Behaviour: draw.inside.a-doorway-narrows-the-view-to-what-shows-through-it
#[test]
fn a_doorway_narrows_the_view_to_what_shows_through_it() {
    let view = north();
    let shown = view.check_polygon(&door(5.0), true).expect("in view");
    assert_eq!(shown.len(), 4, "wholly in view");
    let through = view.through(&shown).expect("a view");
    assert_eq!(through.planes.len(), 4);
    let (lo, hi) = cube(V::new(0.0, 20.0, 2.0), 0.5);
    assert_ne!(
        through.relation(lo, hi),
        Relation::Outside,
        "beyond the doorway"
    );
    let (lo, hi) = cube(V::new(8.0, 20.0, 2.0), 0.5);
    assert_eq!(through.relation(lo, hi), Relation::Outside, "beside it");
    // half off the view's left edge: clipped to it
    let wide = vec![
        V::new(-30.0, 5.0, 0.0),
        V::new(-30.0, 5.0, 2.0),
        V::new(1.0, 5.0, 2.0),
        V::new(1.0, 5.0, 0.0),
    ];
    let clipped = view.check_polygon(&wide, true).expect("partly in view");
    assert!(clipped.iter().all(|c| c.x > -6.0), "{clipped:?}");
    // behind the camera
    assert!(view.check_polygon(&door(-5.0), true).is_none());
    // the camera in the doorway's plane: the doorway whole, unclipped
    let flat = door(0.005);
    assert_eq!(view.check_polygon(&flat, true), Some(flat.clone()));
    // a doorway of 32 corners or more makes no view of its own
    let many: Vec<V> = (0..32)
        .map(|k| {
            #[allow(clippy::cast_precision_loss)]
            let a = k as f32 / 32.0 * std::f32::consts::TAU;
            V::new(cosf(a), 5.0, 1.0 + sinf(a))
        })
        .collect();
    assert!(view.through(&many).is_none());
}

/// A room 10 m square about `(0, y)`, its floor at 0 and ceiling at 4, with `doorways`.
fn room(y: f32, doorways: Vec<Doorway>, no_cull: bool) -> Room {
    Room {
        place: Mat4::IDENTITY,
        lo: Vec3::new(-5.0, y - 5.0, 0.0),
        hi: Vec3::new(5.0, y + 5.0, 4.0),
        no_cull,
        sky: false,
        doorways,
        shared: Vec::new(),
        fog_cut: None,
    }
}

/// A doorway in the wall at `y`, running clockwise from the side `from_south` names.
fn wall(y: f32, from_south: bool, beyond: Beyond) -> Doorway {
    let mut corners: Vec<Vec3> = door(y)
        .into_iter()
        .map(|c| Vec3::new(c.x, c.y, c.z))
        .collect();
    if !from_south {
        corners.reverse();
    }
    Doorway { corners, beyond }
}

/// Three rooms in a row northward (0x100 about y 0, 0x101 about y 10, 0x102 about y 20), each with
/// a doorway into the next, and a doorway out of the last to the land.
fn row() -> Rooms {
    let mut cells = HashMap::new();
    cells.insert(
        0x100,
        room(0.0, vec![wall(5.0, true, Beyond::Cell(0x101))], false),
    );
    cells.insert(
        0x101,
        room(
            10.0,
            vec![
                wall(5.0, false, Beyond::Cell(0x100)),
                wall(15.0, true, Beyond::Cell(0x102)),
            ],
            false,
        ),
    );
    cells.insert(
        0x102,
        room(
            20.0,
            vec![
                wall(15.0, false, Beyond::Cell(0x101)),
                wall(25.0, true, Beyond::Outside),
            ],
            false,
        ),
    );
    Rooms {
        root: 0x100,
        stab: vec![0x101, 0x102],
        cells,
        group_of: HashMap::new(),
        groups: Vec::new(),
        fog_color: [0.5, 0.25, 0.0, 1.0],
    }
}

fn camera(heading: f32) -> Camera {
    Camera {
        position: Vec3::new(0.0, 0.0, 1.5),
        heading,
        pitch: 0.0,
        fov_y: 75.0,
        near: 0.1,
        far: 2000.0,
        orthographic: None,
    }
}

/// Walking from the camera's room: looking down the row, every room is reached through the
/// doorways and the land through the last; looking back, only the camera's room is drawn and no
/// land.
///
/// Behaviour: draw.inside.the-walk-reaches-the-rooms-the-doorways-show
#[test]
fn the_walk_reaches_the_rooms_the_doorways_show() {
    let rooms = row();
    let ahead = rooms.sight(&camera(0.0), 4.0 / 3.0);
    let mut drawn: Vec<u16> = ahead.drawn.iter().copied().collect();
    drawn.sort_unstable();
    assert_eq!(drawn, [0x100, 0x101, 0x102]);
    assert_eq!(ahead.outside.len(), 1, "the land through the last doorway");
    assert!(ahead.outdoors() && ahead.sky() && !ahead.fog);
    let back = rooms.sight(&camera(180.0), 4.0 / 3.0);
    let drawn: Vec<u16> = back.drawn.iter().copied().collect();
    assert_eq!(drawn, [0x100]);
    assert!(!back.outdoors() && !back.sky());
}

/// A room past the interior fog's far distance is left out, and the background painted, only by
/// the distance the room took when it last held the camera: a room the camera never stood in is
/// walked into whatever its distance.
///
/// Behaviour: draw.inside.a-room-past-the-fog-is-left-out-by-its-own-distance
#[test]
fn a_room_past_the_fog_is_left_out_by_its_own_distance() {
    let mut rooms = row();
    let ahead = rooms.sight(&camera(0.0), 4.0 / 3.0);
    assert!(
        !ahead.fog && ahead.drawn.contains(&0x102),
        "never held the camera"
    );
    // the last room held the camera once, in a block whose fog ends at 12 m
    rooms.cells.get_mut(&0x102).expect("the room").fog_cut = Some(12.0);
    let ahead = rooms.sight(&camera(0.0), 4.0 / 3.0);
    assert!(ahead.fog, "the background painted");
    assert!(!ahead.drawn.contains(&0x102) && ahead.drawn.contains(&0x101));
    assert!(!ahead.outdoors(), "nothing walked past it");
    // the camera's own room is never left out
    rooms.cells.get_mut(&0x100).expect("the room").fog_cut = Some(0.5);
    assert!(rooms.sight(&camera(0.0), 4.0 / 3.0).drawn.contains(&0x100));
}

/// Never-culled rooms are taken whole with the rooms drawn with them: entering one reaches every
/// room of its group whatever the view, the group's doorways out add their whole views out, and
/// the culled rooms beyond are walked on. A room of the group wholly out of view is not entered.
///
/// Behaviour: draw.inside.never-culled-rooms-are-taken-with-their-group
#[test]
fn never_culled_rooms_are_taken_with_their_group() {
    let mut rooms = row();
    for low in [0x101, 0x102] {
        let r = rooms.cells.get_mut(&low).expect("the room");
        r.no_cull = true;
        r.shared = vec![0x101, 0x102];
    }
    rooms.make_groups(&[0x100, 0x101, 0x102]);
    assert_eq!(rooms.groups.len(), 1);
    assert_eq!(rooms.groups[0].cells, [0x101, 0x102]);
    assert_eq!(rooms.groups[0].outside.len(), 1);
    // facing the first doorway: the group whole, its doorway out
    let ahead = rooms.sight(&camera(0.0), 4.0 / 3.0);
    let mut drawn: Vec<u16> = ahead.drawn.iter().copied().collect();
    drawn.sort_unstable();
    assert_eq!(drawn, [0x100, 0x101, 0x102]);
    assert!(ahead.outdoors());
    // facing away, the doorway into the group is not checked against the view, but the group's
    // first room lies wholly behind the camera and is not entered
    let back = rooms.sight(&camera(180.0), 4.0 / 3.0);
    assert!(!back.drawn.contains(&0x101) && !back.outdoors());
}
