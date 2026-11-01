//! Every visual description in the 2017 sessions, resolved.

use std::collections::{BTreeMap, HashMap, HashSet};

use auberean_assets::animation::{AllowedModes, AnimMap, BehaviorTable, MotionDesc};
use auberean_assets::fx::FxTable;
use auberean_assets::setup::Setup;
use auberean_assets::visual::VisualDesc;
use auberean_assets::{Context, Decode};
use auberean_dat::locate::{install_dats, DatName};
use auberean_dat::testing::{require_install, TARGET};
use auberean_dat::DatFile;
use auberean_protocol::types::{PartGroupDesc, VisualDesc as WireVisual};
use auberean_protocol::ServerMessage;
use auberean_world::records::WorldRecords;

/// The messages read: objects created, behaviors played, modes set, effects played.
const CREATE: u32 = 0x10058;
const BEHAVIOR: u32 = 0x1007F;
const MODE: u32 = 0x10082;
const FX: u32 = 0x100A0;
const LOOK: u32 = 0x1009F;

fn open(version: &str) -> Option<WorldRecords> {
    let dir = require_install(version);
    let mut files: Vec<(DatName, DatFile)> = install_dats(&dir)
        .ok()?
        .into_iter()
        .filter_map(|(n, p)| DatFile::open(&p).ok().map(|d| (n, d)))
        .collect();
    let mut take = |name: &DatName| {
        let k = files.iter().position(|(n, _)| n == name)?;
        Some(files.swap_remove(k).1)
    };
    let portal = take(&DatName::Portal)?;
    let cell = take(&DatName::Cell(1))?;
    let highres = take(&DatName::HighRes);
    let ctx = Context::from_portal(&portal).ok()?;
    Some(WorldRecords::new(portal, cell, highres, ctx))
}

/// The data files descriptions are resolved against: the end-of-retail set.
struct Installs(Vec<WorldRecords>);

impl Installs {
    fn get<T: Decode>(&self, id: u32) -> Option<T> {
        self.0.iter().find_map(|w| w.get::<T>(id))
    }
}

/// What one record id named by a description is, by the field that names it.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
enum Kind {
    Visual,
    Motion,
    Behaviors,
    Modes,
    Setup,
    AnimMap,
    FxTable,
}

/// Every record id a part group names.
fn group_ids(g: &PartGroupDesc, out: &mut Vec<(Kind, u32)>) {
    for (bit, kind, id) in [
        (PartGroupDesc::SETUP, Kind::Setup, g.setup),
        (PartGroupDesc::ANIM_MAP, Kind::AnimMap, g.anim_map),
        (PartGroupDesc::FX_TABLE, Kind::FxTable, g.fx_table),
    ] {
        if g.bits & bit != 0 && id != 0 {
            out.push((kind, id));
        }
    }
}

/// Every record id a description names.
fn ids(v: &WireVisual) -> Vec<(Kind, u32)> {
    let mut out = Vec::new();
    for (bit, kind, id) in [
        (WireVisual::DID, Kind::Visual, v.did),
        (WireVisual::PARENT, Kind::Visual, v.parent),
        (WireVisual::MOTION_INTERP, Kind::Motion, v.motion_interp),
        (
            WireVisual::BEHAVIOR_TABLE,
            Kind::Behaviors,
            v.behavior_table,
        ),
        (WireVisual::MODES, Kind::Modes, v.modes),
    ] {
        if v.bits & bit != 0 && id != 0 {
            out.push((kind, id));
        }
    }
    if v.bits & WireVisual::GLOBAL != 0 {
        group_ids(&v.global, &mut out);
    }
    if v.bits & WireVisual::PART_GROUPS != 0 {
        for (_, g) in &v.part_groups.entries {
            group_ids(g, &mut out);
        }
    }
    out
}

fn decodes(i: &Installs, kind: Kind, id: u32) -> bool {
    match kind {
        Kind::Visual => i.get::<VisualDesc>(id).is_some(),
        Kind::Motion => i.get::<MotionDesc>(id).is_some(),
        Kind::Behaviors => i.get::<BehaviorTable>(id).is_some(),
        Kind::Modes => i.get::<AllowedModes>(id).is_some(),
        Kind::Setup => i.get::<Setup>(id).is_some(),
        Kind::AnimMap => i.get::<AnimMap>(id).is_some(),
        Kind::FxTable => i.get::<FxTable>(id).is_some(),
    }
}

/// An object's tables: its behavior table and its effect tables.
#[derive(Clone, Debug, Default)]
struct Tables {
    behaviors: Option<u32>,
    fx: Vec<u32>,
}

