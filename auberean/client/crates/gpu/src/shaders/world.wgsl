// The world's shaders. The vertex programs are assembled from the pieces the client's vertex
// program is linked from, one function per piece, chosen per draw by its switches:
//
//   transform_to_clip         the position through world, view and projection
//   transform_normal          the normal into the world's frame
//   dir_light_diffuse_specular  the sun's diffuse share and its specular highlight
//   apply_ambient             the ambient light added to the lit color
//   modulate_material_diffuse the layer's diffuse color (and the draw's tint)
//   modulate_vertex_diffuse   the vertex's own color
//   linear_fog                how much of the surface shows through the fog, by distance
//   tex_light_direction       the sun's direction in the surface's own frame, for a normal map
//
// The pixel side is either the device's texture stage arithmetic for the first stage with the
// specular color added after it, or a layer's own pixel program (the normal-mapped one); then
// the alpha test and the fog.

struct FrameUniforms {
    view_proj: mat4x4<f32>,
    eye: vec4<f32>,
    // start, end, 1 / (end - start), 1 when fog is on
    fog: vec4<f32>,
    fog_color: vec4<f32>,
    // the direction toward the sun, unit length
    to_sun: vec4<f32>,
    // the sun's diffuse color times its brightness
    sun_color: vec4<f32>,
    // the ambient light: its color times its brightness
    ambient: vec4<f32>,
    // the direction the camera looks
    forward: vec4<f32>,
    // the sun's specular color: its color, not scaled by its brightness
    sun_specular: vec4<f32>,
    // the alpha fog: start, end, 1 / (end - start), unused
    alpha_fog: vec4<f32>,
    // world to the sun's shadow map's picture coordinates
    shadow_uv: mat4x4<f32>,
    // the clouds' shadows: picture repeats per meter, the slide along u and v, 1 when drawn
    cloud: vec4<f32>,
    // 1 when the near terrain's shadows and fog are laid on after its layers, unused x3
    terrain: vec4<f32>,
    // the picture's width and height in pixels, unused x2
    viewport: vec4<f32>,
};

struct DrawUniforms {
    model: mat4x4<f32>,
    // the layer's diffuse color times the tint
    diffuse: vec4<f32>,
    // the sun's diffuse color times the layer's diffuse color, each at most 1 (alpha: the
    // layer's, at most 1): the pixel programs' lit color
    lit_diffuse: vec4<f32>,
    // the layer's specular color times the tint
    specular: vec4<f32>,
    // the layer's ambient color
    layer_ambient: vec4<f32>,
    // specular power, alpha reference, the ripples' push (the water's reflection), unused
    params: vec4<f32>,
    // switches, unused
    stage: vec4<u32>,
    // the texture factor: the layer's diffuse color times the tint, as a stored color
    tfactor: vec4<f32>,
    // the first three stages' arithmetic: color op, color args (arg1 | arg2 << 8), alpha op,
    // alpha args; a stage past the layer's last is disabled (op 1)
    ops: array<vec4<u32>, 3>,
    // the pixel program (0 the stage arithmetic, 1 the normal-mapped program), its options, unused
    program: vec4<u32>,
    // the coordinate set each of the first three stages reads (2: the anisotropic coordinates;
    // 3: the eye's reflection about the surface)
    uv_sets: vec4<u32>,
    // each of the first three stages' coordinate transform, two rows each:
    // u' = (a, b, c) . (u, v, 1), v' = (d, e, f) . (u, v, 1)
    uv_rows: array<vec4<f32>, 6>,
    // the draw's point lights (how many: stage.y), inside in place of the sun (inside: stage.z),
    // outdoors beside it: each its place (w: 10 / range squared), its diffuse color and its
    // specular color
    pt_origin: array<vec4<f32>, 4>,
    pt_diffuse: array<vec4<f32>, 4>,
    pt_specular: array<vec4<f32>, 4>,
    // a frill's spot on the ground (w: 1), whose shadow its vertices take for their color
    shade: vec4<f32>,
};

@group(0) @binding(0) var<uniform> frame: FrameUniforms;
@group(1) @binding(0) var<uniform> draw: DrawUniforms;
@group(2) @binding(0) var tex0: texture_2d<f32>;
@group(2) @binding(1) var samp0: sampler;
@group(2) @binding(2) var tex1: texture_2d<f32>;
@group(2) @binding(3) var samp1: sampler;
@group(2) @binding(4) var tex2: texture_2d<f32>;
@group(2) @binding(5) var samp2: sampler;
// The sun's shadow map (white with none): what a body standing on the land takes about its spot.
// The sun's shadow map while the frame's spots are worked out, then the picture of those spots.
@group(3) @binding(0) var shadow_map: texture_2d<f32>;

// What the draw's light and specular are multiplied by for the shadow about its spot.
var<private> ground_tint: vec3<f32> = vec3<f32>(1.0);

const DYNAMIC_LIGHTING: u32 = 0x1u;
const SPECULAR: u32 = 0x4u;
const MATERIAL_DIFFUSE: u32 = 0x10u;
const VERTEX_DIFFUSE: u32 = 0x20u;
const DEPTH_FOG: u32 = 0x100u;
const DEPTH_ALPHA_FOG: u32 = 0x1000u;
// Set on an interior cell's draws: the light is in the vertices' colors (the interior's lights,
// baked and chosen per cell), no sun.
const LIT_IN_VERTICES: u32 = 0x40000000u;

// the normal-mapped pixel program's options
const DETAIL_ON_DIFFUSE: u32 = 0x1u;
const DETAIL_ON_NORMAL: u32 = 0x2u;
const ALPHA_FROM_DIFFUSE: u32 = 0x4u;
const GLOSS: u32 = 0x8u;
const HALF_VECTOR_SPECULAR: u32 = 0x10u;
const ENVIRONMENT_ADD: u32 = 0x20u;
const ENVIRONMENT_GLOSS: u32 = 0x40u;

fn transform_to_clip(p: vec3<f32>, model: mat4x4<f32>) -> vec4<f32> {
    return frame.view_proj * (model * vec4<f32>(p, 1.0));
}

