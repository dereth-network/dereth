//! The effects on the player as the status bar's effects panel lists them, asked of the scripts
//! each time the recorded sessions' scripts said an effect changed; and the objects' own visual
//! descriptions as their scripts read and change them.

use std::collections::BTreeSet;

use auberean_client_contract::Action;
use auberean_client_net::LocalTime;
use auberean_client_runtime::effects::{SESSION_START, UPDATE_EFFECT};
use auberean_client_runtime::Client;
use auberean_protocol::{ClientMessage, ServerMessage};
use auberean_script::args::{self, Arg};
use auberean_script::graph::Body;
use auberean_testkit::captures;

use crate::calls::{client, data, PLAY_CHARACTER};

/// The server's calls that add an effect to the player's list and take one off.
const ADD_EFFECT: u32 = 0x8208_0017;
const REMOVE_EFFECT: u32 = 0x8208_0016;

/// An effect's record (a script object of this package), and the slot holding its number.
const EFFECT_RECORD: u32 = 0x14E;
const RECORD_EID: usize = 23;

/// The numbers of the effect records a call's values carry (the session's start carries the
/// player's whole registry of effects, the shown ones and the ones under them).
fn records(bytes: &[u8], lib: &auberean_script::Library) -> BTreeSet<u32> {
    let mut out = BTreeSet::new();
    for a in args::decode(bytes, lib).unwrap_or_default() {
        let Arg::Object(g) = a else { continue };
        for e in &g.entries {
            if let Body::Object(o) = &e.body {
                if u32::from(o.package) == EFFECT_RECORD {
                    out.extend(o.slots.get(RECORD_EID).copied());
                }
            }
        }
    }
    out
}

fn watching(data: &auberean_client_runtime::ClientData) -> Client {
    let mut c = client(data);
    c.observe = vec![ADD_EFFECT, REMOVE_EFFECT, SESSION_START];
    c
}

/// Each time a recorded session's scripts said an effect on the player changed, the effects the
/// scripts list for the panel each have a number, a name and an icon, no number twice, and every
/// one was in the player's effects when the session started or added by the server since, and not
/// taken off since (each call counted as it took effect; an effect under another is listed once
/// the one above it goes).
///
/// Behaviour: ui.statusbar.the-effects-from-the-scripts-are-the-players-effects
#[test]
fn the_effects_from_the_scripts_are_the_effects_the_server_gave_the_player() {
    let data = data().expect("the September 2013 install opens");
    let index = auberean_testkit::data::require_capture_index();
    let lib = &data.scripts.as_ref().expect("scripts").1.library;
    let mut types: Vec<u32> = (0x1_0000..0x1_0100)
        .filter(|t| ServerMessage::is_known_type(*t))
        .collect();
    types.push(PLAY_CHARACTER);
    let mut capture = -1;
    let mut c = watching(&data);
    let mut frame = f64::NEG_INFINITY;
    // The effects the session's start listed or the server added since, once a session started.
    let mut given: Option<BTreeSet<u32>> = None;
    let mut asked = 0usize;
    let mut listed = 0usize;
    let mut timed = 0usize;
    let mut pulsed = 0usize;
    let mut problems: Vec<String> = Vec::new();
    captures::messages_in_time_order(&index, &types, &[2013, 2017], |m| {
        if m.capture != capture {
            capture = m.capture;
            c = watching(&data);
            frame = f64::NEG_INFINITY;
            given = None;
        }
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
        c.take_message(m.blob_id, msg);
        let noticed = c.notices.iter().any(|n| n.native == UPDATE_EFFECT);
        c.notices.clear();
        let player = c.model.player;
        for (_, function, a, object) in std::mem::take(&mut c.observed) {
            if object.is_some() && object != player {
                continue;
            }
            match function {
                SESSION_START => {
                    let bytes = a.map(|a| a.0).unwrap_or_default();
                    given = Some(records(&bytes, lib));
                }
                ADD_EFFECT | REMOVE_EFFECT => {
                    // The values decode last first: the effect's number is the last.
                    let bytes = a.map(|a| a.0).unwrap_or_default();
                    let eid = match args::decode(&bytes, lib)
                        .ok()
                        .and_then(|v| v.last().cloned())
                    {
                        Some(Arg::Int(e)) => e,
                        _ => continue,
                    };
                    if let Some(g) = given.as_mut() {
                        if function == ADD_EFFECT {
                            g.insert(eid);
                        } else {
                            g.remove(&eid);
                        }
                    }
                }
                _ => {}
            }
        }
        let Some(g) = given.as_ref() else { return };
        if !noticed {
            return;
        }
        c.refresh_effects();
        let list = c.effects();
        asked += 1;
        listed += list.nodes.len();
        let mut eids = BTreeSet::new();
        for n in &list.nodes {
            if n.eid == 0 || n.name.is_empty() || n.icon == 0 || !eids.insert(n.eid) {
                problems.push(format!("capture {capture}: {n:?}"));
            }
            if !g.contains(&n.eid) {
                problems.push(format!("capture {capture}: effect {} not given", n.eid));
            }
            if f64::from_bits(n.duration) != -1.0 {
                timed += 1;
            }
            if n.pulsed() {
                pulsed += 1;
            }
        }
    })
    .expect("the index reads");
    eprintln!("{asked} effect changes, {listed} effects listed ({timed} timed, {pulsed} pulsed)");
    problems.truncate(40);
    assert!(asked >= 1000, "only {asked} effect changes");
    assert!(listed >= 10_000, "only {listed} effects listed");
    assert!(timed >= 1000, "only {timed} timed effects listed");
    assert!(problems.is_empty(), "{problems:#?}");
}

