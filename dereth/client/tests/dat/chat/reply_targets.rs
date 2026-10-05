//! The chat reply aliases address the person they name. A tell from a player (sender id in the
//! player range, addressed to this player, not from this player) remembers its sender as the last
//! teller; a monarch (0x4000) or patron (0x2000) channel broadcast remembers its speaker; no other
//! tell or channel does. Typing an alias and a space in the chat entry expands it to
//! `@tell <name>, ` with the caret after the space, and the three reply keys address three
//! different people. The dangerous failure is a tell sent to the wrong player, so every gate has a
//! negative control: an NPC's tell, a tell to somebody else, and a tell to oneself do not arm.
//!
//! | gate | what it stops | oracle |
//! |---|---|---|
//! | sender id in 0x50000001..0x6FFFFFFF inclusive | replying to an NPC | every recorded tell in the seven selected recordings has a sender outside that range |
//! | target id equals the local player id | replying to an overheard tell | a captured body with its target field replaced |
//! | sender id differs from target id | a self-tell arming the reply | a captured body addressed to its own sender |
//!
//! Fixture: `Communication_HearDirectSpeech 0x02BD` tells decoded from seven named recordings
//! through the transport, `ObjectStream` and the HUD (player-like cases rewrite the sender or
//! target field of a captured blob); monarch and patron broadcasts are synthetic encodings. The
//! applications are headless with no shard connection, and every fixture path is an `expect`.

use crate::common::sim_app::{app_in_gameplay, gameplay};

use dereth_client::app::App;

use dereth_client_net::client_session::testing::capture::{shared_session, Datagram};
use dereth_client_net::client_session::SessionEvent;
use dereth_client_net::recording::connection_sequence_number;
use dereth_client_runtime::net::ClientNetwork;
use dereth_client_runtime::objects::ObjectStream;
use dereth_primitives::{LocalTime, ObjectId};
use dereth_protocol::comms::{CommunicationChannelBroadcastRecv, CommunicationHearDirectSpeech};
use dereth_protocol::{Message as _, Opcode};
use {dereth_desktop::pump::Pump, dereth_input::win32::Win32Message};

use dereth_ui::{ElemHandle, ElementId};
use dereth_ui_screens::chat::window::ENTRY;

use winit::event::MouseButton;

// ---------------------------------------------------------------------------------------------
// The capture corpus
// ---------------------------------------------------------------------------------------------

/// The seven named recordings this file's claims are measured over. Later recordings (the
/// fellowship sessions) add overheard and player-to-player traffic and are not part of the
/// population this file asserts over.
const SEVEN_SESSIONS: [&str; 7] = [
    "first-login-walk-jump",
    "early-inventory-and-casting",
    "short-second-connection",
    "login-account-booted",
    "ddd-interrogation-only",
    "long-solo-play",
    "short-play-with-training",
];

/// One recorded session through the real transport, session and object stream.
fn replay(records: &[Datagram]) -> Vec<SessionEvent> {
    let mut net = ClientNetwork::new(
        "127.0.0.1:19000",
        7304,
        "ac01",
        "pass",
        connection_sequence_number(records).unwrap_or(0),
    )
    .expect("host");
    let mut objects = ObjectStream::new();
    let mut events = Vec::new();
    let mut entered = false;
    for r in records {
        let now = LocalTime(r.t);
        if !r.c2s {
            net.feed(&r.raw, r.peer(), now);
        }
        net.tick(now);
        let _ = net.take_outgoing();
        for e in objects.pump(&mut net, now) {
            if let SessionEvent::CharacterSet(set) = &e {
                if !entered {
                    if let Some(c) = set.characters.first() {
                        let account = set.account.clone();
                        net.enter_world(c.gid, &account);
                        entered = true;
                    }
                }
            }
            events.push(e);
        }
    }
    events
}

/// Every 0x02BD blob decoded from the seven selected recordings, with its source filename.
/// The leading opcode dword `SessionEvent::UiEvent` requires is kept.
fn corpus_tells() -> Vec<(String, Vec<u8>)> {
    // In file-name order, the order the recordings sit in on disk.
    let mut names = SEVEN_SESSIONS;
    names.sort_unstable();
    let mut out = Vec::new();
    for name in names {
        for e in replay(shared_session(name)) {
            if let SessionEvent::UiEvent { opcode, blob } = e {
                if opcode == Opcode::COMMUNICATION_HEAR_DIRECT_SPEECH {
                    out.push((name.to_owned(), blob));
                }
            }
        }
    }
    assert!(
        !out.is_empty(),
        "the seven recordings carry incoming tells; none decoded"
    );
    out
}

