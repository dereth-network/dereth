// Bloom (a chain of halving downsamples and tent upsamples), exposure, a filmic tone map and a
// light grade, from the linear world into the frame target's encoding.

struct Post {
    // x exposure, y bloom strength, z 1 for AgX (else ACES), w the bloom levels.
    tone: vec4<f32>,
    // The source texel size (for the bloom passes); z the slope of the tone map's toe, w unused.
    texel: vec4<f32>,
    // x saturation, y contrast, z vignette, w 1 on the first downsample.
    grade: vec4<f32>,
    viewport: vec4<f32>,
};

@group(0) @binding(0) var src: texture_2d<f32>;
@group(0) @binding(1) var bloom: texture_2d<f32>;
@group(0) @binding(2) var lin: sampler;
@group(0) @binding(3) var<uniform> post: Post;

struct VsOut {
    @builtin(position) pos: vec4<f32>,
    @location(0) uv: vec2<f32>,
};

@vertex
fn vs(@builtin(vertex_index) i: u32) -> VsOut {
    let x = f32((i << 1u) & 2u);
    let y = f32(i & 2u);
    var o: VsOut;
    o.pos = vec4<f32>(x * 2.0 - 1.0, 1.0 - y * 2.0, 0.0, 1.0);
    o.uv = vec2<f32>(x, y);
    return o;
}

fn s(uv: vec2<f32>) -> vec3<f32> {
    return textureSampleLevel(src, lin, uv, 0.0).rgb;
}

fn karis(c: vec3<f32>) -> f32 {
    return 1.0 / (1.0 + luminance(c));
}

// The thirteen-tap downsample, with a luminance-weighted average on the first level so single
// bright texels do not flicker into large blobs.
@fragment
fn fs_down(i: VsOut) -> @location(0) vec4<f32> {
    let t = post.texel.xy;
    let uv = i.uv;
    let a = s(uv + t * vec2<f32>(-2.0, -2.0));
    let b = s(uv + t * vec2<f32>(0.0, -2.0));
    let c = s(uv + t * vec2<f32>(2.0, -2.0));
    let d = s(uv + t * vec2<f32>(-2.0, 0.0));
    let e = s(uv);
    let f = s(uv + t * vec2<f32>(2.0, 0.0));
    let g = s(uv + t * vec2<f32>(-2.0, 2.0));
    let h = s(uv + t * vec2<f32>(0.0, 2.0));
    let k = s(uv + t * vec2<f32>(2.0, 2.0));
    let l = s(uv + t * vec2<f32>(-1.0, -1.0));
    let m = s(uv + t * vec2<f32>(1.0, -1.0));
    let n = s(uv + t * vec2<f32>(-1.0, 1.0));
    let o = s(uv + t * vec2<f32>(1.0, 1.0));
    if (post.grade.w > 0.5) {
        let g0 = (a + b + d + e) * 0.25;
        let g1 = (b + c + e + f) * 0.25;
        let g2 = (d + e + g + h) * 0.25;
        let g3 = (e + f + h + k) * 0.25;
        let g4 = (l + m + n + o) * 0.25;
        let w0 = karis(g0) * 0.125;
        let w1 = karis(g1) * 0.125;
        let w2 = karis(g2) * 0.125;
        let w3 = karis(g3) * 0.125;
        let w4 = karis(g4) * 0.5;
        let sum = g0 * w0 + g1 * w1 + g2 * w2 + g3 * w3 + g4 * w4;
        return vec4<f32>(max(sum / max(w0 + w1 + w2 + w3 + w4, 1e-4), vec3<f32>(0.0)), 1.0);
    }
    var r = e * 0.125;
    r = r + (a + c + g + k) * 0.03125;
    r = r + (b + d + f + h) * 0.0625;
    r = r + (l + m + n + o) * 0.125;
    return vec4<f32>(max(r, vec3<f32>(0.0)), 1.0);
}

// The tent upsample, added onto the level above.
@fragment
fn fs_up(i: VsOut) -> @location(0) vec4<f32> {
    let t = post.texel.xy;
    let uv = i.uv;
    var r = s(uv) * 4.0;
    r = r + (s(uv + vec2<f32>(-t.x, 0.0)) + s(uv + vec2<f32>(t.x, 0.0)) + s(uv + vec2<f32>(0.0, -t.y)) + s(uv + vec2<f32>(0.0, t.y))) * 2.0;
    r = r + s(uv + vec2<f32>(-t.x, -t.y)) + s(uv + vec2<f32>(t.x, -t.y)) + s(uv + vec2<f32>(-t.x, t.y)) + s(uv + vec2<f32>(t.x, t.y));
    return vec4<f32>(r / 16.0, 1.0);
}

