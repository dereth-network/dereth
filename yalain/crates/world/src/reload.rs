//! Reloading the world database while the server runs.
//!
//! The host compiles the database again and hands the result over; the world takes it without a
//! restart and says what it did. A compile that fails never reaches here, so the world goes on as
//! it was; everything a reload changes is worked out before the world is touched.
//!
//! - **Taken at once:** the templates the world makes things of, the loot, what storekeepers sell,
//!   the quests, what people say, the story's scenes, the doors' locks, the effects and the
//!   crafting, the templates of creatures' weapons, the operator's banned names, the placed
//!   objects, the camps and the veins, and the areas of player conflict (a player whose
//!   landblock's status changed is told it at once). A template or an effect the new database no
//!   longer has stays known, so a thing or an effect already in the world still resolves.
//! - **Held until a restart:** nothing the world database holds.
//!
//! **Placed objects.** The database's objects before and after pair up: by the database's own
//! id; then the same object; then, among those the database numbers itself, the nearest of the
//! same template and kind. An object that pairs and is the same is left alone (a person walking
//! its route walks on). One that moved or changed keeps its id and is made again where it now
//! stands: every client that holds it and still sees it is sent the object anew with a newer
//! instance stamp, which replaces the one it holds; one that no longer sees it forgets it. A new
//! object is made for every client whose view holds it; one taken away is forgotten by every
//! client that holds it. An object a player is using is let go first: a chest open is closed for
//! its opener (what it held goes with it), and a use waiting on a door or a chest is answered.
//!
//! **Veins** (the craft nodes the database places, standing in the world as nodes anyone may
//! mine) pair by their template and place. One the same keeps its state: its uses so far, or,
//! spent, its coming back. One taken away (or moved, or of another template, which is one taken
//! away and one new) leaves the world, or no longer comes back; a player mining it has the use
//! answered as the vein used up. A new one stands in its place at once.
//!
//! **Camps** pair by their keys. A camp the same, or changed only in how many it keeps and how
//! soon it replaces them, keeps its creatures (more are made at the next step, or the extra
//! retire). Any other change, or a camp taken away, retires its creatures, and a new or changed
//! camp fills at the world's next step. A retiring creature at rest leaves the world at once; one
//! fighting a player is no longer its camp's and leaves once it is at rest again (if it dies
//! first, it dies as any creature, and nothing replaces it).
//!
//! Only the world database's own objects and camps' creatures change: a player's items, corpses,
//! pets and summoned things, and whatever lies on the ground, are never touched.
//!
//! What the server reads from the install alone (the character data, the skills, the levels) is
//! read at start and is not part of the world database.

use std::collections::{BTreeMap, HashMap, HashSet, VecDeque};

use yalain_content::{
    Camp, Catalog, ContentObject, ContentPack, Kind, LockTable, LootTable, QuestTable, SpawnTable,
    StoreTable, StoryTable, TalkTable,
};
use yalain_net::ServerNet;
use yalain_store::{Store, StoredPosition};

use crate::craft::Crafting;
use crate::effect_defs::EffectTable;
use crate::names::NameRules;
use crate::things::Mode;
use crate::world::CONTENT_ID_BASE;
use crate::{presence, Clock, Ground, Server};