fn decode(blob: &[u8]) -> CommunicationHearDirectSpeech {
    let mut r = dereth_protocol::archive::Reader::new(blob.get(4..).unwrap_or_default());
    CommunicationHearDirectSpeech::read(&mut r).expect("a recorded 0x02BD decodes")
}

/// The final four u32 fields are sender id, target id, text type and secret flags. Replacing
/// the sender's four-byte field in place preserves every other byte of each selected blob.
const SENDER_ID_FROM_END: usize = 16;

fn with_sender_id(blob: &[u8], id: ObjectId) -> Vec<u8> {
    let mut out = blob.to_vec();
    let at = out.len() - SENDER_ID_FROM_END;
    out[at..at + 4].copy_from_slice(&id.0.to_le_bytes());
    out
}

fn with_target_id(blob: &[u8], id: ObjectId) -> Vec<u8> {
    let mut out = blob.to_vec();
    let at = out.len() - SENDER_ID_FROM_END + 4;
    out[at..at + 4].copy_from_slice(&id.0.to_le_bytes());
    out
}

/// Deliver SessionEvent::PlayerCreated to the HUD and the world owning its ChatState.
/// This supplies the local player id used by note_last_teller's target comparison.
fn hud_for(player: ObjectId) -> (dereth_client_shell::hud::Hud, ObjectStream) {
    let mut hud = dereth_client_shell::hud::Hud::default();
    let mut objects = ObjectStream::new();
    let _ = hud.apply_events(&[SessionEvent::PlayerCreated(player)], &mut objects.world);
    assert_eq!(hud.player, Some(player), "the player id reached the HUD");
    // A reply target also needs a player body: direct speech is ignored while there is none,
    // even when the player id is known. `Hud::sync` sets `Hud::player_body` from the optional
    // `ViewerFrame`; this helper sets it to true directly to isolate the recipient-name logic, so
    // it does not exercise the live geometry producer.
    hud.player_body = true;
    (hud, objects)
}

fn feed(
    hud: &mut dereth_client_shell::hud::Hud,
    objects: &mut ObjectStream,
    opcode: Opcode,
    blob: Vec<u8>,
) {
    let _ = hud.apply_events(
        &[SessionEvent::UiEvent { opcode, blob }],
        &mut objects.world,
    );
}

/// Construct player-created followed by the supplied captured/modified tell and two synthetic
/// channel broadcasts. This is the event order consumed by the HUD, not a replay of a recorded
/// monarch/patron conversation.
fn session_events(player: ObjectId, tell: &[u8]) -> Vec<SessionEvent> {
    let broadcast = |channel: u32, who: &str| SessionEvent::UiEvent {
        opcode: Opcode::COMMUNICATION_CHANNEL_BROADCAST,
        blob: ui_blob(&CommunicationChannelBroadcastRecv {
            channel,
            sender_name: who.to_owned(),
            message: "orders".to_owned(),
        }),
    };
    vec![
        SessionEvent::PlayerCreated(player),
        SessionEvent::UiEvent {
            opcode: Opcode::COMMUNICATION_HEAR_DIRECT_SPEECH,
            blob: tell.to_vec(),
        },
        broadcast(dereth_client_runtime::hud::channel::MONARCH, "Aldis"),
        broadcast(dereth_client_runtime::hud::channel::PATRON, "Plonk"),
    ]
}

fn ui_blob<M: dereth_protocol::Message>(m: &M) -> Vec<u8> {
    let mut w = dereth_protocol::archive::Writer::new();
    w.u32(M::OPCODE.0);
    m.write(&mut w).expect("the message encodes");
    w.into_inner()
}

// ---------------------------------------------------------------------------------------------
// 1. The misdirection guard, against the corpus
// ---------------------------------------------------------------------------------------------

