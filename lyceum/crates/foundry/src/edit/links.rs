//! Links between spawns: what a lever, a pressure plate or a door sets off.
//!
//! The server reads a link as a parent and a child (`landblock_instance_link`); the child is
//! marked (`is_Link_Child`) so that it is made with its parent, not alone. What a link does is the
//! parent's to say: a generator makes its children, and a door, a switch or a pressure plate that
//! makes nothing is set off by each of its children once that one is used or walked over and has
//! done what it does itself. A link stays within its landblock: the server makes a parent's
//! children from the spawns of the parent's own landblock.
//!
//! What sets a link's parent off is the child's to say: a lever, a pressure plate or a door does
//! when it is used or walked over; a creature only through an Activate step of its emotes (on
//! being given something, used, killed, or a quest answered), which sets off its link's parent,
//! or with none the generator that made it.
//!
//! A link is written with its parent: in the pack's file of it, or for one of the server's own
//! spawns in its landblock's override file, the spawn written again whole there with what it
//! links. The child's mark is written with the child, the same way. A file a dropped building
//! writes again whole when it moves keeps what was linked and marked in it ([`carry_links`]).

use std::collections::{BTreeMap, BTreeSet};
use std::path::{Path, PathBuf};

use super::{guid_text, token_symbol, Override, Overrides, Session, Spawn};
use crate::error::Error;
use crate::sql_ids::{self, WeenieFacts};
use crate::symbol;

/// The weenie types a link sets off when one of them is its parent and makes nothing: a door, a
/// pressure plate, a switch (a lever, or a trap that casts).
pub const DOOR: u32 = 19;
pub const PRESSURE_PLATE: u32 = 24;
pub const SWITCH: u32 = 26;

/// A creature: what sets off its link's parent through an Activate step of its emotes.
pub const CREATURE: u32 = 10;

/// Every link the world makes, by guid.
#[derive(Debug, Default)]
pub(crate) struct Table {
    /// Each parent's children, in the order the links list them; each child's parents.
    children: BTreeMap<u32, Vec<u32>>,
    parents: BTreeMap<u32, Vec<u32>>,
    /// The class each of the packs' instances places, and what the packs' SQL says of each class
    /// it writes, as compiled.
    classes: BTreeMap<u32, u32>,
    facts: BTreeMap<u32, WeenieFacts>,
}

impl Table {
    fn add(&mut self, parent: u32, child: u32) {
        let kids = self.children.entry(parent).or_default();
        if !kids.contains(&child) {
            kids.push(child);
            self.parents.entry(child).or_default().push(parent);
        }
    }
}

/// Where the pack being edited writes a spawn's rows.
enum Home {
    /// The pack's own file of it, and its guid as the file writes it.
    Own(PathBuf, String),
    /// One of the server's own: its landblock, that landblock's override file, and its guid.
    Server(u16, PathBuf, u32),
}

/// A change of a parent's links: a child taken out (by its guid), or one added (as written).
enum Change {
    Remove(u32),
    Add(String),
}

/// The landblock an instance's guid names.
fn landblock_of(guid: u32) -> u16 {
    u16::try_from((guid >> 12) & 0xFFFF).unwrap_or(0)
}

impl Session {
    /// Every link the world makes: the server's own, but those of a parent a pack writes again or
    /// takes away, and the packs' as compiled.
    fn links(&self) -> &Table {
        self.link_table.get_or_init(|| {
            let mut t = Table::default();
            let mut written: BTreeSet<u32> = BTreeSet::new();
            let mut pack_links: Vec<(u32, u32)> = Vec::new();
            for text in self.compiled.sql.values() {
                for i in sql_ids::instances(text) {
                    let Some(g) = sql_ids::number(&i.guid) else {
                        continue;
                    };
                    written.insert(g);
                    if let Some(c) = sql_ids::number(&i.weenie) {
                        t.classes.insert(g, c);
                    }
                }
                written.extend(
                    sql_ids::deleted_instances(text)
                        .iter()
                        .filter_map(|g| sql_ids::number(g)),
                );
                for (w, f) in sql_ids::weenie_facts(text) {
                    if let Some(c) = sql_ids::number(&w) {
                        t.facts.insert(c, f);
                    }
                }
                sql_ids::rows(text, |row| {
                    if row.table == "landblock_instance_link" {
                        let g = |c: &str| row.get(c).and_then(sql_ids::number);
                        if let (Some(p), Some(c)) = (g("parent_GUID"), g("child_GUID")) {
                            pack_links.push((p, c));
                        }
                    }
                });
            }
            for (p, kids) in &self.retail_links {
                if !written.contains(p) {
                    for c in kids {
                        t.add(*p, *c);
                    }
                }
            }
            for (p, c) in pack_links {
                t.add(p, c);
            }
            t
        })
    }

