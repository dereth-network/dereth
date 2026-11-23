//! The physical sky: the lookup tables of a scattering atmosphere (after Hillaire's model), the sky
//! drawn from them over whatever sky the game draws, and a veil of haze over the horizon.
//!
//! - **The tables.** The transmittance from any height toward any zenith angle and the light
//!   scattered more than once depend only on the air: they are drawn when it changes. The sky as
//!   seen from the eye depends on the sun and the eye's height: it is drawn when the sun moves a
//!   tenth of a degree or the eye five metres.
//! - **The sky.** Drawn over the game's own sky as opaque as the game asks ([`SkyLight::cover`]),
//!   then (the game's stars, sun, moons and clouds drawn over it in between) the veil over the
//!   horizon, toward the sky's own colour there by day and the fog's by night. Both in display
//!   values, or taken to linear light for a frame drawn in linear light.
//!
//! What the sky is lit by comes from the game as plain data ([`SkyLight`]): the sun, its colour,
//! the air of the day, how much of the day it is and the fog. The constants' rows are those
//! `shaders/atmosphere.slang` names; a game may add its own rows after them.

use dereth_gfx::{
    Address, Blend, ColorAttachment, ColorTarget, Cull, DepthState, Device, Draw, Factor, Filter,
    Format, GraphicsDesc, Load, Pipeline, RasterPass, Recorder, Rect, Sampler, SamplerDesc,
    Texture, TextureDesc, TextureUsage,
};
use glam::{Mat4, Vec3, Vec4};

use crate::frame::{push_words, PerFrame};
use crate::shaders::{fullscreen, shader, Entry};

/// The planet's radius in the sky model, kilometres.
pub const PLANET_RADIUS_KM: f32 = 6360.0;
/// The atmosphere's outer radius, kilometres.
pub const TOP_RADIUS_KM: f32 = 6460.0;
/// The sky model's radiance for unit sun illuminance is scaled by this before the picture's
/// roll-off: a noon sky's horizon then sits near the top of the range without clipping.
pub const EXPOSURE: f32 = 9.5;
/// The transmittance table's size, the multiple-scattering table's, and the sky-view table's with
/// its raymarch's steps.
pub const TRANSMITTANCE: (u32, u32) = (256, 64);
pub const MULTIPLE: (u32, u32) = (32, 32);
pub const SKY_VIEW: (u32, u32, u32) = (192, 108, 30);
/// How bright the horizon's veil is at night against the fog, and how wide, against by day.
const NIGHT_VEIL: f32 = 0.5;
const NIGHT_VEIL_BAND: f32 = 0.35;
/// The tables' format.
const TABLE: Format = Format::Rgba16Float;
/// The size of the base rows (`ROWS` of 16 bytes after the ray's matrix), and of the constants'
/// buffer, which holds a game's own rows after them.
pub const ROWS: usize = 16;
const CONSTANTS_SIZE: u64 = 512;

/// The air of a day, and how the sky is exposed for it.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Air {
    /// The air's scattering, as factors on a clear day's: its haze (Mie) and its air (Rayleigh).
    pub mie: f32,
    pub rayleigh: f32,
    /// How grey the sky is, 0 blue to 1 grey.
    pub overcast: f32,
    /// How brightly the sky is exposed, against a clear day's.
    pub exposure: f32,
}

/// What the sky is drawn from this frame, as plain data from the game.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct SkyLight {
    /// The view (its rotation is what is read) and the projection the sky is drawn through.
    pub view: Mat4,
    pub projection: Mat4,
    /// The eye in the frame's space, metres, z up.
    pub eye: Vec3,
    /// The target's size in pixels.
    pub extent: (u32, u32),
    /// The direction toward the sun (any length).
    pub sun: Vec3,
    /// The sun's colour, in linear light.
    pub sun_color: Vec3,
    /// How far into the low sun of dawn or dusk the day is, 0 to 1 ([`twilight`]).
    pub twilight: f32,
    pub air: Air,
    /// How much of the day it is, 0 night to 1 day.
    pub day: f32,
    /// How much of the game's own sky the physical sky covers.
    pub cover: f32,
    /// The fog's end, metres, and its colour in linear light; none on a moment without fog.
    pub fog: Option<(f32, Vec3)>,
    /// The frame's colors are linear light (else display values).
    pub linear: bool,
    /// How much of the fog's color is laid over the whole sky before the land, 0 to 1: a world's
    /// sky tints' veil; 0 for every other world.
    pub sky_veil: f32,
}

