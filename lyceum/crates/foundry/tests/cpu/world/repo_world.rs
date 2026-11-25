//! Behaviour: none (tooling: a world repository opened as the world builder's world, over its pinned land)

use std::path::PathBuf;

use dereth_dat::ModernDat;
use lyceum_foundry::compile::Field;
use lyceum_foundry::edit::{Op, Session, VertexSet};
use lyceum_foundry::repo::Repo;
use lyceum_foundry::repo_world::{check_inputs, check_land, world_of};
use sha2::{Digest, Sha256};

use crate::common::Fixture;

fn sha(bytes: &[u8]) -> String {
    Sha256::digest(bytes)
        .iter()
        .map(|b| format!("{b:02x}"))
        .collect()
}

/// A repository of a base of two packs and a patch with one, its world file pinning `land`
/// (`[land]` and its file hashes) when given.
fn repo(f: &Fixture, land: Option<&str>) -> PathBuf {
    f.write(
        "repo/base/library/pack.toml",
        "name = \"library\"\nversion = \"1\"\nprov = { from = \"source\", source = \"ACE-World-Database\", revision = \"v0.9.296\" }\n",
    );
    f.write(
        "repo/base/library/quest/A.toml",
        "[[quest]]\nname = \"A\"\n",
    );
    f.write(
        "repo/base/emergence/pack.toml",
        "name = \"emergence\"\nversion = \"1\"\n",
    );
    f.write(
        "repo/patches/2027-01-fixes/patch.toml",
        "id = \"2027-01-fixes\"\ntitle = \"Fixes\"\ndate = \"2027-01-01\"\nafter = \"\"\npacks = [\"fixes\"]\n",
    );
    f.write(
        "repo/patches/2027-01-fixes/packs/fixes/pack.toml",
        "name = \"fixes\"\nversion = \"1\"\n",
    );
    f.write(
        "repo/world.toml",
        &format!(
            "# A test world.\nformat_version = 1\ngame = \"ac1\"\nkey = \"test-repo\"\nera = \"newdereth\"\npacks = [\"base/library\", \"base/emergence\"]\nlineage = [\"2027-01-fixes\"]\n{}",
            land.unwrap_or_default()
        ),
    )
}

/// An overlay folder of four files, and the `[land]` that pins it.
fn land(f: &Fixture) -> (PathBuf, String) {
    let dir = f.path().join("land");
    let mut pins = String::from(
        "[land]\nconverter = \"dereth-art newdereth-world\"\nconverter_commit = \"abc123\"\nconverter_args = [\"--vaults\", \"land/vaults.txt\"]\nledger = \"land/generations.tsv\"\n",
    );
    for (file, key) in lyceum_foundry::repo_world::LAND_FILES {
        let bytes = format!("{file} of the land");
        f.write(&format!("land/{file}"), &bytes);
        pins.push_str(&format!("{key} = \"{}\"\n", sha(bytes.as_bytes())));
    }
    (dir, pins)
}

/// The land a repository pins is taken only as pinned: file for file by SHA-256. A file built
/// otherwise is refused with what was expected and found and the converter's command from the
/// pins; a pinned land not given is asked for.
#[test]
fn a_repositorys_land_is_taken_only_as_its_pins_name_it() {
    let f = Fixture::new("repo-land");
    let (dir, pins) = land(&f);
    let repo = Repo::open(&repo(&f, Some(&pins))).unwrap();
    check_land(&repo, Some(&dir)).unwrap();
    assert!(check_land(&repo, None).unwrap_err().contains("--on"));
    std::fs::write(dir.join("places.tsv"), "another place").unwrap();
    let why = check_land(&repo, Some(&dir)).unwrap_err();
    assert!(why.contains("places.tsv: expected"), "{why}");
    assert!(why.contains(&sha(b"another place")), "{why}");
    assert!(
        why.contains("dereth commit abc123")
            && why.contains(&format!(
                "dereth-art newdereth-world --world {} --dats <AC1 install> --ac2-dats <AC2 install> --out <folder>",
                repo.path.display()
            )),
        "{why}"
    );
    assert!(!why.contains("overlay_cell.dat"), "{why}");
}

