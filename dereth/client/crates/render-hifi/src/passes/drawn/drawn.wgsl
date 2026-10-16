// The drawn ground: the landscape drawn by the presentation itself, in place of the replayed
// landscape cells.
//
// The ground is a grid of square patches, each `CELLS` quads a side, finer near the eye and
// coarser away from it (each level twice the last's spacing). Every vertex stands at retail's
// height plus the offset: near the eye, the scene's own offset for the block (its tiles, so what
// stands on the ground and the ground agree); far off, the smooth surface through retail's
// vertices with an eroded relief over it; between, the two mixed by the distance from the eye.
// Close by, a fine relief and, when asked for, small terraces are laid over it, leveled under each
// body. Toward the outer edge of each level's ring a vertex slides onto the next level's grid, so
// patches of neighboring levels meet without cracks.
//
// Every quad is cut along the diagonal of the retail cell it lies in, so at the finest spacing
// retail's own planes are reproduced exactly where the offset is nought.
//
// Each pixel takes the normal of the drawn surface there (finite differences of the drawn
// height, so the bands and the fine relief shade at any distance), leaned toward the smooth
// surface's where the ground is pinned, as the replayed landscape's would be; and the landscape's
// material class, so the art pack's ground lays its materials over it.

struct Frame {
    // Render space (height up) to clip, at the frame's jitter.
    clip_from_render: mat4x4<f32>,
    // Render space (height up) to view space.
    view_from_render: mat4x4<f32>,
    // xyz the eye, render space.
    eye: vec4<f32>,
    // xy where render space's origin lies in the world, meters; z the near texture's blocks a
    // side; w how far from the eye the ground is drawn, meters, short of the far plane.
    global: vec4<f32>,
    // x the fine relief's height, y its size; z the level round's radius under a body (nought for
    // none), w its fade.
    k0: vec4<f32>,
    // x a terrace's height (nought for none), y its cap, z the slope it starts at (degrees),
    // w the share of the rounding kept far off.
    k1: vec4<f32>,
    // x, y where the near ground starts and ends giving way to the far; z 1 for ground to the
    // horizon; w the far relief's height.
    k2: vec4<f32>,
    // x the far relief's size; y how far the ground is drawn; z the diagnostic view; w how many
    // bodies.
    k3: vec4<f32>,
    // The bodies the fine relief is leveled under, render space.
    bodies: array<vec4<f32>, 32>,
    // The block each slot of the near texture holds, `x * 256 + y`, or -1: four slots a vector.
    slots: array<vec4<i32>, 64>,
    // x how many of `stamped` hold a cell; y how many meters the forest cover's whole canopy
    // raises the far ground (nought for none); z and w where it rises in, meters from the eye.
    k4: vec4<f32>,
    // The landscape cells whose buildings stamped their openings this frame, world meters: xy the
    // south-west corner, z the side.
    stamped: array<vec4<f32>, 64>,
    // The water: x 1 while it is drawn as water, over a bed lowered under it; y the deepest the
    // bed lies, z how far from the shore it reaches that depth, meters; w unused.
    water: vec4<f32>,
};

struct Instance {
    // xy the patch's south-west corner, world meters; z its spacing; w its level.
    @location(0) at: vec4<f32>,
    // x where its vertices start sliding onto the next level's grid, y where they have, meters
    // from the eye.
    @location(1) morph: vec4<f32>,
};

struct VsOut {
    @builtin(position) pos: vec4<f32>,
    // xy world meters, z the drawn height, w the distance from the eye.
    @location(0) world: vec4<f32>,
    // How much of the fine relief the mesh carries there: all of it inside the finest level's
    // ring, none where that ring meets the next level.
    @location(1) fine_share: f32,
};

@group(0) @binding(0) var<uniform> frame: Frame;
// Each landscape vertex's height, meters.
@group(0) @binding(1) var heights: texture_2d<f32>;
// Each landscape vertex's terrain word.
@group(0) @binding(2) var words: texture_2d<u32>;
// The near offset, a sample a meter: each slot a block, 192 samples a side, x the offset bodies
// stand on and y how much of it the pins leave.
@group(0) @binding(3) var near_tex: texture_2d<f32>;
// Where the water lies, a value pair each landscape vertex (see the shore's arithmetic).
@group(0) @binding(4) var water_field: texture_2d<f32>;
// The forest cover's canopy at each landscape vertex: the share its crowns cover times their
// height, a step a quarter meter, as a share of 255 steps.
@group(0) @binding(5) var canopy_tex: texture_2d<f32>;