    /// The server's own instance `guid`, as the server places it.
    fn retail_instance(&self, guid: u32) -> Option<&sql_ids::Instance> {
        self.retail_instances
            .get(&landblock_of(guid))?
            .iter()
            .find(|i| sql_ids::number(&i.guid) == Some(guid))
    }

    /// What the SQL says of the class instance `guid` places: a pack's instance as compiled, else
    /// the server's own; the packs' SQL of the class, else the server's.
    fn instance_facts(&self, guid: u32) -> Option<WeenieFacts> {
        let t = self.links();
        let class = match t.classes.get(&guid) {
            Some(c) => *c,
            None => sql_ids::number(&self.retail_instance(guid)?.weenie)?,
        };
        t.facts
            .get(&class)
            .or_else(|| self.retail.get(&class))
            .cloned()
    }

    /// Whether instance `guid` is set off by what it links: a door, a switch or a pressure plate
    /// that makes nothing.
    fn set_off_by_links(&self, guid: u32) -> bool {
        self.instance_facts(guid)
            .is_some_and(|f| matches!(f.kind, Some(DOOR | PRESSURE_PLATE | SWITCH)) && !f.generates)
    }

    /// The guid spawn `s` stands in the world with: one of the server's own as written, a pack's
    /// by its symbol as the lockfile numbers it.
    #[must_use]
    pub fn spawn_number(&self, s: &Spawn) -> Option<u32> {
        if let Some(g) = sql_ids::number(&s.guid) {
            return Some(g);
        }
        self.token_number(&self.packs.get(s.pack)?.name, &s.guid)
    }

    /// The guid an instance's token, as pack `pack` writes it, is numbered.
    fn token_number(&self, pack: &str, written: &str) -> Option<u32> {
        if !written.trim().starts_with("{{guid:") {
            return None;
        }
        let sym = token_symbol(pack, written)?;
        let lb = symbol::instance_landblock(&sym).ok()??;
        self.compiled
            .lock
            .get(symbol::Kind::Guid, &symbol::canonical_instance(&sym, lb))
    }

    /// The guid a link of the pack being edited names as written: a number, or a token.
    fn written_number(&self, written: &str) -> Option<u32> {
        sql_ids::number(written).or_else(|| {
            let pack = &self.packs[self.target].name;
            self.token_number(pack, written)
        })
    }

    /// Whether spawn `s` is a door, a switch or a pressure plate that makes nothing: one that
    /// what it links sets off.
    #[must_use]
    pub fn can_be_set_off(&self, s: &Spawn) -> bool {
        self.spawn_number(s)
            .is_some_and(|g| self.set_off_by_links(g))
    }

    /// Whether spawn `s` is what sets things off: a lever (a switch), a pressure plate, a door
    /// (which opens the next), or a creature (through its emotes).
    #[must_use]
    pub fn sets_things_off(&self, s: &Spawn) -> bool {
        self.spawn_number(s)
            .and_then(|g| self.instance_facts(g))
            .is_some_and(|f| matches!(f.kind, Some(DOOR | PRESSURE_PLATE | SWITCH | CREATURE)))
    }

    /// What owns spawn `s` through a link: the parent of its link that makes what it links (a
    /// generator, a house), when it has one.
    #[must_use]
    pub fn link_owner(&self, s: &Spawn) -> Option<Spawn> {
        let g = self.spawn_number(s)?;
        let p = *self
            .links()
            .parents
            .get(&g)?
            .iter()
            .find(|p| !self.set_off_by_links(**p))?;
        self.spawn_by_number(p)
    }

