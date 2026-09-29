//! Fellowship membership, allegiance, and the things in the interface that follow them.
//!
//! These are the model halves: the fellowship receivers, the allegiance tree walk, the one piece
//! of chat state the allegiance tab writes, the fellowship lines and the friends list's bytes and
//! commands. They drive a model-only client with messages through `Inbound` and requests through
//! `Player`, need no shipped layout, and so belong on this side of the tier line; the halves that
//! read the shipped panels are in `tests/dat/social.rs`.

use std::collections::BTreeMap;

use dereth_client_contract::UiRequest;
use dereth_client_model::chat::TalkFocus;
use dereth_client_model::selection::{SelectionPhysics, SelectionType};
use dereth_client_model::weenie::{bitfield, item_type};
use dereth_primitives::{CellId, ObjectId};
use dereth_protocol::social::{
    Fellow, Fellowship, FellowshipDisband, FellowshipDismiss, FellowshipFullUpdate,
    FellowshipQuitNotice, FellowshipUpdateFellow,
};
use dereth_testkit::{HeadlessClient, Inbound, Player};
use dereth_ui_screens::view::GameView as _;

/// Every scenario in this file, for the census: the name, **the behaviour ids the
/// scenario asserts**, and the function.
pub static ALL: &[dereth_testkit::behaviours::Scenario] = &[
    (
        "fellowship_membership_moves_the_cycle_and_the_tab",
        &["fellowship.membership.moves-the-tab-target-cycle-and-the-chat-tab"],
        fellowship_membership_moves_the_cycle_and_the_tab,
    ),
    (
        "the_allegiance_tree_is_walked_from_the_player_outwards",
        &["allegiance.roster.is-walked-from-the-player-outwards-and-titled-by-rank"],
        the_allegiance_tree_is_walked_from_the_player_outwards,
    ),
    (
        "the_tabs_request_writes_the_one_talk_focus_mask",
        &["allegiance.channels.the-panels-request-writes-the-one-talk-focus-mask"],
        the_tabs_request_writes_the_one_talk_focus_mask,
    ),
    (
        "a_roster_the_shard_really_sent_is_read_whole",
        &["allegiance.roster.a-roster-the-shard-really-sent-is-read-whole"],
        a_roster_the_shard_really_sent_is_read_whole,
    ),
    (
        "a_confirmed_question_sends_the_oath_or_the_break",
        &["allegiance.buttons.a-confirmed-question-is-what-sends-the-oath-or-the-break"],
        a_confirmed_question_sends_the_oath_or_the_break,
    ),
    (
        "the_recorded_fellowship_story_becomes_the_lines_the_player_read",
        &["fellowship.lines.the-recorded-story-becomes-the-lines-the-player-read"],
        the_recorded_fellowship_story_becomes_the_lines_the_player_read,
    ),
    (
        "a_fellowship_refusal_code_prints_its_own_line_or_nothing",
        &["fellowship.lines.a-refusal-code-prints-its-own-line-or-nothing-at-all"],
        a_fellowship_refusal_code_prints_its_own_line_or_nothing,
    ),
    (
        "a_fellow_is_known_as_one_on_the_radar",
        &["fellowship.membership.colours-a-fellow-on-the-radar"],
        a_fellow_is_known_as_one_on_the_radar,
    ),
    (
        "a_fellowship_invitation_reaches_the_tab_that_asks",
        &["fellowship.invitation.reaches-the-tab-that-asks-the-player"],
        a_fellowship_invitation_reaches_the_tab_that_asks,
    ),
    (
        "a_populated_friends_list_is_read_whole_and_written_back",
        &["friends.update.a-populated-list-is-read-whole-and-written-back-unchanged"],
        a_populated_friends_list_is_read_whole_and_written_back,
    ),
    (
        "adding_a_friend_by_name_sends_the_name",
        &["friends.commands.adding-by-name-sends-the-name-the-player-typed"],
        adding_a_friend_by_name_sends_the_name,
    ),
    (
        "removing_a_friend_by_name_names_who_it_is",
        &["friends.commands.removing-by-name-names-who-it-is-or-sends-nothing"],
        removing_a_friend_by_name_names_who_it_is,
    ),
    (
        "listing_friends_is_local_and_in_the_tabs_order",
        &["friends.commands.listing-is-local-and-in-the-order-the-tab-shows"],
        listing_friends_is_local_and_in_the_tabs_order,
    ),
    (
        "the_old_friends_form_goes_out_on_the_other_queue",
        &["friends.commands.the-old-form-goes-out-on-the-other-queue"],
        the_old_friends_form_goes_out_on_the_other_queue,
    ),
    (
        "allegiance_login_becomes_a_chat_line",
        &["allegiance.member-login.becomes-a-chat-line"],
        allegiance_login_becomes_a_chat_line,
    ),
];

/// Run one of this file's scenarios under a recorder, and check that the behaviour ids it
/// asserted are exactly the ones its [`ALL`] entry declares.
fn scenario(name: &str) {
    dereth_testkit::behaviours::run_scenario(ALL, name);
}

const PLAYER: ObjectId = ObjectId(0x5000_0001);
const FELLOW: ObjectId = ObjectId(0x5000_0002);
const STRANGER: ObjectId = ObjectId(0x5000_0003);

fn a_fellow(name: &str, level: u32) -> Fellow {
    Fellow {
        name: name.to_owned(),
        level,
        cp_cache: 0,
        lum_cache: 0,
        share_loot: 0,
        max_health: 100,
        max_stamina: 100,
        max_mana: 100,
        current_health: 100,
        current_stamina: 100,
        current_mana: 100,
    }
}

/// A membership list holding exactly `members`, led by `leader`.
fn full_update(
    members: &[(ObjectId, &str)],
    leader: ObjectId,
    locked: bool,
) -> FellowshipFullUpdate {
    let mut f = Fellowship {
        name: "Test Fellowship".to_owned(),
        leader,
        share_xp: 1,
        even_xp_split: 1,
        open_fellow: 0,
        locked: i32::from(locked),
        ..Fellowship::default()
    };
    f.members.table_size = 8;
    for (id, name) in members {
        f.members.entries.push((id.0, a_fellow(name, 20)));
    }
    f.fellows_departed.table_size = 8;
    FellowshipFullUpdate(f)
}

/// The physics a creature needs for the target key's candidate filter to have a real opinion.
fn candidate_physics() -> SelectionPhysics {
    SelectionPhysics {
        player_space: (5.0, 5.0, 0.0),
        cloaked: false,
        reports_collisions_as_environment: false,
    }
}

/// A player and two creatures built identically -- attackable, visible, not vendors -- so that
/// the **only** thing that can separate them afterwards is the membership list.
fn two_identical_candidates() -> (HeadlessClient, BTreeMap<ObjectId, SelectionPhysics>) {
    let mut c = HeadlessClient::model();
    let mut physics = BTreeMap::new();
    {
        let w = c.world_mut();
        let mut me = dereth_client_model::Weenie::new(PLAYER);
        me.valid = true;
        me.has_phys_obj = true;
        w.tables.weenies.insert(PLAYER, me);
        w.player = Some(PLAYER);
        for id in [FELLOW, STRANGER] {
            let mut it = dereth_client_model::Weenie::new(id);
            it.valid = true;
            it.has_phys_obj = true;
            it.pwd.obj_type |= item_type::CREATURE;
            it.pwd.bitfield |= bitfield::ATTACKABLE;
            it.pwd.radar_enum = Some(4);
            w.tables.weenies.insert(id, it);
            w.tables.physics.insert(
                id,
                dereth_client_model::objects::PhysicsPresence {
                    cell: Some(CellId(0x00A9_0100)),
                    state: 0,
                    parent: None,
                    setup_id: 0,
                },
            );
            physics.insert(id, candidate_physics());
            assert!(
                w.object_is_attackable(id),
                "the premise: {id:?} is attackable"
            );
        }
    }
    (c, physics)
}

