//! A merged database compiled against an install into the tables the server reads: the content
//! pack (every person and static object described from its template), the spawn, loot, store,
//! quest and talk tables. What a row leaves out the install supplies: a keeper's store, a sale's
//! product, price, currency and name, a quest's name, description, icon and journal; every data
//! id a row names is checked against the install.
//!
//! What is said (a step's line, a keeper's greeting, a person's line) goes out as the string
//! reference a row gives; a row with only new prose (`tell_text`, `greeting_text`, `line_text`)
//! has it said as literal text, which the client shows unchanged. An object's `name_text` alone is
//! its name as literal text too, as a player's is told; a sale's alone is served under the
//! template's own name.
//!
//! Each table row's `placed` column carries its provenance, so a running server can say where a
//! creature or a sale came from.

use std::collections::BTreeMap;

use auberean_assets::entity::Entity;
use auberean_assets::script::Qualities;
use dereth_primitives::num::math;
use yalain_content::quests::{
    Give as TableGive, Need as TableNeed, Place as StepPlace, QuestDef, QuestTable,
    Room as TableRoom, Step as TableStep, Trigger as TableTrigger, Update as TableUpdate,
};
use yalain_content::stores::{Keeper as TableKeeper, Sale as TableSale, SaleGate, StoreTable};
use yalain_content::talk::{Line, TalkTable};
use yalain_content::{
    describe, facts, Camp as TableCamp, ChestTreasure, ContentObject, ContentPack, FactIds, Kind,
    LootProfile, LootTable, Place as TablePlace, SpawnTable, Words,
};

use crate::merge::Merged;
use crate::rows::{
    At, EffectOf, Gate, Located, Need, ObjectKind, Prov, Row, Source, StrRef, Trigger, Update,
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

/// A quest's definition, as the install holds it: name (a string reference, or the words standing
/// in for one the install's text can't write) and description (string, table), icon, whether
/// updates play an effect, the journal entry of each phase from the first, and the effects each
/// kind of update puts on the player (the kind, the effect's saved state).
#[derive(Debug, Clone, PartialEq)]
pub struct QuestDefinition {
    pub state: u32,
    pub name: Words,
    pub description: (u32, u32),
    pub icon: u32,
    pub play_fx: bool,
    pub journals: Vec<(u32, u32)>,
    /// The effects each kind of update puts on: the kind, the effect's saved state, and the
    /// spellcraft its record puts it on at.
    pub effects: Vec<(u32, u32, f32)>,
    /// The phases done by other quests (subquests), in their order.
    pub subquests: Vec<yalain_content::quests::Subquests>,
    /// Done once: its package's setup says it may be done one time.
    pub once: bool,
    /// How a kill's move on it reaches the killer's fellows, as its package's setup says.
    pub fellows: yalain_content::quests::Fellows,
    /// The range each phase is counted in, from the first (empty: every phase's is 1), as the
    /// definition's phases say.
    pub ranges: Vec<u32>,
    /// The least level a player is given it at (0: any), as its package's setup says.
    pub min_level: i32,
    /// Given only to heroes, as its package's setup says.
    pub hero_only: bool,
    /// For the monarch of an allegiance alone, as its package's setup says. Read, and nothing yet
    /// holds a quest to it.
    pub monarch_only: bool,
    /// What fails it, as its package's setup says.
    pub fails: FailsOn,
    /// The experience its completion pays, as its package's setup says.
    pub xp: u64,
    /// How its completion's experience is worked out, as its package's setup says.
    pub pay: yalain_content::quests::CompletionPay,
    /// A quest a player is to have at a status to be given it, as its package's setup says.
    pub required: Option<(u32, u32)>,
}

/// What fails a quest under way, as its setup says: its player's death, its leaving the world, its
/// being taken through portal space.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct FailsOn {
    pub death: bool,
    pub logout: bool,
    pub teleport: bool,
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
    /// The variables a string of the install's text takes, by their ids in its order, when the
    /// text is at hand (an empty list for a string it does not hold).
    fn variables(&self, _string: u32, _table: u32) -> Option<Vec<u32>> {
        None
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
    /// What creatures' death effects make, by the effects' names.
    pub death_spawns: BTreeMap<String, yalain_content::DeathSpawn>,
    /// The kinds of pet the rows make, by effect, and what each summoning skill allows its user.
    pub pet_kinds: BTreeMap<u32, yalain_content::PetKind>,
    pub pet_limits: BTreeMap<u32, Vec<(usize, u32)>>,
    /// The places templates name: where a person who sends players by a map sends the holder of
    /// a map of that template.
    pub places_named: BTreeMap<u32, TablePlace>,
    /// The places by their names (lowercase): where an effect that sends a player to a named place
    /// (a city's recall) sends it.
    pub places_by_name: BTreeMap<String, TablePlace>,
    /// The saved states effects' names stand for where the install holds no effect by the name.
    pub effect_names: BTreeMap<String, u32>,
    /// The kingdom rows: the effects that raise kingdom points, and the steps and gives for one
    /// kingdom's players alone.
    pub kingdoms: yalain_content::kingdoms::KingdomTable,
    /// The tips a player is shown once, by moment.
    pub popups: yalain_content::popups::PopupTable,
    /// What effects do where their own data says nothing: what wards keep off.
    pub effect_rules: yalain_content::effect_rules::EffectRules,
    /// What mining's nodes do and where surveyor's maps lead, each kind only when it has rows.
    pub gathering: yalain_content::gathering::Gathering,
    /// Crafting's own work: map recipes' waits, spellbinding's totems, refining's products, each
    /// kind only when it has rows.
    pub crafts: yalain_content::crafts::Crafts,
    /// The dye plants: species and crosses, each kind only when it has rows.
    pub plants: yalain_content::plants::DyePlants,
    /// The worn looks of templates whose data names none, by template in the rows' order.
    pub worn: yalain_content::worn_table::WornLooks,
    /// The treasure numbers the world gives: its rules and its sorts' numbers, each kind only
    /// when it has rows of it.
    pub treasure: yalain_content::treasure_tables::TreasureRows,
    /// What items' uses do where their templates name nothing, what books, cards and people show,
    /// and what every new character is given.
    pub items: yalain_content::item_rows::ItemRows,
    /// What the world says of creatures' behavior: whom concealing effects spare or make hated,
    /// who heals its kin, who never attacks first, who fights whom, shooters' rhythms, who marks the
    /// players it sees, who takes on only those it hates, whom players may not harm.
    pub creatures: yalain_content::creature_rows::CreatureRows,
    /// The world's own shops: each a copy of a store template under its title, by key.
    pub shops: Vec<Shop>,
    /// What the compile left out or could not serve, and why.
    pub warnings: Vec<Problem>,
}

/// A shop of the world's own (`[[store]]`): the store template it copies, the id its copy has, and
/// the title and line the copy shows. Its keepers sell the template's sales; the shop window names
/// the copy.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Shop {
    pub key: String,
    pub from: u32,
    pub id: u32,
    pub name_text: String,
    pub description_text: Option<String>,
    /// Whether a lockfile gives it its id; else this compile numbered it, and the world builder
    /// keeps the number in the world's lockfile.
    pub pinned: bool,
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
/// Does a step's or a give's gate name an order of uses keeping its own count, at a count it may
/// reach? A problem names the field where it does not.
fn sequence_gate_holds(
    cx: &mut Ctx<'_>,
    t: &Tables,
    l: &Located,
    g: &yalain_content::item_rows::SequenceGate,
) -> bool {
    use yalain_content::item_rows::SequenceNeed;
    let Some(q) = t.items.sequences.iter().find(|q| q.key == g.key) else {
        cx.problem(l, format!("requires: no [[sequence]] {} is served", g.key));
        return false;
    };
    if q.quest.is_some() {
        cx.problem(
            l,
            format!(
                "requires: [[sequence]] {} counts by its quest's phase: name the quest",
                g.key
            ),
        );
        return false;
    }
    if let SequenceNeed::At(n) = g.need {
        if usize::try_from(n).map_or(true, |n| n > q.uses.len()) {
            cx.problem(
                l,
                format!(
                    "requires: [[sequence]] {} has {} uses, not {n}",
                    g.key,
                    q.uses.len()
                ),
            );
            return false;
        }
    }
    true
}

/// Is a row's name for what it makes one a creature may take, where the template is a creature's?
/// A creature is named by a string reference alone, which its examination and its corpse are
/// named by too; new prose alone is a problem.
fn named_as_a_creature_is(
    cx: &mut Ctx<'_>,
    l: &Located,
    field: &str,
    name: &Option<Words>,
    props: &yalain_content::Props,
) -> bool {
    let prose = matches!(name, Some(Words::Literal(_)));
    if prose && yalain_content::CreatureStats::from_props(props).is_some() {
        cx.problem(
            l,
            format!("{field}: a creature is named by a string reference, which its corpse is named after too"),
        );
        return false;
    }
    true
}

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

/// A row's `requires` as the gate the server judges, its status written as a template's is.
const fn quest_gate((quest, g): (u32, Gate)) -> yalain_content::QuestGate {
    yalain_content::QuestGate {
        quest,
        status: match g {
            Gate::Done => QUEST_COMPLETED,
            Gate::UnderWay => QUEST_UNDERWAY,
            Gate::Phase(n) => QUEST_UNDERWAY | n,
            Gate::Had => yalain_content::QuestGate::HAD,
        },
    }
}

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

    /// A template of the install that gives a material when it is mined.
    fn node(&mut self, l: &Located, field: &str, t: u32) -> bool {
        if !self.template(l, field, t) {
            return false;
        }
        let install = self.install;
        let props =
            yalain_content::Props::of(t, &|id| install.entity(id), &|id| install.property_name(id));
        if props.word("Mine_Object").is_some() {
            true
        } else {
            self.problem(
                l,
                format!("{field}: template {t:08X} gives no material when mined"),
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
        self.place_for(places, l, "to", to)
    }

    /// Where the place a row's field names stands (`key`, or `pack:key`), or a problem naming the
    /// field.
    fn place_for(
        &mut self,
        places: &BTreeMap<(String, String), &Located>,
        l: &Located,
        field: &str,
        to: &str,
    ) -> Option<TablePlace> {
        let (pack, key) = match to.split_once(':') {
            Some((p, k)) => (p.to_string(), k.to_string()),
            None => (l.pack.clone(), to.to_string()),
        };
        let Some(pl) = places.get(&(pack, key)) else {
            self.problem(l, format!("{field}: no place {to}"));
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
            cancel_wait: None,
            xp: 0,
            cancellable: false,
            placed: format!("derived subquest from {:08X}", def.state),
            hidden: true,
            effects: def.effects,
            subquests: def.subquests,
            min_level: def.min_level,
            fails_on_death: def.fails.death,
            fails_on_logout: def.fails.logout,
            fails_on_teleport: def.fails.teleport,
            once: def.once,
            fellows: def.fellows,
            ranges: def.ranges,
            hero_only: def.hero_only,
            required: def.required,
            pay: yalain_content::quests::CompletionPay::default(),
        });
    }
}

/// How a quest's completion pays: as its setup says, unless its row says otherwise. A row's amount
/// is paid in place of the setup's share, weighed as the setup says unless the row says it is whole. A
/// row's share is paid whole, as nearly every share a setup names is (89 of 91), and at most what it
/// comes to at the row's level, within the row's least and most.
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
            least: q.xp_least.unwrap_or(0),
            most: q.xp_most.unwrap_or(0),
            flat: true,
            ..CompletionPay::default()
        };
    }
    if q.xp.is_some() {
        return CompletionPay {
            share: 0.0,
            flat: q.xp_whole.unwrap_or(setup.flat),
            ..setup
        };
    }
    setup
}