/// A world database compiled again, with what the install adds to it.
#[derive(Debug, Default)]
pub struct WorldTables {
    pub content: ContentPack,
    pub catalog: Catalog,
    pub spawns: SpawnTable,
    pub loot: LootTable,
    pub stores: StoreTable,
    pub quests: QuestTable,
    pub talk: TalkTable,
    pub story: StoryTable,
    /// The locked doors and their keys.
    pub locks: LockTable,
    /// The places templates name (the maps' places).
    pub places_named: std::collections::BTreeMap<u32, yalain_content::Place>,
    /// The places by their names (the cities' recalls' places).
    pub places_by_name: std::collections::BTreeMap<String, yalain_content::Place>,
    pub effects: EffectTable,
    pub crafting: Crafting,
    /// The templates of the weapons creatures wield, by the names their templates give them.
    pub weapons: std::collections::HashMap<String, u32>,
    /// What the database says of those weapons beyond their templates.
    pub weapon_rules: std::collections::HashMap<String, yalain_content::WeaponRule>,
    /// What creatures' death effects make.
    pub death_spawns: std::collections::HashMap<String, yalain_content::DeathSpawn>,
    /// The kinds of pet the rows make, and what each summoning skill allows its user.
    pub pet_kinds: Vec<yalain_content::PetKind>,
    pub pet_limits: Vec<(u32, Vec<(usize, u32)>)>,
    /// The operator's banned names read again, when the server was given a list.
    pub names: Option<NameRules>,
    /// What the install says of the placed objects' packages and templates, read again with
    /// them, when the host read it.
    pub kinds: Option<PlacedKinds>,
    /// The ground under the camps, read again with them, when the host read it.
    pub ground: Option<Box<dyn Ground + Send>>,
    /// The files clients' requests for records are served from, opened again through the world's
    /// overlay as it now is, when the host opened them.
    pub files: Option<Box<dyn crate::download::DataFiles + Send>>,
    /// The world's overlay as clients are patched to it, as it now is, with those files.
    pub patch: Option<crate::world_patch::WorldPatch>,
}

/// What the install says of the placed objects' script packages and templates: which are
/// brokers, portals walked into, people, shrines, objects that cannot be used, doors and chests.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct PlacedKinds {
    pub brokers: BTreeMap<u32, crate::market::Broker>,
    pub walk_in: HashSet<u32>,
    pub persons: HashSet<u32>,
    pub lifestones: HashSet<u32>,
    pub shrines: HashSet<u32>,
    pub cannot_use: HashSet<u32>,
    pub doors: HashMap<u32, crate::usable::doors::Kind>,
    pub doors_through_locks: HashSet<u32>,
    pub chests: HashSet<u32>,
}

/// The tables a reload looks at, by the name it reports them under.
pub mod table {
    pub const TEMPLATES: &str = "templates";
    pub const LOOT: &str = "loot";
    pub const STORES: &str = "stores";
    pub const QUESTS: &str = "quests";
    pub const TALK: &str = "talk";
    pub const STORY: &str = "story";
    pub const LOCKS: &str = "locks";
    pub const PLACES_NAMED: &str = "places named";
    pub const EFFECTS: &str = "effects";
    pub const CRAFTING: &str = "crafting";
    pub const WEAPONS: &str = "creature weapons";
    pub const BANNED_NAMES: &str = "banned names";
    pub const PLACED: &str = "placed objects";
    pub const CAMPS: &str = "camps";
    pub const VEINS: &str = "veins";
    pub const CONFLICT: &str = "conflict areas";
}

/// What a reload did, table by table.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Reloaded {
    /// Changed and taken at once.
    pub applied: Vec<&'static str>,
    /// Changed, but held until the server starts again.
    pub restart: Vec<&'static str>,
    /// The same as before.
    pub unchanged: Vec<&'static str>,
    /// What the placed objects and the camps came to, a line each, when they changed.
    pub details: Vec<String>,
}

impl Reloaded {
    /// One line for the operator: what was taken, what waits for a restart, what was the same.
    #[must_use]
    pub fn summary(&self) -> String {
        let list = |v: &[&str]| {
            if v.is_empty() {
                "nothing".to_string()
            } else {
                v.join(", ")
            }
        };
        format!(
            "reloaded: {}; needs a restart: {}; unchanged: {}",
            list(&self.applied),
            list(&self.restart),
            list(&self.unchanged)
        )
    }

    fn note(&mut self, name: &'static str, changed: bool, live: bool) {
        match (changed, live) {
            (false, _) => self.unchanged.push(name),
            (true, true) => self.applied.push(name),
            (true, false) => self.restart.push(name),
        }
    }
}