fn skipped(
    c: &HeadlessClient,
    physics: &BTreeMap<ObjectId, SelectionPhysics>,
    id: ObjectId,
) -> bool {
    c.view()
        .world()
        .selection_type_rejects(SelectionType::Monster, id, &physics[&id])
}

fn focus_changes(c: &mut HeadlessClient) -> Vec<(TalkFocus, bool)> {
    c.world_mut()
        .chat
        .take_talk_focus_notices()
        .into_iter()
        .map(|n| (n.focus, n.enabled))
        .collect()
}

/// Membership moves both the target cycle and the chat tab, and only for the people in it.
pub fn fellowship_membership_moves_the_cycle_and_the_tab() {
    let (mut c, physics) = two_identical_candidates();
    let both_targets = !skipped(&c, &physics, FELLOW) && !skipped(&c, &physics, STRANGER);
    let no_tab_yet = focus_changes(&mut c).is_empty();

    c.when(Inbound::message(&full_update(
        &[(PLAYER, "Me"), (FELLOW, "Bex")],
        PLAYER,
        false,
    )));
    let joined = skipped(&c, &physics, FELLOW) && !skipped(&c, &physics, STRANGER);
    let tab_on = focus_changes(&mut c) == vec![(TalkFocus::Fellowship, true)];
    let counted = c.view().hud().stats.fellowships_created == 1
        && c.view().hud().stats.fellowship_members == 2;

    // A second list replaces rather than merges: the member who is not in it is a target again.
    c.when(Inbound::message(&full_update(
        &[(PLAYER, "Me"), (STRANGER, "Caius")],
        PLAYER,
        false,
    )));
    let replaced = !skipped(&c, &physics, FELLOW)
        && skipped(&c, &physics, STRANGER)
        && c.view().hud().stats.fellowships_created == 1
        && c.view().hud().stats.fellowship_leader_changes == 0;
    c.when(Inbound::message(&full_update(
        &[(PLAYER, "Me"), (STRANGER, "Caius")],
        STRANGER,
        false,
    )));
    let leader_moved = c.view().hud().stats.fellowship_leader_changes == 1
        && c.view().hud().stats.fellowships_created == 1;

    // One member added on their own leaves the cycle the same way.
    let (mut one, physics_one) = two_identical_candidates();
    one.when(Inbound::message(&full_update(
        &[(PLAYER, "Me")],
        PLAYER,
        false,
    )));
    let not_yet = !skipped(&one, &physics_one, FELLOW);
    one.when(Inbound::message(&FellowshipUpdateFellow {
        fellow_id: FELLOW,
        fellow: a_fellow("Bex", 20),
        update_type: 1,
    }));
    let added = skipped(&one, &physics_one, FELLOW)
        && one.view().hud().stats.fellows_added == 1
        && one.view().hud().stats.fellowship_members == 2;
    // A second message about the same member updates rather than adds.
    one.when(Inbound::message(&FellowshipUpdateFellow {
        fellow_id: FELLOW,
        fellow: a_fellow("Bex", 21),
        update_type: 2,
    }));
    let updated = one.view().hud().stats.fellows_added == 1
        && one.view().hud().stats.fellow_updates == 2
        && one
            .view()
            .world()
            .fellowship
            .as_ref()
            .is_some_and(|f| f.members[&FELLOW].level == 21);

    // An update with no fellowship at all is refused and says so, rather than crashing.
    let (mut orphan, _) = two_identical_candidates();
    orphan.when(Inbound::message(&FellowshipUpdateFellow {
        fellow_id: FELLOW,
        fellow: a_fellow("Bex", 20),
        update_type: 1,
    }));
    let refused = orphan
        .view()
        .hud()
        .stats
        .fellow_updates_without_a_fellowship
        == 1
        && orphan.view().hud().stats.fellow_updates == 0
        && orphan.view().hud().stats.undecodable == 0;

    // A disband puts everybody back and turns the tab off.
    let (mut band, physics_band) = two_identical_candidates();
    band.when(Inbound::message(&full_update(
        &[(PLAYER, "Me"), (FELLOW, "Bex")],
        PLAYER,
        false,
    )));
    let _ = focus_changes(&mut band);
    band.when(Inbound::message(&FellowshipDisband));
    let disbanded = !skipped(&band, &physics_band, FELLOW)
        && focus_changes(&mut band) == vec![(TalkFocus::Fellowship, false)]
        && band.view().hud().stats.fellowship_members == 0;

    // Somebody else leaving removes one row and leaves your own tab alone.
    let (mut theirs, physics_theirs) = two_identical_candidates();
    theirs.when(Inbound::message(&full_update(
        &[(PLAYER, "Me"), (FELLOW, "Bex"), (STRANGER, "Caius")],
        PLAYER,
        false,
    )));
    let _ = focus_changes(&mut theirs);
    theirs.when(Inbound::message(&FellowshipQuitNotice { member: FELLOW }));
    let they_left = !skipped(&theirs, &physics_theirs, FELLOW)
        && skipped(&theirs, &physics_theirs, STRANGER)
        && theirs.view().world().fellowship.is_some()
        && focus_changes(&mut theirs).is_empty();

    // …and you leaving deletes the fellowship and turns it off.
    let (mut mine, physics_mine) = two_identical_candidates();
    mine.when(Inbound::message(&full_update(
        &[(PLAYER, "Me"), (FELLOW, "Bex")],
        PLAYER,
        false,
    )));
    let _ = focus_changes(&mut mine);
    mine.when(Inbound::message(&FellowshipQuitNotice { member: PLAYER }));
    let i_left = mine.view().world().fellowship.is_none()
        && !skipped(&mine, &physics_mine, FELLOW)
        && focus_changes(&mut mine) == vec![(TalkFocus::Fellowship, false)]
        && mine.view().hud().stats.fellowship_departures_our_own == 1;

    // A dismissal is the same branch under its own counter -- "you quit" and "you were
    // dismissed" are different things to show a player.
    let (mut sacked, physics_sacked) = two_identical_candidates();
    sacked.when(Inbound::message(&full_update(
        &[(PLAYER, "Me"), (FELLOW, "Bex")],
        PLAYER,
        false,
    )));
    sacked.when(Inbound::message(&FellowshipDismiss { target: FELLOW }));
    let dismissed = !skipped(&sacked, &physics_sacked, FELLOW)
        && sacked.view().hud().stats.fellowship_dismissals == 1
        && sacked.view().hud().stats.fellowship_quits == 0;

    // A departure from a locked fellowship is remembered so they can be re-admitted.
    let mut remembered = true;
    for locked in [false, true] {
        let (mut l, _) = two_identical_candidates();
        l.when(Inbound::message(&full_update(
            &[(PLAYER, "Me"), (FELLOW, "Bex")],
            PLAYER,
            locked,
        )));
        l.when(Inbound::message(&FellowshipQuitNotice { member: FELLOW }));
        remembered &= l
            .view()
            .world()
            .fellowship
            .as_ref()
            .is_some_and(|f| f.fellows_departed.contains_key(&FELLOW) == locked);
    }

    c.assert_behaviour(
        "fellowship.membership.moves-the-tab-target-cycle-and-the-chat-tab",
        move |_| {
            both_targets
                && no_tab_yet
                && joined
                && tab_on
                && counted
                && replaced
                && leader_moved
                && not_yet
                && added
                && updated
                && refused
                && disbanded
                && they_left
                && i_left
                && dismissed
                && remembered
        },
    );
}

