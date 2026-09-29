//! The presentation seam: the device the frame loop draws through (`Presentation`), the
//! presentation a headless run has (`NullPresentation`), and the drawn world as the frame reads
//! and writes it (`Scene`, `SceneMut`).
//!
//! `Scene` is what the drawn world looks like from outside the device: the body, the census
//! lines, the sound listener, the environment options. `Presentation` is the device itself.
//!
//! Backend-neutral by construction: no method, type or argument here
//! names a graphics API.

use dereth_primitives::{CellId, DataId, Frame, ObjectId, Viewport};

/// The scene counters the three census lines in `App` print — `ViewerBlockReport`,
/// `ObjectReport` and `report_scene`.
///
/// An owned plain struct rather than `crate::world::SceneStats` itself, because that type carries
/// `crate::particles::ParticleStats` and `crate::sky::SkyStats`, both of which hold device state;
/// these ten are every field the ungated half of `App` reads.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct SceneCensus {
    pub blocks_meshed: usize,
    pub terrain_surfaces: usize,
    pub scenery_objects: usize,
    pub buildings: usize,
    pub static_objects: usize,
    pub object_triangles: usize,
    pub server_objects_animated: usize,
    pub server_objects_held: usize,
    pub server_object_setups: usize,
    pub server_object_triangles: usize,
}

/// The drawn world, seen from outside the device.
///
/// Implemented by `crate::world::WorldScene` behind the device feature. Everything here is either
/// plain data or a type from a crate that has no device in it (`dereth_animation`, `dereth_audio`,
/// `dereth_assets`, `dereth_physics`, `crate::character`).
pub trait Scene: std::fmt::Debug {
    /// The world state this scene draws: the application's, beside the drawing half.
    fn world(&self) -> &crate::world_state::WorldState;

    // ---------------------------------------------------------------------------------------
    // the local body
    // ---------------------------------------------------------------------------------------

    /// The local body, or `None` before `0xF746` has stood one up.
    fn character(&self) -> Option<&crate::character::Character>;

    // ---------------------------------------------------------------------------------------
    // what the census lines print
    // ---------------------------------------------------------------------------------------

    fn census(&self) -> SceneCensus;
    /// The world-object store's drawable count.
    fn server_object_count(&self) -> usize;
    /// The viewer's cell, which is what tells an indoor frame from an outdoor one.
    fn viewer_cell_id(&self) -> Option<CellId>;
    /// The landscape window's centre.
    fn viewer_block(&self) -> Option<(i32, i32)>;
    /// Whether a landblock within the player's reach is still waiting to be built. The portal
    /// tunnel is held while this is true. A scene that builds everything at once never is.
    fn loading_near_viewer(&self) -> bool {
        false
    }
    /// How many landblocks are still queued to be built.
    fn blocks_pending(&self) -> usize {
        0
    }
    /// `WorldScene::env_cell_counts().1` — interior draw batches in the viewer's own cell.
    fn interior_batches(&self) -> usize;

    // ---------------------------------------------------------------------------------------
    // reads the UI and the HUD make
    // ---------------------------------------------------------------------------------------

    /// The map panel's displayed date and time pair.
    fn game_date_time(&self) -> Option<(String, String)>;
    /// The gameplay UI's FPS meter reads this frame rate.
    fn frame_rate_fps(&self) -> f32;
    /// …and the three degrade values the same meter formats beside it: whether the multiplier
    /// follows the frame rate, the automatic multiplier, and the user-supplied bias.
    fn degrade_meter(&self) -> (bool, f32, f32);
    /// The render preferences held by this scene.
    fn render_preferences(&self) -> crate::render_prefs::RenderPreferences;
    /// A server object's frame, for the sound placement and the selection geometry.
    fn server_object_frame(&self, id: ObjectId) -> Option<Frame>;
    /// The 3D viewport and the projection the frame is drawn with -- the smart box element's
    /// rectangle, not the window's. `(0, 0)` asks for the whole back buffer.
    fn effective_viewport(&self, width: u32, height: u32) -> Viewport;
    fn view_params(&self, width: u32, height: u32) -> dereth_client_contract::camera::ViewParams;
    /// The scene as the pick sweep reads it (`crate::pick::PickScene`).
    fn as_pick_scene(&self) -> &dyn crate::pick::PickScene;

