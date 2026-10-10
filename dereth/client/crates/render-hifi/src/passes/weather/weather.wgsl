// Weather: wet ground, puddles, splashes and a snow cover drawn over the replayed world by a
// full-screen pass, and the falling rain and snow drawn over that as particles fixed in the world;
// all of it kept off whatever stands under a roof.

struct Weather {
    // Render space, height up, to clip space.
    clip_from_render: mat4x4<f32>,
    // Clip space to render space.
    render_from_clip: mat4x4<f32>,
    // The world viewport: x, y, width, height, in target pixels.
    viewport: vec4<f32>,
    // The eye, render space; w the time in seconds.
    eye: vec4<f32>,
    // Toward the sun; w its brightness.
    sun_dir: vec4<f32>,
    // The sun's colour, linear; w how much of a day it is.
    sun_col: vec4<f32>,
    // The fog's colour, linear; w 1 when the picture is read and written as linear light.
    fog: vec4<f32>,
    // The ambient colour, linear; w its level.
    ambient: vec4<f32>,
    // x the quality; y how much of what falls is snow, 0 rain to 1 snow; z how much falls, 0 to
    // 1; w radians per pixel.
    params: vec4<f32>,
    // x, y where the world fog begins and is full; z the exposure the re-shaded world's linear
    // light is shown at; w unused.
    fogp: vec4<f32>,
    // x the period the time wraps over, seconds; y, z where render space's origin sits in the
    // world, wrapped to the noise's period; w that period, metres.
    frame: vec4<f32>,
    // x 1 for a view of the masks (the landscape, the road and the puddles), 2 for the shelter.
    debug: vec4<f32>,
    // The overhead map: x, y its centre, render space; z how far it reaches; w its side, texels.
    cover: vec4<f32>,
    // x the height its depth starts at; y the depth's span; z 1 when it holds anything.
    cover_span: vec4<f32>,
    // The sun's light on a snow cover, as the frame lights the world.
    snow_sun: vec4<f32>,
    // The sky's light on it; w 1 when the picture is the re-shaded world's linear light.
    snow_amb: vec4<f32>,
    // How far the rain (x) and the snow (y) have fallen, and how far the wind has carried both
    // east (z) and north (w), metres, each wrapped to the lattices' period.
    fall: vec4<f32>,
    // The rain's speed of fall (x) and the snow's (y), and the wind's velocity east (z) and north
    // (w), metres a second.
    motion: vec4<f32>,
    // x, y where render space's origin sits in the world, wrapped to the lattices' period; z that
    // period, metres.
    lattice: vec4<f32>,
    // Where each of rain's three lattices ends in the instances drawn: x, y, z; w unused.
    rain_counts: vec4<f32>,
    // Where each of snow's ends, counted on from rain's last.
    snow_counts: vec4<f32>,
};

@group(0) @binding(0) var<uniform> W: Weather;
@group(0) @binding(1) var colour: texture_2d<f32>;
// The depth each pixel sees: in a frame that steps indoors, the rooms' where they are drawn, and
// through their openings the depth of what was drawn before the step.
@group(0) @binding(2) var depth: texture_depth_2d;
@group(0) @binding(3) var field_height: texture_2d<f32>;
@group(0) @binding(4) var field_normal: texture_2d<f32>;
@group(0) @binding(5) var field_class: texture_2d<u32>;
@group(0) @binding(6) var<uniform> field: TerrainFieldParams;
@group(0) @binding(7) var shelter_depth: texture_depth_2d;

const PI: f32 = 3.14159265;

fn decode(c: vec3<f32>) -> vec3<f32> {
    if (W.fog.w > 0.5) {
        return c;
    }
    return srgb_to_linear(c);
}

fn encode(c: vec3<f32>) -> vec3<f32> {
    if (W.fog.w > 0.5) {
        return c;
    }
    return linear_to_srgb(clamp(c, vec3<f32>(0.0), vec3<f32>(1.0)));
}

// --- noise ------------------------------------------------------------------------------------

fn pcg(v: u32) -> u32 {
    let state = v * 747796405u + 2891336453u;
    let word = ((state >> ((state >> 28u) + 4u)) ^ state) * 277803737u;
    return (word >> 22u) ^ word;
}

fn rand2(c: vec2<i32>, salt: u32) -> vec2<f32> {
    let h = pcg(bitcast<u32>(c.x) * 73856093u ^ pcg(bitcast<u32>(c.y) * 19349663u ^ salt));
    let h2 = pcg(h ^ 0x9E3779B9u);
    return vec2<f32>(f32(h & 0xFFFFu), f32(h2 & 0xFFFFu)) / 65535.0;
}

fn rand1(c: vec2<i32>, salt: u32) -> f32 {
    return rand2(c, salt).x;
}

// The world position of render-space `q`, wrapped to the noise's period. Every lattice below
// repeats over that period, so the noise is fixed to the world wherever render space sits.
fn world_xy(q: vec2<f32>) -> vec2<f32> {
    return q + W.frame.yz;
}

// Lattice cells across the noise's period at `s` cells per metre.
fn period_cells(s: f32) -> i32 {
    return max(i32(round(W.frame.w * s)), 1);
}

fn wrap_cell(c: vec2<i32>, n: i32) -> vec2<i32> {
    return ((c % vec2<i32>(n)) + vec2<i32>(n)) % vec2<i32>(n);
}

// Value noise at world point `p`, `s` lattice cells per metre.
fn wnoise(p: vec2<f32>, s: f32, salt: u32) -> f32 {
    let n = period_cells(s);
    let x = p * s;
    let i = vec2<i32>(floor(x));
    let f = fract(x);
    let u = f * f * (3.0 - 2.0 * f);
    let a = rand1(wrap_cell(i, n), salt);
    let b = rand1(wrap_cell(i + vec2<i32>(1, 0), n), salt);
    let c = rand1(wrap_cell(i + vec2<i32>(0, 1), n), salt);
    let d = rand1(wrap_cell(i + vec2<i32>(1, 1), n), salt);
    return mix(mix(a, b, u.x), mix(c, d, u.x), u.y);
}

