//! Behaviour: none (tooling: the pack format the world builders share)
//! The shared grammar of worlds, packs, symbols, the lockfile and conflicts, spoken in a small
//! dialect of its own so that no game's vocabulary is needed.

use std::fmt;
use std::path::{Path, PathBuf};

use dereth_pack::conflict::{settle, Claim, Item, Override, Scope, Settled};
use dereth_pack::pack::{PackHeader, Remove};
use dereth_pack::store::{DiskStore, SourceStore};
use dereth_pack::{Dialect, Error, Loc, Lock, Numbering, SymbolKind, World};

/// The test's game: two record kinds, a counted kind a world may start elsewhere, a counted kind
/// it may not, and one numbered in its landblock.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
struct Toy;

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
enum ToyKind {
    Mesh,
    Picture,
    Class,
    Table,
    Spot,
}

impl fmt::Display for ToyKind {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.kind_name())
    }
}

impl SymbolKind for ToyKind {
    const KINDS: &'static [Self] = &[
        Self::Mesh,
        Self::Picture,
        Self::Class,
        Self::Table,
        Self::Spot,
    ];

    fn kind_name(self) -> &'static str {
        match self {
            Self::Mesh => "mesh",
            Self::Picture => "picture",
            Self::Class => "class",
            Self::Table => "table",
            Self::Spot => "spot",
        }
    }

    fn numbering(self) -> Numbering {
        match self {
            Self::Mesh => Numbering::Record {
                first: 0x0100_0000,
                last: 0x01FF_FFFF,
            },
            Self::Picture => Numbering::Record {
                first: 0x0600_0000,
                last: 0x06FF_FFFF,
            },
            Self::Class => Numbering::Counted {
                start: 5000,
                world_sets: true,
            },
            Self::Table => Numbering::Counted {
                start: 70,
                world_sets: false,
            },
            Self::Spot => Numbering::InLandblock,
        }
    }

    fn written_in_hex(self) -> bool {
        !matches!(self, Self::Class | Self::Table)
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum ToyEra {
    Early,
    Late,
}

impl Dialect for Toy {
    const GAME: &'static str = "toy";
    const PRODUCT: &'static str = "Toybox";
    const BUILD_COMMAND: &'static str = "toybox build";
    const ITEM_FORMS: &'static str =
        "record <id>, class <n>, landblock <XXYY>, or a new record's kind and symbol";
    const NAMES_BASE: bool = true;
    type Kind = ToyKind;
    type Era = ToyEra;

    fn era(name: Option<&str>) -> Result<ToyEra, String> {
        match name {
            None | Some("late") => Ok(ToyEra::Late),
            Some("early") => Ok(ToyEra::Early),
            Some(other) => Err(format!("{other:?} is not an era: early or late")),
        }
    }

    fn item(_text: &str, words: &[&str]) -> Option<String> {
        match words {
            ["class", n] => n.parse::<u32>().ok().map(|n| format!("class {n}")),
            _ => None,
        }
    }
}

/// A folder of its own under the system's temporary folder, removed when dropped.
struct Folder(PathBuf);

impl Folder {
    fn new(tag: &str) -> Self {
        let dir = std::env::temp_dir().join(format!(
            "dereth-pack-{tag}-{}-{:?}",
            std::process::id(),
            std::thread::current().id()
        ));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        Self(dir)
    }

    fn write(&self, rel: &str, text: &str) -> PathBuf {
        let path = self.0.join(rel);
        std::fs::create_dir_all(path.parent().unwrap()).unwrap();
        std::fs::write(&path, text).unwrap();
        path
    }
}

impl Drop for Folder {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.0);
    }
}

fn world(text: &str) -> Result<World<Toy>, Error> {
    World::parse(Path::new("w/world.toml"), text)
}

#[test]
fn a_world_that_names_no_game_is_the_first_game_s_and_another_game_s_world_is_refused() {
    // A dialect whose game is not the default reads only worlds that name it.
    let e = world("key = \"w\"\n").unwrap_err().to_string();
    assert!(e.contains("is for \"ac1\"") && e.contains("Toybox"), "{e}");
    let w = world("key = \"w\"\ngame = \"toy\"\n").unwrap();
    assert_eq!(w.era, ToyEra::Late);
    assert_eq!(w.out, Path::new("w").join("build"));
    let e = world("key = \"w\"\ngame = \"other\"\n")
        .unwrap_err()
        .to_string();
    assert!(e.contains("is for \"other\""), "{e}");
}

