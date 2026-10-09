// The opaque world lit again per pixel: the vertex light each surface was drawn with is divided
// out, and the sun (with its shadow), a sky hemisphere and the scene's point lights are put back,
// in linear high dynamic range.

struct Params {
    clip_to_render: mat4x4<f32>,
    view_to_render: mat4x4<f32>,
    render_to_clip: mat4x4<f32>,
    cascade: array<mat4x4<f32>, 4>,
    // The far distance of each cascade, metres from the eye.
    splits: vec4<f32>,
    // The world size of one shadow texel in each cascade, metres.
    texel: vec4<f32>,
    // The depth range of each cascade, metres.
    depth_range: vec4<f32>,
    viewport: vec4<f32>,
    // xyz the eye; w the number of point lights.
    eye: vec4<f32>,
    // xyz toward the sun; w 1 when the shadow maps are drawn.
    sun_dir: vec4<f32>,
    // The sun as the vertex light took it (0..1 per channel); w its gain in the new light.
    sun_legacy: vec4<f32>,
    // The ambient as the vertex light took it; w its gain.
    amb_legacy: vec4<f32>,
    // x fog start, y fog end, z 1 when there is fog.
    fog: vec4<f32>,
    // The fog colour, linear.
    fog_col: vec4<f32>,
    // x point light gain, y contact shadow length (metres), z the shadow map size, w debug view.
    tune: vec4<f32>,
    // The view direction, render space; w the frame's far plane.
    forward: vec4<f32>,
    // y the strength of the light through leaves; z the share of the sun the sky fills shade
    // with, where the bounced light does not; w how thick, metres, a thing in front of a contact
    // ray is taken to be close to the eye.
    extra: vec4<f32>,
    // x 1 when the lamps are lit, y the gain of their glowing parts, z 1 to show their light
    // alone.
    lamp: vec4<f32>,
};

struct Light {
    // xyz position, w falloff distance.
    pos: vec4<f32>,
    // rgb colour times intensity; w 1 for a light inside a building or dungeon cell.
    col: vec4<f32>,
};

@group(0) @binding(0) var picture: texture_2d<f32>;
@group(0) @binding(1) var normals: texture_2d<f32>;
@group(0) @binding(2) var depth: texture_depth_2d;
@group(0) @binding(3) var shadow_map: texture_depth_2d_array;
@group(0) @binding(4) var shadow_cmp: sampler_comparison;
@group(0) @binding(5) var<uniform> params: Params;
@group(0) @binding(6) var<uniform> lights: array<Light, 128>;
// The outdoor lamps' light on each pixel, rgb; alpha how much of the pixel is a lamp's glowing
// part, negative where the lamps light nothing.
@group(0) @binding(7) var lamp_light: texture_2d<f32>;

@vertex
fn vs(@builtin(vertex_index) i: u32) -> @builtin(position) vec4<f32> {
    let x = f32((i << 1u) & 2u);
    let y = f32(i & 2u);
    return vec4<f32>(x * 2.0 - 1.0, 1.0 - y * 2.0, 0.0, 1.0);
}

fn ign(p: vec2<f32>) -> f32 {
    return fract(52.9829189 * fract(dot(p, vec2<f32>(0.06711056, 5.83715e-3))));
}

fn render_at(px: vec2<f32>, d: f32) -> vec3<f32> {
    let v = params.viewport;
    let uv = (px - v.xy) / v.zw;
    let ndc = vec4<f32>(uv.x * 2.0 - 1.0, 1.0 - uv.y * 2.0, d, 1.0);
    let w = params.clip_to_render * ndc;
    return w.xyz / w.w;
}

const TAPS: i32 = 32;

fn vogel(k: i32, n: i32, phi: f32) -> vec2<f32> {
    let golden = 2.39996323;
    let r = sqrt((f32(k) + 0.5) / f32(n));
    let t = f32(k) * golden + phi;
    return vec2<f32>(cos(t), sin(t)) * r;
}

