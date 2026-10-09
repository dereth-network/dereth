// The air between the eye and the land: the world replayed without its linear fog is dimmed by
// the air in front of it and lit by the light that air scatters toward the eye.
//
// Two layers of air act on every pixel of land. The atmosphere's own air tints far land toward
// the sky's colour at the horizon in that direction. The low haze, calibrated to the authored
// fog's range, fades the land into the haze's light, the same light the sky's horizon veil is
// drawn in; it lies thickest over low ground. Near the edge of the resident landscape the land
// fades a little further into the haze, by how close it lies to that edge, so the end of the
// world is softened the same way wherever the eye stands. The sky, which the horizon veil has
// already covered, is left exactly as it was drawn.

@group(0) @binding(3) var picture: texture_2d<f32>;
@group(0) @binding(4) var depth: texture_depth_2d;

// How far into the air a point `dist` metres away lies: nothing short of where the haze begins,
// then easing in over the haze's soft start, so the air takes hold gradually rather than at a
// line, and the near field it has only just reached is barely touched.
fn into_air(dist: f32) -> f32 {
    let x = max(dist - atmosphere.haze.x, 0.0);
    let soft = atmosphere.look.w;
    return select(x - 0.5 * soft, x * x / (2.0 * soft), x < soft);
}

// How much of the haze lies between the eye and a point `dist` metres away and `rise` metres
// higher, as the fraction of the point's light that comes through.
fn haze_transmittance(dist: f32, rise: f32) -> f32 {
    let scale = atmosphere.haze.y;
    let x = rise / scale;
    var density = 1.0;
    if (abs(x) > 1e-3) {
        density = (1.0 - exp(-x)) / x;
    }
    density = clamp(density, atmosphere.haze_shape.x, atmosphere.haze_shape.y);
    return exp(-haze_depth(dist) * density);
}

// The haze's optical depth to a point `dist` metres away at the eye's height: growing faster
// than the distance, so the near and middle ground stay crisp and the far land takes the haze.
fn haze_depth(dist: f32) -> f32 {
    let reach = atmosphere.tint.z;
    return atmosphere.haze_colour.w * reach * pow(into_air(dist) / reach, atmosphere.tint.y);
}

// The same per channel: the haze leans toward the air's own blue, so far land keeps its red
// longest and takes the sky's blue first.
fn haze_transmittance_rgb(dist: f32, rise: f32) -> vec3<f32> {
    let scale = atmosphere.haze.y;
    let x = rise / scale;
    var density = 1.0;
    if (abs(x) > 1e-3) {
        density = (1.0 - exp(-x)) / x;
    }
    density = clamp(density, atmosphere.haze_shape.x, atmosphere.haze_shape.y);
    let ratio = atmosphere.rayleigh.rgb / atmosphere.rayleigh.g;
    let lean = mix(vec3<f32>(1.0), ratio, atmosphere.tint.x);
    // Kept to the same brightness overall, so the lean changes the hue and not the depth.
    let weights = lean / dot(lean, vec3<f32>(0.2126, 0.7152, 0.0722));
    return exp(-haze_depth(dist) * density * weights);
}

// 1 well inside the resident landscape, falling toward its edge to leave a share, by how far
// along the way from the eye to the edge the point lies across the ground. It depends on where
// the point is, not on where it is seen in the picture.
fn edge_fade(p: vec3<f32>) -> f32 {
    let w = atmosphere.window;
    if (w.z <= w.x || w.w <= w.y) {
        return 1.0;
    }
    let e = atmosphere.eye.xy;
    let dh = p.xy - e;
    let len = length(dh);
    if (len < 1.0) {
        return 1.0;
    }
    let dir = dh / len;
    var tx = 1e9;
    var ty = 1e9;
    if (dir.x > 1e-5) {
        tx = (w.z - e.x) / dir.x;
    } else if (dir.x < -1e-5) {
        tx = (w.x - e.x) / dir.x;
    }
    if (dir.y > 1e-5) {
        ty = (w.w - e.y) / dir.y;
    } else if (dir.y < -1e-5) {
        ty = (w.y - e.y) / dir.y;
    }
    let exit = max(min(tx, ty), 1.0);
    return 1.0 - atmosphere.look.y * smoothstep(atmosphere.tables.z, 1.0, len / exit);
}

@fragment
fn fs_aerial(@builtin(position) position: vec4<f32>) -> @location(0) vec4<f32> {
    let px = vec2<i32>(position.xy);
    let c = textureLoad(picture, px, 0);
    let v = atmosphere.viewport;
    if (position.x < v.x || position.y < v.y || position.x >= v.x + v.z || position.y >= v.y + v.w) {
        return c;
    }
    let z = textureLoad(depth, px, 0);
    if (z >= 1.0) {
        return c;
    }
    let uv = (position.xy - v.xy) / v.zw;
    let ndc = vec2<f32>(uv.x * 2.0 - 1.0, 1.0 - uv.y * 2.0);
    let dir = view_ray(ndc);
    let lin = srgb_to_linear(c.rgb);
    let q = atmosphere.position_from_ndc * vec4<f32>(ndc, z, 1.0);
    let p = q.xyz / q.w;
    let dist = length(p - atmosphere.eye.xyz);
    // The atmosphere's air, at the eye's height and drawn thicker than the real air, from where
    // the haze begins, so the near field is drawn exactly as the world is.
    let air = medium(atmosphere.eye.w).extinction;
    let far = into_air(dist);
    let through_air = mix(vec3<f32>(1.0),
        exp(-air * (far * 0.001 * atmosphere.misc.y)), atmosphere.misc.x);
    let aerial = lin * through_air + horizon_sky(dir) * (vec3<f32>(1.0) - through_air);
    let haze = haze_light(dir, 1.0 - exp(-dist / atmosphere.haze_shape.w));
    let t = haze_transmittance_rgb(dist, p.z - atmosphere.eye.z);
    let out = aerial * t + haze * (vec3<f32>(1.0) - t);
    // The edge of the resident landscape fades toward the haze evenly to the eye rather than in
    // light, so a dark hill there softens instead of turning milky.
    let edge = edge_fade(p);
    let s = mix(linear_to_srgb(max(haze, vec3<f32>(0.0))), linear_to_srgb(max(out, vec3<f32>(0.0))), edge);
    // Dithered where the air changed the pixel, so its gradients do not band; a pixel the air
    // left alone comes back exactly as it was.
    let change = abs(s - c.rgb);
    let amount = clamp(max(change.x, max(change.y, change.z)) * 64.0, 0.0, 1.0);
    let d = s + vec3<f32>(dither(position.xy) * amount);
    return vec4<f32>(clamp(d, vec3<f32>(0.0), vec3<f32>(1.0)), c.a);
}
