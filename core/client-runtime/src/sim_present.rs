//! A presentation with no device that still builds and drives the world: the local body, its
//! physics, the streaming window, the server's objects and the calendar, with nothing drawn.
//!
//! [`crate::present::NullPresentation`] leaves the world empty, which is what a run that wants no
//! world asks for. `SimPresentation` is the same device (it draws nothing and counts what it was
//! asked to do) with the world builder behind the world methods: loading builds the world and
//! attaches the body ([`crate::world_build::load`]), streaming builds the blocks the window
//! wants, object dispatch is [`crate::world_objects`] with no appearance, and the scene's update
//! is [`crate::world_step::update`]. So an action a script or a front end injects reaches a body
//! that moves.
//!
//! A presentation that draws can wrap this one and draw the world state it holds.

use std::sync::Arc;

use dereth_dat::RetailDatStore;
use dereth_primitives::{CellId, DataId, Frame, ObjectId, Viewport};

use crate::present::{NullPresentation, PresentError, Presentation, Scene, SceneCensus, SceneMut};
use crate::scene::SceneConfig;
use crate::world_build::BlockResidency;
use crate::world_objects::{NoAppearance, ObjectCounters};
use crate::world_state::WorldState;
use crate::world_step::StepCounters;

/// The device-free presentation that builds a world. See the module header.
#[derive(Debug)]
pub struct SimPresentation {
    device: NullPresentation,
    world: Option<SimWorld>,
    /// What the frame simulation and object dispatch counted, over the whole run.
    pub steps: StepCounters,
    pub objects: ObjectCounters,
}

/// The half of a device-free world the presentation holds beside the application's `WorldState`:
/// the configuration it was loaded with and its resident blocks.
#[derive(Debug)]
struct SimWorld {
    cfg: SceneConfig,
    residency: BlockResidency,
}

impl SimPresentation {
    /// A presentation whose back buffer is `width` x `height`, with no world until one is loaded.
    #[must_use]
    pub fn new(width: u32, height: u32) -> Self {
        Self {
            device: NullPresentation::new(width, height),
            world: None,
            steps: StepCounters::default(),
            objects: ObjectCounters::default(),
        }
    }

    /// The device half, and what it was asked to do.
    #[must_use]
    pub fn device(&self) -> &NullPresentation {
        &self.device
    }

    /// The device counters, for a front end that also submits UI operations.
    pub fn counts_mut(&mut self) -> &mut crate::present::NullPresentationCounts {
        self.device.counts_mut()
    }

    /// The resident blocks, when a world is loaded.
    pub fn resident_blocks(&self) -> impl Iterator<Item = (i32, i32)> + '_ {
        self.world.iter().flat_map(|w| w.residency.resident())
    }
}

impl Presentation for SimPresentation {
    fn as_any(&self) -> &dyn std::any::Any {
        self
    }
    fn as_any_mut(&mut self) -> &mut dyn std::any::Any {
        self
    }

    fn prepare_graphics_device(&mut self) {
        self.device.prepare_graphics_device();
    }
    fn start_frame(&mut self) -> Result<(), PresentError> {
        self.device.start_frame()
    }
    fn set_game_viewport(&mut self, viewport: Option<Viewport>) {
        self.device.set_game_viewport(viewport);
    }
    fn draw_scene(&mut self, world: Option<&WorldState>) -> Result<(), PresentError> {
        self.device.draw_scene(world)
    }
    fn end_frame(&mut self) -> Result<(), PresentError> {
        self.device.end_frame()
    }
    fn wait_idle(&mut self) -> Result<(), PresentError> {
        self.device.wait_idle()
    }

    fn size(&self) -> (u32, u32) {
        self.device.size()
    }
    fn resize(&mut self, width: u32, height: u32) -> Result<(), PresentError> {
        self.device.resize(width, height)
    }
    fn set_presentation_sync(&mut self, full_screen: bool, sync_to_refresh: bool) {
        self.device
            .set_presentation_sync(full_screen, sync_to_refresh);
    }
    fn capture_png(&mut self, path: &std::path::Path) -> Result<(), PresentError> {
        self.device.capture_png(path)
    }

    fn texture_filtering(&self) -> u32 {
        self.device.texture_filtering()
    }
    fn apply_device_preference_requests(
        &mut self,
        requests: Vec<dereth_client_contract::UiRequest>,
    ) -> Vec<dereth_client_contract::UiRequest> {
        self.device.apply_device_preference_requests(requests)
    }
    fn apply_render_preference_requests(
        &mut self,
        requests: Vec<dereth_client_contract::UiRequest>,
    ) -> Vec<dereth_client_contract::UiRequest> {
        self.device.apply_render_preference_requests(requests)
    }

