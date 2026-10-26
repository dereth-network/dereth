//! The spec's plugin conformance cases, run on this client's driver ([`crate::plugins`]).
//!
//! A case is a folder under `docs/plugins/conformance/<profile>/`: a test plugin (`plugin.toml`,
//! Lua files) and an `expect.toml` saying what it proves, the scenario it runs in, what happens to
//! it, and what it must produce:
//!
//! ```toml
//! says = "A handler hears the login, then each experience gain, in order."
//! [scenario]                 # absent: no login, frames only
//! session = "long-solo-play" # a recording under fixtures/packet-captures, or
//! world = "scripted"         # a scripted world: a character and the [[object]]s below
//! character = "+Aldric"      # optional; the first character otherwise
//! seconds = 120              # seconds to run after entering the world
//! [[object]]                 # a scripted world's objects
//! id = 0x80000010
//! name = "Corpse of a Drudge"
//! type = "container"         # an item-type word, or its bits as a number
//! corpse = true              # optional: the object is a corpse
//! player = true              # optional: a creature that is another player's character
//! container = "player"       # optional: "player" or an object id; absent is on the ground
//! value = 100                # optional
//! magical = true             # optional
//! ints = { 105 = 9 }         # optional: what an appraisal tells
//! spells = [2074]            # optional: what an appraisal tells
//! worn = true                # optional: the scripted character wears or wields it, in the slot
//!                            #   `locations` names (its equip mask)
//! spell_id = 1679            # optional: the spell it casts when used (a wand's, a food's)
//! useability = 0x000A0004    # optional: how it is used, and on what (the game's use flags)
//! target_type = "creature"   # optional: the item types it may be used on
//! place = { ns = 42.3, ew = 33.9 } # optional: where it stands
//! max_stack = 100            # optional: the most one stack of it holds
//! material = 64              # optional: its material; workmanship = 6.0 and structure = 40
//!                            #   (its uses, a salvage bag's units) likewise
//! vendor = { buys = "misc" } # optional: a vendor, used it shows the objects in it, sells them
//!                            #   at their value and buys what its item types cover
//! [[wall]]                   # a scripted world's walls: each stops the character, blocks a line
//! from = [42.2, 33.8]        #   of sight across it, and walking routes go round it; on the ground
//! to = [42.21, 33.8]         #   from one place to the other, each [ns, ew]
//! pk = true                  # optional: the scripted character is a player killer
//! attack_refusals = [0x1D]   # optional: the scripted server refuses the next attacks, one each
//! repeat_attacks = true      # optional: each swing ends with the attack sequence going on
//! [scenario.pregame]         # optional, with world = "scripted": the character list comes
//! characters = ["A", "B"]    #   first, and the world is entered by logging on
//! still_in_world = ["A"]     #   the first entry of each is refused as still in the world
//! [net]                      # optional: HTTPS answers, URL = a file in the case's folder
//! "https://example.org/a.txt" = "a.txt"
//! [host]                     # optional
//! store_quota = 64           # bytes per store scope
//! policy = { vendor = "deny" } # the world's policy, per automation domain
//! policy_document = "..."    # or the world's published policy document
//! [[step]]
//! after = 30                 # seconds after entering the world (or after loading)
//! say = "/xp"                # or click = "window.node", or set = "window.node" with value = …;
//!                            # "dereth.plugins/panel.node" names the host's own window; or
//!                            # answer = "yes" or "no" (the host's question), with
//!                            # pick = "a.met" (the line picked); or
//!                            # teleport = [ns, ew] (a second in portal space), or die = true,
//!                            # or player_moves = true (a movement key), or
//!                            # server_takes_control = true, or unload = "plugin.id", or
//!                            # bar = "plugin.id" (its button on the plugin bar clicked), or
//!                            # resize = "window" with width = …, height = … (cells, lines),
//!                            # or map_click = { ns, ew, button, ctrl, shift, alt } (a press
//!                            # on the game's map; button "left" and no modifier when absent),
//!                            # or select = 0x80000050 (an object selected, as a click on it)
//! [expect]                   # every key optional; each is compared exactly
//! status = "running"
//! console = ["login\t+Brann"]
//! printed = ["XP this session: 0"]
//! hidden = ["a line"]
//! sent = ["appraise 0x80000011"] # the messages the plugin's actions sent
//! bar = ["test.x\tpaused\topen"] # the plugin bar: id, state, and whether its window is open
//! moves = ["face 90", "autorun on"] # what the character's body and the session were asked
//! overlays = """world ..."""     # the markers, as the host lists them
//! place = [42.3, 33.9]       # where the character ended, to 0.01
//! ui = """window ..."""
//! layout = """window ..."""  # each open window laid out by a host with no display
//! ```

