//! The chat window's lines as the player's scripts write them, from a recorded 2017 session.

use std::net::SocketAddr;
use std::sync::Arc;

use auberean_client_contract::Action;
use auberean_client_net::LocalTime;
use auberean_client_runtime::data::{DataFiles, ENGLISH};
use auberean_client_runtime::shell::ShellStatus;
use auberean_client_runtime::{link_scripts, Client, ClientData, ClientSettings};
use auberean_dat::locate::{install_dats, DatName};
use auberean_dat::testing::require_target_install;
use auberean_dat::DatFile;
use auberean_protocol::{ClientMessage, ServerMessage};
use auberean_testkit::captures;

/// The client's entering the world.
const PLAY_CHARACTER: u32 = 0x1002B;

/// A 2017 run of the client in which another player, a woman whose name the server sends as
/// "Tysis [F]", speaks near the recording player.
const SESSION: &str = "478cf1fc123fa081:6";

fn client() -> Client {
    client_and_program(false).0
}

/// A client on the target install's scripts (recording what its scripts send when asked), and
/// the scripts' program.
fn client_and_program(script_sends: bool) -> (Client, Arc<auberean_script::Program>) {
    let dir = require_target_install();
    let found = install_dats(&dir).expect("the install's files");
    let (_, p) = found
        .iter()
        .find(|(n, _)| *n == DatName::Portal)
        .expect("the portal file");
    let portal = Arc::new(DatFile::open(p).expect("the portal file opens"));
    let (_, l) = found
        .iter()
        .find(|(n, _)| *n == DatName::Local("English".into()))
        .expect("the English file");
    let english = Arc::new(DatFile::open(l).expect("the English file opens"));
    let program = Arc::new(link_scripts(&portal).expect("the library links"));
    let data = ClientData {
        files: DataFiles {
            files: vec![Arc::clone(&portal), english],
            language: ENGLISH,
        },
        scripts: Some((portal, Arc::clone(&program))),
    };
    let settings = ClientSettings {
        login_server: SocketAddr::from(([127, 0, 0, 1], 9000)),
        account: Vec::new(),
        ticket: Vec::new(),
        timestamp: 0,
        session_key: [0; 64],
        script_sends,
    };
    (Client::new(settings, data), program)
}

/// A woman's saying reaches the chat window as her scripts write it: her name without the gender
/// mark the server sends with it ("Tysis [F]" reads "Tysis"), "says," and her words in quotes, as
/// a line of the spoken kind. The mark only chooses words in the line (her, she); it is never
/// shown.
///
/// Behaviour: script.chat.a-saying-is-written-by-the-scripts-without-the-gender-mark
#[test]
fn a_saying_is_written_by_the_scripts_without_the_gender_mark() {
    let index = auberean_testkit::data::require_capture_index();
    let mut types: Vec<u32> = (0x1_0000..0x1_0100)
        .filter(|t| ServerMessage::is_known_type(*t))
        .collect();
    types.push(PLAY_CHARACTER);
    let mut c = client();
    let mut frame = f64::NEG_INFINITY;
    let mut lines = Vec::new();
    captures::capture_messages_in_time_order(&index, &types, SESSION, |m| {
        if m.time > frame {
            frame = m.time;
            c.set_time(LocalTime(m.time));
            c.upkeep();
        }
        if !m.to_client {
            if let Ok(ClientMessage::PlayCharacter(p)) = ClientMessage::decode(&m.raw) {
                c.act(Action::PlayCharacter {
                    character: p.character,
                });
            }
            return;
        }
        if let Ok(msg) = ServerMessage::decode(&m.raw) {
            c.take_message(m.blob_id, msg);
        }
        for s in std::mem::take(&mut c.shown) {
            let text = c.write_text(&s.text).expect("the scripts run");
            lines.push((s.about, s.kind, text));
        }
    })
    .expect("the index reads");
    let hers: Vec<&(Option<u64>, u32, String)> = lines
        .iter()
        .filter(|(_, _, t)| t.starts_with("Tysis"))
        .collect();
    assert!(!hers.is_empty(), "her lines are shown: {lines:?}");
    eprintln!("{hers:?}");
    for (_, kind, text) in &hers {
        assert!(
            text.starts_with("Tysis says, \""),
            "a saying, by name: {text}"
        );
        assert!(!text.contains('['), "no gender mark: {text}");
        assert_eq!(*kind, 16, "the spoken kind");
    }
}

/// The words a send carries: the string references among its arguments, as their literal text.
fn words_sent(args: &[u8], program: &auberean_script::Program) -> Vec<String> {
    use auberean_script::args::Arg;
    use auberean_script::graph::{Body, PackedObject};
    use auberean_script::native_object::NativeObject;
    let Ok(values) = auberean_script::args::decode(args, &program.library) else {
        return Vec::new();
    };
    values
        .iter()
        .filter_map(|a| match a {
            Arg::Object(g) => Some(g.entries.iter().filter_map(|e| match &e.body {
                Body::Object(PackedObject {
                    native: Some(NativeObject::TextInfo(t)),
                    ..
                }) => t.literal.as_deref().map(String::from_utf16_lossy),
                _ => None,
            })),
            _ => None,
        })
        .flatten()
        .collect()
}

/// The client through the recorded session to its end, in the world, recording what its scripts
/// send, its commands made known as the chat panel makes them known when it is made.
fn in_the_world() -> (Client, Arc<auberean_script::Program>) {
    let index = auberean_testkit::data::require_capture_index();
    let mut types: Vec<u32> = (0x1_0000..0x1_0100)
        .filter(|t| ServerMessage::is_known_type(*t))
        .collect();
    types.push(PLAY_CHARACTER);
    let (mut c, program) = client_and_program(true);
    let mut frame = f64::NEG_INFINITY;
    captures::capture_messages_in_time_order(&index, &types, SESSION, |m| {
        if m.time > frame {
            frame = m.time;
            c.set_time(LocalTime(m.time));
            c.upkeep();
        }
        if !m.to_client {
            if let Ok(ClientMessage::PlayCharacter(p)) = ClientMessage::decode(&m.raw) {
                c.act(Action::PlayCharacter {
                    character: p.character,
                });
            }
            return;
        }
        if let Ok(msg) = ServerMessage::decode(&m.raw) {
            c.take_message(m.blob_id, msg);
        }
        c.shown.clear();
    })
    .expect("the index reads");
    assert!(c.model.player_object().is_some(), "in the world");
    c.know_commands();
    (c, program)
}

/// The words of the sends since `before`, and the lines shown since they were last taken.
fn said_since(
    c: &mut Client,
    before: usize,
    program: &auberean_script::Program,
) -> (Vec<Vec<String>>, Vec<String>) {
    let sends = c.sent()[before..]
        .iter()
        .filter_map(|s| match &s.message {
            ClientMessage::ServerCall(sc) => Some(words_sent(&sc.args.0, program)),
            _ => None,
        })
        .collect();
    let shown = std::mem::take(&mut c.shown)
        .iter()
        .filter_map(|s| c.write_text(&s.text))
        .collect();
    (sends, shown)
}