// -------------------------------------------------------------------------------------------

#[test]
fn scenario_fellowship_membership_moves_the_cycle_and_the_tab() {
    scenario("fellowship_membership_moves_the_cycle_and_the_tab");
}

// =============================================================================================
// allegiance.roster.is-walked-from-the-player-outwards-and-titled-by-rank
// =============================================================================================

const MONARCH: ObjectId = ObjectId(0x5000_0009);
const PATRON: ObjectId = ObjectId(0x5000_0005);
const VASSAL_OLD: ObjectId = ObjectId(0x5000_0010);
const VASSAL_NEW: ObjectId = ObjectId(0x5000_0011);
const VASSALS_VASSAL: ObjectId = ObjectId(0x5000_0020);

/// ```text
/// Bob   (monarch, online)
/// +- Cid          (the player's patron, offline)
///    +- Lark      (the player)
///       +- Eve    \ added second -> head insertion puts it first
///       +- Dee    / added first
///          +- Fay (a vassal's vassal: the node the walk must not reach)
/// ```
///
/// **Synthesised**, and declared so: there is no recording of a character inside a populated
/// allegiance. It is delivered as an encoded message through the client's own receiver, so what the
/// walk runs over came off the wire and not out of a struct literal.
fn a_populated_allegiance() -> dereth_protocol::social::AllegianceUpdate {
    use dereth_testkit::adapters_social::{allegiance_answer, allegiance_member};
    allegiance_answer(
        "The Hand of Dereth",
        6,
        2,
        vec![
            (None, allegiance_member(MONARCH, "Bob", 9, true)),
            (Some(MONARCH), allegiance_member(PATRON, "Cid", 5, false)),
            (Some(PATRON), allegiance_member(PLAYER, "Lark", 3, true)),
            (Some(PLAYER), allegiance_member(VASSAL_OLD, "Dee", 1, true)),
            (Some(PLAYER), allegiance_member(VASSAL_NEW, "Eve", 1, false)),
            (
                Some(VASSAL_OLD),
                allegiance_member(VASSALS_VASSAL, "Fay", 1, true),
            ),
        ],
    )
}

/// The player's monarch, patron and own vassals -- and not a vassal's vassal -- each titled by the
/// rank the shard gave him; and a character in no allegiance walks to nothing at all.
pub fn the_allegiance_tree_is_walked_from_the_player_outwards() {
    let mut c = HeadlessClient::model();
    c.given(dereth_testkit::Given::APlayer(PLAYER));

    // **The control, and it is the case the corpus can witness**: every recorded allegiance answer
    // carries no member at all. A world that has never seen one walks to an empty roster, and the
    // walk is observably a walk rather than an early return -- the player is reported as absent
    // from a tree he has no node in, not as present with nothing around him.
    let before = c.view().hud().view(c.view().objects()).allegiance_roster();
    let empty_to_start = before.subject.is_none()
        && before.monarch.is_none()
        && before.patron.is_none()
        && before.vassals.is_empty()
        && (before.total_members, before.total_vassals) == (0, 0);

    c.when(Inbound::message(&a_populated_allegiance()));
    let r = c.view().hud().view(c.view().objects()).allegiance_roster();

    let header =
        (r.total_members, r.total_vassals) == (6, 2) && r.allegiance_name == "The Hand of Dereth";
    let walked = r.monarch.as_ref().map(|e| e.id) == Some(MONARCH)
        && r.patron.as_ref().map(|e| e.id) == Some(PATRON)
        && r.subject.as_ref().map(|e| e.id) == Some(PLAYER)
        && r.vassals.iter().map(|e| e.id).collect::<Vec<_>>() == vec![VASSAL_NEW, VASSAL_OLD];
    // The vassal's vassal is the node a walk that descended would find, and it must not be here:
    // the step from one vassal to the next is a step along the list and not into the subtree.
    let did_not_descend = !r.vassals.iter().any(|e| e.id == VASSALS_VASSAL);
    let titled = r
        .monarch
        .as_ref()
        .is_some_and(|e| e.full_name == "King Bob")
        && r.patron
            .as_ref()
            .is_some_and(|e| e.full_name == "Thane Cid")
        && r.subject
            .as_ref()
            .is_some_and(|e| e.full_name == "Baron Lark")
        && r.vassals[0].full_name == "Yeoman Eve"
        && r.vassals[1].full_name == "Yeoman Dee";
    // Who is online, and what each has passed up, carried per entry.
    let carried = !r.vassals[0].logged_in
        && r.vassals[1].logged_in
        && r.vassals[0].cp_cached == 7 * (VASSAL_NEW.0 & 0xFFF)
        && r.vassals[1].cp_cached == 7 * (VASSAL_OLD.0 & 0xFFF);

    c.assert_behaviour(
        "allegiance.roster.is-walked-from-the-player-outwards-and-titled-by-rank",
        move |_| empty_to_start && header && walked && did_not_descend && titled && carried,
    );
}

#[test]
fn scenario_the_allegiance_tree_is_walked_from_the_player_outwards() {
    scenario("the_allegiance_tree_is_walked_from_the_player_outwards");
}

// =============================================================================================
// allegiance.channels.the-panels-request-writes-the-one-talk-focus-mask
// =============================================================================================

/// The tab's request lands on the one piece of chat state the talk-to menu reads, and a row that is
/// not a channel is ignored rather than written.
pub fn the_tabs_request_writes_the_one_talk_focus_mask() {
    let mut c = HeadlessClient::model();
    c.given(dereth_testkit::Given::APlayer(PLAYER));
    let off_to_start = !c
        .view()
        .world()
        .chat
        .is_talk_focus_enabled(TalkFocus::Vassals);

    c.when(Player::Ui(vec![
        UiRequest::SetTalkFocusEnabled {
            focus: 6,
            enabled: true,
        },
        UiRequest::SetTalkFocusEnabled {
            focus: 5,
            enabled: true,
        },
        // Not a channel. The menu has fourteen rows and this is not one of them.
        UiRequest::SetTalkFocusEnabled {
            focus: 99,
            enabled: true,
        },
    ]));
    let written = {
        let chat = &c.view().world().chat;
        chat.is_talk_focus_enabled(TalkFocus::Vassals)
            && chat.is_talk_focus_enabled(TalkFocus::Monarch)
            && !chat.is_talk_focus_enabled(TalkFocus::Patron)
    };
    // One notice per legal write, and none at all for the row that is not a channel.
    let told = c.world_mut().chat.take_talk_focus_notices().len() == 2;

    c.when(Player::ui(UiRequest::SetTalkFocusEnabled {
        focus: 6,
        enabled: false,
    }));
    let closes_again = !c
        .view()
        .world()
        .chat
        .is_talk_focus_enabled(TalkFocus::Vassals);

    c.assert_behaviour(
        "allegiance.channels.the-panels-request-writes-the-one-talk-focus-mask",
        move |_| off_to_start && written && told && closes_again,
    );
}

#[test]
fn scenario_the_tabs_request_writes_the_one_talk_focus_mask() {
    scenario("the_tabs_request_writes_the_one_talk_focus_mask");
}

// =============================================================================================
// allegiance.roster.a-roster-the-shard-really-sent-is-read-whole
// =============================================================================================

