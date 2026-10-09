//! The ordinary shader text is exactly what it was before the high-fidelity presentation
//! existed, so nothing the presentation adds can have changed how the ordinary frame is shaded.
//! The derived shaders start from this text; when it changes on purpose, the pins are updated
//! with it and every anchor is checked again.

use dereth_render::wgpu::sidecar::{legacy_shader_source, splat_shader_source};
use dereth_render::VertexFormat;
use dereth_render_cpu::shader::{fragment_source, vertex_source};
use sha2::{Digest, Sha256};

/// Each pinned text and its SHA-256.
const PINS: &[(&str, &str)] = &[
    (
        "vertex XyzDiffuseTex1 bgra=false",
        "99bc6d836bd40c25076816281a8af6ba1d9309789df814254dea37ff7226ffc3",
    ),
    (
        "vertex XyzDiffuseTex1 bgra=true",
        "6397b5560ee10e797e73c00346e5415e4b5177a5122584328df4748f1b92bac5",
    ),
    (
        "vertex XyzRhwDiffuseTex1 bgra=false",
        "d511ed3a17816b08e523beee9c46684f2b0c9044fc6487251471e265e59c494f",
    ),
    (
        "vertex XyzRhwDiffuseTex1 bgra=true",
        "e1d78cc1f81c1ecd9815b029349f1948dbbc3329fb886c8bd024b588d1b21702",
    ),
    (
        "vertex XyzNormalDiffuseTex1 bgra=false",
        "3c5087bfb0aa00ffbdb5bfcd1e716f8ec8ea65643c1903323e20485c94e64f49",
    ),
    (
        "vertex XyzNormalDiffuseTex1 bgra=true",
        "6dd1c83cab1e7b620f464edd3f5af4180f4ef2733d7c28ee8a68136631e994ec",
    ),
    (
        "vertex XyzDiffuseTex2 bgra=false",
        "6cdf15e052301cd5705d1b3604321d2ed8551e893a43bd0073f8719aa1aba4e0",
    ),
    (
        "vertex XyzDiffuseTex2 bgra=true",
        "b86efcd42c2e9d2235b1182b02f159a7435026114ab38bf8f225897444025cea",
    ),
    (
        "vertex XyzNormalDiffuseTex2 bgra=false",
        "7900b3d94eeaf0bd4791e27c27387ed0a9e5e6634d39f037d3d0f6752e52778a",
    ),
    (
        "vertex XyzNormalDiffuseTex2 bgra=true",
        "55ed8e46dd26a945f994f37284bf733e64c0b052e83e7f86e45f635541771352",
    ),
    (
        "fragment",
        "6397b5560ee10e797e73c00346e5415e4b5177a5122584328df4748f1b92bac5",
    ),
    (
        "device XyzDiffuseTex1",
        "01d85f761efa0e84845e303a37316316861a57efbbb8049426b853375c6d3951",
    ),
    (
        "device XyzRhwDiffuseTex1",
        "f5b0a0f16992216e1e128520e9adb0213a9d744e9a22be6762abe97ed408a52c",
    ),
    (
        "device XyzNormalDiffuseTex1",
        "801352ca5e6b49d0352515b7c97e47c726d943936172ec96954b330f72c1397b",
    ),
    (
        "device XyzDiffuseTex2",
        "e388dc69b82b01562404927a3671c2bfe7336309617d82f86914a9244acaf98b",
    ),
    (
        "device XyzNormalDiffuseTex2",
        "3756e0e8048eab4a3361e24c534723547a0b3d29cee5e7bf7944b42369cea1c7",
    ),
    (
        "splat XyzDiffuseTex1",
        "fb0e47ce31d79dab995b5cdff3bc824aa93d71f8ade3fde7d567ade67ce2418d",
    ),
    (
        "splat XyzRhwDiffuseTex1",
        "58978d76c2bbbe96ce4ed15548359d01e6b5db5797b2584e99f1e0d8de2f804c",
    ),
    (
        "splat XyzNormalDiffuseTex1",
        "b170439b1b3da1932eca54758b9694d9d1b894601169d11477f9d052caf7913b",
    ),
    (
        "splat XyzDiffuseTex2",
        "c86e7d5d7140c023d2e558cd4c95a894c5a4952555fc61cda9b1d3ffe9cbb784",
    ),
    (
        "splat XyzNormalDiffuseTex2",
        "bda4aec77470b77594d5627762a5c0b736a95435d7966a42c325e518679318be",
    ),
];

fn sha256(text: &str) -> String {
    Sha256::digest(text.as_bytes())
        .iter()
        .map(|b| format!("{b:02x}"))
        .collect()
}

/// Every pinned text, by name.
fn texts() -> Vec<(String, String)> {
    let mut out = Vec::new();
    for f in VertexFormat::all() {
        for bgra in [false, true] {
            out.push((format!("vertex {f:?} bgra={bgra}"), vertex_source(f, bgra)));
        }
    }
    out.push(("fragment".to_owned(), fragment_source()));
    for f in VertexFormat::all() {
        out.push((format!("device {f:?}"), legacy_shader_source(f)));
    }
    for f in VertexFormat::all() {
        out.push((format!("splat {f:?}"), splat_shader_source(f)));
    }
    out
}

/// Behaviour: hifi.off.the-legacy-shader-text-is-pinned
#[test]
fn the_ordinary_shader_text_is_unchanged_for_every_permutation() {
    let now: Vec<(String, String)> = texts()
        .into_iter()
        .map(|(name, text)| (name, sha256(&text)))
        .collect();
    let table: String = now
        .iter()
        .map(|(name, sha)| format!("    (\"{name}\", \"{sha}\"),\n"))
        .collect();
    assert_eq!(
        now.len(),
        PINS.len(),
        "the pinned set differs; the texts now are:\n{table}"
    );
    for ((name, sha), (pin_name, pin)) in now.iter().zip(PINS) {
        assert_eq!(
            name, pin_name,
            "the pinned set differs; the texts now are:\n{table}"
        );
        assert_eq!(sha, pin, "{name} changed; the texts now are:\n{table}");
    }
}
