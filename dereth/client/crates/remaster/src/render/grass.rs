//! The grass field: clumps of blades placed each frame round the eye from the terrain grid, as
//! `drawn.slang`'s `cs_grass` places them, and drawn into the G-buffer by `grass.slang`.
//!
//! Nothing is stored per block: where grass grows, how tall and how dry comes from each terrain
//! type's row ([`GrassKind`]), the road bits, the slope, the water and what stands overhead. Near
//! the eye the clumps are dense, each a few curved blades; farther off fewer, wider clumps of
//! single-triangle blades stand in for them; past the far reach the ground's own picture, whose
//! color the blades take, carries on.
//!
//! Flowers and pebbles ride on the same placing: flowers in clusters on grassland, each cluster a
//! few colors of its region's palette ([`PALETTE`]); pebbles on bare ground, scree and the road's
//! edges ([`pebbles`]).

use dereth_gfx::{
    BufferDesc, BufferUsage, ColorAttachment, ColorTarget, Compare, ComputeDesc, Cull,
    DepthAttachment, DepthState, Device, GraphicsDesc, Load, Location, Pipeline, RasterPass,
    Recorder, Rect, Texture, Touch, Use,
};
use glam::{Mat4, Vec3};

use super::drawn::DrawnPlan;
use super::{push_words, shader, Targets, ALBEDO, DEPTH, LIGHT, MATERIAL, NORMAL};
use crate::scene::FrameDelta;
use crate::shaders::Entry;

/// A square tile of candidate points, meters, as the shader lays them.
const TILE: f32 = 8.0;
/// How far the near clumps reach, how far the far ones do, and over how many meters the near hand
/// over to the far.
pub(crate) const NEAR_REACH: f32 = 85.0;
pub(crate) const FAR_REACH: f32 = 260.0;
const HAND_OVER: f32 = 15.0;
/// Candidate clumps a square meter, near and far.
const NEAR_CLUMPS: f32 = 16.0;
const FAR_CLUMPS: f32 = 1.6;
/// The room for clumps, near and far.
const NEAR_ROOM: u32 = 450_000;
const FAR_ROOM: u32 = 260_000;
/// The bytes of one clump.
const CLUMP_BYTES: u64 = 32;
/// Blades a clump, segments a blade and the blade's width at its root, meters: near and far.
const NEAR_BLADES: (u32, u32, f32) = (7, 4, 0.05);
const FAR_BLADES: (u32, u32, f32) = (4, 1, 0.13);
/// How far the flowers and pebbles reach, candidates a square meter, and the room for them.
pub(crate) const SCATTER_REACH: f32 = 90.0;
const SCATTER_CANDIDATES: f32 = 4.0;
const SCATTER_ROOM: u32 = 160_000;
/// The corners of one flower or pebble.
const SCATTER_CORNERS: u32 = 24;

/// What of the field a frame asks for.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct FieldAsk {
    /// The grass's blades.
    pub blades: bool,
    /// The flowers and pebbles.
    pub scatter: bool,
}
/// The constants' rows: the field's, each type's grass, the scatter's, each type's pebbles, the
/// palette.
const ROWS: usize = 11 + 32 + 1 + 32 + PALETTE.len();

/// The flowers' colors, display values: white, yellow, pink, violet, blue, orange, cream and red. A
/// region's clusters take a run of them from where its scene type starts.
pub const PALETTE: [[f32; 3]; 8] = [
    [0.93, 0.92, 0.86],
    [0.96, 0.82, 0.22],
    [0.92, 0.52, 0.68],
    [0.62, 0.48, 0.86],
    [0.42, 0.56, 0.92],
    [0.94, 0.55, 0.2],
    [0.95, 0.9, 0.62],
    [0.84, 0.24, 0.22],
];

/// The share of the scatter's candidates that are pebbles on each terrain type, before the road's
/// edges and the slopes add theirs.
#[must_use]
pub fn pebbles() -> [f32; 32] {
    let mut p = [0.0; 32];
    // Barren rock, mud, packed dirt, patchy dirt, the sands strewn with rock, semi-barren rock.
    for (t, share) in [
        (0, 0.6),
        (5, 0.2),
        (7, 0.45),
        (8, 0.35),
        (12, 0.7),
        (13, 0.5),
        (14, 0.5),
    ] {
        p[t] = share;
    }
    p
}
/// The field's clock wraps over this many seconds; every motion of the wind fits it closely
/// enough that the wrap is not seen.
const CLOCK_PERIOD: f64 = 1200.0;

