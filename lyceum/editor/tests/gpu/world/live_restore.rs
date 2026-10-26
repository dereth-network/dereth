//! Behaviour: none (tooling: what the editor's own world holds of the server's own content once a pack's change of it goes)
//! The server's own spawns of Holtburg, moved and taken away by the pack and put back while the
//! editor's world runs, are the server's own there again at once: from an override file that keeps
//! other changes, and from one that goes. A pack's copy of a server weenie that the server refuses
//! at its first push leaves the server's own weenie in the world.
//! Fixture: the server and its import tool beside `$EMPYREAN_SERVER_BIN`, and the world database
//! `$EMPYREAN_WORLD_SQL`.

use std::collections::{BTreeMap, BTreeSet};
use std::path::{Path, PathBuf};

use dereth_primitives::Vec3;
use lyceum_editor::app::{EditorApp, LootAction};
use lyceum_foundry::edit::{Op, Weenie};
use lyceum_foundry::script::Action as Edit;
use lyceum_foundry::sql_ids::number;

use crate::{editor_with, gpu_lock, HOLTBURG};

/// The editor on `world` over the server's world database, its world up.
fn live_editor(world: &Path) -> EditorApp {
    let server =
        PathBuf::from(std::env::var_os("EMPYREAN_SERVER_BIN").expect("EMPYREAN_SERVER_BIN"));
    let sql = PathBuf::from(std::env::var_os("EMPYREAN_WORLD_SQL").expect("EMPYREAN_WORLD_SQL"));
    let mut ed = editor_with(
        world,
        &[
            "--empyrean",
            server.parent().unwrap().to_str().unwrap(),
            "--weenies",
            sql.to_str().unwrap(),
        ],
    );
    ed.wait_for_names().unwrap();
    ed.set_live(true).unwrap();
    ed.wait_live().unwrap();
    ed
}

/// Holtburg's spawns as the editor's world database has them after the pack's override file of
/// Holtburg was imported last (the import writes them out beside it, each link from the instance
/// to its generator): each one's cell and place by its guid, and each link, generator first.
type Held = (BTreeMap<u32, (u32, [f32; 3])>, BTreeSet<(u32, u32)>);

fn held(ed: &EditorApp, mark: usize) -> Held {
    let file = "A9B4 holtburg overrides";
    assert!(
        ed.live_log(mark)
            .iter()
            .any(|l| l.contains(&format!("Imported {file}.sql"))),
        "the override file imported: {:?}",
        ed.live_log(mark)
    );
    let path = ed
        .live_content()
        .unwrap()
        .join("json")
        .join("landblocks")
        .join(format!("{file}.json"));
    let json: serde_json::Value =
        serde_json::from_str(&std::fs::read_to_string(&path).unwrap()).unwrap();
    let value = &json["value"];
    let n = |v: &serde_json::Value| u32::try_from(v.as_u64().unwrap()).unwrap();
    #[allow(clippy::cast_possible_truncation)]
    let f = |v: &serde_json::Value| v.as_f64().unwrap() as f32;
    let spawns = value["weenies"]
        .as_array()
        .into_iter()
        .flatten()
        .map(|w| {
            let pos = &w["pos"];
            let o = &pos["frame"]["origin"];
            (
                n(&w["id"]),
                (n(&pos["objcell_id"]), [f(&o["x"]), f(&o["y"]), f(&o["z"])]),
            )
        })
        .collect();
    let links = value["links"]
        .as_array()
        .into_iter()
        .flatten()
        .map(|l| (n(&l["target"]), n(&l["source"])))
        .collect();
    (spawns, links)
}

/// Whether `at` is `want`, to the millimeter.
fn at(at: [f32; 3], want: Vec3) -> bool {
    (at[0] - want.x).abs() < 1e-3 && (at[1] - want.y).abs() < 1e-3 && (at[2] - want.z).abs() < 1e-3
}

/// A change pushed to the editor's world: what its world database holds of Holtburg after it.
fn pushed(ed: &mut EditorApp, edit: &Edit) -> Held {
    let mark = ed.live_log(0).len();
    ed.edit(edit).unwrap();
    ed.push_live().unwrap();
    held(ed, mark)
}