fn smoothstep(a: f32, b: f32, x: f32) -> f32 {
    let t = ((x - a) / (b - a)).clamp(0.0, 1.0);
    t * t * (3.0 - 2.0 * t)
}

/// How far into the low sun of dawn or dusk the day is with the sun `elevation` degrees up: 1 with
/// it 4 degrees up or lower, 0 with it 18 degrees up or higher.
#[must_use]
pub fn twilight(elevation: f32) -> f32 {
    1.0 - smoothstep(4.0, 18.0, elevation)
}

/// The sun's illuminance as the sky is exposed for: `exposure`, tinted toward the sun's colour
/// (`color`, linear), the more so in the low sun of `twilight`.
#[must_use]
pub fn sun_illuminance(color: Vec3, twilight: f32, exposure: f32) -> Vec3 {
    let peak = color.max_element().max(1e-3);
    let t = 0.5 + (0.7 - 0.5) * twilight;
    let tint = Vec3::ONE.lerp(color / peak, t);
    tint * EXPOSURE * exposure
}

/// The direction in the frame's space through each point of the screen: the inverse of the
/// projection times the view's rotation alone.
#[must_use]
pub fn ray_from_ndc(view: Mat4, projection: Mat4) -> Mat4 {
    let mut rotation = view;
    rotation.w_axis = Vec4::W;
    (projection * rotation).inverse()
}

/// The constants' rows every sky shader reads, for `light`: the ray's matrix, then [`ROWS`] rows.
#[must_use]
#[allow(clippy::cast_precision_loss)] // a target's extent and table sizes
pub fn rows(light: &SkyLight) -> Vec<u8> {
    let Air {
        mie,
        rayleigh,
        overcast,
        ..
    } = light.air;
    let day = light.day;
    let fog_max = light.fog.map_or(2400.0, |f| f.0);
    let band = (0.045 * (2400.0 / fog_max.max(50.0)).sqrt()).clamp(0.03, 0.08)
        * (NIGHT_VEIL_BAND + (1.0 - NIGHT_VEIL_BAND) * day);
    let fog_color = light.fog.map_or(Vec3::new(0.55, 0.58, 0.7), |f| f.1);
    let eye = light.eye;
    let rows: [Vec4; ROWS - 4] = [
        eye.extend((eye.z.max(0.0) + 2.0) * 0.001),
        Vec4::new(light.extent.0 as f32, light.extent.1 as f32, 0.0, 0.0),
        light.sun.normalize_or(Vec3::Z).extend(overcast),
        sun_illuminance(light.sun_color, light.twilight, light.air.exposure).extend(light.twilight),
        (Vec3::new(5.802e-3, 13.558e-3, 33.1e-3) * rayleigh).extend(-1.0 / 8.0),
        Vec4::new(3.996e-3 * mie, 4.44e-3 * mie, -1.0 / 1.2, 0.8),
        Vec4::new(0.650e-3, 1.881e-3, 0.085e-3, 0.0),
        Vec3::new(0.25, 0.27, 0.2).extend(PLANET_RADIUS_KM),
        fog_color.extend(0.0),
        Vec4::new(
            light.cover,
            if light.linear { 1.0 } else { 0.0 },
            TOP_RADIUS_KM,
            0.0,
        ),
        Vec4::new(SKY_VIEW.0 as f32, SKY_VIEW.1 as f32, 0.0, SKY_VIEW.2 as f32),
        Vec4::new(
            band,
            day,
            NIGHT_VEIL + (1.0 - NIGHT_VEIL) * day,
            light.sky_veil,
        ),
    ];
    let mut out = Vec::with_capacity(16 * 16);
    for f in ray_from_ndc(light.view, light.projection).to_cols_array() {
        out.extend_from_slice(&f.to_le_bytes());
    }
    for r in rows {
        for f in r.to_array() {
            out.extend_from_slice(&f.to_le_bytes());
        }
    }
    out
}

