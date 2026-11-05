//! The container natives: hashes, sets, lists and arrays of words, 64-bit values and references,
//! and their iterators.
//!
//! The library names every container package by its element kinds and family (see
//! `auberean_script::library::container_name`); the natives here are bound to every package of a
//! family by the method's name, and read the element kinds from the object itself, so one
//! function serves `AAHash`, `ARHash`, `LRHash` and the rest. A 64-bit element takes two slots,
//! low word first; a reference element (hashed or not) read out of a container is tagged as a
//! reference, any other as written by a native. A container whose receiver is the null reference makes the call crash.
//!
//! Hash tables keep the engine's buckets (`auberean_script::native_object`): a new entry goes to the
//! head of its chain, and growing or shrinking rehashes into another prime bucket count keeping
//! each chain's order. Iterators stand at a place in packing order (buckets in order, each chain
//! from its head; a list from its front), the end being past the last entry.
//!
//! Multi-hashes are the engine's other table (`auberean_script::multi_hash`): several entries
//! under one key, a new one after the first of its key. Their iterators stand at an entry, not a
//! place, as the engine's hold the entry itself: removing another entry does not move them.
//!
//! A table or set keyed by hashed references files a key by the object it names: a string by its
//! text, a case-insensitive string by its text in lower case, a string reference by its own bucket
//! rule; any other object in bucket 0. Two keys are the same when they name objects of one type
//! that are equal (strings of the same text, case-insensitive strings equal but for case, string
//! references alike); objects of any other type are never the same key, not even one object twice.

use std::sync::Arc;

use auberean_script::heap::Heap;
use auberean_script::library::{container_name, Container, Elem, Library};
use auberean_script::multi_hash::{MultiHash, Node};
use auberean_script::multi_map::MultiMap;
use auberean_script::native::{Binding, Call, NativeFn, NativeTable, Status, Stop};
use auberean_script::native_object::{
    native_type, Hash, HashSet, Item, NativeObject, TextInfo, BUCKET_SIZES,
};
use auberean_script::value::{join, split, tag, NativeId, NULL};

/// An iterator standing past the end.
const END: u32 = auberean_script::native_object::ITERATOR_END;

/// The container methods: family, method, function. A family is the container's family name,
/// with `Iter` for its iterators.
const METHODS: &[(&str, &str, NativeFn)] = &[
    ("Hash", "add", hash_add),
    ("Hash", "set", hash_set),
    ("Hash", "erase", hash_erase),
    ("Hash", "find", hash_find),
    ("Hash", "remove", hash_remove),
    ("Hash", "contains", hash_contains),
    ("Hash", "num_elements", count),
    ("Hash", "num_buckets", bucket_count),
    ("Hash", "empty", is_empty),
    ("Hash", "get_begin", begin),
    ("Hash", "get_end", end),
    ("Hash", "flush", flush),
    ("Hash", "grow", grow),
    ("Hash", "shrink", shrink),
    ("Hash", "resize", resize),
    ("Hash", "optimize_size", optimize_size),
    ("Hash", "too_big", too_big),
    ("Hash", "too_small", too_small),
    ("HashIter", "next", iter_next),
    ("HashIter", "data", iter_value),
    ("HashIter", "key", iter_key),
    ("HashIter", "notdone", iter_not_done),
    ("HashIter", "equals", iter_equals),
    ("HashIter", "remove", iter_remove),
    ("HashSet", "add", set_add),
    ("HashSet", "contains", set_contains),
    ("HashSet", "erase", set_erase),
    ("HashSet", "num_elements", count),
    ("HashSet", "num_buckets", bucket_count),
    ("HashSet", "empty", is_empty),
    ("HashSet", "get_begin", begin),
    ("HashSet", "get_end", end),
    ("HashSet", "flush", flush),
    ("HashSet", "grow", grow),
    ("HashSet", "shrink", shrink),
    ("HashSet", "resize", resize),
    ("HashSet", "optimize_size", optimize_size),
    ("HashSet", "too_big", too_big),
    ("HashSet", "too_small", too_small),
    ("HashSetIter", "next", iter_next),
    ("HashSetIter", "key", iter_key),
    ("HashSetIter", "notdone", iter_not_done),
    ("HashSetIter", "equals", iter_equals),
    ("HashSetIter", "remove", iter_remove),
    ("List", "num_elements", count),
    ("List", "empty", is_empty),
    ("List", "flush", flush),
    ("List", "get_begin", begin),
    ("List", "get_end", end),
    ("List", "get_reverse", list_reverse),
    ("List", "push_back", list_push_back),
    ("List", "push_tail", list_push_back),
    ("List", "insert_back", list_push_back),
    ("List", "insert_tail", list_push_back),
    ("List", "push_front", list_push_front),
    ("List", "push_head", list_push_front),
    ("List", "insert_front", list_push_front),
    ("List", "insert_head", list_push_front),
    ("List", "push", list_insert_at),
    ("List", "insert", list_insert_at),
    ("List", "pop_front", list_pop_front),
    ("List", "pop_head", list_pop_front),
    ("List", "remove_front", list_pop_front),
    ("List", "remove_head", list_pop_front),
    ("List", "pop_back", list_pop_back),
    ("List", "pop_tail", list_pop_back),
    ("List", "remove_back", list_pop_back),
    ("List", "remove_tail", list_pop_back),
    ("List", "peek_front", list_peek_front),
    ("List", "peek_head", list_peek_front),
    ("List", "peek_back", list_peek_back),
    ("List", "peek_tail", list_peek_back),
    ("List", "erase_front", list_erase_front),
    ("List", "erase_head", list_erase_front),
    ("List", "erase_back", list_erase_back),
    ("List", "erase_tail", list_erase_back),
    ("List", "remove", list_remove),
    ("List", "contains", seq_contains),
    ("List", "get_position", list_position),
    ("List", "inq_data", list_at),
    ("List", "push_unique_back", list_unique_back),
    ("List", "push_unique_tail", list_unique_back),
    ("List", "insert_unique_back", list_unique_back),
    ("List", "insert_unique_tail", list_unique_back),
    ("List", "push_unique_front", list_unique_front),
    ("List", "push_unique_head", list_unique_front),
    ("List", "insert_unique_front", list_unique_front),
    ("List", "insert_unique_head", list_unique_front),
    ("List", "insert_sort_int", list_sort_int),
    ("List", "insert_sort_float", list_sort_float),
    ("List", "insert_sort_ulint", list_sort_ulint),
    ("ListIter", "next", iter_next),
    ("ListIter", "prev", iter_prev),
    ("ListIter", "data", iter_key),
    ("ListIter", "notdone", iter_not_done),
    ("ListIter", "equals", iter_equals),
    ("ListIter", "remove", iter_remove),
    ("ListIter", "insert", iter_insert),
    ("Array", "push", array_push),
    ("Array", "push_unique", array_push_unique),
    ("Array", "pop", array_pop),
    ("Array", "num_elements", count),
    ("Array", "empty", is_empty),
    ("Array", "flush", flush),
    ("Array", "query", array_query),
    ("Array", "get", array_get),
    ("Array", "set", array_set),
    ("Array", "erase", array_erase),
    ("Array", "contains", seq_contains),
    ("Array", "set_num_elements", array_resize),
    ("MultiHash", "add", multi_add),
    ("MultiHash", "remove", multi_remove),
    ("MultiHash", "clear", multi_clear),
    ("MultiHash", "size", multi_size),
    ("MultiHash", "empty", multi_empty),
    ("MultiHash", "count", multi_count),
    ("MultiHash", "erase", multi_erase),
    ("MultiHash", "find", multi_find),
    ("MultiHash", "find_last", multi_find_last),
    ("MultiHash", "get_begin", multi_begin),
    ("MultiHash", "get_end", multi_end),
    ("MultiHashIter", "next", multi_iter_next),
    ("MultiHashIter", "key", multi_iter_key),
    ("MultiHashIter", "data", multi_iter_data),
    ("MultiHashIter", "equals", multi_iter_equals),
    ("MultiMap", "insert", map_insert),
    ("MultiMap", "clear", map_clear),
    ("MultiMap", "empty", map_empty),
    ("MultiMap", "count", map_count),
    ("MultiMap", "erase", map_erase),
    ("MultiMap", "remove", map_remove),
    ("MultiMap", "find", map_find),
    ("MultiMap", "find_last", map_find_last),
    ("MultiMap", "get_begin", map_begin),
    ("MultiMap", "get_end", map_end),
    ("MultiMap", "lower_bound", map_lower_bound),
    ("MultiMap", "upper_bound", map_upper_bound),
    ("MultiMapIter", "prev", map_iter_prev),
    ("MultiMapIter", "next", map_iter_next),
    ("MultiMapIter", "key", map_iter_key),
    ("MultiMapIter", "data", map_iter_data),
    ("MultiMapIter", "equals", map_iter_equals),
];