// Four octaves of it, from `s` cells per metre up.
fn wfbm(p: vec2<f32>, s: f32, salt: u32) -> f32 {
    var sum = 0.0;
    var a = 0.5;
    var k = s;
    for (var i = 0; i < 4; i = i + 1) {
        sum = sum + a * wnoise(p, k, salt + u32(i) * 101u);
        k = k * 2.0;
        a = a * 0.5;
    }
    return sum;
}

// --- time ------------------------------------------------------------------------------------

// The time wraps over a period. Every motion is given a whole number of cycles in that period, so
// it is where it was when the time wraps, and the wrap is never seen.

// A rate, cycles a second, nudged to a whole number of cycles over the period.
fn per_period(rate: f32) -> f32 {
    let p = W.frame.x;
    return max(round(rate * p), 1.0) / p;
}

// An angular speed, radians a second, nudged the same way.
fn turns_per_period(w: f32) -> f32 {
    let p = W.frame.x;
    return 2.0 * PI * max(round(w * p / (2.0 * PI)), 1.0) / p;
}

// --- what stands overhead ---------------------------------------------------------------------

// The height of the highest static surface over render-space `q`, or far below anything when
// nothing stands there or the map does not reach it.
fn shelter_height(q: vec2<f32>) -> f32 {
    if (W.cover_span.z < 0.5) {
        return -1.0e9;
    }
    let s = (q - W.cover.xy) / W.cover.z;
    if (abs(s.x) >= 1.0 || abs(s.y) >= 1.0) {
        return -1.0e9;
    }
    let side = W.cover.w;
    let uv = vec2<f32>(s.x * 0.5 + 0.5, 0.5 - s.y * 0.5);
    let t = clamp(vec2<i32>(uv * side), vec2<i32>(0), vec2<i32>(i32(side) - 1));
    let d = textureLoad(shelter_depth, t, 0);
    if (d >= 1.0) {
        return -1.0e9;
    }
    return W.cover_span.x - d * W.cover_span.y;
}

// Whether something stands more than `margin` metres over `p`: 1 sheltered, 0 open.
fn sheltered(p: vec3<f32>, margin: f32) -> f32 {
    return select(0.0, 1.0, shelter_height(p.xy) > p.z + margin);
}

// The share of the ground round `p` that something shelters, over a patch half a metre across
// whose middle wanders with the world's noise, so the dry ground under a roof's edge ends in a
// soft, uneven line rather than in the map's texels.
fn shelter_share(p: vec3<f32>, margin: f32) -> f32 {
    let wp = world_xy(p.xy);
    let wander = vec2<f32>(wnoise(wp, 1.3, 71u), wnoise(wp, 1.3, 73u)) - vec2<f32>(0.5);
    let centre = p.xy + wander * 0.7;
    var n = 0.0;
    for (var j = -1; j <= 1; j = j + 1) {
        for (var i = -1; i <= 1; i = i + 1) {
            let o = vec2<f32>(f32(i), f32(j)) * 0.25;
            n = n + sheltered(vec3<f32>(centre + o, p.z), margin);
        }
    }
    return smoothstep(0.0, 1.0, n / 9.0);
}

// --- the landscape under the weather ----------------------------------------------------------

fn field_vertex_class(t: vec2<i32>) -> u32 {
    let size = vec2<i32>(field.origin_size.zw);
    return textureLoad(field_class, clamp(t, vec2<i32>(0), size - vec2<i32>(1)), 0).r;
}

// How much of the ground at render-space `p` the painted road covers, 0..1: the road shapes the
// landscape draws from the road corners of the cell, with soft, slightly uneven edges.
fn road_at(p: vec2<f32>, wp: vec2<f32>) -> f32 {
    if (field.origin_size.z < 2.0) {
        return 0.0;
    }
    let t = (p - field.origin_size.xy) / 24.0;
    let c = floor(t);
    let i = vec2<i32>(c);
    let dx = (t.x - c.x) * 24.0;
    let dy = (t.y - c.y) * 24.0;
    let a = u32(field_vertex_class(i) == 7u);
    let b = u32(field_vertex_class(i + vec2<i32>(0, 1)) == 7u);
    let cc = u32(field_vertex_class(i + vec2<i32>(1, 0)) == 7u);
    let d = u32(field_vertex_class(i + vec2<i32>(1, 1)) == 7u);
    let code = a | (b << 1u) | (cc << 2u) | (d << 3u);
    if (code == 0u) {
        return 0.0;
    }
    if (code == 15u) {
        return 1.0;
    }
    // Signed distance to the road's edge, metres, negative on the road.
    let k = 0.70710678;
    let w = 5.0;
    let f = 19.0;
    var sd = 24.0;
    switch code {
        case 1u: { sd = (dx + dy - w) * k; }
        case 2u: { sd = (f - (dy - dx)) * k; }
        case 4u: { sd = ((dy - dx) + f) * k; }
        case 8u: { sd = (24.0 + f - (dx + dy)) * k; }
        case 3u: { sd = dx - w; }
        case 5u: { sd = dy - w; }
        case 12u: { sd = f - dx; }
        case 10u: { sd = f - dy; }
        case 9u: { sd = (abs(dx - dy) - w) * k; }
        case 6u: { sd = (abs(dx + dy - 24.0) - w) * k; }
        case 7u: { sd = min(dx - w, dy - w); }
        case 11u: { sd = min(dx - w, f - dy); }
        case 13u: { sd = min(f - dx, dy - w); }
        case 14u: { sd = min(f - dx, f - dy); }
        default: { sd = 24.0; }
    }
    let wobble = (wnoise(wp, 0.3, 61u) - 0.5) * 1.8 + (wnoise(wp, 1.1, 67u) - 0.5) * 0.6;
    return 1.0 - smoothstep(-1.0, 1.0, sd + wobble);
}

