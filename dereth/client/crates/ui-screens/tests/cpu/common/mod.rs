//! Shared synthetic UI fixtures.

use dereth_ui::UiSystem;

pub(crate) fn ui() -> UiSystem {
    UiSystem::new((800, 600))
}

pub(crate) mod panel_catalogue;