    fn load_first_pixel_scene(
        &mut self,
        assets: &dyn dereth_primitives::AssetSource,
        id: DataId,
    ) -> Result<(), PresentError> {
        self.device.load_first_pixel_scene(assets, id)
    }

    /// The world, built with no device, with a body when the configuration asks for one.
    fn load_world(
        &mut self,
        store: &Arc<RetailDatStore>,
        cfg: SceneConfig,
        world: &mut Option<WorldState>,
    ) -> Result<(), dereth_world_data::landblock::WorldError> {
        let _ = self.device.load_world(store, cfg, world);
        let (ws, residency) = crate::world_build::load(store, &cfg)?;
        *world = Some(ws);
        self.world = Some(SimWorld { cfg, residency });
        Ok(())
    }

    fn release_world(&mut self, world: &mut Option<WorldState>) -> u32 {
        let _ = self.device.release_world(world);
        self.world = None;
        *world = None;
        0
    }

    fn stream_world(
        &mut self,
        store: &RetailDatStore,
        world: Option<&mut WorldState>,
    ) -> Result<(), dereth_world_data::landblock::WorldError> {
        let _ = self.device.stream_world(store, None);
        if let (Some(ws), Some(sim)) = (world, self.world.as_mut()) {
            sim.residency.stream(ws, store, &sim.cfg);
        }
        Ok(())
    }

    fn sync_objects(
        &mut self,
        store: &Arc<RetailDatStore>,
        stream: &mut crate::objects::ObjectStream,
        world: Option<&mut WorldState>,
    ) -> Result<(), dereth_world_data::landblock::WorldError> {
        let _ = self.device.sync_objects(store, stream, None);
        if let (Some(ws), Some(sim)) = (world, self.world.as_ref()) {
            let Ok(()) = crate::world_objects::sync_objects(
                ws,
                store,
                &sim.cfg,
                stream,
                &mut self.objects,
                &mut NoAppearance,
            );
        }
        Ok(())
    }

    fn prepare_object_dispatch(
        &mut self,
        store: &Arc<RetailDatStore>,
        stream: &mut crate::objects::ObjectStream,
        world: Option<&mut WorldState>,
    ) -> Result<(), dereth_world_data::landblock::WorldError> {
        let _ = self.device.prepare_object_dispatch(store, stream, None);
        if let (Some(ws), Some(sim)) = (world, self.world.as_ref()) {
            let Ok(()) = crate::world_objects::prepare_object_dispatch(
                ws,
                store,
                &sim.cfg,
                stream,
                &mut self.objects,
                &mut NoAppearance,
            );
        }
        Ok(())
    }

    fn update_render_preferences(
        &mut self,
        store: &RetailDatStore,
        _world: Option<&mut WorldState>,
    ) -> Result<crate::frame_events::RenderPrefWork, dereth_world_data::landblock::WorldError> {
        self.device.update_render_preferences(store, None)
    }

    fn set_world_view_state(&mut self, hidden: bool, view_distance: Option<f32>) {
        self.device.set_world_view_state(hidden, view_distance);
    }

    fn overlay_upload(
        &mut self,
        texture: dereth_client_contract::overlay::OverlayTexture,
        data: &dereth_primitives::TextureData,
    ) -> Result<(), PresentError> {
        self.device.overlay_upload(texture, data)
    }
    fn overlay_release(
        &mut self,
        texture: dereth_client_contract::overlay::OverlayTexture,
    ) -> dereth_client_contract::overlay::OverlayReleased {
        self.device.overlay_release(texture)
    }
    fn draw_overlay(
        &mut self,
        items: &[dereth_client_contract::overlay::OverlayItem],
    ) -> Result<(), PresentError> {
        self.device.draw_overlay(items)
    }
    fn set_ground_markers(&mut self, markers: &[dereth_client_contract::overlay::GroundMarker]) {
        self.device.set_ground_markers(markers);
    }

