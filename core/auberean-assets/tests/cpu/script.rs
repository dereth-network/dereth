//! The script library's sections over bytes built in the test: the version entries, the hash
//! tables' headers, and a package's fields named for saving.
//! Fixture: sections written in the layout the client reads.

use auberean_assets::script::Section;
use auberean_dat::Writer;

/// The version section holds pairs of narrow strings, counted.
///
/// Behaviour: data.script-library.a-version-entry-is-two-strings
#[test]
fn a_version_entry_is_two_strings() {
    let mut w = Writer::new();
    w.u32(2);
    for s in ["client", "1.0", "server", "2.5"] {
        w.narrow_string(s);
    }
    let s = Section::read(5, &w.finish()).expect("the section reads");
    assert_eq!(
        s,
        Section::Versions(vec![
            ("client".into(), "1.0".into()),
            ("server".into(), "2.5".into()),
        ])
    );
}

/// A hash table's header counts its entries in its low 24 bits; the top byte only sizes the
/// table's buckets.
///
/// Behaviour: data.script-library.a-hash-tables-count-is-24-bits
#[test]
fn a_hash_tables_count_is_24_bits() {
    let n: u32 = 0x1_0002;
    let mut w = Writer::new();
    w.u32(n | (9 << 24));
    for i in 0..n {
        w.u32(i);
        w.u32(i ^ 0xFF);
    }
    let Section::ServerCallable(v) = Section::read(7, &w.finish()).expect("the section reads")
    else {
        panic!("not the callable table");
    };
    assert_eq!(v.len(), 0x1_0002);
    assert_eq!(v[0x1_0001], (0x1_0001, 0x1_00FE));
}

/// A package's fields named for saving are counted in a full word, each a name, a slot, and a
/// tag of one byte; the bytes after the tag up to the next multiple of four are passed over.
///
/// Behaviour: data.script-library.a-saved-fields-tag-is-one-byte
#[test]
fn a_saved_fields_tag_is_one_byte() {
    let mut w = Writer::new();
    w.u32(1); // one package
    w.narrow_string("Pkg");
    w.narrow_string("");
    for v in [0, 4, 0, 1] {
        w.u32(v); // flags, size, checksum, id
    }
    w.i32(-1); // no base
    w.u32(1); // one saved field
    w.narrow_string("hp");
    w.u32(3);
    w.bytes(&[7, 0xEE, 0xEE, 0xEE]);
    w.u32(0); // no functions
    let Section::Exports(v) = Section::read(4, &w.finish()).expect("the section reads") else {
        panic!("not the packages");
    };
    assert_eq!(v[0].checkpoints, vec![("hp".to_string(), 3, 7)]);
}
