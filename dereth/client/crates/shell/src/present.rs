//! The presentation, as this executable draws through it: the runtime's device seam
//! (`Presentation`, `NullPresentation`, `Scene`) and what the UI adds to it
//! (`ClientPresentation`): the overlay the UI draws over the world, its textures, the movie
//! frame, the target reticule's projection and the four preview spaces.
//!
//! Backend-neutral by construction: no method, type or argument here names a graphics API, and no
//! backend type appears in a signature. The device implementation is `crate::gpu::Renderer`,
//! behind the device features; `NullPresentation` is the headless one, which performs the device
//! steps as **counted no-ops**, so a headless `App` still records all fourteen `FrameStep`s in
//! order and a test can assert that the step it cares about was asked for.

use std::sync::Arc;

use dereth_dat::RetailDatStore;
use dereth_primitives::{DataId, ObjectId, Viewport};

pub use dereth_client_runtime::present::{
    NullPresentation, NullPresentationCounts, PresentError, Presentation, Scene, SceneCensus,
    SceneMut,
};
/// The world state the application owns; every world method is handed it.
pub use dereth_client_runtime::world_state::WorldState;

/// Which preview space: the scene's, which holds the spaces (`crate::gpu` re-exports it too).
pub use dereth_scene::gpu::PreviewId;

/// What `Renderer::release_ui_textures` did, so the release can be asserted on rather than
/// assumed. `unknown` is a double release and must stay zero.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct UiReleaseReport {
    pub freed: u32,
    pub still_linked: u32,
    pub unknown: u32,
}

/// A device error as the application reports it: the device's own message, unchanged.
#[must_use]
pub fn present_error(e: dereth_render::RenderError) -> PresentError {
    PresentError(e.to_string())
}

/// What the UI adds to the device: the overlay, its textures, the movie frame, the target
/// reticule's projection and the four preview spaces.
pub trait ClientPresentation: Presentation {
    /// The frame's 2D half, the UI draw list `PresentFrame` blits over the world.
    ///
    /// # Errors
    /// Whatever the device answers.
    fn draw_ui(&mut self, cmds: &[dereth_ui::UiDrawCmd]) -> Result<(), PresentError>;
    /// Upload whatever this draw list newly references, outside the frame bracket.
    fn prepare_ui(&mut self, store: &RetailDatStore, cmds: &[dereth_ui::UiDrawCmd]);
    fn release_ui_textures(&mut self) -> UiReleaseReport;
    fn set_movie_frame(&mut self, id: DataId, texture: &dereth_primitives::TextureData);
    /// Where a tracked object projects on screen, for the target reticule.
    fn target_projection(
        &self,
        id: ObjectId,
        world: Option<&WorldState>,
    ) -> Option<dereth_ui_screens::hud::target::Projection>;

    // ---------------------------------------------------------------------------------------
    // the four preview spaces
    // ---------------------------------------------------------------------------------------

