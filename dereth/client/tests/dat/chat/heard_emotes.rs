//! A heard emote (`0x01E0`) and a heard soul emote (`0x01E2`) are drawn on the main chat log as
//! `<name> <text>`, on the real `App`. Neither message carries a text type; both are drawn as
//! type `0xC`, whose colour-table slot is the grey `0xD2D2C8` (the colour the timestamp prefix
//! shares), so an emote is grey where a speech line beside it is not. The hearing gate runs
//! first and refuses a squelched speaker. The soul-emote arm discards the player's own soul emote
//! (the client already printed it locally when sending it) and otherwise marks the name with `^`,
//! which the common marker search trims straight back off, so both opcodes with the same body
//! draw the same glyphs.
//!
//! The senders here have no physics object in the App's object stream; the hearing gate accepts
//! such a speaker without a distance check, so the range half of the gate is not this file's
//! subject (the emote and hearing tests over `long-solo-play`'s recorded object stream cover it);
//! the squelch half is.
//!
//! Fixture: an opcode census of the recorded corpus, which carries each of the four emote
//! opcodes (`0x01DF`, `0x01E0`, `0x01E1`, `0x01E2`) in `requested-death-vitae-salvage`; the drawn
//! stations use synthesised messages handed to [`App::apply_hud_events`] on a headless App, and
//! the log is read as its glyph trace.

use crate::common::client_dir_or_workspace_client as client_dir;

use std::path::PathBuf;

use dereth_client::app::App;
use dereth_client::config::Config;
use dereth_client_model::chat::text_type;
use dereth_client_net::client_session::testing::Corpus;
use dereth_client_net::client_session::SessionEvent;
use dereth_primitives::ObjectId;
use dereth_protocol::{Message as _, Opcode};
use dereth_ui::framework::mode;
use dereth_ui::{ElemHandle, ElementId, UiSystem};
use dereth_ui_screens::chat::colors;
use dereth_ui_screens::chat::window::LOG;
use dereth_ui_screens::screens::gameplay::GamePlayScreen;

const OPAQUE: u32 = 0xFF00_0000;

/// The chat-colour table's grey override for slot 12, the emote text type.
const GREY: u32 = OPAQUE | 0x00D2_D2C8;

/// The fill every unoverridden slot keeps: the control that says the assertion above is reading
/// a table and not a constant.
const GREEN: u32 = OPAQUE | 0x0080_FF7F;

const LOCAL: ObjectId = ObjectId(0x5000_0001);
const SPEAKER: ObjectId = ObjectId(0x5000_0044);

// =================================================================================================
// 1. The recorded emotes
// =================================================================================================

fn u32_at(b: &[u8], off: usize) -> Option<u32> {
    Some(u32::from_le_bytes(b.get(off..off + 4)?.try_into().ok()?))
}

/// Count opcodes across **three** wrapper spaces, not one: the bare-blob space, the `0xF7B0` game
/// event space and the `0xF7B1` game action space. A census of one space returns a confident zero
/// when the subject lives in another, and these four opcodes span two of them —
/// `0x01DF`/`0x01E1` are outbound game actions, `0x01E0`/`0x01E2` inbound messages.
fn corpus_counts(wanted: &[u32]) -> Vec<(u32, usize)> {
    let mut counts: Vec<(u32, usize)> = wanted.iter().map(|w| (*w, 0)).collect();
    let mut calibration = 0usize;
    for corpus in Corpus::shared_all() {
        for blob in &corpus.blobs {
            let b = &blob.payload;
            let outer = u32_at(b, 0);
            // `0xF7B0` (an event) is `[opcode][iid][stamp][sub-type]`, so its sub-type is at
            // 12; `0xF7B1` (an action) is `[opcode][stamp][sub-type]`, so its sub-type is at 8.
            // Reading 12 for both reads the start of the body of every client action.
            let inner = match outer {
                Some(0xF7B0) => u32_at(b, 12),
                Some(0xF7B1) => u32_at(b, 8),
                _ => None,
            };
            for (op, n) in &mut counts {
                if outer == Some(*op) || inner == Some(*op) {
                    *n += 1;
                }
            }
            // `0x0015 Communication_Talk` — the same queue, the same wrapper and the same
            // one-packed-string body as `0x01DF`, and the corpus *does* carry it. Without it a
            // reader that silently found nothing would report the same four zeros.
            if outer == Some(0x0015) || inner == Some(0x0015) {
                calibration += 1;
            }
        }
    }
    assert!(
        calibration > 0,
        "the calibration found no 0x0015 either -- the reader is broken"
    );
    eprintln!("the 0x0015 Talk calibration is {calibration}");
    counts
}