    // ---------------------------------------------------------------------------------------
    // the draw's half of the selected-parts seam
    // ---------------------------------------------------------------------------------------

    fn take_selected_part_drawn(&self) -> bool;
    fn clear_selected_part_drawn(&self);
    /// Set the object whose selected parts the draw tracks.
    fn set_selected_object_id(&self, id: Option<ObjectId>);

    // ---------------------------------------------------------------------------------------
    // the sound system's reads
    // ---------------------------------------------------------------------------------------

    fn listener(&self) -> dereth_audio::Listener;
    fn terrain_neighbourhood(&self) -> dereth_audio::TerrainNeighbourhood;
    fn character_sound_table(&self) -> Option<DataId>;
    fn region(&self) -> &dereth_assets::Region;
}

/// The writes on the drawn world, split off `Scene`, which is the reads
/// alone: the application holds the world state and the presentation the drawing half, so a
/// read-only view of the two (one shared borrow of each) can be a `Scene` without being able to
/// write, and only a view holding both mutably is a `SceneMut`.
pub trait SceneMut: Scene {
    /// The world state, writable. The writes that touch nothing but the
    /// simulation are its own methods (`look`, `follow_character_now`, the movement dispatch and
    /// its latch, object geometry and lighting, the motion-table swap, the body itself); what
    /// stays on this trait is what also reaches the drawing half.
    fn world_mut(&mut self) -> &mut crate::world_state::WorldState;
    fn take_sound_events(&mut self) -> Vec<crate::audio::SoundTrigger>;

    // ---------------------------------------------------------------------------------------
    // writes
    // ---------------------------------------------------------------------------------------

    /// The smart-box simulation step.
    fn update(
        &mut self,
        input: crate::camera::CameraInput,
        character: crate::character::CharacterInput,
        now: dereth_primitives::LocalTime,
        dt: f32,
    );
    /// The swept camera's frame becomes the render camera.
    fn apply_camera_translucency(&mut self);
    /// The landscape window's release of every block the player teleported away from.
    fn release_landscape_for_teleport(&mut self, destination: dereth_primitives::LandblockId);
    /// The barber panel's two local effects.
    fn replace_player_particle_script(&mut self, script: DataId) -> bool;
    /// The player system's process-owned environment presets.
    fn set_environment_override_state(
        &mut self,
        state: crate::environment::EnvironmentOverrideState,
    );
    fn sync_environment_override_flags(&mut self);
    /// The player module's three environment-change arms.
    fn set_always_daylight(&mut self, on: bool) -> bool;
    fn set_weather_enabled(&mut self, on: bool);
    fn set_world_fog(&mut self, on: bool) -> bool;
}

/// A presentation's failure, as the application reports it: the device's own message.
///
/// The trait returns this rather than a renderer's error type, so the application loop names no
/// renderer; a device presentation converts its errors on the way out, and the message the
/// application prints (`Display`) is the device error's own text, unchanged.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PresentError(pub String);

impl std::fmt::Display for PresentError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(&self.0)
    }
}

impl std::error::Error for PresentError {}

/// The device, and every service the frame loop asks of it: the four device steps of the
/// documented frame order (`PrepareDevice`, `BeginFrame`, the world draw, `PresentFrame` -- see
/// [`crate::frame::FrameStep`]), the back buffer and its presentation parameters, the screenshot,
/// the preference names the device owns, and the world scene's lifetime.
///
/// What draws over the world -- the UI overlay, its textures, the preview spaces -- is the front
/// end's, and a front end that has one extends this trait with it.
///
/// Backend-neutral by construction: no method, type or argument here names a graphics API.
pub trait Presentation: std::fmt::Debug {
    /// For the accessors that hand the concrete device back to a front end or a test.
    fn as_any(&self) -> &dyn std::any::Any;
    fn as_any_mut(&mut self) -> &mut dyn std::any::Any;

