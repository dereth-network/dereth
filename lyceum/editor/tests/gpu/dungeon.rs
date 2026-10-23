//! Behaviour: none (tooling: the world editor's dungeon mode over the retail dats)
//! Dungeon mode: the look first and the pieces by what they are on the grid; a room taken away
//! whole and what fits in its place; clicks in the world view that only select; picking through
//! a floor; the dungeon alone drawn; restyling, renaming and deleting a dungeon.

use std::collections::BTreeSet;

use dereth_primitives::Vec3;
use lyceum_editor::app::{EditorApp, Fit, Selection, Target};
use lyceum_foundry::kit::Kind;
use lyceum_foundry::source::Link;

use super::{
    capture, click, editor_with, gpu_lock, grid_point, hover, key, sample, shots, HOLTBURG,
};

/// Holtburg Redoubt.
pub const REDOUBT: u16 = 0x0163;

/// A cell of the room of Holtburg Redoubt with a ceiling piece over its stairwell.
pub const CEILING_CELL: u16 = 0x017D;

/// Keep the frame drawn last as `name` among the shots of `group`.
pub fn keep(ed: &mut EditorApp, group: &str, name: &str) {
    let path = shots(group).join(name);
    ed.capture(&path).unwrap();
}

/// Into Holtburg Redoubt, the kit gathered, a few frames drawn.
fn into_redoubt(tag: &str) -> (dereth_dat::testing::ScratchDir, EditorApp) {
    let (dir, world) = sample(tag);
    let mut ed = editor_with(&world, &["--size", "1600x1000"]);
    ed.wait_for_pieces();
    ed.enter_dungeon(REDOUBT).unwrap();
    for _ in 0..3 {
        assert!(ed.frame());
    }
    (dir, ed)
}

#[test]
fn a_dungeons_look_comes_first_and_every_piece_holtburg_redoubt_is_built_of_stands_under_it_in_a_group_by_what_it_is(
) {
    let _g = gpu_lock();
    let (_dir, mut ed) = into_redoubt("editor-palette");
    // Its look heads the card: chosen, shown by its textures.
    let look = ed.dungeon_look(REDOUBT).expect("the redoubt's look");
    assert_eq!(ed.kit.look, Some(look));
    assert!(ed.dungeon_list.look_combo.is_some());
    // Every piece the redoubt is built of, in the palette of its look, under a group a builder
    // knows: none under Other.
    let pieces = ed.kit.pieces.clone().unwrap();
    let used: BTreeSet<usize> = (0..pieces.len())
        .filter(|i| pieces[*i].landblocks.contains_key(&REDOUBT))
        .collect();
    assert_eq!(used.len(), 25);
    let groups = ed.piece_groups(Some(look), false);
    for i in &used {
        let (kind, _) = groups
            .iter()
            .find(|(_, list)| list.contains(i))
            .unwrap_or_else(|| panic!("{} in the palette", pieces[*i].label()));
        assert_ne!(*kind, Kind::Other, "{}", pieces[*i].label());
    }
    let kinds: BTreeSet<Kind> = used.iter().map(|i| pieces[*i].form.kind).collect();
    println!(
        "the redoubt's pieces by group: {:?}",
        kinds
            .iter()
            .map(|k| (
                ed.kind_name(*k),
                used.iter().filter(|i| pieces[**i].form.kind == *k).count()
            ))
            .collect::<Vec<_>>()
    );
    // The palette as drawn: its groups' headers, and each piece as its floor plan.
    keep(&mut ed, "palette", "redoubt-look.png");
    let shown: BTreeSet<Kind> = ed.kit.group_rects.iter().map(|(k, _)| *k).collect();
    assert!(
        shown.contains(&Kind::DeadEnd) && shown.contains(&Kind::Corridor),
        "{shown:?}"
    );
    // Each tile a floor plan: its floors filled in the plan's slate gray, its doorways as bars in
    // the turn's color.
    let img = capture(&mut ed, _dir.path());
    let s = ed.ui_scale();
    let (mut floor, mut doorway) = (0, 0);
    for (_, r) in ed.kit.piece_rects.iter().take(8) {
        let mut y = r.min.y;
        while y < r.max.y {
            let mut x = r.min.x;
            while x < r.max.x {
                let [red, green, blue] = super::pixel(&img, x * s, y * s);
                floor += usize::from(
                    red == green
                        && blue.abs_diff(red.saturating_add(8)) <= 1
                        && (40..=120).contains(&red),
                );
                doorway += usize::from(
                    red.abs_diff(0xF0) < 12
                        && green.abs_diff(0xD0) < 12
                        && blue.abs_diff(0x40) < 12,
                );
                x += 1.0;
            }
            y += 1.0;
        }
    }
    assert!(
        floor > 2000 && doorway > 100,
        "floors {floor}, doorways {doorway}"
    );
    // The pieces of the redoubt alone, found by its landblock: each in its group.
    ed.kit.filter = "0163".into();
    for _ in 0..2 {
        ed.frame();
    }
    let found: BTreeSet<usize> = ed.kit.piece_rects.iter().map(|(i, _)| *i).collect();
    assert!(used.iter().all(|i| found.contains(i)), "{found:?}");
    keep(&mut ed, "palette", "redoubt-pieces.png");
}

/// The middle of cell `n` of landblock `lb`, in its landblock's meters.
pub fn middle(ed: &EditorApp, lb: u16, n: u16) -> Vec3 {
    let c = ed.session.cells(lb).remove(&n).unwrap();
    let e = ed.session.environment(c.environment).unwrap();
    let vs = &e.cells[usize::from(c.cell_struct)].vertex_array.vertices;
    #[allow(clippy::cast_precision_loss)]
    let k = vs.len() as f32;
    vs.iter().fold(Vec3::ZERO, |m, v| {
        let p = dereth_primitives::frame::localtoglobal(&c.frame, v.position);
        Vec3::new(m.x + p.x / k, m.y + p.y / k, m.z + p.z / k)
    })
}

/// The camera at `at` (landblock meters of `lb`), turned `yaw` and pitched `pitch` (degrees).
pub fn put_camera(ed: &mut EditorApp, lb: u16, at: Vec3, yaw: f32, pitch: f32) {
    let o = lyceum_foundry::pick::block_origin(lb);
    let speed = ed.camera.speed;
    ed.camera = lyceum_editor::camera::FlyCamera::new(
        Vec3::new(o.x + at.x, o.y + at.y, o.z + at.z),
        yaw.to_radians(),
        pitch.to_radians(),
    );
    ed.camera.speed = speed;
}

