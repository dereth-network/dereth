//! Server messages no recorded session carries, and the fixture that drives them: a census that
//! the corpus still lacks all sixteen late receivers' opcodes; the three handlers that consume and
//! discard (`0x01CB`, `0x00C3`, `0xF630`); and the four portal-storm notices (`0x02C9`..`0x02CC`),
//! which light the lamp, differ as the client's own notices do, and queue a storm script only for
//! the two warnings. The enchantment, book-page, allegiance and chat-report receivers driven through
//! the same fixture are `magic::enchantment_receivers`, `panels::book_page_replies`,
//! `panels::allegiance_info_replies` and `chat::channel_and_age_replies`.
//! Fixture: the recorded `0x0013 Login_PlayerDescription` and `0x02C2 Magic_UpdateEnchantment` from
//! `fellowship-one-vassal` and the `0x00B4` book from `long-solo-play`, fed fragmented into a
//! socket-free replay endpoint (`App::attach_replay_network`); the retail dats; a headless App.

#![cfg(gpu)]

use crate::common::client_dir_required as client_dir;
use crate::common::gpu_lock;

use dereth_client::app::App;
use dereth_client_net::client_session::testing::{session_names, Corpus, Direction};
use dereth_client_runtime::config::Config;
use dereth_client_runtime::dropped;
use dereth_client_runtime::net::ClientNetwork;
use dereth_primitives::{LocalTime, ObjectId};
use dereth_protocol::objects::{ItemCreateObject, ObjectCreatePayload};
use dereth_protocol::{Message, Opcode};
use dereth_ui_screens::screens::gameplay::GamePlayScreen;
use dereth_ui_screens::view::GameView;

/// The player this session's blobs are re-addressed to.
pub(crate) const PLAYER: ObjectId = ObjectId(0x5000_0001);

// ---------------------------------------------------------------------------------------------
// The fixture: recorded blobs, unwrapped at +12
// ---------------------------------------------------------------------------------------------

/// Every server-to-client blob in `scenario` whose **game-event** opcode is `want`.
pub(crate) fn recorded(scenario: &str, want: Opcode) -> Vec<Vec<u8>> {
    Corpus::shared(scenario)
        .blobs
        .iter()
        .filter(|b| b.dir == Direction::ServerToClient)
        .filter(|b| {
            b.opcode == 0xF7B0
                && b.payload.len() >= 16
                && u32::from_le_bytes(b.payload[12..16].try_into().expect("four bytes")) == want.0
        })
        .map(|b| b.payload.clone())
        .collect()
}

/// **The server-to-client game-event subset of the negative census.** Every station below
/// encodes its own message; this test checks that no recording carries any opcode in `WANT`, with
/// three positive controls through the same reader.
///
/// If a corpus rebuild ever brings one of these in, this fails and the station that owns it
/// should be switched to the recorded blob — which is strictly better evidence.
#[test]
fn no_recording_carries_a_late_receivers_game_event() {
    const WANT: [Opcode; 15] = [
        Opcode::MAGIC_UPDATE_MULTIPLE_ENCHANTMENTS,
        Opcode::MAGIC_REMOVE_MULTIPLE_ENCHANTMENTS,
        Opcode::MAGIC_DISPEL_MULTIPLE_ENCHANTMENTS,
        Opcode::WRITING_BOOK_ADD_PAGE_RESPONSE,
        Opcode::WRITING_BOOK_DELETE_PAGE_RESPONSE,
        Opcode::ITEM_APPRAISE_DONE,
        Opcode::ITEM_GET_INSCRIPTION_RESPONSE,
        Opcode::ALLEGIANCE_ALLEGIANCE_INFO_RESPONSE_EVENT,
        Opcode::ALLEGIANCE_ALLEGIANCE_UPDATE_ABORTED,
        Opcode::COMMUNICATION_CHANNEL_LIST,
        Opcode::COMMUNICATION_CHANNEL_INDEX,
        Opcode::CHARACTER_QUERY_AGE_RESPONSE,
        Opcode::MISC_PORTAL_STORM_BREWING,
        Opcode::MISC_PORTAL_STORM_IMMINENT,
        Opcode::MISC_PORTAL_STORM_SUBSIDED,
    ];
    let mut found = Vec::new();
    for s in session_names() {
        for w in WANT {
            let n = recorded(s, w).len();
            if n > 0 {
                found.push(format!("{s} {:#06X} x{n}", w.0));
            }
        }
    }
    assert!(
        found.is_empty(),
        "the corpus grew one of these opcodes: {found:?}"
    );

    // The instrument must be able to look: the same reader finds three opcodes the corpus does carry.
    for control in [
        Opcode::HOUSE_HOUSE_STATUS,
        Opcode::LOGIN_PLAYER_DESCRIPTION,
        Opcode::MAGIC_UPDATE_ENCHANTMENT,
    ] {
        assert!(
            !recorded("fellowship-one-vassal", control).is_empty(),
            "an instrument that cannot look reports absence -- {:#06X} is the control",
            control.0
        );
    }
}

