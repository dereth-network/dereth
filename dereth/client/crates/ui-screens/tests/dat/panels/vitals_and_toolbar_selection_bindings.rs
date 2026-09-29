//! The build binds VitalsPanel's six meters/labels and the toolbar's four selection children
//! through the catalogue and writes/drives them.
//! Fixture: shipped layouts, strings and keymaps loaded from the retail DATs.

use crate::common::layout::RegistrationOrder;

use dereth_primitives::ObjectId;
use dereth_ui::{ElemHandle, ElementId, Screen, UiSystem};
use dereth_ui_screens::bind::{attr, attr_float, Bound};
use dereth_ui_screens::panels::catalogue;
use dereth_ui_screens::screens::gameplay::{window, GamePlayScreen};
use dereth_ui_screens::view::{GameView, Vital};

// ---------------------------------------------------------------------------------------------
// The live tree
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

/// The text on one element, or `""` -- `options_client_captions.rs`'s.
fn text_of(ui: &mut UiSystem, h: ElemHandle) -> String {
    ui.text_element_mut(h)
        .map(|t| t.glyphs.inq_text(false))
        .unwrap_or_default()
}

fn window_root(ui: &UiSystem, s: &GamePlayScreen, w: ElementId) -> ElemHandle {
    let root = s.root().expect("the gameplay root");
    ui.get_child_recursive(root, w)
        .unwrap_or_else(|| panic!("{:#010X} is not in the shipped tree", w.0))
}

/// The one thing both groups' entries claim about **this** build: the handle the production
/// consumer reads by field name is the element the catalogue's id names, resolved from the
/// window's own root. A wrong id in the table, or a consumer that had quietly started resolving
/// the element some other way, breaks this.
fn assert_bound_is_the_catalogued_element(
    ui: &UiSystem,
    bound: &Bound,
    root: ElemHandle,
    class: &str,
    field: &str,
    id: u32,
) -> ElemHandle {
    let spec = catalogue::spec(class).unwrap_or_else(|| panic!("{class} is catalogued"));
    let row = spec
        .children
        .iter()
        .find(|c| c.field == field)
        .unwrap_or_else(|| panic!("{class}: no catalogue row named {field}"));
    assert_eq!(row.id.0, id, "{class}::{field}: the catalogue's id");
    let h = bound
        .get(field)
        .unwrap_or_else(|| panic!("{class}::{field} ({id:#010X}) did not bind"));
    assert_eq!(
        ui.node(h).expect("alive").desc.element_id,
        ElementId(id),
        "{class}::{field}: the bound handle is not {id:#010X}"
    );
    assert_eq!(
        ui.get_child_recursive(root, ElementId(id)),
        Some(h),
        "{class}::{field}: and it is the element the window's own subtree holds"
    );
    h
}

// ---------------------------------------------------------------------------------------------
// Group 1 — `VitalsPanel`'s six meters and labels
// ---------------------------------------------------------------------------------------------

/// `(id, catalogue field name)` for the six, meters first, then labels. The catalogue lists them
/// in numeric order instead, which is not a disagreement: `bind_children` has no ordering contract.
const VITALS: [(u32, &str); 6] = [
    (0x1000_00E6, "health_meter"),
    (0x1000_00EC, "stamina_meter"),
    (0x1000_00EE, "mana_meter"),
    (0x1000_00EB, "health_label"),
    (0x1000_00ED, "stamina_label"),
    (0x1000_00EF, "mana_label"),
];

#[derive(Debug, Default)]
struct Player {
    health: (u32, u32),
    stamina: (u32, u32),
    mana: (u32, u32),
}

const PLAYER: ObjectId = ObjectId(0x5000_0001);

impl GameView for Player {
    fn player(&self) -> Option<ObjectId> {
        Some(PLAYER)
    }
    fn vital(&self, _id: ObjectId, which: Vital) -> Option<(u32, u32)> {
        Some(match which {
            Vital::Health => self.health,
            Vital::Stamina => self.stamina,
            Vital::Mana => self.mana,
        })
    }
}