#[test]
fn the_room_looked_up_at_in_holtburg_redoubt_is_selected_by_a_click_deleted_whole_its_doorways_freed_and_one_undo_brings_it_back(
) {
    let _g = gpu_lock();
    let (_dir, mut ed) = into_redoubt("editor-delete-room");
    let before = ed.session.cells(REDOUBT);
    let room = ed.room_piece(REDOUBT, CEILING_CELL).expect("its piece");
    assert_eq!(room.cells.len(), 10);
    // Standing in its middle, looking up at its ceiling: a click selects it.
    let m = middle(&ed, REDOUBT, 0x0185);
    put_camera(&mut ed, REDOUBT, Vec3::new(m.x, m.y, m.z - 0.5), 0.0, 80.0);
    for _ in 0..3 {
        ed.frame();
    }
    keep(&mut ed, "delete", "01-looking-up.png");
    let w = ed.world_rect.unwrap();
    let s = ed.ui_scale();
    click(&mut ed, (w.center().x * s, w.center().y * s));
    let Some(Selection::Cell { landblock, cell }) = ed.selection.clone() else {
        panic!("a room selected: {:?}", ed.selection);
    };
    assert_eq!(landblock, REDOUBT);
    assert!(room.holds(cell), "{cell:04X} of the room looked up at");
    ed.frame();
    keep(&mut ed, "delete", "02-selected.png");
    // Delete: every part of it closed, what stood in it gone, the doorways into it free.
    let steps = ed.session.history().0;
    key(&mut ed, 0x2E, true);
    key(&mut ed, 0x2E, false);
    ed.frame();
    assert_eq!(ed.session.history().0, steps + 1, "{}", ed.panels.message);
    let after = ed.session.cells(REDOUBT);
    for n in &room.cells {
        let c = &after[n];
        assert!(
            c.frame.origin.z <= lyceum_foundry::cells::CLOSED_DEPTH + 1.0
                && c.portals.is_empty()
                && c.static_objects.is_empty(),
            "{n:04X}"
        );
    }
    let free = ed.free_portals(REDOUBT, &[]);
    assert_eq!(free.len(), 3, "{free:?}");
    assert!(ed.selection.is_none());
    for _ in 0..2 {
        ed.frame();
    }
    assert!(
        ed.layout.doorway_dots.len() >= 2,
        "the free doorways on the grid"
    );
    keep(&mut ed, "delete", "03-deleted.png");
    // One undo: as it was.
    key(&mut ed, 0x11, true);
    key(&mut ed, 0x5A, true);
    key(&mut ed, 0x5A, false);
    key(&mut ed, 0x11, false);
    ed.frame();
    assert_eq!(ed.session.cells(REDOUBT), before);
    // The ceiling's own cell, chosen from the list, deletes too: its whole room.
    ed.choose_cell(REDOUBT, CEILING_CELL).unwrap();
    ed.frame();
    let del = ed.panels.delete_room.expect("the room card's Delete");
    click(&mut ed, (del.center().x * s, del.center().y * s));
    assert!(ed.session.cells(REDOUBT)[&CEILING_CELL].frame.origin.z < -1000.0);
}

#[test]
fn a_free_doorway_chosen_on_the_grid_lists_what_fits_there_best_first_its_ghost_drawn_and_a_click_puts_one_in(
) {
    let _g = gpu_lock();
    let (_dir, mut ed) = into_redoubt("editor-fits");
    let room = ed.room_piece(REDOUBT, CEILING_CELL).unwrap();
    let env = room.environment;
    ed.delete_room(REDOUBT, CEILING_CELL).unwrap();
    // The camera where the room stood, the grid showing its floor.
    let m = middle(&ed, REDOUBT, 0x0177);
    put_camera(&mut ed, REDOUBT, m, 180.0, -20.0);
    for _ in 0..3 {
        ed.frame();
    }
    // A free doorway's dot clicked: the palette lists what fits there, best first.
    let (link, at) = ed
        .layout
        .doorway_dots
        .iter()
        .find(|(l, _)| matches!(l, lyceum_foundry::source::Link::Base { cell: 0x0177, .. }))
        .cloned()
        .expect("the corridor's doorway into the room taken away");
    let s = ed.ui_scale();
    click(&mut ed, (at.x * s, at.y * s));
    assert_eq!(ed.layout.target, Some(Target::Doorway(link)));
    for _ in 0..2 {
        ed.frame();
    }
    let fits = ed.fits(REDOUBT);
    assert!(!fits.is_empty(), "{}", ed.panels.message);
    assert!(
        fits.windows(2).all(|w| w[0].score >= w[1].score),
        "best first"
    );
    let pieces = ed.kit.pieces.clone().unwrap();
    // The room that stood there is among them, joining all three doorways.
    let back = fits
        .iter()
        .position(|f| pieces[f.piece].environment == env)
        .expect("the room taken away fits back");
    assert_eq!(fits[back].placement.joins(), 3);
    assert_eq!(back, 0, "joining the most, it comes first");
    println!(
        "{} fit: {:?}",
        fits.len(),
        fits.iter()
            .map(|f| (pieces[f.piece].label(), f.score))
            .collect::<Vec<_>>()
    );
    keep(&mut ed, "fits", "01-doorway-chosen.png");
    // The pointer over its tile: its ghost on the grid and in the world view.
    let before = capture(&mut ed, _dir.path());
    let (_, tile) = ed.kit.fit_rects[back];
    hover(&mut ed, (tile.center().x * s, tile.center().y * s));
    ed.frame();
    assert!(ed.layout.ghost.as_ref().is_some_and(|g| g.fits()));
    assert!(ed.ghost_draw.triangles > 0);
    let after = capture(&mut ed, _dir.path());
    let w = ed.world_rect.unwrap();
    let changed = super::difference(
        &before,
        &after,
        (w.center().x * s, w.center().y * s),
        w.width().min(w.height()) * s * 0.45,
    );
    assert!(
        changed > 5.0,
        "the ghost drawn in the world view: {changed}"
    );
    keep(&mut ed, "fits", "02-ghost.png");
    // A click puts it in: every doorway joined again.
    click(&mut ed, (tile.center().x * s, tile.center().y * s));
    assert!(
        ed.free_portals(REDOUBT, &[]).is_empty(),
        "{}",
        ed.panels.message
    );
    assert_eq!(ed.session.cell_names(REDOUBT).len(), 10);
    let walk = ed.session.walk(REDOUBT);
    assert!(walk.is_empty(), "{walk:#?}");
    ed.frame();
    keep(&mut ed, "fits", "03-put-in.png");
}

