// One step of the lamp light's edge-aware a-trous filter.

struct Step {
    // x the spacing, pixels.
    step: vec4<i32>,
    // The projection's depth terms: view distance = y / (depth - x).
    proj: vec4<f32>,
};

@group(0) @binding(0) var f_src: texture_2d<f32>;
@group(0) @binding(1) var f_normals: texture_2d<f32>;
@group(0) @binding(2) var f_dst: texture_storage_2d<rgba16float, write>;
@group(0) @binding(3) var<uniform> f_step: Step;
@group(0) @binding(4) var f_depth: texture_depth_2d;

fn view_z(d: f32) -> f32 {
    return f_step.proj.y / max(d - f_step.proj.x, 1e-7);
}

fn kw(i: i32) -> f32 {
    return select(select(0.0625, 0.25, i == 1), 0.375, i == 0);
}

// One step of an edge-aware a-trous filter, stopped at depth and normal edges and kept on
// the pixels the lamps light.
@compute @workgroup_size(8, 8)
fn filter_step(@builtin(global_invocation_id) id: vec3<u32>) {
    let size = vec2<i32>(textureDimensions(f_dst));
    let px = vec2<i32>(id.xy);
    if (px.x >= size.x || px.y >= size.y) {
        return;
    }
    let a0 = textureLoad(f_src, px, 0);
    if (a0.w < 0.0) {
        textureStore(f_dst, px, a0);
        return;
    }
    let n0 = textureLoad(f_normals, px, 0).xyz;
    let d0 = view_z(textureLoad(f_depth, px, 0));
    var sa = vec3<f32>(0.0);
    var wsum = 0.0;
    for (var y: i32 = -2; y <= 2; y = y + 1) {
        for (var x: i32 = -2; x <= 2; x = x + 1) {
            let q = clamp(px + vec2<i32>(x, y) * f_step.step.x, vec2<i32>(0), size - 1);
            let a = textureLoad(f_src, q, 0);
            if (a.w < 0.0) {
                continue;
            }
            let n = textureLoad(f_normals, q, 0).xyz;
            let dq = view_z(textureLoad(f_depth, q, 0));
            let wn = pow(max(dot(n0, n), 0.0), 32.0);
            let wd = exp(-abs(dq - d0) / (d0 * 0.02 + 0.05));
            let w = kw(abs(x)) * kw(abs(y)) * wn * wd;
            sa = sa + a.rgb * w;
            wsum = wsum + w;
        }
    }
    if (wsum > 1e-5) {
        textureStore(f_dst, px, vec4<f32>(sa / wsum, a0.w));
    } else {
        textureStore(f_dst, px, a0);
    }
}
