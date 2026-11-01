//! The install a compile reads, from its records: templates and their qualities, the property
//! list, the generator profiles' names, store templates' sales and quests' definitions (saved
//! states, read with the script library's names for their fields, and whether the quest's package
//! sets it up to be done once), and optionally the language file's text for the tables' reading
//! labels.

use std::cell::{OnceCell, RefCell};
use std::collections::{BTreeMap, HashMap};

use auberean_assets::entity::Entity;
use auberean_assets::script::{Qualities, ScriptState};
use auberean_assets::strings::{EnumMap, StringTable};
use auberean_assets::Context;
use auberean_script::graph::{Body, Graph, PackedObject};
use auberean_script::library::{Library, RECORD};
use auberean_script::native_object::{Item, NativeObject};
use auberean_script::opcode::{length, low_byte, signed};
use auberean_script::value::FunctionId;
use yalain_content::{FactIds, Props, FACT_NAMES};

use crate::compile::{Install, InstallSale, QuestDefinition};

/// The generator-profile names' enumeration (loot profiles among them).
pub const PROFILE_NAMES: u32 = 0x2300_0100;

/// A record reader: a record's bytes by id.
pub type Records<'a> = Box<dyn Fn(u32) -> Option<Vec<u8>> + 'a>;

/// An install, read from its portal file's records (and its language file's, for labels).
#[allow(missing_debug_implementations)]
pub struct DatInstall<'a> {
    portal: Records<'a>,
    local: Option<Records<'a>>,
    ctx: Context,
    lib: Library,
    profiles: BTreeMap<u32, String>,
    states: Vec<u32>,
    quests: OnceCell<HashMap<u32, u32>>,
    strings: RefCell<HashMap<u32, Option<StringTable>>>,
    // A compile reads one template's chain many times (describing, examining, checking): each
    // entity and qualities record is decoded once.
    entities: RefCell<HashMap<u32, Option<Entity>>>,
    qualities: RefCell<HashMap<u32, Option<Qualities>>>,
}

impl<'a> DatInstall<'a> {
    /// An install from its portal file's records, its decoding context, and the ids of its saved
    /// states (`0x6F...`, searched for a quest's definition when a row names no state).
    ///
    /// # Errors
    /// The script library or the profile names do not read.
    pub fn new(portal: Records<'a>, ctx: Context, states: Vec<u32>) -> Result<Self, String> {
        let lib = Library::from_record(&portal(RECORD).ok_or("the install has no script library")?)
            .map_err(|e| format!("the script library: {e}"))?;
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
            quests: OnceCell::new(),
            strings: RefCell::new(HashMap::new()),
            entities: RefCell::new(HashMap::new()),
            qualities: RefCell::new(HashMap::new()),
        })
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
        Graph::decode(&st.graph, &self.lib).ok()
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

    fn quest_states(&self) -> &HashMap<u32, u32> {
        self.quests.get_or_init(|| {
            let mut out = HashMap::new();
            for &id in &self.states {
                let Some(g) = self.graph(id) else { continue };
                let nav = Nav::new(&self.lib, &g);
                let Some(root) = g.root().and_then(|r| nav.object(r)) else {
                    continue;
                };
                if nav.field(root, "m_questPhaseInfo").is_none() {
                    continue;
                }
                if let Some(q) = nav.field(root, "mEnum") {
                    out.entry(q).or_insert(id);
                }
            }
            out
        })
    }
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
        let nav = Nav::new(&self.lib, &g);
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

    fn quest_definition(&self, state: Option<u32>, id: u32) -> Option<QuestDefinition> {
        let state = match state {
            Some(s) => s,
            None => *self.quest_states().get(&id)?,
        };
        let g = self.graph(state)?;
        let nav = Nav::new(&self.lib, &g);
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
        for (k, v) in nav.entries(nav.field(root, "m_questPhaseInfo")?)? {
            let Item::Word(phase) = k else { continue };
            let o = nav.object(v.reference()?)?;
            let j = match nav.native(nav.field(o, "m_siJournalEntry")?)? {
                NativeObject::TextInfo(t) => (t.string_id, t.table),
                _ => (0, 0),
            };
            phases.push((phase, j));
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
        Some(QuestDefinition {
            state,
            name: text("m_siLongName")?,
            description: text("m_siDescription")?,
            icon: nav.field(root, "m_didIcon")?,
            play_fx: nav.field(root, "m_bPlayFXOnUpdate").unwrap_or(0) != 0,
            journals: phases.into_iter().map(|(_, j)| j).collect(),
            effects,
            subquests,
            once: done_once(&self.lib, root.package_id()),
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

/// The call a quest's setup makes to say how many times it may be done.
const SET_SOLUTIONS: &str = "Quest::SetMaxNumberOfSolutions";

/// Is a quest's package set up to be done once: its `Setup` (its own, else the nearest of its
/// bases') passes the literal 1 to [`SET_SOLUTIONS`]? Any other value (the globals' -1, any
/// number) or no such call: it may be done again (2017: of the quests whose setup says 1, 0 of 49
/// were given again; of the others, 50 were).
fn done_once(lib: &Library, package: u32) -> bool {
    let mut at = lib.export_of(package);
    let mut guard = 0;
    while let Some(i) = at {
        let Some(p) = lib.packages.get(i) else {
            return false;
        };
        if let Some(f) = p.functions.iter().find(|f| f.name == "Setup") {
            let Some(offset) = f.offset else {
                return false;
            };
            let start = (offset / 4) as usize;
            let end = start + (f.size / 4) as usize;
            return passes_one(lib, start, end.min(lib.code.len()));
        }
        at = p.parent;
        guard += 1;
        if guard > lib.packages.len() {
            return false;
        }
    }
    false
}

/// Does the code in `start..end` call [`SET_SOLUTIONS`] with the literal 1 the last value pushed?
fn passes_one(lib: &Library, start: usize, end: usize) -> bool {
    // The last literal pushed, cleared by anything else that puts a value on the stack.
    let mut literal: Option<i64> = None;
    let mut at = start;
    while at < end {
        let word = lib.code[at];
        match low_byte(word) {
            // A small literal; a literal in the next word.
            0x03 => literal = Some(i64::from(signed(word))),
            0x02 => literal = lib.code.get(at + 1).map(|v| i64::from(*v)),
            // The receiver pushed for the call leaves the argument alone.
            0x23 => {}
            // A call: the one that sets the count reads the literal pushed before it.
            0x0B | 0x4F | 0x51 | 0x53 => {
                let id = lib.code.get(at + 1).copied().unwrap_or(0);
                if lib.function_name(FunctionId(id)).as_deref() == Some(SET_SOLUTIONS) {
                    return literal == Some(1);
                }
                literal = None;
            }
            _ => literal = None,
        }
        at += length(&lib.code, at).unwrap_or(1);
    }
    false
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
