//! Allegiance and fellowship: what the shipped tabs draw when the shard names who you are with.
//!
//! Every scenario here opens the retail dats under `$DERETH_TEST_DAT_DIR`, and **this binary must run
//! serially**: two headless clients in one process share the UI request globals.
//!
//! `ALL` is this file's own list, concatenated with the other subjects' in `census.rs`, so a
//! scenario that is written and not listed shows up as a shortfall rather than as a silent gap.
//! The model-only social claims -- the tree walk and the talk-focus mask -- are in
//! `tests/cpu/social.rs`, because neither needs a layout.
//!
//! **The populated rosters here are synthesised and say so.** There is no recording of a character
//! inside an allegiance that has members; the one exception is the follower-count scenario, which
//! replays the recorded answer that does carry three.

use dereth_primitives::ObjectId;
use dereth_testkit::adapters_social::{allegiance_answer, allegiance_member};
use dereth_testkit::{ClientSpec, HeadlessClient, Inbound};
use dereth_ui::{ElemHandle, ElementId, UiSystem};
use dereth_ui_screens::panels::allegiance::{
    ID_EXPERIENCE_PASSED_UP, ID_FOLLOWERS, MONARCH_FOLLOWERS, MONARCH_PATRON_BLOCK,
    PLAYER_FOLLOWERS, ROW_NAME, STRING_TABLE, VASSAL_LIST,
};
use dereth_ui_screens::screens::gameplay::GamePlayScreen;

/// Every scenario in this file, for the census: the name, **the behaviour ids the
/// scenario asserts**, and the function.
pub static ALL: &[dereth_testkit::behaviours::Scenario] = &[
    (
        "the_allegiance_tab_is_bound_and_empty_without_an_allegiance",
        &["allegiance.panel.the-shipped-vassal-list-is-bound-and-empty-without-an-allegiance"],
        the_allegiance_tab_is_bound_and_empty_without_an_allegiance,
    ),
    (
        "a_roster_fills_the_shipped_vassal_list",
        &["allegiance.panel.a-roster-fills-the-shipped-vassal-list-and-is-replaced-whole"],
        a_roster_fills_the_shipped_vassal_list,
    ),
    (
        "an_online_relative_opens_that_allegiance_channel",
        &["allegiance.channels.an-online-patron-monarch-or-vassal-opens-that-channel"],
        an_online_relative_opens_that_allegiance_channel,
    ),
    (
        "a_monarch_is_shown_no_monarch_and_no_patron_row",
        &["allegiance.panel.a-monarch-has-no-monarch-row-and-no-patron-row"],
        a_monarch_is_shown_no_monarch_and_no_patron_row,
    ),
    (
        "the_patron_row_shows_the_players_own_tithed_experience",
        &["allegiance.panel.the-visible-patron-row-shows-the-players-own-tithed-experience"],
        the_patron_row_shows_the_players_own_tithed_experience,
    ),
    (
        "a_member_with_no_title_for_his_sex_is_drawn_by_name",
        &["allegiance.panel.a-member-whose-gender-carries-no-title-is-drawn-by-bare-name"],
        a_member_with_no_title_for_his_sex_is_drawn_by_name,
    ),
    (
        "both_followers_numbers_come_from_the_answers_header",
        &["allegiance.panel.the-followers-numbers-come-from-the-profile-header"],
        both_followers_numbers_come_from_the_answers_header,
    ),
    (
        "a_login_notification_leaves_the_tab_alone",
        &["allegiance.roster.a-login-notification-redraws-nothing"],
        a_login_notification_leaves_the_tab_alone,
    ),
    (
        "opening_the_tab_asks_the_shard_and_closing_it_unasks",
        &["allegiance.subscription.the-tab-asks-the-shard-on-the-way-up-and-unasks-on-the-way-down"],
        opening_the_tab_asks_the_shard_and_closing_it_unasks,
    ),
    (
        "a_tab_left_open_asks_once_and_not_once_a_frame",
        &["allegiance.subscription.is-an-edge-and-not-a-poll"],
        a_tab_left_open_asks_once_and_not_once_a_frame,
    ),
    (
        "the_opening_ask_is_withheld_without_a_player",
        &["allegiance.subscription.the-opening-ask-is-withheld-without-a-player"],
        the_opening_ask_is_withheld_without_a_player,
    ),
    (
        "the_answer_to_the_ask_fills_the_tab",
        &["allegiance.subscription.the-answer-fills-the-tab-and-provokes-no-second-ask"],
        the_answer_to_the_ask_fills_the_tab,
    ),
    (
        "the_roster_the_shard_really_sent_reaches_the_list",
        &["allegiance.panel.the-roster-the-shard-really-sent-reaches-the-list"],
        the_roster_the_shard_really_sent_reaches_the_list,
    ),
    (
        "breaking_asks_a_question_and_sends_nothing",
        &["allegiance.buttons.breaking-asks-a-question-and-sends-nothing-by-itself"],
        breaking_asks_a_question_and_sends_nothing,
    ),
    (
        "picking_a_vassal_arms_the_kick_button",
        &["allegiance.buttons.picking-a-vassal-arms-the-kick-button-from-the-rows-own-id"],
        picking_a_vassal_arms_the_kick_button,
    ),
    (
        "swear_is_dark_while_the_player_has_a_patron",
        &["allegiance.buttons.swear-is-dark-while-the-player-already-has-a-patron"],
        swear_is_dark_while_the_player_has_a_patron,
    ),
    (
        "every_typed_allegiance_command_sends_its_own_message",
        &["allegiance.commands.every-typed-allegiance-command-sends-its-own-message"],
        every_typed_allegiance_command_sends_its_own_message,
    ),
    (
        "a_refused_allegiance_line_prints_and_sends_nothing",
        &["allegiance.commands.a-refused-line-is-printed-where-the-player-reads-it-and-sends-nothing"],
        a_refused_allegiance_line_prints_and_sends_nothing,
    ),
    (
        "listening_is_an_option_and_broadcasting_is_a_channel",
        &["allegiance.commands.listening-is-a-local-option-and-broadcasting-is-a-channel"],
        listening_is_an_option_and_broadcasting_is_a_channel,
    ),
    (
        "the_fellowship_lines_reach_the_live_chat_log",
        &["fellowship.lines.reach-the-live-chat-log-in-the-colour-of-a-broadcast"],
        the_fellowship_lines_reach_the_live_chat_log,
    ),
    (
        "the_social_page_opening_is_not_the_fellowship_tab_opening",
        &["fellowship.subscription.the-social-page-opening-is-not-the-tab-opening"],
        the_social_page_opening_is_not_the_fellowship_tab_opening,
    ),
    (
        "a_membership_list_draws_a_roster_with_names_stats_and_meters",
        &["fellowship.panel.a-membership-list-draws-a-roster-with-names-stats-and-meters"],
        a_membership_list_draws_a_roster_with_names_stats_and_meters,
    ),
    (
        "disband_is_offered_only_to_the_leader",
        &["fellowship.buttons.disband-is-offered-only-to-the-leader"],
        disband_is_offered_only_to_the_leader,
    ),
    (
        "a_leader_who_leaves_hands_the_lead_on_first",
        &["fellowship.buttons.a-leader-who-leaves-hands-the-lead-on-first"],
        a_leader_who_leaves_hands_the_lead_on_first,
    ),
    (
        "opening_the_fellowship_changes_the_tab_first",
        &["fellowship.buttons.opening-the-fellowship-changes-the-tab-before-the-shard-answers"],
        opening_the_fellowship_changes_the_tab_first,
    ),
    (
        "picking_a_row_then_dismissing_names_that_fellow",
        &["fellowship.buttons.picking-a-row-then-dismissing-names-that-fellow"],
        picking_a_row_then_dismissing_names_that_fellow,
    ),
    (
        "recruit_follows_the_world_selection",
        &["fellowship.buttons.recruit-follows-the-world-selection-and-not-the-list"],
        recruit_follows_the_world_selection,
    ),
    (
        "the_name_box_gates_create_and_the_tick_box_is_the_source",
        &["fellowship.create.the-name-box-gates-the-button-and-the-tick-box-is-the-source"],
        the_name_box_gates_create_and_the_tick_box_is_the_source,
    ),
    (
        "the_friends_tab_lists_what_the_shard_sent_it",
        &["friends.panel.the-tab-lists-the-friends-the-shard-sent-with-the-online-ones-first"],
        the_friends_tab_lists_what_the_shard_sent_it,
    ),
    (
        "each_kind_of_friends_change_moves_the_list",
        &["friends.update.each-kind-of-change-moves-the-list-on-its-own"],
        each_kind_of_friends_change_moves_the_list,
    ),
    (
        "picking_a_friend_arms_remove_and_the_press_names_him",
        &["friends.buttons.picking-a-row-arms-remove-and-the-press-names-that-friend"],
        picking_a_friend_arms_remove_and_the_press_names_him,
    ),
    (
        "tell_follows_whether_the_friend_is_online",
        &["friends.buttons.tell-follows-whether-the-friend-is-online"],
        tell_follows_whether_the_friend_is_online,
    ),
    (
        "typing_a_name_arms_add_and_the_press_sends_it_alone",
        &["friends.buttons.typing-a-name-arms-add-and-the-press-sends-the-name-alone"],
        typing_a_name_arms_add_and_the_press_sends_it_alone,
    ),
];

/// Run one of this file's scenarios under a recorder, and check that the behaviour ids it
/// asserted are exactly the ones its [`ALL`] entry declares.
fn scenario(name: &str) {
    dereth_testkit::behaviours::run_scenario(ALL, name);
}

const ME: ObjectId = ObjectId(0x5000_0001);
const BOB: ObjectId = ObjectId(0x5000_0009);
const CID: ObjectId = ObjectId(0x5000_000B);
const DEE: ObjectId = ObjectId(0x5000_0010);
const EVE: ObjectId = ObjectId(0x5000_0011);
const FAY: ObjectId = ObjectId(0x5000_0020);

/// The experience line's own element inside a patron row.
const EXPERIENCE_PASSED_UP: ElementId = ElementId(0x1000_0492);

// ---------------------------------------------------------------------------------------------
// Reaching the shipped tree
// ---------------------------------------------------------------------------------------------

/// A client in the world with the shipped gameplay screen up and `ME` as the player.
///
/// The player id has to be set before the answer arrives: every walk is keyed on it, and a client
/// that had none would read the roster as somebody else's.
fn a_client_in_gameplay() -> HeadlessClient {
    let mut c = HeadlessClient::new(ClientSpec::gameplay(4));
    assert!(
        c.world_mut().set_player(ME),
        "the scenario's own player id takes"
    );
    c
}

/// Run `f` against the shipped gameplay screen's root and the shell it was laid out in.
///
/// The same shape `chat.rs` uses: a scenario reaching the live client can borrow one of
/// the shell's halves at a time, and reading a text element needs the mutable one because the
/// glyphs are laid out on demand.
fn on_the_shipped_tree<T>(
    c: &mut HeadlessClient,
    f: impl FnOnce(&mut UiSystem, ElemHandle) -> T,
) -> T {
    let app = c.app_mut();
    let shell = app.ui_mut().expect("the UI shell is up");
    let root = {
        let screen = shell.flow.current_mut().expect("a screen is up");
        let any: &mut dyn std::any::Any = &mut **screen;
        any.downcast_mut::<GamePlayScreen>()
            .expect("the gameplay screen")
            .root()
            .expect("the gameplay screen's root")
    };
    f(&mut shell.ui, root)
}

/// The text of `ROW_NAME` in each live row of the vassal list, in list order.
fn vassal_row_names(c: &mut HeadlessClient) -> Vec<String> {
    on_the_shipped_tree(c, |ui, root| {
        let list = ui
            .get_child_recursive(root, VASSAL_LIST)
            .expect("the shipped vassal list");
        let texts: Vec<ElemHandle> = ui
            .children(list)
            .into_iter()
            .filter_map(|row| ui.get_child_recursive(row, ROW_NAME))
            .collect();
        texts
            .into_iter()
            .filter_map(|t| ui.text_element_mut(t).map(|t| t.glyphs.inq_text(false)))
            .collect()
    })
}

/// The drawn text of one element of the shipped tree, found by id.
fn text_of(c: &mut HeadlessClient, id: ElementId) -> String {
    on_the_shipped_tree(c, |ui, root| {
        let h = ui
            .get_child_recursive(root, id)
            .expect("the shipped element");
        ui.text_element_mut(h)
            .map_or_else(String::new, |t| t.glyphs.inq_text(false))
    })
}

/// A localisation token's hash, through the one function in the tree that computes it. The rows
/// below are looked up by it, which is how the client itself finds them.
fn token(name: &str) -> u32 {
    dereth_ui::persist::preferences::token_of(name)
}

/// The shipped "followers" row with `n` put between its literal pieces.
///
/// Resolved here rather than taken from the panel, so that the expectation is the shipped string
/// table and not the code being asserted over.
fn followers_text(c: &mut HeadlessClient, n: u32) -> String {
    on_the_shipped_tree(c, |ui, _| {
        let parts = ui
            .resolve_string_variants(STRING_TABLE, token(ID_FOLLOWERS))
            .expect("the shipped followers row resolves");
        assert_eq!(parts.len(), 2, "one variable in the shipped row: {parts:?}");
        format!("{}{n}{}", parts[0], parts[1])
    })
}

/// The shipped "experience passed up" row with `value` in it.
fn experience_text(c: &mut HeadlessClient, value: &str) -> String {
    on_the_shipped_tree(c, |ui, _| {
        ui.resolve_string_rendered(
            STRING_TABLE,
            token(ID_EXPERIENCE_PASSED_UP),
            &[value.to_owned()],
        )
        .expect("the shipped experience row resolves")
    })
}

/// Deliver `answer` and run the frames the tab rebuilds on.
fn answer(c: &mut HeadlessClient, a: &dereth_protocol::social::AllegianceUpdate) {
    c.when(Inbound::message(a));
    c.tick(3);
}

/// Whether each of the three allegiance channels is open.
fn channels(c: &HeadlessClient) -> (bool, bool, bool) {
    use dereth_client_model::chat::TalkFocus;
    let chat = &c.view().world().chat;
    (
        chat.is_talk_focus_enabled(TalkFocus::Patron),
        chat.is_talk_focus_enabled(TalkFocus::Monarch),
        chat.is_talk_focus_enabled(TalkFocus::Vassals),
    )
}

// =============================================================================================
// 1. allegiance.panel.the-shipped-vassal-list-is-bound-and-empty-without-an-allegiance
// =============================================================================================

