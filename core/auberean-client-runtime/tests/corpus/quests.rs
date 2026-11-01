//! The journal's quests after every recorded session that gave or changed one: the list the
//! player's scripts keep, asked for as the journal asks, against what the server sent.

use std::collections::BTreeSet;

use auberean_client_contract::Action;
use auberean_client_net::LocalTime;
use auberean_client_runtime::Client;
use auberean_protocol::{ClientMessage, ServerMessage};
use auberean_script::args::{self, Arg};
use auberean_script::graph::Body;
use auberean_script::library::Library;
use auberean_script::native_object::{NativeObject, QuestInfo};
use auberean_testkit::captures::{self, carrier, Message};

use crate::calls::{client, data, PLAY_CHARACTER};

/// The client's log-off: the journal is read before it goes.
const LOG_OFF: u32 = 0x10027;

/// The server's calls that carry quests: the session's start (with the player's whole list) and a
/// quest's update.
const SESSION_START: u32 = 0x8212_0027;
const UPDATE_QUEST: u32 = 0x8213_0024;

/// An update that removes the quest from the list (the update's kind, its phase bits aside).
const REMOVE: u32 = 0x5000_0000;

/// The first native object of a packed argument.
fn native(arg: Option<&Arg>) -> Option<&NativeObject> {
    let Some(Arg::Object(g)) = arg else {
        return None;
    };
    g.entries.iter().find_map(|e| match &e.body {
        Body::Object(o) => o.native.as_ref(),
        _ => None,
    })
}

/// What the server's quest calls leave in the player's list: each entry into the world starts
/// without one until the session's start gives it (its records read as the client reads a
/// list); an update without an error replaces the first record with its id or goes before the
/// first record given earlier, or, to remove, takes out the last record with its id.
#[derive(Default)]
struct Sent {
    list: Option<Vec<QuestInfo>>,
}

impl Sent {
    fn take(&mut self, m: &Message, lib: &Library) {
        let Some(call) = m.script_call() else { return };
        if call.object.is_some() {
            return;
        }
        let Ok(values) = args::decode(&call.args, lib) else {
            return;
        };
        match call.function {
            SESSION_START => {
                if let Some(NativeObject::QuestList(l)) = native(values.get(2)) {
                    self.list = Some(QuestInfo::read_list_over_one(l));
                }
            }
            UPDATE_QUEST => {
                let (Some(Arg::Int(0)), Some(NativeObject::Quest(q)), Some(Arg::Int(kind))) =
                    (values.first(), native(values.get(1)), values.get(2))
                else {
                    return;
                };
                let Some(list) = self.list.as_mut() else {
                    return;
                };
                if kind & 0xF000_0000 == REMOVE {
                    if let Some(at) = list.iter().rposition(|x| x.id == q.id) {
                        list.remove(at);
                    }
                } else if let Some(at) = list.iter().position(|x| x.id == q.id) {
                    list[at] = (**q).clone();
                } else {
                    let at = list
                        .iter()
                        .position(|x| q.bestowed_at() > x.bestowed_at())
                        .unwrap_or(list.len());
                    list.insert(at, (**q).clone());
                }
            }
            _ => {}
        }
    }
}

/// Whenever a recorded player whose server changed a quest logs off, and at the end of its
/// capture, the journal's list (a copy of the list the player's scripts keep, asked for through
/// the interface) holds what the server sent: the session start's records, each update copied
/// over its quest's record or added before the first record given earlier than it, removals
/// taken out.
///
/// Behaviour: script.model.the-journal-holds-the-quests-the-server-sent
#[test]
fn the_journal_holds_every_quest_the_server_gave_newest_given_first() {
    let data = data().expect("the September 2013 install opens");
    let lib = &data.scripts.as_ref().expect("the scripts").1.library;
    let index = auberean_testkit::data::require_capture_index();
    // The captures with a quest update.
    let mut with_updates = BTreeSet::new();
    captures::messages(&index, &[carrier::TO_PLAYER], &[2013, 2017], |m| {
        if m.script_call().is_some_and(|c| c.function == UPDATE_QUEST) {
            with_updates.insert(m.capture);
        }
    })
    .expect("the index reads");
    let mut types: Vec<u32> = (0x1_0000..0x1_0100)
        .filter(|t| ServerMessage::is_known_type(*t))
        .collect();
    types.extend([PLAY_CHARACTER, LOG_OFF]);
    let mut capture = -1;
    let mut c = client(&data);
    let mut sent = Sent::default();
    let mut frame = f64::NEG_INFINITY;
    let (mut compared, mut same, mut quests) = (0u64, 0u64, 0usize);
    let mut differ = Vec::new();
    let mut check = |c: &mut Client, sent: &Sent, capture: i64| {
        let (Some(want), Some(got)) = (sent.list.as_ref(), c.quests()) else {
            return;
        };
        compared += 1;
        quests += got.len();
        if *want == got {
            same += 1;
        } else if differ.len() < 5 {
            let ids = |l: &[QuestInfo]| l.iter().map(|q| q.id).collect::<Vec<_>>();
            differ.push((capture, ids(want), ids(&got)));
        }
    };
    captures::messages_in_time_order(&index, &types, &[2013, 2017], |m| {
        if !with_updates.contains(&m.capture) {
            return;
        }
        if m.capture != capture {
            check(&mut c, &sent, capture);
            capture = m.capture;
            c = client(&data);
            sent = Sent::default();
            frame = f64::NEG_INFINITY;
        }
        if m.time > frame {
            frame = m.time;
            c.set_time(LocalTime(m.time));
            c.upkeep();
        }
        if !m.to_client {
            match ClientMessage::decode(&m.raw) {
                Ok(ClientMessage::PlayCharacter(p)) => {
                    sent.list = None;
                    c.act(Action::PlayCharacter {
                        character: p.character,
                    });
                }
                Ok(ClientMessage::LogOff(_)) => check(&mut c, &sent, capture),
                _ => {}
            }
            return;
        }
        if m.message_type == carrier::TO_PLAYER {
            sent.take(&m, lib);
        }
        if let Ok(msg) = ServerMessage::decode(&m.raw) {
            c.take_message(m.blob_id, msg);
        }
    })
    .expect("the index reads");
    check(&mut c, &sent, capture);
    eprintln!(
        "{} captures with quest updates; {compared} journals compared, {same} the same, {quests} quests held; differ: {differ:?}",
        with_updates.len()
    );
    assert!(compared >= 100, "{compared} journals compared");
    assert_eq!(same, compared, "{differ:?}");
}
