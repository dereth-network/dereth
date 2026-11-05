//! The object database: records loaded by id, decoded once and kept, found through the master
//! id map's enumerations, and the enum maps' names with their base maps beneath them.
//!
//! The data files are looked in in order; the first that holds a record is the one read. The
//! master id map (named by the first file's header) maps an enumeration *mapping* to an id map,
//! and that id map an enumeration *value* to a record: the property list is value `0x40000003` of
//! mapping `0x40000002`, the enum maps are found through mapping `0x40000001`, the string tables'
//! language through mapping `0x40000004`.
//!
//! An enumeration names its values either by an enum map (`0x28...`, whose base map's names it
//! extends) or by an id map (`0x2A...`, its name tables): every name lookup answers from whichever
//! the enumeration's record is.
//!
//! Records are handed out as [`Obj`] handles and kept in per-type caches with free lists
//! ([`crate::cache`]); the caches' clock moves only when the host gives them time
//! ([`Database::use_time`]).

use std::any::{Any, TypeId};
use std::sync::{Arc, Mutex, PoisonError};

use auberean_dat::disk::StoreSet;
use auberean_dat::qualified::{divine_cell_type, divine_type};
use auberean_dat::{DatFile, QualifiedId, QualifiedIds, RecordType};

use crate::cache::{self, Obj, Shared};

use crate::entity::{Entity, EntityGroup};
use crate::fx::FxTable;
use crate::property::{Names, PropertyList, MASTER_PROPERTY_MAPPING, MASTER_PROPERTY_VALUE};
use crate::strings::{EnumMap, StringTable};
use crate::tables::IdMap;
use crate::visual::{Appearance, VisualDesc};
use crate::{decode, Context, Decode};

/// The mapping the enum maps are found through.
pub const ENUM_MAP_MAPPING: u32 = 0x4000_0001;

/// How many base maps deep an enum map's chain is followed (no retail chain is near this deep;
/// a loop in edited data must end).
const BASE_LIMIT: u32 = 32;

/// The gathering a record's needed records are collected for when it goes into a data graph.
pub const GATHER_DEPENDENCIES: u32 = 0x4000_0007;

/// What a world-building tool learns from the records a database loads: a node for each record
/// and an edge to each record it needs. The game installs none.
pub trait DataGraph: Send {
    fn add_node(&mut self, id: u32);
    fn add_edge(&mut self, from: u32, to: u32);
}

/// The types whose records never go into a data graph (beside every cell file's): the region's
/// parts, sounds, scenes and the like.
const NOT_GRAPHED: [u32; 24] = [
    0x4000_000B,
    0x4000_000C,
    0x4000_000D,
    0x4000_000E,
    0x4000_000F,
    0x4000_0010,
    0x4000_0011,
    0x4000_0012,
    0x4000_0013,
    0x4000_0014,
    0x4000_0015,
    0x4000_0016,
    0x4000_0019,
    0x4000_001D,
    0x4000_001F,
    0x4000_0028,
    0x4000_002A,
    0x4000_002F,
    0x4000_0036,
    0x4000_0042,
    0x4000_0046,
    0x4000_004F,
    0x4000_0054,
    0x4000_005D,
];

/// Whether a type's records go into a data graph: not a cell file's, not one of [`NOT_GRAPHED`].
#[must_use]
pub fn graphs_type(kind: u32) -> bool {
    !RecordType(kind).is_cell_type() && !NOT_GRAPHED.contains(&kind)
}

