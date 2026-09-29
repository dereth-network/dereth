//! `RecordingBackend` — a [`RenderBackend`] that records instead of drawing.
//!
//! **`RecordingBackend` is a deliverable, not a convenience.** It is what keeps the edge between
//! the renderer and the world-drawing code trait-only: the world code can be written and tested
//! without a device.
//!
//! It is also how every other module in this crate is tested without a GPU, and it enforces the one
//! rule the real renderer must obey: **`draw()` never sorts, culls or reorders**. The recording is a `Vec` in submission order, and
//! [`RecordingBackend::draw_order`] is the assertion helper for that.

use dereth_primitives::{
    DrawBatch, MeshData, MeshHandle, RenderBackend, TextureData, TextureHandle,
};

use crate::camera::ViewParams;
use crate::pso::PipelineKey;
use crate::DrawConstants;

/// One recorded call, in submission order.
#[derive(Debug, Clone)]
pub enum RecordedCall {
    /// Recorded when a frame begins.
    BeginFrame(Box<ViewParams>),
    /// Recorded when a frame ends.
    PresentFrame,
    UploadMesh {
        handle: MeshHandle,
        vertex_bytes: usize,
        index_count: usize,
        stride: u32,
    },
    UploadTexture {
        handle: TextureHandle,
        width: u32,
        height: u32,
        format: dereth_primitives::TextureFormat,
        levels: usize,
    },
    /// A pipeline-state change. The real renderer folds this into the draw; recording it separately
    /// is what lets a caller assert that the state it asked for is the state it got.
    SetPipeline(PipelineKey, DrawConstants),
    /// One `draw()`, recorded verbatim.
    Draw(DrawBatch),
}

/// Records every `DrawBatch` and every state change without a GPU.
#[derive(Debug, Default)]
pub struct RecordingBackend {
    pub calls: Vec<RecordedCall>,
    next_mesh: u32,
    next_texture: u32,
    /// The pipeline the next `draw()` will be recorded under, if the caller set one.
    current: Option<(PipelineKey, DrawConstants)>,
}

impl RecordingBackend {
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// Forget everything recorded so far, keeping the handle counters so handles stay unique.
    pub fn clear(&mut self) {
        self.calls.clear();
    }

    /// Begin a frame: clear with flags 7, color `(0, 0, 0, 1)`, depth `1.0`, and open the list.
    pub fn begin_frame(&mut self, view: &ViewParams) {
        self.calls.push(RecordedCall::BeginFrame(Box::new(*view)));
    }

    /// Recorded at the end of a frame.
    pub fn end_frame(&mut self) {
        self.calls.push(RecordedCall::PresentFrame);
    }

    /// Record the pipeline state a subsequent `draw()` runs under.
    pub fn set_pipeline(&mut self, key: PipelineKey, constants: DrawConstants) {
        self.current = Some((key, constants));
        self.calls.push(RecordedCall::SetPipeline(key, constants));
    }

    /// The pipeline currently selected, if any.
    #[must_use]
    pub fn current_pipeline(&self) -> Option<(PipelineKey, DrawConstants)> {
        self.current
    }

    /// Every recorded `draw()`, in submission order. The whole point of the backend: a test that
    /// wants to prove nothing was reordered compares this against what it submitted.
    #[must_use]
    pub fn draws(&self) -> Vec<&DrawBatch> {
        self.calls
            .iter()
            .filter_map(|c| match c {
                RecordedCall::Draw(b) => Some(b),
                _ => None,
            })
            .collect()
    }

    /// The mesh handle of every recorded draw, in submission order.
    #[must_use]
    pub fn draw_order(&self) -> Vec<MeshHandle> {
        self.draws().into_iter().map(|b| b.mesh).collect()
    }

    /// Every `(key, constants)` pair a draw ran under, in submission order. A draw submitted with no
    /// preceding `set_pipeline` contributes `None`.
    #[must_use]
    pub fn draw_pipelines(&self) -> Vec<Option<(PipelineKey, DrawConstants)>> {
        let mut cur = None;
        let mut out = Vec::new();
        for call in &self.calls {
            match call {
                RecordedCall::SetPipeline(k, c) => cur = Some((*k, *c)),
                RecordedCall::Draw(_) => out.push(cur),
                _ => {}
            }
        }
        out
    }
}

impl RenderBackend for RecordingBackend {
    fn upload_mesh(&mut self, m: &MeshData) -> MeshHandle {
        let handle = MeshHandle(self.next_mesh);
        self.next_mesh += 1;
        self.calls.push(RecordedCall::UploadMesh {
            handle,
            vertex_bytes: m.vertices.len(),
            index_count: m.indices.len(),
            stride: m.stride,
        });
        handle
    }

    fn upload_texture(&mut self, t: &TextureData) -> TextureHandle {
        let handle = TextureHandle(self.next_texture);
        self.next_texture += 1;
        self.calls.push(RecordedCall::UploadTexture {
            handle,
            width: t.width,
            height: t.height,
            format: t.format,
            levels: t.levels.len(),
        });
        handle
    }

