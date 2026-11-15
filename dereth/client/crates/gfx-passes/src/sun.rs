//! The sun's shadows' shared part: four cascades around the camera's view, looking down the sun,
//! their maps, and the pass that works out for each pixel the sun lights how much of the sun
//! reaches it. What casts into the maps is the game's own: it draws its casters into each
//! cascade's map (or its cache, which [`SunMaps`] keeps beside each map for a game that draws what
//! stands still once) with the cascade's view and projection, then asks for the sun's share.
//!
//! - **The cascades** ([`cascades`]). Each covers a slice of the view, the sphere round the slice
//!   seen down the sun. It is placed in whole texels of its own map and its depth range in whole
//!   steps, so it moves only when the view moves it by a texel; [`CascadeKey`] is what places it.
//! - **The sun's share** ([`MaskPlan::record`]). Each pixel lit by the sun (the ground and what
//!   stands in the sun) is looked up in the first cascade that covers it, offset along its normal
//!   by a texel and compared against the casters with a 4 x 4 tent filter, blended into the next
//!   cascade over the last fifth of each cascade, and faded out toward the shadows' reach. The
//!   shadows fade in as the sun rises off the horizon ([`sun_of`]).
//!
//! Its shader is `shaders/shadow_mask.slang`; its pipeline is made with it ([`SunMaps::new`]) and
//! never after.

use dereth_gfx::{
    BufferUsage, ColorAttachment, ColorTarget, Cull, Device, Format, GraphicsDesc, Load, Pipeline,
    RasterPass, Recorder, Rect, Texture, TextureDesc, TextureUsage, Touch, Use,
};
use glam::{Mat4, Vec3, Vec4, Vec4Swizzles};

use crate::frame::{Camera, PerFrame};
use crate::gbuffer::Targets;
use crate::shaders::{shader, Entry};

/// The cascades.
pub const CASCADES: usize = 4;

/// The far end of each cascade, metres along the view.
pub const SPLITS: [f32; CASCADES] = [14.0, 45.0, 150.0, 560.0];

/// How far toward the sun casters are looked for beyond a cascade's own sphere, metres.
const REACH: f32 = 900.0;

/// The steps a cascade's depth range moves in, metres.
const DEPTH_STEP: f32 = 64.0;

/// Where the shadows start to fade out, as a share of the last cascade's reach.
const FADE_START: f32 = 0.85;

/// The sun's height (the sine of its elevation) below which there are no shadows, and above which
/// they are at full strength.
const SUN_LOW: f32 = 0.02;
const SUN_HIGH: f32 = 0.12;

/// The depth the shadow maps hold.
pub const DEPTH: Format = Format::D32Float;

/// The format of the sun's share.
pub const VISIBILITY: Format = Format::R8Unorm;

/// What places one cascade: when this is the same, the cascade's cached casters are the same.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct CascadeKey {
    /// The cascade's centre across the sun, in its own texels.
    pub centre: [i64; 2],
    /// Its radius, and its depth range in steps toward and away from the sun.
    pub radius: u32,
    pub depth: [i64; 2],
    /// The sun's direction.
    pub sun: [u32; 3],
}

/// One cascade.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Cascade {
    /// From the frame's space to the map's clip space.
    pub view_proj: Mat4,
    /// From the frame's space to the sun's: x and y across the sun, z back toward it.
    pub light_view: Mat4,
    /// The box it covers in the sun's space: across the sun, and the distances along it.
    pub across: [f32; 4],
    pub along: [f32; 2],
    /// One texel's size, metres.
    pub texel: f32,
    /// Its far end along the view, metres.
    pub far: f32,
    pub key: CascadeKey,
}

impl Cascade {
    /// Whether a sphere in the frame's space reaches into the cascade's box.
    #[must_use]
    pub fn reaches(&self, centre: Vec3, radius: f32) -> bool {
        let q = self.light_view * centre.extend(1.0);
        let along = -q.z;
        q.x + radius >= self.across[0]
            && q.x - radius <= self.across[1]
            && q.y + radius >= self.across[2]
            && q.y - radius <= self.across[3]
            && along + radius >= self.along[0]
            && along - radius <= self.along[1]
    }

