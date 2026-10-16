// The water, drawn over the lit world where the drawn ground lays retail's water.
//
// The drawn ground lays a bed under the water, lower the farther inside it, and the light per
// pixel has lit it. This pass finds, for each pixel, where the line of sight first passes under
// the water's surface on its way to what the pixel shows: the surface lies at retail's level,
// level across the water and up to its edge (see the shore's arithmetic). There the water is
// drawn: what lies under it seen through it, bent by its ripples and faded into the water's own
// color by the depth of water passed through; the sky and the land reflected in it by the
// Fresnel share; and the sun's glint. Toward its edge the water thins to nothing, so the shore is
// a soft change of depth and color with nothing laid along it.
//
// The ripples are one small picture of wind ripples laid several times over the water, at
// different sizes and turns, each drifting with the game's clock; they calm with distance, so far
// water lies smooth and mirrors the sky.

struct Water {
    // Render space, height up, to clip space, at the frame's jitter.
    clip_from_render: mat4x4<f32>,
    // Clip space to render space, at the frame's jitter: back from the depth to the very point
    // each pixel's depth was drawn for.
    render_from_clip: mat4x4<f32>,
    // The same without the jitter: the line of sight through each pixel's middle, which the
    // water's own look follows, so it holds still whatever fraction of a pixel the frame is
    // drawn off.
    steady_from_clip: mat4x4<f32>,
    // The sun's shadow cascades: render space to each cascade's clip space.
    cascade: array<mat4x4<f32>, 4>,
    // The world viewport: x, y, width, height, in target pixels.
    viewport: vec4<f32>,
    // xyz the eye, render space; w how far from the eye the ground is drawn, meters.
    eye: vec4<f32>,
    // xy where render space's origin lies in the world, meters; z how much of the sky is the
    // physical sky; w 1 when the world is drawn with its authored fog.
    global: vec4<f32>,
    // xyz toward the sun; w 1 while the sun is up.
    sun: vec4<f32>,
    // rgb the sun's light, linear, as the light per pixel takes it; w unused.
    sun_light: vec4<f32>,
    // x where the authored fog begins, y where it is whole, meters; zw unused.
    fog: vec4<f32>,
    // The sky toward the horizon and overhead, linear, where no physical sky is drawn.
    sky_low: vec4<f32>,
    sky_high: vec4<f32>,
    // Retail's water under the light the ordinary picture draws it with, linear, for terrain
    // types 16 to 20.
    hue: array<vec4<f32>, 5>,
    // x the water's own color, a share of retail's; y how far the eye sees into it, meters;
    // z the deepest the bed lies, w how far from the shore it reaches that depth, meters.
    look: vec4<f32>,
    // x how much the ripples bend the surface; y how far from the eye they have calmed, meters;
    // z how strongly the water reflects; w how far from its edge it takes to come in, meters.
    look2: vec4<f32>,
    // Each layer of ripples: x meters a repeat, y how much it bends the surface, z its turn
    // (radians), w how far it has drifted along its own east, a share of a repeat.
    layers: array<vec4<f32>, 4>,
    // The far distance of each cascade, meters from the eye.
    splits: vec4<f32>,
    // The world size of one shadow texel in each cascade, meters.
    texel: vec4<f32>,
    // x 1 when the cascades hold this frame's shadow, y their size in texels; z a diagnostic
    // view (1 the depth of water, 2 the reflection alone); w unused.
    shadow: vec4<f32>,
    // x the rings' clock, seconds; y how many bodies stand in `waders`; z how strongly the water
    // rings round them; w unused.
    wake: vec4<f32>,
    // The bodies near the eye the water rings round where they stand in it, render space.
    waders: array<vec4<f32>, 8>,
};

@group(0) @binding(0) var<uniform> W: Water;
@group(0) @binding(1) var depth: texture_depth_2d;
// The lit world before the water.
@group(0) @binding(2) var picture: texture_2d<f32>;
// The world as the ordinary picture draws it, before the light per pixel.
@group(0) @binding(3) var ordinary: texture_2d<f32>;
// Each landscape vertex's height, meters.
@group(0) @binding(4) var heights: texture_2d<f32>;
// Where the water lies, a value pair each landscape vertex.
@group(0) @binding(5) var water_field: texture_2d<f32>;
// Each landscape vertex's terrain word.
@group(0) @binding(6) var words: texture_2d<u32>;
// The ripples' picture: slopes east and north in red and green.
@group(0) @binding(7) var ripple_tex: texture_2d<f32>;
@group(0) @binding(8) var wrap_samp: sampler;
@group(0) @binding(9) var clamp_samp: sampler;
@group(0) @binding(10) var sky_lut: texture_2d<f32>;
@group(0) @binding(12) var shadow_map: texture_depth_2d_array;
@group(0) @binding(13) var shadow_cmp: sampler_comparison;
// The world's normals, with each surface's material class in alpha.
@group(0) @binding(14) var normals: texture_2d<f32>;

