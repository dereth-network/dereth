//! A world database release: its archive is the same bytes for the same files, however they were
//! read, and its notes are the change log's sections for the release.
//! Fixture: the server test kit's world, and change logs written in the tests.

use std::path::Path;

use yalain_world_db::release;
use yalain_world_db::rows::read;
use yalain_world_db::Files;

/// The test world's files, read from the server's test kit, in the order the folder lists them
/// or the reverse.
fn fixture(reverse: bool) -> Files {
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../testkit/fixtures/world");
    let mut found = Vec::new();
    let mut stack = vec![root.clone()];
    while let Some(d) = stack.pop() {
        for e in std::fs::read_dir(&d).unwrap().flatten() {
            let p = e.path();
            if p.is_dir() {
                stack.push(p);
            } else {
                let rel = p
                    .strip_prefix(&root)
                    .unwrap()
                    .to_string_lossy()
                    .replace('\\', "/");
                found.push((rel, std::fs::read_to_string(&p).unwrap()));
            }
        }
    }
    if reverse {
        found.reverse();
    }
    found.into_iter().collect()
}

fn build(files: &Files, tag: &str) -> Vec<u8> {
    let db = read(files).unwrap();
    release::archive(
        &release::release_files(files, &db),
        &release::manifest(tag, &db),
    )
}

/// The names and dates of an archive's entries, read off its local headers.
fn entries(zip: &[u8]) -> Vec<(String, u16, u16, u16)> {
    let u16_at = |p: usize| u16::from_le_bytes([zip[p], zip[p + 1]]);
    let u32_at = |p: usize| u32::from_le_bytes([zip[p], zip[p + 1], zip[p + 2], zip[p + 3]]);
    let mut out = Vec::new();
    let mut p = 0;
    while u32_at(p) == 0x0403_4B50 {
        let (method, time, date) = (u16_at(p + 8), u16_at(p + 10), u16_at(p + 12));
        let size = usize::try_from(u32_at(p + 18)).unwrap();
        let (n, extra) = (usize::from(u16_at(p + 26)), usize::from(u16_at(p + 28)));
        let name = String::from_utf8(zip[p + 30..p + 30 + n].to_vec()).unwrap();
        out.push((name, method, time, date));
        p += 30 + n + extra + size;
    }
    out
}

