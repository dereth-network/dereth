// The air between the eye and the world as the sky and the land both see it: the sky toward the
// horizon and the light of the low haze. Shared by the sky's horizon veil and the air over the
// land, so the land fades into exactly the colour the sky has behind it.

@group(0) @binding(1) var sky_table: texture_2d<f32>;
@group(0) @binding(2) var sky_sampler: sampler;

// A render-space direction from normalised device coordinates.
fn view_ray(ndc: vec2<f32>) -> vec3<f32> {
    let p = atmosphere.ray_from_ndc * vec4<f32>(ndc, 0.5, 1.0);
    return normalize(p.xyz / p.w);
}

// The exposed sky radiance just over the horizon toward `dir`'s azimuth, or a little higher
// when `dir` looks up: what the air in front of the land scatters toward the eye.
fn horizon_sky(dir: vec3<f32>) -> vec3<f32> {
    let r = eye_radius();
    let uv = sky_view_uv(r, max(dir.z, 0.01), light_view_cos(dir));
    return exposed_sky(textureSampleLevel(sky_table, sky_sampler, uv, 0.0).rgb);
}

// The low haze's own light toward `dir`. By day it is mostly the sky's own colour at the
// horizon in that direction, so the distance is lit by the same sky that is drawn over it; the
// rest is the authored fog colour, glowing warmer and brighter toward the sun. In the low sun
// the sky's colour is taken only toward the sun, and away from it the authored fog's colour
// stays. `reach` is how much of the glow the air in front of the point has built up, 0 to 1.
fn haze_light(dir: vec3<f32>, reach: f32) -> vec3<f32> {
    let g = 0.6;
    let mu = dot(dir, atmosphere.sun.xyz);
    // The Henyey-Greenstein lobe, 1 looking straight at the sun.
    let lobe = pow((1.0 + g * g - 2.0 * g) / max(1.0 + g * g - 2.0 * g * mu, 1e-4), 1.5);
    let sun = atmosphere.sun_illuminance.rgb;
    let warm = sun / max(max(sun.r, max(sun.g, sun.b)), 1e-4);
    let fog = atmosphere.haze_colour.rgb;
    let authored = fog * mix(vec3<f32>(1.0), warm, lobe) * (1.0 + atmosphere.haze.w * lobe * reach);
    let toward_sun = pow(0.5 + 0.5 * light_view_cos(dir), 3.0);
    let share = atmosphere.look.x * mix(1.0, toward_sun, atmosphere.sun_illuminance.w);
    return shoulder(mix(authored, horizon_sky(dir), share));
}
