//! The public check of a database, needing no install: the schema (read by [`crate::rows`]), keys,
//! references between rows, the merge, the shape of every row's provenance, and that a line's
//! token name is the one its string id is keyed by. What it cannot see without the install (that
//! a template exists, that an identifier is the install's) the compile checks.

use std::collections::{BTreeMap, BTreeSet};

use crate::merge::{merge, Merged};
use crate::rows::{read, Database, Located, Prov, Row, Source, Trigger, Update};
use crate::{Files, Problem};

/// Where a row may come from.
pub const FROM: [&str; 4] = ["capture", "dat", "derived", "authored"];

/// How sure a measured row is.
pub const CONFIDENCE: [&str; 3] = ["high", "medium", "low"];

/// The counts a measured row's sample may give.
pub const SAMPLES: [&str; 7] = [
    "sessions",
    "creatures",
    "corpses",
    "views",
    "trips",
    "lines",
    "visits",
];

/// The rules a derived row or field may name; each is stated in the database's format
/// description.
pub const RULES: [&str; 20] = [
    "camp-regen-generator",
    "camp-regen-median",
    "camp-regen-hall",
    "camp-profile-species",
    "keeper-store",
    "store-sales",
    "store-sales-unseen",
    "quest-definition",
    "quest-setup-static",
    "quest-death-effect",
    "quest-kill-fellows",
    "quest-retry-measured",
    "talk-token-role",
    "talk-quest-number",
    "talk-quest-name",
    "hoard-key-table",
    "hoard-key-text",
    "hoard-teleport-place",
    "map-marked-place",
    "item-spot-regen",
];

/// An evidence handle: capital letters, digits and hyphens, in parts joined by hyphens (the
/// project's own evidence index says what each names).
#[must_use]
pub fn is_handle(s: &str) -> bool {
    (3..=64).contains(&s.len())
        && s.contains('-')
        && !s.starts_with('-')
        && !s.ends_with('-')
        && s.bytes()
            .all(|b| b.is_ascii_uppercase() || b.is_ascii_digit() || b == b'-')
}

/// Check a database's files. Returns every problem, sorted; none means it reads, merges and
/// every reference and provenance is well formed. A database whose rows do not all read is
/// held only to its reading: a row left out would make others' references look broken.
#[must_use]
pub fn check(files: &Files) -> Vec<Problem> {
    match read(files) {
        Ok(db) => check_read(&db).0,
        Err(p) => p,
    }
}

/// [`check`] of a database already read: every problem, sorted, and the rows merged when they
/// merge.
#[must_use]
pub fn check_read(db: &Database) -> (Vec<Problem>, Option<Merged>) {
    let mut problems = check_database(db);
    let merged = match merge(db) {
        Ok(m) => Some(m),
        Err(p) => {
            problems.extend(p);
            None
        }
    };
    problems.sort();
    (problems, merged)
}

/// A reference's pack and key (`pack:key` or `key`, this pack).
fn split_ref(s: &str, this: &str) -> (String, String) {
    match s.split_once(':') {
        Some((p, k)) => (p.to_string(), k.to_string()),
        None => (this.to_string(), s.to_string()),
    }
}

/// A placed key with its landblock: `key@XXYY`, or `key` in `here`.
fn placed(key: &str, here: u16) -> (String, u16) {
    match key.split_once('@') {
        Some((k, b)) => (k.to_string(), u16::from_str_radix(b, 16).unwrap_or(0)),
        None => (key.to_string(), here),
    }
}