    fn preview_ensure(
        &mut self,
        id: dereth_client_contract::overlay::PreviewSpace,
        assets: &std::sync::Arc<dereth_world_data::anim_assets::DatAnimAssets>,
    ) -> bool {
        self.device.preview_ensure(id, assets)
    }
    fn preview_set_light(
        &mut self,
        id: dereth_client_contract::overlay::PreviewSpace,
        light: dereth_client_contract::overlay::PreviewLight,
        intensity: f32,
        direction: dereth_primitives::Vec3,
    ) {
        self.device
            .preview_set_light(id, light, intensity, direction)
    }
    fn preview_use_sharp_mode(&mut self, id: dereth_client_contract::overlay::PreviewSpace) {
        self.device.preview_use_sharp_mode(id)
    }
    fn preview_use_world_fov(&mut self, id: dereth_client_contract::overlay::PreviewSpace) {
        self.device.preview_use_world_fov(id)
    }
    fn preview_set_fov(&mut self, id: dereth_client_contract::overlay::PreviewSpace, radians: f32) {
        self.device.preview_set_fov(id, radians)
    }
    fn preview_set_camera_position(
        &mut self,
        id: dereth_client_contract::overlay::PreviewSpace,
        position: dereth_primitives::Vec3,
    ) {
        self.device.preview_set_camera_position(id, position)
    }
    fn preview_set_camera_direction(
        &mut self,
        id: dereth_client_contract::overlay::PreviewSpace,
        direction: dereth_primitives::Vec3,
    ) {
        self.device.preview_set_camera_direction(id, direction)
    }
    fn preview_set_camera_direction_degrees(
        &mut self,
        id: dereth_client_contract::overlay::PreviewSpace,
        degrees: dereth_primitives::Vec3,
    ) {
        self.device
            .preview_set_camera_direction_degrees(id, degrees)
    }
    fn preview_remove_all_objects(&mut self, id: dereth_client_contract::overlay::PreviewSpace) {
        self.device.preview_remove_all_objects(id)
    }
    fn preview_add_object(
        &mut self,
        id: dereth_client_contract::overlay::PreviewSpace,
        store: &dereth_dat::RetailDatStore,
        setup: DataId,
    ) -> Result<Option<usize>, PresentError> {
        self.device.preview_add_object(id, store, setup)
    }
    fn preview_add_object_dressed(
        &mut self,
        id: dereth_client_contract::overlay::PreviewSpace,
        store: &dereth_dat::RetailDatStore,
        setup: DataId,
        objdesc: Option<&dereth_animation::parts::ObjDesc>,
    ) -> Result<Option<usize>, PresentError> {
        self.device
            .preview_add_object_dressed(id, store, setup, objdesc)
    }
    fn preview_set_heading(
        &mut self,
        id: dereth_client_contract::overlay::PreviewSpace,
        index: usize,
        degrees: f32,
    ) {
        self.device.preview_set_heading(id, index, degrees)
    }
    fn preview_set_scale(
        &mut self,
        id: dereth_client_contract::overlay::PreviewSpace,
        index: usize,
        scale: f32,
    ) {
        self.device.preview_set_scale(id, index, scale)
    }
    fn preview_set_sequence_animation(
        &mut self,
        id: dereth_client_contract::overlay::PreviewSpace,
        index: usize,
        animation: DataId,
        clear: bool,
        low_frame: i32,
        framerate: f32,
    ) -> bool {
        self.device
            .preview_set_sequence_animation(id, index, animation, clear, low_frame, framerate)
    }
    fn preview_clear_sequence_anims(
        &mut self,
        id: dereth_client_contract::overlay::PreviewSpace,
        index: usize,
    ) {
        self.device.preview_clear_sequence_anims(id, index)
    }
    fn preview_has_anims(
        &self,
        id: dereth_client_contract::overlay::PreviewSpace,
        index: usize,
    ) -> bool {
        self.device.preview_has_anims(id, index)
    }
    fn preview_use_time(&mut self, id: dereth_client_contract::overlay::PreviewSpace, dt: f64) {
        self.device.preview_use_time(id, dt)
    }
    fn preview_curr_frame_number(
        &self,
        id: dereth_client_contract::overlay::PreviewSpace,
        index: usize,
    ) -> Option<u32> {
        self.device.preview_curr_frame_number(id, index)
    }
    fn preview_object_bounding_box(
        &self,
        id: dereth_client_contract::overlay::PreviewSpace,
        index: usize,
        store: &dereth_dat::RetailDatStore,
    ) -> Option<dereth_physics::geom::BBox> {
        self.device.preview_object_bounding_box(id, index, store)
    }
    fn preview_part_array_mut(
        &mut self,
        id: dereth_client_contract::overlay::PreviewSpace,
        index: usize,
    ) -> Option<&mut dereth_animation::parts::PartArray> {
        self.device.preview_part_array_mut(id, index)
    }

    fn scene<'a>(&'a self, world: Option<&'a WorldState>) -> Option<Box<dyn Scene + 'a>> {
        Some(Box::new(SimScene {
            ws: world?,
            sim: self.world.as_ref()?,
        }))
    }

    fn scene_mut<'a>(
        &'a mut self,
        world: Option<&'a mut WorldState>,
    ) -> Option<Box<dyn SceneMut + 'a>> {
        let Self {
            world: sim, steps, ..
        } = self;
        Some(Box::new(SimSceneMut {
            ws: world?,
            sim: sim.as_mut()?,
            steps,
        }))
    }
}

/// The world as the frame reads it, with nothing drawn.
#[derive(Debug)]
struct SimScene<'a> {
    ws: &'a WorldState,
    sim: &'a SimWorld,
}