/// The records a loaded record needs, gathered as its type gathers them (none for a type that
/// names no other record).
pub fn gather_sub_ids(record: &dyn Any, kind: u32, out: &mut QualifiedIds) {
    if let Some(t) = record.downcast_ref::<StringTable>() {
        t.sub_ids(kind, out);
    } else if let Some(m) = record.downcast_ref::<IdMap>() {
        m.sub_ids(kind, out);
    } else if let Some(m) = record.downcast_ref::<crate::strings::EnumMap>() {
        m.sub_ids(out);
    } else if let Some(e) = record.downcast_ref::<Entity>() {
        e.sub_ids(out);
    } else if let Some(g) = record.downcast_ref::<EntityGroup>() {
        g.sub_ids(out);
    } else if let Some(l) = record.downcast_ref::<PropertyList>() {
        l.sub_ids(out);
    } else if let Some(v) = record.downcast_ref::<VisualDesc>() {
        v.sub_ids(kind, out);
    } else if let Some(a) = record.downcast_ref::<Appearance>() {
        a.sub_ids(kind, out);
    } else if let Some(t) = record.downcast_ref::<FxTable>() {
        t.sub_ids(kind, out);
    } else if let Some(q) = record.downcast_ref::<crate::script::Qualities>() {
        q.sub_ids(out);
    } else if let Some(s) = record.downcast_ref::<crate::sound::SoundInfo>() {
        s.sub_ids(out);
    } else if let Some(m) = record.downcast_ref::<crate::sound::MusicInfo>() {
        m.sub_ids(out);
    } else if !cell_sub_ids(record, out) {
        region_sub_ids(record, out);
    }
}

/// The records a cell file's record names (a landblock, its info, an interior cell); whether it
/// was one.
fn cell_sub_ids(record: &dyn Any, out: &mut QualifiedIds) -> bool {
    use crate::landblock::{Cell, Landblock, LandblockInfo};
    if let Some(l) = record.downcast_ref::<Landblock>() {
        l.sub_ids(out);
    } else if let Some(i) = record.downcast_ref::<LandblockInfo>() {
        i.sub_ids(out);
    } else if let Some(c) = record.downcast_ref::<Cell>() {
        c.sub_ids(out);
    } else {
        return false;
    }
    true
}

/// The records a region record or a scene names.
fn region_sub_ids(record: &dyn Any, out: &mut QualifiedIds) {
    use crate::region::{
        DayDesc, DetailMap, EncounterDesc, FogDesc, MapNotes, PropertyDesc, Region, SceneDesc,
        SkyDesc, SoundDesc, SurfaceDesc, TerrainDesc, WaterDesc,
    };
    if let Some(r) = record.downcast_ref::<Region>() {
        r.sub_ids(out);
    } else if let Some(r) = record.downcast_ref::<SoundDesc>() {
        r.sub_ids(out);
    } else if let Some(r) = record.downcast_ref::<SceneDesc>() {
        r.sub_ids(out);
    } else if let Some(r) = record.downcast_ref::<TerrainDesc>() {
        r.sub_ids(out);
    } else if let Some(r) = record.downcast_ref::<SurfaceDesc>() {
        r.sub_ids(out);
    } else if let Some(r) = record.downcast_ref::<EncounterDesc>() {
        r.sub_ids(out);
    } else if let Some(r) = record.downcast_ref::<SkyDesc>() {
        r.sub_ids(out);
    } else if let Some(r) = record.downcast_ref::<DayDesc>() {
        r.sub_ids(out);
    } else if let Some(r) = record.downcast_ref::<PropertyDesc>() {
        r.sub_ids(out);
    } else if let Some(r) = record.downcast_ref::<WaterDesc>() {
        r.sub_ids(out);
    } else if let Some(r) = record.downcast_ref::<FogDesc>() {
        r.sub_ids(out);
    } else if let Some(r) = record.downcast_ref::<DetailMap>() {
        r.sub_ids(out);
    } else if let Some(r) = record.downcast_ref::<MapNotes>() {
        r.sub_ids(out);
    } else if let Some(r) = record.downcast_ref::<crate::visual::Scene>() {
        r.sub_ids(out);
    }
}

