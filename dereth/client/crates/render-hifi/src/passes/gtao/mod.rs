//! Ambient and contact occlusion from the world's depth.
//!
//! A horizon search in a few screen-space slices around every pixel finds how much of the
//! hemisphere over the surface is open, the visible arc integrated against the cosine of the
//! surface's normal (taken from depth). An edge-aware blur removes the search's noise, and the
//! term multiplies the picture in linear light: under eaves, in building corners, where walls
//! meet the ground and under objects. Fogged distance and the sky are left alone.

use std::collections::HashMap;

use crate::graph::{EncodeCx, HifiPass, PrepareCx};
use crate::resources::{Resource, ResourceName, TextureSpec};
use crate::{Caps, DebugView, HifiError, HifiFrame, HifiSettings, Level};

const WGSL: &str = include_str!("gtao.wgsl");

/// The search's working format: the visibility and the view distance.
const WORK_FORMAT: wgpu::TextureFormat = wgpu::TextureFormat::Rg32Float;

/// The search radius, metres.
const RADIUS: f32 = 1.5;
/// How much of the occlusion found is applied.
const STRENGTH: f32 = 1.0;
/// The power the visibility is raised to.
const POWER: f32 = 1.5;
/// How much more a sample's approach toward the eye counts in its distance.
const THICKNESS: f32 = 3.0;

/// The constant block's size: two matrices and five vectors.
const PARAMS_BYTES: u64 = 2 * 64 + 5 * 16;

/// The pass's pipelines, made on first use.
#[derive(Debug)]
struct Gpu {
    layout: wgpu::BindGroupLayout,
    params: wgpu::Buffer,
    search: wgpu::RenderPipeline,
    denoise: wgpu::RenderPipeline,
    module: wgpu::ShaderModule,
    pipeline_layout: wgpu::PipelineLayout,
    /// The multiply, the grey view and the coloured shade, by the target's format.
    apply: HashMap<(wgpu::TextureFormat, Apply), wgpu::RenderPipeline>,
}

/// How the term reaches the picture.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
enum Apply {
    /// Multiplied in by the blend, in place.
    Blend,
    /// The same, on the surfaces the bounced light does not light alone.
    BlendInside,
    /// Shown alone, as grey.
    View,
    /// Read with the picture and written into another, with light bounced in the corners.
    Shade,
}

/// Ambient occlusion.
///
/// A frame that steps indoors draws its rooms after the opaque slot, over the resolved
/// picture, as they were recorded: there the occlusion is drawn late, once they are down, by a
/// second instance of the pass ([`Gtao::late`]) over the whole picture and the whole frame's
/// depth. Every other frame takes it in the opaque slot.
#[derive(Debug, Default)]
pub struct Gtao {
    gpu: Option<Gpu>,
    /// This instance draws the occlusion of frames that step indoors, after their rooms.
    late: bool,
}

impl Gtao {
    /// The instance that draws the occlusion of a frame that steps indoors, once its rooms are
    /// drawn.
    #[must_use]
    pub fn late() -> Self {
        Self {
            gpu: None,
            late: true,
        }
    }
}

/// Whether this frame is re-shaded up to an indoor step, its rooms drawn as recorded after it.
fn steps_indoors(cx: &EncodeCx<'_, '_>) -> bool {
    cx.reshade.is_some()
        && crate::reshade::ReshadePlan::new(cx.seam.tables()).is_some_and(|p| p.is_split())
}

/// How hard the search works at each level: slices, steps per side, blur passes.
fn effort(level: Level) -> (u32, u32, u32) {
    match level {
        Level::Off | Level::Low => (2, 6, 1),
        Level::Medium => (2, 8, 2),
        Level::High => (3, 10, 2),
        Level::Ultra => (4, 14, 2),
    }
}

