//! Behaviour: none (checks the world builder's editor without a window: where things fall on the screen, its scripts, its playtest characters, and how it starts and talks to Yalain)

mod play;

use std::path::Path;

use athenaeum_editor::app::{badge, halved, kind_word, leaning, parse_id};
use athenaeum_editor::scene::picture_camera;
use athenaeum_editor::script::{parse, Step};
use athenaeum_editor::view::{global, hit_ground, local, View};
use dereth_builder::camera::FlyCamera;
use dereth_primitives::num::math;
use dereth_primitives::Vec3;
use yalain_world_db::rows::{At, Object, ObjectKind, Prov, Row, Source};

fn close(a: [f32; 2], b: [f32; 2]) -> bool {
    (a[0] - b[0]).abs() < 0.05 && (a[1] - b[1]).abs() < 0.05
}

#[test]
fn a_landblocks_point_is_one_point_of_the_worlds_grid() {
    assert_eq!(
        global(0x77B8, [1.5, 100.0, 182.5]),
        [119.0 * 160.0 + 1.5, 184.0 * 160.0 + 100.0, 182.5]
    );
    for (lb, at) in [
        (0x77B8, [1.5, 100.0, 182.5]),
        (0x0000, [0.0, 0.0, 0.0]),
        (0xFEFE, [159.5, 0.25, -3.0]),
    ] {
        assert_eq!(local(global(lb, at)), (lb, at));
    }
}

#[test]
fn a_pixel_turned_into_a_ray_falls_back_on_that_pixel() {
    let camera = FlyCamera::new(Vec3::new(1000.0, 2000.0, 50.0), 0.7, -0.3);
    let v = View::of(&camera, 70.0, (1600, 900));
    // Straight ahead is the middle of the window.
    let ahead = [
        v.eye[0] + v.forward[0] * 10.0,
        v.eye[1] + v.forward[1] * 10.0,
        v.eye[2] + v.forward[2] * 10.0,
    ];
    assert!(close(v.project(ahead).unwrap(), [800.0, 450.0]));
    for px in [[10.0, 20.0], [1590.0, 880.0], [800.0, 100.0]] {
        let d = v.ray(px);
        let p = [
            v.eye[0] + d[0] * 37.0,
            v.eye[1] + d[1] * 37.0,
            v.eye[2] + d[2] * 37.0,
        ];
        assert!(close(v.project(p).unwrap(), px), "{px:?}");
    }
    // Behind the eye is nowhere on the window.
    let behind = [
        v.eye[0] - v.forward[0],
        v.eye[1] - v.forward[1],
        v.eye[2] - v.forward[2],
    ];
    assert!(v.project(behind).is_none());
}

#[test]
fn the_camera_draws_from_the_heading_and_pitch_it_flies_at() {
    // Looking north and level: heading 0; turned a quarter to the left (west): heading 270.
    let north = View::of(
        &FlyCamera::new(Vec3::new(0.0, 0.0, 0.0), 0.0, 0.0),
        70.0,
        (800, 600),
    );
    assert!(north.heading().abs() < 1e-3 || (north.heading() - 360.0).abs() < 1e-3);
    assert!(north.pitch().abs() < 1e-3);
    let west = View::of(
        &FlyCamera::new(Vec3::new(0.0, 0.0, 0.0), std::f32::consts::FRAC_PI_2, -0.5),
        70.0,
        (800, 600),
    );
    assert!((west.heading() - 270.0).abs() < 1e-2, "{}", west.heading());
    assert!((west.pitch() + 0.5f32.to_degrees()).abs() < 1e-2);
}

#[test]
fn a_ray_meets_the_ground_where_it_first_goes_under_it() {
    // A slope rising to the east: the ground is x / 10 meters high.
    let mut ground = |x: f32, _y: f32| Some(x / 10.0);
    let hit = hit_ground([0.0, 0.0, 20.0], [0.6, 0.0, -0.8], 500.0, 2.0, &mut ground).unwrap();
    // Along the ray z = 20 - 4/3 x meets z = x / 10 at x = 20 / (4/3 + 1/10).
    let x = 20.0 / (4.0 / 3.0 + 0.1);
    assert!((hit[0] - x).abs() < 0.02, "{hit:?}");
    assert!((hit[2] - x / 10.0).abs() < 0.01);
    // A ray going up never meets it; off the world there is no ground.
    assert!(hit_ground([0.0, 0.0, 20.0], [0.0, 0.6, 0.8], 500.0, 2.0, &mut ground).is_none());
    assert!(hit_ground(
        [0.0, 0.0, 20.0],
        [0.6, 0.0, -0.8],
        500.0,
        2.0,
        &mut |_, _| None
    )
    .is_none());
}