/// An alias made with `/alias`, then typed in a line after a `;`, is said as what it stands for;
/// what it stands for is expanded in turn, an alias already being expanded stays as typed, and
/// the search goes on after each expansion; a predefined alias cannot be made; a line held by a
/// toolbar slot is said when the slot is used, as if typed; an alias removed is said as typed.
///
/// Behaviour: script.chat.an-alias-in-a-line-is-said-as-what-it-stands-for
#[test]
fn an_alias_in_a_line_is_said_as_what_it_stands_for_and_a_slots_line_as_if_typed() {
    use auberean_client_contract::ToolbarShortcut;
    use auberean_script::options::kind;
    /// The server function an alias is sent to.
    const SET_ALIAS: u32 = 0x8212_0011;
    let (mut c, program) = in_the_world();
    let say = |c: &mut Client, typed: &str| {
        let before = c.sent().len();
        let status = c.process_text(typed);
        let functions: Vec<u32> = c.sent()[before..]
            .iter()
            .filter_map(|s| match &s.message {
                ClientMessage::ServerCall(sc) => Some(sc.function.0),
                _ => None,
            })
            .collect();
        let (sends, shown) = said_since(c, before, &program);
        (status, functions, sends, shown)
    };
    for line in ["/alias ;a ;b x", "/alias ;b ;a y"] {
        let (status, functions, _, shown) = say(&mut c, line);
        assert_eq!(status, ShellStatus::Success, "{line}");
        assert_eq!(functions, [SET_ALIAS], "{line}: {shown:?}");
    }
    let (_, functions, _, shown) = say(&mut c, "/alias ;wm Well met, stranger");
    assert_eq!(functions, [SET_ALIAS]);
    assert!(
        shown
            .iter()
            .any(|l| l.contains("[;wm]") && l.contains("[Well met, stranger]")),
        "{shown:?}"
    );
    for (typed, said) in [
        (";wm", "Well met, stranger"),
        (
            "I say ;wm and ;nothing",
            "I say Well met, stranger and ;nothing",
        ),
        (";wm;wm ok", "Well met, strangerWell met, stranger ok"),
        (";a", ";a y x"),
    ] {
        let (status, _, sends, shown) = say(&mut c, typed);
        assert_eq!(status, ShellStatus::Success, "{typed}");
        assert_eq!(sends, [vec![said.to_owned()]], "{typed:?}: {shown:?}");
    }
    // The predefined aliases are kept in a table keyed by the strings themselves.
    let (_, functions, _, shown) = say(&mut c, "/alias ;loc mine");
    assert!(functions.is_empty(), "{functions:?}");
    assert!(shown.iter().any(|l| l.contains("predefined")), "{shown:?}");
    assert_eq!(
        c.shell.lines().get(1).map(String::as_str),
        Some(";a"),
        "the line kept is the line as typed"
    );
    // A toolbar slot holding a line of text.
    c.act(Action::SetToolbarSlot {
        slot: 0,
        shortcut: ToolbarShortcut {
            kind: kind::TEXT,
            text: "/say ;wm, from a slot".encode_utf16().collect(),
            ..ToolbarShortcut::default()
        },
    });
    let before = c.sent().len();
    c.act(Action::UseToolbarSlot { slot: 0 });
    let (sends, shown) = said_since(&mut c, before, &program);
    assert_eq!(
        sends,
        [vec!["Well met, stranger, from a slot".to_owned()]],
        "{shown:?}"
    );
    let (_, functions, _, _) = say(&mut c, "/alias remove ;wm");
    assert_eq!(functions, [SET_ALIAS]);
    let (_, _, sends, _) = say(&mut c, ";wm");
    assert_eq!(sends, [vec![";wm".to_owned()]]);
}

/// A line typed (as the chat panel hands it on, and as the line-typed action gives it) is carried
/// out by the player's command line: a line that is no command is said whole, one word or more,
/// and `/say` says the rest of its line, the command cut off. Each goes to the server as one
/// saying carrying those words; none is answered with the say command's usage line.
///
/// Behaviour: script.chat.a-typed-line-is-said-and-a-say-command-says-the-rest-of-its-line
#[test]
fn a_typed_line_is_said_and_a_say_command_says_the_rest_of_its_line() {
    let (mut c, program) = in_the_world();
    let cases = [
        ("hello", "hello"),
        ("/say message", "message"),
        ("Well met, stranger", "Well met, stranger"),
    ];
    for (through_action, (typed, said)) in [(false, true), (true, false)]
        .into_iter()
        .flat_map(|(a, _)| cases.iter().map(move |c| (a, *c)))
    {
        let before = c.sent().len();
        if through_action {
            c.act(Action::Say { text: typed.into() });
        } else {
            c.process_text(typed);
        }
        let sends: Vec<Vec<String>> = c.sent()[before..]
            .iter()
            .filter_map(|s| match &s.message {
                ClientMessage::ServerCall(sc) => Some(words_sent(&sc.args.0, &program)),
                _ => None,
            })
            .collect();
        let shown: Vec<String> = std::mem::take(&mut c.shown)
            .iter()
            .filter_map(|s| c.write_text(&s.text))
            .collect();
        assert_eq!(
            sends,
            [vec![said.to_owned()]],
            "{typed:?} (the action: {through_action}) is said as {said:?}; shown {shown:?}"
        );
        assert!(
            shown.iter().all(|l| !l.contains("usage")),
            "{typed:?}: no usage line: {shown:?}"
        );
    }
}

/// A saying (on the speaker's object) and a tell (to the player), as the servers call them.
const SAYING: u32 = 0x8205_0016;
const HEAR_TELL: u32 = 0x8205_0017;

/// The string references among a call's arguments, in order.
fn texts(
    args: &[u8],
    program: &auberean_script::Program,
) -> Vec<auberean_script::native_object::TextInfo> {
    use auberean_script::args::Arg;
    use auberean_script::graph::{Body, PackedObject};
    use auberean_script::native_object::NativeObject;
    let Ok(values) = auberean_script::args::decode(args, &program.library) else {
        return Vec::new();
    };
    values
        .iter()
        .filter_map(|a| match a {
            Arg::Object(g) => g.entries.iter().find_map(|e| match &e.body {
                Body::Object(PackedObject {
                    native: Some(NativeObject::TextInfo(t)),
                    ..
                }) => Some(t.clone()),
                _ => None,
            }),
            _ => None,
        })
        .collect()
}

/// A name as the chat shows it: its marks after the name (`[F]`, `[Mv]`) cut off.
fn bare(name: &str) -> String {
    let units: Vec<u16> = name.encode_utf16().collect();
    String::from_utf16_lossy(&auberean_text::meta::strip_meta_letters(&units))
        .trim()
        .to_owned()
}

/// What the replay counted.
#[derive(Debug, Default)]
struct Counts {
    /// Sayings and tells recorded; of those, by a speaker the client did not know then.
    calls: usize,
    unknown_speaker: usize,
    /// Calls a later line of their kind holds the words of: with the speaker's bare name (or the
    /// player's own, or a speaker named by its record: the words alone); without the name; calls
    /// no such line followed.
    written: usize,
    wrong: usize,
    missing: usize,
    /// Lines of the spoken or tell kind showing a name's mark.
    marked: usize,
}

/// One capture replayed: each saying's and tell's words and speaker, as written, matched against
/// the first line of its kind after it that holds the words.
fn replay(index: &std::path::Path, capture: i64, types: &[u32], n: &mut Counts) {
    let mut c = client();
    let program = c
        .scripts
        .as_ref()
        .map(|s| std::sync::Arc::clone(&s.host.program))
        .expect("the scripts");
    let mut frame = f64::NEG_INFINITY;
    let mut lines: Vec<(u32, String)> = Vec::new();
    let mut expected: Vec<(usize, u32, String, Option<String>)> = Vec::new();
    captures::capture_by_id_in_time_order(index, types, capture, |m| {
        if m.time > frame {
            frame = m.time;
            c.set_time(LocalTime(m.time));
            c.upkeep();
        }
        if !m.to_client {
            if let Ok(ClientMessage::PlayCharacter(p)) = ClientMessage::decode(&m.raw) {
                c.act(Action::PlayCharacter {
                    character: p.character,
                });
            }
        } else {
            if let Some(s) = m
                .script_call()
                .filter(|s| s.function == SAYING || s.function == HEAR_TELL)
            {
                n.calls += 1;
                let t = texts(&s.args, &program);
                let written: Vec<String> = t.iter().filter_map(|t| c.write_text(t)).collect();
                // The speaker's name to look for: none for the player's own sayings ("You say")
                // or a speaker named by its qualities record rather than a literal.
                let (known, who) = if s.function == SAYING {
                    let o = s.object.and_then(|o| c.model.object(o));
                    let here = o.is_some();
                    let mine = s.object.is_some() && s.object == c.model.player;
                    let name = o.filter(|o| o.weenie.name.has_literal != 0).map(|o| {
                        auberean_script::native_object::TextInfo {
                            has_literal: 1,
                            literal: o.weenie.name.literal.clone().map(|l| l.0),
                            ..Default::default()
                        }
                    });
                    let who = if mine {
                        None
                    } else {
                        name.and_then(|t| c.write_text(&t))
                    };
                    (here, who)
                } else {
                    (true, written.get(1).cloned())
                };
                match written.first() {
                    Some(words) if known => {
                        let kind = if s.function == SAYING { 16 } else { 32 };
                        expected.push((lines.len(), kind, words.clone(), who.map(|w| bare(&w))));
                    }
                    _ => n.unknown_speaker += 1,
                }
            }
            if let Ok(msg) = ServerMessage::decode(&m.raw) {
                c.take_message(m.blob_id, msg);
            }
        }
        for s in std::mem::take(&mut c.shown) {
            if let Some(t) = c.write_text(&s.text) {
                lines.push((s.kind, t));
            }
        }
    })
    .expect("the capture reads");
    // The spoken and tell kinds' lines (a room's line, of its channel's kind, holds the
    // channel's name in brackets and whatever its speaker typed).
    n.marked += lines
        .iter()
        .filter(|(k, t)| (*k == 16 || *k == 32) && (t.contains("[F") || t.contains("[M")))
        .count();
    let mut used = vec![false; lines.len()];
    for (at, kind, words, who) in expected {
        let found = (at..lines.len())
            .find(|i| !used[*i] && lines[*i].0 == kind && lines[*i].1.contains(words.as_str()));
        match found {
            Some(i) => {
                used[i] = true;
                if who.as_ref().is_none_or(|w| lines[i].1.contains(w.as_str())) {
                    n.written += 1;
                } else {
                    n.wrong += 1;
                }
            }
            None => n.missing += 1,
        }
    }
}

