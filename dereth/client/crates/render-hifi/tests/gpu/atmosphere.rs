//! The air's haze as the shader works it out is the haze the model is calibrated with.

use dereth_primitives::num::math;
use dereth_render::wgpu::sidecar::wgpu;
use dereth_render_hifi::passes::atmosphere::{self, model};
use dereth_render_hifi::snapshot::{HifiFog, HifiFrame, HifiSky, HifiSkyObject};
use dereth_render_hifi::Level;
use glam::Vec3;

/// The air's shader with one more entry point, which writes the haze's transmittance for each
/// (distance, rise) it is given.
fn probe_source() -> String {
    let [_, _, (_, aerial)] = atmosphere::shader_sources();
    aerial
        + "
@group(1) @binding(0) var<storage, read> probe_in: array<vec2<f32>>;
@group(1) @binding(1) var<storage, read_write> probe_out: array<f32>;

@compute @workgroup_size(64)
fn probe_haze(@builtin(global_invocation_id) id: vec3<u32>) {
    if (id.x >= arrayLength(&probe_in)) {
        return;
    }
    let p = probe_in[id.x];
    probe_out[id.x] = haze_transmittance(p.x, p.y);
}
"
}

fn frame(group: &str, fog: HifiFog, sun_elevation_deg: f32, dome_luminosity: f32) -> HifiFrame {
    let e = sun_elevation_deg.to_radians();
    HifiFrame {
        sky: HifiSky {
            sun_direction: Vec3::new(math::cosf(e), 0.0, math::sinf(e)),
            sun_brightness: 0.8,
            sun_color: [255, 220, 180],
            fog: Some(fog),
            objects: vec![HifiSkyObject {
                index: 0,
                gfx_id: 0x0100_15EE,
                luminosity: dome_luminosity,
                ..HifiSkyObject::default()
            }],
            day_group: group.to_owned(),
            outdoor: true,
            ..HifiSky::default()
        },
        ..HifiFrame::default()
    }
}

/// The shader's transmittance at each of `points` for `frame`'s constants, read back.
fn shader_haze(
    device: &wgpu::Device,
    queue: &wgpu::Queue,
    module: &wgpu::ShaderModule,
    frame: &HifiFrame,
    points: &[[f32; 2]],
) -> Vec<f32> {
    use wgpu::util::DeviceExt;
    let pipeline = device.create_compute_pipeline(&wgpu::ComputePipelineDescriptor {
        label: Some("haze probe"),
        layout: None,
        module,
        entry_point: Some("probe_haze"),
        compilation_options: wgpu::PipelineCompilationOptions::default(),
        cache: None,
    });
    let constants = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
        label: Some("haze probe constants"),
        contents: &model::constants(frame, Level::High),
        usage: wgpu::BufferUsages::UNIFORM,
    });
    let input: Vec<u8> = points
        .iter()
        .flat_map(|p| p.iter().flat_map(|f| f.to_le_bytes()))
        .collect();
    let input = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
        label: Some("haze probe points"),
        contents: &input,
        usage: wgpu::BufferUsages::STORAGE,
    });
    let size = (points.len() * 4) as u64;
    let output = device.create_buffer(&wgpu::BufferDescriptor {
        label: Some("haze probe out"),
        size,
        usage: wgpu::BufferUsages::STORAGE | wgpu::BufferUsages::COPY_SRC,
        mapped_at_creation: false,
    });
    let readback = device.create_buffer(&wgpu::BufferDescriptor {
        label: Some("haze probe readback"),
        size,
        usage: wgpu::BufferUsages::MAP_READ | wgpu::BufferUsages::COPY_DST,
        mapped_at_creation: false,
    });
    let group = |index: u32, entries: &[wgpu::BindGroupEntry<'_>]| {
        device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("haze probe"),
            layout: &pipeline.get_bind_group_layout(index),
            entries,
        })
    };
    let g0 = group(
        0,
        &[wgpu::BindGroupEntry {
            binding: 0,
            resource: constants.as_entire_binding(),
        }],
    );
    let g1 = group(
        1,
        &[
            wgpu::BindGroupEntry {
                binding: 0,
                resource: input.as_entire_binding(),
            },
            wgpu::BindGroupEntry {
                binding: 1,
                resource: output.as_entire_binding(),
            },
        ],
    );
    let mut encoder = device.create_command_encoder(&wgpu::CommandEncoderDescriptor::default());
    {
        let mut pass = encoder.begin_compute_pass(&wgpu::ComputePassDescriptor::default());
        pass.set_pipeline(&pipeline);
        pass.set_bind_group(0, &g0, &[]);
        pass.set_bind_group(1, &g1, &[]);
        let groups = u32::try_from(points.len().div_ceil(64)).expect("a small probe");
        pass.dispatch_workgroups(groups, 1, 1);
    }
    encoder.copy_buffer_to_buffer(&output, 0, &readback, 0, size);
    queue.submit([encoder.finish()]);
    readback
        .slice(..)
        .map_async(wgpu::MapMode::Read, |r| r.expect("the probe maps"));
    device
        .poll(wgpu::PollType::wait_indefinitely())
        .expect("the probe finishes");
    let bytes = readback
        .slice(..)
        .get_mapped_range()
        .expect("the probe is mapped");
    bytes
        .as_chunks::<4>()
        .0
        .iter()
        .map(|c| f32::from_le_bytes(*c))
        .collect()
}

