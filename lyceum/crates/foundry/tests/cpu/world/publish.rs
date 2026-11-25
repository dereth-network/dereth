//! Behaviour: none (tooling: a world repository published for its operator's server, its overlay numbered from the repository's ledger)

use std::cell::RefCell;
use std::ffi::OsString;
use std::path::{Path, PathBuf};

use dereth_dat::overlay::OverlayDir;
use dereth_dat::ModernDat;
use lyceum_foundry::export::Importer;
use lyceum_foundry::publish::{publish, Publish};

use crate::common::{Fixture, PICTURE};

/// The server's import tool, as a test has it: it writes a world pack where it is told, and
/// remembers what it was asked.
#[derive(Default)]
struct Tool {
    asked: RefCell<Vec<Vec<OsString>>>,
}

impl Importer for Tool {
    fn run(&self, args: &[OsString]) -> Result<String, String> {
        self.asked.borrow_mut().push(args.to_vec());
        let out = args
            .iter()
            .position(|a| a == "--out")
            .and_then(|i| args.get(i + 1))
            .ok_or("no --out")?;
        std::fs::write(out, b"a world pack").map_err(|e| e.to_string())?;
        Ok("built".into())
    }
}

/// A repository whose base's own pack makes a surface texture of the picture `levels` names.
fn repo(f: &Fixture, picture: u32) -> PathBuf {
    f.write(
        "repo/base/library/pack.toml",
        "name = \"library\"\nversion = \"1\"\n",
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
        "repo/base/emergence/records/stone.toml",
        &format!("[[surface-texture]]\nname = \"stone\"\nlevels = [0x{picture:08X}]\n"),
    );
    f.write(
        "repo/world.toml",
        "format_version = 1\ngame = \"ac1\"\nkey = \"test-repo\"\nera = \"newdereth\"\npacks = [\"base/library\", \"base/emergence\"]\nlineage = []\n[base]\nmonth = \"2026-12\"\n[land]\nledger = \"land/generations.tsv\"\n",
    )
}

/// The served overlay's own iterations over the client's portal file.
fn iterations(f: &Fixture, out: &Path) -> Vec<u32> {
    let base =
        lyceum_foundry::dats::DatSet::for_world(&f.dats(), dereth_primitives::EraId::Eor).unwrap();
    OverlayDir::new(&out.join("overlay"))
        .unwrap()
        .layer_over(
            ModernDat::Portal,
            base.file(ModernDat::Portal).unwrap(),
            None,
        )
        .unwrap()
        .expect("the overlay changes the portal file")
        .own_iterations()
        .to_vec()
}

/// An operator publishes only content its author has numbered: refused until then, naming its
/// digest. The author's publish numbers it (generation 1, one iteration past the files), and the
/// ledger's line is written; an operator's publish of the same content numbers it alike, byte for
/// byte. Changed content is refused again, and its author's publish is the next generation, with
/// one more iteration. Beside the overlay: the world pack the server's tool built at the layer, what
/// the server reads (patching on, the world's era and calendar), and every file's hash.
#[test]
fn a_published_worlds_overlay_is_numbered_from_its_repositorys_ledger() {
    let f = Fixture::new("publish");
    let world = repo(&f, PICTURE);
    let ledger = world.parent().unwrap().join("land/generations.tsv");
    let tool = Tool::default();
    let out = f.path().join("served");
    let operator = Publish {
        layer: None,
        land: None,
        number: false,
        importer: &tool,
    };
    let author = Publish {
        number: true,
        ..operator
    };
    let why = publish(&world, &f.dats(), &out, &operator)
        .unwrap_err()
        .to_string();
    assert!(
        why.contains("has no generation") && why.contains("--number"),
        "{why}"
    );
    assert!(!ledger.exists());

    let first = publish(&world, &f.dats(), &out, &author).unwrap();
    assert_eq!(
        (first.generation, first.numbered, first.layer.as_str()),
        (1, true, "base")
    );
    assert!(std::fs::read_to_string(&ledger)
        .unwrap()
        .contains(&format!("{}\t1", first.digest)));
    let once = iterations(&f, &out);
    assert_eq!(once.len(), 1);
    let asked = tool.asked.borrow().last().cloned().unwrap();
    assert_eq!(&asked[..2], ["world", "build"]);
    assert!(asked.iter().any(|a| a == "--layer") && asked.iter().any(|a| a == "base"));
    let config = std::fs::read_to_string(out.join("empyrean.toml")).unwrap();
    let t: toml::Table = toml::from_str(&config).unwrap();
    assert_eq!(t["dat_overlay"]["patching"].as_bool(), Some(true));
    assert!(
        t["server"]["world_pack_path"]
            .as_str()
            .is_some_and(|p| Path::new(p).is_absolute()),
        "{config}"
    );
    assert_eq!(t["era"]["profile"].as_str(), Some("newdereth"));
    assert_eq!(
        t["era"]["launched_at"].as_str(),
        Some("2026-12-01T00:00:00Z")
    );
    let readme = std::fs::read_to_string(out.join("README.txt")).unwrap();
    for (file, hash) in &first.files {
        assert!(
            readme.contains(&format!(
                "{hash}  {}",
                file.display().to_string().replace('\\', "/")
            )),
            "{readme}"
        );
    }

    // An operator's publish of the same content: the same generation, the same bytes.
    let again = publish(&world, &f.dats(), &out, &operator).unwrap();
    assert_eq!((again.generation, again.numbered), (1, false));
    assert_eq!(again.files, first.files);

    // Changed content: refused to the operator, the next generation to its author.
    repo(&f, PICTURE + 1);
    assert!(publish(&world, &f.dats(), &out, &operator).is_err());
    let second = publish(&world, &f.dats(), &out, &author).unwrap();
    assert_eq!(second.generation, 2);
    let twice = iterations(&f, &out);
    assert_eq!(
        twice,
        [once[0], once[0] + 1],
        "one more iteration, past the first"
    );

    // A folder holding something else is not written over.
    let other = f.path().join("other");
    std::fs::create_dir_all(&other).unwrap();
    std::fs::write(other.join("mine.txt"), "mine").unwrap();
    assert!(publish(&world, &f.dats(), &other, &author).is_err());
}

