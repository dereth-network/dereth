//! Behaviour: none (experimental)
//!
//! The remaster on Direct3D 12: the same frames as on Vulkan, from the same shaders' DXIL. Fixture:
//! every station, at the remaster's identity and with its whole look, on a Direct3D 12 device and on
//! a Vulkan one of the same adapter, each with its debug layer where the machine has it: Direct3D
//! 12's comes with Windows' Graphics Tools, Vulkan's with the Khronos validation layer. Without one,
//! nothing on that device is checked and "quiet" means nothing; the test says which checked.

use auberean_remaster::Remaster;
use dereth_gfx::{Backend, DeviceDesc};

use crate::common::{frame, records, settled, turn, STATIONS};

const W: u32 = 640;
const H: u32 = 480;

/// The mean of the largest channel difference between two pictures, and the share of their pixels
/// apart by more than sixteen levels.
fn apart(a: &[u8], b: &[u8]) -> (f64, f64) {
    let (mut sum, mut far, mut n) = (0u64, 0u64, 0u64);
    for (x, y) in a.chunks(4).zip(b.chunks(4)) {
        let d = (0..3).map(|c| x[c].abs_diff(y[c])).max().unwrap_or(0);
        sum += u64::from(d);
        far += u64::from(d > 16);
        n += 1;
    }
    #[allow(clippy::cast_precision_loss)] // pixel counts and level sums
    let out = (sum as f64 / n.max(1) as f64, far as f64 / n.max(1) as f64);
    out
}

fn remaster(backend: Backend, look: bool) -> Remaster {
    let mut r = Remaster::with(&DeviceDesc {
        debug: true,
        backend,
        ..DeviceDesc::default()
    })
    .expect("the remaster's device");
    if look {
        r.look();
    }
    r
}

/// **On Direct3D 12 the remaster draws as on Vulkan**: at every station, at its identity and with
/// its whole look, the frame the remaster draws on a Direct3D 12 device is under two levels from the
/// one it draws on Vulkan on average, with under one pixel in a hundred apart by more than sixteen
/// levels, and the debug layers, where the machine has them, are quiet.
#[test]
fn on_direct3d_12_the_remaster_draws_as_on_vulkan() {
    let _turn = turn();
    let records = records();
    let mut load = |id: u32| auberean_draw::picture::load(records, id);
    for look in [false, true] {
        let mut dx = remaster(Backend::D3d12, look);
        let mut vk = remaster(Backend::Vulkan, look);
        if !look {
            eprintln!(
                "debug layers checking: Direct3D 12 {}, Vulkan {}",
                dx.device().validating(),
                vk.device().validating()
            );
        }
        for s in &STATIONS {
            let f = frame(s);
            let (a, _) = settled(|| dx.render(&f, W, H, &mut load).expect("the frame").rgba);
            let (b, _) = settled(|| vk.render(&f, W, H, &mut load).expect("the frame").rgba);
            let (mean, far) = apart(&a, &b);
            eprintln!(
                "{} {}: {mean:.2} levels apart on average, {:.2}% by over 16",
                s.name,
                if look { "look" } else { "identity" },
                far * 100.0
            );
            if let Some(dir) = std::env::var_os("AUBEREAN_REMASTER_CAPTURE_DIR") {
                let dir = std::path::PathBuf::from(dir);
                let kind = if look { "look" } else { "identity" };
                crate::common::write_png(
                    &dir.join(format!("{}-{kind}-d3d12.png", s.name)),
                    W,
                    H,
                    &a,
                );
                crate::common::write_png(
                    &dir.join(format!("{}-{kind}-vulkan.png", s.name)),
                    W,
                    H,
                    &b,
                );
            }
            assert!(mean < 2.0, "{}: {mean:.2} levels apart", s.name);
            assert!(
                far < 0.01,
                "{}: {:.2}% apart by over 16",
                s.name,
                far * 100.0
            );
        }
    }
    let errors = dereth_gfx::take_validation_errors();
    assert!(errors.is_empty(), "the debug layers: {errors:#?}");
}
