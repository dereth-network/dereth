//! Behaviour: none (tooling: the statics standing in a building's rooms, picked, moved and taken away as edits of a pack, over the retail files)

use dereth_primitives::frame::{localtoglobal, V3};
use dereth_primitives::Vec3;
use lyceum_foundry::compile;
use lyceum_foundry::edit::{Op, Session};
use lyceum_foundry::pick::{self, Hit, Mask, Models, Ray};
use lyceum_foundry::source::Rotation;
use lyceum_foundry::world::World;

const HOLTBURG: u16 = 0xA9B4;

/// A Holtburg room with at least two statics, and its number.
fn furnished(s: &Session) -> (u16, dereth_assets::world::EnvCell) {
    s.cells(HOLTBURG)
        .into_iter()
        .find(|(_, c)| c.static_objects.len() >= 2)
        .expect("a furnished room")
}

#[test]
fn a_static_in_a_holtburg_room_is_picked_from_inside_the_room() {
    let (_dir, world) = super::sample::sample("pack-cell-pick");
    let s = Session::open(&world, &dereth_dat::testing::dat_dir(), None).unwrap();
    let mut models = Models::default();
    let base = pick::block_origin(HOLTBURG);
    let (mut tried, mut met) = (0, 0);
    for (n, cell) in s.cells(HOLTBURG) {
        let Some((_, floor)) = s.cell_at(HOLTBURG, cell.frame.origin.add(Vec3::new(0.0, 0.0, 1.0)))
        else {
            continue;
        };
        for (index, o) in cell.static_objects.iter().enumerate() {
            let Some(m) = models.get(s.base(), o.id) else {
                continue;
            };
            let center = localtoglobal(&o.frame, m.center);
            // Something under the ceiling, above the floor: met from just over it, looking down.
            if m.radius > 1.2 || center.z < floor + 0.2 || center.z > floor + 1.5 {
                continue;
            }
            let from = base.add(center).add(Vec3::new(0.0, 0.0, m.radius + 0.2));
            let ray = Ray {
                origin: from,
                dir: Vec3::new(0.0, 0.0, -1.0),
            };
            let hit = pick::pick(&s, &mut models, &ray, &[HOLTBURG], Mask::ALL)
                .expect("something is met");
            // It, another of the room's standing on or over it (a lamp on a table), or the
            // room's floor through a hollow one: never anything outside the room.
            tried += 1;
            match hit.hit {
                Hit::CellStatic { cell, index: i, .. } if cell == n => {
                    met += usize::from(i == index)
                }
                Hit::Cell { cell, .. } if cell == n => {}
                _ => panic!("cell {n:04X} static {index}: {hit:?}"),
            }
        }
    }
    assert!(
        tried >= 5 && met * 2 >= tried,
        "{met} of {tried} statics picked"
    );
    // Its floor is met as the cell.
    let (n, cell) = furnished(&s);
    let up = cell.frame.origin.add(Vec3::new(0.0, 0.0, 0.5));
    let ray = Ray {
        origin: base.add(up),
        dir: Vec3::new(0.0, 0.0, -1.0),
    };
    let mask = Mask {
        cell_statics: false,
        ..Mask::ALL
    };
    let hit = pick::pick(&s, &mut models, &ray, &[HOLTBURG], mask).unwrap();
    assert!(
        matches!(hit.hit, Hit::Cell { cell, .. } if cell == n),
        "{hit:?}"
    );
}