#[test]
fn a_script_reads_its_steps_and_runs_them_up_to_each_wait_or_shot() {
    let text = "# a demo\nwait 2\ncamera 77B8 6 86 188 0 -12\nplace person 0x470033D0 77B8 7 105 180\nmove object_n-334c@77B8 77B8 3 104\nset object_person-1@77B8 name { table = 0x1, string = 0x2 }\nshot a.png\nsave\nquit\n";
    let steps = parse(text, Path::new("shots")).unwrap();
    assert_eq!(steps.len(), 8);
    assert_eq!(
        steps[3],
        Step::Move("object n-334c@77B8".into(), 0x77B8, 3.0, 104.0)
    );
    assert_eq!(
        steps[4],
        Step::Set(
            "object person-1@77B8".into(),
            "name".into(),
            "{ table = 0x1, string = 0x2 }".into()
        )
    );
    assert!(parse("jump 3", Path::new("."))
        .unwrap_err()
        .contains("line 1"));
    let file = std::env::temp_dir().join(format!("athenaeum-script-{}.txt", std::process::id()));
    std::fs::write(&file, text).unwrap();
    let mut s = athenaeum_editor::script::Script::load(&file, Path::new("shots")).unwrap();
    std::fs::remove_file(&file).ok();
    assert!(s.due(0.0).is_empty(), "the first step waits");
    assert!(s.due(1.0).is_empty());
    let first = s.due(2.5);
    assert_eq!(first.len(), 5, "up to and with the shot: {first:?}");
    assert_eq!(
        first.last(),
        Some(&Step::Shot(Path::new("shots").join("a.png")))
    );
    assert_eq!(s.due(2.6), [Step::Save, Step::Quit]);
}

#[test]
fn a_rows_letter_says_where_it_came_from() {
    let mut p = Prov::AUTHORED;
    assert_eq!(badge(Some(&p)).0, "A");
    p.source = Source::Capture;
    assert_eq!(badge(Some(&p)).0, "C");
    p.source = Source::Dat;
    assert_eq!(badge(Some(&p)).0, "D");
    p.source = Source::Derived;
    assert_eq!(badge(Some(&p)).0, "R");
    let o = |kind, to: Option<&str>| {
        Row::Object(Object {
            key: "k".into(),
            kind,
            id: None,
            template: 1,
            cell: None,
            at: At {
                x: 0.0,
                y: 0.0,
                z: None,
            },
            rotation: None,
            heading: None,
            scale: None,
            name: None,
            name_text: None,
            to: to.map(str::to_string),
            landings: Vec::new(),
            scene: None,
            locked: false,
            lock: None,
            route: None,
            route_start: 0,
            route_backward: false,
            requires: None,
            recall: false,
            summon: false,
            prov: Prov::AUTHORED,
        })
    };
    assert_eq!(kind_word(&o(ObjectKind::Npc, None)), "person");
    assert_eq!(kind_word(&o(ObjectKind::Static, Some("inn"))), "portal");
    assert_eq!(kind_word(&o(ObjectKind::Static, None)), "object");
    assert_eq!(parse_id("0x470033D0"), Some(0x4700_33D0));
    assert_eq!(parse_id("470033d0"), Some(0x4700_33D0));
    assert_eq!(parse_id("0"), None);
    assert_eq!(parse_id("vendor"), None);
}

#[test]
fn a_script_clicks_and_drags_in_stages_a_frame_can_see() {
    let steps = parse(
        "click 10 20\ndrag 1 2 3 4\nkey KeyR ctrl\ntype a b",
        Path::new("."),
    )
    .unwrap();
    assert_eq!(
        &steps[..5],
        &[
            Step::Point(10.0, 20.0),
            Step::Wait(0.1),
            Step::Button(true, true),
            Step::Wait(0.1),
            Step::Button(true, false),
        ]
    );
    assert_eq!(steps[6], Step::Wait(0.1));
    assert_eq!(steps[9], Step::Point(3.0, 4.0));
    assert_eq!(steps[12], Step::Key("KeyR".into(), true, false));
    assert_eq!(steps[13], Step::Type("a b".into()));
}