    /// Build the space if it does not exist. `true` when this call created it.
    fn preview_ensure(
        &mut self,
        id: PreviewId,
        assets: &Arc<crate::anim_assets::DatAnimAssets>,
    ) -> bool;
    fn preview_set_light(
        &mut self,
        id: PreviewId,
        light: dereth_world_render::lighting::LightType,
        intensity: f32,
        direction: dereth_primitives::Vec3,
    );
    fn preview_use_sharp_mode(&mut self, id: PreviewId);
    fn preview_use_world_fov(&mut self, id: PreviewId);
    fn preview_set_camera_position(&mut self, id: PreviewId, position: dereth_primitives::Vec3);
    fn preview_set_camera_direction(&mut self, id: PreviewId, direction: dereth_primitives::Vec3);
    fn preview_set_camera_direction_degrees(
        &mut self,
        id: PreviewId,
        degrees: dereth_primitives::Vec3,
    );
    fn preview_remove_all_objects(&mut self, id: PreviewId);
    /// # Errors
    /// Whatever the device answers while baking the object's meshes.
    fn preview_add_object(
        &mut self,
        id: PreviewId,
        store: &RetailDatStore,
        setup: DataId,
    ) -> Result<Option<usize>, PresentError>;
    /// # Errors
    /// As `Presentation::preview_add_object`.
    fn preview_add_object_dressed(
        &mut self,
        id: PreviewId,
        store: &RetailDatStore,
        setup: DataId,
        objdesc: Option<&dereth_animation::parts::ObjDesc>,
    ) -> Result<Option<usize>, PresentError>;
    fn preview_set_heading(&mut self, id: PreviewId, index: usize, degrees: f32);
    fn preview_set_sequence_animation(
        &mut self,
        id: PreviewId,
        index: usize,
        animation: DataId,
        clear: bool,
        low_frame: i32,
        framerate: f32,
    ) -> bool;
    fn preview_clear_sequence_anims(&mut self, id: PreviewId, index: usize);
    fn preview_has_anims(&self, id: PreviewId, index: usize) -> bool;
    fn preview_use_time(&mut self, id: PreviewId, dt: f64);
    /// The current frame number of preview object `index`.
    fn preview_curr_frame_number(&self, id: PreviewId, index: usize) -> Option<u32>;
    /// The preview object's bounding box, which the identify portrait frames its camera from.
    fn preview_object_bounding_box(
        &self,
        id: PreviewId,
        index: usize,
        store: &RetailDatStore,
    ) -> Option<dereth_physics::geom::BBox>;
    /// The doll's live part array, for the selection blink.
    fn preview_part_array_mut(
        &mut self,
        id: PreviewId,
        index: usize,
    ) -> Option<&mut dereth_animation::parts::PartArray>;
    /// draw this space into that element's rectangle this frame.
    fn preview_queue(&mut self, id: PreviewId, who: dereth_ui::ElemHandle, rect: Viewport);
}

impl ClientPresentation for NullPresentation {
    fn draw_ui(&mut self, _cmds: &[dereth_ui::UiDrawCmd]) -> Result<(), PresentError> {
        self.counts_mut().draw_ui += 1;
        Ok(())
    }
    fn prepare_ui(&mut self, _store: &RetailDatStore, _cmds: &[dereth_ui::UiDrawCmd]) {
        self.counts_mut().prepare_ui += 1;
    }
    fn release_ui_textures(&mut self) -> UiReleaseReport {
        self.counts_mut().release_ui_textures += 1;
        UiReleaseReport::default()
    }
    fn set_movie_frame(&mut self, _id: DataId, _texture: &dereth_primitives::TextureData) {
        self.counts_mut().set_movie_frame += 1;
    }
    fn target_projection(
        &self,
        _id: ObjectId,
        _world: Option<&WorldState>,
    ) -> Option<dereth_ui_screens::hud::target::Projection> {
        None
    }

