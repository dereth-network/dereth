//! Where a new character starts and what it starts with, read from the client's character
//! generation data: per start area, its places (the copies of an instanced training hall, or a
//! few spots outdoors); per race, its start area; per race, its starting skills.
//!
//! The data is a saved script state (a packed object graph) the client also reads to draw its
//! creation screens. [`StartRules::from_matrix`] reads it with the script library's names for the
//! fields; the server's tests build rules by hand.

use std::collections::{BTreeMap, HashMap};

use auberean_script::graph::{Body, Graph, PackedObject};
use auberean_script::library::Library;
use auberean_script::native_object::{Item, NativeObject};
use yalain_store::StoredPosition;

/// A start area: its kind (the creation screen's choice), its name, its places.
#[derive(Debug, Clone, PartialEq)]
pub struct StartArea {
    pub kind: u32,
    pub name: String,
    pub places: Vec<StoredPosition>,
}

/// Where new characters start and what they start with.
#[derive(Debug, Clone, PartialEq, Default)]
pub struct StartRules {
    /// The start areas, by kind.
    pub areas: BTreeMap<u32, StartArea>,
    /// Per race, the kinds of its start areas.
    pub by_race: BTreeMap<u32, Vec<u32>>,
    /// Per race, its starting skills in the data's order: the skill and its level (0, a skill
    /// to raise; 1, a gift).
    pub skills: BTreeMap<u32, Vec<(u32, u32)>>,
    /// Per race, the templates of what it starts with, in the data's order: its weapons, then what
    /// it wears.
    pub items: BTreeMap<u32, Vec<u32>>,
}

/// Which starting-skills table each race reads: the data keys the tables by a skill family, and
/// pairs a race with its family nowhere the client holds; this pairing is every 2017 session
/// start's (each character's skills are its family's table, matched race by race).
pub const SKILL_TABLE_OF_RACE: [(u32, u32); 5] =
    [(1, 4), (2, 2), (4, 8), (8, 32_768), (16, 1_048_576)];

/// The library's kind of a float field.
const FLOAT_MEMBER: u32 = 3;

/// Reading a saved state's graph by the library's names for its fields.
#[allow(missing_debug_implementations)]
pub struct Nav<'a> {
    lib: &'a Library,
    by_id: HashMap<u32, &'a PackedObject>,
    /// The singletons the graph names, by id: their saved states.
    singletons: HashMap<u32, u32>,
}

impl<'a> Nav<'a> {
    #[must_use]
    pub fn new(lib: &'a Library, g: &'a Graph) -> Self {
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

    /// The saved states a list field names (its singletons', and its packed objects' own).
    ///
    /// # Errors
    /// The field is missing or is not a list.
    pub fn states(&self, o: &PackedObject, name: &str) -> Result<Vec<u32>, String> {
        let r = self.field(o, name)?;
        if r == 0 || r == crate::graph::NULL {
            return Ok(Vec::new());
        }
        Ok(self
            .items(r)?
            .into_iter()
            .filter_map(|i| i.reference())
            .filter_map(|id| {
                self.singletons
                    .get(&id)
                    .copied()
                    .or_else(|| self.by_id.get(&id).map(|o| o.state))
            })
            .collect())
    }

    /// The saved state a singleton entry of the graph names.
    #[must_use]
    pub fn singleton(&self, id: u32) -> Option<u32> {
        self.singletons.get(&id).copied()
    }

    pub fn object(&self, id: u32) -> Result<&'a PackedObject, String> {
        self.by_id
            .get(&id)
            .copied()
            .ok_or_else(|| format!("no object {id:#x} in the graph"))
    }

    /// A script object's field, by the library's name for it.
    ///
    /// # Errors
    /// The library has no such field for the object's package.
    pub fn field(&self, o: &PackedObject, name: &str) -> Result<u32, String> {
        self.member(o, name).map(|(w, _)| w)
    }

    /// A script object's field and the library's kind for it (2 an integer, 3 a float).
    fn member(&self, o: &PackedObject, name: &str) -> Result<(u32, u32), String> {
        let pkg = self.lib.package_name(o.package_id());
        let frame = self
            .lib
            .frame(&pkg)
            .ok_or_else(|| format!("the library has no frame for {pkg}"))?;
        let m = frame
            .members
            .iter()
            .find(|m| m.name == name)
            .ok_or_else(|| format!("{pkg} has no field {name}"))?;
        let slot = usize::try_from(m.slot).map_err(|_| format!("{pkg}::{name} is not a field"))?;
        o.slots
            .get(slot)
            .copied()
            .map(|w| (w, m.kind))
            .ok_or_else(|| format!("{pkg} has no slot {slot}"))
    }

    /// A double field: it lies in two slots, the low word first.
    ///
    /// # Errors
    /// The library has no such field for the object's package.
    pub fn double(&self, o: &PackedObject, name: &str) -> Result<f64, String> {
        let pkg = self.lib.package_name(o.package_id());
        let frame = self
            .lib
            .frame(&pkg)
            .ok_or_else(|| format!("the library has no frame for {pkg}"))?;
        let m = frame
            .members
            .iter()
            .find(|m| m.name == name)
            .ok_or_else(|| format!("{pkg} has no field {name}"))?;
        let slot = usize::try_from(m.slot).map_err(|_| format!("{pkg}::{name} is not a field"))?;
        let word = |i: usize| {
            o.slots
                .get(i)
                .copied()
                .ok_or_else(|| format!("{pkg} has no slot {i}"))
        };
        Ok(f64::from_bits(
            u64::from(word(slot)?) | u64::from(word(slot + 1)?) << 32,
        ))
    }

