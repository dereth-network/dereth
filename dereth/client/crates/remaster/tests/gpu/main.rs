//! The remaster's claims that need a device, drawn with no window on the hardware GPU and read
//! back.
//!
//! Fixture: a hardware adapter with Vulkan 1.3, or with Direct3D 12 at shader model 6.6 when
//! `DERETH_TEST_RENDERER=d3d12` asks for it. These tests run with `--features retail-dats,gpu`, one
//! GPU run at a time; `retail-dats` alone does not build them.

// The tests' sizes and indices are small and bounded.
#![allow(clippy::cast_possible_truncation, clippy::cast_precision_loss)]

mod instances;
mod sway;
mod trees;

use dereth_gfx::{Backend, Device, DeviceDesc};

/// The backend the tests' devices are made on: Direct3D 12 where `DERETH_TEST_RENDERER` says
/// `d3d12`, Vulkan otherwise.
pub fn backend() -> Backend {
    match std::env::var("DERETH_TEST_RENDERER").as_deref() {
        Ok(r) if r.trim().eq_ignore_ascii_case("d3d12") => Backend::D3d12,
        _ => Backend::Vulkan,
    }
}

/// A device of the layer's own on the hardware adapter, on [`backend`], with the driver's
/// validation where it is installed; a software rasterizer is refused.
pub fn hardware() -> Device {
    let d = Device::create(&DeviceDesc {
        debug: true,
        backend: backend(),
        ..DeviceDesc::default()
    })
    .expect("a device");
    assert!(
        d.caps().hardware,
        "{} is a software rasterizer",
        d.caps().name
    );
    eprintln!("adapter: {} on {:?}", d.caps().name, d.backend());
    d
}

/// The validation layer reported nothing.
pub fn no_validation_errors() {
    let errors = dereth_gfx::take_validation_errors();
    assert!(errors.is_empty(), "the validation layer: {errors:#?}");
}
