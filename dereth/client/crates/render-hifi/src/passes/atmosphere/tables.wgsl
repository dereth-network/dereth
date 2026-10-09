// The sky's lookup tables, each drawn as a full-target triangle: the transmittance from any
// height toward any zenith angle, the light scattered more than once, and the sky as seen from
// the eye in every direction.

@group(0) @binding(1) var transmittance_table: texture_2d<f32>;
@group(0) @binding(2) var table_sampler: sampler;
@group(0) @binding(3) var multiple_table: texture_2d<f32>;

fn transmittance_to_top(r: f32, mu: f32) -> vec3<f32> {
    return textureSampleLevel(transmittance_table, table_sampler, transmittance_uv(r, mu), 0.0).rgb;
}

// Sunlight reaching a point at radius `r` whose sun is at zenith cosine `mu_sun`: none when the
// planet is in the way.
fn sunlight(r: f32, mu_sun: f32) -> vec3<f32> {
    let o = vec3<f32>(0.0, 0.0, r);
    let s = vec3<f32>(sqrt(max(1.0 - mu_sun * mu_sun, 0.0)), 0.0, mu_sun);
    if (ray_sphere(o, s, planet_radius()) > 0.0) {
        return vec3<f32>(0.0);
    }
    return transmittance_to_top(r, mu_sun);
}

fn multiple_scattering(r: f32, mu_sun: f32) -> vec3<f32> {
    let u = clamp(mu_sun * 0.5 + 0.5, 0.0, 1.0);
    let v = clamp((r - planet_radius()) / (top_radius() - planet_radius()), 0.0, 1.0);
    return textureSampleLevel(multiple_table, table_sampler, vec2<f32>(u, v), 0.0).rgb;
}

@fragment
fn fs_transmittance(i: FullscreenOut) -> @location(0) vec4<f32> {
    let rm = transmittance_from_uv(i.uv);
    let r = rm.x;
    let mu = rm.y;
    let o = vec3<f32>(0.0, 0.0, r);
    let d = vec3<f32>(sqrt(max(1.0 - mu * mu, 0.0)), 0.0, mu);
    let t_max = max(ray_sphere(o, d, top_radius()), 0.0);
    let steps = 40;
    var depth = vec3<f32>(0.0);
    let dt = t_max / f32(steps);
    for (var s = 0; s < steps; s = s + 1) {
        let p = o + d * ((f32(s) + 0.5) * dt);
        depth = depth + medium(length(p) - planet_radius()).extinction * dt;
    }
    return vec4<f32>(exp(-depth), 1.0);
}

