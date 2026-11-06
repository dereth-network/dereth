//! The server's content pack: the objects with behavior the world places, each described from its
//! template in the data files and placed where the recorded sessions saw it (or where an author put
//! it), every row naming where it came from.
//!
//! **Depends on** the record decoders (`auberean-assets`), for describing a template. **Used by**
//! the game server (`yalain-world`), which reads a pack at start, and the server executable
//! (`yalain`), which builds one.
//!
//! A pack is a list of [`ContentObject`]s. Each says *what* the object is, described from its
//! template in the data files ([`describe`]: the entity template's chain and its qualities record
//! give the look, the script package, the effect it enters the world with, its scale and its
//! physics words), and *where* it stands, from a placement (the recorded sessions, or an author).
//! Every row keeps both provenances, so a placement or a description can always be traced back
//! and an authored one told from an evidenced one.
//!
//! The text form ([`ContentPack::to_text`], [`ContentPack::from_text`]) is tab-separated, one
//! object per line, so a pack can be read, compared and edited by hand.
//!
//! **Must never** do I/O: a pack is text in and text out, and a description is made from records
//! the caller decoded.

use std::collections::BTreeMap;
use std::fmt::Write as _;

use auberean_assets::entity::Entity;
use auberean_assets::property::PropertyValue;
use auberean_assets::script::Qualities;

pub mod locks;
pub mod loot;
pub mod movement;
pub mod props;
pub mod quests;
pub mod spawns;
pub mod stores;
pub mod story;
pub mod take_gates;
pub mod talk;
pub mod treasure_effects;
pub mod treasure_looks;
pub mod treasure_words;
pub mod worn_table;

pub use locks::{Lock, LockTable};
pub use loot::{ChestTreasure, LootProfile, LootTable};
pub use movement::{CampMoves, Movement, MovementOverrides, MovementType, Route};
pub use props::{CreatureStats, Prop, Props};
pub use quests::QuestTable;
pub use spawns::{Camp, SpawnTable};
pub use stores::StoreTable;
pub use story::StoryTable;
pub use talk::{TalkTable, Words};

/// A template the world makes objects of while it runs (a creature, a corpse, an item): how its
/// objects look and are described, what examining one shows, and its design values.
#[derive(Debug, Clone, PartialEq)]
pub struct Template {
    pub id: u32,
    pub description: Description,
    pub facts: Facts,
    pub props: Props,
}

/// The craft skills and recipes a new character starts with (their saved states).
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct CraftStart {
    pub skills: Vec<u32>,
    pub recipes: Vec<u32>,
    /// What each butchery profile yields.
    pub butchery: Vec<ButcheryYields>,
}

/// A butchery profile's materials (their templates) and each one's share, and the share of cuts
/// that find nothing.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct ButcheryYields {
    pub profile: u32,
    pub items: Vec<u32>,
    pub weights: Vec<u32>,
    pub empty: u32,
}

/// What the world database says of a weapon creatures wield beyond its template: of the
/// creatures whose template asks two of it, the share given two (the rest one); the stance it is
/// fought with when its template sets none.
#[derive(Debug, Clone, Copy, Default, PartialEq)]
pub struct WeaponRule {
    pub two_share: Option<f32>,
    pub stance: Option<u32>,
}

/// What a creature's death effect makes around its killer, in place of what the install can say.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct DeathSpawn {
    /// The template of what it makes.
    pub template: u32,
    /// How many it tries to make.
    pub count: u32,
    /// The chance that each is made.
    pub chance: f32,
    /// Where each is made, in meters from the killer, as the killer faces.
    pub offset: [f32; 2],
    /// How long each stands, in seconds, where it doesn't stay until killed.
    pub lasts: Option<f32>,
}

/// The templates the world makes objects of, by id, and the names of the generator profiles
/// (loot profiles among them) by value.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct Catalog {
    pub templates: BTreeMap<u32, Template>,
    pub profiles: BTreeMap<u32, String>,
    /// The icons of the item templates whose looks give one: each layer, its picture and color.
    pub icons: BTreeMap<u32, Vec<[u32; 3]>>,
    /// What the appearance tables of those items do to an icon, by table: its base tables first.
    pub icon_tables: BTreeMap<u32, Vec<IconTable>>,
    /// The bodies of the templates whose looks give one: the radius and height of the look's
    /// cylinder (before the object's own scale).
    pub bodies: BTreeMap<u32, [f32; 2]>,
    /// What the templates of a class that says so are wielded as (their implement kinds).
    pub implements: BTreeMap<u32, u32>,
}