// How much of the ground at render-space `p` is of ground class `kind` (water 6, snow 5),
// between the field's vertices.
fn class_share(p: vec2<f32>, kind: u32) -> f32 {
    if (field.origin_size.z < 2.0) {
        return 0.0;
    }
    let t = (p - field.origin_size.xy) / 24.0;
    let c = floor(t);
    let f = t - c;
    let i = vec2<i32>(c);
    let a = f32(field_vertex_class(i) == kind);
    let b = f32(field_vertex_class(i + vec2<i32>(1, 0)) == kind);
    let cc = f32(field_vertex_class(i + vec2<i32>(0, 1)) == kind);
    let d = f32(field_vertex_class(i + vec2<i32>(1, 1)) == kind);
    return mix(mix(a, b, f.x), mix(cc, d, f.x), f.y);
}

// --- the world on screen ----------------------------------------------------------------------

fn ndc_of(px: vec2<f32>) -> vec2<f32> {
    let v = W.viewport;
    let s = (px - v.xy) / v.zw;
    return vec2<f32>(s.x * 2.0 - 1.0, 1.0 - s.y * 2.0);
}

fn px_of(ndc: vec2<f32>) -> vec2<f32> {
    let v = W.viewport;
    return v.xy + vec2<f32>(ndc.x * 0.5 + 0.5, 0.5 - ndc.y * 0.5) * v.zw;
}

fn world_at(ndc: vec2<f32>, d: f32) -> vec3<f32> {
    let p = W.render_from_clip * vec4<f32>(ndc, d, 1.0);
    return p.xyz / p.w;
}

fn in_view(px: vec2<f32>) -> bool {
    let v = W.viewport;
    return px.x >= v.x && px.y >= v.y && px.x < v.x + v.z && px.y < v.y + v.w;
}

fn depth_at(px: vec2<i32>) -> f32 {
    return textureLoad(depth, px, 0);
}

fn point_at(px: vec2<i32>) -> vec3<f32> {
    let c = vec2<f32>(px) + vec2<f32>(0.5);
    return world_at(ndc_of(c), depth_at(px));
}

fn picture(px: vec2<i32>) -> vec3<f32> {
    return decode(textureLoad(colour, px, 0).rgb);
}

// The normal of the surface at `px`, from its neighbours' depth: on each axis the nearer of the
// two neighbours, so an edge never bends it.
fn normal_at(px: vec2<i32>, p: vec3<f32>) -> vec3<f32> {
    let l = point_at(px - vec2<i32>(1, 0));
    let r = point_at(px + vec2<i32>(1, 0));
    let u = point_at(px - vec2<i32>(0, 1));
    let d = point_at(px + vec2<i32>(0, 1));
    var dx = r - p;
    if (length(p - l) < length(dx)) {
        dx = p - l;
    }
    var dy = d - p;
    if (length(p - u) < length(dy)) {
        dy = p - u;
    }
    var n = normalize(cross(dx, dy));
    let to_eye = W.eye.xyz - p;
    if (dot(n, to_eye) < 0.0) {
        n = -n;
    }
    return n;
}

// The overcast sky in direction `r`, as a puddle mirrors it: the fog's colour `fog`, dimmer
// toward the horizon and brighter toward the zenith. It does not read the picture, so a reflection
// of it has no edge where the screen or a roof would cut it.
fn sky_colour(r: vec3<f32>, fog: vec3<f32>) -> vec3<f32> {
    return fog * (0.7 + 0.5 * clamp(r.z, 0.0, 1.0));
}

// What a puddle mirrors along `r`: what the march along it found (`found`, its colour and how
// sure the march is of it, 0 to 1), over the sky in that direction (the fog's colour `fog`) as far
// as the march is unsure. Where the march found nothing (it left the picture, ran behind what is
// drawn nearer, or met a surface edge-on) the puddle holds the sky, never a dark hole.
fn mirrored(found: vec4<f32>, r: vec3<f32>, fog: vec3<f32>) -> vec3<f32> {
    return mix(sky_colour(r, fog), found.rgb, clamp(found.a, 0.0, 1.0));
}

// The world reflected along `r` from `p`, marched over the depth on screen: (colour, how sure the
// march is of it). A ray that runs behind something drawn in front of it (a body, a post, the near
// edge of a wall) has met nothing there and marches on past it; a crossing counts only where the
// ray goes behind a surface by less than a surface's thickness, on its face rather than edge-on,
// clear of the puddle's own ground and away from the screen's edges, and how sure the march is
// fades with each of those, so the mirror fades into the sky rather than smearing an edge along
// the ground. A ray that finds nothing but goes up under a roof (a porch's, an eave's), which the
// picture never shows from below, sees the shade under it: what the picture last showed along the
// ray, darkened. One that ends hidden behind something nearer (past a post) is taken half to see
// what hid it rather than the open sky.
fn reflect_world(p: vec3<f32>, r: vec3<f32>, dist: f32, steps: i32) -> vec4<f32> {
    if (steps == 0) {
        return vec4<f32>(0.0);
    }
    var t = 0.15 + dist * 0.01;
    var last = 0.0;
    // Whether the step before was in front of what is drawn where it fell.
    var clear = true;
    // What last hid the ray, and whether it is hidden still; and what the picture last showed
    // where the ray passed.
    var hidden = vec3<f32>(0.0);
    var hid = 0.0;
    var seen = vec3<f32>(0.0);
    let grow = 1.0 + 6.0 / f32(steps);
    for (var i = 0; i < steps; i = i + 1) {
        let q = p + r * t;
        let c = W.clip_from_render * vec4<f32>(q, 1.0);
        if (c.w <= 0.0) {
            break;
        }
        let ndc = c.xyz / c.w;
        let px = px_of(ndc.xy);
        if (!in_view(px)) {
            break;
        }
        let sd = depth_at(vec2<i32>(px));
        seen = picture(vec2<i32>(px));
        if (sd < ndc.z && sd < 1.0) {
            // Behind what is drawn here: where did the ray cross it? Halve the last step a few
            // times when the step before was clear of it; after a stretch behind something
            // nearer, the crossing is where the ray now is.
            var hit = t;
            if (clear) {
                var lo = last;
                var hi = t;
                for (var b = 0; b < 6; b = b + 1) {
                    let mid = 0.5 * (lo + hi);
                    let mc = W.clip_from_render * vec4<f32>(p + r * mid, 1.0);
                    let mn = mc.xyz / mc.w;
                    if (depth_at(vec2<i32>(px_of(mn.xy))) < mn.z) {
                        hi = mid;
                    } else {
                        lo = mid;
                    }
                }
                hit = hi;
            }
            let hq = p + r * hit;
            let hc = W.clip_from_render * vec4<f32>(hq, 1.0);
            let hn = hc.xyz / hc.w;
            let hp = vec2<i32>(px_of(hn.xy));
            let s = point_at(hp);
            // How far behind the drawn surface the crossing lies, against the thickness a
            // surface is taken to have.
            let behind = length(hq - W.eye.xyz) - length(s - W.eye.xyz);
            let thick = 0.15 + 0.04 * hit + 0.01 * dist;
            // Only what stands above the puddle is mirrored in it, never the ground beside it.
            if (behind < thick && s.z > p.z + 0.1 && length(s - p) > 0.3) {
                // On a surface's face: a crossing at its edge, met edge-on, smears.
                let n = normal_at(hp, s);
                let facing = smoothstep(0.05, 0.3, -dot(n, r));
                // Away from the screen's edges, where the march runs out of picture.
                let e = abs(hn.xy);
                let edge = (1.0 - smoothstep(0.8, 1.0, e.x)) * (1.0 - smoothstep(0.8, 1.0, e.y));
                let firm = 1.0 - smoothstep(thick * 0.5, thick, behind);
                // Clear of the ground the puddle lies on.
                let off = smoothstep(0.1, 0.4, s.z - p.z) * smoothstep(0.3, 0.8, length(s - p));
                return vec4<f32>(picture(hp), facing * edge * firm * off);
            }
            clear = false;
            hidden = seen;
            hid = 1.0;
        } else {
            clear = true;
            hid = 0.0;
        }
        last = t;
        t = t * grow + 0.05;
    }
    // Up under a roof: whether a roof stands over the ray within a few metres of the puddle.
    var roofed = 0.0;
    for (var k = 0; k < 4; k = k + 1) {
        roofed = max(roofed, sheltered(p + r * (0.75 * exp2(f32(k))), 0.1));
    }
    if (roofed > 0.5) {
        return vec4<f32>(seen * 0.6, 0.85);
    }
    return vec4<f32>(hidden, 0.5 * hid);
}

