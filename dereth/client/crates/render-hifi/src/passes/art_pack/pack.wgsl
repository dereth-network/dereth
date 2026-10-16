// An art pack's models, drawn into the re-shaded world's two targets in place of the objects they
// replace, and into the sun's shadow cascades.
//
// The re-shaded colour of every other surface is the ordinary frame's: its colour under the
// light its vertices were drawn with. The light per pixel divides that light back out of it and
// puts the new light in. So a model's colour here is its own colour under the same old light,
// worked out from its normal exactly as the light per pixel works it out, and the light per
// pixel lights it as it lights everything else. A lit surface takes the old light as the world's
// objects took it, with their brighter ambient, which the light per pixel leaves in: it comes out
// as bright as the object it replaces. Its normal target carries its class: a lit
// surface with its roughness and whether it is a metal, or cut-out growth with how much of the
// sun it was drawn with and how open its place in its crown is to the sky.

struct Frame {
    // Render space, height up, to clip space.
    clip_from_render: mat4x4<f32>,
    // Render space to view space, for the normal target.
    view_from_render: mat4x4<f32>,
    // xyz the eye.
    eye: vec4<f32>,
    // xyz toward the sun.
    sun_dir: vec4<f32>,
    // The sun as the old light took it, 0..1 a channel.
    sun_legacy: vec4<f32>,
    // The ambient light as the old light took it.
    amb_legacy: vec4<f32>,
    // x the frame's place in the anti-aliasing's dither of the edges of cut-outs (1 and over),
    // nought on a frame drawn without it.
    taa: vec4<f32>,
    // The ambient light the world's objects took from the old light: the landscape's and a share
    // of the sun's more.
    amb_objects: vec4<f32>,
};

struct Material {
    base_color: vec4<f32>,
    // x metalness, y roughness, z normal strength, w occlusion strength.
    pbr: vec4<f32>,
    // x the alpha a pixel is cut below (0: none), y 1 for cut-out growth, z 1 when both faces
    // are drawn.
    flags: vec4<f32>,
    // The crown of the model's leaves, for cut-out growth: xy the axis it stands round, z the
    // height of its lowest leaves and w of its highest, in the model's own space; all nought for
    // none.
    crown_frame: vec4<f32>,
    // How far the leaves reach from the axis in each of eight bands of height, lowest first.
    crown: array<vec4<f32>, 2>,
    // A level of pictures of the model seen from round it: x how many views, y the tiles to a
    // row of its pictures, z the rows; all nought for any other level.
    views: vec4<f32>,
    // Its quad: x half its width, y the height of its foot, z of its top.
    quad: vec4<f32>,
};

struct Caster {
    // Render space to the cascade's clip space.
    light_from_render: mat4x4<f32>,
};

@group(0) @binding(0) var<uniform> frame: Frame;
@group(0) @binding(1) var<uniform> caster: Caster;
@group(1) @binding(0) var<uniform> material: Material;
@group(1) @binding(1) var base_tex: texture_2d<f32>;
@group(1) @binding(2) var normal_tex: texture_2d<f32>;
@group(1) @binding(3) var metal_rough_tex: texture_2d<f32>;
@group(1) @binding(4) var occlusion_tex: texture_2d<f32>;
@group(1) @binding(5) var samp: sampler;

struct Vertex {
    @location(0) position: vec3<f32>,
    @location(1) normal: vec3<f32>,
    @location(2) tangent: vec4<f32>,
    @location(3) uv: vec2<f32>,
    @location(4) color: vec4<f32>,
};

// The model's placement: the three rows of its model-to-render transform, and in `keep.xy` the
// values of the screen's dither its level keeps (all of 0..1 away from a change of level).
struct Instance {
    @location(5) r0: vec4<f32>,
    @location(6) r1: vec4<f32>,
    @location(7) r2: vec4<f32>,
    @location(8) keep: vec4<f32>,
};

fn place(i: Instance, p: vec3<f32>) -> vec3<f32> {
    let q = vec4<f32>(p, 1.0);
    return vec3<f32>(dot(i.r0, q), dot(i.r1, q), dot(i.r2, q));
}