/// Every saying and tell the 2005, 2013 and 2017 captures recorded, run through the player's
/// scripts as the client takes the recorded messages, reaches the chat windows as a line of the
/// spoken kind (a saying) or the tell kind holding the words as written and the speaker's name
/// without its gender and vowel marks (the player's own as "You"): no such line shows a mark, at
/// most one in a hundred misses the name, and at least nine in ten of the sayings and tells by a
/// speaker the client knew are written (the 2026-10-28 count: 2,716 of 2,976, 1 without the
/// name). (Names and words are never printed: only the counts.)
///
/// Behaviour: script.chat.every-recorded-saying-and-tell-is-written-with-the-words-and-a-bare-name
#[test]
fn every_recorded_saying_and_tell_is_written_with_the_words_and_a_bare_name() {
    use auberean_testkit::captures::carrier;
    let index = auberean_testkit::data::require_capture_index();
    let mut chatty = std::collections::BTreeSet::new();
    captures::messages(&index, &carrier::ALL[1..], &[2005, 2013, 2017], |m| {
        if m.to_client
            && m.script_call()
                .is_some_and(|c| c.function == SAYING || c.function == HEAR_TELL)
        {
            chatty.insert(m.capture);
        }
    })
    .expect("the index reads");
    let mut types: Vec<u32> = (0x1_0000..0x1_0100)
        .filter(|t| ServerMessage::is_known_type(*t))
        .collect();
    types.push(PLAY_CHARACTER);
    let mut n = Counts::default();
    for capture in &chatty {
        replay(&index, *capture, &types, &mut n);
    }
    eprintln!("captures {}, {n:?}", chatty.len());
    assert!(n.calls > 3000, "the corpus's sayings and tells: {n:?}");
    assert_eq!(n.marked, 0, "no line shows a name's mark");
    let matched = n.calls - n.unknown_speaker;
    assert!(
        n.wrong * 100 <= matched,
        "a line with the words names its speaker: {n:?}"
    );
    // The rest the player's scripts did not write at all (a native they reach not written yet):
    // no line anywhere holds their words.
    assert!(
        n.written * 100 >= matched * 90,
        "at least 90 in 100 written: {n:?}"
    );
}

/// A server's call that sleeps (entering portal space after a delay) leaves the player's call
/// waiting: nothing more happens before the delay has passed on the client's clock, and at the
/// first upkeep after it the call goes on to its end (the portal space notice to the panels).
///
/// Behaviour: script.model.a-sleeping-call-goes-on-when-its-time-comes
#[test]
fn a_sleeping_call_goes_on_at_the_first_upkeep_after_its_time() {
    use auberean_protocol::script::{ClientCall, ScriptArgs};
    use auberean_script::value::FunctionId;
    /// The server's call that enters portal space after a delay, and the panels' notice of it.
    const ENTER_PORTAL_SPACE: u32 = 0x8212_0020;
    const PORTAL_SPACE: u32 = (0x91 << 16) | 0x01;
    let (mut c, _) = in_the_world();
    // Well after the session's last instant.
    let start = 1.0e7;
    c.set_time(LocalTime(start));
    let call = ServerMessage::PlayerCall(ClientCall {
        function: FunctionId(ENTER_PORTAL_SPACE),
        args: Some(ScriptArgs(
            auberean_script::args::encode(&[auberean_script::args::Arg::LongFloat(
                2.0f64.to_bits(),
            )])
            .expect("packs"),
        )),
    });
    c.notices.clear();
    c.take_message(3 << 56, call);
    let notices = |c: &Client| {
        c.notices
            .iter()
            .filter(|n| n.native == PORTAL_SPACE)
            .count()
    };
    assert_eq!(notices(&c), 0, "{:?}", c.calls);
    assert_eq!(c.waiting.len(), 1, "the call sleeps: {:?}", c.calls);
    for (after, woken) in [(1.0, 0), (1.99, 0), (2.5, 1)] {
        c.set_time(LocalTime(start + after));
        c.upkeep();
        assert_eq!(notices(&c), woken, "after {after} s");
    }
    assert_eq!((c.woken.calls, c.woken.completed), (1, 1), "{:?}", c.woken);
    assert!(c.waiting.is_empty());
}

/// The message the client sends a chat channel's line in, through the chat server.
const CHAT_SERVER_DATA: u32 = 0x100D3;
/// The client's own report of where its player is.
const CLIENT_POSITION: u32 = 0x1008E;

/// Map coordinates as the client writes them (`12.3N, 45.6W`) when they are the whole text of a
/// recorded channel line (its length, a byte, just before it).
fn recorded_coordinates(raw: &[u8]) -> Option<String> {
    for skew in 0..2 {
        let units: Vec<u16> = raw[skew..]
            .as_chunks::<2>()
            .0
            .iter()
            .map(|b| u16::from_le_bytes(*b))
            .collect();
        let text = String::from_utf16_lossy(&units);
        let chars: Vec<char> = text.chars().collect();
        // Find `<n>.<d>[NS], <n>.<d>[EW]`, with its length just before it.
        for at in 0..chars.len() {
            let rest: String = chars[at..].iter().collect();
            if let Some(found) = coordinates_at(&rest) {
                // The length byte is the high byte of the unit before the text.
                let before = chars.get(at.wrapping_sub(1)).copied().unwrap_or('\0');
                if u32::from(before) >> 8 == u32::try_from(found.chars().count()).unwrap_or(0) {
                    return Some(found);
                }
            }
        }
    }
    None
}

/// The map coordinates a string starts with, if it starts with some.
fn coordinates_at(s: &str) -> Option<String> {
    let number = |s: &str| -> Option<usize> {
        let digits = s.chars().take_while(char::is_ascii_digit).count();
        let rest = &s[digits..];
        (digits > 0 && rest.starts_with('.') && rest[1..].starts_with(|c: char| c.is_ascii_digit()))
            .then_some(digits + 2)
    };
    let a = number(s)?;
    let ns = s[a..].chars().next().filter(|c| matches!(c, 'N' | 'S'))?;
    let rest = s[a + 1..].strip_prefix(", ")?;
    let b = number(rest)?;
    let ew = rest[b..]
        .chars()
        .next()
        .filter(|c| matches!(c, 'E' | 'W'))?;
    Some(format!("{}{ns}, {}{ew}", &s[..a], &rest[..b]))
}

