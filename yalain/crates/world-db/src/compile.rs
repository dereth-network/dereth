//! A merged database compiled against an install into the tables the server reads: the content
//! pack (every person and static object described from its template), the spawn, loot, store,
//! quest and talk tables. What a row leaves out the install supplies: a keeper's store, a sale's
//! product, price, currency and name, a quest's name, description, icon and journal; every data
//! id a row names is checked against the install.
//!
//! What is said (a step's line, a keeper's greeting, a person's line) goes out as the string
//! reference a row gives; a row with only new prose (`tell_text`, `greeting_text`, `line_text`)
//! has it said as literal text, which the client shows unchanged. Names are still the install's:
//! an object's or a sale's `name_text` alone is served under the template's own name.
//!
//! Each table row's `placed` column carries its provenance, so a running server can say where a
//! creature or a sale came from.

use std::collections::BTreeMap;

use auberean_assets::entity::Entity;
use auberean_assets::script::Qualities;
use dereth_primitives::num::math;
use yalain_content::quests::{
    Give as TableGive, Need as TableNeed, Place as StepPlace, QuestDef, QuestTable,
    Step as TableStep, Trigger as TableTrigger, Update as TableUpdate,
};
use yalain_content::stores::{Keeper as TableKeeper, Sale as TableSale, SaleGate, StoreTable};
use yalain_content::talk::{Line, TalkTable};
use yalain_content::{
    describe, facts, Camp as TableCamp, ContentObject, ContentPack, FactIds, Kind, LootProfile,
    LootTable, Place as TablePlace, SpawnTable, Words,
};

use crate::merge::Merged;
use crate::rows::{
    At, Gate, Located, Need, ObjectKind, Prov, Row, Source, StrRef, Trigger, Update,
};
use crate::Problem;

/// One sale of a store template, as the install holds it.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct InstallSale {
    pub ordinal: u32,
    pub product: u32,
    pub cost: f32,
    pub currency: u32,
}

/// A quest's definition, as the install holds it: name and description (string, table), icon,
/// whether updates play an effect, the journal entry of each phase from the first, and the effects
/// each kind of update puts on the player (the kind, the effect's saved state).
#[derive(Debug, Clone, PartialEq)]
pub struct QuestDefinition {
    pub state: u32,
    pub name: (u32, u32),
    pub description: (u32, u32),
    pub icon: u32,
    pub play_fx: bool,
    pub journals: Vec<(u32, u32)>,
    pub effects: Vec<(u32, u32)>,
    /// The phases done by other quests (subquests), in their order.
    pub subquests: Vec<yalain_content::quests::Subquests>,
    /// Done once: its package's setup says it may be done one time.
    pub once: bool,
    /// How a kill's move on it reaches the killer's fellows, as its package's setup says.
    pub fellows: yalain_content::quests::Fellows,
    /// Its last phase waits for something other than a kill to end it: the phase's range is more
    /// than one.
    pub last_waits: bool,
    /// The least level a player is given it at (0: any), as its package's setup says.
    pub min_level: i32,
    /// Given only to heroes, as its package's setup says.
    pub hero_only: bool,
    /// For the monarch of an allegiance alone, as its package's setup says. Read, and nothing yet
    /// holds a quest to it.
    pub monarch_only: bool,
    /// The experience its completion pays, as its package's setup says.
    pub xp: u64,
    /// How its completion's experience is worked out, as its package's setup says.
    pub pay: yalain_content::quests::CompletionPay,
    /// A quest a player is to have at a status to be given it, as its package's setup says.
    pub required: Option<(u32, u32)>,
}

/// What a compile reads from the user's install.
pub trait Install {
    /// An entity (a template, a member of a template's chain, a look).
    fn entity(&self, id: u32) -> Option<Entity>;
    /// A template's qualities record.
    fn qualities(&self, id: u32) -> Option<Qualities>;
    /// The property ids examining an object reads.
    fn fact_ids(&self) -> FactIds;
    /// A property's name by its id.
    fn property_name(&self, id: u32) -> Option<String>;
    /// A generator profile's name (loot profiles among them) by its value.
    fn profile_name(&self, value: u32) -> Option<String>;
    /// A store template's sales.
    fn store_sales(&self, store: u32) -> Option<Vec<InstallSale>>;
    /// Who each of a store template's sales is for, by ordinal (none listed: every sale is for
    /// everyone), and whether the store hides the sales a player may not have.
    fn store_gates(&self, _store: u32) -> (Vec<(u32, SaleGate)>, bool) {
        (Vec::new(), false)
    }
    /// A quest's definition: the state named, else the one defining the quest id.
    fn quest_definition(&self, state: Option<u32>, id: u32) -> Option<QuestDefinition>;
    /// A template's name to read a table by, when the install's text is at hand.
    fn label(&self, _template: u32) -> String {
        String::new()
    }
}

