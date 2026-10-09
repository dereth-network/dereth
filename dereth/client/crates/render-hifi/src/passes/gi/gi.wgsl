// Bounced light over the relit world, at half resolution.
//
// Each half-resolution pixel looks along a few screen-space slices through the depth buffer. Each
// slice's hemisphere is cut into 32 sectors, spaced so that every sector carries the same share of
// cosine-weighted light. A depth sample covers the sectors between its front face and its back
// (the surface taken as a slab of some thickness), and the sectors it newly covers take its lit
// colour: one bounce of sun, sky and lamp light. Whatever no sample covers sees the sky (or, below
// the horizon, the distant ground). The result is accumulated over frames, reprojected with the
// camera, filtered with depth and normal edges, and written over the relight's flat sky term.

struct Params {
    clip_to_render: mat4x4<f32>,
    render_to_clip: mat4x4<f32>,
    prev_render_to_clip: mat4x4<f32>,
    view_to_render: mat4x4<f32>,
    viewport: vec4<f32>,
    // xyz the eye; w the frame number.
    eye: vec4<f32>,
    // xyz the eye last frame; w 1 when last frame's history may be used.
    prev_eye: vec4<f32>,
    // xyz toward the sun; w 1 when the sun is up.
    sun_dir: vec4<f32>,
    // The sun, linear; w its gain in the relight.
    sun_lin: vec4<f32>,
    // The ambient, linear; w its gain in the relight.
    amb_lin: vec4<f32>,
    // The sky's radiance, linear; w the search radius, metres.
    sky: vec4<f32>,
    // The distant ground's radiance; w the thickness taken behind each surface, metres.
    ground: vec4<f32>,
    // x pixels per metre at one metre, y strength, z the largest search radius in pixels, w view.
    tune: vec4<f32>,
    // xy the half-resolution size; z the filter's stride; w the bounce strength.
    half: vec4<f32>,
};

@group(0) @binding(0) var hdr: texture_2d<f32>;
@group(0) @binding(1) var normals: texture_2d<f32>;
@group(0) @binding(2) var depth: texture_depth_2d;
@group(0) @binding(3) var surface: texture_2d<f32>;
@group(0) @binding(4) var tex_a: texture_2d<f32>;
@group(0) @binding(5) var tex_b: texture_2d<f32>;
@group(0) @binding(6) var<uniform> params: Params;

const PI: f32 = 3.14159265;
const HALF_PI: f32 = 1.57079633;
const SLICES: i32 = 3;
const STEPS: i32 = 14;

@vertex
fn vs(@builtin(vertex_index) i: u32) -> @builtin(position) vec4<f32> {
    let x = f32((i << 1u) & 2u);
    let y = f32(i & 2u);
    return vec4<f32>(x * 2.0 - 1.0, 1.0 - y * 2.0, 0.0, 1.0);
}

fn render_at(px: vec2<f32>, d: f32) -> vec3<f32> {
    let v = params.viewport;
    let uv = (px - v.xy) / v.zw;
    let ndc = vec4<f32>(uv.x * 2.0 - 1.0, 1.0 - uv.y * 2.0, d, 1.0);
    let w = params.clip_to_render * ndc;
    return w.xyz / w.w;
}

fn world_normal(px: vec2<i32>) -> vec3<f32> {
    let nw = textureLoad(normals, px, 0);
    return normalize((params.view_to_render * vec4<f32>(nw.xyz, 0.0)).xyz + vec3<f32>(0.0, 0.0, 1e-5));
}

fn full_size() -> vec2<i32> {
    return vec2<i32>(textureDimensions(depth));
}

// The full-resolution pixel a half-resolution pixel stands for.
fn full_of(h: vec2<i32>) -> vec2<i32> {
    return min(h * 2, full_size() - vec2<i32>(1));
}

// Interleaved gradient noise, moved along each frame.
fn ign(p: vec2<f32>, frame: f32) -> f32 {
    let q = p + vec2<f32>(5.588238, 5.588238) * (frame % 64.0);
    return fract(52.9829189 * fract(dot(q, vec2<f32>(0.06711056, 5.83715e-3))));
}

fn sectors(lo: f32, hi: f32) -> u32 {
    let a = u32(clamp(lo * 32.0, 0.0, 32.0));
    let b = u32(clamp(ceil(hi * 32.0), 0.0, 32.0));
    if (b <= a) {
        return 0u;
    }
    let n = b - a;
    let run = select((1u << n) - 1u, 0xffffffffu, n >= 32u);
    return run << a;
}

