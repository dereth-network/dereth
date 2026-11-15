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
//!   the cascade's map and the objects drawn over it. While the wind sways an art pack's models
//!   by more than a cascade's texel, that cascade is drawn again every frame, so their shadows
//!   sway with them rather than stand where they stood when the cache was drawn.
//! - **The sun's share.** Each pixel lit by the sun (the ground and what stands in the sun) is
//!   looked up in the first cascade that covers it, offset along its normal by a texel and
//!   compared against the casters with a 4 x 4 tent filter, blended into the next cascade over
//!   the last fifth of each cascade, and faded out toward the shadows' reach. The shadows fade in
//!   as the sun rises off the horizon.
//!
//! The cascades, their maps and the sun's share are the shared passes' (`dereth_gfx_passes::sun`);
//! what casts into them, and when a cascade's cache is drawn again, is this module's.

use std::collections::{BTreeMap, BTreeSet};

use dereth_gfx::{
    Compare, Cull, DepthAttachment, DepthState, Device, GraphicsDesc, IndexType, Pipeline,
    RasterPass, Recorder, Rect, Shader, Texture, Touch, Use,
};
pub use dereth_gfx_passes::sun::{
    cascades, frustum, sun_of, Cascade, CascadeKey, CASCADES, SPLITS,
};
use dereth_gfx_passes::sun::{MaskPlan, SunMaps, DEPTH};
use glam::{Mat4, Vec3, Vec4Swizzles};

use super::scene::{class, Bucket, GpuScene, PartItems, NONE};
use super::{PerFrame, Targets};
use crate::scene::{BlockId, Camera, FrameDelta};
use crate::shaders::{spirv, Entry};

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

/// How strongly the wind blows that an art pack's models sway in, meters a second, from the
/// frame's constants `rows`: 0 where the sway is off and the wind is stilled.
fn wind_strength(rows: &[u8]) -> f32 {
    let at = super::light::FRAME_WIND * 16;
    let f = |k: usize| {
        rows.get(at + k * 4..at + k * 4 + 4)
            .and_then(|b| b.try_into().ok())
            .map_or(0.0, f32::from_le_bytes)
    };
    dereth_primitives::num::math::hypotf(f(0), f(1))
}

/// The pipelines the shadows draw with.
#[derive(Debug, Clone, Copy)]
struct Pipelines {
    caster: Pipeline,
    ground: Pipeline,
}

