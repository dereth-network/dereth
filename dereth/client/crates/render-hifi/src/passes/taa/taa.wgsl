// Temporal anti-aliasing: each frame of the world is drawn a fraction of a pixel off the last,
// and blended into the frames before it, followed to where each surface stood in the last frame.
//
// The blend works on light squeezed into 0..1 (each colour over one plus its brightness), so a
// few bright pixels cannot dominate it, and keeps the history inside the spread of the current
// frame's colours around each pixel, so what the current frame no longer shows (a surface that
// moved away, a light that went out) cannot linger as a trail. A sharpening over the result gives
// back the crispness the blend takes.
//
// The history is kept squeezed; the picture handed on is linear light again.

struct Taa {
    // The world viewport: x, y, width, height, in target pixels.
    viewport: vec4<f32>,
    // xy the frame's jitter, in normalised device coordinates; z 1 when the history may be used;
    // w the share of the current frame in the blend while nothing moves.
    jitter: vec4<f32>,
    // x the sharpening; y the share of the current frame at and past `z` pixels of motion; w
    // how strongly a translucent change in a pixel takes the history out.
    tune: vec4<f32>,
    // The target's size in pixels, and its reciprocal.
    size: vec4<f32>,
    // x 1 to show, in place of the picture, the share of the current frame (red), the frames
    // held (green) and the translucent change (blue).
    show: vec4<f32>,
    // This frame's unjittered normalised device coordinates and depth to the last frame's clip
    // space, the same point of the world.
    reproject: mat4x4<f32>,
};

@group(0) @binding(0) var current: texture_2d<f32>;
@group(0) @binding(1) var depth: texture_depth_2d;
@group(0) @binding(2) var motion: texture_2d<f32>;
@group(0) @binding(3) var history: texture_2d<f32>;
@group(0) @binding(4) var lin: sampler;
@group(0) @binding(5) var<uniform> taa: Taa;
@group(0) @binding(6) var before_alpha: texture_2d<f32>;

@vertex
fn vs(@builtin(vertex_index) i: u32) -> @builtin(position) vec4<f32> {
    let x = f32((i << 1u) & 2u);
    let y = f32(i & 2u);
    return vec4<f32>(x * 2.0 - 1.0, 1.0 - y * 2.0, 0.0, 1.0);
}

fn squeeze(c: vec3<f32>) -> vec3<f32> {
    let k = max(c, vec3<f32>(0.0));
    return k / (1.0 + luminance(k));
}

fn unsqueeze(c: vec3<f32>) -> vec3<f32> {
    return c / max(1.0 - luminance(c), 1e-3);
}

fn to_ycocg(c: vec3<f32>) -> vec3<f32> {
    return vec3<f32>(
        0.25 * c.r + 0.5 * c.g + 0.25 * c.b,
        0.5 * c.r - 0.5 * c.b,
        -0.25 * c.r + 0.5 * c.g - 0.25 * c.b,
    );
}

fn from_ycocg(c: vec3<f32>) -> vec3<f32> {
    return vec3<f32>(c.x + c.y - c.z, c.x + c.z, c.x - c.y - c.z);
}

// A pixel inside the world viewport.
fn inside(p: vec2<i32>) -> vec2<i32> {
    let lo = vec2<i32>(taa.viewport.xy);
    let hi = lo + vec2<i32>(taa.viewport.zw) - vec2<i32>(1);
    return clamp(p, lo, hi);
}

// The current frame at pixel `p`, squeezed, as luma and chroma.
fn current_at(p: vec2<i32>) -> vec3<f32> {
    return to_ycocg(squeeze(textureLoad(current, inside(p), 0).rgb));
}