impl Catalog {
    #[must_use]
    pub fn get(&self, id: u32) -> Option<&Template> {
        self.templates.get(&id)
    }

    /// Add a template.
    pub fn insert(&mut self, t: Template) {
        self.templates.insert(t.id, t);
    }

    /// The name of a generator profile's value.
    #[must_use]
    pub fn profile_name(&self, value: u32) -> Option<&str> {
        self.profiles.get(&value).map(String::as_str)
    }
}

/// What kind of object a row places.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub enum Kind {
    /// A creature a generator would keep alive (placed where one stood; it does not move yet).
    Creature,
    /// A person of the world: a storekeeper, a quest giver, a guard.
    Npc,
    /// An object of the world with a fixed id of its own: a sign, a portal, a lifestone.
    Static,
}

impl Kind {
    #[must_use]
    pub const fn word(self) -> &'static str {
        match self {
            Self::Creature => "creature",
            Self::Npc => "npc",
            Self::Static => "static",
        }
    }

    fn from_word(s: &str) -> Option<Self> {
        Some(match s {
            "creature" => Self::Creature,
            "npc" => Self::Npc,
            "static" => Self::Static,
            _ => return None,
        })
    }
}

/// What an object is, from its template.
#[derive(Debug, Clone, PartialEq)]
pub struct Description {
    /// The script package its object runs as.
    pub package: u32,
    /// Its look: the visual description it is drawn from.
    pub look: u32,
    /// The effect it shows as it enters the world (0 for none).
    pub enter_fx: u32,
    /// Its scale, when the template sets one.
    pub scale: Option<f32>,
    /// A name the server gives it, as a text reference (string id, table id). The client names
    /// an object from its template when the server gives none; the 2017 servers named some
    /// objects of one template each their own way (a portal by where it leads), so a name comes
    /// with a placement, not from the template.
    pub name: Option<(u32, u32)>,
    /// The name it is shown with, as literal text, in place of its template's.
    pub name_text: Option<String>,
    /// The physics type, the movement ethereality and the placement ethereality, each as its low
    /// and high word.
    pub numbers: [i32; 6],
    /// Which records it was read from.
    pub source: String,
}

/// A place in the world: a cell, a point in its landblock and a rotation (scalar first).
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Place {
    pub cell: u32,
    pub origin: [f32; 3],
    pub rotation: [f32; 4],
}

impl Place {
    /// The place as text: the cell in hex, then the point and the rotation, `:` between.
    #[must_use]
    pub fn to_text(&self) -> String {
        let [x, y, z] = self.origin;
        let [qw, qx, qy, qz] = self.rotation;
        format!("{:08X}:{x}:{y}:{z}:{qw}:{qx}:{qy}:{qz}", self.cell)
    }

    /// Read a place's text.
    ///
    /// # Errors
    /// Not eight parts, or a part that does not read.
    pub fn from_text(s: &str) -> Result<Self, String> {
        let p: Vec<&str> = s.split(':').collect();
        if p.len() != 8 {
            return Err(format!("a place is cell:x:y:z:qw:qx:qy:qz, not {s}"));
        }
        let f = |i: usize| float(p[i]);
        Ok(Self {
            cell: hex32(p[0])?,
            origin: [f(1)?, f(2)?, f(3)?],
            rotation: [f(4)?, f(5)?, f(6)?, f(7)?],
        })
    }
}

/// What examining an object shows of its template: its name and description (string references:
/// string id, table), level, greatest health and vigor, the kingdom it belongs to, whether it is
/// a group monster, the level a player needs to use it; and of a placed portal, what may be
/// recalled to it ([`portal`] flags).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct Facts {
    pub name: Option<(u32, u32)>,
    pub description: Option<(u32, u32)>,
    pub level: Option<i32>,
    pub max_health: Option<i32>,
    pub max_vigor: Option<i32>,
    pub faction: Option<u32>,
    pub group: bool,
    pub min_level: Option<i32>,
    /// A placed portal's flags: whether a player's portal recall returns to where it leads, and
    /// whether a summoned portal opens to it.
    pub portal: u32,
}

/// A portal's flags.
pub mod portal {
    /// A player who used it may recall to where it leads.
    pub const RECALL: u32 = 0x1;
    /// A player who used it may summon a portal to where it leads.
    pub const SUMMON: u32 = 0x2;
}

