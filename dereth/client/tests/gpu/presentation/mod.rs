//! Presentation: the window, resolution and display preferences, sync to refresh, screenshots and the headless capture,
//! frame pacing without a physics tick, the shell's mode transitions, shutdown, the world clock,
//! and what stays bounded over a long session.

mod frame_independence;
mod headless_capture_determinism;
mod headless_settings;
mod long_session_growth;
mod long_session_mesh_eviction;
mod pre_tod_capture;
mod resolution_dropdown;
mod screens_at_resolution;
mod screenshot_file;
mod selected_preferences_profile;
mod shell;
mod shutdown;
mod static_event_queue_growth;
mod sync_to_refresh;
mod window_position;
mod world_clock_sync;