// How far the surface at pixel `p` moved since the last frame, in the viewport's units, this
// frame's place less the last's: its own motion where it moved, otherwise the camera's.
fn motion_at(p: vec2<i32>) -> vec2<f32> {
    let m = textureLoad(motion, p, 0);
    if (m.w > 0.5) {
        return m.xy;
    }
    let d = textureLoad(depth, p, 0);
    let uv = (vec2<f32>(p) + vec2<f32>(0.5) - taa.viewport.xy) / taa.viewport.zw;
    let now = vec2<f32>(uv.x * 2.0 - 1.0, 1.0 - uv.y * 2.0) - taa.jitter.xy;
    let last = taa.reproject * vec4<f32>(now, d, 1.0);
    if (last.w <= 1e-6) {
        return vec2<f32>(0.0);
    }
    return (now - last.xy / last.w) * vec2<f32>(0.5, -0.5);
}

// The history at `uv` (the viewport's units), through a Catmull-Rom filter of five bilinear
// samples, so following the motion does not blur it; `w` how many frames it holds.
fn history_at(uv: vec2<f32>) -> vec4<f32> {
    let at = (taa.viewport.xy + uv * taa.viewport.zw) - vec2<f32>(0.5);
    let base = floor(at);
    let f = at - base;
    let w0 = f * (-0.5 + f * (1.0 - 0.5 * f));
    let w1 = 1.0 + f * f * (-2.5 + 1.5 * f);
    let w2 = f * (0.5 + f * (2.0 - 1.5 * f));
    let w3 = f * f * (-0.5 + 0.5 * f);
    let w12 = w1 + w2;
    let o12 = w2 / w12;
    let lo = taa.viewport.xy + vec2<f32>(0.5);
    let hi = taa.viewport.xy + taa.viewport.zw - vec2<f32>(0.5);
    let t0 = clamp(base - vec2<f32>(0.5), lo, hi) * taa.size.zw;
    let t3 = clamp(base + vec2<f32>(2.5), lo, hi) * taa.size.zw;
    let t12 = clamp(base + vec2<f32>(0.5) + o12, lo, hi) * taa.size.zw;
    var c = textureSampleLevel(history, lin, vec2<f32>(t12.x, t0.y), 0.0) * (w12.x * w0.y);
    c = c + textureSampleLevel(history, lin, vec2<f32>(t0.x, t12.y), 0.0) * (w0.x * w12.y);
    c = c + textureSampleLevel(history, lin, vec2<f32>(t12.x, t12.y), 0.0) * (w12.x * w12.y);
    c = c + textureSampleLevel(history, lin, vec2<f32>(t3.x, t12.y), 0.0) * (w3.x * w12.y);
    c = c + textureSampleLevel(history, lin, vec2<f32>(t12.x, t3.y), 0.0) * (w12.x * w3.y);
    let weight = w12.x * w0.y + w0.x * w12.y + w12.x * w12.y + w3.x * w12.y + w12.x * w3.y;
    return max(c / max(weight, 1e-4), vec4<f32>(0.0));
}

// `h` brought inside the box `lo`..`hi` along the line toward the box's middle.
fn clip_into(lo: vec3<f32>, hi: vec3<f32>, h: vec3<f32>) -> vec3<f32> {
    let middle = 0.5 * (hi + lo);
    let half = 0.5 * (hi - lo) + vec3<f32>(1e-5);
    let v = h - middle;
    let units = abs(v / half);
    let most = max(units.x, max(units.y, units.z));
    if (most > 1.0) {
        return middle + v / most;
    }
    return h;
}

