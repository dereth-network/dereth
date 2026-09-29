//! Login, and the client that has no device at all.
//!
//! What a recorded login delivers, and the claim the `Presentation` seam was cut for: a whole
//! client with no device and no window.
//!
//! One file per subject, so that two changes adding rows at the same time do not edit the same
//! file. [`ROWS`] is in id order; the registry's own test asserts that, and that no id and no
//! evidence handle is repeated anywhere in it.

// `behaviour!` is `#[macro_export]`ed by `mod.rs` above this module's declaration, so it is in
// textual scope here and needs no import.
use super::{Behaviour, Evidence, Tier, RETAIL, TOOLING};

/// This subject's rows, in id order.
pub static ROWS: &[Behaviour] = &[
    behaviour! {
        id: "headless.a-whole-client-with-no-device-or-window",
        says: "The client is a complete running client with no graphics device and no window: it \
               runs every per-frame job in order, hands its draw and present work to whatever it \
               is displaying through once per frame, and ticks on a simulated clock whose time \
               after n frames is exactly n steps.",
        since: RETAIL,
        evidence: Evidence::Private("AC-EVID-R2-1"),
        station: "dereth-testkit::dat::login::scenario_a_whole_client_with_no_device_or_window",
        tier: Tier::Dat,
    },
    behaviour! {
        id: "login.chargen.two-olthoi-heritages-reuse-preview-ids",
        says: "In the shipped character-creation data the two Olthoi heritages share one \
               background scene and each uses one body for both sexes, while their bodies and \
               preview animations differ, so the creation preview must change with the heritage's \
               animations and not only with the body and scene.",
        since: RETAIL,
        evidence: Evidence::Private("AC-EVID-O622-CHARGEN"),
        station: "dereth-client::dat::login::chargen_preview_key::the_two_olthoi_heritages_reuse_ids_across_the_columns_the_preview_key_reads",
        tier: Tier::Dat,
    },
    behaviour! {
        id: "login.connect.credentials-decide-whether-the-client-connects",
        says: "The client means to connect by default and does so whenever it is started with an \
               account and a host; with neither it runs offline, told not to connect it stays \
               offline, and given an account without a host it stops with an error rather than \
               quietly running offline.",
        since: TOOLING,
        evidence: Evidence::Private("AC-EVID-CONNECT-CONNECT"),
        station: "dereth-client::gpu::login::connect_and_patch_screens::credentials_are_what_decides_whether_the_client_connects",
        tier: Tier::Gpu,
    },
    behaviour! {
        id: "login.data-download.the-shards-interrogation-is-surfaced-and-left-unanswered",
        says: "The shard's data-download interrogation reaches the client as an event rather than being \
               swallowed by the transport, and this client does not answer it -- which costs it nothing, \
               because the shard lets a character into the world on the handshake alone.",
        since: RETAIL,
        evidence: Evidence::Private("AC-EVID-O9-DDD"),
        station: "dereth-testkit::cpu::login::scenario_the_data_download_interrogation_is_surfaced_and_unanswered",
        tier: Tier::Cpu,
    },
    behaviour! {
        id: "login.disconnect.a-boot-keeps-the-process-up-until-the-player-asks",
        says: "A boot puts up the disconnect screen and the client keeps running behind it for as \
               long as the player leaves it there; pressing its button goes on to the closing \
               screen rather than quitting on the spot.",
        since: RETAIL,
        evidence: Evidence::Private("AC-EVID-O100-DISCONNECT-BOOT"),
        station: "dereth-client::gpu::login::booted_and_banned::the_process_stays_up_on_a_boot_and_ends_only_when_the_player_asks",
        tier: Tier::Gpu,
    },
    behaviour! {
        id: "login.disconnect.a-boot-shows-the-servers-own-reason",
        says: "When the shard boots the account the disconnect screen puts the shard's own reason \
               inside the client's sentence -- You have been booted from Asheron's Call because \
               the password entered for this account was not correct. -- rather than the Code of \
               Conduct default.",
        since: RETAIL,
        evidence: Evidence::Private("AC-EVID-O100-DISCONNECT"),
        station: "dereth-client::gpu::login::booted_and_banned::the_corpus_0xf7dc_reaches_the_screen_with_the_servers_own_reason",
        tier: Tier::Gpu,
    },
    behaviour! {
        id: "login.disconnect.ok-reaches-the-epilogue-and-ends-the-loop",
        says: "Pressing OK on the disconnection screen moves to the closing screen, which logs off \
               and ends the client within that same frame.",
        since: RETAIL,
        evidence: Evidence::Private("AC-EVID-SHUTDOWN-DISCONNECT"),
        station: "dereth-client::gpu::presentation::shutdown::the_ok_button_reaches_the_epilogue_which_logs_off_and_ends_the_main_loop",
        tier: Tier::Gpu,
    },
    behaviour! {
        id: "login.disconnect.shows-the-servers-reason-and-the-process-stays-until-quit",
        says: "A disconnect puts up the disconnect screen and the client keeps running behind it; \
               only the player's press on its button moves on to the closing screen, and the \
               client ends on the frame after the closing screen says it is done.",
        since: RETAIL,
        evidence: Evidence::Private("AC-EVID-O71-DISCONNECT"),
        station: "dereth-client::gpu::login::disconnect_and_leave_game::the_process_stays_up_on_a_disconnect_and_ends_only_when_the_player_asks",
        tier: Tier::Gpu,
    },
    behaviour! {
        id: "login.enter-world.reaches-the-hud-from-the-wizard-as-well-as-from-character-select",
        says: "A character who has just been created walks into the world exactly as one picked from the list \
               does: the wizard's last page gives way to the heads-up display the moment the world is \
               entered, and it stays put until then. No other screen answers that notice.",
        since: RETAIL,
        evidence: Evidence::Private("AC-EVID-O267"),
        station: "dereth-testkit::dat::login::scenario_entering_the_world_reaches_the_hud_from_the_wizard",
        tier: Tier::Dat,
    },
    behaviour! {
        id: "login.enter-world.the-description-and-not-the-ready-message-puts-you-in-the-world",
        says: "Replaying a recorded login, the shard's word that the world is ready does not make the client \
               playable; the player's own description does. Before that the client sits at character select \
               and sends its enter-world request and its character pick on the login queue.",
        since: RETAIL,
        evidence: Evidence::Private("AC-EVID-O9-IN-WORLD"),
        station: "dereth-testkit::cpu::login::scenario_the_description_is_what_puts_you_in_the_world",
        tier: Tier::Cpu,
    },
    behaviour! {
        id: "login.exit-world.the-teardown-drops-the-world-connection-and-keeps-the-login-server",
        says: "Leaving the world closes the connection to the world server and keeps the one to \
               the login server, which becomes the server the client talks to again, and the \
               client is marked as no longer in the game.",
        since: RETAIL,
        evidence: Evidence::Private("AC-EVID-O298-EXIT-WORLD"),
        station: "dereth-client-net::cpu::login::exit_world_teardown::the_teardown_drops_the_world_and_keeps_the_login_server",
        tier: Tier::Cpu,
    },
    behaviour! {
        id: "login.handshake.is-the-retail-clients-own-bytes-on-the-shards-two-ports",
        says: "Given a recorded shard's half of a login, this client answers with the datagrams the retail \
               client answered with, byte for byte and checksum included: the login request to the port it \
               was offered and the connect response to the next port up, and its first message on the wire is \
               numbered one rather than zero.",
        since: RETAIL,
        evidence: Evidence::Private("AC-EVID-O9-HANDSHAKE"),
        station: "dereth-testkit::cpu::login::scenario_the_handshake_is_the_retail_clients_own",
        tier: Tier::Cpu,
    },
    behaviour! {
        id: "login.logoff.a-log-off-in-mid-air-is-refused-out-loud",
        says: "Pressing log off in mid-jump is refused with Cannot log off while in mid-air. on \
               the notice strip, no log-off request is made, and landing afterwards does not \
               revive it: the player has to press again.",
        since: RETAIL,
        evidence: Evidence::Private("AC-EVID-P1-LOGOFF"),
        station: "dereth-client::gpu::login::airborne_logoff_refusal::a_log_off_pressed_in_mid_air_is_refused_out_loud_and_landing_does_not_revive_it",
        tier: Tier::Gpu,
    },
    behaviour! {
        id: "login.patch.the-data-patch-screen-shows-the-documented-states",
        says: "The first screen reads Connecting... until the link is up and Connected! after, and \
               its patch line walks through the shipped sentences -- Looking for data to patch..., \
               a percentage of so many kilobytes complete, Patching Done! -- and it moves on to \
               the intro only once both are done and the character list has arrived.",
        since: RETAIL,
        evidence: Evidence::Private("AC-EVID-CONNECT-PATCH"),
        station: "dereth-client::gpu::login::connect_and_patch_screens::the_data_patch_screen_shows_the_documented_states",
        tier: Tier::Gpu,
    },
    behaviour! {
        id: "login.player-identity.two-bodies-cannot-share-one-id",
        says: "Two bodies registered under one object id leave the first of them out of the physics sweep \
               with nothing said, which is why the client refuses to move a body onto an id another body \
               already holds and counts the refusal instead; and with the whole of a recording made solid \
               around it, the player's own body still answers to the id the shard gave it and is still swept.",
        since: RETAIL,
        evidence: Evidence::Private("AC-EVID-PLAYER-ID-COLLISION"),
        station: "dereth-testkit::dat::login::scenario_two_bodies_cannot_share_one_id",
        tier: Tier::Dat,
    },
    behaviour! {
        id: "login.replay.character-set-is-the-recordings-own",
        says: "Given exactly the datagrams a shard sent a retail client, this client reaches the \
               character-select screen and offers the characters that recording carried -- the \
               same account, the same names, in the recorded order.",
        since: RETAIL,
        evidence: Evidence::Private("AC-EVID-O9"),
        station: "dereth-testkit::cpu::login::scenario_login_offers_the_recordings_own_characters",
        tier: Tier::Cpu,
    },
    behaviour! {
        id: "login.replay.every-blob-the-shard-sends-is-ephemeral-and-unordered",
        says: "Every message a recorded shard sends is marked as one the client may handle immediately and \
               none of them carries an ordering type, which is the fact that decides how the client's own \
               queue must route them -- route on the wrong one and the session stops at the character list.",
        since: RETAIL,
        evidence: Evidence::Private("AC-EVID-O9-EPHEMERAL"),
        station: "dereth-testkit::cpu::login::scenario_every_recorded_blob_is_ephemeral_and_unordered",
        tier: Tier::Cpu,
    },
    behaviour! {
        id: "login.replay.every-recorded-session-is-consumed-whole-and-stays-connected",
        says: "Every login in every recording this project holds runs to the end of the recording through the \
               client's own transport: the link comes up and stays up unless the recording itself goes \
               silent, the only datagrams refused are the ones the recording itself delivered twice, every \
               message whose pieces are all on the wire is put back together, and none is left waiting.",
        since: RETAIL,
        evidence: Evidence::Private("AC-EVID-P1-110"),
        station: "dereth-testkit::cpu::login::scenario_every_recorded_session_is_consumed_whole",
        tier: Tier::Cpu,
    },
    behaviour! {
        id: "login.second-entry.a-second-entry-from-the-same-session-succeeds-with-fresh-state",
        says: "A player can enter the world, log off back to character select and enter again as a \
               different character without restarting: the second entry sends the same two-step \
               request as the first, addressed to the character picked the second time, and the \
               log-off itself puts nothing more on the wire.",
        since: RETAIL,
        evidence: Evidence::Private("AC-EVID-O280-SECOND-ENTRY"),
        station: "dereth-client-net::cpu::login::second_entry::a_second_entry_to_the_world_from_the_same_session_succeeds",
        tier: Tier::Cpu,
    },
    behaviour! {
        id: "login.second-login.the-world-is-reset-on-entry-and-nothing-carries-over",
        says: "Logging off leaves the world standing behind character select, and entering the \
               world again wipes it first: a second login stands the body in the landblock and at \
               the place the shard names, not where the first session left it.",
        since: RETAIL,
        evidence: Evidence::Private("AC-EVID-O939-SECOND-LOGIN"),
        station: "dereth-client::gpu::login::second_login::a_second_login_stands_the_body_where_the_server_says_and_not_where_the_first_one_left_it",
        tier: Tier::Gpu,
    },
    behaviour! {
        id: "logout.departure.a-player-killer-stays-until-the-fade-deadline",
        says: "A player killer who logs off sends the log-off request at once but stays standing \
               in the world twenty seconds longer than anyone else: the world does not begin to \
               fade until twenty-three seconds after the answer, and then it fades once.",
        since: RETAIL,
        evidence: Evidence::Private("AC-EVID-F60-DEPARTURE-PLAYER"),
        station: "dereth-client::gpu::login::logout_departure::a_player_killer_stays_in_the_world_until_the_native_twenty_three_second_fade_deadline",
        tier: Tier::Gpu,
    },
    behaviour! {
        id: "logout.departure.the-logout-emote-plays-before-the-screen-changes",
        says: "Answering yes to leaving keeps the player in the world while the departure plays: \
               the body claps its hands over its head, three seconds after the request the world \
               fades into the portal tunnel, and the character list comes up only when the shard \
               sends it, about six seconds in, never on the client's own say.",
        since: RETAIL,
        evidence: Evidence::Private("AC-EVID-F60-DEPARTURE"),
        station: "dereth-client::gpu::login::logout_departure::the_departure_plays_for_six_seconds_before_the_screen_changes",
        tier: Tier::Gpu,
    },
    behaviour! {
        id: "logout.notices.the-logoff-notice-raises-the-confirmation-and-arms-the-quit",
        says: "Asking to log off raises the log-off confirmation, in its own words rather than the \
               end-session ones, and sets the client to quit the game rather than go back to \
               character select if the player agrees; asking again while it is up raises no second \
               box.",
        since: RETAIL,
        evidence: Evidence::Private("AC-EVID-P37-NOTICES"),
        station: "dereth-ui-screens::dat::login::logoff_notices::the_logoff_notice_raises_the_logoff_confirmation_and_arms_the_quit",
        tier: Tier::Dat,
    },
    behaviour! {
        id: "session.end.the-world-the-character-left-is-torn-down",
        says: "When a character's session ends the client throws the whole world away: every object it was \
               holding is released, so a recording read after its log-off shows an empty world however much \
               traffic it carried while it was live.",
        since: RETAIL,
        evidence: Evidence::Private("AC-EVID-O135-TEARDOWN"),
        station: "dereth-testkit::cpu::objects::scenario_the_world_is_torn_down_when_the_character_leaves",
        tier: Tier::Cpu,
    },
];