/// Records loaded from data files, kept in per-type caches.
// D2-12628, D2-12627, D2-12613, D2-12614, D2-12626, D2-12636
pub struct Database {
    files: Vec<Arc<DatFile>>,
    /// The files as the data cache stores into them, when it does: records are read through them,
    /// so a record stored for the session is read as the files' own are.
    stores: Option<Arc<StoreSet>>,
    master_map: u32,
    ctx: Context,
    caches: Shared,
    client: bool,
    server: bool,
    graph: Mutex<Option<Box<dyn DataGraph>>>,
}

impl core::fmt::Debug for Database {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        f.debug_struct("Database")
            .field("files", &self.files.len())
            .field("master_map", &format_args!("{:08X}", self.master_map))
            .finish_non_exhaustive()
    }
}

impl Database {
    /// A database over these files, looked in in this order. The master id map is the first
    /// file's; the property list (which decoding properties needs) is found through it.
    // D2-12599, D2-12639, D2-12616
    #[must_use]
    pub fn new(files: Vec<Arc<DatFile>>) -> Self {
        Self::make(files, None)
    }

    /// A database over these files as the data cache stores into them: every record is read
    /// from the file its type belongs to, as stored (a record stored there for the session is
    /// read as the file's own are); a record no store holds is looked for in `files`, in order.
    #[must_use]
    pub fn over_stores(files: Vec<Arc<DatFile>>, stores: Arc<StoreSet>) -> Self {
        Self::make(files, Some(stores))
    }

    fn make(files: Vec<Arc<DatFile>>, stores: Option<Arc<StoreSet>>) -> Self {
        // D2-05125
        let master_map = files.first().map_or(0, |f| f.header().master_map_id);
        let mut db = Self {
            files,
            stores,
            master_map,
            ctx: Context::empty(),
            caches: Arc::new(Mutex::new(cache::Caches::default())),
            client: true,
            server: false,
            graph: Mutex::new(None),
        };
        if let Some(list) =
            db.get_by_enum::<PropertyList>(MASTER_PROPERTY_VALUE, MASTER_PROPERTY_MAPPING)
        {
            db.ctx = Context::with_properties(PropertyList::clone(&list));
        }
        db
    }

    /// The record type of an id: by the id chain, else (a cell file's record) by its low word.
    #[must_use]
    pub fn kind_of(id: u32) -> u32 {
        match divine_type(id) {
            #[allow(clippy::cast_possible_truncation)]
            0 => divine_cell_type(id as u16),
            k => k,
        }
    }

    /// Say whether the database serves a client and a server.
    pub fn set_roles(&mut self, client: bool, server: bool) {
        self.client = client;
        self.server = server;
    }

    /// Give the caches time: `now` is the host's clock in seconds; past a type's ideal number of
    /// free records, its oldest freed more than 30 seconds before goes.
    pub fn use_time(&self, now: f64) {
        cache::use_time(&self.caches, now);
    }

    /// Throw away the free records of one type, or of every type.
    pub fn flush_free_objects(&self, kind: Option<u32>) {
        cache::flush_free(&self.caches, kind);
    }

    /// Whether one type's cache (`Some`; `false` when the type has none yet) or every type's
    /// keeps free records.
    // D2-12635
    pub fn keep_free_objects(&self, kind: Option<u32>, keep: bool) -> bool {
        cache::keep_free_objects(&self.caches, kind, keep)
    }

    /// Set how many free records of a type are kept past their time.
    pub fn set_free_list_ideal(&self, kind: u32, ideal: usize) {
        cache::set_free_list_ideal(&self.caches, kind, ideal);
    }

    /// Throw away the free cell records; `true` when no cell record is left loaded.
    #[must_use]
    pub fn unload_cell_data(&self) -> bool {
        cache::unload_cell_data(&self.caches)
    }

    /// How many records of a type are loaded, and how many of those nobody holds.
    #[must_use]
    pub fn cached(&self, kind: u32) -> (usize, usize) {
        cache::counts(&self.caches, kind)
    }

    /// Throw away every cache (handles still out keep their records).
    pub fn destroy_object_caches(&self) {
        cache::destroy_all(&self.caches);
    }