/// The tab is in the shipped tree, it is driven on an ordinary frame, and a character in no
/// allegiance is shown an empty list -- which is a different reading from a tab that never ran.
pub fn the_allegiance_tab_is_bound_and_empty_without_an_allegiance() {
    let mut c = a_client_in_gameplay();
    let (bound, driven, no_rows, channel_shut) = {
        let p = &c.view().expect_app().hud().panels.allegiance;
        (
            p.bound(),
            p.rebuilds >= 1,
            p.rows().is_empty(),
            !p.vassal_chat_enabled,
        )
    };
    // And the tree agrees: the list exists and holds nothing. A tab that computed the right rows
    // and wrote none would pass everything above.
    let drawn = vassal_row_names(&mut c);

    c.assert_behaviour(
        "allegiance.panel.the-shipped-vassal-list-is-bound-and-empty-without-an-allegiance",
        move |_| bound && driven && no_rows && channel_shut && drawn.is_empty(),
    );
    c.shutdown();
}

#[test]
fn scenario_the_allegiance_tab_is_bound_and_empty_without_an_allegiance() {
    scenario("the_allegiance_tab_is_bound_and_empty_without_an_allegiance");
}

// =============================================================================================
// 2. allegiance.panel.a-roster-fills-the-shipped-vassal-list-and-is-replaced-whole
// =============================================================================================

/// The same tree the model-tier walk scenario uses: a monarch, a patron, the player, two vassals of
/// his and one of theirs. **Synthesised**; see the module note.
fn a_populated_allegiance() -> dereth_protocol::social::AllegianceUpdate {
    allegiance_answer(
        "The Hand of Dereth",
        6,
        2,
        vec![
            (None, allegiance_member(BOB, "Bob", 9, true)),
            (Some(BOB), allegiance_member(CID, "Cid", 5, false)),
            (Some(CID), allegiance_member(ME, "Lark", 3, true)),
            (Some(ME), allegiance_member(DEE, "Dee", 1, true)),
            (Some(ME), allegiance_member(EVE, "Eve", 1, false)),
            (Some(DEE), allegiance_member(FAY, "Fay", 1, true)),
        ],
    )
}

/// The vassals reach the shipped list, newest first and titled; a repeat redraws nothing; and
/// leaving the allegiance takes the rows away rather than leaving them behind.
pub fn a_roster_fills_the_shipped_vassal_list() {
    let mut c = a_client_in_gameplay();
    let nothing_first = vassal_row_names(&mut c).is_empty();

    answer(&mut c, &a_populated_allegiance());
    let drawn = vassal_row_names(&mut c);
    let (rows, rebuilds, away, channel) = {
        let p = &c.view().expect_app().hud().panels.allegiance;
        (
            p.rows()
                .iter()
                .map(|r| (r.id, r.name.clone()))
                .collect::<Vec<_>>(),
            p.rebuilds,
            p.rows()
                .iter()
                .map(|r| r.logged_out_marker)
                .collect::<Vec<_>>(),
            p.vassal_chat_enabled,
        )
    };
    let in_the_model =
        rows == vec![
            (EVE, "Yeoman Eve".to_owned()),
            (DEE, "Yeoman Dee".to_owned()),
        ] && away == vec![true, false]
            && channel;

    // The same roster again: the tab's own guard refuses to rebuild, and the list is not appended
    // to. `rebuilds` is what separates "ran and found the same thing" from "never ran".
    answer(&mut c, &a_populated_allegiance());
    let unchanged = c.view().expect_app().hud().panels.allegiance.rebuilds == rebuilds
        && vassal_row_names(&mut c) == drawn;

    // Leaving: the update is a full replace, not a merge.
    answer(&mut c, &allegiance_answer("", 0, 0, Vec::new()));
    let emptied = vassal_row_names(&mut c).is_empty()
        && c.view()
            .expect_app()
            .hud()
            .panels
            .allegiance
            .rows()
            .is_empty();

    c.assert_behaviour(
        "allegiance.panel.a-roster-fills-the-shipped-vassal-list-and-is-replaced-whole",
        move |_| {
            nothing_first
                && drawn == vec!["Yeoman Eve".to_owned(), "Yeoman Dee".to_owned()]
                && in_the_model
                && unchanged
                && emptied
        },
    );
    c.shutdown();
}

#[test]
fn scenario_a_roster_fills_the_shipped_vassal_list() {
    scenario("a_roster_fills_the_shipped_vassal_list");
}

// =============================================================================================
// 3. allegiance.channels.an-online-patron-monarch-or-vassal-opens-that-channel
// =============================================================================================

/// Who is online decides which of the three allegiance channels the player may pick.
pub fn an_online_relative_opens_that_allegiance_channel() {
    let mut c = a_client_in_gameplay();
    let shut_to_start = channels(&c) == (false, false, false);

    // Bob is monarch and the player's patron, and he is offline; Cid is the player's vassal and is
    // online. Only the vassal channel opens.
    answer(
        &mut c,
        &allegiance_answer(
            "The Hand of Dereth",
            3,
            1,
            vec![
                (None, allegiance_member(BOB, "Bob", 5, false)),
                (Some(BOB), allegiance_member(ME, "Lark", 3, true)),
                (Some(ME), allegiance_member(CID, "Cid", 1, true)),
            ],
        ),
    );
    let only_vassals = channels(&c) == (false, false, true);

    // Bob logs in: he is the patron and the monarch, and he is not the player, so both open.
    answer(
        &mut c,
        &allegiance_answer(
            "The Hand of Dereth",
            3,
            1,
            vec![
                (None, allegiance_member(BOB, "Bob", 5, true)),
                (Some(BOB), allegiance_member(ME, "Lark", 3, true)),
                (Some(ME), allegiance_member(CID, "Cid", 1, true)),
            ],
        ),
    );
    let all_three = channels(&c) == (true, true, true);

    // Everybody logs out again, and all three close through the same path.
    answer(
        &mut c,
        &allegiance_answer(
            "The Hand of Dereth",
            3,
            1,
            vec![
                (None, allegiance_member(BOB, "Bob", 5, false)),
                (Some(BOB), allegiance_member(ME, "Lark", 3, true)),
                (Some(ME), allegiance_member(CID, "Cid", 1, false)),
            ],
        ),
    );
    let shut_again = channels(&c) == (false, false, false);

    c.assert_behaviour(
        "allegiance.channels.an-online-patron-monarch-or-vassal-opens-that-channel",
        move |_| shut_to_start && only_vassals && all_three && shut_again,
    );
    c.shutdown();
}

#[test]
fn scenario_an_online_relative_opens_that_allegiance_channel() {
    scenario("an_online_relative_opens_that_allegiance_channel");
}

// =============================================================================================
// 4. allegiance.panel.a-monarch-has-no-monarch-row-and-no-patron-row
// =============================================================================================

/// The head of his own allegiance is his own monarch and has no patron, so both rows are hidden and
/// the monarch channel stays shut however many members are online.
pub fn a_monarch_is_shown_no_monarch_and_no_patron_row() {
    let mut c = a_client_in_gameplay();
    answer(
        &mut c,
        &allegiance_answer(
            "The Hand of Dereth",
            2,
            1,
            vec![
                (None, allegiance_member(ME, "Lark", 9, true)),
                (Some(ME), allegiance_member(CID, "Cid", 1, true)),
            ],
        ),
    );

    let roster = {
        use dereth_ui_screens::view::GameView as _;
        let v = c.view();
        let app = v.expect_app();
        app.hud().view(app.objects()).allegiance_roster()
    };
    let walked = roster.subject.as_ref().map(|s| s.id) == Some(ME)
        && roster.monarch.as_ref().map(|m| m.id) == Some(ME)
        && roster.patron.is_none();
    // Only the vassal channel: the monarch is the player himself, so that row is never offered.
    let only_vassals = channels(&c) == (false, false, true);

    let (monarch_field, patron_field) = {
        let p = &c.view().expect_app().hud().panels.allegiance;
        (
            p.monarch_field.expect("bound"),
            p.patron_field.expect("bound"),
        )
    };
    let hidden = on_the_shipped_tree(&mut c, |ui, _| {
        let visible = |h: ElemHandle| ui.node(h).expect("alive").region.flags.visible;
        !visible(monarch_field) && !visible(patron_field)
    });

    c.assert_behaviour(
        "allegiance.panel.a-monarch-has-no-monarch-row-and-no-patron-row",
        move |_| walked && only_vassals && hidden,
    );
    c.shutdown();
}

#[test]
fn scenario_a_monarch_is_shown_no_monarch_and_no_patron_row() {
    scenario("a_monarch_is_shown_no_monarch_and_no_patron_row");
}

// =============================================================================================
// 5. allegiance.panel.the-visible-patron-row-shows-the-players-own-tithed-experience
// =============================================================================================

/// One member, with `cp_tithed` set -- the number the patron row prints.
fn tithed(
    id: ObjectId,
    name: &str,
    rank: u32,
    cp_tithed: u32,
) -> dereth_protocol::social::AllegianceData {
    dereth_protocol::social::AllegianceData {
        cp_tithed,
        ..allegiance_member(id, name, rank, true)
    }
}

/// The experience line is the player's own, wherever the row is drawn; nought is written out; and
/// with no patron the whole row stays hidden.
pub fn the_patron_row_shows_the_players_own_tithed_experience() {
    const EXPERIENCE: u32 = 1_234_567;
    let mut c = a_client_in_gameplay();
    let expected = experience_text(&mut c, "1,234,567");

    // A patron who is not the monarch: the distinct patron row is drawn, and its experience line is
    // the player's own tithe and not the patron's cached experience.
    answer(
        &mut c,
        &allegiance_answer(
            "The Hand of Dereth",
            3,
            0,
            vec![
                (None, allegiance_member(BOB, "Bob", 9, true)),
                (Some(BOB), allegiance_member(CID, "Cid", 5, true)),
                (Some(CID), tithed(ME, "Lark", 3, EXPERIENCE)),
            ],
        ),
    );
    let patron_field = c
        .view()
        .expect_app()
        .hud()
        .panels
        .allegiance
        .patron_field
        .expect("bound");
    let in_the_patron_row = on_the_shipped_tree(&mut c, |ui, _| {
        let h = ui
            .get_child_recursive(patron_field, EXPERIENCE_PASSED_UP)
            .expect("the line");
        ui.text_element_mut(h)
            .map_or_else(String::new, |t| t.glyphs.inq_text(false))
    });

    // The patron is the monarch: the same own-tithe line moves into the combined row above.
    answer(
        &mut c,
        &allegiance_answer(
            "The Hand of Dereth",
            2,
            0,
            vec![
                (None, allegiance_member(BOB, "Bob", 9, true)),
                (Some(BOB), tithed(ME, "Lark", 3, EXPERIENCE)),
            ],
        ),
    );
    let monarch_field = c
        .view()
        .expect_app()
        .hud()
        .panels
        .allegiance
        .monarch_field
        .expect("bound");
    let combined = |c: &mut HeadlessClient| {
        on_the_shipped_tree(c, |ui, _| {
            let block = ui
                .get_child_recursive(monarch_field, MONARCH_PATRON_BLOCK)
                .expect("the block");
            let h = ui
                .get_child_recursive(block, EXPERIENCE_PASSED_UP)
                .expect("the line");
            ui.text_element_mut(h)
                .map_or_else(String::new, |t| t.glyphs.inq_text(false))
        })
    };
    let in_the_combined_row = combined(&mut c);

    // Nought is a number, not a missing value.
    answer(
        &mut c,
        &allegiance_answer(
            "The Hand of Dereth",
            2,
            0,
            vec![
                (None, allegiance_member(BOB, "Bob", 9, true)),
                (Some(BOB), tithed(ME, "Lark", 3, 0)),
            ],
        ),
    );
    let zero_text = experience_text(&mut c, "0");
    let nought_is_drawn = combined(&mut c) == zero_text;

    // A player with no patron at all: the row stays hidden rather than being given a line about
    // nobody. A second client, because a tab that has drawn a patron cannot un-draw one.
    let mut alone = a_client_in_gameplay();
    answer(
        &mut alone,
        &allegiance_answer(
            "The Hand of Dereth",
            1,
            0,
            vec![(None, tithed(ME, "Lark", 9, EXPERIENCE))],
        ),
    );
    let field = alone
        .view()
        .expect_app()
        .hud()
        .panels
        .allegiance
        .patron_field
        .expect("bound");
    let row_hidden = on_the_shipped_tree(&mut alone, |ui, _| {
        !ui.node(field).expect("alive").region.flags.visible
    });
    alone.shutdown();

    c.assert_behaviour(
        "allegiance.panel.the-visible-patron-row-shows-the-players-own-tithed-experience",
        move |_| {
            expected == "1,234,567"
                && in_the_patron_row == expected
                && in_the_combined_row == expected
                && nought_is_drawn
                && row_hidden
        },
    );
    c.shutdown();
}

#[test]
fn scenario_the_patron_row_shows_the_players_own_tithed_experience() {
    scenario("the_patron_row_shows_the_players_own_tithed_experience");
}

// =============================================================================================
// 6. allegiance.panel.a-member-whose-gender-carries-no-title-is-drawn-by-bare-name
// =============================================================================================

/// The rank title is chosen by the member's sex, and a sex the titles are not written for leaves
/// the bare name rather than inventing one.
pub fn a_member_with_no_title_for_his_sex_is_drawn_by_name() {
    let mut c = a_client_in_gameplay();
    let mut drawn: Vec<(u32, String, u8)> = Vec::new();
    for gender in [0_u32, 3, 255, 1, 2] {
        let monarch = dereth_protocol::social::AllegianceData {
            gender,
            ..allegiance_member(BOB, "Bob", 9, true)
        };
        answer(
            &mut c,
            &allegiance_answer(
                "The Hand of Dereth",
                2,
                0,
                vec![
                    (None, monarch),
                    (Some(BOB), allegiance_member(ME, "Lark", 3, true)),
                ],
            ),
        );
        let reached = c
            .view()
            .world()
            .allegiance
            .look_up(BOB)
            .expect("the encoded member reached the model")
            .gender;
        let name = c
            .view()
            .expect_app()
            .hud()
            .panels
            .allegiance
            .monarch_name
            .expect("bound");
        let text = on_the_shipped_tree(&mut c, |ui, _| {
            ui.text_element_mut(name)
                .map_or_else(String::new, |t| t.glyphs.inq_text(false))
        });
        drawn.push((gender, text, reached));
    }

    c.assert_behaviour(
        "allegiance.panel.a-member-whose-gender-carries-no-title-is-drawn-by-bare-name",
        move |_| {
            drawn
                == vec![
                    (0, "Bob".to_owned(), 0),
                    (3, "Bob".to_owned(), 3),
                    (255, "Bob".to_owned(), 255),
                    (1, "King Bob".to_owned(), 1),
                    (2, "Queen Bob".to_owned(), 2),
                ]
        },
    );
    c.shutdown();
}

#[test]
fn scenario_a_member_with_no_title_for_his_sex_is_drawn_by_name() {
    scenario("a_member_with_no_title_for_his_sex_is_drawn_by_name");
}

// =============================================================================================
// 7. allegiance.panel.the-followers-numbers-come-from-the-profile-header
// 8. allegiance.roster.a-login-notification-redraws-nothing
// =============================================================================================

