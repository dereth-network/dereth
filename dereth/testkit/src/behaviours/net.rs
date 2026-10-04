//! The link -- the handshake, what the client does when the shard goes quiet, and what the
//! link-status panel tells the player about it.
//!
//! The connection, goodbye and packet-loss rows live here rather than under the shell.
//!
//! The one subject in this registry whose claims are about the connection rather than about
//! anything a player points at. They are still behaviour: a player whose shard has stopped
//! answering sees a dead link or a reconnect, and which one he sees is what these rows say.
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
        id: "data-request.missing-landblock.a-record-that-is-there-but-unreadable-is-not-asked-for",
        says: "A piece of the world that is present in the player's own data files but will not \
               decode is a different failure from one that is missing, and the client does not ask \
               the shard for it: the shard would only send the bytes that are already there.",
        since: RETAIL,
        evidence: Evidence::Private("AC-EVID-P4-1B2-UNDECODABLE"),
        station: "dereth-testkit::dat::net::scenario_a_record_that_is_present_but_unreadable_is_not_asked_for",
        tier: Tier::Dat,
    },
    behaviour! {
        id: "data-request.missing-landblock.a-refusal-lets-the-client-ask-for-it-again",
        says: "When the shard refuses a request for a missing piece of the world, the client stops \
               counting that request as outstanding, so the next time the player walks into that \
               ground it asks again. Without that a single refusal would silence the request for \
               the rest of the session.",
        since: RETAIL,
        evidence: Evidence::Private("AC-EVID-P4-1B2-RETRY"),
        station: "dereth-testkit::dat::net::scenario_a_refusal_lets_the_client_ask_again",
        tier: Tier::Dat,
    },
    behaviour! {
        id: "data-request.missing-landblock.is-asked-for-once-and-the-answer-builds-the-ground",
        says: "Ground the player's own data files do not carry is asked for from the shard exactly \
               once however many times the client walks into it, and the shard's answer is written \
               into the player's own files, after which that ground builds and can be stood on.",
        since: RETAIL,
        evidence: Evidence::Private("AC-EVID-P4-1B2"),
        station: "dereth-testkit::dat::net::scenario_a_missing_landblock_is_asked_for_once_and_then_builds",
        tier: Tier::Dat,
    },
    behaviour! {
        id: "data-request.missing-landblock.with-no-link-the-miss-is-kept-rather-than-asked-for",
        says: "A piece of the world found missing while the client has no shard to ask is \
               remembered rather than dropped, so the first connected moment afterwards asks for \
               it. Nothing is sent while there is nowhere to send it.",
        since: RETAIL,
        evidence: Evidence::Private("AC-EVID-P4-1B2-NO-LINK"),
        station: "dereth-testkit::dat::net::scenario_a_miss_with_no_link_is_kept",
        tier: Tier::Dat,
    },
    behaviour! {
        id: "link.connected.the-lamp-lights-when-the-handshake-finishes-and-not-when-the-shard-first-answers",
        says: "The link is reported as up only once an ordinary datagram from the shard has been \
               accepted: the shard's opening answer puts the client into the middle of the \
               handshake and no further. The edge is reported exactly once and is withdrawn \
               unconsumed if the link is torn down before anything reads it, and the lamp goes out \
               again when the link goes down.",
        since: RETAIL,
        evidence: Evidence::Private("AC-EVID-P1-13-EDGE"),
        station: "dereth-testkit::dat::net::scenario_the_link_lamp_lights_on_the_finished_handshake",
        tier: Tier::Dat,
    },
    behaviour! {
        id: "link.dropped.a-goodbye-is-answered-once-and-the-link-is-taken-down-on-the-next-pass",
        says: "A shard that says goodbye with no complaint is not an error: the link stays up \
               until the client has sent its own goodbye back, exactly one of them however many \
               times the shard repeats itself, and only the pass after that takes the link down \
               and tells the player the shard has gone.",
        since: RETAIL,
        evidence: Evidence::Private("AC-EVID-P1-13-GRACEFUL"),
        station: "dereth-testkit::cpu::net::scenario_a_goodbye_with_no_complaint_is_answered_once_and_then_the_link_goes_down",
        tier: Tier::Cpu,
    },
    behaviour! {
        id: "link.dropped.a-refusal-after-the-login-window-closes-is-a-lost-shard-and-not-a-login-failure",
        says: "A refusal that arrives while the client is still logging in is shown to the player \
               as the reason the login failed; the same refusal after the login is over logs the \
               client off and is shown as the shard having gone, because there is no login left \
               to fail. A refusal that arrives before the link is even up is still read.",
        since: RETAIL,
        evidence: Evidence::Private("AC-EVID-P1-13-LATE-ERROR"),
        station: "dereth-testkit::cpu::net::scenario_a_refusal_after_the_login_is_over_is_a_lost_shard",
        tier: Tier::Cpu,
    },
    behaviour! {
        id: "link.dropped.a-shard-that-says-why-keeps-that-reason-while-the-goodbye-completes",
        says: "When the shard drops the player for a reason it names -- a full world, say -- the \
               player is told that reason, and it stays the reason they are shown even after the \
               ordinary goodbye that follows has run its course and taken the link down.",
        since: RETAIL,
        evidence: Evidence::Private("AC-EVID-P1-13-REASON"),
        station: "dereth-testkit::cpu::net::scenario_a_named_reason_survives_the_goodbye_that_follows_it",
        tier: Tier::Cpu,
    },
    behaviour! {
        id: "link.dropped.a-tampered-datagram-changes-nothing-and-another-recipient-leaving-is-not-the-shard",
        says: "A datagram whose contents do not match what it claims is thrown away whole -- the \
               readable reason inside it is not acted on -- and the link is still up afterwards. \
               And a second party on the same link saying goodbye takes only that party away: the \
               player's own shard connection is untouched and nobody is told anything.",
        since: RETAIL,
        evidence: Evidence::Private("AC-EVID-P1-13-GUARDS"),
        station: "dereth-testkit::cpu::net::scenario_a_tampered_datagram_changes_nothing_and_another_party_leaving_is_not_the_shard",
        tier: Tier::Cpu,
    },
    behaviour! {
        id: "link.dropped.only-real-silence-takes-the-link-down-and-a-stalled-client-is-not-silence",
        says: "The link is dropped after more than the silence the client allows, strictly more, \
               and a client that was itself frozen for that long does not count its own stall as \
               the shard having gone quiet -- the next ordinary frame after it looks again and \
               drops the link only if the silence was real.",
        since: RETAIL,
        evidence: Evidence::Private("AC-EVID-P1-13-SILENCE"),
        station: "dereth-testkit::cpu::net::scenario_only_real_silence_takes_the_link_down",
        tier: Tier::Cpu,
    },
    behaviour! {
        id: "link.first-connection.a-refusal-before-the-link-is-up-is-an-error-box-and-then-the-client-exits",
        says: "When the shard refuses the first connection before it is up -- a client of the \
               wrong version, say -- the player is shown one modal error box, captioned Game \
               Error, whose text says the connection could not be established and quotes that \
               refusal's own sentence from the client's connection-error strings. When the box is \
               closed the client exits: nothing more is drawn and there is no login screen to go \
               back to. A refusal after the link has been up is not this.",
        since: RETAIL,
        evidence: Evidence::Private("AC-EVID-RR78-BOX"),
        station: "dereth-testkit::dat::net::scenario_a_wrong_version_refusal_is_an_error_box_and_then_the_client_exits",
        tier: Tier::Dat,
    },
    behaviour! {
        id: "link.first-connection.every-refusal-reads-its-own-sentence-from-the-connection-error-strings",
        says: "Each reason the first connection can fail for is shown with its own sentence from \
               the client's connection-error strings -- the full world, the account in use, the \
               wrong version, the timed-out shard -- and not with a code or a generic line.",
        since: RETAIL,
        evidence: Evidence::Private("AC-EVID-RR78-ROWS"),
        station: "dereth-testkit::dat::net::scenario_every_refusal_reads_its_own_sentence",
        tier: Tier::Dat,
    },
    behaviour! {
        id: "link.flow.each-closed-server-interval-is-reported-with-its-byte-count",
        says: "Each time a datagram from the shard opens a newer half-second interval of the \
               shard's, the client reports back how many bytes it received in the interval just \
               closed, headers included, and that interval's number; the report rides the next \
               packet the client sends. Every such report the recorded client sent is sent again, \
               from the shard's datagrams alone.",
        since: RETAIL,
        evidence: Evidence::Private("AC-EVID-FLOW-REPORT"),
        station: "dereth-client-net::cpu::net::flow_report::every_recorded_flow_report_is_reproduced_from_the_server_datagrams",
        tier: Tier::Cpu,
    },
    behaviour! {
        id: "link.goodbye.the-client-sends-one-disconnect-per-connection-and-then-goes-silent",
        says: "Logging off sends one goodbye datagram on each open connection, each addressed with \
               that connection's own id and iteration rather than the first connection's.",
        since: RETAIL,
        evidence: Evidence::Private("AC-EVID-O269-GOODBYE"),
        station: "dereth-client-net::cpu::net::disconnect::every_connection_gets_its_own_packet",
        tier: Tier::Cpu,
    },
    behaviour! {
        id: "link.handshake.answers-the-shard-on-the-next-port-and-echoes-its-cookie",
        says: "The shard answers a login by naming the port it will really talk on and a cookie \
               of its own. The client replies on that next port and sends the cookie back \
               unchanged, and only then does it consider itself connecting rather than \
               connected.",
        since: RETAIL,
        evidence: Evidence::Private("AC-EVID-P4-7B-HANDSHAKE"),
        station: "dereth-testkit::cpu::net::scenario_the_handshake_answers_on_the_next_port",
        tier: Tier::Cpu,
    },
    behaviour! {
        id: "link.header.the-recipient-is-the-net-id-the-server-assigned",
        says: "Every ordinary datagram the client sends carries, as its recipient, the connection \
               id the server assigned when the link was set up, and two otherwise identical \
               datagrams sent under different assigned ids differ only in that field and its \
               checksum.",
        since: RETAIL,
        evidence: Evidence::Private("AC-EVID-O471-HEADER"),
        station: "dereth-client-net::cpu::net::recipient_id::the_flow_queue_stamps_the_assigned_net_id_on_every_packet",
        tier: Tier::Cpu,
    },
    behaviour! {
        id: "link.keep-alive.the-recipient-is-always-zero",
        says: "The keep-alive the client sends every 110 seconds carries a recipient and an \
               iteration of zero whatever connection id the server assigned, goes to the server's \
               next port up, and is sent unsequenced.",
        since: RETAIL,
        evidence: Evidence::Private("AC-EVID-O471-KEEP-ALIVE"),
        station: "dereth-client-net::cpu::net::recipient_id::the_keep_alive_recipient_is_a_hard_zero_in_retail",
        tier: Tier::Cpu,
    },
    behaviour! {
        id: "link.packet-loss.a-burst-ages-out-of-a-window-far-longer-than-the-line-says",
        says: "A burst of loss shows up in the figure at once, is diluted as clean readings pile \
               up behind it, and finally leaves the figure altogether -- forty readings later, \
               which is a window of over a minute rather than the ten seconds the line beside it \
               claims. The wording is the original's and is left alone.",
        since: RETAIL,
        evidence: Evidence::Private("AC-EVID-P1-16-WINDOW"),
        station: "dereth-testkit::cpu::net::scenario_a_burst_of_loss_ages_out_of_the_window",
        tier: Tier::Cpu,
    },
    behaviour! {
        id: "link.packet-loss.a-client-that-has-heard-nothing-reads-as-total-loss-until-the-first-reading",
        says: "Before the first reading has been taken the packet-loss line reads as complete \
               loss rather than as none, which is what the client starts it at; a link that has \
               simply never been touched contributes nothing rather than that figure, so the two \
               are different states and the line says which one the player is in.",
        since: RETAIL,
        evidence: Evidence::Private("AC-EVID-P1-16-SENTINEL"),
        station: "dereth-testkit::dat::net::scenario_a_client_that_has_heard_nothing_reads_as_total_loss",
        tier: Tier::Dat,
    },
    behaviour! {
        id: "link.packet-loss.counts-what-the-client-sent-as-well-as-what-it-received",
        says: "The packet-loss figure is twice the datagrams the two ends asked to have sent \
               again, over everything that crossed the link in both directions -- so the same \
               number of lost datagrams reads as less loss on a busier link, which is what a \
               proportion means.",
        since: RETAIL,
        evidence: Evidence::Private("AC-EVID-P1-16-BOTH-WAYS"),
        station: "dereth-testkit::cpu::net::scenario_the_packet_loss_figure_counts_both_directions",
        tier: Tier::Cpu,
    },
    behaviour! {
        id: "link.packet-loss.the-line-always-carries-a-number-and-it-is-the-ping-that-can-be-unknown",
        says: "The packet-loss line on the link-status panel always shows a figure, to two \
               decimal places, whatever that figure is -- it is never shown as unknown. The line \
               above it, the round trip to the shard, is the one that genuinely can be unknown \
               and says so until the shard answers.",
        since: RETAIL,
        evidence: Evidence::Private("AC-EVID-P1-16"),
        station: "dereth-testkit::dat::net::scenario_the_packet_loss_line_always_carries_a_number",
        tier: Tier::Dat,
    },
    behaviour! {
        id: "link.packet-loss.what-the-panel-shows-is-the-links-own-figure-on-a-lossy-link-and-zero-on-a-clean-one",
        says: "The figure the panel shows a player is the link's own arithmetic and not a second \
               copy of it: a link really losing datagrams moves it off the starting figure to \
               something between none and all, and a clean link reads exactly none once the first \
               reading has landed.",
        since: RETAIL,
        evidence: Evidence::Private("AC-EVID-P1-16-WIRED"),
        station: "dereth-testkit::dat::net::scenario_the_panel_shows_the_links_own_packet_loss_figure",
        tier: Tier::Dat,
    },
    behaviour! {
        id: "link.reading.is-taken-every-two-seconds-and-the-counters-start-again-after-it",
        says: "The client takes a reading of the link every two seconds whether or not anything \
               arrived in between, counts it once and not twice, and starts its counters again \
               afterwards -- so a quiet stretch is a reading of nothing rather than no reading at \
               all, and it carries how long it has been since anything came.",
        since: RETAIL,
        evidence: Evidence::Private("AC-EVID-P1-13-HEARTBEAT"),
        station: "dereth-testkit::cpu::net::scenario_a_link_reading_is_taken_every_two_seconds",
        tier: Tier::Cpu,
    },
    behaviour! {
        id: "link.referral-cookie.outlives-the-handshake-that-carried-it",
        says: "The cookie the shard sent while the connection was being made is kept with the \
               connection itself, so it is still there once the link is up -- which is the only \
               moment it is ever needed. The copy the handshake used is thrown away at that \
               point.",
        since: RETAIL,
        evidence: Evidence::Private("AC-EVID-P4-7B"),
        station: "dereth-testkit::cpu::net::scenario_the_cookie_outlives_the_handshake",
        tier: Tier::Cpu,
    },
    behaviour! {
        id: "link.replay.every-recorded-client-datagram-is-reproduced-byte-for-byte",
        says: "Every datagram the recorded client sent in every recorded session is produced again \
               by this client in order and byte for byte, checksum included, even through a \
               heavily lossy session in which datagrams had to be sent again.",
        since: RETAIL,
        evidence: Evidence::Private("AC-EVID-REPLAY-REPLAY"),
        station: "dereth-client-net::cpu::net::replay::replay_every_recorded_capture",
        tier: Tier::Cpu,
    },
    behaviour! {
        id: "link.retransmit.a-lost-datagram-is-naked-and-resent",
        says: "When a datagram is lost on the way, the other end asks for that one sequence number \
               once, the client sends its saved copy again, and once everything has arrived nobody \
               asks again.",
        since: RETAIL,
        evidence: Evidence::Private("AC-EVID-O193-RETRANSMIT"),
        station: "dereth-client-net::cpu::net::retransmit::a_single_dropped_datagram_is_naked_and_resent",
        tier: Tier::Cpu,
    },
    behaviour! {
        id: "link.retransmit.a-reject-retransmit-ends-the-retry-and-keeps-the-key-stream",
        says: "A side that keeps asking for a lost datagram stops asking for good once it is told \
               the datagram cannot be sent again, instead of repeating the request until the link \
               times out.",
        since: RETAIL,
        evidence: Evidence::Private("AC-EVID-O193-RETRANSMIT-REJECT"),
        station: "dereth-client-net::cpu::net::retransmit::a_reject_retransmit_ends_the_peers_retry_loop",
        tier: Tier::Cpu,
    },
    behaviour! {
        id: "link.session-replay.every-recorded-client-blob-is-re-originated-byte-for-byte",
        says: "Every recorded session replays through the client with every shard message \
               delivered, and every message the recorded client sent is produced again by this \
               client byte for byte, including its action-order stamp; no recorded client message \
               is left that the client cannot produce.",
        since: RETAIL,
        evidence: Evidence::Private("AC-EVID-REPLAY-SESSION-REPLAY"),
        station: "dereth-client-net::cpu::net::corpus_conformance::replay_every_scenario",
        tier: Tier::Cpu,
    },
    behaviour! {
        id: "link.silent-shard.a-recorded-silence-ends-the-link-and-says-the-shard-died",
        says: "A shard that stops answering for longer than the client gives it ends the link, and \
               the player is told the shard died rather than left in a world that has quietly \
               stopped changing. The recordings this is read off are sessions where that really \
               happened, and the silence that ends them is the recording's own.",
        since: RETAIL,
        evidence: Evidence::Private("AC-EVID-P1-110-SILENCE"),
        station: "dereth-testkit::cpu::net::scenario_a_recorded_silence_ends_the_link",
        tier: Tier::Cpu,
    },
    behaviour! {
        id: "link.silent-shard.logs-in-again-once-with-the-cookie-it-was-given",
        says: "When a shard that handed out a cookie stops answering for long enough, the client \
               drops that connection, tells the player the shard died, and builds exactly one \
               fresh login for the same shard carrying the same cookie -- counted as one attempt, \
               and, on a client that has only ever talked to this one shard, never actually sent.",
        since: RETAIL,
        evidence: Evidence::Private("AC-EVID-P4-7B-SELF-REFERRAL"),
        station: "dereth-testkit::cpu::net::scenario_a_silent_shard_is_logged_into_again",
        tier: Tier::Cpu,
    },
    behaviour! {
        id: "link.silent-shard.with-no-cookie-is-an-ordinary-timeout",
        says: "A shard that handed out no cookie and then goes quiet is an ordinary lost \
               connection: the client queues no second login, keeps the connection in the state \
               the timeout left it in, and does not log itself off.",
        since: RETAIL,
        evidence: Evidence::Private("AC-EVID-P4-7B-NO-COOKIE"),
        station: "dereth-testkit::cpu::net::scenario_a_silent_shard_with_no_cookie_just_drops",
        tier: Tier::Cpu,
    },
    behaviour! {
        id: "link.time-sync.a-client-running-fast-accuses-itself-once-per-sync-until-it-is-back-in-the-window",
        says: "When the shard's time sync finds the client's clock more than 60 seconds out, the \
               client first only notes the time; if it is still out more than 60 seconds later it \
               reports itself to the shard as running a speed hack, repeats that on every later \
               sync, and one sync back in tolerance clears it so the next lapse starts over.",
        since: RETAIL,
        evidence: Evidence::Private("AC-EVID-P48-TIME-SYNC"),
        station: "dereth-client-net::cpu::net::time_sync_speed_check::the_accusation_repeats_until_a_sync_lands_inside_the_window",
        tier: Tier::Cpu,
    },
    behaviour! {
        id: "link.time-sync.the-periodic-sections-wait-for-the-next-packet-the-client-sends",
        says: "Every three seconds the client queues its time report and echo request, and they \
               wait, with any byte-count report, for the next packet it sends -- the two-second \
               acknowledgement or a data packet -- rather than going alone; when they go they \
               carry the time they are sent at.",
        since: RETAIL,
        evidence: Evidence::Private("AC-EVID-FLOW-PERIODIC-SECTIONS"),
        station: "dereth-transport::lib::flow::tests::the_periodic_sections_wait_for_an_acknowledgement_and_carry_the_send_time",
        tier: Tier::Cpu,
    },
    behaviour! {
        id: "link.world-view.an-all-zero-server-blob-is-refused-as-retail-refuses-it",
        says: "A world message from the shard that is nothing but zeros is refused and dropped as \
               a message nothing handles, without being held for any object, while a real position \
               update of the same size on the same channel is handled normally.",
        since: RETAIL,
        evidence: Evidence::Private("AC-EVID-P1190-WORLD-VIEW"),
        station: "dereth-client-net::cpu::net::all_zero_blob::the_world_view_dispatcher_refuses_it_exactly_as_retail_does",
        tier: Tier::Cpu,
    },
    behaviour! {
        id: "net.dat-patch.a-deletion-the-manifest-names-hides-that-record-alone",
        says: "A deletion the world's overlay manifest names hides that one record, where a \
               shard's purge of the cell file takes a whole landblock; the installed file keeps it.",
        since: THIS_CLIENT,
        divergence: "CD-031",
        evidence: Evidence::Private("AC-EVID-DAT-OVERLAY-TOMBSTONES"),
        station: "dereth-client::dat::net::dat_patch_apply::a_deletion_the_manifest_names_hides_that_record_alone",
        tier: Tier::Dat,
    },
    behaviour! {
        id: "net.dat-patch.a-downloaded-record-is-written-and-seen-after-reopen",
        says: "A data file the shard sends during the update exchange is written into that world's \
               overlay over the client's data files and is there, unchanged, when the files are \
               opened again afterwards with the overlay over them.",
        since: THIS_CLIENT,
        divergence: "CD-031",
        evidence: Evidence::Private("AC-EVID-P41B-DAT-PATCH"),
        station: "dereth-client::dat::net::dat_patch_apply::a_patched_record_survives_a_fresh_open",
        tier: Tier::Dat,
    },
    behaviour! {
        id: "net.dat-patch.a-malformed-or-older-record-writes-nothing",
        says: "A downloaded data file that is malformed in any of several ways is refused and the \
               client's data files are left exactly as they were, not one byte changed.",
        since: RETAIL,
        evidence: Evidence::Private("AC-EVID-P41B-DAT-PATCH-MALFORMED"),
        station: "dereth-client::dat::net::dat_patch_apply::a_malformed_record_is_refused_and_not_one_byte_is_written",
        tier: Tier::Dat,
    },
    behaviour! {
        id: "net.dat-patch.a-record-the-manifest-does-not-name-is-refused",
        says: "With the world's overlay manifest in hand, a downloaded record whose bytes are not \
               the ones the manifest names is refused and not written, and the one it names is.",
        since: THIS_CLIENT,
        divergence: "CD-031",
        evidence: Evidence::Private("AC-EVID-DAT-OVERLAY-HASHES"),
        station: "dereth-client::dat::net::dat_patch_apply::a_record_whose_bytes_are_not_the_manifests_is_refused",
        tier: Tier::Dat,
    },
    behaviour! {
        id: "net.dat-patch.an-overlay-the-client-cannot-take-is-refused-and-nothing-is-written",
        says: "A world's overlay is refused and reported, and nothing of it is written, when the \
               world's manifest names another base than the data file the client holds, when \
               the world is on the player's overlay blocklist, or when the overlay folder \
               already holds another world's overlay; the update then ends at once.",
        since: THIS_CLIENT,
        divergence: "CD-031",
        evidence: Evidence::Private("AC-EVID-DAT-OVERLAY-REFUSALS"),
        station: "dereth-client::dat::net::dat_patch_apply::an_overlay_made_against_another_base_or_for_a_blocked_or_other_world_is_refused",
        tier: Tier::Dat,
    },
    behaviour! {
        id: "net.dat-patch.the-iteration-list-counts-as-delivered",
        says: "When the shard sends its copy of a data file's version list during an update, the \
               client counts it as delivered so the update can finish, but keeps its own list plus \
               the new version rather than writing the shard's copy.",
        since: RETAIL,
        evidence: Evidence::Private("AC-EVID-P41B-DAT-PATCH-ITERATION"),
        station: "dereth-client::dat::net::dat_patch_apply::the_servers_iteration_list_counts_as_delivered_and_is_not_written",
        tier: Tier::Dat,
    },
    behaviour! {
        id: "net.dat-patch.the-owners-client-directory-is-never-written",
        says: "Downloaded data never goes into the installed data files: it is written to the \
               world's overlay folder, the installed files are byte for byte as they were, the \
               data folder is never taken as an overlay folder, and a client with no overlay \
               folder writes nothing and says so.",
        since: THIS_CLIENT,
        divergence: "CD-031",
        evidence: Evidence::Private("AC-EVID-P41B-DAT-PATCH-OWNERS"),
        station: "dereth-client::dat::net::dat_patch_apply::a_patch_never_writes_the_installed_files",
        tier: Tier::Dat,
    },
    behaviour! {
        id: "net.game-actions.every-retail-action-is-reproduced-byte-for-byte",
        says: "Every game action the recorded retail clients sent that this client also sends -- \
               uses, inventory moves, combat mode changes, targeted melee attacks, wields and the \
               rest -- comes out byte for byte the same, on the same ordered queue, differing only \
               in the running action number.",
        since: RETAIL,
        evidence: Evidence::Private("AC-EVID-INTERACTION-GAME-ACTIONS"),
        station: "dereth-client::gpu::net::game_action_wire::every_retail_game_action_is_reproduced_byte_for_byte",
        tier: Tier::Gpu,
    },
    behaviour! {
        id: "objects.ordered-replies.every-recorded-reply-reaches-a-consumer",
        says: "Every kind of ordered reply this client has a receiver for is carried by the recordings and is \
               handed to that receiver, and between them the receivers take the great majority of the ordered \
               traffic the recorded shards sent rather than a handful of rare messages.",
        since: RETAIL,
        evidence: Evidence::Private("AC-EVID-O135"),
        station: "dereth-testkit::cpu::objects::scenario_every_recorded_reply_reaches_a_consumer",
        tier: Tier::Cpu,
    },
];
