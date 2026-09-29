//! Offscreen software devices and comparisons against authored glyph pixels.
#![cfg(any(feature = "vulkan", feature = "wgpu", all(windows, feature = "d3d12")))]
use dereth_render::device::{DeviceConfig, Gpu};
use dereth_render::font::TextVertex;

pub fn warp(width: u32, height: u32) -> Gpu {
    Gpu::new(
        None,
        &DeviceConfig {
            width,
            height,
            ..DeviceConfig::default()
        },
    )
    .expect("the pixel test requires an offscreen software device")
}
pub fn require_device() {
    drop(warp(64, 64));
}

pub fn compare(got: &[u8], want: &[u8], cut: u8) -> (u64, u8, u64) {
    assert_eq!(
        got.len(),
        want.len(),
        "readback and reference dimensions agree"
    );
    let mut differ = 0;
    let mut worst = 0;
    let mut soft_where_hard = 0;
    for (&g, &w) in got.iter().zip(want) {
        let d = g.abs_diff(w);
        if d != 0 {
            differ += 1;
            worst = worst.max(d);
        }
        if (w == 0 || w == 255) && g > cut && g < 255 - cut {
            soft_where_hard += 1;
        }
    }
    (differ, worst, soft_where_hard)
}

pub fn pack(v: &[TextVertex]) -> Vec<u8> {
    let mut out = Vec::with_capacity(v.len() * 24);
    for x in v {
        out.extend_from_slice(&x.origin[0].to_le_bytes());
        out.extend_from_slice(&x.origin[1].to_le_bytes());
        out.extend_from_slice(&x.origin[2].to_le_bytes());
        out.extend_from_slice(&x.diffuse.to_le_bytes());
        out.extend_from_slice(&x.u.to_le_bytes());
        out.extend_from_slice(&x.v.to_le_bytes());
    }
    out
}

pub fn identity16() -> [f32; 16] {
    [
        1.0, 0.0, 0.0, 0.0, 0.0, 1.0, 0.0, 0.0, 0.0, 0.0, 1.0, 0.0, 0.0, 0.0, 0.0, 1.0,
    ]
}

pub fn bgra(a8: &[u8]) -> Vec<u8> {
    let mut v = Vec::with_capacity(a8.len() * 4);
    for a in a8 {
        v.extend_from_slice(&[0xFF, 0xFF, 0xFF, *a]);
    }
    v
}
