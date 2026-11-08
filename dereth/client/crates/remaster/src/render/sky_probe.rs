//! The sky all round the eye, for what reflects it: the sky pass drawn again, as it draws the
//! frame's sky, from the eye toward each of six ways, each into a small square picture of its own.
//! The water reads it wherever a reflection looks at the sky, so what it reflects there is the
//! sky as it is drawn, its dome, clouds, sun and veil, whether or not that part of the sky is on
//! screen.
//!
//! It is drawn only while the water is, and only outdoors, where the frame draws a sky.

use dereth_gfx::{BufferUsage, Device, Format, Texture, TextureDesc, TextureUsage};
use glam::{Mat4, Vec3, Vec4};

use super::PerFrame;
use crate::scene::Camera;

/// A side of each of the six pictures, pixels.
pub const SIDE: u32 = 64;

/// The six ways, each with the way that is up in its picture: east, west, north, south, up and
/// down. The water's shader holds the same table.
pub const WAYS: [(Vec3, Vec3); 6] = [
    (Vec3::X, Vec3::Z),
    (Vec3::NEG_X, Vec3::Z),
    (Vec3::Y, Vec3::Z),
    (Vec3::NEG_Y, Vec3::Z),
    (Vec3::Z, Vec3::Y),
    (Vec3::NEG_Z, Vec3::Y),
];

/// The sky all round the eye: its six pictures and the constants each is drawn with.
#[derive(Debug)]
pub struct SkyProbe {
    faces: Vec<(Texture, u32)>,
    format: Option<Format>,
    /// The frame's constants seen each way, and the sky's.
    frames: Vec<PerFrame>,
    skies: Vec<PerFrame>,
}

/// One frame's sky all round the eye.
#[derive(Debug, Clone, Copy)]
pub struct SkyProbePlan {
    /// Each way's picture, and its index in the table.
    pub faces: [(Texture, u32); 6],
    /// Each way's frame's constants, and its sky's, where the sky's are drawn.
    pub frames: [u32; 6],
    pub skies: Option<[u32; 6]>,
}

/// The camera looking the `k`th of [`WAYS`] from `eye`, a quarter turn wide, its near and far
/// planes `near` and `far`.
#[must_use]
pub fn way(k: usize, eye: Vec3, near: f32, far: f32) -> Camera {
    let (forward, up) = WAYS[k];
    Camera {
        view: Mat4::look_to_lh(eye, forward, up),
        projection: Mat4::perspective_lh(std::f32::consts::FRAC_PI_2, 1.0, near, far),
        previous_view_projection: None,
        eye,
        viewport: [0, 0, SIDE, SIDE],
        fov_y: std::f32::consts::FRAC_PI_2,
        aspect: 1.0,
        near,
        far,
    }
}

/// `rows` with the float row `at` replaced by `v`.
fn put(rows: &mut [u8], at: usize, v: Vec4) {
    for (k, f) in v.to_array().iter().enumerate() {
        let o = at * 16 + k * 4;
        if o + 4 <= rows.len() {
            rows[o..o + 4].copy_from_slice(&f.to_le_bytes());
        }
    }
}

/// `rows` with the matrix starting at row `at` replaced by `m`, by its columns.
fn put_mat(rows: &mut [u8], at: usize, m: Mat4) {
    for (k, c) in m.to_cols_array_2d().iter().enumerate() {
        put(rows, at + k, Vec4::from_array(*c));
    }
}

impl SkyProbe {
    /// The probe, holding no picture until it is first drawn.
    #[must_use]
    pub fn new() -> Self {
        Self {
            faces: Vec::new(),
            format: None,
            frames: Vec::new(),
            skies: Vec::new(),
        }
    }

    /// The frame's six views of the sky: their pictures in `format`, made as they are first
    /// wanted, and their constants, written from the frame's sky frame constants `frame_rows`
    /// (seen by `camera`, with the sky's far plane `far`) and the sky's own constants `sky_rows`,
    /// where the sky's are drawn.
    ///
    /// # Errors
    /// What the device refuses.
    pub(crate) fn prepare(
        &mut self,
        gfx: &mut Device,
        format: Format,
        camera: &Camera,
        far: f32,
        frame_rows: &[u8],
        sky_rows: Option<&[u8]>,
    ) -> Result<SkyProbePlan, dereth_gfx::Error> {
        if self.format != Some(format) {
            for (t, _) in self.faces.drain(..) {
                gfx.destroy_texture(t);
            }
            for _ in 0..6 {
                let t = gfx.create_texture(&TextureDesc {
                    label: "remaster sky probe",
                    width: SIDE,
                    height: SIDE,
                    mips: 1,
                    format,
                    usage: TextureUsage::COLOR_TARGET | TextureUsage::SAMPLED,
                })?;
                let i = gfx.bind_texture(t)?;
                self.faces.push((t, i));
            }
            self.format = Some(format);
        }
        while self.frames.len() < 6 {
            self.frames.push(PerFrame::new(
                gfx,
                "remaster sky probe frame",
                512,
                BufferUsage::default(),
            )?);
            self.skies.push(PerFrame::new(
                gfx,
                "remaster sky probe sky",
                512,
                BufferUsage::default(),
            )?);
        }
        #[allow(clippy::cast_precision_loss)] // a picture's side
        let side = SIDE as f32;
        let mut frames = [0; 6];
        let mut skies = [0; 6];
        for k in 0..6 {
            let c = way(k, camera.eye, camera.near, far);
            let view_projection = c.projection * c.view;
            // The frame's constants: the view and projection together, the view, their inverse
            // and the target's extent (`retail.slang`'s rows 0, 4, 16 and 20).
            let mut rows = frame_rows.to_vec();
            put_mat(&mut rows, 0, view_projection);
            put_mat(&mut rows, 4, c.view);
            put_mat(&mut rows, 16, view_projection.inverse());
            put(&mut rows, 20, Vec4::new(side, side, 0.0, 0.0));
            self.frames[k].write(gfx, &rows)?;
            frames[k] = self.frames[k].index;
            if let Some(sky) = sky_rows {
                // The sky's: the way from a pixel to its direction and the target's extent
                // (`atmosphere.slang`'s rows 0 and 5).
                let mut rows = sky.to_vec();
                put_mat(&mut rows, 0, super::sky::ray_from_ndc(&c));
                put(&mut rows, 5, Vec4::new(side, side, 0.0, 0.0));
                self.skies[k].write(gfx, &rows)?;
                skies[k] = self.skies[k].index;
            }
        }
        let faces = std::array::from_fn(|k| self.faces[k]);
        Ok(SkyProbePlan {
            faces,
            frames,
            skies: sky_rows.map(|_| skies),
        })
    }
}

impl Default for SkyProbe {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    //! Behaviour: none (experimental)
    use super::*;

    /// Each way's picture looks its way: the way itself lands in the picture's middle, and the
    /// way up in it lands above the middle.
    #[test]
    fn each_way_looks_its_way_with_its_up_at_the_top() {
        for (k, &(forward, up)) in WAYS.iter().enumerate() {
            let c = way(k, Vec3::new(10.0, 20.0, 30.0), 0.1, 1000.0);
            let vp = c.projection * c.view;
            let at = |d: Vec3| {
                let p = vp * (c.eye + d * 50.0).extend(1.0);
                p.truncate() / p.w
            };
            let middle = at(forward);
            assert!(
                middle.x.abs() < 1e-4 && middle.y.abs() < 1e-4,
                "{k}: {middle}"
            );
            let above = at(forward + up * 0.5);
            assert!(above.y > 0.1 && above.x.abs() < 1e-4, "{k}: {above}");
        }
    }
}
