//! The remaster's look: what each layer says of retail's light, given as a look file's text, kept
//! while the remaster is off and handed to it whenever it is on. Retail Look never reads it.

use dereth_remaster::look::{Layer, LookError, LookToml};

/// Where a layer of the remaster's look comes from.
pub use dereth_remaster::look::Layer as RemasterLookLayer;
/// Why a file is not a look.
pub use dereth_remaster::look::LookError as RemasterLookError;
/// Every layer's look.
pub use dereth_remaster::look::LookStack as RemasterLooks;
use dereth_render::device::Gpu;

use super::remaster_view::remaster;
use super::SceneDraw;

impl SceneDraw {
    /// Set one layer of the remaster's look from a look file's text, or take the layer away with
    /// `None`; the remaster lights its next frame by it when it is on, and from when it is turned
    /// on otherwise. Returns whether the look changed.
    ///
    /// # Errors
    /// [`LookError`] when the text is not a look; the layer is then as it was.
    pub fn set_remaster_look(
        &mut self,
        layer: Layer,
        text: Option<&str>,
        gpu: &mut Gpu,
    ) -> Result<bool, LookError> {
        let look = text.map(LookToml::parse).transpose()?;
        let changed = self.remaster_view.get_mut().looks.set(layer, look.clone());
        if changed {
            if let Some(r) = remaster(gpu) {
                r.renderer().set_look(layer, look);
            }
        }
        Ok(changed)
    }

    /// The remaster's looks, every layer's.
    #[must_use]
    pub fn remaster_looks(&self) -> RemasterLooks {
        self.remaster_view.borrow().looks.clone()
    }
}

/// How many frames the player's look file is left between two reads.
const LOOK_EVERY: u32 = 15;

/// The player's look file, read every [`LOOK_EVERY`] frames while the remaster is on, and what it
/// held when it was last read.
#[derive(Debug, Default)]
pub(crate) struct LookTomlWatch {
    path: Option<std::path::PathBuf>,
    /// Frames since it was last read; it is read on the first poll.
    frames: Option<u32>,
    /// What it held when it was last read: `None` before the first read, `Some(None)` when there
    /// was no file.
    last: Option<Option<String>>,
}

impl LookTomlWatch {
    /// Watch `path` instead, from its next poll; `None` watches nothing.
    pub(crate) fn set(&mut self, path: Option<std::path::PathBuf>) {
        if path != self.path {
            *self = Self {
                path,
                ..Self::default()
            };
        }
    }

    /// One frame: the file's text when this frame read it and it held something other than when
    /// it was last read (`Some(None)` when it is gone); `None` otherwise.
    pub(crate) fn poll(&mut self) -> Option<Option<String>> {
        let path = self.path.as_ref()?;
        let frames = self.frames.map_or(LOOK_EVERY, |f| f + 1);
        if frames < LOOK_EVERY {
            self.frames = Some(frames);
            return None;
        }
        self.frames = Some(0);
        let text = std::fs::read_to_string(path).ok();
        if self.last.as_ref() == Some(&text) {
            return None;
        }
        self.last = Some(text.clone());
        Some(text)
    }
}

impl SceneDraw {
    /// The player's look file, read by the remaster while it is on and again whenever what it
    /// holds changes; `None` reads none and leaves the player's look as it is.
    pub fn set_remaster_look_file(&mut self, path: Option<std::path::PathBuf>) {
        self.remaster_view.get_mut().look_file.set(path);
    }

    /// One frame of the player's look file, while the remaster is on: when it changed, its look
    /// in place of the player's, or none when it is gone. A file that is not a look leaves the
    /// player's look as it was, and says why once.
    pub(super) fn poll_remaster_look(&mut self, gpu: &mut Gpu) {
        if !self.remaster_on() {
            return;
        }
        let Some(text) = self.remaster_view.get_mut().look_file.poll() else {
            return;
        };
        if let Err(e) = self.set_remaster_look(Layer::Player, text.as_deref(), gpu) {
            tracing::warn!(target: "dereth::render", "the look file is not read: {e}");
        }
    }
}

#[cfg(test)]
mod tests {
    //! Behaviour: none (experimental)
    use super::*;

    /// **The player's look file is read on the first poll, then every few frames, and handed on
    /// only when what it holds changed** (its text, or that it is gone).
    #[test]
    fn the_look_file_is_read_every_few_frames_and_handed_on_when_it_changes() {
        let dir = std::env::temp_dir().join(format!("dereth-look-watch-{}", std::process::id()));
        std::fs::create_dir_all(&dir).expect("a folder");
        let file = dir.join("look.toml");
        std::fs::remove_file(&file).ok();
        let mut w = LookTomlWatch::default();
        assert_eq!(w.poll(), None, "nothing is watched");
        w.set(Some(file.clone()));
        assert_eq!(w.poll(), Some(None), "read at once: no file");
        std::fs::write(&file, "tint = 0\n").expect("written");
        for _ in 1..LOOK_EVERY {
            assert_eq!(w.poll(), None, "between reads");
        }
        assert_eq!(w.poll(), Some(Some("tint = 0\n".into())));
        for _ in 0..LOOK_EVERY {
            assert_eq!(w.poll(), None, "read again, unchanged");
        }
        std::fs::remove_file(&file).expect("removed");
        let mut seen = None;
        for _ in 0..LOOK_EVERY {
            seen = seen.or(w.poll());
        }
        assert_eq!(seen, Some(None), "gone");
        std::fs::remove_dir_all(&dir).ok();
    }
}
