//! The lock-state-to-movable table holds for every window; cascade lock swaps both halves of all
//! ten chrome blocks at three stations; the layout's element types show which half is unlocked;
//! world view hides eight borders when locked; toolbar block has sixteen ids.
//! Fixture: shipped layouts, strings and keymaps loaded from the retail DATs.

use crate::common::layout::RegistrationOrder;

use crate::common::*;
use dereth_primitives::LocalTime;
use dereth_ui::{ElemHandle, ElementId, Screen, UiSystem};
use dereth_ui_screens::hud::floaty::{
    ChromeBlock, ChromePiece, FLOATY_CHROME, GAMEPLAY_WINDOWS, SMART_BOX_CHROME,
};
use dereth_ui_screens::screens::gameplay::GamePlayScreen;

// =============================================================================================
// The live tree.
// =============================================================================================

fn env() -> UiSystem {
    let (ui, _flow, _store) =
        crate::common::layout::load((800, 600), RegistrationOrder::BeforeResolver);
    ui
}

fn pump(ui: &mut UiSystem, s: &mut GamePlayScreen) {
    for _ in 0..16 {
        let batch = ui.drain_outbox();
        if batch.is_empty() {
            return;
        }
        for d in batch {
            match d {
                dereth_ui::Delivery::Element { msg, .. } => {
                    s.on_element_message(&mut dereth_ui::framework::ScreenCx::new(ui), &msg)
                }
                dereth_ui::Delivery::Global { id, param, .. } => {
                    s.on_global_message(&mut dereth_ui::framework::ScreenCx::new(ui), id, param)
                }
                dereth_ui::Delivery::Notice { id, payload, .. } => {
                    s.on_notice(&mut dereth_ui::framework::ScreenCx::new(ui), id, &payload)
                }
            }
        }
    }
    panic!("the message pump did not settle in 16 rounds");
}

fn screen() -> (UiSystem, GamePlayScreen) {
    let mut ui = env();
    let mut s = GamePlayScreen::default();
    s.create(&mut dereth_ui::framework::ScreenCx::new(&mut ui))
        .expect("the gameplay screen builds from the shipped layout");
    pump(&mut ui, &mut s);
    ui.requests.clear();
    (ui, s)
}

/// Every `DragHandle` under `h`, which is every place a player can grab this window.
fn dragbars(ui: &UiSystem, h: ElemHandle) -> Vec<ElemHandle> {
    let mut all = Vec::new();
    walk(ui, h, &mut all);
    all.into_iter()
        .filter(|&e| {
            ui.node(e)
                .is_some_and(|n| n.ty().0 == dereth_ui::factory::ty::DRAGBAR.0)
        })
        .collect()
}

/// Show only `keep` of the sixteen, so nothing else can occlude the handle under test and a
/// "did not move" answer can only be about the lock.
fn isolate(ui: &mut UiSystem, s: &mut GamePlayScreen, keep: ElementId) {
    let root = *s.roots().first().expect("root");
    for w in GAMEPLAY_WINDOWS {
        if let Some(h) = ui.get_child_recursive(root, w.element) {
            let v = w.element == keep;
            ui.set_visible(h, v);
        }
    }
    pump(ui, s);
}

/// Pick a signed delta of magnitude `want` on one axis that the client's mouse-move clamp
/// to `[0, parent - self]` will not eat. Panics if the window is boxed in on that axis, so a
/// measurement that could not have been made never scores as "did not move".
fn room(origin: i32, size: i32, parent: i32, want: i32) -> i32 {
    let forward = parent - size - origin;
    if forward >= want {
        return want;
    }
    if origin >= want {
        return -want;
    }
    panic!(
        "no room to drag: origin {origin}, size {size}, parent {parent} -- \
         {forward} px forward and {origin} px back, wanted {want}"
    );
}

