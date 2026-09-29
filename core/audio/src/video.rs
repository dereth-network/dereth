//! The intro movie: `turbine_logo_ac.avi` from the retail install, demuxed and decoded.
//!
//! The original movie player builds a DirectShow graph with `IGraphBuilder::Render`; Intelligent
//! Connect inserts the AVI
//! splitter and the Cinepak ICM wrapper, and a texture-renderer subclass copies each decoded RGB24
//! frame into a UI surface. DirectShow, `baseclasses` and that renderer are implementation details
//! free to change, so this module replaces the graph with an explicit demux and decode and keeps
//! only the observable parts:
//!
//! * 640x480, 29.97 fps, 219 frames, about 7.31 s, **no looping** (`dwFlags` is always 0);
//! * the **vertical flip** — the ICM decompressor writes a bottom-up DIB and the texture renderer
//!   copies it into a top-down surface;
//! * **alpha forced opaque** — `dst = 0xFF000000 | (R << 16) | (G << 8) | B` for `PFID_A8R8G8B8`;
//! * a missing or undecodable file **skips the step silently**: no error, no log;
//! * the movie's audio bypasses every `Sound.*` preference, and there is no skip-on-input. Adding one
//!   is a deliberate improvement, not a port.
//!
//! # What is proved here
//!
//! Three decoders now agree on all 219 frames of the shipped movie, and two of the comparisons are
//! exact and were executed on this machine:
//!
//! * against **`ffmpeg`**, pixel for pixel, 67,276,800 pixels, 0 differing —
//!   `tests/dat/presentation/intro_movie.rs`;
//! * against **Microsoft's own `iccvid.dll`**, the decoder the retail client actually played this
//!   movie through, byte for byte on every frame's post-flip surface.
//!
//! The `iccvid.dll` comparison drove the DLL through ICM in a 32-bit helper (the DLL is i386-only,
//! so a 64-bit process cannot load it) and compared per-frame SHA-256s. That helper is not kept, so
//! the `iccvid.dll` comparison is a recorded result rather than a re-runnable test; the `ffmpeg`
//! comparison still runs.

use std::path::Path;
use std::sync::Arc;

use dereth_primitives::LocalTime;

/// `'cvid'` — the only compression the shipped movie uses.
pub const FOURCC_CVID: [u8; 4] = *b"cvid";

/// What the AVI header says about the video stream.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct MovieInfo {
    pub width: u32,
    pub height: u32,
    /// `avih.dwMicroSecPerFrame`. 33367 for the shipped movie, i.e. 29.970 fps.
    pub micro_sec_per_frame: u32,
    /// `avih.dwTotalFrames`.
    pub total_frames: u32,
    /// `strf.biCompression`.
    pub compression: [u8; 4],
    /// `strf.biBitCount`.
    pub bits_per_pixel: u16,
}

impl MovieInfo {
    #[must_use]
    pub fn fps(self) -> f64 {
        if self.micro_sec_per_frame == 0 {
            0.0
        } else {
            1_000_000.0 / f64::from(self.micro_sec_per_frame)
        }
    }
}

/// What the AVI header says about the **audio** stream, if it has one.
///
/// `turbine_logo_ac.avi` ships `WAVE_FORMAT_PCM, 2 ch, 16 bit, 48000 Hz` — an ordinary
/// `WAVEFORMATEX` in the audio `strl`'s `strf`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct MovieAudioInfo {
    /// `wFormatTag`. 1 is `WAVE_FORMAT_PCM`, the only one the shipped movie uses.
    pub format_tag: u16,
    /// `nChannels`.
    pub channels: u16,
    /// `nSamplesPerSec`.
    pub sample_rate: u32,
    /// `wBitsPerSample`.
    pub bits_per_sample: u16,
}

/// The intro movie's decoded soundtrack, ready for the mixer.
#[derive(Debug, Clone)]
pub struct MovieAudio {
    /// What the file said it was, before the widening below.
    pub info: MovieAudioInfo,
    /// Interleaved L,R at [`crate::MIX_RATE`], exactly like [`crate::Sample::frames`].
    pub frames: Arc<[f32]>,
}

/// One AVI file, demuxed, with a Cinepak decoder attached.
#[derive(Debug)]
pub struct Movie {
    info: MovieInfo,
    data: Vec<u8>,
    /// Byte ranges of the video stream's `##dc`/`##db` chunks, in presentation order.
    frames: Vec<(usize, usize)>,
    /// The audio stream's `WAVEFORMATEX`, when the file has an audio `strl`.
    audio_info: Option<MovieAudioInfo>,
    /// Byte ranges of the audio stream's `##wb` chunks, in order.
    audio_chunks: Vec<(usize, usize)>,
    next: usize,
    dec: Cinepak,
    /// A8R8G8B8, top-down, after the flip.
    surface: Vec<u8>,
    start: Option<LocalTime>,
}

