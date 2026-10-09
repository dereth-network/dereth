// Ground-truth ambient occlusion over the world's depth: a horizon search in slices around each
// pixel, an edge-aware blur, and a multiply into the picture in linear light.

struct Params {
    // View to clip, and back.
    projection: mat4x4<f32>,
    inverse_projection: mat4x4<f32>,
    // The world viewport in target pixels: x, y, width, height.
    viewport: vec4<f32>,
    // Radius (metres), falloff share of the radius, pixels per metre at one metre, largest
    // screen radius (pixels).
    search: vec4<f32>,
    // Slices, steps per side, strength, final power.
    shape: vec4<f32>,
    // Fog start and end (metres), whether there is fog, the distance the term is gone by.
    fog: vec4<f32>,
    // Debug view, whether the target stores encoded (gamma) values, blur depth tolerance,
    // blur step in pixels.
    misc: vec4<f32>,
};

@group(0) @binding(0) var depth_tex: texture_depth_2d;
@group(0) @binding(1) var ao_in: texture_2d<f32>;
@group(0) @binding(2) var<uniform> params: Params;
@group(0) @binding(3) var picture: texture_2d<f32>;

const PI: f32 = 3.14159265;
const HALF_PI: f32 = 1.57079632;
// How much wider the search grows per metre of distance.
const RADIUS_PER_METRE: f32 = 0.08;

struct VsOut {
    @builtin(position) position: vec4<f32>,
    // The blur pass: its taps are this many pixels apart, less one.
    @location(0) @interpolate(flat) pass_index: u32,
};

@vertex
fn vs(@builtin(vertex_index) i: u32, @builtin(instance_index) k: u32) -> VsOut {
    let x = f32((i << 1u) & 2u);
    let y = f32(i & 2u);
    var out: VsOut;
    out.position = vec4<f32>(x * 2.0 - 1.0, 1.0 - y * 2.0, 0.0, 1.0);
    out.pass_index = k;
    return out;
}

fn in_viewport(p: vec2<f32>) -> bool {
    let v = params.viewport;
    return p.x >= v.x && p.y >= v.y && p.x < v.x + v.z && p.y < v.y + v.w;
}

// The view-space point under pixel centre `p` at depth `d`.
fn view_at(p: vec2<f32>, d: f32) -> vec3<f32> {
    let v = params.viewport;
    let uv = (p - v.xy) / v.zw;
    let ndc = vec2<f32>(uv.x * 2.0 - 1.0, 1.0 - uv.y * 2.0);
    let q = params.inverse_projection * vec4<f32>(ndc, d, 1.0);
    return q.xyz / q.w;
}

fn depth_at(px: vec2<i32>) -> f32 {
    return textureLoad(depth_tex, px, 0);
}

fn view_of(px: vec2<i32>) -> vec3<f32> {
    let v = params.viewport;
    let lo = vec2<i32>(v.xy);
    let hi = vec2<i32>(v.xy + v.zw) - vec2<i32>(1, 1);
    let q = clamp(px, lo, hi);
    return view_at(vec2<f32>(q) + 0.5, depth_at(q));
}

// Interleaved gradient noise.
fn ign(p: vec2<f32>) -> f32 {
    return fract(52.9829189 * fract(dot(p, vec2<f32>(0.06711056, 5.83715e-3))));
}

// A 4x4 ordered pattern, 0..1.
fn bayer4(px: vec2<i32>) -> f32 {
    let x = u32(px.x) & 3u;
    let y = u32(px.y) & 3u;
    let m = array<u32, 16>(0u, 8u, 2u, 10u, 12u, 4u, 14u, 6u, 3u, 11u, 1u, 9u, 15u, 7u, 13u, 5u);
    return (f32(m[y * 4u + x]) + 0.5) / 16.0;
}

// A sample's distance for the falloff: whatever of it comes toward the eye counts more, so a
// thin thing in front (a leaf card, an arm) shades what lies behind it less.
fn thick_length(delta: vec3<f32>, v: vec3<f32>) -> f32 {
    let toward = max(dot(delta, v), 0.0);
    return length(delta + v * (toward * params.misc.w));
}

