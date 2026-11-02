//! What the data-file writer's tests share: a file in memory that can die part way, the client's
//! test clock, and the seeded operations the client's own runs were given.
//! Behaviour: none (test support)

// The files and records built here are small: every length and offset fits in 32 bits.
#![allow(clippy::cast_possible_truncation, dead_code)]

use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::Arc;

use auberean_dat::disk::{Files, OpenMode, OpenSpec, Store};
use dereth_dat::{DatStorageMut, MemoryFile};

/// One file in memory; a write past `limit` (counted over the file's life) is lost, as when the
/// process dies there.
#[derive(Debug, Clone, Default)]
pub(crate) struct Crashing {
    pub file: MemoryFile,
    pub writes: Arc<AtomicUsize>,
    pub limit: Arc<AtomicUsize>,
}

impl DatStorageMut for Crashing {
    fn read_exact_at(&mut self, offset: u64, buf: &mut [u8]) -> std::io::Result<()> {
        DatStorageMut::read_exact_at(&mut self.file, offset, buf)
    }

    fn write_all_at(&mut self, offset: u64, buf: &[u8]) -> std::io::Result<()> {
        let n = self.writes.fetch_add(1, Ordering::Relaxed) + 1;
        if n <= self.limit.load(Ordering::Relaxed) {
            self.file.write_all_at(offset, buf)?;
        }
        Ok(())
    }
}

/// The host's files: one, in memory.
#[derive(Debug)]
pub(crate) struct OneFile(pub Crashing);

impl OneFile {
    pub fn new(bytes: &[u8]) -> Self {
        let c = Crashing {
            limit: Arc::new(AtomicUsize::new(usize::MAX)),
            ..Crashing::default()
        };
        c.file.clone().write_all_at(0, bytes).unwrap();
        Self(c)
    }

    pub fn bytes(&self) -> Vec<u8> {
        self.0.file.bytes()
    }

    pub fn writes(&self) -> usize {
        self.0.writes.load(Ordering::Relaxed)
    }

    pub fn die_after(&self, more: usize) {
        self.0.limit.store(self.writes() + more, Ordering::Relaxed);
    }
}

impl Files for OneFile {
    fn open(&mut self, _name: &str, mode: OpenMode) -> Option<Box<dyn DatStorageMut>> {
        if mode == OpenMode::Create {
            self.0.file = MemoryFile::default();
        }
        Some(Box::new(self.0.clone()))
    }
}

/// A store dating records from the client's test clock on.
pub(crate) fn store() -> Store {
    let mut t = 0x5000_0000u32;
    Store::new(Box::new(move || {
        t += 1;
        t - 1
    }))
}

pub(crate) fn spec(flags: u32, file_size: u32, block_size: u32) -> OpenSpec {
    OpenSpec::new(
        "case.dat", "dir", 2, 7, flags, file_size, block_size, -1, -1, false,
    )
}

/// xorshift32.
struct Xorshift(u32);

impl Xorshift {
    fn next(&mut self) -> u32 {
        let mut x = self.0;
        x ^= x << 13;
        x ^= x >> 17;
        x ^= x << 5;
        self.0 = x;
        x
    }
}

#[derive(Debug, Clone, Copy)]
pub(crate) enum Op {
    Save(u32, u32, u32),
    Delete(u32),
    Mask(u32, u32),
}

/// A case's operations: mostly saves (60 in 100); when shrinking, 90 in 100 for the first half
/// and 5 in 100 after; else removals, and 2 in 100 removals by mask.
pub(crate) fn operations(seed: u32, count: u32, space: u32, size: u32, shrink: bool) -> Vec<Op> {
    let mut r = Xorshift(seed);
    (0..count)
        .map(|i| {
            let k = r.next() % 100;
            let id = 0x0600_0000 + r.next() % space;
            let saves = match (shrink, i < count / 2) {
                (false, _) => 60,
                (true, true) => 90,
                (true, false) => 5,
            };
            if k < saves {
                Op::Save(id, 1 + r.next() % size, i & 0xFF)
            } else if k < 98 {
                Op::Delete(id)
            } else {
                Op::Mask(id, 0xFFFF_FF00)
            }
        })
        .collect()
}

pub(crate) fn payload(id: u32, n: u32, salt: u32) -> Vec<u8> {
    (0..n)
        .map(|k| (id.wrapping_mul(31).wrapping_add(k * 7).wrapping_add(salt) & 0xFF) as u8)
        .collect()
}

pub(crate) fn apply(s: &mut Store, ops: &[Op]) -> Vec<bool> {
    ops.iter()
        .map(|op| match *op {
            Op::Save(id, n, salt) => s.save_data(id, &payload(id, n, salt)),
            Op::Delete(id) => s.delete_data(id),
            Op::Mask(id, mask) => s.delete_data_by_mask(id, mask),
        })
        .collect()
}

pub(crate) fn fnv(b: &[u8]) -> u64 {
    let mut h = 0xCBF2_9CE4_8422_2325u64;
    for &x in b {
        h ^= u64::from(x);
        h = h.wrapping_mul(0x100_0000_01B3);
    }
    h
}

/// The file's digest, its stamp's leading word set to 0.
pub(crate) fn digest(mut b: Vec<u8>) -> u64 {
    if b.len() >= 0x16C {
        b[0x168..0x16C].fill(0);
    }
    fnv(&b)
}
