//! The object caches behind a [`Database`](crate::Database): one per record type, holding every
//! record loaded, how many holders each has, and the records nobody holds any more.
//!
//! A record handed out is held through an [`Obj`]: cloning one adds a holder, dropping one takes
//! a holder away. When a record's last holder lets go it is not thrown away at once but put on
//! its type's **free list** (newest last, with the time it was freed), so that asking for it
//! again finds it in memory; the list keeps at most the type's greatest number (the oldest freed
//! goes first when it overflows), and once the list is longer than the type's ideal number, the
//! oldest record freed more than 30 seconds ago goes each time the caches are given time. A type
//! whose caches are told not to keep free records, or a record not allowed on the list, is thrown
//! away when its last holder lets go.

use std::any::{Any, TypeId};
use std::collections::{HashMap, VecDeque};
use std::fmt;
use std::ops::Deref;
use std::sync::{Arc, Mutex, PoisonError, Weak};

use auberean_dat::RecordType;

pub(crate) type Held = Arc<dyn Any + Send + Sync>;

/// A record's key in its type's cache: its id and the Rust type it was decoded as.
pub(crate) type Key = (u32, TypeId);

/// How long a free record past the ideal number stays, in seconds of the caches' clock.
pub const FREE_LINGER: f64 = 30.0;

/// The free list's sizes when the type table gives none.
// D2-12010
const DEFAULT_SIZES: (usize, usize) = (16, 16);

struct Slot {
    value: Held,
    /// Holders outside the cache.
    holders: u32,
    freed_at: f64,
    allowed_in_free_list: bool,
}

/// One record type's cache.
pub(crate) struct ObjectCache {
    kind: u32,
    objects: HashMap<Key, Slot>,
    /// Oldest freed first.
    free: VecDeque<Key>,
    may_keep: bool,
    keep_free: bool,
    ideal: usize,
    max: usize,
}

impl ObjectCache {
    /// A type's cache, its free list sized by the type table.
    // D2-12663, D2-12661, D2-05013
    fn new(kind: u32) -> Self {
        let (ideal, max) = match RecordType(kind).cache_sizes() {
            (0, 0) => DEFAULT_SIZES,
            (i, m) => (i as usize, m as usize),
        };
        Self {
            kind,
            objects: HashMap::new(),
            free: VecDeque::new(),
            may_keep: true,
            keep_free: true,
            ideal,
            max,
        }
    }

    /// Add a record with one holder; `false` (nothing changed) when the cache holds its key.
    // D2-12641, D2-12664
    fn add(&mut self, key: Key, value: Held) -> bool {
        if key.0 == 0 || self.objects.contains_key(&key) {
            return false;
        }
        self.objects.insert(
            key,
            Slot {
                value,
                holders: 1,
                freed_at: 0.0,
                allowed_in_free_list: true,
            },
        );
        true
    }

    /// A loaded record, with one more holder; taken off the free list when nobody held it.
    // D2-12656
    fn get_if_in_memory(&mut self, key: Key) -> Option<Held> {
        let slot = self.objects.get_mut(&key)?;
        if slot.holders == 0 {
            self.free.retain(|k| *k != key);
        }
        let slot = self.objects.get_mut(&key)?;
        slot.holders += 1;
        Some(slot.value.clone())
    }

    /// A loaded record that somebody holds, with one more holder.
    // D2-12657
    fn get_if_using(&mut self, key: Key) -> Option<Held> {
        let slot = self.objects.get_mut(&key).filter(|s| s.holders > 0)?;
        slot.holders += 1;
        Some(slot.value.clone())
    }

    /// One more holder of a held record.
    // D2-12687
    fn add_holder(&mut self, key: Key) {
        if let Some(s) = self.objects.get_mut(&key) {
            s.holders += 1;
        }
    }

    /// One holder fewer; a record nobody holds any more is freed.
    // D2-12658, D2-12688
    fn release(&mut self, key: Key, now: f64) {
        let Some(slot) = self.objects.get_mut(&key) else {
            return;
        };
        if slot.holders == 0 {
            return;
        }
        slot.holders -= 1;
        if slot.holders == 0 {
            self.free_object(key, now);
        }
    }