impl Gpu {
    fn new(device: &wgpu::Device) -> Self {
        let module = device.create_shader_module(wgpu::ShaderModuleDescriptor {
            label: Some("gtao"),
            source: wgpu::ShaderSource::Wgsl(WGSL.into()),
        });
        let layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("gtao"),
            entries: &[
                wgpu::BindGroupLayoutEntry {
                    binding: 0,
                    visibility: wgpu::ShaderStages::FRAGMENT,
                    ty: wgpu::BindingType::Texture {
                        sample_type: wgpu::TextureSampleType::Depth,
                        view_dimension: wgpu::TextureViewDimension::D2,
                        multisampled: false,
                    },
                    count: None,
                },
                wgpu::BindGroupLayoutEntry {
                    binding: 1,
                    visibility: wgpu::ShaderStages::FRAGMENT,
                    ty: wgpu::BindingType::Texture {
                        sample_type: wgpu::TextureSampleType::Float { filterable: false },
                        view_dimension: wgpu::TextureViewDimension::D2,
                        multisampled: false,
                    },
                    count: None,
                },
                wgpu::BindGroupLayoutEntry {
                    binding: 3,
                    visibility: wgpu::ShaderStages::FRAGMENT,
                    ty: wgpu::BindingType::Texture {
                        sample_type: wgpu::TextureSampleType::Float { filterable: false },
                        view_dimension: wgpu::TextureViewDimension::D2,
                        multisampled: false,
                    },
                    count: None,
                },
                wgpu::BindGroupLayoutEntry {
                    binding: 2,
                    visibility: wgpu::ShaderStages::FRAGMENT,
                    ty: wgpu::BindingType::Buffer {
                        ty: wgpu::BufferBindingType::Uniform,
                        has_dynamic_offset: false,
                        min_binding_size: wgpu::BufferSize::new(PARAMS_BYTES),
                    },
                    count: None,
                },
            ],
        });
        let pipeline_layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
            label: Some("gtao"),
            bind_group_layouts: &[Some(&layout)],
            immediate_size: 0,
        });
        let params = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("gtao params"),
            size: PARAMS_BYTES,
            usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });
        let search = pipeline(
            device,
            &pipeline_layout,
            &module,
            "fs_ao",
            WORK_FORMAT,
            None,
        );
        let denoise = pipeline(
            device,
            &pipeline_layout,
            &module,
            "fs_denoise",
            WORK_FORMAT,
            None,
        );
        Self {
            layout,
            params,
            search,
            denoise,
            module,
            pipeline_layout,
            apply: HashMap::new(),
        }
    }

    fn apply(
        &mut self,
        device: &wgpu::Device,
        format: wgpu::TextureFormat,
        how: Apply,
    ) -> &wgpu::RenderPipeline {
        let (module, layout) = (&self.module, &self.pipeline_layout);
        self.apply
            .entry((format, how))
            .or_insert_with(|| match how {
                Apply::View => pipeline(device, layout, module, "fs_view", format, None),
                Apply::Shade => pipeline(device, layout, module, "fs_shade", format, None),
                Apply::Blend | Apply::BlendInside => {
                    // The target times the term; its alpha kept.
                    let blend = wgpu::BlendState {
                        color: wgpu::BlendComponent {
                            src_factor: wgpu::BlendFactor::Zero,
                            dst_factor: wgpu::BlendFactor::Src,
                            operation: wgpu::BlendOperation::Add,
                        },
                        alpha: wgpu::BlendComponent {
                            src_factor: wgpu::BlendFactor::Zero,
                            dst_factor: wgpu::BlendFactor::One,
                            operation: wgpu::BlendOperation::Add,
                        },
                    };
                    let entry = if how == Apply::Blend {
                        "fs_apply"
                    } else {
                        "fs_apply_inside"
                    };
                    pipeline(device, layout, module, entry, format, Some(blend))
                }
            })
    }
}

fn pipeline(
    device: &wgpu::Device,
    layout: &wgpu::PipelineLayout,
    module: &wgpu::ShaderModule,
    entry: &str,
    format: wgpu::TextureFormat,
    blend: Option<wgpu::BlendState>,
) -> wgpu::RenderPipeline {
    device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
        label: Some("gtao"),
        layout: Some(layout),
        vertex: wgpu::VertexState {
            module,
            entry_point: Some("vs"),
            compilation_options: wgpu::PipelineCompilationOptions::default(),
            buffers: &[],
        },
        primitive: wgpu::PrimitiveState::default(),
        depth_stencil: None,
        multisample: wgpu::MultisampleState::default(),
        fragment: Some(wgpu::FragmentState {
            module,
            entry_point: Some(entry),
            compilation_options: wgpu::PipelineCompilationOptions::default(),
            targets: &[Some(wgpu::ColorTargetState {
                format,
                blend,
                write_mask: wgpu::ColorWrites::ALL,
            })],
        }),
        multiview_mask: None,
        cache: None,
    })
}

