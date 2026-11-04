//! Behaviour: none (checks the world builder's editor end to end without a window, on the world database and the end-of-retail install)
//!
//! The editor is driven as the window drives it, a frame's input at a time. The install is the
//! end-of-retail data files the tests read everywhere; the database is read from
//! `YALAIN_TEST_WORLD_DB` (a checkout), else from the release this build pins in the per-user cache
//! (`yalain world fetch`); without either, a test fails saying so.

use std::path::PathBuf;

use athenaeum_editor::app::{Editor, ViewInput};
use athenaeum_editor::live::Programs;
use athenaeum_editor::scene::{Scene, WorldData};
use athenaeum_editor::view;
use athenaeum_foundry::{build, new, world, Session};
use auberean_dat::testing::{require_install, TARGET};
use dereth_builder::camera::FlyCamera;
use dereth_primitives::Vec3;
use yalain_world_db::release::{cache_dir_for, Host, PINNED_TAG};

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