/// `rows` with the ray's matrix replaced by `ray`: the same sky seen another way (a mirror's).
#[must_use]
pub fn seen_through(rows: &[u8], ray: Mat4) -> Vec<u8> {
    let mut out = rows.to_vec();
    for (k, f) in ray.to_cols_array().iter().enumerate() {
        if let Some(at) = out.get_mut(k * 4..k * 4 + 4) {
            at.copy_from_slice(&f.to_le_bytes());
        }
    }
    out
}

/// A float rounded to a whole number, as a key's bits: minus zero is zero.
fn whole(v: f32) -> u32 {
    (v.round() + 0.0).to_bits()
}

/// What the tables depend on: the air's scattering.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct TablesKey([u32; 2]);

/// What the sky-view table depends on: the air, the sun to a tenth of a degree (elevation and
/// heading) and the eye's height to five metres.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct ViewKey {
    tables: TablesKey,
    sun: [u32; 3],
    height: u32,
}

/// The sky's pipelines.
#[derive(Debug, Clone, Copy)]
struct Pipelines {
    transmittance: Pipeline,
    multiple: Pipeline,
    sky_view: Pipeline,
    /// The sky and the veil, for the frame's own targets and for the post passes' ones.
    sky: [Pipeline; 2],
    veil: [Pipeline; 2],
}

/// The sky's resources and state.
#[derive(Debug)]
pub struct Sky {
    pipelines: Pipelines,
    sampler: (Sampler, u32),
    tables: Option<[(Texture, u32); 3]>,
    constants: PerFrame,
    /// The last frame's constants, for views of the sky other than the frame's own.
    last_rows: Vec<u8>,
    tables_key: Option<TablesKey>,
    view_key: Option<ViewKey>,
    /// Times the air's tables and the sky-view table were drawn.
    pub tables_drawn: u64,
    pub views_drawn: u64,
}

/// What the sky draws this frame.
#[derive(Debug, Clone)]
pub struct SkyPlan {
    pipelines: Pipelines,
    post: bool,
    constants: u32,
    sampler: u32,
    tables: [(Texture, u32); 3],
    draw_tables: bool,
    draw_view: bool,
}

impl Sky {
    /// The sky, its pipelines made now; it is drawn into targets of `color`, or of `post` when the
    /// post passes run.
    ///
    /// # Errors
    /// What the device refuses.
    pub fn new(d: &mut Device, color: Format, post: Format) -> Result<Self, dereth_gfx::Error> {
        Self::with_depth(d, color, post, None)
    }

