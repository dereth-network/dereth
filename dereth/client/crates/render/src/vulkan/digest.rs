//! Test builds: one number for what a frame submitted, so two builds, or one build under two
//! settings, can be shown to have submitted exactly the same frame without reading a pixel; and
//! what the device asked the driver for when it was made (`Request`).
//!
//! The Vulkan device records nothing to replay, so the digest is taken as the frame is
//! submitted: each viewport and depth-clear rectangle; each bind, by the texture slot and
//! sampler request it binds; each landscape-layer set, by its slots and tilings; and each draw,
//! by its pipeline's key and every constant and vertex byte it uploads, in order. It is a 64-bit
//! FNV-1a hash, the same on every machine and in every run, and it is the same arithmetic as the
//! `wgpu` device's, over what this device can see.

#[cfg(feature = "test-support")]
use std::hash::{Hash, Hasher};

#[cfg(feature = "test-support")]
use crate::PipelineKey;

/// FNV-1a, 64 bits.
#[cfg(feature = "test-support")]
#[derive(Debug, Clone, Copy)]
struct Fnv(u64);

#[cfg(feature = "test-support")]
impl Fnv {
    const fn new() -> Self {
        Self(0xCBF2_9CE4_8422_2325)
    }
}

#[cfg(feature = "test-support")]
impl Hasher for Fnv {
    fn finish(&self) -> u64 {
        self.0
    }

    fn write(&mut self, bytes: &[u8]) {
        for b in bytes {
            self.0 ^= u64::from(*b);
            self.0 = self.0.wrapping_mul(0x0000_0100_0000_01B3);
        }
    }
}

/// One step of a frame, as the digest takes it.
#[cfg(feature = "test-support")]
#[derive(Hash)]
pub(crate) enum Step<'a> {
    Viewport([u32; 4]),
    ClearDepth([u32; 4]),
    Bind {
        stage: u8,
        slot: u32,
        sampler: u32,
    },
    Layers(&'a [u32]),
    Draw {
        key: &'a PipelineKey,
        frame: &'a [u8],
        draw: &'a [u8],
        vertices: &'a [u8],
    },
}

/// The running digest of the frame being recorded, and the last ended frame's.
#[cfg(feature = "test-support")]
#[derive(Debug, Default)]
pub(crate) struct FrameDigest {
    on: bool,
    running: Option<Fnv>,
    last: Option<u64>,
}

#[cfg(feature = "test-support")]
impl FrameDigest {
    /// Start or stop digesting.
    pub(crate) fn set(&mut self, on: bool) {
        self.on = on;
        if !on {
            self.last = None;
            self.running = None;
        }
    }

    /// A frame begins.
    pub(crate) fn begin(&mut self) {
        self.running = self.on.then(Fnv::new);
    }

    /// One step of the frame.
    pub(crate) fn note(&mut self, step: &Step<'_>) {
        if let Some(h) = self.running.as_mut() {
            step.hash(h);
        }
    }

    /// The frame ends.
    pub(crate) fn end(&mut self) {
        if let Some(h) = self.running.take() {
            self.last = Some(h.finish());
        }
    }

    /// The last ended frame's digest, while digesting is on.
    pub(crate) fn last(&self) -> Option<u64> {
        self.last
    }
}

/// What the device asked the driver for when it was made: the instance's API version, layers
/// and extensions, and the device's extensions and every feature structure it passed, each as
/// a line. Kept in test builds only; elsewhere noting is free and keeps nothing.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Request {
    #[cfg(feature = "test-support")]
    lines: Vec<String>,
}

impl Request {
    /// Note one line of the request.
    #[allow(unused_variables, clippy::needless_pass_by_value)]
    pub(crate) fn note(&mut self, line: impl FnOnce() -> String) {
        #[cfg(feature = "test-support")]
        self.lines.push(line());
    }

    /// The request, line by line.
    #[cfg(feature = "test-support")]
    #[must_use]
    pub fn lines(&self) -> &[String] {
        &self.lines
    }
}