/// What reloading keeps of the placed objects and camps from one reload to the next.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct Placed {
    /// The placed objects as the world database last gave them, place for place with the world's
    /// (where a person walking its route set out from, not where it stands now); none until the
    /// world is given its camps.
    pub loaded: Option<Vec<ContentObject>>,
    /// The veins the database placed (they stand in the world as nodes).
    pub veins: Vec<ContentObject>,
    /// The ids a reload kept for objects the database numbers itself, now that their places in
    /// the list no longer number them.
    pub numbered: HashSet<u64>,
    /// The last number a reload gave a newly placed object.
    pub last_number: u64,
    /// The instance stamp of each object made again (the others' is 1).
    pub instances: HashMap<u64, u16>,
    /// The retiring creatures that were fighting: each leaves the world once it is at rest.
    pub retiring: Vec<u64>,
}

impl Placed {
    /// The instance stamp the world's object with this id is made with.
    #[must_use]
    pub fn instance(&self, id: u64) -> u16 {
        self.instances.get(&id).copied().unwrap_or(1)
    }

    /// The id the database gave an object itself (none for one it numbers, whatever id a reload
    /// kept for it).
    #[must_use]
    pub fn database_id(&self, o: &ContentObject) -> Option<u64> {
        o.object.filter(|id| !self.numbered.contains(id))
    }
}

