// An art pack's ground, laid over the re-shaded landscape in place of retail's pictures.
//
// Each pixel the landscape-normal pass marked as the landscape is found on the ground from the
// depth, in the cell it lies in. That cell's layers are retail's: the ground under them, then up
// to five overlays, each a ground blended in through one of retail's masks at a quarter turn. The
// pass lays the same layers again from the pack's materials. Far off, where the landscape is drawn
// in coarser cells, a cell's layers are those of the coarse cell it lies in, laid across all of
// it as retail lays them there. Close by, where a mask is partway,
// the higher of the two materials shows through (so grass stands up through dirt along their
// edge) instead of the two fading into each other; where it is whole, and far off, the mask's
// answer stands.
//
// Each material repeats over the ground, square with north, a whole number of times in every
// 1536 metres, so the repeats meet seamlessly wherever the world is. It is laid out in the broad
// patches of light, dark and colour of the retail picture it replaces, so the ground keeps its
// character from afar. Near the eye a finer repeat of the same material adds detail; farther off
// a coarser repeat is mixed in and the colour varies gently over metres to hundreds of metres, so
// the repeats do not show. A ground the pack has no material for is retail's picture, laid as
// retail lays it.
//
// The colour written is the material's under the old light, worked out from the new normal
// exactly as the light per pixel works it out and laid on the color as the landscape's own
// pictures take it, so the light per pixel lights it as it lights retail's ground; the normal
// carries its class with the material's roughness (3.05 to 3.35).

struct Params {
    // Clip space to render space, height up, at the frame's jitter.
    clip_to_render: mat4x4<f32>,
    // Render space, height up, to view space.
    render_to_view: mat4x4<f32>,
    // The world viewport: x, y, width, height, in target pixels.
    viewport: vec4<f32>,
    // xyz the eye, render space.
    eye: vec4<f32>,
    // xyz toward the sun.
    sun_dir: vec4<f32>,
    // The sun as the old light took it, 0..1 a channel.
    sun_legacy: vec4<f32>,
    // The ambient light as the old light took it.
    amb_legacy: vec4<f32>,
    // x where the authored fog begins, y where it is whole, metres; z 1 when the world is drawn
    // with it.
    fog: vec4<f32>,
    // The fog's colour, in the display's encoding.
    fog_colour: vec4<f32>,
    // xy the render-space position of the south-west corner of cell texel (0, 0); zw the cell
    // texels across and up.
    cells: vec4<f32>,
    // xy the global cell coordinates of cell texel (0, 0), whole numbers.
    first_cell: vec4<f32>,
    // x the detail near the eye, y the variation far off (0 none, 1 full), z how far two
    // materials' heights move their edge, w 1 to show the layers' materials as flat colours.
    tune: vec4<f32>,
    // x how strongly the retail ground's broad patches lay out the materials, y the share of that
    // they keep far off; zw unused.
    tune2: vec4<f32>,
};

@group(0) @binding(0) var depth: texture_depth_2d;
@group(0) @binding(1) var prior: texture_2d<f32>;
@group(0) @binding(2) var cells: texture_2d<u32>;
@group(0) @binding(3) var masks: texture_2d_array<f32>;
@group(0) @binding(4) var albedo_tex: texture_2d_array<f32>;
@group(0) @binding(5) var normal_tex: texture_2d_array<f32>;
@group(0) @binding(6) var wrap_samp: sampler;
@group(0) @binding(7) var clamp_samp: sampler;
@group(0) @binding(8) var<uniform> params: Params;
// Two a ground, for the 33 retail grounds (terrain types 0 to 31, and 32 the road):
// [2g] x the material's layer (negative for none), y 1 for retail's own picture, z that picture's
// repeats across a cell, w how strongly the normal picture bends the normal;
// [2g + 1] x the repeats in the period, y the finer repeats near the eye, z the coarser ones far
// off, w the repeats across a cell of the ground's broad picture (nought for none).
@group(0) @binding(9) var<uniform> grounds: array<vec4<f32>, 66>;
// Each retail ground's own picture shrunk to its broad patches, a layer a ground.
@group(0) @binding(10) var broad_tex: texture_2d_array<f32>;
// Each material's roughness and occlusion, in red and green, at half its pictures' size.
@group(0) @binding(11) var surface_tex: texture_2d_array<f32>;

