//! The interface's draw list and the overlay batches it becomes: the kit's
//! ([`dereth_ui_kit::draw`]), uploading into the shell's overlay under this interface's own key
//! range, every quad on whole pixels.

use auberean_shell::interface::{Overlay as ShellOverlay, OverlayTexture, PreviewSpace};
use dereth_primitives::TextureData;

pub use dereth_ui_kit::draw::{with_alpha, Argb, DrawList, Quad, Rect, TextureSink, WHITE};

use crate::art::Art;

/// The keys this interface's pictures are uploaded under: a range of their own, so they never
/// meet another interface's.
const KEY_BASE: u64 = 0x4148_5A00_0000_0000;

/// The shell's overlay as the kit's texture sink: what is uploaded is kept there from frame to
/// frame, and what is let go of leaves it.
struct Sink<'a> {
    overlay: &'a mut ShellOverlay,
    size: (u32, u32),
}

impl TextureSink for Sink<'_> {
    fn size(&self) -> (u32, u32) {
        self.size
    }
    fn upload(&mut self, key: OverlayTexture, data: &TextureData) -> bool {
        self.overlay.upload(key, data)
    }
    fn release(&mut self, key: OverlayTexture) {
        self.overlay.release(key);
    }
}

/// Every picture uploaded so far, and this frame's batches.
#[derive(Debug)]
pub struct Overlay(dereth_ui_kit::draw::Overlay);

impl Default for Overlay {
    fn default() -> Self {
        Self(dereth_ui_kit::draw::Overlay::new(KEY_BASE))
    }
}

impl Overlay {
    /// `list` as this frame's batches in `overlay`, over a window of `size` pixels, uploading
    /// every picture it names that is not uploaded yet. `spaces` mark where a viewport of the
    /// overlay's scenes stands among the batches: each over the first so many quads of the list
    /// (those are drawn under the scenes) and under the rest.
    pub fn compose(
        &mut self,
        overlay: &mut ShellOverlay,
        size: (u32, u32),
        art: &Art,
        list: &DrawList,
        spaces: &[(PreviewSpace, Rect, usize)],
    ) {
        self.0
            .compose(&mut Sink { overlay, size }, art, list, None, spaces);
        overlay.items.extend_from_slice(self.0.items());
    }

    /// Let go of every picture uploaded: the next frame drawn uploads what it draws anew.
    pub fn release(&mut self, overlay: &mut ShellOverlay) {
        self.0.release(&mut Sink {
            overlay,
            size: (0, 0),
        });
    }
}
