//! Behaviour: none (tooling: the world editor's dungeon mode across floors, over the retail dats)
//! A dungeon of three floors built with what fits here alone: a room's open top built on from the
//! floor above, its lid giving way; an open floor built under from the floor below; floors counted
//! from the arrival and stepped one past the highest and the lowest built; and, live, the
//! playtest character walking down through all three.

use std::collections::BTreeSet;

use dereth_primitives::Vec3;
use lyceum_editor::app::{floor_number, sill, EditorApp, Fit, Selection, Target};
use lyceum_foundry::grid::square_middle;
use lyceum_foundry::kit::KitPiece;
use lyceum_foundry::source::Link;

use crate::world::dungeon::{choose_doorway, keep, north_south, put_fit, REDOUBT};
use crate::{click, gpu_lock, grid_point, key, HOLTBURG};

/// The kit piece fit `f` sets in.
fn fit_piece<'a>(ed: &'a EditorApp, f: &Fit) -> &'a KitPiece {
    &ed.kit.pieces.as_ref().unwrap()[f.piece]
}

/// Whether fit `f` sets in a level corridor running north and south that opens up when `up` and
/// down when `down`, and no other way.
fn corridor(up: bool, down: bool) -> impl Fn(&EditorApp, &Fit) -> bool {
    move |ed, f| {
        let form = &fit_piece(ed, f).form;
        north_south(ed, f) && form.above == up && form.below == down
    }
}

/// Floor heights to the meter.
fn rounded(floors: &[f32]) -> Vec<i32> {
    floors
        .iter()
        .map(|f| dereth_primitives::num::floor_to_i32(f + 0.5))
        .collect()
}

/// The floor shown, by its number from the floor players arrive on.
fn floor_shown(ed: &mut EditorApp, lb: u16) -> i32 {
    let (_, shown) = ed.plan_floors(lb);
    floor_number(shown, ed.arrival_floor(lb))
}

/// PageUp (`1`) or PageDown (`-1`) pressed: the floor shown after, by its number.
fn step(ed: &mut EditorApp, lb: u16, by: i32) -> i32 {
    let vk = if by > 0 { 0x21 } else { 0x22 };
    key(ed, vk, true);
    key(ed, vk, false);
    // The step is taken after the frame's panels, which the next frame draws on it.
    for _ in 0..2 {
        ed.frame();
    }
    floor_shown(ed, lb)
}

/// Square `s` of the floor shown clicked where it is farthest from any dot: chosen to build on,
/// what fits there listed.
fn choose_square(ed: &mut EditorApp, s: (i32, i32)) {
    let (x, y) = square_middle(s);
    ed.layout.center = Some((x, y));
    for _ in 0..2 {
        ed.frame();
    }
    let off = [
        (3.5, 3.5),
        (-3.5, 3.5),
        (3.5, -3.5),
        (-3.5, -3.5),
        (0.0, 3.5),
        (3.5, 0.0),
        (0.0, -3.5),
        (-3.5, 0.0),
    ];
    let clear = |(dx, dy): (f32, f32)| {
        let p = ed.layout.to_screen((x + dx, y + dy)).unwrap();
        ed.layout
            .doorway_dots
            .iter()
            .map(|(_, d)| d.distance(p))
            .fold(f32::MAX, f32::min)
    };
    let (dx, dy) = off
        .into_iter()
        .max_by(|a, b| clear(*a).total_cmp(&clear(*b)))
        .unwrap();
    let g = grid_point(ed, (x + dx, y + dy));
    click(ed, g);
    assert_eq!(ed.layout.target, Some(Target::Square(s)));
    for _ in 0..2 {
        ed.frame();
    }
}

/// The free doorway on a side of dungeon `lb`'s rooms at floor height `z` farthest along
/// `toward`.
fn side_doorway(ed: &EditorApp, lb: u16, z: f32, toward: f32) -> (Link, Vec3) {
    ed.side_doorways(lb)
        .into_iter()
        .filter(|d| (sill(d) - z).abs() < 1.0)
        .map(|d| (d.link, d.opening.middle))
        .max_by(|a, b| (a.1.y * toward).total_cmp(&(b.1.y * toward)))
        .expect("a free doorway on that floor")
}