/// Behaviour: hifi.sky.the-shaders-haze-is-the-calibrated-haze
/// For a sunny noon, a rainy noon and a night, the haze the air's shader works out on the
/// device for land at every distance out past the fog's end, below, level with and above the
/// eye, is the haze the model is calibrated with, to within the device's rounding.
#[test]
#[ignore = "draws on the hardware GPU: run with --ignored, one GPU run at a time"]
fn the_shaders_haze_is_the_haze_the_model_is_calibrated_with() {
    let (device, queue) = crate::hardware();
    let module = device.create_shader_module(wgpu::ShaderModuleDescriptor {
        label: Some("haze probe"),
        source: wgpu::ShaderSource::Wgsl(probe_source().into()),
    });
    let day_fog = HifiFog {
        min: 150.0,
        max: 2400.0,
        color: [195, 200, 220],
    };
    let rain_fog = HifiFog {
        max: 1500.0,
        ..day_fog
    };
    let night_fog = HifiFog {
        min: 0.0,
        max: 400.0,
        color: [23, 23, 39],
    };
    let frames = [
        ("sunny noon", frame("Sunny", day_fog, 67.0, 97.0)),
        ("rainy noon", frame("Rainy", rain_fog, 67.0, 97.0)),
        ("night", frame("Sunny", night_fog, 1.0, 11.0)),
    ];
    let mut points = Vec::new();
    for d in (0..4000).step_by(23) {
        for rise in [-400.0, -60.0, 0.0, 0.5, 60.0, 400.0] {
            #[allow(clippy::cast_precision_loss)] // small distances
            points.push([d as f32, rise]);
        }
    }
    for (name, f) in &frames {
        let haze = model::Haze::for_fog(
            f.sky.fog.as_ref(),
            model::Weather::of(&f.sky.day_group),
            model::day_weight(&f.sky),
        );
        let got = shader_haze(&device, &queue, &module, f, &points);
        let mut worst = 0.0f32;
        for (p, g) in points.iter().zip(&got) {
            let want = haze.transmittance(p[0], p[1]);
            let off = (want - g).abs();
            worst = worst.max(off);
            assert!(
                off < 2e-4,
                "{name}: at {} m and {} m up the shader leaves {g}, the model {want}",
                p[0],
                p[1]
            );
        }
        eprintln!(
            "{name}: {} points, worst difference {worst:e}",
            points.len()
        );
    }
}
