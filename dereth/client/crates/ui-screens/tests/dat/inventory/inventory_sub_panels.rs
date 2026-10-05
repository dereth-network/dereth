//! The two sub-panel handles are the sub-panel elements under the inventory page; the three lists
//! this build binds are those reached through them; burden pair and caption sit in their owning
//! sub-panel; this build neither shows, hides nor moves them.
//! Fixture: shipped layouts, strings and keymaps loaded from the retail DATs.

use crate::common::layout::RegistrationOrder;

use crate::common::*;
use dereth_ui::{ElemHandle, ElementId, Screen, UiSystem};
use dereth_ui_screens::panels::inventory::{
    InventoryPanels, BURDEN_METER, BURDEN_TEXT, CONTAINER_LIST, CONTENTS_TEXT, ITEM_LIST,
    PAPER_DOLL, TITLE_TEXT, TOP_CONTAINER,
};
use dereth_ui_screens::screens::gameplay::window::INVENTORY_PAGE;
use dereth_ui_screens::screens::gameplay::GamePlayScreen;
use dereth_ui_screens::view::EmptyGameView;

/// The backpack sub-panel, checked as element type `0x10000022` during initialization.
const BACKPACK_UI: ElementId = ElementId(0x1000_01CE);
/// The 3D-items sub-panel, checked as element type `0x10000021` during initialization.
const ITEMS_3D_UI: ElementId = ElementId(0x1000_01CF);
/// The three game element types post-init casts to, from `element_types::ty`.
const TY_PAPER_DOLL: u32 = 0x1000_0024;
const TY_BACKPACK: u32 = 0x1000_0022;
const TY_ITEMS_3D: u32 = 0x1000_0021;

// ---------------------------------------------------------------------------------------------
// Harness — `lamp_panels.rs`'s.
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

/// The panel bound exactly as `dereth_client_shell::hud::Hud::drive` binds it: off the inventory page.
fn bound() -> (UiSystem, GamePlayScreen, InventoryPanels) {
    let (mut ui, s) = gameplay();
    let page = find(&ui, &s, INVENTORY_PAGE);
    let mut p = InventoryPanels::default();
    p.post_init(&mut ui, page);
    (ui, s, p)
}

// ---------------------------------------------------------------------------------------------
// 1. The handles are the two sub-panels, and they are children of the page
// ---------------------------------------------------------------------------------------------

/// `0x100001CE` and `0x100001CF` are the Backpack and Items panels themselves —
/// the types post-init's two type-checked casts name — and both hang directly under the inventory
/// page, which is what makes this build's one-level-up search reach their contents.
#[test]
fn the_two_handles_are_the_two_sub_panel_elements_under_the_inventory_page() {
    let (ui, s) = gameplay();
    let page = find(&ui, &s, INVENTORY_PAGE);
    let doll = find(&ui, &s, PAPER_DOLL);
    let backpack = find(&ui, &s, BACKPACK_UI);
    let items3d = find(&ui, &s, ITEMS_3D_UI);

    assert_eq!(
        ty_of(&ui, doll),
        TY_PAPER_DOLL,
        "0x100001CD is the paper-doll panel (cast to 0x10000024)"
    );
    assert_eq!(
        ty_of(&ui, backpack),
        TY_BACKPACK,
        "0x100001CE is the backpack panel (cast to 0x10000022)"
    );
    assert_eq!(
        ty_of(&ui, items3d),
        TY_ITEMS_3D,
        "0x100001CF is the item-grid panel (cast to 0x10000021)"
    );

    for (h, id) in [
        (doll, PAPER_DOLL),
        (backpack, BACKPACK_UI),
        (items3d, ITEMS_3D_UI),
    ] {
        assert_eq!(
            ui.parent(h),
            Some(page),
            "{:#010X} is a direct child of the inventory page 0x1000018B",
            id.0
        );
    }
    // The title text is the fourth binding and is a peer of the three, not inside any of them --
    // which is why `post_init` finds it from the page too.
    let title = find(&ui, &s, TITLE_TEXT);
    assert_eq!(
        ui.parent(title),
        Some(page),
        "the title text 0x100001D3 is the page's own caption"
    );
}

// ---------------------------------------------------------------------------------------------
// 2. The route lands in the same place
// ---------------------------------------------------------------------------------------------