use std::path::{Path, PathBuf};

use dereth_plugin_api::{AutoDomain, DomainRule, Value};

use crate::plugins::{read_package, PluginRun, ScriptedObject, ScriptedPregame};

/// Where the conformance cases are, relative to the workspace root.
pub const CASES_DIR: &str = "docs/plugins/conformance";

/// The cases about the time budget of an entry itself, as `<profile>/<case>`: a handler that never
/// returns, stopped by its budget until the plugin is suspended for it. They run at the host's own
/// budget, timed as an entry runs; every other case runs at
/// [`crate::plugins::UNHURRIED_BUDGET_MS`].
pub const BUDGET_CASES: &[&str] = &["core/budget"];

/// Whether `case` is one of [`BUDGET_CASES`].
fn is_budget_case(case: &Case) -> bool {
    debug_assert!(
        BUDGET_CASES
            .iter()
            .all(|c| workspace_root().join(CASES_DIR).join(c).is_dir()),
        "every budget case is a case's folder: {BUDGET_CASES:?}"
    );
    let profile = case
        .dir
        .parent()
        .and_then(Path::file_name)
        .map(|p| p.to_string_lossy().into_owned())
        .unwrap_or_default();
    BUDGET_CASES.contains(&format!("{profile}/{}", case.name).as_str())
}

/// The workspace root, from this crate's manifest.
#[must_use]
pub fn workspace_root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../..")
}

/// What a step does.
#[derive(Debug, Clone, PartialEq)]
pub enum Action {
    Say(String),
    Click(String),
    Set(String, Value),
    /// A teleport of the scripted character to map coordinates (north-south, east-west).
    Teleport(f64, f64),
    /// The scripted character dies.
    Die,
    /// The player works a movement key.
    PlayerMoves,
    /// The server takes control of the character's movement.
    ServerTakesControl,
    /// The player unloads a plugin.
    Unload(String),
    /// The player clicks a plugin's button on the plugin bar.
    Bar(String),
    /// The player resizes a window to cells and lines.
    Resize(String, i64, i64),
    /// A scripted object walks to map coordinates (north-south, east-west).
    MoveObject(u32, f64, f64),
    /// The player presses a plugin's hotkey, by its node's id.
    Hotkey(String),
    /// The player presses the game's map.
    MapClick(dereth_client_contract::panels::map::MapClick),
    /// An object is selected, as a click on it in the world selects it.
    Select(u32),
    /// The player answers the host's question: yes or no, the line named picked.
    Answer {
        yes: bool,
        pick: Option<String>,
    },
    /// Someone speaks to the character: `kind` is `tell` (a tell to it), `say` (speech near it)
    /// or `emote`, from `from` (an object id, `id`), the words `text`.
    Hear {
        kind: String,
        from: String,
        id: u32,
        text: String,
    },
    /// The player chooses rows of a list (from 1), or a tab of a `tabs` node.
    Choose(String, Vec<i64>),
    /// Objects come into the scripted world.
    Spawn(Vec<ScriptedObject>),
    /// An object leaves the scripted world.
    Remove(u32),
}

/// One timed step.
#[derive(Debug, Clone, PartialEq)]
pub struct Step {
    pub after: f64,
    pub action: Action,
}

/// One case, read from its folder.
#[derive(Debug, Clone)]
pub struct Case {
    pub name: String,
    pub dir: PathBuf,
    pub says: String,
    pub scenario: Option<(String, Option<String>, f64)>,
    /// A scripted world: the character's name, the objects and the seconds to run.
    pub scripted: Option<(String, Vec<ScriptedObject>, f64)>,
    /// The `[host]` table.
    pub host: Value,
    /// A scripted pre-game: the character list comes up first.
    pub pregame: Option<ScriptedPregame>,
    /// HTTPS answers: URL and the file's bytes.
    pub net: Vec<(String, Vec<u8>)>,
    /// A scripted world's walls, each from one `(ns, ew)` to another.
    pub walls: Vec<((f64, f64), (f64, f64))>,
    /// A scripted world's portals and recalls, for journeys.
    pub portals: Vec<crate::scripted_travel::ScriptedPortal>,
    pub recalls: Vec<crate::scripted_travel::ScriptedRecall>,
    pub steps: Vec<Step>,
    pub expect: Value,
}