/// Bind the container natives of every container package of the library, by family and method.
/// Multi-hashes keyed by hashed references are left to their stand-ins.
pub fn bind(t: &mut NativeTable, lib: &Library) {
    for p in lib.packages.iter().filter(|p| p.is_native()) {
        let Some(name) = container_name(&p.name) else {
            continue;
        };
        let (family, hashed) = match name.container {
            Container::Hash(..) => ("Hash", false),
            Container::MultiHash(k, _) => ("MultiHash", k == Elem::HashedRef),
            Container::MultiMap(..) => ("MultiMap", false),
            Container::HashSet(_) => ("HashSet", false),
            Container::List(_) => ("List", false),
            Container::Array(_) => ("Array", false),
        };
        if hashed {
            continue;
        }
        for f in &p.functions {
            let found = METHODS.iter().find(|(fam, m, _)| {
                *m == f.name
                    && fam
                        .strip_suffix("Iter")
                        .map_or(!name.iterator && *fam == family, |b| {
                            name.iterator && b == family
                        })
            });
            if let Some((_, _, native)) = found {
                t.bind(NativeId(f.id.0), Binding::Ported(*native));
            }
        }
    }
}

// ---------------------------------------------------------------------------------------------
// Slots
// ---------------------------------------------------------------------------------------------

fn width(e: Elem) -> i64 {
    if e == Elem::Long {
        2
    } else {
        1
    }
}

/// Parameters below the receiver: each takes its width in slots, low word lowest.
struct Params {
    next: i64,
}

impl Params {
    fn new() -> Self {
        Self { next: -2 }
    }

    /// The slot of the next parameter's low word.
    fn take(&mut self, w: i64) -> i64 {
        let low = self.next - (w - 1);
        self.next -= w;
        low
    }
}

fn read_item(c: &Call<'_>, at: i64, kind: Elem) -> Result<Item, Stop> {
    Ok(match kind {
        Elem::Word => Item::Word(c.cx.at(at)?),
        Elem::Long => Item::Long(join(c.cx.at(at)?, c.cx.at(at + 1)?)),
        Elem::Ref => Item::Ref(c.cx.at(at)?),
        Elem::HashedRef => Item::HashedRef {
            id: c.cx.at(at)?,
            hash: 0,
            use_hash: 0,
        },
    })
}

fn item_tag(kind: Elem) -> u8 {
    if matches!(kind, Elem::Ref | Elem::HashedRef) {
        tag::REFERENCE
    } else {
        tag::MIXED
    }
}

fn item_words(item: Item) -> (u32, Option<u32>) {
    match item {
        Item::Word(v) | Item::Ref(v) | Item::HashedRef { id: v, .. } => (v, None),
        Item::Long(v) => {
            let (lo, hi) = split(v);
            (lo, Some(hi))
        }
    }
}

/// Write an element into result slots starting at `at`.
fn write_item(c: &mut Call<'_>, at: i64, item: Item, kind: Elem) -> Result<(), Stop> {
    let t = item_tag(kind);
    let (lo, hi) = item_words(item);
    c.write(at, lo, t)?;
    if let Some(hi) = hi {
        c.write(at + 1, hi, t)?;
    }
    Ok(())
}

/// Set the tag of the slot an out reference names, as the natives do before they look.
fn out_tag(c: &mut Call<'_>, r: u32, kind: Elem) -> Result<(), Stop> {
    let prog = Arc::clone(c.vm.program());
    c.vm.write_slot(c.host, c.cx, r, 0, None, item_tag(kind), &prog)
}

/// Write an element through an out reference (its tag already set).
fn out_item(c: &mut Call<'_>, r: u32, item: Item) -> Result<(), Stop> {
    let prog = Arc::clone(c.vm.program());
    let (lo, hi) = item_words(item);
    c.vm.write_data(c.host, c.cx, r, 0, lo, &prog)?;
    if let Some(hi) = hi {
        c.vm.write_data(c.host, c.cx, r, 1, hi, &prog)?;
    }
    Ok(())
}

fn result(c: &mut Call<'_>, p: &mut Params, v: u32) -> Result<Status, Stop> {
    let at = p.take(1);
    c.write(at, v, tag::MIXED)?;
    Ok(Status::Success)
}

/// The receiver, or `None` when it is the null reference (the call then crashes).
fn receiver(c: &Call<'_>) -> Result<Option<u32>, Stop> {
    let r = c.receiver()?;
    Ok((r != NULL).then_some(r))
}

macro_rules! with_receiver {
    ($c:ident, $r:ident) => {
        let Some($r) = receiver($c)? else {
            return Ok(Status::Crash);
        };
    };
}

fn wrong(c: &Call<'_>, what: &str) -> Stop {
    c.fail(format!("the receiver is not {what}"))
}

/// Same key: words, longs and references compare by value.
fn same(a: &Item, b: &Item) -> bool {
    match (a, b) {
        (Item::HashedRef { id: x, .. }, Item::HashedRef { id: y, .. }) => x == y,
        _ => a == b,
    }
}

/// The bucket among `n` a hashed reference's object lands in: a string's text hashed, a
/// case-insensitive string's text hashed in lower case, a string reference's own bucket; any other
/// object (or none) bucket 0.
// D2-16457, D2-00752, D2-04322, D2-04277
fn hashed_bucket(heap: &Heap, id: u32, n: usize) -> usize {
    if n == 0 || id == NULL {
        return 0;
    }
    #[allow(clippy::cast_possible_truncation)]
    let n32 = n as u32;
    let ty = heap.object(id).map_or(0, |o| o.native_type);
    let b = match heap.native(id) {
        Ok(NativeObject::Text(t)) if ty == native_type::WP_STRING => {
            auberean_text::info::string_hash(t) % n32
        }
        Ok(NativeObject::Text(t)) if ty == native_type::CASE_INSENSITIVE_WP_STRING => {
            insensitive_hash(t) % n32
        }
        Ok(NativeObject::TextInfo(t)) => info_bucket(t, n32),
        _ => 0,
    };
    b as usize
}

/// A wide string's hash with each letter in lower case (no all-ones guard).
fn insensitive_hash(t: &[u16]) -> u32 {
    let mut h: u32 = 0;
    for &u in t.iter().take_while(|u| **u != 0) {
        h = h.wrapping_mul(16).wrapping_add(u32::from(ascii_lower(u)));
        let top = h & 0xF000_0000;
        if top != 0 {
            h = ((top >> 24) ^ h) & 0x0FFF_FFFF;
        }
    }
    h
}

fn ascii_lower(u: u16) -> u16 {
    if (u16::from(b'A')..=u16::from(b'Z')).contains(&u) {
        u + 32
    } else {
        u
    }
}

/// A string reference's bucket among `n`: a literal's text hashed, else its table's and its
/// token's remainders added.
// D2-14123
fn info_bucket(t: &TextInfo, n: u32) -> u32 {
    match &t.literal {
        Some(lit) => auberean_text::info::string_hash(lit) % n,
        None => (t.table % n + t.string_id % n) % n,
    }
}

/// Whether two hashed references name equal objects of one type (see the module's notes).
// D2-16458, D2-00753, D2-04351, D2-04324, D2-14147
fn hashed_equal(heap: &Heap, a: u32, b: u32) -> bool {
    if a == NULL || b == NULL {
        return false;
    }
    let ty = |id| heap.object(id).map_or(0, |o| o.native_type);
    let (ta, tb) = (ty(a), ty(b));
    if ta != tb {
        return false;
    }
    match (heap.native(a), heap.native(b)) {
        (Ok(NativeObject::Text(x)), Ok(NativeObject::Text(y))) => {
            let (x, y) = (cstr(x), cstr(y));
            if ta == native_type::WP_STRING {
                x == y
            } else if ta == native_type::CASE_INSENSITIVE_WP_STRING {
                x.len() == y.len()
                    && x.iter()
                        .zip(y)
                        .all(|(p, q)| ascii_lower(*p) == ascii_lower(*q))
            } else {
                false
            }
        }
        (Ok(NativeObject::TextInfo(x)), Ok(NativeObject::TextInfo(y))) => {
            x.string_id == y.string_id
                && x.table == y.table
                && x.literal == y.literal
                && x.variables == y.variables
        }
        _ => false,
    }
}

/// A wide string to its first zero.
fn cstr(t: &[u16]) -> &[u16] {
    &t[..t.iter().position(|u| *u == 0).unwrap_or(t.len())]
}

/// A key's bucket among `n` (a hashed reference's bucket kept in it, as the engine keeps it).
fn key_bucket(heap: &Heap, k: &mut Item, n: usize) -> usize {
    match k {
        Item::HashedRef { id, hash, use_hash } => {
            let b = hashed_bucket(heap, *id, n);
            #[allow(clippy::cast_possible_truncation)]
            {
                *hash = b as u32;
            }
            *use_hash = 0;
            b
        }
        _ => k.bucket(n),
    }
}

/// Whether a key held is the key looked for.
fn same_key(heap: &Heap, held: &Item, k: &Item) -> bool {
    match (held, k) {
        (Item::HashedRef { id: a, .. }, Item::HashedRef { id: b, .. }) => {
            hashed_equal(heap, *a, *b)
        }
        _ => same(held, k),
    }
}

// ---------------------------------------------------------------------------------------------
// Hash tables
// ---------------------------------------------------------------------------------------------

fn hash_mut<'a>(c: &'a mut Call<'_>, r: u32) -> Result<&'a mut Hash, Stop> {
    match c.vm.heap.native_mut(r)? {
        NativeObject::Hash(h) => Ok(h),
        _ => Err(Stop::Native {
            native: c.native,
            what: "the receiver is not a hash table".into(),
        }),
    }
}

fn hash_kinds(c: &mut Call<'_>, r: u32) -> Result<(Elem, Elem), Stop> {
    let h = hash_mut(c, r)?;
    Ok((h.key, h.value))
}

