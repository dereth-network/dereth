//! The user interface on a real device: font and text rasterisation, element fills and tiling,
//! edit-field highlights, the drag ghost, keyboard focus and the typing barrier, the escape and
//! action keys, the shipped key bindings, and the saved screen layout.

mod drag_after_resize;
mod edit_field_selection_highlight;
mod escape_and_action_arms;
mod every_bindable_action;
mod fill_and_erase;
mod focus_loss_release;
mod font_rasterisation;
mod panel_pixel_residuals;
mod screen_layout_persistence;
mod shipped_key_bindings;
mod text_outline_pass;
mod tiling_and_rotation;
mod typing_barrier;
