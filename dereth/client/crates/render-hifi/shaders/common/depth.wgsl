// Positions back from depth, through the inverse of the frame's world to clip transform.

fn reconstruct_position(inverse_view_projection: mat4x4<f32>, uv: vec2<f32>, depth: f32) -> vec3<f32> {
    let ndc = vec2<f32>(uv.x * 2.0 - 1.0, 1.0 - uv.y * 2.0);
    let p = inverse_view_projection * vec4<f32>(ndc, depth, 1.0);
    return p.xyz / p.w;
}

fn is_sky(depth: f32) -> bool {
    return depth >= 1.0;
}