/// A description of an object made: the server's creation of an object, and its new description.
const CREATE_OBJECT: u32 = 0x10058;
const UPDATE_VISUAL_DESC: u32 = 0x1009F;

/// The visual descriptions the servers sent, as a message carries them.
fn sent_visual(m: &ServerMessage) -> Option<&auberean_protocol::types::VisualDesc> {
    match m {
        ServerMessage::CreateObject(c) => Some(&c.visual),
        ServerMessage::UpdateVisualDesc(e) => Some(&e.action),
        _ => None,
    }
}

/// Every visual description the 2005, 2013 and 2017 servers sent (in object creations and new
/// descriptions; 174,401, none naming a stored record) reads into the client's own form and packs
/// again to the same bytes: the fields marked present are those that differ from an empty
/// description's, the tables in the order sent.
///
/// Behaviour: script.looks.a-description-packs-again-as-it-was-sent
#[test]
fn every_sent_description_packs_again_as_it_was_sent() {
    use auberean_client_runtime::looks::{held, pack};
    use auberean_protocol::wire::{Field, Writer};
    let index = auberean_testkit::data::require_capture_index();
    let (mut sent, mut named, mut same) = (0usize, 0usize, 0usize);
    let mut differ: Vec<String> = Vec::new();
    captures::messages(
        &index,
        &[CREATE_OBJECT, UPDATE_VISUAL_DESC],
        &[2005, 2013, 2017],
        |m| {
            if !m.to_client {
                return;
            }
            let Ok(msg) = ServerMessage::decode(&m.raw) else {
                return;
            };
            let Some(v) = sent_visual(&msg) else { return };
            sent += 1;
            if v.bits & auberean_protocol::types::VisualDesc::DID != 0 {
                named += 1;
                return;
            }
            let mut w = Writer::default();
            v.write(&mut w);
            let bytes = w.finish();
            let again = pack(&held(v, Some));
            if again == bytes {
                same += 1;
            } else if differ.len() < 10 {
                differ.push(format!("{}#{}: {:#x}", m.session, m.index, v.bits));
            }
        },
    )
    .expect("the index reads");
    eprintln!("descriptions sent {sent}: naming a record {named}, packed again the same {same}");
    assert!(sent > 100_000, "{sent}");
    assert_eq!(same + named, sent, "{differ:?}");
}

