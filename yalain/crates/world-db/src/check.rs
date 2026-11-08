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
pub const RULES: [&str; 22] = [
    "pet-from-its-effect",
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
    "treasure-pool-fit",
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
        Ok(m) => {
            problems.extend(check_treasure_sorts(&m));
            problems.extend(check_treasure_bands(&m));
            problems.extend(check_treasure_families(&m));
            Some(m)
        }
        Err(p) => {
            problems.extend(p);
            None
        }
    };
    problems.sort();
    (problems, merged)
}

/// A world that gives any sort's treasure numbers gives every sort's: a sort it left out would
/// draw none of its own.
fn check_treasure_sorts(m: &Merged) -> Vec<Problem> {
    use yalain_content::treasure_tables::Sort;
    let rows: Vec<(&Located, Sort)> = m
        .rows
        .iter()
        .filter_map(|l| match &l.row {
            Row::TreasureSort(r) => Some((l, r.sort)),
            _ => None,
        })
        .collect();
    let Some((first, _)) = rows.first() else {
        return Vec::new();
    };
    let missing: Vec<&str> = Sort::ALL
        .into_iter()
        .filter(|s| !rows.iter().any(|(_, x)| x == s))
        .map(Sort::name)
        .collect();
    if missing.is_empty() {
        return Vec::new();
    }
    vec![Problem::new(
        &first.file,
        first.line,
        format!(
            "[[treasure_sort]]: no row of {}; a world that gives any sort gives all four",
            missing.join(", ")
        ),
    )]
}

/// The treasure rows that must agree with each other: a word's weights, one per count band of its
/// sort, the world's counts or the server's own; a potion's states, one per tier the world's rules
/// or the server's own give; and each sort's words and the potions in an order given once.
fn check_treasure_bands(m: &Merged) -> Vec<Problem> {
    use yalain_content::treasure_tables::{Sort, TreasureTables};
    let defaults = TreasureTables::retail_2017();
    let mut out = Vec::new();
    let mut counts: BTreeMap<Sort, (usize, Option<&Located>)> = Sort::ALL
        .into_iter()
        .map(|s| (s, (defaults.sort(s).counts.len(), None)))
        .collect();
    let mut tiers = (defaults.rules.potion_tiers.len(), None);
    let mut words: Vec<(&Located, Sort, u32, usize)> = Vec::new();
    let mut potions: Vec<(&Located, u32, usize)> = Vec::new();
    for l in &m.rows {
        match &l.row {
            Row::TreasureSort(r) => {
                counts.insert(r.sort, (r.tables.counts.len(), Some(l)));
            }
            Row::TreasureRules(r) => tiers = (r.rules.potion_tiers.len(), Some(l)),
            Row::TreasureWord(r) => words.push((l, r.sort, r.order, r.word.weights.len())),
            Row::TreasurePotion(r) => potions.push((l, r.order, r.states.len())),
            _ => {}
        }
    }
    let at = |l: &Located, m: String| Problem::new(&l.file, l.line, m);
    if words.is_empty() {
        // The server's own words: their sorts' counts must keep the bands they are weighted by.
        for sort in Sort::ALL {
            let own = defaults.sort(sort).words.first().map(|w| w.weights.len());
            if let (Some(n), (bands, Some(l))) = (own, counts[&sort]) {
                if n != bands {
                    out.push(at(
                        l,
                        format!(
                            "counts: {bands} bands, but the server's own words for a {} are weighted by {n}: give its [[treasure_word]] rows",
                            sort.name()
                        ),
                    ));
                }
            }
        }
    }
    let mut orders = BTreeSet::new();
    for (l, sort, order, n) in &words {
        let bands = counts[sort].0;
        if *n != bands {
            out.push(at(
                l,
                format!(
                    "weights: {n}, but a {}'s counts have {bands} bands: one weight a band",
                    sort.name()
                ),
            ));
        }
        if !orders.insert((*sort, *order)) {
            out.push(at(
                l,
                format!("order {order}: another {} word has it", sort.name()),
            ));
        }
    }
    if potions.is_empty() {
        if let (n, Some(l)) = tiers {
            let own = defaults.potions.first().map_or(0, Vec::len);
            if n != own {
                out.push(at(
                    l,
                    format!(
                        "potion_tiers: {n} tiers, but the server's own potions have {own} states each: give the world's [[treasure_potion]] rows"
                    ),
                ));
            }
        }
    }
    let mut orders = BTreeSet::new();
    for (l, order, n) in &potions {
        if *n != tiers.0 {
            out.push(at(
                l,
                format!(
                    "states: {n}, but the potions have {} tiers: one state a tier",
                    tiers.0
                ),
            ));
        }
        if !orders.insert(*order) {
            out.push(at(l, format!("order {order}: another potion has it")));
        }
    }
    out
}

