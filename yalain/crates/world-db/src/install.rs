//! The install a compile reads, from its records: templates and their qualities, the property
//! list, the generator profiles' names, store templates' sales (and who each is for) and quests' definitions (saved
//! states, read with the script library's names for their fields, and what the quest's package sets
//! it up to be: done once, for whom), and optionally the language file's text for the tables'
//! reading labels.

use std::cell::{OnceCell, RefCell};
use std::collections::{BTreeMap, HashMap};

use auberean_assets::entity::Entity;
use auberean_assets::script::{Qualities, ScriptState};
use auberean_assets::strings::{EnumMap, StringTable};
use auberean_assets::Context;
use auberean_script::graph::{Body, Graph, PackedObject};
use auberean_script::library::Library;
use auberean_script::native_object::{Item, NativeObject};
use auberean_script::opcode::{length, low_byte, signed, tag_of};
use auberean_script::value::{join, tag, FunctionId};
use yalain_content::stores::SaleGate;
use yalain_content::{FactIds, Props, FACT_NAMES};

use crate::compile::{Install, InstallSale, QuestDefinition};

/// The generator-profile names' enumeration (loot profiles among them).
pub const PROFILE_NAMES: u32 = 0x2300_0100;

/// The quests' names' enumeration.
pub const QUEST_NAMES: u32 = 0x2300_0090;

/// A record reader: a record's bytes by id.
pub type Records<'a> = Box<dyn Fn(u32) -> Option<Vec<u8>> + 'a>;

/// What finds the saved states that define quests, for a caller that has read them all anyway:
/// each with its quest ([`quest_of_state`]), ascending by state. Asked when a row first names no
/// state.
pub type QuestStates<'a> = Box<dyn Fn() -> Vec<(u32, u32)> + 'a>;

/// An install, read from its portal file's records (and its language file's, for labels).
#[allow(missing_debug_implementations)]
pub struct DatInstall<'a> {
    portal: Records<'a>,
    local: Option<Records<'a>>,
    ctx: Context,
    lib: &'a Library,
    profiles: BTreeMap<u32, String>,
    states: Vec<u32>,
    found: Option<QuestStates<'a>>,
    quests: OnceCell<HashMap<u32, u32>>,
    quest_names: OnceCell<HashMap<String, u32>>,
    strings: RefCell<HashMap<u32, Option<StringTable>>>,
    // A compile reads one template's chain many times (describing, examining, checking): each
    // entity and qualities record is decoded once.
    entities: RefCell<HashMap<u32, Option<Entity>>>,
    qualities: RefCell<HashMap<u32, Option<Qualities>>>,
}

impl<'a> DatInstall<'a> {
    /// An install from its portal file's records, its decoding context, its script library (read by
    /// the caller, which may share it), and the ids of its saved states (`0x6F...`, searched for a
    /// quest's definition when a row names no state).
    ///
    /// # Errors
    /// The profile names do not read.
    pub fn new(
        portal: Records<'a>,
        ctx: Context,
        lib: &'a Library,
        states: Vec<u32>,
    ) -> Result<Self, String> {
        let bytes = portal(PROFILE_NAMES).ok_or("the install has no generator profile names")?;
        let names: EnumMap = auberean_assets::decode(PROFILE_NAMES, &bytes, &ctx)
            .map_err(|e| format!("{PROFILE_NAMES:08X}: {e}"))?;
        Ok(Self {
            portal,
            local: None,
            ctx,
            lib,
            profiles: names.names.into_iter().collect(),
            states,
            found: None,
            quests: OnceCell::new(),
            quest_names: OnceCell::new(),
            strings: RefCell::new(HashMap::new()),
            entities: RefCell::new(HashMap::new()),
            qualities: RefCell::new(HashMap::new()),
        })
    }

    /// Take the quests' saved states from `found` rather than searching the states one by one.
    #[must_use]
    pub fn with_quest_states(mut self, found: QuestStates<'a>) -> Self {
        self.found = Some(found);
        self
    }

