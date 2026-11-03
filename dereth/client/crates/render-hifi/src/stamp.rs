//! A building's door stamp, drawn by the presentation. The ordinary frame draws the landscape cell
//! by cell, nearest last, so whatever stands before a door is drawn over its stamp again. The
//! presentation draws its ground whole before any stamp, so a stamp here looks first: a door
//! behind what was drawn before the first stamp cuts nothing, and the ground needs no second
//! drawing over the stamps.

use dereth_render::wgpu::sidecar::{build_derived_pipeline, SidecarContext};
use dereth_render_cpu::pso::{portal_stamp_mask, PipelineKey};

use crate::derive::pipeline::{HDR_FORMAT, NORMAL_FORMAT};

const WGSL: &str = include_str!("stamp.wgsl");

/// How far past what was drawn a door may stand and still cut, meters, and how much more per meter
/// from the eye, where the ground is coarser.
const REACH: (f32, f32) = (1.5, 0.01);
/// How far up the door the view may be clear for a pixel to cut, meters: the ground rises against
/// a sill, and as in the ordinary frame a door still cuts it there, though a low view crosses it
/// well before the door.
const RISE: f32 = 1.5;

/// The stamp's pipeline, and the group naming the depth it looks at.
#[derive(Debug)]
pub struct StampTest {
    pipeline: wgpu::RenderPipeline,
    layout: wgpu::BindGroupLayout,
    params: wgpu::Buffer,
    group: Option<wgpu::BindGroup>,
}

impl StampTest {
    /// The pipeline, drawing into the world's targets with no colour.
    #[must_use]
    pub fn new(cx: &SidecarContext<'_>) -> Self {
        let device = cx.device();
        let layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("door stamp"),
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
                    visibility: wgpu::ShaderStages::VERTEX_FRAGMENT,
                    ty: wgpu::BindingType::Buffer {
                        ty: wgpu::BufferBindingType::Uniform,
                        has_dynamic_offset: false,
                        min_binding_size: None,
                    },
                    count: None,
                },
            ],
        });
        let pipeline_layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
            label: Some("door stamp"),
            bind_group_layouts: &[
                Some(cx.uniform_bind_layout()),
                Some(cx.texture_bind_layout()),
                Some(cx.texture_bind_layout()),
                Some(cx.sampler_bind_layout()),
                Some(&layout),
            ],
            immediate_size: 0,
        });
        let module = device.create_shader_module(wgpu::ShaderModuleDescriptor {
            label: Some("door stamp"),
            source: wgpu::ShaderSource::Wgsl(WGSL.into()),
        });
        let none = |format| {
            Some(wgpu::ColorTargetState {
                format,
                blend: None,
                write_mask: wgpu::ColorWrites::empty(),
            })
        };
        let pipeline = build_derived_pipeline(
            device,
            &pipeline_layout,
            &module,
            "fs_stamp",
            &PipelineKey::portal_stamp(portal_stamp_mask::BUILDING),
            &[none(HDR_FORMAT), none(NORMAL_FORMAT)],
        );
        let params = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("door stamp"),
            size: 32,
            usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });
        Self {
            pipeline,
            layout,
            params,
            group: None,
        }
    }

    /// This frame's: the stamps look at `kept`, the depth kept before the first of them.
    pub fn prepare(
        &mut self,
        cx: &SidecarContext<'_>,
        kept: &wgpu::TextureView,
        (near, far): (f32, f32),
        pixels_at_one_meter: f32,
    ) {
        // The recorded stamps stand in the frame's own pixels, whatever the render scale.
        let (width, height) = cx.output_size();
        #[allow(clippy::cast_precision_loss)] // pixels
        let values = [
            width as f32,
            height as f32,
            near,
            far,
            REACH.0,
            REACH.1,
            RISE,
            pixels_at_one_meter,
        ];
        let bytes: Vec<u8> = values.iter().flat_map(|v| v.to_le_bytes()).collect();
        cx.queue().write_buffer(&self.params, 0, &bytes);
        self.group = Some(cx.device().create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("door stamp"),
            layout: &self.layout,
            entries: &[
                wgpu::BindGroupEntry {
                    binding: 0,
                    resource: wgpu::BindingResource::TextureView(kept),
                },
                wgpu::BindGroupEntry {
                    binding: 1,
                    resource: self.params.as_entire_binding(),
                },
            ],
        }));
    }

    /// The pipeline and group to draw this frame's stamps with.
    #[must_use]
    pub fn draw(&self) -> Option<(&wgpu::RenderPipeline, &wgpu::BindGroup)> {
        Some((&self.pipeline, self.group.as_ref()?))
    }
}

#[cfg(test)]
mod tests {
    //! Behaviour: none (experimental hi-fi door stamps)

    #[test]
    fn the_stamp_shader_parses_and_validates() {
        let module = naga::front::wgsl::parse_str(super::WGSL)
            .unwrap_or_else(|e| panic!("{}", e.emit_to_string(super::WGSL)));
        naga::valid::Validator::new(
            naga::valid::ValidationFlags::all(),
            naga::valid::Capabilities::all(),
        )
        .validate(&module)
        .unwrap_or_else(|e| panic!("{e:?}"));
    }
}
