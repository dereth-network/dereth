//! The sound device: the default output endpoint through `cpal`, which the mixer in
//! [`dereth_client_runtime::audio`] plays through. Everything else is the runtime's, at its old
//! path.

pub use dereth_client_runtime::audio::*;

/// The default output endpoint through `cpal`: the executable's [`AudioOutput`] implementation.
/// It owns the device probe and the output stream.
///
/// [`AudioOutput`]: dereth_client_runtime::platform::audio_out::AudioOutput
#[derive(Default)]
pub struct CpalOutput {
    endpoint: Option<(cpal::Device, cpal::SupportedStreamConfig)>,
}

impl std::fmt::Debug for CpalOutput {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("CpalOutput")
            .field("probed", &self.endpoint.is_some())
            .finish()
    }
}

impl dereth_client_runtime::platform::audio_out::AudioOutput for CpalOutput {
    fn probe(&mut self) -> Result<(u32, u16), String> {
        use cpal::traits::{DeviceTrait, HostTrait};
        let host = cpal::default_host();
        let (device, cfg) = host
            .default_output_device()
            .ok_or_else(|| "no default output device".to_owned())
            .and_then(|d| {
                d.default_output_config()
                    .map(|c| (d, c))
                    .map_err(|e| e.to_string())
            })?;
        let found = (cfg.sample_rate().0, cfg.channels());
        self.endpoint = Some((device, cfg));
        Ok(found)
    }

    fn start(
        &mut self,
        mut fill: Box<dyn FnMut(&mut [f32]) + Send>,
    ) -> Result<Box<dyn std::any::Any>, String> {
        use cpal::traits::{DeviceTrait, StreamTrait};
        let Some((device, cfg)) = self.endpoint.as_ref() else {
            return Err("no endpoint was probed".to_owned());
        };
        let stream_cfg: cpal::StreamConfig = cfg.config();
        let stream = device
            .build_output_stream(
                &stream_cfg,
                move |out: &mut [f32], _: &cpal::OutputCallbackInfo| fill(out),
                move |e| tracing::warn!("audio stream error: {e}"),
                None,
            )
            .map_err(|e| e.to_string())?;
        stream.play().map_err(|e| e.to_string())?;
        Ok(Box::new(stream))
    }
}

/// Install [`CpalOutput`] as the mixer's default endpoint. Idempotent.
pub fn install_default_output() {
    dereth_client_runtime::audio::install_default_output(|| Box::new(CpalOutput::default()));
}
