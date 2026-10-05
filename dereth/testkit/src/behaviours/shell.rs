//! The shell -- the keyboard, the options pages, the windows, the dialogs and the wizard.
//!
//! `ui.rs` is the drawn surface a claim is about -- a notice
//! bubble, a label, a list box's rows; this file is everything the player operates the client
//! *with*: which key does what and where the client wrote it down, which option page carries which
//! control and what it sends when it is moved, what a dialog box does when it is answered, and how
//! the character-creation wizard walks. The shell has enough claims that folding them into `ui.rs`
//! would make one very large subject file.
//!
//! **The `shell_only` row is here too.** The `ClientSpec::shell` backend is the UI shell over a
//! host state the scenario writes, with no `App` under it. A backend is not a subject: what that
//! row claims is a thing the player operates the client with, which is what
//! this file is. The row keeps its id, because renaming an id is renaming a claim and the prefix
//! still says the true thing about it -- that it is asserted against the shell alone.
//!
//! One file per subject, so that two changes adding rows at the same time do not edit the same
//! file. [`ROWS`] is in id order; the registry's own test asserts that, and that no id and no
//! evidence handle is repeated anywhere in it.

// `behaviour!` is `#[macro_export]`ed by `mod.rs` above this module's declaration, so it is in
// textual scope here and needs no import.
use super::{Behaviour, Evidence, Tier, RETAIL, THIS_CLIENT, TOOLING};

