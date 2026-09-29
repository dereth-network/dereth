//! The world around the viewer: the landblock window and how it streams, recentres and releases,
//! the viewer's cell, interiors and the statics baked into them, object placement, houses, and
//! what a teleport draws on the way and on arrival.

mod cell_statics;
mod coarse_block_objects;
mod dungeon_landblock_window;
mod house_barrier;
mod interiors;
mod landblock_interior_release;
mod landblock_object_release;
mod landblock_recentre;
mod landblock_round_trip;
mod landblock_streaming;
mod placement_surface_clamp;
mod portal_space_chat_line;
mod portal_tunnel_view;
mod scale_hook_seam;
mod scripted_static_collision_body;
mod streaming_slot_release;
mod teleport_arrival_draw;
mod unplaceable_object_lifetime;
mod viewer_cell;
mod viewpoint_sweep_lag;