/// The recorded corpus carries each of the four emote opcodes. This checks the opcode counts
/// only; it does not inspect bodies, timestamps, pairing or session names, and the rendering
/// stations below are synthesised.
#[test]
fn the_corpus_records_each_emote_request_and_answer() {
    let counts = corpus_counts(&[0x01DF, 0x01E0, 0x01E1, 0x01E2]);
    for (op, n) in &counts {
        assert!(
            *n > 0,
            "the corpus records the acted- and soul-emote request and answer opcodes; {op:#06X} \
             occurs {n} times in {counts:?}"
        );
    }
}

// =================================================================================================
// The application: a connected headless fixture
// =================================================================================================

/// A dedicated preferences path under the system temp directory, separate from the user's
/// `UserPreferences.ini`. This test does not assert that the parent directory is absent.
fn prefs_file() -> PathBuf {
    std::env::temp_dir().join("dereth-heard-emotes-not-created/prefs.ini")
}

fn connected_config() -> Config {
    Config {
        connect: true,
        account: "p1133".into(),
        host: "127.0.0.1".into(),
        port: 9000,
        ui: true,
        headless: true,
        sound: false,
        dat_dir: client_dir(),
        preferences_file: prefs_file(),
        ..Config::default()
    }
}

fn app_in_gameplay() -> App {
    let d = client_dir();
    assert!(
        dereth_dat::testing::have_dats(),
        "the retail dats are required at {} -- set DERETH_TEST_DAT_DIR",
        d.display()
    );
    let mut app =
        crate::common::sim_app::new(connected_config()).expect("the application comes up headless");
    app.start_shell().expect("the UI shell comes up");
    let s = dereth_client::world::SceneConfig {
        landblock: app.config().landblock,
        land_radius: app.config().land_radius,
        scenery_radius: app.config().scenery_radius,
        ..dereth_client::world::SceneConfig::default()
    };
    app.load_static_scene(s).expect("the static scene loads");
    // `0x0013 Login_PlayerDescription`: the chat windows take their stored filters off it, and
    // the default module's filter bag is absent, so each window keeps `0xFBFFFFFF`, whose bit 12
    // is set: the emote text type is accepted.
    app.apply_hud_events(&[SessionEvent::PlayerDescription(Box::default())]);
    // The current player id is what the soul-emote self-suppression check compares the sender
    // against.
    app.apply_hud_events(&[SessionEvent::PlayerCreated(LOCAL)]);
    app.queue_ui_mode(mode::GAME_PLAY);
    for _ in 0..8 {
        app.frame();
        if app.ui().and_then(|u| u.flow.current_mode()) == Some(mode::GAME_PLAY) {
            assert_eq!(
                app.hud().player,
                Some(LOCAL),
                "the premise: the client knows its own id"
            );
            return app;
        }
    }
    panic!(
        "the flow did not reach the gameplay screen: {:?}",
        app.ui().and_then(|u| u.flow.current_mode())
    );
}

fn gameplay(app: &mut App) -> (&mut UiSystem, &mut GamePlayScreen) {
    let shell = app.ui_mut().expect("shell");
    let ui = &mut shell.ui;
    let screen = shell.flow.current_mut().expect("a screen is up");
    let any: &mut dyn std::any::Any = &mut **screen;
    (
        ui,
        any.downcast_mut::<GamePlayScreen>()
            .expect("the gameplay screen"),
    )
}

fn find(app: &App, id: ElementId) -> ElemHandle {
    let shell = app.ui().expect("shell");
    let root = shell.flow.current().expect("a screen").roots()[0];
    shell
        .ui
        .get_child_recursive(root, id)
        .unwrap_or_else(|| panic!("{id:?} is in the shipped layout"))
}