    /// A record nobody holds: onto the free list, or thrown away when the cache keeps no free
    /// records or the record may not be kept.
    // D2-12642
    fn free_object(&mut self, key: Key, now: f64) {
        let allowed = self
            .objects
            .get(&key)
            .is_some_and(|s| s.allowed_in_free_list);
        if !self.keep_free || !allowed {
            self.destroy(key);
        } else {
            self.free_list_add(key, now);
        }
    }

    /// Put a freed record last on the free list, throwing the oldest away when the list is longer
    /// than its greatest size.
    // D2-12644
    fn free_list_add(&mut self, key: Key, now: f64) {
        if let Some(s) = self.objects.get_mut(&key) {
            s.freed_at = now;
        }
        self.free.push_back(key);
        if self.free.len() > self.max {
            if let Some(oldest) = self.free_list_remove_oldest() {
                self.destroy(oldest);
            }
        }
    }

    // D2-12645
    fn free_list_remove(&mut self, key: Key) {
        self.free.retain(|k| *k != key);
    }

    // D2-12646
    fn free_list_remove_oldest(&mut self) -> Option<Key> {
        self.free.pop_front()
    }

    /// Before a new record of the type is read: a type that recycles, with more unused records
    /// than its ideal number, gives up its oldest unused one for it.
    // D2-12647, D2-12766
    fn get_free_obj(&mut self) {
        if RecordType(self.kind).recycles_free_records() && self.free.len() > self.ideal {
            if let Some(oldest) = self.free_list_remove_oldest() {
                self.destroy(oldest);
            }
        }
    }

    /// Throw a record away.
    // D2-12654, D2-12653
    fn destroy(&mut self, key: Key) {
        self.free_list_remove(key);
        self.objects.remove(&key);
    }

    /// Throw away a record held or not.
    // D2-12655, D2-12699
    fn remove(&mut self, key: Key) -> bool {
        self.free_list_remove(key);
        self.objects.remove(&key).is_some()
    }

    /// Given time: past the ideal number of free records, the oldest freed more than
    /// [`FREE_LINGER`] seconds ago goes.
    // D2-12640
    fn use_time(&mut self, now: f64) {
        if self.free.len() <= self.ideal {
            return;
        }
        let Some(&oldest) = self.free.front() else {
            return;
        };
        if self
            .objects
            .get(&oldest)
            .is_some_and(|s| s.freed_at + FREE_LINGER < now)
        {
            self.free_list_remove(oldest);
            self.destroy(oldest);
        }
    }

    /// Throw away every free record.
    // D2-12649
    fn flush_free(&mut self) {
        while let Some(k) = self.free_list_remove_oldest() {
            self.destroy(k);
        }
    }

    /// Whether to keep free records; turning it off throws away the free ones.
    // D2-12651
    fn keep_free_objects(&mut self, keep: bool) {
        if !self.may_keep {
            return;
        }
        let was = self.keep_free;
        self.keep_free = keep;
        if was && !keep {
            self.flush_free();
        }
    }

    /// The keys of every record loaded.
    fn keys(&self) -> Vec<Key> {
        self.objects.keys().copied().collect()
    }
}

/// Every type's cache, and the clock the caches were last given.
#[derive(Default)]
pub(crate) struct Caches {
    by_type: HashMap<u32, ObjectCache>,
    now: f64,
}

impl Caches {
    /// A type's cache, made the first time a record of the type is added.
    // D2-12622, D2-12631, D2-12695
    fn cache(&mut self, kind: u32) -> &mut ObjectCache {
        self.by_type
            .entry(kind)
            .or_insert_with(|| ObjectCache::new(kind))
    }

    fn cache_if_made(&mut self, kind: u32) -> Option<&mut ObjectCache> {
        self.by_type.get_mut(&kind)
    }
}

/// The caches shared by a database and the handles it gives out.
pub(crate) type Shared = Arc<Mutex<Caches>>;

pub(crate) fn lock(s: &Mutex<Caches>) -> std::sync::MutexGuard<'_, Caches> {
    s.lock().unwrap_or_else(PoisonError::into_inner)
}

