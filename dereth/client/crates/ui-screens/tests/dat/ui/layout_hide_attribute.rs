//! From the shipped layouts alone (no post-init): master property names the attribute hide; toolbar
//! shows only the peace dove; gameplay root shows exactly six windows; intro states show their own
//! media; item-slot decorations hidden but three; inventory drag overlay starts down.
//! Fixture: shipped layouts, strings and keymaps loaded from the retail DATs.

use crate::common::layout::RegistrationOrder;
use std::collections::BTreeMap;

use dereth_primitives::{AssetSource, DataId};
use dereth_ui::framework::LayoutEnum;
use dereth_ui::{ElemHandle, ElementId, StateId, UiSystem};

use dereth_ui_screens::env::create_and_add_root_element;

fn env() -> Option<UiSystem> {
    let (ui, _flow, _store) =
        crate::common::layout::load((800, 600), RegistrationOrder::BeforeResolver);
    Some(ui)
}

fn visible(ui: &UiSystem, root: ElemHandle, id: u32) -> Option<bool> {
    let h = ui.get_child_recursive(root, ElementId(id))?;
    Some(ui.node(h)?.region.flags.visible)
}

/// The shipped property table names the hide attribute at `0x3B`.
///
/// Read straight out of `client_portal.dat` object `0x39000001`: the row's `name` field is an
/// index into the same object's enum-name table. This is the only one of the four facts that does
/// not depend on any of this crate's code, and it is the reason the constant in `dereth_ui` is
/// called [`dereth_ui::props::attr::HIDE`] rather than `VISIBLE`.
#[test]
fn the_master_property_names_the_attribute_hide() {
    let dir = dereth_dat::testing::dat_dir();
    let store = dereth_dat::RetailDatStore::open_dir(&dir).unwrap_or_else(|e| {
        panic!(
            "the retail dats are this test's oracle and they are not under {} ({e}) -- \
             set DERETH_TEST_DAT_DIR",
            dir.display()
        )
    });
    let master_id = DataId(0x3900_0001);
    let bytes = store
        .read(master_id)
        .expect("the shipped MasterProperty table 0x39000001");
    let master =
        <dereth_assets::MasterProperty as dereth_assets::Decode>::decode_payload(master_id, &bytes)
            .unwrap();
    let names: BTreeMap<u32, &str> = master
        .enum_names
        .iter()
        .map(|(k, v)| (*k, v.as_str()))
        .collect();
    let (_, row) = master
        .properties
        .iter()
        .find(|(id, _)| *id == dereth_ui::props::attr::HIDE)
        .expect("MasterProperty has a row for 0x3B");
    assert_eq!(names.get(&row.name).copied(), Some("UICore_Element_hide"));
    assert_eq!(row.group, 8, "property group 8 is the UI group");
    assert_eq!(
        row.property_type,
        dereth_assets::ui::BasePropertyType::Bool as u32,
        "a Bool, so `true` is a hide and not a level"
    );
}

/// Behaviour: ui.layout.the-hide-attribute-hides-an-element-at-build
/// The toolbar comes up showing only the peace mode dove.
#[test]
fn the_toolbar_comes_up_showing_only_the_peace_mode_dove() {
    let mut ui = env().expect("the test environment: retail dats and a WARP device");
    // The toolbar is a child of the gameplay root, not a root of its own.
    let root =
        create_and_add_root_element(&mut ui, LayoutEnum(0x1000_0006), ElementId(0x1000_0495))
            .expect("classic_gameplay root");

    let mut shown = Vec::new();
    for (mode, e) in dereth_ui_screens::toolbar::combat_mode::BUTTONS {
        let v = visible(&ui, root, e.0).unwrap_or_else(|| panic!("{:#010X} is in the tree", e.0));
        if v {
            shown.push((mode, e.0));
        }
    }
    assert_eq!(
        shown,
        vec![(
            dereth_ui_screens::toolbar::combat_mode::NONCOMBAT,
            0x1000_0192
        )],
        "exactly the peace-mode icon, and it is the first of the four"
    );
}