impl Movie {
    /// Open the movie. **`None` means the step is skipped silently**, which is what the client
    /// does for a missing or undecodable file — no error, no log.
    #[must_use]
    pub fn open(path: &Path) -> Option<Self> {
        let data = std::fs::read(path).ok()?;
        Self::from_bytes(data)
    }

    /// The demux, split out so a test can drive it from bytes.
    #[must_use]
    pub fn from_bytes(data: Vec<u8>) -> Option<Self> {
        let d = demux(&data)?;
        let info = d.info;
        let w = usize::try_from(info.width).ok()?;
        let h = usize::try_from(info.height).ok()?;
        if w == 0 || h == 0 || info.compression != FOURCC_CVID {
            return None;
        }
        Some(Self {
            info,
            data,
            frames: d.frames,
            audio_info: d.audio_info,
            audio_chunks: d.audio_chunks,
            next: 0,
            dec: Cinepak::new(w, h),
            surface: vec![0; w * h * 4],
            start: None,
        })
    }

    /// What the audio `strl` declared, or `None` for a file with no audio stream.
    #[must_use]
    pub const fn audio_info(&self) -> Option<MovieAudioInfo> {
        self.audio_info
    }

    /// The whole soundtrack, decoded and widened to interleaved stereo at [`crate::MIX_RATE`].
    ///
    /// **The movie's audio does not enter sound management.** In the retail client the AVI is played
    /// by DirectShow, whose audio renderer is its own filter graph — so the track
    /// bypasses sound management entirely and with it every `Sound.*` preference, the sixteen-voice
    /// pool and positional attenuation. That is recorded at the head of this module and it is why this
    /// returns the samples rather than starting a voice.
    ///
    /// `None` when there is no audio stream, when the format is not `WAVE_FORMAT_PCM`, or when the
    /// `movi` list carried no `##wb` chunks. The caller plays a silent movie in that case.
    #[must_use]
    pub fn audio(&self) -> Option<MovieAudio> {
        let info = self.audio_info?;
        // Only `WAVE_FORMAT_PCM`. The shipped movie is PCM and adding an ACM path for a format no
        // shipped file uses would be untestable code.
        if info.format_tag != WAVE_FORMAT_PCM || info.channels == 0 || info.sample_rate == 0 {
            return None;
        }
        let mut pcm: Vec<f32> = Vec::new();
        for &(off, len) in &self.audio_chunks {
            let bytes = self.data.get(off..off + len)?;
            match info.bits_per_sample {
                // RIFF 8-bit PCM is unsigned with centre 128; 16-bit is signed little-endian.
                // The same two rules `crate::wave::decode_pcm` applies to a `0x0A` record.
                8 => pcm.extend(bytes.iter().map(|&b| (f32::from(b) - 128.0) / 128.0)),
                16 => pcm.extend(
                    bytes
                        .as_chunks::<2>()
                        .0
                        .iter()
                        .map(|c| f32::from(i16::from_le_bytes(*c)) / 32768.0),
                ),
                _ => return None,
            }
        }
        if pcm.is_empty() {
            return None;
        }
        let frames = crate::wave::to_stereo_at_mix_rate(&pcm, info.channels, info.sample_rate);
        Some(MovieAudio { info, frames })
    }

    #[must_use]
    pub fn info(&self) -> MovieInfo {
        self.info
    }

    /// How many video chunks the `movi` list actually holds, which for a well-formed file equals
    /// `avih.dwTotalFrames`.
    #[must_use]
    pub fn frame_count(&self) -> usize {
        self.frames.len()
    }

    /// Whether movie playback has finished. There is no loop: rewinding the media position to 0
    /// is never reached because the flags are always 0.
    #[must_use]
    pub fn is_finished(&self) -> bool {
        self.next >= self.frames.len()
    }

    /// Decode the frame that should be showing at `now`, or `None` if the last decoded one is still
    /// current or the movie has ended.
    ///
    /// The displayed frame rate is `min(movie fps, client fps)`: the element is drawn every frame
    /// whether or not a new video frame arrived, and there is no interpolation.
    pub fn next_frame(&mut self, now: LocalTime) -> Option<&[u8]> {
        let start = *self.start.get_or_insert(now);
        let elapsed = now.0 - start.0;
        let per = f64::from(self.info.micro_sec_per_frame) / 1_000_000.0;
        let want = if per > 0.0 {
            (elapsed / per).floor()
        } else {
            0.0
        };
        // LINT-OK: a frame index derived from wall-clock time and immediately bounded by the
        // frame count; not engine arithmetic, and the movie is 219 frames long.
        #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
        let want = if want < 0.0 { 0 } else { want as usize };
        if self.next > want || self.is_finished() {
            return None;
        }
        // Catch up: every frame up to `want` is decoded, because Cinepak is inter-coded and skipping
        // one would leave the persistent buffer wrong.
        while self.next <= want && !self.is_finished() {
            self.decode_next()?;
        }
        Some(&self.surface)
    }