/// Where a key goes in table `r`: its bucket, and its place in the bucket's chain when it is there.
fn place(c: &mut Call<'_>, r: u32, k: &mut Item) -> Result<(usize, Option<usize>), Stop> {
    let heap = &c.vm.heap;
    let h = match heap.native(r)? {
        NativeObject::Hash(h) => h,
        _ => return Err(wrong(c, "a hash table")),
    };
    let b = key_bucket(heap, k, h.buckets.len());
    let at = h.buckets[b].iter().position(|(x, _)| same_key(heap, x, k));
    Ok((b, at))
}

/// Add an entry unless its key is there: true when added.
// D2-09126, D2-09208, D2-09339, D2-09423, D2-09478
fn hash_add(c: &mut Call<'_>) -> Result<Status, Stop> {
    with_receiver!(c, r);
    let (kk, vk) = hash_kinds(c, r)?;
    let mut p = Params::new();
    let mut k = read_item(c, p.take(width(kk)), kk)?;
    let v = read_item(c, p.take(width(vk)), vk)?;
    let (b, at) = place(c, r, &mut k)?;
    let added = at.is_none();
    if added {
        hash_mut(c, r)?.buckets[b].insert(0, (k, v));
    }
    result(c, &mut p, u32::from(added))
}

/// Set an entry's value, adding it when its key is not there: always true.
// D2-09127, D2-09209, D2-09340, D2-09424, D2-09479
fn hash_set(c: &mut Call<'_>) -> Result<Status, Stop> {
    with_receiver!(c, r);
    let (kk, vk) = hash_kinds(c, r)?;
    let mut p = Params::new();
    let mut k = read_item(c, p.take(width(kk)), kk)?;
    let v = read_item(c, p.take(width(vk)), vk)?;
    let (b, at) = place(c, r, &mut k)?;
    let h = hash_mut(c, r)?;
    match at {
        Some(i) => h.buckets[b][i].1 = v,
        None => h.buckets[b].insert(0, (k, v)),
    }
    result(c, &mut p, 1)
}

/// Remove an entry: true when it was there.
// D2-09052, D2-09278, D2-09476
fn hash_erase(c: &mut Call<'_>) -> Result<Status, Stop> {
    with_receiver!(c, r);
    let (kk, _) = hash_kinds(c, r)?;
    let mut p = Params::new();
    let mut k = read_item(c, p.take(width(kk)), kk)?;
    let (b, at) = place(c, r, &mut k)?;
    let h = hash_mut(c, r)?;
    let found = at.map(|i| h.buckets[b].remove(i));
    result(c, &mut p, u32::from(found.is_some()))
}

/// Look a key up: the value through the out reference when found; true when found.
// D2-09063, D2-09136, D2-09214, D2-09273, D2-09347, D2-09429, D2-09491, D2-09501
fn hash_find(c: &mut Call<'_>) -> Result<Status, Stop> {
    with_receiver!(c, r);
    let (kk, vk) = hash_kinds(c, r)?;
    let mut p = Params::new();
    let mut k = read_item(c, p.take(width(kk)), kk)?;
    let out = c.cx.at(p.take(1))?;
    out_tag(c, out, vk)?;
    let (b, at) = place(c, r, &mut k)?;
    let h = hash_mut(c, r)?;
    let v = at.map(|i| h.buckets[b][i].1);
    if let Some(v) = v {
        out_item(c, out, v)?;
    }
    result(c, &mut p, u32::from(v.is_some()))
}

/// Remove an entry, its value through the out reference; true when it was there.
// D2-09064, D2-09137, D2-09215, D2-09274, D2-09348, D2-09430, D2-09492, D2-09502
fn hash_remove(c: &mut Call<'_>) -> Result<Status, Stop> {
    with_receiver!(c, r);
    let (kk, vk) = hash_kinds(c, r)?;
    let mut p = Params::new();
    let mut k = read_item(c, p.take(width(kk)), kk)?;
    let out = c.cx.at(p.take(1))?;
    out_tag(c, out, vk)?;
    let (b, at) = place(c, r, &mut k)?;
    let h = hash_mut(c, r)?;
    let v = at.map(|i| h.buckets[b].remove(i).1);
    if let Some(v) = v {
        out_item(c, out, v)?;
    }
    result(c, &mut p, u32::from(v.is_some()))
}

/// Whether a key is there.
// D2-09207, D2-09422, D2-09475
fn hash_contains(c: &mut Call<'_>) -> Result<Status, Stop> {
    with_receiver!(c, r);
    let (kk, _) = hash_kinds(c, r)?;
    let mut p = Params::new();
    let mut k = read_item(c, p.take(width(kk)), kk)?;
    let yes = place(c, r, &mut k)?.1.is_some();
    result(c, &mut p, u32::from(yes))
}

// ---------------------------------------------------------------------------------------------
// Hash sets
// ---------------------------------------------------------------------------------------------

fn set_mut<'a>(c: &'a mut Call<'_>, r: u32) -> Result<&'a mut HashSet, Stop> {
    match c.vm.heap.native_mut(r)? {
        NativeObject::HashSet(s) => Ok(s),
        _ => Err(Stop::Native {
            native: c.native,
            what: "the receiver is not a hash set".into(),
        }),
    }
}

/// The item a set native is asked about (its bucket kept in it), where it goes in set `r` (its
/// bucket, and its place in the chain when it is there), and the parameters after it.
fn set_lookup(c: &mut Call<'_>, r: u32) -> Result<(Item, usize, Option<usize>, Params), Stop> {
    let kind = set_mut(c, r)?.item;
    let mut p = Params::new();
    let mut k = read_item(c, p.take(width(kind)), kind)?;
    let heap = &c.vm.heap;
    let s = match heap.native(r)? {
        NativeObject::HashSet(s) => s,
        _ => return Err(wrong(c, "a hash set")),
    };
    let b = key_bucket(heap, &mut k, s.buckets.len());
    let at = s.buckets[b].iter().position(|x| same_key(heap, x, &k));
    Ok((k, b, at, p))
}

/// Add an item unless it is there: true when added.
// D2-09114, D2-09280
fn set_add(c: &mut Call<'_>) -> Result<Status, Stop> {
    with_receiver!(c, r);
    let (k, b, at, mut p) = set_lookup(c, r)?;
    let added = at.is_none();
    if added {
        set_mut(c, r)?.buckets[b].insert(0, k);
    }
    result(c, &mut p, u32::from(added))
}

/// Whether an item is there.
// D2-09207, D2-09422
fn set_contains(c: &mut Call<'_>) -> Result<Status, Stop> {
    with_receiver!(c, r);
    let (_, _, at, mut p) = set_lookup(c, r)?;
    result(c, &mut p, u32::from(at.is_some()))
}

/// Remove an item: true when it was there.
// D2-09052, D2-09278
fn set_erase(c: &mut Call<'_>) -> Result<Status, Stop> {
    with_receiver!(c, r);
    let (_, b, at, mut p) = set_lookup(c, r)?;
    if let Some(i) = at {
        set_mut(c, r)?.buckets[b].remove(i);
    }
    result(c, &mut p, u32::from(at.is_some()))
}

// ---------------------------------------------------------------------------------------------
// Every container
// ---------------------------------------------------------------------------------------------

/// How many entries a container holds, and its element kinds (key, value).
fn shape(n: &NativeObject) -> Option<(usize, Elem, Option<Elem>)> {
    Some(match n {
        NativeObject::Hash(h) => (h.len(), h.key, Some(h.value)),
        NativeObject::HashSet(s) => (s.buckets.iter().map(Vec::len).sum(), s.item, None),
        NativeObject::Sequence { item, items } => (items.len(), *item, None),
        NativeObject::MultiMap(t) => (t.len(), t.key, Some(t.value)),
        _ => return None,
    })
}

/// The entry at a place in packing order: (key or item, value).
fn entry_at(n: &NativeObject, at: usize) -> Option<(Item, Option<Item>)> {
    match n {
        NativeObject::Hash(h) => h.entries().nth(at).map(|(k, v)| (*k, Some(*v))),
        NativeObject::HashSet(s) => s.buckets.iter().flatten().nth(at).map(|k| (*k, None)),
        NativeObject::Sequence { items, .. } => items.get(at).map(|k| (*k, None)),
        NativeObject::MultiMap(t) => t.entries().get(at).map(|n| (n.key, Some(n.value))),
        _ => None,
    }
}

/// Remove the entry at a place in packing order.
fn remove_at(n: &mut NativeObject, at: usize) -> bool {
    match n {
        NativeObject::Hash(h) => {
            let mut k = at;
            for b in &mut h.buckets {
                if k < b.len() {
                    b.remove(k);
                    return true;
                }
                k -= b.len();
            }
            false
        }
        NativeObject::HashSet(s) => {
            let mut k = at;
            for b in &mut s.buckets {
                if k < b.len() {
                    b.remove(k);
                    return true;
                }
                k -= b.len();
            }
            false
        }
        NativeObject::Sequence { items, .. } if at < items.len() => {
            items.remove(at);
            true
        }
        NativeObject::MultiMap(t) => match t.entries().get(at) {
            Some(n) => {
                let id = n.id;
                t.remove_node(id)
            }
            None => false,
        },
        _ => false,
    }
}

