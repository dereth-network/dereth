// Ported from ACE (ACEmulator), AGPL-3.0: Source/ACE.Server/Managers/PropertyManager.cs
//! Port of `Source/ACE.Server/Managers/PropertyManager.cs`: the server properties (`PropertyManager`),
//! their defaults (`DefaultPropertyManager`), `Property<T>` and `ConfigurationEntry<T>`.
//!
//! ACE's static class becomes [`PropertyManagerState`] (`w.property_manager`):
//!
//! - **The caches.** ACE's four `ConcurrentDictionary<string, ConfigurationEntry<T>>` are ordinal
//!   (case-sensitive) maps behind one mutex, so the typed getters take `&World` as ACE's static
//!   getters are callable from anywhere. The iteration order of ACE's concurrent dictionaries is
//!   not observable (the writes are keyed upserts), so they are `BTreeMap`s.
//! - **`DatabaseManager.ShardConfig`** is only ever used by this class in ACE, so its handle lives
//!   here ([`PropertyManagerState::shard_config`]). ACE's `ShardConfigDatabase` opens its own
//!   context per call; the server binary gives it its own connection to `shard.db`
//!   ([`install_shard_config`]). `World::new` gives it an empty in-memory shard.
//! - **The `_workerThread` timer** (a `System.Timers.Timer` of 300 s that runs `DoWork` on a pool
//!   thread) is a deadline on the monotonic clock that the world loop checks each iteration
//!   ([`run_worker_timer`]); `DoWork` therefore runs on the world thread.
//! - **Before `Initialize`.** A new state already holds the defaults, as after
//!   `DefaultPropertyManager.LoadDefaultProperties()`, so that a world built without
//!   `Program.Main`'s start-up (a unit test) reads ACE's defaults rather than `GetBool`'s
//!   fallbacks. `Initialize` loads them again (the same values) and then the database.

use std::collections::BTreeMap;
use std::sync::{Arc, LazyLock, Mutex, MutexGuard};
use std::time::Duration;

use empyrean_common::dotnet::{format as dotnet_format, DotNetDict};
use empyrean_store::models::shard::{
    ConfigPropertiesBoolean, ConfigPropertiesDouble, ConfigPropertiesLong, ConfigPropertiesString,
};
use empyrean_store::{MemShard, ShardConfigDatabase};

use crate::factories::loot_generation_factory::tables_logic::cantrips::cantrip_chance;
use crate::World;

/// `DatabaseManager.ShardConfig`: the shard's `config_properties_*` tables.
pub type ShardConfigHandle = Arc<Mutex<Box<dyn ShardConfigDatabase + Send>>>;

/// A [`ShardConfigHandle`] over `db`.
#[must_use]
pub fn shard_config_handle(db: Box<dyn ShardConfigDatabase + Send>) -> ShardConfigHandle {
    Arc::new(Mutex::new(db))
}

/// The mutable static state of ACE's `PropertyManager`, held as a field of `World`.
pub struct PropertyManagerState {
    caches: Mutex<Caches>,
    /// `DatabaseManager.ShardConfig`.
    pub shard_config: ShardConfigHandle,
    // ACE: PropertyManager._workerThread
    worker_thread: Option<WorkerTimer>,
}

impl Default for PropertyManagerState {
    fn default() -> Self {
        let state = Self {
            caches: Mutex::new(Caches::default()),
            shard_config: shard_config_handle(Box::new(MemShard::new())),
            worker_thread: None,
        };
        default_property_manager::load_default_properties(&state);
        state
    }
}

impl std::fmt::Debug for PropertyManagerState {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let caches = lock(&self.caches);
        f.debug_struct("PropertyManagerState")
            .field("cached_boolean_settings", &caches.bools.len())
            .field("cached_long_settings", &caches.longs.len())
            .field("cached_double_settings", &caches.doubles.len())
            .field("cached_string_settings", &caches.strings.len())
            .field("worker_thread", &self.worker_thread)
            .finish_non_exhaustive()
    }
}

impl PropertyManagerState {
    fn caches(&self) -> MutexGuard<'_, Caches> {
        lock(&self.caches)
    }

    fn shard_config(&self) -> MutexGuard<'_, Box<dyn ShardConfigDatabase + Send>> {
        lock(&self.shard_config)
    }

    /// Not ACE: the cache entry for `key`, for tests and diagnostics (`None` when not cached).
    #[must_use]
    pub fn cached_bool(&self, key: &str) -> Option<ConfigurationEntry<bool>> {
        self.caches().bools.get(key).cloned()
    }

    /// Not ACE: see [`cached_bool`](Self::cached_bool).
    #[must_use]
    pub fn cached_long(&self, key: &str) -> Option<ConfigurationEntry<i64>> {
        self.caches().longs.get(key).cloned()
    }

    /// Not ACE: see [`cached_bool`](Self::cached_bool).
    #[must_use]
    pub fn cached_double(&self, key: &str) -> Option<ConfigurationEntry<f64>> {
        self.caches().doubles.get(key).cloned()
    }

    /// Not ACE: see [`cached_bool`](Self::cached_bool).
    #[must_use]
    pub fn cached_string(&self, key: &str) -> Option<ConfigurationEntry<String>> {
        self.caches().strings.get(key).cloned()
    }

    /// Not ACE: when the worker timer next fires (monotonic clock), or `None` when it is stopped
    /// or was never created.
    #[must_use]
    pub fn worker_timer_due(&self) -> Option<Duration> {
        self.worker_thread
            .as_ref()
            .filter(|t| t.enabled)
            .map(|t| t.due)
    }
}

fn lock<T: ?Sized>(m: &Mutex<T>) -> MutexGuard<'_, T> {
    m.lock().unwrap_or_else(std::sync::PoisonError::into_inner)
}

// ACE: PropertyManager.CachedBooleanSettings, PropertyManager.CachedLongSettings, PropertyManager.CachedDoubleSettings, PropertyManager.CachedStringSettings
/// caching internally to the server
#[derive(Debug, Default)]
struct Caches {
    bools: BTreeMap<String, ConfigurationEntry<bool>>,
    longs: BTreeMap<String, ConfigurationEntry<i64>>,
    doubles: BTreeMap<String, ConfigurationEntry<f64>>,
    strings: BTreeMap<String, ConfigurationEntry<String>>,
}

/// `new Timer(300000)` with `AutoReset = true`: while enabled, `Elapsed` is raised every
/// `interval`, starting `interval` after `Start()`.
#[derive(Debug, Clone, Copy)]
struct WorkerTimer {
    interval: Duration,
    enabled: bool,
    /// The next `Elapsed`, on the monotonic clock (`w.now.monotonic`).
    due: Duration,
}

impl WorkerTimer {
    fn start(&mut self, now: Duration) {
        self.enabled = true;
        self.due = now + self.interval;
    }

