//! Heard speech: the three "hear" messages (`0x02BB` speech, `0x02BC` ranged speech, `0x02BD`
//! tells) are composed by `dereth_client::chat` through the real `Hud` and drawn on the shipped
//! chat log in their channel's colour, and the stay-in-chat-mode option reaches the chat entry.
//!
//! The whole line -- name, verb, comma and quotes -- is the message body; the prefix slot is
//! the optional timestamp. The display path builds two string records and appends them in this
//! order:
//!
//! ```text
//!   argument 4: window id
//!   argument 3: string record 2 = the timestamp
//!   argument 2: string record 1 = the body
//!   argument 1: chat type
//! ```
//!
//! Record 2 is the `%#H:%M:%S ` clock, produced only when the display-timestamps player option
//! is on (and never for chat type `0x1A`).
//!
//! | claim | oracle |
//! |---|---|
//! | the seven templates and their argument order | the client's format literals |
//! | `0x02BC` has no `You say` arm | the self-speech template belongs to the ordinary-speech handler only; the behavioural distinction is exercised below |
//! | what a line looks like | recorded `0x02BB` / `0x02BD` bodies, replayed through the real `ClientNetwork` / `ObjectStream` / `Hud` |
//! | what is drawn | the log element's own glyphs, read back after the append, never the model |
//! | the colour | asserted separately from the text, so a right string in the wrong colour and a wrong string in the right colour each fail one assertion and not the other |
//! | `StayInChatMode` | the first option word shifted by 11 and masked by 1 |
//!
//! The recordings carry no `0x02BC` ([`the_corpus_carries_no_ranged_speech_at_all`]), so the
//! ranged line is a recorded `0x02BB` body re-addressed as a `0x02BC`: every string in it is off
//! the wire and the framing is this test's.
//!
//! Fixture: seven named recordings fed to a `ClientNetwork` with no socket peer; the one gesture
//! that raises a request (the Enter in the stay-in-chat-mode test) raises it into a process-local
//! queue. Every fixture path is an `expect` or an `assert!`.

use crate::common::sim_app::gameplay;

use crate::common::client_dir_or_workspace_client as client_dir;

use dereth_client::app::App;
use dereth_client::chat;
use dereth_client::config::Config;
use dereth_client::hud::Hud;
use dereth_client::net::ClientNetwork;
use dereth_client::objects::ObjectStream;
use dereth_client_model::player::options::option::{DISPLAY_TIME_STAMPS, STAY_IN_CHAT_MODE};
use dereth_client_model::scroll;
use dereth_client_net::client_session::testing::capture::{shared_session, Datagram};
use dereth_client_net::client_session::SessionEvent;
use dereth_client_net::recording::connection_sequence_number;
use dereth_primitives::LocalTime;
use dereth_protocol::comms::{
    CommunicationHearDirectSpeech, CommunicationHearRangedSpeech, CommunicationHearSpeech,
};
use dereth_protocol::{Message as _, Opcode};
use dereth_ui::framework::mode;
use dereth_ui::{ElemHandle, ElementId, UiSystem};
use dereth_ui_screens::chat::colors;
use dereth_ui_screens::chat::interface::ChatMessage;
use dereth_ui_screens::chat::window::{ENTRY, LOG};

use dereth_ui_screens::view::{PlayerOption, UiRequest};
use winit::keyboard::KeyCode;

const OPAQUE: u32 = 0xFF00_0000;

/// The shipped chat log `0x10000011` supplies attribute `0x1D`, element 0, as the colour for a
/// `Tell` run's glyphs.
const TAG_COLOR: u32 = 0xFF00_B200;

// =============================================================================================
// The corpus -- the same replay `chat::reply_targets` uses
// =============================================================================================

/// The seven named recordings every claim in this file is measured over, in file-name order.
/// Later recordings (the `fellowship-*` sessions, with overheard says and a player-to-player tell)
/// are not part of the population.
const SEVEN_SESSIONS: [&str; 7] = [
    "ddd-interrogation-only",
    "early-inventory-and-casting",
    "first-login-walk-jump",
    "login-account-booted",
    "long-solo-play",
    "short-play-with-training",
    "short-second-connection",
];

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

/// Every incoming `0x02BB`, `0x02BC` and `0x02BD` in the seven captures, in one pass.
fn corpus_speech() -> (
    Vec<CommunicationHearSpeech>,
    Vec<CommunicationHearRangedSpeech>,
    Vec<CommunicationHearDirectSpeech>,
) {
    let (mut says, mut ranged, mut tells) = (Vec::new(), Vec::new(), Vec::new());
    for name in SEVEN_SESSIONS {
        for e in replay(shared_session(name)) {
            let Some((opcode, body)) = e.ui_body() else {
                continue;
            };
            let mut r = dereth_protocol::archive::Reader::new(body);
            match opcode {
                Opcode::COMMUNICATION_HEAR_SPEECH => {
                    says.push(CommunicationHearSpeech::read(&mut r).expect("a recorded 0x02BB"));
                }
                Opcode::COMMUNICATION_HEAR_RANGED_SPEECH => {
                    ranged.push(
                        CommunicationHearRangedSpeech::read(&mut r).expect("a recorded 0x02BC"),
                    );
                }
                Opcode::COMMUNICATION_HEAR_DIRECT_SPEECH => {
                    tells.push(
                        CommunicationHearDirectSpeech::read(&mut r).expect("a recorded 0x02BD"),
                    );
                }
                _ => {}
            }
        }
    }
    (says, ranged, tells)
}

