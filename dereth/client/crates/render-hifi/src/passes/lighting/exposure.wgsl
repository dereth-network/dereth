// The exposure the tone map takes, as the eye adapts. Outdoors it is the exposure the sun and the
// sky set. Indoors it is keyed to the frame's own brightness: the mean of the logarithm of the
// light over a grid across the world viewport, the middle weighted most, with the empty dark
// behind the rooms left out. From one frame to the next it eases toward that, more slowly into
// the dark than into the light, as eyes adjust.

struct Params {
    viewport: vec4<f32>,
    // x 1 to key the exposure to the frame, 0 to take `y`; y the outdoor exposure; z 1 to take
    // the target at once; w the seconds since the last frame.
    mode: vec4<f32>,
    // x the brightness the frame's mean is brought to, y the least exposure, z the greatest,
    // w how much of the way toward the key the exposure goes (1 all of it).
    key: vec4<f32>,
    // x the seconds the eye takes to adjust toward a brighter exposure (into the dark), y toward
    // a dimmer one (into the light).
    rate: vec4<f32>,
};

@group(0) @binding(0) var picture: texture_2d<f32>;
@group(0) @binding(1) var depth: texture_depth_2d;
@group(0) @binding(2) var<uniform> params: Params;
// [0] the exposure in force, [1] the frame's mean brightness, [2] the target.
@group(0) @binding(3) var<storage, read_write> state: array<f32, 4>;

const SIDE: u32 = 16u;
const TAPS: u32 = 8u;

var<workgroup> log_sum: array<f32, 256>;
var<workgroup> weight_sum: array<f32, 256>;

// The exposure that brings a frame whose light has geometric mean `mean` toward the key, by
// `strength` of the way, held between the least and the greatest.
fn keyed(mean: f32, key: f32, strength: f32, least: f32, most: f32) -> f32 {
    let full = key / max(mean, 1e-4);
    return clamp(pow(full, strength), least, most);
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
                s = s + weight * log2(max(lum, 1e-4));
                w = w + weight;
            }
        }
    }
    log_sum[i] = s;
    weight_sum[i] = w;
    workgroupBarrier();
    for (var stride: u32 = 128u; stride > 0u; stride = stride >> 1u) {
        if (i < stride) {
            log_sum[i] = log_sum[i] + log_sum[i + stride];
            weight_sum[i] = weight_sum[i] + weight_sum[i + stride];
        }
        workgroupBarrier();
    }
    if (i != 0u) {
        return;
    }
    var target_exposure = params.mode.y;
    var mean = -1.0;
    if (params.mode.x > 0.5 && weight_sum[0] > 0.0) {
        mean = exp2(log_sum[0] / weight_sum[0]);
        target_exposure = keyed(mean, params.key.x, params.key.w, params.key.y, params.key.z);
    }
    var now = target_exposure;
    let was = state[0];
    if (params.mode.z < 0.5 && was > 0.0) {
        now = adapt(was, target_exposure, params.mode.w, params.rate.x, params.rate.y);
    }
    state[0] = now;
    state[1] = mean;
    state[2] = target_exposure;
}
