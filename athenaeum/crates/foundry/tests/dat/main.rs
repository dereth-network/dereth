//! Behaviour: none (checks the world builder end to end on the world database and the end-of-retail install: a world over the database, edited, saved and compiled into the server's tables)
//!
//! The install is the end-of-retail data files the tests read everywhere; the database is read from
//! `YALAIN_TEST_WORLD_DB` (a checkout), else from the release this build pins in the per-user cache
//! (`yalain world fetch`); without either, a test fails saying so.

use std::path::{Path, PathBuf};

use athenaeum_foundry::edit::{NewKind, NewRow};
use athenaeum_foundry::{build, new, world, Layer, Session};
use auberean_dat::locate::{install_dats, DatName};
use auberean_dat::testing::{open_wrong_once, require_install, TARGET};
use yalain_content::{ContentObject, Kind};
use yalain_world_db::compile::{outdoor_cell, Tables};
use yalain_world_db::release::{cache_dir_for, Host, PINNED_TAG};

/// The variable naming a world database checkout for the tests.
const WORLD_DB_VAR: &str = "YALAIN_TEST_WORLD_DB";

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

/// A folder of the test's own, empty.
fn scratch(name: &str) -> PathBuf {
    let d = std::env::temp_dir().join(format!("athenaeum-dat-{name}-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&d);
    std::fs::create_dir_all(&d).unwrap();
    d
}

fn compiled(dir: &Path, base: &Path) -> Tables {
    let opened = world::open(dir, Some(base)).expect("the world opens over the database");
    let files = build::InstallFiles::open(&require_install(TARGET)).expect("the install");
    let install = files.install().expect("the install's script library");
    build::compile_world(&opened, &install).expect("the world compiles")
}

fn at(t: &Tables, template: u32, cell: u32) -> Vec<&ContentObject> {
    t.content
        .objects
        .iter()
        .filter(|o| o.template == template && o.cell == cell)
        .collect()
}

/// A storekeeper of Arwic's, placed in a new world over the database, saved and compiled, is a
/// person of the server's content where it was placed, and a person of the database moved in the
/// world stands where it was moved to and no longer where it stood.
#[test]
fn a_person_placed_and_one_moved_in_a_world_are_where_the_server_puts_them() {
    let base = database();
    let root = scratch("place");
    let dir = root.join("arwic-plus");
    new::new_world(&dir, None).unwrap();
    let opened = world::open(&dir, Some(&base)).unwrap();
    let base_objects = {
        let mut s = Session::open(opened.clone(), None).unwrap();
        assert!(s.problems().is_empty(), "{:?}", s.problems());
        // An Arwic person of the database, moved a few meters.
        let moved = "object n-334c@77B8";
        let was = s
            .entry(moved)
            .expect("the database places this person")
            .clone();
        assert_eq!(was.layer, Layer::Base);
        s.move_to(moved, 0x77B8, [3.0, 104.0, 182.5]).unwrap();
        // A weapon vendor placed beside them.
        let placed = s
            .place(&NewRow {
                kind: NewKind::Person,
                template: 0x4700_33D0,
                landblock: 0x77B8,
                at: [7.0, 105.0, 182.5],
                heading: 180.0,
            })
            .unwrap();
        assert_eq!(placed, "object person-1@77B8");
        assert!(s.problems().is_empty(), "{:?}", s.problems());
        s.save().unwrap();
        opened.base.merged.rows.len()
    };
    let t = compiled(&dir, &base);
    let vendor = at(&t, 0x4700_33D0, outdoor_cell(0x77B8, 7.0, 105.0));
    let vendor = vendor
        .iter()
        .find(|o| o.origin == [7.0, 105.0, 182.5])
        .expect("the vendor is in the server's content");
    assert_eq!(vendor.kind, Kind::Npc);
    assert!(vendor.placed.starts_with("authored"), "{}", vendor.placed);
    assert_ne!(vendor.description.look, 0, "described from its template");
    let orven: Vec<&ContentObject> = t
        .content
        .objects
        .iter()
        .filter(|o| o.template == 0x4700_334C)
        .collect();
    assert!(orven.iter().any(|o| o.origin == [3.0, 104.0, 182.5]));
    assert!(
        !orven
            .iter()
            .any(|o| (o.origin[0] - 1.3796).abs() < 1e-3 && (o.origin[1] - 100.7791).abs() < 1e-3),
        "no longer where the database put them"
    );
    // The database's own content, and one more.
    let plain = {
        let empty = root.join("empty");
        new::new_world(&empty, None).unwrap();
        compiled(&empty, &base)
    };
    assert_eq!(t.content.objects.len(), plain.content.objects.len() + 1);
    assert!(base_objects > 0);
    std::fs::remove_dir_all(&root).ok();
}

/// A script library read wrong once, over a file that did not change, is read again and a compile
/// reads the install with it.
/// Behaviour: data.read.script-library-reads-are-checked
#[test]
fn a_script_library_read_wrong_once_is_read_again_for_a_compile() {
    let dir = require_install(TARGET);
    let found = install_dats(&dir).expect("the install");
    let (_, p) = found
        .iter()
        .find(|(n, _)| *n == DatName::Portal)
        .expect("a portal file");
    let portal = open_wrong_once(p, auberean_script::library::RECORD);
    let files = build::InstallFiles::of(portal, None).expect("the install");
    files.install().expect("the second read decodes");
}
