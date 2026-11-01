//! The behaviors and modes the 2017 sessions played on objects, played on bodies put together
//! from the objects' visual descriptions.

use std::collections::{BTreeMap, HashMap};

use auberean_animation::host::{error, Event};
use auberean_animation::look::{Desc, Group, Keyed, GLOBAL_KEY};
use auberean_animation::queries::TemplateQueries;
use auberean_animation::track::Params;
use auberean_animation::{Body, BodyRecords, CachedRecords, Look};
use auberean_dat::locate::{install_dats, DatName};
use auberean_dat::testing::{require_install, TARGET};
use auberean_dat::DatFile;
use auberean_primitives::{DataId, Vec3};
use auberean_protocol::table::{Table, BUCKET_COUNTS};
use auberean_protocol::types::{
    BehaviorParams as Wire, PartGroupDesc as WireGroup, VisualDesc as WireVisual,
};
use auberean_protocol::ServerMessage;
use auberean_world::place::PlaceRecords;
use auberean_world::records::WorldRecords;

const CREATE: u32 = 0x10058;
const BEHAVIOR: u32 = 0x1007F;
const MODE: u32 = 0x10082;

/// The longest a body is stepped between two recorded events, in seconds.
const MOST_ELAPSED: f64 = 5.0;
/// The step it is advanced by.
const STEP: f64 = 0.1;

fn open() -> WorldRecords {
    let dir = require_install(TARGET);
    let mut files: Vec<(DatName, DatFile)> = install_dats(&dir)
        .expect("list the install")
        .into_iter()
        .filter_map(|(n, p)| DatFile::open(&p).ok().map(|d| (n, d)))
        .collect();
    let mut take = |name: &DatName| {
        let k = files.iter().position(|(n, _)| n == name)?;
        Some(files.swap_remove(k).1)
    };
    let portal = take(&DatName::Portal).expect("a portal file");
    let cell = take(&DatName::Cell(1)).expect("a cell file");
    let highres = take(&DatName::HighRes);
    let ctx = auberean_assets::Context::from_portal(&portal).expect("the property list");
    WorldRecords::new(portal, cell, highres, ctx)
}

/// The parameters a message carries, each field it does not send at its default.
fn params(w: &Wire) -> Params {
    let has = |b: u32| w.bits & b != 0;
    let mut p = Params::default();
    if has(Wire::BEHAVIOR) {
        p.behavior = w.behavior;
    }
    if has(Wire::TIME_SCALE) {
        p.time_scale = w.time_scale;
    }
    if has(Wire::MODE) {
        p.mode = w.mode;
    }
    if has(Wire::HOLD_CYCLES) {
        p.hold_cycles = w.hold_cycles;
    }
    if has(Wire::FX_SCRIPT) {
        p.fx_script = w.fx_script;
    }
    if has(Wire::FX) {
        p.fx = w.fx;
    }
    if has(Wire::TARGET) {
        p.target = w.target;
    }
    if has(Wire::CAMERA) {
        p.camera_target = w.camera_target;
        p.camera_behavior = w.camera_behavior;
    }
    if has(Wire::CLONE) {
        p.clone_visual = w.clone_visual;
        p.clone_appearance = w.clone_appearance;
    }
    if has(Wire::IMPULSE) {
        p.impulse = w.impulse;
    }
    p.set_words(w.bits, &w.words);
    if has(Wire::CONTEXT) {
        p.context = w.context;
    }
    p
}

fn keyed<V: Clone>(t: &Table<V>) -> Keyed<V> {
    let buckets = BUCKET_COUNTS
        .get(usize::from(t.size_class))
        .copied()
        .unwrap_or(BUCKET_COUNTS[0]);
    Keyed::stored(buckets, t.entries.clone())
}

