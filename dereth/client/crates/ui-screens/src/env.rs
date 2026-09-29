//! The asset source and the layout-enum resolver a screen constructs from.
//!
//! [`Env`] lives in `dereth_ui::env`, beside the `UiSystem` that holds it: a screen, a panel
//! or an item slot reads it with `ui.env()` / `ui.require_env()` off the `UiSystem` it was
//! handed. Every `dereth_ui_screens::env::...` path resolves through the glob below.

use std::rc::Rc;

use dereth_primitives::AssetSource;
use dereth_ui::framework::LayoutEnumResolver;
use dereth_ui::UiSystem;

pub use dereth_ui::env::*;

/// Give `ui` the environment built from `assets` and `resolver`, and read once what this crate
/// memoises from it (the shipped number grouping, [`crate::panels::numfmt::prime`]).
///
/// What a host does once when it opens the dats, and what a test does before it builds a screen.
pub fn install(
    ui: &mut UiSystem,
    assets: Rc<dyn AssetSource>,
    resolver: Rc<dyn LayoutEnumResolver>,
) {
    install_env(ui, Env::new(assets, resolver));
}

/// [`install`] for an [`Env`] the caller already holds.
pub fn install_env(ui: &mut UiSystem, env: Env) {
    crate::panels::numfmt::prime(&env);
    ui.set_env(Some(env));
}

/// The client's create-and-add-root-element `(layout enum, element id)`, against the environment
/// `ui` holds.
pub fn create_and_add_root_element(
    ui: &mut UiSystem,
    layout: dereth_ui::framework::LayoutEnum,
    element: dereth_ui::ElementId,
) -> Result<dereth_ui::ElemHandle, dereth_ui::UiError> {
    ui.require_env()?
        .create_and_add_root_element(ui, layout, element)
}

/// [`Env::create_child_element_by_enum`], against the environment `ui` holds.
pub fn create_child_element_by_enum(
    ui: &mut UiSystem,
    parent: dereth_ui::ElemHandle,
    layout: dereth_ui::framework::LayoutEnum,
    element: dereth_ui::ElementId,
) -> Result<dereth_ui::ElemHandle, dereth_ui::UiError> {
    ui.require_env()?
        .create_child_element_by_enum(ui, parent, layout, element)
}

/// [`Env::create_child_element_by_data_id`], against the environment `ui` holds.
pub fn create_child_element_by_data_id(
    ui: &mut UiSystem,
    parent: dereth_ui::ElemHandle,
    layout: dereth_primitives::DataId,
    element: dereth_ui::ElementId,
) -> Result<dereth_ui::ElemHandle, dereth_ui::UiError> {
    ui.require_env()?
        .create_child_element_by_data_id(ui, parent, layout, element)
}

/// [`Env::did_by_enum`], against the environment `ui` holds; `None` when it holds none.
#[must_use]
pub fn did_by_enum(ui: &UiSystem, group: u32, value: u32) -> Option<dereth_primitives::DataId> {
    ui.env()?.did_by_enum(group, value)
}
