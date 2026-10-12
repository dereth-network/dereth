// The exposure the tone map takes, as the eye adapts. Outdoors it is the exposure the sun and the
// sky set. Indoors it is keyed to the frame's own brightness: the mean of the logarithm of the
// light over a grid across the world viewport, the middle weighted most, with the empty dark
// behind the rooms left out. A room draws much as the ordinary frame draws it, a dark one a little
// brighter and a bright one a little darker, and never so bright that the brightest of what it
// shows is drawn white. From one frame to the next it eases toward that, more slowly into the dark
// than into the light, as eyes adjust.
//
// Outdoors the sun and the sky's exposure is held to the ordinary picture. Once the light, the
// occlusion, the bounced light and the air are drawn, and before the weather, the frame as the
// tone map would draw it is compared with the frame as the ordinary picture draws it, over the same
// grid, at a ladder of exposures about the sun and the sky's (`measure`); where it would be drawn
// more than a little brighter than the ordinary picture (or, by day, darker) the exposure is moved
// just so far that it is not (`main`). What the light adds in shade, contrast and color is kept:
// only the whole is brought down or up; and the weather, drawn after, darkens the wet world as it
// would.

struct Params {
    viewport: vec4<f32>,
    // x 1 to key the exposure to the frame, 0 to take `y`; y the outdoor exposure; z 1 to take
    // the target at once; w the seconds since the last frame.
    mode: vec4<f32>,
    // x the brightness the frame's mean is brought to, y the least exposure, z the greatest,
    // w how much of the way toward the key the exposure goes (1 all of it).
    key: vec4<f32>,
    // x the seconds the eye takes to adjust toward a brighter exposure (into the dark), y toward
    // a dimmer one (into the light); z the exposure that draws a room as the ordinary frame
    // does, w the light its brightest share may be brought to at most.
    rate: vec4<f32>,
    // x 1 to hold the outdoor exposure to the ordinary picture; y the least and z the most the
    // frame's mean brightness may be of the ordinary picture's; w unused.
    anchor: vec4<f32>,
    // The tone map's look: x the slope of its toe, y its contrast, z its vignette; w unused.
    look: vec4<f32>,
    // The authored fog the ordinary picture draws and the light per pixel leaves to the sky's
    // air: x where it begins, y where it is whole, metres; z 1 when the picture before the light
    // lacks it; w unused.
    fog: vec4<f32>,
    // The fog's color, as the display encodes it.
    fog_color: vec4<f32>,
    // xyz the eye, render space; w unused.
    eye: vec4<f32>,
    clip_to_render: mat4x4<f32>,
};

@group(0) @binding(0) var picture: texture_2d<f32>;
@group(0) @binding(1) var depth: texture_depth_2d;
@group(0) @binding(2) var<uniform> params: Params;
// [0] the exposure in force, [1] the frame's mean brightness, [2] the target, [3] the light of
// the brightest share of the frame.
@group(0) @binding(3) var<storage, read_write> state: array<f32, 4>;
// The frame as the ordinary picture draws it, linear: what the light per pixel started from.
@group(0) @binding(4) var ordinary: texture_2d<f32>;
// What `measure` found of the outdoor frame: [0] the ordinary picture's summed brightness, [1..9]
// the frame's at each rung of the ladder.
@group(0) @binding(5) var<storage, read_write> measured: array<f32, 12>;

const SIDE: u32 = 16u;
const TAPS: u32 = 8u;

var<workgroup> log_sum: array<f32, 256>;
var<workgroup> square_sum: array<f32, 256>;
var<workgroup> weight_sum: array<f32, 256>;

// The rungs of the ladder of exposures the outdoor frame is drawn at, as shares of the sun and
// the sky's: from two fifths up to twice, three rungs to each doubling.
const RUNGS: u32 = 8u;
// The rung of the sun and the sky's own exposure.
const OWN_RUNG: u32 = 4u;
var<workgroup> ordinary_sum: array<f32, 256>;
var<workgroup> rung_sum: array<array<f32, 8>, 256>;

// The share of the sun and the sky's exposure rung `k` of the ladder stands at.
fn rung(k: u32) -> f32 {
    return exp2((f32(k) - f32(OWN_RUNG)) / 3.0);
}

