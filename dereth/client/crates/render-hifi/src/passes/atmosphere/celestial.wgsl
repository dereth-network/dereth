// The heavens drawn over the physical sky, where retail draws its own: retail's night layer,
// which blackens the sky at its hours, the stars over it as points of light, and the sun's disk
// and its glow where its light comes from; then the moons where retail places them, balls the
// sky makes itself with the art pack's pictures laid on them, lit by the sun as in open space;
// then the clouds over all of them. Drawn into the sky's own picture, blended as the sky's
// layers blend; the horizon's veil goes over it afterwards.
//
// Every pixel is worked out from the steady line through its middle, so the heavens hold still
// under the anti-aliasing's jitter; the stars twinkle with the game's clock alone.

@group(0) @binding(4) var cloud_picture: texture_2d<f32>;
@group(0) @binding(5) var moon_sampler: sampler;
// Each moon's ground (colour, height in alpha), its lights and its clouds (cover in alpha).
@group(0) @binding(6) var moon0: texture_2d<f32>;
@group(0) @binding(7) var moon1: texture_2d<f32>;
@group(0) @binding(8) var lamps0: texture_2d<f32>;
@group(0) @binding(9) var lamps1: texture_2d<f32>;
@group(0) @binding(10) var veil0: texture_2d<f32>;
@group(0) @binding(11) var veil1: texture_2d<f32>;

// Cells along each edge of the six faces the star field is laid on.
const STAR_CELLS: f32 = 160.0;
// The share of cells that hold a star.
const STAR_CHANCE: f32 = 0.42;
// How strongly a moon's height picture tilts its ground, against its radius.
const MOON_RELIEF: f32 = 0.03;
// The share of a moon's sunlight that falls off as a matte ball's; the rest its dust spreads
// evenly out to the limb.
const MOON_MATTE: f32 = 0.35;
// The light a moon's night side takes from the world it circles, at its brightest: when the
// moon is new and the world, seen from it, full.
const MOON_SHINE: f32 = 0.012;
// How far a moon rocks on its ball each way, radians.
const MOON_SWAY: f32 = 0.12;
// How bright a moon's own lights burn.
const MOON_LAMPS: f32 = 3.0;
// How much the air makes them flicker.
const MOON_FLICKER: f32 = 0.55;
// How far over a moon's ground its clouds lie, against its radius.
const MOON_CLOUD_HEIGHT: f32 = 0.02;
// How much of the sun a moon's clouds keep from the ground under them, at their thickest.
const MOON_CLOUD_SHADOW: f32 = 0.75;
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

// Moon `k`'s own frame as the eye sees it: east, north and toward the world it circles, its north
// held toward the zenith as its picture is painted, and rocking slowly on its ball as a moon held
// face-on to its world does. Overhead, where the zenith gives no way up, north is the way retail's
// card turns its picture up.
fn moon_frame(k: i32) -> mat3x3<f32> {
    let c = heavens.moon_center[k].xyz;
    let toward = -normalize(c);
    let v = heavens.moon_v[k].xyz;
    // Retail's picture's v runs down its card.
    let card_up = -normalize(v - toward * dot(v, toward));
    let up = vec3<f32>(0.0, 0.0, 1.0) - toward * toward.z;
    let north = normalize(mix(card_up, up / max(length(up), 1e-6), smoothstep(0.02, 0.1, length(up))));
    let east = cross(north, toward);
    let sway = MOON_SWAY * vec2<f32>(
        sin(6.2831853 * (heavens.stars.z + 0.37 * f32(k))),
        0.7 * sin(6.2831853 * (2.0 * heavens.stars.z + 0.21 + 0.5 * f32(k))),
    );
    let e1 = east * cos(sway.x) + toward * sin(sway.x);
    let t1 = toward * cos(sway.x) - east * sin(sway.x);
    let n2 = north * cos(sway.y) + t1 * sin(sway.y);
    let t2 = t1 * cos(sway.y) - north * sin(sway.y);
    return mat3x3<f32>(e1, n2, t2);
}