/// The log element's glyph trace as adjacent `(text, colour)` runs. This reads shaped glyph data,
/// not framebuffer pixels. It follows the same trace method as the timestamp station.
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

/// The whole glyph trace as one string, and the colour of the first same-colour run containing
/// `needle`.
fn drawn(ui: &mut UiSystem, log: ElemHandle, needle: &str) -> (String, Option<u32>) {
    let all = runs(ui, log);
    let joined: String = all.iter().map(|(s, _)| s.as_str()).collect();
    let colour = all
        .iter()
        .find(|(s, _)| s.contains(needle))
        .map(|(_, c)| *c);
    (joined, colour)
}

/// One `0x01E0` or `0x01E2` body. The two messages have identical layouts: `dereth-protocol`
/// builds both from one macro, and the client reaches one composition from two handlers.
fn emote_event(opcode: Opcode, sender: ObjectId, name: &str, text: &str) -> SessionEvent {
    let m = dereth_protocol::comms::CommunicationHearEmote {
        sender,
        sender_name: name.to_owned(),
        text: text.to_owned(),
    };
    let mut w = dereth_protocol::archive::Writer::new();
    w.u32(opcode.0);
    m.write(&mut w).expect("the message encodes");
    SessionEvent::UiEvent {
        opcode,
        blob: w.into_inner(),
    }
}

/// Hand one message to the running application and let it reach the drawn log.
///
/// The emote arms push their line into the batch directly (the speech family's shape in this
/// client, not the scroll's), so the composed line is in **this** call's return; the `frame()`
/// then carries it into the gameplay chat log during the application's frame processing.
fn hear_on_the_log(
    app: &mut App,
    e: SessionEvent,
) -> Vec<dereth_ui_screens::chat::interface::ChatMessage> {
    let composed = app.apply_hud_events(&[e]);
    assert!(app.frame(), "the client is still running");
    composed
}

/// `(composed, squelched, out_of_earshot, self_echoes_discarded)` — the four counters the two arms
/// own. A line missing from the log cannot tell a refusal at the gate apart from a message that
/// never decoded; these can.
fn counts(app: &App) -> (u64, u64, u64, u64) {
    let s = &app.hud().stats;
    (
        s.emote_lines_composed,
        s.emote_lines_squelched,
        s.emote_lines_out_of_earshot,
        s.soul_emote_self_echoes_discarded,
    )
}

// =================================================================================================
// 2. The drawn line, for both opcodes
// =================================================================================================