impl<S: Store> Server<S> {
    /// Take a world database compiled again: what can change while the world runs changes now
    /// (told to the clients that see it), the rest is reported as needing a restart and left as
    /// it was.
    pub fn reload(&mut self, net: &mut ServerNet, clock: Clock, mut t: WorldTables) -> Reloaded {
        let mut r = Reloaded::default();
        // The placed objects as the world holds them: with camps, the camps keep the creatures.
        if !t.spawns.camps.is_empty() {
            t.content.objects.retain(|o| o.kind != Kind::Creature);
        }

        let mut catalog = self.catalog.clone();
        catalog.templates.extend(t.catalog.templates);
        catalog.profiles.extend(t.catalog.profiles);
        catalog.icons.extend(t.catalog.icons);
        catalog.icon_tables.extend(t.catalog.icon_tables);
        catalog.bodies.extend(t.catalog.bodies);
        catalog.implements.extend(t.catalog.implements);
        let changed = catalog != self.catalog;
        r.note(table::TEMPLATES, changed, true);
        self.catalog = catalog;

        let changed = t.loot != self.loot;
        r.note(table::LOOT, changed, true);
        self.loot = t.loot;

        let changed = t.stores != self.stores;
        r.note(table::STORES, changed, true);
        self.stores = t.stores;

        let changed = t.quests != self.quests;
        r.note(table::QUESTS, changed, true);
        self.quests = t.quests;

        let changed = t.talk != self.talk;
        r.note(table::TALK, changed, true);
        if changed {
            // Who has lines for a player near, and who chatters, is found again.
            self.talkers = None;
        }
        self.talk = t.talk;

        let changed = t.story != self.story;
        r.note(table::STORY, changed, true);
        self.story = t.story;

        let changed = t.locks != self.locks;
        r.note(table::LOCKS, changed, true);
        self.locks = t.locks;

        let changed =
            t.places_named != self.places_named || t.places_by_name != self.places_by_name;
        r.note(table::PLACES_NAMED, changed, true);
        self.places_named = t.places_named;
        self.places_by_name = t.places_by_name;

        let mut effects = self.effect_table.clone();
        effects.effects.extend(t.effects.effects);
        let changed = effects != self.effect_table;
        r.note(table::EFFECTS, changed, true);
        self.effect_table = effects;

        let changed = t.crafting != self.crafting;
        r.note(table::CRAFTING, changed, true);
        self.crafting = t.crafting;

        // A creature already armed keeps its weapons; those made from now on are of the new table.
        let changed =
            t.weapons != self.creature_weapons || t.weapon_rules != self.creature_weapon_rules;
        r.note(table::WEAPONS, changed, true);
        self.creature_weapons = t.weapons;
        self.creature_weapon_rules = t.weapon_rules;
        self.death_spawns = t.death_spawns;
        let fresh = crate::pets::Pets::default();
        self.pets.kinds = fresh.kinds;
        self.pets.limits = fresh.limits;
        self.pets.read_rows(t.pet_kinds, t.pet_limits);

        if let Some(names) = t.names {
            r.note(table::BANNED_NAMES, names != self.names, true);
            self.names = names;
        }

        // What the placed objects are (doors, chests, brokers ...) is known before they are made.
        if let Some(k) = t.kinds {
            self.brokers.extend(k.brokers);
            self.walk_in_packages.extend(k.walk_in);
            self.person_packages.extend(k.persons);
            self.lifestone_packages.extend(k.lifestones);
            self.shrine_packages.extend(k.shrines);
            self.cannot_use_packages.extend(k.cannot_use);
            self.doors.packages.extend(k.doors);
            self.doors
                .creatures_through_locks
                .extend(k.doors_through_locks);
            self.chests.packages.extend(k.chests);
        }

        // Veins stand in the world as nodes, not among the placed objects.
        let (veins, placed): (Vec<_>, Vec<_>) = std::mem::take(&mut t.content.objects)
            .into_iter()
            .partition(|o| {
                self.catalog
                    .get(o.template)
                    .is_some_and(crate::mining::is_craft_node)
            });
        let changed = take_placed(self, net, placed);
        self.stand_placed();
        self.workshops = crate::workshops::Workshops::of(&self.content.objects, &self.catalog);
        r.note(table::PLACED, changed.is_some(), true);
        r.details.extend(changed);

        // A player whose landblock's conflict changed is told its new status at once.
        let changed = t.content.conflict != self.content.conflict;
        r.note(table::CONFLICT, changed, true);
        if changed {
            self.content.conflict = std::mem::take(&mut t.content.conflict);
            let worlds: Vec<_> = self.players.keys().copied().collect();
            for world in worlds {
                crate::kingdoms::check(self, net, world);
            }
        }

        if let Some(g) = t.ground {
            self.ground = Some(g);
        }
        if let Some(f) = t.files {
            self.data = Some(f);
            self.world_patch = t.patch.map(std::sync::Arc::new);
        }
        let changed = take_camps(self, net, clock, t.spawns);
        r.note(table::CAMPS, changed.is_some(), true);
        r.details.extend(changed);

        let changed = take_veins(self, net, veins);
        r.note(table::VEINS, changed.is_some(), true);
        r.details.extend(changed);
        r
    }
}

/// A place counted across the whole world, in meters (landblocks are 160 m on a side).
fn spot(o: &ContentObject) -> [f32; 3] {
    #[allow(clippy::cast_precision_loss)]
    let block = |b: u32| (b & 0xFF) as f32 * 160.0;
    [
        block(o.cell >> 24) + o.origin[0],
        block(o.cell >> 16) + o.origin[1],
        o.origin[2],
    ]
}

