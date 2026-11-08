//! Behaviour: none (an art pack source's list, read by the launcher)

use super::{is_newer, Catalog, CatalogError, Entry, Mismatch, FORMAT, MAX_PACK_BYTES};
use dereth_assets::art_pack::About;
use dereth_assets::packed_art::{Contents, Packed};

const SHA: &str = "0123456789abcdef0123456789abcdef0123456789abcdef0123456789abcdef";

fn packed(id: &str, version: &str, hash: &str) -> Packed {
    Packed {
        contents: Contents {
            format: 1,
            name: "Holtburg".into(),
            hash: hash.into(),
            records: Vec::new(),
        },
        manifest: format!(
            "[pack]\nname = \"Holtburg\"\nformat = 1\nid = \"{id}\"\nversion = \"{version}\"\n\
             description = \"Holtburg's houses.\"\nauthors = [\"Ann\"]\nlicense = \"CC-BY-4.0\"\n\
             made_for = [\"end-of-retail\", \"1999\"]\n"
        ),
    }
}

fn entry() -> Entry {
    Entry::for_packed(
        &packed("com.example.holtburg", "1.2.0", "feedface"),
        &About::default(),
        "holtburg/holtburg-1.2.0.dat",
        1234,
        &SHA.to_ascii_uppercase(),
        " Roofs. ",
        "2026-11-08",
    )
    .expect("an entry")
}

#[test]
fn an_entry_is_made_from_the_packed_manifest_and_reads_back_from_the_list() {
    let e = entry();
    assert_eq!(e.id, "com.example.holtburg");
    assert_eq!(e.name, "Holtburg");
    assert_eq!(e.version, "1.2.0");
    assert_eq!(e.authors, ["Ann"]);
    assert_eq!(e.license, "CC-BY-4.0");
    assert_eq!(e.made_for, ["end-of-retail", "1999"]);
    assert_eq!(e.sha256, SHA, "lower-case");
    assert_eq!(e.hash, "feedface");
    assert_eq!(e.notes, "Roofs.");
    let list = Catalog {
        name: "Example".into(),
        packs: vec![e.clone()],
    };
    let read = Catalog::parse(list.to_json().as_bytes()).expect("reads");
    assert!(read.skipped.is_empty(), "{:?}", read.skipped);
    assert_eq!(read.catalog, list);
}

#[test]
fn a_pack_is_listed_by_its_manifests_id_and_version_or_the_sources_word_for_them() {
    let none = About::default();
    let given = About {
        id: "a.b".into(),
        version: "1.0.0".into(),
        license: "CC0-1.0".into(),
        ..About::default()
    };
    let mut p = packed("com.example.holtburg", "1.2.0", "h");
    p.manifest = "[pack]\nname = \"x\"\nformat = 1\nversion = \"1.0.0\"\n".into();
    assert!(
        Entry::for_packed(&p, &none, "f.dat", 1, SHA, "", "").is_err(),
        "no id"
    );
    p.manifest = "[pack]\nname = \"x\"\nformat = 1\nid = \"a.b\"\n".into();
    assert!(
        Entry::for_packed(&p, &none, "f.dat", 1, SHA, "", "").is_err(),
        "no version"
    );
    p.manifest = "[pack]\nname = \"x\"\nformat = 1\n".into();
    let e = Entry::for_packed(&p, &given, "f.dat", 1, SHA, "", "").expect("the source's word");
    assert_eq!(
        (e.id.as_str(), e.version.as_str(), e.license.as_str()),
        ("a.b", "1.0.0", "CC0-1.0")
    );
    assert_eq!(e.check_packed(&p), Ok(()), "taken by its content hash");
    let mut other = p.clone();
    other.contents.hash = "other".into();
    assert!(e.check_packed(&other).is_err());
    p.manifest = "[pack]\nname = \"x\"\nformat = 1\nid = \"a.c\"\n".into();
    assert!(
        Entry::for_packed(&p, &given, "f.dat", 1, SHA, "", "").is_err(),
        "the two disagree"
    );
}