    /// Whether a box in the frame's space reaches into the cascade's box.
    #[must_use]
    pub fn reaches_box(&self, min: Vec3, max: Vec3) -> bool {
        let mut lo = Vec3::splat(f32::INFINITY);
        let mut hi = Vec3::splat(f32::NEG_INFINITY);
        for k in 0..8 {
            let p = Vec3::new(
                if k & 1 == 0 { min.x } else { max.x },
                if k & 2 == 0 { min.y } else { max.y },
                if k & 4 == 0 { min.z } else { max.z },
            );
            let q = (self.light_view * p.extend(1.0)).xyz();
            lo = lo.min(Vec3::new(q.x, q.y, -q.z));
            hi = hi.max(Vec3::new(q.x, q.y, -q.z));
        }
        hi.x >= self.across[0]
            && lo.x <= self.across[1]
            && hi.y >= self.across[2]
            && lo.y <= self.across[3]
            && hi.z >= self.along[0]
            && lo.z <= self.along[1]
    }
}

/// The sun's direction toward it, unit length, and how strong the shadows are: `None` when there
/// is no sun to cast them.
#[must_use]
pub fn sun_of(sun: Vec3) -> Option<(Vec3, f32)> {
    if sun.length_squared() < 1e-8 {
        return None;
    }
    let l = sun.normalize();
    let strength = ((l.z - SUN_LOW) / (SUN_HIGH - SUN_LOW)).clamp(0.0, 1.0);
    (strength > 0.0).then_some((l, strength))
}

/// The four cascades around `camera`'s view, looking down the sun from `sun` (toward the sun,
/// unit length), for maps `size` texels a side.
#[must_use]
#[allow(clippy::cast_possible_truncation)] // texel counts and depth steps are small
pub fn cascades(camera: &Camera, sun: Vec3, size: u32) -> [Cascade; CASCADES] {
    let inv = (camera.projection * camera.view).inverse();
    let un = |x: f32, y: f32| {
        let p = inv * Vec4::new(x, y, 0.5, 1.0);
        p.xyz() / p.w
    };
    let eye = camera.eye;
    let forward = (un(0.0, 0.0) - eye).normalize_or(Vec3::Y);
    let rays: Vec<Vec3> = [(-1.0, -1.0), (1.0, -1.0), (1.0, 1.0), (-1.0, 1.0)]
        .iter()
        .map(|(x, y)| un(*x, *y) - eye)
        .collect();
    let up = if sun.z.abs() > 0.99 { Vec3::Y } else { Vec3::Z };
    let light_view = Mat4::look_at_rh(Vec3::ZERO, -sun, up);
    let sun_bits = sun.to_array().map(f32::to_bits);
    let mut near = camera.near.max(0.05);
    let mut out = [Cascade {
        view_proj: Mat4::IDENTITY,
        light_view,
        across: [0.0; 4],
        along: [0.0; 2],
        texel: 1.0,
        far: 0.0,
        key: CascadeKey {
            centre: [0; 2],
            radius: 0,
            depth: [0; 2],
            sun: sun_bits,
        },
    }; CASCADES];
    for (c, cascade) in out.iter_mut().enumerate() {
        let far = SPLITS[c].max(near + 1.0);
        let mut corners = Vec::with_capacity(8);
        for d in [near, far] {
            for r in &rays {
                corners.push(eye + *r * (d / r.dot(forward).max(1e-3)));
            }
        }
        let centre = corners.iter().copied().sum::<Vec3>() / 8.0;
        let radius = corners
            .iter()
            .map(|p| (*p - centre).length())
            .fold(0.0f32, f32::max);
        // Whole sixteenths of a metre, so the radius holds still as the view turns.
        let radius = (radius * 16.0).ceil() / 16.0;
        #[allow(clippy::cast_precision_loss)] // a map's side
        let texel = 2.0 * radius / size as f32;
        let q = light_view * centre.extend(1.0);
        let cx = (q.x / texel).floor();
        let cy = (q.y / texel).floor();
        let (x, y) = (cx * texel, cy * texel);
        let near_step = ((-q.z - radius - REACH) / DEPTH_STEP).floor();
        let far_step = ((-q.z + radius) / DEPTH_STEP).ceil();
        let (z_near, z_far) = (near_step * DEPTH_STEP, far_step * DEPTH_STEP);
        let proj = Mat4::orthographic_rh(
            x - radius,
            x + radius,
            y - radius,
            y + radius,
            z_near,
            z_far,
        );
        *cascade = Cascade {
            view_proj: proj * light_view,
            light_view,
            across: [x - radius, x + radius, y - radius, y + radius],
            along: [z_near, z_far],
            texel,
            far,
            key: CascadeKey {
                centre: [cx as i64, cy as i64],
                radius: radius.to_bits(),
                depth: [near_step as i64, far_step as i64],
                sun: sun_bits,
            },
        };
        near = far;
    }
    out
}