    /// Read labels from a language file's records.
    #[must_use]
    pub fn with_text(mut self, local: Records<'a>) -> Self {
        self.local = Some(local);
        self
    }

    fn decode<T: auberean_assets::Decode>(&self, id: u32) -> Option<T> {
        let bytes = (self.portal)(id)?;
        auberean_assets::decode(id, &bytes, &self.ctx).ok()
    }

    fn graph(&self, id: u32) -> Option<Graph> {
        let st: ScriptState = self.decode(id)?;
        Graph::decode(&st.graph, self.lib).ok()
    }

    fn text(&self, string: u32, table: u32) -> Option<String> {
        let local = self.local.as_ref()?;
        let mut cache = self.strings.borrow_mut();
        let t = cache.entry(table).or_insert_with(|| {
            let bytes = local(table)?;
            auberean_assets::decode::<StringTable>(table, &bytes, &self.ctx).ok()
        });
        t.as_ref()?
            .entries
            .iter()
            .find(|e| e.token == string)
            .and_then(|e| e.strings.iter().flatten().next().cloned())
    }

    /// A quest's id by its name in the install's enumeration.
    fn quest_named(&self, name: &str) -> Option<u32> {
        self.quest_names
            .get_or_init(|| {
                self.decode::<EnumMap>(QUEST_NAMES)
                    .map_or_else(HashMap::new, |m| {
                        m.names.into_iter().map(|(v, n)| (n, v)).collect()
                    })
            })
            .get(name)
            .copied()
    }

    fn quest_states(&self) -> &HashMap<u32, u32> {
        self.quests.get_or_init(|| {
            let found = self.found.as_ref().map_or_else(
                || {
                    let quest = |&id: &u32| Some((id, quest_of_state(self.lib, &self.graph(id)?)?));
                    self.states.iter().filter_map(quest).collect()
                },
                |found| found(),
            );
            let mut out = HashMap::new();
            for (state, quest) in found {
                out.entry(quest).or_insert(state);
            }
            out
        })
    }
}

/// The quest a saved state defines, when it defines one: the enumeration value of a state with
/// quest phases.
#[must_use]
pub fn quest_of_state(lib: &Library, g: &Graph) -> Option<u32> {
    let nav = Nav::new(lib, g);
    let root = g.root().and_then(|r| nav.object(r))?;
    nav.field(root, "m_questPhaseInfo")?;
    nav.field(root, "mEnum")
}