#[test]
fn the_shortcuts_key_is_ctrl_and_on_a_mac_cmd_too() {
    use winit::keyboard::ModifiersState as M;
    assert!(athenaeum_editor::gui::command_key(M::CONTROL));
    assert_eq!(
        athenaeum_editor::gui::command_key(M::SUPER),
        cfg!(target_os = "macos")
    );
    assert!(!athenaeum_editor::gui::command_key(M::SHIFT));
}

mod gizmo_math {
    use athenaeum_editor::gizmo::*;

    fn ray(origin: [f32; 3], dir: [f32; 3]) -> Ray {
        let l = (dir[0] * dir[0] + dir[1] * dir[1] + dir[2] * dir[2]).sqrt();
        Ray {
            origin,
            dir: [dir[0] / l, dir[1] / l, dir[2] / l],
        }
    }

    #[test]
    fn a_handle_is_taken_within_its_grab_and_the_nearest_arrow_wins() {
        let h = Handles {
            origin: [100.0, 100.0],
            arrows: vec![(Handle::X, [200.0, 100.0]), (Handle::Y, [100.0, 0.0])],
            ring: vec![[170.0, 170.0], [180.0, 160.0]],
        };
        assert_eq!(handle_at(&h, [150.0, 104.0]), Some(Handle::X));
        assert_eq!(handle_at(&h, [150.0, 110.0]), None);
        assert_eq!(handle_at(&h, [175.0, 166.0]), Some(Handle::Turn));
    }

    #[test]
    fn a_drag_along_an_arrow_moves_by_how_far_the_pointers_closest_point_moved() {
        let o = [0.0, 0.0, 0.0];
        let from = ray([1.0, 0.0, 10.0], [0.0, 0.1, -1.0]);
        let to = ray([4.0, 0.0, 10.0], [0.0, 0.1, -1.0]);
        assert!((drag_along(Handle::X, o, &from, &to).unwrap() - 3.0).abs() < 1e-4);
        assert!(drag_along(Handle::Y, o, &from, &to).unwrap().abs() < 1e-4);
    }

    #[test]
    fn a_drag_on_the_ring_turns_by_the_angle_swept_the_short_way() {
        let o = [0.0, 0.0, 5.0];
        let down = [0.0, 0.0, -1.0];
        let north = ray([0.0, 3.0, 20.0], down);
        let east = ray([3.0, 0.0, 20.0], down);
        assert!((drag_turn(o, &north, &east).unwrap() - 90.0).abs() < 1e-3);
        let south_west = ray([-3.0, 0.1, 20.0], down);
        assert!((drag_turn(o, &east, &south_west).unwrap() + 178.09).abs() < 0.05);
    }

    #[test]
    fn snapping_goes_to_the_nearest_step_and_shift_does_the_other() {
        assert_eq!(snapped(12.3, 5.0), 10.0);
        assert_eq!(snapped(12.6, 5.0), 15.0);
        assert_eq!(snapped(7.3, 0.0), 7.3);
        let mut s = Snap::default();
        assert!(!s.applies(false) && s.applies(true));
        s.on = true;
        assert!(s.applies(false) && !s.applies(true));
        assert_eq!(s.turn_step(15.0), s.angle);
    }
}

#[test]
fn a_name_reads_without_the_games_marks() {
    use athenaeum_editor::app::plain_name;
    assert_eq!(plain_name("Frank the Trader[mn]"), "Frank the Trader");
    assert_eq!(plain_name("Refining Workshop[E]"), "Refining Workshop");
    assert_eq!(plain_name("Plain"), "Plain");
}

#[test]
fn a_landblock_is_a_place_to_go_and_other_words_are_not() {
    use athenaeum_editor::app::go_to_of;
    assert_eq!(go_to_of("5766"), Some((0x5766, [80.0, 80.0])));
    assert_eq!(go_to_of(" 5766:123,45 "), Some((0x5766, [123.0, 45.0])));
    assert_eq!(go_to_of("frank"), None);
    assert_eq!(go_to_of("576"), None);
    assert_eq!(go_to_of("5766:12"), None);
}