    /// What spawn `s` sets off: the parent of its link, when that one is set off by what it links.
    #[must_use]
    pub fn sets_off(&self, s: &Spawn) -> Option<Spawn> {
        let g = self.spawn_number(s)?;
        let p = *self
            .links()
            .parents
            .get(&g)?
            .iter()
            .find(|p| self.set_off_by_links(**p))?;
        self.spawn_by_number(p)
    }

    /// What sets spawn `s` off: the children of its links, when it is set off by them, in the
    /// order the links list them.
    #[must_use]
    pub fn set_off_by(&self, s: &Spawn) -> Vec<Spawn> {
        let Some(g) = self.spawn_number(s).filter(|g| self.set_off_by_links(*g)) else {
            return Vec::new();
        };
        let kids = self.links().children.get(&g).cloned().unwrap_or_default();
        if kids.is_empty() {
            return Vec::new();
        }
        let here = self.spawns_of_landblock(landblock_of(g));
        kids.iter()
            .filter_map(|k| here.iter().find(|(n, _)| n == k).map(|(_, x)| x.clone()))
            .collect()
    }

    /// What spawn `s` could be made to set off: every spawn of its landblock that what it links
    /// sets off, but itself and those it would set itself off through; the nearest first.
    #[must_use]
    pub fn link_targets(&self, s: &Spawn) -> Vec<Spawn> {
        let Some(g) = self.spawn_number(s) else {
            return Vec::new();
        };
        let at = s.frame.origin;
        let far = |x: &Spawn| {
            let d = (
                x.frame.origin.x - at.x,
                x.frame.origin.y - at.y,
                x.frame.origin.z - at.z,
            );
            d.0 * d.0 + d.1 * d.1 + d.2 * d.2
        };
        let mut out: Vec<(f32, Spawn)> = self
            .spawns_of_landblock(landblock_of(g))
            .into_iter()
            .filter(|(n, _)| *n != g && self.set_off_by_links(*n) && !self.reaches(*n, g))
            .map(|(_, x)| (far(&x), x))
            .collect();
        out.sort_by(|a, b| a.0.total_cmp(&b.0));
        out.into_iter().map(|(_, x)| x).collect()
    }

    /// Whether instance `from` is set off through the links by `by`: `by` among its parents,
    /// theirs, and on.
    fn reaches(&self, from: u32, by: u32) -> bool {
        let t = self.links();
        let mut seen = BTreeSet::new();
        let mut up = vec![from];
        while let Some(x) = up.pop() {
            if x == by {
                return true;
            }
            if seen.insert(x) {
                up.extend(t.parents.get(&x).into_iter().flatten().copied());
            }
        }
        false
    }

    /// Every spawn standing in landblock `lb`, the packs' and the server's own (shown or not, but
    /// those a pack writes again or takes away), with its guid.
    fn spawns_of_landblock(&self, lb: u16) -> Vec<(u32, Spawn)> {
        let classes = self.pack_classes();
        let mut out: Vec<(u32, Spawn)> = self
            .pack_spawns(&classes)
            .into_iter()
            .filter_map(|s| Some((self.spawn_number(&s)?, s)))
            .filter(|(n, _)| landblock_of(*n) == lb)
            .collect();
        let taken = self.server_taken();
        for i in self.retail_instances.get(&lb).into_iter().flatten() {
            if let Some(s) = self.server_spawn(lb, i, &taken, &classes) {
                if let Some(g) = sql_ids::number(&s.guid) {
                    out.push((g, s));
                }
            }
        }
        out
    }

    /// The spawn whose guid is `guid`.
    fn spawn_by_number(&self, guid: u32) -> Option<Spawn> {
        self.spawns_of_landblock(landblock_of(guid))
            .into_iter()
            .find(|(n, _)| *n == guid)
            .map(|(_, s)| s)
    }

    /// The text of `path` as the drafts in `out` leave it, else as the session holds it.
    pub(super) fn current(
        &mut self,
        path: &Path,
        out: &BTreeMap<PathBuf, Option<String>>,
    ) -> Option<String> {
        match out.get(path) {
            Some(t) => t.clone(),
            None => self.text(path),
        }
    }

