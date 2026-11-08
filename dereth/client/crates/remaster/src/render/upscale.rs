//! The upscaler: the world drawn at a lower resolution, each frame through a projection moved by a
//! different part of a pixel, and the frames gathered into a picture at the display's size, in
//! place of the temporal anti-aliasing. The post passes' bloom and tone map then run at the
//! display's size over what it leaves.
//!
//! Its upscaler is AMD's FidelityFX Super Resolution 3.1 (`shaders/fsr3.slang`, under its MIT
//! notice in `shaders/fsr3_LICENSE.txt`), or, in a build that asks for it and where its DLL stands
//! beside the client, NVIDIA's DLSS Super Resolution (`dlss.rs`), or, on a device over Metal,
//! MetalFX's temporal scaler (`metalfx.rs`). Off, the world is drawn at the display's size, and
//! the frame is the one drawn without it.
//!
//! - **The inputs.** `shaders/upscale_inputs.slang` writes each pixel's depth, from the outdoor
//!   surfaces' depth or the frame's by its class, and its motion, from the depth and the last
//!   frame's view, or from the post passes' motion target on the parts that moved.
//! - **The pictures it keeps** are storage buffers at the frame's current render and upscale sizes.
//!   A new size makes them again and starts it over.
//! - Every pipeline is made with the pass ([`UpscalePass::new`]) and none after.

use dereth_gfx::{
    Buffer, BufferDesc, BufferUsage, ColorAttachment, ColorTarget, ComputeDesc, Cull, Device,
    GraphicsDesc, Load, Location, Pipeline, RasterPass, Recorder, Rect, Texture, TextureDesc,
    TextureUsage, Touch, Use,
};

use super::light::Rows;
use super::post::{halton, LINEAR};
use super::{push_words, shader, PerFrame, Targets};
use crate::scene::Camera;
use crate::shaders::Entry;

/// Which upscaler draws the frame. The DLSS modes are the options of a device that has it; where
/// it does not, or for ray reconstruction, which is not drawn yet, they are drawn as FSR.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum UpscaleMode {
    /// None: the world is drawn at the display's size.
    #[default]
    Off,
    /// AMD's FidelityFX Super Resolution 3.1.
    Fsr,
    /// NVIDIA's super resolution.
    DlssSr,
    /// NVIDIA's ray reconstruction.
    DlssRr,
    /// MetalFX's temporal scaler, the option of a device over Metal; elsewhere drawn as FSR.
    MetalFx,
}

/// Which upscaler drew the last frame: 0 none, 1 FSR, 2 DLSS, 3 MetalFX. One remaster draws in a
/// process, so the process keeps it, for the report to read without the device.
static DRAWN: std::sync::atomic::AtomicU8 = std::sync::atomic::AtomicU8::new(0);

/// The upscaler that drew the last frame, by its name: `none`, `FSR`, `DLSS` or `MetalFX`.
#[must_use]
pub fn drawn_now() -> &'static str {
    drawn_name(DRAWN.load(std::sync::atomic::Ordering::Relaxed))
}

fn drawn_name(drawn: u8) -> &'static str {
    match drawn {
        1 => "FSR",
        2 => "DLSS",
        3 => "MetalFX",
        _ => "none",
    }
}

/// Note that `drawn` drew this frame, and say so when it is another than the last frame's.
fn note_drawn(drawn: u8) {
    let was = DRAWN.swap(drawn, std::sync::atomic::Ordering::Relaxed);
    if was != drawn {
        tracing::info!(target: "dereth::remaster", "the upscaler drawing: {}", drawn_name(drawn));
    }
}

/// How the frame is upscaled.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct UpscaleSettings {
    pub mode: UpscaleMode,
    /// The render size's share of the display's, each way, from a quarter to the whole.
    pub scale: f32,
    /// The sharpening, from 0 (none) to 1.
    pub sharpness: f32,
    /// Whether it takes little history where blended surfaces changed a pixel, such as the rain,
    /// which has no motion of its own for it to follow. Off is for comparing.
    pub reactive: bool,
}

impl Default for UpscaleSettings {
    fn default() -> Self {
        Self {
            mode: UpscaleMode::Off,
            scale: 1.0,
            sharpness: 0.5,
            reactive: true,
        }
    }
}

impl UpscaleSettings {
    /// Whether an upscaler draws the frame.
    #[must_use]
    pub fn runs(&self) -> bool {
        self.mode != UpscaleMode::Off
    }

    /// The size the world is drawn at for a display of `display`.
    #[must_use]
    pub fn render_size(&self, display: (u32, u32)) -> (u32, u32) {
        if !self.runs() {
            return display;
        }
        let scale = self.scale.clamp(0.25, 1.0);
        #[allow(
            clippy::cast_precision_loss,
            clippy::cast_possible_truncation,
            clippy::cast_sign_loss
        )] // a target's size, under the display's
        let one = |d: u32| ((d as f32 * scale).round() as u32).clamp(1, d.max(1));
        (one(display.0), one(display.1))
    }

    /// The levels a picture's detail is sharpened by for a display of `display`: drawn on fewer
    /// pixels, textures would otherwise take the smaller levels they take at the render size, and
    /// the display's pixels would have only that detail to gather.
    #[must_use]
    pub fn mip_bias(&self, display: (u32, u32)) -> f32 {
        if !self.runs() {
            return 0.0;
        }
        #[allow(clippy::cast_precision_loss)] // targets' widths
        let share = self.render_size(display).0 as f32 / display.0.max(1) as f32;
        dereth_primitives::num::math::logf(share) / core::f32::consts::LN_2
    }
}

/// The jitter sequence's length for a world `render` wide on a display `display` wide: eight, and
/// as many times more as the display has pixels for each drawn.
#[must_use]
pub fn phase_count(render: u32, display: u32) -> u32 {
    #[allow(clippy::cast_precision_loss)] // targets' widths
    let ratio = display as f32 / render.max(1) as f32;
    #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)] // small and positive
    let count = (8.0 * ratio * ratio) as u32;
    count.max(1)
}

/// How far the frame numbered `index` is jittered, in render pixels, `phases` the sequence's
/// length: inside the pixel.
#[must_use]
pub fn jitter_at(index: u32, phases: u32) -> [f32; 2] {
    let place = index % phases.max(1) + 1;
    [halton(place, 2) - 0.5, halton(place, 3) - 0.5]
}