/// The server's tables.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct Tables {
    pub content: ContentPack,
    pub spawns: SpawnTable,
    pub loot: LootTable,
    pub stores: StoreTable,
    pub quests: QuestTable,
    pub talk: TalkTable,
    /// The craft skills and recipes a new character starts with.
    pub crafting: yalain_content::CraftStart,
    /// The story's scenes, the shards that bestow them, the scenes a new character holds.
    pub story: yalain_content::StoryTable,
    /// The locked doors, by their world ids, and the keys that open them.
    pub locks: yalain_content::LockTable,
    /// The templates of the weapons creatures wield, by the names their templates give them,
    /// where a name is no template of the install's own.
    pub weapons: BTreeMap<String, u32>,
    /// What a row says of a weapon beyond its template (the share of creatures given two, its
    /// stance), by the weapon's name.
    pub weapon_rules: BTreeMap<String, yalain_content::WeaponRule>,
    /// The places templates name: where a person who sends players by a map sends the holder of
    /// a map of that template.
    pub places_named: BTreeMap<u32, TablePlace>,
    /// The places by their names (lowercase): where an effect that sends a player to a named place
    /// (a city's recall) sends it.
    pub places_by_name: BTreeMap<String, TablePlace>,
    /// What the compile left out or could not serve, and why.
    pub warnings: Vec<Problem>,
}

impl Tables {
    /// Rows by table: content, spawns, loot, keepers, sales, quests, steps, gives, lines.
    #[must_use]
    pub fn counts(&self) -> [(&'static str, usize); 9] {
        [
            ("content", self.content.objects.len()),
            ("spawns", self.spawns.camps.len()),
            ("loot", self.loot.profiles.len()),
            ("keepers", self.stores.keepers.len()),
            ("sales", self.stores.sales.len()),
            ("quests", self.quests.quests.len()),
            ("steps", self.quests.steps.len()),
            ("gives", self.quests.gives.len()),
            ("talk", self.talk.lines.len()),
        ]
    }
}

/// A measurement's confidence as the tables carry it, `authored` for a row without one.
fn confidence_word(p: &Prov) -> String {
    p.confidence
        .map_or_else(|| "authored".into(), |c| c.word().to_string())
}

/// A row's provenance as the tables' `placed` column carries it.
#[must_use]
pub fn prov_text(p: &Prov) -> String {
    let mut s = match p.source {
        Source::Capture => {
            let mut s = format!(
                "capture {} {} ({}",
                p.evidence.as_deref().unwrap_or(""),
                p.reference.as_deref().unwrap_or(""),
                p.confidence.map_or("", |c| c.word())
            );
            for (k, v) in &p.sample {
                s.push_str(&format!(", {v} {k}"));
            }
            s.push(')');
            s
        }
        Source::Derived => format!(
            "derived {}{}",
            p.rule.as_deref().unwrap_or(""),
            p.record
                .map(|r| format!(" from {r:08X}"))
                .unwrap_or_default()
        ),
        Source::Dat => format!("dat {:08X}", p.record.unwrap_or(0)),
        Source::Authored => format!(
            "authored{}",
            p.by.as_deref()
                .map(|b| format!(" by {b}"))
                .unwrap_or_default()
        ),
    };
    for (k, f) in &p.fields {
        s.push_str(&format!(
            "; {k} {}{}",
            f.source,
            f.rule
                .as_deref()
                .map(|r| format!(" {r}"))
                .unwrap_or_default()
        ));
    }
    s.replace(['\t', '\n'], " ")
}

/// What is said: the string reference a row gives, else its new prose as literal text.
fn words(line: Option<StrRef>, text: Option<&String>) -> Option<Words> {
    match (line, text) {
        (Some(r), _) => Some(Words::Ref(r.string, r.table)),
        (None, Some(t)) => Some(Words::Literal(t.clone())),
        (None, None) => None,
    }
}

/// A heading as a quest step's trip writes it: degrees clockwise from north, times 255/360.
#[must_use]
pub fn heading_byte(degrees: f32) -> u8 {
    #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
    let b = (degrees.rem_euclid(360.0) * 255.0 / 360.0).round() as u32;
    u8::try_from(b % 256).unwrap_or(0)
}

/// The heading in degrees a rotation (scalar first, about up) faces: the opposite of the turn
/// [`heading_rotation`] makes.
#[must_use]
pub fn rotation_heading(q: [f32; 4]) -> f32 {
    (-2.0 * math::atan2f(q[3], q[0]).to_degrees()).rem_euclid(360.0)
}

/// The outdoor cell of a landblock holding a point: 8 x 8 cells of 20 m, numbered from 1 along y
/// within each x.
#[must_use]
pub fn outdoor_cell(landblock: u16, x: f32, y: f32) -> u32 {
    #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
    let c = |v: f32| ((v / 20.0).floor().clamp(0.0, 7.0)) as u32;
    u32::from(landblock) << 16 | (c(x) * 8 + c(y) + 1)
}

/// The rotation a heading in degrees gives (the world turns by the heading's opposite about up),
/// scalar first.
#[must_use]
pub fn heading_rotation(degrees: f32) -> [f32; 4] {
    let half = -degrees.to_radians() / 2.0;
    [math::cosf(half), 0.0, 0.0, math::sinf(half)]
}

/// A quest status as a template's `Usage_RequiredQuestStatus` writes it: completed, and under way
/// (a phase in the low bits).
const QUEST_COMPLETED: u32 = 0x3000_0000;
const QUEST_UNDERWAY: u32 = 0x1000_0000;

const fn pair(r: StrRef) -> (u32, u32) {
    (r.string, r.table)
}

struct Ctx<'a> {
    install: &'a dyn Install,
    problems: Vec<Problem>,
    warnings: Vec<Problem>,
}