/// A place a case names: `[ns, ew]` outdoors, or `{ cell, x, y, z }`.
fn case_place(v: &Value, path: &Path) -> dereth_primitives::Position {
    match v.as_list() {
        Some(l) if l.len() == 2 => dereth_plugin_host::places::from_coords(
            l[0].as_f64().unwrap_or_default(),
            l[1].as_f64().unwrap_or_default(),
            0.0,
        ),
        _ => dereth_plugin_host::places::from_value(v).ok(),
    }
    .unwrap_or_else(|| panic!("{}: {v:?} is not a place", path.display()))
}

fn toml_value(v: &toml::Value) -> Value {
    dereth_plugin_host::package::toml_to_value(v)
}

/// Read one case.
///
/// # Panics
/// Panics when `expect.toml` is missing or malformed: the cases are committed with the spec.
#[must_use]
pub fn read_case(dir: &Path) -> Case {
    let path = dir.join("expect.toml");
    let text = std::fs::read_to_string(&path).unwrap_or_else(|e| panic!("{}: {e}", path.display()));
    let table: toml::Table =
        toml::from_str(&text).unwrap_or_else(|e| panic!("{}: {e}", path.display()));
    let doc = toml_value(&toml::Value::Table(table));
    let s = doc.get("scenario");
    let is_scripted = s.get("world").as_str() == Some("scripted");
    let scenario = (s.as_map().is_some() && !is_scripted).then(|| {
        (
            s.get("session")
                .as_str()
                .unwrap_or_else(|| panic!("{}: scenario.session", path.display()))
                .to_owned(),
            s.get("character").as_str().map(str::to_owned),
            s.get("seconds").as_f64().unwrap_or(0.0),
        )
    });
    let scripted = is_scripted.then(|| {
        let objects: Vec<ScriptedObject> = doc
            .get("object")
            .as_list()
            .unwrap_or_default()
            .iter()
            .map(|o| scripted_object(o, &path))
            .collect();
        (
            s.get("character").as_str().unwrap_or("Tester").to_owned(),
            objects,
            s.get("seconds").as_f64().unwrap_or(0.0),
        )
    });
    let portals = doc
        .get("portal")
        .as_list()
        .unwrap_or_default()
        .iter()
        .map(|p| crate::scripted_travel::ScriptedPortal {
            name: p.get("name").as_str().unwrap_or("Portal").to_owned(),
            class: p
                .get("class")
                .as_i64()
                .and_then(|c| u32::try_from(c).ok())
                .unwrap_or(1),
            at: case_place(p.get("at"), &path),
            to: case_place(p.get("to"), &path),
            source: match p.get("source").as_str().unwrap_or("hint") {
                "learned" => dereth_navigation::EdgeSource::Learned,
                "published" => dereth_navigation::EdgeSource::Published,
                _ => dereth_navigation::EdgeSource::Hint,
            },
            refuses: p.get("refuses").as_bool().unwrap_or(false),
        })
        .collect();
    let pair = |v: &Value| -> (f64, f64) {
        let l = v.as_list().unwrap_or_default();
        let n = |i: usize| l.get(i).and_then(Value::as_f64);
        n(0).zip(n(1))
            .unwrap_or_else(|| panic!("{}: a wall's end is [ns, ew]", path.display()))
    };
    let walls = doc
        .get("wall")
        .as_list()
        .unwrap_or_default()
        .iter()
        .map(|w| (pair(w.get("from")), pair(w.get("to"))))
        .collect();
    let recalls = doc
        .get("recall")
        .as_list()
        .unwrap_or_default()
        .iter()
        .map(|r| crate::scripted_travel::ScriptedRecall {
            recall: match r.get("recall").as_str() {
                Some("marketplace") => dereth_navigation::Recall::Marketplace,
                _ => dereth_navigation::Recall::Lifestone,
            },
            to: case_place(r.get("to"), &path),
        })
        .collect();
    assert!(
        doc.get("grants").is_nil(),
        "{}: a case grants nothing: every plugin may do what each permission covers",
        path.display()
    );
    let mut steps: Vec<Step> = doc
        .get("step")
        .as_list()
        .unwrap_or_default()
        .iter()
        .map(|s| {
            let after = s.get("after").as_f64().unwrap_or(0.0);
            let action = if let Some(line) = s.get("say").as_str() {
                Action::Say(line.to_owned())
            } else if let Some(node) = s.get("click").as_str() {
                Action::Click(node.to_owned())
            } else if let Some(node) = s.get("set").as_str() {
                Action::Set(node.to_owned(), s.get("value").clone())
            } else if let Some(to) = s.get("teleport").as_list() {
                let n = |i: usize| to.get(i).and_then(Value::as_f64).unwrap_or_default();
                Action::Teleport(n(0), n(1))
            } else if s.get("die").as_bool() == Some(true) {
                Action::Die
            } else if s.get("player_moves").as_bool() == Some(true) {
                Action::PlayerMoves
            } else if s.get("server_takes_control").as_bool() == Some(true) {
                Action::ServerTakesControl
            } else if let Some(id) = s.get("unload").as_str() {
                Action::Unload(id.to_owned())
            } else if let Some(id) = s.get("bar").as_str() {
                Action::Bar(id.to_owned())
            } else if let Some(node) = s.get("hotkey").as_str() {
                Action::Hotkey(node.to_owned())
            } else if let Some(answer) = s.get("answer").as_str() {
                Action::Answer {
                    yes: answer == "yes",
                    pick: s.get("pick").as_str().map(str::to_owned),
                }
            } else if let Some(id) = s.get("select").as_i64() {
                Action::Select(u32::try_from(id).unwrap_or(0))
            } else if let Some(text) = s.get("hear").get("text").as_str() {
                let h = s.get("hear");
                Action::Hear {
                    kind: h.get("kind").as_str().unwrap_or("tell").to_owned(),
                    from: h.get("from").as_str().unwrap_or("Someone").to_owned(),
                    id: h
                        .get("id")
                        .as_i64()
                        .and_then(|i| u32::try_from(i).ok())
                        .unwrap_or(0x5000_0100),
                    text: text.to_owned(),
                }
            } else if let Some(node) = s.get("choose").as_str() {
                Action::Choose(
                    node.to_owned(),
                    s.get("rows")
                        .as_list()
                        .unwrap_or_default()
                        .iter()
                        .filter_map(Value::as_i64)
                        .collect(),
                )
            } else if let Some(list) = s.get("spawn").as_list() {
                Action::Spawn(list.iter().map(|o| scripted_object(o, &path)).collect())
            } else if let Some(id) = s.get("remove").as_i64() {
                Action::Remove(u32::try_from(id).unwrap_or(0))
            } else if let Some(id) = s.get("move_object").get("id").as_i64() {
                let m = s.get("move_object");
                Action::MoveObject(
                    u32::try_from(id).unwrap_or(0),
                    m.get("ns").as_f64().unwrap_or_default(),
                    m.get("ew").as_f64().unwrap_or_default(),
                )
            } else if let Some(w) = s.get("resize").as_str() {
                Action::Resize(
                    w.to_owned(),
                    s.get("width").as_i64().unwrap_or(0),
                    s.get("height").as_i64().unwrap_or(0),
                )
            } else if s.get("map_click").as_map().is_some() {
                use dereth_client_contract::panels::map::{MapButton, MapClick};
                let m = s.get("map_click");
                let held = |k: &str| m.get(k).as_bool() == Some(true);
                Action::MapClick(MapClick {
                    ns: m.get("ns").as_f64().unwrap_or_default(),
                    ew: m.get("ew").as_f64().unwrap_or_default(),
                    button: match m.get("button").as_str().unwrap_or("left") {
                        "left" => MapButton::Left,
                        "right" => MapButton::Right,
                        "middle" => MapButton::Middle,
                        other => panic!("{}: no map button {other:?}", path.display()),
                    },
                    ctrl: held("ctrl"),
                    shift: held("shift"),
                    alt: held("alt"),
                })
            } else {
                panic!(
                    "{}: a step says, clicks, sets, answers, teleports, dies, moves the \
                     character, hands it to the server, unloads a plugin, presses the map or \
                     selects an object",
                    path.display()
                )
            };
            Step { after, action }
        })
        .collect();
    steps.sort_by(|a, b| a.after.total_cmp(&b.after));
    let names = |v: &Value| -> Vec<String> {
        v.as_list()
            .unwrap_or_default()
            .iter()
            .filter_map(|n| n.as_str().map(str::to_owned))
            .collect()
    };
    let pregame = s.get("pregame").as_map().map(|_| {
        let p = s.get("pregame");
        ScriptedPregame {
            characters: names(p.get("characters"))
                .into_iter()
                .zip(0x5000_0001_u32..)
                .map(|(n, id)| (id, n))
                .collect(),
            still_in_world: names(p.get("still_in_world")),
            objects: scripted.as_ref().map(|s| s.1.clone()).unwrap_or_default(),
        }
    });
    let net = doc
        .get("net")
        .as_map()
        .into_iter()
        .flatten()
        .map(|(url, file)| {
            let f = dir.join(file.as_str().unwrap_or_default());
            let bytes = std::fs::read(&f).unwrap_or_else(|e| panic!("{}: {e}", f.display()));
            (url.clone(), bytes)
        })
        .collect();
    Case {
        name: dir
            .file_name()
            .map(|n| n.to_string_lossy().into_owned())
            .unwrap_or_default(),
        dir: dir.to_path_buf(),
        says: doc.get("says").as_str().unwrap_or_default().to_owned(),
        scenario,
        scripted,
        host: doc.get("host").clone(),
        pregame,
        net,
        walls,
        portals,
        recalls,
        steps,
        expect: doc.get("expect").clone(),
    }
}

