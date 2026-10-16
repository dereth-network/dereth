//! The motion variant: the re-shaded stages, run again for the draws of things that moved since
//! the last frame, so that each writes how far its surface moved on screen.
//!
//! It starts from the re-shaded text, whose alpha test is the same, dither and all (so it covers
//! the same pixels). The vertex stage places the vertices as the re-shaded stage does, a hair
//! nearer the eye so the depth its re-shaded draw left never hides it, and hands on one more
//! output: where the vertex stood in the last frame's clip space. Every pixel stage ends in one output, the motion: in `xy` how far the
//! surface moved since the last frame, this frame's place less the last's, in the world
//! viewport's units (0..1 across it, `y` down); in `w`, 1, saying the pixel's motion is its own.
//! `z` is left nought for the anti-aliasing's own use.
//!
//! The per-frame block a draw of this variant is bound with is the one its re-shaded draw was
//! bound with, with three fields put to other uses, none of which the stages read here:
//! - `view`: the transform from the draw's world position (the frame's drawing space) to the
//!   last frame's clip space;
//! - `fog_color.xy`: the frame's jitter, in normalised device coordinates;
//! - `ambient`: the world viewport, in target pixels: x, y, width, height.

use dereth_render::VertexFormat;

use super::reshade::{PIXEL_STAGES, SPLAT_STAGE};
use super::{Anchor, Scope};

/// The motion target's format.
pub const MOTION_FORMAT: wgpu::TextureFormat = wgpu::TextureFormat::Rgba16Float;

/// The colour target of a motion pipeline: nothing blends, every channel is written.
#[must_use]
pub fn targets() -> Vec<Option<wgpu::ColorTargetState>> {
    vec![Some(wgpu::ColorTargetState {
        format: MOTION_FORMAT,
        blend: None,
        write_mask: wgpu::ColorWrites::ALL,
    })]
}

/// The anchors of the motion variant, applied to the re-shaded text of `format`'s source, with
/// the splat's stage when `splat`.
#[must_use]
pub fn anchors(format: VertexFormat, splat: bool) -> Vec<Anchor> {
    let last = if format.is_pre_transformed() {
        // Drawn on the screen as it stands: no motion of its own.
        "    o.hifi_last = o.pos;\n    return o;\n"
    } else {
        "    o.hifi_last = world * g_frame.view;
    // A hair nearer the eye than the re-shaded draw, whose depth this one is tested against.
    o.pos.z = o.pos.z - 2e-6 * o.pos.w;
    return o;
"
    };
    let mut a = vec![
        Anchor {
            name: "last-position-output",
            scope: Scope::Source,
            find: "    @location(9) sun_share: f32,\n",
            with: "    @location(9) sun_share: f32,
    // Where the vertex stood in the last frame's clip space.
    @location(10) hifi_last: vec4<f32>,
"
            .to_owned(),
        },
        Anchor {
            name: "last-position",
            scope: Scope::Function("vs_main"),
            find: "    return o;\n",
            with: last.to_owned(),
        },
    ];
    for stage in PIXEL_STAGES {
        a.push(Anchor {
            name: "motion-output",
            scope: Scope::Function(stage),
            find: ") -> HifiOut {",
            with: ") -> @location(0) vec4<f32> {".to_owned(),
        });
        a.push(Anchor {
            name: "motion-return",
            scope: Scope::Function(stage),
            find: "return hifi_out(c, i, hifi_class());",
            with: "return hifi_motion(i);".to_owned(),
        });
    }
    if splat {
        a.push(Anchor {
            name: "motion-output",
            scope: Scope::Function(SPLAT_STAGE),
            find: ") -> HifiOut {",
            with: ") -> @location(0) vec4<f32> {".to_owned(),
        });
        a.push(Anchor {
            name: "motion-return",
            scope: Scope::Function(SPLAT_STAGE),
            find: "return hifi_out(lit, i, HIFI_CLASS_TERRAIN);",
            with: "return hifi_motion(i);".to_owned(),
        });
    }
    a
}

/// The text the motion variant appends.
pub const PRELUDE: &str = "
// ---- the motion output -------------------------------------------------------------------

// How far the surface at this pixel moved since the last frame, in the world viewport's units,
// `y` down, with 1 in `w`; all nought where the surface stood behind the last frame's eye.
fn hifi_motion(i: VsOut) -> vec4<f32> {
    if (i.hifi_last.w <= 1e-6) {
        return vec4<f32>(0.0);
    }
    let v = g_frame.ambient;
    let uv = (i.pos.xy - v.xy) / max(v.zw, vec2<f32>(1.0));
    let now = vec2<f32>(uv.x * 2.0 - 1.0, 1.0 - uv.y * 2.0) - g_frame.fog_color.xy;
    let last = i.hifi_last.xy / i.hifi_last.w;
    return vec4<f32>((now - last) * vec2<f32>(0.5, -0.5), 0.0, 1.0);
}
";