    fn stop(&mut self) {
        self.enabled = false;
    }
}

/// The worker timer's interval, `new Timer(300000)` (milliseconds).
pub const WORKER_INTERVAL: Duration = Duration::from_millis(300_000);

// ACE: PropertyManager.Initialize
/// Initializes the PropertyManager. Run this only once per server instance.
///
/// `load_default_values`: should we use the DefaultPropertyManager to load the default properties
/// for keys?
///
/// DIVERGE: ACE forces `content_folder` to `/ace/Content` on every start inside a container
/// (`DOTNET_RUNNING_IN_CONTAINER`); Empyrean has no container special case, so a container sets
/// the folder like any other install (brand).
pub fn initialize(w: &mut World, load_default_values: bool) {
    if load_default_values {
        default_property_manager::load_default_properties(&w.property_manager);
        // DIVERGE: the era's own defaults (`EraRules::property_defaults`, ClassicACE's
        // per-ruleset overrides after the defaults); a value in the database still wins.
        for (key, value) in w.era.property_defaults {
            default_property_manager::modify_bool(&w.property_manager, key, *value);
        }
    }

    load_properties_from_db(w);

    let mut timer = WorkerTimer {
        interval: WORKER_INTERVAL,
        enabled: false,
        due: Duration::ZERO,
    };
    timer.start(w.now.monotonic);
    w.property_manager.worker_thread = Some(timer);
}

/// Not ACE: installs `DatabaseManager.ShardConfig` (the host's, before [`initialize`]).
pub fn install_shard_config(w: &mut World, shard_config: ShardConfigHandle) {
    w.property_manager.shard_config = shard_config;
}

// ACE: PropertyManager.LoadPropertiesFromDB
/// Loads the variables from the database directly into the cache.
fn load_properties_from_db(w: &World) {
    let pm = &w.property_manager;
    let (bools, longs, doubles, strings) = {
        let mut db = pm.shard_config();
        (
            db.get_all_bools(),
            db.get_all_longs(),
            db.get_all_doubles(),
            db.get_all_strings(),
        )
    };
    let mut caches = pm.caches();

    for i in bools {
        caches.bools.insert(
            i.key,
            ConfigurationEntry::with_description(false, i.value, i.description),
        );
    }

    for i in longs {
        caches.longs.insert(
            i.key,
            ConfigurationEntry::with_description(false, i.value, i.description),
        );
    }

    for i in doubles {
        caches.doubles.insert(
            i.key,
            ConfigurationEntry::with_description(false, i.value, i.description),
        );
    }

    for i in strings {
        caches.strings.insert(
            i.key,
            ConfigurationEntry::with_description(false, i.value, i.description),
        );
    }
}

// ACE: PropertyManager.ResyncVariables
/// Resyncs the variables with the database manually. Disables the timer so that the elapsed event
/// cannot run during the update operation.
///
/// DIVERGE: before `Initialize` ACE's `_workerThread` is null and this throws; here `DoWork` runs
/// and the (absent) timer stays absent, so a world built without start-up can still shut down.
pub fn resync_variables(w: &mut World) {
    if let Some(t) = w.property_manager.worker_thread.as_mut() {
        t.stop();
    }

    do_work(w);

    let now = w.now.monotonic;
    if let Some(t) = w.property_manager.worker_thread.as_mut() {
        t.start(now);
    }
}

// ACE: PropertyManager.StopUpdating
/// Stops updating the cached store from the database.
pub fn stop_updating(w: &mut World) {
    if let Some(t) = w.property_manager.worker_thread.as_mut() {
        t.stop();
    }
}

/// Not ACE: the worker timer's `Elapsed` event. The world loop calls this each iteration; when the
/// timer is enabled and due, `DoWork` runs and the next `Elapsed` is one interval later (the
/// period is kept, as `AutoReset` does).
pub fn run_worker_timer(w: &mut World) {
    let now = w.now.monotonic;
    let Some(t) = w.property_manager.worker_thread.as_mut() else {
        return;
    };
    if !t.enabled || now < t.due {
        return;
    }
    t.due += t.interval;
    do_work(w);
}

// ACE: PropertyManager.GetBool
/// Retrieves a boolean property from the cache or database.
///
/// `fallback`: the value to return if the property cannot be found (ACE's default: `false`).
/// `cache_fallback`: whether or not the fallback property should be cached (ACE's default: `true`).
pub fn get_bool(w: &World, key: &str, fallback: bool, cache_fallback: bool) -> Property<bool> {
    let pm = &w.property_manager;
    // first, check the cache. If the key exists in the cache, grab it regardless of its modified value
    // then, check the database. if the key exists in the database, grab it and cache it
    // finally, set it to a default of false.
    if let Some(e) = pm.caches().bools.get(key) {
        return Property::new(e.item, e.description.clone());
    }

    let db_value = pm.shard_config().get_bool(key);

    let use_fallback = db_value.is_none();

    let value = db_value.as_ref().map_or(fallback, |v| v.value);
    let description = db_value.and_then(|v| v.description);

    if !use_fallback || cache_fallback {
        pm.caches().bools.insert(
            key.to_owned(),
            ConfigurationEntry::with_description(use_fallback, value, description.clone()),
        );
    }

    Property::new(value, description)
}

// ACE: PropertyManager.ModifyBool
/// Modifies a boolean value in the cache and marks it for being synced on the next cycle. Returns
/// true if the property was modified, false if no property exists with the given key.
pub fn modify_bool(w: &World, key: &str, new_val: bool) -> bool {
    default_property_manager::modify_bool(&w.property_manager, key, new_val)
}

// ACE: PropertyManager.ModifyBoolDescription
pub fn modify_bool_description(w: &World, key: &str, description: Option<&str>) {
    if let Some(e) = w.property_manager.caches().bools.get_mut(key) {
        e.modify_description(description.map(str::to_owned));
    } else {
        log::warn!("Attempted to modify {key} which did not exist in the BOOL cache.");
    }
}

// ACE: PropertyManager.GetLong
/// Retrieves an integer property from the cache or database (see [`get_bool`]; ACE's default
/// fallback is `0`).
pub fn get_long(w: &World, key: &str, fallback: i64, cache_fallback: bool) -> Property<i64> {
    let pm = &w.property_manager;
    if let Some(e) = pm.caches().longs.get(key) {
        return Property::new(e.item, e.description.clone());
    }

    let db_value = pm.shard_config().get_long(key);

    let use_fallback = db_value.is_none();

    let value = db_value.as_ref().map_or(fallback, |v| v.value);
    let description = db_value.and_then(|v| v.description);

    if !use_fallback || cache_fallback {
        pm.caches().longs.insert(
            key.to_owned(),
            ConfigurationEntry::with_description(use_fallback, value, description.clone()),
        );
    }

    Property::new(value, description)
}