impl Install for DatInstall<'_> {
    fn entity(&self, id: u32) -> Option<Entity> {
        if let Some(e) = self.entities.borrow().get(&id) {
            return e.clone();
        }
        let e: Option<Entity> = self.decode(id);
        self.entities.borrow_mut().insert(id, e.clone());
        e
    }

    fn qualities(&self, id: u32) -> Option<Qualities> {
        if let Some(q) = self.qualities.borrow().get(&id) {
            return q.clone();
        }
        let q: Option<Qualities> = self.decode(id);
        self.qualities.borrow_mut().insert(id, q.clone());
        q
    }

    fn fact_ids(&self) -> FactIds {
        let id = |name: &str| {
            self.ctx
                .properties()
                .and_then(|l| l.names.names.iter().find(|(_, n)| n == name))
                .map_or(0, |(v, _)| *v)
        };
        let n = FACT_NAMES;
        FactIds {
            name: id(n[0]),
            description: id(n[1]),
            level: id(n[2]),
            max_health: id(n[3]),
            max_vigor: id(n[4]),
            faction: id(n[5]),
            group: id(n[6]),
            min_level: id(n[7]),
        }
    }

    fn property_name(&self, id: u32) -> Option<String> {
        self.ctx
            .properties()
            .and_then(|l| l.names.name(id))
            .map(str::to_string)
    }

    fn profile_name(&self, value: u32) -> Option<String> {
        self.profiles.get(&value).cloned()
    }

    fn store_sales(&self, store: u32) -> Option<Vec<InstallSale>> {
        let g = self.graph(store)?;
        let nav = Nav::new(self.lib, &g);
        let root = nav.object(g.root()?)?;
        let mut out = Vec::new();
        for item in nav.items(nav.field(root, "m_sales")?)? {
            let o = nav.object(item.reference()?)?;
            out.push(InstallSale {
                ordinal: nav.field(o, "m_uiOrdinal")?,
                product: nav.field(o, "m_didProduct")?,
                cost: f32::from_bits(nav.field(o, "m_fCost")?),
                currency: nav.field(o, "m_didTrade")?,
            });
        }
        Some(out)
    }

    fn store_gates(&self, store: u32) -> (Vec<(u32, SaleGate)>, bool) {
        let read = || -> Option<(Vec<(u32, SaleGate)>, bool)> {
            let g = self.graph(store)?;
            let nav = Nav::new(self.lib, &g);
            let root = nav.object(g.root()?)?;
            // The store's first flag: it hides the sales a player may not have.
            let hides = nav.field(root, "m_uiFlags").is_some_and(|f| f & 1 != 0);
            let mut out = Vec::new();
            for item in nav.items(nav.field(root, "m_sales")?)? {
                let o = nav.object(item.reference()?)?;
                let quests = nav
                    .field(o, "m_requiredQuests")
                    .and_then(|h| nav.entries(h))
                    .unwrap_or_default()
                    .into_iter()
                    .filter_map(|(k, v)| match (k, v) {
                        (Item::Word(q), Item::Word(st)) => Some((q, st)),
                        _ => None,
                    })
                    .collect();
                let gate = SaleGate {
                    race: nav.field(o, "m_race").unwrap_or(0),
                    level: nav.field(o, "m_uiLevel").unwrap_or(0),
                    quests,
                };
                if gate != SaleGate::default() {
                    out.push((nav.field(o, "m_uiOrdinal")?, gate));
                }
            }
            Some((out, hides))
        };
        read().unwrap_or_default()
    }

    fn quest_definition(&self, state: Option<u32>, id: u32) -> Option<QuestDefinition> {
        let state = match state {
            Some(s) => s,
            None => *self.quest_states().get(&id)?,
        };
        let g = self.graph(state)?;
        let nav = Nav::new(self.lib, &g);
        let root = nav.object(g.root()?)?;
        if nav.field(root, "mEnum")? != id {
            return None;
        }
        let text = |field: &str| -> Option<(u32, u32)> {
            match nav.native(nav.field(root, field)?)? {
                NativeObject::TextInfo(t) => Some((t.string_id, t.table)),
                _ => None,
            }
        };
        let mut phases = Vec::new();
        let mut subquests = Vec::new();
        let mut ranges = Vec::new();
        for (k, v) in nav.entries(nav.field(root, "m_questPhaseInfo")?)? {
            let Item::Word(phase) = k else { continue };
            let o = nav.object(v.reference()?)?;
            let j = match nav.native(nav.field(o, "m_siJournalEntry")?)? {
                NativeObject::TextInfo(t) => (t.string_id, t.table),
                _ => (0, 0),
            };
            phases.push((phase, j));
            ranges.push((phase, nav.field(o, "m_uiPhaseRange").unwrap_or(1)));
            // The quests the phase is done by, in their order.
            let quests: Vec<u32> = nav
                .field(o, "m_listSubquests")
                .and_then(|f| nav.items(f))
                .unwrap_or_default()
                .into_iter()
                .filter_map(|i| match i {
                    Item::Word(q) => Some(q),
                    _ => None,
                })
                .collect();
            if !quests.is_empty() {
                subquests.push(yalain_content::quests::Subquests {
                    phase,
                    quests,
                    any: nav.field(o, "m_bSubquestAny").unwrap_or(0) != 0,
                });
            }
        }
        phases.sort_by_key(|(p, _)| *p);
        subquests.sort_by_key(|p| p.phase);
        // The effects each kind of update puts on: a list of effect records by the kind.
        let mut effects = Vec::new();
        let by_kind = nav
            .field(root, "m_questUpdateEffects")
            .and_then(|f| nav.entries(f))
            .unwrap_or_default();
        for (k, v) in by_kind {
            let (Item::Word(kind), Some(list)) = (k, v.reference()) else {
                continue;
            };
            for r in nav.items(list).unwrap_or_default() {
                let Some(state) = r
                    .reference()
                    .and_then(|r| nav.object(r))
                    .and_then(|rec| nav.field(rec, "m_effect"))
                    .and_then(|e| nav.state_of(e))
                else {
                    continue;
                };
                effects.push((kind, state));
            }
        }
        effects.sort_by_key(|(k, _)| *k);
        // A last phase of one kill's range ends the quest as it is moved on; one of any other range
        // waits for a use to end it (its journal says so: "Return to ...", "Report ...").
        let last_waits = ranges
            .iter()
            .max_by_key(|(p, _)| *p)
            .is_some_and(|(_, r)| *r != 1);
        let setup = setup_of(self.lib, root.package_id());
        Some(QuestDefinition {
            state,
            name: text("m_siLongName")?,
            description: text("m_siDescription")?,
            icon: nav.field(root, "m_didIcon")?,
            play_fx: nav.field(root, "m_bPlayFXOnUpdate").unwrap_or(0) != 0,
            journals: phases.into_iter().map(|(_, j)| j).collect(),
            effects,
            subquests,
            once: setup.once,
            fellows: setup.fellows,
            last_waits,
            min_level: setup.min_level,
            hero_only: setup.hero_only,
            monarch_only: setup.monarch_only,
            xp: setup.xp,
            pay: setup.pay,
            // The status to be in; one naming no status is to be completed, as an item's is.
            required: setup.required.as_deref().and_then(|name| {
                let status = setup
                    .required_status
                    .unwrap_or(yalain_content::quests::COMPLETED);
                Some((self.quest_named(name)?, status))
            }),
        })
    }

    fn label(&self, template: u32) -> String {
        let props = Props::of(template, &|id| self.entity(id), &|id| {
            self.property_name(id)
        });
        match props.get("Name") {
            Some(yalain_content::Prop::Str(s, t)) => self.text(*s, *t).unwrap_or_default(),
            _ => String::new(),
        }
    }
}

