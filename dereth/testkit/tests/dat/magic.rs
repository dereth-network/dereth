//! The spellbook, the enchantment registry, the cast and its two failures, the resist, the vitae,
//! the spell-examine pane and the component page -- all through the shipped panels, over the retail
//! dats. Every scenario builds a whole client (`App::with_platform` over a headless platform), and
//! **this binary runs serially**: two headless clients in one process share the UI request globals.
//! Each scenario is a `pub fn` listed in `ALL`, which `census.rs` checks against the registry.
//! An enchantment's start is rebased on receipt, so most of these move the session clock with a
//! `dereth_testkit::Peer`: it sends the shard's own time-sync header, delivers ordered game events
//! and replays recorded blobs re-addressed to this session, over the socket-free endpoint the
//! harness installs. Nothing is bound and no datagram leaves this process.

use dereth_client_model::Request;
use dereth_primitives::num::math;
use dereth_testkit::{ClientSpec, HeadlessClient, Inbound};
use dereth_ui_screens::view::GameView as _;

/// Every scenario in this file, for the census: the name, **the behaviour ids the
/// scenario asserts**, and the function.
pub static ALL: &[dereth_testkit::behaviours::Scenario] = &[
    (
        "a_spellbook_click_reveals_and_selects",
        &["spellbook.click.reveals-and-selects-the-row"],
        a_spellbook_click_reveals_and_selects,
    ),
    (
        "a_removed_spell_leaves_the_book_the_bar_and_the_copy",
        &["spellbook.removal.reaches-the-book-the-bar-and-the-copy-a-relog-reads"],
        a_removed_spell_leaves_the_book_the_bar_and_the_copy,
    ),
    (
        "the_bar_keeps_a_spell_the_table_cannot_draw",
        &["spellbook.the-bar-keeps-a-spell-the-shipped-table-cannot-draw"],
        the_bar_keeps_a_spell_the_table_cannot_draw,
    ),
    (
        "a_timed_buff_shows_its_real_remaining_time_across_a_relog",
        &["enchantments.duration.a-timed-buff-shows-its-real-remaining-time-across-a-relog"],
        a_timed_buff_shows_its_real_remaining_time_across_a_relog,
    ),
    (
        "the_row_survives_its_own_duration_running_out",
        &["enchantments.expiry.the-row-stays-until-the-shard-takes-it-and-not-when-its-time-runs-out"],
        the_row_survives_its_own_duration_running_out,
    ),
    (
        "the_shards_removal_empties_the_pane_and_the_skill",
        &["enchantments.removal.takes-the-row-out-of-the-pane-the-skill-back-to-base-and-says-so"],
        the_shards_removal_empties_the_pane_and_the_skill,
    ),
    (
        "a_dispel_empties_the_same_panels_without_a_line",
        &["enchantments.dispel.empties-the-same-two-panels-and-says-nothing"],
        a_dispel_empties_the_same_panels_without_a_line,
    ),
    (
        "a_second_buff_reaches_an_already_open_pane",
        &["enchantments.pane.a-second-buff-reaches-a-pane-that-is-already-open"],
        a_second_buff_reaches_an_already_open_pane,
    ),
    (
        "a_recorded_resist_is_drawn_verbatim_on_the_magic_channel",
        &["magic.resist.the-shards-own-sentence-is-drawn-verbatim-on-the-magic-channel"],
        a_recorded_resist_is_drawn_verbatim_on_the_magic_channel,
    ),
    (
        "a_squelched_magic_channel_drops_the_same_bytes",
        &["magic.resist.a-squelched-magic-channel-drops-the-same-recorded-bytes"],
        a_squelched_magic_channel_drops_the_same_bytes,
    ),
    (
        "the_resist_sound_reaches_the_clients_queue",
        &["magic.resist.the-sound-that-comes-with-it-reaches-the-clients-own-queue"],
        the_resist_sound_reaches_the_clients_queue,
    ),
    (
        "a_fizzle_ends_the_cast_prints_its_line_and_burns_nothing",
        &["magic.cast.a-fizzle-ends-the-cast-prints-its-line-and-burns-nothing"],
        a_fizzle_ends_the_cast_prints_its_line_and_burns_nothing,
    ),
    (
        "a_recorded_fizzle_prints_the_line_without_ending_the_cast",
        &["magic.cast.the-shards-recorded-fizzle-prints-the-same-line-and-does-not-end-the-cast"],
        a_recorded_fizzle_prints_the_line_without_ending_the_cast,
    ),
    (
        "a_refusal_the_client_makes_itself_never_reaches_the_shard",
        &["magic.cast.a-refusal-the-client-makes-itself-is-said-in-its-own-words-and-never-sent"],
        a_refusal_the_client_makes_itself_never_reaches_the_shard,
    ),
    (
        "a_live_vitae_lights_the_lamp_and_fills_the_panel",
        &["vitae.lamp.lights-for-a-live-penalty-and-the-panel-says-how-much"],
        a_live_vitae_lights_the_lamp_and_fills_the_panel,
    ),
    (
        "the_panel_follows_every_tick_and_goes_dark_at_full_strength",
        &["vitae.panel.follows-every-tick-and-goes-dark-at-full-strength"],
        the_panel_follows_every_tick_and_goes_dark_at_full_strength,
    ),
    (
        "the_penalty_takes_the_skill_down_but_leaves_the_row_plain",
        &["vitae.penalty.takes-the-skill-down-leaves-the-row-plain-and-spares-the-attributes"],
        the_penalty_takes_the_skill_down_but_leaves_the_row_plain,
    ),
    (
        "a_secondary_click_on_a_spell_opens_its_description",
        &["spell-examine.pane.a-secondary-click-on-a-known-spell-opens-its-description"],
        a_secondary_click_on_a_spell_opens_its_description,
    ),
    (
        "the_primary_click_selects_the_row_and_examines_nothing",
        &["spell-examine.pane.the-primary-click-selects-the-row-and-examines-nothing"],
        the_primary_click_selects_the_row_and_examines_nothing,
    ),
    (
        "the_secondary_click_moves_the_books_own_selection_too",
        &["spell-examine.pane.the-secondary-click-moves-the-books-own-selection-too"],
        the_secondary_click_moves_the_books_own_selection_too,
    ),
    (
        "an_enchantment_shows_how_long_it_lasts_and_no_range",
        &["spell-examine.pane.an-enchantment-shows-how-long-it-lasts-and-no-range"],
        an_enchantment_shows_how_long_it_lasts_and_no_range,
    ),
    (
        "a_second_look_re_opens_the_window_the_close_control_shut",
        &["spell-examine.pane.a-second-look-re-opens-the-window-the-close-control-shut"],
        a_second_look_re_opens_the_window_the_close_control_shut,
    ),
    (
        "the_same_look_from_the_cast_bar_opens_it_without_selecting",
        &["spell-examine.pane.the-same-look-from-the-cast-bar-opens-it-without-selecting"],
        the_same_look_from_the_cast_bar_opens_it_without_selecting,
    ),
    (
        "a_look_cancels_an_appraisal_in_flight_and_only_the_first_of_two_does",
        &["spell-examine.cancel.a-look-cancels-an-appraisal-in-flight-and-only-the-first-of-two-does"],
        a_look_cancels_an_appraisal_in_flight_and_only_the_first_of_two_does,
    ),
    (
        "a_look_with_nothing_in_flight_asks_the_shard_nothing",
        &["spell-examine.cancel.a-look-with-nothing-in-flight-asks-the-shard-nothing"],
        a_look_with_nothing_in_flight_asks_the_shard_nothing,
    ),
    (
        "the_account_the_shard_names_is_the_one_the_client_keeps",
        &["spell-examine.formula.the-account-the-shard-names-is-the-one-the-client-keeps"],
        the_account_the_shard_names_is_the_one_the_client_keeps,
    ),
    (
        "the_formula_the_client_would_cast_follows_that_account",
        &["spell-examine.formula.the-formula-the-client-would-cast-follows-that-account"],
        the_formula_the_client_would_cast_follows_that_account,
    ),
    (
        "the_look_lists_the_tapers_this_account_must_carry",
        &["spell-examine.formula.the-look-lists-the-tapers-this-account-must-carry"],
        the_look_lists_the_tapers_this_account_must_carry,
    ),
    (
        "two_accounts_are_shown_different_tapers_for_one_spell",
        &["spell-examine.formula.two-accounts-are-shown-different-tapers-for-one-spell"],
        two_accounts_are_shown_different_tapers_for_one_spell,
    ),
    (
        "the_shipped_foci_table_reaches_the_game_model_with_the_description",
        &["spell-examine.formula.the-shipped-foci-table-reaches-the-game-model-with-the-description"],
        the_shipped_foci_table_reaches_the_game_model_with_the_description,
    ),
    (
        "carrying_a_foci_changes_what_the_client_would_spend",
        &["spell-examine.formula.carrying-a-foci-changes-what-the-client-would-spend"],
        carrying_a_foci_changes_what_the_client_would_spend,
    ),
    (
        "a_foci_for_another_school_leaves_the_long_formula_alone",
        &["spell-examine.formula.a-foci-for-another-school-leaves-the-long-formula-alone"],
        a_foci_for_another_school_leaves_the_long_formula_alone,
    ),
    (
        "the_look_with_a_foci_lists_a_scarab_and_four_tapers",
        &["spell-examine.formula.the-look-with-a-foci-lists-a-scarab-and-four-tapers"],
        the_look_with_a_foci_lists_a_scarab_and_four_tapers,
    ),
    (
        "the_look_without_a_foci_lists_the_long_per_account_formula",
        &["spell-examine.formula.the-look-without-a-foci-lists-the-long-per-account-formula"],
        the_look_without_a_foci_lists_the_long_per_account_formula,
    ),
    (
        "a_click_on_a_component_icon_selects_the_one_in_the_pack",
        &["spell-examine.components.a-click-on-an-icon-selects-the-one-in-the-pack"],
        a_click_on_a_component_icon_selects_the_one_in_the_pack,
    ),
    (
        "each_component_icon_names_its_own_slot",
        &["spell-examine.components.each-icon-names-its-own-slot"],
        each_component_icon_names_its_own_slot,
    ),
    (
        "a_component_the_player_does_not_carry_selects_nothing",
        &["spell-examine.components.one-the-player-does-not-carry-selects-nothing"],
        a_component_the_player_does_not_carry_selects_nothing,
    ),
    (
        "the_secondary_click_does_nothing_on_a_component_icon",
        &["spell-examine.components.the-secondary-click-does-nothing-on-an-icon"],
        the_secondary_click_does_nothing_on_a_component_icon,
    ),
    (
        "the_component_a_slot_stands_for_is_a_representative_of_its_kind",
        &["spell-examine.components.the-answer-is-a-representative-of-the-class-and-not-one-object"],
        the_component_a_slot_stands_for_is_a_representative_of_its_kind,
    ),
    (
        "the_components_the_player_lacks_are_the_ones_that_are_marked",
        &["spell-examine.marks.the-components-the-player-lacks-are-the-ones-that-are-marked"],
        the_components_the_player_lacks_are_the_ones_that_are_marked,
    ),
    (
        "everything_carried_marks_nothing_and_nothing_carried_marks_everything",
        &["spell-examine.marks.everything-carried-marks-nothing-and-nothing-carried-marks-everything"],
        everything_carried_marks_nothing_and_nothing_carried_marks_everything,
    ),
    (
        "a_component_arriving_clears_its_mark_and_one_leaving_brings_it_back",
        &["spell-examine.marks.a-component-arriving-clears-its-mark-and-one-leaving-brings-it-back"],
        a_component_arriving_clears_its_mark_and_one_leaving_brings_it_back,
    ),
    (
        "a_closed_window_still_re_marks_its_rows",
        &["spell-examine.marks.a-closed-window-still-re-marks-its-rows"],
        a_closed_window_still_re_marks_its_rows,
    ),
    (
        "a_second_of_the_same_kind_keeps_the_mark_off",
        &["spell-examine.marks.a-second-of-the-same-kind-keeps-the-mark-off"],
        a_second_of_the_same_kind_keeps_the_mark_off,
    ),
    (
        "the_notice_moves_for_a_component_and_for_nothing_else",
        &["spell-examine.marks.the-notice-moves-for-a-component-and-for-nothing-else"],
        the_notice_moves_for_a_component_and_for_nothing_else,
    ),
    (
        "a_component_row_shows_how_many_are_held_and_goes_away_at_none",
        &["spell-components.strip.a-row-shows-how-many-are-held-and-goes-away-at-none"],
        a_component_row_shows_how_many_are_held_and_goes_away_at_none,
    ),
    (
        "the_component_page_rebuilds_on_the_pack_and_not_every_frame",
        &["spell-components.strip.it-rebuilds-when-the-pack-changes-and-not-every-frame"],
        the_component_page_rebuilds_on_the_pack_and_not_every_frame,
    ),
    (
        "a_header_is_drawn_only_for_a_kind_the_player_holds_something_of",
        &["spell-components.strip.a-header-is-drawn-only-for-a-kind-the-player-holds-something-of"],
        a_header_is_drawn_only_for_a_kind_the_player_holds_something_of,
    ),
    (
        "the_component_walk_is_kind_order_with_each_kinds_rows_under_its_header",
        &["spell-components.strip.the-walk-is-kind-order-with-each-kinds-rows-under-its-header"],
        the_component_walk_is_kind_order_with_each_kinds_rows_under_its_header,
    ),
    (
        "the_component_row_icon_is_the_shipped_tables_and_not_the_objects",
        &["spell-components.strip.the-icon-is-the-shipped-tables-and-not-the-objects"],
        the_component_row_icon_is_the_shipped_tables_and_not_the_objects,
    ),
    (
        "an_identical_spellbook_frame_does_not_rebuild_and_a_changed_list_does",
        &["spellbook.redraw.an-identical-frame-does-not-rebuild-and-a-changed-spell-list-does"],
        an_identical_spellbook_frame_does_not_rebuild_and_a_changed_list_does,
    ),
    (
        "a_spellbook_badge_follows_a_spells_own_flags",
        &["spellbook.redraw.a-badge-follows-a-spells-own-flags-under-an-unchanged-list"],
        a_spellbook_badge_follows_a_spells_own_flags,
    ),
    (
        "the_spellbook_rows_re_sort_when_a_spells_place_in_the_order_moves",
        &["spellbook.redraw.the-rows-re-sort-when-a-spells-place-in-the-order-moves"],
        the_spellbook_rows_re_sort_when_a_spells_place_in_the_order_moves,
    ),
    (
        "the_components_page_is_the_census_of_what_the_player_carries",
        &["spell-components.strip.the-list-is-the-census-of-what-the-player-carries"],
        the_components_page_is_the_census_of_what_the_player_carries,
    ),
    (
        "the_held_count_follows_a_stack_size_the_shard_changes",
        &["spell-components.strip.the-held-count-follows-a-stack-size-the-shard-changes"],
        the_held_count_follows_a_stack_size_the_shard_changes,
    ),
    (
        "a_click_on_a_component_row_selects_that_component_in_the_world",
        &["spell-components.strip.a-click-on-a-row-selects-that-component-in-the-world"],
        a_click_on_a_component_row_selects_that_component_in_the_world,
    ),
    (
        "a_click_on_a_header_clears_the_selection_and_the_first_one_does_nothing",
        &["spell-components.strip.a-click-on-a-header-clears-the-selection-and-the-first-one-does-nothing"],
        a_click_on_a_header_clears_the_selection_and_the_first_one_does_nothing,
    ),
    (
        "selecting_a_component_in_the_world_highlights_its_row",
        &["spell-components.strip.selecting-a-component-in-the-world-highlights-its-row"],
        selecting_a_component_in_the_world_highlights_its_row,
    ),
    (
        "every_shipped_magic_key_raises_its_own_instruction",
        &["spellbar.keys.every-shipped-magic-key-raises-its-own-instruction-and-the-release-raises-none"],
        every_shipped_magic_key_raises_its_own_instruction,
    ),
    (
        "the_instructions_move_the_selection_and_the_cast_keys_cast",
        &["spellbar.keys.the-instructions-move-the-selection-and-the-cast-keys-cast"],
        the_instructions_move_the_selection_and_the_cast_keys_cast,
    ),
    (
        "the_tab_instructions_walk_the_eight_banks",
        &["spellbar.tabs.the-tab-instructions-walk-the-eight-banks-and-stop-at-the-ends"],
        the_tab_instructions_walk_the_eight_banks,
    ),
    (
        "the_frames_own_pass_delivers_what_was_queued_in_order",
        &["spellbar.keys.the-frames-own-pass-delivers-what-was-queued-in-order"],
        the_frames_own_pass_delivers_what_was_queued_in_order,
    ),
    (
        "a_real_key_press_moves_the_selection_the_ring_and_the_scroll",
        &["spellbar.keys.a-real-key-press-moves-the-selection-the-ring-and-the-scroll"],
        a_real_key_press_moves_the_selection_the_ring_and_the_scroll,
    ),
    (
        "one_click_selects_and_only_the_double_click_casts",
        &["spellbar.click.one-click-selects-and-only-the-double-click-casts"],
        one_click_selects_and_only_the_double_click_casts,
    ),
];

/// Run one of this file's scenarios under a recorder, and check that the behaviour ids it
/// asserted are exactly the ones its [`ALL`] entry declares.
fn scenario(name: &str) {
    dereth_testkit::behaviours::run_scenario(ALL, name);
}

// -------------------------------------------------------------------------------------------
// 14. spellbook.click.reveals-and-selects-the-row
// -------------------------------------------------------------------------------------------

/// Clicking a clipped spell row scrolls it into view and selects it, in the same frame.
pub fn a_spellbook_click_reveals_and_selects() {
    use dereth_client::pump::Pump;
    use dereth_primitives::DataId;
    use dereth_ui::widgets::listbox::{scroll_offset_of, set_scroll_offset};
    use dereth_ui_screens::panels::remaining::SPELL_PAGE;
    use dereth_ui_screens::screens::gameplay::GamePlayScreen;
    use dereth_ui_screens::view::SpellEntry;

    let mut c = HeadlessClient::new(ClientSpec::gameplay(4));
    c.hud_mut().spells = (1..=40)
        .map(|id| SpellEntry {
            id,
            name: format!("Spell {id}"),
            icon: Some(DataId(0x0600_13A5)),
            school: 4,
            level: 1,
            icon_power: 1,
            display_order: i32::try_from(id).expect("a small id"),
            bitfield: 0,
        })
        .collect();
    {
        let app = c.app_mut();
        let shell = app.ui_mut().expect("shell");
        let screen = shell.flow.current_mut().expect("gameplay");
        let any: &mut dyn std::any::Any = &mut **screen;
        let gameplay = any
            .downcast_mut::<GamePlayScreen>()
            .expect("the gameplay screen");
        let page = shell
            .ui
            .get_child_recursive(gameplay.root().expect("a root"), SPELL_PAGE)
            .expect("the spell page is in the shipped layout");
        let panel_id = gameplay
            .panels
            .pages
            .iter()
            .find(|p| p.handle == page)
            .expect("its panel")
            .panel_id;
        gameplay
            .panels
            .recv_set_panel_visibility(&mut shell.ui, panel_id, true);
    }
    c.tick(1);

    let (h, ch) = {
        let list = c
            .view()
            .expect_app()
            .hud()
            .panels
            .spellbook
            .list
            .as_ref()
            .expect("the list");
        assert_eq!(list.slots.iter().filter(|s| s.spell.is_some()).count(), 40);
        (list.handle, list.cell.1)
    };
    let view_box = c.view().expect_app().ui().expect("shell").ui.screen_box(h);
    let sy = if view_box.height() % ch == 0 {
        ch / 2
    } else {
        0
    };
    set_scroll_offset(&mut c.app_mut().ui_mut().expect("shell").ui, h, 0, sy);
    let index = usize::try_from((view_box.height() + sy - 2) / ch).expect("a row index");
    let (slot, ring) = {
        let list = c
            .view()
            .expect_app()
            .hud()
            .panels
            .spellbook
            .list
            .as_ref()
            .expect("the list");
        let row = &list.slots[index];
        assert!(!row.selected, "no selection is pre-supplied");
        (
            row.handle,
            row.selected_ring.expect("the shipped selection ring"),
        )
    };
    let (x, y) = (view_box.x0 + 10, view_box.y1 - 1);
    {
        let ui = &c.view().expect_app().ui().expect("shell").ui;
        let hit = ui
            .hit_test_screen(x, y)
            .expect("a visible row under the pointer");
        assert!(
            hit == slot || ui.is_ancestor_of(slot, hit),
            "the pointer must hit that row"
        );
        assert!(
            ui.node(slot).expect("the row").region.box_.y1 >= view_box.height(),
            "and the row must start clipped, or there is nothing to reveal"
        );
    }

    let mut pump = Pump::new();
    pump.state.is_ready = true;
    pump.state.is_active_app = true;
    let messages = [
        pump.mouse_move_message(f64::from(x), f64::from(y), 100_000),
        pump.mouse_button_message(winit::event::MouseButton::Left, true, 100_010)
            .expect("the left button"),
        pump.mouse_button_message(winit::event::MouseButton::Left, false, 100_020)
            .expect("the left button"),
    ];
    for m in messages {
        pump.dispatch(m);
        c.app_mut()
            .input_manager_mut()
            .expect("real input maps")
            .on_message(m);
    }
    c.tick(1);

    let expected_y = i32::try_from(index).expect("a row index") * ch - view_box.height() + ch;
    let after = {
        let app = c.view().expect_app();
        let ui = &app.ui().expect("shell").ui;
        (
            scroll_offset_of(ui, h),
            ui.node(slot).expect("the row").region.box_.y1,
            ui.node(ring).expect("the ring").region.flags.visible,
            app.hud()
                .panels
                .spellbook
                .list
                .as_ref()
                .expect("the list")
                .slots
                .iter()
                .enumerate()
                .all(|(i, s)| s.selected == (i == index)),
        )
    };
    // No further input: an unchanged pass must not erase the ring or move the viewport.
    c.tick(1);

    c.assert_behaviour("spellbook.click.reveals-and-selects-the-row", move |v| {
        let app = v.expect_app();
        let list = app.hud().panels.spellbook.list.as_ref().expect("the list");
        after.0 == Some((0, expected_y))
            && after.1 == view_box.height() - 1
            && after.2
            && after.3
            && list.slots[index].selected
            && scroll_offset_of(&app.ui().expect("shell").ui, h) == Some((0, expected_y))
    });
    c.shutdown();
}

#[test]
fn scenario_a_spellbook_click_reveals_and_selects() {
    scenario("a_spellbook_click_reveals_and_selects");
}

// -------------------------------------------------------------------------------------------
// 15. spellbook.removal.reaches-the-book-the-bar-and-the-copy-a-relog-reads
// 16. spellbook.the-bar-keeps-a-spell-the-shipped-table-cannot-draw
// -------------------------------------------------------------------------------------------
//
// The spells are the shipped table's own and are never written here; the panels are the shipped
// layout; and the prune the bar raises is consumed by the client's own request dispatch rather
// than read out of the panel, so a prune with no production arm would contribute nothing.

/// An id the shipped spell table has no row for. The character knows it; the panel cannot draw
/// it, because the question the bar asks is what the character knows.
const KNOWN_BUT_UNDRAWABLE: u32 = 0xFFFF_FFF0;
/// The player the description below belongs to.
const A_PLAYER: dereth_primitives::ObjectId = dereth_primitives::ObjectId(0x5000_000A);

/// The rows the bar's tab actually draws -- the list's own cells, not the model.
fn bar_rows(c: &HeadlessClient, tab: usize) -> Vec<u32> {
    c.view().hud().panels.spellcasting.lists[tab]
        .as_ref()
        .expect("the shipped bar bound its lists")
        .slots
        .iter()
        .filter_map(|s| s.spell)
        .collect()
}

/// A character who knows five of the shipped table's spells plus [`KNOWN_BUT_UNDRAWABLE`], with
/// all six on the first tab of the bar -- delivered as the description the login sends, so the
/// panels are built the way the client builds them.
fn a_character_with_six_favourites() -> (HeadlessClient, Vec<u32>) {
    use dereth_protocol::archive::PackedHash;
    use dereth_protocol::types::qualities::SpellBookPage;

    let mut c = HeadlessClient::new(ClientSpec::gameplay(4));
    let known: Vec<u32> = c
        .view()
        .hud()
        .spell_table
        .as_ref()
        .expect("the shipped spell table")
        .spells
        .keys()
        .copied()
        .take(5)
        .collect();
    assert_eq!(known.len(), 5, "the shipped table has spells");
    assert!(
        !c.view()
            .hud()
            .spell_table
            .as_ref()
            .expect("the shipped spell table")
            .spells
            .contains_key(&KNOWN_BUT_UNDRAWABLE),
        "and no row for the undrawable one"
    );

    let bar: Vec<u32> = known
        .iter()
        .copied()
        .chain([KNOWN_BUT_UNDRAWABLE])
        .collect();
    let page = SpellBookPage {
        casting_likelihood: 1.0,
        legacy: None,
    };
    let mut d = dereth_protocol::login::LoginPlayerDescription::default();
    d.qualities.spell_book = Some(PackedHash {
        table_size: 8,
        entries: bar.iter().map(|id| (*id, page)).collect(),
    });
    d.player_module.spell_bars = {
        let mut bars = vec![Vec::new(); 8];
        bars[0] = bar.clone();
        bars
    };

    // The description is the player weenie's, so a described player needs one.
    c.world_mut()
        .seed_player_desc(A_PLAYER, dereth_client_model::Qualities::default());
    c.when(Inbound::event(
        dereth_client_net::client_session::SessionEvent::PlayerDescription(Box::new(d)),
    ));
    c.world_mut().player_system.spell_tabs[0] = bar;
    c.tick(1);
    (c, known)
}

/// The shard's removal reaches the book, the bar and the copy a relog reads.
pub fn a_removed_spell_leaves_the_book_the_bar_and_the_copy() {
    let (mut c, known) = a_character_with_six_favourites();
    let doomed = known[2];

    let drawn_before = c.view().hud().panels.spellbook.shown.contains(&doomed)
        && bar_rows(&c, 0).contains(&doomed)
        && c.view().hud().stats.spells_removed == 0;
    let outbound_before = c.view().outbound().len();

    // The shard takes it away.
    c.when(Inbound::message(
        &dereth_protocol::qualities::MagicRemoveSpell {
            layered_spell_id: doomed,
        },
    ));
    let book_lost_it = c.view().hud().stats.spells_removed == 1
        && !c.view().hud().spells.iter().any(|s| s.id == doomed)
        // and took out nothing else
        && c.view().hud().spells.len() == known.len() - 1;

    // The frame after it: the panels follow, and the bar tells the shard it has lost the row.
    c.tick(1);
    let panels_followed = !c.view().hud().panels.spellbook.shown.contains(&doomed)
        && !bar_rows(&c, 0).contains(&doomed)
        && c.view().interaction().stats.spell_favorites_changed == 1;

    let pruned: Vec<(u32, i32)> = c.view().outbound()[outbound_before..]
        .iter()
        .filter_map(|r| match r {
            Request::RemoveSpellFavorite(m) => Some((m.spell_id, m.spell_bank)),
            _ => None,
        })
        .collect();

    let packed_lost_it = {
        let w = c.view().world();
        !w.player_system.spell_tabs[0].contains(&doomed)
            && w.player_system
                .client_packed_module()
                .expect("a module to re-pack")
                .spell_bars[0]
                .contains(&doomed)
                == false
    };

    c.assert_behaviour(
        "spellbook.removal.reaches-the-book-the-bar-and-the-copy-a-relog-reads",
        move |_| {
            drawn_before
            && book_lost_it
            && panels_followed
            // one prune, one tab, one spell
            && pruned == vec![(doomed, 0)]
            && packed_lost_it
        },
    );
    c.shutdown();
}

/// A spell the shipped table cannot draw keeps its place on the bar.
pub fn the_bar_keeps_a_spell_the_table_cannot_draw() {
    let (mut c, _known) = a_character_with_six_favourites();
    let outbound_before = c.view().outbound().len();
    // A second frame, so that a prune that merely took one more pass would still show up.
    c.tick(1);

    let still_a_favourite =
        c.view().world().player_system.spell_tabs[0].contains(&KNOWN_BUT_UNDRAWABLE);
    let not_drawn = !bar_rows(&c, 0).contains(&KNOWN_BUT_UNDRAWABLE);
    let not_pruned = c.view().interaction().stats.spell_favorites_changed == 0;
    let nothing_sent = !c.view().outbound()[outbound_before..].iter().any(
        |r| matches!(r, Request::RemoveSpellFavorite(m) if m.spell_id == KNOWN_BUT_UNDRAWABLE),
    );

    c.assert_behaviour(
        "spellbook.the-bar-keeps-a-spell-the-shipped-table-cannot-draw",
        move |v| {
            still_a_favourite
            && not_drawn
            && not_pruned
            && nothing_sent
            // and the character really does still know it
            && v.hud().view(v.objects()).is_spell_known(KNOWN_BUT_UNDRAWABLE)
        },
    );
    c.shutdown();
}

#[test]
fn scenario_a_removed_spell_leaves_the_book_the_bar_and_the_copy() {
    scenario("a_removed_spell_leaves_the_book_the_bar_and_the_copy");
}

#[test]
fn scenario_the_bar_keeps_a_spell_the_table_cannot_draw() {
    scenario("the_bar_keeps_a_spell_the_table_cannot_draw");
}

// -------------------------------------------------------------------------------------------
// enchantments.duration / expiry / removal / dispel / pane
//
// These scenarios are about the **session clock**: the client rebases an enchantment's start on
// receipt, so a claim about a remaining time is only a claim when the clock is somewhere a real
// session's would be. That is why these five build a whole client with a socket-free endpoint
// under it and move the clock with the shard's own time-sync header rather than by writing a
// field -- the client has no setter for it, and inventing one would be asserting over a clock
// the client does not keep.
// -------------------------------------------------------------------------------------------