#[test]
fn a_world_s_era_and_counted_starts_are_read_in_the_dialect_s_words() {
    let w = world("key = \"w\"\ngame = \"toy\"\nera = \"early\"\n[ids]\nclass = 9000\n").unwrap();
    assert_eq!(w.era, ToyEra::Early);
    assert_eq!(w.id_start(ToyKind::Class), 9000);
    assert_eq!(w.id_start(ToyKind::Table), 70, "the kind's own start");
    assert_eq!(w.id_start(ToyKind::Mesh), 0);
    let e = world("key = \"w\"\ngame = \"toy\"\nera = \"middle\"\n")
        .unwrap_err()
        .to_string();
    assert!(e.contains("not an era"), "{e}");
    // A counted kind the world may not start elsewhere is no key of `[ids]`.
    let e = world("key = \"w\"\ngame = \"toy\"\n[ids]\ntable = 5\n")
        .unwrap_err()
        .to_string();
    assert!(e.contains("unknown field `table`, expected `class`"), "{e}");
}

#[test]
fn an_override_names_the_shared_forms_and_the_dialect_s_items() {
    let w = world(
        "key = \"w\"\ngame = \"toy\"\n\
         [[override]]\npack = \"b\"\nover = \"a\"\n\
         items = [\"landblock a9b4\", \"class 12\", \"record 0x0100ABCD\", \"picture a:stripes\"]\n",
    )
    .unwrap();
    assert_eq!(
        w.overrides[0].items.as_deref().unwrap(),
        &[
            Scope::Landblock(0xA9B4),
            Scope::Item("class 12".into()),
            Scope::Item("record 0x0100ABCD".into()),
            Scope::Item("picture a:stripes".into()),
        ]
    );
    // A kind that is no record, and an item of no form, are refused with the dialect's forms.
    for bad in ["class a:guide-x", "spot a:x@A9B4", "weenie 12"] {
        let e = Scope::parse::<Toy>(bad).unwrap_err();
        assert!(
            e.contains("is not an item: record <id>, class <n>"),
            "{bad}: {e}"
        );
    }
    let e = world(
        "key = \"w\"\ngame = \"toy\"\n\
         [[override]]\npack = \"a\"\nover = \"b\"\n\
         [[override]]\npack = \"b\"\nover = \"a\"\n",
    )
    .unwrap_err()
    .to_string();
    assert!(e.contains("each win everything over the other"), "{e}");
}

#[test]
fn the_lockfile_lists_kinds_in_the_game_s_order_and_writes_ids_as_each_kind_does() {
    let mut lock = Lock::<Toy>::default();
    lock.insert(ToyKind::Spot, "a:guide@A9B4", 0x7A9B_4FFF);
    lock.insert(ToyKind::Class, "a:guide", 5000);
    lock.insert(ToyKind::Mesh, "a:post", 0x0100_1234);
    let text = lock.render();
    assert_eq!(
        text,
        "# This world's ids, numbered by toybox build. Commit it with world.toml: a rebuild\n\
         # keeps every id here.\n\
         \n[mesh]\n\"a:post\" = 0x01001234\n\
         \n[class]\n\"a:guide\" = 5000\n\
         \n[spot]\n\"a:guide@A9B4\" = 0x7A9B4FFF\n"
    );
    let back = Lock::<Toy>::parse(Path::new("world.lock"), &text).unwrap();
    assert_eq!(back, lock);
    assert!(back.holds_record_id(0x0100_1234));
    assert!(!back.holds_record_id(5000), "a class is no record");
    let e = Lock::<Toy>::parse(Path::new("world.lock"), "[wcid]\n\"a:b\" = 1\n")
        .unwrap_err()
        .to_string();
    assert!(e.contains("[wcid] is not a kind of symbol"), "{e}");
}

