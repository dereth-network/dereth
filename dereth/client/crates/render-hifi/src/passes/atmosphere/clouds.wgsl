// The clouds, drawn again where retail's cloud picture lays them: a layer of air over the eye,
// its cover read from retail's own picture as retail lays it over the world and drifts it with
// the game's clock, its edges broken up by fine noise fixed to the world, lit by the sun through
// itself and by the sky about it. Worked out at a share of the frame's pixels, once a frame,
// into a picture of their own: linear light over them, premultiplied, and how much of what lies
// behind them they hide.
//
// Every pixel steps through the layer at the same places frame after frame: nothing in them
// moves but with the game's clock.

@group(0) @binding(4) var detail: texture_3d<f32>;
@group(0) @binding(5) var wrap_sampler: sampler;
@group(0) @binding(6) var deck: texture_2d<f32>;

// How many times thicker than retail's picture's own see-through a column of cloud is, straight
// up, where its cover is thin and where it is thickest: retail's light cover stays light, and its
// thick cover closes into solid cloud.
const THIN: f32 = 1.0;
const THICK: f32 = 2.0;
// The longest stretch of the layer a line is stepped through, meters.
const MOST_SPAN: f32 = 30000.0;
// The fine detail's texels across one period.
const DETAIL_TEXELS: f32 = 64.0;

// How much of the layer's depth a column of cloud reaches, from its cover: thin cover keeps low
// and flat, thick cover climbs the whole depth.
fn column_top(c: f32) -> f32 {
    return mix(0.3, 1.0, sqrt(c));
}

// The fine detail at `q` (in its periods), read at level `fine` and faded toward its mean where
// it is finer than the footprint calls for.
fn billow(q: vec3<f32>, fine: f32) -> f32 {
    let n = textureSampleLevel(detail, wrap_sampler, q, fine).r;
    return mix(0.5, n, 1.0 - smoothstep(3.0, 5.0, fine));
}

// How much light a meter of the cloud takes at the point `p` (render space, meters) `h` of the
// way up the layer, read over a footprint `footprint` meters across: retail's picture at the
// level that calls for, and the fine detail at its own; `full` reads the finest detail too. A
// column straight up hides about as much as retail's picture there does, its cover broken into
// billows of three sizes.
fn extinction(p: vec3<f32>, h: f32, footprint: f32, full: bool) -> f32 {
    let lod = max(log2(footprint * abs(heavens.laying.y) * 256.0), 0.0);
    let uv = deck_uv(p.xy);
    let cover = textureSampleLevel(deck, wrap_sampler, uv, lod).a * heavens.cloud.y;
    if (cover < 0.01) {
        return 0.0;
    }
    // The detail, fixed to the world as the picture is, turning over slowly with the clock.
    let q = vec3<f32>((p.xy + heavens.drift.zw) / heavens.march.w, h * heavens.cloud.w / heavens.march.w)
        + vec3<f32>(0.0, 0.0, heavens.march.z);
    let fine = log2(max(footprint * DETAIL_TEXELS / heavens.march.w, 1.0));
    let detail_share = heavens.tint.w;
    let n1 = billow(q, fine);
    let n2 = billow(q * 4.0 + vec3<f32>(0.37, 0.61, 0.13), fine + 2.0);
    // Retail's cover broken into puffs, about as much of it as before, its edges drawn firmer;
    // where retail's picture shows clear sky it stays clear.
    let puffs = cover + (0.6 * n1 + 0.4 * n2 - 0.5) * detail_share * smoothstep(0.02, 0.35, cover);
    let c = smoothstep(0.08, 1.0, puffs);
    if (c < 0.01) {
        return 0.0;
    }
    let top = column_top(c);
    // A flat base and a rounded top.
    let shape = smoothstep(0.0, 0.08, h) * (1.0 - smoothstep(top * 0.45, top, h));
    if (shape < 0.01) {
        return 0.0;
    }
    // The smallest billows eat into the edges.
    var n3 = 0.5;
    if (full) {
        n3 = billow(q * 16.0 + vec3<f32>(0.71, 0.29, 0.53), fine + 4.0);
    }
    let erosion = detail_share * 0.9 * n3 * (1.0 - 0.5 * c);
    let s = clamp((shape - erosion) / max(1.0 - erosion, 0.05), 0.0, 1.0);
    let tau = -log(1.0 - 0.97 * c) * mix(THIN, THICK, c * c);
    let column = heavens.cloud.w * top * 0.7;
    return s * tau / max(column, 1.0);
}

// The Henyey-Greenstein phase, against an even spread's.
fn hg(mu: f32, g: f32) -> f32 {
    let g2 = g * g;
    return (1.0 - g2) / pow(max(1.0 + g2 - 2.0 * g * mu, 1e-4), 1.5);
}

// The texels a side of the clouds' shadow map.
const SHADOW_TEXELS: f32 = 1024.0;