/// A thirty-minute buff that has been running for ten shows twenty minutes left, and still shows
/// the right number after a relog.
pub fn a_timed_buff_shows_its_real_remaining_time_across_a_relog() {
    use support::{a_beneficial_spell, cell_seconds, duration_cell, timed_buff_login};

    /// The buff's total length, as the spell's own data gives it: thirty minutes.
    const DURATION: f64 = 1800.0;
    /// Where the session clock is put before the first description, and where after it.
    ///
    /// **An hour, and the hour is load-bearing.** A clock smaller than the remaining time still
    /// prints a wrong-but-non-zero number; only a clock larger than it reaches the nothing-left
    /// display this claim rules out.
    const FIRST_LOGIN_AT: f64 = 3600.0;
    const RELOG_AT: f64 = 4200.0;

    let (mut c, mut peer) = support::a_client_with_a_peer();
    let spell = a_beneficial_spell(&c).0;

    peer.set_clock(&mut c, FIRST_LOGIN_AT);
    let clock = c.view().expect_app().clock().cur_time;
    assert!(
        (clock - FIRST_LOGIN_AT).abs() < 1.0,
        "the premise: the session clock is {clock:.3} and not near {FIRST_LOGIN_AT:.0}; without a \
         clock far from zero a rebased start and a raw one give the same answer"
    );

    peer.event(&mut c, &timed_buff_login(spell, DURATION, 600.0));
    c.tick(8);
    let arrived = c.view().expect_app().hud().player_desc_received;
    let cell = duration_cell(&c, spell).expect("the buff the description carried is in the pane");
    // Read back into seconds rather than compared with a literal: the frames between the packet
    // landing and this read advance the clock by tens of milliseconds and the cell truncates. The
    // tolerance is seconds and the defect is minutes.
    let first = cell_seconds(&cell);
    let twenty_minutes_left = cell != "0:00" && (first - 1200).abs() <= 5;

    // The relog: the same message again, at a later clock, with the shard's own decremented
    // start. It is the only place a fresh session learns the registry from.
    peer.set_clock(&mut c, RELOG_AT);
    peer.event(&mut c, &timed_buff_login(spell, DURATION, 1200.0));
    c.tick(8);
    let after = duration_cell(&c, spell).expect("the buff survives the second login");
    let second = cell_seconds(&after);
    let ten_minutes_left = after != "0:00" && (second - 600).abs() <= 5;
    // And it went **down** rather than being re-anchored: the shard's own decremented start is
    // what moved it.
    let it_went_down = second < first;

    // The control the shape of the number rests on: an item's permanent spell has no remaining
    // time and the pane blanks it rather than printing one, so the number above cannot be coming
    // from a path that prints something for everything.
    let blanks_a_permanent_one = {
        let app = c.view().expect_app();
        let view = app.hud().view(app.objects());
        let e = view
            .active_effects()
            .into_iter()
            .find(|e| e.spell == spell)
            .expect("the buff");
        !e.permanent && dereth_ui_screens::panels::effects::duration_cell(&e, false).is_empty()
    };

    c.assert_behaviour(
        "enchantments.duration.a-timed-buff-shows-its-real-remaining-time-across-a-relog",
        move |_| {
            arrived
                && twenty_minutes_left
                && ten_minutes_left
                && it_went_down
                && blanks_a_permanent_one
        },
    );
    c.shutdown();
}

/// The row stays until the shard takes it away -- the client keeps no expiry clock of its own.
pub fn the_row_survives_its_own_duration_running_out() {
    use support::{chat_log, pane_rows, remaining, skill_row, Station, EXPIRED};

    let mut s = Station::arm();
    let base = s.base_skill;
    let drawn = pane_rows(&s.c) == vec![s.spell] && skill_row(&s.c).0 == base + support::BUFF_VALUE;
    let before = remaining(&s.c, s.spell).expect("the entry");
    let just_cast = before > 0.0 && before <= support::BUFF_DURATION;
    let lines_before = chat_log(&mut s.c).len();

    s.clock_far_past_the_duration();

    // The remaining time is allowed to go past; what is not allowed is the row going away.
    let after = remaining(&s.c, s.spell).unwrap_or_else(|| {
        panic!("the client expired the enchantment on a clock it does not keep")
    });
    let gone_past = after < 0.0;
    let still_drawn = pane_rows(&s.c) == vec![s.spell];
    let still_buffed = skill_row(&s.c).0 == base + support::BUFF_VALUE;
    let said_nothing = chat_log(&mut s.c)[lines_before..]
        .iter()
        .all(|(_, t)| !t.contains(EXPIRED));

    s.c.assert_behaviour(
        "enchantments.expiry.the-row-stays-until-the-shard-takes-it-and-not-when-its-time-runs-out",
        move |_| drawn && just_cast && gone_past && still_drawn && still_buffed && said_nothing,
    );
    s.c.shutdown();
}

/// The shard's removal takes the row out of the pane, the skill back to base, and prints the line.
pub fn the_shards_removal_empties_the_pane_and_the_skill() {
    use support::{chat_log, helpful_panel_is_open, pane_rows, skill_row, Station, EXPIRED};

    let mut s = Station::arm();
    let base = s.base_skill;
    let buffed =
        pane_rows(&s.c) == vec![s.spell] && skill_row(&s.c) == (base + support::BUFF_VALUE, 1);
    let lines_before = chat_log(&mut s.c).len();

    // Let its own clock run out first, so the removal is an expiry and not a dispel of a running
    // buff -- which is the sequence the claim names.
    s.clock_far_past_the_duration();
    s.remove();

    let took_it_out =
        s.c.view()
            .expect_app()
            .interaction()
            .stats
            .enchantments_removed
            == 1;
    let counts_followed = {
        let app = s.c.view().expect_app();
        app.hud().view(app.objects()).enchantment_counts() == (0, 0)
    };
    let pane_emptied = pane_rows(&s.c).is_empty() && helpful_panel_is_open(&mut s.c);
    let skill_back_to_base = skill_row(&s.c) == (base, 0);
    let one_line =
        s.c.view()
            .expect_app()
            .interaction()
            .stats
            .enchantment_expiry_lines
            == 1;
    let name = s.name.clone();
    let line_is_the_spells_own = chat_log(&mut s.c)[lines_before..]
        .iter()
        .find(|(_, t)| t.contains(EXPIRED))
        .is_some_and(|(ty, t)| t.trim_end() == format!("{name}{EXPIRED}") && *ty == support::MAGIC);

    s.c.assert_behaviour(
        "enchantments.removal.takes-the-row-out-of-the-pane-the-skill-back-to-base-and-says-so",
        move |_| {
            buffed
                && took_it_out
                && counts_followed
                && pane_emptied
                && skill_back_to_base
                && one_line
                && line_is_the_spells_own
        },
    );
    s.c.shutdown();
}

/// A dispel empties the same two panels and says nothing at all.
pub fn a_dispel_empties_the_same_panels_without_a_line() {
    use support::{chat_log, pane_rows, skill_row, Station, EXPIRED};

    let mut s = Station::arm();
    let base = s.base_skill;
    let drawn = pane_rows(&s.c) == vec![s.spell];
    let lines_before = chat_log(&mut s.c).len();

    s.dispel();

    let pane_emptied = pane_rows(&s.c).is_empty();
    let skill_back_to_base = skill_row(&s.c) == (base, 0);
    let no_line =
        s.c.view()
            .expect_app()
            .interaction()
            .stats
            .enchantment_expiry_lines
            == 0
            && chat_log(&mut s.c)[lines_before..]
                .iter()
                .all(|(_, t)| !t.contains(EXPIRED));

    s.c.assert_behaviour(
        "enchantments.dispel.empties-the-same-two-panels-and-says-nothing",
        move |_| drawn && pane_emptied && skill_back_to_base && no_line,
    );
    s.c.shutdown();
}

/// A second buff reaches a pane that is already open, with no second click.
pub fn a_second_buff_reaches_an_already_open_pane() {
    use support::{pane_rows, Station};

    let mut s = Station::arm();
    let first = s.spell;
    let second = s.arm_a_second_buff();

    let mut rows = pane_rows(&s.c);
    rows.sort_unstable();
    let mut want = vec![first, second];
    want.sort_unstable();
    let both_drawn = rows == want;
    let counts_followed = {
        let app = s.c.view().expect_app();
        app.hud().view(app.objects()).enchantment_counts() == (2, 0)
    };

    s.c.assert_behaviour(
        "enchantments.pane.a-second-buff-reaches-a-pane-that-is-already-open",
        move |_| both_drawn && counts_followed,
    );
    s.c.shutdown();
}

#[test]
fn scenario_a_timed_buff_shows_its_real_remaining_time_across_a_relog() {
    scenario("a_timed_buff_shows_its_real_remaining_time_across_a_relog");
}

#[test]
fn scenario_the_row_survives_its_own_duration_running_out() {
    scenario("the_row_survives_its_own_duration_running_out");
}

#[test]
fn scenario_the_shards_removal_empties_the_pane_and_the_skill() {
    scenario("the_shards_removal_empties_the_pane_and_the_skill");
}

#[test]
fn scenario_a_dispel_empties_the_same_panels_without_a_line() {
    scenario("a_dispel_empties_the_same_panels_without_a_line");
}

#[test]
fn scenario_a_second_buff_reaches_an_already_open_pane() {
    scenario("a_second_buff_reaches_an_already_open_pane");
}

mod support {
    use dereth_primitives::{LocalTime, ObjectId};
    use dereth_protocol::types::qualities::{
        AcQualities, Attribute, AttributeCache, Enchantment as ProtocolEnchantment,
        Skill as WireSkill, StatMod,
    };
    use dereth_testkit::{ClientSpec, HeadlessClient};
    use dereth_ui::{ElemHandle, ElementId, UiSystem};
    use dereth_ui_screens::screens::gameplay::GamePlayScreen;
    use dereth_ui_screens::view::GameView as _;

    /// The character these scenarios drive.
    pub const PLAYER: ObjectId = ObjectId(0x5000_0001);

    /// The skill the buff below raises, and the one the Skills page row is read off.
    const A_SKILL: u32 = 0x2C;
    /// The four bits the shard writes for an ordinary beneficial self-buff.
    const SKILL_ADD: u32 = 0x0000_0010 | 0x0000_1000 | 0x0000_8000 | 0x0200_0000;
    /// The character's own ranks and starting level in that skill.
    const SKILL_RANKS: u32 = 50;
    const SKILL_INIT: u32 = 5;
    const BASE_ATTRIBUTE: u32 = 100;
    /// The buff's value, as the spell's own row in the world data carries it.
    pub const BUFF_VALUE: i32 = 45;
    /// Short enough that a client-side timer, if one existed, would fire inside the clock moves
    /// these scenarios make.
    pub const BUFF_DURATION: f64 = 20.0;
    /// Where the clock is when the buff arrives, and where it is moved to afterwards -- five
    /// times the duration later, so nothing can be passing by accident.
    const ARMED_AT: f64 = 1000.0;
    const LONG_AFTER: f64 = 1120.0;

    /// The sentence the client appends when the shard takes an enchantment away, without the
    /// newline the chat system trims.
    pub const EXPIRED: &str = " has expired.";
    /// The chat channel it is written on.
    pub const MAGIC: u8 = 7;

    /// The shard these scenarios play: `dereth_testkit::replay::Peer`.
    pub use dereth_testkit::Peer;

    /// A whole client in the gameplay screen with the endpoint attached and the player's own body
    /// created, so that an ordered event addressed to him can be delivered.
    pub fn a_client_with_a_peer() -> (HeadlessClient, Peer) {
        let mut c = HeadlessClient::new(ClientSpec::gameplay_in_world(4));
        let mut peer = Peer::attach(&mut c, PLAYER);

        let mut p = dereth_protocol::objects::ObjectCreatePayload {
            id: PLAYER,
            ..Default::default()
        };
        p.physicsdesc.bitfield |= dereth_protocol::types::physicsdesc::flags::SETUP;
        p.physicsdesc.setup_id = Some(0x0200_0001);
        p.physicsdesc.timestamps.instance = 1;
        peer.send(
            &mut c,
            10,
            dereth_protocol::write_blob(&dereth_protocol::objects::ItemCreateObject(p))
                .expect("the create encodes"),
        );
        c.tick(1);
        c.world_mut().player = Some(PLAYER);
        (c, peer)
    }

    /// A beneficial spell the shipped table really has a row for, chosen at run time.
    ///
    /// The pane joins every enchantment against that table and drops the ones it misses, so an
    /// invented id would make a scenario green by measuring nothing.
    pub fn a_beneficial_spell(c: &HeadlessClient) -> (u32, String) {
        let app = c.view().expect_app();
        let table = app
            .hud()
            .spell_table
            .as_ref()
            .expect("the shipped spell table loads");
        table
            .spells
            .iter()
            .find(|(_, b)| b.bitfield & 4 != 0 && !b.name.is_empty())
            .map(|(id, b)| (*id, b.name.clone()))
            .expect("the shipped table has at least one beneficial spell")
    }

    fn attribute(v: u32) -> Attribute {
        Attribute {
            level_from_cp: 0,
            init_level: v,
            cp_spent: 0,
        }
    }

    /// A **specialised** skill, so the shipped table's own minimum cannot zero its base level.
    fn specialised(ranks: u32) -> WireSkill {
        WireSkill {
            level_from_pp: u16::try_from(ranks).expect("the rank count is small"),
            format_version: 1,
            sac: 3,
            pp: 0,
            init_level: SKILL_INIT,
            resistance_of_last_check: 0,
            last_used_time: 0.0,
        }
    }

    fn player_module() -> dereth_protocol::login::PlayerModule {
        dereth_protocol::login::PlayerModule {
            spell_bars: vec![Vec::new()],
            spell_filters: dereth_protocol::login::PlayerModule::DEFAULT_SPELL_FILTERS,
            options2: dereth_protocol::login::PlayerModule::DEFAULT_OPTIONS2,
            ..dereth_protocol::login::PlayerModule::default()
        }
    }

    fn description(qualities: AcQualities) -> dereth_protocol::login::LoginPlayerDescription {
        dereth_protocol::login::LoginPlayerDescription {
            qualities,
            player_module: player_module(),
            content_profiles: Vec::new(),
            inventory_placements: Vec::new(),
        }
    }

    /// The character, with an **empty** registry, so every enchantment is one that arrived live.
    pub fn a_character() -> dereth_protocol::login::LoginPlayerDescription {
        use dereth_protocol::archive::PackedHash;
        use dereth_protocol::types::qualities::{attribute_cache_mask as m, quality_flags};
        let cache = AttributeCache {
            flags: m::STRENGTH | m::COORDINATION,
            strength: Some(attribute(BASE_ATTRIBUTE)),
            coordination: Some(attribute(BASE_ATTRIBUTE)),
            ..AttributeCache::default()
        };
        description(AcQualities {
            flags: quality_flags::ATTRIBUTE_CACHE | quality_flags::SKILLS,
            attribute_cache: Some(cache),
            skills: Some(PackedHash {
                table_size: 8,
                entries: vec![(A_SKILL, specialised(SKILL_RANKS))],
            }),
            ..AcQualities::default()
        })
    }

    /// A description carrying one timed enchantment, in the shard's own writer order: the length
    /// is the spell's own and the start is **negative** elapsed, which is how the shard counts it
    /// down.
    pub fn timed_buff_login(
        spell: u32,
        duration: f64,
        elapsed: f64,
    ) -> dereth_protocol::login::LoginPlayerDescription {
        use dereth_protocol::types::qualities::{quality_flags, EnchantmentRegistry};
        let e = ProtocolEnchantment {
            id: spell,
            category_word: 1,
            power_level: 6,
            start_time: -elapsed,
            duration,
            caster: PLAYER,
            degrade_modifier: 0.0,
            degrade_limit: 0.0,
            last_time_degraded: 0.0,
            smod: StatMod {
                kind: SKILL_ADD,
                key: 6,
                value: 40.0,
            },
            spell_set_id: None,
        };
        description(AcQualities {
            flags: quality_flags::ENCHANTMENT_REGISTRY,
            enchantments: Some(EnchantmentRegistry {
                flags: EnchantmentRegistry::ADDITIVE,
                additive: Some(vec![e]),
                ..EnchantmentRegistry::default()
            }),
            ..AcQualities::default()
        })
    }

    /// One live enchantment message body. Its start is **relative to receipt**, so a buff that has
    /// just been cast carries zero.
    fn buff(spell: u32, category: u32) -> ProtocolEnchantment {
        ProtocolEnchantment {
            id: spell,
            category_word: category,
            power_level: 8,
            start_time: 0.0,
            duration: BUFF_DURATION,
            caster: PLAYER,
            degrade_modifier: 0.0,
            degrade_limit: 0.0,
            last_time_degraded: 0.0,
            #[allow(clippy::cast_precision_loss)]
            smod: StatMod {
                kind: SKILL_ADD,
                key: A_SKILL,
                value: BUFF_VALUE as f32,
            },
            spell_set_id: None,
        }
    }

    // -----------------------------------------------------------------------------------------
    // Reading what the client drew
    // -----------------------------------------------------------------------------------------

    fn gameplay(c: &mut HeadlessClient) -> (&mut UiSystem, &mut GamePlayScreen) {
        let shell = c.app_mut().ui_mut().expect("the shell");
        let ui = &mut shell.ui;
        let screen = shell.flow.current_mut().expect("a screen");
        let any: &mut dyn std::any::Any = &mut **screen;
        let screen = any
            .downcast_mut::<GamePlayScreen>()
            .expect("the gameplay screen is up");
        (ui, screen)
    }

    fn find(c: &mut HeadlessClient, id: ElementId) -> ElemHandle {
        let (ui, screen) = gameplay(c);
        let root = screen.root().expect("the gameplay root");
        ui.get_child_recursive(root, id)
            .unwrap_or_else(|| panic!("{:#010X} is not in the shipped tree", id.0))
    }

    /// Every chat line any window has taken, with the channel it was written on.
    pub fn chat_log(c: &mut HeadlessClient) -> Vec<(u8, String)> {
        gameplay(c)
            .1
            .chat
            .iter()
            .flat_map(|w| w.log.iter().cloned())
            .collect()
    }

    /// The spells the effects pane is drawing, in the order it drew them.
    pub fn pane_rows(c: &HeadlessClient) -> Vec<u32> {
        c.view()
            .expect_app()
            .hud()
            .panels
            .effects_helpful
            .rows
            .iter()
            .map(|r| r.spell)
            .collect()
    }

    /// The drawn number on the Skills page's row for the buffed skill, with its colour.
    pub fn skill_row(c: &HeadlessClient) -> (i32, u32) {
        let app = c.view().expect_app();
        let r = app
            .hud()
            .panels
            .skills
            .rows
            .iter()
            .find(|r| r.skill == A_SKILL)
            .expect("the buffed skill has a row on the page");
        (r.value, r.font)
    }

    /// The remaining time the pane would draw for `spell`, or nothing when the row has gone.
    pub fn remaining(c: &HeadlessClient, spell: u32) -> Option<f64> {
        let app = c.view().expect_app();
        let view = app.hud().view(app.objects());
        view.active_effects()
            .into_iter()
            .find(|e| e.spell == spell)
            .map(|e| e.remaining)
    }

    /// The pane's own value cell for `spell`.
    pub fn duration_cell(c: &HeadlessClient, spell: u32) -> Option<String> {
        let app = c.view().expect_app();
        let view = app.hud().view(app.objects());
        let e = view
            .active_effects()
            .into_iter()
            .find(|e| e.spell == spell)?;
        Some(dereth_ui_screens::panels::effects::duration_cell(&e, true))
    }

    /// A cell in either of the two shapes the pane writes, back into seconds, so an assertion can
    /// carry a tolerance.
    pub fn cell_seconds(cell: &str) -> i64 {
        let parts: Vec<i64> = cell
            .split(':')
            .map(|p| p.parse::<i64>().expect("a numeric field"))
            .collect();
        match parts.as_slice() {
            [m, s] => m * 60 + s,
            [h, m, s] => h * 3600 + m * 60 + s,
            _ => panic!("neither of the two shapes the cell is written in: {cell:?}"),
        }
    }

    pub fn helpful_panel_is_open(c: &mut HeadlessClient) -> bool {
        let panel = find(c, dereth_ui_screens::panels::effects::HELPFUL_PANEL);
        gameplay(c)
            .0
            .node(panel)
            .expect("a live node")
            .region
            .flags
            .visible
    }

    // -----------------------------------------------------------------------------------------
    // The fixture the four enchantment-life scenarios share
    // -----------------------------------------------------------------------------------------

    /// The buff lamp the player clicks to open the pane.
    const BUFF_LAMP: ElementId = ElementId(0x1000_00F5);

    /// A character with one live buff and the helpful pane open, reached the way a player reaches
    /// it: the enchantment lights the lamp and a pointer click on the lamp opens the pane.
    pub struct Station {
        pub c: HeadlessClient,
        peer: Peer,
        pub spell: u32,
        pub name: String,
        pub base_skill: i32,
    }

    impl Station {
        pub fn arm() -> Self {
            let (mut c, mut peer) = a_client_with_a_peer();
            peer.set_clock(&mut c, ARMED_AT);
            peer.event(&mut c, &a_character());
            c.tick(8);
            assert!(
                c.view().expect_app().hud().player_desc_received,
                "the description reached the player-description arm"
            );
            assert!(
                c.view().world().magic.spell_table.is_some(),
                "and the shipped spell table is loaded, or every pane row is dropped"
            );
            let base_skill = skill_row(&c).0;
            assert!(
                base_skill >= i32::try_from(SKILL_RANKS + SKILL_INIT).expect("small"),
                "the unbuffed row carries at least the character's own ranks: {base_skill}"
            );
            assert_eq!(
                skill_row(&c).1,
                0,
                "no enchantment yet, so the row is drawn plain"
            );

            let (spell, name) = a_beneficial_spell(&c);
            peer.event(
                &mut c,
                &dereth_protocol::qualities::MagicUpdateEnchantment(buff(spell, 100)),
            );
            c.tick(6);
            assert_eq!(
                {
                    let app = c.view().expect_app();
                    app.hud().view(app.objects()).enchantment_counts()
                },
                (1, 0),
                "one helpful enchantment was counted, which is what lights the lamp"
            );

            // The three events a player produces, and nothing else.
            let lamp = find(&mut c, BUFF_LAMP);
            {
                let (ui, _) = gameplay(&mut c);
                let b = ui.screen_box(lamp);
                assert!(b.is_valid(), "the lamp has no box to point at");
                let (x, y) = ((b.x0 + b.x1) / 2, (b.y0 + b.y1) / 2);
                ui.mouse_move(LocalTime(0.0), x, y);
                ui.mouse_down(dereth_ui::focus::action::PRIMARY_CLICK, x, y);
                ui.mouse_up(dereth_ui::focus::action::PRIMARY_CLICK, x, y, false);
            }
            c.tick(4);
            assert!(
                helpful_panel_is_open(&mut c),
                "the click has to open the pane before its contents can matter"
            );
            Self {
                c,
                peer,
                spell,
                name,
                base_skill,
            }
        }

        /// Move the session clock far past the buff's own duration.
        pub fn clock_far_past_the_duration(&mut self) {
            self.peer.set_clock(&mut self.c, LONG_AFTER);
            self.c.tick(8);
            let clock = self.c.view().expect_app().clock().cur_time;
            assert!(
                clock - ARMED_AT > BUFF_DURATION * 5.0,
                "the premise: the clock really moved past the duration ({clock:.1})"
            );
        }

        pub fn remove(&mut self) {
            let spell = self.spell;
            self.peer.event(
                &mut self.c,
                &dereth_protocol::qualities::MagicRemoveEnchantment {
                    layered_spell_id: spell,
                },
            );
            self.c.tick(8);
        }

        pub fn dispel(&mut self) {
            let spell = self.spell;
            self.peer.event(
                &mut self.c,
                &dereth_protocol::qualities::MagicDispelEnchantment {
                    layered_spell_id: spell,
                },
            );
            self.c.tick(8);
        }

        /// A second beneficial spell, in a different category so the duel keeps both.
        pub fn arm_a_second_buff(&mut self) -> u32 {
            let first = self.spell;
            let second = {
                let app = self.c.view().expect_app();
                let table = app.hud().spell_table.as_ref().expect("the shipped table");
                *table
                    .spells
                    .iter()
                    .find(|(id, b)| **id != first && b.bitfield & 4 != 0 && !b.name.is_empty())
                    .map(|(id, _)| id)
                    .expect("a second beneficial spell")
            };
            self.peer.event(
                &mut self.c,
                &dereth_protocol::qualities::MagicUpdateEnchantment(buff(second, 101)),
            );
            self.c.tick(8);
            second
        }
    }
    // -----------------------------------------------------------------------------------------
    // The recorded resists
    // -----------------------------------------------------------------------------------------

    /// The queue the shard puts a chat sentence on.
    pub const UI_QUEUE: u16 = 9;
    /// And the one it puts an effect on.
    pub const SMARTBOX_QUEUE: u16 = 10;

    /// The message a chat sentence arrives as, and the one an effect's sound arrives as.
    const TEXTBOX: u32 = 0xF7E0;
    const SOUND_EVENT: u32 = 0xF750;
    /// The sound the shard plays when a spell is resisted.
    const RESIST_SOUND: i32 = 0x91;
    /// The fragment of the shard's own sentence that marks one.
    const RESIST: &str = "resists your spell";
    /// The channel the shard marks it with.
    const MAGIC_CHANNEL: u32 = 7;

    /// Every server-to-client blob of `opcode` in `session`, with the time it arrived.
    ///
    /// `opcode` is the decoded corpus's own label, which for a message inside the ordered
    /// envelope is that message's **sub-type**; the payload it hands back still carries the whole
    /// envelope, so a body starts sixteen bytes in.
    pub fn recorded(session: &str, opcode: u32) -> Vec<(f64, Vec<u8>)> {
        let c = dereth_client_net::client_session::testing::Corpus::load(session)
            .expect("the decoded corpus parses")
            .unwrap_or_else(|| panic!("the decoded corpus has no session {session}"));
        let mut out = Vec::new();
        for b in c.blobs {
            if b.dir != dereth_client_net::client_session::testing::Direction::ServerToClient
                || b.opcode != opcode
            {
                continue;
            }
            let t = std::time::Duration::from_micros(b.t_rel_micros).as_secs_f64();
            out.push((t, b.payload));
        }
        out
    }

    /// Decode one recorded chat sentence with the production reader.
    pub fn textbox(blob: &[u8]) -> dereth_protocol::comms::CommunicationTextboxString {
        dereth_protocol::read_body_padded::<dereth_protocol::comms::CommunicationTextboxString>(
            &blob[4..],
        )
        .expect("the recorded body decodes")
    }

    /// Decode one recorded sound event with the production reader.
    pub fn sound(blob: &[u8]) -> dereth_protocol::objects::EffectsSoundEvent {
        dereth_protocol::read_body_padded::<dereth_protocol::objects::EffectsSoundEvent>(&blob[4..])
            .expect("the recorded body decodes")
    }

    /// The one recording that carries resists, and each resist paired with the sound that came
    /// with it: how long after the sentence the sound arrived, the sentence, the object the sound
    /// was played on, and its volume.
    ///
    /// The recordings are the ones the corpus index names, so a recording added or renamed changes
    /// what this reads rather than what it asserts.
    pub fn resists() -> (&'static str, Vec<(f64, String, ObjectId, f32)>) {
        let mut carrying: Vec<&'static str> = Vec::new();
        for (id, _slug) in dereth_client_net::client_session::testing::session_index() {
            let n = recorded(id, TEXTBOX)
                .iter()
                .filter(|(_, b)| textbox(b).text.contains(RESIST))
                .count();
            if n > 0 {
                carrying.push(id);
            }
        }
        assert_eq!(
            carrying.len(),
            1,
            "exactly one recording carries a resist -- the corpus has one run with any magic in \
             it at all: {carrying:?}"
        );
        let session = carrying[0];

        let texts: Vec<(f64, String)> = recorded(session, TEXTBOX)
            .iter()
            .map(|(t, b)| (*t, textbox(b)))
            .filter(|(_, m)| m.text.contains(RESIST))
            .map(|(t, m)| {
                assert_eq!(
                    m.text_type, MAGIC_CHANNEL,
                    "the channel is the shard's own and the client does not choose it"
                );
                (t, m.text)
            })
            .collect();
        let sounds: Vec<(f64, ObjectId, f32)> = recorded(session, SOUND_EVENT)
            .iter()
            .map(|(t, b)| (*t, sound(b)))
            .filter(|(_, m)| m.sound_type == RESIST_SOUND)
            .map(|(t, m)| (t, m.id, m.volume))
            .collect();
        assert_eq!(
            texts.len(),
            sounds.len(),
            "each resist the shard announces is paired with the sound it plays"
        );
        (
            session,
            texts
                .into_iter()
                .zip(sounds)
                .map(|((tt, text), (ts, id, volume))| (ts - tt, text, id, volume))
                .collect(),
        )
    }

    /// Every server-to-client ordered blob in `session` whose sub-type is `sub`.
    ///
    /// The decoded corpus labels an ordered blob with the **envelope's** opcode and hands back the
    /// whole envelope, so the sub-type is twelve bytes in and a body sixteen.
    pub fn recorded_event(session: &str, sub: u32) -> Vec<Vec<u8>> {
        const ORDERED_EVENT: u32 = 0xF7B0;
        recorded(session, ORDERED_EVENT)
            .into_iter()
            .map(|(_, b)| b)
            .filter(|b| {
                b.get(12..16)
                    .is_some_and(|w| u32::from_le_bytes(w.try_into().expect("four bytes")) == sub)
            })
            .collect()
    }

    /// The first recorded resist sentence, as the shard framed it.
    pub fn first_resist_blob(session: &str) -> Vec<u8> {
        recorded(session, TEXTBOX)
            .into_iter()
            .map(|(_, b)| b)
            .find(|b| textbox(b).text.contains(RESIST))
            .expect("the recording carries one")
    }

    /// The first recorded resist sound, re-addressed to this scenario's player and otherwise
    /// untouched.
    pub fn first_resist_sound_blob(session: &str) -> Vec<u8> {
        let mut b = recorded(session, SOUND_EVENT)
            .into_iter()
            .map(|(_, b)| b)
            .find(|b| sound(b).sound_type == RESIST_SOUND)
            .expect("the recording carries one");
        b[4..8].copy_from_slice(&PLAYER.0.to_le_bytes());
        b
    }
}

// -------------------------------------------------------------------------------------------
// magic.resist.*
//
// The recording's own bytes are replayed on the queue it puts them on; the sentence a scenario
// expects is the blob's own decoded text and never a constant, because the recording's first
// resist and its fourth name two different monsters and a scenario that wrote either down would
// be asserting its own transcription. Which recordings carry a resist is the first scenario's
// premise, read at run time.
// -------------------------------------------------------------------------------------------

/// The chat channel the shard marks a resist with.
const MAGIC_CHANNEL: u8 = 7;
/// The sound the shard plays with it.
const RESIST_SOUND: i32 = 0x91;

/// The shard's sentence is drawn verbatim, on the channel the shard marked it with.
pub fn a_recorded_resist_is_drawn_verbatim_on_the_magic_channel() {
    use support::{chat_log, resists, textbox, UI_QUEUE};

    // The premise, read out of the recordings rather than written down: one recording carries
    // resists, every sentence in it is paired with a resist sound on the caster's own object, and
    // the two arrive together.
    let (session, pairs) = resists();
    let paired = !pairs.is_empty()
        && pairs.iter().all(|(dt, _, id, volume)| {
            (0.0..0.050).contains(dt) && *id == pairs[0].2 && (volume - 1.0).abs() < f32::EPSILON
        });
    // …and the object they are on is the session's own player, not the monster.
    let on_the_caster = pairs[0].2 .0 >> 28 == 5;
    // Two different monsters produced them, so the name in the sentence really is the target's.
    let names: std::collections::BTreeSet<&str> =
        pairs.iter().map(|(_, text, _, _)| text.as_str()).collect();
    let two_monsters = names.len() == 2;

    let (mut c, mut peer) = support::a_client_with_a_peer();
    let before = chat_log(&mut c).len();

    let blob = support::first_resist_blob(session);
    let expected = textbox(&blob).text;
    peer.send(&mut c, UI_QUEUE, blob);
    c.tick(6);

    let after = chat_log(&mut c);
    let drawn = after[before..]
        .iter()
        .find(|(_, t)| t.contains("resists"))
        .cloned();
    let verbatim = drawn
        .as_ref()
        .is_some_and(|(ty, t)| t.trim_end() == expected && *ty == MAGIC_CHANNEL);
    // And the colour is the wire type's, not one the client chose: a client that wrote a zero
    // here would draw a magic line in the broadcast colour.
    let coloured =
        dereth_ui_screens::chat::colors::color_for_type(MAGIC_CHANNEL).hex == 0x003F_BFFF;

    c.assert_behaviour(
        "magic.resist.the-shards-own-sentence-is-drawn-verbatim-on-the-magic-channel",
        move |_| paired && on_the_caster && two_monsters && verbatim && coloured,
    );
    c.shutdown();
}

