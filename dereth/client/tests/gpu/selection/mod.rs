//! Selection: picking objects in the world with the pointer, the hover tooltip, the target
//! brackets and indicator, the selection blink, selection cycling and automatic targeting, and
//! the range checks that close panels and drop selections when the player walks away.

mod auto_target;
mod hover_tooltip_occlusion;
mod indoor_pick_outdoor_objects;
mod indoor_portal_clip;
mod object_hover_tooltip;
mod object_range_checks;
mod open_door_pick;
mod select_self;
mod selection_blink;
mod selection_cycle_actions;
mod selection_persistence;
mod self_target_indicator;
mod target_brackets;
mod viewport_click_passthrough;