    // ---------------------------------------------------------------------------------------
    // the frame bracket -- `FrameStep::PrepareDevice` .. `FrameStep::PresentFrame`
    // ---------------------------------------------------------------------------------------

    fn prepare_graphics_device(&mut self);
    fn start_frame(&mut self) -> Result<(), PresentError>;
    fn set_game_viewport(&mut self, viewport: Option<Viewport>);
    /// Draw the world half of the frame, from the world state the application owns.
    fn draw_scene(
        &mut self,
        world: Option<&crate::world_state::WorldState>,
    ) -> Result<(), PresentError>;
    fn end_frame(&mut self) -> Result<(), PresentError>;
    /// The last release of the shutdown sequence.
    fn wait_idle(&mut self) -> Result<(), PresentError>;

    // ---------------------------------------------------------------------------------------
    // the back buffer and the presentation parameters
    // ---------------------------------------------------------------------------------------

    fn size(&self) -> (u32, u32);
    fn resize(&mut self, width: u32, height: u32) -> Result<(), PresentError>;
    fn set_presentation_sync(&mut self, full_screen: bool, sync_to_refresh: bool);
    /// Save the current back buffer as a PNG.
    ///
    /// # Errors
    /// Whatever the device answers; a presentation with no device answers `Ok(())`.
    fn capture_png(&mut self, path: &std::path::Path) -> Result<(), PresentError>;

    // ---------------------------------------------------------------------------------------
    // the preference names the device owns
    // ---------------------------------------------------------------------------------------

    fn texture_filtering(&self) -> u32;
    /// The device-owned `Render.*` names alone.
    fn apply_device_preference_requests(
        &mut self,
        requests: Vec<dereth_client_contract::UiRequest>,
    ) -> Vec<dereth_client_contract::UiRequest>;
    /// The device-owned names **and** the scene-owned ones.
    fn apply_render_preference_requests(
        &mut self,
        requests: Vec<dereth_client_contract::UiRequest>,
    ) -> Vec<dereth_client_contract::UiRequest>;

    // ---------------------------------------------------------------------------------------
    // the world scene's lifetime
    // ---------------------------------------------------------------------------------------

    /// Stand one decoded surface up as the whole scene.
    ///
    /// # Errors
    /// When the surface is missing or will not decode.
    fn load_first_pixel_scene(
        &mut self,
        assets: &dyn dereth_primitives::AssetSource,
        id: DataId,
    ) -> Result<(), PresentError>;
    /// Build the world around a landblock.
    ///
    /// # Errors
    /// When the region, the landblock or a device resource is unavailable.
    ///
    /// The world state it builds is handed back in `world`, which the application owns; the
    /// drawing half stays here. A presentation with no device leaves `world` as it was.
    fn load_world(
        &mut self,
        store: &std::sync::Arc<dereth_dat::RetailDatStore>,
        cfg: crate::scene::SceneConfig,
        world: &mut Option<crate::world_state::WorldState>,
    ) -> Result<(), crate::landblock::WorldError>;
    /// Returns the number of textures released. The world state goes with the drawing half: a
    /// presentation that had a scene clears `world`.
    fn release_world(&mut self, world: &mut Option<crate::world_state::WorldState>) -> u32;
    /// Build the landblocks the window scrolled onto this frame.
    ///
    /// # Errors
    /// When a landblock will not build.
    fn stream_world(
        &mut self,
        store: &dereth_dat::RetailDatStore,
        world: Option<&mut crate::world_state::WorldState>,
    ) -> Result<(), crate::landblock::WorldError>;
    /// # Errors
    /// When an object's geometry will not build.
    fn sync_objects(
        &mut self,
        store: &std::sync::Arc<dereth_dat::RetailDatStore>,
        stream: &mut crate::objects::ObjectStream,
        world: Option<&mut crate::world_state::WorldState>,
    ) -> Result<(), crate::landblock::WorldError>;
    /// # Errors
    /// As [`Presentation::sync_objects`].
    fn prepare_object_dispatch(
        &mut self,
        store: &std::sync::Arc<dereth_dat::RetailDatStore>,
        stream: &mut crate::objects::ObjectStream,
        world: Option<&mut crate::world_state::WorldState>,
    ) -> Result<(), crate::landblock::WorldError>;
    /// Poll changed render preferences and apply any requested scene rebuilds.
    ///
    /// # Errors
    /// When the rebuild a changed preference asked for fails.
    fn update_render_preferences(
        &mut self,
        store: &dereth_dat::RetailDatStore,
        world: Option<&mut crate::world_state::WorldState>,
    ) -> Result<crate::frame_events::RenderPrefWork, crate::landblock::WorldError>;
    /// Set whether the world is hidden and the optional view-distance override.
    fn set_world_view_state(&mut self, hidden: bool, view_distance: Option<f32>);