/// A squelched magic channel drops the same recorded bytes, and counts that it did.
pub fn a_squelched_magic_channel_drops_the_same_bytes() {
    use support::{chat_log, resists, UI_QUEUE};

    let (session, _) = resists();
    let (mut c, mut peer) = support::a_client_with_a_peer();
    c.world_mut()
        .chat
        .squelch
        .global
        .types
        .insert(u32::from(MAGIC_CHANNEL));
    let before = chat_log(&mut c).len();

    peer.send(&mut c, UI_QUEUE, support::first_resist_blob(session));
    c.tick(6);

    let after = chat_log(&mut c);
    let nothing_drawn = after[before..].iter().all(|(_, t)| !t.contains("resists"));
    // The gate counted it, so the silence is the squelch and not a decode that failed.
    let counted = c.view().expect_app().hud().stats.textbox_lines_squelched == 1;

    c.assert_behaviour(
        "magic.resist.a-squelched-magic-channel-drops-the-same-recorded-bytes",
        move |_| nothing_drawn && counted,
    );
    c.shutdown();
}

/// The sound that comes with a resist reaches the client's own sound queue.
pub fn the_resist_sound_reaches_the_clients_queue() {
    use support::{resists, sound, SMARTBOX_QUEUE};

    let (session, _) = resists();
    let (mut c, mut peer) = support::a_client_with_a_peer();
    let before = c.view().objects().stats.sound_events;

    // The recording's own sound blob, re-addressed to this session's player and otherwise
    // untouched.
    let blob = support::first_resist_sound_blob(session);
    let m = sound(&blob);
    let the_recorded_body = m.sound_type == RESIST_SOUND
        && (m.volume - 1.0).abs() < f32::EPSILON
        && m.id == support::PLAYER;

    peer.send(&mut c, SMARTBOX_QUEUE, blob);
    c.tick(6);

    let accepted = c.view().objects().stats.sound_events - before == 1;
    let nothing_dropped = c.view().objects().stats.unhandled == 0;

    c.assert_behaviour(
        "magic.resist.the-sound-that-comes-with-it-reaches-the-clients-own-queue",
        move |_| the_recorded_body && accepted && nothing_dropped,
    );
    c.shutdown();
}

#[test]
fn scenario_a_recorded_resist_is_drawn_verbatim_on_the_magic_channel() {
    scenario("a_recorded_resist_is_drawn_verbatim_on_the_magic_channel");
}

#[test]
fn scenario_a_squelched_magic_channel_drops_the_same_bytes() {
    scenario("a_squelched_magic_channel_drops_the_same_bytes");
}

#[test]
fn scenario_the_resist_sound_reaches_the_clients_queue() {
    scenario("the_resist_sound_reaches_the_clients_queue");
}

// -------------------------------------------------------------------------------------------
// magic.cast.*
//
// A fizzle, the shard's own recorded fizzle, and the refusals the client makes itself. Which
// recordings carry a fizzle is the recorded-fizzle scenario's premise, read at run time.
// -------------------------------------------------------------------------------------------

/// A fizzle ends the cast, prints its line, and burns nothing the player is carrying.
pub fn a_fizzle_ends_the_cast_prints_its_line_and_burns_nothing() {
    use cast::{CastBench, CASTING, FIZZLED, MAGIC_CHANNEL};

    let mut b = CastBench::new();
    let idle_before = b.busy_count() == 0;
    b.cast_at(Some(cast::DRUDGE));

    // The cast really went out and the client really went busy, or every silence below is a
    // silence after nothing.
    let went_out = b.wire_has_targeted_cast() && b.spells_cast() == 1 && b.busy_count() == 1;
    let announced = b
        .spew()
        .iter()
        .any(|s| s.trim_end() == format!("{CASTING}{}", cast::SPELL));
    let lines_before = b.chat().len();
    let carried: Vec<i64> = b.formula_slots().iter().map(|s| b.held(*s)).collect();

    b.use_done(cast::YOUR_SPELL_FIZZLED);

    let cast_is_idle_again = b.busy_count() == 0 && b.uses_done() == 1;
    let after = b.chat();
    let printed = after[lines_before..]
        .iter()
        .find(|(_, t)| t.contains("fizzl"))
        .is_some_and(|(ty, t)| t.trim_end() == FIZZLED && *ty == MAGIC_CHANNEL);

    // The client burned nothing of its own accord: the shard burns, and the client learns of it
    // from the stack update it sends afterwards.
    let now: Vec<i64> = b.formula_slots().iter().map(|s| b.held(*s)).collect();
    let burned_nothing = now == carried && now.iter().all(|n| *n >= 10);
    // …and the other half of the same rule: when the shard's own burn arrives, the count follows.
    let slots = b.formula_slots();
    let was = b.held(slots[0]);
    b.restock(0, slots[0], 9);
    let follows_the_shard = b.held(slots[0]) == was - 1;
    b.shutdown();

    // The control that separates "the client prints for this answer" from "for every answer":
    // a clean acknowledgement takes the busy count down and writes nothing.
    let mut b = CastBench::new();
    b.cast_at(Some(cast::DRUDGE));
    let busy = b.busy_count() == 1;
    let lines_before = b.chat().len();
    b.use_done(0);
    let after = b.chat();
    let clean_is_silent = b.busy_count() == 0
        && after[lines_before..]
            .iter()
            .all(|(_, t)| !t.contains("fizzl"));
    b.shutdown();

    let mut c = HeadlessClient::model();
    c.assert_behaviour(
        "magic.cast.a-fizzle-ends-the-cast-prints-its-line-and-burns-nothing",
        move |_| {
            idle_before
                && went_out
                && announced
                && cast_is_idle_again
                && printed
                && burned_nothing
                && follows_the_shard
                && busy
                && clean_is_silent
        },
    );
}

/// The shard's own recorded fizzle prints the same line, and leaves the cast where it was.
pub fn a_recorded_fizzle_prints_the_line_without_ending_the_cast() {
    use cast::{recorded_fizzles, CastBench, FIZZLED, MAGIC_CHANNEL};

    // The premise, read out of the recordings: at least one of them carries a fizzle on the
    // shard's error message, and every body it carries is the bare code.
    let (session, bodies) = recorded_fizzles();
    let the_corpus_carries_them = !bodies.is_empty()
        && bodies.iter().all(|b| {
            b.len() == 20
                && u32::from_le_bytes(b[16..20].try_into().expect("four bytes"))
                    == cast::YOUR_SPELL_FIZZLED
        });

    let mut b = CastBench::new();
    b.cast_at(Some(cast::DRUDGE));
    let busy = b.busy_count() == 1;
    let lines_before = b.chat().len();

    b.replay(
        bodies
            .into_iter()
            .next()
            .expect("the recording carries one"),
    );

    let after = b.chat();
    let printed = after[lines_before..]
        .iter()
        .find(|(_, t)| t.contains("fizzl"))
        .is_some_and(|(ty, t)| t.trim_end() == FIZZLED && *ty == MAGIC_CHANNEL);
    // The shard's error message says what went wrong; it does not end the cast. That belongs to
    // the acknowledgement, and a client that did both would end the cast twice.
    let cast_untouched = b.busy_count() == 1;
    b.shutdown();

    let _ = session;
    let mut c = HeadlessClient::model();
    c.assert_behaviour(
        "magic.cast.the-shards-recorded-fizzle-prints-the-same-line-and-does-not-end-the-cast",
        move |_| the_corpus_carries_them && busy && printed && cast_untouched,
    );
}

/// Every refusal the client makes itself is said in its own words and never reaches the shard.
pub fn a_refusal_the_client_makes_itself_never_reaches_the_shard() {
    use cast::{CastBench, CANNOT_CAST_ON, CASTING};

    // 1. A target the spell cannot be cast on: the refusal names the target and nothing goes out.
    let wrong_target = {
        let mut b = CastBench::new();
        let before = b.spew_lines();
        b.cast_at(Some(cast::STATUE));
        let nothing_sent = !b.wire_has_any_cast() && b.spells_cast() == 0 && b.busy_count() == 0;
        let bubbles = b.spew();
        let named = bubbles
            .iter()
            .find(|s| s.starts_with(CANNOT_CAST_ON))
            .is_some_and(|s| s.trim_end() == format!("{CANNOT_CAST_ON}{}", cast::STATUE_NAME));
        // Exactly one refusal, and the success line is past it.
        let once = b.spew_lines() - before == 1 && bubbles.iter().all(|s| !s.starts_with(CASTING));
        b.shutdown();
        nothing_sent && named && once
    };

    // 2. Nothing selected at all: a different sentence, reached before the target is looked at.
    let no_target = {
        let mut b = CastBench::new();
        b.cast_at(None);
        let nothing_sent = !b.wire_has_any_cast();
        let said = b
            .spew()
            .iter()
            .find(|s| s.contains("suitable target"))
            .is_some_and(|s| s.trim_end() == dereth_client_model::magic::messages::NEED_TARGET);
        b.shutdown();
        nothing_sent && said
    };

    // 3. A component missing: the third local exit, and it is taken before the target is looked
    // at at all.
    let missing_component = {
        let mut b = CastBench::new();
        b.drop_the_first_component_class();
        let out_of_the_pack = b.held(b.formula_slots()[0]) == 0;
        b.cast_at(Some(cast::DRUDGE));
        let nothing_sent = !b.wire_has_targeted_cast();
        let bubbles = b.spew();
        let said = bubbles
            .iter()
            .find(|s| s.contains("components"))
            .is_some_and(|s| {
                s.trim_end() == dereth_client_model::magic::messages::MISSING_COMPONENTS
            });
        let never_looked = bubbles.iter().all(|s| !s.starts_with(CASTING));
        b.shutdown();
        out_of_the_pack && nothing_sent && said && never_looked
    };

    // The positive control for all three: the same gesture at a legal target sends and announces,
    // so the silences above are refusals and not a bench that never casts anything.
    let control = {
        let mut b = CastBench::new();
        b.cast_at(Some(cast::DRUDGE));
        let sent = b.wire_has_targeted_cast();
        let bubbles = b.spew();
        let announced = bubbles
            .iter()
            .any(|s| s.trim_end() == format!("{CASTING}{}", cast::SPELL))
            && bubbles.iter().all(|s| !s.starts_with(CANNOT_CAST_ON));
        b.shutdown();
        sent && announced
    };

    let mut c = HeadlessClient::model();
    c.assert_behaviour(
        "magic.cast.a-refusal-the-client-makes-itself-is-said-in-its-own-words-and-never-sent",
        move |_| wrong_target && no_target && missing_component && control,
    );
}

#[test]
fn scenario_a_fizzle_ends_the_cast_prints_its_line_and_burns_nothing() {
    scenario("a_fizzle_ends_the_cast_prints_its_line_and_burns_nothing");
}

#[test]
fn scenario_a_recorded_fizzle_prints_the_line_without_ending_the_cast() {
    scenario("a_recorded_fizzle_prints_the_line_without_ending_the_cast");
}

#[test]
fn scenario_a_refusal_the_client_makes_itself_never_reaches_the_shard() {
    scenario("a_refusal_the_client_makes_itself_never_reaches_the_shard");
}

/// The bench the three casting scenarios share: a whole client whose character carries the whole
/// of one spell's formula, with a legal target and an illegal one in the world.
mod cast {
    use dereth_client_model::{NullSink, World};
    use dereth_primitives::ObjectId;
    use dereth_testkit::HeadlessClient;

    use super::support::{a_client_with_a_peer, chat_log, Peer, PLAYER};

    /// The shard's answer when a spell fizzles.
    pub const YOUR_SPELL_FIZZLED: u32 = 0x0402;
    /// The line the client writes for it, without the line break the chat system trims.
    pub const FIZZLED: &str = "Your spell fizzled.";
    /// The channel it is written on.
    pub const MAGIC_CHANNEL: u8 = 7;
    /// The refusal the client makes when a spell cannot be cast on what is selected.
    pub const CANNOT_CAST_ON: &str = "This spell cannot be cast on ";
    /// And the line it writes when a cast really starts.
    pub const CASTING: &str = "Casting ";

    /// The subject: a war spell with an eight-slot formula.
    const SUBJECT: u32 = 62;
    pub const SPELL: &str = "Acid Stream V";

    /// The legal target: an attackable creature.
    pub const DRUDGE: ObjectId = ObjectId(0x6000_0001);
    /// The illegal one: an object the client knows and that is not attackable.
    pub const STATUE: ObjectId = ObjectId(0x6000_0002);
    pub const STATUE_NAME: &str = "Ancient Statue";
    /// Somewhere that is not the player's pack.
    const ELSEWHERE: ObjectId = ObjectId(0x7000_0001);
    /// One component object per formula slot.
    const COMPONENTS: [ObjectId; 8] = [
        ObjectId(0x8000_0301),
        ObjectId(0x8000_0302),
        ObjectId(0x8000_0303),
        ObjectId(0x8000_0304),
        ObjectId(0x8000_0305),
        ObjectId(0x8000_0306),
        ObjectId(0x8000_0307),
        ObjectId(0x8000_0308),
    ];

    /// The bit that says an object may be attacked.
    const ATTACKABLE: u32 = 0x0010;
    const TYPE_CREATURE: u32 = 0x0000_0010;
    const TYPE_SPELL_COMPONENTS: u32 = 0x0000_1000;
    /// The quality that says this character has to spend components at all.
    const COMPONENTS_REQUIRED: u32 = 0x44;

    /// The two cast messages, by ordered sub-type.
    const CAST_TARGETED: u32 = 0x004A;
    const CAST_UNTARGETED: u32 = 0x0048;

    /// The two recorded fizzles, and the recording that carries them.
    ///
    /// The recordings are the ones the corpus index names. That the fizzles arrive as the shard's
    /// error message is read here rather than written down.
    pub fn recorded_fizzles() -> (&'static str, Vec<Vec<u8>>) {
        /// The acknowledgement, and the error message.
        const USE_DONE: u32 = 0x01C7;
        const WEENIE_ERROR: u32 = 0x028A;

        let is_fizzle = |b: &Vec<u8>| {
            b.len() >= 20
                && u32::from_le_bytes(b[16..20].try_into().expect("four bytes"))
                    == YOUR_SPELL_FIZZLED
        };
        let mut carrying: Vec<(&'static str, Vec<Vec<u8>>)> = Vec::new();
        let mut acknowledgements = 0usize;
        for (id, _slug) in dereth_client_net::client_session::testing::session_index() {
            // The acknowledgement is read only as the reader's own control. Over every recording
            // the index names, the fizzle is neither confined to one recording nor absent from the
            // acknowledgement, so this scenario claims neither.
            acknowledgements += super::support::recorded_event(id, USE_DONE).len();
            let errors: Vec<Vec<u8>> = super::support::recorded_event(id, WEENIE_ERROR)
                .into_iter()
                .filter(is_fizzle)
                .collect();
            if !errors.is_empty() {
                carrying.push((id, errors));
            }
        }
        assert!(
            acknowledgements > 0,
            "the reader has to be able to look: no acknowledgement anywhere means it is wrong"
        );
        assert!(
            !carrying.is_empty(),
            "no recording carries a fizzle on the shard's error message at all"
        );
        carrying.remove(0)
    }

    pub struct CastBench {
        c: HeadlessClient,
        peer: Peer,
    }

    impl CastBench {
        pub fn new() -> Self {
            let (mut c, mut peer) = a_client_with_a_peer();
            {
                let w = c.world_mut();
                let me = w.weenie_mut(PLAYER).expect("the player was created");
                me.pwd.name = "Aldis".into();
                me.pwd.obj_type = TYPE_CREATURE;
            }
            // The description is what pushes the shipped spell table, the component table and the
            // components-required quality into the game model.
            peer.event(&mut c, &a_caster());
            c.tick(8);
            assert!(
                c.view().expect_app().hud().player_desc_received,
                "the description reached the player-description arm"
            );
            assert!(
                c.view().world().magic.spell_table.is_some(),
                "without the shipped spell table the cast takes its silent no-table arm"
            );
            assert!(
                !c.view().world().magic.catalogue.is_empty(),
                "and without the component table no component resolves to a class"
            );
            assert!(
                c.view().world().are_spell_components_required(),
                "the premise: this character has to spend components, so the check runs"
            );

            for (id, name, bitfield) in [
                (DRUDGE, "Drudge Slave", ATTACKABLE),
                (STATUE, STATUE_NAME, 0),
            ] {
                let w = c.world_mut();
                put(
                    w,
                    id,
                    dereth_protocol::types::PublicWeenieDesc {
                        name: name.to_owned(),
                        obj_type: TYPE_CREATURE,
                        bitfield,
                        ..Default::default()
                    },
                );
            }

            let mut b = Self { c, peer };
            let slots = b.formula_slots();
            assert_eq!(
                slots.len(),
                8,
                "the subject spell has an eight-slot formula"
            );
            for (i, scid) in slots.iter().enumerate() {
                b.restock(i, *scid, 10);
            }
            b.c.tick(2);
            let _ = b.wire();
            b
        }

        /// The formula's components, out of the cast path's own source.
        pub fn formula_slots(&self) -> Vec<u32> {
            let w = self.c.view().world();
            let table = w
                .magic
                .spell_table
                .clone()
                .expect("the description pushed the table");
            let base = table.spells.get(&SUBJECT).expect("the subject spell");
            assert_eq!(base.name, SPELL);
            let f = w.spell_formula(base);
            let n = dereth_client_model::magic::num_spell_components(&f);
            f[..n].to_vec()
        }

        /// Put `stack` of one component class in the pack, through the seam the shard's own move
        /// arrives on -- never by a hand call into the tracker.
        pub fn restock(&mut self, slot: usize, scid: u32, stack: u16) {
            let (wcid, name) = {
                let cat = &self.c.view().world().magic.catalogue;
                let wcid = cat.scid_to_wcid(scid);
                assert_ne!(wcid, 0, "the component resolves to a class");
                (
                    wcid,
                    cat.inq_spell_component_base(scid)
                        .expect("a base")
                        .name
                        .clone(),
                )
            };
            let id = COMPONENTS[slot];
            let w = self.c.world_mut();
            put(
                w,
                id,
                dereth_protocol::types::PublicWeenieDesc {
                    name,
                    wcid,
                    obj_type: TYPE_SPELL_COMPONENTS,
                    stack_size: Some(stack),
                    container_id: Some(PLAYER),
                    ..Default::default()
                },
            );
            refresh_contents(w, PLAYER);
            w.server_says_move_item(id, PLAYER, 0, ObjectId(0), 0, true, &mut NullSink);
            self.c.tick(2);
        }

        /// Take every object of the first slot's class out of the pack. Owning is membership of
        /// the class, so one stack left behind would leave the cast legal.
        pub fn drop_the_first_component_class(&mut self) {
            let slots = self.formula_slots();
            let dropped: Vec<ObjectId> = slots
                .iter()
                .enumerate()
                .filter(|(_, s)| **s == slots[0])
                .map(|(i, _)| COMPONENTS[i])
                .collect();
            let w = self.c.world_mut();
            put(
                w,
                ELSEWHERE,
                dereth_protocol::types::PublicWeenieDesc::default(),
            );
            for id in &dropped {
                if let Some(x) = w.tables.weenies.get_mut(*id) {
                    x.pwd.container_id = Some(ELSEWHERE);
                }
            }
            refresh_contents(w, PLAYER);
            for id in &dropped {
                w.server_says_move_item(*id, ELSEWHERE, 0, ObjectId(0), 0, true, &mut NullSink);
            }
            self.c.tick(2);
        }

        /// How many of the class the client believes the player holds.
        pub fn held(&self, scid: u32) -> i64 {
            let w = self.c.view().world();
            let wcid = w.magic.catalogue.scid_to_wcid(scid);
            w.magic.components.num_component(&w.magic.catalogue, wcid)
        }

        /// Select a target the way the client's own selection does, then press Cast -- which is
        /// the only route into the cast at all.
        pub fn cast_at(&mut self, target: Option<ObjectId>) {
            self.c
                .world_mut()
                .set_selected_object(target, false, &mut NullSink);
            let mut panels = std::mem::take(&mut self.c.app_mut().hud_mut().panels);
            {
                let shell = self.c.app_mut().ui_mut().expect("the shell");
                let ui = &mut shell.ui;
                let tab = panels.spellcasting.open_sub_menu_index(ui);
                panels.spellcasting.set_selected(ui, tab, SUBJECT);
                panels.spellcasting.cast(ui);
            }
            self.c.app_mut().hud_mut().panels = panels;
            self.c.tick(3);
        }

        /// The shard's acknowledgement of the cast.
        pub fn use_done(&mut self, failure_type: u32) {
            self.peer.event(
                &mut self.c,
                &dereth_protocol::objects::ItemUseDone { failure_type },
            );
            self.c.tick(6);
        }

        /// Replay one recorded blob, re-addressed and re-stamped for this session: the recorded
        /// sequence numbers are the ones that session had reached, and replaying them into a
        /// session whose counter starts at zero would stall every one of them.
        pub fn replay(&mut self, blob: Vec<u8>) {
            self.peer.replay_blob(&mut self.c, blob);
            self.c.tick(6);
        }

        /// The ordered game actions this client has built into a datagram since the last look.
        fn wire(&mut self) -> Vec<u32> {
            let out = self
                .c
                .app_mut()
                .replay_network_mut()
                .expect("the replay endpoint")
                .take_outgoing();
            let mut ops = Vec::new();
            for (raw, _) in &out {
                let Ok(p) = dereth_transport::ParsedPacket::parse(raw) else {
                    continue;
                };
                for f in &p.fragments {
                    if f.payload.len() < dereth_protocol::OrderedActionHeader::PACK_SIZE + 4 {
                        continue;
                    }
                    if u32::from_le_bytes(f.payload[0..4].try_into().expect("four bytes"))
                        != dereth_protocol::OrderedActionHeader::MAGIC
                    {
                        continue;
                    }
                    ops.push(u32::from_le_bytes(
                        f.payload[8..12].try_into().expect("four bytes"),
                    ));
                }
            }
            ops
        }

        pub fn wire_has_targeted_cast(&mut self) -> bool {
            self.wire().contains(&CAST_TARGETED)
        }

        pub fn wire_has_any_cast(&mut self) -> bool {
            let w = self.wire();
            w.contains(&CAST_TARGETED) || w.contains(&CAST_UNTARGETED)
        }

        /// The strip the client's own refusals are drawn in. They are on a channel the shipped
        /// chat windows filter out, so this is where they are read.
        pub fn spew(&self) -> Vec<String> {
            let m = &self.c.view().expect_app().hud().panels.spew.model;
            m.items.iter().chain(m.pending.iter()).cloned().collect()
        }

        pub fn spew_lines(&self) -> u64 {
            self.c.view().expect_app().hud().stats.spew_lines
        }

        pub fn chat(&mut self) -> Vec<(u8, String)> {
            chat_log(&mut self.c)
        }

        pub fn busy_count(&self) -> u32 {
            self.c.view().world().magic.busy_count
        }

        pub fn spells_cast(&self) -> u64 {
            self.c.view().expect_app().interaction().stats.spells_cast
        }

        pub fn uses_done(&self) -> u64 {
            self.c.view().expect_app().interaction().stats.uses_done
        }

        pub fn shutdown(self) {
            self.c.shutdown();
        }
    }

    fn put(w: &mut World, id: ObjectId, pwd: dereth_protocol::types::PublicWeenieDesc) {
        let mut wn = dereth_client_model::weenie::Weenie::new(id);
        wn.pwd = pwd;
        w.tables.weenies.insert(id, wn);
    }

    fn refresh_contents(w: &mut World, container: ObjectId) {
        let held: Vec<dereth_protocol::types::ContentProfile> = w
            .tables
            .weenies
            .iter()
            .filter(|(_, x)| x.pwd.container_id == Some(container))
            .map(|(k, x)| dereth_protocol::types::ContentProfile {
                iid: k,
                container_properties: u32::from(x.is_container()),
            })
            .collect();
        w.view_object_contents(container, &held, &mut NullSink);
    }

    /// The description a character who has to spend components arrives with.
    fn a_caster() -> dereth_protocol::login::LoginPlayerDescription {
        use dereth_protocol::archive::PackedHash;
        use dereth_protocol::types::qualities::{
            base_flags, quality_flags, AcBaseQualities, AcQualities, PropertyTables,
        };
        let qualities = AcQualities {
            base: AcBaseQualities {
                flags: base_flags::BOOL,
                weenie_type: 0x0A,
                tables: PropertyTables {
                    bools: Some(PackedHash {
                        table_size: 8,
                        entries: vec![(COMPONENTS_REQUIRED, 1)],
                    }),
                    ..PropertyTables::default()
                },
            },
            flags: quality_flags::SKILLS,
            skills: Some(PackedHash {
                table_size: 8,
                entries: Vec::new(),
            }),
            ..AcQualities::default()
        };
        dereth_protocol::login::LoginPlayerDescription {
            qualities,
            player_module: dereth_protocol::login::PlayerModule {
                spell_bars: vec![Vec::new()],
                spell_filters: dereth_protocol::login::PlayerModule::DEFAULT_SPELL_FILTERS,
                options2: dereth_protocol::login::PlayerModule::DEFAULT_OPTIONS2,
                ..dereth_protocol::login::PlayerModule::default()
            },
            content_profiles: Vec::new(),
            inventory_placements: Vec::new(),
        }
    }
}

// -------------------------------------------------------------------------------------------
// vitae.*
//
// The corpus has no vitae in it at all -- nobody died on camera -- so every message here is
// constructed and said to be; what is not constructed is the route.
// -------------------------------------------------------------------------------------------

/// A live vitae lights the lamp and fills the panel with the penalty and what it will cost.
pub fn a_live_vitae_lights_the_lamp_and_fills_the_panel() {
    use vitae::{Vitae, CP_POOL, LEVEL_OF_THE_CHARACTER, TICKS};

    let mut v = Vitae::new();
    v.describe(None);

    // A character with no vitae is a known full-strength character, not an unknown one, and the
    // lamp is dark.
    let full_strength = v.value() == Some(1.0) && v.lamp_is_dark();

    v.arrives(TICKS[0]);

    let in_the_registry = v.registry_vitae() == TICKS[0];
    // It joins neither enchantment counter: a vitae is not one of the three lists.
    let counts_untouched = v.enchantment_counts() == (0, 0);
    let lamp_is_lit = v.value() == Some(TICKS[0]) && !v.lamp_is_dark();

    v.open_the_panel();
    let text = v.panel_text();
    let says_the_penalty = text.contains('5') && !text.contains("full strength");
    // …and what earning it back will cost, computed here from the character's own numbers rather
    // than through the function the panel used.
    #[allow(clippy::cast_possible_truncation)]
    let threshold = ((math::pow(f64::from(LEVEL_OF_THE_CHARACTER), 2.5) * 2.5 + 20.0)
        * math::pow(f64::from(TICKS[0]), 5.0)
        + 0.5) as i32;
    let need = threshold - CP_POOL;
    let says_what_it_costs =
        text.contains(&dereth_ui_screens::panels::statmgmt::num(i64::from(need)));
    v.shutdown();

    let mut c = HeadlessClient::model();
    c.assert_behaviour(
        "vitae.lamp.lights-for-a-live-penalty-and-the-panel-says-how-much",
        move |_| {
            full_strength
                && in_the_registry
                && counts_untouched
                && lamp_is_lit
                && says_the_penalty
                && says_what_it_costs
        },
    );
}

/// The panel follows every tick as the penalty wears off, and goes dark at full strength.
pub fn the_panel_follows_every_tick_and_goes_dark_at_full_strength() {
    use vitae::{Vitae, TICKS};

    let mut v = Vitae::new();
    v.describe(Some(TICKS[0]));
    v.open_the_panel();

    let mut seen: Vec<(String, bool, u32)> = Vec::new();
    for tick in TICKS {
        v.arrives(tick);
        seen.push((v.panel_text(), v.lamp_is_dark(), v.panel_updates()));
    }

    // Every arrival reached the panel, and each one wrote a different line.
    let rewritten_each_time = seen.windows(2).all(|w| w[1].2 > w[0].2 && w[0].0 != w[1].0);
    // The first three are penalties with the lamp lit; the last is full strength and dark.
    let penalties = seen
        .iter()
        .take(3)
        .all(|(t, dark, _)| !*dark && !t.contains("full strength"));
    let (last, dark, _) = &seen[3];
    let full_strength_is_dark = *dark && last.contains("full strength");
    // And the entry is still in the registry: the shard replaces it as it wears off rather than
    // taking it away.
    let still_there = (v.registry_vitae() - 1.0).abs() < f32::EPSILON;
    v.shutdown();

    let mut c = HeadlessClient::model();
    c.assert_behaviour(
        "vitae.panel.follows-every-tick-and-goes-dark-at-full-strength",
        move |_| rewritten_each_time && penalties && full_strength_is_dark && still_there,
    );
}

/// The penalty takes the skill down, leaves the row drawn plain, and never touches an attribute.
pub fn the_penalty_takes_the_skill_down_but_leaves_the_row_plain() {
    use vitae::{Vitae, TICKS};

    let mut v = Vitae::new();
    v.describe(None);
    let (base, base_font) = v.skill_row();
    let plain_to_start = base_font == 0 && base >= 55;
    let strength = v.strength();

    // The same character, logged in again with the vitae his death left him.
    v.describe(Some(TICKS[0]));
    let (penalised, font) = v.skill_row();

    // The number, computed here rather than through the client's own multiply.
    #[allow(clippy::cast_precision_loss, clippy::cast_possible_truncation)]
    let want = ((base as f32 * TICKS[0]) + 0.5) as i32;
    let took_it_down = penalised == want && penalised < base;
    // **And the row still draws plain.** The page compares the raw skill against the enchanted one
    // with only the vitae added back, so the term cancels; a build that dropped it would draw a
    // vitae-carrying character's whole list as though every skill were debuffed.
    let still_plain = font == 0;
    let footer = v.skill_entry();
    let footer_shows_it = footer.vitae < 0
        && footer.effective - footer.vitae == base
        && footer.effective == penalised;

    // The discriminating case neither a buff alone nor a penalty alone can reach: a buff **on top
    // of** the penalty is drawn green, not red.
    v.buff_the_skill(3.0);
    let (buffed, buffed_font) = v.skill_row();
    let a_buff_on_top_is_green = buffed > penalised && buffed_font == 1;

    // And an attribute never moves: the multiply is on the skill path and not on that one.
    let attribute_untouched = v.strength() == strength;
    v.shutdown();

    let mut c = HeadlessClient::model();
    c.assert_behaviour(
        "vitae.penalty.takes-the-skill-down-leaves-the-row-plain-and-spares-the-attributes",
        move |_| {
            plain_to_start
                && took_it_down
                && still_plain
                && footer_shows_it
                && a_buff_on_top_is_green
                && attribute_untouched
        },
    );
}

#[test]
fn scenario_a_live_vitae_lights_the_lamp_and_fills_the_panel() {
    scenario("a_live_vitae_lights_the_lamp_and_fills_the_panel");
}

#[test]
fn scenario_the_panel_follows_every_tick_and_goes_dark_at_full_strength() {
    scenario("the_panel_follows_every_tick_and_goes_dark_at_full_strength");
}

#[test]
fn scenario_the_penalty_takes_the_skill_down_but_leaves_the_row_plain() {
    scenario("the_penalty_takes_the_skill_down_but_leaves_the_row_plain");
}

/// The fixture the three vitae scenarios share.
mod vitae {
    use dereth_primitives::LocalTime;
    use dereth_protocol::types::qualities::{
        AcQualities, Attribute, AttributeCache, Enchantment as ProtocolEnchantment,
        EnchantmentRegistry, Skill as WireSkill, StatMod,
    };
    use dereth_testkit::HeadlessClient;
    use dereth_ui::{ElemHandle, ElementId};
    use dereth_ui_screens::view::GameView as _;