// ---------------------------------------------------------------------------------------------
// Harness: a socket-free peer
// ---------------------------------------------------------------------------------------------

pub(crate) struct Peer {
    crypto: dereth_transport::CryptoSystem,
    sequence: u32,
    blob: u32,
    /// The [`dereth_protocol::OrderedEventHeader`] stamp that queue 9 delivers in order of.
    stamp: u32,
}

impl Peer {
    pub(crate) fn new() -> (Self, ClientNetwork) {
        let mut net = ClientNetwork::new(
            "127.0.0.1:19100",
            7304,
            "late-receivers-station",
            "unused",
            0,
        )
        .expect("a net");
        net.session.transport.add_connection(
            0xB,
            0,
            1,
            0xDEAD_BEEF,
            0x1234_5678,
            Some("127.0.0.1:19100".parse().expect("addr")),
        );
        (
            Self {
                crypto: dereth_transport::CryptoSystem::new(0xDEAD_BEEF),
                sequence: 1,
                blob: 0,
                stamp: 0,
            },
            net,
        )
    }

    /// Replay one `0xF7B0` blob, re-addressed to this session.
    ///
    /// The recorded stamp values are the ones that session had reached, so replaying them
    /// verbatim into a session whose ordered counter starts at zero stalls every one of them, so
    /// each replay is re-stamped with the next counter value.
    pub(crate) fn replay(&mut self, app: &mut App, mut blob: Vec<u8>) {
        assert!(
            blob.len() >= 16,
            "a 0xF7B0 blob is at least a wire action-order header: {blob:02x?}"
        );
        assert_eq!(
            u32::from_le_bytes(blob[0..4].try_into().expect("four bytes")),
            0xF7B0,
            "this helper re-stamps ordered game events only"
        );
        self.stamp += 1;
        blob[4..8].copy_from_slice(&PLAYER.0.to_le_bytes());
        blob[8..12].copy_from_slice(&self.stamp.to_le_bytes());
        self.send(app, 9, blob);
    }

    /// Frame `m` as a game event and replay it — the production encoder, end to end.
    pub(crate) fn event<M: Message>(&mut self, app: &mut App, m: &M) {
        let blob = dereth_protocol::events::pack_event(PLAYER, 0, m).expect("a game event");
        self.replay(app, blob);
    }

