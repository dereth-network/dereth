//! The WGSL the Vulkan and `wgpu` backends share beyond the legacy shader: the landscape
//! composite's compute shader and the landscape splat's pixel shader. Both backends compile the
//! same text, so the two produce the same composite and the same splat.
//!
//! It also holds the rewrite both use where a sampler cannot carry a level-of-detail bias: every
//! texture sample of the legacy pixel stage and of the splat takes the bias the bound sampler
//! would have carried from the frame block's otherwise unused `screen.y`.

/// The composite, one invocation per destination texel.
///
/// Pixels are BGRA8 bytes read as little-endian `u32`s: B in bits 0..8, G 8..16, R 16..24, A 24..32.
///
/// `job` layout, in `u32`s: `[size, stride, base_present, base_offset, base_w, base_h, base_tiling,
/// overlay_count]`, then per overlay `[alpha_offset, alpha_w, alpha_h, rotation, tex_present,
/// tex_offset, tex_w, tex_h, tiling]`. Offsets and the output row stride are in pool words.
pub(crate) const TERRAIN_MERGE: &str = r"
@group(0) @binding(0) var<storage, read> pool: array<u32>;
@group(0) @binding(1) var<storage, read> job: array<u32>;
@group(0) @binding(2) var<storage, read_write> out_px: array<u32>;

// The debug colour a missing texture fills with: bytes 00 FF 00 00.
const MISSING: u32 = 0x0000FF00u;

fn fetch_tiled(off: u32, w: u32, h: u32, tiling: u32, size: u32, x: u32, y: u32) -> u32 {
    let t = max(tiling, 1u);
    let sx = (x * t * w / size) % w;
    let sy = (y * t * h / size) % h;
    return pool[off + sy * w + sx];
}

// The alpha map's walk for each rotation, as `rotated_offset`.
fn rotated(rot: u32, w: u32, h: u32, col: u32, row: u32) -> u32 {
    let W = i32(w);
    let H = i32(h);
    let c = i32(col);
    let r = i32(row);
    var idx: i32;
    switch rot {
        case 0u: { idx = c + r * W; }
        case 1u: { idx = (W - 1) + c * W - r; }
        case 2u: { idx = (W * H - 1) - c - r * W; }
        default: { idx = (H - 1) * W - c * W + r; }
    }
    return u32(idx);
}

// `integer_blend`: a == 255 keeps the destination, a == 0 takes the overlay; alpha untouched.
fn blend(d: u32, s: u32, a: u32) -> u32 {
    if (a == 255u) {
        return d;
    }
    var ap = a;
    if (a > 128u) {
        ap = a + 1u;
    }
    let ia = 256u - ap;
    var o = d & 0xFF000000u;
    for (var c = 0u; c < 3u; c = c + 1u) {
        let sh = c * 8u;
        let sc = (s >> sh) & 0xFFu;
        let dc = (d >> sh) & 0xFFu;
        o = o | ((((sc * ia + dc * ap) >> 8u) & 0xFFu) << sh);
    }
    return o;
}

@compute @workgroup_size(8, 8, 1)
fn cs_merge(@builtin(global_invocation_id) gid: vec3<u32>) {
    let size = job[0];
    if (gid.x >= size || gid.y >= size) {
        return;
    }
    let x = gid.x;
    let y = gid.y;
    var px = MISSING;
    if (job[2] != 0u) {
        px = fetch_tiled(job[3], job[4], job[5], job[6], size, x, y);
    }
    let n = job[7];
    for (var k = 0u; k < n; k = k + 1u) {
        let b = 8u + k * 9u;
        let aw = job[b + 1u];
        let ah = job[b + 2u];
        let ax = x * aw / size;
        let ay = y * ah / size;
        let a = pool[job[b] + rotated(job[b + 3u], aw, ah, ax, ay)] >> 24u;
        var s = MISSING;
        if (job[b + 4u] != 0u) {
            s = fetch_tiled(job[b + 5u], job[b + 6u], job[b + 7u], job[b + 8u], size, x, y);
        }
        px = blend(px, s, a);
    }
    out_px[y * job[1] + x] = px;
}
";