    /// Draws exactly this batch, exactly now, in submission order. Never sorts, never culls.
    fn draw(&mut self, batch: &DrawBatch) {
        self.calls.push(RecordedCall::Draw(batch.clone()));
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::pso::{PipelineKey, SurfaceContext};
    use crate::surface::{surface_type as st, Surface};
    use dereth_primitives::{Frame, TextureFormat};

    fn batch(mesh: MeshHandle, texture: Option<TextureHandle>) -> DrawBatch {
        DrawBatch {
            mesh,
            texture,
            transform: Frame::default(),
            range: 0..3,
        }
    }

    // Oracle: a synthetic scene of 100 batches records in submission order with the expected
    // keys. The backend does not sort, cull or reorder in `draw()`, and contract items 11.7/11.8
    // make submission order the observable.
    #[test]
    fn a_hundred_batches_record_in_submission_order() {
        let mut b = RecordingBackend::new();
        let tex = b.upload_texture(&TextureData {
            width: 4,
            height: 4,
            format: TextureFormat::Bgra8,
            levels: vec![vec![0; 64]],
        });
        let mut submitted = Vec::new();
        for _ in 0..100u32 {
            let mesh = b.upload_mesh(&MeshData {
                vertices: vec![0; 24],
                indices: vec![0, 1, 2],
                stride: 24,
            });
            submitted.push(mesh);
            b.draw(&batch(mesh, Some(tex)));
        }
        assert_eq!(b.draws().len(), 100);
        assert_eq!(b.draw_order(), submitted);
        // Deliberately submit in a "wrong" order: the recording must not repair it.
        b.clear();
        for mesh in submitted.iter().rev() {
            b.draw(&batch(*mesh, None));
        }
        let reversed: Vec<_> = submitted.iter().rev().copied().collect();
        assert_eq!(b.draw_order(), reversed);
    }

    // Oracle: same unit -- "with the expected keys". Each batch is drawn under the pipeline key its
    // surface produces, and the recording must attribute each draw to
    // the state that was live when it was submitted.
    #[test]
    fn each_draw_is_attributed_to_the_pipeline_that_was_live() {
        let mut b = RecordingBackend::new();
        let ctx = SurfaceContext::default();
        let opaque = Surface {
            r#type: st::BASE1_IMAGE,
            ..Surface::default()
        };
        let additive = Surface {
            r#type: st::BASE1_IMAGE | st::ADDITIVE,
            ..Surface::default()
        };
        let (k_opaque, r_opaque) = PipelineKey::from_surface(&opaque, ctx);
        let (k_add, r_add) = PipelineKey::from_surface(&additive, ctx);
        assert_ne!(k_opaque, k_add);

        let mesh = b.upload_mesh(&MeshData::default());
        b.begin_frame(&ViewParams::default());
        b.set_pipeline(
            k_opaque,
            DrawConstants {
                alpha_ref: r_opaque,
                ..DrawConstants::default()
            },
        );
        b.draw(&batch(mesh, None));
        b.draw(&batch(mesh, None));
        b.set_pipeline(
            k_add,
            DrawConstants {
                alpha_ref: r_add,
                ..DrawConstants::default()
            },
        );
        b.draw(&batch(mesh, None));
        b.end_frame();

        let keys: Vec<_> = b
            .draw_pipelines()
            .into_iter()
            .map(|p| p.map(|(k, _)| k))
            .collect();
        assert_eq!(keys, vec![Some(k_opaque), Some(k_opaque), Some(k_add)]);
        // The frame bracket is recorded too, around the uploads that preceded it.
        assert!(matches!(b.calls[0], RecordedCall::UploadMesh { .. }));
        assert!(matches!(b.calls[1], RecordedCall::BeginFrame(_)));
        assert!(matches!(b.calls.last(), Some(RecordedCall::PresentFrame)));
    }

    // Oracle: the renderer seam trait in dereth-primitives -- handles are opaque and must be distinct per
    // upload, because DrawBatch identifies its geometry by handle alone.
    #[test]
    fn handles_are_unique_and_uploads_are_recorded() {
        let mut b = RecordingBackend::new();
        let m0 = b.upload_mesh(&MeshData {
            vertices: vec![1, 2, 3],
            indices: vec![0],
            stride: 3,
        });
        let m1 = b.upload_mesh(&MeshData::default());
        assert_ne!(m0, m1);
        let t0 = b.upload_texture(&TextureData {
            width: 2,
            height: 2,
            format: TextureFormat::Bc1,
            levels: vec![vec![0; 8]],
        });
        assert_eq!(t0, TextureHandle(0));
        match &b.calls[0] {
            RecordedCall::UploadMesh {
                vertex_bytes,
                index_count,
                stride,
                ..
            } => {
                assert_eq!((*vertex_bytes, *index_count, *stride), (3, 1, 3));
            }
            other => panic!("{other:?}"),
        }
        match &b.calls[2] {
            RecordedCall::UploadTexture {
                width,
                height,
                format,
                levels,
                ..
            } => {
                assert_eq!(
                    (*width, *height, *format, *levels),
                    (2, 2, TextureFormat::Bc1, 1)
                );
            }
            other => panic!("{other:?}"),
        }
    }

    // Oracle: the renderer seam's object safety -- the client holds the backend behind `dyn`, so
    // the recording backend has to work through it too.
    #[test]
    fn the_recording_backend_works_through_a_trait_object() {
        let mut b = RecordingBackend::new();
        {
            let dynamic: &mut dyn RenderBackend = &mut b;
            let m = dynamic.upload_mesh(&MeshData::default());
            dynamic.draw(&batch(m, None));
        }
        assert_eq!(b.draws().len(), 1);
    }
}