/// The item-type bits a case names by word.
fn item_type(v: &Value, path: &Path) -> u32 {
    use dereth_rules::weenie::item_type as t;
    if let Some(n) = v.as_i64() {
        return u32::try_from(n).unwrap_or(0);
    }
    match v.as_str().unwrap_or("misc") {
        "melee_weapon" => t::MELEE_WEAPON,
        "missile_weapon" => t::MISSILE_WEAPON,
        "caster" => t::CASTER,
        "armor" => t::ARMOR,
        "clothing" => t::CLOTHING,
        "jewelry" => t::JEWELRY,
        "gem" => t::GEM,
        "food" => t::FOOD,
        "money" => t::MONEY,
        "misc" => t::MISC,
        "container" => t::CONTAINER,
        "creature" => t::CREATURE,
        "salvage" => t::TINKERING_MATERIAL,
        "spell_component" => t::SPELL_COMPONENTS,
        "salvaging_tool" => t::TINKERING_TOOL,
        other => panic!("{}: no item type {other:?}", path.display()),
    }
}

/// A table keyed by property number, each value read by `f`.
fn keyed<T>(v: &Value, f: impl Fn(&Value) -> Option<T>) -> Vec<(u32, T)> {
    v.as_map()
        .into_iter()
        .flatten()
        .filter_map(|(k, v)| Some((k.parse().ok()?, f(v)?)))
        .collect()
}