/// Every recorded NPC tell in the seven selected recordings declines to become a reply target.
/// Their target ids are player-range ids; their sender ids are not. Retargeting each message to
/// the same local player isolates the sender-range guard from the target guard.
///
/// Both counters matter: as many declines as tells and zero writes prove the handler ran,
/// whereas an unreachable handler would satisfy zero writes alone. Widening
/// `Hud::CLICKABLE_PLAYER_IDS` to every id makes every tell a write and leaves an NPC remembered.
#[test]
fn no_recorded_npc_tell_arms_the_reply_key() {
    let tells = corpus_tells();
    let mut senders: std::collections::BTreeSet<String> = std::collections::BTreeSet::new();
    let mut addressed_to_a_player = 0usize;

    // The player id is taken from the tells themselves -- these really are addressed to us -- so
    // the *only* gate under test here is the sender's id range.
    let player = ObjectId(decode(&tells[0].1).target_id.0);
    let (mut hud, mut objects) = hud_for(player);
    let mut targets_seen: std::collections::BTreeSet<u32> = std::collections::BTreeSet::new();

    for (_, blob) in &tells {
        let m = decode(blob);
        senders.insert(m.sender_name.clone());
        targets_seen.insert(m.target_id.0);
        if (0x5000_0001..0x7000_0000).contains(&m.target_id.0) {
            addressed_to_a_player += 1;
        }
        // Re-target every tell at *this* player so the target gate cannot be what declines them.
        feed(
            &mut hud,
            &mut objects,
            Opcode::COMMUNICATION_HEAR_DIRECT_SPEECH,
            with_target_id(blob, player),
        );
    }

    eprintln!(
        "{} tells from {} distinct speakers, {addressed_to_a_player} addressed to a \
         player object; targets {targets_seen:?}",
        tells.len(),
        senders.len()
    );
    assert_eq!(
        addressed_to_a_player,
        tells.len(),
        "every recorded tell is addressed to a player character"
    );
    assert_eq!(
        hud.stats.last_teller_declined,
        tells.len() as u64,
        "the arm ran on every recorded tell"
    );
    assert_eq!(hud.stats.last_teller_writes, 0, "and stored none of them");
    assert_eq!(objects.world.chat.last_teller, None);
    assert_eq!(
        objects.world.chat.last_teller_name, "",
        "no NPC became the reply target"
    );
}

// ---------------------------------------------------------------------------------------------
// 2. A player's tell does arm it — off a captured blob, four bytes changed
// ---------------------------------------------------------------------------------------------

/// Behaviour: chat.tell.a-reply-goes-to-whoever-last-wrote-to-you-and-nowhere-when-nobody-has
///
/// A captured NPC tell with a player-range sender id populates both remembered fields. The
/// first Jonathan message in the named corpus is used; `early-inventory-and-casting` and
/// `short-play-with-training` both carry it, so no particular session is required. Only the
/// sender field is overwritten: at most four bytes differ, and the decoded message, name and
/// target are checked separately.
///
/// Removing the name write or the `note_last_teller` call from the 0x02BD consumer fails these
/// assertions. The positive is a transformed captured blob, not an observed player-to-player
/// tell.
#[test]
fn a_players_tell_addressed_to_us_becomes_the_reply_target() {
    let tells = corpus_tells();
    let (from, recorded) = tells
        .iter()
        .find(|(_, b)| decode(b).sender_name == "Jonathan")
        .expect(
            "early-inventory-and-casting and short-play-with-training both carry Jonathan's tells",
        );
    let original = decode(recorded);
    let me = original.target_id;

    // A player character id, inside the clickable range the client tests for.
    let alba = ObjectId(0x5000_0007);
    let blob = with_sender_id(recorded, alba);
    let differing = recorded.iter().zip(&blob).filter(|(a, b)| a != b).count();
    assert!(
        differing <= 4,
        "only the four bytes of senderID may differ from the recorded blob; {differing} do"
    );
    let sent = decode(&blob);
    assert_eq!(sent.message, original.message, "the shard's own words");
    assert_eq!(sent.sender_name, "Jonathan");
    assert_eq!(sent.target_id, me);

    let (mut hud, mut objects) = hud_for(me);
    feed(
        &mut hud,
        &mut objects,
        Opcode::COMMUNICATION_HEAR_DIRECT_SPEECH,
        blob,
    );

    assert_eq!(hud.stats.last_teller_writes, 1, "one tell, one write");
    assert_eq!(hud.stats.last_teller_declined, 0);
    assert_eq!(
        objects.world.chat.last_teller,
        Some(alba),
        "the sender id is remembered"
    );
    assert_eq!(
        objects.world.chat.last_teller_name, "Jonathan",
        "the sender name is remembered"
    );
    eprintln!(
        "{from}: a captured tell with senderID -> {:#010X} arms `@r`",
        alba.0
    );

    // And it is visible through the seam the screen reads, not only in the field.
    let t = hud.reply_targets(&objects.world);
    assert_eq!(t.last_teller.as_deref(), Some("Jonathan"));
    assert_eq!(t.monarch, None, "no monarch has spoken");
    assert_eq!(t.patron, None, "and no patron");
}