fn turn(i: Instance, d: vec3<f32>) -> vec3<f32> {
    return vec3<f32>(dot(i.r0.xyz, d), dot(i.r1.xyz, d), dot(i.r2.xyz, d));
}

const TAU: f32 = 6.28318530718;

struct VsOut {
    @builtin(position) position: vec4<f32>,
    @location(0) world: vec3<f32>,
    @location(1) normal: vec3<f32>,
    @location(2) tangent: vec4<f32>,
    @location(3) uv: vec2<f32>,
    @location(4) color: vec4<f32>,
    // The point in the model's own space.
    @location(5) local: vec3<f32>,
    // The values of the screen's dither the level keeps.
    @location(6) @interpolate(flat) keep: vec2<f32>,
};

@vertex
fn vs_main(v: Vertex, i: Instance) -> VsOut {
    var o: VsOut;
    let world = place(i, v.position);
    o.position = frame.clip_from_render * vec4<f32>(world, 1.0);
    o.world = world;
    o.local = v.position;
    o.normal = turn(i, v.normal);
    o.tangent = vec4<f32>(turn(i, v.tangent.xyz), v.tangent.w);
    o.uv = v.uv;
    o.color = v.color;
    o.keep = i.keep.xy;
    return o;
}

struct Out {
    @location(0) colour: vec4<f32>,
    @location(1) normal: vec4<f32>,
};

// How far the crown's leaves reach from its axis in band `k`.
fn crown_reach(k: u32) -> f32 {
    return material.crown[k / 4u][k % 4u];
}

// How open a leaf at `local` (the model's own space) is to the sky, 1 on the outside of its
// crown to 0 deep inside: near the axis for its height, or under a part of the crown that
// reaches farther out than it does. 1 where the model has no crown.
fn crown_open(local: vec3<f32>) -> f32 {
    let f = material.crown_frame;
    let span = f.w - f.z;
    if (span <= 1e-3) {
        return 1.0;
    }
    let r = length(local.xy - f.xy);
    let t = clamp((local.z - f.z) / span * 8.0 - 0.5, 0.0, 7.0);
    let k = u32(floor(t));
    let reach = mix(crown_reach(k), crown_reach(min(k + 1u, 7u)), t - floor(t));
    let inward = clamp(1.0 - r / max(reach, 0.05), 0.0, 1.0);
    // The widest band above: a leaf it overhangs is in its shade.
    var over = 0.0;
    for (var j: u32 = k + 1u; j < 8u; j = j + 1u) {
        over = max(over, crown_reach(j));
    }
    let under = clamp((over - r) / max(over, 0.05), 0.0, 1.0);
    return 1.0 - max(inward, under * 0.7);
}

// A leaf's class: 5, and over 256 its code, sixteen steps of the sun it was drawn with (`share`,
// one of those steps already) and sixteen of how open its place in its crown is (`open`, a step
// 0..15). The normal target's half floats hold every such class exactly.
fn leaf_class(share: f32, open: f32) -> f32 {
    return 5.0 + (round(share * 15.0) * 16.0 + open) / 256.0;
}

// The light a surface facing `ndl` of the sun took from the old light, linear, as the light per
// pixel works it out.
fn legacy_light(share: f32) -> vec3<f32> {
    let legacy = min(frame.amb_legacy.rgb + frame.sun_legacy.rgb * share, vec3<f32>(1.0));
    return pow(max(legacy, vec3<f32>(0.03)), vec3<f32>(2.2));
}

// The light an object's surface facing `ndl` of the sun took from the old light, linear: as
// `legacy_light`, with the objects' ambient.
fn object_light(share: f32) -> vec3<f32> {
    let legacy = min(frame.amb_objects.rgb + frame.sun_legacy.rgb * share, vec3<f32>(1.0));
    return pow(max(legacy, vec3<f32>(0.03)), vec3<f32>(2.2));
}