const CELL: f32 = 24.0;
const PERIOD_CELLS: i32 = 64;
const PERIOD: f32 = 1536.0;
const COVERED: u32 = 0x80000000u;
const NO_GROUND: u32 = 63u;
// Where the detail fades out, and where the coarser repeat comes in, metres from the eye.
const DETAIL_NEAR: f32 = 3.0;
const DETAIL_FAR: f32 = 22.0;
const MACRO_NEAR: f32 = 14.0;
const MACRO_FAR: f32 = 110.0;
// Where the materials' heights stop deciding the edges between them, metres from the eye: far
// off, where their pictures are drawn smooth, the masks' own soft edges stand.
const HEIGHTS_NEAR: f32 = 12.0;
const HEIGHTS_FAR: f32 = 90.0;
// Where the retail ground's broad patches fade to their share far off, metres from the eye.
const BROAD_NEAR: f32 = 50.0;
const BROAD_FAR: f32 = 160.0;

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

// Where the view ray through pixel `px` meets the plane through `p` facing `n`: the ground a
// neighbouring pixel sees, for how far the pictures move from one pixel to the next.
fn on_plane(px: vec2<f32>, p: vec3<f32>, n: vec3<f32>) -> vec3<f32> {
    let r = render_at(px, 0.5) - params.eye.xyz;
    let across = dot(r, n);
    let reach = dot(p - params.eye.xyz, n);
    if (abs(across) < 1e-6 || reach / across <= 0.0) {
        return p;
    }
    return params.eye.xyz + r * (reach / across);
}

// A cell's coordinates in its mask, as the mask is walked at `rot` quarter turns.
fn turned(uv: vec2<f32>, rot: u32) -> vec2<f32> {
    switch rot {
        case 1u: { return vec2<f32>(1.0 - uv.y, uv.x); }
        case 2u: { return vec2<f32>(1.0 - uv.x, 1.0 - uv.y); }
        case 3u: { return vec2<f32>(uv.y, 1.0 - uv.x); }
        default: { return uv; }
    }
}

// How far `turned` moves a step `g` across the cell.
fn turned_step(g: vec2<f32>, rot: u32) -> vec2<f32> {
    switch rot {
        case 1u: { return vec2<f32>(-g.y, g.x); }
        case 2u: { return -g; }
        case 3u: { return vec2<f32>(g.y, -g.x); }
        default: { return g; }
    }
}

// What a layer of ground is at a pixel.
struct Layer {
    // The base colour, linear.
    albedo: vec3<f32>,
    // How high its surface stands, 0..1.
    height: f32,
    // The normal's lean toward east and north.
    tilt: vec2<f32>,
    roughness: f32,
    occlusion: f32,
    // 1 for the pack's material, 0 for retail's picture.
    pack: f32,
};

fn mix_layer(a: Layer, b: Layer, t: f32) -> Layer {
    var o: Layer;
    o.albedo = mix(a.albedo, b.albedo, t);
    o.height = mix(a.height, b.height, t);
    o.tilt = mix(a.tilt, b.tilt, t);
    o.roughness = mix(a.roughness, b.roughness, t);
    o.occlusion = mix(a.occlusion, b.occlusion, t);
    o.pack = mix(a.pack, b.pack, t);
    return o;
}

// How much of `over` shows where its mask lays `t` of it over `under`, `near` 1 close to the eye
// and 0 far off: none and all where the mask says so; between, the mask's share moved toward the
// higher surface, most halfway, and close by sharpened, so the higher material (grass, a road's
// stones) stands up through the lower along their edge. Where the heights are level, or far off,
// the mask's soft edge stands as retail draws it.
fn height_blend(under: f32, over: f32, t: f32, near: f32) -> f32 {
    if (t <= 0.0) {
        return 0.0;
    }
    if (t >= 1.0) {
        return 1.0;
    }
    let moved = clamp(t + (over - under) * params.tune.z * near * 4.0 * t * (1.0 - t), 0.0, 1.0);
    return mix(moved, smoothstep(0.3, 0.7, moved), near);
}