// --- rain on the ground -----------------------------------------------------------------------

// The ripples of drops landing in a puddle at `q`: a slope to bend its normal by.
fn ripples(q: vec2<f32>, t: f32) -> vec2<f32> {
    var g = vec2<f32>(0.0);
    for (var layer = 0; layer < 2; layer = layer + 1) {
        let scale = 0.5 + f32(layer) * 0.3;
        let n = period_cells(1.0 / scale);
        let s = q / scale + vec2<f32>(f32(layer) * 0.37, f32(layer) * 0.71);
        let base = vec2<i32>(floor(s));
        for (var j = -1; j <= 1; j = j + 1) {
            for (var i = -1; i <= 1; i = i + 1) {
                let cell = base + vec2<i32>(i, j);
                let r = rand2(wrap_cell(cell, n), 11u + u32(layer));
                let centre = vec2<f32>(cell) + vec2<f32>(0.2) + r * 0.6;
                let phase = fract(t * per_period(0.9 + r.x * 0.5) + r.y);
                let d = s - centre;
                let len = length(d);
                let radius = phase * 0.9;
                let x = len - radius;
                let ring = sin(x * 26.0) * exp(-x * x * 90.0) * (1.0 - phase) * (1.0 - phase);
                if (len > 0.0001) {
                    g = g + d / len * ring;
                }
            }
        }
    }
    return g * 0.07;
}

// Splashes where drops land on upward ground at `q`: how bright, 0..1.
fn splashes(q: vec2<f32>, t: f32, footprint: f32) -> f32 {
    var s = 0.0;
    let size = 0.2;
    let n = period_cells(1.0 / size);
    let c = q / size;
    let base = vec2<i32>(floor(c));
    for (var j = -1; j <= 1; j = j + 1) {
        for (var i = -1; i <= 1; i = i + 1) {
            let cell = base + vec2<i32>(i, j);
            let r = rand2(wrap_cell(cell, n), 23u);
            let centre = (vec2<f32>(cell) + vec2<f32>(0.15) + r * 0.7) * size;
            let rate = per_period(1.7 + r.y);
            let phase = fract(t * rate + r.x * 7.0);
            let life = 0.16;
            if (phase < life) {
                let a = phase / life;
                let d = length(q - centre);
                // A crown that widens and fades, and the drop's bright landing point.
                let radius = 0.006 + a * 0.035;
                let width = max(0.004, footprint * 0.7);
                let ring = exp(-pow((d - radius) / width, 2.0)) * (1.0 - a);
                let landing = exp(-pow(d / max(0.006, footprint), 2.0)) * (1.0 - a) * (1.0 - a);
                // Drops smaller than a pixel give their light to the pixel as a whole.
                let cover = clamp(0.03 / max(footprint, 0.001), 0.0, 1.0);
                s = s + (ring * 0.5 + landing * 0.6) * cover;
            }
        }
    }
    return clamp(s, 0.0, 1.0);
}

// --- the ground in the snow ------------------------------------------------------------------

// The colour of a snow cover facing `n`, lit as the frame lights the surface it lies on, in the
// picture's terms: a plain white that takes the light's brightness, not its colour, so it is white
// under a warm or a cool light alike, and shades with the slope as the surface does.
fn snow_colour(n: vec3<f32>) -> vec3<f32> {
    let sun = max(dot(n, W.sun_dir.xyz), 0.0);
    if (W.snow_amb.w > 0.5) {
        // The re-shaded world's linear light, before exposure: the light per pixel's own gains.
        let e = luminance(srgb_to_linear(W.snow_sun.rgb)) * 2.6 * sun
            + luminance(srgb_to_linear(W.snow_amb.rgb)) * 0.85;
        // Lifted a little by day toward the white the exposure shows, as on the ordinary frame.
        let white = 1.0 / max(W.fogp.z, 0.1);
        return vec3<f32>(0.85 * (e + max(white - e, 0.0) * 0.6 * W.sun_col.w));
    }
    // The ordinary frame: the vertex light's sum, clamped as it clamps it, lifted a little by day
    // so fresh snow under the overcast reads as white rather than as grey.
    let e = clamp(luminance(W.snow_amb.rgb + W.snow_sun.rgb * sun), 0.0, 1.0);
    return srgb_to_linear(vec3<f32>(0.92 * (e + (1.0 - e) * 0.6 * W.sun_col.w)));
}