/// The other two gates, each on its own, each with a captured blob behind it.
///
/// A player-range sender addressed to someone else does not redirect our reply. A self-tell
/// declines too: self is inside the player range, so the range guard alone cannot stop it; the
/// self branch prints "You think" without recording a reply target. Each case uses a fresh
/// HUD and world and checks that the decline arm ran; removing either gate fails.
#[test]
fn a_tell_we_only_overheard_and_a_tell_to_ourselves_both_store_nothing() {
    let tells = corpus_tells();
    let recorded = &tells[0].1;
    let me = ObjectId(0x5000_0007);
    let someone_else = ObjectId(0x5000_0008);
    let a_player = ObjectId(0x5000_0009);

    // (a) addressed to someone else.
    let (mut hud, mut objects) = hud_for(me);
    let blob = with_target_id(&with_sender_id(recorded, a_player), someone_else);
    feed(
        &mut hud,
        &mut objects,
        Opcode::COMMUNICATION_HEAR_DIRECT_SPEECH,
        blob,
    );
    assert_eq!(hud.stats.last_teller_declined, 1, "the arm ran");
    assert_eq!(
        objects.world.chat.last_teller_name, "",
        "a tell to someone else is not ours"
    );

    // (b) a tell to ourselves.
    let (mut hud, mut objects) = hud_for(me);
    let blob = with_target_id(&with_sender_id(recorded, me), me);
    feed(
        &mut hud,
        &mut objects,
        Opcode::COMMUNICATION_HEAR_DIRECT_SPEECH,
        blob,
    );
    assert_eq!(hud.stats.last_teller_declined, 1, "the arm ran");
    assert_eq!(
        objects.world.chat.last_teller, None,
        "`You think, \"...\"` stores nothing"
    );
}

/// Session cleanup must discard remembered names before another character can reuse them.
/// This case arms last-teller and monarch; patron starts empty, so it does not independently
/// demonstrate clearing an armed patron name.
#[test]
fn logging_off_forgets_all_three_names() {
    let tells = corpus_tells();
    let me = ObjectId(0x5000_0007);
    let (mut hud, mut objects) = hud_for(me);
    feed(
        &mut hud,
        &mut objects,
        Opcode::COMMUNICATION_HEAR_DIRECT_SPEECH,
        with_target_id(&with_sender_id(&tells[0].1, ObjectId(0x5000_0009)), me),
    );
    feed(
        &mut hud,
        &mut objects,
        Opcode::COMMUNICATION_CHANNEL_BROADCAST,
        ui_blob(&CommunicationChannelBroadcastRecv {
            channel: dereth_client_runtime::hud::channel::MONARCH,
            sender_name: "Aldis".to_owned(),
            message: "all hail".to_owned(),
        }),
    );
    let before = hud.reply_targets(&objects.world);
    assert!(
        before.last_teller.is_some() && before.monarch.is_some(),
        "both are armed: {before:?}"
    );

    let _ = hud.apply_events(&[SessionEvent::LoggedOff], &mut objects.world);
    // The HUD's LoggedOff clears its channel names. `ObjectStream::reset` normally replaces the
    // world; that assignment is made directly here to clear the world's last-teller pair, so this
    // exercises the two state owners, not the full network-driven reset path.
    objects.world = dereth_client_model::World::new();
    let after = hud.reply_targets(&objects.world);
    assert_eq!(
        after,
        dereth_ui_screens::chat::window::ReplyTargets::default(),
        "all three cleared"
    );
}

// ---------------------------------------------------------------------------------------------
// 3. The monarch and the patron
// ---------------------------------------------------------------------------------------------