impl Facts {
    /// The facts as text: `key=value` pairs, `;` between.
    #[must_use]
    pub fn to_text(&self) -> String {
        let mut out = Vec::new();
        let r = |(s, t): (u32, u32)| format!("{s:08X}:{t:08X}");
        if let Some(n) = self.name {
            out.push(format!("name={}", r(n)));
        }
        if let Some(n) = self.description {
            out.push(format!("description={}", r(n)));
        }
        for (k, v) in [
            ("level", self.level),
            ("health", self.max_health),
            ("vigor", self.max_vigor),
            ("minlevel", self.min_level),
        ] {
            if let Some(v) = v {
                out.push(format!("{k}={v}"));
            }
        }
        if let Some(f) = self.faction {
            out.push(format!("faction={f:X}"));
        }
        if self.group {
            out.push("group=1".into());
        }
        if self.portal != 0 {
            out.push(format!("portal={:X}", self.portal));
        }
        out.join(";")
    }

    /// Read the facts' text.
    ///
    /// # Errors
    /// A pair that does not read.
    pub fn from_text(s: &str) -> Result<Self, String> {
        let mut f = Self::default();
        for pair in s.split(';').filter(|p| !p.is_empty()) {
            let (k, v) = pair
                .split_once('=')
                .ok_or_else(|| format!("a fact is key=value, not {pair}"))?;
            let r = |v: &str| -> Result<(u32, u32), String> {
                let (a, b) = v
                    .split_once(':')
                    .ok_or_else(|| format!("a reference is string:table, not {v}"))?;
                Ok((hex32(a)?, hex32(b)?))
            };
            let int = |v: &str| v.parse::<i32>().map_err(|e| format!("{v}: {e}"));
            match k {
                "name" => f.name = Some(r(v)?),
                "description" => f.description = Some(r(v)?),
                "level" => f.level = Some(int(v)?),
                "health" => f.max_health = Some(int(v)?),
                "vigor" => f.max_vigor = Some(int(v)?),
                "minlevel" => f.min_level = Some(int(v)?),
                "faction" => f.faction = Some(hex32(v)?),
                "group" => f.group = v == "1",
                "portal" => f.portal = hex32(v)?,
                other => return Err(format!("an unknown fact {other}")),
            }
        }
        Ok(f)
    }
}

/// The property ids [`facts`] reads, as the data's property list numbers them.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct FactIds {
    pub name: u32,
    pub description: u32,
    pub level: u32,
    pub max_health: u32,
    pub max_vigor: u32,
    pub faction: u32,
    pub group: u32,
    pub min_level: u32,
}

/// The property names [`FactIds`] stands for, in its field order.
pub const FACT_NAMES: [&str; 8] = [
    "Name",
    "Description",
    "GameplayStatistics_Level",
    "GameplayStatistics_MaxHealth",
    "GameplayStatistics_MaxVigor",
    "AI_FactionMembership",
    "NPC_IsGroupMonster",
    "Usage_MinLevel",
];

/// A template's facts: the first value of each property along its chain of entities (a derived
/// template overrides its parent).
#[must_use]
pub fn facts(template: u32, entity: &dyn Fn(u32) -> Option<Entity>, ids: &FactIds) -> Facts {
    let mut f = Facts::default();
    let mut did = template;
    for _ in 0..CHAIN_LIMIT {
        let Some(e) = entity(did) else { break };
        for p in e.properties.iter().flat_map(|c| c.iter()) {
            let int = |v: &PropertyValue| match v {
                PropertyValue::Integer(i) => Some(*i),
                PropertyValue::Enum(e) => Some(e.cast_signed()),
                _ => None,
            };
            let text = |v: &PropertyValue| match v {
                PropertyValue::StringInfo { info, .. } => Some((info.token, info.table.0)),
                _ => None,
            };
            if p.id == ids.name && f.name.is_none() {
                f.name = text(&p.value);
            } else if p.id == ids.description && f.description.is_none() {
                f.description = text(&p.value);
            } else if p.id == ids.level && f.level.is_none() {
                f.level = int(&p.value);
            } else if p.id == ids.max_health && f.max_health.is_none() {
                f.max_health = int(&p.value);
            } else if p.id == ids.max_vigor && f.max_vigor.is_none() {
                f.max_vigor = int(&p.value);
            } else if p.id == ids.faction && f.faction.is_none() {
                f.faction = int(&p.value).map(i32::cast_unsigned);
            } else if p.id == ids.min_level && f.min_level.is_none() {
                f.min_level = int(&p.value);
            } else if p.id == ids.group {
                if let PropertyValue::Bool(b) = p.value {
                    f.group |= b;
                }
            }
        }
        match (e.kind, e.object) {
            (Some(entity_kind::DERIVED), Some(o)) => did = o,
            (Some(entity_kind::CLASS), Some(o)) if o >> 24 == ENTITY_TOP => did = o,
            _ => break,
        }
    }
    f
}

