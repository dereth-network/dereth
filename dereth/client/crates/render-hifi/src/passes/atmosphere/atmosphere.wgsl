// The atmosphere every sky shader shares: the frame's constants, the scattering medium, the
// planet's geometry and the parametrisations of the lookup tables.
//
// The sky model is a planet of radius `ground.w` kilometres under an atmosphere reaching
// `misc.z`, holding Rayleigh scatterers (air), Mie scatterers (haze) and an absorbing ozone
// layer. Distances in the sky model are kilometres; distances in the world are metres.

struct Atmosphere {
    // Normalised device coordinates to a render-space direction: the inverse of the projection
    // times the view's rotation alone.
    ray_from_ndc: mat4x4<f32>,
    // Normalised device coordinates and depth to a render-space position.
    position_from_ndc: mat4x4<f32>,
    // xyz the eye in render space (metres); w the eye's height over the planet, kilometres.
    eye: vec4<f32>,
    // The world viewport in target pixels: x, y, width, height.
    viewport: vec4<f32>,
    // xyz the unit direction toward the sun; w how overcast the day is, 0 clear to 1 grey.
    sun: vec4<f32>,
    // rgb the sun's illuminance as the sky is exposed for; w how far into the low sun of dawn or
    // dusk the day is, 0 to 1.
    sun_illuminance: vec4<f32>,
    // rgb Rayleigh scattering per kilometre at the ground; w = -1 / scale height (km).
    rayleigh: vec4<f32>,
    // x Mie scattering, y Mie extinction per kilometre at the ground; z = -1 / scale height;
    // w the Mie phase asymmetry.
    mie: vec4<f32>,
    // rgb ozone absorption per kilometre at the layer's peak; w unused.
    ozone: vec4<f32>,
    // rgb the ground's albedo; w the planet's radius, kilometres.
    ground: vec4<f32>,
    // rgb the low haze's colour, linear; w its extinction per metre at the eye's height.
    haze_colour: vec4<f32>,
    // x where the haze begins, metres; y its height scale, metres; z the width of the haze
    // band over the horizon, as the sine of an elevation; w how much brighter than the authored
    // fog colour it glows looking straight at the sun.
    haze: vec4<f32>,
    // The resident landscape's extent in render space: min x, min y, max x, max y.
    window: vec4<f32>,
    // x how much of the frame the physical sky replaces (0 the authored sky, 1 the physical);
    // y how many times thicker than the real air the air between the eye and the world is drawn;
    // z the atmosphere's outer radius, kilometres; w the frame number, for dither.
    misc: vec4<f32>,
    // xy the sky-view table's size; z where the edge fade of the landscape begins, as a fraction
    // of the way to the window's edge; w the sky-view raymarch's step count.
    tables: vec4<f32>,
    // x, y the least and the most the haze's density along a line of sight differs from its
    // density at the eye's height; z how bright the horizon's veil over the sky is against the
    // haze over the land; w the distance over which the haze's glow builds up, metres.
    haze_shape: vec4<f32>,
    // x how much of the haze's colour is the sky's colour at the horizon; y how much of the land
    // at the very edge of the resident landscape the haze takes; z how opaque the physical sky
    // is drawn over the authored dome; w the length of the air's soft start, metres.
    look: vec4<f32>,
    // x how far the haze's depth leans toward the blue, 0 grey to 1 the air's Rayleigh ratio;
    // y the power of distance the haze's depth grows as; z the length of air over which it
    // reaches its calibrated depth, metres; w unused.
    tint: vec4<f32>,
};

@group(0) @binding(0) var<uniform> atmosphere: Atmosphere;

const PI: f32 = 3.14159265358979;

fn planet_radius() -> f32 {
    return atmosphere.ground.w;
}

fn top_radius() -> f32 {
    return atmosphere.misc.z;
}

// The nearest non-negative distance along `d` (unit) from `o` to a sphere of radius `r` about
// the planet's centre, or -1 when the ray misses it.
fn ray_sphere(o: vec3<f32>, d: vec3<f32>, r: f32) -> f32 {
    let b = dot(o, d);
    let c = dot(o, o) - r * r;
    let disc = b * b - c;
    if (disc < 0.0) {
        return -1.0;
    }
    let s = sqrt(disc);
    let near = -b - s;
    let far = -b + s;
    if (near >= 0.0) {
        return near;
    }
    if (far >= 0.0) {
        return far;
    }
    return -1.0;
}