/// Only monarch 0x4000 and patron 0x2000 broadcasts remember their speaker. Fellowship 0x800,
/// vassals 0x1000, co-vassals 0x1000000 and allegiance 0x2000000 do not; there is no vassal
/// reply alias. The channel handler has exactly those two writes.
///
/// The messages are synthetic `dereth_protocol` encodings: the recordings carry no
/// `Communication_ChannelBroadcast 0x0147`, so this file claims no recorded-channel oracle.
/// Swapping the monarch and patron arms of `note_at_channel_speaker` is the targeted negative.
#[test]
fn only_the_monarch_and_patron_channels_remember_their_speaker() {
    let me = ObjectId(0x5000_0007);
    let (mut hud, mut objects) = hud_for(me);
    let say = |channel: u32, who: &str| CommunicationChannelBroadcastRecv {
        channel,
        sender_name: who.to_owned(),
        message: "orders".to_owned(),
    };
    let send = |hud: &mut dereth_client_shell::hud::Hud,
                objects: &mut ObjectStream,
                m: CommunicationChannelBroadcastRecv| {
        feed(
            hud,
            objects,
            Opcode::COMMUNICATION_CHANNEL_BROADCAST,
            ui_blob(&m),
        );
    };

    // The four channels that store nothing, first, so a later write cannot mask a wrong one.
    for (channel, who) in [
        (0x800u32, "Fella"),
        (0x1000, "Vassal"),
        (0x100_0000, "CoVassal"),
        (0x200_0000, "Ally"),
    ] {
        send(&mut hud, &mut objects, say(channel, who));
    }
    assert_eq!(
        hud.stats.at_channel_name_writes, 0,
        "four channels, no name remembered"
    );
    assert_eq!(hud.reply_targets(&objects.world), Default::default());

    send(
        &mut hud,
        &mut objects,
        say(dereth_client_runtime::hud::channel::MONARCH, "Aldis"),
    );
    send(
        &mut hud,
        &mut objects,
        say(dereth_client_runtime::hud::channel::PATRON, "Plonk"),
    );
    assert_eq!(hud.stats.at_channel_name_writes, 2);
    let t = hud.reply_targets(&objects.world);
    assert_eq!(t.monarch.as_deref(), Some("Aldis"), "0x4000 is the monarch");
    assert_eq!(t.patron.as_deref(), Some("Plonk"), "0x2000 is the patron");
    assert_eq!(t.last_teller, None, "and a broadcast is not a tell");

    // An empty sender denotes the local broadcast echo and follows the "You say to your"
    // display branch. It must neither add a write nor erase the remembered monarch.
    send(
        &mut hud,
        &mut objects,
        say(dereth_client_runtime::hud::channel::MONARCH, ""),
    );
    assert_eq!(
        hud.stats.at_channel_name_writes, 2,
        "our own line stores nothing"
    );
    assert_eq!(
        hud.reply_targets(&objects.world).monarch.as_deref(),
        Some("Aldis"),
        "and does not clear what was there"
    );
}

// ---------------------------------------------------------------------------------------------
// 4. End to end, in the live element tree
// ---------------------------------------------------------------------------------------------

fn find(app: &App, id: ElementId) -> ElemHandle {
    let shell = app.ui().expect("shell");
    let root = shell.flow.current().expect("a screen").roots()[0];
    shell
        .ui
        .get_child_recursive(root, id)
        .unwrap_or_else(|| panic!("{id:?} is in the shipped layout"))
}

fn centre(app: &App, h: ElemHandle) -> (i32, i32) {
    let b = app.ui().expect("shell").ui.screen_box(h);
    ((b.x0 + b.x1) / 2, (b.y0 + b.y1) / 2)
}

fn entry_text(app: &mut App) -> String {
    let entry = find(app, ENTRY);
    let (ui, _) = gameplay(app);
    ui.text_element_mut(entry)
        .map(|t| t.glyphs.inq_text(false))
        .unwrap_or_default()
}

fn set_entry(app: &mut App, s: &str) {
    let entry = find(app, ENTRY);
    let (ui, _) = gameplay(app);
    if let Some(t) = ui.text_element_mut(entry) {
        t.set_text(s);
        t.cursor = t.glyphs.len();
    }
}