/// The six planes of a view and projection's frustum, each facing in, as `(normal, distance)` with
/// a unit normal: a point `p` is inside when `plane.dot(p.extend(1))` is at least 0 for every one.
/// The depth runs from 0 to 1, as the device's does.
#[must_use]
pub fn frustum(m: Mat4) -> [Vec4; 6] {
    let r = |i: usize| m.row(i);
    [
        r(3) + r(0),
        r(3) - r(0),
        r(3) + r(1),
        r(3) - r(1),
        r(2),
        r(3) - r(2),
    ]
    .map(|p| p / p.truncate().length().max(1e-12))
}

/// One cascade's map, which the frame samples, its index in the table, and the cache beside it a
/// game can keep what stands still in and copy into the map each frame.
#[derive(Debug, Clone, Copy)]
pub struct CascadeMap {
    pub map: Texture,
    pub map_index: u32,
    pub cache: Texture,
}

/// The cascades' maps, the sun's share and the pass between them.
#[derive(Debug)]
pub struct SunMaps {
    mask: Pipeline,
    maps: Vec<CascadeMap>,
    size: u32,
    constants: PerFrame,
    visibility: Option<(Texture, u32, u32, u32)>,
}

/// The sun's share's part of a frame: what [`MaskPlan::record`] reads and writes.
#[derive(Debug, Clone)]
pub struct MaskPlan {
    mask: Pipeline,
    /// The cascades' constants' index in the table.
    pub constants: u32,
    /// The cascades' maps.
    pub maps: Vec<Texture>,
    /// The sun's share, and its index in the table.
    pub visibility: Texture,
    pub visibility_index: u32,
}

fn words(w: &[u32]) -> Vec<u8> {
    w.iter().flat_map(|x| x.to_le_bytes()).collect()
}

fn floats(f: &[f32]) -> Vec<u8> {
    f.iter().flat_map(|x| x.to_le_bytes()).collect()
}

impl SunMaps {
    /// No maps yet, the pass's pipeline made now.
    ///
    /// # Errors
    /// What the device refuses.
    pub fn new(d: &mut Device) -> Result<Self, dereth_gfx::Error> {
        let mask = d.create_graphics_pipeline(&GraphicsDesc {
            label: "remaster sun visibility",
            vertex: crate::shaders::fullscreen(),
            fragment: Some(shader(Entry::ShadowMaskFsMain)),
            color: &[ColorTarget {
                format: VISIBILITY,
                blend: None,
                write_alpha: true,
            }],
            depth: None,
            cull: Cull::None,
        })?;
        Ok(Self {
            mask,
            maps: Vec::new(),
            size: 0,
            constants: PerFrame::new(d, "remaster shadow cascades", 512, BufferUsage::default())?,
            visibility: None,
        })
    }

    /// The cascades' maps, `size` texels a side (at least 16), made now when they are not that
    /// size already. Whether they were made: a game's cached casters are then gone.
    ///
    /// # Errors
    /// What the device refuses.
    pub fn ensure(&mut self, d: &mut Device, size: u32) -> Result<bool, dereth_gfx::Error> {
        let size = size.max(16);
        if self.size == size && !self.maps.is_empty() {
            return Ok(false);
        }
        for c in self.maps.drain(..) {
            d.destroy_texture(c.map);
            d.destroy_texture(c.cache);
        }
        for _ in 0..CASCADES {
            let map = d.create_texture(&TextureDesc {
                label: "remaster shadow map",
                width: size,
                height: size,
                mips: 1,
                format: DEPTH,
                usage: TextureUsage::DEPTH_TARGET
                    | TextureUsage::SAMPLED
                    | TextureUsage::TRANSFER_DST,
            })?;
            let cache = d.create_texture(&TextureDesc {
                label: "remaster shadow cache",
                width: size,
                height: size,
                mips: 1,
                format: DEPTH,
                usage: TextureUsage::DEPTH_TARGET | TextureUsage::TRANSFER_SRC,
            })?;
            let map_index = d.bind_texture(map)?;
            self.maps.push(CascadeMap {
                map,
                map_index,
                cache,
            });
        }
        self.size = size;
        Ok(true)
    }

    /// The cascades' maps, as last made.
    #[must_use]
    pub fn maps(&self) -> &[CascadeMap] {
        &self.maps
    }

    /// Each map's side, texels.
    #[must_use]
    pub fn size(&self) -> u32 {
        self.size
    }

