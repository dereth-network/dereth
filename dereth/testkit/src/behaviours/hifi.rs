//! The optional high-fidelity presentation -- a second, modern drawing of the world that changes
//! pixels only: that it is off and absent unless asked for, and that what it is handed cannot
//! reach back into the game.
//!
//! One file per subject, so that two changes adding rows at the same time do not edit the same
//! file. [`ROWS`] is in id order; the registry's own test asserts that, and that no id and no
//! evidence handle is repeated anywhere in it.

// `behaviour!` is `#[macro_export]`ed by `mod.rs` above this module's declaration, so it is in
// textual scope here and needs no import.
use super::{Behaviour, Evidence, Tier, THIS_CLIENT, TOOLING};

/// This subject's rows, in id order.
pub static ROWS: &[Behaviour] = &[
    behaviour! {
        id: "hifi.build.the-desktop-client-carries-the-effects-and-the-browser-client-never-does",
        says: "The desktop client as ordinarily built carries the experimental rendering \
               effects; the browser client never asks for them, and nothing it depends on \
               brings them in for its target.",
        since: THIS_CLIENT,
        divergence: "CD-037",
        evidence: Evidence::Private("AC-EVID-HIFI-BUILD-DEFAULT"),
        station: "dereth-client::cpu::rendering::hifi_default_off::the_desktop_build_carries_the_effects_and_the_browser_build_never_does",
        tier: Tier::Cpu,
    },
    behaviour! {
        id: "hifi.camera.positions-come-back-from-depth",
        says: "A point projected with the frame's camera comes back from its depth to where it \
               was, and reprojects to where the previous camera saw it.",
        since: TOOLING,
        evidence: Evidence::Private("AC-EVID-HIFI-CAMERA"),
        station: "dereth-render-hifi::cpu::shared::camera::a_point_comes_back_from_its_depth_and_reprojects_into_the_previous_frame",
        tier: Tier::Cpu,
    },
    behaviour! {
        id: "hifi.defaults.with-default-settings-no-pass-runs",
        says: "With the high-fidelity settings at their defaults the presentation would run no \
               pass and change nothing.",
        since: THIS_CLIENT,
        divergence: "CD-037",
        evidence: Evidence::Private("AC-EVID-HIFI-DEFAULT-SETTINGS"),
        station: "dereth-render-hifi::cpu::graph::defaults::with_default_settings_no_pass_is_wanted_and_nothing_is_effective",
        tier: Tier::Cpu,
    },
    behaviour! {
        id: "hifi.derive.every-anchor-matches-once-in-every-permutation",
        says: "Each replacement that turns an ordinary shader into a high-fidelity one finds its \
               text exactly once, in every shader it is applied to and nowhere else, and a shader \
               that has drifted fails to derive rather than deriving wrongly.",
        since: TOOLING,
        evidence: Evidence::Private("AC-EVID-HIFI-DERIVE-ANCHORS"),
        station: "dereth-render-hifi::cpu::derive::anchors::every_anchor_matches_once_where_it_applies_and_nowhere_else",
        tier: Tier::Cpu,
    },
    behaviour! {
        id: "hifi.derive.every-derived-shader-validates",
        says: "Every shader derived for the high-fidelity presentation parses and validates \
               without a device, with the entry points and outputs its pipelines expect.",
        since: TOOLING,
        evidence: Evidence::Private("AC-EVID-HIFI-DERIVE-VALID"),
        station: "dereth-render-hifi::cpu::derive::validation::every_derived_permutation_parses_and_validates_with_two_outputs_per_pixel_stage",
        tier: Tier::Cpu,
    },
    behaviour! {
        id: "hifi.graph.several-passes-share-the-world-replay",
        says: "Several passes acting on the world replay share it: each draw goes to the first \
               that does something other than replay it as recorded.",
        since: TOOLING,
        evidence: Evidence::Private("AC-EVID-HIFI-CHAIN"),
        station: "dereth-render-hifi::cpu::graph::chain::each_draw_goes_to_the_first_filter_that_acts_on_it",
        tier: Tier::Cpu,
    },
    behaviour! {
        id: "hifi.graph.the-passes-run-in-one-fixed-order",
        says: "Every pass of the high-fidelity presentation has one place in one fixed order, and \
               every option has its pass.",
        since: TOOLING,
        evidence: Evidence::Private("AC-EVID-HIFI-PASS-ORDER"),
        station: "dereth-render-hifi::cpu::graph::pass_order::every_pass_has_one_slot_and_the_slots_run_in_the_fixed_order",
        tier: Tier::Cpu,
    },
    behaviour! {
        id: "hifi.lamps.no-lamp-is-looked-for-unless-the-lamps-are-drawn",
        says: "A block's bake reads nothing for the lamps of its placements; they are looked \
               for only while the high-fidelity presentation draws the lamps.",
        since: THIS_CLIENT,
        divergence: "CD-037",
        evidence: Evidence::Private("AC-EVID-HIFI-LAMP-PLACEMENT"),
        station: "dereth-scene::lib::world_scene::imp::hifi_lamps::tests::a_baked_block_reads_nothing_for_its_lamps_until_they_are_asked_for",
        tier: Tier::Cpu,
    },
    behaviour! {
        id: "hifi.lighting.at-dusk-a-dark-canopy-does-not-glow",
        says: "At dusk, with the high-fidelity lighting alone or every box ticked, the trees the \
               authored light draws as dark shapes against the sky stay dark: no more than a \
               few tenths of a percent more of the frame is lit up lime.",
        since: THIS_CLIENT,
        divergence: "CD-037",
        evidence: Evidence::Private("AC-EVID-HIFI-DUSK-CANOPY"),
        station: "dereth-client::gpu::rendering::hifi_lighting::at_dusk_a_canopy_the_authored_light_draws_dark_does_not_glow",
        tier: Tier::Gpu,
    },
    behaviour! {
        id: "hifi.lighting.every-box-still-draws-the-occlusion-indoors",
        says: "Underground, in a house's room and at a doorway seen from the room, every \
               high-fidelity box ticked still draws the ambient occlusion: the bounced light, \
               which draws nothing there, does not switch it off, and a room drawn after the \
               frame steps indoors still takes it.",
        since: THIS_CLIENT,
        divergence: "CD-037",
        evidence: Evidence::Private("AC-EVID-HIFI-INDOOR-OCCLUSION"),
        station: "dereth-client::gpu::rendering::hifi_lighting::with_every_box_ticked_the_occlusion_is_still_drawn_indoors_and_underground",
        tier: Tier::Gpu,
    },
    behaviour! {
        id: "hifi.lighting.every-surface-shader-derives-and-validates",
        says: "Every surface permutation derived for the lighting parses and validates without a \
               device, each of its replacements found once, writing the four surface targets.",
        since: TOOLING,
        evidence: Evidence::Private("AC-EVID-HIFI-SURFACE-DERIVE"),
        station: "dereth-render-hifi::cpu::derive::surface::every_surface_permutation_derives_once_per_anchor_and_writes_four_targets",
        tier: Tier::Cpu,
    },
    behaviour! {
        id: "hifi.lighting.the-light-that-follows-the-player-pools-on-no-outdoor-ground",
        says: "At night in the open, with the high-fidelity lighting, the ground round the body \
               is as bright with the light that follows the player as without it, as the \
               ordinary frame draws it: outdoors the landscape and everything on it are lit by \
               the sun alone. The body itself keeps at least nine tenths of the share of its \
               ordinary brightness that the ground keeps.",
        since: THIS_CLIENT,
        divergence: "CD-037",
        evidence: Evidence::Private("AC-EVID-HIFI-VIEWER-LIGHT"),
        station: "dereth-client::gpu::rendering::hifi_lighting::at_night_in_the_open_the_light_that_follows_the_player_leaves_the_ground_as_drawn",
        tier: Tier::Gpu,
    },
    behaviour! {
        id: "hifi.off.a-failure-falls-back-to-identical-pixels-and-uninstalls",
        says: "A high-fidelity presentation that fails, by its own error or by a step the \
               device refuses, leaves that frame exactly the ordinary frame and is removed, and \
               is not put back until the preferences change.",
        since: THIS_CLIENT,
        divergence: "CD-037",
        evidence: Evidence::Private("AC-EVID-HIFI-FALLBACK"),
        station: "dereth-client::gpu::rendering::hifi_off::presentation::a_failing_presentation_draws_the_ordinary_frame_and_is_removed",
        tier: Tier::Gpu,
    },
    behaviour! {
        id: "hifi.off.leaving-horizon-with-every-box-ticked-restores-everything",
        says: "A frame drawn with every high-fidelity box ticked under Horizon and then left by \
               leaving Horizon is, by day, at night, under the lamps, at a doorway and \
               underground, the frame drawn before: the same pixels, recording, draws, \
               textures, texture slots, render passes and scene census, and every object the \
               device holds by its own count, its memory included.",
        since: THIS_CLIENT,
        divergence: "CD-037",
        evidence: Evidence::Private("AC-EVID-HIFI-LEAVE-HORIZON"),
        station: "dereth-client::gpu::rendering::hifi_off::presentation::leaving_horizon_with_every_box_ticked_restores_every_pixel_counter_and_device_object",
        tier: Tier::Gpu,
    },
    behaviour! {
        id: "hifi.off.passthrough-is-pixel-identical",
        says: "At every capture station, the frame drawn through the high-fidelity presentation \
               with no pass of its own is the ordinary frame, byte for byte.",
        since: THIS_CLIENT,
        divergence: "CD-037",
        evidence: Evidence::Private("AC-EVID-HIFI-PASSTHROUGH"),
        station: "dereth-client::gpu::rendering::hifi_off::presentation::passthrough_draws_every_station_byte_for_byte",
        tier: Tier::Gpu,
    },
    behaviour! {
        id: "hifi.off.the-device-request-is-unchanged-with-the-presentation-off",
        says: "A device made without the high-fidelity request has exactly the ordinary \
               device's features and limits, one made with it adds only the presentation's own, \
               and neither, nor a frame drawn through the presentation, changes how \
               floating-point arithmetic rounds, keeps subnormal results, reads subnormal \
               operands or answers a division by zero, an invalid operation or an overflow.",
        since: THIS_CLIENT,
        divergence: "CD-037",
        evidence: Evidence::Private("AC-EVID-HIFI-DEVICE-REQUEST"),
        station: "dereth-client::gpu::rendering::hifi_off::presentation::the_device_request_and_the_float_environment_are_unchanged_with_the_presentation_off",
        tier: Tier::Gpu,
    },
    behaviour! {
        id: "hifi.off.the-legacy-shader-text-is-pinned",
        says: "The ordinary shaders are exactly the text they were before the high-fidelity \
               presentation existed, in every permutation, so nothing it adds changes how the \
               ordinary frame is shaded.",
        since: THIS_CLIENT,
        divergence: "CD-037",
        evidence: Evidence::Private("AC-EVID-HIFI-LEGACY-PINS"),
        station: "dereth-render-hifi::cpu::derive::legacy_shader_pins::the_ordinary_shader_text_is_unchanged_for_every_permutation",
        tier: Tier::Cpu,
    },
    behaviour! {
        id: "hifi.off.the-recorded-frame-and-pixels-match-the-base-build",
        says: "Every capture station, drawn with the high-fidelity presentation off or not \
               built in, records the same frame, draws the same pixels and counts the same \
               draws, textures, texture slots, render passes, frames and scene census as the \
               build before the presentation existed, on the same device and driver.",
        since: THIS_CLIENT,
        divergence: "CD-037",
        evidence: Evidence::Private("AC-EVID-HIFI-BASE-IDENTITY"),
        station: "dereth-client::gpu::rendering::hifi_off::every_station_records_and_draws_what_the_base_build_did",
        tier: Tier::Gpu,
    },
    behaviour! {
        id: "hifi.off.toggling-off-restores-every-pixel-and-counter",
        says: "Turning the high-fidelity presentation on and off again over a still frame \
               leaves every pixel, the recorded frame and every device counter as they were, \
               every object the device holds by its own count included, with one render pass a \
               frame while it is off.",
        since: THIS_CLIENT,
        divergence: "CD-037",
        evidence: Evidence::Private("AC-EVID-HIFI-TOGGLES"),
        station: "dereth-client::gpu::rendering::hifi_off::presentation::toggling_the_presentation_restores_every_pixel_recording_and_counter",
        tier: Tier::Gpu,
    },
    behaviour! {
        id: "hifi.off.unticking-an-effect-gives-back-what-it-held",
        says: "Unticking a high-fidelity box gives back the texture, buffer and traced-scene \
               memory its effect held at once: the better lighting alone after every box holds \
               what it held before them.",
        since: THIS_CLIENT,
        divergence: "CD-037",
        evidence: Evidence::Private("AC-EVID-HIFI-UNTICK-MEMORY"),
        station: "dereth-client::gpu::rendering::hifi_off::presentation::unticking_an_effect_gives_back_the_memory_it_held",
        tier: Tier::Gpu,
    },
    behaviour! {
        id: "hifi.options.a-live-fidelity-change-is-kept-for-the-next-world",
        says: "A high-fidelity option changed while playing is recorded in the scene the next \
               world is built from, as the landscape options are, so a second login keeps it; \
               the flag that says the Horizon interface is shown is not an option and is not \
               recorded so.",
        since: THIS_CLIENT,
        divergence: "CD-037",
        evidence: Evidence::Private("AC-EVID-HIFI-NEXT-WORLD"),
        station: "dereth-client-runtime::lib::app::tests::a_live_fidelity_or_landscape_change_is_kept_for_the_next_world",
        tier: Tier::Cpu,
    },
    behaviour! {
        id: "hifi.options.every-fidelity-option-is-off-by-default",
        says: "Every high-fidelity option is off in a fresh profile, and none is in effect \
               unless the Horizon interface is shown: a profile that ticks every box and starts \
               in the classic or the modern interface has nothing in effect. An older build's \
               levels and options this build does not have are not read, and are no error.",
        since: THIS_CLIENT,
        divergence: "CD-037",
        evidence: Evidence::Private("AC-EVID-HIFI-PREFS-DEFAULT"),
        station: "dereth-client-runtime::lib::render_prefs::tests::every_fidelity_option_is_off_by_default_and_none_is_in_effect_outside_horizon",
        tier: Tier::Cpu,
    },
    behaviour! {
        id: "hifi.options.no-combination-of-the-boxes-draws-what-cannot-draw",
        says: "Of every combination of the six boxes, on a device with ray tracing and without, \
               each box draws exactly when it is ticked and the options page does not grey it: \
               the shadows, the bounced light and the lamps draw nothing without the better \
               lighting, the lamps nothing without ray tracing, the ambient occlusion nothing \
               while the bounced light covers it; nothing is re-shaded without the better \
               lighting; and a box whose prerequisite comes back draws again.",
        since: THIS_CLIENT,
        divergence: "CD-037",
        evidence: Evidence::Private("AC-EVID-HIFI-DEPENDENCIES"),
        station: "dereth-scene::lib::world_scene::imp::hifi_bridge::tests::every_combination_of_the_boxes_draws_only_what_its_prerequisites_allow",
        tier: Tier::Cpu,
    },
    behaviour! {
        id: "hifi.options.no-sidecar-without-an-effective-feature",
        says: "The scene installs the high-fidelity presentation only while an option is in \
               effect under the Horizon interface: not for every box ticked under another \
               interface, and it removes it when Horizon is left or the last option goes. A \
               wgpu device made without the presentation's request, as one started in another \
               interface is, refuses it, and the poll that asks for it says so.",
        since: THIS_CLIENT,
        divergence: "CD-037",
        evidence: Evidence::Private("AC-EVID-HIFI-INSTALL"),
        station: "dereth-client::gpu::rendering::hifi_off::presentation::the_scene_installs_the_presentation_only_with_an_effective_option",
        tier: Tier::Gpu,
    },
    behaviour! {
        id: "hifi.options.the-boxes-are-on-horizons-page-alone-each-off",
        says: "The experimental rendering effects' six boxes are on the Horizon interface's \
               Client page alone, last, under a heading that calls them highly experimental, \
               each off by default; the classic and the modern interfaces' pages are exactly \
               what they are without the effects; and a build without the effects has no such \
               heading.",
        since: THIS_CLIENT,
        divergence: "CD-037",
        evidence: Evidence::Private("AC-EVID-HIFI-HORIZON-PAGE"),
        station: "dereth-client-contract::lib::options::sheet::tests::the_effects_boxes_are_on_horizons_client_page_alone_each_off",
        tier: Tier::Cpu,
    },
    behaviour! {
        id: "hifi.options.the-fidelity-section-reaches-the-render-preferences",
        says: "The [Fidelity] options reach the render preferences from the profile and by name \
               on the live path, all off by default, with the flag that says the Horizon \
               interface is shown.",
        since: THIS_CLIENT,
        divergence: "CD-037",
        evidence: Evidence::Private("AC-EVID-HIFI-OPTIONS"),
        station: "dereth-client-runtime::lib::render_prefs::tests::the_render_preferences_carry_the_fidelity_section_from_the_file_and_by_name",
        tier: Tier::Cpu,
    },
    behaviour! {
        id: "hifi.options.the-presentation-settings-follow-the-preferences-in-effect",
        says: "The presentation is told exactly the options in effect: none outside the Horizon \
               interface, whatever the profile ticks, and each ticked box's effect under it.",
        since: THIS_CLIENT,
        divergence: "CD-037",
        evidence: Evidence::Private("AC-EVID-HIFI-BRIDGE-SETTINGS"),
        station: "dereth-scene::lib::world_scene::imp::hifi_bridge::tests::the_settings_are_off_outside_horizon_and_follow_each_box_inside_it",
        tier: Tier::Cpu,
    },
    behaviour! {
        id: "hifi.reshade.a-frame-is-re-shaded-up-to-its-indoor-step",
        says: "A frame drawn re-shaded is re-shaded from the start of the world to its end, its \
               translucent draws after the rest; a frame that steps indoors is re-shaded up to its \
               first indoor flush, indoor cell or depth clear and drawn as recorded from there.",
        since: THIS_CLIENT,
        divergence: "CD-037",
        evidence: Evidence::Private("AC-EVID-HIFI-RESHADE-PLAN"),
        station: "dereth-render-hifi::cpu::reshade::plan::an_outdoor_frame_is_re_shaded_whole_and_an_indoor_step_ends_the_re_shade",
        tier: Tier::Cpu,
    },
    behaviour! {
        id: "hifi.reshade.an-indoor-frame-re-shades-only-its-outdoor-part",
        says: "At the stations that step indoors, the frame with re-shading on draws the world \
               after the step with its own pipelines over the resolved picture and matches the \
               ordinary frame within the outdoor stations' bounds.",
        since: THIS_CLIENT,
        divergence: "CD-037",
        evidence: Evidence::Private("AC-EVID-HIFI-RESHADE-SPLIT"),
        station: "dereth-client::gpu::rendering::hifi_reshade::an_indoor_frame_is_re_shaded_up_to_its_step_and_drawn_as_recorded_after",
        tier: Tier::Gpu,
    },
    behaviour! {
        id: "hifi.reshade.enabling-never-holds-up-a-frame",
        says: "Turning re-shading on never holds a frame up: its pipelines are built off the frame, \
               no frame from the change on takes longer than 50 milliseconds, and the frames are \
               drawn the ordinary way until every pipeline is ready.",
        since: THIS_CLIENT,
        divergence: "CD-037",
        evidence: Evidence::Private("AC-EVID-HIFI-RESHADE-ENABLE"),
        station: "dereth-client::gpu::rendering::hifi_reshade::turning_re_shading_on_never_holds_a_frame_past_fifty_milliseconds",
        tier: Tier::Gpu,
    },
    behaviour! {
        id: "hifi.reshade.every-world-draw-has-an-action",
        says: "Every noted draw of the world has an action in the re-shade, whatever its vertex \
               format and whether or not it is a landscape splat; a draw with no note keeps the \
               frame from being re-shaded.",
        since: TOOLING,
        evidence: Evidence::Private("AC-EVID-HIFI-RESHADE-CLASSIFY"),
        station: "dereth-render-hifi::cpu::reshade::plan::every_noted_world_draw_is_classified_and_an_unnoted_one_holds_the_frame_back",
        tier: Tier::Cpu,
    },
    behaviour! {
        id: "hifi.reshade.the-landscape-takes-the-fields-smooth-normals",
        says: "In the re-shaded world, the drawn ground is found on the landscape field from the \
               depth and given the field's smooth normal and the landscape class: most of the \
               lower third of the view at the town, shore, forest and vista stations.",
        since: THIS_CLIENT,
        divergence: "CD-037",
        evidence: Evidence::Private("AC-EVID-HIFI-RESHADE-GROUND"),
        station: "dereth-client::gpu::rendering::hifi_reshade::the_ground_in_view_takes_the_landscape_fields_normal_and_class",
        tier: Tier::Gpu,
    },
    behaviour! {
        id: "hifi.reshade.the-parity-view-matches-the-ordinary-frame",
        says: "At every outdoor station the re-shaded world written back through the neutral \
               resolve differs from the ordinary frame by less than two levels on average and in \
               fewer than half a percent of pixels by outline, with every draw of the world \
               re-shaded.",
        since: THIS_CLIENT,
        divergence: "CD-037",
        evidence: Evidence::Private("AC-EVID-HIFI-RESHADE-PARITY"),
        station: "dereth-client::gpu::rendering::hifi_reshade::the_parity_view_matches_the_ordinary_frame_at_every_outdoor_station",
        tier: Tier::Gpu,
    },
    behaviour! {
        id: "hifi.resources.the-shared-resources-keep-to-their-budget",
        says: "The high-fidelity presentation's shared resources are charged against a video \
               memory budget, refused past it, kept while a frame asks for them and released \
               after a quiet spell.",
        since: TOOLING,
        evidence: Evidence::Private("AC-EVID-HIFI-RESOURCES"),
        station: "dereth-render-hifi::cpu::shared::resources::the_shared_resources_keep_to_their_budget_and_go_when_unused",
        tier: Tier::Cpu,
    },
    behaviour! {
        id: "hifi.shaders.the-composite-shader-validates",
        says: "The shaders the high-fidelity composite and the depth each pixel sees are drawn \
               with parse and validate without a device, and the second looks for exactly the \
               depth a building's stamp writes.",
        since: TOOLING,
        evidence: Evidence::Private("AC-EVID-HIFI-COMPOSITE-SHADER"),
        station: "dereth-render-hifi::cpu::shared::shaders::the_composite_shader_parses_and_validates",
        tier: Tier::Cpu,
    },
    behaviour! {
        id: "hifi.shaders.the-re-shade-shaders-validate",
        says: "The shaders of the neutral resolve, the normal and census views and the \
               landscape-normal pass parse and validate without a device.",
        since: TOOLING,
        evidence: Evidence::Private("AC-EVID-HIFI-RESHADE-SHADERS"),
        station: "dereth-render-hifi::cpu::shared::shaders::the_re_shade_shaders_parse_and_validate",
        tier: Tier::Cpu,
    },
    behaviour! {
        id: "hifi.shaders.the-shared-shader-code-validates",
        says: "The shader code the high-fidelity passes share parses and validates on its own, \
               without a device.",
        since: TOOLING,
        evidence: Evidence::Private("AC-EVID-HIFI-SHARED-SHADERS"),
        station: "dereth-render-hifi::cpu::shared::shaders::every_shared_shader_parses_and_validates",
        tier: Tier::Cpu,
    },
    behaviour! {
        id: "hifi.sky.a-rainy-sky-is-no-brighter-than-the-authored-one",
        says: "On a rainy day the sky, the clouds and the rain over it are on the whole no \
               brighter with the physical sky than the authored sky is.",
        since: THIS_CLIENT,
        divergence: "CD-037",
        evidence: Evidence::Private("AC-EVID-HIFI-SKY-RAIN"),
        station: "dereth-client::gpu::rendering::hifi_atmosphere::a_rainy_sky_is_no_brighter_than_the_authored_one",
        tier: Tier::Gpu,
    },
    behaviour! {
        id: "hifi.sky.at-dusk-the-near-field-keeps-its-colour",
        says: "At dusk the near land toward the low sun is drawn exactly as it is with the air \
               off, and a near tree drawn over the glow stays dark against the sky behind it.",
        since: THIS_CLIENT,
        divergence: "CD-037",
        evidence: Evidence::Private("AC-EVID-HIFI-SKY-DUSK"),
        station: "dereth-client::gpu::rendering::hifi_atmosphere::at_dusk_the_near_land_toward_the_sun_keeps_its_colour_and_a_near_tree_its_silhouette",
        tier: Tier::Gpu,
    },
    behaviour! {
        id: "hifi.sky.from-inside-a-room-the-depth-through-its-openings-is-the-outdoors",
        says: "In a frame split between a room and the outdoors, the depth the effects read and \
               the occlusion they draw are, through the room's door, the outdoors', not the flat \
               depth of the opening the room stamps there: as they are with the room drawn with \
               no depth clear, whether or not the frame is re-shaded.",
        since: THIS_CLIENT,
        divergence: "CD-037",
        evidence: Evidence::Private("AC-EVID-HIFI-SKY-ROOM-OPENINGS"),
        station: "dereth-client::gpu::rendering::hifi_atmosphere::from_inside_a_room_the_depth_through_its_openings_is_the_outdoors",
        tier: Tier::Gpu,
    },
    behaviour! {
        id: "hifi.sky.hills-at-the-edge-of-the-land-fade-evenly",
        says: "Hills reaching the edge of the resident landscape fade evenly into the haze: the \
               land below a crest never brightens against the crest by more than a few levels \
               beyond what it does with the air off.",
        since: THIS_CLIENT,
        divergence: "CD-037",
        evidence: Evidence::Private("AC-EVID-HIFI-SKY-EDGE"),
        station: "dereth-client::gpu::rendering::hifi_atmosphere::hills_at_the_edge_of_the_land_fade_evenly_with_no_band_of_mist_below_their_crest",
        tier: Tier::Gpu,
    },
    behaviour! {
        id: "hifi.sky.interiors-and-split-frames-are-drawn-as-they-always-are",
        says: "With the physical sky and the air on, a body inside a room, and a body in a \
               doorway with its camera in the room, are drawn exactly as they are with the \
               high-fidelity presentation off.",
        since: THIS_CLIENT,
        divergence: "CD-037",
        evidence: Evidence::Private("AC-EVID-HIFI-SKY-INTERIORS"),
        station: "dereth-client::gpu::rendering::hifi_atmosphere::interiors_and_split_frames_are_drawn_as_they_always_are",
        tier: Tier::Gpu,
    },
    behaviour! {
        id: "hifi.sky.land-seen-through-a-building-keeps-its-colour",
        says: "From outdoors, the land seen through a building's window and on out through the \
               window across the room keeps the colour it has with the air off: none of it \
               inside the openings turns to the haze of the far distance.",
        since: THIS_CLIENT,
        divergence: "CD-037",
        evidence: Evidence::Private("AC-EVID-HIFI-SKY-THROUGH-WINDOW"),
        station: "dereth-client::gpu::rendering::hifi_atmosphere::land_seen_through_a_buildings_windows_keeps_its_colour",
        tier: Tier::Gpu,
    },
    behaviour! {
        id: "hifi.sky.positions-come-back-height-up",
        says: "A point the frame's camera projects comes back from its depth as the snapshot's \
               height-up point, and a view ray from the screen points at it with no part of the \
               eye's position in it.",
        since: TOOLING,
        evidence: Evidence::Private("AC-EVID-HIFI-SKY-HEIGHT-UP"),
        station: "dereth-render-hifi::cpu::sky::atmosphere::a_projected_point_comes_back_height_up_and_a_ray_points_at_it",
        tier: Tier::Cpu,
    },
    behaviour! {
        id: "hifi.sky.the-authored-night-sky-and-its-stars-are-kept",
        says: "At midnight the physical sky draws nothing over the authored dark sky: high in the \
               sky the picture is the ordinary one to within dither, with its stars, and the \
               haze's veil over the horizon stays darker than the hazed land, so the hills keep \
               at least half their line against the sky.",
        since: THIS_CLIENT,
        divergence: "CD-037",
        evidence: Evidence::Private("AC-EVID-HIFI-SKY-NIGHT"),
        station: "dereth-client::gpu::rendering::hifi_atmosphere::at_night_the_authored_sky_and_its_stars_are_kept",
        tier: Tier::Gpu,
    },
    behaviour! {
        id: "hifi.sky.the-farther-the-land-the-more-it-takes-the-horizons-colour",
        says: "Over the wide view from high ground, the farther the land, the nearer its colour \
               comes to the sky's just over the horizon, until the farthest land is well under \
               two thirds as far from it as the nearest.",
        since: THIS_CLIENT,
        divergence: "CD-037",
        evidence: Evidence::Private("AC-EVID-HIFI-SKY-AERIAL"),
        station: "dereth-client::gpu::rendering::hifi_atmosphere::the_farther_the_land_the_more_it_takes_the_colour_of_the_sky_over_the_horizon",
        tier: Tier::Gpu,
    },
    behaviour! {
        id: "hifi.sky.the-haze-is-calibrated-to-the-authored-fog",
        says: "The high-fidelity haze leaves the land all its light short of where the authored \
               fog begins and never nearer than its own least start, less the farther and the \
               lower the land, and the weather's share at the authored fog's end; a rainy day and \
               the night's short fog make a thicker haze, and an overcast sky is exposed darker.",
        since: THIS_CLIENT,
        divergence: "CD-037",
        evidence: Evidence::Private("AC-EVID-HIFI-SKY-HAZE"),
        station: "dereth-render-hifi::cpu::sky::atmosphere::the_haze_takes_more_light_the_farther_and_lower_the_land_and_meets_its_calibration",
        tier: Tier::Cpu,
    },
    behaviour! {
        id: "hifi.sky.the-physical-sky-follows-the-authored-day",
        says: "The physical sky takes the whole sky by day, none of it at night, when the \
               authored dome is dim, and part of it at dusk; only the day group's first plain sky \
               object is its dome.",
        since: THIS_CLIENT,
        divergence: "CD-037",
        evidence: Evidence::Private("AC-EVID-HIFI-SKY-DAY"),
        station: "dereth-render-hifi::cpu::sky::atmosphere::the_physical_sky_takes_the_day_leaves_the_night_and_shares_the_dusk",
        tier: Tier::Cpu,
    },
    behaviour! {
        id: "hifi.sky.the-physical-sky-takes-the-domes-place-under-the-clouds",
        says: "On a sunny noon the physical sky changes the sky over the authored dome, the \
               clouds are still drawn over it with their texture, the world is replayed without \
               its fog, and the scene records exactly the frame it records with the presentation \
               off.",
        since: THIS_CLIENT,
        divergence: "CD-037",
        evidence: Evidence::Private("AC-EVID-HIFI-SKY-DOME"),
        station: "dereth-client::gpu::rendering::hifi_atmosphere::the_physical_sky_takes_the_domes_place_and_the_clouds_stay_over_it",
        tier: Tier::Gpu,
    },
    behaviour! {
        id: "hifi.sky.the-shaders-haze-is-the-calibrated-haze",
        says: "The haze the air's shader works out on the device, for land at any distance below, \
               level with and above the eye, by day, in the rain and at night, is the haze the \
               model is calibrated with.",
        since: TOOLING,
        evidence: Evidence::Private("AC-EVID-HIFI-SKY-SHADER-HAZE"),
        station: "dereth-render-hifi::gpu::atmosphere::the_shaders_haze_is_the_haze_the_model_is_calibrated_with",
        tier: Tier::Gpu,
    },
    behaviour! {
        id: "hifi.sky.the-sky-shaders-validate",
        says: "Every shader of the physical sky and the air parses, validates and has its entry \
               points.",
        since: TOOLING,
        evidence: Evidence::Private("AC-EVID-HIFI-SKY-SHADERS"),
        station: "dereth-render-hifi::cpu::sky::atmosphere::every_sky_and_air_shader_parses_validates_and_has_its_entry_points",
        tier: Tier::Cpu,
    },
    behaviour! {
        id: "hifi.snapshot.the-frame-snapshot-holds-no-borrow",
        says: "What the scene hands the high-fidelity presentation about a frame is a plain owned \
               copy that holds no reference back into the game, so the presentation cannot change \
               anything the game reads.",
        since: THIS_CLIENT,
        divergence: "CD-037",
        evidence: Evidence::Private("AC-EVID-HIFI-SNAPSHOT-PLAIN"),
        station: "dereth-render-hifi::cpu::graph::snapshot::a_frame_snapshot_moves_to_another_thread_and_back_unchanged",
        tier: Tier::Cpu,
    },
    behaviour! {
        id: "hifi.startup.a-device-that-cannot-draw-the-presentation-says-so",
        says: "A player who turns the high-fidelity presentation on while the client runs on a \
               device that cannot draw it is told so once, on the channel the client's own \
               refusals use, and the presentation stays off.",
        since: THIS_CLIENT,
        divergence: "CD-037",
        evidence: Evidence::Private("AC-EVID-HIFI-REFUSED-NOTICE"),
        station: "dereth-client::gpu::rendering::hifi_visual_only::turning_the_presentation_on_where_it_cannot_draw_tells_the_player_once",
        tier: Tier::Gpu,
    },
    behaviour! {
        id: "hifi.startup.the-device-is-widened-only-on-wgpu-in-horizon-with-a-box-ticked",
        says: "At start-up the client's device is the named backend or the default, whatever \
               boxes the profile ticks: the effects never move it. Only a client that starts in \
               the Horizon interface on the wgpu device with a box ticked has the device asked \
               for what the presentation draws with; every other start-up, and a build without \
               the presentation, asks for the ordinary device.",
        since: THIS_CLIENT,
        divergence: "CD-037",
        evidence: Evidence::Private("AC-EVID-HIFI-STARTUP-DEVICE"),
        station: "dereth-client-shell::lib::app::tests::the_start_up_device_is_never_moved_and_is_widened_only_on_wgpu_in_horizon_with_a_box_ticked",
        tier: Tier::Cpu,
    },
    behaviour! {
        id: "hifi.terrain-field.the-diagonal-is-the-landscapes-own",
        says: "The landscape field's cell diagonal, on the CPU and as its shader computes it, is \
               the landscape's own everywhere on the map.",
        since: TOOLING,
        evidence: Evidence::Private("AC-EVID-HIFI-FIELD-DIAGONAL"),
        station: "dereth-render-hifi::cpu::reshade::terrain_field::the_field_diagonal_is_the_landscape_diagonal_in_the_shader_and_on_the_cpu",
        tier: Tier::Cpu,
    },
    behaviour! {
        id: "hifi.terrain-field.the-field-is-seamless-across-blocks",
        says: "The landscape field holds one height per landscape vertex across block edges, a \
               full-detail block's over a coarser neighbour's, with ground classes from the terrain \
               words.",
        since: TOOLING,
        evidence: Evidence::Private("AC-EVID-HIFI-FIELD-SEAMLESS"),
        station: "dereth-render-hifi::cpu::reshade::terrain_field::the_field_holds_one_height_per_vertex_and_the_finer_block_wins_an_edge",
        tier: Tier::Cpu,
    },
    behaviour! {
        id: "hifi.terrain-field.the-plane-height-is-the-drawn-landscapes",
        says: "The landscape field's height at any point of a full-detail block, on the CPU and in \
               its shader, is the height of the drawn triangle there to a tenth of a millimetre.",
        since: TOOLING,
        evidence: Evidence::Private("AC-EVID-HIFI-FIELD-HEIGHT"),
        station: "dereth-render-hifi::cpu::reshade::terrain_field::the_field_height_is_the_drawn_triangle_height_to_a_tenth_of_a_millimetre",
        tier: Tier::Cpu,
    },
    behaviour! {
        id: "hifi.visual-only.a-session-plays-identically-with-the-presentation-on",
        says: "A session that walks the body forward, turns and walks again decides exactly the \
               same on every frame with the high-fidelity presentation drawing every frame as \
               with it off: the messages it sends the server, byte for byte; the body's cell, \
               position and heading, the camera's viewpoint and the offered objects' positions, \
               to the bit; the objects offered to picking and the cells drawn; what a pick at \
               each of a grid of points names; and whether the selected object was drawn.",
        since: THIS_CLIENT,
        divergence: "CD-037",
        evidence: Evidence::Private("AC-EVID-HIFI-VISUAL-ONLY"),
        station: "dereth-client::gpu::rendering::hifi_visual_only::a_session_walks_turns_and_offers_the_same_objects_with_the_presentation_on",
        tier: Tier::Gpu,
    },
];