fn transform_normal(n: vec3<f32>, model: mat4x4<f32>) -> vec3<f32> {
    let m = mat3x3<f32>(model[0].xyz, model[1].xyz, model[2].xyz);
    let w = m * n;
    let l = length(w);
    if (l > 0.0) {
        return w / l;
    }
    return vec3<f32>(0.0, 0.0, 1.0);
}

// lit(N·L, N·H, power): (1, max(N·L, 0), N·L > 0 and N·H > 0 ? N·H^power : 0, 1), the power
// held within the vertex program's limit of 128.
fn dir_light_diffuse_specular(n: vec3<f32>, power: f32) -> vec2<f32> {
    let nl = dot(n, frame.to_sun.xyz);
    let h = normalize(frame.to_sun.xyz - frame.forward.xyz);
    let nh = dot(n, h);
    var spec = 0.0;
    if (nl > 0.0 && nh > 0.0) {
        spec = pow(nh, clamp(power, -127.9961, 127.9961));
    }
    return vec2<f32>(max(nl, 0.0), spec);
}

// The sun's direction in the frame of the surface's normal map: along its u and v directions and
// its normal, unit length.
fn tex_light_direction(n: vec3<f32>, tu: vec3<f32>, tv: vec3<f32>) -> vec3<f32> {
    let l = frame.to_sun.xyz;
    let t = vec3<f32>(dot(tu, l), dot(tv, l), dot(n, l));
    let len = length(t);
    if (len > 0.0) {
        return t / len;
    }
    return vec3<f32>(0.0, 0.0, 1.0);
}

// Whether every part of `v` is a number: its exponent's bits are not all set (neither infinite
// nor NaN). Read from the bits, so no compiler's float rules can fold it away.
fn finite3(v: vec3<f32>) -> bool {
    let e = bitcast<vec3<u32>>(v) & vec3<u32>(0x7f800000u);
    return all(e != vec3<u32>(0x7f800000u));
}

// A direction toward the light in the surface's own frame (unit length), packed into 0..1 as
// color 0 carries it. Some meshes store a surface frame that is not a number where their
// pictures' coordinates run flat (the foot of a crystal, for one): the direction made from it is
// not a number either, and the device's colors read each such part as 1, so the packed direction
// is white and the normal map there is lit as if it faced the light. Decided here from the stored
// frame, so every device draws it the same.
fn packed_light(l: vec3<f32>, v: MeshIn) -> vec3<f32> {
    if (!(finite3(v.normal) && finite3(v.tangent_u) && finite3(v.tangent_v))) {
        return vec3<f32>(1.0);
    }
    return (l + vec3<f32>(1.0)) * 0.5;
}

// Point light k's share at `world` on a surface facing `n`: how squarely its light meets the
// surface times its attenuation 1 / (1 + 10 d² / R²); the specular intensity with the half vector
// toward the viewer, times the attenuation; and the attenuation.
fn pt_light(k: u32, world: vec3<f32>, n: vec3<f32>, power: f32) -> vec3<f32> {
    let o = draw.pt_origin[k];
    let to = o.xyz - world;
    let d2 = dot(to, to);
    let att = 1.0 / (1.0 + o.w * d2);
    let l = to * inverseSqrt(max(d2, 1e-12));
    let nl = dot(n, l);
    let nh = dot(n, -frame.forward.xyz);
    var spec = 0.0;
    if (nl > 0.0 && nh > 0.0) {
        spec = pow(nh, clamp(power, -127.9961, 127.9961)) * att;
    }
    return vec3<f32>(max(nl, 0.0) * att, spec, att);
}

// The kept point lights of a surface lit in its vertices, as they shine at the moment, at `world`
// on a surface facing `n` (along `tu` and `tv`): each light's diffuse color times how squarely it
// meets the surface over 1 + 10 d² / R²; and (w-less) each light's direction in the surface's own
// frame, a unit, times max(1 - d² / R², 0), to add to the vertex's own lean.
struct KeptLight {
    color: vec3<f32>,
    lean: vec3<f32>,
};

fn kept_lights(world: vec3<f32>, n: vec3<f32>, tu: vec3<f32>, tv: vec3<f32>) -> KeptLight {
    var out: KeptLight;
    out.color = vec3<f32>(0.0);
    out.lean = vec3<f32>(0.0);
    let count = min(draw.stage.y, 4u);
    for (var k = 0u; k < count; k = k + 1u) {
        let o = draw.pt_origin[k];
        let to = o.xyz - world;
        let d2 = dot(to, to);
        let att = 1.0 / (1.0 + o.w * d2);
        let l = to * inverseSqrt(max(d2, 1e-12));
        out.color = out.color + draw.pt_diffuse[k].rgb * max(dot(n, l), 0.0) * att;
        let seen = vec3<f32>(dot(tu, to), dot(tv, to), dot(n, to));
        let range2 = 10.0 / max(o.w, 1e-12);
        let f = max(1.0 - dot(seen, seen) / range2, 0.0);
        let sl = length(seen);
        if (sl > 0.0) {
            out.lean = out.lean + seen / sl * f;
        }
    }
    return out;
}

fn apply_ambient(c: vec4<f32>) -> vec4<f32> {
    let a = min(frame.ambient.rgb * draw.layer_ambient.rgb, vec3<f32>(1.0));
    return vec4<f32>(c.rgb + a, c.a);
}

fn modulate_material_diffuse(c: vec4<f32>) -> vec4<f32> {
    return c * vec4<f32>(ground_tint, 1.0) * draw.diffuse;
}

fn modulate_vertex_diffuse(c: vec4<f32>, v: vec4<f32>) -> vec4<f32> {
    return c * v;
}

fn linear_fog(world: vec3<f32>) -> f32 {
    if (frame.fog.w < 0.5) {
        return 1.0;
    }
    let d = distance(world, frame.eye.xyz);
    return clamp((frame.fog.y - d) * frame.fog.z, 0.0, 1.0);
}

// ---- terrain ----

struct TerrainIn {
    @location(0) position: vec3<f32>,
    @location(1) color: vec4<f32>,
    @location(2) uv: vec2<f32>,
    @location(3) mask_uv: vec2<f32>,
};

