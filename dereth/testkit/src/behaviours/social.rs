//! Allegiance and fellowship -- who the shard says you are with.
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
        id: "allegiance.buttons.a-confirmed-question-is-what-sends-the-oath-or-the-break",
        says: "Swearing sends the oath to the object the player had selected; kicking a vassal and \
               breaking from a patron send the same thing about a different person, the vassal the \
               player picked and the patron the tree says he has now. Breaking looks the patron up \
               again as the question is answered rather than remembering who it asked about, so a \
               player whose patron has gone in the meantime sends nothing at all.",
        since: RETAIL,
        evidence: Evidence::Private("AC-EVID-G17-ACTIONS"),
        station: "dereth-testkit::cpu::social::scenario_a_confirmed_question_sends_the_oath_or_the_break",
        tier: Tier::Cpu,
    },
    behaviour! {
        id: "allegiance.buttons.breaking-asks-a-question-and-sends-nothing-by-itself",
        says: "Pressing Break on the allegiance tab asks the player whether he means it and sends \
               nothing: one misclick must not cost him his allegiance. The button is lit only while \
               he has a patron to break from, and Swear is dark for exactly as long.",
        since: RETAIL,
        evidence: Evidence::Private("AC-EVID-G17-BREAK"),
        station: "dereth-testkit::dat::social::scenario_breaking_asks_a_question_and_sends_nothing",
        tier: Tier::Dat,
    },
    behaviour! {
        id: "allegiance.buttons.picking-a-vassal-arms-the-kick-button-from-the-rows-own-id",
        says: "Kick is dark until the player picks a vassal out of the list, and the vassal it would \
               act on is the one carried by the row he clicked rather than a position in the roster. \
               Pressing it then asks a question and still sends nothing.",
        since: RETAIL,
        evidence: Evidence::Private("AC-EVID-G17-KICK"),
        station: "dereth-testkit::dat::social::scenario_picking_a_vassal_arms_the_kick_button",
        tier: Tier::Dat,
    },
    behaviour! {
        id: "allegiance.buttons.swear-is-dark-while-the-player-already-has-a-patron",
        says: "A player who already swore to somebody cannot swear again, so the Swear button is \
               dark, and pressing a dark button raises nothing at all rather than asking a question \
               it would refuse to act on.",
        since: RETAIL,
        evidence: Evidence::Private("AC-EVID-G17-SWEAR"),
        station: "dereth-testkit::dat::social::scenario_swear_is_dark_while_the_player_has_a_patron",
        tier: Tier::Dat,
    },
    behaviour! {
        id: "allegiance.channels.an-online-patron-monarch-or-vassal-opens-that-channel",
        says: "The three allegiance channels are offered only while somebody is there to hear them: \
               the patron channel while the player's patron is online, the monarch channel while the \
               monarch is online and is somebody other than the player, and the vassal channel while \
               any direct vassal is online. Every rebuild of the tab says so again, and when they all \
               log out the three close again.",
        since: RETAIL,
        evidence: Evidence::Private("AC-EVID-P1-3-CHANNELS"),
        station: "dereth-testkit::dat::social::scenario_an_online_relative_opens_that_allegiance_channel",
        tier: Tier::Dat,
    },
    behaviour! {
        id: "allegiance.channels.the-panels-request-writes-the-one-talk-focus-mask",
        says: "When the allegiance tab asks for a channel to be opened or closed, the answer is \
               written into the single piece of chat state the talk-to menu itself reads, so the menu \
               and the tab can never disagree. A channel number that is not a channel is ignored \
               rather than written, and it raises no notice either.",
        since: RETAIL,
        evidence: Evidence::Private("AC-EVID-P1-3-MASK"),
        station: "dereth-testkit::cpu::social::scenario_the_tabs_request_writes_the_one_talk_focus_mask",
        tier: Tier::Cpu,
    },
    behaviour! {
        id: "allegiance.commands.a-refused-line-is-printed-where-the-player-reads-it-and-sends-nothing",
        says: "A mistyped allegiance command is answered with the client's own hint and sends \
               nothing: a bare command, an unknown one, one that left out the name it needed, one \
               given an officer level that is not a level, and one given an argument where it takes \
               none. The answer appears in the strip across the top of the screen, which is the one \
               surface that shows this kind of line.",
        since: RETAIL,
        evidence: Evidence::Private("AC-EVID-P2-6A-REFUSALS"),
        station: "dereth-testkit::dat::social::scenario_a_refused_allegiance_line_prints_and_sends_nothing",
        tier: Tier::Dat,
    },
    behaviour! {
        id: "allegiance.commands.every-typed-allegiance-command-sends-its-own-message",
        says: "The allegiance commands have no button anywhere in the interface: the typed line is \
               the whole gesture. Naming the allegiance, the message of the day, the officers and \
               their titles, the ban list, the lock and its bypass, the house, an enquiry about a \
               member, the recall home, gagging and booting from the allegiance channel and booting \
               from the allegiance itself -- each line typed into the chat entry puts exactly one \
               message in the outbox, and it is that line's own.",
        since: RETAIL,
        evidence: Evidence::Private("AC-EVID-P2-6A"),
        station: "dereth-testkit::dat::social::scenario_every_typed_allegiance_command_sends_its_own_message",
        tier: Tier::Dat,
    },
    behaviour! {
        id: "allegiance.commands.listening-is-a-local-option-and-broadcasting-is-a-channel",
        says: "Two lines that look like allegiance commands are not: turning allegiance chat on and \
               off changes one of the player's own stored options and tells the shard that, and \
               saying it again when it already agrees sends nothing at all; and broadcasting to the \
               allegiance is an ordinary line on the allegiance-wide channel, by either the short \
               name or the long one.",
        since: RETAIL,
        evidence: Evidence::Private("AC-EVID-P2-6A-NEIGHBOURS"),
        station: "dereth-testkit::dat::social::scenario_listening_is_an_option_and_broadcasting_is_a_channel",
        tier: Tier::Dat,
    },
    behaviour! {
        id: "allegiance.member-login.becomes-a-chat-line",
        says: "When a member of your allegiance logs on or off, a line appears in the chat \
               window naming them -- \"<Name> is logged in.\" or \"<Name> has logged out.\" -- \
               using the bare member name from the cached allegiance roster. A notification for \
               somebody the roster does not hold prints nothing at all.",
        since: RETAIL,
        evidence: Evidence::Private("AC-EVID-G42"),
        station: "dereth-testkit::cpu::social::scenario_allegiance_login_becomes_a_chat_line",
        tier: Tier::Cpu,
    },
    behaviour! {
        id: "allegiance.panel.a-member-whose-gender-carries-no-title-is-drawn-by-bare-name",
        says: "A member of the allegiance is drawn with the title his rank earns him, and a member \
               whose sex the shard gave as neither of the two the titles are written for is drawn by \
               his bare name instead of being given a title that does not exist.",
        since: RETAIL,
        evidence: Evidence::Private("AC-EVID-P1-3-GENDER"),
        station: "dereth-testkit::dat::social::scenario_a_member_with_no_title_for_his_sex_is_drawn_by_name",
        tier: Tier::Dat,
    },
    behaviour! {
        id: "allegiance.panel.a-monarch-has-no-monarch-row-and-no-patron-row",
        says: "A player who is the head of his own allegiance has neither a patron nor anybody above \
               him, so the tab hides both of those rows rather than drawing them empty, and the \
               monarch channel stays shut for him however many members are online.",
        since: RETAIL,
        evidence: Evidence::Private("AC-EVID-P1-3-MONARCH"),
        station: "dereth-testkit::dat::social::scenario_a_monarch_is_shown_no_monarch_and_no_patron_row",
        tier: Tier::Dat,
    },
    behaviour! {
        id: "allegiance.panel.a-rank-buff-changing-redraws-the-players-line-alone",
        says: "When a spell on the player's allegiance rank lands or wears off, the rank line on the \
               tab is redrawn at once, without waiting for the shard's next roster; nothing else on \
               the tab is rebuilt, the chat channels are not re-decided, and a request still waiting \
               for the shard's answer goes on waiting.",
        since: RETAIL,
        evidence: Evidence::Private("AC-EVID-ENCHANTED-READS-RANK-REDRAW"),
        station: "dereth-ui-screens::dat::social::allegiance_panel_sections::a_rank_buff_changing_redraws_the_players_line_and_nothing_else",
        tier: Tier::Dat,
    },
    behaviour! {
        id: "allegiance.panel.a-rank-buff-shows-the-buffed-rank-and-how-far-it-moved",
        says: "The player's rank line reads the rank with his enchantments applied. When that is the \
               rank the allegiance tree gives him it reads Rank: Baron [3]; when a spell has moved it, \
               the line shows the moved rank and the difference, Rank: Baron [4 (+1)], under the title \
               of the rank he really holds.",
        since: RETAIL,
        evidence: Evidence::Private("AC-EVID-ENCHANTED-READS-RANK-LINE"),
        station: "dereth-ui-screens::dat::social::allegiance_panel_sections::a_rank_buff_shows_the_buffed_rank_and_how_far_it_moved",
        tier: Tier::Dat,
    },
    behaviour! {
        id: "allegiance.panel.a-roster-fills-the-shipped-vassal-list-and-is-replaced-whole",
        says: "The vassals the shard named are drawn into the shipped list, newest first, each under \
               the title his rank earns him, and an offline one is marked as away. A later answer \
               that carries the same roster redraws nothing, and one that carries no allegiance at \
               all empties the list rather than leaving the old rows behind it.",
        since: RETAIL,
        evidence: Evidence::Private("AC-EVID-O201-ROWS"),
        station: "dereth-testkit::dat::social::scenario_a_roster_fills_the_shipped_vassal_list",
        tier: Tier::Dat,
    },
    behaviour! {
        id: "allegiance.panel.the-followers-numbers-come-from-the-profile-header",
        says: "The tab shows two follower counts, and each is the number the shard put in the \
               answer's own header: the player's is how many followers the shard counts under him, \
               and the monarch's is the whole allegiance less the monarch himself. Neither is the \
               number of members the answer happened to carry, which is only the player's own corner \
               of the tree.",
        since: RETAIL,
        evidence: Evidence::Private("AC-EVID-P1-27"),
        station: "dereth-testkit::dat::social::scenario_both_followers_numbers_come_from_the_answers_header",
        tier: Tier::Dat,
    },
    behaviour! {
        id: "allegiance.panel.the-roster-the-shard-really-sent-reaches-the-list",
        says: "The roster a shard actually sent -- not one this client encoded for itself -- reaches \
               the vassal list: the player's own vassal is drawn there under his full name, he is \
               shown as present, and the vassal channel opens for him. A client that could only read \
               its own writing would draw an empty tab for ever and look exactly like a character \
               with no allegiance.",
        since: RETAIL,
        evidence: Evidence::Private("AC-EVID-G17-PANEL"),
        station: "dereth-testkit::dat::social::scenario_the_roster_the_shard_really_sent_reaches_the_list",
        tier: Tier::Dat,
    },
    behaviour! {
        id: "allegiance.panel.the-shipped-vassal-list-is-bound-and-empty-without-an-allegiance",
        says: "The allegiance tab of the shipped interface really is built and driven on an ordinary \
               frame: its vassal list is found and bound, the tab is rebuilt at least once, and a \
               character in no allegiance is shown an empty list with the vassal channel shut -- \
               which is a different thing from a tab that was never driven at all.",
        since: RETAIL,
        evidence: Evidence::Private("AC-EVID-O201"),
        station: "dereth-testkit::dat::social::scenario_the_allegiance_tab_is_bound_and_empty_without_an_allegiance",
        tier: Tier::Dat,
    },
    behaviour! {
        id: "allegiance.panel.the-visible-patron-row-shows-the-players-own-tithed-experience",
        says: "The experience line in the patron row is the experience the player himself has passed \
               up, not his patron's; when his patron is also the monarch the same line moves into the \
               combined row above; a nought is written out rather than left blank; and a player with \
               no patron keeps the whole row hidden instead of being shown a line about nobody.",
        since: RETAIL,
        evidence: Evidence::Private("AC-EVID-P1-3-TITHE"),
        station: "dereth-testkit::dat::social::scenario_the_patron_row_shows_the_players_own_tithed_experience",
        tier: Tier::Dat,
    },
    behaviour! {
        id: "allegiance.roster.a-login-notification-redraws-nothing",
        says: "Being told that a member of the allegiance has logged in or out does not rebuild the \
               tab: the two follower counts and the rows stay exactly as the last full answer left \
               them, because a login notice is a line in the chat window and not a new roster.",
        since: RETAIL,
        evidence: Evidence::Private("AC-EVID-P1-27-LOGIN"),
        station: "dereth-testkit::dat::social::scenario_a_login_notification_leaves_the_tab_alone",
        tier: Tier::Dat,
    },
    behaviour! {
        id: "allegiance.roster.a-roster-the-shard-really-sent-is-read-whole",
        says: "A recorded allegiance answer from a live shard is read to its last byte, and every \
               member arrives with the shard's own values on him -- his name, his sex, his people, \
               his rank, how loyal and how able he is, and what he has passed up and set aside. Each \
               is hung off the member the shard said he follows, so the tree the player is shown is \
               the tree the shard described.",
        since: RETAIL,
        evidence: Evidence::Private("AC-EVID-G17-BYTES"),
        station: "dereth-testkit::cpu::social::scenario_a_roster_the_shard_really_sent_is_read_whole",
        tier: Tier::Cpu,
    },
    behaviour! {
        id: "allegiance.roster.carries-the-players-rank-read-enchanted",
        says: "The roster handed to the allegiance tab carries the player's own allegiance rank as \
               the enchanted read gives it: the stored rank plus any spell on it when the list of \
               enchantable properties names the rank, the stored rank when it does not or is not \
               loaded, and nothing (zero) when the player carries no rank.",
        since: RETAIL,
        evidence: Evidence::Private("AC-EVID-ENCHANTED-READS-ROSTER"),
        station: "dereth-client-runtime::lib::allegiance_view::tests::the_roster_carries_the_players_rank_read_enchanted_through_the_filter",
        tier: Tier::Cpu,
    },
    behaviour! {
        id: "allegiance.roster.is-walked-from-the-player-outwards-and-titled-by-rank",
        says: "The allegiance the shard sends is read as a tree and walked outwards from the player: \
               his monarch, his patron and his own direct vassals, newest first, and not a vassal's \
               vassal. Each one is named with the title his rank earns him and carries whether he is \
               online and what he has passed up. A character in no allegiance walks to nothing at all \
               rather than to a half-filled roster.",
        since: RETAIL,
        evidence: Evidence::Private("AC-EVID-O201-WALK"),
        station: "dereth-testkit::cpu::social::scenario_the_allegiance_tree_is_walked_from_the_player_outwards",
        tier: Tier::Cpu,
    },
    behaviour! {
        id: "allegiance.subscription.is-an-edge-and-not-a-poll",
        says: "The tab asks the shard for the roster when it opens and not again while it stays \
               open: it is the change that asks and not the state, so a tab left up does not send a \
               fresh request on every frame the client draws.",
        since: RETAIL,
        evidence: Evidence::Private("AC-EVID-O935-EDGE"),
        station: "dereth-testkit::dat::social::scenario_a_tab_left_open_asks_once_and_not_once_a_frame",
        tier: Tier::Dat,
    },
    behaviour! {
        id: "allegiance.subscription.the-answer-fills-the-tab-and-provokes-no-second-ask",
        says: "The shard answers the request with the roster, and that answer alone fills the tab -- \
               nothing further has to happen for the player to see his allegiance. The answer does \
               not itself provoke another request, so the two cannot chase each other.",
        since: RETAIL,
        evidence: Evidence::Private("AC-EVID-O935-ANSWER"),
        station: "dereth-testkit::dat::social::scenario_the_answer_to_the_ask_fills_the_tab",
        tier: Tier::Dat,
    },
    behaviour! {
        id: "allegiance.subscription.the-opening-ask-is-withheld-without-a-player",
        says: "Opening the tab before the shard has described the player sends nothing, while \
               closing it sends the unsubscribe all the same. The tab still comes up either way: it \
               is the request that is withheld and not the interface.",
        since: RETAIL,
        evidence: Evidence::Private("AC-EVID-O935-GUARD"),
        station: "dereth-testkit::dat::social::scenario_the_opening_ask_is_withheld_without_a_player",
        tier: Tier::Dat,
    },
    behaviour! {
        id: "allegiance.subscription.the-tab-asks-the-shard-on-the-way-up-and-unasks-on-the-way-down",
        says: "The shard sends an allegiance roster only to a client that has asked for one, so the \
               tab asks: once as it is built, once when the player's own description arrives, and \
               once more when the player opens it -- and it unasks when he closes it again. The \
               request the client sends is the one the recordings carry, to the byte.",
        since: RETAIL,
        evidence: Evidence::Private("AC-EVID-O935"),
        station: "dereth-testkit::dat::social::scenario_opening_the_tab_asks_the_shard_and_closing_it_unasks",
        tier: Tier::Dat,
    },
    behaviour! {
        id: "confirmation.any-front-end-answers-the-servers-question-the-same-way",
        says: "The server's question is asked and answered the same way whatever shows it: a plain \
               Yes/No question reads as the server wrote it and the others end in \" Continue?\", \
               a second question while one is up takes over the open box rather than opening \
               another, the answer goes back whether it is yes or no, and a question the server \
               withdraws is answered no on the way out.",
        since: RETAIL,
        evidence: Evidence::Private("AC-EVID-SEAM-DIALOGS"),
        station: "dereth-testkit::cpu::social::scenario_any_front_end_answers_the_servers_question_the_same_way",
        tier: Tier::Cpu,
    },
    behaviour! {
        id: "fellowship.buttons.a-leader-who-leaves-hands-the-lead-on-first",
        says: "A leader who leaves his fellowship hands the lead to somebody else before he goes, in \
               that order and in the same breath, so the fellowship is never left without one. A \
               plain member who leaves sends only his own departure.",
        since: RETAIL,
        evidence: Evidence::Private("AC-EVID-G16-QUIT"),
        station: "dereth-testkit::dat::social::scenario_a_leader_who_leaves_hands_the_lead_on_first",
        tier: Tier::Dat,
    },
    behaviour! {
        id: "fellowship.buttons.disband-is-offered-only-to-the-leader",
        says: "Only the leader may disband a fellowship: for anybody else the button is dark and \
               pressing it does nothing at all, not even silently. For the leader it sends the same \
               leaving message his own departure would, with the flag that says this one ends the \
               fellowship.",
        since: RETAIL,
        evidence: Evidence::Private("AC-EVID-G16-DISBAND"),
        station: "dereth-testkit::dat::social::scenario_disband_is_offered_only_to_the_leader",
        tier: Tier::Dat,
    },
    behaviour! {
        id: "fellowship.buttons.opening-the-fellowship-changes-the-tab-before-the-shard-answers",
        says: "Opening a fellowship to anybody who wants to join is shown as done the moment the \
               player presses the button: the flag flips locally and the new value is what goes to \
               the shard, so the tab does not sit unchanged waiting for an answer.",
        since: RETAIL,
        evidence: Evidence::Private("AC-EVID-G16-OPEN"),
        station: "dereth-testkit::dat::social::scenario_opening_the_fellowship_changes_the_tab_first",
        tier: Tier::Dat,
    },
    behaviour! {
        id: "fellowship.buttons.picking-a-row-then-dismissing-names-that-fellow",
        says: "Dismissing somebody is two gestures and not one: the button is dark until the player \
               picks a row, and the person it then names is the one whose row he picked. Picking the \
               row also makes that fellow the player's selection in the world.",
        since: RETAIL,
        evidence: Evidence::Private("AC-EVID-G16-DISMISS"),
        station: "dereth-testkit::dat::social::scenario_picking_a_row_then_dismissing_names_that_fellow",
        tier: Tier::Dat,
    },
    behaviour! {
        id: "fellowship.buttons.recruit-follows-the-world-selection-and-not-the-list",
        says: "The person the player would recruit is whoever he has selected in the world, not \
               whoever is highlighted in the list. The button is offered only while that selection \
               is somebody who is not already in the fellowship, and it names that person by who he \
               is rather than by his name.",
        since: RETAIL,
        evidence: Evidence::Private("AC-EVID-G16-RECRUIT"),
        station: "dereth-testkit::dat::social::scenario_recruit_follows_the_world_selection",
        tier: Tier::Dat,
    },
    behaviour! {
        id: "fellowship.create.the-name-box-gates-the-button-and-the-tick-box-is-the-source",
        says: "The Create button is dark until something is typed in the name box, and pressing it \
               tidies the typed name, writes the tidied name back into the box for the player to see, \
               and sends that. Whether the fellowship shares experience is whatever the tick box \
               beside it says, which starts ticked and changes the message the next Create sends \
               when the player unticks it.",
        since: RETAIL,
        evidence: Evidence::Private("AC-EVID-G16-CREATE"),
        station: "dereth-testkit::dat::social::scenario_the_name_box_gates_create_and_the_tick_box_is_the_source",
        tier: Tier::Dat,
    },
    behaviour! {
        id: "fellowship.invitation.reaches-the-tab-that-asks-the-player",
        says: "An invitation to join somebody's fellowship reaches the fellowship tab's own queue \
               and is asked about, rather than being dropped as a question for a part of the \
               interface that is not there.",
        since: RETAIL,
        evidence: Evidence::Private("AC-EVID-G16-INVITE"),
        station: "dereth-testkit::cpu::social::scenario_a_fellowship_invitation_reaches_the_tab_that_asks",
        tier: Tier::Cpu,
    },
    behaviour! {
        id: "fellowship.lines.a-refusal-code-prints-its-own-line-or-nothing-at-all",
        says: "Each thing the shard refuses, or announces, about a fellowship has its own line: the \
               fellowship being opened or closed, a new leader, the lead being handed on, entering \
               and leaving a channel. A code the client has nothing to say about prints nothing and \
               is not counted as a message it failed to read -- the message was read, there is \
               simply no line for it.",
        since: RETAIL,
        evidence: Evidence::Private("AC-EVID-P1-33-CODES"),
        station: "dereth-testkit::cpu::social::scenario_a_fellowship_refusal_code_prints_its_own_line_or_nothing",
        tier: Tier::Cpu,
    },
    behaviour! {
        id: "fellowship.lines.reach-the-live-chat-log-in-the-colour-of-a-broadcast",
        says: "The fellowship lines are not only composed, they are drawn: creating a fellowship, \
               opening it, closing it and leaving it each put their line in the chat log the player \
               is looking at, every glyph of it in the colour this kind of line is drawn in.",
        since: RETAIL,
        evidence: Evidence::Private("AC-EVID-P1-33-LOG"),
        station: "dereth-testkit::dat::social::scenario_the_fellowship_lines_reach_the_live_chat_log",
        tier: Tier::Dat,
    },
    behaviour! {
        id: "fellowship.lines.the-recorded-story-becomes-the-lines-the-player-read",
        says: "A recorded session in which a fellowship is made, joined, left, handed on, dismissed \
               from, opened and disbanded produces exactly the lines that player read, in the order \
               he read them: who recruited him and whether the fellowship was open, who joined and \
               who left, who leads now, and that it is over. Two of them are lines the client writes \
               for itself rather than repeating the shard: creating the fellowship, and leaving it.",
        since: RETAIL,
        evidence: Evidence::Private("AC-EVID-P1-33"),
        station: "dereth-testkit::cpu::social::scenario_the_recorded_fellowship_story_becomes_the_lines_the_player_read",
        tier: Tier::Cpu,
    },
    behaviour! {
        id: "fellowship.membership.colours-a-fellow-on-the-radar",
        says: "Being in a fellowship with somebody is something the radar can see: a fellow is known \
               as one, the leader is known apart from the rest, and somebody outside the fellowship \
               is neither.",
        since: RETAIL,
        evidence: Evidence::Private("AC-EVID-G16-RADAR"),
        station: "dereth-testkit::cpu::social::scenario_a_fellow_is_known_as_one_on_the_radar",
        tier: Tier::Cpu,
    },
    behaviour! {
        id: "fellowship.membership.moves-the-tab-target-cycle-and-the-chat-tab",
        says: "Joining a fellowship takes your fellows out of the cycle the target key steps through \
               and turns the fellowship row of the chat menu on; leaving, being dismissed, or the \
               fellowship disbanding puts them back and turns it off again. A later membership list \
               replaces the old one rather than merging, somebody else's departure leaves your own tab \
               alone, and a departure from a locked fellowship is remembered so they can be \
               re-admitted.",
        since: RETAIL,
        evidence: Evidence::Private("AC-EVID-O937-FELLOWSHIP"),
        station: "dereth-testkit::cpu::social::scenario_fellowship_membership_moves_the_cycle_and_the_tab",
        tier: Tier::Cpu,
    },
    behaviour! {
        id: "fellowship.panel.a-membership-list-draws-a-roster-with-names-stats-and-meters",
        says: "While the tab is up, the membership list the shard sends fills it: one row per \
               member, each with his name, how far on he is, and three meters standing at the \
               fractions of health, stamina and mana the shard reported. Before there is a \
               fellowship the tab shows the frame that says there is none, and a second copy of the \
               same list redraws nothing.",
        since: RETAIL,
        evidence: Evidence::Private("AC-EVID-G16-ROSTER"),
        station: "dereth-testkit::dat::social::scenario_a_membership_list_draws_a_roster_with_names_stats_and_meters",
        tier: Tier::Dat,
    },
    behaviour! {
        id: "fellowship.subscription.the-social-page-opening-is-not-the-tab-opening",
        says: "The fellowship tab shares its page with the allegiance tab, and the page opens on the \
               other one -- so opening the page asks the shard for nothing. It is bringing this tab \
               up that asks, and leaving it that unasks, once each way and not once a frame.",
        since: RETAIL,
        evidence: Evidence::Private("AC-EVID-G16-SUBSCRIBE"),
        station: "dereth-testkit::dat::social::scenario_the_social_page_opening_is_not_the_fellowship_tab_opening",
        tier: Tier::Dat,
    },
    behaviour! {
        id: "friends.appear-offline.the-box-opens-at-the-characters-bit-and-a-press-raises-the-option",
        says: "The friends panel's Appear Offline box opens unticked for a character who does not \
               appear offline, and a press on it ticks it and sends exactly one option change, \
               turning that one option on.",
        since: RETAIL,
        evidence: Evidence::Private("AC-EVID-G42-APPEAR-OFFLINE"),
        station: "dereth-ui-screens::dat::panels::panel_option_checkboxes::appear_offline::a_real_press_on_the_appear_offline_box_names_the_option",
        tier: Tier::Dat,
    },
    behaviour! {
        id: "friends.buttons.picking-a-row-arms-remove-and-the-press-names-that-friend",
        says: "The Remove button is dark until the player picks a friend out of the list, and the \
               person it then names is the one whose row he picked -- named by who he is, not by his \
               name.",
        since: RETAIL,
        evidence: Evidence::Private("AC-EVID-G26-REMOVE"),
        station: "dereth-testkit::dat::social::scenario_picking_a_friend_arms_remove_and_the_press_names_him",
        tier: Tier::Dat,
    },
    behaviour! {
        id: "friends.buttons.tell-follows-whether-the-friend-is-online",
        says: "The button that starts a private message is offered for a friend who is online and \
               refused for one who is not, while removing him stays offered either way.",
        since: RETAIL,
        evidence: Evidence::Private("AC-EVID-G26-TELL"),
        station: "dereth-testkit::dat::social::scenario_tell_follows_whether_the_friend_is_online",
        tier: Tier::Dat,
    },
    behaviour! {
        id: "friends.buttons.typing-a-name-arms-add-and-the-press-sends-the-name-alone",
        says: "The Add button is dark while the name box is empty and lit once something is typed \
               in it. Pressing it sends the typed name and nothing else -- there is no room in the \
               message for anything but a name -- and then empties the box and darkens the button \
               again.",
        since: RETAIL,
        evidence: Evidence::Private("AC-EVID-G26-ADD"),
        station: "dereth-testkit::dat::social::scenario_typing_a_name_arms_add_and_the_press_sends_it_alone",
        tier: Tier::Dat,
    },
    behaviour! {
        id: "friends.commands.adding-by-name-sends-the-name-the-player-typed",
        says: "Typing the add-a-friend command sends the same thing the button does, by either \
               spelling of the command, and the marker an allegiance title puts in front of a name \
               is trimmed off before the name goes out.",
        since: RETAIL,
        evidence: Evidence::Private("AC-EVID-G26-CMD-ADD"),
        station: "dereth-testkit::cpu::social::scenario_adding_a_friend_by_name_sends_the_name",
        tier: Tier::Cpu,
    },
    behaviour! {
        id: "friends.commands.listing-is-local-and-in-the-order-the-tab-shows",
        says: "Asking for the friends list prints it in the chat window and sends nothing to the \
               shard: the heading, then the friends who are online, then the rest -- and a player \
               with none is told so. Asking only for the online ones leaves the others out and says \
               so when there are none. A command the client does not know, and one that left out the \
               name it needed, are each answered and send nothing.",
        since: RETAIL,
        evidence: Evidence::Private("AC-EVID-G26-CMD-LIST"),
        station: "dereth-testkit::cpu::social::scenario_listing_friends_is_local_and_in_the_tabs_order",
        tier: Tier::Cpu,
    },
    behaviour! {
        id: "friends.commands.removing-by-name-names-who-it-is-or-sends-nothing",
        says: "The remove-a-friend command takes a name and sends who that person is, looked up in \
               the list the shard sent and without minding how it was capitalised; a name nobody on \
               the list answers to sends nothing at all. Asking for all of them to go is a different \
               message with nothing in it, and the client says so and empties the list without \
               waiting to be told.",
        since: RETAIL,
        evidence: Evidence::Private("AC-EVID-G26-CMD-REMOVE"),
        station: "dereth-testkit::cpu::social::scenario_removing_a_friend_by_name_names_who_it_is",
        tier: Tier::Cpu,
    },
    behaviour! {
        id: "friends.commands.the-old-form-goes-out-on-the-other-queue",
        says: "One spelling of the friends command is not a game action at all: it goes out on the \
               queue that carries no ordering stamp, with an empty name, which is the only form the \
               client composes for it.",
        since: RETAIL,
        evidence: Evidence::Private("AC-EVID-G26-CMD-OLD"),
        station: "dereth-testkit::cpu::social::scenario_the_old_friends_form_goes_out_on_the_other_queue",
        tier: Tier::Cpu,
    },
    behaviour! {
        id: "friends.panel.the-tab-lists-the-friends-the-shard-sent-with-the-online-ones-first",
        says: "The friends tab of the shipped interface is built and driven on an ordinary frame, \
               and the list the shard sends fills it: the friends who are online first and in name \
               order, then the rest the same way, each row saying whether that friend is there. An \
               empty list really arrives and correctly draws nothing, which is a different thing \
               from never having been told.",
        since: RETAIL,
        evidence: Evidence::Private("AC-EVID-G26"),
        station: "dereth-testkit::dat::social::scenario_the_friends_tab_lists_what_the_shard_sent_it",
        tier: Tier::Dat,
    },
    behaviour! {
        id: "friends.update.a-populated-list-is-read-whole-and-written-back-unchanged",
        says: "A friends list with somebody in it is read to its last byte -- who he is, whether he \
               is online, whether he is hiding, his name, and the two lists that hang off him -- and \
               what kind of change it was is the last thing in it rather than the first. Written \
               back out it is the same bytes again, so a field the reader skipped would not survive \
               the round trip.",
        since: RETAIL,
        evidence: Evidence::Private("AC-EVID-G26-BYTES"),
        station: "dereth-testkit::cpu::social::scenario_a_populated_friends_list_is_read_whole_and_written_back",
        tier: Tier::Cpu,
    },
    behaviour! {
        id: "friends.update.each-kind-of-change-moves-the-list-on-its-own",
        says: "The shard says what kind of change it is sending, and each kind does its own thing: a \
               whole list replaces, somebody added is inserted beside the others rather than \
               replacing them, somebody removed goes, and a friend logging in or out moves between \
               the two blocks without joining or leaving the list. A kind the client does not know \
               changes nothing and is counted.",
        since: RETAIL,
        evidence: Evidence::Private("AC-EVID-G26-TYPES"),
        station: "dereth-testkit::dat::social::scenario_each_kind_of_friends_change_moves_the_list",
        tier: Tier::Dat,
    },
    behaviour! {
        id: "panels.redraw.the-vassal-list-follows-a-members-own-fields-changing-under-an-unchanged-id",
        says: "A vassal logging out or passing up experience redraws that row and the marks that \
               go with it, although the roster names the same people it did before; with nobody \
               left online the vassal channel goes with it.",
        since: RETAIL,
        evidence: Evidence::Private("AC-EVID-O487-ALLEGIANCE"),
        station: "dereth-testkit::dat::inventory::scenario_a_vassals_own_fields_redraw_the_row",
        tier: Tier::Dat,
    },
    behaviour! {
        id: "social.allegiance-info.prints-the-report",
        says: "The shard's answer to an allegiance info query about a member is printed as a \
               report in the chat log, and the player's own allegiance panel and tree are left \
               untouched.",
        since: RETAIL,
        evidence: Evidence::Private("AC-EVID-P24-ALLEGIANCE-INFO"),
        station: "dereth-client::gpu::panels::allegiance_info_replies::an_allegiance_info_response_prints_the_report_and_leaves_the_panel_alone",
        tier: Tier::Gpu,
    },
    behaviour! {
        id: "social.recordings.fellowship-and-allegiance-rosters-reach-the-session",
        says: "Every recorded fellowship session replays through the client with every shard \
               message delivered and every client message reproduced, and every \
               Fellowship_FullUpdate and Allegiance_AllegianceUpdate in them decodes, each \
               fellowship with its members and each session carrying at least one allegiance \
               roster.",
        since: RETAIL,
        evidence: Evidence::Private("AC-EVID-P05-RECORDINGS"),
        station: "dereth-client-net::cpu::social::social_recordings::every_recorded_social_roster_decodes_its_members_and_locks",
        tier: Tier::Cpu,
    },
];