/// Drag moves the window.
fn drag_moves_the_window(ui: &mut UiSystem, win: ElemHandle, bar: ElemHandle) -> bool {
    let parent = ui.parent(bar).expect("a dragbar has a parent");
    assert_eq!(
        parent, win,
        "this layout parents every window dragbar on its window"
    );
    let before = ui.node(win).expect("node").region.box_;
    let grandparent = ui.parent(win).expect("the window has a parent");
    let pb = ui.node(grandparent).expect("node").region.box_;
    let b = ui.screen_box(bar);
    let (gx, gy) = ((b.x0 + b.x1) / 2, (b.y0 + b.y1) / 2);
    let dx = room(before.x0, before.width(), pb.width(), 40);
    let dy = room(before.y0, before.height(), pb.height(), 30);
    ui.mouse_move(LocalTime(1.0), gx, gy);
    ui.mouse_down(dereth_ui::focus::action::PRIMARY_CLICK, gx, gy);
    ui.mouse_move(LocalTime(1.1), gx + dx, gy + dy);
    let after = ui.node(win).expect("node").region.box_;
    ui.mouse_up(
        dereth_ui::focus::action::PRIMARY_CLICK,
        gx + dx,
        gy + dy,
        false,
    );
    assert!(
        !ui.node(win).expect("node").flags.is_moving(),
        "stopping movement must clear the moving flag on release"
    );
    if (after.x0, after.y0) == (before.x0, before.y0) {
        return false;
    }
    // A move is `dragStart + (mouse - mouseInitial)`, not a jump to the pointer.
    assert_eq!(
        (after.x0 - before.x0, after.y0 - before.y0),
        (dx, dy),
        "the window must follow the pointer by the drag delta"
    );
    assert_eq!(
        (after.width(), after.height()),
        (before.width(), before.height()),
        "a drag is not a resize"
    );
    true
}

// =============================================================================================
// What each of the sixteen is expected to do, and why.
// =============================================================================================

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Expect {
    /// Movable while unlocked, immovable while locked — the `<RADA>` behaviour, and what every
    /// window that has a locked-status update must do.
    LockGated,
    /// Movable in **both** states. `FloatingChat` has no locked-status update, no chrome fields
    /// and never registers for global `0x0D`, so its handle is never touched by the lock.
    AlwaysMovable,
    /// The shipped layout gives this window no `DragHandle` at all, in either state.
    /// `WorldView`'s eight chrome children are `ResizeHandle` (type 9); it is resizable,
    /// not draggable, and hides those eight when
    /// locked rather than swapping them.
    NoHandle,
}

/// One drag handle's row of the table: its element id, then its visibility and whether it moved,
/// at each of the three stations.
type Cell = (u32, [bool; 3], [bool; 3]);
/// One window's row: its tag, what it is expected to do, and one [`Cell`] per drag handle.
type Row = (String, Expect, Vec<Cell>);

const EXPECTED: [(&[u8; 6], Expect); 16] = [
    (b"<SBOX>", Expect::NoHandle),
    (b"<CHAT>", Expect::LockGated),
    (b"<FCH1>", Expect::AlwaysMovable),
    (b"<FCH2>", Expect::AlwaysMovable),
    (b"<FCH3>", Expect::AlwaysMovable),
    (b"<FCH4>", Expect::AlwaysMovable),
    (b"<EXAM>", Expect::LockGated),
    (b"<VITS>", Expect::LockGated),
    (b"<SVIT>", Expect::LockGated),
    (b"<ENVP>", Expect::LockGated),
    (b"<PANS>", Expect::LockGated),
    (b"<TBAR>", Expect::LockGated),
    (b"<INDI>", Expect::LockGated),
    (b"<PBAR>", Expect::LockGated),
    (b"<COMB>", Expect::LockGated),
    (b"<RADA>", Expect::LockGated),
];

// =============================================================================================
// The acceptance: the table, at three stations, over every handle.
// =============================================================================================