/// What the replay of the objects' own descriptions counted.
#[derive(Debug, Default)]
struct Dressing {
    /// Objects with a body at the runs' ends; of those, told to move by the server, and of
    /// those, with a motion interpreter; the others with one.
    bodies: usize,
    moved: usize,
    moved_interpreting: usize,
    still_interpreting: usize,
    /// Objects whose own description, before any change, answers what the server sent (packed
    /// again the same); those extending an entity, extending the entity's visual description
    /// instead; and those naming a record.
    as_sent: usize,
    entity_parent: usize,
    named: usize,
    /// A change added, then taken by the body: the change answered and the description the body
    /// took unchanged until then; then taken out again.
    changed: usize,
    taken: usize,
    taken_out: usize,
    /// Started again from the physics record: could, and could not.
    started: usize,
    not_started: usize,
    /// New descriptions from the server that replaced a change the scripts had made, and changes
    /// still standing at the run's end (the server's description held back for the object).
    replaced: usize,
    still_changed: usize,
}

/// An object's own description as its scripts read and change it. Over the 2017 runs in which
/// the server sent objects new descriptions: every object the server told to move has a motion
/// interpreter (its description's total names a motion description), and the player too; an
/// object's own description, unchanged, answers the one the server sent; a description added to
/// it is answered at once while the body keeps the one it took, until the body takes the change
/// (the object's description is then the change, packed); taken out again, it is gone; started
/// again from the object's physics record, it extends the visual description the record names;
/// and a description the server sends, once it takes effect, replaces a change the scripts made
/// and the body had not taken. (The 2026-10-31 count, 12 runs: 1,242 bodies, 184 told to move,
/// 803 of the others with a motion interpreter too; 405 changes replaced, 39 still standing where
/// the object's ordered stream held the server's description back or dropped it.)
///
/// Behaviour: script.looks.an-objects-own-description-is-read-changed-and-taken-as-its-scripts-ask
#[test]
fn an_objects_own_description_is_read_changed_and_taken_as_its_scripts_ask() {
    use auberean_client_runtime::looks::{self, pack, Records};
    use auberean_world::look::{Keyed, Look, GLOBAL_KEY};
    use std::collections::BTreeMap;
    // A change: one appearance value for every part group.
    const TABLE: u32 = 0x2000_01CD;
    const KEY: u32 = 0x4100_0002;
    let mut change = Look::default();
    let mut values = Keyed::default();
    values.set(KEY, 0.5);
    change.global.augment_appearance(TABLE, &values);

    let data = data().expect("the September 2013 install opens");
    let index = auberean_testkit::data::require_capture_index();
    let mut runs: BTreeMap<i64, ()> = BTreeMap::new();
    captures::messages(&index, &[UPDATE_VISUAL_DESC], &[2017], |m| {
        if m.to_client && runs.len() < 12 {
            runs.insert(m.capture, ());
        }
    })
    .expect("the index reads");
    let mut types: Vec<u32> = (0x1_0000..0x1_0100)
        .filter(|t| ServerMessage::is_known_type(*t))
        .collect();
    types.push(PLAY_CHARACTER);
    let mut n = Dressing::default();
    for run in runs.keys() {
        let mut c = client(&data);
        let mut frame = f64::NEG_INFINITY;
        let mut moved: BTreeSet<u64> = BTreeSet::new();
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
            match &msg {
                ServerMessage::DoMode(e) | ServerMessage::MotionInterpChange(e) => {
                    moved.insert(e.sender.id);
                }
                _ => {}
            }
            // The scripts change the object's own description just before the server sends it
            // a new one.
            let renewed = match &msg {
                ServerMessage::UpdateVisualDesc(e)
                    if c.model.object(e.sender.id).is_some_and(|o| o.in_world) =>
                {
                    let id = e.sender.id;
                    c.with_host(|h| looks::apply_additive(h, id, &change))
                        .expect("the scripts")
                        .then_some(id)
                }
                _ => None,
            };
            c.take_message(m.blob_id, msg);
            if let Some(o) = renewed {
                let s = c.scripts.as_ref().expect("the scripts");
                if !s.host.looks.contains_key(&o) {
                    n.replaced += 1;
                }
            }
        })
        .expect("the index reads");
        let Some(player) = c.model.player.filter(|p| c.model.object(*p).is_some()) else {
            continue;
        };
        let bodies: Vec<u64> = c
            .model
            .objects()
            .filter(|o| o.in_world || o.id == player)
            .map(|o| o.id)
            .collect();
        c.with_host(|h| {
            assert!(looks::has_motion_interpreter(h, player), "the player");
            for id in bodies {
                n.bodies += 1;
                // A change the scripts made whose new description from the server is still held
                // for the object: the change stands.
                if h.looks.remove(&id).is_some() {
                    n.still_changed += 1;
                }
                let interpreting = looks::has_motion_interpreter(h, id);
                if moved.contains(&id) {
                    n.moved += 1;
                    n.moved_interpreting += usize::from(interpreting);
                } else {
                    n.still_interpreting += usize::from(interpreting);
                }
                let sent = h.model.object(id).expect("here").visual.clone();
                let own = looks::local(h, id);
                if sent.bits & auberean_protocol::types::VisualDesc::DID != 0 {
                    n.named += 1;
                } else if divine_type_of(sent.parent) == ENTITY {
                    let db = auberean_script::Host::database(h).expect("the data files");
                    let want = Records(db).entity_visual(auberean_primitives::DataId(sent.parent));
                    if Some(own.parent) == want {
                        n.entity_parent += 1;
                    }
                } else {
                    let mut w = auberean_protocol::wire::Writer::default();
                    auberean_protocol::wire::Field::write(&sent, &mut w);
                    if pack(&own) == w.finish() {
                        n.as_sent += 1;
                    }
                }
                // A change answered at once, the body keeping what it took.
                assert!(looks::apply_additive(h, id, &change), "{id:#x}");
                let changed = looks::local(h, id);
                if changed.applied_value(GLOBAL_KEY, TABLE, KEY) == Some(0.5)
                    && h.model.object(id).expect("here").visual == sent
                {
                    n.changed += 1;
                }
                // Taken by the body: the object's description is the change.
                let _ = looks::apply_to_body(h, id);
                if h.model.object(id).expect("here").visual == looks::wire_of(&changed)
                    && !h.looks.contains_key(&id)
                {
                    n.taken += 1;
                }
                assert!(looks::apply_subtractive(h, id, &change), "{id:#x}");
                if looks::local(h, id)
                    .applied_value(GLOBAL_KEY, TABLE, KEY)
                    .is_none()
                {
                    n.taken_out += 1;
                }
                // Started again from the physics record.
                if looks::init(h, id) {
                    let record = auberean_client_runtime::repository::answer(
                        h,
                        &auberean_client_runtime::repository::DATA_ID,
                        id,
                        1,
                    )
                    .unwrap_or(0);
                    let want = {
                        let db = auberean_script::Host::database(h).expect("the data files");
                        let r = Records(db);
                        match divine_type_of(record) {
                            ENTITY => db
                                .get::<auberean_assets::entity::Entity>(record)
                                .and_then(|e| e.object)
                                .and_then(|o| r.entity_visual(auberean_primitives::DataId(o))),
                            _ => r.entity_visual(auberean_primitives::DataId(record)),
                        }
                    };
                    let own = looks::local(h, id);
                    assert_eq!(Some(own.parent), want, "{id:#x}");
                    assert!(own.groups.is_empty() && own.global.appearance.is_empty());
                    n.started += 1;
                } else {
                    n.not_started += 1;
                }
                h.looks.remove(&id);
            }
        })
        .expect("the scripts");
    }
    eprintln!("runs {}: {n:?}", runs.len());
    assert!(n.bodies > 100, "{n:?}");
    assert!(n.moved > 20 && n.moved_interpreting == n.moved, "{n:?}");
    assert!(n.still_interpreting < n.bodies - n.moved, "{n:?}");
    assert_eq!(n.as_sent + n.entity_parent + n.named, n.bodies, "{n:?}");
    assert_eq!(
        (n.changed, n.taken, n.taken_out),
        (n.bodies, n.bodies, n.bodies),
        "{n:?}"
    );
    assert!(n.started > 0, "{n:?}");
    assert!(n.replaced > 0, "{n:?}");
}