/// A folder under a release's name is taken for that release only when its files are the archive's:
/// another's, or one with a file changed or missing, is refused in words naming both hashes.
/// Behaviour: server.world-db.release-folder-holds-to-its-archive-hash
#[test]
fn a_folder_of_other_files_does_not_pass_for_a_release() {
    let zip = build(&fixture(false), "v0.3.0");
    let sha = release::sha256_hex(&zip);
    let files = release::read_stored(&zip).expect("its files");
    let dir = std::env::temp_dir().join(format!("yalain-release-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    assert!(release::holds_archive(&dir, &sha)
        .expect_err("nothing there")
        .contains("holds no world"));
    for (name, text) in &files {
        let p = dir.join(name);
        std::fs::create_dir_all(p.parent().expect("a folder")).expect("made");
        std::fs::write(&p, text).expect("written");
    }
    assert_eq!(release::holds_archive(&dir, &sha), Ok(()));
    std::fs::write(dir.join("world.toml"), "changed").expect("written");
    let why = release::holds_archive(&dir, &sha).expect_err("another's");
    assert!(
        why.contains("is not that release") && why.contains(&sha),
        "{why}"
    );
    std::fs::write(dir.join("world.toml"), &files["world.toml"]).expect("written");
    assert_eq!(release::holds_archive(&dir, &sha), Ok(()));
    let pack = files.keys().find(|k| k.contains('/')).expect("a pack file");
    std::fs::remove_file(dir.join(pack)).expect("removed");
    assert!(release::holds_archive(&dir, &sha).is_err());
    std::fs::remove_file(dir.join("manifest.json")).expect("removed");
    assert!(release::holds_archive(&dir, &sha)
        .expect_err("no manifest")
        .contains("not a release's folder"));
    let _ = std::fs::remove_dir_all(&dir);
}

/// A release's folder, as a fetch unpacks it, hashes to its archive's SHA-256, one file changed or
/// missing does not; and the archive's bytes for given files stay as they are, as every pinned
/// hash depends on it.
/// Behaviour: server.world-db.release-folder-holds-to-its-archive-hash
#[test]
fn a_release_folder_hashes_to_its_archive_and_the_archive_format_holds() {
    let zip = build(&fixture(false), "v0.3.0");
    let sha = release::sha256_hex(&zip);
    let files = release::read_stored(&zip).expect("its files");
    assert_eq!(release::unpacked_sha256(&files).as_deref(), Some(&*sha));
    let mut changed = files.clone();
    changed.insert("world.toml".into(), "changed".into());
    assert_ne!(release::unpacked_sha256(&changed).as_deref(), Some(&*sha));
    let mut missing = files.clone();
    let pack = missing
        .keys()
        .find(|k| k.contains('/'))
        .cloned()
        .expect("a pack file");
    missing.remove(&pack);
    assert_ne!(release::unpacked_sha256(&missing).as_deref(), Some(&*sha));
    let mut no_manifest = files;
    no_manifest.remove("manifest.json");
    assert_eq!(release::unpacked_sha256(&no_manifest), None);

    let mut little = Files::new();
    little.insert("world.toml".into(), "key = \"w\"\n".into());
    little.insert("p/pack.toml".into(), "name = \"p\"\n".into());
    assert_eq!(
        release::sha256_hex(&release::archive(&little, "{}\n")),
        "cd3587cf7fbff3f203fc746fa75ab6e5d36b627ba920b653cef734272288de22"
    );
}

/// Behaviour: server.world-db.release-the-same-bytes-for-the-same-files
#[test]
fn a_release_built_twice_from_the_same_files_is_the_same_bytes() {
    let a = build(&fixture(false), "v0.3.0");
    let b = build(&fixture(true), "v0.3.0");
    assert_eq!(a, b);
    assert_eq!(release::sha256_hex(&a), release::sha256_hex(&b));
    // Every entry stored, dated 1980-01-01 00:00, in path order, the manifest among them.
    let e = entries(&a);
    assert!(e.len() > 3 && e.iter().any(|x| x.0 == "manifest.json"));
    for (name, method, time, date) in &e {
        assert_eq!((*method, *time, *date), (0, 0, (1 << 5) | 1), "{name}");
    }
    assert!(e.windows(2).all(|w| w[0].0 < w[1].0));
    // Another tag is another archive.
    assert_ne!(build(&fixture(false), "v0.3.1"), a);
}

const CHANGES: &str = "# Changes

Notes for each release.

## Unreleased

- Not yet.

## v0.4.0

- Later than the release.

## v0.3.0

- Doors open.
- Chests hold
  what they held.

## v0.2.0

- Camps fill.

## v0.1.0

- The first release.
";

/// Behaviour: server.world-db.release-notes-from-the-change-log
#[test]
fn the_notes_are_the_change_log_s_sections_since_the_last_release_newest_first() {
    let sha = "ab".repeat(32);
    let n = release::notes(CHANGES, "v0.3.0", Some("v0.1.0"), &sha).unwrap();
    assert!(n.starts_with(
        "Changes since v0.1.0:\n\n- Doors open.\n- Chests hold\n  what they held.\n- Camps fill.\n\n## Download"
    ), "{n}");
    assert!(n.contains(&format!("SHA-256: `{sha}`")));
    assert!(n.contains("yalain-world-v0.3.0.zip") && n.contains("--version v0.3.0"));
    for absent in [
        "Not yet",
        "Later than",
        "The first release",
        "# Changes",
        "Notes for",
    ] {
        assert!(!n.contains(absent), "{absent}: {n}");
    }
    // Without a last release, the tag's own section.
    let own = release::notes(CHANGES, "v0.3.0", None, &sha).unwrap();
    assert!(
        own.starts_with("- Doors open.") && !own.contains("Camps fill"),
        "{own}"
    );
    // A tag the change log has no section for, or a "last release" not before the tag, is refused.
    let e = release::notes(CHANGES, "v0.5.0", Some("v0.1.0"), &sha).unwrap_err();
    assert!(e.contains("no section for v0.5.0"), "{e}");
    assert!(release::notes(CHANGES, "v0.3.0", Some("v0.3.0"), &sha).is_err());
    assert!(release::notes(CHANGES, "v0.3.0", Some("latest"), &sha).is_err());
    assert_eq!(release::tag_numbers("v0.10.2"), Some([0, 10, 2]));
    assert!(release::tag_numbers("v0.10.0") > release::tag_numbers("v0.9.0"));
}
