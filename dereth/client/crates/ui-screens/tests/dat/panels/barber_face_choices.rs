//! The barber's face-choice element is a plain field under the barber panel framing the five part
//! rows, and every row it holds is already reached by the barber panel.
//! Fixture: shipped layouts, strings and keymaps loaded from the retail DATs.

use crate::common::layout::RegistrationOrder;

use crate::common::*;
use dereth_ui::{ElemHandle, ElementId, Screen, UiSystem};
use dereth_ui_screens::panels::barber::{self, PART_ROWS};
use dereth_ui_screens::screens::gameplay::GamePlayScreen;

/// The barber's face-choice element, the subject.
const FACE_CHOICES: ElementId = ElementId(0x1000_059D);
/// The barber's left column, the text-element frame `FACE_CHOICES` hangs under.
const LEFT_COLUMN: ElementId = ElementId(0x1000_05A8);
/// A field element, `dereth_ui::factory::engine::FIELD`.
const FIELD: u32 = 3;
/// A button element.
const BUTTON: u32 = 1;

// ---------------------------------------------------------------------------------------------
// Harness — `lamp_panels.rs`'s, so both files measure the same shipped tree.
// ---------------------------------------------------------------------------------------------

fn env() -> UiSystem {
    let (ui, _flow, _store) =
        crate::common::layout::load((800, 600), RegistrationOrder::BeforeResolver);
    ui
}

fn gameplay() -> (UiSystem, GamePlayScreen) {
    let mut ui = env();
    let mut s = GamePlayScreen::default();
    s.create(&mut dereth_ui::framework::ScreenCx::new(&mut ui))
        .expect("the gameplay screen builds from the shipped layout");
    (ui, s)
}

fn find(ui: &UiSystem, s: &GamePlayScreen, id: ElementId) -> ElemHandle {
    let root = s.root().expect("the gameplay root");
    ui.get_child_recursive(root, id)
        .unwrap_or_else(|| panic!("{:#010X} is not in the shipped tree", id.0))
}

// ---------------------------------------------------------------------------------------------
// 1. What the element is
// ---------------------------------------------------------------------------------------------

/// The shipped layout makes `0x1000059D` a plain field element under the barber's left column;
/// its type and five-child hierarchy prove it is **not** a list box or row host.
#[test]
fn the_face_choices_element_is_a_plain_field_under_the_barber_panel() {
    let (ui, s) = gameplay();
    let panel = find(&ui, &s, barber::PANEL);
    let column = find(&ui, &s, LEFT_COLUMN);
    let h = find(&ui, &s, FACE_CHOICES);

    assert_eq!(
        ty_of(&ui, h),
        FIELD,
        "0x1000059D is authored as engine type {FIELD} (field element); a list box would be 5"
    );
    assert_eq!(
        ui.parent(h),
        Some(column),
        "0x1000059D hangs directly under the barber's left column 0x100005A8"
    );
    assert!(
        ui.is_ancestor_of(panel, h),
        "and the column is inside the barber panel at 0x10000598, so page initialization's \
         recursive descendant lookup by element id finds 0x1000059D"
    );
    // The barber panel opens hidden with the rest of the stack; the frame itself is authored
    // visible, which is why retail's write-only handle had nothing to do to it.
    assert!(
        ui.node(h).expect("alive").region.flags.visible,
        "the frame is authored visible"
    );
}

/// Behaviour: barber.face-choices.the-element-is-an-inert-frame-around-rows-the-panel-already-reaches
/// **The assertion that rejects the card as written.** If `0x1000059D` were "the barber's
/// face-choice list", its children would be face thumbnails and populating it would mean
/// something. They are the five appearance rows `panels/barber.rs` already drives, in the native
/// field order: hair, eyes, nose, mouth, skin.
#[test]
fn the_face_choices_element_is_the_frame_around_the_five_part_rows() {
    let (ui, s) = gameplay();
    let h = find(&ui, &s, FACE_CHOICES);

    let kids: Vec<u32> = ui.children(h).iter().map(|c| id_of(&ui, *c)).collect();
    let rows: Vec<u32> = PART_ROWS.iter().map(|(id, _)| id.0).collect();
    assert_eq!(
        kids, rows,
        "0x1000059D's children are exactly barber::PART_ROWS; there is nothing else under it to \
         populate, and nothing missing from it"
    );
    // Each row is a button, which is what makes the five of them the clickable appearance rows and
    // not list items.
    for (id, _) in PART_ROWS {
        let row = find(&ui, &s, id);
        assert_eq!(
            ty_of(&ui, row),
            BUTTON,
            "{:#010X} is a button element",
            id.0
        );
        assert_eq!(
            ui.parent(row),
            Some(h),
            "{:#010X} hangs under 0x1000059D",
            id.0
        );
    }
}

/// The five rows under it are reachable from the barber panel the way `panels/barber.rs` reaches
/// them — so the frame carries no route the panel's own bindings do not already have, which is the
/// other half of "there is nothing owed here".
#[test]
fn every_row_the_frame_holds_is_already_reached_by_the_barber_panel() {
    let (mut ui, s) = gameplay();
    let root = s.root().expect("the gameplay root");
    let mut p = barber::BarberPanel::default();
    p.post_init(&mut ui, root);

    let panel = find(&ui, &s, barber::PANEL);
    let face = find(&ui, &s, FACE_CHOICES);
    for (id, part) in PART_ROWS {
        let row = ui
            .get_child_recursive(panel, id)
            .unwrap_or_else(|| panic!("{part:?} row {:#010X} off the panel root", id.0));
        assert_eq!(
            ui.parent(row),
            Some(face),
            "{part:?}'s row is found through 0x1000059D whether or not anything holds a handle \
             to it"
        );
    }
}