/// The calls a quest's setup makes to say how many times it may be done, and how a kill's move on
/// it reaches the killer's fellows.
const SET_SOLUTIONS: &str = "Quest::SetMaxNumberOfSolutions";
const SET_SHARED: u32 = 0x01A7_000D;
const SET_NEAR_LEVEL: u32 = 0x01A7_008B;
const SET_NEAR_BY: u32 = 0x01A7_0078;

/// The calls a quest's setup makes to say who may be given it: the least level, heroes only, and a
/// quest (by its name) and the status it is to be in.
const SET_LEAST_LEVEL: u32 = 0x01A7_003D;
const SET_HERO_ONLY: u32 = 0x01A7_009A;
const SET_REQUIRED_NAMED: u32 = 0x01A7_0091;
const SET_REQUIRED_STATUS: u32 = 0x01A7_001B;
const SET_MONARCH_ONLY: u32 = 0x01A7_0057;

/// The calls a quest's setup makes to say what its completion pays: an amount, or a share of what
/// the player's level takes to reach the next, with what limits that, and whether the amount is
/// paid whole whatever the quest's challenge; and what it adds to the player's quest rating.
const SET_XP: u32 = 0x01A7_0021;
const SET_XP_SHARE: u32 = 0x01A7_00B4;
const SET_XP_LEAST: u32 = 0x01A7_0099;
const SET_XP_MOST: u32 = 0x01A7_0088;
const SET_XP_MOST_AT_LEVEL: u32 = 0x01A7_006D;
const SET_XP_FLAT: u32 = 0x01A7_0061;
const SET_POINTS: u32 = 0x01A7_0024;