    /// A number field as its value: an integer's (signed) or a float's.
    ///
    /// # Errors
    /// The library has no such field for the object's package.
    pub fn number(&self, o: &PackedObject, name: &str) -> Result<f32, String> {
        let (w, kind) = self.member(o, name)?;
        #[allow(clippy::cast_precision_loss)]
        Ok(if kind == FLOAT_MEMBER {
            f32::from_bits(w)
        } else {
            w.cast_signed() as f32
        })
    }

    /// The objects a list field holds (none for a null reference).
    ///
    /// # Errors
    /// The field is missing or is not a list.
    pub fn list(&self, o: &PackedObject, name: &str) -> Result<Vec<&'a PackedObject>, String> {
        let r = self.field(o, name)?;
        if r == 0 || r == crate::graph::NULL {
            return Ok(Vec::new());
        }
        self.items(r)?
            .into_iter()
            .filter_map(|i| i.reference())
            .map(|id| self.object(id))
            .collect()
    }

    pub fn native(&self, id: u32) -> Result<&'a NativeObject, String> {
        self.object(id)?
            .native
            .as_ref()
            .ok_or_else(|| format!("object {id:#x} is not a native"))
    }

    pub fn entries(&self, id: u32) -> Result<Vec<(Item, Item)>, String> {
        match self.native(id)? {
            NativeObject::Hash(h) => Ok(h.entries().copied().collect()),
            _ => Err(format!("object {id:#x} is not a hash table")),
        }
    }

    pub fn items(&self, id: u32) -> Result<Vec<Item>, String> {
        match self.native(id)? {
            NativeObject::Sequence { items, .. } => Ok(items.clone()),
            _ => Err(format!("object {id:#x} is not a list")),
        }
    }

    pub fn reference(i: Item) -> Result<u32, String> {
        i.reference()
            .ok_or_else(|| format!("{i:?} is not a reference"))
    }

    pub fn word(i: Item) -> Result<u32, String> {
        match i {
            Item::Word(w) => Ok(w),
            other => Err(format!("{other:?} is not a word")),
        }
    }
}

impl StartRules {
    /// Read the character generation data's graph.
    ///
    /// # Errors
    /// A table or field the data should hold is missing or of another shape.
    pub fn from_matrix(lib: &Library, g: &Graph) -> Result<Self, String> {
        let nav = Nav::new(lib, g);
        let root = nav.object(g.root().ok_or("an empty graph")?)?;
        let mut rules = Self::default();
        for (_, area) in nav.entries(nav.field(root, "m_StartAreaHash")?)? {
            let a = nav.object(Nav::reference(area)?)?;
            let kind = nav.field(a, "m_startingLocType")?;
            let name = match nav.native(nav.field(a, "m_desc")?) {
                Ok(NativeObject::Text(units)) => String::from_utf16_lossy(units),
                _ => String::new(),
            };
            let mut places = Vec::new();
            for p in nav.items(nav.field(a, "m_posList")?)? {
                if let NativeObject::Position { cell, frame } = nav.native(Nav::reference(p)?)? {
                    let f = |i: usize| f32::from_bits(frame[i]);
                    places.push(StoredPosition {
                        cell: *cell,
                        origin: [f(0), f(1), f(2)],
                        rotation: [f(3), f(4), f(5), f(6)],
                    });
                }
            }
            rules.areas.insert(kind, StartArea { kind, name, places });
        }
        for (race, by_species) in nav.entries(nav.field(root, "m_StartingLocationTable")?)? {
            let race = Nav::word(race)?;
            let mut kinds = Vec::new();
            for (_, list) in nav.entries(Nav::reference(by_species)?)? {
                for a in nav.items(Nav::reference(list)?)? {
                    kinds.push(nav.field(nav.object(Nav::reference(a)?)?, "m_startingLocType")?);
                }
            }
            rules.by_race.insert(race, kinds);
        }
        let mut tables = BTreeMap::new();
        for (family, list) in nav.entries(nav.field(root, "m_StartingSkillsTable")?)? {
            let mut skills = Vec::new();
            for s in nav.items(Nav::reference(list)?)? {
                let p = nav.object(Nav::reference(s)?)?;
                skills.push((nav.field(p, "skill")?, nav.field(p, "level")?));
            }
            tables.insert(Nav::word(family)?, skills);
        }
        for (race, family) in SKILL_TABLE_OF_RACE {
            if let Some(t) = tables.get(&family) {
                rules.skills.insert(race, t.clone());
            }
        }
        // By race, then sex and class, though every race's list stands under one of each.
        for (race, by_sex) in nav.entries(nav.field(root, "m_StartingInventoryTable")?)? {
            let items = rules.items.entry(Nav::word(race)?).or_default();
            for (_, by_class) in nav.entries(Nav::reference(by_sex)?)? {
                for (_, list) in nav.entries(Nav::reference(by_class)?)? {
                    for i in nav.items(Nav::reference(list)?)? {
                        let item = nav.object(Nav::reference(i)?)?;
                        items.push(nav.field(item, "entityDID")?);
                    }
                }
            }
        }
        Ok(rules)
    }

    /// The start area a new character of a race begins in: the one its creation chose, if that
    /// is one of its race's, else its race's first.
    #[must_use]
    pub fn area_for(&self, race: u32, chosen: u32) -> Option<&StartArea> {
        let kinds = self.by_race.get(&race)?;
        let kind = if kinds.contains(&chosen) {
            chosen
        } else {
            *kinds.first()?
        };
        self.areas.get(&kind)
    }
}