#[test]
fn a_place_is_typed_without_its_quotes() {
    use athenaeum_editor::app::field_toml;
    assert_eq!(field_toml("to", "inn"), "\"inn\"");
    assert_eq!(field_toml("to", "\"inn\""), "\"inn\"");
    assert_eq!(field_toml("to", " "), "");
    assert_eq!(field_toml("kind", "static"), "\"static\"");
    assert_eq!(field_toml("radius", "5.0"), "5.0");
    assert_eq!(field_toml("name_text", "Old Tom"), "\"Old Tom\"");
    assert_eq!(
        field_toml("name_text", "Tom \"Red\" Lee"),
        "\"Tom \\\"Red\\\" Lee\""
    );
}

#[test]
fn a_line_meets_a_box_from_outside_and_inside_and_misses_it() {
    use athenaeum_editor::view::ray_box;
    let (lo, hi) = ([-1.0, -1.0, 0.0], [1.0, 1.0, 2.0]);
    assert_eq!(
        ray_box([-5.0, 0.0, 1.0], [1.0, 0.0, 0.0], lo, hi),
        Some(4.0)
    );
    assert_eq!(ray_box([0.0, 0.0, 1.0], [1.0, 0.0, 0.0], lo, hi), Some(0.0));
    assert_eq!(ray_box([-5.0, 3.0, 1.0], [1.0, 0.0, 0.0], lo, hi), None);
    assert_eq!(
        ray_box([5.0, 0.0, 1.0], [1.0, 0.0, 0.0], lo, hi),
        None,
        "behind"
    );
}

#[test]
fn turning_back_undoes_a_rows_heading() {
    use athenaeum_editor::view::{heading_rotation, unrotated};
    let east = unrotated(heading_rotation(90.0), [0.0, 1.0, 0.0]);
    let back = unrotated(heading_rotation(0.0), [0.0, 1.0, 0.0]);
    assert!((back[1] - 1.0).abs() < 1e-5);
    assert!(east[1].abs() < 1e-5, "{east:?}");
    assert!((east[0].abs() - 1.0).abs() < 1e-5, "{east:?}");
}

#[test]
fn a_camps_creatures_are_written_as_its_field_reads() {
    use athenaeum_editor::app::spawns_toml;
    assert_eq!(
        spawns_toml(&[(0x4700_1000, 2), (0x4700_1001, 1)]),
        "[{ template = 0x47001000, weight = 2 }, { template = 0x47001001, weight = 1 }]"
    );
}

#[test]
fn a_floor_is_level_either_way_round_and_a_wall_is_not() {
    use athenaeum_editor::view::is_level;
    let floor = [[0.0, 0.0, 0.0], [1.0, 0.0, 0.0], [0.0, 1.0, 0.0]];
    assert!(is_level(&floor));
    assert!(is_level(&[floor[1], floor[0], floor[2]]));
    assert!(!is_level(&[
        [0.0, 0.0, 0.0],
        [1.0, 0.0, 0.0],
        [0.0, 0.0, 1.0]
    ]));
}

#[test]
fn a_picture_holds_all_of_what_it_shows_whatever_its_size_and_shape() {
    let boxes = [
        ([-0.1, -0.1, 0.0], [0.1, 0.1, 0.02]),
        ([-0.4, -0.3, 0.0], [0.4, 0.3, 1.9]),
        ([-30.0, -8.0, -2.0], [30.0, 8.0, 12.0]),
        ([5.0, 5.0, 5.0], [5.0, 5.0, 5.0]),
    ];
    for (lo, hi) in boxes {
        let c = picture_camera(lo, hi);
        let (h, p) = (c.heading.to_radians(), c.pitch.to_radians());
        let (sin, cos) = (math::sinf, math::cosf);
        let f = [sin(h) * cos(p), cos(h) * cos(p), sin(p)];
        let r = [cos(h), -sin(h), 0.0];
        let u = [
            r[1] * f[2] - r[2] * f[1],
            r[2] * f[0] - r[0] * f[2],
            r[0] * f[1] - r[1] * f[0],
        ];
        let half = (c.fov_y / 2.0).to_radians();
        let reach = sin(half) / cos(half);
        let dot = |a: [f32; 3], b: [f32; 3]| a[0] * b[0] + a[1] * b[1] + a[2] * b[2];
        for corner in 0..8 {
            let pick = |i: usize| if corner >> i & 1 == 0 { lo[i] } else { hi[i] };
            let v = [
                pick(0) - c.position.x,
                pick(1) - c.position.y,
                pick(2) - c.position.z,
            ];
            let depth = dot(v, f);
            assert!(depth > c.near && depth < c.far, "{lo:?} {hi:?}: {depth}");
            assert!(dot(v, r).abs() <= depth * reach * 1.001, "{lo:?} {hi:?}");
            assert!(dot(v, u).abs() <= depth * reach * 1.001, "{lo:?} {hi:?}");
        }
    }
}