/// The one **recorded** roster with members in it. Its header and its record count differ from each
/// other and from the two numbers the tab draws, which is what makes it the oracle: a tab that
/// printed the record count would agree with the header on any other roster.
const ROSTER_SESSION: &str = "fellowship-one-vassal";
const ROSTER_BLOB: usize = 351;

/// The recipient the recorded answer was addressed to, and the answer's own header.
///
/// Read off the corpus rather than pinned: the blob is the shard's, and the two expectations below
/// are computed from it.
fn recorded_roster() -> (ObjectId, u32, u32, usize) {
    use dereth_client_net::client_session::testing::{Corpus, Direction};
    use dereth_protocol::Message as _;

    let corpus = Corpus::load(ROSTER_SESSION)
        .expect("the locked corpus decodes")
        .expect("the corpus holds the recording");
    let row = corpus
        .blobs
        .into_iter()
        .find(|b| b.idx == ROSTER_BLOB)
        .expect("the recorded roster is in the corpus");
    assert_eq!(row.dir, Direction::ServerToClient);
    let player = ObjectId(u32::from_le_bytes(
        row.payload[4..8].try_into().expect("four bytes"),
    ));
    let mut r = dereth_protocol::archive::Reader::new(&row.payload[16..]);
    let m = dereth_protocol::social::AllegianceUpdate::read(&mut r)
        .expect("the recorded answer decodes");
    let p = &m.profile;
    (
        player,
        p.total_members,
        p.total_vassals,
        p.hierarchy.members.len(),
    )
}

/// A client set up as the character the recorded answer was addressed to, with that answer applied.
fn the_recorded_roster() -> (HeadlessClient, u32, u32, usize) {
    let (player, total_members, total_vassals, records) = recorded_roster();
    let mut c = HeadlessClient::new(ClientSpec::gameplay(4));
    assert!(
        c.world_mut().set_player(player),
        "the recorded recipient is this client's player"
    );
    c.when(Inbound::from_corpus(
        ROSTER_SESSION,
        ROSTER_BLOB..ROSTER_BLOB + 1,
    ));
    c.tick(3);
    (c, total_members, total_vassals, records)
}

/// Each followers box is a header field of the answer, and neither is the number of records it
/// carried.
pub fn both_followers_numbers_come_from_the_answers_header() {
    let (mut c, total_members, total_vassals, records) = the_recorded_roster();
    // The premise, restated so that a re-promoted corpus cannot make this pass by accident: all
    // three numbers differ, so the two formulas and the record count are distinguishable.
    let premise = total_members != total_vassals
        && u32::try_from(records).expect("small") != total_vassals
        && u32::try_from(records).expect("small") != total_members - 1;

    let driven = {
        let p = &c.view().expect_app().hud().panels.allegiance;
        p.rebuilds > 0 && p.rows().len() == 1
    };

    let player_text = text_of(&mut c, PLAYER_FOLLOWERS);
    let monarch_text = text_of(&mut c, MONARCH_FOLLOWERS);
    let want_player = followers_text(&mut c, total_vassals);
    let want_monarch = followers_text(&mut c, total_members - 1);

    c.assert_behaviour(
        "allegiance.panel.the-followers-numbers-come-from-the-profile-header",
        move |_| premise && driven && player_text == want_player && monarch_text == want_monarch,
    );
    c.shutdown();
}

#[test]
fn scenario_both_followers_numbers_come_from_the_answers_header() {
    scenario("both_followers_numbers_come_from_the_answers_header");
}

/// Being told a member logged in or out leaves the tab exactly as the last full answer left it.
pub fn a_login_notification_leaves_the_tab_alone() {
    let (mut c, _, _, _) = the_recorded_roster();
    let before = (
        text_of(&mut c, PLAYER_FOLLOWERS),
        text_of(&mut c, MONARCH_FOLLOWERS),
    );
    let rebuilds = c.view().expect_app().hud().panels.allegiance.rebuilds;
    let rows = c.view().expect_app().hud().panels.allegiance.rows().len();

    // The vassal the recorded roster carries, logging out and then in again -- the two shapes the
    // notification takes.
    let vassal = c.view().expect_app().hud().panels.allegiance.rows()[0].id;
    for now_logged_in in [0_i32, 1] {
        c.when(Inbound::message(
            &dereth_protocol::social::AllegianceLoginNotification {
                member: vassal,
                now_logged_in,
            },
        ));
        c.tick(2);
    }

    let after = (
        text_of(&mut c, PLAYER_FOLLOWERS),
        text_of(&mut c, MONARCH_FOLLOWERS),
    );
    let still = c.view().expect_app().hud().panels.allegiance.rebuilds == rebuilds
        && c.view().expect_app().hud().panels.allegiance.rows().len() == rows;

    c.assert_behaviour(
        "allegiance.roster.a-login-notification-redraws-nothing",
        move |_| after == before && still,
    );
    c.shutdown();
}

#[test]
fn scenario_a_login_notification_leaves_the_tab_alone() {
    scenario("a_login_notification_leaves_the_tab_alone");
}

// =============================================================================================
// 9-12. allegiance.subscription.*
// =============================================================================================

/// The exact bodies the recordings carry, opcode dword included. Every subscribe in the three
/// recorded sessions is byte-identical, and so is every unsubscribe; they are stated here as
/// literals because a test that reads a constant back through the symbol that wrote it cannot
/// detect a wrong constant.
const SUBSCRIBE: [u8; 8] = [0x1F, 0x00, 0x00, 0x00, 0x01, 0x00, 0x00, 0x00];
const UNSUBSCRIBE: [u8; 8] = [0x1F, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00];

/// The page of the shipped toolbar the allegiance tab lives on, by the panel id its button carries.
const SOCIAL_PANEL_ID: u32 = 0x0C;

/// A client whose shell is up and whose gameplay screen has **not** been settled yet, so that the
/// frame the tab is built on is one this scenario runs and can watch.
fn a_client_before_the_first_frame(with_player: bool) -> HeadlessClient {
    let spec = ClientSpec {
        shell: true,
        ui_mode: Some(dereth_ui::framework::mode::GAME_PLAY),
        settle_frames: 0,
        ..ClientSpec::retail()
    };
    let mut c = HeadlessClient::new(spec);
    if with_player {
        assert!(c.world_mut().set_player(ME), "the player id takes");
    }
    c
}

/// Every allegiance subscription the client put in its outbox after `mark`, as the bytes that would
/// go on the wire. **They are not sent**: this is the production writer, not a transport.
fn subscriptions_since(c: &HeadlessClient, from: usize) -> Vec<Vec<u8>> {
    use dereth_protocol::Message as _;
    c.view().outbound()[from..]
        .iter()
        .filter_map(|r| match r {
            dereth_client_model::Request::AllegianceUpdateRequest(m) => {
                let mut blob = dereth_protocol::social::AllegianceUpdateRequest::OPCODE
                    .0
                    .to_le_bytes()
                    .to_vec();
                blob.extend(dereth_protocol::write_body(m).expect("the body encodes"));
                Some(blob)
            }
            _ => None,
        })
        .collect()
}

/// How many requests the client has produced so far, to count the next window from.
fn mark(c: &HeadlessClient) -> usize {
    c.view().outbound().len()
}

/// The toolbar button that opens the social page, found by the panel id it carries rather than
/// paired by name.
fn social_button(c: &mut HeadlessClient) -> ElemHandle {
    let app = c.app_mut();
    let shell = app.ui_mut().expect("the UI shell is up");
    let screen = shell.flow.current_mut().expect("a screen is up");
    let any: &mut dyn std::any::Any = &mut **screen;
    any.downcast_mut::<GamePlayScreen>()
        .expect("the gameplay screen")
        .toolbar
        .buttons
        .iter()
        .find(|b| b.panel_id == SOCIAL_PANEL_ID)
        .expect("one of the shipped toolbar's panel buttons opens the social page")
        .handle
}

/// A real click on an element: the pointer moves on to it, presses and releases, with the client's
/// own frame between the edges so that the screen sees each one as it would from a player's hand.
fn click(c: &mut HeadlessClient, h: ElemHandle) {
    let (x, y) = on_the_shipped_tree(c, |ui, _| {
        let (ox, oy) = ui.screen_origin(h);
        let b = ui.node(h).expect("the button is alive").region.box_;
        (ox + b.width() / 2, oy + b.height() / 2)
    });
    #[allow(clippy::cast_precision_loss)]
    let now = dereth_primitives::LocalTime(c.frames() as f64);
    on_the_shipped_tree(c, |ui, _| ui.mouse_move(now, x, y));
    c.tick(1);
    on_the_shipped_tree(c, |ui, _| {
        ui.mouse_down(dereth_ui::focus::action::PRIMARY_CLICK, x, y)
    });
    c.tick(1);
    on_the_shipped_tree(c, |ui, _| {
        ui.mouse_up(dereth_ui::focus::action::PRIMARY_CLICK, x, y, false);
    });
    c.tick(1);
}

/// Whether the allegiance tab itself is up, by the whole parent chain and not just its own flag.
fn tab_visible(c: &HeadlessClient) -> bool {
    let app = c.view().expect_app();
    let h = app
        .hud()
        .panels
        .allegiance
        .panel
        .expect("the tab is in the shipped tree");
    app.ui().expect("the shell is up").ui.is_visible(h)
}

/// The tab asks the shard for the roster as it comes up and unasks as it goes down, with the bytes
/// the recordings carry.
pub fn opening_the_tab_asks_the_shard_and_closing_it_unasks() {
    let mut c = a_client_before_the_first_frame(true);
    let start = mark(&c);
    c.tick(1);
    // The pair every recorded session opens with: the tab binding itself, and the player's own
    // description arriving. Neither is a tab opening -- the tab is still down.
    let bring_up = subscriptions_since(&c, start);
    let down_to_start = !tab_visible(&c);

    let button = social_button(&mut c);
    let before_click = mark(&c);
    click(&mut c, button);
    let opened = tab_visible(&c);
    let on_opening = subscriptions_since(&c, before_click);

    let before_second = mark(&c);
    click(&mut c, button);
    let closed = !tab_visible(&c);
    let on_closing = subscriptions_since(&c, before_second);

    c.assert_behaviour(
        "allegiance.subscription.the-tab-asks-the-shard-on-the-way-up-and-unasks-on-the-way-down",
        move |_| {
            bring_up == vec![SUBSCRIBE.to_vec(), SUBSCRIBE.to_vec()]
                && down_to_start
                && opened
                && on_opening == vec![SUBSCRIBE.to_vec()]
                && closed
                && on_closing == vec![UNSUBSCRIBE.to_vec()]
        },
    );
    c.shutdown();
}

#[test]
fn scenario_opening_the_tab_asks_the_shard_and_closing_it_unasks() {
    scenario("opening_the_tab_asks_the_shard_and_closing_it_unasks");
}

/// A tab left open asks once, not once a frame -- on a sixty-frame second the difference would be
/// sixty game actions a second going to the shard for nothing.
pub fn a_tab_left_open_asks_once_and_not_once_a_frame() {
    let mut c = a_client_before_the_first_frame(true);
    c.tick(1);
    let button = social_button(&mut c);
    let before_click = mark(&c);
    click(&mut c, button);
    let once_on_opening = subscriptions_since(&c, before_click).len() == 1;

    let after_click = mark(&c);
    c.tick(10);
    let silent = subscriptions_since(&c, after_click).is_empty();

    c.assert_behaviour(
        "allegiance.subscription.is-an-edge-and-not-a-poll",
        move |_| once_on_opening && silent,
    );
    c.shutdown();
}

#[test]
fn scenario_a_tab_left_open_asks_once_and_not_once_a_frame() {
    scenario("a_tab_left_open_asks_once_and_not_once_a_frame");
}

/// The asymmetry is the client's own and is kept rather than tidied: the opening ask is withheld
/// from a client that has no description of its own player yet, and the closing one is not.
pub fn the_opening_ask_is_withheld_without_a_player() {
    let mut c = a_client_before_the_first_frame(false);
    let start = mark(&c);
    c.tick(1);
    // The tab's own bring-up ask has no guard in front of it, so it goes out even here; the
    // description arm never fires at all. One, not two.
    let bring_up = subscriptions_since(&c, start) == vec![SUBSCRIBE.to_vec()];

    let button = social_button(&mut c);
    let before_open = mark(&c);
    click(&mut c, button);
    // The tab still comes up. Only the ask is withheld.
    let opened = tab_visible(&c);
    let silent_on_opening = subscriptions_since(&c, before_open).is_empty();

    let before_close = mark(&c);
    click(&mut c, button);
    let asks_on_closing = subscriptions_since(&c, before_close) == vec![UNSUBSCRIBE.to_vec()];

    c.assert_behaviour(
        "allegiance.subscription.the-opening-ask-is-withheld-without-a-player",
        move |_| bring_up && opened && silent_on_opening && asks_on_closing,
    );
    c.shutdown();
}

#[test]
fn scenario_the_opening_ask_is_withheld_without_a_player() {
    scenario("the_opening_ask_is_withheld_without_a_player");
}

/// The answer to the ask fills the tab with no further step, and does not itself provoke another
/// ask -- which is what separates "the tab was never told" from "the tab cannot draw".
pub fn the_answer_to_the_ask_fills_the_tab() {
    let mut c = a_client_before_the_first_frame(true);
    c.tick(1);
    let button = social_button(&mut c);
    let before_open = mark(&c);
    click(&mut c, button);
    let asked = subscriptions_since(&c, before_open) == vec![SUBSCRIBE.to_vec()];
    let empty_until_answered = c
        .view()
        .expect_app()
        .hud()
        .panels
        .allegiance
        .rows()
        .is_empty();

    // **One idle frame before the window opens.** On the `App` backend the harness reads the
    // client's outbox by cloning the frame's own `last_sent`, and it does that on every step as
    // well as on every frame -- so a window that starts on the frame a request went out and then
    // takes a `when` step counts that same request a second time. An idle frame empties the slot,
    // and what is counted below is the client's and not the harness's. (It is a harness seam, not
    // this scenario's.)
    c.tick(1);
    let after_open = mark(&c);
    answer(
        &mut c,
        &allegiance_answer(
            "The Hand of Dereth",
            3,
            1,
            vec![
                (None, allegiance_member(BOB, "Bob", 5, true)),
                (Some(BOB), allegiance_member(ME, "Lark", 3, true)),
                (Some(ME), allegiance_member(CID, "Cid", 1, true)),
            ],
        ),
    );
    let filled = {
        let rows = c
            .view()
            .expect_app()
            .hud()
            .panels
            .allegiance
            .rows()
            .to_vec();
        rows.len() == 1 && rows[0].id == CID && rows[0].name.contains("Cid")
    };
    let no_second_ask = subscriptions_since(&c, after_open).is_empty();

    c.assert_behaviour(
        "allegiance.subscription.the-answer-fills-the-tab-and-provokes-no-second-ask",
        move |_| asked && empty_until_answered && filled && no_second_ask,
    );
    c.shutdown();
}