// The ordinary picture's brightness at `px`, from the picture before the light per pixel, `c`,
// with the authored fog put back where the picture lacks it.
fn ordinary_brightness(c: vec3<f32>, px: vec2<i32>, g: vec2<f32>) -> f32 {
    var shown = linear_to_srgb(clamp(c, vec3<f32>(0.0), vec3<f32>(1.0)));
    if (params.fog.z > 0.5) {
        let d = textureLoad(depth, px, 0);
        if (d < 1.0) {
            let ndc = vec4<f32>(g.x * 2.0 - 1.0, 1.0 - g.y * 2.0, d, 1.0);
            let w = params.clip_to_render * ndc;
            let dist = length(w.xyz / w.w - params.eye.xyz);
            let seen = clamp((params.fog.y - dist) / max(params.fog.y - params.fog.x, 1e-3), 0.0, 1.0);
            shown = mix(params.fog_color.rgb, shown, seen);
        }
    }
    return luminance(shown);
}

// The brightness, 0..1, a pixel of linear light `x` is drawn at by the tone map: its curve, the
// straight toe under it, the display encoding and the contrast about middle gray.
fn drawn_brightness(x: vec3<f32>, toe: f32, contrast: f32) -> f32 {
    let m = max(aces(x), x * toe);
    let g = luminance(linear_to_srgb(m));
    return clamp((g - 0.5) * contrast + 0.5, 0.0, 1.0);
}

// The share of the sun and the sky's exposure that brings the frame's mean brightness `ratio` of
// the ordinary picture's back within `least` and `most`, from two neighboring rungs `lo` and `hi`
// (shares) at which it stands at `r_lo` and `r_hi` and between which the bound it passes lies:
// found along the logarithm of the share.
fn between_rungs(lo: f32, r_lo: f32, hi: f32, r_hi: f32, bound: f32) -> f32 {
    let t = clamp((bound - r_lo) / max(r_hi - r_lo, 1e-6), 0.0, 1.0);
    return exp2(mix(log2(lo), log2(hi), t));
}

// The exposure for a frame whose light has geometric mean `mean`: `base`, the exposure that
// draws a room as the ordinary frame does, moved toward the key by `strength` of the way, held
// between the least and the greatest; and lowered further, though not below the least, so that
// `bright`, the light of the frame's brightest share, is brought to `white` at most.
fn keyed(mean: f32, bright: f32, key: f32, strength: f32, base: f32, white: f32, least: f32, most: f32) -> f32 {
    let toward = clamp(base * pow(key / max(mean, 1e-4), strength), least, most);
    return max(min(toward, white / max(bright, 1e-4)), least);
}

// One step of the eye's adjustment from `before` toward `goal`, `dt` seconds on: an exponential
// ease, in the logarithm of the exposure, `up` seconds to brighten and `down` to dim. A goal
// reached stays exactly where it is.
fn adapt(before: f32, goal: f32, dt: f32, up: f32, down: f32) -> f32 {
    let tau = select(down, up, goal > before);
    let k = 1.0 - exp(-dt / max(tau, 1e-3));
    return before * pow(goal / before, k);
}

