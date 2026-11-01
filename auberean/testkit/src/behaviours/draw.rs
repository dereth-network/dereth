//! Drawing: how stored meshes are read for drawing and how the landscape's layers are laid on.
//!
//! A row claims what the September 2013 client does when it draws a mesh or a terrain layer.

use super::{Behaviour, Evidence, Tier, RETAIL};

/// This subject's rows, in id order.
pub static ROWS: &[Behaviour] = &[
    behaviour! {
        id: "draw.cells.a-cells-appearance-dresses-its-mesh-by-material-kind",
        says: "An interior cell that names an appearance key is drawn with that key's change set of the cells' appearance table: its mesh's materials are swapped by material kind, so one mesh serves a grassy hall and a sandy canyon.",
        since: RETAIL,
        evidence: Evidence::Private("AC2-EVID-DRAW-CELL-APPEARANCE"),
        station: "auberean-draw::dat::cells::a_cells_appearance_dresses_its_mesh_by_material_kind",
        tier: Tier::Dat,
    },
    behaviour! {
        id: "draw.decal.a-decal-fades-in-and-out-over-its-life",
        says: "A decal lives out its lifespan, fading in over its first tenth (a quarter second at \
               most) and out over its last fifth (a second at most); one drawn once shows whole at \
               once and is gone once drawn; its vertices take its color with its alpha times how \
               much shows.",
        since: RETAIL,
        evidence: Evidence::Private("AC2-EVID-DECAL-LIFE"),
        station: "auberean-draw::cpu::decal::a_decal_fades_in_and_out_over_its_life",
        tier: Tier::Cpu,
    },
    behaviour! {
        id: "draw.decal.a-decal-is-the-ground-cut-to-its-box",
        says: "A decal is the ground cut to its box (its width along its direction, its depth \
               across, its height up), each piece fanned from its first corner, its picture \
               coordinates its place across the box with the middle at one half, lifted fifteen \
               millimeters.",
        since: RETAIL,
        evidence: Evidence::Private("AC2-EVID-DECAL-CONSTRUCT"),
        station: "auberean-draw::cpu::decal::a_decal_is_the_ground_cut_to_its_box",
        tier: Tier::Cpu,
    },
    behaviour! {
        id: "draw.decal.a-landscape-decal-takes-the-nine-squares-about-it",
        says: "A decal on the landscape takes the ground of the nine squares about it, its own and \
               those a square's side away along each axis and diagonal.",
        since: RETAIL,
        evidence: Evidence::Private("AC2-EVID-DECAL-LANDSCAPE"),
        station: "auberean-draw::cpu::decal::a_landscape_decal_takes_the_nine_squares_about_it",
        tier: Tier::Cpu,
    },
    behaviour! {
        id: "draw.decal.a-point-light-lays-a-pool-of-its-light-below-it",
        says: "A point light standing above the land, less than its range up, lays a pool of its \
               light there each frame: a box twice its range on every side about it running north, \
               its color times its intensity, how much of it shows and one less its height over \
               its range, held to 0..1, drawn once with the night's, the twilight's or the day's \
               pool material by the time of day.",
        since: RETAIL,
        evidence: Evidence::Private("AC2-EVID-LIGHT-POOL"),
        station: "auberean-draw::cpu::decal::a_point_light_lays_a_pool_of_its_light_below_it",
        tier: Tier::Cpu,
    },
    behaviour! {
        id: "draw.decal.a-polygon-is-cut-to-a-planes-side",
        says: "A decal's polygon cut by a plane keeps the side where the plane's measure is not \
               negative, a point where an edge crosses put between the kept and the dropped point \
               with its normal taken along the edge; nothing is left when every point is dropped.",
        since: RETAIL,
        evidence: Evidence::Private("AC2-EVID-DECAL-CLIP"),
        station: "auberean-draw::cpu::decal::a_polygon_is_cut_to_a_planes_side",
        tier: Tier::Cpu,
    },
    behaviour! {
        id: "draw.decal.a-simple-shadow-is-a-dark-disc-under-the-body",
        says: "At the simple dynamic object shadows a body whose object casts a volume lays a dark \
               disc on the land under it each frame: 0.85 of its physics box wide and deep, two \
               meters high, running the way it faces, black with the shadow strength's three \
               channels summed and taken a third of in its alpha, drawn once with the disc \
               material.",
        since: RETAIL,
        evidence: Evidence::Private("AC2-EVID-DECAL-DISC"),
        station: "auberean-gpu::gpu::shadows::a_simple_shadow_is_a_dark_disc_under_the_body",
        tier: Tier::Gpu,
    },
    behaviour! {
        id: "draw.decal.a-torchs-pool-brightens-the-land",
        says: "A torch's light lays a warm pool on the land at night, drawn after the landscape: \
               the town's placed objects' lights brighten the land about them.",
        since: RETAIL,
        evidence: Evidence::Private("AC2-EVID-LIGHT-POOL-DRAW"),
        station: "auberean-gpu::gpu::shadows::a_torchs_pool_brightens_the_land",
        tier: Tier::Gpu,
    },
    behaviour! {
        id: "draw.decal.decals-are-drawn-by-material-within-a-groups-room",
        says: "Decals are kept in groups by material, a group made for each material found and \
               none for a decal in no cell; a group draws its decals together only while they fit \
               a vertex short of 8,192, else none of them, its decals drawn once then staying.",
        since: RETAIL,
        evidence: Evidence::Private("AC2-EVID-DECAL-GROUPS"),
        station: "auberean-draw::cpu::decal::decals_are_drawn_by_material_within_a_groups_room",
        tier: Tier::Cpu,
    },
    behaviour! {
        id: "draw.effects.a-fired-system-stands-at-its-effects-holding-location-or-bone",
        says: "A particle system an effect fires and forgets is set going at the effect's holding \
               location (that location's own place on the object), else at its bone (at its offset \
               when the object has no such bone), else on the water of the object's cell, else at \
               its offset: a creature's steps raise their dust at its feet.",
        since: RETAIL,
        evidence: Evidence::Private("AC2-EVID-DRAW-FIRED-PARTICLES"),
        station: "auberean-draw::dat::effects::a_fired_system_stands_at_its_effects_bone",
        tier: Tier::Dat,
    },
    behaviour! {
        id: "draw.effects.a-placed-light-fades-in-and-out",
        says: "A light an effect places on an object shows whole while it lives as long as its \
               object or fades in no time; else it fades in over its fade time and out over its \
               last, held to its object's opacity; it is let go of once its lifespan is spent.",
        since: RETAIL,
        evidence: Evidence::Private("AC2-EVID-EFFECT-LIGHT-FADE"),
        station: "auberean-draw::cpu::lights::a_placed_light_fades_in_and_out",
        tier: Tier::Cpu,
    },
    behaviour! {
        id: "draw.effects.an-appearances-effects-play-in-place-of-its-tables",
        says: "The effects an object's appearance lists for an effect id (each appearance atom's \
               effects for every id it names, gathered in order of their first ids, an atom of \
               the same first id replacing an earlier one) are the object's own for those ids, \
               looked up before the effect table its look names: the camp's Arcane Fire burns \
               its appearance's blue flames in place of its table's orange ones.",
        since: RETAIL,
        evidence: Evidence::Private("AC2-EVID-DRAW-APPEARANCE-EFFECTS"),
        station: "auberean-draw::dat::effects::the_arcane_fire_burns_its_appearances_blue_flames",
        tier: Tier::Dat,
    },
    behaviour! {
        id: "draw.effects.an-effects-appearance-is-laid-over-its-object-until-it-stops",
        says: "An effect that names an appearance table lays that table's value over the whole \
               object at the strength the effect has (its key's strength times the effect's; at \
               none or below the key comes off instead), as it starts, each time it is played \
               again while it plays, and as its strength ramps; asked to stop, it takes the key \
               off, while effects let go of with their object leave theirs: a wounded creature \
               bleeds until the wound stops.",
        since: RETAIL,
        evidence: Evidence::Private("AC2-EVID-DRAW-APPEARANCE-FX"),
        station: "auberean-draw::dat::effects::the_wounded_effect_lays_its_appearance_over_its_object_until_it_stops",
        tier: Tier::Dat,
    },
    behaviour! {
        id: "draw.effects.an-object-plays-its-looks-startup-effects-when-it-is-made",
        says: "An object whose look names startup effects plays them from the moment it is made, from \
               its look's own effect lists before the effect table the look names: a gateway's \
               swirl is two particle systems its look starts, which nothing on the wire asks for.",
        since: RETAIL,
        evidence: Evidence::Private("AC2-EVID-DRAW-STARTUP-EFFECTS"),
        station: "auberean-draw::dat::effects::the_quick_exit_plays_its_swirls_from_its_look",
        tier: Tier::Dat,
    },
    behaviour! {
        id: "draw.effects.placed-objects-torches-light-the-land",
        says: "The objects placed with the world place the lights their startup effects name, as \
               point lights at the effects' offsets: the town's torches are orange lights of range \
               8, and those less than their range above the land lay a pool of their light there.",
        since: RETAIL,
        evidence: Evidence::Private("AC2-EVID-EFFECT-LIGHTS-PLACED"),
        station: "auberean-draw::dat::effects::placed_objects_torches_light_the_land",
        tier: Tier::Dat,
    },
    behaviour! {
        id: "draw.frame.the-water-is-drawn-before-what-is-seen-through",
        says: "A frame of the world draws its opaque objects, then the water, then the objects seen \
               through, the particle systems last: a flame over a lake adds itself onto the water \
               behind it rather than being covered by it.",
        since: RETAIL,
        evidence: Evidence::Private("AC2-EVID-DRAW-FRAME-ORDER"),
        station: "auberean-draw::dat::effects::the_arcane_fires_flames_are_drawn_after_the_water",
        tier: Tier::Dat,
    },
    behaviour! {
        id: "draw.inside.a-closed-room-shows-nothing-outdoors",
        says: "From a room whose cells see neither the sky nor a doorway out, a frame draws nothing                outdoors: no sky, no land, and no light outdoors.",
        since: RETAIL,
        evidence: Evidence::Private("AC2-EVID-DRAW-INSIDE-CLOSED"),
        station: "auberean-draw::dat::outdoors::a_closed_room_shows_nothing_outdoors",
        tier: Tier::Dat,
    },
    behaviour! {
        id: "draw.inside.a-doorway-narrows-the-view-to-what-shows-through-it",
        says: "Seen from inside, a doorway is clipped to the view it is seen through (whole when the \
               camera stands within a centimeter of its plane, nothing when it lies outside a plane), and \
               the view through it is one plane through the camera along each edge of what shows, \
               holding what lies beyond; a doorway clipped to 32 corners or more keeps the view it was \
               seen through.",
        since: RETAIL,
        evidence: Evidence::Private("AC2-EVID-DRAW-CULL-DOORWAY"),
        station: "auberean-draw::cpu::cull::a_doorway_narrows_the_view_to_what_shows_through_it",
        tier: Tier::Cpu,
    },
    behaviour! {
        id: "draw.inside.a-doorway-out-shows-the-land-under-the-light-outdoors",
        says: "From a room whose cells have a doorway out, a frame draws the land outside as a                frame outdoors would (its ground, its objects, its water, the sky) under the light                and fog outdoors, and the rooms and what stands in them under the interior's light;                the walls hide the land but where the doorways open.",
        since: RETAIL,
        evidence: Evidence::Private("AC2-EVID-DRAW-INSIDE-LAND"),
        station: "auberean-draw::dat::outdoors::a_doorway_out_shows_the_land_under_the_light_outdoors",
        tier: Tier::Dat,
    },
    behaviour! {
        id: "draw.inside.a-flickering-light-lights-its-draws-as-it-stands-at-the-moment",
        says: "A light whose intensity moves (by noise, a sine, a bounce or the gradient noise)                lights the objects near it as its intensity stands at the moment, taken once a                frame for every draw it lights, the rooms it is kept by among them; lit particles take it                afresh for each particle; the rooms' own vertices keep the light their record lists as it                stood when they were baked.",
        since: RETAIL,
        evidence: Evidence::Private("AC2-EVID-DRAW-LIGHT-FLICKER"),
        station: "auberean-draw::dat::inside::a_flickering_light_lights_its_draws_as_it_stands_at_the_moment",
        tier: Tier::Dat,
    },
    behaviour! {
        id: "draw.inside.a-lit-particle-inside-is-lit-by-the-lights-kept-where-its-system-stands",
        says: "Inside, a lit particle system is lit by the lights kept where it stands, reaching no \
               farther than its place, in place of the sun: at most five, each particle by its own \
               distance from them; with none about it, its particles are black.",
        since: RETAIL,
        evidence: Evidence::Private("AC2-EVID-PARTICLE-LIGHTS-HALL"),
        station: "auberean-draw::dat::inside::a_lit_particle_inside_is_lit_by_the_lights_kept_where_its_system_stands",
        tier: Tier::Dat,
    },
    behaviour! {
        id: "draw.inside.a-room-open-to-the-sky-shows-the-sky",
        says: "From a room whose cells include one open to the sky, a frame draws the sky under the                light outdoors, with no land, whether or not a doorway leads out.",
        since: RETAIL,
        evidence: Evidence::Private("AC2-EVID-DRAW-INSIDE-SKY"),
        station: "auberean-draw::dat::outdoors::a_room_open_to_the_sky_shows_the_sky_and_no_land",
        tier: Tier::Dat,
    },
    behaviour! {
        id: "draw.inside.a-room-past-the-fog-is-left-out-by-its-own-distance",
        says: "Seen from inside, a room other than the camera's whose box lies wholly past the interior \
               fog's far distance from the camera is left out, with what lies beyond it, and the \
               background is painted the fog's color; the distance is the one the room itself took \
               when it last held the camera, so a room the camera never stood in is never left out.",
        since: RETAIL,
        evidence: Evidence::Private("AC2-EVID-DRAW-CULL-FOG"),
        station: "auberean-draw::cpu::cull::a_room_past_the_fog_is_left_out_by_its_own_distance",
        tier: Tier::Cpu,
    },
    behaviour! {
        id: "draw.inside.an-object-inside-is-lit-by-the-lights-it-keeps",
        says: "Inside, an object is lit by the lights it keeps where it stands, the heaviest there first, in place of the sun: as many point lights as its layer's program takes, at most four, each fading as 1 / (1 + 10 d squared / R squared); a normal-mapped layer lights its normal map from the first and adds the others to its second color.",
        since: RETAIL,
        evidence: Evidence::Private("AC2-EVID-DRAW-OBJECT-LIGHTS"),
        station: "auberean-draw::dat::inside::an_object_inside_is_lit_by_the_lights_it_keeps",
        tier: Tier::Dat,
    },
    behaviour! {
        id: "draw.inside.every-doorway-runs-clockwise-from-inside-its-cell",
        says: "Every doorway of every interior cell is stored running clockwise as seen from inside its \
               cell, and names side 1, so the view made from its corners in their order holds what \
               lies beyond it; no doorway is turned round.",
        since: RETAIL,
        evidence: Evidence::Private("AC2-EVID-DRAW-CULL-WINDING"),
        station: "auberean-draw::dat::cull::every_doorway_runs_clockwise_from_inside_its_cell",
        tier: Tier::Dat,
    },
    behaviour! {
        id: "draw.inside.never-culled-rooms-are-taken-with-their-group",
        says: "Seen from inside, rooms never culled are entered with the rooms drawn with them, each \
               whole whatever the view; the group's doorways out add their whole views out, and the \
               culled rooms beyond are walked on with the view the group was entered by.",
        since: RETAIL,
        evidence: Evidence::Private("AC2-EVID-DRAW-CULL-GROUP"),
        station: "auberean-draw::cpu::cull::never_culled_rooms_are_taken_with_their_group",
        tier: Tier::Cpu,
    },
    behaviour! {
        id: "draw.inside.the-cameras-view-is-looser-than-its-picture",
        says: "The view the walk through the doorways starts from reaches across half the field of view \
               times the picture's shape, and up and down to ninety degrees less half the field of \
               view, with a near plane; looser than the picture.",
        since: RETAIL,
        evidence: Evidence::Private("AC2-EVID-DRAW-CULL-CAMERA"),
        station: "auberean-draw::cpu::cull::the_cameras_view_is_looser_than_its_picture",
        tier: Tier::Cpu,
    },
    behaviour! {
        id: "draw.inside.the-fog-paints-the-background-where-rooms-are-left-out",
        says: "When a room is left out of a frame seen from inside for the interior fog, the frame's                background is painted the fog's color, blended by its alpha over black, before anything                else is drawn: it shows where the rooms left out would have been.",
        since: RETAIL,
        evidence: Evidence::Private("AC2-EVID-DRAW-CULL-FOG-PAINT"),
        station: "auberean-gpu::gpu::inside::the_fog_paints_the_background_where_rooms_are_left_out",
        tier: Tier::Gpu,
    },
    behaviour! {
        id: "draw.inside.the-frame-is-chosen-by-the-cell-the-camera-stands-in",
        says: "What a frame draws is chosen by the cell the camera stands in, not the player's: a                room, or a room it sees, that holds the camera; past them, the land outside where one                has a doorway out (else the room the camera started from); from the land, a room of                the block with a doorway out that holds the camera, else the land.",
        since: RETAIL,
        evidence: Evidence::Private("AC2-EVID-DRAW-CAMERA-CELL"),
        station: "auberean-physics::dat::camera::the_camera_stands_in_the_room_that_holds_it_or_on_the_land",
        tier: Tier::Dat,
    },
    behaviour! {
        id: "draw.inside.the-land-shows-only-through-a-doorway-in-view",
        says: "Seen from inside, the land, its water and its sky are drawn only while a doorway out \
               shows, and of the objects outdoors only those in the view through it; facing into the \
               cave, none is drawn.",
        since: RETAIL,
        evidence: Evidence::Private("AC2-EVID-DRAW-CULL-LAND"),
        station: "auberean-draw::dat::cull::the_land_shows_only_through_a_doorway_in_view",
        tier: Tier::Dat,
    },
    behaviour! {
        id: "draw.inside.the-walk-reaches-the-rooms-the-doorways-show",
        says: "Seen from inside, the rooms drawn are those the walk from the camera's room reaches \
               through doorways in view, each narrowing the view, among the rooms the camera's room \
               sees; a doorway out adds a view onto the land.",
        since: RETAIL,
        evidence: Evidence::Private("AC2-EVID-DRAW-CULL-WALK"),
        station: "auberean-draw::cpu::cull::the_walk_reaches_the_rooms_the_doorways_show",
        tier: Tier::Cpu,
    },
    behaviour! {
        id: "draw.material.a-device-keeps-its-classs-layers-with-stages",
        says: "A device keeps of a material the layers made for its hardware class that have stages, and the parameter fields that write to them, renumbered; a layer's vertex program takes each of its first four stages' coordinate source (when the stage has a texture, a renderer's texture or a source) and a texture matrix for each stage an animation moves.",
        since: RETAIL,
        evidence: Evidence::Private("AC2-EVID-DRAW-SUB-OBJECTS"),
        station: "auberean-draw::dat::materials::a_device_keeps_its_classs_layers_with_stages",
        tier: Tier::Dat,
    },
    behaviour! {
        id: "draw.material.a-half-vector-stage-feeds-the-per-pixel-specular",
        says: "The layers whose pixel program lights the specular per pixel from the half vector are \
               exactly those handing a stage, without a texture, the half vector between the light \
               and the eye in the surface's frame as its coordinates: six materials.",
        since: RETAIL,
        evidence: Evidence::Private("AC2-EVID-HALF-VECTOR-SPECULAR"),
        station: "auberean-draw::dat::materials::a_half_vector_stage_feeds_the_per_pixel_specular",
        tier: Tier::Dat,
    },
    behaviour! {
        id: "draw.material.a-layers-programs-cover-its-light-counts",
        says: "A dynamically lit layer has a vertex program for each count up to one directional and four \
               point lights (never neither, leaving out a kind it ignores); a layer lit per pixel by one \
               light has a directional and a point program; any other one with no light. A device with \
               more lights than any program takes uses the program with the most it has, and with none the \
               first: by its switches, an interior cell's layer takes four of the six lights kept about \
               it (see the next row for a program too long for the device).",
        since: RETAIL,
        evidence: Evidence::Private("AC2-EVID-DRAW-LIGHT-VARIANTS"),
        station: "auberean-draw::cpu::material::a_layers_programs_cover_its_light_counts",
        tier: Tier::Cpu,
    },
    behaviour! {
        id: "draw.material.a-modifier-sets-power-and-coordinate-scale",
        says: "A material modifier's specular power and its scale of a stage's texture \
               coordinates reach the layer through the material's parameter of the same name.",
        since: RETAIL,
        evidence: Evidence::Private("AC2-EVID-DRAW-MATERIAL-FIELDS"),
        station: "auberean-draw::cpu::material::a_modifier_sets_power_and_coordinate_scale",
        tier: Tier::Cpu,
    },
    behaviour! {
        id: "draw.material.a-moving-material-is-drawn-as-it-stands-at-the-moment",
        says: "A material whose pictures slide, turn or stretch, or whose specular power or alpha                reference moves, is drawn as its waveforms stand at the moment of the client's run,                each draw taking them afresh: the specular power, the alpha reference, then each                stage's animations in their order (v before u).",
        since: RETAIL,
        evidence: Evidence::Private("AC2-EVID-DRAW-MATERIAL-MOTION"),
        station: "auberean-draw::dat::materials::a_moving_material_is_drawn_as_it_stands_at_the_moment",
        tier: Tier::Dat,
    },
    behaviour! {
        id: "draw.material.a-pixel-program-is-read-from-its-instructions",
        says: "A normal-mapped layer's pixel program may add a detail map to its diffuse or its \
               normal map, take its alpha from the diffuse map, gloss its specular with it, or add \
               an environment map to it, alone or times its alpha.",
        since: RETAIL,
        evidence: Evidence::Private("AC2-EVID-DRAW-PIXEL-PROGRAMS"),
        station: "auberean-draw::cpu::material::a_pixel_program_is_read_from_its_instructions",
        tier: Tier::Cpu,
    },
    behaviour! {
        id: "draw.material.a-program-too-long-for-the-device-is-not-made",
        says: "A layer has no vertex program for a count of lights whose program is longer than the \
               device takes (128 instructions once its pieces are joined), and a draw with that many \
               lights takes the nearest program that was made: of the install's materials, 28 \
               normal-mapped layers' programs for four point lights (one for three and the sun) are too \
               long, the interior cells' among them, so a cell is lit by at most three point lights.",
        since: RETAIL,
        evidence: Evidence::Private("AC2-EVID-DRAW-PROGRAM-LENGTH"),
        station: "auberean-draw::dat::materials::a_program_too_long_for_the_device_is_not_made",
        tier: Tier::Dat,
    },
    behaviour! {
        id: "draw.material.a-shiny-surface-shows-its-environment-map",
        says: "A surface whose pixel program adds an environment map shows it: looked up by the \
               eye's reflection about the surface, taken along the camera's right and up, halved and \
               raised by one half, it brightens the surface where it is drawn.",
        since: RETAIL,
        evidence: Evidence::Private("AC2-EVID-ENVIRONMENT-MAP-SHINE"),
        station: "auberean-gpu::gpu::materials::a_shiny_surface_shows_its_environment_map",
        tier: Tier::Gpu,
    },
    behaviour! {
        id: "draw.material.an-animation-field-sets-its-first-or-second-waveform",
        says: "A material parameter's coordinate-animation field sets the animation in that place of the layer's list by its place in the animation: the first waveform (a slide's u, a turn, a stretch's u) or the second (a slide's or a stretch's v).",
        since: RETAIL,
        evidence: Evidence::Private("AC2-EVID-DRAW-MATERIAL-ANIMATION-FIELDS"),
        station: "auberean-draw::cpu::material::an_animation_field_sets_its_first_or_second_waveform",
        tier: Tier::Cpu,
    },
    behaviour! {
        id: "draw.material.an-anisotropic-stage-looks-up-the-instances-map",
        says: "The stages reading the anisotropic coordinates (how squarely the light and the half \
               vector meet the surface) are three materials' anisotropic maps, and every instance of \
               those in use names a picture for that stage.",
        since: RETAIL,
        evidence: Evidence::Private("AC2-EVID-ANISOTROPIC-COORDINATES"),
        station: "auberean-draw::dat::materials::an_anisotropic_stage_looks_up_the_instances_map",
        tier: Tier::Dat,
    },
    behaviour! {
        id: "draw.material.an-environment-map-is-looked-up-by-the-eyes-reflection",
        says: "The stages reading the eye's reflection about the surface are three materials' \
               environment maps; the pixel programs of the two in use add the map to the diffuse \
               map, one times the diffuse map's alpha, and every one of their 42 instances names a \
               picture for it.",
        since: RETAIL,
        evidence: Evidence::Private("AC2-EVID-ENVIRONMENT-MAPS"),
        station: "auberean-draw::dat::materials::an_environment_map_is_looked_up_by_the_eyes_reflection",
        tier: Tier::Dat,
    },
    behaviour! {
        id: "draw.material.coordinate-animations-apply-in-order",
        says: "A layer's texture-coordinate slides, turns and scales apply to a stage's \
               coordinates in the order the layer lists them.",
        since: RETAIL,
        evidence: Evidence::Private("AC2-EVID-DRAW-UV-TRANSFORM"),
        station: "auberean-draw::cpu::material::coordinate_animations_apply_in_order",
        tier: Tier::Cpu,
    },
    behaviour! {
        id: "draw.material.every-material-instance-is-drawn-with-a-layer",
        says: "Every material instance in the data is made ready from its material and modifiers and has a layer the shader-capable device draws; every parameter field reaches a layer, and every coordinate-animation field an animation, its material has.",
        since: RETAIL,
        evidence: Evidence::Private("AC2-EVID-DRAW-MATERIAL-INSTANCES"),
        station: "auberean-draw::dat::materials::every_material_instance_is_drawn_with_a_layer",
        tier: Tier::Dat,
    },
    behaviour! {
        id: "draw.material.specular-is-off-at-zero-power",
        says: "A layer whose specular power is zero draws no specular light, even with \
               highlights switched on, unless its pixel program adds specular itself.",
        since: RETAIL,
        evidence: Evidence::Private("AC2-EVID-DRAW-SPECULAR-OFF"),
        station: "auberean-draw::cpu::material::specular_is_off_at_zero_power",
        tier: Tier::Cpu,
    },
    behaviour! {
        id: "draw.mesh.a-vertex-format-lays-its-elements-out-in-order",
        says: "A stored vertex holds its position, blend weights, normal, point size, diffuse and \
               specular colors, tangent-space vectors and coordinate pairs in that order, sized \
               by its format word.",
        since: RETAIL,
        evidence: Evidence::Private("AC2-EVID-DRAW-VERTEX-LAYOUT"),
        station: "auberean-draw::cpu::layout::a_vertex_format_lays_its_elements_out_in_order",
        tier: Tier::Cpu,
    },
    behaviour! {
        id: "draw.mesh.detail-falls-from-near-to-far",
        says: "An object is drawn at full detail inside its near distance, at half at its ideal \
               distance, at the least past its far distance, and not at all beyond its visible \
               distance.",
        since: RETAIL,
        evidence: Evidence::Private("AC2-EVID-DRAW-DETAIL"),
        station: "auberean-draw::cpu::layout::detail_falls_from_near_to_far",
        tier: Tier::Cpu,
    },
    behaviour! {
        id: "draw.particles.a-repulsor-moves-at-its-objects-velocity",
        says: "A repulsor's speed is its object's velocity as its physics moves it, not how far \
               the object was carried since the last frame: a human running in place parts the \
               start hall's mist about it; one standing still does not.",
        since: RETAIL,
        evidence: Evidence::Private("AC2-EVID-PARTICLE-REPULSOR-SPEED"),
        station: "auberean-draw::dat::effects::a_human_running_in_place_parts_the_mist",
        tier: Tier::Dat,
    },
    behaviour! {
        id: "draw.particles.a-repulsor-pushes-the-particles-in-its-range",
        says: "A repulsor an object's effect carries pushes the particles moved by repulsors within \
               its range along the way from it, harder the nearer (a (1 - d / range)), not along \
               the emitter's constrained axes; an attractor pulls. It pushes while its object \
               moves at least the speed it asks for and within its lifetime (none under 0.0002 \
               s), and goes once it has outlived that or its object lets it go: the start hall's \
               mist parts around a walking human. A system thinks once a whole step has passed, \
               the time held to the step at full precision.",
        since: RETAIL,
        evidence: Evidence::Private("AC2-EVID-PARTICLE-REPULSORS"),
        station: "auberean-draw::cpu::particles::a_repulsor_pushes_the_particles_in_its_range",
        tier: Tier::Cpu,
    },
    behaviour! {
        id: "draw.particles.a-square-emitter-without-fill-starts-particles-on-its-edges",
        says: "A particle emitter shaped as a square and not filled starts each particle on one of \
               the square's four edges.",
        since: RETAIL,
        evidence: Evidence::Private("AC2-EVID-DRAW-PARTICLE-SHAPES"),
        station: "auberean-draw::cpu::particles::a_square_emitter_without_fill_starts_particles_on_its_edges",
        tier: Tier::Cpu,
    },
    behaviour! {
        id: "draw.particles.a-streak-stretches-from-its-tail-to-its-head",
        says: "A streaking particle is drawn from where it was the streak's time ago to where it \
               is, as wide as its size across the way it runs on the screen.",
        since: RETAIL,
        evidence: Evidence::Private("AC2-EVID-DRAW-PARTICLE-STREAKS"),
        station: "auberean-draw::cpu::particles::a_streak_stretches_from_its_tail_to_its_head",
        tier: Tier::Cpu,
    },
    behaviour! {
        id: "draw.particles.a-system-goes-once-it-is-spent-and-its-particles-are-dead",
        says: "A particle system goes once none of its emitters has anything more to discharge (or it \n               was stopped) and its last particles have died.",
        since: RETAIL,
        evidence: Evidence::Private("AC2-EVID-DRAW-PARTICLE-DEATH"),
        station: "auberean-draw::cpu::particles::a_system_goes_once_it_is_spent_and_its_particles_are_dead",
        tier: Tier::Cpu,
    },
    behaviour! {
        id: "draw.particles.a-system-without-limits-runs-until-stopped",
        says: "A particle system with a physics duration, or with an emitter that has neither an \n               emission limit nor a time limit, runs until it is stopped; any other lasts about its \n               longest emitter's start, lifespan and emitting time less its fast-forward.",
        since: RETAIL,
        evidence: Evidence::Private("AC2-EVID-DRAW-PARTICLE-LIFETIME"),
        station: "auberean-draw::cpu::particles::a_system_without_limits_runs_until_stopped",
        tier: Tier::Cpu,
    },
    behaviour! {
        id: "draw.particles.a-thrown-particle-falls-with-gravity-times-its-mass",
        says: "A thrown particle moves from where it was born along its velocity, pulled by \
               gravity times its mass (a negative mass rises), and is drawn one physics step \
               behind.",
        since: RETAIL,
        evidence: Evidence::Private("AC2-EVID-DRAW-PARTICLE-PHYSICS"),
        station: "auberean-draw::cpu::particles::a_thrown_particle_falls_with_gravity_times_its_mass",
        tier: Tier::Cpu,
    },
    behaviour! {
        id: "draw.particles.a-timed-emitter-stops-at-its-emission-limit",
        says: "A particle emitter discharges its blast every birth interval and stops once it has \
               discharged its emission limit.",
        since: RETAIL,
        evidence: Evidence::Private("AC2-EVID-DRAW-PARTICLE-EMISSION"),
        station: "auberean-draw::cpu::particles::a_timed_emitter_stops_at_its_emission_limit",
        tier: Tier::Cpu,
    },
    behaviour! {
        id: "draw.particles.a-world-system-reckons-distance-from-where-it-is-first-placed",
        says: "A particle system whose particles live in the world places each emitter where the \
               system is first queued, so an emitter discharging by distance discharges for the way \
               the system moves from there, not for the way from the world's origin.",
        since: RETAIL,
        evidence: Evidence::Private("AC2-EVID-DRAW-PARTICLE-PLACE"),
        station: "auberean-draw::cpu::particles::a_world_system_reckons_distance_from_where_it_is_first_placed",
        tier: Tier::Cpu,
    },
    behaviour! {
        id: "draw.particles.color-and-size-blend-toward-the-next-keyframe-that-blends-them",
        says: "A particle's color and size blend from its last keyframe toward the next keyframe \
               that blends them, by its age between the two.",
        since: RETAIL,
        evidence: Evidence::Private("AC2-EVID-DRAW-PARTICLE-KEYFRAMES"),
        station: "auberean-draw::cpu::particles::color_and_size_blend_toward_the_next_keyframe_that_blends_them",
        tier: Tier::Cpu,
    },
    behaviour! {
        id: "draw.particles.every-stored-system-runs-until-stopped-or-for-a-time",
        says: "Every particle system in the data either runs until it is stopped or lasts a time its \n               emitters give; about two in three run until stopped.",
        since: RETAIL,
        evidence: Evidence::Private("AC2-EVID-DRAW-PARTICLE-LIFETIMES-DATA"),
        station: "auberean-draw::dat::effects::every_particle_system_has_a_lifetime",
        tier: Tier::Dat,
    },
    behaviour! {
        id: "draw.particles.keyframes-link-to-the-next-that-blends-each-value",
        says: "Each particle keyframe leads to the next keyframe blending its size, its mass and its \
               color, or to itself when none does.",
        since: RETAIL,
        evidence: Evidence::Private("AC2-EVID-DRAW-PARTICLE-KEY-LINKS"),
        station: "auberean-draw::cpu::particles::keyframes_link_to_the_next_that_blends_each_value",
        tier: Tier::Cpu,
    },
    behaviour! {
        id: "draw.particles.waveforms-take-their-shape-at-a-time",
        says: "A waveform's value at a time is its base when steady, grows with the time at speed, \
               follows a sine of its frequency and phase (sine, square, bounce), or rolls within \
               its amplitude.",
        since: RETAIL,
        evidence: Evidence::Private("AC2-EVID-DRAW-WAVEFORMS"),
        station: "auberean-draw::cpu::particles::waveforms_take_their_shape_at_a_time",
        tier: Tier::Cpu,
    },
    behaviour! {
        id: "draw.picture.a-textures-levels-are-its-stored-levels",
        says: "A texture's levels of detail are its picture list's levels as stored, each half the \
               one before, never made by averaging the largest: a branch's needles keep their \
               clear-or-solid edges at every distance.",
        since: RETAIL,
        evidence: Evidence::Private("AC2-EVID-DRAW-TEXTURE-LEVELS"),
        station: "auberean-draw::dat::pictures::a_textures_levels_are_its_picture_lists_stored_levels",
        tier: Tier::Dat,
    },
    behaviour! {
        id: "draw.picture.texture-detail-leaves-out-the-largest-levels",
        says: "Texture detail from lowest to highest leaves out the four, three, two or one largest \
               levels of a picture list, none at the highest when its high detail is kept, never \
               every level.",
        since: RETAIL,
        evidence: Evidence::Private("AC2-EVID-DRAW-TEXTURE-DETAIL"),
        station: "auberean-draw::cpu::picture::texture_detail_leaves_out_the_largest_levels",
        tier: Tier::Cpu,
    },
    behaviour! {
        id: "draw.portal.portal-space-comes-in-and-goes-out-a-second-a-phase",
        says: "Portal space comes in over a second (the world's view narrowing to a fifth under a black veil while the tunnel and the nebula fade in), stays in with no world drawn, then goes out over a second (the tunnel's view widening to 2.4 times as white fades in) and comes out over another (the world's view widening back as the white fades out); started at once, it is in at once.",
        since: RETAIL,
        evidence: Evidence::Private("AC2-EVID-PORTAL-PHASES"),
        station: "auberean-draw::cpu::portal::portal_space_comes_in_and_goes_out_a_second_a_phase",
        tier: Tier::Cpu,
    },
    behaviour! {
        id: "draw.portal.the-tunnel-is-rings-three-meters-apart-the-camera-rushes-along",
        says: "The tunnel is 150 rings 3 m apart along a winding tube, each of radius 10 + 6 sin(0.01 d) at its distance d; the camera rushes along at 120 m/s, the rings it passes made again at the far end; the rings fade from 1.0 nearest to 0.8 at the sixteenth and from there to nothing at the far end.",
        since: RETAIL,
        evidence: Evidence::Private("AC2-EVID-PORTAL-TUNNEL"),
        station: "auberean-draw::cpu::portal::the_tunnel_is_rings_three_meters_apart_the_camera_rushes_along",
        tier: Tier::Cpu,
    },
    behaviour! {
        id: "draw.quality.an-overall-quality-sets-the-graphics-preferences",
        says: "Each of five overall graphics qualities sets every graphics preference but the screen's \
               brightness, values and defaults; the quality the preferences hold is the first of the \
               five whose every preference but the adaptive degrade's bias they match, else none.",
        since: RETAIL,
        evidence: Evidence::Private("AC2-EVID-RENDER-QUALITY"),
        station: "auberean-draw::cpu::quality::an_overall_quality_sets_the_graphics_preferences",
        tier: Tier::Cpu,
    },
    behaviour! {
        id: "draw.shadow.cloud-shadows-slide-with-the-running-time",
        says: "The clouds' shadow picture lies flat over the landscape at its scale and slides by \
               its speeds times the client's running time.",
        since: RETAIL,
        evidence: Evidence::Private("AC2-EVID-DRAW-CLOUD-SHADOWS"),
        station: "auberean-draw::cpu::shadow::cloud_shadows_slide_with_the_running_time",
        tier: Tier::Cpu,
    },
    behaviour! {
        id: "draw.shadow.the-map-spans-the-near-terrain-from-the-sun",
        says: "The sun's shadow map covers the detailed terrain's extent as the sun sees it, and a \
               shadow in it takes away 0.95 of the day's shadow color.",
        since: RETAIL,
        evidence: Evidence::Private("AC2-EVID-DRAW-SHADOW-MAP"),
        station: "auberean-draw::cpu::shadow::the_map_spans_the_near_terrain_from_the_sun",
        tier: Tier::Cpu,
    },
    behaviour! {
        id: "draw.shadow.the-sun-looks-along-its-light-with-up-kept-up",
        says: "The shadow map looks along the sun's light, with the world's up kept as near up as \
               it can be, or north when the light falls straight down.",
        since: RETAIL,
        evidence: Evidence::Private("AC2-EVID-DRAW-SHADOW-FRAME"),
        station: "auberean-draw::cpu::shadow::the_sun_looks_along_its_light_with_up_kept_up",
        tier: Tier::Cpu,
    },
    behaviour! {
        id: "draw.systems.a-system-set-going-on-its-own-draws-until-it-is-spent",
        says: "A particle system set going on its own, owned by no object, draws where it was set \
               each frame until it is spent and its particles are dead, then goes.",
        since: RETAIL,
        evidence: Evidence::Private("AC2-EVID-DRAW-AUTONOMOUS"),
        station: "auberean-draw::dat::effects::a_system_set_going_on_its_own_draws_until_it_is_spent",
        tier: Tier::Dat,
    },
    behaviour! {
        id: "draw.systems.objects-placed-with-the-world-share-their-particle-systems",
        says: "A particle system whose material is unlit and whose particles live with their object serves \
               every object placed with the world that asks for its record, drawn at each; an object the \
               server makes takes a shared system too but forbids sharing it from then on. A system its last \
               owner lets go of discharges no more; one that may not be shared draws where it was left until \
               its particles are gone.",
        since: RETAIL,
        evidence: Evidence::Private("AC2-EVID-DRAW-PARTICLE-SHARING"),
        station: "auberean-draw::dat::effects::objects_placed_with_the_world_share_their_particle_systems",
        tier: Tier::Dat,
    },
    behaviour! {
        id: "draw.terrain.a-mask-turns-a-quarter-per-step",
        says: "A terrain layer's mask covers one ten-meter square and turns a quarter clockwise \
               for each step of its rotation.",
        since: RETAIL,
        evidence: Evidence::Private("AC2-EVID-DRAW-MASK-TURN"),
        station: "auberean-draw::cpu::layout::a_mask_turns_a_quarter_per_step",
        tier: Tier::Cpu,
    },
    behaviour! {
        id: "draw.volume.a-bodys-shadow-darkens-the-ground-and-not-the-body",
        says: "At the volumetric quality a body's volumes of shadow darken what they enclose: \
               counted in the stencil where the sight to the nearest surface passes through them \
               (a face failing the depth test counts up turned away, down turned toward the \
               camera), and those places drawn times one less the shadow strength; the body's own \
               draws come after, so its shadow never falls on it.",
        since: RETAIL,
        evidence: Evidence::Private("AC2-EVID-VOLUME-DRAW"),
        station: "auberean-gpu::gpu::shadows::a_bodys_shadow_darkens_the_ground_and_not_the_body",
        tier: Tier::Gpu,
    },
    behaviour! {
        id: "draw.volume.a-closed-mesh-casts-a-closed-volume-away-from-the-light",
        says: "A mesh's volume of shadow is its triangles facing the light (met edge on included) \
               pushed a thousandth of a meter along the light, again sixty meters along it turned \
               to face out, and a side along each of their edges whose other triangle does not \
               face the light: a closed mesh casts a closed volume.",
        since: RETAIL,
        evidence: Evidence::Private("AC2-EVID-VOLUME-BUILD"),
        station: "auberean-draw::cpu::volume::a_closed_mesh_casts_a_closed_volume_away_from_the_light",
        tier: Tier::Cpu,
    },
    behaviour! {
        id: "draw.volume.a-player-casts-a-closed-volume-of-shadow",
        says: "A player casts a volume of shadow: the entity a player is made from casts one among \
               its drawing properties, and the posed body casts a closed volume from each of its \
               meshes of more than three triangles.",
        since: RETAIL,
        evidence: Evidence::Private("AC2-EVID-VOLUME-PLAYER"),
        station: "auberean-draw::dat::volumes::a_player_casts_a_closed_volume_of_shadow",
        tier: Tier::Dat,
    },
    behaviour! {
        id: "draw.volume.a-shadow-takes-away-the-shadow-color-or-gray-inside",
        says: "A volume's shadow takes away the shadow color of the light outdoors, held to 0..1 \
               with alpha one, and a mid gray (0.45) inside.",
        since: RETAIL,
        evidence: Evidence::Private("AC2-EVID-VOLUME-STRENGTH"),
        station: "auberean-draw::cpu::volume::a_shadow_takes_away_the_shadow_color_or_gray_inside",
        tier: Tier::Cpu,
    },
    behaviour! {
        id: "draw.volume.a-volume-is-made-from-a-coarse-level",
        says: "A mesh's volume is made from a coarse level: of the levels marked for the model \
               detail (every level when none is), the count less one, or less two at the highest \
               model detail when drawn at half detail or more, taken as a level of the mesh \
               itself.",
        since: RETAIL,
        evidence: Evidence::Private("AC2-EVID-VOLUME-LEVEL"),
        station: "auberean-draw::cpu::volume::a_volume_is_made_from_a_coarse_level",
        tier: Tier::Cpu,
    },
    behaviour! {
        id: "draw.volume.a-volume-is-made-only-whole-and-in-room",
        says: "A volume is made only whole and in the room the frame has left (of a hundred \
               volumes, forty thousand vertices and sixty thousand indices): none for a mesh of \
               three triangles or fewer, none whose vertices or indices would pass the room left.",
        since: RETAIL,
        evidence: Evidence::Private("AC2-EVID-VOLUME-ROOM"),
        station: "auberean-draw::cpu::volume::a_volume_is_made_only_whole_and_in_room",
        tier: Tier::Cpu,
    },
    behaviour! {
        id: "draw.volume.an-objects-drawing-properties-say-how-it-is-drawn",
        says: "An object's drawing properties say how it is drawn: casting a volume of shadow, \
               casting a shadow on the landscape and being drawn with the objects seen through \
               each set or clear their bit of its graphics state, not being drawn in the game says \
               whether it is, and each is kept among its own properties.",
        since: RETAIL,
        evidence: Evidence::Private("AC2-EVID-RENDER-PROPERTIES"),
        station: "auberean-physics::cpu::solid::an_objects_drawing_properties_say_how_it_is_drawn",
        tier: Tier::Cpu,
    },
    behaviour! {
        id: "draw.volume.the-lights-an-object-keeps-throw-its-shadow",
        says: "The lights an object keeps throw its shadow, the first deciding how: a directional \
               light along its direction; a point light by the sum over the lights on the object's \
               side of each one's way from itself to the object leaning a fifth down, by its \
               weight (a sky light the sun's way), made a unit, straight down when they sum to \
               nothing; with no light, the sun's way.",
        since: RETAIL,
        evidence: Evidence::Private("AC2-EVID-VOLUME-DIRECTION"),
        station: "auberean-draw::cpu::volume::the_lights_an_object_keeps_throw_its_shadow",
        tier: Tier::Cpu,
    },
    behaviour! {
        id: "draw.volume.triangles-meet-at-edges-run-the-other-way",
        says: "A mesh level's triangles meet at their edges: vertices within two ten-thousandths \
               of a meter of each other on every axis are one, and an edge a later triangle runs \
               the other way joins the first such edge still unmet.",
        since: RETAIL,
        evidence: Evidence::Private("AC2-EVID-VOLUME-CONNECTIVITY"),
        station: "auberean-draw::cpu::volume::triangles_meet_at_edges_run_the_other_way",
        tier: Tier::Cpu,
    },
    behaviour! {
        id: "draw.world.a-frill-takes-its-color-from-the-shadow-about-it",
        says: "A frill's vertices take, in place of their own color, the sun's shadow map about \
               the frill's spot on the ground: the texels there and two and four away each way, \
               five by five, averaged over those within the map (white when none is), opaque. \
               Only a material that reads its vertices' color shows it, and none of the frills' \
               materials in the data does.",
        since: RETAIL,
        evidence: Evidence::Private("AC2-EVID-DRAW-FRILL-SHADE"),
        station: "auberean-draw::dat::landscape::every_frill_is_drawn_with_its_spot_for_the_shadow_about_it",
        tier: Tier::Dat,
    },
    behaviour! {
        id: "draw.world.the-far-landscape-and-the-frills-reach-as-far-as-set",
        says: "The far landscape reaches as many blocks about the viewer's as its radius, the detailed blocks drawn at every radius; frills are drawn only in the land cells within their radius of the viewer's, none when turned off, fading out over the last eight meters of their reach.",
        since: RETAIL,
        evidence: Evidence::Private("AC2-EVID-DRAW-LANDSCAPE-REACH"),
        station: "auberean-draw::dat::landscape::the_far_landscape_and_the_frills_reach_as_far_as_set",
        tier: Tier::Dat,
    },
    behaviour! {
        id: "draw.world.the-light-is-that-of-the-block-the-viewer-stands-in",
        says: "The fog blocks blended into the light are those of the block the viewer stands in, even past the edge of the block its place is given in; with no sky the light travels up and reaches no ground.",
        since: RETAIL,
        evidence: Evidence::Private("AC2-EVID-DRAW-LIGHT-BLOCK"),
        station: "auberean-draw::dat::landscape::the_light_is_that_of_the_block_the_viewer_stands_in",
        tier: Tier::Dat,
    },
    behaviour! {
        id: "draw.world.the-sun-throws-a-lens-flare",
        says: "A sky object with flare points (the sun) throws a lens flare after the late sky: \
               placed 1000 m out along its direction from the frame's origin, each point a \
               picture on the line from it through the screen's middle at minus its axis, square \
               on the display, added on; as opaque as the square of one less its distance from \
               the middle over 1.42 (at least a fifth) times its fade, which rises or falls at its \
               speed by whether a marker at the object passed the depth test the frame before; \
               with the light coronas preference.",
        since: RETAIL,
        evidence: Evidence::Private("AC2-EVID-DRAW-LENS-FLARE"),
        station: "auberean-draw::dat::landscape::the_sun_throws_a_lens_flare",
        tier: Tier::Dat,
    },
];