/// The tables of a created object: those its description names, then those of the visual
/// description records it starts from and their ancestors.
fn tables(i: &Installs, v: &WireVisual) -> Tables {
    let mut t = Tables::default();
    if v.bits & WireVisual::BEHAVIOR_TABLE != 0 && v.behavior_table != 0 {
        t.behaviors = Some(v.behavior_table);
    }
    let mut groups: Vec<&PartGroupDesc> = v.part_groups.entries.iter().map(|(_, g)| g).collect();
    if v.bits & WireVisual::GLOBAL != 0 {
        groups.push(&v.global);
    }
    for g in groups {
        if g.bits & PartGroupDesc::FX_TABLE != 0 && g.fx_table != 0 {
            t.fx.push(g.fx_table);
        }
    }
    let mut next = [v.did, v.parent]
        .into_iter()
        .filter(|&d| d != 0)
        .collect::<Vec<_>>();
    let mut seen = HashSet::new();
    while let Some(d) = next.pop() {
        if !seen.insert(d) || seen.len() > 64 {
            continue;
        }
        let Some(rec) = i.get::<VisualDesc>(d) else {
            continue;
        };
        if t.behaviors.is_none() {
            t.behaviors = rec.behavior_table.map(|b| b.0);
        }
        let groups = rec
            .part_groups
            .iter()
            .flatten()
            .map(|(_, g)| g)
            .chain(rec.global.iter());
        for g in groups {
            if let Some(f) = g.fx_table {
                t.fx.push(f.0);
            }
        }
        if let Some(p) = rec.parent {
            next.push(p.0);
        }
    }
    t
}

/// Every effect id an effect table and the tables under it hold.
fn effects(
    i: &Installs,
    cache: &mut HashMap<u32, HashSet<u32>>,
    id: u32,
    depth: usize,
) -> HashSet<u32> {
    if let Some(hit) = cache.get(&id) {
        return hit.clone();
    }
    let mut out = HashSet::new();
    if depth < 8 {
        if let Some(t) = i.get::<FxTable>(id) {
            out.extend(t.effects.iter().map(|(k, _)| *k));
            for s in &t.sub_tables {
                out.extend(effects(i, cache, s.0, depth + 1));
            }
        }
    }
    cache.insert(id, out.clone());
    out
}

#[derive(Debug, Default)]
struct Count {
    seen: u64,
    ok: u64,
    /// Played on an object the session never created (it was created before the recording began).
    unknown_object: u64,
    /// The object's description names no table to check against.
    no_table: u64,
    failures: BTreeMap<u32, u64>,
}