/// The three reply names, delivered the way a session delivers them: one `0x02BD` tell
/// addressed to the local player and two `0x0147` broadcasts on the monarch and patron channels,
/// sent as session UI events to the application's HUD. These are synthetic protocol messages,
/// not packets from a live shard. The next `Hud::drive` copies the communication state's names
/// onto the screen's reply targets.
fn remember_three_speakers(app: &mut App) {
    use dereth_client_net::client_session::SessionEvent;
    use dereth_primitives::ObjectId;
    use dereth_protocol::comms::{
        CommunicationChannelBroadcastRecv, CommunicationHearDirectSpeech,
    };
    use dereth_protocol::{Message as _, Opcode};

    let me = ObjectId(0x5000_0007);
    let blob = |op: Opcode, body: &dyn Fn(&mut dereth_protocol::archive::Writer)| -> Vec<u8> {
        let mut w = dereth_protocol::archive::Writer::new();
        w.u32(op.0);
        body(&mut w);
        w.into_inner()
    };
    let tell = CommunicationHearDirectSpeech {
        message: "well met".to_owned(),
        sender_name: "Alba".to_owned(),
        sender_id: ObjectId(0x5000_0009),
        target_id: me,
        text_type: dereth_client_model::chat::text_type::SPEECH_DIRECT,
        secret_flags: 0,
    };
    let say = |channel: u32, who: &str| CommunicationChannelBroadcastRecv {
        channel,
        sender_name: who.to_owned(),
        message: "orders".to_owned(),
    };
    let events = vec![
        SessionEvent::PlayerCreated(me),
        SessionEvent::UiEvent {
            opcode: Opcode::COMMUNICATION_HEAR_DIRECT_SPEECH,
            blob: blob(Opcode::COMMUNICATION_HEAR_DIRECT_SPEECH, &|w| {
                tell.write(w).expect("encodes");
            }),
        },
        SessionEvent::UiEvent {
            opcode: Opcode::COMMUNICATION_CHANNEL_BROADCAST,
            blob: blob(Opcode::COMMUNICATION_CHANNEL_BROADCAST, &|w| {
                say(dereth_client_runtime::hud::channel::MONARCH, "Aldis")
                    .write(w)
                    .expect("encodes");
            }),
        },
        SessionEvent::UiEvent {
            opcode: Opcode::COMMUNICATION_CHANNEL_BROADCAST,
            blob: blob(Opcode::COMMUNICATION_CHANNEL_BROADCAST, &|w| {
                say(dereth_client_runtime::hud::channel::PATRON, "Plonk")
                    .write(w)
                    .expect("encodes");
            }),
        },
    ];
    app.apply_hud_events(&events);
    app.frame();
    let (_, screen) = gameplay(app);
    assert_eq!(
        screen.reply_targets.last_teller.as_deref(),
        Some("Alba"),
        "the tell reached `GamePlayScreen::reply_targets` through `Hud::drive`"
    );
    assert_eq!(screen.reply_targets.monarch.as_deref(), Some("Aldis"));
    assert_eq!(screen.reply_targets.patron.as_deref(), Some("Plonk"));
}

/// Keyboard and pointer messages through `Pump` and the input shell.
/// No operating-system window procedure is involved.
struct Hand {
    pump: Pump,
    time_ms: u32,
}

impl Hand {
    fn new() -> Self {
        let mut pump = Pump::new();
        pump.state.is_ready = true;
        pump.state.is_active_app = true;
        Self {
            pump,
            time_ms: 300_000,
        }
    }

    fn send(&mut self, app: &mut App, m: Win32Message) {
        self.pump.dispatch(m);
        if let Some(input) = app.input_manager_mut() {
            input.on_message(m);
        }
    }

    fn click(&mut self, app: &mut App, at: (i32, i32)) {
        self.time_ms += 10;
        let m = self
            .pump
            .mouse_move_message(f64::from(at.0), f64::from(at.1), self.time_ms);
        self.send(app, m);
        for pressed in [true, false] {
            self.time_ms += 10;
            let m = self
                .pump
                .mouse_button_message(MouseButton::Left, pressed, self.time_ms)
                .expect("the left button is in the 0x201 block");
            self.send(app, m);
        }
        app.frame();
    }

    fn type_text(&mut self, app: &mut App, s: &str) {
        for b in s.bytes() {
            self.time_ms += 10;
            let m = Win32Message::new(
                dereth_input::win32::msg::WM_CHAR,
                b as usize,
                0,
                self.time_ms,
            );
            self.send(app, m);
            app.frame();
        }
    }
}