/// The room selected after a piece is set in: its first cell.
fn selected(ed: &EditorApp) -> u16 {
    match ed.selection {
        Some(Selection::Cell { cell, .. }) => cell,
        ref other => panic!("the piece set in selected: {other:?}"),
    }
}

/// Whether link `l`'s doorway is drawn as a dot to build on, on the floor shown.
fn dotted(ed: &EditorApp, l: &Link) -> bool {
    ed.layout.doorway_dots.iter().any(|(d, _)| d == l)
}

/// Whether link `l`'s opening, up or down, is marked on its own floor, the floor shown.
fn marked(ed: &EditorApp, l: &Link) -> bool {
    ed.layout.marked_ways.iter().any(|(d, _)| d == l)
}

/// Whether fit `f` sets in the upper part of a tall room, with no floor of its own, whose
/// doorway on the south is a narrow one, as a passage's.
fn upper_part_entered_from_the_south(ed: &EditorApp, f: &Fit) -> bool {
    let p = fit_piece(ed, f);
    p.form.below
        && !p.form.above
        && p.form.doors.iter().any(|d| {
            d.side.is_some()
                && d.width < 5.0
                && lyceum_foundry::grid::placed(&d.opening, &f.placement.frame)
                    .out
                    .y
                    < -0.5
        })
}

/// Whether fit `f` sets in a dead end with a floor of its own.
fn dead_end(ed: &EditorApp, f: &Fit) -> bool {
    let p = fit_piece(ed, f);
    p.form.kind == lyceum_foundry::kit::Kind::DeadEnd && !p.form.below
}

/// The rooms of a dungeon of three floors, by their first cells, in the order a body walking
/// north meets them, with the floor each stands on: on floor +1 a passage's dead end (1) and the
/// upper part of a tall corridor (2), with no floor of its own; on floor 0 the corridor's lower
/// part (3), the room players arrive in (4) and a corridor with no floor (5); on floor -1 the
/// corridor under it (6).
pub struct ThreeFloors {
    pub landblock: u16,
    pub rooms: [u16; 6],
}

/// The floor each of [`ThreeFloors::rooms`] stands on, by its number.
pub const FLOORS: [i32; 6] = [1, 1, 0, 0, 0, -1];