    /// Decode exactly one frame, ignoring timing. Returns the A8R8G8B8 surface, top-down.
    pub fn decode_frame(&mut self, index: usize) -> Option<&[u8]> {
        if index < self.next {
            return None;
        }
        while self.next <= index {
            self.decode_next()?;
        }
        Some(&self.surface)
    }

    fn decode_next(&mut self) -> Option<()> {
        let (off, len) = *self.frames.get(self.next)?;
        let chunk = self.data.get(off..off + len)?;
        self.dec.decode(chunk)?;
        self.dec.to_surface(&mut self.surface);
        self.next += 1;
        Some(())
    }

    /// The frame as the ICM decompressor delivered it: a **bottom-up** RGB24 DIB, three bytes per
    /// pixel in B, G, R order, row 0 being the *bottom* of the picture.
    ///
    /// The sample renderer reads this and writes rows in the opposite order, which is
    /// the vertical flip. Exposed so the flip is real code with a real test rather than an assertion
    /// about an invariant nothing can see.
    #[must_use]
    pub fn bottom_up_dib(&self) -> Vec<u8> {
        self.dec.bottom_up_rgb24()
    }

    /// The decoder's raw planes: Y at full resolution, U and V at half, i.e. `yuv420p` with the
    /// chroma stored as `offset + 128`. Exposed so that a future fixture can pin the codec output
    /// without going through the colour conversion.
    #[must_use]
    pub fn yuv_planes(&self) -> (&[u8], &[u8], &[u8]) {
        (&self.dec.y, &self.dec.u, &self.dec.v)
    }
}

/// Copy a bottom-up RGB24 DIB into a top-down A8R8G8B8
/// surface, with **alpha forced opaque**.
///
/// `dst = 0xFF000000 | (R << 16) | (G << 8) | B`, written little-endian, so the bytes are B, G, R,
/// 0xFF. The source byte order is RGB24's native B, G, R. The vertical flip is essential: without it
/// the logo is upside down.
pub fn flip_bottom_up_rgb24_to_argb(src: &[u8], width: usize, height: usize, dst: &mut [u8]) {
    let pitch = width * 3;
    for row in 0..height {
        let s = (height - 1 - row) * pitch;
        let d = row * width * 4;
        for x in 0..width {
            let b = src[s + x * 3];
            let g = src[s + x * 3 + 1];
            let r = src[s + x * 3 + 2];
            dst[d + x * 4] = b;
            dst[d + x * 4 + 1] = g;
            dst[d + x * 4 + 2] = r;
            dst[d + x * 4 + 3] = 0xFF;
        }
    }
}

// -------------------------------------------------------------------------------------------------
// AVI demux
// -------------------------------------------------------------------------------------------------

fn u32le(b: &[u8], o: usize) -> Option<u32> {
    Some(u32::from_le_bytes([
        *b.get(o)?,
        *b.get(o + 1)?,
        *b.get(o + 2)?,
        *b.get(o + 3)?,
    ]))
}

fn u16le(b: &[u8], o: usize) -> Option<u16> {
    Some(u16::from_le_bytes([*b.get(o)?, *b.get(o + 1)?]))
}

/// `WAVE_FORMAT_PCM`. The only `wFormatTag` the shipped movie uses.
pub const WAVE_FORMAT_PCM: u16 = 1;

/// Everything one pass over the file yields.
struct Demuxed {
    info: MovieInfo,
    frames: Vec<(usize, usize)>,
    audio_info: Option<MovieAudioInfo>,
    audio_chunks: Vec<(usize, usize)>,
}

/// Parse `RIFF/AVI ` far enough to find both streams' formats and their `movi` chunks.
fn demux(d: &[u8]) -> Option<Demuxed> {
    if d.get(0..4)? != b"RIFF" || d.get(8..12)? != b"AVI " {
        return None;
    }
    let riff_end = (12 + u32le(d, 4)? as usize).min(d.len());

    let mut info = MovieInfo {
        width: 0,
        height: 0,
        micro_sec_per_frame: 0,
        total_frames: 0,
        compression: [0; 4],
        bits_per_pixel: 0,
    };
    let mut frames = Vec::new();
    let mut audio_chunks = Vec::new();
    let mut streams = Streams::default();

    // A flat walk of the top-level list, descending only into the lists that matter.
    let mut off = 12usize;
    while off + 8 <= riff_end {
        let id = d.get(off..off + 4)?;
        let size = u32le(d, off + 4)? as usize;
        let body = off + 8;
        if id == b"LIST" {
            match d.get(body..body + 4)? {
                b"hdrl" => {
                    parse_hdrl(
                        d,
                        body + 4,
                        (body + size).min(riff_end),
                        &mut info,
                        &mut streams,
                    )?;
                }
                b"movi" => {
                    let end = (body + size).min(riff_end);
                    let vs = streams.video.unwrap_or(0);
                    collect_movi(d, body + 4, end, vs, &VIDEO_SUFFIXES, &mut frames);
                    // The audio stream is collected in the same list and in the same order; a file
                    // with no audio `strl` has no index to look for and stays silent.
                    if let Some(a) = streams.audio {
                        collect_movi(d, body + 4, end, a, &AUDIO_SUFFIXES, &mut audio_chunks);
                    }
                }
                _ => {}
            }
        }
        off = body + size + (size & 1);
    }
    if info.width == 0 || frames.is_empty() {
        return None;
    }
    Some(Demuxed {
        info,
        frames,
        audio_info: streams.audio_info,
        audio_chunks,
    })
}

