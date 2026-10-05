//! A world with no device: the runtime's `SimPresentation` holding a `WorldState`, driven in the
//! order `App::frame` drives a drawn scene.
//!
//! The simulation a drawn scene runs is this one: loading builds the same world and attaches the
//! same body, `update` is the same physics step, re-centre and object step, streaming builds the
//! blocks the window asks for, and object dispatch is the same dispatch with nothing drawn. A
//! module whose claim is about the simulation and reads no pixel runs here, with no device.

use std::sync::Arc;

use dereth_client_runtime::camera::CameraInput;
use dereth_client_runtime::character::CharacterInput;
use dereth_client_runtime::object_step::ObjectStepStats;
use dereth_client_runtime::objects::ObjectStream;
use dereth_client_runtime::present::{Presentation, SceneMut};
use dereth_client_runtime::scene::SceneConfig;
use dereth_client_runtime::sim_present::SimPresentation;
use dereth_client_runtime::world_state::WorldState;
use dereth_dat::RetailDatStore;
use dereth_primitives::LocalTime;

/// The back buffer a device-free world reports. Nothing is drawn into it.
const SIZE: (u32, u32) = (800, 600);

/// A loaded device-free world.
pub struct SimWorld {
    store: Arc<RetailDatStore>,
    present: SimPresentation,
    world: Option<WorldState>,
}

impl SimWorld {
    /// The world `cfg` names, with the body when `cfg.character` asks for one, or a failed test.
    pub fn load(store: &Arc<RetailDatStore>, cfg: SceneConfig) -> Self {
        let mut present = SimPresentation::new(SIZE.0, SIZE.1);
        let mut world = None;
        present
            .load_world(store, cfg, &mut world)
            .expect("the device-free world loads");
        assert!(world.is_some(), "loading built no world state");
        Self {
            store: Arc::clone(store),
            present,
            world,
        }
    }

    /// What the object step counted over the run: the half object dispatch counted and the half the
    /// simulation step counted, summed as a drawn scene's statistics sum them.
    pub fn object_steps(&self) -> ObjectStepStats {
        let (d, f) = (self.present.objects.steps, self.present.steps.steps);
        ObjectStepStats {
            remote_move_tos_failed: d.remote_move_tos_failed + f.remote_move_tos_failed,
            remote_last_move_to_error: if f.remote_move_tos_failed != 0 {
                f.remote_last_move_to_error
            } else {
                d.remote_last_move_to_error
            },
            remote_target_updates: d.remote_target_updates + f.remote_target_updates,
            remote_sticks_pulled: d.remote_sticks_pulled + f.remote_sticks_pulled,
            remote_sticks_applied: d.remote_sticks_applied + f.remote_sticks_applied,
            remote_sticks_unresolved: d.remote_sticks_unresolved + f.remote_sticks_unresolved,
            remote_move_tos_performed: d.remote_move_tos_performed + f.remote_move_tos_performed,
        }
    }

    /// The world state.
    pub fn ws(&self) -> &WorldState {
        self.world.as_ref().expect("a loaded world")
    }

    /// The world state, writable.
    pub fn ws_mut(&mut self) -> &mut WorldState {
        self.world.as_mut().expect("a loaded world")
    }

    /// The scene the frame writes through.
    pub fn scene_mut(&mut self) -> Box<dyn SceneMut + '_> {
        self.present
            .scene_mut(self.world.as_mut())
            .expect("a loaded world has a scene")
    }

    /// The simulation step: physics, the window's re-centre, and the objects.
    pub fn update(
        &mut self,
        input: CameraInput,
        character: CharacterInput,
        now: LocalTime,
        dt: f32,
    ) {
        self.scene_mut().update(input, character, now, dt);
    }

    /// The swept camera's step, which follows the simulation step in `App::frame`.
    pub fn update_viewer(&mut self, input: CameraInput, now: LocalTime, dt: f64) {
        let mut scene = self.scene_mut();
        dereth_client_runtime::camera::update_viewer(&mut *scene, input, now, dt);
    }

    /// Build the blocks the window has queued.
    pub fn stream(&mut self) {
        self.present
            .stream_world(&self.store, self.world.as_mut())
            .expect("the streamed blocks build");
    }

    /// Object dispatch: the stream's pending creates, removals, positions and movement.
    ///
    /// # Errors
    /// What the world's dispatch reports.
    pub fn sync_objects(
        &mut self,
        stream: &mut ObjectStream,
    ) -> Result<(), dereth_world_data::landblock::WorldError> {
        self.present
            .sync_objects(&self.store, stream, self.world.as_mut())
    }
}