const CELLS: u32 = 16u;
const CELL: f32 = 24.0;
const BLOCK: f32 = 192.0;
const SIDE: i32 = 2041;
const WALKABLE_Z: f32 = 0.6641741;

fn vertex_height(g: vec2<i32>) -> f32 {
    return textureLoad(heights, clamp(g, vec2<i32>(0), vec2<i32>(SIDE - 1)), 0).r;
}

fn vertex_word(g: vec2<i32>) -> u32 {
    return textureLoad(words, clamp(g, vec2<i32>(0), vec2<i32>(SIDE - 1)), 0).r;
}

// Whether the cell's shared edge runs south-west to north-east: retail's hash of the cell.
fn sw_to_ne(c: vec2<i32>) -> bool {
    let x = bitcast<u32>(c.x);
    let y = bitcast<u32>(c.y);
    let v = (x * 0x0CCAC033u + 0x6C1AC587u) * y + x * 0xBDE41C43u + 0xAE6470DBu;
    return (v >> 31u) != 0u;
}

// Retail's height at world `p`, and its triangle's rise east and north.
fn retail(p: vec2<f32>) -> vec3<f32> {
    let q = p / CELL;
    let c = vec2<i32>(floor(q));
    let f = clamp(q - vec2<f32>(c), vec2<f32>(0.0), vec2<f32>(1.0));
    let h00 = vertex_height(c);
    let h10 = vertex_height(c + vec2<i32>(1, 0));
    let h01 = vertex_height(c + vec2<i32>(0, 1));
    let h11 = vertex_height(c + vec2<i32>(1, 1));
    if (sw_to_ne(c)) {
        if (f.x >= f.y) {
            return vec3<f32>(h00 + f.x * (h10 - h00) + f.y * (h11 - h10), (h10 - h00) / CELL, (h11 - h10) / CELL);
        }
        return vec3<f32>(h00 + f.y * (h01 - h00) + f.x * (h11 - h01), (h11 - h01) / CELL, (h01 - h00) / CELL);
    }
    if (f.x + f.y <= 1.0) {
        return vec3<f32>(h00 + f.x * (h10 - h00) + f.y * (h01 - h00), (h10 - h00) / CELL, (h01 - h00) / CELL);
    }
    return vec3<f32>(h11 + (1.0 - f.x) * (h01 - h11) + (1.0 - f.y) * (h10 - h11), (h11 - h01) / CELL, (h11 - h10) / CELL);
}

fn catmull(t: f32) -> vec4<f32> {
    let t2 = t * t;
    let t3 = t2 * t;
    return 0.5 * vec4<f32>(-t3 + 2.0 * t2 - t, 3.0 * t3 - 5.0 * t2 + 2.0, -3.0 * t3 + 4.0 * t2 + t, t3 - t2);
}

// The smooth surface through retail's vertices at world `p`.
fn smooth_height(p: vec2<f32>) -> f32 {
    let q = p / CELL;
    let c = vec2<i32>(floor(q));
    let f = clamp(q - vec2<f32>(c), vec2<f32>(0.0), vec2<f32>(1.0));
    let wx = catmull(f.x);
    let wy = catmull(f.y);
    var h = 0.0;
    for (var i = 0; i < 4; i = i + 1) {
        var row = 0.0;
        for (var j = 0; j < 4; j = j + 1) {
            row = row + wy[j] * vertex_height(c + vec2<i32>(i - 1, j - 1));
        }
        h = h + wx[i] * row;
    }
    return h;
}

// A lattice point's hash, 0..1: the scene's own arithmetic.
fn lattice_hash(p: vec2<i32>) -> f32 {
    let h0 = (bitcast<u32>(p.x) * 0x8DA6B343u) ^ (bitcast<u32>(p.y) * 0xD8163841u);
    let h1 = (h0 ^ (h0 >> 13u)) * 0x2C1B3C6Du;
    let h2 = h1 ^ (h1 >> 15u);
    return f32(h2 & 0xFFFFu) / 65535.0;
}