    /// [`Self::new`], drawn in passes that hold a depth of format `depth`, which the sky neither
    /// tests nor writes.
    ///
    /// # Errors
    /// What the device refuses.
    pub fn with_depth(
        d: &mut Device,
        color: Format,
        post: Format,
        depth: Option<Format>,
    ) -> Result<Self, dereth_gfx::Error> {
        let depth = depth.map(|format| DepthState {
            format,
            compare: None,
            write: false,
        });
        let table = |d: &mut Device, label, e: Entry| {
            d.create_graphics_pipeline(&GraphicsDesc {
                label,
                vertex: fullscreen(),
                fragment: Some(shader(e)),
                color: &[ColorTarget {
                    format: TABLE,
                    blend: None,
                    write_alpha: true,
                }],
                depth: None,
                cull: Cull::None,
            })
        };
        let over = |d: &mut Device, label, e: Entry, color: Format| {
            d.create_graphics_pipeline(&GraphicsDesc {
                label,
                vertex: fullscreen(),
                fragment: Some(shader(e)),
                color: &[ColorTarget {
                    format: color,
                    blend: Some(Blend {
                        src: Factor::SrcAlpha,
                        dst: Factor::OneMinusSrcAlpha,
                    }),
                    write_alpha: false,
                }],
                depth,
                cull: Cull::None,
            })
        };
        let pipelines = Pipelines {
            transmittance: table(
                d,
                "remaster sky transmittance",
                Entry::SkyTablesFsTransmittance,
            )?,
            multiple: table(
                d,
                "remaster sky multiple scattering",
                Entry::SkyTablesFsMultiple,
            )?,
            sky_view: table(d, "remaster sky view", Entry::SkyTablesFsSkyView)?,
            sky: [
                over(d, "remaster physical sky", Entry::SkyFsSky, color)?,
                over(d, "remaster physical sky, linear", Entry::SkyFsSky, post)?,
            ],
            veil: [
                over(d, "remaster horizon veil", Entry::SkyFsVeil, color)?,
                over(d, "remaster horizon veil, linear", Entry::SkyFsVeil, post)?,
            ],
        };
        let sampler = d.create_sampler(&SamplerDesc {
            filter: Filter::Linear,
            mip: Filter::Nearest,
            address: Address::Clamp,
            address_v: Address::Clamp,
            ..SamplerDesc::default()
        })?;
        let sampler_index = d.bind_sampler(sampler)?;
        Ok(Self {
            pipelines,
            sampler: (sampler, sampler_index),
            tables: None,
            constants: PerFrame::new(
                d,
                "remaster sky constants",
                CONSTANTS_SIZE,
                Default::default(),
            )?,
            last_rows: Vec::new(),
            tables_key: None,
            view_key: None,
            tables_drawn: 0,
            views_drawn: 0,
        })
    }

    /// The tables, made the first time they are asked for.
    ///
    /// # Errors
    /// What the device refuses.
    pub fn tables(&mut self, d: &mut Device) -> Result<[(Texture, u32); 3], dereth_gfx::Error> {
        if let Some(t) = self.tables {
            return Ok(t);
        }
        let mut make = |label, (w, h): (u32, u32)| -> Result<(Texture, u32), dereth_gfx::Error> {
            let t = d.create_texture(&TextureDesc {
                label,
                width: w,
                height: h,
                mips: 1,
                format: TABLE,
                usage: TextureUsage::COLOR_TARGET | TextureUsage::SAMPLED,
            })?;
            Ok((t, d.bind_texture(t)?))
        };
        let t = [
            make("remaster sky transmittance", TRANSMITTANCE)?,
            make("remaster sky multiple scattering", MULTIPLE)?,
            make("remaster sky view", (SKY_VIEW.0, SKY_VIEW.1))?,
        ];
        self.tables = Some(t);
        Ok(t)
    }

    /// The sky's constants the frame was drawn with.
    #[must_use]
    pub fn rows(&self) -> &[u8] {
        &self.last_rows
    }

    /// Work out the frame's sky from `light`: its constants written (the game's own rows `more`
    /// after the sky's), and which tables are drawn again. `post` is whether it is drawn into the
    /// post passes' target.
    ///
    /// # Errors
    /// What the device refuses.
    pub fn prepare(
        &mut self,
        d: &mut Device,
        light: &SkyLight,
        more: &[u8],
        post: bool,
    ) -> Result<SkyPlan, dereth_gfx::Error> {
        let tables = self.tables(d)?;
        let mut rows = rows(light);
        rows.extend_from_slice(more);
        self.constants.write(d, &rows)?;
        self.last_rows = rows;
        let tables_key = TablesKey([light.air.mie.to_bits(), light.air.rayleigh.to_bits()]);
        let sun = light.sun.normalize_or(Vec3::Z);
        let view_key = ViewKey {
            tables: tables_key,
            sun: (sun * 1800.0 / std::f32::consts::PI).to_array().map(whole),
            height: whole(light.eye.z / 5.0),
        };
        let draw_tables = self.tables_key != Some(tables_key);
        let draw_view = draw_tables || self.view_key != Some(view_key);
        self.tables_key = Some(tables_key);
        self.view_key = Some(view_key);
        self.tables_drawn += u64::from(draw_tables);
        self.views_drawn += u64::from(draw_view);
        Ok(SkyPlan {
            pipelines: self.pipelines,
            post,
            constants: self.constants.index,
            sampler: self.sampler.1,
            tables,
            draw_tables,
            draw_view,
        })
    }
}