/// The predefined alias `;loc`, typed, is said as the player's map coordinates, and `/loc` shows
/// the place line with the same coordinates. Typed where a recorded 2017 client last reported its
/// player, at each channel line whose whole text was coordinates (5 lines in 3 runs), it says what
/// the recorded client wrote, to the tenth, 4 times; the fifth line names a place 7 east of where
/// its player was.
///
/// Behaviour: script.chat.the-loc-alias-says-the-players-map-coordinates
#[test]
fn the_loc_alias_says_the_players_map_coordinates_as_the_recorded_clients_wrote_them() {
    let index = auberean_testkit::data::require_capture_index();
    // The runs holding such lines, and the lines' times.
    let mut runs: Vec<(String, Vec<(f64, String)>)> = Vec::new();
    let con = captures::open(&index).expect("the index opens");
    let mut keys: Vec<(String, i64)> = Vec::new();
    {
        let mut stmt = con
            .prepare(
                "SELECT s.session_key, s.capture_id FROM session s JOIN capture c USING (capture_id) \
                 WHERE c.file LIKE '2017-%' AND s.login = 1 ORDER BY s.capture_id",
            )
            .expect("the query");
        let rows = stmt
            .query_map([], |r| Ok((r.get::<_, String>(0)?, r.get::<_, i64>(1)?)))
            .expect("the rows");
        for r in rows {
            keys.push(r.expect("a row"));
        }
    }
    let _ = captures::messages_in_time_order(&index, &[CHAT_SERVER_DATA], &[2017], |m| {
        if m.to_client {
            return;
        }
        let Some(text) = recorded_coordinates(&m.raw) else {
            return;
        };
        let Some((key, _)) = keys.iter().find(|(_, c)| *c == m.capture) else {
            return;
        };
        match runs.iter_mut().find(|(k, _)| k == key) {
            Some((_, lines)) => lines.push((m.time, text)),
            None => runs.push((key.clone(), vec![(m.time, text)])),
        }
    })
    .expect("the index reads");
    let lines: usize = runs.iter().map(|(_, l)| l.len()).sum();
    eprintln!("recorded coordinate lines: {lines} in {} runs", runs.len());
    assert!(lines >= 5 && runs.len() >= 3, "{runs:?}");
    let mut types: Vec<u32> = (0x1_0000..0x1_0100)
        .filter(|t| ServerMessage::is_known_type(*t))
        .collect();
    types.extend([PLAY_CHARACTER, CLIENT_POSITION, CHAT_SERVER_DATA]);
    let mut matched = 0;
    for (key, recorded) in &runs {
        let (mut c, program) = client_and_program(true);
        let mut frame = f64::NEG_INFINITY;
        let mut commands_known = false;
        let mut at = 0;
        captures::capture_messages_in_time_order(&index, &types, key, |m| {
            if m.time > frame {
                frame = m.time;
                c.set_time(LocalTime(m.time));
                c.upkeep();
            }
            if m.to_client {
                if let Ok(msg) = ServerMessage::decode(&m.raw) {
                    c.take_message(m.blob_id, msg);
                }
                c.shown.clear();
                return;
            }
            match ClientMessage::decode(&m.raw) {
                Ok(ClientMessage::PlayCharacter(p)) => {
                    c.act(Action::PlayCharacter {
                        character: p.character,
                    });
                }
                Ok(ClientMessage::Position(p)) if c.body.placed() => {
                    let heading = c.body.heading;
                    c.body.follow(p.cell, p.origin, heading);
                }
                _ => {}
            }
            if m.message_type != CHAT_SERVER_DATA {
                return;
            }
            let Some((_, want)) = recorded.get(at).filter(|(t, _)| (*t - m.time).abs() < 1e-6)
            else {
                return;
            };
            at += 1;
            // Made known with each session's play screen.
            if !commands_known || c.with_host(|h| h.commands.names.is_empty()).unwrap_or(true) {
                c.know_commands();
                commands_known = true;
            }
            let before = c.sent().len();
            c.process_text(";loc");
            let (sends, _) = said_since(&mut c, before, &program);
            let said = match &sends[..] {
                [words] => words.first().cloned().unwrap_or_default(),
                _ => String::new(),
            };
            let before = c.sent().len();
            c.process_text("/loc");
            let (sends, shown) = said_since(&mut c, before, &program);
            eprintln!(
                "{key}: at {}: recorded {want:?}; said {said:?}; /loc shows {shown:?}",
                m.time
            );
            assert!(sends.is_empty(), "{sends:?}");
            assert_eq!(shown, [format!("You are at: {said}\n")], "{key}");
            if said == *want {
                matched += 1;
            }
        })
        .expect("the index reads");
    }
    assert!(matched + 1 >= lines && matched >= 4, "{matched} of {lines}");
}

/// The server's line of text for the player (an announcement, an admin's line).
const SERVER_TEXT: u32 = 0x100C6;

/// Every line of text the 2005, 2013 and 2017 servers sent a player in the world is given to the
/// player's scripts, which show it in the chat windows as the line the server wrote, of the kind
/// it named; with no player in the world it shows nowhere.
///
/// Behaviour: script.chat.the-servers-lines-of-text-are-shown-by-the-players-scripts
#[test]
fn the_servers_lines_of_text_are_shown_by_the_players_scripts() {
    let index = auberean_testkit::data::require_capture_index();
    let con = captures::open(&index).expect("the index opens");
    let runs: Vec<i64> = {
        let mut stmt = con
            .prepare(&format!(
                "SELECT DISTINCT s.capture_id FROM message m JOIN session s USING (session_id) \
                 WHERE m.mtype = {SERVER_TEXT} AND m.dir = 's2c' ORDER BY 1"
            ))
            .expect("the query");
        let rows = stmt
            .query_map([], |r| r.get::<_, i64>(0))
            .expect("the rows");
        rows.map(|r| r.expect("a row")).collect()
    };
    let mut types: Vec<u32> = (0x1_0000..0x1_0100)
        .filter(|t| ServerMessage::is_known_type(*t))
        .collect();
    types.push(PLAY_CHARACTER);
    let (mut sent, mut in_world, mut shown_as_sent, mut other) = (0, 0, 0, 0);
    let mut tally = auberean_client_runtime::CallTally::default();
    for run in &runs {
        let mut c = client();
        let mut frame = f64::NEG_INFINITY;
        captures::capture_by_id_in_time_order(&index, &types, *run, |m| {
            if m.time > frame {
                frame = m.time;
                c.set_time(LocalTime(m.time));
                c.upkeep();
            }
            if !m.to_client {
                if let Ok(ClientMessage::PlayCharacter(p)) = ClientMessage::decode(&m.raw) {
                    c.act(Action::PlayCharacter {
                        character: p.character,
                    });
                }
                return;
            }
            let Ok(msg) = ServerMessage::decode(&m.raw) else {
                return;
            };
            c.shown.clear();
            let text = match &msg {
                ServerMessage::Notice(n) if m.message_type == SERVER_TEXT => {
                    sent += 1;
                    Some((n.kind, n.text.clone()))
                }
                _ => None,
            };
            let player_here = c.model.player_object().is_some();
            c.take_message(m.blob_id, msg);
            let Some((kind, info)) = text else {
                return;
            };
            if !player_here {
                assert!(c.shown.is_empty(), "nothing shows without a player");
                return;
            }
            in_world += 1;
            let want = auberean_client_runtime::text::from_wire(&info)
                .text(&mut c.scripts.as_mut().expect("the scripts").host.text);
            let lines: Vec<(u32, String)> = std::mem::take(&mut c.shown)
                .iter()
                .filter_map(|s| Some((s.kind, c.write_text(&s.text)?)))
                .collect();
            if lines == [(kind, want.clone())] {
                shown_as_sent += 1;
            } else {
                other += 1;
                eprintln!("{run}: kind {kind} {want:?}: shown {lines:?}");
            }
        })
        .expect("the index reads");
        tally.calls += c.server_texts.calls;
        tally.completed += c.server_texts.completed;
    }
    eprintln!(
        "server lines: {sent} in {} runs; {in_world} with the player in the world: {shown_as_sent} shown as sent, {other} otherwise; calls {tally:?}",
        runs.len()
    );
    assert!(in_world >= 100, "{in_world}");
    assert_eq!(tally.completed, tally.calls);
    assert_eq!(shown_as_sent, in_world);
}

/// The call a creature's line to everyone near it comes in (its idle lines), carried to the
/// speaker as an object in a cell.
const BROADCAST_LINE: u32 = 0x8205_001F;
/// The client's request that the server send an object again.
const FORCE_CREATE: u32 = 0x1008F;
/// A frame of the replay: the client's world upkeep runs once a frame, messages or not.
const FRAME: f64 = 1.0 / 30.0;

/// What the unknown speakers' replay counted.
#[derive(Debug, Default)]
struct AskedFor {
    /// Broadcast lines recorded; of those, from an object the client did not have then.
    lines: usize,
    unknown: usize,
    /// Requests for those speakers: the recorded client's, ours, and ours within a quarter of a
    /// second of a recorded one.
    recorded: usize,
    ours: usize,
    matched: usize,
}