#[test]
fn in_dungeon_mode_a_click_in_the_world_view_only_selects_even_with_a_piece_chosen() {
    let _g = gpu_lock();
    let (_dir, mut ed) = into_redoubt("editor-select-only");
    let m = middle(&ed, REDOUBT, 0x0185);
    put_camera(&mut ed, REDOUBT, m, 0.0, -30.0);
    let pieces = ed.kit.pieces.clone().unwrap();
    let corridor = (0..pieces.len())
        .find(|i| {
            pieces[*i].form.kind == Kind::Corridor && pieces[*i].in_look(ed.kit.look.unwrap())
        })
        .unwrap();
    super::choose_piece(&mut ed, corridor);
    let cells = ed.session.cells(REDOUBT).len();
    let w = ed.world_rect.unwrap();
    let s = ed.ui_scale();
    click(&mut ed, (w.center().x * s, w.center().y * s));
    assert_eq!(ed.session.cells(REDOUBT).len(), cells, "nothing placed");
    assert!(
        matches!(ed.selection, Some(Selection::Cell { .. })),
        "{:?}",
        ed.selection
    );
}

#[test]
fn from_a_room_on_the_floor_above_a_click_through_its_floor_selects_the_room_below_not_the_ground_at_its_height(
) {
    let _g = gpu_lock();
    let (_dir, mut ed) = into_redoubt("editor-pick-above");
    // In 019B, whose floor is the sea floor's height and opens down into the room below.
    let m = middle(&ed, REDOUBT, 0x019B);
    put_camera(&mut ed, REDOUBT, Vec3::new(m.x, m.y, m.z), 0.0, -85.0);
    for _ in 0..3 {
        ed.frame();
    }
    let ground = ed.session.ground(REDOUBT, m.x, m.y).unwrap();
    assert!(
        ground.abs() < 0.5,
        "the ground at the floor's height: {ground}"
    );
    let w = ed.world_rect.unwrap();
    let s = ed.ui_scale();
    click(&mut ed, (w.center().x * s, w.center().y * s));
    let Some(Selection::Cell { landblock, cell }) = ed.selection.clone() else {
        panic!("a room below selected: {:?}", ed.selection);
    };
    assert_eq!(landblock, REDOUBT);
    assert!(
        middle(&ed, REDOUBT, cell).z < 0.0,
        "{cell:04X} is below the floor"
    );
    keep(&mut ed, "pick", "through-the-floor.png");
}

#[test]
fn in_dungeon_mode_the_world_view_draws_only_the_dungeon_with_the_camera_outside_its_rooms() {
    let _g = gpu_lock();
    let (_dir, mut ed) = into_redoubt("editor-dungeon-only");
    // High over the redoubt, outside every room, looking down at it.
    let m = middle(&ed, REDOUBT, 0x019B);
    put_camera(
        &mut ed,
        REDOUBT,
        Vec3::new(m.x - 40.0, m.y - 60.0, 70.0),
        20.0,
        -45.0,
    );
    for _ in 0..5 {
        ed.frame();
    }
    assert!(ed.camera_cell().is_none(), "the camera outside its rooms");
    keep(&mut ed, "dungeon-only", "from-outside.png");
    let drawn: BTreeSet<u32> = ed.cells_seen().into_keys().collect();
    println!(
        "drawn {:?}",
        drawn.iter().map(|c| format!("{c:08X}")).collect::<Vec<_>>()
    );
    let mine = drawn
        .iter()
        .filter(|c| **c >> 16 == u32::from(REDOUBT))
        .count();
    let all = ed.session.cells(REDOUBT).len();
    assert!(mine * 2 > all, "{mine} of its {all} rooms drawn");
    assert_eq!(drawn.len(), mine, "nothing but its rooms");
    keep(&mut ed, "dungeon-only", "from-outside.png");
    // Left: the world again.
    ed.leave_dungeon();
    for _ in 0..3 {
        ed.frame();
    }
    assert!(ed
        .cells_seen()
        .keys()
        .all(|c| *c >> 16 != u32::from(REDOUBT)));
}

/// The kind of what fit `f` sets in.
fn kind_of(ed: &EditorApp, f: &Fit) -> Kind {
    ed.kit.pieces.as_ref().unwrap()[f.piece].form.kind
}

/// Whether fit `f` sets in a level corridor whose two doorways face north and south.
pub fn north_south(ed: &EditorApp, f: &Fit) -> bool {
    let p = &ed.kit.pieces.as_ref().unwrap()[f.piece];
    let ends: Vec<Vec3> = p
        .form
        .doors
        .iter()
        .filter(|d| d.side.is_some())
        .map(|d| lyceum_foundry::grid::placed(&d.opening, &f.placement.frame).middle)
        .collect();
    p.form.kind == Kind::Corridor
        && p.form.rise == 0.0
        && ends.len() == 2
        && (ends[0].y - ends[1].y).abs() > (ends[0].x - ends[1].x).abs()
}

/// The grid panned to free doorway `link` (its middle at `at`, landblock meters), and its dot
/// clicked: what fits there listed.
pub fn choose_doorway(ed: &mut EditorApp, link: &Link, at: Vec3) {
    ed.layout.center = Some((at.x, at.y));
    for _ in 0..2 {
        ed.frame();
    }
    let (_, dot) = ed
        .layout
        .doorway_dots
        .iter()
        .find(|(l, _)| l == link)
        .cloned()
        .unwrap_or_else(|| panic!("the dot of {link:?}"));
    let s = ed.ui_scale();
    click(ed, (dot.x * s, dot.y * s));
    assert_eq!(ed.layout.target, Some(Target::Doorway(link.clone())));
}

/// Of the fits listed at the grid's choice, best first, the first `want` takes, the list scrolled
/// down with the wheel till it is in view; its ghost shown, then clicked in: its piece.
pub fn put_fit(ed: &mut EditorApp, lb: u16, want: &dyn Fn(&EditorApp, &Fit) -> bool) -> usize {
    for _ in 0..2 {
        ed.frame();
    }
    let fits = ed.fits(lb);
    assert!(
        fits.iter().any(|f| want(ed, f)),
        "none of the {} fits: {:?}",
        fits.len(),
        fits.iter()
            .map(|f| {
                let p = &ed.kit.pieces.as_ref().unwrap()[f.piece];
                format!(
                    "{} {:?}{}{}",
                    p.label(),
                    p.form.kind,
                    if p.form.above { " up" } else { "" },
                    if p.form.below { " down" } else { "" }
                )
            })
            .collect::<Vec<_>>()
    );
    let s = ed.ui_scale();
    let mut found = None;
    for _ in 0..200 {
        let view = ed.kit.fits_view.expect("the fits' list");
        found = ed
            .kit
            .fit_rects
            .iter()
            .copied()
            .filter(|(_, r)| view.contains(r.center()))
            .find(|(k, _)| want(ed, &fits[*k]));
        if found.is_some() {
            break;
        }
        hover(ed, (view.center().x * s, view.center().y * s));
        ed.queue(dereth_input::host::HostEvent::MouseWheel { notches: -1.0 });
        for _ in 0..2 {
            ed.frame();
        }
    }
    let (k, tile) = found.unwrap_or_else(|| {
        panic!(
            "fit {:?} of {} scrolled into view: view {:?}, tiles {:?}",
            fits.iter().position(|f| want(ed, f)),
            fits.len(),
            ed.kit.fits_view,
            ed.kit.fit_rects.iter().take(6).collect::<Vec<_>>()
        )
    });
    let steps = ed.session.history().0;
    hover(ed, (tile.center().x * s, tile.center().y * s));
    ed.frame();
    assert!(ed.layout.ghost.as_ref().is_some_and(|g| g.fits()));
    click(ed, (tile.center().x * s, tile.center().y * s));
    assert_eq!(
        ed.session.history().0,
        steps + 1,
        "set in as one step: {}",
        ed.panels.message
    );
    fits[k].piece
}