/// This subject's rows, in id order.
pub static ROWS: &[Behaviour] = &[
    behaviour! {
        id: "character-select.delete.a-pending-deletion-is-drawn-red-and-last-and-offers-restore",
        says: "A character the shard says is waiting to be deleted is drawn in red and moved to \
               the bottom of the list, while every other row keeps its colour and its place. The \
               row itself says nothing but the name -- there is no countdown written on it -- and \
               picking it offers to restore the character rather than to delete or play them.",
        since: RETAIL,
        evidence: Evidence::Private("AC-EVID-P1-136-PENDING"),
        station: "dereth-testkit::dat::shell::scenario_a_pending_deletion_is_drawn_red_and_last_and_offers_restore",
        tier: Tier::Dat,
    },
    behaviour! {
        id: "character-select.delete.cancelling-after-typing-the-phrase-deletes-nothing",
        says: "A player who types the phrase correctly and then changes their mind loses nothing: \
               cancelling asks the shard for nothing at all, raises no waiting box, closes the \
               question, and leaves every character in the list and the same one picked.",
        since: RETAIL,
        evidence: Evidence::Private("AC-EVID-O52-CANCEL"),
        station: "dereth-testkit::dat::shell::scenario_cancelling_after_typing_the_phrase_deletes_nothing",
        tier: Tier::Dat,
    },
    behaviour! {
        id: "character-select.delete.only-the-typed-phrase-deletes-and-it-deletes-the-one-that-was-picked",
        says: "Deleting a character takes the shipped phrase typed into the box, in any case, and \
               anything else deletes nothing while still closing the question. The right phrase \
               puts the waiting box up first and then asks the shard to delete the character the \
               question named -- identified by their place in the list the shard itself sent, \
               which is not their place in the sorted list on screen.",
        since: RETAIL,
        evidence: Evidence::Private("AC-EVID-O52-DELETE"),
        station: "dereth-testkit::dat::shell::scenario_only_the_typed_phrase_deletes_and_it_deletes_the_one_that_was_picked",
        tier: Tier::Dat,
    },
    behaviour! {
        id: "character-select.delete.the-request-that-leaves-the-client-carries-the-account-and-the-shards-own-place",
        says: "The delete the client really puts on a datagram goes out on the log-on queue and \
               carries the account name followed by the character's place in the list the shard \
               itself sent -- not their place in the sorted list on screen, which is a different \
               character. Read back the way the shard reads it, both fields are what was meant.",
        since: RETAIL,
        evidence: Evidence::Private("AC-EVID-P1-136-WIRE"),
        station: "dereth-testkit::dat::shell::scenario_the_delete_that_leaves_the_client_carries_the_account_and_the_shards_own_place",
        tier: Tier::Dat,
    },
    behaviour! {
        id: "character-select.dialogs.an-error-brought-in-with-the-screen-becomes-a-one-button-message",
        says: "A refusal carried in with the character list -- the shard's reason for sending the \
               player back -- is shown as a message box with a single button, carrying that \
               reason, and pressing the button takes it away.",
        since: RETAIL,
        evidence: Evidence::Private("AC-EVID-O52-ERROR"),
        station: "dereth-testkit::dat::shell::scenario_an_error_brought_in_with_the_screen_becomes_a_one_button_message",
        tier: Tier::Dat,
    },
    behaviour! {
        id: "character-select.dialogs.answering-yes-to-leaving-runs-the-closing-sequence-and-no-stays",
        says: "Answering yes to the question about leaving runs the client's closing sequence, \
               and answering no takes the question down and leaves the player on the character \
               list with the list untouched.",
        since: RETAIL,
        evidence: Evidence::Private("AC-EVID-O52-EXIT-ANSWER"),
        station: "dereth-testkit::dat::shell::scenario_answering_yes_to_leaving_runs_the_closing_sequence_and_no_stays",
        tier: Tier::Dat,
    },
    behaviour! {
        id: "character-select.dialogs.deleting-asks-about-the-character-that-was-picked-and-names-only-them",
        says: "Pressing delete raises a modal question with a box to type in, and the sentence in \
               it names the character the player picked and none of the others -- which is the \
               only thing on the screen that says who would be destroyed. The box grows to hold \
               the whole sentence and is centred, rather than showing one line in a corner.",
        since: RETAIL,
        evidence: Evidence::Private("AC-EVID-O52-DELETE-PROMPT"),
        station: "dereth-testkit::dat::shell::scenario_deleting_asks_about_the_character_that_was_picked",
        tier: Tier::Dat,
    },
    behaviour! {
        id: "character-select.dialogs.leaving-raises-a-modal-question-in-the-shipped-words",
        says: "The exit button raises a real question on the screen -- modal, in the shape a \
               question has, carrying the shipped words -- rather than only noting to itself that \
               one was wanted.",
        since: RETAIL,
        evidence: Evidence::Private("AC-EVID-O52-EXIT"),
        station: "dereth-testkit::dat::shell::scenario_leaving_raises_a_modal_question_in_the_shipped_words",
        tier: Tier::Dat,
    },
    behaviour! {
        id: "character-select.dialogs.restoring-raises-a-box-with-no-buttons-that-the-next-list-takes-down",
        says: "Restoring a character waiting to be deleted asks the shard to do it and puts up a \
               waiting box with no buttons at all, which the player therefore cannot dismiss. The \
               next list the shard sends is what takes it down.",
        since: RETAIL,
        evidence: Evidence::Private("AC-EVID-O52-RESTORE"),
        station: "dereth-testkit::dat::shell::scenario_restoring_raises_a_box_with_no_buttons_that_the_next_list_takes_down",
        tier: Tier::Dat,
    },
    behaviour! {
        id: "character-select.dialogs.the-question-is-modal-and-a-second-press-behind-it-reaches-nothing",
        says: "While the delete question is up the screen behind it takes no presses: pressing \
               where the delete button was raises no second question, and pressing another button \
               back there raises nothing either.",
        since: RETAIL,
        evidence: Evidence::Private("AC-EVID-O52-MODAL"),
        station: "dereth-testkit::dat::shell::scenario_the_question_is_modal_and_a_press_behind_it_reaches_nothing",
        tier: Tier::Dat,
    },
    behaviour! {
        id: "character-select.enter-world.a-double-click-sends-the-retail-bytes",
        says: "Double-clicking a character asks to enter the world as that character and raises \
               the entering-world box first, and the Login_SendEnterWorld request is byte for byte \
               the one the retail client sent in every recorded session that carries one.",
        since: RETAIL,
        evidence: Evidence::Private("AC-EVID-CHARSELECT-ENTER-WORLD"),
        station: "dereth-client::gpu::login::character_select_rows::double_clicking_a_row_asks_to_enter_the_world_with_the_retail_clients_own_bytes",
        tier: Tier::Gpu,
    },
    behaviour! {
        id: "character-select.enter-world.a-double-press-raises-the-waiting-box-before-it-asks-to-log-on",
        says: "One press on a character picks them and enters no world; two presses in quick \
               succession put the waiting box up and then ask the shard to log that character in \
               -- the box first, so the player is never left pressing at a screen that has already \
               asked.",
        since: RETAIL,
        evidence: Evidence::Private("AC-EVID-O52-ENTER"),
        station: "dereth-testkit::dat::shell::scenario_a_double_press_raises_the_waiting_box_before_it_asks_to_log_on",
        tier: Tier::Dat,
    },
    behaviour! {
        id: "character-select.list.the-new-character-appears-after-finish",
        says: "When the character list arrives again after a character is created, the character \
               select list is rebuilt from it without a relaunch: the new character appears \
               alongside the others, sorted by name and not greyed out.",
        since: RETAIL,
        evidence: Evidence::Private("AC-EVID-O267-LIST"),
        station: "dereth-ui-screens::cpu::login::chargen_finish_character_set::the_notice_puts_the_new_character_in_the_list",
        tier: Tier::Cpu,
    },
    behaviour! {
        id: "character-select.restore.a-refusal-takes-the-waiting-box-down-and-says-why-in-the-shipped-words",
        says: "A restore the shard refuses still takes the waiting box down -- the box the player \
               cannot dismiss -- and puts the shipped sentence for that refusal in front of them \
               with a button on it, while the character stays where it was, still waiting to be \
               deleted. A restore that succeeds shows no message at all, and a second refusal \
               carrying the same reason as the first takes the second box down too.",
        since: RETAIL,
        evidence: Evidence::Private("AC-EVID-P4-7A-REFUSED"),
        station: "dereth-testkit::dat::shell::scenario_a_refused_restore_takes_the_waiting_box_down_and_says_why",
        tier: Tier::Dat,
    },
    behaviour! {
        id: "character-select.restore.a-restored-character-comes-back-in-its-own-place-and-in-the-ordinary-colour",
        says: "When the shard says the restore worked, the waiting box goes, the list is no \
               longer than it was, and the character is back among the others: in the place their \
               name sorts to rather than at the bottom, drawn in exactly the colour the rows that \
               were never deleted are drawn in, and offering to be deleted or played again.",
        since: RETAIL,
        evidence: Evidence::Private("AC-EVID-P1-137-DRAWN"),
        station: "dereth-testkit::dat::shell::scenario_a_restored_character_comes_back_in_its_own_place_and_in_the_ordinary_colour",
        tier: Tier::Dat,
    },
    behaviour! {
        id: "character-select.restore.the-answer-overwrites-the-place-it-was-asked-about-and-never-appends",
        says: "The shard's answer to a restore is written over the place in the list the client \
               asked about, whatever identity it carries -- so a character brought back under \
               another name replaces the row rather than being added beside it, and the list \
               never grows.",
        since: RETAIL,
        evidence: Evidence::Private("AC-EVID-P1-137-SLOT"),
        station: "dereth-testkit::dat::shell::scenario_the_restore_answer_overwrites_the_place_it_was_asked_about_and_never_appends",
        tier: Tier::Dat,
    },
    behaviour! {
        id: "character-select.restore.the-request-that-leaves-the-client-is-the-characters-own-id-on-the-control-queue",
        says: "The restore the client really puts on a datagram goes out on the control queue and \
               carries the character's own id and two empty names -- the id and not their place \
               in the list, which the shard would silently find nothing for.",
        since: RETAIL,
        evidence: Evidence::Private("AC-EVID-P1-137-WIRE"),
        station: "dereth-testkit::dat::shell::scenario_the_restore_that_leaves_the_client_is_the_characters_own_id_on_the_control_queue",
        tier: Tier::Dat,
    },
    behaviour! {
        id: "character-select.round-trip.deleting-and-restoring-over-the-wire-leaves-the-list-where-it-started",
        says: "Deleting a character and restoring it, both over real datagrams, ends with the \
               list exactly as it began. The two answers are not alike: the shard follows a \
               delete with a fresh list, which is what takes that waiting box down, and follows a \
               restore with nothing else at all, so the restore's box has to be taken down by the \
               answer itself.",
        since: RETAIL,
        evidence: Evidence::Private("AC-EVID-P4-7A-ROUND-TRIP"),
        station: "dereth-testkit::dat::shell::scenario_deleting_and_restoring_over_the_wire_leaves_the_list_where_it_started",
        tier: Tier::Dat,
    },
    behaviour! {
        id: "character-select.rows.names-are-real-elements-at-the-shipped-pitch-in-alphabetical-order",
        says: "The character list shows the account's characters in alphabetical order whatever \
               order the shard sent them in, each on its own row stacked from the top of the list \
               box at the pitch the box divides into, while the character picked by default is the \
               one the shard listed first, the last one played.",
        since: RETAIL,
        evidence: Evidence::Private("AC-EVID-CHARSELECT-ROWS"),
        station: "dereth-client::gpu::login::character_select_rows::the_rows_are_real_elements_at_the_documented_pitch_and_in_alphabetical_order",
        tier: Tier::Gpu,
    },
    behaviour! {
        id: "chargen.appearance.choosing-the-other-sex-changes-the-body",
        says: "Pressing the other sex on the appearance page changes the body the preview is built \
               from -- and neither is the shared body every people falls back to -- so the model \
               visibly changes, with every changed pixel inside the appearance page.",
        since: RETAIL,
        evidence: Evidence::Private("AC-EVID-O137-APPEARANCE"),
        station: "dereth-client::gpu::login::chargen_appearance_controls::choosing_the_other_sex_changes_the_body_and_repaints_the_viewport",
        tier: Tier::Gpu,
    },
    behaviour! {
        id: "chargen.appearance.clicking-a-colour-moves-the-marker-and-tints-the-shade-wheel-with-it",
        says: "Clicking one of the colour spots picks it: the marker leaves the spot it was over \
               and appears over the new one, and the shade wheel beside them is re-tinted in the \
               colour that was picked rather than staying as it was.",
        since: RETAIL,
        evidence: Evidence::Private("AC-EVID-O95-MARKER"),
        station: "dereth-testkit::dat::shell::scenario_clicking_a_colour_moves_the_marker_and_tints_the_shade_wheel",
        tier: Tier::Dat,
    },
    behaviour! {
        id: "chargen.appearance.the-background-object-stands-behind-the-model",
        says: "The model on the appearance page stands in a setting rather than a void: the \
               preview holds a second object, built from the scenery the chosen people names, and \
               it is really drawn.",
        since: RETAIL,
        evidence: Evidence::Private("AC-EVID-O132-APPEARANCE-BACKGROUND"),
        station: "dereth-client::gpu::login::chargen_dressing::the_background_object_stands_behind_the_model",
        tier: Tier::Gpu,
    },
    behaviour! {
        id: "chargen.appearance.the-colour-spots-are-the-chosen-parts-own-colours-and-the-spare-ones-are-blank",
        says: "The row of colour spots on the appearance page really is coloured, each spot in the \
               colour that choice would give the character, and the spots past the colours this \
               part actually offers are blank rather than black.",
        since: RETAIL,
        evidence: Evidence::Private("AC-EVID-O95-SPOTS"),
        station: "dereth-testkit::dat::shell::scenario_the_colour_spots_are_the_chosen_parts_own_colours",
        tier: Tier::Dat,
    },
    behaviour! {
        id: "chargen.appearance.the-eyes-have-one-colour-each-and-no-shade-to-slide",
        says: "The eyes are the one part of a face whose choices are single colours rather than \
               ranges, so their spots show one colour each, their wheel is the plain disk with no \
               tint, and the shade slider is not shown at all.",
        since: RETAIL,
        evidence: Evidence::Private("AC-EVID-O95-EYES"),
        station: "dereth-testkit::dat::shell::scenario_the_eyes_have_one_colour_each_and_no_shade_to_slide",
        tier: Tier::Dat,
    },
    behaviour! {
        id: "chargen.appearance.the-face-tab-takes-the-hat-off-and-the-clothes-tab-puts-it-back",
        says: "Looking at the face takes the character's hat off so the face can be seen, and \
               going back to the clothes puts the same hat back on. A character wearing no hat \
               still has no hat afterwards, and drawing the page again -- including leaving it and \
               coming back -- does not lose the hat that was taken off.",
        since: RETAIL,
        evidence: Evidence::Private("AC-EVID-O95-HAT"),
        station: "dereth-testkit::dat::shell::scenario_the_face_tab_takes_the_hat_off_and_the_clothes_tab_puts_it_back",
        tier: Tier::Dat,
    },
    behaviour! {
        id: "chargen.appearance.the-part-arrows-wrap-the-way-the-client-wraps-them",
        says: "Each appearance arrow steps its part one choice at a time, and stepping back past \
               the first hair style wraps round to the last; the headgear arrow alone passes \
               through a no-hat choice on its way round, and never leaves its range.",
        since: RETAIL,
        evidence: Evidence::Private("AC-EVID-CHARGEN-APPEARANCE"),
        station: "dereth-client::gpu::login::chargen_wizard::the_part_arrows_wrap_the_way_the_client_wraps_them",
        tier: Tier::Gpu,
    },
    behaviour! {
        id: "chargen.appearance.the-preview-wears-the-clothes-the-wizard-chose",
        says: "The model on the appearance page wears what the wizard has chosen: its face, hair \
               and clothes are drawn as different pieces from the bare body the same setup gives, \
               not painted over it.",
        since: RETAIL,
        evidence: Evidence::Private("AC-EVID-O132-APPEARANCE"),
        station: "dereth-client::gpu::login::chargen_dressing::the_dressed_model_draws_different_geometry_from_the_naked_setup",
        tier: Tier::Gpu,
    },
    behaviour! {
        id: "chargen.appearance.the-zoom-halves-move-the-camera-and-exactly-one-is-lit",
        says: "The two zoom halves on the appearance page are lit one at a time: the page opens on \
               the face tab zoomed in with plus lit, pressing minus zooms out and swaps which half \
               is lit, and pressing the lit half again leaves it lit and the camera where it is.",
        since: RETAIL,
        evidence: Evidence::Private("AC-EVID-O137-APPEARANCE-ZOOM"),
        station: "dereth-client::gpu::login::chargen_appearance_controls::exactly_one_zoom_half_is_lit_and_at_rest_on_the_face_tab_it_is_the_plus",
        tier: Tier::Gpu,
    },
    behaviour! {
        id: "chargen.classic.pages-show-world-choices-and-bounded-help",
        says: "Classic creation pages show distinct world colors, complete clothing on its first visit without rerolling later visits, project edits into the preview, and keep help scrolling within measured text. Repeated hairstyle icons use numbered choices, the named first template is shown once for later data, attribute thumbs are transparent and the name entry has a visible frame.",
        since: THIS_CLIENT,
        divergence: "CD-015",
        evidence: Evidence::Private("AC-EVID-CLASSIC-CREATION-CONTROLS"),
        station: "dereth-classic-ui::lib::panels::pregame::tests::both_worlds_project_real_keys_face_pixels_preview_resources_and_results",
        tier: Tier::Dat,
    },
    behaviour! {
        id: "chargen.classic.preview-light-faces-camera",
        says: "The creation preview's directional light illuminates the surface facing its camera from above.",
        since: THIS_CLIENT,
        divergence: "CD-015",
        evidence: Evidence::Private("AC-EVID-CLASSIC-CREATION-LIGHT"),
        station: "dereth-classic-ui::lib::previews::tests::creation_light_faces_the_preview_camera",
        tier: Tier::Cpu,
    },
    behaviour! {
        id: "chargen.controls.wheel-over-scrollbars",
        says: "Creation scrollbars accept wheel detents over their tracks, thumbs and arrows, with one step per detent and without moving keyboard focus.",
        since: THIS_CLIENT,
        divergence: "CD-015",
        evidence: Evidence::Private("AC-EVID-CREATION-DIRECT-WHEEL"),
        station: "dereth-client-shell::lib::ui::creation_tests::creation_attribute_and_shade_bars_take_one_step_per_wheel_detent",
        tier: Tier::Dat,
    },
    behaviour! {
        id: "chargen.dialogs.a-people-the-account-cannot-play-is-refused-in-a-message-box",
        says: "Choosing a people the account has not bought the right to play raises a box saying \
               so -- one with a single button and nothing to answer -- and the people is refused \
               rather than taken. Dismissing the box leaves the choice where it was.",
        since: RETAIL,
        evidence: Evidence::Private("AC-EVID-O176-TOD"),
        station: "dereth-testkit::dat::shell::scenario_a_people_the_account_cannot_play_is_refused_in_a_message_box",
        tier: Tier::Dat,
    },
    behaviour! {
        id: "chargen.dialogs.a-second-one-waits-its-turn-rather-than-stacking-on-the-first",
        says: "Two of the wizard's boxes wanted at once are one box on screen and one waiting: \
               the second has its place in the queue but nothing drawn for it, and there is one \
               box's worth on the screen rather than two piled up with the older one unreachable. \
               Answering the first brings the one that was waiting up in its place.",
        since: RETAIL,
        evidence: Evidence::Private("AC-EVID-O176-QUEUE"),
        station: "dereth-testkit::dat::shell::scenario_a_second_box_waits_its_turn_rather_than_stacking",
        tier: Tier::Dat,
    },
    behaviour! {
        id: "chargen.dialogs.finishing-with-credits-unspent-asks-first-and-a-yes-goes-on-to-create",
        says: "Finishing with attribute points still to spend asks the player whether they meant \
               to, in a modal question in the shipped words, and creates nothing while the \
               question is up. Answering yes goes on and creates the character.",
        since: RETAIL,
        evidence: Evidence::Private("AC-EVID-O176-CREDITS"),
        station: "dereth-testkit::dat::shell::scenario_finishing_with_credits_unspent_asks_first",
        tier: Tier::Dat,
    },
    behaviour! {
        id: "chargen.dialogs.finishing-with-no-name-refuses-in-a-message-box",
        says: "Finishing with no name typed refuses in a box with a single button, in the shipped \
               words, and dismissing it clears the refusal so the next attempt is judged afresh.",
        since: RETAIL,
        evidence: Evidence::Private("AC-EVID-O176-NO-NAME"),
        station: "dereth-testkit::dat::shell::scenario_finishing_with_no_name_refuses_in_a_message_box",
        tier: Tier::Dat,
    },
    behaviour! {
        id: "chargen.dialogs.one-answered-from-outside-the-screen-is-taken-down-on-the-next-frame",
        says: "A box answered by something other than the player's own press -- the shard \
               answering, or the client answering for them -- still comes off the screen on the \
               next frame, element and all, rather than being left over whatever comes next.",
        since: RETAIL,
        evidence: Evidence::Private("AC-EVID-O176-HEADLESS-CLOSE"),
        station: "dereth-testkit::dat::shell::scenario_a_box_answered_from_outside_the_screen_comes_down",
        tier: Tier::Dat,
    },
    behaviour! {
        id: "chargen.dialogs.one-closed-from-underneath-the-screen-is-replaced-rather-than-left-behind",
        says: "When a box is closed underneath the screen that raised it, the screen does not leave \
               the old one sitting on the player's screen still listening for answers: it is taken \
               away and a fresh one is put up in its place, because the screen still wants the \
               question asked. A close from underneath is not an answer to it.",
        since: RETAIL,
        evidence: Evidence::Private("AC-EVID-O176-ORPHAN"),
        station: "dereth-testkit::dat::shell::scenario_a_box_closed_from_underneath_the_screen_is_replaced",
        tier: Tier::Dat,
    },
    behaviour! {
        id: "chargen.dialogs.the-two-answer-buttons-really-answer-and-a-pointer-can-press-either-of-them",
        says: "The yes and no of the re-roll question really answer it, in opposite directions: \
               yes rolls a fresh character and no leaves every part of the old one alone and \
               stays on the page. Both work when pressed with a real pointer, hit test and all, \
               and not only when the button is told directly.",
        since: RETAIL,
        evidence: Evidence::Private("AC-EVID-O176-ANSWERS"),
        station: "dereth-testkit::dat::shell::scenario_the_warnings_two_buttons_answer_it_and_a_pointer_can_press_either",
        tier: Tier::Dat,
    },
    behaviour! {
        id: "chargen.dialogs.the-warning-before-a-re-roll-is-a-modal-question-in-the-shipped-words",
        says: "The question before a whole character is thrown away is really drawn: a modal box \
               that takes the screen's clicks, of the shape a question has, carrying the shipped \
               warning rather than being blank or being only a note in the client's own records.",
        since: RETAIL,
        evidence: Evidence::Private("AC-EVID-O176"),
        station: "dereth-testkit::dat::shell::scenario_the_re_roll_warning_is_a_modal_question_in_the_shipped_words",
        tier: Tier::Dat,
    },
    behaviour! {
        id: "chargen.dialogs.the-warning-before-a-re-roll-is-the-last-pages-alone",
        says: "The question is asked on the last page only. On any other page the random button \
               rolls that page's own choice at once, with no box raised and nothing to answer.",
        since: RETAIL,
        evidence: Evidence::Private("AC-EVID-O176-SUMMARY-ONLY"),
        station: "dereth-testkit::dat::shell::scenario_only_the_last_page_asks_before_re_rolling",
        tier: Tier::Dat,
    },
    behaviour! {
        id: "chargen.dialogs.the-wizard-never-shows-a-please-wait-box",
        says: "The wizard keeps a place for a please-wait box and never raises one: nothing it \
               does puts one on the screen, and even a place recorded for it by hand draws \
               nothing and adds nothing to the screen.",
        since: RETAIL,
        evidence: Evidence::Private("AC-EVID-O176-PLEASE-WAIT"),
        station: "dereth-testkit::dat::shell::scenario_the_wizard_never_shows_a_please_wait_box",
        tier: Tier::Dat,
    },
    behaviour! {
        id: "chargen.exit.saying-yes-goes-back-to-choosing-a-character-and-saying-no-stays-on-the-page",
        says: "Answering yes to the warning leaves the wizard for the character-choosing screen \
               specifically, and answering no takes the warning down and leaves the player on the \
               page they were on with nothing left behind.",
        since: RETAIL,
        evidence: Evidence::Private("AC-EVID-O119-ANSWER"),
        station: "dereth-testkit::dat::shell::scenario_yes_leaves_the_wizard_and_no_stays_on_the_page",
        tier: Tier::Dat,
    },
    behaviour! {
        id: "chargen.exit.the-back-arrow-is-exit-on-the-first-page-and-a-step-back-on-any-other",
        says: "The back arrow on the wizard's first page raises the same warning the exit button \
               does and lands in the same place; on any other page it steps back one page and \
               raises nothing.",
        since: RETAIL,
        evidence: Evidence::Private("AC-EVID-O119-ARROW"),
        station: "dereth-testkit::dat::shell::scenario_the_back_arrow_is_exit_only_on_the_first_page",
        tier: Tier::Dat,
    },
    behaviour! {
        id: "chargen.exit.the-exit-button-raises-a-modal-warning-in-the-shipped-words",
        says: "Leaving character creation asks first: the exit button raises a warning that takes \
               the whole screen's clicks and says what the shipped text says, and pressing exit \
               again while it is up raises no second one.",
        since: RETAIL,
        evidence: Evidence::Private("AC-EVID-O119-DIALOG"),
        station: "dereth-testkit::dat::shell::scenario_the_exit_button_raises_a_modal_warning",
        tier: Tier::Dat,
    },
    behaviour! {
        id: "chargen.finish.a-built-character-passes-every-check-ace-makes",
        says: "A character built through the wizard, with a profession, trained skills and a face \
               chosen, is sent with every choice inside what the shipped character tables offer: \
               its people, sex, template, hair, eyes, nose, mouth, colours and clothes each name a \
               row that exists, no hat is sent as none, and every shade lies between nought and \
               one.",
        since: RETAIL,
        evidence: Evidence::Private("AC-EVID-CHARGEN-FINISH"),
        station: "dereth-client::gpu::login::chargen_wizard::a_character_with_a_chosen_profession_skills_and_face_passes_every_check_ace_makes",
        tier: Tier::Gpu,
    },
    behaviour! {
        id: "chargen.finish.a-character-never-shown-the-clothes-tab-goes-out-bare-headed",
        says: "A character created without the clothes tab ever being opened is sent with no hat, \
               even though a hat was rolled for it, and one whose player did open the tab is sent \
               wearing that hat; shirt, trousers, shoes and hair are the same either way.",
        since: RETAIL,
        evidence: Evidence::Private("AC-EVID-O220-FINISH"),
        station: "dereth-client::gpu::login::chargen_face_tab::a_character_created_without_opening_the_clothes_tab_goes_out_bare_headed",
        tier: Tier::Gpu,
    },
    behaviour! {
        id: "chargen.finish.a-refused-creation-stops-the-wizard-waiting-and-says-why",
        says: "While the shard is being asked for a new character the wizard is waiting on the \
               answer, and a refusal -- the name is already somebody else's -- stops that waiting \
               and puts the shipped sentence for that reason in front of the player with a button \
               that closes it, leaving the wizard ready to be finished again. Every refusal is \
               answered that way, including the ones that share the client's own general reason, \
               so none of them leaves the player looking at nothing.",
        since: RETAIL,
        evidence: Evidence::Private("AC-EVID-P1-138-REFUSED"),
        station: "dereth-testkit::dat::shell::scenario_a_refused_creation_stops_the_wizard_waiting_and_says_why",
        tier: Tier::Dat,
    },
    behaviour! {
        id: "chargen.finish.a-scripted-creation-names-the-character-on-the-summary-page",
        says: "A run started to create a character opens the wizard, picks the first people and \
               home town, goes to the summary page, names the character there and presses Finish, \
               so the creation is really asked for: Finish does nothing on any other page.",
        since: TOOLING,
        evidence: Evidence::Private("AC-EVID-SCRIPTED-CREATION-SUMMARY"),
        station: "dereth-client::dat::login::character_creation_request::a_scripted_creation_reaches_the_summary_page_and_sends_the_request",
        tier: Tier::Dat,
    },
    behaviour! {
        id: "chargen.finish.a-set-without-the-new-name-falls-back-to-character-select",
        says: "After Finish, when the character list the shard sends back does not hold the name \
               the wizard just created, the wizard logs nobody on and falls back to the character \
               select screen instead of entering the world.",
        since: RETAIL,
        evidence: Evidence::Private("AC-EVID-O267-FINISH"),
        station: "dereth-ui-screens::cpu::login::chargen_finish_character_set::a_set_without_the_new_name_falls_through_to_character_select",
        tier: Tier::Cpu,
    },
    behaviour! {
        id: "chargen.finish.composes-the-character-the-player-built-and-only-once",
        says: "The finish button composes exactly the character the player built -- the name \
               typed, the people, the home town, the profession, the six attributes the summary \
               page printed, every skill at the level it was set to, and a face whose every \
               feature is one the chosen people really have. The build fits the budget it was \
               given, and a second press creates nothing, so a double click never makes two \
               characters.",
        since: RETAIL,
        evidence: Evidence::Private("AC-EVID-O70-FINISH"),
        station: "dereth-testkit::dat::shell::scenario_finish_composes_the_character_the_player_built_and_only_once",
        tier: Tier::Dat,
    },
    behaviour! {
        id: "chargen.finish.every-refusal-the-shard-can-send-draws-its-own-sentence",
        says: "Each refusal the shard can answer a creation with resolves to the sentence the \
               wizard's own arm for it names, and the four sentences involved are four different \
               ones the shipped table can really draw -- so two different refusals are never told \
               to the player in the same words, and none of them is shown as a bare token. The \
               one answer that is not a refusal names no sentence at all.",
        since: RETAIL,
        evidence: Evidence::Private("AC-EVID-P1-138-TABLE"),
        station: "dereth-testkit::dat::shell::scenario_every_refusal_the_shard_can_send_draws_its_own_sentence",
        tier: Tier::Dat,
    },
    behaviour! {
        id: "chargen.finish.its-caption-is-drawn-in-the-font-the-shipped-layout-names",
        says: "The finish button is shown with a caption, in the font the shipped layout names for \
               it, and that font really draws the letters of it -- which it did not always, the \
               caption's font being one of the two the client used to refuse as too tall.",
        since: RETAIL,
        evidence: Evidence::Private("AC-EVID-O70-FONT"),
        station: "dereth-testkit::dat::shell::scenario_the_finish_buttons_caption_is_drawn_in_the_font_the_layout_names",
        tier: Tier::Dat,
    },
    behaviour! {
        id: "chargen.finish.replaces-the-forward-arrow-on-the-last-page-and-does-nothing-anywhere-else",
        says: "On the last page of the wizard the finish button takes the forward arrow's place, \
               and on every other page it is the arrow that is shown; the button is not merely \
               hidden elsewhere but inert, so a press that reached it on another page would still \
               create nothing. On the last page with no name typed it refuses and says why.",
        since: RETAIL,
        evidence: Evidence::Private("AC-EVID-O70-SWAP"),
        station: "dereth-testkit::dat::shell::scenario_finish_replaces_the_forward_arrow_on_the_last_page_alone",
        tier: Tier::Dat,
    },
    behaviour! {
        id: "chargen.finish.the-created-character-joins-the-list-without-a-relaunch",
        says: "When the shard accepts a newly created character, the new character is added to the \
               end of the character list at once, in the same session, without the shard resending \
               the list and without restarting the client; the existing characters and the \
               account's other details are left as they were.",
        since: RETAIL,
        evidence: Evidence::Private("AC-EVID-O267-FINISH-CREATED"),
        station: "dereth-client-net::cpu::login::new_character_joins_list::the_created_character_joins_the_list_without_a_second_character_set_message",
        tier: Tier::Cpu,
    },
    behaviour! {
        id: "chargen.finish.the-request-carries-the-selected-characters-slot-in-server-order",
        says: "The creation request carries a slot: the index, in the character list as the shard \
               sent it, of the character selected on the character screen when Create Character \
               was pressed -- not that character's place in the alphabetical list. With nothing \
               selected, or after the shard has refused a creation, the slot is -1.",
        since: RETAIL,
        evidence: Evidence::Private("AC-EVID-CHARGEN-SLOT"),
        station: "dereth-client::dat::login::character_creation_request::the_creation_request_carries_the_selected_characters_index_in_the_servers_order",
        tier: Tier::Dat,
    },
    behaviour! {
        id: "chargen.heritage.one-bullet-is-lit-and-the-page-behind-it-describes-that-people",
        says: "The wizard opens on a people already chosen for the player, with that bullet lit. \
               Choosing another lights that one and puts out the first, changes the picture behind \
               the page to that people's own, and fills the pane beside it with their skills and \
               their description out of the shipped text.",
        since: RETAIL,
        evidence: Evidence::Private("AC-EVID-O70-HERITAGE"),
        station: "dereth-testkit::dat::shell::scenario_choosing_a_heritage_lights_its_bullet_and_describes_that_people",
        tier: Tier::Dat,
    },
    behaviour! {
        id: "chargen.heritage.the-home-town-follows-the-people-and-is-one-of-that-peoples-own",
        says: "Choosing a people settles a home town at the same moment, always one of that \
               people's own; as the data files ship each people lists exactly one, so the town a \
               new character opens in follows from the people they are rather than from a roll, \
               and the choice that would have rolled it takes no draw when there is only one \
               answer.",
        since: RETAIL,
        evidence: Evidence::Private("AC-EVID-O95-START-AREA"),
        station: "dereth-testkit::dat::shell::scenario_the_home_town_follows_the_people",
        tier: Tier::Dat,
    },
    behaviour! {
        id: "chargen.name.accepted-edits-use-shared-narrow-formatting",
        says: "Accepted name edits use the shared narrow-byte formatter; refused edits preserve the accepted name.",
        since: RETAIL,
        evidence: Evidence::Private("AC-EVID-CREATION-NAME-FORMAT"),
        station: "dereth-chargen::lib::tests::accepted_ascii_names_use_shared_formatting_without_overwriting_refused_edits",
        tier: Tier::Cpu,
    },
    behaviour! {
        id: "chargen.palette.samples-follow-entry-layout",
        says: "Creation color choices use representative entries from the decoded palette layout, preserving distinct eye, hair and skin choices in both supported layouts.",
        since: THIS_CLIENT,
        divergence: "CD-015",
        evidence: Evidence::Private("AC-EVID-CREATION-PALETTE-LAYOUT"),
        station: "dereth-chargen::dat::color_choices_sample_the_decoded_palette_layout",
        tier: Tier::Dat,
    },
    behaviour! {
        id: "chargen.preview.the-portal-space-plays-at-forty-frames-a-second-inside-its-viewport",
        says: "The portal tunnel's animation starts on its first frame and moves on at forty \
               frames for every second that passes, so half a second after it starts it is twenty \
               frames on.",
        since: RETAIL,
        evidence: Evidence::Private("AC-EVID-O66-PREVIEW"),
        station: "dereth-client::gpu::login::chargen_preview_space::the_portal_sequence_advances_at_forty_frames_a_second_of_elapsed_time",
        tier: Tier::Gpu,
    },
    behaviour! {
        id: "chargen.profession.the-six-sliders-are-named-and-sit-where-their-numbers-say",
        says: "Each of the six attribute sliders carries that attribute's name, the number it is \
               currently at, and a thumb sitting at that number rather than pinned to one end. A \
               chosen profession really moves them, so the six are not all the same.",
        since: RETAIL,
        evidence: Evidence::Private("AC-EVID-O70-SLIDERS"),
        station: "dereth-testkit::dat::shell::scenario_the_six_attribute_sliders_are_named_and_sit_at_their_values",
        tier: Tier::Dat,
    },
    behaviour! {
        id: "chargen.random.classic-entry-keeps-nested-draw-order",
        says: "Classic entry and page randomization preserve their nested draws over the world tables, while summary completes only missing choices.",
        since: RETAIL,
        evidence: Evidence::Private("AC-EVID-CREATION-CLASSIC-RANDOM"),
        station: "dereth-chargen::dat::classic_entry_keeps_nested_draws_and_summary_only_completes_missing_choices",
        tier: Tier::Dat,
    },
    behaviour! {
        id: "chargen.random.classic-random-picks-only-aluvian-gharundim-or-sho",
        says: "On any world, every random heritage the classic creation screens make -- the \
               opening roll, quick creation and the Random button -- is Aluvian, Gharu'ndim or \
               Sho. A world's other heritages are still there to choose by hand.",
        since: THIS_CLIENT,
        divergence: "CD-015",
        evidence: Evidence::Private("AC-EVID-OCT06-CLASSIC-RANDOM-HERITAGE"),
        station: "dereth-chargen::dat::classic_random_heritage_on_the_final_world_is_aluvian_gharundim_or_sho",
        tier: Tier::Dat,
    },
    behaviour! {
        id: "chargen.random.on-the-last-page-it-warns-first-and-only-a-yes-re-rolls-the-whole-character",
        says: "The random button on the last page throws the whole character away, so it asks \
               first and changes nothing while the question is up. Answering yes rolls a fresh \
               character, with a profession and its skills and credits spent on them; answering no \
               leaves every part of the character exactly as it was.",
        since: RETAIL,
        evidence: Evidence::Private("AC-EVID-O95-WARNING"),
        station: "dereth-testkit::dat::shell::scenario_the_last_page_warns_before_re_rolling_the_whole_character",
        tier: Tier::Dat,
    },
    behaviour! {
        id: "chargen.random.on-the-town-page-it-can-land-on-any-town-and-not-only-the-peoples-own",
        says: "The random button on the home-town page rolls flatly over the towns a new character \
               may start in, so it can hand a player a town no people of theirs would have been \
               given -- which is not the same roll the people themselves settle the town with.",
        since: RETAIL,
        evidence: Evidence::Private("AC-EVID-O95-TOWN-ROLL"),
        station: "dereth-testkit::dat::shell::scenario_the_town_pages_roll_can_land_on_any_town",
        tier: Tier::Dat,
    },
    behaviour! {
        id: "chargen.random.the-button-re-rolls-the-page-the-player-is-on",
        says: "The random button rolls what the page it is pressed on is about, and nothing else: \
               the people, the profession, the skills, the face, the clothes or the home town. \
               The profession and the face exclude what is already chosen, so one press really \
               moves them, and a re-rolled set of skills never overspends the credits.",
        since: RETAIL,
        evidence: Evidence::Private("AC-EVID-O95-PAGES"),
        station: "dereth-testkit::dat::shell::scenario_the_random_button_re_rolls_the_page_the_player_is_on",
        tier: Tier::Dat,
    },
    behaviour! {
        id: "chargen.random.the-opening-people-is-one-a-plain-account-may-play-and-the-expansion-adds-one",
        says: "The people the wizard opens on is one of the three a plain account may play, and \
               an account with the expansion may also open on the fourth -- out of the thirteen \
               the data files carry, so that is a choice and not the whole list.",
        since: RETAIL,
        evidence: Evidence::Private("AC-EVID-O95-RANGE"),
        station: "dereth-testkit::dat::shell::scenario_the_opening_people_is_one_a_plain_account_may_play",
        tier: Tier::Dat,
    },
    behaviour! {
        id: "chargen.random.the-opening-roll-is-the-seeds-own-and-two-clients-roll-the-same-character",
        says: "The character the wizard opens on is decided entirely by the two seeds the client \
               starts its dice with, so two clients started the same way open on the identical \
               character down to the shade of every part. The only thing the screen adds to the \
               roll is that the page the player arrives on takes the hat off, keeping it to put \
               back.",
        since: RETAIL,
        evidence: Evidence::Private("AC-EVID-O95-SEEDS"),
        station: "dereth-testkit::dat::shell::scenario_the_opening_roll_is_the_seeds_own_and_two_clients_roll_the_same_character",
        tier: Tier::Dat,
    },
    behaviour! {
        id: "chargen.random.the-people-and-the-sex-come-out-of-a-different-draw-from-everything-else",
        says: "The client rolls with two separate sets of dice and does not mix them: the people \
               and the sex come from one, everything else from the other. Changing one seed moves \
               only its own half, and after a roll the first set is exactly two throws along -- \
               which is what a roll that took the people from the wrong dice would not be.",
        since: RETAIL,
        evidence: Evidence::Private("AC-EVID-O95-STREAMS"),
        station: "dereth-testkit::dat::shell::scenario_the_people_and_the_sex_come_out_of_their_own_stream",
        tier: Tier::Dat,
    },
    behaviour! {
        id: "chargen.random.the-wizard-opens-on-a-character-already-rolled",
        says: "The wizard opens on a whole character rather than on an empty form: a people with \
               its bullet lit, a sex, a home town, a profession, a face with every feature chosen \
               and clothes with a colour and a shade for each -- so nothing the player sees is \
               blank before they have touched anything.",
        since: RETAIL,
        evidence: Evidence::Private("AC-EVID-O95"),
        station: "dereth-testkit::dat::shell::scenario_the_wizard_opens_on_a_character_already_rolled",
        tier: Tier::Dat,
    },
    behaviour! {
        id: "chargen.scroll.a-pane-measures-every-line-of-its-text-its-margins-and-the-blank-row-after-it",
        says: "A description pane measures what it is holding after it is filled and not only when \
               it was built empty: every wrapped line of the paragraph, the four margins the pane \
               keeps around it, and the blank row the paragraph's closing newline leaves at the \
               end. That last row is what gives the people's page a bar with real travel rather \
               than one that exactly fills its box and is dead.",
        since: RETAIL,
        evidence: Evidence::Private("AC-EVID-O118-MEASURE"),
        station: "dereth-testkit::dat::shell::scenario_a_pane_measures_its_whole_paragraph_and_the_blank_row_after_it",
        tier: Tier::Dat,
    },
    behaviour! {
        id: "chargen.scroll.a-pane-with-text-in-it-has-a-live-bar-whose-thumb-is-the-size-of-what-is-shown",
        says: "Each of the wizard's description panes reports how much text it is holding, so its \
               bar is live exactly when there is something to scroll to, and the thumb covers as \
               much of the track as the pane shows of its text.",
        since: RETAIL,
        evidence: Evidence::Private("AC-EVID-O118-EXTENT"),
        station: "dereth-testkit::dat::shell::scenario_a_filled_pane_has_a_live_bar_sized_to_what_is_shown",
        tier: Tier::Dat,
    },
    behaviour! {
        id: "chargen.scroll.a-press-on-the-track-moves-a-whole-page-towards-the-press",
        says: "Pressing the bar's track below the thumb moves the list down by a whole boxful, and \
               pressing above it moves back by the same -- towards the press in each case, not \
               away from it.",
        since: RETAIL,
        evidence: Evidence::Private("AC-EVID-O118-TRACK"),
        station: "dereth-testkit::dat::shell::scenario_a_press_on_the_track_moves_a_whole_page_towards_the_press",
        tier: Tier::Dat,
    },
    behaviour! {
        id: "chargen.scroll.an-arrow-moves-one-row-and-the-two-arrows-go-opposite-ways",
        says: "One press of a bar's arrow moves the list by exactly one row, and the two arrows go \
               in opposite directions -- so pressing one and then the other leaves the list where \
               it began. At the top, pressing up again goes nowhere.",
        since: RETAIL,
        evidence: Evidence::Private("AC-EVID-O118-ARROWS"),
        station: "dereth-testkit::dat::shell::scenario_an_arrow_moves_one_row_and_the_two_go_opposite_ways",
        tier: Tier::Dat,
    },
    behaviour! {
        id: "chargen.scroll.dragging-the-thumb-moves-the-rows-by-the-same-fraction",
        says: "Putting the thumb half way down its track puts the list half way down what it can \
               scroll, and the rows themselves move up by exactly that many pixels -- not only the \
               number the client keeps. Putting it back at the top puts the rows back.",
        since: RETAIL,
        evidence: Evidence::Private("AC-EVID-O118-THUMB"),
        station: "dereth-testkit::dat::shell::scenario_dragging_the_thumb_moves_the_rows_by_the_same_fraction",
        tier: Tier::Dat,
    },
    behaviour! {
        id: "chargen.scroll.each-pane-drives-the-bar-beside-it-and-not-another-pages",
        says: "Three of the wizard's pages carry a description pane and all three name the same \
               bar, and each drives the one on its own page: three panes, three bars, and \
               scrolling one leaves the others exactly where they were. The second list likewise \
               has its own bar rather than sharing the first list's.",
        since: RETAIL,
        evidence: Evidence::Private("AC-EVID-O118-BINDING"),
        station: "dereth-testkit::dat::shell::scenario_each_pane_drives_the_bar_beside_it",
        tier: Tier::Dat,
    },
    behaviour! {
        id: "chargen.scroll.the-wheel-moves-a-list-one-row-and-stops-at-the-top",
        says: "A turn of the wheel over one of the wizard's lists moves it by one row, the same \
               as one press of the bar's arrow and not a whole page; turning it the other way at \
               the top of the list moves nothing.",
        since: RETAIL,
        evidence: Evidence::Private("AC-EVID-O118-WHEEL"),
        station: "dereth-testkit::dat::shell::scenario_the_wheel_moves_a_list_one_row_and_stops_at_the_top",
        tier: Tier::Dat,
    },
    behaviour! {
        id: "chargen.scroll.three-panes-and-two-lists-scroll-and-the-town-page-has-nothing-to-scroll",
        says: "Five things in the wizard scroll -- the three description panes and the two lists -- \
               and the home-town page is not one of them: it has no bar at all. The profession \
               page's six sliders are sliders and not bars.",
        since: RETAIL,
        evidence: Evidence::Private("AC-EVID-O118-CENSUS"),
        station: "dereth-testkit::dat::shell::scenario_three_panes_and_two_lists_scroll_and_the_town_page_has_none",
        tier: Tier::Dat,
    },
    behaviour! {
        id: "chargen.skills.changes-refund-prior-cost-and-refuse-unaffordable-classes",
        says: "Skill changes refund the prior class before checking affordability and refuse unavailable classes.",
        since: RETAIL,
        evidence: Evidence::Private("AC-EVID-CREATION-SKILL-AFFORDABILITY"),
        station: "dereth-chargen::dat::skill_changes_refund_prior_cost_and_refuse_unaffordable_or_unavailable_classes",
        tier: Tier::Dat,
    },
    behaviour! {
        id: "chargen.skills.every-row-is-drawn-under-its-new-heading-and-back-again",
        says: "Every skill on the page that can be moved between groups is drawn under its new \
               heading the moment it moves there, in name order among the rows already there, and \
               is drawn back exactly where it started when it is moved back -- with the credits \
               returned. The picture keeps following the list on every press and not only when the \
               page is entered afresh.",
        since: RETAIL,
        evidence: Evidence::Private("AC-EVID-O136"),
        station: "dereth-testkit::dat::shell::scenario_every_row_is_drawn_under_its_new_heading_and_back_again",
        tier: Tier::Dat,
    },
    behaviour! {
        id: "chargen.skills.every-row-shows-its-own-score-and-its-own-two-prices",
        says: "Every row of the skills list shows that skill's name, the score it would start at \
               as a number, and the two prices for moving it up and down -- its own, not the row \
               above's and never a heading's words. Each arrow is lit exactly when the move it \
               offers can be afforded and made. Every skill the character has a level for has \
               exactly one row, and nothing else does.",
        since: RETAIL,
        evidence: Evidence::Private("AC-EVID-O96-CELLS"),
        station: "dereth-testkit::dat::shell::scenario_every_row_shows_its_own_score_and_its_own_two_prices",
        tier: Tier::Dat,
    },
    behaviour! {
        id: "chargen.skills.the-four-headings-sit-above-their-own-groups-and-each-group-is-in-name-order",
        says: "The skills list is heading, group, heading, group -- the four category headings are \
               not stacked at the top with every skill beneath them. A skill sits under the \
               heading its level and its usability name, and inside a group the rows are in name \
               order.",
        since: RETAIL,
        evidence: Evidence::Private("AC-EVID-O96-HEADINGS"),
        station: "dereth-testkit::dat::shell::scenario_the_four_headings_sit_above_their_own_groups",
        tier: Tier::Dat,
    },
    behaviour! {
        id: "chargen.skills.the-page-reads-the-way-the-original-drew-it-for-the-same-character",
        says: "Driven to the same character, the skills page shows what the original client showed \
               -- the same twelve items in the same order, each with the same score and the same \
               two prices and the same arrows lit -- and the credit meter reads the same number.",
        since: RETAIL,
        evidence: Evidence::Private("AC-EVID-O96-FRAME"),
        station: "dereth-testkit::dat::shell::scenario_the_skills_page_reads_the_way_the_original_drew_it",
        tier: Tier::Dat,
    },
    behaviour! {
        id: "chargen.skills.the-reported-press-moves-the-row-it-names-and-leaving-the-page-changes-nothing",
        says: "The press that was reported -- training one particular skill on a character with \
               credits to spend -- takes the price out of the credits and moves that row out of \
               the untrained group and into the trained one on the screen. Leaving the page and \
               coming back draws exactly the same list, because the press had already done it.",
        since: RETAIL,
        evidence: Evidence::Private("AC-EVID-O136-SWEEP"),
        station: "dereth-testkit::dat::shell::scenario_the_reported_press_moves_the_row_it_names",
        tier: Tier::Dat,
    },
    behaviour! {
        id: "chargen.skills.the-rows-are-stacked-one-below-another-and-none-overlaps",
        says: "The skills list draws its items in one column, each starting where the one above it \
               ends, at the row template's own height -- so no two rows overlap and no heading is \
               written over the top of a skill.",
        since: RETAIL,
        evidence: Evidence::Private("AC-EVID-O96-LAYOUT"),
        station: "dereth-testkit::dat::shell::scenario_the_skill_rows_are_stacked_and_none_overlaps",
        tier: Tier::Dat,
    },
    behaviour! {
        id: "chargen.skills.training-one-moves-its-row-under-the-trained-heading-and-re-prices-the-rest",
        says: "Pressing a skill's up arrow trains it, takes the price out of the credits, and \
               moves that row out of the untrained group and into the trained one, in name order \
               -- on the screen and not only in what the client holds. Every other row is then \
               priced again, so a skill that has just become unaffordable stops offering itself.",
        since: RETAIL,
        evidence: Evidence::Private("AC-EVID-O96-TRAIN"),
        station: "dereth-testkit::dat::shell::scenario_training_a_skill_moves_its_row_and_re_prices_the_rest",
        tier: Tier::Dat,
    },
    behaviour! {
        id: "chargen.summary.a-press-in-the-name-box-puts-the-caret-where-it-was-pressed-and-typing-goes-there",
        says: "Pressing inside the name box when it already holds the keyboard puts the caret \
               where the press landed rather than selecting the whole box again, so what is typed \
               next joins what is there at that point. The selection a press makes belongs to the \
               press and is over when the button is let go, while the selection itself stays -- \
               which is why text selected that way can still be copied afterwards.",
        since: RETAIL,
        evidence: Evidence::Private("AC-EVID-O140-CARET"),
        station: "dereth-testkit::dat::shell::scenario_a_press_in_the_name_box_puts_the_caret_where_it_was_pressed",
        tier: Tier::Dat,
    },
    behaviour! {
        id: "chargen.summary.a-press-on-nothing-takes-the-keyboard-away-and-a-press-back-in-the-box-returns-it",
        says: "Pressing somewhere on the page that answers to nothing takes the keyboard away from \
               whatever held it, and typing then enters nothing anywhere -- which is what a player \
               who has clicked off the box sees. Pressing back inside the box gives it the \
               keyboard again and typing enters as before.",
        since: RETAIL,
        evidence: Evidence::Private("AC-EVID-O140-REFOCUS"),
        station: "dereth-testkit::dat::shell::scenario_a_press_on_nothing_takes_the_keyboard_away_and_a_press_back_in_returns_it",
        tier: Tier::Dat,
    },
    behaviour! {
        id: "chargen.summary.a-press-that-types-nothing-leaves-a-plain-caret-and-only-a-gesture-puts-the-highlight-back",
        says: "Pressing into the name box and typing nothing leaves a plain caret, and it stays a \
               plain caret however long the client runs: the highlight over the prompt does not \
               come back by itself. It comes back on a gesture -- leaving the page and returning, \
               or rolling a fresh character -- which is what makes the quiet frames a measurement \
               rather than a box that cannot be highlighted at all.",
        since: RETAIL,
        evidence: Evidence::Private("AC-EVID-O619"),
        station: "dereth-testkit::dat::shell::scenario_a_press_that_types_nothing_leaves_a_plain_caret",
        tier: Tier::Dat,
    },
    behaviour! {
        id: "chargen.summary.lists-every-choice-the-wizard-has-made-and-what-they-came-to",
        says: "The last page lists the choices the player made -- profession, sex, people, home \
               town -- then the six attributes with the health, stamina and mana they come to and \
               the skill credits left, then the four skill sections with a score beside every \
               skill. The pane beside it holds the instructions and that people's own naming \
               examples.",
        since: RETAIL,
        evidence: Evidence::Private("AC-EVID-O70-SUMMARY"),
        station: "dereth-testkit::dat::shell::scenario_the_summary_page_lists_every_choice_and_what_it_came_to",
        tier: Tier::Dat,
    },
    behaviour! {
        id: "chargen.summary.the-name-box-prompts-takes-the-keyboard-and-the-first-character-replaces-the-prompt",
        says: "The name box on the last page takes the keyboard by itself, so a player can simply \
               type, and shows the shipped prompt until they do. The prompt is wholly selected, so \
               the first character typed replaces it rather than being added to it, and leaving \
               the page and coming back does not put the prompt over the name again.",
        since: RETAIL,
        evidence: Evidence::Private("AC-EVID-O70-NAME"),
        station: "dereth-testkit::dat::shell::scenario_the_name_box_prompts_takes_the_keyboard_and_keeps_what_replaced_the_prompt",
        tier: Tier::Dat,
    },
    behaviour! {
        id: "chargen.tables.world-keys-and-costs-remain-authoritative",
        says: "Both creation interfaces use the active world keys, costs, resources and result identities without substituting interface-era choices.",
        since: THIS_CLIENT,
        divergence: "CD-015",
        evidence: Evidence::Private("AC-EVID-CREATION-WORLD-RULES"),
        station: "dereth-chargen::dat::classic_choices_keep_world_keys_and_templates_keep_unavailable_skills_inactive",
        tier: Tier::Dat,
    },
    behaviour! {
        id: "chargen.tabs.the-page-the-player-is-on-is-the-only-bright-one",
        says: "The strip of tabs along the wizard shows exactly one bright tab, and it is the page \
               the player is on -- from every page, not only from the first.",
        since: RETAIL,
        evidence: Evidence::Private("AC-EVID-O70-TABS"),
        station: "dereth-testkit::dat::shell::scenario_the_tab_strip_brightens_only_the_page_it_is_on",
        tier: Tier::Dat,
    },
    behaviour! {
        id: "chargen.tabs.the-two-insect-peoples-lose-three-pages-outright-rather-than-having-them-greyed",
        says: "The two insect peoples have no profession, no skills to spend and no choice of home \
               town, and the three tabs for those pages go away entirely rather than being greyed \
               out -- so there is nothing there to point at, not something there that refuses.",
        since: RETAIL,
        evidence: Evidence::Private("AC-EVID-O95-OLTHOI"),
        station: "dereth-testkit::dat::shell::scenario_the_insect_peoples_lose_three_tabs_outright",
        tier: Tier::Dat,
    },
    behaviour! {
        id: "chargen.text.an-earlier-world-shows-its-own-creation-texts",
        says: "On a world whose creation table names its own texts (February 2005), the retail \
               interface's creation wizard shows them: the heritage's description as the \
               heritage pane, the profession's description for the chosen sex, and the name \
               page's help with the sex's naming help on the summary page. The end of retail \
               names none and keeps the interface's strings.",
        since: THIS_CLIENT,
        divergence: "CD-010",
        evidence: Evidence::Private("AC-EVID-OCT05-CHARGEN-ERA-TEXT"),
        station: "dereth-client-shell::lib::ui::creation_tests::earlier_world_heritage_profession_and_naming_texts_are_the_worlds_own",
        tier: Tier::Dat,
    },
    behaviour! {
        id: "chargen.town.an-earlier-world-draws-a-dot-on-the-map-for-each-start",
        says: "On a world whose starts are not the shipped map's four towns (February 2005: two \
               by each of Holtburg, Shoushi and Yaraq), the retail interface's town page keeps \
               each town's name on the map and draws a dot for each start beside its town, on \
               the side the start lies, no two overlapping. A dot chooses its start, the title \
               names it and the pane shows the town's own text.",
        since: THIS_CLIENT,
        divergence: "CD-010",
        evidence: Evidence::Private("AC-EVID-OCT05-CHARGEN-TOWN-DOTS"),
        station: "dereth-client-shell::lib::ui::creation_tests::earlier_world_town_page_draws_a_dot_by_its_town_for_each_of_the_six_starts",
        tier: Tier::Dat,
    },
    behaviour! {
        id: "chargen.town.one-pin-is-lit-and-the-page-is-titled-and-described-for-that-town",
        says: "Choosing a home town lights that pin on the map and puts the others out, titles the \
               page with that town's name, and fills the pane with that town's description \
               followed by the standing instruction.",
        since: RETAIL,
        evidence: Evidence::Private("AC-EVID-O70-TOWN"),
        station: "dereth-testkit::dat::shell::scenario_choosing_a_town_lights_its_pin_and_titles_the_page",
        tier: Tier::Dat,
    },
    behaviour! {
        id: "dialog-keys.a-box-that-takes-typing-adds-the-dialog-map-without-taking-the-keys-from-the-box",
        says: "Two elements in the whole of the shipped interface name a key map of their own, both \
               of them boxes a player types into, and both name the pre-game one. Giving one of \
               them the keyboard really does put that map in front of the client, beside the \
               screen's own -- and takes it away again when the keyboard leaves. It does not take \
               enter or escape away from the box: those still belong to the box itself, because a \
               box registers its own keys afterwards and therefore first.",
        since: RETAIL,
        evidence: Evidence::Private("AC-EVID-O355-FOCUS"),
        station: "dereth-testkit::dat::shell::scenario_a_box_that_takes_typing_adds_the_dialog_map_without_taking_the_keys",
        tier: Tier::Dat,
    },
    behaviour! {
        id: "dialog-keys.character-select.enter-is-declined-and-escape-asks-once-whether-to-quit",
        says: "On the character list enter really arrives at the screen and the screen does \
               nothing with it -- no box opens and nothing moves -- while escape raises the \
               question about leaving the game, exactly one of it, and pressing escape again while \
               it is up raises no second one.",
        since: RETAIL,
        evidence: Evidence::Private("AC-EVID-O355-CHARSELECT"),
        station: "dereth-testkit::dat::shell::scenario_enter_is_declined_on_the_character_list_and_escape_asks_once",
        tier: Tier::Dat,
    },
    behaviour! {
        id: "dialog-keys.credits.a-key-the-roll-does-not-own-leaves-it-running",
        says: "A key the credit roll does not own leaves it running: the key really is delivered \
               and offered around, the roll is simply not one of the screens that answers it -- \
               and the key the roll does own, pressed straight afterwards on the same screen, \
               still ends it.",
        since: RETAIL,
        evidence: Evidence::Private("AC-EVID-O398-FOREIGN-KEY"),
        station: "dereth-testkit::dat::shell::scenario_a_key_the_roll_does_not_own_leaves_it_running",
        tier: Tier::Dat,
    },
    behaviour! {
        id: "dialog-keys.credits.either-key-ends-the-roll-and-ends-it-once",
        says: "The credit roll is ended by enter as well as by escape -- the screen does not look \
               at which key it was -- and either of them ends it exactly once, taking the player \
               back to the character list rather than being delivered twice and switching twice.",
        since: RETAIL,
        evidence: Evidence::Private("AC-EVID-O355-CREDITS"),
        station: "dereth-testkit::dat::shell::scenario_either_key_ends_the_credit_roll_and_ends_it_once",
        tier: Tier::Dat,
    },
    behaviour! {
        id: "dialog-keys.intro.a-real-press-of-enter-advances-twice-because-two-handlers-answer-it",
        says: "One real press of enter on the opening sequence moves it on by two pictures, \
               because the screen listens both for the key as a bound action and for the character \
               the desktop makes of it, and both move it on. A printable key, which no map here \
               binds, takes only the second road and moves it on by one.",
        since: RETAIL,
        evidence: Evidence::Private("AC-EVID-O355-TWICE"),
        station: "dereth-testkit::dat::shell::scenario_a_real_press_of_enter_advances_twice",
        tier: Tier::Dat,
    },
    behaviour! {
        id: "dialog-keys.intro.enter-advances-the-opening-sequence-and-escape-leaves-it",
        says: "Enter on the opening sequence moves it on one picture and escape leaves it for the \
               character list. The key really reaches the screen -- the pre-game map outranks the \
               in-game chat map that used to swallow it -- and the screen going away takes both of \
               its maps with it rather than leaving one behind.",
        since: RETAIL,
        evidence: Evidence::Private("AC-EVID-O355-INTRO"),
        station: "dereth-testkit::dat::shell::scenario_enter_advances_the_opening_sequence_and_escape_leaves_it",
        tier: Tier::Dat,
    },
    behaviour! {
        id: "dialog-keys.intro.the-screen-answers-the-keys-its-own-maps-carry-and-no-others",
        says: "The opening sequence answers what arrives on its own maps and refuses the same \
               action arriving on another -- so it is a screen that listens for its keys, not one \
               that swallows everything while it is up.",
        since: RETAIL,
        evidence: Evidence::Private("AC-EVID-O355-FILTER"),
        station: "dereth-testkit::dat::shell::scenario_the_opening_screen_answers_only_the_keys_its_own_maps_carry",
        tier: Tier::Dat,
    },
    behaviour! {
        id: "dialog-keys.only-the-opening-sequence-refuses-a-key-on-its-way-back-up",
        says: "Of the three screens before the world, only the opening sequence refuses a key on \
               its way back up: the same coming-up edge moves the credit roll on and raises the \
               question about leaving on the character list. It takes a made-up edge to see it at \
               all, because both of the keys these screens listen for are one-shot in the shipped \
               bindings and a real finger never produces one.",
        since: RETAIL,
        evidence: Evidence::Private("AC-EVID-O398-RELEASE-EDGE"),
        station: "dereth-testkit::dat::shell::scenario_only_the_opening_sequence_refuses_a_key_on_its_way_back_up",
        tier: Tier::Dat,
    },
    behaviour! {
        id: "focus.press.a-press-on-a-button-takes-the-keyboard-and-the-button-keeps-its-own-look",
        says: "Pressing a button takes the keyboard away from whatever had it, and the button \
               shows its own pressed picture rather than a general look-at-me one; letting go puts \
               the picture back without giving the keyboard back.",
        since: RETAIL,
        evidence: Evidence::Private("AC-EVID-O266-BUTTON"),
        station: "dereth-testkit::dat::shell::scenario_a_press_on_a_button_takes_the_keyboard",
        tier: Tier::Dat,
    },
    behaviour! {
        id: "focus.press.a-press-on-a-list-takes-the-keyboard-and-its-look-follows",
        says: "Pressing a list takes the keyboard, and unlike a button the list's own look \
               follows -- it leaves its resting picture for whichever one its layout gives it.",
        since: RETAIL,
        evidence: Evidence::Private("AC-EVID-O266-LIST"),
        station: "dereth-testkit::dat::shell::scenario_a_press_on_a_list_takes_the_keyboard_and_its_look_follows",
        tier: Tier::Dat,
    },
    behaviour! {
        id: "focus.press.a-press-on-a-scrollbar-takes-the-keyboard-too",
        says: "Pressing a scrollbar takes the keyboard as well: a bar is one of the things that \n \
               can scroll, so what the pointer lands on there holds the keyboard afterwards.",
        since: RETAIL,
        evidence: Evidence::Private("AC-EVID-O266-BAR"),
        station: "dereth-testkit::dat::shell::scenario_a_press_on_a_scrollbar_takes_the_keyboard_without_moving_it",
        tier: Tier::Dat,
    },
    behaviour! {
        id: "focus.press.a-press-on-something-that-cannot-scroll-moves-the-keyboard-nowhere",
        says: "Pressing anything that is not in the family of things that can scroll leaves the \
               keyboard exactly where it was, including on the chat entry -- so the keyboard does \
               not follow the pointer everywhere it goes.",
        since: RETAIL,
        evidence: Evidence::Private("AC-EVID-O266-OUTSIDE"),
        station: "dereth-testkit::dat::shell::scenario_a_press_on_something_that_cannot_scroll_moves_the_keyboard_nowhere",
        tier: Tier::Dat,
    },
    behaviour! {
        id: "focus.press.typing-stops-when-the-keyboard-leaves-the-entry",
        says: "While the chat entry holds the keyboard the player's typing reaches it; pressing a \
               button beside it takes the keyboard away and the typing stops going there.",
        since: RETAIL,
        evidence: Evidence::Private("AC-EVID-O266-TYPING"),
        station: "dereth-testkit::dat::shell::scenario_typing_stops_when_the_keyboard_leaves_the_entry",
        tier: Tier::Dat,
    },
    behaviour! {
        id: "input.dispatch.declined-runtime-actions-are-offered-once",
        says: "Subsequent host messages cannot offer an already-declined action to the interface again; runtime receives it once in its original order.",
        since: THIS_CLIENT,
        divergence: "CD-015",
        evidence: Evidence::Private("AC-EVID-INPUT-MESSAGE-OWNERSHIP"),
        station: "dereth-client-shell::lib::front_end::message_tests::declined_runtime_actions_are_not_offered_again_to_ui_on_following_messages",
        tier: Tier::Dat,
    },
    behaviour! {
        id: "intro.click.a-click-advances-one-picture-and-letting-go-is-not-another",
        says: "A click on the opening sequence moves it on by one picture rather than doing \
               nothing and rather than skipping to the end, and letting the button go is not a \
               second move -- so one click is one picture.",
        since: RETAIL,
        evidence: Evidence::Private("AC-EVID-O103-CLICK"),
        station: "dereth-testkit::dat::shell::scenario_a_click_advances_one_picture_and_the_release_is_not_another",
        tier: Tier::Dat,
    },
    behaviour! {
        id: "intro.click.clicking-through-the-sequence-ends-at-character-select-and-not-before",
        says: "Clicking through the whole opening sequence shows every picture in turn and lands \
               on the character-choosing screen after the last one, not before it.",
        since: RETAIL,
        evidence: Evidence::Private("AC-EVID-O103-SEQUENCE"),
        station: "dereth-testkit::dat::shell::scenario_clicking_through_the_intro_ends_at_character_select",
        tier: Tier::Dat,
    },
    behaviour! {
        id: "intro.keyboard.any-character-advances-one-picture-and-escape-skips",
        says: "Any key that types a character moves the opening sequence on by one picture, and \
               escape leaves it for the character-choosing screen.",
        since: RETAIL,
        evidence: Evidence::Private("AC-EVID-O103-KEYS"),
        station: "dereth-testkit::dat::shell::scenario_any_character_advances_and_escape_skips",
        tier: Tier::Dat,
    },
    behaviour! {
        id: "intro.movie.the-logo-movie-is-two-hundred-and-nineteen-frames-and-stops",
        says: "The logo movie at start-up is 640 by 480 at just under thirty frames a second and \
               runs 219 frames, about seven and a third seconds; every frame is opaque, almost \
               every frame differs from the one before, and it stops at the end rather than \
               looping.",
        since: RETAIL,
        evidence: Evidence::Private("AC-EVID-INTRO-MOVIE"),
        station: "dereth-client::gpu::login::intro_sequence::the_logo_movie_is_two_hundred_and_nineteen_distinct_frames_and_stops",
        tier: Tier::Gpu,
    },
    behaviour! {
        id: "intro.quit.the-quit-action-skips-the-rest-of-it",
        says: "The action bound to quitting skips the rest of the opening sequence outright, \
               where a click would only have moved it on one picture.",
        since: RETAIL,
        evidence: Evidence::Private("AC-EVID-O103-QUIT"),
        station: "dereth-testkit::dat::shell::scenario_the_quit_action_skips_the_rest_of_the_intro",
        tier: Tier::Dat,
    },
    behaviour! {
        id: "keymap.dialog-keys.the-shipped-map-binds-only-escape-and-enter-and-each-once",
        says: "The key map the screens before the world listen on carries two keys and no more -- \
               one to answer and one to back out -- and the shipped map binds each of them exactly \
               once. It is the screens before the world that claim it: in the world neither of \
               those two keys belongs to it, although both still reach something.",
        since: RETAIL,
        evidence: Evidence::Private("AC-EVID-O355-MAP"),
        station: "dereth-testkit::dat::shell::scenario_the_dialog_map_binds_only_escape_and_enter",
        tier: Tier::Dat,
    },
    behaviour! {
        id: "keymap.function-keys.each-one-does-what-the-shipped-map-binds-it-to",
        says: "Every function key does what the shipped key map binds it to and nothing else: the \
               ones bound to a panel raise that panel's own action, the one the shipped map leaves \
               free produces no action at all, and a key bound to something that is not a panel \
               raises that instead.",
        since: RETAIL,
        evidence: Evidence::Private("AC-EVID-P1-94-MAP"),
        station: "dereth-testkit::dat::shell::scenario_every_function_key_does_what_the_shipped_map_says",
        tier: Tier::Dat,
    },
    behaviour! {
        id: "keymap.function-keys.each-panel-key-opens-its-own-page-and-shuts-it-again",
        says: "Pressing a panel's function key opens that page in the heads-up display -- the \
               element that listens for it really becomes visible -- and pressing the same key \
               again closes it.",
        since: RETAIL,
        evidence: Evidence::Private("AC-EVID-P1-94-PANELS"),
        station: "dereth-testkit::dat::shell::scenario_each_panel_function_key_opens_and_shuts_its_page",
        tier: Tier::Dat,
    },
    behaviour! {
        id: "keymap.lifecycle.interface-switch-and-session-end-release-runtime-movement",
        says: "Interface changes and character-session teardown deliver held movement releases before retiring their input callback scopes.",
        since: THIS_CLIENT,
        divergence: "CD-015",
        evidence: Evidence::Private("AC-EVID-KEYMAP-SCOPE-RELEASE"),
        station: "dereth-client-shell::lib::input::tests::interface_switch_and_session_end_release_runtime_movement",
        tier: Tier::Dat,
    },
    behaviour! {
        id: "keymap.modified-digits.a-number-with-the-modifier-uses-a-quick-slot-and-not-the-chat-window",
        says: "Holding the modifier and pressing one of the first four numbers uses that quick \
               slot rather than opening the matching floating chat window: the shipped key map \
               binds the very same key to both, and the client settles the tie the way it \
               registers the two maps. The plain number still uses its own quick slot, and those \
               chat windows really do have a working toggle -- so this is a tie being settled and \
               not a window that was never built.",
        since: RETAIL,
        evidence: Evidence::Private("AC-EVID-P1-141"),
        station: "dereth-testkit::dat::shell::scenario_a_modified_number_uses_a_quick_slot_and_not_the_chat_window",
        tier: Tier::Dat,
    },
    behaviour! {
        id: "keymap.rebind.a-rebound-key-moves-the-body-and-the-old-one-stops",
        says: "A key rebound to walking forward walks the body, and the key it was taken from \
               stops walking it -- while an untouched binding still works, so it is the binding \
               that changed and not the keyboard.",
        since: RETAIL,
        evidence: Evidence::Private("AC-EVID-O196-REBIND"),
        station: "dereth-testkit::dat::shell::scenario_a_rebound_key_walks_the_body_and_the_old_one_stops",
        tier: Tier::Dat,
    },
    behaviour! {
        id: "keymap.rebind.is-written-beside-the-preferences-on-a-clean-exit",
        says: "A rebind survives the session: leaving the client writes the whole merged key map \
               into a file beside the player's preferences, carrying both the new binding and the \
               freeing of the old one. A client with nowhere to keep preferences writes nothing at \
               all rather than into whatever folder it was started from.",
        since: RETAIL,
        evidence: Evidence::Private("AC-EVID-O196-SAVE"),
        station: "dereth-testkit::dat::shell::scenario_a_rebind_is_written_beside_the_preferences",
        tier: Tier::Dat,
    },
    behaviour! {
        id: "keymap.storage.saved-maps-remain-specific-to-their-interface",
        says: "Loading, saving and restoring key maps addresses the intended interface even while the other interface is active; saved clears survive returning and reloading.",
        since: THIS_CLIENT,
        divergence: "CD-022",
        evidence: Evidence::Private("AC-EVID-KEYMAP-FACE-STORAGE"),
        station: "dereth-client-shell::lib::input::tests::stored_maps_remain_face_specific_while_classic_is_active",
        tier: Tier::Dat,
    },
    behaviour! {
        id: "keymap.system-keys.the-four-the-desktop-owns-are-taken-last-and-do-nothing",
        says: "The four key combinations the desktop owns are bound last of all, so any other \
               binding for the same key still wins; when one of them does reach the client it is \
               swallowed and changes nothing whatever, while an action the client has no arm for \
               is handed back unconsumed.",
        since: RETAIL,
        evidence: Evidence::Private("AC-EVID-P1-145-ACTION"),
        station: "dereth-testkit::dat::shell::scenario_the_desktops_four_are_taken_last_and_do_nothing",
        tier: Tier::Dat,
    },
    behaviour! {
        id: "keymap.walk-order.the-chat-map-is-walked-first-and-movement-before-the-camera",
        says: "The key maps are walked in the order the client registers them: the chat window's \
               own map is consulted before anything else in its band, movement is consulted before \
               the camera, and the map that leaves the chat bar sits above the barrier a focused \
               text box puts in front of the keyboard -- which is what lets one key still leave it.",
        since: RETAIL,
        evidence: Evidence::Private("AC-EVID-O214-ORDER"),
        station: "dereth-testkit::dat::shell::scenario_the_chat_map_is_walked_first_and_movement_before_the_camera",
        tier: Tier::Dat,
    },
    behaviour! {
        id: "movement.walk-mode-key.holding-it-follows-the-run-by-default-option-both-ways",
        says: "On a character with the shipped defaults, holding the walk-mode key walks and \
               letting go runs; on a character who has turned run-by-default off the same key \
               does the opposite, so the key is read against the option and not against a fixed \
               answer.",
        since: RETAIL,
        evidence: Evidence::Private("AC-EVID-O427-POLARITY"),
        station: "dereth-testkit::dat::shell::scenario_the_walk_mode_key_follows_the_run_by_default_option",
        tier: Tier::Dat,
    },
    behaviour! {
        id: "movement.walk-mode-key.the-option-is-read-on-every-press-and-not-at-start-up",
        says: "A player who changes run-by-default in the middle of a session sees the walk-mode \
               key change with it on the very next press, without having to log out.",
        since: RETAIL,
        evidence: Evidence::Private("AC-EVID-O427-LIVE"),
        station: "dereth-testkit::dat::shell::scenario_the_walk_mode_option_is_read_on_every_press",
        tier: Tier::Dat,
    },
    behaviour! {
        id: "options.character-options.the-page-has-fifty-rows-each-editing-its-own-setting",
        says: "The character options page is built with fifty rows, no two of which change \
               the same setting, the last of them whether player-killer deaths are heard; the rows \
               for side-by-side vitals, weather, distance fog and always-day start off while \
               coordinates on the radar and hearing player-killer deaths start on.",
        since: RETAIL,
        evidence: Evidence::Private("AC-EVID-P165-CHARACTER-OPTIONS"),
        station: "dereth-client::gpu::panels::character_options::the_character_options_page_is_fifty_rows_and_each_edits_its_own_bit",
        tier: Tier::Gpu,
    },
    behaviour! {
        id: "options.character-page.a-deferred-change-reaches-the-shard-eight-minutes-later-and-once",
        says: "A setting the client does not send at once still reaches the shard on its own: the \
               frame carries the timer, and eight minutes after the first unsent change the whole \
               settings record goes out, once and not again.",
        since: RETAIL,
        evidence: Evidence::Private("AC-EVID-O244-FLUSH"),
        station: "dereth-testkit::dat::shell::scenario_a_deferred_change_is_flushed_by_the_frame",
        tier: Tier::Dat,
    },
    behaviour! {
        id: "options.character-page.a-row-ticked-on-stays-ticked-and-unticking-still-works",
        says: "A row of the character options page that the player ticks stays ticked -- drawn, \
               remembered by the row and set in the client's own settings, and still all three a \
               second later -- and a row they untick stays unticked.",
        since: RETAIL,
        evidence: Evidence::Private("AC-EVID-P1-58-TICK"),
        station: "dereth-testkit::dat::shell::scenario_a_ticked_row_stays_ticked_and_unticking_still_works",
        tier: Tier::Dat,
    },
    behaviour! {
        id: "options.character-page.a-tick-changes-only-that-bit-of-what-the-shard-sent",
        says: "Ticking a row changes that one setting of the record the shard sent and nothing \
               else in it, including the settings this client does not understand; ticking \
               something that is already ticked is not a change at all. Some settings go out the \
               moment they are moved and the rest are held until the record is saved.",
        since: RETAIL,
        evidence: Evidence::Private("AC-EVID-O244-WRITE"),
        station: "dereth-testkit::dat::shell::scenario_a_tick_changes_only_that_setting",
        tier: Tier::Dat,
    },
    behaviour! {
        id: "options.character-page.a-tick-raises-one-set-player-option",
        says: "Ticking a box on the character options page sends one change naming that one option \
               and its new value, marks the page changed and moves no other row.",
        since: RETAIL,
        evidence: Evidence::Private("AC-EVID-O62-CHARACTER-PAGE-TICK"),
        station: "dereth-ui-screens::dat::panels::options_character_page::clicking_a_check_box_emits_one_set_player_option_and_nothing_else",
        tier: Tier::Dat,
    },
    behaviour! {
        id: "options.character-page.defaults-follow-the-late-player-view",
        says: "Character Defaults uses the player defaults once the player view arrives, even when the page was built before login.",
        since: THIS_CLIENT,
        divergence: "CD-015",
        evidence: Evidence::Private("AC-EVID-UI-CHARACTER-DEFAULTS"),
        station: "dereth-ui-screens::dat::panels::options_character_page::defaults::restore_defaults_writes_every_row_from_get_default_option_value",
        tier: Tier::Dat,
    },
    behaviour! {
        id: "options.character-page.each-row-shows-the-bit-the-shard-sent-for-it",
        says: "Every row of the character options page shows what the shard actually sent for \
               that character rather than an empty page, each row reading its own setting and no \
               other; and a client that has not been told yet answers that it does not know, \
               which is a different thing from answering no.",
        since: RETAIL,
        evidence: Evidence::Private("AC-EVID-O244-READ"),
        station: "dereth-testkit::dat::shell::scenario_every_row_shows_the_setting_the_shard_sent",
        tier: Tier::Dat,
    },
    behaviour! {
        id: "options.character-page.one-tick-and-one-visit-sends-one-message-with-one-bit-moved",
        says: "One tick and one visit to the page sends exactly one settings message, differing \
               from what the shard sent in the one setting that moved and in nothing else -- the \
               spell bars, the window sizes and every other setting come through untouched -- and \
               a second visit with nothing changed sends nothing.",
        since: RETAIL,
        evidence: Evidence::Private("AC-EVID-O244-ONE"),
        station: "dereth-testkit::dat::shell::scenario_one_tick_and_one_visit_sends_one_message",
        tier: Tier::Dat,
    },
    behaviour! {
        id: "options.character-page.restore-defaults-writes-every-row-from-its-default",
        says: "Restore Defaults on the character options page writes every one of its 50 rows, \
               changed or not, to that option's own default, one option change per row, so the \
               options that default on, such as toggle run, go out on and the rest off -- hearing \
               player-killer deaths among them, though a new character starts with it on -- and \
               each box shows its new state.",
        since: RETAIL,
        evidence: Evidence::Private("AC-EVID-G19-CHARACTER-PAGE"),
        station: "dereth-ui-screens::dat::panels::options_character_page::defaults::restore_defaults_writes_every_row_from_get_default_option_value",
        tier: Tier::Dat,
    },
    behaviour! {
        id: "options.character-page.the-excluded-row-goes-out-at-once-and-cancel-restores-both",
        says: "Ticking a setting on the character options page that turns another one off puts \
               the other row out at once, without the page having to be opened again; cancelling \
               puts both rows back where the page opened and tells the shard about both, so a \
               cancelled change cannot be left standing on the shard.",
        since: RETAIL,
        evidence: Evidence::Private("AC-EVID-P1-34-PAGE"),
        station: "dereth-testkit::dat::shell::scenario_the_excluded_row_goes_out_at_once_and_cancel_restores_both",
        tier: Tier::Dat,
    },
    behaviour! {
        id: "options.character-page.the-page-is-forty-nine-toggles-captioned-from-their-own-tokens",
        says: "Every one of the character options page's 50 check boxes is captioned from its \
               own entry in the shipped string table, such as Keep Combat Targets in View, \
               Display 3D Tooltips and Side By Side Vitals, and none from the client's display \
               and sound settings.",
        since: RETAIL,
        evidence: Evidence::Private("AC-EVID-O62-CHARACTER-PAGE"),
        station: "dereth-ui-screens::dat::panels::options_character_page::every_row_captions_itself_from_its_own_token_and_none_from_the_preference_registry",
        tier: Tier::Dat,
    },
    behaviour! {
        id: "options.character-page.the-settings-it-sends-back-are-byte-for-byte-the-ones-a-real-client-sent",
        says: "Every settings record a recorded retail client sent, taken in and sent back out \
               unchanged, is the same bytes -- so the shape the client saves is the shape the \
               shard was really given, down to the sections it carries and the padding at its end.",
        since: RETAIL,
        evidence: Evidence::Private("AC-EVID-O244-WIRE"),
        station: "dereth-testkit::dat::shell::scenario_the_settings_it_sends_back_are_byte_for_byte_the_recorded_ones",
        tier: Tier::Dat,
    },
    behaviour! {
        id: "options.character-page.with-no-description-nothing-is-sent-and-nothing-is-invented",
        says: "A client the shard has not yet described sends no settings record at all rather \
               than making one up out of the defaults, while a setting the player moves still \
               takes effect locally; and a record the shard shaped is narrowed to the shape this \
               client sends before it goes back.",
        since: RETAIL,
        evidence: Evidence::Private("AC-EVID-O244-EMPTY"),
        station: "dereth-testkit::dat::shell::scenario_with_no_description_nothing_is_sent",
        tier: Tier::Dat,
    },
    behaviour! {
        id: "options.chat-page.a-partly-chosen-group-and-a-wholly-chosen-one-are-drawn-differently",
        says: "A group of chat kinds of which only some are chosen is drawn with a different \
               picture from one where all of them are, and the picture really reaches the frame \
               the client drew rather than only being recorded as wanted; pressing the box once \
               turns the partly chosen group off and again chooses all of it.",
        since: RETAIL,
        evidence: Evidence::Private("AC-EVID-P1-17-IMAGES"),
        station: "dereth-testkit::dat::shell::scenario_a_partly_chosen_group_is_drawn_differently",
        tier: Tier::Dat,
    },
    behaviour! {
        id: "options.chat-page.apply-cancel-and-defaults-do-what-an-option-page-does",
        says: "Cancelling the chat options page puts back what it opened with and touches only \
               the controls that moved, applying takes a fresh snapshot so a later cancel goes \
               back to that instead, and restoring the defaults puts every control back whether \
               or not the player touched it.",
        since: RETAIL,
        evidence: Evidence::Private("AC-EVID-P1-17-BUTTONS"),
        station: "dereth-testkit::dat::shell::scenario_apply_cancel_and_defaults_do_what_an_option_page_does",
        tier: Tier::Dat,
    },
    behaviour! {
        id: "options.chat-page.the-chat-fonts-face-and-size-sit-under-the-windows-opacity",
        says: "The chat font's face and size are chosen on the chat options tab, in two \
               drop-downs just under the windows' two opacity sliders, and the client options \
               tab no longer lists them.",
        since: THIS_CLIENT,
        divergence: "CD-021",
        evidence: Evidence::Private("AC-EVID-UI-OPTIONS-R3-FONTS"),
        station: "dereth-testkit::dat::shell::scenario_the_chat_options_tab_draws_its_controls",
        tier: Tier::Dat,
    },
    behaviour! {
        id: "options.chat-page.the-opacity-slider-writes-what-the-fade-reads-and-keeps-the-two-in-order",
        says: "Moving the idle-opacity slider sets the value every chat window's fade is measured \
               from, so the fade has somewhere to travel; dragging the active-opacity slider below \
               it takes the idle one down with it, so the two can never cross.",
        since: RETAIL,
        evidence: Evidence::Private("AC-EVID-P1-17-OPACITY"),
        station: "dereth-testkit::dat::shell::scenario_the_opacity_slider_writes_what_the_fade_reads",
        tier: Tier::Dat,
    },
    behaviour! {
        id: "options.chat-page.the-tab-comes-up-with-every-control-the-page-declares",
        says: "The chat options tab comes up built rather than empty: two sliders and one filter \
               control per chat window, sixty-four tick boxes between them, six headings and six \
               rules, every caption found in the shipped text, each control showing its own \
               window's default -- and opening it sends nothing.",
        since: RETAIL,
        evidence: Evidence::Private("AC-EVID-P1-17-BUILD"),
        station: "dereth-testkit::dat::shell::scenario_the_chat_options_tab_draws_its_controls",
        tier: Tier::Dat,
    },
    behaviour! {
        id: "options.chat-page.ticking-a-filter-reaches-the-window-that-routes-by-it-and-sends-nothing",
        says: "Turning a group of chat kinds off reaches the chat window's own filter -- the one \
               that decides what it shows -- and the other windows do not move with it. Nothing \
               goes to the shard at the time; the change is kept and travels with the settings \
               record when that is next sent. Turning the group back on restores exactly its own \
               kinds.",
        since: RETAIL,
        evidence: Evidence::Private("AC-EVID-P1-17-FILTER"),
        station: "dereth-testkit::dat::shell::scenario_ticking_a_filter_reaches_the_window_and_sends_nothing",
        tier: Tier::Dat,
    },
    behaviour! {
        id: "options.classic.resolution-follows-the-live-display",
        says: "The classic resolution choice follows the actual display resize on entering the world without starting a confirmation or changing unrelated settings.",
        since: THIS_CLIENT,
        divergence: "CD-015",
        evidence: Evidence::Private("AC-EVID-UI-CLASSIC-RESOLUTION"),
        station: "dereth-classic-ui::lib::runtime::chat_tests::classic_resolution_label_tracks_the_world_resize_after_pregame",
        tier: Tier::Dat,
    },
    behaviour! {
        id: "options.client-page.a-slider-drag-writes-the-preference-every-tick",
        says: "A press on the client options page's sound volume slider writes the sound volume at \
               once, and no other setting, as the position the bar then reports: a press near the \
               left end writes a quiet value and one near the right end a loud one, before \
               anything is applied.",
        since: RETAIL,
        evidence: Evidence::Private("AC-EVID-O213-CLIENT-PAGE"),
        station: "dereth-ui-screens::dat::panels::options_client_page::dragging_the_sound_volume_slider_writes_the_preference_on_every_tick",
        tier: Tier::Dat,
    },
    behaviour! {
        id: "options.client-page.an-unticked-sound-box-greys-its-slider",
        says: "Unticking one of the sound boxes on the client options page switches that sound off \
               and greys the volume slider paired with it; ticking it again switches the sound \
               back on and brings the slider back, and the other two pairs are left alone.",
        since: RETAIL,
        evidence: Evidence::Private("AC-EVID-O213-CLIENT-PAGE-UNTICKED"),
        station: "dereth-ui-screens::dat::panels::options_client_page::unticking_a_sound_check_box_greys_its_paired_slider",
        tier: Tier::Dat,
    },
    behaviour! {
        id: "options.client-page.cancel-and-close-revert-only-what-changed",
        says: "Cancel on the client options page writes back the old value of only the controls \
               that changed, so one moved slider means one write, of its old value, and with \
               nothing changed it writes nothing at all.",
        since: RETAIL,
        evidence: Evidence::Private("AC-EVID-O213-CLIENT-PAGE-CANCEL"),
        station: "dereth-ui-screens::dat::panels::options_client_page::cancel_writes_the_old_value_back_and_only_for_the_controls_that_changed",
        tier: Tier::Dat,
    },
    behaviour! {
        id: "options.client-page.defaults-leave-the-interface-choice-as-it-is",
        says: "The client options page's Defaults button, in either interface, leaves the \
               Interface row at the interface being shown and writes nothing for it, while every \
               other row goes back to its default.",
        since: THIS_CLIENT,
        divergence: "CD-021",
        evidence: Evidence::Private("AC-EVID-OPTIONS-DEFAULTS-INTERFACE"),
        station: "dereth-ui-screens::dat::panels::options_client_page::defaults_leave_the_interface_choice_where_it_is_and_reset_the_other_rows",
        tier: Tier::Dat,
    },
    behaviour! {
        id: "options.client-page.defaults-write-through-to-the-store",
        says: "Defaults on the client options page writes every one of its controls' defaults \
               into the stored settings, but closing the page without Apply puts the old values \
               back; Defaults followed by Apply sticks, and the page reopens on those values, \
               such as mouse-look sensitivity 0.55 and automatic degrades off.",
        since: RETAIL,
        evidence: Evidence::Private("AC-EVID-O222-CLIENT-PAGE-DEFAULTS"),
        station: "dereth-ui-screens::dat::panels::options_preference_store::defaults_writes_through_to_the_store_and_survives_a_reopen",
        tier: Tier::Dat,
    },
    behaviour! {
        id: "options.client-page.defaults-writes-the-eight-sound-preferences",
        says: "Pressing Defaults on the client options page writes every control back to its \
               default, the eight sound settings among them in page order: stereo sound, effect, \
               ambient and interface sounds all on at full volume, and sound played only while the \
               window is active.",
        since: RETAIL,
        evidence: Evidence::Private("AC-EVID-O194-CLIENT-PAGE"),
        station: "dereth-ui-screens::dat::panels::options_client_page::sound_defaults::clicking_defaults_on_the_client_options_page_writes_the_eight_sound_preferences",
        tier: Tier::Dat,
    },
    behaviour! {
        id: "options.client-page.every-row-header-and-slider-end-carries-its-shipped-caption",
        says: "Each of the client options page's six wide sliders carries its two end captions, \
               Soft and Hard, Slow and Fast, Narrow and Wide, Dark and Bright, Speed and Detail, \
               Close and Far, while the mouse-look slider and the paired sound sliders carry \
               none.",
        since: RETAIL,
        evidence: Evidence::Private("AC-EVID-O221-CLIENT-PAGE"),
        station: "dereth-ui-screens::dat::panels::options_client_captions::the_five_headings_and_the_twelve_slider_end_captions_are_the_shared_and_shipped_strings",
        tier: Tier::Dat,
    },
    behaviour! {
        id: "options.client-page.opens-on-the-stored-value-and-cancel-reverts-to-it",
        says: "The client options page opens on what the stored settings hold, even when something \
               else changed them while it was shut, and Cancel after a change puts the setting \
               back to what was stored when the page opened, not to the value the page was built \
               with, in the stored settings as well as on the page.",
        since: RETAIL,
        evidence: Evidence::Private("AC-EVID-O222-CLIENT-PAGE"),
        station: "dereth-ui-screens::dat::panels::options_preference_store::cancel_reverts_to_what_the_store_held_when_the_page_opened",
        tier: Tier::Dat,
    },
    behaviour! {
        id: "options.client-page.the-classic-defaults-reset-the-texture-sizes-as-the-retail-ones-do",
        says: "The classic interface's Client page Defaults puts the landscape and environment \
               texture sizes and the detail textures back where the modern interface's Defaults \
               puts them, as well as the sound, the screen and the camera.",
        since: THIS_CLIENT,
        divergence: "CD-021",
        evidence: Evidence::Private("AC-EVID-OPTIONS-DEFAULTS-TEXTURES"),
        station: "dereth-classic-ui::lib::panels::services::tests::sound_reset_restores_saved_draft_and_defaults_reset_the_texture_sizes",
        tier: Tier::Cpu,
    },
    behaviour! {
        id: "options.client-page.the-classic-graphics-rows-read-as-the-retail-ones",
        says: "The classic interface's Client page names every graphics row as the retail \
               interface's page does, Adaptive Degrade and Adaptive Degrade Bias among them, \
               labels the brightness, bias and degrade distance sliders Dark and Bright, Speed \
               and Detail, Close and Far, lists both texture sizes from Very Low to Very High \
               with the stored size chosen, and leaves both detail-texture boxes free to change.",
        since: THIS_CLIENT,
        divergence: "CD-021",
        evidence: Evidence::Private("AC-EVID-CLASSIC-GRAPHICS-ROWS"),
        station: "dereth-classic-ui::lib::panels::services::tests::the_classic_graphics_rows_carry_the_retail_captions_end_labels_and_texture_steps",
        tier: Tier::Cpu,
    },
    behaviour! {
        id: "options.client-page.the-classic-page-sends-the-environment-detail-textures-as-set",
        says: "With the classic interface up, the Environment Detail Textures setting reaches \
               the world as it is stored, so building and interior detail textures stay drawn, \
               and the classic page sends no texture size of its own over the stored ones.",
        since: THIS_CLIENT,
        divergence: "CD-021",
        evidence: Evidence::Private("AC-EVID-CLASSIC-DETAIL-TEXTURES"),
        station: "dereth-classic-ui::lib::settings_host::tests::the_environment_detail_textures_box_reaches_the_scene_as_stored_and_the_sizes_are_not_sent",
        tier: Tier::Cpu,
    },
    behaviour! {
        id: "options.client-page.the-drop-downs-list-their-registered-choices-and-open-on-the-stored-one",
        says: "Each drop-down on the client options page opens with the row for the stored setting \
               selected and that row's caption on the shut drop-down's face, so a landscape draw \
               distance of 25 reads Extreme; a stored value that no row carries falls back to the \
               first row.",
        since: RETAIL,
        evidence: Evidence::Private("AC-EVID-G19-CLIENT-PAGE"),
        station: "dereth-ui-screens::dat::panels::options_client_drop_downs::the_page_opens_with_the_stored_setting_selected",
        tier: Tier::Dat,
    },
    behaviour! {
        id: "options.client-page.the-terrain-and-sky-modes-are-chosen-on-the-graphics-section",
        says: "The client options page ends its Graphics section with Terrain Mode, Sky Mode \
               and Object Mode: the first two offer World Default and three named styles, the \
               third World Default, Legacy and Modern; picking one applies it at once, it is \
               still chosen after the settings are saved and read back, and Restore Defaults \
               puts all three back to World Default.",
        since: THIS_CLIENT,
        divergence: "CD-012",
        evidence: Evidence::Private("AC-EVID-TERRAIN-MODES-OPTIONS"),
        station: "dereth-ui-screens::dat::panels::options_client_drop_downs::the_terrain_mode_drop_down_lists_the_three_named_modes_and_a_press_chooses_one",
        tier: Tier::Dat,
    },
    behaviour! {
        id: "options.defaults.the-client-starts-in-a-window-and-defaults-keep-its-starting-size",
        says: "A fresh profile starts the client in a window at 1024 by 768, and the client options \
               page's Defaults button puts full screen off and the resolution back at 1024 by 768.",
        since: THIS_CLIENT,
        divergence: "CD-021",
        evidence: Evidence::Private("AC-EVID-OPTIONS-UNIFY-DEFAULTS"),
        station: "dereth-ui-screens::dat::panels::options_client_page::the_client_starts_in_a_window_at_1024x768_and_defaults_restore_that",
        tier: Tier::Dat,
    },
    behaviour! {
        id: "options.distance-fog.turns-world-fog-off-and-back",
        says: "Ticking Disable Distance Fog turns the world's distance fog off, and unticking it \
               turns the fog back on.",
        since: RETAIL,
        evidence: Evidence::Private("AC-EVID-P165-DISTANCE-FOG"),
        station: "dereth-client::gpu::panels::character_options::disable_distance_fog_takes_the_fog_off",
        tier: Tier::Gpu,
    },
    behaviour! {
        id: "options.fellowship.the-same-holds-from-the-shipped-default-and-turning-it-off-again-is-one-message",
        says: "From the settings a character starts with, turning auto-accept on tells the shard \
               that ignoring is off before it says auto-accept is on; turning auto-accept off \
               again turns nothing else on, so it is one message, and logging in again shows the \
               boxes the shard's own answer.",
        since: RETAIL,
        evidence: Evidence::Private("AC-EVID-P1-34-MIRROR"),
        station: "dereth-testkit::dat::shell::scenario_auto_accept_from_the_default_clears_ignoring_at_the_shard",
        tier: Tier::Dat,
    },
    behaviour! {
        id: "options.fellowship.turning-one-of-the-two-on-tells-the-shard-the-other-is-off-first",
        says: "Ignoring fellowship requests and accepting them automatically cannot both be on, \
               and the shard -- which is what actually decides whether a request is accepted -- is \
               told about the one being turned off before the one being turned on. So the box the \
               player sees matches what would really happen to a request, and still does after \
               logging out and back in.",
        since: RETAIL,
        evidence: Evidence::Private("AC-EVID-P1-34-SEQUENCE"),
        station: "dereth-testkit::dat::shell::scenario_one_fellowship_setting_turns_the_other_off_at_the_shard",
        tier: Tier::Dat,
    },
    behaviour! {
        id: "options.gameplay-page.mouse-turning-settings-sets-the-preset-and-nothing-else",
        says: "Pressing Use Mouse Turning Settings on the gameplay options page sets camera \
               stiffness to 0.95, camera adjustment speed to 50, mouse-look sensitivity to 0.7, \
               turns align-to-slope off and inverted mouse look and turning with the camera on, \
               and prints a chat line for each one it changed. It changes no other option and \
               none of the character's options; each page's Defaults button restores defaults.",
        since: RETAIL,
        evidence: Evidence::Private("AC-EVID-OPTIONS-UNIFY-MOUSE-TURNING"),
        station: "dereth-ui-screens::dat::panels::options_gameplay_page::use_mouse_turning_settings_sets_the_preset_and_nothing_else",
        tier: Tier::Dat,
    },
    behaviour! {
        id: "options.key-bindings.a-capture-in-flight-swallows-the-key-and-gives-it-back-afterwards",
        says: "While the page is waiting for a key, the key the player presses does only that: it \
               does not also do whatever it is currently bound to, so binding a movement key does \
               not walk the character across the dialog. The moment the capture is over the key \
               works again.",
        since: RETAIL,
        evidence: Evidence::Private("AC-EVID-O263-SWALLOW"),
        station: "dereth-testkit::dat::shell::scenario_a_capture_in_flight_swallows_the_key",
        tier: Tier::Dat,
    },
    behaviour! {
        id: "options.key-bindings.a-cell-shows-the-key-the-way-the-desktop-names-it",
        says: "A key cell on the key-bindings page shows the key the way a player would name it -- \
               the letter on the key, or the desktop's own name for the ones that have one, like \
               the left control key -- and never the internal symbol the client looks the name up \
               by.",
        since: RETAIL,
        evidence: Evidence::Private("AC-EVID-P1-43-NAME"),
        station: "dereth-testkit::dat::shell::scenario_a_cell_shows_the_key_the_way_the_desktop_names_it",
        tier: Tier::Dat,
    },
    behaviour! {
        id: "options.key-bindings.a-key-already-in-use-asks-first-and-one-that-cannot-be-taken-refuses",
        says: "Pressing a key that is already doing something else asks before taking it: \
               answering no leaves both the new row and the old one exactly as they were, and \
               answering yes moves the key and takes it off the action that had it. A key bound to \
               something the player may not rebind is refused outright, in a box with one button, \
               and nothing changes.",
        since: RETAIL,
        evidence: Evidence::Private("AC-EVID-P1-43-CONFLICT"),
        station: "dereth-testkit::dat::shell::scenario_a_key_already_in_use_asks_first_and_one_that_cannot_be_taken_refuses",
        tier: Tier::Dat,
    },
    behaviour! {
        id: "options.key-bindings.a-key-pressed-over-a-row-rebinds-it-and-frees-the-old-key",
        says: "Clicking a row's key button and then pressing a key rebinds that action to the new \
               key in a running client, and the key it was taken from is left bound to nothing at \
               all rather than simply removed -- which is what stops the shipped default coming \
               back and leaving both keys firing.",
        since: RETAIL,
        evidence: Evidence::Private("AC-EVID-O263-REBIND"),
        station: "dereth-testkit::dat::shell::scenario_a_key_pressed_over_a_row_rebinds_it",
        tier: Tier::Dat,
    },
    behaviour! {
        id: "options.key-bindings.a-press-on-a-cell-waits-for-a-key-and-undo-puts-the-old-one-back",
        says: "Pressing a key cell puts the page into waiting for a key, and says so on the screen \
               -- naming the action and the key that cancels. Escape cancels it and changes \
               nothing; a key pressed instead becomes the new binding and the cell redraws. Undo \
               lights up when something has changed, and pressing it puts the old key back and \
               greys itself again.",
        since: RETAIL,
        evidence: Evidence::Private("AC-EVID-P1-43-GESTURE"),
        station: "dereth-testkit::dat::shell::scenario_a_press_on_a_cell_waits_for_a_key_and_undo_puts_the_old_one_back",
        tier: Tier::Dat,
    },
    behaviour! {
        id: "options.key-bindings.a-set-of-keys-can-be-saved-under-a-name-and-loaded-back",
        says: "The page's save button asks for a name and writes the keys out under it; the load \
               button lists what has been saved, and choosing one puts those keys back -- in what \
               the client answers to and on the page, which redraws the cells.",
        since: RETAIL,
        evidence: Evidence::Private("AC-EVID-P1-43-FILES"),
        station: "dereth-testkit::dat::shell::scenario_a_set_of_keys_can_be_saved_under_a_name_and_loaded_back",
        tier: Tier::Dat,
    },
    behaviour! {
        id: "options.key-bindings.classic-wheel-covers-binding-columns",
        says: "The mouse wheel scrolls the classic keyboard list over action names and every binding column, and slots remain clickable.",
        since: THIS_CLIENT,
        divergence: "CD-022",
        evidence: Evidence::Private("AC-EVID-UI-CLASSIC-KEY-WHEEL"),
        station: "dereth-classic-ui::lib::panels::pregame::tests::the_real_keyboard_list_scrolls_over_names_and_each_binding_column",
        tier: Tier::Cpu,
    },
    behaviour! {
        id: "options.key-bindings.default-file-has-a-friendly-label",
        says: "The default key map is labelled Default; named schemes keep their names and the actual persistence filename is unchanged.",
        since: THIS_CLIENT,
        divergence: "CD-022",
        evidence: Evidence::Private("AC-EVID-UI-KEY-DEFAULT"),
        station: "dereth-client-shell::lib::input::tests::default_keymap_label_keeps_real_filenames_and_named_schemes",
        tier: Tier::Dat,
    },
    behaviour! {
        id: "options.key-bindings.every-section-is-titled-in-words",
        says: "Every heading down the key-bindings page is a word a player would read -- Movement \
               and the rest -- and not the token the client looks that word up by.",
        since: RETAIL,
        evidence: Evidence::Private("AC-EVID-P1-43-TITLES"),
        station: "dereth-testkit::dat::shell::scenario_every_section_is_titled_in_words",
        tier: Tier::Dat,
    },
    behaviour! {
        id: "options.key-bindings.modern-hides-six-rows-without-removing-actions",
        says: "The modern keyboard editor omits six optional action rows while keeping their actions and saved bindings available.",
        since: THIS_CLIENT,
        divergence: "CD-022",
        evidence: Evidence::Private("AC-EVID-UI-KEY-ROWS"),
        station: "dereth-ui-screens::dat::panels::options_key_bindings::modern_omits_only_the_six_requested_rows_and_keeps_their_bindable_actions",
        tier: Tier::Dat,
    },
    behaviour! {
        id: "options.key-bindings.own-action-conflicts-use-the-row-caption",
        says: "A conflict with a saved binding to this client's own action names that action in the overwrite prompt, even when the editor does not list its row.",
        since: THIS_CLIENT,
        divergence: "CD-019",
        evidence: Evidence::Private("AC-EVID-R2-OWN-KEYS"),
        station: "dereth-ui-screens::dat::panels::options_key_bindings::own_action_conflicts_show_the_action_name_in_the_actual_overwrite_dialog",
        tier: Tier::Dat,
    },
    behaviour! {
        id: "options.key-bindings.resting-on-a-cell-says-what-a-press-there-would-do",
        says: "Resting the pointer on a key cell shows a note saying what a press would do: a cell \
               with a key in it names the key and says how to take it away, and an empty one says \
               how to fill it and offers nothing to erase. Erasing the key in a cell changes its \
               note to the empty one's.",
        since: RETAIL,
        evidence: Evidence::Private("AC-EVID-P1-43-TOOLTIP"),
        station: "dereth-testkit::dat::shell::scenario_resting_on_a_cell_says_what_a_press_there_would_do",
        tier: Tier::Dat,
    },
    behaviour! {
        id: "options.key-bindings.restoring-the-defaults-gives-back-the-shipped-keys-and-not-the-saved-ones",
        says: "Restoring the defaults gives back the keys the client shipped with, even on a \
               client that started with a saved set already loaded -- what the page shows as \
               current is the saved set and what it restores is the shipped one, and they are not \
               the same thing.",
        since: RETAIL,
        evidence: Evidence::Private("AC-EVID-P1-43-DEFAULTS"),
        station: "dereth-testkit::dat::shell::scenario_restoring_the_defaults_gives_back_the_shipped_keys",
        tier: Tier::Dat,
    },
    behaviour! {
        id: "options.key-bindings.saving-over-a-set-asks-first-and-one-that-cannot-be-written-refuses",
        says: "Saving under a name that already exists asks before replacing it: no leaves the old \
               file untouched and yes writes the keys that are live now. A file that cannot be \
               written is never asked about at all -- it says so in a box with one button and the \
               file is left alone.",
        since: RETAIL,
        evidence: Evidence::Private("AC-EVID-P1-43-OVERWRITE"),
        station: "dereth-testkit::dat::shell::scenario_saving_over_a_set_asks_first_and_one_that_cannot_be_written_refuses",
        tier: Tier::Dat,
    },
    behaviour! {
        id: "options.key-bindings.taking-a-key-clears-it-from-the-rows-that-had-it-on-the-screen",
        says: "Taking a key from another action clears it from that row on the screen, not only in \
               what the client answers to -- including when the key was on two rows in two \
               different sections, both of which lose it at once. The same key held with a \
               modifier is a different key and is left where it is.",
        since: RETAIL,
        evidence: Evidence::Private("AC-EVID-P1-43-REFRESH"),
        station: "dereth-testkit::dat::shell::scenario_taking_a_key_clears_it_from_the_rows_that_had_it",
        tier: Tier::Dat,
    },
    behaviour! {
        id: "options.key-bindings.the-page-builds-one-row-for-every-bindable-action-once",
        says: "The key-bindings page of a running client shows a row for every action the player \
               is allowed to bind and for no others, with its section headings, and it builds them \
               once when the page comes up rather than again on every frame.",
        since: RETAIL,
        evidence: Evidence::Private("AC-EVID-O263-ROWS"),
        station: "dereth-testkit::dat::shell::scenario_the_key_binding_page_builds_one_row_per_bindable_action",
        tier: Tier::Dat,
    },
    behaviour! {
        id: "options.key-bindings.the-questions-about-a-key-in-use-are-the-shipped-sentences-with-the-key-and-the-action-in-them",
        says: "The question about a key already in use is the shipped sentence with the key and \
               the action's name in it; when the key is in use several times over it is the other \
               shipped sentence, with one line per use; and the refusal for a key that cannot be \
               taken names the key rather than the row it was pressed on.",
        since: RETAIL,
        evidence: Evidence::Private("AC-EVID-P1-43-PROMPTS"),
        station: "dereth-testkit::dat::shell::scenario_the_questions_about_a_key_in_use_are_the_shipped_sentences",
        tier: Tier::Dat,
    },
    behaviour! {
        id: "options.key-bindings.undo-opens-greyed-because-nothing-has-changed-yet",
        says: "The undo button on the key-bindings page is greyed when the page comes up, because \
               nothing has been changed yet -- it is not lit before the player has touched \
               anything.",
        since: RETAIL,
        evidence: Evidence::Private("AC-EVID-P1-43-REVERT"),
        station: "dereth-testkit::dat::shell::scenario_undo_opens_greyed_because_nothing_has_changed_yet",
        tier: Tier::Dat,
    },
    behaviour! {
        id: "options.pages.a-row-for-what-the-worlds-era-lacks-is-not-shown",
        says: "In a world whose era lacks what an option sets (rare items, cloaks, titles, \
               houses, the trade window), neither interface's options pages show its row: the \
               rows below move up into its place, and nothing greys it.",
        since: THIS_CLIENT,
        divergence: "CD-021",
        evidence: Evidence::Private("AC-EVID-UI-OPTIONS-R3-ERA"),
        station: "dereth-ui-screens::dat::panels::options_character_page::a_row_for_what_the_worlds_era_lacks_leaves_the_page",
        tier: Tier::Dat,
    },
    behaviour! {
        id: "options.pages.both-interfaces-draw-the-same-four-pages-under-the-same-headings",
        says: "Both interfaces' options windows have the same four pages, Game and Support, \
               Character Options, Chat Options and Client Options, and each page's rows sit \
               under the same headings in both; the modern interface's character options page \
               has seven headings over its 50 check boxes and its client options page five \
               (Sound, Display, Graphics Quality, Era Look, Camera and Mouse), with no row for \
               Sync with Refresh Rate and a row for the landscape's detail texture.",
        since: THIS_CLIENT,
        divergence: "CD-021",
        evidence: Evidence::Private("AC-EVID-OPTIONS-UNIFY-PAGES"),
        station: "dereth-ui-screens::dat::panels::options_character_page::the_page_is_seven_headings_seven_separators_and_fifty_toggles_off_the_shipped_tree",
        tier: Tier::Dat,
    },
    behaviour! {
        id: "options.pages.the-classic-window-draws-the-shared-pages-its-own-way",
        says: "The classic interface's options window has the same four pages: its Options page \
               is Leave World, Exit Game, Configure Keyboard, Urgent Assistance and Report Abuse, \
               with no Setup 3D Acceleration, In-Game Help or Retail Interface button; the \
               interface is chosen on its Client page, as on the retail one, and Apply writes \
               the choice.",
        since: THIS_CLIENT,
        divergence: "CD-021",
        evidence: Evidence::Private("AC-EVID-OPTIONS-UNIFY-CLASSIC-PAGES"),
        station: "dereth-classic-ui::lib::panels::services::tests::the_options_page_has_the_shared_buttons_and_the_interface_choice_is_a_client_row",
        tier: Tier::Cpu,
    },
    behaviour! {
        id: "options.panel-boxes.each-panel-option-box-draws-its-caption-and-a-press-names-its-option",
        says: "The five character-option boxes that live on the allegiance and fellowship panels \
               each draw the caption the shipped string table gives their option, laid out and \
               measured in a font rather than merely recorded.",
        since: RETAIL,
        evidence: Evidence::Private("AC-EVID-G27-PANEL-BOXES"),
        station: "dereth-ui-screens::dat::panels::panel_option_checkboxes::every_panel_option_box_places_the_glyphs_of_its_id_playeroption_caption",
        tier: Tier::Dat,
    },
    behaviour! {
        id: "options.panel-boxes.share-experience-names-luminance-only-on-a-world-with-it",
        says: "The fellowship's sharing box, and the same option on the character options page, \
               read Share Fellowship Experience and Luminance as the interface's strings have \
               it on a world with luminance, and Share Fellowship Experience on one without.",
        since: THIS_CLIENT,
        divergence: "CD-010",
        evidence: Evidence::Private("AC-EVID-R2-SHARE-LUMINANCE"),
        station: "dereth-ui-screens::dat::panels::panel_option_checkboxes::the_share_experience_box_names_luminance_only_on_a_world_with_luminance",
        tier: Tier::Dat,
    },
    behaviour! {
        id: "options.preferences-file.a-retail-shaped-file-applies-every-key-the-options-page-attaches",
        says: "A preferences file written the way the original client writes it, each key under \
               its section heading, applies every one of the 34 settings the option pages carry a \
               row for; the other nine keys in the file belong to other parts of the client and \
               are left to them rather than treated as errors.",
        since: RETAIL,
        evidence: Evidence::Private("AC-EVID-O933-PREFERENCES-FILE"),
        station: "dereth-ui-screens::cpu::panels::options_preferences_file::the_census_of_a_retail_shaped_profile",
        tier: Tier::Cpu,
    },
    behaviour! {
        id: "options.preferences-file.drop-down-values-load-by-label-or-index",
        says: "A drop-down setting in the preferences file loads either by its choice's name, \
               matched regardless of case, or by a number that is taken as a position in the \
               choice list; a number past the end, a negative number or an unknown word picks the \
               first choice, so a drop-down setting never fails to load.",
        since: RETAIL,
        evidence: Evidence::Private("AC-EVID-O933-PREFERENCES-FILE-DROP"),
        station: "dereth-ui-screens::cpu::panels::options_preferences_file::a_number_in_the_file_is_an_index_into_the_choice_list_and_a_label_matches_case_insensitively",
        tier: Tier::Cpu,
    },
    behaviour! {
        id: "options.preferences.the-options-page-reads-the-preferences-file-not-the-defaults",
        says: "The options page shows what the player's own preferences file says -- sound on, \
               each volume where he left it, his field of view -- rather than the client's \
               built-in defaults, which would show every sound muted.",
        since: RETAIL,
        evidence: Evidence::Private("AC-EVID-F67-PREFERENCES"),
        station: "dereth-client::gpu::panels::options_reflect_preferences::the_option_store_the_page_reads_reflects_the_preferences_file_and_not_the_defaults",
        tier: Tier::Gpu,
    },
    behaviour! {
        id: "options.side-by-side-vitals.swaps-the-vitals-windows",
        says: "Ticking Side By Side Vitals takes the stacked vitals down and puts the side-by-side \
               window up, and it stays that way through later frames; unticking it puts the \
               stacked vitals back and the side-by-side window away.",
        since: RETAIL,
        evidence: Evidence::Private("AC-EVID-P165-SIDE-BY-SIDE-VITALS"),
        station: "dereth-client::gpu::panels::character_options::side_by_side_vitals_shows_one_vitals_window_and_hides_the_other",
        tier: Tier::Gpu,
    },
    behaviour! {
        id: "pointer.a-button-lights-under-the-pointer-and-sinks-under-the-press",
        says: "A button the pointer is resting on lights up, sinks while it is held down, comes \
               back up to merely lit the moment the pointer slides off it while still held, and \
               goes back to resting when the button is let go -- four looks, in that order, from \
               real pointer movement.",
        since: RETAIL,
        evidence: Evidence::Private("AC-EVID-O36-STATES"),
        station: "dereth-testkit::dat::shell::scenario_a_button_lights_under_the_pointer_and_sinks_under_the_press",
        tier: Tier::Dat,
    },
    behaviour! {
        id: "pointer.a-move-makes-what-is-under-the-pointer-the-one-entered-until-it-leaves",
        says: "Before the pointer has ever moved nothing is under it; one move over a button makes \
               that button the thing under the pointer; and the pointer leaving the window clears \
               it again -- which is the one thing that does.",
        since: RETAIL,
        evidence: Evidence::Private("AC-EVID-O36-HIT-TEST"),
        station: "dereth-testkit::dat::shell::scenario_a_move_makes_what_is_under_the_pointer_the_one_entered",
        tier: Tier::Dat,
    },
    behaviour! {
        id: "pointer.a-press-dragged-off-a-button-releases-it-without-firing-it",
        says: "A press that begins on a button and is dragged off it before the button comes up \
               does not fire it: the press and the release both happen and reach it, and what the \
               button does is not done.",
        since: RETAIL,
        evidence: Evidence::Private("AC-EVID-O36-DRAG-OFF"),
        station: "dereth-testkit::dat::shell::scenario_a_press_dragged_off_a_button_releases_it_without_firing_it",
        tier: Tier::Dat,
    },
    behaviour! {
        id: "pointer.click.a-press-on-a-toolbar-button-opens-the-panel-it-owns",
        says: "A real press on a button of the toolbar -- the pointer moved onto the button the \
               layout placed, pressed and let go -- opens the panel that button owns and brings \
               the stack of panels up with it, from nothing but the press.",
        since: RETAIL,
        evidence: Evidence::Private("AC-EVID-O36-TOOLBAR"),
        station: "dereth-testkit::dat::shell::scenario_a_press_on_a_toolbar_button_opens_the_panel_it_owns",
        tier: Tier::Dat,
    },
    behaviour! {
        id: "pointer.click.every-button-the-wizard-builds-can-be-pressed-including-the-three-it-greys",
        says: "Every button the character-creation wizard builds is something the pointer can land \
               on, including the three tabs the wizard greys out again on every page change -- \
               being greyed must not make a button invisible to the pointer for the rest of its \
               life.",
        since: RETAIL,
        evidence: Evidence::Private("AC-EVID-O44-BUTTONS"),
        station: "dereth-testkit::dat::shell::scenario_every_button_the_wizard_builds_can_be_pressed",
        tier: Tier::Dat,
    },
    behaviour! {
        id: "pointer.click.one-press-on-a-character-picks-it-and-two-takes-them-into-the-world",
        says: "One press on a row of the character list picks that character -- a row the list did \
               not pick for itself, so the change is evidence -- and a second press straight after \
               it, which is a double one, takes that character into the world.",
        since: RETAIL,
        evidence: Evidence::Private("AC-EVID-O36-ROW"),
        station: "dereth-testkit::dat::shell::scenario_one_press_on_a_character_picks_it_and_two_takes_them_into_the_world",
        tier: Tier::Dat,
    },
    behaviour! {
        id: "pointer.click.the-character-lists-own-buttons-each-raise-what-they-name",
        says: "Each button along the bottom of the character list does its own thing to a real \
               press: one puts the wizard up, one shows the credits, one raises the question about \
               deleting the chosen character and one the question about leaving -- and the first \
               of those questions is a real box that has to be dismissed before the button behind \
               it can be pressed.",
        since: RETAIL,
        evidence: Evidence::Private("AC-EVID-O36-BUTTONS"),
        station: "dereth-testkit::dat::shell::scenario_the_character_lists_own_buttons_each_raise_what_they_name",
        tier: Tier::Dat,
    },
    behaviour! {
        id: "pointer.click.the-same-press-works-on-the-character-list-and-in-the-wizard",
        says: "One press of the button really reaches the screen it lands on, both on the \
               character list -- where it puts the wizard up -- and in the wizard itself, where \
               pressing a tab changes the page. The same gesture on both, through the client's own \
               pointer.",
        since: RETAIL,
        evidence: Evidence::Private("AC-EVID-O44"),
        station: "dereth-testkit::dat::shell::scenario_the_same_press_works_on_the_character_list_and_in_the_wizard",
        tier: Tier::Dat,
    },
    behaviour! {
        id: "pointer.click.the-wizards-arrows-and-its-way-out-all-answer-a-press",
        says: "The wizard's forward and back arrows move it a page each way when they are pressed, \
               and the way out raises the question about leaving -- all three from a real press \
               where a player makes one.",
        since: RETAIL,
        evidence: Evidence::Private("AC-EVID-O44-CHROME"),
        station: "dereth-testkit::dat::shell::scenario_the_wizards_arrows_and_its_way_out_all_answer_a_press",
        tier: Tier::Dat,
    },
    behaviour! {
        id: "pointer.the-left-button-and-the-wheel-each-belong-to-one-shipped-map",
        says: "The left button is bound in two of the shipped sets of keys and only one of them is \
               a set this client ever uses -- the interface's own -- so nothing else can take a \
               press away from the interface. The wheel is bound in exactly one set, and that one \
               is not put in front of the client at start-up but when something takes the \
               keyboard. A real press carries the interface's own set and its own action.",
        since: RETAIL,
        evidence: Evidence::Private("AC-EVID-O44-MAPS"),
        station: "dereth-testkit::dat::shell::scenario_the_left_button_and_the_wheel_each_belong_to_one_shipped_map",
        tier: Tier::Dat,
    },
    behaviour! {
        id: "pointer.wheel.a-detent-over-a-check-box-scrolls-the-list-and-leaves-the-box-alone",
        says: "One turn of the wheel over a check box inside a scrolling list scrolls the list \
               that owns it and leaves the box exactly as it was.",
        since: RETAIL,
        evidence: Evidence::Private("AC-EVID-P1-42-WHEEL"),
        station: "dereth-testkit::dat::shell::scenario_a_detent_over_a_tick_box_scrolls_the_list",
        tier: Tier::Dat,
    },
    behaviour! {
        id: "pointer.wheel.one-detent-over-the-chat-log-moves-it-one-line-and-the-other-way-puts-it-back",
        says: "One turn of the wheel over the chat log moves it by one line -- the same line one \
               press of the bar's arrow takes, not a whole page -- and turning it the other way \
               puts it back exactly. Five turns are five lines. At the end of the log the wheel is \
               held there rather than ignored: the press still reaches the log, it simply has \
               nowhere further to go.",
        since: RETAIL,
        evidence: Evidence::Private("AC-EVID-O245-DETENT"),
        station: "dereth-testkit::dat::shell::scenario_one_detent_over_the_chat_log_moves_it_one_line",
        tier: Tier::Dat,
    },
    behaviour! {
        id: "pointer.wheel.pressing-the-log-is-what-lets-the-wheel-move-it-and-typing-does-not-take-it-away",
        says: "The wheel over the chat log works once something has been pressed: with nothing \
               holding the keyboard a turn of the wheel does nothing at all. Pressing the log \
               itself is enough -- it can be picked at even though it cannot be typed into -- and \
               so is typing in the entry below it, which is the case that used to be the only one \
               that worked. Letting the keyboard go makes the wheel inert again.",
        since: RETAIL,
        evidence: Evidence::Private("AC-EVID-O245-ARMED"),
        station: "dereth-testkit::dat::shell::scenario_pressing_the_log_is_what_lets_the_wheel_move_it",
        tier: Tier::Dat,
    },
    behaviour! {
        id: "pointer.wheel.the-arming-happens-when-the-keyboard-moves-and-once-per-move",
        says: "What the keyboard's new holder can be driven with is worked out when the keyboard \
               moves and not again on every frame -- once when it arrives and once when it leaves \
               -- so a key held down across frames is not thrown away and picked up again sixty \
               times a second.",
        since: RETAIL,
        evidence: Evidence::Private("AC-EVID-O245-EDGE"),
        station: "dereth-testkit::dat::shell::scenario_the_arming_happens_when_the_keyboard_moves_and_once_per_move",
        tier: Tier::Dat,
    },
    behaviour! {
        id: "pointer.wheel.the-same-box-still-answers-a-click-and-so-does-the-bar",
        says: "The very same check box still flips when it is clicked, and a click on the list's \
               own scrollbar still scrolls the list -- so what refuses the wheel refuses the \
               wheel and nothing else.",
        since: RETAIL,
        evidence: Evidence::Private("AC-EVID-P1-42-CLICK"),
        station: "dereth-testkit::dat::shell::scenario_the_same_box_still_answers_a_click_and_so_does_the_bar",
        tier: Tier::Dat,
    },
    behaviour! {
        id: "relog.state.a-session-that-changed-nothing-deferred-saves-nothing-at-the-log-off",
        says: "A session whose only change was a setting that had already gone out at the moment \
               it was ticked saves nothing at all when the player logs off, and the departure \
               itself still goes -- so the saving on the way out is a saving of what is unsaved \
               and not a blanket send.",
        since: RETAIL,
        evidence: Evidence::Private("AC-EVID-P1-122-CLEAN"),
        station: "dereth-testkit::dat::shell::scenario_a_session_that_changed_nothing_deferred_saves_nothing_at_the_log_off",
        tier: Tier::Dat,
    },
    behaviour! {
        id: "relog.state.every-deferred-change-is-saved-at-the-log-off-and-found-by-the-next-session",
        says: "Everything a player changed that the client keeps to itself until the way out -- \
               the settings that are not saved as they are ticked, the chat window's own filters, \
               where that window sits -- is sent to the shard when they log off, ahead of the \
               departure itself, so the next login on that character finds every one of them as \
               they were left rather than back at its default. The notebook is the one thing on \
               that list that never crosses the wire at all.",
        since: RETAIL,
        evidence: Evidence::Private("AC-EVID-P1-122-RELOG"),
        station: "dereth-testkit::dat::shell::scenario_every_deferred_change_is_saved_at_the_log_off_and_found_by_the_next_session",
        tier: Tier::Dat,
    },
    behaviour! {
        id: "relog.state.only-the-shortcut-and-the-auto-saved-option-reach-the-shard-at-the-gesture",
        says: "Of the things a player changes in a session, only two kinds reach the shard at the \
               moment they are changed: an item dropped on the shortcut bar, and the handful of \
               settings the client saves as they are ticked. Everything else -- the other \
               settings, the chat window's filters, its placement -- puts nothing on the wire at \
               all when it is changed, though the client is holding every one of them.",
        since: RETAIL,
        evidence: Evidence::Private("AC-EVID-P1-122-GESTURE"),
        station: "dereth-testkit::dat::shell::scenario_only_the_shortcut_and_the_auto_saved_option_reach_the_shard_at_the_gesture",
        tier: Tier::Dat,
    },
    behaviour! {
        id: "selection.fellow.a-selection-outside-the-fellowship-starts-at-one-end-and-with-no-fellowship-nothing-moves",
        says: "With something selected that is not in the fellowship -- or with nothing selected \
               at all -- the forward key starts at the first member and the backward key at the \
               last. A player in no fellowship presses either and the selection does not move.",
        since: RETAIL,
        evidence: Evidence::Private("AC-EVID-P1-142-EDGES"),
        station: "dereth-testkit::dat::shell::scenario_an_outsider_starts_at_one_end_and_no_fellowship_moves_nothing",
        tier: Tier::Dat,
    },
    behaviour! {
        id: "selection.fellow.one-key-walks-the-fellowship-forward-and-the-other-back-both-wrapping",
        says: "One key selects the next member of the fellowship and another the previous one, \
               each wrapping round the ends, and the player's own character is one of the members \
               they land on. The keys the shipped map binds to the two really reach them.",
        since: RETAIL,
        evidence: Evidence::Private("AC-EVID-P1-142-CYCLE"),
        station: "dereth-testkit::dat::shell::scenario_the_two_keys_walk_the_fellowship_both_ways_and_wrap",
        tier: Tier::Dat,
    },
    behaviour! {
        id: "selection.fellow.the-cycle-follows-the-order-the-panel-shows",
        says: "The order the two keys walk the fellowship in is the order the fellowship panel \
               lists it in, so the selection moves down the list the player is looking at.",
        since: RETAIL,
        evidence: Evidence::Private("AC-EVID-P1-142-ORDER"),
        station: "dereth-testkit::dat::shell::scenario_the_fellow_cycle_follows_the_panels_order",
        tier: Tier::Dat,
    },
    behaviour! {
        id: "selection.previous.a-target-that-was-cleared-comes-back",
        says: "Clearing the selection is itself remembered, so the key that goes back brings the \
               cleared target back; going back again is then the empty step and leaves it alone.",
        since: RETAIL,
        evidence: Evidence::Private("AC-EVID-P1-143-CLEAR"),
        station: "dereth-testkit::dat::shell::scenario_a_cleared_target_comes_back",
        tier: Tier::Dat,
    },
    behaviour! {
        id: "selection.previous.the-key-goes-back-one-and-is-a-toggle-rather-than-a-stack",
        says: "The key the shipped map binds to it selects whatever was selected before the \
               current thing, and pressing it again returns to where it started -- it remembers \
               one step and not a trail, so nothing further back is reachable by pressing it.",
        since: RETAIL,
        evidence: Evidence::Private("AC-EVID-P1-143-TOGGLE"),
        station: "dereth-testkit::dat::shell::scenario_the_key_that_goes_back_is_a_toggle_and_not_a_stack",
        tier: Tier::Dat,
    },
    behaviour! {
        id: "selection.previous.with-nothing-behind-it-the-key-selects-nothing",
        says: "With nothing yet selected in the session the key selects nothing, and it is still \
               inert immediately after the very first selection, because there is nothing behind \
               that one either.",
        since: RETAIL,
        evidence: Evidence::Private("AC-EVID-P1-143-EMPTY"),
        station: "dereth-testkit::dat::shell::scenario_with_nothing_behind_it_the_key_selects_nothing",
        tier: Tier::Dat,
    },
    behaviour! {
        id: "shell-only.character-select.a-pick-the-returning-list-cannot-honour-falls-back-to-the-shards-own-order",
        says: "The remembered pick only decides the highlight when it still can: a player who \
               never picked anybody comes back to the first row the shard listed, and so does one \
               whose remembered character is not in the list the shard sends next. Neither leaves \
               the highlight empty or on a row that is no longer there.",
        since: RETAIL,
        evidence: Evidence::Private("AC-EVID-G23-FALLBACK"),
        station: "dereth-testkit::dat::shell::scenario_a_pick_the_returning_list_cannot_honour_falls_back_to_the_shards_own_order",
        tier: Tier::Dat,
    },
    behaviour! {
        id: "shell-only.character-select.the-returning-list-selects-the-character-just-played",
        says: "Coming back to the character list after playing, the row the list highlights is the \
               character that was just played -- not the first one the shard listed. The pick is \
               remembered across the whole trip into the world and back, although everything the \
               screen itself held was thrown away when the world came up.",
        since: RETAIL,
        evidence: Evidence::Private("AC-EVID-G23"),
        station: "dereth-testkit::dat::shell::scenario_the_returning_character_list_selects_the_character_that_was_just_played",
        tier: Tier::Dat,
    },
    behaviour! {
        id: "shell-only.character-set.a-list-arriving-in-the-world-sends-the-player-back-to-the-character-screen",
        says: "A character list arriving while the player is in the world is what carries them \
               back to the character screen, and the same list arriving while they are already \
               there moves nothing -- it rebuilds the rows and no more. With no list arriving at \
               all the player stays in the world however long the client runs, so it is the \
               arrival and not the frame that does it.",
        since: RETAIL,
        evidence: Evidence::Private("AC-EVID-O280-SET"),
        station: "dereth-testkit::dat::shell::scenario_a_list_arriving_in_the_world_sends_the_player_back_and_at_the_list_does_not",
        tier: Tier::Dat,
    },
    behaviour! {
        id: "shell-only.character-set.every-arrival-is-a-notice-even-when-the-list-has-not-changed",
        says: "Every character list the session decodes is counted as an arrival, including one \
               whose characters are exactly the ones already held -- which is the list the shard \
               sends after a log-off. A client that compared the lists instead would see nothing \
               happen at the one moment something did.",
        since: RETAIL,
        evidence: Evidence::Private("AC-EVID-O280-NOTICE"),
        station: "dereth-testkit::dat::shell::scenario_every_character_set_the_session_decodes_is_an_arrival",
        tier: Tier::Dat,
    },
    behaviour! {
        id: "shell-only.enter-world.the-way-into-the-world-opens-again-after-a-log-off",
        says: "Going into the world, coming back out to the character screen and going in again \
               all work: the step that carries the player into the world is taken on each entry \
               rather than only on the first, so a player who logs off is not left looking at the \
               character list with a world still running behind it.",
        since: RETAIL,
        evidence: Evidence::Private("AC-EVID-O280-REENTRY"),
        station: "dereth-testkit::dat::shell::scenario_the_way_into_the_world_opens_again_after_a_log_off",
        tier: Tier::Dat,
    },
    behaviour! {
        id: "shell-only.log-off.answering-yes-asks-to-log-off-and-moves-no-screen-of-its-own",
        says: "Answering yes to leaving the world asks the shard to log the character off and \
               changes no screen in that frame: the player stays in the world for as long as the \
               shard takes to answer, which is the window the leaving is played out in, and what \
               brings them back is the shard's own answer rather than the button.",
        since: RETAIL,
        evidence: Evidence::Private("AC-EVID-O280-CONFIRM"),
        station: "dereth-testkit::dat::shell::scenario_answering_yes_asks_to_log_off_and_moves_no_screen_of_its_own",
        tier: Tier::Dat,
    },
    behaviour! {
        id: "shell-only.log-off.the-shards-answer-does-not-end-a-client-that-has-a-screen-to-go-back-to",
        says: "The shard's answer to a log-off does not end a client that has screens: it keeps \
               running, frame after frame, with the character screen to go back to. A client \
               built with no screens at all still ends on that same answer, because there is \
               nothing for it to go back to.",
        since: RETAIL,
        evidence: Evidence::Private("AC-EVID-O280-ALIVE"),
        station: "dereth-testkit::dat::shell::scenario_the_shards_log_off_answer_does_not_end_a_client_with_screens",
        tier: Tier::Dat,
    },
    behaviour! {
        id: "shell.input-replay.validates-and-preserves-device-events",
        says: "A headless desktop run validates its input replay before startup, rejects malformed \
               or out-of-order events, and delivers each device event once at its requested frame \
               in file order, retaining pointer coordinates, key identities and translated text.",
        since: TOOLING,
        evidence: Evidence::Private("AC-EVID-HEADLESS-INPUT-REPLAY"),
        station: "dereth-client::bin.dereth-client::input_replay::tests::replay_preserves_host_events_and_order_at_each_frame",
        tier: Tier::Cpu,
    },
    behaviour! {
        id: "text-entry.backspace.a-tap-deletes-one-character-and-nothing-follows-it",
        says: "Backspace pressed and let go deletes one character, and no further character is \
               lost however long the client runs afterwards.",
        since: RETAIL,
        evidence: Evidence::Private("AC-EVID-P1-10-TAP"),
        station: "dereth-testkit::dat::shell::scenario_a_backspace_tap_deletes_one_and_nothing_follows",
        tier: Tier::Dat,
    },
    behaviour! {
        id: "text-entry.backspace.one-press-deletes-one-character-and-a-hold-repeats-at-the-systems-rate",
        says: "One press of Backspace deletes exactly one character from the line being typed; \
               holding it deletes nothing more until the system's repeat delay has passed and then \
               one per repeat interval, and letting go stops it.",
        since: RETAIL,
        evidence: Evidence::Private("AC-EVID-P1-10-HOLD"),
        station: "dereth-testkit::dat::shell::scenario_one_backspace_deletes_one_character_and_a_hold_repeats",
        tier: Tier::Dat,
    },
    behaviour! {
        id: "text-entry.filters.the-how-many-box-takes-digits-only",
        says: "The box a player says how many of a stack to move in takes digits and nothing else, \
               so a stray letter cannot be typed into it -- which is what makes the number safe to \
               read, since a leading zero and an x would otherwise be read as a different number \
               entirely and move a different number of things.",
        since: RETAIL,
        evidence: Evidence::Private("AC-EVID-P4-2-STACK"),
        station: "dereth-testkit::dat::shell::scenario_the_how_many_box_takes_digits_only",
        tier: Tier::Dat,
    },
    behaviour! {
        id: "text-entry.filters.the-how-many-of-a-component-box-takes-digits-only-on-every-row",
        says: "The box beside a spell component, where a player says how many to keep, takes \
               digits only -- on every row of the list and not merely on the first, because the \
               rule is put on each row as the list is built.",
        since: RETAIL,
        evidence: Evidence::Private("AC-EVID-P4-2-COMPONENT"),
        station: "dereth-testkit::dat::shell::scenario_the_how_many_of_a_component_box_takes_digits_only_on_every_row",
        tier: Tier::Dat,
    },
    behaviour! {
        id: "text-entry.filters.the-name-box-takes-the-letters-a-name-may-have-and-nothing-else",
        says: "The box a new character is named in takes letters, an apostrophe, a space and a \
               hyphen, and refuses digits and every other punctuation mark -- so a name a player \
               types is one the shard will accept.",
        since: RETAIL,
        evidence: Evidence::Private("AC-EVID-P4-2-NAME"),
        station: "dereth-testkit::dat::shell::scenario_the_name_box_takes_the_letters_a_name_may_have",
        tier: Tier::Dat,
    },
    behaviour! {
        id: "text-entry.filters.three-of-the-clients-boxes-take-only-certain-characters-and-the-rest-take-anything",
        says: "Of all the boxes in the shipped interface a player can type into, three take only \
               certain characters -- a name, a stack size and a component count -- and every other \
               one takes whatever is typed. A fourth thing the client restricts is a caption \
               nobody can type in at all.",
        since: RETAIL,
        evidence: Evidence::Private("AC-EVID-P4-2-CENSUS"),
        station: "dereth-testkit::dat::shell::scenario_three_of_the_clients_boxes_take_only_certain_characters",
        tier: Tier::Dat,
    },
    behaviour! {
        id: "text-entry.focus.a-character-typed-the-moment-a-screen-takes-the-keyboard-is-not-lost",
        says: "When a screen gives the keyboard to a box the player can type into, the very next \
               character they type reaches it rather than being thrown away; typed before that, \
               with nothing able to take it, a character is lost -- which is what makes the first \
               half a reading and not a certainty. The first character replaces the prompt the box \
               was showing.",
        since: RETAIL,
        evidence: Evidence::Private("AC-EVID-O191-NEXT"),
        station: "dereth-testkit::dat::shell::scenario_a_character_typed_the_moment_a_screen_takes_the_keyboard_is_not_lost",
        tier: Tier::Dat,
    },
    behaviour! {
        id: "text-entry.focus.a-screen-that-takes-the-keyboard-in-its-own-pass-keeps-the-next-character",
        says: "The same holds when the screen takes the keyboard in its own per-frame pass rather \
               than in answering a press: the character typed straight afterwards still arrives, \
               and one typed while nothing held the keyboard still does not.",
        since: RETAIL,
        evidence: Evidence::Private("AC-EVID-O191-UPDATE"),
        station: "dereth-testkit::dat::shell::scenario_a_screen_that_takes_the_keyboard_in_its_own_pass_keeps_the_next_character",
        tier: Tier::Dat,
    },
    behaviour! {
        id: "text-entry.focus.opening-the-chat-bar-with-a-key-swallows-that-keys-own-character",
        says: "Opening the chat bar with a key arms the client to swallow the character that key \
               itself would type, so the line does not begin with it; focusing the same box with \
               the mouse arms nothing, because a click types no character to swallow.",
        since: RETAIL,
        evidence: Evidence::Private("AC-EVID-O214-ARM"),
        station: "dereth-testkit::dat::shell::scenario_opening_the_chat_bar_with_a_key_swallows_its_own_character",
        tier: Tier::Dat,
    },
    behaviour! {
        id: "text-entry.focus.the-armed-swallow-eats-exactly-one-character",
        says: "The swallow takes one character and one only: the next keystroke after it is typed \
               into the box normally.",
        since: RETAIL,
        evidence: Evidence::Private("AC-EVID-O214-ONE"),
        station: "dereth-testkit::dat::shell::scenario_the_armed_swallow_eats_exactly_one_character",
        tier: Tier::Dat,
    },
    behaviour! {
        id: "text-entry.focus.the-typing-switch-follows-a-box-that-can-be-typed-into-and-is-thrown-once-per-change",
        says: "The client counts the player as typing only while a box that can be typed into \
               holds the keyboard -- a box that can be picked at but not typed into does not count \
               -- and it decides that once when the keyboard moves, not over and over on every \
               frame.",
        since: RETAIL,
        evidence: Evidence::Private("AC-EVID-O191-EDGES"),
        station: "dereth-testkit::dat::shell::scenario_the_typing_switch_follows_an_editable_box_and_nothing_else",
        tier: Tier::Dat,
    },
    behaviour! {
        id: "text-entry.ime.a-composed-character-reaches-the-box-unchanged",
        says: "A character composed on another keyboard -- an accented letter, a Chinese character \
               -- reaches the box being typed in exactly as it was composed, neither narrowed to a \
               single byte nor dropped, in a box that takes anything.",
        since: RETAIL,
        evidence: Evidence::Private("AC-EVID-P4-2-COMMIT"),
        station: "dereth-testkit::dat::shell::scenario_a_composed_character_reaches_the_box_unchanged",
        tier: Tier::Dat,
    },
    behaviour! {
        id: "text-entry.ime.every-composition-message-is-handed-to-the-desktop-and-an-ordinary-character-is-not",
        says: "The client writes no composition of its own: every message a keyboard's own input \
               method sends is offered on and then left to the desktop to answer, so the method's \
               own window keeps drawing and composition really starts. An ordinary typed character \
               takes the other road and reaches the client's own input.",
        since: RETAIL,
        evidence: Evidence::Private("AC-EVID-P4-2-IME"),
        station: "dereth-testkit::dat::shell::scenario_every_composition_message_is_handed_to_the_desktop",
        tier: Tier::Dat,
    },
    behaviour! {
        id: "text-entry.paste.a-paste-puts-the-clipboard-in-once-and-not-a-letter-with-it",
        says: "Pasting with the keyboard puts the clipboard in once and does not also type the \
               letter that was held down with the modifier. The same key without the modifier, and \
               with the shift key, is still a letter, and characters another keyboard layout \
               produces still reach the box.",
        since: RETAIL,
        evidence: Evidence::Private("AC-EVID-P4-2-PASTE"),
        station: "dereth-testkit::dat::shell::scenario_a_paste_puts_the_clipboard_in_once_and_not_a_letter_with_it",
        tier: Tier::Dat,
    },
    behaviour! {
        id: "text-entry.paste.line-breaks-in-what-was-pasted-never-reach-the-shard",
        says: "Several lines pasted into a one-line box become one line, with the breaks and the \
               tabs gone rather than hidden in it -- and pasting them is not pressing return, so \
               nothing is said until the player says it. What then goes out is that one line.",
        since: RETAIL,
        evidence: Evidence::Private("AC-EVID-P4-2-LINEBREAKS"),
        station: "dereth-testkit::dat::shell::scenario_line_breaks_in_what_was_pasted_never_reach_the_shard",
        tier: Tier::Dat,
    },
    behaviour! {
        id: "viewport.a-press-in-the-middle-of-a-moved-view-finds-what-is-ahead-of-the-eye",
        says: "With the view moved and resized inside the window, a press in the middle of that \
               view finds the thing straight ahead of the eye -- and a press in the middle of the \
               window, which is no longer the middle of the view, does not, which is what stops \
               the first half passing for the wrong reason.",
        since: RETAIL,
        evidence: Evidence::Private("AC-EVID-F20-RAY"),
        station: "dereth-testkit::dat::shell::scenario_a_press_in_the_middle_of_a_moved_view_finds_what_is_ahead_of_the_eye",
        tier: Tier::Dat,
    },
    behaviour! {
        id: "viewport.a-press-outside-the-view-is-refused-though-it-is-inside-the-window",
        says: "A press that lands inside the window but outside the view the player is looking \
               through is refused and counted as such -- above it, left of it, past either far \
               edge, and the window's own last pixel -- while the very same five presses are taken \
               when the view is the whole window.",
        since: RETAIL,
        evidence: Evidence::Private("AC-EVID-F20-REFUSAL"),
        station: "dereth-testkit::cpu::shell::scenario_a_press_outside_the_view_is_refused_though_it_is_inside_the_window",
        tier: Tier::Cpu,
    },
    behaviour! {
        id: "viewport.a-really-moved-view-measures-a-press-against-itself-and-the-default-is-not",
        says: "The view a player looks through, really moved and resized in a running client, \
               measures a press against its own corner and refuses one outside itself -- while the \
               view as it ships, filling the window from its corner, takes every press in the \
               window and answers with the window's own number.",
        since: RETAIL,
        evidence: Evidence::Private("AC-EVID-F20-SBOX"),
        station: "dereth-testkit::dat::shell::scenario_a_really_moved_view_measures_a_press_against_itself",
        tier: Tier::Dat,
    },
    behaviour! {
        id: "viewport.a-world-press-is-measured-against-the-view-the-frame-pushed-in",
        says: "The part of the client that turns a press into a look into the world measures it \
               against the view the frame pushed in, not against the picture it is handed: the same \
               press is armed at the view's own point, a press outside the view arms nothing at \
               all, and with no view pushed in the window is the view and both presses behave as \
               they always did.",
        since: RETAIL,
        evidence: Evidence::Private("AC-EVID-F20-INTERACTION"),
        station: "dereth-testkit::cpu::shell::scenario_a_world_press_is_measured_against_the_view_the_frame_pushed_in",
        tier: Tier::Cpu,
    },
    behaviour! {
        id: "viewport.the-armed-point-is-measured-from-the-views-own-corner",
        says: "The point the client arms a look at is the press measured from the view's own \
               corner and not from the window's; with the view at the window's corner the two are \
               the same number, which is what everything used to be able to assume.",
        since: RETAIL,
        evidence: Evidence::Private("AC-EVID-F20-CURSOR"),
        station: "dereth-testkit::cpu::shell::scenario_the_armed_point_is_measured_from_the_views_own_corner",
        tier: Tier::Cpu,
    },
    behaviour! {
        id: "window.focus.a-window-remembers-the-box-that-held-the-keyboard-and-gives-it-back",
        says: "A window remembers which of its boxes the player was typing in. Putting it aside \
               takes the caret away and keeps the memory; working in it again puts the caret back \
               where it was, including when the window lost out to another one. A window already \
               being worked in is not disturbed, and pressing something inside a window works in \
               the window rather than in the thing pressed.",
        since: RETAIL,
        evidence: Evidence::Private("AC-EVID-O245-WINDOW-FOCUS"),
        station: "dereth-testkit::dat::shell::scenario_a_window_remembers_the_box_that_held_the_keyboard",
        tier: Tier::Dat,
    },
    behaviour! {
        id: "window.full-screen.a-window-too-big-for-the-desktop-is-pulled-back-onto-it",
        says: "A window wider or taller than the part of the desktop the player can actually use \
               is moved onto it rather than left hanging off an edge, and one too wide for it is \
               put flush against the left edge with only its frame overhanging. A desktop that \
               will not say where its usable part is leaves the window centred.",
        since: RETAIL,
        evidence: Evidence::Private("AC-EVID-P1-18-CLAMP"),
        station: "dereth-testkit::cpu::shell::scenario_a_window_too_big_for_the_desktop_is_pulled_back_onto_it",
        tier: Tier::Cpu,
    },
    behaviour! {
        id: "window.full-screen.it-fills-the-monitor-the-window-is-on-and-not-the-resolution-that-was-asked-for",
        says: "Full screen fills the monitor the window is on, edge to edge, whatever resolution \
               the player's display setting asked for -- this client borrows no display mode, so \
               there is nothing to switch to. On a second monitor it is that monitor's own \
               rectangle, and the task bar is covered rather than avoided.",
        since: THIS_CLIENT,
        divergence: "CD-002",
        evidence: Evidence::Private("AC-EVID-P1-18-FULL"),
        station: "dereth-testkit::cpu::shell::scenario_full_screen_fills_the_monitor_the_window_is_on",
        tier: Tier::Cpu,
    },
    behaviour! {
        id: "window.full-screen.it-never-floats-over-the-players-other-windows",
        says: "A full-screen client is an ordinary window in the stacking order: it never asks to \
               be kept in front of everything else, on any path, so alt-tabbing away really does \
               leave it behind. This is a deliberate divergence from the original, which had to \
               float because its full screen borrowed the display mode.",
        since: THIS_CLIENT,
        divergence: "CD-003",
        evidence: Evidence::Private("AC-EVID-P1-66"),
        station: "dereth-testkit::cpu::shell::scenario_full_screen_never_asks_to_float_over_everything_else",
        tier: Tier::Cpu,
    },
    behaviour! {
        id: "window.full-screen.the-options-page-turns-it-on-and-off-while-the-player-plays",
        says: "Ticking the full-screen box on the options page reaches the window on the very \
               next frame, and unticking it puts the window back -- and either way the setting is \
               kept for the next time the client starts.",
        since: RETAIL,
        evidence: Evidence::Private("AC-EVID-P1-18-OPTION"),
        station: "dereth-testkit::dat::shell::scenario_the_options_page_turns_full_screen_on_and_off_mid_session",
        tier: Tier::Dat,
    },
    behaviour! {
        id: "window.full-screen.the-setting-is-kept-from-the-start-and-applied-on-entering-the-world",
        says: "A client whose saved setting asks for full screen still starts windowed: the \
               patch screen and the character list are a window, and the setting is applied on \
               entering the world and taken away again on leaving it. The setting itself survives \
               the round trip.",
        since: THIS_CLIENT,
        divergence: "CD-005",
        evidence: Evidence::Private("AC-EVID-P1-18-GAMEPLAY"),
        station: "dereth-testkit::dat::shell::scenario_the_full_screen_setting_is_kept_and_applied_on_entering_the_world",
        tier: Tier::Dat,
    },
    behaviour! {
        id: "window.full-screen.the-switch-key-is-refused-outside-the-world-and-works-inside-it",
        says: "The keyboard shortcut that switches between full screen and a window does nothing \
               at the character list, and switches both ways once the player is in the world. It \
               is refused rather than honoured and quietly undone, which is what happened before \
               the gate was shut.",
        since: THIS_CLIENT,
        divergence: "CD-005",
        evidence: Evidence::Private("AC-EVID-P1-18-ALTENTER"),
        station: "dereth-testkit::dat::shell::scenario_the_full_screen_switch_key_is_refused_outside_the_world",
        tier: Tier::Dat,
    },
    behaviour! {
        id: "window.full-screen.windowed-has-a-frame-and-full-screen-has-none",
        says: "A windowed client has a caption bar, a system menu and a minimise button, and can \
               never be resized by dragging its edge; a full-screen one has no frame at all. \
               Neither is shown on screen until the client asks for it to be.",
        since: RETAIL,
        evidence: Evidence::Private("AC-EVID-P1-18-STYLE"),
        station: "dereth-testkit::cpu::shell::scenario_a_window_has_its_frame_and_a_full_screen_one_has_none",
        tier: Tier::Cpu,
    },
    behaviour! {
        id: "window.full-screen.windowed-is-the-picture-the-player-asked-for-plus-its-frame-centred",
        says: "A windowed client is exactly the picture size the player's display setting asked \
               for, plus the desktop's own frame and caption around it, centred on the monitor.",
        since: RETAIL,
        evidence: Evidence::Private("AC-EVID-P1-18-RECT"),
        station: "dereth-testkit::cpu::shell::scenario_a_window_is_the_asked_for_picture_plus_its_frame_centred",
        tier: Tier::Cpu,
    },
    behaviour! {
        id: "window.layout.a-change-made-before-the-character-is-described-survives-it",
        says: "A window moved before the shard has described the character is not quietly taken \
               as the shard's own answer; once the description arrives, a move made after it wins \
               over what it carried, a move made just before the screen is rebuilt survives the \
               rebuild, and a later description from the shard does overwrite a move, because the \
               shard's answer is authoritative and a move is not.",
        since: RETAIL,
        evidence: Evidence::Private("AC-EVID-ASTRA-TOOLBAR-ORDER"),
        station: "dereth-testkit::dat::shell::scenario_a_change_before_the_description_survives_it",
        tier: Tier::Dat,
    },
    behaviour! {
        id: "window.layout.a-saved-layout-is-pulled-onto-the-screen-and-moves-only-the-window-it-names",
        says: "A saved layout that would put a window off the screen pulls it back onto it, \
               remembers where it really ended up, and moves no window it does not name; making \
               the display smaller pulls it back again and that is remembered too.",
        since: RETAIL,
        evidence: Evidence::Private("AC-EVID-ASTRA-TOOLBAR-LAYOUT"),
        station: "dereth-testkit::dat::shell::scenario_a_saved_layout_is_clamped_and_moves_only_what_it_names",
        tier: Tier::Dat,
    },
    behaviour! {
        id: "window.layout.moving-or-hiding-a-window-is-remembered-and-comes-back-when-the-screen-is-rebuilt",
        says: "Moving, resizing or hiding a window is remembered in the character's own settings \
               -- at the size it really became rather than the size asked for -- and the window \
               comes back that way when the screen is rebuilt. Nothing is sent at the time, and \
               everything else in the settings, including what this client does not understand, \
               comes through untouched.",
        since: RETAIL,
        evidence: Evidence::Private("AC-EVID-ASTRA-TOOLBAR-KEEP"),
        station: "dereth-testkit::dat::shell::scenario_moving_a_window_is_remembered_across_a_rebuild",
        tier: Tier::Dat,
    },
    behaviour! {
        id: "window.system-keys.the-desktop-keeps-the-two-it-must-and-the-client-eats-the-third",
        says: "The key combination that switches applications never reaches the desktop's own \
               handling and so cannot move the player out of the client by accident, while the \
               two that toggle full screen and close the window are deliberately let through to \
               it.",
        since: RETAIL,
        evidence: Evidence::Private("AC-EVID-P1-145-WINDOW"),
        station: "dereth-testkit::dat::shell::scenario_the_window_keeps_the_switcher_and_passes_the_other_two",
        tier: Tier::Dat,
    },
];