#[test]
fn a_rooms_static_is_moved_and_taken_away_as_edits_of_the_pack_and_undone() {
    let (dir, world) = super::sample::sample("pack-cell-statics");
    let mut s = Session::open(&world, &dereth_dat::testing::dat_dir(), None).unwrap();
    let (n, before) = furnished(&s);
    let o = before.static_objects[1];
    let to = [o.frame.origin.x + 0.5, o.frame.origin.y, o.frame.origin.z];
    s.apply(&Op::MoveCellStatic {
        landblock: HOLTBURG,
        cell: n,
        index: 1,
        at: to,
        rotation: Some(Rotation::Heading(45.0)),
    })
    .unwrap();
    let moved = s.cells(HOLTBURG)[&n].static_objects[1];
    assert_eq!(moved.id, o.id);
    // Written to the millimeter.
    let d = moved.frame.origin.sub(Vec3::new(to[0], to[1], to[2]));
    assert!(d.magnitude() < 1e-3, "{:?}", moved.frame.origin);
    assert_eq!(moved.frame.rotation, compile::heading_quat(45.0));
    // The first taken away: the moved one is the first now, and still the room's second.
    s.apply(&Op::RemoveCellStatic {
        landblock: HOLTBURG,
        cell: n,
        index: 0,
    })
    .unwrap();
    let after = &s.cells(HOLTBURG)[&n];
    assert_eq!(after.static_objects.len(), before.static_objects.len() - 1);
    assert_eq!(after.static_objects[0].id, o.id);
    assert_eq!(s.cell_static_origin(HOLTBURG, n, 0), 1);
    // Moved again by its place now: the same move written over.
    let again = [to[0] + 0.25, to[1], to[2]];
    s.apply(&Op::MoveCellStatic {
        landblock: HOLTBURG,
        cell: n,
        index: 0,
        at: again,
        rotation: None,
    })
    .unwrap();
    assert!((s.cells(HOLTBURG)[&n].static_objects[0].frame.origin.x - again[0]).abs() < 1e-3);
    s.save().unwrap();
    let file = dir.path().join("holtburg/edits/landblocks/A9B4.toml");
    let text = std::fs::read_to_string(&file).unwrap();
    assert_eq!(text.matches("[[move-cell-static]]").count(), 1, "{text}");
    assert_eq!(text.matches("[[remove-cell-static]]").count(), 1, "{text}");
    for line in [
        format!("cell = 0x{n:04X}"),
        "index = 1".to_owned(),
        "index = 0".to_owned(),
        format!("model = 0x{:08X}", before.static_objects[0].id.raw()),
        "heading = 45.0".to_owned(),
    ] {
        assert!(text.contains(&line), "{line}\n{text}");
    }
    // A full build of what was saved gives what the session holds.
    let w = World::read(&world).unwrap();
    let lock = lyceum_foundry::lock::Lock::read(&w.lock_path()).unwrap();
    let full = compile::compile(&w, s.packs(), s.base(), &lock).unwrap();
    assert_eq!(full.records, s.compiled().records);
    assert_eq!(full.cell_statics, s.compiled().cell_statics);
    // Undone, the room is the world's own again.
    for _ in 0..3 {
        s.undo().unwrap();
    }
    assert_eq!(s.cells(HOLTBURG)[&n], before);
}

#[test]
fn a_cell_static_that_is_not_there_is_refused_with_its_line() {
    let (dir, world) = super::sample::sample("pack-cell-static-bad");
    let s = Session::open(&world, &dereth_dat::testing::dat_dir(), None).unwrap();
    let (n, cell) = furnished(&s);
    let file = dir.path().join("holtburg/edits/landblocks/A9B4.toml");
    let text = std::fs::read_to_string(&file).unwrap();
    let count = cell.static_objects.len();
    std::fs::write(
        &file,
        format!("{text}\n[[remove-cell-static]]\ncell = 0x{n:04X}\nindex = {count}\n"),
    )
    .unwrap();
    let e = Session::open(&world, &dereth_dat::testing::dat_dir(), None)
        .unwrap_err()
        .to_string();
    assert!(
        e.contains("A9B4.toml:") && e.contains(&format!("there is no {count}")),
        "{e}"
    );
}

/// The sample world with a second pack, `rival`, after it; `extra` at the end of the world file.
fn with_rival(dir: &std::path::Path, extra: &str) -> std::path::PathBuf {
    let rival = dir.join("rival");
    std::fs::create_dir_all(rival.join("edits/landblocks")).unwrap();
    std::fs::write(
        rival.join("pack.toml"),
        "name = \"rival\"\nversion = \"0.1.0\"\nlicense = \"MIT\"\n",
    )
    .unwrap();
    let world = dir.join("world.toml");
    let text = std::fs::read_to_string(&world).unwrap();
    std::fs::write(
        &world,
        format!(
            "{}\n{extra}",
            text.replace(
                "packs = [\"holtburg\"]",
                "packs = [\"holtburg\", \"rival\"]"
            )
        ),
    )
    .unwrap();
    world
}