/// Put the `Hud` into the state the three speech handlers require before they will draw anything.
///
/// * The player body must exist for all three handlers: a client with no body draws no speech at
///   all. `Hud::player_body` is written by `Hud::sync` from the `ViewerFrame` `App::frame` builds
///   out of `WorldScene::character`; the emote and hearing tests drive that producer for real,
///   standing a recorded speaker at measured distances. Here it is set directly, because this
///   file's subject is the seven format templates and not the geometry.
/// * A physics body for the speaker: ranged speech resolves both ids and answers out of range if
///   either is missing, so `0x02BC` from a speaker the object table has never seen draws nothing.
///   The radar entry below is that body, standing at the listener's own feet so that any positive
///   `range` on the message admits it.
///
/// The asymmetry is the client's, not this helper's: the ordinary audibility check's missing-body
/// arm is an escape (audible at any range) while the ranged-distance check's missing-body arm is
/// a refusal. `0x02BB` and `0x02BD` therefore do not need the radar entry and `0x02BC` cannot do
/// without it.
fn embody(hud: &mut Hud, speaker: dereth_primitives::ObjectId) {
    hud.player_body = true;
    hud.radar.push(dereth_ui_screens::view::RadarEntry {
        id: speaker,
        player_space: (0.0, 0.0, 0.0),
        in_world: true,
        ..dereth_ui_screens::view::RadarEntry::default()
    });
}

/// Feed one message to a real `Hud` as the network would, and take the `ChatMessage`s it pushed.
fn through_hud<M: dereth_protocol::Message>(
    player: Option<dereth_primitives::ObjectId>,
    m: &M,
) -> Vec<ChatMessage> {
    through_hud_from(player, None, m)
}

/// [`through_hud`] with the speaker named, so the `0x02BC` range check (`objects_in_range`) has a body to measure.
fn through_hud_from<M: dereth_protocol::Message>(
    player: Option<dereth_primitives::ObjectId>,
    speaker: Option<dereth_primitives::ObjectId>,
    m: &M,
) -> Vec<ChatMessage> {
    let mut hud = Hud::default();
    let mut objects = ObjectStream::new();
    if let Some(p) = player {
        let _ = hud.apply_events(&[SessionEvent::PlayerCreated(p)], &mut objects.world);
    }
    embody(
        &mut hud,
        speaker.or(player).unwrap_or(dereth_primitives::ObjectId(0)),
    );
    let mut w = dereth_protocol::archive::Writer::new();
    w.u32(M::OPCODE.0);
    m.write(&mut w).expect("the message re-encodes");
    hud.apply_events(
        &[SessionEvent::UiEvent {
            opcode: M::OPCODE,
            blob: w.into_inner(),
        }],
        &mut objects.world,
    )
}

// =============================================================================================
// 1. The denominators
// =============================================================================================

/// The corpus carries no `0x02BC` at all, beside the counts of the two it does carry.
///
/// This is why the ranged line in the drawn test is a recorded `0x02BB` body re-addressed. The
/// other two counts are asserted beside it, so a replay that silently stopped decoding cannot pass
/// as a corpus with no ranged speech in it.
#[test]
fn the_corpus_carries_no_ranged_speech_at_all() {
    let (says, ranged, tells) = corpus_speech();
    assert!(
        !says.is_empty(),
        "the recordings carry overheard `0x02BB` say lines"
    );
    assert!(
        !tells.is_empty(),
        "the recordings carry incoming `0x02BD` tells"
    );
    assert_eq!(
        ranged.len(),
        0,
        "and zero `0x02BC`; the ranged-speech handler is not exercised by a captured fixture"
    );
}

// =============================================================================================
// 2. The three arms, through the real `Hud`
// =============================================================================================

/// Every recorded tell reaches `Hud`'s output as one body with no prefix, and the body is the
/// one `chat::hear_direct_speech_line` composes (the speaker's name is inside the body).
#[test]
fn every_recorded_tell_comes_out_of_the_hud_as_one_body_and_no_prefix() {
    let (_, _, tells) = corpus_speech();
    let mut drawn = 0usize;
    for m in &tells {
        let out = through_hud(Some(m.target_id), m);
        let want = chat::hear_direct_speech_line(
            m.sender_id.0,
            m.target_id.0,
            Some(m.target_id.0),
            &m.sender_name,
            &m.message,
        )
        .expect("every recorded tell is addressed to the player that received it");
        assert_eq!(out.len(), 1, "one line for {m:?}");
        assert_eq!(
            out[0].prefix, None,
            "the prefix slot is the timestamp's, not the speaker's"
        );
        assert_eq!(out[0].body, want);
        assert_eq!(u32::from(out[0].ty), m.text_type);
        // The name is *inside* the body, and the verb is between it and the message.
        assert!(out[0]
            .body
            .contains(&format!("{} tells you, \"", m.sender_name)));
        drawn += 1;
    }
    assert_eq!(
        drawn,
        tells.len(),
        "every recorded tell was driven, not a subset"
    );
    assert!(drawn > 0, "the recordings carry tells to drive");
}

