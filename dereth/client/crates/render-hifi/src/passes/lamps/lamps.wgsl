enable wgpu_ray_query;

// The light of the outdoor lamps on every lit pixel: for each lamp in reach, a windowed
// inverse-square falloff, a slight wrap, and one ray toward a jittered point on the lamp's small
// sphere, so a wall, a roof or a tree between the lamp and the surface keeps its light off it.
// The rays stop a little short of the lamp, so the lamp's own glass and housing do not shade it.
// Alpha carries how much of the pixel is the lamp itself (its glass), for the glow.

struct Params {
    clip_to_render: mat4x4<f32>,
    view_to_render: mat4x4<f32>,
    viewport: vec4<f32>,
    // xyz the eye; w the number of lamps.
    eye: vec4<f32>,
    // x the light gain, y the frame number, z frames already accumulated, w the glow gain.
    tune: vec4<f32>,
    // x the soft core radius (metres), y 1 to trace, z 1 to mark the lamps, w unused.
    shape: vec4<f32>,
};

struct Lamp {
    // xyz position; w the range, metres.
    pos: vec4<f32>,
    // rgb colour times brightness (night and flicker folded in); w the light's own radius.
    col: vec4<f32>,
    // x how far short of the lamp a ray stops, y the radius of the lamp's glowing part, z the
    // source (0 authored, 1 flame, 2 glow), w unused.
    misc: vec4<f32>,
};

@group(0) @binding(0) var depth: texture_depth_2d;
@group(0) @binding(1) var normals: texture_2d<f32>;
@group(0) @binding(2) var scene: acceleration_structure;
@group(0) @binding(3) var out_light: texture_storage_2d<rgba16float, write>;
@group(0) @binding(4) var<uniform> params: Params;
@group(0) @binding(5) var atlas: texture_2d_array<f32>;
@group(0) @binding(6) var atlas_sampler: sampler;
@group(0) @binding(7) var<storage, read> cuts: array<vec4<u32>>;
@group(0) @binding(8) var<storage, read> uvs: array<vec2<f32>>;
@group(0) @binding(9) var<storage, read> lamps: array<Lamp>;
@group(0) @binding(10) var history: texture_2d<f32>;
// How open each lamp stands, 0 shut in (a lamp inside a building) to 1 in the open.
@group(0) @binding(11) var<storage, read_write> open: array<f32>;

fn render_at(px: vec2<f32>, d: f32) -> vec3<f32> {
    let v = params.viewport;
    let uv = (px - v.xy) / v.zw;
    let ndc = vec4<f32>(uv.x * 2.0 - 1.0, 1.0 - uv.y * 2.0, d, 1.0);
    let w = params.clip_to_render * ndc;
    return w.xyz / w.w;
}

fn pcg(v: u32) -> u32 {
    let s = v * 747796405u + 2891336453u;
    let w = ((s >> ((s >> 28u) + 4u)) ^ s) * 277803737u;
    return (w >> 22u) ^ w;
}

fn rand(seed: ptr<function, u32>) -> f32 {
    *seed = pcg(*seed);
    return f32(*seed) / 4294967296.0;
}

fn solid(c: RayIntersection) -> bool {
    let k = c.instance_custom_data + c.geometry_index;
    let cut = cuts[k];
    let base = cut.x + c.primitive_index * 3u;
    let b = c.barycentrics;
    let uv = uvs[base] * (1.0 - b.x - b.y) + uvs[base + 1u] * b.x + uvs[base + 2u] * b.y;
    let a = textureSampleLevel(atlas, atlas_sampler, uv, i32(cut.y), 0.0).r;
    return a * 255.0 >= max(f32(cut.z), 64.0);
}

// Whether anything solid lies along the ray before `t_max`.
fn blocked(origin: vec3<f32>, dir: vec3<f32>, t_max: f32) -> bool {
    var rq: ray_query;
    rayQueryInitialize(&rq, scene, RayDesc(4u, 0xFFu, 0.0, t_max, origin, dir));
    while (rayQueryProceed(&rq)) {
        let c = rayQueryGetCandidateIntersection(&rq);
        if (c.kind == RAY_QUERY_INTERSECTION_TRIANGLE && solid(c)) {
            rayQueryConfirmIntersection(&rq);
        }
    }
    let hit = rayQueryGetCommittedIntersection(&rq);
    return hit.kind != RAY_QUERY_INTERSECTION_NONE;
}

