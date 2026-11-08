//! The sun's shadows: four cascades around the camera's view, looking down the sun, and a pass that
//! works out for each pixel the sun lights how much of the sun reaches it.
//!
//! - **Casters.** Every resident block's parts and ground, whether the walk drew them or not, and
//!   every object: what casts a shadow doesn't depend on what is in view, so walking or turning
//!   never makes a shadow appear or vanish. Each caster is drawn at its finest level, so a
//!   caster's shadow doesn't change shape as its drawn level does.
//! - **Cached cascades.** A cascade is placed in whole texels of its own map and its depth range
//!   in whole steps, so it moves only when the view moves it by a texel. The blocks' parts and
//!   ground are drawn into a cache once, and again only when the cascade moves, the sun moves,
//!   the frame's origin moves or what is resident changes. Each frame the cache is copied into
//!   the cascade's map and the objects drawn over it.
//! - **The sun's share.** Each pixel lit by the sun (the ground and what stands in the sun) is
//!   looked up in the first cascade that covers it, offset along its normal by a texel and
//!   compared against the casters with a 4 x 4 tent filter, blended into the next cascade over
//!   the last fifth of each cascade, and faded out toward the shadows' reach. The shadows fade in
//!   as the sun rises off the horizon.

use std::collections::{BTreeMap, BTreeSet};

use dereth_gfx::{
    ColorAttachment, ColorTarget, Compare, Cull, DepthAttachment, DepthState, Device, Format,
    GraphicsDesc, IndexType, Load, Pipeline, RasterPass, Recorder, Rect, Shader, Texture,
    TextureDesc, TextureUsage, Touch, Use,
};
use glam::{Mat4, Vec3, Vec4, Vec4Swizzles};

use super::scene::{class, Bucket, GpuScene, PartItems, NONE};
use super::{PerFrame, Targets};
use crate::scene::{BlockId, Camera, FrameDelta};
use crate::shaders::{spirv, Entry};

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
const DEPTH: Format = Format::D32Float;

/// The format of the sun's share.
const VISIBILITY: Format = Format::R8Unorm;

/// What casts the sun's shadows.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Casters {
    /// Everything resident: every block's parts and ground, and every object.
    Resident,
    /// Only what stands in the camera's view, drawn anew every frame: what shadows drawn from the
    /// view alone look like, kept to show the difference.
    InView,
}

/// The shadows' settings.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ShadowSettings {
    pub on: bool,
    /// Each cascade's map is this many texels a side.
    pub size: u32,
    pub casters: Casters,
}

impl Default for ShadowSettings {
    fn default() -> Self {
        Self {
            on: false,
            size: 2048,
            casters: Casters::Resident,
        }
    }
}

/// What the shadows counted.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct ShadowStats {
    /// Frames the shadows were drawn for.
    pub frames: u64,
    /// Cascades whose blocks' casters were drawn again, over all frames.
    pub cascades_redrawn: u64,
    /// The last frame's cascades drawn again, and the draw items they drew.
    pub last_redrawn: u32,
    pub last_static_items: u32,
    /// The last frame's ground blocks drawn into the caches.
    pub last_ground_blocks: u32,
    /// The last frame's object items drawn over the caches, over all cascades.
    pub last_dynamic_items: u32,
    /// The bytes the last frame's shadows wrote to the device.
    pub last_bytes: u64,
}

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

/// The pipelines the shadows draw with.
#[derive(Debug, Clone, Copy)]
struct Pipelines {
    caster: Pipeline,
    ground: Pipeline,
    mask: Pipeline,
}

/// One cascade's resources.
#[derive(Debug)]
struct CascadeGpu {
    /// The map the frame samples, and its index in the table.
    map: Texture,
    map_index: u32,
    /// The blocks' casters, kept between frames.
    cache: Texture,
    /// The frame's constants as the cascade sees it.
    frame: PerFrame,
    /// The blocks' parts' commands, and the ground blocks, as last drawn into the cache.
    statics: PerFrame,
    grounds: PerFrame,
    /// The objects' commands, each frame.
    dynamics: PerFrame,
}

/// What placed a cascade's cache last.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct CacheKey {
    cascade: CascadeKey,
    resident: u64,
    origin: [i32; 2],
}

/// The shadows' resources and state.
#[derive(Debug)]
pub struct ShadowPass {
    pub settings: ShadowSettings,
    pipelines: Pipelines,
    cascades: Vec<CascadeGpu>,
    size: u32,
    keys: [Option<CacheKey>; CASCADES],
    constants: PerFrame,
    visibility: Option<(Texture, u32, u32, u32)>,
    /// The objects' instances the shadows wrote, with the transform they wrote.
    written: BTreeMap<u32, Mat4>,
    pub stats: ShadowStats,
}