    use super::support::{a_client_with_a_peer, Peer, PLAYER};

    /// The lamp that lights when a character carries a vitae, and the panel behind it.
    const LAMP: ElementId = ElementId(0x1000_00F4);
    /// The id a dark lamp rests in.
    const DARK: u32 = 0x0D;

    /// The skill the penalty is read off.
    const A_SKILL: u32 = 0x2C;
    /// The character these scenarios build.
    const BASE_ATTRIBUTE: u32 = 100;
    const SKILL_RANKS: u32 = 50;
    const SKILL_INIT: u32 = 5;
    pub const LEVEL_OF_THE_CHARACTER: i32 = 40;
    /// Experience already earned back against the current vitae point.
    pub const CP_POOL: i32 = 1_000;
    /// The character's level, and the pool, as the shard keys them.
    const LEVEL: u32 = 25;
    const VITAE_CP_POOL: u32 = 129;

    /// The enchantment family a vitae belongs to, and the spell it is.
    const VITAE_FAMILY: u32 = 0x0080_0000 | 0x0000_4000;
    const SPELL_VITAE: u32 = 0x29A;
    /// The family an ordinary beneficial skill buff belongs to.
    const SKILL_BUFF: u32 = 0x0000_0010 | 0x0000_1000 | 0x0000_8000 | 0x0200_0000;

    /// Four arrivals, in order: a death's penalty, then it wearing off.
    pub const TICKS: [f32; 4] = [0.95, 0.96, 0.98, 1.00];

    pub struct Vitae {
        c: HeadlessClient,
        peer: Peer,
    }

    impl Vitae {
        pub fn new() -> Self {
            let (c, peer) = a_client_with_a_peer();
            Self { c, peer }
        }

        /// Log the character in, with or without the vitae his death left him.
        pub fn describe(&mut self, carrying: Option<f32>) {
            let registry = carrying.map(|m| EnchantmentRegistry {
                flags: EnchantmentRegistry::VITAE,
                vitae: Some(one(m)),
                ..EnchantmentRegistry::default()
            });
            self.peer.event(&mut self.c, &a_character(registry));
            self.c.tick(8);
            assert!(
                self.c.view().expect_app().hud().player_desc_received,
                "the description reached the player-description arm"
            );
            assert!(
                self.c.view().expect_app().hud().skills.len() > 30,
                "a bound page has a row per skill, or nothing below can look at one"
            );
        }

        /// One live arrival carrying a vitae.
        pub fn arrives(&mut self, multiplier: f32) {
            self.peer.event(
                &mut self.c,
                &dereth_protocol::qualities::MagicUpdateEnchantment(one(multiplier)),
            );
            self.c.tick(8);
        }

        /// An ordinary beneficial buff on the same skill, on top of whatever is there.
        pub fn buff_the_skill(&mut self, value: f32) {
            self.peer.event(
                &mut self.c,
                &dereth_protocol::qualities::MagicUpdateEnchantment(ProtocolEnchantment {
                    id: 4624,
                    category_word: 100,
                    power_level: 8,
                    start_time: 0.0,
                    duration: 1800.0,
                    caster: PLAYER,
                    degrade_modifier: 0.0,
                    degrade_limit: 0.0,
                    last_time_degraded: 0.0,
                    smod: StatMod {
                        kind: SKILL_BUFF,
                        key: A_SKILL,
                        value,
                    },
                    spell_set_id: None,
                }),
            );
            self.c.tick(8);
        }

        fn find(&mut self, id: ElementId) -> ElemHandle {
            let shell = self.c.app_mut().ui_mut().expect("the shell");
            let ui = &mut shell.ui;
            let screen = shell.flow.current_mut().expect("a screen");
            let any: &mut dyn std::any::Any = &mut **screen;
            let screen = any
                .downcast_mut::<dereth_ui_screens::screens::gameplay::GamePlayScreen>()
                .expect("the gameplay screen is up");
            let root = screen.root().expect("the gameplay root");
            ui.get_child_recursive(root, id)
                .unwrap_or_else(|| panic!("{:#010X} is not in the shipped tree", id.0))
        }

        pub fn lamp_is_dark(&mut self) -> bool {
            let h = self.find(LAMP);
            self.c
                .app_mut()
                .ui_mut()
                .expect("the shell")
                .ui
                .node(h)
                .expect("a live node")
                .state
                .0
                == DARK
        }

        /// Open the panel the way a player does: a pointer click on its own lamp.
        pub fn open_the_panel(&mut self) {
            let panel = self.find(dereth_ui_screens::panels::vitae::PANEL);
            let lamp = self.find(LAMP);
            {
                let ui = &mut self.c.app_mut().ui_mut().expect("the shell").ui;
                assert!(
                    !ui.node(panel).expect("a live node").region.flags.visible,
                    "every registered page starts hidden"
                );
                let b = ui.screen_box(lamp);
                assert!(b.is_valid(), "the lamp has no box to point at");
                let (x, y) = ((b.x0 + b.x1) / 2, (b.y0 + b.y1) / 2);
                ui.mouse_move(LocalTime(0.0), x, y);
                ui.mouse_down(dereth_ui::focus::action::PRIMARY_CLICK, x, y);
                ui.mouse_up(dereth_ui::focus::action::PRIMARY_CLICK, x, y, false);
            }
            self.c.tick(4);
            let ui = &mut self.c.app_mut().ui_mut().expect("the shell").ui;
            assert!(
                ui.node(panel).expect("a live node").region.flags.visible,
                "the click has to open the panel before its text can matter"
            );
        }

        /// The panel's drawn text, read off the element rather than off the model behind it.
        pub fn panel_text(&mut self) -> String {
            let h = self.find(dereth_ui_screens::panels::vitae::MAIN_TEXT);
            let ui = &mut self.c.app_mut().ui_mut().expect("the shell").ui;
            ui.text_element_mut(h)
                .expect("a text element")
                .glyphs
                .glyphs
                .iter()
                .filter_map(|g| char::from_u32(u32::from(g.data)))
                .collect()
        }

        pub fn panel_updates(&self) -> u32 {
            self.c.view().expect_app().hud().panels.vitae.updates
        }

        pub fn value(&self) -> Option<f32> {
            let app = self.c.view().expect_app();
            app.hud().view(app.objects()).vitae()
        }

        pub fn enchantment_counts(&self) -> (u32, u32) {
            let app = self.c.view().expect_app();
            app.hud().view(app.objects()).enchantment_counts()
        }

        pub fn registry_vitae(&self) -> f32 {
            self.c
                .view()
                .world()
                .player_qualities()
                .expect("the character's qualities")
                .enchantments
                .vitae_value()
        }

        /// The drawn number on the Skills page's row, with its colour.
        pub fn skill_row(&self) -> (i32, u32) {
            let app = self.c.view().expect_app();
            let r = app
                .hud()
                .panels
                .skills
                .rows
                .iter()
                .find(|r| r.skill == A_SKILL)
                .expect("the skill has a row on the page");
            (r.value, r.font)
        }

        /// The entry behind that row, which is what the footer's own segment is built on.
        pub fn skill_entry(&self) -> dereth_ui_screens::view::SkillEntry {
            self.c
                .view()
                .expect_app()
                .hud()
                .skills
                .iter()
                .find(|s| s.id == A_SKILL)
                .cloned()
                .expect("the row's entry")
        }

        pub fn strength(&self) -> Option<i32> {
            let app = self.c.view().expect_app();
            app.hud().view(app.objects()).attribute(1)
        }

        pub fn shutdown(self) {
            self.c.shutdown();
        }
    }

    /// One arrival carrying a vitae. A vitae is **permanent**, which is what keeps a purge off it,
    /// and its value is the multiplier the lamp and the skill path both read.
    fn one(multiplier: f32) -> ProtocolEnchantment {
        ProtocolEnchantment {
            id: SPELL_VITAE,
            category_word: 0,
            power_level: 1,
            start_time: 0.0,
            duration: -1.0,
            caster: PLAYER,
            degrade_modifier: 0.0,
            degrade_limit: 0.0,
            last_time_degraded: 0.0,
            smod: StatMod {
                kind: VITAE_FAMILY,
                key: 0,
                value: multiplier,
            },
            spell_set_id: None,
        }
    }

    fn attribute(v: u32) -> Attribute {
        Attribute {
            level_from_cp: 0,
            init_level: v,
            cp_spent: 0,
        }
    }

    fn specialised(ranks: u32) -> WireSkill {
        WireSkill {
            level_from_pp: u16::try_from(ranks).expect("the rank count is small"),
            format_version: 1,
            sac: 3,
            pp: 0,
            init_level: SKILL_INIT,
            resistance_of_last_check: 0,
            last_used_time: 0.0,
        }
    }

    /// The character, with `registry` for his enchantments.
    ///
    /// The description deliberately carries the character's level and **not** the level he died
    /// at, so the panel's fall-back from one to the other is the branch these scenarios take.
    fn a_character(
        registry: Option<EnchantmentRegistry>,
    ) -> dereth_protocol::login::LoginPlayerDescription {
        use dereth_protocol::archive::PackedHash;
        use dereth_protocol::types::qualities::{
            attribute_cache_mask as m, base_flags, quality_flags, AcBaseQualities, PropertyTables,
        };
        let cache = AttributeCache {
            flags: m::STRENGTH | m::COORDINATION,
            strength: Some(attribute(BASE_ATTRIBUTE)),
            coordination: Some(attribute(BASE_ATTRIBUTE)),
            ..AttributeCache::default()
        };
        let mut flags = quality_flags::ATTRIBUTE_CACHE | quality_flags::SKILLS;
        if registry.is_some() {
            flags |= quality_flags::ENCHANTMENT_REGISTRY;
        }
        dereth_protocol::login::LoginPlayerDescription {
            qualities: AcQualities {
                base: AcBaseQualities {
                    flags: base_flags::INT,
                    weenie_type: 0x0A,
                    tables: PropertyTables {
                        ints: Some(PackedHash {
                            table_size: 8,
                            entries: vec![
                                (LEVEL, LEVEL_OF_THE_CHARACTER),
                                (VITAE_CP_POOL, CP_POOL),
                            ],
                        }),
                        ..PropertyTables::default()
                    },
                },
                flags,
                attribute_cache: Some(cache),
                skills: Some(PackedHash {
                    table_size: 8,
                    entries: vec![(A_SKILL, specialised(SKILL_RANKS))],
                }),
                enchantments: registry,
                ..AcQualities::default()
            },
            player_module: dereth_protocol::login::PlayerModule {
                spell_bars: vec![Vec::new()],
                spell_filters: dereth_protocol::login::PlayerModule::DEFAULT_SPELL_FILTERS,
                options2: dereth_protocol::login::PlayerModule::DEFAULT_OPTIONS2,
                ..dereth_protocol::login::PlayerModule::default()
            },
            content_profiles: Vec::new(),
            inventory_placements: Vec::new(),
        }
    }
}

// ---------------------------------------------------------------------------------------------
// spell-examine.*
//
// The spell-examine pane: the description a secondary click on a spell opens, the appraisal a look
// cancels, the per-account taper formula, and what carrying a foci changes in it.
//
// **Every number the pane draws is read out of the shipped tables at run time.** The three spell
// **ids** below are the only data values here; the name, description, component names, mana,
// range and duration are read off `Hud::spell_table` and `Hud::component_catalogue`, which is
// where the pane itself reads them. The per-account and per-foci formulas are derived the same
// way, through `dereth_client_model::magic`, whose own arithmetic `dereth-client-model` tests.
//
// **The secondary click is written here and not in `dereth_testkit::player`.** `Player::Click` is
// the primary button only, and the whole of this family is the *other* one, so the gesture is
// local to this file.
// ---------------------------------------------------------------------------------------------

/// `Flame Bolt I` -- War Magic, a bolt: five components, a range and no duration.
const FLAME_BOLT: u32 = 27;
/// `Strength Self I` -- Creature Enchantment: a duration and no range at all.
const STRENGTH_SELF: u32 = 2;
/// `Regeneration Self V` -- Life Magic, and an eight-slot formula two of whose slots are tapers.
const REGENERATION_SELF_V: u32 = 169;
/// Life Magic, which is the school the foci these scenarios carry belongs to.
const LIFE_MAGIC: u32 = 2;
/// Synthetic. Two strings invented here; neither is an account.
const AN_ACCOUNT: &str = "derer66done";
const ANOTHER_ACCOUNT: &str = "derer66dtwo";

/// The object the player is appraising when a spell is looked at. Any id will do: what is under
/// test is what goes out, and the only thing the client asks of the id is that it is not zero.
const AN_ITEM: dereth_primitives::ObjectId = dereth_primitives::ObjectId(0x8000_0123);

pub fn a_secondary_click_on_a_spell_opens_its_description() {
    use dereth_ui_screens::panels::{examination, spell_examine};

    let mut c = examine::a_book_of_two();
    let shut_before = examine::window_is_up(&mut c);
    let spell = examine::shipped_spell(&c, FLAME_BOLT);
    let components = examine::component_names(&c, &spell.comps);

    assert_eq!(
        spell.school, 1,
        "the shipped row is the war bolt this scenario drives"
    );
    assert!(
        spell.base_mana >= 1 && spell.mana_mod == 0,
        "with a plain mana cost"
    );
    assert!(spell.duration.is_none(), "a bolt is not an enchantment");
    assert_eq!(components.len(), 5, "and its formula has five slots");

    let (x, y) = examine::spellbook_row_point(&mut c, 0, FLAME_BOLT);
    examine::press(&mut c, x, y, 100_000, examine::SECONDARY);

    // Every expected line is built from the shipped row, not written down.
    let school_line = format!("School: {}", examine::school_name(spell.school));
    let mana_line = format!("Mana: {}", spell.base_mana);
    let range_line = format!("Range: {:.1} yds.", examine::yards(&spell));
    let mut description = spell.description.clone();
    description.push_str("\n\nCOMPONENTS:");
    for n in &components {
        description.push_str("\n     ");
        description.push_str(n);
    }

    let snap = c.ui_snapshot();
    let drawn = (
        snap.is_visible(examination::WINDOW),
        snap.is_visible(spell_examine::BASE),
        snap.text_of(examination::DISPLAYED_NAME_TEXT).to_owned(),
        snap.text_of(spell_examine::MAGIC_SCHOOL_TEXT).to_owned(),
        snap.text_of(spell_examine::MANA_TEXT).to_owned(),
        snap.text_of(spell_examine::RANGE_TEXT).to_owned(),
        snap.text_of(spell_examine::DURATION_TEXT).to_owned(),
        snap.text_of(spell_examine::DISPLAY_TEXT).to_owned(),
    );
    let pane = examine::pane_facts(&mut c);
    // The cursor is the half the client must not get wrong: looking at a spell and looking at
    // nothing are two different things, and only one of them arms the pointer.
    let cursor = c.view().expect_app().interaction().target_mode();
    let sent = c.outbound().len();
    let name = spell.name.clone();

    c.assert_behaviour(
        "spell-examine.pane.a-secondary-click-on-a-known-spell-opens-its-description",
        move |_| {
            !shut_before
                && drawn.0
                && drawn.1
                && drawn.2 == name
                && drawn.3 == school_line
                && drawn.4 == mana_line
                && drawn.5 == range_line
                && drawn.6.is_empty()
                && drawn.7 == description
                && pane.spell == FLAME_BOLT
                && pane.component_names == components
                && pane.rows_drawn == 5
                && pane.component_scids.len() == 5
                && cursor == dereth_client::interaction::TargetMode::None
                && sent == 0
        },
    );
    c.shutdown();
}

pub fn the_primary_click_selects_the_row_and_examines_nothing() {
    use dereth_ui_screens::panels::examination;

    let mut c = examine::a_book_of_two();
    let (x, y) = examine::spellbook_row_point(&mut c, 0, FLAME_BOLT);
    examine::press(&mut c, x, y, 100_000, examine::PRIMARY);

    let up = c.ui_snapshot().is_visible(examination::WINDOW);
    let pulled = examine::pane_facts(&mut c).examines_pulled;
    let selected = c.view().expect_app().hud().panels.spellbook.selected_spell;

    c.assert_behaviour(
        "spell-examine.pane.the-primary-click-selects-the-row-and-examines-nothing",
        move |_| !up && pulled == 0 && selected == FLAME_BOLT,
    );
    c.shutdown();
}

pub fn the_secondary_click_moves_the_books_own_selection_too() {
    let mut c = examine::a_book_of_two();
    let before = c.view().expect_app().hud().panels.spellbook.selected_spell;
    let name = examine::shipped_spell(&c, STRENGTH_SELF).name;

    let (x, y) = examine::spellbook_row_point(&mut c, 1, STRENGTH_SELF);
    examine::press(&mut c, x, y, 100_000, examine::SECONDARY);

    let title = examine::title(&mut c);
    let after = c.view().expect_app().hud().panels.spellbook.selected_spell;

    c.assert_behaviour(
        "spell-examine.pane.the-secondary-click-moves-the-books-own-selection-too",
        move |_| before != STRENGTH_SELF && after == STRENGTH_SELF && title == name,
    );
    c.shutdown();
}

pub fn an_enchantment_shows_how_long_it_lasts_and_no_range() {
    use dereth_ui_screens::panels::spell_examine;

    let mut c = examine::a_book_of_two();
    let spell = examine::shipped_spell(&c, STRENGTH_SELF);
    let components = examine::component_names(&c, &spell.comps);
    let seconds = spell
        .duration
        .map(|(d, _, _)| d)
        .expect("an enchantment lasts a while");

    assert!(
        seconds >= 60.0,
        "the shipped row is long enough for the minutes arm"
    );
    assert!(
        spell.base_range_constant == 0.0 && spell.base_range_mod == 0.0,
        "and it reaches nowhere, which is what leaves the range line empty"
    );

    let (x, y) = examine::spellbook_row_point(&mut c, 1, STRENGTH_SELF);
    examine::press(&mut c, x, y, 100_000, examine::SECONDARY);

    let school_line = format!("School: {}", examine::school_name(spell.school));
    let mana_line = format!("Mana: {}", spell.base_mana);
    #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
    let duration_line = format!("Duration: {} min.", (seconds / 60.0).round() as u32);

    let snap = c.ui_snapshot();
    let drawn = (
        snap.text_of(spell_examine::MAGIC_SCHOOL_TEXT).to_owned(),
        snap.text_of(spell_examine::MANA_TEXT).to_owned(),
        snap.text_of(spell_examine::DURATION_TEXT).to_owned(),
        snap.text_of(spell_examine::RANGE_TEXT).to_owned(),
    );
    let pane = examine::pane_facts(&mut c);

    c.assert_behaviour(
        "spell-examine.pane.an-enchantment-shows-how-long-it-lasts-and-no-range",
        move |_| {
            drawn.0 == school_line
                && drawn.1 == mana_line
                && drawn.2 == duration_line
                && drawn.3.is_empty()
                && pane.component_names == components
        },
    );
    c.shutdown();
}

pub fn a_second_look_re_opens_the_window_the_close_control_shut() {
    use dereth_ui_screens::panels::examination;

    let mut c = examine::a_book_of_two();
    let name = examine::shipped_spell(&c, FLAME_BOLT).name;
    let (x, y) = examine::spellbook_row_point(&mut c, 0, FLAME_BOLT);

    examine::press(&mut c, x, y, 100_000, examine::SECONDARY);
    let opened = c.ui_snapshot().is_visible(examination::WINDOW);
    let closes_before = examine::pane_facts(&mut c).closed;

    let (cx, cy) = examine::centre_of(&mut c, examination::CLOSE_BUTTON);
    examine::press(&mut c, cx, cy, 200_000, examine::PRIMARY);
    let shut = !c.ui_snapshot().is_visible(examination::WINDOW);
    let closes_after = examine::pane_facts(&mut c).closed;

    examine::press(&mut c, x, y, 300_000, examine::SECONDARY);
    let re_opened = c.ui_snapshot().is_visible(examination::WINDOW);
    let title = examine::title(&mut c);

    c.assert_behaviour(
        "spell-examine.pane.a-second-look-re-opens-the-window-the-close-control-shut",
        move |_| opened && shut && closes_after == closes_before + 1 && re_opened && title == name,
    );
    c.shutdown();
}

pub fn the_same_look_from_the_cast_bar_opens_it_without_selecting() {
    use dereth_ui_screens::panels::{examination, spell_examine};

    let mut c = examine::a_book_of_two();
    // The favourite bank the character's own description carries, and the stance the bar is shown
    // in.
    {
        let w = c.world_mut();
        w.player_system.spell_tabs[0] = vec![FLAME_BOLT];
        w.combat.combat_mode = dereth_client_model::combat::CombatMode::Magic;
    }
    c.tick(3);
    let spell = examine::shipped_spell(&c, FLAME_BOLT);
    let name = spell.name.clone();
    let components = examine::component_names(&c, &spell.comps);
    let shut_before = examine::window_is_up(&mut c);

    let (tab, x, y) = {
        let app = c.view().expect_app();
        let ui = &app.ui().expect("the UI shell").ui;
        let tab = app.hud().panels.spellcasting.open_sub_menu_index(ui);
        let list = app.hud().panels.spellcasting.lists[tab]
            .as_ref()
            .expect("the bound sub-menu list");
        assert_eq!(
            list.slots.first().and_then(|s| s.spell),
            Some(FLAME_BOLT),
            "the favourite reached the bar"
        );
        let b = ui.screen_box(list.slots[0].handle);
        assert!(
            b.width() > 0 && b.height() > 0,
            "and the row has an extent to press on"
        );
        (tab, b.x0 + 4, b.y0 + 4)
    };
    let selected_before =
        c.view().expect_app().hud().panels.spellcasting.sub_menus[tab].selected_spell;

    examine::press(&mut c, x, y, 100_000, examine::SECONDARY);

    let snap = c.ui_snapshot();
    let up = (
        snap.is_visible(examination::WINDOW),
        snap.is_visible(spell_examine::BASE),
    );
    let title = examine::title(&mut c);
    let pane = examine::pane_facts(&mut c);
    let selected_after =
        c.view().expect_app().hud().panels.spellcasting.sub_menus[tab].selected_spell;

    c.assert_behaviour(
        "spell-examine.pane.the-same-look-from-the-cast-bar-opens-it-without-selecting",
        move |_| {
            !shut_before
                && up.0
                && up.1
                && title == name
                && pane.component_names == components
                && selected_after == selected_before
        },
    );
    c.shutdown();
}

pub fn a_look_cancels_an_appraisal_in_flight_and_only_the_first_of_two_does() {
    use dereth_client::interaction::TargetMode;
    use dereth_client_contract::UiRequest;
    use dereth_testkit::Player;

    let mut c = examine::a_book_of_two();
    let name = examine::shipped_spell(&c, FLAME_BOLT).name;

    // The toolbar's Examine button with a selection: the only way this client puts an appraisal in
    // flight without a pick in the scene.
    let mark = c.outbound().len();
    c.when(Player::ui(UiRequest::Examine(AN_ITEM))).tick(3);
    let asked = examine::appraisals(&c, mark);
    let in_flight = examine::pane_facts(&mut c).awaiting;

    let (x, y) = examine::spellbook_row_point(&mut c, 0, FLAME_BOLT);
    let mark = c.outbound().len();
    examine::press(&mut c, x, y, 100_000, examine::SECONDARY);
    let cancelled = examine::appraisals(&c, mark);
    let cursor_after_cancel = c.view().expect_app().interaction().target_mode();
    let still_examining = c.view().world().appraisal.examining;
    let pane = examine::pane_facts(&mut c);
    let title = examine::title(&mut c);

    // The second look: both ids are already clear, so it has nothing left to forget.
    let (x2, y2) = examine::spellbook_row_point(&mut c, 1, STRENGTH_SELF);
    let mark = c.outbound().len();
    examine::press(&mut c, x2, y2, 200_000, examine::SECONDARY);
    let again = examine::appraisals(&c, mark);
    let second_pane = examine::pane_facts(&mut c);

    // The other zero, in the same scenario, so the two meanings cannot collapse into one: looking
    // at *nothing* arms the pointer and asks the shard nothing at all.
    let mark = c.outbound().len();
    c.when(Player::ui(UiRequest::Examine(dereth_primitives::ObjectId(
        0,
    ))))
    .tick(3);
    let null_look = (
        examine::appraisals(&c, mark),
        c.view().expect_app().interaction().target_mode(),
    );

    c.assert_behaviour(
        "spell-examine.cancel.a-look-cancels-an-appraisal-in-flight-and-only-the-first-of-two-does",
        move |_| {
            asked == vec![AN_ITEM]
                && in_flight == Some(AN_ITEM)
                && cancelled == vec![dereth_primitives::ObjectId(0)]
                && cursor_after_cancel == TargetMode::None
                && still_examining.is_none()
                && pane.awaiting.is_none()
                && pane.current.is_none()
                && pane.cancels == 1
                && pane.examines_pulled == 1
                && title == name
                && again.is_empty()
                && second_pane.examines_pulled == 2
                && second_pane.cancels == 1
                && null_look.0.is_empty()
                && null_look.1 == TargetMode::Examine
        },
    );
    c.shutdown();
}

pub fn a_look_with_nothing_in_flight_asks_the_shard_nothing() {
    use dereth_ui_screens::panels::examination;

    let mut c = examine::a_book_of_two();
    let name = examine::shipped_spell(&c, FLAME_BOLT).name;
    let quiet = examine::pane_facts(&mut c);

    let (x, y) = examine::spellbook_row_point(&mut c, 0, FLAME_BOLT);
    let mark = c.outbound().len();
    examine::press(&mut c, x, y, 100_000, examine::SECONDARY);

    let sent = examine::appraisals(&c, mark);
    let up = c.ui_snapshot().is_visible(examination::WINDOW);
    let title = examine::title(&mut c);
    let pane = examine::pane_facts(&mut c);

    c.assert_behaviour(
        "spell-examine.cancel.a-look-with-nothing-in-flight-asks-the-shard-nothing",
        move |_| {
            quiet.awaiting.is_none()
                && quiet.current.is_none()
                && up
                && title == name
                && sent.is_empty()
                && pane.examines_pulled == 1
                && pane.cancels == 0
        },
    );
    c.shutdown();
}

pub fn the_account_the_shard_names_is_the_one_the_client_keeps() {
    let mut c = examine::a_client();
    let before = c.view().world().player_system.account.clone();

    examine::greet(&mut c, AN_ACCOUNT);
    let first = c.view().world().player_system.account.clone();
    examine::greet(&mut c, ANOTHER_ACCOUNT);
    let second = c.view().world().player_system.account.clone();

    c.assert_behaviour(
        "spell-examine.formula.the-account-the-shard-names-is-the-one-the-client-keeps",
        move |_| before.is_empty() && first == AN_ACCOUNT && second == ANOTHER_ACCOUNT,
    );
    c.shutdown();
}

pub fn the_formula_the_client_would_cast_follows_that_account() {
    let mut c = examine::a_client();
    let spell = examine::shipped_spell(&c, REGENERATION_SELF_V);
    let stored = dereth_client_model::magic::decrypt_formula(&spell.raw_comps, spell.comp_key);

    assert_eq!(
        spell.school, LIFE_MAGIC,
        "the shipped row is the Life spell this scenario drives"
    );
    assert_eq!(
        stored.iter().filter(|s| **s != 0).count(),
        8,
        "and its formula fills every slot"
    );

    let nameless = c.view().world().spell_formula(&spell);
    examine::greet(&mut c, AN_ACCOUNT);
    let mine = c.view().world().spell_formula(&spell);
    examine::greet(&mut c, ANOTHER_ACCOUNT);
    let theirs = c.view().world().spell_formula(&spell);

    c.assert_behaviour(
        "spell-examine.formula.the-formula-the-client-would-cast-follows-that-account",
        move |_| mine != nameless && mine != stored && theirs != mine && theirs != stored,
    );
    c.shutdown();
}

pub fn the_look_lists_the_tapers_this_account_must_carry() {
    let mut c = examine::a_book_of_one(REGENERATION_SELF_V, AN_ACCOUNT);
    let spell = examine::shipped_spell(&c, REGENERATION_SELF_V);
    let stored = dereth_client_model::magic::decrypt_formula(&spell.raw_comps, spell.comp_key);
    let mine = examine::component_names(&c, &c.view().world().spell_formula(&spell));
    let from_the_table = examine::component_names(&c, &stored);
    let theirs = {
        let mut f = stored;
        dereth_client_model::magic::randomize_for_name(
            &mut f,
            ANOTHER_ACCOUNT,
            spell.formula_version,
        );
        examine::component_names(&c, &f)
    };

    assert_ne!(
        mine, from_the_table,
        "the premise: this account's tapers are not the table's"
    );
    assert_ne!(mine, theirs, "nor the other account's");

    let (x, y) = examine::spellbook_row_point(&mut c, 0, REGENERATION_SELF_V);
    examine::press(&mut c, x, y, 100_000, examine::SECONDARY);
    let pane = examine::pane_facts(&mut c);

    c.assert_behaviour(
        "spell-examine.formula.the-look-lists-the-tapers-this-account-must-carry",
        move |_| {
            pane.spell == REGENERATION_SELF_V
                && pane.component_names == mine
                && pane.component_names != from_the_table
                && pane.component_names != theirs
                && pane.rows_drawn == 8
        },
    );
    c.shutdown();
}

pub fn two_accounts_are_shown_different_tapers_for_one_spell() {
    let mut c = examine::a_book_of_one(REGENERATION_SELF_V, ANOTHER_ACCOUNT);
    let spell = examine::shipped_spell(&c, REGENERATION_SELF_V);
    let theirs = examine::component_names(&c, &c.view().world().spell_formula(&spell));
    let mine = {
        let mut f = dereth_client_model::magic::decrypt_formula(&spell.raw_comps, spell.comp_key);
        dereth_client_model::magic::randomize_for_name(&mut f, AN_ACCOUNT, spell.formula_version);
        examine::component_names(&c, &f)
    };

    let (x, y) = examine::spellbook_row_point(&mut c, 0, REGENERATION_SELF_V);
    examine::press(&mut c, x, y, 100_000, examine::SECONDARY);
    let pane = examine::pane_facts(&mut c);

    c.assert_behaviour(
        "spell-examine.formula.two-accounts-are-shown-different-tapers-for-one-spell",
        move |_| {
            pane.spell == REGENERATION_SELF_V
                && pane.component_names == theirs
                && pane.component_names != mine
        },
    );
    c.shutdown();
}

pub fn the_shipped_foci_table_reaches_the_game_model_with_the_description() {
    let mut c = examine::a_client();
    let shipped: Vec<(u32, u32)> = {
        let mut rows = c.view().expect_app().hud().school_pack_wcid.clone();
        rows.sort_unstable();
        rows
    };
    assert!(
        shipped.len() >= 5,
        "the shipped mapper names a foci for each of the spell schools"
    );
    assert!(
        shipped.iter().all(|(_, w)| *w != 0),
        "and every one is a real class"
    );

    let before: Vec<u32> = shipped
        .iter()
        .map(|(s, _)| c.view().world().school_of_magic_to_wcid(*s))
        .collect();
    examine::describe(&mut c);
    let after: Vec<u32> = shipped
        .iter()
        .map(|(s, _)| c.view().world().school_of_magic_to_wcid(*s))
        .collect();
    let want: Vec<u32> = shipped.iter().map(|(_, w)| *w).collect();
    // A school the shipped table has no foci for stays at nothing on either side.
    let unmapped: u32 = (1..64)
        .find(|s| !shipped.iter().any(|(k, _)| k == s))
        .expect("a school with none");
    let none = c.view().world().school_of_magic_to_wcid(unmapped);

    c.assert_behaviour(
        "spell-examine.formula.the-shipped-foci-table-reaches-the-game-model-with-the-description",
        move |_| before.iter().all(|w| *w == 0) && after == want && none == 0,
    );
    c.shutdown();
}

pub fn carrying_a_foci_changes_what_the_client_would_spend() {
    let mut c = examine::a_client();
    examine::greet(&mut c, AN_ACCOUNT);
    examine::describe(&mut c);
    let me = examine::stand_in_the_world(&mut c);
    let spell = examine::shipped_spell(&c, REGENERATION_SELF_V);

    let before = c.view().world().spell_formula(&spell);
    let foci = examine::foci_of(&c, LIFE_MAGIC);
    examine::carry_the_foci(&mut c, me, foci);
    let after = c.view().world().spell_formula(&spell);

    let before_len = before.iter().filter(|s| **s != 0).count();
    let after_len = after.iter().filter(|s| **s != 0).count();

    c.assert_behaviour(
        "spell-examine.formula.carrying-a-foci-changes-what-the-client-would-spend",
        move |_| before_len == 8 && after_len < before_len && after != before,
    );
    c.shutdown();
}

pub fn a_foci_for_another_school_leaves_the_long_formula_alone() {
    let mut c = examine::a_client();
    examine::greet(&mut c, AN_ACCOUNT);
    examine::describe(&mut c);
    let me = examine::stand_in_the_world(&mut c);
    let spell = examine::shipped_spell(&c, REGENERATION_SELF_V);

    let before = c.view().world().spell_formula(&spell);
    let wrong = examine::foci_of(&c, examine::another_school(&c, LIFE_MAGIC));
    assert_ne!(
        wrong,
        examine::foci_of(&c, LIFE_MAGIC),
        "the premise: it is another school's"
    );
    examine::carry_the_foci(&mut c, me, wrong);
    let after = c.view().world().spell_formula(&spell);

    c.assert_behaviour(
        "spell-examine.formula.a-foci-for-another-school-leaves-the-long-formula-alone",
        move |_| after == before && before.iter().filter(|s| **s != 0).count() == 8,
    );
    c.shutdown();
}

pub fn the_look_with_a_foci_lists_a_scarab_and_four_tapers() {
    let mut c = examine::a_client();
    examine::greet(&mut c, AN_ACCOUNT);
    examine::describe(&mut c);
    let me = examine::stand_in_the_world(&mut c);
    let foci = examine::foci_of(&c, LIFE_MAGIC);
    examine::carry_the_foci(&mut c, me, foci);
    examine::learn_and_open(&mut c, &[REGENERATION_SELF_V]);

    let spell = examine::shipped_spell(&c, REGENERATION_SELF_V);
    let want = examine::component_names(&c, &c.view().world().spell_formula(&spell));
    assert_eq!(want.len(), 5, "a scarab and four tapers");
    assert_eq!(
        want[1..]
            .iter()
            .collect::<std::collections::BTreeSet<_>>()
            .len(),
        1,
        "and the four are all one component"
    );

    let (x, y) = examine::spellbook_row_point(&mut c, 0, REGENERATION_SELF_V);
    examine::press(&mut c, x, y, 100_000, examine::SECONDARY);
    let pane = examine::pane_facts(&mut c);

    c.assert_behaviour(
        "spell-examine.formula.the-look-with-a-foci-lists-a-scarab-and-four-tapers",
        move |_| {
            pane.spell == REGENERATION_SELF_V
                && pane.component_names == want
                && pane.rows_drawn == 5
        },
    );
    c.shutdown();
}

pub fn the_look_without_a_foci_lists_the_long_per_account_formula() {
    let mut c = examine::a_client();
    examine::greet(&mut c, AN_ACCOUNT);
    examine::describe(&mut c);
    let me = examine::stand_in_the_world(&mut c);
    examine::learn_and_open(&mut c, &[REGENERATION_SELF_V]);

    let spell = examine::shipped_spell(&c, REGENERATION_SELF_V);
    let long = examine::component_names(&c, &c.view().world().spell_formula(&spell));
    assert_eq!(long.len(), 8, "the long formula fills every slot");

    let (x, y) = examine::spellbook_row_point(&mut c, 0, REGENERATION_SELF_V);
    examine::press(&mut c, x, y, 100_000, examine::SECONDARY);
    let pane = examine::pane_facts(&mut c);

    // What the same spell on the same client would cost *with* the foci, so "no taper in this
    // list" is a comparison against the short list and not against a word.
    let foci = examine::foci_of(&c, LIFE_MAGIC);
    examine::carry_the_foci(&mut c, me, foci);
    let short = examine::component_names(&c, &c.view().world().spell_formula(&spell));
    let taper = short[1].clone();
    assert_eq!(short.len(), 5, "the foci really does shorten it");

    c.assert_behaviour(
        "spell-examine.formula.the-look-without-a-foci-lists-the-long-per-account-formula",
        move |_| {
            pane.spell == REGENERATION_SELF_V
                && pane.component_names == long
                && pane.rows_drawn == 8
                && !pane.component_names.contains(&taper)
        },
    );
    c.shutdown();
}

#[test]
fn scenario_a_secondary_click_on_a_spell_opens_its_description() {
    scenario("a_secondary_click_on_a_spell_opens_its_description");
}

#[test]
fn scenario_the_primary_click_selects_the_row_and_examines_nothing() {
    scenario("the_primary_click_selects_the_row_and_examines_nothing");
}

#[test]
fn scenario_the_secondary_click_moves_the_books_own_selection_too() {
    scenario("the_secondary_click_moves_the_books_own_selection_too");
}

#[test]
fn scenario_an_enchantment_shows_how_long_it_lasts_and_no_range() {
    scenario("an_enchantment_shows_how_long_it_lasts_and_no_range");
}

#[test]
fn scenario_a_second_look_re_opens_the_window_the_close_control_shut() {
    scenario("a_second_look_re_opens_the_window_the_close_control_shut");
}

#[test]
fn scenario_the_same_look_from_the_cast_bar_opens_it_without_selecting() {
    scenario("the_same_look_from_the_cast_bar_opens_it_without_selecting");
}

#[test]
fn scenario_a_look_cancels_an_appraisal_in_flight_and_only_the_first_of_two_does() {
    scenario("a_look_cancels_an_appraisal_in_flight_and_only_the_first_of_two_does");
}

#[test]
fn scenario_a_look_with_nothing_in_flight_asks_the_shard_nothing() {
    scenario("a_look_with_nothing_in_flight_asks_the_shard_nothing");
}

#[test]
fn scenario_the_account_the_shard_names_is_the_one_the_client_keeps() {
    scenario("the_account_the_shard_names_is_the_one_the_client_keeps");
}

#[test]
fn scenario_the_formula_the_client_would_cast_follows_that_account() {
    scenario("the_formula_the_client_would_cast_follows_that_account");
}

#[test]
fn scenario_the_look_lists_the_tapers_this_account_must_carry() {
    scenario("the_look_lists_the_tapers_this_account_must_carry");
}

#[test]
fn scenario_two_accounts_are_shown_different_tapers_for_one_spell() {
    scenario("two_accounts_are_shown_different_tapers_for_one_spell");
}

#[test]
fn scenario_the_shipped_foci_table_reaches_the_game_model_with_the_description() {
    scenario("the_shipped_foci_table_reaches_the_game_model_with_the_description");
}

#[test]
fn scenario_carrying_a_foci_changes_what_the_client_would_spend() {
    scenario("carrying_a_foci_changes_what_the_client_would_spend");
}

#[test]
fn scenario_a_foci_for_another_school_leaves_the_long_formula_alone() {
    scenario("a_foci_for_another_school_leaves_the_long_formula_alone");
}

#[test]
fn scenario_the_look_with_a_foci_lists_a_scarab_and_four_tapers() {
    scenario("the_look_with_a_foci_lists_a_scarab_and_four_tapers");
}

#[test]
fn scenario_the_look_without_a_foci_lists_the_long_per_account_formula() {
    scenario("the_look_without_a_foci_lists_the_long_per_account_formula");
}

/// The spell-examine family's own setup: the shipped spellbook page, the secondary click, and the
/// pane's facts as one owned value an assertion closure can hold.
mod examine {
    use dereth_client::platform::keys::MouseButton;
    use dereth_client::pump::{Pump, Win32Message};
    use dereth_primitives::{DataId, ObjectId};
    use dereth_testkit::{ClientSpec, HeadlessClient, Inbound};
    use dereth_ui::{ElemHandle, ElementId, UiSystem};
    use dereth_ui_screens::panels::examination;
    use dereth_ui_screens::screens::gameplay::GamePlayScreen;
    use dereth_ui_screens::view::SpellEntry;