/// Behaviour: chat.speech.your-own-echo-and-a-remote-speaker-are-drawn-from-two-different-forms
///
/// `0x02BB` takes the `You say` arm for the player's own line and the `says` arm for anyone
/// else's.
///
/// Driven from the same recorded message twice, changing only which id the `Hud` believes is the
/// player. Both directions, because a test that only drives the remote arm passes on a build where
/// the self-echo branch does not exist, which is what `0x02BC` is.
#[test]
fn the_recorded_say_line_echoes_as_you_say_when_the_speaker_is_the_player() {
    let (says, _, _) = corpus_speech();
    let m = says
        .iter()
        .find(|m| !m.message.is_empty() && !m.sender_name.is_empty())
        .expect("a recorded 0x02BB with a body and a speaker");

    let remote = through_hud(Some(dereth_primitives::ObjectId(m.sender_id.0 ^ 1)), m);
    assert_eq!(remote.len(), 1);
    assert_eq!(remote[0].prefix, None);
    assert_eq!(
        remote[0].body,
        format!("{} says, \"{}\"", tell_run_of(m), m.message)
    );

    let echo = through_hud(Some(m.sender_id), m);
    assert_eq!(echo.len(), 1);
    assert_eq!(echo[0].prefix, None);
    assert_eq!(
        echo[0].body,
        format!("You say, \"{}\"", m.message),
        "`You say, \"%s\"`, the player's own line"
    );
    assert_ne!(
        echo[0].body, remote[0].body,
        "the two arms are not the same string"
    );
}

/// The subject half of a recorded speaker, spelled out here rather than taken from the local
/// formatter: a player id inside `0x50000001..=0x6FFFFFFF` becomes
/// the clickable `<Tell:...>` run, anything else is the bare name.
fn tell_run_of(m: &CommunicationHearSpeech) -> String {
    let id = m.sender_id.0;
    if (0x5000_0001..0x7000_0000).contains(&id) {
        format!("<Tell:IIDString:{id}:{0}>{0}<\\Tell>", m.sender_name)
    } else {
        m.sender_name.clone()
    }
}

/// `0x02BC` is routed to `hear_ranged_speech_line`, not to `hear_speech_line`: the one thing
/// about this wire that is easy to get wrong and that no corpus message can catch.
///
/// The ranged handler uses exactly two templates, `SAYS` and `SAYS_CLICKABLE`, and has no
/// `You say` arm, so a ranged line echoed back to its own speaker still renders as the speaker
/// saying the text. Squelch and range gates normally make that case unreachable; this test
/// reproduces the branch structure rather than claiming it occurs in ordinary play.
///
/// The body is a recorded `0x02BB` body re-addressed as a `0x02BC`; see
/// [`the_corpus_carries_no_ranged_speech_at_all`].
///
/// Falsified by routing the `0x02BC` arm to `chat::hear_speech_line`: the self-addressed case
/// below then renders `You say, "..."`.
#[test]
fn a_ranged_line_addressed_to_its_own_speaker_still_says_says() {
    let (says, _, _) = corpus_speech();
    let src = says
        .iter()
        .find(|m| !m.message.is_empty() && !m.sender_name.is_empty())
        .expect("a recorded 0x02BB to re-address");
    let m = CommunicationHearRangedSpeech {
        message: src.message.clone(),
        sender_name: src.sender_name.clone(),
        sender_id: src.sender_id,
        range: 20.0,
        text_type: src.text_type,
    };

    // The player *is* the speaker — the only case in which the two handlers differ. The speaker
    // is named so the ranged check has a body to measure; see `embody`.
    let out = through_hud_from(Some(m.sender_id), Some(m.sender_id), &m);
    assert_eq!(out.len(), 1);
    assert_eq!(out[0].prefix, None);
    assert_eq!(
        out[0].body,
        format!("{} says, \"{}\"", tell_run_of(src), m.message),
        "0x02BC has no `You say` arm"
    );
    assert!(
        !out[0].body.starts_with("You say"),
        "and it must not borrow the other handler's"
    );
}

/// Behaviour: chat.tell.a-tell-addressed-to-someone-else-is-counted-and-not-drawn
///
/// A `0x02BD` addressed to somebody else draws nothing at all, and the client counts it rather
/// than losing it.
///
/// After the self-tell branch, the direct-speech handler requires the target id to equal the
/// player's id; a mismatch returns without formatting or appending text.
/// `hear_direct_speech_line` answers `None`, and the arm pushes nothing.
///
/// Three states, not two: the counters separate "no tell arrived" from "a tell arrived and was
/// correctly not drawn".
#[test]
fn a_tell_addressed_to_somebody_else_is_counted_and_not_drawn() {
    let (_, _, tells) = corpus_speech();
    let m = tells
        .iter()
        .find(|m| !m.message.is_empty())
        .expect("a recorded tell with a body");

    let mut hud = Hud::default();
    let mut objects = ObjectStream::new();
    // A player who is neither the sender nor the target: the shard copied us somebody else's tell.
    let bystander = dereth_primitives::ObjectId(m.target_id.0 ^ 0x0F00_0000);
    assert_ne!(bystander, m.target_id);
    assert_ne!(bystander, m.sender_id);
    let _ = hud.apply_events(
        &[SessionEvent::PlayerCreated(bystander)],
        &mut objects.world,
    );
    embody(&mut hud, m.sender_id);

    let mut w = dereth_protocol::archive::Writer::new();
    w.u32(Opcode::COMMUNICATION_HEAR_DIRECT_SPEECH.0);
    m.write(&mut w).expect("the message re-encodes");
    let out = hud.apply_events(
        &[SessionEvent::UiEvent {
            opcode: Opcode::COMMUNICATION_HEAR_DIRECT_SPEECH,
            blob: w.into_inner(),
        }],
        &mut objects.world,
    );
    assert!(out.is_empty(), "nothing is drawn: {out:?}");
    assert_eq!(hud.stats.speech_lines_composed, 0);
    assert_eq!(
        hud.stats.speech_lines_not_addressed_to_us, 1,
        "and it was seen, not lost"
    );
    assert_eq!(
        hud.stats.undecodable, 0,
        "it decoded fine — this is a branch, not a failure"
    );

    // The same message with the right player takes the other branch, so the gate is shown live in
    // both directions rather than only in the one that draws nothing.
    let drawn = through_hud(Some(m.target_id), m);
    assert_eq!(drawn.len(), 1, "addressed to us, it draws");
}