impl Facts {
    /// With the name its properties hold where its chain of entities says none: some item templates
    /// keep theirs only in their qualities record.
    #[must_use]
    pub fn named_by(mut self, props: &Props) -> Self {
        if self.name.is_none() {
            if let Some(Prop::Str(string, table)) = props.get("Name") {
                self.name = Some((*string, *table));
            }
        }
        self
    }
}

/// One object of the pack: what it is and where it stands.
#[derive(Debug, Clone, PartialEq)]
pub struct ContentObject {
    /// The world's id for a static object, which keeps it; `None` for one the server numbers.
    pub object: Option<u64>,
    /// The entity template.
    pub template: u32,
    pub description: Description,
    pub cell: u32,
    /// Where in the cell's landblock.
    pub origin: [f32; 3],
    /// The rotation, scalar first (w, x, y, z).
    pub rotation: [f32; 4],
    pub kind: Kind,
    /// A name to read the pack by (the template's English name, when the builder knew it).
    pub label: String,
    /// Where the placement came from.
    pub placed: String,
    /// Where using it takes a player (a portal's destination), if anywhere.
    pub destination: Option<Place>,
    /// What examining it shows of its template.
    pub facts: Facts,
    /// The route a placed person walks, the waypoint it sets out toward and which way it goes on
    /// (its settings unused).
    pub walks: Option<crate::movement::CampMoves>,
    /// The quest a player must hold, and where, to use it (a portal its placed instance keeps
    /// closed), over its template's.
    pub gate: Option<QuestGate>,
}

/// A quest a use needs: the quest, and the status it must be at, written as a template's
/// `Usage_RequiredQuestStatus` is (completed `0x30000000`; under way `0x10000000`, with the phase
/// in the low bits when one is needed).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct QuestGate {
    pub quest: u32,
    pub status: u32,
}

impl QuestGate {
    /// The gate as text: the quest and the status in hex, `:` between.
    #[must_use]
    pub fn to_text(&self) -> String {
        format!("{:08X}:{:08X}", self.quest, self.status)
    }

    /// Read a gate's text.
    ///
    /// # Errors
    /// Not two parts, or a part that does not read.
    pub fn from_text(s: &str) -> Result<Self, String> {
        let (q, st) = s
            .split_once(':')
            .ok_or_else(|| format!("{s}: a gate is quest:status"))?;
        Ok(Self {
            quest: hex32(q)?,
            status: hex32(st)?,
        })
    }
}

impl ContentObject {
    /// The landblock it stands in.
    #[must_use]
    pub const fn landblock(&self) -> u16 {
        (self.cell >> 16) as u16
    }
}

/// The world's objects, and the kind of player conflict its landblocks allow.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct ContentPack {
    pub objects: Vec<ContentObject>,
    /// The landblocks that allow player conflict of some kind; any other is a peace area. Not part
    /// of the text form: the world database's areas fill it.
    pub conflict: BTreeMap<u16, Conflict>,
}

/// The kind of player conflict a place allows.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum Conflict {
    /// No player may fight another.
    #[default]
    Peace,
    /// Players of different kingdoms may fight, neutral players stay out of it.
    Controlled,
    /// Players of different kingdoms may fight.
    KingdomVsKingdom,
    /// Any player may fight any other.
    FreeForAll,
}

impl Conflict {
    /// The status the client is told of (1 to 4).
    #[must_use]
    pub const fn status(self) -> u32 {
        match self {
            Self::Peace => 1,
            Self::Controlled => 2,
            Self::KingdomVsKingdom => 3,
            Self::FreeForAll => 4,
        }
    }