fn hash(p: vec2<i32>) -> f32 {
    var h = bitcast<u32>(p.x) * 0x8DA6B343u ^ bitcast<u32>(p.y) * 0xD8163841u;
    h = (h ^ (h >> 13u)) * 0x2C1B3C6Du;
    h = h ^ (h >> 15u);
    return f32(h & 0xFFFFu) / 65535.0;
}

// Smooth noise over a lattice `cell` metres wide, `wrap` cells to the period, 0..1.
fn noise(pos: vec2<f32>, cell: f32, wrap: i32) -> f32 {
    let q = pos / cell;
    let i = vec2<i32>(floor(q));
    let f = q - floor(q);
    let s = f * f * (3.0 - 2.0 * f);
    let w = vec2<i32>(wrap);
    let a = hash(((i % w) + w) % w);
    let b = hash((((i + vec2<i32>(1, 0)) % w) + w) % w);
    let c = hash((((i + vec2<i32>(0, 1)) % w) + w) % w);
    let d = hash((((i + vec2<i32>(1, 1)) % w) + w) % w);
    return mix(mix(a, b, s.x), mix(c, d, s.x), s.y);
}

// How the retail ground `g`'s broad picture varies about its mean at cell coordinates `cell_uv`
// of a drawn cell `cell_m` metres across (`dx`, `dy` the step to the next pixel), `dist` metres
// from the eye: each channel's share of its mean, 1 for none.
fn broad_at(g: u32, cell_uv: vec2<f32>, dx: vec2<f32>, dy: vec2<f32>, dist: f32, cell_m: f32) -> vec3<f32> {
    let k = grounds[g * 2u + 1u].w;
    let strength = params.tune2.x * mix(1.0, params.tune2.y, smoothstep(BROAD_NEAR, BROAD_FAR, dist));
    if (k <= 0.0 || strength <= 0.0) {
        return vec3<f32>(1.0);
    }
    let uv = cell_uv * k;
    let gx = vec2<f32>(dx.x, -dx.y) / cell_m * k;
    let gy = vec2<f32>(dy.x, -dy.y) / cell_m * k;
    let here = textureSampleGrad(broad_tex, wrap_samp, uv, i32(g), gx, gy).rgb;
    let mean = textureSampleLevel(broad_tex, wrap_samp, uv, i32(g), 16.0).rgb;
    let share = clamp(here / max(mean, vec3<f32>(0.01)), vec3<f32>(0.5), vec3<f32>(1.8));
    return mix(vec3<f32>(1.0), share, strength);
}

