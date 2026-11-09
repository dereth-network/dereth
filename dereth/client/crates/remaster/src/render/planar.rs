//! The water's reflection drawn as a picture of its own, where the reflections are not traced: the
//! scene seen mirrored in the level of the water nearest the eye, at half the frame's size and cut
//! off at that level, its ground and meshes drawn into a G-buffer of its own, its sky drawn and the
//! two resolved as the frame's own are, by retail's light alone (no shadows, occlusion or lamps).
//! The water reads it where its surface lies at that level, so what it reflects there is what
//! stands over the water, whether the camera sees it or not.
//!
//! The mirrored view is turned left for right as well, so its triangles keep the turn the frame's
//! pipelines cull by; the water reads it turned back.

use dereth_gfx::{Device, Format, TextureDesc, TextureUsage};
use glam::{Mat4, Vec3, Vec4};

use super::{PerFrame, Targets, ALBEDO, DEPTH, LIGHT, MATERIAL, NORMAL};
use crate::scene::{Camera, FrameDelta};

/// How far from the eye the water is looked for, meters.
const WATER_WITHIN: f32 = 300.0;
/// The water's terrain types.
const WATER_TYPES: std::ops::RangeInclusive<u16> = 16..=20;
/// The landscape's vertices' spacing, meters.
const CELL: f32 = 24.0;

/// The mirrored view's picture and its constants.
#[derive(Debug, Default)]
pub struct PlanarReflection {
    targets: Option<(Targets, Format)>,
    frame: Option<PerFrame>,
    sky: Option<PerFrame>,
    atmosphere: Option<PerFrame>,
}

/// One frame's mirrored view.
#[derive(Debug, Clone, Copy)]
pub struct PlanarPlan {
    /// Its targets, the color the water reads.
    pub targets: Targets,
    /// Its frame's constants, its sky's, and its atmosphere's where the sky's tables are drawn.
    pub frame: u32,
    pub sky: u32,
    pub atmosphere: Option<u32>,
    /// The water's level it is mirrored in, and its view and projection together.
    pub level: f32,
    pub view_projection: Mat4,
}

/// The level of the water nearest `frame`'s eye, within [`WATER_WITHIN`]: retail's height at
/// the nearest landscape vertex of a water type. `None` with no drawn ground or no water near.
#[must_use]
pub fn water_level(frame: &FrameDelta) -> Option<f32> {
    let drawn = frame.drawn.as_ref()?;
    let world = &drawn.world;
    let side = i32::try_from(world.side).ok()?;
    if world.heights.len() < (world.side * world.side) as usize {
        return None;
    }
    let eye = frame.camera.eye;
    let (ex, ey) = (eye.x + drawn.global.x, eye.y + drawn.global.y);
    let reach = dereth_primitives::num::floor_to_i32(WATER_WITHIN / CELL) + 1;
    let (cx, cy) = (
        dereth_primitives::num::floor_to_i32(ex / CELL),
        dereth_primitives::num::floor_to_i32(ey / CELL),
    );
    let mut best: Option<(f32, f32)> = None;
    for gy in (cy - reach).max(0)..=(cy + reach).min(side - 1) {
        for gx in (cx - reach).max(0)..=(cx + reach).min(side - 1) {
            #[allow(clippy::cast_sign_loss)] // clamped to the world above
            let k = (gy * side + gx) as usize;
            if !WATER_TYPES.contains(&((world.words[k] >> 2) & 0x1F)) {
                continue;
            }
            #[allow(clippy::cast_precision_loss)] // vertex indices
            let (x, y) = (gx as f32 * CELL, gy as f32 * CELL);
            let d = ((x - ex) * (x - ex) + (y - ey) * (y - ey)).sqrt();
            if d <= WATER_WITHIN && best.is_none_or(|b| d < b.0) {
                best = Some((d, world.heights[k]));
            }
        }
    }
    best.map(|b| b.1)
}