    /// Its word, as the world database writes it.
    #[must_use]
    pub const fn word(self) -> &'static str {
        match self {
            Self::Peace => "peace",
            Self::Controlled => "controlled",
            Self::KingdomVsKingdom => "kingdom-vs-kingdom",
            Self::FreeForAll => "free-for-all",
        }
    }

    /// The kind a word names.
    #[must_use]
    pub fn from_word(w: &str) -> Option<Self> {
        [
            Self::Peace,
            Self::Controlled,
            Self::KingdomVsKingdom,
            Self::FreeForAll,
        ]
        .into_iter()
        .find(|c| c.word() == w)
    }
}

/// Property ids of the entity templates (the property list's own numbers).
pub mod property {
    /// The physical object an entity is drawn as (an entity whose object is a visual description).
    pub const LOOK: u32 = 0x4100_020B;
    /// The entity's scale.
    pub const SCALE: u32 = 0x4100_0219;
    /// The effect an object shows as it enters the world.
    pub const ENTER_FX: u32 = 0x4100_0290;
}

/// Entity kinds along a template's chain.
pub mod entity_kind {
    /// A drawable entity: its object is a visual description.
    pub const DRAWABLE: u32 = 0x4000_0001;
    /// A class template: its object is the script package.
    pub const CLASS: u32 = 0x4000_0002;
    /// A template derived from another: its object is the parent template.
    pub const DERIVED: u32 = 0x4000_0007;
}

/// The top byte of an entity template's id, and of a visual description's.
pub const ENTITY_TOP: u32 = 0x47;
pub const VISUAL_DESC_TOP: u32 = 0x1F;

/// The qualities' data id naming an object's body: the drawable entity it is drawn as.
pub const BODY: u32 = 1;

/// A template's qualities record: the entity id with `0x81` in its top byte.
#[must_use]
pub const fn qualities_id(template: u32) -> u32 {
    0x8100_0000 | (template & 0x00FF_FFFF)
}

const CHAIN_LIMIT: usize = 16;

/// Describe a template from the data files: walk its chain of entities from the template to its
/// class (the first value of each property found wins, so a derived template overrides its
/// parent; a class may lead on to another class), take the look from the drawable entity the
/// chain names (or, when it names none, the body the template's qualities record names), the
/// package from the last class, and the physics words from the qualities record.
///
/// # Errors
/// The template, an entity of its chain or its look is missing, or the chain never reaches a
/// class.
pub fn describe(
    template: u32,
    entity: &dyn Fn(u32) -> Option<Entity>,
    qualities: &dyn Fn(u32) -> Option<Qualities>,
) -> Result<Description, String> {
    let mut did = template;
    let mut look_entity = None;
    let mut enter_fx = None;
    let mut scale = None;
    let mut package = None;
    for _ in 0..CHAIN_LIMIT {
        let e = entity(did).ok_or_else(|| format!("entity {did:08X} is missing"))?;
        for p in e.properties.iter().flat_map(|c| c.iter()) {
            match (p.id, &p.value) {
                (property::LOOK, PropertyValue::DataFile(d)) => {
                    look_entity.get_or_insert(d.0);
                }
                (property::ENTER_FX, PropertyValue::Enum(v)) => {
                    enter_fx.get_or_insert(*v);
                }
                (property::SCALE, PropertyValue::Float(v)) => {
                    scale.get_or_insert(*v);
                }
                _ => {}
            }
        }
        match e.kind {
            Some(entity_kind::DERIVED) => {
                did = e
                    .object
                    .ok_or_else(|| format!("entity {did:08X} derives from nothing"))?;
            }
            // A class may name another class entity rather than its package.
            Some(entity_kind::CLASS) => match e.object {
                Some(o) if o >> 24 == ENTITY_TOP => did = o,
                o => {
                    package = o;
                    break;
                }
            },
            other => {
                return Err(format!(
                    "entity {did:08X} of kind {other:?} ends the chain of {template:08X}"
                ))
            }
        }
    }
    let package = package.ok_or_else(|| format!("the chain of {template:08X} reaches no class"))?;
    let q = qualities(qualities_id(template));
    // A template whose chain names no look (a class template placed as it is) is drawn as the
    // body its qualities record names.
    let body = q.as_ref().and_then(|q| {
        q.data_ids
            .iter()
            .find(|(k, _)| *k == BODY)
            .map(|(_, d)| d.0)
    });
    let look_did = look_entity
        .or(body)
        .ok_or_else(|| format!("{template:08X} names no look"))?;
    // The body may be a visual description itself rather than an entity drawn as one.
    let look = if look_did >> 24 == VISUAL_DESC_TOP {
        look_did
    } else {
        entity(look_did).and_then(|e| e.object).ok_or_else(|| {
            format!("the look entity {look_did:08X} of {template:08X} has no visual description")
        })?
    };
    let d = q.as_ref().and_then(|q| q.description.as_ref());
    let word = |v: Option<i32>| v.unwrap_or(0);
    let numbers = d.map_or([0; 6], |d| {
        [
            word(d.physics_type_low),
            word(d.physics_type_high),
            word(d.movement_ethereal_low),
            word(d.movement_ethereal_high),
            word(d.placement_ethereal_low),
            word(d.placement_ethereal_high),
        ]
    });
    let mut source = format!("dat entity {template:08X}, look {look_did:08X}");
    if q.is_some() {
        let _ = write!(source, ", qualities {:08X}", qualities_id(template));
    }
    Ok(Description {
        name_text: None,
        package,
        look,
        enter_fx: enter_fx.unwrap_or(0),
        scale,
        name: None,
        numbers,
        source,
    })
}