    /// Where the pack being edited writes spawn `s`'s rows (`out` holding the drafts so far).
    fn home(&mut self, s: &Spawn, out: &BTreeMap<PathBuf, Option<String>>) -> Result<Home, Error> {
        if let Some(lb) = self.override_landblock(&s.file) {
            let g = sql_ids::number(&s.guid).ok_or_else(|| {
                Error::Refused(format!("{} is not one of the server's spawns", s.guid))
            })?;
            return Ok(Home::Server(lb, s.file.clone(), g));
        }
        if s.pack != self.target || !self.packs[self.target].sql.iter().any(|f| f.path == s.file) {
            return Err(Error::Refused(format!(
                "{} is another pack's: its links are changed in that pack",
                s.file.display()
            )));
        }
        if self.current(&s.file, out).is_none() {
            return Err(Error::Refused(format!("{} is not there", s.file.display())));
        }
        Ok(Home::Own(s.file.clone(), s.guid.clone()))
    }

    /// Spawn `child` (guid `guid`) as a link of the pack being edited names it: one of the server's
    /// own by its guid, the pack's own by its token as its file writes it, another pack's by its
    /// symbol in full.
    fn written_guid(&self, child: &Spawn, guid: u32) -> String {
        if self.override_landblock(&child.file).is_some() || sql_ids::number(&child.guid).is_some()
        {
            return guid_text(guid);
        }
        if child.pack == self.target {
            return child.guid.trim().to_owned();
        }
        self.packs
            .get(child.pack)
            .and_then(|p| token_symbol(&p.name, &child.guid))
            .map_or_else(|| guid_text(guid), |s| format!("{{{{guid:{s}}}}}"))
    }

    /// Parent `p`'s links with `change` made, drafted into `out`: in the pack's file of it, or for
    /// one of the server's own, written again whole into its override file.
    fn draft_children(
        &mut self,
        p: &Spawn,
        change: &Change,
        out: &mut BTreeMap<PathBuf, Option<String>>,
    ) -> Result<(), Error> {
        let edit = |s: &Self, kids: &mut Vec<String>| match change {
            Change::Remove(c) => kids.retain(|k| s.written_number(k) != Some(*c)),
            Change::Add(c) => {
                if !kids.iter().any(|k| k.trim() == c.trim()) {
                    kids.push(c.clone());
                }
            }
        };
        match self.home(p, out)? {
            Home::Server(lb, file, g) => {
                let now = self.current(&file, out).unwrap_or_default();
                let mut o = Overrides::read(&now);
                let Override::Placed(_, kids) = self.override_entry(lb, &mut o, g)? else {
                    return Err(Error::Refused(format!(
                        "{} is taken away by this pack",
                        guid_text(g)
                    )));
                };
                let mut list = std::mem::take(kids);
                edit(self, &mut list);
                if let Some(Override::Placed(_, kids)) = o.entries.get_mut(&g) {
                    *kids = list;
                }
                let text = o.render(lb, &self.today, &|w| self.weenie_name(w));
                out.insert(file, Some(text));
            }
            Home::Own(file, written) => {
                let now = self
                    .current(&file, out)
                    .ok_or_else(|| Error::Refused(format!("{} is not there", file.display())))?;
                let mut kids = children_in(&now, &written);
                edit(self, &mut kids);
                out.insert(
                    file,
                    Some(with_children(&now, &written, &kids, &self.today)),
                );
            }
        }
        Ok(())
    }