// Smooth noise over a lattice `cell` meters wide, 0..1: the scene's own arithmetic.
fn value_noise(p: vec2<f32>, cell: f32) -> f32 {
    let q = p / cell;
    let i = vec2<i32>(floor(q));
    let f = q - floor(q);
    let s = f * f * (3.0 - 2.0 * f);
    let a = lattice_hash(i);
    let b = lattice_hash(i + vec2<i32>(1, 0));
    let c = lattice_hash(i + vec2<i32>(0, 1));
    let d = lattice_hash(i + vec2<i32>(1, 1));
    let south = a + (b - a) * s.x;
    let north = c + (d - c) * s.x;
    return south + (north - south) * s.y;
}

// One octave of the far relief: ridged noise `cell` meters wide, moved `offset` meters.
fn ridge_octave(p: vec2<f32>, offset: f32, cell: f32) -> f32 {
    let n = value_noise(vec2<f32>(p.x + offset, p.y - offset), cell);
    let ridge = 1.0 - abs(2.0 * n - 1.0);
    return ridge * ridge - 0.45;
}

// The far ground's eroded relief at world `p` on ground rising `slope`: the scene's own
// arithmetic, three octaves of ridged noise, each half the last's size and weight.
fn far_relief(p: vec2<f32>, scale: f32, amplitude: f32, slope: f32) -> f32 {
    return far_relief_seen(p, scale, amplitude, slope, 0.0);
}

// How much of a detail `cell` meters across shows in a pixel that covers `foot` meters of
// ground: all of it while the pixel is a quarter of it or less, none once the pixel is half of
// it, so detail finer than the pixels can hold is left out rather than sampled.
fn resolved(cell: f32, foot: f32) -> f32 {
    return 1.0 - smoothstep(0.25 * cell, 0.5 * cell, foot);
}

// The far relief as a pixel covering `foot` meters of ground sees it: each octave left out as it
// grows finer than the pixel.
fn far_relief_seen(p: vec2<f32>, scale: f32, amplitude: f32, slope: f32, foot: f32) -> f32 {
    if (amplitude <= 0.0) {
        return 0.0;
    }
    let cell = max(scale, 1.0);
    let sum = ridge_octave(p, 0.0, cell) * resolved(cell, foot)
        + 0.5 * ridge_octave(p, 37.0, cell * 0.5) * resolved(cell * 0.5, foot)
        + 0.25 * ridge_octave(p, 74.0, cell * 0.25) * resolved(cell * 0.25, foot);
    return amplitude * sum / 1.75 * (0.3 + min(slope, 1.2));
}

// The near texture's sample at world meter `g`: the offset and the pins' weight, or nought and
// whole where no slot holds the block.
fn near_sample(g: vec2<i32>) -> vec3<f32> {
    let n = i32(frame.global.z);
    let block = vec2<i32>(floor(vec2<f32>(g) / BLOCK));
    let slot = ((block % vec2<i32>(n)) + vec2<i32>(n)) % vec2<i32>(n);
    let k = slot.y * n + slot.x;
    let held = frame.slots[k / 4][k % 4];
    if (held != block.x * 256 + block.y) {
        return vec3<f32>(0.0, 1.0, 0.0);
    }
    let local = g - block * 192;
    let v = textureLoad(near_tex, slot * 192 + local, 0).rg;
    return vec3<f32>(v, 1.0);
}

// The near offset and the pins' weight at world `p`, between the samples; z whether any sample
// round it is held.
fn near_at(p: vec2<f32>) -> vec3<f32> {
    let i = vec2<i32>(floor(p));
    let f = p - floor(p);
    let a = near_sample(i);
    let b = near_sample(i + vec2<i32>(1, 0));
    let c = near_sample(i + vec2<i32>(0, 1));
    let d = near_sample(i + vec2<i32>(1, 1));
    let south = mix(a, b, f.x);
    let north = mix(c, d, f.x);
    return mix(south, north, f.y);
}

