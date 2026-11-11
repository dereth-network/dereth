//! The recipes the world defines (its `[[recipe]]` rows), where the install holds none.
//!
//! A row's recipe is made from an install recipe's saved state: everything the row does not say
//! (its craft skill and category unless the row names them, its animation and cycles, its checks
//! and their thresholds, its cost, its flags) is that recipe's. The row replaces its name and
//! description; its slots, one per ingredient the row lists, each taking any of the row's
//! templates, as many as it says; and what its outcomes make. Of each outcome's actions only
//! those that make its product (the first, now the row's product and count), stamp it with its
//! maker, and use up its ingredients (one for each of the row's slots, as many as the slot takes)
//! are kept: the rest (a quest moved on, a line of its own) were the install recipe's.
//!
//! The recipe made is read into crafting as an install recipe is, and its record (the saved state
//! as a data file stores it) is served to a client that asks for it: a client's own files hold no
//! record by its id, and its crafting page reads a recipe from its record.

use std::collections::{BTreeMap, BTreeSet};

use auberean_script::graph::{Body, Entry, Graph, PackedObject};
use auberean_script::library::Library;
use auberean_script::native_object::{Item, NativeObject};
use auberean_script::value::tag;
use yalain_content::crafts::Recipe;

/// The record of a saved state: its id, its graph, the package of its root, as a data file holds
/// it.
///
/// # Errors
/// The graph does not pack.
pub fn record(id: u32, g: &Graph) -> Result<Vec<u8>, String> {
    let package = root(g)?.package_id();
    let state = auberean_assets::script::ScriptState {
        graph: g.encode()?,
        package,
    };
    Ok(auberean_assets::encode(id, &state))
}

/// A row's recipe, made from the saved state of the recipe it names (`like`).
///
/// # Errors
/// That state is not a recipe this can make one from: it has no slot, or a field a recipe holds
/// is missing.
pub fn make(lib: &Library, like: &Graph, row: &Recipe) -> Result<Graph, String> {
    let mut e = Edit {
        lib,
        g: like.clone(),
    };
    let root = like.root().ok_or("an empty graph")?;
    // Its objects came from the install recipe's state; they come from the row's now.
    for entry in &mut e.g.entries {
        if let Body::Object(o) = &mut entry.body {
            if o.state == row.like {
                o.state = row.recipe;
            }
        }
    }
    e.text(root, "m_siName", Some(row.name))?;
    e.text(root, "m_siDesc", row.description)?;
    if let Some(skill) = row.skill {
        e.set(root, "m_didCraftSkill", skill)?;
    }
    if let Some(category) = row.category {
        e.set(root, "m_craftSkillCategory", category)?;
    }
    let n = u32::try_from(row.ingredients.len()).map_err(|e| e.to_string())?;
    e.set(root, "m_maxNumIngredients", n)?;
    // The slots: each a copy of the install recipe's first.
    let list = e.get(root, "m_ingredients")?;
    let first = e
        .items(list)?
        .first()
        .and_then(Item::reference)
        .ok_or("the recipe it is made from has no slot")?;
    let mut slots = Vec::new();
    for (i, ing) in (1..).zip(&row.ingredients) {
        let id = if i == 1 { first } else { e.copy(first)? };
        e.set(id, "m_uiOrdinal", i)?;
        e.set(id, "m_uiQuantity", ing.count)?;
        let items: Vec<(Item, Item)> = ing
            .items
            .iter()
            .map(|t| (Item::Word(*t), Item::Word(*t)))
            .collect();
        e.hash(id, "m_hashItems", &items)?;
        e.hash(id, "m_hashItemClasses", &[])?;
        e.text(id, "m_siDesc", ing.name)?;
        e.text(id, "m_siPluralDesc", None)?;
        e.text(id, "m_siForcedDesc", None)?;
        slots.push(Item::Ref(id));
    }
    e.set_items(list, slots)?;
    // The outcomes: what each makes, stamps and uses up.
    let outcomes: Vec<u32> = e
        .g
        .ids
        .iter()
        .zip(&e.g.entries)
        .filter_map(|(id, entry)| match &entry.body {
            Body::Object(o) if lib.package_name(o.package_id()) == "CraftRandomEntry" => Some(*id),
            _ => None,
        })
        .collect();
    for outcome in outcomes {
        let list = e.get(outcome, "m_outcomeActions")?;
        if list == 0 || list == auberean_script::value::NULL {
            continue;
        }
        let (mut produced, mut used) = (false, false);
        let mut kept = Vec::new();
        for action in e.items(list)? {
            let Some(id) = action.reference() else {
                continue;
            };
            match e.package(id)?.as_str() {
                "ProduceRecipeAction" if !produced => {
                    e.set(id, "m_entityDID", row.product)?;
                    e.set(id, "m_uiQuantity", row.count)?;
                    produced = true;
                    kept.push(action);
                }
                "StampRecipeAction" => kept.push(action),
                "DestroyRecipeAction" if !used => {
                    for (i, ing) in (1..).zip(&row.ingredients) {
                        let id = if i == 1 { id } else { e.copy(id)? };
                        e.set(id, "m_uiOrdinal", i)?;
                        e.set(id, "m_uiQuantity", ing.count)?;
                        kept.push(Item::Ref(id));
                    }
                    used = true;
                }
                _ => {}
            }
        }
        e.set_items(list, kept)?;
    }
    e.prune();
    Ok(e.g)
}