// Moon `k`'s radius, meters: as large as retail's picture draws its disk.
fn moon_radius(k: i32) -> f32 {
    return length(heavens.moon_u[k].xyz) * 0.48;
}

struct MoonOut {
    @builtin(position) position: vec4<f32>,
    // The point drawn, meters from the eye.
    @location(0) at: vec3<f32>,
    // Where it lies on the ball, in the moon's own frame.
    @location(1) local: vec3<f32>,
    // The moon, twice, and 1 for its clouds' shell over its ground.
    @location(2) @interpolate(flat) which: u32,
};

// The ball the sky makes for each moon, its ground and then its clouds' shell, each drawn a pixel
// and a half larger than it is so its edge can be smoothed over the pixel. Instance `i`: moon
// i / 2, its clouds when odd.
@vertex
fn vs_moon(@location(0) local: vec3<f32>, @builtin(instance_index) i: u32) -> MoonOut {
    let k = i32(i / 2u);
    let veil = (i & 1u) == 1u;
    let c = heavens.moon_center[k];
    var o: MoonOut;
    o.which = i;
    o.local = local;
    o.at = vec3<f32>(0.0);
    o.position = vec4<f32>(2.0, 2.0, 2.0, 1.0);
    if (c.w < 0.5 || (veil && heavens.moon_clouds[k] < 0.5)) {
        return o;
    }
    let r = moon_radius(k) * select(1.0, 1.0 + MOON_CLOUD_HEIGHT, veil) + 1.5 * heavens.stars.w * length(c.xyz);
    o.at = c.xyz + moon_frame(k) * local * r;
    let clip = heavens.clip_from_ray * vec4<f32>(o.at, 1.0);
    // Past all depth tests, as the sky is.
    o.position = vec4<f32>(clip.xy, 0.5 * clip.w, clip.w);
    return o;
}

// Moon `k`'s picture `layer` (0 its ground, 1 its lights, 2 its clouds) at `uv`, its steps over a
// pixel `dx` and `dy`.
fn moon_picture(k: i32, layer: i32, uv: vec2<f32>, dx: vec2<f32>, dy: vec2<f32>) -> vec4<f32> {
    if (layer == 0) {
        if (k == 0) {
            return textureSampleGrad(moon0, moon_sampler, uv, dx, dy);
        }
        return textureSampleGrad(moon1, moon_sampler, uv, dx, dy);
    }
    if (layer == 1) {
        if (k == 0) {
            return textureSampleGrad(lamps0, moon_sampler, uv, dx, dy);
        }
        return textureSampleGrad(lamps1, moon_sampler, uv, dx, dy);
    }
    if (k == 0) {
        return textureSampleGrad(veil0, moon_sampler, uv, dx, dy);
    }
    return textureSampleGrad(veil1, moon_sampler, uv, dx, dy);
}

// Where the point `p` of a moon's unit ball lies on its pictures, longitude by latitude.
fn moon_uv(p: vec3<f32>) -> vec2<f32> {
    return vec2<f32>(atan2(p.x, p.z) / 6.2831853 + 0.5, 0.5 - asin(clamp(p.y, -1.0, 1.0)) / 3.1415927);
}

// How bright a moon's light reads through the air at `uv` of its pictures now: each light's own
// flicker, two quick and one slow, whole cycles of the clock's period, the colors shifting a little
// apart, as a low star's do.
fn moon_flicker(k: i32, uv: vec2<f32>, low: f32) -> vec3<f32> {
    let cell = floor(uv * vec2<f32>(512.0, 256.0));
    let h0 = pcg(u32(cell.x) + u32(cell.y) * 512u + u32(k) * 262144u + 0x5bd1e995u);
    let h1 = pcg(h0);
    let ph = heavens.stars.z;
    let k1 = f32(900u + (h0 >> 8u) % 1500u);
    let k2 = f32(1400u + (h1 >> 8u) % 1700u);
    let k3 = f32(30u + (h1 >> 20u) % 90u);
    let p = vec3<f32>(0.0, 0.07, 0.15);
    let wave = 0.4 * sin(6.2831853 * (k1 * ph + unit(h0) + p))
        + 0.3 * sin(6.2831853 * (k2 * ph + unit(h1) - p))
        + 0.3 * sin(6.2831853 * (k3 * ph + unit(h0 >> 4u)));
    return max(vec3<f32>(1.0) + heavens.stars.y * MOON_FLICKER * low * wave, vec3<f32>(0.0));
}