    /// Feed one blob as ACE framed it — **fragmented**, one fragment per datagram.
    ///
    /// Fragment **data** is capped at **448** bytes; headers make a valid datagram larger than that.
    /// One oversized fragment (the recorded `0x0013` is 1,880 bytes) is rejected by the transport
    /// parser as [`dereth_client_net::RejectReason::NoConnection`], which reads like a torn-down
    /// session rather than an over-long fragment.
    pub(crate) fn send(&mut self, app: &mut App, queue: u16, bytes: Vec<u8>) {
        const MAX_FRAG_DATA: usize = 448;
        self.blob += 1;
        let chunks: Vec<&[u8]> = bytes.chunks(MAX_FRAG_DATA).collect();
        let num_frags = u16::try_from(chunks.len()).expect("a blob this file builds fits");
        for (i, chunk) in chunks.iter().enumerate() {
            self.sequence += 1;
            let mut packet = dereth_transport::OutPacket::new(dereth_transport::ProtoHeader {
                seq_id: self.sequence,
                rec_id: 0xB,
                interval: 0x100,
                iteration: 1,
                ..Default::default()
            });
            packet
                .add_fragment(dereth_transport::Fragment::new(
                    dereth_transport::FragmentHeader {
                        blob_id_low: self.blob,
                        blob_id_high: 0x8000_0000,
                        num_frags,
                        blob_frag_size: 0,
                        blob_num: u16::try_from(i).expect("fits"),
                        queue_id: queue,
                    },
                    (*chunk).to_vec(),
                ))
                .expect("one fragment");
            let raw = packet
                .serialize(Some(self.crypto.next()))
                .expect("a datagram");
            app.replay_network_mut()
                .expect("the replay endpoint")
                .session
                .transport
                .feed(&raw, None, LocalTime(0.0))
                .expect("fed");
        }
    }
}

pub(crate) fn setup(tag: &str) -> (App, Peer) {
    assert!(
        dereth_dat::testing::have_dats(),
        "the retail dats are this file's oracle: none at {} -- set DERETH_TEST_DAT_DIR",
        client_dir().display()
    );
    dropped::clear();
    let mut app = App::new(Config {
        ui: true,
        headless: true,
        sound: false,
        width: 800,
        height: 600,
        dat_dir: client_dir(),
        preferences_file: std::env::temp_dir()
            .join(format!("dereth-late-receivers-{tag}-not-created/prefs.ini")),
        ..Config::default()
    })
    .expect("an application");
    app.start_shell().expect("the shell starts");
    let s = dereth_client_runtime::scene::SceneConfig {
        landblock: app.config().landblock,
        land_radius: app.config().land_radius,
        scenery_radius: app.config().scenery_radius,
        ..dereth_client_runtime::scene::SceneConfig::default()
    };
    app.load_static_scene(s).expect("a static scene");
    app.queue_ui_mode(dereth_ui::framework::mode::GAME_PLAY);
    for _ in 0..4 {
        app.frame();
    }
    let (mut peer, net) = Peer::new();
    app.attach_replay_network(net)
        .expect("the replay endpoint attaches");

    let mut p = ObjectCreatePayload {
        id: PLAYER,
        ..Default::default()
    };
    p.physicsdesc.bitfield |= dereth_protocol::types::physicsdesc::flags::SETUP;
    p.physicsdesc.setup_id = Some(0x0200_0001);
    p.physicsdesc.timestamps.instance = 1;
    peer.send(
        &mut app,
        10,
        dereth_protocol::write_blob(&ItemCreateObject(p)).expect("blob"),
    );
    app.frame();
    app.probe_mut().objects_mut().world.player = Some(PLAYER);
    app.probe_mut()
        .objects_mut()
        .world
        .weenie_mut(PLAYER)
        .expect("the player was created")
        .pwd
        .name = "Larktest".to_string();

    settle(&mut app);
    (app, peer)
}

/// The shard's own `0x0013`, which is what gives the player a qualities registry and pushes the portal
/// dat's `SpellTable` into the game world's `magic` field.
pub(crate) fn describe_player(app: &mut App, peer: &mut Peer) {
    let blob = recorded("fellowship-one-vassal", Opcode::LOGIN_PLAYER_DESCRIPTION)
        .pop()
        .expect("every fellowship capture opens with one 0x0013");
    peer.replay(app, blob);
    settle(app);
    assert!(
        app.objects().world.player_qualities().is_some(),
        "the recorded 0x0013 must give the player a registry, or every enchantment station below \
         measures the guard rather than the arm"
    );
    assert!(
        app.objects().world.magic.spell_table.is_some(),
        "and the spell table, or the removal-announcement path takes its unknown-spell return"
    );
}

