// A building's door stamp: the far depth over the door, where nothing drawn before the first
// stamp stands before it. The recorded vertices stand in screen pixels, each with its true depth
// in its first picture coordinate.

struct Params {
    // xy the frame's pixels the stamps were recorded in; z and w the camera's near and far planes,
    // meters.
    target_near_far: vec4<f32>,
    // x how far past what was drawn a door may stand and still cut, meters; y how much more per
    // meter from the eye; z how far up the door the view may be clear, meters; w the pixels a
    // meter covers a meter from the eye.
    reach: vec4<f32>,
};

@group(4) @binding(0) var kept: texture_depth_2d;
@group(4) @binding(1) var<uniform> params: Params;

struct VsIn {
    @location(0) pos: vec4<f32>,
    @location(3) uv0: vec2<f32>,
};

struct VsOut {
    @builtin(position) pos: vec4<f32>,
    @location(0) door: f32,
};

struct Out {
    @location(0) colour: vec4<f32>,
    @location(1) normal: vec4<f32>,
};

@vertex
fn vs_main(i: VsIn) -> VsOut {
    var o: VsOut;
    let size = params.target_near_far.xy;
    o.pos = vec4<f32>(i.pos.x * 2.0 / size.x - 1.0, 1.0 - i.pos.y * 2.0 / size.y, i.pos.z, 1.0);
    o.door = i.uv0.x;
    return o;
}

// How far before a door, meters, and how much more per meter from the eye, what hides it may stand
// and still be the ground at its doorway, which the stamp cuts. What stands farther before it, a
// rise between the eye and the building, hides the door as it hides the wall beside it.
const AT_DOOR: vec2<f32> = vec2<f32>(3.0, 0.05);

// Meters from the eye of the depth `d`.
fn meters(d: f32) -> f32 {
    let near = params.target_near_far.z;
    let far = params.target_near_far.w;
    return near * far / (far - d * (far - near));
}

@fragment
fn fs_stamp(i: VsOut) -> Out {
    let door = meters(i.door);
    let here = meters(textureLoad(kept, vec2<i32>(i.pos.xy), 0));
    if (here < door - AT_DOOR.x - AT_DOOR.y * door) {
        discard;
    }
    let allowed = door - params.reach.x - params.reach.y * door;
    let rise = params.reach.z * params.reach.w / door;
    for (var k = 0; k < 6; k = k + 1) {
        let up = rise * f32(k) / 5.0;
        let at = vec2<i32>(i.pos.xy - vec2<f32>(0.0, up));
        if (at.y < 0 || meters(textureLoad(kept, at, 0)) >= allowed) {
            return Out(vec4<f32>(0.0), vec4<f32>(0.0));
        }
    }
    discard;
    return Out(vec4<f32>(0.0), vec4<f32>(0.0));
}