// The forest cover's canopy at world `p`: how high it raises the ground, meters, between the
// vertices, as far as it has risen in `dist` meters from the eye.
fn canopy(p: vec2<f32>, dist: f32) -> f32 {
    if (frame.k4.y <= 0.0) {
        return 0.0;
    }
    let rise = smoothstep(frame.k4.z, max(frame.k4.w, frame.k4.z + 1.0), dist);
    if (rise <= 0.0) {
        return 0.0;
    }
    let q = p / CELL;
    let c = vec2<i32>(floor(q));
    let f = q - floor(q);
    let hi = vec2<i32>(textureDimensions(canopy_tex)) - vec2<i32>(1);
    let a = textureLoad(canopy_tex, clamp(c, vec2<i32>(0), hi), 0).r;
    let b = textureLoad(canopy_tex, clamp(c + vec2<i32>(1, 0), vec2<i32>(0), hi), 0).r;
    let d = textureLoad(canopy_tex, clamp(c + vec2<i32>(0, 1), vec2<i32>(0), hi), 0).r;
    let e = textureLoad(canopy_tex, clamp(c + vec2<i32>(1, 1), vec2<i32>(0), hi), 0).r;
    let v = mix(mix(a, b, f.x), mix(d, e, f.x), f.y);
    return v * frame.k4.y * rise;
}

// How far `dist` meters from the eye has given way to the far ground: 0 near, 1 far.
fn morph_far(dist: f32) -> f32 {
    if (frame.k2.z < 0.5) {
        return 0.0;
    }
    return smoothstep(frame.k2.x, max(frame.k2.y, frame.k2.x + 1e-3), dist);
}

// How much of the fine relief and the terraces is left at world `p`: none under a body, all of it
// a level round's fade away.
fn unleveled(p: vec2<f32>) -> f32 {
    if (frame.k0.z <= 0.0) {
        return 1.0;
    }
    var keep = 1.0;
    let count = i32(frame.k3.w);
    for (var k = 0; k < count; k = k + 1) {
        let b = frame.bodies[k].xy + frame.global.xy;
        let d = length(p - b);
        keep = min(keep, smoothstep(frame.k0.z, frame.k0.z + max(frame.k0.w, 1e-3), d));
    }
    return keep;
}

// The fine relief and the terraces at world `p` on retail ground `r` (height, rise east and
// north), `pinned` how much the pins leave, `far` how far it has given way to the far ground, as
// a pixel covering `foot` meters of ground sees them (nought for all of them).
fn fine(p: vec2<f32>, r: vec3<f32>, pinned: f32, far: f32, foot: f32) -> f32 {
    let room = pinned * (1.0 - far);
    if (room <= 0.0) {
        return 0.0;
    }
    var h = 0.0;
    let micro = frame.k0.x;
    if (micro > 0.0) {
        let s = max(frame.k0.y, 0.05);
        let n = (value_noise(p, s) - 0.5) * 0.65 * resolved(s, foot)
            + (value_noise(p + vec2<f32>(11.3, 7.1), s * 0.45) - 0.5) * 0.35 * resolved(s * 0.45, foot);
        h = h + n * 2.0 * micro;
    }
    let band = frame.k1.x;
    if (band > 0.0) {
        let slope = length(r.yz);
        let nz = inverseSqrt(1.0 + slope * slope);
        // Walkable ground steeper than the terraces' slope.
        let start = tan(radians(frame.k1.z));
        let steep = smoothstep(start, tan(radians(frame.k1.z + 5.0)), slope) * select(0.0, 1.0, nz >= WALKABLE_Z);
        if (steep > 0.0) {
            let q = r.x / band;
            let f = q - floor(q);
            let riser = 0.35;
            let off = (smoothstep(1.0 - riser, 1.0, f) - f + (1.0 - riser) * 0.5) * band;
            h = h + clamp(off, -frame.k1.y, frame.k1.y) * steep * resolved(band, foot);
        }
    }
    return h * room * unleveled(p);
}