/// A jitter in render pixels as a move of the projection in clip space, whose y runs the other way
/// from the pixels'.
#[must_use]
pub fn jitter_in_clip_space(jitter: [f32; 2], render: (u32, u32)) -> [f32; 2] {
    #[allow(clippy::cast_precision_loss)] // a target's size
    [
        2.0 * jitter[0] / render.0.max(1) as f32,
        -2.0 * jitter[1] / render.1.max(1) as f32,
    ]
}

/// What turns a depth on the device into a distance along the view, from the left-handed
/// projection it was drawn by: the distance is `[1] / (depth - [0])`. The other two undo the
/// projection's x and y.
#[must_use]
pub fn device_to_view(projection: glam::Mat4) -> [f32; 4] {
    [
        projection.z_axis.z,
        projection.w_axis.z,
        1.0 / projection.x_axis.x,
        1.0 / projection.y_axis.y,
    ]
}

/// One buffer the upscaler keeps a picture in, its index in the table and its size.
#[derive(Debug, Clone, Copy)]
struct Kept {
    buffer: Buffer,
    index: u32,
    bytes: u64,
}

/// The pictures the upscaler keeps, at its render and upscale sizes.
#[derive(Debug, Clone, Copy)]
struct Pictures {
    render: (u32, u32),
    display: (u32, u32),
    /// The format of the world's colors.
    color: dereth_gfx::Format,
    input_motion: Kept,
    input_depth: Kept,
    dilated_motion: Kept,
    dilated_depth: Kept,
    reconstructed: Kept,
    intermediate: Kept,
    farthest_half: Kept,
    luma: [Kept; 2],
    history: [Kept; 2],
    accumulation: [Kept; 2],
    reactive: Kept,
    shading_change: Kept,
    levels: [Kept; 3],
    new_locks: Kept,
    upscaled: [Kept; 2],
    output: Kept,
    /// How much of each pixel the blended surfaces changed, and the exposure found from the frame.
    input_reactive: Kept,
    exposure: Kept,
    /// The picture at the display's size that the bloom and the tone map read.
    shown: (Texture, u32),
    /// The world's colors as they stood before the blended surfaces.
    opaque: (Texture, u32),
    /// The depth and the motion as pictures, for an upscaler that takes them so.
    #[cfg(any(all(feature = "dlss", windows), target_os = "macos"))]
    input_pictures: (Bound2, Bound2),
}

/// A picture and its index in the table.
#[cfg(any(all(feature = "dlss", windows), target_os = "macos"))]
type Bound2 = (Texture, u32);

impl Pictures {
    fn make(
        gfx: &mut Device,
        render: (u32, u32),
        display: (u32, u32),
        color: dereth_gfx::Format,
    ) -> Result<Self, dereth_gfx::Error> {
        let opaque = gfx.create_texture(&TextureDesc {
            label: "remaster upscaler opaque",
            width: render.0,
            height: render.1,
            mips: 1,
            format: color,
            usage: TextureUsage::SAMPLED | TextureUsage::TRANSFER_DST,
        })?;
        let opaque = (opaque, gfx.bind_texture(opaque)?);
        #[cfg(any(all(feature = "dlss", windows), target_os = "macos"))]
        let input_pictures = {
            let mut make = |label, format| -> Result<Bound2, dereth_gfx::Error> {
                let t = gfx.create_texture(&TextureDesc {
                    label,
                    width: render.0,
                    height: render.1,
                    mips: 1,
                    format,
                    usage: TextureUsage::COLOR_TARGET | TextureUsage::SAMPLED,
                })?;
                Ok((t, gfx.bind_texture(t)?))
            };
            (
                make("remaster upscaler depth", dereth_gfx::Format::R32Float)?,
                make("remaster upscaler motion", LINEAR)?,
            )
        };
        let mut make = |size: (u32, u32), pixel: u64| -> Result<Kept, dereth_gfx::Error> {
            let bytes = (u64::from(size.0) * u64::from(size.1) * pixel).max(16);
            let buffer = gfx.create_buffer(&BufferDesc {
                label: "remaster upscaler",
                size: bytes,
                usage: BufferUsage::STORAGE | BufferUsage::TRANSFER_DST,
                location: Location::Gpu,
            })?;
            Ok(Kept {
                buffer,
                index: gfx.bind_buffer(buffer)?,
                bytes,
            })
        };
        let half = (render.0 / 2).max(1);
        let half = (half, (render.1 / 2).max(1));
        let level = |k: u32| ((half.0 >> k).max(1), (half.1 >> k).max(1));
        let pictures = Self {
            render,
            display,
            color,
            input_motion: make(render, 4)?,
            input_depth: make(render, 4)?,
            dilated_motion: make(render, 4)?,
            dilated_depth: make(render, 4)?,
            reconstructed: make(render, 4)?,
            intermediate: make(render, 4)?,
            farthest_half: make(half, 4)?,
            luma: [make(render, 4)?, make(render, 4)?],
            history: [make(render, 8)?, make(render, 8)?],
            accumulation: [make(render, 4)?, make(render, 4)?],
            reactive: make(render, 8)?,
            shading_change: make(half, 4)?,
            levels: [make(level(0), 4)?, make(level(1), 4)?, make(level(2), 4)?],
            new_locks: make(display, 4)?,
            upscaled: [make(display, 8)?, make(display, 8)?],
            output: make(display, 8)?,
            input_reactive: make(render, 4)?,
            exposure: make((1, 1), 16)?,
            opaque,
            #[cfg(any(all(feature = "dlss", windows), target_os = "macos"))]
            input_pictures,
            shown: {
                let texture = gfx.create_texture(&TextureDesc {
                    label: "remaster upscaled",
                    width: display.0,
                    height: display.1,
                    mips: 1,
                    format: LINEAR,
                    usage: TextureUsage::COLOR_TARGET
                        | TextureUsage::SAMPLED
                        | TextureUsage::TRANSFER_SRC
                        | if cfg!(any(all(feature = "dlss", windows), target_os = "macos")) {
                            TextureUsage::STORAGE
                        } else {
                            TextureUsage::SAMPLED
                        },
                })?;
                (texture, gfx.bind_texture(texture)?)
            },
        };
        Ok(pictures)
    }