/// A part group as a description sends it: only the fields it carries.
fn group_of(g: &WireGroup, key: u32) -> Group {
    let has = |bit: u32| g.bits & bit != 0;
    let id = |bit: u32, v: u32| DataId(if has(bit) { v } else { 0 });
    let appearance = if has(WireGroup::APPEARANCE) {
        let buckets = BUCKET_COUNTS
            .get(usize::from(g.appearance.size_class))
            .copied()
            .unwrap_or(BUCKET_COUNTS[0]);
        let entries = g
            .appearance
            .entries
            .iter()
            .map(|(apr, keys)| (*apr, keyed(keys)))
            .collect();
        Keyed::stored(buckets, entries)
    } else {
        Keyed::default()
    };
    Group {
        key,
        parent_key: if has(WireGroup::PARENT_KEY) {
            g.parent_key
        } else {
            0
        },
        connection: if has(WireGroup::CONNECTION_POINT) {
            g.connection_point
        } else {
            0
        },
        setup: id(WireGroup::SETUP, g.setup),
        anim_map: id(WireGroup::ANIM_MAP, g.anim_map),
        appearance,
        fx_table: id(WireGroup::FX_TABLE, g.fx_table),
        startup_fx: if has(WireGroup::STARTUP_FX) {
            keyed(&g.startup_fx)
        } else {
            Keyed::default()
        },
        ..Group::default()
    }
}

/// A description as the client holds one it was sent: its record (an entity's object followed to
/// its description) as the parent it starts from, its tables, its changes to every group and its
/// own part groups hung from their parents by key.
fn desc_of(world: &WorldRecords, v: &WireVisual) -> Desc {
    let has = |bit: u32| v.bits & bit != 0;
    let id = |bit: u32, x: u32| DataId(if has(bit) { x } else { 0 });
    let mut parent = id(WireVisual::PARENT, v.parent);
    if parent.0 >> 24 == 0x47 {
        parent = world
            .entity(parent)
            .and_then(|e| e.object)
            .map_or(DataId::NONE, DataId);
    }
    let mut groups: Vec<Group> = if has(WireVisual::PART_GROUPS) {
        v.part_groups
            .entries
            .iter()
            .map(|(k, g)| {
                let key = if g.bits & WireGroup::KEY != 0 {
                    g.key
                } else {
                    *k
                };
                group_of(g, key)
            })
            .collect()
    } else {
        Vec::new()
    };
    for i in 0..groups.len() {
        let pk = groups[i].parent_key;
        if pk == 0 {
            continue;
        }
        if let Some(p) = groups.iter().position(|g| g.key == pk) {
            let child = groups[i].key;
            groups[p].children.push(child);
            groups[i].parent = Some(pk);
        }
    }
    let mut global = if has(WireVisual::GLOBAL) {
        group_of(&v.global, GLOBAL_KEY)
    } else {
        Group::default()
    };
    global.key = GLOBAL_KEY;
    let f = |bit: u32, x: f32| if has(bit) { x } else { 1.0 };
    Desc {
        parent,
        motion_desc: id(WireVisual::MOTION_INTERP, v.motion_interp),
        behavior_table: id(WireVisual::BEHAVIOR_TABLE, v.behavior_table),
        modes: id(WireVisual::MODES, v.modes),
        scale: if has(WireVisual::SCALE) {
            v.scale
        } else {
            Vec3::new(1.0, 1.0, 1.0)
        },
        child_scale: f(WireVisual::CHILD_SCALE, v.child_scale),
        particle_scale: f(WireVisual::PARTICLE_SCALE, v.particle_scale),
        global,
        groups,
        icon: Vec::new(),
    }
}

struct Object {
    body: Option<Body>,
    time: f64,
}

/// Step a recorded object's body by the time since its last event (at most five seconds).
fn step(o: &mut Object, now: f64, recs: &CachedRecords<'_>) {
    let mut left = (now - o.time).clamp(0.0, MOST_ELAPSED);
    o.time = now;
    if let Some(b) = o.body.as_mut() {
        while left > 0.0 {
            #[allow(clippy::cast_possible_truncation)]
            let dt = left.min(STEP) as f32;
            b.advance_unseen(recs, dt);
            left -= STEP;
        }
        b.take_events();
        b.triggers.clean_up();
    }
}