/// A new dungeon near Holtburg in Holtburg Redoubt's look, built with what fits here alone, along
/// a line running north, in the look's halls (rooms ten meters across, open to each other side to
/// side) and passages:
/// - floor 0: a corridor, the room players arrive in; south of it another, open above, its top
///   lidded;
/// - floor +1: over that one, the square chosen, a corridor whose floor opens onto its top, the
///   lid giving way: one tall corridor of two parts; south of it, through a passage's doorway, a
///   passage's dead end;
/// - floor 0: north of the first room, a corridor open above and below;
/// - floor -1: under it, the square chosen, a corridor open above.
pub fn build_three_floors(ed: &mut EditorApp, shots_of: &str) -> ThreeFloors {
    let lb = ed.new_dungeon(Some(HOLTBURG)).unwrap();
    ed.choose_dungeon_look(lb, Some(REDOUBT)).unwrap();
    ed.kit.chosen = None;
    for _ in 0..2 {
        ed.frame();
    }
    // A new dungeon opens on floor 0, with nothing to step past.
    assert_eq!(ed.plan_floors(lb), (vec![0.0], 0.0));
    assert_eq!(ed.floor_name(lb, 0.0), "Floor 0 (0.0 m)");
    assert_eq!(step(ed, lb, 1), 0);
    // Floor 0: the room players arrive in. On a square that joins nothing, each piece is offered
    // once.
    choose_square(ed, (9, 10));
    let fits = ed.fits(lb);
    let offered: BTreeSet<usize> = fits.iter().map(|f| f.piece).collect();
    println!("on an empty square, {} fit", fits.len());
    assert_eq!(offered.len(), fits.len(), "each piece once");
    put_fit(ed, lb, &corridor(true, false));
    let arrive = selected(ed);
    assert!(ed.arrival_floor(lb).abs() < 0.01);
    // One floor past it each way, to start another.
    assert_eq!(rounded(&ed.plan_floors(lb).0), [-6, 0, 6]);
    keep(ed, shots_of, "01-floor-0.png");
    // South of it, a corridor open above: its top lidded, and marked on its floor.
    let (link, at) = side_doorway(ed, lb, 0.0, -1.0);
    choose_doorway(ed, &link, at);
    put_fit(ed, lb, &corridor(true, false));
    let lower = selected(ed);
    let lids = ed.lids(lb);
    assert_eq!(lids.pieces.len(), 2, "a lid over each top");
    let name = ed.session.cell_names(lb)[&lower].clone();
    let (top, _) = lids
        .ways
        .iter()
        .find(|(w, _)| matches!(&w.link, Link::Cell { cell, .. } if *cell == name))
        .cloned()
        .expect("its top under a lid");
    for _ in 0..2 {
        ed.frame();
    }
    assert!(marked(ed, &top.link), "its top marked on floor 0");
    assert!(!dotted(ed, &top.link));
    keep(ed, shots_of, "02-open-above-lidded.png");
    // Floor +1: its top a dot to build on; the square over it chosen, what fits there is what
    // opens down onto it.
    assert_eq!(step(ed, lb, 1), 1);
    assert!(dotted(ed, &top.link), "its top a dot on floor +1");
    choose_square(ed, (9, 9));
    let fits = ed.fits(lb);
    assert!(!fits.is_empty(), "{}", ed.panels.message);
    assert!(
        fits.iter().all(|f| fit_piece(ed, f).form.below),
        "every fit opens down onto the top"
    );
    let onto_top = |f: &Fit| f.placement.links.iter().flatten().any(|l| *l == top.link);
    assert!(fits.iter().all(onto_top));
    keep(ed, shots_of, "03-floor-plus-1-what-fits.png");
    put_fit(ed, lb, &upper_part_entered_from_the_south);
    let upper = selected(ed);
    // The lid gave way: the top joins the corridor over it, one tall corridor.
    assert_eq!(ed.lids(lb).pieces.len(), 1, "its lid taken away");
    assert!(ed.session.cells(lb)[&lower]
        .portals
        .iter()
        .any(|p| p.other_cell_id == u32::from(upper)));
    assert_eq!(floor_shown(ed, lb), 1);
    // South of it on floor +1, a passage's dead end, with a floor to stand on.
    let (link, at) = side_doorway(ed, lb, 6.0, -1.0);
    choose_doorway(ed, &link, at);
    put_fit(ed, lb, &dead_end);
    let landing = selected(ed);
    keep(ed, shots_of, "04-floor-plus-1-built.png");
    assert_eq!(step(ed, lb, -1), 0);
    // North of the first room, a corridor open above and below: its open floor marked.
    let (link, at) = side_doorway(ed, lb, 0.0, 1.0);
    choose_doorway(ed, &link, at);
    put_fit(ed, lb, &corridor(true, true));
    let drop = selected(ed);
    let name = ed.session.cell_names(lb)[&drop].clone();
    let bottom = ed
        .free_portals(lb, &[])
        .into_iter()
        .find(|f| {
            f.opening.out.z < -0.5 && matches!(&f.link, Link::Cell { cell, .. } if *cell == name)
        })
        .expect("its open floor free");
    for _ in 0..2 {
        ed.frame();
    }
    assert!(marked(ed, &bottom.link), "its open floor marked on floor 0");
    keep(ed, shots_of, "05-open-floor.png");
    // Floor -1, one past the lowest: the square under it chosen, what fits there is what opens
    // up onto it.
    assert_eq!(step(ed, lb, -1), -1);
    assert!(dotted(ed, &bottom.link), "its open floor a dot on floor -1");
    choose_square(ed, (9, 11));
    let fits = ed.fits(lb);
    assert!(!fits.is_empty(), "{}", ed.panels.message);
    assert!(fits.iter().all(|f| fit_piece(ed, f).form.above));
    keep(ed, shots_of, "06-floor-minus-1-what-fits.png");
    put_fit(ed, lb, &corridor(true, false));
    let under = selected(ed);
    // Every room has a floor under it, its own or one below an opening, and is reached.
    let walk = ed.session.walk(lb);
    assert!(walk.is_empty(), "{walk:#?}");
    // Three floors built, and the selector one past each way.
    let arrival = ed.arrival_floor(lb);
    let cells = ed.session.cells(lb);
    let floor_of = |ed: &EditorApp, c: u16| {
        let cell = &cells[&c];
        let env = ed.session.environment(cell.environment).unwrap();
        let low = env.cells[usize::from(cell.cell_struct)]
            .vertex_array
            .vertices
            .iter()
            .map(|v| v.position.z)
            .fold(f32::MAX, f32::min);
        floor_number(cell.frame.origin.z + low, arrival)
    };
    let lids = ed.lids(lb).cells();
    let built: BTreeSet<i32> = cells
        .iter()
        .filter(|(n, c)| {
            !lids.contains(n) && c.frame.origin.z > lyceum_foundry::cells::CLOSED_DEPTH + 1.0
        })
        .map(|(n, _)| floor_of(ed, *n))
        .collect();
    assert_eq!(built, BTreeSet::from([-1, 0, 1]));
    let (floors, _) = ed.plan_floors(lb);
    assert_eq!(rounded(&floors), [-12, -6, 0, 6, 12]);
    ed.choose_floor(6.0);
    ed.frame();
    assert_eq!(step(ed, lb, 1), 2);
    assert_eq!(step(ed, lb, 1), 2, "no further than one past");
    ed.choose_floor(0.0);
    ed.layout.target = None;
    for _ in 0..2 {
        ed.frame();
    }
    keep(ed, shots_of, "07-three-floors.png");
    let rooms = [landing, upper, lower, arrive, drop, under];
    assert_eq!(rooms.map(|c| floor_of(ed, c)), FLOORS, "each on its floor");
    ThreeFloors {
        landblock: lb,
        rooms,
    }
}