    /// Every buffer.
    fn all(&self) -> Vec<Kept> {
        let mut v = vec![
            self.input_motion,
            self.input_depth,
            self.dilated_motion,
            self.dilated_depth,
            self.reconstructed,
            self.intermediate,
            self.farthest_half,
            self.reactive,
            self.shading_change,
            self.new_locks,
            self.output,
            self.input_reactive,
            self.exposure,
        ];
        for pair in [self.luma, self.history, self.accumulation, self.upscaled] {
            v.extend(pair);
        }
        v.extend(self.levels);
        v
    }

    fn destroy(&self, gfx: &mut Device) {
        for k in self.all() {
            gfx.destroy_buffer(k.buffer);
        }
        gfx.destroy_texture(self.shown.0);
        gfx.destroy_texture(self.opaque.0);
        #[cfg(any(all(feature = "dlss", windows), target_os = "macos"))]
        {
            gfx.destroy_texture(self.input_pictures.0 .0);
            gfx.destroy_texture(self.input_pictures.1 .0);
        }
    }
}

/// The upscaler's pipelines.
#[derive(Debug, Clone, Copy)]
struct Pipelines {
    inputs: Pipeline,
    clear: Pipeline,
    prepare: Pipeline,
    farthest: Pipeline,
    level_zero: Pipeline,
    level: Pipeline,
    change: Pipeline,
    reactivity: Pipeline,
    instability: Pipeline,
    accumulate: Pipeline,
    rcas: Pipeline,
    show: Pipeline,
    reactive_input: Pipeline,
    exposure: Pipeline,
    /// The depth and the motion written as pictures, for a scaler outside the layer: made with
    /// MetalFX's, and once DLSS's library is found (FSR, which draws without one, never needs it).
    #[cfg(any(all(feature = "dlss", windows), target_os = "macos"))]
    input_pictures: Option<Pipeline>,
}

/// The pipeline that writes the depth and the motion as pictures.
#[cfg(any(all(feature = "dlss", windows), target_os = "macos"))]
fn input_pictures_pipeline(gfx: &mut Device) -> Result<Pipeline, dereth_gfx::Error> {
    gfx.create_graphics_pipeline(&GraphicsDesc {
        label: "remaster upscaler inputs as pictures",
        vertex: shader(Entry::FullscreenVsMain),
        fragment: Some(shader(Entry::Fsr3PassesFsInputsAsPictures)),
        color: &[dereth_gfx::Format::R32Float, LINEAR].map(|format| ColorTarget {
            format,
            blend: None,
            write_alpha: true,
        }),
        depth: None,
        cull: Cull::None,
    })
}

/// The upscaler.
#[derive(Debug)]
pub struct UpscalePass {
    /// The driver's DLSS library, when it was found, and whether it has been looked for.
    #[cfg(all(feature = "dlss", windows))]
    ngx: Option<std::sync::Arc<super::dlss::Ngx>>,
    #[cfg(all(feature = "dlss", windows))]
    ngx_tried: bool,
    /// MetalFX's scaler, while one fits, and whether the device refused one.
    #[cfg(target_os = "macos")]
    scaler: Option<super::metalfx::Scaler>,
    #[cfg(target_os = "macos")]
    scaler_refused: bool,
    /// The frames it has scaled.
    #[cfg(target_os = "macos")]
    scaled: u64,
    settings: UpscaleSettings,
    pipelines: Pipelines,
    constants: PerFrame,
    pictures: Option<Pictures>,
    /// Whether what it has gathered is another picture's: the settings changed.
    start_over: bool,
    /// Frames since it last started over, and frames it has drawn in all.
    frame: u32,
    frames: u64,
    /// This frame's and the last frame's jitter, in render pixels, and the sequence's place.
    jitter: [f32; 2],
    last_jitter: [f32; 2],
    place: u32,
    phases: u32,
}

/// What the upscaler takes from the post passes each frame.
#[derive(Debug, Clone, Copy)]
pub(crate) struct UpscaleInputs {
    /// The scene's colors, by their index in the table, and themselves.
    pub color: u32,
    #[cfg(any(all(feature = "dlss", windows), target_os = "macos"))]
    pub color_texture: Texture,
    /// Whether there is no last frame to gather from.
    pub fresh: bool,
    /// Whether the colors are linear light and unclamped.
    pub hdr: bool,
}

/// One frame of the upscaler, as [`UpscalePass::prepare`] made it.
#[derive(Debug, Clone)]
pub struct UpscalePlan {
    pipelines: Pipelines,
    pictures: Pictures,
    constants: u32,
    /// The pictures kept by turns this frame writes; it reads the others.
    write: usize,
    /// Whether it starts over this frame.
    start_over: bool,
    sharpens: bool,
    reactive: bool,
    /// Whether DLSS draws this frame, and what it was told of it.
    #[cfg(all(feature = "dlss", windows))]
    dlss: Option<DlssFrame>,
    /// Whether MetalFX scales this frame, and what it reads and writes.
    #[cfg(target_os = "macos")]
    metalfx: Option<super::metalfx::Images>,
    #[cfg(any(all(feature = "dlss", windows), target_os = "macos"))]
    jitter: [f32; 2],
}

/// What DLSS is told of a frame.
#[cfg(all(feature = "dlss", windows))]
#[derive(Debug, Clone)]
struct DlssFrame {
    ngx: std::sync::Arc<super::dlss::Ngx>,
    hdr: bool,
    reset: bool,
    images: super::dlss::Images,
}