/// A record held from a database: derefs to the record; while any handle to it lives the record
/// stays loaded, and when the last one goes it is freed to its type's free list.
pub struct Obj<T: Send + Sync + 'static> {
    value: Arc<T>,
    kind: u32,
    key: Key,
    caches: Weak<Mutex<Caches>>,
}

impl<T: Send + Sync + 'static> Obj<T> {
    pub(crate) fn new(value: Arc<T>, kind: u32, id: u32, caches: &Shared) -> Self {
        Self {
            value,
            kind,
            key: (id, TypeId::of::<T>()),
            caches: Arc::downgrade(caches),
        }
    }

    /// The record's id.
    #[must_use]
    pub fn id(&self) -> u32 {
        self.key.0
    }

    /// The record itself, shared, independent of the cache's holding of it.
    #[must_use]
    pub fn arc(&self) -> Arc<T> {
        self.value.clone()
    }

    /// A copy of the record, owned by the caller and held by no cache.
    // D2-12697
    #[must_use]
    pub fn copy(&self) -> T
    where
        T: Clone,
    {
        T::clone(&self.value)
    }

    /// Keep the record off its type's free list: when its last holder lets go it is thrown away.
    pub fn not_in_free_list(&self) {
        if let Some(c) = self.caches.upgrade() {
            if let Some(s) = lock(&c)
                .cache_if_made(self.kind)
                .and_then(|oc| oc.objects.get_mut(&self.key))
            {
                s.allowed_in_free_list = false;
            }
        }
    }
}

impl<T: Send + Sync + 'static> Deref for Obj<T> {
    type Target = T;

    fn deref(&self) -> &T {
        &self.value
    }
}

impl<T: Send + Sync + 'static> Clone for Obj<T> {
    fn clone(&self) -> Self {
        if let Some(c) = self.caches.upgrade() {
            lock(&c).cache(self.kind).add_holder(self.key);
        }
        Self {
            value: self.value.clone(),
            kind: self.kind,
            key: self.key,
            caches: self.caches.clone(),
        }
    }
}

// D2-03652
impl<T: Send + Sync + 'static> Drop for Obj<T> {
    fn drop(&mut self) {
        if let Some(c) = self.caches.upgrade() {
            let mut g = lock(&c);
            let now = g.now;
            if let Some(oc) = g.cache_if_made(self.kind) {
                oc.release(self.key, now);
            }
        }
    }
}

impl<T: Send + Sync + fmt::Debug + 'static> fmt::Debug for Obj<T> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_tuple("Obj").field(&self.value).finish()
    }
}

// ---------------------------------------------------------------- the database's use of them

/// A record already loaded, given one more holder (loaded records only; nothing is read).
pub(crate) fn in_memory<T: Send + Sync + 'static>(
    shared: &Shared,
    kind: u32,
    id: u32,
) -> Option<Obj<T>> {
    let key = (id, TypeId::of::<T>());
    let held = lock(shared).cache_if_made(kind)?.get_if_in_memory(key)?;
    let value = held.downcast::<T>().ok()?;
    Some(Obj::new(value, kind, id, shared))
}

/// A record somebody holds, given one more holder.
pub(crate) fn in_use<T: Send + Sync + 'static>(
    shared: &Shared,
    kind: u32,
    id: u32,
) -> Option<Obj<T>> {
    let key = (id, TypeId::of::<T>());
    let held = lock(shared).cache_if_made(kind)?.get_if_using(key)?;
    let value = held.downcast::<T>().ok()?;
    Some(Obj::new(value, kind, id, shared))
}

/// Make room before a record of a type is read ([`ObjectCache::get_free_obj`]).
pub(crate) fn get_free_obj(shared: &Shared, kind: u32) {
    if let Some(c) = lock(shared).cache_if_made(kind) {
        c.get_free_obj();
    }
}