struct TerrainOut {
    // the same position in every pass over the near terrain, so the passes laid over its layers
    // meet its depth exactly
    @builtin(position) @invariant clip: vec4<f32>,
    @location(0) color: vec4<f32>,
    @location(1) uv: vec2<f32>,
    @location(2) mask_uv: vec2<f32>,
    @location(3) fog: f32,
    // the sun's shadow map's coordinates, and the clouds'
    @location(4) shadow_uv: vec2<f32>,
    @location(5) cloud_uv: vec2<f32>,
};

@vertex
fn vs_terrain(v: TerrainIn) -> TerrainOut {
    var o: TerrainOut;
    o.clip = frame.view_proj * vec4<f32>(v.position, 1.0);
    // white, times the vertex's lit color
    o.color = modulate_vertex_diffuse(vec4<f32>(1.0), v.color);
    o.uv = v.uv;
    o.mask_uv = v.mask_uv;
    o.fog = linear_fog(v.position);
    o.shadow_uv = (frame.shadow_uv * vec4<f32>(v.position, 1.0)).xy;
    // the ground's place times the clouds' scale, slid by the running time
    o.cloud_uv = v.position.xy * frame.cloud.x + frame.cloud.yz;
    return o;
}

fn fogged(c: vec3<f32>, f: f32) -> vec3<f32> {
    return mix(frame.fog_color.rgb, c, f);
}

// The near terrain's own fog, unless it is laid on after the shadows.
fn terrain_fogged(c: vec3<f32>, f: f32) -> vec3<f32> {
    if (frame.terrain.x > 0.5) {
        return c;
    }
    return fogged(c, f);
}

// The bottom layer: the ground modulated twice by the lit color.
@fragment
fn fs_terrain_base(v: TerrainOut) -> @location(0) vec4<f32> {
    let t = textureSample(tex0, samp0, v.uv);
    let c = clamp(2.0 * t.rgb * v.color.rgb, vec3<f32>(0.0), vec3<f32>(1.0));
    return vec4<f32>(terrain_fogged(c, v.fog), 1.0);
}

// A blended layer: the same, with the mask's alpha.
@fragment
fn fs_terrain_mask(v: TerrainOut) -> @location(0) vec4<f32> {
    let t = textureSample(tex0, samp0, v.uv);
    let m = textureSample(tex1, samp1, v.mask_uv);
    let c = clamp(2.0 * t.rgb * v.color.rgb, vec3<f32>(0.0), vec3<f32>(1.0));
    return vec4<f32>(terrain_fogged(c, v.fog), m.a);
}

// The clouds' shadow at a point: the cloud picture where it lies, or white with no clouds.
fn cloud_shade(uv: vec2<f32>) -> vec3<f32> {
    let c = textureSample(tex1, samp1, uv).rgb;
    if (frame.cloud.w < 0.5) {
        return vec3<f32>(1.0);
    }
    return c;
}

// Over the near terrain's layers: the sun's shadow map (stage 0) times the clouds (stage 1),
// multiplying what the layers drew.
@fragment
fn fs_terrain_shade(v: TerrainOut) -> @location(0) vec4<f32> {
    let s = textureSample(tex0, samp0, v.shadow_uv).rgb;
    return vec4<f32>(s * cloud_shade(v.cloud_uv), 1.0);
}

// Then the fog over it: the fog color, as opaque as the point is fogged.
@fragment
fn fs_terrain_fog(v: TerrainOut) -> @location(0) vec4<f32> {
    return vec4<f32>(frame.fog_color.rgb, 1.0 - v.fog);
}

// The far landscape: the picture added to the lit color, signed and doubled; with the clouds'
// shadows, added signed, then doubled against the cloud picture.
@fragment
fn fs_terrain_far(v: TerrainOut) -> @location(0) vec4<f32> {
    let t = textureSample(tex0, samp0, v.uv);
    let cloud = textureSample(tex1, samp1, v.cloud_uv).rgb;
    var c = clamp(2.0 * (t.rgb + v.color.rgb - vec3<f32>(0.5)), vec3<f32>(0.0), vec3<f32>(1.0));
    if (frame.cloud.w > 0.5) {
        let signed = clamp(t.rgb + v.color.rgb - vec3<f32>(0.5), vec3<f32>(0.0), vec3<f32>(1.0));
        c = clamp(2.0 * signed * cloud, vec3<f32>(0.0), vec3<f32>(1.0));
    }
    return vec4<f32>(fogged(c, v.fog), 1.0);
}

// ---- placed objects and the sky ----

struct MeshIn {
    @location(0) position: vec3<f32>,
    @location(1) normal: vec3<f32>,
    @location(2) diffuse: vec4<f32>,
    @location(3) uv0: vec2<f32>,
    @location(4) uv1: vec2<f32>,
    @location(5) tangent_u: vec3<f32>,
    @location(6) tangent_v: vec3<f32>,
    @location(7) to_light: vec3<f32>,
};

struct MeshOut {
    @builtin(position) clip: vec4<f32>,
    @location(0) color0: vec4<f32>,
    @location(1) color1: vec4<f32>,
    @location(2) uv0: vec2<f32>,
    @location(3) uv1: vec2<f32>,
    @location(4) uv2: vec2<f32>,
    @location(5) fog: f32,
    // for a program lit per pixel against the half vector: the half vector between the light and
    // the eye in the surface's own frame, times the square root of the light's attenuation; and
    // 0.75 times the attenuation
    @location(7) half_vector: vec4<f32>,
    @location(8) ground_tint: vec3<f32>,
};

// The half vector between the eye and the light `to_light` (a unit for the sun, the way to a point
// light for one) at `world`, in the frame of the surface's normal map (along its u and v directions
// and its normal), a unit times the square root of the light's attenuation `att`; with 0.75 times
// the attenuation.
fn tex_half_vector(world: vec3<f32>, to_light: vec3<f32>, att: f32, n: vec3<f32>, tu: vec3<f32>, tv: vec3<f32>) -> vec4<f32> {
    var e = frame.eye.xyz - world;
    let el = length(e);
    if (el > 0.0) {
        e = e / el;
    }
    var h = e + to_light;
    let hl = length(h);
    if (hl > 0.0) {
        h = h / hl;
    }
    var t = vec3<f32>(dot(tu, h), dot(tv, h), dot(n, h));
    let tl = length(t);
    if (tl > 0.0) {
        t = t / tl;
    }
    return vec4<f32>(t * sqrt(att), 0.75 * att);
}