// The drawn ground's height at world `p`, `dist` meters from the eye, and its parts: x the
// height, y the pins' weight, z retail's height, w how far it has given way to the far ground.
// `fine_share` is how much of the fine relief is laid over it; `foot` the ground a pixel covers
// there, meters, which leaves out the detail finer than it (nought for all of it).
fn drawn(p: vec2<f32>, dist: f32, fine_share: f32, foot: f32) -> vec4<f32> {
    let r = retail(p);
    let far = morph_far(dist);
    let near = near_at(p);
    var offset = near.x;
    // Under the water the bed lies below retail's surface, which the water is drawn at; far off,
    // the rounding and the eroded relief keep out of the water and its shore, as the pins keep
    // the near offset out, so the shore stands at retail's height and no island rises in a lake.
    var inside = -1000.0;
    if (frame.water.x > 0.5) {
        inside = shore_distance(p, foot);
    }
    if (far > 0.0) {
        let rounded = smooth_height(p);
        let dry = 1.0 - smoothstep(-SHORE_CELL, 0.0, inside);
        let far_offset = (frame.k1.w * (rounded - r.x)
            + far_relief_seen(p, frame.k3.x, frame.k2.w, length(r.yz), foot)) * dry;
        offset = mix(offset, far_offset, far);
    }
    // The forest cover's canopy far off, kept out of the water and its shore.
    var h = r.x + offset - bed_depth(inside, frame.water.y, frame.water.z)
        + canopy(p, dist) * (1.0 - smoothstep(-SHORE_CELL, 0.0, inside));
    if (fine_share > 0.0) {
        h = h + fine(p, r, near.y, far, foot) * fine_share;
    }
    // Beside the water the land never lies below the water's level at its edge, and falls away
    // below it only gently going out: a hollow under the water's level by the shore would read as
    // land the water should have flooded.
    if (inside <= 0.0 && inside > -SHORE_NEAR) {
        h = max(h, shore_floor(level_at(p), inside));
    }
    return vec4<f32>(h, near.y, r.x, far);
}

// How far a patch vertex at world `p` has slid onto the next level's grid: nought inside its
// level's ring, whole at the ring's edge; `morph` where the slide starts and ends, meters across
// the ground from the eye at `eye`. Across the ground, as the patches are chosen, so a level
// meets the next only where its own vertices have slid all the way and the next level's have not
// begun to.
fn slid(p: vec2<f32>, morph: vec2<f32>, eye: vec2<f32>) -> f32 {
    let d = length(p - eye);
    return clamp((d - morph.x) / max(morph.y - morph.x, 1e-3), 0.0, 1.0);
}

// Where the vertex `grid` quads from its patch's corner, at world `p` on a grid `spacing` meters
// apart, stands `k` of the way slid: each odd vertex moves onto its even neighbor to the south or
// west, the next level's grid.
fn slide(p: vec2<f32>, grid: vec2<f32>, spacing: f32, k: f32) -> vec2<f32> {
    let odd = fract(grid * 0.5) * 2.0;
    return p - odd * spacing * k;
}

// How much of the fine relief a vertex of a patch of level `level` carries, slid `k` of the way
// onto the next level's grid: the fine relief is the finest level's alone, and fades out as its
// vertices slide, so it is gone where the finest level meets the next and the two meet without a
// crack.
fn fine_share_of(level: f32, k: f32) -> f32 {
    return select(0.0, 1.0 - k, level < 0.5);
}

// A patch vertex's world position, its level's grid slid onto the next level's toward the ring's
// edge; its distance from the eye; and how far it has slid.
fn patch_vertex(vi: u32, inst: Instance) -> vec4<f32> {
    let quad = vi / 6u;
    let corner = vi % 6u;
    let qx = quad % CELLS;
    let qy = quad / CELLS;
    let spacing = inst.at.z;
    let center = inst.at.xy + (vec2<f32>(f32(qx), f32(qy)) + 0.5) * spacing;
    let cut = sw_to_ne(vec2<i32>(floor(center / CELL)));
    // The quad's two triangles, cut as the retail cell it lies in is.
    var ne = array<vec2<u32>, 6>(
        vec2<u32>(0u, 0u), vec2<u32>(1u, 0u), vec2<u32>(1u, 1u),
        vec2<u32>(0u, 0u), vec2<u32>(1u, 1u), vec2<u32>(0u, 1u),
    );
    var nw = array<vec2<u32>, 6>(
        vec2<u32>(0u, 0u), vec2<u32>(1u, 0u), vec2<u32>(0u, 1u),
        vec2<u32>(1u, 0u), vec2<u32>(1u, 1u), vec2<u32>(0u, 1u),
    );
    let c = select(nw[corner], ne[corner], cut);
    let grid = vec2<f32>(f32(qx + c.x), f32(qy + c.y));
    let at = inst.at.xy + grid * spacing;
    let eye = frame.eye.xyz + vec3<f32>(frame.global.xy, 0.0);
    let k = slid(at, inst.morph.xy, eye.xy);
    let p = slide(at, grid, spacing, k);
    let dist = length(vec3<f32>(p - eye.xy, retail(p).x - eye.z));
    return vec4<f32>(p, dist, k);
}