#[test]
fn scenario_the_answer_to_the_ask_fills_the_tab() {
    scenario("the_answer_to_the_ask_fills_the_tab");
}

// =============================================================================================
// 13-16. The shard's own roster, and the three buttons on the tab
// =============================================================================================

/// The recorded character of the roster recording, and the one vassal it gives him.
const RECORDED_ME: ObjectId = ObjectId(0x5000_001E);
const RECORDED_VASSAL: ObjectId = ObjectId(0x5000_0020);

/// A client as the recorded character, with the tab open and the shard's own roster applied.
///
/// The roster is the recording's, not one this client encoded: a round trip cannot see a reader
/// that is self-consistently wrong, which is the whole reason this scenario replays a shard's bytes
/// rather than building its own.
fn a_client_showing_the_recorded_roster() -> HeadlessClient {
    let spec = ClientSpec {
        shell: true,
        ui_mode: Some(dereth_ui::framework::mode::GAME_PLAY),
        settle_frames: 0,
        ..ClientSpec::retail()
    };
    let mut c = HeadlessClient::new(spec);
    assert!(
        c.world_mut().set_player(RECORDED_ME),
        "the recorded character is this client's"
    );
    c.tick(1);
    let button = social_button(&mut c);
    click(&mut c, button);
    c.when(Inbound::from_corpus(
        ROSTER_SESSION,
        ROSTER_BLOB..ROSTER_BLOB + 1,
    ));
    c.tick(3);
    c
}

/// Whether a button on the tab is offered, read off the element and not off the tab's own mirror.
fn button_enabled(c: &HeadlessClient, id: ElementId) -> bool {
    let app = c.view().expect_app();
    let shell = app.ui().expect("the shell is up");
    let any: &dyn std::any::Any = shell.flow.current().expect("a screen is up");
    let root = any
        .downcast_ref::<GamePlayScreen>()
        .expect("the gameplay screen")
        .root()
        .expect("the gameplay screen's root");
    let h = shell
        .ui
        .get_child_recursive(root, id)
        .expect("the shipped button");
    shell
        .ui
        .node(h)
        .is_some_and(|n| n.state != dereth_ui::widgets::button::state::DISABLED)
}

/// One element of the shipped tree, by id.
fn element(c: &mut HeadlessClient, id: ElementId) -> ElemHandle {
    on_the_shipped_tree(c, |ui, root| {
        ui.get_child_recursive(root, id)
            .expect("the shipped element")
    })
}

/// The roster a shard really sent reaches the shipped vassal list.
pub fn the_roster_the_shard_really_sent_reaches_the_list() {
    let mut c = a_client_showing_the_recorded_roster();
    let (rows, channel) = {
        let p = &c.view().expect_app().hud().panels.allegiance;
        (
            p.rows()
                .iter()
                .map(|r| (r.id, r.name.clone(), r.logged_out_marker))
                .collect::<Vec<_>>(),
            p.vassal_chat_enabled,
        )
    };
    // One vassal, present, drawn under a name the shard gave and a title his rank earns him.
    let in_the_model =
        rows.len() == 1 && rows[0].0 == RECORDED_VASSAL && !rows[0].1.is_empty() && !rows[0].2;
    let drawn = vassal_row_names(&mut c);
    let on_the_screen = drawn.len() == 1 && drawn[0] == rows[0].1;

    c.assert_behaviour(
        "allegiance.panel.the-roster-the-shard-really-sent-reaches-the-list",
        move |_| in_the_model && channel && on_the_screen,
    );
    c.shutdown();
}

#[test]
fn scenario_the_roster_the_shard_really_sent_reaches_the_list() {
    scenario("the_roster_the_shard_really_sent_reaches_the_list");
}

/// Pressing Break asks a question and sends nothing: one misclick must not cost an allegiance.
pub fn breaking_asks_a_question_and_sends_nothing() {
    use dereth_ui_screens::panels::allegiance::{BREAK_BUTTON, KICK_BUTTON, SWEAR_BUTTON};

    let mut c = a_client_showing_the_recorded_roster();
    // The recorded character has a patron, so Break is lit and Swear is exactly its negation; and
    // no vassal row is picked yet, so Kick is dark.
    let lit = button_enabled(&c, BREAK_BUTTON)
        && !button_enabled(&c, SWEAR_BUTTON)
        && !button_enabled(&c, KICK_BUTTON);

    let before = c
        .view()
        .expect_app()
        .interaction()
        .stats
        .allegiance_confirmations_raised;
    let mark_out = mark(&c);
    let button = element(&mut c, BREAK_BUTTON);
    click(&mut c, button);
    let asked = c
        .view()
        .expect_app()
        .interaction()
        .stats
        .allegiance_confirmations_raised
        == before + 1;
    let sent_nothing = allegiance_actions_since(&c, mark_out).is_empty();

    c.assert_behaviour(
        "allegiance.buttons.breaking-asks-a-question-and-sends-nothing-by-itself",
        move |_| lit && asked && sent_nothing,
    );
    c.shutdown();
}

#[test]
fn scenario_breaking_asks_a_question_and_sends_nothing() {
    scenario("breaking_asks_a_question_and_sends_nothing");
}

/// Every oath or break the client put in its outbox after `from`. Nothing is sent: these are the
/// requests, before a transport ever sees them.
fn allegiance_actions_since(c: &HeadlessClient, from: usize) -> Vec<&dereth_client_model::Request> {
    c.view().outbound()[from..]
        .iter()
        .filter(|r| {
            matches!(
                r,
                dereth_client_model::Request::SwearAllegiance(_)
                    | dereth_client_model::Request::BreakAllegiance(_)
            )
        })
        .collect()
}

/// Kick is dark until a vassal row is picked, and the vassal it would act on comes off that row.
pub fn picking_a_vassal_arms_the_kick_button() {
    use dereth_ui_screens::panels::allegiance::KICK_BUTTON;

    let mut c = a_client_showing_the_recorded_roster();
    let one_row = c.view().expect_app().hud().panels.allegiance.rows().len() == 1;
    let dark_to_start = !button_enabled(&c, KICK_BUTTON);

    let row = c
        .view()
        .expect_app()
        .hud()
        .panels
        .allegiance
        .list
        .as_ref()
        .expect("the vassal list is bound")
        .items[0];
    click(&mut c, row);
    let picked = c
        .view()
        .expect_app()
        .hud()
        .panels
        .allegiance
        .selected_vassal
        == Some(RECORDED_VASSAL);
    let armed = button_enabled(&c, KICK_BUTTON);

    let before = c
        .view()
        .expect_app()
        .interaction()
        .stats
        .allegiance_confirmations_raised;
    let mark_out = mark(&c);
    let button = element(&mut c, KICK_BUTTON);
    click(&mut c, button);
    let asked = c
        .view()
        .expect_app()
        .interaction()
        .stats
        .allegiance_confirmations_raised
        == before + 1;
    let sent_nothing = allegiance_actions_since(&c, mark_out).is_empty();

    c.assert_behaviour(
        "allegiance.buttons.picking-a-vassal-arms-the-kick-button-from-the-rows-own-id",
        move |_| one_row && dark_to_start && picked && armed && asked && sent_nothing,
    );
    c.shutdown();
}

#[test]
fn scenario_picking_a_vassal_arms_the_kick_button() {
    scenario("picking_a_vassal_arms_the_kick_button");
}

/// Swear is dark while the player already has a patron, and a dark button raises nothing at all.
pub fn swear_is_dark_while_the_player_has_a_patron() {
    use dereth_ui_screens::panels::allegiance::SWEAR_BUTTON;

    let mut c = a_client_showing_the_recorded_roster();
    let dark = !button_enabled(&c, SWEAR_BUTTON);

    let before = c
        .view()
        .expect_app()
        .interaction()
        .stats
        .allegiance_confirmations_raised;
    let button = element(&mut c, SWEAR_BUTTON);
    click(&mut c, button);
    let silent = c
        .view()
        .expect_app()
        .interaction()
        .stats
        .allegiance_confirmations_raised
        == before;

    c.assert_behaviour(
        "allegiance.buttons.swear-is-dark-while-the-player-already-has-a-patron",
        move |_| dark && silent,
    );
    c.shutdown();
}

#[test]
fn scenario_swear_is_dark_while_the_player_has_a_patron() {
    scenario("swear_is_dark_while_the_player_has_a_patron");
}

// =============================================================================================
// 17-19. allegiance.commands.*
// =============================================================================================
//
// **None of these has a button.** Every one of the allegiance commands is reached from a typed
// line and from nowhere else in the interface, so the line is the gesture and these scenarios
// type it: the pointer presses the chat entry, one character message per character, and the return
// key. There is no recorded body for any of them -- nobody typed one while the recordings were
// being made, and saying so is more useful than pretending otherwise -- so the bytes below are
// composed here longhand and compared with what the production sender makes.

use dereth_client_model::allegiance_cmd as cmd;
use dereth_client_model::Request;
use dereth_protocol::social as wire;
use dereth_testkit::adapters_chat::Hand;

/// A length in characters, the characters, and zero padding to the next four-byte boundary -- the
/// layout a recorded client-to-server string in the corpus shows the retail client using.
fn pstr(s: &str) -> Vec<u8> {
    let mut v = u16::try_from(s.len())
        .expect("short")
        .to_le_bytes()
        .to_vec();
    v.extend_from_slice(s.as_bytes());
    while v.len() % 4 != 0 {
        v.push(0);
    }
    v
}

fn dw(n: u32) -> Vec<u8> {
    n.to_le_bytes().to_vec()
}

/// The framing a game action goes out in: the ordered-action header, the stamp the client
/// allocated, then the opcode and the body.
fn action(stamp: u32, opcode: u32, body: &[u8]) -> Vec<u8> {
    let mut v = vec![0xb1, 0xf7, 0x00, 0x00];
    v.extend_from_slice(&stamp.to_le_bytes());
    v.extend_from_slice(&opcode.to_le_bytes());
    v.extend_from_slice(body);
    v
}

/// A client in the world as the recorded character, with the recorded roster applied, ready to be
/// typed at.
///
/// The roster is the corpus's, so every line below is typed by a character who really is in an
/// allegiance -- and **none** of the commands reads it, which is worth having a real one for: a
/// client that resolved names against the roster would silently drop every name that is not one of
/// the player's own vassals.
fn a_client_to_type_at() -> HeadlessClient {
    let mut c = HeadlessClient::new(ClientSpec::gameplay(4));
    assert!(
        c.world_mut().set_player(RECORDED_ME),
        "the recorded character is this client's"
    );
    c.when(Inbound::from_corpus(
        ROSTER_SESSION,
        ROSTER_BLOB..ROSTER_BLOB + 1,
    ));
    c.tick(1);
    c
}

/// The text of every live bubble in the strip across the top of the screen, in order.
fn bubbles(c: &mut HeadlessClient) -> Vec<String> {
    on_the_shipped_tree(c, |ui, root| {
        let list = ui
            .get_child_recursive(root, dereth_ui_screens::hud::speech_bubbles::LIST_BOX)
            .expect("the strip is in the shipped layout");
        let kids = ui.children(list);
        kids.into_iter()
            .filter_map(|h| ui.text_element_mut(h).map(|t| t.glyphs.inq_text(false)))
            .collect()
    })
}

/// Which message each request is, so that a line that built the wrong one is a failure and not a
/// silent pass. It is written out rather than read off the request, because a test that reads a
/// constant back through the symbol that wrote it cannot detect a wrong one.
fn opcode_of(r: &Request) -> u32 {
    match r {
        Request::AllegianceQueryName(_) => 0x0030,
        Request::AllegianceClearName(_) => 0x0031,
        Request::AllegianceSetName(_) => 0x0033,
        Request::AllegianceSetOfficer(_) => 0x003B,
        Request::AllegianceSetOfficerTitle(_) => 0x003C,
        Request::AllegianceListOfficerTitles(_) => 0x003D,
        Request::AllegianceClearOfficerTitles(_) => 0x003E,
        Request::AllegianceDoLockAction(_) => 0x003F,
        Request::AllegianceSetApprovedVassal(_) => 0x0040,
        Request::AllegianceChatGag(_) => 0x0041,
        Request::AllegianceDoHouseAction(_) => 0x0042,
        Request::AllegianceSetMotd(_) => 0x0254,
        Request::AllegianceQueryMotd(_) => 0x0255,
        Request::AllegianceClearMotd(_) => 0x0256,
        Request::AllegianceBreakAllegianceBoot(_) => 0x0277,
        Request::AllegianceInfoRequest(_) => 0x027B,
        Request::AllegianceChatBoot(_) => 0x02A0,
        Request::AllegianceAddBan(_) => 0x02A1,
        Request::AllegianceRemoveBan(_) => 0x02A2,
        Request::AllegianceListBans(_) => 0x02A3,
        Request::AllegianceRemoveOfficer(_) => 0x02A5,
        Request::AllegianceListOfficers(_) => 0x02A6,
        Request::AllegianceClearOfficers(_) => 0x02A7,
        Request::AllegianceRecallHometown(_) => 0x02AB,
        other => panic!("not an allegiance command request: {other:?}"),
    }
}