// The ground `g` at `pos` (metres into the period, east and north) and coordinates `cell_uv` in
// a drawn cell `cell_m` metres across, with `dx` and `dy` the ground's step to the next pixel
// across and down, `dist` metres from the eye.
fn ground_at(g: u32, pos: vec2<f32>, cell_uv: vec2<f32>, dx: vec2<f32>, dy: vec2<f32>, dist: f32, cell_m: f32) -> Layer {
    var l: Layer;
    l.albedo = vec3<f32>(0.2);
    l.height = 0.5;
    l.tilt = vec2<f32>(0.0);
    l.roughness = 0.9;
    l.occlusion = 1.0;
    l.pack = 0.0;
    if (g > 32u) {
        return l;
    }
    let a = grounds[g * 2u];
    let b = grounds[g * 2u + 1u];
    if (a.x < 0.0) {
        return l;
    }
    let layer = i32(a.x);
    if (a.y > 0.5) {
        // Retail's own picture, its repeats square with the cell.
        let k = a.z;
        let uv = cell_uv * k;
        let gx = vec2<f32>(dx.x, -dx.y) / cell_m * k;
        let gy = vec2<f32>(dy.x, -dy.y) / cell_m * k;
        let c = textureSampleGrad(albedo_tex, wrap_samp, uv, layer, gx, gy);
        let nm = textureSampleGrad(normal_tex, wrap_samp, uv, layer, gx, gy);
        let s = textureSampleGrad(surface_tex, wrap_samp, uv, layer, gx, gy);
        l.albedo = c.rgb;
        l.height = c.a;
        l.tilt = (nm.xy * 2.0 - vec2<f32>(1.0)) * a.w;
        l.roughness = s.x;
        l.occlusion = s.y / occlusion_mean(uv, layer);
        return l;
    }
    // North up the pictures: their rows run down, toward the south.
    let flip = vec2<f32>(1.0, -1.0);
    let per = b.x / PERIOD;
    let uv = pos * flip * per;
    let c = textureSampleGrad(albedo_tex, wrap_samp, uv, layer, dx * flip * per, dy * flip * per);
    let nm = textureSampleGrad(normal_tex, wrap_samp, uv, layer, dx * flip * per, dy * flip * per);
    let s = textureSampleGrad(surface_tex, wrap_samp, uv, layer, dx * flip * per, dy * flip * per);
    l.albedo = c.rgb;
    l.height = c.a;
    l.tilt = (nm.xy * 2.0 - vec2<f32>(1.0)) * a.w;
    l.roughness = s.x;
    // Relative to the material's mean: its color already holds retail's picture's mean, so its
    // occlusion shapes the light without darkening the ground as a whole.
    let occ_mean = occlusion_mean(uv, layer);
    l.occlusion = s.y / occ_mean;
    l.pack = 1.0;
    // Laid out in the retail ground's own broad patches of light, dark and colour.
    l.albedo = l.albedo * broad_at(g, cell_uv, dx, dy, dist, cell_m);
    // Far off, a coarser repeat of the same material, offset, mixed in.
    let far = smoothstep(MACRO_NEAR, MACRO_FAR, dist) * 0.5 * params.tune.y;
    if (far > 0.001) {
        let pm = b.z / PERIOD;
        let um = pos * flip * pm + vec2<f32>(0.37, 0.61);
        let cm = textureSampleGrad(albedo_tex, wrap_samp, um, layer, dx * flip * pm, dy * flip * pm);
        let nmm = textureSampleGrad(normal_tex, wrap_samp, um, layer, dx * flip * pm, dy * flip * pm);
        let sm = textureSampleGrad(surface_tex, wrap_samp, um, layer, dx * flip * pm, dy * flip * pm);
        l.albedo = mix(l.albedo, cm.rgb, far);
        l.height = mix(l.height, cm.a, far);
        l.tilt = mix(l.tilt, (nmm.xy * 2.0 - vec2<f32>(1.0)) * a.w, far);
        l.roughness = mix(l.roughness, sm.x, far);
        l.occlusion = mix(l.occlusion, sm.y / occ_mean, far);
    }
    // Near the eye, a finer repeat of the same material lends its grain to the colour, the
    // normal and the height.
    let near = (1.0 - smoothstep(DETAIL_NEAR, DETAIL_FAR, dist)) * params.tune.x;
    if (near > 0.001) {
        let pd = b.y / PERIOD;
        let ud = pos * flip * pd + vec2<f32>(0.23, 0.79);
        let cd = textureSampleGrad(albedo_tex, wrap_samp, ud, layer, dx * flip * pd, dy * flip * pd);
        let sd = textureSampleGrad(normal_tex, wrap_samp, ud, layer, dx * flip * pd, dy * flip * pd);
        // The material's mean colour: its one-texel level.
        let mean = textureSampleLevel(albedo_tex, wrap_samp, uv, layer, 16.0).rgb;
        let grain = clamp(luminance(cd.rgb) / max(luminance(mean), 0.01), 0.6, 1.5);
        l.albedo = l.albedo * mix(1.0, grain, near * 0.45);
        l.tilt = l.tilt + (sd.xy * 2.0 - vec2<f32>(1.0)) * a.w * near * 0.5;
        l.height = clamp(l.height + (cd.a - 0.5) * near * 0.3, 0.0, 1.0);
    }
    return l;
}