fn scripted_object(o: &Value, path: &Path) -> ScriptedObject {
    let int = |k: &str| o.get(k).as_i64();
    let id = int("id")
        .and_then(|i| u32::try_from(i).ok())
        .unwrap_or_else(|| panic!("{}: an object needs an id", path.display()));
    let mut bitfield = 0;
    if o.get("corpse").as_bool() == Some(true) {
        use dereth_rules::weenie::bitfield as b;
        bitfield |= b::CORPSE | b::OPENABLE | b::STUCK;
    }
    let obj_type = item_type(o.get("type"), path);
    if o.get("player").as_bool() == Some(true) {
        // Another player's character.
        bitfield |= dereth_rules::weenie::bitfield::PLAYER;
    } else if obj_type & dereth_rules::weenie::item_type::CREATURE != 0
        && o.get("vendor") == &Value::Nil
    {
        // A creature a case names is a monster: one the player can attack.
        bitfield |= dereth_rules::weenie::bitfield::ATTACKABLE;
    }
    let container = match o.get("container") {
        Value::Str(s) if s == "player" => Some(crate::plugins::SCRIPTED_PLAYER),
        v => v.as_i64().and_then(|i| u32::try_from(i).ok()),
    };
    ScriptedObject {
        id,
        name: o.get("name").as_str().unwrap_or("Thing").to_owned(),
        obj_type,
        bitfield,
        container,
        value: int("value")
            .and_then(|v| u32::try_from(v).ok())
            .unwrap_or(0),
        stack: int("stack").and_then(|v| u16::try_from(v).ok()),
        effects: (o.get("magical").as_bool() == Some(true)).then_some(1),
        ints: o
            .get("ints")
            .as_map()
            .into_iter()
            .flatten()
            .filter_map(|(k, v)| Some((k.parse().ok()?, i32::try_from(v.as_i64()?).ok()?)))
            .collect(),
        spells: o
            .get("spells")
            .as_list()
            .unwrap_or_default()
            .iter()
            .filter_map(|v| v.as_i64().and_then(|i| u32::try_from(i).ok()))
            .collect(),
        int64s: keyed(o.get("int64s"), Value::as_i64),
        floats: keyed(o.get("floats"), Value::as_f64),
        strings: keyed(o.get("strings"), |v| v.as_str().map(str::to_owned)),
        bools: keyed(o.get("bools"), Value::as_bool),
        dids: keyed(o.get("dids"), |v| {
            v.as_i64().and_then(|i| u32::try_from(i).ok())
        }),
        health: o
            .get("health")
            .as_list()
            .and_then(|l| Some((l.first()?.as_i64()?, l.get(1)?.as_i64()?)))
            .and_then(|(a, b)| Some((u32::try_from(a).ok()?, u32::try_from(b).ok()?))),
        locations: int("locations").and_then(|v| u32::try_from(v).ok()),
        worn: o.get("worn").as_bool() == Some(true),
        door: match o.get("door").as_str() {
            Some("locked") => Some(crate::plugins::ScriptedDoor { locked: true }),
            Some(_) => Some(crate::plugins::ScriptedDoor { locked: false }),
            None => None,
        },
        place: o
            .get("place")
            .get("ns")
            .as_f64()
            .zip(o.get("place").get("ew").as_f64()),
        max_stack: int("max_stack").and_then(|v| u16::try_from(v).ok()),
        material: int("material").and_then(|v| u32::try_from(v).ok()),
        #[allow(clippy::cast_possible_truncation)]
        workmanship: o.get("workmanship").as_f64().map(|w| w as f32),
        structure: int("structure").and_then(|v| u16::try_from(v).ok()),
        spell_id: int("spell_id").and_then(|v| u16::try_from(v).ok()),
        useability: int("useability").and_then(|v| u32::try_from(v).ok()),
        target_type: match o.get("target_type") {
            Value::Nil => None,
            v => Some(item_type(v, path)),
        },
        vendor: match o.get("vendor") {
            Value::Nil => None,
            v => Some(crate::plugins::ScriptedVendor {
                buys: item_type(v.get("buys"), path),
            }),
        },
        remade_as: None,
    }
}