/// What one terrain type grows.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct GrassKind {
    /// The share of the candidate clumps kept, 0 to 1.
    pub share: f32,
    /// How tall its blades stand, meters.
    pub height: f32,
    /// How dry its tips are, 0 fresh to 1 straw.
    pub dryness: f32,
    /// How patchily it grows, 0 evenly to 1 in clumps with bare ground between.
    pub patchy: f32,
}

const NONE_GROWS: GrassKind = GrassKind {
    share: 0.0,
    height: 0.0,
    dryness: 0.0,
    patchy: 0.0,
};

/// What each terrain type grows, by its number in the terrain word.
#[must_use]
pub fn kinds() -> [GrassKind; 32] {
    let mut k = [NONE_GROWS; 32];
    let mut set = |t: usize, share, height, dryness, patchy| {
        k[t] = GrassKind {
            share,
            height,
            dryness,
            patchy,
        };
    };
    // Grassland, lush grass, patchy grassland.
    // From a third of a meter on thin grassland to about two thirds in the marsh; less on bare ground.
    set(1, 1.0, 0.42, 0.25, 0.15);
    set(3, 1.0, 0.54, 0.1, 0.05);
    set(9, 0.85, 0.36, 0.4, 0.75);
    // Marsh: the tallest and darkest, in tufts.
    set(4, 0.6, 0.65, 0.05, 0.6);
    // Patchy dirt, mud: a little at the edges.
    set(8, 0.25, 0.27, 0.55, 0.9);
    set(5, 0.08, 0.28, 0.3, 0.9);
    // Semi-barren rock: a few dry tufts.
    set(14, 0.1, 0.25, 0.7, 0.9);
    // Forest floor: sparse.
    set(21, 0.3, 0.3, 0.35, 0.7);
    // Farmland.
    set(24, 0.6, 0.49, 0.6, 0.3);
    k
}

/// What each terrain type grows, as `looks` lay their changes over the remaster's own.
#[must_use]
pub fn kinds_with(looks: &crate::look::LookStack) -> [GrassKind; 32] {
    let mut out = kinds();
    for (t, k) in (0u8..).zip(out.iter_mut()) {
        let p = looks.grass(t);
        k.share = p.share.unwrap_or(k.share);
        k.height = p.height.unwrap_or(k.height);
        k.dryness = p.dryness.unwrap_or(k.dryness);
        k.patchy = p.patchy.unwrap_or(k.patchy);
    }
    out
}

/// What the field's pass draws this frame.
#[derive(Debug, Clone, Copy)]
pub struct GrassPlan {
    place: Pipeline,
    draw: Pipeline,
    scatter_place: Pipeline,
    scatter_draw: Pipeline,
    constants: (dereth_gfx::Buffer, u32),
    clumps: (dereth_gfx::Buffer, u32),
    args: (dereth_gfx::Buffer, u32),
    stride: u32,
    tiles: u32,
    frame: u32,
    weather: (u32, Option<Texture>),
    /// Whether the blades are drawn, and the flowers and pebbles.
    blades: bool,
    scatter: bool,
    /// The blades drawn again for their motion.
    motion: Pipeline,
}

/// The grass field's pass.
#[derive(Debug)]
pub struct GrassPass {
    place: Pipeline,
    draw: Pipeline,
    scatter_place: Pipeline,
    scatter_draw: Pipeline,
    motion: Pipeline,
    constants: (dereth_gfx::Buffer, u32),
    clumps: (dereth_gfx::Buffer, u32),
    args: (dereth_gfx::Buffer, u32),
    /// The field's clock last frame, for the blades' motion.
    last_clock: Option<f32>,
}

fn bound(
    d: &mut Device,
    label: &'static str,
    size: u64,
    usage: BufferUsage,
) -> Result<(dereth_gfx::Buffer, u32), dereth_gfx::Error> {
    let b = d.create_buffer(&BufferDesc {
        label,
        size: size.max(16),
        usage,
        location: Location::Gpu,
    })?;
    Ok((b, d.bind_buffer(b)?))
}