// The light from a direction no sample covers: the sky above the horizon, the far ground below.
fn open_light(dir: vec3<f32>) -> vec3<f32> {
    let z = dir.z;
    let sky = params.sky.rgb * (0.75 + 0.35 * clamp(z, 0.0, 1.0));
    return mix(params.ground.rgb, sky, smoothstep(-0.12, 0.08, z));
}

@fragment
fn fs_trace(@builtin(position) pos: vec4<f32>) -> @location(0) vec4<f32> {
    let h = vec2<i32>(pos.xy);
    let fp = full_of(h);
    let d = textureLoad(depth, fp, 0);
    let nw = textureLoad(normals, fp, 0);
    if (d >= 1.0 || dot(nw.xyz, nw.xyz) < 0.25) {
        return vec4<f32>(0.0);
    }
    let px = vec2<f32>(fp) + 0.5;
    let p = render_at(px, d);
    let n = world_normal(fp);
    let to_eye = params.eye.xyz - p;
    let dist = length(to_eye);
    let v = to_eye / max(dist, 1e-4);
    let radius = params.sky.w * (1.0 + dist * 0.015);
    let radius_px = min(radius * params.tune.x / max(dist, 0.1), params.tune.z);
    let thickness = params.ground.w * (1.0 + dist * 0.02);
    let frame = params.eye.w;
    let noise_a = ign(pos.xy, frame);
    let noise_b = ign(pos.xy + vec2<f32>(37.0, 17.0), frame);
    let size = vec2<f32>(full_size());

    var light = vec3<f32>(0.0);
    var weight = 0.0;
    for (var s: i32 = 0; s < SLICES; s = s + 1) {
        let phi = (f32(s) + noise_a) / f32(SLICES) * PI;
        let dir2 = vec2<f32>(cos(phi), sin(phi));
        // The slice's direction across the surface, in render space.
        let q = render_at(px + dir2 * 4.0, d);
        var along = q - p;
        along = along - v * dot(along, v);
        let al = length(along);
        if (al < 1e-6) {
            continue;
        }
        let dp = along / al;
        let axis = normalize(cross(dp, v));
        let pn = n - axis * dot(n, axis);
        let pl = length(pn);
        if (pl < 1e-4) {
            continue;
        }
        let n_ang = sign(dot(pn, dp)) * acos(clamp(dot(pn, v) / pl, -1.0, 1.0));
        var occ = 0u;
        var bounce = vec3<f32>(0.0);
        if (radius_px >= 1.5) {
            for (var side: i32 = 0; side < 2; side = side + 1) {
                let sgn = select(-1.0, 1.0, side == 0);
                for (var k: i32 = 0; k < STEPS; k = k + 1) {
                    let t = (f32(k) + fract(noise_b + f32(side) * 0.5)) / f32(STEPS);
                    let off = max(t * t * radius_px, f32(k) + 1.0);
                    let spx = px + dir2 * sgn * off;
                    if (any(spx < vec2<f32>(0.0)) || any(spx >= size)) {
                        break;
                    }
                    let si = vec2<i32>(spx);
                    let sd = textureLoad(depth, si, 0);
                    if (sd >= 1.0) {
                        continue;
                    }
                    let sp = render_at(vec2<f32>(si) + 0.5, sd);
                    let delta = sp - p;
                    let len = length(delta);
                    if (len < 1e-3 || len > radius * 1.5) {
                        continue;
                    }
                    let back = delta - v * thickness;
                    let front_a = acos(clamp(dot(delta / len, v), -1.0, 1.0));
                    let back_a = acos(clamp(dot(normalize(back), v), -1.0, 1.0));
                    let a0 = clamp(sgn * front_a - n_ang, -HALF_PI, HALF_PI);
                    let a1 = clamp(sgn * back_a - n_ang, -HALF_PI, HALF_PI);
                    let x0 = 0.5 + 0.5 * sin(a0);
                    let x1 = 0.5 + 0.5 * sin(a1);
                    let bits = sectors(min(x0, x1), max(x0, x1));
                    let fresh = bits & ~occ;
                    if (fresh != 0u) {
                        let sn = world_normal(si);
                        // A surface lights this one only from its own front.
                        let facing = clamp(dot(sn, -delta / len) + 0.1, 0.0, 1.0);
                        let fall = 1.0 - smoothstep(radius * 0.6, radius * 1.5, len);
                        let sl = textureLoad(hdr, si, 0).rgb;
                        let share = f32(countOneBits(fresh)) / 32.0;
                        bounce = bounce + min(sl, vec3<f32>(16.0)) * share * facing * fall;
                        // What is covered but too far off still shuts out the sky.
                        occ = occ | bits;
                    }
                }
            }
        }
        // The open sectors, in eight groups of four, each with its own direction.
        var open = vec3<f32>(0.0);
        for (var g: i32 = 0; g < 8; g = g + 1) {
            let free = countOneBits(~occ & (0xfu << u32(g * 4)));
            if (free == 0u) {
                continue;
            }
            let x = (f32(g) + 0.5) / 8.0;
            let ang = n_ang + asin(clamp(x * 2.0 - 1.0, -1.0, 1.0));
            let dir = v * cos(ang) + dp * sin(ang);
            open = open + open_light(dir) * f32(free) / 32.0;
        }
        light = light + (bounce * params.half.w + open) * pl;
        weight = weight + pl;
    }
    if (weight <= 0.0) {
        return vec4<f32>(open_light(n), dist);
    }
    return vec4<f32>(light / weight, dist);
}