#[test]
fn a_missing_lockfile_is_an_empty_one() {
    let f = Folder::new("lock");
    let lock = Lock::<Toy>::read(&f.0.join("world.lock")).unwrap();
    assert!(lock.is_empty());
}

#[test]
fn a_pack_header_names_its_game_and_a_pack_of_another_game_is_refused() {
    let at = Path::new("p/pack.toml");
    let h =
        PackHeader::parse::<Toy>(at, "name = \"p\"\nversion = \"1\"\ngame = \"toy\"\n").unwrap();
    assert_eq!(h.name, "p");
    let e = PackHeader::parse::<Toy>(at, "name = \"p\"\nversion = \"1\"\n")
        .unwrap_err()
        .to_string();
    assert!(e.contains("this pack is for \"ac1\""), "{e}");
    let e = PackHeader::parse::<Toy>(at, "name = \"P q\"\nversion = \"1\"\ngame = \"toy\"\n")
        .unwrap_err()
        .to_string();
    assert!(e.contains("is not a pack name"), "{e}");
    let e = PackHeader::parse::<Toy>(at, "format_version = 9\nname = \"p\"\nversion = \"1\"\n")
        .unwrap_err()
        .to_string();
    assert!(e.contains("needs a newer Toybox"), "{e}");
}

#[test]
fn an_upgrade_rewrites_only_the_format_version_and_keeps_backups() {
    let f = Folder::new("upgrade");
    let world = f.write(
        "world.toml",
        "# my world\nkey = \"w\"\ngame = \"toy\"\npacks = [\"p\"]\n",
    );
    let pack = f.write("p/pack.toml", "name = \"p\" # mine\nversion = \"1\"\n");
    let lock = f.write("world.lock", "[class]\n\"p:x\" = 5000\n");
    let files = [world.clone(), pack.clone()];
    let would = dereth_pack::format::upgrade_files::<Toy>(&DiskStore, &files, &lock, true).unwrap();
    assert_eq!(would.len(), 2);
    assert!(!world.with_extension("toml.before-upgrade").exists());
    let done = dereth_pack::format::upgrade_files::<Toy>(&DiskStore, &files, &lock, false).unwrap();
    assert_eq!(done.len(), 2);
    let text = DiskStore.read_to_string(&world).unwrap();
    assert!(text.starts_with("# my world\n") && text.contains("format_version = 1"));
    assert_eq!(
        DiskStore
            .read_to_string(&world.with_extension("toml.before-upgrade"))
            .unwrap(),
        "# my world\nkey = \"w\"\ngame = \"toy\"\npacks = [\"p\"]\n"
    );
    assert!(lock.with_extension("lock.before-upgrade").is_file());
    let again =
        dereth_pack::format::upgrade_files::<Toy>(&DiskStore, &files, &lock, false).unwrap();
    assert!(again.is_empty(), "already current");
}

#[test]
fn claims_that_agree_stand_together_and_an_override_settles_those_that_do_not() {
    let at = |line| Loc::new(Path::new("x.toml"), line);
    let item = Item::new("class 12").in_landblock(0xA9B4);
    let a = Claim::new("a", &at(1), "sets it to 1", "1");
    let b = Claim::new("b", &at(2), "sets it to 2", "2");
    let mut out = Settled::default();
    assert_eq!(settle(&[], &item, &[a.clone(), a.clone()], &mut out), 1);
    assert_eq!(out, Settled::default());
    assert_eq!(settle(&[], &item, &[a.clone(), b.clone()], &mut out), 1);
    assert_eq!(out.conflicts.len(), 1);
    assert_eq!(
        out.conflicts[0].line(),
        "x.toml:1: class 12: a sets it to 1; b sets it to 2 (x.toml:2)"
    );
    let by_landblock = Override {
        pack: "a".into(),
        over: "b".into(),
        items: Some(vec![Scope::Landblock(0xA9B4)]),
        at: at(9),
    };
    let mut out = Settled::default();
    assert_eq!(settle(&[by_landblock], &item, &[a, b], &mut out), 0);
    assert!(out.conflicts.is_empty());
    assert_eq!(
        out.overridden[0].to_string(),
        "class 12: a's (x.toml:1) over b's"
    );
}

#[derive(serde::Deserialize)]
struct Row {
    #[serde(default)]
    prov: Option<dereth_pack::provenance::Provenance>,
}