fn root(g: &Graph) -> Result<&PackedObject, String> {
    match g.entries.first().map(|e| &e.body) {
        Some(Body::Object(o)) => Ok(o),
        _ => Err("the graph's root is no object".into()),
    }
}

/// A graph being made over, its fields by the library's names for them.
struct Edit<'a> {
    lib: &'a Library,
    g: Graph,
}

impl Edit<'_> {
    fn at(&self, id: u32) -> Result<usize, String> {
        self.g
            .ids
            .iter()
            .position(|x| *x == id)
            .ok_or_else(|| format!("no entry {id:#x}"))
    }

    fn object(&mut self, id: u32) -> Result<&mut PackedObject, String> {
        let i = self.at(id)?;
        match &mut self.g.entries[i].body {
            Body::Object(o) => Ok(o),
            _ => Err(format!("entry {id:#x} is no object")),
        }
    }

    fn package(&mut self, id: u32) -> Result<String, String> {
        let lib = self.lib;
        Ok(lib.package_name(self.object(id)?.package_id()))
    }

    /// The slot a field of an object's package lies in.
    fn slot(&mut self, id: u32, name: &str) -> Result<usize, String> {
        let pkg = self.package(id)?;
        let frame = self
            .lib
            .frame(&pkg)
            .ok_or_else(|| format!("the library has no frame for {pkg}"))?;
        let m = frame
            .members
            .iter()
            .find(|m| m.name == name)
            .ok_or_else(|| format!("{pkg} has no field {name}"))?;
        usize::try_from(m.slot).map_err(|_| format!("{pkg}::{name} is not a field"))
    }

    fn get(&mut self, id: u32, name: &str) -> Result<u32, String> {
        let slot = self.slot(id, name)?;
        self.object(id)?
            .slots
            .get(slot)
            .copied()
            .ok_or_else(|| format!("{name}: no slot {slot}"))
    }

    /// Set a field to a value of the kind it holds: its tag stays.
    fn set(&mut self, id: u32, name: &str, value: u32) -> Result<(), String> {
        let slot = self.slot(id, name)?;
        let o = self.object(id)?;
        *o.slots
            .get_mut(slot)
            .ok_or_else(|| format!("{name}: no slot {slot}"))? = value;
        Ok(())
    }

    /// The native object a field refers to.
    fn native(&mut self, id: u32, name: &str) -> Result<&mut NativeObject, String> {
        let r = self.get(id, name)?;
        self.object(r)?
            .native
            .as_mut()
            .ok_or_else(|| format!("{name} is no native object"))
    }

    /// Set a string field to a string reference `(string, table)`, or to none.
    fn text(&mut self, id: u32, name: &str, to: Option<(u32, u32)>) -> Result<(), String> {
        let NativeObject::TextInfo(t) = self.native(id, name)? else {
            return Err(format!("{name} is no string"));
        };
        let (string, table) = to.unwrap_or((0, 0));
        t.string_id = string;
        t.table = table;
        t.has_literal = 0;
        t.literal = None;
        t.variables.clear();
        Ok(())
    }

    /// Fill a hash table field with these entries, as the client's tables insert them.
    fn hash(&mut self, id: u32, name: &str, entries: &[(Item, Item)]) -> Result<(), String> {
        let n = self.native(id, name)?;
        let NativeObject::Hash(h) = &*n else {
            return Err(format!("{name} is no hash table"));
        };
        *n = crate::graph::hash(h.key, h.value, h.size_index, entries);
        Ok(())
    }

    fn items(&mut self, list: u32) -> Result<Vec<Item>, String> {
        match self.object(list)?.native.as_ref() {
            Some(NativeObject::Sequence { items, .. }) => Ok(items.clone()),
            _ => Err(format!("entry {list:#x} is no list")),
        }
    }

    fn set_items(&mut self, list: u32, to: Vec<Item>) -> Result<(), String> {
        match self.object(list)?.native.as_mut() {
            Some(NativeObject::Sequence { items, .. }) => {
                *items = to;
                Ok(())
            }
            _ => Err(format!("entry {list:#x} is no list")),
        }
    }

    /// The ids an entry refers to.
    fn refers(entry: &Entry) -> Vec<u32> {
        match &entry.body {
            Body::Object(o) => o
                .slots
                .iter()
                .zip(&o.tags)
                .filter(|(_, t)| tag::is_reference(**t))
                .map(|(s, _)| *s)
                .chain(o.native.iter().flat_map(NativeObject::references))
                .filter(|r| *r != 0 && *r != auberean_script::value::NULL)
                .collect(),
            Body::Relative(r) => vec![*r],
            Body::Singleton(_) | Body::Elsewhere => Vec::new(),
        }
    }

    /// A copy of an entry and of everything it reaches but the singletons (which stay one), under
    /// new ids: the copy's id.
    fn copy(&mut self, id: u32) -> Result<u32, String> {
        let mut map = BTreeMap::new();
        let mut todo = vec![id];
        let mut next = self.g.ids.iter().copied().max().unwrap_or(0) + 1;
        while let Some(x) = todo.pop() {
            if map.contains_key(&x) {
                continue;
            }
            let entry = &self.g.entries[self.at(x)?];
            if matches!(entry.body, Body::Singleton(_)) {
                continue;
            }
            map.insert(x, next);
            next += 1;
            todo.extend(Self::refers(entry));
        }
        for (old, new) in &map {
            let mut entry = self.g.entries[self.at(*old)?].clone();
            let remap = |r: u32| map.get(&r).copied().unwrap_or(r);
            match &mut entry.body {
                Body::Object(o) => {
                    for (s, t) in o.slots.iter_mut().zip(&o.tags) {
                        if tag::is_reference(*t) {
                            *s = remap(*s);
                        }
                    }
                    if let Some(n) = &mut o.native {
                        n.remap(&mut |r| remap(r));
                    }
                }
                Body::Relative(r) => *r = remap(*r),
                Body::Singleton(_) | Body::Elsewhere => {}
            }
            self.g.ids.push(*new);
            self.g.entries.push(entry);
        }
        map.get(&id)
            .copied()
            .ok_or_else(|| format!("entry {id:#x} is a singleton"))
    }

    /// Leave out every entry the root no longer reaches.
    fn prune(&mut self) {
        let Some(root) = self.g.root() else { return };
        let at: BTreeMap<u32, usize> = self
            .g
            .ids
            .iter()
            .enumerate()
            .map(|(i, x)| (*x, i))
            .collect();
        let mut reached = BTreeSet::new();
        let mut todo = vec![root];
        while let Some(x) = todo.pop() {
            if !reached.insert(x) {
                continue;
            }
            if let Some(i) = at.get(&x) {
                todo.extend(Self::refers(&self.g.entries[*i]));
            }
        }
        let (ids, entries) = self
            .g
            .ids
            .iter()
            .zip(&self.g.entries)
            .filter(|(id, _)| reached.contains(id))
            .map(|(id, e)| (*id, e.clone()))
            .unzip();
        self.g.ids = ids;
        self.g.entries = entries;
    }
}