/// One typed line per command, each putting its own message in the outbox with its own bytes.
#[allow(clippy::too_many_lines)]
pub fn every_typed_allegiance_command_sends_its_own_message() {
    let mut c = a_client_to_type_at();
    let mut hand = Hand::new();
    let mut session = dereth_client_net::client_session::Session::new(
        dereth_client_net::client_session::testing::MockTransport::new(),
    );

    // (the typed line, the message it must build, the body that must go on the wire)
    let cases: Vec<(&str, Request, Vec<u8>)> = vec![
        (
            "@allegiance name",
            Request::AllegianceQueryName(wire::AllegianceQueryAllegianceName),
            Vec::new(),
        ),
        (
            "@allegiance name set The Hand",
            Request::AllegianceSetName(wire::AllegianceSetAllegianceName {
                name: "The Hand".into(),
            }),
            pstr("The Hand"),
        ),
        (
            "@allegiance name clear",
            Request::AllegianceClearName(wire::AllegianceClearAllegianceName),
            Vec::new(),
        ),
        (
            "@motd",
            Request::AllegianceQueryMotd(wire::AllegianceQueryMotd),
            Vec::new(),
        ),
        (
            "@motd set Be excellent",
            Request::AllegianceSetMotd(wire::AllegianceSetMotd {
                motd: "Be excellent".into(),
            }),
            pstr("Be excellent"),
        ),
        (
            "@motd clear",
            Request::AllegianceClearMotd(wire::AllegianceClearMotd),
            Vec::new(),
        ),
        (
            "@allegiance motd",
            Request::AllegianceQueryMotd(wire::AllegianceQueryMotd),
            Vec::new(),
        ),
        (
            "@allegiance officer",
            Request::AllegianceListOfficers(wire::AllegianceListAllegianceOfficers),
            Vec::new(),
        ),
        (
            "@allegiance officer list",
            Request::AllegianceListOfficers(wire::AllegianceListAllegianceOfficers),
            Vec::new(),
        ),
        (
            "@allegiance officer clear",
            Request::AllegianceClearOfficers(wire::AllegianceClearAllegianceOfficers),
            Vec::new(),
        ),
        (
            "@allegiance officer add 2 Bob",
            Request::AllegianceSetOfficer(wire::AllegianceSetAllegianceOfficer {
                name: "Bob".into(),
                level: 2,
            }),
            [pstr("Bob"), dw(2)].concat(),
        ),
        (
            "@allegiance officer set 3 Bob",
            Request::AllegianceSetOfficer(wire::AllegianceSetAllegianceOfficer {
                name: "Bob".into(),
                level: 3,
            }),
            [pstr("Bob"), dw(3)].concat(),
        ),
        (
            "@allegiance officer remove Bob",
            Request::AllegianceRemoveOfficer(wire::AllegianceRemoveAllegianceOfficer {
                name: "Bob".into(),
            }),
            pstr("Bob"),
        ),
        (
            "@allegiance title",
            Request::AllegianceListOfficerTitles(wire::AllegianceListAllegianceOfficerTitles),
            Vec::new(),
        ),
        (
            "@allegiance title clear",
            Request::AllegianceClearOfficerTitles(wire::AllegianceClearAllegianceOfficerTitles),
            Vec::new(),
        ),
        // The level is **first** on the wire here, where the officer message above puts the name
        // first. The asymmetry is the client's own and is kept rather than tidied.
        (
            "@allegiance title set 1 Speaker",
            Request::AllegianceSetOfficerTitle(wire::AllegianceSetAllegianceOfficerTitle {
                level: 1,
                title: "Speaker".into(),
            }),
            [dw(1), pstr("Speaker")].concat(),
        ),
        (
            "@allegiance ban list",
            Request::AllegianceListBans(wire::AllegianceListAllegianceBans),
            Vec::new(),
        ),
        (
            "@allegiance ban add Bob",
            Request::AllegianceAddBan(wire::AllegianceAddAllegianceBan { name: "Bob".into() }),
            pstr("Bob"),
        ),
        (
            "@allegiance ban remove Bob",
            Request::AllegianceRemoveBan(wire::AllegianceRemoveAllegianceBan {
                name: "Bob".into(),
            }),
            pstr("Bob"),
        ),
        (
            "@allegiance lock",
            Request::AllegianceDoLockAction(wire::AllegianceDoAllegianceLockAction {
                action: cmd::lock_action::CHECK,
            }),
            dw(4),
        ),
        (
            "@allegiance lock on",
            Request::AllegianceDoLockAction(wire::AllegianceDoAllegianceLockAction {
                action: cmd::lock_action::ON,
            }),
            dw(2),
        ),
        (
            "@allegiance lock off",
            Request::AllegianceDoLockAction(wire::AllegianceDoAllegianceLockAction {
                action: cmd::lock_action::OFF,
            }),
            dw(1),
        ),
        (
            "@allegiance lock toggle",
            Request::AllegianceDoLockAction(wire::AllegianceDoAllegianceLockAction {
                action: cmd::lock_action::TOGGLE,
            }),
            dw(3),
        ),
        (
            "@allegiance lock bypass",
            Request::AllegianceDoLockAction(wire::AllegianceDoAllegianceLockAction {
                action: cmd::lock_action::CHECK_APPROVED,
            }),
            dw(5),
        ),
        (
            "@allegiance lock bypass clear",
            Request::AllegianceDoLockAction(wire::AllegianceDoAllegianceLockAction {
                action: cmd::lock_action::CLEAR_APPROVED,
            }),
            dw(6),
        ),
        (
            "@allegiance lock bypass Bob",
            Request::AllegianceSetApprovedVassal(wire::AllegianceSetAllegianceApprovedVassal {
                name: "Bob".into(),
            }),
            pstr("Bob"),
        ),
        (
            "@allegiance house",
            Request::AllegianceDoHouseAction(wire::AllegianceDoAllegianceHouseAction {
                action: cmd::house_action::HELP,
            }),
            dw(1),
        ),
        (
            "@allegiance house guest open",
            Request::AllegianceDoHouseAction(wire::AllegianceDoAllegianceHouseAction {
                action: cmd::house_action::GUEST_OPEN,
            }),
            dw(2),
        ),
        (
            "@allegiance house guest close",
            Request::AllegianceDoHouseAction(wire::AllegianceDoAllegianceHouseAction {
                action: cmd::house_action::GUEST_CLOSE,
            }),
            dw(3),
        ),
        (
            "@allegiance house storage open",
            Request::AllegianceDoHouseAction(wire::AllegianceDoAllegianceHouseAction {
                action: cmd::house_action::STORAGE_OPEN,
            }),
            dw(4),
        ),
        (
            "@allegiance house storage close",
            Request::AllegianceDoHouseAction(wire::AllegianceDoAllegianceHouseAction {
                action: cmd::house_action::STORAGE_CLOSE,
            }),
            dw(5),
        ),
        (
            "@allegiance info Bob",
            Request::AllegianceInfoRequest(wire::AllegianceInfoRequest { name: "Bob".into() }),
            pstr("Bob"),
        ),
        (
            "@ah",
            Request::AllegianceRecallHometown(wire::AllegianceRecallAllegianceHometown),
            Vec::new(),
        ),
        (
            "@alh",
            Request::AllegianceRecallHometown(wire::AllegianceRecallAllegianceHometown),
            Vec::new(),
        ),
        (
            "@allegiance hometown",
            Request::AllegianceRecallHometown(wire::AllegianceRecallAllegianceHometown),
            Vec::new(),
        ),
        (
            "@allegiance chat gag Bob",
            Request::AllegianceChatGag(wire::AllegianceChatGag {
                name: "Bob".into(),
                gagged: 1,
            }),
            [pstr("Bob"), dw(1)].concat(),
        ),
        (
            "@allegiance chat ungag Bob",
            Request::AllegianceChatGag(wire::AllegianceChatGag {
                name: "Bob".into(),
                gagged: 0,
            }),
            [pstr("Bob"), dw(0)].concat(),
        ),
        (
            "@allegiance chat kick Bob, being rude",
            Request::AllegianceChatBoot(wire::AllegianceChatBoot {
                name: "Bob".into(),
                reason: "being rude".into(),
            }),
            [pstr("Bob"), pstr("being rude")].concat(),
        ),
        (
            "@allegiance ch kick Bob",
            Request::AllegianceChatBoot(wire::AllegianceChatBoot {
                name: "Bob".into(),
                reason: cmd::NO_REASON_GIVEN.into(),
            }),
            [pstr("Bob"), pstr(cmd::NO_REASON_GIVEN)].concat(),
        ),
        (
            "@allegiance boot Bob",
            Request::AllegianceBreakAllegianceBoot(wire::AllegianceBreakAllegianceBoot {
                name: "Bob".into(),
                account_boot: 0,
            }),
            [pstr("Bob"), dw(0)].concat(),
        ),
        (
            "@allegiance boot -account Bob",
            Request::AllegianceBreakAllegianceBoot(wire::AllegianceBreakAllegianceBoot {
                name: "Bob".into(),
                account_boot: 1,
            }),
            [pstr("Bob"), dw(1)].concat(),
        ),
    ];

    let mut every_line_holds = true;
    let mut stamp = 1_u32;
    for (line, want, body) in &cases {
        let (unimplemented, refused, lines) = {
            let s = &c.view().expect_app().interaction().stats;
            (
                s.chat_commands_unimplemented,
                s.chat_commands_refused,
                s.chat_lines_sent,
            )
        };
        hand.say(&mut c, line);
        let reached_the_client = {
            let s = &c.view().expect_app().interaction().stats;
            // The keystrokes have to become a chat line before anything else is judged, and a
            // wired command neither falls to the catch-all nor prints a refusal.
            s.chat_lines_sent == lines + 1
                && s.chat_commands_unimplemented == unimplemented
                && s.chat_commands_refused == refused
        };
        let built =
            c.view().expect_app().interaction().last_sent.as_slice() == std::slice::from_ref(want);

        // The wire: one send, one stamp, and the body composed above rather than by the writer
        // being asserted over. **Nothing leaves this process**; the transport is a mock.
        let sent = dereth_client_runtime::requests::send_request(&mut session, want);
        let packet = session
            .transport
            .sent
            .last()
            .expect("one datagram per request");
        let framed = (packet.queue, packet.ordered) == (dereth_primitives::NetQueue::Weenie, true)
            && packet.payload == action(stamp, opcode_of(want), body);
        stamp += 1;

        c.tick(1);
        let not_repeated = c.view().expect_app().interaction().last_sent.is_empty();

        assert!(
            reached_the_client && built && sent && framed && not_repeated,
            "{line}: reached={reached_the_client} built={built} sent={sent} framed={framed} \
             once={not_repeated}; last_sent={:?}",
            c.view().expect_app().interaction().last_sent
        );
        every_line_holds &= reached_the_client && built && sent && framed && not_repeated;
    }

    // And between them the lines cover every distinct allegiance message there is a command for,
    // counted off the datagrams rather than off the table.
    let mut opcodes: Vec<u32> = session
        .transport
        .sent
        .iter()
        .map(|p| u32::from_le_bytes(p.payload[8..12].try_into().expect("an opcode dword")))
        .collect();
    opcodes.sort_unstable();
    opcodes.dedup();
    let distinct = opcodes.len();
    let counted = c
        .view()
        .expect_app()
        .interaction()
        .stats
        .allegiance_command_requests
        == u32::try_from(cases.len()).expect("small");

    c.assert_behaviour(
        "allegiance.commands.every-typed-allegiance-command-sends-its-own-message",
        move |_| every_line_holds && distinct == 24 && counted,
    );
    c.shutdown();
}

#[test]
fn scenario_every_typed_allegiance_command_sends_its_own_message() {
    scenario("every_typed_allegiance_command_sends_its_own_message");
}

/// A mistyped line is answered and sends nothing -- an arm that sent on every line would satisfy
/// the table above and empty a monarch's officer list on a typo.
pub fn a_refused_allegiance_line_prints_and_sends_nothing() {
    let mut c = a_client_to_type_at();
    let mut hand = Hand::new();
    let mut every_refusal_holds = true;

    for (line, want) in [
        // An unknown sub-command, and a bare command, both reach the client's own hint.
        ("@allegiance", cmd::PLEASE_SEE_HELP_ALLEGIANCE),
        ("@allegiance wibble", cmd::PLEASE_SEE_HELP_ALLEGIANCE),
        ("@allegiance name wibble", cmd::PLEASE_SEE_HELP_ALLEGIANCE),
        (
            "@allegiance house wibble open",
            cmd::PLEASE_SEE_HELP_ALLEGIANCE,
        ),
        // Four commands share one answer when the name they need is missing.
        ("@allegiance info", cmd::PLEASE_SPECIFY_AN_ACTUAL_NAME),
        ("@allegiance boot", cmd::PLEASE_SPECIFY_AN_ACTUAL_NAME),
        ("@allegiance ban add", cmd::PLEASE_SPECIFY_AN_ACTUAL_NAME),
        ("@allegiance chat gag", cmd::PLEASE_SPECIFY_AN_ACTUAL_NAME),
        (
            "@allegiance officer remove",
            cmd::PLEASE_SPECIFY_AN_ALLEGIANCE_MEMBER,
        ),
        (
            "@allegiance officer set 9 Bob",
            cmd::PLEASE_SPECIFY_A_VALID_OFFICER_LEVEL_LONG,
        ),
        (
            "@allegiance title set 0 x",
            cmd::PLEASE_SPECIFY_A_VALID_OFFICER_LEVEL,
        ),
        ("@ah now", cmd::THIS_COMMAND_TAKES_NO_ARGUMENTS),
    ] {
        let before = c
            .view()
            .expect_app()
            .interaction()
            .stats
            .allegiance_command_requests;
        hand.say(&mut c, line);
        let silent = {
            let i = c.view().expect_app().interaction();
            i.last_sent.is_empty()
                && i.stats.allegiance_command_requests == before
                // Refused by the command itself, not dropped for want of an arm.
                && i.stats.chat_commands_unimplemented == 0
        };
        // Three frames: one for the answer to reach the scroll, one for it to be drained, one for
        // the strip to build the element. The strip is the one surface whose shipped filter accepts
        // this kind of line, so it is where the player actually reads it -- and the observable is
        // the element and not a counter.
        c.tick(3);
        let printed = bubbles(&mut c)
            .iter()
            .any(|t| t.trim_end() == want.trim_end());
        assert!(
            silent && printed,
            "{line}: silent={silent} printed={printed}"
        );
        every_refusal_holds &= silent && printed;
        c.tick(1);
    }

    c.assert_behaviour(
        "allegiance.commands.a-refused-line-is-printed-where-the-player-reads-it-and-sends-nothing",
        move |_| every_refusal_holds,
    );
    c.shutdown();
}

#[test]
fn scenario_a_refused_allegiance_line_prints_and_sends_nothing() {
    scenario("a_refused_allegiance_line_prints_and_sends_nothing");
}

/// The two lines in this family that are not allegiance messages at all.
pub fn listening_is_an_option_and_broadcasting_is_a_channel() {
    let mut c = a_client_to_type_at();
    let mut hand = Hand::new();

    // Hearing allegiance chat starts on, so "off" is the change and "on" puts it back. Each is one
    // of the options the client tells the shard about at once rather than at the next save.
    let mut option_lines = true;
    for (line, value) in [("@allegiance chat off", 0_u32), ("@allegiance chat on", 1)] {
        hand.say(&mut c, line);
        let i = c.view().expect_app().interaction();
        option_lines &= i.last_sent.as_slice()
            == [Request::PlayerOptionChanged(
                dereth_protocol::login::CharacterPlayerOptionChangedEvent {
                    option: u32::try_from(cmd::HEAR_ALLEGIANCE_CHAT_ORDINAL).expect("small"),
                    value,
                },
            )]
            && i.stats.allegiance_command_requests == 0;
        c.tick(1);
    }
    // Saying it again when it already agrees changes nothing, so nothing is sent.
    hand.say(&mut c, "@allegiance chat on");
    let already = c.view().expect_app().interaction().last_sent.is_empty();
    c.tick(1);

    let broadcast =
        Request::ChannelBroadcast(dereth_protocol::comms::CommunicationChannelBroadcast {
            channel: cmd::ALLEGIANCE_BROADCAST_CHANNEL,
            message: "well met".into(),
        });
    hand.say(&mut c, "@ab well met");
    let short = c.view().expect_app().interaction().last_sent.as_slice()
        == std::slice::from_ref(&broadcast);
    c.tick(1);
    hand.say(&mut c, "@allegiance broadcast well met");
    let long = c.view().expect_app().interaction().last_sent.as_slice()
        == std::slice::from_ref(&broadcast);
    c.tick(1);

    c.assert_behaviour(
        "allegiance.commands.listening-is-a-local-option-and-broadcasting-is-a-channel",
        move |_| option_lines && already && short && long,
    );
    c.shutdown();
}