/// The store template a storekeeper template opens, as its template names it.
#[must_use]
pub fn keeper_store(install: &dyn Install, template: u32) -> Option<u32> {
    yalain_content::Props::of(template, &|id| install.entity(id), &|id| {
        install.property_name(id)
    })
    .word("StoreTemplate")
}

/// The id a new shop's copy takes: the lowest of the shops' copies' saved states
/// ([`auberean_pack::SHOP_COPIES`]) not `taken`; never one of the world's recipes'.
#[must_use]
pub fn next_shop_id(taken: &dyn Fn(u32) -> bool) -> Option<u32> {
    let (first, last) = auberean_pack::SHOP_COPIES;
    (first..=last).find(|id| !taken(*id))
}

/// The world's shops, as a compile numbers them ([`Tables::shops`]).
///
/// # Errors
/// A shop copying what is no store template, and a lockfile giving a shop an id outside the
/// shops' copies' saved states ([`auberean_pack::SHOP_COPIES`]) or two shops one id.
pub fn shops_of(merged: &Merged, install: &dyn Install) -> Result<Vec<Shop>, Vec<Problem>> {
    let mut cx = Ctx {
        install,
        problems: Vec::new(),
        warnings: Vec::new(),
    };
    let out = shops(merged, &mut cx);
    if cx.problems.is_empty() {
        Ok(out)
    } else {
        Err(cx.problems)
    }
}