/// A creature's line to everyone near it, from an object the client was never sent, waits for
/// that object; 25 seconds after the last message that waited for it, at the next frame's
/// upkeep, the client asks the server to send the object again and lets the
/// waiting messages go, and a later line starts the wait again. Over every such line the 2017
/// servers sent (89 of 157 from an object the client had never been sent, in 22 runs), the client
/// asks for those speakers when the recorded clients did (69 requests), each within a quarter of a
/// second of the recorded request, and at no other time; the servers never sent the speaker in
/// answer.
///
/// Behaviour: model.events.an-unknown-speaker-is-asked-for-25-seconds-after-its-last-line
#[test]
fn an_unknown_speaker_is_asked_for_25_seconds_after_its_last_line() {
    use auberean_testkit::captures::carrier;
    use std::collections::{BTreeMap, BTreeSet};
    let index = auberean_testkit::data::require_capture_index();
    let mut runs = BTreeSet::new();
    captures::messages(&index, &[carrier::TO_CELL_OBJECT], &[2017], |m| {
        if m.to_client
            && m.script_call()
                .is_some_and(|c| c.function == BROADCAST_LINE)
        {
            runs.insert(m.capture);
        }
    })
    .expect("the index reads");
    let mut types: Vec<u32> = (0x1_0000..0x1_0100)
        .filter(|t| ServerMessage::is_known_type(*t))
        .collect();
    types.extend([PLAY_CHARACTER, FORCE_CREATE]);
    let mut n = AskedFor::default();
    for run in &runs {
        let mut c = client();
        let mut frame = f64::NEG_INFINITY;
        let mut speakers: BTreeSet<u64> = BTreeSet::new();
        let mut recorded: BTreeMap<u64, Vec<f64>> = BTreeMap::new();
        captures::capture_by_id_in_time_order(&index, &types, *run, |m| {
            if m.time > frame {
                if frame.is_finite() {
                    while frame + FRAME < m.time {
                        frame += FRAME;
                        c.set_time(LocalTime(frame));
                        c.upkeep();
                    }
                }
                frame = m.time;
                c.set_time(LocalTime(m.time));
                c.upkeep();
            }
            if !m.to_client {
                match ClientMessage::decode(&m.raw) {
                    Ok(ClientMessage::PlayCharacter(p)) => c.act(Action::PlayCharacter {
                        character: p.character,
                    }),
                    Ok(ClientMessage::ForceCreate(t)) => {
                        recorded.entry(t.0).or_default().push(m.time);
                    }
                    _ => {}
                }
                return;
            }
            if let Some(s) = m.script_call().filter(|s| s.function == BROADCAST_LINE) {
                n.lines += 1;
                if let Some(o) = s.object.filter(|o| c.model.object(*o).is_none()) {
                    n.unknown += 1;
                    speakers.insert(o);
                }
            }
            if let Ok(msg) = ServerMessage::decode(&m.raw) {
                c.take_message(m.blob_id, msg);
            }
            c.shown.clear();
        })
        .expect("the index reads");
        let mut ours: BTreeMap<u64, Vec<f64>> = BTreeMap::new();
        for s in c.sent() {
            if let ClientMessage::ForceCreate(t) = &s.message {
                ours.entry(t.0).or_default().push(s.at);
            }
        }
        for o in &speakers {
            let r = recorded.get(o).map_or(&[][..], Vec::as_slice);
            let ours = ours.get(o).map_or(&[][..], Vec::as_slice);
            n.recorded += r.len();
            n.ours += ours.len();
            let mut used = vec![false; ours.len()];
            for t in r {
                if let Some(i) = (0..ours.len()).find(|i| !used[*i] && (ours[*i] - t).abs() <= 0.25)
                {
                    used[i] = true;
                    n.matched += 1;
                } else {
                    eprintln!("{run}: {o:#x} asked for at {t}; ours {ours:?}");
                }
            }
        }
    }
    eprintln!("runs {}: {n:?}", runs.len());
    assert_eq!((n.lines, n.unknown), (157, 89), "{n:?}");
    assert_eq!(n.recorded, 69, "{n:?}");
    assert_eq!(n.matched, n.recorded, "{n:?}");
    assert_eq!(n.ours, n.recorded, "{n:?}");
}

/// The server's call telling the player it dealt a killing blow, with the defender's name.
const DISPLAY_KILLING_MSG: u32 = 0x8207_002D;
/// The killing blow's line: the enum of its table and its string; the name's variable.
const KILLING_LINE: (u32, u32) = (0x4100_000A, 0xEB50_E552);
const DEFENDER: u32 = 0x9E66_1D49;

/// What the killing blows' replay counted.
#[derive(Debug, Default)]
struct Blows {
    /// Calls recorded; of those, with the player in the world.
    sent: usize,
    in_world: usize,
    /// Shown as the call took effect; shown later, when the player's stream let the call go;
    /// still held by the player's stream at the run's end (a message before it the recording
    /// lost); shown otherwise.
    shown: usize,
    shown_later: usize,
    held: usize,
    other: usize,
}

/// Every killing blow the 2013 and 2017 servers told a player in the world is shown by the
/// player's scripts as one line of the kind 8: the killing blow's string from its table ("Your
/// killing blow slays" and the name), the defender's name (as the server wrote it) in its
/// variable. A call the player's ordered stream holds until a message before it comes is shown
/// when the stream lets it go (3 of the 510).
///
/// Behaviour: script.chat.a-killing-blow-is-shown-by-the-players-scripts
#[test]
fn a_killing_blow_is_shown_by_the_players_scripts() {
    use auberean_testkit::captures::carrier;
    use std::collections::BTreeSet;
    let index = auberean_testkit::data::require_capture_index();
    let mut runs = BTreeSet::new();
    captures::messages(&index, &[carrier::TO_PLAYER], &[2013, 2017], |m| {
        if m.script_call()
            .is_some_and(|c| c.function == DISPLAY_KILLING_MSG)
        {
            runs.insert(m.capture);
        }
    })
    .expect("the index reads");
    let mut types: Vec<u32> = (0x1_0000..0x1_0100)
        .filter(|t| ServerMessage::is_known_type(*t))
        .collect();
    types.push(PLAY_CHARACTER);
    let mut n = Blows::default();
    for run in &runs {
        let mut c = client();
        let program = c
            .scripts
            .as_ref()
            .map(|s| Arc::clone(&s.host.program))
            .expect("the scripts");
        let mut frame = f64::NEG_INFINITY;
        // Lines due from calls the player's stream holds.
        let mut pending: Vec<String> = Vec::new();
        captures::capture_by_id_in_time_order(&index, &types, *run, |m| {
            if m.time > frame {
                frame = m.time;
                c.set_time(LocalTime(m.time));
                c.upkeep();
            }
            if !m.to_client {
                if let Ok(ClientMessage::PlayCharacter(p)) = ClientMessage::decode(&m.raw) {
                    c.act(Action::PlayCharacter {
                        character: p.character,
                    });
                }
                return;
            }
            let Ok(msg) = ServerMessage::decode(&m.raw) else {
                return;
            };
            c.shown.clear();
            let defender = m
                .script_call()
                .filter(|s| s.function == DISPLAY_KILLING_MSG)
                .map(|s| texts(&s.args, &program));
            let player_here = c.model.player_object().is_some();
            let calls = c.calls.calls;
            c.take_message(m.blob_id, msg);
            let mut lines: Vec<(u32, String)> = std::mem::take(&mut c.shown)
                .iter()
                .filter_map(|s| Some((s.kind, c.write_text(&s.text)?)))
                .collect();
            lines.retain(|(kind, line)| {
                let Some(i) = (*kind == 8)
                    .then(|| pending.iter().position(|p| p == line))
                    .flatten()
                else {
                    return true;
                };
                pending.remove(i);
                n.shown_later += 1;
                false
            });
            let Some(defender) = defender else {
                return;
            };
            n.sent += 1;
            if !player_here {
                return;
            }
            n.in_world += 1;
            let [defender] = &defender[..] else {
                panic!("one name: {defender:?}");
            };
            let s = c.scripts.as_mut().expect("the scripts");
            let table = s
                .host
                .text
                .data_id_by_enum(KILLING_LINE.0, auberean_text::source::TABLE_MAPPING);
            let mut want = auberean_text::StringInfo::entry(table, KILLING_LINE.1);
            want.add_var(
                DEFENDER,
                auberean_text::Value::Text(Box::new(auberean_client_runtime::text::from_script(
                    defender,
                ))),
            );
            let want = want.text(&mut s.host.text);
            assert!(want.starts_with("Your killing blow slays "), "{want:?}");
            if c.calls.calls == calls {
                pending.push(want);
            } else if lines == [(8, want.clone())] {
                n.shown += 1;
            } else {
                n.other += 1;
                eprintln!("{run}: {want:?}: shown {lines:?}");
            }
        })
        .expect("the index reads");
        n.held += pending.len();
    }
    eprintln!("killing blows in {} runs: {n:?}", runs.len());
    assert!(n.in_world >= 500, "{n:?}");
    assert_eq!(n.other, 0, "{n:?}");
    assert_eq!(n.shown + n.shown_later + n.held, n.in_world, "{n:?}");
    assert_eq!(n.held, 0, "{n:?}");
}