impl GrassPass {
    /// The pass, its pipelines made now.
    ///
    /// # Errors
    /// What the device refuses.
    pub fn new(gfx: &mut Device) -> Result<Self, dereth_gfx::Error> {
        let targets = [ALBEDO, NORMAL, LIGHT, MATERIAL].map(|format| ColorTarget {
            format,
            blend: None,
            write_alpha: true,
        });
        let draw = gfx.create_graphics_pipeline(&GraphicsDesc {
            label: "remaster grass",
            vertex: shader(Entry::GrassVsBlade),
            fragment: Some(shader(Entry::GrassFsBlade)),
            color: &targets,
            depth: Some(DepthState {
                format: DEPTH,
                compare: Some(Compare::LessEqual),
                write: true,
            }),
            cull: Cull::None,
        })?;
        let place = gfx.create_compute_pipeline(&ComputeDesc {
            label: "remaster grass placing",
            shader: shader(Entry::DrawnCsGrass),
        })?;
        let scatter_draw = gfx.create_graphics_pipeline(&GraphicsDesc {
            label: "remaster flowers and pebbles",
            vertex: shader(Entry::GrassVsScatter),
            fragment: Some(shader(Entry::GrassFsScatter)),
            color: &targets,
            depth: Some(DepthState {
                format: DEPTH,
                compare: Some(Compare::LessEqual),
                write: true,
            }),
            cull: Cull::None,
        })?;
        let scatter_place = gfx.create_compute_pipeline(&ComputeDesc {
            label: "remaster flowers and pebbles placing",
            shader: shader(Entry::DrawnCsScatter),
        })?;
        let motion = gfx.create_graphics_pipeline(&GraphicsDesc {
            label: "remaster grass motion",
            vertex: shader(Entry::GrassVsBladeMotion),
            fragment: Some(shader(Entry::GrassFsBladeMotion)),
            color: &[ColorTarget {
                format: super::post::MOTION,
                blend: None,
                write_alpha: true,
            }],
            depth: None,
            cull: Cull::None,
        })?;
        Ok(Self {
            place,
            draw,
            scatter_place,
            scatter_draw,
            motion,
            last_clock: None,
            constants: bound(
                gfx,
                "remaster grass constants",
                (ROWS * 16) as u64,
                BufferUsage::STORAGE,
            )?,
            clumps: bound(
                gfx,
                "remaster grass clumps",
                u64::from(NEAR_ROOM + FAR_ROOM + SCATTER_ROOM) * CLUMP_BYTES,
                BufferUsage::STORAGE,
            )?,
            args: bound(
                gfx,
                "remaster grass draws",
                64,
                BufferUsage::STORAGE | BufferUsage::INDIRECT,
            )?,
        })
    }

    /// The field for `frame`, seen through the pair `(clip, eye)` (the eye in the frame's space), lit by the
    /// frame's constants `constants`; wet and sheltered by the weather's constants and overhead
    /// map; its blades, and its flowers and pebbles, as the pair `(blades, scatter)` asks. `None` where
    /// nothing of the ground is drawn, or neither is asked.
    ///
    /// # Errors
    /// What the device refuses.
    pub fn prepare(
        &mut self,
        gfx: &mut Device,
        frame: &FrameDelta,
        (clip, eye): (Mat4, Vec3),
        constants: u32,
        weather: (u32, Option<Texture>),
        ask: FieldAsk,
    ) -> Result<Option<GrassPlan>, dereth_gfx::Error> {
        let FieldAsk {
            blades, scatter, ..
        } = ask;
        let Some(ground) = frame.drawn.as_ref() else {
            return Ok(None);
        };
        if !frame.landscape || !(blades || scatter) {
            return Ok(None);
        }
        let global = ground.global;
        let world_eye = Vec3::new(eye.x + global.x, eye.y + global.y, eye.z);
        let first = |c: f32| dereth_primitives::num::floor_to_i32((c - FAR_REACH) / TILE);
        #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)] // a few dozen
        let side = (2.0 * FAR_REACH / TILE).ceil() as u32 + 1;
        let mut rows: Vec<[u32; 4]> = super::shadow::frustum(clip)
            .iter()
            .map(|p| p.to_array().map(f32::to_bits))
            .collect();
        rows.push([eye.x, eye.y, eye.z, NEAR_REACH].map(f32::to_bits));
        rows.push([global.x, global.y, FAR_REACH, HAND_OVER].map(f32::to_bits));
        rows.push([
            side,
            u32::from_ne_bytes(first(world_eye.x).to_ne_bytes()),
            u32::from_ne_bytes(first(world_eye.y).to_ne_bytes()),
            NEAR_ROOM,
        ]);
        let w = super::weather::wind(frame.clock);
        #[allow(clippy::cast_possible_truncation)] // a wrapped clock
        let clock = frame.clock.rem_euclid(CLOCK_PERIOD) as f32;
        let last = self.last_clock.replace(clock).unwrap_or(clock);
        rows.push([w[0], w[1], clock, last].map(f32::to_bits));
        rows.push([
            NEAR_CLUMPS.to_bits(),
            FAR_CLUMPS.to_bits(),
            1.0f32.to_bits(),
            FAR_ROOM,
        ]);
        for k in kinds() {
            rows.push([k.share, k.height, k.dryness, k.patchy].map(f32::to_bits));
        }
        rows.push([
            SCATTER_CANDIDATES.to_bits(),
            SCATTER_REACH.to_bits(),
            SCATTER_ROOM,
            1.0f32.to_bits(),
        ]);
        for p in pebbles() {
            rows.push([p, 0.0, 0.0, 0.0].map(f32::to_bits));
        }
        for c in PALETTE {
            rows.push([c[0], c[1], c[2], 1.0].map(f32::to_bits));
        }
        let bytes: Vec<u8> = rows
            .iter()
            .flat_map(|r| r.iter().flat_map(|w| w.to_le_bytes()))
            .collect();
        gfx.write_buffer(self.constants.0, 0, &bytes)?;
        let backend = gfx.backend();
        let stride = backend.command_words() * 4;
        let near = backend.command(NEAR_BLADES.0 * ((NEAR_BLADES.1 - 1) * 6 + 3), 0, 0, 0);
        let far = backend.command(FAR_BLADES.0 * ((FAR_BLADES.1 - 1) * 6 + 3), 0, 0, 0);
        let mut args = push_words(&near);
        args.extend(push_words(&far));
        args.extend(push_words(&backend.command(SCATTER_CORNERS, 0, 0, 0)));
        gfx.write_buffer(self.args.0, 0, &args)?;
        Ok(Some(GrassPlan {
            place: self.place,
            draw: self.draw,
            scatter_place: self.scatter_place,
            scatter_draw: self.scatter_draw,
            motion: self.motion,
            blades,
            scatter,
            constants: self.constants,
            clumps: self.clumps,
            args: self.args,
            stride,
            tiles: side,
            frame: constants,
            weather,
        }))
    }
}