@compute @workgroup_size(16, 16)
fn main(@builtin(local_invocation_index) i: u32, @builtin(local_invocation_id) l: vec3<u32>) {
    var s = 0.0;
    var q = 0.0;
    var w = 0.0;
    if (params.mode.x > 0.5) {
        let v = params.viewport;
        for (var a: u32 = 0u; a < TAPS; a = a + 1u) {
            for (var b: u32 = 0u; b < TAPS; b = b + 1u) {
                let g = vec2<f32>(f32(l.x * TAPS + a) + 0.5, f32(l.y * TAPS + b) + 0.5)
                    / f32(SIDE * TAPS);
                let px = vec2<i32>(v.xy + g * v.zw);
                let c = textureLoad(picture, px, 0).rgb;
                let lum = luminance(max(c, vec3<f32>(0.0)));
                let d = textureLoad(depth, px, 0);
                if (d >= 1.0 && lum < 1e-3) {
                    continue;
                }
                let r = g - vec2<f32>(0.5);
                let weight = 1.0 - 1.2 * dot(r, r);
                let lg = log2(max(lum, 1e-4));
                s = s + weight * lg;
                q = q + weight * lg * lg;
                w = w + weight;
            }
        }
    }
    log_sum[i] = s;
    square_sum[i] = q;
    weight_sum[i] = w;
    workgroupBarrier();
    for (var stride: u32 = 128u; stride > 0u; stride = stride >> 1u) {
        if (i < stride) {
            log_sum[i] = log_sum[i] + log_sum[i + stride];
            square_sum[i] = square_sum[i] + square_sum[i + stride];
            weight_sum[i] = weight_sum[i] + weight_sum[i + stride];
        }
        workgroupBarrier();
    }
    if (i != 0u) {
        return;
    }
    var target_exposure = params.mode.y;
    var mean = -1.0;
    var bright = -1.0;
    if (params.mode.x > 0.5 && weight_sum[0] > 0.0) {
        let m = log_sum[0] / weight_sum[0];
        let spread = sqrt(max(square_sum[0] / weight_sum[0] - m * m, 0.0));
        mean = exp2(m);
        // About the brightest fiftieth of the frame, taking its light as spread evenly in the
        // logarithm about the mean.
        bright = exp2(m + 2.0 * spread);
        target_exposure = keyed(
            mean,
            bright,
            params.key.x,
            params.key.w,
            params.rate.z,
            params.rate.w,
            params.key.y,
            params.key.z,
        );
    }
    if (params.anchor.x > 0.5 && measured[0] > 0.0) {
        // The frame's mean brightness at each rung, as a share of the ordinary picture's.
        var ratio = array<f32, 8>(0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0);
        for (var k: u32 = 0u; k < RUNGS; k = k + 1u) {
            ratio[k] = measured[k + 1u] / measured[0];
        }
        var share = 1.0;
        if (ratio[OWN_RUNG] > params.anchor.z) {
            share = rung(0u);
            for (var k: u32 = OWN_RUNG; k > 0u; k = k - 1u) {
                if (ratio[k - 1u] <= params.anchor.z) {
                    share = between_rungs(rung(k - 1u), ratio[k - 1u], rung(k), ratio[k], params.anchor.z);
                    break;
                }
            }
        } else if (ratio[OWN_RUNG] < params.anchor.y) {
            share = rung(RUNGS - 1u);
            for (var k: u32 = OWN_RUNG + 1u; k < RUNGS; k = k + 1u) {
                if (ratio[k] >= params.anchor.y) {
                    share = between_rungs(rung(k - 1u), ratio[k - 1u], rung(k), ratio[k], params.anchor.y);
                    break;
                }
            }
        }
        target_exposure = params.mode.y * share;
    }
    var now = target_exposure;
    let was = state[0];
    if (params.mode.z < 0.5 && was > 0.0) {
        now = adapt(was, target_exposure, params.mode.w, params.rate.x, params.rate.y);
    }
    state[0] = now;
    state[1] = mean;
    state[2] = target_exposure;
    state[3] = bright;
}

// The outdoor frame as the tone map would draw it at each rung of the ladder, and as the ordinary
// picture draws it, summed over the grid into `measured`.
@compute @workgroup_size(16, 16)
fn measure(@builtin(local_invocation_index) i: u32, @builtin(local_invocation_id) l: vec3<u32>) {
    // Outdoors: the frame drawn at each rung, and as the ordinary picture draws it.
    var ord = 0.0;
    var rungs = array<f32, 8>(0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0);
    {
        let v = params.viewport;
        for (var a: u32 = 0u; a < TAPS; a = a + 1u) {
            for (var b: u32 = 0u; b < TAPS; b = b + 1u) {
                let g = vec2<f32>(f32(l.x * TAPS + a) + 0.5, f32(l.y * TAPS + b) + 0.5)
                    / f32(SIDE * TAPS);
                let px = vec2<i32>(v.xy + g * v.zw);
                let c = max(textureLoad(picture, px, 0).rgb, vec3<f32>(0.0));
                ord = ord + ordinary_brightness(textureLoad(ordinary, px, 0).rgb, px, g);
                let r = g - vec2<f32>(0.5);
                let lit = c * params.mode.y * (1.0 - params.look.z * dot(r, r));
                for (var k: u32 = 0u; k < RUNGS; k = k + 1u) {
                    rungs[k] = rungs[k] + drawn_brightness(lit * rung(k), params.look.x, params.look.y);
                }
            }
        }
    }
    ordinary_sum[i] = ord;
    rung_sum[i] = rungs;
    ordinary_sum[i] = ord;
    rung_sum[i] = rungs;
    workgroupBarrier();
    for (var stride: u32 = 128u; stride > 0u; stride = stride >> 1u) {
        if (i < stride) {
            ordinary_sum[i] = ordinary_sum[i] + ordinary_sum[i + stride];
            for (var k: u32 = 0u; k < RUNGS; k = k + 1u) {
                rung_sum[i][k] = rung_sum[i][k] + rung_sum[i + stride][k];
            }
        }
        workgroupBarrier();
    }
    if (i != 0u) {
        return;
    }
    measured[0] = ordinary_sum[0];
    for (var k: u32 = 0u; k < RUNGS; k = k + 1u) {
        measured[k + 1u] = rung_sum[0][k];
    }
}