/// The one recorded allegiance answer with members in it: three of them, from a live shard, to the
/// character the recording was made on. Everything above this line is a roster this client encoded
/// for itself, and a round trip cannot see a reader that is self-consistently wrong -- which is the
/// defect this claim exists to catch.
const RECORDED_SESSION: &str = "fellowship-one-vassal";
const RECORDED_ROSTER: usize = 351;

/// The three characters in it, and the shape of the tree: the monarch, the recorded character under
/// him, and the recorded character's own one vassal.
const RECORDED_ME: ObjectId = ObjectId(0x5000_001E);
const RECORDED_MONARCH: ObjectId = ObjectId(0x5000_001F);
const RECORDED_VASSAL: ObjectId = ObjectId(0x5000_0020);

/// Every member of a shard's own roster reaches the model with the shard's own values on him.
pub fn a_roster_the_shard_really_sent_is_read_whole() {
    let mut c = HeadlessClient::model();
    c.given(dereth_testkit::Given::APlayer(RECORDED_ME));
    c.when(Inbound::from_corpus(
        RECORDED_SESSION,
        RECORDED_ROSTER..RECORDED_ROSTER + 1,
    ));

    /// Everything but the name, which is the recording's and is read separately: the names in the
    /// public corpus are stand-ins the scrubber chose, so pinning one here would be pinning the
    /// scrubber rather than the shard.
    type Fields = (u8, u8, u16, u32, u16, u16, u32, u32, bool);
    let fields = |id: ObjectId| -> Option<Fields> {
        c.view().world().allegiance.look_up(id).map(|d| {
            (
                d.gender,
                d.hg,
                d.rank,
                d.level,
                d.loyalty,
                d.leadership,
                d.cp_cached,
                d.cp_tithed,
                d.is_logged_in(),
            )
        })
    };
    let name = |id: ObjectId| {
        c.view()
            .world()
            .allegiance
            .look_up(id)
            .map(|d| d.name.clone())
    };

    // **Every one of these is at a different place on the wire and five of them are a different
    // width from the next**, so a reader that agreed with itself rather than with the shard would
    // put a wrong number in almost every one. The monarch: male, Aluvian, rank 1, level 1, and the
    // two numbers the shard keeps about how loyal and how able he is.
    let monarch = fields(RECORDED_MONARCH) == Some((1, 1, 1, 1, 10, 5, 0, 0, true));
    // The recorded character is a different sex and a different people from his monarch, and his
    // own vassal is a third people again -- so a reader that had those two swapped, or read one
    // byte where the other is, could not put all three right.
    let me = fields(RECORDED_ME) == Some((2, 4, 1, 1, 10, 5, 0, 0, true));
    let vassal = fields(RECORDED_VASSAL) == Some((2, 3, 1, 1, 10, 5, 0, 0, true));

    // The names are last on the wire, after every optional block, so a member whose name arrives
    // whole is a member the reader walked to the end of. Three of them, each its own.
    let names: Vec<String> = [RECORDED_MONARCH, RECORDED_ME, RECORDED_VASSAL]
        .into_iter()
        .filter_map(name)
        .collect();
    let named = names.len() == 3
        && names
            .iter()
            .all(|n| !n.is_empty() && n.chars().all(char::is_alphanumeric))
        && names
            .iter()
            .collect::<std::collections::BTreeSet<_>>()
            .len()
            == 3;

    // And the tree: each member hangs off the one the shard said he follows.
    let ids = c.view().world().allegiance_roster_ids();
    let shaped = ids.subject == Some(RECORDED_ME)
        && ids.monarch == Some(RECORDED_MONARCH)
        && ids.patron == Some(RECORDED_MONARCH)
        && ids.vassals == vec![RECORDED_VASSAL];

    c.assert_behaviour(
        "allegiance.roster.a-roster-the-shard-really-sent-is-read-whole",
        move |_| monarch && me && vassal && named && shaped,
    );
}

#[test]
fn scenario_a_roster_the_shard_really_sent_is_read_whole() {
    scenario("a_roster_the_shard_really_sent_is_read_whole");
}

// =============================================================================================
// allegiance.buttons.a-confirmed-question-is-what-sends-the-oath-or-the-break
// =============================================================================================

/// What the three answered questions put on the wire.
///
/// The gestures that raise the questions are the shipped tab's and are asserted in the dat tier;
/// this is the other half -- what a **yes** actually sends. It is driven straight at the game model
/// because the answer arrives through the dialog framework, which is the shell's, and the claim is
/// about the three actions and not about the dialog.
pub fn a_confirmed_question_sends_the_oath_or_the_break() {
    use dereth_client_model::{RecordingRequests, Request};

    let mut c = HeadlessClient::model();
    c.given(dereth_testkit::Given::APlayer(RECORDED_ME));
    let body = |r: &Request| match r {
        Request::SwearAllegiance(m) => (
            0x001D_u32,
            dereth_protocol::write_body(m).expect("the body encodes"),
        ),
        Request::BreakAllegiance(m) => (
            0x001E_u32,
            dereth_protocol::write_body(m).expect("the body encodes"),
        ),
        other => panic!("not an allegiance action: {other:?}"),
    };

    let mut req = RecordingRequests::default();
    // Swearing: to the object the player had selected.
    c.world_mut().swear_allegiance(&mut req, RECORDED_MONARCH);
    let swear = body(&req.0[0]) == (0x001D, vec![0x1F, 0x00, 0x00, 0x50]);
    // Kicking a vassal: the same message about a different person.
    c.world_mut().break_allegiance(&mut req, RECORDED_VASSAL);
    let kick = body(&req.0[1]) == (0x001E, vec![0x20, 0x00, 0x00, 0x50]);
    // Breaking with no tree loaded: there is no patron to name, so nothing is sent rather than a
    // message about nobody.
    let nobody = c
        .world_mut()
        .break_allegiance_from_patron(&mut req)
        .is_none()
        && req.0.len() == 2;

    // With the shard's own roster applied, the patron is looked up again as the question is
    // answered, and that is who the break names.
    c.when(Inbound::from_corpus(
        RECORDED_SESSION,
        RECORDED_ROSTER..RECORDED_ROSTER + 1,
    ));
    let named = c.world_mut().break_allegiance_from_patron(&mut req) == Some(RECORDED_MONARCH);
    let broke = body(&req.0[2]) == (0x001E, vec![0x1F, 0x00, 0x00, 0x50]);

    c.assert_behaviour(
        "allegiance.buttons.a-confirmed-question-is-what-sends-the-oath-or-the-break",
        move |_| swear && kick && nobody && named && broke,
    );
}

#[test]
fn scenario_a_confirmed_question_sends_the_oath_or_the_break() {
    scenario("a_confirmed_question_sends_the_oath_or_the_break");
}

// =============================================================================================
// fellowship.lines.*
// =============================================================================================
//
// **The names in the expected lines are the public corpus's stand-ins, not the characters' own.**
// The recordings are pseudonymised, so a line the shard composed around a name arrives with a
// same-length stand-in in it; the assertion is the same one either way -- the whole line, verbatim,
// in the order the player read it.

/// The messages the fellowship lines are composed from: the two refusal shapes, the membership
/// update, the disband, the dismissal, the quit and the join.
const FELLOWSHIP_FAMILY: [u32; 7] = [0x028A, 0x028B, 0x02BE, 0x02C0, 0x02BF, 0x00A3, 0x00A4];

/// The character each of the three recordings was made on.
fn recorded_player(session: &str) -> ObjectId {
    match session {
        "fellowship-one-vassal" => ObjectId(0x5000_001E),
        "fellowship-two-monarch" => ObjectId(0x5000_001F),
        "fellowship-three-vassal" => ObjectId(0x5000_0020),
        other => panic!("{other} is not one of the three fellowship recordings"),
    }
}

