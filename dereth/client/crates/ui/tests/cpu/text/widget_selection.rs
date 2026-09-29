//! A selection is drawn by every element that derives from the text element, not only by the text
//! element itself: `Button`, `Menu` and `Scrollbar` derive from it, so the text draw's selection
//! arm belongs to them too.
//!
//! No shipped screen puts a live selection on a button, so the shipped-data tests cannot see this;
//! here the case is constructed, one element of each type in a container from a `LayoutDesc`
//! written in the test, so removing a widget's selection delegation turns the test red. No dats.

use crate::common::NoAssets;
use dereth_primitives::DataId;
use dereth_ui::desc::{incorporation, ElementDesc, LayoutDesc, StateDesc};
use dereth_ui::factory::ty;
use dereth_ui::{ElementId, ElementType, UiSystem};

fn desc(id: u32, ty: ElementType, w: i32, h: i32) -> ElementDesc {
    ElementDesc {
        base: StateDesc {
            incorporation: incorporation::LEGACY_ALL_GEOMETRY,
            x: 0,
            y: 0,
            width: w,
            height: h,
            ..StateDesc::default()
        },
        element_id: ElementId(id),
        ty,
        ..ElementDesc::default()
    }
}

const CONTAINER: u32 = 0x1000_0010;

/// Build a container holding one element of each type under test, each selectable.
fn tree(ui: &mut UiSystem, kinds: &[(u32, ElementType)]) -> Vec<dereth_ui::ElemHandle> {
    let mut container = desc(CONTAINER, ty::FIELD, 400, 200);
    for (id, kind) in kinds {
        container
            .children
            .insert(ElementId(*id), desc(*id, *kind, 200, 40));
    }
    let l = LayoutDesc {
        did: DataId(0x2100_0001),
        display_width: 800,
        display_height: 600,
        elements: std::iter::once((ElementId(CONTAINER), container)).collect(),
    };
    let d = l
        .access_element(ElementId(CONTAINER))
        .cloned()
        .expect("root");
    let c = ui
        .create_element_recursive_from_full_desc(&NoAssets, &l, &d)
        .expect("no inheritance")
        .expect("registered");
    let root = ui.root();
    ui.set_parent(c, Some(root));
    kinds
        .iter()
        .map(|(id, _)| {
            ui.get_child(c, ElementId(*id))
                .expect("the child was built")
        })
        .collect()
}

/// Every invert rectangle the whole tree emits, by element.
fn inverts(ui: &mut UiSystem, h: dereth_ui::ElemHandle) -> usize {
    let mut back = dereth_ui::RecordingDrawBackend::default();
    ui.draw(&mut back);
    back.calls
        .iter()
        .filter(|c| c.who == h)
        .map(|c| c.invert.len())
        .sum()
}

/// Behaviour: ui.text.a-button-menu-or-scrollbar-draws-its-own-selection
///
/// **A `TextElement` and the three classes that derive from it all draw their selection.**
///
/// Asserted per type with a **calibration in both directions**: before `SelectAll` each element
/// draws none, after it each draws one rectangle per glyph. A delegation that had been forgotten
/// reads as `0` in the second column while the plain text element reads `4`.
#[test]
fn every_class_that_derives_from_uielement_text_draws_its_selection() {
    let mut ui = UiSystem::new((800, 600));
    dereth_ui::factory::register_engine_classes(&mut ui);
    let kinds: &[(u32, ElementType, &str)] = &[
        (0x1000_0021, ty::TEXT, "TextElement"),
        (0x1000_0022, ty::BUTTON, "Button"),
        (0x1000_0023, ty::MENU, "Menu"),
        (0x1000_0024, ty::SCROLLBAR, "Scrollbar"),
    ];
    let handles = tree(
        &mut ui,
        &kinds.iter().map(|(a, b, _)| (*a, *b)).collect::<Vec<_>>(),
    );

    for (h, (_, _, name)) in handles.iter().zip(kinds) {
        assert_eq!(
            inverts(&mut ui, *h),
            0,
            "{name}: nothing selected, nothing inverted"
        );
    }
    for (h, (_, _, name)) in handles.iter().zip(kinds) {
        // `0x27` — select-all's whole body is inside the selectable gate, and it is
        // set through the element's own attribute path so the widgets forward it as the client
        // does rather than having the bit poked into them.
        ui.set_attribute_bool(*h, dereth_ui::props::attr::TEXT_SELECTABLE, true);
        let t = ui
            .text_element_mut(*h)
            .unwrap_or_else(|| panic!("{name} has a text half"));
        t.set_text("abcd");
        t.select_all();
        assert_eq!(t.get_selection(), Some((0, 4)), "{name}: `SelectAll` took");
    }
    for (h, (_, _, name)) in handles.iter().zip(kinds) {
        assert_eq!(
            inverts(&mut ui, *h),
            4,
            "{name}: one colour-inverting rectangle per selected glyph"
        );
    }
}
