//! A `RenderBackend` that records instead of drawing.
//!
//! The renderer ships `RecordingBackend`, but this crate depends on the renderer through the
//! **trait only**, so it carries its own recorder for its own tests.
//! It is deliberately
//! the same shape — a `Vec` in submission order, no sorting, no culling — so a test written against
//! one reads the same against the other.
//!
//! Available outside `cfg(test)` so a downstream integration test can use it too.

use dereth_primitives::{
    DrawBatch, MeshData, MeshHandle, RenderBackend, TextureData, TextureHandle,
};

/// Records every call in submission order. **Never sorts, culls or reorders** — submission order is
/// the observable.
#[derive(Debug, Default)]
pub struct Recorder {
    /// `(handle, vertex bytes, index count, stride)` per `upload_mesh`.
    pub uploads: Vec<(MeshHandle, usize, usize, u32)>,
    /// `(handle, width, height, level count)` per `upload_texture`.
    pub textures: Vec<(TextureHandle, u32, u32, usize)>,
    /// Every `draw()`, verbatim, in order.
    pub draws: Vec<DrawBatch>,
    next_mesh: u32,
    next_texture: u32,
}

impl Recorder {
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// The mesh handle of every recorded draw, in submission order.
    #[must_use]
    pub fn draw_order(&self) -> Vec<MeshHandle> {
        self.draws.iter().map(|b| b.mesh).collect()
    }

    pub fn clear(&mut self) {
        self.uploads.clear();
        self.textures.clear();
        self.draws.clear();
    }
}

impl RenderBackend for Recorder {
    fn upload_mesh(&mut self, m: &MeshData) -> MeshHandle {
        let h = MeshHandle(self.next_mesh);
        self.next_mesh += 1;
        self.uploads
            .push((h, m.vertices.len(), m.indices.len(), m.stride));
        h
    }

    fn upload_texture(&mut self, t: &TextureData) -> TextureHandle {
        let h = TextureHandle(self.next_texture);
        self.next_texture += 1;
        self.textures.push((h, t.width, t.height, t.levels.len()));
        h
    }

    fn draw(&mut self, batch: &DrawBatch) {
        self.draws.push(batch.clone());
    }
}

#[cfg(test)]
mod tests {
    // Index arithmetic in test fixtures, bounded by the loops that build them.
    #![allow(clippy::cast_possible_truncation)]

    use super::*;
    use dereth_primitives::Frame;

    /// Draws record in submission order.
    #[test]
    fn draws_record_in_submission_order() {
        let mut r = Recorder::new();
        for i in 0..100u32 {
            let m = r.upload_mesh(&MeshData::default());
            r.draw(&DrawBatch {
                mesh: m,
                texture: None,
                transform: Frame::default(),
                range: i..i + 3,
            });
        }
        assert_eq!(r.draws.len(), 100);
        for (i, b) in r.draws.iter().enumerate() {
            // LINT-OK: index arithmetic over a 100-element loop.
            assert_eq!(b.mesh, MeshHandle(i as u32));
            assert_eq!(b.range.start, i as u32);
        }
    }
}