struct Medium {
    scattering: vec3<f32>,
    extinction: vec3<f32>,
    rayleigh: vec3<f32>,
    mie: f32,
};

// The scattering medium at height `h` kilometres over the ground.
fn medium(h: f32) -> Medium {
    let hh = max(h, 0.0);
    let rayleigh_density = exp(hh * atmosphere.rayleigh.w);
    let mie_density = exp(hh * atmosphere.mie.z);
    let ozone_density = max(0.0, 1.0 - abs(hh - 25.0) / 15.0);
    var m: Medium;
    m.rayleigh = atmosphere.rayleigh.rgb * rayleigh_density;
    m.mie = atmosphere.mie.x * mie_density;
    m.scattering = m.rayleigh + vec3<f32>(m.mie);
    m.extinction = m.rayleigh + vec3<f32>(atmosphere.mie.y * mie_density)
        + atmosphere.ozone.rgb * ozone_density;
    return m;
}

fn rayleigh_phase(mu: f32) -> f32 {
    return 3.0 / (16.0 * PI) * (1.0 + mu * mu);
}

// The Cornette-Shanks form of the Mie phase function.
fn mie_phase(mu: f32, g: f32) -> f32 {
    let g2 = g * g;
    let k = 3.0 / (8.0 * PI) * (1.0 - g2) / (2.0 + g2);
    return k * (1.0 + mu * mu) / pow(max(1.0 + g2 - 2.0 * g * mu, 1e-4), 1.5);
}

// The transmittance table maps (height, cosine of the zenith angle) so that both the ground
// and the horizon get resolution.
fn transmittance_uv(r: f32, mu: f32) -> vec2<f32> {
    let bottom = planet_radius();
    let top = top_radius();
    let h = sqrt(max(top * top - bottom * bottom, 0.0));
    let rho = sqrt(max(r * r - bottom * bottom, 0.0));
    let disc = r * r * (mu * mu - 1.0) + top * top;
    let d = max(0.0, -r * mu + sqrt(max(disc, 0.0)));
    let d_min = top - r;
    let d_max = rho + h;
    let x_mu = (d - d_min) / max(d_max - d_min, 1e-6);
    let x_r = rho / h;
    return vec2<f32>(x_mu, x_r);
}

fn transmittance_from_uv(uv: vec2<f32>) -> vec2<f32> {
    let bottom = planet_radius();
    let top = top_radius();
    let h = sqrt(max(top * top - bottom * bottom, 0.0));
    let rho = h * uv.y;
    let r = sqrt(rho * rho + bottom * bottom);
    let d_min = top - r;
    let d_max = rho + h;
    let d = d_min + uv.x * (d_max - d_min);
    var mu = 1.0;
    if (d > 0.0) {
        mu = (h * h - rho * rho - d * d) / (2.0 * r * d);
    }
    return vec2<f32>(r, clamp(mu, -1.0, 1.0));
}

// The sky-view table maps (cosine of the view's zenith angle, the angle to the sun about the
// vertical) with most of its rows near the horizon and most of its columns near the sun.
fn sky_view_uv(r: f32, view_zenith_cos: f32, light_view_cos: f32) -> vec2<f32> {
    let bottom = planet_radius();
    let v_horizon = sqrt(max(r * r - bottom * bottom, 0.0));
    let cos_beta = v_horizon / r;
    let beta = acos(clamp(cos_beta, -1.0, 1.0));
    let zenith_horizon = PI - beta;
    let view_zenith = acos(clamp(view_zenith_cos, -1.0, 1.0));
    var v: f32;
    if (view_zenith < zenith_horizon) {
        var c = view_zenith / zenith_horizon;
        c = 1.0 - c;
        c = sqrt(max(c, 0.0));
        c = 1.0 - c;
        v = c * 0.5;
    } else {
        var c = (view_zenith - zenith_horizon) / max(beta, 1e-6);
        c = sqrt(max(c, 0.0));
        v = c * 0.5 + 0.5;
    }
    var u = -light_view_cos * 0.5 + 0.5;
    u = sqrt(max(u, 0.0));
    return vec2<f32>(u, v);
}