    /// The drawn world as one read-only view: the application's world state beside this
    /// presentation's drawing half. `None` when either is missing.
    fn scene<'a>(
        &'a self,
        world: Option<&'a crate::world_state::WorldState>,
    ) -> Option<Box<dyn Scene + 'a>>;
    /// …and the same view with both halves writable.
    fn scene_mut<'a>(
        &'a mut self,
        world: Option<&'a mut crate::world_state::WorldState>,
    ) -> Option<Box<dyn SceneMut + 'a>>;
}

/// How many times a `NullPresentation` was asked for each device step.
///
/// The headless path performs no work, but it is still *asked*, and the fourteen `FrameStep`s are
/// still recorded in order. These counters are what lets a test say the frame reached the device
/// steps rather than only that it did not crash. The overlay's counters are counted by the front
/// end that draws one.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct NullPresentationCounts {
    pub prepare_graphics_device: u64,
    pub start_frame: u64,
    pub set_game_viewport: u64,
    pub draw_scene: u64,
    pub draw_ui: u64,
    pub end_frame: u64,
    pub wait_idle: u64,
    pub resize: u64,
    pub prepare_ui: u64,
    pub release_ui_textures: u64,
    pub set_movie_frame: u64,
    pub capture_png: u64,
    pub load_world: u64,
    pub release_world: u64,
    pub stream_world: u64,
    pub sync_objects: u64,
    pub prepare_object_dispatch: u64,
    pub update_render_preferences: u64,
    pub preview_calls: u64,
}

/// The presentation a headless `App` gets: it draws nothing and holds no scene, and records what
/// it was asked to do.
///
/// It is deliberately **not** a silent stub. `NullPresentation::size` answers the extent it was
/// constructed with, so a UI lays out against a real display rectangle exactly as it does with a
/// device, and every device step increments its counter.
#[derive(Debug, Clone, Default)]
pub struct NullPresentation {
    size: (u32, u32),
    texture_filtering: u32,
    counts: NullPresentationCounts,
}

impl NullPresentation {
    /// A presentation whose back buffer is `width` x `height` and which draws nothing.
    #[must_use]
    pub fn new(width: u32, height: u32) -> Self {
        Self {
            size: (width, height),
            texture_filtering: 0,
            counts: NullPresentationCounts::default(),
        }
    }

    /// What this presentation has been asked to do.
    #[must_use]
    pub const fn counts(&self) -> NullPresentationCounts {
        self.counts
    }

    /// The counters, for a front end that asks this presentation for its overlay too.
    pub fn counts_mut(&mut self) -> &mut NullPresentationCounts {
        &mut self.counts
    }
}

impl Presentation for NullPresentation {
    fn as_any(&self) -> &dyn std::any::Any {
        self
    }
    fn as_any_mut(&mut self) -> &mut dyn std::any::Any {
        self
    }