/// The lock state to movable table holds for every window.
#[test]
fn the_lock_state_to_movable_table_holds_for_every_window() {
    let mut reference: Option<[bool; 3]> = None;
    let mut rows: Vec<Row> = Vec::new();

    for w in GAMEPLAY_WINDOWS {
        let (_, expect) = EXPECTED
            .iter()
            .find(|(t, _)| *t == w.tag)
            .copied()
            .expect("every window in the table has an expectation");
        let (mut ui, mut s) = screen();
        let root = *s.roots().first().expect("root");
        isolate(&mut ui, &mut s, w.element);
        let h = ui.get_child_recursive(root, w.element).unwrap_or_else(|| {
            panic!(
                "{} is not in the shipped layout",
                String::from_utf8_lossy(w.tag)
            )
        });
        let bar_ids: Vec<u32> = dragbars(&ui, h)
            .into_iter()
            .map(|e| ui.node(e).expect("node").element_id().0)
            .collect();

        if expect == Expect::NoHandle {
            assert!(
                bar_ids.is_empty(),
                "{} is expected to have no drag handle and the layout gives it {bar_ids:02X?}",
                String::from_utf8_lossy(w.tag)
            );
            rows.push((
                String::from_utf8_lossy(w.tag).into_owned(),
                expect,
                Vec::new(),
            ));
            continue;
        }
        assert!(
            !bar_ids.is_empty(),
            "{} must have at least one DragHandle for this table to mean anything",
            String::from_utf8_lossy(w.tag)
        );

        let mut cells: Vec<Cell> = Vec::new();
        for id in bar_ids {
            let mut vis = [false; 3];
            let mut moved = [false; 3];
            for (station, locked) in [false, true, false].into_iter().enumerate() {
                s.cascade_lock(&mut ui, locked);
                pump(&mut ui, &mut s);
                let h = ui.get_child_recursive(root, w.element).expect("window");
                let bar = dragbars(&ui, h)
                    .into_iter()
                    .find(|&e| ui.node(e).expect("node").element_id().0 == id)
                    .expect("the same handle at every station");
                vis[station] = ui.node(bar).expect("node").region.flags.visible;
                moved[station] = drag_moves_the_window(&mut ui, h, bar);
            }
            cells.push((id, vis, moved));
        }

        // Every handle of one window must agree with every other handle of that window.
        let first = cells[0].2;
        for (id, _, m) in &cells {
            assert_eq!(
                *m,
                first,
                "{}: handle {id:#010X} disagrees with the rest of its own window",
                String::from_utf8_lossy(w.tag)
            );
        }
        match expect {
            Expect::LockGated => {
                for (id, v, m) in &cells {
                    // The two halves, separately.
                    assert_eq!(
                        *v,
                        [true, false, true],
                        "{} {id:#010X}: the lock update must show the handle only while unlocked",
                        String::from_utf8_lossy(w.tag)
                    );
                    assert_eq!(
                        *m,
                        [true, false, true],
                        "{} {id:#010X}: unlocked must be movable and locked must not, at every station",
                        String::from_utf8_lossy(w.tag)
                    );
                }
            }
            Expect::AlwaysMovable => {
                for (id, v, m) in &cells {
                    assert_eq!(
                        *v,
                        [true; 3],
                        "{} {id:#010X}: FloatingChat never touches its handle",
                        String::from_utf8_lossy(w.tag)
                    );
                    assert_eq!(
                        *m,
                        [true; 3],
                        "{} {id:#010X}: FloatingChat has no lock update, so it moves in both states",
                        String::from_utf8_lossy(w.tag)
                    );
                }
            }
            Expect::NoHandle => unreachable!("handled above"),
        }
        if w.tag == b"<RADA>" {
            reference = Some(cells[0].2);
        }
        rows.push((String::from_utf8_lossy(w.tag).into_owned(), expect, cells));
    }

    // The reference, and the agreement with it.
    let radar = reference.expect("<RADA> must be in the table");
    assert_eq!(
        radar,
        [true, false, true],
        "the polarity reference itself moved wrongly -- fix <RADA> before reading the rest"
    );
    let mut gated = 0_usize;
    for (tag, expect, cells) in &rows {
        if *expect != Expect::LockGated {
            continue;
        }
        gated += 1;
        for (id, _, m) in cells {
            assert_eq!(
                *m, radar,
                "{tag} {id:#010X} does not agree with the minimap"
            );
        }
    }
    // Assert the denominator: a table that silently covered three windows would pass otherwise.
    assert_eq!(rows.len(), 16, "all sixteen windows measured");
    assert_eq!(gated, 11, "eleven of the sixteen are lock-gated");
    let handles: usize = rows.iter().map(|(_, _, c)| c.len()).sum();
    assert_eq!(
        handles, 32,
        "thirty-two drag handles across the sixteen windows"
    );
}

/// Behaviour: ui.lock.locking-hides-every-windows-grab-handles-and-unlocking-shows-them
/// The lock swaps both halves of every chrome block at three stations.
#[test]
fn the_lock_swaps_both_halves_of_every_chrome_block_at_three_stations() {
    let (mut ui, mut s) = screen();
    let root = *s.roots().first().expect("root");
    let mut checked = 0_usize;
    for locked in [false, true, false] {
        s.cascade_lock(&mut ui, locked);
        pump(&mut ui, &mut s);
        for block in FLOATY_CHROME {
            for p in ChromePiece::ALL {
                let u = ui
                    .get_child_recursive(root, block.unlocked(p))
                    .unwrap_or_else(|| panic!("{} {p:?}", block.class));
                let l = ui
                    .get_child_recursive(root, block.locked(p))
                    .unwrap_or_else(|| panic!("{} {p:?} _Locked", block.class));
                assert_eq!(
                    ui.node(u).expect("node").region.flags.visible,
                    !locked,
                    "locked = {locked}: {} {p:?} (the grab handle)",
                    block.class
                );
                assert_eq!(
                    ui.node(l).expect("node").region.flags.visible,
                    locked,
                    "locked = {locked}: {} {p:?} _Locked (the plain border)",
                    block.class
                );
                checked += 2;
            }
        }
    }
    assert_eq!(
        checked, 480,
        "ten blocks x eight pieces x two halves x three stations"
    );
}

