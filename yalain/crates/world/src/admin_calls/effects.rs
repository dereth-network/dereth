//! An admin's commands over effects and what is shown on an object.

use auberean_protocol::physics::{Fx, Word};
use auberean_protocol::types::ObjectStamp;
use auberean_protocol::ServerMessage;
use yalain_store::Store;

use super::{Args, Cx, Target};
use crate::effects::{self, Cast, On};
use crate::places;

/// The calls, by their function ids.
pub mod function {
    pub const ADD: u32 = 0x8201_0026;
    pub const DUMP: u32 = 0x8201_0047;
    pub const DISPEL_ALL: u32 = 0x8201_0014;
    pub const DISPEL: u32 = 0x8201_0054;
    pub const DO_FX: u32 = 0x8201_0032;
    pub const STOP_FX: u32 = 0x8201_0040;
}

/// A call of this kind: its answer; `None` for any other.
pub(super) fn call<S: Store>(cx: &mut Cx<'_, S>, f: u32, a: &Args<'_>) -> Option<Vec<String>> {
    Some(match f {
        function::ADD => add(cx, a),
        function::DUMP => dump(cx, a),
        function::DISPEL_ALL => dispel_all(cx, a),
        function::DISPEL => dispel(cx, a),
        function::DO_FX => do_fx(cx, a),
        function::STOP_FX => stop_fx(cx, a),
        _ => return None,
    })
}

/// What an effect goes on, from an id.
fn on<S: Store>(cx: &Cx<'_, S>, id: u64) -> Option<On> {
    match cx.target(id)? {
        Target::Player(w) => Some(On::Player(w)),
        Target::Thing(t) => Some(On::Creature(t)),
    }
}

fn add<S: Store>(cx: &mut Cx<'_, S>, a: &Args<'_>) -> Vec<String> {
    let Some(on) = on(cx, a.long(0).unwrap_or(0)) else {
        return vec!["No target.".to_owned()];
    };
    let word = a.text(1).unwrap_or_default();
    let Ok(state) = u32::from_str_radix(word.trim().trim_start_matches("0x"), 16) else {
        return vec![format!(
            "{word}: give an effect's id in hex; this server has no names for them."
        )];
    };
    let cast = Cast {
        caster: cx.me(),
        spellcraft: a.float(2).unwrap_or(100.0),
        pk: false,
        skill: 0,
        item: 0,
        giver: None,
        answers: false,
    };
    if effects::apply_on(cx.s, cx.net, on, state, cast, cx.clock) {
        vec![format!("Effect {state:08X} is on.")]
    } else {
        vec![format!("Effect {state:08X} did nothing.")]
    }
}

fn dump<S: Store>(cx: &Cx<'_, S>, a: &Args<'_>) -> Vec<String> {
    let Some(on) = on(cx, a.long(0).unwrap_or(0)) else {
        return vec!["No target.".to_owned()];
    };
    let records = effects::records_of(cx.s, on);
    let mut lines = vec![format!("{} effects.", records.len())];
    lines.extend(records.iter().map(|e| {
        format!(
            "{}: state {:08X}, spellcraft {}, by {:#x}",
            e.eid, e.state, e.spellcraft, e.caster
        )
    }));
    lines
}

fn dispel_all<S: Store>(cx: &mut Cx<'_, S>, a: &Args<'_>) -> Vec<String> {
    let Some(on) = on(cx, a.long(0).unwrap_or(0)) else {
        return vec!["No target.".to_owned()];
    };
    let eids: Vec<u32> = effects::records_of(cx.s, on)
        .iter()
        .map(|e| e.eid)
        .collect();
    let n = eids
        .iter()
        .filter(|e| effects::remove_on(cx.s, cx.net, on, **e))
        .count();
    vec![format!("{n} effects dispelled.")]
}

fn dispel<S: Store>(cx: &mut Cx<'_, S>, a: &Args<'_>) -> Vec<String> {
    let Some(on) = on(cx, a.long(0).unwrap_or(0)) else {
        return vec!["No target.".to_owned()];
    };
    let eid = a.int(1).unwrap_or(0);
    if effects::remove_on(cx.s, cx.net, on, eid) {
        vec![format!("Effect {eid} dispelled.")]
    } else {
        vec![format!("No effect {eid}.")]
    }
}

/// The next visual stamp of whatever an effect is shown on: a player's and a thing's are their own,
/// and a client drops a message whose stamp doesn't follow the object's.
fn stamp<S: Store>(cx: &mut Cx<'_, S>, id: u64) -> ObjectStamp {
    crate::combat::stamp_of(cx.s, id).unwrap_or_else(|| places::object_stamp(cx.s, id))
}

fn do_fx<S: Store>(cx: &mut Cx<'_, S>, a: &Args<'_>) -> Vec<String> {
    let id = if a.long(0).unwrap_or(0) == 0 {
        cx.me()
    } else {
        a.long(0).unwrap_or(0)
    };
    let fx = a.int(1).unwrap_or(0);
    let scalar = a.float(2).unwrap_or(1.0);
    let sender = stamp(cx, id);
    let m = ServerMessage::DoFx(auberean_protocol::physics::ObjectEvent {
        sender,
        action: Fx { fx, scalar },
    });
    places::to_all_knowing(cx.s, cx.net, id, &m);
    vec![format!("Effect {fx:#x} shown on {id:#x}.")]
}

fn stop_fx<S: Store>(cx: &mut Cx<'_, S>, a: &Args<'_>) -> Vec<String> {
    let id = if a.long(0).unwrap_or(0) == 0 {
        cx.me()
    } else {
        a.long(0).unwrap_or(0)
    };
    let fx = a.int(1).unwrap_or(0);
    let sender = stamp(cx, id);
    let m = ServerMessage::StopFx(auberean_protocol::physics::ObjectEvent {
        sender,
        action: Word(fx),
    });
    places::to_all_knowing(cx.s, cx.net, id, &m);
    vec![format!("Effect {fx:#x} stopped on {id:#x}.")]
}
