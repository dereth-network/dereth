//! Behaviour: none (checks formats, geometry, numeric contracts or repository tools)
//! One open container built by DatWriter serves concurrent readers on many threads, each their own
//! record.
//! Fixture: the shipped retail DAT records and recorded inputs.

use dereth_dat::container::DatFile;
use dereth_dat::write::DatWriter;
use dereth_primitives::DataId;

/// A payload that names its record and spans several 0x400-byte blocks, so a read that took
/// another thread's cursor position would come back with the wrong bytes.
fn payload(i: u32) -> Vec<u8> {
    let len = 0x900 + i * 7;
    (0..len)
        .map(|k| (i.wrapping_mul(31).wrapping_add(k) & 0xFF).to_le_bytes()[0])
        .collect()
}

#[test]
fn one_open_container_serves_concurrent_readers_their_own_records() {
    let dir = std::env::temp_dir().join("dereth_dat_positional_reads");
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).expect("a scratch directory");
    let path = dir.join("positional.dat");

    const RECORDS: u32 = 24;
    {
        let mut w = DatWriter::create(&path, 0x400, 1, 0, 0x400 + 400 * 0x400).expect("create");
        for i in 0..RECORDS {
            w.save(DataId(0x0600_0001 + i), &payload(i), 2, 1, 1_700_000_000)
                .expect("save");
        }
    }

    let file = DatFile::open(&path).expect("open");
    std::thread::scope(|s| {
        for t in 0..8u32 {
            let file = &file;
            s.spawn(move || {
                for round in 0..50u32 {
                    let i = (t * 7 + round * 5) % RECORDS;
                    let got = file.read(DataId(0x0600_0001 + i)).expect("read");
                    assert_eq!(got, payload(i), "thread {t} round {round}: record {i}");
                }
            });
        }
    });
    let _ = std::fs::remove_dir_all(&dir);
}
