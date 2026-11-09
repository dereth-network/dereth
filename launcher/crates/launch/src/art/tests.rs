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

// ----- GitHub repositories ---------------------------------------------------------------------

/// GitHub's releases API answer for a repository publishing one pack, as recorded, with the users
/// it names left out; and the checksum file of its newest release.
const RELEASES: &[u8] = include_bytes!("testdata/github-releases.json");
const SUMS: &str = include_str!("testdata/enhanced-environment.dat.sha256");
const NEWEST_SHA: &str = "67a13b64610f2562769c33060f9f216fe33c58d39beaf44c49baf2e8014fc9f8";
const REPO: &str = "https://github.com/jacobjohnston/dereth-enhanced-environment";

#[test]
fn a_github_repositorys_address_names_its_owner_and_repository() {
    use super::github::repository;
    let want = Some((
        "jacobjohnston".to_owned(),
        "dereth-enhanced-environment".to_owned(),
    ));
    assert_eq!(repository(REPO), want);
    assert_eq!(repository(&format!("{REPO}/")), want);
    assert_eq!(repository(&format!("{REPO}.git")), want);
    assert_eq!(repository(&format!("{REPO}/releases")), None);
    assert_eq!(repository("https://github.com/jacobjohnston"), None);
    assert_eq!(repository("https://example.org/a/b"), None);
    assert_eq!(
        super::github::fallback_id("Jacob_Johnston", "dereth.enhanced"),
        "com.github.jacob-johnston.dereth-enhanced"
    );
}

#[test]
fn a_github_repositorys_newest_release_is_its_packs_latest_version() {
    use super::github::{catalog, newest};
    let n = newest(RELEASES).expect("reads");
    assert_eq!(n.version, "0.1.1");
    assert_eq!(n.title, "Enhanced Environment", "the version off its title");
    assert_eq!(n.file, "enhanced-environment.dat");
    assert_eq!(n.size, 252_625_920);
    assert_eq!(n.released, "2026-11-04");
    assert!(n
        .url
        .ends_with("/releases/download/v0.1.1/enhanced-environment.dat"));
    assert!(n
        .sums_url
        .as_deref()
        .unwrap()
        .ends_with("/v0.1.1/enhanced-environment.dat.sha256"));
    assert_eq!(n.digest.as_deref(), Some(NEWEST_SHA));
    assert!(n.notes.starts_with("## 0.1.1"));

    let c = catalog(
        "jacobjohnston",
        "dereth-enhanced-environment",
        &n,
        Some(SUMS),
    )
    .expect("lists");
    assert_eq!(c.name, "jacobjohnston/dereth-enhanced-environment");
    let e = &c.packs[0];
    assert_eq!(e.id, "com.github.jacobjohnston.dereth-enhanced-environment");
    assert_eq!(
        (e.version.as_str(), e.sha256.as_str(), e.size),
        ("0.1.1", NEWEST_SHA, 252_625_920)
    );
    assert_eq!(e.description, n.notes, "the release's body describes it");
    // It is kept as any source's list is: as an art.json, read back whole.
    let back = dereth_art_catalog::Catalog::parse(c.to_json().as_bytes()).expect("reads back");
    assert!(back.skipped.is_empty(), "{:?}", back.skipped);
    assert_eq!(back.catalog, c);

    let other = SUMS.replace('6', "7");
    assert!(catalog("o", "r", &n, Some(&other))
        .unwrap_err()
        .contains("GitHub says"));
    assert!(catalog("o", "r", &n, None)
        .unwrap_err()
        .contains("has no enhanced-environment.dat.sha256"));
    assert!(catalog("o", "r", &n, Some("nothing here")).is_err());
}