    pub const PRIMARY: MouseButton = MouseButton::Left;
    /// The button the whole of this family is about. `dereth_testkit::Player::Click` is the primary
    /// one only; see this section's header.
    pub const SECONDARY: MouseButton = MouseButton::Right;

    /// The school names the pane writes, in the shipped enumeration's own order, and the name
    /// anything outside it takes. They are what the player reads, so they travel with the claim
    /// rather than being read back out of the client's own table -- which is the one thing a
    /// scenario asserting through that table could not catch.
    pub const fn school_name(school: u32) -> &'static str {
        match school {
            1 => "War Magic",
            2 => "Life Magic",
            3 => "Item Enchantment",
            4 => "Creature Enchantment",
            5 => "Void Magic",
            _ => "None",
        }
    }

    /// The character these scenarios put in the world when they need one.
    const PLAYER: ObjectId = ObjectId(0x5116_0000);
    /// The foci a scenario puts in a side pack.
    const FOCI: ObjectId = ObjectId(0x5116_0001);

    /// A whole client on the shipped gameplay screen, and nothing else.
    pub fn a_client() -> HeadlessClient {
        HeadlessClient::new(ClientSpec::gameplay(4))
    }

    /// The same, with the two spells the pane's two forks need in the character's book and the
    /// spellbook page up.
    pub fn a_book_of_two() -> HeadlessClient {
        let mut c = a_client();
        learn_and_open(&mut c, &[super::FLAME_BOLT, super::STRENGTH_SELF]);
        c
    }

    /// One spell in the book, under a named account whose description has arrived.
    pub fn a_book_of_one(spell: u32, account: &str) -> HeadlessClient {
        let mut c = a_client();
        greet(&mut c, account);
        describe(&mut c);
        learn_and_open(&mut c, &[spell]);
        c
    }

    /// Put `spells` in the character's book, as its own description would have left them, and
    /// raise the spellbook's page.
    pub fn learn_and_open(c: &mut HeadlessClient, spells: &[u32]) {
        let entries: Vec<SpellEntry> = spells
            .iter()
            .enumerate()
            .map(|(i, s)| entry(c, *s, u32::try_from(i).expect("a small book") + 1))
            .collect();
        c.hud_mut().spells = entries;
        open_the_spellbook(c);
        c.tick(1);
    }

    /// One row of the book. Everything but the id is read off the shipped table, which is where
    /// the production host reads it too.
    fn entry(c: &HeadlessClient, spell: u32, order: u32) -> SpellEntry {
        let b = shipped_spell(c, spell);
        SpellEntry {
            id: spell,
            name: b.name.clone(),
            icon: Some(DataId(b.icon)),
            school: b.school,
            level: 1,
            icon_power: 1,
            display_order: i32::try_from(order).expect("a small order"),
            bitfield: 0,
        }
    }

    /// One row of the shipped spell table, which is the pane's own source.
    pub fn shipped_spell(c: &HeadlessClient, spell: u32) -> dereth_assets::tables::SpellBase {
        c.view()
            .expect_app()
            .hud()
            .spell_table
            .as_ref()
            .expect("the client loads the shipped spell table at startup")
            .spells
            .get(&spell)
            .unwrap_or_else(|| panic!("the shipped table carries spell {spell}"))
            .clone()
    }

    /// The names a formula's non-zero slots spell out, in slot order -- the lines the pane writes.
    pub fn component_names(c: &HeadlessClient, formula: &[u32]) -> Vec<String> {
        let cat = &c.view().expect_app().hud().component_catalogue;
        formula
            .iter()
            .filter(|s| **s != 0)
            .map(|s| {
                cat.inq_spell_component_base(*s)
                    .unwrap_or_else(|| panic!("the shipped component table carries slot {s}"))
                    .name
                    .clone()
            })
            .collect()
    }

    /// The range the pane writes, in yards, for a character with no skill of his own: the
    /// constant, capped, over the metres in a yard. Done here in the scenario's own terms rather
    /// than through the panel's helper.
    pub fn yards(b: &dereth_assets::tables::SpellBase) -> f64 {
        let metres = f64::from(b.base_range_constant).min(75.0);
        metres / 0.9144
    }

    /// `Login_CharacterSet` with a synthetic account name, **through the codec** -- which is the
    /// only way the client ever obtains one.
    pub fn greet(c: &mut HeadlessClient, account: &str) {
        let set = dereth_protocol::login::LoginCharacterSet {
            status: 0,
            characters: Vec::new(),
            deleted: Vec::new(),
            num_allowed_characters: 5,
            account: account.to_owned(),
            use_turbine_chat: 1,
            has_throne_of_destiny: 1,
        };
        let bytes = dereth_protocol::write_body(&set).expect("the greeting encodes");
        let back: dereth_protocol::login::LoginCharacterSet =
            dereth_protocol::read_body(&bytes).expect("and decodes");
        assert_eq!(back.account, account, "the name survives the wire");
        c.when(Inbound::event(
            dereth_client_net::client_session::SessionEvent::CharacterSet(Box::new(back)),
        ));
        c.tick(1);
    }

    /// The character's own description. An empty one is enough: what these scenarios are about is
    /// the hand-off it carries and not the qualities.
    pub fn describe(c: &mut HeadlessClient) {
        c.when(Inbound::event(
            dereth_client_net::client_session::SessionEvent::PlayerDescription(Box::default()),
        ));
        c.tick(1);
    }

    /// Put the player in the world with an inventory, so the walk that looks for a foci has a list
    /// to walk and the formula does not take its "not in the world yet" arm.
    pub fn stand_in_the_world(c: &mut HeadlessClient) -> ObjectId {
        let w = c.world_mut();
        let me = w.player.unwrap_or(PLAYER);
        if w.weenie(me).is_none() {
            let mut wn = dereth_client_model::weenie::Weenie::new(me);
            wn.pwd = dereth_protocol::types::PublicWeenieDesc {
                name: "Tester".into(),
                // The type the component walk expects of the thing whose pack it is reading.
                obj_type: 0x0000_0010,
                ..dereth_protocol::types::PublicWeenieDesc::default()
            };
            w.tables.weenies.insert(me, wn);
        }
        w.set_player(me);
        if let Some(x) = w.tables.weenies.get_mut(me) {
            if x.qualities.is_none() {
                x.qualities = Some(dereth_client_model::qualities::Qualities::default());
            }
        }
        if w.inventory(me).is_none() {
            w.tables
                .inventories
                .insert(me, dereth_client_model::objects::ObjectInventory::new(me));
        }
        me
    }

    /// The class the shipped table gives as `school`'s foci.
    pub fn foci_of(c: &HeadlessClient, school: u32) -> u32 {
        c.view()
            .expect_app()
            .hud()
            .school_pack_wcid
            .iter()
            .find(|(s, _)| *s == school)
            .map(|(_, w)| *w)
            .unwrap_or_else(|| panic!("the shipped mapper names a foci for school {school}"))
    }

    /// Some other school the shipped mapper also names a foci for.
    pub fn another_school(c: &HeadlessClient, not: u32) -> u32 {
        c.view()
            .expect_app()
            .hud()
            .school_pack_wcid
            .iter()
            .map(|(s, _)| *s)
            .find(|s| *s != not)
            .expect("the shipped mapper names more than one school")
    }

    /// A foci in the player's side-pack list, which is the only list the ownership walk reads.
    pub fn carry_the_foci(c: &mut HeadlessClient, me: ObjectId, wcid: u32) {
        let w = c.world_mut();
        let mut foci = dereth_client_model::weenie::Weenie::new(FOCI);
        foci.pwd = dereth_protocol::types::PublicWeenieDesc {
            name: "A foci".into(),
            wcid,
            container_id: Some(me),
            ..dereth_protocol::types::PublicWeenieDesc::default()
        };
        w.tables.weenies.insert(FOCI, foci);
        let mut inv = w
            .inventory(me)
            .cloned()
            .expect("the player has an inventory");
        inv.containers.push(FOCI);
        w.tables.inventories.insert(me, inv);
    }

    /// Raise the spellbook's page, as the panel-visibility notice does.
    pub fn open_the_spellbook(c: &mut HeadlessClient) {
        let (ui, screen) = parts(c);
        let root = screen.root().expect("the gameplay root");
        let page = ui
            .get_child_recursive(root, dereth_ui_screens::panels::remaining::SPELL_PAGE)
            .expect("the spell page is in the shipped layout");
        let panel_id = screen
            .panels
            .pages
            .iter()
            .find(|p| p.handle == page)
            .expect("the spell page is in the panel stack")
            .panel_id;
        screen.panels.recv_set_panel_visibility(ui, panel_id, true);
    }

    /// The shipped tree and the gameplay screen.
    pub fn parts(c: &mut HeadlessClient) -> (&mut UiSystem, &mut GamePlayScreen) {
        let shell = c.app_mut().ui_mut().expect("the UI shell");
        let ui = &mut shell.ui;
        let screen = shell.flow.current_mut().expect("a current screen");
        let any: &mut dyn std::any::Any = &mut **screen;
        let screen = any
            .downcast_mut::<GamePlayScreen>()
            .expect("the gameplay screen");
        (ui, screen)
    }

    /// One real pointer gesture at a screen point, through the client's own pump and its own input
    /// manager -- the delivery `dereth_testkit::Player::Click` makes, with the button the caller
    /// names.
    pub fn press(c: &mut HeadlessClient, x: i32, y: i32, at: u32, button: MouseButton) {
        let mut pump = Pump::new();
        pump.state.is_ready = true;
        pump.state.is_active_app = true;
        let m = pump.mouse_move_message(f64::from(x), f64::from(y), at);
        deliver(c, &mut pump, m);
        for (down, dt) in [(true, 10), (false, 20)] {
            let m = pump
                .button_message(button, down, at + dt)
                .expect("the client's own table names this button");
            deliver(c, &mut pump, m);
        }
        // Four, which is what the harness's own click step runs: two for the press and two so that
        // what it raised reaches the panels and the interaction layer.
        c.tick(4);
    }

    fn deliver(c: &mut HeadlessClient, pump: &mut Pump, m: Win32Message) {
        pump.dispatch(m);
        c.app_mut()
            .input_manager_mut()
            .expect("the real input maps")
            .on_message(m);
    }

    /// A point inside the spellbook row at `index`, in screen coordinates.
    pub fn spellbook_row_point(c: &mut HeadlessClient, index: usize, spell: u32) -> (i32, i32) {
        let handle = {
            let list = c
                .view()
                .expect_app()
                .hud()
                .panels
                .spellbook
                .list
                .as_ref()
                .expect("the shipped spellbook list bound");
            assert_eq!(
                list.slots[index].spell,
                Some(spell),
                "row {index} carries the spell this scenario is about"
            );
            list.slots[index].handle
        };
        let b = c
            .view()
            .expect_app()
            .ui()
            .expect("the UI shell")
            .ui
            .screen_box(handle);
        (b.x0 + 4, b.y0 + 4)
    }

    /// The centre of one element of the examine window.
    pub fn centre_of(c: &mut HeadlessClient, id: ElementId) -> (i32, i32) {
        let h = find(c, id);
        let b = c
            .view()
            .expect_app()
            .ui()
            .expect("the UI shell")
            .ui
            .screen_box(h);
        ((b.x0 + b.x1) / 2, (b.y0 + b.y1) / 2)
    }

    fn find(c: &mut HeadlessClient, id: ElementId) -> ElemHandle {
        let (ui, screen) = parts(c);
        let root = screen.root().expect("the gameplay root");
        let window = ui
            .get_child_recursive(root, examination::WINDOW)
            .expect("the examine window is in the shipped layout");
        ui.get_child_recursive(window, id)
            .unwrap_or_else(|| panic!("{id:?}"))
    }

    /// Whether the examine window is up.
    pub fn window_is_up(c: &mut HeadlessClient) -> bool {
        c.ui_snapshot().is_visible(examination::WINDOW)
    }

    /// The title the pane wrote, off the live tree.
    pub fn title(c: &mut HeadlessClient) -> String {
        c.ui_snapshot()
            .text_of(examination::DISPLAYED_NAME_TEXT)
            .to_owned()
    }

    /// What the examine panel is showing, as an owned value.
    #[derive(Debug, Clone)]
    pub struct PaneFacts {
        pub spell: u32,
        pub component_names: Vec<String>,
        pub component_scids: Vec<u32>,
        pub rows_drawn: u32,
        pub awaiting: Option<ObjectId>,
        pub current: Option<ObjectId>,
        pub examines_pulled: u32,
        pub cancels: u32,
        pub closed: u32,
        pub filled: u32,
        pub components_marked: u32,
        pub rows_marked_missing: usize,
        pub notices_pulled: u32,
        pub component_clicks: u32,
        pub component_selections: u32,
        pub self_selections_absorbed: u32,
        pub examine_newly_selected_item: bool,
    }

    pub fn pane_facts(c: &mut HeadlessClient) -> PaneFacts {
        let p = &parts(c).1.examination;
        PaneFacts {
            spell: p.spell.spell,
            component_names: p.spell.component_names.clone(),
            component_scids: p.spell.component_scids.clone(),
            rows_drawn: p.spell.rows_drawn,
            awaiting: p.awaiting,
            current: p.current,
            examines_pulled: p.spell_examines_pulled,
            cancels: p.appraisals_cancelled,
            closed: p.closed,
            filled: p.spell.filled,
            components_marked: p.spell.components_marked,
            rows_marked_missing: p.spell.rows_marked_missing,
            notices_pulled: p.component_notices_pulled,
            component_clicks: p.component_clicks,
            component_selections: p.component_selections,
            self_selections_absorbed: p.self_selections_absorbed,
            examine_newly_selected_item: p.examine_newly_selected_item,
        }
    }

    /// Every appraisal the client has asked for since `from`, with the id each carried.
    pub fn appraisals(c: &HeadlessClient, from: usize) -> Vec<ObjectId> {
        c.outbound()[from..]
            .iter()
            .filter_map(|r| match r {
                dereth_client_model::Request::Appraise(a) => Some(a.target),
                _ => None,
            })
            .collect()
    }

    // -----------------------------------------------------------------------------------------
    // The pack, the formula icons and the marks over them, for `spell-examine.components.*` and
    // `spell-examine.marks.*`.
    // -----------------------------------------------------------------------------------------

    /// The item type every call site of the component update gates on.
    const TYPE_SPELL_COMPONENTS: u32 = 0x0000_1000;

    /// A second object of one slot's kind, for the scenarios about owning a kind rather than a
    /// thing.
    pub const A_SECOND_OF_ITS_KIND: ObjectId = ObjectId(0x8116_0400);
    /// Something that is not a component at all, in the same pack.
    pub const A_PLAIN_ITEM: ObjectId = ObjectId(0x8116_0999);
    /// Somewhere that is not the player's pack.
    const ELSEWHERE: ObjectId = ObjectId(0x7116_0001);

    /// The component object this family puts in the pack for formula slot `slot`.
    pub fn component_object(slot: usize) -> ObjectId {
        ObjectId(0x8116_0300 + u32::try_from(slot).expect("a small formula"))
    }

    /// The formula the pane would draw for `spell`, truncated to the slots
    /// that carry a component -- the pane's own source, and **not** the shipped table's raw slots
    /// (see this section's header).
    pub fn formula_of(c: &HeadlessClient, spell: u32) -> Vec<u32> {
        let base = shipped_spell(c, spell);
        let f = c.view().world().spell_formula(&base);
        let n = dereth_client_model::magic::num_spell_components(&f);
        assert!(n > 0, "the shipped formula for spell {spell} has slots");
        f[..n].to_vec()
    }

    /// A client standing in the world with the shipped component catalogue handed over by its own
    /// description, and the formula the pane would draw for `spell`.
    fn a_client_with_a_pack(spell: u32) -> (HeadlessClient, Vec<u32>) {
        let mut c = a_client();
        describe(&mut c);
        let _ = stand_in_the_world(&mut c);
        assert!(
            !c.view().world().magic.catalogue.is_empty(),
            "the description handed the shipped component table to the game model"
        );
        let formula = formula_of(&c, spell);
        (c, formula)
    }

    /// Fill the pack with everything outside `missing`, learn `spell` and look at it.
    fn look_at(c: &mut HeadlessClient, spell: u32, formula: &[u32], missing: &[usize]) {
        for (i, scid) in formula.iter().enumerate() {
            if !missing.contains(&i) {
                carry(c, component_object(i), *scid, 10);
            }
        }
        learn_and_open(c, &[spell]);
        let (x, y) = spellbook_row_point(c, 0, spell);
        press(c, x, y, 100_000, SECONDARY);
        let pane = pane_facts(c);
        assert!(window_is_up(c), "the look opened the description");
        assert_eq!(
            pane.component_scids, formula,
            "and its rows carry the formula"
        );
        assert_eq!(pane.rows_drawn as usize, formula.len(), "one icon per slot");
    }

    /// The pane open on `spell`, with every slot outside `missing` already in the player's pack --
    /// so the marks are whatever the fill itself made them.
    pub fn a_pane_on(spell: u32, missing: &[usize]) -> (HeadlessClient, Vec<u32>) {
        let (mut c, formula) = a_client_with_a_pack(spell);
        look_at(&mut c, spell, &formula, missing);
        (c, formula)
    }

    /// The same, with `n` slots left out -- chosen as slots whose **kind** appears in that formula
    /// exactly once, because two slots of one kind stand or fall together and a scenario built on
    /// a duplicated kind would be measuring something else.
    pub fn a_pane_missing_unique(spell: u32, n: usize) -> (HeadlessClient, Vec<u32>, Vec<usize>) {
        let (mut c, formula) = a_client_with_a_pack(spell);
        let missing: Vec<usize> = (0..formula.len())
            .filter(|i| formula.iter().filter(|s| **s == formula[*i]).count() == 1)
            .take(n)
            .collect();
        assert_eq!(
            missing.len(),
            n,
            "the shipped formula has {n} slots of their own kind"
        );
        look_at(&mut c, spell, &formula, &missing);
        (c, formula, missing)
    }

    /// Put a stack of the kind slot `scid` names in the player's pack, through the client's own
    /// move seam. Nothing here adds a row to the component tracker by hand.
    pub fn carry(c: &mut HeadlessClient, id: ObjectId, scid: u32, stack: u16) {
        let (wcid, name) = {
            let cat = &c.view().world().magic.catalogue;
            let wcid = cat.scid_to_wcid(scid);
            assert_ne!(wcid, 0, "slot {scid} names a component kind");
            let name = cat
                .inq_spell_component_base(scid)
                .expect("the shipped table has a row for it")
                .name
                .clone();
            (wcid, name)
        };
        let me = c.view().world().player.expect("the player is in the world");
        let w = c.world_mut();
        put(
            w,
            id,
            dereth_protocol::types::PublicWeenieDesc {
                name,
                wcid,
                obj_type: TYPE_SPELL_COMPONENTS,
                stack_size: Some(stack),
                container_id: Some(me),
                ..dereth_protocol::types::PublicWeenieDesc::default()
            },
        );
        refresh_contents(w, me);
        w.server_says_move_item(
            id,
            me,
            0,
            ObjectId(0),
            0,
            true,
            &mut dereth_client_model::NullSink,
        );
    }

    /// The same for something that is not a component, which is the control for the notice.
    pub fn carry_a_plain_item(c: &mut HeadlessClient, id: ObjectId) {
        let me = c.view().world().player.expect("the player is in the world");
        let w = c.world_mut();
        put(
            w,
            id,
            dereth_protocol::types::PublicWeenieDesc {
                name: "A sceptre".into(),
                wcid: 999,
                obj_type: 0,
                container_id: Some(me),
                ..dereth_protocol::types::PublicWeenieDesc::default()
            },
        );
        refresh_contents(w, me);
        w.server_says_move_item(
            id,
            me,
            0,
            ObjectId(0),
            0,
            true,
            &mut dereth_client_model::NullSink,
        );
    }

    /// Move one out of the player's ownership, through the same seam.
    pub fn drop_it(c: &mut HeadlessClient, id: ObjectId) {
        let me = c.view().world().player.expect("the player is in the world");
        let w = c.world_mut();
        if w.weenie(ELSEWHERE).is_none() {
            put(
                w,
                ELSEWHERE,
                dereth_protocol::types::PublicWeenieDesc::default(),
            );
        }
        if let Some(x) = w.tables.weenies.get_mut(id) {
            x.pwd.container_id = Some(ELSEWHERE);
        }
        refresh_contents(w, me);
        w.server_says_move_item(
            id,
            ELSEWHERE,
            0,
            ObjectId(0),
            0,
            true,
            &mut dereth_client_model::NullSink,
        );
    }

    fn put(
        w: &mut dereth_client_model::World,
        id: ObjectId,
        pwd: dereth_protocol::types::PublicWeenieDesc,
    ) {
        let mut wn = dereth_client_model::weenie::Weenie::new(id);
        wn.pwd = pwd;
        w.tables.weenies.insert(id, wn);
    }

    fn refresh_contents(w: &mut dereth_client_model::World, container: ObjectId) {
        let held: Vec<dereth_protocol::types::ContentProfile> = w
            .tables
            .weenies
            .iter()
            .filter(|(_, x)| x.pwd.container_id == Some(container))
            .map(|(k, x)| dereth_protocol::types::ContentProfile {
                iid: k,
                container_properties: u32::from(x.is_container()),
            })
            .collect();
        w.view_object_contents(container, &held, &mut dereth_client_model::NullSink);
    }

