//! World streaming and rendering over shared simulation state.
//!
//! The scene owns device resources, baked geometry and palettes, particle emitters,
//! draw ordering, lighting and the view of the active landblock window. Pure world
//! rules live in `dereth-world-render`; device commands live in `dereth-render`.
//! Cohesive child modules share the scene records without changing resource ownership.

/// The retail region record, the landblock identity helpers and [`WorldError`].
///
/// Defined in [`dereth_client_runtime::landblock`], because `land_source.rs`, `env_cells.rs` and
/// `character.rs` are core modules and all three name them. The `dereth_client::world` paths
/// (`dereth_client::world::lbi_did` and its siblings) resolve through this `pub use`.
pub use dereth_client_runtime::landblock::{
    block_xy, landblock_did, lbi_did, load_region, WorldError, DEFAULT_LANDBLOCK, DERETH_REGION,
};

/// Process-owned administrative environment presets.
///
/// The whole cluster -- `EnvironmentOverride`, [`EnvironmentOverrideState`],
/// `admin_environs_radar_blank` and `admin_environs_override` -- is defined in
/// [`dereth_client_runtime::environment`], because `present::Scene::set_environment_override_state`
/// names the handle and that trait belongs to the headless crate. The struct, its seven fields and
/// `snapshot`/`advance` are `pub` there, with no device-feature gate; the path
/// `dereth_client::world::EnvironmentOverrideState` resolves through this `pub use`.
pub use dereth_client_runtime::environment::{EnvironmentOverride, EnvironmentOverrideState};
/// What one [`WorldScene::update_from_preferences`] poll actually did.
///
/// Defined in [`dereth_client_runtime::frame_events`], because it is the payload of
/// `FrameEvent::RenderPreferencesPolled`. It is six flags and three counts and names nothing
/// else; the path `dereth_client::world::RenderPrefWork` resolves through this `pub use`.
pub use dereth_client_runtime::frame_events::RenderPrefWork;
/// What the scene is built from: `dereth_client_runtime`'s, re-exported at this path.
pub use dereth_client_runtime::scene::SceneConfig;

/// The wire `ObjDesc` as the animation runtime's one. Defined in
/// [`dereth_client_runtime::movement::to_anim_objdesc`] and re-exported here, so
/// `dereth_client::world::to_anim_objdesc` resolves.
pub use dereth_client_runtime::movement::to_anim_objdesc;

#[cfg(any(feature = "vulkan", feature = "wgpu", all(windows, feature = "d3d12")))]
pub use imp::*;

#[cfg(any(feature = "vulkan", feature = "wgpu", all(windows, feature = "d3d12")))]
#[path = "world_scene/imp.rs"]
mod imp;