/// All five aliases through the live entry. A captured tell with its sender and target ids
/// replaced supplies the last-teller name; two synthetic broadcasts supply monarch and patron.
/// The application's HUD event delivery and the next `Hud::drive` populate
/// `GamePlayScreen::reply_targets`, rather than a direct screen-field assignment, so the
/// application's own state owner and frame-to-screen transfer are what is tested.
///
/// Each `REPLY_ALIASES` entry is typed without its trailing space (no expansion), then with it
/// (the exact composed text and the caret position). Removing the `Hud::drive` transfer leaves
/// every alias unexpanded.
#[test]
fn a_captured_tell_arms_all_five_aliases_in_the_live_tree() {
    let tells = corpus_tells();
    let recorded = &tells[0].1;
    let me = ObjectId(0x5000_0007);
    let blob = with_target_id(&with_sender_id(recorded, ObjectId(0x5000_0009)), me);
    let name = decode(&blob).sender_name;

    let mut app = app_in_gameplay(4);
    let entry = find(&app, ENTRY);
    let at = centre(&app, entry);
    {
        let (_, screen) = gameplay(&mut app);
        assert_eq!(
            screen.reply_targets,
            dereth_ui_screens::chat::window::ReplyTargets::default(),
            "the screen comes up with no remembered names, as the client does"
        );
    }

    // The producer is fed to the application's own `Hud` and object tables, so what reaches the
    // screen is what `App::frame`'s own `Hud::drive` puts there. A locally built `Hud` would be
    // overwritten by the application's on the next frame.
    app.apply_hud_events(&session_events(me, &blob));
    app.frame();
    {
        let (_, screen) = gameplay(&mut app);
        assert_eq!(
            screen.reply_targets.last_teller.as_deref(),
            Some(name.as_str()),
            "the name off the wire reached the screen through the frame loop"
        );
    }

    let mut hand = Hand::new();
    hand.click(&mut app, at);
    {
        let (ui, _) = gameplay(&mut app);
        assert_eq!(
            ui.focus_element(),
            Some(entry),
            "the click focused the entry box"
        );
    }

    // Type each supported reply alias into the focused text element; character message 0x12
    // routes to the entry-character consumer and expands only when the final space arrives.
    let mut expanded = 0usize;
    use dereth_client_contract::chat::entry::ReplyTarget;
    for (alias, target) in [
        ("r ", ReplyTarget::LastTeller),
        ("rp ", ReplyTarget::LastTeller),
        ("reply ", ReplyTarget::LastTeller),
        ("mr ", ReplyTarget::Monarch),
        ("pr ", ReplyTarget::Patron),
    ] {
        let want_name = match target {
            ReplyTarget::LastTeller => name.as_str(),
            ReplyTarget::Monarch => "Aldis",
            ReplyTarget::Patron => "Plonk",
        };
        set_entry(&mut app, "");
        app.frame();
        // Everything but the trailing space, which is the trigger.
        let head = alias.strip_suffix(' ').unwrap_or(alias);
        hand.type_text(&mut app, &format!("@{head}"));
        assert_eq!(
            entry_text(&mut app),
            format!("@{head}"),
            "`@{head}` alone expands nothing"
        );
        hand.type_text(&mut app, " ");

        let want = format!("@tell {want_name}, ");
        assert_eq!(
            entry_text(&mut app),
            want,
            "`@{alias}` expands to the {target:?} name"
        );
        let (ui, _) = gameplay(&mut app);
        let cursor = ui
            .text_element_mut(entry)
            .map(|t| t.cursor)
            .expect("the entry is a text box");
        assert_eq!(
            cursor,
            want.chars().count(),
            "cursor position equals len(\"@tell {want_name},\") + 1 -- past the typed space"
        );
        expanded += 1;
    }
    assert_eq!(expanded, 5, "all five supported aliases expanded");
    eprintln!("5 of 5 aliases expand from recorded tell and synthetic channel names");
    app.shutdown();
}

/// Behaviour: chat.tell.the-three-reply-keys-address-three-different-people
///
/// The reply actions select three different remembered speakers: monarch (0x10000020),
/// patron (0x10000021), and last teller (0x10000022). The chat window emits an entry request;
/// the application frame runs that request through the shared entry owner and applies its
/// update before the visible recipient is read back.
#[test]
fn the_three_reply_keys_address_three_different_people() {
    let mut app = app_in_gameplay(4);
    let me = ObjectId(0x5000_0007);
    let tells = corpus_tells();
    let blob = with_target_id(&with_sender_id(&tells[0].1, ObjectId(0x5000_0009)), me);
    let teller = decode(&blob).sender_name;
    app.apply_hud_events(&session_events(me, &blob));
    app.frame();

    use dereth_client_contract::actions::chat_entry;
    for (act, want) in [
        (chat_entry::MONARCH_REPLY.0, "Aldis"),
        (chat_entry::PATRON_REPLY.0, "Plonk"),
        (chat_entry::REPLY.0, teller.as_str()),
    ] {
        set_entry(&mut app, "");
        let (ui, screen) = gameplay(&mut app);
        assert!(
            screen.chat_on_action(ui, act),
            "{act:#X} is a handled chat action"
        );
        assert!(app.frame(), "the entry request reaches its shared owner");
        assert_eq!(
            entry_text(&mut app),
            format!("@tell {want}, "),
            "{act:#X} composes a tell to {want}"
        );
    }
    app.shutdown();
}

// -------------------------------------------------------------------------------------------
// 5. `@r ` becomes a tell, through the real element message
// -------------------------------------------------------------------------------------------