mod friends {
    //! The friends service through the player's own scripts, on the recorded session brought
    //! into the world: a tell to another world goes out through the service and its answer is
    //! shown, a tell heard is shown, a friend's state reaches the friends page, and the friends
    //! list asked for reaches it whole.

    use core::time::Duration;

    use auberean_client_contract::{Action, SocialAction};
    use auberean_client_net::friends::stream::{
        Answer, Event, FriendStatus, Hr, Mode, Request, ServerConfig, ServerEvent, ServerOut,
        ServerStream, Ticket,
    };
    use auberean_client_net::friends::{FriendsLink, Io};
    use auberean_client_runtime::friends::{ChatExtra, Friends, FriendsOptions, UserStatus};
    use auberean_client_runtime::Client;

    use super::in_the_world;

    /// The world's name as the session's login named it (the recorded session's).
    const WORLD: &str = "Dawnsong";

    /// The world's side of the friends stream: admits the ticket, keeps what it is asked, answers a
    /// tell with `tell_answer`.
    struct World {
        stream: Option<ServerStream>,
        asked: Vec<Request>,
        tell_answer: Hr,
    }

    impl World {
        fn handle(&mut self, now: Duration, e: ServerEvent) {
            let Some(s) = self.stream.as_mut() else {
                return;
            };
            match e {
                ServerEvent::Ticket { cx, .. } => {
                    s.admit(now, cx);
                    let _ = s.enter(now, Vec::new());
                }
                ServerEvent::Request { cx, request } => {
                    self.asked.push(request.clone());
                    let (hr, answer) = match request {
                        Request::GetFriendsList => (
                            Hr::OK,
                            Answer::GetFriendsList {
                                friends: vec![
                                    FriendStatus::new(&format!("Cara@{WORLD}"), Mode(1)),
                                    FriendStatus::new("Bob@coldeve", Mode(0)),
                                ],
                            },
                        ),
                        Request::SendToUserEx { .. } => (self.tell_answer, Answer::SendToUserEx),
                        other => (Hr::OK, Answer::empty(other.kind())),
                    };
                    let _ = s.answer(now, cx, hr, answer);
                }
                ServerEvent::Hello(_) | ServerEvent::Ended(_) => {}
            }
        }

        fn event(&mut self, c: &Client, e: Event) {
            let now = Duration::from_secs_f64(c.local_time());
            self.stream
                .as_mut()
                .expect("a stream")
                .event(now, e)
                .expect("sent");
        }
    }

    /// Carry the stream both ways at the client's own time, then let the client take what came.
    fn pump(c: &mut Client, w: &mut World) {
        let now = Duration::from_secs_f64(c.local_time());
        for _ in 0..4 {
            loop {
                let mut moved = false;
                let link = c
                    .friends_mut()
                    .and_then(|f| f.connection().io())
                    .expect("the link");
                while let Some(io) = link.poll_io(now) {
                    moved = true;
                    match io {
                        Io::Dial { .. } => {
                            w.stream =
                                Some(ServerStream::new(now, ServerConfig::new("dawnsong", WORLD)));
                            link.opened(now);
                        }
                        Io::Write(b) => {
                            if let Some(s) = w.stream.as_mut() {
                                s.on_bytes(now, &b);
                            }
                        }
                        Io::Close => w.stream = None,
                    }
                }
                let outs = w.stream.as_mut().map(|s| s.poll(now)).unwrap_or_default();
                for out in outs {
                    moved = true;
                    match out {
                        ServerOut::Write(b) => link.on_bytes(now, &b),
                        ServerOut::Close => link.closed(now),
                        ServerOut::Event(e) => w.handle(now, e),
                    }
                }
                if !moved {
                    break;
                }
            }
            c.pump_friends();
        }
    }

    /// The recorded player in the world with the friends service up, its ticket taken.
    fn connected() -> (Client, World) {
        let (mut c, _) = in_the_world();
        let mut w = World {
            stream: None,
            asked: Vec::new(),
            tell_answer: Hr::OK,
        };
        let options = FriendsOptions {
            server: "127.0.0.1".into(),
            port: 9000,
            use_ticket: true,
            ticket: Ticket::new("player", "secret").encode().ok(),
            named: true,
        };
        c.use_friends(Friends::new(
            Box::new(FriendsLink::new("auberean-test/1.0", None)),
            &options,
        ));
        pump(&mut c, &mut w);
        let f = c.friends_mut().expect("the layer");
        assert!(f.connected());
        if f.world_name().is_empty() {
            f.set_world_name(&WORLD.encode_utf16().collect::<Vec<_>>());
        }
        c.shown.clear();
        c.notices.clear();
        (c, w)
    }

    /// The lines shown since they were last taken.
    fn shown(c: &mut Client) -> Vec<String> {
        std::mem::take(&mut c.shown)
            .iter()
            .filter_map(|s| c.write_text(&s.text))
            .collect()
    }

    /// `/tell Name@World, text` goes to the friends service (the name and text as typed), and the
    /// service's answer is shown by the player's scripts: the client's own echo when the tell went,
    /// the tellee not on when the service says so, the tellee unknown when it says that.
    ///
    /// Behaviour: friends.scripts.a-tell-to-another-world-goes-through-the-service-and-its-answer-is-shown
    #[test]
    fn a_tell_to_another_world_goes_through_the_service_and_its_answer_is_shown() {
        let (mut c, mut w) = connected();
        for (answer, says) in [
            (Hr::OK, "hi there"),
            (Hr::NOT_ONLINE, "not logged in"),
            (Hr::NO_SUCH_USER, "doesn't exist"),
        ] {
            w.tell_answer = answer;
            let asked = w.asked.len();
            c.process_text("/tell Bob@coldeve, hi there");
            pump(&mut c, &mut w);
            assert_eq!(
                w.asked.get(asked),
                Some(&Request::SendToUserEx {
                    user: "Bob@coldeve".into(),
                    text: "hi there".into(),
                    extra: w.asked.get(asked).and_then(|r| match r {
                        Request::SendToUserEx { extra, .. } => *extra,
                        _ => None,
                    }),
                }),
                "{:?}",
                w.asked
            );
            let lines = shown(&mut c);
            assert!(
                lines
                    .iter()
                    .any(|l| l.contains("Bob@coldeve") && l.contains(says)),
                "{answer:?}: {lines:?}"
            );
        }
    }

    /// A tell heard through the service is shown by the player's scripts with the teller's name and
    /// the text as sent, markup and all, as the teller's own words.
    ///
    /// Behaviour: friends.scripts.a-tell-heard-through-the-service-is-shown-with-its-text-as-sent
    #[test]
    fn a_tell_heard_through_the_service_is_shown_with_its_text_as_sent() {
        let (mut c, mut w) = connected();
        let extra = ChatExtra::new(&"Bob [M]".encode_utf16().collect::<Vec<_>>(), 0);
        w.event(
            &c,
            Event::UTellEv {
                user: "Bob@coldeve".into(),
                text: "well [met]".into(),
                extra: Some(extra.to_stream()),
            },
        );
        pump(&mut c, &mut w);
        let lines = shown(&mut c);
        assert!(
            lines.iter().any(|l| l.contains("Bob@coldeve")
                && l.contains("tells you")
                && l.contains("well [met]")),
            "{lines:?}"
        );
    }