/// One cascade's resources.
#[derive(Debug)]
struct CascadeGpu {
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
    /// The cascades' maps, each with the cache the blocks' casters are kept in between frames,
    /// and the sun's share.
    sun: SunMaps,
    keys: [Option<CacheKey>; CASCADES],
    /// The objects' instances the shadows wrote, with the transform they wrote.
    written: BTreeMap<u32, Mat4>,
    /// The blocks' parts' spheres in the frame's space, block after block, with whether each is
    /// ground cover, and each block's box round its parts': worked out once a frame that redraws
    /// a cascade, for every cascade.
    spheres: Vec<(Vec3, f32, bool)>,
    block_boxes: Vec<(Vec3, Vec3)>,
    /// What `spheres` and `block_boxes` were worked out for: the scene's resident statics, the
    /// frame's origin and which parts the processor draws ([`super::instances`]). They stand
    /// until one of them changes.
    spheres_of: Option<(u64, [i32; 2], u64)>,
    /// How far the wind bends the most-bent vertex of any resident model, meters at a wind of a
    /// meter a second, and the meshes' generation it was found at.
    bend: (Option<u64>, f32),
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
    /// The sun's share's part of the frame.
    mask: MaskPlan,
    /// The sun's share's index in the table.
    pub visibility_index: u32,
    /// The bytes one indirect draw command takes on the device.
    command_bytes: u32,
    /// The cascades as the parts chosen on the device are cast into them.
    pub(crate) parts: super::instances::PartsCascades,
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
        };
        Ok(Self {
            settings: ShadowSettings::default(),
            pipelines,
            cascades: Vec::new(),
            sun: SunMaps::new(d)?,
            keys: [None; CASCADES],
            written: BTreeMap::new(),
            spheres: Vec::new(),
            block_boxes: Vec::new(),
            spheres_of: None,
            bend: (None, 0.0),
            stats: ShadowStats::default(),
        })
    }

    /// The cascades' maps, made at the settings' size on first use, and each cascade's buffers.
    fn maps(&mut self, d: &mut Device) -> Result<(), dereth_gfx::Error> {
        if self.sun.ensure(d, self.settings.size)? {
            self.keys = [None; CASCADES];
        }
        while self.cascades.len() < CASCADES {
            self.cascades.push(CascadeGpu {
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
        Ok(())
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
        self.sun.read_visibility(d)
    }

    /// Work out the frame's cascades and their casters, and write what they draw.
    ///
    /// `camera` is the frame's camera before any sub-pixel jitter, so the cascades hold still under
    /// the anti-aliasing; `frame_rows` is the frame's constants; `drawn` the instances the frame's
    /// draw wrote; `area` the world's size; `instances` the models the device casts itself.
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
        instances: &super::instances::DeviceParts,
    ) -> Result<Option<ShadowPlan>, dereth_gfx::Error> {
        if !self.settings.on {
            return Ok(None);
        }
        let Some((sun, strength)) = sun_of(frame.light.sun) else {
            return Ok(None);
        };
        self.maps(d)?;
        let cascades = cascades(camera, sun, self.sun.size());
        // How far the wind moves the models it sways this frame: a cascade whose texels are
        // smaller is drawn again every frame.
        if self.bend.0 != Some(scene.mesh_generation) {
            let most = scene
                .meshes
                .values()
                .fold(0.0f32, |most, m| most.max(m.sway));
            self.bend = (Some(scene.mesh_generation), most);
        }
        let swayed = self.bend.1 * wind_strength(frame_rows);
        let swaying = |c: &Cascade| swayed > c.texel;
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
                super::command_into(backend, d.index_count, d.first_index, i, out);
            }
        };
        // The blocks' parts' spheres, once for every cascade drawn again, and the box round each
        // block's: a cascade its box misses has none of the block's parts.
        let redraws = cascades.iter().enumerate().any(|(c, cascade)| {
            drawn_mode
                || swaying(cascade)
                || self.keys[c]
                    != Some(CacheKey {
                        cascade: cascade.key,
                        resident,
                        origin,
                    })
        });
        let spheres_of = (resident, origin, instances.generation());
        if redraws && self.spheres_of != Some(spheres_of) {
            self.spheres_of = Some(spheres_of);
            self.spheres.clear();
            self.block_boxes.clear();
            for (id, b) in &scene.blocks {
                let offset = block_offset(*id, origin);
                let (mut min, mut max) =
                    (Vec3::splat(f32::INFINITY), Vec3::splat(f32::NEG_INFINITY));
                // The parts the device chooses for, it casts itself.
                let cpu = instances.cpu_parts(*id);
                for j in 0..cpu.map_or(b.parts.len(), <[u32]>::len) {
                    let Some(p) = b.parts.get(cpu.map_or(j, |c| c[j] as usize)) else {
                        continue;
                    };
                    let (centre, radius) = part_sphere(p, p.transform, offset);
                    min = min.min(centre - Vec3::splat(radius));
                    max = max.max(centre + Vec3::splat(radius));
                    self.spheres.push((centre, radius, p.cover));
                }
                self.block_boxes.push((min, max));
            }
        }

        let mut plans = Vec::with_capacity(CASCADES);
        let mut redraw_bits = 0u32;
        let (mut redrawn, mut static_items, mut ground_blocks, mut dynamic_items) = (0, 0, 0, 0);
        for (c, cascade) in cascades.iter().enumerate() {
            let key = CacheKey {
                cascade: cascade.key,
                resident,
                origin,
            };
            let redraw = drawn_mode || swaying(cascade) || self.keys[c] != Some(key);
            if redraw {
                redraw_bits |= 1 << c;
            }
            let gpu = &mut self.cascades[c];
            let maps = self.sun.maps()[c];
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
                let mut first = 0;
                for ((id, b), (lo, hi)) in scene.blocks.iter().zip(&self.block_boxes) {
                    let offset = block_offset(*id, origin);
                    let cpu = instances.cpu_parts(*id);
                    let n = cpu.map_or(b.parts.len(), <[u32]>::len);
                    let spheres = self.spheres.get(first..first + n).unwrap_or(&[]);
                    first += n;
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
                    if n == 0 || !cascade.reaches_box(*lo, *hi) {
                        continue;
                    }
                    for (j, (centre, radius, cover)) in spheres.iter().enumerate() {
                        let Some(p) = b.parts.get(cpu.map_or(j, |c| c[j] as usize)) else {
                            continue;
                        };
                        let (centre, radius) = (*centre, *radius);
                        // Ground cover casts in the nearest cascade alone: the next one, which
                        // the sun's share blends into over the nearest's last fifth, has none.
                        if *cover && c > 0 {
                            continue;
                        }
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
                map: maps.map,
                cache: maps.cache,
                frame: gpu.frame.index,
                redraw,
                statics: (gpu.statics.buffer, statics),
                grounds: (gpu.grounds.index, grounds),
                dynamics: (gpu.dynamics.buffer, dynamics),
            });
        }

        // The sun's share's constants.
        let (mask, bytes) = self.sun.prepare(d, &cascades, strength, area)?;
        written_bytes += bytes;

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
            size: self.sun.size(),
            visibility_index: mask.visibility_index,
            mask,
            command_bytes,
            parts: super::instances::PartsCascades {
                light_view: cascades[0].light_view,
                across: cascades.map(|c| c.across),
                along: cascades.map(|c| c.along),
                redraw: redraw_bits,
                in_view: drawn_mode,
            },
        }))
    }
}

