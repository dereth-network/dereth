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
use athenaeum_foundry::edit::NewKind;
use athenaeum_foundry::{build, new, world, Session};
use auberean_dat::testing::{require_install, TARGET};
use dereth_builder::camera::FlyCamera;
use dereth_primitives::Vec3;
use winit::keyboard::KeyCode;
use yalain_world_db::release::{pinned_in_cache, PINNED_TAG};
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
    pinned_in_cache().unwrap_or_else(|why| {
        panic!(
            "no world database: {why}. Set {WORLD_DB_VAR} to a checkout, or fetch the pinned release ({PINNED_TAG}) with `yalain world fetch`"
        )
    })
}

/// An editor and the folder of its world, which is removed with everything in it when the editor
/// is dropped, even by a test that fails.
struct Opened {
    editor: Editor<'static>,
    dir: PathBuf,
}

impl std::ops::Deref for Opened {
    type Target = Editor<'static>;
    fn deref(&self) -> &Editor<'static> {
        &self.editor
    }
}

impl std::ops::DerefMut for Opened {
    fn deref_mut(&mut self) -> &mut Editor<'static> {
        &mut self.editor
    }
}

impl Drop for Opened {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.dir);
    }
}

/// An editor on a new, empty world of the test's own, its camera over the town looking north. What
/// it borrows lives as long as the test program.
fn editor(name: &str) -> Opened {
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
    Opened { editor: e, dir }
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

fn pixel_or_none(e: &Editor<'_>, g: [f32; 3]) -> Option<[f32; 2]> {
    e.view().project(g)
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
fn with_a_row(name: &str) -> (Opened, String, [f32; 3]) {
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

#[test]
fn a_row_an_undo_took_away_is_let_go() {
    let (mut e, _, _) = with_a_row("undo-selected");
    frame(
        &mut e,
        &ViewInput {
            pressed: vec![KeyCode::KeyZ],
            ctrl: true,
            ..ViewInput::default()
        },
    );
    frame(&mut e, &ViewInput::default());
    assert_eq!(e.selected, None);
}

/// A room of the shelter's hall, whose rooms run on past their block's own ground, and a point in it.
const HALL: u16 = 0x0298;
const IN_THE_HALL: [f32; 3] = [52.16, 238.5, -42.5];

fn in_the_hall(name: &str) -> Opened {
    let mut e = editor(name);
    let g = view::global(HALL, IN_THE_HALL);
    e.camera.position = Vec3::new(g[0], g[1], g[2]);
    e.camera.pitch = -0.5;
    assert!(e.enter_block(HALL, IN_THE_HALL), "the point is in a room");
    let _ = e.frame();
    e
}

#[test]
fn a_camera_among_a_dungeons_rooms_is_in_one_though_the_ground_under_it_is_another_blocks() {
    let e = in_the_hall("rooms");
    let (lb, low) = e.inside().expect("in a room");
    assert_eq!(lb, HALL);
    assert!(low >= 0x100, "{low:04X}");
    assert_eq!(
        view::local(view::global(HALL, IN_THE_HALL)).0,
        0x0299,
        "not the block under it"
    );
}

#[test]
fn a_row_placed_on_a_rooms_floor_names_the_room_and_its_block() {
    let mut e = in_the_hall("place-room");
    let px = [800.0, 700.0];
    e.tool = Tool::Place;
    frame(
        &mut e,
        &ViewInput {
            pointer: px,
            ..ViewInput::default()
        },
    );
    let ghost = e.ghost().expect("a floor under the pointer");
    assert!(ghost[2] < IN_THE_HALL[2], "below the camera: {ghost:?}");
    click(&mut e, px, false);
    let item = e.selected.clone().expect("the row");
    assert!(item.ends_with("@0298"), "{item}");
    let en = e.session.entry(&item).unwrap().clone();
    let Row::Object(o) = &en.row.row else {
        panic!("an object")
    };
    let cell = o.cell.expect("a room");
    assert_eq!(cell >> 16, 0x0298);
    assert!(cell & 0xFFFF >= 0x100, "{cell:08X}");
    assert!(
        e.session.problems().is_empty(),
        "{:?}",
        e.session.problems()
    );
}

#[test]
fn a_row_in_a_room_is_dragged_over_the_floor_and_keeps_its_block() {
    let mut e = in_the_hall("drag-room");
    let px = [800.0, 700.0];
    e.tool = Tool::Place;
    frame(
        &mut e,
        &ViewInput {
            pointer: px,
            ..ViewInput::default()
        },
    );
    click(&mut e, px, false);
    let item = e.selected.clone().expect("the row");
    let g = place_of(&e, &item);
    let on = pixel(&e, [g[0], g[1], g[2] + 1.0]);
    // Clear of its handles: the marker is a meter up, the drag ends on the floor nearer the camera.
    e.select(None);
    drag(&mut e, on, [on[0] + 60.0, on[1] + 60.0], false);
    let moved = e.selected.clone().expect("still the row");
    assert_eq!(moved, item, "the same block, so the same item");
    let now = place_of(&e, &item);
    assert!(
        ((now[0] - g[0]).powi(2) + (now[1] - g[1]).powi(2)).sqrt() > 0.1,
        "{now:?}"
    );
    assert!((now[2] - g[2]).abs() < 2.0, "still on that floor: {now:?}");
}

#[test]
fn another_blocks_rooms_rows_do_not_show_over_the_ground_they_fall_on() {
    let mut e = editor("other-rooms");
    let hall_row = e.rows_matching("shelter guard", 50);
    assert!(!hall_row.is_empty());
    // Over the block whose ground the hall's rows fall on, the camera is not among them.
    let g = view::global(0x0299, [52.0, 78.0, 40.0]);
    e.camera.position = Vec3::new(g[0], g[1], g[2]);
    frame(&mut e, &ViewInput::default());
    let at = pixel_or_none(&e, view::global(HALL, [52.16, 240.0, -43.5]));
    if let Some(at) = at {
        assert_eq!(e.hovered_item(at), None);
    }
}

#[test]
fn a_click_on_a_rows_body_selects_it_though_its_marker_is_elsewhere() {
    let (mut e, item, g) = with_a_row("body");
    e.select(None);
    // The person's head, well above the marker a meter off the ground, and clear of its pixels.
    let head = pixel(&e, [g[0], g[1], g[2] + 1.75]);
    let marker = pixel(&e, [g[0], g[1], g[2] + 1.0]);
    assert!(
        (head[1] - marker[1]).abs() > athenaeum_editor::app::HIT,
        "{head:?} {marker:?}"
    );
    click(&mut e, head, false);
    assert_eq!(e.selected.as_deref(), Some(item.as_str()));
    e.select(None);
    click(&mut e, [head[0] + 300.0, head[1] - 200.0], false);
    assert_eq!(e.selected, None, "clear of it, nothing");
}

/// Two rows in the town, selected together, the second leading.
fn with_two_rows(name: &str) -> (Opened, String, String, [f32; 3], [f32; 3]) {
    let (mut e, first, g) = with_a_row(name);
    let h = view::global(TOWN, [OPEN[0] + 4.0, OPEN[1], 0.0]);
    let h = [h[0], h[1], e.scene.ground(h[0], h[1]).expect("ground")];
    e.place_at(h, 0.0);
    frame(&mut e, &ViewInput::default());
    let second = e.selected.clone().expect("the second row");
    let at = pixel(&e, [g[0], g[1], g[2] + 1.0]);
    click(&mut e, at, true);
    assert_eq!(
        e.selected.as_deref(),
        Some(first.as_str()),
        "the last added leads"
    );
    assert_eq!(e.also, vec![second.clone()]);
    (e, first, second, g, h)
}

#[test]
fn a_selection_moves_together_and_one_undo_takes_it_back() {
    let (mut e, first, second, g, h) = with_two_rows("group-move");
    let handles = gizmo::handles(&e.view(), g, true).unwrap();
    let tip = handles.arrows.iter().find(|a| a.0 == Handle::X).unwrap().1;
    let mid = [
        (handles.origin[0] + tip[0]) / 2.0,
        (handles.origin[1] + tip[1]) / 2.0,
    ];
    let steps = e.session.steps().0;
    drag(&mut e, mid, [mid[0] + 50.0, mid[1]], false);
    assert_eq!(e.session.steps().0, steps + 1, "one step");
    let (a, b) = (place_of(&e, &first), place_of(&e, &second));
    assert!(a[0] > g[0] + 0.5, "{a:?}");
    assert!(
        ((b[0] - a[0]) - (h[0] - g[0])).abs() < 0.01,
        "they keep their distance"
    );
    frame(
        &mut e,
        &ViewInput {
            pressed: vec![KeyCode::KeyZ],
            ctrl: true,
            ..ViewInput::default()
        },
    );
    assert_eq!(place_of(&e, &first), g);
    assert_eq!(place_of(&e, &second), h);
}

#[test]
fn a_selection_turns_about_the_row_that_leads() {
    let (mut e, first, second, g, h) = with_two_rows("group-turn");
    e.snap.on = true;
    e.snap.angle = 90.0;
    key(&mut e, KeyCode::KeyR, false);
    assert!((heading(&e, &first) - 90.0).abs() < 0.01);
    assert!((heading(&e, &second) - 90.0).abs() < 0.01);
    assert_eq!(place_of(&e, &first), g, "the leader stays");
    let b = place_of(&e, &second);
    // East of the leader, a quarter turn clockwise puts it south.
    assert!((b[0] - g[0]).abs() < 0.01, "{b:?}");
    assert!((b[1] - (g[1] - (h[0] - g[0]))).abs() < 0.01, "{b:?}");
}

#[test]
fn a_selection_copies_and_goes_together() {
    let (mut e, first, second, _, _) = with_two_rows("group-copy");
    let rows = e.session.entries().len();
    frame(
        &mut e,
        &ViewInput {
            pressed: vec![KeyCode::KeyD],
            ctrl: true,
            ..ViewInput::default()
        },
    );
    assert_eq!(e.session.entries().len(), rows + 2);
    assert_eq!(e.also.len(), 1, "the copies are selected");
    assert_ne!(e.selected.as_deref(), Some(first.as_str()));
    let steps = e.session.steps().0;
    key(&mut e, KeyCode::Delete, false);
    assert_eq!(e.session.entries().len(), rows);
    assert_eq!(e.session.steps().0, steps + 1, "one step");
    assert!(e.session.entry(&first).is_some() && e.session.entry(&second).is_some());
    assert_eq!(e.selected, None);
}

#[test]
fn shift_click_takes_a_selected_row_out_and_a_plain_click_lets_the_rest_go() {
    let (mut e, first, second, g, h) = with_two_rows("group-toggle");
    let on = |e: &Editor<'_>, p: [f32; 3]| pixel(e, [p[0], p[1], p[2] + 1.0]);
    let at = on(&e, h);
    click(&mut e, at, true);
    assert_eq!(e.selected.as_deref(), Some(first.as_str()));
    assert!(e.also.is_empty(), "{:?}", e.also);
    let at = on(&e, h);
    click(&mut e, at, false);
    assert_eq!(e.selected.as_deref(), Some(second.as_str()));
    let at = on(&e, g);
    click(&mut e, at, false);
    assert_eq!(e.selected.as_deref(), Some(first.as_str()));
    assert!(e.also.is_empty());
}

#[test]
fn a_portal_leads_to_this_worlds_place_and_it_builds_clean() {
    let (mut e, object, g) = with_a_row("link");
    e.new_kind = NewKind::Place;
    e.place_at([g[0] + 5.0, g[1], g[2]], 0.0);
    let places = e.places_to_lead_to(&object, 10);
    let (to, _) = places.first().expect("a place").clone();
    assert_eq!(to, "place-1", "this world's own comes first");
    assert!(
        places.iter().skip(1).any(|(t, _)| t.contains(':')),
        "the database's, by their pack: {places:?}"
    );
    e.lead_to(&object, &to);
    assert!(
        e.session.problems().is_empty(),
        "{:?}",
        e.session.problems()
    );
    let Row::Object(o) = &e.session.entry(&object).unwrap().row.row else {
        panic!("an object")
    };
    assert_eq!(o.to.as_deref(), Some("place-1"));
    let (other, _) = places
        .iter()
        .find(|(t, _)| t.contains(':'))
        .unwrap()
        .clone();
    e.lead_to(&object, &other);
    assert!(
        e.session.problems().is_empty(),
        "{:?}",
        e.session.problems()
    );
}

#[test]
fn a_camps_creatures_change_by_template_and_weight() {
    let (mut e, _, g) = with_a_row("spawns");
    e.new_kind = NewKind::Camp;
    e.place_at([g[0] + 5.0, g[1], g[2]], 0.0);
    let camp = e.selected.clone().expect("the camp");
    let first = 0x4700_33D0;
    e.set_spawns(&camp, &[(first, 3), (0x4700_100C, 1)]);
    assert!(
        e.session.problems().is_empty(),
        "{:?}",
        e.session.problems()
    );
    let Row::Camp(c) = &e.session.entry(&camp).unwrap().row.row else {
        panic!("a camp")
    };
    assert_eq!(c.spawns, vec![(first, 3), (0x4700_100C, 1)]);
}

#[test]
fn a_row_of_a_room_keeps_its_block_and_room_when_its_numbers_change_from_outdoors() {
    let mut e = in_the_hall("numbers");
    let px = [800.0, 700.0];
    e.tool = Tool::Place;
    frame(
        &mut e,
        &ViewInput {
            pointer: px,
            ..ViewInput::default()
        },
    );
    click(&mut e, px, false);
    let item = e.selected.clone().expect("the row");
    let Row::Object(o) = &e.session.entry(&item).unwrap().row.row else {
        panic!("an object")
    };
    let room = o.cell.expect("a room");
    let at = e.session.entry(&item).unwrap().at();
    // Out of the rooms, as the inspector's numbers would write it.
    e.go_to(TOWN, OPEN);
    frame(&mut e, &ViewInput::default());
    assert_eq!(e.inside(), None);
    e.session
        .move_to(&item, HALL, [at.x + 1.0, at.y, at.z.unwrap()])
        .unwrap();
    let Row::Object(o) = &e.session.entry(&item).unwrap().row.row else {
        panic!("an object")
    };
    assert_eq!(o.cell, Some(room));
    assert_eq!(e.session.entry(&item).unwrap().landblock(), HALL);
}

#[test]
fn under_the_camera_in_a_room_is_its_floor() {
    let mut e = in_the_hall("under");
    let g = e.under_camera();
    assert!(
        g[2] < IN_THE_HALL[2] && g[2] > IN_THE_HALL[2] - 5.0,
        "{g:?}"
    );
}

/// A point of the town whose stones or boards stand over the ground.
fn on_something_built(e: &mut Editor<'_>) -> [f32; 3] {
    for x in (2..158).step_by(4) {
        for y in (2..158).step_by(4) {
            #[allow(clippy::cast_precision_loss)]
            let g = view::global(TOWN, [x as f32, y as f32, 0.0]);
            let z = e.scene.ground(g[0], g[1]).expect("the town has ground");
            let over = e.surface_under([g[0], g[1], z + 3.0]);
            if over[2] > z + 0.3 && over[2] < z + 3.0 {
                return over;
            }
        }
    }
    panic!("nothing built to stand on in the town");
}

#[test]
fn placing_lands_on_what_the_world_has_built_and_not_the_ground_under_it() {
    let mut e = editor("platform");
    let top = on_something_built(&mut e);
    let under = e.scene.ground(top[0], top[1]).unwrap();
    assert!(top[2] > under + 0.3, "{top:?} over {under}");
    // Looking down at it from just over it, under whatever else stands higher there.
    e.camera.position = Vec3::new(top[0], top[1] - 0.2, top[2] + 1.5);
    e.camera.pitch = -1.4;
    let px = pixel(&e, top);
    e.tool = Tool::Place;
    frame(
        &mut e,
        &ViewInput {
            pointer: px,
            ..ViewInput::default()
        },
    );
    let ghost = e.ghost().expect("something under the pointer");
    assert!(
        (ghost[2] - top[2]).abs() < 0.25,
        "{ghost:?} against {top:?}"
    );
    click(&mut e, px, false);
    let item = e.selected.clone().expect("the row");
    assert!((place_of(&e, &item)[2] - top[2]).abs() < 0.25);
}

#[test]
fn a_look_at_a_wall_takes_what_is_at_its_foot_and_open_ground_stays_the_ground() {
    let mut e = editor("wall-foot");
    let g = ground(&mut e);
    let at = e.surface_under([g[0], g[1], g[2] + 1.0]);
    assert!(
        (at[2] - g[2]).abs() < 0.01,
        "open ground: {at:?} against {g:?}"
    );
}

#[test]
fn a_drag_with_snap_on_stays_on_the_platform() {
    let mut e = editor("platform-snap");
    let top = on_something_built(&mut e);
    e.snap.on = true;
    e.snap.grid = 0.25;
    let snapped = e.surface_under([
        (top[0] / 0.25).round() * 0.25,
        (top[1] / 0.25).round() * 0.25,
        top[2],
    ]);
    assert!(
        (snapped[2] - top[2]).abs() < 0.25,
        "{snapped:?} against {top:?}"
    );
}

#[test]
fn a_row_lands_on_a_placed_objects_top_and_a_person_is_not_stood_on() {
    let (mut e, person, g) = with_a_row("on-a-box");
    // A person is not something to stand on.
    e.select(None);
    frame(&mut e, &ViewInput::default());
    let over = e.surface_under([g[0], g[1], g[2] + 1.5]);
    assert!((over[2] - g[2]).abs() < 0.01, "{over:?}");
    assert!(e.session.entry(&person).is_some());
    // A workshop placed beside it is.
    let h = view::global(TOWN, [OPEN[0] + 6.0, OPEN[1], 0.0]);
    let h = [h[0], h[1], e.scene.ground(h[0], h[1]).expect("ground")];
    e.new_kind = NewKind::Static;
    e.template = "0x47000CD2".into();
    e.place_at(h, 0.0);
    frame(&mut e, &ViewInput::default());
    e.select(None);
    frame(&mut e, &ViewInput::default());
    let top = e.surface_under([h[0], h[1], h[2] + 30.0]);
    assert!(top[2] > h[2] + 0.2, "{top:?} over {h:?}");
    // And what it is selected, it does not stand on itself.
    let item = e
        .session
        .entries()
        .iter()
        .rev()
        .find(|x| x.item.starts_with("object static-"))
        .map(|x| x.item.clone())
        .expect("the workshop");
    e.select(Some(item));
    let under = e.surface_under([h[0], h[1], h[2] + 30.0]);
    assert!(under[2] < top[2], "{under:?} under {top:?}");
}

#[test]
fn keys_scale_the_selection_as_one_step_and_it_is_drawn_and_clicked_so() {
    let (mut e, a, b, g, _) = with_two_rows("scale");
    let steps = e.session.steps().0;
    key(&mut e, KeyCode::Equal, false);
    assert_eq!(e.session.steps().0, steps + 1, "one step");
    let scale = |e: &Editor<'_>, i: &str| match &e.session.entry(i).unwrap().row.row {
        Row::Object(o) => o.scale.expect("a scale"),
        other => panic!("{other:?}"),
    };
    assert!((scale(&e, &a) - athenaeum_editor::app::SCALE_STEP).abs() < 0.01);
    assert!((scale(&e, &b) - athenaeum_editor::app::SCALE_STEP).abs() < 0.01);
    for _ in 0..12 {
        key(&mut e, KeyCode::Equal, false);
    }
    assert!(
        e.session.problems().is_empty(),
        "{:?}",
        e.session.problems()
    );
    frame(&mut e, &ViewInput::default());
    e.select(None);
    // Far over the head it stood at before, the body now grown under the pointer.
    let high = pixel(&e, [g[0], g[1], g[2] + 3.5]);
    click(&mut e, high, false);
    assert_eq!(e.selected.as_deref(), Some(a.as_str()));
}

#[test]
fn the_finder_goes_by_name_or_id_and_alone_leaves_out_what_shows_nothing() {
    let mut e = editor("finder");
    let (vendor, bare) = (0x4700_33D0, 0x4700_000C);
    assert!(e.look(vendor).is_some());
    assert!(e.look(bare).is_none());
    e.set_templates(vec![bare, vendor]);
    assert_eq!(&*e.templates_matching("Weapon VENDOR"), [vendor]);
    assert_eq!(&*e.templates_matching("0x470033d0"), [vendor]);
    assert_eq!(&*e.templates_matching(" "), [vendor]);
    assert_eq!(&*e.templates_matching("0x4700000c"), [bare]);
    assert!(e
        .templates_matching("no template is called this")
        .is_empty());
}

#[test]
fn a_looks_picture_shows_its_body_and_one_with_no_body_has_none() {
    let mut e = editor("picture");
    let (look, _) = e.look(0x4700_33D0).expect("a look");
    let frame = e.scene.picture_frame(look).expect("a picture");
    assert!(!frame.objects.is_empty());
    assert!(e.scene.picture_frame(0x02FF_FFFF).is_none());
}

fn ctrl_key(e: &mut Editor<'_>, code: KeyCode, pointer: [f32; 2]) {
    frame(
        e,
        &ViewInput {
            pressed: vec![code],
            ctrl: true,
            pointer,
            ..ViewInput::default()
        },
    );
}

#[test]
fn g_stands_every_row_selected_on_the_ground_as_one_step() {
    let (mut e, a, b, g, h) = with_two_rows("ground");
    let raised = |p: [f32; 3], up: f32| [OPEN[0] + (p[0] - g[0]), OPEN[1], p[2] + up];
    e.session.move_to(&a, TOWN, raised(g, 6.0)).unwrap();
    e.session.move_to(&b, TOWN, raised(h, 3.0)).unwrap();
    let steps = e.session.steps().0;
    key(&mut e, KeyCode::KeyG, false);
    assert_eq!(e.session.steps().0, steps + 1, "one step");
    // The town's boards stand a little over the ground here, and they are what it stands on.
    for (item, p) in [(&a, g), (&b, h)] {
        let z = place_of(&e, item)[2];
        assert!(
            z >= p[2] - 0.05 && z < p[2] + 1.0,
            "{item}: {z} over {}",
            p[2]
        );
    }
    assert_eq!(e.selected.as_deref(), Some(a.as_str()), "still selected");
    assert_eq!(e.also, vec![b.clone()], "still selected");
}

#[test]
fn r_with_snap_on_lands_on_a_whole_step() {
    let (mut e, item, _) = with_a_row("snap-turn");
    e.session.turn(&item, 7.0).unwrap();
    e.snap.on = true;
    key(&mut e, KeyCode::KeyR, false);
    assert!(
        (heading(&e, &item) - 15.0).abs() < 0.01,
        "{}",
        heading(&e, &item)
    );
    key(&mut e, KeyCode::KeyR, false);
    assert!(
        (heading(&e, &item) - 30.0).abs() < 0.01,
        "{}",
        heading(&e, &item)
    );
    key(&mut e, KeyCode::KeyR, true);
    assert!(
        (heading(&e, &item) - 15.0).abs() < 0.01,
        "{}",
        heading(&e, &item)
    );
}

#[test]
fn rows_copied_go_down_in_another_landblock_standing_as_they_stood() {
    let (mut e, a, b, g, h) = with_two_rows("paste");
    ctrl_key(&mut e, KeyCode::KeyC, [0.0, 0.0]);
    // Over the next block, looking down at open ground.
    let to = [g[0] + view::BLOCK, g[1], 0.0];
    let z = e.scene.ground(to[0], to[1]).expect("ground there");
    e.paste_at([to[0], to[1], z]);
    let lead = e.selected.clone().expect("a copy selected");
    let other = e.also.first().cloned().expect("the other copy selected");
    assert!(
        lead != a && lead != b && other != a && other != b,
        "{lead} {other}"
    );
    let block = |e: &Editor<'_>, i: &str| e.session.entry(i).expect("the row").landblock();
    assert_ne!(block(&e, &lead), TOWN, "in the other block");
    assert_eq!(block(&e, &a), TOWN, "the rows copied stay");
    assert_eq!(block(&e, &b), TOWN, "the rows copied stay");
    let (l, o) = (place_of(&e, &lead), place_of(&e, &other));
    assert!((l[0] - to[0]).abs() < 0.01, "the lead at the point: {l:?}");
    assert!(((o[0] - l[0]) - (h[0] - g[0])).abs() < 0.01, "{l:?} {o:?}");
    assert!(((o[1] - l[1]) - (h[1] - g[1])).abs() < 0.01, "{l:?} {o:?}");
    let steps = e.session.steps().0;
    e.session.undo();
    assert_eq!(e.session.steps().0, steps - 1);
    assert!(e.session.entry(&lead).is_none(), "one undo takes both");
    assert!(e.session.entry(&other).is_none(), "one undo takes both");
}

#[test]
fn what_can_be_worn_is_pictured_on_a_body_and_differs_from_the_bare_body() {
    let mut e = editor("worn");
    let (boots, vendor) = (0x4700_07D0, 0x4700_33D0);
    assert!(e.worn_look(vendor).is_none(), "a person is not worn");
    let worn = e.worn_look(boots).expect("boots are worn");
    assert!(worn.keys.contains(&(0x10, 1.0)), "shown: {worn:?}");
    let body = 0x1F00_0023;
    let vertices = |f: &auberean_draw::scene::Frame| -> usize {
        f.objects.iter().map(|o| o.vertices.len()).sum()
    };
    let bare = e.scene.picture_frame(body).expect("the body");
    let dressed = e
        .scene
        .worn_frame(body, worn.table, &worn.keys)
        .expect("dressed");
    assert_ne!(
        vertices(&bare),
        vertices(&dressed),
        "the boots change the body"
    );
}

#[test]
fn a_template_chosen_is_placed_as_what_it_is() {
    let mut e = editor("kinds");
    for (t, person) in [
        (0x4700_33D0, true),  // a vendor
        (0x4700_23B3, true),  // a creature
        (0x4700_1A58, false), // a building
        (0x4700_26C4, false), // a barrel
        (0x4700_3432, false), // a portal
        (0x4700_33A5, false), // a portal of effects alone
    ] {
        assert_eq!(e.is_person(t), person, "{t:08X}");
    }
    e.choose_template(0x4700_1A58);
    assert_eq!(e.new_kind, NewKind::Static);
    assert_eq!(e.tool, Tool::Place);
    e.choose_template(0x4700_33D0);
    assert_eq!(e.new_kind, NewKind::Person);
    e.new_kind = NewKind::Camp;
    e.choose_template(0x4700_23B3);
    assert_eq!(e.new_kind, NewKind::Camp, "a creature for the camp");
    e.choose_template(0x4700_1A58);
    assert_eq!(e.new_kind, NewKind::Static, "a camp keeps no building");
}

#[test]
fn the_finder_leaves_out_things_to_carry_until_asked() {
    let mut e = editor("items");
    let (gem, bore, boots) = (0x4700_0803, 0x4700_3432, 0x4700_07D0);
    e.set_templates(vec![boots, gem, bore]);
    assert_eq!(&*e.templates_matching("portal"), [bore]);
    assert_eq!(&*e.templates_matching(""), [bore]);
    assert_eq!(&*e.templates_matching("0x47000803"), [gem], "by its id");
    e.show_items = true;
    assert_eq!(&*e.templates_matching("portal"), [gem, bore]);
    e.show_items = false;
    e.show_worn = true;
    assert_eq!(&*e.templates_matching("boots"), [boots]);
}

#[test]
fn a_row_of_effects_alone_draws_them_and_stops_when_it_is_gone() {
    let mut e = editor("row-effects");
    let (look, _) = e.look(0x4700_33A5).expect("a portal of effects alone");
    let g = ground(&mut e);
    let v = e.view();
    let row = athenaeum_editor::scene::Standing {
        look,
        at: [g[0], g[1], g[2]],
        rotation: [1.0, 0.0, 0.0, 0.0],
        scale: 1.0,
    };
    let bare = e.scene.frame(&v, &[], None).objects.len();
    let _ = e.scene.frame(&v, &[row], None);
    let with = e.scene.frame(&v, &[row], None).objects.len();
    assert!(with > bare, "{with} against {bare}");
    let _ = e.scene.frame(&v, &[], None);
    assert_eq!(
        e.scene.frame(&v, &[], None).objects.len(),
        bare,
        "gone with it"
    );
}

#[test]
fn find_rows_goes_to_a_room_by_its_id() {
    let mut e = editor("room-go");
    assert_eq!(
        athenaeum_editor::app::room_of("03BB022C"),
        Some(0x03BB_022C)
    );
    assert_eq!(
        athenaeum_editor::app::room_of("0x03BB022C"),
        Some(0x03BB_022C)
    );
    assert_eq!(athenaeum_editor::app::room_of("5766"), None);
    assert_eq!(
        athenaeum_editor::app::room_of("57660023"),
        None,
        "land, not a room"
    );
    e.go_to_room(0x03BB_022C);
    frame(&mut e, &ViewInput::default());
    assert!(
        e.status.is_empty() || !e.status.starts_with("no room"),
        "{}",
        e.status
    );
    assert_eq!(
        e.inside().map(|(lb, _)| lb),
        Some(0x03BB),
        "among its rooms"
    );
}

#[test]
fn a_row_selected_is_drawn_however_far_off_it_stands() {
    let (mut e, item, _) = with_a_row("far");
    e.camera.position.x += 400.0;
    frame(&mut e, &ViewInput::default());
    let selected = e.frame().objects.len();
    e.select(None);
    frame(&mut e, &ViewInput::default());
    let not = e.frame().objects.len();
    assert!(selected > not, "{item}: {selected} against {not}");
}

#[test]
fn the_map_shows_the_world_and_marks_this_worlds_rows() {
    let (mut e, item, g) = with_a_row("map");
    let map = e.scene.map_picture();
    let side = athenaeum_editor::map::SIDE;
    assert_eq!(map.len(), side * side * 4);
    let pixel = |p: [f32; 2]| {
        #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
        let (x, y) = (p[0] as usize, p[1] as usize);
        let at = (y * side + x) * 4;
        [map[at], map[at + 1], map[at + 2]]
    };
    // The town stands on land, not in the sea.
    let town = pixel(athenaeum_editor::map::pixel_of(g[0], g[1]));
    assert_ne!(town, [0, 0, 0], "the town's block is held");
    assert!(
        i32::from(town[2]) - i32::from(town[0]) < 60,
        "not water: {town:?}"
    );
    let marks = e.map_marks();
    assert_eq!(marks.len(), 1, "this world's row alone: {marks:?}");
    assert_eq!(marks[0].2, TOWN, "{item}");
    let (lb, at) = athenaeum_editor::map::place_at(marks[0].0).expect("on the map");
    assert_eq!(lb, TOWN);
    e.go_to(lb, at);
    assert!((e.view().eye[0] - g[0]).abs() < 1.0, "a click goes there");
}

/// The camera high over a spot where the land grows plenty, looking down.
fn over_trees(e: &mut Editor<'_>) -> [f32; 3] {
    let g = view::global(0xB590, [75.0, 45.0, 0.0]);
    let z = e.scene.ground(g[0], g[1]).expect("ground");
    e.camera.position = Vec3::new(g[0], g[1] - 5.0, z + 60.0);
    e.camera.pitch = -1.45;
    e.camera.yaw = 0.0;
    e.size = (1600, 900);
    [g[0], g[1], z]
}

/// A clearing drawn on the ground by dragging is a row, and what the land grows goes from the
/// editor's own view at once; undone, it comes back. A rectangle is drawn corner to corner.
#[test]
fn a_clearing_drawn_on_the_ground_clears_the_view_and_undo_brings_it_back() {
    let mut e = editor("clearing");
    let g = over_trees(&mut e);
    let grown = |e: &mut Editor<'_>| e.frame().counts.scenery;
    let before = grown(&mut e);
    assert!(before > 0, "the land grows here");

    e.new_kind = NewKind::Clearing;
    e.tool = Tool::Place;
    let from = pixel(&e, g);
    let to = pixel(&e, [g[0] + 30.0, g[1], g[2]]);
    frame(
        &mut e,
        &ViewInput {
            pointer: from,
            ..ViewInput::default()
        },
    );
    drag(&mut e, from, to, false);
    assert_eq!(e.tool, Tool::Select, "drawn, the tool is put down");
    let item = e.selected.clone().expect("the new row is selected");
    assert!(item.starts_with("clearing clearing-1@"), "{item}");
    let Row::Clearing(c) = &e.session.entry(&item).expect("the row").row.row else {
        panic!("a clearing");
    };
    let radius = c.radius.expect("round");
    assert!((radius - 30.0).abs() < 8.0, "dragged about 30 m: {radius}");
    let cleared = grown(&mut e);
    assert!(cleared < before, "{cleared} of {before}");

    e.session.undo();
    assert_eq!(grown(&mut e), before, "undone, it all grows again");

    // A rectangle, corner to corner, its middle where the drag's is.
    e.new_kind = NewKind::Clearing;
    e.clearing_rectangle = true;
    e.tool = Tool::Place;
    let corner = pixel(&e, [g[0] - 20.0, g[1] - 10.0, g[2]]);
    let other = pixel(&e, [g[0] + 20.0, g[1] + 10.0, g[2]]);
    frame(
        &mut e,
        &ViewInput {
            pointer: corner,
            ..ViewInput::default()
        },
    );
    drag(&mut e, corner, other, false);
    let item = e.selected.clone().expect("the new row is selected");
    let Row::Clearing(c) = &e.session.entry(&item).expect("the row").row.row else {
        panic!("a clearing");
    };
    let [x, y] = c.size.expect("a rectangle");
    assert!((x - 40.0).abs() < 8.0 && (y - 20.0).abs() < 8.0, "{x} {y}");
    assert_eq!(c.heading, Some(0.0));
    assert!(grown(&mut e) < before);
}

/// Escape while drawing drops the drawing and keeps the tool in hand.
#[test]
fn escape_drops_a_clearing_being_drawn_and_keeps_the_tool() {
    let mut e = editor("clearing-escape");
    let g = over_trees(&mut e);
    e.new_kind = NewKind::Clearing;
    e.tool = Tool::Place;
    let from = pixel(&e, g);
    let down = ViewInput {
        pointer: from,
        left_pressed: true,
        left_down: true,
        ..ViewInput::default()
    };
    frame(
        &mut e,
        &ViewInput {
            pointer: from,
            ..ViewInput::default()
        },
    );
    frame(&mut e, &down);
    key(&mut e, KeyCode::Escape, false);
    assert_eq!(e.tool, Tool::Place);
    frame(
        &mut e,
        &ViewInput {
            pointer: from,
            left_released: true,
            ..ViewInput::default()
        },
    );
    assert!(e.session.clearings().is_empty());
}

#[test]
fn a_file_written_by_hand_is_taken_in_and_one_changed_here_as_well_is_asked_about() {
    let mut e = editor("by-hand");
    e.look_at_disk();
    let cell = yalain_world_db::compile::outdoor_cell(TOWN, OPEN[0], OPEN[1]);
    let place = |key: &str| {
        format!(
            "[[place]]\nkey = \"{key}\"\ncell = 0x{cell:08X}\nat = [{}, {}, 0.0]\nheading = 0.0\nprov = {{ from = \"authored\" }}\n",
            OPEN[0], OPEN[1]
        )
    };
    let rel = format!("{}/server/places.toml", e.session.target());
    let path = e.session.dir().join(&rel);
    std::fs::create_dir_all(path.parent().unwrap()).unwrap();
    std::fs::write(&path, place("by-hand")).unwrap();
    e.look_at_disk();
    assert!(e.session.entry("place by-hand").is_some(), "{}", e.status);
    assert!(!e.session.unsaved());
    // Changed here and then on disk: kept as it is here, and asked about.
    e.session.turn("place by-hand", 90.0).unwrap();
    std::fs::write(&path, place("written-again")).unwrap();
    e.look_at_disk();
    assert!(e.session.clashes().contains(&rel), "{}", e.status);
    assert!(e.session.entry("place by-hand").is_some());
    e.save();
    assert!(
        std::fs::read_to_string(&path)
            .unwrap()
            .contains("written-again"),
        "the disk's text is not saved over"
    );
}

#[test]
fn a_problem_opens_its_row_or_else_its_file_at_its_line() {
    let mut e = editor("open-problem");
    e.look_at_disk();
    // A quest step that leads nowhere, written by hand: its problem opens the quest's file.
    let rel = format!("{}/server/quests/40000088.toml", e.session.target());
    let path = e.session.dir().join(&rel);
    std::fs::create_dir_all(path.parent().unwrap()).unwrap();
    std::fs::write(
        &path,
        "[[quest]]\nid = 0x40000088\nid_name = \"QuestTest\"\nprov = { from = \"authored\" }\n\n\
         [[step]]\nkey = \"s1\"\nquest = 0x40000088\nneeds = 0\ntrigger = { use = 0x470020BB, on = 0x4700115B }\n\
         update = \"bestow\"\nto = \"nowhere\"\nprov = { from = \"authored\" }\n",
    )
    .unwrap();
    e.look_at_disk();
    let p = e
        .session
        .problems()
        .iter()
        .find(|p| p.file == rel)
        .cloned()
        .unwrap_or_else(|| panic!("a problem in the quest: {:?}", e.session.problems()));
    e.open_problem(&p);
    assert_eq!(e.viewing(), Some((rel.as_str(), p.line)));
    assert_eq!(e.selected, None);
    // A portal that leads nowhere: its problem selects it.
    let g = ground(&mut e);
    e.place_at(g, 0.0);
    let portal = e.selected.clone().expect("the placed row");
    e.session.set(&portal, "to", "\"nowhere-else\"").unwrap();
    e.select(None);
    let p = e
        .session
        .problems()
        .iter()
        .find(|p| p.message.contains("nowhere-else"))
        .cloned()
        .unwrap_or_else(|| panic!("a problem with the portal: {:?}", e.session.problems()));
    e.open_problem(&p);
    assert_eq!(e.selected.as_deref(), Some(portal.as_str()), "{p}");
}

/// A camp's spots as its row has them: each one's cell and point.
fn spots_of(e: &Editor<'_>, item: &str) -> Vec<(u32, [f32; 3])> {
    match &e.session.entry(item).expect("the camp").row.row {
        Row::Camp(c) => c
            .spots
            .iter()
            .map(|(cell, at)| (*cell, [at.x, at.y, at.z.expect("a height")]))
            .collect(),
        other => panic!("{other:?}"),
    }
}

#[test]
fn an_indoor_camp_has_its_spots_added_and_dragged_over_the_floor() {
    let mut e = in_the_hall("spots");
    e.new_kind = NewKind::Camp;
    e.tool = Tool::Place;
    let px = [800.0, 700.0];
    frame(
        &mut e,
        &ViewInput {
            pointer: px,
            ..ViewInput::default()
        },
    );
    click(&mut e, px, false);
    let camp = e.selected.clone().expect("the camp");
    // Placed in a room, it has one spot, where it was put.
    let first = spots_of(&e, &camp);
    assert_eq!(first.len(), 1);
    let (room, at) = first[0];
    assert_eq!(room >> 16, u32::from(HALL));
    assert!(room & 0xFFFF >= 0x100, "{room:08X}");
    let placed = e.session.entry(&camp).unwrap().at();
    assert_eq!([at[0], at[1]], [placed.x, placed.y]);
    // The spot tool adds another where the floor is clicked.
    e.tool = Tool::Spot;
    let px = [740.0, 720.0];
    frame(
        &mut e,
        &ViewInput {
            pointer: px,
            ..ViewInput::default()
        },
    );
    assert!(e.ghost().is_some(), "a floor under the pointer");
    click(&mut e, px, false);
    assert_eq!(e.tool, Tool::Select, "one click, one spot");
    let two = spots_of(&e, &camp);
    assert_eq!(two.len(), 2, "{}", e.status);
    // Its marker drags it over the floor, into a room of the hall's.
    let (_, s) = two[1];
    let g = view::global(HALL, [s[0], s[1], s[2] + athenaeum_editor::app::SPOT_UP]);
    let on = pixel(&e, g);
    drag(&mut e, on, [on[0] + 50.0, on[1] + 30.0], false);
    assert_eq!(
        e.selected.as_deref(),
        Some(camp.as_str()),
        "the camp stays selected"
    );
    let now = spots_of(&e, &camp);
    assert_eq!(now.len(), 2);
    assert_eq!(now[0], two[0], "the other spot stays");
    let (cell, moved) = now[1];
    assert!(
        ((moved[0] - s[0]).powi(2) + (moved[1] - s[1]).powi(2)).sqrt() > 0.1,
        "{moved:?}"
    );
    assert!(
        (moved[2] - s[2]).abs() < 2.0,
        "still on that floor: {moved:?}"
    );
    assert_eq!(cell >> 16, u32::from(HALL));
    assert!(cell & 0xFFFF >= 0x100, "{cell:08X}");
    // Dragged by its marker, the camp takes its spots along.
    let c = place_of(&e, &camp);
    e.select(None);
    let on = pixel(&e, [c[0], c[1], c[2] + 1.0]);
    drag(&mut e, on, [on[0] + 60.0, on[1] + 60.0], false);
    let after = place_of(&e, &camp);
    let by = [after[0] - c[0], after[1] - c[1]];
    assert!(
        (by[0].powi(2) + by[1].powi(2)).sqrt() > 0.1,
        "the camp moved: {after:?}"
    );
    for ((_, was), (_, is)) in now.iter().zip(spots_of(&e, &camp)) {
        assert!(
            (is[0] - was[0] - by[0]).abs() < 0.01 && (is[1] - was[1] - by[1]).abs() < 0.01,
            "{was:?} went to {is:?}, the camp by {by:?}"
        );
    }
    assert!(
        e.session.problems().is_empty(),
        "{:?}",
        e.session.problems()
    );
}

/// The panels drawn once, as a window draws them, with nothing pressed.
fn draw_panels(e: &mut Editor<'_>, ctx: &egui::Context) {
    let input = egui::RawInput {
        screen_rect: Some(egui::Rect::from_min_size(
            egui::Pos2::ZERO,
            egui::vec2(1600.0, 900.0),
        )),
        ..Default::default()
    };
    let mut out = ctx.run_ui(input, |ui| e.panels(ui));
    // Nothing draws them: the pictures they hold are let go.
    out.textures_delta.clear();
}

/// The inspector shows every field of a selected object, each as its row writes it: a portal's
/// gates among them.
#[test]
fn the_inspector_shows_every_field_of_the_selected_row_as_it_is_written() {
    let mut e = editor("inspector");
    let g = ground(&mut e);
    e.place_at(g, 0.0);
    let item = e.selected.clone().expect("the placed row");
    e.session
        .set(&item, "requires", "{ quest = 0x800001D5, underway = true }")
        .unwrap();
    e.session.set(&item, "hero", "true").unwrap();
    e.session.set(&item, "kingdom", "4").unwrap();
    let ctx = egui::Context::default();
    draw_panels(&mut e, &ctx);
    draw_panels(&mut e, &ctx);
    let row = e.session.entry(&item).unwrap().row.row.clone();
    let shown: Vec<(String, String)> = athenaeum_foundry::fields::of(&row)
        .iter()
        .map(|f| {
            (
                f.name.to_string(),
                athenaeum_foundry::fields::text(&row, f.name),
            )
        })
        .collect();
    assert_eq!(e.shown_fields(), &shown[..]);
    let get = |name: &str| {
        shown
            .iter()
            .find(|(n, _)| n == name)
            .map(|(_, t)| t.as_str())
            .unwrap()
    };
    assert_eq!(get("requires"), "{ quest = 0x800001D5, underway = true }");
    assert_eq!((get("hero"), get("kingdom")), ("true", "4"));
}

/// A kind of pet and a skill's limit added in the Pets window are the world's own rows, chosen
/// and shown with every field; the database's own kinds are listed with them.
#[test]
fn the_pets_window_adds_a_kind_of_pet_and_a_limit_and_shows_every_field() {
    let mut e = editor("pets");
    let base_pets = e.session.pets().len();
    assert!(base_pets > 0, "the database has kinds of pet");
    e.pets.open = true;
    e.add_pet("0x7FFF0001", "0x47000862");
    assert_eq!(
        e.pets.chosen.as_deref(),
        Some("pet 0x7fff0001"),
        "{}",
        e.status
    );
    e.add_pet_limit("4095", "2", "0x2000");
    assert_eq!(
        e.pets.chosen.as_deref(),
        Some("pet_limit 0x00000fff"),
        "{}",
        e.status
    );
    assert_eq!(e.session.pets().len(), base_pets + 2);
    e.pets.chosen = Some("pet 0x7fff0001".into());
    let ctx = egui::Context::default();
    draw_panels(&mut e, &ctx);
    draw_panels(&mut e, &ctx);
    let row = e
        .session
        .pets()
        .iter()
        .find(|l| yalain_world_db::merge::item_key(l).as_deref() == Some("pet 0x7fff0001"))
        .map(|l| l.row.clone())
        .unwrap();
    let shown: Vec<(String, String)> = athenaeum_foundry::fields::of(&row)
        .iter()
        .map(|f| {
            (
                f.name.to_string(),
                athenaeum_foundry::fields::text(&row, f.name),
            )
        })
        .collect();
    assert_eq!(e.pet_fields(), &shown[..]);
    e.session.remove_pet("pet 0x7fff0001").unwrap();
    assert_eq!(e.session.pets().len(), base_pets + 1);
    assert!(
        e.session.problems().is_empty(),
        "{:?}",
        e.session.problems()
    );
}

/// A storekeeper placed and selected has a shop title in the inspector: set, it is the world's own
/// shop and the inspector shows it; emptied, the shop is gone and the world builds clean.
#[test]
fn a_storekeepers_shop_title_is_set_in_the_inspector_and_shown_there() {
    let mut e = editor("shop-title");
    e.template = "0x470033D0".into();
    let g = ground(&mut e);
    e.place_at(g, 0.0);
    let item = e.selected.clone().expect("the placed keeper");
    let ctx = egui::Context::default();
    draw_panels(&mut e, &ctx);
    assert_eq!(e.shown_shop_title(), "");
    e.set_shop_title(&item, "Bob's Fine Goods");
    assert_eq!(
        e.session.shop_title(0x4700_33D0),
        Some("Bob's Fine Goods"),
        "{}",
        e.status
    );
    draw_panels(&mut e, &ctx);
    assert_eq!(e.shown_shop_title(), "Bob's Fine Goods");
    assert!(
        e.session.problems().is_empty(),
        "{:?}",
        e.session.problems()
    );
    e.set_shop_title(&item, "");
    assert_eq!(e.session.shop_title(0x4700_33D0), None, "{}", e.status);
    draw_panels(&mut e, &ctx);
    assert_eq!(e.shown_shop_title(), "");
}