/// Which `strl` was which, while the header walk is in progress.
#[derive(Debug, Default)]
struct Streams {
    video: Option<usize>,
    audio: Option<usize>,
    audio_info: Option<MovieAudioInfo>,
    next_index: usize,
}

fn parse_hdrl(
    d: &[u8],
    mut off: usize,
    end: usize,
    info: &mut MovieInfo,
    streams: &mut Streams,
) -> Option<()> {
    while off + 8 <= end {
        let id = d.get(off..off + 4)?;
        let size = u32le(d, off + 4)? as usize;
        let body = off + 8;
        match id {
            b"avih" => {
                // `AVIMAINHEADER`: dwMicroSecPerFrame, dwMaxBytesPerSec, dwPaddingGranularity,
                // dwFlags, dwTotalFrames, dwInitialFrames, dwStreams, dwSuggestedBufferSize,
                // dwWidth, dwHeight.
                info.micro_sec_per_frame = u32le(d, body)?;
                info.total_frames = u32le(d, body + 16)?;
            }
            b"LIST" if d.get(body..body + 4)? == b"strl" => {
                let kind = parse_strl(d, body + 4, (body + size).min(end), info, streams)?;
                let idx = streams.next_index;
                match kind {
                    StreamKind::Video if streams.video.is_none() => streams.video = Some(idx),
                    StreamKind::Audio if streams.audio.is_none() => streams.audio = Some(idx),
                    _ => {}
                }
                streams.next_index += 1;
            }
            _ => {}
        }
        off = body + size + (size & 1);
    }
    Some(())
}

/// Which stream a `strl` describes. Anything that is neither is skipped and still consumes an index.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum StreamKind {
    Video,
    Audio,
    Other,
}

/// Returns what this `strl` describes, and fills the matching format.
fn parse_strl(
    d: &[u8],
    mut off: usize,
    end: usize,
    info: &mut MovieInfo,
    streams: &mut Streams,
) -> Option<StreamKind> {
    let mut kind = StreamKind::Other;
    while off + 8 <= end {
        let id = d.get(off..off + 4)?;
        let size = u32le(d, off + 4)? as usize;
        let body = off + 8;
        match id {
            b"strh" => {
                kind = match d.get(body..body + 4)? {
                    b"vids" => StreamKind::Video,
                    b"auds" => StreamKind::Audio,
                    _ => StreamKind::Other,
                }
            }
            b"strf" if kind == StreamKind::Video => {
                // `BITMAPINFOHEADER`: biSize, biWidth, biHeight, biPlanes, biBitCount, biCompression.
                info.width = u32le(d, body + 4)?;
                info.height = u32le(d, body + 8)?;
                info.bits_per_pixel = u16le(d, body + 14)?;
                info.compression
                    .copy_from_slice(d.get(body + 16..body + 20)?);
            }
            b"strf" if kind == StreamKind::Audio && streams.audio_info.is_none() => {
                // `WAVEFORMATEX`: wFormatTag, nChannels, nSamplesPerSec, nAvgBytesPerSec,
                // nBlockAlign, wBitsPerSample, cbSize -- the same 18-byte header a `0x0A` wave
                // record carries, which is why `crate::wave` reads it at the same offsets.
                streams.audio_info = Some(MovieAudioInfo {
                    format_tag: u16le(d, body)?,
                    channels: u16le(d, body + 2)?,
                    sample_rate: u32le(d, body + 4)?,
                    bits_per_sample: u16le(d, body + 14)?,
                });
            }
            _ => {}
        }
        off = body + size + (size & 1);
    }
    Some(kind)
}

/// A video chunk is `##dc` (compressed) or `##db` (uncompressed DIB).
const VIDEO_SUFFIXES: [&[u8; 2]; 2] = [b"dc", b"db"];
/// An audio chunk is `##wb`.
const AUDIO_SUFFIXES: [&[u8; 2]; 1] = [b"wb"];