#[test]
fn scenario_listening_is_an_option_and_broadcasting_is_a_channel() {
    scenario("listening_is_an_option_and_broadcasting_is_a_channel");
}

// =============================================================================================
// 20. fellowship.lines.reach-the-live-chat-log-in-the-colour-of-a-broadcast
// =============================================================================================

/// The character the two recordings below were made on.
const FELLOWSHIP_AUTHOR: ObjectId = ObjectId(0x5000_001F);
/// The colour every glyph of a broadcast line is drawn in.
const BROADCAST_GREEN: u32 = 0xFF80_FF7F;

/// The colour every glyph of `needle` is drawn in, inside the live chat log.
///
/// Its absence is a panic, which is the assertion that the line reached the log at all: a scenario
/// that skipped a missing line would pass over one that was composed and never drawn, and being
/// drawn is this claim's whole point.
fn colour_in_the_log(c: &mut HeadlessClient, needle: &str) -> u32 {
    on_the_shipped_tree(c, |ui, root| {
        let h = ui
            .get_child_recursive(root, dereth_ui_screens::chat::window::LOG)
            .expect("the chat log is in the shipped layout");
        let glyphs: Vec<(char, u32)> = ui.text_element_mut(h).map_or_else(Vec::new, |t| {
            t.glyphs
                .glyphs
                .iter()
                .map(|g| (char::from_u32(u32::from(g.data)).unwrap_or('?'), g.color))
                .collect()
        });
        let text: String = glyphs.iter().map(|(ch, _)| *ch).collect();
        let at = text
            .find(needle)
            .unwrap_or_else(|| panic!("{needle:?} never reached the log; it holds {text:?}"));
        let n = needle.chars().count();
        let colours: Vec<u32> = glyphs[at..at + n].iter().map(|(_, col)| *col).collect();
        assert!(
            colours.windows(2).all(|w| w[0] == w[1]),
            "drawn in more than one colour"
        );
        colours[0]
    })
}

/// The fellowship lines are drawn, and drawn in the colour this kind of line is drawn in.
pub fn the_fellowship_lines_reach_the_live_chat_log() {
    let mut c = HeadlessClient::new(ClientSpec::gameplay(4));
    assert!(
        c.world_mut().set_player(FELLOWSHIP_AUTHOR),
        "the recorded character is this client's"
    );
    let before = on_the_shipped_tree(&mut c, |ui, root| {
        let h = ui
            .get_child_recursive(root, dereth_ui_screens::chat::window::LOG)
            .expect("the chat log");
        ui.text_element_mut(h)
            .map_or_else(String::new, |t| t.glyphs.inq_text(false))
    });
    let nothing_said_yet = !before.contains("Fellowship");

    // The recorded creation, and the recorded opening from the session that did it.
    c.when(Inbound::from_corpus("fellowship-two-monarch", 525..526));
    c.when(Inbound::from_corpus("fellowship-three-vassal", 426..427));
    // The one line no recording carries, through the production writer.
    c.when(Inbound::message(
        &dereth_protocol::comms::CommunicationWeenieErrorWithString {
            error_type: 0x050C,
            text: "Of The ring".to_owned(),
        },
    ));
    // And the recorded departure.
    c.when(Inbound::from_corpus("fellowship-two-monarch", 751..752));
    c.tick(1);

    let colours: Vec<u32> = [
        "You have created the Fellowship of Of The ring.",
        "Of The ring is now an open fellowship; anyone may recruit new members.",
        "Of The ring is now a closed fellowship.",
        "You are no longer a member of the Of The ring Fellowship.",
    ]
    .into_iter()
    .map(|needle| colour_in_the_log(&mut c, needle))
    .collect();
    // Two of the four are lines the client writes for itself: the creation and the departure.
    let composed = c.view().expect_app().hud().stats.fellowship_lines_composed == 2;

    c.assert_behaviour(
        "fellowship.lines.reach-the-live-chat-log-in-the-colour-of-a-broadcast",
        move |_| nothing_said_yet && colours == vec![BROADCAST_GREEN; 4] && composed,
    );
    c.shutdown();
}

#[test]
fn scenario_the_fellowship_lines_reach_the_live_chat_log() {
    scenario("the_fellowship_lines_reach_the_live_chat_log");
}

// =============================================================================================
// 21-28. The fellowship tab: from the player's click to the bytes that would go out
// =============================================================================================
//
// Every scenario here starts from the shipped element tree, makes a real mouse gesture on a real
// element found by its own id, and then asks what a player would see: the rows the list holds, the
// state a button is in, and the message that **would** be sent. **No datagram leaves this
// process**: the messages are taken out of the client's outbox and encoded with the production
// writer, and there is no socket anywhere here.
//
// Each one's first assertion is the state before the gesture, so a scenario that passes because
// its observable was already true cannot exist.

/// The social page: a tabbed container the allegiance tab and the fellowship tab share, and which
/// opens on the allegiance one.
const SOCIAL_PAGE: ElementId = ElementId(0x1000_018F);
/// The fellowship tab's own page inside it.
const FELLOWSHIP_PAGE: ElementId = ElementId(0x1000_0292);
/// The share-experience tick box on the create side of the tab, bound over the same option the
/// create message carries.
const SHARE_XP_BOX: ElementId = ElementId(0x1000_0272);

const F_ME: ObjectId = ObjectId(0x5000_0001);
const F_FELLOW: ObjectId = ObjectId(0x5000_001F);
const F_OUTSIDER: ObjectId = ObjectId(0x5000_0020);

/// A client in the world with three player bodies in it: the player, somebody who will be in his
/// fellowship, and somebody who will not.
fn a_client_with_three_players() -> HeadlessClient {
    let mut c = HeadlessClient::new(ClientSpec::gameplay(4));
    {
        let w = c.world_mut();
        assert!(w.set_player(F_ME), "the player id takes");
        for id in [F_ME, F_FELLOW, F_OUTSIDER] {
            let mut it = dereth_client_model::Weenie::new(id);
            it.valid = true;
            it.pwd.bitfield |= dereth_client_model::weenie::bitfield::PLAYER;
            w.tables.weenies.insert(id, it);
        }
    }
    c
}

/// The caption that opens the fellowship tab, read out of the live tabbed container's own table
/// rather than named -- which is the element a player can actually click.
fn fellowship_tab(c: &mut HeadlessClient) -> ElemHandle {
    on_the_shipped_tree(c, |ui, root| {
        let page = ui
            .get_child_recursive(root, SOCIAL_PAGE)
            .expect("the social page");
        let tab = ui
            .node(page)
            .and_then(|n| {
                n.behaviour
                    .as_ref()?
                    .as_any()?
                    .downcast_ref::<dereth_ui::widgets::panel::Panel>()
            })
            .and_then(|p| p.page_to_tab.get(&FELLOWSHIP_PAGE).copied())
            .expect("the social page's tab table names the fellowship page");
        ui.get_child_recursive(page, tab)
            .expect("the tab caption element")
    })
}

/// Whether the fellowship tab is up, by the whole parent chain.
fn fellowship_tab_visible(c: &HeadlessClient) -> bool {
    let app = c.view().expect_app();
    let h = app
        .hud()
        .panels
        .fellowship
        .panel
        .expect("the tab is in the shipped tree");
    app.ui().expect("the shell is up").ui.is_visible(h)
}

/// Every fellowship message the client put in its outbox after `from`, as the bytes that would go
/// on the wire.
fn fellowship_sent_since(c: &HeadlessClient, from: usize) -> Vec<Vec<u8>> {
    use dereth_protocol::Opcode;
    c.view().outbound()[from..]
        .iter()
        .filter_map(|r| {
            let (op, body) = match r {
                Request::FellowshipUpdateRequest(m) => (
                    Opcode::FELLOWSHIP_UPDATE_REQUEST,
                    dereth_protocol::write_body(m),
                ),
                Request::FellowshipCreate(m) => {
                    (Opcode::FELLOWSHIP_CREATE, dereth_protocol::write_body(m))
                }
                Request::FellowshipQuit(m) => {
                    (Opcode::FELLOWSHIP_QUIT, dereth_protocol::write_body(m))
                }
                Request::FellowshipDismiss(m) => {
                    (Opcode::FELLOWSHIP_DISMISS, dereth_protocol::write_body(m))
                }
                Request::FellowshipRecruit(m) => {
                    (Opcode::FELLOWSHIP_RECRUIT, dereth_protocol::write_body(m))
                }
                Request::FellowshipAssignNewLeader(m) => (
                    Opcode::FELLOWSHIP_ASSIGN_NEW_LEADER,
                    dereth_protocol::write_body(m),
                ),
                Request::FellowshipChangeFellowOpenness(m) => (
                    Opcode::FELLOWSHIP_CHANGE_FELLOW_OPENNESS,
                    dereth_protocol::write_body(m),
                ),
                _ => return None,
            };
            let mut blob = op.0.to_le_bytes().to_vec();
            blob.extend(body.expect("the body encodes"));
            Some(blob)
        })
        .collect()
}

/// A membership list naming `members`, led by `leader`.
///
/// The even split is off so that the tab takes the proportion arm rather than the flat one; with no
/// experience table in a headless run the proportion sum is nought and the share reads nought,
/// which is what the meters below are asserted against.
fn membership(
    members: &[(ObjectId, &str)],
    leader: ObjectId,
    open: bool,
) -> dereth_protocol::social::FellowshipFullUpdate {
    use dereth_protocol::social::{Fellow, Fellowship, FellowshipFullUpdate};
    let mut f = Fellowship {
        name: "Of The ring".to_owned(),
        leader,
        share_xp: 1,
        even_xp_split: 0,
        open_fellow: i32::from(open),
        locked: 0,
        ..Fellowship::default()
    };
    f.members.table_size = 8;
    for (id, name) in members {
        f.members.entries.push((
            id.0,
            Fellow {
                name: (*name).to_owned(),
                level: 20,
                cp_cache: 0,
                lum_cache: 0,
                share_loot: 0,
                max_health: 100,
                max_stamina: 120,
                max_mana: 140,
                current_health: 75,
                current_stamina: 60,
                current_mana: 35,
            },
        ));
    }
    f.fellows_departed.table_size = 8;
    FellowshipFullUpdate(f)
}

/// Open the social page and then its fellowship tab.
fn open_the_fellowship_tab(c: &mut HeadlessClient) {
    let b = social_button(c);
    click(c, b);
    let tab = fellowship_tab(c);
    click(c, tab);
    assert!(
        fellowship_tab_visible(c),
        "clicking the caption must bring the tab up"
    );
}

/// Put a fellowship on the wire and draw it.
fn join(c: &mut HeadlessClient, members: &[(ObjectId, &str)], leader: ObjectId, open: bool) {
    c.when(Inbound::message(&membership(members, leader, open)));
    c.tick(1);
}

fn button_lit(c: &HeadlessClient, h: Option<ElemHandle>) -> bool {
    h.is_some_and(|h| {
        dereth_ui_screens::panels::fellowship::button_enabled(
            &c.view().expect_app().ui().expect("the shell is up").ui,
            h,
        )
    })
}

/// Opening the page the tab shares is not opening the tab, and only the tab asks.
pub fn the_social_page_opening_is_not_the_fellowship_tab_opening() {
    let mut c = a_client_with_three_players();
    let start = mark(&c);
    c.tick(1);
    let down = !fellowship_tab_visible(&c) && fellowship_sent_since(&c, start).is_empty();

    // The page opens on the other tab, so nothing is asked for yet.
    let button = social_button(&mut c);
    let before_page = mark(&c);
    click(&mut c, button);
    let page_only =
        !fellowship_tab_visible(&c) && fellowship_sent_since(&c, before_page).is_empty();

    let tab = fellowship_tab(&mut c);
    let before_tab = mark(&c);
    click(&mut c, tab);
    let up = fellowship_tab_visible(&c);
    let asked = fellowship_sent_since(&c, before_tab)
        == vec![vec![0xA6, 0x00, 0x00, 0x00, 0x01, 0x00, 0x00, 0x00]];

    // And away again: one edge each way, with no repeats in between.
    let before_close = mark(&c);
    click(&mut c, button);
    let closed = !fellowship_tab_visible(&c)
        && fellowship_sent_since(&c, before_close)
            == vec![vec![0xA6, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00]];
    let twice = c
        .view()
        .expect_app()
        .hud()
        .panels
        .fellowship
        .update_requests
        == 2;

    c.assert_behaviour(
        "fellowship.subscription.the-social-page-opening-is-not-the-tab-opening",
        move |_| down && page_only && up && asked && closed && twice,
    );
    c.shutdown();
}

#[test]
fn scenario_the_social_page_opening_is_not_the_fellowship_tab_opening() {
    scenario("the_social_page_opening_is_not_the_fellowship_tab_opening");
}

/// The membership list fills the tab with rows a player can read.
pub fn a_membership_list_draws_a_roster_with_names_stats_and_meters() {
    let mut c = a_client_with_three_players();
    open_the_fellowship_tab(&mut c);

    let (not_in, in_f) = {
        let p = &c.view().expect_app().hud().panels.fellowship;
        (p.not_in_frame.expect("bound"), p.in_frame.expect("bound"))
    };
    let visible = |c: &HeadlessClient, h: ElemHandle| {
        c.view()
            .expect_app()
            .ui()
            .expect("the shell is up")
            .ui
            .is_visible(h)
    };
    // Before: no fellowship, so no rows and the frame that says there is none.
    let before = c
        .view()
        .expect_app()
        .hud()
        .panels
        .fellowship
        .rows()
        .is_empty()
        && visible(&c, not_in)
        && !visible(&c, in_f);

    join(
        &mut c,
        &[(F_ME, "Larktest"), (F_FELLOW, "Bex")],
        F_ME,
        false,
    );
    let swapped = !visible(&c, not_in) && visible(&c, in_f);

    let (rows, list_items, failures, rebuilds) = {
        let p = &c.view().expect_app().hud().panels.fellowship;
        let list = p.list.as_ref().expect("the list is in the shipped tree");
        (
            p.rows().to_vec(),
            list.items.len(),
            list.create_failures,
            p.rebuilds,
        )
    };
    let drawn = rows.len() == 2
        && rows.iter().map(|r| r.id).collect::<Vec<_>>() == vec![F_ME, F_FELLOW]
        && rows[1].name.contains("Bex")
        && rows[1].stats.contains("20")
        && rows[1].health.contains("75")
        && rows[1].health.contains("100")
        && (rows[1].meters[0] - 0.75).abs() < 1e-6
        && (rows[1].meters[1] - 0.5).abs() < 1e-6
        && (rows[1].meters[2] - 0.25).abs() < 1e-6;
    // The rows are in the live list, not only in the tab's own mirror.
    let in_the_tree = list_items == 2 && failures == 0;

    // And an unchanged fellowship rewrites nothing.
    c.tick(1);
    let idempotent = c.view().expect_app().hud().panels.fellowship.rebuilds == rebuilds;

    c.assert_behaviour(
        "fellowship.panel.a-membership-list-draws-a-roster-with-names-stats-and-meters",
        move |_| before && swapped && drawn && in_the_tree && idempotent,
    );
    c.shutdown();
}

