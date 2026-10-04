//! The device and the retail dats, as the gpu tier's stations open them.
//!
//! A station that cannot get either fails rather than returning early: a test that printed
//! "skipping" and passed would read as a pass on a machine that tested nothing.

use std::sync::Arc;

use dereth_dat::RetailDatStore;
use dereth_render::device::{DeviceConfig, Gpu};

/// A device with a back buffer of `width` by `height`, or a failed test.
///
/// Backend and adapter selection use the renderer's normal test configuration, including
/// `DERETH_TEST_RENDERER` and `DERETH_TEST_GPU`. The caller owns the binary's device lock.
pub fn test_gpu(width: u32, height: u32) -> Gpu {
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
