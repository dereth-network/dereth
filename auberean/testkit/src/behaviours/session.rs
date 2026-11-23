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
        id: "session.controls.a-release-ends-the-press-its-source-began",
        says: "A control's release ends the press its own source began, whatever else was pressed \
               between; the same control pressed by another source stays, and a source's press of \
               another control takes the place of what it pressed before.",
        since: RETAIL,
        evidence: Evidence::Private("AC2-EVID-CONTROLS-SOURCES"),
        station: "auberean-client-runtime::cpu::world::controls::a_release_ends_the_press_its_source_began",
        tier: Tier::Cpu,
    },
    behaviour! {
        id: "session.controls.held-latched-and-single-press-controls-through-the-contract",
        says: "Controls pressed through the contract are carried as the data's action map says each \
               toggles: held from press to release, a latched run until pressed again or ended by \
               forward or backward, a jump one press for its frame; a modifier held chooses the \
               values; the movement controls begun are told once; a client with no action map \
               carries none.",
        since: RETAIL,
        evidence: Evidence::Private("AC2-EVID-CONTROLS-TOGGLES"),
        station: "auberean-client-runtime::cpu::world::controls::held_latched_and_single_press_controls_through_the_contract",
        tier: Tier::Cpu,
    },
    behaviour! {
        id: "session.controls.letting-go-keeps-a-run-and-a-press-and-typing-keeps-modifiers",
        says: "Letting go of the controls in force ends the held ones and leaves a latched run and \
               a single press; for a text entry the modifiers' and the interface's stay too. A \
               mouse look control flips mouse look, in which a turn strafes while the mouse steers; \
               the camera's and the interface's controls press the camera and ask for use.",
        since: RETAIL,
        evidence: Evidence::Private("AC2-EVID-CONTROLS-LETTING-GO"),
        station: "auberean-client-runtime::cpu::world::controls::letting_go_keeps_a_run_and_a_press_and_typing_keeps_modifiers",
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
        id: "session.driver.a-fixed-step-run-enters-the-world-the-same-every-time",
        says: "Our choice: the SDK's driver runs the client's frames on a clock and a port the host \
               gives it; on a fixed-step clock over a network in memory, the client logs in, enters \
               the world as its events tell it and says the scene is drawn, and a second run sends \
               the same messages on the same frames.",
        since: TOOLING,
        evidence: Evidence::Private("AC2-EVID-DRIVER-FIXED-STEP"),
        station: "auberean-client-sdk::cpu::driver::the_driver_on_a_fixed_step_clock_enters_the_world_the_same_way_every_run",
        tier: Tier::Cpu,
    },
    behaviour! {
        id: "session.driver.a-frame-hook-hears-each-notice-once",
        says: "Our choice: the frame's hook is handed what the player's scripts told the panels since \
               it last ran, read as the contract carries it, once and in the order told, whenever \
               the host takes the notices (those taken between frames in the next); the host still \
               takes every one, and none from before the hook was installed reach it.",
        since: TOOLING,
        evidence: Evidence::Private("AC2-EVID-DRIVER-HOOK-NOTICES"),
        station: "auberean-client-runtime::cpu::world::hook::a_frame_hook_hears_each_notice_once_in_order_whenever_the_host_takes_them",
        tier: Tier::Cpu,
    },
    behaviour! {
        id: "session.driver.a-frame-hook-sees-each-event-once-and-its-actions-are-carried-out",
        says: "Our choice: a hook installed through the driver runs once a frame, at its end, and \
               is handed every event since it last ran, once and in the order the host takes them \
               (those from between frames in the next); the actions it answers with are carried \
               out as a front end's are. Taken out, it runs no more.",
        since: TOOLING,
        evidence: Evidence::Private("AC2-EVID-DRIVER-FRAME-HOOK"),
        station: "auberean-client-sdk::cpu::driver::a_frame_hook_sees_each_event_once_in_order_and_its_actions_are_carried_out",
        tier: Tier::Cpu,
    },
    behaviour! {
        id: "session.events.a-refused-entry-is-told-and-the-session-is-back-at-the-list",
        says: "Our choice: an entry the server refuses is told to a front end with its reason, \
               once, and the session goes back from entering to the character list.",
        since: TOOLING,
        evidence: Evidence::Private("AC2-EVID-EVENTS-ENTRY-REFUSED"),
        station: "auberean-headless::cpu::world_entry::a_refused_entry_is_told_and_the_session_goes_back_to_the_list",
        tier: Tier::Cpu,
    },
    behaviour! {
        id: "session.events.the-login-and-the-sessions-end-are-told-once",
        says: "Our choice: the client tells a front end how its login goes (each login going out, \
               then the server's answer acknowledged), that it is connecting, and once the server \
               is gone, that the session is over and how it ended: each once, in order.",
        since: TOOLING,
        evidence: Evidence::Private("AC2-EVID-EVENTS-SESSION-END"),
        station: "auberean-client-runtime::cpu::world::session::the_login_and_the_sessions_end_are_told_once_each_in_order",
        tier: Tier::Cpu,
    },
    behaviour! {
        id: "session.events.the-session-tells-its-phases-in-order",
        says: "Our choice: the client tells a front end where its session stands as it moves \
               (connecting, at the character list with the account's characters, entering the \
               world, in it, back at the list after a log-off, gone once closed), each phase once, \
               in order: a front end driven by the events alone can log in and play.",
        since: TOOLING,
        evidence: Evidence::Private("AC2-EVID-EVENTS-SESSION-PHASES"),
        station: "auberean-headless::cpu::world_entry::the_client_tells_its_sessions_phases_in_order_and_the_characters_it_enters_from",
        tier: Tier::Cpu,
    },
    behaviour! {
        id: "session.headless.a-control-pressed-held-and-let-go-is-in-force-for-its-frames",
        says: "Our choice: with the action map read from the data, a headless script's begin presses \
               a control and its end lets it go, hold keeps it pressed for its seconds and press for \
               one frame; the movement the controls in force ask for each frame says so, and nothing \
               is in force after.",
        since: TOOLING,
        evidence: Evidence::Private("AC2-EVID-HEADLESS-CONTROLS"),
        station: "auberean-headless::dat::script::a_control_begun_held_pressed_and_let_go_is_in_force_for_its_frames",
        tier: Tier::Dat,
    },
    behaviour! {
        id: "session.headless.a-script-logs-in-enters-tells-the-view-and-logs-off",
        says: "Our choice: a headless script logs in, enters the world as the character it names, \
               tells where the player stands and the view as text, dumps the session's events in \
               order and logs off back to the character list, the entry and the log-off sent once \
               each.",
        since: TOOLING,
        evidence: Evidence::Private("AC2-EVID-HEADLESS-RUN-SESSION"),
        station: "auberean-headless::cpu::run::a_script_logs_in_enters_tells_the_view_dumps_the_events_and_logs_off",
        tier: Tier::Cpu,
    },
    behaviour! {
        id: "session.headless.a-script-reads-one-command-a-line",
        says: "Our choice: the headless client's script is one command a line, a verb and its words; \
               say keeps all after it, a comment runs from # to the end of any other line, and a \
               wrong line is told with its number and what is wrong.",
        since: TOOLING,
        evidence: Evidence::Private("AC2-EVID-HEADLESS-SCRIPT-LINES"),
        station: "auberean-headless::cpu::script::every_verb_reads_as_its_command_and_a_wrong_line_is_told_with_its_number",
        tier: Tier::Cpu,
    },
    behaviour! {
        id: "session.headless.a-script-selects-uses-and-says-through-actions",
        says: "Our choice: without the player's scripts, a headless script's selections and uses are \
               the client's own, each with the server's call, and the view says what is selected; a \
               line said is told.",
        since: TOOLING,
        evidence: Evidence::Private("AC2-EVID-HEADLESS-RUN-SELECT-USE"),
        station: "auberean-headless::cpu::run::a_script_selects_uses_and_says_through_actions_alone",
        tier: Tier::Cpu,
    },
    behaviour! {
        id: "session.headless.a-script-walks-selects-uses-and-says-with-no-window",
        says: "Our choice: with no window, a headless script walks the player's body through the \
               world to a wall, selects the player, uses it and says a line; the view says what is \
               selected and the client makes the server's calls for the selection and the use.",
        since: TOOLING,
        evidence: Evidence::Private("AC2-EVID-HEADLESS-WALK-SCRIPT"),
        station: "auberean-headless::dat::script::a_script_walks_the_body_through_the_hall_selects_the_player_uses_it_and_says_a_line",
        tier: Tier::Dat,
    },
    behaviour! {
        id: "session.headless.actions-read-from-their-text-form",
        says: "Our choice: the contract's actions have a text form for a headless script, the \
               action's name and its fields in order; a wrong one says what the action takes, and an \
               unknown name lists the names.",
        since: TOOLING,
        evidence: Evidence::Private("AC2-EVID-HEADLESS-ACTION-TEXT"),
        station: "auberean-headless::cpu::script::every_action_named_reads_from_its_words_and_a_wrong_one_says_what_it_takes",
        tier: Tier::Cpu,
    },
    behaviour! {
        id: "session.headless.an-entry-refused-is-asked-again-two-seconds-later",
        says: "Our choice: a headless script's entry that the server refuses for the character still \
               in the world is told and asked again two seconds later, and the second goes in.",
        since: TOOLING,
        evidence: Evidence::Private("AC2-EVID-HEADLESS-RUN-REFUSED"),
        station: "auberean-headless::cpu::run::an_entry_the_server_refuses_is_asked_again_two_seconds_later",
        tier: Tier::Cpu,
    },
    behaviour! {
        id: "session.headless.offers-are-answered-as-the-standing-orders-say",
        says: "Our choice: the headless client answers what other players offer as its standing \
               orders say: agreeing, it takes a place in a fellowship and an oath, opens a trade \
               another asks for and accepts it once the other side has, once; offering, it offers the \
               first item of its pack when a trade opens.",
        since: TOOLING,
        evidence: Evidence::Private("AC2-EVID-HEADLESS-RUN-OFFERS"),
        station: "auberean-headless::cpu::run::offers_are_answered_as_the_standing_orders_say",
        tier: Tier::Cpu,
    },
    behaviour! {
        id: "session.headless.the-built-in-script-does-what-its-flags-did",
        says: "Our choice: the headless client's built-in script for its line and trade flags says \
               the line three seconds after entering and every ten after, asks for the trade at the \
               third second, and logs off when its time is up.",
        since: TOOLING,
        evidence: Evidence::Private("AC2-EVID-HEADLESS-BUILTIN-RUN"),
        station: "auberean-headless::cpu::builtin::the_built_in_script_says_asks_for_a_trade_and_logs_off_when_its_flags_say",
        tier: Tier::Cpu,
    },
    behaviour! {
        id: "session.headless.the-flags-make-the-built-in-script",
        says: "Our choice: the headless client's flags make a script of its commands that does what \
               they ask, timed from the entry: the walk's legs, the trade, the makings, the line \
               said, and at the end the slots told, the log-off and the stop.",
        since: TOOLING,
        evidence: Evidence::Private("AC2-EVID-HEADLESS-BUILTIN-SCRIPT"),
        station: "auberean-headless::cpu::builtin::the_flags_make_the_script_that_does_what_they_ask",
        tier: Tier::Cpu,
    },
    behaviour! {
        id: "session.headless.the-walk-flag-runs-and-turns-with-no-window",
        says: "Our choice: with no window, the headless client's built-in walk runs the player's body \
               forward for two seconds from the second second, then lets the run go and turns right \
               for one.",
        since: TOOLING,
        evidence: Evidence::Private("AC2-EVID-HEADLESS-WALK-FLAG"),
        station: "auberean-headless::dat::script::the_built_in_walk_runs_the_body_forward_for_two_seconds_then_turns_for_one",
        tier: Tier::Dat,
    },
    behaviour! {
        id: "session.headless.tick-run-and-at-wait-their-frames-and-seconds",
        says: "Our choice: a headless script's tick runs its frames, run its seconds of the clock, \
               and at waits until its seconds after the entry (nothing when already past); a command \
               that takes no time runs in the same frame as the one before it.",
        since: TOOLING,
        evidence: Evidence::Private("AC2-EVID-HEADLESS-RUN-WAITS"),
        station: "auberean-headless::cpu::run::tick_run_and_at_wait_their_frames_and_seconds",
        tier: Tier::Cpu,
    },
    behaviour! {
        id: "session.headless.typed-lines-run-at-once-and-scripts-carry-plugin-lines",
        says: "Our choice: a line typed into the headless client runs at once when it takes no time \
               and is told so when it takes time; a script's plugin lines reach the plugin host, and \
               a line that is none is told.",
        since: TOOLING,
        evidence: Evidence::Private("AC2-EVID-HEADLESS-RUN-TYPED"),
        station: "auberean-headless::cpu::run::typed_lines_run_at_once_and_a_scripts_plugin_lines_reach_the_plugins",
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
        id: "session.login.a-login-server-lost-before-its-first-packet-fails-the-login",
        says: "A login server that answers the login and is then given up for dead before its \
               first ordinary packet fails the login, once, with the generic connection error.",
        since: RETAIL,
        evidence: Evidence::Private("AC2-EVID-SESS-LOGIN-HANDSHAKE-LOST"),
        station: "auberean-client-net::cpu::server_lost::a_login_server_lost_before_its_first_packet_fails_the_login",
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
        id: "session.login.each-attempt-is-told-out-of-twenty",
        says: "Each login that goes out is told as its attempt of the twenty the client makes, \
               for the connecting screen to count.",
        since: RETAIL,
        evidence: Evidence::Private("AC2-EVID-SESS-LOGIN-ATTEMPTS"),
        station: "auberean-client-net::cpu::server_lost::each_login_attempt_is_told_out_of_twenty",
        tier: Tier::Cpu,
    },
    behaviour! {
        id: "session.login.the-login-servers-acknowledgement-is-told",
        says: "Each acknowledgement of the login server's connection request is told, as the \
               client starts waiting for that server's first packet.",
        since: RETAIL,
        evidence: Evidence::Private("AC2-EVID-SESS-LOGIN-ACKNOWLEDGED"),
        station: "auberean-client-net::cpu::server_lost::the_login_servers_acknowledgement_is_told_before_it_connects",
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
        id: "session.message.bytes-after-a-messages-fields-are-not-read",
        says: "A message from a server with bytes after its fields (an end padded with zeros) is \
               read for its fields and takes effect; the bytes after them are not read.",
        since: RETAIL,
        evidence: Evidence::Private("AC2-EVID-SESS-MESSAGE-TRAILING-BYTES"),
        station: "auberean-client-net::cpu::server_lost::a_message_with_bytes_after_its_fields_is_read_for_its_fields",
        tier: Tier::Cpu,
    },
    behaviour! {
        id: "session.players.a-count-tells-the-hub-the-worlds-address-and-nothing-else",
        says: "A count of a world's players tells the Dereth Network's hub the world's address and \
               port, the count and the game, and nothing else: no account, no character and no id \
               of this client.",
        since: THIS_CLIENT,
        divergence: "CD2-reports-player-count",
        evidence: Evidence::Private("AC2-EVID-PLAYER-COUNT-WHAT-IT-SAYS"),
        station: "auberean-client::bin.auberean::player_count::tests::a_report_says_the_worlds_address_the_count_and_the_game_and_nothing_else",
        tier: Tier::Cpu,
    },
    behaviour! {
        id: "session.players.a-report-that-fails-is-forgotten",
        says: "A hub that is gone or refuses a report is nobody's concern: the report ends, with \
               nothing shown and nothing retried.",
        since: THIS_CLIENT,
        divergence: "CD2-reports-player-count",
        evidence: Evidence::Private("AC2-EVID-PLAYER-COUNT-FAILS"),
        station: "auberean-client::bin.auberean::player_count::tests::a_hub_that_is_gone_or_refuses_is_nobodys_concern",
        tier: Tier::Cpu,
    },
    behaviour! {
        id: "session.players.a-world-on-a-private-network-is-never-reported",
        says: "A world on this computer or a private network is never reported under its own address.",
        since: THIS_CLIENT,
        divergence: "CD2-reports-player-count",
        evidence: Evidence::Private("AC2-EVID-PLAYER-COUNT-PRIVATE"),
        station: "auberean-client::bin.auberean::player_count::tests::only_a_world_on_the_internet_is_reported_under_its_own_address",
        tier: Tier::Cpu,
    },
    behaviour! {
        id: "session.players.a-worlds-line-of-how-many-others-are-on-counts-its-players",
        says: "A line the server writes out itself saying, and saying only, how many other players \
               are logged in counts the world's players, the player with them, once; a line of the \
               game's own strings, or one saying anything else, counts nothing.",
        since: THIS_CLIENT,
        divergence: "CD2-reports-player-count",
        evidence: Evidence::Private("AC2-EVID-PLAYER-COUNT-WELCOME-LINE"),
        station: "auberean-client-runtime::cpu::world::players::a_worlds_welcome_line_of_how_many_others_are_on_is_taken_once",
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
        says: "A turn smaller than 0.0002 radians, the way round through north included, is no \
               reason to send a report.",
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
        id: "session.selection.a-selection-that-may-no-longer-be-selected-is-let-go",
        says: "When the player's scripts say the object selected may no longer be selected, the \
               client lets the selection go at its next frame for every front end, each notice \
               heard once whether or not a front end took it first; a dead object stays \
               selected, and a notice about another object, one that may be selected or the \
               player itself changes nothing.",
        since: RETAIL,
        evidence: Evidence::Private("AC2-EVID-SELECTION-UNSELECTABLE"),
        station: "auberean-client-sdk::cpu::selection::a_selection_that_may_no_longer_be_selected_is_let_go_unless_dead",
        tier: Tier::Cpu,
    },
    behaviour! {
        id: "session.selection.actions-alone-select-by-the-keys-rules",
        says: "Through actions alone the selection keys' rules choose: the next of a kind nearest \
               first, then farther, round again past the last, the previous backward, the nearest, \
               the player itself and then the selection before it, the last selection, the last \
               attacker. Our choice: with no scripts the client keeps the selection, which the \
               view says, and makes the server's calls for selecting and using it.",
        since: TOOLING,
        evidence: Evidence::Private("AC2-EVID-SELECTION-ASKED-ALONE"),
        station: "auberean-client-sdk::cpu::selection::actions_alone_select_by_the_keys_rules_and_without_scripts_make_the_calls",
        tier: Tier::Cpu,
    },
    behaviour! {
        id: "session.selection.the-selection-goes-with-its-object",
        says: "An object selected that is taken out of the world is no longer selected; when the \
               player targets what attacks it, the attack on it stops and the nearest foe is \
               selected in its place.",
        since: RETAIL,
        evidence: Evidence::Private("AC2-EVID-SELECTION-GONE"),
        station: "auberean-client-sdk::cpu::selection::the_selection_goes_with_its_object_out_of_the_world",
        tier: Tier::Cpu,
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
        says: "A character that logged off with a quest under way enters again: the whole-list \
               notice the scripts send the journal at the session's start carries the quest, as \
               the journal asks for it.",
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
        id: "session.server.a-death-in-its-creatures-moment-is-a-fight-lost",
        says: "The session's helpers count a player that dies in the same moment as the creature it \
               fights, or while a helper that acts after the fight waits, as a fight lost: they fail \
               at once, and a session that may be played again is played again.",
        since: TOOLING,
        evidence: Evidence::Private("AC2-EVID-SESS-MUTUAL-KILL"),
        station: "yalain-testkit::dat::session::a_player_dying_in_the_moment_its_creature_dies_has_lost_the_fight",
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
        station: "yalain-testkit::corpus::keeping::every_recorded_look_of_a_template_without_worn_properties_is_one_the_world_gives",
        tier: Tier::Corpus,
    },
    behaviour! {
        id: "session.server.camera-based-movement-keeps-the-servers-word",
        says: "Our choice: under the orbit camera's camera-based movement, on loopback against the server's \
               executable, a player who walks, runs, backs up, turns in place, strafes both ways, runs while turning, jumps \
               standing and running, starts and stops, crosses a block's edge and back, and \
               closes in on a creature and fights it (and runs while the camera swings) draws no \
               correction of its place and no refused report.",
        since: THIS_CLIENT,
        divergence: "CD2-orbit-camera",
        evidence: Evidence::Private("AC2-EVID-SCHEME-CAMERA-SERVER-WORD"),
        station: "yalain::live::session::ways_of_moving::camera_based_movement_draws_no_correction_from_the_server_executable",
        tier: Tier::Live,
    },
    behaviour! {
        id: "session.server.character-based-movement-keeps-the-servers-word",
        says: "Our choice: the same session under the orbit camera's character-based movement draws no \
               correction of the player's place and no refused report.",
        since: THIS_CLIENT,
        divergence: "CD2-orbit-camera",
        evidence: Evidence::Private("AC2-EVID-SCHEME-CHARACTER-SERVER-WORD"),
        station: "yalain::live::session::ways_of_moving::character_based_movement_draws_no_correction_from_the_server_executable",
        tier: Tier::Live,
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
        id: "session.server.our-client-shows-a-worlds-shop-under-its-title",
        says: "Our client, sent a world's copy of a store template under a title of its own as it \
               logs in, reads the copy as the store's scripts ask for it and shows a keeper's shop \
               under that title, with the store's own sales.",
        since: TOOLING,
        evidence: Evidence::Private("AC2-EVID-SESSION-SHOP-TITLE"),
        station: "yalain-testkit::dat::world_patch::our_client_shows_a_worlds_shop_under_its_own_title",
        tier: Tier::Dat,
    },
    behaviour! {
        id: "session.server.our-client-takes-a-worlds-overlay-as-it-logs-in",
        says: "Our client says it keeps overlays and which files it holds ahead of what it holds, \
               stores a world's records it lacks as it logs in, and reads them as its own.",
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
        id: "session.server.the-games-own-steering-keeps-the-servers-word",
        says: "On loopback against the server's executable, a player steered as the game steers it who \
               walks, runs, backs up, turns in place, strafes both ways, runs while turning, jumps \
               standing and running, starts and stops, crosses a block's edge and back, and \
               closes in on a creature and fights it draws no correction of its place and no refused report.",
        since: RETAIL,
        evidence: Evidence::Private("AC2-EVID-SCHEME-GAME-SERVER-WORD"),
        station: "yalain::live::session::ways_of_moving::the_games_own_steering_draws_no_correction_from_the_server_executable",
        tier: Tier::Live,
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
    behaviour! {
        id: "session.view.the-view-counts-the-servers-calls",
        says: "Our choice: the view counts the server's calls the player's scripts ran and those \
               that ran to their end, as they stand at its refresh.",
        since: TOOLING,
        evidence: Evidence::Private("AC2-EVID-VIEW-CALLS"),
        station: "auberean-client-sdk::cpu::view::the_view_counts_the_servers_calls_run_and_run_to_their_end",
        tier: Tier::Cpu,
    },
    behaviour! {
        id: "session.view.the-view-says-what-the-server-and-the-scripts-told",
        says: "Our choice: the game's view a front end reads in place of the client tells the \
               player's vitals as the server gave them, the objects around the player named as \
               their scripts write them, and the selection as the player's scripts hold it, each \
               as the last refresh read it (the host refreshes it once a frame).",
        since: TOOLING,
        evidence: Evidence::Private("AC2-EVID-VIEW-SCRIPTS-TOLD"),
        station: "auberean-interface::dat::screens::the_view_says_what_the_server_and_the_scripts_told",
        tier: Tier::Dat,
    },
    behaviour! {
        id: "session.view.the-view-says-what-the-server-told",
        says: "Our choice: the game's view tells the account's characters as the server listed \
               them (their names, the slots, the account), then the player and where its body \
               stands once it is in the world; it says what its last refresh read until the host \
               refreshes it.",
        since: TOOLING,
        evidence: Evidence::Private("AC2-EVID-VIEW-SERVER-TOLD"),
        station: "auberean-client-sdk::cpu::view::the_view_lists_the_characters_then_the_player_where_the_server_put_it",
        tier: Tier::Cpu,
    },
    behaviour! {
        id: "session.walk.with-no-window-a-creature-in-the-way-holds-the-body-up",
        says: "Our choice: with no window, a creature the server puts in the player's way stands \
               where the server put it and holds the player's body up as the forward control \
               runs it there, short of the creature, before it slides off round it; the body \
               never runs through it, as a window's scene runs it.",
        since: TOOLING,
        evidence: Evidence::Private("AC2-EVID-SIM-CREATURE-IN-THE-WAY"),
        station: "auberean-headless::dat::walk::with_no_window_a_creature_in_the_way_holds_the_body_up",
        tier: Tier::Dat,
    },
    behaviour! {
        id: "session.walk.with-no-window-the-body-climbs-the-steps",
        says: "Our choice: with no window, the forward control held runs the player's body west \
               from the hall's starting place up the platform's steps, each lower than its \
               step-up height, and onto the platform, as a window's scene runs it.",
        since: TOOLING,
        evidence: Evidence::Private("AC2-EVID-SIM-STEPS"),
        station: "auberean-headless::dat::walk::with_no_window_the_forward_control_runs_the_body_up_the_steps",
        tier: Tier::Dat,
    },
    behaviour! {
        id: "session.walk.with-no-window-the-body-runs-to-a-wall-and-stops-there",
        says: "Our choice: with no window, the forward control held runs the player's body \
               through the hall's cells on their floor and through a doorway, and stops it \
               against a wall while the control is still held; the client reports it where it \
               stopped.",
        since: TOOLING,
        evidence: Evidence::Private("AC2-EVID-SIM-WALL"),
        station: "auberean-headless::dat::walk::with_no_window_the_forward_control_runs_the_body_to_a_wall_and_stops_it_there",
        tier: Tier::Dat,
    },
];
