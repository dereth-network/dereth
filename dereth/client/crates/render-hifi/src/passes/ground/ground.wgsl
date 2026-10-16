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
    // While the presentation draws the ground itself: x 1 when the world's terrain words are
    // bound, for the ground past the cells laid out here; yz where render space's origin lies in
    // the world, meters; w unused.
    drawn: vec4<f32>,
    // Scree and rock by the drawn slope, degrees: x where scree starts, y the slopes it blends in
    // over; z where rock starts, w the slopes it blends in over.
    slope: vec4<f32>,
    // x how much of the scree and rock is laid (nought for none), y and z the grounds they are
    // drawn with; w how much the slope they are laid by is the smooth surface's.
    slope2: vec4<f32>,
    // x how much of the scree and rock shows through snow and ice; yzw unused.
    slope3: vec4<f32>,
    // The forest cover: x how strongly it tints the far ground (nought for none); y and z where it
    // comes in, meters from the eye; w how much its crowns shade the ground under them past the
    // sun's shadows.
    cover: vec4<f32>,
    // The water, while it is drawn as water: x 1; y the ground its bed is drawn with away from
    // the shore; z how much darker the wet bank is; w how much grass grows in over a muddy bank.
    water: vec4<f32>,
    // x the ground grown in over a muddy bank where no other lies beside it; yzw unused.
    water2: vec4<f32>,
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
// Two a ground, for the 33 retail grounds (terrain types 0 to 31, and 32 the road), and from
// `STEEP` on the same again for the pack's materials for each ground's steep slopes:
// [2g] x the material's layer (negative for none), y 1 for retail's own picture, z that picture's
// repeats across a cell, w how strongly the normal picture bends the normal;
// [2g + 1] x the repeats in the period, y the finer repeats near the eye, z the coarser ones far
// off, w the repeats across a cell of the ground's broad picture (nought for none).
@group(0) @binding(9) var<uniform> grounds: array<vec4<f32>, 194>;
// Each retail ground's own picture shrunk to its broad patches, a layer a ground.
@group(0) @binding(10) var broad_tex: texture_2d_array<f32>;
// Each material's roughness and occlusion, in red and green, at half its pictures' size.
@group(0) @binding(11) var surface_tex: texture_2d_array<f32>;
// The world's landscape terrain words, while the presentation draws the ground itself.
@group(0) @binding(12) var world_words: texture_2d<u32>;
// The world's landscape vertex heights, likewise.
@group(0) @binding(13) var world_heights: texture_2d<f32>;
// The picture as the opaque world left it: where the presentation draws the ground itself, its
// pixels hold their distance from the eye (see `placed`).
@group(0) @binding(14) var places: texture_2d<f32>;
// Where the water lies, a value pair each landscape vertex (see the shore's arithmetic).
@group(0) @binding(15) var water_field: texture_2d<f32>;
// The forest cover at each of the world's landscape vertices: the crowns' colour, and in alpha
// the share of the ground they cover.
@group(0) @binding(16) var cover_tex: texture_2d<f32>;

const CELL: f32 = 24.0;
const PERIOD_CELLS: i32 = 64;
const PERIOD: f32 = 1536.0;
const COVERED: u32 = 0x80000000u;
const NO_GROUND: u32 = 63u;
// The retail grounds: the terrain types and the road.
const GROUNDS: u32 = 33u;
// Where the table's steep materials begin: a ground's own steep material is `STEEP` past it.
const STEEP: u32 = 64u;
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

// The scale of the distance the drawn ground writes in place of a color, and the blue that marks
// it: the drawn ground's own.
const CODE_SCALE: f32 = 4096.0;
const CODE_WHOLE_SCALE: f32 = 65536.0;
const CODE_MARK: f32 = -4096.0;

// The distance the drawn ground wrote in place of a color, meters: the whole 64s a whole
// number, the rest as written.
fn coded_distance(code: vec4<f32>) -> f32 {
    return round(code.r * CODE_WHOLE_SCALE) * 64.0 + code.g * CODE_SCALE;
}

// The drawn ground's share of a pixel a cut-out edge was blended over, from the mark its
// distance left there: 1 where nothing was.
fn ground_share(code: vec4<f32>) -> f32 {
    return clamp(code.b / CODE_MARK, 0.0, 1.0);
}

// The color of the edge blended over the drawn ground's distance `code`, made whole: its own
// share of the color, the mark taken out (the distance it carries is under two hundredths), over
// its share of the pixel.
fn edge_colour(code: vec4<f32>) -> vec3<f32> {
    let share = ground_share(code);
    let edge = vec3<f32>(code.r, code.g, code.b - share * CODE_MARK);
    return max(edge, vec3<f32>(0.0)) / max(1.0 - share, 0.05);
}