/// The constant block for `frame`.
fn params(s: &HifiSettings, f: &HifiFrame, encoded: bool) -> Vec<u8> {
    let (slices, steps, _) = effort(s.ambient_occlusion);
    let cam = &f.camera;
    let vp = cam.viewport;
    let vh = vp[3].max(1) as f32;
    // Pixels per metre of a thing one metre away.
    let scale = 0.5 * vh * cam.projection.y_axis.y.abs();
    let (fog_min, fog_max, has_fog) = match f.sky.fog {
        Some(fog) if fog.max > fog.min && fog.max > 0.0 => (fog.min, fog.max, 1.0),
        _ => (0.0, 0.0, 0.0),
    };
    let fade_end = if has_fog > 0.5 {
        fog_max.min(400.0)
    } else {
        400.0
    };
    let debug = if s.debug == DebugView::Ao { 1.0 } else { 0.0 };
    let mut out = Vec::with_capacity(usize::try_from(PARAMS_BYTES).unwrap_or(0));
    for m in [cam.projection, cam.projection.inverse()] {
        for v in m.to_cols_array() {
            out.extend_from_slice(&v.to_le_bytes());
        }
    }
    let vecs: [[f32; 4]; 5] = [
        [vp[0] as f32, vp[1] as f32, vp[2] as f32, vp[3] as f32],
        [RADIUS, 0.6, scale, 0.2 * vh],
        [slices as f32, steps as f32, STRENGTH, POWER],
        [fog_min, fog_max, has_fog, fade_end],
        [debug, if encoded { 1.0 } else { 0.0 }, 0.035, THICKNESS],
    ];
    for v in vecs {
        for x in v {
            out.extend_from_slice(&x.to_le_bytes());
        }
    }
    out
}

/// Whether the bounced light darkens this frame's creases itself, so a second occlusion over the
/// surfaces it lights would darken them twice: it is drawn into the re-shaded outdoor frame
/// alone, never indoors, underground or in a frame that steps indoors. In such a frame the
/// occlusion is drawn on the rooms and what stands in them only; everywhere else, on all of it.
fn covered_by_bounced_light(cx: &EncodeCx<'_, '_>) -> bool {
    let s = cx.settings;
    s.lighting.is_on()
        && s.global_illumination.is_on()
        && s.debug != DebugView::Ao
        && cx.reshade.is_some()
        && cx.frame.sky.outdoor
        && !crate::reshade::ReshadePlan::new(cx.seam.tables()).is_some_and(|p| p.is_split())
}