fn shape_of(c: &Call<'_>, r: u32) -> Result<(usize, Elem, Option<Elem>), Stop> {
    let n = c.vm.heap.native(r)?;
    shape(n).ok_or_else(|| wrong(c, "a container"))
}

/// How many entries.
// D2-08629, D2-09203
fn count(c: &mut Call<'_>) -> Result<Status, Stop> {
    with_receiver!(c, r);
    let (n, ..) = shape_of(c, r)?;
    #[allow(clippy::cast_possible_truncation)]
    result(c, &mut Params::new(), n as u32)
}

/// Whether there is no entry.
// D2-09176, D2-09202, D2-09328
fn is_empty(c: &mut Call<'_>) -> Result<Status, Stop> {
    with_receiver!(c, r);
    let (n, ..) = shape_of(c, r)?;
    result(c, &mut Params::new(), u32::from(n == 0))
}

/// Remove every entry.
// D2-09185, D2-09206, D2-09399, D2-09421, D2-09496, D2-09505
fn flush(c: &mut Call<'_>) -> Result<Status, Stop> {
    with_receiver!(c, r);
    match c.vm.heap.native_mut(r)? {
        NativeObject::Hash(h) => h.buckets.iter_mut().for_each(Vec::clear),
        NativeObject::HashSet(s) => s.buckets.iter_mut().for_each(Vec::clear),
        NativeObject::Sequence { items, .. } => items.clear(),
        NativeObject::MultiMap(t) => t.clear(),
        NativeObject::MultiHash(t) => t.clear(),
        _ => return Err(wrong(c, "a container")),
    }
    Ok(Status::Success)
}

/// The bucket count of a hash table or set.
// D2-08536
fn bucket_count(c: &mut Call<'_>) -> Result<Status, Stop> {
    with_receiver!(c, r);
    let n = match c.vm.heap.native(r)? {
        NativeObject::Hash(h) => h.buckets.len(),
        NativeObject::HashSet(s) => s.buckets.len(),
        _ => return Err(wrong(c, "a hash table")),
    };
    #[allow(clippy::cast_possible_truncation)]
    result(c, &mut Params::new(), n as u32)
}

/// Rehash a hash table or set into the bucket count of place `index`, each chain keeping its
/// order (a hashed reference into the bucket `hashed` gives its id): false when that is the count
/// it has.
fn rehash(n: &mut NativeObject, index: u8, hashed: &dyn Fn(u32) -> usize) -> bool {
    let bucket = |k: &Item, b: usize| match k {
        Item::HashedRef {
            id, use_hash: 0, ..
        } => hashed(*id),
        _ => k.bucket(b),
    };
    match n {
        NativeObject::Hash(h) => {
            if h.size_index == index {
                return false;
            }
            let all: Vec<(Item, Item)> = h.entries().copied().collect();
            h.size_index = index;
            h.buckets = vec![Vec::new(); BUCKET_SIZES[usize::from(index)] as usize];
            let b = h.buckets.len();
            for (mut k, v) in all {
                let at = bucket(&k, b);
                if let Item::HashedRef {
                    hash, use_hash: 0, ..
                } = &mut k
                {
                    #[allow(clippy::cast_possible_truncation)]
                    {
                        *hash = at as u32;
                    }
                }
                h.buckets[at].push((k, v));
            }
            true
        }
        NativeObject::HashSet(s) => {
            if s.size_index == index {
                return false;
            }
            let all: Vec<Item> = s.buckets.iter().flatten().copied().collect();
            s.size_index = index;
            s.buckets = vec![Vec::new(); BUCKET_SIZES[usize::from(index)] as usize];
            let b = s.buckets.len();
            for mut k in all {
                let at = bucket(&k, b);
                if let Item::HashedRef {
                    hash, use_hash: 0, ..
                } = &mut k
                {
                    #[allow(clippy::cast_possible_truncation)]
                    {
                        *hash = at as u32;
                    }
                }
                s.buckets[at].push(k);
            }
            true
        }
        _ => false,
    }
}

fn size_index(n: &NativeObject) -> Option<u8> {
    match n {
        NativeObject::Hash(h) => Some(h.size_index),
        NativeObject::HashSet(s) => Some(s.size_index),
        _ => None,
    }
}

/// The place of the smallest bucket count at least `n`, the largest when none is.
fn size_for(n: u32) -> u8 {
    let i = BUCKET_SIZES
        .partition_point(|s| *s < n)
        .min(BUCKET_SIZES.len() - 1);
    #[allow(clippy::cast_possible_truncation)]
    let i = i as u8;
    i
}

fn resize_to(
    c: &mut Call<'_>,
    r: u32,
    pick: impl FnOnce(u8, usize) -> Option<u8>,
    args: i64,
) -> Result<Status, Stop> {
    let n = c.vm.heap.native(r)?;
    let Some(now) = size_index(n) else {
        return Err(wrong(c, "a hash table"));
    };
    let len = shape(n).map_or(0, |s| s.0);
    let done = match pick(now, len) {
        Some(i) => {
            // The hashed references' buckets in the new count, from the objects they name.
            let size = BUCKET_SIZES[usize::from(i)] as usize;
            let ids: Vec<u32> = match n {
                NativeObject::Hash(h) => h.entries().map(|(k, _)| *k).collect(),
                NativeObject::HashSet(s) => s.buckets.iter().flatten().copied().collect(),
                _ => Vec::new(),
            }
            .into_iter()
            .filter_map(|k| match k {
                Item::HashedRef { id, .. } => Some(id),
                _ => None,
            })
            .collect();
            let heap = &c.vm.heap;
            let buckets: std::collections::HashMap<u32, usize> = ids
                .into_iter()
                .map(|id| (id, hashed_bucket(heap, id, size)))
                .collect();
            let n = c.vm.heap.native_mut(r)?;
            rehash(n, i, &|id| buckets.get(&id).copied().unwrap_or(0))
        }
        None => false,
    };
    c.write(-2 - args, u32::from(done), tag::MIXED)?;
    Ok(Status::Success)
}

/// Grow to the next bucket count.
// D2-09059, D2-09122, D2-09133, D2-09270, D2-09287, D2-09487
fn grow(c: &mut Call<'_>) -> Result<Status, Stop> {
    with_receiver!(c, r);
    #[allow(clippy::cast_possible_truncation)]
    resize_to(
        c,
        r,
        |now, _| ((now as usize) + 1 < BUCKET_SIZES.len()).then_some(now + 1),
        0,
    )
}

/// Shrink to the previous bucket count.
// D2-09061, D2-09124, D2-09135, D2-09271, D2-09289, D2-09489
fn shrink(c: &mut Call<'_>) -> Result<Status, Stop> {
    with_receiver!(c, r);
    resize_to(c, r, |now, _| now.checked_sub(1), 0)
}

/// Rehash into the smallest bucket count at least the one asked for.
// D2-09060, D2-09123, D2-09134, D2-09288, D2-09346, D2-09488
fn resize(c: &mut Call<'_>) -> Result<Status, Stop> {
    with_receiver!(c, r);
    let want = c.arg(0)?;
    resize_to(c, r, |_, _| Some(size_for(want)), 1)
}

/// Rehash into the bucket count that suits the entry count.
// D2-09062, D2-09125, D2-09272, D2-09490
fn optimize_size(c: &mut Call<'_>) -> Result<Status, Stop> {
    with_receiver!(c, r);
    #[allow(clippy::cast_possible_truncation)]
    resize_to(c, r, |_, len| Some(size_for(len as u32)), 0)
}

/// Whether the table has more than twice as many buckets as entries.
// D2-09204
fn too_big(c: &mut Call<'_>) -> Result<Status, Stop> {
    with_receiver!(c, r);
    let (len, ..) = shape_of(c, r)?;
    let buckets = match c.vm.heap.native(r)? {
        NativeObject::Hash(h) => h.buckets.len(),
        NativeObject::HashSet(s) => s.buckets.len(),
        _ => return Err(wrong(c, "a hash table")),
    };
    result(c, &mut Params::new(), u32::from(len * 2 < buckets))
}

/// Whether the table has fewer buckets than half its entries.
// D2-09205
fn too_small(c: &mut Call<'_>) -> Result<Status, Stop> {
    with_receiver!(c, r);
    let (len, ..) = shape_of(c, r)?;
    let buckets = match c.vm.heap.native(r)? {
        NativeObject::Hash(h) => h.buckets.len(),
        NativeObject::HashSet(s) => s.buckets.len(),
        _ => return Err(wrong(c, "a hash table")),
    };
    result(c, &mut Params::new(), u32::from(buckets < len / 2))
}

// ---------------------------------------------------------------------------------------------
// Iterators
// ---------------------------------------------------------------------------------------------

/// A new iterator over container `r`, standing at `at`.
fn new_iterator(c: &mut Call<'_>, r: u32, at: u32) -> Result<u32, Stop> {
    let ty = c.vm.heap.object(r)?.native_type;
    let lib = &c.vm.program().library;
    let name = lib
        .package(ty)
        .map(|p| format!("{}Iter", p.name))
        .ok_or_else(|| wrong(c, "a container"))?;
    let it = lib
        .package_id(&name)
        .ok_or_else(|| c.fail(format!("no iterator package for {ty:#x}")))?;
    Ok(c.vm.heap.add_native(
        it,
        NativeObject::Iterator {
            container: r,
            position: at,
        },
    ))
}