/// Collect the chunks belonging to one stream, in file order.
fn collect_movi(
    d: &[u8],
    mut off: usize,
    end: usize,
    stream: usize,
    suffixes: &[&[u8; 2]],
    out: &mut Vec<(usize, usize)>,
) {
    let want = [
        b'0' + u8::try_from(stream / 10).unwrap_or(0),
        b'0' + u8::try_from(stream % 10).unwrap_or(0),
    ];
    while off + 8 <= end {
        let Some(id) = d.get(off..off + 4) else {
            return;
        };
        let Some(size) = u32le(d, off + 4) else {
            return;
        };
        let size = size as usize;
        let body = off + 8;
        if id[0..2] == want
            && suffixes.iter().any(|s| &id[2..4] == s.as_slice())
            && body + size <= d.len()
        {
            out.push((body, size));
        }
        off = body + size + (size & 1);
    }
}

// -------------------------------------------------------------------------------------------------
// Cinepak
// -------------------------------------------------------------------------------------------------

/// One codebook entry: four luma samples and a shared chroma pair.
///
/// The four Y values cover the entry's 2x2 area as `y0 y1 / y2 y3`. `u` and `v` are **signed**
/// offsets about 128.
#[derive(Debug, Clone, Copy, Default)]
struct Entry {
    y: [u8; 4],
    u: i8,
    v: i8,
}

#[derive(Debug, Clone)]
struct StripCodebooks {
    v1: [Entry; 256],
    v4: [Entry; 256],
}

impl Default for StripCodebooks {
    fn default() -> Self {
        Self {
            v1: [Entry::default(); 256],
            v4: [Entry::default(); 256],
        }
    }
}

/// A Cinepak decoder, holding the persistent frame buffer and per-strip codebooks that inter-coded
/// frames update in place.
#[derive(Debug)]
pub struct Cinepak {
    width: usize,
    height: usize,
    /// Full-resolution luma.
    y: Vec<u8>,
    /// Half-resolution chroma, stored as `yuv420p` does: `u[row/2 * (w/2) + col/2] = value + 128`.
    u: Vec<u8>,
    v: Vec<u8>,
    strips: Vec<StripCodebooks>,
}

impl Cinepak {
    #[must_use]
    pub fn new(width: usize, height: usize) -> Self {
        Self {
            width,
            height,
            y: vec![0; width * height],
            u: vec![128; (width / 2) * (height / 2)],
            v: vec![128; (width / 2) * (height / 2)],
            strips: Vec::new(),
        }
    }

    /// Decode one Cinepak frame into the persistent buffer.
    ///
    /// Frame header, 10 bytes: `flags(1) length(3 BE) width(2 BE) height(2 BE) strips(2 BE)`.
    /// `flags & 1` set means the frame is inter-coded. Strip header, 12 bytes:
    /// `id(2 BE) size(2 BE) y0(2) x0(2) y1(2) x1(2)`, with the vertical extent accumulated across
    /// strips — the shipped movie is three 160-row strips of a 480-row picture.
    pub fn decode(&mut self, frame: &[u8]) -> Option<()> {
        if frame.len() < 10 {
            return None;
        }
        let flags = frame[0];
        let num_strips = usize::from(u16::from_be_bytes([frame[8], frame[9]]));
        if self.strips.len() < num_strips {
            self.strips.resize(num_strips, StripCodebooks::default());
        }
        let mut off = 10usize;
        let mut y_top = 0usize;
        for i in 0..num_strips {
            if off + 12 > frame.len() {
                return None;
            }
            let size = usize::from(u16::from_be_bytes([frame[off + 2], frame[off + 3]]));
            let s_y0 = usize::from(u16::from_be_bytes([frame[off + 4], frame[off + 5]]));
            let s_x0 = usize::from(u16::from_be_bytes([frame[off + 6], frame[off + 7]]));
            let s_y1 = usize::from(u16::from_be_bytes([frame[off + 8], frame[off + 9]]));
            let s_x1 = usize::from(u16::from_be_bytes([frame[off + 10], frame[off + 11]]));
            if size < 12 || off + size > frame.len() {
                return None;
            }
            // On a keyframe each strip after the first starts from the previous strip's codebooks;
            // on an inter frame each strip keeps its own from the previous frame.
            if i > 0 && (flags & 0x01) == 0 {
                self.strips[i] = self.strips[i - 1].clone();
            }
            let y_bottom = (y_top + s_y1.saturating_sub(s_y0)).min(self.height);
            let x_right = s_x1.min(self.width);
            self.decode_strip(
                i,
                &frame[off + 12..off + size],
                y_top,
                s_x0,
                y_bottom,
                x_right,
            );
            y_top = y_bottom;
            off += size;
        }
        Some(())
    }