/// How the database's placed objects before and after pair up: each new object's old one, if
/// it has one. By the database's own id first; then the same object; then, among those the
/// database numbers itself, the nearest of the same template and kind.
fn pair(old: &[ContentObject], new: &[ContentObject]) -> Vec<Option<usize>> {
    let mut of_new = vec![None; new.len()];
    let mut taken = vec![false; old.len()];
    let mut by_id: HashMap<u64, VecDeque<usize>> = HashMap::new();
    for (i, o) in old.iter().enumerate() {
        if let Some(id) = o.object {
            by_id.entry(id).or_default().push_back(i);
        }
    }
    for (j, o) in new.iter().enumerate() {
        let Some(id) = o.object else { continue };
        if let Some(i) = by_id.get_mut(&id).and_then(VecDeque::pop_front) {
            of_new[j] = Some(i);
            taken[i] = true;
        }
    }

    let key = |o: &ContentObject| (o.template, o.cell, o.origin.map(f32::to_bits));
    let mut same: HashMap<_, Vec<usize>> = HashMap::new();
    for (i, o) in old.iter().enumerate() {
        if !taken[i] && o.object.is_none() {
            same.entry(key(o)).or_default().push(i);
        }
    }
    for (j, o) in new.iter().enumerate() {
        if of_new[j].is_some() || o.object.is_some() {
            continue;
        }
        let Some(list) = same.get_mut(&key(o)) else {
            continue;
        };
        if let Some(at) = list.iter().position(|&i| !taken[i] && old[i] == *o) {
            let i = list.remove(at);
            of_new[j] = Some(i);
            taken[i] = true;
        }
    }

    let mut alike: HashMap<(u32, &str), Vec<usize>> = HashMap::new();
    for (i, o) in old.iter().enumerate() {
        if !taken[i] && o.object.is_none() {
            alike
                .entry((o.template, o.kind.word()))
                .or_default()
                .push(i);
        }
    }
    // Those that only moved first, then any; the nearest each time.
    for moved_only in [true, false] {
        for (j, o) in new.iter().enumerate() {
            if of_new[j].is_some() || o.object.is_some() {
                continue;
            }
            let Some(list) = alike.get(&(o.template, o.kind.word())) else {
                continue;
            };
            let here = spot(o);
            let far = |i: usize| {
                let there = spot(&old[i]);
                (0..3).map(|k| (here[k] - there[k]).powi(2)).sum::<f32>()
            };
            let nearest = list
                .iter()
                .copied()
                .filter(|&i| !taken[i] && (!moved_only || only_moved(&old[i], o)))
                .min_by(|&a, &b| far(a).total_cmp(&far(b)).then(a.cmp(&b)));
            if let Some(i) = nearest {
                of_new[j] = Some(i);
                taken[i] = true;
            }
        }
    }
    of_new
}

/// Does an object differ from another only in where it stands (and where its row says it came
/// from, which no client is told)?
fn only_moved(a: &ContentObject, b: &ContentObject) -> bool {
    let mut a = ContentObject {
        cell: b.cell,
        origin: b.origin,
        rotation: b.rotation,
        placed: b.placed.clone(),
        ..a.clone()
    };
    a.description.source.clone_from(&b.description.source);
    a == *b
}