// The steepest slope the ripples' picture holds.
const MOST_SLOPE: f32 = 0.6;
// What a still water surface reflects looking straight down.
const F0: f32 = 0.02;
// The share of the Fresnel reflection drawn: water seen at a low angle reflects nearly all the
// sky, which would turn retail's blue to the sky's white; this keeps the water's own color
// showing through.
const REFLECTED: f32 = 0.55;

fn render_at(px: vec2<f32>, d: f32) -> vec3<f32> {
    let v = W.viewport;
    let uv = (px - v.xy) / v.zw;
    let w = W.render_from_clip * vec4<f32>(uv.x * 2.0 - 1.0, 1.0 - uv.y * 2.0, d, 1.0);
    return w.xyz / w.w;
}

// The line of sight through the middle of pixel `px`, without the frame's jitter: a unit
// direction, render space.
fn steady_ray(px: vec2<f32>) -> vec3<f32> {
    let v = W.viewport;
    let uv = (px - v.xy) / v.zw;
    let w = W.steady_from_clip * vec4<f32>(uv.x * 2.0 - 1.0, 1.0 - uv.y * 2.0, 0.5, 1.0);
    return normalize(w.xyz / w.w - W.eye.xyz);
}

// The target pixel a render-space point lands on; z 1 when it is in the viewport and in front.
fn pixel_of(q: vec3<f32>) -> vec3<f32> {
    let c = W.clip_from_render * vec4<f32>(q, 1.0);
    if (c.w <= 0.001) {
        return vec3<f32>(0.0);
    }
    let ndc = c.xy / c.w;
    if (abs(ndc.x) >= 1.0 || abs(ndc.y) >= 1.0) {
        return vec3<f32>(0.0);
    }
    let v = W.viewport;
    return vec3<f32>(v.xy + vec2<f32>(ndc.x * 0.5 + 0.5, 0.5 - ndc.y * 0.5) * v.zw, 1.0);
}

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

// Retail's water hue at world `p`: the water types of the cell's corners, blended across the
// cell, the land corners leaving their share to the water ones.
fn hue_at(p: vec2<f32>) -> vec3<f32> {
    let q = p / SHORE_CELL;
    let c = vec2<i32>(floor(q));
    let f = clamp(q - floor(q), vec2<f32>(0.0), vec2<f32>(1.0));
    var sum = vec3<f32>(0.0);
    var n = 0.0;
    for (var k = 0; k < 4; k = k + 1) {
        let o = vec2<i32>(k & 1, k >> 1u);
        let g = clamp(c + o, vec2<i32>(0), vec2<i32>(SHORE_SIDE - 1));
        let t = (textureLoad(words, g, 0).r >> 2u) & 0x1Fu;
        let w = select(1.0 - f.x, f.x, o.x == 1) * select(1.0 - f.y, f.y, o.y == 1) + 1e-4;
        if (t >= 16u && t <= 20u) {
            sum = sum + W.hue[t - 16u].rgb * w;
            n = n + w;
        }
    }
    if (n <= 0.0) {
        return W.hue[0].rgb;
    }
    return sum / n;
}

// How much of a layer of ripples `repeat` meters a repeat shows in a pixel that covers `foot`
// meters of water: all of it while the pixel is a twenty-fourth of a repeat or less, none once it
// is a sixth. A ripple finer than the pixel is left out, not sampled, so the far water lies calm and
// holds still under the anti-aliasing's jitter.
fn ripple_resolved(repeat: f32, foot: f32) -> f32 {
    return 1.0 - smoothstep(repeat / 24.0, repeat / 6.0, foot);
}

// The ripples' slope east and north at world `p`, `dist` meters from the eye, as a pixel
// covering `dp_dx` and `dp_dy` of the water sees them, `foot` meters of it at the most.
fn ripples(p: vec2<f32>, dist: f32, dp_dx: vec2<f32>, dp_dy: vec2<f32>, foot: f32) -> vec2<f32> {
    let calm = 1.0 - smoothstep(W.look2.y * 0.15, W.look2.y, dist);
    if (calm <= 0.0 || W.look2.x <= 0.0) {
        return vec2<f32>(0.0);
    }
    // Patches of rougher and glassier water, tens of meters across.
    let gust = 0.55 + 0.9 * shore_noise(p + vec2<f32>(5.0, 11.0), 41.0);
    var s = vec2<f32>(0.0);
    for (var k = 0; k < 4; k = k + 1) {
        let l = W.layers[k];
        let c = cos(l.z);
        let sn = sin(l.z);
        // The layer's own east and north, and the picture's rows running down to the south.
        let along = vec2<f32>(c * p.x + sn * p.y, -sn * p.x + c * p.y) / l.x;
        let uv = vec2<f32>(along.x - l.w, -along.y);
        let gx = vec2<f32>(c * dp_dx.x + sn * dp_dx.y, sn * dp_dx.x - c * dp_dx.y) / l.x;
        let gy = vec2<f32>(c * dp_dy.x + sn * dp_dy.y, sn * dp_dy.x - c * dp_dy.y) / l.x;
        let t = textureSampleGrad(ripple_tex, wrap_samp, uv, gx, gy).rg * 2.0 - vec2<f32>(1.0);
        // Back from the layer's turn.
        let slope = vec2<f32>(c * t.x - sn * t.y, sn * t.x + c * t.y) * MOST_SLOPE;
        s = s + slope * l.y * ripple_resolved(l.x, foot);
    }
    return s * 0.35 * W.look2.x * calm * gust;
}