// The anisotropic coordinates at `world` on a surface facing `n`, the light `to_light` away (a
// unit for the sun, the way to a point light for one): n·l, and n·h for the half vector between the
// light and the eye.
fn anisotropic_uv(world: vec3<f32>, to_light: vec3<f32>, n: vec3<f32>) -> vec2<f32> {
    var e = frame.eye.xyz - world;
    let el = length(e);
    if (el > 0.0) {
        e = e / el;
    }
    var h = e + to_light;
    let hl = length(h);
    if (hl > 0.0) {
        h = h / hl;
    }
    return vec2<f32>(dot(n, to_light), dot(n, h));
}

// The coordinates an environment map is looked up by at `world` on a surface facing `n`: the
// direction toward the eye reflected about the normal, taken along the camera's right and up and
// packed into 0..1. No coordinate transform applies.
fn reflection_uv(world: vec3<f32>, n: vec3<f32>) -> vec2<f32> {
    var e = frame.eye.xyz - world;
    let el = length(e);
    if (el > 0.0) {
        e = e / el;
    }
    var r = 2.0 * dot(n, e) * n - e;
    let rl = length(r);
    if (rl > 0.0) {
        r = r / rl;
    }
    let f = frame.forward.xyz;
    var right = cross(f, vec3<f32>(0.0, 0.0, 1.0));
    let l = length(right);
    if (l > 0.0) {
        right = right / l;
    }
    let up = cross(right, f);
    return vec2<f32>(dot(r, right), dot(r, up)) * 0.5 + vec2<f32>(0.5);
}

// Stage k's coordinates: its coordinate set through its transform.
fn stage_uv(k: u32, v: MeshIn) -> vec2<f32> {
    var which = draw.uv_sets.x;
    if (k == 1u) {
        which = draw.uv_sets.y;
    } else if (k == 2u) {
        which = draw.uv_sets.z;
    }
    var t = v.uv0;
    if (which != 0u) {
        t = v.uv1;
    }
    let a = draw.uv_rows[2u * k];
    let b = draw.uv_rows[2u * k + 1u];
    return vec2<f32>(dot(a.xyz, vec3<f32>(t, 1.0)), dot(b.xyz, vec3<f32>(t, 1.0)));
}

// A frill's color: the sun's shadow map (in the third texture slot) about the frill's spot on the
// ground, read at the texel there and the texels two and four away each way, a five by five grid,
// averaged over those within the map (white when none is), opaque.
fn frill_shade(at: vec3<f32>) -> vec4<f32> {
    let uv = (frame.shadow_uv * vec4<f32>(at, 1.0)).xy;
    let size = vec2<i32>(textureDimensions(tex2));
    let p = vec2<i32>(uv * vec2<f32>(size));
    var sum = vec3<f32>(0.0);
    var n = 0.0;
    for (var i = -4; i <= 4; i = i + 2) {
        for (var j = -4; j <= 4; j = j + 2) {
            let q = p + vec2<i32>(i, j);
            if (q.x >= 0 && q.y >= 0 && q.x < size.x && q.y < size.y) {
                sum = sum + textureLoad(tex2, q, 0).rgb;
                n = n + 1.0;
            }
        }
    }
    if (n == 0.0) {
        return vec4<f32>(1.0);
    }
    return vec4<f32>(clamp(sum / n, vec3<f32>(0.0), vec3<f32>(1.0)), 1.0);
}

// A spot's share of the sun: the shadow map about it on the ground, as a frill reads it.
fn spot_shade(at: vec3<f32>) -> vec3<f32> {
    let uv = (frame.shadow_uv * vec4<f32>(at, 1.0)).xy;
    let size = vec2<i32>(textureDimensions(shadow_map));
    let p = vec2<i32>(uv * vec2<f32>(size));
    var sum = vec3<f32>(0.0);
    var n = 0.0;
    for (var i = -4; i <= 4; i = i + 2) {
        for (var j = -4; j <= 4; j = j + 2) {
            let q = p + vec2<i32>(i, j);
            if (q.x >= 0 && q.y >= 0 && q.x < size.x && q.y < size.y) {
                sum = sum + textureLoad(shadow_map, q, 0).rgb;
                n = n + 1.0;
            }
        }
    }
    if (n == 0.0) {
        return vec3<f32>(1.0);
    }
    return clamp(sum / n, vec3<f32>(0.0), vec3<f32>(1.0));
}

// Each spot of the frame's bodies, worked out once a frame into one texel of a picture of its own.
struct SpotOut {
    @builtin(position) clip: vec4<f32>,
    @location(0) @interpolate(flat) shade: vec3<f32>,
};

@vertex
fn vs_spot(@builtin(vertex_index) k: u32, @location(0) at: vec3<f32>) -> SpotOut {
    var o: SpotOut;
    let col = f32(k % 256u);
    let row = f32(k / 256u);
    o.clip = vec4<f32>((col + 0.5) / 128.0 - 1.0, 1.0 - (row + 0.5) / 8.0, 0.0, 1.0);
    o.shade = spot_shade(at);
    return o;
}

@fragment
fn fs_spot(v: SpotOut) -> @location(0) vec4<f32> {
    return vec4<f32>(v.shade, 1.0);
}

// A body's share of the sun: its spot's texel of the frame's spots.
fn body_shade(k: i32) -> vec3<f32> {
    return textureLoad(shadow_map, vec2<i32>(k % 256, k / 256), 0).rgb;
}