/// One cascade's part of a frame.
#[derive(Debug, Clone)]
struct CascadePlan {
    map: Texture,
    cache: Texture,
    frame: u32,
    redraw: bool,
    statics: (dereth_gfx::Buffer, u32),
    grounds: (u32, Vec<(u32, u32, u32)>),
    dynamics: (dereth_gfx::Buffer, u32),
}

/// What the shadows draw this frame.
#[derive(Debug, Clone)]
pub struct ShadowPlan {
    pipelines: Pipelines,
    cascades: Vec<CascadePlan>,
    size: u32,
    constants: u32,
    visibility: Texture,
    /// The sun's share's index in the table.
    pub visibility_index: u32,
    /// The bytes one indirect draw command takes on the device.
    command_bytes: u32,
}

fn shader(e: Entry) -> Shader<'static> {
    Shader {
        spirv: spirv(e),
        dxil: e.dxil_bytes(),
        entry: e.name(),
    }
}

fn words(w: &[u32]) -> Vec<u8> {
    w.iter().flat_map(|x| x.to_le_bytes()).collect()
}

fn floats(f: &[f32]) -> Vec<u8> {
    f.iter().flat_map(|x| x.to_le_bytes()).collect()
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

/// One block's offset from the frame's origin.
#[allow(clippy::cast_precision_loss)] // a few hundred blocks
fn block_offset(id: BlockId, origin: [i32; 2]) -> Vec3 {
    let (x, y) = (i32::from(id >> 8), i32::from(id & 0xFF));
    Vec3::new(
        (x - origin[0]) as f32 * 192.0,
        (y - origin[1]) as f32 * 192.0,
        0.0,
    )
}

/// A part's sphere in the frame's space: its own sphere placed by its transform, plus `offset`.
fn part_sphere(p: &PartItems, transform: Mat4, offset: Vec3) -> (Vec3, f32) {
    let centre = transform.transform_point3(Vec3::from(p.sphere.0)) + offset;
    let scale = transform
        .x_axis
        .xyz()
        .length()
        .max(transform.y_axis.xyz().length())
        .max(transform.z_axis.xyz().length());
    (centre, p.sphere.1 * scale)
}

impl ShadowPass {
    /// The shadows, off, their pipelines made now.
    ///
    /// # Errors
    /// What the device refuses.
    pub fn new(d: &mut Device) -> Result<Self, dereth_gfx::Error> {
        let depth = Some(DepthState {
            format: DEPTH,
            compare: Some(Compare::LessEqual),
            write: true,
        });
        let pipelines = Pipelines {
            caster: d.create_graphics_pipeline(&GraphicsDesc {
                label: "remaster shadow casters",
                vertex: shader(Entry::GbufferVsMain),
                fragment: Some(shader(Entry::GbufferFsCaster)),
                color: &[],
                depth,
                cull: Cull::None,
            })?,
            ground: d.create_graphics_pipeline(&GraphicsDesc {
                label: "remaster shadow ground",
                vertex: shader(Entry::TerrainVsMain),
                fragment: None,
                color: &[],
                depth,
                cull: Cull::None,
            })?,
            mask: d.create_graphics_pipeline(&GraphicsDesc {
                label: "remaster sun visibility",
                vertex: shader(Entry::FullscreenVsMain),
                fragment: Some(shader(Entry::ShadowMaskFsMain)),
                color: &[ColorTarget {
                    format: VISIBILITY,
                    blend: None,
                    write_alpha: true,
                }],
                depth: None,
                cull: Cull::None,
            })?,
        };
        Ok(Self {
            settings: ShadowSettings::default(),
            pipelines,
            cascades: Vec::new(),
            size: 0,
            keys: [None; CASCADES],
            constants: PerFrame::new(d, "remaster shadow cascades", 512, Default::default())?,
            visibility: None,
            written: BTreeMap::new(),
            stats: ShadowStats::default(),
        })
    }

    /// The cascades' maps, made at the settings' size on first use.
    fn maps(&mut self, d: &mut Device) -> Result<(), dereth_gfx::Error> {
        let size = self.settings.size.max(16);
        if self.size == size && !self.cascades.is_empty() {
            return Ok(());
        }
        for c in self.cascades.drain(..) {
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
            self.cascades.push(CascadeGpu {
                map,
                map_index,
                cache,
                frame: PerFrame::new(d, "remaster shadow frame", 512, Default::default())?,
                statics: PerFrame::new(
                    d,
                    "remaster shadow statics",
                    1 << 16,
                    dereth_gfx::BufferUsage::INDIRECT,
                )?,
                grounds: PerFrame::new(d, "remaster shadow ground", 1 << 12, Default::default())?,
                dynamics: PerFrame::new(
                    d,
                    "remaster shadow objects",
                    1 << 12,
                    dereth_gfx::BufferUsage::INDIRECT,
                )?,
            });
        }
        self.size = size;
        self.keys = [None; CASCADES];
        Ok(())
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

    /// Work out the frame's cascades and their casters, and write what they draw.
    ///
    /// `camera` is the frame's camera before any sub-pixel jitter, so the cascades hold still under
    /// the anti-aliasing; `frame_rows` is the frame's constants; `drawn` the instances the frame's
    /// draw wrote; `area` the world's size.
    ///
    /// # Errors
    /// What the device refuses.
    #[allow(clippy::too_many_lines, clippy::too_many_arguments)]
    pub(crate) fn prepare(
        &mut self,
        d: &mut Device,
        scene: &mut GpuScene,
        frame: &FrameDelta,
        camera: &Camera,
        frame_rows: &[u8],
        drawn: &BTreeSet<u32>,
        area: (u32, u32),
    ) -> Result<Option<ShadowPlan>, dereth_gfx::Error> {
        if !self.settings.on {
            return Ok(None);
        }
        let Some((sun, strength)) = sun_of(frame.light.sun) else {
            return Ok(None);
        };
        self.maps(d)?;
        let (visibility, visibility_index) = self.visibility(d, area.0, area.1)?;
        let cascades = cascades(camera, sun, self.size);
        let origin = frame.origin;
        let resident = scene.static_generation;
        let drawn_mode = self.settings.casters == Casters::InView;
        let view = frustum(camera.projection * camera.view);
        let in_view = |c: Vec3, r: f32| view.iter().all(|p| p.dot(c.extend(1.0)) >= -r);

        // The objects' parts the frame didn't draw get their instance rows here, when they moved.
        let mut objects: Vec<(Vec3, f32, std::ops::Range<u32>)> = Vec::new();
        let mut rows: Vec<(u32, Vec<u8>)> = Vec::new();
        for pose in &frame.objects {
            let Some(o) = scene.objects.get(&pose.key) else {
                continue;
            };
            for (k, pp) in pose.parts.iter().enumerate() {
                let Some(p) = o.parts.get(k) else {
                    continue;
                };
                if pp.hidden {
                    continue;
                }
                let is_drawn = drawn.contains(&p.instance);

                if !is_drawn && self.written.get(&p.instance) != Some(&pp.transform) {
                    let scale = Vec3::new(
                        pp.transform.x_axis.xyz().length(),
                        pp.transform.y_axis.xyz().length(),
                        pp.transform.z_axis.xyz().length(),
                    );
                    let row =
                        GpuScene::instance_bytes(pp.transform, scale, NONE, class::SUN, 0, NONE);
                    rows.push((p.instance, row));
                    self.written.insert(p.instance, pp.transform);
                } else if is_drawn {
                    self.written.remove(&p.instance);
                }
                let Some(items) = p.levels.first() else {
                    continue;
                };
                let (centre, radius) = part_sphere(p, pp.transform, Vec3::ZERO);
                if drawn_mode && !in_view(centre, radius) {
                    continue;
                }
                objects.push((centre, radius, items.clone()));
            }
        }

        let mut written_bytes = 0u64;
        for (i, row) in &rows {
            scene.write_instance(d, *i, row)?;
            written_bytes += row.len() as u64;
        }
        let casts = |scene: &GpuScene, i: u32| {
            matches!(
                scene.buckets.get(i as usize),
                Some(Bucket::Opaque | Bucket::OpaqueTwoSided)
            )
        };
        let backend = d.backend();
        let command_bytes = super::command_bytes(d);
        let command = |scene: &GpuScene, i: u32, out: &mut Vec<u8>| {
            if let Some(d) = scene.item_draws.get(i as usize) {
                out.extend(super::command(backend, d.index_count, d.first_index, i));
            }
        };

        let mut plans = Vec::with_capacity(CASCADES);
        let (mut redrawn, mut static_items, mut ground_blocks, mut dynamic_items) = (0, 0, 0, 0);
        for (c, cascade) in cascades.iter().enumerate() {
            let key = CacheKey {
                cascade: cascade.key,
                resident,
                origin,
            };
            let redraw = drawn_mode || self.keys[c] != Some(key);
            let gpu = &mut self.cascades[c];
            // The frame's constants with the cascade's view and projection.
            let mut rows = frame_rows.to_vec();
            if rows.len() >= 64 {
                rows[..64].copy_from_slice(&floats(&cascade.view_proj.to_cols_array()));
            }
            gpu.frame.write(d, &rows)?;
            written_bytes += rows.len() as u64;
            let mut grounds = Vec::new();
            let mut statics = 0u32;
            if redraw {
                let mut cmds = Vec::new();
                let mut ground_rows: Vec<[u32; 4]> = Vec::new();
                for (id, b) in &scene.blocks {
                    let offset = block_offset(*id, origin);
                    if let Some(g) = scene.ground.get(id) {
                        let min = offset + Vec3::new(0.0, 0.0, g.z[0]);
                        let max = offset + Vec3::new(192.0, 192.0, g.z[1]);
                        let seen = !drawn_mode || {
                            let c = (min + max) * 0.5;
                            in_view(c, (max - min).length() * 0.5)
                        };
                        if b.slot != NONE && seen && cascade.reaches_box(min, max) {
                            ground_rows.push([g.vertices.start, g.cells.start, g.side, b.slot]);
                        }
                    }
                    for p in &b.parts {
                        let (centre, radius) = part_sphere(p, p.transform, offset);
                        if !cascade.reaches(centre, radius) {
                            continue;
                        }
                        if drawn_mode && !in_view(centre, radius) {
                            continue;
                        }
                        let Some(items) = p.levels.first() else {
                            continue;
                        };
                        for i in items.clone() {
                            if casts(scene, i) {
                                command(scene, i, &mut cmds);
                                statics += 1;
                            }
                        }
                    }
                }
                ground_rows.sort_by_key(|g| g[2]);
                for (k, g) in ground_rows.iter().enumerate() {
                    let k = u32::try_from(k).unwrap_or(0);
                    match grounds.last_mut() {
                        Some((side, _, n)) if *side == g[2] => *n += 1,
                        _ => grounds.push((g[2], k, 1)),
                    }
                }
                ground_blocks += u32::try_from(ground_rows.len()).unwrap_or(u32::MAX);
                let gb: Vec<u8> = ground_rows.iter().flat_map(|g| words(g)).collect();
                gpu.grounds.write(d, &gb)?;
                gpu.statics.write(d, &cmds)?;
                written_bytes += (gb.len() + cmds.len()) as u64;
                self.keys[c] = Some(key);
                redrawn += 1;
                static_items += statics;
            }
            let mut dyn_cmds = Vec::new();
            let mut dynamics = 0u32;
            for (centre, radius, items) in &objects {
                if !cascade.reaches(*centre, *radius) {
                    continue;
                }
                for i in items.clone() {
                    if casts(scene, i) {
                        command(scene, i, &mut dyn_cmds);
                        dynamics += 1;
                    }
                }
            }
            gpu.dynamics.write(d, &dyn_cmds)?;
            written_bytes += dyn_cmds.len() as u64;
            dynamic_items += dynamics;
            plans.push(CascadePlan {
                map: gpu.map,
                cache: gpu.cache,
                frame: gpu.frame.index,
                redraw,
                statics: (gpu.statics.buffer, statics),
                grounds: (gpu.grounds.index, grounds),
                dynamics: (gpu.dynamics.buffer, dynamics),
            });
        }

        // The sun's share's constants.
        let mut bytes = Vec::new();
        for c in &cascades {
            bytes.extend(floats(&c.view_proj.to_cols_array()));
        }
        bytes.extend(floats(&cascades.map(|c| c.far)));
        bytes.extend(floats(&cascades.map(|c| c.texel)));
        let maps: Vec<u32> = self.cascades.iter().map(|c| c.map_index).collect();
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
        written_bytes += bytes.len() as u64;

        self.stats.frames += 1;
        self.stats.cascades_redrawn += u64::from(redrawn);
        self.stats.last_redrawn = redrawn;
        self.stats.last_static_items = static_items;
        self.stats.last_ground_blocks = ground_blocks;
        self.stats.last_dynamic_items = dynamic_items;
        self.stats.last_bytes = written_bytes;
        Ok(Some(ShadowPlan {
            pipelines: self.pipelines,
            cascades: plans,
            size: self.size,
            constants: self.constants.index,
            visibility,
            visibility_index,
            command_bytes,
        }))
    }
}

impl ShadowPlan {
    /// The sun's share's target.
    #[must_use]
    pub fn visibility(&self) -> Texture {
        self.visibility
    }

    /// The cascades' constants, by their index in the table, and their maps: what the fog's
    /// shafts look the sun up in.
    #[must_use]
    pub fn cascades(&self) -> (u32, Vec<Texture>) {
        (
            self.constants,
            self.cascades.iter().map(|c| c.map).collect(),
        )
    }

    /// Draw the cascades' maps: each cache drawn again where it moved, copied into its map, and
    /// the objects drawn over it. `caster_push` gives the meshes' push constants for a cascade's
    /// constants, `ground_push` the ground's for a cascade's constants, ground list and first
    /// entry, and `indices` is the scene's index buffer. With `drawn`, the ground drawn as a grid
    /// of patches, which follows the eye: it is drawn over each map every frame, and the caches
    /// hold none of the ground.
    ///
    /// # Errors
    /// What the layer refuses.
    pub(crate) fn record_maps(
        &self,
        r: &mut Recorder<'_>,
        indices: dereth_gfx::Buffer,
        caster_push: &dyn Fn(u32) -> Vec<u8>,
        ground_push: &dyn Fn(u32, u32, u32) -> Vec<u8>,
        drawn: Option<&super::drawn::DrawnPlan>,
    ) -> Result<(), dereth_gfx::Error> {
        let full = Rect {
            x: 0,
            y: 0,
            width: self.size,
            height: self.size,
        };
        let p = self.pipelines;
        for (c, cp) in self.cascades.iter().enumerate() {
            if cp.redraw {
                r.timestamp(
                    [
                        "shadow cache 0",
                        "shadow cache 1",
                        "shadow cache 2",
                        "shadow cache 3",
                    ][c],
                );
                r.raster(
                    &RasterPass {
                        name: "shadow cache",
                        color: &[],
                        depth: Some(DepthAttachment {
                            texture: cp.cache,
                            clear: Some(1.0),
                            write: true,
                        }),
                        touches: &[
                            Touch::Buffer(cp.statics.0, Use::Indirect),
                            Touch::Buffer(indices, Use::Index),
                        ],
                        area: Some(full),
                    },
                    |d| {
                        if drawn.is_none() && !cp.grounds.1.is_empty() {
                            d.set_pipeline(p.ground)?;
                            for (side, first, count) in &cp.grounds.1 {
                                d.push(0, &ground_push(cp.frame, cp.grounds.0, *first))?;
                                d.draw(side * side * 6, *count, 0, 0);
                            }
                        }
                        if cp.statics.1 > 0 {
                            d.set_pipeline(p.caster)?;
                            d.push(0, &caster_push(cp.frame))?;
                            d.bind_index_buffer(indices, 0, IndexType::U32)?;
                            d.draw_indexed_indirect(
                                cp.statics.0,
                                0,
                                cp.statics.1,
                                self.command_bytes,
                            )?;
                        }
                        Ok(())
                    },
                )?;
            }
            r.copy_texture(cp.cache, cp.map)?;
            if cp.dynamics.1 > 0 || drawn.is_some() {
                let mut touches = vec![
                    Touch::Buffer(cp.dynamics.0, Use::Indirect),
                    Touch::Buffer(indices, Use::Index),
                ];
                if let Some(g) = drawn {
                    touches.extend(g.touches());
                }
                r.raster(
                    &RasterPass {
                        name: "shadow objects",
                        color: &[],
                        depth: Some(DepthAttachment {
                            texture: cp.map,
                            clear: None,
                            write: true,
                        }),
                        touches: &touches,
                        area: Some(full),
                    },
                    |d| {
                        if let Some(g) = drawn {
                            g.draw_casters(d, cp.frame)?;
                        }
                        if cp.dynamics.1 > 0 {
                            d.set_pipeline(p.caster)?;
                            d.push(0, &caster_push(cp.frame))?;
                            d.bind_index_buffer(indices, 0, IndexType::U32)?;
                            d.draw_indexed_indirect(
                                cp.dynamics.0,
                                0,
                                cp.dynamics.1,
                                self.command_bytes,
                            )?;
                        }
                        Ok(())
                    },
                )?;
            }
        }
        Ok(())
    }

    /// Work out the sun's share at each pixel of the world, from the G-buffer and the maps.
    /// `frame` is the frame's constants' index; `depth` the depth the outdoor surfaces stand at.
    ///
    /// # Errors
    /// What the layer refuses.
    pub(crate) fn record_mask(
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
        touches.extend(
            self.cascades
                .iter()
                .map(|c| Touch::Texture(c.map, Use::Sampled)),
        );
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
                d.set_pipeline(self.pipelines.mask)?;
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