/// Take the database's placed objects: what changed is made, made again or forgotten for the
/// clients that see it. A line saying what changed, if anything did.
fn take_placed<S: Store>(
    s: &mut Server<S>,
    net: &mut ServerNet,
    new: Vec<ContentObject>,
) -> Option<String> {
    let live_ids: Vec<u64> = (0..s.content.objects.len())
        .map(|i| presence::content_id(&s.content.objects, i))
        .collect();
    // The database's form of what the world holds: as it was last loaded, else as it stands.
    let old = match s.placed.loaded.take() {
        Some(l) if l.len() == s.content.objects.len() => l,
        _ => s
            .content
            .objects
            .iter()
            .map(|o| ContentObject {
                object: s.placed.database_id(o),
                ..o.clone()
            })
            .collect(),
    };
    let pairs = pair(&old, &new);
    let same = old.len() == new.len()
        && pairs
            .iter()
            .enumerate()
            .all(|(j, p)| *p == Some(j) && old[j] == new[j]);
    if same {
        s.placed.loaded = Some(new);
        return None;
    }

    // Worked out before anything changes: what stays, what is made again, what is new, what goes.
    let mut kept = vec![false; old.len()];
    let mut old_to_new = HashMap::new();
    let mut remade = Vec::new();
    let mut moved = 0;
    let mut made = Vec::new();
    for (j, p) in pairs.iter().enumerate() {
        match *p {
            Some(i) => {
                kept[i] = true;
                old_to_new.insert(i, j);
                if old[i] != new[j] {
                    remade.push(j);
                    if only_moved(&old[i], &new[j]) {
                        moved += 1;
                    }
                }
            }
            None => made.push(j),
        }
    }
    let gone: Vec<usize> = (0..old.len()).filter(|&i| !kept[i]).collect();

    // Each object's id: an old one's kept, a new one's the database's or the next number free.
    let mut used: HashSet<u64> = live_ids.iter().copied().collect();
    used.extend(new.iter().filter_map(|o| o.object));
    let low = |id: u64| {
        (id & 0xFFFF_0000_0000_0000 == CONTENT_ID_BASE).then_some(id & 0x0000_FFFF_FFFF_FFFF)
    };
    let mut number = used
        .iter()
        .filter_map(|id| low(*id))
        .chain([s.placed.last_number, new.len() as u64])
        .max()
        .unwrap_or(0);
    let mut ids = Vec::with_capacity(new.len());
    for (j, o) in new.iter().enumerate() {
        let id = match (pairs[j], o.object) {
            (Some(i), _) => live_ids[i],
            (None, Some(id)) => id,
            (None, None) => loop {
                number += 1;
                let id = CONTENT_ID_BASE | number;
                if !used.contains(&id) && s.things.get(id).is_none() {
                    used.insert(id);
                    break id;
                }
            },
        };
        ids.push(id);
    }
    s.placed.last_number = number;

    // What is taken away, or made again, is let go by whoever uses it, with the world as it was.
    for &i in &gone {
        release(s, net, live_ids[i]);
    }
    for &j in &remade {
        release(s, net, ids[j]);
    }
    for &i in &gone {
        let id = live_ids[i];
        presence::thing_gone(s, net, id, s.placed.instance(id));
        s.placed.instances.remove(&id);
        s.visuals.remove(&id);
        s.touching.retain(|_, t| *t != id);
    }

    // The world's objects now: those left alone as they stand, the rest as the database has them.
    let mut live: Vec<Option<ContentObject>> = std::mem::take(&mut s.content.objects)
        .into_iter()
        .map(Some)
        .collect();
    s.placed.numbered.clear();
    let mut objects = Vec::with_capacity(new.len());
    for (j, o) in new.iter().enumerate() {
        let mut object = match pairs[j] {
            Some(i) if !remade.contains(&j) => live[i].take().unwrap_or_else(|| o.clone()),
            _ => o.clone(),
        };
        let id = ids[j];
        object.object = if o.object == Some(id) {
            Some(id)
        } else if id == CONTENT_ID_BASE | (j as u64 + 1) {
            None
        } else {
            s.placed.numbered.insert(id);
            Some(id)
        };
        objects.push(object);
    }
    s.content.objects = objects;
    s.placed.loaded = Some(new);

    // What the world kept by an object's place in the list follows it to its new place.
    s.talkers = None;
    s.greeted = std::mem::take(&mut s.greeted)
        .into_iter()
        .filter_map(|((c, i), t)| old_to_new.get(&i).map(|j| ((c, *j), t)))
        .collect();
    s.chatter = std::mem::take(&mut s.chatter)
        .into_iter()
        .filter_map(|(i, t)| old_to_new.get(&i).map(|j| (*j, t)))
        .collect();
    // A person left alone walks on; one made again sets out afresh from its new place.
    s.walkers
        .by_id
        .retain(|_, w| match old_to_new.get(&w.index) {
            Some(&j) if !remade.contains(&j) => {
                w.index = j;
                true
            }
            _ => false,
        });

    for &j in &remade {
        let id = ids[j];
        let was = s.placed.instance(id);
        s.placed.instances.insert(id, was.wrapping_add(1));
        s.visuals.remove(&id);
        presence::content_placed(s, net, j, Some(was));
    }
    for &j in &made {
        presence::content_placed(s, net, j, None);
    }
    Some(format!(
        "placed objects: {} new, {} moved, {} changed, {} taken away",
        made.len(),
        moved,
        remade.len() - moved,
        gone.len()
    ))
}