    /// Whether whoever uses the database asks for records only as it needs them, never ahead in
    /// the background (the cell, sound and string-download code then skip their background
    /// requests). The client's database never says so.
    // D2-00035
    #[must_use]
    pub fn blocking(&self) -> bool {
        false
    }

    /// Save a record to the files: the client's caches cannot, so nothing is saved.
    // D2-12690
    #[must_use]
    pub fn save_to_disk(&self, _id: u32) -> bool {
        false
    }

    /// Ask every type's cache for its last words before the database goes; none has any.
    // D2-12619
    pub fn ask_for_last_words(&self) {}

    /// Install a data graph (or take it away): every record loaded from then on goes into it.
    pub fn set_data_graph(&self, graph: Option<Box<dyn DataGraph>>) {
        *self.graph.lock().unwrap_or_else(PoisonError::into_inner) = graph;
    }

    /// Take the data graph out (none installed after).
    pub fn take_data_graph(&self) -> Option<Box<dyn DataGraph>> {
        self.graph
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .take()
    }

    /// Put a category record into a data graph: an edge from it to the categories' enum map (no
    /// node of its own). Category records are of a type never graphed as they load, so only a
    /// caller filling a graph itself reaches this.
    // D2-12667
    pub fn fill_categories_graph(&self, id: u32, graph: &mut dyn DataGraph) {
        graph.add_edge(id, self.data_id_by_enum(0x4000_0030, ENUM_MAP_MAPPING));
    }

    /// Put a record just loaded into the data graph, when one is installed and its type goes
    /// there: a node for it and an edge to each record it needs.
    // D2-12610, D2-12686, D2-12703
    pub(crate) fn add_to_data_graph(&self, id: u32, kind: u32, record: &dyn Any) {
        if id == 0 || !graphs_type(kind) {
            return;
        }
        if self
            .graph
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .is_none()
        {
            return;
        }
        let mut needs = QualifiedIds::new();
        gather_sub_ids(record, GATHER_DEPENDENCIES, &mut needs);
        if let Some(g) = self
            .graph
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .as_mut()
        {
            g.add_node(id);
            for q in needs.iter() {
                g.add_edge(id, q.id);
            }
        }
    }

    /// The decoding context: the install's property list.
    #[must_use]
    pub fn context(&self) -> &Context {
        &self.ctx
    }

    /// The master id map's record id.
    #[must_use]
    pub fn master_map(&self) -> u32 {
        self.master_map
    }

    /// A record's bytes: from the file its type belongs to as stored, when the database is over
    /// the cache's stores and that file holds it; else from the first file that holds it.
    #[must_use]
    pub fn read(&self, id: u32) -> Option<Vec<u8>> {
        if let Some(s) = &self.stores {
            let q = QualifiedId {
                id,
                kind: Self::kind_of(id),
            };
            if let Some(b) = s.load_data(q) {
                return Some(b);
            }
        }
        self.files.iter().find_map(|f| f.read(id).ok())
    }

    /// The stores records are read through, when the database is over them.
    #[must_use]
    pub fn stores(&self) -> Option<&Arc<StoreSet>> {
        self.stores.as_ref()
    }

