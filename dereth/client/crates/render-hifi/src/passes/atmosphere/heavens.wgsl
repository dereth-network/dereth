// The Remastered sky: retail's clouds, sun, moons and stars drawn again over the physical sky,
// each where retail puts it and at retail's hours. What the clouds and the heavens share: the
// constants, and where retail's cloud picture lies over the world.

struct Heavens {
    // x 1 when the clouds are drawn again; y how much of the sky they cover against retail's
    // picture; z their base over the eye, meters; w their depth, meters.
    cloud: vec4<f32>,
    // Retail's cloud picture over the world: u = x + y * px, v = z + w * py, for a point
    // (px, py) of the world in meters, wrapped as `drift.zw` wraps it.
    laying: vec4<f32>,
    // xy how far retail's drift has carried the picture, in picture units within one; zw where
    // render space's origin lies in the world, meters, wrapped to the picture's period.
    drift: vec4<f32>,
    // rgb the sun's light on the clouds, linear; w unused.
    cloud_sun: vec4<f32>,
    // rgb the light the clouds take from all about them, linear: the light retail's own deck is
    // drawn with at this hour; w how much darker their bases are than their tops.
    cloud_ambient: vec4<f32>,
    // rgb the stars' tint, from the night's light, in the sky's encoding; w how much fine detail
    // breaks the clouds up.
    tint: vec4<f32>,
    // x the steps through the clouds' depth; y radians a texel of the clouds' picture spans; z
    // how far the fine detail has turned over, within its period; w the fine detail's size,
    // meters, across one period of its noise.
    march: vec4<f32>,
    // xyz toward the drawn sun; w the angular radius of its disk, radians.
    sun_disk: vec4<f32>,
    // rgb the sun's color, linear, brightest channel 1; w how strong its glow is.
    sun_color: vec4<f32>,
    // For each moon: xyz its center, meters from the eye; w 1 when it is drawn again.
    moon_center: array<vec4<f32>, 2>,
    // xyz the step across it for one unit of its picture's u, over its size; w its picture's u
    // at the center.
    moon_u: array<vec4<f32>, 2>,
    // xyz the step for one unit of v; w its picture's v at the center.
    moon_v: array<vec4<f32>, 2>,
    // rgb how bright it is drawn; w how opaque.
    moon_light: array<vec4<f32>, 2>,
    // x how bright the stars are, retail's night layer's hours in it; y how much they twinkle;
    // z the clock's place in the twinkle's period, 0..1; w radians a pixel of the frame spans.
    stars: vec4<f32>,
    // x how much of the sky retail's night layer blackens at this hour; y the daylight, 0 night
    // to 1 day; z how far the clouds fade into the distance, meters; w how much of retail's cloud
    // deck its hours leave showing.
    night: vec4<f32>,
    // The clouds' shadow map: xy its center, render space; z how far it reaches each way from
    // it; w the clouds' base, render space. All 0 when the clouds cast no shadows.
    shadow: vec4<f32>,
};

@group(0) @binding(3) var<uniform> heavens: Heavens;

// Retail's cloud picture's coordinates at the world point under render-space `p`, drifted.
fn deck_uv(p: vec2<f32>) -> vec2<f32> {
    let w = p + heavens.drift.zw;
    return vec2<f32>(
        heavens.laying.x + heavens.laying.y * w.x,
        heavens.laying.z + heavens.laying.w * w.y,
    ) + heavens.drift.xy;
}

// A well-mixed 32-bit value from `v`.
fn pcg(v: u32) -> u32 {
    let state = v * 747796405u + 2891336453u;
    let word = ((state >> ((state >> 28u) + 4u)) ^ state) * 277803737u;
    return (word >> 22u) ^ word;
}

fn unit(h: u32) -> f32 {
    return f32(h & 0xffffffu) / 16777216.0;
}

// How far along a ray climbing at `mu` (the sine of its elevation) from the eye, on the
// planet's surface, it meets the sphere `h` meters over the eye: the rationalized form, so the
// planet's size takes no precision from a cloud's few hundred meters.
fn shell(mu: f32, h: f32) -> f32 {
    let r = PLANET_METERS;
    let k = h * (2.0 * r + h);
    return k / max(r * mu + sqrt(max(r * r * mu * mu + k, 0.0)), 1e-3);
}

// The planet's radius in meters: the sky model's, for the clouds' curve to the horizon.
const PLANET_METERS: f32 = 6360000.0;