impl UpscalePass {
    /// The pass on `gfx`, its pipelines made, off.
    ///
    /// # Errors
    /// What the device refuses.
    pub fn new(gfx: &mut Device) -> Result<Self, dereth_gfx::Error> {
        let mut compute = |label, e| {
            gfx.create_compute_pipeline(&ComputeDesc {
                label,
                shader: shader(e),
            })
        };
        let inputs = compute("remaster upscaler inputs", Entry::UpscaleInputsCsMain)?;
        let clear = compute(
            "remaster upscaler clear",
            Entry::Fsr3PassesCsClearReconstructed,
        )?;
        let prepare = compute(
            "remaster upscaler prepare",
            Entry::Fsr3PassesCsPrepareInputs,
        )?;
        let farthest = compute(
            "remaster upscaler farthest",
            Entry::Fsr3PassesCsFarthestHalf,
        )?;
        let level_zero = compute(
            "remaster upscaler shading level zero",
            Entry::Fsr3PassesCsShadingLevelZero,
        )?;
        let level = compute(
            "remaster upscaler shading level",
            Entry::Fsr3PassesCsShadingLevel,
        )?;
        let change = compute(
            "remaster upscaler shading change",
            Entry::Fsr3PassesCsShadingChange,
        )?;
        let reactivity = compute(
            "remaster upscaler reactivity",
            Entry::Fsr3PassesCsReactivity,
        )?;
        let instability = compute(
            "remaster upscaler instability",
            Entry::Fsr3PassesCsLumaInstability,
        )?;
        let reactive_input = compute(
            "remaster upscaler reactive mask",
            Entry::Fsr3PassesCsReactiveInput,
        )?;
        let exposure = compute("remaster upscaler exposure", Entry::Fsr3PassesCsExposure)?;
        let accumulate = compute(
            "remaster upscaler accumulate",
            Entry::Fsr3PassesCsAccumulate,
        )?;
        let rcas = compute("remaster upscaler sharpen", Entry::Fsr3PassesCsRcas)?;
        let show = gfx.create_graphics_pipeline(&GraphicsDesc {
            label: "remaster upscaler show",
            vertex: shader(Entry::FullscreenVsMain),
            fragment: Some(shader(Entry::Fsr3PassesFsShow)),
            color: &[ColorTarget {
                format: LINEAR,
                blend: None,
                write_alpha: true,
            }],
            depth: None,
            cull: Cull::None,
        })?;
        #[cfg(target_os = "macos")]
        let input_pictures = Some(input_pictures_pipeline(gfx)?);
        #[cfg(all(feature = "dlss", windows))]
        let input_pictures = None;
        Ok(Self {
            #[cfg(all(feature = "dlss", windows))]
            ngx: None,
            #[cfg(all(feature = "dlss", windows))]
            ngx_tried: false,
            #[cfg(target_os = "macos")]
            scaler: None,
            #[cfg(target_os = "macos")]
            scaler_refused: false,
            #[cfg(target_os = "macos")]
            scaled: 0,
            settings: UpscaleSettings::default(),
            pipelines: Pipelines {
                inputs,
                clear,
                prepare,
                farthest,
                level_zero,
                level,
                change,
                reactivity,
                reactive_input,
                exposure,
                #[cfg(any(all(feature = "dlss", windows), target_os = "macos"))]
                input_pictures,
                instability,
                accumulate,
                rcas,
                show,
            },
            constants: PerFrame::new(
                gfx,
                "remaster upscaler constants",
                256,
                BufferUsage::default(),
            )?,
            pictures: None,
            start_over: true,
            frame: 0,
            frames: 0,
            jitter: [0.0; 2],
            last_jitter: [0.0; 2],
            place: 0,
            phases: 8,
        })
    }

    /// How the frame is upscaled from the next frame on.
    pub fn set_settings(&mut self, settings: UpscaleSettings) {
        if settings != self.settings {
            self.start_over = true;
        }
        self.settings = settings;
        if !settings.runs() {
            note_drawn(0);
        }
    }

    /// How the frame is upscaled.
    #[must_use]
    pub fn settings(&self) -> UpscaleSettings {
        self.settings
    }

    /// Frames it has drawn, since the renderer was made.
    #[must_use]
    pub fn frames(&self) -> u64 {
        self.frames
    }

    /// The next jitter for a world of `render` on a display of `display`, as a move of the
    /// projection in clip space; `None` while it is off.
    pub fn next_jitter(&mut self, render: (u32, u32), display: (u32, u32)) -> Option<[f32; 2]> {
        if !self.settings.runs() {
            return None;
        }
        self.phases = phase_count(render.0, display.0);
        self.last_jitter = self.jitter;
        self.jitter = jitter_at(self.place, self.phases);
        self.place = self.place.wrapping_add(1);
        Some(jitter_in_clip_space(self.jitter, render))
    }

    /// Off, what it keeps is let go.
    pub fn idle(&mut self, gfx: &mut Device) {
        #[cfg(target_os = "macos")]
        {
            self.scaler = None;
        }
        if let Some(p) = self.pictures.take() {
            p.destroy(gfx);
        }
        self.start_over = true;
    }

    /// MetalFX's images for this frame, a scaler that fits them made, or `None` where the frame is
    /// not its to scale: another mode, or a device that refuses one, whose frames FSR draws.
    #[cfg(target_os = "macos")]
    fn metalfx_images(
        &mut self,
        gfx: &Device,
        color: Texture,
        pictures: &Pictures,
    ) -> Option<super::metalfx::Images> {
        if self.settings.mode != UpscaleMode::MetalFx || self.scaler_refused {
            return None;
        }
        let images = super::metalfx::Images {
            color,
            depth: pictures.input_pictures.0 .0,
            motion: pictures.input_pictures.1 .0,
            output: pictures.shown.0,
            input: pictures.render,
            size: pictures.display,
        };
        if !self.scaler.as_ref().is_some_and(|s| s.fits(&images)) {
            self.scaler = None;
            match super::metalfx::Scaler::new(gfx, &images) {
                Ok(s) => {
                    tracing::info!(
                        target: "dereth::remaster",
                        "MetalFX scales {:?} up to {:?}",
                        images.input,
                        images.size
                    );
                    self.scaler = Some(s);
                }
                Err(e) => {
                    tracing::warn!(target: "dereth::remaster", "no MetalFX, FSR draws the frames: {e}");
                    self.scaler_refused = true;
                    return None;
                }
            }
        }
        Some(images)
    }

    /// Scale the frame `plan` recorded, once `done`, what it recorded before the scaler, is done.
    ///
    /// # Errors
    /// What the device refuses.
    #[cfg(target_os = "macos")]
    pub(crate) fn run_scaler(
        &mut self,
        gfx: &Device,
        plan: &UpscalePlan,
        done: dereth_gfx::Ticket,
    ) -> Result<(), dereth_gfx::Error> {
        if let (Some(s), Some(images)) = (&mut self.scaler, &plan.metalfx) {
            s.encode(gfx, images, plan.jitter, plan.start_over, done.value())?;
            self.scaled += 1;
        }
        Ok(())
    }