@vertex
fn vs_ground(@builtin(vertex_index) vi: u32, inst: Instance) -> VsOut {
    let v = patch_vertex(vi, inst);
    let fine_share = fine_share_of(inst.at.w, v.w);
    let h = drawn(v.xy, v.z, fine_share, 0.0).x;
    let render = vec3<f32>(v.xy - frame.global.xy, h);
    var o: VsOut;
    o.pos = frame.clip_from_render * vec4<f32>(render, 1.0);
    o.world = vec4<f32>(v.xy, h, v.z);
    o.fine_share = fine_share;
    return o;
}

struct Out {
    @location(0) color: vec4<f32>,
    @location(1) normal: vec4<f32>,
};

// The drawn surface's normal at world `p`, where it stands at `h`, by differences `e` meters on,
// with `fine_share` of the fine relief, as a pixel covering `foot` meters sees it.
fn drawn_normal(p: vec2<f32>, dist: f32, e: f32, h: f32, fine_share: f32, foot: f32) -> vec3<f32> {
    let hx = drawn(p + vec2<f32>(e, 0.0), dist, fine_share, foot).x - h;
    let hy = drawn(p + vec2<f32>(0.0, e), dist, fine_share, foot).x - h;
    return normalize(vec3<f32>(-hx / e, -hy / e, 1.0));
}

// The smooth surface's normal at world `p`, by differences `e` meters on.
fn smooth_normal(p: vec2<f32>, e: f32) -> vec3<f32> {
    let h = smooth_height(p);
    let hx = smooth_height(p + vec2<f32>(e, 0.0)) - h;
    let hy = smooth_height(p + vec2<f32>(0.0, e)) - h;
    return normalize(vec3<f32>(-hx / e, -hy / e, 1.0));
}

// The landscape's material class, as the art pack's ground looks for it.
const TERRAIN_CLASS: f32 = 3.0;
// The scale of the distance written in place of a color, and the blue that marks it (the art
// pack's ground reads them with its own copies).
const CODE_SCALE: f32 = 4096.0;
const CODE_WHOLE_SCALE: f32 = 65536.0;
const CODE_MARK: f32 = -4096.0;

// The distance `exact` meters from the eye, written in place of a color: the whole 64s scaled
// further down than the rest, each still read back exactly.
fn distance_code(exact: f32) -> vec4<f32> {
    let whole = floor(exact / 64.0);
    return vec4<f32>(whole / CODE_WHOLE_SCALE, (exact - whole * 64.0) / CODE_SCALE, CODE_MARK, 1.0);
}

// The ground a fragment at world `p` covers: the shape finer than it is left out, and the normal
// is the slope across it, so nothing finer than a pixel is sampled and the picture holds still
// under the anti-aliasing's jitter.
fn footprint(p: vec2<f32>) -> f32 {
    return max(length(dpdx(p)), length(dpdy(p)));
}

@fragment
fn fs_ground(in: VsOut) -> Out {
    return shade(in, footprint(in.world.xy));
}

// How much of the offset the pins may leave for the ground to count as a building's own, kept
// at retail's height round it.
const PINNED: f32 = 0.05;

// The ground drawn again once the buildings have stamped their openings into the depth, over
// whatever lies behind it: a stamp shows a building's rooms through the ground drawn before it,
// which the drawn ground, drawn whole at the first cell, would otherwise be everywhere. Inside a
// cell whose building stamped, the ground kept at retail's height round its buildings is not
// drawn again: there, as retail draws it, the building's stamp erases its own ground over the
// doorway. The rest of such a cell (a hill the viewer stands on beside a house) is drawn again,
// so no other building's doorway shows through it. Far off, where no near offset is held to tell
// the buildings' ground apart, the whole cell is left as the stamp left it.
@fragment
fn fs_ground_over(in: VsOut) -> Out {
    let foot = footprint(in.world.xy);
    let count = i32(frame.k4.x);
    for (var k = 0; k < count; k = k + 1) {
        let r = frame.stamped[k];
        if (all(in.world.xy >= r.xy) && all(in.world.xy < r.xy + r.zz)) {
            let near = near_at(in.world.xy);
            if (near.z < 0.5 || near.y < PINNED) {
                discard;
            }
        }
    }
    return shade(in, foot);
}

