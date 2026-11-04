// Shafts of sunlight streaming from the sun across the picture where the open sky shows past
// leaves, branches and roofs: each pixel looks back along the line to the sun on screen and
// gathers the open sky it passes, nearer sky counting more, so a bright gap in a canopy streaks
// outward from the sun and whatever stands before the sky cuts the streak. Added to the lit
// picture before the tone map.

struct Params {
    // xy where the sun stands in the viewport, 0 to 1; z how strong the shafts are; w how far
    // along the line to the sun a pixel looks, a share of the line.
    sun: vec4<f32>,
    // rgb the sun's light, linear; w how much each sample counts less than the one before.
    colour: vec4<f32>,
    viewport: vec4<f32>,
    // x the frame's number, wrapped; y how tight the bright sky about the sun is; zw unused.
    extra: vec4<f32>,
};

@group(0) @binding(0) var depth: texture_depth_2d;
@group(0) @binding(1) var<uniform> params: Params;

@vertex
fn vs(@builtin(vertex_index) i: u32) -> @builtin(position) vec4<f32> {
    let x = f32((i << 1u) & 2u);
    let y = f32(i & 2u);
    return vec4<f32>(x * 2.0 - 1.0, 1.0 - y * 2.0, 0.0, 1.0);
}

// How many points along the line to the sun are looked at.
const SAMPLES: i32 = 40;

fn ign(p: vec2<f32>) -> f32 {
    return fract(52.9829189 * fract(dot(p, vec2<f32>(0.06711056, 0.00583715))));
}

// How much open sky shows at `uv` in the viewport, the brighter the nearer the sun.
fn open_sky(uv: vec2<f32>, aspect: f32) -> f32 {
    let v = params.viewport;
    let px = vec2<i32>(clamp(uv, vec2<f32>(0.0), vec2<f32>(0.9999)) * v.zw + v.xy);
    if (textureLoad(depth, px, 0) < 1.0) {
        return 0.0;
    }
    let off = (uv - params.sun.xy) * vec2<f32>(aspect, 1.0);
    return exp(-dot(off, off) * params.extra.y);
}

@fragment
fn fs(@builtin(position) p: vec4<f32>) -> @location(0) vec4<f32> {
    let v = params.viewport;
    let uv = (p.xy - v.xy) / v.zw;
    let aspect = v.z / max(v.w, 1.0);
    let toward = (params.sun.xy - uv) * params.sun.w / f32(SAMPLES);
    var at = uv + toward * fract(ign(p.xy) + 0.618034 * params.extra.x);
    var weight = 1.0;
    var gathered = 0.0;
    for (var i: i32 = 0; i < SAMPLES; i = i + 1) {
        gathered = gathered + open_sky(at, aspect) * weight;
        weight = weight * params.colour.w;
        at = at + toward;
    }
    return vec4<f32>(params.colour.rgb * (gathered * params.sun.z / f32(SAMPLES)), 0.0);
}
