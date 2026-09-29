//! The sound device. The mixer and every sound decision are the runtime's, at their old path.
//!
//! A worker has no audio context, so the output here is a pull: the runtime hands the output
//! its fill (the mixer, resampled to the output rate) and the worker calls [`pull`] once a frame
//! for as many sample frames as the page's audio clock has used, and posts them to the page,
//! which plays them through an audio worklet.

pub use dereth_client_runtime::audio::*;

use std::cell::RefCell;

/// The rate the page's audio context is opened at.
pub const OUTPUT_RATE: u32 = 48_000;

/// The channels the page plays: stereo, interleaved.
pub const OUTPUT_CHANNELS: u16 = 2;

type Fill = Box<dyn FnMut(&mut [f32]) + Send>;

thread_local! {
    static FILL: RefCell<Option<Fill>> = const { RefCell::new(None) };
}

/// The page's audio output, as the runtime's output seam sees it.
#[derive(Debug, Default)]
pub struct PageOutput;

impl dereth_client_runtime::platform::audio_out::AudioOutput for PageOutput {
    fn probe(&mut self) -> Result<(u32, u16), String> {
        Ok((OUTPUT_RATE, OUTPUT_CHANNELS))
    }

    fn start(&mut self, fill: Fill) -> Result<Box<dyn std::any::Any>, String> {
        FILL.with(|f| *f.borrow_mut() = Some(fill));
        Ok(Box::new(()))
    }
}

/// Install [`PageOutput`] as the mixer's default endpoint. Idempotent.
pub fn install_default_output() {
    dereth_client_runtime::audio::install_default_output(|| Box::new(PageOutput));
}

/// The next `frames` sample frames of the mix, interleaved stereo; empty before the runtime has
/// started its output.
#[must_use]
pub fn pull(frames: usize) -> Vec<f32> {
    FILL.with(|f| {
        let mut f = f.borrow_mut();
        let Some(fill) = f.as_mut() else {
            return Vec::new();
        };
        let mut out = vec![0.0; frames * usize::from(OUTPUT_CHANNELS)];
        fill(&mut out);
        out
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use dereth_client_runtime::platform::audio_out::AudioOutput;

    /// Before the runtime starts the output there is nothing to pull; once it has, a pull is the
    /// fill's output for that many stereo frames.
    #[test]
    fn a_pull_is_the_started_fill_s_output() {
        assert!(pull(4).is_empty());
        let mut out = PageOutput;
        assert_eq!(out.probe(), Ok((OUTPUT_RATE, OUTPUT_CHANNELS)));
        out.start(Box::new(|block: &mut [f32]| block.fill(0.25)))
            .expect("started");
        assert_eq!(pull(4), vec![0.25; 8]);
    }
}