    fn prepare_graphics_device(&mut self) {
        self.counts.prepare_graphics_device += 1;
    }
    fn start_frame(&mut self) -> Result<(), PresentError> {
        self.counts.start_frame += 1;
        Ok(())
    }
    fn set_game_viewport(&mut self, _viewport: Option<Viewport>) {
        self.counts.set_game_viewport += 1;
    }
    fn draw_scene(
        &mut self,
        _world: Option<&crate::world_state::WorldState>,
    ) -> Result<(), PresentError> {
        self.counts.draw_scene += 1;
        Ok(())
    }
    fn end_frame(&mut self) -> Result<(), PresentError> {
        self.counts.end_frame += 1;
        Ok(())
    }
    fn wait_idle(&mut self) -> Result<(), PresentError> {
        self.counts.wait_idle += 1;
        Ok(())
    }

    fn size(&self) -> (u32, u32) {
        self.size
    }
    fn resize(&mut self, width: u32, height: u32) -> Result<(), PresentError> {
        self.counts.resize += 1;
        self.size = (width, height);
        Ok(())
    }
    fn set_presentation_sync(&mut self, _full_screen: bool, _sync_to_refresh: bool) {}
    fn capture_png(&mut self, _path: &std::path::Path) -> Result<(), PresentError> {
        self.counts.capture_png += 1;
        Ok(())
    }

    fn texture_filtering(&self) -> u32 {
        self.texture_filtering
    }
    fn apply_device_preference_requests(
        &mut self,
        requests: Vec<dereth_client_contract::UiRequest>,
    ) -> Vec<dereth_client_contract::UiRequest> {
        requests
    }
    fn apply_render_preference_requests(
        &mut self,
        requests: Vec<dereth_client_contract::UiRequest>,
    ) -> Vec<dereth_client_contract::UiRequest> {
        requests
    }

    fn load_first_pixel_scene(
        &mut self,
        _assets: &dyn dereth_primitives::AssetSource,
        _id: DataId,
    ) -> Result<(), PresentError> {
        Ok(())
    }
    fn load_world(
        &mut self,
        _store: &std::sync::Arc<dereth_dat::RetailDatStore>,
        _cfg: crate::scene::SceneConfig,
        _world: &mut Option<crate::world_state::WorldState>,
    ) -> Result<(), crate::landblock::WorldError> {
        self.counts.load_world += 1;
        Ok(())
    }
    fn release_world(&mut self, _world: &mut Option<crate::world_state::WorldState>) -> u32 {
        self.counts.release_world += 1;
        0
    }
    fn stream_world(
        &mut self,
        _store: &dereth_dat::RetailDatStore,
        _world: Option<&mut crate::world_state::WorldState>,
    ) -> Result<(), crate::landblock::WorldError> {
        self.counts.stream_world += 1;
        Ok(())
    }
    fn sync_objects(
        &mut self,
        _store: &std::sync::Arc<dereth_dat::RetailDatStore>,
        _stream: &mut crate::objects::ObjectStream,
        _world: Option<&mut crate::world_state::WorldState>,
    ) -> Result<(), crate::landblock::WorldError> {
        self.counts.sync_objects += 1;
        Ok(())
    }
    fn prepare_object_dispatch(
        &mut self,
        _store: &std::sync::Arc<dereth_dat::RetailDatStore>,
        _stream: &mut crate::objects::ObjectStream,
        _world: Option<&mut crate::world_state::WorldState>,
    ) -> Result<(), crate::landblock::WorldError> {
        self.counts.prepare_object_dispatch += 1;
        Ok(())
    }
    fn update_render_preferences(
        &mut self,
        _store: &dereth_dat::RetailDatStore,
        _world: Option<&mut crate::world_state::WorldState>,
    ) -> Result<crate::frame_events::RenderPrefWork, crate::landblock::WorldError> {
        self.counts.update_render_preferences += 1;
        Ok(crate::frame_events::RenderPrefWork::default())
    }
    fn set_world_view_state(&mut self, _hidden: bool, _view_distance: Option<f32>) {}

    fn scene<'a>(
        &'a self,
        _world: Option<&'a crate::world_state::WorldState>,
    ) -> Option<Box<dyn Scene + 'a>> {
        None
    }
    fn scene_mut<'a>(
        &'a mut self,
        _world: Option<&'a mut crate::world_state::WorldState>,
    ) -> Option<Box<dyn SceneMut + 'a>> {
        None
    }
}
