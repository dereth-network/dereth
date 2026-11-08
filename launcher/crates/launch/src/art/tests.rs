//! Behaviour: none (the launcher's art packs: their sources, installs and order)

use std::path::PathBuf;

use dereth_art_catalog::Mismatch;

use super::{resume_at, room_for, stream, ArtError, Shelf, FIRST_PARTY, PACK_FILE};

const OURS: &str = "https://packs.example.org/art.json";

fn temp(name: &str) -> PathBuf {
    let dir = std::env::temp_dir().join(format!("dereth-art-{name}-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    dir
}

fn sha(bytes: &[u8]) -> String {
    dereth_plugin_package::catalog::sha256_hex(bytes)
}

fn pack(id: &str, version: &str) -> Vec<u8> {
    crate::testing::art_pack(id, version)
}

/// A source's list naming each pack of `packs` (id, version) at `<id>.dat` beside it.
fn list(packs: &[(&str, &str)]) -> Vec<u8> {
    let bytes: Vec<Vec<u8>> = packs.iter().map(|(id, v)| pack(id, v)).collect();
    let refs: Vec<&[u8]> = bytes.iter().map(Vec::as_slice).collect();
    crate::testing::art_list(&refs)
}

/// `shelf` with the source at `OURS` reading `packs`.
fn listing(shelf: &mut Shelf, packs: &[(&str, &str)]) {
    if shelf.source(OURS).is_none() {
        shelf.add_source(OURS).unwrap();
    }
    shelf.record_read(OURS, Ok(list(packs)), 1).unwrap();
}

/// Install `id` from `OURS`, its file being `bytes`.
fn install(shelf: &mut Shelf, id: &str, bytes: &[u8]) -> Result<super::Done, ArtError> {
    let d = shelf.plan_install(OURS, id)?;
    std::fs::create_dir_all(d.partial.parent().unwrap()).unwrap();
    std::fs::write(&d.partial, bytes).unwrap();
    shelf.finish(&d, bytes.len() as u64, &sha(bytes))
}

#[test]
fn dereths_own_source_is_there_to_begin_with_and_stays() {
    let dir = temp("sources");
    let mut shelf = Shelf::open(&dir);
    assert_eq!(shelf.sources.len(), 1);
    assert!(shelf.sources[0].first_party());
    assert_eq!(
        shelf.remove_source(FIRST_PARTY),
        Err(ArtError::FirstPartyStays)
    );
    assert!(matches!(
        shelf.add_source("http://packs.example.org/art.json"),
        Err(ArtError::NotHttps(_))
    ));
    assert_eq!(
        shelf
            .add_source(" HTTPS://packs.example.org/art.json ")
            .unwrap(),
        OURS
    );
    assert!(matches!(
        shelf.add_source(OURS),
        Err(ArtError::AlreadyAdded(_))
    ));
    shelf
        .record_read(OURS, Ok(list(&[("org.example.trees", "1.0.0")])), 5)
        .unwrap();
    shelf.record_read(OURS, Err("offline".into()), 6).unwrap();
    let again = Shelf::open(&dir);
    let s = again.source(OURS).unwrap();
    assert_eq!(s.catalog.packs.len(), 1, "the last list read stays in use");
    assert_eq!(s.error.as_deref(), Some("offline"));
    assert_eq!(
        s.label(),
        "packs.example.org",
        "a list with no name goes by its host"
    );
    let _ = std::fs::remove_dir_all(dir);
}

#[test]
fn a_download_is_installed_only_when_its_size_sha256_and_pack_are_the_ones_listed() {
    let dir = temp("checks");
    let mut shelf = Shelf::open(&dir);
    listing(&mut shelf, &[("org.example.trees", "1.0.0")]);
    let good = pack("org.example.trees", "1.0.0");

    let d = shelf.plan_install(OURS, "org.example.trees").unwrap();
    assert_eq!(d.url, "https://packs.example.org/org.example.trees.dat");
    std::fs::create_dir_all(d.partial.parent().unwrap()).unwrap();
    std::fs::write(&d.partial, &good).unwrap();
    let wrong = sha(b"something else");
    let refused = shelf.finish(&d, good.len() as u64, &wrong);
    assert!(
        matches!(refused, Err(ArtError::Mismatch(Mismatch::Hash { .. }))),
        "{refused:?}"
    );
    assert!(!d.partial.exists(), "a refused download is deleted");
    assert!(shelf.packs.is_empty() && shelf.stack().is_empty());

    // The listed bytes, but not the pack: another id inside, the same size and hash claimed.
    let other = pack("org.example.rocks", "1.0.0");
    std::fs::write(&d.partial, &other).unwrap();
    let mut forged = shelf.clone();
    let source = forged.sources.iter_mut().find(|s| s.url == OURS).unwrap();
    source.catalog.packs[0].sha256 = sha(&other);
    source.catalog.packs[0].size = other.len() as u64;
    assert!(matches!(
        forged.finish(&d, other.len() as u64, &sha(&other)),
        Err(ArtError::Mismatch(Mismatch::Says { field: "id", .. }))
    ));

    let done = install(&mut shelf, "org.example.trees", &good).unwrap();
    assert_eq!(done.said(), "Installed org.example.trees 1.0.0.");
    assert_eq!(
        shelf.stack(),
        [dir.join("org.example.trees").join(PACK_FILE)]
    );
    let _ = std::fs::remove_dir_all(dir);
}

#[test]
fn the_stack_is_the_installed_packs_that_are_on_in_order_a_new_one_last() {
    let dir = temp("stack");
    let mut shelf = Shelf::open(&dir);
    let ids = [
        "org.example.trees",
        "org.example.rocks",
        "org.example.houses",
    ];
    listing(&mut shelf, &ids.map(|id| (id, "1.0.0")));
    for id in ids {
        install(&mut shelf, id, &pack(id, "1.0.0")).unwrap();
    }
    let order = |shelf: &Shelf| -> Vec<String> {
        shelf
            .stack()
            .iter()
            .map(|p| {
                p.parent()
                    .unwrap()
                    .file_name()
                    .unwrap()
                    .to_string_lossy()
                    .into_owned()
            })
            .collect()
    };
    assert_eq!(order(&shelf), ids, "each new one last, so it wins");
    shelf.move_pack("org.example.houses", false).unwrap();
    assert_eq!(
        order(&shelf),
        [
            "org.example.trees",
            "org.example.houses",
            "org.example.rocks"
        ]
    );
    shelf.move_pack("org.example.rocks", true).unwrap();
    assert_eq!(order(&shelf)[2], "org.example.rocks", "the last stays last");
    shelf.set_on("org.example.trees", false).unwrap();
    assert_eq!(order(&shelf), ["org.example.houses", "org.example.rocks"]);
    let view = shelf.view();
    let shown: Vec<&str> = view.installed.iter().map(|p| p.id.as_str()).collect();
    assert_eq!(
        shown,
        [
            "org.example.rocks",
            "org.example.houses",
            "org.example.trees"
        ],
        "the winner first"
    );
    assert!(!view.installed[2].on);

    let again = Shelf::open(&dir);
    assert_eq!(
        order(&again),
        ["org.example.houses", "org.example.rocks"],
        "kept"
    );
    shelf.remove("org.example.houses").unwrap();
    assert!(!dir.join("org.example.houses").exists());
    std::fs::remove_file(dir.join("org.example.rocks").join(PACK_FILE)).unwrap();
    assert_eq!(
        order(&Shelf::open(&dir)),
        Vec::<String>::new(),
        "a pack whose file is gone is dropped"
    );
    let _ = std::fs::remove_dir_all(dir);
}

#[test]
fn an_installed_pack_updates_only_from_its_own_source_to_a_higher_version_in_its_place() {
    let dir = temp("updates");
    let mut shelf = Shelf::open(&dir);
    listing(
        &mut shelf,
        &[
            ("org.example.trees", "1.0.0"),
            ("org.example.rocks", "1.0.0"),
        ],
    );
    install(
        &mut shelf,
        "org.example.trees",
        &pack("org.example.trees", "1.0.0"),
    )
    .unwrap();
    install(
        &mut shelf,
        "org.example.rocks",
        &pack("org.example.rocks", "1.0.0"),
    )
    .unwrap();
    shelf.set_on("org.example.trees", false).unwrap();
    assert!(shelf.pending().is_empty());
    assert!(matches!(
        shelf.plan_update("org.example.trees"),
        Err(ArtError::NothingNewer(_))
    ));

    listing(
        &mut shelf,
        &[
            ("org.example.trees", "1.10.0"),
            ("org.example.rocks", "0.9.0"),
        ],
    );
    let pending = shelf.pending();
    assert_eq!(pending.len(), 1, "a lower version is no update");
    assert_eq!(
        (pending[0].from.as_str(), pending[0].to.as_str()),
        ("1.0.0", "1.10.0")
    );
    let d = shelf.plan_update("org.example.trees").unwrap();
    let bytes = pack("org.example.trees", "1.10.0");
    std::fs::write(&d.partial, &bytes).unwrap();
    let done = shelf.finish(&d, bytes.len() as u64, &sha(&bytes)).unwrap();
    assert_eq!(done.said(), "Updated org.example.trees to 1.10.0.");
    assert_eq!(shelf.packs[0].id, "org.example.trees", "in its place");
    assert!(!shelf.packs[0].on, "and still off");
    assert_eq!(shelf.packs[0].release.version, "1.10.0");

    shelf.remove_source(OURS).unwrap();
    assert!(shelf.view().installed.iter().all(|p| p.source_removed));
    assert!(matches!(
        shelf.plan_update("org.example.rocks"),
        Err(ArtError::NoSource(_))
    ));
    let _ = std::fs::remove_dir_all(dir);
}

#[test]
fn dereths_own_packs_install_only_from_dereths_own_source() {
    let dir = temp("first-party");
    let mut shelf = Shelf::open(&dir);
    listing(&mut shelf, &[("network.dereth.remastered", "1.0.0")]);
    assert!(matches!(
        shelf.plan_install(OURS, "network.dereth.remastered"),
        Err(ArtError::FirstPartyOnly(_))
    ));
    assert!(!shelf.view().available[0].installable);
    let _ = std::fs::remove_dir_all(dir);
}

#[test]
fn a_download_that_stopped_goes_on_from_where_it_stopped_and_hashes_the_whole() {
    let dir = temp("stream");
    let partial = dir.join("x").join("download.partial");
    let whole: Vec<u8> = (0..3_000_000u32).map(|i| (i % 251) as u8).collect();
    let size = whole.len() as u64;
    assert_eq!(resume_at(&partial, size), 0);
    let mut seen = Vec::new();
    let (n, hash) = stream(&partial, 0, size, &mut &whole[..1_000_000], &mut |d, _| {
        seen.push(d)
    })
    .unwrap();
    assert_eq!(n, 1_000_000);
    assert_ne!(hash, sha(&whole));
    assert_eq!(*seen.last().unwrap(), 1_000_000);
    let at = resume_at(&partial, size);
    assert_eq!(at, 1_000_000);
    let (n, hash) = stream(&partial, at, size, &mut &whole[1_000_000..], &mut |_, _| {}).unwrap();
    assert_eq!((n, hash), (size, sha(&whole)));
    assert_eq!(std::fs::read(&partial).unwrap(), whole);
    assert_eq!(
        resume_at(&partial, size),
        0,
        "a whole file is no place to go on from"
    );
    assert!(!partial.exists());

    let over = stream(&partial, 0, 10, &mut &whole[..11], &mut |_, _| {});
    assert!(over.is_err());
    assert!(
        !partial.exists(),
        "a download larger than listed is deleted"
    );
    assert!(room_for(None, size, 0));
    assert!(room_for(Some(size + (64 << 20)), size, 0));
    assert!(!room_for(Some(size), size, 0));
    assert!(
        room_for(Some(64 << 20), size, size),
        "only what is left needs room"
    );
    let _ = std::fs::remove_dir_all(dir);
}
