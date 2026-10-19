//! On Direct3D 12 the `wgpu` device builds its shaders with DXC, from the library beside the
//! client, and with FXC, part of Windows, when the library is not there. Which one the device took
//! is read off its adapter: FXC holds Direct3D 12 to an older shader model, which has no subgroup
//! operations, and DXC offers them wherever the adapter has them. Either way the device comes up,
//! builds a compute pipeline and runs it. The client's own device names the compiler on its
//! adapter's line, and once it compiles with DXC, which is not safe to call from two threads at
//! once, the process builds one pipeline at a time.
//!
//! Fixture: the hardware GPU on Direct3D 12, and DXC's library in the folder the test-only
//! `DERETH_TEST_DXC_DIR` names (`bin/x64` of DXC's Windows release).

#![cfg(all(gpu, windows, feature = "hifi"))]

use std::path::{Path, PathBuf};

use dereth_render::device::{Backend, DeviceConfig, Gpu};
use dereth_render::wgpu::compiler::{self, ShaderCompiler};
use dereth_render::wgpu::sidecar::wgpu;

/// Doubles every value of its buffer.
const DOUBLED: &str = "
@group(0) @binding(0) var<storage, read_write> values: array<u32>;
@compute @workgroup_size(64)
fn main(@builtin(global_invocation_id) id: vec3<u32>) {
    if id.x < arrayLength(&values) {
        values[id.x] = values[id.x] * 2u;
    }
}
";

/// How many values the pipeline doubles.
const VALUES: u32 = 256;

/// Drive a future that the native device resolves without waiting on anything else.
fn block_on<F: std::future::Future>(future: F) -> F::Output {
    let mut future = std::pin::pin!(future);
    let mut cx = std::task::Context::from_waker(std::task::Waker::noop());
    loop {
        if let std::task::Poll::Ready(out) = future.as_mut().poll(&mut cx) {
            return out;
        }
        std::thread::yield_now();
    }
}

/// The folder holding DXC's library, which this test needs.
fn dxc_folder() -> PathBuf {
    let dir = std::env::var_os("DERETH_TEST_DXC_DIR")
        .filter(|d| !d.is_empty())
        .map(PathBuf::from)
        .expect(
            "this test needs DXC: set DERETH_TEST_DXC_DIR to the folder holding dxcompiler.dll \
             (bin/x64 of DXC's Windows release)",
        );
    assert!(
        dir.join(compiler::DXC_LIBRARY).is_file(),
        "DERETH_TEST_DXC_DIR names {}, which holds no {}",
        dir.display(),
        compiler::DXC_LIBRARY
    );
    dir
}