fn append(path: &std::path::Path, text: &str) {
    let before = std::fs::read_to_string(path).unwrap_or_default();
    std::fs::write(path, format!("{before}\n{text}")).unwrap();
}

#[test]
fn two_packs_moving_a_rooms_static_or_reshaping_a_cell_differently_conflict_until_the_world_says_which_wins(
) {
    let (dir, world) = super::sample::sample("pack-cell-conflict");
    let s = Session::open(&world, &dereth_dat::testing::dat_dir(), None).unwrap();
    let (n, cell) = furnished(&s);
    let o = cell.static_objects[1].frame.origin;
    // The dead end the hideout starts with, reshaped by each as its own piece, drawn otherwise.
    let hideout = s.cells(0x019E);
    let (dn, dead) = hideout
        .iter()
        .find(|(_, c)| c.portals.len() == 1)
        .map(|(n, c)| (*n, c.clone()))
        .unwrap();
    let q = dead.frame.rotation;
    let reshape = |surfaces: &str| {
        format!(
            "[[replace-cell]]\ncell = 0x{dn:04X}\nenvironment = 0x{:08X}\npiece = {}\nsurfaces = [{surfaces}]\nat = [{}, {}, {}]\nrotation = [{}, {}, {}, {}]\nportals = [\"keep\"]\n",
            dead.environment.raw(),
            dead.cell_struct,
            dead.frame.origin.x,
            dead.frame.origin.y,
            dead.frame.origin.z,
            q.w,
            q.x,
            q.y,
            q.z
        )
    };
    let list = |ids: &[dereth_primitives::DataId]| {
        ids.iter()
            .map(|i| format!("0x{:08X}", i.raw()))
            .collect::<Vec<_>>()
            .join(", ")
    };
    let mut other = dead.surfaces.clone();
    other.reverse();
    let move_to = |x: f32| {
        format!(
            "[[move-cell-static]]\ncell = 0x{n:04X}\nindex = 1\nat = [{x}, {}, {}]\n",
            o.y, o.z
        )
    };
    append(
        &dir.path().join("holtburg/edits/landblocks/A9B4.toml"),
        &move_to(o.x + 1.0),
    );
    append(
        &dir.path().join("holtburg/edits/landblocks/019E.toml"),
        &reshape(&list(&dead.surfaces)),
    );
    let w = with_rival(dir.path(), "");
    append(
        &dir.path().join("rival/edits/landblocks/A9B4.toml"),
        &move_to(o.x + 2.0),
    );
    append(
        &dir.path().join("rival/edits/landblocks/019E.toml"),
        &reshape(&list(&other)),
    );
    let build =
        |w: &std::path::Path| compile::build(w, &dereth_dat::testing::dat_dir(), None, false);
    let Err(lyceum_foundry::Error::Conflicts(c)) = build(&w) else {
        panic!("not in conflict")
    };
    let what: Vec<&str> = c.iter().map(|c| c.what.as_str()).collect();
    assert_eq!(
        what,
        [
            format!("cell 019E {dn:04X}"),
            format!("cell-static A9B4 {n:04X} 1")
        ]
    );
    assert!(c.iter().all(|c| c
        .claims
        .iter()
        .map(|x| x.pack.as_str())
        .eq(["holtburg", "rival"])));
    // The world says rival wins both: its move and its reshape are built.
    let w = with_rival(
        dir.path(),
        &format!(
            "[[override]]\npack = \"rival\"\nover = \"holtburg\"\nitems = [\"cell-static A9B4 {n:04X} 1\", \"cell 019E {dn:04X}\"]\n"
        ),
    );
    let built = build(&w).unwrap();
    assert_eq!(built.overridden.len(), 2);
    let s = Session::open(&w, &dereth_dat::testing::dat_dir(), None).unwrap();
    assert!((s.cells(HOLTBURG)[&n].static_objects[1].frame.origin.x - (o.x + 2.0)).abs() < 1e-3);
    assert_eq!(s.cells(0x019E)[&dn].surfaces, other);
}