/// A self-tell is `You think, "..."`, and that branch is taken before the player id is consulted.
///
/// The sender-equals-target comparison sits above the player-target gate, so a self-tell renders
/// even for a `Hud` that has never seen a `PlayerCreated`. Asserted with no player at all, which
/// is the state that discriminates.
#[test]
fn a_self_tell_is_you_think_even_before_the_player_id_is_known() {
    let (_, _, tells) = corpus_speech();
    let src = tells
        .iter()
        .find(|m| !m.message.is_empty())
        .expect("a recorded tell with a body");
    let m = CommunicationHearDirectSpeech {
        message: src.message.clone(),
        sender_name: src.sender_name.clone(),
        sender_id: src.sender_id,
        target_id: src.sender_id,
        text_type: src.text_type,
        secret_flags: src.secret_flags,
    };
    let out = through_hud(None, &m);
    assert_eq!(
        out.len(),
        1,
        "no `PlayerCreated` has landed and it still draws"
    );
    assert_eq!(out[0].body, format!("You think, \"{}\"", m.message));
    assert_eq!(out[0].prefix, None);
}

// =============================================================================================
// 3. The rendered line, on the shipped chat log, glyph by glyph
// =============================================================================================

fn base_config() -> Config {
    Config {
        headless: true,
        sound: false,
        dat_dir: client_dir(),
        ..Config::default()
    }
}

fn app_in_gameplay(frames: u32) -> App {
    let d = client_dir();
    assert!(
        dereth_dat::testing::have_dats(),
        "the retail dats are required at {} -- set DERETH_TEST_DAT_DIR",
        d.display()
    );
    let cfg = Config {
        ui: true,
        ..base_config()
    };
    let mut app = crate::common::sim_app::new(cfg).expect("the application comes up");
    app.start_shell().expect("the UI comes up");
    let s = dereth_client::world::SceneConfig {
        landblock: app.config().landblock,
        land_radius: app.config().land_radius,
        scenery_radius: app.config().scenery_radius,
        ..dereth_client::world::SceneConfig::default()
    };
    app.load_static_scene(s).expect("the static scene loads");
    app.queue_ui_mode(mode::GAME_PLAY);
    for _ in 0..frames {
        app.frame();
    }
    app
}

fn find(app: &App, id: ElementId) -> ElemHandle {
    let shell = app.ui().expect("shell");
    let root = shell.flow.current().expect("a screen").roots()[0];
    shell
        .ui
        .get_child_recursive(root, id)
        .unwrap_or_else(|| panic!("{id:?} is in the shipped layout"))
}

/// The log's glyphs as `(text, colour)` runs — what a player actually sees, read back off the
/// element rather than off the colour table.
fn runs(ui: &mut UiSystem, log: ElemHandle) -> Vec<(String, u32)> {
    let t = ui
        .text_element_mut(log)
        .expect("the chat log is a text element");
    let mut out: Vec<(String, u32)> = Vec::new();
    for g in &t.glyphs.glyphs {
        let ch = char::from_u32(u32::from(g.data)).unwrap_or('\u{FFFD}');
        match out.last_mut() {
            Some((s, c)) if *c == g.color => s.push(ch),
            _ => out.push((ch.to_string(), g.color)),
        }
    }
    out
}

