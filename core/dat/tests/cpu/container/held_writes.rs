//! Behaviour: none (checks formats, geometry, numeric contracts or repository tools)
//! The writer hands its storage a run of writes that each take up where the last left off as one
//! write, in the order they were made: a long record costs a handful of writes rather than one a
//! block, and the header still reaches the storage before the record's last block.
//! Fixture: containers made in memory with the dat writer, through a storage that records every
//! write.

use std::path::PathBuf;
use std::sync::{Arc, Mutex};

use dereth_dat::container::{DatFile, DatStorage};
use dereth_dat::folder::DatStorageMut;
use dereth_dat::write::DatWriter;
use dereth_dat::ContainerLayout;
use dereth_primitives::DataId;

const BLOCK: u32 = 0x100;
const ID: DataId = DataId(0x0600_0001);

#[derive(Debug, Default)]
struct Recorded {
    bytes: Vec<u8>,
    /// Every write the storage was handed, as its offset and length.
    writes: Vec<(u64, usize)>,
}

/// Bytes in memory, remembering every write.
#[derive(Debug, Clone, Default)]
struct Recording(Arc<Mutex<Recorded>>);

impl DatStorageMut for Recording {
    fn read_exact_at(&mut self, offset: u64, buf: &mut [u8]) -> std::io::Result<()> {
        let r = self.0.lock().unwrap();
        let at = usize::try_from(offset).unwrap();
        let src = r
            .bytes
            .get(at..at + buf.len())
            .ok_or(std::io::ErrorKind::UnexpectedEof)?;
        buf.copy_from_slice(src);
        Ok(())
    }

    fn write_all_at(&mut self, offset: u64, buf: &[u8]) -> std::io::Result<()> {
        let mut r = self.0.lock().unwrap();
        let at = usize::try_from(offset).unwrap();
        if r.bytes.len() < at + buf.len() {
            r.bytes.resize(at + buf.len(), 0);
        }
        r.bytes[at..at + buf.len()].copy_from_slice(buf);
        r.writes.push((offset, buf.len()));
        Ok(())
    }
}

impl DatStorage for Recording {
    fn read_exact_at(&self, offset: u64, buf: &mut [u8]) -> std::io::Result<()> {
        let r = self.0.lock().unwrap();
        let at = usize::try_from(offset).unwrap();
        let src = r
            .bytes
            .get(at..at + buf.len())
            .ok_or(std::io::ErrorKind::UnexpectedEof)?;
        buf.copy_from_slice(src);
        Ok(())
    }
}

fn made() -> (Recording, DatWriter) {
    let storage = Recording::default();
    let w = DatWriter::create_in(
        PathBuf::from("held.dat"),
        Box::new(storage.clone()),
        BLOCK,
        1,
        0,
        0x400 + BLOCK * 64,
    )
    .unwrap();
    (storage, w)
}

fn payload(n: usize) -> Vec<u8> {
    (0..n).map(|i| u8::try_from(i % 251).unwrap()).collect()
}

/// A record about a thousand blocks long, saved into a container that must grow by a
/// megabyte of blocks to hold it, reaches the storage in a handful of writes, reads back whole
/// through the writer and through an ordinary reader, and the writer counts what it handed over.
#[test]
fn a_record_many_blocks_long_goes_to_the_storage_in_a_handful_of_writes() {
    let (storage, mut w) = made();
    let before = storage.0.lock().unwrap().writes.len();
    assert_eq!(
        w.storage_writes(),
        u64::try_from(before).unwrap(),
        "the writer counts each write it hands over"
    );
    let bytes = payload(1 << 18);
    w.save(ID, &bytes, 1, 1, 7).unwrap();
    let writes = storage.0.lock().unwrap().writes.len() - before;
    let blocks = bytes.len().div_ceil(BLOCK as usize - 4);
    assert!(blocks > 1000, "{blocks} blocks");
    assert!(
        writes <= 16,
        "{writes} writes for a record of {blocks} blocks and a growth of the file"
    );
    assert_eq!(w.storage_writes(), u64::try_from(before + writes).unwrap());
    assert_eq!(
        w.read(ID).unwrap(),
        bytes,
        "the writer reads its own writes"
    );
    drop(w);
    let file = DatFile::from_storage(PathBuf::from("held.dat"), Box::new(storage)).unwrap();
    assert_eq!(file.read(ID).unwrap(), bytes, "an ordinary reader reads it");
}

/// A record three blocks long, in a fresh container's free blocks in a row: its first two blocks
/// go as one write, then the header naming the free list's new head, then its last block, so a
/// process that stops between them leaves the header behind the record, as when each write went
/// alone.
#[test]
fn the_header_still_reaches_the_storage_before_a_records_last_block() {
    let (storage, mut w) = made();
    let before = storage.0.lock().unwrap().writes.len();
    let per = BLOCK as usize - 4;
    let bytes = payload(per * 3);
    w.save(ID, &bytes, 1, 1, 7).unwrap();
    let head = u64::from(w.entry(ID).unwrap().expect("the record is there").offset);
    let writes = storage.0.lock().unwrap().writes[before..].to_vec();
    let header = ContainerLayout::MODERN.header.at;
    let covers = |at: u64| {
        writes
            .iter()
            .position(|&(o, n)| o <= at && at < o + n as u64)
            .unwrap_or_else(|| panic!("no write covers {at:#x}: {writes:x?}"))
    };
    let first = covers(head);
    assert_eq!(
        writes[first],
        (head, 2 * BLOCK as usize),
        "the first two blocks go as one write: {writes:x?}"
    );
    let last = covers(head + 2 * u64::from(BLOCK));
    let header_write = writes
        .iter()
        .position(|&(o, n)| o <= header && header < o + n as u64)
        .expect("the header is written");
    assert!(
        first < header_write && header_write < last,
        "chain, header, last block: {writes:x?}"
    );
}