/// The world's shops, each with its copy's id: the one a lockfile gives its key, else the lowest
/// of the shops' copies' saved states ([`auberean_pack::SHOP_COPIES`]) no lockfile gives, taken in
/// the order of the keys, so every compile of the same rows numbers them alike.
fn shops(merged: &Merged, cx: &mut Ctx<'_>) -> Vec<Shop> {
    let (first, last) = auberean_pack::SHOP_COPIES;
    let mut taken: BTreeMap<u32, &str> = BTreeMap::new();
    for (key, id) in &merged.stores {
        if let Some(other) = taken.insert(*id, key) {
            cx.problems.push(Problem::new(
                "world.lock",
                0,
                format!("store: {other} and {key} are both given {id:#010X}"),
            ));
        }
    }
    let mut rows: Vec<(&Located, &crate::rows::Store)> = merged
        .rows
        .iter()
        .filter_map(|l| match &l.row {
            Row::Store(s) => Some((l, s)),
            _ => None,
        })
        .collect();
    rows.sort_by(|a, b| a.1.key.cmp(&b.1.key));
    let mut out = Vec::new();
    for (l, s) in rows {
        if cx.install.store_sales(s.from).is_none() {
            cx.problem(l, format!("from: {:08X} is no store template", s.from));
            continue;
        }
        let (id, pinned) = match merged.stores.get(&s.key) {
            Some(id) if (first..=last).contains(id) => (*id, true),
            Some(id) => {
                cx.problem(
                    l,
                    format!(
                        "world.lock gives store {} {id:#010X}, outside the shops' copies ({first:#010X} to {last:#010X})",
                        s.key
                    ),
                );
                continue;
            }
            None => {
                let Some(next) = next_shop_id(&|id| taken.contains_key(&id)) else {
                    cx.problem(l, "no saved state is left for the shop's copy");
                    continue;
                };
                taken.insert(next, &s.key);
                (next, false)
            }
        };
        out.push(Shop {
            key: s.key.clone(),
            from: s.from,
            id,
            name_text: s.name_text.clone(),
            description_text: s.description_text.clone(),
            pinned,
        });
    }
    out
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
    // Each camp's row in the spawn table, by its key and landblock; and the steps a kill of one
    // camp's creature triggers, to point at that row once every camp has its own.
    let mut camps_at: BTreeMap<(&str, u16), usize> = BTreeMap::new();
    let mut kills_in: Vec<(&Located, usize, &str)> = Vec::new();
    // The steps an event gates, by their row in the quest table, to copy once each is whole.
    let mut event_steps: Vec<(usize, String)> = Vec::new();
    let mut kingdom_steps: Vec<(usize, u32)> = Vec::new();
    // The steps a craft skill gates, likewise.
    let mut craft_steps: Vec<(usize, yalain_content::crafts::CraftGate)> = Vec::new();
    // The steps an untraining under way gates, likewise.
    let mut untraining_steps: Vec<usize> = Vec::new();
    // The steps for heroes alone, likewise.
    let mut hero_steps: Vec<usize> = Vec::new();
    // The steps and gives that wait on their player having fought a camp's creature, with the camps
    // they name, resolved once every camp has its row.
    let mut fought_steps: Vec<(&Located, usize, &[String])> = Vec::new();
    let mut fought_gives: Vec<(&Located, TableGive, &[String])> = Vec::new();
    // The item rows whose use puts effects on a camp's creatures, by their place in the item
    // rows, with the camp they name, likewise.
    let mut effects_on_rows: Vec<(&Located, usize, &str, &[u32])> = Vec::new();
    // The steps that also give one of several items, likewise.
    let mut pick_steps: Vec<(usize, Vec<(u32, u32)>)> = Vec::new();
    // The steps and gives an order of uses keeping its own count gates, likewise, checked once
    // every order has its row.
    let mut sequence_steps: Vec<(&Located, usize, yalain_content::item_rows::SequenceGate)> =
        Vec::new();
    let mut sequence_gives: Vec<(
        &Located,
        yalain_content::quests::Give,
        yalain_content::item_rows::SequenceGate,
    )> = Vec::new();
    // The steps a tally gates or adds to, likewise.
    let mut tally_steps: Vec<(usize, yalain_content::tallies::Gate)> = Vec::new();
    let mut tally_step_adds: Vec<(usize, yalain_content::tallies::Adds)> = Vec::new();
    // The steps that set an escort following, likewise, once every pet kind is known.
    let mut escort_steps: Vec<(&Located, usize, yalain_content::escorts::Escort)> = Vec::new();
    let mut act_steps: Vec<(&Located, usize, u32)> = Vec::new();
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
    t.shops = shops(merged, &mut cx);
    let mut rooms: BTreeMap<(String, String), Vec<TableRoom>> = BTreeMap::new();
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
                }
                if o.name.is_none() {
                    d.name_text.clone_from(&o.name_text);
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
                let gate = o.requires.map(quest_gate);
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
                    let lock = yalain_content::Lock { key: o.lock };
                    match (o.kind, o.id) {
                        (ObjectKind::Static, Some(id)) => {
                            t.locks.objects.insert(id, lock);
                        }
                        (ObjectKind::Static, None) => {
                            // One the server numbers is known by where it stands and what it is.
                            let cell = o.cell.unwrap_or_else(|| outdoor_cell(lb, o.at.x, o.at.y));
                            if t.locks.placed.insert((cell, o.template), lock).is_some() {
                                cx.warn(
                                    l,
                                    "locked: another locked object of its template stands in its cell, and one lock holds both",
                                );
                            }
                        }
                        (ObjectKind::Npc, _) => {
                            cx.warn(l, "locked: only a static object is locked")
                        }
                    }
                }
                objects_at.insert((l.pack.clone(), o.key.clone(), lb), t.content.objects.len());
                let cell = o.cell.unwrap_or_else(|| outdoor_cell(lb, o.at.x, o.at.y));
                if let Some(event) = &o.event {
                    t.content
                        .events
                        .objects
                        .push(yalain_content::events::ObjectGate {
                            object: t.content.objects.len(),
                            event: event.clone(),
                        });
                }
                t.content.objects.push(ContentObject {
                    object: match o.kind {
                        ObjectKind::Static => o.id,
                        ObjectKind::Npc => None,
                    },
                    template: o.template,
                    description: d,
                    cell,
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
                    facts: {
                        let f = facts(o.template, &entity, &ids);
                        yalain_content::Facts {
                            portal: portal_flags(o.recall, o.summon),
                            min_level: o.min_level.or(f.min_level),
                            hero: o.hero,
                            kingdom: o.kingdom,
                            ..f
                        }
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
                camps_at.insert((c.key.as_str(), lb), t.spawns.camps.len());
                if let Some(event) = &c.event {
                    t.content.events.camps.push((id.clone(), event.clone()));
                }
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
                    name: c.name.map(pair),
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
                if !r.draws.is_empty() || r.rare.is_some() {
                    t.loot.pools.profiles.insert(
                        name.clone(),
                        yalain_content::pools::Draws {
                            draws: r.draws.iter().map(pool_entry).collect(),
                            rare: r.rare.clone(),
                        },
                    );
                }
                t.loot.profiles.push(LootProfile {
                    name,
                    counts: r.counts.clone(),
                    items: r.items.clone(),
                    extras: r.extras.clone(),
                    placed: prov_text(&r.prov),
                    confidence: confidence_word(&r.prov),
                    chest: match (r.level, r.value.as_slice()) {
                        (Some(level), [low, high]) => Some(ChestTreasure {
                            level,
                            value: (*low, *high),
                        }),
                        _ => None,
                    },
                });
            }
            Row::TreasurePool(p) => {
                if !p
                    .items
                    .iter()
                    .all(|e| e.template.is_none_or(|t| cx.template(l, "items", t)))
                {
                    continue;
                }
                let race = p.race.as_deref().map_or(0, |r| {
                    crate::rows::POOL_RACES
                        .iter()
                        .find(|(n, _)| *n == r)
                        .map_or(0, |(_, b)| *b)
                });
                t.loot.pools.pools.insert(
                    p.name.clone(),
                    yalain_content::pools::Pool {
                        name: p.name.clone(),
                        race,
                        own: p.own.unwrap_or(0.0),
                        entries: p.pools.iter().chain(&p.items).map(pool_entry).collect(),
                    },
                );
            }
            // Read whole, below.
            Row::TreasureRules(_)
            | Row::TreasureWord(_)
            | Row::TreasureLook(_)
            | Row::TreasurePotion(_)
            | Row::TreasureSort(_)
            | Row::TreasureFamily(_) => {}
            Row::TreasureValue(r) => {
                t.loot
                    .values
                    .insert(i32::try_from(r.level).unwrap_or(i32::MAX), r.values.clone());
            }
            Row::Keeper(k) => {
                if !cx.template(l, "template", k.template) {
                    continue;
                }
                let shop = match &k.shop {
                    None => None,
                    Some(key) => match t.shops.iter().find(|s| s.key == *key) {
                        Some(s) => Some((s.from, s.id)),
                        None => {
                            cx.problem(l, format!("shop: no [[store]] {key}"));
                            continue;
                        }
                    },
                };
                if let (Some(store), Some((from, _))) = (k.store, shop) {
                    if store != from {
                        cx.problem(
                            l,
                            format!(
                                "store, shop: the shop copies store {from:08X}; leave store out"
                            ),
                        );
                        continue;
                    }
                }
                let store = k
                    .store
                    .or(shop.map(|(from, _)| from))
                    .or_else(|| keeper_store(install, k.template));
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
                    shown: shop.map(|(_, id)| id),
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
                match q.companion_of {
                    Some(of) if of == q.id => {
                        cx.problem(l, "companion_of: a quest cannot go with itself");
                    }
                    Some(of) if !quest_rows.contains(&of) => cx.problem(
                        l,
                        format!(
                            "companion_of: quest {of:08X} has no row, so no player ever holds it"
                        ),
                    ),
                    Some(of) => t.content.companions.entry(of).or_default().push(q.id),
                    None => {}
                }
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
                    cancel_wait: q.cancel_wait,
                    xp: q.xp.unwrap_or(def.xp),
                    cancellable: q.cancellable.unwrap_or(false),
                    placed: prov_text(&q.prov),
                    // A quest is shown when its row says so, else when its definition has a
                    // description to show.
                    hidden: !q.visible.unwrap_or(def.description.0 != 0),
                    effects: def.effects,
                    subquests: def.subquests,
                    min_level: q.min_level.unwrap_or(def.min_level),
                    // What fails it: as its row says, else as its setup does.
                    fails_on_death: q.fails_on_death.unwrap_or(def.fails.death),
                    fails_on_logout: q.fails_on_logout.unwrap_or(def.fails.logout),
                    fails_on_teleport: q.fails_on_teleport.unwrap_or(def.fails.teleport),
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
                    ranges: def.ranges,
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
                    Trigger::GiveClass(c, x) => (TableTrigger::GiveClass(c, x), vec![x]),
                    Trigger::Arrive(x) => (TableTrigger::Arrive(x), vec![x]),
                    Trigger::ArriveFrom(x, o) => (TableTrigger::ArriveFrom(x, o), vec![x]),
                    Trigger::Take(x) => (TableTrigger::Take(x), vec![x]),
                    Trigger::Kill(x) => (TableTrigger::Kill(x), vec![x]),
                    // The camp's row comes once every camp has its own.
                    Trigger::KillIn(x, _) => (TableTrigger::KillIn(x, u32::MAX), vec![x]),
                    Trigger::KillNear(x, m) => (TableTrigger::KillNear(x, m), vec![x]),
                    Trigger::HurtNear(x, m) => (TableTrigger::HurtNear(x, m), vec![x]),
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
                for (tpl, _) in &s.gives_one_of {
                    ok &= cx.template(l, "gives_one_of", *tpl);
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
                if let Trigger::KillIn(_, camp) = &s.trigger {
                    kills_in.push((l, t.quests.steps.len(), camp.as_str()));
                }
                if !s.gives_one_of.is_empty() {
                    pick_steps.push((t.quests.steps.len(), s.gives_one_of.clone()));
                }
                for g in &s.sequences {
                    sequence_steps.push((l, t.quests.steps.len(), g.clone()));
                }
                for e in &s.events {
                    event_steps.push((t.quests.steps.len(), e.clone()));
                }
                for g in &s.tallies {
                    tally_steps.push((t.quests.steps.len(), g.clone()));
                }
                if let Some(a) = &s.adds {
                    tally_step_adds.push((t.quests.steps.len(), (**a).clone()));
                }
                if let Some(e) = &s.escort {
                    escort_steps.push((l, t.quests.steps.len(), (**e).clone()));
                }
                if let Some(k) = s.kingdom {
                    kingdom_steps.push((t.quests.steps.len(), k));
                }
                for c in &s.crafts {
                    craft_steps.push((t.quests.steps.len(), c.clone()));
                }
                if s.untraining {
                    untraining_steps.push(t.quests.steps.len());
                }
                if s.heroes {
                    hero_steps.push(t.quests.steps.len());
                }
                if !s.fought.is_empty() {
                    fought_steps.push((l, t.quests.steps.len(), &s.fought));
                }
                for a in &s.acts {
                    act_steps.push((l, t.quests.steps.len(), *a));
                }
                if !s.given_effects.is_empty() {
                    t.content
                        .given_effects
                        .insert((s.quest, s.key.clone()), s.given_effects.clone());
                }
                if let Some(after) = s.again_after {
                    t.content.step_waits.insert((s.quest, s.key.clone()), after);
                }
                t.quests.steps.push(TableStep {
                    key: s.key.clone(),
                    quest: s.quest,
                    needs: s.needs,
                    trigger,
                    tell: words(s.tell, s.tell_text.as_ref()),
                    update: match s.update {
                        Update::Bestow => TableUpdate::Bestow,
                        Update::Advance => TableUpdate::Advance(s.phase.unwrap_or(0)),
                        Update::Complete => TableUpdate::Complete,
                        Update::Fail => TableUpdate::Fail,
                        Update::Nothing => TableUpdate::Nothing,
                    },
                    xp: s.xp.unwrap_or(0),
                    gives: s.gives.clone(),
                    counts: s.counts.clone(),
                    effects: s.effects.clone(),
                    text: s.text.map(|r| (r.string, r.table)),
                    recipes: s.recipes.clone(),
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
                                    Need::Failed => TableNeed::Failed,
                                    Need::NeverDone => TableNeed::NeverDone,
                                },
                            )
                        })
                        .collect(),
                    takes: s.takes.clone(),
                    races: s.races.clone(),
                    effects_last: s.effects_last,
                    effects_quiet: s.effects_quiet,
                    fellows: s.fellows,
                    wants: s.wants,
                    levels: (s.least_level.is_some() || s.most_level.is_some())
                        .then_some((s.least_level.unwrap_or(0), s.most_level.unwrap_or(i32::MAX))),
                    #[allow(clippy::cast_possible_truncation)]
                    xp_share: s.xp_share.unwrap_or(0.0) as f32,
                    to,
                    transforms: s.transforms.as_deref().cloned(),
                    #[allow(clippy::cast_possible_truncation)]
                    chance: s.chance.map(|c| c as f32),
                    to_fellows: s.to_fellows,
                });
            }
            Row::Give(g) => {
                let mut ok = cx.template(l, "to", g.to);
                for tpl in g.item.iter().chain(&g.gives) {
                    ok &= cx.template(l, "item, gives", *tpl);
                }
                for (tpl, _) in &g.gives_one_of {
                    ok &= cx.template(l, "gives_one_of", *tpl);
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
                    effects_quiet: g.effects_quiet,
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
                                    Need::Failed => TableNeed::Failed,
                                    Need::NeverDone => TableNeed::NeverDone,
                                },
                            )
                        })
                        .collect(),
                    placed: prov_text(&g.prov),
                });
                if let (Some(k), Some(give)) = (g.kingdom, t.quests.gives.last()) {
                    t.kingdoms.gives.push((give.clone(), k));
                }
                if let Some(give) = t.quests.gives.last() {
                    for c in &g.crafts {
                        t.content.craft_gates.gives.push((give.clone(), c.clone()));
                    }
                    if g.untraining {
                        t.content.untraining_gates.gives.push(give.clone());
                    }
                    if g.heroes {
                        t.content.hero_gates.gives.push(give.clone());
                    }
                    if !g.fought.is_empty() {
                        fought_gives.push((l, give.clone(), &g.fought));
                    }
                    if !g.gives_one_of.is_empty() {
                        t.items
                            .give_picks
                            .push((give.clone(), g.gives_one_of.clone()));
                    }
                    for gate in &g.sequences {
                        sequence_gives.push((l, give.clone(), gate.clone()));
                    }
                }
                if let Some(c) = g.item_class {
                    let row = t.quests.gives.len() - 1;
                    t.content.class_gives.insert(row, c);
                }
                if let Some(give) = t.quests.gives.last() {
                    let tallies = &mut t.content.tallies;
                    for gate in &g.tallies {
                        tallies.gives.push((give.clone(), gate.clone()));
                    }
                    if let Some(a) = &g.adds {
                        tallies.give_adds.push((give.clone(), a.clone()));
                    }
                }
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
            Row::Tool(r) => {
                if cx.template(l, "template", r.template) {
                    t.crafting.tool_yields.insert(r.template, r.tool_yield);
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
            Row::EffectName(r) => {
                t.effect_names.insert(r.name.clone(), r.state);
            }
            Row::Popup(r) => {
                t.popups.by_moment.insert(r.on, r.effect);
            }
            Row::ItemUse(r) => {
                if cx.template(l, "template", r.template) {
                    t.items.uses.insert(
                        r.template,
                        yalain_content::item_rows::ItemUse {
                            template: r.template,
                            effects: r.effects.clone(),
                            behavior: r.behavior,
                            wears: r.wears,
                        },
                    );
                }
            }
            Row::Book(r) => {
                if cx.template(l, "template", r.template) {
                    t.items.books.insert(r.template, r.shows);
                }
            }
            Row::StartItem(r) => {
                if cx.template(l, "template", r.template) {
                    t.items.start_items.insert(r.template, r.count);
                }
            }
            Row::Interaction(r) => {
                use yalain_content::item_rows::On;
                let on = match r.on {
                    On::Template(x) => cx.template(l, "on", x),
                    On::Class(_) => true,
                };
                let mut known = cx.template(l, "item", r.item) & on;
                for (g, _) in &r.gives {
                    known &= cx.template(l, "gives", *g);
                }
                for (g, _) in &r.gated_gives {
                    known &= cx.template(l, "gated_gives", *g);
                }
                for p in &r.places {
                    known &= cx.template(l, "places", p.template);
                }
                if known {
                    if let Some((camp, effects)) = &r.effects_on {
                        effects_on_rows.push((
                            l,
                            t.items.interactions.len(),
                            camp.as_str(),
                            effects.as_slice(),
                        ));
                    }
                    t.items
                        .interactions
                        .push(yalain_content::item_rows::Interaction {
                            item: r.item,
                            on: r.on,
                            gives: r.gives.clone(),
                            count: r.count,
                            gated_gives: r.gated_gives.clone(),
                            places: r.places.clone(),
                            effects_on: None,
                            effects_here: r.effects_here.clone(),
                            tell: r.tell.map(|s| (s.string, s.table)),
                        });
                }
            }
            Row::ObjectStates(r) => {
                let st = &r.states;
                let mut known = true;
                for (template, _) in &st.templates {
                    known &= cx.template(l, "templates", *template);
                    if t.items.states_of(*template).is_some() {
                        cx.problem(
                            l,
                            format!("templates: {template:08X} holds a state of another row"),
                        );
                        known = false;
                    }
                }
                for c in &st.changes {
                    known &= cx.template(l, "changes", c.item);
                }
                if known {
                    t.items.object_states.push(st.clone());
                }
            }
            // One naming placed objects waits until every placed object has its place.
            Row::Sequence(r) if r.objects.iter().all(Option::is_none) => {
                if let Some(q) = sequence(&mut cx, &t, &places, l, r.sequence.clone(), r) {
                    t.items.sequences.push(q);
                }
            }
            Row::Sequence(_) => {}
            Row::Concealing(r) => {
                t.creatures.concealing.insert((r.effect, r.class), r.regard);
            }
            Row::NpcTable(r) => {
                t.creatures.heals_kin.insert(r.name.clone(), r.heals_kin);
            }
            Row::FriendTable(r) => {
                if let Some(first) = r.attacks_first {
                    t.creatures.attacks_first.insert(r.table, first);
                }
                if !r.fights.is_empty() {
                    t.creatures
                        .fights
                        .insert(r.table, r.fights.iter().copied().collect());
                }
            }
            Row::Creature(r) => {
                if cx.template(l, "template", r.template) {
                    if let Some(rhythm) = r.rhythm {
                        t.creatures.rhythms.insert(r.template, rhythm);
                    }
                    if let Some(mark) = &r.marks {
                        t.creatures.marks.insert(r.template, mark.clone());
                    }
                    if r.only_hated {
                        t.creatures.only_hated.insert(r.template);
                    }
                    if r.players_kept_out {
                        let line = r.kept_out_line.map(|l| (l.string, l.table));
                        t.creatures.kept_out.insert(r.template, line);
                    }
                }
            }
            Row::Defense(r) => {
                let d = &r.defense;
                let mut ok = cx.template(l, "device", d.device);
                for (s, _) in &d.spawns {
                    ok &= cx.template(l, "spawns", *s);
                }
                if let Some(then) = d.then {
                    ok &= cx.template(l, "then", then);
                    let props =
                        yalain_content::Props::of(then, &entity, &|id| install.property_name(id));
                    ok &=
                        named_as_a_creature_is(&mut cx, l, "then_name_text", &d.then_name, &props);
                }
                if ok {
                    t.content.defenses.rows.push(d.clone());
                }
            }
            Row::EffectRule(r) => {
                if let Some(p) = r.places.clone() {
                    if cx.template(l, "places", p.template) {
                        match &r.effect {
                            EffectOf::State(e) => {
                                t.effect_rules.places.insert(*e, p);
                            }
                            EffectOf::Name(n) => {
                                t.effect_rules.places_named.insert(n.clone(), p);
                            }
                        }
                    }
                }
                let Some(effect) = r.effect.state() else {
                    continue;
                };
                if let Some(ward) = &r.ward {
                    t.effect_rules.wards.insert(effect, ward.clone());
                }
                if let Some(share) = r.vitae_share {
                    t.effect_rules.vitae_taps.insert(effect, share);
                }
                if let Some(restore) = r.vitae_restore {
                    t.effect_rules.vitae_restores.insert(effect, restore);
                }
                if let Some(then) = r.then {
                    t.effect_rules.immunities.insert(effect, then);
                }
                if let Some(reset) = r.reset {
                    t.effect_rules.resets.insert(effect, reset);
                }
                if r.untrains {
                    t.effect_rules.untrains.insert(effect);
                }
                if let Some(skill) = r.skill {
                    t.effect_rules.skills.insert(effect, skill);
                }
                if let Some(g) = r.gives {
                    if cx.template(l, "gives", g.template) {
                        t.effect_rules.grants.insert(effect, g);
                    }
                }
                if !r.dispels.is_empty() {
                    t.effect_rules.dispels.insert(effect, r.dispels.clone());
                }
            }
            Row::Mine(r) => {
                let m = &r.mine;
                let mut known = true;
                for (field, n) in m.delving.iter().map(|(_, n)| ("delving", *n)) {
                    known &= cx.node(l, field, n);
                }
                for (field, list) in [
                    ("tools", m.tools.clone()),
                    ("lodestones", m.lodestones.iter().map(|(_, t)| *t).collect()),
                    ("finds", m.finds.iter().map(|(t, _)| *t).collect::<Vec<_>>()),
                ] {
                    for t in list {
                        known &= cx.template(l, field, t);
                    }
                }
                if known {
                    t.gathering.mines.insert(m.skill, m.clone());
                }
            }
            Row::MapRegion(r) => {
                let mut known = true;
                for (_, n) in &r.region.nodes {
                    known &= cx.node(l, "nodes", *n);
                }
                if known {
                    t.gathering
                        .map_regions
                        .insert(r.region.region.clone(), r.region.clone());
                }
            }
            Row::MapRecipes(r) => {
                t.crafts
                    .map_recipes
                    .insert(r.recipes.region.clone(), r.recipes.clone());
            }
            Row::Spellbinding(r) => {
                if cx.template(l, "template", r.totem.template) {
                    t.crafts.totems.push(r.totem);
                }
            }
            Row::Refining(r) => {
                if cx.template(l, "product", r.refining.product) {
                    t.crafts
                        .refining
                        .insert(r.refining.recipe.clone(), r.refining.clone());
                }
            }
            Row::Recipe(r) => {
                let known = r
                    .recipe
                    .ingredients
                    .iter()
                    .flat_map(|i| &i.items)
                    .fold(cx.template(l, "product", r.recipe.product), |ok, t| {
                        cx.template(l, "ingredients", *t) && ok
                    });
                if known {
                    t.crafts.recipes.insert(r.recipe.recipe, r.recipe.clone());
                }
            }
            Row::Combination(r) => {
                let c = &r.combination;
                let known = c
                    .items
                    .iter()
                    .fold(cx.template(l, "product", c.product), |ok, t| {
                        cx.template(l, "items", *t) && ok
                    });
                if known {
                    t.crafts.combinations.push(c.clone());
                }
            }
            Row::Plant(r) => {
                use yalain_content::plants::{FERTILIZER, WATERING_CAN};
                let sp = r.species;
                let held = [
                    ("plant", Some(sp.plant)),
                    ("seed", Some(sp.seed)),
                    ("pollen", sp.pollen),
                    ("harvest", Some(sp.harvest)),
                    // The care items a row names (the dye plants' are the server's own).
                    ("water", Some(sp.water).filter(|w| *w != WATERING_CAN)),
                    (
                        "fertilizer",
                        Some(sp.fertilizer).filter(|f| *f != FERTILIZER),
                    ),
                    ("garden", sp.garden.map(|g| g.template)),
                ]
                .into_iter()
                .filter_map(|(field, t)| t.map(|t| (field, t)))
                .fold(true, |ok, (field, t)| cx.template(l, field, t) && ok);
                if held {
                    t.plants.species.insert(sp.plant, sp);
                }
            }
            Row::PlantCross(r) => {
                let c = r.cross;
                let held = [("plant", c.plant), ("pollen", c.pollen), ("seeds", c.seeds)]
                    .into_iter()
                    .fold(true, |ok, (field, t)| cx.template(l, field, t) && ok);
                if held {
                    t.plants.crosses.insert((c.plant, c.pollen), c);
                }
            }
            Row::Worn(r) => {
                use yalain_content::worn_table::LOOKS_HELD;
                let w = r.worn;
                if cx.template(l, "template", w.template) {
                    let install = cx.install;
                    let own =
                        yalain_content::Props::of(w.template, &|id| install.entity(id), &|id| {
                            install.property_name(id)
                        });
                    if own.word("Item_WornAppearanceFile1").is_some() {
                        cx.warn(
                            l,
                            format!(
                                "template: {:08X} names its own worn looks, which stand over the row",
                                w.template
                            ),
                        );
                    }
                    if t.worn.of(w.template).len() >= LOOKS_HELD {
                        cx.problem(
                            l,
                            format!(
                                "template: {:08X} has more than {LOOKS_HELD} worn looks",
                                w.template
                            ),
                        );
                    } else {
                        t.worn.push(w);
                    }
                }
            }
            Row::PointsBonus(r) => {
                t.kingdoms.points.insert(
                    r.effect,
                    yalain_content::kingdoms::PointsBonus {
                        add: r.add,
                        up_to: r.up_to,
                    },
                );
            }
            // What a failure ends with and what it hides are placed once every row is.
            Row::Event(r) => t.content.events.events.push(yalain_content::events::Event {
                key: r.key.clone(),
                game_event: r.game_event,
                windows: r.when.clone(),
                on_players: r.on_players.clone(),
                failure: None,
                hides: Vec::new(),
            }),
            Row::Tally(r) => {
                let mut known = true;
                for c in &r.tally.clears {
                    known &= cx.template(l, "clears", *c);
                }
                if known {
                    t.content.tallies.tallies.push(r.tally.clone());
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
            Row::DeathSpawn(r) => {
                let mut held = cx.template(l, "template", r.template);
                if let Some(c) = r.creature {
                    held &= cx.template(l, "creature", c);
                }
                let mut place = |field: &str, name: &Option<String>| match name {
                    Some(n) => {
                        let p = cx.place_for(&places, l, field, n);
                        held &= p.is_some();
                        p
                    }
                    None => None,
                };
                let (at, to) = (place("at", &r.at), place("to", &r.to));
                if held {
                    t.death_spawns.insert(
                        r.effect.clone(),
                        yalain_content::DeathSpawn {
                            template: r.template,
                            count: r.count,
                            chance: r.chance,
                            offset: [r.offset.x, r.offset.y],
                            lasts: r.lasts,
                            kingdom: r.kingdom,
                            at,
                            to,
                            name: r.name.clone(),
                        },
                    );
                }
            }
            Row::Pet(r) => {
                if !cx.template(l, "template", r.template) {
                    continue;
                }
                let props =
                    yalain_content::Props::of(r.template, &entity, &|id| install.property_name(id));
                let named: Vec<u32> = (1..=20)
                    .filter_map(|n| props.word(&format!("NPC_StartingSkill{n}")))
                    .collect();
                let strange: Vec<String> = r
                    .attacks
                    .iter()
                    .filter(|(skill, _)| !named.contains(skill))
                    .map(|(skill, _)| skill.to_string())
                    .collect();
                if strange.is_empty() {
                    t.pet_kinds.insert(r.effect, pet_kind(r));
                } else {
                    cx.problem(
                        l,
                        format!(
                            "attacks: template {:08X} names no skill {}",
                            r.template,
                            strange.join(", ")
                        ),
                    );
                }
            }
            Row::PetLimit(r) => {
                let limits = r.limits.iter().map(|(most, c)| (*most as usize, *c));
                t.pet_limits.insert(r.skill, limits.collect());
            }
            Row::Place(p) => {
                if let (Some(row), Some(holds), Some(at)) = (
                    &p.row,
                    p.holds,
                    p.row.is_some().then(|| cx.place_row(l)).flatten(),
                ) {
                    rooms
                        .entry((l.pack.clone(), row.clone()))
                        .or_default()
                        .push(TableRoom {
                            place: StepPlace {
                                cell: at.cell,
                                origin: at.origin,
                                heading: heading_byte(rotation_heading(at.rotation)),
                            },
                            holds,
                        });
                }
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
            Row::Talk(_)
            | Row::Route(_)
            | Row::Remove(_)
            | Row::Clearing(_)
            | Row::Store(_)
            | Row::DeathTreasure(_)
            | Row::Lever(_)
            | Row::Offering(_)
            | Row::Guarded(_)
            | Row::Shifting(_)
            | Row::Hazard(_)
            | Row::SafePassage(_)
            | Row::CreatureEffect(_) => {}
        }
    }
    for (l, scene) in door_scenes {
        if !t.story.scenes.contains_key(&scene) {
            cx.problem(l, format!("scene: no scene {scene:08X} is served"));
        }
    }
    starts.sort_unstable();
    t.story.at_start = starts.into_iter().map(|(_, id)| id).collect();
    // A kill of one camp's creature: that camp's row.
    for (l, step, camp) in kills_in {
        let (key, lb) = match camp.split_once('@') {
            Some((k, b)) => (k, u16::from_str_radix(b, 16).ok()),
            None => (camp, None),
        };
        let mut rows = camps_at
            .iter()
            .filter(|((k, b), _)| *k == key && lb.is_none_or(|lb| lb == *b));
        match (rows.next(), rows.next()) {
            (Some((_, &row)), None) => {
                if let TableTrigger::KillIn(_, c) = &mut t.quests.steps[step].trigger {
                    *c = u32::try_from(row).unwrap_or(u32::MAX);
                }
            }
            (None, _) => cx.problem(l, format!("trigger: no camp {camp} is served")),
            (Some(_), Some(_)) => cx.problem(
                l,
                format!("trigger: several camps are {camp}: name one as {camp}@XXYY"),
            ),
        }
    }
    // The camps a step or a give waits on its player having fought: their rows.
    let fought_rows = |cx: &mut Ctx, l: &Located, camps: &[String]| -> Option<Vec<u32>> {
        let mut out = Vec::new();
        for camp in camps {
            let named = camp.split_once(':').map_or(camp.as_str(), |(_, k)| k);
            let (key, lb) = match named.split_once('@') {
                Some((k, b)) => (k, u16::from_str_radix(b, 16).ok()),
                None => (named, None),
            };
            let mut rows = camps_at
                .iter()
                .filter(|((k, b), _)| *k == key && lb.is_none_or(|lb| lb == *b));
            match (rows.next(), rows.next()) {
                (Some((_, &row)), None) => out.push(u32::try_from(row).unwrap_or(u32::MAX)),
                (None, _) => {
                    cx.problem(l, format!("requires: no camp {camp} is served"));
                    return None;
                }
                (Some(_), Some(_)) => {
                    cx.problem(
                        l,
                        format!("requires: several camps are {camp}: name one as {camp}@XXYY"),
                    );
                    return None;
                }
            }
        }
        Some(out)
    };
    // The camp an item row's use puts effects on: its row.
    for (l, i, camp, effects) in effects_on_rows {
        let named = camp.split_once(':').map_or(camp, |(_, k)| k);
        let (key, lb) = match named.split_once('@') {
            Some((k, b)) => (k, u16::from_str_radix(b, 16).ok()),
            None => (named, None),
        };
        let mut rows = camps_at
            .iter()
            .filter(|((k, b), _)| *k == key && lb.is_none_or(|lb| lb == *b));
        match (rows.next(), rows.next()) {
            (Some((_, &row)), None) => {
                if let Some(r) = t.items.interactions.get_mut(i) {
                    r.effects_on = Some(yalain_content::item_rows::EffectsOn {
                        camp: row,
                        effects: effects.to_vec(),
                    });
                }
            }
            (None, _) => cx.problem(l, format!("effects_on: no camp {camp} is served")),
            (Some(_), Some(_)) => cx.problem(
                l,
                format!("effects_on: several camps are {camp}: name one as {camp}@XXYY"),
            ),
        }
    }
    for (l, i, camps) in fought_steps {
        if let (Some(rows), Some(st)) = (fought_rows(&mut cx, l, camps), t.quests.steps.get(i)) {
            t.content.fought_gates.steps.push((st.clone(), rows));
        }
    }
    for (l, give, camps) in fought_gives {
        if let Some(rows) = fought_rows(&mut cx, l, camps) {
            t.content.fought_gates.gives.push((give, rows));
        }
    }
    // The steps events gate, whole; and a gate names an event.
    for (i, e) in event_steps {
        if let Some(st) = t.quests.steps.get(i) {
            t.content.events.steps.push((st.clone(), e));
        }
    }
    // The steps tallies gate and fill, whole, likewise.
    for (i, g) in tally_steps {
        if let Some(st) = t.quests.steps.get(i) {
            t.content.tallies.steps.push((st.clone(), g));
        }
    }
    for (i, a) in tally_step_adds {
        if let Some(st) = t.quests.steps.get(i) {
            t.content.tallies.step_adds.push((st.clone(), a));
        }
    }
    for (l, i, e) in escort_steps {
        let Some(st) = t.quests.steps.get(i) else {
            continue;
        };
        if !cx.template(l, "escort", e.template) {
            continue;
        }
        if !t.pet_kinds.contains_key(&e.template) {
            cx.problem(
                l,
                format!(
                    "escort: no [[pet]] row has effect {:08X}, so nothing would follow",
                    e.template
                ),
            );
            continue;
        }
        let Some(made_at) = yalain_content::escorts::phase_after(st) else {
            cx.problem(
                l,
                "escort: a step that completes or fails its quest leaves nothing to escort",
            );
            continue;
        };
        if e.until.is_some_and(|u| u <= made_at) {
            cx.problem(
                l,
                format!(
                    "escort: until must be after the phase the step leaves its quest at, {made_at}"
                ),
            );
            continue;
        }
        t.content.escorts.steps.push((st.clone(), e));
    }
    // The steps a kingdom gates, whole, once every camp's kill has its camp.
    for (i, k) in kingdom_steps {
        if let Some(st) = t.quests.steps.get(i) {
            t.kingdoms.steps.push((st.clone(), k));
        }
    }
    // The steps a craft skill gates, likewise.
    for (i, c) in craft_steps {
        if let Some(st) = t.quests.steps.get(i) {
            t.content.craft_gates.steps.push((st.clone(), c));
        }
    }
    // The steps an untraining under way gates, likewise.
    for i in untraining_steps {
        if let Some(st) = t.quests.steps.get(i) {
            t.content.untraining_gates.steps.push(st.clone());
        }
    }
    // The steps for heroes alone, likewise.
    for i in hero_steps {
        if let Some(st) = t.quests.steps.get(i) {
            t.content.hero_gates.steps.push(st.clone());
        }
    }
    // The steps that also give one of several items, likewise.
    for (i, p) in pick_steps {
        if let Some(st) = t.quests.steps.get(i) {
            t.items.step_picks.push((st.clone(), p));
        }
    }
    // The steps an act gates, whole, likewise; and an act a step names has scenes.
    for (l, i, a) in act_steps {
        if !t.story.scenes.values().any(|sc| sc.act == a) {
            cx.problem(l, format!("requires: no scene of act {a:08X} is served"));
        }
        if let Some(st) = t.quests.steps.get(i) {
            t.story.step_acts.push((st.clone(), a));
        }
    }
    let keys: std::collections::BTreeSet<&str> = t
        .content
        .events
        .events
        .iter()
        .map(|e| e.key.as_str())
        .collect();
    for l in &merged.rows {
        let named: Vec<&String> = match &l.row {
            Row::Object(o) => o.event.iter().collect(),
            Row::Camp(c) => c.event.iter().collect(),
            Row::Step(s) => s.events.iter().collect(),
            _ => Vec::new(),
        };
        for e in named {
            if !keys.contains(e.as_str()) {
                cx.problem(l, format!("event: no [[event]] {e} is served"));
            }
        }
    }
    // An event's failure and what it hides, once every placed object and camp has its row.
    for l in &merged.rows {
        let Row::Event(r) = &l.row else {
            continue;
        };
        let hides = placed_objects(&mut cx, &objects_at, l, "hides", &r.hides);
        let failure = match &r.failure {
            None => Some(None),
            Some(f) => {
                let camp = match &f.until_killed {
                    None => Some(None),
                    Some(name) => {
                        let key = name.split_once(':').map_or(name.as_str(), |(_, k)| k);
                        let (key, lb) = match key.split_once('@') {
                            Some((k, b)) => (k, u16::from_str_radix(b, 16).ok()),
                            None => (key, l.landblock),
                        };
                        let camp = lb.and_then(|lb| camps_at.get(&(key, lb)).copied());
                        if camp.is_none() {
                            cx.problem(l, format!("failure: no camp {name} is served"));
                        }
                        camp.map(Some)
                    }
                };
                camp.map(|until_killed| {
                    Some(yalain_content::events::Failure {
                        quest: f.quest,
                        lasts: f.lasts,
                        until_killed,
                    })
                })
            }
        };
        let (Some(hides), Some(failure)) = (hides, failure) else {
            continue;
        };
        if let Some(e) = t.content.events.events.iter_mut().find(|e| e.key == r.key) {
            e.hides = hides;
            e.failure = failure;
        }
    }
    cx.problems.extend(crate::check::tallies(
        &merged.rows.iter().collect::<Vec<_>>(),
    ));
    // Levers, offerings, guarded objects and orders of uses of placed objects, once every placed
    // object has its place in the content pack (and every camp its row in the spawn table).
    for l in &merged.rows {
        match &l.row {
            Row::Sequence(r) if r.objects.iter().any(Option::is_some) => {
                let mut q = r.sequence.clone();
                let mut ok = true;
                for (i, name) in r.objects.iter().enumerate() {
                    let Some(name) = name else {
                        q.objects.push(None);
                        continue;
                    };
                    match placed_object(&mut cx, &objects_at, l, "uses", name) {
                        Some(o) => {
                            q.uses[i].0 = t.content.objects[o].template;
                            q.objects.push(Some(o));
                        }
                        None => ok = false,
                    }
                }
                if !ok {
                    continue;
                }
                if let Some(q) = sequence(&mut cx, &t, &places, l, q, r) {
                    t.items.sequences.push(q);
                }
            }
            Row::Lever(r) => {
                let levers = placed_objects(&mut cx, &objects_at, l, "levers", &r.levers);
                let opens = placed_objects(&mut cx, &objects_at, l, "opens", &r.opens);
                if let (Some(levers), Some(opens)) = (levers, opens) {
                    t.content.levers.rows.push(yalain_content::levers::Lever {
                        key: r.key.clone(),
                        levers,
                        opens,
                        open_for: r.open_for,
                        within: r.within,
                    });
                }
            }
            Row::Offering(r) => {
                let object = placed_object(&mut cx, &objects_at, l, "object", &r.object);
                let opens = placed_objects(&mut cx, &objects_at, l, "opens", &r.opens);
                let mut takes = Vec::new();
                let mut ok = true;
                for (item, shows) in &r.takes {
                    ok &= cx.template(l, "takes", *item);
                    let shows = match shows {
                        Some(name) => match placed_object(&mut cx, &objects_at, l, "takes", name) {
                            Some(i) => Some(i),
                            None => {
                                ok = false;
                                None
                            }
                        },
                        None => None,
                    };
                    takes.push(yalain_content::offerings::Part { item: *item, shows });
                }
                let (Some(object), Some(opens), true) = (object, opens, ok) else {
                    continue;
                };
                if t.content.offerings.of_object(object).is_some() {
                    cx.problem(
                        l,
                        format!("object: {} takes another row's offering", r.object),
                    );
                    continue;
                }
                t.content
                    .offerings
                    .offerings
                    .push(yalain_content::offerings::Offering {
                        key: r.key.clone(),
                        object,
                        takes,
                        opens,
                        lasts: r.lasts,
                    });
            }
            Row::Guarded(r) => {
                let objects = placed_objects(&mut cx, &objects_at, l, "objects", &r.objects);
                let key = r.camp.split_once(':').map_or(r.camp.as_str(), |(_, k)| k);
                let (key, lb) = match key.split_once('@') {
                    Some((k, b)) => (k, u16::from_str_radix(b, 16).ok()),
                    None => (key, l.landblock),
                };
                let camp = lb.and_then(|lb| camps_at.get(&(key, lb)).copied());
                if camp.is_none() {
                    cx.problem(l, format!("camp: no camp {} is served", r.camp));
                }
                let (Some(objects), Some(camp)) = (objects, camp) else {
                    continue;
                };
                t.content.guards.rows.push(yalain_content::guards::Guarded {
                    key: r.key.clone(),
                    camp,
                    objects,
                    refused: r.refused.map(|s| (s.string, s.table)),
                    hidden: r.hidden,
                    gone_for: r.gone_for,
                });
            }
            Row::Shifting(r) => {
                let Some(objects) = placed_objects(&mut cx, &objects_at, l, "objects", &r.objects)
                else {
                    continue;
                };
                if let Some(o) = objects
                    .iter()
                    .find(|o| t.content.shifting.of_object(**o).is_some())
                {
                    let name = &r.objects[objects.iter().position(|x| x == o).unwrap_or(0)];
                    cx.problem(l, format!("objects: {name} shifts in another row"));
                    continue;
                }
                t.content
                    .shifting
                    .rows
                    .push(yalain_content::shifting::Shifting {
                        key: r.key.clone(),
                        objects,
                        gone_for: r.gone_for,
                        moves_after: r.moves_after,
                    });
            }
            Row::Hazard(r) => {
                use yalain_content::hazards::{Harm, Hazard};
                let Some(objects) = placed_objects(&mut cx, &objects_at, l, "objects", &r.objects)
                else {
                    continue;
                };
                let spots: Option<Vec<_>> = r
                    .spots
                    .iter()
                    .map(|(cell, at)| cx.z(l, *at).map(|origin| (*cell, origin)))
                    .collect();
                let Some(spots) = spots else {
                    continue;
                };
                let harm = match (r.kills, r.harm) {
                    (true, _) => Some(Harm::All),
                    (false, Some(points)) => Some(Harm::Points(points)),
                    (false, None) => None,
                };
                #[allow(clippy::cast_possible_truncation)]
                let touch = r.touch.map(|m| m as f32);
                t.content.hazards.rows.push(Hazard {
                    key: r.key.clone(),
                    objects,
                    spots,
                    used: r.used,
                    touch,
                    effects: r.effects.clone(),
                    harm,
                });
            }
            Row::SafePassage(r) => {
                use yalain_content::creature_rows::{Guards, Pass, SafePassage};
                let guards = match (&r.camp, r.template) {
                    (Some(camp), _) => {
                        let named = camp.split_once(':').map_or(camp.as_str(), |(_, k)| k);
                        let (key, lb) = match named.split_once('@') {
                            Some((k, b)) => (k, u16::from_str_radix(b, 16).ok()),
                            None => (named, None),
                        };
                        let mut rows = camps_at
                            .iter()
                            .filter(|((k, b), _)| *k == key && lb.is_none_or(|lb| lb == *b));
                        match (rows.next(), rows.next()) {
                            (Some((_, &row)), None) => {
                                Guards::Camp(u32::try_from(row).unwrap_or(u32::MAX))
                            }
                            (None, _) => {
                                cx.problem(l, format!("camp: no camp {camp} is served"));
                                continue;
                            }
                            (Some(_), Some(_)) => {
                                cx.problem(
                                    l,
                                    format!(
                                        "camp: several camps are {camp}: name one as {camp}@XXYY"
                                    ),
                                );
                                continue;
                            }
                        }
                    }
                    (None, Some(template)) => {
                        if !cx.template(l, "template", template) {
                            continue;
                        }
                        Guards::Template(template)
                    }
                    (None, None) => continue,
                };
                let pass = match (r.item, r.effect) {
                    (Some(item), _) => {
                        if !cx.template(l, "item", item) {
                            continue;
                        }
                        Pass::Item(item)
                    }
                    (None, Some(effect)) => Pass::Effect(effect),
                    (None, None) => continue,
                };
                t.creatures.safe_passage.push(SafePassage { guards, pass });
            }
            Row::CreatureEffect(r) => {
                let mut ok = true;
                for template in &r.templates {
                    ok &= cx.template(l, "templates", *template);
                }
                if ok {
                    t.creatures
                        .effects
                        .push(yalain_content::creature_rows::CreatureEffect {
                            templates: r.templates.clone(),
                            effect: r.effect,
                            spellcraft: r.spellcraft.clone(),
                        });
                }
            }
            _ => {}
        }
    }
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
        if let Some(key) = &r.fills {
            let &Words::Ref(string, table) = &words else {
                cx.problem(
                    l,
                    "fills: the count fills a variable of the install's text: name the line by `line`",
                );
                continue;
            };
            let variable = match install.variables(string, table).as_deref() {
                Some([one]) => *one,
                Some(other) => {
                    cx.problem(
                        l,
                        format!(
                            "fills: the line {string:08X} of {table:08X} takes {} variables, not one",
                            other.len()
                        ),
                    );
                    continue;
                }
                None => {
                    cx.warn(
                        l,
                        "fills: the install's text is not at hand to name the line's variable, so the line is said without its count",
                    );
                    0
                }
            };
            t.content.tallies.fills.push((
                words.clone(),
                yalain_content::tallies::Fill {
                    tally: key.clone(),
                    variable,
                    full: self::words(r.full_line, r.full_line_text.as_ref()),
                },
            ));
        } else if r.full_line.is_some() || r.full_line_text.is_some() {
            cx.problem(
                l,
                "full_line: a line says another once a tally is full only with fills",
            );
            continue;
        }
        let gates: Vec<yalain_content::tallies::Gate> = r.requires.clone();
        let event_gates: Vec<yalain_content::events::LineGate> = r.events.clone();
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
        if let Some(line) = t.talk.lines.last() {
            for g in gates {
                t.content.tallies.lines.push((line.clone(), g));
            }
            for g in event_gates {
                t.content.events.lines.push((line.clone(), g));
            }
        }
    }
    // A portal a row makes leads to its own place, else where its template's destination names, once
    // every place is known: the place named by the template, else the place of the destination's
    // name. With its own place, any object that is no creature is made a portal.
    for l in &merged.rows {
        let Row::DeathSpawn(r) = &l.row else { continue };
        let props = yalain_content::Props::of(r.template, &entity, &|id| install.property_name(id));
        named_as_a_creature_is(&mut cx, l, "name_text", &r.name, &props);
        let creature = yalain_content::CreatureStats::from_props(&props).is_some();
        match props.text("PortalDestination") {
            // A row's own destination: any object it makes leads there, but no creature.
            None if r.to.is_some() && creature => cx.problem(
                l,
                format!(
                    "to: {:08X} is a creature: only a portal or an object leads anywhere",
                    r.template
                ),
            ),
            _ if r.to.is_some() => {}
            Some(name) => {
                let known = t.places_named.contains_key(&r.template)
                    || t.places_by_name.contains_key(&name.to_ascii_lowercase());
                if !known {
                    cx.warn(
                        l,
                        format!(
                            "template: no place is named by {:08X} or is named {name}, where its portal leads, so none is made",
                            r.template
                        ),
                    );
                }
            }
            None if r.kingdom.is_some() => cx.warn(
                l,
                "kingdom: only a portal it makes keeps players of other kingdoms out",
            ),
            None => {}
        }
    }
    // The steps and gives an order of uses keeping its own count gates, once every order has its
    // row: the order named must keep its own count, and have the uses counted.
    for (l, i, g) in sequence_steps {
        if sequence_gate_holds(&mut cx, &t, l, &g) {
            if let Some(st) = t.quests.steps.get(i) {
                t.items.sequence_steps.push((st.clone(), g));
            }
        }
    }
    for (l, give, g) in sequence_gives {
        if sequence_gate_holds(&mut cx, &t, l, &g) {
            t.items.sequence_gives.push((give, g));
        }
    }
    death_treasures(merged, (&camp_keys, &quest_rows), &mut t, &mut cx);
    subquest_definitions(&mut t.quests, install);
    t.quests.rooms = rooms.into_values().collect();
    t.treasure = treasure_rows(merged);
    cx.warnings.extend(treasure_warnings(merged));
    if cx.problems.is_empty() {
        t.warnings = cx.warnings;
        Ok(t)
    } else {
        cx.problems.sort();
        Err(cx.problems)
    }
}

/// An order of uses as a row gives it, its uses' templates known: its place taken from the world's
/// places, and each state a use names held by its template; none, with problems, otherwise.
fn sequence(
    cx: &mut Ctx<'_>,
    t: &Tables,
    places: &BTreeMap<(String, String), &Located>,
    l: &Located,
    mut q: yalain_content::item_rows::Sequence,
    r: &crate::rows::SequenceRow,
) -> Option<yalain_content::item_rows::Sequence> {
    if let Some(to) = &r.to {
        let p = cx.place(places, l, to)?;
        q.to = Some(StepPlace {
            cell: p.cell,
            origin: p.origin,
            heading: heading_byte(rotation_heading(p.rotation)),
        });
    }
    let mut known = true;
    for (template, state) in &q.uses {
        known &= cx.template(l, "uses", *template);
        let Some(state) = state else {
            continue;
        };
        let held = t
            .items
            .states_of(*template)
            .is_some_and(|st| st.states.contains(state));
        if !held {
            cx.problem(
                l,
                format!(
                    "uses: {template:08X} holds no state {state} (an [[object_states]] row loaded before this one names it)"
                ),
            );
            known = false;
        }
    }
    known.then_some(q)
}

/// The place in the content pack of the placed object a row names (`key@XXYY`, or `pack:key@XXYY`
/// in another pack; a bare `key` in the row's own landblock), or a problem naming the field.
fn placed_object(
    cx: &mut Ctx<'_>,
    objects_at: &BTreeMap<(String, String, u16), usize>,
    l: &Located,
    field: &str,
    name: &str,
) -> Option<usize> {
    let (pack, key) = match name.split_once(':') {
        Some((p, k)) => (p.to_string(), k.to_string()),
        None => (l.pack.clone(), name.to_string()),
    };
    let (key, lb) = match key.split_once('@') {
        Some((k, b)) => (k.to_string(), u16::from_str_radix(b, 16).unwrap_or(0)),
        None => (key, l.landblock.unwrap_or(0)),
    };
    let found = objects_at.get(&(pack, key, lb)).copied();
    if found.is_none() {
        cx.problem(l, format!("{field}: no placed object {name} is served"));
    }
    found
}

/// The places in the content pack of the placed objects a row names, each once; none when one is
/// not served.
fn placed_objects(
    cx: &mut Ctx<'_>,
    objects_at: &BTreeMap<(String, String, u16), usize>,
    l: &Located,
    field: &str,
    names: &[String],
) -> Option<Vec<usize>> {
    let mut found = Vec::new();
    let mut ok = true;
    for name in names {
        match placed_object(cx, objects_at, l, field, name) {
            Some(i) if !found.contains(&i) => found.push(i),
            Some(_) => {
                cx.problem(l, format!("{field}: {name} is named twice"));
                ok = false;
            }
            None => ok = false,
        }
    }
    ok.then_some(found)
}

/// What the world's rows give of the treasure numbers: each kind only where it has rows, its words,
/// potions and families by their order, and the families' pairs and unnamed states from their tiers.
#[must_use]
pub fn treasure_rows(merged: &Merged) -> yalain_content::treasure_tables::TreasureRows {
    let mut t = yalain_content::treasure_tables::TreasureRows::default();
    for l in &merged.rows {
        match &l.row {
            Row::TreasureRules(r) => t.rules = Some(r.rules.clone()),
            Row::TreasureSort(r) => {
                t.sorts.insert(r.sort, r.tables.clone());
            }
            Row::TreasureWord(r) => {
                t.words
                    .entry(r.sort)
                    .or_default()
                    .insert(r.order, r.word.clone());
            }
            Row::TreasureLook(r) => t.looks.push(r.look.clone()),
            Row::TreasurePotion(r) => {
                t.potions.insert(r.order, r.states.clone());
            }
            Row::TreasureFamily(r) => {
                t.families
                    .entry(r.sort)
                    .or_default()
                    .insert(r.order, r.family.clone());
                t.pairs.extend(r.pairs.iter().copied());
                t.unnaming.extend(r.unnamed.iter().copied());
            }
            _ => {}
        }
    }
    t.pairs.sort_unstable();
    t.pairs.dedup();
    t.unnaming.sort_unstable();
    t.unnaming.dedup();
    t
}

/// What the world's treasure rows should say but need not stop for: a band of a sort's counts where
/// none of the world's words for it is weighted (an item of such a band takes the plain word), and
/// a state the server's own families give that the world's families do not (an item that carries
/// it puts nothing on what it hits, and shows it as its own).
#[must_use]
pub fn treasure_warnings(merged: &Merged) -> Vec<Problem> {
    use yalain_content::treasure_tables::{Sort, TreasureTables};
    let rows = treasure_rows(merged);
    let (tables, _) = TreasureTables::with_rows(&rows);
    let first = |kind: &str| {
        merged
            .rows
            .iter()
            .find(|l| l.row.kind() == kind)
            .map_or((String::new(), 0), |l| (l.file.clone(), l.line))
    };
    let mut out = Vec::new();
    if !rows.words.is_empty() {
        for sort in [Sort::Weapon, Sort::Armor, Sort::Shield] {
            let of = tables.sort(sort);
            let empty: Vec<i32> = of
                .counts
                .iter()
                .enumerate()
                .filter(|(i, _)| {
                    !of.words
                        .iter()
                        .any(|w| w.weights.get(*i).is_some_and(|x| *x > 0.0))
                })
                .map(|(_, b)| b.from)
                .collect();
            if empty.is_empty() {
                continue;
            }
            let at = merged.rows.iter().find(|l| match &l.row {
                Row::TreasureWord(r) => r.sort == sort,
                _ => false,
            });
            let (file, line) = at.map_or(("", 0), |l| (l.file.as_str(), l.line));
            out.push(Problem::new(
                file,
                line,
                format!(
                    "[[treasure_word]]: no {} word is weighted in the bands from {empty:?}: their items take the plain word",
                    sort.name()
                ),
            ));
        }
    }
    if !rows.families.is_empty() {
        let theirs: std::collections::BTreeSet<u32> = tables
            .families()
            .flat_map(|f| f.tiers.iter().map(|t| t.state))
            .collect();
        let dropped: Vec<String> = TreasureTables::retail_2017()
            .families()
            .flat_map(|f| f.tiers.iter().map(|t| t.state))
            .filter(|s| !theirs.contains(s))
            .map(|s| format!("{s:#010x}"))
            .collect();
        if !dropped.is_empty() {
            let (file, line) = first("treasure_family");
            let shown: Vec<&str> = dropped.iter().take(8).map(String::as_str).collect();
            out.push(Problem::new(
                &file,
                line,
                format!(
                    "[[treasure_family]]: {} of the server's own enchantments are in none of the world's families ({}{}): an item that carries one keeps it, but it puts nothing on what the item hits and shows as the item's own",
                    dropped.len(),
                    shown.join(", "),
                    if dropped.len() > shown.len() { ", ..." } else { "" }
                ),
            ));
        }
    }
    out
}

/// The death treasures, once every camp has its id in the spawn table: each names what is in the
/// install and a camp that is served, and no creature or chest is given one item by two rows (a
/// camp's row and its creatures' template's among them). One a quest gates names a quest a row
/// defines, or is warned of.
fn death_treasures(
    merged: &Merged,
    (camp_keys, quest_rows): (&BTreeMap<&str, usize>, &std::collections::BTreeSet<u32>),
    t: &mut Tables,
    cx: &mut Ctx<'_>,
) {
    use yalain_content::{DeathTreasure, Dropper};
    let mut rows: Vec<(&Located, DeathTreasure)> = Vec::new();
    for l in &merged.rows {
        let Row::DeathTreasure(r) = &l.row else {
            continue;
        };
        let mut ok = cx.template(l, "item", r.item);
        let from = match (r.template, &r.camp) {
            (Some(template), _) => {
                ok &= cx.template(l, "template", template);
                Dropper::Template(template)
            }
            (None, Some(camp)) => {
                let key = camp.split_once(':').map_or(camp.as_str(), |(_, k)| k);
                let (key, lb) = key
                    .split_once('@')
                    .map_or((key, None), |(k, b)| (k, u16::from_str_radix(b, 16).ok()));
                let Some(lb) = lb.or(l.landblock) else {
                    cx.problem(l, "camp: a camp is named key@XXYY");
                    continue;
                };
                let id = if camp_keys.get(key).copied().unwrap_or(0) > 1 {
                    format!("{key}@{lb:04X}")
                } else {
                    key.to_string()
                };
                let served = merged.rows.iter().any(|c| {
                    matches!(&c.row, Row::Camp(c2) if c2.key == key) && c.landblock == Some(lb)
                }) && t.spawns.camps.iter().any(|c| c.id == id);
                if !served {
                    cx.problem(l, format!("camp: no camp {key}@{lb:04X} is served"));
                    continue;
                }
                Dropper::Camp(id)
            }
            (None, None) => continue,
        };
        if let Some((quest, _)) = r.requires {
            if !quest_rows.contains(&quest) {
                cx.warn(
                    l,
                    format!("requires: quest {quest:08X} has no row, so no player ever holds it"),
                );
            }
        }
        if ok {
            rows.push((
                l,
                DeathTreasure {
                    from,
                    item: r.item,
                    chance: r.chance,
                    requires: r.requires.map(quest_gate),
                    placed: prov_text(&r.prov),
                    confidence: confidence_word(&r.prov),
                },
            ));
        }
    }
    // The templates a camp makes, by its id.
    let makes = |id: &str| -> Vec<u32> {
        t.spawns
            .camps
            .iter()
            .find(|c| c.id == id)
            .map(|c| c.templates.iter().map(|(t, _)| *t).collect())
            .unwrap_or_default()
    };
    let mut twice = Vec::new();
    for (i, (l, a)) in rows.iter().enumerate() {
        for (_, b) in &rows[..i] {
            if a.item != b.item {
                continue;
            }
            let both = match (&a.from, &b.from) {
                (x, y) if x == y => true,
                (Dropper::Camp(c), Dropper::Template(x))
                | (Dropper::Template(x), Dropper::Camp(c)) => makes(c).contains(x),
                _ => false,
            };
            if both {
                twice.push((
                    *l,
                    format!(
                        "item: {:08X} is given by another row to the same creatures or chests",
                        a.item
                    ),
                ));
            }
        }
    }
    for (l, m) in twice {
        cx.problem(l, m);
    }
    t.loot.treasures = rows.into_iter().map(|(_, d)| d).collect();
}

/// The kind of pet a `[[pet]]` makes: what it leaves out, the server's general numbers.
fn pet_kind(r: &crate::rows::Pet) -> yalain_content::PetKind {
    use yalain_content::{PetHire, PetKind, PetTends, PetWhom};
    let mut k = PetKind::new(r.effect, r.template, r.flags, &r.health);
    k.armor.clone_from(&r.armor);
    k.per_level_squared = r.per_level_squared.unwrap_or(k.per_level_squared);
    k.variance = r.variance.unwrap_or(k.variance);
    k.lands = r.lands.unwrap_or(k.lands);
    k.hits = r.swing.unwrap_or(k.hits);
    k.stays = r.stays;
    k.area = r.area.unwrap_or(0.0);
    k.taunts = r.taunts;
    k.ends_on_strike = r.ends_on_strike;
    k.vigor_per_level = r
        .vigor_per_level
        .map(|v| i32::try_from(v).unwrap_or(i32::MAX));
    k.vigor_per_second = r.vigor_per_second;
    k.outdoors_only = r.outdoors_only;
    k.attacks.clone_from(&r.attacks);
    k.made = r.count;
    k.corpse_after = if r.no_corpse {
        None
    } else {
        Some(
            r.corpse_after
                .map_or(k.corpse_after.unwrap_or(0.0), f64::from),
        )
    };
    k.tends = r.tends.map(|t| PetTends {
        whom: if t.pets {
            PetWhom::OwnersPets
        } else {
            PetWhom::Owner
        },
        effect: t.effect,
        every: f64::from(t.every),
        below: t.below,
    });
    k.hire = r.hire.map(|h| PetHire {
        class: h.class,
        seconds: f64::from(h.seconds),
        level: h.level,
    });
    k.item_effects.clone_from(&r.item_effects);
    k
}

/// A row's pool entry as the server draws it: a window left open reaches every level that way.
fn pool_entry(e: &crate::rows::PoolEntry) -> yalain_content::pools::Entry {
    use yalain_content::pools::{Entry, Target, OPEN_ABOVE};
    Entry {
        to: match (&e.pool, e.template) {
            (Some(p), _) => Target::Pool(p.clone()),
            (None, t) => Target::Template(t.unwrap_or(0)),
        },
        weight: e.weight,
        least: e.least.unwrap_or(0),
        most: e.most.unwrap_or(OPEN_ABOVE),
    }
}