// The frame's dither for the edges of cut-outs at `at` (a pixel), 0..1, as the re-shaded
// world's: a pattern that spreads its values evenly over any few pixels, moved each frame.
fn dither_at(at: vec2<f32>) -> f32 {
    let k = floor(fract(frame.taa.x) * 64.0);
    let q = at + vec2<f32>(5.588238 * k);
    return fract(52.9829189 * fract(dot(q, vec2<f32>(0.06711056, 5.83715e-3))));
}

// How much of the alpha's range a cut-out's edge is dithered across on a frame drawn for the
// anti-aliasing, as the re-shaded world's.
const DITHER_BAND: f32 = 0.2;

// A cut-out's alpha at `at` as its cutoff is tested against: on a frame drawn for the
// anti-aliasing, moved by the frame's dither, so frames of the edge average into a soft one.
fn dithered_alpha(a: f32, at: vec2<f32>) -> f32 {
    return a + (0.5 - dither_at(at)) * DITHER_BAND * step(1.0, frame.taa.x);
}

@fragment
fn fs_main(in: VsOut, @builtin(front_facing) front: bool) -> Out {
    // Where one level of detail fades into the next, each keeps its share of the dither, the
    // two shares together all of it. (A dither of its own, apart from the cut-outs'.)
    let fading = dither_at(in.position.xy + vec2<f32>(37.0, 17.0));
    if (fading < in.keep.x || fading >= in.keep.y) {
        discard;
    }
    let base = textureSample(base_tex, samp, in.uv) * material.base_color * in.color;
    // Cut before the other pictures are read: a cut pixel reads them only as a helper of the
    // pixels round it.
    if (material.flags.x > 0.0 && dithered_alpha(base.a, in.position.xy) < material.flags.x) {
        discard;
    }
    let m = textureSample(normal_tex, samp, in.uv).xyz * 2.0 - vec3<f32>(1.0);
    let mr = textureSample(metal_rough_tex, samp, in.uv);
    let ao = textureSample(occlusion_tex, samp, in.uv).r;
    let foliage = material.flags.y > 0.5;
    var n = normalize(in.normal);
    var t = in.tangent.xyz - n * dot(n, in.tangent.xyz);
    t = select(vec3<f32>(0.0), normalize(t), dot(t, t) > 1e-8);
    // The back of a two-sided surface faces the other way. A leaf card keeps its plant's
    // normal on both faces: the crown's outward lean is what lights it.
    if (!front && !foliage) {
        n = -n;
        t = -t;
    }
    let b = cross(n, t) * select(-1.0, 1.0, in.tangent.w >= 0.0);
    let bent = vec3<f32>(m.xy * material.pbr.z, max(m.z, 1e-3));
    if (dot(t, t) > 0.5) {
        n = normalize(t * bent.x + b * bent.y + n * bent.z);
    }
    let metal = clamp(material.pbr.x * mr.b, 0.0, 1.0);
    let rough = clamp(material.pbr.y * mr.g, 0.0, 1.0);
    let ndl = dot(n, frame.sun_dir.xyz);
    // How much of the sun the surface is drawn with: for a leaf, which its class carries in
    // sixteen steps, so the step is the one it is drawn with.
    let leaf_share = round(clamp(ndl * 0.5 + 0.5, 0.0, 1.0) * 15.0) / 15.0;
    let share = select(max(ndl, 0.0), leaf_share, foliage);
    // The occlusion darkens the light from the sky and the ground, not the sun's: here the
    // colour carries both, so it is applied by the share of the old light the sun is not.
    let weights = vec3<f32>(0.2126, 0.7152, 0.0722);
    let sun_part = dot(frame.sun_legacy.rgb, weights) * share;
    let sky_part = dot(select(frame.amb_objects.rgb, frame.amb_legacy.rgb, foliage), weights);
    let direct = sun_part / max(sun_part + sky_part, 1e-3);
    let occlusion = mix(mix(1.0, ao, material.pbr.w), 1.0, direct);
    let albedo = base.rgb * occlusion;
    var o: Out;
    let view_n = normalize((frame.view_from_render * vec4<f32>(n, 0.0)).xyz);
    if (foliage) {
        return leaf_out(albedo, n, in.local, in.position.xy);
    } else {
        o.colour = vec4<f32>(albedo * object_light(share), 1.0);
        // A lit surface carrying its roughness: 1.02..1.18 for a dielectric, 1.32..1.48 for a
        // metal.
        let code = select(1.02, 1.32, metal > 0.5) + 0.16 * rough;
        o.normal = vec4<f32>(view_n, code);
    }
    return o;
}