impl Ctx<'_> {
    fn problem(&mut self, l: &Located, m: impl Into<String>) {
        self.problems.push(Problem::new(&l.file, l.line, m));
    }

    fn warn(&mut self, l: &Located, m: impl Into<String>) {
        self.warnings.push(Problem::new(&l.file, l.line, m));
    }

    fn template(&mut self, l: &Located, field: &str, t: u32) -> bool {
        if self.install.entity(t).is_some() {
            true
        } else {
            self.problem(
                l,
                format!("{field}: template {t:08X} is not in the install"),
            );
            false
        }
    }

    fn z(&mut self, l: &Located, at: At) -> Option<[f32; 3]> {
        if let Some(z) = at.z {
            Some([at.x, at.y, z])
        } else {
            self.problem(
                l,
                "at: a z of \"ground\" needs the land's height, which this compile does not read yet",
            );
            None
        }
    }

    /// The route a camp names (`key`, or `pack:key` in another pack), each waypoint with its rest.
    fn route(
        &mut self,
        routes: &BTreeMap<(String, String), &Located>,
        l: &Located,
        name: &str,
    ) -> Option<yalain_content::Route> {
        let (pack, key) = match name.split_once(':') {
            Some((p, k)) => (p.to_string(), k.to_string()),
            None => (l.pack.clone(), name.to_string()),
        };
        let Some(rl) = routes.get(&(pack, key)) else {
            self.problem(l, format!("route: no route {name}"));
            return None;
        };
        let Row::Route(r) = &rl.row else { return None };
        let mut waypoints = Vec::new();
        for w in &r.waypoints {
            waypoints.push(yalain_content::movement::Waypoint {
                cell: w.cell,
                at: self.z(rl, w.at)?,
                rest: w.rest.unwrap_or(r.rest),
            });
        }
        Some(yalain_content::Route {
            key: r.key.clone(),
            shape: r.shape,
            gait: r.gait,
            speed: r.speed,
            waypoints,
        })
    }

    /// The place a row's `to` names (`key`, or `pack:key` in another pack).
    fn place(
        &mut self,
        places: &BTreeMap<(String, String), &Located>,
        l: &Located,
        to: &str,
    ) -> Option<TablePlace> {
        let (pack, key) = match to.split_once(':') {
            Some((p, k)) => (p.to_string(), k.to_string()),
            None => (l.pack.clone(), to.to_string()),
        };
        let Some(pl) = places.get(&(pack, key)) else {
            self.problem(l, format!("to: no place {to}"));
            return None;
        };
        self.place_row(pl)
    }

    /// Where a place row stands.
    fn place_row(&mut self, pl: &Located) -> Option<TablePlace> {
        let Row::Place(p) = &pl.row else { return None };
        let Some(cell) = p.cell else {
            self.problem(pl, "cell: a place names its cell");
            return None;
        };
        let origin = self.z(pl, p.at)?;
        Some(TablePlace {
            cell,
            origin,
            rotation: p
                .rotation
                .unwrap_or_else(|| heading_rotation(p.heading.unwrap_or(0.0))),
        })
    }
}

/// A placed portal's flags: recalled to, summoned to.
fn portal_flags(recall: bool, summon: bool) -> u32 {
    let mut f = 0;
    if recall {
        f |= yalain_content::portal::RECALL;
    }
    if summon {
        f |= yalain_content::portal::SUMMON;
    }
    f
}