/// The view of `camera` mirrored in the level `level`, turned left for right, and its
/// projection, which cuts away what lies below the level; with the projection the sky is drawn
/// through, which cuts nothing away, and the mirrored eye.
#[must_use]
pub fn mirrored(camera: &Camera, level: f32, sky_projection: Mat4) -> (Mat4, Mat4, Mat4, Vec3) {
    // Mirror the world in the level: (x, y, z) to (x, y, 2 level - z).
    let mirror = Mat4::from_cols(
        Vec4::X,
        Vec4::Y,
        Vec4::NEG_Z,
        Vec4::new(0.0, 0.0, 2.0 * level, 1.0),
    );
    let view = camera.view * mirror;
    let flip = Mat4::from_scale(Vec3::new(-1.0, 1.0, 1.0));
    let projection = flip * camera.projection;
    let sky = flip * sky_projection;
    // The level as a plane in the eye's space, its kept side (over the water) positive.
    let plane = view.inverse().transpose() * Vec4::new(0.0, 0.0, 1.0, -level);
    let cut = oblique(projection, plane);
    let eye = Vec3::new(camera.eye.x, camera.eye.y, 2.0 * level - camera.eye.z);
    (view, cut, sky, eye)
}

/// `projection` (depth 0 to 1) with its near plane moved onto `plane` (in the eye's space, its
/// kept side positive), so what lies on the other side is cut away; the far plane is kept as near
/// as it can be.
fn oblique(projection: Mat4, plane: Vec4) -> Mat4 {
    let rows = projection.transpose();
    let sign = |v: f32| if v >= 0.0 { 1.0 } else { -1.0 };
    let corner = projection.inverse() * Vec4::new(sign(plane.x), sign(plane.y), 1.0, 1.0);
    let scaled = plane * (1.0 / plane.dot(corner));
    Mat4::from_cols(rows.x_axis, rows.y_axis, scaled, rows.w_axis).transpose()
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

impl PlanarReflection {
    /// The mirrored view for the frame `frame` drawn at `size` into colors of `format`, from the
    /// frame's constants `frame_rows`, its sky's `sky_rows` (seen through `sky_projection`) and
    /// its atmosphere's `atmosphere_rows`; `None` with no water near the eye.
    ///
    /// # Errors
    /// What the device refuses.
    #[allow(clippy::too_many_arguments)]
    pub(crate) fn prepare(
        &mut self,
        gfx: &mut Device,
        frame: &FrameDelta,
        size: (u32, u32),
        format: Format,
        frame_rows: &[u8],
        sky_rows: &[u8],
        sky_projection: Mat4,
        atmosphere_rows: Option<&[u8]>,
    ) -> Result<Option<PlanarPlan>, dereth_gfx::Error> {
        let Some(level) = water_level(frame) else {
            return Ok(None);
        };
        let (w, h) = ((size.0 / 2).max(1), (size.1 / 2).max(1));
        let targets = self.targets_for(gfx, w, h, format)?;
        let camera = &frame.camera;
        let (view, projection, sky, eye) = mirrored(camera, level, sky_projection);
        let view_projection = projection * view;
        #[allow(clippy::cast_precision_loss)] // a target's size
        let extent = Vec4::new(w as f32, h as f32, 0.0, 0.0);
        // The frame's constants seen mirrored: rows 0, 4, 8, 16 and 20 of `retail.slang`.
        let mut rows = frame_rows.to_vec();
        put_mat(&mut rows, 0, view_projection);
        put_mat(&mut rows, 4, view);
        put(&mut rows, 8, eye.extend(1.0));
        put_mat(&mut rows, 16, view_projection.inverse());
        put(&mut rows, 20, extent);
        let mut sky_frame = sky_rows.to_vec();
        put_mat(&mut sky_frame, 0, sky * view);
        put_mat(&mut sky_frame, 4, view);
        put(&mut sky_frame, 8, eye.extend(1.0));
        put_mat(&mut sky_frame, 16, (sky * view).inverse());
        put(&mut sky_frame, 20, extent);
        let mut write = |slot: &mut Option<PerFrame>, label, bytes: &[u8]| {
            let b = match slot {
                Some(b) => b,
                None => slot.insert(PerFrame::new(
                    gfx,
                    label,
                    1024,
                    dereth_gfx::BufferUsage::default(),
                )?),
            };
            b.write(gfx, bytes)?;
            Ok::<u32, dereth_gfx::Error>(b.index)
        };
        let frame_index = write(&mut self.frame, "remaster planar frame", &rows)?;
        let sky_index = write(&mut self.sky, "remaster planar sky frame", &sky_frame)?;
        let atmosphere = match atmosphere_rows {
            Some(a) => {
                let seen = Camera {
                    view,
                    projection: sky,
                    previous_view_projection: None,
                    eye,
                    viewport: [0, 0, w, h],
                    fov_y: camera.fov_y,
                    aspect: camera.aspect,
                    near: camera.near,
                    far: camera.far,
                };
                let mut a = a.to_vec();
                put_mat(&mut a, 0, super::sky::ray_from_ndc(&seen));
                put(&mut a, 5, extent);
                Some(write(
                    &mut self.atmosphere,
                    "remaster planar atmosphere",
                    &a,
                )?)
            }
            None => None,
        };
        Ok(Some(PlanarPlan {
            targets,
            frame: frame_index,
            sky: sky_index,
            atmosphere,
            level,
            view_projection,
        }))
    }

    /// The targets at `width` by `height`, colors in `format`, made again when either changes.
    fn targets_for(
        &mut self,
        gfx: &mut Device,
        width: u32,
        height: u32,
        format: Format,
    ) -> Result<Targets, dereth_gfx::Error> {
        if let Some((t, f)) = self.targets {
            if (t.width, t.height, f) == (width, height, format) {
                return Ok(t);
            }
            for x in [t.albedo, t.normal, t.light, t.material, t.depth, t.color] {
                gfx.destroy_texture(x);
            }
        }
        let mut make = |label, format, usage| {
            gfx.create_texture(&TextureDesc {
                label,
                width,
                height,
                mips: 1,
                format,
                usage,
            })
        };
        let target = TextureUsage::COLOR_TARGET | TextureUsage::SAMPLED;
        let albedo = make("remaster planar albedo", ALBEDO, target)?;
        let normal = make("remaster planar normal", NORMAL, target)?;
        let light = make("remaster planar light", LIGHT, target)?;
        let material = make("remaster planar material", MATERIAL, target)?;
        let depth = make(
            "remaster planar depth",
            DEPTH,
            TextureUsage::DEPTH_TARGET | TextureUsage::SAMPLED,
        )?;
        let color = make("remaster planar color", format, target)?;
        let mut index = [0; 7];
        for (k, t) in [albedo, normal, light, material, depth, color]
            .into_iter()
            .enumerate()
        {
            index[k] = gfx.bind_texture(t)?;
        }
        index[6] = index[4];
        let t = Targets {
            width,
            height,
            albedo,
            normal,
            light,
            material,
            depth,
            color,
            outdoor_depth: depth,
            index,
        };
        self.targets = Some((t, format));
        Ok(t)
    }
}

#[cfg(test)]
mod tests {
    //! Behaviour: none (experimental)
    use super::*;

    fn camera() -> Camera {
        let eye = Vec3::new(0.0, 0.0, 12.0);
        Camera {
            view: Mat4::look_to_lh(eye, Vec3::new(0.0, 1.0, -0.3).normalize(), Vec3::Z),
            projection: Mat4::perspective_lh(1.0, 1.5, 0.5, 1000.0),
            previous_view_projection: None,
            eye,
            viewport: [0, 0, 640, 480],
            fov_y: 1.0,
            aspect: 1.5,
            near: 0.5,
            far: 1000.0,
        }
    }

    fn ndc(m: Mat4, p: Vec3) -> Vec3 {
        let c = m * p.extend(1.0);
        c.truncate() / c.w
    }

    /// What stands over the water is seen where its mirror image stands, left for right; what
    /// lies under the water is cut away; and the water's own level lands where the frame sees it,
    /// left for right.
    #[test]
    fn the_mirrored_view_sees_what_stands_over_the_water_and_cuts_what_lies_under_it() {
        let c = camera();
        let level = 10.0;
        let (view, projection, _, eye) = mirrored(&c, level, c.projection);
        assert!((eye.z - 8.0).abs() < 1e-5);
        let vp = projection * view;
        let frame = c.projection * c.view;
        // A point on the water: where the frame sees it, turned left for right.
        let on = Vec3::new(3.0, 40.0, level);
        let (a, b) = (ndc(vp, on), ndc(frame, on));
        assert!(
            (a.x + b.x).abs() < 1e-3 && (a.y - b.y).abs() < 1e-3,
            "{a} {b}"
        );
        // A treetop over the water stands in front of the near plane; the bed under it, behind.
        let over = ndc(vp, Vec3::new(3.0, 40.0, level + 6.0));
        assert!((0.0..=1.0).contains(&over.z), "{over}");
        let under = ndc(vp, Vec3::new(3.0, 40.0, level - 2.0));
        assert!(!(0.0..=1.0).contains(&under.z), "{under}");
    }
}