/// …and as it writes it.
#[derive(Debug)]
struct SimSceneMut<'a> {
    ws: &'a mut WorldState,
    sim: &'a mut SimWorld,
    steps: &'a mut StepCounters,
}

/// The reads both views answer the same way, from the world state and the configuration.
trait SimHalves {
    fn halves(&self) -> (&WorldState, &SimWorld);
}

impl SimHalves for SimScene<'_> {
    fn halves(&self) -> (&WorldState, &SimWorld) {
        (self.ws, self.sim)
    }
}

impl SimHalves for SimSceneMut<'_> {
    fn halves(&self) -> (&WorldState, &SimWorld) {
        (self.ws, self.sim)
    }
}

/// The census of a world with no drawing: only what the world state counts.
fn census(ws: &WorldState, sim: &SimWorld) -> SceneCensus {
    SceneCensus {
        blocks_meshed: sim.residency.resident().count(),
        server_objects_animated: ws.objects.values().filter(|o| o.sim.animated).count(),
        server_objects_held: ws
            .objects
            .values()
            .filter(|o| o.sim.parent.is_some() && o.drawn)
            .count(),
        ..SceneCensus::default()
    }
}

macro_rules! impl_sim_scene_reads {
    ($ty:ty) => {
        impl Scene for $ty {
            fn world(&self) -> &WorldState {
                self.halves().0
            }
            fn character(&self) -> Option<&crate::character::Character> {
                self.halves().0.character.as_ref()
            }
            fn census(&self) -> SceneCensus {
                let (ws, sim) = self.halves();
                census(ws, sim)
            }
            fn server_object_count(&self) -> usize {
                self.halves().0.server_object_count()
            }
            fn viewer_cell_id(&self) -> Option<CellId> {
                self.halves().0.viewer_cell_id()
            }
            fn viewer_block(&self) -> Option<(i32, i32)> {
                self.halves().0.viewer_block()
            }
            fn loading_near_viewer(&self) -> bool {
                let (ws, sim) = self.halves();
                sim.residency.loading_near_viewer(ws, &sim.cfg)
            }
            fn blocks_pending(&self) -> usize {
                usize::from(self.halves().1.residency.has_pending())
            }
            fn interior_batches(&self) -> usize {
                0
            }
            fn game_date_time(&self) -> Option<(String, String)> {
                self.halves().0.game_date_time()
            }
            fn frame_rate_fps(&self) -> f32 {
                0.0
            }
            fn degrade_meter(&self) -> (bool, f32, f32) {
                (false, 1.0, 0.0)
            }
            fn render_preferences(&self) -> crate::render_prefs::RenderPreferences {
                self.halves().1.cfg.render
            }
            fn server_object_frame(&self, id: ObjectId) -> Option<Frame> {
                self.halves().0.server_object_frame(id)
            }
            fn effective_viewport(&self, width: u32, height: u32) -> Viewport {
                Viewport {
                    x: 0,
                    y: 0,
                    width,
                    height,
                }
            }
            fn view_params(
                &self,
                width: u32,
                height: u32,
            ) -> dereth_client_contract::camera::ViewParams {
                dereth_client_contract::camera::ViewParams {
                    viewport: self.effective_viewport(width, height),
                    fov_y_rad: self.halves().1.cfg.render.game_fov_rad(),
                    ..dereth_client_contract::camera::ViewParams::default()
                }
            }
            fn as_pick_scene(&self) -> &dyn crate::pick::PickScene {
                self
            }
            fn take_selected_part_drawn(&self) -> bool {
                false
            }
            fn set_selected_object_id(&self, _id: Option<ObjectId>) {}
            fn listener(&self) -> dereth_audio::Listener {
                self.halves().0.listener()
            }
            fn terrain_neighbourhood(&self) -> dereth_audio::TerrainNeighbourhood {
                dereth_audio::TerrainNeighbourhood::default()
            }
            fn character_sound_table(&self) -> Option<DataId> {
                self.halves().0.character_sound_table()
            }
            fn region(&self) -> &dereth_assets::Region {
                self.halves().1.residency.region()
            }
        }

        impl crate::pick::PickScene for $ty {
            fn viewer(&self) -> Frame {
                self.halves().0.camera.frame()
            }
            fn object_frame(&self, id: ObjectId) -> Option<Frame> {
                self.halves().0.server_object_frame(id)
            }
            fn fov_y_rad(&self, _screen: (u32, u32)) -> f32 {
                self.halves().1.cfg.render.game_fov_rad()
            }
        }
    };
}

