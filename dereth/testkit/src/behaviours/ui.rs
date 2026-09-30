//! The shell -- the wizard, the notice bubble, the House pane and the host gestures.
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
        id: "character-page.raise.both-buttons-on-both-pages-put-the-request-on-the-wire",
        says: "Both of the raise buttons -- one at a time and ten at a time -- on both the skills \
               page and the attributes page answer a press where a player presses them, and each \
               puts exactly one request on the wire, naming the row that was picked and asking for \
               its own amount. The ten-at-a-time button asks for more than the one-at-a-time one, \
               so the two are not the same request.",
        since: RETAIL,
        evidence: Evidence::Private("AC-EVID-F34-RAISE"),
        station: "dereth-testkit::dat::ui::scenario_both_raise_buttons_on_both_pages_put_the_request_on_the_wire",
        tier: Tier::Dat,
    },
    behaviour! {
        id: "chargen.every-wizard-page-lays-out",
        says: "The character-creation wizard comes up and every one of its pages can be reached \
               and laid out, with its scrolling panes and their scrollbars present.",
        since: RETAIL,
        evidence: Evidence::Private("AC-EVID-O118"),
        station: "dereth-testkit::dat::ui::scenario_every_wizard_page_lays_out",
        tier: Tier::Dat,
    },
    behaviour! {
        id: "clipboard.copy.hands-the-selection-to-the-host-once",
        says: "Copying a selection out of the chat window hands exactly that text to the desktop \
               clipboard, once and not once a frame. A copy with nothing selected leaves whatever the \
               player copied elsewhere untouched, text copied in another application becomes available \
               to paste, the desktop is only read when its contents actually changed, and a format the \
               client cannot read does not wipe what it already had.",
        since: RETAIL,
        evidence: Evidence::Private("AC-EVID-O917"),
        station: "dereth-testkit::cpu::ui::scenario_a_copy_reaches_the_host_clipboard_once",
        tier: Tier::Cpu,
    },
    behaviour! {
        id: "dates.a-pane-draws-in-whatever-zone-it-is-handed-row-by-row",
        says: "A pane draws each of its dates in whatever zone it was handed for that row, so a \
               player whose machine is on the other side of the world reads their own time -- and \
               the three rows of the house pane can be in three different zones at once, because \
               each of them is worked out on its own.",
        since: RETAIL,
        evidence: Evidence::Private("AC-EVID-P1-76-SEAM"),
        station: "dereth-testkit::cpu::ui::scenario_a_pane_draws_in_whatever_zone_it_is_handed_row_by_row",
        tier: Tier::Cpu,
    },
    behaviour! {
        id: "dates.every-date-on-the-house-pane-is-in-the-machines-own-time-and-that-shape",
        says: "The three dates on the house pane -- when it was bought, when the period ends and \
               when the next payment is due -- are each drawn in the time the machine itself \
               keeps, in the shape the shipped runtime writes, with no weekday name on any of \
               them.",
        since: RETAIL,
        evidence: Evidence::Private("AC-EVID-P1-76-HOUSE"),
        station: "dereth-testkit::dat::ui::scenario_every_date_on_the_house_pane_is_in_the_machines_own_time",
        tier: Tier::Dat,
    },
    behaviour! {
        id: "dates.the-ban-expiry-keeps-the-other-shape-and-is-in-the-machines-own-time",
        says: "The one date in the whole client that is drawn in the weekday-and-month-name shape \
               keeps it -- and is still drawn in the time the machine keeps rather than in the \
               time the shard talks in.",
        since: RETAIL,
        evidence: Evidence::Private("AC-EVID-P1-76-BAN"),
        station: "dereth-testkit::dat::ui::scenario_the_ban_expiry_keeps_its_shape_and_is_in_the_machines_own_time",
        tier: Tier::Dat,
    },
    behaviour! {
        id: "dates.the-character-sheets-born-line-is-in-the-machines-own-time-and-that-shape",
        says: "The line on the character sheet saying when the character was born is drawn in the \
               time the machine keeps and in the shape the shipped runtime writes, on the element \
               itself and not only in what the sheet remembers.",
        since: RETAIL,
        evidence: Evidence::Private("AC-EVID-P1-76-BORN"),
        station: "dereth-testkit::dat::ui::scenario_the_character_sheets_born_line_is_in_the_machines_own_time",
        tier: Tier::Dat,
    },
    behaviour! {
        id: "dates.the-chat-stamp-carries-the-machines-own-offset-and-not-a-constant",
        says: "The clock the chat lines are stamped with is handed the offset the machine really \
               has for that instant rather than a fixed one, and the stamp itself has no leading \
               zero on the hour and a space after it.",
        since: RETAIL,
        evidence: Evidence::Private("AC-EVID-P1-76-STAMP"),
        station: "dereth-testkit::dat::ui::scenario_the_chat_stamp_carries_the_machines_own_offset",
        tier: Tier::Dat,
    },
    behaviour! {
        id: "dates.the-line-saying-when-another-house-may-be-bought-is-in-the-machines-own-time",
        says: "The line telling the player when they may buy another house is a date of its own, \
               worked out from when this one was bought, and it is drawn in the machine's own time \
               and in the shipped runtime's shape like the rest of them.",
        since: RETAIL,
        evidence: Evidence::Private("AC-EVID-P1-76-BUY-AGAIN"),
        station: "dereth-testkit::dat::ui::scenario_the_line_saying_when_another_house_may_be_bought_is_in_the_machines_own_time",
        tier: Tier::Dat,
    },
    behaviour! {
        id: "dates.the-shape-is-the-shipped-runtimes-own-and-the-zone-is-something-handed-in",
        says: "Every date the client draws has the shape the runtime it ships with writes -- a \
               plain month, day and year with a twelve-hour clock and a morning or afternoon mark, \
               never the weekday-and-month-name shape -- and the zone is a number handed in rather \
               than a label, so the same instant moves across midnight in both directions as the \
               number changes.",
        since: RETAIL,
        evidence: Evidence::Private("AC-EVID-P1-76-FORMAT"),
        station: "dereth-testkit::cpu::ui::scenario_the_shape_is_the_shipped_runtimes_own_and_the_zone_is_handed_in",
        tier: Tier::Cpu,
    },
    behaviour! {
        id: "house.tab.a-character-with-no-house-is-told-so",
        says: "A character who owns no house is told so in words when the House tab is opened: the \
               pane comes up with one row in it saying they do not currently own a house, rather \
               than blank.",
        since: RETAIL,
        evidence: Evidence::Private("AC-EVID-G9-HOUSE-TAB"),
        station: "dereth-testkit::dat::ui::scenario_the_house_tab_tells_a_houseless_character_they_have_no_house",
        tier: Tier::Dat,
    },
    behaviour! {
        id: "house.tab.the-line-is-written-once-and-into-that-pane-alone",
        says: "That line is written once however long the tab is left up, and into the House pane \
               and no other: the map beside it never carries it.",
        since: RETAIL,
        evidence: Evidence::Private("AC-EVID-G9-HOUSE-ONCE"),
        station: "dereth-testkit::dat::ui::scenario_the_houseless_line_is_written_once_and_into_that_pane_alone",
        tier: Tier::Dat,
    },
    behaviour! {
        id: "hud.lamp-row.a-buff-lights-one-lamp-a-debuff-the-other-and-a-purge-puts-both-out",
        says: "A helpful enchantment lights the lamp for helpful ones and leaves the lamp for \
               harmful ones dark, a harmful one then lights that one too, and sweeping the \
               enchantments away puts both out -- a pair driven from one shared count would pass a \
               something-is-lit test and be wrong.",
        since: RETAIL,
        evidence: Evidence::Private("AC-EVID-O279-EFFECTS"),
        station: "dereth-testkit::dat::ui::scenario_a_buff_lights_one_lamp_and_a_debuff_the_other",
        tier: Tier::Dat,
    },
    behaviour! {
        id: "hud.lamp-row.a-button-of-the-strip-with-no-action-of-its-own-opens-nothing",
        says: "The one button of the lamp strip that carries no action of its own opens no panel \
               at all and fires no action -- which is what stops every press opening everything.",
        since: RETAIL,
        evidence: Evidence::Private("AC-EVID-O279-NO-ACTION"),
        station: "dereth-testkit::dat::ui::scenario_a_button_of_the_strip_with_no_action_of_its_own_opens_nothing",
        tier: Tier::Dat,
    },
    behaviour! {
        id: "hud.lamp-row.a-dark-lamp-swallows-its-own-click",
        says: "A lamp in the strip that has nothing to show sits dark and a click on it opens \
               nothing. At rest the link-status and burden lamps are lit and open their panels, \
               the buff, debuff, vitae and mini-game lamps are dark, and once a dark lamp lights, \
               a click in the same place opens its panel.",
        since: RETAIL,
        evidence: Evidence::Private("AC-EVID-G2-LAMP-ROW"),
        station: "dereth-ui-screens::dat::panels::lamp_strip_hit_test::a_lamp_with_nothing_to_show_is_a_disabled_button_and_swallows_its_own_click",
        tier: Tier::Dat,
    },
    behaviour! {
        id: "hud.lamp-row.every-lamp-opens-its-own-panel-and-leaves-the-other-lamps-panels-down",
        says: "Each lamp of the strip carries the action the shipped layout gives it, and pressing \
               a lit lamp opens the one panel registered for that action, brings the panel stack up \
               with it, and leaves every other lamp's panel down.",
        since: RETAIL,
        evidence: Evidence::Private("AC-EVID-O279-ACTIONS"),
        station: "dereth-testkit::dat::ui::scenario_every_lamp_opens_its_own_panel",
        tier: Tier::Dat,
    },
    behaviour! {
        id: "hud.lamp-row.the-burden-lamp-crosses-both-thresholds-on-the-characters-own-capacity",
        says: "The burden lamp is read against what the character can actually carry: under it, at \
               exactly all of it -- which counts as over, not under -- and at twice it, three \
               different lamps, and it goes out again when the load is dropped.",
        since: RETAIL,
        evidence: Evidence::Private("AC-EVID-O279-BURDEN"),
        station: "dereth-testkit::dat::ui::scenario_the_burden_lamp_crosses_both_thresholds_on_the_characters_own_capacity",
        tier: Tier::Dat,
    },
    behaviour! {
        id: "hud.lamp-row.the-vitae-lamp-lights-for-a-penalty-and-not-for-a-multiplier-of-one",
        says: "The lamp for a death's lingering penalty lights when there is one and goes out when \
               the penalty is lifted -- a penalty of none at all is present and still not a \
               penalty, which is the case a lit-only test cannot see.",
        since: RETAIL,
        evidence: Evidence::Private("AC-EVID-O279-VITAE"),
        station: "dereth-testkit::dat::ui::scenario_the_vitae_lamp_lights_for_a_penalty_and_not_for_a_multiplier_of_one",
        tier: Tier::Dat,
    },
    behaviour! {
        id: "hud.lamp-row.the-way-out-of-the-strip-raises-the-question-about-ending-the-session",
        says: "The button at the end of the lamp strip is the way out, and pressing it raises the \
               question about ending the session -- the asking form of it, the one whose yes goes \
               back to the character list rather than closing the client.",
        since: RETAIL,
        evidence: Evidence::Private("AC-EVID-O279-EXIT"),
        station: "dereth-testkit::dat::ui::scenario_the_way_out_of_the_strip_raises_the_question_about_ending_the_session",
        tier: Tier::Dat,
    },
    behaviour! {
        id: "hud.link-lamp.a-real-failure-puts-the-player-off-the-world-rather-than-reddening-a-lamp",
        says: "When the link really fails the player is taken off the world and shown the \
               refusal, so there is no lamp left on a world they are no longer in. The client \
               stops counting itself connected at the same moment.",
        since: RETAIL,
        evidence: Evidence::Private("AC-EVID-G28-FAILURE"),
        station: "dereth-testkit::dat::ui::scenario_a_real_failure_takes_the_player_off_the_world",
        tier: Tier::Dat,
    },
    behaviour! {
        id: "hud.link-lamp.it-comes-up-good-and-falls-to-lost-when-nothing-is-heard-at-all",
        says: "The link lamp comes up saying the link is good, because the shipped layout gives it \
               nothing to draw at rest; with no link at all every reading falls through and it \
               ends up saying the link is lost.",
        since: RETAIL,
        evidence: Evidence::Private("AC-EVID-O279-LINK"),
        station: "dereth-testkit::dat::ui::scenario_the_link_lamp_comes_up_good_and_falls_to_lost",
        tier: Tier::Dat,
    },
    behaviour! {
        id: "hud.link-lamp.the-shard-setting-the-clock-does-not-change-it-and-nor-does-a-quiet-link",
        says: "The link lamp reads how long it has been since the shard was heard from, on one \
               clock. The shard setting the game clock, which every shard does moments after a \
               login, does not move it; nor does the best part of a minute with nothing arriving, \
               so long as the link itself is still up.",
        since: RETAIL,
        evidence: Evidence::Private("AC-EVID-G28"),
        station: "dereth-testkit::dat::ui::scenario_setting_the_clock_does_not_change_the_lamp",
        tier: Tier::Dat,
    },
    behaviour! {
        id: "hud.portal-storm.the-four-notices-set-the-storm-level-and-only-the-warnings-play-the-storm",
        says: "A brewing or imminent portal storm puts its notice on the speech strip, sets the \
               storm level to the storm's own extent and plays the storm effect on the player, \
               weakly for brewing and strongly for imminent; a strike prints its line in the chat \
               log and a calm puts the level back to nothing, and neither plays the effect.",
        since: RETAIL,
        evidence: Evidence::Private("AC-EVID-P24-PORTAL-STORM"),
        station: "dereth-client::gpu::net::late_receivers::a_portal_storm_lights_the_lamp_and_the_four_notices_differ_as_retail_does",
        tier: Tier::Gpu,
    },
    behaviour! {
        id: "hud.power-bar.a-jump-shows-the-one-bar-the-client-listens-with-and-never-the-other",
        says: "The shipped screen carries two power bars -- a wide one along the bottom and a \
               movable one -- and only one of them asks to be told about a jump. A jump shows that \
               one and draws it, and the other stays hidden and contributes nothing to the \
               picture; letting go puts the one that was up away again.",
        since: RETAIL,
        evidence: Evidence::Private("AC-EVID-F34-POWERBAR"),
        station: "dereth-testkit::dat::ui::scenario_a_jump_shows_one_power_bar_and_never_the_other",
        tier: Tier::Dat,
    },
    behaviour! {
        id: "hud.vitals.a-press-on-the-bar-flips-it-between-its-two-presentations",
        says: "Pressing the vitals bar really answers: it flips between the two presentations the \
               shipped layout draws for it, and pressing again flips it back, so it is a toggle \
               and not a latch. The whole bar swaps, meters and all, and not only its frame.",
        since: RETAIL,
        evidence: Evidence::Private("AC-EVID-AF26"),
        station: "dereth-testkit::dat::ui::scenario_a_press_on_the_vitals_bar_flips_it_between_its_two_presentations",
        tier: Tier::Dat,
    },
    behaviour! {
        id: "hud.vitals.both-vitals-windows-fill-their-three-bars-and-three-labels",
        says: "Both the stacked and the side-by-side vitals windows fill all six of their pieces: \
               the health, stamina and mana bars each fill to current over maximum and their \
               labels read current and maximum, such as 30/120.",
        since: RETAIL,
        evidence: Evidence::Private("AC-EVID-P1180-VITALS"),
        station: "dereth-ui-screens::dat::panels::vitals_and_toolbar_selection_bindings::this_build_binds_the_six_vitals_children_through_the_catalogue_and_writes_them",
        tier: Tier::Dat,
    },
    behaviour! {
        id: "hud.vitals.the-first-press-changes-nothing-on-screen-and-the-second-hides-the-numbers",
        says: "The vitals bar comes up showing its numbers, because the shipped layout names no \
               look for it to start in. The first press moves it into the look that also shows the \
               numbers, so nothing on the screen changes; the second reaches the one that hides \
               them. Two presses to lose the numbers is what the original does.",
        since: RETAIL,
        evidence: Evidence::Private("AC-EVID-F34-VITALS"),
        station: "dereth-testkit::dat::ui::scenario_the_first_press_on_the_vitals_bar_changes_nothing_on_screen",
        tier: Tier::Dat,
    },
    behaviour! {
        id: "hud.vitals.the-vitals-show-what-the-server-sent",
        says: "The stacked vitals show the health, stamina and mana the shard's player description \
               gave: each bar is filled to current over maximum and labelled current/maximum with \
               no spaces.",
        since: RETAIL,
        evidence: Evidence::Private("AC-EVID-HUD-VITALS"),
        station: "dereth-client::gpu::panels::gameplay_hud::the_vitals_show_what_the_recorded_server_sent",
        tier: Tier::Gpu,
    },
    behaviour! {
        id: "hud.windows.the-hud-comes-up-with-the-six-retail-windows",
        says: "Entering the world brings up six windows -- the view of the world, the indicator \
               strip, the stacked vitals, the radar, the main chat window and the toolbar -- and \
               nothing else: the floating chat windows, the examine window, the side-by-side \
               vitals, the panels, the power bar and the combat window all start down.",
        since: RETAIL,
        evidence: Evidence::Private("AC-EVID-HUD-WINDOWS"),
        station: "dereth-client::gpu::panels::gameplay_hud::the_hud_comes_up_with_the_six_windows_the_retail_client_shows_and_no_others",
        tier: Tier::Gpu,
    },
    behaviour! {
        id: "notice.a-line-still-waiting-when-the-character-logs-off-goes-with-the-windows",
        says: "A line the client composed and had not yet shown goes down with the windows when \
               the character logs off, out of the queue it was waiting in and out of the strip \
               that was about to draw it -- nothing is held over to be shown to whoever logs in \
               next.",
        since: RETAIL,
        evidence: Evidence::Private("AC-EVID-O170-LOGOFF"),
        station: "dereth-testkit::cpu::ui::scenario_a_line_still_waiting_when_the_character_logs_off_goes_with_the_windows",
        tier: Tier::Cpu,
    },
    behaviour! {
        id: "notice.a-swing-with-nothing-selected-says-so-on-the-strip",
        says: "Swinging at nothing while in a fighting stance is refused in words on the strip -- \
               the refusal is a second way into the same queue, one that goes round the notice \
               machinery altogether, and it reaches the same surface.",
        since: RETAIL,
        evidence: Evidence::Private("AC-EVID-O170-COMBAT"),
        station: "dereth-testkit::dat::ui::scenario_a_swing_with_nothing_selected_says_so_on_the_strip",
        tier: Tier::Dat,
    },
    behaviour! {
        id: "notice.autorun.a-key-that-does-not-change-the-run-lock-says-nothing",
        says: "A key that leaves the run lock where it already was says nothing at all, while the \
               same key when it does cancel the lock tells the player it is gone.",
        since: RETAIL,
        evidence: Evidence::Private("AC-EVID-O192-SILENT"),
        station: "dereth-testkit::dat::ui::scenario_a_key_that_changes_nothing_says_nothing",
        tier: Tier::Dat,
    },
    behaviour! {
        id: "notice.autorun.the-line-reaches-the-strip-and-the-chat-windows-drop-it",
        says: "The line the client writes about the run lock is shown in the notice strip, as a \
               bubble with those words in it, and every chat window filters it out -- unless the \
               player has asked for that kind of line, in which case it is kept.",
        since: RETAIL,
        evidence: Evidence::Private("AC-EVID-O192-ROUTE"),
        station: "dereth-testkit::dat::ui::scenario_the_autorun_line_reaches_the_strip_and_the_chat_windows_drop_it",
        tier: Tier::Dat,
    },
    behaviour! {
        id: "notice.autorun.turning-the-run-lock-on-or-off-says-so-in-the-message-window",
        says: "Locking the character into a run puts a line in the message window saying so, and \
               a second press puts the opposite line there -- one line per change and no more.",
        since: RETAIL,
        evidence: Evidence::Private("AC-EVID-O192-LINE"),
        station: "dereth-testkit::dat::ui::scenario_the_run_lock_says_so_in_the_message_window",
        tier: Tier::Dat,
    },
    behaviour! {
        id: "notice.bubble.clears-itself-after-five-seconds",
        says: "A notice bubble across the top of the viewport disappears on its own five seconds \
               after it appears, with no further input from the player.",
        since: RETAIL,
        evidence: Evidence::Private("AC-EVID-F26-SPEW"),
        station: "dereth-testkit::dat::ui::scenario_the_notice_bubble_clears_itself",
        tier: Tier::Dat,
    },
    behaviour! {
        id: "notice.failure.a-recall-broken-by-moving-says-so-on-the-strip",
        says: "A recall the player breaks by moving is answered with the words the client has for \
               it, once, in the notice strip -- not in silence, and not in the chat scrollback.",
        since: RETAIL,
        evidence: Evidence::Private("AC-EVID-P1-41-RECALL"),
        station: "dereth-testkit::dat::ui::scenario_a_recall_broken_by_moving_says_so_on_the_strip",
        tier: Tier::Dat,
    },
    behaviour! {
        id: "notice.failure.a-refusal-carrying-the-shards-own-word-puts-it-in-the-line",
        says: "A refusal that comes with a word of the shard's own -- a name, a thing -- puts that \
               word into the line the player reads, in every one of the hundred-odd refusals whose \
               wording has a place for it.",
        since: RETAIL,
        evidence: Evidence::Private("AC-EVID-P1-41-STRING"),
        station: "dereth-testkit::dat::ui::scenario_a_refusal_carrying_the_shards_word_puts_it_in_the_line",
        tier: Tier::Dat,
    },
    behaviour! {
        id: "notice.failure.a-refused-portal-says-so-in-the-chat-log-in-its-own-colour",
        says: "A portal that refuses the player because they have not finished what it asks says \
               so in the chat log, in the words the original prints and in the light blue it \
               prints them in rather than in the plain colour. The other way that refusal can \
               arrive really produces a line too, which every chat window then filters out.",
        since: RETAIL,
        evidence: Evidence::Private("AC-EVID-AF19"),
        station: "dereth-testkit::dat::ui::scenario_a_refused_portal_says_so_in_the_chat_log",
        tier: Tier::Dat,
    },
    behaviour! {
        id: "notice.failure.every-refusal-the-shard-can-send-draws-its-own-line-on-its-own-surface",
        says: "Every refusal the shard can send draws the words that refusal has and no others, \
               and lands on the surface its own kind names: the notice strip for the client's own \
               errors and a chat window for the rest. Three of them are silent on purpose, and a \
               code with no wording at all still draws nothing rather than something wrong.",
        since: RETAIL,
        evidence: Evidence::Private("AC-EVID-P1-41-TABLE"),
        station: "dereth-testkit::dat::ui::scenario_every_refusal_draws_its_own_line_on_its_own_surface",
        tier: Tier::Dat,
    },
    behaviour! {
        id: "notice.locked-container.a-use-of-a-locked-one-says-so-in-the-strip",
        says: "Using a container the player cannot open puts the client's own refusal in the \
               notice strip, naming the thing, above the line saying it is being used -- and that \
               refusal never reaches the chat scrollback. The shard's answer to the use adds \
               nothing to either.",
        since: RETAIL,
        evidence: Evidence::Private("AC-EVID-P1-23C"),
        station: "dereth-testkit::dat::ui::scenario_a_use_of_the_recorded_locked_chest_says_so_in_the_strip",
        tier: Tier::Dat,
    },
    behaviour! {
        id: "notice.locked-container.the-shards-own-refusal-reaches-the-strip-and-not-the-scrollback",
        says: "The refusal the shard itself sends for a locked container is shown to the player \
               in the notice strip, exactly once and word for word, and it never reaches the chat \
               scrollback.",
        since: RETAIL,
        evidence: Evidence::Private("AC-EVID-P1-23B"),
        station: "dereth-testkit::dat::ui::scenario_the_shards_refusal_reaches_the_strip_and_not_the_scrollback",
        tier: Tier::Dat,
    },
    behaviour! {
        id: "notice.refusal.a-refusal-about-something-the-player-asked-for-names-it-and-says-why",
        says: "A refusal about something the player asked for names the thing and gives the \
               reason, in the sentence that kind of request has -- which is why a refusal nobody \
               asked for has no words in it at all: there is no request to name a sentence, and \
               most reasons the shard sends have no word of their own either.",
        since: RETAIL,
        evidence: Evidence::Private("AC-EVID-O170-NAMED"),
        station: "dereth-testkit::dat::ui::scenario_a_refusal_about_something_the_player_asked_for_names_it_and_says_why",
        tier: Tier::Dat,
    },
    behaviour! {
        id: "notice.refusal.every-recorded-refusal-and-nothing-else-reaches-the-strip",
        says: "Over every recording there is, what reaches the strip across the top is exactly the \
               refusals the client composed for itself, plus the lines the shard sends on that \
               same channel, and nothing else. A refusal naming no object at all draws nothing, \
               which is the client refusing to talk about something that is not there rather than \
               a line going astray.",
        since: RETAIL,
        evidence: Evidence::Private("AC-EVID-O170-CORPUS"),
        station: "dereth-testkit::dat::ui::scenario_every_recorded_refusal_and_nothing_else_reaches_the_strip",
        tier: Tier::Dat,
    },
    behaviour! {
        id: "notice.refusal.is-a-bubble-and-not-a-chat-line",
        says: "A refusal the client itself composes is shown to the player as a bubble in the \
               notice strip and does not appear in the chat scrollback.",
        since: RETAIL,
        evidence: Evidence::Private("AC-EVID-O170"),
        station: "dereth-testkit::dat::ui::scenario_a_refusal_is_a_bubble_and_not_a_chat_line",
        tier: Tier::Dat,
    },
    behaviour! {
        id: "notice.the-strip-takes-the-clients-own-channel-and-the-chat-windows-take-the-rest",
        says: "The strip across the top takes the one channel the client talks to itself on and \
               refuses everything else, while the chat windows do the opposite: every one of the \
               five shipped windows drops that channel and takes the broadcast beside it -- and a \
               player who asks for that kind of line gets it in the window too.",
        since: RETAIL,
        evidence: Evidence::Private("AC-EVID-O170-ROUTE"),
        station: "dereth-testkit::cpu::ui::scenario_the_strip_takes_the_clients_own_channel_and_the_windows_take_the_rest",
        tier: Tier::Cpu,
    },
    behaviour! {
        id: "options.support.a-browser-that-will-not-open-says-so-in-a-box-with-the-address-in-it",
        says: "When the desktop will not open a browser the client says so in a box of its own, \
               with the reason and the address written out so the player can go there by hand. \
               The line between a refusal and a success is where it always was, read on both \
               sides of it.",
        since: RETAIL,
        evidence: Evidence::Private("AC-EVID-P1-173-SHELL-ERROR"),
        station: "dereth-testkit::dat::ui::scenario_a_browser_that_will_not_open_says_so_in_a_box_with_the_address_in_it",
        tier: Tier::Dat,
    },
    behaviour! {
        id: "options.support.pressing-either-support-button-opens-the-page-in-the-players-browser",
        says: "Either of the two buttons that offer to raise a support ticket really reaches the \
               desktop and asks it to open the support page, once, at the address the client \
               ships -- and the part of the client that answers that ask claims it and hands every \
               other ask back untouched.",
        since: RETAIL,
        evidence: Evidence::Private("AC-EVID-P1-173-URL"),
        station: "dereth-testkit::dat::ui::scenario_pressing_either_support_button_opens_the_page_in_the_players_browser",
        tier: Tier::Dat,
    },
    behaviour! {
        id: "panels.redraw.a-skill-row-follows-the-enchanted-number-and-the-death-penalty-each-on-their-own",
        says: "A skill row draws the number the player actually has after everything acting on \
               it, and follows a spell landing or expiring and a death penalty arriving or \
               lifting, each on its own with the skill list unchanged; the colour of the number \
               follows the same three values, so a row that got the number right and the colour \
               wrong is still wrong.",
        since: RETAIL,
        evidence: Evidence::Private("AC-EVID-O487-SKILLS"),
        station: "dereth-testkit::dat::inventory::scenario_a_skill_row_follows_the_enchanted_number_and_the_penalty",
        tier: Tier::Dat,
    },
    behaviour! {
        id: "panels.redraw.the-attribute-rows-compare-the-very-text-they-write",
        says: "The attribute panel decides whether to redraw by comparing the very words it is \
               about to write, so it cannot be looking at less than it draws; a buffed attribute \
               is a different word and is redrawn, an unchanged frame is not, and an attribute \
               the shard has said nothing about reads as unknown rather than as blank.",
        since: RETAIL,
        evidence: Evidence::Private("AC-EVID-O487-ATTRIBUTES"),
        station: "dereth-testkit::dat::inventory::scenario_the_attribute_rows_compare_the_text_they_write",
        tier: Tier::Dat,
    },
    behaviour! {
        id: "reader.book.a-recorded-book-opens-the-reader-and-shows-its-first-page",
        says: "A scroll, a letter or a sign the shard opens for the player really opens the \
               reader: the window comes up, the list of pages is as long as the book is, and the \
               first page's own words are on the screen. An unsigned book shows no author, which \
               is the client's own answer rather than a page it forgot to write.",
        since: RETAIL,
        evidence: Evidence::Private("AC-EVID-AF25"),
        station: "dereth-testkit::dat::ui::scenario_a_recorded_book_opens_the_reader_and_shows_its_first_page",
        tier: Tier::Dat,
    },
    behaviour! {
        id: "reader.book.paging-moves-the-page-and-greys-the-control-that-cannot-be-pressed",
        says: "Turning to the next page shows that page's own words, and the control for the way \
               one cannot go is greyed rather than gone -- the back one on the first page and the \
               forward one on the last. Pressing past the last page stops there rather than \
               wrapping round.",
        since: RETAIL,
        evidence: Evidence::Private("AC-EVID-AF25-PAGING"),
        station: "dereth-testkit::dat::ui::scenario_paging_a_book_moves_the_page_and_greys_what_cannot_be_pressed",
        tier: Tier::Dat,
    },
    behaviour! {
        id: "strings.a-row-places-the-values-by-name-so-the-order-they-are-given-in-is-invisible",
        says: "A shipped line of text places the values it is handed by the names it carries, not \
               by the order they arrive in: two lines that name the same pair the opposite way \
               round both read correctly from one supply, a line naming the same value twice \
               writes it twice, a line of seven terms reads the same whichever way the terms come, \
               and a name the line does not carry puts nothing on the screen at all.",
        since: RETAIL,
        evidence: Evidence::Private("AC-EVID-P1-54-BY-NAME"),
        station: "dereth-testkit::dat::ui::scenario_a_line_places_the_values_by_name_and_not_by_the_order_given",
        tier: Tier::Dat,
    },
    behaviour! {
        id: "strings.a-run-of-spaces-in-a-shipped-line-is-drawn-as-one-and-nothing-else-moves",
        says: "Where a shipped line of text carries two spaces in a row the player reads one: the \
               line really does ship the pair, and the client draws it collapsed -- while lines \
               that carry no run and no markup come through unchanged, character for character.",
        since: RETAIL,
        evidence: Evidence::Private("AC-EVID-P1-54-COLLAPSE"),
        station: "dereth-testkit::dat::ui::scenario_a_run_of_spaces_in_a_shipped_line_is_drawn_as_one",
        tier: Tier::Dat,
    },
    behaviour! {
        id: "strings.a-value-the-caller-did-not-give-is-nothing-rather-than-a-complaint",
        says: "A line asked for with fewer values than it names fills the gaps with nothing at \
               all rather than with a marker saying something is missing, and what is left of the \
               line after the gaps is tidied the same way everything else is.",
        since: RETAIL,
        evidence: Evidence::Private("AC-EVID-P1-54-MISSING"),
        station: "dereth-testkit::dat::ui::scenario_a_value_the_caller_did_not_give_is_nothing_rather_than_a_complaint",
        tier: Tier::Dat,
    },
    behaviour! {
        id: "strings.the-key-binding-prompt-loses-both-of-its-double-spaces",
        says: "The line asking the player to press a key for an action ships two runs of two \
               spaces and is shown with neither of them, which is the second place on the screen \
               where the tidying is the difference between what ships and what is read.",
        since: RETAIL,
        evidence: Evidence::Private("AC-EVID-P1-54-PROMPT"),
        station: "dereth-testkit::dat::ui::scenario_the_key_binding_prompt_loses_both_of_its_double_spaces",
        tier: Tier::Dat,
    },
    behaviour! {
        id: "strings.the-one-line-that-asks-to-keep-its-spaces-keeps-them-and-loses-its-asking",
        says: "One shipped line asks in so many words to keep its spaces, and it is the one line \
               in the whole of the shipped text whose two readings differ by more than a run of \
               spaces: it keeps them, and the asking itself is not shown to anybody.",
        since: RETAIL,
        evidence: Evidence::Private("AC-EVID-P1-54-KEEPSPACES"),
        station: "dereth-testkit::dat::ui::scenario_the_one_line_that_asks_to_keep_its_spaces_keeps_them",
        tier: Tier::Dat,
    },
    behaviour! {
        id: "strings.the-pane-draws-the-tidied-line-and-not-the-shipped-double-space",
        says: "The pane about a death's lingering penalty draws the line with one space where the \
               shipped text has two -- read off the glyphs the pane really lays out, in the pane \
               the shipped layout carries, rather than off what the composer handed back.",
        since: RETAIL,
        evidence: Evidence::Private("AC-EVID-P1-54-VITAE-PANE"),
        station: "dereth-testkit::dat::ui::scenario_the_pane_draws_the_tidied_line_and_not_the_shipped_double_space",
        tier: Tier::Dat,
    },
    behaviour! {
        id: "toolbar.buttons.a-panel-opened-over-another-takes-the-covered-panels-button-down",
        says: "Opening a panel from its toolbar button while another panel is open lights the new \
               panel's button and takes the covered panel's button back down, so only one button \
               is ever lit; closing it leaves every button dark, and opening the first panel again \
               lights its button again.",
        since: RETAIL,
        evidence: Evidence::Private("AC-EVID-O281-BUTTONS"),
        station: "dereth-ui-screens::dat::panels::toolbar_panel_buttons::a_panel_opened_over_another_takes_the_first_ones_button_down_with_it",
        tier: Tier::Dat,
    },
    behaviour! {
        id: "toolbar.buttons.the-panel-notice-alone-moves-the-button",
        says: "A toolbar button follows its panel however the panel is opened: with no click at \
               all, a panel coming up lights its button and darkens the button of the panel it \
               covered, and the last panel closing leaves every button dark.",
        since: RETAIL,
        evidence: Evidence::Private("AC-EVID-O281-BUTTONS-PANEL"),
        station: "dereth-ui-screens::dat::panels::toolbar_panel_buttons::the_notice_alone_moves_the_button_in_both_directions",
        tier: Tier::Dat,
    },
    behaviour! {
        id: "toolbar.layout.a-restored-toolbar-is-clamped-inside-its-parent",
        says: "A saved screen layout that places the toolbar off the screen, at 800 by 600 or 1024 \
               by 768, puts it back just inside the window that holds it without moving or opening \
               the backpack, and saving the layout again records where the toolbar really ended up \
               while the file it was read from is left unchanged.",
        since: RETAIL,
        evidence: Evidence::Private("AC-EVID-ASTRA-TOOLBAR-LAYOUT-LAYOUT"),
        station: "dereth-ui-screens::dat::panels::toolbar_layout::local_restore_clamps_the_real_toolbar_without_showing_or_moving_pans",
        tier: Tier::Dat,
    },
    behaviour! {
        id: "toolbar.layout.the-toolbar-writes-its-achieved-position-and-size-back-in-order",
        says: "Every move, resize or show of the toolbar writes its left, top, width, height and \
               visibility into the character's settings in that order, even when nothing changed, \
               and what is written is where it really ended up after being kept inside its parent, \
               with the visibility it was given; the backpack window writes nothing this way.",
        since: RETAIL,
        evidence: Evidence::Private("AC-EVID-ASTRA-TOOLBAR-PERSISTENCE-LAYOUT"),
        station: "dereth-ui-screens::dat::panels::toolbar_layout::persistence::actual_toolbar_writes_equal_calls_and_achieved_values_in_source_order",
        tier: Tier::Dat,
    },
    behaviour! {
        id: "toolbar.layout.visibility-wins-over-a-saved-layout",
        says: "When the character's settings from the shard place the toolbar it is moved before \
               it is resized and kept inside its parent; a saved layout file then decides its \
               position and size and later settings from the shard do not move it again, but the \
               shard's hidden or shown flag still hides or shows it, and loading a layout never \
               makes a hidden toolbar appear.",
        since: RETAIL,
        evidence: Evidence::Private("AC-EVID-ASTRA-TOOLBAR-LAYOUT-LAYOUT-VISIBILITY"),
        station: "dereth-ui-screens::dat::panels::toolbar_layout::player_module_move_precedes_resize_and_visibility_still_wins_over_local_layout",
        tier: Tier::Dat,
    },
    behaviour! {
        id: "toolbar.selection.the-four-selection-children-are-bound-and-driven",
        says: "Selecting a creature writes its name into the toolbar's selected-object field, and \
               reports of its health and mana show the two small meters under the name and fill \
               them to the fraction reported.",
        since: RETAIL,
        evidence: Evidence::Private("AC-EVID-P1180-SELECTION"),
        station: "dereth-ui-screens::dat::panels::vitals_and_toolbar_selection_bindings::this_build_binds_the_four_selection_children_through_the_catalogue_and_drives_them",
        tier: Tier::Dat,
    },
    behaviour! {
        id: "tooltip.a-label-too-long-for-its-box-can-be-read-and-catches-the-pointer",
        says: "A label the client had to cut short offers its whole text when the pointer rests on \
               it, and starts catching the pointer so that it can be rested on at all -- a label \
               whose text fits offers nothing and lets a press fall straight through it.",
        since: RETAIL,
        evidence: Evidence::Private("AC-EVID-G12-TRUNCATED"),
        station: "dereth-testkit::dat::ui::scenario_a_label_too_long_for_its_box_can_be_read_and_catches_the_pointer",
        tier: Tier::Dat,
    },
    behaviour! {
        id: "tooltip.resting-the-pointer-on-something-draws-a-box-with-words-in-it",
        says: "Resting the pointer on something that offers a tooltip draws a box that really has \
               a size and really has words in it, rather than an empty frame. Nothing offers one \
               with the screen at rest: a panel has to be open first.",
        since: RETAIL,
        evidence: Evidence::Private("AC-EVID-G12"),
        station: "dereth-testkit::dat::ui::scenario_resting_the_pointer_on_something_draws_a_box_with_words_in_it",
        tier: Tier::Dat,
    },
    behaviour! {
        id: "tooltip.the-words-are-the-ones-that-element-was-given",
        says: "The words in a tooltip are the ones the shipped layout gives that element -- not a \
               neighbour's, and not an empty string in a box of the right size.",
        since: RETAIL,
        evidence: Evidence::Private("AC-EVID-G12-STRING"),
        station: "dereth-testkit::dat::ui::scenario_the_words_in_a_tooltip_are_the_ones_that_element_was_given",
        tier: Tier::Dat,
    },
    behaviour! {
        id: "ui.bindings.the-shipped-emote-keys-reach-their-emote-actions",
        says: "The emote keys in the shipped key map each reach the emote action they are bound \
               to, dispatched as emotes rather than being dropped.",
        since: RETAIL,
        evidence: Evidence::Private("AC-EVID-O961-BINDINGS"),
        station: "dereth-client::gpu::ui::shipped_key_bindings::the_five_shipped_emote_keys_reach_their_own_map",
        tier: Tier::Gpu,
    },
    behaviour! {
        id: "ui.button.a-click-on-a-button-with-an-action-stops-bubbling",
        says: "A click on a button that carries its own action is handled by the button and goes \
               no further up the window, so one click opens the button's page and the next click \
               closes it.",
        since: RETAIL,
        evidence: Evidence::Private("AC-EVID-O309-BUTTON"),
        station: "dereth-ui::cpu::controls::button_dispatch::a_live_0x12_stops_the_message_and_the_page_toggles_at_two_stations",
        tier: Tier::Cpu,
    },
    behaviour! {
        id: "ui.caret.a-focused-editable-box-draws-a-blinking-one-pixel-caret",
        says: "A text box that can be typed in and has focus draws a one-pixel-wide caret after \
               its text, and the caret blinks on and off.",
        since: RETAIL,
        evidence: Evidence::Private("AC-EVID-P1-CARET"),
        station: "dereth-ui::cpu::text::caret::a_focused_editable_box_draws_a_one_pixel_caret_that_blinks",
        tier: Tier::Cpu,
    },
    behaviour! {
        id: "ui.caret.flashes-at-the-interval-the-player-set-for-their-desktop",
        says: "The caret in a box being typed into flashes at the rate the player chose for their \
               desktop, not at a rate this client picked: the running client asks the desktop for \
               the interval every frame, and the caret is drawn inside it and gone past it. A \
               desktop that answers with no interval at all leaves the caret flashing every frame.",
        since: RETAIL,
        evidence: Evidence::Private("AC-EVID-P1-1"),
        station: "dereth-testkit::dat::ui::scenario_the_caret_flashes_at_the_players_own_desktop_interval",
        tier: Tier::Dat,
    },
    behaviour! {
        id: "ui.clipboard.copies-as-unicode-text-with-one-terminator",
        says: "Copied text is put on the clipboard as Unicode text, character for character, \
               followed by exactly one terminator, and an empty copy is still a single terminator.",
        since: RETAIL,
        evidence: Evidence::Private("AC-EVID-FORMAT-CLIPBOARD"),
        station: "dereth-clipboard::cpu::format::the_payload_is_utf16_with_one_trailing_nul",
        tier: Tier::Cpu,
    },
    behaviour! {
        id: "ui.cursor.aliased-cursor-keys-share-one-hotspot",
        says: "When two pointer states the client can reach are shown with the same shipped cursor \
               image, they also use the same point on it, so switching between them never leaves \
               the cursor pointing from the wrong spot.",
        since: RETAIL,
        evidence: Evidence::Private("AC-EVID-O622-CURSOR"),
        station: "dereth-client::dat::ui::cursor_state::every_aliased_pair_of_reachable_cursor_keys_shares_one_hotspot",
        tier: Tier::Dat,
    },
    behaviour! {
        id: "ui.cursor.an-element-override-wins-and-leaving-restores-the-default",
        says: "The cursor shown is the one belonging to the element holding the pointer, else the \
               one belonging to the element under it, else the default; moving off an element with \
               its own cursor brings the default back.",
        since: RETAIL,
        evidence: Evidence::Private("AC-EVID-O142-CURSOR-ELEMENT"),
        station: "dereth-ui::cpu::elements::cursor_override::the_override_chain_prefers_capture_then_hover_then_the_default",
        tier: Tier::Cpu,
    },
    behaviour! {
        id: "ui.cursor.every-pointer-state-shows-the-shipped-cursor-for-it",
        says: "Each pointer state (idle, busy, using, targeting and the rest, over an object or \
               not) shows the shipped cursor named for it, with its point at the image's corner \
               or, for the using and targeting cursors when not busy, fourteen pixels in on each \
               axis.",
        since: RETAIL,
        evidence: Evidence::Private("AC-EVID-O142-CURSOR"),
        station: "dereth-client::dat::ui::cursor_state::each_of_the_nine_states_pushes_the_did_and_hotspot_the_client_pushes",
        tier: Tier::Dat,
    },
    behaviour! {
        id: "ui.cursor.legal-and-illegal-target-cursors-are-the-right-way-round",
        says: "While picking a target for an item, hovering over something it can be used on shows \
               the valid-target cursor and hovering over something it cannot shows the different \
               invalid-target cursor.",
        since: RETAIL,
        evidence: Evidence::Private("AC-EVID-O142-CURSOR-LEGAL"),
        station: "dereth-client::dat::ui::cursor_state::the_legal_and_illegal_targeting_cursors_are_different_surfaces_and_the_right_way_round",
        tier: Tier::Dat,
    },
    behaviour! {
        id: "ui.dialog.a-menu-dialog-answers-into-its-property",
        says: "A dialog that offers a menu records the chosen entry as its answer, and a menu with \
               nothing in it answers minus one.",
        since: RETAIL,
        evidence: Evidence::Private("AC-EVID-O190-DIALOG-MENU"),
        station: "dereth-ui::dat::dialog_queue_and_menus::a_menu_dialog_answers_into_0xa4_and_an_empty_menu_answers_minus_one_for_a_named_reason",
        tier: Tier::Dat,
    },
    behaviour! {
        id: "ui.dialog.a-second-dialog-on-the-same-queue-waits-behind-the-first",
        says: "A second dialog raised on the same queue while one is already up, such as an error \
               message behind the delete warning, waits behind it and appears only when the first \
               closes, and the open one's banner counts the one waiting.",
        since: RETAIL,
        evidence: Evidence::Private("AC-EVID-O190-DIALOG"),
        station: "dereth-ui::dat::dialog_queue_and_menus::a_second_dialog_on_the_same_queue_waits_and_the_banner_counts_it",
        tier: Tier::Dat,
    },
    behaviour! {
        id: "ui.dialog.each-kind-answers-on-its-own-buttons-into-its-own-property",
        says: "Each kind of dialog records its answer in a place of its own and labels its own \
               buttons, so a caller reading one kind's answer never reads another kind's.",
        since: RETAIL,
        evidence: Evidence::Private("AC-EVID-O175-DIALOG"),
        station: "dereth-ui::dat::dialog_answers::each_kind_answers_into_its_own_property_and_captions_its_own_buttons",
        tier: Tier::Dat,
    },
    behaviour! {
        id: "ui.dialogs.a-message-raised-under-an-open-question-waits-and-a-duplicate-context-is-refused",
        says: "On the character select screen an error that arrives while the exit question is up \
               waits behind it instead of stacking on it, and the question shows that one more is \
               waiting; answering No quits nothing and takes the question down, the error box \
               comes up in its place, and its one button closes it and leaves nothing open.",
        since: RETAIL,
        evidence: Evidence::Private("AC-EVID-O190-DIALOGS"),
        station: "dereth-ui-screens::dat::ui::dialog_queue::an_error_message_raised_while_the_exit_confirmation_is_up_waits_for_it",
        tier: Tier::Dat,
    },
    behaviour! {
        id: "ui.drag.a-drag-ghost-keeps-the-icon-extent-after-a-resize",
        says: "After the window goes full screen, a newly started drag shows a ghost exactly the \
               size of the item's icon, not one stretched by how much the display grew.",
        since: RETAIL,
        evidence: Evidence::Private("AC-EVID-P181-DRAG"),
        station: "dereth-client::gpu::ui::drag_after_resize::after_going_full_screen_a_fresh_drag_is_still_the_icon_sized_quad",
        tier: Tier::Gpu,
    },
    behaviour! {
        id: "ui.drag.a-refused-drag-raises-rejected-at-every-level-it-climbed",
        says: "When nothing under the pointer can be dragged, every element the drag tried on its \
               way up the window is told the drag was refused, the pressed element last.",
        since: RETAIL,
        evidence: Evidence::Private("AC-EVID-P4-DRAG"),
        station: "dereth-ui::dat::drag_walk::every_level_the_walk_entered_raises_its_own_drag_rejected",
        tier: Tier::Dat,
    },
    behaviour! {
        id: "ui.draw.a-tiled-element-repeats-its-picture-at-every-seam",
        says: "An element drawn wider than its picture repeats the picture at every seam, a \
               partial last copy included, instead of stretching it.",
        since: RETAIL,
        evidence: Evidence::Private("AC-EVID-UI-TILING-DRAW"),
        station: "dereth-client::gpu::ui::tiling_and_rotation::a_tiled_element_repeats_its_picture_at_every_seam",
        tier: Tier::Gpu,
    },
    behaviour! {
        id: "ui.draw.button-backgrounds-blend-and-erase-changes-nothing",
        says: "Button backgrounds are blended over the artwork behind them and change pixels only \
               inside their own boxes.",
        since: RETAIL,
        evidence: Evidence::Private("AC-EVID-UI-FILL-DRAW"),
        station: "dereth-client::gpu::ui::fill_and_erase::the_button_background_is_blended_over_the_artwork_and_lands_only_inside_its_own_box",
        tier: Tier::Gpu,
    },
    behaviour! {
        id: "ui.edit.selected-text-draws-one-highlight-per-selected-glyph",
        says: "Selected text in an edit box is highlighted with one inverted rectangle per \
               selected letter, each starting where that letter is drawn and a full line tall.",
        since: RETAIL,
        evidence: Evidence::Private("AC-EVID-O477-EDIT"),
        station: "dereth-client::gpu::ui::edit_field_selection_highlight::the_selection_is_carried_into_the_draw_list_as_one_rectangle_per_selected_glyph",
        tier: Tier::Gpu,
    },
    behaviour! {
        id: "ui.escape.unwinds-jump-target-cursor-and-target-before-opening-options",
        says: "Escape undoes one thing at a time in a fixed order: a jump being charged is \
               finished first and nothing else happens; failing that the targeting cursor is put \
               away, then the selected target is dropped, and only with nothing to undo does it \
               open the gameplay options.",
        since: RETAIL,
        evidence: Evidence::Private("AC-EVID-O635-ESCAPE"),
        station: "dereth-client::gpu::ui::escape_and_action_arms::escape_finishes_a_jump_before_it_looks_at_anything_else",
        tier: Tier::Gpu,
    },
    behaviour! {
        id: "ui.focus.a-log-takes-the-keyboard-and-none-of-what-is-typed",
        says: "The chat log takes the keyboard when it is pressed and swallows every character \
               typed into it, because a log is something to sweep and read and not something to \
               write in -- and the one bit that would change that is the bit it ships without, \
               shown by setting it and watching the same keystrokes land.",
        since: RETAIL,
        evidence: Evidence::Private("AC-EVID-O87-LOG"),
        station: "dereth-testkit::dat::ui::scenario_a_log_takes_the_keyboard_and_none_of_what_is_typed",
        tier: Tier::Dat,
    },
    behaviour! {
        id: "ui.focus.a-press-in-a-box-takes-the-keyboard-and-keeps-it",
        says: "Nothing holds the keyboard until the player presses something: one press in a text \
               box gives it the keyboard, and it still has it ten frames later, the element still \
               alive under it.",
        since: RETAIL,
        evidence: Evidence::Private("AC-EVID-O87-FOCUS"),
        station: "dereth-testkit::dat::ui::scenario_a_press_in_a_box_takes_the_keyboard_and_keeps_it",
        tier: Tier::Dat,
    },
    behaviour! {
        id: "ui.focus.a-text-boxs-focus-policy-comes-from-its-shipped-properties",
        says: "The quantity box of the stack splitter takes its keyboard rules from its shipped \
               layout: it lets go of the keyboard when Enter is pressed, because the layout says \
               so, and keeps it on Escape, which the layout leaves unset.",
        since: RETAIL,
        evidence: Evidence::Private("AC-EVID-ASTRA-TYPED-SPLIT-FOCUS"),
        station: "dereth-ui-screens::dat::ui::text_focus_policy::shipped_stack_box_focus_policy_is_consumed_from_its_resolved_properties",
        tier: Tier::Dat,
    },
    behaviour! {
        id: "ui.focus.losing-focus-releases-every-held-control",
        says: "When the client window loses focus, controls held at that moment are released: a \
               held mouse-look button stops turning the camera, and a fresh press after focus \
               returns looks again.",
        since: RETAIL,
        evidence: Evidence::Private("AC-EVID-O689-FOCUS"),
        station: "dereth-client::gpu::ui::focus_loss_release::focus_loss_releases_the_held_mouse_look_button",
        tier: Tier::Gpu,
    },
    behaviour! {
        id: "ui.focus.the-name-field-of-the-wizard-takes-a-typed-name",
        says: "The summary page's name field is a box a player can press and type into: the \
               characters typed after the press reach the wizard's own record of the name, and the \
               wizard marks the name as entered, which is what it refuses to finish without.",
        since: RETAIL,
        evidence: Evidence::Private("AC-EVID-O87-NAME"),
        station: "dereth-testkit::dat::ui::scenario_the_name_field_of_the_wizard_takes_a_typed_name",
        tier: Tier::Dat,
    },
    behaviour! {
        id: "ui.focus.what-is-typed-reaches-the-box-holding-the-keyboard-and-stops-when-it-lets-go",
        says: "Typing is off until a box holds the keyboard: a press turns it on, the characters \
               typed land in that box, and letting the keyboard go turns it off again -- after \
               which the same keys reach nothing at all and the box keeps what its owner settled \
               on it.",
        since: RETAIL,
        evidence: Evidence::Private("AC-EVID-O87-TYPING"),
        station: "dereth-testkit::dat::ui::scenario_what_is_typed_reaches_the_box_holding_the_keyboard",
        tier: Tier::Dat,
    },
    behaviour! {
        id: "ui.fonts.every-font-the-layouts-name-rasterises",
        says: "Every font the game data ships produces a glyph texture in which every one of its \
               characters is addressable and every printable character can be drawn.",
        since: RETAIL,
        evidence: Evidence::Private("AC-EVID-O94-FONTS"),
        station: "dereth-client::gpu::ui::font_rasterisation::every_retail_font_rasterises_and_the_range_and_texture_size_are_stated",
        tier: Tier::Gpu,
    },
    behaviour! {
        id: "ui.item-cell.a-reused-slot-never-keeps-an-old-item-or-spell-identity",
        says: "A pack slot that is filled again carries only what it now shows: a spell put in a \
               slot that held an item clears the item, an item put back clears the spell, emptying \
               the slot clears both, and once the screen is torn down neither the old slot nor a \
               newly made element answers as the old item.",
        since: RETAIL,
        evidence: Evidence::Private("AC-EVID-ASTRA-UI-ITEM-ITEM-CELL"),
        station: "dereth-ui-screens::dat::ui::item_slot::real_item_assignment_spell_clear_and_reused_arena_never_leak_an_old_item_identity",
        tier: Tier::Dat,
    },
    behaviour! {
        id: "ui.item-cell.dropping-a-thing-back-on-its-own-slot-sends-nothing-and-ghosts-nothing",
        says: "Picking a thing up out of the pack greys its slot, and dropping it back on the very \
               slot it came from asks the shard for nothing at all and takes the greying off \
               again -- which matters because with nothing sent there is no answer coming that \
               could ever take it off.",
        since: RETAIL,
        evidence: Evidence::Private("AC-EVID-AF21-NOOP-DROP"),
        station: "dereth-testkit::dat::ui::scenario_dropping_a_thing_back_on_its_own_slot_sends_nothing_and_ghosts_nothing",
        tier: Tier::Dat,
    },
    behaviour! {
        id: "ui.item-cell.the-drag-icon-is-the-slots-own-child",
        says: "The picture an inventory slot lifts when a drag starts is the slot's own child: it \
               can start a drag at once, and the slot is allowed to build the carried copy, so a \
               drag from a filled slot always has its own picture to carry.",
        since: RETAIL,
        evidence: Evidence::Private("AC-EVID-P4-ITEM-CELL"),
        station: "dereth-ui-screens::dat::ui::item_slot::drag_icon::the_drag_icon_is_the_slots_child_and_the_slot_resolves_it_back",
        tier: Tier::Dat,
    },
    behaviour! {
        id: "ui.item-cell.the-players-own-cell-draws-a-backpack-and-not-a-second-backdrop",
        says: "The cell standing for the player themself draws a backpack over its backing tile: \
               the picture it reaches for is a different one from the tile, and the pixels it \
               composites change part of the tile and leave the rest of it showing, which a second \
               opaque backdrop could not do.",
        since: RETAIL,
        evidence: Evidence::Private("AC-EVID-AF11-BACKPACK"),
        station: "dereth-testkit::dat::ui::scenario_the_players_own_cell_draws_a_backpack_and_not_a_second_backdrop",
        tier: Tier::Dat,
    },
    behaviour! {
        id: "ui.item-cell.what-follows-the-cursor-is-the-icon-alone-and-not-the-lifted-cell",
        says: "What follows the cursor off a real slot of the shipped pack is the thing's own \
               picture with no backing tile and no underlay beneath it -- not the cell's whole \
               composite -- so it reads as a floating icon and keeps clear pixels around itself.",
        since: RETAIL,
        evidence: Evidence::Private("AC-EVID-AF20-DRAG-SURFACE"),
        station: "dereth-testkit::dat::ui::scenario_what_follows_the_cursor_is_the_icon_alone",
        tier: Tier::Dat,
    },
    behaviour! {
        id: "ui.key-binding.a-rebound-key-fires-the-action-and-the-old-key-stops",
        says: "Rebinding a key for an action in the key map makes the new key fire that action and \
               the key it replaced stop firing it, while the action's other bound key keeps \
               working.",
        since: RETAIL,
        evidence: Evidence::Private("AC-EVID-O196-KEY-BINDING"),
        station: "dereth-input::dat::key_rebinding::rebinding_move_forward_moves_which_key_fires_it",
        tier: Tier::Dat,
    },
    behaviour! {
        id: "ui.keyboard.a-bare-escape-does-not-end-the-process",
        says: "Pressing Escape never closes the client, whether or not the chat entry box holds \
               the keyboard; in the chat box it simply takes the keyboard away from the box.",
        since: RETAIL,
        evidence: Evidence::Private("AC-EVID-O150-KEYBOARD-BARE"),
        station: "dereth-client::gpu::ui::typing_barrier::escape_no_longer_ends_the_process",
        tier: Tier::Gpu,
    },
    behaviour! {
        id: "ui.keyboard.a-focused-chat-box-blocks-movement-and-camera-keys",
        says: "While the chat entry box holds the keyboard, a held movement key does not move the \
               player though the letter still goes into the box; once the box lets go, the same \
               key walks again.",
        since: RETAIL,
        evidence: Evidence::Private("AC-EVID-O150-KEYBOARD"),
        station: "dereth-client::gpu::ui::typing_barrier::a_held_w_walks_only_while_the_chat_box_does_not_have_focus",
        tier: Tier::Gpu,
    },
    behaviour! {
        id: "ui.keyboard.in-world-keys-do-nothing-before-the-player-is-in-the-world",
        says: "On the intro, the character screens, character creation and the disconnected \
               screen, keys for things done in the world do nothing: E does not start an examine \
               and R does not start a use, so the cursor does not change, and movement, panel, \
               quickbar, emote, combat and chat keys are dead too, while Escape still answers the \
               screen. In the world they all work again.",
        since: RETAIL,
        evidence: Evidence::Private("AC-EVID-PREGAME-KEYS"),
        station: "dereth-client::dat::ui::pregame_keys::e_and_r_start_no_examine_or_use_on_the_character_screen_and_do_in_the_world",
        tier: Tier::Dat,
    },
    behaviour! {
        id: "ui.layout.a-saved-layout-is-reloaded-at-the-next-login",
        says: "A window layout saved with the save-layout chat command is loaded again \
               automatically at the next login, after entering the world has restored the player's \
               chosen resolution, and the moved window comes back where it was saved.",
        since: RETAIL,
        evidence: Evidence::Private("AC-EVID-O246-LAYOUT"),
        station: "dereth-client::gpu::ui::screen_layout_persistence::wire_login_automatically_loads_the_layout_for_the_restored_gameplay_resolution",
        tier: Tier::Gpu,
    },
    behaviour! {
        id: "ui.layout.the-hide-attribute-hides-an-element-at-build",
        says: "An element that its shipped layout marks hidden comes up hidden when the screen is \
               built: of the toolbar's four stacked combat-stance icons only the peace-mode dove, \
               the one not marked, is showing before any combat mode is set.",
        since: RETAIL,
        evidence: Evidence::Private("AC-EVID-HIDE-POLARITY-LAYOUT"),
        station: "dereth-ui-screens::dat::ui::layout_hide_attribute::the_toolbar_comes_up_showing_only_the_peace_mode_dove",
        tier: Tier::Dat,
    },
    behaviour! {
        id: "ui.list.a-press-on-a-row-selects-it-and-pressing-the-selected-row-raises-activate",
        says: "Pressing the row of a list that is already selected announces that row as pressed \
               again, which is what lets a quick second press act on it, while a double click by \
               itself announces nothing.",
        since: RETAIL,
        evidence: Evidence::Private("AC-EVID-G15-LIST"),
        station: "dereth-ui::cpu::controls::listbox_press::pressing_the_already_selected_row_raises_0x43_and_a_double_click_does_not",
        tier: Tier::Cpu,
    },
    behaviour! {
        id: "ui.list.a-row-built-from-a-template-is-a-list-item-and-a-press-selects-it",
        says: "A row added to a list from one of its shipped templates is a real item of that \
               list, so a press over the row lands on the list and selects that row: the list \
               reports one selection change naming the row and its position, and the pressed row \
               is the one selected.",
        since: RETAIL,
        evidence: Evidence::Private("AC-EVID-O235-LIST"),
        station: "dereth-ui-screens::dat::ui::listbox_template_rows::a_real_press_on_a_row_selects_it_and_raises_message_4",
        tier: Tier::Dat,
    },
    behaviour! {
        id: "ui.list.pressing-a-row-draws-the-band-across-that-row-and-no-other",
        says: "Pressing a row of a list draws the lighter band across that row, with its \
               neighbours untouched; pressing another row moves the band and puts the first one \
               back the way it was.",
        since: RETAIL,
        evidence: Evidence::Private("AC-EVID-P1-24"),
        station: "dereth-testkit::dat::ui::scenario_pressing_a_row_draws_the_band_across_that_row",
        tier: Tier::Dat,
    },
    behaviour! {
        id: "ui.lock.locking-hides-every-windows-grab-handles-and-unlocking-shows-them",
        says: "Locking the interface hides the eight grab handles round every movable window and \
               puts plain borders in their place, and unlocking swaps them back, in both \
               directions across unlock, lock and unlock again.",
        since: RETAIL,
        evidence: Evidence::Private("AC-EVID-O375-LOCK"),
        station: "dereth-ui-screens::dat::ui::ui_lock_window_chrome::the_lock_swaps_both_halves_of_every_chrome_block_at_three_stations",
        tier: Tier::Dat,
    },
    behaviour! {
        id: "ui.lock.the-world-view-hides-its-borders-while-locked",
        says: "The world view's eight borders are shown while the interface is unlocked and hidden \
               while it is locked, with nothing put in their place, across unlock, lock and unlock \
               again.",
        since: RETAIL,
        evidence: Evidence::Private("AC-EVID-O375-LOCK-WORLD"),
        station: "dereth-ui-screens::dat::ui::ui_lock_window_chrome::the_world_view_hides_its_eight_borders_when_the_ui_is_locked",
        tier: Tier::Dat,
    },
    behaviour! {
        id: "ui.panel.the-first-visibility-mode-toggles-the-page",
        says: "A page whose visibility mode is the first one toggles each time its action fires, \
               opening on one click and closing on the next.",
        since: RETAIL,
        evidence: Evidence::Private("AC-EVID-O309-PANEL"),
        station: "dereth-ui::cpu::controls::button_dispatch::a_live_0x12_stops_the_message_and_the_page_toggles_at_two_stations",
        tier: Tier::Cpu,
    },
    behaviour! {
        id: "ui.screen-rebuild.putting-the-same-screen-up-again-builds-it-fresh",
        says: "Asking for the screen that is already up builds it again from the layout rather \
               than leaving it alone: every element of the old one is gone, the heads-up display \
               is bound to the new ones, and they come up as the layout makes them rather than \
               carrying whatever the old ones had been told. Anything said while the old ones \
               were there is dropped with them, and the new ones answer what is said next.",
        since: RETAIL,
        evidence: Evidence::Private("AC-EVID-ASTRA-UI-GENERATION"),
        station: "dereth-testkit::dat::ui::scenario_putting_the_same_screen_up_again_builds_it_fresh",
        tier: Tier::Dat,
    },
    behaviour! {
        id: "ui.screens.every-screen-builds-from-its-shipped-layout-and-binds-its-documented-children",
        says: "Every screen the client puts up builds from its shipped layout with its top-level \
               windows in the expected order, and taking it down again, as a change of screen \
               does, leaves nothing behind.",
        since: RETAIL,
        evidence: Evidence::Private("AC-EVID-SCREEN-CONFORMANCE-SCREENS"),
        station: "dereth-ui-screens::dat::ui::screen_conformance::every_screen_constructs_from_its_real_layout_and_leaks_no_handles",
        tier: Tier::Dat,
    },
    behaviour! {
        id: "ui.scrollbar.a-goal-position-glides-over-its-duration",
        says: "A scrollbar set to move smoothly to a new position glides there over its set \
               duration and stops updating once it arrives.",
        since: RETAIL,
        evidence: Evidence::Private("AC-EVID-O91-SCROLLBAR-GOAL"),
        station: "dereth-ui::cpu::controls::scrollbar::a_goal_position_glides_over_its_duration_and_stops_registering_when_it_arrives",
        tier: Tier::Cpu,
    },
    behaviour! {
        id: "ui.scrollbar.a-held-arrow-repeats-at-its-authored-period-not-per-frame",
        says: "Holding down a scrollbar arrow scrolls at the rate its layout sets, the same number \
               of rows each second at 30 and at 240 frames per second.",
        since: RETAIL,
        evidence: Evidence::Private("AC-EVID-O506-SCROLLBAR"),
        station: "dereth-ui::dat::scrollbar_hold_repeat::a_held_arrow_scrolls_the_same_rows_per_second_at_thirty_and_at_two_hundred_and_forty_fps",
        tier: Tier::Dat,
    },
    behaviour! {
        id: "ui.scrollbar.dragging-the-stack-slider-moves-the-thumb-and-the-quantity-follows",
        says: "The stack splitter's slider is dragged with the pointer: the thumb goes where the \
               pointer takes it, stops hard at each end of its travel, and the quantity to split \
               off follows it from one item at the near end to the whole stack at the far one -- \
               and a released slider stops following the pointer.",
        since: RETAIL,
        evidence: Evidence::Private("AC-EVID-O87-SLIDER"),
        station: "dereth-testkit::dat::ui::scenario_dragging_the_stack_slider_moves_the_thumb_and_the_quantity_follows",
        tier: Tier::Dat,
    },
    behaviour! {
        id: "ui.scrollbar.the-arrows-step-the-content-the-right-way",
        says: "The arrow at the top of a scrollbar scrolls the content back towards its start and \
               the arrow at the bottom forward, wherever the window's layout first put the two \
               arrows.",
        since: RETAIL,
        evidence: Evidence::Private("AC-EVID-O91-SCROLLBAR"),
        station: "dereth-ui::cpu::controls::scrollbar::the_arrow_at_the_top_of_the_bar_walks_the_log_back",
        tier: Tier::Cpu,
    },
    behaviour! {
        id: "ui.scrollbar.the-chat-bars-thumb-is-sized-and-placed-inside-its-track",
        says: "The chat window's bar puts its thumb in the groove between its two arrows rather \
               than leaving it at the box the layout drew it in: it is as wide as the bar, it \
               starts flush under the top arrow, and with nothing to scroll it fills the whole \
               track instead of sitting in it as a stub.",
        since: RETAIL,
        evidence: Evidence::Private("AC-EVID-O87-THUMB"),
        station: "dereth-testkit::dat::ui::scenario_the_chat_bars_thumb_is_sized_and_placed_inside_its_track",
        tier: Tier::Dat,
    },
    behaviour! {
        id: "ui.selection-strip.empty-a-single-item-and-a-stack-are-three-different-strips",
        says: "Nothing selected, one thing selected and a stack selected are three strips and not \
               two: the box and the slider that split a stack are down on an empty strip and down \
               beside a single thing, come up for a stack, and go down again when the selection \
               is dropped.",
        since: RETAIL,
        evidence: Evidence::Private("AC-EVID-AF17-THREE-STATES"),
        station: "dereth-testkit::dat::ui::scenario_empty_a_single_item_and_a_stack_are_three_different_strips",
        tier: Tier::Dat,
    },
    behaviour! {
        id: "ui.selection-strip.the-highlight-comes-down-by-itself-a-quarter-second-later",
        says: "The plate that lights up behind a new selection comes down on its own a quarter of \
               a second later, and only the plate goes -- the name beside it and the strip itself \
               stay -- and selecting again lights it up afresh.",
        since: RETAIL,
        evidence: Evidence::Private("AC-EVID-AF22-HIGHLIGHT"),
        station: "dereth-testkit::dat::ui::scenario_the_highlight_comes_down_by_itself_a_quarter_second_later",
        tier: Tier::Dat,
    },
    behaviour! {
        id: "ui.selection-strip.the-meters-start-down-and-a-creatures-answer-brings-its-bar-up",
        says: "The two meters on the selection strip start down and stay down while nothing has \
               answered for the thing selected; the answer about a creature's health brings its \
               own bar up, and nothing brings the other one up with it.",
        since: RETAIL,
        evidence: Evidence::Private("AC-EVID-AF22-METERS"),
        station: "dereth-testkit::dat::ui::scenario_the_meters_start_down_and_a_creatures_answer_brings_its_bar_up",
        tier: Tier::Dat,
    },
    behaviour! {
        id: "ui.states.a-state-declared-with-no-media-runs-nothing-and-an-undeclared-one-runs-the-base",
        says: "An element put into a state the layout declares but gives nothing to draw runs \
               nothing at all, while an element put into a state the layout does not declare runs \
               what the element itself carries.",
        since: RETAIL,
        evidence: Evidence::Private("AC-EVID-P1-58-MEDIA"),
        station: "dereth-testkit::dat::ui::scenario_a_state_with_nothing_to_draw_runs_nothing",
        tier: Tier::Dat,
    },
    behaviour! {
        id: "ui.states.a-toggle-is-not-unticked-by-its-own-change-of-state",
        says: "A tick box put into the state that means ticked stays ticked: what it draws in \
               that state cannot come back round and untick it.",
        since: RETAIL,
        evidence: Evidence::Private("AC-EVID-P1-58-TOGGLE"),
        station: "dereth-testkit::dat::ui::scenario_a_toggle_is_not_unticked_by_its_own_state",
        tier: Tier::Dat,
    },
    behaviour! {
        id: "ui.strings.a-string-row-decides-where-each-named-value-goes",
        says: "A shipped text line decides where each named value goes, so one set of values fills \
               two lines that mention them in opposite orders, such as a key binding and its \
               overwrite question, correctly in both.",
        since: RETAIL,
        evidence: Evidence::Private("AC-EVID-P143D-STRINGS"),
        station: "dereth-ui::cpu::text::string_variables::one_named_supply_serves_two_rows_that_order_their_variables_oppositely",
        tier: Tier::Cpu,
    },
    behaviour! {
        id: "ui.surface.a-colour-picker-is-sized-from-its-own-box-whatever-the-layout-asks-for",
        says: "A colour picker is sized from its own box however the layout authored it: two \
               elements of one live screen are given the same authored size-from setting and the \
               picker alone answers with the element's own size, while its twin keeps what it was \
               given.",
        since: RETAIL,
        evidence: Evidence::Private("AC-EVID-O442-PICKER"),
        station: "dereth-testkit::dat::ui::scenario_a_colour_picker_is_sized_from_its_own_box_whatever_it_asks_for",
        tier: Tier::Dat,
    },
    behaviour! {
        id: "ui.surface.a-shipped-root-that-owns-none-is-left-without-one-though-the-maker-asks",
        says: "The maker of a layout root asks for a surface for every root it builds, and a root \
               the shipped layouts author as owning none is left without one all the same -- while \
               a root beside it that authors nothing keeps the surface the maker asked for.",
        since: RETAIL,
        evidence: Evidence::Private("AC-EVID-O442-ROOT"),
        station: "dereth-testkit::dat::ui::scenario_a_shipped_root_that_owns_no_surface_is_left_without_one",
        tier: Tier::Dat,
    },
    behaviour! {
        id: "ui.surface.an-element-owning-none-is-stepped-over-and-the-answer-is-its-owners",
        says: "An element told to own no surface stops owning one, and asking it where its surface \
               takes its size from steps past it to the nearest ancestor that owns one -- so it \
               never answers with its own setting, and a child of it lands on the same ancestor.",
        since: RETAIL,
        evidence: Evidence::Private("AC-EVID-O442-WALK"),
        station: "dereth-testkit::dat::ui::scenario_an_element_owning_no_surface_is_stepped_over",
        tier: Tier::Dat,
    },
    behaviour! {
        id: "ui.surface.every-element-of-a-live-screen-is-sized-from-its-own-box-by-default",
        says: "Every element of a whole raised screen answers where its surface takes its size \
               from, and on a screen that authors the setting nowhere every one of them takes the \
               element's own box -- which is the default the readers carry, not one of the other \
               three answers.",
        since: RETAIL,
        evidence: Evidence::Private("AC-EVID-O442-DEFAULT"),
        station: "dereth-testkit::dat::ui::scenario_every_element_of_a_live_screen_is_sized_from_its_own_box",
        tier: Tier::Dat,
    },
    behaviour! {
        id: "ui.tabs.a-tabbed-page-comes-up-with-each-tab-in-the-state-its-panel-set",
        says: "When the game screen is built, every tab of every tabbed panel comes up in the \
               state its panel set: the tab of the open page shows as open and every other tab as \
               closed.",
        since: RETAIL,
        evidence: Evidence::Private("AC-EVID-P1183-TABS"),
        station: "dereth-ui-screens::dat::ui::tab_initial_state::every_tab_of_every_panel_holds_the_state_the_panel_put_it_in",
        tier: Tier::Dat,
    },
    behaviour! {
        id: "ui.text.a-button-menu-or-scrollbar-draws-its-own-selection",
        says: "A button, a menu and a scrollbar that hold selectable text draw its selection just \
               as a plain text box does: nothing when nothing is selected and one highlighted box \
               per selected character.",
        since: RETAIL,
        evidence: Evidence::Private("AC-EVID-O477-TEXT"),
        station: "dereth-ui::cpu::text::widget_selection::every_class_that_derives_from_uielement_text_draws_its_selection",
        tier: Tier::Cpu,
    },
    behaviour! {
        id: "ui.text.a-caption-that-fits-and-asks-to-be-centred-is-still-centred",
        says: "A caption whose words fit the space it has and which asks to sit in the middle of \
               it still does -- the rule about a caption that does not fit does not flatten every \
               other one against the top.",
        since: RETAIL,
        evidence: Evidence::Private("AC-EVID-P1-49-CENTRED"),
        station: "dereth-testkit::dat::ui::scenario_a_caption_that_fits_is_still_centred",
        tier: Tier::Dat,
    },
    behaviour! {
        id: "ui.text.a-caption-too-tall-for-its-box-shows-its-first-line-at-the-top",
        says: "A caption whose words wrap onto more lines than it has room for shows the first \
               line, at the top of the space it has, and the rest is simply not shown -- rather \
               than the whole block being centred so that no line lands where it should.",
        since: RETAIL,
        evidence: Evidence::Private("AC-EVID-P1-49-TOP"),
        station: "dereth-testkit::dat::ui::scenario_a_caption_too_tall_for_its_box_shows_its_first_line_at_the_top",
        tier: Tier::Dat,
    },
    behaviour! {
        id: "ui.text.a-draw-measures-dirty-text-before-it-is-drawn",
        says: "A text pane whose text has grown past its box knows the new extent after the next \
               draw with no other step, so its scrollbar follows the text.",
        since: RETAIL,
        evidence: Evidence::Private("AC-EVID-P159-TEXT"),
        station: "dereth-ui::cpu::text::layout_pass::a_pane_whose_text_outgrows_its_box_reports_its_extent_after_one_draw",
        tier: Tier::Cpu,
    },
    behaviour! {
        id: "ui.text.a-paragraph-too-tall-for-its-box-keeps-every-line-and-scrolls",
        says: "A paragraph too long for the pane it is in keeps every one of its lines, each on \
               its own row and starting at the top, so scrolling really does bring the later ones \
               into view -- nothing is thrown away to make it fit.",
        since: RETAIL,
        evidence: Evidence::Private("AC-EVID-P1-49-SCROLL"),
        station: "dereth-testkit::dat::ui::scenario_a_paragraph_too_tall_for_its_box_keeps_every_line",
        tier: Tier::Dat,
    },
    behaviour! {
        id: "ui.text.a-right-justified-caption-that-wraps-sits-flush-against-its-box",
        says: "A caption that is pushed up against the right of its box and wraps onto a second \
               line sits flush against that edge: the space it broke at does not push the words \
               away from it.",
        since: RETAIL,
        evidence: Evidence::Private("AC-EVID-P1-64-LABEL"),
        station: "dereth-testkit::dat::ui::scenario_a_wrapped_right_justified_caption_is_flush_with_its_box",
        tier: Tier::Dat,
    },
    behaviour! {
        id: "ui.text.a-scrolling-pane-measures-what-it-holds-from-the-draw-itself",
        says: "A pane that can scroll works out how much it is holding when it is drawn, so text \
               written into it is measured without anything having to ask; a pane whose text fits \
               reports that it fits.",
        since: RETAIL,
        evidence: Evidence::Private("AC-EVID-P1-59"),
        station: "dereth-testkit::dat::ui::scenario_a_scrolling_pane_measures_itself_from_the_draw",
        tier: Tier::Dat,
    },
    behaviour! {
        id: "ui.text.a-ui-glyph-is-drawn-pixel-identical-to-its-source-bitmap",
        says: "Interface text lands exactly on the screen's pixel grid: every drawn letter is \
               pixel for pixel the font's own bitmap, with no softened edge, at different screen \
               sizes and positions and under either texture filter.",
        since: RETAIL,
        evidence: Evidence::Private("AC-EVID-O274-TEXT"),
        station: "dereth-render::gpu::ui::glyph_pixels::a_ui_glyph_is_pixel_identical_to_its_source_bitmap_under_both_samplers",
        tier: Tier::Gpu,
    },
    behaviour! {
        id: "ui.text.a-wrapped-line-does-not-carry-the-width-of-the-space-it-broke-at",
        says: "The width of a wrapped line is the words on it and not the space it broke at, \
               while the last line of a piece of text keeps a space at its end; and a space on \
               its own never wraps, so a line whose words exactly fill the box keeps it.",
        since: RETAIL,
        evidence: Evidence::Private("AC-EVID-P1-64-WRAP"),
        station: "dereth-testkit::dat::ui::scenario_a_wrapped_line_drops_the_width_of_its_break",
        tier: Tier::Dat,
    },
    behaviour! {
        id: "ui.text.an-escaped-line-break-in-a-string-table-reaches-the-pane-as-a-break",
        says: "Text from the shipped string table that marks its line breaks with a backslash and \
               an n, as the character sheet's sections do, reaches the pane with real line breaks \
               in their place and no backslash left in it.",
        since: RETAIL,
        evidence: Evidence::Private("AC-EVID-G33-TEXT"),
        station: "dereth-ui-screens::dat::ui::string_table_unescape::the_character_sheet_reaches_the_pane_unescaped",
        tier: Tier::Dat,
    },
    behaviour! {
        id: "ui.text.an-outlined-label-draws-eight-offset-glyphs-behind-the-foreground",
        says: "Text that asks for an outline in a font with no heavier backing sheet is outlined \
               by drawing each letter eight more times, one pixel off in each of the eight \
               directions around it, and each copy lands exactly where its offset says.",
        since: RETAIL,
        evidence: Evidence::Private("AC-EVID-O369-TEXT-OUTLINED"),
        station: "dereth-render::gpu::ui::outline_pixels::the_neighbourhood_outline_draws_eight_glyphs_each_exactly_where_its_offset_says",
        tier: Tier::Gpu,
    },
    behaviour! {
        id: "ui.text.each-appended-run-carries-its-own-font-and-colour",
        says: "Two pieces of text added to one text box with different font choices are drawn in \
               two different fonts from the box's own list of fonts.",
        since: RETAIL,
        evidence: Evidence::Private("AC-EVID-G14-TEXT"),
        station: "dereth-ui::cpu::text::per_run_font::two_runs_in_one_element_draw_in_two_different_fonts",
        tier: Tier::Cpu,
    },
    behaviour! {
        id: "ui.text.outline.every-element-that-asks-for-one-is-drawn-with-one-and-no-other-is",
        says: "Every piece of text in the shipped interface that asks for a dark backing behind it \
               is drawn with one, and nothing that does not ask is given one -- read as the whole \
               set both ways against what the shipped data resolves to, not as a sample of it.",
        since: RETAIL,
        evidence: Evidence::Private("AC-EVID-O369-SET"),
        station: "dereth-testkit::dat::ui::scenario_every_element_that_asks_for_an_outline_is_drawn_with_one",
        tier: Tier::Dat,
    },
    behaviour! {
        id: "ui.text.outline.it-comes-from-the-fonts-own-heavier-sheet-and-fits-inside-it",
        says: "The dark backing is a heavier copy of the same letters that the font itself ships: \
               thirty-seven of the forty-nine shipped fonts carry one and the other twelve declare \
               no spread at all, and every letter of every font that has one reads from inside its \
               own sheet once the spread is added. On the refusal the report was about, the \
               backing covers about three and a half times as many places as the letters do.",
        since: RETAIL,
        evidence: Evidence::Private("AC-EVID-O369-SHEETS"),
        station: "dereth-testkit::dat::ui::scenario_the_outline_comes_from_the_fonts_own_heavier_sheet",
        tier: Tier::Dat,
    },
    behaviour! {
        id: "ui.text.outline.the-refusal-strip-is-drawn-with-one-and-the-chat-log-is-not",
        says: "The strip the shard's refusals appear on asks for a dark backing behind its letters \
               and is drawn with one; the chat log asks for none anywhere in the shipped data and \
               is drawn with none. That is why one of them looked blurry and the other did not.",
        since: RETAIL,
        evidence: Evidence::Private("AC-EVID-O369"),
        station: "dereth-testkit::dat::ui::scenario_the_refusal_strip_is_drawn_with_an_outline_and_the_chat_log_is_not",
        tier: Tier::Dat,
    },
    behaviour! {
        id: "ui.text.outlined-text-submits-the-outline-pass-on-a-real-frame",
        says: "On a real frame, text whose font carries an outline sheet is drawn with an outline \
               pass as well as its letters, and there are never more outline passes than letter \
               passes.",
        since: RETAIL,
        evidence: Evidence::Private("AC-EVID-O369-TEXT"),
        station: "dereth-client::gpu::ui::text_outline_pass::a_real_frame_submits_the_outline_pass",
        tier: Tier::Gpu,
    },
    behaviour! {
        id: "ui.text.shift-arrow-extends-a-selection-and-a-bare-arrow-clears-it",
        says: "Holding Shift while pressing an arrow key in a text box grows the selection from \
               where the caret was, one character per press.",
        since: RETAIL,
        evidence: Evidence::Private("AC-EVID-O917-TEXT"),
        station: "dereth-ui::cpu::text::keyboard_selection::shift_arrow_extends_a_selection_from_the_old_caret",
        tier: Tier::Cpu,
    },
    behaviour! {
        id: "ui.text.shift-click-extends-the-selection",
        says: "A Shift-click in a text box extends the selection from where it began to the new \
               click, while a plain click starts a fresh, empty selection there.",
        since: RETAIL,
        evidence: Evidence::Private("AC-EVID-O91-TEXT"),
        station: "dereth-ui::cpu::text::drag_selection::a_shift_click_extends_the_selection_from_where_it_began",
        tier: Tier::Cpu,
    },
    behaviour! {
        id: "ui.visibility.hiding-a-window-moves-activation-to-the-next-visible-one",
        says: "Hiding the active window hands activation to the most recent window that is still \
               visible, after the hidden one has announced it is hidden and inactive; a window set \
               to activate when shown does so only when it really becomes visible, and showing an \
               already visible window changes nothing.",
        since: RETAIL,
        evidence: Evidence::Private("AC-EVID-ASTRA-VISIBILITY-ACTIVATION-VISIBILITY"),
        station: "dereth-ui::cpu::elements::visibility_and_activation::show_flag_raises_only_on_effective_edge_and_hide_falls_back_after_unregister",
        tier: Tier::Cpu,
    },
    behaviour! {
        id: "ui.window.a-border-drag-resizes-within-its-clamps-and-shift-snaps",
        says: "Dragging a window's border resizes it no smaller or larger than its own limits \
               allow, and a drag pushed past a limit and brought back lands where the pointer is \
               rather than where the limit stopped it.",
        since: RETAIL,
        evidence: Evidence::Private("AC-EVID-O200-WINDOW"),
        station: "dereth-ui::dat::border_resize::the_four_clamps_bound_the_drag_and_the_excess_is_not_lost",
        tier: Tier::Dat,
    },
    behaviour! {
        id: "urgent-assistance.send.the-refusal-that-applies-to-a-typed-command-does-not-apply-here",
        says: "The help channel is refused to a typed channel command, which is what makes asking \
               for help a command rather than a channel; the urgent-assistance window is the one \
               thing in the client that may send on it, and the same refusal is not applied to it.",
        since: RETAIL,
        evidence: Evidence::Private("AC-EVID-P1-173-NOT-REFUSED"),
        station: "dereth-testkit::dat::ui::scenario_the_refusal_for_a_typed_command_does_not_apply_to_the_window",
        tier: Tier::Dat,
    },
    behaviour! {
        id: "urgent-assistance.send.the-report-goes-out-on-the-help-channel",
        says: "The urgent-assistance window, walked the way a player walks it -- past the warning, \
               the report typed in, then send -- puts that report on the wire on the help channel, \
               as one message carrying the channel and the words and nothing else.",
        since: RETAIL,
        evidence: Evidence::Private("AC-EVID-P1-173-HELP"),
        station: "dereth-testkit::dat::ui::scenario_the_urgent_assistance_report_goes_out_on_the_help_channel",
        tier: Tier::Dat,
    },
    behaviour! {
        id: "urgent-assistance.window.walks-warning-form-confirmation-and-send-needs-text",
        says: "The urgent assistance window goes from its warning page to the form on Continue, \
               and a typed report and Send sends that text once on the help channel and moves on \
               to the confirmation page.",
        since: RETAIL,
        evidence: Evidence::Private("AC-EVID-P1150-WINDOW"),
        station: "dereth-ui-screens::dat::panels::urgent_assistance_window::the_wizard_walks_from_the_warning_to_the_form_to_the_confirmation",
        tier: Tier::Dat,
    },
];
