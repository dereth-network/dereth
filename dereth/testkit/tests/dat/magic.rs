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

dereth_testkit::scenarios! {
    scenario_a_spellbook_click_reveals_and_selects => a_spellbook_click_reveals_and_selects ["spellbook.click.reveals-and-selects-the-row"],
    scenario_a_removed_spell_leaves_the_book_the_bar_and_the_copy => a_removed_spell_leaves_the_book_the_bar_and_the_copy ["spellbook.removal.reaches-the-book-the-bar-and-the-copy-a-relog-reads"],
    scenario_the_bar_keeps_a_spell_the_table_cannot_draw => the_bar_keeps_a_spell_the_table_cannot_draw ["spellbook.the-bar-keeps-a-spell-the-shipped-table-cannot-draw"],
    scenario_a_timed_buff_shows_its_real_remaining_time_across_a_relog => a_timed_buff_shows_its_real_remaining_time_across_a_relog ["enchantments.duration.a-timed-buff-shows-its-real-remaining-time-across-a-relog"],
    scenario_the_row_survives_its_own_duration_running_out => the_row_survives_its_own_duration_running_out ["enchantments.expiry.the-row-stays-until-the-shard-takes-it-and-not-when-its-time-runs-out"],
    scenario_the_shards_removal_empties_the_pane_and_the_skill => the_shards_removal_empties_the_pane_and_the_skill ["enchantments.removal.takes-the-row-out-of-the-pane-the-skill-back-to-base-and-says-so"],
    scenario_a_dispel_empties_the_same_panels_without_a_line => a_dispel_empties_the_same_panels_without_a_line ["enchantments.dispel.empties-the-same-two-panels-and-says-nothing"],
    scenario_a_second_buff_reaches_an_already_open_pane => a_second_buff_reaches_an_already_open_pane ["enchantments.pane.a-second-buff-reaches-a-pane-that-is-already-open"],
    scenario_a_recorded_resist_is_drawn_verbatim_on_the_magic_channel => a_recorded_resist_is_drawn_verbatim_on_the_magic_channel ["magic.resist.the-shards-own-sentence-is-drawn-verbatim-on-the-magic-channel"],
    scenario_a_squelched_magic_channel_drops_the_same_bytes => a_squelched_magic_channel_drops_the_same_bytes ["magic.resist.a-squelched-magic-channel-drops-the-same-recorded-bytes"],
    scenario_the_resist_sound_reaches_the_clients_queue => the_resist_sound_reaches_the_clients_queue ["magic.resist.the-sound-that-comes-with-it-reaches-the-clients-own-queue"],
    scenario_a_fizzle_ends_the_cast_prints_its_line_and_burns_nothing => a_fizzle_ends_the_cast_prints_its_line_and_burns_nothing ["magic.cast.a-fizzle-ends-the-cast-prints-its-line-and-burns-nothing"],
    scenario_a_recorded_fizzle_prints_the_line_without_ending_the_cast => a_recorded_fizzle_prints_the_line_without_ending_the_cast ["magic.cast.the-shards-recorded-fizzle-prints-the-same-line-and-does-not-end-the-cast"],
    scenario_a_refusal_the_client_makes_itself_never_reaches_the_shard => a_refusal_the_client_makes_itself_never_reaches_the_shard ["magic.cast.a-refusal-the-client-makes-itself-is-said-in-its-own-words-and-never-sent"],
    scenario_a_live_vitae_lights_the_lamp_and_fills_the_panel => a_live_vitae_lights_the_lamp_and_fills_the_panel ["vitae.lamp.lights-for-a-live-penalty-and-the-panel-says-how-much"],
    scenario_the_panel_follows_every_tick_and_goes_dark_at_full_strength => the_panel_follows_every_tick_and_goes_dark_at_full_strength ["vitae.panel.follows-every-tick-and-goes-dark-at-full-strength"],
    scenario_the_penalty_takes_the_skill_down_but_leaves_the_row_plain => the_penalty_takes_the_skill_down_but_leaves_the_row_plain ["vitae.penalty.takes-the-skill-down-leaves-the-row-plain-and-spares-the-attributes"],
    scenario_a_secondary_click_on_a_spell_opens_its_description => a_secondary_click_on_a_spell_opens_its_description ["spell-examine.pane.a-secondary-click-on-a-known-spell-opens-its-description"],
    scenario_the_primary_click_selects_the_row_and_examines_nothing => the_primary_click_selects_the_row_and_examines_nothing ["spell-examine.pane.the-primary-click-selects-the-row-and-examines-nothing"],
    scenario_the_secondary_click_moves_the_books_own_selection_too => the_secondary_click_moves_the_books_own_selection_too ["spell-examine.pane.the-secondary-click-moves-the-books-own-selection-too"],
    scenario_an_enchantment_shows_how_long_it_lasts_and_no_range => an_enchantment_shows_how_long_it_lasts_and_no_range ["spell-examine.pane.an-enchantment-shows-how-long-it-lasts-and-no-range"],
    scenario_a_second_look_re_opens_the_window_the_close_control_shut => a_second_look_re_opens_the_window_the_close_control_shut ["spell-examine.pane.a-second-look-re-opens-the-window-the-close-control-shut"],
    scenario_the_same_look_from_the_cast_bar_opens_it_without_selecting => the_same_look_from_the_cast_bar_opens_it_without_selecting ["spell-examine.pane.the-same-look-from-the-cast-bar-opens-it-without-selecting"],
    scenario_a_look_cancels_an_appraisal_in_flight_and_only_the_first_of_two_does => a_look_cancels_an_appraisal_in_flight_and_only_the_first_of_two_does ["spell-examine.cancel.a-look-cancels-an-appraisal-in-flight-and-only-the-first-of-two-does"],
    scenario_a_look_with_nothing_in_flight_asks_the_shard_nothing => a_look_with_nothing_in_flight_asks_the_shard_nothing ["spell-examine.cancel.a-look-with-nothing-in-flight-asks-the-shard-nothing"],
    scenario_the_account_the_shard_names_is_the_one_the_client_keeps => the_account_the_shard_names_is_the_one_the_client_keeps ["spell-examine.formula.the-account-the-shard-names-is-the-one-the-client-keeps"],
    scenario_the_formula_the_client_would_cast_follows_that_account => the_formula_the_client_would_cast_follows_that_account ["spell-examine.formula.the-formula-the-client-would-cast-follows-that-account"],
    scenario_the_look_lists_the_tapers_this_account_must_carry => the_look_lists_the_tapers_this_account_must_carry ["spell-examine.formula.the-look-lists-the-tapers-this-account-must-carry"],
    scenario_two_accounts_are_shown_different_tapers_for_one_spell => two_accounts_are_shown_different_tapers_for_one_spell ["spell-examine.formula.two-accounts-are-shown-different-tapers-for-one-spell"],
    scenario_the_shipped_foci_table_reaches_the_game_model_with_the_description => the_shipped_foci_table_reaches_the_game_model_with_the_description ["spell-examine.formula.the-shipped-foci-table-reaches-the-game-model-with-the-description"],
    scenario_carrying_a_foci_changes_what_the_client_would_spend => carrying_a_foci_changes_what_the_client_would_spend ["spell-examine.formula.carrying-a-foci-changes-what-the-client-would-spend"],
    scenario_a_foci_for_another_school_leaves_the_long_formula_alone => a_foci_for_another_school_leaves_the_long_formula_alone ["spell-examine.formula.a-foci-for-another-school-leaves-the-long-formula-alone"],
    scenario_the_look_with_a_foci_lists_a_scarab_and_four_tapers => the_look_with_a_foci_lists_a_scarab_and_four_tapers ["spell-examine.formula.the-look-with-a-foci-lists-a-scarab-and-four-tapers"],
    scenario_the_look_without_a_foci_lists_the_long_per_account_formula => the_look_without_a_foci_lists_the_long_per_account_formula ["spell-examine.formula.the-look-without-a-foci-lists-the-long-per-account-formula"],
    scenario_a_click_on_a_component_icon_selects_the_one_in_the_pack => a_click_on_a_component_icon_selects_the_one_in_the_pack ["spell-examine.components.a-click-on-an-icon-selects-the-one-in-the-pack"],
    scenario_each_component_icon_names_its_own_slot => each_component_icon_names_its_own_slot ["spell-examine.components.each-icon-names-its-own-slot"],
    scenario_a_component_the_player_does_not_carry_selects_nothing => a_component_the_player_does_not_carry_selects_nothing ["spell-examine.components.one-the-player-does-not-carry-selects-nothing"],
    scenario_the_secondary_click_does_nothing_on_a_component_icon => the_secondary_click_does_nothing_on_a_component_icon ["spell-examine.components.the-secondary-click-does-nothing-on-an-icon"],
    scenario_the_component_a_slot_stands_for_is_a_representative_of_its_kind => the_component_a_slot_stands_for_is_a_representative_of_its_kind ["spell-examine.components.the-answer-is-a-representative-of-the-class-and-not-one-object"],
    scenario_the_components_the_player_lacks_are_the_ones_that_are_marked => the_components_the_player_lacks_are_the_ones_that_are_marked ["spell-examine.marks.the-components-the-player-lacks-are-the-ones-that-are-marked"],
    scenario_everything_carried_marks_nothing_and_nothing_carried_marks_everything => everything_carried_marks_nothing_and_nothing_carried_marks_everything ["spell-examine.marks.everything-carried-marks-nothing-and-nothing-carried-marks-everything"],
    scenario_a_component_arriving_clears_its_mark_and_one_leaving_brings_it_back => a_component_arriving_clears_its_mark_and_one_leaving_brings_it_back ["spell-examine.marks.a-component-arriving-clears-its-mark-and-one-leaving-brings-it-back"],
    scenario_a_closed_window_still_re_marks_its_rows => a_closed_window_still_re_marks_its_rows ["spell-examine.marks.a-closed-window-still-re-marks-its-rows"],
    scenario_a_second_of_the_same_kind_keeps_the_mark_off => a_second_of_the_same_kind_keeps_the_mark_off ["spell-examine.marks.a-second-of-the-same-kind-keeps-the-mark-off"],
    scenario_the_notice_moves_for_a_component_and_for_nothing_else => the_notice_moves_for_a_component_and_for_nothing_else ["spell-examine.marks.the-notice-moves-for-a-component-and-for-nothing-else"],
    scenario_a_component_row_shows_how_many_are_held_and_goes_away_at_none => a_component_row_shows_how_many_are_held_and_goes_away_at_none ["spell-components.strip.a-row-shows-how-many-are-held-and-goes-away-at-none"],
    scenario_the_component_page_rebuilds_on_the_pack_and_not_every_frame => the_component_page_rebuilds_on_the_pack_and_not_every_frame ["spell-components.strip.it-rebuilds-when-the-pack-changes-and-not-every-frame"],
    scenario_a_header_is_drawn_only_for_a_kind_the_player_holds_something_of => a_header_is_drawn_only_for_a_kind_the_player_holds_something_of ["spell-components.strip.a-header-is-drawn-only-for-a-kind-the-player-holds-something-of"],
    scenario_the_component_walk_is_kind_order_with_each_kinds_rows_under_its_header => the_component_walk_is_kind_order_with_each_kinds_rows_under_its_header ["spell-components.strip.the-walk-is-kind-order-with-each-kinds-rows-under-its-header"],
    scenario_the_component_row_icon_is_the_shipped_tables_and_not_the_objects => the_component_row_icon_is_the_shipped_tables_and_not_the_objects ["spell-components.strip.the-icon-is-the-shipped-tables-and-not-the-objects"],
    scenario_an_identical_spellbook_frame_does_not_rebuild_and_a_changed_list_does => an_identical_spellbook_frame_does_not_rebuild_and_a_changed_list_does ["spellbook.redraw.an-identical-frame-does-not-rebuild-and-a-changed-spell-list-does"],
    scenario_a_spellbook_badge_follows_a_spells_own_flags => a_spellbook_badge_follows_a_spells_own_flags ["spellbook.redraw.a-badge-follows-a-spells-own-flags-under-an-unchanged-list"],
    scenario_the_spellbook_rows_re_sort_when_a_spells_place_in_the_order_moves => the_spellbook_rows_re_sort_when_a_spells_place_in_the_order_moves ["spellbook.redraw.the-rows-re-sort-when-a-spells-place-in-the-order-moves"],
    scenario_the_components_page_is_the_census_of_what_the_player_carries => the_components_page_is_the_census_of_what_the_player_carries ["spell-components.strip.the-list-is-the-census-of-what-the-player-carries"],
    scenario_the_held_count_follows_a_stack_size_the_shard_changes => the_held_count_follows_a_stack_size_the_shard_changes ["spell-components.strip.the-held-count-follows-a-stack-size-the-shard-changes"],
    scenario_a_click_on_a_component_row_selects_that_component_in_the_world => a_click_on_a_component_row_selects_that_component_in_the_world ["spell-components.strip.a-click-on-a-row-selects-that-component-in-the-world"],
    scenario_a_click_on_a_header_clears_the_selection_and_the_first_one_does_nothing => a_click_on_a_header_clears_the_selection_and_the_first_one_does_nothing ["spell-components.strip.a-click-on-a-header-clears-the-selection-and-the-first-one-does-nothing"],
    scenario_selecting_a_component_in_the_world_highlights_its_row => selecting_a_component_in_the_world_highlights_its_row ["spell-components.strip.selecting-a-component-in-the-world-highlights-its-row"],
    scenario_every_shipped_magic_key_raises_its_own_instruction => every_shipped_magic_key_raises_its_own_instruction ["spellbar.keys.every-shipped-magic-key-raises-its-own-instruction-and-the-release-raises-none"],
    scenario_the_instructions_move_the_selection_and_the_cast_keys_cast => the_instructions_move_the_selection_and_the_cast_keys_cast ["spellbar.keys.the-instructions-move-the-selection-and-the-cast-keys-cast"],
    scenario_the_tab_instructions_walk_the_eight_banks => the_tab_instructions_walk_the_eight_banks ["spellbar.tabs.the-tab-instructions-walk-the-eight-banks-and-stop-at-the-ends"],
    scenario_the_frames_own_pass_delivers_what_was_queued_in_order => the_frames_own_pass_delivers_what_was_queued_in_order ["spellbar.keys.the-frames-own-pass-delivers-what-was-queued-in-order"],
    scenario_a_real_key_press_moves_the_selection_the_ring_and_the_scroll => a_real_key_press_moves_the_selection_the_ring_and_the_scroll ["spellbar.keys.a-real-key-press-moves-the-selection-the-ring-and-the-scroll"],
    scenario_one_click_selects_and_only_the_double_click_casts => one_click_selects_and_only_the_double_click_casts ["spellbar.click.one-click-selects-and-only-the-double-click-casts"],
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

mod support;

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

/// The bench the three casting scenarios share: a whole client whose character carries the whole
/// of one spell's formula, with a legal target and an illegal one in the world.
mod cast;

/// The fixture the three vitae scenarios share.
mod vitae;

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

/// The spell-examine family's own setup: the shipped spellbook page, the secondary click, and the
/// pane's facts as one owned value an assertion closure can hold.
mod examine;

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

/// The components page of the spell book, and what it drew.
mod strip;

/// The spellbook panel, bound off the shipped tree and driven with a book of its own.
mod book;

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

/// The recorded pack, the Components tab, and the list's own items.
mod census;

/// The casting bar on the shipped tree, the shipped keymap over it, and one key or one pointer
/// through the client's own stack.
mod bar;

mod spellbook;
use spellbook::{
    a_removed_spell_leaves_the_book_the_bar_and_the_copy,
    a_spellbook_badge_follows_a_spells_own_flags, a_spellbook_click_reveals_and_selects,
    an_identical_spellbook_frame_does_not_rebuild_and_a_changed_list_does,
    the_bar_keeps_a_spell_the_table_cannot_draw,
    the_spellbook_rows_re_sort_when_a_spells_place_in_the_order_moves,
};

mod enchantments;
use enchantments::{
    a_dispel_empties_the_same_panels_without_a_line, a_second_buff_reaches_an_already_open_pane,
    a_timed_buff_shows_its_real_remaining_time_across_a_relog,
    the_row_survives_its_own_duration_running_out,
    the_shards_removal_empties_the_pane_and_the_skill,
};

mod cast_feedback;
use cast_feedback::{
    a_fizzle_ends_the_cast_prints_its_line_and_burns_nothing,
    a_recorded_fizzle_prints_the_line_without_ending_the_cast,
    a_recorded_resist_is_drawn_verbatim_on_the_magic_channel,
    a_refusal_the_client_makes_itself_never_reaches_the_shard,
    a_squelched_magic_channel_drops_the_same_bytes, the_resist_sound_reaches_the_clients_queue,
};

mod vitae_cases;
use vitae_cases::{
    a_live_vitae_lights_the_lamp_and_fills_the_panel,
    the_panel_follows_every_tick_and_goes_dark_at_full_strength,
    the_penalty_takes_the_skill_down_but_leaves_the_row_plain,
};

mod examination;
use examination::{
    a_foci_for_another_school_leaves_the_long_formula_alone,
    a_look_cancels_an_appraisal_in_flight_and_only_the_first_of_two_does,
    a_look_with_nothing_in_flight_asks_the_shard_nothing,
    a_second_look_re_opens_the_window_the_close_control_shut,
    a_secondary_click_on_a_spell_opens_its_description,
    an_enchantment_shows_how_long_it_lasts_and_no_range,
    carrying_a_foci_changes_what_the_client_would_spend,
    the_account_the_shard_names_is_the_one_the_client_keeps,
    the_formula_the_client_would_cast_follows_that_account,
    the_look_lists_the_tapers_this_account_must_carry,
    the_look_with_a_foci_lists_a_scarab_and_four_tapers,
    the_look_without_a_foci_lists_the_long_per_account_formula,
    the_primary_click_selects_the_row_and_examines_nothing,
    the_same_look_from_the_cast_bar_opens_it_without_selecting,
    the_secondary_click_moves_the_books_own_selection_too,
    the_shipped_foci_table_reaches_the_game_model_with_the_description,
    two_accounts_are_shown_different_tapers_for_one_spell,
};

mod components;
use components::{
    a_click_on_a_component_icon_selects_the_one_in_the_pack,
    a_closed_window_still_re_marks_its_rows,
    a_component_arriving_clears_its_mark_and_one_leaving_brings_it_back,
    a_component_the_player_does_not_carry_selects_nothing,
    a_second_of_the_same_kind_keeps_the_mark_off, each_component_icon_names_its_own_slot,
    everything_carried_marks_nothing_and_nothing_carried_marks_everything,
    the_component_a_slot_stands_for_is_a_representative_of_its_kind,
    the_components_the_player_lacks_are_the_ones_that_are_marked,
    the_notice_moves_for_a_component_and_for_nothing_else,
    the_secondary_click_does_nothing_on_a_component_icon,
};

mod component_rows;
use component_rows::{
    a_component_row_shows_how_many_are_held_and_goes_away_at_none,
    a_header_is_drawn_only_for_a_kind_the_player_holds_something_of,
    the_component_page_rebuilds_on_the_pack_and_not_every_frame,
    the_component_row_icon_is_the_shipped_tables_and_not_the_objects,
    the_component_walk_is_kind_order_with_each_kinds_rows_under_its_header,
};

mod component_inventory;
use component_inventory::{
    a_click_on_a_component_row_selects_that_component_in_the_world,
    a_click_on_a_header_clears_the_selection_and_the_first_one_does_nothing,
    selecting_a_component_in_the_world_highlights_its_row,
    the_components_page_is_the_census_of_what_the_player_carries,
    the_held_count_follows_a_stack_size_the_shard_changes,
};

mod casting_bar;
use casting_bar::{
    a_real_key_press_moves_the_selection_the_ring_and_the_scroll,
    every_shipped_magic_key_raises_its_own_instruction,
    one_click_selects_and_only_the_double_click_casts,
    the_frames_own_pass_delivers_what_was_queued_in_order,
    the_instructions_move_the_selection_and_the_cast_keys_cast,
    the_tab_instructions_walk_the_eight_banks,
};