/// An iterator at the first entry (the end when there is none).
// D2-09057, D2-09120, D2-09131, D2-09189, D2-09212, D2-09268, D2-09285, D2-09344, D2-09407, D2-09427, D2-09485, D2-09499, D2-09522
// D2-09055, D2-09118, D2-09129, D2-09186, D2-09210, D2-09266, D2-09283, D2-09342, D2-09404, D2-09425, D2-09483, D2-09497, D2-09519
fn begin(c: &mut Call<'_>) -> Result<Status, Stop> {
    with_receiver!(c, r);
    let (n, ..) = shape_of(c, r)?;
    let id = new_iterator(c, r, if n == 0 { END } else { 0 })?;
    c.write(-2, id, tag::REFERENCE)?;
    Ok(Status::Success)
}

/// An iterator past the last entry.
// D2-09058, D2-09121, D2-09132, D2-09190, D2-09213, D2-09269, D2-09286, D2-09345, D2-09408, D2-09428, D2-09486, D2-09500, D2-09523
// D2-09056, D2-09119, D2-09130, D2-09188, D2-09211, D2-09267, D2-09284, D2-09343, D2-09406, D2-09426, D2-09484, D2-09498, D2-09521
fn end(c: &mut Call<'_>) -> Result<Status, Stop> {
    with_receiver!(c, r);
    shape_of(c, r)?;
    let id = new_iterator(c, r, END)?;
    c.write(-2, id, tag::REFERENCE)?;
    Ok(Status::Success)
}

/// A list iterator at the last entry, for walking backwards.
// D2-09191, D2-09409, D2-09524, D2-09187, D2-09405, D2-09520
fn list_reverse(c: &mut Call<'_>) -> Result<Status, Stop> {
    with_receiver!(c, r);
    let (n, ..) = shape_of(c, r)?;
    #[allow(clippy::cast_possible_truncation)]
    let id = new_iterator(c, r, if n == 0 { END } else { (n - 1) as u32 })?;
    c.write(-2, id, tag::REFERENCE)?;
    Ok(Status::Success)
}

fn iterator(c: &Call<'_>, r: u32) -> Result<(u32, u32), Stop> {
    match c.vm.heap.native(r)? {
        NativeObject::Iterator {
            container,
            position,
        } => Ok((*container, *position)),
        _ => Err(wrong(c, "an iterator")),
    }
}

fn set_position(c: &mut Call<'_>, r: u32, at: u32) -> Result<(), Stop> {
    if let NativeObject::Iterator { position, .. } = c.vm.heap.native_mut(r)? {
        *position = at;
    }
    Ok(())
}

/// Step to the next entry (past the last: the end).
// D2-09219, D2-09432, D2-09504, D2-09530, D2-09531
fn iter_next(c: &mut Call<'_>) -> Result<Status, Stop> {
    with_receiver!(c, r);
    let (cont, at) = iterator(c, r)?;
    if at != END {
        let (n, ..) = shape_of(c, cont)?;
        let next = at + 1;
        set_position(c, r, if (next as usize) < n { next } else { END })?;
    }
    Ok(Status::Success)
}

/// Step to the previous entry (before the first: the end).
// D2-09416, D2-09531
fn iter_prev(c: &mut Call<'_>) -> Result<Status, Stop> {
    with_receiver!(c, r);
    let (_, at) = iterator(c, r)?;
    if at != END {
        set_position(c, r, at.checked_sub(1).unwrap_or(END))?;
    }
    Ok(Status::Success)
}

/// The entry an iterator stands at.
fn current(c: &Call<'_>, r: u32) -> Result<(Item, Option<Item>, Elem, Option<Elem>), Stop> {
    let (cont, at) = iterator(c, r)?;
    let n = c.vm.heap.native(cont)?;
    let (_, kk, vk) = shape(n).ok_or_else(|| wrong(c, "a container"))?;
    let (k, v) = entry_at(n, at as usize).ok_or_else(|| c.fail("an iterator at the end read"))?;
    Ok((k, v, kk, vk))
}

/// The key (a list's or set's item) the iterator stands at.
// D2-09198, D2-09217, D2-09276, D2-09417, D2-09494, D2-09532
fn iter_key(c: &mut Call<'_>) -> Result<Status, Stop> {
    with_receiver!(c, r);
    let (k, _, kk, _) = current(c, r)?;
    write_item(c, -1 - width(kk), k, kk)?;
    Ok(Status::Success)
}

/// The value the iterator stands at.
// D2-09065, D2-09138, D2-09216, D2-09275, D2-09349, D2-09431, D2-09493, D2-09503
fn iter_value(c: &mut Call<'_>) -> Result<Status, Stop> {
    with_receiver!(c, r);
    let (_, v, _, vk) = current(c, r)?;
    let (v, vk) = v
        .zip(vk)
        .ok_or_else(|| c.fail("a value of a container without values"))?;
    write_item(c, -1 - width(vk), v, vk)?;
    Ok(Status::Success)
}

/// Whether the iterator stands at an entry.
// D2-09199, D2-09220
fn iter_not_done(c: &mut Call<'_>) -> Result<Status, Stop> {
    with_receiver!(c, r);
    let (cont, at) = iterator(c, r)?;
    let (n, ..) = shape_of(c, cont)?;
    result(
        c,
        &mut Params::new(),
        u32::from(at != END && (at as usize) < n),
    )
}

/// Whether two iterators stand at the same entry.
// D2-09218, D2-09418
fn iter_equals(c: &mut Call<'_>) -> Result<Status, Stop> {
    with_receiver!(c, r);
    let other = c.arg(0)?;
    let a = iterator(c, r)?;
    let b = iterator(c, other)?;
    let mut p = Params::new();
    p.take(1);
    result(c, &mut p, u32::from(a == b))
}

/// Remove the entry the iterator stands at; it then stands at the entry that followed.
// D2-09201, D2-09221, D2-09420, D2-09433
fn iter_remove(c: &mut Call<'_>) -> Result<Status, Stop> {
    with_receiver!(c, r);
    let (cont, at) = iterator(c, r)?;
    let removed = at != END && remove_at(c.vm.heap.native_mut(cont)?, at as usize);
    if removed {
        let (n, ..) = shape_of(c, cont)?;
        if at as usize >= n {
            set_position(c, r, END)?;
        }
    }
    // A list's iterator reports whether it removed; a hash table's reports nothing.
    if matches!(c.vm.heap.native(cont)?, NativeObject::Sequence { .. }) {
        result(c, &mut Params::new(), u32::from(removed))
    } else {
        Ok(Status::Success)
    }
}

/// Insert an item before the entry a list iterator stands at (at the end: at the back).
// D2-09200, D2-09419
fn iter_insert(c: &mut Call<'_>) -> Result<Status, Stop> {
    with_receiver!(c, r);
    let (cont, at) = iterator(c, r)?;
    let mut p = Params::new();
    let (_, kind, _) = shape_of(c, cont)?;
    let v = read_item(c, p.take(width(kind)), kind)?;
    let NativeObject::Sequence { items, .. } = c.vm.heap.native_mut(cont)? else {
        return Err(wrong(c, "a list iterator"));
    };
    let place = if at == END {
        items.len()
    } else {
        (at as usize).min(items.len())
    };
    items.insert(place, v);
    if at != END {
        set_position(c, r, at + 1)?;
    }
    result(c, &mut p, 1)
}

// ---------------------------------------------------------------------------------------------
// Lists and arrays
// ---------------------------------------------------------------------------------------------

fn sequence<'a>(c: &'a mut Call<'_>, r: u32) -> Result<(Elem, &'a mut Vec<Item>), Stop> {
    let native = c.native;
    match c.vm.heap.native_mut(r)? {
        NativeObject::Sequence { item, items } => Ok((*item, items)),
        _ => Err(Stop::Native {
            native,
            what: "the receiver is not a list or an array".into(),
        }),
    }
}

fn seq_arg(c: &mut Call<'_>, r: u32) -> Result<(Item, Elem, Params), Stop> {
    let (kind, _) = sequence(c, r)?;
    let mut p = Params::new();
    let v = read_item(c, p.take(width(kind)), kind)?;
    Ok((v, kind, p))
}

/// Add an item at the back: true.
// D2-09386, D2-09511
fn list_push_back(c: &mut Call<'_>) -> Result<Status, Stop> {
    with_receiver!(c, r);
    let (v, _, mut p) = seq_arg(c, r)?;
    sequence(c, r)?.1.push(v);
    result(c, &mut p, 1)
}

/// Add an item at the front: true.
// D2-09387, D2-09512
fn list_push_front(c: &mut Call<'_>) -> Result<Status, Stop> {
    with_receiver!(c, r);
    let (v, _, mut p) = seq_arg(c, r)?;
    sequence(c, r)?.1.insert(0, v);
    result(c, &mut p, 1)
}

/// Insert an item at a place (past the end: at the back): true.
// D2-09393, D2-09513
fn list_insert_at(c: &mut Call<'_>) -> Result<Status, Stop> {
    with_receiver!(c, r);
    let (kind, _) = sequence(c, r)?;
    let mut p = Params::new();
    let at = c.cx.at(p.take(1))? as usize;
    let v = read_item(c, p.take(width(kind)), kind)?;
    let items = sequence(c, r)?.1;
    let at = at.min(items.len());
    items.insert(at, v);
    result(c, &mut p, 1)
}

