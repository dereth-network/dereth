// The water near the eye, simulated: a square of the surface a few tens of meters across, its
// height in each small cell stepped as waves run, so what disturbs the water (a body moving through
// it) spreads, crosses, meets the shore and dies away as waves do.
//
// The waves run as water's do, the longer the faster: each cell's height is pulled by its
// neighbors' a few cells about it through `KERNEL`, the water's vertical derivative (Tessendorf's
// iWave), so a disturbance spreads into a train of ripples rather than one sharp ring, and a body
// moving leaves a V of them behind it. A group of cells reads the square about it once into its
// own memory.
//
// The square is stored wrapped: a cell of the world is always at the same texel, its world cell
// modulo the side, so the square follows the eye a cell at a time without moving what it holds;
// a cell new to the square starts calm. Each step reads one picture (red the height now, green the
// height a step ago, meters; blue how sheltered the water is by a body standing in it, alpha the
// line where the water meets its legs) and writes the next. Land holds the water level, so waves
// turn back from the shore; toward the square's edges the waves are taken up, so none comes back
// from there.
//
// A body standing in the water is never quite still: it sways, and the water it pushes aside
// rises and falls a little with it, so faint rings spread from it as from any body standing in
// water; and it shelters the water about it from the wind's ripples. A body moving has weight: it
// heaps the water up ahead of it and draws it down behind, the faster the more, so a bow wave runs
// before it.

struct Step {
    // xy the square's first cell, world cells; zw the first cell of the square the last step held.
    origin: vec4<i32>,
    // xy the first cell's south-west corner in the world, meters; zw the same in render space.
    place: vec4<f32>,
    // x gravity times a step squared, in cells; y how much of its motion the water loses a step;
    // zw unused.
    wave: vec4<f32>,
    // x the step's count, for the bodies' sway; y how many bodies stand in `start` and `end`.
    misc: vec4<u32>,
    // Each body at the step's start and end: xy where it stands, render space; z its feet's
    // height; w how strongly it stirs the water.
    start: array<vec4<f32>, 8>,
    end: array<vec4<f32>, 8>,
};

@group(0) @binding(0) var<uniform> S: Step;
@group(0) @binding(1) var state: texture_2d<f32>;
@group(0) @binding(2) var next: texture_storage_2d<rgba16float, write>;
// Each landscape vertex's height, meters.
@group(0) @binding(3) var heights: texture_2d<f32>;
// Where the water lies, a value pair each landscape vertex.
@group(0) @binding(4) var water_field: texture_2d<f32>;

// The square's side, cells, and a cell's side, meters.
const SIDE: i32 = 256;
const CELL: f32 = 0.08;
// The cells a group steps, each way, and how far about a cell the kernel reaches.
const GROUP: i32 = 16;
const REACH: i32 = 5;
const TILE: i32 = 26;
// How many cells in from the square's edge the waves are taken up over.
const SPONGE: f32 = 14.0;
// A body's legs, as the water feels them: how far out from where it stands, meters, and how deep
// they push the water at the most.
const LEGS: f32 = 0.35;
const PUSH: f32 = 0.1;
// How much of the water a body pushes aside rises and falls as it sways, and how many times over
// ten seconds, in two motions that drift in and out of step; how far about it the water is
// sheltered, meters.
const SWAY: f32 = 0.45;
const SWAYS: f32 = 9.0;
const SWAYS_TOO: f32 = 13.0;
const SHELTER: f32 = 0.9;
// A moving body's bow: how far ahead of and behind where it stands the water is heaped and drawn
// down, meters; how wide the heap is, meters; how much it heaps a step at a run, meters; and the
// speed that counts as a run, meters a second.
const BOW_OFF: f32 = 0.38;
const BOW_WIDTH: f32 = 0.28;
const BOW_RISE: f32 = 0.008;
const RUN: f32 = 4.0;
// A step, seconds.
const STEP_SECONDS: f32 = 1.0 / 60.0;

var<workgroup> tile: array<f32, 676>;

fn vertex_height(g: vec2<i32>) -> f32 {
    return textureLoad(heights, clamp(g, vec2<i32>(0), vec2<i32>(SHORE_SIDE - 1)), 0).r;
}

// Whether the cell's shared edge runs south-west to north-east: retail's hash of the cell.
fn sw_to_ne(c: vec2<i32>) -> bool {
    let x = bitcast<u32>(c.x);
    let y = bitcast<u32>(c.y);
    let v = (x * 0x0CCAC033u + 0x6C1AC587u) * y + x * 0xBDE41C43u + 0xAE6470DBu;
    return (v >> 31u) != 0u;
}

fn wrap(w: vec2<i32>) -> vec2<i32> {
    return ((w % vec2<i32>(SIDE)) + vec2<i32>(SIDE)) % vec2<i32>(SIDE);
}

// Whether world cell `w` lies in the square starting at `first`.
fn within(w: vec2<i32>, first: vec2<i32>) -> bool {
    let r = w - first;
    return all(r >= vec2<i32>(0)) && all(r < vec2<i32>(SIDE));
}

// The water at world cell `w` as the last step left it: calm outside this square, and where the
// last step's square did not reach.
fn held(w: vec2<i32>) -> vec2<f32> {
    if (!within(w, S.origin.xy) || !within(w, S.origin.zw)) {
        return vec2<f32>(0.0);
    }
    return textureLoad(state, wrap(w), 0).rg;
}

// How far a body's legs at `at` stand in the water at the water's level `level`: none with its
// feet out of it, all from knee deep.
fn wet(at: vec4<f32>, level: f32) -> f32 {
    return clamp((level + 0.05 - at.z) / 0.45, 0.0, 1.0);
}