// Whether `p` lies on the highest solid surface over it, as the overhead map holds it: a roof, a
// rock, a platform's top, rather than a body or anything that moves, which the map does not hold.
// Of the four texels round `p`, the one whose height is nearest its own is taken, so a sloping
// roof is found to its edges.
fn static_top(p: vec3<f32>) -> f32 {
    if (W.cover_span.z < 0.5) {
        return 0.0;
    }
    let s = (p.xy - W.cover.xy) / W.cover.z;
    if (abs(s.x) >= 0.999 || abs(s.y) >= 0.999) {
        return 0.0;
    }
    let side = W.cover.w;
    let uv = vec2<f32>(s.x * 0.5 + 0.5, 0.5 - s.y * 0.5) * side - vec2<f32>(0.5);
    let base = vec2<i32>(floor(uv));
    var nearest = 1.0e9;
    for (var j = 0; j <= 1; j = j + 1) {
        for (var i = 0; i <= 1; i = i + 1) {
            let t = clamp(base + vec2<i32>(i, j), vec2<i32>(0), vec2<i32>(i32(side) - 1));
            let d = textureLoad(shelter_depth, t, 0);
            if (d < 1.0) {
                nearest = min(nearest, abs(W.cover_span.x - d * W.cover_span.y - p.z));
            }
        }
    }
    return 1.0 - smoothstep(0.2, 0.45, nearest);
}

// What the weather needs to know about the surface drawn at a pixel.
struct Ground {
    // The point, render space, and how far it is from the eye.
    p: vec3<f32>,
    dist: f32,
    // The point in the world, wrapped to the noise's period.
    wp: vec2<f32>,
    // Its normal: the landscape's smooth one on the land.
    n: vec3<f32>,
    // How much it faces up as drawn.
    face_z: f32,
    // How much of it is the drawn landscape, and of that how much is road.
    on_land: f32,
    road: f32,
    // How much it faces the sky.
    up: f32,
    // How near it is: far off the world's own fog takes over.
    reach: f32,
    // How much of it is open water facing up.
    on_water: f32,
    // How open it is to the sky: 0 under a roof.
    open: f32,
};

// The ground in the rain: soaked, puddled where it is flat, ringed where it is open water, and
// splashed near the viewer. (colour, how much of it is puddle)
fn wet_ground(
    c0: vec3<f32>,
    g: Ground,
    dir: vec3<f32>,
    t: f32,
    quality: i32,
    air: vec3<f32>,
) -> vec4<f32> {
    var c = c0;
    // Soaked: darker and deeper in colour, walls less than the ground; open water is already
    // wet.
    let wet = mix(0.45, 1.0, g.up) * g.reach * (1.0 - g.on_water) * g.open;
    let l = luminance(c);
    c = mix(vec3<f32>(l), c, 1.0 + 0.3 * wet);
    // Packed road soaks darker than grass, which holds its colour better.
    c = c * mix(1.0, mix(0.62, 0.54, g.road), wet);
    // Puddles where the ground is flat: mostly on the road, few in the grass, and only near
    // enough to be seen as puddles.
    let level_ground = smoothstep(0.955, 0.99, g.n.z) * smoothstep(0.8, 0.93, g.face_z);
    let m = wfbm(g.wp, 0.16, 1u) * 0.8 + wfbm(g.wp + vec2<f32>(3.1, 7.7), 0.7, 2u) * 0.2;
    let lip = mix(0.7, 0.44, g.road);
    let puddle = smoothstep(lip, lip + 0.07, m) * level_ground * (1.0 - smoothstep(15.0, 45.0, g.dist)) * (1.0 - g.on_water) * g.open;
    let steps = select(0, select(16, select(28, 48, quality >= 3), quality >= 2), quality >= 1);
    if (g.up > 0.01 && g.dist < 260.0 && g.open > 0.0) {
        // The drops' rings on a puddle and on open water bend only what the water mirrors, a
        // little: how much it mirrors, and so how dark it is, is the still water's.
        var bent = g.n;
        let ripple_share = max(puddle, g.on_water) * (1.0 - smoothstep(8.0, 22.0, g.dist));
        if (ripple_share > 0.0) {
            let rip = ripples(g.wp, t);
            bent = normalize(mix(g.n, normalize(vec3<f32>(-rip, 1.0)), ripple_share));
        }
        let r = reflect(dir, bent);
        let cosv = clamp(dot(-dir, g.n), 0.0, 1.0);
        let fres = 0.02 + 0.98 * pow(1.0 - cosv, 5.0);
        // The world is mirrored only in the near puddles; farther off, where a march at grazing
        // angles would scatter into specks, they hold the sky alone.
        var found = vec4<f32>(0.0);
        let near_mirror = 1.0 - smoothstep(14.0, 24.0, g.dist);
        if (puddle > 0.02 && near_mirror > 0.0) {
            found = reflect_world(g.p, r, g.dist, steps);
            found.a = found.a * near_mirror;
        }
        let refl = mirrored(found, r, W.fog.rgb);
        // Puddles: the ground under them darkened, the world mirrored over it. The mirror is
        // held down farther off and at grazing angles, where the overcast would otherwise turn a
        // puddle into a white sheet.
        let under = c * 0.42;
        let hold = mix(0.75, 0.4, smoothstep(8.0, 45.0, g.dist)) * mix(0.55, 1.0, smoothstep(0.03, 0.25, cosv));
        let puddled = mix(under, refl * 0.8, clamp(fres * 1.2 + 0.12, 0.0, 1.0) * hold);
        // A wet road shines; wet grass far less; roofs, leaves and the rest off the ground
        // (thatch, shingle, cards whose faces only look up) hardly at all.
        let gloss = mix(0.15, mix(0.35, 1.0, g.road), g.on_land);
        let sheen = c + refl * min(fres, 0.2) * 0.4 * gloss * g.up * g.reach * (1.0 - g.on_water) * g.open;
        // Rain on open water: the rings catch the sky, near enough to be seen as rings; farther
        // off the water is left as the world draws it.
        let rings = refl * clamp(fres * 0.9, 0.0, 0.5) * g.on_water * 0.5
            * (1.0 - smoothstep(15.0, 45.0, g.dist));
        c = mix(sheen, puddled, puddle) + rings;
    }
    // Splashes.
    if (g.up > 0.3 && g.dist < 16.0 && g.on_water < 0.5 && g.open > 0.5) {
        let footprint = g.dist * W.params.w;
        let s = splashes(g.wp, t, footprint) * g.up * (1.0 - smoothstep(7.0, 16.0, g.dist)) * g.open;
        c = c + air * s * 0.6;
    }
    return vec4<f32>(c, puddle);
}

