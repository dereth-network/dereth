// Soft halos around the lamps the eye can see: a bright core and a faint wide glow, added to the
// lit picture. A pixel whose surface stands in front of the lamp takes none, so a wall or a post
// between the eye and the lamp cuts its halo cleanly.

struct Params {
    clip_to_render: mat4x4<f32>,
    viewport: vec4<f32>,
    // xyz the eye; w the number of halos.
    eye: vec4<f32>,
    // x the core radius (metres), y the wide radius, z the wide glow's share, w the gain.
    shape: vec4<f32>,
};

struct Halo {
    // xyz position; w the halo's size, metres.
    pos: vec4<f32>,
    // rgb colour times brightness; w the lamp's place in the lit list.
    col: vec4<f32>,
};

@group(0) @binding(0) var depth: texture_depth_2d;
@group(0) @binding(1) var<uniform> params: Params;
@group(0) @binding(2) var<storage, read> halos: array<Halo>;
// How open each lit lamp stands; a halo's `col.w` is its lamp's place in that list.
@group(0) @binding(3) var<storage, read> open: array<f32>;

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

@fragment
fn fs(@builtin(position) p: vec4<f32>) -> @location(0) vec4<f32> {
    let px = vec2<i32>(p.xy);
    let d = textureLoad(depth, px, 0);
    let eye = params.eye.xyz;
    let far_point = render_at(p.xy, 1.0);
    let ray = normalize(far_point - eye);
    var surface = 1e9;
    if (d < 1.0) {
        surface = length(render_at(p.xy, d) - eye);
    }
    var sum = vec3<f32>(0.0);
    let count = i32(params.eye.w);
    for (var k: i32 = 0; k < count; k = k + 1) {
        let h = halos[k];
        let o = open[u32(h.col.w)];
        if (o <= 0.0) {
            continue;
        }
        let to = h.pos.xyz - eye;
        let t = dot(to, ray);
        if (t <= 0.1) {
            continue;
        }
        let dist = length(to);
        // A surface in front of the lamp hides its halo (with room for the lamp's own glass).
        if (surface < dist - 0.35 * h.pos.w - 0.15) {
            continue;
        }
        let off = length(to - ray * t);
        let s = h.pos.w;
        let a = off / (params.shape.x * s);
        let b = off / (params.shape.y * s);
        let g = exp(-a * a) + params.shape.z * exp(-b * b);
        sum = sum + h.col.rgb * g * o;
    }
    return vec4<f32>(sum * params.shape.w, 0.0);
}