/// The subquests of the quests the rows name, each from its definition in the install where no row
/// gives it, with no challenge, duration or wait of its own; every subquest hidden (none is ever
/// shown on its own).
fn subquest_definitions(quests: &mut yalain_content::QuestTable, install: &dyn Install) {
    let wanted: Vec<u32> = quests
        .quests
        .iter()
        .flat_map(|q| q.subquests.iter().flat_map(|p| p.quests.iter().copied()))
        .collect();
    for &sub in &wanted {
        // A subquest a row gives is hidden all the same.
        if let Some(q) = quests.quests.iter_mut().find(|q| q.id == sub) {
            q.hidden = true;
            continue;
        }
        let Some(def) = install.quest_definition(None, sub) else {
            continue;
        };
        quests.quests.push(QuestDef {
            id: sub,
            name: def.name,
            description: def.description,
            icon: def.icon,
            play_fx: def.play_fx,
            journals: def.journals,
            challenge: -999,
            duration: 0.0,
            timed_phase: 0,
            retry: 0.0,
            xp: 0,
            cancellable: false,
            placed: format!("derived subquest from {:08X}", def.state),
            hidden: true,
            effects: def.effects,
            subquests: def.subquests,
            min_level: def.min_level,
            fails_on_death: false,
            once: def.once,
            fellows: def.fellows,
            last_waits: def.last_waits,
            hero_only: def.hero_only,
            required: def.required,
            pay: yalain_content::quests::CompletionPay::default(),
        });
    }
}

/// How a quest's completion pays: as its setup says, unless its row says otherwise. A row's amount
/// is paid in place of the setup's share. A row's share is paid whole, as nearly every share a setup
/// names is (89 of 91), and at most what it comes to at the row's level.
fn quest_pay(
    setup: yalain_content::quests::CompletionPay,
    q: &crate::rows::Quest,
) -> yalain_content::quests::CompletionPay {
    use yalain_content::quests::CompletionPay;
    if let Some(share) = q.xp_share {
        #[allow(clippy::cast_possible_truncation)]
        let share = share as f32;
        return CompletionPay {
            share,
            most_at_level: q.xp_share_level.map(|level| (share, level)),
            flat: true,
            ..CompletionPay::default()
        };
    }
    if q.xp.is_some() {
        return CompletionPay {
            share: 0.0,
            ..setup
        };
    }
    setup
}