/// The world's families of enchantments agree with each other: each sort's in an order given once,
/// each state a tier of one family alone, and each pair naming a tier's state of some family.
fn check_treasure_families(m: &Merged) -> Vec<Problem> {
    let rows: Vec<(&Located, &crate::rows::TreasureFamily)> = m
        .rows
        .iter()
        .filter_map(|l| match &l.row {
            Row::TreasureFamily(r) => Some((l, r)),
            _ => None,
        })
        .collect();
    let mut out = Vec::new();
    let mut orders = BTreeSet::new();
    let mut states: BTreeMap<u32, &Located> = BTreeMap::new();
    for (l, r) in &rows {
        if !orders.insert((r.sort, r.order)) {
            out.push(Problem::new(
                &l.file,
                l.line,
                format!("order {}: another {} family has it", r.order, r.sort.name()),
            ));
        }
        for t in &r.family.tiers {
            if let Some(other) = states.insert(t.state, l) {
                out.push(Problem::new(
                    &l.file,
                    l.line,
                    format!(
                        "tiers: state {:#010x} is a tier of another family too ({}:{})",
                        t.state, other.file, other.line
                    ),
                ));
            }
        }
    }
    for (l, r) in &rows {
        for (from, pair) in &r.pairs {
            if !states.contains_key(pair) {
                out.push(Problem::new(
                    &l.file,
                    l.line,
                    format!(
                        "tiers: {from:#010x}'s pair {pair:#010x} is no tier of the world's families"
                    ),
                ));
            }
        }
    }
    out
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
    // Each placed object's id: its templates, and whether a row of it leads somewhere.
    let mut ids: BTreeMap<u64, (BTreeSet<u32>, bool)> = BTreeMap::new();
    // Each camp, by its key and landblock in whichever pack: the templates it makes.
    let mut camp_makes: BTreeMap<(String, u16), BTreeSet<u32>> = BTreeMap::new();
    for l in db.rows() {
        let lb = l.landblock.unwrap_or(0);
        match &l.row {
            Row::Place(r) => {
                places.insert((l.pack.clone(), r.key.clone()));
            }
            Row::Camp(r) => {
                camps.insert((l.pack.clone(), r.key.clone(), lb));
                camp_makes
                    .entry((r.key.clone(), lb))
                    .or_default()
                    .extend(r.spawns.iter().map(|(t, _)| *t));
            }
            Row::Object(r) => {
                objects.insert((l.pack.clone(), r.key.clone(), lb));
                if let Some(id) = r.id {
                    let e = ids.entry(id).or_default();
                    e.0.insert(r.template);
                    e.1 |= r.to.is_some();
                }
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
                if (r.hero || r.kingdom.is_some()) && r.kind == crate::rows::ObjectKind::Npc {
                    problems.push(at(
                        "hero, kingdom: a person is talked to; only a static object keeps a player out"
                            .into(),
                    ));
                }
                if r.kingdom.is_some_and(|k| ![1, 2, 4].contains(&k)) {
                    problems.push(at(
                        "kingdom: 1 the Shadow, 2 the Order, 4 the Dominion".into()
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
            Row::Place(r) => {
                if r.row.is_some() != r.holds.is_some() {
                    problems.push(at(
                        "row, holds: a room of a row names both its row and its creature's cell"
                            .into(),
                    ));
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
                if r.to.is_some()
                    && matches!(r.trigger, Trigger::Arrive(_) | Trigger::ArriveFrom(..))
                {
                    problems.push(at(
                        "to: a step made on arrival takes the player nowhere further".into(),
                    ));
                }
                if let Trigger::KillIn(template, camp) = &r.trigger {
                    let (key, lb) = match camp.split_once('@') {
                        Some((k, b)) => (k, u16::from_str_radix(b, 16).ok()),
                        None => (camp.as_str(), None),
                    };
                    let found: Vec<&BTreeSet<u32>> = camp_makes
                        .iter()
                        .filter(|((k, b), _)| k == key && lb.is_none_or(|lb| lb == *b))
                        .map(|(_, makes)| makes)
                        .collect();
                    match found[..] {
                        [] => problems.push(at(format!("trigger: no camp {camp}"))),
                        [makes] if !makes.contains(template) => problems.push(at(format!(
                            "trigger: camp {camp} makes no creature of template {template:#010X}"
                        ))),
                        [_] => {}
                        _ => problems.push(at(format!(
                            "trigger: several camps are {camp}: name one as {camp}@XXYY"
                        ))),
                    }
                }
                if let Trigger::ArriveFrom(template, from) = r.trigger {
                    match ids.get(&from) {
                        None => problems.push(at(format!("trigger: no placed object {from:#018X}"))),
                        Some((t, _)) if !t.contains(&template) => problems.push(at(format!(
                            "trigger: object {from:#018X} is not of template {template:#010X}"
                        ))),
                        Some((_, false)) => problems.push(at(format!(
                            "trigger: object {from:#018X} leads nowhere, so nobody arrives through it"
                        ))),
                        Some(_) => {}
                    }
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
            Row::DeathTreasure(r) => {
                if let Some(camp) = &r.camp {
                    let (pack, key) = split_ref(camp, &l.pack);
                    if key.contains('@') || l.landblock.is_some() {
                        let (key, lb) = placed(&key, l.landblock.unwrap_or(0));
                        if !camps.contains(&(pack, key.clone(), lb)) {
                            problems.push(at(format!("camp: no camp {key}@{lb:04X}")));
                        }
                    } else {
                        problems.push(at("camp: a camp is named key@XXYY".into()));
                    }
                }
            }
            Row::Loot(r) if r.counts.iter().any(|c| !(0.0..=1.0).contains(c)) => {
                problems.push(at("counts: shares between 0 and 1".into()));
            }
            Row::Loot(r)
                if r.level.is_some()
                    != matches!(r.value.as_slice(), [low, high] if low <= high && *low > 0)
                    || (r.level.is_none() && !r.value.is_empty()) =>
            {
                problems.push(at(
                    "level, value: a chest's draw names its level and [low, high]".into(),
                ));
            }
            _ => {}
        }
        if let Some(p) = l.row.prov() {
            check_prov(p, &mut |m| problems.push(at(m)));
        }
    }
    problems.extend(crate::pools_check::check(db));
    problems.extend(check_events(db));
    problems
}

/// Every camp, object and step that names an `[[event]]` names one the database has.
fn check_events(db: &Database) -> Vec<Problem> {
    let keys: BTreeSet<&str> = db
        .rows()
        .filter_map(|l| match &l.row {
            Row::Event(e) => Some(e.key.as_str()),
            _ => None,
        })
        .collect();
    let mut out = Vec::new();
    for l in db.rows() {
        let named: Vec<&String> = match &l.row {
            Row::Object(o) => o.event.iter().collect(),
            Row::Camp(c) => c.event.iter().collect(),
            Row::Step(s) => s.events.iter().collect(),
            _ => Vec::new(),
        };
        for e in named {
            if !keys.contains(e.as_str()) {
                out.push(Problem::new(
                    &l.file,
                    l.line,
                    format!("event: no [[event]] {e}"),
                ));
            }
        }
    }
    out
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
        Row::Clearing(r) => format!("clearing {}@{lb:04X}", r.key),
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