    /// The spawn at `home` marked a link's child, or not, drafted into `out`; nothing written
    /// where it is marked so already.
    fn draft_link_child(
        &mut self,
        home: &Home,
        child: bool,
        out: &mut BTreeMap<PathBuf, Option<String>>,
    ) -> Result<(), Error> {
        match home {
            Home::Server(lb, file, g) => {
                let now = self.current(file, out).unwrap_or_default();
                let mut o = Overrides::read(&now);
                let marked = match o.entries.get(g) {
                    Some(Override::Placed(i, _)) => Some(i.link_child),
                    Some(Override::Removed { .. }) => return Ok(()),
                    None => self.retail_instance(*g).map(|i| i.link_child),
                };
                if marked == Some(child) {
                    return Ok(());
                }
                if let Override::Placed(i, _) = self.override_entry(*lb, &mut o, *g)? {
                    i.link_child = child;
                }
                let text = o.render(*lb, &self.today, &|w| self.weenie_name(w));
                out.insert(file.clone(), Some(text));
            }
            Home::Own(file, written) => {
                let now = self
                    .current(file, out)
                    .ok_or_else(|| Error::Refused(format!("{} is not there", file.display())))?;
                let marked = sql_ids::instances(&now)
                    .iter()
                    .find(|i| i.guid.trim() == written.trim())
                    .map(|i| i.link_child);
                if marked == Some(child) {
                    return Ok(());
                }
                let span = sql_ids::instance_values(&now, written)
                    .and_then(|v| v.get("is_Link_Child").cloned())
                    .ok_or_else(|| {
                        Error::Refused(format!(
                            "{}: {written} writes no is_Link_Child",
                            file.display()
                        ))
                    })?;
                let mut text = now;
                text.replace_range(span, if child { "True" } else { "False" });
                out.insert(file.clone(), Some(text));
            }
        }
        Ok(())
    }

    /// [`super::Op::Link`] drafted into `out`: spawn `guid` of `file` made to set off `target`
    /// (its file and guid), or nothing.
    pub(super) fn draft_link(
        &mut self,
        file: &Path,
        guid: &str,
        target: Option<&(PathBuf, String)>,
        out: &mut BTreeMap<PathBuf, Option<String>>,
    ) -> Result<(), Error> {
        let child = self
            .spawn(file, guid)
            .ok_or_else(|| Error::Refused(format!("{} places no spawn {guid}", file.display())))?;
        let c = self
            .spawn_number(&child)
            .ok_or_else(|| Error::Refused(format!("{guid} has no guid yet")))?;
        let parents: Vec<u32> = self.links().parents.get(&c).cloned().unwrap_or_default();
        if let Some(p) = parents.iter().find(|p| !self.set_off_by_links(**p)) {
            return Err(Error::Refused(format!(
                "{guid} is made by {}, and sets nothing off",
                guid_text(*p)
            )));
        }
        let to = match target {
            None => None,
            Some((tf, tg)) => {
                let t = self.spawn(tf, tg).ok_or_else(|| {
                    Error::Refused(format!("{} places no spawn {tg}", tf.display()))
                })?;
                let n = self
                    .spawn_number(&t)
                    .ok_or_else(|| Error::Refused(format!("{tg} has no guid yet")))?;
                if n == c {
                    return Err(Error::Refused("a spawn does not set itself off".into()));
                }
                if landblock_of(n) != landblock_of(c) {
                    return Err(Error::Refused(format!(
                        "{tg} stands in landblock {:04X} and {guid} in {:04X}: a link stays within \
                         its landblock",
                        landblock_of(n),
                        landblock_of(c)
                    )));
                }
                if !self.set_off_by_links(n) {
                    return Err(Error::Refused(format!(
                        "{tg} is not a door, a switch or a pressure plate that makes nothing: \
                         nothing sets it off"
                    )));
                }
                if self.reaches(n, c) {
                    return Err(Error::Refused(format!(
                        "{guid} sets off {tg} already, through what that sets off: it would set \
                         itself off"
                    )));
                }
                Some((t, n))
            }
        };
        let to_n = to.as_ref().map(|(_, n)| *n);
        if parents.iter().copied().eq(to_n) {
            return Err(Error::Refused(match to_n {
                Some(_) => format!("{guid} sets that off already"),
                None => format!("{guid} sets nothing off"),
            }));
        }
        let child_home = self.home(&child, out)?;
        for p in parents.iter().filter(|p| Some(**p) != to_n) {
            let ps = self
                .spawn_by_number(*p)
                .ok_or_else(|| Error::Refused(format!("no spawn {}", guid_text(*p))))?;
            self.draft_children(&ps, &Change::Remove(c), out)?;
        }
        if let Some((t, n)) = &to {
            if !parents.contains(n) {
                let written = self.written_guid(&child, c);
                self.draft_children(t, &Change::Add(written), out)?;
            }
        }
        self.draft_link_child(&child_home, to.is_some(), out)
    }