    fn decode_strip(
        &mut self,
        strip: usize,
        mut data: &[u8],
        y0: usize,
        x0: usize,
        y1: usize,
        x1: usize,
    ) {
        while data.len() >= 4 {
            let id = u16::from_be_bytes([data[0], data[1]]);
            let size = usize::from(u16::from_be_bytes([data[2], data[3]]));
            if size < 4 || size > data.len() {
                return;
            }
            let body = &data[4..size];
            match id {
                // Full codebook updates. The 0x24/0x26 forms are greyscale: four luma bytes and no
                // chroma at all.
                0x2000 => load_codebook(&mut self.strips[strip].v4, body, true),
                0x2200 => load_codebook(&mut self.strips[strip].v1, body, true),
                0x2400 => load_codebook(&mut self.strips[strip].v4, body, false),
                0x2600 => load_codebook(&mut self.strips[strip].v1, body, false),
                // Selective updates, driven by a 32-bit flag word per 32 entries.
                0x2100 => update_codebook(&mut self.strips[strip].v4, body, true),
                0x2300 => update_codebook(&mut self.strips[strip].v1, body, true),
                0x2500 => update_codebook(&mut self.strips[strip].v4, body, false),
                0x2700 => update_codebook(&mut self.strips[strip].v1, body, false),
                0x3000 => self.vectors(strip, body, y0, x0, y1, x1, Vectors::Intra),
                0x3100 => self.vectors(strip, body, y0, x0, y1, x1, Vectors::Inter),
                0x3200 => self.vectors(strip, body, y0, x0, y1, x1, Vectors::V1Only),
                _ => {}
            }
            data = &data[size..];
        }
    }

    #[allow(clippy::too_many_arguments)]
    fn vectors(
        &mut self,
        strip: usize,
        body: &[u8],
        y0: usize,
        x0: usize,
        y1: usize,
        x1: usize,
        mode: Vectors,
    ) {
        let mut bits = BitReader::new(body);
        let mut y = y0;
        while y + 4 <= y1 {
            let mut x = x0;
            while x + 4 <= x1 {
                match mode {
                    Vectors::V1Only => {
                        let Some(i) = bits.byte() else { return };
                        let e = self.strips[strip].v1[usize::from(i)];
                        self.put_v1(x, y, e);
                    }
                    Vectors::Intra => {
                        let Some(is_v4) = bits.bit() else { return };
                        if is_v4 {
                            let Some(q) = bits.four_bytes() else { return };
                            let e = q.map(|i| self.strips[strip].v4[usize::from(i)]);
                            self.put_v4(x, y, e);
                        } else {
                            let Some(i) = bits.byte() else { return };
                            let e = self.strips[strip].v1[usize::from(i)];
                            self.put_v1(x, y, e);
                        }
                    }
                    Vectors::Inter => {
                        let Some(coded) = bits.bit() else { return };
                        if coded {
                            let Some(is_v4) = bits.bit() else { return };
                            if is_v4 {
                                let Some(q) = bits.four_bytes() else { return };
                                let e = q.map(|i| self.strips[strip].v4[usize::from(i)]);
                                self.put_v4(x, y, e);
                            } else {
                                let Some(i) = bits.byte() else { return };
                                let e = self.strips[strip].v1[usize::from(i)];
                                self.put_v1(x, y, e);
                            }
                        }
                        // An uncoded block keeps whatever the previous frame left in the buffer.
                    }
                }
                x += 4;
            }
            y += 4;
        }
    }

    /// V1: one entry covers the whole 4x4 block, each luma sample filling a 2x2 quadrant.
    fn put_v1(&mut self, x: usize, y: usize, e: Entry) {
        for qy in 0..2usize {
            for qx in 0..2usize {
                let luma = e.y[qy * 2 + qx];
                for dy in 0..2usize {
                    for dx in 0..2usize {
                        let px = x + qx * 2 + dx;
                        let py = y + qy * 2 + dy;
                        if px < self.width && py < self.height {
                            self.y[py * self.width + px] = luma;
                        }
                    }
                }
                self.put_chroma(x + qx * 2, y + qy * 2, e);
            }
        }
    }

    /// V4: four entries, one per 2x2 quadrant, each entry's four luma samples being the four pixels.
    fn put_v4(&mut self, x: usize, y: usize, e: [Entry; 4]) {
        for (q, entry) in e.iter().enumerate() {
            let qx = (q & 1) * 2;
            let qy = (q >> 1) * 2;
            for dy in 0..2usize {
                for dx in 0..2usize {
                    let px = x + qx + dx;
                    let py = y + qy + dy;
                    if px < self.width && py < self.height {
                        self.y[py * self.width + px] = entry.y[dy * 2 + dx];
                    }
                }
            }
            self.put_chroma(x + qx, y + qy, *entry);
        }
    }

    /// One chroma sample per 2x2 luma block, which is exactly `yuv420p`'s subsampling.
    fn put_chroma(&mut self, x: usize, y: usize, e: Entry) {
        let cw = self.width / 2;
        let cx = x / 2;
        let cy = y / 2;
        if cx < cw && cy < self.height / 2 {
            self.u[cy * cw + cx] = (i16::from(e.u) + 128).clamp(0, 255) as u8;
            self.v[cy * cw + cx] = (i16::from(e.v) + 128).clamp(0, 255) as u8;
        }
    }