// The ground in the snow: a cover lying on the tops of solid things where they face the sky (a
// roof, a rock, a platform), whole where they are level, thinning as they steepen and gone where
// they are steep, lit as the surface under it is lit and carrying a little of its relief, so a
// roof's courses show through. The land the game paints as snow takes a lighter cover the same
// way, evenly, with no drifts or patches; other land takes none. Never on open water, leaves,
// bodies or what moves, and never under a roof.
fn snow_ground(c: vec3<f32>, g: Ground, px: vec2<i32>) -> vec3<f32> {
    let snowy = smoothstep(0.2, 0.6, class_share(g.p.xy, 5u));
    let water = smoothstep(0.25, 0.6, class_share(g.p.xy, 6u));
    let lies = max(static_top(g.p) * (1.0 - g.on_land), g.on_land * snowy * (1.0 - water) * 0.6);
    let level = smoothstep(0.55, 0.85, g.n.z);
    let cover = lies * level * g.reach * g.open * 0.92;
    if (cover <= 0.0) {
        return c;
    }
    // The surface's relief: its brightness against its neighbours' a few pixels round.
    let l = luminance(c);
    let around = (luminance(picture(px + vec2<i32>(3, 0))) + luminance(picture(px - vec2<i32>(3, 0)))
        + luminance(picture(px + vec2<i32>(0, 3))) + luminance(picture(px - vec2<i32>(0, 3))) + l) / 5.0;
    let relief = clamp(l / max(around, 0.001), 0.75, 1.2);
    return mix(c, snow_colour(g.n) * mix(1.0, relief, 0.4), cover);
}

// --- the frame --------------------------------------------------------------------------------

@fragment
fn fs_weather(in: FullscreenOut) -> @location(0) vec4<f32> {
    let px = vec2<i32>(in.position.xy);
    let raw = textureLoad(colour, px, 0);
    if (!in_view(in.position.xy)) {
        return raw;
    }
    let t = W.eye.w;
    let quality = i32(W.params.x);
    // How much of what falls is snow, and how much falls: both ease over a few seconds, so the
    // rain and the snow, and the weather and a dry day, blend rather than switch.
    let snowfall = W.params.y;
    let amount = W.params.z;
    var c = decode(raw.rgb);
    let ndc = ndc_of(in.position.xy);
    let d = depth_at(px);
    let near = world_at(ndc, 0.2);
    let dir = normalize(near - W.eye.xyz);
    let sky = d >= 1.0;
    var dist = 1.0e5;
    // The light the air carries onto the splashes: the overcast sky's, from the fog and ambient.
    let air = mix(W.fog.rgb, W.ambient.rgb * W.ambient.w, 0.25);
    if (!sky) {
        let p = world_at(ndc, d);
        dist = length(p - W.eye.xyz);
        let face = normal_at(px, p);
        var g: Ground;
        g.p = p;
        g.dist = dist;
        g.wp = world_xy(p.xy);
        // On the drawn landscape its smooth normal, continuous across every cell and block edge,
        // so no face of it shines or whitens alone.
        var on_land = 0.0;
        if (field.origin_size.z >= 2.0 && field_class_at(p.xy) != 0u) {
            // On it, and facing up the way the ground does: the foot of a wall or a trunk, as
            // close to the ground as it is, keeps its own normal.
            let tol = 0.05 + dist * 0.0015;
            let above = abs(p.z - plane_height(p.xy));
            on_land = (1.0 - smoothstep(tol, tol * 3.0, above)) * smoothstep(0.35, 0.6, face.z);
        }
        g.on_land = on_land;
        g.n = normalize(mix(face, field_normal_at(p.xy), on_land));
        g.face_z = mix(face.z, g.n.z, on_land);
        g.road = road_at(p.xy, g.wp) * on_land;
        g.up = smoothstep(0.3, 0.85, g.n.z);
        // Near the viewer the ground is soaked; farther off the world's own fog takes over.
        g.reach = 1.0 - smoothstep(120.0, 260.0, dist);
        g.on_water = class_share(p.xy, 6u) * smoothstep(0.9, 0.99, g.n.z);
        // Under a roof or a porch the ground stays dry.
        g.open = 1.0 - shelter_share(p, 0.4);
        if (W.debug.x > 1.5) {
            return vec4<f32>(encode(vec3<f32>(1.0 - g.open, on_land, 0.0)), raw.a);
        }
        var ground = c;
        if (snowfall < 1.0 || W.debug.x > 0.5) {
            let wet = wet_ground(c, g, dir, t, quality, air);
            if (W.debug.x > 0.5) {
                return vec4<f32>(encode(vec3<f32>(on_land, g.road, wet.a)), raw.a);
            }
            ground = wet.rgb;
        }
        if (snowfall > 0.0) {
            ground = mix(ground, snow_ground(c, g, px), snowfall);
        }
        c = mix(c, ground, amount);
    }
    return vec4<f32>(encode(c), raw.a);
}