// A color `albedo` (linear) under the old light `legacy` (0..1 a channel, as the vertex light
// holds it), lit as the landscape's own pictures are: the color in the display's encoding times
// the light, then decoded. Retail's ground is drawn that way, and the light per pixel divides the
// old light out of both alike; under a dim light, decoding the product keeps more of the darker
// channels than the product of the decoded parts, so a ground lit the other way would come out
// darker and redder than retail's at dusk.
fn lit_as_drawn(albedo: vec3<f32>, legacy: vec3<f32>) -> vec3<f32> {
    return srgb_to_linear(linear_to_srgb(max(albedo, vec3<f32>(0.0))) * legacy);
}

// A material's mean occlusion: its one-texel level.
fn occlusion_mean(uv: vec2<f32>, layer: i32) -> f32 {
    return max(textureSampleLevel(surface_tex, wrap_samp, uv, layer, 16.0).y, 0.05);
}

// A flat colour for each material, for the layers view.
fn layer_colour(g: u32) -> vec3<f32> {
    let h = hash(vec2<i32>(i32(g), 7));
    let k = hash(vec2<i32>(i32(g), 13));
    return vec3<f32>(0.25 + 0.7 * h, 0.25 + 0.7 * k, 0.25 + 0.7 * fract(h + k));
}

struct Out {
    @location(0) colour: vec4<f32>,
    @location(1) normal: vec4<f32>,
};

