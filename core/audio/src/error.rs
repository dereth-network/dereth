//! The crate's one error type.

use dereth_primitives::DataId;

/// Everything that can go wrong in this crate.
///
/// The retail client fails *silently* almost everywhere in audio: a wave that will not decode leaves
/// The client leaves the sound buffer NULL and its play call returns 0 with no log line. So
/// callers of this crate are expected to swallow most of these,
/// and [`crate::AudioSystem`] does exactly that; the variants exist so that a *test* can tell the
/// difference between "silent because the client is faithful" and "silent because we broke it".
#[derive(Debug, thiserror::Error)]
pub enum AudioError {
    #[error("wave {0}: header is {1} bytes, too short for a WAVEFORMATEX")]
    ShortHeader(DataId, usize),
    #[error("wave {0}: payload is truncated ({1} bytes declared, {2} present)")]
    ShortPayload(DataId, usize, usize),
    #[error("wave {0}: {1} channels is not 1 or 2")]
    BadChannelCount(DataId, u16),
    #[error("wave {0}: {1} bits per sample is not 8 or 16")]
    BadBitDepth(DataId, u16),
    #[error("wave {0}: sample rate 0")]
    ZeroRate(DataId),
    /// The client's own behaviour for a format `msacm32` cannot open: no buffer, no sound, no log.
    #[error("wave {0}: unsupported wFormatTag 0x{1:04X}")]
    UnsupportedFormat(DataId, u16),
    #[error("wave {0}: MPEG Layer 3 decode failed: {1}")]
    Mp3(DataId, String),
    #[error("no wave object {0}")]
    NoSuchWave(DataId),
}