// How much more or less of the water body `k` pushes aside as it sways, `count` steps into the
// simulation: a whole number of sways every ten seconds, so the count's wrap is never seen.
fn sway(k: i32, count: u32) -> f32 {
    let t = f32(count % 600u) / 600.0;
    let one = sin(6.2831853 * (t * SWAYS + f32(k) * 0.37));
    let two = sin(6.2831853 * (t * SWAYS_TOO + f32(k) * 0.71));
    return 1.0 + SWAY * (0.65 * one + 0.35 * two);
}

// How much a body moving from `was` to `at` in a step heaps the water at render space `q` ahead
// of it, or draws it down behind (negative), meters: as much drawn down as heaped, so the water
// neither gains nor loses.
fn bow(q: vec2<f32>, was: vec4<f32>, at: vec4<f32>, level: f32) -> f32 {
    let moved = at.xy - was.xy;
    let len = length(moved);
    if (len < 1e-5) {
        return 0.0;
    }
    let ahead = moved / len;
    let speed = min(len / STEP_SECONDS / RUN, 1.3);
    let front = q - (at.xy + ahead * BOW_OFF);
    let back = q - (at.xy - ahead * BOW_OFF);
    let w2 = BOW_WIDTH * BOW_WIDTH;
    let heap = exp(-dot(front, front) / w2) - exp(-dot(back, back) / w2);
    return BOW_RISE * speed * heap * wet(at, level) * at.w;
}

// How deep a body's legs at `at` push the water at render space `q`, meters, below the surface.
fn pushed(q: vec2<f32>, at: vec4<f32>, level: f32) -> f32 {
    let r = length(q - at.xy) / LEGS;
    if (r >= 1.0) {
        return 0.0;
    }
    if (at.w <= 0.0) {
        return 0.0;
    }
    // Only legs standing in the water, the deeper the more.
    let k = 1.0 - r * r;
    return PUSH * k * k * wet(at, level) * at.w;
}

@compute @workgroup_size(16, 16, 1)
fn cs_step(
    @builtin(global_invocation_id) id: vec3<u32>,
    @builtin(workgroup_id) group: vec3<u32>,
    @builtin(local_invocation_index) index: u32,
) {
    // Cells are stepped by where they lie in the square, so a group's cells lie together in the
    // world wherever the square's wrap falls.
    let first = S.origin.xy;
    let corner = first + vec2<i32>(group.xy) * GROUP - vec2<i32>(REACH);
    for (var i = i32(index); i < TILE * TILE; i = i + GROUP * GROUP) {
        tile[i] = held(corner + vec2<i32>(i % TILE, i / TILE)).x;
    }
    workgroupBarrier();
    let local = vec2<i32>(id.xy) - vec2<i32>(group.xy) * GROUP + vec2<i32>(REACH);
    let w = first + vec2<i32>(id.xy);
    let texel = wrap(w);
    let rel = vec2<f32>(id.xy);
    let p = S.place.xy + (rel + vec2<f32>(0.5)) * CELL;
    let q = S.place.zw + (rel + vec2<f32>(0.5)) * CELL;
    // On land the water holds its level.
    let inside = shore_distance(p, 0.0);
    if (inside <= 0.0) {
        textureStore(next, texel, vec4<f32>(0.0));
        return;
    }
    let now = held(w);
    // The water's vertical derivative there, from its neighbors' heights.
    var pull = 0.0;
    for (var dy = -REACH; dy <= REACH; dy = dy + 1) {
        for (var dx = -REACH; dx <= REACH; dx = dx + 1) {
            let k = KERNEL[abs(dy) * (REACH + 1) + abs(dx)];
            pull = pull + k * tile[(local.y + dy) * TILE + local.x + dx];
        }
    }
    // The step, the water's motion damped a little, more in the shallows at the very edge, where
    // waves break on the shore.
    let a = S.wave.y + (1.0 - smoothstep(0.0, 0.8, inside)) * 0.06;
    var h = (now.x * (2.0 - a) - now.y - S.wave.x * pull) / (1.0 + a);
    // A body moving through the water pushes it aside: the change in how deep its legs push it.
    let level = level_at(p);
    let count = i32(S.misc.y);
    var shelter = 0.0;
    var waterline = 0.0;
    for (var k = 0; k < count; k = k + 1) {
        let now_pushed = pushed(q, S.end[k], level) * sway(k, S.misc.x + 1u);
        h = h - (now_pushed - pushed(q, S.start[k], level) * sway(k, S.misc.x));
        h = h + bow(q, S.start[k], S.end[k], level);
        // The water about it sheltered, and the line where the water meets its legs.
        let r = length(q - S.end[k].xy);
        let there = wet(S.end[k], level) * min(S.end[k].w, 1.0);
        shelter = max(shelter, there * exp(-(r / SHELTER) * (r / SHELTER)));
        // The line laps the legs: wider and brighter as the water there rises, narrower as it falls.
        let lap = clamp(0.5 + now.x * 30.0, 0.15, 1.5);
        let d = (r - LEGS * 1.1) / (0.04 + 0.04 * lap);
        waterline = max(waterline, there * lap * exp(-d * d));
    }
    // Toward the square's edges the waves are taken up.
    let edge = min(min(rel.x, rel.y), min(f32(SIDE - 1) - rel.x, f32(SIDE - 1) - rel.y));
    let calm = mix(0.85, 1.0, smoothstep(0.0, SPONGE, edge));
    textureStore(next, texel, vec4<f32>(clamp(h * calm, -0.3, 0.3), now.x * calm, shelter, waterline));
}