// How far from the eye the finest level of the mesh holds before its vertices start sliding
// onto the next level's grid, meters, at the usual detail.
const FINEST_HELD: f32 = 60.0;

// How far above the drawn surface the point `t` meters along the line `r` from `eye` lies,
// meters: negative below it.
fn over_surface(eye: vec3<f32>, r: vec3<f32>, t: f32, dist: f32, fine_share: f32, foot: f32) -> f32 {
    let q = eye + r * t;
    return q.z - drawn(q.xy, dist, fine_share, foot).x;
}

// How many steps the line of sight is followed in across the stretch the mesh may lie off the
// surface, and how many times the step it meets the surface in is halved.
const SURFACE_STEPS: i32 = 6;
const SURFACE_HALVINGS: i32 = 2;

// Where the line of sight through the mesh at world `at` (east, north and its height) meets the
// drawn surface itself, on steep ground: the mesh only follows the surface as closely as its
// spacing allows, and its vertices slide from one level's grid to the next's as the eye moves,
// so on a cliff's rock bands the point the mesh puts under a pixel moves over the rock, and the
// rock's pictures with it. The surface itself stays put: the line is followed either side of the
// mesh, as far as the mesh may lie off the surface, the step where it passes under the surface
// is halved, and within the last, all but flat, the meeting is drawn straight between its ends.
// Elsewhere, or where the line does not cross the surface there, the mesh's own point.
fn on_surface(at: vec3<f32>, dist: f32, fine_share: f32, foot: f32) -> vec3<f32> {
    // Only where the mesh's levels slide: the finest level, within the first few dozen meters,
    // carries the near offset sample for sample, and there the surface's own point is the mesh's
    // (and following the line there would only find, under the jitter, now one band's corner and
    // now the next).
    if (dist < FINEST_HELD) {
        return at;
    }
    let eye = frame.eye.xyz + vec3<f32>(frame.global.xy, 0.0);
    let to = at - eye;
    let len = length(to);
    let r = to / max(len, 1e-4);
    // Steep ground there or a few meters off along the line (the brink of a face, where the
    // mesh's point may lie on the flat above it and the surface's on the face).
    let along = r.xy / max(length(r.xy), 1e-4) * 4.0;
    let slope = max(length(retail(at.xy).yz),
        max(length(retail(at.xy - along).yz), length(retail(at.xy + along).yz)));
    if (slope < 0.58) {
        return at;
    }
    let reach = clamp(dist * 0.025, 0.3, 6.0);
    let start = max(len - reach, 0.0);
    var lo = start;
    var hi = len + reach;
    var over_lo = over_surface(eye, r, lo, dist, fine_share, foot);
    if (over_lo <= 0.0) {
        return at;
    }
    // The first meeting along the line from there.
    var over_hi = 0.0;
    var met = false;
    for (var k = 1; k <= SURFACE_STEPS; k = k + 1) {
        let t = mix(start, len + reach, f32(k) / f32(SURFACE_STEPS));
        let over = over_surface(eye, r, t, dist, fine_share, foot);
        if (over <= 0.0) {
            hi = t;
            over_hi = over;
            met = true;
            break;
        }
        lo = t;
        over_lo = over;
    }
    if (!met) {
        return at;
    }
    for (var k = 0; k < SURFACE_HALVINGS; k = k + 1) {
        let mid = 0.5 * (lo + hi);
        let over = over_surface(eye, r, mid, dist, fine_share, foot);
        if (over > 0.0) {
            lo = mid;
            over_lo = over;
        } else {
            hi = mid;
            over_hi = over;
        }
    }
    return eye + r * (lo + (hi - lo) * over_lo / max(over_lo - over_hi, 1e-6));
}