/// Behaviour: chat.emote.is-drawn-as-name-then-text
///
/// An emote heard from another player reaches the shipped chat log's glyph trace as
/// `"<name> <text>"`, in the emote text type's grey.
///
/// Both opcodes run through the same application with the same body. Each iteration checks its
/// newly composed message and then searches the accumulated glyph trace, so the second glyph
/// lookup can also see the first line. The final equality compares the two constructed expected
/// strings rather than two independently captured traces. The caret check still covers the whole
/// trace: the soul-emote marker must not survive composition.
#[test]
fn both_kinds_of_heard_emote_draw_the_same_grey_line() {
    let mut app = app_in_gameplay();
    let log = find(&app, LOG);
    let mut drawn_for: Vec<(&str, String)> = Vec::new();

    for (what, opcode, text) in [
        (
            "0x01E0 acted",
            Opcode::COMMUNICATION_HEAR_EMOTE,
            "waves at p1133.",
        ),
        (
            "0x01E2 soul",
            Opcode::COMMUNICATION_HEAR_SOUL_EMOTE,
            "waves at p1133.",
        ),
    ] {
        let before = counts(&app);
        let composed = hear_on_the_log(&mut app, emote_event(opcode, SPEAKER, "Alba", text));
        let after = counts(&app);
        assert_eq!(
            (after.0 - before.0, after.1 - before.1, after.2 - before.2),
            (1, 0, 0),
            "{what}: composed, not squelched, not out of earshot"
        );

        let want = format!("Alba {text}");
        let line = composed
            .iter()
            .find(|m| m.body == want)
            .unwrap_or_else(|| panic!("{what}: no chat line was {want:?}; got {composed:?}"));
        assert_eq!(
            u32::from(line.ty),
            text_type::EMOTE,
            "{what}: type 0xC is supplied by the heard-emote path; neither message carries a type"
        );
        assert_eq!(line.window, 0, "{what}: the source window is zero");

        let (ui, _) = gameplay(&mut app);
        let (joined, colour) = drawn(ui, log, &want);
        assert!(
            joined.contains(&want),
            "{what}: the log draws {want:?}; it has {joined:?}"
        );
        assert!(
            !joined.contains('^'),
            "{what}: the soul-emote marker is trimmed before composition and must never be \
             drawn. Got {joined:?}"
        );
        assert_eq!(
            colour,
            Some(GREY),
            "{what}: the emote text type uses the grey table value"
        );
        assert_eq!(
            Some(GREY),
            Some(OPAQUE | colors::color_for_type(12).hex),
            "{what}: and that grey is the table's own answer for slot 12, not a literal"
        );
        assert_ne!(
            GREY, GREEN,
            "the table really does override slot 12 off the green fill"
        );
        drawn_for.push((what, want));
    }

    assert_eq!(
        drawn_for[0].1, drawn_for[1].1,
        "0x01E0 and 0x01E2 with the same body construct the same expected line after the \
         soul-emote marker is removed"
    );
    eprintln!(
        "both opcode iterations passed composition and grey glyph-trace checks for Alba waves at p1133."
    );
}

/// An emote is grey where a speech line beside it is not, on the same log in the same frame.
///
/// The control the colour assertion above needs: a table that answered one colour for everything
/// would pass every row of it. `0x02BB Communication_HearSpeech` at speech type (2) is the
/// table's own separate override.
#[test]
fn an_emote_is_not_drawn_in_the_colour_of_the_speech_line_beside_it() {
    let mut app = app_in_gameplay();
    let log = find(&app, LOG);

    hear_on_the_log(
        &mut app,
        emote_event(
            Opcode::COMMUNICATION_HEAR_EMOTE,
            SPEAKER,
            "Alba",
            "shrugs at p1133.",
        ),
    );

    let m = dereth_protocol::comms::CommunicationHearSpeech {
        message: "p1133 spoken".to_owned(),
        sender_name: "Alba".to_owned(),
        sender_id: SPEAKER,
        text_type: text_type::SPEECH,
    };
    let mut w = dereth_protocol::archive::Writer::new();
    w.u32(Opcode::COMMUNICATION_HEAR_SPEECH.0);
    m.write(&mut w).expect("the message encodes");
    hear_on_the_log(
        &mut app,
        SessionEvent::UiEvent {
            opcode: Opcode::COMMUNICATION_HEAR_SPEECH,
            blob: w.into_inner(),
        },
    );

    let (ui, _) = gameplay(&mut app);
    let emote = drawn(ui, log, "Alba shrugs at p1133.")
        .1
        .expect("the emote line is drawn");
    let speech = drawn(ui, log, "p1133 spoken")
        .1
        .expect("the speech line is drawn");
    assert_eq!(emote, GREY, "slot 12 uses the grey table value");
    assert_eq!(
        speech,
        OPAQUE | 0x00FF_FFFF,
        "slot 2 uses the white table value"
    );
    assert_ne!(emote, speech, "two types, two colours, one log");
}

// =================================================================================================
// 3. The hearing gate
// =================================================================================================