// A leaf of colour `albedo`, facing `n` (render space), at `local` in its model's own space,
// drawn at the pixel `at`: its colour under the old light, and its class carrying the sun it was
// drawn with and how open its place in its crown is.
fn leaf_out(albedo: vec3<f32>, n: vec3<f32>, local: vec3<f32>, at: vec2<f32>) -> Out {
    let share = round(clamp(dot(n, frame.sun_dir.xyz) * 0.5 + 0.5, 0.0, 1.0) * 15.0) / 15.0;
    var o: Out;
    o.colour = vec4<f32>(albedo * legacy_light(share), 1.0);
    let view_n = normalize((frame.view_from_render * vec4<f32>(n, 0.0)).xyz);
    // How open the leaf is, in sixteen steps dithered so the steps leave no bands.
    let noise = fract(52.9829189 * fract(dot(at, vec2<f32>(0.06711056, 5.83715e-3))));
    let open = clamp(floor(crown_open(local) * 15.0 + noise), 0.0, 15.0);
    o.normal = vec4<f32>(view_n, leaf_class(share, open));
    return o;
}

// A level of pictures of the model seen from round it: an upright quad across the model's axis,
// turned to face the eye, showing the two views nearest the eye's bearing.

struct ImpostorOut {
    @builtin(position) position: vec4<f32>,
    @location(0) uv: vec2<f32>,
    // The eye's bearing round the model's axis, in views (0 up to their count).
    @location(1) @interpolate(flat) bearing: f32,
    @location(2) @interpolate(flat) keep: vec2<f32>,
    // The model's placement, for its turn.
    @location(3) @interpolate(flat) r0: vec4<f32>,
    @location(4) @interpolate(flat) r1: vec4<f32>,
    @location(5) @interpolate(flat) r2: vec4<f32>,
};

@vertex
fn vs_impostor(v: Vertex, i: Instance) -> ImpostorOut {
    var o: ImpostorOut;
    // The eye in the model's own space: the placement's rows are its turn times its scale.
    let d = frame.eye.xyz - vec3<f32>(i.r0.w, i.r1.w, i.r2.w);
    let back = vec3<f32>(
        i.r0.x * d.x + i.r1.x * d.y + i.r2.x * d.z,
        i.r0.y * d.x + i.r1.y * d.y + i.r2.y * d.z,
        i.r0.z * d.x + i.r1.z * d.y + i.r2.z * d.z,
    );
    let bearing = atan2(back.y, back.x);
    // The quad's first axis goes across the eye's view, its height stays up.
    let across = vec3<f32>(-sin(bearing), cos(bearing), 0.0);
    let local = across * v.position.x + vec3<f32>(0.0, 0.0, v.position.z);
    o.position = frame.clip_from_render * vec4<f32>(place(i, local), 1.0);
    o.uv = v.uv;
    let views = max(material.views.x, 1.0);
    let b = bearing / TAU * views;
    o.bearing = b - floor(b / views) * views;
    o.keep = i.keep.xy;
    o.r0 = i.r0;
    o.r1 = i.r1;
    o.r2 = i.r2;
    return o;
}

// A level of pictures' outputs: a leaf's, and the depth of the surface the picture shows there.
struct ImpostorFrag {
    @location(0) colour: vec4<f32>,
    @location(1) normal: vec4<f32>,
    @builtin(frag_depth) depth: f32,
};