/// A vein's template and place.
fn vein_of(o: &ContentObject) -> (u32, StoredPosition) {
    (
        o.template,
        StoredPosition {
            cell: o.cell,
            origin: o.origin,
            rotation: o.rotation,
        },
    )
}

/// Take the database's veins: those the same keep their state; one taken away leaves the world
/// (or no longer comes back), a player mining it answered; a new one stands at once. A line saying
/// what changed, if anything did.
fn take_veins<S: Store>(
    s: &mut Server<S>,
    net: &mut ServerNet,
    new: Vec<ContentObject>,
) -> Option<String> {
    let mut gone: Vec<(u32, StoredPosition)> = s.placed.veins.iter().map(vein_of).collect();
    let mut made = Vec::new();
    for v in new.iter().map(vein_of) {
        match gone.iter().position(|g| *g == v) {
            Some(i) => {
                gone.remove(i);
            }
            None => made.push(v),
        }
    }
    s.placed.veins = new;
    if gone.is_empty() && made.is_empty() {
        return None;
    }
    for (template, at) in &gone {
        let standing = s
            .things
            .by_id
            .values()
            .find(|t| {
                t.template == *template
                    && t.at == Some(*at)
                    && t.node.as_ref().is_some_and(|n| n.public)
            })
            .map(|t| t.id);
        match standing {
            Some(id) => {
                crate::mining::release(s, net, id);
                crate::death::vanish(s, net, id);
            }
            None => {
                // Spent, and waiting to come back: it no longer does.
                if let Some(i) = s.timers.iter().position(|(_, t)| {
                    matches!(t, crate::Timer::VeinBack { template: x, at: y }
                        if x == template && y == at)
                }) {
                    s.timers.remove(i);
                }
            }
        }
    }
    for (template, at) in &made {
        if let Some(id) = crate::mining::vein(s, *template, *at) {
            presence::thing_appears(s, net, id);
        }
    }
    Some(format!(
        "veins: {} new, {} taken away",
        made.len(),
        gone.len()
    ))
}

/// Whoever uses an object of the world lets it go: a use waiting on a door or a chest is
/// answered, a chest open is closed for its opener, and the door or chest is made closed.
fn release<S: Store>(s: &mut Server<S>, net: &mut ServerNet, id: u64) {
    crate::usable::doors::release(s, net, id);
    crate::usable::chests::release(s, net, id);
}

/// May a camp's creatures stay its own across a reload that changed the camp's row?
fn fits(a: &Camp, b: &Camp) -> bool {
    a.cell == b.cell
        && a.origin == b.origin
        && a.radius == b.radius
        && a.templates == b.templates
        && a.spots == b.spots
        && a.moves == b.moves
}

/// Is a creature fighting a player (after one, or with one as its target)?
fn fighting(c: &crate::things::Creature) -> bool {
    c.mode == Mode::Chasing || c.target.is_some()
}

