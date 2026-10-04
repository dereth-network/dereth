//! `0x0A` payload -> PCM, and the one MP3.
//!
//! `dereth-assets` owns the *record* layout (three dwords then two blobs); this
//! module owns what the client does with the blobs when it creates a sound buffer and copies the
//! wave into it:
//!
//! * `wFormatTag == WAVE_FORMAT_PCM` (785 of the 786 shipped waves): the sample bytes are `memcpy`'d
//!   straight into a DirectSound secondary buffer created **in the wave's own format**, and
//!   DirectSound's mixer resamples them to the 11025 Hz stereo primary at playback time. The client
//!   itself does no resampling and no format conversion.
//! * anything else (exactly one wave, `0A000393`): `acmStreamOpen` with a fixed destination format of
//!   **PCM, 1 channel, 11025 Hz, 16-bit**, regardless of what the source is.
//!
//! The decoder preserves the source-format handling and fixed output format described below.
//!
//! ## What this module changes, and why it is allowed to
//!
//! The DirectSound object model is replaceable; the observable content is the sample stream that
//! reaches the 11025 Hz stereo mix. This module therefore does
//! at load time what DirectSound's mixer did at play time: convert to `f32`, duplicate mono to both
//! channels, and resample to [`MIX_RATE`] by linear interpolation — which is what the DirectSound
//! software mixer does. Nothing observes the compressed form or the buffer format, and
//! because `SetFrequency` is never called there is no pitch variation to preserve.

use std::sync::Arc;

use dereth_assets::audio::{Wave, WAVE_FORMAT_MPEGLAYER3, WAVE_FORMAT_PCM};
use dereth_primitives::DataId;

use crate::error::AudioError;

/// The mix rate, step 8: the primary buffer's format is
/// `WAVE_FORMAT_PCM, 2 ch, 16 bit, 11025 Hz`. Everything DirectSound played was resampled to this.
///
/// Mixing at 48 kHz instead is explicitly free to change and is *better*; 11025 is kept because it
/// is what the original mixed at, and because it makes the load-time resample the same operation the
/// original's mixer performed rather than an extra one.
pub const MIX_RATE: u32 = 11025;

/// The ACM destination format: PCM, 1 channel, 11025 Hz, 16-bit.
/// Every compressed wave decodes to this regardless of its source rate or channel count.
pub const ACM_TARGET_RATE: u32 = MIX_RATE;
/// The channel count of that destination format.
pub const ACM_TARGET_CHANNELS: u16 = 1;

/// One decoded wave: interleaved stereo `f32` at [`MIX_RATE`], ready for the voice pool.
///
/// The client's equivalent is a `IDirectSoundBuffer` holding the wave's own format; the difference is
/// where the format conversion happens, not what comes out of it.
#[derive(Debug, Clone)]
pub struct Sample {
    /// Interleaved L,R pairs at [`MIX_RATE`].
    pub frames: Arc<[f32]>,
    /// The source's `nSamplesPerSec`, kept for tests and for counting source rates.
    pub source_rate: u32,
    /// The source's `nChannels`.
    pub source_channels: u16,
    /// The source's `wBitsPerSample` (0 for the MP3).
    pub source_bits: u16,
    /// The source's `wFormatTag`.
    pub format_tag: u16,
}

impl Sample {
    /// Number of stereo frames.
    #[must_use]
    pub fn len_frames(&self) -> usize {
        self.frames.len() / 2
    }

    /// Duration in seconds at [`MIX_RATE`].
    #[must_use]
    pub fn seconds(&self) -> f64 {
        // A frame count fits an f64 exactly at these sizes.
        let n = u32::try_from(self.len_frames()).unwrap_or(u32::MAX);
        f64::from(n) / f64::from(MIX_RATE)
    }
}