// The second order of scattering from every direction over a sphere, and the fraction that
// scatters again, give the whole series of higher orders in closed form.
@fragment
fn fs_multiple(i: FullscreenOut) -> @location(0) vec4<f32> {
    let mu_sun = i.uv.x * 2.0 - 1.0;
    let r = planet_radius() + clamp(i.uv.y, 0.0, 1.0) * (top_radius() - planet_radius());
    let o = vec3<f32>(0.0, 0.0, max(r, planet_radius() + 0.001));
    let sun = vec3<f32>(sqrt(max(1.0 - mu_sun * mu_sun, 0.0)), 0.0, mu_sun);
    let isotropic = 1.0 / (4.0 * PI);
    let n = 8;
    var second = vec3<f32>(0.0);
    var again = vec3<f32>(0.0);
    for (var a = 0; a < n; a = a + 1) {
        for (var b = 0; b < n; b = b + 1) {
            let z = 1.0 - 2.0 * (f32(a) + 0.5) / f32(n);
            let phi = 2.0 * PI * (f32(b) + 0.5) / f32(n);
            let sz = sqrt(max(1.0 - z * z, 0.0));
            let d = vec3<f32>(sz * cos(phi), sz * sin(phi), z);
            let t_ground = ray_sphere(o, d, planet_radius());
            let t_top = ray_sphere(o, d, top_radius());
            var t_max = max(t_top, 0.0);
            if (t_ground > 0.0) {
                t_max = t_ground;
            }
            let steps = 20;
            let dt = t_max / f32(steps);
            var transmittance = vec3<f32>(1.0);
            var light = vec3<f32>(0.0);
            var fraction = vec3<f32>(0.0);
            for (var s = 0; s < steps; s = s + 1) {
                let p = o + d * ((f32(s) + 0.5) * dt);
                let pr = length(p);
                let m = medium(pr - planet_radius());
                let step_t = exp(-m.extinction * dt);
                let mu_p = dot(p / pr, sun);
                let scattered = m.scattering * sunlight(pr, mu_p) * isotropic;
                let ext = max(m.extinction, vec3<f32>(1e-7));
                light = light + transmittance * (scattered - scattered * step_t) / ext;
                fraction = fraction + transmittance * (m.scattering - m.scattering * step_t) / ext;
                transmittance = transmittance * step_t;
            }
            if (t_ground > 0.0) {
                let p = o + d * t_ground;
                let up = normalize(p);
                let mu_g = dot(up, sun);
                light = light + transmittance * sunlight(planet_radius(), mu_g)
                    * max(mu_g, 0.0) * atmosphere.ground.rgb / PI;
            }
            second = second + light;
            again = again + fraction;
        }
    }
    let count = f32(n * n);
    second = second / count;
    again = again / count;
    return vec4<f32>(second / max(vec3<f32>(1.0) - again, vec3<f32>(1e-4)), 1.0);
}

// The sky from the eye: single scattering of the sun by air and haze, with the higher orders
// from the table above, for unit sun illuminance.
@fragment
fn fs_sky_view(i: FullscreenOut) -> @location(0) vec4<f32> {
    let r = eye_radius();
    let angles = sky_view_from_uv(r, i.uv);
    let cos_zenith = angles.x;
    let cos_light = angles.y;
    let sin_zenith = sqrt(max(1.0 - cos_zenith * cos_zenith, 0.0));
    let sin_light = sqrt(max(1.0 - cos_light * cos_light, 0.0));
    let d = vec3<f32>(sin_zenith * cos_light, sin_zenith * sin_light, cos_zenith);
    let sun_z = clamp(atmosphere.sun.z, -1.0, 1.0);
    let sun = vec3<f32>(sqrt(max(1.0 - sun_z * sun_z, 0.0)), 0.0, sun_z);
    let o = vec3<f32>(0.0, 0.0, r);
    let t_ground = ray_sphere(o, d, planet_radius());
    var t_max = max(ray_sphere(o, d, top_radius()), 0.0);
    if (t_ground > 0.0) {
        t_max = t_ground;
    }
    t_max = min(t_max, 4000.0);
    let mu = dot(d, sun);
    let phase_r = rayleigh_phase(mu);
    let phase_m = mie_phase(mu, atmosphere.mie.w);
    let steps = i32(atmosphere.tables.w);
    var transmittance = vec3<f32>(1.0);
    var light = vec3<f32>(0.0);
    for (var s = 0; s < steps; s = s + 1) {
        // Steps grow with distance: short near the eye, long through the thin upper air.
        let f0 = f32(s) / f32(steps);
        let f1 = f32(s + 1) / f32(steps);
        let t0 = t_max * f0 * f0;
        let t1 = t_max * f1 * f1;
        let dt = t1 - t0;
        let p = o + d * (t0 + dt * 0.3);
        let pr = length(p);
        let m = medium(pr - planet_radius());
        let step_t = exp(-m.extinction * dt);
        let mu_p = dot(p / pr, sun);
        let single = (m.rayleigh * phase_r + vec3<f32>(m.mie * phase_m)) * sunlight(pr, mu_p);
        let multi = multiple_scattering(pr, mu_p) * m.scattering;
        let scattered = single + multi;
        let ext = max(m.extinction, vec3<f32>(1e-7));
        light = light + transmittance * (scattered - scattered * step_t) / ext;
        transmittance = transmittance * step_t;
    }
    return vec4<f32>(light, 1.0);
}
