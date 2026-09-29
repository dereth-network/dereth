//! The renderer interface: what world drawing may ask of a device.
//!
//! [`RenderBackend`] takes decoded meshes and textures ([`crate::MeshData`],
//! [`crate::TextureData`]), hands back opaque handles, and accepts [`DrawBatch`]es in submission
//! order. The code that decides *what* is drawn each frame holds a `&mut dyn RenderBackend` and so
//! never names a device, a window or a graphics API; a recording mock stands in for the device in
//! tests.

use crate::asset::{MeshData, MeshHandle, TextureData, TextureHandle};
use crate::space::Frame;

/// One draw submission. Deliberately coarse: batching policy belongs to the renderer.
#[derive(Debug, Clone)]
pub struct DrawBatch {
    pub mesh: MeshHandle,
    pub texture: Option<TextureHandle>,
    /// World transform for this batch.
    pub transform: Frame,
    /// Index range within the mesh.
    pub range: std::ops::Range<u32>,
}

/// What world drawing is allowed to ask of the renderer.
pub trait RenderBackend {
    fn upload_mesh(&mut self, m: &MeshData) -> MeshHandle;
    fn upload_texture(&mut self, t: &TextureData) -> TextureHandle;
    fn draw(&mut self, batch: &DrawBatch);
}