fn take_end(c: &mut Call<'_>, front: bool, remove: bool) -> Result<Status, Stop> {
    with_receiver!(c, r);
    let (kind, _) = sequence(c, r)?;
    let mut p = Params::new();
    let out = c.cx.at(p.take(1))?;
    out_tag(c, out, kind)?;
    let items = sequence(c, r)?.1;
    let v = if items.is_empty() {
        None
    } else if front {
        Some(if remove { items.remove(0) } else { items[0] })
    } else if remove {
        items.pop()
    } else {
        items.last().copied()
    };
    if let Some(v) = v {
        out_item(c, out, v)?;
    }
    result(c, &mut p, u32::from(v.is_some()))
}

/// Take the front item out through the out reference: true when there was one.
// D2-09196, D2-09415, D2-09529
fn list_pop_front(c: &mut Call<'_>) -> Result<Status, Stop> {
    take_end(c, true, true)
}

/// Take the back item out through the out reference.
// D2-09197, D2-09414, D2-09528
fn list_pop_back(c: &mut Call<'_>) -> Result<Status, Stop> {
    take_end(c, false, true)
}

/// Read the front item through the out reference.
// D2-09195, D2-09412, D2-09527
fn list_peek_front(c: &mut Call<'_>) -> Result<Status, Stop> {
    take_end(c, true, false)
}

/// Read the back item through the out reference.
// D2-09194, D2-09413, D2-09526
fn list_peek_back(c: &mut Call<'_>) -> Result<Status, Stop> {
    take_end(c, false, false)
}

/// Drop the front item: true when there was one.
// D2-09398, D2-09516
fn list_erase_front(c: &mut Call<'_>) -> Result<Status, Stop> {
    with_receiver!(c, r);
    let items = sequence(c, r)?.1;
    let had = !items.is_empty();
    if had {
        items.remove(0);
    }
    result(c, &mut Params::new(), u32::from(had))
}

/// Drop the back item: true when there was one.
// D2-09184, D2-09397
fn list_erase_back(c: &mut Call<'_>) -> Result<Status, Stop> {
    with_receiver!(c, r);
    let had = sequence(c, r)?.1.pop().is_some();
    result(c, &mut Params::new(), u32::from(had))
}

/// Remove the last item equal to a value: true when there was one.
// D2-09394, D2-09514
fn list_remove(c: &mut Call<'_>) -> Result<Status, Stop> {
    with_receiver!(c, r);
    let (v, _, mut p) = seq_arg(c, r)?;
    let items = sequence(c, r)?.1;
    let at = items.iter().rposition(|x| same(x, &v));
    if let Some(i) = at {
        items.remove(i);
    }
    result(c, &mut p, u32::from(at.is_some()))
}

/// Whether an item is there.
// D2-09105, D2-09183, D2-09327, D2-09396
fn seq_contains(c: &mut Call<'_>) -> Result<Status, Stop> {
    with_receiver!(c, r);
    let (v, _, mut p) = seq_arg(c, r)?;
    let yes = sequence(c, r)?.1.iter().any(|x| same(x, &v));
    result(c, &mut p, u32::from(yes))
}

/// The place of the first item equal to a value, -1 when none is.
// D2-09179, D2-09385
fn list_position(c: &mut Call<'_>) -> Result<Status, Stop> {
    with_receiver!(c, r);
    let (v, _, mut p) = seq_arg(c, r)?;
    let at = sequence(c, r)?.1.iter().position(|x| same(x, &v));
    #[allow(clippy::cast_possible_truncation)]
    result(c, &mut p, at.map_or(NULL, |i| i as u32))
}

/// The item at a place, through the out reference: true when the place is in the list.
// D2-09193, D2-09411, D2-09525, D2-09395, D2-09515
fn list_at(c: &mut Call<'_>) -> Result<Status, Stop> {
    with_receiver!(c, r);
    let (kind, _) = sequence(c, r)?;
    let mut p = Params::new();
    let at = c.cx.at(p.take(1))? as usize;
    let out = c.cx.at(p.take(1))?;
    out_tag(c, out, kind)?;
    let v = sequence(c, r)?.1.get(at).copied();
    if let Some(v) = v {
        out_item(c, out, v)?;
    }
    result(c, &mut p, u32::from(v.is_some()))
}

/// Add an item at the back unless it is there: true when added.
// D2-09401, D2-09517
fn list_unique_back(c: &mut Call<'_>) -> Result<Status, Stop> {
    with_receiver!(c, r);
    let (v, _, mut p) = seq_arg(c, r)?;
    let items = sequence(c, r)?.1;
    let add = !items.iter().any(|x| same(x, &v));
    if add {
        items.push(v);
    }
    result(c, &mut p, u32::from(add))
}

/// Add an item at the front unless it is there: true when added.
// D2-09402, D2-09518
fn list_unique_front(c: &mut Call<'_>) -> Result<Status, Stop> {
    with_receiver!(c, r);
    let (v, _, mut p) = seq_arg(c, r)?;
    let items = sequence(c, r)?.1;
    let add = !items.iter().any(|x| same(x, &v));
    if add {
        items.insert(0, v);
    }
    result(c, &mut p, u32::from(add))
}

fn sorted_insert(c: &mut Call<'_>, before: fn(&Item, &Item) -> bool) -> Result<Status, Stop> {
    with_receiver!(c, r);
    let (v, _, mut p) = seq_arg(c, r)?;
    let items = sequence(c, r)?.1;
    let at = items
        .iter()
        .position(|x| before(&v, x))
        .unwrap_or(items.len());
    items.insert(at, v);
    result(c, &mut p, 1)
}

fn word(i: &Item) -> u32 {
    item_words(*i).0
}

fn long(i: &Item) -> u64 {
    match *i {
        Item::Long(v) => v,
        other => u64::from(item_words(other).0),
    }
}

/// Insert before the first item it is less than, as signed integers.
// D2-09181, D2-09178
fn list_sort_int(c: &mut Call<'_>) -> Result<Status, Stop> {
    #[allow(clippy::cast_possible_wrap)]
    sorted_insert(c, |a, b| (word(a) as i32) < (word(b) as i32))
}

/// Insert before the first item it is less than, as floats.
// D2-09180, D2-09177
fn list_sort_float(c: &mut Call<'_>) -> Result<Status, Stop> {
    sorted_insert(c, |a, b| f32::from_bits(word(a)) < f32::from_bits(word(b)))
}

/// Insert before the first item it is less than, as unsigned 64-bit integers.
// D2-09392, D2-09391
fn list_sort_ulint(c: &mut Call<'_>) -> Result<Status, Stop> {
    sorted_insert(c, |a, b| long(a) < long(b))
}

/// Add an item at the end: true.
// D2-09109, D2-09334
fn array_push(c: &mut Call<'_>) -> Result<Status, Stop> {
    list_push_back(c)
}

/// Add an item at the end unless it is there: true when added.
// D2-09110, D2-09335
fn array_push_unique(c: &mut Call<'_>) -> Result<Status, Stop> {
    list_unique_back(c)
}

/// Take the last item out through the out reference: true when there was one.
// D2-09111, D2-09336, D2-09509
fn array_pop(c: &mut Call<'_>) -> Result<Status, Stop> {
    take_end(c, false, true)
}

/// The item at a place, through the out reference: true when the place is in the array.
// D2-09112, D2-09337, D2-09510
fn array_query(c: &mut Call<'_>) -> Result<Status, Stop> {
    list_at(c)
}

/// The item at a place (the client reads past the end unchecked; here that stops the run).
// D2-09106, D2-09332, D2-09508
fn array_get(c: &mut Call<'_>) -> Result<Status, Stop> {
    with_receiver!(c, r);
    let (kind, _) = sequence(c, r)?;
    let mut p = Params::new();
    let at = c.cx.at(p.take(1))? as usize;
    let v = sequence(c, r)?
        .1
        .get(at)
        .copied()
        .ok_or_else(|| c.fail(format!("place {at} past the end")))?;
    let slot = p.take(width(kind));
    write_item(c, slot, v, kind)?;
    Ok(Status::Success)
}

/// Replace the item at a place: true when the place is in the array.
// D2-09107, D2-09329
fn array_set(c: &mut Call<'_>) -> Result<Status, Stop> {
    with_receiver!(c, r);
    let (kind, _) = sequence(c, r)?;
    let mut p = Params::new();
    let at = c.cx.at(p.take(1))? as usize;
    let v = read_item(c, p.take(width(kind)), kind)?;
    let items = sequence(c, r)?.1;
    let ok = at < items.len();
    if ok {
        items[at] = v;
    }
    result(c, &mut p, u32::from(ok))
}

/// Remove the item at a place, the last item taking its place: true when the place was in the
/// array.
// D2-09108, D2-09331
fn array_erase(c: &mut Call<'_>) -> Result<Status, Stop> {
    with_receiver!(c, r);
    let mut p = Params::new();
    let at = c.cx.at(p.take(1))? as usize;
    let items = sequence(c, r)?.1;
    let ok = at < items.len();
    if ok {
        items.swap_remove(at);
    }
    result(c, &mut p, u32::from(ok))
}