// Where the ground at pixel `px` lies, render space: from the depth `d`, or, for a pixel of the
// ground the presentation draws itself, along the same view ray at the distance it holds, which
// far off is exact where the depth is coarse by tens or hundreds of metres. A distance far from
// the depth's is not one the drawn ground wrote, and the depth's place stands.
fn placed(px: vec2<f32>, d: f32) -> vec3<f32> {
    let world = render_at(px, d);
    if (params.drawn.x < 0.5) {
        return world;
    }
    let code = textureLoad(places, vec2<i32>(px), 0);
    if (code.b > CODE_MARK * 0.99) {
        return world;
    }
    let ray = world - params.eye.xyz;
    let seen = length(ray);
    let exact = coded_distance(code);
    if (abs(exact - seen) > seen * 0.05 + 1.0) {
        return world;
    }
    return params.eye.xyz + ray * (exact / max(seen, 1e-3));
}

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
    // 1 for snow and ice.
    snow: f32,
    // 1 for a ground's own material for its steep slopes.
    own: f32,
    // 1 for retail's water ground, laid as the bed under the water and its wet bank.
    wet: f32,
    // 1 for mud and marsh.
    mud: f32,
};

fn mix_layer(a: Layer, b: Layer, t: f32) -> Layer {
    var o: Layer;
    o.albedo = mix(a.albedo, b.albedo, t);
    o.height = mix(a.height, b.height, t);
    o.tilt = mix(a.tilt, b.tilt, t);
    o.roughness = mix(a.roughness, b.roughness, t);
    o.occlusion = mix(a.occlusion, b.occlusion, t);
    o.pack = mix(a.pack, b.pack, t);
    o.snow = mix(a.snow, b.snow, t);
    o.own = mix(a.own, b.own, t);
    o.wet = mix(a.wet, b.wet, t);
    o.mud = mix(a.mud, b.mud, t);
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

// How much of a variation `cell` metres across shows in a pixel that covers `foot` metres of
// ground: all of it while the pixel is a quarter of it or less, none once it is half of it.
fn resolved(cell: f32, foot: f32) -> f32 {
    return 1.0 - smoothstep(0.25 * cell, 0.5 * cell, foot);
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
    // A steep material is laid out in its ground's broad patches.
    let flat = select(g, g - STEEP, g >= STEEP);
    let strength = params.tune2.x * mix(1.0, params.tune2.y, smoothstep(BROAD_NEAR, BROAD_FAR, dist));
    if (k <= 0.0 || strength <= 0.0) {
        return vec3<f32>(1.0);
    }
    let uv = cell_uv * k;
    let gx = vec2<f32>(dx.x, -dx.y) / cell_m * k;
    let gy = vec2<f32>(dy.x, -dy.y) / cell_m * k;
    let here = textureSampleGrad(broad_tex, wrap_samp, uv, i32(flat), gx, gy).rgb;
    let mean = textureSampleLevel(broad_tex, wrap_samp, uv, i32(flat), 16.0).rgb;
    let share = clamp(here / max(mean, vec3<f32>(0.01)), vec3<f32>(0.5), vec3<f32>(1.8));
    return mix(vec3<f32>(1.0), share, strength);
}

// The ground `g` at `pos` (metres into the period, east and north) and coordinates `cell_uv` in
// a drawn cell `cell_m` metres across, with `dx` and `dy` the ground's step to the next pixel
// across and down, `dist` metres from the eye.
// Where the pixel lies about the water, while it is drawn as water: how far inside it, meters
// (negative on land), and the ground of the nearest landscape vertex that is not water.
var<private> pixel_inside: f32 = -1000.0;
var<private> pixel_land: u32 = 1u;

// How far from the water a bank is taken to reach, meters.
const BANK_REACH: f32 = 40.0;

// How far inside the water the bed's own ground starts to take over from the land beside it, and
// where it has, meters: the shallows at the edge are the bank's own ground, under the water.
const BED_FROM: f32 = 6.0;
const BED_IN: f32 = 20.0;

fn ground_at(ground: u32, pos: vec2<f32>, cell_uv: vec2<f32>, dx: vec2<f32>, dy: vec2<f32>, dist: f32, cell_m: f32) -> Layer {
    // Retail's water, while the water is drawn as water, is the ground it lies on: the bed's own
    // ground well inside it, and the land beside it at its edge and above it, wet.
    if (params.water.x > 0.5 && ground >= 16u && ground <= 20u) {
        let into = smoothstep(BED_FROM, BED_IN, pixel_inside);
        var l = plain_ground(pixel_land, pos, cell_uv, dx, dy, dist, cell_m);
        if (into > 0.001) {
            l = mix_layer(l, plain_ground(u32(params.water.y), pos, cell_uv, dx, dy, dist, cell_m), into);
        }
        l.wet = 1.0;
        return l;
    }
    return plain_ground(ground, pos, cell_uv, dx, dy, dist, cell_m);
}

// The ground `g` laid as its material lays it.
fn plain_ground(g: u32, pos: vec2<f32>, cell_uv: vec2<f32>, dx: vec2<f32>, dy: vec2<f32>, dist: f32, cell_m: f32) -> Layer {
    var l: Layer;
    l.albedo = vec3<f32>(0.2);
    l.height = 0.5;
    l.tilt = vec2<f32>(0.0);
    l.roughness = 0.9;
    l.occlusion = 1.0;
    l.pack = 0.0;
    l.snow = select(0.0, 1.0, g == 2u || g == 15u || g == 27u);
    l.own = select(0.0, 1.0, g >= STEEP);
    l.wet = 0.0;
    l.mud = select(0.0, 1.0, g == 4u || g == 5u);
    if ((g >= GROUNDS && g < STEEP) || g >= STEEP + GROUNDS) {
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

// The steepest ground a body stands on, as its normal's height.
const WALKABLE_Z: f32 = 0.6641741;

// How far toward straight up retail's own triangle at `q`, in cells, faces: its normal's height.
fn retail_up(q: vec2<f32>) -> f32 {
    let side = vec2<i32>(textureDimensions(world_heights));
    let c = vec2<i32>(floor(q));
    let f = clamp(q - floor(q), vec2<f32>(0.0), vec2<f32>(1.0));
    let at = clamp(c, vec2<i32>(0), side - vec2<i32>(2));
    let h00 = textureLoad(world_heights, at, 0).r;
    let h10 = textureLoad(world_heights, at + vec2<i32>(1, 0), 0).r;
    let h01 = textureLoad(world_heights, at + vec2<i32>(0, 1), 0).r;
    let h11 = textureLoad(world_heights, at + vec2<i32>(1, 1), 0).r;
    let x = bitcast<u32>(c.x);
    let y = bitcast<u32>(c.y);
    let cut = ((x * 0x0CCAC033u + 0x6C1AC587u) * y + x * 0xBDE41C43u + 0xAE6470DBu) >> 31u;
    var g = vec2<f32>(0.0);
    if (cut != 0u) {
        g = select(vec2<f32>(h11 - h01, h01 - h00), vec2<f32>(h10 - h00, h11 - h10), f.x >= f.y);
    } else {
        g = select(vec2<f32>(h11 - h01, h11 - h10), vec2<f32>(h10 - h00, h01 - h00), f.x + f.y <= 1.0);
    }
    let slope = g / CELL;
    return inverseSqrt(1.0 + dot(slope, slope));
}

// How far toward straight up the smooth surface through the world's landscape vertices faces at
// `q`, in cells: its normal's height, from the bicubic's own slopes.
fn smooth_up(q: vec2<f32>) -> f32 {
    let side = vec2<i32>(textureDimensions(world_heights));
    let c = vec2<i32>(floor(q));
    let t = clamp(q - floor(q), vec2<f32>(0.0), vec2<f32>(1.0));
    let t2 = t * t;
    let t3 = t2 * t;
    // The Catmull-Rom weights and their slopes, across and up.
    let wx = 0.5 * vec4<f32>(-t3.x + 2.0 * t2.x - t.x, 3.0 * t3.x - 5.0 * t2.x + 2.0, -3.0 * t3.x + 4.0 * t2.x + t.x, t3.x - t2.x);
    let wy = 0.5 * vec4<f32>(-t3.y + 2.0 * t2.y - t.y, 3.0 * t3.y - 5.0 * t2.y + 2.0, -3.0 * t3.y + 4.0 * t2.y + t.y, t3.y - t2.y);
    let dx = 0.5 * vec4<f32>(-3.0 * t2.x + 4.0 * t.x - 1.0, 9.0 * t2.x - 10.0 * t.x, -9.0 * t2.x + 8.0 * t.x + 1.0, 3.0 * t2.x - 2.0 * t.x);
    let dy = 0.5 * vec4<f32>(-3.0 * t2.y + 4.0 * t.y - 1.0, 9.0 * t2.y - 10.0 * t.y, -9.0 * t2.y + 8.0 * t.y + 1.0, 3.0 * t2.y - 2.0 * t.y);
    var gx = 0.0;
    var gy = 0.0;
    for (var i = 0; i < 4; i = i + 1) {
        for (var j = 0; j < 4; j = j + 1) {
            let g = clamp(c + vec2<i32>(i - 1, j - 1), vec2<i32>(0), side - vec2<i32>(1));
            let h = textureLoad(world_heights, g, 0).r;
            gx = gx + dx[i] * wy[j] * h;
            gy = gy + wx[i] * dy[j] * h;
        }
    }
    let slope = vec2<f32>(gx, gy) / CELL;
    return inverseSqrt(1.0 + dot(slope, slope));
}

// The world's landscape vertex `g`'s height, meters.
fn vertex_height(g: vec2<i32>) -> f32 {
    let side = vec2<i32>(textureDimensions(world_heights));
    return textureLoad(world_heights, clamp(g, vec2<i32>(0), side - vec2<i32>(1)), 0).r;
}

// Whether the cell's shared edge runs south-west to north-east: retail's hash of the cell.
fn sw_to_ne(c: vec2<i32>) -> bool {
    let x = bitcast<u32>(c.x);
    let y = bitcast<u32>(c.y);
    return (((x * 0x0CCAC033u + 0x6C1AC587u) * y + x * 0xBDE41C43u + 0xAE6470DBu) >> 31u) != 0u;
}

// The ground of the world's landscape vertex `g`: its road, or its terrain type.
fn corner_ground(g: vec2<i32>) -> u32 {
    let side = vec2<i32>(textureDimensions(world_words));
    let w = textureLoad(world_words, clamp(g, vec2<i32>(0), side - vec2<i32>(1)), 0).r;
    if ((w & 3u) != 0u) {
        return 32u;
    }
    return (w >> 2u) & 0x1Fu;
}

// The ground laid from the world's landscape vertices `step` cells apart at `gt` (global cell
// coordinates): the four corners' grounds about it, blended across the coarse cell between them.
fn lattice_ground(gt: vec2<f32>, step: i32, mode: u32, pos: vec2<f32>, cell_uv: vec2<f32>, dx: vec2<f32>, dy: vec2<f32>, dist: f32, cell_m: f32) -> Layer {
    let q = gt / f32(step);
    let c = vec2<i32>(floor(q)) * step;
    let f = clamp(q - floor(q), vec2<f32>(0.0), vec2<f32>(1.0));
    let g00 = mapped(corner_ground(c), mode);
    let g10 = mapped(corner_ground(c + vec2<i32>(step, 0)), mode);
    let g01 = mapped(corner_ground(c + vec2<i32>(0, step)), mode);
    let g11 = mapped(corner_ground(c + vec2<i32>(step, step)), mode);
    let a = ground_at(g00, pos, cell_uv, dx, dy, dist, cell_m);
    if (g10 == g00 && g01 == g00 && g11 == g00) {
        return a;
    }
    let b = ground_at(g10, pos, cell_uv, dx, dy, dist, cell_m);
    let c01 = ground_at(g01, pos, cell_uv, dx, dy, dist, cell_m);
    let d = ground_at(g11, pos, cell_uv, dx, dy, dist, cell_m);
    return mix_layer(mix_layer(a, b, f.x), mix_layer(c01, d, f.x), f.y);
}

// The ground past the cells laid out, from the world's own terrain words at `gt` (global cell
// coordinates), each ground laid as `mapped` lays it in `mode`, as a pixel covering `foot` metres
// sees it: each cell's corners blended across it while a cell is two pixels or more, and farther
// off the corners of ever coarser cells (every second vertex, every fourth, ...), the two nearest
// such lattices mixed, so the grounds change no faster across the picture than the pixels can
// hold.
fn far_ground(gt: vec2<f32>, foot: f32, mode: u32, pos: vec2<f32>, cell_uv: vec2<f32>, dx: vec2<f32>, dy: vec2<f32>, dist: f32, cell_m: f32) -> Layer {
    let level = clamp(log2(max(foot * 2.0 / CELL, 1e-3)), 0.0, 6.0);
    let m = floor(level);
    let t = level - m;
    let fine_step = i32(exp2(m));
    let near = lattice_ground(gt, fine_step, mode, pos, cell_uv, dx, dy, dist, cell_m);
    if (t < 0.01) {
        return near;
    }
    let coarse = lattice_ground(gt, fine_step * 2, mode, pos, cell_uv, dx, dy, dist, cell_m);
    return mix_layer(near, coarse, t);
}

// The ground laid in ground `g`'s place in `mode`: in mode 0 the ground itself; on steep ground
// (mode 1 for scree, 2 for rock) the pack's own material for that ground's steep slopes where it
// has one, otherwise the generic scree or rock.
fn mapped(g: u32, mode: u32) -> u32 {
    if (mode == 0u || g >= GROUNDS) {
        return g;
    }
    let s = g + STEEP;
    if (grounds[s * 2u].x >= 0.0) {
        return s;
    }
    return select(u32(params.slope2.z), u32(params.slope2.y), mode == 1u);
}

// The ground of the landscape vertex nearest `gt` (global cell coordinates) that is not water, of
// the four about it, nearest first; the bed's own ground where all four are water.
fn land_beside(gt: vec2<f32>) -> u32 {
    let c = vec2<i32>(floor(gt));
    let f = gt - floor(gt);
    var best = u32(params.water.y);
    var nearest = 1e9;
    for (var k = 0; k < 4; k = k + 1) {
        let o = vec2<i32>(k & 1, k >> 1u);
        let g = corner_ground(c + o);
        let d = length(f - vec2<f32>(o));
        if ((g < 16u || g > 20u) && d < nearest) {
            nearest = d;
            best = g;
        }
    }
    return best;
}

// The ground of the landscape vertex nearest `gt` (global cell coordinates) that is neither water
// nor mud nor marsh, of the four about it, nearest first; the bank's own grass where none is.
fn grass_beside(gt: vec2<f32>) -> u32 {
    let c = vec2<i32>(floor(gt));
    let f = gt - floor(gt);
    var best = u32(params.water2.x);
    var nearest = 1e9;
    for (var k = 0; k < 4; k = k + 1) {
        let o = vec2<i32>(k & 1, k >> 1u);
        let g = corner_ground(c + o);
        let d = length(f - vec2<f32>(o));
        if ((g < 16u || g > 20u) && g != 4u && g != 5u && d < nearest) {
            nearest = d;
            best = g;
        }
    }
    return best;
}

// Where a pixel lies among the grounds, for `layered`.
struct Lay {
    // The cell's layers, as the cell texture holds them.
    words: vec4<u32>,
    // Past the cells laid out: the ground laid from the world's own terrain words.
    far_laid: bool,
    // The pixel's global cell coordinates, for the far ground.
    gt: vec2<f32>,
    // The ground the pixel covers, metres.
    foot: f32,
    // Metres into the period, east and north.
    pos: vec2<f32>,
    // Its coordinates in its drawn cell, and their steps to the next pixel across and down.
    cell_uv: vec2<f32>,
    cell_dx: vec2<f32>,
    cell_dy: vec2<f32>,
    // The ground's steps to the next pixel across and down, metres.
    dx: vec2<f32>,
    dy: vec2<f32>,
    // Metres from the eye.
    dist: f32,
    // The drawn cell's size, metres.
    cell_m: f32,
    // 1 close to the eye, where the materials' heights decide their edges.
    near: f32,
};

// The pixel's ground: the cell's ground and its overlays as retail lays them, each ground laid as
// `mapped` lays it in `mode`.
fn layered(at: Lay, mode: u32) -> Layer {
    if (at.far_laid) {
        return far_ground(at.gt, at.foot, mode, at.pos, at.cell_uv, at.dx, at.dy, at.dist, at.cell_m);
    }
    var m = ground_at(mapped(at.words.x & 0x3Fu, mode), at.pos, at.cell_uv, at.dx, at.dy, at.dist, at.cell_m);
    let count = min((at.words.x >> 8u) & 7u, 5u);
    for (var k = 0u; k < 5u; k = k + 1u) {
        if (k >= count) {
            break;
        }
        let o = (at.words[1u + k / 2u] >> (16u * (k % 2u))) & 0xFFFFu;
        let g = o & 0x3Fu;
        let mask = i32((o >> 6u) & 0x3Fu);
        let rot = (o >> 12u) & 3u;
        let a = textureSampleGrad(
            masks,
            clamp_samp,
            turned(at.cell_uv, rot),
            mask,
            turned_step(at.cell_dx, rot),
            turned_step(at.cell_dy, rot),
        ).r;
        let cover = 1.0 - a;
        if (cover < 0.002) {
            continue;
        }
        let s = ground_at(mapped(g, mode), at.pos, at.cell_uv, at.dx, at.dy, at.dist, at.cell_m);
        m = mix_layer(m, s, height_blend(m.height, s.height, cover, at.near));
    }
    return m;
}

// The layers view's flat colours for the pixel's grounds, laid as `layered` lays them.
fn shown_colour(at: Lay) -> vec3<f32> {
    if (at.far_laid) {
        return layer_colour(corner_ground(vec2<i32>(floor(at.gt))));
    }
    var shown = layer_colour(at.words.x & 0x3Fu);
    let count = min((at.words.x >> 8u) & 7u, 5u);
    for (var k = 0u; k < 5u; k = k + 1u) {
        if (k >= count) {
            break;
        }
        let o = (at.words[1u + k / 2u] >> (16u * (k % 2u))) & 0xFFFFu;
        let mask = i32((o >> 6u) & 0x3Fu);
        let rot = (o >> 12u) & 3u;
        let a = textureSampleGrad(
            masks,
            clamp_samp,
            turned(at.cell_uv, rot),
            mask,
            turned_step(at.cell_dx, rot),
            turned_step(at.cell_dy, rot),
        ).r;
        shown = mix(shown, layer_colour(o & 0x3Fu), 1.0 - a);
    }
    return shown;
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

// Where the sun's shadows end, meters from the eye: past them the forest cover's crowns shade
// the ground under them.
const SHADOWS_END: f32 = 560.0;

// The ground's colour `albedo` at render-space `world`, `dist` meters from the eye, `to_eye`
// toward it, under the forest cover: shaded by its crowns past the sun's shadows, and laid with
// their colour as the far trees thin out and past them. Seen low across the land, the crowns
// hide more of the ground between them than they cover from above.
fn forest(albedo: vec3<f32>, world: vec3<f32>, dist: f32, to_eye: vec3<f32>) -> vec3<f32> {
    if (params.cover.x <= 0.0 && params.cover.w <= 0.0) {
        return albedo;
    }
    let q = (world.xy + params.drawn.yz) / CELL;
    let side = vec2<f32>(textureDimensions(cover_tex));
    let c = textureSampleLevel(cover_tex, clamp_samp, (q + vec2<f32>(0.5)) / side, 0.0);
    if (c.a <= 0.0) {
        return albedo;
    }
    let low = 1.0 - clamp(abs(to_eye.z) / max(dist, 1e-3), 0.0, 1.0);
    let seen = 1.0 - pow(1.0 - c.a, 1.0 + 2.0 * low);
    let shade = c.a * params.cover.w * smoothstep(SHADOWS_END - 60.0, SHADOWS_END + 60.0, dist);
    let laid = seen * params.cover.x * smoothstep(params.cover.y, params.cover.z, dist);
    return mix(albedo * (1.0 - shade), c.rgb, clamp(laid, 0.0, 1.0));
}

@fragment
fn fs_ground(@builtin(position) p: vec4<f32>) -> Out {
    let px = vec2<i32>(p.xy);
    let nw = textureLoad(prior, px, 0);
    // The landscape-normal pass's landscape, exactly: the retail landscape's class. Or, where the
    // presentation draws the ground itself, a pixel of it a cut-out edge was blended over (a
    // leaf's, a butterfly's wing's): the ground's share of it, read from the mark its distance
    // left there, is laid with the ground's material under the edge.
    var under_edge = 0.0;
    var code = vec4<f32>(0.0);
    if (params.drawn.x > 0.5) {
        code = textureLoad(places, px, 0);
        if (code.b > CODE_MARK * 0.99) {
            under_edge = ground_share(code);
            if (under_edge < 0.02) {
                discard;
            }
        }
    } else if (abs(nw.w - 3.0) > 0.01) {
        discard;
    }
    let d = textureLoad(depth, px, 0);
    if (d >= 1.0) {
        discard;
    }
    let world = placed(p.xy, d);
    let t = (world.xy - params.cells.xy) / CELL;
    let ci = vec2<i32>(floor(t));
    var words = vec4<u32>(0u);
    if (all(ci >= vec2<i32>(0)) && ci.x < i32(params.cells.z) && ci.y < i32(params.cells.w)) {
        words = textureLoad(cells, ci, 0);
    }
    // Past the cells laid out here, the ground the presentation draws itself is laid from the
    // world's own terrain words; anything else keeps the pictures it was drawn with.
    let laid = (words.x & COVERED) != 0u;
    let far_laid = !laid && params.drawn.x > 0.5;
    if (!laid && !far_laid) {
        discard;
    }
    let f = clamp(t - floor(t), vec2<f32>(0.0), vec2<f32>(1.0));
    // The drawn cell: `span` cells across, starting at a multiple of `span` (the window starts
    // at a block's corner, eight cells to a block).
    let span_i = select(1 << ((words.x >> 12u) & 3u), 1, far_laid);
    let span = f32(span_i);
    let cell_m = CELL * span;
    let drawn_cell = (ci / vec2<i32>(span_i)) * vec2<i32>(span_i);
    var fd = (vec2<f32>(ci - drawn_cell) + f) / span;
    var global = vec2<i32>(params.first_cell.xy) + ci;
    var gf = f;
    if (far_laid) {
        // The world's own cell, from where render space lies in the world.
        let gt = (world.xy + params.drawn.yz) / CELL;
        global = vec2<i32>(floor(gt));
        gf = clamp(gt - floor(gt), vec2<f32>(0.0), vec2<f32>(1.0));
        fd = gf;
    }
    // The drawn cell's own coordinates as retail's composite has them: east across, north up.
    let cell_uv = vec2<f32>(fd.x, 1.0 - fd.y);
    let p_cell = ((global % vec2<i32>(PERIOD_CELLS)) + vec2<i32>(PERIOD_CELLS)) % vec2<i32>(PERIOD_CELLS);
    let pos = (vec2<f32>(p_cell) + gf) * CELL;
    let to_eye = params.eye.xyz - world;
    let dist = length(to_eye);
    let n = normalize((transpose(params.render_to_view) * vec4<f32>(nw.xyz, 0.0)).xyz);
    // How far the ground moves from this pixel to the next, across and down; no farther than a
    // hundredth of the distance a pixel, where the ground runs nearly along the view.
    let cap = dist * 0.01 + 0.01;
    var dx = (on_plane(p.xy + vec2<f32>(1.0, 0.0), world, n) - world).xy;
    var dy = (on_plane(p.xy + vec2<f32>(0.0, 1.0), world, n) - world).xy;
    // The ground the pixel covers, meters, before the cap: variation finer than it is left out.
    let foot = max(length(dx), length(dy));
    dx = dx * min(1.0, cap / max(length(dx), 1e-6));
    dy = dy * min(1.0, cap / max(length(dy), 1e-6));
    let cell_dx = vec2<f32>(dx.x, -dx.y) / cell_m;
    let cell_dy = vec2<f32>(dy.x, -dy.y) / cell_m;

    let near = 1.0 - smoothstep(HEIGHTS_NEAR, HEIGHTS_FAR, dist);
    var at: Lay;
    at.words = words;
    at.far_laid = far_laid;
    at.gt = (world.xy + params.drawn.yz) / CELL;
    at.foot = foot;
    at.pos = pos;
    at.cell_uv = cell_uv;
    at.cell_dx = cell_dx;
    at.cell_dy = cell_dy;
    at.dx = dx;
    at.dy = dy;
    at.dist = dist;
    at.cell_m = cell_m;
    at.near = near;
    if (params.water.x > 0.5) {
        pixel_inside = shore_distance(world.xy + params.drawn.yz, foot);
        pixel_land = land_beside(at.gt);
    }
    var m = layered(at, 0u);
    // A muddy bank: grass grows in over the mud in patches, more the farther from the water's
    // edge, so the shore reads as wet mud at the water giving way to a grassy bank, not as a flat
    // ring of mud. The patches finer than a pixel are left out, as the ground's variation is.
    if (params.water.x > 0.5 && params.water.w > 0.0 && m.mud > 0.01 && pixel_inside < 0.0 && pixel_inside > -BANK_REACH) {
        let away = smoothstep(-1.0, -10.0, pixel_inside);
        let n = mix(0.5, noise(pos, 16.0, 96), resolved(16.0, foot)) * 0.6
            + mix(0.5, noise(pos + vec2<f32>(7.0, 3.0), 4.0, 384), resolved(4.0, foot)) * 0.4;
        let lo = 0.72 - 0.45 * away;
        let share = params.water.w * m.mud * away * smoothstep(lo, lo + 0.16, n);
        if (share > 0.002) {
            let grass = plain_ground(grass_beside(at.gt), pos, cell_uv, dx, dy, dist, cell_m);
            m = mix_layer(m, grass, height_blend(m.height, grass.height, share, near));
        }
    }
    // Scree and rock where the drawn ground is steep, over the pack's own materials (not over
    // retail's pictures, which are its water): each ground's own material for its steep slopes
    // where the pack has one, the generic scree and rock where it has none.
    if (params.slope2.x > 0.0) {
        // On walkable ground by the smooth surface's slope, as much as asked, so the materials
        // sweep over retail's facets; on unwalkable ground by the drawn ground's own, so the
        // rock bands' treads and risers take scree and rock apart.
        let q = (world.xy + params.drawn.yz) / CELL;
        let steep = 1.0 - smoothstep(WALKABLE_Z - 0.04, WALKABLE_Z + 0.04, retail_up(q));
        let up = mix(n.z, smooth_up(q), params.slope2.w * (1.0 - steep));
        let angle = degrees(acos(clamp(up, -1.0, 1.0)));
        let scree = smoothstep(params.slope.x, params.slope.x + max(params.slope.y, 0.1), angle);
        let rock = smoothstep(params.slope.z, params.slope.z + max(params.slope.w, 0.1), angle);
        let room = params.slope2.x * m.pack * mix(1.0, params.slope3.x, m.snow);
        if (scree * room > 0.002) {
            let s = layered(at, 1u);
            m = mix_layer(m, s, height_blend(m.height, s.height, scree * room, near));
        }
        if (room > 0.002 && (rock > 0.002 || steep > 0.002)) {
            let s = layered(at, 2u);
            // A ground's own steep material covers all of its unwalkable ground, the bands'
            // treads as well as their risers, so no pocket of the flat ground shows on a cliff;
            // the generic rock keeps to the risers.
            let cover = max(rock, steep * s.own) * room;
            if (cover > 0.002) {
                m = mix_layer(m, s, height_blend(m.height, s.height, cover, near));
            }
        }
    }
    // Gentle variation of the pack's colour, in patches a few metres across and over tens and
    // hundreds of metres, as the ground's own pictures vary from place to place.
    // Each is left out as it grows finer than the pixel, so the far ground holds still under
    // the anti-aliasing's jitter.
    let patches = (noise(pos, 4.0, 384) - 0.5) * 0.08 * resolved(4.0, foot)
        + (noise(pos + vec2<f32>(17.0, 5.0), 6.0, 256) - 0.5) * 0.06 * resolved(6.0, foot);
    let broad = (noise(pos, 32.0, 48) - 0.5) * 0.14 * resolved(32.0, foot)
        + (noise(pos + vec2<f32>(500.0), 128.0, 12) - 0.5) * 0.12 * resolved(128.0, foot);
    let vary = 1.0 + params.tune.y * (patches + broad);
    var albedo = clamp(m.albedo * mix(1.0, vary, m.pack), vec3<f32>(0.0), vec3<f32>(1.0));
    var roughness = m.roughness;
    if (params.water.x > 0.5) {
        // Wet ground: retail's water laid above the water's edge, and a strip of any ground along
        // it, darker and smoother, drying away from the water.
        let inside = pixel_inside;
        let damp = smoothstep(-3.0, 0.0, inside) * 0.6;
        let wet = clamp(max(m.wet * smoothstep(-SHORE_CELL, -2.0, inside), damp), 0.0, 1.0);
        albedo = albedo * (1.0 - params.water.z * wet);
        roughness = mix(roughness, 0.55, wet * 0.6);
    }
    if (params.tune.w > 0.5) {
        albedo = shown_colour(at);
    }
    albedo = forest(albedo, world, dist, to_eye);
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
    if (under_edge > 0.0) {
        // The pixel is the edge's own color where the edge covers a fair share of it, so it is
        // lit as the leaf it is, and the ground's material only where the ground is nearly all
        // of it.
        // The edge's own blue went into telling its share, so it is taken from the leaf beside
        // it that covers its pixel most, in proportion to the green.
        var own = edge_colour(code);
        var least = under_edge;
        var beside = vec3<f32>(0.0);
        for (var k = 0; k < 9; k = k + 1) {
            let q = px + vec2<i32>(k % 3 - 1, k / 3 - 1);
            let other = textureLoad(places, q, 0);
            let share = ground_share(other);
            if (share < least) {
                least = share;
                beside = edge_colour(other);
            }
        }
        if (least < 0.5 && beside.g > 1e-3) {
            own.b = own.g * beside.b / beside.g;
        } else {
            own.b = 0.5 * (own.r + own.g);
        }
        o.colour = vec4<f32>(mix(own, c, smoothstep(0.6, 0.95, under_edge)), 1.0);
        o.normal = nw;
        return o;
    }
    o.colour = vec4<f32>(c, 1.0);
    let view_n = normalize((params.render_to_view * vec4<f32>(bent, 0.0)).xyz);
    o.normal = vec4<f32>(view_n, 3.05 + 0.3 * clamp(roughness, 0.0, 1.0));
    return o;
}
