// Soft halos around the lamps the eye can see: a bright core and a faint wide glow, added to the
// lit picture. A pixel whose surface stands in front of the lamp takes none, so a wall or a post
// between the eye and the lamp cuts its halo cleanly.
//
// An art pack's light has a glow in the air instead: the light the air scatters toward the eye
// along each pixel's ray, from the eye to the first surface. A light falling off as the inverse
// square of the distance gives, over a ray passing `h` from it, `(atan(b / h) - atan(a / h)) / h`
// between `a` and `b` along the ray from its nearest point, so the glow is round from every side,
// and fades to nothing where a wall stands close before the light, with no edge.

struct Params {
    clip_to_render: mat4x4<f32>,
    viewport: vec4<f32>,
    // xyz the eye; w the number of halos.
    eye: vec4<f32>,
    // x the core radius (metres), y the wide radius, z the wide glow's share, w the gain.
    shape: vec4<f32>,
    // x the share of a halo's size, y the metres, a surface may stand before its lamp and still
    // show it; z the gain of the glow in the air.
    fit: vec4<f32>,
};

struct Halo {
    // xyz position; w the halo's size, metres.
    pos: vec4<f32>,
    // rgb colour times brightness; w the lamp's place in the lit list.
    col: vec4<f32>,
    // x how brightly the air about it glows (0 for a halo), y how much brighter its glow is drawn
    // against the daylight, z its reach, w the radius of the light itself; metres.
    air: vec4<f32>,
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

// The glow of the air about the light `h`, `to` from the eye and `t` along the ray to its
// nearest point, over the ray as far as the surface `surface` metres away.
fn air_glow(h: Halo, to: vec3<f32>, t: f32, surface: f32) -> vec3<f32> {
    let off = sqrt(max(dot(to, to) - t * t, 0.0));
    let reach = max(h.air.z, 0.1);
    if (off >= reach) {
        return vec3<f32>(0.0);
    }
    // The light's own size keeps the glow finite looking straight at it.
    let hh = sqrt(off * off + h.air.w * h.air.w);
    let end = min(surface, t + reach);
    let start = max(0.0, t - reach);
    if (end <= start) {
        return vec3<f32>(0.0);
    }
    let along = (atan((end - t) / hh) - atan((start - t) / hh)) / hh;
    // Out to its reach the light fades to nothing, as its light on the ground does.
    let q = off / reach;
    let fade = (1.0 - q * q) * (1.0 - q * q);
    return h.col.rgb * (h.air.x * h.air.y * params.fit.z * along * fade);
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
        if (h.air.x > 0.0) {
            sum = sum + air_glow(h, to, t, surface) * o;
            continue;
        }
        if (t <= 0.1) {
            continue;
        }
        let dist = length(to);
        // A surface in front of the lamp hides its halo, fading in over room for the lamp's own glass.
        let room = params.fit.x * h.pos.w + params.fit.y;
        let seen = clamp((surface - dist + room) / room, 0.0, 1.0);
        if (seen <= 0.0) {
            continue;
        }
        let off = length(to - ray * t);
        let s = h.pos.w;
        let a = off / (params.shape.x * s);
        let b = off / (params.shape.y * s);
        let g = exp(-a * a) + params.shape.z * exp(-b * b);
        sum = sum + h.col.rgb * g * o * seen;
    }
    return vec4<f32>(sum * params.shape.w, 0.0);
}