#[test]
fn drafts_pre_releases_and_releases_with_no_single_pack_are_passed_over_or_refused() {
    use super::github::newest;
    let mut list: serde_json::Value = serde_json::from_slice(RELEASES).unwrap();
    list[0]["prerelease"] = true.into();
    let n = newest(&serde_json::to_vec(&list).unwrap()).unwrap();
    assert_eq!(n.version, "0.1.0", "the pre-release is passed over");
    list[1]["draft"] = true.into();
    assert!(
        newest(&serde_json::to_vec(&list).unwrap()).is_err(),
        "nothing published"
    );
    let mut list: serde_json::Value = serde_json::from_slice(RELEASES).unwrap();
    let pack = list[0]["assets"][1].clone();
    list[0]["assets"].as_array_mut().unwrap().push(pack);
    assert!(newest(&serde_json::to_vec(&list).unwrap())
        .unwrap_err()
        .contains("more than one"));
    list[0]["assets"] = serde_json::json!([]);
    assert!(newest(&serde_json::to_vec(&list).unwrap())
        .unwrap_err()
        .contains("no .dat"));
    assert!(newest(br#"{"message": "API rate limit exceeded"}"#).is_err());
}

/// The list a GitHub repository's releases come to, for a pack of `bytes` at version `version`.
fn github_list(bytes: &[u8], version: &str) -> Vec<u8> {
    let n = super::github::Newest {
        version: version.into(),
        title: "Enhanced Environment".into(),
        notes: "Brighter grass.".into(),
        released: "2026-11-04".into(),
        file: "enhanced-environment.dat".into(),
        size: bytes.len() as u64,
        url: format!("{REPO}/releases/download/v{version}/enhanced-environment.dat"),
        digest: None,
        sums_url: None,
    };
    let sums = format!("{}  enhanced-environment.dat\n", sha(bytes));
    super::github::catalog(
        "jacobjohnston",
        "dereth-enhanced-environment",
        &n,
        Some(&sums),
    )
    .unwrap()
    .to_json()
    .into_bytes()
}

#[test]
fn a_github_repositorys_pack_installs_under_its_manifests_id_and_updates_from_its_releases() {
    let dir = temp("github");
    let mut shelf = Shelf::open(&dir);
    let url = shelf.add_source(&format!("{REPO}.git")).unwrap();
    assert_eq!(url, REPO);
    assert!(matches!(
        shelf.add_source(&format!("{REPO}/")),
        Err(ArtError::AlreadyAdded(_))
    ));
    let (api, etag) = shelf.github_ask(REPO).unwrap();
    assert_eq!(api, "https://api.github.com/repos/jacobjohnston/dereth-enhanced-environment/releases?per_page=30");
    assert_eq!(etag, None, "nothing kept yet, so nothing to match");

    let v1 = pack("io.github.jacob.environment", "0.1.0");
    shelf
        .record_github(
            REPO,
            Ok(Some((github_list(&v1, "0.1.0"), Some("W/\"a\"".into())))),
            1,
        )
        .unwrap();
    assert_eq!(
        shelf.github_ask(REPO).unwrap().1.as_deref(),
        Some("W/\"a\"")
    );
    let view = shelf.view();
    let source = view.sources.iter().find(|s| s.url == REPO).unwrap();
    assert!(source.github && source.packs == 1);
    assert_eq!(
        view.available[0].id,
        "com.github.jacobjohnston.dereth-enhanced-environment"
    );

    let d = shelf.plan_install(REPO, &view.available[0].id).unwrap();
    std::fs::create_dir_all(d.partial.parent().unwrap()).unwrap();
    std::fs::write(&d.partial, &v1).unwrap();
    let done = shelf.finish(&d, v1.len() as u64, &sha(&v1)).unwrap();
    assert_eq!(
        done.said(),
        "Installed io.github.jacob.environment 0.1.0.",
        "the manifest's own name"
    );
    assert_eq!(
        shelf.packs[0].id, "io.github.jacob.environment",
        "the manifest's own id"
    );
    assert!(
        shelf.view().available.is_empty(),
        "installed, so no longer offered"
    );

    // Not modified: the list kept stays, and nothing is new.
    shelf.record_github(REPO, Ok(None), 2).unwrap();
    assert!(shelf.pending().is_empty());

    // A new release; one whose pack names another version is refused.
    let v2 = pack("io.github.jacob.environment", "0.2.0");
    shelf
        .record_github(
            REPO,
            Ok(Some((github_list(&v2, "0.2.0"), Some("W/\"b\"".into())))),
            3,
        )
        .unwrap();
    assert_eq!(shelf.pending()[0].to, "0.2.0");
    let d = shelf.plan_update("io.github.jacob.environment").unwrap();
    let wrong = pack("io.github.jacob.environment", "0.3.0");
    shelf
        .record_github(REPO, Ok(Some((github_list(&wrong, "0.2.0"), None))), 4)
        .unwrap();
    let d_wrong = shelf.plan_update("io.github.jacob.environment").unwrap();
    std::fs::write(&d_wrong.partial, &wrong).unwrap();
    assert!(matches!(
        shelf.finish(&d_wrong, wrong.len() as u64, &sha(&wrong)),
        Err(ArtError::Mismatch(Mismatch::Says {
            field: "version",
            ..
        }))
    ));
    shelf
        .record_github(REPO, Ok(Some((github_list(&v2, "0.2.0"), None))), 5)
        .unwrap();
    std::fs::write(&d.partial, &v2).unwrap();
    let done = shelf.finish(&d, v2.len() as u64, &sha(&v2)).unwrap();
    assert_eq!(done.said(), "Updated io.github.jacob.environment to 0.2.0.");
    assert_eq!(shelf.packs.len(), 1);

    // A failed ask keeps the list and forgets the tag, so the next ask is whole.
    shelf
        .record_github(REPO, Err("rate limited".into()), 6)
        .unwrap();
    assert_eq!(shelf.github_ask(REPO).unwrap().1, None);
    assert_eq!(
        Shelf::open(&dir).source(REPO).unwrap().catalog.packs.len(),
        1,
        "kept"
    );
    let _ = std::fs::remove_dir_all(dir);
}

#[test]
fn a_github_repository_cannot_install_one_of_dereths_own_packs() {
    let dir = temp("github-first-party");
    let mut shelf = Shelf::open(&dir);
    shelf.add_source(REPO).unwrap();
    let bytes = pack("network.dereth.remastered", "0.1.0");
    shelf
        .record_github(REPO, Ok(Some((github_list(&bytes, "0.1.0"), None))), 1)
        .unwrap();
    let id = shelf.view().available[0].id.clone();
    let d = shelf.plan_install(REPO, &id).unwrap();
    std::fs::create_dir_all(d.partial.parent().unwrap()).unwrap();
    std::fs::write(&d.partial, &bytes).unwrap();
    assert!(matches!(
        shelf.finish(&d, bytes.len() as u64, &sha(&bytes)),
        Err(ArtError::FirstPartyOnly(_))
    ));
    assert!(shelf.packs.is_empty());
    let _ = std::fs::remove_dir_all(dir);
}