    /// The classic Cinepak colour conversion, the one Dr Tim Ferguson's reference decoder and every
    /// derivative use:
    ///
    /// ```text
    /// r = y + 2*v
    /// g = y - ((u + 1) >> 1) - v
    /// b = y + 2*u
    /// ```
    ///
    /// with `u` and `v` the signed offsets straight out of the codebook, and each result clamped to
    /// 0..255.
    ///
    /// This matches `ffmpeg`'s Cinepak decoder **pixel for pixel on all 219 frames** of the shipped
    /// movie (`tests/dat/presentation/intro_movie.rs`); `(u + 1) >> 1` and `u >> 1` each disagree with it on half the
    /// odd chroma values, so the truncating `u / 2` is settled.
    ///
    /// **This is also verified against the retail decoder itself.** Microsoft's
    /// `iccvid.dll` — what the client actually used through Intelligent Connect — was driven
    /// through ICM by a 32-bit helper and produced **byte-identical** output on all 219 frames.
    /// So the truncating `u / 2` is settled
    /// against two independent implementations, one of them the original.
    #[must_use]
    pub fn rgb_from_yuv(luma: u8, u: i8, v: i8) -> (u8, u8, u8) {
        let y = i32::from(luma);
        let u = i32::from(u);
        let v = i32::from(v);
        let clamp = |x: i32| -> u8 { x.clamp(0, 255) as u8 };
        (clamp(y + 2 * v), clamp(y - u / 2 - v), clamp(y + 2 * u))
    }

    /// The picture as a **bottom-up** RGB24 DIB, which is what the ICM decompressor handed
    /// the texture renderer.
    #[must_use]
    pub fn bottom_up_rgb24(&self) -> Vec<u8> {
        let cw = self.width / 2;
        let mut out = vec![0u8; self.width * self.height * 3];
        for row in 0..self.height {
            // Row 0 of the DIB is the bottom of the picture.
            let src_row = self.height - 1 - row;
            for x in 0..self.width {
                let luma = self.y[src_row * self.width + x];
                let ci = (src_row / 2) * cw + x / 2;
                // LINT-OK: the planes hold `offset + 128` clamped to 0..255, so subtracting
                // 128 lands back in -128..127 by construction.
                #[allow(clippy::cast_possible_truncation)]
                let u = (i16::from(self.u[ci]) - 128) as i8;
                #[allow(clippy::cast_possible_truncation)]
                let v = (i16::from(self.v[ci]) - 128) as i8;
                let (r, g, b) = Self::rgb_from_yuv(luma, u, v);
                let o = (row * self.width + x) * 3;
                out[o] = b;
                out[o + 1] = g;
                out[o + 2] = r;
            }
        }
        out
    }

    /// The bottom-up DIB flipped into a top-down A8R8G8B8 surface, alpha forced opaque.
    fn to_surface(&self, dst: &mut [u8]) {
        let dib = self.bottom_up_rgb24();
        flip_bottom_up_rgb24_to_argb(&dib, self.width, self.height, dst);
    }
}

#[derive(Debug, Clone, Copy)]
enum Vectors {
    Intra,
    Inter,
    V1Only,
}

/// A full codebook load: 6 bytes per entry with chroma, 4 without.
fn load_codebook(book: &mut [Entry; 256], body: &[u8], chroma: bool) {
    let stride = if chroma { 6 } else { 4 };
    let n = (body.len() / stride).min(256);
    for i in 0..n {
        book[i] = entry_from(&body[i * stride..], chroma);
    }
}

/// A selective codebook update: a 32-bit big-endian flag word, then one entry for each set bit,
/// most significant bit first.
fn update_codebook(book: &mut [Entry; 256], body: &[u8], chroma: bool) {
    let stride = if chroma { 6 } else { 4 };
    let mut off = 0usize;
    let mut i = 0usize;
    while i < 256 {
        if off + 4 > body.len() {
            return;
        }
        let flags = u32::from_be_bytes([body[off], body[off + 1], body[off + 2], body[off + 3]]);
        off += 4;
        for bit in 0..32usize {
            if i >= 256 {
                return;
            }
            if flags & (0x8000_0000 >> bit) != 0 {
                if off + stride > body.len() {
                    return;
                }
                book[i] = entry_from(&body[off..], chroma);
                off += stride;
            }
            i += 1;
        }
    }
}

fn entry_from(b: &[u8], chroma: bool) -> Entry {
    let y = [b[0], b[1], b[2], b[3]];
    if chroma {
        Entry {
            y,
            u: b[4] as i8,
            v: b[5] as i8,
        }
    } else {
        Entry { y, u: 0, v: 0 }
    }
}

/// The vector chunks' bit stream: 32-bit big-endian words, most significant bit first, with index
/// bytes read from the same cursor between words.
#[derive(Debug)]
struct BitReader<'a> {
    data: &'a [u8],
    pos: usize,
    word: u32,
    left: u32,
}