// The share of the sun's light that comes through the clouds to the world under each texel of
// their shadow map: along the sun's line from the clouds' base up through their depth, in six
// steps, the cover read at the size of a texel.
@fragment
fn fs_cloud_shadow(i: FullscreenOut) -> @location(0) vec4<f32> {
    let map = heavens.shadow;
    if (map.z <= 0.0 || heavens.cloud.x < 0.5) {
        return vec4<f32>(1.0);
    }
    let sun = atmosphere.sun.xyz;
    let xy = map.xy + (vec2<f32>(i.uv.x, 1.0 - i.uv.y) * 2.0 - 1.0) * map.z;
    let depth = heavens.cloud.w;
    let texel = 2.0 * map.z / SHADOW_TEXELS;
    let rise = depth / max(sun.z, 0.05);
    var tau = 0.0;
    for (var k = 0.0; k < 6.0; k += 1.0) {
        let h = (k + 0.5) / 6.0;
        let along = h * rise;
        let p = vec3<f32>(xy + sun.xy * along, map.w + h * depth);
        tau += extinction(p, h, max(texel, rise / 6.0 * 0.25), false) * rise / 6.0;
    }
    return vec4<f32>(exp(-tau * heavens.night.w));
}

struct CloudOut {
    @location(0) color: vec4<f32>,
};

@fragment
fn fs_clouds(i: FullscreenOut) -> CloudOut {
    var out: CloudOut;
    out.color = vec4<f32>(0.0);
    let ndc = vec2<f32>(i.uv.x * 2.0 - 1.0, 1.0 - i.uv.y * 2.0);
    let d = view_ray(ndc);
    // The layer is over the eye; rays that never climb into it see none, and the land covers
    // them.
    if (heavens.cloud.x < 0.5 || d.z < -0.01) {
        return out;
    }
    let mu = max(d.z, 0.0);
    let base = heavens.cloud.z;
    let depth = heavens.cloud.w;
    let t0 = shell(mu, base);
    let t1 = shell(mu, base + depth);
    let fade = heavens.night.z;
    let steps = max(heavens.march.x, 1.0);
    // Low down the line runs through tens of kilometers of the layer; the first stretch of it is
    // what shows, the cover read ever coarser along it.
    let span = min(t1 - t0, MOST_SPAN);
    let dt = span / steps;
    // The haze's light toward the line: what the clouds fade into far off, as the land does.
    let haze = haze_light(d, 1.0);
    // Each pixel's own steady offset into its first step, so the steps do not band.
    let jitter = fract(52.9829189 * fract(dot(i.position.xy, vec2<f32>(0.06711056, 5.83715e-3))));
    let sun = atmosphere.sun.xyz;
    let cos_sun = dot(d, sun);
    let phase = mix(hg(cos_sun, 0.5), hg(cos_sun, -0.2), 0.5);
    let lit = heavens.cloud_sun.rgb;
    let ambient = heavens.cloud_ambient.rgb;
    let shade = heavens.cloud_ambient.w;
    // The clouds' own color: retail's picture's, read whole from its smallest level. Retail's are
    // one color all over, white on a fair day and cream on a wet one.
    let albedo = srgb_to_linear(textureSampleLevel(deck, wrap_sampler, vec2<f32>(0.5), 16.0).rgb);
    var transmittance = 1.0;
    var light = vec3<f32>(0.0);
    for (var k = 0.0; k < steps; k += 1.0) {
        let t = t0 + (k + jitter) * dt;
        let p = atmosphere.eye.xyz + d * t;
        let h = clamp((k + jitter) / steps * (span / max(t1 - t0, 1.0)), 0.0, 1.0);
        // How wide the pixel's cone is there, and how long the step: what the picture and the
        // detail are read over.
        let footprint = max(t * heavens.march.y, dt * 0.25 * (1.0 - mu));
        let sigma = extinction(p, h, footprint, true);
        if (sigma <= 0.0) {
            continue;
        }
        // The sun through the cloud toward it, in three widening steps.
        var tau = 0.0;
        var at = p;
        var ah = h;
        for (var j = 0; j < 3; j += 1) {
            let len = 90.0 * pow(3.0, f32(j));
            at = at + sun * len;
            ah = ah + sun.z * len / depth;
            if (ah > 1.0 || ah < 0.0) {
                break;
            }
            tau += extinction(at, ah, max(footprint, len * 0.5), false) * len;
        }
        // Light scattered many times keeps more of the sun than the beam alone.
        let beam = max(exp(-tau), 0.55 * exp(-0.25 * tau));
        // The dark rim a cloud's thin edge shows away from the sun, and its lit rim toward it.
        let powder = 1.0 - 0.6 * exp(-2.0 * sigma * 60.0);
        let direct = lit * beam * phase * powder;
        // The sky about the cloud lights its top more than its base, about as retail's deck is lit
        // between them.
        let sky = ambient * mix(1.0 - 0.15 * shade, 1.0 + 0.25 * shade, h);
        // Far off the clouds take the haze's light, as the land does, and hide what is behind
        // them as before: low down they meet the haze over the horizon with no gap.
        let step_light = mix((direct + sky) * albedo, haze, 1.0 - exp(-t / fade));
        let a = 1.0 - exp(-sigma * dt);
        light += transmittance * a * step_light;
        transmittance *= 1.0 - a;
        if (transmittance < 0.01) {
            break;
        }
    }
    // They reach down to the horizon, which the veil then covers; and they show as much as
    // retail's hours show its deck.
    let low = smoothstep(-0.01, 0.0, d.z) * heavens.night.w;
    out.color = vec4<f32>(light * low, (1.0 - transmittance) * low);
    return out;
}
