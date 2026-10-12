// The light of the rooms' own lights on every pixel of a room's surface, worked out per pixel: each
// static light the rooms' vertex colors are burned from, with a smooth falloff over its reach,
// brightest close to it, and the turn of the surface toward it, softly wrapped. With rays, a light
// whose way to the pixel is shut by a wall, a floor or a pillar gives it only the little light the
// room scatters, so no light reaches through a wall. Alpha is 1 on a room's surface, -1 elsewhere.

struct Params {
    clip_to_render: mat4x4<f32>,
    view_to_render: mat4x4<f32>,
    viewport: vec4<f32>,
    // xyz the eye; w the number of lights.
    eye: vec4<f32>,
    // x 1 to trace, y how far short of a light a ray stops (meters), zw unused.
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

// Whether a room's surface pixel carries the burned light it was drawn with: its class's code.
fn burned_surface(kind: f32) -> bool {
    return kind > 1.54 && kind < 1.96;
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
        return;
    }
    let world = render_at(vec2<f32>(id.xy) + 0.5, d);
    let n = normalize((params.view_to_render * vec4<f32>(nw.xyz, 0.0)).xyz);
    let dist_eye = length(params.eye.xyz - world);
    let origin = world + n * (0.02 + dist_eye * 0.001);
    var sum = vec3<f32>(0.0);
    let count = i32(params.eye.w);
    for (var k: i32 = 0; k < count; k = k + 1) {
        let l = lights[k];
        let direct = room_direct(world, n, l.pos.xyz, l.pos.w, l.col.rgb);
        sum = sum + room_scattered(world, l.pos.xyz, l.pos.w, l.col.rgb);
        if (max(direct.r, max(direct.g, direct.b)) <= 0.0) {
            continue;
        }
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
    textureStore(out_light, px, vec4<f32>(sum, 1.0));
}