    /// The sun's share, at the world's size.
    fn visibility(
        &mut self,
        d: &mut Device,
        width: u32,
        height: u32,
    ) -> Result<(Texture, u32), dereth_gfx::Error> {
        if let Some((t, i, w, h)) = self.visibility {
            if (w, h) == (width, height) {
                return Ok((t, i));
            }
            d.destroy_texture(t);
        }
        let t = d.create_texture(&TextureDesc {
            label: "remaster sun visibility",
            width,
            height,
            mips: 1,
            format: VISIBILITY,
            usage: TextureUsage::COLOR_TARGET | TextureUsage::SAMPLED | TextureUsage::TRANSFER_SRC,
        })?;
        let i = d.bind_texture(t)?;
        self.visibility = Some((t, i, width, height));
        Ok((t, i))
    }

    /// The sun's share of the last frame the shadows drew, a byte a pixel, after the device has
    /// finished it.
    ///
    /// # Errors
    /// What the device refuses.
    pub fn read_visibility(
        &mut self,
        d: &mut Device,
    ) -> Result<Option<Vec<u8>>, dereth_gfx::Error> {
        let Some((t, ..)) = self.visibility else {
            return Ok(None);
        };
        d.wait_idle()?;
        d.read_texture(t, 0).map(Some)
    }

    /// Write the frame's cascades' constants for the sun's share, `strength` how strong the
    /// shadows are ([`sun_of`]), with the sun's share at `area` (the world's size). The maps must
    /// have been made ([`Self::ensure`]). Returns the frame's part and the bytes written.
    ///
    /// # Errors
    /// What the device refuses.
    pub fn prepare(
        &mut self,
        d: &mut Device,
        cascades: &[Cascade; CASCADES],
        strength: f32,
        area: (u32, u32),
    ) -> Result<(MaskPlan, u64), dereth_gfx::Error> {
        let (visibility, visibility_index) = self.visibility(d, area.0, area.1)?;
        let mut bytes = Vec::new();
        for c in cascades {
            bytes.extend(floats(&c.view_proj.to_cols_array()));
        }
        bytes.extend(floats(&cascades.map(|c| c.far)));
        bytes.extend(floats(&cascades.map(|c| c.texel)));
        let maps: Vec<u32> = self.maps.iter().map(|c| c.map_index).collect();
        bytes.extend(words(&maps));
        let reach = SPLITS[CASCADES - 1];
        #[allow(clippy::cast_precision_loss)] // a map's side
        bytes.extend(floats(&[
            reach * FADE_START,
            reach,
            strength,
            self.size as f32,
        ]));
        bytes.extend(floats(
            &cascades.map(|c| 1.0 / (c.along[1] - c.along[0]).max(1e-3)),
        ));
        self.constants.write(d, &bytes)?;
        Ok((
            MaskPlan {
                mask: self.mask,
                constants: self.constants.index,
                maps: self.maps.iter().map(|c| c.map).collect(),
                visibility,
                visibility_index,
            },
            bytes.len() as u64,
        ))
    }
}

impl MaskPlan {
    /// Work out the sun's share at each pixel of the world, from the G-buffer and the maps.
    /// `frame` is the frame's constants' index; `depth` the depth the outdoor surfaces stand at.
    ///
    /// # Errors
    /// What the layer refuses.
    pub fn record(
        &self,
        r: &mut Recorder<'_>,
        t: &Targets,
        frame: u32,
        depth: (Texture, u32),
    ) -> Result<(), dereth_gfx::Error> {
        let mut touches = vec![
            Touch::Texture(depth.0, Use::Sampled),
            Touch::Texture(t.normal, Use::Sampled),
            Touch::Texture(t.material, Use::Sampled),
        ];
        touches.extend(self.maps.iter().map(|m| Touch::Texture(*m, Use::Sampled)));
        r.timestamp("sun visibility");
        r.raster(
            &RasterPass {
                name: "sun visibility",
                color: &[ColorAttachment {
                    texture: self.visibility,
                    load: Load::Discard,
                }],
                depth: None,
                touches: &touches,
                area: Some(Rect {
                    x: 0,
                    y: 0,
                    width: t.width,
                    height: t.height,
                }),
            },
            |d| {
                d.set_pipeline(self.mask)?;
                d.push(
                    0,
                    &words(&[
                        frame,
                        t.index[4],
                        depth.1,
                        t.index[1],
                        t.index[3],
                        self.constants,
                    ]),
                )?;
                d.draw(3, 1, 0, 0);
                Ok(())
            },
        )
    }
}