@vertex
fn vs_mesh(v: MeshIn) -> MeshOut {
    var o: MeshOut;
    let sw = draw.stage.x;
    // A frill's vertices take the shadow's color about its spot in place of their own; a body's
    // light takes it.
    var vd = v.diffuse;
    if (draw.shade.w > 1.5) {
        ground_tint = body_shade(i32(draw.shade.x));
    } else if (draw.shade.w > 0.5) {
        vd = frill_shade(draw.shade.xyz);
    }
    o.ground_tint = ground_tint;
    o.clip = transform_to_clip(v.position, draw.model);
    let world = (draw.model * vec4<f32>(v.position, 1.0)).xyz;
    var c0 = vec4<f32>(1.0);
    var c1 = vec4<f32>(0.0);
    if ((sw & LIT_IN_VERTICES) != 0u && draw.program.x == 1u) {
        // Lit in the vertices, normal-mapped: color 0 the vertex's direction toward the light in
        // the surface's own frame (its own lean plus the kept lights'), a unit, handed on as it
        // is: not packed into 0..1 (the pixel side reads it held to 0..1, then unpacks it); color
        // 1 the vertex's light plus the kept lights' plus the ambient.
        let n = transform_normal(v.normal, draw.model);
        let kept = kept_lights(
            world,
            n,
            transform_normal(v.tangent_u, draw.model),
            transform_normal(v.tangent_v, draw.model),
        );
        var lean = v.to_light + kept.lean;
        let ll = length(lean);
        if (ll > 0.0) {
            lean = lean / ll;
        } else {
            lean = vec3<f32>(0.0, 0.0, 1.0);
        }
        c0 = vec4<f32>(lean, 1.0);
        c1 = apply_ambient(vec4<f32>(vd.rgb + kept.color, 1.0));
    } else if ((sw & LIT_IN_VERTICES) != 0u && (sw & DYNAMIC_LIGHTING) != 0u) {
        // Lit in the vertices: the vertex's light plus the kept lights' plus the ambient.
        let n = transform_normal(v.normal, draw.model);
        let kept = kept_lights(world, n, n, n);
        c0 = apply_ambient(vec4<f32>(vd.rgb + kept.color, 1.0));
    } else if (draw.program.x == 1u && (draw.stage.z == 1u || draw.stage.y > 0u)) {
        // Normal-mapped with a point light, or inside with none: the first point light lights the
        // normal map (color 0: its direction in the surface's own frame, packed, alpha its
        // attenuation; with none, no direction at all). Each other's diffuse share goes to color
        // 1 in the color of the light before it, for the program's diffuse colors are counted only
        // where they are used and the first light uses none, and the sun's outside in its own;
        // times the layer's diffuse, plus the ambient; every light's specular intensity times its
        // specular red to color 1's alpha.
        let n = transform_normal(v.normal, draw.model);
        let tu = transform_normal(v.tangent_u, draw.model);
        let tv = transform_normal(v.tangent_v, draw.model);
        let count = min(draw.stage.y, 4u);
        var other = vec3<f32>(0.0);
        var spec = 0.0;
        c0 = vec4<f32>(0.5, 0.5, 0.5, 1.0);
        // with no point light, the half vector is the eye's own, as with the directional light's
        // constants cleared
        o.half_vector = tex_half_vector(world, vec3<f32>(0.0), 1.0, n, tu, tv);
        for (var k = 0u; k < count; k = k + 1u) {
            let r = pt_light(k, world, n, draw.params.x);
            spec = spec + r.y * r.z * draw.pt_specular[k].r;
            if (k == 0u) {
                let to = draw.pt_origin[0].xyz - world;
                var t = vec3<f32>(dot(tu, to), dot(tv, to), dot(n, to));
                let len = length(t);
                if (len > 0.0) {
                    t = t / len;
                }
                c0 = vec4<f32>(packed_light(t, v), r.z);
                o.half_vector = tex_half_vector(world, to, r.z, n, tu, tv);
            } else {
                other = other + r.x * draw.pt_diffuse[k - 1u].rgb;
            }
        }
        if (draw.stage.z == 0u) {
            let f = dir_light_diffuse_specular(n, draw.params.x);
            other = other + f.x * frame.sun_color.rgb;
            spec = spec + f.y * frame.sun_specular.r;
        }
        var c11 = vec3<f32>(1.0);
        if ((sw & MATERIAL_DIFFUSE) != 0u) {
            c11 = (draw.diffuse.rgb * ground_tint);
        }
        c1 = apply_ambient(vec4<f32>(other * c11, spec));
    } else if (draw.stage.z == 1u && (sw & DYNAMIC_LIGHTING) != 0u) {
        // Inside, lit: black plus each point light's share, times the layer's diffuse, plus the
        // ambient; the specular from each light, times the layer's specular color.
        let n = transform_normal(v.normal, draw.model);
        let count = min(draw.stage.y, 4u);
        c0 = vec4<f32>(0.0, 0.0, 0.0, 1.0);
        for (var k = 0u; k < count; k = k + 1u) {
            let r = pt_light(k, world, n, draw.params.x);
            c0 = vec4<f32>(c0.rgb + r.x * draw.pt_diffuse[k].rgb, c0.a);
            if ((sw & SPECULAR) != 0u) {
                c1 = vec4<f32>(c1.rgb + r.y * r.z * draw.pt_specular[k].rgb, 0.0);
            }
        }
        c1 = vec4<f32>(c1.rgb * (draw.specular.rgb * ground_tint), 0.0);
        if ((sw & MATERIAL_DIFFUSE) != 0u) {
            c0 = modulate_material_diffuse(c0);
        }
        c0 = apply_ambient(c0);
        if ((sw & VERTEX_DIFFUSE) != 0u) {
            c0 = modulate_vertex_diffuse(c0, vd);
        }
    } else if (draw.program.x == 1u) {
        // Normal-mapped by the sun alone: color 0 carries its direction in the surface's own
        // frame, packed into 0..1 (alpha: the light's attenuation, 1 for the sun); color 1 the
        // ambient, its alpha the sun's specular intensity times its specular red.
        let n = transform_normal(v.normal, draw.model);
        let tu = transform_normal(v.tangent_u, draw.model);
        let tv = transform_normal(v.tangent_v, draw.model);
        let l = tex_light_direction(n, tu, tv);
        c0 = vec4<f32>(packed_light(l, v), 1.0);
        o.half_vector = tex_half_vector(world, frame.to_sun.xyz, 1.0, n, tu, tv);
        let f = dir_light_diffuse_specular(n, draw.params.x);
        c1 = apply_ambient(vec4<f32>(vec3<f32>(0.0), f.y * frame.sun_specular.r));
    } else if ((sw & DYNAMIC_LIGHTING) != 0u) {
        // Lit: black, plus the sun's share and each point light's beside it (the lights objects
        // carry), times the layer's diffuse, plus the ambient; the specular from the sun and each
        // of them, times the layer's specular color.
        c0 = vec4<f32>(0.0, 0.0, 0.0, 1.0);
        let n = transform_normal(v.normal, draw.model);
        let f = dir_light_diffuse_specular(n, draw.params.x);
        c0 = vec4<f32>(c0.rgb + f.x * frame.sun_color.rgb, c0.a);
        var spec = f.y * frame.sun_specular.rgb;
        let count = min(draw.stage.y, 4u);
        for (var k = 0u; k < count; k = k + 1u) {
            let r = pt_light(k, world, n, draw.params.x);
            c0 = vec4<f32>(c0.rgb + r.x * draw.pt_diffuse[k].rgb, c0.a);
            spec = spec + r.y * r.z * draw.pt_specular[k].rgb;
        }
        if ((sw & SPECULAR) != 0u) {
            c1 = vec4<f32>(spec * (draw.specular.rgb * ground_tint), 0.0);
        }
        if ((sw & MATERIAL_DIFFUSE) != 0u) {
            c0 = modulate_material_diffuse(c0);
        }
        c0 = apply_ambient(c0);
        if ((sw & VERTEX_DIFFUSE) != 0u) {
            c0 = modulate_vertex_diffuse(c0, vd);
        }
    } else {
        // Unlit: white, times the layer's diffuse, times the vertex's color.
        if ((sw & MATERIAL_DIFFUSE) != 0u) {
            c0 = modulate_material_diffuse(c0);
        }
        if ((sw & VERTEX_DIFFUSE) != 0u) {
            c0 = modulate_vertex_diffuse(c0, vd);
        }
    }
    // The alpha fog: the color's alpha fades out between the alpha fog's distances.
    if ((sw & DEPTH_ALPHA_FOG) != 0u && draw.program.x == 0u) {
        let d = distance(world, frame.eye.xyz);
        c0.a = c0.a * clamp((frame.alpha_fog.y - d) * frame.alpha_fog.z, 0.0, 1.0);
    }
    // The device clamps both colors to [0, 1] before the texture stages, so a doubled modulate
    // brightens at most twofold; a direction toward the light is held to 0..1 where each pixel
    // reads it, not at the vertices.
    if ((sw & LIT_IN_VERTICES) != 0u && draw.program.x == 1u) {
        o.color0 = c0;
    } else {
        o.color0 = clamp(c0, vec4<f32>(0.0), vec4<f32>(1.0));
    }
    o.color1 = clamp(c1, vec4<f32>(0.0), vec4<f32>(1.0));
    o.uv0 = stage_uv(0u, v);
    o.uv1 = stage_uv(1u, v);
    o.uv2 = stage_uv(2u, v);
    // A stage reading the anisotropic coordinates: how squarely the light meets the surface (the
    // way to a point light, not made a unit), and how squarely the half vector between the light
    // and the eye does; the sun outside, the first point light inside.
    if (draw.uv_sets.x == 2u || draw.uv_sets.y == 2u || draw.uv_sets.z == 2u) {
        let n = transform_normal(v.normal, draw.model);
        var to = frame.to_sun.xyz;
        if (draw.stage.z == 1u) {
            to = vec3<f32>(0.0);
            if (draw.stage.y > 0u) {
                to = draw.pt_origin[0].xyz - world;
            }
        }
        let a = anisotropic_uv(world, to, n);
        if (draw.uv_sets.x == 2u) {
            o.uv0 = a;
        }
        if (draw.uv_sets.y == 2u) {
            o.uv1 = a;
        }
        if (draw.uv_sets.z == 2u) {
            o.uv2 = a;
        }
    }
    // A stage reading the eye's reflection: an environment map.
    if (draw.uv_sets.x == 3u || draw.uv_sets.y == 3u || draw.uv_sets.z == 3u) {
        let r = reflection_uv(world, transform_normal(v.normal, draw.model));
        if (draw.uv_sets.x == 3u) {
            o.uv0 = r;
        }
        if (draw.uv_sets.y == 3u) {
            o.uv1 = r;
        }
        if (draw.uv_sets.z == 3u) {
            o.uv2 = r;
        }
    }
    if ((sw & DEPTH_FOG) != 0u) {
        o.fog = linear_fog(world);
    } else {
        o.fog = 1.0;
    }
    return o;
}

