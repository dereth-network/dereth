//! The sound device: the default output endpoint through `cpal` (`dereth-audio-out`), which the
//! mixer in [`dereth_client_runtime::audio`] plays through. Everything else is the runtime's, at
//! its old path.

pub use dereth_audio_out::CpalOutput;
pub use dereth_client_runtime::audio::*;

/// Install [`CpalOutput`] as the mixer's default endpoint. Idempotent.
pub fn install_default_output() {
    dereth_client_runtime::audio::install_default_output(|| Box::new(CpalOutput::default()));
}