// The inverse of `sky_view_uv`: (cosine of the view's zenith angle, cosine of the angle to the
// sun about the vertical).
fn sky_view_from_uv(r: f32, uv: vec2<f32>) -> vec2<f32> {
    let bottom = planet_radius();
    let v_horizon = sqrt(max(r * r - bottom * bottom, 0.0));
    let cos_beta = v_horizon / r;
    let beta = acos(clamp(cos_beta, -1.0, 1.0));
    let zenith_horizon = PI - beta;
    var view_zenith: f32;
    if (uv.y < 0.5) {
        var c = 2.0 * uv.y;
        c = 1.0 - c;
        c = c * c;
        c = 1.0 - c;
        view_zenith = zenith_horizon * c;
    } else {
        var c = uv.y * 2.0 - 1.0;
        c = c * c;
        view_zenith = zenith_horizon + beta * c;
    }
    let c = uv.x * uv.x;
    let light_view_cos = -(c * 2.0 - 1.0);
    return vec2<f32>(cos(view_zenith), light_view_cos);
}

// The eye's distance from the planet's centre, kept a little over the ground.
fn eye_radius() -> f32 {
    return planet_radius() + clamp(atmosphere.eye.w, 0.002, top_radius() - planet_radius() - 1.0);
}

// The cosine of `dir`'s angle to the sun about the vertical (+z).
fn light_view_cos(dir: vec3<f32>) -> f32 {
    let a = dir.xy;
    let b = atmosphere.sun.xy;
    let la = length(a);
    let lb = length(b);
    if (la < 1e-5 || lb < 1e-5) {
        return 1.0;
    }
    return clamp(dot(a, b) / (la * lb), -1.0, 1.0);
}

// A smooth roll-off for light brighter than the picture can hold: the identity while every
// channel is under `k`, then the brightest channel approaches 1 exponentially and the others
// follow it in proportion, going toward white the brighter the light is, as a bright sky goes
// white at the horizon. In the low sun it goes less toward white, so the glow of dawn and dusk
// stays orange rather than turning yellow.
fn shoulder(x: vec3<f32>) -> vec3<f32> {
    let k = 0.8;
    let peak = max(x.r, max(x.g, x.b));
    if (peak <= k) {
        return x;
    }
    let rolled = k + (1.0 - k) * (1.0 - exp(-(peak - k) / (1.0 - k)));
    let kept = x * (rolled / peak);
    let white = clamp((peak - k) / 1.2, 0.0, 1.0) * 0.75 * (1.0 - 0.6 * atmosphere.sun_illuminance.w);
    return mix(kept, vec3<f32>(rolled), white);
}

fn luma(c: vec3<f32>) -> f32 {
    return dot(c, vec3<f32>(0.2126, 0.7152, 0.0722));
}

// The sky's radiance, as exposed for the picture, from the sky-view table: given a little of the
// authored fog's cast, greyed toward that cast as the day is overcast, and rolled off where it
// is brighter than the picture can hold.
fn exposed_sky(raw: vec3<f32>) -> vec3<f32> {
    let lit = raw * atmosphere.sun_illuminance.rgb;
    let fog = atmosphere.haze_colour.rgb;
    let tone = clamp(fog / max(luma(fog), 1e-4), vec3<f32>(0.6), vec3<f32>(1.4));
    let grey = luma(lit) * tone;
    return shoulder(mix(lit, grey * 1.05, max(atmosphere.sun.w, 0.06)));
}

// Triangular noise of one step of an eight-bit channel, so gradients do not band.
fn dither(pixel: vec2<f32>) -> f32 {
    let seed = vec2<u32>(pixel) + vec2<u32>(u32(atmosphere.misc.w) * 17u, 0u);
    var h = seed.x * 1664525u + seed.y * 1013904223u + 374761393u;
    h = (h ^ (h >> 15u)) * 2246822519u;
    h = (h ^ (h >> 13u)) * 3266489917u;
    h = h ^ (h >> 16u);
    let a = f32(h & 0xffffu) / 65535.0;
    let b = f32(h >> 16u) / 65535.0;
    return (a + b - 1.0) / 255.0;
}