// A stage argument: the vertex's diffuse (0) or specular (4) color, the previous stage's result
// (1), the stage's texture (2), the texture factor (3); 0x10 takes its complement, 0x20 copies
// its alpha into every channel.
fn stage_arg(a: u32, t: vec4<f32>, cur: vec4<f32>, d: vec4<f32>, s: vec4<f32>) -> vec4<f32> {
    var x = cur;
    switch (a & 0xFu) {
        case 0u: { x = d; }
        case 2u: { x = t; }
        case 3u: { x = vec4<f32>(draw.tfactor.rgb * ground_tint, draw.tfactor.a); }
        case 4u: { x = s; }
        default: { x = cur; }
    }
    if ((a & 0x20u) != 0u) {
        x = vec4<f32>(x.a);
    }
    if ((a & 0x10u) != 0u) {
        x = vec4<f32>(1.0) - x;
    }
    return x;
}

// A stage operation on its two arguments; the blending ones also read the diffuse, texture,
// factor or current alpha.
fn stage_op(op: u32, a1: vec4<f32>, a2: vec4<f32>, t: vec4<f32>, cur: vec4<f32>, d: vec4<f32>) -> vec4<f32> {
    switch op {
        case 2u: { return a1; }
        case 3u: { return a2; }
        case 4u: { return a1 * a2; }
        case 5u: { return 2.0 * a1 * a2; }
        case 6u: { return 4.0 * a1 * a2; }
        case 7u: { return a1 + a2; }
        case 8u: { return a1 + a2 - vec4<f32>(0.5); }
        case 9u: { return 2.0 * (a1 + a2 - vec4<f32>(0.5)); }
        case 10u: { return a1 - a2; }
        case 11u: { return a1 + a2 - a1 * a2; }
        case 12u: { return mix(a2, a1, d.a); }
        case 13u: { return mix(a2, a1, t.a); }
        case 14u: { return mix(a2, a1, draw.tfactor.a); }
        case 15u: { return a1 + a2 * (1.0 - t.a); }
        case 16u: { return mix(a2, a1, cur.a); }
        case 18u: { return vec4<f32>(a1.rgb + a1.a * a2.rgb, a1.a); }
        case 19u: { return vec4<f32>(a1.rgb * a2.rgb + vec3<f32>(a1.a), a1.a); }
        case 20u: { return vec4<f32>((1.0 - a1.a) * a2.rgb + a1.rgb, a1.a); }
        case 21u: { return vec4<f32>((vec3<f32>(1.0) - a1.rgb) * a2.rgb + vec3<f32>(a1.a), a1.a); }
        case 24u: {
            let x = 4.0 * dot(a1.rgb - vec3<f32>(0.5), a2.rgb - vec3<f32>(0.5));
            return vec4<f32>(x);
        }
        default: { return a1 * a2; }
    }
}

