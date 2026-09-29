//! Selection -- picking, targeting and cycling through what is in the world.
//!
//! One file per subject, so that two changes adding rows at the same time do not edit the same
//! file. [`ROWS`] is in id order; the registry's own test asserts that, and that no id and no
//! evidence handle is repeated anywhere in it.

// `behaviour!` is `#[macro_export]`ed by `mod.rs` above this module's declaration, so it is in
// textual scope here and needs no import.
use super::{Behaviour, Evidence, Tier, RETAIL};

/// This subject's rows, in id order.
pub static ROWS: &[Behaviour] = &[
    behaviour! {
        id: "selection.auto-target.a-defender-notification-stamps-the-attack-clock-and-reselects",
        says: "When the server reports that the player was attacked, the client notes the time \
               and, with nothing selected, automatic targeting selects the attacker even when \
               another candidate is closer.",
        since: RETAIL,
        evidence: Evidence::Private("AC-EVID-O626-AUTO-TARGET"),
        station: "dereth-client::gpu::selection::auto_target::defender_notification::each_defender_handler_reselects_the_last_attacker",
        tier: Tier::Gpu,
    },
    behaviour! {
        id: "selection.auto-target.a-deselect-reaches-the-fifteen-second-fallback",
        says: "A selection change that leaves nothing selected runs automatic targeting: an \
               attacker who hit the player under fifteen seconds ago is selected again, and at \
               fifteen seconds or later the closest compass item is chosen instead.",
        since: RETAIL,
        evidence: Evidence::Private("AC-EVID-O634-AUTO-TARGET"),
        station: "dereth-client::gpu::selection::auto_target::selection_change::the_fifteen_second_fallback_leg_fires_from_a_selection_change_and_the_boundary_is_exclusive",
        tier: Tier::Gpu,
    },
    behaviour! {
        id: "selection.auto-target.reselects-the-last-attacker-within-fifteen-seconds-else-the-closest-compass-item",
        says: "Automatic targeting re-selects the last creature that attacked the player if it did \
               so less than fifteen seconds ago; at fifteen seconds or later it selects the \
               closest compass item instead.",
        since: RETAIL,
        evidence: Evidence::Private("AC-EVID-O616-AUTO-TARGET"),
        station: "dereth-client::gpu::selection::auto_target::auto_target_reselects_a_last_attacker_inside_fifteen_seconds_and_not_at_fifteen",
        tier: Tier::Gpu,
    },
    behaviour! {
        id: "selection.blink.a-picked-object-and-worn-item-blink-then-restore",
        says: "Clicking an object in the world makes it blink for a moment and then return to its \
               normal look, while the object beside it does not change.",
        since: RETAIL,
        evidence: Evidence::Private("AC-EVID-P1-BLINK"),
        station: "dereth-client::gpu::selection::selection_blink::a_clicked_world_object_blinks_bright_dim_bright_dim_and_is_restored",
        tier: Tier::Gpu,
    },
    behaviour! {
        id: "selection.blink.a-picked-object-blinks-bright-dim-bright-dim-and-is-restored",
        says: "Clicking an object in the world makes its lighting go bright, dim, bright, dim over \
               about a second and then return to normal, while a neighbouring object stays \
               unchanged throughout.",
        since: RETAIL,
        evidence: Evidence::Private("AC-EVID-P1-BLINK-PICKED"),
        station: "dereth-client::gpu::selection::selection_blink::a_clicked_world_object_blinks_bright_dim_bright_dim_and_is_restored",
        tier: Tier::Gpu,
    },
    behaviour! {
        id: "selection.brackets.a-selected-object-draws-the-shipped-corner-brackets",
        says: "Selecting a creature or a corpse in the world draws the shipped four corner \
               brackets around it, tinted in the selection colour, and deselecting takes them \
               down.",
        since: RETAIL,
        evidence: Evidence::Private("AC-EVID-ASTRA-WORLD-SELECTION-BRACKETS"),
        station: "dereth-client::gpu::selection::target_brackets::app_draws_dat_brackets_for_world_creature_and_corpse_and_clears_deselection",
        tier: Tier::Gpu,
    },
    behaviour! {
        id: "selection.brackets.the-brackets-hide-during-the-portal-tunnel",
        says: "The selection brackets follow the world's shrinking view while it fades out for a \
               portal, and are hidden while the tunnel is up without the target being deselected.",
        since: RETAIL,
        evidence: Evidence::Private("AC-EVID-ASTRA-WORLD-SELECTION-BRACKETS-BRACKETS"),
        station: "dereth-client::gpu::selection::target_brackets::selected_target_tracks_world_fade_projection_and_hides_during_the_tunnel",
        tier: Tier::Gpu,
    },
    behaviour! {
        id: "selection.cycle.each-selection-action-picks-or-refuses-as-the-client-does",
        says: "Each closest-thing selection key picks its own kind: the nearest monster, loose \
               item, player, corpse or compass item, and never something of another kind.",
        since: RETAIL,
        evidence: Evidence::Private("AC-EVID-O580-CYCLE"),
        station: "dereth-client::gpu::selection::selection_cycle_actions::each_kind_selects_its_own_and_no_other",
        tier: Tier::Gpu,
    },
    behaviour! {
        id: "selection.indicator.selecting-yourself-hides-the-target-indicator",
        says: "Selecting someone else draws the target brackets, but clicking your own body \
               selects you and takes the bright target indicator down.",
        since: RETAIL,
        evidence: Evidence::Private("AC-EVID-P1-INDICATOR"),
        station: "dereth-client::gpu::selection::self_target_indicator::selecting_yourself_takes_the_vivid_target_indicator_down_as_retail_does",
        tier: Tier::Gpu,
    },
    behaviour! {
        id: "selection.last-attacker.selects-a-visible-attacker-strictly-inside-radar-range",
        says: "Selecting the last attacker selects it only while it is strictly inside radar \
               range, 75 metres outdoors and 25 indoors: an attacker exactly at that distance or \
               beyond it is not selected.",
        since: RETAIL,
        evidence: Evidence::Private("AC-EVID-O465-LAST-ATTACKER"),
        station: "dereth-client-model::cpu::selection::last_attacker::the_distance_gate_is_the_radar_range_and_it_excludes_the_boundary_at_both_ranges",
        tier: Tier::Cpu,
    },
    behaviour! {
        id: "selection.pick.an-open-door-stops-swallowing-the-pick",
        says: "A closed door between the pointer and a chest behind it takes the click; once the \
               door has swung open the same click reaches the chest, and closing the door makes it \
               take the click again.",
        since: RETAIL,
        evidence: Evidence::Private("AC-EVID-P170E-PICK"),
        station: "dereth-client::gpu::selection::open_door_pick::in_the_doorway::an_open_door_stops_swallowing_the_pick_and_a_closed_one_resumes_it",
        tier: Tier::Gpu,
    },
    behaviour! {
        id: "selection.pick.an-outdoor-object-a-sealed-interior-frame-never-drew-is-not-selectable",
        says: "From inside a room with no view outside, an outdoor object the frame did not draw \
               cannot be selected, even with the pointer right over the place where it stands.",
        since: RETAIL,
        evidence: Evidence::Private("AC-EVID-P170B-PICK"),
        station: "dereth-client::gpu::selection::indoor_pick_outdoor_objects::an_outdoor_object_a_sealed_interior_frame_never_drew_is_not_selectable",
        tier: Tier::Gpu,
    },
    behaviour! {
        id: "selection.pick.clicking-your-own-body-selects-you",
        says: "Clicking your own body in the world selects you, and clicking the empty space just \
               beside you selects nothing.",
        since: RETAIL,
        evidence: Evidence::Private("AC-EVID-P1-PICK"),
        station: "dereth-client::gpu::selection::select_self::clicking_the_local_body_selects_the_player_and_beside_him_selects_nothing",
        tier: Tier::Gpu,
    },
    behaviour! {
        id: "selection.pick.indoors-only-what-a-clipped-opening-shows-is-drawn-and-named",
        says: "Indoors, looking out through a window, an outdoor object inside the window's \
               opening is drawn and can be picked and named, while one outside the opening is \
               neither drawn nor named.",
        since: RETAIL,
        evidence: Evidence::Private("AC-EVID-P170C-PICK"),
        station: "dereth-client::gpu::selection::indoor_portal_clip::facing_a_window_only_what_is_inside_its_cone_is_drawn_and_named",
        tier: Tier::Gpu,
    },
    behaviour! {
        id: "selection.range-checks.a-frame-closes-a-vendor-the-player-walked-away-from",
        says: "When the player walks away from an open vendor, the periodic range check closes the \
               shop once the player is out of range; it does not close it before the first check \
               falls due.",
        since: RETAIL,
        evidence: Evidence::Private("AC-EVID-O385-RANGE-CHECKS"),
        station: "dereth-client::gpu::selection::object_range_checks::a_frame_closes_the_vendor_the_player_has_walked_away_from",
        tier: Tier::Gpu,
    },
    behaviour! {
        id: "selection.range-watch.a-drawn-selection-survives-every-range-exit",
        says: "A selected monster that has been drawn stays selected however often it goes out of \
               radar range and back; each exit re-arms the range watch instead of clearing the \
               selection.",
        since: RETAIL,
        evidence: Evidence::Private("AC-EVID-O671-RANGE-WATCH"),
        station: "dereth-client::gpu::selection::selection_persistence::a_driven_run_keeps_the_selection_across_every_range_exit_once_the_object_has_been_drawn",
        tier: Tier::Gpu,
    },
    behaviour! {
        id: "selection.range-watch.arms-at-the-radius-the-players-cell-chooses",
        says: "The watch that drops a selection once it goes out of range is set at 75 metres \
               while the player stands outdoors and at 25 metres while the player is inside a \
               building.",
        since: RETAIL,
        evidence: Evidence::Private("AC-EVID-O465-RANGE-WATCH"),
        station: "dereth-client::gpu::selection::object_range_checks::range_watch::the_selection_watch_arms_at_the_range_the_players_own_cell_chooses",
        tier: Tier::Gpu,
    },
    behaviour! {
        id: "selection.retention.a-deselect-keeps-the-reference-and-the-cycle-continues-from-it",
        says: "After the player deselects a target or it is cleared, select next carries on \
               outward from where that target was: it picks the next thing beyond it, not the \
               farthest and not the nearest.",
        since: RETAIL,
        evidence: Evidence::Private("AC-EVID-O670-RETENTION"),
        station: "dereth-client-model::cpu::selection::selection_retention::a_deselect_leaves_the_reference_behind_and_the_cycle_continues_from_it",
        tier: Tier::Cpu,
    },
    behaviour! {
        id: "selection.tab.next-steps-outward-previous-inward-and-the-cycle-wraps",
        says: "Select next moves from the current target to the next one farther away and select \
               previous to the next one nearer; at the farthest or nearest end the step finds \
               nothing further, and the key press then wraps round to the other end.",
        since: RETAIL,
        evidence: Evidence::Private("AC-EVID-O172-TAB"),
        station: "dereth-client-model::cpu::selection::tab_target_cycle::next_steps_outward_and_previous_steps_inward",
        tier: Tier::Cpu,
    },
    behaviour! {
        id: "selection.tab.the-range-is-inclusive-and-height-orders-but-never-gates",
        says: "Whether something can be cycled to depends only on its distance across the ground, \
               so a creature 74 metres away and far below can still be selected; height only \
               changes the order the cycle visits things in, and something exactly at the edge of \
               the range is still included.",
        since: RETAIL,
        evidence: Evidence::Private("AC-EVID-O172-TAB-RANGE"),
        station: "dereth-client-model::cpu::selection::tab_target_cycle::z_orders_the_candidates_and_never_gates_them",
        tier: Tier::Cpu,
    },
    behaviour! {
        id: "selection.target-marker.is-colourised-with-retails-integer-hue-and-keeps-its-alpha",
        says: "The selection marker is tinted by replacing each pixel's hue and saturation with \
               the tint's, using the retail client's whole-number colour arithmetic, which differs \
               from simply multiplying the colours; each pixel keeps its own brightness and its \
               own transparency.",
        since: RETAIL,
        evidence: Evidence::Private("AC-EVID-ASTRA-TARGET-MEDIA-TARGET-MARKER"),
        station: "dereth-client::dat::selection::target_marker_colouring::colorize_preserves_alpha_and_value_with_retails_integer_hue_and_saturation",
        tier: Tier::Dat,
    },
    behaviour! {
        id: "selection.tooltip.a-hover-names-only-what-the-frame-drew",
        says: "Hovering over an object that stands behind an interior wall, in a room the frame \
               did not draw, names nothing: the pick is refused and no tooltip shows its name.",
        since: RETAIL,
        evidence: Evidence::Private("AC-EVID-P170-TOOLTIP"),
        station: "dereth-client::gpu::selection::hover_tooltip_occlusion::an_object_behind_an_interior_wall_never_names_itself",
        tier: Tier::Gpu,
    },
    behaviour! {
        id: "selection.tooltip.hovering-an-object-names-it-and-leaving-clears-it",
        says: "Resting the pointer on a chest in the world shows its name in a tooltip and moving \
               off takes the name away; with the show-tooltips option off the chest can still be \
               picked but no name appears.",
        since: RETAIL,
        evidence: Evidence::Private("AC-EVID-P112-TOOLTIP"),
        station: "dereth-client::gpu::selection::object_hover_tooltip::hovering_a_chest_names_it_and_moving_off_takes_the_name_away",
        tier: Tier::Gpu,
    },
    behaviour! {
        id: "selection.viewport.transparent-panel-gutters-pass-clicks-to-the-world",
        says: "Clicks on the transparent padding around the inventory panels fall through to the \
               world view, so pressing on an object seen through a gap between panels selects it.",
        since: RETAIL,
        evidence: Evidence::Private("AC-EVID-P173-VIEWPORT"),
        station: "dereth-client::gpu::selection::viewport_click_passthrough::the_panel_gutters_hand_the_pointer_to_sbox_exactly_as_retail_does",
        tier: Tier::Gpu,
    },
    behaviour! {
        id: "use.range.the-client-sends-a-use-at-any-range-and-the-shard-sends-the-approach",
        says: "Using a character or creature sends the use request naming it whether it is close \
               or far away and changes nothing locally; walking the player into reach is left to \
               the shard, which sends the approach naming the target's own use distance.",
        since: RETAIL,
        evidence: Evidence::Private("AC-EVID-O287-RANGE"),
        station: "dereth-client-model::cpu::selection::use_is_range_blind::a_use_is_range_blind_because_range_is_not_an_input",
        tier: Tier::Cpu,
    },
];
