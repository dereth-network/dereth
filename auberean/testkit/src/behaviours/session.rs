//! The session: the game login, the connections to the login and world servers, the queues, and
//! what the client says to the servers and when.
//!
//! A row claims what the September 2013 client does on the wire, held to the client itself and to
//! the recorded January 2017 sessions.

use super::{Behaviour, Evidence, Tier, RETAIL};

/// This subject's rows, in id order.
pub static ROWS: &[Behaviour] = &[
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
        station: "auberean-client-runtime::cpu::requests::a_typed_line_splits_into_words_with_quoted_runs_kept_whole",
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
        id: "session.login.closing-says-goodbye-on-every-connection",
        says: "Closing in the world says goodbye on every connection at once with no log-off \
               first, and nothing more is sent after it.",
        since: RETAIL,
        evidence: Evidence::Private("AC2-EVID-SESS-CLOSE-GOODBYE"),
        station: "auberean-headless::cpu::world_entry::closing_in_the_world_says_goodbye_on_every_connection",
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
        id: "session.position.a-report-packs-heading-motion-flags-and-stamps",
        says: "A position report carries the heading in its top byte (256 steps to a full turn), the \
               asked motion as one byte each for sideways, forward and up (127 for none), the jump \
               flag with the jump's velocity, the moving flag while the player moves or turns, and \
               the body's instance and teleport stamps.",
        since: RETAIL,
        evidence: Evidence::Private("AC2-EVID-SESS-POSITION-PACK"),
        station: "auberean-client-runtime::cpu::reports::a_report_packs_heading_motion_flags_and_stamps",
        tier: Tier::Cpu,
    },
    behaviour! {
        id: "session.position.reports-on-change-jump-and-every-fifth-of-a-second",
        says: "The client reports the player's position at once when the asked motion changes or \
               the player jumps; while the player moves or turns it reports again on the first frame \
               more than a fifth of a second after the last report; standing still it sends nothing.",
        since: RETAIL,
        evidence: Evidence::Private("AC2-EVID-SESS-POSITION"),
        station: "auberean-client-runtime::cpu::reports::reports_go_on_a_change_a_jump_and_every_fifth_of_a_second_while_moving",
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
        id: "session.time.the-game-clock-moves-forward-to-the-servers-time-sync",
        says: "The server's clock in a packet's time-sync header sets the game clock when it is \
               ahead of it; the game clock then runs on with the client's own, and is never set \
               back.",
        since: RETAIL,
        evidence: Evidence::Private("AC2-EVID-SESS-TIME-SYNC"),
        station: "auberean-client-runtime::cpu::clock::the_game_clock_moves_forward_to_the_servers_time_sync",
        tier: Tier::Cpu,
    },
];