impl SkyPlan {
    /// Draw the tables that changed.
    ///
    /// # Errors
    /// What the layer refuses.
    pub fn record_tables(&self, r: &mut Recorder<'_>) -> Result<(), dereth_gfx::Error> {
        let [transmittance, multiple, view] = self.tables;
        let table = |r: &mut Recorder<'_>,
                     name: &str,
                     target: Texture,
                     size: (u32, u32),
                     p: Pipeline,
                     reads: &[Texture]|
         -> Result<(), dereth_gfx::Error> {
            let touches: Vec<dereth_gfx::Touch> = reads
                .iter()
                .map(|t| dereth_gfx::Touch::Texture(*t, dereth_gfx::Use::Sampled))
                .collect();
            r.raster(
                &RasterPass {
                    name,
                    color: &[ColorAttachment {
                        texture: target,
                        load: Load::Discard,
                    }],
                    depth: None,
                    touches: &touches,
                    area: Some(Rect {
                        x: 0,
                        y: 0,
                        width: size.0,
                        height: size.1,
                    }),
                },
                |d| {
                    d.set_pipeline(p)?;
                    #[allow(clippy::cast_precision_loss)] // a table's size
                    let push = [
                        push_words(&[self.constants, transmittance.1, multiple.1, self.sampler]),
                        (size.0 as f32).to_le_bytes().to_vec(),
                        (size.1 as f32).to_le_bytes().to_vec(),
                    ]
                    .concat();
                    d.push(0, &push)?;
                    d.draw(3, 1, 0, 0);
                    Ok(())
                },
            )
        };
        if self.draw_tables {
            r.timestamp("sky tables");
            table(
                r,
                "sky transmittance",
                transmittance.0,
                TRANSMITTANCE,
                self.pipelines.transmittance,
                &[],
            )?;
            table(
                r,
                "sky multiple scattering",
                multiple.0,
                MULTIPLE,
                self.pipelines.multiple,
                &[transmittance.0],
            )?;
        }
        if self.draw_view {
            r.timestamp("sky view");
            table(
                r,
                "sky view",
                view.0,
                (SKY_VIEW.0, SKY_VIEW.1),
                self.pipelines.sky_view,
                &[transmittance.0, multiple.0],
            )?;
        }
        Ok(())
    }

    /// The sky's constants, by their index in the table.
    #[must_use]
    pub fn constants(&self) -> u32 {
        self.constants
    }

    /// What a pass that draws the sky touches of it: the sky-view table.
    #[must_use]
    pub fn view_table(&self) -> Texture {
        self.tables[2].0
    }

    /// What another pass reads to find the sky's own light any way it looks: the sky's
    /// constants, its sky-view table and the sampler it is read with, by their indices in the
    /// table, and the table itself.
    #[must_use]
    pub fn view_inputs(&self) -> ([u32; 3], Texture) {
        (
            [self.constants, self.tables[2].1, self.sampler],
            self.tables[2].0,
        )
    }

    /// Draw the physical sky over what is drawn.
    ///
    /// # Errors
    /// What the layer refuses.
    pub fn draw_sky(&self, d: &mut Draw<'_>) -> Result<(), dereth_gfx::Error> {
        self.draw_seen(d, self.constants, false)
    }

    /// Draw the veil over the horizon, last of the sky.
    ///
    /// # Errors
    /// What the layer refuses.
    pub fn draw_veil(&self, d: &mut Draw<'_>) -> Result<(), dereth_gfx::Error> {
        self.draw_seen(d, self.constants, true)
    }