/// Decode one `0x0A` payload into a [`Sample`].
///
/// `record` is the whole dat record the [`Wave`] was decoded from, because `Wave::payload` indexes
/// into it. A format the client's ACM path could not open produces
/// [`AudioError::UnsupportedFormat`]; the client's own behaviour there is a null buffer and a
/// silent `Play`, which [`crate::AudioSystem`] reproduces by dropping the error.
pub fn decode(wave: &Wave, record: &[u8]) -> Result<Sample, AudioError> {
    let fmt = wave
        .format
        .ok_or(AudioError::ShortHeader(wave.id, wave.header.len()))?;
    let data = wave.payload(record).ok_or(AudioError::ShortPayload(
        wave.id,
        wave.data_size as usize,
        record.len(),
    ))?;

    match fmt.format_tag {
        WAVE_FORMAT_PCM => {
            if fmt.samples_per_sec == 0 {
                return Err(AudioError::ZeroRate(wave.id));
            }
            if fmt.channels != 1 && fmt.channels != 2 {
                return Err(AudioError::BadChannelCount(wave.id, fmt.channels));
            }
            let mono_or_stereo = decode_pcm(wave.id, data, fmt.channels, fmt.bits_per_sample)?;
            Ok(Sample {
                frames: to_stereo_at_mix_rate(&mono_or_stereo, fmt.channels, fmt.samples_per_sec),
                source_rate: fmt.samples_per_sec,
                source_channels: fmt.channels,
                source_bits: fmt.bits_per_sample,
                format_tag: fmt.format_tag,
            })
        }
        WAVE_FORMAT_MPEGLAYER3 => {
            // The ACM destination is mono 11025 whatever the source is, so the decode is followed by
            // the same down-mix and resample the ACM driver would have done.
            let (pcm, rate, channels) = decode_mp3(wave.id, data)?;
            let mono = if channels == 1 {
                pcm
            } else {
                downmix_to_mono(&pcm, channels)
            };
            let mono = resample_linear(&mono, rate, ACM_TARGET_RATE);
            Ok(Sample {
                frames: to_stereo_at_mix_rate(&mono, ACM_TARGET_CHANNELS, ACM_TARGET_RATE),
                source_rate: fmt.samples_per_sec,
                source_channels: fmt.channels,
                source_bits: fmt.bits_per_sample,
                format_tag: fmt.format_tag,
            })
        }
        other => Err(AudioError::UnsupportedFormat(wave.id, other)),
    }
}

/// PCM8 or PCM16 -> `f32`, still interleaved at the source channel count and rate.
///
/// 8-bit RIFF PCM is **unsigned** with centre 128, 16-bit is signed little-endian: `(s - 128) /
/// 128.0` and `s / 32768.0` respectively. 71 of the shipped waves are 8-bit. A
/// trailing partial frame is dropped, as copying the data-size field's bytes into a buffer sized
/// from the same number would leave it.
fn decode_pcm(id: DataId, data: &[u8], channels: u16, bits: u16) -> Result<Vec<f32>, AudioError> {
    match bits {
        8 => Ok(data
            .iter()
            .map(|&b| (f32::from(b) - 128.0) / 128.0)
            .collect()),
        16 => {
            let n = data.len() / 2;
            let mut out = Vec::with_capacity(n);
            for i in 0..n {
                let s = i16::from_le_bytes([data[2 * i], data[2 * i + 1]]);
                out.push(f32::from(s) / 32768.0);
            }
            // Drop a trailing partial frame rather than emitting a half-frame of silence.
            let per = usize::from(channels);
            out.truncate(out.len() / per * per);
            Ok(out)
        }
        other => Err(AudioError::BadBitDepth(id, other)),
    }
}

