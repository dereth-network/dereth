// The heavens drawn over the physical sky, once, where retail draws its own: retail's night
// layer, which blackens the sky at its hours, the stars over it as points of light, the moons
// where retail places them and lit as spheres from the sun's side, the sun's disk and its glow
// where its light comes from, and the clouds over all of them. Drawn into the sky's own picture, blended as the
// sky's layers blend; the horizon's veil goes over it afterwards.
//
// Every pixel is worked out from the steady line through its middle, so the heavens hold still
// under the anti-aliasing's jitter; the stars twinkle with the game's clock alone.

@group(0) @binding(4) var cloud_picture: texture_2d<f32>;
@group(0) @binding(5) var moon_sampler: sampler;
@group(0) @binding(6) var moon0: texture_2d<f32>;
@group(0) @binding(7) var moon1: texture_2d<f32>;

// Cells along each edge of the six faces the star field is laid on.
const STAR_CELLS: f32 = 160.0;
// The share of cells that hold a star.
const STAR_CHANCE: f32 = 0.42;
// How far toward the sun from the eye the light a moon is lit by comes from: at most a half,
// so a moon shows at least half its face lit.
const MOON_PHASE: f32 = 0.45;
// The twinkle's period, in whole cycles of the clock's period.
const TWINKLE_LEAST: u32 = 160u;
const TWINKLE_SPREAD: u32 = 260u;

// The face of the six that `d` points through, and where on it, -1..1 each way.
fn star_face(d: vec3<f32>) -> vec3<f32> {
    let a = abs(d);
    if (a.x >= a.y && a.x >= a.z) {
        return vec3<f32>(select(1.0, 0.0, d.x > 0.0), d.y / a.x, d.z / a.x);
    }
    if (a.y >= a.z) {
        return vec3<f32>(select(3.0, 2.0, d.y > 0.0), d.x / a.y, d.z / a.y);
    }
    return vec3<f32>(select(5.0, 4.0, d.z > 0.0), d.x / a.z, d.y / a.z);
}

// The direction through `st` (-1..1 each way) on face `face`.
fn star_direction(face: f32, st: vec2<f32>) -> vec3<f32> {
    let f = u32(face);
    let s = select(-1.0, 1.0, (f & 1u) == 0u);
    if (f < 2u) {
        return normalize(vec3<f32>(s, st.x, st.y));
    }
    if (f < 4u) {
        return normalize(vec3<f32>(st.x, s, st.y));
    }
    return normalize(vec3<f32>(st.x, st.y, s));
}

// The light of the star nearest `d`, if its cell holds one: a point a pixel across, its
// brightness and color its own, twinkling as the clock runs. Added to the sky in its encoding.
fn star_light(d: vec3<f32>) -> vec3<f32> {
    let f = star_face(d);
    let g = (f.yz * 0.5 + 0.5) * STAR_CELLS;
    let cell = floor(g);
    let id = u32(cell.x) + u32(cell.y) * 4096u + u32(f.x) * 16777216u;
    let h0 = pcg(id ^ 0x9e3779b9u);
    if (unit(h0) > STAR_CHANCE) {
        return vec3<f32>(0.0);
    }
    let h1 = pcg(h0);
    let h2 = pcg(h1);
    let h3 = pcg(h2);
    // Within the middle of its cell, so its light never reaches the next.
    let at = (cell + vec2<f32>(0.2 + 0.6 * unit(h1), 0.2 + 0.6 * unit(h2))) / STAR_CELLS * 2.0 - 1.0;
    let s = star_direction(f.x, at);
    let off = length(cross(d, s)) / max(heavens.stars.w, 1e-6);
    let m = unit(h3);
    let bright = 0.05 + 0.85 * pow(m, 6.0);
    let size = 0.45 + 0.25 * pow(m, 3.0);
    let spot = exp(-0.5 * (off * off) / (size * size));
    // The twinkle: two of the star's own frequencies, whole cycles of the clock's period, so the
    // period's wrap is never seen; stronger low in the sky.
    let k1 = f32(TWINKLE_LEAST + (h1 >> 8u) % TWINKLE_SPREAD);
    let k2 = f32(TWINKLE_LEAST + (h2 >> 8u) % TWINKLE_SPREAD);
    let ph = heavens.stars.z;
    let wave = 0.6 * sin(6.2831853 * (k1 * ph + unit(h3 >> 4u))) + 0.4 * sin(6.2831853 * (k2 * ph + unit(h1 >> 4u)));
    let low = 1.0 + 1.5 * (1.0 - smoothstep(0.0, 0.35, d.z));
    let twinkle = max(1.0 + heavens.stars.y * 0.3 * low * wave, 0.0);
    let tint = mix(vec3<f32>(0.78, 0.86, 1.0), vec3<f32>(1.0, 0.88, 0.74), unit(h2 >> 3u));
    return tint * heavens.tint.rgb * (bright * spot * twinkle * heavens.stars.x);
}