/// The structural half, independent of any drag: at every one of the ten chrome blocks the shipped
/// layout puts the **inert** pieces in the low eight and the **grab handles** in the high eight.
///
/// This is the reading that settles which half is which without consulting the lock-status update
/// at all — a second instrument that could have disagreed with retail's update and does not. A
/// `Field` cannot be grabbed; a `DragHandle` or `ResizeHandle` is nothing but a grab. So the half
/// that must be visible while the UI is *un*locked is the high one.
#[test]
fn the_layouts_own_element_types_say_which_half_is_the_unlocked_one() {
    use dereth_ui::factory::ty;
    let (ui, s) = screen();
    let root = *s.roots().first().expect("root");
    let mut low_fields = 0_usize;
    let mut high_handles = 0_usize;
    for block in FLOATY_CHROME {
        for p in ChromePiece::ALL {
            let l = ui
                .get_child_recursive(root, block.locked(p))
                .unwrap_or_else(|| panic!("{} {p:?} _Locked missing", block.class));
            let u = ui
                .get_child_recursive(root, block.unlocked(p))
                .unwrap_or_else(|| panic!("{} {p:?} missing", block.class));
            assert_eq!(
                ui.node(l).expect("node").ty(),
                ty::FIELD,
                "{} {p:?}: the _Locked piece is an inert Field",
                block.class
            );
            low_fields += 1;
            let t = ui.node(u).expect("node").ty();
            assert!(
                t == ty::DRAGBAR || t == ty::RESIZEBAR,
                "{} {p:?}: the unlocked piece must be a grab handle, got {t:?}",
                block.class
            );
            high_handles += 1;
        }
    }
    assert_eq!(low_fields, 80, "ten blocks times eight _Locked pieces");
    assert_eq!(high_handles, 80, "ten blocks times eight unlocked pieces");
}

/// Behaviour: ui.lock.the-world-view-hides-its-borders-while-locked
/// The world view hides its eight borders when the ui is locked.
#[test]
fn the_world_view_hides_its_eight_borders_when_the_ui_is_locked() {
    let (mut ui, mut s) = screen();
    let root = *s.roots().first().expect("root");
    for locked in [false, true, false] {
        s.cascade_lock(&mut ui, locked);
        pump(&mut ui, &mut s);
        for (i, id) in SMART_BOX_CHROME.iter().enumerate() {
            let h = ui
                .get_child_recursive(root, *id)
                .unwrap_or_else(|| panic!("smart box chrome {id:?} is not in the layout"));
            assert_eq!(
                ui.node(h).expect("node").region.flags.visible,
                !locked,
                "locked = {locked}: {:?} ({id:?})",
                ChromePiece::ALL[i]
            );
        }
    }
}

/// The toolbar blocks sixteen ids are all present and in the right halves.
#[test]
fn the_toolbar_blocks_sixteen_ids_are_all_present_and_in_the_right_halves() {
    let (ui, s) = screen();
    let root = *s.roots().first().expect("root");
    let b = FLOATY_CHROME[0];
    assert_eq!(b.class, "FloatingToolbar");
    // The floaty toolbar's post-init, in the order it fetches them.
    for (p, unlocked, locked) in [
        (ChromePiece::Top, 0x1000_062C, 0x1000_0624),
        (ChromePiece::Left, 0x1000_062E, 0x1000_0626),
        (ChromePiece::Bottom, 0x1000_0630, 0x1000_0628),
        (ChromePiece::Right, 0x1000_0632, 0x1000_062A),
        (ChromePiece::TopLeft, 0x1000_062B, 0x1000_0623),
        (ChromePiece::TopRight, 0x1000_062D, 0x1000_0625),
        (ChromePiece::BottomLeft, 0x1000_062F, 0x1000_0627),
        (ChromePiece::BottomRight, 0x1000_0631, 0x1000_0629),
    ] {
        assert_eq!(b.unlocked(p), ElementId(unlocked), "{p:?} unlocked id");
        assert_eq!(b.locked(p), ElementId(locked), "{p:?} _Locked id");
        assert!(
            ui.get_child_recursive(root, ElementId(unlocked)).is_some(),
            "{p:?} unlocked"
        );
        assert!(
            ui.get_child_recursive(root, ElementId(locked)).is_some(),
            "{p:?} _Locked"
        );
        assert_eq!(ChromeBlock::slot(p) + 8, unlocked - b.first, "{p:?} slot");
    }
}