impl<'a> BitReader<'a> {
    fn new(data: &'a [u8]) -> Self {
        Self {
            data,
            pos: 0,
            word: 0,
            left: 0,
        }
    }

    fn bit(&mut self) -> Option<bool> {
        if self.left == 0 {
            if self.pos + 4 > self.data.len() {
                return None;
            }
            self.word = u32::from_be_bytes([
                self.data[self.pos],
                self.data[self.pos + 1],
                self.data[self.pos + 2],
                self.data[self.pos + 3],
            ]);
            self.pos += 4;
            self.left = 32;
        }
        let b = self.word & 0x8000_0000 != 0;
        self.word <<= 1;
        self.left -= 1;
        Some(b)
    }

    fn byte(&mut self) -> Option<u8> {
        let b = *self.data.get(self.pos)?;
        self.pos += 1;
        Some(b)
    }

    fn four_bytes(&mut self) -> Option<[u8; 4]> {
        let s = self.data.get(self.pos..self.pos + 4)?;
        self.pos += 4;
        Some([s[0], s[1], s[2], s[3]])
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A missing or undecodable file skips the step silently, with no error and no log.
    #[test]
    fn a_missing_file_returns_none_rather_than_failing() {
        assert!(Movie::open(Path::new("no-such-movie-xyzzy.avi")).is_none());
        assert!(Movie::from_bytes(Vec::new()).is_none());
        assert!(Movie::from_bytes(b"RIFF\0\0\0\0AVI not really".to_vec()).is_none());
    }

    /// The flip and the forced-opaque alpha, on a two-row picture. Oracle: section 2's
    /// `PFID_A8R8G8B8` row — `dst = 0xFF000000 | (R << 16) | (G << 8) | B` — and "the vertical flip
    /// is essential".
    #[test]
    fn the_flip_reverses_the_rows_and_the_alpha_is_forced_opaque() {
        // Two rows of one pixel: DIB row 0 (the picture's bottom) is blue, row 1 is red.
        let dib = vec![
            0xFF, 0x00, 0x00, /* B,G,R = blue */ 0x00, 0x00, 0xFF, /* red */
        ];
        let mut dst = vec![0u8; 8];
        flip_bottom_up_rgb24_to_argb(&dib, 1, 2, &mut dst);
        // Top-down row 0 must be the DIB's last row: red.
        assert_eq!(&dst[0..4], &[0x00, 0x00, 0xFF, 0xFF]);
        assert_eq!(&dst[4..8], &[0xFF, 0x00, 0x00, 0xFF]);
        assert!(
            dst.chunks(4).all(|p| p[3] == 0xFF),
            "alpha is forced opaque everywhere"
        );
    }

    /// The codebook and bit-stream primitives, on hand-built bytes.
    #[test]
    fn a_selective_codebook_update_touches_only_the_flagged_entries() {
        let mut book = [Entry::default(); 256];
        book[1].y = [9, 9, 9, 9];
        // Flag word with bits 0 and 2 set, then two 6-byte entries.
        let mut body = vec![0xA0, 0x00, 0x00, 0x00];
        body.extend_from_slice(&[1, 2, 3, 4, 0x10, 0xF0]);
        body.extend_from_slice(&[5, 6, 7, 8, 0x00, 0x00]);
        update_codebook(&mut book, &body, true);
        assert_eq!(book[0].y, [1, 2, 3, 4]);
        assert_eq!(book[0].u, 0x10);
        assert_eq!(book[0].v, -16, "0xF0 is a signed -16");
        assert_eq!(
            book[1].y,
            [9, 9, 9, 9],
            "entry 1 was not flagged and must be untouched"
        );
        assert_eq!(book[2].y, [5, 6, 7, 8]);
    }

    #[test]
    fn the_bit_reader_walks_32_bit_words_most_significant_bit_first() {
        let data = [0b1010_0000u8, 0, 0, 0, 0xAB];
        let mut r = BitReader::new(&data);
        assert_eq!(r.bit(), Some(true));
        assert_eq!(r.bit(), Some(false));
        assert_eq!(r.bit(), Some(true));
        assert_eq!(r.bit(), Some(false));
        // Index bytes come from the same cursor, after the flag word.
        assert_eq!(r.byte(), Some(0xAB));
        assert_eq!(r.byte(), None);
    }

    /// The greyscale codebook forms carry no chroma at all.
    #[test]
    fn a_greyscale_codebook_entry_has_zero_chroma() {
        let mut book = [Entry::default(); 256];
        load_codebook(&mut book, &[10, 20, 30, 40, 50, 60, 70, 80], false);
        assert_eq!(book[0].y, [10, 20, 30, 40]);
        assert_eq!((book[0].u, book[0].v), (0, 0));
        assert_eq!(book[1].y, [50, 60, 70, 80]);
    }
}