/// Add a record just read (or made) with one holder; when the cache already holds it (another
/// caller added it meanwhile) that one is handed out instead.
pub(crate) fn insert<T: Send + Sync + 'static>(
    shared: &Shared,
    kind: u32,
    id: u32,
    value: Arc<T>,
) -> Obj<T> {
    let key = (id, TypeId::of::<T>());
    {
        let mut g = lock(shared);
        let c = g.cache(kind);
        if !c.add(key, value.clone()) {
            if let Some(v) = c.get_if_in_memory(key).and_then(|h| h.downcast::<T>().ok()) {
                drop(g);
                return Obj::new(v, kind, id, shared);
            }
        }
    }
    Obj::new(value, kind, id, shared)
}

/// Replace a loaded record's value (a reload); holders keep the value they had.
// D2-12643, D2-12609, D2-12689
pub(crate) fn replace<T: Send + Sync + 'static>(
    shared: &Shared,
    kind: u32,
    id: u32,
    value: Arc<T>,
) -> bool {
    let key = (id, TypeId::of::<T>());
    match lock(shared)
        .cache_if_made(kind)
        .and_then(|c| c.objects.get_mut(&key))
    {
        Some(s) => {
            s.value = value;
            true
        }
        None => false,
    }
}

/// Throw a record away from its type's cache, held or not.
pub(crate) fn remove(shared: &Shared, kind: u32, id: u32, ty: TypeId) -> bool {
    lock(shared)
        .cache_if_made(kind)
        .is_some_and(|c| c.remove((id, ty)))
}

/// The ids of a type's loaded records, with the Rust types they were decoded as.
pub(crate) fn loaded(shared: &Shared, kind: u32) -> Vec<Key> {
    lock(shared)
        .cache_if_made(kind)
        .map(|c| c.keys())
        .unwrap_or_default()
}

/// Give the caches time: the clock moves to `now`, and each type's cache may let its oldest free
/// record go.
// D2-12617
pub(crate) fn use_time(shared: &Shared, now: f64) {
    let mut g = lock(shared);
    g.now = now;
    for c in g.by_type.values_mut() {
        c.use_time(now);
    }
}

/// Throw away the free records of one type (`Some`) or of every type.
// D2-12620
pub(crate) fn flush_free(shared: &Shared, kind: Option<u32>) {
    let mut g = lock(shared);
    match kind {
        Some(k) => {
            if let Some(c) = g.cache_if_made(k) {
                c.flush_free();
            }
        }
        None => g.by_type.values_mut().for_each(ObjectCache::flush_free),
    }
}

/// Whether one type's caches (`Some`) or every type's keep free records.
// D2-12629
pub(crate) fn keep_free_objects(shared: &Shared, kind: Option<u32>, keep: bool) -> bool {
    let mut g = lock(shared);
    match kind {
        Some(k) => match g.cache_if_made(k) {
            Some(c) => {
                c.keep_free_objects(keep);
                true
            }
            None => false,
        },
        None => {
            g.by_type
                .values_mut()
                .for_each(|c| c.keep_free_objects(keep));
            true
        }
    }
}

/// Set a type's ideal number of free records.
// D2-12648
pub(crate) fn set_free_list_ideal(shared: &Shared, kind: u32, ideal: usize) {
    lock(shared).cache(kind).ideal = ideal;
}

/// Throw away the free records of every cell type; `true` when no cell record is left loaded.
// D2-12618
pub(crate) fn unload_cell_data(shared: &Shared) -> bool {
    let mut g = lock(shared);
    let mut all_gone = true;
    for c in g.by_type.values_mut() {
        if RecordType(c.kind).is_cell_type() {
            c.flush_free();
            all_gone = all_gone && c.objects.is_empty();
        }
    }
    all_gone
}

/// How many records of a type are loaded, and how many of them are free.
pub(crate) fn counts(shared: &Shared, kind: u32) -> (usize, usize) {
    lock(shared)
        .cache_if_made(kind)
        .map_or((0, 0), |c| (c.objects.len(), c.free.len()))
}

/// Throw away every cache.
// D2-12623, D2-12660, D2-12662, D2-12650, D2-12625, D2-12615
pub(crate) fn destroy_all(shared: &Shared) {
    lock(shared).by_type.clear();
}