/// Decode the one MPEG Layer 3 wave.
///
/// `msacm32` is free to replace: it is a Windows-only, driver-dependent codec bus opened
/// for exactly one 5 KB clip, and if the system codec were missing the client played silence.
/// `symphonia` is the portable substitute used here.
///
/// The payload is a bare MPEG elementary stream — MPEG 2.5 Layer III, 11025 Hz, mono — with no RIFF
/// container, which is exactly what `MpaReader` expects.
fn decode_mp3(id: DataId, data: &[u8]) -> Result<(Vec<f32>, u32, u16), AudioError> {
    use symphonia_bundle_mp3::{MpaDecoder, MpaReader};
    use symphonia_core::codecs::audio::{AudioDecoder, AudioDecoderOptions};
    use symphonia_core::formats::{FormatOptions, FormatReader};
    use symphonia_core::io::{MediaSourceStream, MediaSourceStreamOptions};

    let err = |e: &dyn std::fmt::Display| AudioError::Mp3(id, e.to_string());

    let src = std::io::Cursor::new(data.to_vec());
    let mss = MediaSourceStream::new(Box::new(src), MediaSourceStreamOptions::default());
    let mut reader = MpaReader::try_new(mss, FormatOptions::default()).map_err(|e| err(&e))?;
    let params = reader
        .tracks()
        .first()
        .and_then(|t| t.codec_params.as_ref())
        .and_then(codec_params_audio)
        .ok_or_else(|| AudioError::Mp3(id, "no audio track".to_owned()))?
        .clone();
    let mut decoder =
        MpaDecoder::try_new(&params, &AudioDecoderOptions::default()).map_err(|e| err(&e))?;

    let mut out: Vec<f32> = Vec::new();
    let mut rate = 0u32;
    let mut channels = 0u16;
    let mut scratch: Vec<f32> = Vec::new();
    while let Some(packet) = reader.next_packet().map_err(|e| err(&e))? {
        let buf = decoder.decode(&packet).map_err(|e| err(&e))?;
        rate = buf.spec().rate();
        channels = u16::try_from(buf.num_planes()).unwrap_or(1);
        let n = buf.samples_interleaved();
        scratch.clear();
        scratch.resize(n, 0.0);
        buf.copy_to_slice_interleaved(scratch.as_mut_slice());
        out.extend_from_slice(&scratch);
    }
    if rate == 0 || channels == 0 {
        return Err(AudioError::Mp3(id, "stream decoded to nothing".to_owned()));
    }
    Ok((out, rate, channels))
}

/// A local helper so the `symphonia_core::codecs::CodecParameters` import stays inside [`decode_mp3`].
fn codec_params_audio(
    p: &symphonia_core::codecs::CodecParameters,
) -> Option<&symphonia_core::codecs::audio::AudioCodecParameters> {
    p.audio()
}

/// Average the channels. Only reachable if a future compressed wave is stereo; the shipped MP3 is
/// mono, so nothing in the retail data takes this path.
fn downmix_to_mono(interleaved: &[f32], channels: u16) -> Vec<f32> {
    let c = usize::from(channels).max(1);
    let n = interleaved.len() / c;
    let inv = 1.0 / c as f32;
    (0..n)
        .map(|i| interleaved[i * c..i * c + c].iter().sum::<f32>() * inv)
        .collect()
}

/// Resample and widen to interleaved stereo at [`MIX_RATE`].
///
/// Mono is duplicated into both channels, which is what the DirectSound mixer does before applying
/// the pan. Stereo keeps its channels.
/// `pub(crate)` because [`crate::video`] widens the intro movie's 48 kHz stereo PCM track with the
/// same two lines; the alternative was a second resampler that could drift from this one.
pub(crate) fn to_stereo_at_mix_rate(interleaved: &[f32], channels: u16, rate: u32) -> Arc<[f32]> {
    let frames = if channels == 2 {
        let n = interleaved.len() / 2;
        let l: Vec<f32> = (0..n).map(|i| interleaved[2 * i]).collect();
        let r: Vec<f32> = (0..n).map(|i| interleaved[2 * i + 1]).collect();
        let l = resample_linear(&l, rate, MIX_RATE);
        let r = resample_linear(&r, rate, MIX_RATE);
        let mut out = Vec::with_capacity(l.len() * 2);
        for i in 0..l.len() {
            out.push(l[i]);
            out.push(r[i]);
        }
        out
    } else {
        let m = resample_linear(interleaved, rate, MIX_RATE);
        let mut out = Vec::with_capacity(m.len() * 2);
        for s in m {
            out.push(s);
            out.push(s);
        }
        out
    };
    frames.into()
}