    /// Spawn `s`, being taken away, let go of: taken out of the links of what it set off, and
    /// what set it off left standing alone (unless something else links it). A link another pack
    /// writes is left to that pack, and one of the server's own goes with its child.
    pub(crate) fn draft_unlinked(
        &mut self,
        s: &Spawn,
        out: &mut BTreeMap<PathBuf, Option<String>>,
    ) -> Result<(), Error> {
        let Some(g) = self.spawn_number(s) else {
            return Ok(());
        };
        let parents: Vec<u32> = self
            .links()
            .parents
            .get(&g)
            .cloned()
            .unwrap_or_default()
            .into_iter()
            .filter(|p| self.set_off_by_links(*p))
            .collect();
        let kids: Vec<u32> = if self.set_off_by_links(g) {
            self.links().children.get(&g).cloned().unwrap_or_default()
        } else {
            Vec::new()
        };
        for p in parents {
            let Some(ps) = self.spawn_by_number(p).filter(|x| !x.server) else {
                continue;
            };
            match self.draft_children(&ps, &Change::Remove(g), out) {
                Ok(()) | Err(Error::Refused(_)) => {}
                Err(e) => return Err(e),
            }
        }
        for k in kids {
            let others = self
                .links()
                .parents
                .get(&k)
                .is_some_and(|ps| ps.iter().any(|p| *p != g));
            if others {
                continue;
            }
            let Some(ks) = self.spawn_by_number(k) else {
                continue;
            };
            let Ok(home) = self.home(&ks, out) else {
                continue;
            };
            self.draft_link_child(&home, false, out)?;
        }
        Ok(())
    }
}

/// `new`, a file written again whole, with what `old` (the file as it stood) links and marks that
/// `new` does not: each link added to its parent's (one `new` places), and each spawn `old` marks a
/// link's child marked so; stamped `today`.
#[must_use]
pub(crate) fn carry_links(old: &str, new: &str, today: &str) -> String {
    let rows = |text: &str| {
        let mut out: Vec<(String, String)> = Vec::new();
        sql_ids::rows(text, |row| {
            if row.table == "landblock_instance_link" {
                let v = |c: &str| row.get(c).unwrap_or_default().trim().to_owned();
                out.push((v("parent_GUID"), v("child_GUID")));
            }
        });
        out
    };
    let placed: Vec<sql_ids::Instance> = sql_ids::instances(new);
    let stands = |g: &str| placed.iter().any(|i| i.guid.trim() == g);
    let (had, has) = (rows(old), rows(new));
    let mut out = new.to_owned();
    let mut parents: Vec<&str> = Vec::new();
    for (p, c) in &had {
        if stands(p) && !has.contains(&(p.clone(), c.clone())) && !parents.contains(&p.as_str()) {
            parents.push(p);
        }
    }
    for p in parents {
        let mut kids = children_in(&out, p);
        for (q, c) in &had {
            if q == p && !kids.contains(c) {
                kids.push(c.clone());
            }
        }
        out = with_children(&out, p, &kids, today);
    }
    for i in sql_ids::instances(old).iter().filter(|i| i.link_child) {
        let unmarked = sql_ids::instances(&out)
            .iter()
            .any(|n| n.guid.trim() == i.guid.trim() && !n.link_child);
        if !unmarked {
            continue;
        }
        if let Some(span) =
            sql_ids::instance_values(&out, &i.guid).and_then(|v| v.get("is_Link_Child").cloned())
        {
            out.replace_range(span, "True");
        }
    }
    out
}

/// The children `text` links spawn `parent` (its guid as written) to, as written, in order.
fn children_in(text: &str, parent: &str) -> Vec<String> {
    let mut out = Vec::new();
    sql_ids::rows(text, |row| {
        if row.table == "landblock_instance_link"
            && row
                .get("parent_GUID")
                .is_some_and(|p| p.trim() == parent.trim())
        {
            if let Some(c) = row.get("child_GUID") {
                out.push(c.trim().to_owned());
            }
        }
    });
    out
}

