//! The panels -- the character pages, the appraisal panes, the journal and the house pane.
//!
//! Nearly every one of these claims is a number, a word or a colour the shipped element tree
//! carries after something happened. The scenarios behind these rows read that tree through
//! [`crate::UiSnapshot`](crate::ui_snapshot::UiSnapshot) and the two glyph readers
//! `tests/dat/panels.rs` keeps beside it.
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
        id: "abuse.report.the-page-sends-one-report-naming-who-and-why-and-then-empties-itself",
        says: "Filling the abuse window in the way a player does -- taking the name of whoever is \
               selected, typing the complaint, and pressing Continue once both are filled -- sends \
               exactly one report, carrying that name and those words and nothing else, and sends \
               nothing at all before the press. The window then says it is waiting until the shard \
               answers, the answer replaces that line, and pressing Done empties both fields, puts \
               the window back on its first page, puts Continue out of reach again and sends \
               nothing more.",
        since: RETAIL,
        evidence: Evidence::Private("AC-EVID-P1-41-ABUSE-REPORT"),
        station: "dereth-testkit::dat::panels::scenario_the_abuse_page_sends_one_report_and_then_empties_itself",
        tier: Tier::Dat,
    },
    behaviour! {
        id: "abuse.response.the-shards-answer-writes-the-result-line-and-nothing-in-chat",
        says: "When the shard answers a report of another player -- no such character, you cannot \
               report yourself, or thank you -- the abuse window's result line becomes the shipped \
               sentence for that answer and not one word of it is said in the chat window. An \
               answer the window has no sentence for leaves the line exactly as it was rather than \
               blanking it. The line is written whether the window is open or shut and the window \
               is neither opened nor closed by it, so the last answer is waiting there when the \
               player opens it again.",
        since: RETAIL,
        evidence: Evidence::Private("AC-EVID-P1-41-ABUSE-REPLY"),
        station: "dereth-testkit::dat::panels::scenario_the_shards_abuse_answer_writes_the_result_line_and_nothing_in_chat",
        tier: Tier::Dat,
    },
    behaviour! {
        id: "advancement.answer.an-out-of-order-or-replayed-answer-is-refused",
        says: "When the shard's answer about a skill arrives again after a newer one, as a replay \
               of a recorded answer from earlier in the same session, the client ignores it and \
               the skill stays at the value the newest answer gave.",
        since: RETAIL,
        evidence: Evidence::Private("AC-EVID-O77-ANSWER"),
        station: "dereth-client::dat::panels::skill_advancement::a_replayed_captured_answer_is_refused_as_stale",
        tier: Tier::Dat,
    },
    behaviour! {
        id: "advancement.cost.the-plus-ten-button-is-affordable-when-the-unassigned-experience-covers-it",
        says: "The button that raises an attribute ten points at once lights up exactly when the \
               experience the player has not spent yet covers what those ten points cost, and the \
               same answer comes out under an optimised build as under an unoptimised one.",
        since: RETAIL,
        evidence: Evidence::Private("AC-EVID-P1-23D-ENABLE"),
        station: "dereth-testkit::dat::panels::scenario_the_plus_ten_button_lights_when_the_unassigned_experience_covers_it",
        tier: Tier::Dat,
    },
    behaviour! {
        id: "advancement.cost.the-ten-point-cost-comes-off-the-shipped-experience-table",
        says: "What ten points of an attribute or a skill cost is read off the experience table \
               the retail data ships, as the difference between where the player is and ten ranks \
               further on, and not off the table's last entry.",
        since: RETAIL,
        evidence: Evidence::Private("AC-EVID-P1-23D-TABLE"),
        station: "dereth-testkit::dat::panels::scenario_ten_points_cost_the_distance_between_two_entries_of_the_shipped_table",
        tier: Tier::Dat,
    },
    behaviour! {
        id: "advancement.raise.a-raise-sends-nothing-until-a-row-is-picked-and-then-names-it-and-the-cost",
        says: "The button that raises what is picked sends nothing while nothing is picked, and \
               picking a row is itself silent; once a trained skill is picked, one press asks the \
               shard for one raise, naming that skill and the amount of experience the footer said \
               the raise would cost.",
        since: RETAIL,
        evidence: Evidence::Private("AC-EVID-O422-RAISE-GATE"),
        station: "dereth-testkit::dat::panels::scenario_a_raise_sends_nothing_until_a_skill_row_is_picked",
        tier: Tier::Dat,
    },
    behaviour! {
        id: "advancement.raise.the-ten-point-button-lights-up-with-the-experience-and-a-press-sends-the-raise",
        says: "One point short of what ten raises cost, the ten-point button says it is out of \
               reach and pressing it sends nothing; with exactly that much unspent experience it \
               lights up, the pointer really lands on that page's own button rather than the other \
               page's, and one press asks for one raise naming that stat and that amount -- after \
               which the button puts itself back out of reach while it waits for an answer. The \
               same on an attribute, on a vital and on a trained skill.",
        since: RETAIL,
        evidence: Evidence::Private("AC-EVID-P1-23-PROBE"),
        station: "dereth-testkit::dat::panels::scenario_the_ten_point_button_lights_up_and_a_real_press_sends_the_raise",
        tier: Tier::Dat,
    },
    behaviour! {
        id: "advancement.train.the-train-request-reaches-the-wire",
        says: "Training an untrained skill sends one request naming the skill and the skill \
               credits it costs; raising an untrained skill, training one that is already trained, \
               or raising anything before the character's description has arrived sends nothing.",
        since: RETAIL,
        evidence: Evidence::Private("AC-EVID-O77-TRAIN"),
        station: "dereth-client::dat::panels::skill_advancement::the_train_request_is_0x0047_and_the_two_gates_refuse_each_others_work",
        tier: Tier::Dat,
    },
    behaviour! {
        id: "appraisal.presentation.variants-preserve-order-and-world-facts",
        says: "Appraisal preserves each interface's supported wording, property order and armor \
               headings while displaying the connected world's later properties and requirements \
               through shared facts.",
        since: RETAIL,
        evidence: Evidence::Private("AC-EVID-APPRAISAL-PRESENTATION"),
        station: "dereth-presentation::lib::appraisal::variant_tests::variants_keep_ordered_runs_and_later_world_facts",
        tier: Tier::Cpu,
    },
    behaviour! {
        id: "attributes.enlightenment.each-level-adds-two-to-maximum-health-and-nothing-else",
        says: "A character who carries an enlightenment count has twice that count added to \
               maximum health, and nothing added to maximum stamina or mana. It counts before an \
               enchantment that multiplies health, which scales it too, and a character who never \
               enlightened reads exactly as before.",
        since: RETAIL,
        evidence: Evidence::Private("AC-EVID-ATTRIBUTES-ENLIGHTENMENT"),
        station: "dereth-client-model::lib::attributes::tests::enlightenment_adds_twice_its_count_to_max_health_only",
        tier: Tier::Cpu,
    },
    behaviour! {
        id: "attributes.footer.selecting-a-row-fills-the-footer-with-its-own-numbers",
        says: "With nothing picked the attributes page's footer shows the unspent skill credits \
               and experience; picking any of the nine rows marks only that row as picked and \
               fills the footer with that row's own numbers, including what raising it costs, from \
               the recorded character.",
        since: RETAIL,
        evidence: Evidence::Private("AC-EVID-O78-FOOTER"),
        station: "dereth-client::dat::panels::attributes_panel::selecting_a_row_fills_the_footer_with_that_rows_own_numbers",
        tier: Tier::Dat,
    },
    behaviour! {
        id: "attributes.header.shows-the-heritage-line-and-pk-status",
        says: "The attributes page's header reads the character's sex, heritage and chosen title \
               as one line, spelling three heritages the way the retail client always did, and \
               shows whether the character is a player killer from the character's own public \
               description.",
        since: RETAIL,
        evidence: Evidence::Private("AC-EVID-O78-HEADER"),
        station: "dereth-client::dat::panels::attributes_panel::the_header_shows_the_captures_own_heritage_line_and_pk_status",
        tier: Tier::Dat,
    },
    behaviour! {
        id: "attributes.rows.a-buffed-value-is-green-a-debuffed-one-red-and-an-unmodified-one-white",
        says: "An attribute row shows the number the player actually has after every spell acting \
               on it, and draws it green when something raised it, red when something lowered it \
               and white when nothing touched it -- so a row with the right number in the wrong \
               colour is still wrong.",
        since: RETAIL,
        evidence: Evidence::Private("AC-EVID-P1-32"),
        station: "dereth-testkit::dat::panels::scenario_a_buffed_attribute_row_is_green_a_debuffed_one_red_and_the_rest_white",
        tier: Tier::Dat,
    },
    behaviour! {
        id: "attributes.rows.a-fractional-enchantment-rounds-the-way-the-panel-draws-it",
        says: "A spell whose effect is not a whole number is rounded to the nearest one before the \
               attribute and vital rows draw it, and re-casting the same spell over itself \
               refreshes the row rather than leaving the previous number on screen.",
        since: RETAIL,
        evidence: Evidence::Private("AC-EVID-P1-74B"),
        station: "dereth-testkit::dat::panels::scenario_a_fractional_enchantment_rounds_in_the_rows_the_player_reads",
        tier: Tier::Dat,
    },
    behaviour! {
        id: "attributes.rows.a-vital-row-is-current-over-maximum-and-takes-its-colour-from-the-maximum",
        says: "Health, Stamina and Mana are drawn as the current value over the maximum one, and \
               the colour follows what has been done to the maximum rather than to the current \
               value.",
        since: RETAIL,
        evidence: Evidence::Private("AC-EVID-P1-32-VITALS"),
        station: "dereth-testkit::dat::panels::scenario_a_vital_row_is_current_over_maximum_and_colours_by_the_maximum",
        tier: Tier::Dat,
    },
    behaviour! {
        id: "attributes.rows.every-row-draws-the-icon-its-own-group-names",
        says: "Each of the nine attribute and vital rows draws a picture beside its name, and it \
               is the one the shipped table names for that row's own stat within that row's own \
               group -- nine rows and nine different pictures, not one picture repeated down the \
               list and not a blank column.",
        since: RETAIL,
        evidence: Evidence::Private("AC-EVID-O422-ICONS"),
        station: "dereth-testkit::dat::panels::scenario_every_attribute_row_draws_the_icon_its_own_group_names",
        tier: Tier::Dat,
    },
    behaviour! {
        id: "attributes.selection.a-press-picks-the-row-under-the-pointer-through-the-list-itself",
        says: "Pressing an attribute row picks it: the pointer really lands on the list the rows \
               are drawn in rather than on the row, which is what the page reads the pointer's \
               position out of, and the row under it is the one that is picked. The picked row is \
               drawn as picked and every other row as not, and the panel's footer names it and its \
               number. Pressing the same row again un-picks it and brings the default footer back, \
               and pressing it once more picks it again. None of that sends anything to the shard.",
        since: RETAIL,
        evidence: Evidence::Private("AC-EVID-O422-ATTR-SELECT"),
        station: "dereth-testkit::dat::panels::scenario_a_press_picks_the_attribute_row_under_it_and_lands_on_the_list",
        tier: Tier::Dat,
    },
    behaviour! {
        id: "barber.face-choices.the-element-is-an-inert-frame-around-rows-the-panel-already-reaches",
        says: "The barber's face-choices area is a plain frame and not a list: it holds exactly \
               the five appearance rows the barber already drives, hair, eyes, nose, mouth and \
               skin, each a button, and nothing else to fill.",
        since: RETAIL,
        evidence: Evidence::Private("AC-EVID-P1175-FACE-CHOICES"),
        station: "dereth-ui-screens::dat::panels::barber_face_choices::the_face_choices_element_is_the_frame_around_the_five_part_rows",
        tier: Tier::Dat,
    },
    behaviour! {
        id: "character-sheet.augmentations.a-row-says-time-once-and-times-more-than-once",
        says: "An augmentation the character has taken is written out as one line saying how many \
               times it was taken, in the singular for one and the plural for more than one, with \
               none of the sheet's own markup for that choice left on the screen.",
        since: RETAIL,
        evidence: Evidence::Private("AC-EVID-P1-15-AUGROW"),
        station: "dereth-testkit::dat::panels::scenario_an_augmentation_row_says_time_once_and_times_more_than_once",
        tier: Tier::Dat,
    },
    behaviour! {
        id: "character-sheet.born.the-line-is-the-day-and-time-the-character-was-made",
        says: "The character sheet the burden lamp opens begins by saying the day and the time the \
               character was made, written in the short-date shape the game's own region setting \
               gives -- a date, a clock time and a morning or afternoon mark, with no weekday and \
               no month name in it.",
        since: RETAIL,
        evidence: Evidence::Private("AC-EVID-P1-15-BORN"),
        station: "dereth-testkit::dat::panels::scenario_the_born_line_is_the_day_and_time_the_character_was_made",
        tier: Tier::Dat,
    },
    behaviour! {
        id: "character-sheet.deaths.a-count-the-shard-clears-leaves-the-sheet",
        says: "A number of deaths the shard clears stops being shown: the line goes back to \
               reading exactly as it did before the shard ever sent one, and the sheet is really \
               written again for it rather than left showing old words out of a cache.",
        since: RETAIL,
        evidence: Evidence::Private("AC-EVID-P2-3-SHEET"),
        station: "dereth-testkit::dat::panels::scenario_a_removed_death_count_disappears_from_the_character_sheet",
        tier: Tier::Dat,
    },
    behaviour! {
        id: "character-sheet.luminance.the-heading-is-always-there-and-a-rating-over-five-splits-in-two",
        says: "The sheet's luminance heading is on every character's sheet whether or not anything \
               is listed under it; a rating of the kind that has a second, specialised half is \
               drawn as two lines, the first stopping at five and the second carrying whatever is \
               left over, while a rating with no such half is drawn whole and uncapped; and a kind \
               the character was granted none of contributes no line at all.",
        since: RETAIL,
        evidence: Evidence::Private("AC-EVID-P1-15-LUMINANCE"),
        station: "dereth-testkit::dat::panels::scenario_the_luminance_section_is_a_header_a_split_pair_and_an_unclamped_single",
        tier: Tier::Dat,
    },
    behaviour! {
        id: "character-sheet.mastery.the-lines-name-the-weapon-group-rather-than-the-number-that-picks-it",
        says: "The sheet names the kinds of weapon the character has mastery in by name rather \
               than by the number that picks them, reads the ranged line off a different list of \
               names than the melee one, and leaves the summoning line off a character who has no \
               summoning mastery -- putting it there, named, as soon as the shard grants one.",
        since: RETAIL,
        evidence: Evidence::Private("AC-EVID-P1-15-MASTERY"),
        station: "dereth-testkit::dat::panels::scenario_the_mastery_lines_name_the_weapon_group_rather_than_the_number",
        tier: Tier::Dat,
    },
    behaviour! {
        id: "character-sheet.played.the-line-spells-out-only-the-terms-that-are-not-zero",
        says: "The sheet says how long the character has been played for, writing out only the \
               terms that are not zero -- a character a quarter of a minute old reads fifteen \
               seconds and not a row of noughts -- each term singular or plural as its own number \
               needs, with none of the sheet's own markup on the screen; and a new figure arriving \
               from the shard while the sheet is open rewrites the sentence.",
        since: RETAIL,
        evidence: Evidence::Private("AC-EVID-P1-15-PLAYED"),
        station: "dereth-testkit::dat::panels::scenario_the_playtime_line_spells_out_only_the_terms_that_are_not_zero",
        tier: Tier::Dat,
    },
    behaviour! {
        id: "character-sheet.scroll.a-burdened-sheet-overflows-and-its-bar-scrolls-the-bottom-into-view",
        says: "When the character sheet opened from the burden lamp runs longer than its pane, \
               dragging its scrollbar's thumb to the bottom of the track brings the sheet's last \
               line into view.",
        since: RETAIL,
        evidence: Evidence::Private("AC-EVID-G31-SCROLL"),
        station: "dereth-ui-screens::dat::panels::character_sheet_scroll::a_real_drag_of_the_thumb_brings_the_bottom_of_the_character_sheet_into_view",
        tier: Tier::Dat,
    },
    behaviour! {
        id: "character-sheet.sections.the-six-parts-are-drawn-in-the-order-the-sheet-builds-them",
        says: "The sheet is six parts of one block of text and always in the same order: the \
               character's birth, playtime, deaths and enlightenment, then the natural \
               resistances, then the \
               innate attributes, then the chess and fishing lines, then the masteries and \
               augmentations, and the load last -- and that is the order on the screen and not \
               only the order they were gathered in.",
        since: RETAIL,
        evidence: Evidence::Private("AC-EVID-P1-15-ORDER"),
        station: "dereth-testkit::dat::panels::scenario_the_six_sections_of_the_sheet_are_in_the_order_the_composer_appends_them",
        tier: Tier::Dat,
    },
    behaviour! {
        id: "character.header.the-character-page-heads-with-the-name-the-level-and-the-experience",
        says: "Both halves of the character page head with the character's own name, the level \
               they have reached, the experience they have earned altogether and how much more \
               they need for the next level, with a bar drawn as far along as they are between \
               the two -- and each half carries its own copy of those lines rather than sharing \
               one, so a page that is showing the other tab is not showing a blank heading.",
        since: RETAIL,
        evidence: Evidence::Private("AC-EVID-O65-HEADER"),
        station: "dereth-testkit::dat::panels::scenario_both_character_pages_head_with_the_name_the_level_and_the_experience",
        tier: Tier::Dat,
    },
    behaviour! {
        id: "classic.chat-resize.moves-attached-panels",
        says: "Resizing the Classic chat height moves each attached panel without closing it or changing its width.",
        since: THIS_CLIENT,
        divergence: "CD-015",
        evidence: Evidence::Private("AC-EVID-UI-CLASSIC-CHAT-DOCK"),
        station: "dereth-classic-ui::lib::desktop::tests::dragging_the_chat_divider_moves_attached_windows_without_reopening_them",
        tier: Tier::Cpu,
    },
    behaviour! {
        id: "classic.inventory.hides-fitting-scrollbars",
        says: "Container and item scrollbars disappear when their capacity slots fit; shrinking or stretching the view clamps the visible origin.",
        since: THIS_CLIENT,
        divergence: "CD-015",
        evidence: Evidence::Private("AC-EVID-UI-CLASSIC-SCROLL-FIT"),
        station: "dereth-classic-ui::lib::panels::game::items::fitting_scroll_tests::inventory_bars_follow_capacity_and_stretch_and_clamp_the_visible_origin",
        tier: Tier::Cpu,
    },
    behaviour! {
        id: "classic.paper-doll.accessories-button-shows-what-is-behind-it",
        says: "The accessories button is lit while one of its slots holds an item, and marked while an item that fits only those slots is dragged.",
        since: THIS_CLIENT,
        divergence: "CD-033",
        evidence: Evidence::Private("AC-EVID-UI-CLASSIC-ACCESSORIES-BUTTON"),
        station: "dereth-classic-ui::lib::panels::game::items::accessories_tests::the_button_is_lit_while_a_hidden_slot_holds_an_item_and_marked_for_an_item_only_it_takes",
        tier: Tier::Cpu,
    },
    behaviour! {
        id: "classic.paper-doll.accessories-flyout-opens-and-closes",
        says: "The accessories button toggles a titled flyout standing on it over the doll's legs, the unlocked sigils in order over the cloak and the trinket; a press elsewhere on the panel (unless it starts a drag), its collapse arrow, Escape or the panel closing closes it, and while open it lies over the doll and takes the pointer and drops from what it covers.",
        since: THIS_CLIENT,
        divergence: "CD-033",
        evidence: Evidence::Private("AC-EVID-UI-CLASSIC-ACCESSORIES-TOGGLE"),
        station: "dereth-classic-ui::lib::panels::game::items::accessories_tests::the_button_toggles_the_flyout_and_a_press_elsewhere_or_escape_closes_it",
        tier: Tier::Cpu,
    },
    behaviour! {
        id: "classic.paper-doll.accessories-flyout-springs-open-under-a-held-item",
        says: "A dragged item held over the closed accessories button for 400 ms opens the flyout, which stays open while the drag is over the flyout, the button or the strip between them; leaving them without dropping, or the drag ending elsewhere, closes it again, unless it was open before the drag, and a drop in it keeps it open.",
        since: THIS_CLIENT,
        divergence: "CD-033",
        evidence: Evidence::Private("AC-EVID-UI-CLASSIC-ACCESSORIES-SPRING"),
        station: "dereth-classic-ui::lib::panels::game::items::accessories_tests::an_item_held_over_the_closed_button_opens_the_flyout_after_the_delay",
        tier: Tier::Cpu,
    },
    behaviour! {
        id: "classic.paper-doll.accessories-follow-the-world-and-the-unlocks",
        says: "The classic paper doll keeps the cloak, the trinket and the aetheria sigils behind one button; it holds the cloak and the trinket where the world has them and each sigil the character has unlocked, follows an unlock at once, and the button is absent when none applies.",
        since: THIS_CLIENT,
        divergence: "CD-033",
        evidence: Evidence::Private("AC-EVID-UI-CLASSIC-ACCESSORIES-SLOTS"),
        station: "dereth-classic-ui::lib::panels::game::items::accessories_tests::the_flyout_has_the_worlds_slots_and_each_sigil_the_character_has_unlocked",
        tier: Tier::Cpu,
    },
    behaviour! {
        id: "classic.paper-doll.accessory-slots-work-as-the-doll-slots",
        says: "Each slot in the accessories flyout drags its item out, unequips it on a double click, examines it on a right click, names it under the pointer, takes a dropped item through the shared equip request, and shows the shared accept or refuse hint while an item is dragged over it; a trinket or an aetheria let go on the figure or the button goes into its own slot, and the figure hints it as the equipment rules answer for that slot.",
        since: THIS_CLIENT,
        divergence: "CD-033",
        evidence: Evidence::Private("AC-EVID-UI-CLASSIC-ACCESSORIES-SLOT-USE"),
        station: "dereth-classic-ui::lib::panels::game::items::accessories_tests::a_flyout_slot_drags_out_unequips_examines_and_takes_drops_as_a_doll_slot",
        tier: Tier::Cpu,
    },
    behaviour! {
        id: "classic.paper-doll.centered-drop-feedback",
        says: "A shared equipment acceptance result draws a centered circle or X on the doll; unchanged feedback persists until the pointer leaves.",
        since: THIS_CLIENT,
        divergence: "CD-015",
        evidence: Evidence::Private("AC-EVID-UI-CLASSIC-DOLL-HINT"),
        station: "dereth-classic-ui::lib::control_host::tests::equipment_canvas_hint_keeps_unchanged_state_and_clears_on_leave",
        tier: Tier::Cpu,
    },
    behaviour! {
        id: "classic.paper-doll.drag-picks-equipped-item",
        says: "Dragging a picked doll part chooses its equipped armor before clothing; bare body and empty picks do not begin item drags.",
        since: THIS_CLIENT,
        divergence: "CD-015",
        evidence: Evidence::Private("AC-EVID-UI-CLASSIC-DOLL-DRAG"),
        station: "dereth-classic-ui::lib::panels::game::tests::paperdoll_pick_prefers_armor_over_clothing_and_examines_on_right_click",
        tier: Tier::Cpu,
    },
    behaviour! {
        id: "classic.spell-drag.preserves-transparent-coverage",
        says: "Spell drag art keeps transparent surrounding pixels and opaque foreground colors while ordinary spell rows retain their background.",
        since: THIS_CLIENT,
        divergence: "CD-015",
        evidence: Evidence::Private("AC-EVID-UI-CLASSIC-SPELL-DRAG"),
        station: "dereth-classic-ui::lib::renderer::spell_tests::spell_drag_preserves_background_coverage_and_opaque_foreground",
        tier: Tier::Dat,
    },
    behaviour! {
        id: "classic.spellbook.filters-fit-supported-world",
        says: "Classic spell filters share a school row and three level rows, fitting every school and level exposed by the current world.",
        since: THIS_CLIENT,
        divergence: "CD-015",
        evidence: Evidence::Private("AC-EVID-UI-CLASSIC-SPELL-FILTERS"),
        station: "dereth-classic-ui::lib::panels::game::magic::filter_layout_tests::schools_share_one_row_and_all_available_levels_fit_below_without_overlap",
        tier: Tier::Cpu,
    },
    behaviour! {
        id: "contracts.abandon.the-button-gives-up-the-picked-contract-and-nothing-local-moves",
        says: "With no contract picked, the button that gives one up sends nothing and counts the \
               refusal rather than passing it over in silence; with one picked it asks the shard \
               to give up that contract and nothing else. The row stays on the list and in the \
               count until the shard says it has gone, because whether it has gone is the \
               shard's to say.",
        since: RETAIL,
        evidence: Evidence::Private("AC-EVID-O967-ABANDON"),
        station: "dereth-testkit::dat::panels::scenario_the_abandon_button_gives_up_the_picked_contract_and_nothing_local_moves",
        tier: Tier::Dat,
    },
    behaviour! {
        id: "contracts.detail.picking-a-contract-fills-the-pane-beside-the-list",
        says: "Until a contract is picked the pane beside the list says nothing; picking one \
               fills all six of its lines -- what the contract asks for, who to speak to, how far \
               along it is, whether it is on a clock, where the work is and where the contact is \
               -- and who to speak to changes from the one who handed the contract out to the one \
               who takes it back once the work is under way, with a contract on a clock showing \
               the time left rather than the word for none. Picking sends nothing to the shard.",
        since: RETAIL,
        evidence: Evidence::Private("AC-EVID-O967-DETAIL"),
        station: "dereth-testkit::dat::panels::scenario_picking_a_contract_fills_the_pane_beside_the_list",
        tier: Tier::Dat,
    },
    behaviour! {
        id: "contracts.list.one-contract-at-a-time-is-added-changed-or-taken-away",
        says: "The shard can speak about one contract on its own: one already on the list has how \
               far along it is changed where it stands without moving it, one the shard says to \
               drop leaves both the list and the screen, and one the character did not hold is \
               added back in its place in the order -- each answered on its own rather than by \
               sending the whole list again.",
        since: RETAIL,
        evidence: Evidence::Private("AC-EVID-O967-ONE"),
        station: "dereth-testkit::dat::panels::scenario_one_contract_at_a_time_is_added_changed_or_taken_away",
        tier: Tier::Dat,
    },
    behaviour! {
        id: "contracts.sort.the-two-buttons-choose-the-order-and-pressing-one-twice-turns-it-round",
        says: "The two buttons over the contracts list choose whether it is ordered by name or by \
               how far along each contract is; pressing the one already in force turns the order \
               round, and pressing the other changes what the order is about and puts it the \
               right way up again. The rows on screen follow the order, not only the list behind \
               them, and neither button sends anything to the shard.",
        since: RETAIL,
        evidence: Evidence::Private("AC-EVID-O967-SORT"),
        station: "dereth-testkit::dat::panels::scenario_the_sort_buttons_choose_the_order_and_a_second_press_turns_it_round",
        tier: Tier::Dat,
    },
    behaviour! {
        id: "contracts.tab.the-tab-draws-a-row-per-contract-the-shard-sent-in-name-order",
        says: "The contracts tab draws one row for every contract the shard says the character \
               has taken on, each naming the contract and saying how far along it is, ordered by \
               name without regard to case rather than in the order the shard listed them, with \
               nothing picked to begin with -- and a tab that drew nothing is distinguishable \
               from one that was never asked to draw.",
        since: RETAIL,
        evidence: Evidence::Private("AC-EVID-O967-LIST"),
        station: "dereth-testkit::dat::panels::scenario_the_contracts_tab_draws_a_row_per_contract_in_name_order",
        tier: Tier::Dat,
    },
    behaviour! {
        id: "dialog.confirmation.a-question-about-making-something-adds-the-clients-own-continue",
        says: "A question about something that would change or destroy an item has the client's \
               own word Continue put after what the shard wrote, where the plainest kind of \
               question has nothing added to it -- so which kind was asked is visible on the \
               screen and not only in the message.",
        since: RETAIL,
        evidence: Evidence::Private("AC-EVID-F65-SUFFIX"),
        station: "dereth-testkit::dat::panels::scenario_a_making_question_gets_the_clients_own_continue",
        tier: Tier::Dat,
    },
    behaviour! {
        id: "dialog.confirmation.a-question-appears-and-both-answers-reach-the-shard",
        says: "A yes-or-no question the shard asks appears on screen in the words it was asked in, \
               nothing leaves the client until the player answers, and both answers are sent -- a \
               refusal is a message and not a silence, which is what saves the shard waiting out \
               its own timeout.",
        since: RETAIL,
        evidence: Evidence::Private("AC-EVID-F65"),
        station: "dereth-testkit::dat::panels::scenario_an_npc_question_appears_and_both_answers_reach_the_shard",
        tier: Tier::Dat,
    },
    behaviour! {
        id: "dialog.confirmation.a-second-question-while-one-is-open-takes-over-its-handle-and-is-not-queued",
        says: "A second question from the shard while one is already on screen puts nothing new \
               on screen and is not queued behind it. The first question's words stay up, but the \
               client now holds the second question's handle, so answering the dialog answers the \
               second question and the first is left to the shard's timeout. Withdrawing the \
               first no longer takes the dialog down; withdrawing the second does, and refuses on \
               the way out.",
        since: RETAIL,
        evidence: Evidence::Private("AC-EVID-F65-SLOT"),
        station: "dereth-testkit::dat::panels::scenario_a_second_question_while_one_is_open_takes_over_its_handle",
        tier: Tier::Dat,
    },
    behaviour! {
        id: "dialog.confirmation.an-invitation-and-a-swearing-ask-their-own-panels-question",
        says: "An invitation to a fellowship and somebody swearing allegiance are asked about by \
               their own panels in their own words rather than by the plain question, the answer \
               to each reaches the shard with the question's own handle -- and a kind of question \
               the client knows nothing about raises nothing and answers nothing.",
        since: RETAIL,
        evidence: Evidence::Private("AC-EVID-G16-G17"),
        station: "dereth-testkit::dat::panels::scenario_an_invitation_and_a_swearing_raise_their_own_panels_questions",
        tier: Tier::Dat,
    },
    behaviour! {
        id: "dialog.confirmation.the-same-command-twice-asks-twice-one-question-after-the-other",
        says: "Typing `@die` or `@house abandon` again while its question is still on screen asks \
               the question a second time: the second waits behind the first rather than being \
               dropped or shown beside it, and answering the first brings the second up in the \
               same words. Two Nos send nothing.",
        since: RETAIL,
        evidence: Evidence::Private("AC-EVID-CONFIRM-REPEAT"),
        station: "dereth-testkit::dat::panels::scenario_the_same_command_twice_asks_twice_one_question_after_the_other",
        tier: Tier::Dat,
    },
    behaviour! {
        id: "dialog.confirmation.the-shards-withdrawal-closes-it-and-refuses-on-the-way-out",
        says: "The client runs no clock of its own on a question: the only thing that takes an \
               unanswered one down is the shard withdrawing it, and when it does the client \
               refuses on the way out rather than leaving the shard to time it out -- while a \
               withdrawal naming a different question is ignored outright.",
        since: RETAIL,
        evidence: Evidence::Private("AC-EVID-F65-ABORT"),
        station: "dereth-testkit::dat::panels::scenario_the_shards_withdrawal_closes_the_question_and_refuses_on_the_way_out",
        tier: Tier::Dat,
    },
    behaviour! {
        id: "examine.armour.the-pane-gives-its-level-and-eight-resistances-in-the-order-it-draws-them",
        says: "Assessing a piece of armour draws its armour level and then how well it turns each \
               of the eight kinds of harm, each as a word and a number, in the order the pane \
               draws them and not the order the answer packs them in.",
        since: RETAIL,
        evidence: Evidence::Private("AC-EVID-F29-ARMOUR"),
        station: "dereth-testkit::dat::panels::scenario_the_armour_pane_gives_its_level_and_eight_resistances",
        tier: Tier::Dat,
    },
    behaviour! {
        id: "examine.augmentation.the-cost-line-is-the-shipped-sentence-with-the-number-in-it",
        says: "A gem that would spend the player's unspent experience says so in the shipped \
               sentence, with the number grouped and the sentence's own spacing around it, and \
               says nothing at all when the shard named no cost -- a cost of none is still a \
               cost and is drawn.",
        since: RETAIL,
        evidence: Evidence::Private("AC-EVID-P1-78B-AUG"),
        station: "dereth-testkit::dat::panels::scenario_the_augmentation_cost_line_comes_off_the_shipped_sentence",
        tier: Tier::Dat,
    },
    behaviour! {
        id: "examine.capacity.a-container-says-how-much-it-holds-and-a-book-how-many-pages-are-used",
        says: "A container says how many things and how many other containers it can hold, and \
               says nothing when it holds neither; a book says how many of its pages are written \
               on, used first and total second.",
        since: RETAIL,
        evidence: Evidence::Private("AC-EVID-P1-14-CAPACITY"),
        station: "dereth-testkit::dat::panels::scenario_a_container_says_how_much_it_holds_and_a_book_how_many_pages_are_used",
        tier: Tier::Dat,
    },
    behaviour! {
        id: "examine.character.a-body-part-nothing-can-be-cast-on-is-marked-and-reads-what-is-left",
        says: "A piece of a character's body that no spell can touch is marked with a star in the \
               armour rows and reads as whatever armour is left on it, and a character the shard \
               sent no armour at all for loses those rows and the blank above them while keeping \
               the note that explains the star.",
        since: RETAIL,
        evidence: Evidence::Private("AC-EVID-P1-56-STAR"),
        station: "dereth-testkit::dat::panels::scenario_an_unenchantable_body_part_is_marked_with_a_star",
        tier: Tier::Dat,
    },
    behaviour! {
        id: "examine.character.an-allegiance-rank-puts-its-title-in-front-of-the-name",
        says: "A character of allegiance rank has that rank's title put in front of their name in \
               the window's title bar, and a rank there is no title for leaves the bare name.",
        since: RETAIL,
        evidence: Evidence::Private("AC-EVID-P1-56-TITLE"),
        station: "dereth-testkit::dat::panels::scenario_an_allegiance_rank_puts_its_title_in_front_of_the_name",
        tier: Tier::Dat,
    },
    behaviour! {
        id: "examine.character.every-row-draws-in-the-panes-own-order-and-reaches-the-list",
        says: "A character with every one of these things about them draws every row in the \
               pane's own order, the player-killer, overpower and enlightenment rows included, so a \
               row with the right words in the wrong place is still \
               wrong -- and every one of them reaches a real row of the list rather than only the \
               panel's own record of it.",
        since: RETAIL,
        evidence: Evidence::Private("AC-EVID-P1-56-ORDER"),
        station: "dereth-testkit::dat::panels::scenario_every_character_row_draws_in_the_panes_own_order",
        tier: Tier::Dat,
    },
    behaviour! {
        id: "examine.character.the-allegiance-rows-fork-on-whether-the-monarch-is-also-the-patron",
        says: "A character who follows someone draws a row for their monarch and a row for their \
               patron, or one row naming both when they are the same person; one who leads draws \
               how many follow them, with one follower and several told apart; and a character of \
               no rank draws none of it, with the allegiance's name cleared rather than left \
               holding the last character's.",
        since: RETAIL,
        evidence: Evidence::Private("AC-EVID-P1-56-ALLEGIANCE"),
        station: "dereth-testkit::dat::panels::scenario_the_allegiance_rows_fork_on_the_two_titles",
        tier: Tier::Dat,
    },
    behaviour! {
        id: "examine.character.the-armour-rows-are-the-shards-own-numbers-and-the-three-lines-beside-them",
        says: "Assessing another player draws their armour as three rows of three numbers taken \
               from what the shard sent, and beside them their gender and heritage, the title they \
               have chosen and whether they can be attacked -- with nothing about them left on the \
               pane's own list of what it cannot draw.",
        since: RETAIL,
        evidence: Evidence::Private("AC-EVID-P1-56"),
        station: "dereth-testkit::dat::panels::scenario_the_character_pane_rows_are_the_shards_own_numbers",
        tier: Tier::Dat,
    },
    behaviour! {
        id: "examine.character.the-pane-draws-its-rows-and-has-nothing-left-undrawn",
        says: "Assessing another player draws the character pane: their level and the same nine \
               attribute and vital rows, with the extra rows beneath them really drawn rather than \
               listed as not yet written.",
        since: RETAIL,
        evidence: Evidence::Private("AC-EVID-P1-14-CHARACTER"),
        station: "dereth-testkit::dat::panels::scenario_the_character_pane_draws_its_rows_and_has_nothing_left_undrawn",
        tier: Tier::Dat,
    },
    behaviour! {
        id: "examine.character.the-rating-groups-draw-the-pairs-the-pane-prints-and-no-others",
        says: "The five groups of ratings -- damage, damage resistance, player-killer damage, \
               overpower and damage over time -- draw the pairs of numbers the pane prints and no \
               others, player-killer before overpower: a number that opens a group without being \
               in its pair is a guard and is never drawn itself, and one of the numbers the pane \
               reads is read and then never printed at all.",
        since: RETAIL,
        evidence: Evidence::Private("AC-EVID-P1-56-RATINGS"),
        station: "dereth-testkit::dat::panels::scenario_the_rating_groups_draw_the_pairs_the_pane_prints",
        tier: Tier::Dat,
    },
    behaviour! {
        id: "examine.character.the-seven-tail-rows-each-draw-the-thing-behind-them",
        says: "The foot of the character pane draws their fellowship, when they arrived and how \
               long they have been here, their chess rank, their fishing skill, how often they \
               have died -- as a sentence rather than a number when they never have -- how many \
               titles they have earned, and their enlightenment whenever the shard sent one, even \
               a zero, in that order.",
        since: RETAIL,
        evidence: Evidence::Private("AC-EVID-P1-56-TAIL"),
        station: "dereth-testkit::dat::panels::scenario_the_tail_rows_are_the_seven_the_pane_lists",
        tier: Tier::Dat,
    },
    behaviour! {
        id: "examine.character.the-society-row-is-coloured-by-the-viewers-own-society",
        says: "The society row is drawn plain when the player belongs to none, in the row \
               template's green when the other character is of the player's own society and in its \
               red when they are of a rival one -- both cells of the row, not only the value.",
        since: RETAIL,
        evidence: Evidence::Private("AC-EVID-P1-78B-SOCIETY"),
        station: "dereth-testkit::dat::panels::scenario_the_society_row_is_coloured_by_the_viewers_own_society",
        tier: Tier::Dat,
    },
    behaviour! {
        id: "examine.character.the-society-row-names-the-society-and-the-band-its-rank-falls-in",
        says: "A character who has joined a society has it named at the top of the pane with the \
               band their rank falls in after it, no band at all when the shard sent no rank, and \
               a plain mark of ignorance when the society is one the client has no name for.",
        since: RETAIL,
        evidence: Evidence::Private("AC-EVID-P1-56-SOCIETY"),
        station: "dereth-testkit::dat::panels::scenario_the_society_row_names_the_society_and_its_rank_band",
        tier: Tier::Dat,
    },
    behaviour! {
        id: "examine.consumables.a-potion-says-what-it-restores-and-a-kit-what-it-adds",
        says: "A potion says how much of what it restores and whether it can be sold; a healing \
               kit built on the same numbers says what it adds to the skill instead, and both say \
               how many uses are left -- or that nobody knows.",
        since: RETAIL,
        evidence: Evidence::Private("AC-EVID-P1-14-CONSUMABLES"),
        station: "dereth-testkit::dat::panels::scenario_a_potion_says_what_it_restores_and_a_kit_what_it_adds",
        tier: Tier::Dat,
    },
    behaviour! {
        id: "examine.cooldown.the-pane-says-how-long-it-is-and-how-long-is-left-of-the-players-own",
        says: "Assessing something that has to be waited on says how long the wait is, and adds \
               how much of the player's own wait is left only when it is that same wait and it is \
               still running -- while a thing whose wait has no length draws neither line.",
        since: RETAIL,
        evidence: Evidence::Private("AC-EVID-P1-78-COOLDOWN"),
        station: "dereth-testkit::dat::panels::scenario_the_cooldown_lines_reach_the_item_pane",
        tier: Tier::Dat,
    },
    behaviour! {
        id: "examine.creature.the-creature-pane-draws-the-same-rating-rows-without-the-footnote",
        says: "Assessing a creature that is not another player uses the same extra list and draws \
               the same rating rows in it, with the overpower group ahead of the player-killer \
               one, and no note about what cannot be enchanted -- and a \
               creature with no rating worth drawing draws no extra rows at all.",
        since: RETAIL,
        evidence: Evidence::Private("AC-EVID-P1-56-CREATURE"),
        station: "dereth-testkit::dat::panels::scenario_the_creature_pane_draws_the_same_rating_rows_without_the_footnote",
        tier: Tier::Dat,
    },
    behaviour! {
        id: "examine.creature.the-pane-names-the-kind-the-level-six-attributes-and-three-vitals",
        says: "Assessing a creature draws what kind of creature it is, its level, its six \
               attributes and its three vitals as current over maximum -- with a percentage on \
               health alone -- rather than a frame of labels with nothing beside them.",
        since: RETAIL,
        evidence: Evidence::Private("AC-EVID-F29-CREATURE"),
        station: "dereth-testkit::dat::panels::scenario_the_creature_pane_names_the_kind_the_level_and_the_nine_rows",
        tier: Tier::Dat,
    },
    behaviour! {
        id: "examine.description.the-long-one-is-decorated-and-a-plating-name-replaces-it",
        says: "The description of a thing is decorated where the player reads it -- how well it \
               was made in front of it, what it is made of woven into its own name, and how many \
               of which gem it is set with after it, singular or plural and grouped -- and a \
               plating name replaces the description outright.",
        since: RETAIL,
        evidence: Evidence::Private("AC-EVID-P1-78B-DECOR"),
        station: "dereth-testkit::dat::panels::scenario_the_long_description_is_decorated_where_the_player_reads_it",
        tier: Tier::Dat,
    },
    behaviour! {
        id: "examine.description.the-short-one-is-drawn-only-when-there-is-no-long-one-at-all",
        says: "The shorter description is drawn only when the shard sent no long one whatever -- \
               a long one that is empty is still one and suppresses it -- and a thing with neither \
               is drawn with neither.",
        since: RETAIL,
        evidence: Evidence::Private("AC-EVID-P1-78B-SHORT"),
        station: "dereth-testkit::dat::panels::scenario_the_short_description_is_used_only_when_there_is_no_long_one",
        tier: Tier::Dat,
    },
    behaviour! {
        id: "examine.enchanted.a-raised-line-is-green-a-lowered-one-red-and-everything-else-plain",
        says: "A line of an assessment about something a spell has raised is drawn green, one \
               about something a spell has lowered is drawn red, and every other line plain -- all \
               three on one pane at once, out of the three colours the pane itself declares -- \
               while the same thing with nothing on it is drawn in one colour throughout.",
        since: RETAIL,
        evidence: Evidence::Private("AC-EVID-G14-B"),
        station: "dereth-testkit::dat::panels::scenario_a_raised_line_is_green_a_lowered_one_red_and_the_rest_plain",
        tier: Tier::Dat,
    },
    behaviour! {
        id: "examine.enchanted.every-property-the-pane-highlights-is-answered-by-the-table-its-flag-names",
        says: "Every property the assessment pane would colour can really be asked about, in the \
               one of the client's two tables its own flag names and not in the other -- because a \
               property neither table answers for would be skipped in silence and its line would \
               be drawn plain for ever.",
        since: RETAIL,
        evidence: Evidence::Private("AC-EVID-G14-KEYS"),
        station: "dereth-testkit::cpu::panels::scenario_every_highlighted_property_is_answered_by_the_table_its_flag_names",
        tier: Tier::Cpu,
    },
    behaviour! {
        id: "examine.failed-assess.an-unassessed-stat-reads-unknown-even-when-it-is-enchanted",
        says: "A stat the player failed to assess is drawn in the unknown colour even when the \
               same stat is enchanted, because the failure is decided before the question about \
               enchantment is asked -- and the same stats on an assessment that worked take the \
               raised and lowered colours.",
        since: RETAIL,
        evidence: Evidence::Private("AC-EVID-P1-79-PRECEDENCE"),
        station: "dereth-testkit::dat::panels::scenario_the_failure_colour_outranks_the_enchantment_colour",
        tier: Tier::Dat,
    },
    behaviour! {
        id: "examine.failed-assess.every-value-cell-takes-the-unknown-colour-and-no-label-does",
        says: "An assessment that failed draws all nine of the creature pane's value cells in the \
               colour the row template keeps for what is not known, and leaves every label in the \
               colour it had -- and an assessment that worked draws the cells the shard marked \
               enchanted in the enchanted colours rather than plain.",
        since: RETAIL,
        evidence: Evidence::Private("AC-EVID-P1-79"),
        station: "dereth-testkit::dat::panels::scenario_a_failed_assessment_draws_every_value_cell_in_the_unknown_colour",
        tier: Tier::Dat,
    },
    behaviour! {
        id: "examine.failed-assess.the-item-panes-own-unknown-value-line-is-drawn-plain",
        says: "The line on the item pane saying what a thing is worth, when nobody knows, is drawn \
               in the pane's plain colour and not in the colour the creature pane uses for the \
               same ignorance.",
        since: RETAIL,
        evidence: Evidence::Private("AC-EVID-P1-79-VALUE"),
        station: "dereth-testkit::dat::panels::scenario_a_failed_assessment_leaves_the_item_panes_value_line_plain",
        tier: Tier::Dat,
    },
    behaviour! {
        id: "examine.failed-assess.the-numbers-the-shard-described-are-kept-and-health-is-a-percentage",
        says: "An assessment that failed still shows whatever the shard had already described: the \
               six attributes keep their numbers, health falls back to a bare percentage with no \
               maximum beside it, and only the two the shard said nothing about read as unknown -- \
               while a body described not at all reads unknown nine times.",
        since: RETAIL,
        evidence: Evidence::Private("AC-EVID-P1-79-TEXT"),
        station: "dereth-testkit::dat::panels::scenario_a_failed_assessment_keeps_the_numbers_and_reduces_health_to_a_percentage",
        tier: Tier::Dat,
    },
    behaviour! {
        id: "examine.inscription.a-box-somebody-else-signed-is-not-yours-to-change",
        says: "A thing somebody else has signed shows its words but is not the player's to change, \
               and whatever is typed at it reaches nobody -- while their own signature does not \
               lock them out, and a thing that is not in their possession is not theirs to write \
               on either.",
        since: RETAIL,
        evidence: Evidence::Private("AC-EVID-F35-EDITABLE"),
        station: "dereth-testkit::dat::panels::scenario_a_box_somebody_else_signed_is_not_yours_to_change",
        tier: Tier::Dat,
    },
    behaviour! {
        id: "examine.inscription.a-press-empties-the-box-and-shows-whose-signature-it-will-carry",
        says: "Pressing inside the inscription box takes the invitation out of it rather than \
               dropping a caret into those words, and puts the player's own signature underneath \
               so they can see whose name they are about to leave -- and nothing but a press \
               inside it hands the box the keyboard, so assessing a thing does not quietly take \
               it.",
        since: RETAIL,
        evidence: Evidence::Private("AC-EVID-F35-FOCUS"),
        station: "dereth-testkit::dat::panels::scenario_a_press_empties_the_box_and_shows_whose_signature_it_will_carry",
        tier: Tier::Dat,
    },
    behaviour! {
        id: "examine.inscription.a-signed-thing-with-no-words-shows-a-blank-box-that-is-the-scribes",
        says: "A thing somebody has signed but written nothing on shows a box that is there and \
               blank, with no signature under it -- and it is not the player's to change unless \
               the signature is their own, in which case the press does not wipe what is not there \
               and what they write goes to the shard.",
        since: RETAIL,
        evidence: Evidence::Private("AC-EVID-F42-BLANK"),
        station: "dereth-testkit::dat::panels::scenario_a_signed_thing_with_no_words_shows_a_blank_box",
        tier: Tier::Dat,
    },
    behaviour! {
        id: "examine.inscription.a-target-change-sends-the-literal-placeholder",
        says: "Examining an unsigned item that is not the player's while the inscription box has \
               the caret sends a set-inscription request for that new item carrying the literal \
               invitation <Inscribe here>, because the caret is taken from a box he may not write \
               in; the words he had typed are not sent.",
        since: RETAIL,
        evidence: Evidence::Private("AC-EVID-P48-INSCRIPTION"),
        station: "dereth-client::gpu::panels::inscription_placeholder::the_invitation_leg_sends_the_literal_placeholder_for_an_item_that_is_not_yours",
        tier: Tier::Gpu,
    },
    behaviour! {
        id: "examine.inscription.escape-is-the-edge-that-confirms-it-and-return-is-not",
        says: "The shipped data says the key that confirms an inscription is Escape and not \
               Return: the box carries the attribute for the one and not the other, Return is \
               swallowed with the caret left where it was, and Escape lets the caret go -- which \
               is what sends.",
        since: RETAIL,
        evidence: Evidence::Private("AC-EVID-F35-ESCAPE"),
        station: "dereth-testkit::dat::panels::scenario_escape_is_the_confirm_edge_and_return_is_not",
        tier: Tier::Dat,
    },
    behaviour! {
        id: "examine.inscription.leaving-an-unchanged-box-sends-nothing-and-the-invitation-comes-back",
        says: "An empty box on a thing nobody has signed is nothing to say: the caret going in and \
               out again sends no message and the centred invitation comes back -- and leaving a \
               box whose words have not changed is not a second inscription.",
        since: RETAIL,
        evidence: Evidence::Private("AC-EVID-F35-GUARDS"),
        station: "dereth-testkit::dat::panels::scenario_leaving_an_unchanged_box_sends_nothing",
        tier: Tier::Dat,
    },
    behaviour! {
        id: "examine.inscription.letting-the-caret-go-sends-what-was-written-and-it-comes-back",
        says: "Letting the caret go is what sends what was written; the box keeps showing it and \
               the signature keeps showing who wrote it, and the same thing assessed again comes \
               back carrying both, still the writer's to change.",
        since: RETAIL,
        evidence: Evidence::Private("AC-EVID-F35-WIRE"),
        station: "dereth-testkit::dat::panels::scenario_letting_the_caret_go_sends_what_was_written",
        tier: Tier::Dat,
    },
    behaviour! {
        id: "examine.inscription.live-object-gates-editing",
        says: "An inscription can be edited only while the actual examined object is present and \
               allows inscriptions, followed by the author or privilege checks. A displayed hook \
               target cannot grant permission to edit the live object.",
        since: RETAIL,
        evidence: Evidence::Private("AC-EVID-APPRAISAL-INSCRIPTION"),
        station: "dereth-classic-ui::lib::panels::game::examine::appraisal_tests::a_displayed_inscription_requires_the_live_object_before_ownership_or_privilege",
        tier: Tier::Cpu,
    },
    behaviour! {
        id: "examine.inscription.looking-at-another-you-may-write-on-keeps-the-caret-where-it-was",
        says: "Looking at another thing the player may also write on leaves the caret exactly \
               where it was and sends nothing -- and after the caret has been taken away by \
               something else, the next inscription still commits, once, carrying the second set \
               of words and naming the thing the box was showing.",
        since: RETAIL,
        evidence: Evidence::Private("AC-EVID-F64-KEEP"),
        station: "dereth-testkit::dat::panels::scenario_changing_to_another_one_you_may_write_on_keeps_the_caret",
        tier: Tier::Dat,
    },
    behaviour! {
        id: "examine.inscription.looking-at-one-you-may-not-write-on-takes-the-caret-away",
        says: "Looking at something the player may not write on, or at something that cannot be \
               written on at all, takes the caret out of the box with it: what was typed is \
               discarded rather than sent to either thing, and the keyboard stops going to the box.",
        since: RETAIL,
        evidence: Evidence::Private("AC-EVID-F64"),
        station: "dereth-testkit::dat::panels::scenario_changing_the_target_to_one_you_may_not_write_on_drops_the_caret",
        tier: Tier::Dat,
    },
    behaviour! {
        id: "examine.inscription.the-box-is-there-only-on-something-that-can-be-inscribed",
        says: "The place to write on an item appears only on an item that can be written on, and \
               invites the player to inscribe it when nobody has; on anything else there is no box \
               at all rather than an empty one.",
        since: RETAIL,
        evidence: Evidence::Private("AC-EVID-F29-INSCRIBE"),
        station: "dereth-testkit::dat::panels::scenario_the_inscribe_box_appears_only_on_something_inscribable",
        tier: Tier::Dat,
    },
    behaviour! {
        id: "examine.inscription.the-invitation-is-centred-and-a-real-inscription-is-not",
        says: "An empty inscription box says where to write and says it in the middle of itself; a \
               box carrying somebody's words draws them from the corner with the signature under \
               them.",
        since: RETAIL,
        evidence: Evidence::Private("AC-EVID-F35"),
        station: "dereth-testkit::dat::panels::scenario_the_invitation_is_centred_and_a_real_inscription_is_not",
        tier: Tier::Dat,
    },
    behaviour! {
        id: "examine.inscription.the-windows-close-button-commits-what-was-typed-on-its-way-out",
        says: "Pressing the assessment window's own close button takes the caret away before the \
               window goes, so what was typed is sent on the way out rather than lost.",
        since: RETAIL,
        evidence: Evidence::Private("AC-EVID-F42-CLOSE"),
        station: "dereth-testkit::dat::panels::scenario_the_close_button_commits_on_its_way_out",
        tier: Tier::Dat,
    },
    behaviour! {
        id: "examine.inscription.the-writer-can-rub-out-his-own-words-and-the-invitation-comes-back",
        says: "The writer rubbing their own words out sends an inscription of nothing, the centred \
               invitation comes back with the signature line cleared, and the thing is anybody's \
               to sign again.",
        since: RETAIL,
        evidence: Evidence::Private("AC-EVID-F42-ERASE"),
        station: "dereth-testkit::dat::panels::scenario_the_scribe_can_rub_out_his_own_words",
        tier: Tier::Dat,
    },
    behaviour! {
        id: "examine.item-blocks.every-line-the-item-pane-can-draw-reaches-it-through-the-shard",
        says: "Every sentence the item pane knows how to write -- how it was made, what set it \
               belongs to, its ratings (overpower and player-killer damage among them), what it \
               does for defence and for a caster, who may hold or \
               use it, how far it has come along, what switches it on, what is stored in it, what \
               is left of it, who made it and how rare it is -- is produced from what the shard \
               actually sent, and the pane has no sentence left that it cannot draw.",
        since: RETAIL,
        evidence: Evidence::Private("AC-EVID-P1-14-BLOCKS"),
        station: "dereth-testkit::dat::panels::scenario_every_line_the_item_pane_can_draw_reaches_it_through_the_shard",
        tier: Tier::Dat,
    },
    behaviour! {
        id: "examine.item.the-floating-windows-item-pane-shows-no-icon-of-the-item",
        says: "Examining an item shows its name and its description but no picture of the item: \
               the floating examination window's item pane has no place for one, though the \
               docked panel it replaced did.",
        since: RETAIL,
        evidence: Evidence::Private("AC-EVID-R2C-ITEM-EXAMINE-NO-ICON"),
        station: "dereth-ui-screens::dat::panels::examine_window::examining_an_item_shows_no_icon_because_the_shipped_item_pane_has_no_place_for_one",
        tier: Tier::Dat,
    },
    behaviour! {
        id: "examine.key.the-assess-key-shuts-an-open-pane-and-opens-a-shut-one",
        says: "The key that assesses what the player is looking at is a toggle: pressed with the \
               pane shut it asks the shard and the answer opens it, pressed again on the open pane \
               it shuts it and asks nothing, and pressed a third time it asks again -- so a pane \
               that closed and one that stopped working are different.",
        since: RETAIL,
        evidence: Evidence::Private("AC-EVID-O627-TOGGLE"),
        station: "dereth-testkit::dat::panels::scenario_the_assess_key_shuts_an_open_pane_and_opens_a_shut_one",
        tier: Tier::Dat,
    },
    behaviour! {
        id: "examine.key.the-pane-shuts-with-nothing-under-the-pointer-and-a-shut-one-does-nothing",
        says: "That key shuts the pane even when the player has nothing under the pointer, because \
               the shutting happens before it looks; and with nothing under the pointer and the \
               pane already shut the press does nothing at all rather than opening an empty one.",
        since: RETAIL,
        evidence: Evidence::Private("AC-EVID-O627-NO-SELECTION"),
        station: "dereth-testkit::dat::panels::scenario_the_assess_key_shuts_the_pane_with_nothing_under_the_pointer",
        tier: Tier::Dat,
    },
    behaviour! {
        id: "examine.lifespan.an-item-that-expires-says-when-and-needs-all-three-of-its-numbers",
        says: "An item with a limited life says how long it has left, spelled out in years, days, \
               hours, minutes and seconds, and says it is disintegrating once that has run out; an \
               item missing any one of the three numbers the line is built from says nothing at \
               all rather than something wrong.",
        since: RETAIL,
        evidence: Evidence::Private("AC-EVID-P1-78B"),
        station: "dereth-testkit::dat::panels::scenario_an_item_that_expires_says_when_and_needs_all_three_numbers",
        tier: Tier::Dat,
    },
    behaviour! {
        id: "examine.pane.template-selects-character",
        says: "A character template selects character information even when the identified player \
               has no title, in either interface and after a view snapshot.",
        since: RETAIL,
        evidence: Evidence::Private("AC-EVID-APPRAISAL-PANE"),
        station: "dereth-classic-ui::lib::panels::game::examine::appraisal_tests::a_titleless_template_uses_character_content_in_direct_and_frozen_views",
        tier: Tier::Cpu,
    },
    behaviour! {
        id: "examine.portal.a-portal-with-no-restriction-still-gains-the-blocks-two-separators",
        says: "A portal the shard says nothing restrictive about still gains the two blank lines \
               the block always writes, a portal with restrictions gains those and its sentences, \
               and a thing that is not a portal at all gains neither -- so the separators are part \
               of the block and not part of a sentence.",
        since: RETAIL,
        evidence: Evidence::Private("AC-EVID-P1-78-DENOMINATOR"),
        station: "dereth-testkit::dat::panels::scenario_a_portal_with_no_restrictions_still_gets_the_blocks_separators",
        tier: Tier::Dat,
    },
    behaviour! {
        id: "examine.portal.each-restriction-draws-its-own-sentence-and-only-its-own",
        says: "Assessing a portal draws one sentence for each way it is restricted and no others, \
               in the pane's own order and in its plain colour -- and three of those sentences end \
               in the same words, so a portal that is closed to one kind of player must not read \
               as closed to another.",
        since: RETAIL,
        evidence: Evidence::Private("AC-EVID-P1-78"),
        station: "dereth-testkit::dat::panels::scenario_each_portal_restriction_draws_its_own_sentence_and_only_its_own",
        tier: Tier::Dat,
    },
    behaviour! {
        id: "examine.portrait.appraising-a-creature-paints-its-model-and-an-item-gets-none",
        says: "Appraising a creature draws its live model in the examine window, in the same box \
               as the list of its numbers, and an appraised item gets no model at all.",
        since: RETAIL,
        evidence: Evidence::Private("AC-EVID-F36-PORTRAIT"),
        station: "dereth-client::gpu::panels::examine_portrait::appraising_a_creature_paints_its_model_into_the_identify_panel",
        tier: Tier::Gpu,
    },
    behaviour! {
        id: "examine.portrait.the-model-is-drawn-over-the-attribute-lists-ground",
        says: "In the examine window a creature's model is drawn over the translucent rows of the \
               attribute list laid over it, and under the list's text, so the list does not \
               shade the model.",
        since: RETAIL,
        evidence: Evidence::Private("AC-EVID-OWNER-EXAMINE-PORTRAIT"),
        station: "dereth-client::gpu::panels::examine_portrait::the_creatures_attribute_list_does_not_shade_its_model",
        tier: Tier::Gpu,
    },
    behaviour! {
        id: "examine.route.every-examine-route-sends-the-same-request-and-arms-the-wait",
        says: "All four ways of examining something (clicking it with the examine cursor, a click \
               in examine targeting, the identify button and the assess key) send one appraisal \
               request for that object and mark the client as waiting for its answer, so the reply \
               opens the examine panel.",
        since: RETAIL,
        evidence: Evidence::Private("AC-EVID-O310-ROUTE"),
        station: "dereth-client::dat::panels::examine_routes::every_examine_route_reaches_examine_object_and_sends_the_same_request",
        tier: Tier::Dat,
    },
    behaviour! {
        id: "examine.scroll.the-bar-brings-the-end-of-a-long-description-into-view",
        says: "Pressing the arrow at the bottom of the assessment window's bar, or dragging its \
               thumb to the bottom of the track, brings the end of a description too long for the \
               pane into view -- where before the gesture it could not be read at all.",
        since: RETAIL,
        evidence: Evidence::Private("AC-EVID-G14-A-GESTURE"),
        station: "dereth-testkit::dat::panels::scenario_the_bar_really_scrolls_the_description",
        tier: Tier::Dat,
    },
    behaviour! {
        id: "examine.scroll.the-description-pane-shows-a-bar-only-when-its-text-does-not-fit",
        says: "The assessment window measures the description it is holding: a description that \
               does not fit the pane raises the scrollbar, and one that fits leaves the strip \
               beside the pane bare -- so the bar appearing is a measurement and not decoration.",
        since: RETAIL,
        evidence: Evidence::Private("AC-EVID-G14-A"),
        station: "dereth-testkit::dat::panels::scenario_the_description_pane_shows_a_bar_only_when_its_text_does_not_fit",
        tier: Tier::Dat,
    },
    behaviour! {
        id: "examine.spells.an-enchantment-is-listed-apart-from-the-spells-the-item-was-made-with",
        says: "A spell somebody cast on an item is listed under its own heading and left out of \
               the summary line, which names only the spells the item itself carries.",
        since: RETAIL,
        evidence: Evidence::Private("AC-EVID-F62-ENCHANTMENTS"),
        station: "dereth-testkit::dat::panels::scenario_an_enchantment_is_listed_apart_from_the_items_own_spells",
        tier: Tier::Dat,
    },
    behaviour! {
        id: "examine.spells.an-item-with-spells-on-it-names-them-and-describes-them",
        says: "Assessing an enchanted item lists the spells on it by name, as a summary line first \
               and then a paragraph describing each one, with the descriptions taken from the \
               shipped spell table.",
        since: RETAIL,
        evidence: Evidence::Private("AC-EVID-F62-RECORDED"),
        station: "dereth-testkit::dat::panels::scenario_an_assessed_item_names_its_spells_and_describes_them",
        tier: Tier::Dat,
    },
    behaviour! {
        id: "examine.spells.an-unsuccessful-assessment-says-so-and-a-plain-item-says-nothing",
        says: "An assessment the character was not good enough to make says the spells are unknown \
               rather than inventing any, and an item that carries no spells at all draws neither \
               the summary line nor either heading.",
        since: RETAIL,
        evidence: Evidence::Private("AC-EVID-F62-UNKNOWN"),
        station: "dereth-testkit::dat::panels::scenario_an_unsuccessful_assessment_says_unknown_and_a_plain_item_says_nothing",
        tier: Tier::Dat,
    },
    behaviour! {
        id: "examine.spells.the-magic-lines-come-in-the-clients-own-order-and-a-rate-beats-a-flat-cost",
        says: "The spellcraft, the mana it holds and what it costs to use are drawn in that order \
               above the spell descriptions; an item that spends mana over time says so per so \
               many seconds instead of giving a flat price, and a flat price carries the note that \
               a skill can reduce it.",
        since: RETAIL,
        evidence: Evidence::Private("AC-EVID-F62-MANA"),
        station: "dereth-testkit::dat::panels::scenario_the_magic_lines_come_in_order_and_a_rate_beats_a_flat_cost",
        tier: Tier::Dat,
    },
    behaviour! {
        id: "examine.weapon.an-item-on-someone-elses-hook-takes-its-slot-from-the-reply",
        says: "An item the player can only see on somebody else's hook is described from what the \
               assessment itself says about where it is worn: with that in the reply the weapon \
               lines are drawn, and without it they are absent even though the weapon numbers are \
               there.",
        since: RETAIL,
        evidence: Evidence::Private("AC-EVID-P1-132"),
        station: "dereth-testkit::dat::panels::scenario_an_item_on_someone_elses_hook_takes_its_slot_from_the_reply",
        tier: Tier::Dat,
    },
    behaviour! {
        id: "examine.weapon.the-pane-gives-the-skill-the-damage-the-speed-the-range-and-the-ammunition",
        says: "Assessing a weapon draws the skill it uses, its damage bonus and modifier, its \
               speed as a word and a number, how far it reaches and what it shoots -- and a \
               modifier of exactly none prints as plus nothing rather than as a blank.",
        since: RETAIL,
        evidence: Evidence::Private("AC-EVID-F29-WEAPON"),
        station: "dereth-testkit::dat::panels::scenario_the_weapon_pane_gives_the_skill_the_speed_the_range_and_the_ammunition",
        tier: Tier::Dat,
    },
    behaviour! {
        id: "examine.window.a-component-the-spell-pane-selected-is-not-re-examined",
        says: "Picking one of a spell's components in the examine window selects that component \
               without examining it, so the window stays open on the spell; the next ordinary \
               change of selection re-examines as usual.",
        since: RETAIL,
        evidence: Evidence::Private("AC-EVID-O390-WINDOW"),
        station: "dereth-ui-screens::dat::panels::examine_window::selection_guard::a_component_this_panel_selected_is_not_re_examined_and_the_next_one_still_is",
        tier: Tier::Dat,
    },
    behaviour! {
        id: "examine.window.an-answer-the-pane-did-not-ask-for-opens-nothing-and-a-repeat-does-not-reopen-it",
        says: "The assessment window opens for the answer it asked for and for no other: an answer \
               nobody asked about, or one about a different thing, leaves it down -- and the same \
               answer arriving again after the player has closed the window refills it without \
               putting it back on the screen, which matters because the client asks again several \
               times a second while something is being fought.",
        since: RETAIL,
        evidence: Evidence::Private("AC-EVID-O289-GUARD"),
        station: "dereth-testkit::dat::panels::scenario_an_answer_the_pane_did_not_ask_for_opens_nothing",
        tier: Tier::Dat,
    },
    behaviour! {
        id: "examine.window.the-close-control-and-a-deselect-shut-it-and-the-combat-re-poll-does-not-reopen-it",
        says: "The examine window, once shut with its own close control, stays shut while the \
               combat refresh keeps re-examining the same object every three quarters of a second, \
               though each refresh still refills it; examining a different object opens it again.",
        since: RETAIL,
        evidence: Evidence::Private("AC-EVID-O376-WINDOW"),
        station: "dereth-ui-screens::dat::panels::examine_window::a_closed_window_stays_closed_under_the_combat_re_poll",
        tier: Tier::Dat,
    },
    behaviour! {
        id: "examine.window.the-identify-button-is-what-makes-the-pane-wait-for-an-answer",
        says: "Pressing the identify button with something selected is what makes the assessment \
               pane wait for that thing's answer, and the answer is what opens the window -- so the \
               whole chain from the press to the pane is one gesture and three arrivals.",
        since: RETAIL,
        evidence: Evidence::Private("AC-EVID-O289"),
        station: "dereth-testkit::dat::panels::scenario_the_identify_button_is_what_starts_the_chain",
        tier: Tier::Dat,
    },
    behaviour! {
        id: "house.abandon.two-confirmations-send-it-and-only-the-shards-answer-empties-the-pane",
        says: "Asking to abandon a house asks twice before anything leaves the client, the first \
               answer sends nothing at all, the second sends the one request -- and the House tab \
               still shows the house until the shard says it is gone, because a refusal leaves it \
               the player's.",
        since: RETAIL,
        evidence: Evidence::Private("AC-EVID-P1-130"),
        station: "dereth-testkit::dat::panels::scenario_two_confirmations_send_the_abandon_and_only_the_shard_empties_the_pane",
        tier: Tier::Dat,
    },
    behaviour! {
        id: "house.available.the-free-houses-the-shard-lists-are-written-out-with-their-locations",
        says: "When the shard answers with the houses of a kind that are free, the client writes \
               out how many there are and where each of them is, in the words and the coordinates \
               the player reads on the map. An apartment listing gives the count and no location \
               at all, however large the count is, and a listing longer than four hundred places \
               says so and stops.",
        since: RETAIL,
        evidence: Evidence::Private("AC-EVID-P4-4B-AVAILABLE"),
        station: "dereth-testkit::dat::panels::scenario_the_free_houses_the_shard_lists_are_written_out_with_their_locations",
        tier: Tier::Dat,
    },
    behaviour! {
        id: "house.commands.a-line-the-ladder-refuses-prints-the-shipped-sentence-and-sends-nothing",
        says: "Every house line the client will not act on -- the bare command, a word it does not \
               know, one that left the name off, one that named a kind of house there is none of \
               -- prints the shipped sentence for that refusal and puts nothing on the wire, and \
               none of them is answered with the client's own not-a-command sentence. The three \
               different ways of leaving a name off keep their three different sentences.",
        since: RETAIL,
        evidence: Evidence::Private("AC-EVID-P4-4B-REFUSALS"),
        station: "dereth-testkit::dat::panels::scenario_a_house_line_the_ladder_refuses_prints_the_shipped_sentence_and_sends_nothing",
        tier: Tier::Dat,
    },
    behaviour! {
        id: "house.commands.every-sub-command-that-sends-puts-its-own-message-on-the-wire",
        says: "Every house command a player can type that asks the shard for something builds \
               exactly its own message and nothing else, and that message is framed on the queue \
               the shard reads it from with the arguments the line carried -- the name typed after \
               it, which of the two words was used, which kind of house was named. Typing one \
               never leaves the client counting it as a command it does not know.",
        since: RETAIL,
        evidence: Evidence::Private("AC-EVID-P4-4B-LADDER"),
        station: "dereth-testkit::dat::panels::scenario_every_house_sub_command_that_sends_puts_its_own_message_on_the_wire",
        tier: Tier::Dat,
    },
    behaviour! {
        id: "house.commands.the-help-listing-and-the-house-help-are-what-the-client-ships",
        says: "Asking for help lists the subjects there is help on, and asking for help about \
               houses prints the shipped lines for every house command, both on the chat log. \
               Neither is answered with the client's own not-a-command sentence, a word there is \
               no help for is answered by saying so instead, and a slash works everywhere the at \
               sign does.",
        since: RETAIL,
        evidence: Evidence::Private("AC-EVID-P4-4B-HELP"),
        station: "dereth-testkit::dat::panels::scenario_the_help_listing_and_the_house_help_are_what_the_client_ships",
        tier: Tier::Dat,
    },
    behaviour! {
        id: "house.data.a-paid-up-apartment-drops-the-location-row-and-doubles-the-period",
        says: "An apartment whose maintenance is paid draws no location row at all, counts its \
               maintenance period in ninety days rather than thirty, says the next payment is two \
               whole periods away, and shows the paid sentence in its own colour.",
        since: RETAIL,
        evidence: Evidence::Private("AC-EVID-P1-19-APARTMENT"),
        station: "dereth-testkit::dat::panels::scenario_a_paid_up_apartment_drops_the_location_row_and_doubles_the_period",
        tier: Tier::Dat,
    },
    behaviour! {
        id: "house.data.a-transaction-answer-clears-the-pane-exactly-as-a-status-answer-does",
        says: "The two answers a shard can give about a house that is no longer the player's both \
               empty the House tab back to the two rows a houseless character sees.",
        since: RETAIL,
        evidence: Evidence::Private("AC-EVID-P4-4-TRANSACTION"),
        station: "dereth-testkit::dat::panels::scenario_a_transaction_answer_clears_the_pane_as_a_status_does",
        tier: Tier::Dat,
    },
    behaviour! {
        id: "house.data.a-zero-instant-prints-the-sentinel-and-not-the-start-of-the-epoch",
        says: "A house whose purchase instant is zero says the date is not available rather than \
               printing the first moment of 1970, while a zero maintenance instant still has a \
               period added to it and so is a real date.",
        since: RETAIL,
        evidence: Evidence::Private("AC-EVID-P1-19-SENTINEL"),
        station: "dereth-testkit::dat::panels::scenario_a_zero_instant_prints_the_sentinel_and_not_the_epoch",
        tier: Tier::Dat,
    },
    behaviour! {
        id: "house.data.the-shards-house-message-fills-every-row-of-the-tab",
        says: "When the shard describes the player's house the House tab draws all eight of its \
               rows in its own order -- the price, the rent, when it was bought, when the \
               maintenance period ends and is next due, where it is, the warning about unpaid \
               maintenance and when another may be bought -- where a character with no house has \
               only the last of those and the line saying they have none.",
        since: RETAIL,
        evidence: Evidence::Private("AC-EVID-P1-19"),
        station: "dereth-testkit::dat::panels::scenario_the_shards_house_message_fills_every_row_of_the_tab",
        tier: Tier::Dat,
    },
    behaviour! {
        id: "house.guests.the-guest-list-the-shard-sends-back-is-written-out-on-the-chat-log",
        says: "The guest list the shard answers with is written out on the chat log under its own \
               heading, one line per guest, with a mark beside the ones who may use the storage \
               and not beside the ones who may not. A house with no guests says so in one word \
               rather than printing an empty heading.",
        since: RETAIL,
        evidence: Evidence::Private("AC-EVID-P4-4B-GUESTS"),
        station: "dereth-testkit::dat::panels::scenario_the_guest_list_the_shard_sends_back_is_written_out_on_the_chat_log",
        tier: Tier::Dat,
    },
    behaviour! {
        id: "house.messages.every-house-message-the-shard-can-send-reaches-a-receiver",
        says: "Every message about housing that a shard can send to this client is taken by some \
               receiver rather than landing on the floor, and the count of them is measured rather \
               than looked up so that one going quiet shows up as a shortfall.",
        since: RETAIL,
        evidence: Evidence::Private("AC-EVID-P4-4-CENSUS"),
        station: "dereth-testkit::cpu::panels::scenario_every_house_message_the_shard_can_send_reaches_a_receiver",
        tier: Tier::Cpu,
    },
    behaviour! {
        id: "house.prices.the-price-rows-are-comma-joined-and-their-counts-are-grouped",
        says: "The two price rows of the House tab join their several payments with a comma and a \
               space, group the digits of every count the way the shipped language data says to, \
               and wrap onto as many lines as they need -- while the composer the other windows \
               share is left ungrouped.",
        since: RETAIL,
        evidence: Evidence::Private("AC-EVID-P1-75B"),
        station: "dereth-testkit::dat::panels::scenario_the_house_price_rows_are_comma_joined_and_grouped",
        tier: Tier::Dat,
    },
    behaviour! {
        id: "house.purchase-time.the-line-reads-what-retail-prints-in-each-of-its-three-arms",
        says: "The line about buying another house has three forms -- the wait has not run out and \
               names the date and says apartments are exempt, the wait has run out and the player \
               still owns one, and the wait has run out with no house at all -- and the client \
               draws the right one of the three, whole, with nothing cut off the end.",
        since: RETAIL,
        evidence: Evidence::Private("AC-EVID-P1-71"),
        station: "dereth-testkit::dat::panels::scenario_the_purchase_time_line_reads_what_retail_prints_in_each_arm",
        tier: Tier::Dat,
    },
    behaviour! {
        id: "house.query.the-client-asks-about-the-house-once-when-it-learns-who-it-is",
        says: "Learning who the player is makes the client ask the shard about their house, once \
               and once only however many times it is told again -- and that question is what the \
               whole House tab is waiting on.",
        since: RETAIL,
        evidence: Evidence::Private("AC-EVID-P4-4-QUERY"),
        station: "dereth-testkit::dat::panels::scenario_the_client_asks_about_the_house_once_when_it_learns_who_it_is",
        tier: Tier::Dat,
    },
    behaviour! {
        id: "house.rent.a-new-period-moves-both-dates-and-marks-every-payment-unpaid",
        says: "A shard starting a new maintenance period moves both of the tab's dates and puts \
               every payment back to nothing paid, so the tab keeps warning the player rather than \
               showing the last period's payment against the new one.",
        since: RETAIL,
        evidence: Evidence::Private("AC-EVID-P4-4-RENT-TIME"),
        station: "dereth-testkit::dat::panels::scenario_a_new_maintenance_period_clears_every_payment",
        tier: Tier::Dat,
    },
    behaviour! {
        id: "house.rent.a-payment-update-replaces-the-list-rather-than-merging-into-it",
        says: "A shard's new maintenance list is the list the tab draws -- a shorter one really is \
               shorter -- the purchase row above it does not move, and paying in full at once \
               changes the numbers, the warning sentence and its colour, and pushes the next \
               payment a whole extra period away.",
        since: RETAIL,
        evidence: Evidence::Private("AC-EVID-P4-4-RENT-PAYMENT"),
        station: "dereth-testkit::dat::panels::scenario_a_payment_update_replaces_the_list_rather_than_merging",
        tier: Tier::Dat,
    },
    behaviour! {
        id: "house.rent.neither-update-touches-a-character-with-no-house",
        says: "Both of the maintenance updates are taken and then deliberately ignored by a client \
               whose player owns no house: no house is invented out of them and nothing on the tab \
               is redrawn.",
        since: RETAIL,
        evidence: Evidence::Private("AC-EVID-P4-4-NULL-GUARD"),
        station: "dereth-testkit::dat::panels::scenario_neither_maintenance_update_touches_a_houseless_character",
        tier: Tier::Dat,
    },
    behaviour! {
        id: "house.restrictions.an-update-reaches-the-object-it-names-and-never-the-player",
        says: "A list of who may enter a house is stored against the object the shard names it \
               for, and one addressed at the player themselves is dropped rather than stored.",
        since: RETAIL,
        evidence: Evidence::Private("AC-EVID-P4-4-RESTRICTIONS"),
        station: "dereth-testkit::dat::panels::scenario_a_restriction_update_reaches_the_object_it_names",
        tier: Tier::Dat,
    },
    behaviour! {
        id: "house.rows.a-row-is-as-tall-as-the-text-it-holds-and-fits-inside-the-pane",
        says: "Every row of the House tab is made as tall as the text it holds, so a sentence that \
               wraps onto three lines is drawn on three lines inside its own row and inside the \
               pane, rather than having its tail laid outside the row and clipped away.",
        since: RETAIL,
        evidence: Evidence::Private("AC-EVID-P1-71-HEIGHT"),
        station: "dereth-testkit::dat::panels::scenario_a_house_row_is_as_tall_as_the_text_it_holds",
        tier: Tier::Dat,
    },
    behaviour! {
        id: "house.rows.the-paid-maintenance-line-is-drawn-in-the-colour-the-template-ships",
        says: "The sentence saying the maintenance is already paid is drawn in the second of the \
               three colours the row template ships, and every other row in the first -- so a tab \
               with the right words in the wrong colour is still wrong.",
        since: RETAIL,
        evidence: Evidence::Private("AC-EVID-P1-75-GREEN"),
        station: "dereth-testkit::dat::panels::scenario_the_paid_maintenance_line_is_drawn_in_the_templates_green",
        tier: Tier::Dat,
    },
    behaviour! {
        id: "journal.file.a-client-that-does-not-know-its-character-writes-no-notebook",
        says: "A client that has not been told which character it is playing writes no notebook at \
               all, rather than one shared file for every character on every world.",
        since: RETAIL,
        evidence: Evidence::Private("AC-EVID-G23-NAMELESS"),
        station: "dereth-testkit::dat::panels::scenario_a_client_that_does_not_know_its_character_writes_nothing",
        tier: Tier::Dat,
    },
    behaviour! {
        id: "journal.file.a-notebook-in-the-shipped-format-is-read-and-written-back-in-place",
        says: "A notebook another client wrote is read whole -- including a record this client \
               never writes and a note whose line break is stored as a tab -- and an edit made \
               here lands back in that same file.",
        since: RETAIL,
        evidence: Evidence::Private("AC-EVID-G23-FORMAT"),
        station: "dereth-testkit::dat::panels::scenario_a_notebook_in_the_shipped_format_is_read_and_written_back",
        tier: Tier::Dat,
    },
    behaviour! {
        id: "journal.file.a-page-written-here-is-on-disk-in-the-shipped-format-and-comes-back",
        says: "A page written in the journal is committed to a file named after the world and the \
               character, in the records the client writes and in their order -- and a client \
               started afresh beside that file comes up with the page in it.",
        since: RETAIL,
        evidence: Evidence::Private("AC-EVID-G23-JOURNAL"),
        station: "dereth-testkit::dat::panels::scenario_a_journal_page_survives_a_restart",
        tier: Tier::Dat,
    },
    behaviour! {
        id: "journal.file.a-relog-keeps-each-characters-notebook-and-saves-the-page-still-open",
        says: "Logging off and on again as somebody else opens that character's own notebook, \
               blank where they have none, without writing over the first character's -- and the \
               page the player was still looking at when they logged off is saved for the \
               character who was leaving rather than thrown away.",
        since: RETAIL,
        evidence: Evidence::Private("AC-EVID-P4-5"),
        station: "dereth-testkit::dat::panels::scenario_a_relog_keeps_each_characters_notebook",
        tier: Tier::Dat,
    },
    behaviour! {
        id: "journal.file.a-second-character-gets-a-notebook-of-their-own",
        says: "A second character's session points at a notebook of its own and comes up blank, \
               and writing in it does not write over the first character's file.",
        since: RETAIL,
        evidence: Evidence::Private("AC-EVID-G23-TWO"),
        station: "dereth-testkit::dat::panels::scenario_a_second_character_gets_its_own_notebook",
        tier: Tier::Dat,
    },
    behaviour! {
        id: "journal.list.a-double-press-on-a-row-opens-that-page-in-the-journal",
        says: "Two presses on a row of the page list inside the double-press window bring the \
               journal forward showing that row's own page.",
        since: RETAIL,
        evidence: Evidence::Private("AC-EVID-G9-OPEN-ROW"),
        station: "dereth-testkit::dat::panels::scenario_a_double_press_on_a_row_opens_that_page",
        tier: Tier::Dat,
    },
    behaviour! {
        id: "journal.list.a-pressed-row-is-the-one-the-delete-control-removes",
        says: "Pressing a row of the page list and then the delete control removes that row's page \
               and no other -- which is the only way to see that the press selected anything at \
               all, since a press over a row lands on the list and the list works out which row it \
               was.",
        since: RETAIL,
        evidence: Evidence::Private("AC-EVID-G32-DELETE"),
        station: "dereth-testkit::dat::panels::scenario_pressing_a_row_then_delete_removes_that_rows_page",
        tier: Tier::Dat,
    },
    behaviour! {
        id: "journal.list.the-list-is-the-journals-own-pages-in-page-order",
        says: "The page list holds one row per page of the journal, numbered and titled in page \
               order, with nothing in the timer column for a page whose timer is not running.",
        since: RETAIL,
        evidence: Evidence::Private("AC-EVID-G9-LIST"),
        station: "dereth-testkit::dat::panels::scenario_the_page_list_lists_the_journals_pages",
        tier: Tier::Dat,
    },
    behaviour! {
        id: "journal.list.the-reset-control-clears-the-search-and-deletes-no-page",
        says: "The reset control empties the search box and puts the whole list back with every \
               page still in the notebook -- and on a box that was never typed in it changes \
               nothing and still runs, which is what tells it apart from a control wired to the \
               delete arm.",
        since: RETAIL,
        evidence: Evidence::Private("AC-EVID-G32-RESET"),
        station: "dereth-testkit::dat::panels::scenario_reset_clears_the_search_and_deletes_nothing",
        tier: Tier::Dat,
    },
    behaviour! {
        id: "journal.list.typing-alone-does-not-filter-and-the-search-control-does",
        says: "Typing into the page list's search box narrows nothing by itself; pressing the \
               search control is what narrows the list to the pages whose words contain what was \
               typed, whatever the case.",
        since: RETAIL,
        evidence: Evidence::Private("AC-EVID-G32"),
        station: "dereth-testkit::dat::panels::scenario_only_the_search_control_filters_the_page_list",
        tier: Tier::Dat,
    },
    behaviour! {
        id: "journal.page.a-title-typed-on-one-page-is-there-on-the-way-back",
        says: "A title typed on one page of the journal is gone from the box on the next page and \
               back again on the way back, because turning the page writes the one being left down \
               and reads the one being opened.",
        since: RETAIL,
        evidence: Evidence::Private("AC-EVID-G9-TURN"),
        station: "dereth-testkit::dat::panels::scenario_a_typed_title_survives_a_page_turn",
        tier: Tier::Dat,
    },
    behaviour! {
        id: "journal.page.the-tab-opens-on-page-one-with-the-timer-ready-to-set",
        says: "The quest page opens on another tab; pressing the journal's own puts a notebook up, \
               open at page one, with nowhere recorded, the three boxes of the timer showing and \
               its countdown down.",
        since: RETAIL,
        evidence: Evidence::Private("AC-EVID-G9"),
        station: "dereth-testkit::dat::panels::scenario_the_journal_opens_on_page_one_with_the_timer_editable",
        tier: Tier::Dat,
    },
    behaviour! {
        id: "journal.shared-session-identity-and-save-boundaries",
        says: "Both interfaces read one notebook. Character identity changes isolate pages and reject stale loads; explicit save boundaries retain the outgoing draft without rereading on interface changes.",
        since: THIS_CLIENT,
        divergence: "CD-015",
        evidence: Evidence::Private("AC-EVID-DEDUP-JOURNAL-SESSION"),
        station: "dereth-client-model::lib::journal::tests::identity_reads_and_edits_keep_their_pages_and_save_boundaries",
        tier: Tier::Cpu,
    },
    behaviour! {
        id: "journal.timer.it-counts-down-on-its-own-and-the-button-puts-it-back",
        says: "A time typed into the journal's timer and started replaces the three boxes with a \
               countdown reading what was set, which runs down on its own with no further input -- \
               and pressing the button again puts the boxes back.",
        since: RETAIL,
        evidence: Evidence::Private("AC-EVID-G9-TIMER"),
        station: "dereth-testkit::dat::panels::scenario_the_journal_timer_counts_down_and_the_button_resets_it",
        tier: Tier::Dat,
    },
    behaviour! {
        id: "minigame.board.a-message-about-another-board-changes-nothing",
        says: "Everything the shard says about a board the player is not sitting at is read and \
               then refused: the pieces, which side the player is on, whose turn it is and which \
               board is being played are all exactly as they were, and the window says nothing \
               about it.",
        since: RETAIL,
        evidence: Evidence::Private("AC-EVID-P4-3-GUARD"),
        station: "dereth-testkit::dat::panels::scenario_a_message_about_another_board_changes_nothing",
        tier: Tier::Dat,
    },
    behaviour! {
        id: "minigame.board.the-opponents-move-is-replayed-on-the-players-own-board",
        says: "The opponent's move is played out on the player's own board, which is drawn from \
               the player's own side of it: the piece really leaves the square it was on and \
               appears on the one it went to, both squares draw the pictures the board itself \
               ships and really reach the frame, the square left behind draws nothing at all -- \
               and watching a move sends nothing while handing the turn back.",
        since: RETAIL,
        evidence: Evidence::Private("AC-EVID-P4-3-OPPONENT"),
        station: "dereth-testkit::dat::panels::scenario_the_opponents_move_is_replayed_on_the_players_own_board",
        tier: Tier::Dat,
    },
    behaviour! {
        id: "minigame.board.the-shards-answer-deals-the-board-or-takes-the-window-away",
        says: "A seat the shard gives deals a full board of thirty-two pieces in their opening \
               places and says so in the window; the shard then names which side moves first, and \
               the window says whether it is the player's turn or the opponent's. A seat the \
               shard refuses says so, deals nothing at all, and puts the window back to no game.",
        since: RETAIL,
        evidence: Evidence::Private("AC-EVID-P4-3-DEAL"),
        station: "dereth-testkit::dat::panels::scenario_the_seat_deals_the_board_and_the_start_names_whose_turn_it_is",
        tier: Tier::Dat,
    },
    behaviour! {
        id: "minigame.board.two-presses-move-a-piece-and-a-move-the-rules-refuse-never-leaves",
        says: "Pressing a square of the board picks up what is standing on it and asks for \
               nothing; pressing a second square asks the shard for exactly that move, from that \
               square to that one, and says the move is on its way. A move the game's own rules \
               will not allow is refused where the player made it, with the reason and an \
               invitation to try again, and nothing leaves the client. A move the shard refuses \
               puts the piece back where it stood and gives the turn back.",
        since: RETAIL,
        evidence: Evidence::Private("AC-EVID-P4-3-MOVE"),
        station: "dereth-testkit::dat::panels::scenario_two_presses_move_a_piece_and_a_refused_move_never_leaves",
        tier: Tier::Dat,
    },
    behaviour! {
        id: "minigame.board.using-a-board-raises-the-window-and-asks-for-a-seat",
        says: "Using a game board that is fixed in the world raises the game window -- already \
               built, with all sixty-four squares and its three buttons -- and asks the shard for \
               a seat at that board without asking for a side, saying so on screen. Using the \
               same board again, or a second board while the first game is still going, tells the \
               player which of the two it is and asks the shard for nothing.",
        since: RETAIL,
        evidence: Evidence::Private("AC-EVID-P4-3-JOIN"),
        station: "dereth-testkit::dat::panels::scenario_using_a_board_raises_the_window_and_asks_for_a_seat",
        tier: Tier::Dat,
    },
    behaviour! {
        id: "minigame.indicator.the-lamp-is-lit-from-the-first-use-until-the-game-is-over",
        says: "The little game lamp along the indicator strip is dark until a board is used, then \
               lights with the picture it ships for that -- really drawn in the frame, not merely \
               switched on -- and keeps that same picture through the seat, the start, the \
               player's own move and the opponent's. When the game ends the lamp goes dark again \
               and the board window goes down with it.",
        since: RETAIL,
        evidence: Evidence::Private("AC-EVID-P4-3-LAMP"),
        station: "dereth-testkit::dat::panels::scenario_the_game_lamp_is_lit_from_the_first_use_until_the_game_is_over",
        tier: Tier::Dat,
    },
    behaviour! {
        id: "minigame.messages.every-message-the-game-speaks-reaches-a-receiver-or-a-sender",
        says: "Every message the board game is played in reaches something: each of the six the \
               shard can send is taken by a part of the client rather than dropped on the floor, \
               and each of the five the client can send has a sender behind it and is addressed \
               to the shard.",
        since: RETAIL,
        evidence: Evidence::Private("AC-EVID-P4-3-CENSUS"),
        station: "dereth-testkit::dat::panels::scenario_every_chess_message_reaches_a_receiver_or_a_sender",
        tier: Tier::Dat,
    },
    behaviour! {
        id: "minigame.window.the-buttons-offer-a-stalemate-and-ask-before-resigning",
        says: "Offering a draw turns the offer on and tells the shard it is on; pressing again \
               turns it off and tells the shard that. Resigning asks first, in the window's own \
               words, with a Yes and a No; a second press while the question is up asks nothing \
               more; No closes it, sends nothing and leaves the game running so it can be asked \
               again; and only Yes quits the game and takes the window down. The button that \
               passes a turn is never shown to the player at all, though the client still says so \
               when there is no game to pass in and still sends the pass when there is.",
        since: RETAIL,
        evidence: Evidence::Private("AC-EVID-P4-3-BUTTONS"),
        station: "dereth-testkit::dat::panels::scenario_the_window_buttons_offer_a_draw_and_ask_before_resigning",
        tier: Tier::Dat,
    },
    behaviour! {
        id: "minigame.window.the-offer-of-a-stalemate-and-the-end-of-a-game-are-said-in-the-window",
        says: "An opponent offering a draw is said in the window, and so is their taking the \
               offer back -- neither of which turns on the player's own offer, because agreeing \
               is pressing your own button. When the game ends with the player's side named the \
               winner the window says they are victorious, says how to start another, and puts \
               itself back to no game, no side and no board.",
        since: RETAIL,
        evidence: Evidence::Private("AC-EVID-P4-3-ENDGAME"),
        station: "dereth-testkit::dat::panels::scenario_the_offer_of_a_draw_and_the_end_of_a_game_are_said_in_the_window",
        tier: Tier::Dat,
    },
    behaviour! {
        id: "panels.barber.apply-sends-one-exact-finish-without-mutating-the-player",
        says: "Pressing Apply in the barber window sends one Character_FinishBarber carrying all \
               sixteen appearance fields in retail order and closes the window; the player's look \
               does not change until the shard's own appearance update arrives.",
        since: RETAIL,
        evidence: Evidence::Private("AC-EVID-P25B-BARBER"),
        station: "dereth-client::gpu::panels::barber::physical_apply_sends_one_exact_finish_without_optimistic_player_mutation",
        tier: Tier::Gpu,
    },
    behaviour! {
        id: "panels.book.next-adds-a-page-and-the-reply-makes-it-editable",
        says: "Pressing Next on the last page of a book the player may write in asks the shard to \
               add a page; once the shard agrees the new blank page is selected and can be written \
               on, leaving it sends the written text once, and coming back shows that text without \
               asking for another page.",
        since: RETAIL,
        evidence: Evidence::Private("AC-EVID-P26B-BOOK"),
        station: "dereth-client::gpu::panels::book_authoring::next_adds_a_page_and_the_reply_makes_it_a_real_editable_saved_page",
        tier: Tier::Gpu,
    },
    behaviour! {
        id: "panels.house-purchase.an-apartment-buy-pays-on-the-click",
        says: "Buying an apartment pays on the first click of the buy button with no confirmation \
               question, since the thirty-day restriction the question warns about does not apply \
               to apartments; the purchase goes to the shard at once with the items in the window.",
        since: RETAIL,
        evidence: Evidence::Private("AC-EVID-P44C-HOUSE-PURCHASE"),
        station: "dereth-client::gpu::panels::house_purchase_window::an_apartment_buy_pays_on_the_click_with_no_confirmation",
        tier: Tier::Gpu,
    },
    behaviour! {
        id: "panels.house-purchase.confirmations-use-current-payment-and-handle-every-answer",
        says: "A landscape purchase and maintenance paid for another owner ask for confirmation \
               once while the question is open. Yes rechecks the current payment, No closes the \
               window, and a missing answer releases the question so the player can try again.",
        since: RETAIL,
        evidence: Evidence::Private("AC-EVID-HOUSING-CONFIRMATION-ANSWERS"),
        station: "dereth-classic-ui::lib::panels::services::world::housing::tests::housing_questions_use_shared_confirmation_and_handle_every_answer",
        tier: Tier::Cpu,
    },
    behaviour! {
        id: "panels.house-purchase.each-payment-is-paid-in-full-by-its-own-price",
        says: "The purchase window counts the purchase as paid in full only when everything in \
               the purchase price has been offered, and the maintenance only when everything in \
               the rent has; paying one of them in full never counts the other as paid.",
        since: RETAIL,
        evidence: Evidence::Private("AC-EVID-HOUSING-PAID-IN-FULL-PER-LIST"),
        station: "dereth-client-model::lib::housing::payments::tests::each_payment_is_paid_in_full_only_by_its_own_price",
        tier: Tier::Cpu,
    },
    behaviour! {
        id: "panels.house-purchase.each-profile-opens-the-payment-window-once",
        says: "Each received house profile opens the payment window once. Closing it keeps it \
               closed until another profile arrives, including a repeated use of the same house.",
        since: RETAIL,
        evidence: Evidence::Private("AC-EVID-HOUSING-PROFILE-OPENING"),
        station: "dereth-classic-ui::lib::runtime::house_profile_tests::a_house_profile_opens_once_and_a_repeated_use_reopens_after_close",
        tier: Tier::Cpu,
    },
    behaviour! {
        id: "panels.house-purchase.paying-sends-buy-house-with-the-windows-items",
        says: "Paying for a house sends one House_BuyHouse naming the items in the order they were \
               dropped into the purchase window, byte for byte as the recorded retail client sent \
               it, then empties the window and greys the Buy button.",
        since: RETAIL,
        evidence: Evidence::Private("AC-EVID-P44C-HOUSE-PURCHASE-PAYING"),
        station: "dereth-client::gpu::panels::house_purchase_window::paying_sends_buy_house_with_the_items_in_the_windows_own_order",
        tier: Tier::Gpu,
    },
    behaviour! {
        id: "panels.layout.a-taller-panel-keeps-its-list-rows-in-place",
        says: "A panel made taller than its default, by a saved screen layout or by dragging its \
               edge, stretches its lists with it, and every row of a list stays where the list \
               laid it out, one under the next: a row is never put back at its own template \
               position by the resize.",
        since: RETAIL,
        evidence: Evidence::Private("AC-EVID-TALL-PANEL-LIST-ROWS"),
        station: "dereth-ui-screens::dat::panels::tall_panels::a_taller_panel_keeps_every_list_row_where_the_list_put_it",
        tier: Tier::Dat,
    },
    behaviour! {
        id: "panels.quickbar.shows-nine-numbered-slots-and-their-items",
        says: "The toolbar's first row of nine shortcut slots shows the numbers one to nine as \
               nine different empty pictures, while the second row of nine, which lies outside the \
               window, shares one.",
        since: RETAIL,
        evidence: Evidence::Private("AC-EVID-QUICKBAR-QUICKBAR"),
        station: "dereth-client::gpu::panels::quickbar::the_nine_bank_one_tiles_carry_nine_distinct_empty_state_numerals",
        tier: Tier::Gpu,
    },
    behaviour! {
        id: "panels.radar.a-monarch-update-reshapes-another-players-blip",
        says: "When the shard tells the player who his monarch is, another player on the radar who \
               shares that allegiance changes at once from the plain cross to the hollow box; an \
               update carrying an older stamp changes nothing, and one carrying an equal stamp is \
               taken.",
        since: RETAIL,
        evidence: Evidence::Private("AC-EVID-P23-RADAR"),
        station: "dereth-client::gpu::panels::radar_allegiance_shape::player_monarch_updates_refresh_another_visible_players_radar_shape",
        tier: Tier::Gpu,
    },
    behaviour! {
        id: "panels.radar.environs-six-blanks-the-radar-and-zero-restores-it",
        says: "An admin environment change to setting six blanks the radar down to the player's \
               own mark and closes the world in with dark fog until the sky is black; a setting \
               the client does not know changes nothing, and setting zero brings the radar's blips \
               back.",
        since: RETAIL,
        evidence: Evidence::Private("AC-EVID-P3-RADAR"),
        station: "dereth-client::gpu::panels::radar_environs::encoded_environs_six_blanks_the_radar_and_zero_restores_it",
        tier: Tier::Gpu,
    },
    behaviour! {
        id: "panels.radar.the-coordinate-read-out-follows-the-bodys-cell",
        says: "The coordinates under the radar follow the land cell the body stands in: running \
               within one cell leaves them as they were, and crossing into the next cell north \
               moves the northing up by a tenth.",
        since: RETAIL,
        evidence: Evidence::Private("AC-EVID-P157-RADAR"),
        station: "dereth-client::gpu::panels::radar_coordinates::walking_across_a_land_cell_boundary_moves_the_radar_read_out",
        tier: Tier::Gpu,
    },
    behaviour! {
        id: "panels.range.a-vendor-ground-container-or-trade-closes-when-the-player-walks-away",
        says: "A shop window stays open while the player stays within the vendor's own use \
               distance and closes once, by itself, the moment he walks beyond it, disturbing \
               nothing else.",
        since: RETAIL,
        evidence: Evidence::Private("AC-EVID-O385-RANGE"),
        station: "dereth-client-model::cpu::panels::close_on_leaving_range::a_vendor_window_closes_when_the_player_leaves_its_use_radius",
        tier: Tier::Cpu,
    },
    behaviour! {
        id: "panels.skills.the-skills-page-draws-the-captures-skills",
        says: "The skills page shows the character's own skills from the shard's player \
               description, each under the heading for how far it is trained, with its name, its \
               icon and a value.",
        since: RETAIL,
        evidence: Evidence::Private("AC-EVID-PANEL-CONTENTS-SKILLS"),
        station: "dereth-client::gpu::panels::character_panels::the_skills_page_draws_the_captures_own_skills_with_their_names_and_values",
        tier: Tier::Gpu,
    },
    behaviour! {
        id: "panels.spellbook.the-filter-hides-a-school",
        says: "Turning one school of magic off in the spellbook filter hides exactly that school's \
               spells and keeps the rest, and the filter the player saved reads back unchanged.",
        since: RETAIL,
        evidence: Evidence::Private("AC-EVID-PANEL-CONTENTS-SPELLBOOK"),
        station: "dereth-client::gpu::panels::character_panels::the_filter_hides_a_school_and_the_list_follows",
        tier: Tier::Gpu,
    },
    behaviour! {
        id: "panels.stack.opposite-visibility-notices-for-one-page-settle-on-the-last",
        says: "When one panel page is shown and hidden again within a single frame, the panel \
               window settles at once on the last of the two, so the spellbook stays down and is \
               not the current panel, rather than bouncing between them.",
        since: RETAIL,
        evidence: Evidence::Private("AC-EVID-PANEL-VISIBILITY-SETTLES-STACK"),
        station: "dereth-ui-screens::dat::panels::panel_stack_visibility::an_opposite_parity_pair_of_visibility_notices_for_one_page_settles",
        tier: Tier::Dat,
    },
    behaviour! {
        id: "panels.tabs.a-tabbed-page-shows-one-sub-panel",
        says: "Each of the six tabbed panels -- the map, options, character, quests, social and \
               spells -- shows one of its pages at a time, although the shipped layout marks them \
               all visible.",
        since: RETAIL,
        evidence: Evidence::Private("AC-EVID-PANELS-TABS"),
        station: "dereth-client::gpu::panels::inventory_and_tabs::a_tabbed_page_shows_exactly_one_of_its_sub_panels",
        tier: Tier::Gpu,
    },
    behaviour! {
        id: "qualities.remove.each-kind-is-deleted-rather-than-set-back-to-its-own-zero",
        says: "A property the shard clears is afterwards absent rather than set back to nothing, \
               for every one of the eight kinds of property a character can carry -- and a \
               neighbouring property deliberately holding that same kind's own zero is still \
               there, so absent and zero stay different answers.",
        since: RETAIL,
        evidence: Evidence::Private("AC-EVID-P2-3-KINDS"),
        station: "dereth-testkit::cpu::panels::scenario_every_kind_of_clearing_deletes_the_key_and_absent_is_not_zero",
        tier: Tier::Cpu,
    },
    behaviour! {
        id: "qualities.remove.every-one-of-the-sixteen-clearing-messages-reaches-a-receiver",
        says: "Every one of the sixteen ways a shard can tell this client to clear a property -- \
               eight kinds of property, each for the player alone and for any other body -- \
               reaches something that takes it rather than the floor, while a message this client \
               genuinely does not receive is still reported as unreceived and named.",
        since: RETAIL,
        evidence: Evidence::Private("AC-EVID-P2-3-SIXTEEN"),
        station: "dereth-testkit::cpu::panels::scenario_every_clearing_message_the_shard_can_send_reaches_a_receiver",
        tier: Tier::Cpu,
    },
    behaviour! {
        id: "qualities.remove.one-naming-another-body-reaches-it-and-moves-no-mark-of-its-own",
        says: "A message clearing a property of some other body reaches that body and leaves the \
               player's own properties alone -- and, unlike a message that changes a property, it \
               moves none of the marks a body is drawn and treated by, so somebody marked a player \
               killer stays marked until the shard says otherwise. Only the player has anywhere to \
               keep a property at all, so what a clearing reached is read off those marks and off \
               the order the shard's messages are accepted in.",
        since: RETAIL,
        evidence: Evidence::Private("AC-EVID-P2-3-PUBLIC"),
        station: "dereth-testkit::cpu::panels::scenario_a_public_clearing_reaches_the_object_it_names_and_runs_no_mirror",
        tier: Tier::Cpu,
    },
    behaviour! {
        id: "qualities.remove.one-older-than-the-change-it-would-undo-is-refused",
        says: "A message clearing a property that is older than the change which set it is refused \
               and counted as out of date rather than emptying what the newer one put there, and a \
               change older than a clearing that has already landed is refused the same way -- the \
               two are one conversation about one property and share one place in it.",
        since: RETAIL,
        evidence: Evidence::Private("AC-EVID-P2-3-STALE"),
        station: "dereth-testkit::cpu::panels::scenario_a_clearing_older_than_the_change_it_would_undo_is_refused",
        tier: Tier::Cpu,
    },
    behaviour! {
        id: "skills.enlightenment.each-level-adds-one-to-every-trained-and-specialized-skill",
        says: "A character who carries an enlightenment count has that count added to every \
               trained or specialized skill and to no untrained one. It is part of the skill's \
               base, so an enchantment that multiplies the skill scales it too, and a character \
               who never enlightened reads exactly as before.",
        since: RETAIL,
        evidence: Evidence::Private("AC-EVID-SKILLS-ENLIGHTENMENT"),
        station: "dereth-client-model::lib::skills::tests::enlightenment_adds_to_trained_and_specialized_skills_before_the_multiplier",
        tier: Tier::Cpu,
    },
    behaviour! {
        id: "skills.footer.the-experience-line-is-legible-and-nothing-is-painted-over-it",
        says: "Selecting a skill puts the words Experience To Raise under it, composed in a real \
               font at an opaque colour, and nothing the panel draws afterwards covers them -- the \
               meter behind the line is painted first.",
        since: RETAIL,
        evidence: Evidence::Private("AC-EVID-P1-7-XP"),
        station: "dereth-testkit::dat::panels::scenario_the_experience_to_raise_line_is_legible_and_uncovered",
        tier: Tier::Dat,
    },
    behaviour! {
        id: "skills.footer.the-numbers-under-a-picked-skill-are-the-characters-own",
        says: "Picking a skill the character has already trained fills the panel under the list \
               with that skill's own numbers: what the next point of it costs, how much unspent \
               experience there is to pay with, and how far through the current point the skill \
               already is. A skill that can go no further says so instead of showing a price, and \
               each of the two raise buttons lights up only when the unspent experience covers \
               what that button would spend.",
        since: RETAIL,
        evidence: Evidence::Private("AC-EVID-O65-FOOTER"),
        station: "dereth-testkit::dat::panels::scenario_the_footer_under_a_picked_skill_is_that_characters_own_arithmetic",
        tier: Tier::Dat,
    },
    behaviour! {
        id: "skills.footer.the-title-is-the-name-and-the-number-with-the-change-spelled-out-after-it",
        says: "Selecting a skill titles the footer with the skill's name, a colon and the number \
               the player actually has; when a spell has changed it the difference follows in \
               brackets, signed, green when it is a gain and red when it is a loss.",
        since: RETAIL,
        evidence: Evidence::Private("AC-EVID-P1-7-TITLE"),
        station: "dereth-testkit::dat::panels::scenario_the_footer_title_is_the_name_the_number_and_the_signed_change",
        tier: Tier::Dat,
    },
    behaviour! {
        id: "skills.list.a-header-per-group-and-a-row-per-skill-the-shard-sent",
        says: "The skills page builds one heading per skill group and one row per skill the shard \
               said the character has, and every one of those rows is really created rather than \
               quietly skipped.",
        since: RETAIL,
        evidence: Evidence::Private("AC-EVID-O182-ROWS"),
        station: "dereth-testkit::dat::panels::scenario_the_skills_page_builds_a_header_per_group_and_a_row_per_recorded_skill",
        tier: Tier::Dat,
    },
    behaviour! {
        id: "skills.list.a-page-that-built-nothing-is-told-apart-from-one-that-built-everything",
        says: "A skills page that came up with nothing at all is distinguishable from one that \
               built every row: the list reports what it created, not only what it failed to \
               create, so an empty page cannot read as a healthy one.",
        since: RETAIL,
        evidence: Evidence::Private("AC-EVID-O182-GUARD"),
        station: "dereth-testkit::dat::panels::scenario_an_empty_skills_page_is_distinguishable_from_a_full_one",
        tier: Tier::Dat,
    },
    behaviour! {
        id: "skills.numbers.the-experience-numbers-are-grouped-with-the-shipped-separator",
        says: "The large numbers on the character pages are grouped three digits at a time with \
               the separator the shipped language data gives -- which is the same separator the \
               client's own experience formatter uses, negative numbers included. The total \
               experience, the experience to the next level and the unassigned experience all \
               carry it, before a row is picked and after one is; a skill row's own value is \
               printed plainly and is not grouped, which is the difference between grouping the \
               right numbers and grouping every number.",
        since: RETAIL,
        evidence: Evidence::Private("AC-EVID-O422-SEPARATOR"),
        station: "dereth-testkit::dat::panels::scenario_the_character_pages_numbers_carry_the_shipped_separator",
        tier: Tier::Dat,
    },
    behaviour! {
        id: "skills.raise.the-button-spends-experience-on-a-trained-skill-and-credits-on-an-untrained-one",
        says: "Pressing the button under a picked skill asks the shard to raise it for exactly \
               what the panel said it would cost, and the two kinds of skill are asked for \
               differently: a skill the character already has is raised with experience, while \
               one they do not have yet is trained with skill credits, which the panel says in \
               those words and prices out of the shipped table. Nothing is spent on the client's \
               own copy of the character, and while the client is waiting for the shard's answer \
               the button greys out, so a second press asks for nothing and the same experience \
               cannot be spent twice.",
        since: RETAIL,
        evidence: Evidence::Private("AC-EVID-O65-RAISE"),
        station: "dereth-testkit::dat::panels::scenario_the_raise_button_spends_experience_or_credits_by_what_the_skill_is",
        tier: Tier::Dat,
    },
    behaviour! {
        id: "skills.rows.a-character-who-logs-in-already-enchanted-reads-the-same-total",
        says: "A character who logs in already carrying spells on a skill sees the same number as \
               one who had those spells cast on them while playing -- the two paths agree.",
        since: RETAIL,
        evidence: Evidence::Private("AC-EVID-P1-74-RELOG"),
        station: "dereth-testkit::dat::panels::scenario_a_character_who_logs_in_already_enchanted_reads_the_same_total",
        tier: Tier::Dat,
    },
    behaviour! {
        id: "skills.rows.a-spell-cast-while-playing-moves-the-skill-row-it-is-about",
        says: "A spell that lands while the player is playing moves the skill row it is about and \
               colours it, two spells on one skill add up, a spell that lowers a skill lowers it, \
               and a spell on an attribute lifts the skills worked out from that attribute -- \
               while every other row stays where it was.",
        since: RETAIL,
        evidence: Evidence::Private("AC-EVID-P1-74-LIVE"),
        station: "dereth-testkit::dat::panels::scenario_a_spell_cast_while_playing_moves_the_skill_row_it_is_about",
        tier: Tier::Dat,
    },
    behaviour! {
        id: "skills.rows.a-weakened-character-shows-the-lower-number-without-calling-it-lowered",
        says: "A character weakened by dying reads the reduced number on every skill row, rounded \
               the way the page rounds it rather than simply cut short, and those rows are drawn \
               plain: the weakening is not something done to any one skill, so a whole list \
               painted as lowered is wrong. A skill something has raised while the weakening is \
               in force is still drawn as raised, even though the number shown is below what the \
               character trained to.",
        since: RETAIL,
        evidence: Evidence::Private("AC-EVID-O459-VITAE"),
        station: "dereth-testkit::dat::panels::scenario_a_weakened_character_shows_the_lower_number_still_drawn_plain",
        tier: Tier::Dat,
    },
    behaviour! {
        id: "skills.rows.every-row-draws-its-value-in-a-font-that-exists-and-the-colour-it-declares",
        says: "Every skill row draws its number, in a font that really exists rather than in no \
               font at all, and in the colour the row's own list of colours names for it -- that \
               list being one font and three colours for the value and one of each for the name \
               beside it. The numbers themselves, and the order the rows come in, are the recorded \
               character's own, pinned one by one. A colour or a font asked for past the end of \
               either list falls back to the first entry rather than to nothing.",
        since: RETAIL,
        evidence: Evidence::Private("AC-EVID-O422-VALUE-FONT"),
        station: "dereth-testkit::dat::panels::scenario_every_skill_row_draws_its_value_in_a_font_that_exists",
        tier: Tier::Dat,
    },
    behaviour! {
        id: "skills.rows.the-colour-a-value-is-drawn-in-is-one-of-the-rows-own-three",
        says: "A skill's number is drawn in one of the three colours the row itself ships -- \
               plain when nothing has touched the skill, the raised colour when something has \
               raised it, the lowered colour when something has lowered it -- and always in the \
               one lettering the row carries, so the number is never asked for a lettering that \
               is not there and never comes out invisible. A colour past the three the row has \
               leaves the number plain rather than blanking the cell.",
        since: RETAIL,
        evidence: Evidence::Private("AC-EVID-O65-FONT"),
        station: "dereth-testkit::dat::panels::scenario_a_skill_values_colour_is_one_of_the_rows_own_three",
        tier: Tier::Dat,
    },
    behaviour! {
        id: "skills.rows.the-number-shown-is-the-total-after-everything-and-the-colour-says-which-way",
        says: "A skill row shows the number the character actually has once everything acting on \
               that skill is counted, and not the number they trained up to; it is drawn green \
               when something has raised it, red when something has lowered it and white when \
               nothing has touched it. The colour is per row, so a raised skill and a lowered one \
               sit beside untouched ones on the same page.",
        since: RETAIL,
        evidence: Evidence::Private("AC-EVID-O459-VALUE"),
        station: "dereth-testkit::dat::panels::scenario_a_skill_row_shows_the_total_after_everything_in_the_colour_of_the_change",
        tier: Tier::Dat,
    },
    behaviour! {
        id: "skills.scroll.a-press-on-a-scrolled-list-picks-the-row-it-is-drawn-over",
        says: "With the skills list scrolled, a press picks the skill whose row is drawn under the \
               pointer and not the one that would have been there had the list never moved -- so \
               the skills below the fold can be reached by a real click, where before only the \
               few at the top could be.",
        since: RETAIL,
        evidence: Evidence::Private("AC-EVID-O458-HITTEST"),
        station: "dereth-testkit::dat::panels::scenario_a_press_on_a_scrolled_skills_list_picks_the_row_it_is_drawn_over",
        tier: Tier::Dat,
    },
    behaviour! {
        id: "skills.scroll.each-arrow-moves-the-list-one-row-its-own-way-and-the-top-is-the-top",
        says: "The arrow at the foot of the skills list's bar moves the list down by exactly one \
               row and the arrow at its head moves it back up by one, one press at a time; at the \
               top of the list a further press upwards goes nowhere.",
        since: RETAIL,
        evidence: Evidence::Private("AC-EVID-O458-ARROWS"),
        station: "dereth-testkit::dat::panels::scenario_each_arrow_moves_the_skills_list_one_row_its_own_way",
        tier: Tier::Dat,
    },
    behaviour! {
        id: "skills.scroll.rebuilding-the-list-keeps-where-it-was-scrolled-to",
        says: "Rebuilding the skills list while it is scrolled leaves it scrolled where it was \
               and puts the new rows back against that place in the same frame, so a player two \
               thirds down the list who trains a skill does not find the rows somewhere else \
               while the page catches up.",
        since: RETAIL,
        evidence: Evidence::Private("AC-EVID-O458-REBUILD"),
        station: "dereth-testkit::dat::panels::scenario_rebuilding_the_skills_list_keeps_where_it_was_scrolled_to",
        tier: Tier::Dat,
    },
    behaviour! {
        id: "skills.scroll.the-bar-beside-the-list-says-how-much-is-in-view-and-where-it-is",
        says: "The skills list has a bar down its side and none along its foot; the block on that \
               bar covers as much of it as the part of the list in view covers of the whole list, \
               it sits as far down the bar as the list has been scrolled, and scrolling moves \
               where the block sits without changing how big it is.",
        since: RETAIL,
        evidence: Evidence::Private("AC-EVID-O458-THUMB"),
        station: "dereth-testkit::dat::panels::scenario_the_bar_beside_the_skills_list_reports_the_view_and_where_it_is",
        tier: Tier::Dat,
    },
    behaviour! {
        id: "skills.scroll.the-list-is-one-column-of-equal-rows-and-most-of-it-is-out-of-sight",
        says: "The skills page stacks every group heading and every skill row in a single column \
               of equal height, and the distance it has to scroll through is the sum of them -- \
               which on a real character is over five times what the box shows, so only the first \
               few are in view and the rest are out of sight until the list is scrolled.",
        since: RETAIL,
        evidence: Evidence::Private("AC-EVID-O458-EXTENT"),
        station: "dereth-testkit::dat::panels::scenario_the_skills_list_is_one_column_of_equal_rows_mostly_out_of_sight",
        tier: Tier::Dat,
    },
    behaviour! {
        id: "skills.selection.a-press-picks-a-skill-and-a-second-press-puts-the-footer-back",
        says: "The skills page comes up with nothing picked and with the panel under the list \
               asking the player to pick a skill to improve, the credits and the unspent \
               experience named beneath it and no raise button there at all. Pressing a row picks \
               that row and only that row, pressing the same row again unpicks it and puts the \
               page back exactly as it came up, and pressing a different row moves the pick \
               rather than adding a second one.",
        since: RETAIL,
        evidence: Evidence::Private("AC-EVID-O65-SELECTION"),
        station: "dereth-testkit::dat::panels::scenario_the_skills_page_opens_asking_for_a_pick_and_a_press_picks_one",
        tier: Tier::Dat,
    },
    behaviour! {
        id: "skills.selection.a-press-picks-the-skill-row-under-it-and-the-other-page-stays-put",
        says: "Pressing a skill row picks that skill, and only that skill: the attributes page, \
               whose own list sits at the same place on the screen and is offered the press first, \
               does not move. The pointer lands on the list rather than on the row, pressing the \
               same row again un-picks it, and neither press sends anything to the shard.",
        since: RETAIL,
        evidence: Evidence::Private("AC-EVID-O422-SKILL-SELECT"),
        station: "dereth-testkit::dat::panels::scenario_a_press_picks_the_skill_row_under_it_and_the_attribute_page_stays_put",
        tier: Tier::Dat,
    },
    behaviour! {
        id: "spellbook.filter.turning-a-school-off-hides-its-spells-and-sends-the-whole-list",
        says: "The spellbook's school buttons come up already showing which schools the shard \
               said this character is looking at. Turning one off takes every spell of that \
               school out of the book on screen and leaves the rest where they were; turning it \
               back on brings exactly the same book back, so the buttons filter the book rather \
               than edit it. Each press tells the shard the whole list of schools now showing and \
               not the one that changed.",
        since: RETAIL,
        evidence: Evidence::Private("AC-EVID-O65-FILTER"),
        station: "dereth-testkit::dat::panels::scenario_turning_a_school_off_hides_its_spells_and_sends_the_whole_list",
        tier: Tier::Dat,
    },
    behaviour! {
        id: "stats.classic.next-level-at-the-cap-reads-infinity",
        says: "The classic character page writes INFINITY for the experience to the next level once nothing more is owed: at the experience table's last level, and at the capped account's level 126. Below the cap it writes the grouped number.",
        since: THIS_CLIENT,
        divergence: "CD-015",
        evidence: Evidence::Private("AC-EVID-CLASSIC-NEXT-LEVEL-INFINITY"),
        station: "dereth-classic-ui::lib::panels::game::stats::tests::the_experience_for_the_next_level_reads_infinity_once_nothing_more_is_owed",
        tier: Tier::Cpu,
    },
    behaviour! {
        id: "stats.classic.skill-cost-meter-follows-stretched-footer",
        says: "The skill advancement meter and its caption stay on the same footer row when the Classic interface is stretched.",
        since: THIS_CLIENT,
        divergence: "CD-015",
        evidence: Evidence::Private("AC-EVID-CLASSIC-SKILL-METER"),
        station: "dereth-classic-ui::lib::panels::game::stats::tests::skill_cost_meter_tracks_the_stretched_footer",
        tier: Tier::Cpu,
    },
    behaviour! {
        id: "stats.presentation.preserves-display-variants-and-fractional-vitae",
        says: "The character panels share stat and title facts while preserving their skill \
               grouping and text conventions. Fractional loss of strength is rounded before \
               display; Classic retains at least one percent for a positive loss, while Modern can \
               show full strength.",
        since: RETAIL,
        evidence: Evidence::Private("AC-EVID-CHARACTER-PRESENTATION"),
        station: "dereth-presentation::lib::stats::tests::fractional_vitae_uses_variant_rounding_without_narrowing",
        tier: Tier::Cpu,
    },
    behaviour! {
        id: "titles.tab.picking-a-row-arms-the-button-only-for-a-title-not-already-worn",
        says: "Picking a title on the Titles tab arms the button that wears one only when the \
               title picked is not the one already worn -- and picking sends nothing, so a stray \
               press cannot rewrite which title the character is wearing.",
        since: RETAIL,
        evidence: Evidence::Private("AC-EVID-O936-PICK"),
        station: "dereth-testkit::dat::panels::scenario_picking_a_title_arms_the_button_and_sends_nothing",
        tier: Tier::Dat,
    },
    behaviour! {
        id: "titles.tab.the-button-sends-one-request-for-the-picked-title-and-nothing-local-moves",
        says: "Pressing the button with nothing picked sends nothing; a row pressed and then the \
               button sends exactly one request carrying that row's own title -- and nothing on \
               the client moves, because which title is worn is the shard's to say.",
        since: RETAIL,
        evidence: Evidence::Private("AC-EVID-O936-SEND"),
        station: "dereth-testkit::dat::panels::scenario_the_display_button_puts_the_set_title_request_on_the_wire",
        tier: Tier::Dat,
    },
    behaviour! {
        id: "titles.tab.the-tab-draws-every-earned-title-sorted-by-name",
        says: "The Titles tab draws one row per title the shard says the character has earned, \
               sorted by the name each one resolves to rather than by its number, with the title \
               being worn named above them -- and each row keeps its own title's number, which is \
               what the client reads back when the player asks to wear it.",
        since: RETAIL,
        evidence: Evidence::Private("AC-EVID-O936"),
        station: "dereth-testkit::dat::panels::scenario_the_titles_tab_draws_every_earned_title_sorted_by_name",
        tier: Tier::Dat,
    },
];