/// The free doorway of dungeon `lb` farthest along `toward` (a level direction).
fn farthest_doorway(ed: &EditorApp, lb: u16, toward: Vec3) -> (Link, Vec3) {
    ed.free_portals(lb, &[])
        .into_iter()
        .map(|d| (d.link, d.opening.middle))
        .max_by(|a, b| {
            let along = |p: Vec3| p.x * toward.x + p.y * toward.y;
            along(a.1).total_cmp(&along(b.1))
        })
        .expect("a free doorway")
}

/// A new dungeon near Holtburg in Holtburg Redoubt's look, built with what fits here alone: a
/// level corridor running north and south on a square clicked on the grid; north of it another,
/// a crossing and a third; south of it two more; and a dead end on each doorway left free. Its
/// landblock, and the pieces set in, in order.
pub fn build_with_fits(ed: &mut EditorApp, shots_of: &str) -> (u16, Vec<usize>) {
    let lb = ed.new_dungeon(Some(HOLTBURG)).unwrap();
    ed.choose_dungeon_look(lb, Some(REDOUBT)).unwrap();
    ed.kit.chosen = None;
    for _ in 0..2 {
        ed.frame();
    }
    let mut set = Vec::new();
    // An empty square clicked: of every piece of the look, a corridor running north.
    let g = grid_point(ed, lyceum_foundry::grid::square_middle((9, 9)));
    click(ed, g);
    assert_eq!(ed.layout.target, Some(Target::Square((9, 9))));
    set.push(put_fit(ed, lb, &north_south));
    keep(ed, shots_of, "01-first-corridor.png");
    let north = Vec3::new(0.0, 1.0, 0.0);
    let south = Vec3::new(0.0, -1.0, 0.0);
    let corridor = |ed: &EditorApp, f: &Fit| kind_of(ed, f) == Kind::Corridor && north_south(ed, f);
    let cross = |ed: &EditorApp, f: &Fit| {
        kind_of(ed, f) == Kind::Cross && ed.kit.pieces.as_ref().unwrap()[f.piece].form.rise == 0.0
    };
    for (toward, want) in [
        (north, &corridor as &dyn Fn(&EditorApp, &Fit) -> bool),
        (north, &cross),
        (north, &corridor),
        (south, &corridor),
        (south, &corridor),
    ] {
        let (link, at) = farthest_doorway(ed, lb, toward);
        choose_doorway(ed, &link, at);
        set.push(put_fit(ed, lb, want));
    }
    keep(ed, shots_of, "02-corridors-and-crossing.png");
    // Each doorway left: what fits it, a dead end.
    let dead_end = |ed: &EditorApp, f: &Fit| kind_of(ed, f) == Kind::DeadEnd;
    while let Some(d) = ed.free_portals(lb, &[]).into_iter().next() {
        choose_doorway(ed, &d.link, d.opening.middle);
        if set.len() == 7 {
            ed.frame();
            keep(ed, shots_of, "03-what-fits-a-doorway.png");
        }
        set.push(put_fit(ed, lb, &dead_end));
        assert!(set.len() < 16, "the doorways keep coming");
    }
    ed.frame();
    keep(ed, shots_of, "04-closed.png");
    (lb, set)
}

#[test]
fn a_new_dungeon_of_ten_pieces_is_built_with_what_fits_here_alone_and_every_doorway_joins_and_walks(
) {
    let _g = gpu_lock();
    let (_dir, world) = sample("editor-build");
    let mut ed = editor_with(&world, &["--size", "1600x1000"]);
    ed.wait_for_pieces();
    let (lb, set) = build_with_fits(&mut ed, "build");
    let pieces = ed.kit.pieces.clone().unwrap();
    println!(
        "built {lb:04X} of {:?}",
        set.iter()
            .map(|i| (pieces[*i].label(), pieces[*i].form.kind))
            .collect::<Vec<_>>()
    );
    assert!(set.len() >= 8, "{} pieces", set.len());
    let placed = lyceum_foundry::kit::placed_pieces(&ed.session.cells(lb));
    assert_eq!(placed.len(), set.len());
    assert!(ed.free_portals(lb, &[]).is_empty());
    let walk = ed.session.walk(lb);
    assert!(walk.is_empty(), "{walk:#?}");
    // From outside, above it: the dungeon alone, all of it.
    let o = lyceum_foundry::pick::block_origin(lb);
    let first = middle(&ed, lb, lyceum_foundry::cells::FIRST_CELL);
    put_camera(
        &mut ed,
        lb,
        Vec3::new(first.x - 25.0, first.y - 45.0, first.z + 40.0),
        -25.0,
        -40.0,
    );
    for _ in 0..3 {
        ed.frame();
    }
    assert!(ed.camera_cell().is_none());
    let drawn = ed.cells_seen();
    let all = ed.session.cells(lb).len();
    assert_eq!(drawn.len(), all, "{drawn:?}");
    keep(&mut ed, "build", "05-from-outside.png");
    // In the first room, looking north along it.
    ed.camera.position = Vec3::new(o.x + first.x, o.y + first.y, o.z + first.z);
    ed.camera.yaw = 0.0;
    ed.camera.pitch = -0.05;
    for _ in 0..3 {
        ed.frame();
    }
    keep(&mut ed, "build", "06-inside-looking-north.png");
}