/// The record type of an entity, and a record's type.
const ENTITY: u32 = 0x4000_0049;
fn divine_type_of(id: u32) -> u32 {
    auberean_dat::qualified::divine_type(id)
}

/// Call native `id` as a script calls it: a frame whose receiver is `receiver`, whose arguments
/// (first first) follow it, over `results` result slots; `locals` are pushed first, slots an out
/// argument may reach (`Out(n)` stands for a reference to local `n`). Returns how it went and
/// the context, its result slots at the bottom after the locals.
fn run_native(
    s: &mut auberean_client_runtime::host::Scripts,
    id: u32,
    locals: &[u32],
    receiver: u32,
    args: &[NativeArg],
    results: usize,
) -> (
    Result<auberean_script::Status, auberean_script::Stop>,
    auberean_script::context::Context,
) {
    use auberean_script::value::{tag, NULL};
    use auberean_script::Binding;
    let mut cx = auberean_script::context::Context::new(usize::MAX, 0);
    for l in locals {
        cx.push(*l, tag::REFERENCE).expect("a local");
    }
    for _ in 0..results {
        cx.push(0, tag::INT).expect("a result slot");
    }
    for a in args.iter().rev() {
        match a {
            NativeArg::Word(v) => cx.push(*v, tag::MIXED).expect("an argument"),
            NativeArg::Ref(r) => cx.push(*r, tag::REFERENCE).expect("an argument"),
            NativeArg::Out(n) => {
                let r =
                    s.vm.heap
                        .add_relative(NULL, u32::try_from(*n).expect("small"));
                cx.push(r, tag::REFERENCE).expect("an argument");
            }
        }
    }
    cx.push(receiver, tag::REFERENCE).expect("the receiver");
    cx.enter(cx.ip, 0).expect("a frame");
    cx.fp = cx.sp;
    let sf = cx.sf;
    cx.frames[sf].fp = cx.sp;
    let Some(Binding::Ported(f)) = s.host.program.table.get(id) else {
        return (
            Err(auberean_script::Stop::NotPorted {
                native: id,
                port: "",
            }),
            cx,
        );
    };
    let r = {
        let mut call = auberean_script::Call {
            vm: &mut s.vm,
            cx: &mut cx,
            host: &mut s.host,
            native: id,
        };
        f(&mut call)
    };
    (r, cx)
}