// The device's texture stages, in order: each stage's color and alpha from its own arguments, the
// result held within 0..1 and passed on as the next stage's current color; the first disabled
// stage ends the chain (a disabled first stage leaves the vertex color).
fn texture_stages(v: MeshOut) -> vec4<f32> {
    let tex = array<vec4<f32>, 3>(
        textureSample(tex0, samp0, v.uv0),
        textureSample(tex1, samp1, v.uv1),
        textureSample(tex2, samp2, v.uv2),
    );
    let d = v.color0;
    let s = v.color1;
    var cur = d;
    for (var k = 0u; k < 3u; k = k + 1u) {
        let o = draw.ops[k];
        if (o.x == 1u) {
            break;
        }
        let t = tex[k];
        let c = stage_op(o.x, stage_arg(o.y & 0xFFu, t, cur, d, s), stage_arg((o.y >> 8u) & 0xFFu, t, cur, d, s), t, cur, d);
        var aop = o.z;
        var aargs = o.w;
        if (aop == 1u) {
            // alpha may not be disabled while color runs: it passes the current alpha on
            aop = 2u;
            aargs = 1u;
        }
        let a = stage_op(aop, stage_arg(aargs & 0xFFu, t, cur, d, s), stage_arg((aargs >> 8u) & 0xFFu, t, cur, d, s), t, cur, d);
        cur = clamp(vec4<f32>(c.rgb, a.a), vec4<f32>(0.0), vec4<f32>(1.0));
    }
    return cur;
}

// The normal-mapped pixel program: the normal map (stage 0) lit by the sun's packed direction,
// times the layer's lit diffuse color, plus the ambient; doubled against the diffuse map
// (stage 1); plus the specular intensity times the layer's specular color. A detail map (stage 2)
// is added, signed, to the diffuse map or to the normal map; an environment map there is added to
// the diffuse map, or added times the diffuse map's alpha.
fn bump_program(v: MeshOut) -> vec4<f32> {
    let opts = draw.program.y;
    var nm = textureSample(tex0, samp0, v.uv0);
    var dm = textureSample(tex1, samp1, v.uv1);
    let detail = textureSample(tex2, samp2, v.uv2);
    if ((opts & DETAIL_ON_DIFFUSE) != 0u) {
        dm = vec4<f32>(dm.rgb + detail.rgb - vec3<f32>(0.5), dm.a);
    }
    if ((opts & ENVIRONMENT_ADD) != 0u) {
        dm = vec4<f32>(dm.rgb + detail.rgb, dm.a);
    }
    if ((opts & ENVIRONMENT_GLOSS) != 0u) {
        dm = vec4<f32>(dm.rgb + detail.rgb * dm.a, dm.a);
    }
    if ((opts & DETAIL_ON_NORMAL) != 0u) {
        nm = vec4<f32>(nm.rgb + detail.rgb - vec3<f32>(0.5), nm.a);
    }
    let n = nm.rgb * 2.0 - vec3<f32>(1.0);
    let l = v.color0.rgb * 2.0 - vec3<f32>(1.0);
    let d = clamp(dot(n, l), 0.0, 1.0) * v.color0.a;
    var rgb = (d * (draw.lit_diffuse.rgb * ground_tint) + v.color1.rgb) * 2.0 * dm.rgb;
    var a = draw.diffuse.a;
    if ((opts & ALPHA_FROM_DIFFUSE) != 0u) {
        a = a * dm.a;
    }
    var spec = v.color1.a * (draw.specular.rgb * ground_tint);
    if ((opts & HALF_VECTOR_SPECULAR) != 0u) {
        // The half vector comes as a coordinate set: packed into 0..1, held there, unpacked; its
        // facing on the normal map squared, less 0.75 of the attenuation, four times, held to 0..1.
        let packed = clamp((v.half_vector.xyz + vec3<f32>(1.0)) * 0.5, vec3<f32>(0.0), vec3<f32>(1.0));
        let h = packed * 2.0 - vec3<f32>(1.0);
        let x = clamp(dot(n, h), 0.0, 1.0);
        spec = clamp(4.0 * (x * x - v.half_vector.w), 0.0, 1.0) * (draw.specular.rgb * ground_tint);
    }
    if ((opts & GLOSS) != 0u) {
        spec = spec * dm.a;
    }
    rgb = rgb + spec;
    return vec4<f32>(rgb, a);
}

// The normal-mapped program of a surface lit in its vertices: the diffuse map (with its detail),
// doubled, times the vertex's light and the normal map's facing of the light, plus the diffuse map
// times the ambient. The direction toward the light comes as a unit, each part held to 0..1 and
// then unpacked as a packed one would be: a part below zero reads as fully away (-1), a part
// below one half as turned away, so the normal map is lit from a slant and shows its relief
// more strongly than the true direction would light it.
fn vertex_lit_program(v: MeshOut) -> vec4<f32> {
    let opts = draw.program.y;
    var nm = textureSample(tex0, samp0, v.uv0);
    var dm = textureSample(tex1, samp1, v.uv1);
    let detail = textureSample(tex2, samp2, v.uv2);
    if ((opts & DETAIL_ON_DIFFUSE) != 0u) {
        dm = vec4<f32>(dm.rgb + detail.rgb - vec3<f32>(0.5), dm.a);
    }
    if ((opts & DETAIL_ON_NORMAL) != 0u) {
        nm = vec4<f32>(nm.rgb + detail.rgb - vec3<f32>(0.5), nm.a);
    }
    let n = nm.rgb * 2.0 - vec3<f32>(1.0);
    let l = clamp(v.color0.rgb, vec3<f32>(0.0), vec3<f32>(1.0)) * 2.0 - vec3<f32>(1.0);
    let d = clamp(dot(n, l), 0.0, 1.0);
    let ambient = min(frame.ambient.rgb * draw.layer_ambient.rgb, vec3<f32>(1.0));
    let rgb = 2.0 * dm.rgb * v.color1.rgb * d + dm.rgb * ambient;
    var a = draw.diffuse.a;
    if ((opts & ALPHA_FROM_DIFFUSE) != 0u) {
        a = a * dm.a;
    }
    return vec4<f32>(rgb, a);
}