/// What a quest's package sets up beyond its definition.
#[derive(Debug, Clone, Default, PartialEq)]
struct Setup {
    /// Done once: the literal 1 passed to [`SET_SOLUTIONS`]. Any other value (the globals' -1, any
    /// number) or no such call: it may be done again (2017: of the quests whose setup says 1, 0 of
    /// 49 were given again; of the others, 50 were).
    once: bool,
    /// How a kill's move on it reaches the killer's fellows; a setup with no such call shares
    /// nothing, and a call with no limit named limits nothing.
    fellows: yalain_content::quests::Fellows,
    /// The least level a player is given it at (0: any).
    min_level: i32,
    /// Given only to heroes.
    hero_only: bool,
    /// For the monarch of an allegiance alone.
    monarch_only: bool,
    /// What its completion pays: an amount, or how a share works out.
    xp: u64,
    pay: yalain_content::quests::CompletionPay,
    /// The name of a quest a player is to have at a status to be given it.
    required: Option<String>,
    required_status: Option<u32>,
}

/// A quest package's `Setup` (its own, else the nearest of its bases') read for its literals.
fn setup_of(lib: &Library, package: u32) -> Setup {
    let mut at = lib.export_of(package);
    let mut guard = 0;
    while let Some(i) = at {
        let Some(p) = lib.packages.get(i) else {
            return Setup::default();
        };
        if let Some(f) = p.functions.iter().find(|f| f.name == "Setup") {
            let Some(offset) = f.offset else {
                return Setup::default();
            };
            let start = (offset / 4) as usize;
            let end = start + (f.size / 4) as usize;
            return read_setup(lib, start, end.min(lib.code.len()));
        }
        at = p.parent;
        guard += 1;
        if guard > lib.packages.len() {
            return Setup::default();
        }
    }
    Setup::default()
}

/// The setters' literals in the code in `start..end`: each call reads the literal pushed before it.
fn read_setup(lib: &Library, start: usize, end: usize) -> Setup {
    let mut out = Setup::default();
    // The last literal pushed, cleared by anything else that puts a value on the stack.
    let mut literal: Option<i64> = None;
    // Likewise the last string literal.
    let mut text: Option<String> = None;
    // Likewise the last float literal, which a whole number pushed after it leaves alone.
    let mut float: Option<f32> = None;
    let mut at = start;
    while at < end {
        let word = lib.code[at];
        match low_byte(word) {
            // A small literal; a literal in the next word.
            0x03 | 0x83 => literal = Some(i64::from(signed(word))),
            0x02 if tag_of(word) == tag::FLOAT => {
                literal = None;
                float = lib.code.get(at + 1).map(|v| f32::from_bits(*v));
            }
            0x02 => literal = lib.code.get(at + 1).map(|v| i64::from(*v)),
            // A long literal: the high word, then the low.
            0x82 => {
                literal = match (lib.code.get(at + 1), lib.code.get(at + 2)) {
                    (Some(hi), Some(lo)) => Some(join(*lo, *hi).cast_signed()),
                    _ => None,
                };
            }
            // A string literal.
            0x08 => {
                literal = None;
                text = lib
                    .code
                    .get(at + 1)
                    .and_then(|n| lib.literals.get(*n as usize))
                    .map(|units| String::from_utf16_lossy(units));
            }
            // The receiver pushed for the call leaves the argument alone.
            0x23 => {}
            // A call: the one that sets a value reads the literal pushed before it.
            0x0B | 0x4F | 0x51 | 0x53 => {
                let id = lib.code.get(at + 1).copied().unwrap_or(0);
                match id & 0x7FFF_FFFF {
                    SET_SHARED => out.fellows.shared = literal == Some(1),
                    SET_NEAR_LEVEL => out.fellows.near_level = literal == Some(1),
                    SET_NEAR_BY => out.fellows.near_by = literal == Some(1),
                    SET_LEAST_LEVEL => {
                        out.min_level = literal.and_then(|v| i32::try_from(v).ok()).unwrap_or(0);
                    }
                    SET_HERO_ONLY => out.hero_only = literal == Some(1),
                    SET_REQUIRED_NAMED => out.required.clone_from(&text),
                    SET_REQUIRED_STATUS => {
                        out.required_status = literal.and_then(|v| u32::try_from(v).ok());
                    }
                    SET_MONARCH_ONLY => out.monarch_only = literal == Some(1),
                    SET_XP => out.xp = amount(literal),
                    SET_XP_SHARE => out.pay.share = float.unwrap_or(0.0),
                    SET_XP_LEAST => out.pay.least = amount(literal),
                    SET_XP_MOST => out.pay.most = amount(literal),
                    SET_XP_MOST_AT_LEVEL => {
                        out.pay.most_at_level =
                            float.zip(literal.and_then(|v| i32::try_from(v).ok()));
                    }
                    SET_XP_FLAT => out.pay.flat = literal == Some(1),
                    SET_POINTS => {
                        out.pay.points = literal.and_then(|v| u32::try_from(v).ok()).unwrap_or(0);
                    }
                    _ => {
                        if lib.function_name(FunctionId(id)).as_deref() == Some(SET_SOLUTIONS) {
                            out.once = literal == Some(1);
                        }
                    }
                }
                literal = None;
                text = None;
                float = None;
            }
            _ => {
                literal = None;
                text = None;
                float = None;
            }
        }
        at += length(&lib.code, at).unwrap_or(1);
    }
    out
}