#[test]
fn scenario_a_membership_list_draws_a_roster_with_names_stats_and_meters() {
    scenario("a_membership_list_draws_a_roster_with_names_stats_and_meters");
}

/// Only the leader may disband, and a dark button swallows the press.
pub fn disband_is_offered_only_to_the_leader() {
    let mut c = a_client_with_three_players();
    open_the_fellowship_tab(&mut c);

    // A fellowship the player does not lead.
    join(
        &mut c,
        &[(F_ME, "Larktest"), (F_FELLOW, "Bex")],
        F_FELLOW,
        false,
    );
    let h = c
        .view()
        .expect_app()
        .hud()
        .panels
        .fellowship
        .disband_button
        .expect("bound");
    let dark = !button_lit(&c, Some(h));
    let before_press = mark(&c);
    click(&mut c, h);
    let swallowed = fellowship_sent_since(&c, before_press).is_empty();

    // Now lead it.
    join(
        &mut c,
        &[(F_ME, "Larktest"), (F_FELLOW, "Bex")],
        F_ME,
        false,
    );
    let lit = button_lit(&c, Some(h));
    let before_disband = mark(&c);
    click(&mut c, h);
    let sent = fellowship_sent_since(&c, before_disband)
        == vec![vec![0xA3, 0x00, 0x00, 0x00, 0x01, 0x00, 0x00, 0x00]];

    c.assert_behaviour(
        "fellowship.buttons.disband-is-offered-only-to-the-leader",
        move |_| dark && swallowed && lit && sent,
    );
    c.shutdown();
}

#[test]
fn scenario_disband_is_offered_only_to_the_leader() {
    scenario("disband_is_offered_only_to_the_leader");
}

/// A leader who leaves hands the lead on first; a plain member does not.
pub fn a_leader_who_leaves_hands_the_lead_on_first() {
    let mut c = a_client_with_three_players();
    open_the_fellowship_tab(&mut c);
    join(
        &mut c,
        &[(F_ME, "Larktest"), (F_FELLOW, "Bex")],
        F_ME,
        false,
    );

    let h = c
        .view()
        .expect_app()
        .hud()
        .panels
        .fellowship
        .quit_button
        .expect("bound");
    let always_lit = button_lit(&c, Some(h));
    let before = mark(&c);
    click(&mut c, h);
    // Handing the lead on, then leaving, in that order and in the same breath.
    let as_leader = fellowship_sent_since(&c, before)
        == vec![
            vec![0x90, 0x02, 0x00, 0x00, 0x1F, 0x00, 0x00, 0x50],
            vec![0xA3, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00],
        ];

    // The same button and the same press, in a fellowship somebody else leads.
    let mut member = a_client_with_three_players();
    open_the_fellowship_tab(&mut member);
    join(
        &mut member,
        &[(F_ME, "Larktest"), (F_FELLOW, "Bex")],
        F_FELLOW,
        false,
    );
    let h = member
        .view()
        .expect_app()
        .hud()
        .panels
        .fellowship
        .quit_button
        .expect("bound");
    let before = mark(&member);
    click(&mut member, h);
    let as_member = fellowship_sent_since(&member, before)
        == vec![vec![0xA3, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00]];
    member.shutdown();

    c.assert_behaviour(
        "fellowship.buttons.a-leader-who-leaves-hands-the-lead-on-first",
        move |_| always_lit && as_leader && as_member,
    );
    c.shutdown();
}

#[test]
fn scenario_a_leader_who_leaves_hands_the_lead_on_first() {
    scenario("a_leader_who_leaves_hands_the_lead_on_first");
}

/// Opening the fellowship flips the flag on the press and sends the new value.
pub fn opening_the_fellowship_changes_the_tab_first() {
    let mut c = a_client_with_three_players();
    open_the_fellowship_tab(&mut c);
    join(
        &mut c,
        &[(F_ME, "Larktest"), (F_FELLOW, "Bex")],
        F_ME,
        false,
    );
    let closed_to_start =
        c.view().world().fellowship.as_ref().map(|f| f.open_fellow) == Some(false);

    let h = c
        .view()
        .expect_app()
        .hud()
        .panels
        .fellowship
        .open_button
        .expect("bound");
    let lit = button_lit(&c, Some(h));
    let before = mark(&c);
    click(&mut c, h);
    let sent = fellowship_sent_since(&c, before)
        == vec![vec![0x91, 0x02, 0x00, 0x00, 0x01, 0x00, 0x00, 0x00]];
    // The client does not wait for the shard: the flag is already the new one.
    let flipped = c.view().world().fellowship.as_ref().map(|f| f.open_fellow) == Some(true);

    c.assert_behaviour(
        "fellowship.buttons.opening-the-fellowship-changes-the-tab-before-the-shard-answers",
        move |_| closed_to_start && lit && sent && flipped,
    );
    c.shutdown();
}

#[test]
fn scenario_opening_the_fellowship_changes_the_tab_first() {
    scenario("opening_the_fellowship_changes_the_tab_first");
}

/// Picking a row then dismissing names the fellow whose row was picked.
pub fn picking_a_row_then_dismissing_names_that_fellow() {
    let mut c = a_client_with_three_players();
    open_the_fellowship_tab(&mut c);
    join(
        &mut c,
        &[(F_ME, "Larktest"), (F_FELLOW, "Bex")],
        F_ME,
        false,
    );
    let dark_to_start = !button_lit(
        &c,
        c.view().expect_app().hud().panels.fellowship.dismiss_button,
    );

    let row = {
        let rows = c.view().expect_app().hud().panels.fellowship.rows();
        assert_eq!(rows[1].id, F_FELLOW, "the second row is the fellow's");
        rows[1].element
    };
    click(&mut c, row);
    let picked = c
        .view()
        .expect_app()
        .hud()
        .panels
        .fellowship
        .selected_fellow
        == Some(F_FELLOW);
    let armed = button_lit(
        &c,
        c.view().expect_app().hud().panels.fellowship.dismiss_button,
    );
    // Picking a fellow in the list also makes him the player's selection in the world.
    let selected = c.view().world().selected == Some(F_FELLOW);

    let h = c
        .view()
        .expect_app()
        .hud()
        .panels
        .fellowship
        .dismiss_button
        .expect("bound");
    let before = mark(&c);
    click(&mut c, h);
    let sent = fellowship_sent_since(&c, before)
        == vec![vec![0xA4, 0x00, 0x00, 0x00, 0x1F, 0x00, 0x00, 0x50]];

    c.assert_behaviour(
        "fellowship.buttons.picking-a-row-then-dismissing-names-that-fellow",
        move |_| dark_to_start && picked && armed && selected && sent,
    );
    c.shutdown();
}

#[test]
fn scenario_picking_a_row_then_dismissing_names_that_fellow() {
    scenario("picking_a_row_then_dismissing_names_that_fellow");
}

/// Recruit follows the world selection, and only for somebody who is not already in.
pub fn recruit_follows_the_world_selection() {
    let mut c = a_client_with_three_players();
    open_the_fellowship_tab(&mut c);
    join(
        &mut c,
        &[(F_ME, "Larktest"), (F_FELLOW, "Bex")],
        F_ME,
        false,
    );
    // Before: nothing is selected in the world at all.
    let nothing_selected = !button_lit(
        &c,
        c.view().expect_app().hud().panels.fellowship.recruit_button,
    );

    // Picking a row selects that fellow in the world -- and somebody who is already a member is
    // still refused.
    let row = c.view().expect_app().hud().panels.fellowship.rows()[1].element;
    click(&mut c, row);
    let member_refused = c.view().world().selected == Some(F_FELLOW)
        && !button_lit(
            &c,
            c.view().expect_app().hud().panels.fellowship.recruit_button,
        );

    // Somebody outside it. This is the world selection moving with no fellowship change at all, so
    // the tab has to notice it on its own.
    c.world_mut().selected = Some(F_OUTSIDER);
    c.tick(1);
    let h = c
        .view()
        .expect_app()
        .hud()
        .panels
        .fellowship
        .recruit_button
        .expect("bound");
    let offered = button_lit(&c, Some(h));
    let before = mark(&c);
    click(&mut c, h);
    // Named by who he is and not by his name.
    let sent = fellowship_sent_since(&c, before)
        == vec![vec![0xA5, 0x00, 0x00, 0x00, 0x20, 0x00, 0x00, 0x50]];

    c.assert_behaviour(
        "fellowship.buttons.recruit-follows-the-world-selection-and-not-the-list",
        move |_| nothing_selected && member_refused && offered && sent,
    );
    c.shutdown();
}

#[test]
fn scenario_recruit_follows_the_world_selection() {
    scenario("recruit_follows_the_world_selection");
}

/// The bytes a create would put on the wire, with `share_xp` as given.
fn create_bytes(share_xp: i32) -> Vec<u8> {
    let mut blob = vec![0xA2, 0x00, 0x00, 0x00];
    blob.extend(
        dereth_protocol::write_body(&dereth_protocol::social::FellowshipCreate {
            name: "Of The ring".to_owned(),
            share_xp,
        })
        .expect("the body encodes"),
    );
    blob
}

/// The name box gates the button, the typed name is tidied and written back, and the tick box is
/// what decides whether the fellowship shares experience.
pub fn the_name_box_gates_create_and_the_tick_box_is_the_source() {
    let mut c = a_client_with_three_players();
    open_the_fellowship_tab(&mut c);
    let (entry, create) = {
        let p = &c.view().expect_app().hud().panels.fellowship;
        (
            p.name_entry.expect("bound"),
            p.create_button.expect("bound"),
        )
    };

    // A keystroke that inserts nothing, on an empty box: the button stays dark. It is a real
    // keystroke rather than a poke at the model, because it is the keystroke that is the gate.
    on_the_shipped_tree(&mut c, |ui, _| {
        ui.set_focus_element(Some(entry));
        ui.character(0x09);
    });
    c.tick(1);
    let dark_on_empty = !button_lit(&c, Some(create));

    on_the_shipped_tree(&mut c, |ui, _| {
        for ch in "of The ring".encode_utf16() {
            ui.character(ch);
        }
        ui.set_focus_element(None);
    });
    c.tick(1);
    let lit_once_typed = button_lit(&c, Some(create));

    let before = mark(&c);
    click(&mut c, create);
    let sent = fellowship_sent_since(&c, before) == vec![create_bytes(1)];
    // The typed name is tidied on the press and written back into the box, and the tidying is not
    // a title-caser: the lower-case word the player typed stays lower-case.
    let written_back = on_the_shipped_tree(&mut c, |ui, _| {
        ui.text_element_mut(entry).map(|t| t.glyphs.inq_text(false))
    }) == Some("Of The ring".to_owned());

    // The tick box beside it is the source of the sharing flag, and it starts ticked.
    let checked = |c: &mut HeadlessClient| {
        let h = element(c, SHARE_XP_BOX);
        let ui = &c.view().expect_app().ui().expect("the shell is up").ui;
        dereth_ui_screens::bind::attr_bool(ui, h, dereth_ui_screens::options::toggle::ATTR_CHECKED)
            .unwrap_or(false)
    };
    let ticked_to_start = checked(&mut c);
    let box_handle = element(&mut c, SHARE_XP_BOX);
    click(&mut c, box_handle);
    // The model is the authority; the drawn tick catches up on the following frame.
    let model_changed = !c.view().world().player_system.options.get(15);
    c.tick(2);
    let drawn_unticked = !checked(&mut c);

    let before = mark(&c);
    click(&mut c, create);
    let unticked_sends_nought = fellowship_sent_since(&c, before) == vec![create_bytes(0)];

    c.assert_behaviour(
        "fellowship.create.the-name-box-gates-the-button-and-the-tick-box-is-the-source",
        move |_| {
            dark_on_empty
                && lit_once_typed
                && sent
                && written_back
                && ticked_to_start
                && model_changed
                && drawn_unticked
                && unticked_sends_nought
        },
    );
    c.shutdown();
}

#[test]
fn scenario_the_name_box_gates_create_and_the_tick_box_is_the_source() {
    scenario("the_name_box_gates_create_and_the_tick_box_is_the_source");
}

// =============================================================================================
// 29-33. The friends tab
// =============================================================================================

use dereth_ui_screens::panels::friends;

pub(crate) const FRIEND_A: ObjectId = ObjectId(0x5000_001E);
pub(crate) const FRIEND_B: ObjectId = ObjectId(0x5000_001F);
const FRIEND_C: ObjectId = ObjectId(0x5000_0031);

pub(crate) fn a_friend(
    id: ObjectId,
    name: &str,
    online: bool,
) -> dereth_protocol::social::FriendData {
    dereth_protocol::social::FriendData {
        id,
        online: i32::from(online),
        appear_offline: 0,
        name: name.to_owned(),
        friends_list: Vec::new(),
        friend_of_list: Vec::new(),
    }
}

pub(crate) fn friends_update(
    friends: Vec<dereth_protocol::social::FriendData>,
    update_type: u32,
) -> dereth_protocol::social::SocialFriendsUpdate {
    dereth_protocol::social::SocialFriendsUpdate {
        friends,
        update_type,
    }
}

/// The caption that opens the friends tab, **discovered** rather than named: walk the social page's
/// own tab table and take the page whose subtree carries the friends list. Naming the id would be a
/// guess; this is a measurement, and it fails loudly if the shipped tree disagrees.
fn friends_tab(c: &mut HeadlessClient) -> ElemHandle {
    on_the_shipped_tree(c, |ui, root| {
        let page = ui
            .get_child_recursive(root, SOCIAL_PAGE)
            .expect("the social page");
        let pairs: Vec<(ElementId, ElementId)> = ui
            .node(page)
            .and_then(|n| {
                n.behaviour
                    .as_ref()?
                    .as_any()?
                    .downcast_ref::<dereth_ui::widgets::panel::Panel>()
            })
            .expect("the social page is a tabbed container")
            .page_to_tab
            .iter()
            .map(|(p, t)| (*p, *t))
            .collect();
        for (page_id, tab_id) in pairs {
            let Some(pe) = ui.get_child_recursive(page, page_id) else {
                continue;
            };
            if ui.get_child_recursive(pe, friends::FRIENDS_LIST).is_some() {
                return ui
                    .get_child_recursive(page, tab_id)
                    .expect("the tab caption element");
            }
        }
        panic!("no page of the social panel carries the friends list");
    })
}