/// Behaviour: chat.aliases.typing-a-space-after-a-reply-alias-expands-it-into-a-tell
///
/// `@r hello` typed into the box becomes `@tell <last teller>, hello`.
///
/// Character messages reach the focused text element, whose per-character message 0x12 is
/// routed by `GamePlayScreen::on_element_message` to the chat window's `on_entry_character`,
/// which checks both the entry's identity and a space character.
///
/// The resulting values expose off-by-one errors: the half-open replacement removes `r` but
/// leaves its space, and the caret becomes the replacement's length plus 1, just past that space.
#[test]
fn typing_the_space_after_an_alias_expands_it_into_a_tell() {
    let mut app = app_in_gameplay(4);
    let entry = find(&app, ENTRY);
    let at = centre(&app, entry);

    // All three reply names arrive through synthetic protocol events and the HUD's
    // communication state, rather than by assigning the screen's reply targets.
    remember_three_speakers(&mut app);

    let mut hand = Hand::new();
    hand.click(&mut app, at);
    {
        let (ui, _) = gameplay(&mut app);
        assert_eq!(
            ui.focus_element(),
            Some(entry),
            "the click focused the entry box"
        );
    }

    // `@r` alone is not an alias: the alias literal is `r `, with the space.
    hand.type_text(&mut app, "@r");
    assert_eq!(entry_text(&mut app), "@r", "nothing has expanded yet");

    // ...and then the space arrives, which is the whole trigger.
    hand.type_text(&mut app, " ");
    assert_eq!(
        entry_text(&mut app),
        "@tell Alba, ",
        "the alias and the sigil are gone and the typed space survived"
    );
    {
        let (ui, _) = gameplay(&mut app);
        let cursor = ui
            .text_element_mut(entry)
            .map(|t| t.cursor)
            .expect("the entry is a text box");
        assert_eq!(
            cursor, 12,
            "cursor position equals len(\"@tell Alba,\") + 1 -- past the space"
        );
    }

    // The rest of the line is typed after the caret, which is what makes this useful at all.
    hand.type_text(&mut app, "well met");
    assert_eq!(entry_text(&mut app), "@tell Alba, well met");

    // A second space is an ordinary space: the expansion already happened and `@tell ` is not an
    // alias. This is the direction that would fail on a build that re-ran the replacement.
    hand.type_text(&mut app, " ");
    assert_eq!(entry_text(&mut app), "@tell Alba, well met ");

    // The trigger is a space character, not merely text that begins with an alias. Install an
    // alias-prefixed line directly and type an ordinary character: replacement must not run.
    // Without the guard, editing that line would expand it again under the caret. The earlier
    // cases cannot discriminate this failure because completing an alias always types a space.
    {
        let (ui, _) = gameplay(&mut app);
        if let Some(t) = ui.text_element_mut(entry) {
            t.set_text("@r hello");
            t.cursor = t.glyphs.len();
        }
    }
    app.frame();
    hand.type_text(&mut app, "!");
    assert_eq!(
        entry_text(&mut app),
        "@r hello!",
        "a non-space character must not run the replacement"
    );
    app.shutdown();
}

/// `@mr ` and `@pr ` reach the other two names, and a line with no sigil is left alone.
///
/// Aliases are tried only after a leading '/' or '@'; without that guard an unprefixed `r `
/// line could become a private tell. The no-sigil example must remain exactly as typed.
#[test]
fn the_other_two_aliases_and_the_sigil_guard() {
    let mut app = app_in_gameplay(4);
    let entry = find(&app, ENTRY);
    let at = centre(&app, entry);
    remember_three_speakers(&mut app);
    let mut hand = Hand::new();
    hand.click(&mut app, at);

    hand.type_text(&mut app, "@mr ");
    assert_eq!(
        entry_text(&mut app),
        "@tell Aldis, ",
        "`mr ` is the monarch"
    );

    // Clear and try the patron.
    {
        let (ui, _) = gameplay(&mut app);
        if let Some(t) = ui.text_element_mut(entry) {
            t.set_text("");
        }
    }
    hand.type_text(&mut app, "@pr ");
    assert_eq!(entry_text(&mut app), "@tell Plonk, ", "`pr ` is the patron");

    // No sigil: left exactly as typed.
    {
        let (ui, _) = gameplay(&mut app);
        if let Some(t) = ui.text_element_mut(entry) {
            t.set_text("");
        }
    }
    hand.type_text(&mut app, "rats ");
    assert_eq!(
        entry_text(&mut app),
        "rats ",
        "no `/` or `@`, so nothing is rewritten"
    );
    app.shutdown();
}
