//! Behaviour: none (tooling: the weenie editor's list of where a weenie appears, over the retail dats and the server's world database)
//! Where a weenie appears lists every spawn of it in the world, not only those around the
//! camera: by landblock, each with its count, a page at a time, gathered once and not on every
//! frame. A click on a far one flies there, into its room for one in a dungeon, and it is
//! selected once its landblock's spawns are shown.
//! Fixture: the retail dats, a device, and the server's world database (`EMPYREAN_WORLD_SQL`,
//! else the newest the importer fetched).

use std::collections::{BTreeMap, BTreeSet};

use lyceum_editor::app::{Drill, Selection, Tab, APPEARS_PAGE};
use lyceum_editor::camera::landblock_of;
use lyceum_foundry::edit::{Spawn, Weenie};

use crate::{click, editor_with, gpu_lock, sample, shots, spawn_of, world_sql, HOLTBURG};

/// The life stone: one in Holtburg, and some hundreds more across the world, in dungeons too.
const LIFE_STONE: u32 = 509;

#[test]
fn a_server_weenie_appears_world_wide_by_landblock_and_a_click_on_a_far_one_flies_there() {
    let _gpu = gpu_lock();
    let (_dir, world) = sample("editor-appears");
    let sql = world_sql();
    let mut ed = editor_with(
        &world,
        &["--weenies", sql.to_str().unwrap(), "--size", "1280x1000"],
    );
    ed.wait_for_names().unwrap();
    let shots = shots("appears");
    // The server's own, by landblock, counted from its database apart from the editor.
    let bytes = std::fs::read(&sql).unwrap();
    let instances: String = String::from_utf8_lossy(&bytes)
        .lines()
        .filter(|l| l.starts_with("INSERT INTO `landblock_instance`"))
        .flat_map(|l| [l, "\n"])
        .collect();
    let mut want: BTreeMap<u16, usize> = BTreeMap::new();
    for i in lyceum_foundry::sql_ids::server_content(&instances).instances {
        if lyceum_foundry::sql_ids::number(&i.weenie) == Some(LIFE_STONE) {
            *want
                .entry(u16::try_from(i.cell >> 16).unwrap())
                .or_default() += 1;
        }
    }
    // The camera in Holtburg, the Definition pane opened into where the life stone appears.
    let w = Weenie::Class(LIFE_STONE);
    ed.server_class_now(LIFE_STONE).unwrap().expect("its rows");
    ed.selection = None;
    ed.panels.weenie = Some(w.clone());
    ed.editor.open = false;
    ed.show_tab(Tab::Definition);
    ed.definition.drill_into(&w, Drill::Appears);
    for _ in 0..3 {
        ed.frame();
    }
    let here = landblock_of(ed.camera.position).unwrap().0;
    assert_eq!(
        lyceum_editor::catalog::town_of(here).as_deref(),
        Some("Holtburg")
    );
    // Gathered once, not on each frame.
    let gathers = ed.stats.appears_gathers;
    for _ in 0..5 {
        ed.frame();
    }
    assert_eq!(ed.stats.appears_gathers, gathers, "gathered once");
    // Every one in the world, by landblock: far more landblocks than those around the camera,
    // whose spawns are the only ones shown.
    let listed = ed.appears_of(&w);
    let mut by: BTreeMap<u16, usize> = BTreeMap::new();
    for s in listed.iter() {
        *by.entry(s.landblock()).or_default() += 1;
    }
    assert_eq!(by, want, "every one of the server's");
    let near: BTreeSet<u16> = ed
        .session
        .spawns()
        .iter()
        .filter(|s| ed.session.spawn_weenie(s).as_ref() == Some(&w))
        .map(Spawn::landblock)
        .collect();
    assert!(near.contains(&HOLTBURG) && near.iter().all(|lb| by.contains_key(lb)));
    assert!(by.len() > near.len() + 100, "{} landblocks", by.len());
    // The first page of them, by landblock, each landblock's words with its whole count.
    let a = &ed.definition.appears;
    let first: Vec<u16> = listed
        .iter()
        .take(APPEARS_PAGE)
        .map(Spawn::landblock)
        .collect::<BTreeSet<_>>()
        .into_iter()
        .collect();
    let drawn: Vec<u16> = a.groups.iter().map(|(lb, ..)| *lb).collect();
    assert_eq!(drawn, first);
    for (lb, n, _) in &a.groups {
        assert_eq!(Some(n), want.get(lb), "{lb:04X}'s count");
    }
    assert!(a.next.is_some() && a.page(&w) == 0);
    // A far one in a dungeon's room: a click flies into the room, and it is selected and its
    // handles drawn once its landblock's spawns are shown.
    let (lb, cell, file, guid, r) = a
        .rows
        .iter()
        .find(|(lb, cell, ..)| !near.contains(lb) && cell & 0xFFFF >= 0x100)
        .cloned()
        .expect("a far one indoors, in view");
    ed.capture(&shots.join("01-world-wide.png")).unwrap();
    click(&mut ed, (r.center().x, r.center().y));
    for _ in 0..3 {
        ed.frame();
    }
    assert_eq!(
        ed.selection,
        Some(Selection::Spawn {
            file: file.clone(),
            guid: guid.clone()
        })
    );
    assert_eq!(ed.dungeon_block, Some(lb));
    let (room, _) = ed.camera_cell().expect("the camera in a room");
    assert_eq!(room.0 >> 16, u32::from(lb), "{:08X} for {cell:08X}", room.0);
    assert_eq!(ed.work_block().map(|b| b.0), Some(lb));
    assert!(ed.handles().is_some(), "selected and drawn there");
    ed.capture(&shots.join("02-in-its-room.png")).unwrap();
    // The next page, and a far one outdoors on it: a click flies over it.
    let next = ed.definition.appears.next.unwrap();
    click(&mut ed, (next.center().x, next.center().y));
    ed.frame();
    assert_eq!(ed.definition.appears.page(&w), 1);
    let (lb, _, file, guid, r) = loop {
        if let Some(row) = ed
            .definition
            .appears
            .rows
            .iter()
            .find(|(lb, cell, ..)| !near.contains(lb) && cell & 0xFFFF < 0x100)
            .cloned()
        {
            break row;
        }
        let page = ed.definition.appears.page(&w);
        let next = ed.definition.appears.next.expect("more pages");
        click(&mut ed, (next.center().x, next.center().y));
        ed.frame();
        assert_eq!(
            ed.definition.appears.page(&w),
            page + 1,
            "no far one outdoors"
        );
    };
    click(&mut ed, (r.center().x, r.center().y));
    for _ in 0..3 {
        ed.frame();
    }
    assert_eq!(ed.selection, Some(Selection::Spawn { file, guid }));
    assert_eq!(ed.dungeon_block, None);
    assert_eq!(landblock_of(ed.camera.position).map(|b| b.0), Some(lb));
    assert!(ed.handles().is_some(), "selected and drawn there");
    ed.capture(&shots.join("03-over-it.png")).unwrap();
    // One set down in the world is listed too, gathered again after the edit.
    let gathers = ed.stats.appears_gathers;
    spawn_of(&mut ed, &w);
    ed.frame();
    assert_eq!(ed.stats.appears_gathers, gathers + 1);
    assert_eq!(ed.appears_of(&w).len(), listed.len() + 1);
}
