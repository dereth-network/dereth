// The light of the rooms' own lights on every pixel of a room's surface, worked out per pixel: each
// static light the rooms' vertex colors are burned from, with a smooth falloff over its reach,
// brightest close to it, and the turn of the surface toward it, softly wrapped. With rays, a light
// whose way to the pixel is shut by a wall, a floor or a pillar gives it only the little light the
// room scatters, so no light reaches through a wall. Alpha is how brightly the lights the surface
// sees would shine off it, were it wet, on a room's surface; -1 elsewhere.

struct Params {
    clip_to_render: mat4x4<f32>,
    view_to_render: mat4x4<f32>,
    viewport: vec4<f32>,
    // xyz the eye; w the number of lights.
    eye: vec4<f32>,
    // x 1 to trace, y how far short of a light a ray stops (meters), z the time, w unused.
    tune: vec4<f32>,
};

struct CellLight {
    // xyz position; w the range, meters: the light's falloff and nearly a third again.
    pos: vec4<f32>,
    // rgb the color, 0..1; w unused.
    col: vec4<f32>,
};

@group(0) @binding(0) var depth: texture_depth_2d;
@group(0) @binding(1) var normals: texture_2d<f32>;
@group(0) @binding(2) var out_light: texture_storage_2d<rgba16float, write>;
@group(0) @binding(3) var<uniform> params: Params;
@group(0) @binding(4) var<storage, read> lights: array<CellLight>;
@group(0) @binding(10) var picture: texture_2d<f32>;
@group(0) @binding(11) var out_slime: texture_storage_2d<rgba16float, write>;

fn render_at(px: vec2<f32>, d: f32) -> vec3<f32> {
    let v = params.viewport;
    let uv = (px - v.xy) / v.zw;
    let ndc = vec4<f32>(uv.x * 2.0 - 1.0, 1.0 - uv.y * 2.0, d, 1.0);
    let w = params.clip_to_render * ndc;
    return w.xyz / w.w;
}

// How much of a light's reach `x` (the distance over the range) leaves of its light: smoothly
// down to nothing at the range.
fn room_window(x: f32) -> f32 {
    let w = clamp(1.0 - x * x, 0.0, 1.0);
    return w * w;
}

// One light's direct light at `p`, a surface facing `n`, reaching it along a clear line: falling
// off smoothly to nothing at the light's range, brightest close to it, with the surface's turn
// toward it, softly wrapped.
fn room_direct(p: vec3<f32>, n: vec3<f32>, light_pos: vec3<f32>, range: f32, color: vec3<f32>) -> vec3<f32> {
    let d = light_pos - p;
    let dist = length(d);
    if (dist >= max(range, 0.0)) {
        return vec3<f32>(0.0);
    }
    let x = dist / range;
    let near = 1.0 / (1.0 + 4.0 * x * x);
    let turn = clamp((dot(n, d / max(dist, 1e-4)) + 0.3) / 1.3, 0.0, 1.0);
    return color * (2.4 * room_window(x) * near * turn);
}

// The light the room scatters from one light onto `p`, reaching round corners and onto faces
// turned away: a little of it, falling off with the same reach.
fn room_scattered(p: vec3<f32>, light_pos: vec3<f32>, range: f32, color: vec3<f32>) -> vec3<f32> {
    let dist = length(light_pos - p);
    if (dist >= max(range, 0.0)) {
        return vec3<f32>(0.0);
    }
    return color * (0.3 * room_window(dist / range));
}

// How tight the highlight off a wet surface is.
const SHINE_TIGHTNESS: f32 = 90.0;

// Whether a room's surface pixel carries the burned light it was drawn with: its class's code.
fn burned_surface(kind: f32) -> bool {
    return kind > 1.54 && kind < 1.96;
}

// How far past 2 the picture's alpha says a surface is slimy (see the material variant).
const SLIME_FROM: f32 = 2.01;

