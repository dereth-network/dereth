@group(0) @binding(5) var scene: acceleration_structure;
@group(0) @binding(6) var atlas: texture_2d_array<f32>;
@group(0) @binding(7) var atlas_sampler: sampler;
@group(0) @binding(8) var<storage, read> cuts: array<vec4<u32>>;
@group(0) @binding(9) var<storage, read> uvs: array<vec2<f32>>;

fn solid(c: RayIntersection) -> bool {
    let k = c.instance_custom_data + c.geometry_index;
    let cut = cuts[k];
    let base = cut.x + c.primitive_index * 3u;
    let b = c.barycentrics;
    let uv = uvs[base] * (1.0 - b.x - b.y) + uvs[base + 1u] * b.x + uvs[base + 2u] * b.y;
    let a = textureSampleLevel(atlas, atlas_sampler, uv, i32(cut.y), 0.0).r;
    return a * 255.0 >= max(f32(cut.z), 64.0);
}

// Whether anything solid lies along the ray before `t_max`.
fn blocked(origin: vec3<f32>, dir: vec3<f32>, t_max: f32) -> bool {
    var rq: ray_query;
    rayQueryInitialize(&rq, scene, RayDesc(4u, 0xFFu, 0.0, t_max, origin, dir));
    while (rayQueryProceed(&rq)) {
        let c = rayQueryGetCandidateIntersection(&rq);
        if (c.kind == RAY_QUERY_INTERSECTION_TRIANGLE && solid(c)) {
            rayQueryConfirmIntersection(&rq);
        }
    }
    let hit = rayQueryGetCommittedIntersection(&rq);
    return hit.kind != RAY_QUERY_INTERSECTION_NONE;
}