    /// The frame's constants and the plan its passes are recorded from: `camera` is the frame's
    /// camera with its projection unmoved, `t` the world's targets at the render size, `display`
    /// the display's size and `inputs` what the post passes hand it.
    ///
    /// # Errors
    /// What the device refuses.
    pub(crate) fn prepare(
        &mut self,
        gfx: &mut Device,
        camera: &Camera,
        t: &Targets,
        display: (u32, u32),
        inputs: UpscaleInputs,
    ) -> Result<UpscalePlan, dereth_gfx::Error> {
        let render = (t.width, t.height);
        let color = super::post::scene_format(gfx);
        let pictures = match self.pictures {
            Some(p) if p.render == render && p.display == display && p.color == color => p,
            old => {
                if let Some(p) = old {
                    p.destroy(gfx);
                }
                let p = Pictures::make(gfx, render, display, color)?;
                self.pictures = Some(p);
                self.start_over = true;
                p
            }
        };
        let start_over = self.start_over || inputs.fresh;
        self.start_over = false;
        if start_over {
            self.frame = 0;
            // Nothing was jittered before it for this frame to be compared with.
            self.last_jitter = self.jitter;
        } else {
            self.frame = self.frame.wrapping_add(1);
        }
        self.frames += 1;
        let sharpens = self.settings.sharpness > 0.0;
        let write = (self.frame % 2) as usize;
        let read = 1 - write;
        // The current picture's turn is the frame's, so a start over begins each from the same
        // place.
        let shown = if sharpens {
            pictures.output
        } else {
            pictures.upscaled[write]
        };
        let size = |s: (u32, u32)| [f32::from_bits(s.0), f32::from_bits(s.1)];
        let (r, d) = (size(render), size(display));
        let f = device_to_view(camera.projection);
        #[allow(clippy::cast_precision_loss)] // targets' sizes, and small counts
        let (downscale, phases, frame) = (
            [
                render.0 as f32 / display.0 as f32,
                render.1 as f32 / display.1 as f32,
            ],
            self.phases as f32,
            self.frame as f32,
        );
        // The SDK's sharpness: 0 stops down at none and 2 at the most it allows.
        let stops = -2.0 * self.settings.sharpness.clamp(0.0, 1.0) + 2.0;
        let rcas = dereth_primitives::num::math::powf(2.0, -stops);
        let mut rows = Rows::default();
        let mut row = |v: [f32; 4]| rows.0.push(v.map(f32::to_bits));
        row([r[0], r[1], r[0], r[1]]);
        row([d[0], d[1], d[0], d[1]]);
        row(f);
        row([
            self.jitter[0],
            self.jitter[1],
            self.last_jitter[0],
            self.last_jitter[1],
        ]);
        row([1.0, 1.0, downscale[0], downscale[1]]);
        row([0.0, 0.0, 0.0, phases]);
        row([1.0, 1.0, frame, if inputs.hdr { 1.0 } else { 0.0 }]);
        row([1.0, 1.0 / 3.0, -1.0 / 3.0, rcas]);
        row([0.0; 4]);
        for words in [
            [
                inputs.color,
                pictures.input_motion.index,
                pictures.input_depth.index,
                pictures.dilated_motion.index,
            ],
            [
                pictures.dilated_depth.index,
                pictures.reconstructed.index,
                pictures.intermediate.index,
                pictures.farthest_half.index,
            ],
            [
                pictures.luma[write].index,
                pictures.luma[read].index,
                pictures.history[write].index,
                pictures.history[read].index,
            ],
            [
                pictures.accumulation[write].index,
                pictures.accumulation[read].index,
                pictures.reactive.index,
                pictures.shading_change.index,
            ],
            [
                pictures.levels[0].index,
                pictures.levels[1].index,
                pictures.levels[2].index,
                pictures.new_locks.index,
            ],
            [
                pictures.upscaled[write].index,
                pictures.upscaled[read].index,
                pictures.output.index,
                shown.index,
            ],
            [
                pictures.opaque.1,
                pictures.input_reactive.index,
                pictures.exposure.index,
                0,
            ],
        ] {
            rows.0.push(words);
        }
        self.constants.write(gfx, &rows.bytes())?;
        #[cfg(all(feature = "dlss", windows))]
        let dlss = {
            if self.settings.mode == UpscaleMode::DlssSr && !self.ngx_tried {
                self.ngx_tried = true;
                if let (Some((instance, physical)), Some(device)) = (gfx.raw_instance(), gfx.raw())
                {
                    use ash::vk::Handle as _;
                    let data = std::env::temp_dir();
                    self.ngx = super::dlss::Ngx::on_device(
                        instance,
                        physical,
                        ash::vk::Device::from_raw(device.handle().as_raw()),
                        &data,
                    );
                }
                if self.ngx.is_some() && self.pipelines.input_pictures.is_none() {
                    self.pipelines.input_pictures = Some(input_pictures_pipeline(gfx)?);
                }
            }
            let image = |t: Texture| -> Option<super::dlss::Image> {
                gfx.raw_texture(t)
                    .ok()
                    .flatten()
                    .map(|(image, view, format, size)| super::dlss::Image {
                        image,
                        view,
                        format,
                        size,
                    })
            };
            match (&self.ngx, self.settings.mode, self.pipelines.input_pictures) {
                (Some(ngx), UpscaleMode::DlssSr, Some(_)) => Some(DlssFrame {
                    ngx: std::sync::Arc::clone(ngx),
                    hdr: inputs.hdr,
                    reset: start_over,
                    images: super::dlss::Images {
                        color: image(inputs.color_texture).expect("the frame's own colors"),
                        output: image(pictures.shown.0).expect("the frame's own picture"),
                        depth: image(pictures.input_pictures.0 .0).expect("its own depth"),
                        motion: image(pictures.input_pictures.1 .0).expect("its own motion"),
                    },
                }),
                _ => None,
            }
        };
        #[cfg(target_os = "macos")]
        let metalfx = self.metalfx_images(gfx, inputs.color_texture, &pictures);
        #[allow(unused_mut)]
        let mut drawn = 1;
        #[cfg(all(feature = "dlss", windows))]
        if dlss.is_some() {
            drawn = 2;
        }
        #[cfg(target_os = "macos")]
        if metalfx.is_some() {
            drawn = 3;
        }
        note_drawn(drawn);
        Ok(UpscalePlan {
            pipelines: self.pipelines,
            pictures,
            constants: self.constants.index,
            write,
            start_over,
            sharpens,
            reactive: self.settings.reactive,
            #[cfg(all(feature = "dlss", windows))]
            dlss,
            #[cfg(target_os = "macos")]
            metalfx,
            #[cfg(any(all(feature = "dlss", windows), target_os = "macos"))]
            jitter: self.jitter,
        })
    }
}