/// Compile a merged database against an install.
///
/// # Errors
/// Every row that names what the install lacks (a template, a store's sale, a quest's definition,
/// a profile), a portal leading to no place, a point without its cell.
#[allow(clippy::too_many_lines)]
pub fn compile(merged: &Merged, install: &dyn Install) -> Result<Tables, Vec<Problem>> {
    let mut cx = Ctx {
        install,
        problems: Vec::new(),
        warnings: merged.warnings.clone(),
    };
    let mut t = Tables::default();
    let places: BTreeMap<(String, String), &Located> = merged
        .rows
        .iter()
        .filter_map(|l| match &l.row {
            Row::Place(p) => Some(((l.pack.clone(), p.key.clone()), l)),
            _ => None,
        })
        .collect();
    let routes: BTreeMap<(String, String), &Located> = merged
        .rows
        .iter()
        .filter_map(|l| match &l.row {
            Row::Route(r) => Some(((l.pack.clone(), r.key.clone()), l)),
            _ => None,
        })
        .collect();
    let ids = install.fact_ids();
    let entity = |id: u32| install.entity(id);
    let qualities = |id: u32| install.qualities(id);
    // Each placed object's place in the content pack, for the lines of one person.
    let mut objects_at: BTreeMap<(String, String, u16), usize> = BTreeMap::new();
    // Camp keys a second landblock reuses are made unique with the landblock.
    let mut camp_keys: BTreeMap<&str, usize> = BTreeMap::new();
    // The scenes a new character holds, by their order.
    let mut door_scenes: Vec<(&Located, u32)> = Vec::new();
    let mut starts: Vec<(u32, u32)> = Vec::new();
    let mut quest_rows: std::collections::BTreeSet<u32> = std::collections::BTreeSet::new();
    for l in &merged.rows {
        match &l.row {
            Row::Camp(c) => *camp_keys.entry(c.key.as_str()).or_default() += 1,
            Row::Quest(q) => {
                quest_rows.insert(q.id);
            }
            _ => {}
        }
    }
    for l in &merged.rows {
        let lb = l.landblock.unwrap_or(0);
        match &l.row {
            Row::Object(o) => {
                if !cx.template(l, "template", o.template) {
                    continue;
                }
                let Some(origin) = cx.z(l, o.at) else {
                    continue;
                };
                let mut d = match describe(o.template, &entity, &qualities) {
                    Ok(d) => d,
                    Err(e) => {
                        cx.warn(l, format!("not described, left out: {e}"));
                        continue;
                    }
                };
                match o.scale {
                    Some(s) if s > 0.0 => d.scale = Some(s),
                    Some(s) => cx.warn(l, format!("scale {s}: not above zero, left as it was")),
                    None => {}
                }
                if let Some(n) = o.name {
                    d.name = Some(pair(n));
                    d.source.push_str(", name as recorded");
                } else if o.name_text.is_some() {
                    cx.warn(l, "name_text: served without it until the world's overlay holds the text; give a string reference");
                }
                let destination = match &o.to {
                    None => None,
                    Some(to) => match cx.place(&places, l, to) {
                        Some(p) => Some(p),
                        None => continue,
                    },
                };
                let walks = match &o.route {
                    None => None,
                    Some(r) => match cx.route(&routes, l, r) {
                        Some(route) => Some(yalain_content::CampMoves {
                            route: Some(route),
                            start: o.route_start as usize,
                            backward: o.route_backward,
                            ..yalain_content::CampMoves::default()
                        }),
                        None => continue,
                    },
                };
                let gate = o.requires.map(|(quest, g)| yalain_content::QuestGate {
                    quest,
                    status: match g {
                        Gate::Done => QUEST_COMPLETED,
                        Gate::UnderWay => QUEST_UNDERWAY,
                        Gate::Phase(n) => QUEST_UNDERWAY | n,
                    },
                });
                if let Some((quest, _)) = o.requires {
                    if !quest_rows.contains(&quest) {
                        cx.warn(
                            l,
                            format!(
                                "requires: quest {quest:08X} has no row, so no player ever holds it"
                            ),
                        );
                    }
                }
                if let Some(scene) = o.scene {
                    if destination.is_none() {
                        cx.warn(l, "scene: a door's scene with no `to` is never played");
                    }
                    let cell = o.cell.unwrap_or_else(|| outdoor_cell(lb, o.at.x, o.at.y));
                    t.story.doors.insert((cell, o.template), scene);
                    door_scenes.push((l, scene));
                }
                if o.lock.is_some() && !o.locked {
                    cx.warn(
                        l,
                        "lock: the key id of a door that is not `locked` opens nothing",
                    );
                }
                if o.locked {
                    match (o.kind, o.id) {
                        (ObjectKind::Static, Some(id)) => {
                            t.locks
                                .objects
                                .insert(id, yalain_content::Lock { key: o.lock });
                        }
                        _ => cx.warn(l, "locked: only a static object (one with an id) is locked"),
                    }
                }
                objects_at.insert((l.pack.clone(), o.key.clone(), lb), t.content.objects.len());
                t.content.objects.push(ContentObject {
                    object: match o.kind {
                        ObjectKind::Static => o.id,
                        ObjectKind::Npc => None,
                    },
                    template: o.template,
                    description: d,
                    cell: o.cell.unwrap_or_else(|| outdoor_cell(lb, o.at.x, o.at.y)),
                    origin,
                    rotation: o
                        .rotation
                        .unwrap_or_else(|| heading_rotation(o.heading.unwrap_or(0.0))),
                    kind: match o.kind {
                        ObjectKind::Static => Kind::Static,
                        ObjectKind::Npc => Kind::Npc,
                    },
                    label: install.label(o.template),
                    placed: prov_text(&o.prov),
                    destination,
                    facts: yalain_content::Facts {
                        portal: portal_flags(o.recall, o.summon),
                        ..facts(o.template, &entity, &ids)
                    },
                    walks,
                    gate,
                });
            }
            Row::Camp(c) => {
                let mut ok = true;
                for (tpl, _) in &c.spawns {
                    ok &= cx.template(l, "spawns", *tpl);
                }
                if c.spawns.is_empty() {
                    cx.warn(
                        l,
                        "spawns: a camp naming only a profile is served once profiles are",
                    );
                    continue;
                }
                let Some(origin) = cx.z(l, c.at) else {
                    continue;
                };
                let mut spots = Vec::new();
                for (cell, at) in &c.spots {
                    if let Some(p) = cx.z(l, *at) {
                        spots.push((*cell, p));
                    }
                }
                let route = match &c.route {
                    None => None,
                    Some(r) => match cx.route(&routes, l, r) {
                        Some(r) => Some(r),
                        None => continue,
                    },
                };
                if !ok {
                    continue;
                }
                let id = if camp_keys.get(c.key.as_str()).copied().unwrap_or(0) > 1 {
                    format!("{}@{lb:04X}", c.key)
                } else {
                    c.key.clone()
                };
                t.spawns.camps.push(TableCamp {
                    id,
                    cell: c.cell.unwrap_or_else(|| outdoor_cell(lb, c.at.x, c.at.y)),
                    origin,
                    radius: c.radius,
                    population: c.population,
                    regen: c.regen,
                    templates: c.spawns.clone(),
                    spots,
                    placed: prov_text(&c.prov),
                    confidence: confidence_word(&c.prov),
                    moves: yalain_content::CampMoves {
                        settings: c.moves,
                        route,
                        start: c.route_start as usize,
                        backward: c.route_backward,
                    },
                });
            }
            Row::Loot(r) => {
                let Some(name) = install.profile_name(r.profile) else {
                    cx.problem(
                        l,
                        format!(
                            "profile: {:#x} is not a generator profile of the install",
                            r.profile
                        ),
                    );
                    continue;
                };
                if let Some(n) = &r.profile_name {
                    if *n != name {
                        cx.problem(
                            l,
                            format!(
                                "profile_name: the install names {:#x} {name}, not {n}",
                                r.profile
                            ),
                        );
                        continue;
                    }
                }
                t.loot.profiles.push(LootProfile {
                    name,
                    counts: r.counts.clone(),
                    items: r.items.clone(),
                    extras: r.extras.clone(),
                    placed: prov_text(&r.prov),
                    confidence: confidence_word(&r.prov),
                });
            }
            Row::Keeper(k) => {
                if !cx.template(l, "template", k.template) {
                    continue;
                }
                let store = k.store.or_else(|| {
                    yalain_content::Props::of(k.template, &entity, &|id| install.property_name(id))
                        .word("StoreTemplate")
                });
                let Some(store) = store else {
                    cx.problem(
                        l,
                        format!(
                            "store: template {:08X} names no store template; give store",
                            k.template
                        ),
                    );
                    continue;
                };
                t.stores.keepers.push(TableKeeper {
                    template: k.template,
                    store,
                    greeting: words(k.greeting, k.greeting_text.as_ref()),
                    placed: prov_text(&k.prov),
                });
            }
            Row::Sale(s) => {
                let sold = install
                    .store_sales(s.store)
                    .and_then(|v| v.into_iter().find(|x| x.ordinal == s.ordinal));
                let (Some(product), Some(cost), Some(currency)) = (
                    s.product.or(sold.map(|x| x.product)),
                    s.cost.or(sold.map(|x| x.cost)),
                    s.currency.or(sold.map(|x| x.currency)),
                ) else {
                    cx.problem(l, format!("store {:08X} has no sale {} in the install; give product, cost and currency", s.store, s.ordinal));
                    continue;
                };
                if !cx.template(l, "product", product) {
                    continue;
                }
                if s.name.is_none() && s.name_text.is_some() {
                    cx.warn(l, "name_text: served under the product's own name until the world's overlay holds the text; give a string reference");
                }
                let name = match s.name {
                    Some(n) => Some(pair(n)),
                    None => match yalain_content::Props::of(product, &entity, &|id| {
                        install.property_name(id)
                    })
                    .get("Name")
                    {
                        Some(yalain_content::Prop::Str(a, b)) => Some((*a, *b)),
                        _ => None,
                    },
                };
                let Some(name) = name else {
                    cx.problem(
                        l,
                        format!(
                            "name: product {product:08X} has no name in the install; give name"
                        ),
                    );
                    continue;
                };
                let (gates, hides) = install.store_gates(s.store);
                if hides && !t.stores.hiding.contains(&s.store) {
                    t.stores.hiding.push(s.store);
                }
                t.stores.sales.push(TableSale {
                    store: s.store,
                    ordinal: s.ordinal,
                    product,
                    cost,
                    currency,
                    stack: s.stack.unwrap_or(1),
                    icon: s.icon.clone(),
                    name,
                    placed: prov_text(&s.prov),
                    gate: gates
                        .into_iter()
                        .find(|(o, _)| *o == s.ordinal)
                        .map(|(_, g)| g)
                        .unwrap_or_default(),
                });
            }
            Row::Quest(q) => {
                let Some(def) = install.quest_definition(q.state, q.id) else {
                    cx.problem(
                        l,
                        format!("quest {:08X} has no definition in the install", q.id),
                    );
                    continue;
                };
                t.quests.quests.push(QuestDef {
                    id: q.id,
                    name: def.name,
                    description: def.description,
                    icon: def.icon,
                    play_fx: def.play_fx,
                    journals: def.journals,
                    challenge: q.challenge.unwrap_or(-999),
                    duration: q.duration.unwrap_or(0.0),
                    timed_phase: q.timed_phase.unwrap_or(0),
                    retry: q.retry.unwrap_or(0.0),
                    xp: q.xp.unwrap_or(def.xp),
                    cancellable: q.cancellable.unwrap_or(false),
                    placed: prov_text(&q.prov),
                    // A quest is shown when its row says so, else when its definition has a
                    // description to show.
                    hidden: !q.visible.unwrap_or(def.description.0 != 0),
                    effects: def.effects,
                    subquests: def.subquests,
                    min_level: q.min_level.unwrap_or(def.min_level),
                    fails_on_death: q.fails_on_death.unwrap_or(false),
                    once: q.once.unwrap_or(def.once),
                    // A row says whether; left out, the setup does, and a quest with no setup
                    // stays the killer's. A row that shares a quest whose setup says nothing has
                    // the limits most quests' setups name (205 of the 226 that share name both).
                    fellows: match q.fellows {
                        None => def.fellows,
                        Some(false) => yalain_content::quests::Fellows::default(),
                        Some(true) if def.fellows.shared => def.fellows,
                        Some(true) => yalain_content::quests::Fellows {
                            shared: true,
                            near_level: true,
                            near_by: true,
                        },
                    },
                    last_waits: def.last_waits,
                    hero_only: def.hero_only,
                    required: def.required,
                    pay: yalain_content::quests::CompletionPay {
                        points: q.qp.unwrap_or(def.pay.points),
                        ..quest_pay(def.pay, q)
                    },
                });
            }
            Row::Step(s) => {
                let (trigger, named) = match s.trigger {
                    Trigger::Use(x) => (TableTrigger::Use(x), vec![x]),
                    Trigger::UseOn(i, x) => (TableTrigger::UseOn(i, x), vec![i, x]),
                    Trigger::Give(i, x) => (TableTrigger::Give(i, x), vec![i, x]),
                    Trigger::Arrive(x) => (TableTrigger::Arrive(x), vec![x]),
                    Trigger::Take(x) => (TableTrigger::Take(x), vec![x]),
                    Trigger::Kill(x) => (TableTrigger::Kill(x), vec![x]),
                    Trigger::Done(q) => (TableTrigger::Done(q), Vec::new()),
                    Trigger::Given(q) => (TableTrigger::Given(q), Vec::new()),
                };
                let mut ok = true;
                for tpl in named {
                    ok &= cx.template(l, "trigger", tpl);
                }
                for tpl in s.gives.iter().chain(&s.takes) {
                    ok &= cx.template(l, "gives, takes", *tpl);
                }
                let to = match &s.to {
                    None => None,
                    Some(to) => match cx.place(&places, l, to) {
                        Some(p) => Some(StepPlace {
                            cell: p.cell,
                            origin: p.origin,
                            heading: heading_byte(rotation_heading(p.rotation)),
                        }),
                        None => continue,
                    },
                };
                if !ok {
                    continue;
                }
                t.quests.steps.push(TableStep {
                    quest: s.quest,
                    needs: s.needs,
                    trigger,
                    tell: words(s.tell, s.tell_text.as_ref()),
                    update: match s.update {
                        Update::Bestow => TableUpdate::Bestow,
                        Update::Advance => TableUpdate::Advance(s.phase.unwrap_or(0)),
                        Update::Complete => TableUpdate::Complete,
                        Update::Fail => TableUpdate::Fail,
                    },
                    xp: s.xp.unwrap_or(0),
                    gives: s.gives.clone(),
                    counts: s.counts.clone(),
                    effects: s.effects.clone(),
                    text: s.text.map(|r| (r.string, r.table)),
                    placed: prov_text(&s.prov),
                    requires: s
                        .requires
                        .iter()
                        .map(|(q, n)| {
                            (
                                *q,
                                match n {
                                    Need::Phase(p) => TableNeed::Phase(*p),
                                    Need::Done => TableNeed::Done,
                                    Need::Free => TableNeed::Free,
                                },
                            )
                        })
                        .collect(),
                    takes: s.takes.clone(),
                    races: s.races.clone(),
                    effects_last: s.effects_last,
                    fellows: s.fellows,
                    wants: s.wants,
                    to,
                });
            }
            Row::Give(g) => {
                let mut ok = cx.template(l, "to", g.to);
                for tpl in g.item.iter().chain(&g.gives) {
                    ok &= cx.template(l, "item, gives", *tpl);
                }
                if !ok {
                    continue;
                }
                t.quests.gives.push(TableGive {
                    to: g.to,
                    item: g.item,
                    wants: g.wants,
                    gives: g.gives.clone(),
                    counts: g.counts.clone(),
                    effects: g.effects.clone(),
                    text: g.text.map(|r| (r.string, r.table)),
                    fx: g.fx,
                    requires: g
                        .requires
                        .iter()
                        .map(|(q, n)| {
                            (
                                *q,
                                match n {
                                    Need::Phase(p) => TableNeed::Phase(*p),
                                    Need::Done => TableNeed::Done,
                                    Need::Free => TableNeed::Free,
                                },
                            )
                        })
                        .collect(),
                    placed: prov_text(&g.prov),
                });
            }
            Row::Craft(c) => {
                if let Some(k) = c.skill {
                    t.crafting.skills.push(k);
                }
                if let Some(r) = c.recipe {
                    t.crafting.recipes.push(r);
                }
            }
            Row::Butchery(b) => {
                let mut known = true;
                for i in &b.items {
                    known &= cx.template(l, "items", *i);
                }
                if known {
                    t.crafting.butchery.push(yalain_content::ButcheryYields {
                        profile: b.profile,
                        items: b.items.clone(),
                        weights: b.weights.clone(),
                        empty: b.empty,
                    });
                }
            }
            Row::Scene(r) => {
                let sub = |x: &Option<crate::rows::StrRef>| x.as_ref().map(|r| (r.string, r.table));
                t.story.scenes.insert(
                    r.id,
                    yalain_content::story::Scene {
                        id: r.id,
                        act: r.act,
                        number: r.number,
                        name: (r.name.string, r.name.table),
                        hidden: r.hidden,
                        playable: r.playable,
                        keyframes: r
                            .keyframes
                            .iter()
                            .map(|k| yalain_content::story::Keyframe {
                                seconds: k.seconds,
                                picture: k.picture.unwrap_or(0),
                                music: k.music.unwrap_or(0),
                                voice: k.voice.unwrap_or(0),
                                subtitle: sub(&k.subtitle),
                                tracks: k.tracks,
                            })
                            .collect(),
                    },
                );
                if let Some(order) = r.at_start {
                    starts.push((order, r.id));
                }
            }
            Row::Shard(r) => {
                t.story.shards.insert(r.object, r.scene);
                if let Some(quest) = r.quest {
                    t.story.shard_quests.insert(r.object, quest);
                }
            }
            Row::Weapon(r) => {
                if cx.template(l, "template", r.template) {
                    t.weapons.insert(r.name.clone(), r.template);
                    if r.two_share.is_some() || r.stance.is_some() {
                        t.weapon_rules.insert(
                            r.name.clone(),
                            yalain_content::WeaponRule {
                                two_share: r.two_share,
                                stance: r.stance,
                            },
                        );
                    }
                }
            }
            Row::Place(p) => {
                if p.named_by.is_empty() && p.destination.is_none() {
                    continue;
                }
                let Some(at) = cx.place_row(l) else {
                    continue;
                };
                if let Some(name) = &p.destination {
                    if t.places_by_name
                        .insert(name.to_ascii_lowercase(), at)
                        .is_some()
                    {
                        cx.problem(
                            l,
                            format!("destination: {name} is another place's name too"),
                        );
                    }
                }
                for &template in &p.named_by {
                    if t.places_named.insert(template, at).is_some() {
                        cx.problem(
                            l,
                            format!("named_by: {template:08X} is named by another place too"),
                        );
                    }
                }
            }
            Row::Area(r) => {
                if r.conflict == yalain_content::Conflict::Peace {
                    t.content.conflict.remove(&r.landblock);
                } else {
                    t.content.conflict.insert(r.landblock, r.conflict);
                }
            }
            Row::Talk(_) | Row::Route(_) | Row::Remove(_) => {}
        }
    }
    for (l, scene) in door_scenes {
        if !t.story.scenes.contains_key(&scene) {
            cx.problem(l, format!("scene: no scene {scene:08X} is served"));
        }
    }
    starts.sort_unstable();
    t.story.at_start = starts.into_iter().map(|(_, id)| id).collect();
    // What people say, once every placed person has its place in the content pack.
    for l in &merged.rows {
        let Row::Talk(r) = &l.row else { continue };
        let mut ok = cx.template(l, "who", r.who);
        if let Some(item) = r.item {
            ok &= cx.template(l, "item", item);
        }
        let object = match &r.object {
            None => None,
            Some(o) => {
                let (pack, key) = match o.split_once(':') {
                    Some((p, k)) => (p.to_string(), k.to_string()),
                    None => (l.pack.clone(), o.clone()),
                };
                let (key, lb) = key.split_once('@').map_or((key.clone(), 0), |(k, b)| {
                    (k.to_string(), u16::from_str_radix(b, 16).unwrap_or(0))
                });
                if let Some(i) = objects_at.get(&(pack, key, lb)) {
                    Some(*i)
                } else {
                    cx.problem(l, format!("object: no person {o} is served"));
                    continue;
                }
            }
        };
        let Some(words) = words(r.line, r.line_text.as_ref()) else {
            continue;
        };
        if !ok {
            continue;
        }
        t.talk.lines.push(Line {
            who: r.who,
            object,
            words,
            role: r.role,
            how: r.how,
            when: r.when,
            quest: r.quest,
            update: r.update,
            phase: r.phase,
            item: r.item,
            order: r.order.unwrap_or(0),
            measured: r.prov.source == Source::Capture,
            placed: prov_text(&r.prov),
        });
    }
    subquest_definitions(&mut t.quests, install);
    if cx.problems.is_empty() {
        t.warnings = cx.warnings;
        Ok(t)
    } else {
        cx.problems.sort();
        Err(cx.problems)
    }
}