#[cfg(feature = "live-server-tests")]
#[test]
fn a_dungeon_built_with_what_fits_here_is_walked_through_from_a_portal_placed_in_holtburg() {
    use std::path::PathBuf;
    let _g = gpu_lock();
    let (_dir, world) = sample("live-built-dungeon");
    let server =
        PathBuf::from(std::env::var_os("EMPYREAN_SERVER_BIN").expect("EMPYREAN_SERVER_BIN"));
    let sql = PathBuf::from(std::env::var_os("EMPYREAN_WORLD_SQL").expect("EMPYREAN_WORLD_SQL"));
    let mut ed = editor_with(
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
    let (lb, set) = build_with_fits(&mut ed, "live-built-dungeon");
    assert!(set.len() >= 8);
    ed.leave_dungeon();
    // A portal set on Holtburg's ground with the spawn tool, sent into the new dungeon.
    super::stone_west(&mut ed);
    ed.mode = lyceum_editor::app::Mode::Spawn;
    for _ in 0..2 {
        ed.frame();
    }
    let tile = ed.panels.portal_rect.expect("the portal's tile");
    click(&mut ed, (tile.center().x, tile.center().y));
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
    ed.destination_dungeon(lb).unwrap();
    let spawn = ed
        .session
        .spawns()
        .into_iter()
        .find(|x| x.file == file && x.guid == guid)
        .unwrap();
    let to = ed
        .portal_destination(&spawn)
        .flatten()
        .expect("a destination");
    assert_eq!(to >> 16, u32::from(lb));
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
    let at = spawn.frame.origin;
    ed.look_at(HOLTBURG, Vec3::new(at.x, at.y, at.z + 1.6), 4.0, 0.0, 0.0);
    ed.frame();
    ed.set_live(true).unwrap();
    ed.wait_live().unwrap();
    ed.play_options.interface = lyceum_editor::app::PlayInterface::Modern;
    ed.play_until_entered(&["--use".to_owned(), format!("0x{id:08X}")])
        .unwrap();
    let start = std::time::Instant::now();
    let arrived = loop {
        assert!(ed.frame());
        if let Some(c) = ed
            .play_body()
            .map(|b| b.cell.0)
            .filter(|c| c >> 16 == u32::from(lb))
        {
            break c;
        }
        // Only against a hang: the portal taken is what ends the wait.
        assert!(
            start.elapsed() < std::time::Duration::from_secs(120),
            "the character did not go through: {:?}",
            ed.play_body()
        );
    };
    for _ in 0..120 {
        assert!(ed.frame());
    }
    keep(&mut ed, "live-built-dungeon", "10-arrived.png");
    // Walked on along the corridors, room after room.
    let forward = dereth_input::keys::Key::new(0x57, 0x11);
    ed.queue(dereth_input::host::HostEvent::KeyboardInput {
        key: forward,
        pressed: true,
        text: None,
    });
    let mut rooms: Vec<u32> = vec![arrived];
    let start = std::time::Instant::now();
    while rooms.len() < 4 {
        assert!(ed.frame());
        if let Some(c) = ed.play_body().map(|b| b.cell.0) {
            assert_eq!(c >> 16, u32::from(lb), "still in the dungeon");
            if !rooms.contains(&c) {
                rooms.push(c);
                if rooms.len() == 3 {
                    keep(&mut ed, "live-built-dungeon", "11-walking.png");
                }
            }
        }
        // Only against a hang: the rooms walked through are what end the loop.
        assert!(
            start.elapsed() < std::time::Duration::from_secs(120),
            "walked through {rooms:08X?} only: {:?}",
            ed.play_body()
        );
    }
    ed.queue(dereth_input::host::HostEvent::KeyboardInput {
        key: forward,
        pressed: false,
        text: None,
    });
    for _ in 0..30 {
        assert!(ed.frame());
    }
    keep(&mut ed, "live-built-dungeon", "12-walked.png");
    println!("walked through {rooms:08X?}");
    // In the world, the status bar no longer says the playtest is starting.
    assert!(
        !ed.panels.message.contains("playtest"),
        "{}",
        ed.panels.message
    );
    ed.stop_play();
    assert!(ed.wait_world_empty(), "the character logged off");
    ed.set_live(false).unwrap();
}

#[test]
fn another_look_chosen_restyles_every_room_of_holtburg_redoubt_by_the_worlds_list_where_it_has_one_else_by_what_each_surface_covers(
) {
    use lyceum_foundry::kit::{dress, placed_pieces, Dressed};
    let _g = gpu_lock();
    let (dir, mut ed) = into_redoubt("editor-restyle");
    let own = ed.dungeon_look(REDOUBT).unwrap();
    let pieces = ed.kit.pieces.clone().unwrap();
    let before = ed.session.cells(REDOUBT);
    let placed = placed_pieces(&before);
    // For every other look the card offers: how many of its rooms the world's own list dresses,
    // and how many go by what each surface covers; none is kept as it is.
    let mut counts: Vec<(usize, u16, usize, usize)> = Vec::new();
    let offered = (0..ed.kit.families.len())
        .filter(|f| ed.offered_look(*f))
        .count();
    for f in 0..ed.kit.families.len() {
        if f == own || !ed.offered_look(f) {
            continue;
        }
        let Some(of) = ed.kit.families[f]
            .landblocks
            .iter()
            .map(|(l, _)| *l)
            .find(|l| ed.family_of_landblock(*l) == Some(f))
        else {
            continue;
        };
        let (mut exact, mut role, mut kept) = (0, 0, 0);
        for p in &placed {
            let k = pieces
                .iter()
                .find(|k| k.environment == p.environment)
                .unwrap();
            match dress(k, f, ed.kit.dress.get(f)).1 {
                Dressed::Exact => exact += p.cells.len(),
                Dressed::ByRole => role += p.cells.len(),
                Dressed::Kept => kept += p.cells.len(),
            }
        }
        assert_eq!(kept, 0, "the look of {of:04X} draws every role");
        counts.push((f, of, exact, role));
    }
    let rooms = placed.iter().map(|p| p.cells.len()).sum::<usize>();
    let all_exact = counts.iter().filter(|c| c.3 == 0).count();
    let none_exact = counts.iter().filter(|c| c.2 == 0).count();
    let (exact, role) = counts.iter().fold((0, 0), |(e, r), c| (e + c.2, r + c.3));
    println!(
        "{} other looks of the {offered} offered for the redoubt's {rooms} rooms: {all_exact} draw \
         every room by the world's list, {none_exact} none, the rest some; {exact} rooms by a \
         list and {role} by role in all",
        counts.len()
    );
    // The look whose list covers the most of it, and the one that leaves the most to the roles.
    let most_exact = *counts
        .iter()
        .max_by_key(|c| (c.2, std::cmp::Reverse(c.0)))
        .unwrap();
    let most_role = *counts
        .iter()
        .max_by_key(|c| (c.3, std::cmp::Reverse(c.0)))
        .unwrap();
    assert_ne!(most_exact.0, most_role.0);
    // In its great room, looking along it.
    let m = middle(&ed, REDOUBT, 0x0185);
    put_camera(
        &mut ed,
        REDOUBT,
        Vec3::new(m.x, m.y, m.z - 0.8),
        90.0,
        -12.0,
    );
    for _ in 0..3 {
        ed.frame();
    }
    keep(&mut ed, "restyle", "00-its-own-look.png");
    let own_frame = capture(&mut ed, dir.path());
    let steps = ed.session.history().0;
    for (n, (f, of, exact, role)) in [most_exact, most_role].into_iter().enumerate() {
        ed.choose_dungeon_look(REDOUBT, Some(of)).unwrap();
        println!(
            "the look of {of:04X} ({}): {exact} rooms by its list, {role} by role: {}",
            ed.family_name(f),
            ed.panels.message
        );
        assert!(
            ed.panels.message.contains(&exact.to_string())
                && ed.panels.message.contains(&role.to_string()),
            "{}",
            ed.panels.message
        );
        assert_eq!(ed.kit.look, Some(f));
        assert_eq!(ed.dungeon_look(REDOUBT), Some(f));
        let now = ed.session.cells(REDOUBT);
        for p in &placed {
            let k = pieces
                .iter()
                .find(|k| k.environment == p.environment)
                .unwrap();
            let (surfaces, _) = dress(k, f, ed.kit.dress.get(f));
            for c in &p.cells {
                assert_eq!(now[c].surfaces, surfaces, "{c:04X}");
            }
        }
        assert_eq!(ed.session.history().0, steps + n + 1, "one step each");
        for _ in 0..3 {
            ed.frame();
        }
        let frame = capture(&mut ed, dir.path());
        let w = ed.world_rect.unwrap();
        let s = ed.ui_scale();
        let changed = super::difference(
            &own_frame,
            &frame,
            (w.center().x * s, w.center().y * s),
            w.width().min(w.height()) * s * 0.45,
        );
        assert!(changed > 10.0, "drawn in the new look: {changed}");
        keep(
            &mut ed,
            "restyle",
            &format!(
                "{:02}-look-of-{of:04X}-{exact}-exact-{role}-by-role.png",
                n + 1
            ),
        );
    }
    // Undone twice: its own look again, every room as the world draws it.
    ed.edit(&lyceum_foundry::script::Action::Undo).unwrap();
    ed.edit(&lyceum_foundry::script::Action::Undo).unwrap();
    assert_eq!(ed.session.cells(REDOUBT), before);
}

#[test]
fn renaming_a_dungeon_renames_the_portals_into_it_the_worlds_by_the_packs_copies_and_the_packs_own()
{
    let _g = gpu_lock();
    let (dir, world) = sample("editor-rename");
    let sql = dir.path().join("server.sql");
    let server = format!("{}{}", super::SERVER_PORTALS, super::HIDEOUT_ENTRANCES);
    std::fs::write(&sql, &server).unwrap();
    let mut ed = editor_with(
        &world,
        &["--weenies", sql.to_str().unwrap(), "--size", "1400x1000"],
    );
    super::stone_west(&mut ed);
    ed.wait_for_names().unwrap();
    // A portal of the pack's own, sent into the hideout beside the world's two.
    ed.mode = lyceum_editor::app::Mode::Spawn;
    for _ in 0..2 {
        ed.frame();
    }
    let tile = ed.panels.portal_rect.expect("the portal's tile");
    click(&mut ed, (tile.center().x, tile.center().y));
    let w = ed.world_rect.unwrap();
    let s = ed.ui_scale();
    click(
        &mut ed,
        (w.center().x * s, (w.center().y + w.height() * 0.2) * s),
    );
    ed.destination_dungeon(0x019E).unwrap();
    let into = ed.portal_names(0x019E);
    assert_eq!(into.len(), 3, "{into:?}");
    // Renamed: each portal called by the new name where it said the old one, else by it alone.
    let steps = ed.session.history().0;
    ed.name_dungeon(0x019E, "Warren of Tests").unwrap();
    assert_eq!(ed.session.history().0, steps + 1, "one step");
    let mut names: Vec<String> = ed
        .portal_names(0x019E)
        .into_iter()
        .map(|(_, n)| n)
        .collect();
    names.sort();
    assert_eq!(
        names,
        ["Warren of Tests", "Warren of Tests", "Warren of Tests"],
        "{}",
        ed.panels.message
    );
    // The world's own by the pack's copies of their weenies; the server's file as it was.
    let copies = ed.session.pack_classes();
    assert!(copies.contains_key(&90010) && copies.contains_key(&90011));
    assert_eq!(std::fs::read_to_string(&sql).unwrap(), server);
    // The list calls it so.
    ed.show_tab(lyceum_editor::app::Tab::Dungeons);
    ed.dungeon_list.filter = "warren".into();
    for _ in 0..2 {
        ed.frame();
    }
    assert!(ed.dungeon_list.rects.iter().any(|(lb, _)| *lb == 0x019E));
    keep(&mut ed, "rename", "list.png");
    // One undo: the old names.
    ed.edit(&lyceum_foundry::script::Action::Undo).unwrap();
    let mut names: Vec<String> = ed
        .portal_names(0x019E)
        .into_iter()
        .map(|(_, n)| n)
        .collect();
    names.sort();
    assert_eq!(names, ["Drudge Hideout", "Hideout Back Door", "Portal"]);
}

/// The Dungeons tab filtered to landblock `lb`, and the Delete of its row clicked.
fn delete_from_list(ed: &mut EditorApp, lb: u16) {
    ed.show_tab(lyceum_editor::app::Tab::Dungeons);
    ed.dungeon_list.filter = format!("{lb:04x}");
    for _ in 0..2 {
        ed.frame();
    }
    let (_, del) = ed
        .dungeon_list
        .delete_rects
        .iter()
        .find(|(l, _)| *l == lb)
        .copied()
        .expect("its row's Delete");
    let s = ed.ui_scale();
    click(ed, (del.center().x * s, del.center().y * s));
}

#[test]
fn a_packs_dungeon_is_deleted_whole_with_its_portal_and_the_worlds_own_once_the_question_naming_what_goes_is_answered(
) {
    let _g = gpu_lock();
    let (dir, world) = sample("editor-delete-dungeon");
    let sql = dir.path().join("server.sql");
    std::fs::write(
        &sql,
        format!("{}{}", super::SERVER_PORTALS, super::HIDEOUT_ENTRANCES),
    )
    .unwrap();
    let mut ed = editor_with(
        &world,
        &["--weenies", sql.to_str().unwrap(), "--size", "1600x1000"],
    );
    ed.wait_for_names().unwrap();
    ed.wait_for_pieces();
    // The pack's own: a corridor in a new dungeon, and a portal into it on Holtburg's ground.
    let lb = ed.new_dungeon(Some(HOLTBURG)).unwrap();
    ed.choose_dungeon_look(lb, Some(REDOUBT)).unwrap();
    for _ in 0..2 {
        ed.frame();
    }
    let g = grid_point(&ed, lyceum_foundry::grid::square_middle((9, 9)));
    click(&mut ed, g);
    put_fit(&mut ed, lb, &north_south);
    ed.leave_dungeon();
    super::stone_west(&mut ed);
    ed.mode = lyceum_editor::app::Mode::Spawn;
    for _ in 0..2 {
        ed.frame();
    }
    let tile = ed.panels.portal_rect.expect("the portal's tile");
    click(&mut ed, (tile.center().x, tile.center().y));
    let w = ed.world_rect.unwrap();
    let s = ed.ui_scale();
    click(
        &mut ed,
        (w.center().x * s, (w.center().y + w.height() * 0.2) * s),
    );
    let Some(Selection::Spawn { file, .. }) = ed.selection.clone() else {
        panic!(
            "the portal selected: {:?} {}",
            ed.selection, ed.panels.message
        );
    };
    ed.mode = lyceum_editor::app::Mode::Select;
    assert_eq!(ed.portal_names(lb).len(), 1);
    assert_eq!(ed.entrances(lb).len(), 1);
    let steps = ed.session.history().0;
    // Its row's Delete: gone at once, with its portal's spawn and weenie.
    delete_from_list(&mut ed, lb);
    assert_eq!(ed.session.history().0, steps + 1, "{}", ed.panels.message);
    assert!(ed.session.cell_names(lb).is_empty());
    assert!(ed.portal_names(lb).is_empty());
    assert!(ed.session.spawns().iter().all(|x| x.file != file));
    assert!(!ed.dungeons().iter().any(|d| d.landblock == lb));
    ed.frame();
    keep(&mut ed, "delete-dungeon", "01-packs-deleted.png");
    // The world's own: its row's Delete asks first, naming what goes.
    let (rooms, _, portals) = ed.dungeon_contents(0x019E);
    assert_eq!(portals, 3);
    delete_from_list(&mut ed, 0x019E);
    assert_eq!(ed.dungeon_list.confirm, Some(0x019E));
    assert_eq!(ed.session.history().0, steps + 1, "nothing taken yet");
    ed.frame();
    keep(&mut ed, "delete-dungeon", "02-the-question.png");
    let yes = ed
        .dungeon_list
        .confirm_button
        .expect("the question's Delete");
    click(&mut ed, (yes.center().x * s, yes.center().y * s));
    assert_eq!(ed.session.history().0, steps + 2, "{}", ed.panels.message);
    let left = ed
        .session
        .cells(0x019E)
        .values()
        .filter(|c| c.frame.origin.z > lyceum_foundry::cells::CLOSED_DEPTH + 1.0)
        .count();
    assert_eq!(left, 0, "of its {rooms} rooms");
    assert!(ed.entrances(0x019E).is_empty());
    assert!(!ed.dungeons().iter().any(|d| d.landblock == 0x019E));
    ed.frame();
    keep(&mut ed, "delete-dungeon", "03-worlds-deleted.png");
    // One undo: back, with its portals.
    ed.edit(&lyceum_foundry::script::Action::Undo).unwrap();
    assert_eq!(ed.entrances(0x019E).len(), 3);
    assert!(ed.dungeons().iter().any(|d| d.landblock == 0x019E));
}

/// The editor on `world` with no landblock on its command line: where the world was left.
fn editor_where_left(world: &std::path::Path) -> EditorApp {
    let cache = std::path::Path::new(env!("CARGO_TARGET_TMPDIR")).join("editor-cache");
    let argv: Vec<String> = [
        world.to_str().unwrap(),
        "--dats",
        dereth_dat::testing::dat_dir().to_str().unwrap(),
        "--headless",
        "--size",
        "1600x1000",
        "--no-weenies",
        "--thumbnails",
        cache.to_str().unwrap(),
    ]
    .iter()
    .map(|s| (*s).to_owned())
    .collect();
    let args = lyceum_editor::args::parse(&argv).unwrap();
    let cfg = lyceum_editor::args::runtime_config(&args).unwrap();
    EditorApp::open(&args, cfg).unwrap()
}

#[test]
fn a_world_opens_again_where_it_was_left_in_its_dungeon_with_its_camera_and_tool_and_leave_goes_back_out(
) {
    use lyceum_editor::app::Mode;
    let _g = gpu_lock();
    let (dir, mut ed) = into_redoubt("editor-left");
    let world = dir.path().join("world.toml");
    let outside = ed.dungeon.unwrap().back;
    let m = middle(&ed, REDOUBT, 0x0185);
    put_camera(&mut ed, REDOUBT, m, 45.0, -10.0);
    ed.mode = Mode::Spawn;
    ed.frame();
    let camera = ed.camera;
    ed.keep_place().unwrap();
    drop(ed);
    // Opened with no landblock named: in the redoubt, the camera and the tool as they were.
    let mut ed = editor_where_left(&world);
    assert_eq!(ed.dungeon_mode(), Some(REDOUBT));
    let near = |a: Vec3, b: Vec3| (a.x - b.x).abs() + (a.y - b.y).abs() + (a.z - b.z).abs() < 0.01;
    assert!(near(ed.camera.position, camera.position), "{:?}", ed.camera);
    assert!(
        (ed.camera.yaw - camera.yaw).abs() < 1e-3 && (ed.camera.pitch - camera.pitch).abs() < 1e-3
    );
    assert_eq!(ed.mode, Mode::Spawn);
    for _ in 0..3 {
        assert!(ed.frame());
    }
    assert!(ed
        .camera_cell()
        .is_some_and(|(c, _)| c.0 >> 16 == u32::from(REDOUBT)));
    keep(&mut ed, "left", "opened-again.png");
    // Leave: out where the camera was before it went in.
    ed.leave_dungeon();
    assert!(near(ed.camera.position, outside.position));
    drop(ed);
    // A landblock named on the command line wins.
    let ed = editor_with(&world, &[]);
    assert_eq!(ed.dungeon_mode(), None);
}

#[test]
fn a_refusal_to_set_a_piece_down_leaves_the_status_bar_once_its_ghost_fits_where_the_pointer_is() {
    let _g = gpu_lock();
    let (_dir, mut ed) = into_redoubt("editor-refusal");
    let room = ed.room_piece(REDOUBT, CEILING_CELL).unwrap();
    ed.delete_room(REDOUBT, CEILING_CELL).unwrap();
    // The camera where the room stood, the grid showing its floor.
    let m = middle(&ed, REDOUBT, 0x0177);
    put_camera(&mut ed, REDOUBT, m, 180.0, -20.0);
    for _ in 0..3 {
        ed.frame();
    }
    let pieces = ed.kit.pieces.clone().unwrap();
    let i = pieces
        .iter()
        .position(|p| p.environment == room.environment)
        .unwrap();
    super::choose_piece(&mut ed, i);
    let marks = ed.piece_marks(REDOUBT, i);
    let mark = marks.first().expect("where it stood");
    let square = mark.square().map_or_else(
        || lyceum_foundry::grid::square_of(mark.frame.origin.x, mark.frame.origin.y),
        |(s, _)| s,
    );
    let fits = lyceum_foundry::grid::square_middle(square);
    for _ in 0..2 {
        ed.frame();
    }
    // A square far from every doorway, clicked: refused, and said so.
    let far = (fits.0 + 120.0, fits.1 + 120.0);
    ed.layout.center = Some(far);
    ed.frame();
    let g = grid_point(&ed, far);
    click(&mut ed, g);
    let refusal = ed.panels.message.clone();
    assert!(!refusal.is_empty());
    ed.frame();
    assert_eq!(ed.panels.message, refusal, "still so while it does not fit");
    // The pointer over where it fits: its ghost fits, and the refusal goes.
    ed.layout.center = Some(fits);
    ed.frame();
    let g = grid_point(&ed, fits);
    hover(&mut ed, g);
    ed.frame();
    assert!(ed.layout.ghost.as_ref().is_some_and(|g| g.fits()));
    assert!(ed.panels.message.is_empty(), "{}", ed.panels.message);
}

/// The level doorways of room `n` of landblock `lb`: each sill's middle and the way out through it.
fn doorways_of(ed: &EditorApp, lb: u16, n: u16) -> Vec<(Vec3, Vec3)> {
    let c = ed.session.cells(lb).remove(&n).unwrap();
    let env = ed.session.environment(c.environment).unwrap();
    let slots = env.cells[usize::from(c.cell_struct)].portals.len();
    (0..slots)
        .filter_map(|s| lyceum_foundry::pieces::opening(&env, c.cell_struct, s, &c.frame))
        .filter(|o| o.out.z.abs() < 0.5)
        .map(|o| {
            let sill = o.corners.iter().map(|p| p.z).fold(f32::MAX, f32::min);
            (Vec3::new(o.middle.x, o.middle.y, sill), o.out)
        })
        .collect()
}

#[test]
fn in_a_dungeons_room_a_door_set_by_a_doorway_stands_in_it_a_chest_on_the_floor_and_ctrl_d_copies_the_selected_beside_it(
) {
    use lyceum_editor::app::Mode;
    let _g = gpu_lock();
    let (dir, world) = sample("editor-snap");
    let sql = dir.path().join("server.sql");
    std::fs::write(&sql, super::SERVER_BLANKS).unwrap();
    let mut ed = editor_with(
        &world,
        &["--weenies", sql.to_str().unwrap(), "--size", "1600x1000"],
    );
    ed.wait_for_names().unwrap();
    ed.wait_for_pieces();
    ed.enter_dungeon(REDOUBT).unwrap();
    let door = dereth_primitives::DataId(0x0200_0DD2);
    assert_eq!(
        ed.names.category(door),
        lyceum_foundry::models::Category::Door
    );
    // In a corridor, facing one of its doorways from four meters in, looking down.
    let lb = REDOUBT;
    let cell = 0x0177;
    let (sill, out) = doorways_of(&ed, lb, cell)[0];
    let o = lyceum_foundry::pick::block_origin(lb);
    let stand = Vec3::new(sill.x - out.x * 4.0, sill.y - out.y * 4.0, sill.z + 1.6);
    let yaw = dereth_primitives::num::math::atan2f(-out.x, out.y).to_degrees();
    put_camera(&mut ed, lb, stand, yaw, -25.0);
    for _ in 0..3 {
        ed.frame();
    }
    let on_screen = |ed: &EditorApp, p: Vec3| {
        ed.view()
            .unwrap()
            .project(Vec3::new(o.x + p.x, o.y + p.y, o.z + p.z))
            .unwrap()
    };
    // A door clicked on the floor a meter short of the doorway: in it, on its sill.
    ed.mode = Mode::Place;
    ed.model = Some(door);
    ed.frame();
    let p = on_screen(&ed, Vec3::new(sill.x - out.x, sill.y - out.y, sill.z));
    click(&mut ed, p);
    let statics = |ed: &EditorApp| ed.session.cells(lb).remove(&cell).unwrap().static_objects;
    let set = statics(&ed);
    let placed = set.last().expect("the door set");
    let at = placed.frame.origin;
    assert!(
        (at.x - sill.x).abs() < 0.05
            && (at.y - sill.y).abs() < 0.05
            && (at.z - sill.z).abs() < 0.05,
        "{at:?} for the sill {sill:?}: {}",
        ed.panels.message
    );
    ed.frame();
    keep(&mut ed, "decorate", "01-door-in-doorway.png");
    // A chest clicked on the floor further in: on the floor.
    ed.model = Some(dereth_primitives::DataId(0x0200_040B));
    ed.frame();
    let p = on_screen(
        &ed,
        Vec3::new(sill.x - out.x * 2.5, sill.y - out.y * 2.5, sill.z),
    );
    click(&mut ed, p);
    let chest = statics(&ed).last().unwrap().frame;
    assert!((chest.origin.z - sill.z).abs() < 0.05, "{chest:?}");
    let Some(Selection::CellStatic { index, .. }) = ed.selection.clone() else {
        panic!("the chest selected: {:?}", ed.selection);
    };
    // Ctrl+D: a copy a meter to the camera's right, as one step, the copy selected.
    let steps = ed.session.history().0;
    let n = statics(&ed).len();
    key(&mut ed, 0x11, true);
    key(&mut ed, 0x44, true);
    key(&mut ed, 0x44, false);
    key(&mut ed, 0x11, false);
    ed.frame();
    assert_eq!(ed.session.history().0, steps + 1, "{}", ed.panels.message);
    let now = statics(&ed);
    assert_eq!(now.len(), n + 1);
    let copy = now.last().unwrap().frame;
    let r = ed.camera.right();
    assert!(
        (copy.origin.x - chest.origin.x - r.x).abs() < 0.01
            && (copy.origin.y - chest.origin.y - r.y).abs() < 0.01
            && (copy.origin.z - chest.origin.z).abs() < 0.01,
        "{chest:?} then {copy:?}"
    );
    assert_eq!(copy.rotation, chest.rotation);
    assert_eq!(
        ed.selection,
        Some(Selection::CellStatic {
            landblock: lb,
            cell,
            index: n
        })
    );
    assert_ne!(index, n);
    ed.frame();
    keep(&mut ed, "decorate", "02-chest-copied.png");
}