/// The splat pixel shader, appended to the legacy fragment source. The layer bindings live in
/// set 4, so sets 0 to 3 keep the legacy meaning.
pub(crate) const TERRAIN_SPLAT_TAIL: &str = r"
struct Splat {
    // x = a base texture is bound, y = its tiling, z = the overlay count.
    base: vec4<u32>,
    // Per overlay: x = the alpha map's quarter turns, y = the tile's tiling, z = a tile is bound.
    layers: array<vec4<u32>, 5>,
};
var<immediate> g_splat: Splat;

@group(4) @binding(0) var s_base: texture_2d<f32>;
@group(4) @binding(1) var s_alpha0: texture_2d<f32>;
@group(4) @binding(2) var s_alpha1: texture_2d<f32>;
@group(4) @binding(3) var s_alpha2: texture_2d<f32>;
@group(4) @binding(4) var s_alpha3: texture_2d<f32>;
@group(4) @binding(5) var s_alpha4: texture_2d<f32>;
@group(4) @binding(6) var s_tex0: texture_2d<f32>;
@group(4) @binding(7) var s_tex1: texture_2d<f32>;
@group(4) @binding(8) var s_tex2: texture_2d<f32>;
@group(4) @binding(9) var s_tex3: texture_2d<f32>;
@group(4) @binding(10) var s_tex4: texture_2d<f32>;
@group(4) @binding(11) var s_wrap: sampler;
@group(4) @binding(12) var s_clamp: sampler;

// The debug colour a missing texture fills with: bytes 00 FF 00 00, green.
const SPLAT_MISSING: vec4<f32> = vec4<f32>(0.0, 1.0, 0.0, 0.0);

// Where the composite's alpha-map walk lands for a cell coordinate, per rotation.
fn splat_rotate(uv: vec2<f32>, rot: u32) -> vec2<f32> {
    switch rot {
        case 1u: { return vec2<f32>(1.0 - uv.y, uv.x); }
        case 2u: { return vec2<f32>(1.0 - uv.x, 1.0 - uv.y); }
        case 3u: { return vec2<f32>(uv.y, 1.0 - uv.x); }
        default: { return uv; }
    }
}

// One overlay: the alpha map's alpha keeps the colour beneath (1) or takes the tile (0).
fn splat_layer(below: vec4<f32>, alpha: texture_2d<f32>, tile: texture_2d<f32>, k: u32, uv: vec2<f32>) -> vec4<f32> {
    let l = g_splat.layers[k];
    let a = textureSample(alpha, s_clamp, splat_rotate(uv, l.x)).a;
    var s = SPLAT_MISSING;
    if (l.z != 0u) {
        s = textureSample(tile, s_wrap, uv * f32(max(l.y, 1u)));
    }
    return vec4<f32>(mix(s.rgb, below.rgb, a), below.a);
}

@fragment
fn ps_splat(i: VsOut) -> @location(0) vec4<f32> {
    // The cell's texture coordinates run 0..1 across it with the key's rotation already applied,
    // exactly as they address the composite.
    let uv = clamp(i.uv0, vec2<f32>(0.0), vec2<f32>(1.0));
    // Only the layers the cell has are sampled. Every branch below tests a push constant, which is
    // the same for the whole draw, so the samples stay in uniform control flow and their
    // derivatives are well defined.
    var c = SPLAT_MISSING;
    if (g_splat.base.x != 0u) {
        c = textureSample(s_base, s_wrap, uv * f32(max(g_splat.base.y, 1u)));
    }
    let n = g_splat.base.z;
    if (n > 0u) { c = splat_layer(c, s_alpha0, s_tex0, 0u, uv); }
    if (n > 1u) { c = splat_layer(c, s_alpha1, s_tex1, 1u, uv); }
    if (n > 2u) { c = splat_layer(c, s_alpha2, s_tex2, 2u, uv); }
    if (n > 3u) { c = splat_layer(c, s_alpha3, s_tex3, 3u, uv); }
    if (n > 4u) { c = splat_layer(c, s_alpha4, s_tex4, 4u, uv); }
    // The composite is the stage-0 texture of a MODULATE draw; so is this.
    let lit = c * diffuse_arg(i);
    alpha_test(lit.a);
    return finish(lit, i.fog);
}
";