/// Take the database's camps. A line saying what changed, if anything did.
fn take_camps<S: Store>(
    s: &mut Server<S>,
    net: &mut ServerNet,
    clock: Clock,
    new: SpawnTable,
) -> Option<String> {
    if new == s.spawns {
        return None;
    }
    let now = clock.local.0;
    let old = std::mem::take(&mut s.spawns.camps);
    let mut old_states = std::mem::take(&mut s.camps);
    old_states.resize(old.len(), crate::spawn::CampState::default());
    let by_key: HashMap<&str, usize> = old
        .iter()
        .enumerate()
        .map(|(i, c)| (c.id.as_str(), i))
        .collect();
    let mut index = HashMap::new();
    let mut retire = Vec::new();
    let mut states = Vec::with_capacity(new.camps.len());
    let (mut started, mut changed) = (0, 0);
    for (k, c) in new.camps.iter().enumerate() {
        match by_key.get(c.id.as_str()) {
            Some(&i) if fits(&old[i], c) => {
                let mut st = std::mem::take(&mut old_states[i]);
                if old[i] != *c {
                    changed += 1;
                }
                let population = c.population as usize;
                if st.alive.len() > population {
                    // The extra retire, those at rest first.
                    let mut order = st.alive.clone();
                    order.sort_by_key(|id| {
                        s.things
                            .get(*id)
                            .and_then(|t| t.creature.as_ref())
                            .is_some_and(fighting)
                    });
                    let extra: Vec<u64> = order
                        .into_iter()
                        .take(st.alive.len() - population)
                        .collect();
                    st.alive.retain(|id| !extra.contains(id));
                    retire.extend(extra);
                }
                if st.filled {
                    // More are kept: the camp makes them at its next step.
                    let had = (old[i].population as usize).max(st.alive.len() + st.due.len());
                    for _ in had..population {
                        st.due.push(now);
                    }
                }
                index.insert(i, k);
                states.push(st);
            }
            Some(&i) => {
                changed += 1;
                retire.append(&mut old_states[i].alive);
                states.push(crate::spawn::CampState::default());
            }
            None => {
                started += 1;
                states.push(crate::spawn::CampState::default());
            }
        }
    }
    let new_keys: HashSet<&str> = new.camps.iter().map(|c| c.id.as_str()).collect();
    let mut removed = 0;
    for (i, c) in old.iter().enumerate() {
        if !new_keys.contains(c.id.as_str()) {
            removed += 1;
            retire.append(&mut old_states[i].alive);
        }
    }
    s.spawns = new;
    s.camps = states;
    // Every creature of a camp follows its camp to its new place in the table, or has none.
    for t in s.things.by_id.values_mut() {
        if let Some(c) = t.creature.as_mut() {
            if let Some(i) = c.camp {
                c.camp = index.get(&i).copied();
            }
        }
    }
    let (mut left, mut fights) = (0, 0);
    for id in retire {
        match retire_creature(s, net, id) {
            Retired::Left => left += 1,
            Retired::Fighting => fights += 1,
            Retired::Dying => {}
        }
    }
    Some(format!(
        "camps: {started} new, {changed} changed, {removed} taken away; {left} creatures left \
         the world, {fights} leave once their fights are over"
    ))
}

/// What became of a retiring creature.
enum Retired {
    /// It left the world.
    Left,
    /// It leaves once its fight is over.
    Fighting,
    /// It is dying (or gone already), and goes as it would.
    Dying,
}

/// A camp's creature retires: no longer its camp's; at rest it leaves the world at once, fighting
/// it leaves once at rest again; one dying goes as it would.
fn retire_creature<S: Store>(s: &mut Server<S>, net: &mut ServerNet, id: u64) -> Retired {
    let Some(c) = s.things.get_mut(id).and_then(|t| t.creature.as_mut()) else {
        return Retired::Dying;
    };
    c.camp = None;
    if c.mode == Mode::Dead {
        return Retired::Dying;
    }
    if fighting(c) {
        s.placed.retiring.push(id);
        return Retired::Fighting;
    }
    crate::death::vanish(s, net, id);
    Retired::Left
}

/// The world's step: a retiring creature at rest again leaves the world.
pub fn retire_rested<S: Store>(s: &mut Server<S>, net: &mut ServerNet) {
    if s.placed.retiring.is_empty() {
        return;
    }
    let mut still = Vec::new();
    for id in std::mem::take(&mut s.placed.retiring) {
        match s.things.get(id).and_then(|t| t.creature.as_ref()) {
            None => {}
            Some(c) if c.mode == Mode::Dead => {}
            Some(c) if fighting(c) => still.push(id),
            Some(_) => crate::death::vanish(s, net, id),
        }
    }
    s.placed.retiring = still;
}