    fn preview_ensure(
        &mut self,
        _id: PreviewId,
        _assets: &Arc<crate::anim_assets::DatAnimAssets>,
    ) -> bool {
        self.counts_mut().preview_calls += 1;
        false
    }
    fn preview_set_light(
        &mut self,
        _id: PreviewId,
        _light: dereth_world_render::lighting::LightType,
        _intensity: f32,
        _direction: dereth_primitives::Vec3,
    ) {
        self.counts_mut().preview_calls += 1;
    }
    fn preview_use_sharp_mode(&mut self, _id: PreviewId) {
        self.counts_mut().preview_calls += 1;
    }
    fn preview_use_world_fov(&mut self, _id: PreviewId) {
        self.counts_mut().preview_calls += 1;
    }
    fn preview_set_camera_position(&mut self, _id: PreviewId, _position: dereth_primitives::Vec3) {
        self.counts_mut().preview_calls += 1;
    }
    fn preview_set_camera_direction(
        &mut self,
        _id: PreviewId,
        _direction: dereth_primitives::Vec3,
    ) {
        self.counts_mut().preview_calls += 1;
    }
    fn preview_set_camera_direction_degrees(
        &mut self,
        _id: PreviewId,
        _degrees: dereth_primitives::Vec3,
    ) {
        self.counts_mut().preview_calls += 1;
    }
    fn preview_remove_all_objects(&mut self, _id: PreviewId) {
        self.counts_mut().preview_calls += 1;
    }
    fn preview_add_object(
        &mut self,
        _id: PreviewId,
        _store: &RetailDatStore,
        _setup: DataId,
    ) -> Result<Option<usize>, PresentError> {
        self.counts_mut().preview_calls += 1;
        Ok(None)
    }
    fn preview_add_object_dressed(
        &mut self,
        _id: PreviewId,
        _store: &RetailDatStore,
        _setup: DataId,
        _objdesc: Option<&dereth_animation::parts::ObjDesc>,
    ) -> Result<Option<usize>, PresentError> {
        self.counts_mut().preview_calls += 1;
        Ok(None)
    }
    fn preview_set_heading(&mut self, _id: PreviewId, _index: usize, _degrees: f32) {
        self.counts_mut().preview_calls += 1;
    }
    fn preview_set_sequence_animation(
        &mut self,
        _id: PreviewId,
        _index: usize,
        _animation: DataId,
        _clear: bool,
        _low_frame: i32,
        _framerate: f32,
    ) -> bool {
        self.counts_mut().preview_calls += 1;
        false
    }
    fn preview_clear_sequence_anims(&mut self, _id: PreviewId, _index: usize) {
        self.counts_mut().preview_calls += 1;
    }
    fn preview_has_anims(&self, _id: PreviewId, _index: usize) -> bool {
        false
    }
    fn preview_use_time(&mut self, _id: PreviewId, _dt: f64) {
        self.counts_mut().preview_calls += 1;
    }
    fn preview_curr_frame_number(&self, _id: PreviewId, _index: usize) -> Option<u32> {
        None
    }
    fn preview_object_bounding_box(
        &self,
        _id: PreviewId,
        _index: usize,
        _store: &RetailDatStore,
    ) -> Option<dereth_physics::geom::BBox> {
        None
    }
    fn preview_part_array_mut(
        &mut self,
        _id: PreviewId,
        _index: usize,
    ) -> Option<&mut dereth_animation::parts::PartArray> {
        None
    }
    fn preview_queue(&mut self, _id: PreviewId, _who: dereth_ui::ElemHandle, _rect: Viewport) {
        self.counts_mut().preview_calls += 1;
    }
}

#[cfg(test)]
mod tests {
    use super::{present_error, PresentError};
    use dereth_render::{PixelFormatId, RenderError};

    /// A present error reads exactly as the device error it came from.
    #[test]
    fn a_present_error_reads_exactly_as_the_device_error_it_came_from() {
        let errors = [
            RenderError::BadDimensions {
                width: 0,
                height: 600,
                reason: "zero width",
            },
            RenderError::ShortSourceData {
                format: PixelFormatId::A8R8G8B8,
                width: 4,
                height: 4,
                expected: 64,
                actual: 3,
            },
            RenderError::UnsupportedFormat(PixelFormatId::Unknown),
        ];
        for device in errors {
            let text = device.to_string();
            let e = present_error(device);
            assert_eq!(e.to_string(), text);
            assert_eq!(format!("{e}"), text);
            assert_eq!(e, PresentError(text));
        }
    }

    /// A device error converts on the way out of a presentation method, and the result is an
    /// ordinary error with nothing underneath it.
    #[test]
    fn a_device_error_converts_on_the_way_out() {
        fn present() -> Result<(), PresentError> {
            let device: Result<(), RenderError> = Err(RenderError::BadDimensions {
                width: 4096,
                height: 1,
                reason: "too wide",
            });
            device.map_err(present_error)?;
            Ok(())
        }
        let e = present().unwrap_err();
        assert_eq!(e.to_string(), "bad dimensions 4096x1: too wide");
        let as_error: &dyn std::error::Error = &e;
        assert!(as_error.source().is_none());
    }
}