fn fast_acos(x: f32) -> f32 {
    return acos(clamp(x, -1.0, 1.0));
}

fn inside(px: vec2<i32>) -> bool {
    return in_viewport(vec2<f32>(px) + 0.5);
}

// The surface's step along `axis` at `px`, from the side that continues the surface best.
fn tangent(px: vec2<i32>, p: vec3<f32>, axis: vec2<i32>) -> vec3<f32> {
    let a1 = px - axis;
    let a2 = px - axis * 2;
    let b1 = px + axis;
    let b2 = px + axis * 2;
    var err_a = 1e30;
    var err_b = 1e30;
    var ta = vec3<f32>(0.0);
    var tb = vec3<f32>(0.0);
    if (inside(a2)) {
        let q1 = view_of(a1);
        let q2 = view_of(a2);
        err_a = abs(2.0 * q1.z - q2.z - p.z);
        ta = p - q1;
    }
    if (inside(b2)) {
        let q1 = view_of(b1);
        let q2 = view_of(b2);
        err_b = abs(2.0 * q1.z - q2.z - p.z);
        tb = q1 - p;
    }
    if (err_a < err_b) {
        return ta;
    }
    return tb;
}

// The search: visibility and the view distance, or (1, -1) where there is nothing to occlude.
@fragment
fn fs_ao(@builtin(position) frag: vec4<f32>) -> @location(0) vec2<f32> {
    let px = vec2<i32>(frag.xy);
    if (!in_viewport(frag.xy)) {
        return vec2<f32>(1.0, -1.0);
    }
    let d = depth_at(px);
    if (d >= 1.0) {
        return vec2<f32>(1.0, -1.0);
    }
    let p = view_at(frag.xy, d);
    let dist = length(p);
    let depth_z = abs(p.z);

    // The normal from depth: on each axis, the side whose two neighbours run on in line with
    // this pixel, so a crease or an object's edge never mixes two surfaces.
    let dx = tangent(px, p, vec2<i32>(1, 0));
    let dy = tangent(px, p, vec2<i32>(0, 1));
    let v = normalize(-p);
    var n = normalize(cross(dx, dy));
    if (dot(n, v) < 0.0) {
        n = -n;
    }
    // Nudged off the surface along the normal, against self-occlusion on coarse depth.
    let origin = p + n * (depth_z * 0.0008);

    // Far things search wider, so a house across the square still sits on the ground.
    let radius = max(params.search.x, dist * RADIUS_PER_METRE);
    let screen_radius = min(radius * params.search.z / depth_z, params.search.w);
    if (screen_radius < 1.5) {
        return vec2<f32>(1.0, dist);
    }
    let falloff_range = params.search.y * radius;
    let falloff_from = radius - falloff_range;

    let slices = u32(params.shape.x);
    let steps = u32(params.shape.y);
    let rot_noise = bayer4(px);
    let step_noise = ign(frag.xy);
    let min_s = 1.3 / screen_radius;

    var visibility = 0.0;
    for (var slice = 0u; slice < slices; slice++) {
        let phi = (f32(slice) + rot_noise) * PI / f32(slices);
        let omega = vec2<f32>(cos(phi), sin(phi));
        // The view-space direction of moving along omega on screen.
        let along = view_at(frag.xy + omega, d) - p;
        let dir = normalize(along);
        let ortho = dir - dot(dir, v) * v;
        let axis = normalize(cross(ortho, v));
        let pn = n - axis * dot(n, axis);
        let pn_len = length(pn);
        let sign_n = select(-1.0, 1.0, dot(ortho, pn) >= 0.0);
        let cos_n = clamp(dot(pn, v) / max(pn_len, 1e-5), 0.0, 1.0);
        let ang_n = sign_n * fast_acos(cos_n);

        let low_plus = cos(ang_n + HALF_PI);
        let low_minus = cos(ang_n - HALF_PI);
        var hc_plus = low_plus;
        var hc_minus = low_minus;

        for (var step = 0u; step < steps; step++) {
            let fine = fract(step_noise + f32(step) * 0.61803398);
            var s = (f32(step) + fine) / f32(steps);
            s = s * s + min_s;
            let off = omega * (s * screen_radius);
            let q_plus = frag.xy + off;
            let q_minus = frag.xy - off;
            if (in_viewport(q_plus)) {
                let qp = vec2<i32>(q_plus);
                let dq = depth_at(qp);
                let sp = view_at(vec2<f32>(qp) + 0.5, dq);
                let delta = sp - origin;
                let len = thick_length(delta, v);
                let w = clamp((falloff_from + falloff_range - len) / falloff_range, 0.0, 1.0);
                let shc = mix(low_plus, dot(normalize(delta), v), w);
                hc_plus = max(hc_plus, select(shc, low_plus, dq >= 1.0));
            }
            if (in_viewport(q_minus)) {
                let qm = vec2<i32>(q_minus);
                let dq = depth_at(qm);
                let sm = view_at(vec2<f32>(qm) + 0.5, dq);
                let delta = sm - origin;
                let len = thick_length(delta, v);
                let w = clamp((falloff_from + falloff_range - len) / falloff_range, 0.0, 1.0);
                let shc = mix(low_minus, dot(normalize(delta), v), w);
                hc_minus = max(hc_minus, select(shc, low_minus, dq >= 1.0));
            }
        }
        var h0 = -fast_acos(hc_minus);
        var h1 = fast_acos(hc_plus);
        h0 = ang_n + clamp(h0 - ang_n, -HALF_PI, HALF_PI);
        h1 = ang_n + clamp(h1 - ang_n, -HALF_PI, HALF_PI);
        let sin_n = sin(ang_n);
        let arc0 = (cos_n + 2.0 * h0 * sin_n - cos(2.0 * h0 - ang_n)) * 0.25;
        let arc1 = (cos_n + 2.0 * h1 * sin_n - cos(2.0 * h1 - ang_n)) * 0.25;
        visibility += pn_len * (arc0 + arc1);
    }
    visibility = clamp(visibility / f32(slices), 0.0, 1.0);
    return vec2<f32>(visibility, dist);
}