/// An empty folder of this test's own.
fn empty_folder() -> PathBuf {
    let dir = std::env::temp_dir().join(format!("dereth-gpu-no-dxc-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).expect("a temporary folder");
    dir
}

/// The hardware adapter on Direct3D 12, made with DXC looked for in `folder`, and the compiler it
/// was given.
fn direct3d_12(folder: &Path) -> (ShaderCompiler, wgpu::Adapter) {
    let mut desc = wgpu::InstanceDescriptor::new_without_display_handle();
    desc.backends = wgpu::Backends::DX12;
    let (desc, compiler) = compiler::with_dxc_in(desc, Some(folder));
    let instance = wgpu::Instance::new(desc);
    let adapter = block_on(instance.request_adapter(&wgpu::RequestAdapterOptions {
        power_preference: wgpu::PowerPreference::HighPerformance,
        force_fallback_adapter: false,
        compatible_surface: None,
        ..Default::default()
    }))
    .expect("a Direct3D 12 adapter");
    let info = adapter.get_info();
    assert_ne!(
        info.device_type,
        wgpu::DeviceType::Cpu,
        "{} is a software rasteriser",
        info.name
    );
    (compiler, adapter)
}

/// A device on `adapter` builds the doubling pipeline and runs it: the values it reads back.
fn doubled_on(adapter: &wgpu::Adapter) -> Vec<u32> {
    let (device, queue) =
        block_on(adapter.request_device(&wgpu::DeviceDescriptor::default())).expect("a device");
    let validation = device.push_error_scope(wgpu::ErrorFilter::Validation);
    let internal = device.push_error_scope(wgpu::ErrorFilter::Internal);
    let module = device.create_shader_module(wgpu::ShaderModuleDescriptor {
        label: Some("doubled"),
        source: wgpu::ShaderSource::Wgsl(DOUBLED.into()),
    });
    let pipeline = device.create_compute_pipeline(&wgpu::ComputePipelineDescriptor {
        label: Some("doubled"),
        layout: None,
        module: &module,
        entry_point: Some("main"),
        compilation_options: wgpu::PipelineCompilationOptions::default(),
        cache: None,
    });
    let bytes = u64::from(VALUES) * 4;
    let values = device.create_buffer(&wgpu::BufferDescriptor {
        label: Some("values"),
        size: bytes,
        usage: wgpu::BufferUsages::STORAGE
            | wgpu::BufferUsages::COPY_SRC
            | wgpu::BufferUsages::COPY_DST,
        mapped_at_creation: false,
    });
    let start: Vec<u8> = (0..VALUES).flat_map(u32::to_le_bytes).collect();
    queue.write_buffer(&values, 0, &start);
    let back = device.create_buffer(&wgpu::BufferDescriptor {
        label: Some("read back"),
        size: bytes,
        usage: wgpu::BufferUsages::MAP_READ | wgpu::BufferUsages::COPY_DST,
        mapped_at_creation: false,
    });
    let bind = device.create_bind_group(&wgpu::BindGroupDescriptor {
        label: Some("values"),
        layout: &pipeline.get_bind_group_layout(0),
        entries: &[wgpu::BindGroupEntry {
            binding: 0,
            resource: values.as_entire_binding(),
        }],
    });
    let mut encoder = device.create_command_encoder(&wgpu::CommandEncoderDescriptor::default());
    {
        let mut pass = encoder.begin_compute_pass(&wgpu::ComputePassDescriptor::default());
        pass.set_pipeline(&pipeline);
        pass.set_bind_group(0, &bind, &[]);
        pass.dispatch_workgroups(VALUES.div_ceil(64), 1, 1);
    }
    encoder.copy_buffer_to_buffer(&values, 0, &back, 0, bytes);
    queue.submit([encoder.finish()]);
    assert!(
        block_on(internal.pop()).is_none(),
        "the device failed the doubling pipeline"
    );
    let refused = block_on(validation.pop());
    assert!(
        refused.is_none(),
        "the doubling pipeline was refused: {refused:?}"
    );
    back.slice(..)
        .map_async(wgpu::MapMode::Read, |r| r.expect("read back"));
    device
        .poll(wgpu::PollType::wait_indefinitely())
        .expect("the device finishes");
    let read: Vec<u32> = back
        .slice(..)
        .get_mapped_range()
        .expect("the values read back")
        .as_chunks::<4>()
        .0
        .iter()
        .map(|c| u32::from_le_bytes(*c))
        .collect();
    read
}

fn twice_each() -> Vec<u32> {
    (0..VALUES).map(|i| i * 2).collect()
}

/// Behaviour: hifi.device.shaders-build-with-the-dxc-beside-the-client
#[test]
fn on_direct3d_12_shaders_build_with_the_dxc_beside_the_client_and_with_fxc_without_it() {
    let _gpu = crate::common::gpu_lock();
    let dxc = dxc_folder();

    let (compiler, adapter) = direct3d_12(&dxc);
    assert_eq!(compiler, ShaderCompiler::Dxc, "with {}", dxc.display());
    assert!(
        adapter.features().contains(wgpu::Features::SUBGROUP),
        "the adapter offers what DXC's shader model has: it took FXC"
    );
    assert_eq!(doubled_on(&adapter), twice_each(), "built with DXC");
    drop(adapter);

    let none = empty_folder();
    let (compiler, adapter) = direct3d_12(&none);
    assert_eq!(compiler, ShaderCompiler::Fxc, "with no library");
    assert!(
        !adapter.features().contains(wgpu::Features::SUBGROUP),
        "the adapter offers what only DXC's shader model has: it did not take FXC"
    );
    assert_eq!(doubled_on(&adapter), twice_each(), "built with FXC");
    drop(adapter);
    let _ = std::fs::remove_dir_all(&none);

    // The client's own device, with the library this test names beside it.
    let device = Gpu::new_on(Backend::Wgpu, None, &DeviceConfig::default())
        .unwrap_or_else(|e| panic!("this test needs a wgpu device and none opened: {e}"));
    let on_d3d12 = device.adapter_name().contains(" on Dx12");
    assert_eq!(
        device.shader_compiler(),
        on_d3d12.then_some(ShaderCompiler::Dxc),
        "{}",
        device.adapter_name()
    );
    assert_eq!(
        device.adapter_name().ends_with(" with DXC"),
        on_d3d12,
        "{}",
        device.adapter_name()
    );
    // DXC's compiler is not safe to call from two threads at once: once a device compiles with
    // it, the process builds one pipeline at a time.
    if on_d3d12 {
        assert!(
            compiler::BUILDS.is_on(),
            "a device compiles with DXC and pipelines are still built at once"
        );
    }
}
