//! The windows' saved places as the engine holds them: a hash table from a window (the layout its
//! panel is made from) to a hash table from an element of it to the element's place.
//!
//! A place is the element's corners in clip space (left, top, right, bottom: -1 to 1 across the
//! display, the vertical turned over) and whether it shows; it packs as those four floats and a
//! word, in that order. A new place is hidden with every corner at -3, off the display.
//!
//! Each table walks bucket by bucket, each chain from its head, and packs in that order, after a
//! header word holding its entry count and, in the top byte, its bucket count's place in
//! [`BUCKET_SIZES`]. A new table asks for 32 buckets, which become 47. A place added goes to the
//! head of its chain; an element already held refuses it. Copying a table into another walks the
//! source and puts each entry at the head of its chain in the target's buckets, so each chain of
//! the copy is the source's reversed; a key already there keeps the first.

use crate::native_object::BUCKET_SIZES;
use crate::wire::{In, Out};

/// A table's buckets of keyed entries.
pub type Buckets<T> = Vec<Vec<(u32, T)>>;

/// The bucket count's place in [`BUCKET_SIZES`] of a new table (the 32 asked for become 47).
pub const NEW_TABLE: u8 = 2;

/// A new place: hidden, every corner at -3.
pub const NEW_PLACE: [u32; 5] = [0xC040_0000, 0xC040_0000, 0xC040_0000, 0xC040_0000, 0];

/// Copy entries (in the source's order) into a table of `n` buckets, emptied first: each entry
/// goes to the head of its chain; a key already there keeps the first.
pub fn copy_into<T: Clone>(n: usize, from: impl IntoIterator<Item = (u32, T)>) -> Buckets<T> {
    let mut b: Buckets<T> = vec![Vec::new(); n];
    for (k, v) in from {
        let chain = &mut b[k as usize % n];
        if chain.iter().all(|(x, _)| *x != k) {
            chain.insert(0, (k, v));
        }
    }
    b
}

/// The bucket count of a size place.
#[must_use]
pub fn buckets_of(place: u8) -> usize {
    BUCKET_SIZES.get(usize::from(place)).copied().unwrap_or(11) as usize
}

/// A table of the windows' saved places: window to (element to place). A copy is a copy of every
/// window's table.
// D2-13087, D2-13086, D2-01960
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct UiLocations {
    pub place: u8,
    pub entries: Buckets<Window>,
}

/// One window's elements and their places (four floats and a shown word).
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Window {
    pub place: u8,
    pub entries: Buckets<[u32; 5]>,
}

impl Default for UiLocations {
    fn default() -> Self {
        Self::new()
    }
}

impl UiLocations {
    /// A new table: no window, in 47 buckets.
    #[must_use]
    pub fn new() -> Self {
        Self {
            place: NEW_TABLE,
            entries: vec![Vec::new(); buckets_of(NEW_TABLE)],
        }
    }

    /// Read a packed table; `None` when it does not read to its end.
    // D2-01954, D2-01948
    #[must_use]
    pub fn read(bytes: &[u8]) -> Option<Self> {
        let mut r = In::new(bytes);
        let header = r.u32().ok()?;
        #[allow(clippy::cast_possible_truncation)]
        let place = (header >> 24) as u8;
        let mut entries: Buckets<Window> = vec![Vec::new(); buckets_of(place)];
        for _ in 0..(header & 0x00FF_FFFF) {
            let k = r.u32().ok()?;
            let h = r.u32().ok()?;
            #[allow(clippy::cast_possible_truncation)]
            let p = (h >> 24) as u8;
            let mut inner: Buckets<[u32; 5]> = vec![Vec::new(); buckets_of(p)];
            for _ in 0..(h & 0x00FF_FFFF) {
                let e = r.u32().ok()?;
                let mut d = [0; 5];
                for x in &mut d {
                    *x = r.u32().ok()?;
                }
                let n = inner.len();
                inner[e as usize % n].push((e, d));
            }
            let n = entries.len();
            entries[k as usize % n].push((
                k,
                Window {
                    place: p,
                    entries: inner,
                },
            ));
        }
        r.at_end().then_some(Self { place, entries })
    }

    /// The packed form.
    // D2-01954, D2-01948
    #[must_use]
    pub fn write(&self) -> Vec<u8> {
        fn count<T>(b: &Buckets<T>) -> usize {
            b.iter().map(Vec::len).sum()
        }
        let mut w = Out::new();
        #[allow(clippy::cast_possible_truncation)]
        w.u32(count(&self.entries) as u32 | u32::from(self.place) << 24);
        for (k, inner) in self.entries.iter().flatten() {
            w.u32(*k);
            #[allow(clippy::cast_possible_truncation)]
            w.u32(count(&inner.entries) as u32 | u32::from(inner.place) << 24);
            for (e, d) in inner.entries.iter().flatten() {
                w.u32(*e);
                d.iter().for_each(|x| w.u32(*x));
            }
        }
        w.buf
    }

    /// This table copied over `into`: `into`'s buckets kept, each window a new table of the
    /// source's size holding the source's elements as copied.
    #[must_use]
    pub fn copied_over(&self, into: &Self) -> Self {
        let from = self.entries.iter().flatten().map(|(k, inner)| {
            let n = buckets_of(inner.place);
            let copied = Window {
                place: inner.place,
                entries: copy_into(n, inner.entries.iter().flatten().cloned()),
            };
            (*k, copied)
        });
        Self {
            place: into.place,
            entries: copy_into(buckets_of(into.place), from),
        }
    }

    /// Keep an element's place in a window: the window's elements so far copied into a new table
    /// of 47 buckets, the place added at the head of its chain, and that table copied in as the
    /// window's (over the window's own table, which keeps its buckets; or, for a new window, as a
    /// new table at the head of the window's chain). False when the window already holds the
    /// element.
    // D2-13082
    pub fn add_location(&mut self, window: u32, element: u32, place: [u32; 5]) -> bool {
        let n = self.entries.len();
        let chain = &mut self.entries[window as usize % n];
        let held = chain.iter().position(|(k, _)| *k == window);
        let mut local = copy_into(
            buckets_of(NEW_TABLE),
            held.into_iter()
                .flat_map(|i| chain[i].1.entries.iter().flatten().cloned()),
        );
        let m = local.len();
        let at = &mut local[element as usize % m];
        if at.iter().any(|(e, _)| *e == element) {
            return false;
        }
        at.insert(0, (element, place));
        let walked = local.into_iter().flatten();
        match held {
            Some(i) => {
                let w = &mut chain[i].1;
                w.entries = copy_into(buckets_of(w.place), walked);
            }
            None => chain.insert(
                0,
                (
                    window,
                    Window {
                        place: NEW_TABLE,
                        entries: copy_into(buckets_of(NEW_TABLE), walked),
                    },
                ),
            ),
        }
        true
    }

    /// An element's kept place in a window.
    // D2-13077
    #[must_use]
    pub fn location(&self, window: u32, element: u32) -> Option<[u32; 5]> {
        let n = self.entries.len();
        let (_, w) = self.entries[window as usize % n]
            .iter()
            .find(|(k, _)| *k == window)?;
        let m = w.entries.len();
        w.entries[element as usize % m]
            .iter()
            .find(|(e, _)| *e == element)
            .map(|(_, d)| *d)
    }

    /// Every kept place: window, element, place; in walking order.
    pub fn iter(&self) -> impl Iterator<Item = (u32, u32, [u32; 5])> + '_ {
        self.entries
            .iter()
            .flatten()
            .flat_map(|(k, w)| w.entries.iter().flatten().map(move |(e, d)| (*k, *e, *d)))
    }
}
