//! What every pass of a frame shares: where the frame is seen from, the rows of a constants
//! buffer as the shaders read them, a buffer rewritten every frame, the push constants' words, and
//! how a pixel is found where it was last frame.

use dereth_gfx::{Buffer, BufferDesc, BufferUsage, Device, Location};
use glam::{Mat4, Vec3};

/// Where the frame is seen from.
#[derive(Debug, Clone, Copy, PartialEq, Default)]
pub struct Camera {
    /// From the scene's space to the eye's.
    pub view: Mat4,
    /// From the eye's space to the clip space the game's own renderer draws in.
    pub projection: Mat4,
    /// The last frame's view and projection together, in this frame's space; `None` on the
    /// first frame.
    pub previous_view_projection: Option<Mat4>,
    /// The eye, in the scene's space.
    pub eye: Vec3,
    /// The world's rectangle on the back buffer: x, y, width and height.
    pub viewport: [u32; 4],
    pub fov_y: f32,
    pub aspect: f32,
    pub near: f32,
    pub far: f32,
}

/// Rows of four 32-bit values, as the shaders read a buffer.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct Rows(pub Vec<[u32; 4]>);

impl Rows {
    /// A row of four floats.
    pub fn f(&mut self, v: [f32; 4]) {
        self.0.push(v.map(f32::to_bits));
    }

    /// A row of four words.
    pub fn u(&mut self, v: [u32; 4]) {
        self.0.push(v);
    }

    /// A matrix, as four rows of its columns.
    pub fn mat(&mut self, m: Mat4) {
        for c in m.to_cols_array_2d() {
            self.f(c);
        }
    }

    /// The rows as bytes.
    #[must_use]
    pub fn bytes(&self) -> Vec<u8> {
        self.0
            .iter()
            .flat_map(|r| r.iter().flat_map(|w| w.to_le_bytes()))
            .collect()
    }
}

/// One buffer rewritten every frame, grown when a frame needs more.
#[derive(Debug)]
pub struct PerFrame {
    /// The buffer, and its index in the table.
    pub buffer: Buffer,
    pub index: u32,
    size: u64,
    usage: BufferUsage,
    label: &'static str,
}

impl PerFrame {
    /// A buffer of `size` bytes, a storage buffer besides `usage`, bound in the table.
    ///
    /// # Errors
    /// What the device refuses.
    pub fn new(
        d: &mut Device,
        label: &'static str,
        size: u64,
        usage: BufferUsage,
    ) -> Result<Self, dereth_gfx::Error> {
        let usage = usage | BufferUsage::STORAGE;
        let buffer = d.create_buffer(&BufferDesc {
            label,
            size,
            usage,
            location: Location::Gpu,
        })?;
        let index = d.bind_buffer(buffer)?;
        Ok(Self {
            buffer,
            index,
            size,
            usage,
            label,
        })
    }

    /// Write `bytes` at the start, growing the buffer first when they do not fit.
    ///
    /// # Errors
    /// What the device refuses.
    pub fn write(&mut self, d: &mut Device, bytes: &[u8]) -> Result<(), dereth_gfx::Error> {
        let need = (bytes.len() as u64).max(16);
        if need > self.size {
            let size = need.next_power_of_two();
            let bigger = d.create_buffer(&BufferDesc {
                label: self.label,
                size,
                usage: self.usage,
                location: Location::Gpu,
            })?;
            d.destroy_buffer(std::mem::replace(&mut self.buffer, bigger));
            self.index = d.bind_buffer(self.buffer)?;
            self.size = size;
        }
        if bytes.is_empty() {
            return Ok(());
        }
        d.write_buffer(self.buffer, 0, bytes)
    }
}

/// Little-endian words, as the shaders' push constants read them.
#[must_use]
pub fn push_words(w: &[u32]) -> Vec<u8> {
    let mut out = Vec::with_capacity(w.len() * 4);
    for x in w {
        out.extend_from_slice(&x.to_le_bytes());
    }
    out
}

/// What takes a point of this frame's clip space, as drawn (its projection moved by `jitter`), to
/// where `previous`, last frame's view and projection unmoved, had it: `previous` after the inverse
/// of this frame's `view_proj` moved by `jitter`, worked out in double precision. A pixel's last
/// place is then found from its place and its depth alone, never through the frame's space, where
/// dividing out a far point's barely known depth throws it off its line of sight by a share of
/// its distance from the space's origin; here a depth's error moves the point along that line,
/// which a turning view does not see.
#[must_use]
pub fn reprojection(previous: Mat4, view_proj: Mat4, jitter: [f32; 2]) -> Mat4 {
    let drawn = glam::DMat4::from_translation(glam::DVec3::new(
        f64::from(jitter[0]),
        f64::from(jitter[1]),
        0.0,
    )) * view_proj.as_dmat4();
    (previous.as_dmat4() * drawn.inverse()).as_mat4()
}
