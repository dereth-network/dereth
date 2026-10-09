// The physical sky, drawn into the world where the authored sky dome was, under the clouds, the
// sun, the moons and the stars, which are drawn over it as they always are; and the haze's veil
// over the horizon, drawn over all of them once the sky is drawn and before the land.

// The exposed physical sky toward `dir`. Below the horizon the horizon's own colour is kept: the
// haze over the land covers it, and the dark ground-lit air under it would only show at the
// edge of the land.
fn physical_sky(dir: vec3<f32>) -> vec3<f32> {
    let r = eye_radius();
    let bottom = planet_radius();
    let horizon_cos = -sqrt(max(r * r - bottom * bottom, 0.0)) / r;
    let cos_zenith = max(dir.z, horizon_cos + 0.002);
    let uv = sky_view_uv(r, cos_zenith, light_view_cos(dir));
    return exposed_sky(textureSampleLevel(sky_table, sky_sampler, uv, 0.0).rgb);
}

@fragment
fn fs_sky(i: FullscreenOut) -> @location(0) vec4<f32> {
    let ndc = vec2<f32>(i.uv.x * 2.0 - 1.0, 1.0 - i.uv.y * 2.0);
    let dir = view_ray(ndc);
    let c = linear_to_srgb(physical_sky(dir)) + vec3<f32>(dither(i.position.xy));
    return vec4<f32>(clamp(c, vec3<f32>(0.0), vec3<f32>(1.0)), atmosphere.look.z);
}

// The haze over the horizon: the sky, its clouds and its lights are veiled toward the haze's
// light the nearer they are to the horizon, by day fully at and below it. At night the veil is
// darker and narrower than the haze over the land, so the hills keep their line against the sky.
@fragment
fn fs_veil(i: FullscreenOut) -> @location(0) vec4<f32> {
    let ndc = vec2<f32>(i.uv.x * 2.0 - 1.0, 1.0 - i.uv.y * 2.0);
    let dir = view_ray(ndc);
    let up = max(dir.z, 0.0);
    // In the low sun the glow climbs higher toward the sun, warming the clouds over it.
    let toward_sun = pow(0.5 + 0.5 * light_view_cos(dir), 4.0);
    let glow = atmosphere.sun_illuminance.w * toward_sun * 0.75 * exp(-up / (atmosphere.haze.z * 5.0));
    let band = max(atmosphere.haze.z, 1e-4);
    let day_veil = exp(-up / band);
    // At night the band is narrow over a dark sky, and a veil wholly opaque at and below the
    // horizon showed as a flat band between the far land's crest and the horizon. There the
    // veil begins two bands below the horizon and thins to nothing three bands above it, with
    // no edge anywhere.
    let x = clamp((dir.z + 2.0 * band) / (5.0 * band), 0.0, 1.0);
    let night_veil = (1.0 - x) * (1.0 - x) * (1.0 - x);
    let veil = max(mix(day_veil, night_veil, 1.0 - atmosphere.misc.x), glow);
    let light = haze_light(dir, 1.0) * atmosphere.haze_shape.z;
    let c = linear_to_srgb(light) + vec3<f32>(dither(i.position.xy));
    return vec4<f32>(clamp(c, vec3<f32>(0.0), vec3<f32>(1.0)), veil);
}
