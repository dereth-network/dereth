//! Behaviour: none (checks the world builder's editor without a window: where things fall on the screen, its scripts, and how it starts Yalain)

use std::path::Path;

use athenaeum_editor::app::{badge, kind_word, parse_id};
use athenaeum_editor::live::{self, Programs};
use athenaeum_editor::script::{parse, Step};
use athenaeum_editor::view::{global, hit_ground, local, View};
use dereth_builder::camera::FlyCamera;
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
fn yalain_is_started_on_the_world_over_its_base_keeping_nothing() {
    let p = Programs {
        port: 9300,
        ..Programs::default()
    };
    let entry = live::Entry {
        cell: 0x77B8_0005,
        at: [6.0, 86.0, 183.0],
        heading: 90.0,
    };
    let a = live::server_args(
        &p,
        Path::new("w"),
        Some(Path::new("db")),
        Path::new("install"),
        entry,
    );
    let at = |flag: &str| a.iter().position(|x| x == flag).map(|i| a[i + 1].as_str());
    assert_eq!(at("--world-db"), Some("w"));
    assert_eq!(at("--world-base"), Some("db"));
    assert_eq!(at("--dats"), Some("install"));
    assert_eq!(at("--bind"), Some("127.0.0.1"));
    assert_eq!(at("--login-port"), Some("9300"));
    assert_eq!(at("--world-port"), Some("9302"));
    assert_eq!(at("--add-character"), Some("builder:Builder:1:4096"));
    assert_eq!(at("--place"), Some("builder:Builder:77B80005:6:86:183:90"));
    assert!(a.iter().any(|x| x == "--memory"));
    let no_base = live::server_args(&p, Path::new("w"), None, Path::new("install"), entry);
    assert!(!no_base.iter().any(|x| x == "--world-base"));
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