/// All three "hear" opcodes, composed by the real `Hud` and drawn on the shipped chat log in
/// their own channel's colour.
///
/// A line whose speaker is a `<Tell:IIDString:...>` link is two colour runs: the text renderer
/// draws a `Tell` tag's glyphs with the tag colour (attribute `0x1D`, `#00B200` on the shipped
/// log) instead of the ordinary font colour. The body of each line carries its channel's colour,
/// the three channels differ on the element, and none of them is the prefix grey by accident.
///
/// This reads the glyphs, not the `ChatMessage`. Colour is asserted apart from text at every step.
///
/// The `0x02BB` and `0x02BD` bodies are recorded; the `0x02BC` is a recorded `0x02BB` body
/// re-addressed, because the corpus carries none (see
/// [`the_corpus_carries_no_ranged_speech_at_all`]).
#[test]
fn all_three_hear_opcodes_draw_one_coloured_run_on_the_shipped_log() {
    let (says, _, tells) = corpus_speech();

    let tell = tells
        .iter()
        .find(|m| m.text_type == 3 && !m.message.is_empty())
        .expect("a recorded channel-3 tell");
    let say = says
        .iter()
        .find(|m| m.text_type == 17 && !m.message.is_empty() && !m.sender_name.is_empty())
        .expect("a recorded channel-17 say line");
    // A different recorded body for the ranged line, because three lines that share a string
    // cannot be told apart on the element and the search below would read the wrong run's colour.
    let say2 = says
        .iter()
        .find(|m| !m.message.is_empty() && !m.sender_name.is_empty() && m.message != say.message)
        .expect("a second recorded say line with a different body");
    let ranged = CommunicationHearRangedSpeech {
        message: say2.message.clone(),
        sender_name: say2.sender_name.clone(),
        sender_id: say2.sender_id,
        range: 20.0,
        text_type: 12,
    };

    // Composed by the real `Hud`, from the real bytes -- not by calling `chat::*` here.
    let lines: Vec<(&str, ChatMessage)> = vec![
        ("0x02BD", through_hud(Some(tell.target_id), tell).remove(0)),
        (
            "0x02BB",
            through_hud(Some(dereth_primitives::ObjectId(1)), say).remove(0),
        ),
        (
            "0x02BC",
            through_hud_from(
                Some(dereth_primitives::ObjectId(1)),
                Some(ranged.sender_id),
                &ranged,
            )
            .remove(0),
        ),
    ];

    // Three distinct strings on one log, so every `find` below can only match its own line.
    {
        let mut bodies: Vec<&str> = lines.iter().map(|(_, l)| l.body.as_str()).collect();
        bodies.sort_unstable();
        bodies.dedup();
        assert_eq!(bodies.len(), 3, "the three lines are textually distinct");
    }

    let mut app = app_in_gameplay(4);
    let log = find(&app, LOG);
    let mut seen: Vec<(String, u32)> = Vec::new();

    for (name, line) in &lines {
        assert_eq!(
            line.prefix, None,
            "{name}: the prefix slot is the timestamp's"
        );
        let want_color = OPAQUE | colors::color_for_type(line.ty).hex;
        let before = {
            let (ui, _) = gameplay(&mut app);
            runs(ui, log).len()
        };
        {
            let (ui, screen) = gameplay(&mut app);
            assert!(
                !screen.recv_display_final_string_info(ui, line).is_empty(),
                "{name}: channel {} is accepted by the main window's 0xFBFFFFFF",
                line.ty
            );
        }
        let (ui, _) = gameplay(&mut app);
        let all = runs(ui, log);
        assert!(all.len() > before, "{name}: the line reached the log");

        // `dereth_ui::text::tag::parse` strips the `<Tell:...>` markup off the glyphs and hangs it on
        // them as a tag, so the drawn text is the body minus the markup.
        let visible = strip_tell_markup(&line.body);
        let joined: String = all.iter().map(|(s, _)| s.as_str()).collect();
        assert!(
            joined.contains(&visible),
            "{name}: the log draws {visible:?}; it has {joined:?}"
        );
        assert!(
            !joined.contains("<Tell:IIDString:"),
            "{name}: the clickable markup is a tag on the glyphs, never drawn text"
        );

        // THE TEXT: one run carrying the verb and the message. When the speaker is a link its
        // glyphs are their own run, so the run to read the channel colour off is the part after
        // `<\Tell>`; with no link that is the whole visible line.
        let tail = line
            .body
            .split_once(r"<\Tell>")
            .map_or_else(|| visible.clone(), |(_, r)| r.to_owned());
        let run = all
            .iter()
            .find(|(s, _)| s.contains(&tail))
            .unwrap_or_else(|| panic!("{name}: the line's body is one run; got {all:?}"));
        if let Some(speaker) = visible.strip_suffix(&tail).filter(|s| !s.is_empty()) {
            let link = all
                .iter()
                .find(|(s, _)| s.contains(speaker))
                .unwrap_or_else(|| panic!("{name}: the speaker is drawn; got {all:?}"));
            assert_eq!(
                link.1, TAG_COLOR,
                "{name}: the clickable speaker takes the tag colour"
            );
            assert_ne!(link.1, want_color, "{name}: and not the line's own colour");
        }
        // THE COLOUR: asserted separately, and asserted not to be the prefix grey.
        assert_eq!(
            run.1, want_color,
            "{name}: drawn in channel {}'s colour",
            line.ty
        );
        if line.ty != 12 {
            assert_ne!(
                run.1,
                OPAQUE | colors::GREY.hex,
                "{name}: the speaker is not in the prefix colour -- the prefix is the timestamp"
            );
        }
        seen.push((visible, run.1));
    }

    // Three channels, and the two that differ really did differ **on the element**. A build with no
    // colour table draws all three the same and cannot pass this.
    assert_eq!(
        seen[0].1,
        OPAQUE | colors::YELLOW.hex,
        "0x02BD channel 3 is yellow"
    );
    assert_eq!(
        seen[1].1,
        OPAQUE | colors::LIGHT_BLUE.hex,
        "0x02BB channel 17 is light blue"
    );
    assert_eq!(
        seen[2].1,
        OPAQUE | colors::GREY.hex,
        "0x02BC channel 12 is grey"
    );
    assert_ne!(seen[0].1, seen[1].1);

    // And the speaker's name is not drawn in front of an unformatted line.
    let (ui, _) = gameplay(&mut app);
    let joined: String = runs(ui, log).iter().map(|(s, _)| s.as_str()).collect();
    assert!(
        joined.contains(&format!("{} tells you, \"", tell.sender_name)),
        "the NPC's name is followed by the client's verb"
    );
    assert!(
        !joined.contains(&format!("{}{}", tell.sender_name, tell.message)),
        "and the name never abuts the message; this rejects the formerly observed unformatted adjacency"
    );
    app.shutdown();
}

