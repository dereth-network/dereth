//! The fields of a row as the editor shows them: every field of an object, a camp, a place, a
//! clearing, a kind of pet and a skill's limit of pets, the kind of value each holds, its text as
//! the row writes it, and what is typed for it as the TOML the row reads.
//!
//! Where a row stands and which way it faces (`at`, `rotation`, `heading`), where it came from
//! (`prov`), a camp's creatures and spots (`spawns`, `spots`), and the fields that name a row have
//! controls of their own: [`ELSEWHERE`] names them.

use toml_edit::{Item, Value};
use yalain_world_db::rows::{Gate, Row};

use crate::toml as rowtoml;

/// The kind of value a field holds, and so how it is changed.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Control {
    /// TOML as the row reads it: a number, a data id, a list or a table.
    Toml,
    /// Words: a key, a reference or a name, typed without quotes.
    Words,
    /// On or off; off is the field left out.
    Flag,
    /// One of these values (each as TOML, and as a person reads it); with `optional`, also none,
    /// the field left out.
    Choice {
        values: &'static [(&'static str, &'static str)],
        optional: bool,
        /// What none means, when it may be none.
        none: &'static str,
    },
    /// The quest a player must hold to use it, and where: `{ quest, done = true }` and the rest.
    Gate,
    /// A string of the install: `{ table, string }`.
    Name,
}

/// A field of a row as the inspector shows it.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Field {
    pub name: &'static str,
    pub control: Control,
    /// The fields it is shown among.
    pub group: &'static str,
    /// What it does, in a few words.
    pub hint: &'static str,
}

const fn field(
    name: &'static str,
    control: Control,
    group: &'static str,
    hint: &'static str,
) -> Field {
    Field {
        name,
        control,
        group,
        hint,
    }
}

const YES_NO: &[(&str, &str)] = &[("true", "yes"), ("false", "no")];
const DIRECTIONS: &[(&str, &str)] = &[("\"forward\"", "forward"), ("\"backward\"", "backward")];

/// The kingdoms a placed object may be for, by the number a character's description names each.
pub const KINGDOMS: &[(&str, &str)] = &[("1", "Shadow"), ("2", "Order"), ("4", "Dominion")];

/// How a camp's creatures may move, over their templates'.
pub const MOVEMENTS: &[(&str, &str)] = &[
    ("\"stationary\"", "stationary"),
    ("\"turn-only\"", "turn only"),
    ("\"constrained-to-home\"", "kept near home"),
    ("\"constrained-to-master\"", "follows a master"),
    ("\"free-range\"", "free range"),
];

/// The groups fields are shown in.
const WHAT: &str = "what it is";
const USE: &str = "who may use it";
const TRAVEL: &str = "where it leads";
const DOOR: &str = "door";
const WALK: &str = "walking";
const MOVES: &str = "how its creatures move";
const WHERE: &str = "where";
const SPAWNING: &str = "spawning";