// A film's meniscus: how steep its edge bends the light, and how much its gentle swells do.
const SLIME_EDGE: f32 = 0.5;
const SLIME_SWELL: f32 = 0.35;
// How fast the film creeps down a wall, in meters a second.
const SLIME_CREEP: f32 = 0.012;

fn slime_hash(c: vec3<f32>) -> f32 {
    var v = bitcast<vec3<u32>>(vec3<i32>(c));
    v = v * 1664525u + 1013904223u;
    v.x = v.x + v.y * v.z;
    v.y = v.y + v.z * v.x;
    v.z = v.z + v.x * v.y;
    v = v ^ (v >> vec3<u32>(16u));
    v.x = v.x + v.y * v.z;
    return f32(v.x >> 8u) / 8388608.0 - 1.0;
}

// Smooth noise at `p`, -1..1, in x, and its change along each axis in yzw.
fn slime_noise(p: vec3<f32>) -> vec4<f32> {
    let i = floor(p);
    let f = p - i;
    let u = f * f * f * (f * (f * 6.0 - 15.0) + 10.0);
    let du = 30.0 * f * f * (f * (f - 2.0) + 1.0);
    let a = slime_hash(i);
    let b = slime_hash(i + vec3<f32>(1.0, 0.0, 0.0));
    let c = slime_hash(i + vec3<f32>(0.0, 1.0, 0.0));
    let d = slime_hash(i + vec3<f32>(1.0, 1.0, 0.0));
    let e = slime_hash(i + vec3<f32>(0.0, 0.0, 1.0));
    let g = slime_hash(i + vec3<f32>(1.0, 0.0, 1.0));
    let h = slime_hash(i + vec3<f32>(0.0, 1.0, 1.0));
    let k = slime_hash(i + vec3<f32>(1.0, 1.0, 1.0));
    let k1 = b - a;
    let k2 = c - a;
    let k3 = e - a;
    let k4 = a - b - c + d;
    let k5 = a - c - e + h;
    let k6 = a - b - e + g;
    let k7 = -a + b + c - d + e - g - h + k;
    return vec4<f32>(
        a + k1 * u.x + k2 * u.y + k3 * u.z + k4 * u.x * u.y + k5 * u.y * u.z + k6 * u.z * u.x + k7 * u.x * u.y * u.z,
        du * vec3<f32>(
            k1 + k4 * u.y + k6 * u.z + k7 * u.y * u.z,
            k2 + k4 * u.x + k5 * u.z + k7 * u.x * u.z,
            k3 + k5 * u.y + k6 * u.x + k7 * u.x * u.y,
        ),
    );
}

// Noise at `p` turned level by the angle whose cosine and sine are `c` and `s`, its change turned
// back: the turn hides the noise's square grid.
fn slime_turned(p: vec3<f32>, c: f32, s: f32) -> vec4<f32> {
    let n = slime_noise(vec3<f32>(c * p.x - s * p.y, s * p.x + c * p.y, p.z));
    return vec4<f32>(n.x, c * n.y + s * n.z, c * n.z - s * n.y, n.w);
}

// The film's swell at `p`, and its change per meter: long down a wall, round on a floor.
fn slime_swell(p: vec3<f32>, time: f32, dist: f32) -> vec4<f32> {
    let run = vec3<f32>(1.0, 1.0, 0.28);
    let at = (p + vec3<f32>(0.0, 0.0, time * SLIME_CREEP)) * run;
    let fine = 1.0 - smoothstep(12.0, 30.0, dist);
    let a = slime_turned(at * 1.3, 0.8, 0.6);
    let b = slime_turned(at * 3.7 + vec3<f32>(17.0, 3.0, 9.0), 0.28, 0.96);
    let c = slime_turned(at * 11.0 + vec3<f32>(5.0, 29.0, 11.0), -0.6, 0.8);
    return vec4<f32>(
        a.x * 0.55 + b.x * 0.3 + c.x * 0.15 * fine,
        (a.yzw * 1.3 * 0.55 + b.yzw * 3.7 * 0.3 + c.yzw * 11.0 * 0.15 * fine) * run,
    );
}