/// `dereth_ui::text::tag::parse`'s effect on the drawn text, spelled out here rather than called, so
/// the expectation is independent of the parser.
fn strip_tell_markup(body: &str) -> String {
    let mut out = String::new();
    let mut rest = body;
    while let Some(open) = rest.find('<') {
        out.push_str(&rest[..open]);
        let close = rest[open..].find('>').map_or(rest.len(), |i| open + i + 1);
        rest = &rest[close..];
    }
    out.push_str(rest);
    out
}

// =============================================================================================
// 4. The timestamp — `ChatMessage::prefix`'s real producer
// =============================================================================================

/// The timestamp prefix has a producer, and it is the option word that turns it on.
///
/// `Hud::apply_events` pushes the display-timestamps option (ordinal 33, `options2_` bit 6) off
/// `PlayerSystem::options` at the head of every batch, and the stamp is
/// `scroll::timestamp_prefix`.
///
/// Asserted both ways from the option word and not by writing `display_time_stamps`: a test that
/// sets the field cannot show a missing producer. The counters are the third state: a batch with
/// the option off is a batch that ran and declined.
#[test]
fn the_option_word_turns_the_timestamp_prefix_on_and_off() {
    let (_, _, tells) = corpus_speech();
    let m = tells
        .iter()
        .find(|m| !m.message.is_empty())
        .expect("a recorded tell");

    let drive = |on: bool| -> (Vec<ChatMessage>, u64, u64) {
        let mut hud = Hud::default();
        let mut objects = ObjectStream::new();
        // The option word, as a `0x0013` would leave it. Ordinal 33 is `options2_` bit 6, written
        // here as the literal the client uses.
        assert_eq!(DISPLAY_TIME_STAMPS, 33);
        objects
            .world
            .player_system
            .options
            .set(DISPLAY_TIME_STAMPS, on);
        assert_eq!(
            objects.world.player_system.options.options2 & 0x0000_0040 != 0,
            on,
            "the bit really is options2_ 0x40"
        );
        let _ = hud.apply_events(
            &[SessionEvent::PlayerCreated(m.target_id)],
            &mut objects.world,
        );
        embody(&mut hud, m.sender_id);
        let mut w = dereth_protocol::archive::Writer::new();
        w.u32(Opcode::COMMUNICATION_HEAR_DIRECT_SPEECH.0);
        m.write(&mut w).expect("re-encodes");
        let out = hud.apply_events(
            &[SessionEvent::UiEvent {
                opcode: Opcode::COMMUNICATION_HEAR_DIRECT_SPEECH,
                blob: w.into_inner(),
            }],
            &mut objects.world,
        );
        (
            out,
            hud.stats.timestamps_stamped,
            hud.stats.timestamps_suppressed,
        )
    };

    let (off, stamped_off, suppressed_off) = drive(false);
    assert_eq!(off.len(), 1);
    assert_eq!(off[0].prefix, None, "the shipped default has no stamp");
    assert_eq!(
        (stamped_off, suppressed_off),
        (0, 1),
        "the stamper ran and declined"
    );

    let (on, stamped_on, suppressed_on) = drive(true);
    assert_eq!(on.len(), 1);
    assert_eq!((stamped_on, suppressed_on), (1, 0));
    let stamp = on[0]
        .prefix
        .clone()
        .expect("the option is on, so there is a stamp");
    assert_eq!(on[0].body, off[0].body, "the body is the same either way");

    // The shape, without pinning the wall clock: `H:MM:SS ` with a trailing space, and it is the
    // same string `scroll::timestamp_prefix` renders for some second of some day.
    let digits: Vec<&str> = stamp.trim_end().split(':').collect();
    assert_eq!(digits.len(), 3, "H:MM:SS, got {stamp:?}");
    let h: u32 = digits[0].parse().expect("hours");
    let mi: u32 = digits[1].parse().expect("minutes");
    let s: u32 = digits[2].parse().expect("seconds");
    assert!(h < 24 && mi < 60 && s < 60, "{stamp:?}");
    assert_eq!(digits[1].len(), 2, "%M keeps its leading zero");
    assert_eq!(digits[2].len(), 2, "%S keeps its leading zero");
    assert!(stamp.ends_with(' '), "the format's trailing space");
    assert_eq!(
        stamp,
        scroll::timestamp_prefix(i64::from(h * 3600 + mi * 60 + s), 0),
        "and it is exactly what the transcription renders for that instant"
    );
}