// The edge-aware blur: neighbours at a like distance count, others do not.
@fragment
fn fs_denoise(@builtin(position) frag: vec4<f32>, @location(0) @interpolate(flat) pass_index: u32) -> @location(0) vec2<f32> {
    let px = vec2<i32>(frag.xy);
    let c = textureLoad(ao_in, px, 0).xy;
    if (c.y < 0.0) {
        return c;
    }
    let tol = max(c.y * params.misc.z, 0.05);
    let stride = i32(pass_index) + 1;
    var sum = 0.0;
    var weight = 0.0;
    var sum_all = 0.0;
    var weight_all = 0.0;
    for (var y = -2; y <= 2; y++) {
        for (var x = -2; x <= 2; x++) {
            let q = px + vec2<i32>(x, y) * stride;
            if (!in_viewport(vec2<f32>(q) + 0.5)) {
                continue;
            }
            let s = textureLoad(ao_in, q, 0).xy;
            if (s.y < 0.0) {
                continue;
            }
            let spatial = exp(-f32(x * x + y * y) * 0.18);
            let dw = max(0.0, 1.0 - abs(s.y - c.y) / tol);
            let w = spatial * dw * dw;
            sum += s.x * w;
            weight += w;
            sum_all += s.x * spatial;
            weight_all += spatial;
        }
    }
    // A pixel with few neighbours on its own surface (a leaf seen through a gap) takes the
    // value around it rather than standing out as a speck.
    let own = sum / max(weight, 1e-5);
    let around = sum_all / max(weight_all, 1e-5);
    let alone = clamp(weight / max(0.35 * weight_all, 1e-5), 0.0, 1.0);
    return vec2<f32>(mix(max(own, around), own, alone), c.y);
}

