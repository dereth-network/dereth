use super::{resolve, Catalog, CatalogError, Entry, Mismatch, FORMAT};
use crate::Package;

fn package(id: &str, version: &str, api: &str, extra: &str) -> Package {
    let manifest = format!(
        "id = \"{id}\"\nname = \"Meter\"\nversion = \"{version}\"\napi = \"{api}\"\n{extra}"
    );
    Package {
        origin: String::new(),
        files: [
            ("plugin.toml".to_owned(), manifest.into_bytes()),
            ("main.lua".to_owned(), b"print(1)".to_vec()),
        ]
        .into(),
    }
}

const USES: &str =
    "[permissions]\n\"chat.send\" = \"Says hello.\"\n[net]\ndomains = [\"example.org\"]\n";

fn zip() -> Vec<u8> {
    package("org.example.meter", "1.2.0", "0.7", USES).to_zip()
}

fn entry() -> Entry {
    Entry::for_zip(
        &zip(),
        "meter/org.example.meter-1.2.0.zip",
        "Counts deaths too.",
        "2026-10-26",
    )
    .expect("an entry")
}

#[test]
fn an_entry_is_made_from_a_zip_and_says_what_its_manifest_says() {
    let e = entry();
    assert_eq!(e.id.as_str(), "org.example.meter");
    assert_eq!(e.version.to_string(), "1.2.0");
    assert_eq!(e.api.to_string(), "0.7");
    assert_eq!(e.permissions, ["chat.send"]);
    assert_eq!(e.domains, ["example.org"]);
    assert_eq!(e.size, Some(zip().len() as u64));
    assert_eq!(e.sha256, super::sha256_hex(&zip()));
    assert!(e.check(&zip()).is_ok());
}

#[test]
fn a_list_reads_back_as_it_was_written() {
    let mut c = Catalog {
        name: "Example's plugins".into(),
        plugins: Vec::new(),
    };
    c.put(entry());
    let other = package("org.example.alpha", "0.1.0", "0.1", "").to_zip();
    c.put(Entry::for_zip(&other, "https://cdn.example.org/alpha.zip", "", "").expect("entry"));
    let text = c.to_json();
    assert!(text.starts_with("{\n  \"format\": 1,\n"), "{text}");
    let read = Catalog::parse(text.as_bytes()).expect("reads");
    assert!(read.skipped.is_empty(), "{:?}", read.skipped);
    assert_eq!(read.catalog, c);
    // Kept in id order.
    let ids: Vec<&str> = c.plugins.iter().map(|p| p.id.as_str()).collect();
    assert_eq!(ids, ["org.example.alpha", "org.example.meter"]);
    // An empty list too.
    let empty = Catalog::default().to_json();
    assert_eq!(
        Catalog::parse(empty.as_bytes()).expect("reads").catalog,
        Catalog::default()
    );
}

#[test]
fn a_new_release_replaces_the_entry_of_the_one_before() {
    let mut c = Catalog::default();
    assert!(c.put(entry()).is_none());
    let next = package("org.example.meter", "1.3.0", "0.7", USES).to_zip();
    let replaced = c.put(Entry::for_zip(&next, "meter/1.3.0.zip", "", "").expect("entry"));
    assert_eq!(
        replaced.map(|e| e.version.to_string()).as_deref(),
        Some("1.2.0")
    );
    assert_eq!(c.plugins.len(), 1);
    assert_eq!(c.plugins[0].version.to_string(), "1.3.0");
}

#[test]
fn keys_it_does_not_know_are_ignored_and_a_bad_entry_is_set_aside() {
    let sha = "a".repeat(64);
    let text = format!(
        r#"{{"format": 1, "name": "Mixed", "motd": "hi", "plugins": [
            {{"id": "org.example.good", "version": "1.0.0", "api": "0.7", "zip": "good.zip",
              "sha256": "{sha}", "icon": "good.png"}},
            {{"id": "org.example.nohash", "version": "1.0.0", "api": "0.7", "zip": "x.zip"}},
            {{"id": "org.example.badversion", "version": "1.0", "api": "0.7", "zip": "x.zip", "sha256": "{sha}"}},
            {{"id": "dereth.client", "version": "1.0.0", "api": "0.7", "zip": "x.zip", "sha256": "{sha}"}},
            {{"id": "org.example.good", "version": "2.0.0", "api": "0.7", "zip": "y.zip", "sha256": "{sha}"}},
            "not an object"
        ]}}"#
    );
    let read = Catalog::parse(text.as_bytes()).expect("reads");
    assert_eq!(read.catalog.name, "Mixed");
    assert_eq!(read.catalog.plugins.len(), 1);
    let good = &read.catalog.plugins[0];
    assert_eq!(
        (good.id.as_str(), good.name.as_str()),
        ("org.example.good", "org.example.good")
    );
    assert_eq!(
        good.version.to_string(),
        "1.0.0",
        "the first of a repeated id is kept"
    );
    assert_eq!(read.skipped.len(), 5, "{:?}", read.skipped);
    assert!(
        read.skipped[0].starts_with("org.example.nohash: no sha256"),
        "{:?}",
        read.skipped
    );
    assert!(
        read.skipped[3].contains("more than once"),
        "{:?}",
        read.skipped
    );
    assert!(
        read.skipped[4].starts_with("plugin 6"),
        "{:?}",
        read.skipped
    );
}