    /// Draw the physical sky, or (`veil`) the veil, seen as the sky's constants `constants` say:
    /// for a view of the sky other than the frame's own.
    ///
    /// # Errors
    /// What the layer refuses.
    pub fn draw_seen(
        &self,
        d: &mut Draw<'_>,
        constants: u32,
        veil: bool,
    ) -> Result<(), dereth_gfx::Error> {
        self.draw_into(d, constants, veil, self.post)
    }

    /// [`Self::draw_seen`] into a target of the post passes' format (`post`) or of the frame's
    /// own, whichever the plan was made for: for a picture of the sky drawn apart from the frame.
    ///
    /// # Errors
    /// What the layer refuses.
    pub fn draw_into(
        &self,
        d: &mut Draw<'_>,
        constants: u32,
        veil: bool,
        post: bool,
    ) -> Result<(), dereth_gfx::Error> {
        let p = if veil {
            self.pipelines.veil[usize::from(post)]
        } else {
            self.pipelines.sky[usize::from(post)]
        };
        d.set_pipeline(p)?;
        d.push(0, &push_words(&[constants, self.tables[2].1, self.sampler]))?;
        d.draw(3, 1, 0, 0);
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    //! Behaviour: none (experimental)
    use super::*;

    fn light() -> SkyLight {
        SkyLight {
            view: Mat4::IDENTITY,
            projection: Mat4::perspective_rh(1.0, 1.5, 0.1, 1000.0),
            eye: Vec3::new(1.0, 2.0, 30.0),
            extent: (640, 480),
            sun: Vec3::new(0.3, 0.0, 0.9),
            sun_color: Vec3::new(1.0, 0.9, 0.8),
            twilight: 0.0,
            air: Air {
                mie: 1.0,
                rayleigh: 1.0,
                overcast: 0.0,
                exposure: 1.0,
            },
            day: 1.0,
            cover: 0.8,
            fog: None,
            linear: false,
            sky_veil: 0.0,
        }
    }

    fn row(bytes: &[u8], k: usize) -> [f32; 4] {
        let at = 64 + k * 16;
        std::array::from_fn(|i| {
            f32::from_le_bytes(
                bytes[at + i * 4..at + i * 4 + 4]
                    .try_into()
                    .unwrap_or([0; 4]),
            )
        })
    }

    /// The rows hold the ray's matrix and the sky's sixteen rows, the cover and the frame's
    /// linear light where the shaders read them, and a mirror's view replaces only the ray.
    #[test]
    fn the_rows_hold_the_cover_where_the_shaders_read_it_and_a_mirror_replaces_only_the_ray() {
        let mut l = light();
        let a = rows(&l);
        assert_eq!(a.len(), 64 + (ROWS - 4) * 16);
        assert_eq!(row(&a, 9), [0.8, 0.0, TOP_RADIUS_KM, 0.0]);
        l.linear = true;
        assert_eq!(row(&rows(&l), 9)[1], 1.0);
        let seen = seen_through(&a, Mat4::from_scale(Vec3::splat(2.0)));
        assert_eq!(seen[64..], a[64..]);
        assert_ne!(seen[..64], a[..64]);
    }

    /// The low sun tints the sky's light toward the sun's colour more than the high sun does.
    #[test]
    fn the_low_sun_tints_the_skys_light_more() {
        let warm = Vec3::new(1.0, 0.5, 0.2);
        let high = sun_illuminance(warm, twilight(40.0), 1.0);
        let low = sun_illuminance(warm, twilight(2.0), 1.0);
        assert!(low.z / low.x < high.z / high.x, "{high} {low}");
        assert!((high.x - EXPOSURE).abs() < 1e-5);
    }

    /// Minus zero keys as zero, so a sun crossing the axis doesn't redraw the table for nothing.
    #[test]
    fn minus_zero_keys_as_zero() {
        assert_eq!(whole(-0.3), whole(0.2));
        assert_ne!(whole(0.6), whole(0.2));
    }
}