/// The checks after reading: keys unique in their scope, references that resolve, provenance.
#[must_use]
#[allow(clippy::too_many_lines)]
pub fn check_database(db: &Database) -> Vec<Problem> {
    let mut problems = Vec::new();
    // Every key of a kind, by pack, for references.
    let mut places: BTreeSet<(String, String)> = BTreeSet::new();
    let mut camps: BTreeSet<(String, String, u16)> = BTreeSet::new();
    let mut objects: BTreeSet<(String, String, u16)> = BTreeSet::new();
    let mut routes: BTreeMap<(String, String), usize> = BTreeMap::new();
    for l in db.rows() {
        let lb = l.landblock.unwrap_or(0);
        match &l.row {
            Row::Place(r) => {
                places.insert((l.pack.clone(), r.key.clone()));
            }
            Row::Camp(r) => {
                camps.insert((l.pack.clone(), r.key.clone(), lb));
            }
            Row::Object(r) => {
                objects.insert((l.pack.clone(), r.key.clone(), lb));
            }
            Row::Route(r) => {
                routes.insert((l.pack.clone(), r.key.clone()), r.waypoints.len());
            }
            _ => {}
        }
    }
    let packs: BTreeSet<&str> = db.packs.iter().map(|p| p.name.as_str()).collect();
    let place_ref = |to: &str, pack: &str| -> Option<String> {
        let (p, key) = split_ref(to, pack);
        if !packs.contains(p.as_str()) {
            Some(format!("no pack {p}"))
        } else if !places.contains(&(p.clone(), key.clone())) {
            Some(format!("no place {key} in pack {p}"))
        } else {
            None
        }
    };
    let mut keys: BTreeMap<(String, String), (String, usize)> = BTreeMap::new();
    for l in db.rows() {
        let at = |m: String| Problem::new(&l.file, l.line, m);
        if let Some(k) = scope_key(l) {
            if let Some((f, n)) = keys.insert((l.pack.clone(), k.clone()), (l.file.clone(), l.line))
            {
                problems.push(at(format!("{k} is also {f}:{n}")));
            }
        }
        match &l.row {
            Row::Object(r) => {
                if let Some(why) = r.to.as_deref().and_then(|to| place_ref(to, &l.pack)) {
                    problems.push(at(format!("to: {why}")));
                }
                if r.kind == crate::rows::ObjectKind::Npc && r.id.is_some() {
                    problems.push(at(
                        "id: a person is numbered by the server; only a static object keeps an id"
                            .into(),
                    ));
                }
                if (r.recall || r.summon) && r.to.is_none() {
                    problems.push(at(
                        "recall, summon: only a portal (an object with `to`) is recalled or summoned to"
                            .into(),
                    ));
                }
                if r.requires.is_some() && r.kind == crate::rows::ObjectKind::Npc {
                    problems.push(at(
                        "requires: a person is talked to; only a static object keeps a quest gate"
                            .into(),
                    ));
                }
                match &r.route {
                    Some(_) if r.kind != crate::rows::ObjectKind::Npc => {
                        problems.push(at("route: only a person walks a route".into()))
                    }
                    Some(route) => {
                        let (pack, key) = split_ref(route, &l.pack);
                        match routes.get(&(pack.clone(), key.clone())) {
                            None => {
                                problems.push(at(format!("route: no route {key} in pack {pack}")))
                            }
                            Some(n) if r.route_start as usize >= *n => problems.push(at(format!(
                                "route_start: route {key} has {n} waypoints, counted from 0"
                            ))),
                            Some(_) => {}
                        }
                    }
                    None if r.route_start != 0 || r.route_backward => problems.push(at(
                        "route_start, route_direction: an object without a route has no start"
                            .into(),
                    )),
                    None => {}
                }
            }
            Row::Camp(r) => {
                if let Some(parent) = &r.parent {
                    let (pack, key) = split_ref(parent, &l.pack);
                    let (key, lb) = placed(&key, l.landblock.unwrap_or(0));
                    if !camps.contains(&(pack, key.clone(), lb)) {
                        problems.push(at(format!("parent: no camp {key}@{lb:04X}")));
                    }
                }
                if r.spawns.is_empty() && r.profile.is_none() {
                    problems.push(at(
                        "spawns: a camp spawns something, or names a profile".into()
                    ));
                }
                if r.population == 0 {
                    problems.push(at("population: a camp keeps at least one alive".into()));
                }
                match &r.route {
                    Some(route) => {
                        let (pack, key) = split_ref(route, &l.pack);
                        match routes.get(&(pack.clone(), key.clone())) {
                            None => {
                                problems.push(at(format!("route: no route {key} in pack {pack}")))
                            }
                            Some(n) if r.route_start as usize >= *n => problems.push(at(format!(
                                "route_start: route {key} has {n} waypoints, counted from 0"
                            ))),
                            Some(_) => {}
                        }
                    }
                    None if r.route_start != 0 || r.route_backward => problems.push(at(
                        "route_start, route_direction: a camp without a route has no start".into(),
                    )),
                    None => {}
                }
            }
            Row::Step(r) => {
                if matches!(r.update, Update::Advance) && r.phase.is_none() {
                    problems.push(at("phase: an advance names the phase it moves to".into()));
                }
                if matches!(r.update, Update::Fail) && r.needs == 0 {
                    problems.push(at("needs: only a quest under way fails".into()));
                }
                if let Some(wants) = r.wants {
                    if !matches!(r.trigger, Trigger::Give(..)) {
                        problems.push(at("wants: only a give's step wants a count".into()));
                    }
                    if wants == 0 {
                        problems.push(at("wants: a count of at least 1".into()));
                    }
                }
                if let Some(why) = r.to.as_deref().and_then(|to| place_ref(to, &l.pack)) {
                    problems.push(at(format!("to: {why}")));
                }
                if r.to.is_some() && matches!(r.trigger, Trigger::Arrive(_)) {
                    problems.push(at(
                        "to: a step made on arrival takes the player nowhere further".into(),
                    ));
                }
            }
            Row::Give(r) => {
                if r.wants == Some(0) {
                    problems.push(at("wants: a count of at least 1".into()));
                }
                if r.item.is_none() && r.wants.is_some() {
                    problems.push(at("wants: a count of no item in particular".into()));
                }
                if r.counts.len() > r.gives.len() {
                    problems.push(at("counts: a count for each item given".into()));
                }
            }
            Row::Talk(r) => check_talk(r, l, &objects, &mut |m| problems.push(at(m))),
            Row::Loot(r) if r.counts.iter().any(|c| !(0.0..=1.0).contains(c)) => {
                problems.push(at("counts: shares between 0 and 1".into()));
            }
            _ => {}
        }
        if let Some(p) = l.row.prov() {
            check_prov(p, &mut |m| problems.push(at(m)));
        }
    }
    problems
}