// The rings round a body standing in the water: how far apart they run, meters; how many a
// second spread from it; how high they rise, meters; how far out they have faded to a third, and
// past how far none are drawn.
const RING_APART: f32 = 0.6;
const RING_RATE: f32 = 0.8;
const RING_HEIGHT: f32 = 0.06;
const RING_FADE: f32 = 2.0;
const RING_REACH: f32 = 6.0;

// The slope east and north the rings round the bodies standing in the water give it at render
// space `p`, at the water's level `level`: a few small rings spreading from each, fading within a
// few meters, moving with the rings' clock (a held clock holds them still). Drawn only: nothing
// stands or moves differently for them.
fn rings(p: vec2<f32>, level: f32) -> vec2<f32> {
    var s = vec2<f32>(0.0);
    let count = i32(W.wake.y);
    for (var k = 0; k < count; k = k + 1) {
        let b = W.waders[k].xyz;
        // Only a body standing in the water: its feet under the surface, not far under it.
        if (b.z > level + 0.05 || b.z < level - 1.6) {
            continue;
        }
        let d = p - b.xy;
        let r = length(d);
        if (r > RING_REACH || r < 1e-3) {
            continue;
        }
        if (shore_distance(b.xy + W.global.xy, 0.0) <= 0.0) {
            continue;
        }
        s = s + d / r * ring_rise(r, W.wake.x);
    }
    return s * W.wake.z;
}

// How steeply the rings round a body standing in the water rise going out from it, `r` meters
// out, `clock` seconds into the rings' clock: rings spreading from its legs outward, fading out
// within a few meters.
fn ring_rise(r: f32, clock: f32) -> f32 {
    let wave = 6.2831853 / RING_APART;
    let phase = wave * r - 6.2831853 * RING_RATE * clock;
    let fade = exp(-r / RING_FADE) * smoothstep(0.25, 0.55, r)
        * (1.0 - smoothstep(0.7 * RING_REACH, RING_REACH, r));
    return RING_HEIGHT * (wave * cos(phase) - sin(phase) / RING_FADE) * fade;
}

// The sky toward `dir`: the physical sky where it is drawn, else a gradient from the horizon's
// color to the color overhead.
fn sky_toward(dir: vec3<f32>) -> vec3<f32> {
    let up = clamp(dir.z, 0.0, 1.0);
    var c = mix(W.sky_low.rgb, W.sky_high.rgb, pow(up, 0.6));
    if (W.global.z > 0.0) {
        let r = eye_radius();
        let bottom = planet_radius();
        let horizon_cos = -sqrt(max(r * r - bottom * bottom, 0.0)) / r;
        let cz = max(dir.z, horizon_cos + 0.002);
        let uv = sky_view_uv(r, cz, light_view_cos(dir));
        let phys = exposed_sky(textureSampleLevel(sky_lut, clamp_samp, uv, 0.0).rgb);
        c = mix(c, phys, W.global.z);
    }
    return c;
}

// The sky along `r` from `s`, as the water reflects it: the sky's own model in that direction;
// on water near the eye (`near` of it), where the line leaves the world well inside the picture
// and over its sky, the picture's own sky there, with its clouds for the ripples to break up,
// faded into the model toward the picture's edges. The picture's edge is never stretched over
// the water: far water seen from above reflects the sky high over the picture's top, where the
// picture holds none, and the sky low over the far land is the haze the frame's jitter moves the
// land's line through.
fn sky_far(s: vec3<f32>, r: vec3<f32>, near: f32) -> vec3<f32> {
    let sky = sky_toward(r);
    if (near <= 0.0) {
        return sky;
    }
    let c = W.clip_from_render * vec4<f32>(s + r * W.eye.w, 1.0);
    if (c.w <= 0.001) {
        return sky;
    }
    let ndc = c.xy / c.w;
    let on = (1.0 - smoothstep(0.8, 0.97, max(abs(ndc.x), abs(ndc.y)))) * near;
    if (on <= 0.0) {
        return sky;
    }
    let v = W.viewport;
    let at = v.xy + vec2<f32>(ndc.x * 0.5 + 0.5, 0.5 - ndc.y * 0.5) * v.zw;
    if (textureLoad(depth, vec2<i32>(at), 0) < 1.0) {
        return sky;
    }
    let size = vec2<f32>(textureDimensions(picture));
    return mix(sky, textureSampleLevel(picture, clamp_samp, at / size, 0.0).rgb, on);
}