/// Every object the 2017 sessions create names records that exist and decode (its visual
/// descriptions, motion description, behavior table, valid modes, and each part group's setup,
/// animation map and effect table); every mode set on an object is one its behavior table has, and
/// so is every behavior played on one but two; every effect played is in the object's own effect
/// tables or in another effect table, but for six effects that are in none. Objects created before
/// a recording began, and objects with no table, are counted and not judged.
///
/// Behaviour: animation.corpus.every-recorded-visual-description-resolves
#[test]
fn every_recorded_visual_description_resolves() {
    let index = auberean_testkit::data::require_capture_index();
    let worlds: Vec<WorldRecords> = [TARGET]
        .into_iter()
        .map(|v| open(v).unwrap_or_else(|| panic!("the {v} set's records do not open")))
        .collect();
    let installs = Installs(worlds);
    let mut checked: HashMap<(Kind, u32), bool> = HashMap::new();
    let mut table_cache: HashMap<u32, Option<BehaviorTable>> = HashMap::new();
    let mut fx_cache: HashMap<u32, HashSet<u32>> = HashMap::new();
    let mut objects: HashMap<u64, Tables> = HashMap::new();
    let mut session = -1i64;
    let (mut creates, mut named, mut undecoded) = (0u64, 0u64, BTreeMap::<(Kind, u32), u64>::new());
    let (mut behaviors, mut modes, mut fxs) =
        (Count::default(), Count::default(), Count::default());
    let mut undecodable_messages = 0u64;
    let n = auberean_testkit::captures::messages(
        &index,
        &[CREATE, BEHAVIOR, MODE, FX, LOOK],
        &[2017],
        |m| {
            if m.session != session {
                session = m.session;
                objects.clear();
            }
            let Ok(msg) = ServerMessage::decode(&m.raw) else {
                undecodable_messages += 1;
                return;
            };
            match msg {
                ServerMessage::CreateObject(c) => {
                    creates += 1;
                    for (kind, id) in ids(&c.visual) {
                        named += 1;
                        let ok = *checked
                            .entry((kind, id))
                            .or_insert_with(|| decodes(&installs, kind, id));
                        if !ok {
                            *undecoded.entry((kind, id)).or_default() += 1;
                        }
                    }
                    let t = tables(&installs, &c.visual);
                    objects.insert(c.id, t);
                }
                ServerMessage::UpdateVisualDesc(e) => {
                    let t = tables(&installs, &e.action);
                    if let Some(o) = objects.get_mut(&e.sender.id) {
                        if t.behaviors.is_some() {
                            o.behaviors = t.behaviors;
                        }
                        o.fx.extend(t.fx);
                    }
                }
                ServerMessage::PlayBehavior(e) => {
                    if e.action.bits & auberean_protocol::types::BehaviorParams::BEHAVIOR == 0 {
                        return;
                    }
                    let b = e.action.behavior;
                    judge(&mut behaviors, objects.get(&e.sender.id), b, |t| {
                        let table = table_of(&installs, &mut table_cache, t)?;
                        Some(
                            table.behaviors.iter().any(|(k, _)| *k == b)
                                || table.combos.iter().any(|(k, _)| k.behavior == b),
                        )
                    });
                }
                ServerMessage::DoMode(e) => {
                    let mode = e.action.0;
                    judge(&mut modes, objects.get(&e.sender.id), mode, |t| {
                        let table = table_of(&installs, &mut table_cache, t)?;
                        Some(
                            table.modes.iter().any(|(k, _)| k.mode == mode)
                                || table.next_modes.iter().any(|(k, to)| {
                                    k.mode == mode || to.iter().any(|m| m.mode == mode)
                                })
                                || table.combos.iter().any(|(k, _)| k.mode == mode)
                                || table.initial_mode == mode,
                        )
                    });
                }
                ServerMessage::DoFx(e) => {
                    let fx = e.action.fx;
                    judge(&mut fxs, objects.get(&e.sender.id), fx, |t| {
                        if t.fx.is_empty() {
                            return None;
                        }
                        Some(
                            t.fx.iter()
                                .any(|&f| effects(&installs, &mut fx_cache, f, 0).contains(&fx)),
                        )
                    });
                }
                _ => {}
            }
        },
    )
    .expect("the capture index reads");
    eprintln!("{n} messages read, {undecodable_messages} not decoded; {creates} objects created naming {named} records ({} distinct)", checked.len());
    eprintln!(
        "records that do not resolve: {:?}",
        undecoded.iter().take(20).collect::<Vec<_>>()
    );
    for (name, c) in [
        ("behaviors", &behaviors),
        ("modes", &modes),
        ("effects", &fxs),
    ] {
        eprintln!(
            "{name}: {} played, {} found in the object's tables, {} on objects created before the recording, {} on objects with no table; not found: {:?}",
            c.seen,
            c.ok,
            c.unknown_object,
            c.no_table,
            c.failures.iter().take(12).collect::<Vec<_>>()
        );
    }
    // Effects not in the object's own tables: whether any effect table of the install has them.
    let mut anywhere: HashSet<u32> = HashSet::new();
    let mut holders: BTreeMap<u32, HashSet<u32>> = BTreeMap::new();
    for id in 0x1E00_0000u32..0x1E00_1000 {
        if let Some(t) = installs.get::<FxTable>(id) {
            for (k, _) in &t.effects {
                anywhere.insert(*k);
                if fxs.failures.contains_key(k) {
                    holders.entry(id).or_default().insert(*k);
                }
            }
        }
    }
    let elsewhere: u64 = fxs
        .failures
        .iter()
        .filter(|(k, _)| anywhere.contains(k))
        .map(|(_, n)| n)
        .sum();
    let nowhere: Vec<(&u32, &u64)> = fxs
        .failures
        .iter()
        .filter(|(k, _)| !anywhere.contains(k))
        .collect();
    eprintln!(
        "effects not in the object's own tables: {elsewhere} in other effect tables ({:x?}), {} in none: {nowhere:x?}",
        holders.iter().map(|(t, e)| (*t, e.len())).collect::<Vec<_>>(),
        nowhere.len()
    );
    assert!(creates > 100_000, "{creates} objects created");
    assert_eq!(undecodable_messages, 0);
    assert!(
        undecoded.is_empty(),
        "{} named records do not resolve",
        undecoded.len()
    );
    // Modes: every one set on an object is one of its table's.
    assert_eq!(modes.ok, modes.seen - modes.unknown_object - modes.no_table);
    assert!(modes.ok > 100_000, "{} modes judged", modes.ok);
    // Behaviors: all but two behaviors (played twice each) are in the object's table.
    assert!(behaviors.ok > 40_000, "{} behaviors judged", behaviors.ok);
    let missed: Vec<u32> = behaviors.failures.keys().copied().collect();
    assert_eq!(
        missed,
        [0x4100_001B, 0x4100_001C],
        "behaviors not in their objects' tables"
    );
    // Effects: in the object's own tables, else in another effect table, but for six effects in
    // no effect table of either install.
    assert!(fxs.ok > 240_000, "{} effects judged", fxs.ok);
    let none: Vec<u32> = nowhere.iter().map(|(k, _)| **k).collect();
    assert_eq!(
        none,
        [
            0x4100_01C0,
            0x4100_0241,
            0x4100_0297,
            0x4100_0311,
            0x4100_0343,
            0x4100_0413
        ],
        "effects in no table"
    );
}

fn table_of<'a>(
    i: &Installs,
    cache: &'a mut HashMap<u32, Option<BehaviorTable>>,
    t: &Tables,
) -> Option<&'a BehaviorTable> {
    let id = t.behaviors?;
    cache
        .entry(id)
        .or_insert_with(|| i.get::<BehaviorTable>(id))
        .as_ref()
}

fn judge(
    c: &mut Count,
    object: Option<&Tables>,
    value: u32,
    found: impl FnOnce(&Tables) -> Option<bool>,
) {
    c.seen += 1;
    let Some(t) = object else {
        c.unknown_object += 1;
        return;
    };
    match found(t) {
        None => c.no_table += 1,
        Some(true) => c.ok += 1,
        Some(false) => *c.failures.entry(value).or_default() += 1,
    }
}