// Light brighter than the picture holds rolled off with its hue kept, so a bright face keeps
// its shading rather than clipping flat.
fn moon_roll(x: vec3<f32>) -> vec3<f32> {
    let k = 0.7;
    let peak = max(x.r, max(x.g, x.b));
    if (peak <= k) {
        return x;
    }
    return x * ((k + (1.0 - k) * (1.0 - exp(-(peak - k) / (1.0 - k)))) / peak);
}

// A moon's ball, or its clouds' shell, where it is drawn: lit as in open space from the sun's
// side at the moon's own phase, its night side faintly lit by the world it circles and its own
// lights burning there; in the sky's encoding, premultiplied.
@fragment
fn fs_moon(i: MoonOut) -> @location(0) vec4<f32> {
    let p = normalize(i.local);
    // The pictures' steps over the pixel, taken from whichever side of the seam behind the ball
    // they are smaller on.
    let uv = moon_uv(p);
    let turned = vec2<f32>(fract(uv.x + 0.5), uv.y);
    let seam = abs(dpdx(turned).x) + abs(dpdy(turned).x) < abs(dpdx(uv).x) + abs(dpdy(uv).x);
    let dx = select(dpdx(uv), dpdx(turned), seam);
    let dy = select(dpdy(uv), dpdy(turned), seam);
    let k = i32(i.which / 2u);
    let veil = (i.which & 1u) == 1u;
    let frame = moon_frame(k);
    let c = heavens.moon_center[k].xyz;
    let d = normalize(i.at);
    let toward = -normalize(c);
    let n = frame * p;
    // Its far side is hidden behind its near.
    if (dot(n, d) >= 0.0) {
        discard;
    }
    // The disk's edge, smoothed over a pixel.
    let radius = moon_radius(k) * select(1.0, 1.0 + MOON_CLOUD_HEIGHT, veil);
    let ang = acos(clamp(dot(d, -toward), -1.0, 1.0));
    let edge = clamp((asin(min(radius / length(c), 1.0)) - ang) / max(heavens.stars.w, 1e-6) + 0.5, 0.0, 1.0);
    // The sun's light on it, from the sun's side at its own phase: toward the eye when full.
    let side = atmosphere.sun.xyz - toward * dot(atmosphere.sun.xyz, toward);
    let lean = select(frame[1], normalize(side), length(side) > 1e-3);
    let full = 2.0 * heavens.moon_light[k].z - 1.0;
    let light = normalize(toward * full + lean * sqrt(max(1.0 - full * full, 0.0)));
    let shine = MOON_SHINE * (0.2 + 0.8 * (1.0 - heavens.moon_light[k].z));
    let facing = dot(n, -d);
    var lin: vec3<f32>;
    var cover = 1.0;
    if (veil) {
        // Its clouds: their light spread out to the limb as the ground's is, and lit a little past
        // the line where the ground's day ends.
        let cl = moon_picture(k, 2, uv, dx, dy);
        let mu = dot(n, light);
        let lit = max(mu + 0.05, 0.0);
        let spread = 2.0 * lit / (lit + max(facing, 0.05));
        let day = mix(spread, lit, 0.5) * smoothstep(-0.05, 0.05, mu);
        lin = cl.rgb * (day * heavens.moon_light[k].x + shine * 0.5);
        cover = cl.a;
    } else {
        let ground = moon_picture(k, 0, uv, dx, dy);
        // Its height picture tilts the ground: a step across the pixel each way, at least a texel.
        let size = vec2<f32>(textureDimensions(moon0));
        let step = max(max(length(dx), length(dy)), 1.0 / size.x);
        let h = vec4<f32>(
            moon_picture(k, 0, uv + vec2<f32>(step, 0.0), dx, dy).a,
            moon_picture(k, 0, uv - vec2<f32>(step, 0.0), dx, dy).a,
            moon_picture(k, 0, uv - vec2<f32>(0.0, step), dx, dy).a,
            moon_picture(k, 0, uv + vec2<f32>(0.0, step), dx, dy).a,
        );
        let lon = (uv.x - 0.5) * 6.2831853;
        let lat = asin(clamp(p.y, -1.0, 1.0));
        let east = frame * vec3<f32>(cos(lon), 0.0, -sin(lon));
        let north = frame * vec3<f32>(-sin(lat) * sin(lon), cos(lat), -sin(lat) * cos(lon));
        let slope = vec2<f32>(
            (h.x - h.y) / (2.0 * step * 6.2831853 * max(cos(lat), 0.05)),
            (h.z - h.w) / (2.0 * step * 3.1415927),
        );
        let normal = normalize(n - MOON_RELIEF * (east * slope.x + north * slope.y));
        let matte = max(dot(normal, light), 0.0);
        let dust = 2.0 * matte / (matte + max(dot(normal, -d), 0.05)) * smoothstep(0.0, 0.15, matte);
        var day = mix(dust, matte, MOON_MATTE) * smoothstep(-0.01, 0.04, dot(n, light));
        // The shadow of its clouds, where the sun's line from the ground meets their shell.
        if (heavens.moon_clouds[k] > 0.5) {
            let sun = transpose(frame) * light;
            let rc = 1.0 + MOON_CLOUD_HEIGHT;
            let b = dot(p, sun);
            let t = -b + sqrt(max(b * b - (1.0 - rc * rc), 0.0));
            let over = moon_uv(normalize(p + sun * t));
            day *= 1.0 - MOON_CLOUD_SHADOW * moon_picture(k, 2, over, dx, dy).a;
        }
        // Its own lights on its night side, flickering through the air, and stronger low in the
        // sky where there is more of it.
        let low = 1.0 + 1.5 * (1.0 - smoothstep(0.0, 0.35, -toward.z));
        let lamps = moon_picture(k, 1, uv, dx, dy).rgb * MOON_LAMPS * heavens.moon_light[k].y
            * moon_flicker(k, uv, low) * (1.0 - smoothstep(0.0, 0.25, day)) * smoothstep(0.0, 0.05, facing);
        lin = ground.rgb * (day * heavens.moon_light[k].x + shine) + lamps;
    }
    let alpha = cover * edge * heavens.moon_light[k].w;
    return vec4<f32>(linear_to_srgb(moon_roll(max(lin, vec3<f32>(0.0)))) * alpha, alpha);
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
    let stars = star_light(d) * smoothstep(-0.02, 0.06, d.z);
    let sun = sun_light(d);
    // Retail's night layer, black, as much of it as its hours show; the stars and the sun over it.
    // The moons and then the clouds are drawn over these.
    let a = heavens.night.x;
    var rgb = sun + stars;
    rgb = rgb + vec3<f32>(dither(i.position.xy)) * a;
    return vec4<f32>(clamp(rgb, vec3<f32>(0.0), vec3<f32>(1.0)), clamp(a, 0.0, 1.0));
}

// The clouds over the heavens and the moons.
@fragment
fn fs_cloud_cover(i: FullscreenOut) -> @location(0) vec4<f32> {
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
    let rgb = ac * cloud + vec3<f32>(dither(i.position.xy)) * ac;
    return vec4<f32>(clamp(rgb, vec3<f32>(0.0), vec3<f32>(1.0)), ac);
}
