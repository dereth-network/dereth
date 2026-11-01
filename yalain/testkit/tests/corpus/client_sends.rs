//! What our client sends through the player's scripts, held to what the recorded 2017 clients
//! sent: each call the whole session sends equals recorded calls of its function, value for value,
//! once the things that belong to a session are set aside (object ids, the numbering of a graph's
//! entries, a saying's words).

use std::collections::BTreeMap;

use auberean_client_sdk::protocol::ClientMessage;
use auberean_protocol::script::ServerCall;
use auberean_script::native_object::{native_type, TextInfo};
use auberean_script::value::NULL;
use auberean_testkit::captures::{self, carrier};
use auberean_testkit::data::require_capture_index;
use yalain_testkit::authored;
use yalain_testkit::session::{self as play, Drive, InProcess};
use yalain_world::calls::{self, Node, Value};

/// A call's values with what belongs to its session set aside.
fn canonical(args: &[u8]) -> Option<Vec<Value>> {
    let values = calls::values(args).ok()?;
    Some(values.into_iter().map(canonical_value).collect())
}

fn canonical_value(v: Value) -> Value {
    match v {
        Value::Long(_) => Value::Long(0),
        Value::Graph(g) => {
            let place: BTreeMap<u32, u32> =
                g.iter().zip(1..).map(|((id, _), n)| (*id, n)).collect();
            Value::Graph(
                g.into_iter()
                    .map(|(id, n)| (place[&id], canonical_node(n, &place)))
                    .collect(),
            )
        }
        other => other,
    }
}

fn canonical_node(n: Node, place: &BTreeMap<u32, u32>) -> Node {
    match n {
        Node::Script { package, slots } => Node::Script {
            package,
            slots: slots
                .into_iter()
                .map(|(tag, v)| {
                    if tag & 0x80 != 0 {
                        (tag, 0)
                    } else if tag & 0x0F == 1 && v != NULL {
                        (tag, place.get(&v).copied().unwrap_or(v))
                    } else {
                        (tag, v)
                    }
                })
                .collect(),
        },
        Node::TextInfo(t) => Node::TextInfo(TextInfo {
            literal: t.literal.map(|_| Vec::new()),
            ..t
        }),
        Node::Native { ty, mut bytes } if u32::from(ty) == native_type::EXAMINATION_REQUEST => {
            let kind = bytes
                .get(..4)
                .map(|b| u32::from_le_bytes([b[0], b[1], b[2], b[3]]));
            if matches!(kind, Some(0x4000_0002 | 0x4000_000D)) && bytes.len() >= 16 {
                bytes[8..16].fill(0);
            }
            Node::Native { ty, bytes }
        }
        other => other,
    }
}

/// Every call our client sends through the player's scripts in a whole session (selecting,
/// examining, attacking, using a corpse, an item and a portal, saying something) has the values
/// of calls the recorded 2017 clients sent for the same function, object ids, graph numbering and
/// words aside.
/// Behaviour: session.interface.requests-are-the-recorded-clients
#[test]
fn every_call_our_client_sends_is_one_the_recorded_clients_sent() {
    let index = require_capture_index();
    let mut s = InProcess::new();
    let ok = |r: Result<(), String>| r.unwrap_or_else(|e| panic!("{e}"));
    ok(play::character_list(&mut s));
    let me = play::create(&mut s, "Aubrey").unwrap_or_else(|e| panic!("{e}"));
    ok(play::enter(&mut s, me));
    ok(play::see(&mut s, &[authored::SENTINEL, authored::SHARD]));
    s.client().act(auberean_client_sdk::Action::Examination {
        tabs: Some(play::TABS),
    });
    let from = s.client_ref().sent().len();
    let sentinel = play::find(s.client_ref(), authored::SENTINEL).expect("a sentinel");
    let at = sentinel.place.expect("placed").origin;
    play::walk_to(&mut s, [at.x, at.y, at.z], 1.5).unwrap_or_else(|e| panic!("{e}"));
    // A weapon in hand (a human has no attack with an empty hand), before the selection.
    ok(play::wield(&mut s, authored::SWORD).map(|_| ()));
    play::select_and_examine(&mut s, sentinel.id).unwrap_or_else(|e| panic!("{e}"));
    ok(play::attack_until_dead(&mut s, sentinel.id));
    let (_, item) = play::open_corpse(&mut s).unwrap_or_else(|e| panic!("{e}"));
    ok(play::take(&mut s, item));
    ok(play::say(&mut s, "Well met."));
    let shard = play::find(s.client_ref(), authored::SHARD).expect("the shard");
    play::walk_to(&mut s, authored::SHARD_AT, 2.0).unwrap_or_else(|e| panic!("{e}"));
    ok(play::take_portal(&mut s, shard.id, authored::TOWN));

    // Our calls, by function, in their canonical forms.
    let mut ours: BTreeMap<u32, Vec<Vec<Value>>> = BTreeMap::new();
    for m in play::sent_since(s.client_ref(), from) {
        if let ClientMessage::ServerCall(ServerCall { function, args }) = m {
            let c = canonical(&args.0).expect("our call's values read");
            let forms = ours.entry(function.0).or_default();
            if !forms.contains(&c) {
                forms.push(c);
            }
        }
    }
    assert!(ours.len() >= 5, "the session's calls: {:x?}", ours.keys());

    // The recorded calls of those functions, and how many share each of our forms.
    let mut recorded: BTreeMap<u32, usize> = BTreeMap::new();
    let mut matched: BTreeMap<(u32, usize), usize> = BTreeMap::new();
    captures::messages(&index, &[carrier::TO_SERVER], &[2017], |m| {
        let Some(call) = m.script_call() else { return };
        let Some(forms) = ours.get(&call.function) else {
            return;
        };
        *recorded.entry(call.function).or_default() += 1;
        if let Some(c) = canonical(&call.args) {
            if let Some(i) = forms.iter().position(|f| *f == c) {
                *matched.entry((call.function, i)).or_default() += 1;
            }
        }
    })
    .expect("the capture index reads");
    for (f, forms) in &ours {
        for (i, form) in forms.iter().enumerate() {
            let n = matched.get(&(*f, i)).copied().unwrap_or(0);
            let of = recorded.get(f).copied().unwrap_or(0);
            println!("{f:#010x} form {i}: {n} of {of} recorded calls");
            assert!(n > 0, "{f:#010x} form {i} was never recorded: {form:?}");
        }
    }
}