impl GrassPlan {
    /// Whether the blades are drawn.
    #[must_use]
    pub fn blades(&self) -> bool {
        self.blades
    }

    /// What drawing the blades' motion reads.
    pub(crate) fn motion_touches(&self) -> [Touch; 2] {
        [
            Touch::Buffer(self.clumps.0, Use::StorageRead),
            Touch::Buffer(self.args.0, Use::Indirect),
        ]
    }

    /// The blades drawn again into the motion target, each pixel where the G-buffer holds a blade
    /// writing where its surface was last frame: `post` the post passes' constants, `depths` the
    /// depth's and the outdoor depth's indices.
    ///
    /// # Errors
    /// What the layer refuses.
    pub(crate) fn draw_motion(
        &self,
        d: &mut dereth_gfx::Draw<'_>,
        frame: u32,
        post: u32,
        depths: (u32, u32),
    ) -> Result<(), dereth_gfx::Error> {
        d.set_pipeline(self.motion)?;
        for (k, (base, blades)) in [(0, NEAR_BLADES), (NEAR_ROOM, FAR_BLADES)]
            .into_iter()
            .enumerate()
        {
            d.push(
                0,
                &push_words(&[
                    frame,
                    self.constants.1,
                    self.clumps.1,
                    base,
                    blades.0,
                    blades.1,
                    blades.2.to_bits(),
                    self.weather.0,
                    post,
                    depths.0,
                    depths.1,
                ]),
            )?;
            d.draw_indirect(
                self.args.0,
                u64::from(self.stride) * k as u64,
                1,
                self.stride,
            )?;
        }
        Ok(())
    }

    /// The clumps placed, recorded before the world is drawn, over the drawn ground's grid.
    ///
    /// # Errors
    /// What the layer refuses.
    pub fn record_place(
        &self,
        r: &mut Recorder<'_>,
        drawn: &DrawnPlan,
    ) -> Result<(), dereth_gfx::Error> {
        r.timestamp("grass placing");
        let mut touches = drawn.touches();
        touches.push(Touch::Buffer(self.clumps.0, Use::StorageWrite));
        touches.push(Touch::Buffer(self.args.0, Use::StorageWrite));
        if let Some(cover) = self.weather.1 {
            touches.push(Touch::Texture(cover, Use::Sampled));
        }
        r.compute(&touches, |c| {
            let push = drawn.grass_push(
                self.constants.1,
                self.clumps.1,
                self.args.1,
                self.stride,
                self.weather.0,
            );
            for (on, pipeline) in [
                (self.blades, self.place),
                (self.scatter, self.scatter_place),
            ] {
                if on {
                    c.set_pipeline(pipeline)?;
                    c.push(0, &push)?;
                    c.dispatch(self.tiles, self.tiles, 1);
                }
            }
            Ok(())
        })
    }