    /// The slot number each drawn row is stamped with, read off the **live** rows rather than off
    /// the panel's own list -- because the stamp is what the click's lookup is handed.
    pub fn row_scids(c: &mut HeadlessClient, rows: usize) -> Vec<u32> {
        let handles: Vec<ElemHandle> = {
            let screen = parts(c).1;
            (0..rows)
                .map(|i| {
                    screen
                        .examination
                        .spell
                        .component_row(i)
                        .unwrap_or_else(|| panic!("row {i} is drawn"))
                })
                .collect()
        };
        let ui = &c.view().expect_app().ui().expect("the UI shell").ui;
        handles
            .iter()
            .enumerate()
            .map(|(i, h)| {
                dereth_ui_screens::panels::spell_examine::row_component_scid(ui, *h)
                    .unwrap_or_else(|| panic!("row {i} is stamped with its own slot"))
            })
            .collect()
    }

    /// The centre of the formula icon at `index`.
    pub fn component_icon_point(c: &mut HeadlessClient, index: usize) -> (i32, i32) {
        let row = parts(c)
            .1
            .examination
            .spell
            .component_row(index)
            .unwrap_or_else(|| panic!("row {index} is drawn"));
        let b = c
            .view()
            .expect_app()
            .ui()
            .expect("the UI shell")
            .ui
            .screen_box(row);
        assert!(
            b.width() > 0 && b.height() > 0,
            "the icon has an extent to press on"
        );
        (b.x0 + b.width() / 2, b.y0 + b.height() / 2)
    }

    /// Which formula slots are showing the mark the shipped layout draws over every icon.
    pub fn marked(c: &mut HeadlessClient, slots: usize) -> Vec<usize> {
        let marks: Vec<Option<ElemHandle>> = {
            let (ui, screen) = parts(c);
            (0..slots)
                .map(|i| screen.examination.spell.component_mark(ui, i))
                .collect()
        };
        let ui = &c.view().expect_app().ui().expect("the UI shell").ui;
        marks
            .iter()
            .enumerate()
            .filter_map(|(i, m)| {
                let h = m.unwrap_or_else(|| panic!("row {i} carries a mark child"));
                ui.node(h)
                    .expect("a live mark node")
                    .region
                    .flags
                    .visible
                    .then_some(i)
            })
            .collect()
    }
}

// ---------------------------------------------------------------------------------------------
// spell-examine.components.* and spell-examine.marks.*
//
// Clicking a component icon in the spell-examine pane, and the marks over the components the
// player lacks.
//
// **The formula is read off the pane's own source and never off the raw shipped table.** This
// client resolves a customised taper slot to the lowest taper rather than to the account hash --
// a known deviation from retail -- so a scenario keyed on the table's slots would be marking a
// different component than the one the pane drew. The scenarios query the same resolved formula
// as the pane, so they stay correct when the deviation is fixed.
//
// **The pack is filled through the client's own move seam**; nothing here adds a row to the
// component tracker by hand, and the shipped component table reaches the game model through the
// character's own description rather than being assigned behind the client's back.
//
// Two readings of the shipped layout -- that the formula list declares the
// attribute the gesture hangs on, and that the row template ships one mark child authored visible
// -- stay behind the evidence handle: they are readings of the shipped layout and not behaviours,
// and what each of them justified is asserted below by driving the gesture.
// ---------------------------------------------------------------------------------------------

/// `Acid Stream V` -- War Magic, an eight-slot formula, every slot with an icon.
const ACID_STREAM_V: u32 = 62;

pub fn a_click_on_a_component_icon_selects_the_one_in_the_pack() {
    use dereth_ui_screens::panels::examination;

    let (mut c, formula) = examine::a_pane_on(FLAME_BOLT, &[]);
    let name = examine::shipped_spell(&c, FLAME_BOLT).name;
    let before = c.view().world().selected;

    // Each row really does carry the slot it stands for: the premise the click's own lookup is
    // handed, read off the live rows rather than off the panel's mirror.
    let stamped = examine::row_scids(&mut c, formula.len());

    let (x, y) = examine::component_icon_point(&mut c, 2);
    examine::press(&mut c, x, y, 200_000, examine::PRIMARY);

    let pane = examine::pane_facts(&mut c);
    let selected = c.view().world().selected;
    let up = c.ui_snapshot().is_visible(examination::WINDOW);
    let title = examine::title(&mut c);
    let want = examine::component_object(2);

    c.assert_behaviour(
        "spell-examine.components.a-click-on-an-icon-selects-the-one-in-the-pack",
        move |_| {
            before.is_none()
                && stamped == formula
                && pane.component_clicks == 1
                && pane.component_selections == 1
                && pane.self_selections_absorbed == 1
                && pane.examine_newly_selected_item
                && selected == Some(want)
                && up
                && title == name
        },
    );
    c.shutdown();
}

pub fn each_component_icon_names_its_own_slot() {
    let (mut c, formula) = examine::a_pane_on(FLAME_BOLT, &[]);
    let mut picked = Vec::new();
    for i in 0..formula.len() {
        let (x, y) = examine::component_icon_point(&mut c, i);
        let at = 300_000 + u32::try_from(i).expect("a small formula") * 1_000;
        examine::press(&mut c, x, y, at, examine::PRIMARY);
        picked.push(c.view().world().selected);
    }
    let want: Vec<Option<dereth_primitives::ObjectId>> = (0..formula.len())
        .map(|i| Some(examine::component_object(i)))
        .collect();
    let selections = examine::pane_facts(&mut c).component_selections as usize;

    c.assert_behaviour(
        "spell-examine.components.each-icon-names-its-own-slot",
        move |_| picked == want && selections == want.len(),
    );
    c.shutdown();
}

pub fn a_component_the_player_does_not_carry_selects_nothing() {
    use dereth_ui_screens::panels::examination;

    let (mut c, formula) = examine::a_pane_on(FLAME_BOLT, &[3]);
    let drawn = examine::pane_facts(&mut c).rows_drawn as usize;

    let (x, y) = examine::component_icon_point(&mut c, 3);
    examine::press(&mut c, x, y, 400_000, examine::PRIMARY);
    let refused = examine::pane_facts(&mut c);
    let nothing = c.view().world().selected;
    let up = c.ui_snapshot().is_visible(examination::WINDOW);

    // The control, in the same run: the slot beside it, which the player does carry.
    let (x, y) = examine::component_icon_point(&mut c, 4);
    examine::press(&mut c, x, y, 410_000, examine::PRIMARY);
    let selected = c.view().world().selected;
    let want = examine::component_object(4);

    c.assert_behaviour(
        "spell-examine.components.one-the-player-does-not-carry-selects-nothing",
        move |_| {
            drawn == formula.len()
                && refused.component_clicks == 1
                && refused.component_selections == 0
                && refused.self_selections_absorbed == 0
                && refused.examine_newly_selected_item
                && nothing.is_none()
                && up
                && selected == Some(want)
        },
    );
    c.shutdown();
}

pub fn the_secondary_click_does_nothing_on_a_component_icon() {
    let (mut c, _) = examine::a_pane_on(FLAME_BOLT, &[]);
    let name = examine::shipped_spell(&c, FLAME_BOLT).name;

    let (x, y) = examine::component_icon_point(&mut c, 1);
    examine::press(&mut c, x, y, 500_000, examine::SECONDARY);
    let after_right = (
        examine::pane_facts(&mut c).component_clicks,
        c.view().world().selected,
        examine::title(&mut c),
    );

    examine::press(&mut c, x, y, 510_000, examine::PRIMARY);
    let after_left = c.view().world().selected;
    let want = examine::component_object(1);

    c.assert_behaviour(
        "spell-examine.components.the-secondary-click-does-nothing-on-an-icon",
        move |_| {
            after_right.0 == 0
                && after_right.1.is_none()
                && after_right.2 == name
                && after_left == Some(want)
        },
    );
    c.shutdown();
}

pub fn the_component_a_slot_stands_for_is_a_representative_of_its_kind() {
    let (mut c, formula) = examine::a_pane_on(FLAME_BOLT, &[]);
    let scid = formula[0];
    examine::carry(&mut c, examine::A_SECOND_OF_ITS_KIND, scid, 10);

    let first = c
        .view()
        .world()
        .component_object_id(scid)
        .expect("the kind is carried");
    let again = c.view().world().component_object_id(scid);
    let one_of_the_two =
        first == examine::component_object(0) || first == examine::A_SECOND_OF_ITS_KIND;

    // Take the answered one away; the kind is still owned, so the slot still answers.
    examine::drop_it(&mut c, first);
    let rest = c
        .view()
        .world()
        .component_object_id(scid)
        .expect("the other one is still here");

    // A slot number the shipped table does not know, and one whose kind is not carried.
    let unknown = c.view().world().component_object_id(0);
    let outside = c.view().world().component_object_id(0x00FF_FFFF);

    c.assert_behaviour(
        "spell-examine.components.the-answer-is-a-representative-of-the-class-and-not-one-object",
        move |_| {
            one_of_the_two
                && again == Some(first)
                && rest != first
                && unknown.is_none()
                && outside.is_none()
        },
    );
    c.shutdown();
}

pub fn the_components_the_player_lacks_are_the_ones_that_are_marked() {
    let (mut c, formula, missing) = examine::a_pane_missing_unique(ACID_STREAM_V, 2);
    let slots = formula.len();
    let marked = examine::marked(&mut c, slots);
    let pane = examine::pane_facts(&mut c);

    assert_eq!(slots, 8, "the eight-slot formula this scenario is about");

    c.assert_behaviour(
        "spell-examine.marks.the-components-the-player-lacks-are-the-ones-that-are-marked",
        move |_| {
            marked == missing
                && pane.rows_marked_missing == missing.len()
                // The fill marks the rows itself: the moves that filled the pack each ran the
                // marking pass over a list that did not exist yet, so the run that marked these
                // eight rows is one more than the notices.
                && pane.components_marked > pane.notices_pulled
        },
    );
    c.shutdown();
}

pub fn everything_carried_marks_nothing_and_nothing_carried_marks_everything() {
    let (mut c, formula) = examine::a_pane_on(ACID_STREAM_V, &[]);
    let none_marked = examine::marked(&mut c, formula.len());
    let none_counted = examine::pane_facts(&mut c).rows_marked_missing;
    c.shutdown();

    let all: Vec<usize> = (0..formula.len()).collect();
    let (mut c, formula) = examine::a_pane_on(ACID_STREAM_V, &all);
    let slots = formula.len();
    let every = examine::marked(&mut c, slots);
    let every_counted = examine::pane_facts(&mut c).rows_marked_missing;

    c.assert_behaviour(
        "spell-examine.marks.everything-carried-marks-nothing-and-nothing-carried-marks-everything",
        move |_| {
            none_marked.is_empty() && none_counted == 0 && every == all && every_counted == slots
        },
    );
    c.shutdown();
}

pub fn a_component_arriving_clears_its_mark_and_one_leaving_brings_it_back() {
    let (mut c, formula, missing) = examine::a_pane_missing_unique(ACID_STREAM_V, 2);
    let slots = formula.len();
    let at_first = examine::marked(&mut c, slots);
    let notices_before = examine::pane_facts(&mut c).notices_pulled;

    let slot = missing[0];
    examine::carry(&mut c, examine::component_object(slot), formula[slot], 10);
    c.tick(1);
    let after_arrival = examine::marked(&mut c, slots);
    let live = examine::pane_facts(&mut c);

    examine::drop_it(&mut c, examine::component_object(slot));
    c.tick(1);
    let after_departure = examine::marked(&mut c, slots);
    let still_missing = vec![missing[1]];

    c.assert_behaviour(
        "spell-examine.marks.a-component-arriving-clears-its-mark-and-one-leaving-brings-it-back",
        move |_| {
            at_first == missing
                && after_arrival == still_missing
                && live.notices_pulled > notices_before
                && live.filled == 1
                && after_departure == missing
        },
    );
    c.shutdown();
}

pub fn a_closed_window_still_re_marks_its_rows() {
    use dereth_ui_screens::panels::examination;

    let (mut c, formula, missing) = examine::a_pane_missing_unique(ACID_STREAM_V, 2);
    let slots = formula.len();

    let closed = {
        let (ui, screen) = examine::parts(&mut c);
        screen.examination.close_from_action(ui)
    };
    c.tick(1);
    let shut = !c.ui_snapshot().is_visible(examination::WINDOW);

    let slot = missing[1];
    examine::carry(&mut c, examine::component_object(slot), formula[slot], 10);
    c.tick(1);
    let after = examine::marked(&mut c, slots);
    let want = vec![missing[0]];

    c.assert_behaviour(
        "spell-examine.marks.a-closed-window-still-re-marks-its-rows",
        move |_| closed && shut && after == want,
    );
    c.shutdown();
}

pub fn a_second_of_the_same_kind_keeps_the_mark_off() {
    let (mut c, formula, missing) = examine::a_pane_missing_unique(ACID_STREAM_V, 2);
    let slots = formula.len();
    let slot = (0..slots)
        .find(|i| !missing.contains(i))
        .expect("a slot the player carries");

    let plain_at_first = !examine::marked(&mut c, slots).contains(&slot);

    examine::carry(&mut c, examine::A_SECOND_OF_ITS_KIND, formula[slot], 10);
    c.tick(1);
    let two_of_them = !examine::marked(&mut c, slots).contains(&slot);

    examine::drop_it(&mut c, examine::component_object(slot));
    c.tick(1);
    let one_left = !examine::marked(&mut c, slots).contains(&slot);
    let still_owned = c.view().world().spell_component_is_owned(formula[slot]);

    examine::drop_it(&mut c, examine::A_SECOND_OF_ITS_KIND);
    c.tick(1);
    let none_left = examine::marked(&mut c, slots).contains(&slot);
    let owned_now = c.view().world().spell_component_is_owned(formula[slot]);

    c.assert_behaviour(
        "spell-examine.marks.a-second-of-the-same-kind-keeps-the-mark-off",
        move |_| {
            plain_at_first && two_of_them && one_left && still_owned && none_left && !owned_now
        },
    );
    c.shutdown();
}

pub fn the_notice_moves_for_a_component_and_for_nothing_else() {
    let (mut c, formula, _) = examine::a_pane_missing_unique(ACID_STREAM_V, 2);

    let before = c.view().world().magic.component_serial;
    examine::carry(&mut c, examine::A_SECOND_OF_ITS_KIND, formula[0], 10);
    let after_component = c.view().world().magic.component_serial;

    // An ordinary item in the same pack: it is not a component, so it raises nothing.
    examine::carry_a_plain_item(&mut c, examine::A_PLAIN_ITEM);
    let after_item = c.view().world().magic.component_serial;

    c.tick(2);
    let after_idle_frames = c.view().world().magic.component_serial;

    c.assert_behaviour(
        "spell-examine.marks.the-notice-moves-for-a-component-and-for-nothing-else",
        move |_| {
            after_component > before
                && after_item == after_component
                && after_idle_frames == after_component
        },
    );
    c.shutdown();
}

#[test]
fn scenario_a_click_on_a_component_icon_selects_the_one_in_the_pack() {
    scenario("a_click_on_a_component_icon_selects_the_one_in_the_pack");
}

#[test]
fn scenario_each_component_icon_names_its_own_slot() {
    scenario("each_component_icon_names_its_own_slot");
}

#[test]
fn scenario_a_component_the_player_does_not_carry_selects_nothing() {
    scenario("a_component_the_player_does_not_carry_selects_nothing");
}

#[test]
fn scenario_the_secondary_click_does_nothing_on_a_component_icon() {
    scenario("the_secondary_click_does_nothing_on_a_component_icon");
}

#[test]
fn scenario_the_component_a_slot_stands_for_is_a_representative_of_its_kind() {
    scenario("the_component_a_slot_stands_for_is_a_representative_of_its_kind");
}

#[test]
fn scenario_the_components_the_player_lacks_are_the_ones_that_are_marked() {
    scenario("the_components_the_player_lacks_are_the_ones_that_are_marked");
}

#[test]
fn scenario_everything_carried_marks_nothing_and_nothing_carried_marks_everything() {
    scenario("everything_carried_marks_nothing_and_nothing_carried_marks_everything");
}

#[test]
fn scenario_a_component_arriving_clears_its_mark_and_one_leaving_brings_it_back() {
    scenario("a_component_arriving_clears_its_mark_and_one_leaving_brings_it_back");
}

#[test]
fn scenario_a_closed_window_still_re_marks_its_rows() {
    scenario("a_closed_window_still_re_marks_its_rows");
}

#[test]
fn scenario_a_second_of_the_same_kind_keeps_the_mark_off() {
    scenario("a_second_of_the_same_kind_keeps_the_mark_off");
}

#[test]
fn scenario_the_notice_moves_for_a_component_and_for_nothing_else() {
    scenario("the_notice_moves_for_a_component_and_for_nothing_else");
}

// ---------------------------------------------------------------------------------------------
// spell-components.strip.* -- the components page of the spell book
//
// The page is the real one bound off the shipped tree and driven by the client's own per-frame
// pass; components reach the pack through the normal server-directed move path and never by a
// hand call to the tracker.
//
// A reading of the shipped layout -- that the page's list box carries a header template first
// and a row template second, and that the row template carries the three column elements -- stays
// behind the evidence handle. What it justified is asserted by driving the page.
// ---------------------------------------------------------------------------------------------

/// A picture no shipped component has, put on the component **objects** so that "the row drew the
/// object's picture" and "the row drew the table's" cannot be the same observation.
const A_WRONG_PICTURE: u32 = 0x0600_DEAD;

pub fn a_component_row_shows_how_many_are_held_and_goes_away_at_none() {
    let (mut c, kinds) = strip::a_page_and_five_kinds();
    let (scid, wcid) = (kinds[0].0, kinds[0].1);
    let a = dereth_primitives::ObjectId(0x8116_0501);
    let b = dereth_primitives::ObjectId(0x8116_0502);

    examine::carry(&mut c, a, scid, 7);
    c.tick(1);
    let seven = (strip::owned_text(&mut c, wcid), strip::rows(&c).len());

    // A second pile of the same kind is the same row, with the sum.
    examine::carry(&mut c, b, scid, 4);
    c.tick(1);
    let eleven = (strip::owned_text(&mut c, wcid), strip::rows(&c).len());

    examine::drop_it(&mut c, b);
    c.tick(1);
    let back_to_seven = strip::owned_text(&mut c, wcid);

    examine::drop_it(&mut c, a);
    c.tick(1);
    let gone = (strip::owned_text(&mut c, wcid), strip::rows(&c).is_empty());

    c.assert_behaviour(
        "spell-components.strip.a-row-shows-how-many-are-held-and-goes-away-at-none",
        move |_| {
            seven == (Some("7".to_owned()), 1)
                && eleven == (Some("11".to_owned()), 1)
                && back_to_seven.as_deref() == Some("7")
                && gone == (None, true)
        },
    );
    c.shutdown();
}

pub fn the_component_page_rebuilds_on_the_pack_and_not_every_frame() {
    let (mut c, kinds) = strip::a_page_and_five_kinds();
    let scid = kinds[1].0;

    examine::carry(&mut c, dereth_primitives::ObjectId(0x8116_0601), scid, 3);
    c.tick(1);
    let after_add = strip::rebuilds(&c);

    c.tick(3);
    let after_idle = strip::rebuilds(&c);

    examine::carry(&mut c, dereth_primitives::ObjectId(0x8116_0602), scid, 1);
    c.tick(1);
    let after_second = strip::rebuilds(&c);
    let four = strip::owned_text(&mut c, kinds[1].1);

    c.assert_behaviour(
        "spell-components.strip.it-rebuilds-when-the-pack-changes-and-not-every-frame",
        move |_| {
            after_add > 0
                && after_idle == after_add
                && after_second > after_add
                && four.as_deref() == Some("4")
        },
    );
    c.shutdown();
}

pub fn a_header_is_drawn_only_for_a_kind_the_player_holds_something_of() {
    let (mut c, kinds) = strip::a_page_and_five_kinds();
    c.tick(1);
    let empty = (strip::headers(&c), strip::rows(&c).len());

    let first = dereth_primitives::ObjectId(0x8116_0701);
    examine::carry(&mut c, first, kinds[0].0, 1);
    c.tick(1);
    let one = strip::headers(&c);

    // A second kind, in a different part of the order.
    let second = kinds
        .iter()
        .find(|k| k.3 != kinds[0].3)
        .expect("two kinds in two places")
        .clone();
    examine::carry(
        &mut c,
        dereth_primitives::ObjectId(0x8116_0702),
        second.0,
        1,
    );
    c.tick(1);
    let two = strip::headers(&c);
    let mut want_two = vec![kinds[0].3, second.3];
    want_two.sort_unstable();

    examine::drop_it(&mut c, first);
    c.tick(1);
    let back_to_one = strip::headers(&c);
    let want_first = vec![kinds[0].3];
    let want_second = vec![second.3];

    c.assert_behaviour(
        "spell-components.strip.a-header-is-drawn-only-for-a-kind-the-player-holds-something-of",
        move |_| {
            empty == (Vec::new(), 0)
                && one == want_first
                && two == want_two
                && back_to_one == want_second
        },
    );
    c.shutdown();
}

pub fn the_component_walk_is_kind_order_with_each_kinds_rows_under_its_header() {
    let (mut c, kinds) = strip::a_page_and_five_kinds();
    for (i, k) in kinds.iter().enumerate() {
        let id =
            dereth_primitives::ObjectId(0x8116_0800 + u32::try_from(i).expect("a small formula"));
        examine::carry(&mut c, id, k.0, 1);
    }
    c.tick(1);

    let mut want_headers: Vec<u32> = kinds.iter().map(|k| k.3).collect();
    want_headers.sort_unstable();
    want_headers.dedup();
    let headers = strip::headers(&c);
    let drawn: Vec<u32> = strip::rows(&c).iter().map(|(w, _)| *w).collect();

    let mut want_rows: Vec<u32> = kinds.iter().map(|k| k.1).collect();
    want_rows.sort_by_key(|w| {
        kinds
            .iter()
            .find(|k| k.1 == *w)
            .map(|k| k.3)
            .unwrap_or(u32::MAX)
    });
    let kinds_len = kinds.len();

    c.assert_behaviour(
        "spell-components.strip.the-walk-is-kind-order-with-each-kinds-rows-under-its-header",
        move |_| headers == want_headers && drawn.len() == kinds_len && drawn == want_rows,
    );
    c.shutdown();
}

pub fn the_component_row_icon_is_the_shipped_tables_and_not_the_objects() {
    let (mut c, kinds) = strip::a_page_and_five_kinds();
    let (scid, wcid, name, _) = kinds[0].clone();
    strip::carry_with_a_wrong_picture(&mut c, dereth_primitives::ObjectId(0x8116_0901), scid, 2);
    c.tick(1);

    let want = c
        .view()
        .world()
        .magic
        .catalogue
        .component_icon(wcid)
        .expect("the shipped table has a picture for this kind");
    assert_ne!(
        want, A_WRONG_PICTURE,
        "the premise: the two pictures differ"
    );

    let row = strip::rows(&c)
        .into_iter()
        .find(|(w, _)| *w == wcid)
        .expect("a row")
        .1;
    let drawn = strip::row_image(
        &c,
        row,
        dereth_ui_screens::panels::spellcomponent::row::ICON,
    );
    let drawn_name = strip::child_text(
        &mut c,
        row,
        dereth_ui_screens::panels::spellcomponent::row::NAME,
    );

    c.assert_behaviour(
        "spell-components.strip.the-icon-is-the-shipped-tables-and-not-the-objects",
        move |_| {
            drawn == Some(dereth_primitives::DataId(want))
                && drawn_name.as_deref() == Some(name.as_str())
        },
    );
    c.shutdown();
}

// ---------------------------------------------------------------------------------------------
// spellbook.redraw.*
//
// The panel is bound off the real gameplay screen's spell page, as its own startup binds it, and
// driven with a book the scenario writes, because the production host derives every field but the
// id from the shipped table and cannot produce the frame these claims are about. The dat-free half
// of the same guard is tested in `dereth-ui-screens` itself.
// ---------------------------------------------------------------------------------------------

pub fn an_identical_spellbook_frame_does_not_rebuild_and_a_changed_list_does() {
    let mut c = examine::a_client();
    let mut p = book::bound_panel(&mut c);
    let before = book::a_book();

    let filled = book::update(&mut c, &mut p, &before);
    let order = book::slot_order(&p);
    let idle = book::update(&mut c, &mut p, &before);

    let mut moved = book::a_book();
    moved.0.remove(1);
    let rebuilt = book::update(&mut c, &mut p, &moved);
    let shorter = book::slot_order(&p);
    let idle_again = book::update(&mut c, &mut p, &moved);

    let want = book::ids(&before);
    let want_shorter = book::ids(&moved);

    c.assert_behaviour(
        "spellbook.redraw.an-identical-frame-does-not-rebuild-and-a-changed-spell-list-does",
        move |_| {
            filled && order == want && !idle && rebuilt && shorter == want_shorter && !idle_again
        },
    );
    c.shutdown();
}

pub fn a_spellbook_badge_follows_a_spells_own_flags() {
    let mut c = examine::a_client();
    let mut p = book::bound_panel(&mut c);
    let before = book::a_book();
    let subject = before.0[1].id;

    assert!(
        book::update(&mut c, &mut p, &before),
        "the first pass always fills"
    );
    assert!(
        !book::update(&mut c, &mut p, &before),
        "and the gate is live"
    );
    let plain = book::badge(&c, &p, subject);
    let order_before = book::slot_order(&p);

    let mut after = book::a_book();
    after.0[1].bitfield = book::FELLOWSHIP;
    assert_eq!(
        book::ids(&before),
        book::ids(&after),
        "the spell list is identical across this"
    );

    let rebuilt = book::update(&mut c, &mut p, &after);
    let lit = book::badge(&c, &p, subject);
    let order_after = book::slot_order(&p);
    let closed = !book::update(&mut c, &mut p, &after);

    // And back, which is the other direction and also the control.
    let back = book::update(&mut c, &mut p, &before);
    let plain_again = book::badge(&c, &p, subject);
    let closed_again = !book::update(&mut c, &mut p, &before);

    c.assert_behaviour(
        "spellbook.redraw.a-badge-follows-a-spells-own-flags-under-an-unchanged-list",
        move |_| {
            plain.is_none()
                && rebuilt
                && lit.is_some()
                && order_after == order_before
                && closed
                && back
                && plain_again.is_none()
                && closed_again
        },
    );
    c.shutdown();
}

pub fn the_spellbook_rows_re_sort_when_a_spells_place_in_the_order_moves() {
    let mut c = examine::a_client();
    let mut p = book::bound_panel(&mut c);
    let before = book::a_book();

    assert!(
        book::update(&mut c, &mut p, &before),
        "the first pass always fills"
    );
    assert!(
        !book::update(&mut c, &mut p, &before),
        "and the gate is live"
    );
    let order_before = book::slot_order(&p);

    let mut after = book::a_book();
    after.0[0].display_order = 40;
    assert_eq!(
        before
            .0
            .iter()
            .map(|s| (s.id, s.level, s.school, s.bitfield))
            .collect::<Vec<_>>(),
        after
            .0
            .iter()
            .map(|s| (s.id, s.level, s.school, s.bitfield))
            .collect::<Vec<_>>(),
        "only the player's own order moves"
    );

    let rebuilt = book::update(&mut c, &mut p, &after);
    let order_after = book::slot_order(&p);
    let closed = !book::update(&mut c, &mut p, &after);

    let back = book::update(&mut c, &mut p, &before);
    let order_back = book::slot_order(&p);
    let closed_again = !book::update(&mut c, &mut p, &before);

    let want_after = {
        let mut rows = after.0.clone();
        rows.sort_by_key(|s| s.display_order);
        rows.iter().map(|s| s.id).collect::<Vec<u32>>()
    };

    c.assert_behaviour(
        "spellbook.redraw.the-rows-re-sort-when-a-spells-place-in-the-order-moves",
        move |_| {
            rebuilt
                && order_after == want_after
                && order_after != order_before
                && closed
                && back
                && order_back == order_before
                && closed_again
        },
    );
    c.shutdown();
}

#[test]
fn scenario_a_component_row_shows_how_many_are_held_and_goes_away_at_none() {
    scenario("a_component_row_shows_how_many_are_held_and_goes_away_at_none");
}

#[test]
fn scenario_the_component_page_rebuilds_on_the_pack_and_not_every_frame() {
    scenario("the_component_page_rebuilds_on_the_pack_and_not_every_frame");
}

#[test]
fn scenario_a_header_is_drawn_only_for_a_kind_the_player_holds_something_of() {
    scenario("a_header_is_drawn_only_for_a_kind_the_player_holds_something_of");
}

#[test]
fn scenario_the_component_walk_is_kind_order_with_each_kinds_rows_under_its_header() {
    scenario("the_component_walk_is_kind_order_with_each_kinds_rows_under_its_header");
}

#[test]
fn scenario_the_component_row_icon_is_the_shipped_tables_and_not_the_objects() {
    scenario("the_component_row_icon_is_the_shipped_tables_and_not_the_objects");
}

#[test]
fn scenario_an_identical_spellbook_frame_does_not_rebuild_and_a_changed_list_does() {
    scenario("an_identical_spellbook_frame_does_not_rebuild_and_a_changed_list_does");
}

#[test]
fn scenario_a_spellbook_badge_follows_a_spells_own_flags() {
    scenario("a_spellbook_badge_follows_a_spells_own_flags");
}

#[test]
fn scenario_the_spellbook_rows_re_sort_when_a_spells_place_in_the_order_moves() {
    scenario("the_spellbook_rows_re_sort_when_a_spells_place_in_the_order_moves");
}

/// The components page of the spell book, and what it drew.
mod strip {
    use dereth_primitives::{DataId, ObjectId};
    use dereth_testkit::HeadlessClient;
    use dereth_ui::{ElemHandle, ElementId};
    use dereth_ui_screens::panels::spellcomponent;

    use super::examine;

    /// A client standing in the world with the spell page up and the components page bound, and
    /// the five kinds one shipped formula's slots name: `(slot, kind, name, where it sorts)`.
    pub fn a_page_and_five_kinds() -> (HeadlessClient, Vec<(u32, u32, String, u32)>) {
        let mut c = examine::a_client();
        examine::describe(&mut c);
        let _ = examine::stand_in_the_world(&mut c);
        let formula = examine::formula_of(&c, super::FLAME_BOLT);
        let kinds: Vec<(u32, u32, String, u32)> = {
            let cat = &c.view().world().magic.catalogue;
            formula
                .iter()
                .map(|scid| {
                    let b = cat
                        .inq_spell_component_base(*scid)
                        .expect("the shipped table has a row for every slot");
                    (*scid, cat.scid_to_wcid(*scid), b.name.clone(), b.category)
                })
                .collect()
        };
        assert_eq!(kinds.len(), 5, "the five slots of the bolt's formula");
        assert!(kinds.iter().all(|k| k.1 != 0), "each names a real kind");
        examine::open_the_spellbook(&mut c);
        c.tick(1);
        assert!(
            panel(&c).bound(),
            "the components page bound off the shipped tree"
        );
        assert_eq!(
            panel(&c).templates(),
            2,
            "with its header template and its row template"
        );
        (c, kinds)
    }

