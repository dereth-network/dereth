//! UI fixtures and scenarios for states.

use super::*;
// ---------------------------------------------------------------------------------------------
// ui.states.*
//
// The element below is authored here on purpose: **no shipped element lines the three facts up at
// once** -- base artwork that ends by asking for a state, alternate states declared with nothing
// to draw, and a tick box. Each of the three is a shape the shipped layouts do
// use separately, so a scenario that waited for one that had all three would never run.
//
// It is a `dat`-tier scenario because it stands beside its sibling above; it opens no data file of
// its own, and the layout it drives is the twelve lines below.
// ---------------------------------------------------------------------------------------------

/// The authored tick box, and the attributes the widget reads.
const TOGGLE: dereth_ui::ElementId = dereth_ui::ElementId(0x1000_0900);
/// The button's "toggled" and "toggle button" attributes.
const ATTR_TOGGLED: u32 = 0x0E;
const ATTR_TOGGLE_BUTTON: u32 = 0x0B;

/// No data file at all: every element here is built from a description written above.
#[derive(Debug)]
struct NoAssets;

impl dereth_primitives::AssetSource for NoAssets {
    fn read(
        &self,
        id: dereth_primitives::DataId,
    ) -> Result<Vec<u8>, dereth_primitives::AssetError> {
        Err(dereth_primitives::AssetError::NotFound(id))
    }
    fn exists(&self, _: dereth_primitives::DataId) -> bool {
        false
    }
    fn iter_type(
        &self,
        _: dereth_primitives::DataType,
    ) -> Box<dyn Iterator<Item = dereth_primitives::DataId> + '_> {
        Box::new(std::iter::empty())
    }
}

/// One tick box carrying artwork of its own that ends by asking for a state, and declaring the
/// ticked state with nothing to draw.
fn a_toggle_whose_artwork_asks_for_a_state() -> dereth_ui::UiSystem {
    use dereth_ui::desc::{
        incorporation, ElementDesc, LayoutDesc, MediaDesc, MediaFields, StateDesc,
    };
    use dereth_ui::{PropertyValue, StateId, UiSystem};

    let asks_for_state_one = MediaDesc {
        media_type: 10,
        type_echo_ok: true,
        fields: MediaFields::State {
            state_id: 1,
            probability: 1.0,
        },
    };
    let mut base = StateDesc {
        incorporation: incorporation::LEGACY_ALL_GEOMETRY,
        x: 0,
        y: 0,
        width: 40,
        height: 20,
        media: vec![asks_for_state_one],
        ..StateDesc::default()
    };
    base.properties
        .set(ATTR_TOGGLE_BUTTON, PropertyValue::Bool(true));
    base.properties
        .set(ATTR_TOGGLED, PropertyValue::Bool(false));
    let declared = |id: u32| {
        (
            StateId(id),
            StateDesc {
                state_id: StateId(id),
                media: Vec::new(),
                ..StateDesc::default()
            },
        )
    };
    let desc = ElementDesc {
        base,
        element_id: TOGGLE,
        ty: dereth_ui::factory::ty::BUTTON,
        default_state: StateId(1),
        states: [declared(1), declared(3), declared(6)]
            .into_iter()
            .collect(),
        ..ElementDesc::default()
    };
    let layout = LayoutDesc {
        did: dereth_primitives::DataId(0x2100_0900),
        display_width: 800,
        display_height: 600,
        elements: std::iter::once((TOGGLE, desc)).collect(),
    };
    let mut ui = UiSystem::new((800, 600));
    let d = layout
        .access_element(TOGGLE)
        .cloned()
        .expect("the one element");
    let h = ui
        .create_element_recursive_from_full_desc(&NoAssets, &layout, &d)
        .expect("nothing is inherited")
        .expect("the button registers");
    let root = ui.root();
    ui.set_parent(h, Some(root));
    ui.initialize_tree(h);
    ui
}

fn the_toggle(ui: &dereth_ui::UiSystem) -> dereth_ui::ElemHandle {
    ui.get_child(ui.root(), TOGGLE)
        .expect("the button is under the root")
}

fn is_ticked(ui: &dereth_ui::UiSystem, h: dereth_ui::ElemHandle) -> Option<bool> {
    ui.node(h)
        .and_then(|n| n.merged_properties().get_bool(ATTR_TOGGLED))
}

/// A tick box put into the state that means ticked is not unticked by its own artwork.
pub(super) fn a_toggle_is_not_unticked_by_its_own_state() {
    use dereth_ui::StateId;

    let mut ui = a_toggle_whose_artwork_asks_for_a_state();
    let h = the_toggle(&ui);
    let starts_unticked = is_ticked(&ui, h) == Some(false);

    // The state a tick box is put into when it is ticked, which is what letting go of the button
    // over it reaches.
    ui.set_state(h, StateId(6));
    let survived = is_ticked(&ui, h) == Some(true);

    let mut c = HeadlessClient::model();
    c.assert_behaviour(
        "ui.states.a-toggle-is-not-unticked-by-its-own-change-of-state",
        move |_| starts_unticked && survived,
    );
}

/// A state declared with nothing to draw runs nothing; an undeclared one runs the element's own.
pub(super) fn a_state_with_nothing_to_draw_runs_nothing() {
    use dereth_ui::StateId;

    let mut ui = a_toggle_whose_artwork_asks_for_a_state();
    let h = the_toggle(&ui);

    // A state the layout declares and gives nothing to draw: nothing runs.
    ui.media_effects.clear();
    ui.set_state(h, StateId(3));
    let nothing_ran = ui.media_effects.is_empty();

    // A state the layout does not declare: the element's own artwork is what runs, and it lands --
    // the artwork asks for a state and the element goes there, which is the whole reason the
    // looser rule was dangerous.
    ui.media_effects.clear();
    ui.set_state(h, StateId(0x1234));
    let base_ran = ui.media_effects.iter().any(
        |(_, e)| matches!(e, dereth_ui::media::MediaEffect::SetState { id } if *id == StateId(1)),
    );
    let landed = ui.node(h).map(|n| n.state) == Some(StateId(1));

    let mut c = HeadlessClient::model();
    c.assert_behaviour(
        "ui.states.a-state-declared-with-no-media-runs-nothing-and-an-undeclared-one-runs-the-base",
        move |_| nothing_ran && base_ran && landed,
    );
}
