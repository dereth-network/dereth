// The resolves: from the G-buffer to the world's colors, drawn over the whole target with the
// covering triangle, a pixel at a time.
//
// - `fs_vertex`, the light worked out per vertex: the surface's own color times the light the
//   G-buffer pass interpolated, fogged by the fog factor it interpolated. This is retail's own
//   arithmetic split in two.
// - `fs_pixel`, the light worked out per pixel from the frame's light inputs alone, by retail's
//   class rules: the ground by the sun and its ambient, objects outdoors by the sun and the
//   objects' ambient, objects indoors by their own light set, rooms by their baked burn of the
//   static lights (interpolated, as retail bakes it) and every dynamic light; fogged by the
//   pixel's own distance.
import atmosphere;
import bindless;
import fog;
import intent;
import retail;

struct Push
{
    uint frame;
    // The G-buffer's targets, by their index in the table.
    uint albedo;
    uint normal;
    uint light;
    uint material;
    uint depth;
    uint lights;
    uint sets;
    uint room_set;
    // The depth of the outdoor surfaces, drawn before the interior over them cleared the depth:
    // where the viewer stands inside, the ground and what the sun lights are placed by it.
    uint outdoor_depth;
    // How much of the sun reaches each pixel, by the shadows (~0: all of it everywhere).
    uint sun_visibility;
    // The lamps' light at each pixel, and in its alpha 1 where it replaces the fixed-function
    // lights' share (~0: no lamps).
    uint lamp_light;
    // How much of the ambient reaches each pixel, by the ambient occlusion (~0: all of it
    // everywhere).
    uint ambient_visibility;
    // The fog's constants and its volume (~0: retail's range fog, worked out here).
    uint fog;
    uint fog_volume;
    // The bounced light at each pixel, added (~0: none).
    uint gi;
    // The sky's constants, when they lay the clouds' shadows (~0: no such shadows).
    uint atmosphere;
};

[[vk::push_constant]]
ConstantBuffer<Push> push;

float4 at(uint t, int2 p)
{
    return textures[NonUniformResourceIndex(t)].Load(int3(p, 0));
}

// The finished color of a surface of own color `albedo` under light `light`, fogged by `fog`.
float4 finish(float3 albedo, float3 light, float fog)
{
    return resolved(push.frame, albedo, light, fog);
}

[shader("fragment")]
float4 fs_vertex(float4 position : SV_Position) : SV_Target0
{
    int2 p = int2(position.xy);
    if (uint(at(push.material, p).x * 255.0 + 0.5) == CLASS_NONE)
    {
        // Nothing of the G-buffer here: what the sky drew stays.
        discard;
    }
    float4 l = at(push.light, p);
    return finish(at(push.albedo, p).rgb, l.rgb, l.a);
}

