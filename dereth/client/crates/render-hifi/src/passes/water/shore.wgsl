// Where the world's water lies: the same arithmetic for the drawn ground's bed, the art pack's
// wet bank and the water itself, so the three meet.
//
// `water_field` holds, for each landscape vertex (east across, north up): x how far inside the
// water it lies, meters, negative on land; y how far below it the water's level lies, meters. A
// shader this is added to has each vertex's height (`vertex_height`) and retail's cut of each cell
// (`sw_to_ne`).
// Between the vertices the distance is drawn smoothly through them, and the water's edge is
// broken by a little noise, so it neither runs straight along the cells nor in the same curve
// about every lone water vertex.

const SHORE_CELL: f32 = 24.0;
const SHORE_SIDE: i32 = 2041;
// How far outside the water a point lies at most, meters, for it to be worth the smooth
// distance: farther out, the edge is not near.
const SHORE_NEAR: f32 = 40.0;

fn shore_texel(g: vec2<i32>) -> vec2<f32> {
    return textureLoad(water_field, clamp(g, vec2<i32>(0), vec2<i32>(SHORE_SIDE - 1)), 0).rg;
}

fn shore_catmull(t: f32) -> vec4<f32> {
    let t2 = t * t;
    let t3 = t2 * t;
    return 0.5 * vec4<f32>(-t3 + 2.0 * t2 - t, 3.0 * t3 - 5.0 * t2 + 2.0, -3.0 * t3 + 4.0 * t2 + t, t3 - t2);
}

// A lattice point's hash, 0..1.
fn shore_hash(p: vec2<i32>) -> f32 {
    let h0 = (bitcast<u32>(p.x) * 0x8DA6B343u) ^ (bitcast<u32>(p.y) * 0xD8163841u);
    let h1 = (h0 ^ (h0 >> 13u)) * 0x2C1B3C6Du;
    let h2 = h1 ^ (h1 >> 15u);
    return f32(h2 & 0xFFFFu) / 65535.0;
}

// Smooth noise over a lattice `cell` meters wide, 0..1.
fn shore_noise(p: vec2<f32>, cell: f32) -> f32 {
    let q = p / cell;
    let i = vec2<i32>(floor(q));
    let f = q - floor(q);
    let s = f * f * (3.0 - 2.0 * f);
    let a = shore_hash(i);
    let b = shore_hash(i + vec2<i32>(1, 0));
    let c = shore_hash(i + vec2<i32>(0, 1));
    let d = shore_hash(i + vec2<i32>(1, 1));
    return mix(mix(a, b, s.x), mix(c, d, s.x), s.y);
}

// How much of a wander `cell` meters across shows in a pixel that covers `foot` meters: all of it
// while the pixel is a quarter of it or less, none once it is half of it (nought for all of it).
fn shore_resolved(cell: f32, foot: f32) -> f32 {
    return 1.0 - smoothstep(0.25 * cell, 0.5 * cell, foot);
}

// How much the water's edge is moved at world `p`, meters, out into the land where positive, as
// a pixel covering `foot` meters sees it: a wander finer than the pixel is left out, so a far
// shore holds still under the anti-aliasing's jitter.
fn shore_wander(p: vec2<f32>, foot: f32) -> f32 {
    return (shore_noise(p, 9.0) - 0.5) * 6.0 * shore_resolved(9.0, foot)
        + (shore_noise(p + vec2<f32>(31.7, 12.9), 3.5) - 0.5) * 2.0 * shore_resolved(3.5, foot);
}

// How far inside the water world `p` lies, meters, negative on land, as a pixel covering `foot`
// meters sees it; far from any water, the field's own coarse value.
fn shore_distance(p: vec2<f32>, foot: f32) -> f32 {
    let q = p / SHORE_CELL;
    let c = vec2<i32>(floor(q));
    let f = clamp(q - vec2<f32>(c), vec2<f32>(0.0), vec2<f32>(1.0));
    let a = shore_texel(c).x;
    let b = shore_texel(c + vec2<i32>(1, 0)).x;
    let d = shore_texel(c + vec2<i32>(0, 1)).x;
    let e = shore_texel(c + vec2<i32>(1, 1)).x;
    let nearest = max(max(a, b), max(d, e));
    if (nearest < -SHORE_NEAR) {
        return mix(mix(a, b, f.x), mix(d, e, f.x), f.y);
    }
    let wx = shore_catmull(f.x);
    let wy = shore_catmull(f.y);
    var s = 0.0;
    for (var i = 0; i < 4; i = i + 1) {
        var column = 0.0;
        for (var j = 0; j < 4; j = j + 1) {
            column = column + wy[j] * shore_texel(c + vec2<i32>(i - 1, j - 1)).x;
        }
        s = s + wx[i] * column;
    }
    return s + shore_wander(p, foot);
}

// The made-up depth of the bed below the water's surface `inside` meters inside the water: none at
// its edge, falling away steepest there and leveling out at `depth` meters `shelf` meters in.
fn bed_depth(inside: f32, depth: f32, shelf: f32) -> f32 {
    if (inside <= 0.0) {
        return 0.0;
    }
    let t = 1.0 - min(inside / max(shelf, 1e-3), 1.0);
    return depth * (1.0 - t * t);
}

// How far below the water's level the land beside it may lie, meters for each meter it lies
// outside the water's edge: none at the edge, so the bank never dips under the water beside it,
// and soon none that a rounded bank reaches.
const SHORE_FALL: f32 = 0.25;

// The lowest the land `inside` meters inside the water (negative outside it) is drawn, where the
// water's level there is `level`: the level at the edge, falling away by `SHORE_FALL` a meter out.
fn shore_floor(level: f32, inside: f32) -> f32 {
    return level + min(inside, 0.0) * SHORE_FALL;
}

// The water's level at world `p`: each vertex's level (its height, less how far below it the
// water lies) across retail's triangles, as retail draws its surface.
fn level_at(p: vec2<f32>) -> f32 {
    let q = p / SHORE_CELL;
    let c = vec2<i32>(floor(q));
    let f = clamp(q - vec2<f32>(c), vec2<f32>(0.0), vec2<f32>(1.0));
    let h00 = vertex_height(c) - shore_texel(c).y;
    let h10 = vertex_height(c + vec2<i32>(1, 0)) - shore_texel(c + vec2<i32>(1, 0)).y;
    let h01 = vertex_height(c + vec2<i32>(0, 1)) - shore_texel(c + vec2<i32>(0, 1)).y;
    let h11 = vertex_height(c + vec2<i32>(1, 1)) - shore_texel(c + vec2<i32>(1, 1)).y;
    if (sw_to_ne(c)) {
        if (f.x >= f.y) {
            return h00 + f.x * (h10 - h00) + f.y * (h11 - h10);
        }
        return h00 + f.y * (h01 - h00) + f.x * (h11 - h01);
    }
    if (f.x + f.y <= 1.0) {
        return h00 + f.x * (h10 - h00) + f.y * (h01 - h00);
    }
    return h11 + (1.0 - f.x) * (h01 - h11) + (1.0 - f.y) * (h10 - h11);
}