pub(crate) fn settle(app: &mut App) {
    for _ in 0..6 {
        app.frame();
    }
}

pub(crate) fn gameplay(app: &mut App) -> &mut GamePlayScreen {
    let shell = app.ui_mut().expect("shell");
    let screen = shell.flow.current_mut().expect("a screen is up");
    let any: &mut dyn std::any::Any = &mut **screen;
    any.downcast_mut::<GamePlayScreen>()
        .expect("gameplay screen")
}

/// Every line any chat window has taken, in arrival order.
pub(crate) fn chat(app: &mut App) -> Vec<String> {
    gameplay(app)
        .chat
        .iter()
        .flat_map(|c| c.log.iter().map(|(_, s)| s.clone()))
        .collect()
}

// ---------------------------------------------------------------------------------------------
// 0x01CB / 0x00C3 / 0xF630 — the three arms retail throws away
// ---------------------------------------------------------------------------------------------

/// **All three are received, all three change nothing, and that is what retail does.**
///
/// | opcode | retail | why it is a counter |
/// |---|---|---|
/// | `0x01CB Item_AppraiseDone` | calls an empty handler | it returns zero and does nothing — the same handler `0x01C9` reaches |
/// | `0x00C3 Item_GetInscriptionResponse` | handled in the dispatcher | three packed strings unpacked and discarded; no notice, no call |
/// | `0xF630 Character_SetPlayerVisualDesc` | decoded in the dispatcher, then sends a character-set notice | the notice receiver is empty |
///
/// The station is a **pair** of measurements, not one: it asserts each opcode is consumed (the
/// receiver exists) *and* that the four things a wrong arm would have moved did not move. Either
/// half alone is unfalsifiable — the first would pass for an arm that also wrote an inscription,
/// the second for no arm at all.
///
/// **Falsified by** making any of the three opcodes unreachable: `dropped::unreceived` goes true.
#[test]
fn the_three_dead_handlers_are_consumed_and_move_nothing() {
    let _gpu = gpu_lock();
    let (mut app, mut peer) = setup("stubs");
    describe_player(&mut app, &mut peer);

    // The state a wrong arm would have written into.
    let before = (
        app.objects().world.appraisal.delivered,
        app.objects().world.book.opening,
        app.objects().world.player_objdesc.clone(),
        chat(&mut app).len(),
    );

    peer.event(
        &mut app,
        &dereth_protocol::objects::ItemAppraiseDone { unknown: 0 },
    );
    peer.event(
        &mut app,
        &dereth_protocol::items::ItemGetInscriptionResponse {
            object: PLAYER,
            unknown: 0,
            inscription: "Property of Larktest".to_owned(),
            scribe_name: "Larktest".to_owned(),
            scribe_account: "acct0001".to_owned(),
        },
    );
    // `0xF630` is **not** a game event: it arrives on the UI queue as a bare message, so it goes
    // in through `write_blob` rather than `pack_event`.
    peer.send(
        &mut app,
        9,
        dereth_protocol::write_blob(&dereth_protocol::login::PlayerAppearanceMessage {
            text: "a visual description retail throws away".to_owned(),
        })
        .expect("blob"),
    );
    settle(&mut app);

    let s = &app.interaction().stats;
    assert_eq!(s.appraise_done, 1, "0x01CB consumed");
    assert_eq!(
        s.inscription_responses, 1,
        "0x00C3 consumed, body decoded as retail unpacks it"
    );
    assert_eq!(s.player_visual_descs, 1, "0xF630 consumed");

    let after = (
        app.objects().world.appraisal.delivered,
        app.objects().world.book.opening,
        app.objects().world.player_objdesc.clone(),
        chat(&mut app).len(),
    );
    assert_eq!(
        after, before,
        "no appraisal cached, no book opened, no ObjDesc written, no chat line: all three \
         handlers are no-ops"
    );

    assert!(!dropped::unreceived(Opcode::ITEM_APPRAISE_DONE));
    assert!(!dropped::unreceived(Opcode::ITEM_GET_INSCRIPTION_RESPONSE));
    assert!(!dropped::unreceived(
        Opcode::CHARACTER_SET_PLAYER_VISUAL_DESC
    ));
}