/// A land lies over the AC1 files it was built over, so a player's own files are taken only as
/// `[land]` pins them, each by SHA-256 (one of a list, where it names several copies): one that
/// differs, or isn't there, is refused by name with what was expected and found, and the others
/// aren't named. Files it doesn't pin aren't asked for.
#[test]
fn a_land_is_refused_over_ac1_files_other_than_the_ones_it_was_built_over() {
    let f = Fixture::new("repo-land-inputs");
    let (_, mut pins) = land(&f);
    for (role, key) in lyceum_foundry::repo_world::INPUT_FILES {
        let bytes = format!("{} of the install", role.file_name());
        f.write(&format!("install/{}", role.file_name()), &bytes);
        pins.push_str(&format!("{key} = \"{}\"\n", sha(bytes.as_bytes())));
    }
    let pinned = Repo::open(&repo(&f, Some(&pins))).unwrap();
    let install = f.path().join("install");
    check_inputs(&pinned, &install).unwrap();

    std::fs::write(ModernDat::HighRes.in_dir(&install), "a later picture file").unwrap();
    std::fs::remove_file(ModernDat::Local.in_dir(&install)).unwrap();
    let why = check_inputs(&pinned, &install).unwrap_err();
    assert!(why.contains("client_highres.dat: expected"), "{why}");
    assert!(why.contains(&sha(b"a later picture file")), "{why}");
    assert!(why.contains("client_local_English.dat: expected"), "{why}");
    assert!(why.contains("found no file"), "{why}");
    assert!(why.contains("00.00.11.6096"), "{why}");
    assert!(!why.contains("client_portal.dat"), "{why}");

    // A file pinned as one of several copies is taken as any of them.
    let highres = ModernDat::HighRes.in_dir(&install);
    std::fs::write(&highres, "client_highres.dat of the install").unwrap();
    std::fs::write(
        ModernDat::Local.in_dir(&install),
        "client_local_English.dat laid out otherwise",
    )
    .unwrap();
    let listed = pins.replace(
        &format!(
            "ac1_local_sha256 = \"{}\"",
            sha(b"client_local_English.dat of the install")
        ),
        &format!(
            "ac1_local_sha256 = [\"{}\", \"{}\"]",
            sha(b"client_local_English.dat of the install"),
            sha(b"client_local_English.dat laid out otherwise")
        ),
    );
    assert_ne!(listed, pins, "the pin is a list");
    check_inputs(&Repo::open(&repo(&f, Some(&listed))).unwrap(), &install).unwrap();
    // Pinned as the one copy, it is refused.
    let one = Repo::open(&repo(&f, Some(&pins))).unwrap();
    let why = check_inputs(&one, &install).unwrap_err();
    assert!(
        why.contains("client_local_English.dat: expected") && !why.contains("highres"),
        "{why}"
    );

    // A world pinning its land's files alone asks nothing of the install.
    let (_, only_land) = land(&f);
    let bare = Repo::open(&repo(&f, Some(&only_land))).unwrap();
    check_inputs(&bare, &install).unwrap();
}

/// A repository is the world builder's world at the layer it edits: every layer's packs up to it
/// in order, its lock beside its world file, its build elsewhere. Two packs of one name are
/// refused. A land edit is written into the edited layer's pack and nowhere else.
#[test]
fn a_repository_is_the_world_of_its_layers_packs_and_its_land_edits_go_into_the_edited_layer() {
    let f = Fixture::new("repo-world");
    let path = repo(&f, None);
    let repo = Repo::open(&path).unwrap();
    let out = f.path().join("out");
    let world = world_of(&repo, 1, None, &out).unwrap();
    let root = path.parent().unwrap();
    assert_eq!(
        world.packs,
        [
            root.join("base/library"),
            root.join("base/emergence"),
            root.join("patches/2027-01-fixes/packs/fixes")
        ]
    );
    assert_eq!(world.lock_path(), root.join("world.lock"));
    assert_eq!(world.key, "test-repo");
    assert_eq!(world_of(&repo, 0, None, &out).unwrap().packs.len(), 2);

    let mut s = Session::open_world(world, &f.dats(), None, None).unwrap();
    s.apply(&Op::SetVertices(vec![VertexSet {
        gx: 16,
        gy: 9,
        field: Field::Height,
        value: 30,
    }]))
    .unwrap();
    let written = s.save().unwrap();
    assert!(!written.is_empty());
    for w in &written {
        assert!(
            w.starts_with(root.join("patches/2027-01-fixes/packs/fixes/edits"))
                || *w == root.join("world.lock"),
            "{}",
            w.display()
        );
    }

    // A pack named as another, by its header: the session refuses it.
    f.write(
        "repo/patches/2027-01-fixes/packs/fixes/pack.toml",
        "name = \"emergence\"\nversion = \"1\"\n",
    );
    let repo = Repo::open(&path).unwrap();
    let world = world_of(&repo, 1, None, &out).unwrap();
    let why = Session::open_world(world, &f.dats(), None, None)
        .expect_err("refused")
        .to_string();
    assert!(why.contains("two packs are named emergence"), "{why}");
    // And by its folder: the world is refused.
    f.write(
        "repo/patches/2027-01-fixes/patch.toml",
        "id = \"2027-01-fixes\"\ntitle = \"Fixes\"\ndate = \"2027-01-01\"\nafter = \"\"\npacks = [\"emergence\"]\n",
    );
    f.write(
        "repo/patches/2027-01-fixes/packs/emergence/pack.toml",
        "name = \"later\"\nversion = \"1\"\n",
    );
    let repo = Repo::open(&path).unwrap();
    let why = world_of(&repo, 1, None, &out).unwrap_err();
    assert!(why.contains("two packs are named emergence"), "{why}");
}