/// The index and unwrapped opcode of every blob of `session` in the family, in recorded order.
///
/// A game event carries its own header before the message, so the message starts twelve bytes in;
/// everything else starts at its own opcode.
fn family_blobs(session: &'static str) -> Vec<(usize, u32)> {
    use dereth_client_net::client_session::testing::{Corpus, Direction};
    let corpus = Corpus::load(session)
        .expect("the corpus decodes")
        .unwrap_or_else(|| panic!("the corpus holds {session}"));
    let out: Vec<(usize, u32)> = corpus
        .blobs
        .iter()
        .filter(|b| b.dir == Direction::ServerToClient)
        .filter_map(|b| {
            let op = if b.opcode == 0xF7B0 {
                u32::from_le_bytes(b.payload.get(12..16)?.try_into().ok()?)
            } else {
                b.opcode
            };
            FELLOWSHIP_FAMILY.contains(&op).then_some((b.idx, op))
        })
        .collect();
    assert!(!out.is_empty(), "{session} carries the fellowship family");
    out
}

/// Replay one recording's fellowship family, one blob at a time, and keep every line it produced
/// beside the blob that produced it.
fn fellowship_lines(session: &'static str) -> Vec<(usize, u8, String)> {
    let mut c = HeadlessClient::model();
    assert!(
        c.world_mut().set_player(recorded_player(session)),
        "the recorded character"
    );
    let mut out = Vec::new();
    let mut seen = 0;
    for (idx, _op) in family_blobs(session) {
        c.when(Inbound::from_corpus(session, idx..idx + 1));
        let lines = c.view().chat_lines();
        for m in &lines[seen..] {
            // Every one of these is a broadcast with no speaker in front of it, and the trailing
            // newline the shard's literal carries is taken off before it is drawn.
            assert!(
                m.prefix.is_none() && m.window == 0 && !m.body.ends_with('\n'),
                "{m:?}"
            );
            out.push((idx, m.ty, m.body.clone()));
        }
        seen = lines.len();
    }
    // The channel-membership lines are in every recording and are asserted by code below; leaving
    // them out here lets the fellowship story read on its own.
    out.retain(|(_, _, body)| {
        !body.starts_with("You have entered the ") && !body.starts_with("You have left the ")
    });
    out
}

/// Three recorded sessions, each producing exactly the lines that player read.
pub fn the_recorded_fellowship_story_becomes_the_lines_the_player_read() {
    let one = fellowship_lines("fellowship-one-vassal");
    let two = fellowship_lines("fellowship-two-monarch");
    let three = fellowship_lines("fellowship-three-vassal");

    let line = |idx: usize, ty: u8, body: &str| (idx, ty, body.to_owned());
    // **One** is recruited by the leader, quits twice, watches the lead change hands twice, sees a
    // member dismissed, the fellowship opened, and finally disbanded.
    let one_holds = one
        == vec![
            line(156, 0x1A, "Your offer of Allegiance has been ignored."),
            line(621, 0, "You have been recruited into the Of The ring fellowship, a closed fellowship led by Bex."),
            line(627, 0, "You are no longer a member of the Of The ring Fellowship."),
            line(634, 0, "You have been recruited into the Of The ring fellowship, a closed fellowship led by Bex."),
            line(637, 0, "Ash is now the leader of this fellowship."),
            line(639, 0, "Bex has left your Fellowship."),
            line(651, 0, "You are no longer a member of the Of The ring Fellowship."),
            line(663, 0, "You have been recruited into the Of The ring fellowship, a closed fellowship led by Bex."),
            line(669, 0, "Caius is now a member of your Fellowship."),
            line(677, 0, "Caius is now the leader of this fellowship."),
            line(681, 0, "Bex has been dismissed from the Fellowship."),
            line(695, 0, "Of The ring is now an open fellowship; anyone may recruit new members."),
            line(721, 0, "Bex is now a member of your Fellowship."),
            line(896, 0, "Bex has left your Fellowship."),
            line(906, 0, "Caius has disbanded your Fellowship."),
        ];
    // **Two** creates the fellowship twice, recruits, passes the lead on each time, quits, is
    // dismissed, is recruited back into what is by then an open fellowship, and quits again.
    let two_holds = two
        == vec![
            line(525, 0, "You have created the Fellowship of Of The ring."),
            line(554, 0, "Ash is now a member of your Fellowship."),
            line(560, 0, "Ash has left your Fellowship."),
            line(566, 0, "Ash is now a member of your Fellowship."),
            line(572, 0, "You have passed leadership of the fellowship to Ash"),
            line(573, 0, "Ash is now the leader of this fellowship."),
            line(575, 0, "You are no longer a member of the Of The ring Fellowship."),
            line(583, 0, "You have created the Fellowship of Of The ring."),
            line(598, 0, "Ash is now a member of your Fellowship."),
            line(614, 0, "Caius is now a member of your Fellowship."),
            line(623, 0, "You have passed leadership of the fellowship to Caius"),
            line(624, 0, "Caius is now the leader of this fellowship."),
            line(628, 0, "Caius has dismissed you from the Fellowship."),
            line(653, 0, "You have been recruited into the Of The ring fellowship, an open fellowship led by Caius."),
            line(751, 0, "You are no longer a member of the Of The ring Fellowship."),
        ];
    // **Three** is recruited, is made leader, dismisses somebody, opens the fellowship, recruits
    // him back, watches him leave, and disbands.
    let three_holds = three
        == vec![
            line(401, 0, "You have been recruited into the Of The ring fellowship, a closed fellowship led by Bex."),
            line(404, 0, "Caius is now the leader of this fellowship."),
            line(417, 0, "You dismiss Bex from your Fellowship."),
            line(426, 0, "Of The ring is now an open fellowship; anyone may recruit new members."),
            line(438, 0, "Bex is now a member of your Fellowship."),
            line(529, 0, "Bex has left your Fellowship."),
            line(538, 0, "You have disbanded your Fellowship."),
        ];

    let mut c = HeadlessClient::model();
    c.assert_behaviour(
        "fellowship.lines.the-recorded-story-becomes-the-lines-the-player-read",
        move |_| one_holds && two_holds && three_holds,
    );
}

#[test]
fn scenario_the_recorded_fellowship_story_becomes_the_lines_the_player_read() {
    scenario("the_recorded_fellowship_story_becomes_the_lines_the_player_read");
}

