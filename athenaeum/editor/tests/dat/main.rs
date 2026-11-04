//! Behaviour: none (checks the world builder's editor end to end without a window, on the world database and the end-of-retail install)
//!
//! The editor is driven as the window drives it, a frame's input at a time. The install is the
//! end-of-retail data files the tests read everywhere; the database is read from
//! `YALAIN_TEST_WORLD_DB` (a checkout), else from the release this build pins in the per-user cache
//! (`yalain world fetch`); without either, a test fails saying so.

use std::path::PathBuf;

use athenaeum_editor::app::{Editor, Tool, ViewInput};
use athenaeum_editor::gizmo::{self, Handle};
use athenaeum_editor::live::Programs;
use athenaeum_editor::scene::{Scene, WorldData};
use athenaeum_editor::view;
use athenaeum_foundry::{build, new, world, Session};
use auberean_dat::testing::{require_install, TARGET};
use dereth_builder::camera::FlyCamera;
use dereth_primitives::Vec3;
use winit::keyboard::KeyCode;
use yalain_world_db::release::{cache_dir_for, Host, PINNED_TAG};
use yalain_world_db::rows::Row;

/// The variable naming a world database checkout for the tests.
const WORLD_DB_VAR: &str = "YALAIN_TEST_WORLD_DB";

/// A landblock of open ground in a town, and a point on it.
const TOWN: u16 = 0x5766;
const OPEN: [f32; 2] = [123.0, 45.0];

fn database() -> PathBuf {
    if let Some(v) = std::env::var_os(WORLD_DB_VAR).filter(|v| !v.is_empty()) {
        return PathBuf::from(v);
    }
    let cached =
        cache_dir_for(Host::current(), |n| std::env::var_os(n)).map(|c| c.join(PINNED_TAG));
    match cached {
        Some(d) if d.join("world.toml").is_file() => d,
        _ => panic!(
            "no world database: set {WORLD_DB_VAR} to a checkout, or fetch the pinned release ({PINNED_TAG}) with `yalain world fetch`"
        ),
    }
}