/// `text` with spawn `parent`'s links (its guid as written) made `children`: its rows taken out
/// of the statements that hold them, and written anew after the rest, stamped `today`.
fn with_children(text: &str, parent: &str, children: &[String], today: &str) -> String {
    let parent = parent.trim();
    let mut out = String::with_capacity(text.len());
    for (range, _) in crate::content::statements(text) {
        let piece = &text[range];
        let mut rows: Vec<(String, String)> = Vec::new();
        sql_ids::rows(piece, |row| {
            if row.table == "landblock_instance_link" {
                let v = |c: &str| row.get(c).unwrap_or_default().trim().to_owned();
                rows.push((v("parent_GUID"), v("child_GUID")));
            }
        });
        let keep: Vec<(&str, &str)> = rows
            .iter()
            .filter(|(p, _)| p != parent)
            .map(|(p, c)| (p.as_str(), c.as_str()))
            .collect();
        if keep.len() == rows.len() {
            out.push_str(piece);
            continue;
        }
        if !keep.is_empty() {
            // The space before the statement stays; the statement is written again with the rest.
            out.push_str(&piece[..piece.len() - piece.trim_start().len()]);
            out.push_str(&link_insert(&keep, today));
        }
    }
    if !children.is_empty() {
        let end = out.trim_end().len();
        out.truncate(end);
        out.push_str("\n\n");
        let rows: Vec<(&str, &str)> = children.iter().map(|c| (parent, c.as_str())).collect();
        out.push_str(&link_insert(&rows, today));
    }
    out
}

/// An insert of links, each a parent and a child as written, stamped `today`.
fn link_insert(rows: &[(&str, &str)], today: &str) -> String {
    let rows: Vec<String> = rows
        .iter()
        .map(|(p, c)| format!("({p}, {c}, '{today}')"))
        .collect();
    format!(
        "INSERT INTO `landblock_instance_link` (`parent_GUID`, `child_GUID`, `last_Modified`)\n\
         VALUES {};\n",
        rows.join("\n     , ")
    )
}

#[cfg(test)]
mod tests {
    //! Behaviour: none (tooling: links between spawns as a pack's SQL writes them)
    use super::*;

    const DOOR_FILE: &str = "/* door, placed in landblock A9B4. */\n\
DELETE FROM `landblock_instance` WHERE `guid` = {{guid:door@A9B4}};\n\
\n\
INSERT INTO `landblock_instance` (`guid`, `weenie_Class_Id`, `obj_Cell_Id`, `origin_X`, `origin_Y`, `origin_Z`, `angles_W`, `angles_X`, `angles_Y`, `angles_Z`, `is_Link_Child`, `last_Modified`)\n\
VALUES ({{guid:door@A9B4}}, 90004, 0xA9B40011, 60.0, 60.0, 94.0, 1.0, 0.0, 0.0, 0.0, False, '2026-10-25 00:00:00');\n";

    #[test]
    fn a_parents_links_are_written_after_its_rows_and_taken_out_again_whole() {
        let door = "{{guid:door@A9B4}}";
        let lever = "{{guid:lever@A9B4}}".to_owned();
        let one = with_children(
            DOOR_FILE,
            door,
            std::slice::from_ref(&lever),
            "2026-10-25 00:00:00",
        );
        assert!(one.starts_with(DOOR_FILE.trim_end()));
        assert!(one.ends_with(
            "\n\nINSERT INTO `landblock_instance_link` (`parent_GUID`, `child_GUID`, `last_Modified`)\n\
             VALUES ({{guid:door@A9B4}}, {{guid:lever@A9B4}}, '2026-10-25 00:00:00');\n"
        ));
        assert_eq!(children_in(&one, door), std::slice::from_ref(&lever));
        // A second, then the first taken out: the second stays, written once.
        let two = with_children(
            &one,
            door,
            &[lever.clone(), "0x7A9B4012".to_owned()],
            "2026-10-25 00:00:00",
        );
        assert_eq!(children_in(&two, door), [lever, "0x7A9B4012".to_owned()]);
        assert_eq!(
            two.matches("INSERT INTO `landblock_instance_link`").count(),
            1
        );
        let back = with_children(&two, door, &[], "2026-10-25 00:00:00");
        assert_eq!(back.trim_end(), DOOR_FILE.trim_end());
    }

