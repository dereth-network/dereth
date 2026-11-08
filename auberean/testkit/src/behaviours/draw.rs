//! Drawing: how stored meshes are read for drawing and how the landscape's layers are laid on.
//!
//! A row claims what the September 2013 client does when it draws a mesh or a terrain layer.

use super::{Behaviour, Evidence, Tier, RETAIL, THIS_CLIENT, TOOLING};

/// This subject's rows, in id order.
pub static ROWS: &[Behaviour] = &[
    behaviour! {
        id: "draw.bodies.the-players-meshes-keep-their-detail-farther-out",
        says: "The player's own meshes are drawn at half as much detail again as their distance gives, up to full: they drop a level later than another figure's. What the player holds does not share this.",
        since: RETAIL,
        evidence: Evidence::Private("AC2-EVID-PLAYER-MESH-DETAIL"),
        station: "auberean-draw::dat::bodies::the_players_meshes_keep_their_detail_farther_out",
        tier: Tier::Dat,
    },
    behaviour! {
        id: "draw.cells.a-cells-appearance-dresses-its-mesh-by-material-kind",
        says: "An interior cell that names an appearance key is drawn with that key's change set of the cells' appearance table: its mesh's materials are swapped by material kind, so one mesh serves a grassy hall and a sandy canyon.",
        since: RETAIL,
        evidence: Evidence::Private("AC2-EVID-DRAW-CELL-APPEARANCE"),
        station: "auberean-draw::dat::cells::a_cells_appearance_dresses_its_mesh_by_material_kind",
        tier: Tier::Dat,
    },
    behaviour! {
        id: "draw.decal.a-body-inside-lays-its-disc-on-the-floor",
        says: "Inside, the simple setting's disc is laid on the room's floor under the body and \
               drawn over it: the cells are drawn whole, every layer of them, before the decals.",
        since: RETAIL,
        evidence: Evidence::Private("AC2-EVID-DECAL-DUNGEON-DRAWN"),
        station: "auberean-gpu::gpu::shadows::a_body_inside_lays_its_disc_on_the_floor",
        tier: Tier::Gpu,
    },
    behaviour! {
        id: "draw.decal.a-decal-fades-in-and-out-over-its-life",
        says: "A decal lives out its lifespan, fading in over its first tenth (a quarter second at \
               most) and out over its last fifth (a second at most); one drawn once shows whole at \
               once and is gone once drawn; its vertices take its color with its alpha times how \
               much shows.",
        since: RETAIL,
        evidence: Evidence::Private("AC2-EVID-DECAL-LIFE"),
        station: "auberean-draw::cpu::effects::decal::a_decal_fades_in_and_out_over_its_life",
        tier: Tier::Cpu,
    },
    behaviour! {
        id: "draw.decal.a-decal-inside-is-laid-on-the-cells-ground-about-it",
        says: "A decal inside is laid on the cells' ground about it: the triangles of its cell's \
               mesh (the most detailed level) whose box meets the cube about its point out to the \
               box's largest side, and once, those of each room through a doorway of its cell whose \
               shape meets the cube, cut to the decal's box.",
        since: RETAIL,
        evidence: Evidence::Private("AC2-EVID-DECAL-DUNGEON"),
        station: "auberean-draw::dat::inside::a_decal_inside_is_laid_on_the_cells_ground_about_it",
        tier: Tier::Dat,
    },
    behaviour! {
        id: "draw.decal.a-decal-is-the-ground-cut-to-its-box",
        says: "A decal is the ground cut to its box (its width along its direction, its depth \
               across, its height up), each piece fanned from its first corner, its picture \
               coordinates its place across the box with the middle at one half, lifted fifteen \
               millimeters.",
        since: RETAIL,
        evidence: Evidence::Private("AC2-EVID-DECAL-CONSTRUCT"),
        station: "auberean-draw::cpu::effects::decal::a_decal_is_the_ground_cut_to_its_box",
        tier: Tier::Cpu,
    },
    behaviour! {
        id: "draw.decal.a-landscape-decal-takes-the-nine-squares-about-it",
        says: "A decal on the landscape takes the ground of the nine squares about it, its own and \
               those a square's side away along each axis and diagonal.",
        since: RETAIL,
        evidence: Evidence::Private("AC2-EVID-DECAL-LANDSCAPE"),
        station: "auberean-draw::cpu::effects::decal::a_landscape_decal_takes_the_nine_squares_about_it",
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
        station: "auberean-draw::cpu::effects::decal::a_point_light_lays_a_pool_of_its_light_below_it",
        tier: Tier::Cpu,
    },
    behaviour! {
        id: "draw.decal.a-polygon-is-cut-to-a-planes-side",
        says: "A decal's polygon cut by a plane keeps the side where the plane's measure is not \
               negative, a point where an edge crosses put between the kept and the dropped point \
               with its normal taken along the edge; nothing is left when every point is dropped.",
        since: RETAIL,
        evidence: Evidence::Private("AC2-EVID-DECAL-CLIP"),
        station: "auberean-draw::cpu::effects::decal::a_polygon_is_cut_to_a_planes_side",
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
        station: "auberean-draw::cpu::effects::decal::decals_are_drawn_by_material_within_a_groups_room",
        tier: Tier::Cpu,
    },
    behaviour! {
        id: "draw.decal.light-pools-show-from-a-room-that-looks-out",
        says: "The lights standing outdoors lay their pools on the land whenever the land is drawn: with the camera outdoors or in a room that looks out, with light effects on.",
        since: RETAIL,
        evidence: Evidence::Private("AC2-EVID-LIGHT-POOLS-LOOKING-OUT"),
        station: "auberean-scene::cpu::view::light_pools_show_from_a_room_that_looks_out",
        tier: Tier::Cpu,
    },
    behaviour! {
        id: "draw.effects.a-carried-system-stands-at-the-holding-location-its-entry-names",
        says: "A particle system an effect's entry keeps going at a holding location stands at that location, on that location's bone.",
        since: RETAIL,
        evidence: Evidence::Private("AC2-EVID-FX-CARRIED-LOCATION"),
        station: "auberean-draw::dat::effects::a_carried_system_stands_at_the_holding_location_its_entry_names",
        tier: Tier::Dat,
    },
    behaviour! {
        id: "draw.effects.a-carried-system-stands-on-the-bone-its-entry-names",
        says: "A particle system an effect's entry keeps going on a bone stands on that bone and goes with it; a figure without that bone gets no system.",
        since: RETAIL,
        evidence: Evidence::Private("AC2-EVID-FX-CARRIED-BONE"),
        station: "auberean-draw::dat::effects::a_carried_system_stands_on_the_bone_its_entry_names",
        tier: Tier::Dat,
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
        station: "auberean-draw::cpu::objects::lights::a_placed_light_fades_in_and_out",
        tier: Tier::Cpu,
    },
    behaviour! {
        id: "draw.effects.a-placed-light-stands-in-its-own-cells-block",
        says: "A light an effect placed is taken by the frame's lights as a point light in its                object's cell at its place in that cell's own block, its range as it stands and its                intensity and specular color times how much of it shows; how far and how strongly it                may light anything come from its waveforms' bounds.",
        since: RETAIL,
        evidence: Evidence::Private("AC2-EVID-DRAW-CARRIED-LIGHT-STANDING"),
        station: "auberean-draw::cpu::objects::lights::a_placed_light_stands_in_its_own_cells_block",
        tier: Tier::Cpu,
    },
    behaviour! {
        id: "draw.effects.a-placed-objects-systems-stand-on-its-bones-at-rest",
        says: "An object placed with the world carries a system its effect sets on a bone there, \
               the bone as its look's body first stands; a bone it lacks gives no system.",
        since: RETAIL,
        evidence: Evidence::Private("AC2-EVID-DRAW-PLACED-BONES"),
        station: "auberean-draw::dat::effects::a_placed_objects_systems_stand_on_its_bones_at_rest",
        tier: Tier::Dat,
    },
    behaviour! {
        id: "draw.effects.a-slider-with-no-intensity-goes-to-full",
        says: "An effect entry that moves a slider and stores no intensity moves it to full.",
        since: RETAIL,
        evidence: Evidence::Private("AC2-EVID-FX-SLIDER-DEFAULT"),
        station: "auberean-draw::cpu::effects::a_slider_entry_with_no_intensity_goes_to_full",
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
        id: "draw.effects.an-effect-lasts-while-its-sound-or-system-runs-until-stopped",
        says: "An effect lasts until it is stopped when its sound loops or its particle system runs until stopped; otherwise its entries' other times decide.",
        since: RETAIL,
        evidence: Evidence::Private("AC2-EVID-FX-DURATION"),
        station: "auberean-draw::dat::effects::an_effect_lasts_until_stopped_when_its_sound_or_system_does",
        tier: Tier::Dat,
    },
    behaviour! {
        id: "draw.effects.an-effect-lays-the-decal-its-entry-names",
        says: "An effect's entry with a decal lays it, of its material, life and color, sized and turned by its waveforms at that moment, while the decal effects preference is on.",
        since: RETAIL,
        evidence: Evidence::Private("AC2-EVID-FX-DECAL"),
        station: "auberean-draw::dat::effects::an_effect_lays_the_decal_its_entry_names",
        tier: Tier::Dat,
    },
    behaviour! {
        id: "draw.effects.an-effect-moves-its-sliders-over-its-ramp",
        says: "An effect entry moves its slider to its intensity over its ramp time, and an \
               entry with a color moves the object's four color sliders to that color's \
               parts.",
        since: RETAIL,
        evidence: Evidence::Private("AC2-EVID-FX-SLIDERS"),
        station: "auberean-draw::cpu::effects::particles::an_effect_moves_its_sliders_over_its_ramp",
        tier: Tier::Cpu,
    },
    behaviour! {
        id: "draw.effects.an-effect-running-out-restores-only-a-color-that-lasts",
        says: "An effect that runs out of itself leaves the object's color, unless an entry's color lasts a time.",
        since: RETAIL,
        evidence: Evidence::Private("AC2-EVID-FX-RUN-OUT-COLOR"),
        station: "auberean-draw::dat::effects::an_effect_running_out_restores_only_a_color_that_lasts",
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
        id: "draw.effects.an-effects-color-goes-back-as-it-ends",
        says: "An effect's entry that says so puts its object's color back to white in a tenth of a second as the effect stops, playing or not; a color that lasts holds its effect for as long.",
        since: RETAIL,
        evidence: Evidence::Private("AC2-EVID-FX-RESTORE-COLOR"),
        station: "auberean-draw::dat::effects::an_effects_color_goes_back_as_it_ends",
        tier: Tier::Dat,
    },
    behaviour! {
        id: "draw.effects.an-effects-entries-play-as-the-players-preferences-ask",
        says: "An effect's entries keyed to how much gore, the color of blood or steps raising particles play only as the player's preferences ask; a channel no preference holds says nothing.",
        since: RETAIL,
        evidence: Evidence::Private("AC2-EVID-FX-CHANNELS"),
        station: "auberean-draw::dat::effects::an_effects_entries_play_as_the_players_preferences_ask",
        tier: Tier::Dat,
    },
    behaviour! {
        id: "draw.effects.an-effects-kept-sounds-follow-its-strength",
        says: "An effect asked again while it plays moves the looping sounds it keeps to its new strength.",
        since: RETAIL,
        evidence: Evidence::Private("AC2-EVID-FX-KEPT-SOUND-VOLUME"),
        station: "auberean-draw::dat::effects::an_effects_kept_sounds_follow_its_strength",
        tier: Tier::Dat,
    },
    behaviour! {
        id: "draw.effects.an-object-going-stops-its-effects",
        says: "An object going stops every effect it plays, startup or asked for, with the systems \n               they keep going and their sounds.",
        since: RETAIL,
        evidence: Evidence::Private("AC2-EVID-FX-DESTROY"),
        station: "auberean-draw::dat::effects::an_object_going_stops_its_effects",
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
        id: "draw.effects.carried-lights-light-what-stands-near-them-outdoors",
        says: "Outdoors the lights objects carry light the objects and bodies near them beside the                sun: the town's torches at night leave the frame brighter in more places than darker, a normal map they reach being lit from them in place of the sun, and a human standing by one takes it and throws its shadow from it.",
        since: RETAIL,
        evidence: Evidence::Private("AC2-EVID-DRAW-CARRIED-LIGHTS-OUTDOORS"),
        station: "auberean-gpu::gpu::shadows::carried_lights_light_what_stands_near_them_outdoors",
        tier: Tier::Gpu,
    },
    behaviour! {
        id: "draw.effects.frills-take-the-carried-lights-that-reach-them",
        says: "Outdoors the frills take the lights objects carry that reach them beside the sun, as \
               the other draws standing outdoors do; those no carried light reaches keep the sun \
               alone.",
        since: RETAIL,
        evidence: Evidence::Private("AC2-EVID-DRAW-FRILL-CARRIED-LIGHTS"),
        station: "auberean-draw::dat::outdoors::frills_take_the_carried_lights_that_reach_them",
        tier: Tier::Dat,
    },
    behaviour! {
        id: "draw.effects.lit-particles-outdoors-take-the-carried-lights-before-the-sun",
        says: "Outdoors a lit particle system is lit by the lights objects carry that reach it, \
               strongest first, and then the sun; with none in reach, by the sun alone.",
        since: RETAIL,
        evidence: Evidence::Private("AC2-EVID-DRAW-PARTICLE-CARRIED-LIGHTS"),
        station: "auberean-draw::cpu::objects::lights::lit_particles_outdoors_take_the_carried_lights_before_the_sun",
        tier: Tier::Cpu,
    },
    behaviour! {
        id: "draw.effects.no-lights-while-the-light-source-effects-preference-is-off",
        says: "With the light source effects preference off the effects place no lights at all, so                the objects made then light nothing and lay no pool; turned on, it gives lights only                to what is made after.",
        since: RETAIL,
        evidence: Evidence::Private("AC2-EVID-EFFECT-LIGHTS-PREFERENCE"),
        station: "auberean-draw::dat::effects::no_lights_while_the_light_source_effects_preference_is_off",
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
        id: "draw.flare.a-flare-asks-of-one-pixel-whether-its-object-is-seen",
        says: "A sky object is seen when the one pixel at its place on the screen is not drawn over, so a thin edge across that pixel hides its flare and one beside it does not.",
        since: RETAIL,
        evidence: Evidence::Private("AC2-EVID-FLARE-ONE-PIXEL"),
        station: "auberean-draw::cpu::landscape::shadows::a_flare_asks_of_one_pixel_whether_its_object_is_seen",
        tier: Tier::Cpu,
    },
    behaviour! {
        id: "draw.frame.each-step-draws-its-objects-farthest-first",
        says: "Each step of a frame's objects draws them farthest from the camera first, each \
               object's draws together in their order, every layer of an object with the object; \
               the landscape's frills are drawn in a step of their own after the water.",
        since: RETAIL,
        evidence: Evidence::Private("AC2-EVID-DRAW-OBJECT-ORDER"),
        station: "auberean-draw::dat::volumes::each_step_draws_its_objects_farthest_first_and_the_frills_after_the_water",
        tier: Tier::Dat,
    },
    behaviour! {
        id: "draw.frame.the-water-is-drawn-before-what-is-seen-through",
        says: "A frame of the world draws its objects not seen through, then the water, then the \
               frills, the objects seen through and the particle systems last: a flame over a lake \
               adds itself onto the water behind it rather than being covered by it.",
        since: RETAIL,
        evidence: Evidence::Private("AC2-EVID-DRAW-FRAME-ORDER"),
        station: "auberean-draw::dat::effects::the_arcane_fires_flames_are_drawn_after_the_water",
        tier: Tier::Dat,
    },
    behaviour! {
        id: "draw.inside.a-body-is-drawn-by-the-rooms-it-touches",
        says: "Inside, a body is drawn by the rooms it touches: when one of them is reached and its \n               box, doubled, meets a view that room was entered by.",
        since: RETAIL,
        evidence: Evidence::Private("AC2-EVID-DRAW-CULL-BODY"),
        station: "auberean-draw::cpu::objects::cull::a_body_is_drawn_by_the_rooms_it_touches",
        tier: Tier::Cpu,
    },
    behaviour! {
        id: "draw.inside.a-camera-inside-looks-into-rooms-by-the-doorways-between-them",
        says: "Out through a doorway of its own, a camera inside looks into another building's rooms by \n               the doorways between them, not those out; a room never culled shows whenever its box \n               does.",
        since: RETAIL,
        evidence: Evidence::Private("AC2-EVID-DRAW-CULL-OUT-IN"),
        station: "auberean-draw::cpu::objects::cull::a_camera_inside_looks_into_rooms_by_the_doorways_between_them",
        tier: Tier::Cpu,
    },
    behaviour! {
        id: "draw.inside.a-camera-on-the-land-looks-into-rooms-by-their-doorways-out",
        says: "A camera on the land looks into a room touching the outside by its doorway out: the walk \n               from that room reaches the rooms past it, and it and the rooms it sees are drawn.",
        since: RETAIL,
        evidence: Evidence::Private("AC2-EVID-DRAW-CULL-LOOK-IN"),
        station: "auberean-draw::cpu::objects::cull::a_camera_on_the_land_looks_into_rooms_by_their_doorways_out",
        tier: Tier::Cpu,
    },
    behaviour! {
        id: "draw.inside.a-carried-light-lights-what-stands-near-it-and-throws-shadows-away",
        says: "Inside, a light an object carries joins the lights the rooms and objects within its                reach keep (sorted by their weight where each stands), shining its color times its                intensity, and they go back to their own once it is gone; a body beside it takes it                too and throws its shadow away from it.",
        since: RETAIL,
        evidence: Evidence::Private("AC2-EVID-DRAW-CARRIED-LIGHTS"),
        station: "auberean-draw::dat::inside::a_carried_light_lights_what_stands_near_it_and_throws_shadows_away",
        tier: Tier::Dat,
    },
    behaviour! {
        id: "draw.inside.a-caves-rooms-show-through-its-mouth",
        says: "On the land before a cave, its rooms are drawn while its mouth is in view and not with \n               the camera's back to it; everything standing on the land stays drawn.",
        since: RETAIL,
        evidence: Evidence::Private("AC2-EVID-DRAW-CULL-MOUTH"),
        station: "auberean-draw::dat::cull::the_rooms_of_a_cave_show_through_its_mouth",
        tier: Tier::Dat,
    },
    behaviour! {
        id: "draw.inside.a-cells-own-mesh-keeps-the-sky-lights-alone",
        says: "A cell's own mesh takes only the sky lights of the lights it keeps, from inside and from \n               the land; what stands in its rooms takes the others too.",
        since: RETAIL,
        evidence: Evidence::Private("AC2-EVID-DRAW-CELL-SKY-LIGHTS"),
        station: "auberean-draw::dat::outdoors::a_cells_own_mesh_keeps_the_sky_lights_alone",
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
        station: "auberean-draw::cpu::objects::cull::a_doorway_narrows_the_view_to_what_shows_through_it",
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
        id: "draw.inside.a-normal-mapped-layers-second-light-shines-in-the-firsts-color",
        says: "A normal-mapped layer lights its normal map from its first point light, outdoors as well as inside, and the sun then joins the others in its second color. Each other point light's share shines in the color of the light before it: the program counts its diffuse colors only where they are used, and the first light uses none.",
        since: RETAIL,
        evidence: Evidence::Private("AC2-EVID-DRAW-BUMP-LIGHT-COLORS"),
        station: "auberean-gpu::gpu::inside::a_normal_mapped_layers_second_light_shines_in_the_firsts_color",
        tier: Tier::Gpu,
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
        station: "auberean-draw::cpu::objects::cull::a_room_past_the_fog_is_left_out_by_its_own_distance",
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
        station: "auberean-draw::cpu::objects::cull::never_culled_rooms_are_taken_with_their_group",
        tier: Tier::Cpu,
    },
    behaviour! {
        id: "draw.inside.no-sky-drawn-no-veil-over-it",
        says: "The fog's veil over the sky is drawn with the sky: rooms with no view out and no cell open to the sky show none of it.",
        since: RETAIL,
        evidence: Evidence::Private("AC2-EVID-VEIL-WITH-SKY"),
        station: "auberean-gpu::gpu::inside::rooms_with_no_view_out_show_no_veil_of_the_fog_outside",
        tier: Tier::Gpu,
    },
    behaviour! {
        id: "draw.inside.rooms-seen-from-outside-are-lit-as-rooms",
        says: "The rooms a block lists as seen from outside, drawn in a frame outdoors, are lit as \
               rooms and not by the sun: each drawn once, its mesh by the light its vertices carry                and the lights it keeps where it stands; a light carried inside reaches them, one                carried on the land outside does not.",
        since: RETAIL,
        evidence: Evidence::Private("AC2-EVID-DRAW-ROOMS-SEEN-FROM-OUTSIDE"),
        station: "auberean-draw::dat::outdoors::rooms_seen_from_outside_are_lit_as_rooms",
        tier: Tier::Dat,
    },
    behaviour! {
        id: "draw.inside.the-cameras-view-is-looser-than-its-picture",
        says: "The view the walk through the doorways starts from reaches across half the field of view \
               times the picture's shape, and up and down to ninety degrees less half the field of \
               view, with a near plane; looser than the picture.",
        since: RETAIL,
        evidence: Evidence::Private("AC2-EVID-DRAW-CULL-CAMERA"),
        station: "auberean-draw::cpu::objects::cull::the_cameras_view_is_looser_than_its_picture",
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
        says: "What a frame draws is chosen by the cell the camera stands in, not the player's: \
               the cell the camera's last line of sight ended in, a room beyond a doorway the line \
               went through, the room it started in when it left that by its open top, or the land \
               cell under the camera outside.",
        since: RETAIL,
        evidence: Evidence::Private("AC2-EVID-DRAW-CAMERA-CELL"),
        station: "auberean-physics::dat::camera::the_camera_stands_in_the_cell_its_last_line_ended_in",
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
        station: "auberean-draw::cpu::objects::cull::the_walk_reaches_the_rooms_the_doorways_show",
        tier: Tier::Cpu,
    },
    behaviour! {
        id: "draw.inside.what-stands-in-a-cell-seen-from-the-land-draws-after-the-land",
        says: "With the camera on the land, each step of the world's objects draws what stands on the land first, then what stands in the cells it looks into, each list farthest first.",
        since: RETAIL,
        evidence: Evidence::Private("AC2-EVID-LOOKED-IN-ORDER"),
        station: "auberean-draw::cpu::objects::degrade::what_stands_in_a_cell_seen_from_the_land_draws_after_what_stands_on_it",
        tier: Tier::Cpu,
    },
    behaviour! {
        id: "draw.material.a-device-draws-the-layers-of-its-detail-and-fog",
        says: "A device draws a material's layers made for its material detail and its fog: a \
               layer whose must-set detail or fog flags it lacks, or whose must-clear ones it has, \
               is not drawn; one that asks for none is drawn at every detail.",
        since: RETAIL,
        evidence: Evidence::Private("AC2-EVID-DRAW-LAYER-DETAIL"),
        station: "auberean-draw::cpu::materials::material::a_device_draws_the_layers_of_its_detail_and_fog",
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
        station: "auberean-draw::cpu::materials::material::a_layers_programs_cover_its_light_counts",
        tier: Tier::Cpu,
    },
    behaviour! {
        id: "draw.material.a-material-draws-the-layer-its-detail-asks-for",
        says: "Most material instances hold one layer for each material detail (low, medium, \
               high), of which the device draws the one its detail asks for; every instance draws \
               a layer at every detail.",
        since: RETAIL,
        evidence: Evidence::Private("AC2-EVID-DRAW-MATERIAL-DETAIL"),
        station: "auberean-draw::dat::materials::a_material_draws_the_layer_its_detail_asks_for",
        tier: Tier::Dat,
    },
    behaviour! {
        id: "draw.material.a-modifier-sets-a-layers-reference-texture-and-switches",
        says: "A modifier sets a layer's alpha reference (its waveform whole), a stage's texture \
               by its id and its addressing (zero clamped, else wrapped), and the layer's culling \
               (zero one-sided, else two-sided), depth test (zero always passing, else less or \
               equal) and depth writing (on for anything but zero).",
        since: RETAIL,
        evidence: Evidence::Private("AC2-EVID-DRAW-MODIFIER-FIELDS"),
        station: "auberean-draw::cpu::materials::material::a_modifier_sets_a_layers_reference_texture_and_switches",
        tier: Tier::Cpu,
    },
    behaviour! {
        id: "draw.material.a-modifier-sets-power-and-coordinate-scale",
        says: "A material modifier's specular power and its scale of a stage's texture \
               coordinates reach the layer through the material's parameter of the same name.",
        since: RETAIL,
        evidence: Evidence::Private("AC2-EVID-DRAW-MATERIAL-FIELDS"),
        station: "auberean-draw::cpu::materials::material::a_modifier_sets_power_and_coordinate_scale",
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
        station: "auberean-draw::cpu::materials::material::a_pixel_program_is_read_from_its_instructions",
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
        id: "draw.material.a-surface-frame-that-is-not-a-number-is-lit-as-facing-the-light",
        says: "Where a mesh stores a surface frame that is not a number (203 meshes do, where their \
               pictures' coordinates run flat, as at the crystals' feet), the direction toward the \
               light its normal-mapped layer carries is not a number either, and the device reads \
               each such part as 1: the packed direction is white, and the normal map there is lit \
               as if it faced the light, so the crystals stay bright down to the ground.",
        since: RETAIL,
        evidence: Evidence::Private("AC2-EVID-DRAW-UNDEFINED-SURFACE-FRAME"),
        station: "auberean-gpu::gpu::materials::a_surface_frame_that_is_not_a_number_is_lit_as_facing_the_light",
        tier: Tier::Gpu,
    },
    behaviour! {
        id: "draw.material.a-turn-and-a-stored-matrix-move-coordinates-as-the-clients-do",
        says: "A layer's turn moves a stage's coordinates by its waveform in degrees, \
               counterclockwise as the coordinates run, after what came before it; a stored \
               matrix moves them by its first two columns, its fourth row the slide.",
        since: RETAIL,
        evidence: Evidence::Private("AC2-EVID-DRAW-UV-TURN"),
        station: "auberean-draw::cpu::materials::material::a_turn_and_a_stored_matrix_move_coordinates_as_the_clients_do",
        tier: Tier::Cpu,
    },
    behaviour! {
        id: "draw.material.an-animation-field-sets-its-first-or-second-waveform",
        says: "A material parameter's coordinate-animation field sets the animation in that place of the layer's list by its place in the animation: the first waveform (a slide's u, a turn, a stretch's u) or the second (a slide's or a stretch's v).",
        since: RETAIL,
        evidence: Evidence::Private("AC2-EVID-DRAW-MATERIAL-ANIMATION-FIELDS"),
        station: "auberean-draw::cpu::materials::material::an_animation_field_sets_its_first_or_second_waveform",
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
        station: "auberean-draw::cpu::materials::material::coordinate_animations_apply_in_order",
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
        station: "auberean-draw::cpu::materials::material::specular_is_off_at_zero_power",
        tier: Tier::Cpu,
    },
    behaviour! {
        id: "draw.mesh.a-body-degrades-with-its-distance-from-the-camera",
        says: "A body's detail, materials and opacity fall with its distance from the camera \n               as a placed object's do, its setup's distances scaled; in a scene of the \n               interface it never degrades.",
        since: RETAIL,
        evidence: Evidence::Private("AC2-EVID-DRAW-BODY-DEGRADE"),
        station: "auberean-draw::dat::volumes::a_body_degrades_with_its_distance_from_the_camera",
        tier: Tier::Dat,
    },
    behaviour! {
        id: "draw.mesh.a-bodys-reach-is-worked-out-each-frame",
        says: "How far away a body is drawn is worked out fresh each frame: one that a low scale \
               left undrawn is drawn again once the scale comes back, wherever the camera stands.",
        since: THIS_CLIENT,
        divergence: "CD2-a-bodys-reach-is-worked-out-each-frame",
        evidence: Evidence::Private("AC2-EVID-BODY-REACH-FRESH"),
        station: "auberean-draw::cpu::objects::a_bodys_reach_is_worked_out_each_frame",
        tier: Tier::Cpu,
    },
    behaviour! {
        id: "draw.mesh.a-faded-draw-stops-marking-depth-and-testing-its-alpha",
        says: "A draw not wholly opaque of a layer that tests or blends by alpha marks no \n               depth and passes every texel with some alpha: a blending layer under 0.8, any \n               such layer past 40 m.",
        since: RETAIL,
        evidence: Evidence::Private("AC2-EVID-DRAW-FADED-STATE"),
        station: "auberean-draw::cpu::objects::degrade::a_faded_draw_stops_marking_depth_and_testing_its_alpha",
        tier: Tier::Cpu,
    },
    behaviour! {
        id: "draw.mesh.a-far-object-draws-its-materials-at-a-lower-detail",
        says: "A placed object draws its materials a step lower than the material detail asks \
               past three times its high-detail distance, and at the lowest past three times its \
               medium-detail one; a lower detail is never raised.",
        since: RETAIL,
        evidence: Evidence::Private("AC2-EVID-DRAW-DEGRADE-MATERIALS"),
        station: "auberean-draw::cpu::objects::degrade::a_far_object_draws_its_materials_at_a_lower_detail",
        tier: Tier::Cpu,
    },
    behaviour! {
        id: "draw.mesh.a-lost-device-is-noticed-and-does-not-stop-the-game",
        says: "A device that goes away is noticed, and drawing on it asks for nothing it can't give; another device draws.",
        since: TOOLING,
        evidence: Evidence::Private("AC2-EVID-DEVICE-LOST"),
        station: "auberean-gpu::gpu::lost::a_lost_device_is_noticed_and_does_not_stop_the_game",
        tier: Tier::Gpu,
    },
    behaviour! {
        id: "draw.mesh.a-screenshot-is-drawn-at-full-detail",
        says: "While a screenshot is taken nothing degrades or goes with its distance from the \
               camera: far objects come out at full detail, and the next frame is as before.",
        since: RETAIL,
        evidence: Evidence::Private("AC2-EVID-SHOT-FULL-DETAIL"),
        station: "auberean-draw::dat::outdoors::a_screenshot_is_drawn_at_full_detail",
        tier: Tier::Dat,
    },
    behaviour! {
        id: "draw.mesh.a-vertex-format-lays-its-elements-out-in-order",
        says: "A stored vertex holds its position, blend weights, normal, point size, diffuse and \
               specular colors, tangent-space vectors and coordinate pairs in that order, sized \
               by its format word.",
        since: RETAIL,
        evidence: Evidence::Private("AC2-EVID-DRAW-VERTEX-LAYOUT"),
        station: "auberean-draw::cpu::objects::layout::a_vertex_format_lays_its_elements_out_in_order",
        tier: Tier::Cpu,
    },
    behaviour! {
        id: "draw.mesh.an-object-fades-out-at-the-edge-of-its-visible-distance",
        says: "An object fades out over the last 7.5% of its visible distance, and is not drawn \
               once nothing of it is left.",
        since: RETAIL,
        evidence: Evidence::Private("AC2-EVID-DRAW-DEGRADE-FADE"),
        station: "auberean-draw::cpu::objects::degrade::an_object_fades_out_at_the_edge_of_its_visible_distance",
        tier: Tier::Cpu,
    },
    behaviour! {
        id: "draw.mesh.an-objects-detail-follows-the-camera-every-frame",
        says: "A placed object's level of detail and materials follow its distance from the camera every frame, not \
               only when the frame is built again.",
        since: RETAIL,
        evidence: Evidence::Private("AC2-EVID-DETAIL-EVERY-FRAME"),
        station: "auberean-draw::dat::landscape::an_objects_detail_follows_the_camera_every_frame",
        tier: Tier::Dat,
    },
    behaviour! {
        id: "draw.mesh.an-objects-opacity-follows-the-camera-and-its-landblock",
        says: "An object's opacity is worked out each frame from the camera, times the \n               opacity of the landblock it stands on while that fades in.",
        since: RETAIL,
        evidence: Evidence::Private("AC2-EVID-DRAW-FADE-EACH-FRAME"),
        station: "auberean-draw::cpu::objects::degrade::an_objects_opacity_follows_the_camera_and_its_landblock",
        tier: Tier::Cpu,
    },
    behaviour! {
        id: "draw.mesh.detail-falls-from-near-to-far",
        says: "An object is drawn at full detail inside its near distance, at half at its ideal \
               distance, at the least past its far distance, and not at all beyond its visible \
               distance.",
        since: RETAIL,
        evidence: Evidence::Private("AC2-EVID-DRAW-DETAIL"),
        station: "auberean-draw::cpu::objects::layout::detail_falls_from_near_to_far",
        tier: Tier::Cpu,
    },
    behaviour! {
        id: "draw.mesh.every-draw-of-a-frame-shows-its-own-uniforms",
        says: "However many draws a frame has, each shows its own uniforms, across the device's \
               buffers of them.",
        since: TOOLING,
        evidence: Evidence::Private("AC2-EVID-UNIFORMS-EACH-DRAW"),
        station: "auberean-gpu::gpu::uniforms::every_draw_of_a_frame_shows_its_own_uniforms",
        tier: Tier::Gpu,
    },
    behaviour! {
        id: "draw.mesh.the-adaptive-degrade-draws-moving-bodies-in-as-frames-slow",
        says: "With the adaptive degrade on, a body both mobile and dynamic has its distances \n               scaled by the frame rate, whole at 41 frames a second (bias 0), a tenth at \n               none.",
        since: RETAIL,
        evidence: Evidence::Private("AC2-EVID-DRAW-ADAPTIVE-DEGRADE"),
        station: "auberean-draw::cpu::objects::degrade::the_adaptive_degrade_draws_moving_bodies_in_as_frames_slow",
        tier: Tier::Cpu,
    },
    behaviour! {
        id: "draw.mesh.the-device-draws-a-faded-object-through",
        says: "The device blends a faded object over what is behind it, marks no depth once \n               faded enough, draws an opaque layer whole, and draws nothing of an object \n               faded out.",
        since: RETAIL,
        evidence: Evidence::Private("AC2-EVID-DRAW-FADED-DEVICE"),
        station: "auberean-gpu::gpu::fade::the_device_draws_a_faded_object_through",
        tier: Tier::Gpu,
    },
    behaviour! {
        id: "draw.mesh.the-frame-rate-is-the-mean-of-the-last-forty-frames",
        says: "The frame rate the degrade reads is the mean of the last forty frames' times.",
        since: RETAIL,
        evidence: Evidence::Private("AC2-EVID-DRAW-FRAME-RATE"),
        station: "auberean-draw::cpu::objects::degrade::the_frame_rate_is_the_mean_of_the_last_forty_frames",
        tier: Tier::Cpu,
    },
    behaviour! {
        id: "draw.mesh.the-players-body-turns-see-through-under-the-pointer",
        says: "With the option on, the player's body under the pointer fades at ten times its \n               opacity a second down to 0.3, and comes back the same way.",
        since: RETAIL,
        evidence: Evidence::Private("AC2-EVID-DRAW-HOVER-FADE"),
        station: "auberean-draw::cpu::objects::degrade::the_players_body_turns_see_through_under_the_pointer",
        tier: Tier::Cpu,
    },
    behaviour! {
        id: "draw.mesh.the-reflection-takes-an-objects-detail-distances-at-40-percent",
        says: "The water's reflection takes a placed object's detail distances at 40%: its detail falls that much \
               sooner there than in the frame.",
        since: RETAIL,
        evidence: Evidence::Private("AC2-EVID-REFLECTION-DETAIL"),
        station: "auberean-draw::dat::landscape::the_reflection_takes_an_objects_detail_distances_at_40_percent",
        tier: Tier::Dat,
    },
    behaviour! {
        id: "draw.mesh.the-scenery-draw-distance-draws-placed-objects-in",
        says: "At the scenery draw distance's low and medium settings a placed object's distances \
               are drawn in to 0.4 and 0.7 of themselves (its visible distance never under 20 m); \
               at high they stand.",
        since: RETAIL,
        evidence: Evidence::Private("AC2-EVID-DRAW-SCENERY-DISTANCE"),
        station: "auberean-draw::cpu::objects::degrade::the_scenery_draw_distance_draws_placed_objects_in",
        tier: Tier::Cpu,
    },
    behaviour! {
        id: "draw.mesh.the-waters-reflection-fades-an-object-at-its-own-edge",
        says: "The water's reflection fades an object over the last 7.5% of its own reach there, 40% of \n               its visible distance, whatever the frame shows of it.",
        since: RETAIL,
        evidence: Evidence::Private("AC2-EVID-DRAW-REFLECT-EDGE"),
        station: "auberean-draw::cpu::objects::degrade::the_waters_reflection_fades_an_object_at_its_own_edge",
        tier: Tier::Cpu,
    },
    behaviour! {
        id: "draw.mesh.the-waters-reflection-reaches-40-percent-of-an-objects-distance",
        says: "The water's reflection shows an object out to 40% of its visible distance.",
        since: RETAIL,
        evidence: Evidence::Private("AC2-EVID-DRAW-REFLECTION-REACH"),
        station: "auberean-draw::cpu::objects::degrade::the_waters_reflection_reaches_40_percent_of_an_objects_distance",
        tier: Tier::Cpu,
    },
    behaviour! {
        id: "draw.mesh.the-waters-reflection-sees-a-body-at-its-own-level-of-detail",
        says: "With a water's reflection in the frame a body is drawn at the level of detail the \n               reflection's distances choose, 40% of the frame's, beside the frame's own.",
        since: RETAIL,
        evidence: Evidence::Private("AC2-EVID-DRAW-REFLECT-BODY"),
        station: "auberean-draw::dat::volumes::the_waters_reflection_sees_a_body_at_its_own_level_of_detail",
        tier: Tier::Dat,
    },
    behaviour! {
        id: "draw.particles.a-looks-particle-scale-sizes-its-systems",
        says: "A particle system draws at its look's particle scale, times its object's scale \
               when its description scales it with its object.",
        since: RETAIL,
        evidence: Evidence::Private("AC2-EVID-DRAW-PARTICLE-SCALE"),
        station: "auberean-draw::cpu::effects::particles::a_looks_particle_scale_sizes_its_systems",
        tier: Tier::Cpu,
    },
    behaviour! {
        id: "draw.particles.a-mist-stands-unseen-without-volumetric-fog",
        says: "With the volumetric fog effects off, a particle system whose particles move by \
               repulsors (the start hall's ground mist) neither moves on nor draws; turned on \
               again, it does.",
        since: RETAIL,
        evidence: Evidence::Private("AC2-EVID-DRAW-VOLUMETRIC-FOG"),
        station: "auberean-draw::dat::effects::a_mist_stands_unseen_without_volumetric_fog",
        tier: Tier::Dat,
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
        station: "auberean-draw::cpu::effects::particles::a_repulsor_pushes_the_particles_in_its_range",
        tier: Tier::Cpu,
    },
    behaviour! {
        id: "draw.particles.a-square-emitter-without-fill-starts-particles-on-its-edges",
        says: "A particle emitter shaped as a square and not filled starts each particle on one of \
               the square's four edges.",
        since: RETAIL,
        evidence: Evidence::Private("AC2-EVID-DRAW-PARTICLE-SHAPES"),
        station: "auberean-draw::cpu::effects::particles::a_square_emitter_without_fill_starts_particles_on_its_edges",
        tier: Tier::Cpu,
    },
    behaviour! {
        id: "draw.particles.a-streak-stretches-from-its-tail-to-its-head",
        says: "A streaking particle is drawn from where it was the streak's time ago to where it \
               is, as wide as its size across the way it runs on the screen.",
        since: RETAIL,
        evidence: Evidence::Private("AC2-EVID-DRAW-PARTICLE-STREAKS"),
        station: "auberean-draw::cpu::effects::particles::a_streak_stretches_from_its_tail_to_its_head",
        tier: Tier::Cpu,
    },
    behaviour! {
        id: "draw.particles.a-system-goes-once-it-is-spent-and-its-particles-are-dead",
        says: "A particle system goes once none of its emitters has anything more to discharge (or it \n               was stopped) and its last particles have died.",
        since: RETAIL,
        evidence: Evidence::Private("AC2-EVID-DRAW-PARTICLE-DEATH"),
        station: "auberean-draw::cpu::effects::particles::a_system_goes_once_it_is_spent_and_its_particles_are_dead",
        tier: Tier::Cpu,
    },
    behaviour! {
        id: "draw.particles.a-system-of-its-own-draws-without-its-places-turn",
        says: "A system of its own draws at its place without the place's turn, so a swirl faces \
               the camera however its object is turned.",
        since: RETAIL,
        evidence: Evidence::Private("AC2-EVID-DRAW-PARTICLE-NO-TURN"),
        station: "auberean-draw::cpu::effects::particles::a_system_of_its_own_draws_without_its_places_turn",
        tier: Tier::Cpu,
    },
    behaviour! {
        id: "draw.particles.a-system-without-limits-runs-until-stopped",
        says: "A particle system with a physics duration, or with an emitter that has neither an \n               emission limit nor a time limit, runs until it is stopped; any other lasts about its \n               longest emitter's start, lifespan and emitting time less its fast-forward.",
        since: RETAIL,
        evidence: Evidence::Private("AC2-EVID-DRAW-PARTICLE-LIFETIME"),
        station: "auberean-draw::cpu::effects::particles::a_system_without_limits_runs_until_stopped",
        tier: Tier::Cpu,
    },
    behaviour! {
        id: "draw.particles.a-thrown-particle-falls-with-gravity-times-its-mass",
        says: "A thrown particle moves from where it was born along its velocity, pulled by \
               gravity times its mass (a negative mass rises), and is drawn one physics step \
               behind.",
        since: RETAIL,
        evidence: Evidence::Private("AC2-EVID-DRAW-PARTICLE-PHYSICS"),
        station: "auberean-draw::cpu::effects::particles::a_thrown_particle_falls_with_gravity_times_its_mass",
        tier: Tier::Cpu,
    },
    behaviour! {
        id: "draw.particles.a-timed-emitter-stops-at-its-emission-limit",
        says: "A particle emitter discharges its blast every birth interval and stops once it has \
               discharged its emission limit.",
        since: RETAIL,
        evidence: Evidence::Private("AC2-EVID-DRAW-PARTICLE-EMISSION"),
        station: "auberean-draw::cpu::effects::particles::a_timed_emitter_stops_at_its_emission_limit",
        tier: Tier::Cpu,
    },
    behaviour! {
        id: "draw.particles.a-world-system-reckons-distance-from-where-it-is-first-placed",
        says: "A particle system whose particles live in the world places each emitter where the \
               system is first queued, so an emitter discharging by distance discharges for the way \
               the system moves from there, not for the way from the world's origin.",
        since: RETAIL,
        evidence: Evidence::Private("AC2-EVID-DRAW-PARTICLE-PLACE"),
        station: "auberean-draw::cpu::effects::particles::a_world_system_reckons_distance_from_where_it_is_first_placed",
        tier: Tier::Cpu,
    },
    behaviour! {
        id: "draw.particles.an-effect-carries-the-repulsor-its-entry-names",
        says: "An effect played on an object carries the repulsor its entry names, as a look's startup effects do.",
        since: RETAIL,
        evidence: Evidence::Private("AC2-EVID-FX-REPULSOR"),
        station: "auberean-draw::dat::effects::an_effect_played_carries_the_repulsor_its_entry_names",
        tier: Tier::Dat,
    },
    behaviour! {
        id: "draw.particles.color-and-size-blend-toward-the-next-keyframe-that-blends-them",
        says: "A particle's color and size blend from its last keyframe toward the next keyframe \
               that blends them, by its age between the two.",
        since: RETAIL,
        evidence: Evidence::Private("AC2-EVID-DRAW-PARTICLE-KEYFRAMES"),
        station: "auberean-draw::cpu::effects::particles::color_and_size_blend_toward_the_next_keyframe_that_blends_them",
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
        station: "auberean-draw::cpu::effects::particles::keyframes_link_to_the_next_that_blends_each_value",
        tier: Tier::Cpu,
    },
    behaviour! {
        id: "draw.particles.placed-systems-stop-past-the-objects-visible-distance",
        says: "A placed object's particle systems aren't queued past its visible distance from the                camera, whatever their own fade distances.",
        since: RETAIL,
        evidence: Evidence::Private("AC2-EVID-PARTICLES-OBJECT-DRAWN"),
        station: "auberean-draw::dat::effects::a_placed_object_past_its_visible_distance_has_its_systems_left_unqueued",
        tier: Tier::Dat,
    },
    behaviour! {
        id: "draw.particles.unforced-systems-queue-only-as-their-object-is-drawn",
        says: "A particle system its record doesn't force to draw is queued only while its object                is drawn, so a player in first person at peace sees none of its own; one its record                forces is queued either way.",
        since: RETAIL,
        evidence: Evidence::Private("AC2-EVID-PARTICLES-OBJECT-NOT-DRAWN"),
        station: "auberean-draw::dat::effects::a_system_not_forced_to_draw_is_queued_only_while_its_object_is_drawn",
        tier: Tier::Dat,
    },
    behaviour! {
        id: "draw.particles.waveforms-take-their-shape-at-a-time",
        says: "A waveform's value at a time is its base when steady, grows with the time at speed, \
               follows a sine of its frequency and phase (sine, square, bounce), or rolls within \
               its amplitude.",
        since: RETAIL,
        evidence: Evidence::Private("AC2-EVID-DRAW-WAVEFORMS"),
        station: "auberean-draw::cpu::effects::particles::waveforms_take_their_shape_at_a_time",
        tier: Tier::Cpu,
    },
    behaviour! {
        id: "draw.picture.a-picture-made-for-one-language-is-in-its-file",
        says: "A picture made for one language, a signpost's wood and words, is read from that                language's file.",
        since: RETAIL,
        evidence: Evidence::Private("AC2-EVID-DRAW-LANGUAGE-PICTURES"),
        station: "auberean-draw::dat::pictures::a_picture_made_for_one_language_is_in_its_file",
        tier: Tier::Dat,
    },
    behaviour! {
        id: "draw.picture.a-texture-at-a-lower-detail-starts-at-a-smaller-level",
        says: "A texture at a lower detail starts at a smaller stored level: the levels texture \
               detail leaves out are its largest, the rest kept as stored.",
        since: RETAIL,
        evidence: Evidence::Private("AC2-EVID-DRAW-TEXTURE-LEVELS-DROPPED"),
        station: "auberean-draw::cpu::materials::picture::a_texture_at_a_lower_detail_starts_at_a_smaller_level",
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
        id: "draw.picture.anisotropic-filtering-draws-a-slanted-texture-from-larger-levels",
        says: "With anisotropic filtering a texture seen at a slant is drawn from larger levels \
               than without it.",
        since: RETAIL,
        evidence: Evidence::Private("AC2-EVID-DRAW-ANISOTROPIC"),
        station: "auberean-gpu::gpu::textures::anisotropic_filtering_draws_a_slanted_texture_from_larger_levels",
        tier: Tier::Gpu,
    },
    behaviour! {
        id: "draw.picture.every-stored-picture-is-in-a-format-the-draw-reads",
        says: "Every picture of the install is in a format the draw reads; none is in YUY2.",
        since: RETAIL,
        evidence: Evidence::Private("AC2-EVID-DRAW-STORED-PICTURE-FORMATS"),
        station: "auberean-draw::dat::pictures::every_stored_picture_is_in_a_format_the_draw_reads",
        tier: Tier::Dat,
    },
    behaviour! {
        id: "draw.picture.texture-detail-leaves-out-the-largest-levels",
        says: "Texture detail from lowest to highest leaves out the four, three, two or one largest \
               levels of a picture list, none at the highest when its high detail is kept, never \
               every level.",
        since: RETAIL,
        evidence: Evidence::Private("AC2-EVID-DRAW-TEXTURE-DETAIL"),
        station: "auberean-draw::cpu::materials::picture::texture_detail_leaves_out_the_largest_levels",
        tier: Tier::Cpu,
    },
    behaviour! {
        id: "draw.picture.the-device-draws-a-texture-from-the-level-its-detail-starts-at",
        says: "The device draws a texture from the level its detail starts at, its textures \
               loaded again when the detail changes.",
        since: RETAIL,
        evidence: Evidence::Private("AC2-EVID-DRAW-TEXTURE-DETAIL-FLUSH"),
        station: "auberean-gpu::gpu::textures::the_device_draws_a_texture_from_the_level_its_detail_starts_at",
        tier: Tier::Gpu,
    },
    behaviour! {
        id: "draw.picture.trilinear-filtering-blends-between-a-textures-levels",
        says: "With trilinear filtering a texture blends between its two nearest levels; without \
               it each texel comes from the nearest level alone. The filtering the client starts \
               with stays until it starts again.",
        since: RETAIL,
        evidence: Evidence::Private("AC2-EVID-DRAW-TRILINEAR"),
        station: "auberean-gpu::gpu::textures::trilinear_filtering_blends_between_a_textures_levels",
        tier: Tier::Gpu,
    },
    behaviour! {
        id: "draw.portal.portal-space-comes-in-and-goes-out-a-second-a-phase",
        says: "Portal space comes in over a second (the world's view narrowing to a fifth under a black veil while the tunnel and the nebula fade in), stays in with no world drawn, then goes out over a second (the tunnel's view widening to 2.4 times as white fades in) and comes out over another (the world's view widening back as the white fades out); started at once, it is in at once.",
        since: RETAIL,
        evidence: Evidence::Private("AC2-EVID-PORTAL-PHASES"),
        station: "auberean-draw::cpu::effects::portal::portal_space_comes_in_and_goes_out_a_second_a_phase",
        tier: Tier::Cpu,
    },
    behaviour! {
        id: "draw.portal.puffs-of-cloud-show-near-the-tunnels-start",
        says: "With the tunnel more than 0.9 in view, each ring 40 to 49 ahead has one even chance of letting a puff of cloud go within 0.8 of its radius. The system has the usual fade, measured from the tube's start, so puffs draw only while the camera is within 60 m of it.",
        since: RETAIL,
        evidence: Evidence::Private("AC2-EVID-PORTAL-PUFFS"),
        station: "auberean-draw::dat::effects::puffs_of_cloud_show_near_the_tunnels_start",
        tier: Tier::Dat,
    },
    behaviour! {
        id: "draw.portal.the-tunnel-is-rings-three-meters-apart-the-camera-rushes-along",
        says: "The tunnel is 150 rings 3 m apart along a winding tube, each of radius 10 + 6 sin(0.01 d) at its distance d; the camera rushes along at 120 m/s, the rings it passes made again at the far end; the rings fade from 1.0 nearest to 0.8 at the sixteenth and from there to nothing at the far end.",
        since: RETAIL,
        evidence: Evidence::Private("AC2-EVID-PORTAL-TUNNEL"),
        station: "auberean-draw::cpu::effects::portal::the_tunnel_is_rings_three_meters_apart_the_camera_rushes_along",
        tier: Tier::Cpu,
    },
    behaviour! {
        id: "draw.quality.an-overall-quality-sets-the-graphics-preferences",
        says: "Each of five overall graphics qualities sets every graphics preference but the screen's \
               brightness, values and defaults; the quality the preferences hold is the first of the \
               five whose every preference but the adaptive degrade's bias they match, else none.",
        since: RETAIL,
        evidence: Evidence::Private("AC2-EVID-RENDER-QUALITY"),
        station: "auberean-draw::cpu::options::quality::an_overall_quality_sets_the_graphics_preferences",
        tier: Tier::Cpu,
    },
    behaviour! {
        id: "draw.quality.the-reflection-ripples-only-at-the-highest-water-reflections",
        says: "The water's reflection ripples only with the water reflections preference at its                highest choice; at the lower one it is laid as it stands.",
        since: RETAIL,
        evidence: Evidence::Private("AC2-EVID-WATER-RIPPLES-HIGH"),
        station: "auberean-draw::cpu::options::quality::the_reflection_ripples_only_at_the_highest_water_reflections",
        tier: Tier::Cpu,
    },
    behaviour! {
        id: "draw.quality.the-screen-brightness-sets-each-levels-ramp",
        says: "In a full screen the brightness preference, held to -0.2 to 1, takes a color's \
               level i to 255 i (1 + 2 b), held to 16 bits; in a window it changes nothing.",
        since: RETAIL,
        evidence: Evidence::Private("AC2-EVID-DRAW-BRIGHTNESS"),
        station: "auberean-draw::cpu::options::quality::the_screen_brightness_sets_each_levels_ramp",
        tier: Tier::Cpu,
    },
    behaviour! {
        id: "draw.rain.a-streak-falls-from-80-m-at-150-m-a-second",
        says: "A rain streak starts 80 m over its landing and falls at 150 m a second.",
        since: RETAIL,
        evidence: Evidence::Private("AC2-EVID-RAIN-FALL"),
        station: "auberean-draw::cpu::effects::rain::a_streak_falls_from_80_m_at_150_m_a_second",
        tier: Tier::Cpu,
    },
    behaviour! {
        id: "draw.rain.a-streak-is-a-thin-quad-fading-toward-its-top",
        says: "A rain streak is a quad a tenth of a meter wide across the camera, 8 m long \n               along its slant, its top transparent and its foot pale at 0.45.",
        since: RETAIL,
        evidence: Evidence::Private("AC2-EVID-RAIN-STREAK"),
        station: "auberean-draw::cpu::effects::rain::a_streak_is_a_thin_quad_fading_toward_its_top",
        tier: Tier::Cpu,
    },
    behaviour! {
        id: "draw.rain.a-streak-left-behind-picks-a-new-spot",
        says: "A rain streak more than five cells from the viewer picks a new spot within 130 \n               m of it.",
        since: RETAIL,
        evidence: Evidence::Private("AC2-EVID-RAIN-LEFT-BEHIND"),
        station: "auberean-draw::cpu::effects::rain::a_streak_left_behind_picks_a_new_spot",
        tier: Tier::Cpu,
    },
    behaviour! {
        id: "draw.rain.a-streak-touching-down-throws-up-vapor-and-ripples",
        says: "A rain streak touching down throws up vapor; on the water of the viewer's cell \n               it lands on the water and, within 65 m, makes a ripple.",
        since: RETAIL,
        evidence: Evidence::Private("AC2-EVID-RAIN-TOUCHDOWN"),
        station: "auberean-draw::cpu::effects::rain::a_streak_touching_down_throws_up_vapor_and_ripples",
        tier: Tier::Cpu,
    },
    behaviour! {
        id: "draw.rain.as-many-streaks-fall-as-the-rain-asks",
        says: "As many rain streaks fall as the rain amount asks, up to 2,500; they are drawn \n               over 0.4 of it.",
        since: RETAIL,
        evidence: Evidence::Private("AC2-EVID-RAIN-COUNT"),
        station: "auberean-draw::cpu::effects::rain::as_many_streaks_fall_as_the_rain_asks",
        tier: Tier::Cpu,
    },
    behaviour! {
        id: "draw.rain.over-land-not-loaded-a-spot-keeps-the-viewers-height",
        says: "Over land not loaded a rain streak's spot keeps the viewer's height and makes \n               no ripple.",
        since: RETAIL,
        evidence: Evidence::Private("AC2-EVID-RAIN-UNLOADED"),
        station: "auberean-draw::cpu::effects::rain::over_land_not_loaded_a_spot_keeps_the_viewers_height",
        tier: Tier::Cpu,
    },
    behaviour! {
        id: "draw.rain.rain-strikes-a-bodys-bones-across-it",
        says: "Rain strikes the player's bones lying across it, throwing up vapor three times \n               in four.",
        since: RETAIL,
        evidence: Evidence::Private("AC2-EVID-RAIN-BODY"),
        station: "auberean-draw::cpu::effects::rain::rain_strikes_a_bodys_bones_across_it",
        tier: Tier::Cpu,
    },
    behaviour! {
        id: "draw.rain.water-gives-a-body-bubbles-and-ripples",
        says: "In water, a body's bones deep under give off bubbles and those crossing the \n               surface make ripples.",
        since: RETAIL,
        evidence: Evidence::Private("AC2-EVID-RAIN-WATER-BODY"),
        station: "auberean-draw::cpu::effects::rain::water_gives_a_body_bubbles_and_ripples",
        tier: Tier::Cpu,
    },
    behaviour! {
        id: "draw.shadow.cloud-shadows-slide-with-the-running-time",
        says: "The clouds' shadow picture lies flat over the landscape at its scale and slides by \
               its speeds times the client's running time.",
        since: RETAIL,
        evidence: Evidence::Private("AC2-EVID-DRAW-CLOUD-SHADOWS"),
        station: "auberean-draw::cpu::landscape::shadows::cloud_shadows_slide_with_the_running_time",
        tier: Tier::Cpu,
    },
    behaviour! {
        id: "draw.shadow.the-map-spans-the-near-terrain-from-the-sun",
        says: "The sun's shadow map covers the detailed terrain's extent as the sun sees it, and a \
               shadow in it takes away 0.95 of the day's shadow color.",
        since: RETAIL,
        evidence: Evidence::Private("AC2-EVID-DRAW-SHADOW-MAP"),
        station: "auberean-draw::cpu::landscape::shadows::the_map_spans_the_near_terrain_from_the_sun",
        tier: Tier::Cpu,
    },
    behaviour! {
        id: "draw.shadow.the-sun-looks-along-its-light-with-up-kept-up",
        says: "The shadow map looks along the sun's light, with the world's up kept as near up as \
               it can be, or north when the light falls straight down.",
        since: RETAIL,
        evidence: Evidence::Private("AC2-EVID-DRAW-SHADOW-FRAME"),
        station: "auberean-draw::cpu::landscape::shadows::the_sun_looks_along_its_light_with_up_kept_up",
        tier: Tier::Cpu,
    },
    behaviour! {
        id: "draw.shore.a-beach-makes-one-open-strip-along-it",
        says: "Where the ground runs down into the water straight across a block, the shore is \
               one open strip along it, two vertices a point where the ground's edges cross the \
               water, which carries foam.",
        since: RETAIL,
        evidence: Evidence::Private("AC2-EVID-SHORE-BEACH"),
        station: "auberean-draw::cpu::landscape::shores::a_beach_makes_one_open_strip_along_it",
        tier: Tier::Cpu,
    },
    behaviour! {
        id: "draw.shore.a-block-whose-water-stands-lower-than-the-viewers-has-no-shore",
        says: "A detailed block's water makes a shore only when it stands no more than 0.1 m \
               below the water of the viewer's own block (when that block has water); a block \
               whose water stands lower is passed over.",
        since: RETAIL,
        evidence: Evidence::Private("AC2-EVID-SHORE-LOWER-WATER"),
        station: "auberean-draw::cpu::landscape::shores::a_block_whose_water_stands_lower_than_the_viewers_has_no_shore",
        tier: Tier::Cpu,
    },
    behaviour! {
        id: "draw.shore.a-closed-shore-under-20-m-carries-no-foam",
        says: "A closed shore shorter than 20 m carries no foam; its strip is still drawn into \
               the frame's alpha.",
        since: RETAIL,
        evidence: Evidence::Private("AC2-EVID-SHORE-SHORT-NO-FOAM"),
        station: "auberean-draw::cpu::landscape::shores::a_closed_shore_under_20_m_carries_no_foam",
        tier: Tier::Cpu,
    },
    behaviour! {
        id: "draw.shore.a-ponds-shore-runs-round-it-out-over-the-water",
        says: "A pond's shore is one closed strip round it: its points where the ground's edges \
               cross the water, each pulled 5 cm up the bank, its far edge out over the water \
               twelve times the ground's upward slant there; measured along its length from its \
               first point, ending with a copy of it, with one random offset across it.",
        since: RETAIL,
        evidence: Evidence::Private("AC2-EVID-SHORE-POND"),
        station: "auberean-draw::cpu::landscape::shores::a_ponds_shore_runs_round_it_out_over_the_water",
        tier: Tier::Cpu,
    },
    behaviour! {
        id: "draw.shore.the-camps-pond-has-a-shore-where-its-water-meets-the-ground",
        says: "The camp's pond has its shore at the water's height, drawn first with the shore's \
               alpha material, black at half alpha, then, with the shore effects at their \
               highest, with the foam's, black; found once for the viewer's block and kept while \
               the viewer stays in it; no foam at the middle setting, no shore with them off.",
        since: RETAIL,
        evidence: Evidence::Private("AC2-EVID-SHORE-CAMP"),
        station: "auberean-draw::dat::landscape::the_camps_pond_has_a_shore_where_its_water_meets_the_ground",
        tier: Tier::Dat,
    },
    behaviour! {
        id: "draw.streak.a-bodys-own-streak-is-drawn-where-it-is",
        says: "Every object drawn places its own streaks where it is drawn: a body's (the \
               player's, another's, one the client made), as a held object's where it hangs; \
               turning a streak on or off reaches the object's own and those of what it holds at \
               the holding location, the client's objects among them; turned off, the ribbon \
               fades and goes.",
        since: RETAIL,
        evidence: Evidence::Private("AC2-EVID-STREAK-OWN"),
        station: "auberean-shell::dat::session::a_bodys_own_streak_is_drawn_where_it_is",
        tier: Tier::Dat,
    },
    behaviour! {
        id: "draw.streak.a-held-streak-runs-the-blade-at-its-size",
        says: "A streak's line takes its object's size once: the place it is laid from is the \
               drawing frame's turn and position alone, and the line is scaled by the largest of \
               the object's scales (a sword held at twice its size leaves a ribbon twice as wide).",
        since: RETAIL,
        evidence: Evidence::Private("AC2-EVID-STREAK-SCALE"),
        station: "auberean-shell::dat::session::a_held_streak_runs_the_blade_at_its_size",
        tier: Tier::Dat,
    },
    behaviour! {
        id: "draw.streak.a-moving-line-lays-segments-while-the-streak-is-on",
        says: "A streak lays segments only while it is on: placed after being turned on, the line \
               on its object where the object is drawn is the newest segment, a copy laid after \
               the first; within the least distance of the one before (and, with a least time, \
               before it has passed) the newest moves with the line, the distance run growing; \
               past it another is laid; the line scales with its object.",
        since: RETAIL,
        evidence: Evidence::Private("AC2-EVID-STREAK-TRAIL"),
        station: "auberean-draw::cpu::effects::streak::a_moving_line_lays_segments_while_the_streak_is_on",
        tier: Tier::Cpu,
    },
    behaviour! {
        id: "draw.streak.a-ribbon-runs-along-the-trail",
        says: "A streak's ribbon is two vertices a segment, oldest first, the line's second point \
               then its first (the picture's v 0 and 1), the picture's u the distance run times \
               the texture scale, the streak's color with its alpha times the segment's truncated \
               to bytes; none when its object was not drawn in the frame, with fewer than two \
               segments, or past the frame's room for streaks; drawn after the particle systems.",
        since: RETAIL,
        evidence: Evidence::Private("AC2-EVID-STREAK-RIBBON"),
        station: "auberean-draw::cpu::effects::streak::a_ribbon_runs_along_the_trail",
        tier: Tier::Cpu,
    },
    behaviour! {
        id: "draw.streak.a-swung-streak-leaves-a-ribbon-of-light",
        says: "A streak turned on and swung through the air adds its material's picture onto the \
               frame along the swing, brightening it and darkening nothing; a streak not turned \
               on draws nothing.",
        since: RETAIL,
        evidence: Evidence::Private("AC2-EVID-STREAK-DRAW"),
        station: "auberean-gpu::gpu::streaks::a_swung_streak_leaves_a_ribbon_of_light",
        tier: Tier::Gpu,
    },
    behaviour! {
        id: "draw.streak.a-trail-fades-and-a-streak-ends-with-its-lifespan",
        says: "Every segment of a trail fades as the square of one less its age over the trail \
               time and is gone once its age passes the trail time by a tenth of a second; a \
               streak that lasts a while is on at once and goes off once its lifespan has passed.",
        since: RETAIL,
        evidence: Evidence::Private("AC2-EVID-STREAK-FADE"),
        station: "auberean-draw::cpu::effects::streak::a_trail_fades_and_a_streak_ends_with_its_lifespan",
        tier: Tier::Cpu,
    },
    behaviour! {
        id: "draw.streak.a-weapon-carries-a-streak-a-swing-turns-on",
        says: "A weapon's look makes its streak as the rest of its startup effects, waiting to be \
               turned on; a body's swing turns the streaks of what it holds at a holding location \
               on and off as its effects' entries say; with the streaks preference off no streak \
               is made or turned on.",
        since: RETAIL,
        evidence: Evidence::Private("AC2-EVID-STREAK-FX"),
        station: "auberean-draw::dat::effects::a_weapons_streak_waits_for_a_swing_to_turn_it_on",
        tier: Tier::Dat,
    },
    behaviour! {
        id: "draw.streak.stopping-the-effect-that-turned-a-streak-on-turns-it-off",
        says: "Stopping an effect whose entry turns a streak on turns that streak off, whether or not the effect still plays.",
        since: RETAIL,
        evidence: Evidence::Private("AC2-EVID-FX-STREAK-STOP"),
        station: "auberean-draw::dat::effects::stopping_the_effect_that_turned_a_streak_on_turns_it_off",
        tier: Tier::Dat,
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
        station: "auberean-draw::cpu::objects::layout::a_mask_turns_a_quarter_per_step",
        tier: Tier::Cpu,
    },
    behaviour! {
        id: "draw.view.a-scenes-camera-takes-the-properties-of-its-camera-group",
        says: "An interface scene's camera stands where its entity does, looking along the \
               entity's +y, and takes the properties of its camera group, each of its own kind: \
               seen straight on (orthographic), its field of view, the height it sees straight \
               on, near and far; with none it sees in perspective, 65 degrees high, from 0.1 to \
               2000 m.",
        since: RETAIL,
        evidence: Evidence::Private("AC2-EVID-UI-SCENE-CAMERA"),
        station: "auberean-scene::cpu::view::a_scenes_camera_takes_the_properties_of_its_camera_group",
        tier: Tier::Cpu,
    },
    behaviour! {
        id: "draw.view.a-scenes-objects-play-their-effects",
        says: "The objects an interface scene places play their startup effects while it shows, \
               on its own clock: the avatar hall's braziers burn.",
        since: RETAIL,
        evidence: Evidence::Private("AC2-EVID-DRAW-VIEW-EFFECTS"),
        station: "auberean-draw::dat::effects::a_scenes_objects_play_their_effects",
        tier: Tier::Dat,
    },
    behaviour! {
        id: "draw.view.a-scenes-own-lights-light-its-draws-in-place-of-the-sun",
        says: "An interface scene with lights of its own lights each draw by those it keeps \
               where it stands, as a draw inside keeps a cell's, with the sun off and no ambient \
               light; it is not one light standing for them.",
        since: RETAIL,
        evidence: Evidence::Private("AC2-EVID-DRAW-VIEW-LIGHTS"),
        station: "auberean-draw::dat::parts::a_scenes_own_lights_light_its_draws_in_place_of_the_sun",
        tier: Tier::Dat,
    },
    behaviour! {
        id: "draw.viewport.a-viewports-scene-covers-only-where-it-draws",
        says: "A panel's viewport (the paper doll's) covers the panels behind it only where its \
               scene draws, wholly under what is opaque and partly under what is seen through; \
               everywhere else they show through.",
        since: RETAIL,
        evidence: Evidence::Private("AC2-EVID-VIEWPORT-COVERAGE"),
        station: "auberean-gpu::gpu::shores::a_viewports_scene_covers_only_where_it_draws",
        tier: Tier::Gpu,
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
        station: "auberean-draw::cpu::objects::volume::a_closed_mesh_casts_a_closed_volume_away_from_the_light",
        tier: Tier::Cpu,
    },
    behaviour! {
        id: "draw.volume.a-draws-step-goes-by-its-object",
        says: "A draw's step goes by its object, never by its layer: an object not seen through is \
               drawn with the opaque objects, every layer of it (blended by its pictures' alpha or \
               not, marking how near it is or not), before the volumes darken the frame unless it \
               casts one; an object its drawing properties make seen through is drawn with those \
               seen through; an interior cell's mesh is drawn before the decals.",
        since: RETAIL,
        evidence: Evidence::Private("AC2-EVID-VOLUME-PHASES"),
        station: "auberean-draw::dat::volumes::a_draws_step_goes_by_its_object",
        tier: Tier::Dat,
    },
    behaviour! {
        id: "draw.volume.a-mesh-standing-as-it-did-keeps-its-volume",
        says: "A mesh standing as it did in the frame before, under the same light, keeps its volume, and what fits the frame's room is what a volume made afresh would give.",
        since: TOOLING,
        evidence: Evidence::Private("AC2-EVID-VOLUMES-KEPT"),
        station: "auberean-draw::dat::volumes::a_mesh_standing_as_it_did_keeps_its_volume",
        tier: Tier::Dat,
    },
    behaviour! {
        id: "draw.volume.a-placed-object-takes-its-drawing-properties-from-its-entities",
        says: "An object a landblock or a cell places takes its drawing properties as any object \
               made from an entity does: the entities it is made from first, innermost first, then \
               the placed entity's own, the last that says deciding each bit of how it is drawn.",
        since: RETAIL,
        evidence: Evidence::Private("AC2-EVID-VOLUME-PLACED-PROPERTIES"),
        station: "auberean-physics::cpu::contact::solid::a_placed_object_takes_its_drawing_properties_from_its_entities",
        tier: Tier::Cpu,
    },
    behaviour! {
        id: "draw.volume.a-placed-objects-shadow-turns-away-from-a-carried-light",
        says: "A placed object that casts a volume of shadow throws it the way the lights it keeps \
               throw it, the lights objects carry that reach it among them: outdoors a torch \
               beside it turns its shadow from the sun's way to away from the torch, inside a \
               torch turns it from the way its own lights throw it; once the torch is gone the \
               shadow falls as before.",
        since: RETAIL,
        evidence: Evidence::Private("AC2-EVID-VOLUME-PLACED-CARRIED"),
        station: "auberean-draw::dat::volumes::a_placed_objects_shadow_turns_away_from_a_carried_light",
        tier: Tier::Dat,
    },
    behaviour! {
        id: "draw.volume.a-placed-objects-volume-follows-its-pose",
        says: "A placed object's volume of shadow is cast from it as it stands, and moves as it plays.",
        since: RETAIL,
        evidence: Evidence::Private("AC2-EVID-PLACED-VOLUME-POSED"),
        station: "auberean-draw::dat::placed::a_placed_objects_volume_follows_its_pose",
        tier: Tier::Dat,
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
        station: "auberean-draw::cpu::objects::volume::a_shadow_takes_away_the_shadow_color_or_gray_inside",
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
        station: "auberean-draw::cpu::objects::volume::a_volume_is_made_from_a_coarse_level",
        tier: Tier::Cpu,
    },
    behaviour! {
        id: "draw.volume.a-volume-is-made-only-whole-and-in-room",
        says: "A volume is made only whole and in the room the frame has left (of a hundred \
               volumes, forty thousand vertices and sixty thousand indices): none for a mesh of \
               three triangles or fewer, none whose vertices or indices would pass the room left.",
        since: RETAIL,
        evidence: Evidence::Private("AC2-EVID-VOLUME-ROOM"),
        station: "auberean-draw::cpu::objects::volume::a_volume_is_made_only_whole_and_in_room",
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
        station: "auberean-physics::cpu::contact::solid::an_objects_drawing_properties_say_how_it_is_drawn",
        tier: Tier::Cpu,
    },
    behaviour! {
        id: "draw.volume.at-self-shadowing-a-body-shadows-itself",
        says: "At the self shadowing setting the objects that cast volumes are drawn with every \
               other opaque object before the darkening and make their volumes as they are drawn, \
               at any distance, farthest first, from the level drawn with the near cap pushed five \
               centimeters along the light: a body's own side away from the light is darkened.",
        since: RETAIL,
        evidence: Evidence::Private("AC2-EVID-VOLUME-SELF-SHADOWING"),
        station: "auberean-gpu::gpu::shadows::at_self_shadowing_a_body_shadows_itself",
        tier: Tier::Gpu,
    },
    behaviour! {
        id: "draw.volume.placed-objects-that-say-so-cast-volumes",
        says: "The objects the world places cast volumes of shadow when their entities say so (63 \
               a landblock places, 799 its cells place), as the bodies do: drawn after the volumes, \
               those within forty meters of the camera cast, nearest first, their shadow falling \
               the sun's way outdoors and the way their own lights throw it inside.",
        since: RETAIL,
        evidence: Evidence::Private("AC2-EVID-VOLUME-PLACED"),
        station: "auberean-draw::dat::volumes::placed_objects_that_say_so_cast_volumes",
        tier: Tier::Dat,
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
        station: "auberean-draw::cpu::objects::volume::the_lights_an_object_keeps_throw_its_shadow",
        tier: Tier::Cpu,
    },
    behaviour! {
        id: "draw.volume.triangles-meet-at-edges-run-the-other-way",
        says: "A mesh level's triangles meet at their edges: vertices within two ten-thousandths \
               of a meter of each other on every axis are one, and an edge a later triangle runs \
               the other way joins the first such edge still unmet.",
        since: RETAIL,
        evidence: Evidence::Private("AC2-EVID-VOLUME-CONNECTIVITY"),
        station: "auberean-draw::cpu::objects::volume::triangles_meet_at_edges_run_the_other_way",
        tier: Tier::Cpu,
    },
    behaviour! {
        id: "draw.water.a-rooms-draws-are-not-in-the-reflection",
        says: "A room's draws seen from the land are not in the water's reflection, which gathers only \n               what stands on the land; with the camera in a room there is no reflection at all.",
        since: RETAIL,
        evidence: Evidence::Private("AC2-EVID-DRAW-REFLECT-LAND"),
        station: "auberean-gpu::gpu::shores::a_rooms_draws_are_not_in_the_reflection",
        tier: Tier::Gpu,
    },
    behaviour! {
        id: "draw.water.an-object-under-the-water-reflects-only-past-it",
        says: "An object standing under the water's plane shows in the reflection only past the water's \n               depth: the part above the water reflects, a wall wholly under it does not.",
        since: RETAIL,
        evidence: Evidence::Private("AC2-EVID-DRAW-REFLECT-BELOW"),
        station: "auberean-gpu::gpu::shores::an_object_under_the_water_reflects_only_past_it",
        tier: Tier::Gpu,
    },
    behaviour! {
        id: "draw.water.the-foam-lies-over-the-waters-reflection",
        says: "The foam along the shores is drawn after the water and its reflection, over \
               them.",
        since: RETAIL,
        evidence: Evidence::Private("AC2-EVID-SHORE-FOAM-OVER-REFLECTION"),
        station: "auberean-gpu::gpu::shores::the_foam_lies_over_the_waters_reflection",
        tier: Tier::Gpu,
    },
    behaviour! {
        id: "draw.water.the-frame-keeps-alpha-127-but-where-a-shore-lies",
        says: "The frame's alpha is 127 of 255 under everything drawn (the device writes color \
               alone, every clear leaves that alpha); only the shores' strips write it, alone, \
               before the water: their picture's alpha times half.",
        since: RETAIL,
        evidence: Evidence::Private("AC2-EVID-SHORE-ALPHA"),
        station: "auberean-gpu::gpu::shores::the_frame_keeps_alpha_127_but_where_a_shore_lies",
        tier: Tier::Gpu,
    },
    behaviour! {
        id: "draw.water.the-reflection-is-drawn-512-texels-square",
        says: "The water's reflection is drawn into a picture of its own 512 texels square, with                a depth of its own the same size, whatever the screen's size, and laid over the                screen as it stands.",
        since: RETAIL,
        evidence: Evidence::Private("AC2-EVID-WATER-REFLECTION-SIZE"),
        station: "auberean-gpu::gpu::water::the_reflection_is_drawn_512_texels_square",
        tier: Tier::Gpu,
    },
    behaviour! {
        id: "draw.water.the-reflection-is-laid-over-by-half",
        says: "The water's reflection is laid over the water by the alpha the frame holds, 127                of 255 (the device writes only color, and every clear leaves that alpha): half                the reflection and half the water as it was drawn.",
        since: RETAIL,
        evidence: Evidence::Private("AC2-EVID-WATER-REFLECTION-HALF"),
        station: "auberean-gpu::gpu::water::the_reflection_is_laid_over_by_half",
        tier: Tier::Gpu,
    },
    behaviour! {
        id: "draw.water.the-reflection-shows-the-land-under-the-water",
        says: "The water's reflection shows all the land the mirrored camera can see, what lies under the water too.",
        since: RETAIL,
        evidence: Evidence::Private("AC2-EVID-REFLECTION-LAND"),
        station: "auberean-gpu::gpu::water::the_reflection_shows_the_land_under_the_water",
        tier: Tier::Gpu,
    },
    behaviour! {
        id: "draw.water.the-reflection-takes-objects-by-where-they-stand",
        says: "The water's reflection shows an object standing above the water whole, and none standing below it.",
        since: RETAIL,
        evidence: Evidence::Private("AC2-EVID-REFLECTION-OBJECTS"),
        station: "auberean-gpu::gpu::water::the_reflection_takes_objects_by_where_they_stand",
        tier: Tier::Gpu,
    },
    behaviour! {
        id: "draw.water.the-ripples-push-the-reflection-one-way",
        says: "The ripples push the water's reflection one way only, never back: what the                reflection shows is drawn out downward and to the left in spikes, never up or to                the right; without ripples it lies as it stands.",
        since: RETAIL,
        evidence: Evidence::Private("AC2-EVID-WATER-RIPPLES"),
        station: "auberean-gpu::gpu::water::the_ripples_push_the_reflection_one_way",
        tier: Tier::Gpu,
    },
    behaviour! {
        id: "draw.water.the-water-shows-half-the-ground-under-it",
        says: "The water's own material blends by the destination's alpha, which is 127 of 255                wherever it is drawn (the device writes only color, and every clear leaves that                alpha): the water shows half its own color and half the ground under it.",
        since: RETAIL,
        evidence: Evidence::Private("AC2-EVID-WATER-GROUND-HALF"),
        station: "auberean-gpu::gpu::water::the_water_shows_half_the_ground_under_it",
        tier: Tier::Gpu,
    },
    behaviour! {
        id: "draw.water.the-water-shows-less-of-itself-along-the-shore",
        says: "Across a shore's strip the water shows its own color by the frame's alpha there, \
               less of it near the waterline, so the ground under it shows through more; off the \
               strip half and half.",
        since: RETAIL,
        evidence: Evidence::Private("AC2-EVID-SHORE-WATER-SHARE"),
        station: "auberean-gpu::gpu::shores::the_water_shows_less_of_itself_along_the_shore",
        tier: Tier::Gpu,
    },
    behaviour! {
        id: "draw.world.a-blocks-water-lies-in-quarter-block-polygons",
        says: "A detailed block's water surface is four polygons by four, each a quarter of the                block wide, its picture and its ripples' coordinates both running 0 to 1 across                each polygon, whatever the water's own tile and polygon widths say.",
        since: RETAIL,
        evidence: Evidence::Private("AC2-EVID-WATER-GRID"),
        station: "auberean-draw::dat::landscape::a_detailed_blocks_water_lies_in_quarter_block_polygons",
        tier: Tier::Dat,
    },
    behaviour! {
        id: "draw.world.a-body-takes-the-sun-shadow-about-its-spot",
        says: "A body standing on the land takes the sun's shadow map about its spot in its \n               light and specular.",
        since: RETAIL,
        evidence: Evidence::Private("AC2-EVID-DRAW-BODY-SHADE"),
        station: "auberean-gpu::gpu::shadows::a_body_takes_the_sun_shadow_about_its_spot",
        tier: Tier::Gpu,
    },
    behaviour! {
        id: "draw.world.a-far-blocks-water-repeats-every-four-tile-widths",
        says: "A far block's water surface is one polygon, its picture and the reflection's ripples on one set of                coordinates that repeats every four of the water's tile widths.",
        since: RETAIL,
        evidence: Evidence::Private("AC2-EVID-FAR-WATER-COORDS"),
        station: "auberean-draw::dat::landscape::a_far_blocks_water_repeats_every_four_tile_widths",
        tier: Tier::Dat,
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
        id: "draw.world.a-meshs-depth-writing-fragments-draw-first",
        says: "A mesh draws the fragments whose material's first layer marks how near it is before those that do not, each kind in its stored order.",
        since: RETAIL,
        evidence: Evidence::Private("AC2-EVID-DRAW-FRAGMENT-ORDER"),
        station: "auberean-draw::dat::parts::a_meshs_depth_writing_fragments_draw_first",
        tier: Tier::Dat,
    },
    behaviour! {
        id: "draw.world.a-part-draws-its-own-copy-when-its-look-changes-materials",
        says: "A part draws through its own frame carried by the object's the mesh its instance                holds: the one every part showing it shares, or, when the part's look changes its                materials, its own copy with them changed, the other parts keeping the mesh's own;                it is culled by its mesh's box carried through its frame.",
        since: RETAIL,
        evidence: Evidence::Private("AC2-EVID-PART-DRAW"),
        station: "auberean-draw::dat::parts::a_part_draws_its_own_copy_when_its_look_changes_its_materials",
        tier: Tier::Dat,
    },
    behaviour! {
        id: "draw.world.a-placed-look-with-a-behavior-table-plays-through-one-shared-body",
        says: "An object the world places whose look has a behavior table plays it through one body kept for the look, stepped once a frame, so every object showing the look stands in one pose and moves with the others; a look with no behavior table stands as stored.",
        since: RETAIL,
        evidence: Evidence::Private("AC2-EVID-PLACED-PLAYS"),
        station: "auberean-draw::dat::placed::a_placed_school_of_fish_swims_and_every_object_showing_it_moves_together",
        tier: Tier::Dat,
    },
    behaviour! {
        id: "draw.world.a-placed-objects-box-holds-it-where-its-animation-takes-it",
        says: "The box a placed object that plays is culled by holds every vertex wherever its animation takes it, not only where the mesh is stored.",
        since: THIS_CLIENT,
        divergence: "CD2-placed-boxes-hold-the-animation",
        evidence: Evidence::Private("AC2-EVID-PLACED-BOX"),
        station: "auberean-draw::dat::placed::a_placed_objects_box_holds_it_wherever_its_animation_takes_it",
        tier: Tier::Dat,
    },
    behaviour! {
        id: "draw.world.only-objects-that-say-so-cast-on-the-land",
        says: "An object prints on the sun's shadow map only when its drawing properties say it casts on the land. Those without the property, placed or scenery, leave no shadow there.",
        since: RETAIL,
        evidence: Evidence::Private("AC2-EVID-LAND-CASTERS-0930"),
        station: "auberean-draw::dat::landscape::only_objects_that_say_so_cast_on_the_land",
        tier: Tier::Dat,
    },
    behaviour! {
        id: "draw.world.the-far-landscape-and-the-frills-reach-as-far-as-set",
        says: "The far landscape reaches as many blocks past the detailed ones as its radius, the detailed blocks drawn at every radius; frills are drawn only in the land cells within their radius of the viewer's, none when turned off, fading out over the last eight meters of their reach.",
        since: RETAIL,
        evidence: Evidence::Private("AC2-EVID-DRAW-LANDSCAPE-REACH"),
        station: "auberean-draw::dat::landscape::the_far_landscape_and_the_frills_reach_as_far_as_set",
        tier: Tier::Dat,
    },
    behaviour! {
        id: "draw.world.the-far-landscape-thins-out-with-distance",
        says: "The far landscape's squares lie 1, 2, 2, 4, 4, 8, 8 or 16 vertices apart by how many rings past the \
               detailed blocks a block stands, and a block next to a coarser one fills the gap along their edge.",
        since: RETAIL,
        evidence: Evidence::Private("AC2-EVID-FAR-STRIDE"),
        station: "auberean-draw::dat::landscape::the_far_landscape_thins_out_with_distance",
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
    behaviour! {
        id: "draw.world.the-wind-moves-a-placed-object-that-plays",
        says: "The world's wind blows on the body a placed object plays through, as on any other: a tree that plays travels further in a gale than in still air.",
        since: RETAIL,
        evidence: Evidence::Private("AC2-EVID-PLACED-WIND"),
        station: "auberean-draw::dat::placed::the_wind_moves_a_placed_tree_further_than_still_air_does",
        tier: Tier::Dat,
    },
    behaviour! {
        id: "draw.world.what-lies-on-the-ground-crosses-holes-in-it",
        says: "What is laid on the ground, a disc or a pool of light, takes every square of the land at the land's height, holes included.",
        since: RETAIL,
        evidence: Evidence::Private("AC2-EVID-DECAL-OVER-HOLES"),
        station: "auberean-draw::dat::landscape::what_lies_on_the_ground_crosses_holes_in_it",
        tier: Tier::Dat,
    },
    behaviour! {
        id: "draw.world.what-stands-on-the-land-takes-the-lands-shadow",
        says: "What stands on the land and is not seen through is drawn dark by the share of \n               the sun the land lets through at its spot.",
        since: RETAIL,
        evidence: Evidence::Private("AC2-EVID-DRAW-LAND-SHADOW"),
        station: "auberean-draw::dat::landscape::what_stands_on_the_land_takes_the_lands_shadow",
        tier: Tier::Dat,
    },
    behaviour! {
        id: "draw.world.with-blending-off-a-square-shows-one-ground",
        says: "With the land's blending off a square shows one ground and no masked layer.",
        since: RETAIL,
        evidence: Evidence::Private("AC2-EVID-LANDSCAPE-BLENDING-OFF"),
        station: "auberean-draw::dat::landscape::with_blending_off_a_square_shows_one_ground",
        tier: Tier::Dat,
    },
];