// The drawn ground's outputs for the fragment `in`, covering `foot` meters of ground.
fn shade(in: VsOut, foot: f32) -> Out {
    let dist = in.world.w;
    // Short of the far plane: its last stretch would hold depths the passes after this one take
    // for the sky, so they would leave it unlit and unhazed, a dark sliver at the horizon.
    if (dist > frame.global.w) {
        discard;
    }
    // The surface's own point under the pixel, which its pictures and its normal are laid at.
    let at = on_surface(in.world.xyz, dist, in.fine_share, foot);
    let p = at.xy;
    let parts = drawn(p, dist, in.fine_share, foot);
    let e = max(clamp(dist * 0.002, 0.1, 6.0), foot);
    var n = drawn_normal(p, dist, e, parts.x, in.fine_share, foot);
    // Where the ground is pinned it is retail's, which is shaded with the smooth surface's
    // normal as the replayed landscape is.
    let drawn_share = mix(parts.y, 1.0, parts.w);
    if (drawn_share < 0.999) {
        n = normalize(mix(smooth_normal(p, max(e, 0.5)), n, drawn_share));
    }
    var o: Out;
    let view = u32(frame.k3.z + 0.5);
    if (view != 0u) {
        // A diagnostic view, drawn as it is: no class, so no light takes it.
        let r = retail(p);
        var c = vec3<f32>(0.5);
        if (view == 1u) {
            let off = clamp(parts.x - parts.z, -1.0, 1.0);
            c = mix(vec3<f32>(0.5), select(vec3<f32>(0.1, 0.25, 1.0), vec3<f32>(1.0, 0.15, 0.1), off > 0.0), abs(off));
        } else if (view == 2u) {
            c = mix(vec3<f32>(1.0, 0.1, 0.6), vec3<f32>(0.85), parts.y);
        } else if (view == 3u) {
            // Retail's slope as the study's map draws it: pale under 20 degrees, olive to 30,
            // yellow to 40, orange to the walkable limit, magenta past it.
            let deg = degrees(atan(length(r.yz)));
            c = vec3<f32>(0.85, 0.85, 0.8);
            if (deg >= 20.0) { c = vec3<f32>(0.55, 0.6, 0.2); }
            if (deg >= 30.0) { c = vec3<f32>(0.95, 0.85, 0.1); }
            if (deg >= 40.0) { c = vec3<f32>(0.95, 0.5, 0.1); }
            if (inverseSqrt(1.0 + dot(r.yz, r.yz)) < WALKABLE_Z) { c = vec3<f32>(0.95, 0.15, 0.6); }
        } else if (view == 4u) {
            let nz = inverseSqrt(1.0 + dot(r.yz, r.yz));
            c = select(vec3<f32>(0.95, 0.15, 0.6), vec3<f32>(0.6, 0.8, 0.5), nz >= WALKABLE_Z);
        }
        let shade = 0.55 + 0.45 * max(n.z, 0.0);
        o.color = vec4<f32>(c * c * shade, 1.0);
        o.normal = vec4<f32>(0.0);
        return o;
    }
    // In place of a color, which the art pack's ground always lays over, where the pixel lies:
    // its distance from the eye, in meters, whole 64s in red and the rest in green, both scaled
    // down, marked by a large blue no color has. Far off the depth places a pixel only to tens or
    // hundreds of meters, and the ground laid from that place would change from one frame's
    // jitter to the next. Scaled down, a leaf's cut-out edge blended over the ground (a fir's, a
    // butterfly's) carries under two hundredths of the distance into its color, where whole it came
    // out a glaring green; and the mark tells the ground's share under such an edge.
    let eye = frame.eye.xyz + vec3<f32>(frame.global.xy, 0.0);
    o.color = distance_code(length(at - eye));
    let view_n = normalize((frame.view_from_render * vec4<f32>(n, 0.0)).xyz);
    o.normal = vec4<f32>(view_n, TERRAIN_CLASS);
    return o;
}

// The shadow cascades: the drawn ground as seen from the sun, without the fine relief.
struct Caster {
    light_from_render: mat4x4<f32>,
};
@group(1) @binding(0) var<uniform> caster: Caster;

@vertex
fn vs_caster(@builtin(vertex_index) vi: u32, inst: Instance) -> @builtin(position) vec4<f32> {
    let v = patch_vertex(vi, inst);
    let h = drawn(v.xy, v.z, 0.0, 0.0).x;
    return caster.light_from_render * vec4<f32>(v.xy - frame.global.xy, h, 1.0);
}