/// Every refusal code the recordings carry, each through its own arm -- or its own silence.
pub fn a_fellowship_refusal_code_prints_its_own_line_or_nothing() {
    use std::collections::BTreeMap;

    // (message, code) -> the first line it produced, and how many times it arrived.
    #[allow(clippy::type_complexity)] // a one-off tuple, named where it is read
    let mut seen: BTreeMap<(u32, u32), (Option<(u8, String)>, usize)> = BTreeMap::new();
    let mut arrivals = 0_usize;
    for session in [
        "fellowship-one-vassal",
        "fellowship-two-monarch",
        "fellowship-three-vassal",
    ] {
        let mut c = HeadlessClient::model();
        assert!(
            c.world_mut().set_player(recorded_player(session)),
            "the recorded character"
        );
        let mut seen_lines = 0;
        for (idx, op) in family_blobs(session) {
            c.when(Inbound::from_corpus(session, idx..idx + 1));
            let lines = c.view().chat_lines();
            let produced: Vec<(u8, String)> = lines[seen_lines..]
                .iter()
                .map(|m| (m.ty, m.body.clone()))
                .collect();
            seen_lines = lines.len();
            if op != 0x028A && op != 0x028B {
                continue;
            }
            let code = refusal_code(session, idx, op);
            assert!(
                produced.len() <= 1,
                "one arm, at most one line: {produced:?}"
            );
            seen.entry((op, code))
                .or_insert((produced.first().cloned(), 0))
                .1 += 1;
            arrivals += 1;
        }
        // Nothing in the family was unreadable: a silent code is a code with no line, not a
        // message the client could not read.
        assert_eq!(c.view().hud().stats.undecodable, 0, "{session}");
    }

    let codes: Vec<(u32, u32)> = seen.keys().copied().collect();
    let denominator = codes
        == vec![
            (0x028A, 0x0417),
            (0x028A, 0x048E),
            (0x028A, 0x051D),
            (0x028B, 0x050B),
            (0x028B, 0x050D),
            (0x028B, 0x050E),
            (0x028B, 0x051B),
            (0x028B, 0x051C),
        ];
    let counted = arrivals == seen.values().map(|v| v.1).sum::<usize>();
    let at = |op: u32, code: u32| seen[&(op, code)].0.clone();
    // Two of the eight have no arm, and their silence is the claim: a fellowship that is ignoring
    // requests, and the chat service already being on.
    let silent = at(0x028A, 0x0417).is_none() && at(0x028A, 0x051D).is_none();
    let spoken = at(0x028A, 0x048E)
        == Some((0x1A, "Your offer of Allegiance has been ignored.".to_owned()))
        && at(0x028B, 0x050B)
            == Some((0, "Of The ring is now an open fellowship; anyone may recruit new members.".to_owned()))
        // The public corpus carries a stand-in here, not the character's own name. The line is
        // the same line; the name in it is the recording's.
        && at(0x028B, 0x050D) == Some((0, "Ash is now the leader of this fellowship.".to_owned()))
        && at(0x028B, 0x050E)
            == Some((0, "You have passed leadership of the fellowship to Ash".to_owned()))
        && at(0x028B, 0x051B) == Some((0, "You have entered the General channel.".to_owned()))
        && at(0x028B, 0x051C) == Some((0, "You have left the Allegiance channel.".to_owned()));

    // The one line no recording carries: a fellowship being closed again. Built with the
    // production writer, because the claim is the line and not the recording.
    let mut c = HeadlessClient::model();
    c.when(Inbound::message(
        &dereth_protocol::comms::CommunicationWeenieErrorWithString {
            error_type: 0x050C,
            text: "Of The ring".to_owned(),
        },
    ));
    let closed = c.view().chat_lines().len() == 1
        && c.view().chat_lines()[0].body == "Of The ring is now a closed fellowship."
        && c.view().chat_lines()[0].ty == 0;

    // And a code the client has nothing at all to say about: no line, and not a failure to read.
    let mut quiet = HeadlessClient::model();
    quiet.when(Inbound::message(
        &dereth_protocol::comms::CommunicationWeenieError { error_type: 0x0FFF },
    ));
    quiet.when(Inbound::message(
        &dereth_protocol::comms::CommunicationWeenieErrorWithString {
            error_type: 0x0FFF,
            text: "x".to_owned(),
        },
    ));
    let no_arm = quiet.view().chat_lines().is_empty() && quiet.view().hud().stats.undecodable == 0;

    c.assert_behaviour(
        "fellowship.lines.a-refusal-code-prints-its-own-line-or-nothing-at-all",
        move |_| denominator && counted && silent && spoken && closed && no_arm,
    );
}

/// The code one recorded refusal blob carries.
fn refusal_code(session: &'static str, idx: usize, op: u32) -> u32 {
    use dereth_client_net::client_session::testing::Corpus;
    use dereth_protocol::Message as _;
    let corpus = Corpus::load(session)
        .expect("decodes")
        .expect("the corpus holds it");
    let row = corpus
        .blobs
        .iter()
        .find(|b| b.idx == idx)
        .expect("the blob");
    let body = if row.opcode == 0xF7B0 {
        &row.payload[16..]
    } else {
        &row.payload[4..]
    };
    let mut r = dereth_protocol::archive::Reader::new(body);
    if op == 0x028B {
        dereth_protocol::comms::CommunicationWeenieErrorWithString::read(&mut r)
            .expect("the recorded refusal decodes")
            .error_type
    } else {
        dereth_protocol::comms::CommunicationWeenieError::read(&mut r)
            .expect("the recorded refusal decodes")
            .error_type
    }
}

#[test]
fn scenario_a_fellowship_refusal_code_prints_its_own_line_or_nothing() {
    scenario("a_fellowship_refusal_code_prints_its_own_line_or_nothing");
}

// =============================================================================================
// fellowship.membership.colours-a-fellow-on-the-radar
// fellowship.invitation.reaches-the-tab-that-asks-the-player
// =============================================================================================

/// Being in a fellowship with somebody is something the radar can ask about.
///
/// It is a model claim and opens no dat: what the radar reads is the membership table, and both of
/// the questions it asks about a blip were answered with a flat "no" before the fellowship tab
/// existed -- so a fellow had never been drawn as one, whatever the colour table held.
pub fn a_fellow_is_known_as_one_on_the_radar() {
    let mut c = HeadlessClient::model();
    c.given(dereth_testkit::Given::APlayer(PLAYER));
    let before =
        !c.view().world().is_fellow(FELLOW) && !c.view().world().is_fellowship_leader(FELLOW);

    c.when(Inbound::message(&full_update(
        &[(PLAYER, "Me"), (FELLOW, "Bex")],
        FELLOW,
        false,
    )));
    let known = c.view().world().is_fellow(FELLOW);
    // The leader is separable from the rest, and somebody outside is neither.
    let leader = c.view().world().is_fellowship_leader(FELLOW)
        && !c.view().world().is_fellowship_leader(PLAYER);
    let outsider =
        !c.view().world().is_fellow(STRANGER) && !c.view().world().is_fellowship_leader(STRANGER);

    c.assert_behaviour(
        "fellowship.membership.colours-a-fellow-on-the-radar",
        move |_| before && known && leader && outsider,
    );
}

#[test]
fn scenario_a_fellow_is_known_as_one_on_the_radar() {
    scenario("a_fellow_is_known_as_one_on_the_radar");
}

/// An invitation to join a fellowship reaches the tab's own queue rather than the drop for
/// questions about parts of the interface that are not there.
pub fn a_fellowship_invitation_reaches_the_tab_that_asks() {
    let mut c = HeadlessClient::model();
    c.given(dereth_testkit::Given::APlayer(PLAYER));
    let nothing_asked = c.view().interaction().stats.fellowship_requests_raised == 0;

    c.when(Inbound::message(
        &dereth_protocol::comms::CharacterConfirmationRequest {
            confirmation_type: 4,
            context_id: 1,
            text: "Bex".to_owned(),
        },
    ));

    let raised = c.view().interaction().stats.fellowship_requests_raised == 1;
    let not_dropped = c.view().interaction().stats.confirmations_for_absent_panels == 0;

    c.assert_behaviour(
        "fellowship.invitation.reaches-the-tab-that-asks-the-player",
        move |_| nothing_asked && raised && not_dropped,
    );
}

#[test]
fn scenario_a_fellowship_invitation_reaches_the_tab_that_asks() {
    scenario("a_fellowship_invitation_reaches_the_tab_that_asks");
}

// =============================================================================================
// friends.*  -- the model halves: the bytes, and the typed commands
// =============================================================================================

const FRIEND_ONE: ObjectId = ObjectId(0x5000_001E);
const FRIEND_TWO: ObjectId = ObjectId(0x5000_001F);