// The term as applied: strengthened, raised to its power, and faded with fog and distance.
fn final_term(px: vec2<i32>) -> f32 {
    let s = textureLoad(ao_in, px, 0).xy;
    if (s.y < 0.0) {
        return 1.0;
    }
    let vis = s.x;
    // Never darker than a floor, so a deep fold (a hand at the hip) does not go black, nor a
    // field of grass, whose blades all stand within reach of each other. The term darkens the
    // whole picture, the sunlit part too, so the floor is a high one.
    let ao = max(pow(clamp(1.0 - (1.0 - vis) * params.shape.z, 0.0, 1.0), params.shape.w), 0.55);
    var keep = 1.0 - smoothstep(params.fog.w * 0.6, params.fog.w, s.y);
    if (params.fog.z > 0.5) {
        let f = clamp((s.y - params.fog.x) / max(params.fog.y - params.fog.x, 1.0), 0.0, 1.0);
        keep *= 1.0 - f;
    }
    return mix(1.0, ao, keep);
}

fn encode(t: f32) -> f32 {
    if (params.misc.y > 0.5) {
        return pow(t, 1.0 / 2.2);
    }
    return t;
}

// The multiply into the picture: the blend takes the target times this.
@fragment
fn fs_apply(@builtin(position) frag: vec4<f32>) -> @location(0) vec4<f32> {
    let px = vec2<i32>(frag.xy);
    if (!in_viewport(frag.xy)) {
        return vec4<f32>(1.0);
    }
    let t = encode(final_term(px));
    return vec4<f32>(t, t, t, 1.0);
}

// The multiply into the re-shaded picture where the bounced light is drawn: there `picture` is
// the normals, and only the surfaces it does not light take the occlusion -- a building's or a
// dungeon's cells, and the things standing in them, lit as indoors.
@fragment
fn fs_apply_inside(@builtin(position) frag: vec4<f32>) -> @location(0) vec4<f32> {
    let px = vec2<i32>(frag.xy);
    if (!in_viewport(frag.xy)) {
        return vec4<f32>(1.0);
    }
    let kind = textureLoad(picture, px, 0).w;
    let inside = (kind > 1.2 && kind < 1.3) || (kind > 1.5 && kind < 2.5);
    if (!inside) {
        return vec4<f32>(1.0);
    }
    let t = encode(final_term(px));
    return vec4<f32>(t, t, t, 1.0);
}

// Occlusion with light bounced between the occluding surfaces: bright surfaces darken less, and
// what darkening there is keeps their own hue rather than going grey.
fn multi_bounce(ao: f32, albedo: vec3<f32>) -> vec3<f32> {
    let a = 2.0404 * albedo - 0.3324;
    let b = -4.7951 * albedo + 0.6417;
    let c = 2.7552 * albedo + 0.6903;
    return max(vec3<f32>(ao), ((ao * a + b) * ao + c) * ao);
}

fn decode3(c: vec3<f32>) -> vec3<f32> {
    if (params.misc.y > 0.5) {
        return pow(max(c, vec3<f32>(0.0)), vec3<f32>(2.2));
    }
    return c;
}

fn encode3(c: vec3<f32>) -> vec3<f32> {
    if (params.misc.y > 0.5) {
        return pow(max(c, vec3<f32>(0.0)), vec3<f32>(1.0 / 2.2));
    }
    return c;
}

// The picture times the term, in linear light, into another picture.
@fragment
fn fs_shade(@builtin(position) frag: vec4<f32>) -> @location(0) vec4<f32> {
    let px = vec2<i32>(frag.xy);
    let c = textureLoad(picture, px, 0);
    if (!in_viewport(frag.xy)) {
        return c;
    }
    let t = final_term(px);
    let lin = decode3(c.rgb);
    // The lit colour stands in for the surface's albedo, lifted, as legacy light is dim.
    let albedo = clamp(lin * 1.2, vec3<f32>(0.0), vec3<f32>(0.8));
    let shaded = lin * multi_bounce(t, albedo);
    return vec4<f32>(encode3(shaded), c.a);
}

// The term alone, as grey.
@fragment
fn fs_view(@builtin(position) frag: vec4<f32>) -> @location(0) vec4<f32> {
    let px = vec2<i32>(frag.xy);
    if (!in_viewport(frag.xy)) {
        return textureLoad(picture, px, 0);
    }
    let t = encode(final_term(px));
    return vec4<f32>(t, t, t, 1.0);
}