#[cfg(test)]
mod tests {
    //! Behaviour: none (experimental)
    use super::*;
    use dereth_primitives::num::math;

    fn camera(eye: Vec3, yaw: f32) -> Camera {
        // Looking along `yaw` (radians from north) and level, in the frame's space with z up,
        // through a left-handed projection with y up on the screen.
        let forward = Vec3::new(math::sinf(yaw), math::cosf(yaw), 0.0);
        let to_eye = Mat4::look_to_lh(eye, forward, Vec3::Z);
        Camera {
            view: to_eye,
            projection: Mat4::perspective_lh(1.0, 1.5, 0.5, 2000.0),
            eye,
            near: 0.5,
            far: 2000.0,
            ..Camera::default()
        }
    }

    const SUN: Vec3 = Vec3::new(0.3, -0.4, 0.866);

    /// A cascade holds still, and so does its cache, while the view moves less than one of its
    /// texels; it moves when the view crosses one.
    #[test]
    fn a_cascade_moves_only_when_the_view_moves_by_a_texel() {
        let sun = SUN.normalize();
        let a = cascades(&camera(Vec3::new(10.0, 20.0, 5.0), 0.3), sun, 2048);
        let b = cascades(&camera(Vec3::new(10.0, 20.0, 5.0), 0.3), sun, 2048);
        assert_eq!(
            a.map(|c| c.key),
            b.map(|c| c.key),
            "the same view, the same cascades"
        );
        // Far smaller than the last cascade's texel (about half a metre): it holds still.
        let c = cascades(&camera(Vec3::new(10.001, 20.0, 5.0), 0.3), sun, 2048);
        assert_eq!(a[3].key, c[3].key);
        // A metre: every cascade's texel is smaller, so each moves.
        let e = cascades(&camera(Vec3::new(11.0, 20.0, 5.0), 0.3), sun, 2048);
        for k in 0..CASCADES {
            assert_ne!(a[k].key.centre, e[k].key.centre, "cascade {k}");
        }
    }

    /// Turning on the spot keeps each cascade's size: only where it stands changes.
    #[test]
    fn turning_keeps_each_cascades_size() {
        let sun = SUN.normalize();
        let a = cascades(&camera(Vec3::new(0.0, 0.0, 5.0), 0.0), sun, 2048);
        let b = cascades(&camera(Vec3::new(0.0, 0.0, 5.0), 1.7), sun, 2048);
        for k in 0..CASCADES {
            assert_eq!(a[k].key.radius, b[k].key.radius, "cascade {k}");
            assert!((a[k].texel - b[k].texel).abs() < 1e-6);
        }
    }

    /// The camera's view at each cascade's far end lies inside the cascade, and a caster far
    /// toward the sun from it still reaches into the cascade's box.
    #[test]
    fn each_cascade_covers_its_slice_and_what_stands_toward_the_sun() {
        let sun = SUN.normalize();
        let cam = camera(Vec3::new(50.0, 60.0, 5.0), 0.8);
        let cs = cascades(&cam, sun, 2048);
        let forward = Vec3::new(math::sinf(0.8), math::cosf(0.8), 0.0);
        for c in &cs {
            let p = cam.eye + forward * (c.far * 0.99);
            assert!(c.reaches(p, 0.0), "far end at {}", c.far);
            assert!(
                c.reaches(p + sun * 300.0, 1.0),
                "a caster 300 m toward the sun"
            );
            let q = c.view_proj * p.extend(1.0);
            assert!((0.0..=1.0).contains(&q.z), "{}", q.z);
        }
        // Something far off across the sun is in none of the near ones.
        assert!(!cs[0].reaches(cam.eye + Vec3::new(500.0, 0.0, 0.0), 1.0));
    }

    /// No sun, or one at the horizon, casts no shadows; they come in as it rises.
    #[test]
    fn the_shadows_fade_in_as_the_sun_rises() {
        assert!(sun_of(Vec3::ZERO).is_none());
        assert!(sun_of(Vec3::new(1.0, 0.0, 0.0)).is_none());
        let (_, low) = sun_of(Vec3::new(1.0, 0.0, 0.07)).expect("a low sun");
        let (_, high) = sun_of(Vec3::new(0.2, 0.0, 1.0)).expect("a high sun");
        assert!(low > 0.0 && low < 1.0, "{low}");
        assert!((high - 1.0).abs() < 1e-6);
    }
}