@fragment
fn fs_mesh(v: MeshOut) -> @location(0) vec4<f32> {
    ground_tint = v.ground_tint;
    if (draw.program.x == 1u && (draw.stage.x & LIT_IN_VERTICES) != 0u) {
        let c = vertex_lit_program(v);
        let a = clamp(c.a, 0.0, 1.0);
        if (a <= draw.params.y) {
            discard;
        }
        let rgb = clamp(c.rgb, vec3<f32>(0.0), vec3<f32>(1.0));
        return vec4<f32>(fogged(rgb, v.fog), a);
    }
    if (draw.program.x == 1u) {
        let c = bump_program(v);
        let a = clamp(c.a, 0.0, 1.0);
        if (a <= draw.params.y) {
            discard;
        }
        let rgb = clamp(c.rgb, vec3<f32>(0.0), vec3<f32>(1.0));
        return vec4<f32>(fogged(rgb, v.fog), a);
    }
    let c = texture_stages(v);
    var rgb = c.rgb + v.color1.rgb;
    let a = c.a;
    if (a <= draw.params.y) {
        discard;
    }
    rgb = clamp(rgb, vec3<f32>(0.0), vec3<f32>(1.0));
    return vec4<f32>(fogged(rgb, v.fog), a);
}

// ---- the water's surface with its reflection ----

// The ripples' signed picture on a water surface, into a picture that holds no value below zero.
@fragment
fn fs_ripple(v: MeshOut) -> @location(0) vec4<f32> {
    return vec4<f32>(textureSample(tex0, samp0, v.uv0).rg, 0.0, 1.0);
}

// The reflection (the first picture, the mirrored scene) where this point lies on the screen,
// turned over top to bottom as the mirrored camera sees it, its coordinates pushed by the ripples'
// picture (the second, where this point lies on the screen) by the draw's push (params.z: 0.04 of
// the picture per unit of offset, 0 with no ripples); as opaque as the water's own layer.
@fragment
fn fs_water(v: MeshOut) -> @location(0) vec4<f32> {
    let screen = v.clip.xy / frame.viewport.xy;
    let ripple = textureSample(tex1, samp1, screen).rg;
    let at = vec2<f32>(screen.x, 1.0 - screen.y) + draw.params.z * ripple;
    let c = textureSample(tex0, samp0, at).rgb;
    return vec4<f32>(c, clamp(draw.diffuse.a, 0.0, 1.0));
}

// ---- the sun's shadow map ----

struct CasterOut {
    @builtin(position) clip: vec4<f32>,
    @location(0) uv0: vec2<f32>,
};

// A caster seen from the sun, its first stage's coordinates.
@vertex
fn vs_caster(v: MeshIn) -> CasterOut {
    var o: CasterOut;
    o.clip = transform_to_clip(v.position, draw.model);
    o.uv0 = stage_uv(0u, v);
    return o;
}

// Black, its alpha the first stage's alpha operation on the texture's alpha and a factor of 1.
@fragment
fn fs_caster(v: CasterOut) -> @location(0) vec4<f32> {
    let t = textureSample(tex0, samp0, v.uv0);
    var op = draw.ops[0].z;
    if (op == 1u) {
        op = 3u;
    }
    let a = stage_op(op, vec4<f32>(t.a), vec4<f32>(1.0), t, vec4<f32>(1.0), vec4<f32>(1.0)).a;
    return vec4<f32>(0.0, 0.0, 0.0, clamp(a, 0.0, 1.0));
}

// The lift over the whole map: one less the shadow strength, added.
@fragment
fn fs_lift() -> @location(0) vec4<f32> {
    return frame.fog_color;
}

// ---- the sky's fog veil: a full-screen quad of the fog color ----

// A quad already on the screen: a lens flare's point, its picture times its color (added onto
// what is drawn); or the marker looking for a flare's object (counted, not seen).
struct ScreenOut {
    @builtin(position) clip: vec4<f32>,
    @location(0) uv: vec2<f32>,
    @location(1) color: vec4<f32>,
};

@vertex
fn vs_screen(v: MeshIn) -> ScreenOut {
    var o: ScreenOut;
    o.clip = vec4<f32>(v.position, 1.0);
    o.uv = v.uv0;
    o.color = v.diffuse;
    return o;
}

@fragment
fn fs_screen(v: ScreenOut) -> @location(0) vec4<f32> {
    return textureSample(tex0, samp0, v.uv) * v.color;
}

@vertex
fn vs_veil(@builtin(vertex_index) i: u32) -> @builtin(position) vec4<f32> {
    let x = f32((i << 1u) & 2u) * 2.0 - 1.0;
    let y = f32(i & 2u) * 2.0 - 1.0;
    return vec4<f32>(x, y, 0.0, 1.0);
}

@fragment
fn fs_veil() -> @location(0) vec4<f32> {
    return vec4<f32>(frame.fog_color.rgb, frame.fog_color.a);
}

// ---- the dynamic objects' volumes of shadow ----

// A volume's face: placed by its draw's frame, nothing written but the stencil's count.
@vertex
fn vs_volume(@location(0) position: vec3<f32>) -> @builtin(position) vec4<f32> {
    return transform_to_clip(position, draw.model);
}

@fragment
fn fs_volume() -> @location(0) vec4<f32> {
    return vec4<f32>(0.0);
}

// The darkening where the count is not zero: the shadow's strength, which the blend takes away.
@fragment
fn fs_darken() -> @location(0) vec4<f32> {
    return vec4<f32>((draw.diffuse.rgb * ground_tint), 1.0);
}
