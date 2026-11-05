//! The world: the landscape, its looks, the sky, light and fog, and placed objects.
//!
//! A row claims what the September 2013 client computes for the world from its data files.

use super::{Behaviour, Evidence, Tier, RETAIL, TOOLING};

/// This subject's rows, in id order.
pub static ROWS: &[Behaviour] = &[
    behaviour! {
        id: "world.calendar.a-clock-with-no-calendar-is-one-long-day",
        says: "A clock with no calendar is one 100,000 second day in a one-day year, and has no date \
               to write.",
        since: RETAIL,
        evidence: Evidence::Private("AC2-EVID-CQUALITY-CLOCK-DEFAULT"),
        station: "auberean-world::cpu::clock::a_clock_with_no_calendar_is_one_long_day",
        tier: Tier::Cpu,
    },
    behaviour! {
        id: "world.calendar.a-region-runs-by-its-skys-calendar",
        says: "The world's region runs by its sky's calendar; a region number with no calendar keeps \
               the clock it had, or one with no calendar.",
        since: RETAIL,
        evidence: Evidence::Private("AC2-EVID-CQUALITY-CLOCK-REGION"),
        station: "auberean-client-runtime::corpus::calls::a_region_runs_by_its_skys_calendar",
        tier: Tier::Corpus,
    },
    behaviour! {
        id: "world.calendar.a-stamps-date-takes-the-present-year-and-day",
        says: "A stamp's date takes the present year, day and season, as the client's does; only the \
               part of the day comes from the stamp.",
        since: RETAIL,
        evidence: Evidence::Private("AC2-EVID-CQUALITY-CLOCK-STAMP"),
        station: "auberean-world::cpu::clock::a_stamps_date_takes_the_present_year_day_and_season",
        tier: Tier::Cpu,
    },
    behaviour! {
        id: "world.calendar.dilating-time-keeps-the-time-of-day",
        says: "Dilating time keeps the time of day where it was and runs it at the new rate; a rate \
               at or below 0.0002 is refused.",
        since: RETAIL,
        evidence: Evidence::Private("AC2-EVID-CQUALITY-CLOCK-DILATE"),
        station: "auberean-world::cpu::clock::dilating_time_keeps_the_time_of_day_and_runs_it_at_the_rate",
        tier: Tier::Cpu,
    },
    behaviour! {
        id: "world.calendar.moving-the-time-of-day-goes-straight-there",
        says: "Moving the time of day goes straight to the value and asks for the land to be lit; a \
               world offset below zero wraps forward a day.",
        since: RETAIL,
        evidence: Evidence::Private("AC2-EVID-CQUALITY-CLOCK-MOVE"),
        station: "auberean-world::cpu::clock::moving_the_time_of_day_goes_straight_there_and_asks_for_light",
        tier: Tier::Cpu,
    },
    behaviour! {
        id: "world.calendar.the-running-clock-updates-at-a-change-of-the-part-of-the-day",
        says: "The running clock works out the year, day, season and part of the day at a change of \
               the part only; between changes only the time of day moves.",
        since: RETAIL,
        evidence: Evidence::Private("AC2-EVID-CQUALITY-CLOCK-RUN"),
        station: "auberean-world::cpu::clock::the_running_clock_updates_at_a_change_of_the_part_of_the_day",
        tier: Tier::Cpu,
    },
    behaviour! {
        id: "world.chargen.a-2017-creations-values-are-stops",
        says: "Every physique value of a creation the 2017 client sent is one of its race and sex's \
               stops for the type, and the types the race and sex lack were sent as 0.",
        since: RETAIL,
        evidence: Evidence::Private("AC2-EVID-CHARGEN-CAPTURE"),
        station: "auberean-world::dat::chargen::a_2017_creations_values_are_stops",
        tier: Tier::Dat,
    },
    behaviour! {
        id: "world.chargen.a-new-avatars-defaults-are-what-the-scripts-set",
        says: "The character generator's scripts, given a creation's race and sex, set its class,                implement and starting area to the race's, every type it has to one of its stops,                build and height to the middle stop, and a type it lacks to 0, as the client's own                creation does.",
        since: RETAIL,
        evidence: Evidence::Private("AC2-EVID-WORLD-CHARGEN-DEFAULTS"),
        station: "auberean-world::dat::chargen::a_new_avatars_defaults_are_what_the_scripts_set",
        tier: Tier::Dat,
    },
    behaviour! {
        id: "world.chargen.a-new-look-is-picked-after-whatever-drew-before",
        says: "A new character's look is picked from the client's one generator, never seeded, after \
               whatever the rest of the program drew from it: with nothing drawn before every run \
               picks the same look, and each draw elsewhere moves the look picked next.",
        since: RETAIL,
        evidence: Evidence::Private("AC2-EVID-CHARGEN-SHARED-RANDOM"),
        station: "auberean-world::cpu::chargen::a_new_look_is_picked_after_whatever_drew_before",
        tier: Tier::Cpu,
    },
    behaviour! {
        id: "world.chargen.a-race-and-sexs-choices-and-their-stops",
        says: "A race and sex's physique types and their stops: build and height every 0.01 between \
               the race and sex's bounds, the others their appearance table key's change-set \
               intensities; a new avatar's choices are stops with build and height at the middle, and \
               its look wears its garments.",
        since: RETAIL,
        evidence: Evidence::Private("AC2-EVID-CHARGEN-STOPS"),
        station: "auberean-world::dat::chargen::a_race_and_sexs_choices_and_their_stops",
        tier: Tier::Dat,
    },
    behaviour! {
        id: "world.chargen.a-slider-stops-at-its-values",
        says: "A slider of n stops stands at k/(n-1) for stop k and maps a position back to the stop \
               (n-1) times the position truncated after a nudge of 0.000025; build and height stop \
               every 0.01; a new avatar's height and build start at the middle stop.",
        since: RETAIL,
        evidence: Evidence::Private("AC2-EVID-CHARGEN-SLIDER"),
        station: "auberean-world::cpu::chargen::a_slider_stops_at_its_values",
        tier: Tier::Cpu,
    },
    behaviour! {
        id: "world.chargen.randomize-picks-a-pages-types-among-their-stops",
        says: "Randomizing picks each type of the page among its stops with the session's random \
               numbers and leaves other pages' types alone; a type the avatar lacks becomes 0; all \
               thirteen types are sent in order.",
        since: RETAIL,
        evidence: Evidence::Private("AC2-EVID-CHARGEN-RANDOM"),
        station: "auberean-world::cpu::chargen::randomize_picks_a_pages_types_among_their_stops",
        tier: Tier::Cpu,
    },
    behaviour! {
        id: "world.chargen.the-choices-look-is-the-body-scaled-with-its-values",
        says: "The look of the choices is the body's description scaled by build times height across \
               and height up, with each type's value in its appearance table among the whole avatar's \
               changes, a garment also worn.",
        since: RETAIL,
        evidence: Evidence::Private("AC2-EVID-CHARGEN-LOOK"),
        station: "auberean-world::cpu::chargen::the_choices_look_is_the_body_scaled_with_its_values",
        tier: Tier::Cpu,
    },
    behaviour! {
        id: "world.chargen.the-native-creation-answers-as-the-scripts-do",
        says: "The character generator's scripts, run on the September 2013 library and saved \
               states, and the client's own creation give the same answers over the whole data: \
               the ten races and sexes, their bodies, the valid values of every race, sex and \
               physique type (none for a type it lacks), each type's label, the starting area of \
               every race and class, and the player's avatar entity.",
        since: RETAIL,
        evidence: Evidence::Private("AC2-EVID-WORLD-CHARGEN-SCRIPTS"),
        station: "auberean-world::dat::chargen::the_native_creation_answers_as_the_character_generators_scripts_do",
        tier: Tier::Dat,
    },
    behaviour! {
        id: "world.chargen.with-no-word-filter-the-scripts-leave-a-name-alone",
        says: "The end-of-retail files hold no word filter, so the character generator's scripts \
               leave a typed name's written form as it was and answer that nothing changed.",
        since: RETAIL,
        evidence: Evidence::Private("AC2-EVID-WORLD-CHARGEN-NAME"),
        station: "auberean-world::dat::chargen::with_no_word_filter_the_scripts_leave_a_name_alone",
        tier: Tier::Dat,
    },
    behaviour! {
        id: "world.color.a-packed-color-is-alpha-red-green-blue",
        says: "A packed color word is read as alpha in its top byte, then red, green and blue, each byte \
               over 255.",
        since: RETAIL,
        evidence: Evidence::Private("AC2-EVID-WORLD-COLOR-PACKED"),
        station: "auberean-world::cpu::color::a_packed_color_is_alpha_red_green_blue",
        tier: Tier::Cpu,
    },
    behaviour! {
        id: "world.color.angles-interpolate-the-short-way-round",
        says: "Hues, sun headings and pitches interpolate between two times of day the short way round the \
               circle.",
        since: RETAIL,
        evidence: Evidence::Private("AC2-EVID-WORLD-COLOR-ANGLE"),
        station: "auberean-world::cpu::color::angles_interpolate_the_short_way_round",
        tier: Tier::Cpu,
    },
    behaviour! {
        id: "world.color.hue-saturation-and-brightness-round-trip",
        says: "Colors convert to hue in sextants, saturation and brightness, and back, as the landscape's \
               tints and the day's colors are computed.",
        since: RETAIL,
        evidence: Evidence::Private("AC2-EVID-WORLD-COLOR-HSB"),
        station: "auberean-world::cpu::color::hue_saturation_and_brightness_round_trip",
        tier: Tier::Cpu,
    },
    behaviour! {
        id: "world.fog.a-blocks-corners-blend-their-fog-over-the-days",
        says: "Where any corner of the viewer's block has a fog kind with fog, the light and fog are \
               the four corners' blend by the viewer's place, each corner's kind combined with the \
               day's channel by channel; elsewhere the day's.",
        since: RETAIL,
        evidence: Evidence::Private("AC2-EVID-WORLD-FOG-BLOCKS"),
        station: "auberean-world::cpu::fog::a_blocks_corners_blend_their_fog_over_the_days",
        tier: Tier::Cpu,
    },
    behaviour! {
        id: "world.fog.the-days-light-is-scaled-only-past-its-range",
        says: "The day's light as a fog kind takes it, sun and ambient added, stays as it is up to                two ten-thousandths past full; past that it is scaled so its brightest channel is                full.",
        since: RETAIL,
        evidence: Evidence::Private("AC2-EVID-WORLD-LIGHT-RANGE"),
        station: "auberean-world::cpu::fog::the_days_light_is_scaled_only_past_its_range",
        tier: Tier::Cpu,
    },
    behaviour! {
        id: "world.fog.under-water-the-fog-is-the-waters-by-depth",
        says: "Under water the fog is the water's, the light the same: the water's table of depths, \
               each combined with the light above as a fog kind is, blended between the last depth \
               deeper than the viewer and the one before it (past the deepest, the deepest alone).",
        since: RETAIL,
        evidence: Evidence::Private("AC2-EVID-WORLD-WATER-FOG"),
        station: "auberean-world::cpu::fog::under_water_the_fog_is_the_waters_by_depth",
        tier: Tier::Cpu,
    },
    behaviour! {
        id: "world.hash.the-position-hash-matches-its-transcription",
        says: "The world's position hash, from which the landscape's splits, grounds and tints are chosen, \
               gives the same numbers as an independent transcription of its expression.",
        since: RETAIL,
        evidence: Evidence::Private("AC2-EVID-WORLD-HASH"),
        station: "auberean-world::cpu::hash::the_position_hash_matches_an_independent_transcription",
        tier: Tier::Cpu,
    },
    behaviour! {
        id: "world.inside.a-carried-light-reaches-draws-on-its-side-within-its-range",
        says: "A light an object carries reaches a draw when its most range and the draw's reach \
               span the distance, taken across the blocks between their cells: a point light only \
               for a draw on its side of inside and out, a sky light only for a draw inside.",
        since: RETAIL,
        evidence: Evidence::Private("AC2-EVID-WORLD-CARRIED-LIGHTS"),
        station: "auberean-world::cpu::inside::a_carried_light_reaches_draws_on_its_side_within_its_range",
        tier: Tier::Cpu,
    },
    behaviour! {
        id: "world.inside.a-cells-record-lights-are-baked-as-they-stand-at-that-moment",
        says: "The lights a cell's record lists for its vertices are baked once, as the cell is first                made ready, each at its intensity of that moment (a noise intensity rolled afresh for                each vertex): a flickering light's bake keeps the value it had then while the sun and                the kept lights are lit on top of it again with each change of the day.",
        since: RETAIL,
        evidence: Evidence::Private("AC2-EVID-WORLD-CELL-BAKE"),
        station: "auberean-world::cpu::inside::a_cells_record_lights_are_baked_as_they_stand_at_that_moment",
        tier: Tier::Cpu,
    },
    behaviour! {
        id: "world.inside.a-dim-suns-share-falls-with-its-brightness",
        says: "The share of the sun a cell's nearest stored direction takes is its product with \
               the light, whose length is the sun's brightness, and the next direction takes the \
               rest.",
        since: RETAIL,
        evidence: Evidence::Private("AC2-EVID-WORLD-SUN-SHARE"),
        station: "auberean-world::dat::inside::a_dim_suns_nearest_direction_takes_a_share_scaled_by_its_brightness",
        tier: Tier::Dat,
    },
    behaviour! {
        id: "world.inside.a-draw-keeps-the-lights-that-weigh-most-there",
        says: "A light weighs how far within its range a draw stands, times the most its intensity \
               reaches, times its color's mean, held to 0..1; a sky light's color is the sun's alone \
               times the sun's brightness, the ambient light outside not weighing. A draw keeps at \
               most six lights whose most range reaches it, the heaviest first.",
        since: RETAIL,
        evidence: Evidence::Private("AC2-EVID-INSIDE-KEPT-LIGHTS"),
        station: "auberean-world::dat::inside::a_draw_keeps_the_lights_that_weigh_most_there",
        tier: Tier::Dat,
    },
    behaviour! {
        id: "world.inside.a-lights-packed-list-reads-as-the-run-length-form",
        says: "A light's packed list of lit vertices reads as the run-length form: single \
               indices and runs in order, and a run whose length has its top bit set gives \
               nothing.",
        since: RETAIL,
        evidence: Evidence::Private("AC2-EVID-WORLD-LIT-LIST-RLE"),
        station: "auberean-world::cpu::inside::a_lights_packed_list_reads_as_the_run_length_form",
        tier: Tier::Cpu,
    },
    behaviour! {
        id: "world.inside.a-lit-particle-takes-the-first-five-lights-kept",
        says: "A lit particle takes the first five lights its system keeps: a point light its color \
               times its intensity over 1 + 10 d squared / R squared, a directional light the same \
               not fading, a sky light nothing though it takes one of the five; their sum lights it.",
        since: RETAIL,
        evidence: Evidence::Private("AC2-EVID-PARTICLE-LIGHTS-RULE"),
        station: "auberean-world::cpu::inside::a_lit_particle_takes_the_first_five_lights_kept",
        tier: Tier::Cpu,
    },
    behaviour! {
        id: "world.inside.carried-lights-join-the-lights-a-draw-keeps-by-their-weight",
        says: "The lights objects carry join the lights a draw keeps when they reach it, all sorted                by their weight where the draw stands, strongest first; a draw none reaches keeps                its own in their order. The device and lit particles take a carried light at its                place in the draw's block with its color times its intensity as it stands.",
        since: RETAIL,
        evidence: Evidence::Private("AC2-EVID-WORLD-CARRIED-LIGHTS-KEPT"),
        station: "auberean-world::cpu::inside::carried_lights_join_the_lights_a_draw_keeps_by_their_weight",
        tier: Tier::Cpu,
    },
    behaviour! {
        id: "world.inside.no-cell-holds-a-light-of-its-own",
        says: "No interior cell of the data holds a light of its own as an entity: every entity placed \n               in a cell is an object.",
        since: RETAIL,
        evidence: Evidence::Private("AC2-EVID-WORLD-CELL-NO-LIGHT"),
        station: "auberean-world::dat::inside::no_cell_holds_a_light_entity",
        tier: Tier::Dat,
    },
    behaviour! {
        id: "world.inside.the-halls-light-and-a-cells-lit-vertices",
        says: "Inside there is no sun overhead: a block's interior ambient light and linear interior \
               fog come from its information, its point and sky lights from its light record; a \
               cell's vertices start black and take each light listing them (color times intensity, \
               times how squarely it meets them, over 1 + 10 d squared / R squared), the kept lights \
               about the cell lighting every vertex, never past white; each listed light leans the \
               direction the vertex's normal map is lit from toward itself in the cell's own frame, \
               and the kept lights lean it toward themselves.",
        since: RETAIL,
        evidence: Evidence::Private("AC2-EVID-INSIDE-LIGHT"),
        station: "auberean-world::dat::inside::the_halls_light_and_a_cells_lit_vertices",
        tier: Tier::Dat,
    },
    behaviour! {
        id: "world.inside.the-sun-lights-the-vertices-its-directions-list",
        says: "When a block's light record carries sunlight, the sun adds its color, by how squarely \
               it meets the surface, times its brightness, to the cell vertices its nearest of nine \
               stored directions lists, times the cosine between its way and that direction, and to \
               the second nearest's, times one less that cosine; vertices no direction lists take \
               none of it.",
        since: RETAIL,
        evidence: Evidence::Private("AC2-EVID-INSIDE-SUNLIGHT"),
        station: "auberean-world::dat::inside::the_sun_lights_the_vertices_its_directions_list",
        tier: Tier::Dat,
    },
    behaviour! {
        id: "world.land.a-circle-reaches-a-polygon-by-its-edges-values",
        says: "The land's slope check takes a circle to reach a triangle of ground by the \
               triangle's edge values, which grow with an edge's length: a circle about a point a \
               meter outside a long edge does not reach it though its radius is longer, one about \
               a point a hand's width outside does, and a corner within the radius always does.",
        since: RETAIL,
        evidence: Evidence::Private("AC2-EVID-LAND-CIRCLE"),
        station: "auberean-world::cpu::land::a_circle_reaches_a_polygon_by_its_edges_values",
        tier: Tier::Cpu,
    },
    behaviour! {
        id: "world.land.a-landscape-record-out-of-range-is-refused",
        says: "Landscape constants whose cells are not a power of two from 2 to 16 a block side, or whose \
               vertex fields need more than 32 bits, are refused.",
        since: RETAIL,
        evidence: Evidence::Private("AC2-EVID-WORLD-LAND-RANGE"),
        station: "auberean-world::cpu::land::a_landscape_record_out_of_range_is_refused",
        tier: Tier::Cpu,
    },
    behaviour! {
        id: "world.land.a-point-within-the-band-of-an-edge-is-over-the-polygon",
        says: "A point counts as over a triangle of ground when it lies inside every edge or no \
               more than a hair outside one, the hair measured in the edge's own length.",
        since: RETAIL,
        evidence: Evidence::Private("AC2-EVID-LAND-OVER"),
        station: "auberean-world::cpu::land::a_point_within_the_band_of_an_edge_is_over_the_polygon",
        tier: Tier::Cpu,
    },
    behaviour! {
        id: "world.land.the-landscape-record-gives-the-grid-fields-and-heights",
        says: "The landscape constants give 160-meter landblocks of 17 by 17 vertices ten meters apart, the \
               vertex word's six fields one after another, and a height table bounded by the sky \
               height less the tallest object.",
        since: RETAIL,
        evidence: Evidence::Private("AC2-EVID-WORLD-LAND-CONSTANTS"),
        station: "auberean-world::cpu::land::the_landscape_record_gives_the_grid_fields_and_heights",
        tier: Tier::Cpu,
    },
    behaviour! {
        id: "world.look.a-combined-description-keeps-the-whole-objects-values",
        says: "Changing an object to a stored description keeps the values its total gave every \
               group: gathered (the last table first), its own set to the record with the effects' \
               values laid over, and the gathered values added back.",
        since: RETAIL,
        evidence: Evidence::Private("AC2-EVID-LOOK-COMBINE"),
        station: "auberean-world::cpu::look_edit::a_combined_description_keeps_the_whole_objects_values",
        tier: Tier::Cpu,
    },
    behaviour! {
        id: "world.look.a-description-chain-puts-together-root-first",
        says: "A description chain puts together root first: a table a later description sets \
               overrides, the scales multiply, a group keeps the setup of the first description that \
               has it, a child hangs from its parent's connection point, and a later description's \
               value for an appearance key replaces an earlier one's.",
        since: RETAIL,
        evidence: Evidence::Private("AC2-EVID-LOOK-CHAIN"),
        station: "auberean-world::cpu::look::a_description_chain_puts_together_root_first",
        tier: Tier::Cpu,
    },
    behaviour! {
        id: "world.look.a-description-put-in-place-drops-the-groups-it-does-not-name",
        says: "A description put in place of one extending the same description is added to it and \
               every group it did not add to or merge into goes (the root found again); one \
               extending another description is copied in whole.",
        since: RETAIL,
        evidence: Evidence::Private("AC2-EVID-LOOK-REPLACE"),
        station: "auberean-world::cpu::look_edit::a_description_put_in_place_drops_the_groups_it_does_not_name",
        tier: Tier::Cpu,
    },
    behaviour! {
        id: "world.look.a-description-taken-out-takes-its-values-and-bare-groups",
        says: "A description taken out of another takes off the values it gives every group, then \
               for each of its groups the group's values (each as one value comes off), or, for a \
               group with neither values nor effects, the group and every group hanging from it.",
        since: RETAIL,
        evidence: Evidence::Private("AC2-EVID-LOOK-SUBTRACT"),
        station: "auberean-world::cpu::look_edit::a_description_taken_out_takes_its_values_and_bare_groups",
        tier: Tier::Cpu,
    },
    behaviour! {
        id: "world.look.a-mesh-change-matches-the-mesh-the-part-has-by-then",
        says: "A part's mesh goes through the mesh changes in order, each applying when its kind is \
               the kind of the mesh the part has by then, or every kind; a change of kind 0 does \
               nothing.",
        since: RETAIL,
        evidence: Evidence::Private("AC2-EVID-LOOK-MESH"),
        station: "auberean-world::cpu::look::a_mesh_change_matches_the_mesh_the_part_has_by_then",
        tier: Tier::Cpu,
    },
    behaviour! {
        id: "world.look.a-player-in-his-clothes-puts-together-as-sent",
        says: "A human man as a 2017 server described him puts together as the human body's groups \
               plus the group his helmet hangs from the head; his worn garments swap the body's, \
               head's and helmet's meshes by kind; his skin, hair, markings and the helmet's dye \
               change materials, the dye with a replacement material.",
        since: RETAIL,
        evidence: Evidence::Private("AC2-EVID-LOOK-PLAYER"),
        station: "auberean-world::dat::looks::a_player_in_his_clothes_puts_together_as_sent",
        tier: Tier::Dat,
    },
    behaviour! {
        id: "world.look.a-taken-connection-point-fails-the-description",
        says: "A group hung at a connection point its parent already holds another group at fails the \
               whole description.",
        since: RETAIL,
        evidence: Evidence::Private("AC2-EVID-LOOK-CONNECTION"),
        station: "auberean-world::cpu::look::a_taken_connection_point_fails_the_description",
        tier: Tier::Cpu,
    },
    behaviour! {
        id: "world.look.a-value-for-a-missing-group-goes-to-a-copy-of-the-parents",
        says: "An appearance value for a part group an object's own description lacks goes to a \
               copy of the extended description's group (its key, connection point and parent key \
               only, hung as a group is added); the own groups hanging from it there are marked as \
               hanging from the copy. A group or table of 0 takes no value.",
        since: RETAIL,
        evidence: Evidence::Private("AC2-EVID-LOOK-EDIT-ADD"),
        station: "auberean-world::cpu::look_edit::a_value_for_a_missing_group_goes_to_a_copy_of_the_parents",
        tier: Tier::Cpu,
    },
    behaviour! {
        id: "world.look.a-value-only-the-parent-gives-is-overridden-to-come-off",
        says: "A value taken off comes off the own description's group (a table left with no keys \
               goes); a value only the extended description gives is overridden with -1, which \
               applies nothing; one neither gives is not taken off.",
        since: RETAIL,
        evidence: Evidence::Private("AC2-EVID-LOOK-EDIT-REMOVE"),
        station: "auberean-world::cpu::look_edit::a_value_only_the_parent_gives_is_overridden_to_come_off",
        tier: Tier::Cpu,
    },
    behaviour! {
        id: "world.look.a-value-picks-the-change-set-it-reaches",
        says: "The Shelter Guard's description sets 0.5 for a key whose change sets stand at 0, 0.5 \
               and 0.75: the set at 0.5 applies, one material modifier on every mesh for one material \
               kind.",
        since: RETAIL,
        evidence: Evidence::Private("AC2-EVID-LOOK-GUARD"),
        station: "auberean-world::dat::looks::a_value_picks_the_change_set_it_reaches",
        tier: Tier::Dat,
    },
    behaviour! {
        id: "world.look.a-value-picks-the-last-change-set-not-above-it",
        says: "An appearance value picks the last change set of its key whose intensity is not above \
               it; a value below the first set, or a taken-off value (-1), applies nothing.",
        since: RETAIL,
        evidence: Evidence::Private("AC2-EVID-LOOK-VALUE"),
        station: "auberean-world::cpu::look::a_value_picks_the_last_change_set_not_above_it",
        tier: Tier::Cpu,
    },
    behaviour! {
        id: "world.look.a-whole-object-change-swaps-a-part-group",
        says: "A whole-object value's part-group change replaces the setup of the root's child of its \
               key, or adds the group when the root has no such child.",
        since: RETAIL,
        evidence: Evidence::Private("AC2-EVID-LOOK-SWAP"),
        station: "auberean-world::cpu::look::a_whole_object_change_swaps_a_part_group",
        tier: Tier::Cpu,
    },
    behaviour! {
        id: "world.look.an-added-entry-goes-first-in-its-bucket",
        says: "A keyed table is walked bucket by bucket (the key modulo the bucket count), newest \
               first in a bucket: an added key goes before its bucket's older keys; setting a key \
               keeps its place.",
        since: RETAIL,
        evidence: Evidence::Private("AC2-EVID-LOOK-TABLE-ORDER"),
        station: "auberean-world::cpu::look::an_added_entry_goes_first_in_its_bucket",
        tier: Tier::Cpu,
    },
    behaviour! {
        id: "world.look.an-appearance-tables-keys-steps-and-counts",
        says: "An appearance table has a key when it or one of its base tables (searched first) \
               has it; a key's steps are its change sets' intensities, the bases' first, each put \
               in order after the equal ones; its step count is the table's own.",
        since: RETAIL,
        evidence: Evidence::Private("AC2-EVID-LOOK-TABLE-QUERIES"),
        station: "auberean-world::cpu::look_edit::an_appearance_tables_keys_steps_and_counts",
        tier: Tier::Cpu,
    },
    behaviour! {
        id: "world.look.an-effects-appearance-is-laid-over-the-objects-own",
        says: "An object keeps the appearance values its effects apply apart and lays them over \
               its own description, again whenever its own is set; an effect's value that comes \
               off comes off both.",
        since: RETAIL,
        evidence: Evidence::Private("AC2-EVID-LOOK-EFFECTS"),
        station: "auberean-world::cpu::look_edit::an_effects_appearance_is_laid_over_the_objects_own",
        tier: Tier::Cpu,
    },
    behaviour! {
        id: "world.look.an-icon-atom-replaces-one-of-its-layer-and-kind",
        says: "An icon atom replaces the first gathered of its layer whose picture and color are \
               each there or missing as its own (the replaced one goes, the new one goes last); an \
               object's icon is its chain's icons with the atoms laid over in that order.",
        since: RETAIL,
        evidence: Evidence::Private("AC2-EVID-LOOK-ICON-ATOMS"),
        station: "auberean-world::cpu::look_edit::an_icon_atom_replaces_one_of_its_layer_and_kind",
        tier: Tier::Cpu,
    },
    behaviour! {
        id: "world.look.an-objects-own-copy-draws-as-the-description-it-was-given",
        says: "An object's own description is a copy, and a copy turns each table's buckets \
               around: 87 of the 6,615 stored descriptions put together in another order through a \
               copy and 71 gather their atoms in another order; all but two draw the same \
               templates, modifiers, meshes and opacity as the stored one.",
        since: RETAIL,
        evidence: Evidence::Private("AC2-EVID-LOOK-OWN-COPY"),
        station: "auberean-world::dat::looks::an_objects_own_copy_draws_as_the_description_it_was_given",
        tier: Tier::Dat,
    },
    behaviour! {
        id: "world.look.base-tables-apply-first",
        says: "A table's base tables apply before its own changes: an own mesh change of the same \
               kind at an equal or higher priority replaces the base's, at a lower priority it does \
               not.",
        since: RETAIL,
        evidence: Evidence::Private("AC2-EVID-LOOK-BASE"),
        station: "auberean-world::cpu::look::base_tables_apply_first",
        tier: Tier::Cpu,
    },
    behaviour! {
        id: "world.look.material-changes-replace-by-priority-on-their-kind",
        says: "A material change replaces those of lower priority on the same material kind and the \
               same mesh kind (or every mesh kind when it names every mesh kind); equal priorities \
               both stay, in order; a lower priority is not added.",
        since: RETAIL,
        evidence: Evidence::Private("AC2-EVID-LOOK-ATOMS"),
        station: "auberean-world::cpu::look::material_changes_replace_by_priority_on_their_kind",
        tier: Tier::Cpu,
    },
    behaviour! {
        id: "world.objects.every-placed-object-resolves-to-meshes",
        says: "Every object a landblock places in the 2013 world resolves through its visual descriptions \
               and setups to meshes, and every one of those meshes reads.",
        since: RETAIL,
        evidence: Evidence::Private("AC2-EVID-WORLD-OBJECTS-PARTS"),
        station: "auberean-world::dat::entities::every_placed_object_resolves_to_meshes",
        tier: Tier::Dat,
    },
    behaviour! {
        id: "world.place.a-weighted-setup-draws-every-part-at-its-frame",
        says: "When any of a setup's meshes is weighted to bones, every part of it is drawn at its \
               group's frame, the unweighted ones too; otherwise each part follows the bone of its index.",
        since: RETAIL,
        evidence: Evidence::Private("AC2-EVID-PLACE-WEIGHTED-PARTS"),
        station: "auberean-world::cpu::look::a_weighted_setup_draws_every_part_at_its_frame",
        tier: Tier::Cpu,
    },
    behaviour! {
        id: "world.place.a-weighted-tree-stands-its-trunk-at-its-root",
        says: "A coastal landblock's trees whose canopy is weighted to bones place their trunk, a mesh \
               that only collides, where the tree stands, not on the bone 7.8 m up that their resting \
               pose gives it.",
        since: RETAIL,
        evidence: Evidence::Private("AC2-EVID-PLACE-WEIGHTED-TREE"),
        station: "auberean-world::dat::entities::a_weighted_tree_stands_its_trunk_at_its_root",
        tier: Tier::Dat,
    },
    behaviour! {
        id: "world.region.every-record-a-description-names-is-there",
        says: "The region and its descriptions, days and scenes name the records they need (maps, materials, pictures, days, the calendar, scenes, sky models, objects); every one is in the 2013 data but the world map's picture.",
        since: RETAIL,
        evidence: Evidence::Private("AC2-EVID-WORLD-REGION-NAMED"),
        station: "auberean-world::dat::named::every_record_a_description_names_is_there",
        tier: Tier::Dat,
    },
    behaviour! {
        id: "world.scenery.a-group-is-kept-off-the-edge-and-the-road",
        says: "A scenery group places nothing when the position hash misses its frequency, when it \
               would stand nearer its block's edge than its clearance, or on a road.",
        since: RETAIL,
        evidence: Evidence::Private("AC2-EVID-WORLD-SCENERY-REFUSED"),
        station: "auberean-world::cpu::scenery::a_group_is_kept_off_the_edge_and_the_road",
        tier: Tier::Cpu,
    },
    behaviour! {
        id: "world.scenery.a-group-stands-at-its-offset-turned-on-the-ground",
        says: "A scenery group stands at its hashed offset from its vertex, on the ground there, \
               turned clockwise from north by its hashed rotation, and its objects stand with it.",
        since: RETAIL,
        evidence: Evidence::Private("AC2-EVID-WORLD-SCENERY-PLACE"),
        station: "auberean-world::cpu::scenery::a_group_stands_at_its_offset_turned_on_the_ground",
        tier: Tier::Cpu,
    },
    behaviour! {
        id: "world.scenery.a-vertex-grows-its-terrain-kinds-scene",
        says: "A landscape vertex grows the scene its terrain kind's scenery gives for its scene \
               field, picked among that scenery kind's scenes by frequency with a position hash.",
        since: RETAIL,
        evidence: Evidence::Private("AC2-EVID-WORLD-SCENERY-SCENE"),
        station: "auberean-world::cpu::scenery::a_vertex_grows_its_terrain_kinds_scene",
        tier: Tier::Cpu,
    },
    behaviour! {
        id: "world.scenery.every-terrain-kind-has-a-barren-field",
        says: "Every terrain kind has a scene field under which it grows nothing: a block whose \
               vertices all hold it has no scenery and no frills.",
        since: TOOLING,
        evidence: Evidence::Private("AC2-EVID-SCENERY-BARREN"),
        station: "auberean-world::dat::clearance::a_block_of_barren_vertices_grows_nothing_at_all",
        tier: Tier::Dat,
    },
    behaviour! {
        id: "world.scenery.placed-objects-keep-their-clearance-boxes-and-cylinders",
        says: "A block's placed object keeps scenery groups from its spot, or, when its clearance properties set boxes or upright cylinders about it, from those, measured in its own frame; a box with no extent and a cylinder with no radius keep nothing clear.",
        since: RETAIL,
        evidence: Evidence::Private("AC2-EVID-WORLD-SCENERY-CLEARANCE"),
        station: "auberean-world::cpu::scenery::placed_objects_keep_their_clearance_boxes_and_cylinders",
        tier: Tier::Cpu,
    },
    behaviour! {
        id: "world.scenery.the-worlds-objects-keep-their-clearance",
        says: "The 2013 world's placed objects read their clearance properties: 3,319 of 20,865 ask for boxes or cylinders to be kept clear, and keep scenery out of them.",
        since: RETAIL,
        evidence: Evidence::Private("AC2-EVID-WORLD-SCENERY-CLEARANCE-DATA"),
        station: "auberean-world::dat::clearance::the_worlds_objects_keep_their_clearance",
        tier: Tier::Dat,
    },
    behaviour! {
        id: "world.sky.a-game-times-date-names-its-season-day-and-part-of-the-day",
        says: "A game time's date is its year with the year's name, the season the day of the year \
               has reached with the day of that season, and the part of the day the day's fraction \
               has reached; a calendar with no seasons or no parts of the day gives none.",
        since: RETAIL,
        evidence: Evidence::Private("AC2-EVID-WORLD-SKY-DATE"),
        station: "auberean-world::cpu::clock::a_game_times_date_names_its_season_day_and_part_of_the_day",
        tier: Tier::Cpu,
    },
    behaviour! {
        id: "world.sky.a-vertex-in-its-hills-shadow-gets-no-sun",
        says: "A terrain vertex is lit by the sun only when its self-shadow bit for the stored sun \
               direction nearest the light's is set; light from below the horizon lights every \
               vertex.",
        since: RETAIL,
        evidence: Evidence::Private("AC2-EVID-WORLD-SELF-SHADOW"),
        station: "auberean-world::cpu::clock::a_vertex_in_its_hills_shadow_gets_no_sun",
        tier: Tier::Cpu,
    },
    behaviour! {
        id: "world.sky.terrain-vertices-are-lit-by-ambient-and-the-sun",
        says: "A terrain vertex is lit by the ambient light plus the sun's share by its normal, times its \
               own color, clamped; a shadowed vertex gets only the ambient.",
        since: RETAIL,
        evidence: Evidence::Private("AC2-EVID-WORLD-SKY-LIT"),
        station: "auberean-world::cpu::clock::terrain_vertices_are_lit_by_ambient_and_the_sun",
        tier: Tier::Cpu,
    },
    behaviour! {
        id: "world.sky.the-day-fraction-counts-from-the-calendars-zero",
        says: "The time of day is how far the game time, counted from the calendar's zero, is through its \
               current day.",
        since: RETAIL,
        evidence: Evidence::Private("AC2-EVID-WORLD-SKY-CLOCK"),
        station: "auberean-world::cpu::clock::the_day_fraction_counts_from_the_calendars_zero",
        tier: Tier::Cpu,
    },
    behaviour! {
        id: "world.sky.the-light-interpolates-between-times-of-day",
        says: "The light, fog and sun between two times of day are interpolated by how far the moment is \
               from one to the next, wrapping past midnight, with the sun's heading clockwise from \
               north.",
        since: RETAIL,
        evidence: Evidence::Private("AC2-EVID-WORLD-SKY-LIGHT"),
        station: "auberean-world::cpu::clock::the_light_interpolates_between_times_of_day",
        tier: Tier::Cpu,
    },
    behaviour! {
        id: "world.sky.the-sky-picks-its-day-by-a-hash-of-the-date",
        says: "Which of the sky's days a calendar day shows is a hash of the date weighed against the days' \
               frequencies.",
        since: RETAIL,
        evidence: Evidence::Private("AC2-EVID-WORLD-SKY-DAY"),
        station: "auberean-world::cpu::clock::the_sky_picks_its_day_by_a_hash_of_the_date",
        tier: Tier::Cpu,
    },
    behaviour! {
        id: "world.sound.a-blocks-points-fall-short-of-its-far-edges",
        says: "A landblock's ambient sound points are its vertices that name a kind, in the                vertices' order, a seventeenth of the block's side apart (the side over the                vertices along it, not the squares), so the last row and column stand short of the                block's far edges, each at its vertex's height.",
        since: RETAIL,
        evidence: Evidence::Private("AC2-EVID-AUDIO-AMBIENT-POINTS"),
        station: "auberean-world::cpu::scenery::a_blocks_points_stand_a_seventeenth_of_its_side_apart_at_their_heights",
        tier: Tier::Cpu,
    },
    behaviour! {
        id: "world.sound.a-vertex-names-its-scenery-kinds-sound",
        says: "A landscape vertex names the ambient sound kind of the scenery kind its terrain kind                gives for its scene field (or the terrain's default scenery; an unlisted terrain kind                takes the default terrain); none for scenery kind 0, a scenery kind not described,                or a sound word of 0.",
        since: RETAIL,
        evidence: Evidence::Private("AC2-EVID-AUDIO-AMBIENT-KINDS"),
        station: "auberean-world::cpu::scenery::a_vertex_names_the_sound_of_its_scenery_kind_and_nothing_past_a_gap",
        tier: Tier::Cpu,
    },
    behaviour! {
        id: "world.sound.every-points-kind-is-described",
        says: "Every vertex of every landblock of the end-of-retail world names an ambient kind                (twelve kinds in all), and the region's ambient sound table describes each.",
        since: RETAIL,
        evidence: Evidence::Private("AC2-EVID-AUDIO-AMBIENT-CENSUS"),
        station: "auberean-world::dat::sound::every_landblocks_points_name_kinds_the_regions_ambient_table_describes",
        tier: Tier::Dat,
    },
    behaviour! {
        id: "world.terrain.a-road-is-a-masked-layer-over-the-ground",
        says: "A road crossing a terrain square is a road layer cut in with the road's own mask over the \
               ground; a road on all four corners replaces the ground; a square never has more \
               than three layers.",
        since: RETAIL,
        evidence: Evidence::Private("AC2-EVID-WORLD-TERRAIN-ROAD"),
        station: "auberean-world::cpu::blend::a_road_is_a_masked_layer_over_the_ground",
        tier: Tier::Cpu,
    },
    behaviour! {
        id: "world.terrain.a-square-blends-its-corners-by-the-blend-table",
        says: "Each ten-meter terrain square draws the ground most of its corners share at the bottom and \
               cuts the others in with corner or side masks turned toward their corners, in up to \
               three layers.",
        since: RETAIL,
        evidence: Evidence::Private("AC2-EVID-WORLD-TERRAIN-BLEND"),
        station: "auberean-world::cpu::blend::a_square_blends_its_corners_by_the_blend_table",
        tier: Tier::Cpu,
    },
    behaviour! {
        id: "world.terrain.a-square-is-split-by-the-hash-of-its-world-position",
        says: "Which diagonal splits each ten-meter terrain square is the position hash of the square's \
               place in the world at the split seed, so neighboring blocks always agree.",
        since: RETAIL,
        evidence: Evidence::Private("AC2-EVID-WORLD-TERRAIN-SPLIT"),
        station: "auberean-world::cpu::hash::the_split_is_the_folded_hash_of_the_global_square",
        tier: Tier::Cpu,
    },
    behaviour! {
        id: "world.terrain.every-vertex-finds-its-ground",
        says: "Every terrain vertex of the 2013 world whose terrain kind names a surface finds a ground for \
               its slope, with a base, a corner mask and a side mask.",
        since: RETAIL,
        evidence: Evidence::Private("AC2-EVID-WORLD-TERRAIN-GROUND"),
        station: "auberean-world::dat::terrain::every_vertex_finds_its_ground",
        tier: Tier::Dat,
    },
    behaviour! {
        id: "world.terrain.neighbouring-landblocks-meet-without-a-crack",
        says: "Every landblock of the 2013 world meets its neighbors with the same heights along the whole \
               shared edge, and the same normals and grounds except at corners beside a missing \
               block.",
        since: RETAIL,
        evidence: Evidence::Private("AC2-EVID-WORLD-TERRAIN-EDGES"),
        station: "auberean-world::dat::terrain::every_landblock_meets_its_neighbours_without_a_crack",
        tier: Tier::Dat,
    },
    behaviour! {
        id: "world.terrain.terrain-heights-are-the-records-heights",
        says: "The built terrain's height at every vertex is the height table's entry for that vertex's \
               byte in the landblock record.",
        since: RETAIL,
        evidence: Evidence::Private("AC2-EVID-WORLD-TERRAIN-HEIGHTS"),
        station: "auberean-world::dat::terrain::terrain_heights_are_the_records_heights",
        tier: Tier::Dat,
    },
    behaviour! {
        id: "world.water.a-block-holds-water-above-its-lowest-ground",
        says: "A landblock holds water when its water kind has water and the surface, 0.95 above \
               its height byte's height, stands above the block's lowest ground.",
        since: RETAIL,
        evidence: Evidence::Private("AC2-EVID-WORLD-WATER-BLOCK"),
        station: "auberean-world::cpu::water::a_block_holds_water_above_its_lowest_ground",
        tier: Tier::Cpu,
    },
    behaviour! {
        id: "world.water.a-sky-object-shows-by-its-water-effect",
        says: "A sky object's water effect word says when it shows: 0 always, 1 only with the camera \
               under water, anything else never.",
        since: RETAIL,
        evidence: Evidence::Private("AC2-EVID-WORLD-SKY-WATER-EFFECT"),
        station: "auberean-world::cpu::water::a_sky_object_shows_by_its_water_effect",
        tier: Tier::Cpu,
    },
    behaviour! {
        id: "world.wave.a-frame-loop-steps-through-its-frames",
        says: "A frame-loop waveform shows its amplitude's frames at its frequency's rate, round                and round, each frame stepping its base on by its phase; with no frames it is its                base.",
        since: RETAIL,
        evidence: Evidence::Private("AC2-EVID-WORLD-WAVE-FRAME-LOOP"),
        station: "auberean-world::cpu::clock::a_frame_loop_steps_through_its_frames",
        tier: Tier::Cpu,
    },
    behaviour! {
        id: "world.wave.a-waveform-takes-its-shape-at-the-moment",
        says: "A waveform at a moment of the client's run is its base, its speed, or its sine,                square or bounce about its base, each of its base, amplitude, phase, frequency and                roughness moving on at its own rate; the phase is in half turns.",
        since: RETAIL,
        evidence: Evidence::Private("AC2-EVID-WORLD-WAVE-SHAPES"),
        station: "auberean-world::cpu::clock::a_waveform_takes_its_shape_at_the_moment",
        tier: Tier::Cpu,
    },
    behaviour! {
        id: "world.wave.the-noise-shape-rolls-the-clients-generator-once",
        says: "A noise waveform rolls the client's one random-number generator once each time it                is taken, between minus and plus its amplitude about its base; no other shape draws                on the generator.",
        since: RETAIL,
        evidence: Evidence::Private("AC2-EVID-WORLD-WAVE-NOISE"),
        station: "auberean-world::cpu::clock::the_noise_shape_rolls_the_clients_generator_once",
        tier: Tier::Cpu,
    },
];