// The blend: the history followed to this pixel, held to the current frame's colours around it,
// and the current frame mixed in. Written squeezed, with how many frames the pixel holds in `w`:
// a pixel whose history was just started (the first frames, a surface just come into view)
// takes each new frame as an equal share of what it holds, so it is smooth within a few frames,
// and then the steady share.
@fragment
fn fs_resolve(@builtin(position) p: vec4<f32>) -> @location(0) vec4<f32> {
    let px = vec2<i32>(p.xy);
    var m1 = vec3<f32>(0.0);
    var m2 = vec3<f32>(0.0);
    var lo = vec3<f32>(1e9);
    var hi = vec3<f32>(-1e9);
    var centre = vec3<f32>(0.0);
    var nearest = 2.0;
    var nearest_at = px;
    for (var y = -1; y <= 1; y = y + 1) {
        for (var x = -1; x <= 1; x = x + 1) {
            let q = inside(px + vec2<i32>(x, y));
            let c = current_at(q);
            m1 = m1 + c;
            m2 = m2 + c * c;
            lo = min(lo, c);
            hi = max(hi, c);
            if (x == 0 && y == 0) {
                centre = c;
            }
            // The motion of the nearest surface around, so an edge follows what is in front.
            let d = textureLoad(depth, q, 0);
            if (d < nearest) {
                nearest = d;
                nearest_at = q;
            }
        }
    }
    if (taa.jitter.z < 0.5) {
        return vec4<f32>(from_ycocg(centre), 1.0);
    }
    // At most as many frames as the steady share takes.
    let most = 1.0 / max(taa.jitter.w, 1e-3);
    let mean = m1 / 9.0;
    let spread = sqrt(max(m2 / 9.0 - mean * mean, vec3<f32>(0.0)));
    let box_lo = max(lo, mean - spread * 1.25);
    let box_hi = min(hi, mean + spread * 1.25);
    let moved = motion_at(nearest_at);
    let uv = (p.xy - taa.viewport.xy) / taa.viewport.zw;
    let last = uv - moved;
    if (any(last < vec2<f32>(0.0)) || any(last > vec2<f32>(1.0))) {
        return vec4<f32>(from_ycocg(centre), 1.0);
    }
    let held = history_at(last);
    let h = clip_into(box_lo, box_hi, to_ycocg(held.rgb));
    let frames = min(held.w + 1.0, most);
    // More of the current frame the faster the surface moves across the screen, and where a
    // translucent draw changed the pixel (smoke, sparks, glass): what it shows moves with no
    // depth of its own to follow.
    let pixels = length(moved * taa.viewport.zw);
    var share = mix(taa.jitter.w, taa.tune.y, clamp(pixels / max(taa.tune.z, 1e-3), 0.0, 1.0));
    let translucent = textureLoad(motion, px, 0).z;
    share = max(share, clamp(translucent * taa.tune.w, 0.0, 1.0));
    share = max(share, 1.0 / frames);
    if (taa.show.x > 0.5) {
        return vec4<f32>(share * 0.5, frames / most * 0.5, translucent * 0.5, frames);
    }
    return vec4<f32>(from_ycocg(mix(h, centre, share)), frames);
}

// The blended picture back to linear light, sharpened by its difference from its four
// neighbours, held to their range so no edge rings.
@fragment
fn fs_sharpen(@builtin(position) p: vec4<f32>) -> @location(0) vec4<f32> {
    let px = vec2<i32>(p.xy);
    let c = textureLoad(current, inside(px), 0).rgb;
    let n = textureLoad(current, inside(px + vec2<i32>(0, -1)), 0).rgb;
    let s = textureLoad(current, inside(px + vec2<i32>(0, 1)), 0).rgb;
    let e = textureLoad(current, inside(px + vec2<i32>(1, 0)), 0).rgb;
    let w = textureLoad(current, inside(px + vec2<i32>(-1, 0)), 0).rgb;
    let lo = min(c, min(min(n, s), min(e, w)));
    let hi = max(c, max(max(n, s), max(e, w)));
    let sharp = c + (c * 4.0 - n - s - e - w) * (taa.tune.x * 0.25);
    return vec4<f32>(unsqueeze(clamp(sharp, lo, hi)), 1.0);
}

// Where a translucent draw changed the picture since the opaque world was lit: written into the
// motion target's third channel.
@fragment
fn fs_translucent(@builtin(position) p: vec4<f32>) -> @location(0) vec4<f32> {
    let px = vec2<i32>(p.xy);
    let now = squeeze(textureLoad(current, px, 0).rgb);
    let was = squeeze(textureLoad(before_alpha, px, 0).rgb);
    let d = now - was;
    return vec4<f32>(0.0, 0.0, clamp(length(d) * 4.0, 0.0, 1.0), 0.0);
}