// The distance last frame's history held at `uv`, and its light, from the nearest texel.
@fragment
fn fs_accumulate(@builtin(position) pos: vec4<f32>) -> @location(0) vec4<f32> {
    let h = vec2<i32>(pos.xy);
    let cur = textureLoad(tex_a, h, 0);
    if (cur.a <= 0.0 || params.prev_eye.w < 0.5) {
        return cur;
    }
    let fp = full_of(h);
    let d = textureLoad(depth, fp, 0);
    let p = render_at(vec2<f32>(fp) + 0.5, d);
    let c = params.prev_render_to_clip * vec4<f32>(p, 1.0);
    if (c.w <= 0.0) {
        return cur;
    }
    let ndc = c.xy / c.w;
    let uv = vec2<f32>(ndc.x * 0.5 + 0.5, 0.5 - ndc.y * 0.5);
    if (any(uv < vec2<f32>(0.0)) || any(uv >= vec2<f32>(1.0))) {
        return cur;
    }
    let want = length(p - params.prev_eye.xyz);
    let hp = (params.viewport.xy + uv * params.viewport.zw) * 0.5 - 0.5;
    let base = vec2<i32>(floor(hp));
    let f = fract(hp);
    let hs = vec2<i32>(params.half.xy) - vec2<i32>(1);
    var sum = vec3<f32>(0.0);
    var wsum = 0.0;
    for (var j: i32 = 0; j < 4; j = j + 1) {
        let o = vec2<i32>(j & 1, j >> 1);
        let t = clamp(base + o, vec2<i32>(0), hs);
        let hv = textureLoad(tex_b, t, 0);
        let bw = select(1.0 - f.x, f.x, o.x == 1) * select(1.0 - f.y, f.y, o.y == 1);
        let ok = abs(hv.a - want) < want * 0.03 + 0.05;
        let w = select(0.0, bw, ok && hv.a > 0.0);
        sum = sum + hv.rgb * w;
        wsum = wsum + w;
    }
    if (wsum < 0.25) {
        return cur;
    }
    let hist = sum / wsum;
    // Kept within reach of this frame's value, so a light that moves does not leave a trail.
    let blend = 0.92 * clamp(wsum, 0.0, 1.0);
    return vec4<f32>(mix(cur.rgb, hist, blend), cur.a);
}

// One pass of an edge-stopping filter, at the stride the parameters give.
@fragment
fn fs_filter(@builtin(position) pos: vec4<f32>) -> @location(0) vec4<f32> {
    let h = vec2<i32>(pos.xy);
    let c = textureLoad(tex_a, h, 0);
    if (c.a <= 0.0) {
        return c;
    }
    let n = world_normal(full_of(h));
    let stride = i32(params.half.z);
    let hs = vec2<i32>(params.half.xy) - vec2<i32>(1);
    var sum = vec3<f32>(0.0);
    var wsum = 0.0;
    for (var y: i32 = -2; y <= 2; y = y + 1) {
        for (var x: i32 = -2; x <= 2; x = x + 1) {
            let t = clamp(h + vec2<i32>(x, y) * stride, vec2<i32>(0), hs);
            let s = textureLoad(tex_a, t, 0);
            if (s.a <= 0.0) {
                continue;
            }
            let k = vec2<f32>(f32(x), f32(y));
            let wk = exp(-dot(k, k) * 0.35);
            let wd = exp(-abs(s.a - c.a) / (c.a * 0.04 + 0.05));
            let sn = world_normal(full_of(t));
            let wn = pow(clamp(dot(sn, n), 0.0, 1.0), 16.0);
            let w = wk * wd * wn;
            sum = sum + s.rgb * w;
            wsum = wsum + w;
        }
    }
    return vec4<f32>(sum / max(wsum, 1e-4), c.a);
}