@fragment
fn fs_impostor(in: ImpostorOut) -> ImpostorFrag {
    let fading = dither_at(in.position.xy + vec2<f32>(37.0, 17.0));
    if (fading < in.keep.x || fading >= in.keep.y) {
        discard;
    }
    // The view nearest the eye's bearing; near the middle between two, one or the other by the
    // dither, so frames of it blend them as the eye turns past.
    let views = max(material.views.x, 1.0);
    let columns = max(material.views.y, 1.0);
    let rows = max(material.views.z, 1.0);
    let first = floor(in.bearing);
    let toward = smoothstep(0.35, 0.65, in.bearing - first);
    var k = select(first, first + 1.0, dither_at(in.position.xy + vec2<f32>(11.0, 23.0)) < toward);
    k = k - floor(k / views) * views;
    let tile = vec2<f32>(k - floor(k / columns) * columns, floor(k / columns));
    let grid = vec2<f32>(columns, rows);
    let at = (tile + clamp(in.uv, vec2<f32>(0.0), vec2<f32>(1.0))) / grid;
    // The pictures are read at the level of detail the quad itself calls for, whichever view.
    let gx = dpdx(in.uv) / grid;
    let gy = dpdy(in.uv) / grid;
    let base = textureSampleGrad(base_tex, samp, at, gx, gy) * material.base_color;
    if (dithered_alpha(base.a, in.position.xy) < material.flags.x) {
        discard;
    }
    let nd = textureSampleGrad(normal_tex, samp, at, gx, gy);
    let normal = nd.xyz * 2.0 - vec3<f32>(1.0);
    // Where the surface stands in the model's own space: across the view's picture, up its
    // height, and along the view by its depth.
    let theta = k / views * TAU;
    let across = vec3<f32>(-sin(theta), cos(theta), 0.0);
    let inward = vec3<f32>(-cos(theta), -sin(theta), 0.0);
    let half_width = material.quad.x;
    let local = across * ((in.uv.x * 2.0 - 1.0) * half_width)
        + vec3<f32>(0.0, 0.0, mix(material.quad.z, material.quad.y, in.uv.y))
        + inward * ((nd.w - 0.5) * 2.0 * half_width);
    let n = vec3<f32>(dot(in.r0.xyz, normal), dot(in.r1.xyz, normal), dot(in.r2.xyz, normal));
    let leaf = leaf_out(base.rgb, normalize(n), local, in.position.xy);
    // The depth of the surface itself, not the quad's, so it meets the ground and its shadows
    // where the model would.
    let q = vec4<f32>(local, 1.0);
    let world = vec3<f32>(dot(in.r0, q), dot(in.r1, q), dot(in.r2, q));
    let clip = frame.clip_from_render * vec4<f32>(world, 1.0);
    var o: ImpostorFrag;
    o.colour = leaf.colour;
    o.normal = leaf.normal;
    o.depth = clamp(clip.z / clip.w, 0.0, 1.0);
    return o;
}

// The sun's shadow cascades.

struct CasterOut {
    @builtin(position) position: vec4<f32>,
    @location(0) uv: vec2<f32>,
    @location(1) alpha: f32,
};

@vertex
fn vs_caster(v: Vertex, i: Instance) -> CasterOut {
    var o: CasterOut;
    o.position = caster.light_from_render * vec4<f32>(place(i, v.position), 1.0);
    o.uv = v.uv;
    o.alpha = v.color.a;
    return o;
}

// Cut-out growth: its picture's alpha cuts the shadow too, read at the level of detail the
// cascade's texels call for (a cascade sees a canopy from far off, so the full picture would be
// read a texel in hundreds).
@fragment
fn fs_caster_cut(in: CasterOut) {
    let size = vec2<f32>(textureDimensions(base_tex));
    let step = max(length(dpdx(in.uv * size)), length(dpdy(in.uv * size)));
    let level = max(log2(max(step, 1e-6)), 0.0);
    let a = textureSampleLevel(base_tex, samp, in.uv, level).a * material.base_color.a * in.alpha;
    if (a < material.flags.x) {
        discard;
    }
}