    pub fn panel(c: &HeadlessClient) -> &spellcomponent::SpellComponentPanel {
        &c.view().expect_app().hud().panels.spell_components
    }

    /// `(kind, element)` of every drawn row, in list order.
    pub fn rows(c: &HeadlessClient) -> Vec<(u32, ElemHandle)> {
        panel(c).rows.iter().map(|r| (r.wcid, r.element)).collect()
    }

    /// The kinds the drawn headings stand for, in list order.
    pub fn headers(c: &HeadlessClient) -> Vec<u32> {
        panel(c).headers.iter().map(|(k, _)| *k).collect()
    }

    pub fn rebuilds(c: &HeadlessClient) -> u32 {
        panel(c).rebuilds
    }

    /// The text of one column of a drawn row.
    pub fn child_text(c: &mut HeadlessClient, row: ElemHandle, child: u32) -> Option<String> {
        let ui = &mut c.app_mut().ui_mut().expect("the UI shell").ui;
        let h = ui.get_child_recursive(row, ElementId(child))?;
        ui.text_element_mut(h).map(|t| t.glyphs.inq_text(false))
    }

    /// The picture one column of a drawn row is showing.
    pub fn row_image(c: &HeadlessClient, row: ElemHandle, child: u32) -> Option<DataId> {
        let ui = &c.view().expect_app().ui().expect("the UI shell").ui;
        let h = ui.get_child_recursive(row, ElementId(child))?;
        ui.node(h)?.region.image.as_ref().map(|g| g.did)
    }

    /// The held-count column of the row for `wcid`.
    pub fn owned_text(c: &mut HeadlessClient, wcid: u32) -> Option<String> {
        let row = rows(c).into_iter().find(|(w, _)| *w == wcid)?.1;
        child_text(c, row, spellcomponent::row::OWNED)
    }

    /// A stack whose **own** picture is one no shipped component has, so the drawn picture has to
    /// have come from the table.
    pub fn carry_with_a_wrong_picture(c: &mut HeadlessClient, id: ObjectId, scid: u32, stack: u16) {
        examine::carry(c, id, scid, stack);
        let w = c.world_mut();
        if let Some(x) = w.tables.weenies.get_mut(id) {
            x.pwd.icon_id = super::A_WRONG_PICTURE;
        }
    }
}

/// The spellbook panel, bound off the shipped tree and driven with a book of its own.
mod book {
    use dereth_primitives::DataId;
    use dereth_testkit::HeadlessClient;
    use dereth_ui_screens::panels::remaining::SPELL_PAGE;
    use dereth_ui_screens::panels::spellbook::{SpellbookPanel, DEFAULT_SPELL_FILTERS};
    use dereth_ui_screens::view::{GameView, SpellEntry};

    use super::examine;

    const CREATURE: u32 = 4;
    const LIFE: u32 = 2;
    /// The flag whose badge the shipped overlay table draws over a spell's picture.
    pub const FELLOWSHIP: u32 = 0x2000;

    /// A book the scenario writes. See this section's header for why it is not the production
    /// host's.
    #[derive(Debug)]
    pub struct Book(pub Vec<SpellEntry>);

    impl GameView for Book {
        fn spellbook(&self) -> &[SpellEntry] {
            &self.0
        }
        fn spell_filters(&self) -> u32 {
            DEFAULT_SPELL_FILTERS
        }
    }

    fn entry(id: u32, name: &str, school: u32, level: u32, display_order: i32) -> SpellEntry {
        SpellEntry {
            id,
            name: name.to_owned(),
            icon: Some(DataId(0x0600_1000 + id)),
            school,
            level,
            icon_power: level,
            display_order,
            bitfield: 0,
        }
    }

    pub fn a_book() -> Book {
        Book(vec![
            entry(157, "Strength Self I", CREATURE, 1, 10),
            entry(1074, "Heal Self I", LIFE, 1, 20),
            entry(158, "Strength Self II", CREATURE, 2, 30),
        ])
    }

    pub fn ids(b: &Book) -> Vec<u32> {
        let mut rows = b.0.clone();
        rows.sort_by_key(|s| s.display_order);
        rows.iter().map(|s| s.id).collect()
    }

    /// The panel's own startup against the shipped tree: the spell page off the live gameplay
    /// root, then its list.
    pub fn bound_panel(c: &mut HeadlessClient) -> SpellbookPanel {
        let (ui, screen) = examine::parts(c);
        let root = screen.root().expect("the gameplay screen's root");
        let page = ui
            .get_child_recursive(root, SPELL_PAGE)
            .expect("the spell page");
        let mut p = SpellbookPanel::default();
        p.post_init(ui, page);
        assert!(
            p.list.is_some(),
            "the premise: the shipped tree carries the spell list"
        );
        p
    }

    /// One pass of the panel's own per-frame update, and whether it rebuilt.
    pub fn update(c: &mut HeadlessClient, p: &mut SpellbookPanel, v: &Book) -> bool {
        let ui = examine::parts(c).0;
        p.update(ui, v)
    }

    /// The spells the rows hold, in row order -- read off the elements and not off the panel's
    /// own mirror.
    pub fn slot_order(p: &SpellbookPanel) -> Vec<u32> {
        p.list
            .as_ref()
            .expect("the spell list")
            .slots
            .iter()
            .filter_map(|s| s.spell)
            .collect()
    }

    /// The badge the row holding `spell` is drawing over its picture.
    pub fn badge(c: &HeadlessClient, p: &SpellbookPanel, spell: u32) -> Option<DataId> {
        let ui = &c.view().expect_app().ui().expect("the UI shell").ui;
        let s = p
            .list
            .as_ref()?
            .slots
            .iter()
            .find(|s| s.spell == Some(spell))?;
        match s.icon_recipe(ui)? {
            dereth_ui::region::IconRecipe::Spell { overlay, .. } => overlay,
            dereth_ui::region::IconRecipe::Object { .. } => None,
        }
    }
}

// ---------------------------------------------------------------------------------------------
// spell-components.strip.* over a recorded session
//
// One recording's server half is replayed into the client through
// `dereth_testkit::Inbound::from_corpus`, which decides which envelope each blob is in.
//
// **No count and no pack is written down.** These scenarios walk the tracker the recording
// filled and assert the page against it, with the premise that it filled *something* stated
// rather than assumed -- a login whose pack held no component would make every assertion below
// vacuous, and that is a different answer from a page that drew nothing.
// ---------------------------------------------------------------------------------------------

/// The recording these scenarios replay: the training dungeon, whose character carries spell
/// components.
const A_RECORDED_SESSION: &str = "long-solo-play";
/// Its character.
const A_RECORDED_PLAYER: dereth_primitives::ObjectId = dereth_primitives::ObjectId(0x5000_000A);
/// The item type every call site of the component update gates on.
const A_COMPONENT: u32 = 0x0000_1000;

pub fn the_components_page_is_the_census_of_what_the_player_carries() {
    use dereth_ui_screens::panels::spellcomponent;

    let mut c = census::a_recorded_pack();
    let rows = census::drawn(&c);
    let tracked = census::tracker_rows(&c);
    let headers = strip::headers(&c);
    let non_empty = census::kinds_the_tracker_has(&c);

    // The premise, stated: the recording really did leave components in the pack.
    assert!(!rows.is_empty(), "the recorded pack carries components");
    assert!(!headers.is_empty(), "so the page has at least one heading");
    assert!(
        headers.len() < spellcomponent::CATEGORY_TITLES.len(),
        "and not one of every kind"
    );

    // The list itself: headings and rows and nothing else, a heading first.
    let ids = census::item_ids(&mut c);
    let header_count = ids
        .iter()
        .filter(|i| **i == spellcomponent::HEADER_ELEMENT)
        .count();
    let row_count = ids
        .iter()
        .filter(|i| **i == spellcomponent::ROW_ELEMENT)
        .count();
    let first_is_a_header = ids.first() == Some(&spellcomponent::HEADER_ELEMENT);

    // Row for row against the tracker, and then against the three columns on screen.
    let matches_tracker = rows
        .iter()
        .zip(tracked.iter())
        .all(|(r, (_, wcid, name, owned))| r.wcid == *wcid && &r.name == name && r.owned == *owned);
    let wanted = rows
        .iter()
        .all(|r| r.desired == c.view().world().player_system.desired_comp_level(r.wcid));
    let columns: Vec<(String, String, String)> = rows
        .iter()
        .map(|r| {
            let e = r.element;
            (
                strip::child_text(&mut c, e, spellcomponent::row::NAME).unwrap_or_default(),
                strip::child_text(&mut c, e, spellcomponent::row::OWNED).unwrap_or_default(),
                strip::child_text(&mut c, e, spellcomponent::row::DESIRED).unwrap_or_default(),
            )
        })
        .collect();
    let want_columns: Vec<(String, String, String)> = rows
        .iter()
        .map(|r| (r.name.clone(), r.owned.to_string(), r.desired.to_string()))
        .collect();

    // Every drawn row stands for a real component object of its own kind.
    let real = rows.iter().all(|r| {
        r.object.is_some_and(|o| {
            c.view()
                .world()
                .weenie(o)
                .is_some_and(|w| w.pwd.wcid == r.wcid && w.inq_type() & A_COMPONENT != 0)
        })
    });

    let row_len = rows.len();
    let tracked_len = tracked.len();
    let item_len = ids.len();

    c.assert_behaviour(
        "spell-components.strip.the-list-is-the-census-of-what-the-player-carries",
        move |_| {
            headers == non_empty
                && header_count == headers.len()
                && row_count == row_len
                && header_count + row_count == item_len
                && first_is_a_header
                && row_len == tracked_len
                && matches_tracker
                && wanted
                && columns == want_columns
                && real
        },
    );
    c.shutdown();
}

pub fn the_held_count_follows_a_stack_size_the_shard_changes() {
    use dereth_protocol::items::ItemUpdateStackSize;
    use dereth_ui_screens::panels::spellcomponent;

    let mut c = census::a_recorded_pack();
    let rows = census::drawn(&c);

    // A row whose object is a **stack**: an un-stacked one takes a different arm and could not
    // move.
    let row = rows
        .iter()
        .find(|r| {
            r.object.is_some_and(|o| {
                c.view()
                    .world()
                    .weenie(o)
                    .is_some_and(|w| w.pwd.stack_size.is_some())
            })
        })
        .cloned()
        .unwrap_or_else(|| panic!("the recorded pack carries a stacked component"));
    let object = row.object.expect("checked just above");
    let stack = c
        .view()
        .world()
        .weenie(object)
        .and_then(|w| w.pwd.stack_size)
        .expect("a stack");
    let before_text = strip::child_text(&mut c, row.element, spellcomponent::row::OWNED);
    let before_owned = row.owned;
    let rebuilds_before = strip::rebuilds(&c);
    let applied_before = c.view().interaction().stats.stack_sizes_applied;

    // The stamp gate is per property and eight bits: the next one after whatever the login left.
    let sequence = c
        .view()
        .world()
        .weenie(object)
        .and_then(|w| w.stamper.as_ref())
        .and_then(|s| s.stamp(ItemUpdateStackSize::STAMPER_KEY))
        .map_or(1, |s| s.wrapping_add(1));
    let new_value = u32::from(stack) + 7;
    c.when(dereth_testkit::Inbound::message(&ItemUpdateStackSize {
        sequence,
        item: object,
        amount: new_value,
        new_value,
    }));
    let applied = c.view().interaction().stats.stack_sizes_applied;
    c.tick(2);

    let owned_now = census::tracker_rows(&c)
        .iter()
        .find(|(_, w, _, _)| *w == row.wcid)
        .map(|(_, _, _, n)| *n)
        .unwrap_or_else(|| panic!("the row's kind is still tracked"));
    let rebuilds_after = strip::rebuilds(&c);
    let drawn_now = census::drawn(&c)
        .into_iter()
        .find(|r| r.wcid == row.wcid)
        .unwrap_or_else(|| panic!("the row is still drawn"));
    let after_text = strip::child_text(&mut c, drawn_now.element, spellcomponent::row::OWNED);
    let want = before_owned - i64::from(stack) + i64::from(new_value);

    c.assert_behaviour(
        "spell-components.strip.the-held-count-follows-a-stack-size-the-shard-changes",
        move |_| {
            before_text == Some(before_owned.to_string())
                && applied == applied_before + 1
                && owned_now == want
                && rebuilds_after > rebuilds_before
                && drawn_now.owned == owned_now
                && after_text == Some(owned_now.to_string())
                && after_text != before_text
        },
    );
    c.shutdown();
}

pub fn a_click_on_a_component_row_selects_that_component_in_the_world() {
    use dereth_ui_screens::panels::spellcomponent;

    let mut c = census::a_recorded_pack();
    let row = census::drawn(&c).first().cloned().expect("a drawn row");
    let items = census::items(&c);
    let index = items
        .iter()
        .position(|h| *h == row.element)
        .expect("the row is in the list");
    let first_is_a_header =
        census::element_id(&c, items[0]) == spellcomponent::HEADER_ELEMENT && index >= 1;
    let is_a_row = census::element_id(&c, row.element) == spellcomponent::ROW_ELEMENT;

    // The object the row stands for is a real component object of its kind.
    let object = row.object.expect("a drawn row always has an object");
    let real = c
        .view()
        .world()
        .weenie(object)
        .is_some_and(|w| w.pwd.wcid == row.wcid && w.pwd.obj_type & A_COMPONENT != 0);

    // The recorded login leaves something selected. Clear it, so the press is measured from
    // nothing and a leftover cannot be mistaken for the row's.
    c.world_mut()
        .set_selected_object(None, false, &mut dereth_client_model::NullSink);
    let cleared = c.view().world().selected.is_none();
    let selections_before = c.view().interaction().stats.selections;
    let sent_before = c.outbound().len();

    census::click(&mut c, row.element, 2_000);

    let picked = census::selected_index(&c);
    let selected = c.view().world().selected;
    let flagged = c
        .view()
        .world()
        .tables
        .weenies
        .get(object)
        .is_some_and(|w| w.selected);
    let selections = c.view().interaction().stats.selections;
    let asked = census::asked_for_an_action(&c, sent_before);

    c.assert_behaviour(
        "spell-components.strip.a-click-on-a-row-selects-that-component-in-the-world",
        move |_| {
            first_is_a_header
                && is_a_row
                && real
                && cleared
                && picked == Some(index)
                && selections == selections_before + 1
                && selected == Some(object)
                && flagged
                && !asked
        },
    );
    c.shutdown();
}

pub fn a_click_on_a_header_clears_the_selection_and_the_first_one_does_nothing() {
    use dereth_ui_screens::panels::spellcomponent;

    let mut c = census::a_recorded_pack();
    let row = census::drawn(&c).first().cloned().expect("a drawn row");
    let object = row.object.expect("a drawn row always has an object");
    census::click(&mut c, row.element, 2_000);
    let selected = c.view().world().selected == Some(object);

    // The first heading: the list takes the press and the world selection does not move.
    let first = census::items(&c)[0];
    let first_is_a_header = census::element_id(&c, first) == spellcomponent::HEADER_ELEMENT;
    let before = c.view().interaction().stats.selections;
    census::click(&mut c, first, 3_000);
    let at_the_first = (
        census::selected_index(&c),
        c.view().interaction().stats.selections,
        c.view().world().selected,
    );

    // Any other heading clears it -- and the notice the clearing raises takes the highlight the
    // press left on the heading off again.
    let (_, header) = census::a_header_past_the_first(&c)
        .unwrap_or_else(|| panic!("a heading past the first is on screen to be pressed"));
    census::click(&mut c, header, 4_000);
    let after = (
        c.view().interaction().stats.selections,
        c.view().world().selected,
        census::selected_index(&c),
        c.view()
            .world()
            .tables
            .weenies
            .get(object)
            .is_some_and(|w| w.selected),
    );

    c.assert_behaviour(
        "spell-components.strip.a-click-on-a-header-clears-the-selection-and-the-first-one-does-nothing",
        move |_| {
            selected
                && first_is_a_header
                && at_the_first.0 == Some(0)
                && at_the_first.1 == before
                && at_the_first.2 == Some(object)
                && after.0 == before + 1
                && after.1.is_none()
                && after.2.is_none()
                && !after.3
        },
    );
    c.shutdown();
}

pub fn selecting_a_component_in_the_world_highlights_its_row() {
    use dereth_ui_screens::panels::spellcomponent;

    let mut c = census::a_recorded_pack();

    // A second object of a kind the page already draws, made by replaying the recording's own
    // create for one of them with the object id changed -- so the kind-not-object rule below has
    // something to prove. Nothing is invented but the id.
    let source = census::drawn(&c)[0]
        .object
        .expect("every drawn row has an object");
    census::a_second_pile_of(&mut c, source, dereth_primitives::ObjectId(0x8116_F5F5));
    let rows = census::drawn(&c);

    c.world_mut()
        .set_selected_object(None, false, &mut dereth_client_model::NullSink);
    c.tick(2);
    let cleared = (census::selected_index(&c), strip::panel(&c).selected_object);

    // Every component object the player holds, by kind.
    let owned: Vec<(dereth_primitives::ObjectId, u32)> = {
        let w = c.view().world();
        w.exhaustive_contained_items(A_RECORDED_PLAYER)
            .into_iter()
            .filter_map(|o| {
                w.magic
                    .components
                    .object_is_owned_component(o)
                    .map(|k| (o, k))
            })
            .collect()
    };
    assert!(
        !owned.is_empty(),
        "the recorded pack holds tracked component objects"
    );

    // The object must **not** be its row's own representative: matching on the object rather than
    // on the kind would answer correctly for exactly those, and this scenario has to be able to
    // see that mistake.
    let (object, wcid) = owned
        .iter()
        .find(|(o, k)| rows.iter().any(|r| r.wcid == *k && r.object != Some(*o)))
        .copied()
        .unwrap_or_else(|| panic!("a drawn kind with a second object"));
    let row = rows
        .iter()
        .find(|r| r.wcid == wcid)
        .cloned()
        .expect("its row");
    let index = census::items(&c)
        .iter()
        .position(|h| *h == row.element)
        .expect("it is listed");

    c.world_mut()
        .set_selected_object(Some(object), false, &mut dereth_client_model::NullSink);
    c.tick(2);
    let lit = (
        census::selected_index(&c),
        census::element_id(&c, census::items(&c)[index]),
        strip::panel(&c).selected_object,
        strip::panel(&c).selection_notices,
        c.view().world().selected,
        strip::panel(&c).broadcast_selection,
    );

    // The same selection again does nothing at all.
    let notices = strip::panel(&c).selection_notices;
    c.tick(3);
    let unchanged = strip::panel(&c).selection_notices;

    // Something that is not a component clears the row...
    c.world_mut().set_selected_object(
        Some(A_RECORDED_PLAYER),
        false,
        &mut dereth_client_model::NullSink,
    );
    c.tick(2);
    let after_a_non_component = (census::selected_index(&c), strip::panel(&c).selected_object);

    // ...and with nothing remembered it does nothing rather than clearing again.
    let notices_now = strip::panel(&c).selection_notices;
    c.world_mut()
        .set_selected_object(None, false, &mut dereth_client_model::NullSink);
    c.tick(2);
    let quiet = strip::panel(&c).selection_notices;

    c.assert_behaviour(
        "spell-components.strip.selecting-a-component-in-the-world-highlights-its-row",
        move |_| {
            cleared == (None, None)
                && lit.0 == Some(index)
                && lit.1 == spellcomponent::ROW_ELEMENT
                && lit.2 == Some(object)
                && lit.3 == 1
                && lit.4 == Some(object)
                && lit.5
                && unchanged == notices
                && after_a_non_component == (None, None)
                && quiet == notices_now
        },
    );
    c.shutdown();
}

#[test]
fn scenario_the_components_page_is_the_census_of_what_the_player_carries() {
    scenario("the_components_page_is_the_census_of_what_the_player_carries");
}

#[test]
fn scenario_the_held_count_follows_a_stack_size_the_shard_changes() {
    scenario("the_held_count_follows_a_stack_size_the_shard_changes");
}

#[test]
fn scenario_a_click_on_a_component_row_selects_that_component_in_the_world() {
    scenario("a_click_on_a_component_row_selects_that_component_in_the_world");
}

#[test]
fn scenario_a_click_on_a_header_clears_the_selection_and_the_first_one_does_nothing() {
    scenario("a_click_on_a_header_clears_the_selection_and_the_first_one_does_nothing");
}

#[test]
fn scenario_selecting_a_component_in_the_world_highlights_its_row() {
    scenario("selecting_a_component_in_the_world_highlights_its_row");
}

/// The recorded pack, the Components tab, and the list's own items.
mod census {
    use dereth_primitives::ObjectId;
    use dereth_testkit::{ClientSpec, HeadlessClient, Inbound};
    use dereth_ui::{ElemHandle, ElementId};
    use dereth_ui_screens::panels::remaining::SPELL_PAGE;
    use dereth_ui_screens::panels::spellcomponent::{self, COMPONENT_LIST};
    use dereth_ui_screens::screens::gameplay::GamePlayScreen;

    use super::{examine, strip};

    /// A whole client with the recorded session's server half replayed into it, the spell page up
    /// and the Components tab pressed.
    pub fn a_recorded_pack() -> HeadlessClient {
        let mut c = HeadlessClient::new(ClientSpec::gameplay(4));
        c.world_mut().player = Some(super::A_RECORDED_PLAYER);
        c.when(Inbound::from_corpus(
            super::A_RECORDED_SESSION,
            0..usize::MAX,
        ));
        c.tick(3);
        open_the_components_tab(&mut c);
        c
    }

    fn open_the_components_tab(c: &mut HeadlessClient) {
        examine::open_the_spellbook(c);
        c.tick(3);
        let tab = components_tab(c);
        click(c, tab, 1_000);
        c.tick(3);
        let list = list(c);
        assert!(
            c.view()
                .expect_app()
                .ui()
                .expect("the UI shell")
                .ui
                .is_visible(list),
            "pressing the Components tab brings the list up"
        );
    }

    /// The Components tab's caption, discovered off the spell page's own tab table -- the
    /// sub-page whose subtree carries the components list.
    fn components_tab(c: &HeadlessClient) -> ElemHandle {
        let app = c.view().expect_app();
        let ui = &app.ui().expect("the UI shell").ui;
        let root = screen(c).root().expect("the gameplay root");
        let page = ui
            .get_child_recursive(root, SPELL_PAGE)
            .expect("the spell page");
        let pairs: Vec<(ElementId, ElementId)> = ui
            .node(page)
            .and_then(|n| {
                n.behaviour
                    .as_ref()?
                    .as_any()?
                    .downcast_ref::<dereth_ui::widgets::panel::Panel>()
            })
            .expect("the spell page is a tabbed panel")
            .page_to_tab
            .iter()
            .map(|(p, t)| (*p, *t))
            .collect();
        assert!(pairs.len() >= 2, "the spell page is tabbed");
        for (page_id, tab_id) in pairs {
            let Some(pe) = ui.get_child_recursive(page, page_id) else {
                continue;
            };
            if ui.get_child_recursive(pe, COMPONENT_LIST).is_some() {
                return ui
                    .get_child_recursive(page, tab_id)
                    .expect("that sub-page's tab caption");
            }
        }
        panic!("no sub-page of the spell page carries the components list");
    }

    fn screen(c: &HeadlessClient) -> &GamePlayScreen {
        let any: &dyn std::any::Any = c
            .view()
            .expect_app()
            .ui()
            .expect("the UI shell")
            .flow
            .current()
            .expect("a screen");
        any.downcast_ref().expect("the gameplay screen")
    }

    pub fn list(c: &HeadlessClient) -> ElemHandle {
        let root = screen(c).root().expect("the gameplay root");
        c.view()
            .expect_app()
            .ui()
            .expect("the UI shell")
            .ui
            .get_child_recursive(root, COMPONENT_LIST)
            .expect("the components list")
    }

    /// The list's items, in order: headings and rows interleaved.
    pub fn items(c: &HeadlessClient) -> Vec<ElemHandle> {
        let ui = &c.view().expect_app().ui().expect("the UI shell").ui;
        ui.node(list(c))
            .and_then(|n| {
                n.behaviour
                    .as_ref()?
                    .as_any()?
                    .downcast_ref::<dereth_ui::widgets::listbox::ListBox>()
            })
            .expect("the components list is a list box")
            .items
            .clone()
    }

    pub fn item_ids(c: &mut HeadlessClient) -> Vec<ElementId> {
        items(c).iter().map(|h| element_id(c, *h)).collect()
    }

    /// Which item the list has selected.
    pub fn selected_index(c: &HeadlessClient) -> Option<usize> {
        let ui = &c.view().expect_app().ui().expect("the UI shell").ui;
        ui.node(list(c))
            .and_then(|n| n.behaviour.as_ref())
            .and_then(|b| (**b).as_any())
            .and_then(|a| a.downcast_ref::<dereth_ui::widgets::listbox::ListBox>())
            .and_then(|l| l.selected)
    }

    pub fn element_id(c: &HeadlessClient, h: ElemHandle) -> ElementId {
        c.view()
            .expect_app()
            .ui()
            .expect("the UI shell")
            .ui
            .node(h)
            .expect("alive")
            .element_id()
    }

    /// The drawn rows, with the premise that the recording filled the pack stated rather than
    /// assumed.
    pub fn drawn(c: &HeadlessClient) -> Vec<spellcomponent::DrawnRow> {
        let p = strip::panel(c);
        assert!(p.bound(), "the shipped tree carries the components list");
        assert_eq!(
            p.templates(),
            2,
            "with its heading template and its row template"
        );
        assert!(p.rebuilds >= 1, "and the page has been built at least once");
        assert!(
            !p.rows.is_empty(),
            "the recorded session's pack carries spell components; {} objects are tracked",
            c.view().world().magic.components.tracked_objects()
        );
        p.rows.clone()
    }

    /// The tracker's own census: `(kind order, kind, name, held)` in the page's walk order.
    pub fn tracker_rows(c: &HeadlessClient) -> Vec<(u32, u32, String, i64)> {
        c.view()
            .world()
            .magic
            .components
            .categories()
            .flat_map(|(k, rows)| {
                rows.iter()
                    .map(|d| (k, d.class_id, d.name.clone(), d.num_items()))
                    .collect::<Vec<_>>()
            })
            .collect()
    }

    /// The kinds the tracker has rows in, in its own order.
    pub fn kinds_the_tracker_has(c: &HeadlessClient) -> Vec<u32> {
        c.view()
            .world()
            .magic
            .components
            .categories()
            .filter(|(_, rows)| !rows.is_empty())
            .map(|(k, _)| k)
            .collect()
    }

    /// One real press on a list item, with the hit test asserted first so that a hit-test miss
    /// cannot masquerade as a missing selection. For a list row the hit is the **list box**: the
    /// list owns the press and finds the row under the pointer itself.
    pub fn click(c: &mut HeadlessClient, h: ElemHandle, at: u32) {
        let (x, y) = {
            let ui = &c.view().expect_app().ui().expect("the UI shell").ui;
            let mut parent = Some(h);
            while let Some(p) = parent {
                assert!(
                    ui.node(p).expect("alive").region.flags.visible,
                    "a visible ancestor"
                );
                parent = ui.parent(p);
            }
            let r = ui.screen_clip_box(h);
            assert!(r.is_valid(), "the item is on screen to be pressed");
            let at = ((r.x0 + r.x1) / 2, (r.y0 + r.y1) / 2);
            let hit = ui.hit_test_screen(at.0, at.1);
            assert!(
                hit.is_some_and(|hit| hit == h
                    || ui.is_ancestor_of(h, hit)
                    || owning_list(ui, hit, h)),
                "the hit test picks that item, a descendant of it, or the list box over it"
            );
            at
        };
        examine::press(c, x, y, at, examine::PRIMARY);
    }

    fn owning_list(ui: &dereth_ui::UiSystem, hit: ElemHandle, h: ElemHandle) -> bool {
        ui.node(hit).map(dereth_ui::ElementNode::element_id) == Some(COMPONENT_LIST)
            && ui.is_ancestor_of(hit, h)
    }

    /// The first heading past the first that is on screen to be pressed.
    pub fn a_header_past_the_first(c: &HeadlessClient) -> Option<(usize, ElemHandle)> {
        let items = items(c);
        let ui = &c.view().expect_app().ui().expect("the UI shell").ui;
        items.iter().enumerate().skip(1).find_map(|(i, h)| {
            if element_id(c, *h) != spellcomponent::HEADER_ELEMENT {
                return None;
            }
            let r = ui.screen_clip_box(*h);
            let at = ((r.x0 + r.x1) / 2, (r.y0 + r.y1) / 2);
            let hit = ui.hit_test_screen(at.0, at.1);
            (r.is_valid()
                && hit
                    .is_some_and(|x| x == *h || ui.is_ancestor_of(*h, x) || owning_list(ui, x, *h)))
            .then_some((i, *h))
        })
    }

    /// Whether the client asked the shard to *do* anything since `from`. A selection asks for no
    /// action at all.
    pub fn asked_for_an_action(c: &HeadlessClient, from: usize) -> bool {
        c.outbound()[from..].iter().any(|r| {
            matches!(
                r,
                dereth_client_model::Request::UseEvent(_)
                    | dereth_client_model::Request::UseWithTargetEvent(_)
            )
        })
    }

    /// A second pile of `source`'s kind, made by replaying the recording's **own** create for it
    /// with the object id changed. Nothing is fabricated but the id: the body is the recorded one
    /// and it goes through the same reader the recording's other blobs do, so the pile arrives in
    /// the same container with the same kind, name and stack.
    pub fn a_second_pile_of(c: &mut HeadlessClient, source: ObjectId, fresh: ObjectId) {
        /// The create the shard sends for one object.
        const CREATE: u32 = 0xF745;
        let corpus =
            dereth_client_net::client_session::testing::Corpus::load(super::A_RECORDED_SESSION)
                .expect("the recording parses")
                .expect("the decoded corpus carries it");
        let rows: Vec<&dereth_client_net::client_session::testing::CorpusBlob> = corpus
            .blobs
            .iter()
            .filter(|r| {
                r.dir == dereth_client_net::client_session::testing::Direction::ServerToClient
                    && r.opcode == CREATE
                    && r.payload.get(4..8) == Some(source.0.to_le_bytes().as_slice())
            })
            .collect();
        assert_eq!(
            rows.len(),
            1,
            "exactly one recorded create makes {source:?}"
        );
        let mut payload = rows[0].payload.clone();
        payload[4..8].copy_from_slice(&fresh.0.to_le_bytes());
        c.when(Inbound::event(
            dereth_client_net::client_session::SessionEvent::WorldObject {
                opcode: dereth_protocol::Opcode(CREATE),
                body: payload[4..].to_vec(),
            },
        ));
        c.tick(2);
        let w = c.view().world();
        assert!(
            w.weenie(fresh).is_some(),
            "the replayed create made the second pile"
        );
        assert_eq!(
            w.magic.components.object_is_owned_component(fresh),
            w.magic.components.object_is_owned_component(source),
            "and it is tracked as the same kind"
        );
    }
}