/// Every case of a profile, in name order.
#[must_use]
pub fn cases(profile: &str) -> Vec<Case> {
    let dir = workspace_root().join(CASES_DIR).join(profile);
    let mut dirs: Vec<PathBuf> = std::fs::read_dir(&dir)
        .unwrap_or_else(|e| panic!("{}: {e}", dir.display()))
        .filter_map(Result::ok)
        .map(|e| e.path())
        .filter(|p| p.join("expect.toml").is_file())
        .collect();
    dirs.sort();
    dirs.iter().map(|d| read_case(d)).collect()
}

/// One case of a profile, by folder name.
///
/// # Panics
/// Panics when the profile has no such case.
#[must_use]
pub fn case(profile: &str, name: &str) -> Case {
    read_case(&workspace_root().join(CASES_DIR).join(profile).join(name))
}

/// What a case produced.
#[derive(Debug, Clone, Default)]
pub struct Outcome {
    pub status: String,
    pub console: Vec<String>,
    pub printed: Vec<String>,
    pub hidden: Vec<String>,
    pub ui: String,
    /// Each open window laid out by a host with no display.
    pub layout: String,
    pub sent: Vec<String>,
    pub moves: Vec<String>,
    pub overlays: String,
    /// Where the character ended: map coordinates.
    pub place: Option<(f64, f64)>,
    /// The plugin bar: `id\tstate\topen|closed`.
    pub bar: Vec<String>,
}

/// Whether a case's scripted character is a player killer (`[scenario] pk = true`).
fn case_is_pk(case: &Case) -> bool {
    let path = case.dir.join("expect.toml");
    std::fs::read_to_string(&path)
        .ok()
        .and_then(|t| toml::from_str::<toml::Table>(&t).ok())
        .and_then(|t| t.get("scenario")?.get("pk")?.as_bool())
        .unwrap_or(false)
}

/// Whether a case's `[scenario]` sets `key` to true.
fn case_flag(case: &Case, key: &str) -> bool {
    std::fs::read_to_string(case.dir.join("expect.toml"))
        .ok()
        .and_then(|t| toml::from_str::<toml::Table>(&t).ok())
        .and_then(|t| t.get("scenario")?.get(key)?.as_bool())
        .unwrap_or(false)
}

