//! The default sound output endpoint through cpal, for any client whose mixer plays through the
//! sound engine's output seam.
//!
//! **Depends on** the sound engine's output seam (`dereth-audio`) and `cpal`. **Used by** the
//! desktop host (`dereth-desktop`) and the second game's desktop client (`auberean-client`), each
//! of which hands it to its mixer.
//!
//! **Must never** reach a client runtime or an application half: it is a device and nothing else,
//! so any executable can open sound with it (`cargo xtask seams`).

/// The default output endpoint through `cpal`: the executable's
/// [`AudioOutput`](dereth_audio::output::AudioOutput) implementation. It owns the device probe and
/// the output stream.
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

impl dereth_audio::output::AudioOutput for CpalOutput {
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
        let found = (cfg.sample_rate(), cfg.channels());
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