/// Every behavior a 2017 session played on an object it created plays on a body put together from
/// the object's visual description as the client holds it (the record it starts from with the
/// description's own tables, part groups and appearance over it), but for those its table or the
/// template does not have, those already playing, and some whose animation is missing or of no
/// length (which the client refuses too); every mode set on one has a way there along its table,
/// or is the mode it is headed for. Bodies are stepped by the time between events (at most five
/// seconds).
///
/// Behaviour: animation.corpus.recorded-behaviors-and-modes-play
#[test]
fn recorded_behaviors_and_modes_play() {
    let index = auberean_testkit::data::require_capture_index();
    let world = open();
    let recs = CachedRecords::new(&world);
    let template = recs.character_template().expect("the character template");
    let mut made: HashMap<String, Option<Body>> = HashMap::new();
    let mut objects: HashMap<u64, Object> = HashMap::new();
    let mut session = -1i64;
    let mut played: BTreeMap<&str, u64> = BTreeMap::new();
    let mut modes: BTreeMap<&str, u64> = BTreeMap::new();
    let mut no_animation: BTreeMap<u32, u64> = BTreeMap::new();
    let n = auberean_testkit::captures::messages(&index, &[CREATE, BEHAVIOR, MODE], &[2017], |m| {
        if m.session != session {
            session = m.session;
            objects.clear();
        }
        let Ok(msg) = ServerMessage::decode(&m.raw) else {
            return;
        };
        match msg {
            ServerMessage::CreateObject(c) => {
                let v: &WireVisual = &c.visual;
                let body = made
                    .entry(format!("{v:?}"))
                    .or_insert_with(|| {
                        let look = if v.bits & WireVisual::DID != 0 && v.did != 0 {
                            Look::of_record(&recs, DataId(v.did)).ok()?
                        } else {
                            let desc = desc_of(&world, v);
                            if desc.parent.is_none() && desc.groups.is_empty() {
                                return None;
                            }
                            Look::of(&recs, &desc).ok()?
                        };
                        Body::from_look(&recs, &look)
                    })
                    .clone();
                objects.insert(c.id, Object { body, time: m.time });
            }
            ServerMessage::PlayBehavior(e) => {
                if e.action.bits & Wire::BEHAVIOR == 0 {
                    return;
                }
                let Some(o) = objects.get_mut(&e.sender.id) else {
                    *played.entry("unknown object").or_default() += 1;
                    return;
                };
                step(o, m.time, &recs);
                let Some(body) = o.body.as_mut() else {
                    *played.entry("no body").or_default() += 1;
                    return;
                };
                if !body.modes.can_animate {
                    *played.entry("no table").or_default() += 1;
                    return;
                }
                let p = params(&e.action);
                let b = p.behavior;
                let before = body
                    .modes
                    .to
                    .as_ref()
                    .or(body.modes.from.as_ref())
                    .and_then(|t| t.track(b))
                    .is_some_and(|t| !t.ramping_down);
                let why = if body.do_behavior(&recs, &p) {
                    "played"
                } else if template.behavior(b).is_none() {
                    "template lacks it"
                } else if before {
                    "already playing"
                } else if body.combo(b).is_none() {
                    "table lacks it"
                } else {
                    *no_animation.entry(b).or_default() += 1;
                    "no animation"
                };
                *played.entry(why).or_default() += 1;
            }
            ServerMessage::DoMode(e) => {
                let Some(o) = objects.get_mut(&e.sender.id) else {
                    *modes.entry("unknown object").or_default() += 1;
                    return;
                };
                step(o, m.time, &recs);
                let Some(body) = o.body.as_mut() else {
                    *modes.entry("no body").or_default() += 1;
                    return;
                };
                if !body.modes.can_animate {
                    *modes.entry("no table").or_default() += 1;
                    return;
                }
                let mode = e.action.0;
                let already = body.modes.desired_mode == mode;
                let ok = body.do_mode(&recs, 1, mode);
                let failed = body.take_events().iter().any(|ev| {
                    matches!(ev, Event::ModeDone { context: 1, error: x } if *x == error::NO_ANIMATION)
                });
                let why = if already {
                    "already headed there"
                } else if ok && !failed {
                    "a way there"
                } else {
                    "no way"
                };
                *modes.entry(why).or_default() += 1;
            }
            _ => {}
        }
    });
    let n = n.expect("the capture index reads");
    eprintln!("{n} messages; behaviors {played:?}; modes {modes:?}");
    eprintln!("behaviors with no animation: {no_animation:x?}");
    let count = |m: &BTreeMap<&str, u64>, k: &str| m.get(k).copied().unwrap_or(0);
    assert!(count(&played, "played") > 42_000, "{played:?}");
    assert_eq!(count(&played, "no body"), 0, "{played:?}");
    // Behaviors whose combination names an animation the object's maps lack, or a clip of no
    // length: the client refuses those too. The descriptions' own part groups change none of the
    // bodies their records already make; they make the bodies of objects whose records have none.
    assert!(count(&played, "no animation") <= 700, "{played:?}");
    assert!(count(&modes, "a way there") > 120_000, "{modes:?}");
    assert_eq!(count(&modes, "no way"), 0, "{modes:?}");
}