// How far behind what the picture shows at `at` (a target pixel) the point `q` lies, meters:
// negative in front of it, and in front of the sky.
fn behind_picture(q: vec3<f32>, at: vec2<f32>) -> f32 {
    let d = textureLoad(depth, vec2<i32>(at), 0);
    if (d >= 1.0) {
        return -1.0;
    }
    return length(q - W.eye.xyz) - length(render_at(at, d) - W.eye.xyz);
}

// What the water reflects from `s` along `r`, `near` of the way to the eye as `sky_far` takes it:
// the world on screen where the line meets it, else the sky.
//
// The line is followed across the picture in even steps of a few pixels, out to the horizon or
// the picture's edge, against the depth: so a far hill is met as surely as a near bank, whatever
// fraction of a pixel the frame is drawn off. Where a step has passed behind what the picture
// shows since the last, the meeting is found between the two by halving, and taken only where
// the line truly meets that surface (not where a thing nearer the eye merely hides the line) and
// the surface stands above the water: what lies under the water is not reflected in it.
fn reflected(s: vec3<f32>, r: vec3<f32>, near: f32) -> vec3<f32> {
    let v = W.viewport;
    var reach = W.eye.w;
    let c0 = W.clip_from_render * vec4<f32>(s, 1.0);
    var c1 = W.clip_from_render * vec4<f32>(s + r * reach, 1.0);
    // The far end kept in front of the eye.
    if (c1.w < 0.05) {
        let f = (c0.w - 0.05) / max(c0.w - c1.w, 1e-6);
        reach = reach * f;
        c1 = mix(c0, c1, f);
    }
    let n0 = c0.xy / c0.w;
    let n1 = c1.xy / c1.w;
    let span = length((n1 - n0) * v.zw * 0.5);
    let steps = clamp(i32(span / 5.0), 6, 48);
    let wide = abs(n1.x - n0.x) >= abs(n1.y - n0.y);
    var last = 0.0;
    var in_front = true;
    for (var k = 1; k <= steps; k = k + 1) {
        let u = f32(k) / f32(steps);
        let nd = mix(n0, n1, u);
        if (abs(nd.x) >= 1.0 || abs(nd.y) >= 1.0) {
            break;
        }
        // Where along the line the step lies: the screen is a projection of it.
        var f = u;
        if (wide) {
            f = (nd.x * c0.w - c0.x) / max(abs((c1.x - c0.x) - nd.x * (c1.w - c0.w)), 1e-9)
                * sign((c1.x - c0.x) - nd.x * (c1.w - c0.w));
        } else {
            f = (nd.y * c0.w - c0.y) / max(abs((c1.y - c0.y) - nd.y * (c1.w - c0.w)), 1e-9)
                * sign((c1.y - c0.y) - nd.y * (c1.w - c0.w));
        }
        let t = clamp(f, 0.0, 1.0) * reach;
        let q = s + r * t;
        let at = v.xy + vec2<f32>(nd.x * 0.5 + 0.5, 0.5 - nd.y * 0.5) * v.zw;
        let gap = behind_picture(q, at);
        if (gap > 0.0 && in_front) {
            var lo = last;
            var hi = t;
            for (var j = 0; j < 8; j = j + 1) {
                let mid = 0.5 * (lo + hi);
                let qm = s + r * mid;
                let am = pixel_of(qm);
                if (am.z > 0.5 && behind_picture(qm, am.xy) > 0.0) {
                    hi = mid;
                } else {
                    lo = mid;
                }
            }
            let qh = s + r * hi;
            let hit = pixel_of(qh);
            let dh = textureLoad(depth, vec2<i32>(hit.xy), 0);
            let met = render_at(hit.xy, dh);
            let off = length(qh - W.eye.xyz) - length(met - W.eye.xyz);
            let wet = shore_distance(met.xy + W.global.xy, 0.0) > 0.0;
            let under = wet && met.z < level_at(met.xy + W.global.xy) - 0.1;
            if (hit.z > 0.5 && dh < 1.0 && off < 0.5 + hi * 0.03 && !under) {
                let uv = (hit.xy - v.xy) / v.zw;
                // Fading out toward the edges of the picture, where the world runs off it.
                let edge = smoothstep(0.0, 0.12, min(uv.x, 1.0 - uv.x)) * smoothstep(0.0, 0.08, min(uv.y, 1.0 - uv.y));
                // A little softened, so a reflected leaf or edge shimmers no more than the
                // world it reflects: the picture about the meeting, the things in it (a crown, a
                // trunk, a bank) apart from the sky between them. A crown's cut-out edge shows a
                // different speck of sky from one fraction of a pixel to the next, so a crown is
                // reflected as its own color over as much of the sky as it covers about there:
                // dark and green where it is dense, thinning into the sky at its edge, and as
                // still as the crown.
                let size = vec2<f32>(textureDimensions(picture));
                var world = vec3<f32>(0.0);
                var covered = 0.0;
                for (var j = 0; j < 9; j = j + 1) {
                    let o = vec2<f32>(f32(j % 3) - 1.0, f32(j / 3) - 1.0) * 1.5;
                    let at = hit.xy + o;
                    if (textureLoad(depth, vec2<i32>(at), 0) < 1.0) {
                        world = world + textureSampleLevel(picture, clamp_samp, at / size, 0.0).rgb;
                        covered = covered + 1.0;
                    }
                }
                let sky = sky_far(s, r, near);
                let held = mix(sky, world / max(covered, 1.0), covered / 9.0);
                return mix(sky, held, edge);
            }
        }
        in_front = gap <= 0.0;
        last = t;
    }
    return sky_far(s, r, near);
}