/// Open the social page and then its friends tab.
pub(crate) fn open_the_friends_tab(c: &mut HeadlessClient) {
    let b = social_button(c);
    click(c, b);
    let t = friends_tab(c);
    click(c, t);
    let h = c
        .view()
        .expect_app()
        .hud()
        .panels
        .friends
        .panel
        .expect("the tab is in the tree");
    assert!(
        c.view()
            .expect_app()
            .ui()
            .expect("the shell is up")
            .ui
            .is_visible(h),
        "clicking the caption must bring the friends tab up"
    );
}

/// The names in the shipped list box, in the order a player sees them.
///
/// It reads the live widget's own item order and **not** the element tree's child order, because
/// those are two different orders: a row is inserted into the widget's list at the sorted position
/// while its element is appended to the parent, and the widget's order is the one laid out. Reading
/// the children here reports creation order and makes the sorted insert look like a no-op.
fn friend_names(c: &mut HeadlessClient) -> Vec<String> {
    on_the_shipped_tree(c, |ui, root| {
        let list = ui
            .get_child_recursive(root, friends::FRIENDS_LIST)
            .expect("the friends list");
        let items: Vec<ElemHandle> = ui
            .node(list)
            .and_then(|n| {
                n.behaviour
                    .as_ref()?
                    .as_any()?
                    .downcast_ref::<dereth_ui::widgets::listbox::ListBox>()
            })
            .expect("the friends list is a list box")
            .items
            .clone();
        let texts: Vec<ElemHandle> = items
            .into_iter()
            .filter_map(|row| ui.get_child_recursive(row, friends::ROW_NAME))
            .collect();
        texts
            .into_iter()
            .filter_map(|t| ui.text_element_mut(t).map(|t| t.glyphs.inq_text(false)))
            .collect()
    })
}

/// Whether each row says its friend is there, in list order.
fn friend_row_states(c: &HeadlessClient) -> Vec<dereth_ui::StateId> {
    let app = c.view().expect_app();
    let ui = &app.ui().expect("the shell is up").ui;
    app.hud()
        .panels
        .friends
        .rows()
        .iter()
        .filter_map(|r| ui.get_child_recursive(r.element, friends::ROW_NAME))
        .filter_map(|t| ui.node(t).map(|n| n.state))
        .collect()
}

pub(crate) fn friends_button_lit(c: &HeadlessClient, h: Option<ElemHandle>) -> bool {
    h.is_some_and(|h| {
        friends::button_enabled(&c.view().expect_app().ui().expect("the shell is up").ui, h)
    })
}

/// A client with the friends tab open.
fn a_client_with_the_friends_tab_open() -> HeadlessClient {
    let mut c = a_client_in_gameplay();
    open_the_friends_tab(&mut c);
    c
}

/// The tab lists the friends the shard sent, with the online ones first.
pub fn the_friends_tab_lists_what_the_shard_sent_it() {
    let mut c = a_client_with_the_friends_tab_open();
    // Before: bound, driven, and empty -- which is a different reading from never having run.
    let (bound, driven, told) = {
        let app = c.view().expect_app();
        (
            app.hud().panels.friends.bound(),
            app.hud().panels.friends.rebuilds >= 1,
            app.hud().stats.friends_updates,
        )
    };
    let before = bound && driven && told == 0 && friend_names(&mut c).is_empty();

    // The empty list every recorded session's login carries. It really arrives and correctly draws
    // nothing, and those are two different facts.
    c.when(Inbound::message(&friends_update(Vec::new(), 0)));
    c.tick(1);
    let empty_arrived = {
        let app = c.view().expect_app();
        app.hud().stats.friends_updates == 1 && app.hud().stats.friends == 0
    } && friend_names(&mut c).is_empty();

    // Now a populated one: two on, one off.
    c.when(Inbound::message(&friends_update(
        vec![
            a_friend(FRIEND_C, "Zed", true),
            a_friend(FRIEND_A, "Ash", false),
            a_friend(FRIEND_B, "Bex", true),
        ],
        0,
    )));
    c.tick(1);
    let held = c.view().expect_app().hud().stats.friends == 3;
    // The online block first, each block in name order.
    let drawn = friend_names(&mut c) == vec!["Bex".to_owned(), "Zed".to_owned(), "Ash".to_owned()];
    let states = friend_row_states(&c)
        == vec![
            friends::ROW_STATE_ONLINE,
            friends::ROW_STATE_ONLINE,
            friends::ROW_STATE_OFFLINE,
        ];

    // Idempotent, and it flushes rather than appending.
    let rebuilds = c.view().expect_app().hud().panels.friends.rebuilds;
    c.tick(1);
    let unchanged = c.view().expect_app().hud().panels.friends.rebuilds == rebuilds
        && friend_names(&mut c).len() == 3;

    c.assert_behaviour(
        "friends.panel.the-tab-lists-the-friends-the-shard-sent-with-the-online-ones-first",
        move |_| before && empty_arrived && held && drawn && states && unchanged,
    );
    c.shutdown();
}

#[test]
fn scenario_the_friends_tab_lists_what_the_shard_sent_it() {
    scenario("the_friends_tab_lists_what_the_shard_sent_it");
}

/// Each kind of change the shard sends does its own thing to the list.
pub fn each_kind_of_friends_change_moves_the_list() {
    let mut c = a_client_with_the_friends_tab_open();
    c.when(Inbound::message(&friends_update(
        vec![a_friend(FRIEND_A, "Ash", true)],
        0,
    )));
    c.tick(1);
    let whole = friend_names(&mut c) == vec!["Ash".to_owned()];

    // Somebody added: inserted beside the others rather than replacing them.
    c.when(Inbound::message(&friends_update(
        vec![a_friend(FRIEND_B, "Bex", true)],
        1,
    )));
    c.tick(1);
    let added = c.view().expect_app().hud().stats.friends == 2
        && friend_names(&mut c) == vec!["Ash".to_owned(), "Bex".to_owned()];

    // The same person logging out: he moves into the other block without leaving the list.
    c.when(Inbound::message(&friends_update(
        vec![a_friend(FRIEND_B, "Bex", false)],
        4,
    )));
    c.tick(1);
    let logged_out = c.view().expect_app().hud().stats.friends == 2
        && friend_names(&mut c) == vec!["Ash".to_owned(), "Bex".to_owned()]
        && friend_row_states(&c) == vec![friends::ROW_STATE_ONLINE, friends::ROW_STATE_OFFLINE];

    // Removed, and removed without an announcement -- the same to the list either way.
    c.when(Inbound::message(&friends_update(
        vec![a_friend(FRIEND_B, "Bex", false)],
        2,
    )));
    c.tick(1);
    let removed = friend_names(&mut c) == vec!["Ash".to_owned()];
    c.when(Inbound::message(&friends_update(
        vec![a_friend(FRIEND_A, "Ash", true)],
        3,
    )));
    c.tick(1);
    let removed_quietly = friend_names(&mut c).is_empty();

    // A kind the client does not know changes nothing, and is counted rather than ignored.
    c.when(Inbound::message(&friends_update(
        vec![a_friend(FRIEND_A, "Ash", true)],
        9,
    )));
    c.tick(1);
    let unknown = c
        .view()
        .expect_app()
        .hud()
        .stats
        .friends_updates_unknown_type
        == 1
        && friend_names(&mut c).is_empty();

    c.assert_behaviour(
        "friends.update.each-kind-of-change-moves-the-list-on-its-own",
        move |_| whole && added && logged_out && removed && removed_quietly && unknown,
    );
    c.shutdown();
}

#[test]
fn scenario_each_kind_of_friends_change_moves_the_list() {
    scenario("each_kind_of_friends_change_moves_the_list");
}

/// Picking a row arms Remove, and pressing it names the friend whose row was picked.
pub fn picking_a_friend_arms_remove_and_the_press_names_him() {
    let mut c = a_client_with_the_friends_tab_open();
    c.when(Inbound::message(&friends_update(
        vec![
            a_friend(FRIEND_A, "Ash", true),
            a_friend(FRIEND_B, "Bex", true),
        ],
        0,
    )));
    c.tick(1);

    // Before: nothing picked, so neither removing nor telling is offered -- and adding is.
    let start = mark(&c);
    let before =
        {
            let p = &c.view().expect_app().hud().panels.friends;
            p.selected.is_none()
        } && !friends_button_lit(&c, c.view().expect_app().hud().panels.friends.remove_button)
            && !friends_button_lit(&c, c.view().expect_app().hud().panels.friends.tell_button)
            && friends_button_lit(&c, c.view().expect_app().hud().panels.friends.add_button)
            && social_sent_since(&c, start).is_empty();

    let row = {
        let rows = c.view().expect_app().hud().panels.friends.rows();
        assert_eq!(rows[0].id, FRIEND_A, "the first row is the first friend's");
        rows[0].element
    };
    click(&mut c, row);
    let picked = c.view().expect_app().hud().panels.friends.selected == Some(0);
    let armed = friends_button_lit(&c, c.view().expect_app().hud().panels.friends.remove_button)
        && friends_button_lit(&c, c.view().expect_app().hud().panels.friends.tell_button);

    let h = c
        .view()
        .expect_app()
        .hud()
        .panels
        .friends
        .remove_button
        .expect("bound");
    let before_press = mark(&c);
    click(&mut c, h);
    let named = social_sent_since(&c, before_press)
        == vec![vec![0x17, 0x00, 0x00, 0x00, 0x1E, 0x00, 0x00, 0x50]]
        && c.view().expect_app().hud().panels.friends.remove_requests == 1;

    c.assert_behaviour(
        "friends.buttons.picking-a-row-arms-remove-and-the-press-names-that-friend",
        move |_| before && picked && armed && named,
    );
    c.shutdown();
}

#[test]
fn scenario_picking_a_friend_arms_remove_and_the_press_names_him() {
    scenario("picking_a_friend_arms_remove_and_the_press_names_him");
}

/// Every friends message the client put in its outbox after `from`.
fn social_sent_since(c: &HeadlessClient, from: usize) -> Vec<Vec<u8>> {
    use dereth_protocol::Opcode;
    c.view().outbound()[from..]
        .iter()
        .filter_map(|r| {
            let (op, body) = match r {
                Request::SocialAddFriend(m) => {
                    (Opcode::SOCIAL_ADD_FRIEND, dereth_protocol::write_body(m))
                }
                Request::SocialRemoveFriend(m) => {
                    (Opcode::SOCIAL_REMOVE_FRIEND, dereth_protocol::write_body(m))
                }
                Request::SocialClearFriends(m) => {
                    (Opcode::SOCIAL_CLEAR_FRIENDS, dereth_protocol::write_body(m))
                }
                Request::SocialSendFriendsCommand(m) => (
                    Opcode::SOCIAL_SEND_FRIENDS_COMMAND,
                    dereth_protocol::write_body(m),
                ),
                _ => return None,
            };
            let mut blob = op.0.to_le_bytes().to_vec();
            blob.extend(body.expect("the body encodes"));
            Some(blob)
        })
        .collect()
}

/// Telling a friend is offered while he is online and refused while he is not.
pub fn tell_follows_whether_the_friend_is_online() {
    let mut c = a_client_with_the_friends_tab_open();
    c.when(Inbound::message(&friends_update(
        vec![
            a_friend(FRIEND_A, "Ash", true),
            a_friend(FRIEND_B, "Bex", false),
        ],
        0,
    )));
    c.tick(1);
    let ordered = friend_names(&mut c) == vec!["Ash".to_owned(), "Bex".to_owned()];

    let online_row = c.view().expect_app().hud().panels.friends.rows()[0].element;
    click(&mut c, online_row);
    let offered = friends_button_lit(&c, c.view().expect_app().hud().panels.friends.tell_button);

    let offline_row = c.view().expect_app().hud().panels.friends.rows()[1].element;
    click(&mut c, offline_row);
    let picked = c.view().expect_app().hud().panels.friends.selected == Some(1);
    // Still removable, but not tellable.
    let refused = friends_button_lit(&c, c.view().expect_app().hud().panels.friends.remove_button)
        && !friends_button_lit(&c, c.view().expect_app().hud().panels.friends.tell_button);

    c.assert_behaviour(
        "friends.buttons.tell-follows-whether-the-friend-is-online",
        move |_| ordered && offered && picked && refused,
    );
    c.shutdown();
}

#[test]
fn scenario_tell_follows_whether_the_friend_is_online() {
    scenario("tell_follows_whether_the_friend_is_online");
}

/// Typing a name arms Add, and the press sends the name and nothing else.
pub fn typing_a_name_arms_add_and_the_press_sends_it_alone() {
    let mut c = a_client_with_the_friends_tab_open();
    let entry = c
        .view()
        .expect_app()
        .hud()
        .panels
        .friends
        .name_entry
        .expect("bound");

    // A keystroke that inserts nothing, on an empty box: Add greys. The button starts lit because
    // the list is short, so this first keystroke is what proves the gate runs at all.
    let start = mark(&c);
    on_the_shipped_tree(&mut c, |ui, _| {
        ui.set_focus_element(Some(entry));
        ui.character(0x09);
    });
    c.tick(1);
    let dark_on_empty =
        !friends_button_lit(&c, c.view().expect_app().hud().panels.friends.add_button)
            && social_sent_since(&c, start).is_empty();

    on_the_shipped_tree(&mut c, |ui, _| {
        for ch in "Ash".encode_utf16() {
            ui.character(ch);
        }
        ui.set_focus_element(None);
    });
    c.tick(1);
    let lit = friends_button_lit(&c, c.view().expect_app().hud().panels.friends.add_button);

    let add = c
        .view()
        .expect_app()
        .hud()
        .panels
        .friends
        .add_button
        .expect("bound");
    let before = mark(&c);
    click(&mut c, add);
    // A length-prefixed name padded to four, and no room for anything else.
    let sent = social_sent_since(&c, before)
        == vec![vec![
            0x18, 0x00, 0x00, 0x00, 0x03, 0x00, b'A', b's', b'h', 0x00, 0x00, 0x00,
        ]]
        && c.view().expect_app().hud().panels.friends.add_requests == 1;
    let cleared = on_the_shipped_tree(&mut c, |ui, _| {
        ui.text_element_mut(entry).map(|t| t.glyphs.inq_text(false))
    }) == Some(String::new());
    let dark_again = !friends_button_lit(&c, c.view().expect_app().hud().panels.friends.add_button);

    c.assert_behaviour(
        "friends.buttons.typing-a-name-arms-add-and-the-press-sends-the-name-alone",
        move |_| dark_on_empty && lit && sent && cleared && dark_again,
    );
    c.shutdown();
}

#[test]
fn scenario_typing_a_name_arms_add_and_the_press_sends_it_alone() {
    scenario("typing_a_name_arms_add_and_the_press_sends_it_alone");
}
