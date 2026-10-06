//! A panel made taller than its default keeps its lists laid out: the list stretches with the
//! panel and every row stays where the list put it, in order, one under the next. The Character
//! Options list (headings, separators and toggles from three templates) is the case, moved and
//! resized as a saved screen layout or a drag of the panel's edge does.
//! Fixture: shipped layouts, strings and keymaps loaded from the retail DATs.

use crate::common::layout::RegistrationOrder;

use dereth_ui::framework::Screen;
use dereth_ui::{ElemHandle, ElementId, UiSystem};
use dereth_ui_screens::screens::gameplay::{window, GamePlayScreen};

/// The Character Options page's option box, by literal id.
const OPTION_BOX: ElementId = ElementId(0x1000_01FA);

/// Each row and its box inside the list, top to bottom.
fn rows(ui: &UiSystem, list: ElemHandle) -> Vec<(ElemHandle, i32, i32, i32, i32)> {
    let mut rows: Vec<_> = ui
        .children(list)
        .into_iter()
        .filter_map(|c| {
            let b = ui.node(c)?.region.box_;
            Some((c, b.x0, b.y0, b.width(), b.height()))
        })
        .collect();
    rows.sort_by_key(|r| r.2);
    rows
}

/// Behaviour: panels.layout.a-taller-panel-keeps-its-list-rows-in-place
#[test]
fn a_taller_panel_keeps_every_list_row_where_the_list_put_it() {
    let (mut ui, _flow, _store) =
        crate::common::layout::load((1024, 1280), RegistrationOrder::BeforeResolver);
    let mut s = GamePlayScreen::default();
    s.create(&mut dereth_ui::framework::ScreenCx::new(&mut ui))
        .expect("the gameplay screen builds from the shipped layout");
    let root = s.root().expect("the gameplay root");
    let pans = ui
        .get_child_recursive(root, window::PANEL_STACK)
        .expect("the panel stack");
    let page = s.character_options.page.expect("the page was bound");
    let list = ui
        .get_child_recursive(page, OPTION_BOX)
        .expect("the option box");

    let before = rows(&ui, list);
    assert!(before.len() > 60, "the rows were built: {}", before.len());
    // Stacked: each row starts where the one above it ends.
    for pair in before.windows(2) {
        assert_eq!(pair[1].2, pair[0].2 + pair[0].4, "{pair:?}");
    }
    let list_before = ui.node(list).unwrap().region.box_;

    // The panel 300 pixels taller, its top edge raised: a move, then a resize.
    let b = ui.node(pans).unwrap().region.box_;
    ui.move_to(pans, b.x0, b.y0 - 300);
    ui.resize_to(pans, b.width(), b.height() + 300);

    let list_after = ui.node(list).unwrap().region.box_;
    assert_eq!(
        list_after.height(),
        list_before.height() + 300,
        "the list stretches with the panel"
    );
    assert_eq!(
        rows(&ui, list),
        before,
        "every row keeps its place in the list"
    );
}
