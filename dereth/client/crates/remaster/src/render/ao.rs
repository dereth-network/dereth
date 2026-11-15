//! The ambient occlusion: the shared pass (`dereth_gfx_passes::ao`), and the renderer's hold on
//! it.

pub use dereth_gfx_passes::ao::*;

impl super::Renderer {
    /// How the ambient occlusion is found from the next frame on.
    pub fn set_ao(&mut self, settings: AoSettings) {
        self.ao.set_settings(settings);
    }

    /// How the ambient occlusion is found.
    #[must_use]
    pub fn ao_settings(&self) -> AoSettings {
        self.ao.settings()
    }

    /// The last frame's ambient occlusion at the world's size, row by row, 1 open and 0 shut in;
    /// `None` while the pass is off or before it has run. The device waits for its work first.
    pub fn read_ao(&mut self) -> Option<(u32, u32, Vec<f32>)> {
        let (width, height, ao) = self.ao.occlusion()?;
        self.gfx.wait_idle().ok()?;
        let bytes = self.gfx.read_texture(ao, 0).ok()?;
        Some((
            width,
            height,
            bytes
                .as_chunks::<4>()
                .0
                .iter()
                .map(|w| f32::from_le_bytes(*w))
                .collect(),
        ))
    }
}