// The sun's visibility at `world`, softened by the distance to the blocker (PCSS).
fn cascade_shadow(c: i32, world: vec3<f32>, n: vec3<f32>, noise: f32) -> f32 {
    let texel = params.texel[c];
    let ndl = clamp(dot(n, params.sun_dir.xyz), 0.0, 1.0);
    // Push the point off the surface along its normal and toward the sun, by a few texels.
    let offset = n * texel * (1.5 + 2.0 * (1.0 - ndl)) + params.sun_dir.xyz * texel * 1.0;
    let lp = params.cascade[c] * vec4<f32>(world + offset, 1.0);
    let uv = vec2<f32>(lp.x * 0.5 + 0.5, 0.5 - lp.y * 0.5);
    let z = lp.z;
    if (any(uv < vec2<f32>(0.0)) || any(uv > vec2<f32>(1.0)) || z > 1.0) {
        return -1.0;
    }
    let size = params.tune.z;
    let range = params.depth_range[c];
    let phi = noise * 6.2831853;
    // Blocker search over a disc about a metre and a half across.
    let search = clamp(0.7 / texel, 2.0, 48.0) / size;
    var blockers = 0.0;
    var sum = 0.0;
    for (var k: i32 = 0; k < 16; k = k + 1) {
        let o = vogel(k, 16, phi) * search;
        let t = vec2<i32>((uv + o) * size);
        let b = textureLoad(shadow_map, t, c, 0);
        if (b < z) {
            blockers = blockers + 1.0;
            sum = sum + b;
        }
    }
    if (blockers < 0.5) {
        return 1.0;
    }
    let blocker = sum / blockers;
    // The sun is about half a degree across; widened a little for the old art's sake.
    let penumbra_m = (z - blocker) * range * 0.018 + texel * 1.5;
    let radius = clamp(penumbra_m / texel, 1.0, 40.0) / size;
    var lit = 0.0;
    for (var k: i32 = 0; k < TAPS; k = k + 1) {
        let o = vogel(k, TAPS, phi) * radius;
        lit = lit + textureSampleCompareLevel(shadow_map, shadow_cmp, uv + o, c, z);
    }
    return lit / f32(TAPS);
}

fn sun_shadow(world: vec3<f32>, n: vec3<f32>, dist: f32, noise: f32) -> f32 {
    if (params.sun_dir.w < 0.5) {
        return 1.0;
    }
    for (var c: i32 = 0; c < 4; c = c + 1) {
        if (dist < params.splits[c]) {
            var s = cascade_shadow(c, world, n, noise);
            if (s < 0.0) {
                continue;
            }
            // Blend into the next cascade over the last tenth of this one.
            let edge = params.splits[c] * 0.9;
            if (c < 3 && dist > edge) {
                let s2 = cascade_shadow(c + 1, world, n, noise);
                if (s2 >= 0.0) {
                    s = mix(s, s2, (dist - edge) / (params.splits[c] - edge));
                }
            }
            // Fade out at the far end of the last cascade.
            if (c == 3) {
                s = mix(s, 1.0, clamp((dist - params.splits[3] * 0.8) / (params.splits[3] * 0.2), 0.0, 1.0));
            }
            return s;
        }
    }
    return 1.0;
}

// Short rays toward the sun through the depth buffer: what the shadow maps do not hold (the
// characters, creatures and other moving things) still shadows what is just behind it.
fn contact_shadow(world: vec3<f32>, dist: f32, noise: f32) -> f32 {
    let len = params.tune.y * clamp(dist / 20.0, 0.35, 1.0);
    if (len <= 0.0 || dist > 80.0) {
        return 1.0;
    }
    let steps = 16;
    var occ = 0.0;
    for (var k: i32 = 0; k < steps; k = k + 1) {
        let t = (f32(k) + noise) / f32(steps);
        let p = world + params.sun_dir.xyz * len * t;
        let c = params.render_to_clip * vec4<f32>(p, 1.0);
        if (c.w <= 0.0) {
            break;
        }
        let ndc = c.xyz / c.w;
        let uv = vec2<f32>(ndc.x * 0.5 + 0.5, 0.5 - ndc.y * 0.5);
        if (any(uv < vec2<f32>(0.0)) || any(uv > vec2<f32>(1.0))) {
            break;
        }
        let px = params.viewport.xy + uv * params.viewport.zw;
        let sd = textureLoad(depth, vec2<i32>(px), 0);
        if (sd >= 1.0) {
            continue;
        }
        let sp = render_at(px, sd);
        let ray_far = length(p - params.eye.xyz);
        let scene_far = length(sp - params.eye.xyz);
        let gap = ray_far - scene_far;
        // A limb or a post is a hand's breadth thick: the ground behind it is not in its shadow.
        let thickness = params.extra.w + ray_far * 0.005;
        if (gap > 0.02 + ray_far * 0.002 && gap < thickness) {
            occ = max(occ, 1.0 - t * 0.6);
        }
    }
    return 1.0 - occ;
}

