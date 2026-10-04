//! Vectors: local world.pack record and corruption cases in this module
//! World.pack container: header offsets, lookups/dedupe, duplicate keys refused, hash and
//! structural damage fail open, exact record decode, atomic mapped write.
//! Fixture: synthetic world records, SQL dumps and JSON documents.

use empyrean_content::pack::format::{
    PackHeader, FORMAT_VERSION, HASH_COVERAGE_START, HEADER_LEN, INDEX_ENTRY_LEN, MAGIC,
    SCHEMA_VERSION,
};
use empyrean_content::pack::{decode, encode, Pack, PackWriter, TableId};
use empyrean_content::{ImportError, PackError};

fn sample() -> Vec<u8> {
    let mut w = PackWriter::new([7; 16], 42);
    // Added out of order; the layout sorts by (table, key).
    w.add(TableId::QUEST, 9, &"nine".to_owned()).unwrap();
    w.add(TableId::EVENT, 3, &"three".to_owned()).unwrap();
    w.add(TableId::QUEST, 2, &"two".to_owned()).unwrap();
    // Byte-identical to QUEST 2: stored once.
    w.add(TableId::QUEST, 5, &"two".to_owned()).unwrap();
    w.finish().unwrap().0
}

#[test]
fn header_fields_sit_at_their_offsets() {
    let bytes = sample();
    assert_eq!(&bytes[0..8], b"ERE_PACK");
    assert_eq!(&bytes[0..8], &MAGIC.to_le_bytes());
    let u32_at = |o: usize| u32::from_le_bytes(bytes[o..o + 4].try_into().unwrap());
    let u64_at = |o: usize| u64::from_le_bytes(bytes[o..o + 8].try_into().unwrap());
    assert_eq!(u32_at(0x08), FORMAT_VERSION);
    assert_eq!(u32_at(0x0C), SCHEMA_VERSION);
    assert_eq!(u32_at(0x14), HEADER_LEN);
    assert_eq!(u32_at(0x18), 2, "two tables used");
    assert_eq!(u32_at(0x1C), INDEX_ENTRY_LEN);
    assert_eq!(u64_at(0x28), 4, "index count");
    assert_eq!(u64_at(0x20) % 8, 0);
    assert_eq!(u64_at(0x30) % 8, 0);
    assert_eq!(&bytes[0x60..0x70], &[7u8; 16], "dataset id");
    assert_eq!(u32_at(0x70), 42, "importer version");
    let h = PackHeader::read(&bytes).unwrap();
    assert_eq!(&bytes[0x40..HASH_COVERAGE_START], &h.content_hash);
    Pack::from_bytes(bytes.clone())
        .unwrap()
        .verify_hash()
        .unwrap();
}

#[test]
fn lookups_iteration_and_dedupe() {
    let pack = Pack::from_bytes(sample()).unwrap();
    pack.verify_hash().unwrap();
    assert_eq!(
        pack.get::<String>(TableId::QUEST, 9).unwrap().as_deref(),
        Some("nine")
    );
    assert_eq!(pack.get::<String>(TableId::QUEST, 3).unwrap(), None);
    assert_eq!(
        pack.get::<String>(TableId::EVENT, 3).unwrap().as_deref(),
        Some("three")
    );
    assert_eq!(pack.keys(TableId::QUEST), [2, 5, 9]);
    assert_eq!(pack.count(TableId::QUEST), 3);
    assert_eq!(pack.count(TableId::SPELL), 0);
    let all: Vec<(u64, String)> = pack.all(TableId::QUEST).unwrap();
    assert_eq!(
        all.iter()
            .map(|(k, v)| (*k, v.as_str()))
            .collect::<Vec<_>>(),
        [(2, "two"), (5, "two"), (9, "nine")]
    );
    let raw2 = pack.raw(TableId::QUEST, 2).unwrap().unwrap().as_ptr();
    let raw5 = pack.raw(TableId::QUEST, 5).unwrap().unwrap().as_ptr();
    assert_eq!(raw2, raw5, "identical records share one blob");
}

#[test]
fn duplicate_keys_are_refused() {
    let mut w = PackWriter::new([0; 16], 1);
    w.add(TableId::SPELL, 1, &1u32).unwrap();
    w.add(TableId::SPELL, 1, &2u32).unwrap();
    assert!(matches!(
        w.finish(),
        Err(ImportError::DuplicateKey { table: 11, key: 1 })
    ));
}

#[test]
fn a_changed_byte_fails_the_hash_and_structural_damage_fails_open() {
    let good = sample();
    let mut bad = good.clone();
    let last = bad.len() - 1;
    bad[last] ^= 0xFF;
    let pack = Pack::from_bytes(bad).unwrap();
    assert!(matches!(
        pack.verify_hash(),
        Err(PackError::HashMismatch { .. })
    ));

    let mut magic = good.clone();
    magic[0] = b'X';
    assert!(matches!(
        Pack::from_bytes(magic),
        Err(PackError::BadMagic(_))
    ));

    let mut schema = good.clone();
    schema[0x0C] = schema[0x0C].wrapping_add(1);
    assert!(matches!(
        Pack::from_bytes(schema),
        Err(PackError::SchemaVersion { .. })
    ));

    let truncated = good[..good.len() - 1].to_vec();
    assert!(matches!(
        Pack::from_bytes(truncated),
        Err(PackError::Truncated {
            region: "blobs",
            ..
        })
    ));

    assert!(matches!(
        Pack::from_bytes(good[..64].to_vec()),
        Err(PackError::Truncated {
            region: "header",
            ..
        })
    ));
}

#[test]
fn a_record_must_decode_exactly() {
    let bytes = encode(&7u32);
    assert_eq!(decode::<u32>(&bytes).unwrap(), 7);
    assert!(matches!(
        decode::<u16>(&bytes),
        Err(PackError::Shortfall(2))
    ));
    assert!(matches!(decode::<u64>(&bytes), Err(PackError::Overrun(4))));
    let s = encode(&Some("héllo".to_owned()));
    assert_eq!(
        decode::<Option<String>>(&s).unwrap().as_deref(),
        Some("héllo")
    );
    assert_eq!(
        decode::<Option<String>>(&encode::<Option<String>>(&None)).unwrap(),
        None
    );
}

#[test]
fn a_pack_on_disk_is_mapped_and_written_atomically() {
    let dir =
        std::env::temp_dir().join(format!("empyrean-content-pack-test-{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    let path = dir.join("t.pack");
    empyrean_content::pack::write_atomically(&path, &sample()).unwrap();
    assert!(!dir.join("t.pack.partial").exists());
    let pack = Pack::open(&path).unwrap();
    pack.verify_hash().unwrap();
    assert_eq!(
        pack.get::<String>(TableId::QUEST, 9).unwrap().as_deref(),
        Some("nine")
    );
    drop(pack);
    std::fs::remove_dir_all(&dir).unwrap();
}
