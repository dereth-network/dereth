//! The high-fidelity presentation's claims that need a device: its shaders' own arithmetic,
//! drawn on the hardware GPU and read back.
//!
//! Fixture: a hardware adapter. These tests are ignored by default; run them with `--ignored`,
//! one GPU run at a time.

mod atmosphere;

use dereth_render::wgpu::sidecar::wgpu;

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

/// A device on the machine's hardware adapter; a software rasteriser is refused.
pub fn hardware() -> (wgpu::Device, wgpu::Queue) {
    let instance =
        wgpu::Instance::new(wgpu::InstanceDescriptor::new_without_display_handle_from_env());
    let adapter = block_on(instance.request_adapter(&wgpu::RequestAdapterOptions {
        power_preference: wgpu::PowerPreference::HighPerformance,
        force_fallback_adapter: false,
        compatible_surface: None,
        ..Default::default()
    }))
    .expect("a hardware adapter");
    let info = adapter.get_info();
    assert_ne!(
        info.device_type,
        wgpu::DeviceType::Cpu,
        "{} is a software rasteriser",
        info.name
    );
    eprintln!(
        "adapter: {} ({:?}, {})",
        info.name, info.backend, info.driver
    );
    block_on(adapter.request_device(&wgpu::DeviceDescriptor::default())).expect("a device")
}