impl UpscalePlan {
    /// Whether a scaler outside the layer brings this frame up to the display's size.
    #[cfg(target_os = "macos")]
    #[must_use]
    pub fn scales_outside(&self) -> bool {
        self.metalfx.is_some()
    }

    /// The picture at the display's size, by itself and its index in the table.
    #[must_use]
    pub fn output(&self) -> (Texture, u32) {
        self.pictures.shown
    }

    /// One compute pass: its pipeline over a picture of `size`, by groups of eight.
    fn pass(
        &self,
        r: &mut Recorder<'_>,
        pipeline: Pipeline,
        touches: &[Touch],
        size: (u32, u32),
        level: u32,
    ) -> Result<(), dereth_gfx::Error> {
        r.compute(touches, |c| {
            c.set_pipeline(pipeline)?;
            c.push(0, &push_words(&[self.constants, level]))?;
            c.dispatch(size.0.div_ceil(8).max(1), size.1.div_ceil(8).max(1), 1);
            Ok(())
        })
    }

    /// Keep the world's colors as they stand before the blended surfaces are drawn over them.
    ///
    /// # Errors
    /// What the device refuses.
    pub(crate) fn keep_opaque(
        &self,
        r: &mut Recorder<'_>,
        t: &Targets,
    ) -> Result<(), dereth_gfx::Error> {
        r.copy_texture(t.color, self.pictures.opaque.0)
    }

    /// Record the upscaler's passes. `t` are the frame's targets with the scene's colors the post
    /// passes' own; `frame` the frame's constants' index, `post` the post passes' and
    /// `outdoor_depth` the depth the outdoors are placed by; `motion` the moving parts' last
    /// places, where any moved.
    ///
    /// # Errors
    /// What the device refuses.
    pub fn record(
        &self,
        r: &mut Recorder<'_>,
        t: &Targets,
        frame: u32,
        post: u32,
        outdoor_depth: (Texture, u32),
        motion: Option<(Texture, u32)>,
    ) -> Result<(), dereth_gfx::Error> {
        #[cfg(all(feature = "dlss", windows))]
        if let Some(d) = &self.dlss {
            return self.record_dlss(d, r, t, frame, post, outdoor_depth, motion);
        }
        #[cfg(target_os = "macos")]
        if self.metalfx.is_some() {
            return self.record_for_metalfx(r, t, frame, post, outdoor_depth, motion);
        }
        use Use::{StorageRead as Read, StorageWrite as Write};
        let p = &self.pictures;
        let write = self.write;
        let read = 1 - write;
        let touch = |k: Kept, u: Use| Touch::Buffer(k.buffer, u);
        let render = p.render;
        let display = p.display;
        let half = ((render.0 / 2).max(1), (render.1 / 2).max(1));
        let level = |k: u32| ((half.0 >> k).max(1), (half.1 >> k).max(1));
        r.timestamp("upscale");
        if self.start_over {
            for k in p.all() {
                r.fill_buffer(k.buffer, 0, k.bytes, 0)?;
            }
        }
        // What it takes from the frame.
        let mut touches = vec![
            Touch::Texture(t.depth, Use::Sampled),
            Touch::Texture(outdoor_depth.0, Use::Sampled),
            Touch::Texture(t.material, Use::Sampled),
            touch(p.input_motion, Write),
            touch(p.input_depth, Write),
        ];
        if let Some(m) = motion {
            touches.push(Touch::Texture(m.0, Use::Sampled));
        }
        r.compute(&touches, |c| {
            c.set_pipeline(self.pipelines.inputs)?;
            c.push(
                0,
                &push_words(&[
                    frame,
                    post,
                    t.index[4],
                    outdoor_depth.1,
                    t.index[3],
                    motion.map_or(super::scene::NONE, |m| m.1),
                    p.input_motion.index,
                    p.input_depth.index,
                ]),
            )?;
            c.dispatch(render.0.div_ceil(8).max(1), render.1.div_ceil(8).max(1), 1);
            Ok(())
        })?;
        let f = self.pipelines;
        if self.reactive {
            self.pass(
                r,
                f.reactive_input,
                &[
                    Touch::Texture(t.color, Use::Sampled),
                    Touch::Texture(p.opaque.0, Use::Sampled),
                    touch(p.input_reactive, Write),
                ],
                render,
                0,
            )?;
        }
        r.compute(
            &[
                Touch::Texture(t.color, Use::Sampled),
                touch(p.exposure, Write),
            ],
            |c| {
                c.set_pipeline(f.exposure)?;
                c.push(0, &push_words(&[self.constants, 0]))?;
                c.dispatch(1, 1, 1);
                Ok(())
            },
        )?;
        // Every pass from here on reads the exposure, and the show after them.
        r.compute(&[touch(p.exposure, Read)], |_| Ok(()))?;
        self.pass(r, f.clear, &[touch(p.reconstructed, Write)], render, 0)?;
        self.pass(
            r,
            f.prepare,
            &[
                Touch::Texture(t.color, Use::Sampled),
                touch(p.input_motion, Read),
                touch(p.input_depth, Read),
                touch(p.dilated_motion, Write),
                touch(p.dilated_depth, Write),
                touch(p.reconstructed, Write),
                touch(p.intermediate, Write),
                touch(p.luma[write], Write),
            ],
            render,
            0,
        )?;
        self.pass(
            r,
            f.farthest,
            &[touch(p.intermediate, Read), touch(p.farthest_half, Write)],
            half,
            0,
        )?;
        self.pass(
            r,
            f.level_zero,
            &[
                touch(p.dilated_motion, Read),
                touch(p.luma[write], Read),
                touch(p.luma[read], Read),
                touch(p.levels[0], Write),
            ],
            level(0),
            0,
        )?;
        for k in 1..3u32 {
            self.pass(
                r,
                f.level,
                &[
                    touch(p.levels[k as usize - 1], Read),
                    touch(p.levels[k as usize], Write),
                ],
                level(k),
                k,
            )?;
        }
        self.pass(
            r,
            f.change,
            &[
                touch(p.levels[0], Read),
                touch(p.levels[1], Read),
                touch(p.levels[2], Read),
                touch(p.shading_change, Write),
            ],
            half,
            0,
        )?;
        self.pass(
            r,
            f.reactivity,
            &[
                touch(p.dilated_motion, Read),
                touch(p.dilated_depth, Read),
                touch(p.reconstructed, Read),
                touch(p.shading_change, Read),
                touch(p.input_reactive, Read),
                touch(p.accumulation[read], Read),
                touch(p.luma[write], Read),
                touch(p.accumulation[write], Write),
                touch(p.reactive, Write),
                touch(p.new_locks, Write),
            ],
            render,
            0,
        )?;
        self.pass(
            r,
            f.instability,
            &[
                touch(p.dilated_motion, Read),
                touch(p.reactive, Read),
                touch(p.luma[write], Read),
                touch(p.history[read], Read),
                touch(p.history[write], Write),
                touch(p.intermediate, Write),
            ],
            render,
            0,
        )?;
        self.pass(
            r,
            f.accumulate,
            &[
                Touch::Texture(t.color, Use::Sampled),
                touch(p.dilated_motion, Read),
                touch(p.reactive, Read),
                touch(p.intermediate, Read),
                touch(p.farthest_half, Read),
                touch(p.upscaled[read], Read),
                touch(p.upscaled[write], Write),
                touch(p.new_locks, Write),
            ],
            display,
            0,
        )?;
        let shown = if self.sharpens {
            self.pass(
                r,
                f.rcas,
                &[touch(p.upscaled[write], Read), touch(p.output, Write)],
                display,
                0,
            )?;
            p.output
        } else {
            p.upscaled[write]
        };
        // Into the texture the bloom and the tone map read.
        r.raster(
            &RasterPass {
                name: "upscaled",
                color: &[ColorAttachment {
                    texture: p.shown.0,
                    load: Load::Discard,
                }],
                depth: None,
                touches: &[touch(shown, Read)],
                area: Some(Rect {
                    x: 0,
                    y: 0,
                    width: display.0,
                    height: display.1,
                }),
            },
            |d| {
                d.set_pipeline(f.show)?;
                d.push(0, &push_words(&[self.constants, 0]))?;
                d.draw(3, 1, 0, 0);
                Ok(())
            },
        )
    }
}