    #[test]
    fn another_parents_links_in_the_same_insert_stay() {
        let text = format!(
            "{DOOR_FILE}\nINSERT INTO `landblock_instance_link` (`parent_GUID`, `child_GUID`, `last_Modified`)\n\
             VALUES ({{{{guid:door@A9B4}}}}, 0x7A9B4012, '2019-05-16 00:00:00')\n     , ({{{{guid:gate@A9B4}}}}, 0x7A9B4013, '2019-05-16 00:00:00');\n"
        );
        let after = with_children(&text, "{{guid:door@A9B4}}", &[], "2026-10-25 00:00:00");
        assert!(children_in(&after, "{{guid:door@A9B4}}").is_empty());
        assert_eq!(children_in(&after, "{{guid:gate@A9B4}}"), ["0x7A9B4013"]);
    }

    /// A house's file as a drop writes it: a house that owns its hook, and a door, at `x`.
    fn house_file(x: &str, door_marked: bool) -> String {
        format!(
            "/* a house */
DELETE FROM `landblock_instance` WHERE `guid` = {{{{guid:h@A9B4}}}};
DELETE FROM `landblock_instance` WHERE `guid` = {{{{guid:hook@A9B4}}}};
DELETE FROM `landblock_instance` WHERE `guid` = {{{{guid:d@A9B4}}}};
DELETE FROM `landblock_instance_link` WHERE `parent_GUID` = {{{{guid:h@A9B4}}}};

INSERT INTO `landblock_instance` (`guid`, `weenie_Class_Id`, `obj_Cell_Id`, `origin_X`, `origin_Y`, `origin_Z`, `angles_W`, `angles_X`, `angles_Y`, `angles_Z`, `is_Link_Child`, `last_Modified`)
VALUES ({{{{guid:h@A9B4}}}}, 1, 0xA9B40011, {x}, 60.0, 94.0, 1.0, 0.0, 0.0, 0.0, False, '2026-10-24 00:00:00') /* house */
     , ({{{{guid:hook@A9B4}}}}, 2, 0xA9B40011, {x}, 61.0, 94.0, 1.0, 0.0, 0.0, 0.0, True, '2026-10-24 00:00:00') /* hook */
     , ({{{{guid:d@A9B4}}}}, 3, 0xA9B40011, {x}, 62.0, 94.0, 1.0, 0.0, 0.0, 0.0, {}, '2026-10-24 00:00:00') /* door */;

INSERT INTO `landblock_instance_link` (`parent_GUID`, `child_GUID`, `last_Modified`)
VALUES ({{{{guid:h@A9B4}}}}, {{{{guid:hook@A9B4}}}}, '2026-10-24 00:00:00');
",
            if door_marked { "True" } else { "False" }
        )
    }

    #[test]
    fn a_house_file_written_again_keeps_what_was_linked_to_its_door_and_marked_in_it() {
        // Before the move: a lever sets off its door, and the door opens a gate.
        let old = with_children(
            &house_file("60.0", true),
            "{{guid:d@A9B4}}",
            &["{{guid:lever@A9B4}}".to_owned()],
            "2026-10-25 00:00:00",
        );
        // Written again where it stands now, as its template has it: the door is no child.
        let new = house_file("84.0", false);
        let kept = carry_links(&old, &new, "2026-10-26 00:00:00");
        assert_eq!(
            children_in(&kept, "{{guid:d@A9B4}}"),
            ["{{guid:lever@A9B4}}"]
        );
        assert_eq!(
            children_in(&kept, "{{guid:h@A9B4}}"),
            ["{{guid:hook@A9B4}}"],
            "the house keeps its own"
        );
        let placed = sql_ids::instances(&kept);
        let door = placed.iter().find(|i| i.guid == "{{guid:d@A9B4}}").unwrap();
        assert!(door.link_child, "the door is the gate's child still");
        assert!((door.origin[0] - 84.0).abs() < 1e-3, "where it stands now");
        // Nothing to keep: the file as written.
        assert_eq!(carry_links(&new, &new, "2026-10-26 00:00:00"), new);
        // A link of a spawn the file no longer places goes.
        let gone = carry_links(
            &old,
            &new.replace("{{guid:d@A9B4}}", "{{guid:e@A9B4}}"),
            "2026-10-26 00:00:00",
        );
        assert!(children_in(&gone, "{{guid:d@A9B4}}").is_empty());
    }
}