/// The bias a sample takes when the sampler carries none: the frame block's `screen.y`, which the
/// device fills with the bound sampler's bias for each draw.
const SAMPLE_BIAS: &str = "g_frame.screen.y";

/// Each texture sample of the legacy pixel stage, and what it becomes when it takes its bias from
/// the frame block.
fn legacy_samples() -> [(&'static str, String); 2] {
    [
        (
            "textureSample(g_tex0, g_samp0, i.uv0)",
            format!("textureSampleBias(g_tex0, g_samp0, i.uv0, {SAMPLE_BIAS})"),
        ),
        (
            "textureSample(g_tex1, g_samp1, i.uv1)",
            format!("textureSampleBias(g_tex1, g_samp1, i.uv1, {SAMPLE_BIAS})"),
        ),
    ]
}

/// Each texture sample of [`TERRAIN_SPLAT_TAIL`], and what it becomes when it takes its bias from
/// the frame block.
fn splat_samples() -> [(&'static str, String); 3] {
    [
        (
            "textureSample(alpha, s_clamp, splat_rotate(uv, l.x))",
            format!("textureSampleBias(alpha, s_clamp, splat_rotate(uv, l.x), {SAMPLE_BIAS})"),
        ),
        (
            "textureSample(tile, s_wrap, uv * f32(max(l.y, 1u)))",
            format!("textureSampleBias(tile, s_wrap, uv * f32(max(l.y, 1u)), {SAMPLE_BIAS})"),
        ),
        (
            "textureSample(s_base, s_wrap, uv * f32(max(g_splat.base.y, 1u)))",
            format!(
                "textureSampleBias(s_base, s_wrap, uv * f32(max(g_splat.base.y, 1u)), {SAMPLE_BIAS})"
            ),
        ),
    ]
}

fn rewrite(source: &str, samples: &[(&'static str, String)]) -> String {
    samples
        .iter()
        .fold(source.to_owned(), |s, (from, to)| s.replace(from, to))
}

/// `source`, legacy shader text, with every texture sample taking the bound sampler's bias from
/// the frame block, for a device whose samplers carry none.
pub(crate) fn bias_legacy_samples(source: &str) -> String {
    rewrite(source, &legacy_samples())
}

/// `tail`, the splat tail, with every texture sample taking the bound sampler's bias from the
/// frame block, as [`bias_legacy_samples`] does for the legacy stage.
pub(crate) fn bias_splat_samples(tail: &str) -> String {
    rewrite(tail, &splat_samples())
}

#[cfg(test)]
mod tests {
    //! Behaviour: none (a text rewrite of the shared shader source; the devices' captures test what it draws).
    use super::*;

    /// Every sample the rewrite names is in the source, so none is silently left unbiased, and
    /// the rewritten source has no unbiased sample left.
    #[test]
    fn the_bias_rewrite_reaches_every_texture_sample() {
        let legacy = dereth_render_cpu::shader::fragment_source();
        for (from, _) in legacy_samples() {
            assert!(legacy.contains(from), "the legacy stage samples {from}");
        }
        assert!(!bias_legacy_samples(&legacy).contains("textureSample("));
        for (from, _) in splat_samples() {
            assert!(
                TERRAIN_SPLAT_TAIL.contains(from),
                "the splat samples {from}"
            );
        }
        assert!(!bias_splat_samples(TERRAIN_SPLAT_TAIL).contains("textureSample("));
    }
}