#[cfg(any(all(feature = "dlss", windows), target_os = "macos"))]
impl UpscalePlan {
    /// The depth and the motion, worked out and written as pictures.
    fn record_input_pictures(
        &self,
        r: &mut Recorder<'_>,
        t: &Targets,
        frame: u32,
        post: u32,
        outdoor_depth: (Texture, u32),
        motion: Option<(Texture, u32)>,
    ) -> Result<(Bound2, Bound2), dereth_gfx::Error> {
        let p = &self.pictures;
        let render = p.render;
        r.timestamp("upscale");
        let touch = |k: Kept, u: Use| Touch::Buffer(k.buffer, u);
        let mut touches = vec![
            Touch::Texture(t.depth, Use::Sampled),
            Touch::Texture(outdoor_depth.0, Use::Sampled),
            Touch::Texture(t.material, Use::Sampled),
            touch(p.input_motion, Use::StorageWrite),
            touch(p.input_depth, Use::StorageWrite),
        ];
        if let Some(m) = motion {
            touches.push(Touch::Texture(m.0, Use::Sampled));
        }
        r.compute(&touches, |c| {
            c.set_pipeline(self.pipelines.inputs)?;
            c.push(
                0,
                &push_words(&[
                    frame,
                    post,
                    t.index[4],
                    outdoor_depth.1,
                    t.index[3],
                    motion.map_or(super::scene::NONE, |m| m.1),
                    p.input_motion.index,
                    p.input_depth.index,
                ]),
            )?;
            c.dispatch(render.0.div_ceil(8).max(1), render.1.div_ceil(8).max(1), 1);
            Ok(())
        })?;
        let (depth, moved) = p.input_pictures;
        let Some(pipeline) = self.pipelines.input_pictures else {
            return Ok((depth, moved));
        };
        r.raster(
            &RasterPass {
                name: "upscaler inputs",
                color: &[
                    ColorAttachment {
                        texture: depth.0,
                        load: Load::Discard,
                    },
                    ColorAttachment {
                        texture: moved.0,
                        load: Load::Discard,
                    },
                ],
                depth: None,
                touches: &[
                    touch(p.input_motion, Use::StorageRead),
                    touch(p.input_depth, Use::StorageRead),
                ],
                area: Some(Rect {
                    x: 0,
                    y: 0,
                    width: render.0,
                    height: render.1,
                }),
            },
            |c| {
                c.set_pipeline(pipeline)?;
                c.push(0, &push_words(&[self.constants, 0]))?;
                c.draw(3, 1, 0, 0);
                Ok(())
            },
        )?;
        Ok((depth, moved))
    }

    /// What a scaler outside the layer wants of the images: the inputs read by shaders, the output
    /// general.
    fn hand_over_images(
        &self,
        r: &mut Recorder<'_>,
        t: &Targets,
        (depth, moved): (Bound2, Bound2),
    ) -> Result<(), dereth_gfx::Error> {
        let general = dereth_gfx::State {
            layout: dereth_gfx::vk::ImageLayout::GENERAL,
            stages: dereth_gfx::vk::PipelineStageFlags2::ALL_COMMANDS,
            access: dereth_gfx::vk::AccessFlags2::MEMORY_READ
                | dereth_gfx::vk::AccessFlags2::MEMORY_WRITE,
        };
        r.compute(
            &[
                Touch::Texture(t.color, Use::Sampled),
                Touch::Texture(depth.0, Use::Sampled),
                Touch::Texture(moved.0, Use::Sampled),
                Touch::Texture(self.pictures.shown.0, Use::External(general)),
            ],
            |_| Ok(()),
        )
    }
}

#[cfg(target_os = "macos")]
impl UpscalePlan {
    /// Record what comes before MetalFX's scaler, which [`UpscalePass::run_scaler`] runs after it.
    fn record_for_metalfx(
        &self,
        r: &mut Recorder<'_>,
        t: &Targets,
        frame: u32,
        post: u32,
        outdoor_depth: (Texture, u32),
        motion: Option<(Texture, u32)>,
    ) -> Result<(), dereth_gfx::Error> {
        let pictures = self.record_input_pictures(r, t, frame, post, outdoor_depth, motion)?;
        self.hand_over_images(r, t, pictures)
    }
}