// ---------------------------------------------------------------------------------------------
// 0x02C9 .. 0x02CC — the four portal-storm notices
// ---------------------------------------------------------------------------------------------

/// The input an authored portal-storm indicator would read, taken from the **view**.
fn storm_level(app: &App) -> f32 {
    app.hud().view(app.objects()).portal_storm_level()
}

/// Behaviour: hud.portal-storm.the-four-notices-set-the-storm-level-and-only-the-warnings-play-the-storm
///
/// **The event-to-state projection works, and the three differences between the four handlers are
/// the observable.**
///
/// This does **not** assert a visible lamp. The shipped `0x21000005` layout has no
/// portal-storm indicator (`0x10000005`) instance, so this station stops at the view value and the
/// state decision. A custom layout carrying that element would be driven by
/// `GamePlayScreen::update_indicators`.
///
/// | opcode | notice line | chat line | `play_script` | level |
/// |---:|---|---|---|---|
/// | `0x02C9` brewing | yes, channel `0x1A` | no | `0x73`, `0.0` | the wire's `extent` |
/// | `0x02CA` imminent | yes, channel `0x1A` | no | `0x73`, `1.0` | the wire's `extent` |
/// | `0x02CB` struck | **no** | **yes**, channel 0 | no | 0 |
/// | `0x02CC` subsided | yes, channel `0x1A` | no | no | 0 |
///
/// **Falsified by** making any arm unreachable: the level stays `0.0` and the state decision stays
/// dark.
#[test]
fn a_portal_storm_lights_the_lamp_and_the_four_notices_differ_as_retail_does() {
    let _gpu = gpu_lock();
    let (mut app, mut peer) = setup("storm");

    assert_eq!(storm_level(&app), 0.0, "the projected state starts dark");
    assert_eq!(
        dereth_ui_screens::hud::indicators::portal_storm_state(storm_level(&app)),
        (dereth_ui_screens::hud::indicators::STATE_NOTHING, false),
        "state 0x0D and no message-3 subscription"
    );

    // **0x02C9** — a *fractional* extent, which an integer level could not express.
    let chat_before = chat(&mut app).len();
    peer.event(
        &mut app,
        &dereth_protocol::trade::MiscPortalStormBrewing { extent: 0.25 },
    );
    settle(&mut app);
    assert_eq!(app.interaction().stats.portal_storms_brewing, 1);
    assert_eq!(
        storm_level(&app),
        0.25,
        "the storm-level notice takes the wire's own float"
    );
    assert_eq!(
        dereth_ui_screens::hud::indicators::portal_storm_state(storm_level(&app)),
        (1, true),
        "level > 0.0 selects state 1 and subscribes to global message 3"
    );
    assert_eq!(
        chat(&mut app).len(),
        chat_before,
        "a warning uses display-string type 0x1A, which the main window filters out"
    );

    // **0x02CA** — the same shape with a different literal and a different script intensity.
    peer.event(
        &mut app,
        &dereth_protocol::trade::MiscPortalStormImminent { extent: 1.5 },
    );
    settle(&mut app);
    assert_eq!(app.interaction().stats.portal_storms_imminent, 1);
    assert_eq!(storm_level(&app), 1.5);
    assert_eq!(
        chat(&mut app).len(),
        chat_before,
        "still nothing in the log"
    );

    // **0x02CB** — the only one that writes to the chat log, and the only one with no level.
    peer.event(&mut app, &dereth_protocol::trade::MiscPortalStorm);
    settle(&mut app);
    assert_eq!(app.interaction().stats.portal_storms_struck, 1);
    assert_eq!(app.objects().world.portal_storms_struck, 1);
    assert_eq!(
        storm_level(&app),
        0.0,
        "a completed portal storm resets the level to 0"
    );
    let lines: Vec<String> = chat(&mut app)[chat_before..]
        .iter()
        .filter(|l| !l.trim().is_empty())
        .cloned()
        .collect();
    assert_eq!(
        lines,
        vec!["The Portal Storm has teleported you away from the crowded area!".to_owned()],
        "the completed storm appends this type-0 chat line and the display trims its framing newline"
    );

    // **0x02CC** — a notice line and a zero, and nothing in the log.
    let chat_before = chat(&mut app).len();
    peer.event(&mut app, &dereth_protocol::trade::MiscPortalStormSubsided);
    settle(&mut app);
    assert_eq!(app.interaction().stats.portal_storms_subsided, 1);
    assert_eq!(storm_level(&app), 0.0);
    assert_eq!(
        chat(&mut app).len(),
        chat_before,
        "subsided is a notice, not a chat line"
    );

    // The portal-storm-level notice is raised by all four handlers, unconditionally and last.
    assert_eq!(
        app.interaction().stats.portal_storm_levels,
        4,
        "one per handler, whatever else it did"
    );
    for op in [
        Opcode::MISC_PORTAL_STORM_BREWING,
        Opcode::MISC_PORTAL_STORM_IMMINENT,
        Opcode::MISC_PORTAL_STORM,
        Opcode::MISC_PORTAL_STORM_SUBSIDED,
    ] {
        assert!(
            !dropped::unreceived(op),
            "{:#06X} still has no receiver",
            op.0
        );
    }
}

