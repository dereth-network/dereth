//! The presentation, as this executable draws through it: the runtime's device seam
//! (`Presentation`, `NullPresentation`, `Scene`) and what the UI adds to it
//! (`ClientPresentation`): the retail draw list, its textures, the movie frame, the target
//! reticule's projection and where the preview spaces are drawn.
//!
//! Backend-neutral by construction: no method, type or argument here names a graphics API, and no
//! backend type appears in a signature. The device implementation is `crate::gpu::Renderer`,
//! behind the device features; `NullPresentation` is the headless one, which performs the device
//! steps as **counted no-ops**, so a headless `App` still records all fourteen `FrameStep`s in
//! order and a test can assert that the step it cares about was asked for.

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

/// What the modern UI adds to the device: its draw list, its textures, the movie frame, the
/// target reticule's projection and where its preview spaces are drawn.
pub trait ClientPresentation: Presentation {
    /// The frame's 2D half, the UI draw list `PresentFrame` blits over the world.
    ///
    /// # Errors
    /// Whatever the device answers.
    fn draw_ui(&mut self, cmds: &[dereth_ui::UiDrawCmd]) -> Result<(), PresentError>;
    /// Upload whatever this draw list newly references, outside the frame bracket: its interface
    /// pictures from `interface`, the interface's own files, and the pictures the world names
    /// ([`dereth_ui::ImageSource::World`]) from `world`.
    fn prepare_ui(
        &mut self,
        interface: &RetailDatStore,
        world: &RetailDatStore,
        cmds: &[dereth_ui::UiDrawCmd],
    );
    fn release_ui_textures(&mut self) -> UiReleaseReport;
    fn set_movie_frame(&mut self, id: DataId, texture: &dereth_primitives::TextureData);
    /// Where a tracked object projects on screen, for the target reticule.
    fn target_projection(
        &self,
        id: ObjectId,
        world: Option<&WorldState>,
    ) -> Option<dereth_ui_screens::hud::target::Projection>;

    /// Where a tracked object's own origin stands on screen, at the height of its selection
    /// sphere's centre: the point to centre a name or a marker on.
    fn target_origin(&self, _id: ObjectId, _world: Option<&WorldState>) -> Option<(i32, i32)> {
        None
    }

    /// Where the top of a tracked object's body, as the world is drawn this frame, stands on
    /// screen: the point a name over it stands on.
    fn target_top(&self, _id: ObjectId, _world: Option<&WorldState>) -> Option<(f32, f32)> {
        None
    }

    /// The objects the last world draw could see (those a click could pick): `None` when no
    /// world has been drawn.
    fn drawn_objects(&self) -> Option<std::collections::BTreeSet<ObjectId>> {
        None
    }

    /// Draw preview space `id` into that element's rectangle this frame. The spaces themselves
    /// are the presentation's own (`Presentation::preview_ensure` and the rest).
    fn preview_queue(&mut self, id: PreviewId, who: dereth_ui::ElemHandle, rect: Viewport);

    /// Draw preview space `id` into `rect` this frame over the pictures of the elements in
    /// `subtree` (an element and its descendants) and under their text: the space is drawn once
    /// the last of them has drawn its picture, and their text after it. A presentation that cannot
    /// split the two draws the space at the first of them.
    fn preview_queue_under_text(
        &mut self,
        id: PreviewId,
        subtree: Vec<dereth_ui::ElemHandle>,
        rect: Viewport,
    ) {
        if let Some(&who) = subtree.first() {
            self.preview_queue(id, who, rect);
        }
    }
}

macro_rules! counted_ui {
    ($presentation:ty) => {
        impl ClientPresentation for $presentation {
            fn draw_ui(&mut self, _cmds: &[dereth_ui::UiDrawCmd]) -> Result<(), PresentError> {
                self.counts_mut().draw_ui += 1;
                Ok(())
            }
            fn prepare_ui(
                &mut self,
                _interface: &RetailDatStore,
                _world: &RetailDatStore,
                _cmds: &[dereth_ui::UiDrawCmd],
            ) {
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

            fn preview_queue(
                &mut self,
                _id: PreviewId,
                _who: dereth_ui::ElemHandle,
                _rect: Viewport,
            ) {
                self.counts_mut().preview_calls += 1;
            }
        }
    };
}

counted_ui!(NullPresentation);
counted_ui!(dereth_client_runtime::sim_present::SimPresentation);

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