@fragment
fn fs_ground(@builtin(position) p: vec4<f32>) -> Out {
    let px = vec2<i32>(p.xy);
    let nw = textureLoad(prior, px, 0);
    // The landscape-normal pass's landscape, exactly: the retail landscape's class.
    if (abs(nw.w - 3.0) > 0.01) {
        discard;
    }
    let d = textureLoad(depth, px, 0);
    if (d >= 1.0) {
        discard;
    }
    let world = render_at(p.xy, d);
    let t = (world.xy - params.cells.xy) / CELL;
    let ci = vec2<i32>(floor(t));
    if (any(ci < vec2<i32>(0)) || ci.x >= i32(params.cells.z) || ci.y >= i32(params.cells.w)) {
        discard;
    }
    let words = textureLoad(cells, ci, 0);
    if ((words.x & COVERED) == 0u) {
        discard;
    }
    let f = clamp(t - floor(t), vec2<f32>(0.0), vec2<f32>(1.0));
    // The drawn cell: `span` cells across, starting at a multiple of `span` (the window starts
    // at a block's corner, eight cells to a block).
    let span_i = 1 << ((words.x >> 12u) & 3u);
    let span = f32(span_i);
    let cell_m = CELL * span;
    let drawn = (ci / vec2<i32>(span_i)) * vec2<i32>(span_i);
    let fd = (vec2<f32>(ci - drawn) + f) / span;
    // The drawn cell's own coordinates as retail's composite has them: east across, north up.
    let cell_uv = vec2<f32>(fd.x, 1.0 - fd.y);
    let global = vec2<i32>(params.first_cell.xy) + ci;
    let p_cell = ((global % vec2<i32>(PERIOD_CELLS)) + vec2<i32>(PERIOD_CELLS)) % vec2<i32>(PERIOD_CELLS);
    let pos = (vec2<f32>(p_cell) + f) * CELL;
    let to_eye = params.eye.xyz - world;
    let dist = length(to_eye);
    let n = normalize((transpose(params.render_to_view) * vec4<f32>(nw.xyz, 0.0)).xyz);
    // How far the ground moves from this pixel to the next, across and down; no farther than a
    // hundredth of the distance a pixel, where the ground runs nearly along the view.
    let cap = dist * 0.01 + 0.01;
    var dx = (on_plane(p.xy + vec2<f32>(1.0, 0.0), world, n) - world).xy;
    var dy = (on_plane(p.xy + vec2<f32>(0.0, 1.0), world, n) - world).xy;
    dx = dx * min(1.0, cap / max(length(dx), 1e-6));
    dy = dy * min(1.0, cap / max(length(dy), 1e-6));
    let cell_dx = vec2<f32>(dx.x, -dx.y) / cell_m;
    let cell_dy = vec2<f32>(dy.x, -dy.y) / cell_m;

    let base = words.x & 0x3Fu;
    let count = min((words.x >> 8u) & 7u, 5u);
    var m = ground_at(base, pos, cell_uv, dx, dy, dist, cell_m);
    let near = 1.0 - smoothstep(HEIGHTS_NEAR, HEIGHTS_FAR, dist);
    var shown = layer_colour(base);
    for (var k = 0u; k < 5u; k = k + 1u) {
        if (k >= count) {
            break;
        }
        let o = (words[1u + k / 2u] >> (16u * (k % 2u))) & 0xFFFFu;
        let g = o & 0x3Fu;
        let mask = i32((o >> 6u) & 0x3Fu);
        let rot = (o >> 12u) & 3u;
        let a = textureSampleGrad(
            masks,
            clamp_samp,
            turned(cell_uv, rot),
            mask,
            turned_step(cell_dx, rot),
            turned_step(cell_dy, rot),
        ).r;
        let cover = 1.0 - a;
        if (cover < 0.002) {
            continue;
        }
        let s = ground_at(g, pos, cell_uv, dx, dy, dist, cell_m);
        let w = height_blend(m.height, s.height, cover, near);
        m = mix_layer(m, s, w);
        shown = mix(shown, layer_colour(g), w);
    }
    // Gentle variation of the pack's colour, in patches a few metres across and over tens and
    // hundreds of metres, as the ground's own pictures vary from place to place.
    let patches = (noise(pos, 4.0, 384) - 0.5) * 0.08 + (noise(pos + vec2<f32>(17.0, 5.0), 6.0, 256) - 0.5) * 0.06;
    let broad = (noise(pos, 32.0, 48) - 0.5) * 0.14 + (noise(pos + vec2<f32>(500.0), 128.0, 12) - 0.5) * 0.12;
    let vary = 1.0 + params.tune.y * (patches + broad);
    var albedo = clamp(m.albedo * mix(1.0, vary, m.pack), vec3<f32>(0.0), vec3<f32>(1.0));
    if (params.tune.w > 0.5) {
        albedo = shown;
    }
    // The surface's frame: east and north laid on the landscape's smooth surface.
    let east = normalize(vec3<f32>(1.0, 0.0, 0.0) - n * n.x);
    let north = cross(n, east);
    let bent = normalize(east * m.tilt.x + north * m.tilt.y + n);
    // The old light, as the light per pixel works it out from the normal it is given.
    let ndl = max(dot(bent, params.sun_dir.xyz), 0.0);
    let legacy = min(params.amb_legacy.rgb + params.sun_legacy.rgb * ndl, vec3<f32>(1.0));
    // The occlusion darkens the light from the sky, not the sun's.
    let sun_part = luminance(params.sun_legacy.rgb) * ndl;
    let sky_part = luminance(params.amb_legacy.rgb);
    let direct = sun_part / max(sun_part + sky_part, 1e-3);
    let occlusion = mix(m.occlusion, 1.0, direct);
    var c = lit_as_drawn(albedo * occlusion, legacy);
    if (params.fog.z > 0.5) {
        let fogged = clamp((dist - params.fog.x) / max(params.fog.y - params.fog.x, 1e-3), 0.0, 1.0);
        c = srgb_to_linear(mix(linear_to_srgb(max(c, vec3<f32>(0.0))), params.fog_colour.rgb, fogged));
    }
    var o: Out;
    o.colour = vec4<f32>(c, 1.0);
    let view_n = normalize((params.render_to_view * vec4<f32>(bent, 0.0)).xyz);
    o.normal = vec4<f32>(view_n, 3.05 + 0.3 * clamp(m.roughness, 0.0, 1.0));
    return o;
}