@compute @workgroup_size(8, 8)
fn main(@builtin(global_invocation_id) id: vec3<u32>) {
    let size = textureDimensions(out_light);
    if (id.x >= size.x || id.y >= size.y) {
        return;
    }
    let px = vec2<i32>(id.xy);
    let d = textureLoad(depth, px, 0);
    let nw = textureLoad(normals, px, 0);
    // Lit objects and the landscape only: an interior's own cells, the sky and effects keep
    // what they have.
    let kind = nw.w;
    let lit_kind = (kind > 0.5 && kind < 1.5) || (kind > 2.5 && kind < 3.5);
    if (d >= 1.0 || dot(nw.xyz, nw.xyz) < 0.25 || !lit_kind) {
        textureStore(out_light, px, vec4<f32>(0.0, 0.0, 0.0, -1.0));
        return;
    }
    let p = vec2<f32>(id.xy) + 0.5;
    let world = render_at(p, d);
    let dist = length(params.eye.xyz - world);
    let n = normalize((params.view_to_render * vec4<f32>(nw.xyz, 0.0)).xyz);
    let bias = 0.03 + dist * 0.0015;
    let origin = world + n * bias;
    var seed = pcg(id.x + pcg(id.y + pcg(u32(params.tune.y))));
    let core = params.shape.x;
    let trace_on = params.shape.y > 0.5;

    var sum = vec3<f32>(0.0);
    var glow = 0.0;
    var mark = vec3<f32>(0.0);
    let count = i32(params.eye.w);
    for (var k: i32 = 0; k < count; k = k + 1) {
        let l = lamps[k];
        let lamp_open = open[k];
        if (lamp_open <= 0.0) {
            continue;
        }
        let to = l.pos.xyz - world;
        let r = length(to);
        let range = l.pos.w;
        if (r >= range) {
            continue;
        }
        // The lamp's own glowing part.
        let gr = l.misc.y;
        if (r < gr) {
            let g = 1.0 - r / gr;
            glow = max(glow, g * g * (3.0 - 2.0 * g));
        }
        if (params.shape.z > 0.5 && r < 0.25) {
            mark = vec3<f32>(select(select(0.0, 1.0, l.misc.z < 0.5), 0.0, l.misc.z > 1.5),
                select(0.0, 1.0, l.misc.z > 0.5 && l.misc.z < 1.5),
                select(0.0, 1.0, l.misc.z > 1.5));
        }
        let dir = to / max(r, 1e-4);
        let ndl = clamp((dot(n, dir) + 0.1) / 1.1, 0.0, 1.0);
        if (ndl <= 0.0) {
            continue;
        }
        let x = r / range;
        let window = clamp(1.0 - x * x * x * x, 0.0, 1.0);
        // Inverse square, softened inside the lamp's core so the post's foot is not blown out.
        let atten = window * window / (r * r + core * core);
        let c = l.col.rgb * atten * ndl * lamp_open;
        if (max(c.r, max(c.g, c.b)) < 2e-4) {
            continue;
        }
        var vis = 1.0;
        if (trace_on) {
            // A jittered point on the light's small sphere: soft edges that harden near the wall.
            let u = rand(&seed) * 2.0 - 1.0;
            let a = rand(&seed) * 6.2831853;
            let s = sqrt(max(1.0 - u * u, 0.0));
            let jitter = vec3<f32>(cos(a) * s, sin(a) * s, u) * l.col.w;
            let target_point = l.pos.xyz + jitter;
            let tv = target_point - origin;
            let tl = length(tv);
            let stop = max(tl - l.misc.x, 0.0);
            if (stop > 0.02 && blocked(origin, tv / tl, stop)) {
                vis = 0.0;
            }
        }
        sum = sum + c * vis;
    }
    var out = vec4<f32>(sum * params.tune.x, glow);
    if (params.shape.z > 0.5 && dot(mark, mark) > 0.0) {
        out = vec4<f32>(mark * 40.0, 0.0);
    }
    let n_acc = params.tune.z;
    if (n_acc > 0.5) {
        let h = textureLoad(history, px, 0);
        if (h.w >= 0.0) {
            let w = 1.0 / (n_acc + 1.0);
            out = vec4<f32>(mix(h.rgb, out.rgb, w), out.w);
        }
    }
    textureStore(out_light, px, out);
}

// How open each lamp stands: rays out from it in fourteen directions, and the share that meets
// nothing within a few metres. A lamp shut inside a building (whose floor and inner walls the
// traced scene does not hold) finds walls and a roof all round, and is put out: its light would
// otherwise leak out under the walls.
@compute @workgroup_size(64)
fn enclosure(@builtin(global_invocation_id) id: vec3<u32>) {
    let k = i32(id.x);
    if (k >= i32(params.eye.w)) {
        return;
    }
    let l = lamps[k];
    var dirs = array<vec3<f32>, 14>(
        vec3<f32>(1.0, 0.0, 0.0), vec3<f32>(-1.0, 0.0, 0.0),
        vec3<f32>(0.0, 1.0, 0.0), vec3<f32>(0.0, -1.0, 0.0),
        vec3<f32>(0.0, 0.0, 1.0), vec3<f32>(0.0, 0.0, -1.0),
        vec3<f32>(0.577, 0.577, 0.577), vec3<f32>(-0.577, 0.577, 0.577),
        vec3<f32>(0.577, -0.577, 0.577), vec3<f32>(-0.577, -0.577, 0.577),
        vec3<f32>(0.577, 0.577, -0.577), vec3<f32>(-0.577, 0.577, -0.577),
        vec3<f32>(0.577, -0.577, -0.577), vec3<f32>(-0.577, -0.577, -0.577),
    );
    var shut = 0.0;
    for (var i: i32 = 0; i < 14; i = i + 1) {
        let d = dirs[i];
        // Downward rays meet the ground under any lamp, so only the walls and the roof count.
        if (d.z < -0.5) {
            continue;
        }
        if (blocked(l.pos.xyz + d * l.misc.x, d, 12.0)) {
            shut = shut + 1.0;
        }
    }
    // Nine of the rays count: shut on eight or more is inside.
    let share = shut / 9.0;
    open[k] = clamp((0.85 - share) / 0.15, 0.0, 1.0);
}