/// The pack's columns, in order.
pub const COLUMNS: [&str; NCOLS] = [
    "object",
    "template",
    "cell",
    "x",
    "y",
    "z",
    "qw",
    "qx",
    "qy",
    "qz",
    "kind",
    "label",
    "package",
    "look",
    "enter_fx",
    "scale",
    "name",
    "numbers",
    "placed",
    "described",
    "destination",
    "facts",
    "route",
    "gate",
    "name_text",
];

const NCOLS: usize = 25;

fn hex32(s: &str) -> Result<u32, String> {
    u32::from_str_radix(s, 16).map_err(|e| format!("{s}: {e}"))
}

fn float(s: &str) -> Result<f32, String> {
    s.parse::<f32>().map_err(|e| format!("{s}: {e}"))
}

impl ContentPack {
    /// The pack as text: a header line, then one tab-separated line per object.
    #[must_use]
    pub fn to_text(&self) -> String {
        let mut out = COLUMNS[..NCOLS].join("\t");
        out.push('\n');
        for o in &self.objects {
            let d = &o.description;
            let _ = writeln!(
                out,
                "{}\t{:08X}\t{:08X}\t{}\t{}\t{}\t{}\t{}\t{}\t{}\t{}\t{}\t{:X}\t{:08X}\t{:08X}\t{}\t{}\t{}\t{}\t{}\t{}\t{}\t{}\t{}\t{}",
                o.object.map(|i| format!("{i:016X}")).unwrap_or_default(),
                o.template,
                o.cell,
                o.origin[0],
                o.origin[1],
                o.origin[2],
                o.rotation[0],
                o.rotation[1],
                o.rotation[2],
                o.rotation[3],
                o.kind.word(),
                o.label.replace(['\t', '\n'], " "),
                d.package,
                d.look,
                d.enter_fx,
                d.scale.map(|s| s.to_string()).unwrap_or_default(),
                d.name.map(|(s, t)| format!("{s:08X}:{t:08X}")).unwrap_or_default(),
                d.numbers.iter().map(|n| format!("{n:X}")).collect::<Vec<_>>().join(","),
                o.placed.replace(['\t', '\n'], " "),
                d.source.replace(['\t', '\n'], " "),
                o.destination.map(|p| p.to_text()).unwrap_or_default(),
                o.facts.to_text(),
                o.walks.as_ref().map(crate::movement::CampMoves::route_text).unwrap_or_default(),
                o.gate.map(|g| g.to_text()).unwrap_or_default(),
                d.name_text.as_deref().unwrap_or_default().replace(['\t', '\n'], " "),
            );
        }
        out
    }