fn row(text: &str) -> Result<Row, toml::de::Error> {
    toml::from_str(text)
}

#[test]
fn a_row_says_where_it_came_from_and_what_each_source_needs_and_says_nothing_when_a_person_wrote_it(
) {
    use dereth_pack::provenance::{Confidence, Provenance, Rows, Source};
    assert_eq!(row("").unwrap().prov, None);
    let dat = row("prov = { from = \"dat\", record = 0x0200040B }\n")
        .unwrap()
        .prov
        .unwrap();
    assert_eq!((dat.source, dat.record), (Source::Dat, Some(0x0200_040B)));
    assert_eq!(dat.to_string(), "dat: record 0x0200040B");
    let measured = row(
        "prov = { from = \"capture\", evidence = \"EVID-1\", ref = \"camp 12\", confidence = \"low\", sample = { creatures = 2, sessions = 1 }, fields = { regen = { from = \"derived\", rule = \"regen-median\" }, radius = { from = \"authored\" } } }\n",
    )
    .unwrap()
    .prov
    .unwrap();
    assert_eq!(measured.source, Source::Capture);
    assert_eq!(measured.confidence, Some(Confidence::Low));
    assert_eq!(
        measured.to_string(),
        "capture (low; creatures 2, sessions 1): camp 12"
    );
    assert_eq!(measured.of_field("regen").source, Source::Derived);
    assert_eq!(
        measured.of_field("regen").rule.as_deref(),
        Some("regen-median")
    );
    assert!(measured.of_field("radius").is_plain_authored());
    assert_eq!(measured.of_field("population"), &measured, "the row's own");
    assert_eq!(measured.source.letter(), 'C');
    for (bad, why) in [
        ("prov = { from = \"guess\" }\n", "unknown variant"),
        (
            "prov = { from = \"dat\", record = 1, sure = true }\n",
            "unknown field",
        ),
        ("prov = { from = \"dat\" }\n", "from dat names its record"),
        (
            "prov = { from = \"derived\" }\n",
            "from derived names its rule",
        ),
        (
            "prov = { from = \"capture\", evidence = \"E\", ref = \"r\" }\n",
            "from capture names its confidence",
        ),
    ] {
        let e = row(bad).err().expect(bad).to_string();
        assert!(e.contains(why), "{bad}: {e}");
    }
    let at = Loc::new(Path::new("p/records/a.toml"), 3);
    let mut rows = Rows::default();
    rows.note(&at, Some(&measured));
    rows.note(&Loc::new(Path::new("p/records/a.toml"), 9), None);
    rows.note(
        &Loc::new(Path::new("p/records/a.toml"), 12),
        Some(&Provenance::AUTHORED),
    );
    assert_eq!(rows.of(&at), &measured);
    assert!(rows
        .of(&Loc::new(Path::new("p/records/a.toml"), 9))
        .is_plain_authored());
    assert_eq!(rows.iter().count(), 1, "a person's plain rows are not kept");
}