// --- the falling rain and snow -----------------------------------------------------------------
//
// Every drop and flake is a particle of a lattice fixed in the world. A lattice fills a box round
// the viewer: each particle has its own place in the world, and the box chooses only which repeat
// of the lattice is drawn, so a particle holds still in the world, but for its fall and the wind,
// however the viewer walks or turns or swings the camera round. Only far off, at the box's side,
// does one leave and the next repeat come in at the other side, and it fades out before it does.
// Three lattices for each, each wider and sparser than the last, fill the near air, the middle
// distance and the far.

// Seconds a drop or a flake is blurred over, as a camera's shutter would.
const SHUTTER: f32 = 0.033;
// The widest a streak is drawn, radians either side of its line, and the longest, radians.
const STREAK_WIDEST: f32 = 0.0012;
const STREAK_LONGEST: f32 = 0.1;
// The widest a flake is drawn, radians from its middle.
const FLAKE_WIDEST: f32 = 0.006;

// Rain's lattices, nearest first: x the half-width of the box round the eye, metres; y its
// half-height; z the drop's radius as drawn, metres; w how strongly a streak shows.
fn rain_tier(k: u32) -> vec4<f32> {
    if (k == 0u) {
        return vec4<f32>(6.0, 6.0, 0.002, 0.6);
    }
    if (k == 1u) {
        return vec4<f32>(18.0, 12.0, 0.0025, 0.5);
    }
    return vec4<f32>(48.0, 24.0, 0.003, 0.4);
}

// Snow's: x, y as rain's; z the least radius of a flake, metres, the largest twice it; w how
// strongly a flake shows.
fn snow_tier(k: u32) -> vec4<f32> {
    if (k == 0u) {
        return vec4<f32>(5.0, 5.0, 0.01, 1.0);
    }
    if (k == 1u) {
        return vec4<f32>(15.0, 10.0, 0.012, 0.95);
    }
    return vec4<f32>(40.0, 20.0, 0.014, 0.85);
}

// Three numbers from 0 to 1 for particle `id` of the lattice `salt` names.
fn hash3(id: u32, salt: u32) -> vec3<f32> {
    let a = pcg(id ^ (salt * 2654435769u));
    let b = pcg(a + 1759714724u);
    let c = pcg(b + 3043654221u);
    return vec3<f32>(f32(a >> 8u), f32(b >> 8u), f32(c >> 8u)) / 16777216.0;
}

// Where particle `id` of the lattice `salt` names stands, in the world wrapped to the lattices'
// period: the repeat of its place that falls in the box `size` metres across centred on `eye`,
// the eye in the same terms. `fall` is how far the lattice has fallen and `drift` how far the
// wind has carried it. The eye chooses only the repeat, never the place.
fn lattice_place(
    id: u32,
    salt: u32,
    size: vec3<f32>,
    eye: vec3<f32>,
    fall: f32,
    drift: vec2<f32>,
) -> vec3<f32> {
    let own = hash3(id, salt) * size + vec3<f32>(drift, -fall);
    let corner = eye - size * 0.5;
    let rel = own - corner;
    return corner + rel - size * floor(rel / size);
}

// How a drop's streak (`flake` false) or a flake `radius` metres across at `dist` metres from the
// eye is drawn, with `pixel` radians to a pixel: x its radius on screen, pixels, no less than a
// little over half a pixel and no more than the widest a streak or a flake is drawn; y how much of
// its light it keeps: less as it is drawn wider than it is (by the ratio for a streak's width, its
// square for a flake's area), so one too small to see is faint rather than bright; and none within
// half a metre of the eye, fading in by a metre and a half, so nothing passes the camera as a blur.
fn drawn_size(radius: f32, dist: f32, pixel: f32, flake: bool) -> vec2<f32> {
    let widest = select(STREAK_WIDEST, FLAKE_WIDEST, flake);
    let seen = radius / (max(dist, 0.01) * pixel);
    let drawn = clamp(seen, 0.6, max(widest / pixel, 0.6));
    let keep = pow(min(seen / drawn, 1.0), select(1.0, 2.0, flake));
    return vec2<f32>(drawn, keep * smoothstep(0.5, 1.5, dist));
}

// The tail of a streak from `head` toward `tail` on screen, pixels, with `pixel` radians to a
// pixel: where it is, or nearer the head where the streak would be longer than the longest a
// streak is drawn, as it would be just in front of the eye.
fn streak_tail(head: vec2<f32>, tail: vec2<f32>, pixel: f32) -> vec2<f32> {
    let longest = STREAK_LONGEST / pixel;
    let run = length(tail - head);
    if (run > longest) {
        return head + (tail - head) * (longest / run);
    }
    return tail;
}

// The light a drop (`flake` false) or a flake carries, in the picture's terms: the overcast sky's,
// from the fog and the ambient, as bright as the sky the picture draws, whether the picture is the
// ordinary one or the re-shaded world's linear light, whose sky is the same. A flake is white.
fn fall_light(flake: bool) -> vec3<f32> {
    let day = W.sun_col.w;
    let air = mix(W.fog.rgb, W.ambient.rgb * W.ambient.w, 0.25);
    if (flake) {
        return vec3<f32>(min(luminance(air) * 1.5 + 0.08 * day, 1.0));
    }
    return max(air * 1.15, vec3<f32>(0.02)) + W.sun_col.rgb * 0.05 * day;
}

struct Falling {
    @builtin(position) position: vec4<f32>,
    // The particle's two ends on screen, pixels: where it is, and where it was a shutter's time
    // ago.
    @location(0) @interpolate(flat) head: vec2<f32>,
    @location(1) @interpolate(flat) tail: vec2<f32>,
    // x its radius on screen, pixels; y how strongly it shows; z how far it is from the eye,
    // metres; w 1 for a flake.
    @location(2) @interpolate(flat) shape: vec4<f32>,
    // Its light, in the picture's terms; w how much of what is behind it it hides.
    @location(3) @interpolate(flat) light: vec4<f32>,
};