    /// Read a pack's text.
    ///
    /// # Errors
    /// A line without its columns, or a value that does not read.
    pub fn from_text(text: &str) -> Result<Self, String> {
        let mut objects = Vec::new();
        for (n, line) in text.lines().enumerate().skip(1) {
            if line.trim().is_empty() {
                continue;
            }
            let c: Vec<&str> = line.split('\t').collect();
            // A pack written before destinations, facts, routes, gates and names had fewer columns.
            if !(NCOLS - 5..=NCOLS).contains(&c.len()) {
                return Err(format!("line {}: {} columns, not {NCOLS}", n + 1, c.len()));
            }
            let at = |e: String| format!("line {}: {e}", n + 1);
            let numbers: Vec<i32> = c[17]
                .split(',')
                .map(|s| {
                    u32::from_str_radix(s, 16)
                        .map(u32::cast_signed)
                        .map_err(|e| format!("{s}: {e}"))
                })
                .collect::<Result<_, _>>()
                .map_err(at)?;
            let numbers: [i32; 6] = numbers
                .try_into()
                .map_err(|_| at("six physics words".into()))?;
            let name = if c[16].is_empty() {
                None
            } else {
                let (s, t) = c[16]
                    .split_once(':')
                    .ok_or_else(|| at("a name is string:table".into()))?;
                Some((hex32(s).map_err(at)?, hex32(t).map_err(at)?))
            };
            objects.push(ContentObject {
                object: if c[0].is_empty() {
                    None
                } else {
                    Some(u64::from_str_radix(c[0], 16).map_err(|e| at(e.to_string()))?)
                },
                template: hex32(c[1]).map_err(at)?,
                cell: hex32(c[2]).map_err(at)?,
                origin: [
                    float(c[3]).map_err(at)?,
                    float(c[4]).map_err(at)?,
                    float(c[5]).map_err(at)?,
                ],
                rotation: [
                    float(c[6]).map_err(at)?,
                    float(c[7]).map_err(at)?,
                    float(c[8]).map_err(at)?,
                    float(c[9]).map_err(at)?,
                ],
                kind: Kind::from_word(c[10]).ok_or_else(|| at(format!("kind {}", c[10])))?,
                label: c[11].to_string(),
                description: Description {
                    name_text: c.get(24).filter(|n| !n.is_empty()).map(|n| (*n).to_owned()),
                    package: hex32(c[12]).map_err(at)?,
                    look: hex32(c[13]).map_err(at)?,
                    enter_fx: hex32(c[14]).map_err(at)?,
                    scale: if c[15].is_empty() {
                        None
                    } else {
                        Some(float(c[15]).map_err(at)?)
                    },
                    name,
                    numbers,
                    source: c[19].to_string(),
                },
                placed: c[18].to_string(),
                destination: match c.get(20) {
                    Some(t) if !t.is_empty() => Some(Place::from_text(t).map_err(at)?),
                    _ => None,
                },
                facts: match c.get(21) {
                    Some(t) => Facts::from_text(t).map_err(at)?,
                    None => Facts::default(),
                },
                walks: match c.get(22) {
                    Some(t) if !t.is_empty() => {
                        Some(crate::movement::CampMoves::from_text("", t).map_err(at)?)
                    }
                    _ => None,
                },
                gate: match c.get(23) {
                    Some(t) if !t.is_empty() => Some(QuestGate::from_text(t).map_err(at)?),
                    _ => None,
                },
            });
        }
        Ok(Self {
            objects,
            conflict: BTreeMap::new(),
        })
    }

    /// The objects standing within `radius` landblocks of a landblock (a square), in pack order.
    pub fn near(
        &self,
        landblock: u16,
        radius: u8,
    ) -> impl Iterator<Item = (usize, &ContentObject)> + '_ {
        let (x, y) = (i32::from(landblock >> 8), i32::from(landblock & 0xFF));
        let r = i32::from(radius);
        self.objects.iter().enumerate().filter(move |(_, o)| {
            let lb = o.landblock();
            ((i32::from(lb >> 8)) - x).abs() <= r && ((i32::from(lb & 0xFF)) - y).abs() <= r
        })
    }
}

/// The icon a look gives: the icon layers of its visual description and of the descriptions it
/// extends, a layer of the nearer description taking the place of the same layer further up; in
/// layer order, each as its layer, picture and color.
#[must_use]
pub fn icon_layers(
    look: u32,
    visual: &dyn Fn(u32) -> Option<auberean_assets::visual::VisualDesc>,
) -> Vec<[u32; 3]> {
    let mut layers = BTreeMap::new();
    let mut did = look;
    for _ in 0..CHAIN_LIMIT {
        let Some(vd) = (did != 0).then(|| visual(did)).flatten() else {
            break;
        };
        for l in vd.icon.iter().flatten() {
            layers
                .entry(l.layer)
                .or_insert([l.layer, l.picture.0, l.shift_color.0]);
        }
        match vd.parent {
            Some(parent) => did = parent.0,
            None => break,
        }
    }
    layers.into_values().collect()
}

/// What one appearance table does to an icon: per key, its steps in order, each with the value it
/// starts at and the layers it changes.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct IconTable {
    pub keys: Vec<(u32, Vec<IconStep>)>,
}