impl_sim_scene_reads!(SimScene<'_>);
impl_sim_scene_reads!(SimSceneMut<'_>);

impl SceneMut for SimSceneMut<'_> {
    fn world_mut(&mut self) -> &mut WorldState {
        self.ws
    }

    fn take_sound_events(&mut self) -> Vec<crate::audio::SoundTrigger> {
        crate::world_step::process_hooks(self.ws, self.steps);
        std::mem::take(&mut self.ws.pending_sound)
    }

    fn update(
        &mut self,
        input: crate::camera::CameraInput,
        character: crate::character::CharacterInput,
        now: dereth_primitives::LocalTime,
        dt: f32,
    ) {
        crate::world_step::update(
            self.ws,
            &mut self.sim.residency,
            &self.sim.cfg,
            input,
            character,
            now,
            dt,
            self.steps,
        );
    }

    fn apply_camera_translucency(&mut self) {
        crate::world_step::apply_camera_translucency(self.ws, &self.sim.cfg);
    }

    fn release_landscape_for_teleport(&mut self, destination: dereth_primitives::LandblockId) {
        let SimWorld { cfg, residency } = &mut *self.sim;
        let _ = residency.release_for_teleport(self.ws, cfg, destination);
    }

    /// No particles are simulated with no device, so there is no manager to replace.
    fn replace_player_particle_script(&mut self, _script: DataId) -> bool {
        false
    }

    fn set_environment_override_state(
        &mut self,
        state: crate::environment::EnvironmentOverrideState,
    ) {
        self.ws.environment_override = state;
    }

    /// The override's flags only switch sky passes, and there is no sky.
    fn sync_environment_override_flags(&mut self) {}

    fn set_always_daylight(&mut self, on: bool) -> bool {
        if self.ws.always_daylight == on {
            return false;
        }
        self.ws.always_daylight = on;
        true
    }

    /// Weather is a sky layer, and there is no sky.
    fn set_weather_enabled(&mut self, _on: bool) {}

    fn set_world_fog(&mut self, on: bool) -> bool {
        if self.sim.cfg.world_fog == on {
            return false;
        }
        self.sim.cfg.world_fog = on;
        true
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::actions::{names, Action};
    use crate::app::{App, NullShell, Platform};
    use crate::config::Config;

    /// Behaviour: combat.power-bar.the-bodys-motion-style-sets-the-charge-rate
    #[test]
    #[cfg_attr(
        not(feature = "retail-dats"),
        ignore = "reads retail dats: --features retail-dats"
    )]
    fn input_and_frame_motion_bridges_count_only_changed_facts_and_leave_absent_bodies_alone() {
        let store = Arc::new(dereth_dat::testing::open_store().expect("retail dats"));
        let mut app = App::<NullShell>::bring_up_with_store(
            Config {
                headless: true,
                connect: false,
                sound: false,
                ..Config::default()
            },
            Some(Arc::clone(&store)),
            |_| Ok(Platform::headless(64, 64)),
            |_, _, _, _| Ok(Box::new(SimPresentation::new(64, 64))),
        )
        .expect("bring-up");
        app.load_static_scene(SceneConfig {
            landblock: 0xA9B4,
            character: true,
            ..SceneConfig::default()
        })
        .expect("world with body");
        let set_motion = |app: &App<NullShell>, style, forward| {
            let body = app.world.as_ref().unwrap().character.as_ref().unwrap();
            let mut driver = body.driver_mut();
            driver.movement.interp.interpreted_state.current_style =
                dereth_animation::MotionCommand(style);
            driver.movement.interp.interpreted_state.forward_command =
                dereth_animation::MotionCommand(forward);
        };
        let input = |app: &mut App<NullShell>| {
            app.interaction.prepare_ui_dispatch(
                app.world.as_ref().unwrap().character.as_ref(),
                &mut app.objects.world,
                (64, 64),
            );
        };
        let frame = |app: &mut App<NullShell>| {
            let scene = app.present.scene(app.world.as_ref());
            let _ = crate::interaction::draw_use_time_with_chat_focus(
                &mut app.interaction,
                &store,
                scene.as_deref(),
                &mut app.objects,
                None,
                Vec::new(),
                false,
                (64, 64),
                dereth_primitives::LocalTime(1.0),
                &mut |_| {},
            );
        };
        set_motion(&app, 0x8000_0046, 0x4100_0003);
        input(&mut app);
        assert_eq!(app.interaction.stats.combat_style_bridges, 2);
        frame(&mut app);
        assert_eq!(app.interaction.stats.combat_style_bridges, 2);
        assert_eq!(app.objects.world.combat.current_style, 0x8000_0046);
        assert_eq!(app.objects.world.combat.forward_command, 0x4100_0003);
        set_motion(&app, 0x8000_003d, 0x4100_0003);
        frame(&mut app);
        assert_eq!(app.interaction.stats.combat_style_bridges, 3);
        assert_eq!(app.objects.world.combat.current_style, 0x8000_003d);
        set_motion(&app, 0x8000_003d, 0x4500_0005);
        input(&mut app);
        assert_eq!(app.interaction.stats.combat_style_bridges, 4);
        assert_eq!(app.objects.world.combat.forward_command, 0x4500_0005);
        frame(&mut app);
        assert_eq!(app.interaction.stats.combat_style_bridges, 4);
        app.world.as_mut().unwrap().character = None;
        input(&mut app);
        frame(&mut app);
        assert_eq!(app.interaction.stats.combat_style_bridges, 4);
        assert_eq!(app.objects.world.combat.current_style, 0x8000_003d);
        assert_eq!(app.objects.world.combat.forward_command, 0x4500_0005);
    }

    /// Backing up through a cast, under the orbit camera (`orbit`, character-based) or the game's
    /// own: how far the body goes while the server plays the cast's gesture for the player and
    /// brings them back to the ready.
    fn backed_up_through_a_cast(orbit: bool) -> f32 {
        let store = Arc::new(dereth_dat::testing::open_store().expect("retail dats"));
        let mut app = App::<NullShell>::bring_up_with_store(
            Config {
                headless: true,
                frames: None,
                connect: false,
                sound: false,
                dat_dir: std::path::PathBuf::from("a directory that does not exist"),
                ..Config::default()
            },
            Some(store),
            |_| Ok(Platform::headless(64, 64)),
            |_, _, _, _| Ok(Box::new(SimPresentation::new(64, 64))),
        )
        .expect("bring-up");
        let mut shell = NullShell;
        app.start_shell(&mut shell).expect("the shell starts");
        app.load_static_scene(SceneConfig {
            landblock: 0xA9B4,
            character: true,
            ..SceneConfig::default()
        })
        .expect("the world loads");
        if orbit {
            app.orbit = Some(crate::orbit::OrbitSettings {
                movement: crate::orbit::MovementMode::Character,
                ..crate::orbit::OrbitSettings::default()
            });
        }
        // The server's motion for the player in the magic stance, at `forward` (an index into
        // the command table: 3 is Ready, 51 a cast gesture).
        let server_motion = |app: &mut App<NullShell>, stamp: u16, forward: u16| {
            let buf = dereth_protocol::movement::MovementBuffer {
                movement_timestamp: stamp,
                server_control_timestamp: stamp,
                autonomous: false,
                body: dereth_protocol::movement::MovementBody {
                    movement_type: 0,
                    motion_flags: 0,
                    current_style: 73,
                    interpreted: Some(dereth_protocol::movement::InterpretedMotionState {
                        current_style: Some(73),
                        forward_command: Some(forward),
                        forward_speed: Some(2.0),
                        sidestep_command: None,
                        sidestep_speed: None,
                        turn_command: None,
                        turn_speed: None,
                        actions: Vec::new(),
                    }),
                    sticky_object: None,
                    unhandled: Vec::new(),
                },
            };
            let objects = &app.objects;
            app.world
                .as_mut()
                .expect("a world")
                .dispatch_player_movement(&buf, objects);
        };
        let position = |app: &App<NullShell>| {
            app.world
                .as_ref()
                .and_then(|w| w.character.as_ref())
                .map(crate::character::Character::position)
                .expect("a body")
                .frame
                .origin
        };
        for _ in 0..10 {
            assert!(app.frame(&mut shell));
        }
        server_motion(&mut app, 2, 3);
        for _ in 0..60 {
            assert!(app.frame(&mut shell));
        }
        app.objects.world.magic.casting = true;
        let back = names::action_for_enum_name("MovementBackup").expect("a shipped name");
        app.inject_action(Action::begin(back));
        for _ in 0..5 {
            assert!(app.frame(&mut shell));
        }
        let start = position(&app);
        server_motion(&mut app, 3, 51);
        for _ in 0..14 {
            assert!(app.frame(&mut shell));
        }
        server_motion(&mut app, 4, 3);
        for _ in 0..25 {
            assert!(app.frame(&mut shell));
        }
        let end = position(&app);
        dereth_primitives::num::math::hypotf(end.x - start.x, end.y - start.y)
    }

    /// Behaviour: none (the Horizon interface's own camera, not the retail client's)
    #[test]
    #[cfg_attr(
        not(feature = "retail-dats"),
        ignore = "reads the retail dats: --features retail-dats"
    )]
    fn through_a_cast_the_orbit_camera_moves_the_body_exactly_as_the_game_s_camera_does() {
        // The server's motion for the player during a cast holds the body by the game's rule,
        // under either camera; moving through a long cast's windup comes from that same rule.
        let orbit = backed_up_through_a_cast(true);
        let game = backed_up_through_a_cast(false);
        assert!(
            (orbit - game).abs() < 0.05,
            "the orbit camera {orbit} m, the game's {game} m"
        );
    }

    /// What a body did through a cast under the Horizon camera (`movement`), with the cast key,
    /// S and A pressed and the cast's answers and motions arriving on the frames they did in a
    /// recorded session at 240 frames a second: the player casts, presses the cast key again
    /// every fifth of a second or so (each refused at once as too busy), presses S and A a
    /// little under a second in, goes on pressing the cast key with them held, and lets them go
    /// half a second later, the spell's windup still playing.
    struct SpammedCast {
        /// The heading when S and A were pressed and when they were let go.
        heading: (f32, f32),
        /// How far the body went back and left of the way it faced while they were held.
        back: f32,
        left: f32,
        /// Whether the facing was held on every frame they were.
        locked_throughout: bool,
    }

    fn spammed_cast(movement: crate::orbit::MovementMode) -> SpammedCast {
        let store = Arc::new(dereth_dat::testing::open_store().expect("retail dats"));
        let mut hud = crate::hud::Hud::new();
        hud.load_tables(&store, &dereth_client_model::World::new());
        let table = hud.spell_table.expect("the spell table");
        let spell = *table
            .spells
            .iter()
            .find(|(_, b)| b.bitfield & dereth_client_model::magic::spell_index::SELF_TARGETED != 0)
            .expect("a spell cast on oneself")
            .0;
        let mut app = App::<NullShell>::bring_up_with_store(
            Config {
                headless: true,
                frames: None,
                connect: false,
                sound: false,
                dat_dir: std::path::PathBuf::from("a directory that does not exist"),
                ..Config::default()
            },
            Some(store),
            |_| {
                let mut p = Platform::headless(64, 64);
                p.clock = Box::new(crate::platform::clock::FixedStepClock::new(1.0 / 240.0));
                Ok(p)
            },
            |_, _, _, _| Ok(Box::new(SimPresentation::new(64, 64))),
        )
        .expect("bring-up");
        let mut shell = NullShell;
        app.start_shell(&mut shell).expect("the shell starts");
        app.load_static_scene(SceneConfig {
            landblock: 0xA9B4,
            character: true,
            ..SceneConfig::default()
        })
        .expect("the world loads");
        app.orbit = Some(crate::orbit::OrbitSettings {
            movement,
            ..crate::orbit::OrbitSettings::default()
        });
        app.objects.world.magic.spell_table = Some(Arc::new(table));
        let server_motion = |app: &mut App<NullShell>, stamp: u16, forward: u16| {
            let buf = dereth_protocol::movement::MovementBuffer {
                movement_timestamp: stamp,
                server_control_timestamp: stamp,
                autonomous: false,
                body: dereth_protocol::movement::MovementBody {
                    movement_type: 0,
                    motion_flags: 0,
                    current_style: 73,
                    interpreted: Some(dereth_protocol::movement::InterpretedMotionState {
                        current_style: Some(73),
                        forward_command: Some(forward),
                        forward_speed: Some(2.0),
                        sidestep_command: None,
                        sidestep_speed: None,
                        turn_command: None,
                        turn_speed: None,
                        actions: Vec::new(),
                    }),
                    sticky_object: None,
                    unhandled: Vec::new(),
                },
            };
            let objects = &app.objects;
            app.world
                .as_mut()
                .expect("a world")
                .dispatch_player_movement(&buf, objects);
        };
        let body = |app: &App<NullShell>| {
            app.world
                .as_ref()
                .and_then(|w| w.character.as_ref())
                .map(crate::character::Character::position)
                .expect("a body")
                .frame
        };
        for _ in 0..10 {
            assert!(app.frame(&mut shell));
        }
        server_motion(&mut app, 2, 3);
        for _ in 0..60 {
            assert!(app.frame(&mut shell));
        }
        let back = names::action_for_enum_name("MovementBackup").expect("a shipped name");
        let left = names::action_for_enum_name("MovementTurnLeft").expect("a shipped name");
        // The frames, from the first cast, on which each thing happened.
        enum Event {
            Cast,
            TooBusy,
            Keys(bool),
            Windup,
        }
        use Event::{Cast, Keys, TooBusy, Windup};
        let events = [
            (0, Cast),
            (52, Cast),
            (57, TooBusy),
            (100, Cast),
            (102, TooBusy),
            (148, Cast),
            (150, TooBusy),
            (195, Cast),
            (199, TooBusy),
            (205, Keys(true)),
            (232, Windup),
            (249, Cast),
            (253, TooBusy),
            (299, Cast),
            (301, TooBusy),
            (328, Keys(false)),
        ];
        let mut start = None;
        let mut end = None;
        let mut locked_throughout = true;
        for f in 0..=328 {
            for (_, e) in events.iter().filter(|(at, _)| *at == f) {
                match e {
                    Cast => app.interaction.queue(
                        Vec::new(),
                        vec![dereth_client_contract::view::UiRequest::CastSpell {
                            spell_id: spell,
                        }],
                    ),
                    TooBusy => {
                        app.objects.world.use_done(0x1d);
                    }
                    Keys(down) => {
                        let make = if *down { Action::begin } else { Action::end };
                        app.inject_action(make(back));
                        app.inject_action(make(left));
                    }
                    Windup => server_motion(&mut app, 3, 120),
                }
            }
            if f == 328 {
                end = Some(body(&app));
            }
            assert!(app.frame(&mut shell));
            if f == 205 {
                start = Some(body(&app));
            }
            if (205..328).contains(&f) {
                locked_throughout &= app.orbit_keys.locked();
            }
        }
        let (start, end) = (start.expect("pressed"), end.expect("let go"));
        let h0 = dereth_animation::frame::get_heading(&start);
        let h = h0.to_radians();
        let (sin, cos) = (
            dereth_primitives::num::math::sinf(h),
            dereth_primitives::num::math::cosf(h),
        );
        let (dx, dy) = (end.origin.x - start.origin.x, end.origin.y - start.origin.y);
        SpammedCast {
            heading: (h0, dereth_animation::frame::get_heading(&end)),
            back: -(dx * sin + dy * cos),
            left: -(dx * cos - dy * sin),
            locked_throughout,
        }
    }

    /// Behaviour: none (the Horizon interface's own key handling, not the retail client's)
    #[test]
    #[cfg_attr(
        not(feature = "retail-dats"),
        ignore = "reads the retail dats: --features retail-dats"
    )]
    fn s_and_a_back_up_and_step_left_through_a_cast_however_often_the_cast_key_is_pressed_again() {
        for movement in [
            crate::orbit::MovementMode::Character,
            crate::orbit::MovementMode::Camera,
        ] {
            let r = spammed_cast(movement);
            assert!(
                r.locked_throughout,
                "{movement:?}: the facing is held while the spell is cast"
            );
            assert!(
                ((r.heading.1 - r.heading.0 + 540.0).rem_euclid(360.0) - 180.0).abs() < 0.5,
                "{movement:?}: A steps rather than turning: {:?}",
                r.heading
            );
            assert!(
                r.back > 0.3 && r.left > 0.3,
                "{movement:?}: it backed up and stepped left, the cast key pressed again on the \
                 way: {} back, {} left",
                r.back,
                r.left
            );
        }
    }

    /// The whole application with no device: a headless `App` over a `SimPresentation` loads a
    /// world with a body, and a `MovementForward` injected into its action queue (as a script or
    /// a key would) walks the body forward.
    #[test]
    #[cfg_attr(
        not(feature = "retail-dats"),
        ignore = "reads the retail dats: --features retail-dats"
    )]
    fn movement_forward_walks_a_body_the_app_built_with_no_device() {
        let store = Arc::new(dereth_dat::testing::open_store().unwrap_or_else(|| {
            panic!(
                "the retail dats are this test's oracle and they are not under {}",
                dereth_dat::testing::dat_dir().display()
            )
        }));
        let cfg = Config {
            headless: true,
            frames: None,
            connect: false,
            sound: false,
            dat_dir: std::path::PathBuf::from("a directory that does not exist"),
            ..Config::default()
        };
        let mut app = App::<NullShell>::bring_up_with_store(
            cfg,
            Some(store),
            |_| Ok(Platform::headless(64, 64)),
            |_, _, _, _| Ok(Box::new(SimPresentation::new(64, 64))),
        )
        .expect("bring-up");
        let mut shell = NullShell;
        app.start_shell(&mut shell).expect("the shell starts");
        app.load_static_scene(SceneConfig {
            landblock: 0xA9B4,
            character: true,
            ..SceneConfig::default()
        })
        .expect("the world loads");
        let position = |app: &App<NullShell>| {
            app.world
                .as_ref()
                .and_then(|w| w.character.as_ref())
                .map(crate::character::Character::position)
                .expect("a body")
        };
        for _ in 0..10 {
            assert!(app.frame(&mut shell));
        }
        let start = position(&app);
        let forward = names::action_for_enum_name("MovementForward").expect("a shipped name");
        app.inject_action(Action::begin(forward));
        for _ in 0..60 {
            assert!(app.frame(&mut shell));
        }
        app.inject_action(Action::end(forward));
        assert!(app.frame(&mut shell));
        let end = position(&app);
        let moved = dereth_primitives::num::math::hypotf(
            end.frame.origin.x - start.frame.origin.x,
            end.frame.origin.y - start.frame.origin.y,
        );
        assert!(moved > 2.0, "the body moved {moved} m");
    }
}