/// The stamp draws as its own run in the prefix colour, in front of the body's run.
/// The final-string display path appends the prefix with colour index `0x0C`; this test reads
/// that result from the glyphs.
///
/// This says what the prefix slot is for: two runs, grey then the channel's colour, where the
/// grey run holds a clock and not a name.
#[test]
fn the_stamp_is_a_grey_run_in_front_of_the_bodys_run() {
    let (_, _, tells) = corpus_speech();
    let m = tells
        .iter()
        .find(|m| m.text_type == 3 && !m.message.is_empty())
        .expect("a recorded channel-3 tell");
    let body = chat::hear_direct_speech_line(
        m.sender_id.0,
        m.target_id.0,
        Some(m.target_id.0),
        &m.sender_name,
        &m.message,
    )
    .expect("addressed to the player");
    let stamp = scroll::timestamp_prefix(13 * 3600 + 7 * 60 + 42, 0);
    assert_eq!(stamp, "13:07:42 ");

    let mut app = app_in_gameplay(4);
    let log = find(&app, LOG);
    {
        let (ui, screen) = gameplay(&mut app);
        let line = ChatMessage {
            feedback: dereth_client_contract::feedback::Feedback::ORDINARY,
            ty: 3,
            body: body.clone(),
            prefix: Some(stamp.clone()),
            window: 0,
        };
        assert!(!screen.recv_display_final_string_info(ui, &line).is_empty());
    }
    let (ui, _) = gameplay(&mut app);
    let all = runs(ui, log);
    assert_eq!(
        all,
        vec![
            (stamp, OPAQUE | colors::GREY.hex),
            (strip_tell_markup(&body), OPAQUE | colors::YELLOW.hex),
        ],
        "the clock is grey, the whole speech line is yellow, and the name is inside the yellow"
    );
    app.shutdown();
}

// =============================================================================================
// 5. The stay-in-chat-mode producer
// =============================================================================================

/// `StayInChatMode` is not an auto-save option.
///
/// Auto-save options go out at once as `0x0005`; `StayInChatMode` is ordinal 11, one of the
/// deferred options, and its bit reaches the shard on the next `0x01A1`. Its own route is inward:
/// the option word's bit has to reach `GamePlayScreen`. The list lengths and the membership are
/// reproduced here.
#[test]
fn stay_in_chat_mode_is_not_an_auto_save_option() {
    use dereth_client_model::player::options::{
        is_auto_save_option, AUTO_SAVE_ORDINALS, PLAYER_OPTIONS,
    };
    assert_eq!(PLAYER_OPTIONS.len(), 53, "53 ordinals");
    assert_eq!(AUTO_SAVE_ORDINALS.len(), 22, "22 of them are auto-save");
    assert_eq!(STAY_IN_CHAT_MODE, 11);
    assert_eq!(PLAYER_OPTIONS[STAY_IN_CHAT_MODE].0, "StayInChatMode");
    // The accessor shifts the first option word by 11 and masks one bit: option-word-one bit 11.
    // The literal, not the symbol, is pinned here.
    assert_eq!(PLAYER_OPTIONS[STAY_IN_CHAT_MODE].2, 0x0000_0800);
    assert!(matches!(
        PLAYER_OPTIONS[STAY_IN_CHAT_MODE].1,
        dereth_client_model::player::OptionWord::One
    ));
    assert!(
        !is_auto_save_option(STAY_IN_CHAT_MODE),
        "so the auto-save route does not touch this row"
    );
    // And it is clear in the shipped default.
    assert_eq!(
        dereth_client_model::player::options::DEFAULT_OPTIONS & 0x0000_0800,
        0
    );
}

fn focus_of(app: &App) -> Option<ElemHandle> {
    app.ui().expect("shell").ui.focus_element()
}

/// Set the option the way a player does — a tick on the Character Options page — and let the
/// application's own frame carry it to the screen.
fn tick_stay_in_chat_mode(app: &mut App, on: bool) {
    app.ui_mut()
        .expect("the UI shell is up")
        .ui
        .requests
        .emit(UiRequest::SetPlayerOption(PlayerOption::StayInChatMode, on));
    // Two frames: `ui.rs` drains the request into `Interaction` and `interaction_use_time` applies
    // it, then `Hud::drive` pushes the word into the screen on the next pass.
    app.frame();
    app.frame();
    assert_eq!(
        app.objects()
            .world
            .player_system
            .options
            .get(STAY_IN_CHAT_MODE),
        on,
        "the tick reached the option word"
    );
    let (_, screen) = gameplay(app);
    assert_eq!(
        screen.stay_in_chat_mode, on,
        "and the word reached `GamePlayScreen::stay_in_chat_mode`"
    );
}