impl ShadowPlan {
    /// The sun's share's target.
    #[must_use]
    pub fn visibility(&self) -> Texture {
        self.mask.visibility
    }

    /// The cascades' constants, by their index in the table, and their maps: what the fog's
    /// shafts look the sun up in.
    #[must_use]
    pub fn cascades(&self) -> (u32, Vec<Texture>) {
        (self.mask.constants, self.mask.maps.clone())
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
        parts: Option<&super::instances::PartsPlan>,
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
                let parts = parts.filter(|p| p.casts(c));
                let mut touches = vec![
                    Touch::Buffer(cp.statics.0, Use::Indirect),
                    Touch::Buffer(indices, Use::Index),
                ];
                if let Some(p) = parts {
                    touches.extend(p.caster_touches());
                }
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
                        touches: &touches,
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
                        if cp.statics.1 > 0 || parts.is_some() {
                            d.set_pipeline(p.caster)?;
                            d.push(0, &caster_push(cp.frame))?;
                            d.bind_index_buffer(indices, 0, IndexType::U32)?;
                        }
                        if cp.statics.1 > 0 {
                            d.draw_indexed_indirect(
                                cp.statics.0,
                                0,
                                cp.statics.1,
                                self.command_bytes,
                            )?;
                        }
                        if let Some(parts) = parts {
                            parts.draw_casters(d, c)?;
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
        self.mask.record(r, t, frame, depth)
    }
}

#[cfg(test)]
mod tests {
    //! Behaviour: none (experimental)
    use super::*;
    use dereth_primitives::num::math;

    /// The wind the trees sway in is read from the frame's constants as they are written: its
    /// strength, and none once the sway is off and the wind stilled.
    #[test]
    fn the_wind_the_trees_sway_in_is_read_from_the_frames_constants() {
        let f = crate::scene::FrameDelta {
            clock: 1234.5,
            ..crate::scene::FrameDelta::default()
        };
        let rows = super::super::light::frame_rows(
            &f,
            0.0,
            super::super::light::LightMode::Vertex,
            (640, 480),
        );
        let w = super::super::light::wind_row(&f);
        let blowing = wind_strength(&rows.bytes());
        assert!((blowing - math::hypotf(w[0], w[1])).abs() < 1e-6 && blowing > 0.0);
        let still = super::super::light::swaying(rows, false).bytes();
        assert_eq!(wind_strength(&still), 0.0);
        assert_eq!(wind_strength(&[]), 0.0, "no constants: no wind");
    }
}