#[test]
fn a_picture_is_halved_by_averaging_and_comes_out_opaque() {
    #[rustfmt::skip]
    let big = [
        10, 20, 30, 0,   20, 40, 60, 0,   0, 0, 0, 255,   0, 0, 0, 255,
        30, 60, 90, 0,   40, 80, 120, 0,  0, 0, 0, 255,   0, 0, 0, 255,
        0, 0, 0, 0,      0, 0, 0, 0,      4, 4, 4, 9,     4, 4, 4, 9,
        0, 0, 0, 0,      0, 0, 0, 0,      4, 4, 4, 9,     4, 4, 4, 9,
    ];
    assert_eq!(
        halved(&big, 2),
        [25, 50, 75, 255, 0, 0, 0, 255, 0, 0, 0, 255, 4, 4, 4, 255]
    );
}

#[test]
fn a_lean_keeps_the_heading_on_flat_ground_and_points_up_along_the_slope() {
    let turned = |q: [f32; 4], v: [f32; 3]| {
        let [w, x, y, z] = q;
        let u = [x, y, z];
        let cross = |a: [f32; 3], b: [f32; 3]| {
            [
                a[1] * b[2] - a[2] * b[1],
                a[2] * b[0] - a[0] * b[2],
                a[0] * b[1] - a[1] * b[0],
            ]
        };
        let t = cross(u, v).map(|c| 2.0 * c);
        let c = cross(u, t);
        [
            v[0] + w * t[0] + c[0],
            v[1] + w * t[1] + c[1],
            v[2] + w * t[2] + c[2],
        ]
    };
    let flat = leaning(90.0, [0.0, 0.0, 1.0]);
    let plain = athenaeum_editor::view::heading_rotation(90.0);
    for (a, b) in flat.iter().zip(plain) {
        assert!((a - b).abs() < 1e-5, "{flat:?} {plain:?}");
    }
    let n = [0.6, 0.0, 0.8];
    for heading in [0.0, 37.0, 200.0] {
        let up = turned(leaning(heading, n), [0.0, 0.0, 1.0]);
        for (a, b) in up.iter().zip(n) {
            assert!((a - b).abs() < 1e-4, "{heading}: {up:?}");
        }
    }
}

#[test]
fn a_map_pixel_and_the_place_under_it_agree() {
    use athenaeum_editor::map::{pixel_of, place_at, SIDE};
    assert_eq!(place_at([-1.0, 5.0]), None, "off the map");
    #[allow(clippy::cast_precision_loss)]
    let side = SIDE as f32;
    assert_eq!(place_at([side, 5.0]), None, "off the map");
    let at = global(0x5766, [80.0, 40.0, 0.0]);
    let (lb, local) = place_at(pixel_of(at[0], at[1])).expect("on the map");
    assert_eq!(lb, 0x5766);
    assert!(
        (local[0] - 80.0).abs() < 0.01 && (local[1] - 40.0).abs() < 0.01,
        "{local:?}"
    );
    // North is up: further north is higher on the map.
    assert!(pixel_of(at[0], at[1] + 160.0)[1] < pixel_of(at[0], at[1])[1]);
    // The map's corners are in the world's.
    assert_eq!(place_at([0.5, side - 0.5]).map(|p| p.0), Some(0x0000));
    assert_eq!(place_at([side - 0.5, 0.0]).map(|p| p.0), Some(0xFFFF));
}

#[test]
fn the_map_tells_kinds_of_ground_apart_and_low_from_high() {
    use athenaeum_editor::map::color_of;
    let colors = [
        color_of("TownHuman", 40.0, false),
        color_of("Water-Deep", 40.0, false),
        color_of("HumanBeach", 40.0, false),
        color_of("highland-Snow", 40.0, false),
        color_of("HighlandRock", 40.0, false),
        color_of("GrassLand", 40.0, false),
        color_of("GrassLand", 40.0, true),
    ];
    for (i, a) in colors.iter().enumerate() {
        for b in &colors[i + 1..] {
            assert_ne!(a, b);
        }
    }
    let (low, high) = (
        color_of("GrassLand", 0.0, false),
        color_of("GrassLand", 72.0, false),
    );
    assert!(low[1] < high[1], "{low:?} {high:?}");
}
