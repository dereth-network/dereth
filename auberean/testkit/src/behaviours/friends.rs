//! The friends stream: the protocol between the client and its world's friends service. The
//! elements are the friends service's own where its traffic shows them; the stream that carries
//! them (the frames, the greeting, the ticket, keep-alive and the waits between reconnects) is
//! this network's own.

use super::{Behaviour, Evidence, Tier, RETAIL, THIS_CLIENT};

/// This subject's rows, in id order.
pub static ROWS: &[Behaviour] = &[
    behaviour! {
        id: "friends.client.disconnect-says-goodbye",
        says: "A client that stops says goodbye with the reason shutdown, closes the stream, answers \
               every request still waiting with the not-connected code, and dials no more until \
               asked again.",
        since: THIS_CLIENT,
        divergence: "CD2-friends-stream",
        evidence: Evidence::Private("AC2-EVID-FRIENDS-DISCONNECT"),
        station: "auberean-friends::cpu::client::a_client_that_disconnects_says_goodbye_answers_what_it_owed_and_dials_no_more",
        tier: Tier::Cpu,
    },
    behaviour! {
        id: "friends.client.every-request-answered",
        says: "Every request gets exactly one answer: the world's; the not-connected code when the \
               stream was not up or went down before the world answered; or the invalid-argument \
               code for one holding a value out of its bounds, which is never sent. An answer that \
               matches no request waiting for one is dropped.",
        since: THIS_CLIENT,
        divergence: "CD2-friends-stream",
        evidence: Evidence::Private("AC2-EVID-FRIENDS-EVERY-REQUEST"),
        station: "auberean-friends::cpu::client::every_request_gets_exactly_one_answer_even_across_a_lost_stream",
        tier: Tier::Cpu,
    },
    behaviour! {
        id: "friends.client.greeting",
        says: "A client opens its stream with the four bytes DNF1 and a greeting naming the version, \
               its software and its locale, is up once the world welcomes it, and gives up a world \
               that has not welcomed it within 10 seconds.",
        since: THIS_CLIENT,
        divergence: "CD2-friends-stream",
        evidence: Evidence::Private("AC2-EVID-FRIENDS-CLIENT-GREETING"),
        station: "auberean-friends::cpu::client::the_client_opens_with_the_preamble_and_its_greeting_and_is_up_at_welcome",
        tier: Tier::Cpu,
    },
    behaviour! {
        id: "friends.client.reconnect-waits",
        says: "After a stream ends the client dials again, waiting 1, 2, 5 and 10 seconds, then \
               every 30 seconds, between attempts; asking it to connect does not cut a wait short.",
        since: THIS_CLIENT,
        divergence: "CD2-friends-stream",
        evidence: Evidence::Private("AC2-EVID-FRIENDS-RECONNECT-WAITS"),
        station: "auberean-friends::cpu::client::the_client_waits_one_two_five_ten_then_thirty_seconds_between_dials",
        tier: Tier::Cpu,
    },
    behaviour! {
        id: "friends.client.reconnect-waits-start-again",
        says: "A stream that stood for a minute starts the client's waits again from one second; \
               one that stood less goes on from where they were.",
        since: THIS_CLIENT,
        divergence: "CD2-friends-stream",
        evidence: Evidence::Private("AC2-EVID-FRIENDS-RECONNECT-RESET"),
        station: "auberean-friends::cpu::client::a_stream_that_stood_a_minute_starts_the_waits_again_from_one_second",
        tier: Tier::Cpu,
    },
    behaviour! {
        id: "friends.client.replaced-waits-longest",
        says: "A client whose stream was replaced by a newer one for the same account waits the \
               longest step, 30 seconds, each time, so that two clients of one account do not take \
               turns.",
        since: THIS_CLIENT,
        divergence: "CD2-friends-stream",
        evidence: Evidence::Private("AC2-EVID-FRIENDS-REPLACED-WAIT"),
        station: "auberean-friends::cpu::client::after_being_replaced_the_client_waits_the_longest_step",
        tier: Tier::Cpu,
    },
    behaviour! {
        id: "friends.element.codes-and-modes",
        says: "The friends service's result codes (done, a refused ticket, a banned account, no \
               ticket yet, a second ticket, not connected, no such user, not online, timed out, \
               denied) and its friend states (a bit set: online 1, away 2, not to be disturbed 4, \
               nothing set offline; away is 3 and not to be disturbed 5 for one online) are the \
               original service's own.",
        since: RETAIL,
        evidence: Evidence::Private("AC2-EVID-FRIENDS-CODES-MODES"),
        station: "auberean-friends::cpu::elements::the_result_codes_and_friend_states_are_the_friends_services_own",
        tier: Tier::Cpu,
    },
    behaviour! {
        id: "friends.element.malformed-refused",
        says: "Text that is not one element of the stream's XML, or a known element missing what it \
               needs or holding a value out of bounds, is refused.",
        since: THIS_CLIENT,
        divergence: "CD2-friends-stream",
        evidence: Evidence::Private("AC2-EVID-FRIENDS-MALFORMED"),
        station: "auberean-friends::cpu::elements::text_that_is_no_element_or_breaks_an_elements_bounds_is_refused",
        tier: Tier::Cpu,
    },
    behaviour! {
        id: "friends.element.read-as-captured",
        says: "Every element of the friends service its recorded traffic holds (the friends list, \
               adding and removing a friend, a friend's state, the ignore list, the state flags, \
               away and back, the ticket's answer) reads as the service wrote it, spacing and hex \
               as recorded.",
        since: RETAIL,
        evidence: Evidence::Private("AC2-EVID-FRIENDS-CAPTURED"),
        station: "auberean-friends::cpu::elements::the_friends_services_own_elements_read_as_its_traffic_wrote_them",
        tier: Tier::Cpu,
    },
    behaviour! {
        id: "friends.element.round-trip",
        says: "Every element of the friends stream reads as its fields and writes back to the same \
               text.",
        since: THIS_CLIENT,
        divergence: "CD2-friends-stream",
        evidence: Evidence::Private("AC2-EVID-FRIENDS-ROUND-TRIP"),
        station: "auberean-friends::cpu::elements::every_element_reads_as_its_fields_and_writes_back_the_same",
        tier: Tier::Cpu,
    },
    behaviour! {
        id: "friends.element.unknown-skipped",
        says: "A reader skips an element it does not know, ignores attributes and children it does \
               not know, takes an unknown farewell as protocol, and goes by a list's children \
               rather than its count.",
        since: THIS_CLIENT,
        divergence: "CD2-friends-stream",
        evidence: Evidence::Private("AC2-EVID-FRIENDS-UNKNOWN"),
        station: "auberean-friends::cpu::elements::what_a_reader_does_not_know_it_skips_or_reads_leniently",
        tier: Tier::Cpu,
    },
    behaviour! {
        id: "friends.layer.a-heard-tell-shows-the-tellers-meta-letters-and-escapes-its-text",
        says: "A tell carries the teller's meta letters and chat flags as extra data. A heard tell \
               shows the teller's name with those letters in brackets after it, and its text \
               escaped so that nothing in it reads as markup; a tell with no extra data has no \
               flags and a bare name.",
        since: RETAIL,
        evidence: Evidence::Private("AC2-EVID-GUN-TELL"),
        station: "auberean-client-runtime::cpu::friends::a_heard_tell_shows_the_tellers_meta_letters_and_escapes_its_text",
        tier: Tier::Cpu,
    },
    behaviour! {
        id: "friends.layer.a-lost-stream-is-dialed-again-only-once-one-stood",
        says: "A first attempt to reach the friends service that fails is not tried again in that \
               login. A connection that stood and is then lost is dialed again, and coming back is \
               told as coming back.",
        since: RETAIL,
        evidence: Evidence::Private("AC2-EVID-GUN-CONNECT"),
        station: "auberean-client-runtime::cpu::friends::a_lost_stream_is_dialed_again_only_once_one_stood",
        tier: Tier::Cpu,
    },
    behaviour! {
        id: "friends.layer.a-refused-ticket-comes-back-refused-and-a-banned-account-banned",
        says: "A ticket the friends service refuses comes back as refused, and a banned account's \
               as banned; the account already ticketed keeps its own answer.",
        since: RETAIL,
        evidence: Evidence::Private("AC2-EVID-GUN-TICKET"),
        station: "auberean-client-runtime::cpu::friends::a_refused_ticket_comes_back_refused_and_a_banned_account_banned",
        tier: Tier::Cpu,
    },
    behaviour! {
        id: "friends.layer.a-request-while-the-service-is-down-is-refused-at-once",
        says: "While the friends service is down a request is refused at once and nothing comes \
               back for it later, so the scripts show nothing; a tell with no name or no text is \
               refused the same way.",
        since: RETAIL,
        evidence: Evidence::Private("AC2-EVID-GUN-REQUESTS"),
        station: "auberean-client-runtime::cpu::friends::a_request_while_the_service_is_down_is_refused_at_once",
        tier: Tier::Cpu,
    },
    behaviour! {
        id: "friends.layer.a-tells-code-reaches-the-scripts-as-the-clients-own-result",
        says: "A request's code from the friends service reaches the scripts as the client's own \
               result: not online, no such player, timed out, denied each as their own, and a code \
               the client does not know as the unknown result.",
        since: RETAIL,
        evidence: Evidence::Private("AC2-EVID-GUN-CODES"),
        station: "auberean-client-runtime::cpu::friends::a_tells_code_reaches_the_scripts_as_the_clients_own_result",
        tier: Tier::Cpu,
    },
    behaviour! {
        id: "friends.layer.answers-come-back-with-what-the-request-kept",
        says: "Requests go to the friends service with names as it writes them, and each answer \
               comes back with what its request kept: the friend added (its state told first), the \
               friends and ignore lists with this world's names shortened and the asker's word, a \
               tell's answer with the name and text as the player typed them.",
        since: RETAIL,
        evidence: Evidence::Private("AC2-EVID-GUN-RESULTS"),
        station: "auberean-client-runtime::cpu::friends::answers_come_back_with_what_the_request_kept",
        tier: Tier::Cpu,
    },
    behaviour! {
        id: "friends.layer.events-reach-the-scripts-only-once-the-ticket-is-taken",
        says: "The client connects to the friends service, presents the login's ticket and is told \
               it was taken. The service's events (the friends list as the character enters, a \
               friend's state, a tell) reach the scripts only once the ticket is taken.",
        since: RETAIL,
        evidence: Evidence::Private("AC2-EVID-GUN-STARTUP"),
        station: "auberean-client-runtime::cpu::friends::events_reach_the_scripts_only_once_the_ticket_is_taken",
        tier: Tier::Cpu,
    },
    behaviour! {
        id: "friends.layer.the-client-ending-finishes-with-its-ticket",
        says: "The client ending finishes with a ticket it presented while the service is up, \
               once; the layer ending closes the stream with a goodbye, dials no more and hears \
               nothing more.",
        since: RETAIL,
        evidence: Evidence::Private("AC2-EVID-FRIENDS-CLIENT-ENDING"),
        station: "auberean-client-runtime::cpu::friends::the_client_ending_finishes_with_its_ticket",
        tier: Tier::Cpu,
    },
    behaviour! {
        id: "friends.layer.the-own-worlds-name-is-added-and-taken-off-exactly",
        says: "The world's name from the login is added, after an at sign, to a name with none, \
               and taken off a name whose part from the at sign on is exactly an at sign and the \
               world's name (case and all); any other name is left as it is. The first name the \
               login gives stands.",
        since: RETAIL,
        evidence: Evidence::Private("AC2-EVID-GUN-NAMES"),
        station: "auberean-client-runtime::cpu::friends::the_own_worlds_name_is_added_and_taken_off_exactly",
        tier: Tier::Cpu,
    },
    behaviour! {
        id: "friends.library.block-header",
        says: "Every result and event the friends library hands the retail client is marked valid \
               in its fifth byte and gives its type in the third word; a result gives the client's \
               own context in the eighth word and its code in the ninth, then its fields.",
        since: RETAIL,
        evidence: Evidence::Private("AC2-EVID-FRIENDS-LIBRARY-BLOCK-HEADER"),
        station: "auberean-friends-dll::cpu::layout::every_block_is_marked_valid_and_carries_its_type_and_a_result_its_context_and_code",
        tier: Tier::Cpu,
    },
    behaviour! {
        id: "friends.library.down-requests-answered-later",
        says: "A request the retail client makes while the friends stream is down is accepted and \
               answered at the next pump with the not-connected code, never refused on the spot, \
               since the client drops a tell refused at once without a word; so is one in flight \
               when the stream goes.",
        since: THIS_CLIENT,
        divergence: "CD2-friends-stream",
        evidence: Evidence::Private("AC2-EVID-FRIENDS-LIBRARY-DOWN-REQUESTS"),
        station: "auberean-friends-dll::cpu::engine::a_request_made_while_down_is_answered_at_the_next_pump_with_the_not_connected_code",
        tier: Tier::Cpu,
    },
    behaviour! {
        id: "friends.library.extra-carried",
        says: "The extra data the retail client attaches to a tell is carried in the stream's 16 \
               bytes, with zeros after data that is shorter; longer data is left off the tell.",
        since: THIS_CLIENT,
        divergence: "CD2-friends-stream",
        evidence: Evidence::Private("AC2-EVID-FRIENDS-LIBRARY-EXTRA"),
        station: "auberean-friends-dll::cpu::engine::a_tell_s_extra_data_is_carried_in_sixteen_bytes_or_left_off",
        tier: Tier::Cpu,
    },
    behaviour! {
        id: "friends.library.first-connection-fails-once",
        says: "A login's first connection to the friends service that cannot be made, or that the \
               world does not welcome within 10 seconds, is one lost-connection event, and the \
               library stays down until the next login asks again.",
        since: THIS_CLIENT,
        divergence: "CD2-friends-stream",
        evidence: Evidence::Private("AC2-EVID-FRIENDS-LIBRARY-FIRST-FAILS"),
        station: "auberean-friends-dll::cpu::engine::a_first_connection_that_fails_is_one_lost_event_and_the_library_stays_down",
        tier: Tier::Cpu,
    },
    behaviour! {
        id: "friends.library.first-connection-limit",
        says: "A login's first connection with no answer at all is given up after 15 seconds with \
               one lost-connection event, since the retail client waits for its connect event with \
               no limit of its own.",
        since: THIS_CLIENT,
        divergence: "CD2-friends-stream",
        evidence: Evidence::Private("AC2-EVID-FRIENDS-LIBRARY-FIRST-LIMIT"),
        station: "auberean-friends-dll::cpu::engine::a_first_connection_with_no_answer_at_all_is_given_up_after_fifteen_seconds",
        tier: Tier::Cpu,
    },
    behaviour! {
        id: "friends.library.friend-state-and-words",
        says: "A friend's state reaches the retail client as a name and a state; the state flags' \
               answer as which flag and the flags; the ticket's answer as its status, each in the \
               words the client reads.",
        since: RETAIL,
        evidence: Evidence::Private("AC2-EVID-FRIENDS-LIBRARY-STATE-WORDS"),
        station: "auberean-friends-dll::cpu::layout::a_friend_s_state_the_flags_and_the_ticket_s_status_sit_where_the_client_reads_them",
        tier: Tier::Cpu,
    },
    behaviour! {
        id: "friends.library.friends-entries",
        says: "A friends list reaches the retail client as a count and 12-byte entries of name, \
               state and location, the names and location UTF-16 ending in a NUL, both in the \
               answer to the list and in the friends sent at the character's entry.",
        since: RETAIL,
        evidence: Evidence::Private("AC2-EVID-FRIENDS-LIBRARY-ENTRIES"),
        station: "auberean-friends-dll::cpu::layout::a_friends_list_is_a_count_and_twelve_byte_entries_of_name_state_and_location",
        tier: Tier::Cpu,
    },
    behaviour! {
        id: "friends.library.identifiers",
        says: "The friends library answers the class the retail client creates and the ten \
               interfaces it queries, in the order it queries them.",
        since: RETAIL,
        evidence: Evidence::Private("AC2-EVID-FRIENDS-LIBRARY-IDS"),
        station: "auberean-friends-dll::cpu::ids::the_class_and_the_ten_interfaces_are_the_identifiers_the_client_asks_for",
        tier: Tier::Cpu,
    },
    behaviour! {
        id: "friends.library.ignore-names",
        says: "The ignore list reaches the retail client as a count and an array of pointers to \
               names.",
        since: RETAIL,
        evidence: Evidence::Private("AC2-EVID-FRIENDS-LIBRARY-IGNORE-NAMES"),
        station: "auberean-friends-dll::cpu::layout::the_ignore_list_is_a_count_and_an_array_of_names",
        tier: Tier::Cpu,
    },
    behaviour! {
        id: "friends.library.lost-connection-redials",
        says: "A connection to the friends service that was up and is lost is one lost-connection \
               event; the library dials again after 1, 2, 5 and 10 seconds, then every 30, whatever \
               the retail client's own prompt connect asks, says nothing of the dials that fail, \
               and is connected again once the world welcomes it.",
        since: THIS_CLIENT,
        divergence: "CD2-friends-stream",
        evidence: Evidence::Private("AC2-EVID-FRIENDS-LIBRARY-LOST-REDIALS"),
        station: "auberean-friends-dll::cpu::engine::a_lost_connection_is_one_event_and_the_library_dials_again_on_its_own_waits",
        tier: Tier::Cpu,
    },
    behaviour! {
        id: "friends.library.one-result-per-context",
        says: "Every request the retail client makes gets exactly one result, handed over only \
               inside the client's pump, carrying the client's own context as it gave it.",
        since: RETAIL,
        evidence: Evidence::Private("AC2-EVID-FRIENDS-LIBRARY-ONE-RESULT"),
        station: "auberean-friends-dll::cpu::engine::each_request_is_answered_once_at_a_pump_with_the_client_s_own_context",
        tier: Tier::Cpu,
    },
    behaviour! {
        id: "friends.library.real-sockets",
        says: "The friends library dials, writes and reads on threads of its own, so that nothing \
               the retail client calls waits on the network, and trades with a world on loopback \
               through a cut and a reconnect.",
        since: THIS_CLIENT,
        divergence: "CD2-friends-stream",
        evidence: Evidence::Private("AC2-EVID-FRIENDS-LIBRARY-REAL-SOCKETS"),
        station: "auberean-friends-dll::cpu::host::the_library_on_real_sockets_connects_trades_and_comes_back_after_a_cut",
        tier: Tier::Cpu,
    },
    behaviour! {
        id: "friends.library.retail-loading",
        says: "The built friends library loads by path as the retail client loads it, gives its \
               class factory by name, makes one object that answers the ten interfaces, and keeps \
               the ticket the client hands it without touching the four guard bytes after it.",
        since: RETAIL,
        evidence: Evidence::Private("AC2-EVID-FRIENDS-LIBRARY-LOADING"),
        station: "auberean-friends-dll::cpu::library::the_built_library_loads_as_the_client_loads_it_and_keeps_its_ticket_inside_its_guard",
        tier: Tier::Cpu,
    },
    behaviour! {
        id: "friends.library.retail-session",
        says: "The built friends library serves the retail client's calls through the entries the \
               client calls (set up, connect, the pump, the ticket in two calls, the friends and \
               ignore lists, adding and removing, the state, a tell), hands back every result and \
               event where the client reads it, and takes the client's connect from inside the \
               pump after a lost connection.",
        since: RETAIL,
        evidence: Evidence::Private("AC2-EVID-FRIENDS-LIBRARY-SESSION"),
        station: "auberean-friends-dll::cpu::library::the_built_library_serves_the_client_s_calls_and_hands_back_blocks_the_client_reads",
        tier: Tier::Cpu,
    },
    behaviour! {
        id: "friends.library.second-login-redials",
        says: "A connect while the friends stream is up, a second login in one run, closes the \
               stream, answers what it owed, and dials again, with one connect event for the new \
               attempt.",
        since: THIS_CLIENT,
        divergence: "CD2-friends-stream",
        evidence: Evidence::Private("AC2-EVID-FRIENDS-LIBRARY-SECOND-LOGIN"),
        station: "auberean-friends-dll::cpu::engine::a_connect_while_connected_closes_the_stream_and_dials_again_with_one_event",
        tier: Tier::Cpu,
    },
    behaviour! {
        id: "friends.library.tell-extra-never-null",
        says: "A tell reaches the retail client as its teller, its text and a pointer to its extra \
               data that is never NULL: a size and the bytes, or zero and NULL for none.",
        since: RETAIL,
        evidence: Evidence::Private("AC2-EVID-FRIENDS-LIBRARY-TELL-EXTRA"),
        station: "auberean-friends-dll::cpu::layout::a_tell_carries_its_teller_its_text_and_extra_data_that_is_never_null",
        tier: Tier::Cpu,
    },
    behaviour! {
        id: "friends.library.ticket-kept",
        says: "The ticket the retail client is started with arrives as its text and a NUL; the \
               library hands it back without the NUL, and what the client presents reaches the \
               world as given.",
        since: RETAIL,
        evidence: Evidence::Private("AC2-EVID-FRIENDS-LIBRARY-TICKET"),
        station: "auberean-friends-dll::cpu::engine::the_ticket_is_kept_without_its_nul_and_what_the_client_presents_is_sent",
        tier: Tier::Cpu,
    },
    behaviour! {
        id: "friends.library.world-goodbye-disconnects",
        says: "A friends stream the world closes with a goodbye is one disconnected event, which \
               the retail client shows as the connection lost; the library dials again on its \
               waits.",
        since: THIS_CLIENT,
        divergence: "CD2-friends-stream",
        evidence: Evidence::Private("AC2-EVID-FRIENDS-LIBRARY-WORLD-BYE"),
        station: "auberean-friends-dll::cpu::engine::a_world_that_says_goodbye_is_a_disconnected_event_and_the_library_dials_again",
        tier: Tier::Cpu,
    },
    behaviour! {
        id: "friends.login.a-refused-ticket-ends-the-login",
        says: "A ticket the friends service refuses ends the login before the game login goes out, \
               as a refused password does; a banned account is told so.",
        since: RETAIL,
        evidence: Evidence::Private("AC2-EVID-GUN-LOGIN-REFUSED"),
        station: "auberean-client-runtime::cpu::friends::a_refused_ticket_ends_the_login",
        tier: Tier::Cpu,
    },
    behaviour! {
        id: "friends.login.the-game-login-waits-for-the-friends-service",
        says: "The game login waits for the friends service and goes out once the ticket is taken. \
               When the service cannot be reached, a login given its secret directly goes on at once \
               and the player plays without friends; a login without its secret ends there.",
        since: RETAIL,
        evidence: Evidence::Private("AC2-EVID-GUN-LOGIN-WAITS"),
        station: "auberean-client-runtime::cpu::friends::the_game_login_waits_for_the_friends_service",
        tier: Tier::Cpu,
    },
    behaviour! {
        id: "friends.rooms.a-line-to-a-channel-goes-to-the-chat-service-as-the-recorded-clients-sent-it",
        says: "A line the player's scripts send to a channel goes to the world's chat service on the \
               login connection byte for byte as the recorded clients sent theirs: the header, a \
               mark counted from 1, the room, the text and the speaker's meta letters; each \
               answer comes back to the line it answers (24 of 24 lines of six January 2017 \
               runs).",
        since: RETAIL,
        evidence: Evidence::Private("AC2-EVID-ROOMS-LINE-SENT"),
        station: "auberean-client-runtime::corpus::chat::rooms::a_line_to_a_channel_goes_to_the_chat_service_as_the_recorded_clients_sent_it",
        tier: Tier::Corpus,
    },
    behaviour! {
        id: "friends.rooms.a-rooms-line-heard-carries-the-speakers-meta-letters-and-flags",
        says: "A room's line heard names its speaker with the meta letters the speaker's client \
               attached, in brackets after the name, and carries that client's chat flags; a note \
               that does not read leaves the bare name and no flags. Nothing is heard before the \
               chat service's client started.",
        since: RETAIL,
        evidence: Evidence::Private("AC2-EVID-ROOMS-LINE-HEARD"),
        station: "auberean-client-runtime::cpu::friends::rooms::a_rooms_line_heard_carries_the_speakers_meta_letters_and_flags",
        tier: Tier::Cpu,
    },
    behaviour! {
        id: "friends.rooms.a-rooms-line-heard-is-written-with-its-channel-speaker-and-words",
        says: "Every room's line heard in the world is written by the player's scripts as one line: \
               the channel's name in brackets, the speaker's name without its meta letters, a \
               colon and the words (381 of 381 lines of six January 2017 runs).",
        since: RETAIL,
        evidence: Evidence::Private("AC2-EVID-ROOMS-LINE-WRITTEN"),
        station: "auberean-client-runtime::corpus::chat::rooms::a_rooms_line_heard_is_written_with_its_channel_speaker_and_words",
        tier: Tier::Corpus,
    },
    behaviour! {
        id: "friends.rooms.an-answer-comes-back-with-its-lines-room-and-text",
        says: "The chat service's answer to a line comes back to the player's scripts with the \
               line's room and text and the client's own result for its code; an answer to a mark \
               nobody waits for is dropped.",
        since: RETAIL,
        evidence: Evidence::Private("AC2-EVID-ROOMS-ANSWER"),
        station: "auberean-client-runtime::cpu::friends::rooms::an_answer_comes_back_with_its_lines_room_and_text",
        tier: Tier::Cpu,
    },
    behaviour! {
        id: "friends.rooms.an-empty-line-or-room-0-is-refused-at-once",
        says: "An empty line, or a line to room 0, is refused at once: nothing goes out and nothing \
               waits for an answer.",
        since: RETAIL,
        evidence: Evidence::Private("AC2-EVID-ROOMS-SEND-REFUSED"),
        station: "auberean-client-runtime::cpu::friends::rooms::an_empty_line_or_room_0_is_refused_at_once",
        tier: Tier::Cpu,
    },
    behaviour! {
        id: "friends.rooms.the-character-list-starts-the-chat-service-once-and-its-marks-run-on",
        says: "A character list that names a chat service sends rooms to it and starts its client \
               once for the program: the marks of the lines run on from 1 across later lists. \
               Without one nothing starts and a line is refused.",
        since: RETAIL,
        evidence: Evidence::Private("AC2-EVID-ROOMS-STARTUP"),
        station: "auberean-client-runtime::cpu::friends::rooms::the_character_list_starts_the_chat_service_once_and_its_marks_run_on",
        tier: Tier::Cpu,
    },
    behaviour! {
        id: "friends.scripts.a-friends-state-and-the-list-reach-the-friends-page",
        says: "A friend's new state and the friends list asked for reach the friends page through \
               the player's own scripts: the friend's state, and the list whole, names of this \
               world without the world's name.",
        since: RETAIL,
        evidence: Evidence::Private("AC2-EVID-GUN-SCRIPTS-FRIENDS"),
        station: "auberean-client-runtime::corpus::chat::friends::a_friends_state_and_the_list_reach_the_friends_page",
        tier: Tier::Corpus,
    },
    behaviour! {
        id: "friends.scripts.a-tell-heard-through-the-service-is-shown-with-its-text-as-sent",
        says: "A tell heard through the friends service is shown by the player's own scripts with \
               the teller's name and the text as sent, brackets and all.",
        since: RETAIL,
        evidence: Evidence::Private("AC2-EVID-GUN-TELL-HEARD"),
        station: "auberean-client-runtime::corpus::chat::friends::a_tell_heard_through_the_service_is_shown_with_its_text_as_sent",
        tier: Tier::Corpus,
    },
    behaviour! {
        id: "friends.scripts.a-tell-to-another-world-goes-through-the-service-and-its-answer-is-shown",
        says: "A tell to Name@World goes to the friends service with the name and text as typed, \
               and the service's answer is shown by the player's own scripts: the client's echo \
               when it went, the tellee not on, or the tellee unknown.",
        since: RETAIL,
        evidence: Evidence::Private("AC2-EVID-GUN-SCRIPTS-TELL"),
        station: "auberean-client-runtime::corpus::chat::friends::a_tell_to_another_world_goes_through_the_service_and_its_answer_is_shown",
        tier: Tier::Corpus,
    },
    behaviour! {
        id: "friends.server.bound-to-the-character",
        says: "Until the account's character enters the world every request is answered not \
               connected and no event is sent; entering sends the friends list, and leaving sends \
               nothing.",
        since: THIS_CLIENT,
        divergence: "CD2-friends-stream",
        evidence: Evidence::Private("AC2-EVID-FRIENDS-CHARACTER"),
        station: "auberean-friends::cpu::server::until_the_character_enters_every_request_is_answered_not_connected",
        tier: Tier::Cpu,
    },
    behaviour! {
        id: "friends.server.deadlines",
        says: "A world closes a stream that has not greeted it within 10 seconds of opening, or \
               presented a ticket within 30 seconds of its welcome, with the reason idle.",
        since: THIS_CLIENT,
        divergence: "CD2-friends-stream",
        evidence: Evidence::Private("AC2-EVID-FRIENDS-DEADLINES"),
        station: "auberean-friends::cpu::server::the_world_closes_a_stream_without_a_greeting_in_ten_seconds_or_a_ticket_in_thirty",
        tier: Tier::Cpu,
    },
    behaviour! {
        id: "friends.server.flood",
        says: "A world takes 20 elements from a stream at once and then 5 a second; a stream past \
               that is told flood and closed.",
        since: THIS_CLIENT,
        divergence: "CD2-friends-stream",
        evidence: Evidence::Private("AC2-EVID-FRIENDS-FLOOD"),
        station: "auberean-friends::cpu::server::past_twenty_at_once_then_five_a_second_the_stream_is_closed_for_flood",
        tier: Tier::Cpu,
    },
    behaviour! {
        id: "friends.server.greeting",
        says: "A world answers a greeting with its welcome (its network name, its name as its login \
               shows it, the keep-alive interval) and says protocol to another version or any other \
               first element.",
        since: THIS_CLIENT,
        divergence: "CD2-friends-stream",
        evidence: Evidence::Private("AC2-EVID-FRIENDS-SERVER-GREETING"),
        station: "auberean-friends::cpu::server::the_world_welcomes_a_greeting_and_refuses_another_version_or_another_first_element",
        tier: Tier::Cpu,
    },
    behaviour! {
        id: "friends.server.own-answers",
        says: "A world echoes the start-of-session state flags and answers help calls and room lines \
               not implemented, without its host.",
        since: THIS_CLIENT,
        divergence: "CD2-friends-stream",
        evidence: Evidence::Private("AC2-EVID-FRIENDS-OWN-ANSWERS"),
        station: "auberean-friends::cpu::server::flags_are_echoed_and_calls_and_rooms_are_not_implemented",
        tier: Tier::Cpu,
    },
    behaviour! {
        id: "friends.server.preamble",
        says: "A world closes a connection at its first byte that is not the preamble's, one that \
               begins a web socket upgrade, and one silent for 10 seconds.",
        since: THIS_CLIENT,
        divergence: "CD2-friends-stream",
        evidence: Evidence::Private("AC2-EVID-FRIENDS-PREAMBLE"),
        station: "auberean-friends::cpu::server::bytes_that_do_not_begin_with_the_preamble_are_closed_at_once",
        tier: Tier::Cpu,
    },
    behaviour! {
        id: "friends.server.replaced",
        says: "A stream replaced by a newer one for the same account is told replaced and closed, \
               and hears nothing more.",
        since: THIS_CLIENT,
        divergence: "CD2-friends-stream",
        evidence: Evidence::Private("AC2-EVID-FRIENDS-SERVER-REPLACED"),
        station: "auberean-friends::cpu::server::a_replaced_stream_is_told_so_and_closed",
        tier: Tier::Cpu,
    },
    behaviour! {
        id: "friends.server.ticket-order",
        says: "A request before the ticket is answered not ticketed and a second ticket already \
               ticketed; a refused ticket may be presented again.",
        since: THIS_CLIENT,
        divergence: "CD2-friends-stream",
        evidence: Evidence::Private("AC2-EVID-FRIENDS-TICKET-ORDER"),
        station: "auberean-friends::cpu::server::requests_before_the_ticket_and_a_second_ticket_are_answered_by_the_stream",
        tier: Tier::Cpu,
    },
    behaviour! {
        id: "friends.session.fresh-friends-list-after-reconnect",
        says: "After a lost stream the client dials again, presents its ticket again once welcomed, \
               and a world whose character is still in it sends the friends list again after \
               admitting it.",
        since: THIS_CLIENT,
        divergence: "CD2-friends-stream",
        evidence: Evidence::Private("AC2-EVID-FRIENDS-FRESH-LIST"),
        station: "auberean-friends::cpu::session::after_a_reconnect_the_client_presents_again_and_the_world_sends_the_friends_list_again",
        tier: Tier::Cpu,
    },
    behaviour! {
        id: "friends.session.greet-admit-and-trade",
        says: "A client and its world greet, admit the ticket and trade the friends list, a friend \
               added (its state before its answer), the state flags, and a tell each way, and a \
               quiet minute keeps the stream up.",
        since: THIS_CLIENT,
        divergence: "CD2-friends-stream",
        evidence: Evidence::Private("AC2-EVID-FRIENDS-SESSION"),
        station: "auberean-friends::cpu::session::a_client_and_its_world_greet_admit_the_ticket_and_trade_friends_and_tells",
        tier: Tier::Cpu,
    },
    behaviour! {
        id: "friends.stream.frames",
        says: "A stream is the preamble and then frames, each a 4-byte little-endian length of 1 to \
               65,536 and that many bytes of UTF-8; a frame of no bytes, a longer one or one not \
               UTF-8 is refused.",
        since: THIS_CLIENT,
        divergence: "CD2-friends-stream",
        evidence: Evidence::Private("AC2-EVID-FRIENDS-FRAMES"),
        station: "auberean-friends::cpu::frames::a_stream_is_the_preamble_then_frames_of_a_length_and_one_element",
        tier: Tier::Cpu,
    },
    behaviour! {
        id: "friends.stream.keep-alive",
        says: "Each side pings after the world's keep-alive interval of sending nothing and answers \
               each ping with its number.",
        since: THIS_CLIENT,
        divergence: "CD2-friends-stream",
        evidence: Evidence::Private("AC2-EVID-FRIENDS-KEEP-ALIVE"),
        station: "auberean-friends::cpu::client::the_client_pings_after_the_worlds_interval_of_quiet_and_answers_each_ping",
        tier: Tier::Cpu,
    },
    behaviour! {
        id: "friends.stream.limits",
        says: "The crate's timings and bounds are those of the specification's vectors: the frame, \
               the keep-alive, the silence, the deadlines, the waits, the bound on requests and the \
               lengths of a software name, a ticket and a tell.",
        since: THIS_CLIENT,
        divergence: "CD2-friends-stream",
        evidence: Evidence::Private("AC2-EVID-FRIENDS-LIMITS"),
        station: "auberean-friends::cpu::elements::the_crates_timings_and_bounds_are_the_specifications",
        tier: Tier::Cpu,
    },
    behaviour! {
        id: "friends.stream.protocol-errors-close",
        says: "A side that reads a bad frame or a malformed element says protocol and closes the \
               stream; an element it does not know it skips, and the stream goes on.",
        since: THIS_CLIENT,
        divergence: "CD2-friends-stream",
        evidence: Evidence::Private("AC2-EVID-FRIENDS-PROTOCOL-ERRORS"),
        station: "auberean-friends::cpu::client::a_world_that_breaks_the_protocol_is_told_so_and_left",
        tier: Tier::Cpu,
    },
    behaviour! {
        id: "friends.stream.silence",
        says: "Each side closes a stream it has heard nothing on for 70 seconds, with the reason \
               idle.",
        since: THIS_CLIENT,
        divergence: "CD2-friends-stream",
        evidence: Evidence::Private("AC2-EVID-FRIENDS-SILENCE"),
        station: "auberean-friends::cpu::client::the_client_gives_up_a_stream_it_has_heard_nothing_on_for_seventy_seconds",
        tier: Tier::Cpu,
    },
    behaviour! {
        id: "friends.ticket.a1-form",
        says: "A ticket is a1, a dot, the account and a dot and the secret, each UTF-8 in URL-safe \
               base64 without padding; anything else is no ticket, and a ticket's debug form hides \
               the secret.",
        since: THIS_CLIENT,
        divergence: "CD2-friends-stream",
        evidence: Evidence::Private("AC2-EVID-FRIENDS-TICKET"),
        station: "auberean-friends::cpu::ticket::a_ticket_is_a1_then_the_account_and_secret_in_url_safe_base64",
        tier: Tier::Cpu,
    },
];
