// The flat view: every surface one color, lit by a fixed light by the way its face turns, with no
// pictures. Each draw is a mesh drawn once for every place it stands: the place and the color come
// with each instance. The far landscape keeps the color the day lit each of its points with, its
// picture's average added to it as the full view adds the picture.

struct Frame {
    view_proj: mat4x4<f32>,
    eye: vec4<f32>,
    // toward the light
    to_light: vec4<f32>,
    // the ambient share, the direct share, where the fade into the backdrop starts and ends
    light: vec4<f32>,
    backdrop: vec4<f32>,
};

@group(0) @binding(0) var<uniform> frame: Frame;

struct Placed {
    @location(0) position: vec3<f32>,
    @location(8) row0: vec4<f32>,
    @location(9) row1: vec4<f32>,
    @location(10) row2: vec4<f32>,
    @location(11) row3: vec4<f32>,
    @location(12) color: vec4<f32>,
};

struct PlacedLand {
    @location(0) position: vec3<f32>,
    @location(1) lit: vec4<f32>,
    @location(8) row0: vec4<f32>,
    @location(9) row1: vec4<f32>,
    @location(10) row2: vec4<f32>,
    @location(11) row3: vec4<f32>,
    @location(12) color: vec4<f32>,
};

struct Out {
    @builtin(position) clip: vec4<f32>,
    @location(0) world: vec3<f32>,
    @location(1) color: vec3<f32>,
    // a point's own lit color, and 1 when it is the far landscape's
    @location(2) lit: vec4<f32>,
};

// The instance's place as the world's transforms keep it, its rows uploaded as the columns.
fn placed(p: vec3<f32>, r0: vec4<f32>, r1: vec4<f32>, r2: vec4<f32>, r3: vec4<f32>) -> vec4<f32> {
    return mat4x4<f32>(r0, r1, r2, r3) * vec4<f32>(p, 1.0);
}

@vertex
fn vs_flat(v: Placed) -> Out {
    let world = placed(v.position, v.row0, v.row1, v.row2, v.row3);
    var o: Out;
    o.clip = frame.view_proj * world;
    o.world = world.xyz;
    o.color = v.color.rgb;
    o.lit = vec4<f32>(0.0);
    return o;
}

// The land: a far piece (its instance's color's alpha over 1.5) keeps its points' lit color.
@vertex
fn vs_flat_land(v: PlacedLand) -> Out {
    let world = placed(v.position, v.row0, v.row1, v.row2, v.row3);
    var o: Out;
    o.clip = frame.view_proj * world;
    o.world = world.xyz;
    o.color = v.color.rgb;
    o.lit = vec4<f32>(v.lit.rgb, select(0.0, 1.0, v.color.a > 1.5));
    return o;
}

@fragment
fn fs_flat(i: Out) -> @location(0) vec4<f32> {
    // The face's own normal, turned toward the eye whichever way it was wound.
    var n = normalize(cross(dpdx(i.world), dpdy(i.world)));
    let to_eye = frame.eye.xyz - i.world;
    if (dot(n, to_eye) < 0.0) {
        n = -n;
    }
    let lit = frame.light.x + frame.light.y * max(dot(n, frame.to_light.xyz), 0.0);
    var c = i.color * lit;
    if (i.lit.w > 0.5) {
        c = clamp(2.0 * (i.color + i.lit.rgb - vec3<f32>(0.5)), vec3<f32>(0.0), vec3<f32>(1.0));
    }
    if (frame.light.w > frame.light.z) {
        let d = length(to_eye);
        let t = clamp((d - frame.light.z) / (frame.light.w - frame.light.z), 0.0, 1.0);
        c = mix(c, frame.backdrop.rgb, t);
    }
    return vec4<f32>(c, 1.0);
}