// ---------------------------------------------------------------------------------------------
// spellbar.keys.*, spellbar.tabs.* and spellbar.click.*
//
// The casting bar's keys, tabs and clicks. Combat's own claims are in `combat.rs`.
//
// Nothing here writes a key down: every control is discovered from the shipped keymap, and the
// bar is the one the shipped gameplay tree carries.
// ---------------------------------------------------------------------------------------------

pub fn every_shipped_magic_key_raises_its_own_instruction() {
    use dereth_ui_screens::view::MagicNotice;

    let mut shell = bar::shell();
    shell.set_combat_input_maps(dereth_input::combat::mode::MAGIC);
    let mut d = bar::Driver::new();

    // Stage one from the shipped data: every default the casting section binds.
    let bound = bar::shipped_bindings(&shell, dereth_input::combat::MAGIC_COMBAT_MAP);
    assert!(
        bound.len() > 10,
        "the shipped casting section binds a real set of keys"
    );

    let mut every_key_holds = true;
    let mut seen = 0usize;
    for (action, qc) in &bound {
        let (down, up) = d.press_release(&mut shell, qc);
        every_key_holds &= down.iter().map(|e| e.id).eq([*action]);
        let mut bench = bar::Bench::new();
        let _ = bench.take_notices();
        bench.drive(down, 100.0);
        let raised = bench.take_notices();
        every_key_holds &= raised == vec![bar::the_instruction_for(action.0)];

        // Letting the key go raises nothing: the whole arm is behind "this is a press". Every
        // shipped default is a one-shot and produces no release of its own, so the arm is asked
        // the question directly with the event the input layer would have handed it.
        every_key_holds &= up.is_empty();
        bench.drive(vec![bar::released(*action)], 101.0);
        every_key_holds &= bench.take_notices().is_empty() && bench.magic_actions() == 1;
        seen += 1;
    }

    // The ones that name a numbered slot each name their own, counting from the first.
    let slots_are_their_own = [0_usize, 4, 8].iter().all(|n| {
        bar::the_instruction_for(
            dereth_client::interaction::action::USE_SPELL_SLOT_FIRST
                + u32::try_from(*n).expect("a small slot"),
        ) == MagicNotice::CastQuickslotSpell { slot: *n }
    });

    let mut c = HeadlessClient::model();
    c.assert_behaviour(
        "spellbar.keys.every-shipped-magic-key-raises-its-own-instruction-and-the-release-raises-none",
        move |_| every_key_holds && seen == bound.len() && slots_are_their_own,
    );
}

pub fn the_instructions_move_the_selection_and_the_cast_keys_cast() {
    use dereth_ui_screens::view::{MagicNotice, UiRequest};

    let (mut ui, screen) = bar::gameplay();
    let mut p = dereth_ui_screens::panels::remaining::RemainingPanels::default();
    p.post_init(&mut ui, screen.root().expect("the gameplay root"));
    let bound = p.spellcasting.bound() && p.spellcasting.lists_bound() == 8;

    let view = bar::StubView::with_spells(vec![101, 202, 303]);
    p.update(&mut ui, &view);
    let tab = p.spellcasting.open_sub_menu_index(&ui);
    let mut tell =
        |n: MagicNotice,
         ui: &mut dereth_ui::UiSystem,
         p: &mut dereth_ui_screens::panels::remaining::RemainingPanels| {
            let moved = p.spellcasting.recv_magic_notice(ui, n, &view);
            (moved, p.spellcasting.sub_menus[tab].selected_spell)
        };

    let nothing_at_first = p.spellcasting.sub_menus[tab].selected_spell == 0;
    let first = tell(MagicNotice::NextSpellSelection, &mut ui, &mut p) == (true, 101);
    // A second step, because a handler that selects the first every time is otherwise green.
    let steps = tell(MagicNotice::NextSpellSelection, &mut ui, &mut p) == (true, 202)
        && tell(MagicNotice::PrevSpellSelection, &mut ui, &mut p) == (true, 101);
    let ends = tell(MagicNotice::LastSpellSelection, &mut ui, &mut p) == (true, 303)
        && tell(MagicNotice::NextSpellSelection, &mut ui, &mut p) == (true, 101)
        && tell(MagicNotice::FirstSpellSelection, &mut ui, &mut p) == (true, 101)
        && tell(MagicNotice::PrevSpellSelection, &mut ui, &mut p) == (true, 303);

    // The cast instruction casts what is selected; the one naming a slot moves the selection
    // first and then casts it.
    ui.requests.clear();
    let casts = p
        .spellcasting
        .recv_magic_notice(&mut ui, MagicNotice::CastCurrentSpell, &view)
        && ui.requests.take() == vec![UiRequest::CastSpell { spell_id: 303 }];
    let by_slot = p.spellcasting.recv_magic_notice(
        &mut ui,
        MagicNotice::CastQuickslotSpell { slot: 1 },
        &view,
    ) && ui.requests.take() == vec![UiRequest::CastSpell { spell_id: 202 }]
        && p.spellcasting.sub_menus[tab].selected_spell == 202;

    // A slot past the end does nothing at all -- not even a refusal.
    let past_the_end = !p.spellcasting.recv_magic_notice(
        &mut ui,
        MagicNotice::CastQuickslotSpell { slot: 9 },
        &view,
    ) && ui.requests.take().is_empty();

    let mut c = HeadlessClient::model();
    c.assert_behaviour(
        "spellbar.keys.the-instructions-move-the-selection-and-the-cast-keys-cast",
        move |_| {
            bound && nothing_at_first && first && steps && ends && casts && by_slot && past_the_end
        },
    );
}

pub fn the_tab_instructions_walk_the_eight_banks() {
    use dereth_ui_screens::panels::spellcasting::{
        SpellcastingPanel as S, SUB_MENU_PAGES, SUB_MENU_TAB_BUTTONS,
    };
    use dereth_ui_screens::view::MagicNotice;

    // The ladder as a pure function, in both directions and past both ends.
    let first = SUB_MENU_TAB_BUTTONS[0];
    let last = SUB_MENU_TAB_BUTTONS[SUB_MENU_TAB_BUTTONS.len() - 1];
    let ladder = S::step_tab_id(first, true) != first
        && S::step_tab_id(last, true) == first
        && S::step_tab_id(first, false) == last
        // A bank the client does not know falls back to the first, either way.
        && S::step_tab_id(0, true) == first
        && S::step_tab_id(0, false) == first;

    let (mut ui, screen) = bar::gameplay();
    let mut p = dereth_ui_screens::panels::remaining::RemainingPanels::default();
    p.post_init(&mut ui, screen.root().expect("the gameplay root"));
    let view = bar::StubView::default();

    let start = p.spellcasting.open_sub_menu_index(&ui);
    let forward = p
        .spellcasting
        .recv_magic_notice(&mut ui, MagicNotice::NextSpellTab, &view);
    bar::pump(&mut ui);
    let moved = p.spellcasting.open_sub_menu_index(&ui) != start;
    let back = p
        .spellcasting
        .recv_magic_notice(&mut ui, MagicNotice::PrevSpellTab, &view);
    bar::pump(&mut ui);
    let returned = p.spellcasting.open_sub_menu_index(&ui) == start;

    let to_the_end = p
        .spellcasting
        .recv_magic_notice(&mut ui, MagicNotice::LastSpellTab, &view);
    bar::pump(&mut ui);
    let at_the_end = p.spellcasting.open_sub_menu_index(&ui) == SUB_MENU_PAGES.len() - 1;
    let to_the_start = p
        .spellcasting
        .recv_magic_notice(&mut ui, MagicNotice::FirstSpellTab, &view);
    bar::pump(&mut ui);
    let at_the_start = p.spellcasting.open_sub_menu_index(&ui) == 0;

    let mut c = HeadlessClient::model();
    c.assert_behaviour(
        "spellbar.tabs.the-tab-instructions-walk-the-eight-banks-and-stop-at-the-ends",
        move |_| {
            ladder
                && forward
                && moved
                && back
                && returned
                && to_the_end
                && at_the_end
                && to_the_start
                && at_the_start
        },
    );
}

pub fn the_frames_own_pass_delivers_what_was_queued_in_order() {
    use dereth_ui_screens::view::MagicNotice;

    let (mut ui, screen) = bar::gameplay();
    let mut p = dereth_ui_screens::panels::remaining::RemainingPanels::default();
    p.post_init(&mut ui, screen.root().expect("the gameplay root"));
    let view = bar::StubView::with_spells(vec![101, 202, 303]);
    let tab = p.spellcasting.open_sub_menu_index(&ui);

    // Nothing queued: nothing moves.
    ui.notice_inbox.clear();
    p.update(&mut ui, &view);
    let idle = p.spellcasting.sub_menus[tab].selected_spell == 0;

    // One, queued the way the client's own magic arm queues it.
    ui.notice_inbox.emit(MagicNotice::NextSpellSelection);
    let queued = ui.notice_inbox.len() == 1;
    p.update(&mut ui, &view);
    let drained = ui.notice_inbox.is_empty() && p.spellcasting.sub_menus[tab].selected_spell == 101;

    // Two in one frame, in order -- which is what a player mashing the key produces.
    ui.notice_inbox.emit(MagicNotice::NextSpellSelection);
    ui.notice_inbox.emit(MagicNotice::NextSpellSelection);
    p.update(&mut ui, &view);
    let both = p.spellcasting.sub_menus[tab].selected_spell == 303;

    let mut c = HeadlessClient::model();
    c.assert_behaviour(
        "spellbar.keys.the-frames-own-pass-delivers-what-was-queued-in-order",
        move |_| idle && queued && drained && both,
    );
}

pub fn a_real_key_press_moves_the_selection_the_ring_and_the_scroll() {
    use dereth_client::interaction::action as ia;
    use dereth_ui_screens::view::UiRequest;

    let (mut ui, screen) = bar::gameplay();
    let mut p = dereth_ui_screens::panels::remaining::RemainingPanels::default();
    p.post_init(&mut ui, screen.root().expect("the gameplay root"));
    let view = bar::StubView::with_spells((101..=124).collect());
    p.update(&mut ui, &view);
    let tab = p.spellcasting.open_sub_menu_index(&ui);

    // **Bringing the panels up is not a gesture**, and what it leaves behind is listed rather
    // than filtered, so that a bind which starts asking for something else fails here instead of
    // being absorbed by a blanket allowance.
    let baseline = ui.requests.take();
    let bind_asks_for = baseline
        == vec![
            // The allegiance panel's question puts the busy cursor up before it is asked.
            UiRequest::Busy { raised: true },
            UiRequest::AllegianceUpdateRequest { on: true },
            UiRequest::SetTalkFocusEnabled {
                focus: 5,
                enabled: false,
            },
            UiRequest::SetTalkFocusEnabled {
                focus: 4,
                enabled: false,
            },
            UiRequest::SetTalkFocusEnabled {
                focus: 6,
                enabled: false,
            },
        ];

    let mut shell = bar::shell();
    shell.set_combat_input_maps(dereth_input::combat::mode::MAGIC);
    let qc = bar::the_shipped_control(
        &shell,
        dereth_input::combat::MAGIC_COMBAT_MAP,
        ia::COMBAT_NEXT_SPELL,
    );
    let mut d = bar::Driver::new();
    let mut bench = bar::Bench::new();

    let mut every_press_holds = true;
    for (i, spell) in view.spells.iter().enumerate() {
        let (down, up) = d.press_release(&mut shell, &qc);
        every_press_holds &= up.is_empty();
        #[allow(clippy::cast_precision_loss)]
        bench.drive(down, 100.0 + i as f64 * 6.0);
        // The host's hand-over: what the interaction layer raised goes to the UI's inbox.
        for n in bench.take_notices() {
            ui.notice_inbox.emit(n);
        }
        // The frame's own consumer, not a direct call into the panel.
        p.update(&mut ui, &view);
        every_press_holds &=
            p.spellcasting.sub_menus[tab].selected_spell == *spell && ui.requests.is_empty();

        let w = p.spellcasting.lists[tab]
            .as_ref()
            .expect("the open bank's list");
        // Just far enough to reveal the row that was off the end; a row already on screen is not
        // scrolled to.
        #[allow(clippy::cast_possible_truncation, clippy::cast_possible_wrap)]
        let want = ((i as i32 + 1) * w.cell.0 - ui.screen_box(w.handle).width()).max(0);
        every_press_holds &= w.scroll(&ui) == (want, 0);
        every_press_holds &= w.slots.iter().enumerate().all(|(j, slot)| {
            ui.node(slot.selected_ring.expect("the shipped ring"))
                .expect("alive")
                .region
                .flags
                .visible
                == (i == j)
        });
    }

    // The bar really did overflow, or the scroll above would be a claim about nothing.
    let w = p.spellcasting.lists[tab]
        .as_ref()
        .expect("the open bank's list");
    let viewport = ui.screen_box(w.handle);
    #[allow(clippy::cast_possible_truncation, clippy::cast_possible_wrap)]
    let overflowed = view.spells.len() as i32 * w.cell.0 > viewport.width();
    let tail_is_exactly_visible = ui
        .node(w.slots[view.spells.len() - 1].handle)
        .expect("alive")
        .region
        .box_
        .x1
        == viewport.width() - 1;
    let nothing_was_cast = ui.requests.take().is_empty();

    let mut c = HeadlessClient::model();
    c.assert_behaviour(
        "spellbar.keys.a-real-key-press-moves-the-selection-the-ring-and-the-scroll",
        move |_| {
            bind_asks_for
                && every_press_holds
                && overflowed
                && tail_is_exactly_visible
                && nothing_was_cast
        },
    );
}

pub fn one_click_selects_and_only_the_double_click_casts() {
    use dereth_ui_screens::panels::spellcasting::item_action;
    use dereth_ui_screens::view::UiRequest;

    // The two identities the two arms are told apart by; without them the two gestures are one.
    let told_apart = item_action::SELECT == dereth_ui::focus::action::PRIMARY_CLICK
        && item_action::DOUBLE_CLICK == dereth_ui::focus::DOUBLE_CLICK_ACTIONS[0];

    let (mut ui, mut screen) = bar::gameplay();
    let mut p = dereth_ui_screens::panels::remaining::RemainingPanels::default();
    p.post_init(&mut ui, screen.root().expect("the gameplay root"));
    let view = bar::StubView::with_spells((101..=106).collect());
    p.update(&mut ui, &view);
    let tab = p.spellcasting.open_sub_menu_index(&ui);

    // The second row, not the first, so that "it selected *this* row" cannot be a default.
    let (slot, want) = {
        let w = p.spellcasting.lists[tab]
            .as_ref()
            .expect("the open bank's list");
        (
            w.slots[1].handle,
            w.spell_at(1).expect("row one carries a spell"),
        )
    };
    let not_a_leftover = want == 102;

    // **A pointer cannot reach anything in a window that is shut.** The bar and its open bank
    // start hidden on a freshly built tree, exactly as they do at login, and nothing in this
    // build opens them from the toolbar yet -- which is a separate finding and not this
    // scenario's subject. They are opened here with the same call the toolbar would make;
    // everything after it is the real chain.
    let opened = bar::open_the_window_over(&mut ui, slot) == 2 && ui.is_visible(slot);
    let at = {
        let b = ui.screen_box(slot);
        ((b.x0 + b.x1) / 2, (b.y0 + b.y1) / 2)
    };

    // Bringing the panels up and opening the window are not gestures; the boundary is here.
    bar::pump_to(&mut ui, &mut screen);
    let _ = screen.take_panel_messages();
    ui.requests.clear();

    let msgs = bar::press_through(&mut ui, &mut screen, item_action::SELECT, at);
    let reached = msgs
        .iter()
        .any(|m| m.id == dereth_ui::msg::element::id::MOUSE_PRESS && m.p1 == item_action::SELECT);
    let mut consumed = false;
    for m in &msgs {
        consumed |= p.on_element_message(&mut ui, m, &view);
    }
    let selected = consumed && p.spellcasting.sub_menus[tab].selected_spell == want;
    let cast_nothing = ui.requests.take().is_empty();

    ui.mouse_up(item_action::SELECT, at.0, at.1, false);
    let _ = ui.drain_outbox();
    let _ = screen.take_panel_messages();
    ui.requests.clear();

    // The same element, the same pixel, the same chain: only the gesture differs.
    let msgs = bar::press_through(&mut ui, &mut screen, item_action::DOUBLE_CLICK, at);
    let reached_twice = msgs.iter().any(|m| {
        m.id == dereth_ui::msg::element::id::MOUSE_PRESS && m.p1 == item_action::DOUBLE_CLICK
    });
    for m in &msgs {
        p.on_element_message(&mut ui, m, &view);
    }
    let casts = ui.requests.take() == vec![UiRequest::CastSpell { spell_id: want }];

    let mut c = HeadlessClient::model();
    c.assert_behaviour(
        "spellbar.click.one-click-selects-and-only-the-double-click-casts",
        move |_| {
            told_apart
                && not_a_leftover
                && opened
                && reached
                && selected
                && cast_nothing
                && reached_twice
                && casts
        },
    );
}

#[test]
fn scenario_every_shipped_magic_key_raises_its_own_instruction() {
    scenario("every_shipped_magic_key_raises_its_own_instruction");
}

#[test]
fn scenario_the_instructions_move_the_selection_and_the_cast_keys_cast() {
    scenario("the_instructions_move_the_selection_and_the_cast_keys_cast");
}

#[test]
fn scenario_the_tab_instructions_walk_the_eight_banks() {
    scenario("the_tab_instructions_walk_the_eight_banks");
}

#[test]
fn scenario_the_frames_own_pass_delivers_what_was_queued_in_order() {
    scenario("the_frames_own_pass_delivers_what_was_queued_in_order");
}

#[test]
fn scenario_a_real_key_press_moves_the_selection_the_ring_and_the_scroll() {
    scenario("a_real_key_press_moves_the_selection_the_ring_and_the_scroll");
}

#[test]
fn scenario_one_click_selects_and_only_the_double_click_casts() {
    scenario("one_click_selects_and_only_the_double_click_casts");
}

/// The casting bar on the shipped tree, the shipped keymap over it, and one key or one pointer
/// through the client's own stack.
mod bar {
    use std::collections::BTreeMap;
    use std::rc::Rc;
    use std::sync::Arc;

    use dereth_client::input::InputShell;
    use dereth_client::interaction::{action as ia, Interaction};
    use dereth_input::fire::ControlType;
    use dereth_input::spec::ControlChord;
    use dereth_input::{ActionId, InputMapId};
    use dereth_primitives::{AssetSource, DataId, LocalTime, ObjectId};
    use dereth_ui::{Delivery, ElemHandle, ElementMessage, Screen as _, UiSystem};
    use dereth_ui_screens::screens::gameplay::GamePlayScreen;
    use dereth_ui_screens::view::{CombatBar, GameView, MagicNotice, PlayerOption, SpellEntry};

    const PLAYER: ObjectId = ObjectId(0x5410_1002);
    const MONSTER: ObjectId = ObjectId(0x8410_1777);

    #[derive(Debug)]
    struct Store(Arc<dereth_dat::RetailDatStore>);

    impl AssetSource for Store {
        fn read(&self, id: DataId) -> Result<Vec<u8>, dereth_primitives::AssetError> {
            self.0.read(id)
        }
        fn exists(&self, id: DataId) -> bool {
            self.0.exists(id)
        }
        fn iter_type(
            &self,
            kind: dereth_primitives::DataType,
        ) -> Box<dyn Iterator<Item = DataId> + '_> {
            self.0.iter_type(kind)
        }
    }

    /// The shipped gameplay tree, built by the screen's own startup.
    ///
    /// **`combat.rs` keeps a second copy of this**, because each subject owns its own scenario
    /// file; promoting it into `dereth-testkit`'s own `src/` would remove the duplicate.
    pub fn gameplay() -> (UiSystem, GamePlayScreen) {
        let store = Arc::new(dereth_dat::testing::open_store_or_fail());
        let master_id = DataId(0x3900_0001);
        let master = <dereth_assets::MasterProperty as dereth_assets::Decode>::decode_payload(
            master_id,
            &store.read(master_id).expect("the shipped property table"),
        )
        .expect("it decodes");
        let mut ui = UiSystem::new((800, 600));
        ui.property_types = master.property_types();
        let mut flow = dereth_ui::UiFlow::new();
        dereth_ui_screens::register_all(&mut ui, &mut flow);
        let assets = Rc::new(Store(store));
        let resolver = Rc::new(
            dereth_ui::framework::DidMapperResolver::load_via_master(assets.as_ref())
                .expect("the shipped mapper"),
        );
        dereth_ui_screens::env::install(&mut ui, assets, resolver);
        let mut s = GamePlayScreen::default();
        s.create(&mut dereth_ui::framework::ScreenCx::new(&mut ui))
            .expect("the gameplay screen builds from the shipped layout");
        pump(&mut ui);
        ui.requests.clear();
        ui.notice_inbox.clear();
        (ui, s)
    }

    /// Drain the tree's own delivery queue.
    pub fn pump(ui: &mut UiSystem) {
        for _ in 0..8 {
            if ui.drain_outbox().is_empty() {
                return;
            }
        }
    }

    /// The same, delivering what it drains to the screen -- which is what the flow does once a
    /// frame.
    pub fn pump_to(ui: &mut UiSystem, screen: &mut GamePlayScreen) {
        for _ in 0..8 {
            let batch = ui.drain_outbox();
            if batch.is_empty() {
                return;
            }
            for d in &batch {
                if let Delivery::Element { msg, .. } = d {
                    screen.on_element_message(&mut dereth_ui::framework::ScreenCx::new(ui), msg);
                }
            }
        }
    }

    /// One press, hit-tested and broadcast, then delivered to the screen. Answers what the
    /// screen's own filter forwarded to the panels.
    pub fn press_through(
        ui: &mut UiSystem,
        screen: &mut GamePlayScreen,
        action: u32,
        at: (i32, i32),
    ) -> Vec<ElementMessage> {
        ui.mouse_down(action, at.0, at.1);
        pump_to(ui, screen);
        screen.take_panel_messages()
    }

    /// Show everything between the screen's root and `target` that is shut, with the call the
    /// toolbar would make, and answer how many that was.
    pub fn open_the_window_over(ui: &mut UiSystem, target: ElemHandle) -> usize {
        let mut chain = Vec::new();
        let mut h = Some(target);
        while let Some(c) = h {
            chain.push(c);
            h = ui.parent(c);
        }
        let mut opened = 0;
        for c in chain.into_iter().rev() {
            if !ui.node(c).is_some_and(|n| n.region.flags.visible) {
                ui.set_visible(c, true);
                opened += 1;
            }
        }
        opened
    }

    /// The client's own input shell over the shipped tables.
    pub fn shell() -> InputShell {
        let store = dereth_dat::testing::open_store_or_fail();
        InputShell::new(&store, None).expect("the shipped input tables decode")
    }

    /// Every `(action, control)` the shipped keymap carries for one section.
    pub fn shipped_bindings(s: &InputShell, map: InputMapId) -> Vec<(ActionId, ControlChord)> {
        s.manager
            .keymap
            .section(map)
            .map(|sec| sec.bindings().iter().map(|(q, a)| (*a, *q)).collect())
            .unwrap_or_default()
    }

    /// The one control the shipped keymap binds `action` to in `map`.
    pub fn the_shipped_control(s: &InputShell, map: InputMapId, action: u32) -> ControlChord {
        let mut found: Vec<ControlChord> = shipped_bindings(s, map)
            .into_iter()
            .filter(|(a, _)| a.0 == action)
            .map(|(_, q)| q)
            .collect();
        found.dedup();
        assert_eq!(
            found.len(),
            1,
            "{action:#010X} has one shipped default control in {map:?}"
        );
        found[0]
    }

    /// Which instruction a key stands for, as the client's own arm decides it.
    pub fn the_instruction_for(action: u32) -> MagicNotice {
        match action {
            ia::COMBAT_CAST_CURRENT_SPELL => MagicNotice::CastCurrentSpell,
            ia::COMBAT_PREV_SPELL => MagicNotice::PrevSpellSelection,
            ia::COMBAT_NEXT_SPELL => MagicNotice::NextSpellSelection,
            ia::COMBAT_PREV_SPELL_TAB => MagicNotice::PrevSpellTab,
            ia::COMBAT_NEXT_SPELL_TAB => MagicNotice::NextSpellTab,
            ia::COMBAT_FIRST_SPELL => MagicNotice::FirstSpellSelection,
            ia::COMBAT_LAST_SPELL => MagicNotice::LastSpellSelection,
            ia::COMBAT_FIRST_SPELL_TAB => MagicNotice::FirstSpellTab,
            ia::COMBAT_LAST_SPELL_TAB => MagicNotice::LastSpellTab,
            a => MagicNotice::CastQuickslotSpell {
                slot: usize::try_from(a - ia::USE_SPELL_SLOT_FIRST).expect("a small slot"),
            },
        }
    }

    /// The release event the input layer would build. The shipped casting defaults are one-shots
    /// and produce none, so the arm is asked the question directly.
    pub fn released(action: ActionId) -> dereth_client_runtime::actions::Action {
        dereth_client_runtime::actions::Action {
            id: action,
            phase: dereth_client_runtime::actions::ActionPhase::End,
            extent: 1.0,
            repeats: 1,
        }
    }

    /// One key press through the client's own input shell, drained through its own frame.
    pub struct Driver {
        /// Well clear of the double-click window and the button history, so two presses of one
        /// key are never read as a gesture of each other.
        clock: u32,
    }

    impl Driver {
        pub const fn new() -> Self {
            Self { clock: 1_000 }
        }

        /// A press and its release, with any modifiers held across both.
        pub fn press_release(
            &mut self,
            shell: &mut InputShell,
            qc: &ControlChord,
        ) -> (
            Vec<dereth_client_runtime::actions::Action>,
            Vec<dereth_client_runtime::actions::Action>,
        ) {
            let metas: Vec<_> = shell
                .manager
                .keymap
                .meta_keys
                .iter()
                .filter(|(_, bit)| qc.meta_mode & bit != 0)
                .map(|(cs, _)| *cs)
                .collect();
            self.clock += 6_000;
            let mut t = self.clock;
            for cs in &metas {
                shell
                    .manager
                    .fire_input_event(*cs, ControlType::Button, 0x80, t);
                t += 10;
            }
            shell.use_time(LocalTime(f64::from(t) / 1000.0));
            shell.take_events();

            shell
                .manager
                .fire_input_event(qc.control, ControlType::Button, 0x80, t);
            shell.use_time(LocalTime(f64::from(t) / 1000.0));
            let down = shell.take_events();

            t += 10;
            shell
                .manager
                .fire_input_event(qc.control, ControlType::Button, 0, t);
            shell.use_time(LocalTime(f64::from(t) / 1000.0));
            let up = shell.take_events();

            for cs in metas.iter().rev() {
                t += 10;
                shell
                    .manager
                    .fire_input_event(*cs, ControlType::Button, 0, t);
            }
            shell.use_time(LocalTime(f64::from(t) / 1000.0));
            shell.take_events();
            self.clock = t;
            let hand_on = |events: Vec<dereth_input::InputEvent>| {
                events
                    .iter()
                    .map(dereth_input::InputEvent::to_action)
                    .collect()
            };
            (hand_on(down), hand_on(up))
        }
    }

    /// The client's own interaction slot over a character standing in the casting stance.
    pub struct Bench {
        inter: Interaction,
        objects: dereth_client::objects::ObjectStream,
        store: Arc<dereth_dat::RetailDatStore>,
    }

    impl Bench {
        pub fn new() -> Self {
            let mut objects = dereth_client::objects::ObjectStream::default();
            objects.world = a_world();
            Self {
                inter: Interaction::new(),
                objects,
                store: Arc::new(dereth_dat::testing::open_store_or_fail()),
            }
        }

        /// The magic notices the interaction layer raised and has not handed to a UI.
        pub fn take_notices(&mut self) -> Vec<dereth_ui_screens::view::MagicNotice> {
            self.inter.magic_notices.take()
        }

        pub fn drive(&mut self, actions: Vec<dereth_client_runtime::actions::Action>, now: f64) {
            let _ = dereth_client::interaction::use_time(
                &mut self.inter,
                &self.store,
                None,
                &mut self.objects,
                None,
                actions,
                false,
                (800, 600),
                LocalTime(now),
            );
        }

        pub const fn magic_actions(&self) -> u64 {
            self.inter.stats.magic_actions
        }
    }

    fn a_world() -> dereth_client_model::World {
        let mut w = dereth_client_model::World::new();
        w.player = Some(PLAYER);
        w.tables
            .weenies
            .insert(PLAYER, dereth_client_model::Weenie::new(PLAYER));
        w.tables
            .weenies
            .get_mut(PLAYER)
            .expect("just inserted")
            .pwd
            .name = "Aldis".into();
        w.tables.inventories.insert(
            PLAYER,
            dereth_client_model::objects::ObjectInventory::new(PLAYER),
        );
        let mut m = dereth_client_model::Weenie::new(MONSTER);
        m.pwd.name = "Mosswart".into();
        m.pwd.obj_type = dereth_client_model::weenie::item_type::CREATURE;
        m.pwd.bitfield |= dereth_client_model::weenie::bitfield::ATTACKABLE;
        w.tables.weenies.insert(MONSTER, m);
        w.set_selected_object(Some(MONSTER), false, &mut dereth_client_model::NullSink);
        w.combat.combat_mode = dereth_client_model::combat::CombatMode::Magic;
        w.tables
            .weenies
            .get_mut(PLAYER)
            .expect("the player")
            .qualities
            .get_or_insert_with(dereth_client_model::Qualities::new)
            .set(
                dereth_client_model::qualities::StatKey::new(
                    dereth_client_model::qualities::StatType::Did,
                    dereth_client_model::combat::COMBAT_TABLE_DID,
                ),
                dereth_client_model::qualities::StatValue::Did(DataId(0x3000_0000)),
            );
        w
    }

    /// A view that answers only what these scenarios read.
    #[derive(Debug, Default)]
    pub struct StubView {
        pub spells: Vec<u32>,
        pub spell_entries: Vec<SpellEntry>,
        pub options: BTreeMap<u32, bool>,
        pub bar: Option<CombatBar>,
    }

    impl StubView {
        pub fn with_spells(spells: Vec<u32>) -> Self {
            let spell_entries = spells
                .iter()
                .map(|id| SpellEntry {
                    id: *id,
                    name: format!("Spell {id}"),
                    icon: Some(DataId(0x0600_13A5)),
                    school: 4,
                    level: 1,
                    icon_power: 1,
                    display_order: i32::try_from(*id).expect("a small id"),
                    bitfield: 0,
                })
                .collect();
            Self {
                spells,
                spell_entries,
                ..Self::default()
            }
        }
    }

    impl GameView for StubView {
        fn spellbook(&self) -> &[SpellEntry] {
            &self.spell_entries
        }
        fn spell_tab(&self, _tab: usize) -> &[u32] {
            &self.spells
        }
        fn player_option(&self, o: PlayerOption) -> bool {
            self.options.get(&(o as u32)).copied().unwrap_or(false)
        }
        fn combat_bar(&self) -> CombatBar {
            self.bar.unwrap_or_default()
        }
    }
}