// The point lights on a surface. A light inside a building lights only that building's own
// cells: nothing here knows what stands between it and the world outside, so the roofs, walls and
// ground around the house would otherwise take its light straight through the walls.
fn point_light(world: vec3<f32>, n: vec3<f32>, inside: bool) -> vec3<f32> {
    var sum = vec3<f32>(0.0);
    let count = i32(params.eye.w);
    for (var k: i32 = 0; k < 128; k = k + 1) {
        if (k >= count) {
            break;
        }
        let l = lights[k];
        if ((l.col.w > 0.5) != inside) {
            continue;
        }
        let d = l.pos.xyz - world;
        let dist = length(d);
        let r = l.pos.w;
        if (dist >= r) {
            continue;
        }
        let x = dist / r;
        let window = clamp(1.0 - x * x, 0.0, 1.0);
        let atten = window * window * window / (1.0 + dist * dist * 0.15);
        // A slight wrap, but nothing on faces turned away from the light.
        let ndl = clamp((dot(n, d / max(dist, 1e-4)) + 0.1) / 1.1, 0.0, 1.0);
        sum = sum + l.col.rgb * atten * ndl;
    }
    return sum * params.tune.x;
}

struct Out {
    @location(0) colour: vec4<f32>,
    // What the pixel reflects of new light, faded by the fog; zero where it is not relit.
    @location(1) surface: vec4<f32>,
};

fn out(c: vec4<f32>, s: vec4<f32>) -> Out {
    var o: Out;
    o.colour = c;
    o.surface = s;
    return o;
}

