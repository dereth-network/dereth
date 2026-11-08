//! The remaster -- a second, modern drawing of the world that changes pixels only, off unless a
//! player ticks it: that it stands up and goes away in the running session, that its options are
//! read as they should be, and what it draws.
//!
//! One file per subject, so that two changes adding rows at the same time do not edit the same
//! file. [`ROWS`] is in id order; the registry's own test asserts that, and that no id and no
//! evidence handle is repeated anywhere in it.

// `behaviour!` is `#[macro_export]`ed by `mod.rs` above this module's declaration, so it is in
// textual scope here and needs no import.
use super::{Behaviour, Evidence, Tier, THIS_CLIENT};

/// This subject's rows, in id order.
pub static ROWS: &[Behaviour] = &[
    behaviour! {
        id: "remaster.art-pack.remaster-air",
        says: "While an art pack is drawn outdoors, the air over the land takes the place of the \
               fog on the opaque world: far land fades by two layers of air toward the sky behind \
               it and keeps its own dark and light. With nothing of a pack drawn, the fog stays.",
        since: THIS_CLIENT,
        divergence: "CD-037",
        evidence: Evidence::Private("AC-EVID-REMASTER-AIR"),
        station: "dereth-client::gpu::rendering::remaster_packs::an_art_packs_air_lets_far_land_keep_its_dark_and_light",
        tier: Tier::Gpu,
    },
    behaviour! {
        id: "remaster.art-pack.remaster-cloud-shadows",
        says: "While an art pack is drawn outdoors under a high sun, retail's cloud picture, laid \
               over the world, shades the land under it.",
        since: THIS_CLIENT,
        divergence: "CD-037",
        evidence: Evidence::Private("AC-EVID-REMASTER-CLOUDS"),
        station: "dereth-client::gpu::rendering::remaster_packs::an_art_packs_clouds_shade_the_ground_under_them",
        tier: Tier::Gpu,
    },
    behaviour! {
        id: "remaster.art-pack.remaster-draws-a-models-vertex-color-and-color-factor",
        says: "The remaster draws an art pack's model as its picture times its material's color \
               factor times its vertices' color: a neutral gray leaf whose green is in its \
               vertices or its factor is drawn green.",
        since: THIS_CLIENT,
        divergence: "CD-037",
        evidence: Evidence::Private("AC-EVID-REMASTER-PACK-COLOR"),
        station: "dereth-client::gpu::rendering::remaster_packs::a_gray_leaf_is_drawn_under_its_vertices_color_and_its_materials_factor",
        tier: Tier::Gpu,
    },
    behaviour! {
        id: "remaster.art-pack.remaster-draws-far-trees-and-forest-cover",
        says: "With an art pack on, the remaster draws the trees retail's scenery places out past \
               the blocks round the viewer from the pack's far pictures, thinning toward the \
               reach, and lays the forest cover on the far ground.",
        since: THIS_CLIENT,
        divergence: "CD-037",
        evidence: Evidence::Private("AC-EVID-REMASTER-FAR-FOREST"),
        station: "dereth-client::gpu::rendering::remaster_packs::a_packs_far_pictures_stand_its_trees_past_the_blocks_round_the_viewer",
        tier: Tier::Gpu,
    },
    behaviour! {
        id: "remaster.art-pack.remaster-grade",
        says: "While an art pack is drawn the frame takes a little more saturation and contrast \
               and darker corners; with no pack given it takes none of them.",
        since: THIS_CLIENT,
        divergence: "CD-037",
        evidence: Evidence::Private("AC-EVID-REMASTER-GRADE"),
        station: "dereth-client::gpu::rendering::remaster_packs::an_art_pack_brings_a_grade_and_no_pack_brings_none",
        tier: Tier::Gpu,
    },
    behaviour! {
        id: "remaster.art-pack.remaster-grows-grass-on-muddy-banks",
        says: "With a pack's ground on, grass grows in patches over a muddy bank, more the farther                from the water; retail's water ground is laid as the land beside it and, well                inside the water, as the bed; the ground along the water is wet and darker.",
        since: THIS_CLIENT,
        divergence: "CD-037",
        evidence: Evidence::Private("AC-EVID-REMASTER-BANKS"),
        station: "dereth-client::gpu::rendering::remaster_packs::a_packs_grass_grows_over_a_muddy_bank_away_from_the_water",
        tier: Tier::Gpu,
    },
    behaviour! {
        id: "remaster.art-pack.remaster-light",
        says: "While an art pack is drawn what stands outdoors gains on what the sun only \
               grazes, and leaves take the sky \
               from all round and some of the sun in their own shade, less the deeper they \
               lie in their crown.",
        since: THIS_CLIENT,
        divergence: "CD-037",
        evidence: Evidence::Private("AC-EVID-REMASTER-LIGHT"),
        station: "dereth-client::gpu::rendering::remaster_packs::an_art_packs_light_is_softer_on_what_the_sun_only_grazes",
        tier: Tier::Gpu,
    },
    behaviour! {
        id: "remaster.art-pack.remaster-sharpens-blend-edges",
        says: "Close by, an art pack's materials meet in a sharpened edge along retail's masks, \
               the higher standing up through the lower; far off the masks' soft edge stands.",
        since: THIS_CLIENT,
        divergence: "CD-037",
        evidence: Evidence::Private("AC-EVID-REMASTER-BLEND-EDGES"),
        station: "dereth-client::gpu::rendering::remaster_packs::a_packs_materials_meet_in_a_sharper_edge_close_by",
        tier: Tier::Gpu,
    },
    behaviour! {
        id: "remaster.art-pack.remaster-sky-lights-what-the-sun-misses",
        says: "While an art pack is drawn the open sky holds a share of the sun's light and \
               lights what the sun misses, so folds down a cliff do not read as dark streaks.",
        since: THIS_CLIENT,
        divergence: "CD-037",
        evidence: Evidence::Private("AC-EVID-REMASTER-SKY"),
        station: "dereth-client::gpu::rendering::remaster_packs::folds_in_a_cliff_do_not_read_as_dark_streaks",
        tier: Tier::Gpu,
    },
    behaviour! {
        id: "remaster.art-pack.remaster-steep-material-keeps-its-detail",
        says: "On steep ground a pack's material and its normal are read filtered along the                pixel's long reach, so its detail shows down the fall line and does not run                together into streaks.",
        since: THIS_CLIENT,
        divergence: "CD-037",
        evidence: Evidence::Private("AC-EVID-REMASTER-CLIFFS"),
        station: "dereth-client::gpu::rendering::remaster_packs::a_steep_material_keeps_its_detail_down_the_fall_line",
        tier: Tier::Gpu,
    },
    behaviour! {
        id: "remaster.art-pack.remaster-varies-its-ground",
        says: "With a pack's ground on, its materials are laid out in the broad patches of the                retail pictures they replace and vary gently over the land, so a flat material                does not stay flat.",
        since: THIS_CLIENT,
        divergence: "CD-037",
        evidence: Evidence::Private("AC-EVID-REMASTER-GROUND"),
        station: "dereth-client::gpu::rendering::remaster_packs::a_packs_flat_material_varies_over_the_land",
        tier: Tier::Gpu,
    },
    behaviour! {
        id: "remaster.build.the-desktop-client-carries-the-remaster-and-the-browser-client-never-does",
        says: "The desktop client is built with the remaster, and the browser client never is: neither \
               its manifest nor anything it depends on for its target asks for it.",
        since: THIS_CLIENT,
        divergence: "CD-037",
        evidence: Evidence::Private("AC-EVID-REMASTER-BUILD"),
        station: "dereth-client::cpu::rendering::remaster_build::the_desktop_build_carries_the_remaster_and_the_browser_build_never_does",
        tier: Tier::Cpu,
    },
    behaviour! {
        id: "remaster.options.a-live-fidelity-change-is-kept-for-the-next-world",
        says: "A remaster option changed while playing is recorded in the scene the next \
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
        id: "remaster.options.every-fidelity-option-is-off-by-default",
        says: "The remaster is off in a fresh profile, with the boxes of what it draws on any \
               card ticked under it, so ticking it alone draws the whole of it; tracing, the \
               upscalers and the art pack wait for the player. None is in effect unless the \
               Horizon interface is shown: a profile that ticks every box and starts in the \
               classic or the modern interface has nothing in effect.",
        since: THIS_CLIENT,
        divergence: "CD-037",
        evidence: Evidence::Private("AC-EVID-HIFI-PREFS-DEFAULT"),
        station: "dereth-client-runtime::lib::render_prefs::tests::the_remaster_is_off_at_first_and_none_of_it_is_in_effect_outside_horizon",
        tier: Tier::Cpu,
    },
    behaviour! {
        id: "remaster.options.old-effects-boxes-are-ignored",
        says: "A profile naming the boxes of the experimental effects an earlier build had still loads: \
               they are not read, nothing of them is in effect, and neither is an error.",
        since: THIS_CLIENT,
        divergence: "CD-037",
        evidence: Evidence::Private("AC-EVID-REMASTER-OLD-BOXES"),
        station: "dereth-client-runtime::lib::render_prefs::tests::a_profile_naming_the_retired_effects_loads_and_they_are_ignored",
        tier: Tier::Cpu,
    },
    behaviour! {
        id: "remaster.options.the-boxes-are-on-horizons-page-alone-each-off",
        says: "The experimental rendering effects' seven boxes are on the Horizon interface's \
               Client page alone, last, under a heading that calls them highly experimental, \
               each off by default; the classic and the modern interfaces' pages are exactly \
               what they are without the effects; and a build without the effects has no such \
               heading.",
        since: THIS_CLIENT,
        divergence: "CD-037",
        evidence: Evidence::Private("AC-EVID-HIFI-HORIZON-PAGE"),
        station: "dereth-client-contract::lib::options::sheet::tests::the_remasters_boxes_are_on_horizons_client_page_alone_each_off",
        tier: Tier::Cpu,
    },
    behaviour! {
        id: "remaster.options.the-fidelity-section-reaches-the-render-preferences",
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
        id: "remaster.startup.the-vulkan-device-is-always-ready-for-the-remaster",
        says: "The renderer the client starts on is the one named or the default whatever the remaster's \
               boxes hold, and the Vulkan device of a build with the remaster is always asked \
               for what the remaster draws with, so ticking its box later needs no restart.",
        since: THIS_CLIENT,
        divergence: "CD-037",
        evidence: Evidence::Private("AC-EVID-REMASTER-STARTUP"),
        station: "dereth-client-shell::lib::app::tests::the_start_up_device_is_never_moved_and_vulkan_is_always_ready_for_the_remaster",
        tier: Tier::Cpu,
    },
    behaviour! {
        id: "remaster.toggle.retail-look-is-byte-identical-after-rounds-of-the-box",
        says: "After rounds of the remaster's box ticked and unticked, Retail Look's frame is the                frame of a session whose box was never ticked, byte for byte, with the same draws                submitted and the same counters.",
        since: THIS_CLIENT,
        divergence: "CD-037",
        evidence: Evidence::Private("AC-EVID-REMASTER-TOGGLE-BYTES"),
        station: "dereth-client::gpu::rendering::remaster_on::four_rounds_of_the_box_leave_retail_looks_frame_byte_for_byte_as_never_ticked",
        tier: Tier::Gpu,
    },
    behaviour! {
        id: "remaster.toggle.the-remaster-stands-up-and-goes-away-in-the-running-session",
        says: "Ticking the remaster's box stands it up in the running session and unticking it takes it \
               off the device, round after round, and Retail Look's frame after is the one \
               drawn before any tick, byte for byte.",
        since: THIS_CLIENT,
        divergence: "CD-037",
        evidence: Evidence::Private("AC-EVID-REMASTER-TOGGLE"),
        station: "dereth-client::gpu::rendering::remaster_on::ticking_the_remaster_on_and_off_four_times_stands_it_up_and_takes_it_down_each_time",
        tier: Tier::Gpu,
    },
    behaviour! {
        id: "remaster.toggle.the-window-resizes-while-the-remaster-draws",
        says: "A window resized while the remaster draws into it takes each size, on Vulkan and on \
               Direct3D 12, and the remaster draws every frame at it.",
        since: THIS_CLIENT,
        divergence: "CD-037",
        evidence: Evidence::Private("AC-EVID-REMASTER-RESIZE"),
        station: "dereth-client::gpu::rendering::remaster_on::the_window_resizes_while_the_remaster_draws_on_vulkan_and_direct3d_12",
        tier: Tier::Gpu,
    },
    behaviour! {
        id: "remaster.water.remaster-ripples-and-retails-color",
        says: "The remaster's water ripples from one repeating picture of wind ripples laid over \
               itself, calming with distance; over a lowered bed its own color is retail's water \
               under the bed's light.",
        since: THIS_CLIENT,
        divergence: "CD-037",
        evidence: Evidence::Private("AC-EVID-REMASTER-WATER"),
        station: "dereth-client::gpu::rendering::remaster_water::the_water_ripples_near_lies_calm_far_and_takes_retails_color_over_its_bed",
        tier: Tier::Gpu,
    },
];