// One corner (`v`, 0 to 3) of the quad particle `i` is drawn on. The instances are rain's three
// lattices and then snow's; a particle that does not show this frame (its form not falling, under
// a roof, behind the eye, faded out) is drawn as nothing.
@vertex
fn vs_fall(@builtin(vertex_index) v: u32, @builtin(instance_index) i: u32) -> Falling {
    var out: Falling;
    out.position = vec4<f32>(0.0, 0.0, -2.0, 1.0);
    out.head = vec2<f32>(0.0);
    out.tail = vec2<f32>(0.0);
    out.shape = vec4<f32>(0.0);
    out.light = vec4<f32>(0.0);
    let snowfall = W.params.y;
    let amount = W.params.z;
    let at = f32(i);
    let flake = at >= W.rain_counts.z;
    var counts = W.rain_counts;
    var strength = amount * (1.0 - snowfall);
    if (flake) {
        counts = W.snow_counts;
        strength = amount * snowfall;
    }
    if (at >= counts.z || strength <= 0.0) {
        return out;
    }
    let k = select(select(2u, 1u, at < counts.y), 0u, at < counts.x);
    var tier = rain_tier(k);
    if (flake) {
        tier = snow_tier(k);
    }
    let size = vec3<f32>(2.0 * tier.x, 2.0 * tier.x, 2.0 * tier.y);
    let eye = vec3<f32>(W.eye.xy + W.lattice.xy, W.eye.z);
    let salt = select(81u, 167u, flake) + k * 4099u;
    let place = lattice_place(i, salt, size, eye, select(W.fall.x, W.fall.y, flake), W.fall.zw);
    // How far out in its box it stands, 0 at the eye and 1 at a side: it fades before it leaves.
    let o = abs(place - eye) / (size * 0.5);
    let inside = 1.0 - smoothstep(0.7, 0.97, max(max(o.x, o.y), o.z));
    var p = vec3<f32>(place.xy - W.lattice.xy, place.z);
    var vel = vec3<f32>(W.motion.zw, -W.motion.x);
    let r = hash3(i, salt + 91u);
    if (flake) {
        // A flake sways as it falls, on a slow round of its own about where the fall takes it.
        let t = W.eye.w;
        let w1 = turns_per_period(0.9 + r.x * 0.9);
        let w2 = turns_per_period(0.7 + r.y * 0.8);
        let sway = 0.06 + r.z * 0.1;
        let a1 = t * w1 + r.y * 6.2831853;
        let a2 = t * w2 + r.x * 6.2831853;
        p = p + vec3<f32>(sin(a1), cos(a2), 0.0) * sway;
        vel = vec3<f32>(W.motion.zw + vec2<f32>(cos(a1) * w1, -sin(a2) * w2) * sway, -W.motion.y);
    }
    if (sheltered(p, 0.1) > 0.5) {
        return out;
    }
    let dist = length(p - W.eye.xyz);
    let pixel = W.params.w;
    let ch = W.clip_from_render * vec4<f32>(p, 1.0);
    // A flake is blurred over a quarter of the time a drop is: it falls slowly, and is drawn as
    // a flake rather than a streak.
    let shutter = select(SHUTTER, SHUTTER * 0.25, flake);
    var ct = W.clip_from_render * vec4<f32>(p - vel * shutter, 1.0);
    if (ch.w < 0.1) {
        return out;
    }
    if (ct.w < 0.1) {
        ct = ch;
    }
    let head = px_of(ch.xy / ch.w);
    let tail = streak_tail(head, px_of(ct.xy / ct.w), pixel);
    let radius = select(tier.z, tier.z * (0.6 + 1.2 * r.x), flake);
    let drawn = drawn_size(radius, dist, pixel, flake);
    // Far off, the world's own fog takes most of it.
    let fog = 1.0 - 0.75 * smoothstep(W.fogp.x, W.fogp.y, dist);
    let alpha = drawn.y * tier.w * strength * inside * fog;
    if (alpha < 0.002) {
        return out;
    }
    // A quad over the particle's line, a pixel past its edge all round for the soft edge.
    let along = tail - head;
    let len = length(along);
    let dir = select(vec2<f32>(0.0, 1.0), along / max(len, 0.0001), len > 0.001);
    let across = vec2<f32>(-dir.y, dir.x);
    let grow = drawn.x + 1.0;
    let side = select(-1.0, 1.0, (v & 1u) == 1u);
    let end = select(head - dir * grow, tail + dir * grow, (v & 2u) == 2u);
    out.position = vec4<f32>(ndc_of(end + across * grow * side), 0.5, 1.0);
    out.head = head;
    out.tail = tail;
    out.shape = vec4<f32>(drawn.x, alpha, dist, select(0.0, 1.0, flake));
    out.light = vec4<f32>(encode(fall_light(flake)), select(0.2, 1.0, flake));
    return out;
}

@fragment
fn fs_fall(in: Falling) -> @location(0) vec4<f32> {
    let px = in.position.xy;
    let ab = in.tail - in.head;
    let l2 = dot(ab, ab);
    var h = 0.0;
    if (l2 > 0.000001) {
        h = clamp(dot(px - in.head, ab) / l2, 0.0, 1.0);
    }
    let d = length(px - (in.head + ab * h));
    let radius = in.shape.x;
    var cover = 0.0;
    if (in.shape.w > 0.5) {
        // A flake: soft and round, its light spread along its blur.
        cover = (1.0 - smoothstep(radius * 0.55, radius + 0.5, d))
            / (1.0 + sqrt(l2) / (2.0 * radius + 1.0));
    } else {
        // A streak: brightest at its head, fading toward its tail.
        cover = (1.0 - smoothstep(radius - 0.5, radius + 0.5, d)) * (1.0 - 0.6 * h);
    }
    // Hidden by whatever is drawn nearer than it, softly over a few tens of centimetres, so its
    // edge never crawls along a surface.
    let sd = depth_at(vec2<i32>(px));
    var scene = 1.0e5;
    if (sd < 1.0) {
        scene = length(world_at(ndc_of(px), sd) - W.eye.xyz);
    }
    let a = cover * in.shape.y * smoothstep(0.0, 0.3, scene - in.shape.z);
    if (a < 0.002) {
        discard;
    }
    return vec4<f32>(in.light.rgb * a, a * in.light.w);
}