#[test]
fn a_dungeon_of_three_floors_is_built_with_what_fits_here_alone_its_open_top_built_on_from_above_and_its_open_floor_from_below(
) {
    let _g = gpu_lock();
    let (_dir, world) = crate::sample("editor-floors");
    let mut ed = crate::editor_with(&world, &["--size", "1600x1000"]);
    ed.wait_for_pieces();
    let built = build_three_floors(&mut ed, "floors");
    let lb = built.landblock;
    let [_, upper, lower, _, drop, under] = built.rooms;
    // In the lower part of the tall corridor, looking up through its open top into the part
    // over it; in the room under the open floor, looking up through it.
    for (cell, name) in [
        (lower, "08-looking-up-the-tall-corridor.png"),
        (under, "09-looking-up-through-the-open-floor.png"),
    ] {
        let m = crate::world::dungeon::middle(&ed, lb, cell);
        crate::world::dungeon::put_camera(
            &mut ed,
            lb,
            Vec3::new(m.x, m.y - 3.0, m.z - 1.0),
            0.0,
            55.0,
        );
        for _ in 0..3 {
            ed.frame();
        }
        keep(&mut ed, "floors", name);
        let seen = ed.cells_seen();
        let above = if cell == lower { upper } else { drop };
        assert!(
            seen.contains_key(&((u32::from(lb) << 16) | u32::from(above))),
            "{name}: the room above drawn through the opening: {seen:?}"
        );
    }
    // The corridor's upper part taken away: a lid goes back over the lower part's top in the
    // same step, and undone, the upper part stands on it again with no lid.
    let lids = ed.lids(lb).pieces.len();
    let name = ed.session.cell_names(lb)[&lower].clone();
    let lidded = |ed: &mut EditorApp| {
        ed.lids(lb)
            .ways
            .iter()
            .any(|(w, _)| matches!(&w.link, Link::Cell { cell, .. } if *cell == name))
    };
    let joined = |ed: &EditorApp| {
        ed.session.cells(lb)[&lower]
            .portals
            .iter()
            .any(|p| p.other_cell_id == u32::from(upper))
    };
    assert!(!lidded(&mut ed) && joined(&ed));
    let steps = ed.session.history().0;
    ed.delete_room(lb, upper).unwrap();
    assert_eq!(ed.session.history().0, steps + 1, "one step");
    assert_eq!(ed.lids(lb).pieces.len(), lids + 1);
    assert!(lidded(&mut ed), "its top lidded again");
    assert!(
        ed.free_portals(lb, &[])
            .iter()
            .all(|f| f.opening.out.z < 0.5),
        "no top left open"
    );
    ed.frame();
    keep(&mut ed, "floors", "10-upper-part-taken-away.png");
    ed.edit(&lyceum_foundry::script::Action::Undo).unwrap();
    assert_eq!(ed.lids(lb).pieces.len(), lids);
    assert!(
        !lidded(&mut ed) && joined(&ed),
        "the upper part back, no lid"
    );
}