#[test]
fn a_server_spawn_moved_or_taken_away_and_put_back_is_the_servers_own_again_in_the_editors_world() {
    let _g = gpu_lock();
    let (_dir, world) = crate::sample("live-spawns-back");
    let mut ed = live_editor(&world);
    // One of the server's generators in Holtburg, and an instance of the two or more it links.
    let file = ed.session.override_file(HOLTBURG);
    ed.session.show_server_spawns([HOLTBURG]);
    let spawns: Vec<_> = ed
        .session
        .spawns()
        .into_iter()
        .filter(|s| s.server && s.landblock() == HOLTBURG)
        .collect();
    let (generator, children) = spawns
        .iter()
        .find_map(|s| {
            let g = ed.session.generator(s)?;
            (g.children.len() >= 2).then(|| (s.clone(), g.children))
        })
        .expect("a generator of the server's in Holtburg");
    let g = number(&generator.guid).unwrap();
    let c = number(&children[0]).unwrap();
    let (cell, origin) = (generator.cell, generator.frame.origin);
    let child = ed.session.spawn(&file, &children[0]).unwrap();
    let kids: BTreeSet<(u32, u32)> = children.iter().map(|k| (g, number(k).unwrap())).collect();
    let moved = Vec3::new(origin.x + 2.0, origin.y, origin.z);
    let mv = Edit::Op(Op::MoveSpawn {
        file: file.clone(),
        guid: generator.guid.clone(),
        origin: moved,
        heading: 0.0,
    });
    // Taken away: gone, and its link with it.
    let (s, l) = pushed(
        &mut ed,
        &Edit::Op(Op::RemoveServerSpawn {
            file: file.clone(),
            guid: children[0].clone(),
        }),
    );
    assert!(!s.contains_key(&c) && !l.contains(&(g, c)));
    // The generator moved too, in the same file.
    let (s, _) = pushed(&mut ed, &mv);
    assert!(at(s[&g].1, moved), "{:?}", s[&g]);
    // The move put back, the file keeping the removal: the generator stands where the server
    // places it, linked to the rest of its instances, the one taken away still gone.
    let (s, l) = pushed(&mut ed, &Edit::Undo);
    assert_eq!(s[&g].0, cell);
    assert!(at(s[&g].1, origin), "{:?} for {origin:?}", s[&g]);
    assert!(!s.contains_key(&c) && !l.contains(&(g, c)));
    for k in kids.iter().filter(|(_, k)| *k != c) {
        assert!(l.contains(k), "{k:x?} linked again: {l:x?}");
    }
    // The removal put back, the file gone: the instance as the server places it, linked again.
    let (s, l) = pushed(&mut ed, &Edit::Undo);
    assert!(at(s[&c].1, child.frame.origin), "{:?}", s.get(&c));
    assert!(kids.is_subset(&l), "{kids:x?} in {l:x?}");
    assert!(at(s[&g].1, origin));
    // Moved alone and put back, the file going with it.
    let (s, _) = pushed(&mut ed, &mv);
    assert!(at(s[&g].1, moved));
    let (s, l) = pushed(&mut ed, &Edit::Undo);
    assert!(
        at(s.get(&g).expect("the generator").1, origin),
        "{:?}",
        s[&g]
    );
    assert!(kids.is_subset(&l), "{kids:x?} in {l:x?}");
    ed.set_live(false).unwrap();
}

#[test]
fn a_copy_of_a_server_weenie_the_server_refuses_at_its_first_push_leaves_the_servers_own() {
    let _g = gpu_lock();
    let (_dir, world) = crate::sample("live-refused-copy");
    let mut ed = live_editor(&world);
    // The names wielded treasure 37's pyreals are made with, each draw making some.
    let pyreals = |ed: &mut EditorApp| -> Vec<String> {
        ed.loot_action(LootAction::Roll { id: 37, times: 5 })
            .unwrap();
        let draws = ed.wait_loot_roll().unwrap();
        assert_eq!(draws.len(), 5);
        draws
            .iter()
            .map(|d| {
                d.iter()
                    .find(|(class, _, _)| *class == 273)
                    .map(|(_, _, name)| name.clone())
                    .unwrap_or_else(|| panic!("no pyreals: {draws:?}"))
            })
            .collect()
    };
    assert!(pyreals(&mut ed).iter().all(|n| n == "Pyreal"));
    // The pack's copy, renamed, with its int properties written twice: a row the world database
    // refuses, as it keeps one of each.
    let own = ed.server_class_now(273).unwrap().expect("the server's 273");
    let ints = own
        .find("INSERT INTO `weenie_properties_int`")
        .expect("its int properties");
    let end = ints + own[ints..].find(';').unwrap() + 1;
    let refused = format!(
        "{}\n{}\n",
        own.replacen("'Pyreal')", "'Pyreal Hoard')", 1),
        &own[ints..end]
    );
    let file = ed.session.class_file(273, "Pyreal");
    ed.edit(&Edit::Op(Op::WriteSql {
        file,
        text: Some(refused),
    }))
    .unwrap();
    assert!(ed.push_live().is_err(), "the server refuses it");
    // The world as it was: the server's own pyreals.
    assert!(pyreals(&mut ed).iter().all(|n| n == "Pyreal"));
    assert!(ed.session.weenie_file(&Weenie::Class(273)).is_some());
    ed.set_live(false).unwrap();
}