/// Linear interpolation, which is what the DirectSound software mixer used for rate conversion. An
/// equal-rate source is copied through untouched, so the 471 waves already at 11025 Hz are
/// bit-exact.
///
/// The phase accumulator is integer — `t = j * from / to` in exact rationals — so there is no
/// float-to-int conversion anywhere in here and the position never drifts on a long clip.
fn resample_linear(src: &[f32], from: u32, to: u32) -> Vec<f32> {
    if from == to || src.is_empty() {
        return src.to_vec();
    }
    let n_in = src.len();
    let from64 = u64::from(from);
    let to64 = u64::from(to);
    let n_out = usize::try_from(n_in as u64 * to64 / from64).unwrap_or(n_in);
    let inv = 1.0 / to as f32;
    let mut out = Vec::with_capacity(n_out);
    for j in 0..n_out {
        let t = j as u64 * from64;
        let i = usize::try_from(t / to64).unwrap_or(n_in - 1);
        let frac = (t % to64) as f32 * inv;
        let a = src[i.min(n_in - 1)];
        let b = src[(i + 1).min(n_in - 1)];
        out.push(a + (b - a) * frac);
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use dereth_assets::audio::WaveFormat;

    fn wave_record(fmt: WaveFormat, data: &[u8]) -> (Wave, Vec<u8>) {
        let mut header = Vec::new();
        header.extend_from_slice(&fmt.format_tag.to_le_bytes());
        header.extend_from_slice(&fmt.channels.to_le_bytes());
        header.extend_from_slice(&fmt.samples_per_sec.to_le_bytes());
        header.extend_from_slice(&fmt.avg_bytes_per_sec.to_le_bytes());
        header.extend_from_slice(&fmt.block_align.to_le_bytes());
        header.extend_from_slice(&fmt.bits_per_sample.to_le_bytes());
        header.extend_from_slice(&fmt.cb_size.unwrap_or(0).to_le_bytes());
        let hlen = u32::try_from(header.len()).expect("small");
        let dlen = u32::try_from(data.len()).expect("small");
        let mut record = Vec::new();
        record.extend_from_slice(&0x0A00_0002u32.to_le_bytes());
        record.extend_from_slice(&hlen.to_le_bytes());
        record.extend_from_slice(&dlen.to_le_bytes());
        let data_offset = record.len() + header.len();
        record.extend_from_slice(&header);
        record.extend_from_slice(data);
        let w = Wave {
            id: DataId(0x0A00_0002),
            header_size: hlen,
            data_size: dlen,
            header,
            data_offset,
            format: Some(fmt),
        };
        (w, record)
    }

    fn pcm16_mono(rate: u32) -> WaveFormat {
        WaveFormat {
            format_tag: WAVE_FORMAT_PCM,
            channels: 1,
            samples_per_sec: rate,
            avg_bytes_per_sec: rate * 2,
            block_align: 2,
            bits_per_sample: 16,
            cb_size: Some(0),
        }
    }

    /// Oracle: 8-bit RIFF PCM is unsigned with centre 128, 16-bit PCM is signed little-endian, and
    /// the recovered conversion is `(s - 128) / 128.0`.
    #[test]
    fn eight_bit_is_unsigned_around_128_and_sixteen_bit_is_signed() {
        let mut f8 = pcm16_mono(MIX_RATE);
        f8.bits_per_sample = 8;
        f8.block_align = 1;
        let (w, rec) = wave_record(f8, &[128u8, 255, 0, 64]);
        let s = decode(&w, &rec).expect("decodes");
        // Interleaved stereo, both channels equal.
        assert_eq!(s.frames.len(), 8);
        assert_eq!(s.frames[0], 0.0, "128 is silence");
        assert_eq!(s.frames[2], (255.0 - 128.0) / 128.0);
        assert_eq!(s.frames[4], -1.0, "0 is full negative");
        assert_eq!(s.frames[6], (64.0 - 128.0) / 128.0);

        let data: Vec<u8> = [0i16, 32767, -32768, -1]
            .iter()
            .flat_map(|s| s.to_le_bytes())
            .collect();
        let (w, rec) = wave_record(pcm16_mono(MIX_RATE), &data);
        let s = decode(&w, &rec).expect("decodes");
        assert_eq!(s.frames[0], 0.0);
        assert_eq!(s.frames[2], 32767.0 / 32768.0);
        assert_eq!(s.frames[4], -1.0);
    }

    /// A wave already at the primary buffer's rate is copied through with no resampling at all —
    /// 471 of the 786 shipped waves are 11025 Hz, so this is the common path.
    #[test]
    fn a_wave_already_at_the_mix_rate_is_bit_exact() {
        let data: Vec<u8> = (0..64i16).flat_map(|s| (s * 300).to_le_bytes()).collect();
        let (w, rec) = wave_record(pcm16_mono(MIX_RATE), &data);
        let s = decode(&w, &rec).expect("decodes");
        assert_eq!(s.len_frames(), 64);
        for i in 0..64i16 {
            let want = f32::from(i * 300) / 32768.0;
            let k = usize::try_from(i).expect("non-negative");
            assert_eq!(s.frames[2 * k], want);
            assert_eq!(
                s.frames[2 * k + 1],
                want,
                "mono is duplicated to both channels"
            );
        }
    }

    /// 22050 Hz halves to 11025 Hz: the output is half as long and, for a linear ramp, is the
    /// even-indexed input samples exactly.
    #[test]
    fn a_22050_hz_wave_resamples_to_half_the_frames() {
        let data: Vec<u8> = (0..100i16).flat_map(|s| (s * 100).to_le_bytes()).collect();
        let (w, rec) = wave_record(pcm16_mono(22050), &data);
        let s = decode(&w, &rec).expect("decodes");
        assert_eq!(s.len_frames(), 50);
        assert_eq!(s.source_rate, 22050);
        for j in 0..50i16 {
            let want = f32::from(2 * j * 100) / 32768.0;
            let k = usize::try_from(j).expect("non-negative");
            assert!((s.frames[2 * k] - want).abs() < 1e-6, "frame {j}");
        }
    }

    /// Stereo keeps both channels distinct rather than being folded to mono.
    #[test]
    fn a_stereo_wave_keeps_its_two_channels() {
        let mut f = pcm16_mono(MIX_RATE);
        f.channels = 2;
        f.block_align = 4;
        let data: Vec<u8> = [1000i16, -1000, 2000, -2000]
            .iter()
            .flat_map(|s| s.to_le_bytes())
            .collect();
        let (w, rec) = wave_record(f, &data);
        let s = decode(&w, &rec).expect("decodes");
        assert_eq!(s.len_frames(), 2);
        assert_eq!(s.frames[0], 1000.0 / 32768.0);
        assert_eq!(s.frames[1], -1000.0 / 32768.0);
        assert_eq!(s.frames[2], 2000.0 / 32768.0);
        assert_eq!(s.frames[3], -2000.0 / 32768.0);
    }

    /// The client's silent-failure behaviour for a format `msacm32` cannot open: an error the caller
    /// swallows, never a panic. Oracle: the client leaves its buffer pointer NULL and its play
    /// call returns 0 with no log line.
    #[test]
    fn an_unknown_format_tag_is_an_error_not_a_panic() {
        let mut f = pcm16_mono(MIX_RATE);
        f.format_tag = 0x0011; // IMA ADPCM, which no shipped wave uses.
        let (w, rec) = wave_record(f, &[0u8; 16]);
        assert!(matches!(
            decode(&w, &rec),
            Err(AudioError::UnsupportedFormat(_, 0x0011))
        ));
    }
}