// How much one light at `to_light` (not unit) shines off a film facing `n` seen along `v`.
fn slime_lobe(n: vec3<f32>, v: vec3<f32>, to_light: vec3<f32>, tight: f32, tint: vec3<f32>) -> vec3<f32> {
    let l = normalize(to_light);
    let h = normalize(l + v);
    let n_h = max(dot(n, h), 0.0);
    let fresnel = 0.3 + 0.7 * pow(1.0 - max(dot(v, h), 0.0), 5.0);
    let facing = smoothstep(-0.1, 0.25, dot(n, l));
    // The glint is clear; the sheen round it comes through the film, in the colour of what is under it.
    return (vec3<f32>(0.75 * pow(n_h, tight)) + tint * 0.035 * pow(n_h, 30.0)) * fresnel * facing;
}

// The film on a surface at `world`: rgb what it shines of its lights, a how thick it lies (0..1).
fn slime_film(world: vec3<f32>, bumped: vec3<f32>, flat_n: vec3<f32>, amount: f32, seen: vec3<f32>, under_it: vec3<f32>) -> vec4<f32> {
    let dist = length(params.eye.xyz - world);
    let v = normalize(params.eye.xyz - world);
    let swell = slime_swell(world, params.tune.z, dist);
    // It lies in pools and runs with a sharp edge, as much of the surface as the material says.
    let edge = mix(0.45, 0.0, clamp(amount, 0.0, 1.0));
    // A near black pixel says nothing of its colour: it would tint a glint pure green.
    let tint = (under_it + vec3<f32>(0.012)) / (max(under_it.r, max(under_it.g, under_it.b)) + 0.012);
    let thick = smoothstep(edge - 0.07, edge + 0.07, swell.x);
    if (thick <= 0.001) {
        return vec4<f32>(0.0);
    }
    // At a film's edge the height changes fastest: the meniscus that catches the light.
    let x = clamp((swell.x - edge + 0.07) / 0.14, 0.0, 1.0);
    let steep = 6.0 * x * (1.0 - x) / 0.14;
    var slope = swell.yzw * (steep * SLIME_EDGE * 0.1 + SLIME_SWELL);
    // The film fills the surface's relief and shows little of it.
    let under = normalize(mix(bumped, flat_n, 0.85));
    slope = slope - under * dot(slope, under);
    let n = normalize(under - slope);
    // Far off a tight glint would sparkle from pixel to pixel.
    let tight = mix(420.0, 90.0, smoothstep(6.0, 28.0, dist));
    var shine = vec3<f32>(0.0);
    let count = i32(params.eye.w);
    for (var k: i32 = 0; k < count; k = k + 1) {
        let l = lights[k];
        let to_light = l.pos.xyz - world;
        let r = length(to_light);
        if (r >= l.pos.w) {
            continue;
        }
        let x_r = r / l.pos.w;
        let pale = mix(l.col.rgb, vec3<f32>(dot(l.col.rgb, vec3<f32>(0.2126, 0.7152, 0.0722))), 0.45);
        shine = shine + pale * room_window(x_r) * 2.6 * slime_lobe(n, v, to_light, tight, tint);
    }
    // The light that follows the player: a film glints as the eye moves wherever there is light to see by.
    let seen_by = clamp(dot(seen, vec3<f32>(0.2126, 0.7152, 0.0722)) * 0.8 + 0.05, 0.0, 0.5);
    let near_eye = 1.0 / (1.0 + dist * dist * 0.04);
    let lamp = params.eye.xyz + vec3<f32>(0.0, 0.0, 0.5) - world;
    shine = shine + vec3<f32>(0.85, 0.95, 0.8) * seen_by * near_eye * slime_lobe(n, v, lamp, tight, tint);
    // A low angle mirrors what light the room has.
    let glance = pow(1.0 - max(dot(n, v), 0.0), 4.0);
    shine = shine + tint * dot(seen, vec3<f32>(0.2126, 0.7152, 0.0722)) * glance * 0.12;
    // Overhead it clings thin, and a lamp below would light it as a white bar.
    let overhead = mix(0.2, 1.0, smoothstep(-0.6, -0.1, flat_n.z));
    return vec4<f32>(shine * thick * overhead, thick);
}