/// An editor on a new, empty world of the test's own, its camera over the town looking north. What
/// it borrows lives as long as the test program.
fn editor(name: &str) -> Editor<'static> {
    let dir = std::env::temp_dir().join(format!("athenaeum-editor-{name}-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    new::new_world(&dir, None).expect("a new world");
    let opened = world::open(&dir, Some(&database())).expect("the world opens over the database");
    let session = Session::open(opened, None).expect("the session");
    let dats = require_install(TARGET);
    let files = Box::leak(Box::new(
        build::InstallFiles::open(&dats).expect("the install"),
    ));
    let install = Box::leak(Box::new(files.install().expect("the script library")));
    let data = Box::leak(Box::new(WorldData::open(&dats).expect("the world's data")));
    let scene = Scene::new(data);
    let g = view::global(TOWN, [OPEN[0], OPEN[1] - 20.0, 210.0]);
    let camera = FlyCamera::new(Vec3::new(g[0], g[1], g[2]), 0.0, -0.5);
    let mut e = Editor::new(session, install, scene, camera, Programs::default());
    e.size = (1600, 900);
    e
}

/// A point of open ground in the town, on the world's grid.
fn ground(e: &mut Editor<'_>) -> [f32; 3] {
    let g = view::global(TOWN, [OPEN[0], OPEN[1], 0.0]);
    let z = e.scene.ground(g[0], g[1]).expect("the town has ground");
    [g[0], g[1], z]
}

/// One frame of the window's input over the world.
fn frame(e: &mut Editor<'_>, input: &ViewInput) {
    e.world_input(input, false, false, 0.016);
}

#[test]
fn closing_with_changes_asks_and_closing_without_closes() {
    let mut e = editor("closing");
    e.ask_close();
    assert!(e.close_now, "nothing to lose");
    e.close_now = false;
    let g = ground(&mut e);
    e.place_at(g, 0.0);
    e.ask_close();
    assert!(e.closing(), "the changes are asked after");
    assert!(!e.close_now);
    frame(&mut e, &ViewInput::default());
    assert!(!e.close_now, "and it stays open until answered");
}

/// The pixel a point of the world is seen at.
fn pixel(e: &Editor<'_>, g: [f32; 3]) -> [f32; 2] {
    e.view().project(g).expect("the camera sees it")
}

/// A click of the left button at a pixel: pressed one frame, let go the next.
fn click(e: &mut Editor<'_>, at: [f32; 2], shift: bool) {
    frame(
        e,
        &ViewInput {
            pointer: at,
            left_pressed: true,
            left_down: true,
            shift,
            ..ViewInput::default()
        },
    );
    frame(
        e,
        &ViewInput {
            pointer: at,
            left_released: true,
            shift,
            ..ViewInput::default()
        },
    );
}

fn key(e: &mut Editor<'_>, code: KeyCode, shift: bool) {
    frame(
        e,
        &ViewInput {
            pressed: vec![code],
            shift,
            ..ViewInput::default()
        },
    );
}

fn heading(e: &Editor<'_>, item: &str) -> f32 {
    match &e.session.entry(item).expect("the row").row.row {
        Row::Object(o) => o.heading.expect("a heading"),
        other => panic!("{other:?}"),
    }
}

#[test]
fn a_row_being_placed_stands_under_the_pointer_until_the_click() {
    let mut e = editor("ghost");
    let g = ground(&mut e);
    let at = pixel(&e, g);
    e.tool = Tool::Place;
    frame(
        &mut e,
        &ViewInput {
            pointer: at,
            ..ViewInput::default()
        },
    );
    let ghost = e.ghost().expect("the pointer is on the ground");
    assert!((ghost[0] - g[0]).abs() < 1.0 && (ghost[1] - g[1]).abs() < 1.0);
    let rows = e.session.entries().len();
    assert_eq!(e.session.steps().0, 0, "nothing placed yet");
    click(&mut e, at, false);
    assert_eq!(e.session.entries().len(), rows + 1);
    assert_eq!(e.tool, Tool::Select, "one click, one row");
    assert_eq!(e.ghost(), None);
}

#[test]
fn shift_keeps_placing_and_r_turns_what_is_placed() {
    let mut e = editor("repeat");
    let g = ground(&mut e);
    let at = pixel(&e, g);
    e.tool = Tool::Place;
    frame(
        &mut e,
        &ViewInput {
            pointer: at,
            ..ViewInput::default()
        },
    );
    let facing = (e.view().heading() + 180.0).rem_euclid(360.0);
    key(&mut e, KeyCode::KeyR, false);
    click(&mut e, at, true);
    assert_eq!(e.tool, Tool::Place, "Shift keeps the tool");
    let first = e.selected.clone().expect("the placed row");
    assert!((heading(&e, &first) - (facing + 15.0).rem_euclid(360.0)).abs() < 0.01);
    click(&mut e, [at[0] + 40.0, at[1]], false);
    let second = e.selected.clone().expect("the second row");
    assert_ne!(first, second);
    assert!(
        (heading(&e, &second) - heading(&e, &first)).abs() < 0.01,
        "the same way"
    );
    assert_eq!(e.tool, Tool::Select);
}

#[test]
fn escape_stops_placing_and_keeps_the_selection() {
    let mut e = editor("escape");
    let g = ground(&mut e);
    e.place_at(g, 0.0);
    let placed = e.selected.clone();
    e.tool = Tool::Place;
    key(&mut e, KeyCode::Escape, false);
    assert_eq!(e.tool, Tool::Select);
    assert_eq!(e.selected, placed);
    key(&mut e, KeyCode::Escape, false);
    assert_eq!(e.selected, None, "a second one lets the row go");
}

/// A drag of the left button from one pixel to another, in the three frames a window gives it.
fn drag(e: &mut Editor<'_>, from: [f32; 2], to: [f32; 2], shift: bool) {
    let held = |pointer| ViewInput {
        pointer,
        left_down: true,
        shift,
        ..ViewInput::default()
    };
    frame(
        e,
        &ViewInput {
            left_pressed: true,
            ..held(from)
        },
    );
    frame(e, &held(to));
    frame(
        e,
        &ViewInput {
            pointer: to,
            left_released: true,
            shift,
            ..ViewInput::default()
        },
    );
}

/// An editor with a row placed in the town and selected: the row's item and place.
fn with_a_row(name: &str) -> (Editor<'static>, String, [f32; 3]) {
    let mut e = editor(name);
    // Looking north-east, so the arrows lie apart on the screen.
    e.camera.yaw = -0.6;
    e.camera.position.x -= 20.0;
    let g = ground(&mut e);
    e.place_at(g, 0.0);
    frame(&mut e, &ViewInput::default());
    let item = e.selected.clone().expect("the placed row");
    (e, item, g)
}

fn place_of(e: &Editor<'_>, item: &str) -> [f32; 3] {
    let en = e.session.entry(item).expect("the row");
    let at = en.at();
    view::global(en.landblock(), [at.x, at.y, at.z.expect("a height")])
}

#[test]
fn the_up_arrow_lifts_a_row_and_leaves_it_where_it_stands() {
    let (mut e, item, g) = with_a_row("arrow-up");
    let h = gizmo::handles(&e.view(), g, true).unwrap();
    let tip = h.arrows.iter().find(|a| a.0 == Handle::Z).unwrap().1;
    let mid = [(h.origin[0] + tip[0]) / 2.0, (h.origin[1] + tip[1]) / 2.0];
    drag(&mut e, mid, [mid[0], mid[1] - 60.0], false);
    let now = place_of(&e, &item);
    assert!(now[2] > g[2] + 0.5, "{now:?} from {g:?}");
    assert!((now[0] - g[0]).abs() < 0.01 && (now[1] - g[1]).abs() < 0.01);
    assert_eq!(e.session.steps().0, 2, "the place and one move");
}

#[test]
fn the_east_arrow_snaps_to_the_grid_and_shift_lets_it_go_free() {
    let (mut e, item, g) = with_a_row("arrow-east");
    e.snap.on = true;
    e.snap.grid = 2.0;
    let h = gizmo::handles(&e.view(), g, true).unwrap();
    let tip = h.arrows.iter().find(|a| a.0 == Handle::X).unwrap().1;
    let mid = [(h.origin[0] + tip[0]) / 2.0, (h.origin[1] + tip[1]) / 2.0];
    drag(&mut e, mid, [mid[0] + 73.0, mid[1]], false);
    let snapped = place_of(&e, &item);
    assert!(snapped[0] > g[0], "{snapped:?}");
    assert_eq!(snapped[0] % 2.0, 0.0, "{snapped:?}");
    assert_eq!(snapped[1], g[1]);
    assert_eq!(snapped[2], g[2]);
    let h = gizmo::handles(&e.view(), snapped, true).unwrap();
    let tip = h.arrows.iter().find(|a| a.0 == Handle::X).unwrap().1;
    let mid = [(h.origin[0] + tip[0]) / 2.0, (h.origin[1] + tip[1]) / 2.0];
    drag(&mut e, mid, [mid[0] + 37.0, mid[1]], true);
    assert_ne!(place_of(&e, &item)[0] % 2.0, 0.0, "Shift goes free");
}

#[test]
fn the_ring_turns_a_row_once_when_it_is_let_go_and_escape_drops_it() {
    let (mut e, item, g) = with_a_row("ring");
    let h = gizmo::handles(&e.view(), g, true).unwrap();
    // Two points of the ring well clear of the arrows, the same way round from them.
    let clear = |p: &[f32; 2]| {
        h.arrows
            .iter()
            .map(|a| gizmo::segment_distance(*p, h.origin, a.1))
            .fold(f32::MAX, f32::min)
    };
    let far = (0..h.ring.len())
        .max_by(|a, b| clear(&h.ring[*a]).total_cmp(&clear(&h.ring[*b])))
        .unwrap();
    let on_ring = h.ring[far];
    let on_to = h.ring[(far + 12) % (h.ring.len() - 1)];
    let steps = e.session.steps().0;
    let held = |pointer| ViewInput {
        pointer,
        left_down: true,
        ..ViewInput::default()
    };
    frame(
        &mut e,
        &ViewInput {
            left_pressed: true,
            ..held(on_ring)
        },
    );
    frame(&mut e, &held(on_to));
    assert_eq!(e.session.steps().0, steps, "nothing until it is let go");
    frame(
        &mut e,
        &ViewInput {
            pressed: vec![KeyCode::Escape],
            left_down: true,
            ..held(on_to)
        },
    );
    frame(&mut e, &ViewInput::default());
    assert_eq!(e.session.steps().0, steps, "Escape dropped it");
    assert_eq!(
        e.selected.as_deref(),
        Some(item.as_str()),
        "and kept the row"
    );
    drag(&mut e, on_ring, on_to, false);
    assert_eq!(e.session.steps().0, steps + 1);
    assert!(
        (heading(&e, &item) - 0.0).abs() > 30.0,
        "{}",
        heading(&e, &item)
    );
}

#[test]
fn ctrl_d_copies_a_row_beside_it_and_selects_the_copy() {
    let (mut e, item, g) = with_a_row("duplicate");
    frame(
        &mut e,
        &ViewInput {
            pressed: vec![KeyCode::KeyD],
            ctrl: true,
            ..ViewInput::default()
        },
    );
    let copy = e.selected.clone().expect("the copy");
    assert_ne!(copy, item);
    let now = place_of(&e, &copy);
    let far = ((now[0] - g[0]).powi(2) + (now[1] - g[1]).powi(2)).sqrt();
    assert!(
        (far - athenaeum_editor::app::DUPLICATE_AWAY).abs() < 0.01,
        "{far}"
    );
    assert_eq!(place_of(&e, &item), g, "the first stays");
}

#[test]
fn find_goes_through_the_whole_world_with_the_worlds_own_first() {
    let (mut e, item, _) = with_a_row("find");
    let frank = e.rows_matching("frank trader", 10);
    assert!(!frank.is_empty(), "the database's Frank");
    let found = &e.session.entries()[frank[0]];
    assert_eq!(found.item, "object n-2ee9@5766", "far or near");
    let vendors = e.rows_matching("weapon vendor", 500);
    assert_eq!(
        e.session.entries()[vendors[0]].item,
        item,
        "this world's row comes first"
    );
    assert!(e.rows_matching("nothing holds this", 10).is_empty());
    let by_id = e.rows_matching("0x470033d0", 10);
    assert_eq!(
        e.session.entries()[by_id[0]].item,
        item,
        "by its template too"
    );
}