    /// The blades drawn into the G-buffer.
    ///
    /// # Errors
    /// What the layer refuses.
    pub fn record_draw(&self, r: &mut Recorder<'_>, t: &Targets) -> Result<(), dereth_gfx::Error> {
        r.timestamp("grass");
        let keep = [t.albedo, t.normal, t.light, t.material].map(|texture| ColorAttachment {
            texture,
            load: Load::Keep,
        });
        let mut touches = vec![
            Touch::Buffer(self.clumps.0, Use::StorageRead),
            Touch::Buffer(self.args.0, Use::Indirect),
        ];
        if let Some(cover) = self.weather.1 {
            touches.push(Touch::Texture(cover, Use::Sampled));
        }
        r.raster(
            &RasterPass {
                name: "grass",
                color: &keep,
                depth: Some(DepthAttachment {
                    texture: t.depth,
                    clear: None,
                    write: true,
                }),
                touches: &touches,
                area: Some(Rect {
                    x: 0,
                    y: 0,
                    width: t.width,
                    height: t.height,
                }),
            },
            |d| {
                if self.scatter {
                    d.set_pipeline(self.scatter_draw)?;
                    d.push(
                        0,
                        &push_words(&[
                            self.frame,
                            self.constants.1,
                            self.clumps.1,
                            NEAR_ROOM + FAR_ROOM,
                            0,
                            0,
                            0,
                            self.weather.0,
                        ]),
                    )?;
                    d.draw_indirect(self.args.0, u64::from(self.stride) * 2, 1, self.stride)?;
                }
                if !self.blades {
                    return Ok(());
                }
                d.set_pipeline(self.draw)?;
                for (k, (base, blades)) in [(0, NEAR_BLADES), (NEAR_ROOM, FAR_BLADES)]
                    .into_iter()
                    .enumerate()
                {
                    d.push(
                        0,
                        &push_words(&[
                            self.frame,
                            self.constants.1,
                            self.clumps.1,
                            base,
                            blades.0,
                            blades.1,
                            blades.2.to_bits(),
                            self.weather.0,
                        ]),
                    )?;
                    d.draw_indirect(
                        self.args.0,
                        u64::from(self.stride) * k as u64,
                        1,
                        self.stride,
                    )?;
                }
                Ok(())
            },
        )
    }
}

#[cfg(test)]
mod tests {
    //! Behaviour: none (experimental)
    use super::*;

    /// Pebbles lie on bare and rocky ground and never on grass or water.
    #[test]
    fn pebbles_lie_on_bare_ground_and_not_on_grass_or_water() {
        let p = pebbles();
        for bare in [0, 7, 12] {
            assert!(p[bare] > 0.3, "{bare}");
        }
        for not in [1, 3, 9, 16, 17, 18, 19, 20] {
            assert_eq!(p[not], 0.0, "{not}");
        }
    }

    /// A look's grass table changes what that terrain type grows, number by number, and nothing else.
    #[test]
    fn a_looks_grass_table_changes_what_that_type_grows() {
        use crate::look::{Layer, LookStack, LookToml};
        let mut looks = LookStack::new();
        looks.set(
            Layer::Player,
            Some(LookToml::parse("[grass.3]\nheight = 0.9\n").expect("a look")),
        );
        let k = kinds_with(&looks);
        assert_eq!(k[3].height, 0.9);
        assert_eq!(k[3].share, kinds()[3].share);
        assert_eq!(k[1], kinds()[1]);
        assert!(LookToml::parse("[grass.40]\nheight = 1\n").is_err());
        assert!(LookToml::parse("[grade.dawn]\nsaturation = 1\n").is_err());
    }

    /// Grass grows on the grass types and nowhere wet, sandy, rocky or snowy.
    #[test]
    fn grass_grows_on_grassland_and_not_on_water_sand_rock_or_snow() {
        let k = kinds();
        for grass in [1, 3, 9] {
            assert!(k[grass].share >= 0.75, "{grass}");
        }
        for bare in [0, 2, 10, 11, 12, 13, 15, 16, 17, 18, 19, 20, 22, 31] {
            assert_eq!(k[bare].share, 0.0, "{bare}");
        }
    }
}
