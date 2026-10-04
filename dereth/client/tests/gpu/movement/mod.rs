//! Movement: the player's own body and remote creatures -- walking, running, turning,
//! jumping, approaches (move-to and turn-to), sticking, teleports, and the ground and
//! scenery the body collides with.

// The binary's shared helpers, reachable from this area's modules as `super::common`.
use super::common;

mod animation_hooks;
mod embodied_character;
mod ledge_edge_slide;
mod local_motion_style;
mod motion_continuations;
mod player_animation_stability;
mod remote_approach_replacement;
mod remote_motion_style;
mod remote_root_motion;
mod remote_single_position;
mod remote_stick_to_object;
mod remote_walk_animation;
mod scenery_collision;
mod stance_change_stop_and_resume;
mod vector_and_position_receivers;