// ACE: PropertyManager.ModifyLong
/// Modifies an integer value in the cache and marks it for being synced on the next cycle. Returns
/// true if the property was modified, false if no property exists with the given key.
pub fn modify_long(w: &World, key: &str, new_val: i64) -> bool {
    default_property_manager::modify_long(&w.property_manager, key, new_val)
}

// ACE: PropertyManager.ModifyLongDescription
pub fn modify_long_description(w: &World, key: &str, description: Option<&str>) {
    if let Some(e) = w.property_manager.caches().longs.get_mut(key) {
        e.modify_description(description.map(str::to_owned));
    } else {
        log::warn!("Attempted to modify {key} which did not exist in the LONG cache.");
    }
}

// ACE: PropertyManager.GetDouble
/// Retrieves a float property from the cache or database (see [`get_bool`]; ACE's default fallback
/// is `0.0f`).
pub fn get_double(w: &World, key: &str, fallback: f64, cache_fallback: bool) -> Property<f64> {
    let pm = &w.property_manager;
    if let Some(e) = pm.caches().doubles.get(key) {
        return Property::new(e.item, e.description.clone());
    }

    let db_value = pm.shard_config().get_double(key);

    let use_fallback = db_value.is_none();

    let value = db_value.as_ref().map_or(fallback, |v| v.value);
    let description = db_value.and_then(|v| v.description);

    if !use_fallback || cache_fallback {
        pm.caches().doubles.insert(
            key.to_owned(),
            ConfigurationEntry::with_description(use_fallback, value, description.clone()),
        );
    }

    Property::new(value, description)
}

// ACE: PropertyManager.ModifyDouble
/// Modifies a float value in the cache and marks it for being synced on the next cycle. Returns
/// true if the property was modified, false if no property exists with the given key. `init`
/// (ACE's default: `false`) skips the cantrip table rescaling.
pub fn modify_double(w: &World, key: &str, new_val: f64, init: bool) -> bool {
    if !default_property_manager::modify_double(&w.property_manager, key, new_val) {
        return false;
    }

    if !init {
        match key {
            "cantrip_drop_rate" => {
                cantrip_chance::apply_num_cantrips_mod(w, true);
            }
            "minor_cantrip_drop_rate"
            | "major_cantrip_drop_rate"
            | "epic_cantrip_drop_rate"
            | "legendary_cantrip_drop_rate" => {
                cantrip_chance::apply_cantrip_levels_mod(w, true);
            }
            _ => {}
        }
    }
    true
}

/// The cache part of `ModifyDouble` over the state (also what `LoadDefaultProperties` calls, as
/// `ModifyDouble(key, value, init: true)`); [`modify_double`] adds the cantrip-table rescaling,
/// which needs the world.
fn modify_double_in(pm: &PropertyManagerState, key: &str, new_val: f64) -> bool {
    let Some(default) = default_property_manager::DEFAULT_DOUBLE_PROPERTIES.get(key) else {
        return false;
    };
    {
        let mut caches = pm.caches();
        if let Some(e) = caches.doubles.get_mut(key) {
            e.modify(new_val);
        } else {
            caches.doubles.insert(
                key.to_owned(),
                ConfigurationEntry::with_description(true, new_val, default.description.clone()),
            );
        }
    }
    true
}

// ACE: PropertyManager.ModifyDoubleDescription
pub fn modify_double_description(w: &World, key: &str, description: Option<&str>) {
    if let Some(e) = w.property_manager.caches().doubles.get_mut(key) {
        e.modify_description(description.map(str::to_owned));
    } else {
        log::warn!(
            "Attempted to modify the description of {key} which did not exist in the DOUBLE cache."
        );
    }
}

// ACE: PropertyManager.GetString
/// Retrieves a string property from the cache or database (see [`get_bool`]; ACE's default
/// fallback is `""`).
pub fn get_string(w: &World, key: &str, fallback: &str, cache_fallback: bool) -> Property<String> {
    let pm = &w.property_manager;
    if let Some(e) = pm.caches().strings.get(key) {
        return Property::new(e.item.clone(), e.description.clone());
    }

    let db_value = pm.shard_config().get_string(key);

    let use_fallback = db_value.is_none();

    let (value, description) = match db_value {
        Some(v) => (v.value, v.description),
        None => (fallback.to_owned(), None),
    };

    if !use_fallback || cache_fallback {
        pm.caches().strings.insert(
            key.to_owned(),
            ConfigurationEntry::with_description(use_fallback, value.clone(), description.clone()),
        );
    }

    Property::new(value, description)
}

// ACE: PropertyManager.ModifyString
/// Modifies a string value in the cache and marks it for being synced on the next cycle. Returns
/// true if the property was modified, false if no property exists with the given key.
pub fn modify_string(w: &World, key: &str, new_val: &str) -> bool {
    default_property_manager::modify_string(&w.property_manager, key, new_val)
}

// ACE: PropertyManager.ModifyStringDescription
pub fn modify_string_description(w: &World, key: &str, description: Option<&str>) {
    if let Some(e) = w.property_manager.caches().strings.get_mut(key) {
        e.modify_description(description.map(str::to_owned));
    } else {
        log::warn!("Attempted to modify {key} which did not exist in the STRING cache.");
    }
}

/// The modified entries of one cache, as (key, item, description), collected before the database
/// calls (the cache lock is not held across them).
fn modified<T: Clone>(
    cache: &BTreeMap<String, ConfigurationEntry<T>>,
) -> Vec<(String, T, Option<String>)> {
    cache
        .iter()
        .filter(|(_, e)| e.modified)
        .map(|(k, e)| (k.clone(), e.item.clone(), e.description.clone()))
        .collect()
}

// ACE: PropertyManager.WriteBoolToDB
/// Writes all of the updated boolean values from the cache into the database.
fn write_bool_to_db(w: &World) {
    let pm = &w.property_manager;
    let entries = modified(&pm.caches().bools);
    let mut db = pm.shard_config();
    for (key, item, description) in entries {
        // this probably should be upsert. This does 2 queries per modified datapoint.
        // perhaps run a transaction to queue all the queries at once.
        if db.bool_exists(&key) {
            db.save_bool(&ConfigPropertiesBoolean {
                key,
                value: item,
                description,
            });
        } else {
            db.add_bool(&key, item, description.as_deref());
        }
    }
}

// ACE: PropertyManager.WriteLongToDB
/// Writes all of the updated integer values from the cache into the database.
fn write_long_to_db(w: &World) {
    let pm = &w.property_manager;
    let entries = modified(&pm.caches().longs);
    let mut db = pm.shard_config();
    for (key, item, description) in entries {
        // todo: see boolean section for caveat in this approach
        if db.long_exists(&key) {
            db.save_long(&ConfigPropertiesLong {
                key,
                value: item,
                description,
            });
        } else {
            db.add_long(&key, item, description.as_deref());
        }
    }
}