#[test]
fn a_world_names_its_base_database_and_a_remove_row_takes_out_one_item_of_it() {
    let w = world(
        "key = \"w\"
game = \"toy\"
[base]
db = \"toy-db\"
version = \"v0.3.1\"\n",
    )
    .unwrap();
    let base = w.base.unwrap();
    assert_eq!(
        (base.db.as_str(), base.version.as_deref()),
        ("toy-db", Some("v0.3.1"))
    );
    let w = world(
        "key = \"w\"
game = \"toy\"
[base]
db = \"toy-db\"\n",
    )
    .unwrap();
    assert_eq!(w.base.unwrap().version, None, "the release the build pins");
    assert_eq!(
        world(
            "key = \"w\"
game = \"toy\"\n"
        )
        .unwrap()
        .base,
        None
    );
    let at = Loc::new(Path::new("p/server/a.toml"), 4);
    let remove: Remove = toml::from_str(
        "item = \"class 0012\"
prov = { from = \"authored\", note = \"gone\" }\n",
    )
    .unwrap();
    assert_eq!(remove.item_name::<Toy>(&at).unwrap(), "class 12");
    assert_eq!(remove.prov.unwrap().note.as_deref(), Some("gone"));
    let whole: Remove = toml::from_str("item = \"landblock A9B4\"\n").unwrap();
    let e = whole.item_name::<Toy>(&at).unwrap_err().to_string();
    assert!(
        e.starts_with("p/server/a.toml:4") || e.contains("a.toml:4"),
        "{e}"
    );
    assert!(e.contains("not a landblock"), "{e}");
}

#[test]
fn a_session_holds_text_against_the_store_undoes_and_redoes_as_text_and_saves_only_what_differs() {
    use dereth_pack::session::{History, Sources, Step, Texts};
    use dereth_pack::store::MemoryStore;
    let store = MemoryStore::default();
    let a = PathBuf::from("p/edits/a.toml");
    let b = PathBuf::from("p/edits/b.toml");
    store.write(&a, b"one\n").unwrap();
    let mut sources = Sources::default();
    let mut history = History::default();
    assert_eq!(sources.text(&store, &a).as_deref(), Some("one\n"));
    assert_eq!(sources.text(&store, &b), None, "no file");
    assert!(!sources.unsaved());
    // A change to both files, as one step.
    let after: Texts = [
        (a.clone(), Some("two\n".to_owned())),
        (b.clone(), Some("new\n".into())),
    ]
    .into_iter()
    .collect();
    let before: Texts = after
        .keys()
        .map(|p| (p.clone(), sources.text(&store, p)))
        .collect();
    for (p, t) in &after {
        sources.put(&store, p, t.clone());
    }
    history.record(Step {
        before,
        after: after.clone(),
    });
    assert!(sources.unsaved() && sources.differs(&a));
    assert_eq!(sources.stored(&store, &a).as_deref(), Some("one\n"));
    // Undo puts the text before back; redo the text after.
    let step = history.take_undo().unwrap();
    for (p, t) in &step.before {
        sources.put(&store, p, t.clone());
    }
    history.undone(step);
    assert_eq!(history.counts(), (0, 1));
    assert!(!sources.unsaved(), "back as the store has it");
    let step = history.take_redo().unwrap();
    for (p, t) in &step.after {
        sources.put(&store, p, t.clone());
    }
    history.push(step);
    // Only what differs is written, and nothing twice.
    assert_eq!(sources.save(&store).unwrap(), vec![a.clone(), b.clone()]);
    assert_eq!(store.read_to_string(&b).unwrap(), "new\n");
    assert!(sources.save(&store).unwrap().is_empty());
    // A file the session took away again is removed.
    sources.put(&store, &b, None);
    assert_eq!(sources.save(&store).unwrap(), vec![b.clone()]);
    assert!(!store.is_file(&b));
}

#[test]
fn steps_merged_are_one_step_holding_the_text_before_the_first_and_after_the_last() {
    use dereth_pack::session::{History, Step, Texts};
    let a = PathBuf::from("a");
    let step = |before: &str, after: &str| Step {
        before: [(a.clone(), Some(before.to_owned()))]
            .into_iter()
            .collect::<Texts>(),
        after: [(a.clone(), Some(after.to_owned()))]
            .into_iter()
            .collect::<Texts>(),
    };
    let mut h = History::default();
    h.record(step("0", "1"));
    let start = h.len();
    h.push(step("1", "2"));
    h.push(step("2", "3"));
    h.merge_since(start);
    assert_eq!(h.counts(), (2, 0));
    h.merge_since(0);
    let merged = h.take_undo().unwrap();
    assert_eq!(merged, step("0", "3"));
}

#[test]
fn the_lockfile_is_saved_only_when_its_ids_changed_and_never_made_empty() {
    use dereth_pack::store::MemoryStore;
    let store = MemoryStore::default();
    let path = Path::new("w/world.lock");
    let mut lock = Lock::<Toy>::default();
    assert!(!lock.save(&store, path).unwrap(), "no ids, no file");
    lock.insert(ToyKind::Class, "a:x", 5000);
    assert!(lock.save(&store, path).unwrap());
    assert!(!lock.save(&store, path).unwrap(), "the same text");
    assert_eq!(Lock::<Toy>::read_from(&store, path).unwrap(), lock);
}