// The sun's visibility at `q` through the cascade `c`, -1 when `q` lies outside it.
fn cascade_vis(c: i32, q: vec3<f32>) -> f32 {
    let texel = W.texel[c];
    let l = W.sun.xyz;
    let lp = W.cascade[c] * vec4<f32>(q + vec3<f32>(0.0, 0.0, texel * 1.5) + l * texel, 1.0);
    let uv = vec2<f32>(lp.x * 0.5 + 0.5, 0.5 - lp.y * 0.5);
    if (any(uv < vec2<f32>(0.0)) || any(uv > vec2<f32>(1.0)) || lp.z > 1.0) {
        return -1.0;
    }
    let step = 1.5 / W.shadow.y;
    var lit = 0.0;
    for (var k = 0; k < 4; k = k + 1) {
        let o = vec2<f32>(f32(k & 1) - 0.5, f32(k >> 1u) - 0.5) * step;
        lit = lit + textureSampleCompareLevel(shadow_map, shadow_cmp, uv + o, c, lp.z);
    }
    return lit * 0.25;
}

// The sun's visibility at the surface point `q`.
fn sun_vis(q: vec3<f32>) -> f32 {
    if (W.shadow.x < 0.5) {
        return 1.0;
    }
    let dist = length(q - W.eye.xyz);
    for (var c = 0; c < 4; c = c + 1) {
        if (dist < W.splits[c]) {
            let s = cascade_vis(c, q);
            if (s >= 0.0) {
                return s;
            }
        }
    }
    return 1.0;
}

// The sun's specular reflection off a surface of roughness `rough`, without its Fresnel term.
fn glint(n: vec3<f32>, l: vec3<f32>, v: vec3<f32>, rough: f32) -> f32 {
    let a = rough * rough;
    let a2 = a * a;
    let h = normalize(l + v);
    let ndh = max(dot(n, h), 0.0);
    let ndl = max(dot(n, l), 0.0);
    let ndv = max(dot(n, v), 1e-3);
    let d = ndh * ndh * (a2 - 1.0) + 1.0;
    let distribution = a2 / (3.14159265 * d * d);
    let k = (rough + 1.0) * (rough + 1.0) / 8.0;
    let visibility = 1.0 / ((ndl * (1.0 - k) + k) * (ndv * (1.0 - k) + k) * 4.0);
    return distribution * visibility * ndl;
}

// Where the line of sight through pixel `px` meets the level water at `level`, east and north in
// render space: the water a neighboring pixel sees, for how far the ripples move from one pixel
// to the next; no farther than `most` meters from `s`, where the water runs along the view.
fn on_level(px: vec2<f32>, level: f32, s: vec3<f32>, most: f32) -> vec2<f32> {
    let eye = W.eye.xyz;
    let r = steady_ray(px);
    if (r.z > -1e-5) {
        return s.xy + normalize(r.xy + vec2<f32>(1e-6)) * most;
    }
    let q = eye.xy + r.xy * ((level - eye.z) / r.z);
    let step = q - s.xy;
    return s.xy + step * min(1.0, most / max(length(step), 1e-6));
}

// How much of the water is drawn `inside` meters inside its edge, over `deep` meters of water
// under the surface and `through` meters of it along the line of sight: nothing at the edge,
// coming in over `soft` meters and over the first few centimeters of depth; but whole where what
// is seen lies well under the surface, as the bed beyond a bank's lip does, which the lip hides
// close by.
fn edge_share(inside: f32, soft: f32, deep: f32, through: f32) -> f32 {
    let shore = max(smoothstep(0.0, max(soft, 1e-3), inside), smoothstep(0.15, 0.8, deep));
    return shore * smoothstep(0.0, 0.08, deep + through * 0.1);
}