/// The gameplay root comes up showing exactly the six windows retail shows.
#[test]
fn the_gameplay_root_comes_up_showing_exactly_the_six_windows_retail_shows() {
    use dereth_ui_screens::screens::gameplay::window;

    let mut ui = env().expect("the test environment: retail dats and a WARP device");
    let root =
        create_and_add_root_element(&mut ui, LayoutEnum(0x1000_0006), ElementId(0x1000_0495))
            .expect("classic_gameplay root");

    /// `<INDI>`, the indicator strip — a child of the gameplay root that `window` does not name.
    const INDICATORS: u32 = 0x1000_0611;

    let up: Vec<u32> = ui
        .children(root)
        .into_iter()
        .filter(|h| ui.node(*h).is_some_and(|n| n.region.flags.visible))
        .filter_map(|h| ui.node(h).map(|n| n.element_id().0))
        .collect();
    let mut up_sorted = up.clone();
    up_sorted.sort_unstable();

    let mut want = [
        window::SMART_BOX.0, // the 3D viewport
        INDICATORS,          // the indicator strip, top left
        window::STACKED_VITALS.0,
        window::RADAR.0,
        window::MAIN_CHAT.0,
        window::TOOLBAR.0,
        window::PANEL_STACK.0,
        window::SIDE_VITALS.0,
    ];
    want.sort_unstable();
    assert_eq!(
        up_sorted
            .iter()
            .map(|v| format!("{v:#010X}"))
            .collect::<Vec<_>>(),
        want.iter()
            .map(|v| format!("{v:#010X}"))
            .collect::<Vec<_>>(),
        "the layout alone, read as `hide`, brings up retail's six windows plus <PANS> and <SVIT>"
    );
    assert_eq!(
        ui.children(root).len(),
        18,
        "and eighteen children in total"
    );

    assert_eq!(
        visible(&ui, root, window::KEYBOARD.0),
        Some(false),
        "classic_keyboard"
    );
    assert_eq!(
        visible(&ui, root, window::ADMIN.0),
        Some(false),
        "classic_admin"
    );
    // The four floaty chat windows, which retail does not draw.
    for fch in [0x1000_0505u32, 0x1000_050E, 0x1000_050F, 0x1000_0510] {
        assert_eq!(visible(&ui, root, fch), Some(false), "{fch:#010X}");
    }
}

/// Each intro state shows its own media child and hides the other.
#[test]
fn each_intro_state_shows_its_own_media_child_and_hides_the_other() {
    let mut ui = env().expect("the test environment: retail dats and a WARP device");
    let root =
        create_and_add_root_element(&mut ui, LayoutEnum(0x1000_0002), ElementId(0x1000_0419))
            .expect("classic_intro root");

    const MOVIE: u32 = 0x1000_0444;
    const SPLASH: u32 = 0x1000_0434;

    assert_eq!(
        visible(&ui, root, MOVIE),
        Some(false),
        "no frame yet: the movie field is down"
    );
    assert_eq!(
        visible(&ui, root, SPLASH),
        Some(false),
        "and so is the splash field"
    );

    // The intro screen's four states, in the order root field `0x10000047` gives them.
    // 0x1000003E is the movie's; 0x10000038/39/3A are the splash field's.
    for (state, movie, splash) in [
        (0x1000_003Eu32, true, false),
        (0x1000_0038, false, true),
        (0x1000_0039, false, true),
        (0x1000_003A, false, true),
    ] {
        ui.set_state(root, StateId(state));
        assert_eq!(
            visible(&ui, root, MOVIE),
            Some(movie),
            "movie in state {state:#010X}"
        );
        assert_eq!(
            visible(&ui, root, SPLASH),
            Some(splash),
            "splash in state {state:#010X}"
        );
    }
}

/// The item slots decorations are hidden by the layout and only three are not.
#[test]
fn the_item_slots_decorations_are_hidden_by_the_layout_and_only_three_are_not() {
    let mut ui = env().expect("the test environment: retail dats and a WARP device");
    // `0x1000033A` is the default `ItemSlot` root (`UI_ItemList_ItemSlotID`'s default).
    let root =
        create_and_add_root_element(&mut ui, LayoutEnum(0x1000_0038), ElementId(0x1000_033A))
            .expect("ItemSlot root");

    // Ten cooldown wedges plus five named decorations.
    let mut hidden_by_layout: Vec<u32> = (0x1000_054Fu32..=0x1000_0558).collect();
    hidden_by_layout.extend([
        0x1000_0349, // the ghosted icon
        0x1000_04F5, // the quantity icon
        0x1000_034A, // the shortcut-numeral icon
        0x1000_0437, // the sell-state icon
        0x1000_0438, // the trade-state icon
    ]);
    assert_eq!(hidden_by_layout.len(), 15);
    for id in hidden_by_layout {
        assert_eq!(
            visible(&ui, root, id),
            Some(false),
            "{id:#010X} is hidden by the layout"
        );
    }
    for id in [
        0x1000_0347u32, // capacity bar   -> update_capacity_display, i.e. ItemSlot::rest
        0x1000_0348,    // structure bar  -> update_structure_display, i.e. ItemSlot::rest
        0x1000_0342,    // selection ring -> the item slot's set-state empty arm
    ] {
        assert_eq!(
            visible(&ui, root, id),
            Some(true),
            "{id:#010X} carries no 0x3B, so only code can take it down"
        );
    }
}

/// The inventory button's drag overlay (`0x1000046C`), whose whole job is to appear
/// *while a drag is over the inventory button*, inherits `0x3B = true` from `0x1000045E` and so
/// starts down. Its base declares `0x3B = false` in three states — the drag-over states — which is
/// the same "hidden except when I have something to say" shape as the intro's.
#[test]
fn the_inventory_button_drag_overlay_starts_down() {
    let mut ui = env().expect("the test environment: retail dats and a WARP device");
    let root =
        create_and_add_root_element(&mut ui, LayoutEnum(0x1000_0006), ElementId(0x1000_0495))
            .expect("classic_gameplay root");
    assert_eq!(
        visible(
            &ui,
            root,
            dereth_ui_screens::toolbar::INVENTORY_DRAG_OVERLAY.0
        ),
        Some(false)
    );
}