/// Behaviour: inventory.panel.the-backpack-and-item-grid-sections-hold-the-three-lists-and-are-never-moved
/// **The assertion that goes stale loudly.** Retail reaches three item-list widgets through the two
/// handles — the top container and the container list inside the Backpack panel, and the item
/// list inside the Items panel. This build reaches them from the page. Both arrive at the same
/// elements.
#[test]
fn the_three_lists_this_build_binds_are_the_three_retail_reaches_through_the_handles() {
    let (ui, s, p) = bound();
    let backpack = find(&ui, &s, BACKPACK_UI);
    let items3d = find(&ui, &s, ITEMS_3D_UI);

    // The retail route, hop by hop: a direct child lookup on the sub-panel, the way each
    // sub-panel's own post-init does it off itself.
    let retail_top = ui
        .get_child(backpack, TOP_CONTAINER)
        .expect("top container under 0x100001CE");
    let retail_containers = ui
        .get_child(backpack, CONTAINER_LIST)
        .expect("container list under 0x100001CE");
    let retail_items = ui
        .get_child(items3d, ITEM_LIST)
        .expect("item list under 0x100001CF");

    // This build's route: `get_child_recursive` from the page, one level above both.
    let ours_top = p
        .top_container
        .as_ref()
        .expect("top container bound")
        .handle;
    let ours_containers = p
        .container_list
        .as_ref()
        .expect("container list bound")
        .handle;
    let ours_items = p.item_list.as_ref().expect("item list bound").handle;

    assert_eq!(ours_top, retail_top, "top container 0x100001C9");
    assert_eq!(
        ours_containers, retail_containers,
        "container list 0x100001CA"
    );
    assert_eq!(ours_items, retail_items, "item list 0x100001C6");

    // Stated the other way as well, so a future `post_init` that found a same-id list in some other
    // subtree fails here and not only on the equality above.
    assert!(
        ui.is_ancestor_of(backpack, ours_top),
        "the top container is inside the backpack panel"
    );
    assert!(
        ui.is_ancestor_of(backpack, ours_containers),
        "so is the container list"
    );
    assert!(
        ui.is_ancestor_of(items3d, ours_items),
        "and the item list is inside the item-grid panel"
    );
    assert!(
        !ui.is_ancestor_of(items3d, ours_top),
        "and the two subtrees are not confused"
    );
}

/// The rest of what the two sub-panels own, on the same test: the burden pair belongs to the
/// backpack (burden text and meter) and the contents caption to the 3D
/// items panel. `post_init` writes both, and both are inside the
/// sub-panel whose class declares them.
#[test]
fn the_burden_pair_and_the_contents_caption_sit_in_the_sub_panel_that_owns_them() {
    let (ui, s, p) = bound();
    let backpack = find(&ui, &s, BACKPACK_UI);
    let items3d = find(&ui, &s, ITEMS_3D_UI);

    for (h, what) in [
        (p.burden_text.expect("burden text bound"), "burden text"),
        (p.burden_meter.expect("burden meter bound"), "burden meter"),
    ] {
        assert!(
            ui.is_ancestor_of(backpack, h),
            "{what} is inside the backpack panel 0x100001CE"
        );
    }
    assert_eq!(
        ui.get_child_recursive(items3d, CONTENTS_TEXT),
        ui.get_child_recursive(find(&ui, &s, INVENTORY_PAGE), CONTENTS_TEXT),
        "the contents text 0x100001C5 is inside the item-grid panel, and the page search finds that one"
    );
    // The two ids `post_init` is asked for by name, so the test names them too and a rename here
    // cannot silently stop covering them.
    assert_eq!(BURDEN_TEXT, ElementId(0x1000_01D8));
    assert_eq!(BURDEN_METER, ElementId(0x1000_01D9));
}

/// Retail never shows, hides, moves or restates either sub-panel — every use of the two fields in
/// the image is a dereference to a list inside them. This build must not invent one: both stay
/// exactly as the layout authored them across `post_init` and a frame of `update`.
#[test]
fn neither_sub_panel_is_shown_hidden_or_moved_by_this_build() {
    let (ui, s) = gameplay();
    let backpack = find(&ui, &s, BACKPACK_UI);
    let items3d = find(&ui, &s, ITEMS_3D_UI);
    let authored = |ui: &UiSystem, h: ElemHandle| {
        let n = ui.node(h).expect("alive");
        (n.region.flags.visible, n.region.box_, n.state.0)
    };
    let before = (authored(&ui, backpack), authored(&ui, items3d));
    drop(ui);

    let (mut ui, s, mut p) = bound();
    let backpack = find(&ui, &s, BACKPACK_UI);
    let items3d = find(&ui, &s, ITEMS_3D_UI);
    let after_post_init = (authored(&ui, backpack), authored(&ui, items3d));
    assert_eq!(
        before, after_post_init,
        "post_init leaves both sub-panel elements alone"
    );

    let view = EmptyGameView;
    p.update(&mut ui, &view);
    let after_update = (authored(&ui, backpack), authored(&ui, items3d));
    assert_eq!(before, after_update, "and so does a frame of update");
}
