//! The backend layer's claims that need a device: what it draws, samples, dispatches and
//! measures, on the hardware GPU and read back.
//!
//! Fixture: a hardware adapter with Vulkan 1.3, or with Direct3D 12 at shader model 6.6 when
//! `DERETH_TEST_RENDERER=d3d12` asks for it. These tests run with `--features retail-dats`, the
//! switch every device tier sits behind, one GPU run at a time; without it they are ignored. With
//! the Khronos validation layer (or Direct3D 12's debug layer) installed, every test also fails on
//! any error it reports.

// The tests' sizes and indices are small and bounded.
#![allow(clippy::cast_possible_truncation)]

mod adopt;
mod bindless;
mod compute;
mod draw;
mod rt;

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
    eprintln!(
        "adapter: {} on {:?}{}",
        d.caps().name,
        d.backend(),
        if d.validating() { ", validated" } else { "" }
    );
    d
}

/// The validation layer reported nothing.
pub fn no_validation_errors() {
    let errors = dereth_gfx::take_validation_errors();
    assert!(errors.is_empty(), "the validation layer: {errors:#?}");
}

/// The tests' shaders: Slang sources in `tests/shaders/`, compiled ahead of time by
/// `cargo xtask shaders` into `tests/shaders/compiled/`, which is checked in, so the tests need no
/// shader compiler.
pub mod shaders {
    /// The stage an entry point runs in.
    #[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
    #[allow(dead_code)]
    pub enum Stage {
        Vertex,
        Fragment,
        Compute,
        Mesh,
        Task,
    }

    include!("../shaders/compiled/shaders.rs");
}

/// The SPIR-V words of the entry point `e`.
pub fn spirv(e: shaders::Entry) -> Vec<u32> {
    e.spirv_bytes()
        .as_chunks::<4>()
        .0
        .iter()
        .map(|w| u32::from_le_bytes(*w))
        .collect()
}

/// The layer's view of the entry point `e`, whose words are `words`.
pub fn shader(e: shaders::Entry, words: &[u32]) -> dereth_gfx::Shader<'_> {
    dereth_gfx::Shader {
        spirv: words,
        dxil: e.dxil_bytes(),
        entry: e.name(),
    }
}

/// `bytes` as RGBA8 pixels.
pub fn pixels(bytes: &[u8]) -> Vec<[u8; 4]> {
    bytes.as_chunks::<4>().0.to_vec()
}