/// One friend record.
fn a_friend(id: ObjectId, name: &str, online: bool) -> dereth_protocol::social::FriendData {
    dereth_protocol::social::FriendData {
        id,
        online: i32::from(online),
        appear_offline: 0,
        name: name.to_owned(),
        friends_list: Vec::new(),
        friend_of_list: Vec::new(),
    }
}

/// A friends list the shard sends, of the given kind.
fn friends_update(
    friends: Vec<dereth_protocol::social::FriendData>,
    update_type: u32,
) -> dereth_protocol::social::SocialFriendsUpdate {
    dereth_protocol::social::SocialFriendsUpdate {
        friends,
        update_type,
    }
}

/// One line typed into a chat window and submitted, through the production chat-line path. Nothing
/// here calls a friends handler by name.
fn typed(c: &mut HeadlessClient, text: &str) {
    c.when(Player::ui(UiRequest::ChatLine {
        text: text.to_owned(),
        window: 8,
    }));
}

/// Everything the client put in its outbox after `from`, as the bytes that would go on the wire.
/// **Nothing is sent**: this is the production writer, not a transport.
fn social_sent_since(c: &HeadlessClient, from: usize) -> Vec<Vec<u8>> {
    use dereth_protocol::Opcode;
    c.view().outbound()[from..]
        .iter()
        .filter_map(|r| {
            let (op, body) = match r {
                dereth_client_model::Request::SocialAddFriend(m) => {
                    (Opcode::SOCIAL_ADD_FRIEND, dereth_protocol::write_body(m))
                }
                dereth_client_model::Request::SocialRemoveFriend(m) => {
                    (Opcode::SOCIAL_REMOVE_FRIEND, dereth_protocol::write_body(m))
                }
                dereth_client_model::Request::SocialClearFriends(m) => {
                    (Opcode::SOCIAL_CLEAR_FRIENDS, dereth_protocol::write_body(m))
                }
                dereth_client_model::Request::SocialSendFriendsCommand(m) => (
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

/// Every line the client put on the local scroll since the last look.
fn scroll(c: &mut HeadlessClient) -> Vec<String> {
    c.world_mut()
        .scroll
        .drain()
        .into_iter()
        .map(|l| l.body)
        .collect()
}

/// A populated friends list is read to its last byte and written back unchanged.
///
/// **The bytes are composed here longhand**, field by field, rather than taken from a recording:
/// the one populated list on record is in the private captures and carries real character
/// names, and the claim does not need them. Composing them independently is what makes this more
/// than a round trip -- a reader that agreed with its own writer would still fail against a layout
/// written out by hand.
pub fn a_populated_friends_list_is_read_whole_and_written_back() {
    // count, then one record -- who he is, whether he is online, whether he is hiding, his name as
    // a length-prefixed string padded to four, then his two hanging lists -- and the kind of
    // change last of all rather than first.
    let mut want: Vec<u8> = Vec::new();
    want.extend(1_u32.to_le_bytes()); // one record
    want.extend(FRIEND_TWO.0.to_le_bytes());
    want.extend(1_u32.to_le_bytes()); // online
    want.extend(0_u32.to_le_bytes()); // not hiding
    want.extend(3_u16.to_le_bytes()); // "Bex"
    want.extend(b"Bex");
    want.extend([0, 0, 0]); // padded to four
    want.extend(0_u32.to_le_bytes()); // his own friends, none
    want.extend(0_u32.to_le_bytes()); // whose friend he is, none
    want.extend(1_u32.to_le_bytes()); // the kind of change: somebody added
    assert_eq!(want.len(), 36, "the shape this claim is about");

    let m = friends_update(vec![a_friend(FRIEND_TWO, "Bex", true)], 1);
    let written = dereth_protocol::write_body(&m).expect("the list encodes") == want;

    let back: dereth_protocol::social::SocialFriendsUpdate =
        dereth_protocol::read_body(&want).expect("the cursor lands exactly on the end");
    let read_whole = back.friends.len() == 1
        && back.friends[0].id == FRIEND_TWO
        && back.friends[0].online == 1
        && back.friends[0].appear_offline == 0
        && back.friends[0].name == "Bex"
        && back.friends[0].friends_list.is_empty()
        && back.friends[0].friend_of_list.is_empty()
        && back.update_type == 1;

    // The same record with the two dwords that differ when a friend logs out.
    let out = friends_update(vec![a_friend(FRIEND_TWO, "Bex", false)], 4);
    let logout = dereth_protocol::write_body(&out).expect("encodes");
    let differs_in_two_places =
        logout.len() == want.len() && logout.iter().zip(&want).filter(|(a, b)| a != b).count() == 2;

    let mut c = HeadlessClient::model();
    c.assert_behaviour(
        "friends.update.a-populated-list-is-read-whole-and-written-back-unchanged",
        move |_| written && read_whole && differs_in_two_places,
    );
}

#[test]
fn scenario_a_populated_friends_list_is_read_whole_and_written_back() {
    scenario("a_populated_friends_list_is_read_whole_and_written_back");
}

/// The bytes the add-a-friend message carries for `name`: the name and nothing else.
fn add_friend_bytes(name: &str) -> Vec<u8> {
    let mut blob = vec![0x18, 0x00, 0x00, 0x00];
    blob.extend(u16::try_from(name.len()).expect("short").to_le_bytes());
    blob.extend(name.as_bytes());
    while blob.len() % 4 != 0 {
        blob.push(0);
    }
    blob
}

/// Typing the add command sends the name, by either spelling, with the title marker trimmed.
pub fn adding_a_friend_by_name_sends_the_name() {
    let mut c = HeadlessClient::model();
    c.given(dereth_testkit::Given::APlayer(PLAYER));
    let nothing_asked = c.view().interaction().stats.friends_requests == 0;

    let m = c.view().outbound().len();
    typed(&mut c, "@friends add Bex");
    let sent = social_sent_since(&c, m) == vec![add_friend_bytes("Bex")];
    let ran = c.view().interaction().stats.friends_requests == 1
        && c.view().interaction().stats.chat_commands_refused == 0;

    // The other spelling of the same command.
    let m = c.view().outbound().len();
    typed(&mut c, "@friends_add Ash");
    let alias = social_sent_since(&c, m) == vec![add_friend_bytes("Ash")];

    // An allegiance title puts a marker in front of a name; it is trimmed before the name goes out.
    let m = c.view().outbound().len();
    typed(&mut c, "@friends add +Ash");
    let trimmed = social_sent_since(&c, m) == vec![add_friend_bytes("Ash")];

    c.assert_behaviour(
        "friends.commands.adding-by-name-sends-the-name-the-player-typed",
        move |_| nothing_asked && sent && ran && alias && trimmed,
    );
}

#[test]
fn scenario_adding_a_friend_by_name_sends_the_name() {
    scenario("adding_a_friend_by_name_sends_the_name");
}

/// The remove command takes a name and sends who that person is -- or sends nothing.
pub fn removing_a_friend_by_name_names_who_it_is() {
    let mut c = HeadlessClient::model();
    c.given(dereth_testkit::Given::APlayer(PLAYER));

    // With no list to look the name up in, there is nobody to name and nothing is sent.
    let m = c.view().outbound().len();
    typed(&mut c, "@friends remove Ash");
    let nobody = social_sent_since(&c, m).is_empty()
        && c.view().interaction().stats.friends_not_a_friend_refusals == 1;

    c.when(Inbound::message(&friends_update(
        vec![
            a_friend(FRIEND_ONE, "Ash", true),
            a_friend(FRIEND_TWO, "Bex", true),
        ],
        0,
    )));

    // Found without minding how it was capitalised, and named by who he is.
    let m = c.view().outbound().len();
    typed(&mut c, "@friends remove ash");
    let named =
        social_sent_since(&c, m) == vec![vec![0x17, 0x00, 0x00, 0x00, 0x1E, 0x00, 0x00, 0x50]];
    let counted = c.view().interaction().stats.friends_requests == 1;

    // All of them at once is a different message with nothing in it, and the client says so and
    // empties the list rather than waiting to be told.
    let _ = scroll(&mut c);
    let m = c.view().outbound().len();
    typed(&mut c, "@friends remove -ALL");
    let cleared = social_sent_since(&c, m) == vec![vec![0x25, 0x00, 0x00, 0x00]];
    let said_so = scroll(&mut c)
        .iter()
        .any(|l| l.contains("Your friends list has been cleared"));
    let emptied = c.view().world().player_system.social.friends.is_empty();

    c.assert_behaviour(
        "friends.commands.removing-by-name-names-who-it-is-or-sends-nothing",
        move |_| nobody && named && counted && cleared && said_so && emptied,
    );
}

#[test]
fn scenario_removing_a_friend_by_name_names_who_it_is() {
    scenario("removing_a_friend_by_name_names_who_it_is");
}

/// Listing the friends prints and sends nothing, in the order the tab shows them.
pub fn listing_friends_is_local_and_in_the_tabs_order() {
    let mut c = HeadlessClient::model();
    c.given(dereth_testkit::Given::APlayer(PLAYER));

    let m = c.view().outbound().len();
    typed(&mut c, "@friends");
    let empty = scroll(&mut c) == vec!["Your friends list is empty!".to_owned()]
        && c.view().interaction().stats.friends_listings == 1
        && social_sent_since(&c, m).is_empty();

    c.when(Inbound::message(&friends_update(
        vec![
            a_friend(FRIEND_ONE, "Ash", false),
            a_friend(FRIEND_TWO, "Bex", true),
        ],
        0,
    )));
    let _ = scroll(&mut c);

    typed(&mut c, "@friends");
    let listed = scroll(&mut c)
        == vec![
            "Your friends:".to_owned(),
            "Bex (Online)".to_owned(),
            "Ash".to_owned(),
        ];

    typed(&mut c, "@friends online");
    let online_only = scroll(&mut c) == vec!["Your friends:".to_owned(), "Bex (Online)".to_owned()];

    // Only offline friends: the heading, and then the line that says there are none on.
    c.when(Inbound::message(&friends_update(
        vec![a_friend(FRIEND_ONE, "Ash", false)],
        0,
    )));
    let _ = scroll(&mut c);
    typed(&mut c, "@friends online");
    let none_on = scroll(&mut c)
        == vec![
            "Your friends:".to_owned(),
            "You have no friends that are online.".to_owned(),
        ];

    // The refusals, and the two that need a name.
    let m = c.view().outbound().len();
    typed(&mut c, "@friends wibble");
    let unknown = scroll(&mut c) == vec!["Invalid friends command specified.".to_owned()];
    typed(&mut c, "@friends add");
    let no_add_name = scroll(&mut c)
        == vec!["You must specify the name of the friend you wish to add.".to_owned()];
    typed(&mut c, "@friends remove");
    let no_remove_name = scroll(&mut c)
        == vec!["You must specify the name of the friend you wish to remove.".to_owned()];
    let silent = social_sent_since(&c, m).is_empty();

    c.assert_behaviour(
        "friends.commands.listing-is-local-and-in-the-order-the-tab-shows",
        move |_| {
            empty
                && listed
                && online_only
                && none_on
                && unknown
                && no_add_name
                && no_remove_name
                && silent
        },
    );
}

#[test]
fn scenario_listing_friends_is_local_and_in_the_tabs_order() {
    scenario("listing_friends_is_local_and_in_the_tabs_order");
}

/// One spelling of the command is not a game action and goes out on the other queue.
pub fn the_old_friends_form_goes_out_on_the_other_queue() {
    let mut c = HeadlessClient::model();
    c.given(dereth_testkit::Given::APlayer(PLAYER));
    let m = c.view().outbound().len();
    typed(&mut c, "@friends old");
    let sent = social_sent_since(&c, m)
        == vec![vec![
            0xCD, 0xF7, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00,
        ]];
    let counted = c.view().interaction().stats.friends_requests == 1;

    c.assert_behaviour(
        "friends.commands.the-old-form-goes-out-on-the-other-queue",
        move |_| sent && counted,
    );
}

#[test]
fn scenario_the_old_friends_form_goes_out_on_the_other_queue() {
    scenario("the_old_friends_form_goes_out_on_the_other_queue");
}

// -------------------------------------------------------------------------------------------
// 4. allegiance.member-login.becomes-a-chat-line
// -------------------------------------------------------------------------------------------

/// An allegiance member logging on becomes one chat line naming them.
pub fn allegiance_login_becomes_a_chat_line() {
    use dereth_protocol::social::{
        AllegianceData, AllegianceHierarchy, AllegianceLoginNotification, AllegianceProfile,
        AllegianceUpdate,
    };
    const VASSAL: ObjectId = ObjectId(0x5000_0002);
    const STRANGER: ObjectId = ObjectId(0x5000_0099);

    // No `dropped::clear()` here: the ledger is a process-global, this scenario never reads it,
    // and a clear from this thread would disturb the objects scenario that does.
    let mut c = HeadlessClient::model();
    c.world_mut().player = Some(PLAYER);

    let monarch = AllegianceData {
        id: PLAYER,
        name: "Larktest".to_owned(),
        gender: 1,
        heritage_group: 1,
        rank: 10,
        ..AllegianceData::default()
    };
    let vassal = AllegianceData {
        id: VASSAL,
        name: "Frang".to_owned(),
        gender: 1,
        heritage_group: 1,
        rank: 3,
        ..AllegianceData::default()
    };
    let update = AllegianceUpdate {
        rank: 10,
        profile: AllegianceProfile {
            total_members: 2,
            total_vassals: 1,
            hierarchy: AllegianceHierarchy {
                version: 1,
                old_officer: Some(0),
                members: vec![(None, monarch), (Some(PLAYER), vassal)],
                ..AllegianceHierarchy::default()
            },
        },
    };
    c.when(Inbound::message(&update));
    assert_eq!(
        c.view().world().allegiance.total,
        2,
        "the roster must be cached before a notification can name anybody out of it"
    );

    // The scroll is drained at the head of the next batch, so the line the notification queues is
    // carried by the frame after it -- the client's own ordering, not a quirk of this harness.
    c.when(Inbound::message(&AllegianceLoginNotification {
        member: VASSAL,
        now_logged_in: 1,
    }))
    .tick(1);
    let on = c.chat_lines().len();
    c.when(Inbound::message(&AllegianceLoginNotification {
        member: VASSAL,
        now_logged_in: 0,
    }))
    .tick(1);
    c.when(Inbound::message(&AllegianceLoginNotification {
        member: STRANGER,
        now_logged_in: 1,
    }))
    .tick(1);

    c.assert_behaviour("allegiance.member-login.becomes-a-chat-line", move |v| {
        on == 1
            && v.chat_text() == ["Frang is logged in.", "Frang has logged out."]
            && v.hud().stats.allegiance_logins == 3
            && v.hud().stats.allegiance_logins_announced == 2
    });
}

#[test]
fn scenario_allegiance_login_becomes_a_chat_line() {
    scenario("allegiance_login_becomes_a_chat_line");
}