// The relight's sky term, as it put it in: what the bounced light replaces.
fn old_sky(n: vec3<f32>) -> vec3<f32> {
    let up = n.z * 0.5 + 0.5;
    let amb = params.amb_lin.rgb;
    let sky = amb * vec3<f32>(0.85, 0.95, 1.15);
    let ground = amb * 0.45 + params.sun_lin.rgb * vec3<f32>(0.10, 0.08, 0.05);
    return mix(ground, sky, up) * params.amb_lin.w;
}

// The bounced light at a full-resolution pixel, from the four half-resolution texels around it,
// each kept only where it lies on the same surface.
fn upsampled(fp: vec2<i32>, dist: f32, n: vec3<f32>) -> vec4<f32> {
    let hp = (vec2<f32>(fp) + 0.5) * 0.5 - 0.5;
    let base = vec2<i32>(floor(hp));
    let f = fract(hp);
    let hs = vec2<i32>(params.half.xy) - vec2<i32>(1);
    var sum = vec3<f32>(0.0);
    var wsum = 0.0;
    var best = vec3<f32>(0.0);
    var best_w = -1.0;
    for (var j: i32 = 0; j < 4; j = j + 1) {
        let o = vec2<i32>(j & 1, j >> 1);
        let t = clamp(base + o, vec2<i32>(0), hs);
        let s = textureLoad(tex_a, t, 0);
        if (s.a <= 0.0) {
            continue;
        }
        let bw = select(1.0 - f.x, f.x, o.x == 1) * select(1.0 - f.y, f.y, o.y == 1);
        let wd = exp(-abs(s.a - dist) / (dist * 0.02 + 0.03));
        let sn = world_normal(full_of(t));
        let wn = pow(clamp(dot(sn, n), 0.0, 1.0), 8.0);
        let w = (bw + 0.02) * wd * wn;
        sum = sum + s.rgb * w;
        wsum = wsum + w;
        if (wd * (wn + 0.01) > best_w) {
            best_w = wd * (wn + 0.01);
            best = s.rgb;
        }
    }
    if (wsum < 1e-3) {
        return vec4<f32>(best, select(0.0, 1.0, best_w > 0.0));
    }
    return vec4<f32>(sum / wsum, 1.0);
}

@fragment
fn fs_composite(@builtin(position) pos: vec4<f32>) -> @location(0) vec4<f32> {
    let fp = vec2<i32>(pos.xy);
    let r = textureLoad(surface, fp, 0);
    let d = textureLoad(depth, fp, 0);
    if (d >= 1.0 || r.a < 0.5) {
        return vec4<f32>(0.0);
    }
    let p = render_at(pos.xy, d);
    let dist = length(params.eye.xyz - p);
    let n = world_normal(fp);
    let e = upsampled(fp, dist, n);
    if (e.a <= 0.0) {
        return vec4<f32>(0.0);
    }
    var before = old_sky(n);
    var bounced = e.rgb * params.tune.y;
    // After dark the sky holds no light to occlude: what it brings the relight already holds,
    // and the bounce (from the lamps) only adds to it.
    if (params.sun_dir.w < 0.5) {
        bounced = max(bounced, before);
    }
    if (textureLoad(normals, fp, 0).w > 3.5) {
        // A leaf: the relight gave it the sky from all round, and the bounce never takes more
        // than half of it away (the horizons a card finds among its neighbours are leaves the
        // light passes between).
        let amb = params.amb_lin.rgb;
        let sky = amb * vec3<f32>(0.85, 0.95, 1.15);
        let ground = amb * 0.45 + params.sun_lin.rgb * vec3<f32>(0.10, 0.08, 0.05);
        before = mix(ground, sky, 0.5 + 0.3 * n.z) * params.amb_lin.w;
        bounced = max(bounced, before * 0.5);
    }
    let add = r.rgb * (bounced - before);
    return vec4<f32>(add, 0.0);
}

// The bounced light alone, for looking at.
@fragment
fn fs_view(@builtin(position) pos: vec4<f32>) -> @location(0) vec4<f32> {
    let fp = vec2<i32>(pos.xy);
    let d = textureLoad(depth, fp, 0);
    if (d >= 1.0) {
        return vec4<f32>(0.0, 0.0, 0.0, 1.0);
    }
    let p = render_at(pos.xy, d);
    let dist = length(params.eye.xyz - p);
    let n = world_normal(fp);
    let e = upsampled(fp, dist, n);
    return vec4<f32>(e.rgb * params.tune.y * 0.5, 1.0);
}