// ACE: PropertyManager.WriteDoubleToDB
/// Writes all of the updated float values from the cache into the database.
fn write_double_to_db(w: &World) {
    let pm = &w.property_manager;
    let entries = modified(&pm.caches().doubles);
    let mut db = pm.shard_config();
    for (key, item, description) in entries {
        // todo: see boolean section for caveat in this approach
        if db.double_exists(&key) {
            db.save_double(&ConfigPropertiesDouble {
                key,
                value: item,
                description,
            });
        } else {
            db.add_double(&key, item, description.as_deref());
        }
    }
}

// ACE: PropertyManager.WriteStringToDB
/// Writes all of the updated string values from the cache into the database.
fn write_string_to_db(w: &World) {
    let pm = &w.property_manager;
    let entries = modified(&pm.caches().strings);
    let mut db = pm.shard_config();
    for (key, item, description) in entries {
        // todo: see boolean section for caveat in this approach
        if db.string_exists(&key) {
            db.save_string(&ConfigPropertiesString {
                key,
                value: item,
                description,
            });
        } else {
            db.add_string(&key, &item, description.as_deref());
        }
    }
}

// ACE: PropertyManager.DoWork
/// DIVERGE: ACE logs how long the pass took; ported code does not read the OS clock, so the
/// debug line has no duration.
pub fn do_work(w: &World) {
    // first, check for variables updated on the server-side. Write those to the DB.
    // then, compare variables to DB and update from DB as necessary. (needs to minimize r/w)

    write_bool_to_db(w);
    write_long_to_db(w);
    write_double_to_db(w);
    write_string_to_db(w);

    // next, we need to fetch all of the variables from the DB and compare them quickly.
    load_properties_from_db(w);

    log::debug!("PropertyManager DoWork done");
}

// ACE: PropertyManager.ListProperties
#[must_use]
pub fn list_properties(w: &World) -> String {
    let mut props = "Boolean properties:\n".to_owned();
    for (key, value) in default_property_manager::DEFAULT_BOOLEAN_PROPERTIES.iter() {
        props += &format!(
            "\t{key}: {} (current is {}, default is {})\n",
            value.description.as_deref().unwrap_or_default(),
            bool_to_string(get_bool(w, key, false, true).item),
            bool_to_string(value.item)
        );
    }

    props += "\nLong properties:\n";
    for (key, value) in default_property_manager::DEFAULT_LONG_PROPERTIES.iter() {
        props += &format!(
            "\t{key}: {} (current is {}, default is {})\n",
            value.description.as_deref().unwrap_or_default(),
            dotnet_format::to_string(get_long(w, key, 0, true).item),
            dotnet_format::to_string(value.item)
        );
    }

    props += "\nDouble properties:\n";
    for (key, value) in default_property_manager::DEFAULT_DOUBLE_PROPERTIES.iter() {
        props += &format!(
            "\t{key}: {} (current is {}, default is {})\n",
            value.description.as_deref().unwrap_or_default(),
            dotnet_format::to_string(get_double(w, key, 0.0, true).item),
            dotnet_format::to_string(value.item)
        );
    }

    props += "\nString properties:\n";
    for (key, value) in default_property_manager::DEFAULT_STRING_PROPERTIES.iter() {
        props += &format!(
            "\t{key}: {} (default is hidden)\n",
            value.description.as_deref().unwrap_or_default()
        );
    }

    props
}

/// `bool.ToString()`.
fn bool_to_string(b: bool) -> &'static str {
    if b {
        "True"
    } else {
        "False"
    }
}

/// `T.ToString()` in `en-US`, for the four property types.
pub trait CsToString {
    /// `ToString()`.
    fn cs_to_string(&self) -> String;
}

impl CsToString for bool {
    fn cs_to_string(&self) -> String {
        bool_to_string(*self).to_owned()
    }
}

impl CsToString for i64 {
    fn cs_to_string(&self) -> String {
        dotnet_format::to_string(*self)
    }
}

impl CsToString for f64 {
    fn cs_to_string(&self) -> String {
        dotnet_format::to_string(*self)
    }
}

impl CsToString for String {
    fn cs_to_string(&self) -> String {
        self.clone()
    }
}

// ACE: Property
/// `Property<T>`: a property's value and its description.
#[derive(Debug, Clone, PartialEq)]
pub struct Property<T> {
    pub item: T,
    pub description: Option<String>,
}

impl<T> Property<T> {
    // ACE: Property.Property
    #[must_use]
    pub fn new(item: T, description: Option<String>) -> Self {
        Self { item, description }
    }
}

// ACE: ConfigurationEntry
/// `ConfigurationEntry<T>`: a cached property and whether it changed since the last database sync.
#[derive(Debug, Clone, PartialEq)]
pub struct ConfigurationEntry<T> {
    pub modified: bool,
    pub item: T,
    pub description: Option<String>,
}

impl<T> ConfigurationEntry<T> {
    // ACE: ConfigurationEntry.ConfigurationEntry
    #[must_use]
    pub fn new(modified: bool, item: T) -> Self {
        Self {
            modified,
            item,
            description: None,
        }
    }

    // ACE: ConfigurationEntry.ConfigurationEntry
    #[must_use]
    pub fn with_description(modified: bool, item: T, description: Option<String>) -> Self {
        Self {
            modified,
            item,
            description,
        }
    }

    // ACE: ConfigurationEntry.Modify
    pub fn modify(&mut self, item: T) {
        self.item = item;
        self.modified = true;
    }

    // ACE: ConfigurationEntry.ModifyDescription
    pub fn modify_description(&mut self, description: Option<String>) {
        self.description = description;
        self.modified = true;
    }
}

// ACE: ConfigurationEntry.ToString
impl<T: CsToString> std::fmt::Display for ConfigurationEntry<T> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(
            f,
            "{} {}",
            self.item.cs_to_string(),
            bool_to_string(self.modified)
        )
    }
}

/// `DefaultPropertyManager`: the properties every server starts with.
pub mod default_property_manager {
    use super::{
        lock, modify_double_in, ConfigurationEntry, DotNetDict, LazyLock, Property,
        PropertyManagerState,
    };

