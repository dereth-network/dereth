//! Rendering -- what is drawn and how: detail, lighting, fog, sky, textures, draw order.
//!
//! One file per subject, so that two changes adding rows at the same time do not edit the same
//! file. [`ROWS`] is in id order; the registry's own test asserts that, and that no id and no
//! evidence handle is repeated anywhere in it.

// `behaviour!` is `#[macro_export]`ed by `mod.rs` above this module's declaration, so it is in
// textual scope here and needs no import.
use super::{Behaviour, Evidence, Tier, RETAIL, THIS_CLIENT};

/// This subject's rows, in id order.
pub static ROWS: &[Behaviour] = &[
    behaviour! {
        id: "rendering.billboards.a-billboarding-part-faces-the-viewer",
        says: "Parts of moving creatures and objects that the art marks to face the viewer turn as \
               the camera moves over them, so the picture differs from one where they are left \
               unturned while every object's animation stays identical.",
        since: RETAIL,
        evidence: Evidence::Private("AC-EVID-O188-BILLBOARDS"),
        station: "dereth-client::gpu::rendering::billboards::a_billboarding_part_turns_and_the_control_arm_does_not",
        tier: Tier::Gpu,
    },
    behaviour! {
        id: "rendering.billboards.a-mode-five-card-turns-with-the-camera-and-a-mode-one-static-never-does",
        says: "As the camera circles a block of scenery, placements marked to turn toward the \
               viewer do turn, flat pictures marked to stand upright stay upright while they turn, \
               and placements marked never to turn keep the same orientation from every bearing.",
        since: RETAIL,
        evidence: Evidence::Private("AC-EVID-O188-BILLBOARDS-MODE"),
        station: "dereth-client::gpu::rendering::billboards::a_mode_five_card_turns_as_the_camera_orbits_and_a_mode_one_static_never_does",
        tier: Tier::Gpu,
    },
    behaviour! {
        id: "rendering.culling.an-object-outside-the-view-cone-is-not-drawn",
        says: "An object standing behind the camera is not drawn at all, so it cannot be drawn as \
               the highlighted selection either, while an object in front of the camera in the \
               same frame is drawn.",
        since: RETAIL,
        evidence: Evidence::Private("AC-EVID-O731-CULLING"),
        station: "dereth-client::gpu::rendering::object_viewcone_cull::with_the_cull_on_the_object_behind_the_camera_is_not_drawn_at_all",
        tier: Tier::Gpu,
    },
    behaviour! {
        id: "rendering.culling.the-view-cone-cull-changes-no-pixel",
        says: "Skipping the objects that fall outside the camera's view cone changes no pixel of \
               the picture at any camera position checked over a replayed recorded session, even \
               though it does skip drawing some of them.",
        since: RETAIL,
        evidence: Evidence::Private("AC-EVID-O784-CULLING"),
        station: "dereth-client::gpu::rendering::object_viewcone_cull::the_cull_changes_no_pixel_at_any_station",
        tier: Tier::Gpu,
    },
    behaviour! {
        id: "rendering.degrade.a-baked-static-draws-the-level-its-distance-selects",
        says: "Every building and piece of scenery in view is drawn at the level of detail its own \
               detail record selects for its distance from the camera, checked placement by \
               placement at three camera heights over Holtburg.",
        since: RETAIL,
        evidence: Evidence::Private("AC-EVID-O93-DEGRADE"),
        station: "dereth-client::gpu::rendering::static_degrade_levels::every_placement_draws_the_level_get_degrade_names_at_three_distances",
        tier: Tier::Gpu,
    },
    behaviour! {
        id: "rendering.degrade.a-creature-stops-drawing-at-its-records-own-distance",
        says: "A creature moved farther and farther from the camera is drawn up to the distance \
               its own level-of-detail record names and is not drawn at all beyond it, the cut \
               falling within a couple of metres of that distance.",
        since: RETAIL,
        evidence: Evidence::Private("AC-EVID-F14-DEGRADE"),
        station: "dereth-client::gpu::rendering::object_draw_distance::a_creature_stops_being_submitted_at_the_distance_the_record_names",
        tier: Tier::Gpu,
    },
    behaviour! {
        id: "rendering.degrade.a-creatures-parts-draw-the-level-their-record-selects",
        says: "Every part of every creature and object in view is drawn at the level of detail its \
               own detail record selects for its distance from the camera, checked part by part at \
               three camera heights over a replayed recorded session.",
        since: RETAIL,
        evidence: Evidence::Private("AC-EVID-O189-DEGRADE"),
        station: "dereth-client::gpu::rendering::part_degrade_levels::every_part_draws_the_level_get_degrade_names_at_three_distances",
        tier: Tier::Gpu,
    },
    behaviour! {
        id: "rendering.degrade.a-far-land-cells-statics-are-drawn-at-the-cells-distance",
        says: "Over Holtburg with the camera 40 m up, every static in an outdoor cell more than \
               50 m away across the ground is measured at that cell's ground distance and level \
               bearing, while nearer statics keep their own distance.",
        since: RETAIL,
        evidence: Evidence::Private("AC-EVID-CELL-DISTANCE-STATICS"),
        station: "dereth-client::gpu::rendering::static_degrade_levels::every_static_in_a_land_cell_more_than_fifty_metres_away_is_measured_at_the_cells_distance",
        tier: Tier::Gpu,
    },
    behaviour! {
        id: "rendering.degrade.a-negative-bias-reads-the-min-and-ideal-distances",
        says: "How much detail an object is drawn with at a distance follows the detail setting: \
               turned below normal, each level's switch-over distance slides between that level's \
               minimum and ideal distances, and turned above normal, between its ideal and maximum \
               distances, for every shipped level-of-detail record.",
        since: RETAIL,
        evidence: Evidence::Private("AC-EVID-O102-DEGRADE"),
        station: "dereth-client::dat::rendering::degrade_bias::every_shipped_record_matches_the_retail_expression_on_both_signs_of_the_bias",
        tier: Tier::Dat,
    },
    behaviour! {
        id: "rendering.degrade.an-objects-parts-share-its-distance-past-the-share-distance",
        says: "With the camera standing just over one creature, that creature's parts are each drawn \
               at their own distance, while every part of a creature or object five metres or more \
               away across the ground is sorted at, and turned toward, the distance and bearing of \
               that object as a whole, or of its outdoor cell when that cell is more than 50 m away \
               across the ground, over a replayed recorded session.",
        since: RETAIL,
        evidence: Evidence::Private("AC-EVID-SHARE-DISTANCE-OBJECT-PARTS"),
        station: "dereth-client::gpu::rendering::object_part_submission::past_the_share_distance_every_part_sorts_and_turns_at_its_objects_own_distance",
        tier: Tier::Gpu,
    },
    behaviour! {
        id: "rendering.degrade.an-older-eras-part-draws-the-levels-its-own-id-reaches",
        says: "A body part from the February 2005 files is drawn with its detail levels, so up close                the torso is the denser model those files list first rather than the coarser model                the body names, on the February 2005 world and in its look on the end-of-retail                world alike.",
        since: RETAIL,
        evidence: Evidence::Private("AC-EVID-PRETOD-DEGRADE-ID"),
        station: "dereth-client::dat::rendering::pre_tod_degrade::a_february_2005_torso_draws_its_records_nearest_level_up_close_on_either_world",
        tier: Tier::Dat,
    },
    behaviour! {
        id: "rendering.degrade.an-older-world-chooses-detail-from-the-raw-distance",
        says: "On a world from before Throne of Destiny another character's parts change detail at                their own distances, within a few metres of the camera, while the end-of-retail                world keeps every part at its nearest detail for the first 50 m.",
        since: RETAIL,
        evidence: Evidence::Private("AC-EVID-PRETOD-DEGRADE-DISTANCE"),
        station: "dereth-client::dat::rendering::pre_tod_degrade::an_older_world_changes_the_torsos_level_at_the_raw_distance_and_the_later_world_50_m_out",
        tier: Tier::Dat,
    },
    behaviour! {
        id: "rendering.degrade.detail-falls-under-load-and-recovers",
        says: "With automatic degrades on, the detail bias climbs to its highest while frames are \
               cheap, is driven to its lowest while they are expensive and climbs back when they \
               are cheap again, stepping down by 0.15 and up by 0.10 at a time, so detail is shed \
               faster than it is restored.",
        since: RETAIL,
        evidence: Evidence::Private("AC-EVID-DEGRADE-DEGRADE"),
        station: "dereth-client::gpu::rendering::adaptive_degrade::detail_falls_back_under_load_and_recovers_when_frames_get_cheap",
        tier: Tier::Gpu,
    },
    behaviour! {
        id: "rendering.degrade.inside-the-share-distance-each-part-measures-itself",
        says: "While the camera is closer to an object across the ground than the share distance, each \
               of its parts takes its detail and facing from its own distance and bearing from the \
               camera; height above the object does not count toward that closeness.",
        since: RETAIL,
        evidence: Evidence::Private("AC-EVID-SHARE-DISTANCE-INSIDE"),
        station: "dereth-world-render::lib::objects::parts::tests::inside_the_share_distance_each_part_measures_its_own_distance_and_heading",
        tier: Tier::Cpu,
    },
    behaviour! {
        id: "rendering.degrade.past-fifty-units-every-object-in-a-land-cell-takes-the-cells-distance",
        says: "Every object standing in an outdoor cell more than 50 m from the camera across the \
               ground chooses its detail at that cell's ground distance and faces along the cell's \
               level bearing, whatever its own distance; nearer cells and interior cells leave each \
               object to measure itself or share its own distance.",
        since: RETAIL,
        evidence: Evidence::Private("AC-EVID-CELL-DISTANCE-FAR"),
        station: "dereth-world-render::lib::objects::parts::tests::past_fifty_units_every_object_in_a_land_cell_takes_the_cells_horizontal_distance_and_heading",
        tier: Tier::Cpu,
    },
    behaviour! {
        id: "rendering.degrade.past-the-share-distance-every-part-takes-the-objects-distance-and-heading",
        says: "Once the camera is at or past the share distance from an object across the ground, every \
               part of it takes the object's own distance and bearing, so two parts that turn to face \
               the viewer turn to the same orientation and sort together.",
        since: RETAIL,
        evidence: Evidence::Private("AC-EVID-SHARE-DISTANCE-PAST"),
        station: "dereth-world-render::lib::objects::parts::tests::past_the_share_distance_every_part_takes_the_objects_distance_and_heading",
        tier: Tier::Cpu,
    },
    behaviour! {
        id: "rendering.degrade.the-degrade-distance-is-subtracted-from-the-viewer-distance",
        says: "The detail level is chosen from the viewer distance less the degrade distance (50 m by \
               default), not from the distance itself once it passes 50 m, so a level whose band \
               begins inside the first 50 m is still drawn farther out.",
        since: RETAIL,
        evidence: Evidence::Private("AC-EVID-DEGRADE-LOOKUP-SUBTRACTS"),
        station: "dereth-world-render::lib::objects::degrade::tests::the_degrade_distance_is_subtracted_not_a_threshold",
        tier: Tier::Cpu,
    },
    behaviour! {
        id: "rendering.degrade.the-local-body-never-degrades",
        says: "The player's own body is always drawn at full detail however far the camera pulls \
               back, even at distances where its parts on anyone else would drop to coarser models \
               and other objects in view do.",
        since: RETAIL,
        evidence: Evidence::Private("AC-EVID-O189-DEGRADE-LOCAL"),
        station: "dereth-client::gpu::rendering::part_degrade_levels::the_local_body_does_not_degrade_however_far_the_chase_camera_pulls_back",
        tier: Tier::Gpu,
    },
    behaviour! {
        id: "rendering.degrade.the-share-distances-start-at-five-and-four-metres",
        says: "Until automatic degrades first change the detail bias, and for good when they are off, \
               objects share their distance with their parts from five metres and particle emitters \
               from four; the first accepted change sets both from the new bias along with the light \
               limits.",
        since: RETAIL,
        evidence: Evidence::Private("AC-EVID-SHARE-DISTANCE-STARTUP"),
        station: "dereth-world-render::lib::degrade_loop::tests::the_share_distances_start_at_five_and_four_metres_until_the_loop_first_moves_the_bias",
        tier: Tier::Cpu,
    },
    behaviour! {
        id: "rendering.degrade.the-shipped-default-is-automatic",
        says: "Automatic degrades are on by default, so the detail bias follows the frame rate; \
               with the Adaptive Degrade option turned off, at start-up or while playing, the bias \
               stays at its fixed manual value whatever frame times the client sees.",
        since: RETAIL,
        evidence: Evidence::Private("AC-EVID-DEGRADE-DEGRADE-SHIPPED"),
        station: "dereth-client::gpu::rendering::adaptive_degrade::the_shipped_client_runs_automatic_degrades_and_the_preference_turns_them_off",
        tier: Tier::Gpu,
    },
    behaviour! {
        id: "rendering.detail-textures.the-preference-binds-a-second-stage-and-moves-pixels",
        says: "With building detail textures turned on, buildings are drawn with a second, \
               repeating detail texture layered over their own from the very next frame; turned \
               off, no draw in the frame uses a second texture layer.",
        since: RETAIL,
        evidence: Evidence::Private("AC-EVID-P39-DETAIL-TEXTURES"),
        station: "dereth-client::gpu::rendering::detail_textures::the_building_pass_binds_a_second_texture_stage_and_the_pixels_move",
        tier: Tier::Gpu,
    },
    behaviour! {
        id: "rendering.draw-order.blocks-and-cells-draw-far-to-near-with-the-sky-around-them",
        says: "The ground is drawn from far to near, both between the blocks of land around the \
               player and between the squares within each block.",
        since: RETAIL,
        evidence: Evidence::Private("AC-EVID-DRAW-ORDER-DRAW-ORDER"),
        station: "dereth-world-render::dat::rendering::draw_order::the_recorded_batch_sequence_is_far_to_near_between_and_within_blocks",
        tier: Tier::Dat,
    },
    behaviour! {
        id: "rendering.draw-order.near-foliage-occludes-what-stands-behind-it",
        says: "A creature standing behind a plant is partly hidden by the plant's leaves, losing a \
               large share of the pixels it shows when standing in the clear beside it.",
        since: RETAIL,
        evidence: Evidence::Private("AC-EVID-RDF-FOLIAGE-OCCLUSION-DRAW-ORDER"),
        station: "dereth-client::gpu::rendering::foliage_occlusion::the_plants_leaves_hide_a_creature_standing_behind_them",
        tier: Tier::Gpu,
    },
    behaviour! {
        id: "rendering.effects.a-play-script-for-an-unknown-object-is-parked",
        says: "An Effects_PlayScriptType message about an object the client does not hold is set \
               aside rather than played or thrown away: nothing reaches the objects the client is \
               drawing and no effect is started.",
        since: RETAIL,
        evidence: Evidence::Private("AC-EVID-F19-EFFECTS-PLAY"),
        station: "dereth-client::gpu::rendering::play_script_messages::a_wire_message_for_an_object_the_client_does_not_hold_is_parked_not_delivered",
        tier: Tier::Gpu,
    },
    behaviour! {
        id: "rendering.effects.a-play-script-message-reaches-a-visible-emitter",
        says: "An Effects_PlayScriptType message from the shard about an object the client holds \
               plays that effect on the object within a few frames: it gains live emitters holding \
               particles, exactly the emitters the effect's script names, and the message is \
               handled rather than ignored.",
        since: RETAIL,
        evidence: Evidence::Private("AC-EVID-F19-EFFECTS"),
        station: "dereth-client::gpu::rendering::play_script_messages::a_wire_play_script_type_reaches_a_visible_emitter",
        tier: Tier::Gpu,
    },
    behaviour! {
        id: "rendering.effects.a-played-script-type-creates-the-objects-emitters",
        says: "Playing an effect type on an object starts the effect the object's own effect table \
               names for that strength: the object gains live particle emitters, one for each \
               particle the effect's script creates, and exactly the emitters that script asks \
               for.",
        since: RETAIL,
        evidence: Evidence::Private("AC-EVID-F17-EFFECTS"),
        station: "dereth-client::gpu::rendering::play_script_messages::a_played_script_type_gives_the_object_it_names_live_emitters",
        tier: Tier::Gpu,
    },
    behaviour! {
        id: "rendering.effects.an-objects-own-emitters-burn-where-it-stands-in-any-landblock",
        says: "A campfire standing in the landblock next to the player's draws its flames and \
               smoke on the campfire, not floating at the same spot in the player's own landblock.",
        since: RETAIL,
        evidence: Evidence::Private("AC-EVID-OBJECT-PARTICLE-SPACE"),
        station: "dereth-client::dat::rendering::object_particle_space::a_campfire_in_the_next_landblock_burns_where_it_stands",
        tier: Tier::Dat,
    },
    behaviour! {
        id: "rendering.effects.an-objects-particles-stay-put-when-the-viewer-changes-landblock",
        says: "When the player walks into another landblock, the flames and smoke already in the \
               air above a campfire stay above the campfire, including those of a campfire too far \
               away to be animating.",
        since: RETAIL,
        evidence: Evidence::Private("AC-EVID-OBJECT-PARTICLE-SPACE-SCROLL"),
        station: "dereth-client::dat::rendering::object_particle_space::a_campfires_particles_stay_at_the_campfire_when_the_viewer_walks_into_its_landblock",
        tier: Tier::Dat,
    },
    behaviour! {
        id: "rendering.indoor.a-sealed-room-sees-no-outdoors-and-a-windowed-house-does",
        says: "Standing in a sealed dungeon room the client finds no view out to the outdoors, so \
               it draws no land, scenery or sky there, while standing in a Holtburg house it finds \
               outdoor views through the windows and draws the outdoors through them.",
        since: RETAIL,
        evidence: Evidence::Private("AC-EVID-F5-INDOOR"),
        station: "dereth-client::gpu::rendering::indoor_outdoor_gate::a_sealed_dungeon_room_sees_no_outdoors_and_a_house_with_windows_does",
        tier: Tier::Gpu,
    },
    behaviour! {
        id: "rendering.indoor.the-outdoor-depth-is-cleared-and-openings-restamped",
        says: "When the player is indoors, the depth the outdoor pass left behind is cleared \
               before the rooms are drawn and each door or window opening is stamped back into it, \
               so the land seen through a window is not covered by the room drawn behind it.",
        since: RETAIL,
        evidence: Evidence::Private("AC-EVID-F14-INDOOR"),
        station: "dereth-client::gpu::rendering::indoor_depth_clear::the_openings_are_stamped_back_into_the_cleared_depth",
        tier: Tier::Gpu,
    },
    behaviour! {
        id: "rendering.interior.a-body-in-several-cells-is-drawn-by-each-of-them",
        says: "A body that reaches into more than one room is drawn by every room it is in, each                seen through its own view: standing on cellar stairs with the camera in the room                above, the whole body is drawn, not only the legs the stairwell's opening shows.",
        since: RETAIL,
        evidence: Evidence::Private("AC-EVID-BODY-SHADOW-CELLS"),
        station: "dereth-client::gpu::rendering::interior_cell_views::a_body_on_the_cellar_stairs_is_drawn_whole_from_the_room_above",
        tier: Tier::Gpu,
    },
    behaviour! {
        id: "rendering.interior.a-visible-door-survives-depth-testing-at-the-recorded-pose",
        says: "A villa's gate door that reaches into the courtyard is drawn and seen by a player \
               whose camera has settled at the front gate, painting well over a hundred pixels \
               rather than being lost behind the courtyard's depth.",
        since: RETAIL,
        evidence: Evidence::Private("AC-EVID-P199-INTERIOR"),
        station: "dereth-client::gpu::rendering::villa_door_visibility::the_villa_courtyard_door_is_visible_at_the_recorded_pose",
        tier: Tier::Gpu,
    },
    behaviour! {
        id: "rendering.interior.each-reached-cell-draws-through-its-own-portal-view",
        says: "Indoors, the room the camera is in is seen across the whole screen, while every \
               other room reached through a doorway is seen only through that doorway's outline on \
               screen, a view smaller than the screen.",
        since: RETAIL,
        evidence: Evidence::Private("AC-EVID-P170D-INTERIOR"),
        station: "dereth-client::gpu::rendering::interior_cell_views::a_cell_reached_through_a_doorway_has_a_view_smaller_than_the_screen",
        tier: Tier::Gpu,
    },
    behaviour! {
        id: "rendering.landscape.the-draw-distance-preference-sets-the-land-window",
        says: "The landscape draw-distance choice in the player's profile sets how far land is \
               drawn: Very Low, Low, Medium, High, Very High and Extreme give a radius of 3, 5, 8, \
               11, 15 and 25 landblocks, a bare number is read as a position in that list, and no \
               setting leaves the shipped default.",
        since: RETAIL,
        evidence: Evidence::Private("AC-EVID-G24-LANDSCAPE"),
        station: "dereth-client::gpu::rendering::landscape_draw_distance::the_landscape_draw_distance_preference_reaches_the_window",
        tier: Tier::Gpu,
    },
    behaviour! {
        id: "rendering.landscape.the-landscape-detail-preference-draws-a-fading-detail-texture",
        says: "With Landscape Detail Textures turned on in the player's profile, a fine \
               repeating detail texture is drawn over the ground near the player, strongest \
               within 10 metres and gone by 50; seen from high above, the ground looks exactly as \
               it does with the option off, and with it off two loads of the same view are \
               identical.",
        since: THIS_CLIENT,
        divergence: "CD-011",
        evidence: Evidence::Private("AC-EVID-LEGACY-TERRAIN-DETAIL"),
        station: "dereth-client::gpu::rendering::detail_textures::the_landscape_detail_texture_covers_the_near_ground_and_fades_out_by_fifty_metres",
        tier: Tier::Gpu,
    },
    behaviour! {
        id: "rendering.lighting.interior-station-matches-the-retail-pair-region-by-region",
        says: "Standing at a fixed spot in a training academy room, the lit room the client draws \
               matches a retail screenshot from the same spot region by region: each lit surface's \
               brightness relative to a reference patch of floor is within a fifth of retail's, no \
               region is more than 20 levels off, and the hearth glow is close.",
        since: RETAIL,
        evidence: Evidence::Private("AC-EVID-P189C-LIGHTING"),
        station: "dereth-client::gpu::rendering::interior_lighting_parity::the_station_matches_the_retail_pair_region_by_region",
        private_oracle: true,
        tier: Tier::Gpu,
    },
    behaviour! {
        id: "rendering.lighting.resident-light-pools-and-sunlight-light-the-scene",
        says: "Standing in a training academy room, the torches and lamps placed there are \
               registered as lights and gathered nearest first into the lights that shade the \
               scene, alongside a white light carried with the viewer.",
        since: RETAIL,
        evidence: Evidence::Private("AC-EVID-P3-LIGHTING"),
        station: "dereth-client::gpu::rendering::lighting::a_torch_in_an_academy_cell_enters_the_static_pool_when_the_body_stands_there",
        tier: Tier::Gpu,
    },
    behaviour! {
        id: "rendering.markers.marker-placements-in-the-dat-are-never-drawn",
        says: "The placement markers the world data carries for its own editors are never drawn: \
               across every landblock, the placements the client hides are exactly those made only \
               of the eleven marker models, and no real object is hidden with them.",
        since: RETAIL,
        evidence: Evidence::Private("AC-EVID-O73-MARKERS"),
        station: "dereth-client::dat::rendering::degrade_marker_parts::the_guard_hides_exactly_the_marker_placements_of_the_whole_cell_dat",
        tier: Tier::Gpu,
    },
    behaviour! {
        id: "rendering.mesh.a-polygon-without-uv-indices-uses-each-vertexs-first-uv",
        says: "A polygon of an object or a room that carries no texture-coordinate choices of its \
               own takes each corner's first texture coordinate from its vertex, in both the \
               object and the room mesh builders; explicit choices stay per corner, and a vertex \
               with no coordinates gets zero.",
        since: RETAIL,
        evidence: Evidence::Private("AC-EVID-ASTRA-MESH-UV-MESH"),
        station: "dereth-client::dat::rendering::mesh_uv_indices::absent_cell_uv_indices_select_vertex_uv_zero",
        tier: Tier::Dat,
    },
    behaviour! {
        id: "rendering.night.the-night-cast-sky-and-fog-match-retail",
        says: "At the exact game date and second of a retail night recording, the client's night \
               carries retail's colour cast, green falling short of red by about retail's own \
               amount, while a render that knows only the time of day and not the date does not.",
        since: RETAIL,
        evidence: Evidence::Private("AC-EVID-P4-NIGHT"),
        station: "dereth-client::gpu::rendering::night_and_fog::at_the_captures_own_second_the_night_carries_retails_green_deficit",
        private_oracle: true,
        tier: Tier::Gpu,
    },
    behaviour! {
        id: "rendering.object-parts.are-submitted-farthest-first-by-sort-centre",
        says: "The parts of creatures and objects in view are drawn farthest first, ordered by \
               each part's distance from the camera, measured to its own sort centre while the \
               camera is inside its object's share distance and to the object's origin beyond \
               it, at every camera height tried over a replayed recorded session.",
        since: RETAIL,
        evidence: Evidence::Private("AC-EVID-O197-OBJECT-PARTS"),
        station: "dereth-client::gpu::rendering::object_part_submission::every_part_is_submitted_farthest_first_on_the_clients_own_cypt",
        tier: Tier::Gpu,
    },
    behaviour! {
        id: "rendering.object-parts.transparent-subsets-are-deferred-clip-before-blend",
        says: "Each piece of a creature or object is drawn according to its surface: solid pieces \
               at once, and see-through pieces held back until after all of them, with the cut-out \
               pieces all drawn before the blended ones.",
        since: RETAIL,
        evidence: Evidence::Private("AC-EVID-O216-OBJECT-PARTS"),
        station: "dereth-client::gpu::rendering::object_part_submission::every_subset_lands_where_add_mesh_to_alpha_list_would_put_it",
        tier: Tier::Gpu,
    },
    behaviour! {
        id: "rendering.objects.a-bare-body-part-draws-as-the-other-eras-bare-part",
        says: "With another era's object mode, a bare arm or hand is drawn as that era drew a \
               bare arm or hand, not as the model that era kept under the same number (for the \
               end-of-retail arm, the February 2005 armoured arm).",
        since: THIS_CLIENT,
        divergence: "CD-014",
        evidence: Evidence::Private("AC-EVID-OBJECT-FIDELITY-BARE"),
        station: "dereth-client::dat::rendering::object_look::a_bare_arm_draws_as_the_other_eras_bare_arm_and_not_its_armoured_one",
        tier: Tier::Dat,
    },
    behaviour! {
        id: "rendering.objects.a-building-shell-takes-the-look-only-with-the-worlds-geometry",
        says: "With another era's object mode, a building takes that era's look only where its \
               shape is the world's, so its doorways still meet the world's interiors; a \
               reshaped building keeps the world's look.",
        since: THIS_CLIENT,
        divergence: "CD-014",
        evidence: Evidence::Private("AC-EVID-OBJECT-FIDELITY-SHELL"),
        station: "dereth-client::dat::rendering::object_look::a_building_shell_takes_the_older_look_only_where_its_geometry_is_the_worlds",
        tier: Tier::Dat,
    },
    behaviour! {
        id: "rendering.objects.a-clothing-picture-follows-the-surface-onto-the-other-eras-part",
        says: "With another era's object mode, a garment still shows on the body: the later files \
               painted the body with other pictures, so the picture the world's garment replaces \
               is put on the same surface of the other era's part, as that era's own garment does.",
        since: THIS_CLIENT,
        divergence: "CD-014",
        evidence: Evidence::Private("AC-EVID-ERA-VISUALS-SLOT"),
        station: "dereth-client::dat::rendering::object_look::a_clothing_picture_follows_the_surface_onto_the_later_arm",
        tier: Tier::Dat,
    },
    behaviour! {
        id: "rendering.objects.a-colour-the-other-era-lacks-is-that-eras-colour-from-the-same-place",
        says: "With another era's object mode, a colour the other era does not have (a hair, skin \
               or eye colour, or a dye) is drawn as that era's colour from the same place in the \
               same choice of colours, so a body the world dressed in a newer colour still takes \
               the other era's look.",
        since: THIS_CLIENT,
        divergence: "CD-014",
        evidence: Evidence::Private("AC-EVID-OBJECT-POLISH-COLOUR"),
        station: "dereth-client::dat::rendering::object_look::a_new_end_of_retail_body_takes_the_older_look_with_the_older_hair_colour",
        tier: Tier::Dat,
    },
    behaviour! {
        id: "rendering.objects.a-dye-the-other-era-cannot-match-is-left-off",
        says: "With another era's object mode, a dye or skin colour the other era has nothing \
               for in the same place is left off the other era's part, which keeps its own colour \
               there; the rest of the object still takes the other era's look.",
        since: THIS_CLIENT,
        divergence: "CD-014",
        evidence: Evidence::Private("AC-EVID-OBJECT-COLOURS-DYE"),
        station: "dereth-client::dat::rendering::object_look::a_dye_the_older_files_cannot_match_is_left_off_and_the_body_takes_the_older_look",
        tier: Tier::Dat,
    },
    behaviour! {
        id: "rendering.objects.a-head-draws-the-other-eras-head-for-the-same-hair-style",
        says: "With another era's object mode, a head wearing one of the character-creation hair \
               styles is drawn as the other era's head for the style in the same place of its \
               list, with the same face; a style the other era's list lacks, or has bald where \
               the world's is not, keeps the world's head.",
        since: THIS_CLIENT,
        divergence: "CD-014",
        evidence: Evidence::Private("AC-EVID-OBJECT-POLISH-HAIR"),
        station: "dereth-client::dat::rendering::object_look::a_head_draws_the_other_eras_head_for_the_same_hair_style",
        tier: Tier::Dat,
    },
    behaviour! {
        id: "rendering.objects.a-part-draws-wholly-from-one-era",
        says: "With another era's object mode, each part of an object is drawn wholly in one \
               era's look, its model with that era's own paint: a model the other era does not \
               have, or has as something else, keeps the world's look (its colours aside), and is \
               never painted with whatever the other era keeps under the same numbers.",
        since: THIS_CLIENT,
        divergence: "CD-014",
        evidence: Evidence::Private("AC-EVID-OBJECT-FIDELITY-ONE-ERA"),
        station: "dereth-client::dat::rendering::object_look::a_later_model_the_older_files_lack_is_drawn_wholly_from_the_worlds_records",
        tier: Tier::Dat,
    },
    behaviour! {
        id: "rendering.objects.a-part-kept-in-the-worlds-look-takes-the-other-eras-colours",
        says: "With another era's object mode, a part kept in the world's look beside parts drawn \
               in the other era's look takes the other era's colours where that era has them, so \
               a later head on an older body has the older body's skin.",
        since: THIS_CLIENT,
        divergence: "CD-014",
        evidence: Evidence::Private("AC-EVID-OBJECT-COLOURS-SKIN"),
        station: "dereth-client::dat::rendering::object_look::a_later_head_on_an_older_body_takes_the_older_skin_colour",
        tier: Tier::Dat,
    },
    behaviour! {
        id: "rendering.objects.a-remodelled-creature-draws-the-other-eras-remodel",
        says: "With another era's object mode, a creature or object the other era built with other \
               models is drawn with that era's models, moved by the world's animations, where each \
               of its parts rests where the world's part of the same place rests; a part the other \
               era's object lacks is not drawn, and an object the other era built with more parts, \
               or with parts resting elsewhere, keeps the world's look.",
        since: THIS_CLIENT,
        divergence: "CD-014",
        evidence: Evidence::Private("AC-EVID-OBJECT-POLISH-REMODEL"),
        station: "dereth-client::dat::rendering::object_look::a_remodelled_creature_draws_the_other_eras_remodel_where_it_rests_as_the_worlds_does",
        tier: Tier::Dat,
    },
    behaviour! {
        id: "rendering.objects.an-interior-room-draws-from-the-other-eras-record-where-it-is-the-same-room",
        says: "With another era's object mode, an interior room is drawn from that era's own record \
               of it, with that era's paint, where that era has the same room in the same place; \
               a room it lacks, or holds elsewhere or joined otherwise, keeps the world's look.",
        since: THIS_CLIENT,
        divergence: "CD-014",
        evidence: Evidence::Private("AC-EVID-OBJECT-INTERIORS-ROOMS"),
        station: "dereth-client::dat::rendering::object_look::an_interior_takes_the_other_eras_room_only_where_it_is_the_same_room_in_the_same_place",
        tier: Tier::Dat,
    },
    behaviour! {
        id: "rendering.objects.an-object-keeps-the-worlds-setup-under-another-eras-look",
        says: "With another era's object mode, every object keeps the world's own parts and how \
               they move: the body keeps its 17 parts on a February 2005 world and its 34 at the \
               end of retail, and only what each part looks like comes from the other era.",
        since: THIS_CLIENT,
        divergence: "CD-014",
        evidence: Evidence::Private("AC-EVID-ERA-VISUALS-SETUP"),
        station: "dereth-client::dat::rendering::object_look::the_human_body_keeps_the_worlds_seventeen_parts_and_the_later_files_name_the_same_ones",
        tier: Tier::Dat,
    },
    behaviour! {
        id: "rendering.objects.an-object-mode-asked-for-before-its-verdicts-are-ready-is-drawn-when-they-arrive",
        says: "Another era's object mode chosen before the client has finished working out which \
               of that era's records stand for the world's keeps the objects as they are drawn, \
               tells the player once that the look is still being prepared, and draws the look \
               the frame the work is done, exactly as it would have been drawn from the start.",
        since: THIS_CLIENT,
        divergence: "CD-014",
        evidence: Evidence::Private("AC-EVID-IDENTITY-PREBUILD-WAIT"),
        station: "dereth-client::gpu::rendering::object_modes::an_object_mode_asked_for_before_its_verdicts_are_ready_keeps_the_look_until_they_arrive",
        tier: Tier::Gpu,
    },
    behaviour! {
        id: "rendering.objects.an-object-mode-switch-redraws-the-town-and-the-body-while-the-world-is-drawn",
        says: "Changing the object mode while the world is on screen redraws the buildings, \
               statics, scenery, creatures, items and the player's body in the chosen era's look \
               from the next frame; changing back draws exactly the picture the world started \
               with, and nothing the landscape places is added or lost.",
        since: THIS_CLIENT,
        divergence: "CD-014",
        evidence: Evidence::Private("AC-EVID-ERA-VISUALS-SWITCH"),
        station: "dereth-client::gpu::rendering::object_modes::an_object_mode_switch_rebuilds_the_town_and_the_body_and_switching_back_restores_the_frame",
        tier: Tier::Gpu,
    },
    behaviour! {
        id: "rendering.objects.an-object-mode-without-its-files-is-refused",
        says: "Choosing the Legacy object mode without the older data files leaves the objects \
               drawn as they were, puts the option back, and tells the player in the chat window \
               that the object mode requires legacy DATs.",
        since: THIS_CLIENT,
        divergence: "CD-014",
        evidence: Evidence::Private("AC-EVID-ERA-VISUALS-REFUSED"),
        station: "dereth-client::gpu::rendering::object_modes::an_object_mode_without_its_files_is_refused_and_the_world_keeps_its_own",
        tier: Tier::Gpu,
    },
    behaviour! {
        id: "rendering.objects.an-object-whose-picture-change-cannot-be-placed-keeps-the-worlds-look",
        says: "A part whose clothing or paint change has no matching surface on the other \
               era's part is drawn in the world's own look, rather than without the change.",
        since: THIS_CLIENT,
        divergence: "CD-014",
        evidence: Evidence::Private("AC-EVID-ERA-VISUALS-FALLBACK"),
        station: "dereth-client::dat::rendering::object_look::a_change_with_no_matching_slot_draws_the_object_with_the_worlds_records",
        tier: Tier::Dat,
    },
    behaviour! {
        id: "rendering.objects.an-older-world-draws-its-objects-with-the-later-look",
        says: "With the Modern object mode, a February 2005 world draws its town and its \
               player's body with the end-of-retail models and pictures, while its land, ground \
               and sky stay as chosen.",
        since: THIS_CLIENT,
        divergence: "CD-014",
        evidence: Evidence::Private("AC-EVID-ERA-VISUALS-LATER"),
        station: "dereth-client::gpu::rendering::object_modes::an_older_world_draws_its_objects_with_the_later_look_and_keeps_its_own_setups",
        tier: Tier::Gpu,
    },
    behaviour! {
        id: "rendering.objects.interiors-follow-their-building-in-another-eras-look",
        says: "With another era's object mode, a town's rooms take that era's look with their \
               buildings, and switching the mode back draws them in the world's own look again.",
        since: THIS_CLIENT,
        divergence: "CD-014",
        evidence: Evidence::Private("AC-EVID-OBJECT-INTERIORS-FOLLOW"),
        station: "dereth-client::gpu::rendering::object_modes::holtburgs_rooms_take_the_other_eras_look_with_their_buildings_and_go_back_with_them",
        tier: Tier::Gpu,
    },
    behaviour! {
        id: "rendering.objects.the-object-identity-is-kept-in-the-hosts-store-and-read-back",
        says: "Which of the other era's records stand for the world's is worked out once per set \
               of data files and kept in the client's own store, named by the files' hashes; the \
               next start reads it back without reading the data files whole, and a kept answer \
               of another version is worked out again.",
        since: THIS_CLIENT,
        divergence: "CD-014",
        evidence: Evidence::Private("AC-EVID-IDENTITY-PREBUILD-CACHE"),
        station: "dereth-client::dat::rendering::object_look::the_identity_is_kept_in_the_hosts_store_and_read_back_without_reading_the_files_whole",
        tier: Tier::Dat,
    },
    behaviour! {
        id: "rendering.objects.the-object-identity-worked-out-a-step-at-a-time-is-the-one-worked-out-at-once",
        says: "The client works out which of the other era's records stand for the world's a few \
               milliseconds a frame from start-up, and the answer is the same as working it out \
               all at once.",
        since: THIS_CLIENT,
        divergence: "CD-014",
        evidence: Evidence::Private("AC-EVID-IDENTITY-PREBUILD-STEPS"),
        station: "dereth-client::dat::rendering::object_look::the_identity_worked_out_a_unit_at_a_time_is_the_identity_worked_out_at_once",
        tier: Tier::Dat,
    },
    behaviour! {
        id: "rendering.objects.the-paper-doll-wears-the-bodys-look",
        says: "With another era's object mode, the inventory's paper doll is drawn in the same \
               look as the player's body in the world, part for part.",
        since: THIS_CLIENT,
        divergence: "CD-014",
        evidence: Evidence::Private("AC-EVID-OBJECT-FIDELITY-DOLL"),
        station: "dereth-client::gpu::rendering::object_modes::the_paper_doll_wears_the_look_the_body_wears_in_the_world",
        tier: Tier::Gpu,
    },
    behaviour! {
        id: "rendering.particles.a-non-luminous-particle-is-lit-by-the-scene",
        says: "Particles that do not glow on their own are shaded by the lights around them: each \
               colour is drawn at the fraction of its unlit brightness the scene's lighting \
               predicts, and the same effect standing nearer the light is clearly brighter than \
               one farther away.",
        since: RETAIL,
        evidence: Evidence::Private("AC-EVID-P189G-PARTICLES"),
        station: "dereth-client::gpu::rendering::particle_lighting::a_non_luminous_particle_is_lit_by_the_scene",
        tier: Tier::Gpu,
    },
    behaviour! {
        id: "rendering.particles.emitters-in-unreached-cells-are-not-drawn",
        says: "From a villa's courtyard, the particle effects in basement rooms that cannot be \
               seen from there add nothing to the picture: the courtyard floor looks the same with \
               particles on as with them off.",
        since: RETAIL,
        evidence: Evidence::Private("AC-EVID-P199-PARTICLES"),
        station: "dereth-client::gpu::rendering::unseen_cell_particles::the_villa_courtyard_floor_does_not_show_unseen_basement_particles",
        tier: Tier::Gpu,
    },
    behaviour! {
        id: "rendering.particles.past-the-particle-share-distance-an-emitters-particles-take-its-distance-and-heading",
        says: "Once the camera is at or past the particle share distance from an emitter across the \
               ground, all of its particles are sorted at the emitter's distance and turn toward its \
               bearing; inside it each particle is measured and turned where it is.",
        since: RETAIL,
        evidence: Evidence::Private("AC-EVID-SHARE-DISTANCE-PARTICLES"),
        station: "dereth-scene::lib::particles::tests::past_the_particle_share_distance_every_particle_takes_the_emitters_distance_and_heading",
        tier: Tier::Cpu,
    },
    behaviour! {
        id: "rendering.particles.scripted-statics-become-drawn-emitters",
        says: "The particle effects built into objects placed around Holtburg start by themselves: \
               every placed object whose model carries a default effect starts it when the land \
               loads, and each gives at least one live particle emitter.",
        since: RETAIL,
        evidence: Evidence::Private("AC-EVID-PARTICLES-PARTICLES"),
        station: "dereth-client::gpu::rendering::particles::the_scripted_statics_around_holtburg_become_live_emitters",
        tier: Tier::Gpu,
    },
    behaviour! {
        id: "rendering.particles.the-additive-fire-core-matches-retail",
        says: "At the training academy hearth, the glow of the fire's additive flame particles has \
               an average brightness within a set tolerance of a retail screenshot from the same \
               spot, and its brightest pixels exceed retail's by no more than a fixed ceiling.",
        since: RETAIL,
        evidence: Evidence::Private("AC-EVID-P189F-PARTICLES"),
        station: "dereth-client::gpu::rendering::fire_particles::the_fire_core_is_bounded_against_retail",
        private_oracle: true,
        tier: Tier::Gpu,
    },
    behaviour! {
        id: "rendering.particles.the-player-materialise-shimmer-follows-each-body-part",
        says: "The shimmer played when a player appears or vanishes puts its sparkles on fourteen \
               of the body's parts, and each stream of sparkles follows its own part as it moves \
               rather than the body's origin.",
        since: RETAIL,
        evidence: Evidence::Private("AC-EVID-P196-PARTICLES"),
        station: "dereth-client::dat::rendering::materialize_shimmer::the_player_shimmer_updates_each_emitter_from_its_own_live_part",
        tier: Tier::Dat,
    },
    behaviour! {
        id: "rendering.portals.a-closed-door-crosses-the-outdoor-indoor-boundary",
        says: "A closed Holtburg house door, placed outdoors but reaching into the room, is drawn \
               in the outdoor pass and again after the room's depth is reset, so a player standing \
               in the room sees the whole door rather than one with pieces missing.",
        since: RETAIL,
        evidence: Evidence::Private("AC-EVID-G25-PORTALS"),
        station: "dereth-client::gpu::rendering::building_boundary_draw::the_closed_holtburg_door_naturally_crosses_the_outdoor_indoor_boundary",
        tier: Tier::Gpu,
    },
    behaviour! {
        id: "rendering.portals.a-held-item-draws-in-its-holders-phase",
        says: "Seen from inside a room through a doorway, a weapon held by a creature standing \
               outdoors is drawn with its holder in the outdoor pass, rather than being split off \
               into the later pass that draws the room.",
        since: RETAIL,
        evidence: Evidence::Private("AC-EVID-F30-PORTALS-HELD"),
        station: "dereth-client::gpu::rendering::objects_through_doorways::a_held_item_uses_its_holders_outdoor_draw_phase",
        tier: Tier::Gpu,
    },
    behaviour! {
        id: "rendering.portals.a-wall-occludes-an-object-inside-from-outdoors",
        says: "From outdoors, an object standing inside a house behind its wall is hidden by the \
               wall and paints no pixel, while the same object standing in the open in front of \
               the house is plainly visible.",
        since: RETAIL,
        evidence: Evidence::Private("AC-EVID-G25-PORTALS-WALL"),
        station: "dereth-client::gpu::rendering::building_boundary_draw::the_buildings_wall_occludes_an_object_inside_it_from_outdoors",
        tier: Tier::Gpu,
    },
    behaviour! {
        id: "rendering.portals.an-interior-appears-through-its-window-from-outside",
        says: "Standing outside a Holtburg house, the player sees the room inside through its \
               window and only through the window: every pixel the rooms add lies inside the \
               outline of an opening and none over the walls.",
        since: RETAIL,
        evidence: Evidence::Private("AC-EVID-OUTDOOR-PORTALS-PORTALS"),
        station: "dereth-client::gpu::rendering::interiors_through_outdoor_portals::an_interior_appears_through_the_window_and_nowhere_else",
        tier: Tier::Gpu,
    },
    behaviour! {
        id: "rendering.portals.an-object-outside-survives-the-portal-depth-stamp",
        says: "A player in a room looking out through a doorway sees an object standing outside: \
               resetting the depth inside the opening keeps nearly all of the object seen through \
               it, and the object's pixels outside the opening are the same either way.",
        since: RETAIL,
        evidence: Evidence::Private("AC-EVID-F30-PORTALS"),
        station: "dereth-client::gpu::rendering::objects_through_doorways::an_object_outside_the_doorway_survives_the_portal_depth_stamp",
        tier: Tier::Gpu,
    },
    behaviour! {
        id: "rendering.portals.the-alpha-flush-runs-before-the-building",
        says: "The see-through pieces of land already queued are drawn before each building's \
               rooms are drawn through its openings, rather than all at the end of the landscape; \
               this moves them earlier without drawing any twice or dropping any, and it changes \
               no pixel the building's openings do not.",
        since: RETAIL,
        evidence: Evidence::Private("AC-EVID-CLIPPED-OUTDOOR-PASS-PORTALS-ALPHA"),
        station: "dereth-client::gpu::rendering::clipped_outdoor_pass::the_alpha_flush_runs_before_the_building_and_changes_nothing_it_should_not",
        tier: Tier::Gpu,
    },
    behaviour! {
        id: "rendering.portals.the-depth-stamp-is-drawn-only-inside-the-openings",
        says: "Seen from outside, each visible door or window opening of a building has its depth \
               reset before the room behind it is drawn, so the ground in front does not hide the \
               room; the reset changes pixels only inside the openings and never over the \
               surrounding wall, looking level or looking down.",
        since: RETAIL,
        evidence: Evidence::Private("AC-EVID-CLIPPED-OUTDOOR-PASS-PORTALS"),
        station: "dereth-client::gpu::rendering::clipped_outdoor_pass::the_depth_stamp_is_drawn_and_only_inside_the_openings",
        tier: Tier::Gpu,
    },
    behaviour! {
        id: "rendering.preferences.a-render-option-applies-on-the-next-frame",
        says: "Changing the landscape draw distance on the options page takes effect on the very \
               next frame: the ring of land loaded around the player is rebuilt at the new size \
               and holds more ground than before, with nothing failing to load.",
        since: RETAIL,
        evidence: Evidence::Private("AC-EVID-P35B-PREFERENCES"),
        station: "dereth-client::gpu::rendering::render_preferences::the_landscape_draw_distance_rebuilds_the_ring_on_the_next_frame",
        tier: Tier::Gpu,
    },
    behaviour! {
        id: "rendering.preferences.render-options-reach-projection-and-device",
        says: "The render settings saved in the player's profile reach the picture: field of view \
               and aspect ratio set the projection, degrade distance and graphics performance set \
               how far detail holds, and texture detail sets how finely land is textured; with no \
               profile the client uses retail's 90 degrees and 4:3.",
        since: RETAIL,
        evidence: Evidence::Private("AC-EVID-P3-PREFERENCES"),
        station: "dereth-client::gpu::rendering::render_preferences::the_profile_s_render_preferences_reach_the_projection_and_the_device",
        tier: Tier::Gpu,
    },
    behaviour! {
        id: "rendering.preview.panel-previews-draw-graphics-object-level-zero",
        says: "The paper doll shows the player's dressed body at its most detailed: every part of \
               the body is drawn from the highest-detail model the data gives it, not the coarser \
               one the body's own layout lists.",
        since: RETAIL,
        evidence: Evidence::Private("AC-EVID-P180-PREVIEW"),
        station: "dereth-client::gpu::rendering::preview_level_zero::the_paper_doll_bakes_gfxobj_zero_for_every_part",
        tier: Tier::Gpu,
    },
    behaviour! {
        id: "rendering.samplers.bias-changes-apply-at-begin-scene-and-banks-survive-mid-frame-changes",
        says: "Changing the texture filtering option takes effect for the filter at once but for \
               texture sharpness only from the next frame, and a sharpened preview draw puts the \
               normal sharpness back afterwards, even when it fails.",
        since: RETAIL,
        evidence: Evidence::Private("AC-EVID-ASTRA-SAMPLER-BIAS-SAMPLERS"),
        station: "dereth-render::lib::d3d12::tests::sampler_bias_changes_at_begin_scene_and_guarded_preview_restores_global_baseline",
        tier: Tier::Cpu,
    },
    behaviour! {
        id: "rendering.scene.the-static-holtburg-scene-draws-lit-and-stable",
        says: "Looking over Holtburg from above, nearly all of the ground filling the lower half \
               of the picture is lit and nearly all of the sky across the top is drawn, so the \
               scene is neither lost nor left black.",
        since: RETAIL,
        evidence: Evidence::Private("AC-EVID-STATIC-SCENE-SCENE"),
        station: "dereth-client::gpu::rendering::static_scene::a_frame_of_the_static_scene_is_mostly_lit_below_the_horizon",
        tier: Tier::Gpu,
    },
    behaviour! {
        id: "rendering.scenery.scenery-stops-at-the-full-detail-ring",
        says: "Trees, rocks and other scenery are placed only on land drawn at full detail: the \
               block the player stands on and the eight around it, exactly that three-by-three \
               core, and on no block farther out however wide the land is drawn.",
        since: RETAIL,
        evidence: Evidence::Private("AC-EVID-G41-SCENERY"),
        station: "dereth-client::gpu::rendering::terrain_lod_seams::the_shipped_scenery_radius_selects_exactly_the_full_detail_blocks",
        tier: Tier::Gpu,
    },
    behaviour! {
        id: "rendering.sky.the-sky-is-drawn-and-the-day-cycle-moves-it",
        says: "The sky is drawn rather than left black, and the time of day changes it: noon and \
               midnight give clearly different pictures, and the sun and the ambient light both \
               change between them.",
        since: RETAIL,
        evidence: Evidence::Private("AC-EVID-SKY-SKY"),
        station: "dereth-client::gpu::rendering::sky::the_sky_is_not_black_and_the_day_cycle_moves_it",
        tier: Tier::Gpu,
    },
    behaviour! {
        id: "rendering.terrain.a-ground-style-switch-rebuilds-the-ground-while-the-world-is-drawn",
        says: "Changing the terrain mode while the world is on screen redraws the ground in the new \
               style from the next frame, every block around the player, with the same \
               scenery and buildings; changing back draws exactly the picture the world \
               started with.",
        since: THIS_CLIENT,
        divergence: "CD-012",
        evidence: Evidence::Private("AC-EVID-TERRAIN-MODES-SWITCH"),
        station: "dereth-client::gpu::rendering::terrain_modes::a_ground_switch_rebuilds_every_resident_block_and_switching_back_restores_the_frame",
        tier: Tier::Gpu,
    },
    behaviour! {
        id: "rendering.terrain.a-style-without-its-files-is-refused-and-the-world-keeps-its-own",
        says: "Choosing an older terrain mode or sky without the older data files leaves the world \
               drawn as it was, puts the option back, and tells the player in the chat window \
               that the mode requires legacy DATs.",
        since: THIS_CLIENT,
        divergence: "CD-012",
        evidence: Evidence::Private("AC-EVID-TERRAIN-MODES-REFUSED"),
        station: "dereth-client::gpu::rendering::terrain_modes::a_style_without_its_files_is_refused_and_the_world_keeps_its_own",
        tier: Tier::Gpu,
    },
    behaviour! {
        id: "rendering.terrain.a-terrain-type-the-region-does-not-name-takes-its-neighbours-ground",
        says: "Where the world's land uses a terrain type the chosen ground style has no texture of \
               its own for, such as Desolate Lands under an older style, that spot is drawn \
               as the ground most common around it rather than as a stand-in texture.",
        since: THIS_CLIENT,
        divergence: "CD-013",
        evidence: Evidence::Private("AC-EVID-TERRAIN-MODES-FILL"),
        station: "dereth-client::gpu::rendering::terrain_modes::a_terrain_type_the_ground_region_does_not_name_takes_its_neighbours_ground",
        tier: Tier::Gpu,
    },
    behaviour! {
        id: "rendering.terrain.an-older-world-draws-the-ground-its-hardware-client-drew",
        says: "A world from before Throne of Destiny is drawn with the region its own client used \
               when drawing with 3D hardware: blended ground textures, that region's detail \
               textures and its sky. Asked for the software region instead, its ground is drawn by \
               recolouring a few shared ground pictures for each square and no detail texture is \
               drawn over the ground, because that region names none.",
        since: THIS_CLIENT,
        divergence: "CD-010",
        evidence: Evidence::Private("AC-EVID-LEGACY-TERRAIN-PALSHIFT"),
        station: "dereth-client::gpu::rendering::pre_tod_ground::an_older_world_draws_the_hardware_ground_and_on_request_the_software_one",
        tier: Tier::Gpu,
    },
    behaviour! {
        id: "rendering.terrain.every-ground-and-sky-style-draws-the-end-of-retail-world",
        says: "With the older data files given beside it, the end-of-retail world can be drawn with \
               any of the three terrain modes and any of the three skies, always with its own \
               scenery and buildings; its own Modern Blend and Modern sky draw exactly what it \
               draws by default.",
        since: THIS_CLIENT,
        divergence: "CD-012",
        evidence: Evidence::Private("AC-EVID-TERRAIN-MODES-EVERY-STYLE"),
        station: "dereth-client::gpu::rendering::terrain_modes::every_ground_and_sky_style_draws_the_end_of_retail_world",
        tier: Tier::Gpu,
    },
    behaviour! {
        id: "rendering.terrain.lod-seams-share-the-coarser-neighbours-edge",
        says: "Where a block of land meets the next, coarser block farther out, its outer edge \
               follows the coarser block's edge exactly, so no crack opens in the ground between \
               two levels of terrain detail.",
        since: RETAIL,
        evidence: Evidence::Private("AC-EVID-G41-TERRAIN"),
        station: "dereth-client::gpu::rendering::terrain_lod_seams::the_outward_edge_of_a_stitched_block_is_exactly_its_coarser_neighbours_polyline",
        tier: Tier::Gpu,
    },
    behaviour! {
        id: "rendering.terrain.the-alpha-map-mask-blends-between-textures",
        says: "The masks that blend one ground texture into the next at a cell's corners carry \
               their gradient in the transparency channel, so ground textures fade into each other \
               rather than meeting in hard-edged patches.",
        since: RETAIL,
        evidence: Evidence::Private("AC-EVID-TERRAIN-BLEND-TERRAIN"),
        station: "dereth-client::dat::rendering::terrain_blend::a_retail_alpha_map_carries_its_mask_in_the_alpha_channel",
        tier: Tier::Dat,
    },
    behaviour! {
        id: "rendering.terrain.the-detail-textures-follow-the-drawn-ground-style",
        says: "With detail textures turned on, every terrain mode draws them over the ground and                buildings, Palette Shift included: they come from the chosen style's own data                where it has them and from the world's own where it does not.",
        since: THIS_CLIENT,
        divergence: "CD-012",
        evidence: Evidence::Private("AC-EVID-TERRAIN-MODES-DETAIL"),
        station: "dereth-client::gpu::rendering::terrain_modes::the_detail_textures_follow_the_drawn_ground_style_and_palette_shift_draws_them_too",
        tier: Tier::Gpu,
    },
    behaviour! {
        id: "rendering.terrain.the-later-ground-draws-an-older-world-with-the-later-land-surface",
        says: "With the later ground chosen in the player's profile, a world from before Throne \
               of Destiny keeps its own land, scenery, buildings and objects but its ground is \
               drawn with the end-of-retail ground textures, blends and landscape detail texture; \
               the end-of-retail world with no older files beside it keeps its own ground.",
        since: THIS_CLIENT,
        divergence: "CD-012",
        evidence: Evidence::Private("AC-EVID-LEGACY-TERRAIN-LATER-GROUND"),
        station: "dereth-client::gpu::rendering::pre_tod_ground::the_later_ground_draws_an_older_world_with_the_later_land_surface_under_its_own_scenery",
        tier: Tier::Gpu,
    },
    behaviour! {
        id: "rendering.texture.a-constant-texel-reads-back-unchanged-through-every-world-pipeline",
        says: "A texture of one flat colour, stored plain or block-compressed, drawn across the \
               whole screen through each way world geometry is drawn, unlit or fully lit, comes \
               out on screen as exactly that colour.",
        since: RETAIL,
        evidence: Evidence::Private("AC-EVID-P189D-TEXTURE"),
        station: "dereth-render::gpu::rendering::texel_readback::a_constant_texel_reads_back_unchanged_through_every_world_permutation",
        tier: Tier::Gpu,
    },
    behaviour! {
        id: "rendering.textures.runtime-mips-preserve-channels-and-keep-their-owners-alive",
        says: "World textures get their smaller copies made when they are loaded, each keeping \
               every colour channel and transparency, for square, oblong, odd-sized and \
               single-pixel images alike.",
        since: RETAIL,
        evidence: Evidence::Private("AC-EVID-ASTRA-RUNTIME-MIPS-TEXTURES"),
        station: "dereth-render::lib::d3d12::tests::runtime_mips_preserve_every_channel_and_handle_rectangles_and_odd_extents",
        tier: Tier::Cpu,
    },
    behaviour! {
        id: "rendering.textures.solid-colour-textures-stay-bounded-per-session",
        says: "Surfaces with a flat colour and no picture get one small texture per distinct \
               colour, a session never makes more of them than the world data can name, and \
               walking back over the same ground adds none.",
        since: RETAIL,
        evidence: Evidence::Private("AC-EVID-O611-TEXTURES"),
        station: "dereth-client::gpu::rendering::solid_colour_textures::a_session_reaches_a_small_fraction_of_the_ceiling_and_stops_growing",
        tier: Tier::Gpu,
    },
    behaviour! {
        id: "rendering.textures.the-filtering-preference-is-read-before-the-first-draw",
        says: "The texture-filtering choice saved in the player's profile is already in force \
               before the client draws its first frame, the options page reports the same choice \
               once the screens are up, and reading the profile leaves the file byte for byte \
               unchanged.",
        since: RETAIL,
        evidence: Evidence::Private("AC-EVID-ASTRA-TEXTURE-SAMPLERS-TEXTURES-FILTERING"),
        station: "dereth-client::gpu::rendering::texture_filtering::app_loads_the_configured_choice_name_before_first_draw_without_writing_the_profile",
        tier: Tier::Gpu,
    },
    behaviour! {
        id: "rendering.textures.the-highres-dat-is-a-server-grant-and-the-level-a-preference",
        says: "Once the high-resolution textures are granted, the texture-detail preference \
               decides: only the highest setting draws a floor from its 256 by 256 high-resolution \
               art, and a medium setting keeps the original 128 by 128 art.",
        since: RETAIL,
        evidence: Evidence::Private("AC-EVID-P189E-TEXTURES"),
        station: "dereth-client::gpu::rendering::highres_texture_policy::with_the_grant_only_the_highest_preference_keeps_the_high_res_level",
        tier: Tier::Gpu,
    },
    behaviour! {
        id: "rendering.textures.the-sharp-flag-sharpens-minified-samples",
        says: "A model preview drawn in sharp mode samples its shrunken textures more crisply than \
               the same preview drawn plainly, but only while texture filtering is at one of its \
               two lower settings; at the higher settings sharp mode changes nothing, and the next \
               plain preview is exactly as before.",
        since: RETAIL,
        evidence: Evidence::Private("AC-EVID-ASTRA-TEXTURE-SAMPLERS-TEXTURES"),
        station: "dereth-client::gpu::rendering::texture_filtering::actual_dat_preview_sharp_flag_changes_minified_samples",
        tier: Tier::Gpu,
    },
    behaviour! {
        id: "rendering.textures.ui-and-world-producers-never-share-a-cache-key",
        says: "Interface pictures, world textures, font sheets and flat-colour textures are cached \
               apart: when all four would name the same underlying picture, each still gets its \
               own texture, so none can be drawn in place of another, and releasing one leaves the \
               others in place.",
        since: RETAIL,
        evidence: Evidence::Private("AC-EVID-O610-TEXTURES"),
        station: "dereth-client::gpu::rendering::texture_cache_key_spaces::one_payload_offered_to_every_producer_takes_four_slots",
        tier: Tier::Gpu,
    },
    behaviour! {
        id: "rendering.translucency.a-parts-translucency-ramp-reaches-the-raster",
        says: "As the player's body fades out, each part is drawn at an opacity of one minus the \
               fade, so a body a quarter of the way faded shows three quarters of itself over what \
               is behind it and one three quarters faded shows a quarter.",
        since: RETAIL,
        evidence: Evidence::Private("AC-EVID-O156-TRANSLUCENCY"),
        station: "dereth-client::gpu::rendering::part_translucency::the_ramp_reaches_the_raster_at_one_minus_t",
        tier: Tier::Gpu,
    },
    behaviour! {
        id: "rendering.translucency.authored-surface-translucency-changes-only-translucent-pixels",
        says: "Surfaces the art marks as partly see-through are drawn that way, and honouring it \
               changes the picture only where such a surface is: looking at the horizon only \
               pixels above it change, and looking down at ground with no such surface in view the \
               picture is identical either way.",
        since: RETAIL,
        evidence: Evidence::Private("AC-EVID-O101-TRANSLUCENCY"),
        station: "dereth-client::gpu::rendering::surface_translucency::the_pair_differs_only_where_a_translucent_surface_is_drawn",
        tier: Tier::Gpu,
    },
    behaviour! {
        id: "rendering.translucency.first-person-hides-the-body",
        says: "Putting the camera into first person hides the player's own body through the \
               ordinary per-frame camera update, while in third person the body is drawn solid and \
               unfaded.",
        since: RETAIL,
        evidence: Evidence::Private("AC-EVID-O156-TRANSLUCENCY-FIRST"),
        station: "dereth-client::gpu::rendering::part_translucency::first_person_hides_the_body_through_the_frames_own_camera_call",
        tier: Tier::Gpu,
    },
    behaviour! {
        id: "rendering.ui-sampler.blits-and-fills-bind-the-sampler-the-material-selects",
        says: "Each interface picture is sampled the way its own material asks: a picture drawn at \
               its own size is sampled by nearest texel, a stretched one is smoothed, flat fills \
               are sampled by nearest texel, and every sampler choice in the frame is accounted \
               for by those draws and the text.",
        since: RETAIL,
        evidence: Evidence::Private("AC-EVID-O388-UI-SAMPLER"),
        station: "dereth-client::gpu::rendering::ui_sampler_selection::each_blit_and_fill_binds_the_sampler_the_material_logic_selects",
        tier: Tier::Gpu,
    },
    behaviour! {
        id: "rendering.ui-sampler.every-glyph-draw-binds-point-clamp",
        says: "Every letter of interface text in a real frame is sampled from its font picture by \
               nearest texel and never smoothed, so text stays crisp at the size it is drawn.",
        since: RETAIL,
        evidence: Evidence::Private("AC-EVID-O370-UI-SAMPLER"),
        station: "dereth-client::gpu::rendering::ui_sampler_selection::every_glyph_draw_of_a_real_frame_binds_the_point_sampler",
        tier: Tier::Gpu,
    },
    behaviour! {
        id: "rendering.ui-sampler.u-and-v-address-modes-are-chosen-independently",
        says: "A tiled interface picture decides separately across and down whether it repeats: a \
               strip stretched along one axis repeats along that axis and stays clamped along the \
               other, and a picture drawn without tiling is clamped both ways.",
        since: RETAIL,
        evidence: Evidence::Private("AC-EVID-O443-UI-SAMPLER"),
        station: "dereth-client::gpu::rendering::ui_texture_addressing::the_material_refines_each_axis_against_its_own_repeat_count",
        tier: Tier::Gpu,
    },
    behaviour! {
        id: "rendering.viewport.the-scene-renders-inside-the-world-view-rectangle",
        says: "The 3D world is drawn only inside the world-view rectangle the interface leaves for \
               it: pixels outside the rectangle stay untouched, and inside it the world is framed \
               for that rectangle rather than being a crop of a full-window view.",
        since: RETAIL,
        evidence: Evidence::Private("AC-EVID-F19-VIEWPORT"),
        station: "dereth-client::gpu::rendering::game_viewport::the_scene_renders_inside_the_world_view_rectangle",
        tier: Tier::Gpu,
    },
    behaviour! {
        id: "rendering.weather.rain-falls-across-the-whole-view",
        says: "On a rainy day the rain falls across the whole view, all the way to the bottom of \
               the screen, rather than stopping at the horizon halfway down.",
        since: RETAIL,
        evidence: Evidence::Private("AC-EVID-RAIN-WEATHER"),
        station: "dereth-client::gpu::rendering::rain_extent::the_weather_layer_falls_across_the_whole_view",
        tier: Tier::Gpu,
    },
    behaviour! {
        id: "terrain.split.drawn-and-walked-triangles-use-one-diagonal-rule",
        says: "Every landscape cell in the world is cut into its two triangles along the same \
               diagonal whether the ground is being drawn or walked on, so the player never stands \
               on ground that is not where it is drawn; over the whole world the two diagonals \
               each take close to half the cells.",
        since: RETAIL,
        evidence: Evidence::Private("AC-EVID-CROSS-TRACK-SPLIT"),
        station: "dereth-client::cpu::rendering::terrain_split_rule::the_terrain_split_rule_agrees_between_rendering_and_physics",
        tier: Tier::Cpu,
    },
];