// The surface's flat facing from the depth round the pixel, whichever side is nearer.
fn flat_normal(px: vec2<i32>, p: vec3<f32>, fallback: vec3<f32>) -> vec3<f32> {
    let size = vec2<i32>(textureDimensions(depth));
    let side = vec2<i32>(select(1, -1, px.x + 1 >= size.x), select(1, -1, px.y + 1 >= size.y));
    let qx = px + vec2<i32>(side.x, 0);
    let qy = px + vec2<i32>(0, side.y);
    let along_x = render_at(vec2<f32>(qx) + 0.5, textureLoad(depth, qx, 0)) - p;
    let along_y = render_at(vec2<f32>(qy) + 0.5, textureLoad(depth, qy, 0)) - p;
    let n = cross(along_x, along_y);
    let l = length(n);
    if (l < 1e-8) {
        return fallback;
    }
    let u = n / l;
    return select(-u, u, dot(u, fallback) > 0.0);
}

{{TRACE}}

@compute @workgroup_size(8, 8)
fn main(@builtin(global_invocation_id) id: vec3<u32>) {
    let size = textureDimensions(out_light);
    if (id.x >= size.x || id.y >= size.y) {
        return;
    }
    let px = vec2<i32>(id.xy);
    let d = textureLoad(depth, px, 0);
    let nw = textureLoad(normals, px, 0);
    if (d >= 1.0 || dot(nw.xyz, nw.xyz) < 0.25 || !burned_surface(nw.w)) {
        textureStore(out_light, px, vec4<f32>(0.0, 0.0, 0.0, -1.0));
        textureStore(out_slime, px, vec4<f32>(0.0));
        return;
    }
    let world = render_at(vec2<f32>(id.xy) + 0.5, d);
    let n = normalize((params.view_to_render * vec4<f32>(nw.xyz, 0.0)).xyz);
    let dist_eye = length(params.eye.xyz - world);
    let origin = world + n * (0.02 + dist_eye * 0.001);
    let to_eye = normalize(params.eye.xyz - world);
    var sum = vec3<f32>(0.0);
    // How brightly the lights this surface sees would shine off it, were it wet.
    var shine = 0.0;
    let count = i32(params.eye.w);
    for (var k: i32 = 0; k < count; k = k + 1) {
        let l = lights[k];
        let direct = room_direct(world, n, l.pos.xyz, l.pos.w, l.col.rgb);
        sum = sum + room_scattered(world, l.pos.xyz, l.pos.w, l.col.rgb);
        if (max(direct.r, max(direct.g, direct.b)) <= 0.0) {
            continue;
        }
        // Before any wall shuts the light out: a highlight cut by a shadow's hard edge shows as a
        // bright shape. What the burned light says of the place keeps it from leaking through.
        let half_way = normalize(normalize(l.pos.xyz - world) + to_eye);
        let brightness = dot(direct, vec3<f32>(0.2126, 0.7152, 0.0722));
        shine = shine + brightness * pow(max(dot(n, half_way), 0.0), SHINE_TIGHTNESS);
        if (params.tune.x > 0.5) {
            let to = l.pos.xyz - origin;
            let r = length(to);
            let stop = r - params.tune.y;
            if (stop > 0.02 && blocked(origin, to / r, stop)) {
                continue;
            }
        }
        sum = sum + direct;
    }
    textureStore(out_light, px, vec4<f32>(sum, shine));
    let drawn = textureLoad(picture, px, 0);
    let slime = drawn.a - SLIME_FROM;
    var film = vec4<f32>(0.0);
    if (slime > 0.0) {
        film = slime_film(world, n, flat_normal(px, world, n), slime, sum, drawn.rgb);
    }
    textureStore(out_slime, px, film);
}
