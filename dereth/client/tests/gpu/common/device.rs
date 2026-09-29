//! The device and the retail dats, as the gpu tier's stations open them.
//!
//! A station that cannot get either fails rather than returning early: a test that printed
//! "skipping" and passed would read as a pass on a machine that tested nothing.

use std::sync::Arc;

use dereth_dat::RetailDatStore;
use dereth_render::device::{DeviceConfig, Gpu};

/// A device that prefers a software rasteriser, with a back buffer of `width` by `height`, or a
/// failed test.
///
/// The preference is only honoured where the backend has one: D3D12 always has WARP, but the
/// default Vulkan backend picks a CPU-type physical device only when one is installed (lavapipe,
/// SwiftShader) and otherwise takes the machine's GPU. `DERETH_TEST_GPU=hardware|software`
/// overrides the preference for every device of a run; `DERETH_TEST_RENDERER=d3d12` with the
/// preference left as it is runs the tier on WARP.
pub fn software_gpu(width: u32, height: u32) -> Gpu {
    let cfg = DeviceConfig {
        width,
        height,
        ..DeviceConfig::default()
    };
    Gpu::new(None, &cfg).unwrap_or_else(|e| {
        panic!("the gpu tier needs a device ({width}x{height}) and none opened: {e}")
    })
}

/// The retail dats, shared, or a failed test that says where they were looked for.
pub fn dats() -> Arc<RetailDatStore> {
    Arc::new(dereth_dat::testing::open_store_or_fail())
}

/// The retail dats, owned, or a failed test that says where they were looked for.
pub fn dat_store() -> RetailDatStore {
    dereth_dat::testing::open_store_or_fail()
}