/// An amount of experience a literal names; none for no literal or a negative one.
fn amount(literal: Option<i64>) -> u64 {
    literal.and_then(|v| u64::try_from(v).ok()).unwrap_or(0)
}

/// Reading a saved state's graph by the library's names for its fields.
struct Nav<'g> {
    lib: &'g Library,
    by_id: HashMap<u32, &'g PackedObject>,
    singletons: HashMap<u32, u32>,
}

impl<'g> Nav<'g> {
    fn new(lib: &'g Library, g: &'g Graph) -> Self {
        let mut by_id = HashMap::new();
        let mut singletons = HashMap::new();
        for (id, e) in g.ids.iter().zip(&g.entries) {
            match &e.body {
                Body::Object(o) => {
                    by_id.insert(*id, o);
                }
                Body::Singleton(state) => {
                    singletons.insert(*id, *state);
                }
                _ => {}
            }
        }
        Self {
            lib,
            by_id,
            singletons,
        }
    }

    /// The saved state a reference names: a singleton's, or an object's own.
    fn state_of(&self, id: u32) -> Option<u32> {
        self.singletons
            .get(&id)
            .copied()
            .or_else(|| self.object(id).map(|o| o.state))
    }

    fn object(&self, id: u32) -> Option<&'g PackedObject> {
        self.by_id.get(&id).copied()
    }

    fn field(&self, o: &PackedObject, name: &str) -> Option<u32> {
        let pkg = self.lib.package_name(o.package_id());
        let frame = self.lib.frame(&pkg)?;
        let m = frame.members.iter().find(|m| m.name == name)?;
        o.slots.get(usize::try_from(m.slot).ok()?).copied()
    }

    fn native(&self, id: u32) -> Option<&'g NativeObject> {
        self.object(id)?.native.as_ref()
    }

    fn items(&self, id: u32) -> Option<Vec<Item>> {
        match self.native(id)? {
            NativeObject::Sequence { items, .. } => Some(items.clone()),
            _ => None,
        }
    }

    fn entries(&self, id: u32) -> Option<Vec<(Item, Item)>> {
        match self.native(id)? {
            NativeObject::Hash(h) => Some(h.entries().copied().collect()),
            _ => None,
        }
    }
}