/// The hearing gate runs first, and it refuses a squelched speaker for text type `0xC`.
///
/// Both opcodes, because the soul-emote arm reaches the gate only through its common tail and a build
/// that lost that call would still pass the `0x01E0` half. The counters separate the two refusals
/// the hearing gate can make — squelched and out of earshot — so a line missing for the *wrong* reason
/// fails here.
#[test]
fn a_squelched_speakers_emote_never_reaches_the_log() {
    let mut app = app_in_gameplay();
    let log = find(&app, LOG);

    // The control first: unsquelched, the same speaker is heard. Without it, a station that
    // muted everything could pass because the emote never arrived at all.
    hear_on_the_log(
        &mut app,
        emote_event(
            Opcode::COMMUNICATION_HEAR_EMOTE,
            SPEAKER,
            "Alba",
            "waves before the squelch.",
        ),
    );
    {
        let (ui, _) = gameplay(&mut app);
        let (joined, _) = drawn(ui, log, "waves before the squelch.");
        assert!(
            joined.contains("Alba waves before the squelch."),
            "the control: {joined:?}"
        );
    }

    // The hearing gate asks the per-character squelch entry for the sender id; text type `0xC`
    // is a legal channel for that lookup.
    {
        let squelch = &mut app.probe_mut().objects_mut().world.chat.squelch;
        let mut entry = dereth_client_model::chat::SquelchEntry {
            name: "Alba".to_owned(),
            ..Default::default()
        };
        entry.squelch_everything();
        squelch.characters.insert(SPEAKER, entry);
    }

    for (what, opcode) in [
        ("0x01E0 acted", Opcode::COMMUNICATION_HEAR_EMOTE),
        ("0x01E2 soul", Opcode::COMMUNICATION_HEAR_SOUL_EMOTE),
    ] {
        let needle = format!("p1133 squelched {what}");
        let before = counts(&app);
        let composed = hear_on_the_log(&mut app, emote_event(opcode, SPEAKER, "Alba", &needle));
        let after = counts(&app);
        assert_eq!(
            (after.0 - before.0, after.1 - before.1, after.2 - before.2),
            (0, 1, 0),
            "{what}: refused at the squelch, and not miscounted as out of earshot"
        );
        assert!(
            composed.iter().all(|m| !m.body.contains(&needle)),
            "{what}: the gate runs before the composition -- nothing was composed"
        );
        let (ui, _) = gameplay(&mut app);
        let (joined, _) = drawn(ui, log, &needle);
        assert!(
            !joined.contains(&needle),
            "{what}: and nothing was drawn. Got {joined:?}"
        );
    }
}

/// Behaviour: chat.soul-emote.your-own-is-not-echoed-back
///
/// Your own soul emote is discarded and your own acted emote is not: the asymmetry that is the
/// whole of the soul-emote arm before it reaches the common composition path.
///
/// Soul-emote self suppression avoids repeating the pose the client printed locally when it sent
/// `0x01E1` (`"You wave."`). Acted emotes have no such local echo and remain eligible to be drawn
/// even when they name the current player.
#[test]
fn your_own_soul_emote_is_discarded_and_your_own_acted_emote_is_drawn() {
    let mut app = app_in_gameplay();
    let log = find(&app, LOG);

    let before = counts(&app);
    hear_on_the_log(
        &mut app,
        emote_event(
            Opcode::COMMUNICATION_HEAR_SOUL_EMOTE,
            LOCAL,
            "p1133",
            "p1133 own soul emote",
        ),
    );
    let after = counts(&app);
    assert_eq!(
        (after.0 - before.0, after.3 - before.3),
        (0, 1),
        "the self soul emote returns before common composition: nothing composed"
    );
    {
        let (ui, _) = gameplay(&mut app);
        let (joined, _) = drawn(ui, log, "p1133 own soul emote");
        assert!(
            !joined.contains("p1133 own soul emote"),
            "drawn nowhere. Got {joined:?}"
        );
    }

    let before = counts(&app);
    hear_on_the_log(
        &mut app,
        emote_event(
            Opcode::COMMUNICATION_HEAR_EMOTE,
            LOCAL,
            "p1133",
            "p1133 own acted emote",
        ),
    );
    let after = counts(&app);
    assert_eq!(
        (after.0 - before.0, after.3 - before.3),
        (1, 0),
        "the acted-emote arm has no self-suppression check -- your own acted emote is composed"
    );
    let (ui, _) = gameplay(&mut app);
    let (joined, colour) = drawn(ui, log, "p1133 own acted emote");
    assert!(
        joined.contains("p1133 p1133 own acted emote"),
        "and drawn: {joined:?}"
    );
    assert_eq!(colour, Some(GREY));
}