/// A repository's land lies over the AC1 files it was built over, so publishing over others is
/// refused, naming the file: the overlay a server sent would not be the repository's land. Over
/// the files it pins, it publishes.
#[test]
fn a_repository_is_published_only_over_the_ac1_files_its_land_was_built_over() {
    use sha2::{Digest, Sha256};
    let f = Fixture::new("publish-inputs");
    let world = repo(&f, PICTURE);
    let pinned = |portal: &str| {
        let text = std::fs::read_to_string(&world).unwrap();
        let text = text
            .lines()
            .filter(|l| !l.starts_with("ac1_portal_sha256"))
            .collect::<Vec<_>>()
            .join("\n");
        std::fs::write(
            &world,
            format!("{text}\nac1_portal_sha256 = \"{portal}\"\n"),
        )
        .unwrap();
    };
    let tool = Tool::default();
    let author = Publish {
        layer: None,
        land: None,
        number: true,
        importer: &tool,
    };
    let out = f.path().join("served");
    pinned(&"0".repeat(64));
    let why = publish(&world, &f.dats(), &out, &author)
        .unwrap_err()
        .to_string();
    assert!(why.contains("client_portal.dat: expected"), "{why}");
    assert!(tool.asked.borrow().is_empty(), "nothing built");
    assert!(!out.exists());

    let bytes = std::fs::read(ModernDat::Portal.in_dir(&f.dats())).unwrap();
    let hex: String = Sha256::digest(&bytes)
        .iter()
        .map(|b| format!("{b:02x}"))
        .collect();
    pinned(&hex);
    publish(&world, &f.dats(), &out, &author).unwrap();
}

/// A world with no repository is exported with its overlay numbered by its content, from a ledger
/// beside its world file that the export keeps: the first export is generation 1; an unchanged
/// export keeps it, so a client holding it doesn't patch again; a changed record is the next
/// generation, one iteration on, so a client holding the last one patches.
#[test]
fn a_plain_worlds_export_numbers_its_overlay_by_its_content() {
    use lyceum_foundry::export::{export, Era, Options, Target};
    let f = Fixture::new("export-numbered");
    let world = f.world();
    let textures = |names: &[&str]| {
        let rows: String = names
            .iter()
            .map(|n| format!("[[surface-texture]]\nname = \"{n}\"\nlevels = [0x{PICTURE:08X}]\n"))
            .collect();
        f.write("sample/records/stone.toml", &rows);
    };
    let dump = f.write("ACE-World-Database-v0.9.295.sql", "-- the world database");
    let tool = Tool::default();
    let out = f.path().join("for-empyrean");
    let opts = Options {
        target: Target::Empyrean,
        out: &out,
        era: Era::Eor,
        world_sql: Some(&dump),
        importer: Some(&tool),
        gdle_quests: None,
    };
    let ledger = f.path().join("world.generations.tsv");

    textures(&["stone"]);
    export(&world, &f.dats(), &opts).unwrap();
    let once = iterations(&f, &out);
    assert_eq!(once.len(), 1, "generation 1");
    let lines = std::fs::read_to_string(&ledger).expect("the ledger beside the world");
    assert!(lines.contains("\t1"), "{lines}");

    export(&world, &f.dats(), &opts).unwrap();
    assert_eq!(iterations(&f, &out), once, "unchanged: the same iteration");
    assert_eq!(std::fs::read_to_string(&ledger).unwrap(), lines);

    textures(&["stone", "slate"]);
    export(&world, &f.dats(), &opts).unwrap();
    assert_eq!(
        iterations(&f, &out),
        [once[0], once[0] + 1],
        "changed: one iteration more"
    );
    assert_eq!(
        std::fs::read_to_string(&ledger).unwrap().lines().count(),
        lines.lines().count() + 1,
        "the ledger's new line"
    );
}