    /// A friend's new state and the friends list asked for reach the friends page through the
    /// player's scripts: the page's notices carry the friend's state, and the list whole, names of
    /// this world without the world's name.
    ///
    /// Behaviour: friends.scripts.a-friends-state-and-the-list-reach-the-friends-page
    #[test]
    fn a_friends_state_and_the_list_reach_the_friends_page() {
        /// The scripts' notices to the friends page: the list, and one friend's state.
        const LIST: u32 = 0x1AD2_0001;
        const STATUS: u32 = 0x1AD2_0004;
        let (mut c, mut w) = connected();
        w.event(
            &c,
            Event::FriendStat {
                user: format!("Cara@{WORLD}"),
                mode: Mode(3),
            },
        );
        pump(&mut c, &mut w);
        let status = c
            .notices
            .iter()
            .find(|n| n.native == STATUS)
            .unwrap_or_else(|| panic!("a friend's state: {:?}", c.notices));
        let s = status
            .objects
            .iter()
            .flatten()
            .find_map(UserStatus::of)
            .expect("the friend's state");
        assert_eq!(String::from_utf16_lossy(&s.name), "Cara");
        assert_eq!(s.mode, 3);
        c.notices.clear();
        c.act(Action::Social(SocialAction::GetFriends));
        pump(&mut c, &mut w);
        let list = c
            .notices
            .iter()
            .find(|n| n.native == LIST)
            .unwrap_or_else(|| panic!("the list: {:?}", c.notices));
        let names: Vec<(String, u32)> = list
            .listed
            .iter()
            .flatten()
            .flatten()
            .filter_map(UserStatus::of)
            .map(|s| (String::from_utf16_lossy(&s.name), s.mode))
            .collect();
        assert_eq!(
            names,
            [("Cara".to_owned(), 1), ("Bob@coldeve".to_owned(), 0)]
        );
    }
}

/// The player's folder for one test: files by name behind a lock the test keeps a handle to, and a
/// clock stopped at one time.
#[derive(Clone, Default)]
struct Folder(Arc<std::sync::Mutex<std::collections::BTreeMap<String, Vec<u8>>>>);

impl auberean_script::local::LocalSystem for Folder {
    fn folder(&self) -> Option<String> {
        Some(r"C:\Users\Player\Dereth".to_owned())
    }

    fn local_time(&self) -> Option<auberean_script::local::LocalTime> {
        Some(auberean_script::local::LocalTime {
            year: 2017,
            month: 1,
            day: 9,
            hour: 20,
            minute: 5,
            second: 3,
            ..Default::default()
        })
    }

    fn append_line(&mut self, file: &str, bytes: &[u8]) -> bool {
        self.0
            .lock()
            .unwrap()
            .entry(file.to_owned())
            .or_default()
            .extend_from_slice(bytes);
        true
    }

    fn write_file(&mut self, file: &str, bytes: &[u8]) -> bool {
        self.0
            .lock()
            .unwrap()
            .insert(file.to_owned(), bytes.to_vec());
        true
    }

    fn read_file(&mut self, file: &str) -> Option<Vec<u8>> {
        self.0.lock().unwrap().get(file).cloned()
    }
}

/// `/logfile open` with no folder to write in shows the player's scripts' line that the file
/// could not be opened; with one, it opens the player's log there (the player's name and `.log`,
/// shown after the folder's full path, as the client shows its own folder),
/// marked as text in 16-bit units, and every line the chat window is given is written to it after
/// the local time.
///
/// Behaviour: script.chat.the-chat-log-is-written-with-the-local-time
#[test]
fn the_chat_log_says_when_it_cannot_open_and_writes_each_line_after_the_local_time() {
    let (mut c, _) = in_the_world();
    let shown = |c: &mut Client| -> Vec<String> {
        std::mem::take(&mut c.shown)
            .iter()
            .filter_map(|s| c.write_text(&s.text))
            .collect()
    };
    shown(&mut c);
    c.process_text("/logfile open");
    assert_eq!(
        shown(&mut c),
        ["Failed to open file \\Vacuos.log for writing.\n"]
    );
    let folder = Folder::default();
    let local = folder.clone();
    c.with_host(|h| h.set_local(Some(Box::new(local))));
    c.process_text("/logfile open");
    assert_eq!(
        shown(&mut c),
        ["Any text displayed in chatboxes after this will write to: C:\\Users\\Player\\Dereth\\Vacuos.log\n"]
    );
    c.process_text("/alias ;wm Well met");
    let lines = shown(&mut c);
    assert!(!lines.is_empty());
    let files = folder.0.lock().unwrap().clone();
    assert_eq!(files.keys().collect::<Vec<_>>(), ["Vacuos.log"]);
    let bytes = &files["Vacuos.log"];
    assert_eq!(&bytes[..2], &[0xFF, 0xFE], "marked");
    let units: Vec<u16> = bytes[2..]
        .chunks(2)
        .map(|p| u16::from_le_bytes([p[0], p[1]]))
        .collect();
    let text = String::from_utf16(&units).unwrap();
    let want: String = lines
        .iter()
        .map(|l| format!("<Jan 09 2017, 20:05:03>  {}", l.replace('\n', "\r\n")))
        .collect();
    assert!(text.ends_with(&want), "{text:?} ends with {want:?}");
}

/// The player's commands to save and load the panels' places reach the play screen as notices, each
/// naming the file; the player's files are written and read back by a bare name.
///
/// Behaviour: script.chat.the-layout-commands-reach-the-panels-with-their-file
#[test]
fn the_layout_commands_reach_the_panels_with_their_file() {
    let (mut c, _) = in_the_world();
    let folder = Folder::default();
    let local = folder.clone();
    c.with_host(|h| h.set_local(Some(Box::new(local))));
    c.notices.clear();
    c.process_text("/saveui mine");
    c.process_text("/loadui mine");
    let natives: Vec<u32> = c.notices.iter().map(|n| n.native).collect();
    assert_eq!(natives, [0x85 << 16 | 0x1C, 0x85 << 16 | 0x1D]);
    for n in &c.notices {
        assert_eq!(n.about, c.model.player, "it is about the player");
        assert!(
            n.objects
                .iter()
                .flatten()
                .any(|o| matches!(o, auberean_script::native_object::NativeObject::TextInfo(_))),
            "it carries the name"
        );
    }
    assert!(c.write_player_file("mine.txt", b"<CHAT1> X:1 Y: 2 W: 3 H: 4\n"));
    assert_eq!(
        c.read_player_file("mine.txt").as_deref(),
        Some(&b"<CHAT1> X:1 Y: 2 W: 3 H: 4\n"[..])
    );
    assert_eq!(c.read_player_file("other.txt"), None);
    assert_eq!(folder.0.lock().unwrap().len(), 1);
}

mod rooms {
    //! Rooms through the chat service, on 2017 runs whose player wrote to the general channel:
    //! the player's lines typed again to the general channel's command go to the service as the
    //! recorded client sent them, and every room's line heard is written by the player's scripts.

    use auberean_client_contract::Action;
    use auberean_client_net::LocalTime;
    use auberean_protocol::{ClientMessage, ServerMessage};
    use auberean_testkit::captures;
    use dereth_protocol::turbine::{decode_incoming, IncomingPayload, SendToRoomById};

    use super::{client_and_program, PLAY_CHARACTER};

    /// The general channel's command, as the player's scripts made it known.
    const GENERAL_CHANNEL_CHAT: u32 = 0x1AA4_0194;

    /// January 2017 runs in which the player wrote only to the general channel, every line
    /// answered before the recording ended.
    const RUNS: [&str; 6] = [
        "11ed67e39fbb902e:0",
        "3db070dfe56fc773:0",
        "19fa8fae5416db4c:0",
        "35db83e294f9e06c:0",
        "9f07c7a72ce28326:0",
        "36243488d1703ade:0",
    ];

    /// The chat service's packet with its count before it, as the service's codec reads one.
    fn framed(packet: &[u8]) -> Vec<u8> {
        let mut f = u32::try_from(packet.len())
            .expect("a packet")
            .to_le_bytes()
            .to_vec();
        f.extend_from_slice(packet);
        f
    }