    /// A record decoded as `T`: from memory when it is loaded (taken off its free list if
    /// nobody held it), else read, decoded and kept; `None` for id 0, a record no file holds, or
    /// one that does not decode as `T`.
    // D2-12694, D2-12600, D2-12701, D2-12702
    pub fn get<T: Decode + Send + Sync + 'static>(&self, id: u32) -> Option<Obj<T>> {
        self.get_typed(id, Self::kind_of(id))
    }

    /// [`Database::get`] for a record of a given type.
    // D2-03304, D2-03650, D2-03651, D2-03713, D2-03714, D2-05648, D2-06168, D2-06893, D2-07832, D2-07988, D2-10366, D2-11015
    pub fn get_typed<T: Decode + Send + Sync + 'static>(
        &self,
        id: u32,
        kind: u32,
    ) -> Option<Obj<T>> {
        if id == 0 {
            return None;
        }
        // A record got here is kept with its named records counted as loaded: a request for it
        // later asks for none of them.
        let key = (id, TypeId::of::<T>());
        if let Some(o) = cache::in_memory::<T>(&self.caches, kind, id) {
            cache::set_subs_loaded(&self.caches, kind, key);
            return Some(o);
        }
        cache::get_free_obj(&self.caches, kind);
        let v = Arc::new(self.load::<T>(id)?);
        let o = cache::insert(&self.caches, kind, id, Arc::clone(&v));
        cache::set_subs_loaded(&self.caches, kind, key);
        self.add_to_data_graph(id, kind, &*v);
        Some(o)
    }

    /// The caches, for the requests made through them.
    pub(crate) fn caches(&self) -> &Shared {
        &self.caches
    }

    fn load<T: Decode>(&self, id: u32) -> Option<T> {
        self.read(id)
            .and_then(|b| decode::<T>(id, &b, &self.ctx).ok())
    }

    /// A record decoded as `T` when it is already loaded; never reads a file.
    // D2-12630, D2-12765
    pub fn get_if_loaded<T: Decode + Send + Sync + 'static>(&self, id: u32) -> Option<Obj<T>> {
        cache::in_memory::<T>(&self.caches, Self::kind_of(id), id)
    }

    /// A record decoded as `T` when somebody holds it.
    pub fn get_if_using<T: Decode + Send + Sync + 'static>(&self, id: u32) -> Option<Obj<T>> {
        cache::in_use::<T>(&self.caches, Self::kind_of(id), id)
    }

    /// Put a record made in memory into its type's cache, as if read; `None` when a record of its
    /// id is already loaded.
    // D2-12698
    pub fn put<T: Send + Sync + 'static>(&self, id: u32, record: T) -> Option<Obj<T>> {
        let kind = Self::kind_of(id);
        if cache::loaded(&self.caches, kind).contains(&(id, TypeId::of::<T>())) {
            return None;
        }
        let v = Arc::new(record);
        let o = cache::insert(&self.caches, kind, id, Arc::clone(&v));
        self.add_to_data_graph(id, kind, &*v);
        Some(o)
    }

    /// Throw a loaded record away, held or not.
    pub fn remove<T: Send + Sync + 'static>(&self, id: u32) -> bool {
        cache::remove(&self.caches, Self::kind_of(id), id, TypeId::of::<T>())
    }

    /// Read a loaded record again from the files; its holders keep what they had, later askers
    /// get the new one. `false` when it is not loaded or no longer reads.
    pub fn reload<T: Decode + Send + Sync + 'static>(&self, id: u32) -> bool {
        let kind = Self::kind_of(id);
        if !cache::loaded(&self.caches, kind).contains(&(id, TypeId::of::<T>())) {
            return false;
        }
        match self.load::<T>(id) {
            Some(v) => cache::replace(&self.caches, kind, id, Arc::new(v)),
            None => false,
        }
    }

    /// The record id an enumeration value maps to (0 for none): the master id map names the
    /// mapping's id map, and that map names the record.
    // D2-12602, D2-12601, D2-12696, D2-12713
    pub fn data_id_by_enum(&self, value: u32, mapping: u32) -> u32 {
        if self.master_map == 0 {
            return 0;
        }
        let Some(master) = self.get::<IdMap>(self.master_map) else {
            return 0;
        };
        let sub = id_map_lookup(&master, mapping);
        self.get::<IdMap>(sub)
            .map_or(0, |m| id_map_lookup(&m, value))
    }

    /// The record an enumeration value maps to, decoded as `T`.
    // D2-12700, D2-12603
    pub fn get_by_enum<T: Decode + Send + Sync + 'static>(
        &self,
        value: u32,
        mapping: u32,
    ) -> Option<Obj<T>> {
        self.get(self.data_id_by_enum(value, mapping))
    }

    /// The install's property list, found through its enumeration.
    #[must_use]
    pub fn property_list(&self) -> Option<&PropertyList> {
        self.ctx.properties()
    }

    /// An entity record.
    #[must_use]
    pub fn entity(&self, id: u32) -> Option<Arc<Entity>> {
        (divine_type(id) == RecordType::ENTITY.0)
            .then(|| self.get::<Entity>(id).map(|o| o.arc()))
            .flatten()
    }

    // ------------------------------------------------------------------ enum maps

    /// An enum map record.
    // D2-03389, D2-14327, D2-14328
    #[must_use]
    pub fn enum_map(&self, id: u32) -> Option<Obj<EnumMap>> {
        self.get::<EnumMap>(id)
    }

    /// An enum map and its base maps, the map itself first.
    fn chain(&self, id: u32) -> Vec<Obj<EnumMap>> {
        let mut out = Vec::new();
        let mut next = id;
        for _ in 0..BASE_LIMIT {
            let Some(m) = self.enum_map(next) else { break };
            next = m.base;
            out.push(m);
            if next == 0 {
                break;
            }
        }
        out
    }

    /// The name of a value in an enum map, or in its base maps.
    // D2-14339, D2-14340
    #[must_use]
    pub fn enum_map_name(&self, map: u32, value: u32) -> Option<String> {
        self.chain(map).iter().find_map(|m| {
            sorted(m)
                .into_iter()
                .find(|(v, _)| *v == value)
                .map(|(_, n)| n)
        })
    }

    /// The value of a name in an enum map, or in its base maps (letter case ignored; in one map
    /// the greatest value of a name wins).
    // D2-14349, D2-14350, D2-14331
    #[must_use]
    pub fn enum_map_value(&self, map: u32, name: &str) -> Option<u32> {
        self.chain(map).iter().find_map(|m| m.enum_of(name))
    }

    /// Every value and name of an enum map in the game's order: the map's own by value, then its
    /// base map's, and so on.
    // D2-14346, D2-14337, D2-14338, D2-14347, D2-14348, D2-14329, D2-14333
    #[must_use]
    pub fn enum_map_entries(&self, map: u32) -> Vec<(u32, String)> {
        self.chain(map).iter().flat_map(|m| sorted(m)).collect()
    }

    /// How many names an enum map and its base maps hold.
    // D2-14332
    #[must_use]
    pub fn enum_map_len(&self, map: u32) -> usize {
        self.chain(map).iter().map(|m| sorted(m).len()).sum()
    }

    // ------------------------------------------------------------------ enumerations

    /// The record an enumeration's names are in (an enum map or an id map), through the enum
    /// maps' mapping.
    fn enumeration(&self, enumeration: u32) -> (u32, u32) {
        let did = self.data_id_by_enum(enumeration, ENUM_MAP_MAPPING);
        (did, divine_type(did))
    }

    /// The name of a value of an enumeration.
    // D2-14341, D2-14342, D2-14343, D2-14345, D2-12714
    #[must_use]
    pub fn enumeration_name(&self, enumeration: u32, value: u32) -> Option<String> {
        let (did, _) = self.enumeration(enumeration);
        self.name_by_did(did, value)
    }

    /// The name of a value in an enum map or id map record.
    // D2-14344
    #[must_use]
    pub fn name_by_did(&self, did: u32, value: u32) -> Option<String> {
        match divine_type(did) {
            t if t == RecordType::ENUM_MAP.0 => self.enum_map_name(did, value),
            t if t == RecordType::ID_MAP.0 => {
                let m = self.get::<IdMap>(did)?;
                m.names
                    .iter()
                    .chain(m.tool_names.iter())
                    .find(|(k, _)| *k == value)
                    .map(|(_, n)| n.clone())
            }
            _ => None,
        }
    }

    /// The value of a name of an enumeration (letter case ignored).
    // D2-14351, D2-14352
    #[must_use]
    pub fn enumeration_value(&self, enumeration: u32, name: &str) -> Option<u32> {
        let (did, _) = self.enumeration(enumeration);
        self.value_by_did(did, name)
    }

    /// The value of a name in an enum map or id map record (letter case ignored; in an id map,
    /// the first name in its stored order, the game's own names before the tools').
    // D2-14353, D2-14354, D2-12710
    #[must_use]
    pub fn value_by_did(&self, did: u32, name: &str) -> Option<u32> {
        match divine_type(did) {
            t if t == RecordType::ENUM_MAP.0 => self.enum_map_value(did, name),
            t if t == RecordType::ID_MAP.0 => {
                let m = self.get::<IdMap>(did)?;
                m.names
                    .iter()
                    .chain(m.tool_names.iter())
                    .find(|(_, n)| n.eq_ignore_ascii_case(name))
                    .map(|(k, _)| *k)
            }
            _ => None,
        }
    }

    /// Every value of an enumeration: an enum map's in its order, an id map's game record ids'
    /// values in stored order.
    // D2-14363, D2-14364, D2-12716
    #[must_use]
    pub fn enumeration_values(&self, enumeration: u32) -> Vec<u32> {
        let (did, ty) = self.enumeration(enumeration);
        if ty == RecordType::ENUM_MAP.0 {
            self.enum_map_entries(did)
                .into_iter()
                .map(|(v, _)| v)
                .collect()
        } else if ty == RecordType::ID_MAP.0 {
            self.get::<IdMap>(did)
                .map(|m| m.ids.iter().map(|(k, _)| *k).collect())
                .unwrap_or_default()
        } else {
            Vec::new()
        }
    }

    /// Every name of an enumeration: an enum map's in its order, an id map's game names in stored
    /// order.
    // D2-14367, D2-14368, D2-12720
    #[must_use]
    pub fn enumeration_names(&self, enumeration: u32) -> Vec<String> {
        let (did, ty) = self.enumeration(enumeration);
        if ty == RecordType::ENUM_MAP.0 {
            self.enum_map_entries(did)
                .into_iter()
                .map(|(_, n)| n)
                .collect()
        } else if ty == RecordType::ID_MAP.0 {
            self.get::<IdMap>(did)
                .map(|m| m.names.iter().map(|(_, n)| n.clone()).collect())
                .unwrap_or_default()
        } else {
            Vec::new()
        }
    }

    /// How many values an enumeration has: an enum map's names with its bases', an id map's game
    /// record ids.
    // D2-14336
    #[must_use]
    pub fn enumeration_len(&self, enumeration: u32) -> usize {
        let (did, ty) = self.enumeration(enumeration);
        if ty == RecordType::ENUM_MAP.0 {
            self.enum_map_len(did)
        } else if ty == RecordType::ID_MAP.0 {
            self.get::<IdMap>(did).map_or(0, |m| m.ids.len())
        } else {
            0
        }
    }
}

/// An id map's record for a value: its game half first, then its tools' half (0 for none).
// D2-12713
fn id_map_lookup(m: &IdMap, value: u32) -> u32 {
    m.ids
        .iter()
        .chain(m.tool_ids.iter())
        .find(|(k, _)| *k == value)
        .map_or(0, |(_, d)| d.0)
}

/// An enum map's own names as the game holds them: one per value (the first stored), by value.
fn sorted(m: &EnumMap) -> Vec<(u32, String)> {
    let mut v: Vec<(u32, String)> = Vec::with_capacity(m.names.len());
    for (k, n) in &m.names {
        if !v.iter().any(|(x, _)| x == k) {
            v.push((*k, n.clone()));
        }
    }
    v.sort_by_key(|(k, _)| *k);
    v
}

impl Names for Database {
    fn enum_name(&self, map: u32, value: u32) -> Option<String> {
        self.name_by_did(map, value)
    }

    fn enumeration_name(&self, enumeration: u32, value: u32) -> Option<String> {
        Database::enumeration_name(self, enumeration, value)
    }
}