@fragment
fn fs_relight(@builtin(position) p: vec4<f32>) -> Out {
    let px = vec2<i32>(p.xy);
    let c = textureLoad(picture, px, 0);
    let nw = textureLoad(normals, px, 0);
    let d = textureLoad(depth, px, 0);
    let kind = nw.w;
    // The world's unlit scenery (the trees, drawn with their light baked into their vertices) is
    // lit again as foliage; bright unlit things (effects, glows) are left as drawn.
    let unlit_scenery = kind < 0.5 && luminance(c.rgb) < 0.6;
    if (d >= 1.0 || dot(nw.xyz, nw.xyz) < 0.25 || (kind < 0.5 && !unlit_scenery)) {
        return out(c, vec4<f32>(0.0));
    }
    // A lit thing drawn without the sun among its lights while the sun is up stands indoors:
    // in a building's or a dungeon's cell, lit by the cell's own lights. It keeps the light it
    // was drawn with, as the cell around it does.
    if (kind > 1.2 && kind < 1.3 && luminance(params.sun_legacy.rgb) > 0.01) {
        return out(c, vec4<f32>(0.0));
    }
    // Leaves and other cut-out growth: thin, lit from either side.
    let leafy = kind > 3.5 || unlit_scenery;
    let world = render_at(p.xy, d);
    let to_eye = params.eye.xyz - world;
    let dist = length(to_eye);
    let n = normalize((params.view_to_render * vec4<f32>(nw.xyz, 0.0)).xyz);
    let noise = ign(p.xy);
    var f = 0.0;
    if (params.fog.z > 0.5) {
        f = clamp((dist - params.fog.x) / max(params.fog.y - params.fog.x, 1e-3), 0.0, 1.0);
    }
    // The colour as drawn: the new light is faded out where the fog has taken it.
    let lit = c.rgb;
    let ndl_raw = dot(n, params.sun_dir.xyz);
    let ndl = max(ndl_raw, 0.0);
    let baked = kind > 1.5 && kind < 2.5;
    // How much of the sun a leaf was drawn with, which its class carries where it is known: its
    // normal here is its plant's, not the card's that the vertex light used.
    let share = select(0.55, clamp((kind - 4.1) / 0.3, 0.0, 1.0), kind > 4.05);
    var albedo: vec3<f32>;
    var legacy_lin = vec3<f32>(1.0);
    if (baked) {
        // An interior cell's light is its own; only the point lights are added.
        albedo = lit;
    } else {
        let legacy = min(
            params.amb_legacy.rgb + params.sun_legacy.rgb * select(ndl, share, leafy),
            vec3<f32>(1.0),
        );
        legacy_lin = pow(max(legacy, vec3<f32>(0.03)), vec3<f32>(2.2));
        // One grey scale for the surface's own colour, so a tinted ambient does not tint it.
        albedo = min(lit / max(luminance(legacy_lin), 0.02), vec3<f32>(1.0));
    }
    // The soft wrap below reaches a little past the terminator, so the shadow is looked up there
    // too: a face turned just away from a low sun is in its own building's shadow, not lit.
    let wrap = clamp((ndl_raw + 0.15) / 1.15, 0.0, 1.0);
    var shadow = 1.0;
    let ao = 1.0;
    if (!baked && (wrap > 0.0 || leafy)) {
        // A leaf facing away from the sun still sees it through itself: its visibility is taken
        // from the side the sun is on.
        let ns = select(n, -n, leafy && ndl_raw < 0.0);
        shadow = sun_shadow(world, ns, dist, noise);
        if (leafy) {
            // A canopy is never wholly dark: light finds its way between the leaves.
            shadow = 0.4 + 0.6 * shadow;
        } else {
            shadow = shadow * contact_shadow(world, dist, fract(noise + 0.37));
        }
    }
    let through = select(0.0, shadow, leafy && params.sun_legacy.w > 0.0);
    if (params.tune.w > 0.5 && params.tune.w < 1.5) {
        return out(vec4<f32>(vec3<f32>(shadow * ao), 1.0), vec4<f32>(0.0));
    }
    var light = vec3<f32>(0.0);
    if (baked) {
        light = vec3<f32>(1.0);
    } else {
        let sun_lin = pow(params.sun_legacy.rgb, vec3<f32>(2.2));
        let amb_lin = pow(params.amb_legacy.rgb, vec3<f32>(2.2));
        // Sky from above, a warm bounce from the ground below.
        let up = n.z * 0.5 + 0.5;
        let sky = amb_lin * vec3<f32>(0.85, 0.95, 1.15);
        let ground = amb_lin * 0.45 + sun_lin * vec3<f32>(0.10, 0.08, 0.05);
        let hemi = mix(ground, sky, up) * ao * params.amb_legacy.w;
        // A canopy takes the sky from all round, its crown more than its underside; however
        // deep the occlusion, light still finds its way between the leaves.
        let open_hemi = mix(ground, sky, 0.5 + 0.3 * n.z) * params.amb_legacy.w;
        let leaf_hemi = open_hemi * mix(0.5, 1.0, ao);
        if (params.tune.w > 1.5) {
            return out(vec4<f32>(hemi * 2.0, 1.0), vec4<f32>(0.0));
        }
        // The soft wrap keeps the terminator from being a hard line on low-polygon shapes.
        if (leafy) {
            // A leaf card carries its plant's normal: the crown facing the sun is brightest and
            // the far side falls off softly, and light through the leaves (below) lifts that
            // side again. Unlit scenery has only its card's face, so both faces take the sun
            // nearly alike (crossed cards facing either way meet along a line, and a large
            // difference shows it).
            let leaf_wrap = select(
                0.6 + 0.4 * clamp(ndl_raw * 0.5 + 0.5, 0.0, 1.0),
                0.3 + 0.7 * clamp((ndl_raw + 0.5) / 1.5, 0.0, 1.0),
                kind > 3.5,
            );
            light = leaf_hemi + sun_lin * params.sun_legacy.w * leaf_wrap * shadow * 0.8;
        } else {
            // Where the sun does not reach, the open sky still lights the surface: a share of
            // the sun's light, more from above, in place of what the bounced light would bring.
            let fill = sun_lin * params.sun_legacy.w * params.extra.z * (0.5 + 0.5 * up)
                * (1.0 - wrap * shadow);
            light = hemi + fill + sun_lin * params.sun_legacy.w * wrap * shadow;
            // A little sheen where the sun glances off toward the eye.
            let h = normalize(params.sun_dir.xyz + to_eye / max(dist, 1e-3));
            let spec = pow(max(dot(n, h), 0.0), 48.0) * 0.12 * shadow * ndl;
            light = light + sun_lin * params.sun_legacy.w * spec;
        }
    }
    // The new light over the old, applied to the colour as drawn (so whatever the vertex light
    // held beyond the sun and sky is kept), and the point lights on the surface's own colour.
    var ratio = clamp(light / legacy_lin, vec3<f32>(0.0), vec3<f32>(4.0));
    // How high the sun stands: 0 at the horizon and below, 1 from about twenty degrees up.
    let day = smoothstep(0.0, 0.35, params.sun_dir.z);
    if (leafy) {
        // A leaf takes the new light's brightness and little of its colour: the ratio by
        // luminance, each channel within a quarter of it, and less of it as the sun goes down.
        // Divided channel by channel, the dim tinted light of dusk would multiply the channel
        // it held least of (the green, under a purple sky) and the canopy would glow.
        let lum_ratio = min(
            luminance(light) / max(luminance(legacy_lin), 1e-4),
            mix(1.0, 2.0, day),
        );
        ratio = lum_ratio * clamp(ratio / max(lum_ratio, 1e-4), vec3<f32>(0.8), vec3<f32>(1.25));
    }
    var relit = select(lit * mix(ratio, vec3<f32>(1.0), f), lit, baked);
    relit = relit + albedo * point_light(world, n, baked) * (1.0 - f);
    let refl = select(min(lit / legacy_lin, vec3<f32>(2.0)) * (1.0 - f), vec3<f32>(0.0), baked);
    if (leafy && through > 0.0) {
        // Light through the leaf: a soft diffuse share from the far side, and a bright forward
        // lobe where the eye looks toward the sun through it, tinted deeper by the leaf.
        // Both fade as the sun goes down, and the tint is mostly the leaf's brightness, so a
        // low sun seen through a tree does not light it up.
        let e = to_eye / max(dist, 1e-3);
        let forward = pow(clamp(dot(-e, params.sun_dir.xyz), 0.0, 1.0), 5.0);
        let back = clamp(-ndl_raw, 0.0, 1.0);
        let grey = luminance(refl);
        let tint = mix(vec3<f32>(grey), refl * refl / max(grey, 0.04), 0.35);
        let sun_lin = pow(params.sun_legacy.rgb, vec3<f32>(2.2)) * params.sun_legacy.w;
        let amount = (back * 0.45 + forward * 1.6) * through * params.extra.y * day;
        relit = relit + clamp(tint, vec3<f32>(0.0), vec3<f32>(1.5)) * sun_lin * amount;
    }
    if (params.lamp.x > 0.5 && !baked) {
        let lp = textureLoad(lamp_light, px, 0);
        if (lp.w >= 0.0) {
            if (params.lamp.z > 0.5) {
                return out(vec4<f32>(lp.rgb * 0.5, 1.0), vec4<f32>(0.0));
            }
            // The lamps' light on the surface's own colour; the fog takes some of it with distance.
            // The colour is taken channel by channel out of the light it was drawn with, so the
            // night's tinted ambient does not tint the lamplight too.
            let per = min(lit / max(legacy_lin, vec3<f32>(0.02)), vec3<f32>(1.0));
            let own = mix(albedo, per, 0.6);
            relit = relit + own * lp.rgb * (1.0 - f * 0.7);
            // A lamp's own glass and flame glow.
            relit = relit + lit * lp.w * params.lamp.y;
        }
    }
    return out(vec4<f32>(relit, c.a), vec4<f32>(refl, 1.0));
}