/// An object's fields, as the inspector shows them.
pub const OBJECT: &[Field] = &[
    field(
        "template",
        Control::Toml,
        WHAT,
        "what it is: a template's data id",
    ),
    field(
        "kind",
        Control::Choice {
            values: &[("\"npc\"", "person"), ("\"static\"", "object")],
            optional: false,
            none: "",
        },
        WHAT,
        "a person the server numbers, or an object with an id of its own",
    ),
    field(
        "name",
        Control::Name,
        WHAT,
        "a name of the install's text, in place of its template's",
    ),
    field(
        "name_text",
        Control::Words,
        WHAT,
        "a name of its own, in place of its template's",
    ),
    field(
        "scale",
        Control::Toml,
        WHAT,
        "how large it stands, in place of its template's",
    ),
    field(
        "requires",
        Control::Gate,
        USE,
        "the quest a player must hold to use it",
    ),
    field(
        "min_level",
        Control::Toml,
        USE,
        "the least level a player must be to use it",
    ),
    field("hero", Control::Flag, USE, "only a hero may use it"),
    field(
        "kingdom",
        Control::Choice {
            values: KINGDOMS,
            optional: true,
            none: "any",
        },
        USE,
        "the kingdom a player must be of to use it",
    ),
    field(
        "to",
        Control::Words,
        TRAVEL,
        "the place using it takes a player to",
    ),
    field(
        "recall",
        Control::Flag,
        TRAVEL,
        "a player who used it may recall to it",
    ),
    field(
        "summon",
        Control::Flag,
        TRAVEL,
        "a player who used it may summon a portal to it",
    ),
    field(
        "landings",
        Control::Toml,
        TRAVEL,
        "a lifestone's recall spots: { cell, at, heading, seen }",
    ),
    field(
        "scene",
        Control::Toml,
        TRAVEL,
        "a door's scene, played on the way in place of portal space",
    ),
    field(
        "locked",
        Control::Flag,
        DOOR,
        "a locked door: only its key opens it from the front",
    ),
    field(
        "lock",
        Control::Toml,
        DOOR,
        "the id a key must carry to open it",
    ),
    field(
        "route",
        Control::Words,
        WALK,
        "the route a person walks: key, or pack:key",
    ),
    field(
        "route_start",
        Control::Toml,
        WALK,
        "the waypoint it sets out toward, from 0",
    ),
    field(
        "route_direction",
        Control::Choice {
            values: DIRECTIONS,
            optional: true,
            none: "forward",
        },
        WALK,
        "which way it goes on along its route",
    ),
    field(
        "event",
        Control::Words,
        WHERE,
        "the event it stands only while it is on",
    ),
    field("id", Control::Toml, WHERE, "an object's world id"),
    field(
        "cell",
        Control::Toml,
        WHERE,
        "the cell it stands in; outdoors it follows from where it stands",
    ),
];

/// A camp's fields, as the inspector shows them.
pub const CAMP: &[Field] = &[
    field(
        "name",
        Control::Name,
        WHAT,
        "the name its creatures are told by, in place of their template's",
    ),
    field(
        "radius",
        Control::Toml,
        SPAWNING,
        "outdoors, how far from its middle a creature appears, meters",
    ),
    field(
        "population",
        Control::Toml,
        SPAWNING,
        "how many it keeps alive",
    ),
    field(
        "regen",
        Control::Toml,
        SPAWNING,
        "seconds from a death to its replacement",
    ),
    field(
        "profile",
        Control::Toml,
        SPAWNING,
        "the generator profile it stands for",
    ),
    field(
        "profile_name",
        Control::Words,
        SPAWNING,
        "that profile's name",
    ),
    field(
        "parent",
        Control::Words,
        SPAWNING,
        "a master camp: key, or pack:key",
    ),
    field(
        "movement",
        Control::Choice {
            values: MOVEMENTS,
            optional: true,
            none: "as their template",
        },
        MOVES,
        "how its creatures may move",
    ),
    field(
        "wanders",
        Control::Choice {
            values: YES_NO,
            optional: true,
            none: "as their template",
        },
        MOVES,
        "whether they wander at rest",
    ),
    field(
        "wander_range",
        Control::Toml,
        MOVES,
        "how far from home they wander, meters",
    ),
    field(
        "wander_chance",
        Control::Toml,
        MOVES,
        "the chance a wander sets out when one is due, 0 to 1",
    ),
    field(
        "homesick",
        Control::Toml,
        MOVES,
        "the distance from home they keep to, meters",
    ),
    field(
        "perception",
        Control::Toml,
        MOVES,
        "how near a player must come to be noticed, meters",
    ),
    field(
        "uses_doors",
        Control::Choice {
            values: YES_NO,
            optional: true,
            none: "as their template",
        },
        MOVES,
        "whether they open doors",
    ),
    field(
        "route",
        Control::Words,
        WALK,
        "the route its creatures walk at rest: key, or pack:key",
    ),
    field(
        "route_start",
        Control::Toml,
        WALK,
        "the waypoint they set out toward, from 0",
    ),
    field(
        "route_direction",
        Control::Choice {
            values: DIRECTIONS,
            optional: true,
            none: "forward",
        },
        WALK,
        "which way they go on along it",
    ),
    field(
        "event",
        Control::Words,
        WHERE,
        "the event it keeps its creatures only while it is on",
    ),
    field(
        "cell",
        Control::Toml,
        WHERE,
        "the cell of its middle; outdoors it follows from where it stands",
    ),
];