/// The codes a case's scripted server refuses its first attacks with
/// (`[scenario] attack_refusals`).
fn case_attack_refusals(case: &Case) -> Vec<u32> {
    let path = case.dir.join("expect.toml");
    std::fs::read_to_string(&path)
        .ok()
        .and_then(|t| toml::from_str::<toml::Table>(&t).ok())
        .and_then(|t| {
            let list = t
                .get("scenario")?
                .get("attack_refusals")?
                .as_array()?
                .clone();
            Some(
                list.iter()
                    .filter_map(|v| u32::try_from(v.as_integer()?).ok())
                    .collect(),
            )
        })
        .unwrap_or_default()
}

/// Run one case.
#[must_use]
pub fn run(case: &Case) -> Outcome {
    let package = read_package(&case.dir);
    let id = package
        .manifest()
        .map(|m| m.id.to_string())
        .unwrap_or_default();
    let host = &case.host;
    let pk = case_is_pk(case);
    let budget = is_budget_case(case);
    let mut run = PluginRun::with_config(|c| {
        if budget {
            c.limits.call_budget_ms = dereth_plugin_api::engine::Limits::default().call_budget_ms;
        }
        if let Some(q) = host.get("store_quota").as_i64() {
            c.store_quota = usize::try_from(q).unwrap_or(0);
        }
        for (d, r) in host.get("policy").as_map().into_iter().flatten() {
            let d = AutoDomain::parse(d).unwrap_or_else(|| panic!("no automation domain {d}"));
            let r = r
                .as_str()
                .and_then(DomainRule::parse)
                .unwrap_or_else(|| panic!("no policy rule {r:?}"));
            c.policy.automation.insert(d, r);
        }
    });
    if pk {
        run = run.with_player_killer();
    }
    run = run.with_attack_refusals(case_attack_refusals(case));
    if case_flag(case, "repeat_attacks") {
        run = run.with_repeating_attacks();
    }
    // A data folder of the plugin's own, kept in memory for the run.
    if host.get("data_folder").as_bool() == Some(true) {
        run = run.with_files(Box::new(dereth_plugin_host::MemoryFiles::default()));
    }
    if !case.net.is_empty() {
        let mut net = dereth_plugin_host::TableNet::default();
        for (url, body) in &case.net {
            net.answers.insert(
                url.clone(),
                dereth_plugin_host::HttpResponse {
                    status: 200,
                    content_type: None,
                    body: body.clone(),
                },
            );
        }
        run = run.with_net(Box::new(net));
    }
    if let Some(doc) = host.get("policy_document").as_str() {
        let p = dereth_plugin_host::policy_doc::parse(doc)
            .unwrap_or_else(|e| panic!("{}: policy_document: {e}", case.name));
        run.host().set_world_policy(p);
    }
    // The plugin is loaded before the world, so that it hears the login.
    run.load(package);
    match (&case.pregame, &case.scripted) {
        (Some(p), _) => run.scripted_pregame(p.clone()),
        (None, Some((character, objects, _))) => {
            run.scripted_portals(&case.portals, case.recalls.clone());
            for (from, to) in &case.walls {
                run.wall(*from, *to);
            }
            run.scripted_world(character, objects.clone());
        }
        (None, None) => {}
    }
    let total = match (&case.scenario, &case.scripted) {
        (Some((session, character, seconds)), _) => {
            run.login(session, character.as_deref());
            *seconds
        }
        (None, Some((_, _, seconds))) => *seconds,
        (None, None) => case.steps.last().map_or(0.0, |s| s.after),
    };
    for step in &case.steps {
        let wait = step.after - run.since_entry();
        if wait > 0.0 {
            run.advance(wait);
        }
        // `plugin/window.node` names another owner's window (the host's own, `dereth.plugins`).
        let owner = |node: &str| -> (String, String) {
            match node.split_once('/') {
                Some((p, n)) => (p.to_owned(), n.to_owned()),
                None => (id.clone(), node.to_owned()),
            }
        };
        match &step.action {
            Action::Say(line) => run.type_line(line),
            Action::Click(node) => {
                let (p, n) = owner(node);
                run.click(&p, &n);
            }
            Action::Set(node, value) => {
                let (p, n) = owner(node);
                run.set(&p, &n, value.clone());
            }
            Action::Teleport(ns, ew) => run.teleport(*ns, *ew),
            Action::Die => run.die(),
            Action::PlayerMoves => run.player_moves(),
            Action::ServerTakesControl => run.server_takes_control(),
            Action::Unload(id) => run.unload(id),
            Action::Bar(id) => run.bar_click(id),
            Action::Resize(w, width, height) => run.resize(&id, w, *width, *height),
            Action::MoveObject(object, ns, ew) => run.move_object(*object, *ns, *ew),
            Action::Hotkey(node) => run.press_hotkey(&id, node),
            Action::MapClick(click) => run.map_click(click),
            Action::Select(object) => run.select(*object),
            Action::Answer { yes, pick } => run.answer(*yes, pick.as_deref()),
            Action::Hear {
                kind,
                from,
                id: from_id,
                text,
            } => run.hear(kind, from, *from_id, text),
            Action::Choose(node, rows) => {
                let (p, n) = owner(node);
                run.choose(&p, &n, rows.clone());
            }
            Action::Spawn(objects) => {
                run.spawn(objects.clone());
                run.frame();
            }
            Action::Remove(object) => {
                run.remove(*object);
                run.frame();
            }
        }
    }
    let wait = total - run.since_entry();
    if wait > 0.0 {
        run.advance(wait);
    }
    Outcome {
        status: run.status(&id),
        console: run.console(&id),
        printed: run.printed(),
        hidden: run.hidden(),
        ui: run.ui_dump(),
        layout: run.layout_dump(),
        sent: run.sent(),
        moves: run.moves(),
        overlays: run.overlays(),
        place: run.player_place().map(|(ns, ew, _)| (ns, ew)),
        bar: run.bar(),
    }
}