// Moon `k` toward `d`: its color in the sky's encoding, and how opaque it is there.
fn moon_light(k: i32, d: vec3<f32>) -> vec4<f32> {
    let c = heavens.moon_center[k];
    if (c.w < 0.5) {
        return vec4<f32>(0.0);
    }
    let u = heavens.moon_u[k].xyz;
    let v = heavens.moon_v[k].xyz;
    let n = cross(u, v);
    let dn = dot(d, n);
    if (abs(dn) < 1e-9) {
        return vec4<f32>(0.0);
    }
    let t = dot(c.xyz, n) / dn;
    if (t <= 0.0) {
        return vec4<f32>(0.0);
    }
    let p = d * t - c.xyz;
    let uv = vec2<f32>(heavens.moon_u[k].w + dot(p, u) / dot(u, u), heavens.moon_v[k].w + dot(p, v) / dot(v, v));
    if (any(uv < vec2<f32>(0.0)) || any(uv > vec2<f32>(1.0))) {
        return vec4<f32>(0.0);
    }
    // The disk, its edge smoothed over a pixel.
    let q = (uv - vec2<f32>(0.5)) * 2.0 / 0.96;
    let r = length(q);
    let pixel = heavens.stars.w * t / (length(u) * 0.48);
    let edge = clamp((1.0 - r) / max(pixel, 1e-4) + 0.5, 0.0, 1.0);
    // The picture's level the pixel's footprint calls for.
    let texels = heavens.stars.w * t / length(u) * 256.0;
    let lod = max(log2(texels), 0.0);
    var tex: vec4<f32>;
    if (k == 0) {
        tex = textureSampleLevel(moon0, moon_sampler, uv, lod);
    } else {
        tex = textureSampleLevel(moon1, moon_sampler, uv, lod);
    }
    // Lit as a sphere from the sun's side: the light comes from part of the way between the eye
    // and the sun, so a moon is never darker than half lit and its lit side faces the sun; the
    // night side keeps a faint glow of its own.
    let nz = sqrt(max(1.0 - min(r * r, 1.0), 0.0));
    let toward = -normalize(c.xyz);
    let normal = normalize(normalize(u) * q.x + normalize(v) * q.y + toward * nz);
    let light = normalize(mix(toward, atmosphere.sun.xyz, MOON_PHASE));
    let sun = dot(normal, light);
    let shade = 0.08 + 1.2 * smoothstep(-0.1, 0.4, sun);
    let lin = srgb_to_linear(tex.rgb) * shade * heavens.moon_light[k].rgb;
    let alpha = min(tex.a, edge) * heavens.moon_light[k].w;
    return vec4<f32>(linear_to_srgb(max(lin, vec3<f32>(0.0))), alpha);
}

// The sun's disk and its glow toward `d`, added to the sky in its encoding.
fn sun_light(d: vec3<f32>) -> vec3<f32> {
    let s = heavens.sun_disk.xyz;
    let r = heavens.sun_disk.w;
    let ang = atan2(length(cross(d, s)), dot(d, s));
    let pixel = heavens.stars.w;
    let disk = clamp((r - ang) / max(pixel, 1e-6) + 0.5, 0.0, 1.0);
    let x = min(ang / max(r, 1e-6), 1.0);
    let limb = 0.62 + 0.38 * sqrt(max(1.0 - x * x, 0.0));
    let col = heavens.sun_color.rgb;
    let corona = exp(-max(ang - r, 0.0) / (r * 1.3)) * 0.35;
    let glow = exp(-ang / 0.08) * 0.12 + exp(-ang / 0.32) * 0.04;
    // It sinks behind the horizon's line.
    let horizon = smoothstep(-0.004, 0.004, d.z);
    let lit = disk * limb * mix(col, vec3<f32>(1.0), 0.7) + (corona + glow) * heavens.sun_color.w * col;
    return lit * horizon * heavens.night.y;
}

@fragment
fn fs_heavens(i: FullscreenOut) -> @location(0) vec4<f32> {
    let ndc = vec2<f32>(i.uv.x * 2.0 - 1.0, 1.0 - i.uv.y * 2.0);
    let d = view_ray(ndc);
    // The clouds, enlarged from their own picture and softened over a texel each way, which
    // hides the steady offsets their steps were taken at: premultiplied linear light, and their
    // cover.
    let texel = 0.5 / vec2<f32>(textureDimensions(cloud_picture));
    let cl = 0.2 * textureSampleLevel(cloud_picture, sky_sampler, i.uv, 0.0)
        + 0.2 * textureSampleLevel(cloud_picture, sky_sampler, i.uv + vec2<f32>(texel.x, texel.y), 0.0)
        + 0.2 * textureSampleLevel(cloud_picture, sky_sampler, i.uv + vec2<f32>(-texel.x, texel.y), 0.0)
        + 0.2 * textureSampleLevel(cloud_picture, sky_sampler, i.uv + vec2<f32>(texel.x, -texel.y), 0.0)
        + 0.2 * textureSampleLevel(cloud_picture, sky_sampler, i.uv - texel, 0.0);
    let ac = clamp(cl.a, 0.0, 1.0);
    // Rolled off where brighter than the sky's picture holds, so a bright cloud keeps its shape.
    let cloud = linear_to_srgb(shoulder(max(cl.rgb / max(ac, 1e-4), vec3<f32>(0.0))));
    let m0 = moon_light(0, d);
    let m1 = moon_light(1, d);
    let stars = star_light(d) * smoothstep(-0.02, 0.06, d.z);
    let sun = sun_light(d);
    // Layered from the sky up: retail's night layer, black, as much of it as its hours show;
    // the stars over it; the moons; the sun; the clouds.
    let night = heavens.night.x;
    let behind_moons = m1.rgb * m1.a + (1.0 - m1.a) * (m0.rgb * m0.a + (1.0 - m0.a) * stars);
    let moons_a = 1.0 - (1.0 - m0.a) * (1.0 - m1.a);
    var rgb = ac * cloud + (1.0 - ac) * (sun + behind_moons);
    let a = 1.0 - (1.0 - ac) * (1.0 - moons_a) * (1.0 - night);
    rgb = rgb + vec3<f32>(dither(i.position.xy)) * a;
    return vec4<f32>(clamp(rgb, vec3<f32>(0.0), vec3<f32>(1.0)), clamp(a, 0.0, 1.0));
}
