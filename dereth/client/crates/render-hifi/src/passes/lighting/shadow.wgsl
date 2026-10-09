// Depth from the sun: the retained landscape and static geometry of each block, drawn into one
// cascade layer.

struct Caster {
    // Render space, height up, to the cascade's clip space.
    light_from_render: mat4x4<f32>,
    // The block's south-west corner, render space.
    origin: vec4<f32>,
};
@group(0) @binding(0) var<uniform> caster: Caster;

@vertex
fn vs(@location(0) p: vec3<f32>) -> @builtin(position) vec4<f32> {
    return caster.light_from_render * vec4<f32>(p + caster.origin.xyz, 1.0);
}

// Leaves and other cut-out surfaces: their texture's alpha cuts the shadow too.
@group(1) @binding(0) var leaf: texture_2d<f32>;
@group(1) @binding(1) var leaf_sampler: sampler;

struct CutOut {
    @builtin(position) pos: vec4<f32>,
    @location(0) uv: vec2<f32>,
};

@vertex
fn vs_cut(@location(0) p: vec3<f32>, @location(1) uv: vec2<f32>) -> CutOut {
    var o: CutOut;
    o.pos = caster.light_from_render * vec4<f32>(p + caster.origin.xyz, 1.0);
    o.uv = uv;
    return o;
}

@fragment
fn fs_cut(i: CutOut) {
    if (textureSampleLevel(leaf, leaf_sampler, i.uv, 0.0).a < 0.45) {
        discard;
    }
}