/// One step of an appearance key: the value it starts at, and the layers it changes (each its
/// layer, its picture and color, 0 for what it leaves alone).
pub type IconStep = (f32, Vec<[u32; 3]>);

/// What an appearance table and its base tables do to an icon, the base tables first. Those that
/// change no icon are left out.
#[must_use]
pub fn icon_tables(
    table: u32,
    appearance: &dyn Fn(u32) -> Option<auberean_assets::visual::Appearance>,
) -> Vec<IconTable> {
    fn gather(
        table: u32,
        appearance: &dyn Fn(u32) -> Option<auberean_assets::visual::Appearance>,
        out: &mut Vec<IconTable>,
        depth: usize,
    ) {
        let Some(t) = appearance(table) else {
            return;
        };
        if depth < CHAIN_LIMIT {
            for base in &t.base {
                gather(*base, appearance, out, depth + 1);
            }
        }
        let keys: Vec<(u32, Vec<IconStep>)> = t
            .keys
            .iter()
            .filter(|(_, sets)| sets.iter().any(|s| !s.icon_layers.is_empty()))
            .map(|(key, sets)| {
                let steps = sets
                    .iter()
                    .map(|s| {
                        let layers = s
                            .icon_layers
                            .iter()
                            .map(|l| [l.layer, l.picture.0, l.shift_color.0])
                            .collect();
                        (s.intensity, layers)
                    })
                    .collect();
                (*key, steps)
            })
            .collect();
        if !keys.is_empty() {
            out.push(IconTable { keys });
        }
    }
    let mut out = Vec::new();
    gather(table, appearance, &mut out, 0);
    out
}

/// An icon takes what an appearance key gives it at a value: in each table, the last step whose
/// value is not above it. A layer of the same number takes its picture and its color, when it has
/// them; else it goes in by its number.
pub fn apply_icon_value(icon: &mut Vec<[u32; 3]>, tables: &[IconTable], key: u32, value: f32) {
    for t in tables {
        let Some((_, steps)) = t.keys.iter().find(|(k, _)| *k == key) else {
            continue;
        };
        let picked = steps.iter().take_while(|(from, _)| value >= *from).last();
        for l in picked.iter().flat_map(|(_, layers)| layers) {
            if let Some(x) = icon.iter_mut().find(|x| x[0] == l[0]) {
                if l[1] != 0 {
                    x[1] = l[1];
                }
                if l[2] != 0 {
                    x[2] = l[2];
                }
            } else {
                let at = icon.iter().position(|x| x[0] > l[0]).unwrap_or(icon.len());
                icon.insert(at, *l);
            }
        }
    }
}

/// The root part groups of looks: a creature's or a player's, and an object's.
pub const ROOT_GROUPS: [u32; 2] = [0x8100_0001, 0x4000_0001];

/// The body a look gives: its root part group's setup's radius and height (`setup` reads them),
/// times the first scale up its chain of visual descriptions (its height's).
#[must_use]
pub fn look_body(
    look: u32,
    visual: &dyn Fn(u32) -> Option<auberean_assets::visual::VisualDesc>,
    setup: &dyn Fn(u32) -> Option<(f32, f32)>,
) -> Option<[f32; 2]> {
    let (root, z) = look_setup(look, visual)?;
    let (r, h) = setup(root)?;
    Some([r * z, h * z])
}

/// The setup a look's body is made of (its root part group's) and the first scale up its chain of
/// visual descriptions (its height's; 1 when none sets one).
#[must_use]
pub fn look_setup(
    look: u32,
    visual: &dyn Fn(u32) -> Option<auberean_assets::visual::VisualDesc>,
) -> Option<(u32, f32)> {
    let mut did = look;
    let mut scale = None;
    let mut root = None;
    for _ in 0..CHAIN_LIMIT {
        let Some(vd) = (did != 0).then(|| visual(did)).flatten() else {
            break;
        };
        if scale.is_none() {
            scale = vd.scale.map(|s| s.z);
        }
        if root.is_none() {
            root = vd
                .part_groups
                .iter()
                .flatten()
                .find(|(k, g)| ROOT_GROUPS.contains(k) && g.setup.is_some())
                .and_then(|(_, g)| g.setup);
        }
        match vd.parent {
            Some(parent) => did = parent.0,
            None => break,
        }
    }
    Some((root?.0, scale.unwrap_or(1.0)))
}