/// A place's fields, as the inspector shows them.
pub const PLACE: &[Field] = &[
    field(
        "key",
        Control::Words,
        WHAT,
        "its name; what leads to it follows a new one",
    ),
    field(
        "destination",
        Control::Words,
        WHAT,
        "the name an effect that sends a player to a named place looks it up by",
    ),
    field(
        "named_by",
        Control::Toml,
        WHAT,
        "templates whose destination names it: a portal's, or a surveyor's map's",
    ),
    field(
        "row",
        Control::Words,
        WHAT,
        "the row of like rooms it is one of",
    ),
    field(
        "holds",
        Control::Toml,
        WHAT,
        "the cell its room's creature stands in",
    ),
    field(
        "cell",
        Control::Toml,
        WHERE,
        "the cell it is in; outdoors it follows from where it is",
    ),
];

/// A round clearing's fields.
pub const ROUND_CLEARING: &[Field] = &[field(
    "radius",
    Control::Toml,
    WHAT,
    "how far from its middle nothing grows, meters",
)];

/// A rectangular clearing's fields.
pub const RECTANGLE_CLEARING: &[Field] = &[field(
    "size",
    Control::Toml,
    WHAT,
    "its sides, [x, y] in meters",
)];

const PET_IS: &str = "what it is";
const FIGHT: &str = "how it fights";
const AFTER: &str = "its corpse";

/// A kind of pet's fields, as the Pets window shows them.
pub const PET: &[Field] = &[
    field(
        "template",
        Control::Toml,
        PET_IS,
        "what it makes: a creature's template",
    ),
    field(
        "flags",
        Control::Toml,
        PET_IS,
        "the flags of its pet data as its owner is told them",
    ),
    field(
        "count",
        Control::Toml,
        PET_IS,
        "how many the effect makes at once; 1 when left out",
    ),
    field(
        "stays",
        Control::Flag,
        PET_IS,
        "it stays where it is made and never follows",
    ),
    field(
        "outdoors_only",
        Control::Flag,
        PET_IS,
        "it can't be set down indoors",
    ),
    field(
        "health",
        Control::Toml,
        PET_IS,
        "its greatest health at levels: { level, health }, levels going up",
    ),
    field(
        "armor",
        Control::Toml,
        PET_IS,
        "its armor at levels: { level, armor }, levels going up",
    ),
    field(
        "tends",
        Control::Toml,
        PET_IS,
        "whom it tends: { whom, effect, every, below }",
    ),
    field(
        "hire",
        Control::Toml,
        PET_IS,
        "a contract's terms: { class, seconds, level }",
    ),
    field(
        "item_effects",
        Control::Toml,
        PET_IS,
        "effects an item's use may name that make this pet",
    ),
    field(
        "attacks",
        Control::Toml,
        FIGHT,
        "the attacks it uses: { skill, weight }",
    ),
    field(
        "per_level_squared",
        Control::Toml,
        FIGHT,
        "what a hit takes: this times its level squared",
    ),
    field(
        "variance",
        Control::Toml,
        FIGHT,
        "how far a hit varies either side",
    ),
    field(
        "lands",
        Control::Toml,
        FIGHT,
        "the share of its strikes that land, 0 to 1",
    ),
    field(
        "swing",
        Control::Toml,
        FIGHT,
        "hits a swing on its one target",
    ),
    field(
        "area",
        Control::Toml,
        FIGHT,
        "its missile lands on every creature this near, meters",
    ),
    field("taunts", Control::Flag, FIGHT, "its strikes taunt"),
    field(
        "ends_on_strike",
        Control::Flag,
        FIGHT,
        "it ends as its first strike lands",
    ),
    field(
        "vigor_per_level",
        Control::Toml,
        FIGHT,
        "its greatest vigor for each level",
    ),
    field(
        "vigor_per_second",
        Control::Toml,
        FIGHT,
        "it pays its attacks' vigor, which comes back this much a second",
    ),
    field(
        "corpse_after",
        Control::Toml,
        AFTER,
        "seconds after its death its corpse comes",
    ),
    field("no_corpse", Control::Flag, AFTER, "it leaves no corpse"),
];