/// Behaviour: hud.portal-storm.the-four-notices-set-the-storm-level-and-only-the-warnings-play-the-storm
///
/// **The two warnings ask the player's own body for the storm script, and the two intensities
/// differ.**
///
/// Brewing requests the portal-storm script `0x73` at intensity `0.0`; imminent requests the same
/// script at intensity `1.0`. Both target the player's physics body. The other two handlers make no
/// such request.
///
/// The request is queued by `dereth_client_model` and drained by `WorldScene` beside `ObjectStream`'s own
/// script events, because `dereth_client_model` has no scene; this station reads the queue directly, before
/// a frame drains it, so it measures the **producer** rather than the scene's answer.
///
/// **Falsified by** dropping either `pending_portal_storm_scripts.push`, or by giving both the
/// same intensity.
#[test]
fn the_two_warnings_queue_a_portal_storm_script_and_the_calm_ones_do_not() {
    let _gpu = gpu_lock();
    let (mut app, _peer) = setup("storm-script");

    // Driven through the model rather than the wire, because `App::frame` drains the queue on the
    // frame the message lands and the intensities would be gone before this could read them. The
    // *wire* half is the station above; this one is about which handler pushes what.
    let w = &mut app.probe_mut().objects_mut().world;
    w.portal_storm_brewing(0.5);
    w.portal_storm_imminent(0.5);
    w.portal_storm_struck();
    w.portal_storm_subsided();
    assert_eq!(
        w.take_portal_storm_scripts(),
        vec![
            dereth_client_model::portal_storm::BREWING_INTENSITY,
            dereth_client_model::portal_storm::IMMINENT_INTENSITY
        ],
        "two calls, 0.0 then 1.0, and neither of the calm handlers adds a third"
    );
    assert_eq!(
        dereth_client_model::portal_storm::PS_PORTAL_STORM,
        0x73,
        "the portal-storm script ID remains 0x73"
    );
    assert!(
        w.take_portal_storm_scripts().is_empty(),
        "the drain is a take"
    );
}