[shader("fragment")]
float4 fs_pixel(float4 position : SV_Position) : SV_Target0
{
    int2 p = int2(position.xy);
    float4 g = at(push.albedo, p);
    float3 n = oct_decode(at(push.normal, p).xy);
    float4 l = at(push.light, p);
    uint4 mat = uint4(at(push.material, p) * 255.0 + 0.5);
    uint cls = mat.x;
    uint set = mat.z | (mat.w << 8);
    // The pixel's place in the frame's space, from its depth.
    float2 extent = row(push.frame, FRAME_EXTENT).xy;
    float2 ndc = float2(position.x / extent.x * 2.0 - 1.0, 1.0 - position.y / extent.y * 2.0);
    bool outdoors = cls == CLASS_GROUND || cls == CLASS_SUN;
    float d = at(outdoors ? push.outdoor_depth : push.depth, p).x;
    float4 h = transform(push.frame, FRAME_INV_VIEW_PROJ, float4(ndc, d, 1.0));
    float3 world = h.xyz / h.w;
    float dist = length(world - row(push.frame, FRAME_EYE).xyz);
    float fog = cls == CLASS_SKY || air_frame(push.frame) ? 1.0 : fog_factor(push.frame, dist);
    Terms t;
    t.cls = cls;
    t.emissive = l.rgb;
    t.burn = float3(0.0, 0.0, 0.0);
    t.ambient = row(push.frame, FRAME_OBJECT_AMBIENT).rgb;
    t.sun_color = float3(0.0, 0.0, 0.0);
    t.sun_facing = 0.0;
    t.sun = float3(0.0, 0.0, 0.0);
    t.lights = float3(0.0, 0.0, 0.0);
    float3 sun = row(push.frame, FRAME_SUN).xyz;
    if (cls == CLASS_GROUND)
    {
        // The ground takes the sun's color times N.D with D carrying its brightness, and its own
        // ambient.
        t.emissive = float3(0.0, 0.0, 0.0);
        t.ambient = row(push.frame, FRAME_GROUND_AMBIENT).rgb;
        t.sun_color = row(push.frame, FRAME_SUN_COLOR).rgb;
        t.sun_facing = max(dot(n, sun), 0.0);
    }
    else if (cls == CLASS_SUN || cls == CLASS_SKY)
    {
        if (dot(sun, sun) >= 1e-8)
        {
            float b = length(sun);
            t.sun_color = row(push.frame, FRAME_SUN_COLOR).rgb * b;
            t.sun_facing = max(dot(n, sun / b), 0.0);
        }
        t.sun = sun_light(push.frame, n);
    }
    else if (cls == CLASS_INDOOR)
    {
        t.lights = ff_lights(push.lights, push.sets, set, world, n);
    }
    else if (cls == CLASS_CELL)
    {
        // The static lights' burn is the room's own baked light, as retail bakes it at the
        // vertices and the G-buffer pass interpolated it; the dynamic lights are per pixel.
        t.burn = l.rgb;
        t.emissive = float3(0.0, 0.0, 0.0);
        t.lights = ff_lights(push.lights, push.sets, push.room_set, world, n);
    }
    else
    {
        // Nothing of the G-buffer here: what the sky drew stays.
        discard;
    }
    Masks m = unmasked(push.frame);
    if (push.sun_visibility != 0xFFFFFFFF && (cls == CLASS_GROUND || cls == CLASS_SUN))
    {
        m.sun_visibility = at(push.sun_visibility, p).x;
    }
    if (push.ambient_visibility != 0xFFFFFFFF)
    {
        m.ambient_visibility = at(push.ambient_visibility, p).x;
    }
    if (push.lamp_light != 0xFFFFFFFF)
    {
        float4 lamp = at(push.lamp_light, p);
        m.lamps = lamp.rgb;
        m.lamps_replace = max(lamp.a, 0.0);
        m.lamp_glow = max(-lamp.a, 0.0);
    }
    if (push.gi != 0xFFFFFFFF)
    {
        m.gi = at(push.gi, p).rgb;
    }
    if ((cls == CLASS_GROUND || cls == CLASS_SUN) && pack_lit_frame(push.frame))
    {
        m.pack_lit = true;
        float sunlit = m.sun_visibility;
        if (push.atmosphere != 0xFFFFFFFF)
        {
            sunlit *= cloud_light(push.atmosphere, world);
        }
        m.pack_light = pack_light(push.frame, n, sunlit, m.ambient_visibility,
                                  cls == CLASS_SUN, cls == CLASS_SUN && mat.y == KIND_FOLIAGE,
                                  float(mat.z) / 255.0, row(push.frame, FRAME_EYE).xyz - world);
    }
    if (push.fog_volume != 0xFFFFFFFF && cls != CLASS_SKY)
    {
        // The fog from its volume: what the fog in front of the surface keeps of it, and adds.
        float4 f = fog_at(push.fog, push.fog_volume, (float2(p) + 0.5) / extent, dist);
        return resolved_through(push.frame, g.rgb, compose(t, m), f);
    }
    return finish(g.rgb, compose(t, m), fog);
}