/// A summoning skill's limits, as the Pets window shows them.
pub const PET_LIMIT: &[Field] = &[field(
    "limits",
    Control::Toml,
    PET_IS,
    "as many as most of pets sharing a bit of classes: { most, classes }",
)];

/// The fields that have controls of their own, or name their row: where a row stands and which
/// way it faces, where it came from, the key that names it (a place's aside), a camp's creatures
/// and spots, and the effect or skill a kind of pet or a limit is of.
pub const ELSEWHERE: &[&str] = &[
    "key", "at", "rotation", "heading", "prov", "spawns", "spots", "effect", "skill",
];

/// The fields the editor shows for a row, in order.
#[must_use]
pub fn of(row: &Row) -> &'static [Field] {
    match row {
        Row::Object(_) => OBJECT,
        Row::Camp(_) => CAMP,
        Row::Place(_) => PLACE,
        Row::Clearing(c) if c.radius.is_some() => ROUND_CLEARING,
        Row::Clearing(_) => RECTANGLE_CLEARING,
        Row::Pet(_) => PET,
        Row::PetLimit(_) => PET_LIMIT,
        _ => &[],
    }
}

/// The field `name` of a row, as the inspector shows it.
#[must_use]
pub fn find(row: &Row, name: &str) -> Option<&'static Field> {
    of(row).iter().find(|f| f.name == name)
}

/// The row as it writes itself, each field as TOML.
fn table(row: &Row) -> Option<toml_edit::Table> {
    Some(match row {
        Row::Object(o) => rowtoml::object(o, o.to.as_deref(), &o.prov),
        Row::Camp(c) => rowtoml::camp(c, c.parent.as_deref(), &c.prov),
        Row::Place(p) => rowtoml::place(p, &p.prov),
        Row::Clearing(c) => rowtoml::clearing(c, &c.prov),
        Row::Pet(r) => rowtoml::pet(r, &r.prov),
        Row::PetLimit(r) => rowtoml::pet_limit(r, &r.prov),
        _ => return None,
    })
}

/// A field of a row as it is shown to be changed: its TOML, words without their quotes; nothing
/// when the row leaves it out.
#[must_use]
pub fn text(row: &Row, name: &str) -> String {
    let words = find(row, name).is_some_and(|f| f.control == Control::Words);
    let Some(t) = table(row) else {
        return String::new();
    };
    match t.get(name).and_then(Item::as_value) {
        None => String::new(),
        Some(v) if words => v.as_str().map_or_else(|| v_text(v), str::to_string),
        Some(v) => v_text(v),
    }
}

fn v_text(v: &Value) -> String {
    let mut v = v.clone();
    v.decor_mut().clear();
    v.to_string().trim().to_string()
}

/// What is typed for a field, as the TOML its row reads: words need no quotes typed. Nothing
/// typed takes the field away.
#[must_use]
pub fn toml(row: &Row, name: &str, typed: &str) -> String {
    let words = find(row, name).is_some_and(|f| f.control == Control::Words);
    words_toml(words, typed)
}

/// What is typed as TOML: words quoted unless they are already.
#[must_use]
pub fn words_toml(words: bool, typed: &str) -> String {
    let t = typed.trim();
    if words && !t.is_empty() && !t.starts_with('"') {
        format!("\"{}\"", t.replace('\\', "\\\\").replace('"', "\\\""))
    } else {
        t.to_string()
    }
}

/// A gate as its field is written: `{ quest = 0x..., done = true }` and the rest.
#[must_use]
pub fn gate_toml(quest: u32, gate: Gate) -> String {
    let at = match gate {
        Gate::Done => "done = true".to_string(),
        Gate::UnderWay => "underway = true".to_string(),
        Gate::Phase(n) => format!("phase = {n}"),
        Gate::Had => "had = true".to_string(),
    };
    format!("{{ quest = 0x{quest:08X}, {at} }}")
}

/// A string of the install as its field is written.
#[must_use]
pub fn name_toml(table: u32, string: u32) -> String {
    format!("{{ table = 0x{table:08X}, string = 0x{string:08X} }}")
}