// Retail's surface at world `p`, across its triangles.
fn retail_at(p: vec2<f32>) -> f32 {
    let q = p / SHORE_CELL;
    let c = vec2<i32>(floor(q));
    let f = clamp(q - vec2<f32>(c), vec2<f32>(0.0), vec2<f32>(1.0));
    let h00 = vertex_height(c);
    let h10 = vertex_height(c + vec2<i32>(1, 0));
    let h01 = vertex_height(c + vec2<i32>(0, 1));
    let h11 = vertex_height(c + vec2<i32>(1, 1));
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

// The drawn ground at world `p` about the water, as a pixel covering `foot` meters sees it: the
// bed lowered under the water, and beside it land no lower than the water's level.
fn bed_at(p: vec2<f32>, foot: f32) -> f32 {
    let inside = shore_distance(p, foot);
    let r = retail_at(p);
    if (inside <= 0.0) {
        return max(r, level_at(p));
    }
    return r - bed_depth(inside, W.look.z, W.look.w);
}

// The bed at world `p`, roughly: the distance inside the water drawn straight between the
// vertices, without the edge's wander. Enough to tell how far a line runs under the water.
fn bed_roughly(p: vec2<f32>) -> f32 {
    let q = p / SHORE_CELL;
    let c = vec2<i32>(floor(q));
    let f = clamp(q - vec2<f32>(c), vec2<f32>(0.0), vec2<f32>(1.0));
    let inside = mix(
        mix(shore_texel(c).x, shore_texel(c + vec2<i32>(1, 0)).x, f.x),
        mix(shore_texel(c + vec2<i32>(0, 1)).x, shore_texel(c + vec2<i32>(1, 1)).x, f.x),
        f.y,
    );
    let r = retail_at(p);
    if (inside <= 0.0) {
        return max(r, level_at(p));
    }
    return r - bed_depth(inside, W.look.z, W.look.w);
}

// How far the line from the surface point `s` along `dir` runs under the water before it meets
// the bed, meters, the water `deep_here` deep at `s`: followed in doubling steps out to 64 m, the
// meeting found between the last two. Past a bank's lip the bed falls away from the line and it
// runs far, so the water there is whole.
fn water_through(s: vec3<f32>, dir: vec3<f32>, deep_here: f32) -> f32 {
    var last = 0.0;
    var last_gap = deep_here;
    var d = 0.5;
    for (var k = 0; k < 8; k = k + 1) {
        let p = s + dir * d;
        let gap = p.z - bed_roughly(p.xy + W.global.xy);
        if (gap <= 0.0) {
            return last + (d - last) * last_gap / max(last_gap - gap, 1e-4);
        }
        last = d;
        last_gap = gap;
        d = d * 2.0;
    }
    return last;
}

@fragment
fn fs_water(in: FullscreenOut) -> @location(0) vec4<f32> {
    let px = in.position.xy;
    let ip = vec2<i32>(px);
    let d = textureLoad(depth, ip, 0);
    let eye = W.eye.xyz;
    // The steady line of sight through the pixel's middle, which everything the water is made of
    // follows (where its surface is met, how far inside the water that is, how deep, how much of
    // it the line passes through), so the water and its edges hold still whatever fraction of a
    // pixel the frame is drawn off; and how far the depth says the pixel's own sample lies, used
    // only to find what stands in front of the water. Past the world, as far as the ground is
    // drawn.
    let dir = steady_ray(px);
    if (dir.z >= -1e-4) {
        discard;
    }
    var reach = W.eye.w;
    var sample = eye + dir * reach;
    if (d < 1.0) {
        sample = render_at(px, d);
        reach = length(sample - eye);
    }
    // Whether the pixel shows the drawn ground (or the sky past it) rather than a thing standing
    // on it: the ground's own place is decided by the steady line, not by where the frame's
    // jitter put the pixel's sample.
    let kind = textureLoad(normals, ip, 0).w;
    let ground = d >= 1.0 || (kind > 2.95 && kind < 3.45);
    // Only a pixel whose sample lies at the water or under it, by the water, is drawn: what
    // stands clearly above the water, the land away from it and the bank rising from it are not.
    // The bank's lip, at the surface, passes either way; there the steady line decides. For the
    // ground the bounds are looser, so no sample the jitter moves decides it near the water.
    let global = W.global.xy;
    let loose = select(0.0, 1.0, ground);
    if (d < 1.0 && (sample.z > level_at(sample.xy + global) + 0.3 + loose * reach * 0.01
        || shore_distance(sample.xy + global, 0.0) < -SHORE_CELL * (0.5 + loose * 1.5) - loose * reach * 0.01)) {
        discard;
    }
    // Where the line passes under the water's surface: the level under the line where it reaches
    // the depth's sample, then again where the line meets that level, a few times over.
    var level = level_at(eye.xy + dir.xy * min(reach, W.eye.w) + global);
    var t = 0.0;
    for (var k = 0; k < 5; k = k + 1) {
        t = clamp((level - eye.z) / dir.z, 0.0, W.eye.w);
        level = level_at(eye.xy + dir.xy * t + global);
    }
    var met = (level - eye.z) / dir.z;
    t = clamp(met, 0.0, W.eye.w);
    // How far apart along the line the water is met through this pixel and the next: the stretch
    // of the line one pixel covers there.
    let span = max(
        abs(t - clamp((level - eye.z) / min(steady_ray(px + vec2<f32>(1.0, 0.0)).z, -1e-5), 0.0, W.eye.w)),
        abs(t - clamp((level - eye.z) / min(steady_ray(px + vec2<f32>(0.0, 1.0)).z, -1e-5), 0.0, W.eye.w)),
    );
    if (ground) {
        // Over the ground the search above is not enough: seen along the line, the level rises
        // and falls faster than the line descends (a bank, a river down a hillside), and where it
        // settled hung on where in the pixel the sample lay. The line is followed instead over the
        // stretch about the sample and where it meets the level there, a few pixels' stretch and
        // the deepest the bed lies either side, and the first meeting found, then halved down to,
        // is the one the pixel shows, wherever in the pixel the sample lay.
        let deepest = (W.look.z + 1.0) / max(-dir.z, 0.05);
        let lo = max(min(reach, t) - 6.0 * span - deepest, 0.0);
        let hi = min(max(reach, t) + 8.0 * span + 1.0, W.eye.w);
        var last = lo;
        var above = eye.z + dir.z * lo - level_at(eye.xy + dir.xy * lo + global);
        var found = false;
        if (above <= 0.0) {
            t = lo;
            found = true;
        }
        for (var k = 1; k <= 16; k = k + 1) {
            if (found) {
                break;
            }
            let along = mix(lo, hi, f32(k) / 16.0);
            let gap = eye.z + dir.z * along - level_at(eye.xy + dir.xy * along + global);
            if (gap <= 0.0) {
                var a = last;
                var b = along;
                for (var j = 0; j < 6; j = j + 1) {
                    let mid = 0.5 * (a + b);
                    if (eye.z + dir.z * mid - level_at(eye.xy + dir.xy * mid + global) > 0.0) {
                        a = mid;
                    } else {
                        b = mid;
                    }
                }
                // Then where the line meets the level found there, which on level water is
                // exactly where it meets the water, whatever the stretch it was found in.
                t = 0.5 * (a + b);
                for (var j = 0; j < 3; j = j + 1) {
                    t = clamp((level_at(eye.xy + dir.xy * t + global) - eye.z) / dir.z, a, b);
                }
                found = true;
            }
            last = along;
            above = gap;
        }
        if (!found) {
            discard;
        }
        level = level_at(eye.xy + dir.xy * t + global);
        met = t;
    }
    // Past as far as the ground is drawn no water is: the sky shows there, as it does without it.
    if (met >= W.eye.w * 0.999) {
        discard;
    }
    if (ground) {
        // At the bank's lip the steady line is followed back a few pixels' stretch from the
        // water (the stretch where the water is met, so it hangs on nothing the jitter moved),
        // and where the bank rises over it the bank is in front. (Ground farther in front, a
        // hill before a lake, left the water's meeting out of the stretch searched above.)
        let stretch = max(
            abs(t - clamp((level - eye.z) / min(steady_ray(px + vec2<f32>(1.0, 0.0)).z, -1e-5), 0.0, W.eye.w)),
            abs(t - clamp((level - eye.z) / min(steady_ray(px + vec2<f32>(0.0, 1.0)).z, -1e-5), 0.0, W.eye.w)),
        );
        for (var k = 1; k <= 10; k = k + 1) {
            let back = t - (f32(k) * stretch + 0.25 * f32(k));
            if (back > 0.0) {
                let q = eye + dir * back;
                if (q.z < bed_roughly(q.xy + global) - 0.02) {
                    discard;
                }
            }
        }
    } else if (reach < t && sample.z > level + 0.3) {
        // Something stands in front of the water there, above it: the pixel shows it. The bank's
        // lip sits right at the surface, so only what stands clearly above the water counts.
        discard;
    }
    let s = eye + dir * t;
    let sw = s.xy + global;
    // How far the water moves from this pixel to the next, across and down, and the water the
    // pixel covers, meters.
    let dp_dx = on_level(px + vec2<f32>(1.0, 0.0), level, s, 1e9) - s.xy;
    let dp_dy = on_level(px + vec2<f32>(0.0, 1.0), level, s, 1e9) - s.xy;
    let foot = max(length(dp_dx), length(dp_dy));
    let inside = shore_distance(sw, foot);
    // How deep the water stands where the line meets it, over the bed the drawn ground lays.
    let deep_here = level - bed_at(sw, foot);
    if (inside <= 0.0 || deep_here <= 0.0) {
        discard;
    }
    // The eye under the water sees nothing of it drawn.
    if (eye.z < level_at(eye.xy + global) - 0.05 && shore_distance(eye.xy + global, 0.0) > 0.0) {
        discard;
    }
    let dist = t;
    let below = textureLoad(picture, ip, 0).rgb;
    // How much water the line passes through to the bed, and how deep the bed lies where it ends.
    // Close by, a thing standing in the water (a body wading) ends it sooner, as the depth says.
    var through = water_through(s, dir, deep_here);
    through = mix(min(through, max(reach - t, 0.0)), through, smoothstep(20.0, 60.0, dist));
    let deep = through * -dir.z;
    // The surface's slope: its ripples', as the pixel sees them.
    let cap = dist * 0.01 + 0.01;
    let slope = ripples(
        sw,
        dist,
        dp_dx * min(1.0, cap / max(length(dp_dx), 1e-6)),
        dp_dy * min(1.0, cap / max(length(dp_dy), 1e-6)),
        foot,
    ) + rings(s.xy, level) * (1.0 - smoothstep(0.08, 0.15, foot));
    let n = normalize(vec3<f32>(-slope.x, -slope.y, 1.0));
    let v = -dir;
    // What lies under it, seen through it a little bent by the ripples, where what is bent to is
    // under the water too.
    var under = below;
    var under_px = ip;
    let bend = slope * clamp(through, 0.0, 2.0) * W.viewport.w * 0.035;
    if (dot(bend, bend) > 0.25) {
        let q = px + bend;
        let qi = vec2<i32>(clamp(q, W.viewport.xy, W.viewport.xy + W.viewport.zw - vec2<f32>(1.0)));
        let dq = textureLoad(depth, qi, 0);
        if (dq < 1.0 && length(render_at(vec2<f32>(qi), dq) - eye) > t + 0.05) {
            under = textureLoad(picture, qi, 0).rgb;
            under_px = qi;
        }
    }
    // The water's own color: retail's water under the light the bed below takes, as the light
    // per pixel has lit the bed against the ordinary picture.
    let lit = textureLoad(picture, under_px, 0).rgb;
    let old = textureLoad(ordinary, under_px, 0).rgb;
    let ratio = clamp((lit + vec3<f32>(0.003)) / (old + vec3<f32>(0.003)), vec3<f32>(0.0), vec3<f32>(6.0));
    let body = hue_at(sw) * ratio * W.look.x;
    // Light reaching the bed and back up, faded by the water it passes through: red first.
    let extinction = vec3<f32>(1.3, 1.0, 0.82) / max(W.look.y, 0.05);
    let kept = exp(-extinction * (through + deep));
    var c = under * kept + body * (vec3<f32>(1.0) - kept);
    // The sky and the land reflected, by the Fresnel share: off a surface bent half as much as
    // the ripples bend the light through it, so the reflections waver rather than break up; and
    // only as the water deepens from its edge, so a film of water over the bank shows the wet
    // ground rather than a pale rim of sky.
    let nr = normalize(vec3<f32>(-slope.x * 0.5, -slope.y * 0.5, 1.0));
    var r = reflect(-v, nr);
    r.z = max(r.z, 0.01);
    r = normalize(r);
    let cos_v = clamp(dot(nr, v), 0.0, 1.0);
    let fresnel = (F0 + (1.0 - F0) * pow(1.0 - cos_v, 5.0)) * W.look2.z * REFLECTED
        * smoothstep(0.0, 0.3, max(deep_here, deep));
    // Reflected over the pixel's height, two lines half a pixel apart, so what a reflected line
    // meets or misses by a fraction of a pixel (a leaf, a trunk, a bank's edge) is shared between
    // them and holds still under the anti-aliasing's jitter.
    let spread = length(steady_ray(px + vec2<f32>(0.0, 1.0)) - dir) * 0.5;
    let up = vec3<f32>(0.0, 0.0, spread);
    let near = 1.0 - smoothstep(W.look2.y * 0.15, W.look2.y, dist);
    let mirror = 0.5 * (reflected(s, normalize(r + up), near) + reflected(s, normalize(r - up), near));
    c = mix(c, mirror, clamp(fresnel, 0.0, 1.0));
    // The sun's glint, where the sun reaches the surface.
    if (W.sun.w > 0.5) {
        let rough = mix(0.06, 0.14, clamp(length(slope) * 4.0, 0.0, 1.0));
        let g = glint(n, W.sun.xyz, v, rough) * fresnel * sun_vis(s);
        c = c + W.sun_light.rgb * min(g, 40.0);
    }
    // In the authored fog, the water fades into what the fog left of the world behind it.
    if (W.global.w > 0.5) {
        let f = clamp((dist - W.fog.x) / max(W.fog.y - W.fog.x, 1e-3), 0.0, 1.0);
        c = mix(c, below, f);
    }
    // Toward its edge the water thins to nothing: a soft change of depth and color.
    // The edge comes in over at least a pixel and a half of water, so far off it is drawn as
    // smoothly as the pixels allow and holds still under the anti-aliasing's jitter.
    let edge = edge_share(inside, max(W.look2.w, foot * 1.5), deep, through);
    let view = u32(W.shadow.z + 0.5);
    if (view == 1u) {
        return vec4<f32>(vec3<f32>(1.0 - exp(-deep)), 1.0);
    }
    if (view == 2u) {
        return vec4<f32>(mirror, 1.0);
    }
    return vec4<f32>(mix(below, c, edge), 1.0);
}