    /// What a replay found: each recorded line's packet beside the client's own for it; each
    /// room's line heard (speaker, words, room) with the lines the scripts wrote for it; the
    /// lines still waiting for an answer at the end.
    struct Replay {
        requests: Vec<(Vec<u8>, Vec<u8>)>,
        heard: Vec<(String, String, u32, Vec<String>)>,
        waiting: Vec<u32>,
    }

    /// One run brought through: each line the player wrote typed again to the general
    /// channel's command at the moment it was recorded.
    fn replay(run: &str) -> Replay {
        let index = auberean_testkit::data::require_capture_index();
        let mut types: Vec<u32> = (0x1_0000..0x1_0100)
            .filter(|t| ServerMessage::is_known_type(*t))
            .collect();
        types.push(PLAY_CHARACTER);
        let (mut c, _) = client_and_program(true);
        let mut frame = f64::NEG_INFINITY;
        let mut out = Replay {
            requests: Vec::new(),
            heard: Vec::new(),
            waiting: Vec::new(),
        };
        let mut known = false;
        captures::capture_messages_in_time_order(&index, &types, run, |m| {
            if m.time > frame {
                frame = m.time;
                c.set_time(LocalTime(m.time));
                c.upkeep();
            }
            if !m.to_client {
                match ClientMessage::decode(&m.raw) {
                    Ok(ClientMessage::PlayCharacter(p)) => c.act(Action::PlayCharacter {
                        character: p.character,
                    }),
                    Ok(ClientMessage::Chat(p)) => {
                        // Made known with each session's play screen.
                        if !known || c.with_host(|h| h.commands.names.is_empty()).unwrap_or(true) {
                            known = true;
                            c.know_commands();
                        }
                        let command = c
                            .with_host(|h| {
                                h.commands
                                    .names
                                    .iter()
                                    .find(|(_, f)| **f == GENERAL_CHANNEL_CHAT)
                                    .map(|(n, _)| n.clone())
                            })
                            .flatten()
                            .expect("the general channel's command");
                        let bytes = framed(&p.0);
                        let mut r = dereth_protocol::Reader::new(&bytes);
                        let recorded = <SendToRoomById as dereth_protocol::Message>::read(&mut r)
                            .expect("a room line");
                        let before = c.sent().len();
                        c.process_text(&format!("/{command} {}", recorded.text));
                        let mine = c.sent()[before..]
                            .iter()
                            .find_map(|s| match &s.message {
                                ClientMessage::Chat(q) => Some(q.0.clone()),
                                _ => None,
                            })
                            .unwrap_or_default();
                        out.requests.push((mine, p.0));
                    }
                    _ => {}
                }
                return;
            }
            let Ok(msg) = ServerMessage::decode(&m.raw) else {
                return;
            };
            let event = match &msg {
                ServerMessage::Chat(p) => match decode_incoming(&framed(&p.0)).map(|i| i.payload) {
                    Ok(IncomingPayload::RoomEvent(e)) => Some((e.name, e.text, e.room)),
                    _ => None,
                },
                _ => None,
            };
            c.shown.clear();
            c.take_message(m.blob_id, msg);
            let shown: Vec<String> = std::mem::take(&mut c.shown)
                .iter()
                .filter_map(|s| c.write_text(&s.text))
                .collect();
            if let Some((name, text, room)) = event {
                out.heard.push((name, text, room, shown));
            }
        })
        .expect("the index reads");
        out.waiting = c.rooms().map(|r| r.waiting()).unwrap_or_default();
        out
    }

    /// Every line the recorded players wrote to the general channel, typed again to its
    /// command, goes to the chat service byte for byte as the recorded client sent it: the
    /// header, the mark counted from 1 across the run, the general room (1), the text, and what
    /// the player's own name says about it (its meta letters). Each line's answer comes back to
    /// the line it answers: none is left waiting.
    ///
    /// Behaviour: friends.rooms.a-line-to-a-channel-goes-to-the-chat-service-as-the-recorded-clients-sent-it
    #[test]
    fn a_line_to_a_channel_goes_to_the_chat_service_as_the_recorded_clients_sent_it() {
        let mut lines = 0;
        for run in RUNS {
            let r = replay(run);
            assert!(!r.requests.is_empty(), "{run}: the player wrote");
            for (mine, theirs) in &r.requests {
                assert_eq!(
                    mine, theirs,
                    "{run}: the line as the recorded client sent it"
                );
            }
            assert_eq!(r.waiting, Vec::<u32>::new(), "{run}: every line answered");
            lines += r.requests.len();
        }
        assert_eq!(lines, 24, "every recorded line of the six runs");
    }

    /// Every room's line the six runs heard (the general room's, and one of the player's
    /// allegiance's) is written by the player's scripts as one line: the channel's name in
    /// brackets, the speaker's name (its meta letters, which the service's note
    /// carries, left off), a colon and the words, a run of spaces in them written as one.
    ///
    /// Behaviour: friends.rooms.a-rooms-line-heard-is-written-with-its-channel-speaker-and-words
    #[test]
    fn a_rooms_line_heard_is_written_with_its_channel_speaker_and_words() {
        let mut heard = 0;
        for run in RUNS {
            for (name, text, room, shown) in replay(run).heard {
                // The general room, and in one run the player's allegiance's (its room is the one
                // the allegiance's profile names).
                let channel = match room {
                    1 => "General",
                    44 => "Allegiance",
                    _ => panic!("{run}: room {room}"),
                };
                let words = text
                    .split(' ')
                    .filter(|w| !w.is_empty())
                    .collect::<Vec<_>>();
                let trailing = if text.ends_with(' ') { " " } else { "" };
                let leading = if text.starts_with(' ') { " " } else { "" };
                let want = format!(
                    "[{channel}] {name}: {leading}{}{trailing}\n",
                    words.join(" ")
                );
                assert_eq!(shown, [want], "{run}: {name}: {text:?}");
                heard += 1;
            }
        }
        assert_eq!(heard, 381, "every room's line of the six runs");
    }
}

/// `/help` with a command's name shows that command's help, the name taken in any case; with a
/// name no command's help has, the unrecognized-command line naming it; with no name, the help
/// list's label and then every command's line, `  name -- text`, in the names' order (the 137
/// names the player's scripts give help for).
/// Fixture: a recorded session played to its end, in the world.
///
/// Behaviour: script.chat.help-shows-a-commands-help
#[test]
fn help_shows_a_commands_help_every_commands_line_or_that_it_knows_no_such_command() {
    let (mut c, _program) = in_the_world();
    let names: Vec<String> = c
        .with_host(|h| h.commands.help.keys().cloned().collect())
        .expect("the scripts");
    let mut help = |typed: &str| {
        c.shown.clear();
        let status = c.process_text(typed);
        assert_eq!(status, ShellStatus::Success, "{typed}");
        let shown: Vec<(u32, String)> = std::mem::take(&mut c.shown)
            .iter()
            .filter_map(|s| Some((s.kind, c.write_text(&s.text)?)))
            .collect();
        shown
    };
    // Help text is shown as kind 8, the unrecognized-command line as kind 1.
    let tell = help("/help tell");
    assert_eq!(
        tell,
        [(
            8,
            "usage: /tell <target name>, message\nsends text of message to named target player\n"
                .to_owned()
        )]
    );
    assert_eq!(help("/help TELL"), tell);
    assert_eq!(
        help("/help nosuch"),
        [(1, "Unrecognized Command \"nosuch\".\n".to_owned())]
    );
    let all = help("/help");
    assert_eq!(all.len(), 1, "{all:?}");
    assert_eq!(all[0].0, 8);
    let list = &all[0].1;
    let label = "Player Commands:\n To use these commands, prefix the commands names with /\n";
    assert!(list.starts_with(label), "{list}");
    assert!(
        list[label.len()..]
            .starts_with("  ? -- usage: /help <cmd_name>\n  a -- usage: /a <message> \n"),
        "{list}"
    );
    let mut at = 0;
    for name in &names {
        let line = format!("  {name} -- ");
        let found = list[at..].find(&line).map(|i| i + at);
        assert!(found.is_some(), "{name} after {at}: {list}");
        at = found.unwrap_or(at) + line.len();
    }
    assert!(names.len() >= 130, "{}", names.len());
}