#[test]
fn a_list_of_a_newer_format_or_no_format_is_refused_whole() {
    assert_eq!(
        Catalog::parse(br#"{"format": 2, "plugins": []}"#),
        Err(CatalogError::NewerFormat(2))
    );
    for bad in [
        &br#"{"plugins": []}"#[..],
        br#"{"format": 1}"#,
        br#"[1, 2]"#,
        b"not json",
        br#"{"format": 0, "plugins": []}"#,
    ] {
        assert!(
            matches!(Catalog::parse(bad), Err(CatalogError::NotAList(_))),
            "{}",
            String::from_utf8_lossy(bad)
        );
    }
    let big = vec![b' '; super::MAX_BYTES + 1];
    assert_eq!(Catalog::parse(&big), Err(CatalogError::TooLarge));
    assert_eq!(FORMAT, 1);
}

#[test]
fn a_zip_is_taken_only_when_it_is_the_listed_file_and_says_what_its_entry_says() {
    let e = entry();
    let z = zip();
    // Another size, another file.
    let mut longer = z.clone();
    longer.push(0);
    assert!(matches!(e.check(&longer), Err(Mismatch::Size { .. })));
    let mut flipped = z.clone();
    let last = flipped.len() - 1;
    flipped[last] ^= 1;
    assert!(matches!(e.check(&flipped), Err(Mismatch::Hash { .. })));
    // The right file, but the entry says otherwise than the manifest. Each field in turn, with the
    // entry's hash and size set to the zip's so that only the field differs.
    let differs = |pkg: Package| {
        let other = pkg.to_zip();
        let mut lie = e.clone();
        lie.sha256 = super::sha256_hex(&other);
        lie.size = Some(other.len() as u64);
        lie.check(&other).map(|_| ())
    };
    let field = |r: Result<(), Mismatch>| match r {
        Err(Mismatch::Says { field, .. }) => field,
        other => panic!("{other:?}"),
    };
    assert_eq!(
        field(differs(package("org.example.other", "1.2.0", "0.7", USES))),
        "id"
    );
    assert_eq!(
        field(differs(package("org.example.meter", "1.2.1", "0.7", USES))),
        "version"
    );
    assert_eq!(
        field(differs(package("org.example.meter", "1.2.0", "0.6", USES))),
        "API version"
    );
    let more = USES.replace("[net]", "\"auto.loot\" = \"Loots.\"\n[net]");
    assert_eq!(
        field(differs(package("org.example.meter", "1.2.0", "0.7", &more))),
        "permissions"
    );
    let elsewhere = USES.replace("example.org", "example.net");
    assert_eq!(
        field(differs(package(
            "org.example.meter",
            "1.2.0",
            "0.7",
            &elsewhere
        ))),
        "web domains"
    );
    let junk = b"not a zip".to_vec();
    let mut lie = e.clone();
    lie.sha256 = super::sha256_hex(&junk);
    lie.size = None;
    assert!(matches!(lie.check(&junk), Err(Mismatch::NotAPackage(_))));
    // The message says what differs, in words.
    let said = differs(package("org.example.meter", "1.2.1", "0.7", USES))
        .unwrap_err()
        .to_string();
    assert_eq!(
        said,
        "the package's version is 1.2.1, but its source lists 1.2.0"
    );
}

#[test]
fn a_zip_is_found_against_the_lists_own_address() {
    let base = "https://example.github.io/plugins/plugins.json";
    let cases = [
        (
            "meter.zip",
            Some("https://example.github.io/plugins/meter.zip"),
        ),
        (
            "meter/m-1.0.0.zip",
            Some("https://example.github.io/plugins/meter/m-1.0.0.zip"),
        ),
        (
            "./a/../b.zip",
            Some("https://example.github.io/plugins/b.zip"),
        ),
        (
            "../other/x.zip",
            Some("https://example.github.io/other/x.zip"),
        ),
        ("/root.zip", Some("https://example.github.io/root.zip")),
        (
            "//cdn.example.org/x.zip",
            Some("https://cdn.example.org/x.zip"),
        ),
        (
            "https://github.com/o/r/releases/download/v1.0.0/x.zip",
            Some("https://github.com/o/r/releases/download/v1.0.0/x.zip"),
        ),
        (
            "x.zip?v=2",
            Some("https://example.github.io/plugins/x.zip?v=2"),
        ),
        ("http://example.org/x.zip", None),
        ("file:///c:/x.zip", None),
        ("javascript:alert(1)", None),
        ("", None),
    ];
    for (reference, want) in cases {
        assert_eq!(resolve(base, reference).as_deref(), want, "{reference}");
    }
    assert_eq!(
        resolve("https://example.org", "x.zip").as_deref(),
        Some("https://example.org/x.zip")
    );
    assert_eq!(
        resolve("https://example.org/list.json?token=1", "x.zip").as_deref(),
        Some("https://example.org/x.zip")
    );
    assert_eq!(resolve("http://example.org/list.json", "x.zip"), None);
}

#[test]
fn a_script_with_no_manifest_cannot_be_listed() {
    let p = Package {
        origin: String::new(),
        files: [("index.lua".to_owned(), b"print(1)".to_vec())].into(),
    };
    assert!(Entry::for_zip(&p.to_zip(), "x.zip", "", "").is_err());
}