/// Ticking the option keeps the focus in the entry after Enter; unticking releases it. Both
/// ways, driven from the option word.
///
/// The command path:
///
/// ```text
/// if stay-in-chat mode is disabled {
///     the entry relinquishes focus;
///     the entry deactivates;
///     the UI receives a chat-entry toggle-off notice;
/// }
/// process the command;           // either way
/// ```
///
/// The producer is one line in `Hud::drive`, beside `set_lock_ui` and `reply_targets`.
/// Command processing runs on both arms, so the line is sent and the box cleared either way; the
/// default-`false` path is asserted first.
///
/// Falsified by deleting the `screen.stay_in_chat_mode = ...` line in `Hud::drive`: the field
/// falls back to `false`, `tick_stay_in_chat_mode(app, true)` fails on its second assertion, and
/// the focus after Enter is `None` in both arms.
#[test]
fn the_option_word_decides_whether_enter_keeps_the_focus() {
    let mut app = app_in_gameplay(6);
    let entry = find(&app, ENTRY);
    let mut hand = Hand::new();
    let _ = app.ui_mut().expect("the UI shell is up").ui.requests.take();

    // ---- the default: false, and Enter releases. ----
    {
        let (_, screen) = gameplay(&mut app);
        assert!(
            !screen.stay_in_chat_mode,
            "the shipped default option word has bit 11 clear"
        );
    }
    assert_eq!(
        focus_of(&app),
        None,
        "nothing has focus before the chat key"
    );
    hand.tap(&mut app, KeyCode::Enter);
    assert_eq!(focus_of(&app), Some(entry), "Enter focused the chat entry");
    hand.character(&mut app, char::from(0x0D_u8));
    app.frame();
    type_line(&mut app, &mut hand, entry, "hail");
    hand.tap(&mut app, KeyCode::Enter);
    assert_eq!(
        focus_of(&app),
        None,
        "OFF: Enter relinquishes focus before command processing and nothing gives it back"
    );
    assert_eq!(
        text_of(&mut app, entry),
        "",
        "and command processing emptied the entry"
    );
    assert_eq!(
        history(&mut app),
        vec!["hail".to_owned()],
        "the command was accepted into chat history"
    );
    let _ = app.ui_mut().expect("the UI shell is up").ui.requests.take();

    // ---- tick it on, and the same gesture keeps the focus ------------------------------------
    tick_stay_in_chat_mode(&mut app, true);
    hand.tap(&mut app, KeyCode::Enter);
    assert_eq!(focus_of(&app), Some(entry), "the box is open again");
    hand.character(&mut app, char::from(0x0D_u8));
    app.frame();
    type_line(&mut app, &mut hand, entry, "again");
    hand.tap(&mut app, KeyCode::Enter);
    assert_eq!(
        focus_of(&app),
        Some(entry),
        "ON: stay-in-chat mode skips the relinquish block"
    );
    assert_eq!(text_of(&mut app, entry), "", "command processing still ran");
    assert_eq!(
        history(&mut app),
        vec!["hail".to_owned(), "again".to_owned()],
        "and the second command was accepted into chat history"
    );
    // The box is still live: the next character lands in it with no second Enter.
    let before = delivered(&app);
    hand.character(&mut app, 'q');
    app.frame();
    assert_eq!(
        delivered(&app),
        before + 1,
        "the entry is still taking characters"
    );
    assert_eq!(text_of(&mut app, entry), "q");
    let _ = app.ui_mut().expect("the UI shell is up").ui.requests.take();

    // ---- and untick, which must put it back ---------------------------------------------------
    tick_stay_in_chat_mode(&mut app, false);
    hand.tap(&mut app, KeyCode::Enter);
    assert_eq!(focus_of(&app), None, "OFF again: the release comes back");
    assert_eq!(text_of(&mut app, entry), "");
    assert_eq!(
        history(&mut app).len(),
        3,
        "and the third command was accepted into chat history"
    );
    app.shutdown();
}

// ---------------------------------------------------------------------------------------------
// The keyboard: constructed messages through the application's own message pump. No injected
// desktop input and no windowed run.
// ---------------------------------------------------------------------------------------------

struct Hand {
    pump: dereth_client::pump::Pump,
    time_ms: u32,
}

impl Hand {
    fn new() -> Self {
        let mut pump = dereth_client::pump::Pump::new();
        pump.state.is_ready = true;
        pump.state.is_active_app = true;
        Self {
            pump,
            time_ms: 400_000,
        }
    }

    fn send(&mut self, app: &mut App, m: dereth_client::pump::Win32Message) {
        self.pump.dispatch(m);
        if let Some(input) = app.input_manager_mut() {
            input.on_message(m);
        }
    }

    fn key(&mut self, app: &mut App, code: KeyCode, down: bool) {
        self.time_ms += 10;
        let m = self
            .pump
            .key_message_for(code, down, self.time_ms)
            .expect("winit maps this key to a virtual key and a scan code");
        self.send(app, m);
    }

    fn tap(&mut self, app: &mut App, code: KeyCode) {
        self.key(app, code, true);
        app.frame();
        self.key(app, code, false);
        app.frame();
    }

    fn character(&mut self, app: &mut App, ch: char) {
        self.time_ms += 10;
        let m = dereth_client::pump::Win32Message::new(
            dereth_input::win32::msg::WM_CHAR,
            ch as usize,
            0,
            self.time_ms,
        );
        self.send(app, m);
    }
}

fn delivered(app: &App) -> u64 {
    app.ui().expect("shell").stats.characters_delivered
}

fn text_of(app: &mut App, h: ElemHandle) -> String {
    app.ui_mut()
        .expect("shell")
        .ui
        .text_element_mut(h)
        .map_or_else(String::new, |t| t.glyphs.inq_text(false))
}

/// The main chat window's input history, appended when command processing accepts the line. It
/// proves the command/history seam consumed the entry; it does not by itself prove network delivery.
fn history(app: &mut App) -> Vec<String> {
    app.objects()
        .world
        .chat
        .entries
        .get(&8)
        .map_or_else(Vec::new, |entry| entry.history().to_vec())
}

/// Type a line into the focused entry, one character per frame, and check every one arrived.
fn type_line(app: &mut App, hand: &mut Hand, entry: ElemHandle, line: &str) {
    let before = delivered(app);
    for ch in line.chars() {
        hand.character(app, ch);
        app.frame();
    }
    assert_eq!(
        delivered(app),
        before + line.chars().count() as u64,
        "every character of {line:?} reached the input character path"
    );
    assert_eq!(
        text_of(app, entry),
        line,
        "and the box holds the whole line"
    );
}