fn aces(x: vec3<f32>) -> vec3<f32> {
    // The ACES filmic curve fitted with its input and output transforms (Hill).
    let m1 = mat3x3<f32>(
        vec3<f32>(0.59719, 0.07600, 0.02840),
        vec3<f32>(0.35458, 0.90834, 0.13383),
        vec3<f32>(0.04823, 0.01566, 0.83777),
    );
    let m2 = mat3x3<f32>(
        vec3<f32>(1.60475, -0.10208, -0.00327),
        vec3<f32>(-0.53108, 1.10813, -0.07276),
        vec3<f32>(-0.07367, -0.00605, 1.07602),
    );
    let v = m1 * x;
    let a = v * (v + 0.0245786) - 0.000090537;
    let b = v * (0.983729 * v + 0.4329510) + 0.238081;
    return clamp(m2 * (a / b), vec3<f32>(0.0), vec3<f32>(1.0));
}

fn agx_curve(x: vec3<f32>) -> vec3<f32> {
    let x2 = x * x;
    let x4 = x2 * x2;
    return 15.5 * x4 * x2 - 40.14 * x4 * x + 31.96 * x4 - 6.868 * x2 * x + 0.4298 * x2 + 0.1191 * x - 0.00232;
}

fn agx(c: vec3<f32>) -> vec3<f32> {
    let inset = mat3x3<f32>(
        vec3<f32>(0.842479062253094, 0.0423282422610123, 0.0423756549057051),
        vec3<f32>(0.0784335999999992, 0.878468636469772, 0.0784336),
        vec3<f32>(0.0792237451477643, 0.0791661274605434, 0.879142973793104),
    );
    let outset = mat3x3<f32>(
        vec3<f32>(1.19687900512017, -0.0528968517574562, -0.0529716355144438),
        vec3<f32>(-0.0980208811401368, 1.15190312990417, -0.0980434501171241),
        vec3<f32>(-0.0990297440797205, -0.0989611768448433, 1.15107367264116),
    );
    let lo = -12.47393;
    let hi = 4.026069;
    var v = inset * max(c, vec3<f32>(1e-10));
    v = clamp((log2(v) - lo) / (hi - lo), vec3<f32>(0.0), vec3<f32>(1.0));
    v = agx_curve(v);
    // The punchy look.
    let l = luminance(v);
    v = pow(max(v, vec3<f32>(0.0)), vec3<f32>(1.35));
    v = l + 1.4 * (v - l);
    v = outset * v;
    // The curve's output is display-encoded; back to linear for the shared encode.
    return clamp(pow(max(v, vec3<f32>(0.0)), vec3<f32>(2.2)), vec3<f32>(0.0), vec3<f32>(1.0));
}

@fragment
fn fs_tonemap(i: VsOut) -> @location(0) vec4<f32> {
    let px = vec2<i32>(i.pos.xy);
    let c = textureLoad(src, px, 0).rgb;
    let uv = vec2<f32>(i.pos.xy) * post.texel.xy;
    let b = textureSampleLevel(bloom, lin, uv, 0.0).rgb / post.tone.w;
    var x = mix(c, b, post.tone.y) * post.tone.x;
    // Vignette, inside the world viewport.
    let v = (i.pos.xy - post.viewport.xy) / post.viewport.zw - vec2<f32>(0.5);
    x = x * (1.0 - post.grade.z * dot(v, v));
    var m: vec3<f32>;
    if (post.tone.z > 0.5) {
        m = agx(x);
    } else {
        // The fitted curve's toe falls to black far below a tenth of middle grey, which takes
        // night and deep shade with it; below where it meets a straight toe, the toe holds.
        m = max(aces(x), x * post.texel.z);
    }
    // Grade: a little saturation and contrast about middle grey, in display space.
    var g = linear_to_srgb(m);
    let l = luminance(g);
    g = mix(vec3<f32>(l), g, post.grade.x);
    g = (g - 0.5) * post.grade.y + 0.5;
    // A dither of a quarter level, so the gradients of the sky do not band.
    let n = fract(52.9829189 * fract(dot(i.pos.xy, vec2<f32>(0.06711056, 5.83715e-3)))) - 0.5;
    g = g + vec3<f32>(n / 255.0);
    return vec4<f32>(clamp(g, vec3<f32>(0.0), vec3<f32>(1.0)), 1.0);
}