    // ACE: DefaultPropertyManager.DictOf
    /// `new ReadOnlyDictionary(pairs.ToDictionary(...))`: in the pairs' order; a repeated key
    /// throws (`ArgumentException`).
    fn dict_of<V: 'static, T>(
        pairs: &'static [(&'static str, V, &'static str)],
        item: impl Fn(&V) -> T,
    ) -> DotNetDict<&'static str, Property<T>> {
        let mut dict = DotNetDict::new();
        for (key, value, description) in pairs {
            dict.add(
                *key,
                Property::new(item(value), Some((*description).to_owned())),
            );
        }
        dict
    }

    // ACE: DefaultPropertyManager.LoadDefaultProperties
    /// Place any default properties to load in here.
    pub fn load_default_properties(pm: &PropertyManagerState) {
        //bool
        for (key, value) in DEFAULT_BOOLEAN_PROPERTIES.iter() {
            modify_bool(pm, key, value.item);
        }

        //float
        for (key, value) in DEFAULT_DOUBLE_PROPERTIES.iter() {
            modify_double_in(pm, key, value.item);
        }

        //int
        for (key, value) in DEFAULT_LONG_PROPERTIES.iter() {
            modify_long(pm, key, value.item);
        }

        //string
        for (key, value) in DEFAULT_STRING_PROPERTIES.iter() {
            modify_string(pm, key, &value.item);
        }
    }

    /// The body of `PropertyManager.ModifyBool` over the state.
    pub(super) fn modify_bool(pm: &PropertyManagerState, key: &str, new_val: bool) -> bool {
        let Some(default) = DEFAULT_BOOLEAN_PROPERTIES.get(key) else {
            return false;
        };
        let mut caches = lock(&pm.caches);
        if let Some(e) = caches.bools.get_mut(key) {
            e.modify(new_val);
        } else {
            caches.bools.insert(
                key.to_owned(),
                ConfigurationEntry::with_description(true, new_val, default.description.clone()),
            );
        }
        true
    }

    /// The body of `PropertyManager.ModifyLong` over the state.
    pub(super) fn modify_long(pm: &PropertyManagerState, key: &str, new_val: i64) -> bool {
        let Some(default) = DEFAULT_LONG_PROPERTIES.get(key) else {
            return false;
        };
        let mut caches = lock(&pm.caches);
        if let Some(e) = caches.longs.get_mut(key) {
            e.modify(new_val);
        } else {
            caches.longs.insert(
                key.to_owned(),
                ConfigurationEntry::with_description(true, new_val, default.description.clone()),
            );
        }
        true
    }

    /// The cache part of `PropertyManager.ModifyDouble` over the state.
    pub(super) fn modify_double(pm: &PropertyManagerState, key: &str, new_val: f64) -> bool {
        modify_double_in(pm, key, new_val)
    }

    /// The body of `PropertyManager.ModifyString` over the state.
    pub(super) fn modify_string(pm: &PropertyManagerState, key: &str, new_val: &str) -> bool {
        let Some(default) = DEFAULT_STRING_PROPERTIES.get(key) else {
            return false;
        };
        let mut caches = lock(&pm.caches);
        if let Some(e) = caches.strings.get_mut(key) {
            e.modify(new_val.to_owned());
        } else {
            caches.strings.insert(
                key.to_owned(),
                ConfigurationEntry::with_description(
                    true,
                    new_val.to_owned(),
                    default.description.clone(),
                ),
            );
        }
        true
    }

    // ==================================================================================
    // To change these values for the server,
    // please use the /modifybool, /modifylong, /modifydouble, and /modifystring commands
    // ==================================================================================

    // ACE: DefaultPropertyManager.DefaultBooleanProperties
    pub static DEFAULT_BOOLEAN_PROPERTIES: LazyLock<DotNetDict<&'static str, Property<bool>>> =
        LazyLock::new(|| {
            let mut dict = dict_of(DEFAULT_BOOLEAN_PAIRS, |v: &bool| *v);
            // Not ACE's: this server's own options follow ACE's, in [`ADDED_BOOLEAN_PAIRS`].
            for (key, value, description) in ADDED_BOOLEAN_PAIRS {
                dict.add(*key, Property::new(*value, Some((*description).to_owned())));
            }
            dict
        });

    // ACE: DefaultPropertyManager.DefaultLongProperties
    pub static DEFAULT_LONG_PROPERTIES: LazyLock<DotNetDict<&'static str, Property<i64>>> =
        LazyLock::new(|| dict_of(DEFAULT_LONG_PAIRS, |v: &i64| *v));

    // ACE: DefaultPropertyManager.DefaultDoubleProperties
    pub static DEFAULT_DOUBLE_PROPERTIES: LazyLock<DotNetDict<&'static str, Property<f64>>> =
        LazyLock::new(|| dict_of(DEFAULT_DOUBLE_PAIRS, |v: &f64| *v));

    // ACE: DefaultPropertyManager.DefaultStringProperties
    pub static DEFAULT_STRING_PROPERTIES: LazyLock<DotNetDict<&'static str, Property<String>>> =
        LazyLock::new(|| dict_of(DEFAULT_STRING_PAIRS, |v: &&str| (*v).to_owned()));

    const DEFAULT_BOOLEAN_PAIRS: &[(&str, bool, &str)] = &[
        ("account_login_boots_in_use", true, "if FALSE, oldest connection to account is not booted when new connection occurs"),
        ("advanced_combat_pets", false, "(non-retail function) If enabled, Combat Pets can cast spells"),
        ("advocate_fane_auto_bestow", false, "If enabled, Advocate Fane will automatically bestow new advocates to advocate_fane_auto_bestow_level"),
        ("aetheria_heal_color", false, "If enabled, changes the aetheria healing over time messages from the default retail red color to green"),
        ("allow_combat_mode_crafting", false, "If enabled, allows players to do crafting (recipes) from all stances. Forces players to NonCombat first, then continues to recipe action."),
        ("allow_door_hold", true, "enables retail behavior where standing on a door while it is closing keeps the door as ethereal until it is free from collisions, effectively holding the door open for other players"),
        ("allow_fast_chug", true, "enables retail behavior where a player can consume food and drink faster than normal by breaking animation"),
        ("allow_highres_dat", false, "enables client to use highres dat for graphics"),
        ("allow_jump_loot", true, "enables retail behavior where a player can quickly loot items while jumping, bypassing the 'crouch down' animation"),
        ("allow_negative_dispel_resist", true, "enables retail behavior where #-# negative dispels can be resisted"),
        ("allow_negative_rating_curve", true, "enables retail behavior where negative DRR from void dots didn't switch to the reverse rating formula, resulting in a possibly unintended curve that quickly ramps up as -rating goes down, eventually approaching infinity / divide by 0 for -100 rating. less than -100 rating would produce negative numbers."),
        ("allow_pkl_bump", true, "enables retail behavior where /pkl checks for entry collisions, bumping the player position over if standing on another PKLite. This effectively enables /pkl door skipping from retail"),
        ("allow_summoning_killtask_multicredit", true, "enables retail behavior where a summoner can get multiple killtask credits from a monster"),
        ("assess_creature_mod", false, "(non-retail function) If enabled, re-enables former skill formula, when assess creature skill is not trained or spec'ed"),
        ("attribute_augmentation_safety_cap", true, "if TRUE players are not able to use attribute augmentations if the innate value of the target attribute is >= 96. All normal restrictions to these augmentations still apply."),
        ("chat_disable_general", false, "disable general global chat channel"),
        ("chat_disable_lfg", false, "disable lfg global chat channel"),
        ("chat_disable_olthoi", false, "disable olthoi global chat channel"),
        ("chat_disable_roleplay", false, "disable roleplay global chat channel"),
        ("chat_disable_trade", false, "disable trade global chat channel"),
        ("chat_echo_only", false, "global chat returns to sender only"),
        ("chat_echo_reject", false, "global chat returns to sender on reject"),
        ("chat_inform_reject", true, "global chat informs sender on reason for reject"),
        ("chat_log_abuse", false, "log abuse chat"),
        ("chat_log_admin", false, "log admin chat"),
        ("chat_log_advocate", false, "log advocate chat"),
        ("chat_log_allegiance", false, "log allegiance chat"),
        ("chat_log_audit", true, "log audit chat"),
        ("chat_log_debug", false, "log debug chat"),
        ("chat_log_fellow", false, "log fellow chat"),
        ("chat_log_general", false, "log general chat"),
        ("chat_log_global", false, "log global broadcasts"),
        ("chat_log_help", false, "log help chat"),
        ("chat_log_lfg", false, "log LFG chat"),
        ("chat_log_olthoi", false, "log olthoi chat"),
        ("chat_log_qa", false, "log QA chat"),
        ("chat_log_roleplay", false, "log roleplay chat"),
        ("chat_log_sentinel", false, "log sentinel chat"),
        ("chat_log_society", false, "log society chat"),
        ("chat_log_trade", false, "log trade chat"),
        ("chat_log_townchans", false, "log advocate town chat"),
        ("chat_requires_account_15days", false, "global chat privileges requires accounts to be 15 days or older"),
        ("chess_enabled", true, "if FALSE then chess will be disabled"),
        ("use_cloak_proc_custom_scale", false, "If TRUE, the calculation for cloak procs will be based upon the values set by the server oeprator."),
        ("client_movement_formula", false, "If enabled, server uses DoMotion/StopMotion self-client movement methods instead of apply_raw_movement"),
        ("container_opener_name", false, "If enabled, when a player tries to open a container that is already in use by someone else, replaces 'someone else' in the message with the actual name of the player"),
        ("corpse_decay_tick_logging", false, "If ENABLED then player corpse ticks will be logged"),
        ("corpse_destroy_pyreals", true, "If FALSE then pyreals will not be completely destroyed on player death"),
        ("craft_exact_msg", false, "If TRUE, and player has crafting chance of success dialog enabled, shows them an additional message in their chat window with exact %"),
        ("creature_name_check", true, "if enabled, creature names in world database restricts player names during character creation"),
        ("creatures_drop_createlist_wield", false, "If FALSE then Wielded items in CreateList will not drop. Retail defaulted to TRUE but there are currently data errors"),
        ("fastbuff", true, "If TRUE, enables the fast buffing trick from retail."),
        ("fellow_busy_no_recruit", true, "if FALSE, fellows can be recruited while they are busy, different from retail"),
        // DIVERGE: default false where ACE's is true (retail, V377): on retail a fellow holding
        // the kill task was credited when a fellow without it made the kill.
        ("fellow_kt_killer", false, "if FALSE, fellowship kill tasks will share with the fellowship, even if the killer doesn't have the quest"),
        ("fellow_kt_landblock", false, "if TRUE, fellowship kill tasks will share with landblock range (192 distance radius, or entire dungeon)"),
        ("fellow_quest_bonus", false, "if TRUE, applies EvenShare formula to fellowship quest reward XP (300% max bonus, defaults to false in retail)"),
        ("fix_chest_missing_inventory_window", false, "Very non-standard fix. This fixes an acclient bug where unlocking a chest, and then quickly opening it before the client has received the Locked=false update from server can result in the chest opening, but with the chest inventory window not displaying. Bug has a higher chance of appearing with more network latency."),
        ("gateway_ties_summonable", true, "if disabled, players cannot summon ties from gateways. defaults to enabled, as in retail"),
        ("gearknight_core_plating", true, "if disabled, Gear Knight players are not required to use core plating devices for armor and clothing. defaults to enabled, as in retail"),
        ("house_15day_account", true, "if disabled, houses can be purchased with accounts created less than 15 days old"),
        ("house_30day_cooldown", true, "if disabled, houses can be purchased without waiting 30 days between each purchase"),
        ("house_hook_limit", true, "if disabled, house hook limits are ignored"),
        ("house_hookgroup_limit", true, "if disabled, house hook group limits are ignored"),
        ("house_per_char", false, "if TRUE, allows 1 house per char instead of 1 house per account"),
        ("house_purchase_requirements", true, "if disabled, requirements to purchase/rent house are not checked"),
        ("house_rent_enabled", true, "If FALSE then rent is not required"),
        ("iou_trades", false, "(non-retail function) If enabled, IOUs can be traded for objects that are missing in DB but added/restored later on"),
        ("item_dispel", false, "if enabled, allows players to dispel items. defaults to end of retail, where item dispels could only target creatures"),
        ("lifestone_broadcast_death", true, "if true, player deaths are additionally broadcast to other players standing near the destination lifestone"),
        ("loot_quality_mod", true, "if FALSE then the loot quality modifier of a Death Treasure profile does not affect loot generation"),
        ("npc_hairstyle_fullrange", false, "if TRUE, allows generated creatures to use full range of hairstyles. Retail only allowed first nine (0-8) out of 51"),
        ("offline_xp_passup_limit", true, "if FALSE, allows unlimited xp to passup to offline characters in allegiances"),
        ("olthoi_play_disabled", false, "if false, allows players to create and play as olthoi characters"),
        ("override_encounter_spawn_rates", false, "if enabled, landblock encounter spawns are overidden by double properties below."),
        ("permit_corpse_all", false, "If TRUE, /permit grants permittees access to all corpses of the permitter. Defaults to FALSE as per retail, where /permit only grants access to 1 locked corpse"),
        ("persist_movement", false, "If TRUE, persists autonomous movements such as turns and sidesteps through non-autonomous server actions. Retail didn't appear to do this, but some players may prefer this."),
        ("pet_stow_replace", false, "pet stowing for different pet devices becomes a stow and replace. defaults to retail value of false"),
        ("player_config_command", false, "If enabled, players can use /config to change their settings via text commands"),
        ("player_receive_immediate_save", false, "if enabled, when the player receives items from an NPC, they will be saved immediately"),
        ("pk_server", false, "set this to TRUE for darktide servers"),
        ("pk_server_safe_training_academy", false, "set this to TRUE to disable pk fighting in training academy and time to exit starter town safely"),
        ("pkl_server", false, "set this to TRUE for pink servers"),
        ("quest_info_enabled", false, "toggles the /myquests player command"),
        ("rares_real_time", true, "allow for second chance roll based on an rng seeded timestamp for a rare on rare eligible kills that do not generate a rare, rares_max_seconds_between defines maximum seconds before second chance kicks in"),
        ("rares_real_time_v2", false, "chances for a rare to be generated on rare eligible kills are modified by the last time one was found per each player, rares_max_days_between defines maximum days before guaranteed rare generation"),
        ("runrate_add_hooks", false, "if TRUE, adds some runrate hooks that were missing from retail (exhaustion done, raise skill/attribute"),
        ("reportbug_enabled", false, "toggles the /reportbug player command"),
        ("require_spell_comps", true, "if FALSE, spell components are no longer required to be in inventory to cast spells. defaults to enabled, as in retail"),
        ("safe_spell_comps", false, "if TRUE, disables spell component burning for everyone"),
        ("salvage_handle_overages", false, "in retail, if 2 salvage bags were combined beyond 100 structure, the overages would be lost"),
        ("show_ammo_buff", false, "shows active enchantments such as blood drinker on equipped missile ammo during appraisal"),
        ("show_aura_buff", false, "shows active aura enchantments on wielded items during appraisal"),
        ("show_dat_warning", false, "if TRUE, will alert player (dat_warning_msg) when client attempts to download from server and boot them from game, disabled by default"),
        ("show_dot_messages", false, "enabled, shows combat messages for DoT damage ticks. defaults to disabled, as in retail"),
        ("show_first_login_gift", false, "if TRUE, will show on first login that the player earned bonus item (Blackmoor's Favor and/or Asheron's Benediction), disabled by default because msg is kind of odd on an emulator"),
        ("show_mana_conv_bonus_0", true, "if disabled, only shows mana conversion bonus if not zero, during appraisal of casting items"),
        ("smite_uses_takedamage", false, "if enabled, smite applies damage via TakeDamage"),
        ("spellcast_recoil_queue", false, "if true, players can queue the next spell to cast during recoil animation"),
        ("spell_projectile_ethereal", false, "broadcasts all spell projectiles as ethereal to clients only, and manually send stop velocity on collision. can fix various issues with client missing target id."),
        ("suicide_instant_death", false, "if enabled, @die command kills player instantly. defaults to disabled, as in retail"),
        ("taboo_table", true, "if enabled, taboo table restricts player names during character creation"),
        ("tailoring_intermediate_uieffects", false, "If true, tailoring intermediate icons retain the magical/elemental highlight of the original item"),
        ("trajectory_alt_solver", false, "use the alternate trajectory solver for missiles and spell projectiles"),
        ("universal_masteries", true, "if TRUE, matches end of retail masteries - players wielding almost any weapon get +5 DR, except if the weapon \"seems tough to master\". if FALSE, players start with mastery of 1 melee and 1 ranged weapon type based on heritage, and can later re-select these 2 masteries"),
        ("unlimited_sequence_gaps", false, "upon startup, allows server to find all unused guids in a range instead of a set hard limit"),
        ("use_generator_rotation_offset", true, "enables or disables using the generator's current rotation when offseting relative positions"),
        ("use_portal_max_level_requirement", true, "disable this to ignore the max level restriction on portals"),
        ("use_turbine_chat", true, "enables or disables global chat channels (General, LFG, Roleplay, Trade, Olthoi, Society, Allegience)"),
        ("use_wield_requirements", true, "disable this to bypass wield requirements. mostly for dev debugging"),
        // DIVERGE: names our command (brand); shards that stored ACE's description get this one from the shard migration.
        ("version_info_enabled", false, "toggles the /empversion player command"),
        ("vendor_shop_uses_generator", false, "enables or disables vendors using generator system in addition to createlist to create artificial scarcity"),
        ("world_closed", false, "enable this to startup world as a closed to players world"),
    ];

    /// Not ACE's: the boolean options this server adds to ACE's, each a deliberate divergence, loaded
    /// and listed after ACE's.
    pub const ADDED_BOOLEAN_PAIRS: &[(&str, bool, &str)] = &[
        // Not ACE's (retail, V336): ranged monsters close to a random fraction
        // of their attack range before attacking.
        ("monster_ranged_closing", true, "Ranged and casting monsters whose target is beyond their attack range run to a random fraction (2/3 to 16/15) of that range with a non-sticky chase, then turn and attack, as retail's did (39,987 captured closing moves; see V336). If FALSE, ACE's behaviour: missile monsters stand and turn (switching to melee beyond range), casters beyond range chase stickily, and spell range is capped at 75."),
    ];

    const DEFAULT_LONG_PAIRS: &[(&str, i64, &str)] = &[
        ("char_delete_time", 3600, "the amount of time in seconds a deleted character can be restored"),
        ("chat_requires_account_time_seconds", 0, "the amount of time in seconds an account is required to have existed for for global chat privileges"),
        ("chat_requires_player_age", 0, "the amount of time in seconds a player is required to have played for global chat privileges"),
        ("chat_requires_player_level", 0, "the level a player is required to have for global chat privileges"),
        ("corpse_spam_limit", 15, "the number of corpses a player is allowed to leave on a landblock at one time"),
        ("default_subscription_level", 1, "retail defaults to 1, 1 = standard subscription (same as 2 and 3), 4 grants ToD pre-order bonus item Asheron's Benediction"),
        ("fellowship_even_share_level", 50, "level when fellowship XP sharing is no longer restricted"),
        ("mansion_min_rank", 6, "overrides the default allegiance rank required to own a mansion"),
        ("max_chars_per_account", 11, "retail defaults to 11, client supports up to 20"),
        ("pk_timer", 20, "the number of seconds where a player cannot perform certain actions (ie. teleporting) after becoming involved in a PK battle"),
        ("player_save_interval", 300, "the number of seconds between automatic player saves"),
        ("rares_max_days_between", 45, "for rares_real_time_v2: the maximum number of days a player can go before a rare is generated on rare eligible creature kills"),
        ("rares_max_seconds_between", 5256000, "for rares_real_time: the maximum number of seconds a player can go before a second chance at a rare is allowed on rare eligible creature kills that did not generate a rare"),
        ("summoning_killtask_multicredit_cap", 2, "if allow_summoning_killtask_multicredit is enabled, the maximum # of killtask credits a player can receive from 1 kill"),
        ("teleport_visibility_fix", 0, "Fixes some possible issues with invisible players and mobs. 0 = default / disabled, 1 = players only, 2 = creatures, 3 = all world objects"),
    ];

    const DEFAULT_DOUBLE_PAIRS: &[(&str, f64, &str)] = &[
        ("cantrip_drop_rate", 1.0, "Scales the chance for cantrips to drop in each tier. Defaults to 1.0, as per end of retail"),
        ("cloak_cooldown_seconds", 5.0, "The number of seconds between possible cloak procs."),
        ("cloak_max_proc_base", 0.25, "The max proc chance of a cloak."),
        ("cloak_max_proc_damage_percentage", 0.3, "The damage percentage at which cloak proc chance plateaus."),
        ("cloak_min_proc", 0.0, "The minimum proc chance of a cloak."),
        ("minor_cantrip_drop_rate", 1.0, "Scales the chance for minor cantrips to drop, relative to other cantrip levels in the tier. Defaults to 1.0, as per end of retail"),
        ("major_cantrip_drop_rate", 1.0, "Scales the chance for major cantrips to drop, relative to other cantrip levels in the tier. Defaults to 1.0, as per end of retail"),
        ("epic_cantrip_drop_rate", 1.0, "Scales the chance for epic cantrips to drop, relative to other cantrip levels in the tier. Defaults to 1.0, as per end of retail"),
        ("legendary_cantrip_drop_rate", 1.0, "Scales the chance for legendary cantrips to drop, relative to other cantrip levels in the tier. Defaults to 1.0, as per end of retail"),
        ("advocate_fane_auto_bestow_level", 1.0, "the level that advocates are automatically bestowed by Advocate Fane if advocate_fane_auto_bestow is true"),
        ("aetheria_drop_rate", 1.0, "Modifier for Aetheria drop rate, 1 being normal"),
        ("chess_ai_start_time", -1.0, "the number of seconds for the chess ai to start. defaults to -1 (disabled)"),
        ("encounter_delay", 1800.0, "the number of seconds a generator profile for regions is delayed from returning to free slots"),
        ("encounter_regen_interval", 600.0, "the number of seconds a generator for regions at which spawns its next set of objects"),
        ("fast_missile_modifier", 1.2, "The speed multiplier applied to fast missiles. Defaults to retail value of 1.2"),
        ("ignore_magic_armor_pvp_scalar", 1.0, "Scales the effectiveness of IgnoreMagicArmor (ie. hollow weapons) in pvp battles. 1.0 = full effectiveness / ignore all enchantments on armor (default), 0.5 = half effectiveness / use half enchantments from armor, 0.0 = no effectiveness / use full enchantments from armor"),
        ("ignore_magic_resist_pvp_scalar", 1.0, "Scales the effectiveness of IgnoreMagicResist (ie. hollow weapons) in pvp battles. 1.0 = full effectiveness / ignore all resistances from life enchantments (default), 0.5 = half effectiveness / use half resistances from life enchantments, 0.0 = no effectiveness / use full resistances from life enchantments"),
        ("luminance_modifier", 1.0, "Scales the amount of luminance received by players"),
        ("melee_max_angle", 0.0, "for melee players, the maximum angle before a TurnTo is required. retail appeared to have required a TurnTo even for the smallest of angle offsets."),
        ("mob_awareness_range", 1.0, "Scales the distance the monsters become alerted and aggro the players"),
        ("pk_new_character_grace_period", 300.0, "the number of seconds, in addition to pk_respite_timer, that a player killer is set to non-player killer status after first exiting training academy"),
        ("pk_respite_timer", 300.0, "the number of seconds that a player killer is set to non-player killer status after dying to another player killer"),
        ("quest_lum_modifier", 1.0, "Scale multiplier for amount of quest luminance received by players.  Quest lum is also modified by 'luminance_modifier'."),
        ("quest_mindelta_rate", 1.0, "scales all quest min delta time between solves, 1 being normal"),
        ("quest_xp_modifier", 1.0, "Scale multiplier for amount of quest XP received by players.  Quest XP is also modified by 'xp_modifier'."),
        ("rare_drop_rate_percent", 0.04, "Adjust the chance of a rare to spawn as a percentage. Default is 0.04, or 1 in 2,500. Max is 100, or every eligible drop."),
        ("spellcast_max_angle", 20.0, "for advanced player spell casting, the maximum angle to target release a spell projectile. retail seemed to default to value of around 20, although some players seem to prefer a higher 45 degree angle"),
        ("trophy_drop_rate", 1.0, "Modifier for trophies dropped on creature death"),
        ("unlocker_window", 10.0, "The number of seconds a player unlocking a chest has exclusive access to first opening the chest."),
        ("vendor_unique_rot_time", 300.0, "the number of seconds before unique items sold to vendors disappear"),
        ("vitae_penalty", 0.05, "the amount of vitae penalty a player gets per death"),
        ("vitae_penalty_max", 0.4, "the maximum vitae penalty a player can have"),
        ("void_pvp_modifier", 0.5, "Scales the amount of damage players take from Void Magic. Defaults to 0.5, as per retail. For earlier content where DRR isn't as readily available, this can be adjusted for balance."),
        ("xp_modifier", 1.0, "scales the amount of xp received by players"),
    ];

    const DEFAULT_STRING_PAIRS: &[(&str, &str, &str)] = &[
        // DIVERGE: the description names the working directory where ACE's names ACE's assembly (brand); stored rows: the shard migration.
        ("content_folder", "Content", "for content creators to live edit weenies. defaults to the Content folder in the server's working directory"),
        // DIVERGE: ACE's two DAT warnings send players to ACE's site; ours to https://dereth.network (brand). Shards that stored ACE's text are moved by the shard migration.
        ("dat_older_warning_msg", "Your DAT files are incomplete.\nThis server does not support dynamic DAT updating at this time.\nPlease visit https://dereth.network to download the complete DAT files.", "Warning message displayed (if show_dat_warning is true) to player if client attempts DAT download from server"),
        ("dat_newer_warning_msg", "Your DAT files are newer than expected.\nPlease visit https://dereth.network to download the correct DAT files.", "Warning message displayed (if show_dat_warning is true) to player if client connects to this server"),
        ("popup_header", "Welcome to Asheron's Call!", "Welcome message displayed when you log in"),
        ("popup_welcome", "To begin your training, speak to the Society Greeter. Walk up to the Society Greeter using the 'W' key, then double-click on her to initiate a conversation.", "Welcome message popup in training halls"),
        ("popup_welcome_olthoi", "Welcome to the Olthoi hive! Be sure to talk to the Olthoi Queen to receive the Olthoi protections granted by the energies of the hive.", "Welcome message displayed on the first login for an Olthoi Player"),
        ("popup_motd", "", "Popup message of the day"),
        ("server_motd", "", "Server message of the day"),
    ];
}