impl HifiPass for Gtao {
    fn name(&self) -> &'static str {
        if self.late {
            "gtao late"
        } else {
            "gtao"
        }
    }

    fn wanted(&self, s: &HifiSettings, f: &HifiFrame, _caps: &Caps) -> bool {
        (s.ambient_occlusion.is_on() || s.debug == DebugView::Ao)
            && f.camera.viewport[2] > 0
            && f.camera.viewport[3] > 0
    }

    fn prepare(&mut self, _cx: &mut PrepareCx<'_, '_>) -> Result<(), HifiError> {
        Ok(())
    }

    fn encode(&mut self, cx: &mut EncodeCx<'_, '_>) -> Result<(), HifiError> {
        // A frame that steps indoors takes its occlusion late, over the picture its rooms are
        // drawn into; every other frame takes it here.
        let late_frame = steps_indoors(cx);
        if late_frame != self.late {
            return Ok(());
        }
        // The late instance works on the picture as drawn, as without re-shading.
        let reshade = if self.late { None } else { cx.reshade };
        let depth_view = if self.late { cx.world_depth } else { cx.depth };
        let covered = !self.late && covered_by_bounced_light(cx);
        let device = cx.seam.device().clone();
        let gpu = self.gpu.get_or_insert_with(|| Gpu::new(&device));
        let mut settings = *cx.settings;
        if !settings.ambient_occlusion.is_on() {
            settings.ambient_occlusion = Level::High;
        }
        let (_, _, blurs) = effort(settings.ambient_occlusion);
        let (width, height) = cx.seam.surface_size();
        let work = |cx: &mut EncodeCx<'_, '_>, name| {
            cx.resources.texture(
                &device,
                name,
                TextureSpec {
                    width,
                    height,
                    format: WORK_FORMAT,
                    usage: wgpu::TextureUsages::RENDER_ATTACHMENT
                        | wgpu::TextureUsages::TEXTURE_BINDING,
                },
            )
        };
        let a = work(cx, ResourceName::Ao)?;
        let b = work(cx, ResourceName::BentNormal)?;

        // In a re-shaded frame the term is multiplied into the linear light in place. Otherwise
        // the picture is read and the shaded picture written into the other one, decoding the
        // stored values unless the format decodes them itself.
        let input = cx.colour.clone();
        let view = settings.debug == DebugView::Ao;
        let (target, format, how) = match reshade {
            Some(rt) => (
                rt.hdr.clone(),
                wgpu::TextureFormat::Rgba16Float,
                if view {
                    Apply::View
                } else if covered {
                    Apply::BlendInside
                } else {
                    Apply::Blend
                },
            ),
            None => {
                let world = match cx.resources.get(ResourceName::WorldColour) {
                    Some(Resource::Texture(_, v)) => Some(v.clone()),
                    _ => None,
                };
                let out = match world {
                    Some(w) if w != input => w,
                    _ => cx.resources.texture(
                        &device,
                        ResourceName::ScratchColour,
                        TextureSpec {
                            width,
                            height,
                            format: cx.seam.surface_format(),
                            usage: wgpu::TextureUsages::RENDER_ATTACHMENT
                                | wgpu::TextureUsages::TEXTURE_BINDING
                                | wgpu::TextureUsages::COPY_SRC,
                        },
                    )?,
                };
                (
                    out,
                    cx.seam.surface_format(),
                    if view { Apply::View } else { Apply::Shade },
                )
            }
        };
        let encoded = !format.is_srgb() && reshade.is_none();
        cx.seam
            .queue()
            .write_buffer(&gpu.params, 0, &params(&settings, cx.frame, encoded));

        let bind_with = |ao: &wgpu::TextureView, picture: &wgpu::TextureView| {
            device.create_bind_group(&wgpu::BindGroupDescriptor {
                label: Some("gtao"),
                layout: &gpu.layout,
                entries: &[
                    wgpu::BindGroupEntry {
                        binding: 0,
                        resource: wgpu::BindingResource::TextureView(depth_view),
                    },
                    wgpu::BindGroupEntry {
                        binding: 1,
                        resource: wgpu::BindingResource::TextureView(ao),
                    },
                    wgpu::BindGroupEntry {
                        binding: 2,
                        resource: gpu.params.as_entire_binding(),
                    },
                    wgpu::BindGroupEntry {
                        binding: 3,
                        resource: wgpu::BindingResource::TextureView(picture),
                    },
                ],
            })
        };
        let bind = |ao: &wgpu::TextureView| bind_with(ao, &input);
        let vp = cx.frame.camera.viewport;
        let x = vp[0].min(width.saturating_sub(1));
        let y = vp[1].min(height.saturating_sub(1));
        let scissor = (x, y, vp[2].min(width - x), vp[3].min(height - y));
        let run = |encoder: &mut wgpu::CommandEncoder,
                   timestamps: Option<wgpu::RenderPassTimestampWrites<'_>>,
                   out: &wgpu::TextureView,
                   clear: bool,
                   clip: bool,
                   instance: u32,
                   pipeline: &wgpu::RenderPipeline,
                   group: &wgpu::BindGroup| {
            let mut pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
                label: Some("gtao"),
                color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                    view: out,
                    depth_slice: None,
                    resolve_target: None,
                    ops: wgpu::Operations {
                        load: if clear {
                            wgpu::LoadOp::Clear(wgpu::Color {
                                r: 1.0,
                                g: -1.0,
                                b: 0.0,
                                a: 0.0,
                            })
                        } else {
                            wgpu::LoadOp::Load
                        },
                        store: wgpu::StoreOp::Store,
                    },
                })],
                depth_stencil_attachment: None,
                timestamp_writes: timestamps,
                occlusion_query_set: None,
                multiview_mask: None,
            });
            if clip {
                pass.set_scissor_rect(scissor.0, scissor.1, scissor.2, scissor.3);
            }
            pass.set_pipeline(pipeline);
            pass.set_bind_group(0, group, &[]);
            pass.draw(0..3, instance..instance + 1);
        };

        let from_b = bind(&b);
        let from_a = bind(&a);
        run(
            cx.encoder,
            cx.timer.writes("gtao search"),
            &a,
            true,
            true,
            0,
            &gpu.search,
            &from_b,
        );
        cx.seam.count_pass();
        let mut last_in_a = true;
        for i in 0..blurs {
            let (out, group) = if last_in_a {
                (&b, &from_a)
            } else {
                (&a, &from_b)
            };
            run(
                cx.encoder,
                if i == 0 {
                    cx.timer.writes("gtao blur")
                } else {
                    None
                },
                out,
                true,
                true,
                i,
                &gpu.denoise,
                group,
            );
            cx.seam.count_pass();
            last_in_a = !last_in_a;
        }
        let last = if last_in_a { &a } else { &b };
        // Where the bounced light is drawn the apply reads the normals' classes in place of the
        // picture.
        let inside_group = match (how, reshade) {
            (Apply::BlendInside, Some(rt)) => Some(bind_with(last, &rt.normal)),
            _ => None,
        };
        let apply = gpu.apply(&device, format, how).clone();
        let group = match &inside_group {
            Some(g) => g,
            None if last_in_a => &from_a,
            None => &from_b,
        };
        let in_place = reshade.is_some();
        run(
            cx.encoder,
            cx.timer.writes("gtao apply"),
            &target,
            false,
            in_place,
            0,
            &apply,
            group,
        );
        cx.seam.count_pass();
        if !in_place {
            *cx.colour = target;
        }
        Ok(())
    }
}