#[cfg(feature = "live-server-tests")]
#[test]
fn the_playtest_character_walks_down_through_all_three_floors_of_a_dungeon_built_with_what_fits_here(
) {
    use std::path::PathBuf;
    let _g = gpu_lock();
    let (_dir, world) = crate::sample("live-three-floors");
    let server =
        PathBuf::from(std::env::var_os("EMPYREAN_SERVER_BIN").expect("EMPYREAN_SERVER_BIN"));
    let sql = PathBuf::from(std::env::var_os("EMPYREAN_WORLD_SQL").expect("EMPYREAN_WORLD_SQL"));
    let mut ed = crate::editor_with(
        &world,
        &[
            "--empyrean",
            server.parent().unwrap().to_str().unwrap(),
            "--weenies",
            sql.to_str().unwrap(),
            "--size",
            "1600x1000",
        ],
    );
    ed.wait_for_names().unwrap();
    ed.wait_for_pieces();
    let built = build_three_floors(&mut ed, "live-three-floors");
    let lb = built.landblock;
    // Each cell of the rooms built, with the floor its room stands on.
    let mut floor_of_cell = std::collections::BTreeMap::new();
    for (room, floor) in built.rooms.iter().zip(FLOORS) {
        for c in ed.room_piece(lb, *room).unwrap().cells {
            floor_of_cell.insert((u32::from(lb) << 16) | u32::from(c), floor);
        }
    }
    let floor_of = |c: u32| floor_of_cell.get(&c).copied();
    // A spot on the dead end's floor on floor +1, in the cell whose cell tree holds it, as the
    // client finds it.
    let landing = ed.room_piece(lb, built.rooms[0]).unwrap().cells;
    let cells = ed.session.cells(lb);
    let shapes = lyceum_foundry::walk::cell_shapes(&cells, &|id| ed.session.environment(id));
    let (x, y0) = square_middle((9, 8));
    let (cell, spot) = (0..19u8)
        .map(|k| y0 + 4.5 - f32::from(k) * 0.5)
        .find_map(|y| {
            let (c, floor) = ed.session.cell_at(lb, Vec3::new(x, y, 7.5))?;
            let p = Vec3::new(x, y, floor + 0.5);
            let holding: Vec<u16> = shapes
                .iter()
                .filter(|s| s.holds(p))
                .map(|s| s.index)
                .collect();
            (landing.contains(&c) && floor > 5.0 && holding == [c])
                .then_some((c, Vec3::new(x, y, floor)))
        })
        .expect("a floor in the dead end");
    // A portal set on Holtburg's ground with the spawn tool, sending who uses it into the dead end.
    ed.leave_dungeon();
    crate::stone_west(&mut ed);
    ed.mode = lyceum_editor::app::Mode::Spawn;
    for _ in 0..2 {
        ed.frame();
    }
    crate::new_portal(&mut ed, "Hideout Portal");
    let w = ed.world_rect.unwrap();
    let s = ed.ui_scale();
    click(
        &mut ed,
        (w.center().x * s, (w.center().y + w.height() * 0.2) * s),
    );
    let Some(Selection::Spawn { file, guid }) = ed.selection.clone() else {
        panic!(
            "the portal selected: {:?} {}",
            ed.selection, ed.panels.message
        );
    };
    let portal = ed.form_weenie().expect("the portal's weenie");
    ed.form_edit(
        &portal,
        &lyceum_editor::app::FormEdit::Destination {
            cell: (u32::from(lb) << 16) | u32::from(cell),
            at: [spot.x, spot.y, spot.z],
        },
    )
    .unwrap();
    let portal = ed
        .session
        .spawns()
        .into_iter()
        .find(|x| x.file == file && x.guid == guid)
        .unwrap();
    let symbol = guid
        .trim()
        .trim_start_matches("{{guid:")
        .trim_end_matches("}}")
        .to_owned();
    let id = ed
        .session
        .compiled()
        .lock
        .of(lyceum_foundry::symbol::Kind::Guid)
        .find(|(name, _)| *name == symbol || name.ends_with(&format!(":{symbol}")))
        .map(|(_, id)| id)
        .unwrap_or_else(|| panic!("the portal's guid: {guid}"));
    // The character comes in a few meters from it, facing it, and uses it as a player would.
    let at = portal.frame.origin;
    ed.look_at(HOLTBURG, Vec3::new(at.x, at.y, at.z + 1.6), 4.0, 0.0, 0.0);
    ed.frame();
    ed.set_live(true).unwrap();
    ed.wait_live().unwrap();
    ed.play_options.interface = lyceum_editor::app::PlayInterface::Modern;
    ed.play_until_entered(&["--use".to_owned(), format!("0x{id:08X}")])
        .unwrap();
    let begun = std::time::Instant::now();
    loop {
        assert!(ed.frame());
        if !ed.play_teleporting()
            && ed
                .play_body()
                .is_some_and(|b| b.cell.0 >> 16 == u32::from(lb))
        {
            break;
        }
        // Only against a hang: the portal taken is what ends the wait.
        assert!(
            begun.elapsed() < std::time::Duration::from_secs(120),
            "the character did not go through: {:?}",
            ed.play_body()
        );
    }
    for _ in 0..90 {
        assert!(ed.frame());
    }
    let start = ed.play_body().map(|b| b.cell.0).expect("the character in");
    assert_eq!(floor_of(start), Some(1), "{start:08X}");
    keep(&mut ed, "live-three-floors", "10-on-floor-plus-1.png");
    // Walked north: off floor +1 into the tall corridor, down onto floor 0, through the room
    // players arrive in, off it into the corridor with no floor and down onto floor -1.
    let forward = dereth_input::keys::Key::new(0x57, 0x11);
    ed.queue(dereth_input::host::HostEvent::KeyboardInput {
        key: forward,
        pressed: true,
        text: None,
    });
    let jump = dereth_input::keys::Key::new(0x20, 0x39);
    let mut floors: Vec<i32> = vec![1];
    let mut cells: Vec<u32> = vec![start];
    let (mut last, mut still, mut jumps) = (None, 0u32, 0u32);
    let begun = std::time::Instant::now();
    while floors.last() != Some(&-1) {
        assert!(ed.frame());
        let Some(body) = ed.play_body() else { continue };
        let c = body.cell.0;
        assert_eq!(c >> 16, u32::from(lb), "still in the dungeon");
        if !cells.contains(&c) {
            cells.push(c);
        }
        if let Some(f) = floor_of(c).filter(|f| floors.last() != Some(f)) {
            floors.push(f);
            keep(
                &mut ed,
                "live-three-floors",
                &format!("1{}-reached-floor-{}.png", floors.len(), f),
            );
        }
        // A walking player is held at the edge of a drop, as in retail: stood still there, the
        // character jumps, and goes over.
        let here = body.frame.origin;
        let moved = last.is_none_or(|p: Vec3| {
            (here.x - p.x).abs() + (here.y - p.y).abs() + (here.z - p.z).abs() > 0.01
        });
        last = Some(here);
        still = if moved { 0 } else { still + 1 };
        if still >= 30 {
            for pressed in [true, false] {
                ed.queue(dereth_input::host::HostEvent::KeyboardInput {
                    key: jump,
                    pressed,
                    text: None,
                });
                for _ in 0..6 {
                    assert!(ed.frame());
                }
            }
            still = 0;
            jumps += 1;
        }
        // Only against a hang: reaching floor -1 is what ends the loop.
        assert!(
            begun.elapsed() < std::time::Duration::from_secs(120),
            "walked through {cells:08X?}, floors {floors:?}: {:?}, ready {}, log {:#?}",
            ed.play_body(),
            ed.play_ready(),
            ed.live_log(0).iter().rev().take(40).collect::<Vec<_>>()
        );
    }
    ed.queue(dereth_input::host::HostEvent::KeyboardInput {
        key: forward,
        pressed: false,
        text: None,
    });
    for _ in 0..60 {
        assert!(ed.frame());
    }
    keep(&mut ed, "live-three-floors", "19-on-floor-minus-1.png");
    println!("walked through {cells:08X?}, floors {floors:?}, jumping {jumps} time(s)");
    assert_eq!(floors, vec![1, 0, -1], "down a floor at a time");
    ed.stop_play();
    assert!(ed.wait_world_empty(), "the character logged off");
    ed.set_live(false).unwrap();
}