#[cfg(all(feature = "dlss", windows))]
impl UpscalePlan {
    /// Record a frame of DLSS: the depth and the motion as pictures, then the library's own work,
    /// into the picture the bloom and the tone map read.
    #[allow(clippy::too_many_arguments)] // what the upscaler's own record takes, and the frame
    fn record_dlss(
        &self,
        d: &DlssFrame,
        r: &mut Recorder<'_>,
        t: &Targets,
        frame: u32,
        post: u32,
        outdoor_depth: (Texture, u32),
        motion: Option<(Texture, u32)>,
    ) -> Result<(), dereth_gfx::Error> {
        let pictures = self.record_input_pictures(r, t, frame, post, outdoor_depth, motion)?;
        self.hand_over_images(r, t, pictures)?;
        let p = &self.pictures;
        if let Some(cmd) = r.raw() {
            d.ngx.draw(
                cmd,
                p.render,
                p.display,
                d.hdr,
                d.images,
                self.jitter,
                d.reset,
            );
        }
        Ok(())
    }
}

impl super::Renderer {
    /// Run the scaler outside the layer that `plan`'s frame goes through, once `done`, what the
    /// frame recorded before it, is done.
    ///
    /// # Errors
    /// What the device refuses.
    #[cfg(target_os = "macos")]
    pub fn run_scaler(
        &mut self,
        plan: &super::record::Plan,
        done: dereth_gfx::Ticket,
    ) -> Result<(), dereth_gfx::Error> {
        match plan.upscale_plan() {
            Some(u) => self.upscale.run_scaler(&self.gfx, u, done),
            None => Ok(()),
        }
    }

    /// The frames MetalFX's scaler has scaled: none where there is no Metal.
    #[must_use]
    pub fn metalfx_frames(&self) -> u64 {
        #[cfg(target_os = "macos")]
        {
            self.upscale.scaled
        }
        #[cfg(not(target_os = "macos"))]
        {
            0
        }
    }

    /// How the frame is upscaled from the next frame on.
    pub fn set_upscale(&mut self, settings: UpscaleSettings) {
        self.upscale.set_settings(settings);
    }

    /// How the frame is upscaled.
    #[must_use]
    pub fn upscale(&self) -> UpscaleSettings {
        self.upscale.settings()
    }

    /// Frames the upscaler drew, since the renderer was made.
    #[must_use]
    pub fn upscaled_frames(&self) -> u64 {
        self.upscale.frames()
    }
}

#[cfg(test)]
mod tests {
    //! Behaviour: none (experimental)
    use super::*;

    /// Off, textures take the levels they always did; drawn at half the pixels each way they
    /// take the level one finer, and at all of them the level they did.
    #[test]
    fn the_mip_bias_makes_up_for_the_pixels_not_drawn() {
        let s = |mode, scale| UpscaleSettings {
            mode,
            scale,
            ..UpscaleSettings::default()
        };
        assert_eq!(s(UpscaleMode::Off, 0.5).mip_bias((1000, 500)), 0.0);
        assert!((s(UpscaleMode::Fsr, 0.5).mip_bias((1000, 500)) + 1.0).abs() < 1e-3);
        assert!(s(UpscaleMode::Fsr, 1.0).mip_bias((1000, 500)).abs() < 1e-3);
    }

    /// Off by default, and off the world is drawn at the display's size whatever the scale says.
    #[test]
    fn the_upscaler_is_off_by_default_and_off_keeps_the_display_size() {
        let s = UpscaleSettings::default();
        assert!(!s.runs());
        let halved = UpscaleSettings { scale: 0.5, ..s };
        assert_eq!(halved.render_size((1920, 1080)), (1920, 1080));
    }

    /// On, the render size is the display's by the scale, held between a quarter and the whole,
    /// and never nothing.
    #[test]
    fn the_render_size_is_the_display_size_by_the_scale() {
        let at = |scale| UpscaleSettings {
            mode: UpscaleMode::Fsr,
            scale,
            sharpness: 0.0,
            reactive: true,
        };
        assert_eq!(at(1.0).render_size((1920, 1080)), (1920, 1080));
        assert_eq!(at(0.5).render_size((1920, 1080)), (960, 540));
        assert_eq!(at(0.75).render_size((1921, 1081)), (1441, 811));
        assert_eq!(at(0.01).render_size((1000, 1000)), (250, 250));
        assert_eq!(at(9.0).render_size((1000, 1000)), (1000, 1000));
        assert_eq!(at(0.25).render_size((1, 1)), (1, 1));
    }

    /// Eight places at the whole scale, thirty-two at half, as the SDK counts them.
    #[test]
    fn the_jitter_sequence_lengthens_with_the_upscale() {
        assert_eq!(phase_count(1920, 1920), 8);
        assert_eq!(phase_count(960, 1920), 32);
        assert_eq!(phase_count(1280, 1920), 18);
    }

    /// Every place of the sequence is inside the pixel, none repeats within it, and the sequence
    /// comes round.
    #[test]
    fn the_jitter_sequence_covers_the_pixel_and_comes_round() {
        let phases = 32;
        let moves: Vec<[f32; 2]> = (0..phases).map(|i| jitter_at(i, phases)).collect();
        for m in &moves {
            assert!(m.iter().all(|v| (-0.5..0.5).contains(v)), "{m:?}");
        }
        for (i, a) in moves.iter().enumerate() {
            assert!(moves[i + 1..].iter().all(|b| a != b));
        }
        assert_eq!(jitter_at(phases, phases), moves[0]);
    }

    /// A jitter in pixels moves the projection the same way in x and the other way in y, and by
    /// two units of clip space for a whole target.
    #[test]
    fn the_jitter_in_clip_space_flips_y() {
        assert_eq!(jitter_in_clip_space([0.5, 0.25], (100, 50)), [0.01, -0.01]);
    }

    /// The factors give back the distance a depth was made from, under the left-handed projection
    /// the frames are drawn by.
    #[test]
    fn the_depth_factors_give_back_the_distance() {
        let p = glam::Mat4::perspective_lh(1.0, 1.5, 0.5, 1000.0);
        let f = device_to_view(p);
        for z in [0.5f32, 2.0, 40.0, 900.0] {
            let c = p * glam::Vec4::new(0.0, 0.0, z, 1.0);
            let depth = c.z / c.w;
            let back = f[1] / (depth - f[0]);
            assert!((back - z).abs() < z * 1e-3, "{z}: {back}");
        }
    }
}
