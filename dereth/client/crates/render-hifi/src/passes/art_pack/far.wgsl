// The far trees' culling: each tree of the set tested against the view, its distance and its
// block, and those kept listed model by model for the draws that follow, each with how much of
// its size it keeps and how much wider it stands.
//
// A tree is kept while it lies within the reach, outside every block whose own objects stand in
// the world (those draw their trees themselves), and in the view. Past the distance every tree is
// drawn to, the trees thin out toward the reach: a tree's own number says how soon it goes, and it
// shrinks away over a short stretch rather than popping. The trees left standing stand wider,
// making up part of the coverage the thinning takes, and a tree too small for the screen to hold
// shrinks away too.

struct Cull {
    // The view's six planes in render space, each the inward normal and its offset.
    planes: array<vec4<f32>, 6>,
    // xyz the eye in render space; w how many trees.
    eye: vec4<f32>,
    // xy where render space's origin lies in the world, meters; z the reach; w where the
    // thinning starts.
    reach: vec4<f32>,
    // x how much of the thinned coverage the trees left make up by standing wider; y how far a
    // tree's fade stretches, a share of the thinning; z pixels a meter at one meter's distance;
    // w the least height on screen a tree is drawn at, pixels.
    look: vec4<f32>,
};

struct Tree {
    at: vec3<f32>,
    scale: f32,
    rotation: vec4<f32>,
    model: u32,
    seed: u32,
    block: u32,
    spare: u32,
};

// A model's far pictures: xyz the centre of its bounds in its own space, w their radius; then
// where its trees' list starts.
struct Model {
    bounds: vec4<f32>,
    base: u32,
    spare0: u32,
    spare1: u32,
    spare2: u32,
};

@group(0) @binding(0) var<uniform> cull: Cull;
@group(0) @binding(1) var<storage, read> trees: array<Tree>;
@group(0) @binding(2) var<storage, read> models: array<Model>;
// One bit a slot of trees: set for a block gone out of reach, or whose own objects stand in
// the world.
@group(0) @binding(3) var<storage, read> excluded: array<u32>;
@group(0) @binding(4) var<storage, read_write> visible: array<vec2<u32>>;
// Each model's draw: its index count, instance count, first index, base vertex and first
// instance; the instance count is counted here.
@group(0) @binding(5) var<storage, read_write> draws: array<atomic<u32>>;

// How much of the thinning `d` meters from the eye leaves standing: all of it to where the
// thinning starts, none at the reach.
fn standing(d: f32) -> f32 {
    let span = max(cull.reach.z - cull.reach.w, 1.0);
    return clamp(1.0 - (d - cull.reach.w) / span, 0.0, 1.0);
}

@compute @workgroup_size(64)
fn cs_cull(@builtin(global_invocation_id) id: vec3<u32>) {
    let i = id.x;
    if (i >= u32(cull.eye.w)) {
        return;
    }
    let t = trees[i];
    if ((excluded[t.block >> 5u] & (1u << (t.block & 31u))) != 0u) {
        return;
    }
    let foot = t.at - vec3<f32>(cull.reach.xy, 0.0);
    let d = length(foot - cull.eye.xyz);
    if (d > cull.reach.z) {
        return;
    }
    // The thinning: a tree goes once the share left standing falls below its own number, over
    // a short stretch.
    let left = standing(d);
    let own = f32(t.seed >> 8u) / 16777216.0;
    let band = max(cull.look.y, 1e-3);
    var keep = clamp((left - own) / band + 1.0, 0.0, 1.0);
    if (left >= 1.0) {
        keep = 1.0;
    }
    let m = models[t.model];
    let radius = m.bounds.w * t.scale;
    // A tree smaller on the screen than the least height fades out.
    let pixels = 2.0 * radius * cull.look.z / max(d, 1.0);
    keep = keep * smoothstep(cull.look.w * 0.5, cull.look.w, pixels);
    if (keep <= 0.0) {
        return;
    }
    // Its bounds, turned with it only about the upright: a tree is upright.
    let centre = foot + vec3<f32>(0.0, 0.0, m.bounds.z * t.scale);
    for (var p = 0; p < 6; p = p + 1) {
        if (dot(cull.planes[p].xyz, centre) + cull.planes[p].w < -radius) {
            return;
        }
    }
    let wider = min(1.0 + cull.look.x * (1.0 / max(left, 0.05) - 1.0), 1.5);
    let slot = atomicAdd(&draws[t.model * 5u + 1u], 1u);
    visible[m.base + slot] = vec2<u32>(i, pack2x16float(vec2<f32>(keep, wider)));
}