/// A line has its words, its token name is its string's, its person is placed, and only an
/// advance names a phase.
fn check_talk(
    r: &crate::rows::Talk,
    l: &Located,
    objects: &BTreeSet<(String, String, u16)>,
    out: &mut dyn FnMut(String),
) {
    if r.line.is_none() && r.line_text.is_none() {
        out("line: a line is a string reference or line_text".into());
    }
    if let Some(name) = &r.line_name {
        match r.line {
            None => out("line_name: names the token of a line, which this row lacks".into()),
            Some(line) => {
                let token = auberean_assets::strings::token(name);
                if token != line.string {
                    out(format!(
                        "line_name: {name} is the token {token:#010X}, not the line's {:#010X}",
                        line.string
                    ));
                }
            }
        }
    }
    if let Some(object) = &r.object {
        let (pack, key) = split_ref(object, &l.pack);
        if key.contains('@') {
            let (key, lb) = placed(&key, 0);
            if !objects.contains(&(pack, key.clone(), lb)) {
                out(format!("object: no object {key}@{lb:04X}"));
            }
        } else {
            out("object: a placed person is named key@XXYY".into());
        }
    }
    if r.phase.is_some()
        && r.update != Some(yalain_content::talk::TalkUpdate::Advance)
        && r.role != Some(yalain_content::talk::Role::Remind)
    {
        out("phase: only an advance or a reminder names a phase".into());
    }
}

/// The key a row must not share within its pack: an object's or camp's within its landblock, a
/// place's, a talk row's.
fn scope_key(l: &Located) -> Option<String> {
    let lb = l.landblock.unwrap_or(0);
    Some(match &l.row {
        Row::Object(r) => format!("{}@{lb:04X}", r.key),
        Row::Camp(r) => format!("{}@{lb:04X}", r.key),
        Row::Place(r) => format!("place {}", r.key),
        Row::Talk(r) => format!("talk {}", r.key),
        Row::Route(r) => format!("route {}", r.key),
        _ => return None,
    })
}

/// What the shared grammar does not hold a provenance to: a measurement's handle, a rule from
/// the list, the names of a sample's counts.
fn check_prov(p: &Prov, out: &mut dyn FnMut(String)) {
    match p.source {
        Source::Capture => {
            if !p.evidence.as_deref().is_some_and(is_handle) {
                out(format!(
                    "prov: a measured row names its evidence handle, not {:?}",
                    p.evidence
                ));
            }
            if p.reference.as_deref().is_none_or(str::is_empty) {
                out("prov: a measured row gives its ref".into());
            }
        }
        Source::Derived => {
            if !p.rule.as_deref().is_some_and(|r| RULES.contains(&r)) {
                out(format!("prov: derived by {:?}, not a listed rule", p.rule));
            }
        }
        Source::Dat | Source::Authored => {}
    }
    for k in p.sample.keys() {
        if !SAMPLES.contains(&k.as_str()) {
            out(format!("prov: sample {k} is not one of {SAMPLES:?}"));
        }
    }
    for (k, f) in &p.fields {
        check_prov(f, &mut |m| out(format!("{k}: {m}")));
    }
}