#[test]
fn a_list_keeps_the_entries_that_read_and_says_why_each_other_did_not() {
    let good = |id: &str| {
        format!(
            r#"{{"id": "{id}", "version": "1.0.0", "file": "a.dat", "sha256": "{SHA}", "size": 10}}"#
        )
    };
    let text = format!(
        r#"{{"format": 1, "name": " Ours ", "extra": true, "packs": [
            {},
            {},
            {{"id": "Not An Id", "version": "1.0.0", "file": "a.dat", "sha256": "{SHA}", "size": 10}},
            {{"id": "a.c", "version": "1.0", "file": "a.dat", "sha256": "{SHA}", "size": 10}},
            {{"id": "a.d", "version": "1.0.0", "file": "a.dat", "sha256": "abc", "size": 10}},
            {{"id": "a.e", "version": "1.0.0", "file": "a.dat", "sha256": "{SHA}"}},
            {{"id": "a.f", "version": "1.0.0", "file": "a.dat", "sha256": "{SHA}", "size": {}}},
            {{"id": "a.g", "version": "1.0.0", "sha256": "{SHA}", "size": 10}},
            {},
            "a string"
        ]}}"#,
        good("a.b"),
        good("a.b"),
        MAX_PACK_BYTES + 1,
        good("a.h"),
    );
    let read = Catalog::parse(text.as_bytes()).expect("reads");
    assert_eq!(read.catalog.name, "Ours");
    let ids: Vec<&str> = read.catalog.packs.iter().map(|p| p.id.as_str()).collect();
    assert_eq!(ids, ["a.b", "a.h"]);
    assert_eq!(read.skipped.len(), 8, "{:?}", read.skipped);
    assert!(read.skipped[0].contains("more than once"));
    assert!(read
        .skipped
        .iter()
        .any(|s| s.contains("not an art pack id")));
    assert!(read.skipped.iter().any(|s| s.starts_with("a.e: no size")));
    assert!(read.skipped.iter().any(|s| s.starts_with("a.g: no file")));
    assert_eq!(
        read.catalog.get("a.h").map(|p| p.name.as_str()),
        Some("a.h")
    );
}

#[test]
fn a_list_of_a_newer_format_or_of_no_packs_is_refused_whole() {
    let newer = format!(r#"{{"format": {}, "packs": []}}"#, FORMAT + 1);
    assert_eq!(
        Catalog::parse(newer.as_bytes()),
        Err(CatalogError::NewerFormat(FORMAT + 1))
    );
    assert!(matches!(
        Catalog::parse(br#"{"format": 1, "plugins": []}"#),
        Err(CatalogError::NotAList(_))
    ));
    assert!(matches!(
        Catalog::parse(b"not json"),
        Err(CatalogError::NotAList(_))
    ));
    assert_eq!(
        Catalog::parse(&vec![b' '; super::MAX_BYTES + 1]),
        Err(CatalogError::TooLarge)
    );
}

#[test]
fn a_download_is_refused_unless_its_size_and_sha256_are_the_entrys() {
    let e = entry();
    assert_eq!(e.check_download(1234, SHA), Ok(()));
    assert_eq!(e.check_download(1234, &SHA.to_ascii_uppercase()), Ok(()));
    assert_eq!(
        e.check_download(1233, SHA),
        Err(Mismatch::Size {
            expected: 1234,
            got: 1233
        })
    );
    let other = SHA.replace('0', "f");
    assert_eq!(
        e.check_download(1234, &other),
        Err(Mismatch::Hash {
            expected: SHA.into(),
            got: other.clone()
        })
    );
    assert!(e
        .check_download(1234, &other)
        .unwrap_err()
        .to_string()
        .contains("SHA-256"));
}

#[test]
fn a_download_is_refused_unless_the_pack_in_it_is_the_one_listed() {
    let e = entry();
    assert_eq!(
        e.check_packed(&packed("com.example.holtburg", "1.2.0", "feedface")),
        Ok(())
    );
    let says = |p: Packed| match e.check_packed(&p) {
        Err(Mismatch::Says { field, .. }) => field,
        other => panic!("{other:?}"),
    };
    assert_eq!(says(packed("com.example.other", "1.2.0", "feedface")), "id");
    assert_eq!(
        says(packed("com.example.holtburg", "1.3.0", "feedface")),
        "version"
    );
    assert_eq!(
        says(packed("com.example.holtburg", "1.2.0", "beef")),
        "content hash"
    );
    let mut broken = packed("com.example.holtburg", "1.2.0", "feedface");
    broken.manifest = "not toml [".into();
    assert!(matches!(
        e.check_packed(&broken),
        Err(Mismatch::NotAPack(_))
    ));
}

#[test]
fn a_version_is_newer_only_by_its_numbers() {
    assert!(is_newer("1.10.0", "1.9.9"));
    assert!(is_newer("2.0.0", "1.99.99"));
    assert!(!is_newer("1.2.0", "1.2.0"));
    assert!(!is_newer("1.1.9", "1.2.0"));
    assert!(!is_newer("1.2", "1.0.0"));
    assert!(!is_newer("1.2.0", "what"));
}