/// A lockfile two branches each added an id to, merged with git's markers in it, is regenerated:
/// every id kept, but the one both gave to two symbols, which the earlier pack's keeps; the later
/// pack's symbol is numbered again, and its own pack's rows that named the old id name the new one.
/// The earlier pack's rows are left as they are.
#[test]
fn a_merged_lockfile_is_regenerated_and_a_symbol_numbered_again_takes_its_rows_with_it() {
    let f = Fixture::new("repo-lock");
    let path = repo(&f, None);
    let root = path.parent().unwrap().to_path_buf();
    let texture = |name: &str| {
        format!(
            "[[surface-texture]]\nname = \"{name}\"\nlevels = [0x{:08X}]\n",
            crate::common::PICTURE
        )
    };
    f.write("repo/base/emergence/records/stone.toml", &texture("stone"));
    f.write(
        "repo/patches/2027-01-fixes/packs/fixes/records/moss.toml",
        &texture("moss"),
    );
    let row =
        |class: u32| format!("[[weenie]]\nclass = {class}\n\n[weenie.did]\nSurface = 0x05000011\n");
    f.write("repo/base/emergence/weenie/2000000.toml", &row(2_000_000));
    f.write(
        "repo/patches/2027-01-fixes/packs/fixes/weenie/2000001.toml",
        &row(2_000_001),
    );
    f.write(
        "repo/world.lock",
        "# This world's ids.\n\n[surface-texture]\n<<<<<<< ours\n\"emergence:stone\" = 0x05000011\n=======\n\"fixes:moss\" = 0x05000011\n>>>>>>> theirs\n",
    );
    let repo = Repo::open(&path).unwrap();
    let why = lyceum_foundry::repo_world::check_lock(&repo, None).unwrap_err();
    assert!(why.contains("never by hand: lyceum lock"), "{why}");
    let world = world_of(&repo, 1, None, &f.path().join("out")).unwrap();
    let done = lyceum_foundry::lock_merge::rebuild(world, &f.dats()).unwrap();
    lyceum_foundry::repo_world::check_lock(&repo, None).unwrap();
    assert_eq!(done.len(), 1, "{done:?}");
    let moved = &done[0];
    assert_eq!(
        (moved.symbol.as_str(), moved.from),
        ("fixes:moss", 0x0500_0011)
    );
    assert_ne!(moved.to, 0x0500_0011);
    let lock = std::fs::read_to_string(root.join("world.lock")).unwrap();
    assert!(!lock.contains("<<<<<<<"), "{lock}");
    assert!(lock.contains("\"emergence:stone\" = 0x05000011"), "{lock}");
    assert!(
        lock.contains(&format!("\"fixes:moss\" = 0x{:08X}", moved.to)),
        "{lock}"
    );
    let fixed = root.join("patches/2027-01-fixes/packs/fixes/weenie/2000001.toml");
    assert_eq!(moved.rows, std::slice::from_ref(&fixed));
    assert!(std::fs::read_to_string(&fixed)
        .unwrap()
        .contains(&format!("Surface = 0x{:08X}", moved.to)));
    assert!(
        std::fs::read_to_string(root.join("base/emergence/weenie/2000000.toml"))
            .unwrap()
            .contains("Surface = 0x05000011")
    );
}

/// A world repository's world builder opens where its characters start: the destination of the
/// weenie its first `Start:` point names, as the newest layer moves it; none without one.
#[test]
fn a_repository_starts_where_its_characters_do() {
    let f = Fixture::new("repo-start");
    let path = repo(&f, None);
    assert_eq!(
        lyceum_foundry::repo_world::start_landblock(&Repo::open(&path).unwrap()),
        None
    );
    f.write(
        "repo/base/emergence/poi/Start__Arwic.toml",
        "[[poi]]\nname = \"Start: Arwic\"\nweenie_class_id = 2300001\n",
    );
    f.write(
        "repo/base/emergence/weenie/2300001.toml",
        "[[weenie]]\nclass = 2300001\nclass_name = \"start-arwic\"\ntype = \"Portal\"\n\n[weenie.position]\nDestination = { cell = 0x6498002C, at = [122.5, 76.5, 128.0], rotation = [1.0, 0.0, 0.0, 0.0] }\n",
    );
    assert_eq!(
        lyceum_foundry::repo_world::start_landblock(&Repo::open(&path).unwrap()),
        Some(0x6498)
    );
    f.write(
        "repo/patches/2027-01-fixes/packs/fixes/weenie/2300001.toml",
        "[[weenie]]\nclass = 2300001\n\n[weenie.position]\nDestination = { cell = 0x64990001, at = [12.0, 12.0, 128.0], rotation = [1.0, 0.0, 0.0, 0.0] }\n",
    );
    assert_eq!(
        lyceum_foundry::repo_world::start_landblock(&Repo::open(&path).unwrap()),
        Some(0x6499)
    );
}