/// An argument of a native called by [`run_native`].
#[derive(Clone, Copy)]
enum NativeArg {
    Word(u32),
    Ref(u32),
    Out(usize),
}

/// What the descriptions scripts hold answered.
#[derive(Debug, Default)]
struct Held {
    descriptions: usize,
    /// With an icon as drawn, and those the icon native answered alike (layers and the answer);
    /// drawable as the check of the icon answers.
    with_icon: usize,
    icon_same: usize,
    drawable_same: usize,
    /// The scale answered as held; the appearance values of every group read back, added to an
    /// empty description and read back the same; cleared and then adding nothing.
    scale_same: usize,
    appearance_same: usize,
    cleared: usize,
    /// With an appearance value for every group; the first of them held on its own, given to an
    /// empty description and taken off again as it was.
    with_value: usize,
    value_same: usize,
    differ: Vec<String>,
}

/// A visual description a script holds answers as the client's own form does: over every
/// description the 2017 servers sent (none naming a stored record), its icon is the one the
/// inventory draws the object with (the layers, and whether it has one), the icon's check says
/// whether it can be drawn as the inventory's does, its scale is its own, the appearance values it
/// gives every part group read back, and added to an empty description read back the same; one
/// such value held on its own reads back, is given to an empty description and taken off again;
/// cleared, it adds nothing.
///
/// Behaviour: script.looks.a-held-description-answers-as-its-own-form
#[test]
fn a_held_description_answers_its_icon_scale_and_appearance_as_its_own_form() {
    use auberean_client_runtime::looks::{
        appearance_of, desc_of, held, icon_of, Records, ScriptLook,
    };
    use auberean_world::look::{icon_is_valid, total_icon, Look, GLOBAL_KEY};
    const VISUAL_DESC: u32 = 0x71;
    const TOTAL_ICON: u32 = VISUAL_DESC << 16 | 0x0C;
    const ADDS_NOTHING: u32 = VISUAL_DESC << 16 | 0x02;
    const SCALE: u32 = VISUAL_DESC << 16 | 0x08;
    const CLEAR: u32 = VISUAL_DESC << 16 | 0x01;
    const GROUP_APPEARANCE: u32 = VISUAL_DESC << 16 | 0x0A;
    const ADD_GROUP_APPEARANCE: u32 = VISUAL_DESC << 16 | 0x0B;
    const DRAWABLE: u32 = 0x2E << 16 | 0x02;
    const ADD_KEY: u32 = VISUAL_DESC << 16 | 0x03;
    const REMOVE_KEY: u32 = VISUAL_DESC << 16 | 0x04;
    const KEY: u32 = 0x10 << 16 | 0x01;
    const SET_KEY: u32 = 0x10 << 16 | 0x02;
    const STRENGTH: u32 = 0x10 << 16 | 0x03;
    const SET_STRENGTH: u32 = 0x10 << 16 | 0x04;
    let data = data().expect("the September 2013 install opens");
    let index = auberean_testkit::data::require_capture_index();
    let mut c = client(&data);
    let s = c.scripts.as_mut().expect("the scripts");
    let mut n = Held::default();
    captures::messages(&index, &[CREATE_OBJECT, UPDATE_VISUAL_DESC], &[2017], |m| {
        if !m.to_client {
            return;
        }
        let Ok(msg) = ServerMessage::decode(&m.raw) else {
            return;
        };
        let Some(v) = sent_visual(&msg) else { return };
        if v.bits & auberean_protocol::types::VisualDesc::DID != 0 {
            return;
        }
        n.descriptions += 1;
        let look = held(v, Some);
        let want = {
            let db = auberean_script::Host::database(&mut s.host).expect("the data files");
            let records = Records(db);
            let desc = desc_of(v, Some);
            let own: Vec<auberean_assets::visual::IconLayer> = v
                .icon
                .iter()
                .map(|l| auberean_assets::visual::IconLayer {
                    layer: l.0,
                    picture: auberean_primitives::DataId(l.1),
                    shift_color: auberean_assets::common::Color32(l.2),
                })
                .collect();
            total_icon(&records, desc.parent, &own, &desc.global)
        };
        let new_look = |s: &mut auberean_client_runtime::host::Scripts, l: Look| {
            s.vm.heap.add_native(VISUAL_DESC, ScriptLook::native(l))
        };
        let r = new_look(s, look.clone());
        // The icon, into a new icon description an out argument reaches.
        let prog = std::sync::Arc::clone(&s.host.program);
        let icon = s.vm.new_native(0x2E, &prog);
        let (how, cx) = run_native(s, TOTAL_ICON, &[icon], r, &[NativeArg::Out(0)], 1);
        assert!(how.is_ok(), "the icon: {how:?}");
        let got = icon_of(s.vm.heap.native(icon).expect("the icon"));
        let answer = cx.data[1];
        if !want.is_empty() {
            n.with_icon += 1;
        }
        if got == want && answer == u32::from(!want.is_empty()) {
            n.icon_same += 1;
        } else if n.differ.len() < 8 {
            n.differ.push(format!("icon {}#{}", m.session, m.index));
        }
        let (how, cx) = run_native(s, DRAWABLE, &[], icon, &[], 1);
        assert!(how.is_ok(), "the check: {how:?}");
        n.drawable_same += usize::from(cx.data[0] == u32::from(icon_is_valid(&want)));
        // The scale, a new vector.
        let (how, cx) = run_native(s, SCALE, &[], r, &[], 1);
        assert!(how.is_ok(), "the scale: {how:?}");
        let scale = match s.vm.heap.native(cx.data[0]) {
            Ok(auberean_script::native_object::NativeObject::Vector(w)) => *w,
            other => panic!("the scale: {other:?}"),
        };
        let s3 = look.scale;
        n.scale_same += usize::from(scale == [s3.x.to_bits(), s3.y.to_bits(), s3.z.to_bits()]);
        // The appearance values every group is given, read back and added to an empty one.
        let table = s.vm.new_native(0x0F, &prog);
        let (how, cx) = run_native(
            s,
            GROUP_APPEARANCE,
            &[table],
            r,
            &[NativeArg::Word(GLOBAL_KEY), NativeArg::Out(0)],
            1,
        );
        assert!(how.is_ok(), "the appearance: {how:?}");
        let read = appearance_of(s.vm.heap.native(table).expect("the table"));
        let empty = new_look(s, Look::default());
        let (how, _) = run_native(
            s,
            ADD_GROUP_APPEARANCE,
            &[],
            empty,
            &[NativeArg::Word(GLOBAL_KEY), NativeArg::Ref(table)],
            1,
        );
        assert!(how.is_ok(), "adding the appearance: {how:?}");
        let again = s.vm.new_native(0x0F, &prog);
        let (how, _) = run_native(
            s,
            GROUP_APPEARANCE,
            &[again],
            empty,
            &[NativeArg::Word(GLOBAL_KEY), NativeArg::Out(0)],
            1,
        );
        assert!(how.is_ok(), "the appearance again: {how:?}");
        let sorted = |k: &auberean_world::look::Keyed<auberean_world::look::Keyed<f32>>| {
            let mut v: Vec<(u32, Vec<(u32, u32)>)> = k
                .entries
                .iter()
                .map(|(a, vals)| {
                    let mut e: Vec<(u32, u32)> = vals
                        .entries
                        .iter()
                        .map(|(k, f)| (*k, f.to_bits()))
                        .collect();
                    e.sort_unstable();
                    (*a, e)
                })
                .collect();
            v.sort();
            v
        };
        let added = appearance_of(s.vm.heap.native(again).expect("the table"));
        if cx.data[1] == 1
            && read.entries == look.global.appearance.entries
            && sorted(&added) == sorted(&read)
        {
            n.appearance_same += 1;
        } else if n.differ.len() < 8 {
            n.differ
                .push(format!("appearance {}#{}", m.session, m.index));
        }
        // One appearance value, held on its own, given to an empty description and taken off.
        if let Some((apr, key, value)) = look
            .global
            .appearance
            .entries
            .first()
            .and_then(|(a, v)| v.entries.first().map(|(k, f)| (*a, *k, *f)))
        {
            n.with_value += 1;
            let applied = s.vm.new_native(0x10, &prog);
            for (id, v) in [(SET_KEY, key), (SET_STRENGTH, value.to_bits())] {
                let (how, _) = run_native(s, id, &[], applied, &[NativeArg::Word(v)], 0);
                assert!(how.is_ok(), "setting a value: {how:?}");
            }
            let (_, k) = run_native(s, KEY, &[], applied, &[], 1);
            let (_, f) = run_native(s, STRENGTH, &[], applied, &[], 1);
            let one = new_look(s, Look::default());
            let args = [
                NativeArg::Word(GLOBAL_KEY),
                NativeArg::Word(apr),
                NativeArg::Ref(applied),
            ];
            let (how, added) = run_native(s, ADD_KEY, &[], one, &args, 1);
            assert!(how.is_ok(), "adding a value: {how:?}");
            let given = |s: &mut auberean_client_runtime::host::Scripts| {
                let o = s.vm.heap.native(one).expect("the description");
                ScriptLook::of(o).and_then(|l| l.applied_value(GLOBAL_KEY, apr, key))
            };
            let had = given(s);
            let (how, removed) = run_native(s, REMOVE_KEY, &[], one, &args, 1);
            assert!(how.is_ok(), "taking a value off: {how:?}");
            if k.data[0] == key
                && f.data[0] == value.to_bits()
                && added.data[0] == 1
                && had.map(f32::to_bits) == Some(value.to_bits())
                && removed.data[0] == 1
                && given(s).is_none()
            {
                n.value_same += 1;
            } else if n.differ.len() < 8 {
                n.differ.push(format!("value {}#{}", m.session, m.index));
            }
        }
        // Cleared, it adds nothing.
        let (how, _) = run_native(s, CLEAR, &[], r, &[], 0);
        assert!(how.is_ok(), "clearing: {how:?}");
        let (how, cx) = run_native(s, ADDS_NOTHING, &[], r, &[], 1);
        assert!(how.is_ok(), "adds nothing: {how:?}");
        n.cleared += usize::from(cx.data[0] == 1);
        s.vm.collect();
    })
    .expect("the index reads");
    eprintln!("{n:?}");
    assert!(n.descriptions > 50_000, "{n:?}");
    assert!(n.with_icon > 10_000, "{n:?}");
    assert!(n.with_value > 1_000, "{n:?}");
    assert_eq!(n.value_same, n.with_value, "{n:?}");
    for same in [
        n.icon_same,
        n.drawable_same,
        n.scale_same,
        n.appearance_same,
        n.cleared,
    ] {
        assert_eq!(same, n.descriptions, "{n:?}");
    }
}