/// Behaviour: hud.vitals.both-vitals-windows-fill-their-three-bars-and-three-labels
/// **Group 1, this build's half — the station the `LAYOUT` entries cite.**
///
/// Both vitals windows bind all six through `catalogue::spec("VitalsPanel")` + `bind_children`, the
/// handles are the catalogued elements, and `update_vitals` writes `METER_LEVEL` and the label on
/// every one of the six. If the route were anything else — a literal somewhere else, a different
/// element, a consumer that had stopped reading these handles — one of those three fails.
#[test]
fn this_build_binds_the_six_vitals_children_through_the_catalogue_and_writes_them() {
    let (mut ui, mut s) = gameplay();
    let stacked_root = window_root(&ui, &s, window::STACKED_VITALS);
    let side_root = window_root(&ui, &s, window::SIDE_VITALS);

    for (root, bound, which) in [
        (stacked_root, s.stacked_vitals.clone(), "stacked"),
        (side_root, s.side_vitals.clone(), "side-by-side"),
    ] {
        assert!(
            bound.is_complete(),
            "{which} vitals: unresolved {:?}",
            bound.missing.iter().map(|m| m.field).collect::<Vec<_>>()
        );
        assert_eq!(bound.found.len(), 6, "{which} vitals: all six");
        for (id, field) in VITALS {
            assert_bound_is_the_catalogued_element(&ui, &bound, root, "VitalsPanel", field, id);
        }
    }
    // The six are the whole of the catalogue's spec, and nothing else.
    let spec = catalogue::spec("VitalsPanel").expect("VitalsPanel is catalogued");
    let mut have: Vec<u32> = spec.children.iter().map(|c| c.id.0).collect();
    have.sort_unstable();
    let mut want: Vec<u32> = VITALS.iter().map(|&(id, _)| id).collect();
    want.sort_unstable();
    assert_eq!(have, want, "the VITALS spec is exactly the six");

    // And the production writer reaches every one of them. `update_vitals` guards on the value
    // having changed, so the two rounds below are a change each.
    let view = Player {
        health: (30, 120),
        stamina: (75, 150),
        mana: (10, 40),
    };
    assert!(
        s.update_vitals(&mut ui, &view),
        "the first values are a change"
    );

    let levels = [
        ("health_meter", "health_label", 30.0_f32 / 120.0, "30/120"),
        ("stamina_meter", "stamina_label", 75.0 / 150.0, "75/150"),
        ("mana_meter", "mana_label", 10.0 / 40.0, "10/40"),
    ];
    for bound in [&s.stacked_vitals, &s.side_vitals] {
        for (meter, label, level, text) in levels {
            let m = bound.get(meter).expect("bound");
            assert!(
                (attr_float(&ui, m, attr::METER_LEVEL).expect("a level") - level).abs() < 1e-6,
                "{meter}: METER_LEVEL 0x69 is cur/max"
            );
            let l = bound.get(label).expect("bound");
            assert_eq!(text_of(&mut ui, l), text, "{label}: the cur/max string");
        }
    }
}

// ---------------------------------------------------------------------------------------------
// Group 2 — `Toolbar`'s four selection children
// ---------------------------------------------------------------------------------------------

/// The selected-object field, the element the other three are looked up **inside**.
const SEL_FIELD: u32 = 0x1000_019E;

#[derive(Debug, Default)]
struct Selected {
    selection: Option<ObjectId>,
    meters: (Option<f32>, Option<f32>),
}

const TARGET: ObjectId = ObjectId(0x6000_0001);

impl GameView for Selected {
    fn selection(&self) -> Option<ObjectId> {
        self.selection
    }
    fn name(&self, id: ObjectId) -> Option<&str> {
        (id == TARGET).then_some("Drudge Slinker")
    }
    fn selected_meters(&self) -> (Option<f32>, Option<f32>) {
        self.meters
    }
}

/// Behaviour: toolbar.selection.the-four-selection-children-are-bound-and-driven
/// **Group 2, this build's half — the station the `LAYOUT` entries cite.**
///
/// All four bind through `catalogue::spec("Toolbar")` + `bind_children` off
/// `window::TOOLBAR`, the handles are the catalogued elements, and the two production writers
/// reach them: `update_toolbar_selection` writes the selected-object name and resets
/// the field's state, and `update_selected_meters` shows and fills the two meters.
#[test]
fn this_build_binds_the_four_selection_children_through_the_catalogue_and_drives_them() {
    let (mut ui, mut s) = gameplay();
    let root = window_root(&ui, &s, window::TOOLBAR);
    let bound = s.toolbar_children.clone();
    assert!(
        bound.is_complete(),
        "toolbar: unresolved {:?}",
        bound.missing.iter().map(|m| m.field).collect::<Vec<_>>()
    );
    let four = [
        ("sel_object_field", SEL_FIELD),
        ("sel_object_name", 0x1000_019F_u32),
        ("sel_object_health_meter", 0x1000_01A1),
        ("sel_object_mana_meter", 0x1000_01A2),
    ];
    for (field, id) in four {
        assert_bound_is_the_catalogued_element(&ui, &bound, root, "Toolbar", field, id);
    }
    // Retail's two-step is visible in the shipped layout too: the other three really are inside
    // the field, so retail's lookup inside the field and this build's search
    // from the toolbar root land on the same elements.
    let field_h = bound.get("sel_object_field").expect("bound");
    for (name, _) in &four[1..] {
        let h = bound.get(name).expect("bound");
        assert!(
            ui.is_ancestor_of(field_h, h),
            "{name} is inside the selected-object field 0x1000019E"
        );
    }

    // The read-out half: the selection-changed handler's name write.
    let view = Selected {
        selection: Some(TARGET),
        meters: (None, None),
    };
    let out = s.update_toolbar_selection(&mut ui, &view);
    assert!(out.wrote, "a live selection is a read-out");
    let name = bound.get("sel_object_name").expect("bound");
    assert_eq!(
        text_of(&mut ui, name),
        "Drudge Slinker",
        "the selected-object name carries the selected object's name"
    );

    // The meters' only writers: the object-health and item-mana update notices.
    let with_meters = Selected {
        selection: Some(TARGET),
        meters: (Some(0.25), Some(0.75)),
    };
    assert_eq!(
        s.update_selected_meters(&mut ui, &with_meters),
        2,
        "both meters were written"
    );
    for (field, want) in [
        ("sel_object_health_meter", 0.25_f32),
        ("sel_object_mana_meter", 0.75),
    ] {
        let h = bound.get(field).expect("bound");
        assert!(
            ui.is_visible(h) || ui.node(h).expect("alive").region.flags.visible,
            "{field} shown"
        );
        assert!(
            (attr_float(&ui, h, attr::METER_LEVEL).expect("a level") - want).abs() < 1e-6,
            "{field}: METER_LEVEL 0x69"
        );
    }
}
