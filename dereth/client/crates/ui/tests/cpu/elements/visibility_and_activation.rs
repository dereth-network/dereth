//! Showing and hiding a root window and what it does to activation. A show raises the
//! visibility-changed broadcast only on an effective edge (an ancestor that hides the window keeps
//! it hidden); a window that activates on show takes activation from the current one, in the order
//! visibility, old deactivation, new activation; hiding the active window unregisters it and hands
//! activation to the last visible registered root, skipping any under a hidden parent. A window
//! under a hidden ancestor, a detached one and a hidden one cannot activate.
//!
//! Fixture: hollow root windows under the manager root, built in the test; no dats, no layout.
use dereth_ui::{ElemHandle, UiSystem};

fn events(ui: &mut UiSystem) -> Vec<(ElemHandle, u32)> {
    ui.drain_outbox()
        .into_iter()
        .filter_map(|d| match d {
            dereth_ui::Delivery::Element { msg, .. } if matches!(msg.id.0, 0x18 | 0x29 | 0x2A) => {
                Some((msg.source, msg.id.0))
            }
            _ => None,
        })
        .collect()
}

fn window(ui: &mut UiSystem, parent: Option<ElemHandle>, on_show: bool) -> ElemHandle {
    let h = ui.create_hollow(parent);
    ui.node_mut(h).unwrap().flags.set_is_root_element(true);
    ui.set_attribute_bool(h, 0x34, on_show);
    h
}

/// Behaviour: ui.visibility.hiding-a-window-moves-activation-to-the-next-visible-one
#[test]
fn show_flag_raises_only_on_effective_edge_and_hide_falls_back_after_unregister() {
    let mut ui = UiSystem::new((800, 600));
    let root = ui.root();
    let first = window(&mut ui, Some(root), false);
    let popup = window(&mut ui, Some(root), true);
    let ordinary = window(&mut ui, Some(root), false);
    ui.set_visible(popup, false);
    ui.set_visible(ordinary, false);
    assert!(ui.activate(first));
    ui.set_visible(ordinary, true);
    assert_eq!(
        ui.active_element(),
        Some(first),
        "ordinary show never activates"
    );
    for h in [first, popup, ordinary] {
        ui.register_for_element_messages(h, dereth_ui::ListenerId::External(0xAF16));
    }
    let _ = ui.drain_outbox();
    ui.set_visible(popup, true);
    assert_eq!(
        events(&mut ui),
        vec![(popup, 0x18), (first, 0x2A), (popup, 0x29)],
        "visibility broadcast, old deactivation, new activation in source order"
    );
    assert_eq!(ui.active_element(), Some(popup));
    assert_eq!(ui.children(root).last(), Some(&popup));
    assert!(ui.activatable_elements().contains(&popup));
    assert!(ui.activate(ordinary));
    let _ = ui.drain_outbox();
    ui.set_visible(popup, true);
    assert!(
        events(&mut ui).is_empty(),
        "equal show does not broadcast or reactivate"
    );
    assert_eq!(
        ui.active_element(),
        Some(ordinary),
        "an equal show does not reactivate"
    );
    assert_eq!(ui.children(root).last(), Some(&ordinary));
    ui.set_visible(ordinary, false);
    assert_eq!(
        events(&mut ui),
        vec![(ordinary, 0x18), (ordinary, 0x2A), (popup, 0x29)],
        "hide/deactivate precede the fallback activation"
    );
    assert_eq!(
        ui.active_element(),
        Some(popup),
        "last visible registered root wins"
    );
    assert!(
        !ui.activatable_elements().contains(&ordinary),
        "unregister precedes fallback"
    );
    ui.set_visible(popup, false);
    assert_eq!(ui.active_element(), Some(first));
    assert!(!ui.activatable_elements().contains(&popup));
    ui.set_visible(popup, false);
    assert_eq!(
        ui.active_element(),
        Some(first),
        "equal hide does not disturb focus owner"
    );
}

#[test]
fn hidden_ancestor_detached_and_hidden_self_cannot_activate() {
    let mut ui = UiSystem::new((800, 600));
    let root = ui.root();
    let first = window(&mut ui, Some(root), false);
    let parent = window(&mut ui, Some(root), false);
    let child = window(&mut ui, Some(parent), true);
    let detached = window(&mut ui, None, true);
    assert!(ui.activate(first));
    ui.set_visible(parent, false);
    ui.set_visible(child, false);
    ui.set_visible(child, true);
    assert_eq!(
        ui.active_element(),
        Some(first),
        "own flag changed but effective visibility did not"
    );
    assert!(!ui.activate(child));
    ui.set_visible(detached, false);
    ui.set_visible(detached, true);
    assert!(
        !ui.activate(detached),
        "visible parent chain must end at manager root"
    );
    ui.set_visible(child, false);
    ui.set_visible(parent, true);
    assert!(!ui.activate(child));
    ui.set_visible(child, true);
    assert_eq!(
        ui.active_element(),
        Some(child),
        "real effective edge does activate"
    );
    ui.set_visible(root, false);
    assert!(!ui.activate(first), "manager root visibility is included");
}

/// Behaviour: ui.visibility.hiding-a-window-moves-activation-to-the-next-visible-one
#[test]
fn fallback_skips_registered_window_under_hidden_parent() {
    let mut ui = UiSystem::new((800, 600));
    let root = ui.root();
    let first = window(&mut ui, Some(root), false);
    let parent = window(&mut ui, Some(root), false);
    let child = window(&mut ui, Some(parent), false);
    let popup = window(&mut ui, Some(root), true);
    assert!(ui.activate(first));
    assert!(ui.activate(child));
    ui.set_visible(popup, false);
    ui.set_visible(popup, true);
    ui.set_visible(parent, false);
    ui.set_visible(popup, false);
    assert_eq!(
        ui.active_element(),
        Some(first),
        "the fallback reads effective visibility, not the own flag"
    );
}