/// Set the item count: true. New places hold zero, or no reference in a list of references.
// D2-09104, D2-09330, D2-09507, D2-09506
fn array_resize(c: &mut Call<'_>) -> Result<Status, Stop> {
    with_receiver!(c, r);
    let (kind, _) = sequence(c, r)?;
    let mut p = Params::new();
    let n = c.cx.at(p.take(1))? as usize;
    let zero = match kind {
        Elem::Long => Item::Long(0),
        Elem::Ref => Item::Ref(NULL),
        _ => Item::Word(0),
    };
    sequence(c, r)?.1.resize(n, zero);
    result(c, &mut p, 1)
}

// ---------------------------------------------------------------------------------------------
// Multi-hashes
// ---------------------------------------------------------------------------------------------

fn multi_mut<'a>(c: &'a mut Call<'_>, r: u32) -> Result<&'a mut MultiHash, Stop> {
    let native = c.native;
    match c.vm.heap.native_mut(r)? {
        NativeObject::MultiHash(t) => Ok(t),
        _ => Err(Stop::Native {
            native,
            what: "the receiver is not a multi-hash".into(),
        }),
    }
}

fn multi_kinds(c: &mut Call<'_>, r: u32) -> Result<(Elem, Elem), Stop> {
    let t = multi_mut(c, r)?;
    Ok((t.key, t.value))
}

/// The key argument, first below the receiver.
fn multi_key(c: &mut Call<'_>, r: u32) -> Result<(Item, Params), Stop> {
    let (kk, _) = multi_kinds(c, r)?;
    let mut p = Params::new();
    let k = read_item(c, p.take(width(kk)), kk)?;
    Ok((k, p))
}

/// Write a multi-hash element into the result slots below the receiver, as the engine does: a
/// word tagged as a native's, a reference as a reference, a 64-bit value's low slot tagged as a
/// native's long (its high slot's tag left as it was).
fn multi_result(c: &mut Call<'_>, item: Item, kind: Elem) -> Result<(), Stop> {
    let at = -1 - width(kind);
    match item {
        Item::Long(v) => {
            let (lo, hi) = split(v);
            c.write(at, lo, tag::LONG | tag::MIXED)?;
            let i = c.cx.frame_index(at + 1)?;
            c.cx.data[i] = hi;
            Ok(())
        }
        other => c.write(at, item_words(other).0, item_tag(kind)),
    }
}

/// Add an entry: after the first entry with its key, else at its chain's head (the table grows
/// first when it would hold more entries than buckets). No result.
// D2-09150, D2-09231, D2-09300, D2-09362
fn multi_add(c: &mut Call<'_>) -> Result<Status, Stop> {
    with_receiver!(c, r);
    let (kk, vk) = multi_kinds(c, r)?;
    let mut p = Params::new();
    let k = read_item(c, p.take(width(kk)), kk)?;
    let v = read_item(c, p.take(width(vk)), vk)?;
    multi_mut(c, r)?.add(k, v);
    Ok(Status::Success)
}

/// Remove every entry with a key: how many there were.
// D2-09079, D2-09149, D2-09299
fn multi_remove(c: &mut Call<'_>) -> Result<Status, Stop> {
    with_receiver!(c, r);
    let (k, mut p) = multi_key(c, r)?;
    let n = multi_mut(c, r)?.remove_key(&k);
    #[allow(clippy::cast_possible_truncation)]
    result(c, &mut p, n as u32)
}

/// Remove every entry.
// D2-09226, D2-09438
fn multi_clear(c: &mut Call<'_>) -> Result<Status, Stop> {
    with_receiver!(c, r);
    multi_mut(c, r)?.clear();
    Ok(Status::Success)
}

/// How many entries.
// D2-09203
fn multi_size(c: &mut Call<'_>) -> Result<Status, Stop> {
    with_receiver!(c, r);
    let n = multi_mut(c, r)?.len();
    #[allow(clippy::cast_possible_truncation)]
    result(c, &mut Params::new(), n as u32)
}

/// Whether there is no entry: written into the result slot's low byte only, as the engine
/// writes it.
// D2-09222
fn multi_empty(c: &mut Call<'_>) -> Result<Status, Stop> {
    with_receiver!(c, r);
    let empty = multi_mut(c, r)?.is_empty();
    let before = c.cx.at(-2)?;
    c.write(-2, (before & !0xFF) | u32::from(empty), tag::MIXED)?;
    Ok(Status::Success)
}

/// How many entries have a key.
// D2-09068, D2-09140, D2-09290, D2-09434
fn multi_count(c: &mut Call<'_>) -> Result<Status, Stop> {
    with_receiver!(c, r);
    let (k, mut p) = multi_key(c, r)?;
    let n = multi_mut(c, r)?.count(&k);
    #[allow(clippy::cast_possible_truncation)]
    result(c, &mut p, n as u32)
}

/// Remove the entry an iterator (`fp[-2]`) stands at, when it is one of this table's. No result.
// D2-09074, D2-09439
fn multi_erase(c: &mut Call<'_>) -> Result<Status, Stop> {
    with_receiver!(c, r);
    let it = c.arg(0)?;
    let (container, at) = iterator(c, it)?;
    if container == r && at != END {
        multi_mut(c, r)?.remove_node(at);
    }
    Ok(Status::Success)
}

/// A new iterator over the receiver standing at an entry (`None`: past the last), written below
/// the arguments taken so far.
fn multi_iterator(
    c: &mut Call<'_>,
    r: u32,
    p: &mut Params,
    at: Option<u32>,
) -> Result<Status, Stop> {
    let id = new_iterator(c, r, at.unwrap_or(END))?;
    let slot = p.take(1);
    c.write(slot, id, tag::REFERENCE)?;
    Ok(Status::Success)
}

/// An iterator at the first entry with a key (past the last when there is none).
// D2-09075, D2-09145, D2-09227, D2-09295, D2-09357, D2-09440
fn multi_find(c: &mut Call<'_>) -> Result<Status, Stop> {
    with_receiver!(c, r);
    let (k, mut p) = multi_key(c, r)?;
    let at = multi_mut(c, r)?.find(&k);
    multi_iterator(c, r, &mut p, at)
}

/// An iterator where a key's entries end.
// D2-09076, D2-09146, D2-09228, D2-09296, D2-09358, D2-09441
fn multi_find_last(c: &mut Call<'_>) -> Result<Status, Stop> {
    with_receiver!(c, r);
    let (k, mut p) = multi_key(c, r)?;
    let at = multi_mut(c, r)?.end_of(&k);
    multi_iterator(c, r, &mut p, at)
}

/// An iterator at the first entry.
// D2-09077, D2-09147, D2-09229, D2-09297, D2-09359, D2-09442
fn multi_begin(c: &mut Call<'_>) -> Result<Status, Stop> {
    with_receiver!(c, r);
    let at = multi_mut(c, r)?.first();
    multi_iterator(c, r, &mut Params::new(), at)
}

/// An iterator past the last entry.
// D2-09078, D2-09148, D2-09230, D2-09298, D2-09360, D2-09443
fn multi_end(c: &mut Call<'_>) -> Result<Status, Stop> {
    with_receiver!(c, r);
    multi_mut(c, r)?;
    multi_iterator(c, r, &mut Params::new(), None)
}

/// The table an iterator walks and the entry it stands at, which must be there (the engine
/// reads through the entry it holds: past the last, or removed, it reads what is not there).
fn multi_entry(c: &mut Call<'_>, it: u32) -> Result<(u32, Node, Elem, Elem), Stop> {
    let (container, at) = iterator(c, it)?;
    if at == END {
        return Err(c.fail("an iterator past the last entry read"));
    }
    let native = c.native;
    let t = multi_mut(c, container)?;
    let (kk, vk) = (t.key, t.value);
    let node = t.node(at).copied().ok_or_else(|| Stop::Native {
        native,
        what: "an iterator at a removed entry read".into(),
    })?;
    Ok((container, node, kk, vk))
}

/// Step to the next entry: the next in its chain, else the head of the next bucket with one.
// D2-09234, D2-09445
fn multi_iter_next(c: &mut Call<'_>) -> Result<Status, Stop> {
    with_receiver!(c, r);
    let (container, node, ..) = multi_entry(c, r)?;
    let next = multi_mut(c, container)?.after(node.id).flatten();
    set_position(c, r, next.unwrap_or(END))?;
    Ok(Status::Success)
}

/// The key of the entry the iterator stands at.
// D2-09233, D2-09248, D2-09302
fn multi_iter_key(c: &mut Call<'_>) -> Result<Status, Stop> {
    with_receiver!(c, r);
    let (_, node, kk, _) = multi_entry(c, r)?;
    multi_result(c, node.key, kk)?;
    Ok(Status::Success)
}

/// The value of the entry the iterator stands at.
// D2-09151, D2-09232, D2-09248, D2-09301, D2-09444
fn multi_iter_data(c: &mut Call<'_>) -> Result<Status, Stop> {
    with_receiver!(c, r);
    let (_, node, _, vk) = multi_entry(c, r)?;
    multi_result(c, node.value, vk)?;
    Ok(Status::Success)
}

/// Whether two iterators (the receiver and `fp[-2]`) stand at the same entry: both past the last
/// of any tables, or the same entry of one.
// D2-09080
fn multi_iter_equals(c: &mut Call<'_>) -> Result<Status, Stop> {
    with_receiver!(c, r);
    let other = c.arg(0)?;
    let (ca, a) = iterator(c, r)?;
    let (cb, b) = iterator(c, other)?;
    let same = a == b && (a == END || ca == cb);
    let mut p = Params::new();
    p.take(1);
    result(c, &mut p, u32::from(same))
}