/// Compare what a case produced with its expectations; answers each difference.
#[must_use]
pub fn check(case: &Case, out: &Outcome) -> Vec<String> {
    let mut diffs = Vec::new();
    let e = &case.expect;
    let lines = |key: &str| -> Option<Vec<String>> {
        e.get(key).as_list().map(|l| {
            l.iter()
                .map(|v| v.as_str().unwrap_or_default().to_owned())
                .collect()
        })
    };
    if let Some(s) = e.get("status").as_str() {
        if out.status != s {
            diffs.push(format!(
                "status: expected {s:?}, got {:?} (console: {:?})",
                out.status, out.console
            ));
        }
    }
    for (key, got) in [
        ("console", &out.console),
        ("printed", &out.printed),
        ("hidden", &out.hidden),
        ("sent", &out.sent),
        ("moves", &out.moves),
        ("bar", &out.bar),
    ] {
        if let Some(want) = lines(key) {
            if &want != got {
                diffs.push(format!("{key}:\n  expected {want:?}\n  got      {got:?}"));
            }
        }
    }
    if let Some(want) = e.get("overlays").as_str() {
        if want.trim_end() != out.overlays.trim_end() {
            diffs.push(format!(
                "overlays:\n--- expected\n{want}\n--- got\n{}",
                out.overlays
            ));
        }
    }
    if let Some(want) = e.get("place").as_list() {
        let want: Vec<f64> = want.iter().filter_map(Value::as_f64).collect();
        let near = out.place.is_some_and(|(ns, ew)| {
            want.len() == 2 && (ns - want[0]).abs() < 0.01 && (ew - want[1]).abs() < 0.01
        });
        if !near {
            diffs.push(format!("place: expected {want:?}, got {:?}", out.place));
        }
    }
    if let Some(want) = e.get("ui").as_str() {
        if want.trim_end() != out.ui.trim_end() {
            diffs.push(format!("ui:\n--- expected\n{want}\n--- got\n{}", out.ui));
        }
    }
    if let Some(want) = e.get("layout").as_str() {
        if want.trim_end() != out.layout.trim_end() {
            diffs.push(format!(
                "layout:\n--- expected\n{want}\n--- got\n{}",
                out.layout
            ));
        }
    }
    diffs
}

/// Run a case and fail with its differences.
///
/// # Panics
/// Panics when the case's output differs from its expectations.
pub fn assert_case(profile: &str, name: &str) -> Outcome {
    let c = case(profile, name);
    let out = run(&c);
    let diffs = check(&c, &out);
    assert!(
        diffs.is_empty(),
        "conformance case {profile}/{name} ({}):\n{}",
        c.says,
        diffs.join("\n")
    );
    out
}
