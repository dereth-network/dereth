//! The session: the game login, the connections to the login and world servers, the queues, and
//! what the client says to the servers and when.
//!
//! A row claims what the September 2013 client does on the wire, held to the client itself and to
//! the recorded January 2017 sessions.

use super::{Behaviour, Evidence, Tier, RETAIL, THIS_CLIENT, TOOLING};

/// This subject's rows, in id order.
pub static ROWS: &[Behaviour] = &[
    behaviour! {
        id: "session.connection.a-net-error-ends-the-login-or-logs-off",
        says: "A net error while the login is still going ends the login with that error, and no \
               login goes out again; a connection coming up after a failed login does not make it \
               connected. Once the login is over, a net error logs the client off, its goodbye \
               sent, nothing more after it, and the server is dead.",
        since: RETAIL,
        evidence: Evidence::Private("AC2-EVID-SESS-NET-ERROR"),
        station: "auberean-client-net::cpu::server_lost::a_net_error_ends_a_login_still_going_and_once_logged_in_logs_off",
        tier: Tier::Cpu,
    },
    behaviour! {
        id: "session.connection.a-server-that-starts-over-ends-the-session",
        says: "A connection request of a newer iteration from a server the session is talking to ends the session: the server is dead.",
        since: RETAIL,
        evidence: Evidence::Private("AC2-EVID-CONNECT-NEWER-ITERATION"),
        station: "auberean-client-net::cpu::server_lost::a_server_that_starts_over_ends_the_session",
        tier: Tier::Cpu,
    },
    behaviour! {
        id: "session.connection.a-servers-goodbye-with-an-error-is-told",
        says: "The login server or the current world server letting its connection go with an error \
               is told, with the error; letting it go with no error is not.",
        since: RETAIL,
        evidence: Evidence::Private("AC2-EVID-SESS-ERROR-GOODBYE"),
        station: "auberean-client-net::cpu::server_lost::the_login_servers_goodbye_with_an_error_is_told_and_one_without_is_not",
        tier: Tier::Cpu,
    },
    behaviour! {
        id: "session.connection.a-silent-server-is-dead-after-140-seconds",
        says: "A login server that falls silent is given 140 seconds from when it was last heard; \
               then its connection is let go, the client logs off, nothing more is sent (not even a \
               goodbye to the dead server) and the server is dead, once.",
        since: RETAIL,
        evidence: Evidence::Private("AC2-EVID-SESS-SERVER-DIED"),
        station: "auberean-client-net::cpu::server_lost::a_silent_login_server_is_dead_after_140_seconds_and_the_client_logs_off",
        tier: Tier::Cpu,
    },
    behaviour! {
        id: "session.connection.the-login-connection-is-held-and-every-referral-followed",
        says: "The login connection stays up from the login to the end of the session. A referral \
               to a world server the client has no connection to is followed with a world login, \
               and the server's switch makes it the current world; a referral to one it is still \
               connected to only refreshes that connection, and the switch follows on it. World \
               messages go to the current world server, the character list's to the login server, \
               and leaving the world drops every world connection.",
        since: RETAIL,
        evidence: Evidence::Private("AC2-EVID-SESS-REFERRAL"),
        station: "auberean-client-net::cpu::session::the_login_connection_is_held_and_every_referral_is_followed",
        tier: Tier::Cpu,
    },
    behaviour! {
        id: "session.double.a-server-double-takes-the-client-into-the-world",
        says: "Played the servers' side of a game login, the client sends its data versions when \
               asked, enters the world with the character picked from the list, follows the referral \
               to the world server, takes the player's creation and private updates in order, says \
               the scene is drawn once the player's object is here, reports its position while the \
               player runs, and on logging off leaves the world and keeps the login connection.",
        since: RETAIL,
        evidence: Evidence::Private("AC2-EVID-SESS-DOUBLE"),
        station: "auberean-headless::cpu::world_entry::a_server_double_takes_the_client_through_login_character_list_and_world_entry",
        tier: Tier::Cpu,
    },
    behaviour! {
        id: "session.interface.a-panels-selection-is-not-looked-at",
        says: "A selection made from a panel (the radar, a name, the pack, a fellow) sends no look: the head stays on what the selection keys and the world's clicks last chose.",
        since: RETAIL,
        evidence: Evidence::Private("AC2-EVID-SELECT-NO-LOOK"),
        station: "auberean-headless::cpu::world_entry::a_panels_selection_is_not_looked_at",
        tier: Tier::Cpu,
    },
    behaviour! {
        id: "session.interface.looking-at-the-selection",
        says: "Selecting an object turns the player to look at it, and the client tells the server \
               what the player looks at with each selection; with the option to look at the \
               selection off, the client tells the server once that the player looks at nothing, \
               and nothing more while it looks at nothing.",
        since: RETAIL,
        evidence: Evidence::Private("AC2-EVID-SESS-LOOK-AT"),
        station: "auberean-headless::cpu::world_entry::a_selection_is_looked_at_and_with_the_option_off_nothing_is_once",
        tier: Tier::Cpu,
    },
    behaviour! {
        id: "session.interface.requests-are-the-recorded-clients",
        says: "Every call our client sends through the player's scripts in a whole session (the \
               selection, the examination stop and queries, the attack, the uses of a corpse, an \
               item and a portal, a saying) has the values of calls the recorded 2017 clients sent \
               for the same function, object ids, the numbering of a graph's entries and a saying's \
               words aside.",
        since: RETAIL,
        evidence: Evidence::Private("AC2-EVID-SESS-PANEL-CALLS"),
        station: "yalain-testkit::corpus::client_sends::every_call_our_client_sends_is_one_the_recorded_clients_sent",
        tier: Tier::Corpus,
    },
    behaviour! {
        id: "session.interface.say-hands-the-line-and-its-words",
        says: "A line typed into the chat box is a command's line (one that is no command gets \
               the say command first, a command's marks are cut); its words split at spaces with \
               a quoted run kept whole, the first naming the command, which is handed the words \
               and the rest of the line after the command word.",
        since: RETAIL,
        evidence: Evidence::Private("AC2-EVID-SESS-SAY"),
        station: "auberean-client-runtime::cpu::world::requests::a_typed_line_splits_into_words_with_quoted_runs_kept_whole",
        tier: Tier::Cpu,
    },
    behaviour! {
        id: "session.interface.the-player-never-looks-at-itself-or-what-it-holds",
        says: "Selecting the player itself or something it holds tells the server the player \
               looks at nothing, and the server's saying it looks at one leaves it looking at \
               nothing; with the option to look at the selection off, a player the server set \
               looking at something is told to look at nothing at the next selection.",
        since: RETAIL,
        evidence: Evidence::Private("AC2-EVID-SESS-LOOK-NOT-SELF"),
        station: "auberean-headless::cpu::world_entry::selecting_itself_or_what_it_holds_looks_at_nothing",
        tier: Tier::Cpu,
    },
    behaviour! {
        id: "session.interface.the-players-head-sent-at-most-every-half-second",
        says: "Where the player's head points, once a world turns it, is sent to the server \
               while the player looks at nothing: at once, then only when it moved more than \
               0.0002 either way and more than half a second after the last time; nothing while \
               the player looks at an object.",
        since: RETAIL,
        evidence: Evidence::Private("AC2-EVID-SESS-HEAD-DIR"),
        station: "auberean-headless::cpu::world_entry::where_the_players_head_points_is_sent_at_most_every_half_second",
        tier: Tier::Cpu,
    },
    behaviour! {
        id: "session.link.the-rate-received-is-the-last-two-measures",
        says: "The rate the connection receives at is the bytes its last two measures counted over \
               the time they spanned, and 0 before any time has passed; the first measure's time \
               counts as one second.",
        since: RETAIL,
        evidence: Evidence::Private("AC2-EVID-SESS-RECEIVE-RATE"),
        station: "auberean-client-net::cpu::server_lost::the_rate_received_is_the_last_two_measures_bytes_over_their_time",
        tier: Tier::Cpu,
    },
    behaviour! {
        id: "session.login.a-login-server-address-moves-where-the-login-goes",
        says: "An address for the login server from a server moves the login: the next login, two \
               seconds after the last, goes to that address.",
        since: RETAIL,
        evidence: Evidence::Private("AC2-EVID-SESS-LOGON-ADDR"),
        station: "auberean-client-net::cpu::server_lost::a_login_server_address_moves_where_the_next_login_goes",
        tier: Tier::Cpu,
    },
    behaviour! {
        id: "session.login.a-refused-entry-logs-off-no-character-and-may-ask-again",
        says: "An entry the server refuses is seen with its reason; the client logs off with no \
               character, stays at the character list on its login connection, and a second entry \
               goes into the world.",
        since: RETAIL,
        evidence: Evidence::Private("AC2-EVID-SESS-REFUSED-ENTRY"),
        station: "auberean-headless::cpu::world_entry::a_refused_entry_logs_off_no_character_and_a_second_entry_goes_in",
        tier: Tier::Cpu,
    },
    behaviour! {
        id: "session.login.an-unanswered-login-fails-once",
        says: "A login no server answers is sent twenty times, two seconds apart, and then fails \
               once, the server timed out; it is never told connected.",
        since: RETAIL,
        evidence: Evidence::Private("AC2-EVID-SESS-LOGIN-TIMEOUT"),
        station: "auberean-client-net::cpu::server_lost::an_unanswered_login_is_sent_twenty_times_and_fails_once",
        tier: Tier::Cpu,
    },
    behaviour! {
        id: "session.login.closing-says-goodbye-on-every-connection",
        says: "Closing in the world says goodbye on every connection at once with no log-off \
               first, and nothing more is sent after it.",
        since: RETAIL,
        evidence: Evidence::Private("AC2-EVID-SESS-CLOSE-GOODBYE"),
        station: "auberean-headless::cpu::world_entry::closing_in_the_world_says_goodbye_on_every_connection",
        tier: Tier::Cpu,
    },
    behaviour! {
        id: "session.logon.no-ticket-is-an-account-login",
        says: "With no ticket the login is an account login: the account and the session-key blob, \
               nothing in the ticket's place.",
        since: RETAIL,
        evidence: Evidence::Private("AC2-EVID-LOGON-ACCOUNT"),
        station: "auberean-client-net::cpu::logon::a_login_with_no_ticket_is_an_account_login",
        tier: Tier::Cpu,
    },
    behaviour! {
        id: "session.logon.the-login-carries-version-ticket-account-and-key-blob",
        says: "The game login carries the version 1802, a ticket login with flags 1, the time, the \
               account as typed, a session-key blob (its twelve-byte header, then 64 bytes) and the \
               ticket in its counted blob as a string (its 16-bit count, the bytes, padding to \
               four), every string scrambled.",
        since: RETAIL,
        evidence: Evidence::Private("AC2-EVID-SESS-LOGON"),
        station: "auberean-client-net::cpu::logon::the_login_carries_version_ticket_account_and_key_blob",
        tier: Tier::Cpu,
    },
    behaviour! {
        id: "session.position.a-report-carries-the-stamp-of-the-last-move-ordered",
        says: "A move the server orders the player's body, walking it up to what it uses, keeps \
               its stamp, and every position report after it carries that stamp, so a report that \
               the player moved by hand tells the server which walk it interrupts.",
        since: RETAIL,
        evidence: Evidence::Private("AC2-EVID-SESS-MOVE-TO-STAMP"),
        station: "auberean-client-runtime::cpu::world::reports::a_report_carries_the_stamp_of_the_last_move_ordered",
        tier: Tier::Cpu,
    },
    behaviour! {
        id: "session.position.a-report-packs-heading-motion-flags-and-stamps",
        says: "A position report carries the heading in its top byte (256 steps to a full turn), the \
               asked motion as one byte each for sideways, forward and up (127 for none), the jump \
               flag with the jump's velocity, the moving flag while the player moves or turns, and \
               the body's instance and teleport stamps.",
        since: RETAIL,
        evidence: Evidence::Private("AC2-EVID-SESS-POSITION-PACK"),
        station: "auberean-client-runtime::cpu::world::reports::a_report_packs_heading_motion_flags_and_stamps",
        tier: Tier::Cpu,
    },
    behaviour! {
        id: "session.position.a-report-says-where-the-body-is-turning-to",
        says: "While the player's body turns toward a heading its position report says that \
               heading, and it goes on reporting until the body faces it.",
        since: RETAIL,
        evidence: Evidence::Private("AC2-EVID-SESS-POSITION-TURNING"),
        station: "auberean-client-runtime::cpu::world::reports::a_report_says_where_the_body_is_turning_to",
        tier: Tier::Cpu,
    },
    behaviour! {
        id: "session.position.a-turn-under-the-heading-tolerance-is-not-reported",
        says: "A turn smaller than 0.0002 radians, the way round through north included, is no                reason to send a report.",
        since: RETAIL,
        evidence: Evidence::Private("AC2-EVID-SESS-POSITION-TOLERANCE"),
        station: "auberean-client-runtime::cpu::world::reports::a_turn_under_the_heading_tolerance_is_not_reported",
        tier: Tier::Cpu,
    },
    behaviour! {
        id: "session.position.recorded-reports-carry-the-stamp-of-the-last-move-ordered",
        says: "The recorded 2017 players' position reports carry the stamp of the last move the \
               server ordered their bodies to make, as this client's do when fed the same messages.",
        since: RETAIL,
        evidence: Evidence::Private("AC2-EVID-SESS-MOVE-TO-STAMP-CORPUS"),
        station: "auberean-client-runtime::corpus::interface::recorded_reports_carry_the_stamp_of_the_last_move_ordered",
        tier: Tier::Corpus,
    },
    behaviour! {
        id: "session.position.reports-on-change-jump-and-every-fifth-of-a-second",
        says: "The client reports the player's position at once when the asked motion changes or \
               the player jumps; while the player moves or turns it reports again on the first frame \
               more than a fifth of a second after the last report; standing still it sends nothing.",
        since: RETAIL,
        evidence: Evidence::Private("AC2-EVID-SESS-POSITION"),
        station: "auberean-client-runtime::cpu::world::reports::reports_go_on_a_change_a_jump_and_every_fifth_of_a_second_while_moving",
        tier: Tier::Cpu,
    },
    behaviour! {
        id: "session.queues.data-before-events-and-the-logon-queue-keeps-only-chat",
        says: "The client receives on the control, events, logon and data queues, and each frame \
               reads the data queue, then the events queue, then the control queue (whose messages \
               it drops), then the logon queue, where it keeps only the chat service's. It sends \
               the data download's messages on the data queue, the character list's and chat on the \
               logon queue, and everything of the world on the game-events queue.",
        since: RETAIL,
        evidence: Evidence::Private("AC2-EVID-SESS-QUEUES"),
        station: "auberean-client-net::cpu::session::the_client_reads_the_data_queue_first_and_keeps_only_chat_from_the_logon_queue",
        tier: Tier::Cpu,
    },
    behaviour! {
        id: "session.replay.every-2017-game-login-replays-through-the-client",
        says: "Every recorded January 2017 game login, its servers played from the recording, \
               replays through the client to its end: the login connection held until the server \
               lets it go, every world login the recorded client made made again by following the \
               referrals, and the player of every world entry made and held.",
        since: RETAIL,
        evidence: Evidence::Private("AC2-EVID-SESS-REPLAY"),
        station: "auberean-headless::corpus::sessions::every_recorded_2017_game_login_replays_through_the_client",
        tier: Tier::Corpus,
    },
    behaviour! {
        id: "session.replay.what-the-client-sends-equals-the-recorded-client",
        says: "Through every recorded 2017 game login, what the client sends of its own accord \
               equals the recorded client's: its data versions byte for byte, its entries into the \
               world and its log-offs, its scene-drawn reports, its position reports' cadence and \
               flags, every blob's ordering type, and the stamps of its data versions, entries and \
               log-offs.",
        since: RETAIL,
        evidence: Evidence::Private("AC2-EVID-SESS-SENDS"),
        station: "auberean-headless::corpus::sessions::what_the_client_sends_on_its_own_equals_the_recorded_client",
        tier: Tier::Corpus,
    },
    behaviour! {
        id: "session.send.every-message-stamped-from-its-counter",
        says: "Each message the client sends carries the ordering type and stamp of its counter: \
               the game-events queue's are ordered, stamped from a counter counted up before each \
               send and started again at world entry; the data download's and chat have their own \
               type, stamped with the count so far from 0; everything else is unordered, counted \
               up before each send. One blob sequence, from 0, runs across every connection.",
        since: RETAIL,
        evidence: Evidence::Private("AC2-EVID-SESS-BLOB-STAMPS"),
        station: "auberean-client-net::cpu::session::every_message_is_stamped_from_its_counter_and_the_sequence_starts_at_zero",
        tier: Tier::Cpu,
    },
    behaviour! {
        id: "session.server.a-character-entering-again-gives-the-journal-its-quests",
        says: "A character that logged off with a quest under way enters again: the whole-list                notice the scripts send the journal at the session's start carries the quest, as                the journal asks for it.",
        since: RETAIL,
        evidence: Evidence::Private("AC2-EVID-SESS-JOURNAL-AT-ENTRY"),
        station: "yalain-testkit::dat::session::a_character_entering_again_gives_the_journal_its_quests",
        tier: Tier::Dat,
    },
    behaviour! {
        id: "session.server.a-dead-player-comes-back-by-the-dialogs-ask",
        says: "A player killed asks to be brought back, as death's dialog does, and is told it lives again.",
        since: RETAIL,
        evidence: Evidence::Private("AC2-EVID-SESS-RESURRECT"),
        station: "yalain-testkit::dat::session::a_dead_player_asks_and_comes_back",
        tier: Tier::Dat,
    },
    behaviour! {
        id: "session.server.a-looted-item-with-its-places-in-its-qualities-goes-where-they-say",
        says: "An item whose template keeps its places only in its qualities record, used from the \
               pack, goes on the body where they say and leaves what is in hand there.",
        since: RETAIL,
        evidence: Evidence::Private("AC2-EVID-SERVER-PLACES-IN-QUALITIES"),
        station: "yalain-testkit::dat::session::looted_moccasins_used_go_on_the_feet_and_the_weapon_stays_in_hand",
        tier: Tier::Dat,
    },
    behaviour! {
        id: "session.server.an-admin-passes-through-and-cannot-die",
        says: "An admin's typed ethereality, never-say-die and attackable commands run the game's own \
               scripts to calls the server honors: what it passes through is told to every client that \
               knows it, it cannot die, and creatures leave it alone; the override is lifted as typed.",
        since: TOOLING,
        evidence: Evidence::Private("AC2-EVID-SESSION-ADMIN-GODMODE"),
        station: "yalain-testkit::dat::admin::an_admin_types_its_way_out_of_harm_and_through_the_world",
        tier: Tier::Dat,
    },
    behaviour! {
        id: "session.server.an-admins-command-reaches-the-server-and-is-answered",
        says: "An admin's command that goes to the server runs the game's scripts to a plain call, the \
               server answers it, and the client's own admin script shows the line.",
        since: RETAIL,
        evidence: Evidence::Private("AC2-EVID-SESSION-ADMIN-ANSWER"),
        station: "yalain-testkit::dat::admin::an_admins_server_command_is_answered_in_the_clients_window",
        tier: Tier::Dat,
    },
    behaviour! {
        id: "session.server.an-item-without-worn-properties-changes-the-body-as-2017-showed",
        says: "An item whose template names no worn appearance in its properties gives its wearer \
               the look the 2017 servers sent for it.",
        since: RETAIL,
        evidence: Evidence::Private("AC2-EVID-SERVER-WORN-IN-CODE"),
        station: "yalain-testkit::corpus::keeping::every_recorded_look_of_a_template_without_worn_properties_is_in_the_table",
        tier: Tier::Corpus,
    },
    behaviour! {
        id: "session.server.our-client-enters-with-the-weapon-it-left-wielded",
        says: "Our client wields its starting sword on our server, logs off and enters again: its \
               model has the sword hanging from the player and wielded by it, and the server \
               keeps it wielded.",
        since: RETAIL,
        evidence: Evidence::Private("AC2-EVID-SESS-KEEP-WIELDED"),
        station: "yalain-testkit::dat::session::our_client_enters_again_with_the_weapon_it_left_wielded",
        tier: Tier::Dat,
    },
    behaviour! {
        id: "session.server.our-client-plays-a-whole-session-on-our-server",
        says: "Our client and our server play a whole session together: the character list, a \
               character made and entered, the player walking with its reports taken, the people \
               and creatures around seen, a creature selected (the selection, its examination and \
               the look at it told in that order) and examined, attacked until it dies, its \
               corpse opened by using it and its item taken into the pack the player's scripts \
               keep, a saying heard, the hall's shard taken to the town through portal space, a \
               log-off and an entry again where the character left. Every request is the player's \
               scripts' own, as the game's panels ask for it.",
        since: RETAIL,
        evidence: Evidence::Private("AC2-EVID-SESS-WHOLE"),
        station: "yalain-testkit::dat::session::our_client_plays_a_whole_session_on_our_server",
        tier: Tier::Dat,
    },
    behaviour! {
        id: "session.server.our-client-plays-a-whole-session-on-the-server-executable",
        says: "The same whole session over real sockets: our client on loopback against the \
               server's executable started with the install and a small world of its data.",
        since: RETAIL,
        evidence: Evidence::Private("AC2-EVID-SESS-LOOPBACK"),
        station: "yalain::live::session::our_client_plays_a_whole_session_on_the_server_executable",
        tier: Tier::Live,
    },
    behaviour! {
        id: "session.server.our-client-takes-a-worlds-overlay-as-it-logs-in",
        says: "Our client says it keeps overlays and which files it holds ahead of what it holds,                stores a world's records it lacks as it logs in, and reads them as its own.",
        since: THIS_CLIENT,
        divergence: "CD2-world-overlay",
        evidence: Evidence::Private("AC2-EVID-SESSION-OVERLAY-TAKEN"),
        station: "yalain-testkit::dat::world_patch::our_client_takes_a_worlds_overlay_as_it_logs_in_and_reads_it",
        tier: Tier::Dat,
    },
    behaviour! {
        id: "session.server.our-clients-kept-window-places-come-back-on-the-next-entry",
        says: "Our client keeps a panel's place through the player's interface into its options \
               and sends it to our server to keep; logged off and entered again, its scripts \
               answer the place it kept. A new character keeps none.",
        since: RETAIL,
        evidence: Evidence::Private("AC2-EVID-SESS-KEEP-PLACES"),
        station: "yalain-testkit::dat::session::our_clients_kept_window_places_come_back_on_the_next_entry",
        tier: Tier::Dat,
    },
    behaviour! {
        id: "session.server.our-clients-options-sent-at-log-off-come-back-on-the-next-entry",
        says: "An option our client changes in play comes back at the next entry when the play \
               screen's end sends the options to our server before the log off, and not otherwise.",
        since: RETAIL,
        evidence: Evidence::Private("AC2-EVID-SESS-KEEP-OPTIONS"),
        station: "yalain-testkit::dat::session::our_clients_options_sent_at_log_off_come_back_on_the_next_entry",
        tier: Tier::Dat,
    },
    behaviour! {
        id: "session.time.a-players-position-leaves-the-game-clock-alone",
        says: "A position the server gives for the player places its body and leaves the game \
               clock alone, whether its time is ahead of the clock or behind it: only the \
               server's time-sync sets the clock.",
        since: RETAIL,
        evidence: Evidence::Private("AC2-EVID-SESS-POSITION-CLOCK"),
        station: "auberean-client-runtime::cpu::world::clock::a_players_position_leaves_the_game_clock_alone",
        tier: Tier::Cpu,
    },
    behaviour! {
        id: "session.time.a-position-of-the-players-own-moves-it-by-its-stamps",
        says: "A position of the player's own moves its body only by a newer stamp: a teleport \
               puts it there facing the position's heading, a forced position puts it there \
               facing as it faced, and any other moves nothing.",
        since: RETAIL,
        evidence: Evidence::Private("AC2-EVID-SESS-PLAYER-POSITION"),
        station: "auberean-client-runtime::cpu::world::clock::a_position_of_the_players_own_moves_it_by_its_stamps",
        tier: Tier::Cpu,
    },
    behaviour! {
        id: "session.time.a-position-of-the-players-own-with-an-impulse-throws-it",
        says: "A position of the player's own with an impulse and a newer stamp puts its body \
               there, facing as it faced unless the position teleports it, then throws it at the \
               position's velocity.",
        since: RETAIL,
        evidence: Evidence::Private("AC2-EVID-SESS-PLAYER-IMPULSE"),
        station: "auberean-client-runtime::cpu::world::clock::a_position_of_the_players_own_with_an_impulse_throws_it_once",
        tier: Tier::Cpu,
    },
    behaviour! {
        id: "session.time.a-thread-is-never-answered-an-earlier-elapsed-time",
        says: "The elapsed time a thread is answered never goes back: a reading earlier than the \
               last answer answers the last again, unless the last was not above zero.",
        since: RETAIL,
        evidence: Evidence::Private("AC2-EVID-SESS-ELAPSED"),
        station: "auberean-primitives::cpu::clock::a_thread_is_never_answered_an_earlier_elapsed_time",
        tier: Tier::Cpu,
    },
    behaviour! {
        id: "session.time.the-clock-follows-the-hosts-elapsed-time-and-never-runs-back",
        says: "The client's clock is the elapsed time plus the server's offset and never runs back \
               (an earlier reading moves it on by a millionth of a millionth of a second); the \
               server's time moves the offset only when more than a billionth of a second ahead, \
               and the local time follows at the next update.",
        since: RETAIL,
        evidence: Evidence::Private("AC2-EVID-SESS-CLOCK"),
        station: "auberean-primitives::cpu::clock::the_clock_follows_the_hosts_elapsed_time_and_never_runs_back",
        tier: Tier::Cpu,
    },
    behaviour! {
        id: "session.time.the-game-clock-is-the-elapsed-time-until-the-server-moves-it",
        says: "The game clock is the client's timer, brought up to the elapsed time at the start \
               of each frame: before the server's first time it is the elapsed time, and it never \
               runs back, though the host's clock may.",
        since: RETAIL,
        evidence: Evidence::Private("AC2-EVID-SESS-GAME-CLOCK"),
        station: "auberean-client-runtime::cpu::world::clock::the_game_clock_is_the_elapsed_time_until_the_server_moves_it",
        tier: Tier::Cpu,
    },
    behaviour! {
        id: "session.time.the-game-clock-moves-forward-to-the-servers-time-sync",
        says: "The server's clock in a packet's time-sync header sets the game clock when it is \
               ahead of it; the game clock then runs on with the client's own, and is never set \
               back.",
        since: RETAIL,
        evidence: Evidence::Private("AC2-EVID-SESS-TIME-SYNC"),
        station: "auberean-client-runtime::cpu::world::clock::the_game_clock_moves_forward_to_the_servers_time_sync",
        tier: Tier::Cpu,
    },
];