// ---------------------------------------------------------------------------------------------
// Multi-maps
// ---------------------------------------------------------------------------------------------

fn map_mut<'a>(c: &'a mut Call<'_>, r: u32) -> Result<&'a mut MultiMap, Stop> {
    let native = c.native;
    match c.vm.heap.native_mut(r)? {
        NativeObject::MultiMap(t) => Ok(t),
        _ => Err(Stop::Native {
            native,
            what: "the receiver is not a multi-map".into(),
        }),
    }
}

/// The key argument, first below the receiver.
fn map_key(c: &mut Call<'_>, r: u32) -> Result<(Item, Params), Stop> {
    let kk = map_mut(c, r)?.key;
    let mut p = Params::new();
    let k = read_item(c, p.take(width(kk)), kk)?;
    Ok((k, p))
}

/// Add an entry after every entry whose key is not greater. No result.
// D2-09100, D2-09171, D2-09322, D2-09379
fn map_insert(c: &mut Call<'_>) -> Result<Status, Stop> {
    with_receiver!(c, r);
    let (kk, vk) = {
        let t = map_mut(c, r)?;
        (t.key, t.value)
    };
    let mut p = Params::new();
    let k = read_item(c, p.take(width(kk)), kk)?;
    let v = read_item(c, p.take(width(vk)), vk)?;
    map_mut(c, r)?.insert(k, v);
    Ok(Status::Success)
}

/// Remove every entry.
// D2-09091, D2-09162, D2-09315, D2-09372
fn map_clear(c: &mut Call<'_>) -> Result<Status, Stop> {
    with_receiver!(c, r);
    map_mut(c, r)?.clear();
    Ok(Status::Success)
}

/// Whether there is no entry: written into the result slot's low byte only, as the engine
/// writes it.
// D2-09303
fn map_empty(c: &mut Call<'_>) -> Result<Status, Stop> {
    with_receiver!(c, r);
    let empty = map_mut(c, r)?.is_empty();
    let before = c.cx.at(-2)?;
    c.write(-2, (before & !0xFF) | u32::from(empty), tag::MIXED)?;
    Ok(Status::Success)
}

/// How many entries have a key.
// D2-09153, D2-09306
fn map_count(c: &mut Call<'_>) -> Result<Status, Stop> {
    with_receiver!(c, r);
    let (k, mut p) = map_key(c, r)?;
    let n = map_mut(c, r)?.count(&k);
    #[allow(clippy::cast_possible_truncation)]
    result(c, &mut p, n as u32)
}

/// Remove the entry an iterator (`fp[-2]`) stands at. No result. The engine unlinks whatever
/// node the iterator holds from this map's tree; one that is past the last, removed already or
/// another map's stops the call here.
// D2-09092, D2-09163
fn map_erase(c: &mut Call<'_>) -> Result<Status, Stop> {
    with_receiver!(c, r);
    let it = c.arg(0)?;
    let (container, at) = iterator(c, it)?;
    if container != r || at == END || !map_mut(c, r)?.remove_node(at) {
        return Err(c.fail("an iterator not at one of this map's entries erased"));
    }
    Ok(Status::Success)
}

/// Remove every entry with a key. No result.
// D2-09101, D2-09172, D2-09323, D2-09380
fn map_remove(c: &mut Call<'_>) -> Result<Status, Stop> {
    with_receiver!(c, r);
    let (k, _) = map_key(c, r)?;
    map_mut(c, r)?.remove_key(&k);
    Ok(Status::Success)
}

/// An iterator at the first entry with a key (past the last when there is none).
// D2-09093, D2-09164, D2-09316, D2-09373
fn map_find(c: &mut Call<'_>) -> Result<Status, Stop> {
    with_receiver!(c, r);
    let (k, mut p) = map_key(c, r)?;
    let at = map_mut(c, r)?.find(&k);
    multi_iterator(c, r, &mut p, at)
}

/// An iterator after a key's last entry, whether the map holds the key or not.
// D2-09094, D2-09165, D2-09317, D2-09374
fn map_find_last(c: &mut Call<'_>) -> Result<Status, Stop> {
    with_receiver!(c, r);
    let (k, mut p) = map_key(c, r)?;
    let at = map_mut(c, r)?.find_last(&k);
    multi_iterator(c, r, &mut p, at)
}

/// An iterator at the first entry.
// D2-09095, D2-09166, D2-09318, D2-09375
fn map_begin(c: &mut Call<'_>) -> Result<Status, Stop> {
    with_receiver!(c, r);
    let at = map_mut(c, r)?.first();
    multi_iterator(c, r, &mut Params::new(), at)
}

/// An iterator past the last entry.
// D2-09096, D2-09167, D2-09319, D2-09376, D2-09088, D2-09159, D2-09312, D2-09369
fn map_end(c: &mut Call<'_>) -> Result<Status, Stop> {
    with_receiver!(c, r);
    map_mut(c, r)?;
    multi_iterator(c, r, &mut Params::new(), None)
}

/// An iterator at the first entry whose key is not less than a key.
// D2-09097, D2-09168, D2-09320, D2-09377
fn map_lower_bound(c: &mut Call<'_>) -> Result<Status, Stop> {
    with_receiver!(c, r);
    let (k, mut p) = map_key(c, r)?;
    let at = map_mut(c, r)?.lower_bound(&k);
    multi_iterator(c, r, &mut p, at)
}

/// An iterator at the first entry whose key is greater than a key.
// D2-09098, D2-09169, D2-09321, D2-09378
fn map_upper_bound(c: &mut Call<'_>) -> Result<Status, Stop> {
    with_receiver!(c, r);
    let (k, mut p) = map_key(c, r)?;
    let at = map_mut(c, r)?.upper_bound(&k);
    multi_iterator(c, r, &mut p, at)
}

/// The map an iterator walks and the entry it stands at (`None`: past the last).
fn map_place(c: &mut Call<'_>, it: u32) -> Result<(u32, Option<u32>), Stop> {
    let (container, at) = iterator(c, it)?;
    let at = (at != END).then_some(at);
    if let Some(id) = at {
        if map_mut(c, container)?.node(id).is_none() {
            return Err(c.fail("an iterator at a removed entry used"));
        }
    }
    Ok((container, at))
}

/// Step back to the entry before; from past the last, to the last entry. The engine steps from
/// the first entry into its tree's insides: that stops the call here.
// D2-09381
fn map_iter_prev(c: &mut Call<'_>) -> Result<Status, Stop> {
    with_receiver!(c, r);
    let (container, at) = map_place(c, r)?;
    match map_mut(c, container)?.before(at).flatten() {
        Some(id) => set_position(c, r, id)?,
        None => return Err(c.fail("an iterator stepped back from the first entry")),
    }
    Ok(Status::Success)
}

/// Step to the entry after (past the last after the last). The engine steps from past the last
/// into its tree's insides: that stops the call here.
// D2-09175
fn map_iter_next(c: &mut Call<'_>) -> Result<Status, Stop> {
    with_receiver!(c, r);
    let (container, at) = map_place(c, r)?;
    let Some(id) = at else {
        return Err(c.fail("an iterator past the last entry stepped on"));
    };
    let next = map_mut(c, container)?.after(id).flatten();
    set_position(c, r, next.unwrap_or(END))?;
    Ok(Status::Success)
}

/// The entry an iterator stands at, which must be there.
fn map_entry(c: &mut Call<'_>, it: u32) -> Result<(Node, Elem, Elem), Stop> {
    let (container, at) = map_place(c, it)?;
    let Some(id) = at else {
        return Err(c.fail("an iterator past the last entry read"));
    };
    let t = map_mut(c, container)?;
    let (kk, vk, node) = (t.key, t.value, t.node(id).copied());
    let node = node.ok_or_else(|| c.fail("an iterator at a removed entry read"))?;
    Ok((node, kk, vk))
}

/// The key of the entry the iterator stands at.
// D2-09174, D2-09325
fn map_iter_key(c: &mut Call<'_>) -> Result<Status, Stop> {
    with_receiver!(c, r);
    let (node, kk, _) = map_entry(c, r)?;
    multi_result(c, node.key, kk)?;
    Ok(Status::Success)
}

/// The value of the entry the iterator stands at.
// D2-09102, D2-09173, D2-09324
fn map_iter_data(c: &mut Call<'_>) -> Result<Status, Stop> {
    with_receiver!(c, r);
    let (node, _, vk) = map_entry(c, r)?;
    multi_result(c, node.value, vk)?;
    Ok(Status::Success)
}

/// Whether two iterators (the receiver and `fp[-2]`) stand at the same place of one map (past the
/// last of one map is its own place): written into the result slot's low byte only.
// D2-09103
fn map_iter_equals(c: &mut Call<'_>) -> Result<Status, Stop> {
    with_receiver!(c, r);
    let other = c.arg(0)?;
    let (ca, a) = iterator(c, r)?;
    let (cb, b) = iterator(c, other)?;
    let same = ca == cb && a == b;
    let before = c.cx.at(-3)?;
    c.write(-3, (before & !0xFF) | u32::from(same), tag::MIXED)?;
    Ok(Status::Success)
}
