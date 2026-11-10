//! Behaviour: none (checks the world builder's editing: rows placed, moved, turned, changed, removed and put back as pack text, with undo, redo and save)

use std::io;
use std::path::Path;
use std::sync::Arc;

use athenaeum_foundry::edit::{ClearingShape, NewKind, NewRow, NEW_CLEARING};
use athenaeum_foundry::{Layer, Session};
use dereth_pack::store::{MemoryStore, SourceStore};
use yalain_world_db::layer::{self, Base};
use yalain_world_db::rows::{Row, Source};

use crate::{base, dir, session, store, world_files};

const LB: u16 = 0x77B8;
const FILE: &str = "fill-test/server/landblocks/77B8.toml";
const KEEPER: u32 = 0x4700_33D0;

fn person(at: [f32; 3]) -> NewRow {
    NewRow {
        kind: NewKind::Person,
        template: KEEPER,
        landblock: LB,
        at,
        heading: 90.0,
    }
}

fn text<'s>(s: &'s Session, rel: &str) -> &'s str {
    s.files()
        .get(rel)
        .map_or_else(|| panic!("no file {rel}"), String::as_str)
}

fn clean(s: &Session) {
    assert!(s.problems().is_empty(), "{:?}", s.problems());
}

/// A store tests can look into after the session saves.
struct Shared(Arc<MemoryStore>);

impl SourceStore for Shared {
    fn read(&self, path: &Path) -> io::Result<Vec<u8>> {
        self.0.read(path)
    }
    fn is_file(&self, path: &Path) -> bool {
        self.0.is_file(path)
    }
    fn create_new(&self, path: &Path, bytes: &[u8]) -> io::Result<()> {
        self.0.create_new(path, bytes)
    }
    fn replace(&self, path: &Path, temp: &Path, bytes: &[u8]) -> io::Result<()> {
        self.0.replace(path, temp, bytes)
    }
    fn write(&self, path: &Path, bytes: &[u8]) -> io::Result<()> {
        self.0.write(path, bytes)
    }
    fn remove(&self, path: &Path) -> io::Result<()> {
        self.0.remove(path)
    }
}

#[test]
fn a_new_world_shows_the_bases_rows_and_has_no_problems() {
    let s = session();
    clean(&s);
    assert_eq!(s.target(), "fill-test");
    let mut items: Vec<&str> = s.entries().iter().map(|e| e.item.as_str()).collect();
    items.sort_unstable();
    assert_eq!(
        items,
        [
            "camp c-4586@77B8",
            "object n-334c@77B8",
            "object s-5c@77B8",
            "place p-1"
        ]
    );
    assert!(s.entries().iter().all(|e| e.layer == Layer::Base));
}

#[test]
fn placing_a_person_writes_an_authored_row_in_its_landblocks_file() {
    let mut s = session();
    let item = s.place(&person([10.0, 20.5, 30.25])).unwrap();
    assert_eq!(item, "object person-1@77B8");
    assert_eq!(
        text(&s, FILE),
        "[[object]]\nkey = \"person-1\"\nkind = \"npc\"\ntemplate = 0x470033D0\nat = [10.0, 20.5, 30.25]\nheading = 90.0\nprov = { from = \"authored\" }\n"
    );
    clean(&s);
    let e = s.entry(&item).unwrap();
    assert_eq!(e.layer, Layer::Target);
    assert_eq!(e.prov().unwrap().source, Source::Authored);
    assert!(e.was.is_none());
}

#[test]
fn new_rows_take_the_next_free_key_of_their_kind() {
    let mut s = session();
    let a = s.place(&person([10.0, 20.0, 30.0])).unwrap();
    let b = s.place(&person([12.0, 20.0, 30.0])).unwrap();
    let c = s
        .place(&NewRow {
            kind: NewKind::Camp,
            ..person([20.0, 20.0, 30.0])
        })
        .unwrap();
    let d = s
        .place(&NewRow {
            kind: NewKind::Place,
            ..person([30.0, 20.0, 30.0])
        })
        .unwrap();
    assert_eq!(
        [a, b, c, d],
        [
            "object person-1@77B8",
            "object person-2@77B8",
            "camp camp-1@77B8",
            "place place-1"
        ]
    );
    clean(&s);
    // A place names its cell, the land cell under its point.
    assert!(text(&s, "fill-test/server/places.toml").contains("cell = 0x77B8000A"));
}

#[test]
fn moving_a_base_row_writes_a_row_of_its_key_that_replaces_it() {
    let mut s = session();
    let before = s.entries().len();
    let item = s
        .move_to("object n-334c@77B8", LB, [5.0, 6.0, 182.5])
        .unwrap();
    assert_eq!(item, "object n-334c@77B8");
    clean(&s);
    assert_eq!(s.entries().len(), before, "replaced, not added");
    let t = text(&s, FILE);
    assert!(t.contains("key = \"n-334c\""), "{t}");
    assert!(t.contains("at = [5.0, 6.0, 182.5]"), "{t}");
    // The land cell follows the point; the rotation and template are the base's.
    assert!(t.contains("cell = 0x77B80001"), "{t}");
    assert!(t.contains("template = 0x4700334C"), "{t}");
    assert!(t.contains("rotation = [0.2499, 0.0, 0.0, 0.9683]"), "{t}");
    let e = s.entry(&item).unwrap();
    assert_eq!(e.layer, Layer::Target);
    // Where it came from stays the base's, its evidence and how sure with it; the fields the move
    // changed are a person's.
    let prov = e.prov().expect("its provenance");
    assert_eq!(
        (prov.source, prov.evidence.as_deref()),
        (Source::Capture, Some("TEST-OBJECTS"))
    );
    assert!(prov.confidence.is_some());
    let marked: Vec<(&str, Source)> = prov
        .fields
        .iter()
        .map(|(k, p)| (k.as_str(), p.source))
        .collect();
    assert_eq!(
        marked,
        [("at", Source::Authored), ("cell", Source::Authored)]
    );
    let was = e.was.as_ref().expect("what it replaces");
    assert_eq!(was.row.prov().unwrap().source, Source::Capture);
    // The base's files are never written.
    assert!(s.files().keys().all(|f| !f.starts_with("seed/")));
}

#[test]
fn a_moved_portal_keeps_its_destination_in_the_base() {
    let mut s = session();
    s.move_to("object s-5c@77B8", LB, [121.0, 110.0, 150.0])
        .unwrap();
    clean(&s);
    let t = text(&s, FILE);
    assert!(t.contains("to = \"seed:p-1\""), "{t}");
    assert!(t.contains("id = 0x4077B8000000005C"), "{t}");
}

#[test]
fn moving_a_row_into_another_landblock_moves_it_to_that_landblocks_file() {
    let mut s = session();
    let item = s.place(&person([150.0, 20.0, 30.0])).unwrap();
    let moved = s.move_to(&item, 0x78B8, [5.0, 20.0, 31.0]).unwrap();
    assert_eq!(moved, "object person-1@78B8");
    clean(&s);
    assert!(
        !s.files().contains_key(FILE),
        "nothing is left in 77B8's file"
    );
    assert!(text(&s, "fill-test/server/landblocks/78B8.toml").contains("key = \"person-1\""));
    assert!(s.entry(&item).is_none());
    assert_eq!(s.entry(&moved).unwrap().layer, Layer::Target);
}

#[test]
fn moving_a_base_row_into_another_landblock_takes_it_away_where_it_was() {
    let mut s = session();
    let moved = s
        .move_to("camp c-4586@77B8", 0x77B9, [10.0, 10.0, 130.0])
        .unwrap();
    assert_eq!(moved, "camp c-4586@77B9");
    clean(&s);
    assert!(s.entry("camp c-4586@77B8").is_none());
    assert!(text(&s, FILE).contains("items = [\"camp c-4586@77B8\"]"));
    assert_eq!(s.removed().len(), 1);
}

#[test]
fn a_row_placed_in_a_room_names_it_and_moves_from_room_to_room_and_out() {
    let mut s = session();
    let room = (u32::from(LB) << 16) | 0x0105;
    let item = s.place_in(&person([1.0, 2.0, 3.0]), Some(room)).unwrap();
    clean(&s);
    let cell = |s: &Session, i: &str| match &s.entry(i).unwrap().row.row {
        Row::Object(o) => o.cell,
        other => panic!("{other:?}"),
    };
    assert_eq!(cell(&s, &item), Some(room));
    let next = room + 1;
    s.move_into(&item, LB, [4.0, 2.0, 3.0], Some(next)).unwrap();
    assert_eq!(cell(&s, &item), Some(next), "to the next room");
    s.move_to(&item, LB, [5.0, 2.0, 3.0]).unwrap();
    assert_eq!(cell(&s, &item), Some(next), "a plain move keeps the room");
    let land = (u32::from(LB) << 16) | 0x0001;
    s.move_into(&item, LB, [6.0, 2.0, 3.0], Some(land)).unwrap();
    assert_eq!(cell(&s, &item), Some(land), "and out");
    clean(&s);
}

#[test]
fn changes_gathered_together_undo_and_redo_as_one_step() {
    let mut s = session();
    let a = s.place(&person([1.0, 2.0, 3.0])).unwrap();
    let b = s.place(&person([4.0, 2.0, 3.0])).unwrap();
    let steps = s.steps().0;
    s.begin_step();
    s.move_to(&a, LB, [10.0, 2.0, 3.0]).unwrap();
    s.move_to(&b, LB, [20.0, 2.0, 3.0]).unwrap();
    s.end_step();
    assert_eq!(s.steps().0, steps + 1);
    s.undo();
    assert_eq!(s.entry(&a).unwrap().at().x, 1.0);
    assert_eq!(s.entry(&b).unwrap().at().x, 4.0);
    s.redo();
    assert_eq!(s.entry(&a).unwrap().at().x, 10.0);
    assert_eq!(s.entry(&b).unwrap().at().x, 20.0);
}

#[test]
fn a_place_renamed_takes_what_leads_to_it_along_as_one_step() {
    let mut s = session();
    let place = s
        .place(&NewRow {
            kind: NewKind::Place,
            ..person([5.0, 6.0, 7.0])
        })
        .unwrap();
    let door = s.place(&person([1.0, 2.0, 3.0])).unwrap();
    let old = match &s.entry(&place).unwrap().row.row {
        Row::Place(p) => p.key.clone(),
        other => panic!("{other:?}"),
    };
    s.set(&door, "to", &format!("\"{old}\"")).unwrap();
    let steps = s.steps().0;
    let renamed = s.rename_place(&place, "the-inn").unwrap();
    assert_eq!(s.steps().0, steps + 1, "one step");
    clean(&s);
    assert!(s.entry(&place).is_none());
    assert!(s.entry(&renamed).is_some());
    let to = |s: &Session| match &s.entry(&door).unwrap().row.row {
        Row::Object(o) => o.to.clone(),
        other => panic!("{other:?}"),
    };
    assert_eq!(to(&s).as_deref(), Some("the-inn"));
    assert!(s.rename_place(&renamed, "not a key").is_err());
    assert!(s.rename_place(&door, "elsewhere").is_err(), "not a place");
    s.undo();
    assert!(s.entry(&place).is_some(), "one undo takes it all back");
    assert_eq!(to(&s), Some(old));
}

#[test]
fn a_row_copied_from_a_room_stands_where_it_goes() {
    let mut s = session();
    let item = s
        .place_in(&person([1.0, 2.0, 3.0]), Some(0x77B7_0105))
        .unwrap();
    let cell = |s: &Session, i: &str| match &s.entry(i).unwrap().row.row {
        Row::Object(o) => o.cell,
        other => panic!("{other:?}"),
    };
    assert_eq!(cell(&s, &item), Some(0x77B7_0105));
    let kept = s.duplicate(&item, LB, [4.0, 2.0, 3.0]).unwrap();
    assert_eq!(
        cell(&s, &kept),
        Some(0x77B7_0105),
        "beside it: the room is kept"
    );
    let out = s.copy_to(&item, LB, [4.0, 2.0, 3.0], None).unwrap();
    let land = cell(&s, &out).expect("a land cell");
    assert!(land & 0xFFFF < 0x100, "{land:08X}");
    let other = s
        .copy_to(&item, LB, [4.0, 2.0, 3.0], Some(0x77B7_0106))
        .unwrap();
    assert_eq!(cell(&s, &other), Some(0x77B7_0106));
    clean(&s);
}

#[test]
fn an_objects_scale_is_set_kept_and_taken_away() {
    let mut s = session();
    let item = s.place(&person([1.0, 2.0, 3.0])).unwrap();
    s.set(&item, "scale", "1.5").unwrap();
    clean(&s);
    let scale = |s: &Session| match &s.entry(&item).unwrap().row.row {
        Row::Object(o) => o.scale,
        other => panic!("{other:?}"),
    };
    assert_eq!(scale(&s), Some(1.5));
    s.move_to(&item, LB, [4.0, 2.0, 3.0]).unwrap();
    assert_eq!(scale(&s), Some(1.5), "a move keeps it");
    s.set(&item, "scale", "").unwrap();
    assert_eq!(scale(&s), None);
}

#[test]
fn a_copy_of_a_row_has_a_key_of_its_own_and_the_rest_as_it_was() {
    let mut s = session();
    let first = s.place(&person([1.0, 2.0, 3.0])).unwrap();
    s.set(
        &first,
        "name",
        "{ table = 0x25001B44, string = 0x8FE5C6EE }",
    )
    .unwrap();
    let copy = s.duplicate(&first, LB, [4.0, 5.0, 6.0]).unwrap();
    assert_ne!(copy, first);
    clean(&s);
    let at = s.entry(&copy).unwrap().at();
    assert_eq!((at.x, at.y, at.z), (4.0, 5.0, Some(6.0)));
    let Row::Object(o) = &s.entry(&copy).unwrap().row.row else {
        panic!("an object")
    };
    assert_eq!(o.template, KEEPER);
    assert_eq!(o.name.map(|n| n.string), Some(0x8FE5_C6EE));
    assert_eq!(o.heading, Some(90.0));
    let at = s.entry(&first).unwrap().at();
    assert_eq!(at.x, 1.0, "the first stays where it stood");
    s.undo();
    assert!(s.entry(&copy).is_none(), "one step undoes it");
    assert!(s.entry(&first).is_some());
}

#[test]
fn a_copy_of_a_base_row_is_a_new_row_and_the_base_row_stays() {
    let mut s = session();
    let base_row = s
        .entries()
        .iter()
        .find(|e| e.layer == Layer::Base && matches!(e.row.row, Row::Object(_)))
        .map(|e| e.item.clone())
        .expect("a row of the base");
    let copy = s.duplicate(&base_row, LB, [9.0, 9.0, 1.0]).unwrap();
    clean(&s);
    assert_eq!(s.entry(&copy).unwrap().layer, Layer::Target);
    assert_eq!(s.entry(&copy).unwrap().was, None);
    assert_eq!(s.entry(&base_row).unwrap().layer, Layer::Base);
}

#[test]
fn turning_a_row_writes_a_heading_in_place_of_its_rotation() {
    let mut s = session();
    s.turn("object n-334c@77B8", 405.0).unwrap();
    clean(&s);
    let t = text(&s, FILE);
    assert!(t.contains("heading = 45.0"), "{t}");
    assert!(!t.contains("rotation"), "{t}");
}

#[test]
fn a_rows_field_is_set_as_toml_and_its_key_is_not() {
    let mut s = session();
    let item = s.place(&person([10.0, 20.0, 30.0])).unwrap();
    s.set(&item, "template", "0x47001234").unwrap();
    assert!(text(&s, FILE).contains("template = 0x47001234"));
    match &s.entry(&item).unwrap().row.row {
        Row::Object(o) => assert_eq!(o.template, 0x4700_1234),
        other => panic!("{other:?}"),
    }
    assert!(s.set(&item, "key", "\"other\"").is_err());
    assert!(s.set(&item, "template", "not toml [").is_err());
    // A field the format lacks, or a value that would leave the row unread, is refused and
    // nothing changes.
    let files = s.files().clone();
    let steps = s.steps();
    assert!(s.set(&item, "colour", "1").is_err());
    let why = s.set(&item, "template", "\"a name\"").unwrap_err();
    assert!(why.contains("template"), "{why}");
    assert_eq!(s.files(), &files);
    assert_eq!(s.steps(), steps);
    clean(&s);
    // A field taken away that the row may lack.
    s.set(&item, "heading", "").unwrap();
    assert!(!text(&s, FILE).contains("heading"));
    clean(&s);
}

#[test]
fn deleting_a_base_row_removes_it_and_putting_it_back_restores_it() {
    let mut s = session();
    let item = "object n-334c@77B8";
    s.delete(item).unwrap();
    clean(&s);
    assert!(s.entry(item).is_none());
    assert!(text(&s, FILE).contains("[[remove]]\nitems = [\"object n-334c@77B8\"]"));
    assert_eq!(s.removed()[0].0, item);
    s.put_back(item).unwrap();
    clean(&s);
    assert_eq!(s.entry(item).unwrap().layer, Layer::Base);
    assert!(!s.files().contains_key(FILE));
    assert!(s.removed().is_empty());
}

#[test]
fn putting_back_a_changed_base_row_drops_the_worlds_row() {
    let mut s = session();
    let item = "object n-334c@77B8";
    s.turn(item, 10.0).unwrap();
    assert_eq!(s.entry(item).unwrap().layer, Layer::Target);
    s.put_back(item).unwrap();
    clean(&s);
    assert_eq!(s.entry(item).unwrap().layer, Layer::Base);
    assert!(!s.files().contains_key(FILE));
}

#[test]
fn deleting_the_worlds_own_row_deletes_its_table() {
    let mut s = session();
    let a = s.place(&person([10.0, 20.0, 30.0])).unwrap();
    let b = s.place(&person([12.0, 20.0, 30.0])).unwrap();
    s.delete(&a).unwrap();
    clean(&s);
    assert!(s.entry(&a).is_none());
    assert!(s.entry(&b).is_some());
    let t = text(&s, FILE);
    assert!(!t.contains("person-1") && t.contains("person-2"), "{t}");
    assert!(!t.contains("remove"), "{t}");
}

#[test]
fn undo_and_redo_step_through_the_changes_as_text() {
    let mut s = session();
    let start = s.files().clone();
    let item = s.place(&person([10.0, 20.0, 30.0])).unwrap();
    let placed = s.files().clone();
    s.move_to(&item, LB, [11.0, 20.0, 30.0]).unwrap();
    let moved = s.files().clone();
    assert_eq!(s.steps(), (2, 0));
    assert!(s.undo());
    assert_eq!(s.files(), &placed);
    assert!(s.undo());
    assert_eq!(s.files(), &start);
    assert!(!s.undo());
    assert!(s.entry(&item).is_none());
    assert!(s.redo());
    assert!(s.redo());
    assert_eq!(s.files(), &moved);
    assert!(!s.redo());
    // A new change after an undo cannot be redone past.
    s.undo();
    s.turn(&item, 30.0).unwrap();
    assert_eq!(s.steps(), (2, 0));
}

#[test]
fn saving_writes_only_the_files_that_changed() {
    let files = world_files();
    let shared = Arc::new(store(&files));
    let mut s = Session::new(
        dir(),
        Box::new(Shared(Arc::clone(&shared))),
        files.clone(),
        base(),
        None,
    )
    .unwrap();
    let before = shared.snapshot();
    s.place(&person([10.0, 20.0, 30.0])).unwrap();
    assert!(s.unsaved());
    assert_eq!(
        shared.snapshot(),
        before,
        "nothing reaches the store before a save"
    );
    let written = s.save().unwrap();
    assert_eq!(written, [dir().join(FILE)]);
    assert!(!s.unsaved());
    let after = shared.snapshot();
    assert_eq!(after.len(), before.len() + 1);
    assert_eq!(
        String::from_utf8(after[&dir().join(FILE)].clone()).unwrap(),
        s.files()[FILE]
    );
    // Undone and saved, the file the session made is removed again.
    s.undo();
    assert_eq!(s.save().unwrap(), [dir().join(FILE)]);
    assert_eq!(shared.snapshot(), before);
}

#[test]
fn an_edit_keeps_the_comments_and_rows_around_it() {
    let mut files = world_files();
    files.insert(
        FILE.into(),
        "# The inn's people.\n\n[[object]]\nkey = \"barkeep\"\nkind = \"npc\"\ntemplate = 0x470033D0\nat = [1.0, 2.0, 3.0]\nprov = { from = \"authored\" } # by hand\n\n# The cellar.\n[[object]]\nkey = \"cook\"\nkind = \"npc\"\ntemplate = 0x470033D0\nat = [4.0, 5.0, 6.0]\nprov = { from = \"authored\" }\n".into(),
    );
    let mut s = Session::new(dir(), Box::new(store(&files)), files, base(), None).unwrap();
    clean(&s);
    s.move_to("object cook@77B8", LB, [7.0, 8.0, 9.0]).unwrap();
    let t = text(&s, FILE);
    assert!(
        t.starts_with("# The inn's people.\n\n[[object]]\nkey = \"barkeep\""),
        "{t}"
    );
    assert!(t.contains("# by hand"), "{t}");
    assert!(
        t.contains("# The cellar.\n[[object]]\nkey = \"cook\""),
        "{t}"
    );
    assert!(t.contains("at = [7.0, 8.0, 9.0]"), "{t}");
}

#[test]
fn a_row_of_another_world_pack_is_left_alone() {
    let mut files = world_files();
    files.insert(
        "world.toml".into(),
        files["world.toml"].replace(
            "packs = [\"fill-test\"]",
            "packs = [\"town\", \"fill-test\"]",
        ),
    );
    files.insert(
        "town/pack.toml".into(),
        "format_version = 1\nname = \"town\"\nversion = \"0.1.0\"\ngame = \"ac2\"\n".into(),
    );
    files.insert(
        "town/server/landblocks/77B8.toml".into(),
        "[[object]]\nkey = \"mayor\"\nkind = \"npc\"\ntemplate = 0x470033D0\nat = [1.0, 2.0, 3.0]\nprov = { from = \"authored\" }\n".into(),
    );
    let mut s = Session::new(dir(), Box::new(store(&files)), files, base(), None).unwrap();
    clean(&s);
    let e = s.entry("object mayor@77B8").unwrap();
    assert_eq!(e.layer, Layer::Other);
    assert!(s.move_to("object mayor@77B8", LB, [0.0, 0.0, 0.0]).is_err());
    assert!(s.delete("object mayor@77B8").is_err());
    // Naming the pack makes it the one edited.
    let files = s.files().clone();
    let mut s = Session::new(dir(), Box::new(store(&files)), files, base(), Some("town")).unwrap();
    s.move_to("object mayor@77B8", LB, [0.0, 0.0, 0.0]).unwrap();
    clean(&s);
}

#[test]
fn the_sessions_world_checks_over_its_base_as_the_server_reads_it() {
    let mut s = session();
    s.place(&person([10.0, 20.0, 30.0])).unwrap();
    s.move_to("object s-5c@77B8", LB, [100.0, 100.0, 150.0])
        .unwrap();
    s.delete("camp c-4586@77B8").unwrap();
    let base = base();
    assert!(layer::check(&base, s.files()).is_empty());
    let db = yalain_world_db::rows::read(s.files()).unwrap();
    let merged = layer::merge_over(&base, &db).unwrap();
    let mut items: Vec<String> = merged
        .rows
        .iter()
        .filter_map(yalain_world_db::merge::item_key)
        .collect();
    // The world's new row comes after the base's.
    assert_eq!(
        items.last().map(String::as_str),
        Some("object person-1@77B8")
    );
    items.sort_unstable();
    assert_eq!(
        items,
        [
            "object n-334c@77B8",
            "object person-1@77B8",
            "object s-5c@77B8",
            "place p-1"
        ]
    );
}

#[test]
fn a_moved_camp_keeps_how_its_creatures_move_and_its_route_in_the_base() {
    let mut files = crate::base_files();
    let camp = files.get_mut("seed/server/landblocks/77B8.toml").unwrap();
    *camp = camp.replace(
        "spawns = [{ template = 0x47002224, weight = 2 }]\n",
        "spawns = [{ template = 0x47002224, weight = 2 }]\nmovement = \"free-range\"\n\
         wanders = false\nwander_range = 12.5\nwander_chance = 0.25\nhomesick = 60.0\n\
         perception = 12.0\nuses_doors = false\nroute = \"r-1\"\nroute_start = 1\n\
         route_direction = \"backward\"\nname = { table = 0x2500284B, string = 0xE77B18D7 }\n",
    );
    files.insert(
        "seed/server/routes.toml".into(),
        "[[route]]\nkey = \"r-1\"\nwaypoints = [{ cell = 0x77B80040, at = [140.0, 140.0, 133.0], \
         rest = 25.0 }, { cell = 0x77B80040, at = [150.0, 140.0, 133.0] }]\nshape = \"loop\"\n\
         gait = \"run\"\nspeed = 3.65\nrest = 20.0\nprov = { from = \"authored\" }\n"
            .into(),
    );
    let base = layer::Base::new(&files).expect("the base reads");
    let world = world_files();
    let mut s = Session::new(dir(), Box::new(store(&world)), world, base, None).unwrap();
    clean(&s);
    s.move_to("camp c-4586@77B8", LB, [145.0, 145.0, 133.0])
        .unwrap();
    clean(&s);
    let t = text(&s, FILE);
    for field in [
        "movement = \"free-range\"",
        "wanders = false",
        "wander_range = 12.5",
        "wander_chance = 0.25",
        "homesick = 60.0",
        "perception = 12.0",
        "uses_doors = false",
        "route = \"seed:r-1\"",
        "route_start = 1",
        "route_direction = \"backward\"",
    ] {
        assert!(t.contains(field), "{field} in {t}");
    }
    let Row::Camp(c) = &s.entry("camp c-4586@77B8").unwrap().row.row else {
        panic!("a camp");
    };
    assert_eq!(c.route.as_deref(), Some("seed:r-1"));
    let name = yalain_world_db::rows::StrRef {
        table: 0x2500_284B,
        string: 0xE77B_18D7,
    };
    assert_eq!(c.name, Some(name), "its name in {t}");

    // A route row, written as the database writes it, reads back the same.
    let db = yalain_world_db::rows::read(&files).unwrap();
    let route = db
        .rows()
        .find_map(|l| match &l.row {
            Row::Route(r) => Some(r.clone()),
            _ => None,
        })
        .unwrap();
    let table = athenaeum_foundry::toml::route(&route, &route.prov);
    let mut doc = toml_edit::DocumentMut::new();
    let mut a = toml_edit::ArrayOfTables::new();
    a.push(table);
    doc.insert("route", toml_edit::Item::ArrayOfTables(a));
    files.insert("seed/server/routes.toml".into(), doc.to_string());
    let again = yalain_world_db::rows::read(&files).unwrap();
    let back = again
        .rows()
        .find_map(|l| match &l.row {
            Row::Route(r) => Some(r.clone()),
            _ => None,
        })
        .unwrap();
    assert_eq!(back, route);
}

#[test]
fn a_clearing_is_placed_moved_reshaped_copied_and_taken_away_as_a_row() {
    let mut s = session();
    let round = ClearingShape::Round { radius: 12.0 };
    let item = s.place_clearing(LB, [10.0, 20.0], round).unwrap();
    assert_eq!(item, "clearing clearing-1@77B8");
    clean(&s);
    assert!(
        text(&s, FILE).contains("[[clearing]]"),
        "{}",
        text(&s, FILE)
    );
    let shape = |s: &Session, item: &str| match &s.entry(item).expect("the row").row.row {
        Row::Clearing(c) => (c.radius, c.size, c.heading, c.at.x, c.at.z),
        r => panic!("{r:?}"),
    };
    assert_eq!(shape(&s, &item), (Some(12.0), None, None, 10.0, None));
    assert_eq!(s.clearings().len(), 1);

    // Field by field, and refused when it would leave no one shape.
    s.set(&item, "radius", "30.0").unwrap();
    assert_eq!(shape(&s, &item).0, Some(30.0));
    assert!(s.set(&item, "radius", "").is_err());
    assert!(s.set(&item, "size", "[1.0, 2.0]").is_err());
    clean(&s);

    // A height it is moved to means nothing: it lies on the ground.
    s.move_to(&item, LB, [40.0, 50.0, 99.0]).unwrap();
    assert_eq!(shape(&s, &item).3, 40.0);
    assert_eq!(shape(&s, &item).4, None);

    // Another shape is one step.
    let steps = s.steps().0;
    let rectangle = ClearingShape::Rectangle {
        size: [8.0, 4.0],
        heading: 30.0,
    };
    s.shape_clearing(&item, rectangle).unwrap();
    assert_eq!(s.steps().0, steps + 1);
    assert_eq!(
        shape(&s, &item),
        (None, Some([8.0, 4.0]), Some(30.0), 40.0, None)
    );
    s.turn(&item, 45.0).unwrap();
    assert_eq!(shape(&s, &item).2, Some(45.0));
    clean(&s);
    s.undo();
    s.undo();
    assert_eq!(shape(&s, &item).0, Some(30.0));

    // Into another block, its key and item follow; a copy has a key of its own.
    let there = s.move_to(&item, 0x77B9, [1.0, 2.0, 0.0]).unwrap();
    assert_eq!(there, "clearing clearing-1@77B9");
    let copy = s.duplicate(&there, 0x77B9, [5.0, 5.0, 0.0]).unwrap();
    assert_eq!(copy, "clearing clearing-2@77B9");
    clean(&s);
    assert_eq!(s.clearings().len(), 2);
    s.delete(&copy).unwrap();
    s.delete(&there).unwrap();
    assert!(s.clearings().is_empty());
    clean(&s);
}

#[test]
fn placing_the_kind_clearing_with_nothing_drawn_gives_the_usual_size() {
    let mut s = session();
    let item = s
        .place(&NewRow {
            kind: NewKind::Clearing,
            template: 0,
            landblock: LB,
            at: [10.0, 20.0, 5.0],
            heading: 0.0,
        })
        .unwrap();
    let Row::Clearing(c) = &s.entry(&item).unwrap().row.row else {
        panic!("a clearing");
    };
    assert_eq!(c.radius, Some(NEW_CLEARING));
    assert!(!NewKind::Clearing.has_template());
}

/// A world's kinds of pet can be read and changed: a base row is written into the target, which
/// replaces it; a field the format lacks, a value that leaves the row unread, and the field that
/// names it are refused.
#[test]
fn a_pet_row_of_the_base_is_seen_and_changed_in_the_target() {
    let mut files = crate::base_files();
    files.insert(
        "seed/server/pets.toml".into(),
        r#"[[pet]]
effect = 0x7000092A
template = 0x47000862
flags = 1096
health = [{ level = 10, health = 200 }]
armor = [{ level = 10, armor = 20 }, { level = 50, armor = 240 }]
prov = { from = "authored" }

[[pet_limit]]
skill = 704
limits = [{ most = 1, classes = 0x1000 }]
prov = { from = "authored" }
"#
        .into(),
    );
    let base = Base::new(&files).expect("the base reads");
    let world = crate::world_files();
    let mut s =
        Session::new(crate::dir(), Box::new(store(&world)), world, base, None).expect("a session");
    assert_eq!(s.pets().len(), 2);
    let item = "pet 0x7000092a";
    s.set(item, "flags", "3128").unwrap();
    let written = s
        .pets()
        .iter()
        .find_map(|l| match &l.row {
            Row::Pet(r) => Some(r.clone()),
            _ => None,
        })
        .expect("the row");
    assert_eq!(written.flags, 3128);
    assert_eq!(written.health, vec![(10, 200)], "the rest kept");
    assert_eq!(written.armor, vec![(10, 20), (50, 240)], "the rest kept");
    s.set(
        "pet_limit 0x000002c0",
        "limits",
        "[{ most = 2, classes = 0x1000 }]",
    )
    .unwrap();
    let files = s.files().clone();
    assert!(s.set(item, "colour", "1").is_err());
    assert!(s.set(item, "effect", "1").is_err());
    assert!(s.set(item, "template", "\"a name\"").is_err());
    assert_eq!(s.files(), &files, "nothing changed");
    assert!(s.problems().is_empty(), "{:?}", s.problems());
}

/// The camp's spots as its row reads: each one's cell and point.
fn spots(s: &Session, item: &str) -> Vec<(u32, [f32; 2])> {
    match &s.entry(item).unwrap().row.row {
        Row::Camp(c) => c
            .spots
            .iter()
            .map(|(cell, at)| (*cell, [at.x, at.y]))
            .collect(),
        other => panic!("{other:?}"),
    }
}

#[test]
fn a_camp_moved_takes_its_spots_along() {
    let mut s = session();
    let camp = "camp c-4586@77B8";
    s.set(
        camp,
        "spots",
        "[{ cell = 0x77B80001, at = [140.0, 140.0, 133.0] }, { cell = 0x77B80105, at = [20.0, 30.0, 10.0] }]",
    )
    .unwrap();
    s.set(camp, "cell", "0x77B80105").unwrap();
    clean(&s);
    // On land, ten metres east: each spot goes as far, and a land spot's cell follows its point.
    let moved = s.move_to(camp, LB, [154.8, 144.7, 133.0]).unwrap();
    let land = yalain_world_db::compile::outdoor_cell(LB, 150.0, 140.0);
    assert_ne!(land, 0x77B8_0001);
    assert_eq!(
        spots(&s, &moved),
        [(land, [150.0, 140.0]), (0x77B8_0105, [30.0, 30.0])]
    );
    // Into another room: a spot in the camp's old room goes into its new one.
    let moved = s
        .move_into(&moved, LB, [154.8, 144.7, 133.0], Some(0x77B8_0106))
        .unwrap();
    assert_eq!(spots(&s, &moved)[1].0, 0x77B8_0106);
    clean(&s);
}

#[test]
fn a_place_renamed_takes_the_quest_steps_that_lead_to_it_along() {
    let mut files = world_files();
    files.insert(
        "fill-test/server/places.toml".into(),
        "[[place]]\nkey = \"inn\"\ncell = 0x77B80003\nat = [1.0, 2.0, 3.0]\nheading = 0.0\nprov = { from = \"authored\" }\n".into(),
    );
    files.insert(
        "fill-test/server/quests/40000088.toml".into(),
        "[[quest]]\nid = 0x40000088\nid_name = \"QuestTest\"\nprov = { from = \"authored\" }\n\n\
         [[step]]\nkey = \"s1\"\nquest = 0x40000088\nneeds = 0\ntrigger = { use = 0x470020BB, on = 0x4700115B }\n\
         update = \"bestow\"\nto = \"inn\"\nprov = { from = \"authored\" }\n"
            .into(),
    );
    let mut s = Session::new(dir(), Box::new(store(&files)), files, base(), None).unwrap();
    clean(&s);
    let quest = "fill-test/server/quests/40000088.toml";
    s.rename_place("place inn", "the-inn").unwrap();
    clean(&s);
    assert!(
        text(&s, quest).contains("to = \"the-inn\""),
        "{}",
        text(&s, quest)
    );
    s.undo();
    assert!(
        text(&s, quest).contains("to = \"inn\""),
        "one undo takes it back"
    );
}

#[test]
fn a_camp_moved_into_another_landblock_takes_what_names_it_along() {
    let camp = |cell: &str| {
        format!(
            "[[camp]]\nkey = \"beetles\"\ncell = {cell}\nat = [10.0, 10.0, 133.0]\nradius = 5.0\npopulation = 2\n\
             regen = 192.0\nspawns = [{{ template = 0x47002224, weight = 1 }}]\nprov = {{ from = \"authored\" }}\n"
        )
    };
    let mut files = world_files();
    files.insert(
        "fill-test/server/landblocks/77B8.toml".into(),
        camp("0x77B80001"),
    );
    files.insert(
        "fill-test/server/landblocks/77B9.toml".into(),
        camp("0x77B90001"),
    );
    let quest = "fill-test/server/quests/40000088.toml";
    files.insert(
        quest.into(),
        "[[quest]]\nid = 0x40000088\nid_name = \"QuestTest\"\nprov = { from = \"authored\" }\n\n\
         [[step]]\nkey = \"s1\"\nquest = 0x40000088\nneeds = 0\ntrigger = { kill = 0x47002224, camp = \"beetles@77B8\" }\n\
         update = \"bestow\"\nprov = { from = \"authored\" }\n\n\
         [[death_treasure]]\nkey = \"shell\"\ncamp = \"beetles@77B8\"\nitem = 0x470020B9\nchance = 0.5\n\
         prov = { from = \"authored\" }\n"
            .into(),
    );
    let mut s = Session::new(dir(), Box::new(store(&files)), files, base(), None).unwrap();
    clean(&s);
    let moved = s
        .move_to("camp beetles@77B8", 0x77B9, [30.0, 30.0, 133.0])
        .unwrap();
    clean(&s);
    let (key, lb) = moved.trim_start_matches("camp ").split_once('@').unwrap();
    assert_ne!(key, "beetles", "a key of its own there: {moved}");
    assert_eq!(lb, "77B9");
    let named = format!("camp = \"{key}@77B9\"");
    assert_eq!(
        text(&s, quest).matches(&named).count(),
        2,
        "{}",
        text(&s, quest)
    );
    s.undo();
    assert_eq!(
        text(&s, quest).matches("camp = \"beetles@77B8\"").count(),
        2
    );
}

/// The world's files as the store has them now, by path in the world's folder: what a look at the
/// disk reads.
fn on_disk(disk: &MemoryStore) -> yalain_world_db::Files {
    disk.snapshot()
        .into_iter()
        .filter_map(|(p, b)| {
            let rel = p
                .strip_prefix(dir())
                .ok()?
                .to_string_lossy()
                .replace('\\', "/");
            Some((rel, String::from_utf8(b).ok()?))
        })
        .collect()
}

fn a_place(key: &str) -> String {
    format!(
        "[[place]]\nkey = \"{key}\"\ncell = 0x77B80003\nat = [1.0, 2.0, 3.0]\nheading = 0.0\nprov = {{ from = \"authored\" }}\n"
    )
}

const PLACES: &str = "fill-test/server/places.toml";

/// A session over a new world with a place of its own, `inn`, and the store it reads and saves.
fn with_a_place() -> (Session, Arc<MemoryStore>) {
    let mut files = world_files();
    files.insert(PLACES.into(), a_place("inn"));
    let disk = Arc::new(store(&files));
    let s = Session::new(
        dir(),
        Box::new(Shared(Arc::clone(&disk))),
        files,
        base(),
        None,
    )
    .unwrap();
    (s, disk)
}

#[test]
fn a_file_changed_on_disk_is_taken_in_and_the_world_read_again() {
    let (mut s, disk) = with_a_place();
    assert!(
        s.take_disk_changes(&on_disk(&disk)).is_empty(),
        "nothing changed yet"
    );
    disk.write(&dir().join(PLACES), a_place("tavern").as_bytes())
        .unwrap();
    let revision = s.revision();
    let found = s.take_disk_changes(&on_disk(&disk));
    assert_eq!(found.taken, [PLACES]);
    assert!(found.clashing.is_empty());
    assert!(s.revision() > revision);
    assert!(s.entry("place tavern").is_some() && s.entry("place inn").is_none());
    assert!(
        !s.unsaved(),
        "what the disk has is no change of the session's"
    );
}

#[test]
fn a_file_added_or_taken_away_on_disk_is_taken_in() {
    let (mut s, disk) = with_a_place();
    let quest = "fill-test/server/quests/40000088.toml";
    disk.write(
        &dir().join(quest),
        b"[[quest]]\nid = 0x40000088\nid_name = \"QuestTest\"\nprov = { from = \"authored\" }\n",
    )
    .unwrap();
    assert_eq!(s.take_disk_changes(&on_disk(&disk)).taken, [quest]);
    assert!(s.files().contains_key(quest), "a new file is the world's");
    disk.remove(&dir().join(PLACES)).unwrap();
    assert_eq!(s.take_disk_changes(&on_disk(&disk)).taken, [PLACES]);
    assert!(!s.files().contains_key(PLACES));
    assert!(s.entry("place inn").is_none());
    assert!(!s.unsaved());
    clean(&s);
}

#[test]
fn a_file_changed_on_disk_and_in_the_session_clashes_and_neither_is_written_over() {
    let (mut s, disk) = with_a_place();
    // A change here, and another to the same file by hand, not yet saved here.
    s.turn("place inn", 90.0).unwrap();
    s.place(&person([10.0, 20.0, 30.0])).unwrap();
    disk.write(&dir().join(PLACES), a_place("tavern").as_bytes())
        .unwrap();
    let found = s.take_disk_changes(&on_disk(&disk));
    assert_eq!(found.clashing, [PLACES]);
    assert!(found.taken.is_empty());
    assert!(s.clashes().contains(PLACES));
    assert!(s.entry("place inn").is_some(), "the session's text is kept");
    assert!(s.take_disk_changes(&on_disk(&disk)).is_empty(), "said once");
    // Saving writes the rest, and not the file that clashes.
    let said = s.save().unwrap_err();
    assert!(said.contains(PLACES), "{said}");
    assert_eq!(
        disk.read_to_string(&dir().join(PLACES)).unwrap(),
        a_place("tavern"),
        "the hand's text stays"
    );
    assert!(disk.is_file(&dir().join(FILE)), "the other file is saved");
    assert!(s.unsaved());
    // Kept: the next save writes it.
    s.keep_mine(PLACES).unwrap();
    assert!(s.clashes().is_empty());
    s.save().unwrap();
    assert!(disk
        .read_to_string(&dir().join(PLACES))
        .unwrap()
        .contains("key = \"inn\""));
    assert!(!s.unsaved());
}

#[test]
fn a_clash_settled_for_the_disk_takes_its_text_and_lets_undo_go() {
    let (mut s, disk) = with_a_place();
    s.turn("place inn", 90.0).unwrap();
    disk.write(&dir().join(PLACES), a_place("tavern").as_bytes())
        .unwrap();
    s.take_disk_changes(&on_disk(&disk));
    s.take_disk(PLACES).unwrap();
    assert!(s.clashes().is_empty());
    assert!(s.entry("place tavern").is_some());
    assert!(!s.unsaved());
    assert_eq!(s.steps(), (0, 0), "undo would put the session's text back");
}

#[test]
fn a_file_changed_on_disk_after_the_last_look_is_not_saved_over() {
    let (mut s, disk) = with_a_place();
    s.turn("place inn", 90.0).unwrap();
    disk.write(&dir().join(PLACES), a_place("tavern").as_bytes())
        .unwrap();
    // No look at the disk since: the save finds it.
    assert!(s.save().is_err());
    assert_eq!(
        disk.read_to_string(&dir().join(PLACES)).unwrap(),
        a_place("tavern")
    );
    assert!(s.clashes().contains(PLACES));
}

#[test]
fn undo_lets_go_once_a_file_it_would_put_back_is_taken_in_from_disk() {
    let (mut s, disk) = with_a_place();
    s.turn("place inn", 90.0).unwrap();
    s.place(&person([10.0, 20.0, 30.0])).unwrap();
    s.save().unwrap();
    assert_eq!(s.steps(), (2, 0));
    // A file no step changed leaves the history as it is.
    let quest = "fill-test/server/quests/40000088.toml";
    disk.write(
        &dir().join(quest),
        b"[[quest]]\nid = 0x40000088\nid_name = \"QuestTest\"\nprov = { from = \"authored\" }\n",
    )
    .unwrap();
    assert!(!s.take_disk_changes(&on_disk(&disk)).history_dropped);
    assert_eq!(s.steps(), (2, 0));
    // One a step changed lets it go: an undo would put the hand's change back as it was.
    disk.write(&dir().join(PLACES), a_place("tavern").as_bytes())
        .unwrap();
    let found = s.take_disk_changes(&on_disk(&disk));
    assert!(found.history_dropped);
    assert_eq!(s.steps(), (0, 0));
    assert!(s.entry("place tavern").is_some());
}

/// A recorded pet row changed keeps where it came from, its evidence and how sure; only the field
/// changed is marked a person's.
#[test]
fn a_recorded_pet_row_changed_keeps_its_provenance_and_marks_the_field() {
    let mut files = crate::base_files();
    files.insert(
        "seed/server/pets.toml".into(),
        r#"[[pet]]
effect = 0x7000092A
template = 0x47000862
flags = 1096
health = [{ level = 10, health = 200 }]
prov = { from = "capture", evidence = "TEST-PETS", ref = "pet 1", confidence = "high", sample = { sessions = 3 } }
"#
        .into(),
    );
    let base = Base::new(&files).expect("the base reads");
    let world = crate::world_files();
    let mut s =
        Session::new(crate::dir(), Box::new(store(&world)), world, base, None).expect("a session");
    s.set("pet 0x7000092a", "flags", "3128").unwrap();
    clean(&s);
    let prov = s
        .pets()
        .iter()
        .find_map(|l| match &l.row {
            Row::Pet(r) if r.flags == 3128 => Some(r.prov.clone()),
            _ => None,
        })
        .expect("the row, changed");
    assert_eq!(
        (
            prov.source,
            prov.evidence.as_deref(),
            prov.reference.as_deref()
        ),
        (Source::Capture, Some("TEST-PETS"), Some("pet 1"))
    );
    assert!(prov.confidence.is_some());
    assert_eq!(prov.sample.len(), 1, "{prov:?}");
    let marked: Vec<(&str, Source)> = prov
        .fields
        .iter()
        .map(|(k, p)| (k.as_str(), p.source))
        .collect();
    assert_eq!(marked, [("flags", Source::Authored)]);
}

#[test]
fn a_problem_on_a_line_of_a_placed_rows_table_is_that_rows_and_others_are_no_rows() {
    let mut files = world_files();
    files.insert(
        FILE.into(),
        "[[camp]]\nkey = \"beetles\"\nat = [10.0, 10.0, 133.0]\nradius = 5.0\nparent = \"nowhere\"\npopulation = 2\n\
         regen = 192.0\nspawns = [{ template = 0x47002224, weight = 1 }]\nprov = { from = \"authored\" }\n\n\
         [[remove]]\nitems = [\"object s-5c@77B8\"]\nprov = { from = \"authored\" }\n"
            .into(),
    );
    let quest = "fill-test/server/quests/40000088.toml";
    files.insert(
        quest.into(),
        "[[quest]]\nid = 0x40000088\nid_name = \"QuestTest\"\nprov = { from = \"authored\" }\n"
            .into(),
    );
    let s = Session::new(dir(), Box::new(store(&files)), files, base(), None).unwrap();
    // The world's check finds the camp's parent missing, and any line of its table is the camp's.
    let p = s
        .problems()
        .iter()
        .find(|p| p.file == FILE)
        .expect("a problem with the camp");
    let camp = s.row_at(&p.file, p.line).map(|e| e.item.as_str());
    assert_eq!(camp, Some("camp beetles@77B8"), "{p}");
    assert_eq!(
        s.row_at(FILE, 4).map(|e| e.item.as_str()),
        Some("camp beetles@77B8")
    );
    // The removal's lines, a quest's and no line are no placed row's.
    assert!(s.row_at(FILE, 11).is_none());
    assert!(s.row_at(quest, 2).is_none());
    assert!(s.row_at(FILE, 0).is_none());
    // A base file's lines go by the base's rows.
    let seed = "seed/server/landblocks/77B8.toml";
    assert_eq!(
        s.row_at(seed, 26).map(|e| e.item.as_str()),
        Some("camp c-4586@77B8")
    );
    assert_eq!(
        s.row_at(seed, 9).map(|e| e.item.as_str()),
        Some("object n-334c@77B8")
    );
    assert!(s.row_at(seed, 2).is_none());
}

#[test]
fn a_camp_placed_in_a_room_has_its_first_spot_where_it_was_put_and_outdoors_none() {
    let mut s = session();
    let camp = |at| NewRow {
        kind: NewKind::Camp,
        ..person(at)
    };
    let room = 0x77B8_0105;
    let inside = s.place_in(&camp([20.0, 30.0, 10.0]), Some(room)).unwrap();
    assert_eq!(spots(&s, &inside), [(room, [20.0, 30.0])]);
    let outdoors = s.place(&camp([40.0, 30.0, 10.0])).unwrap();
    assert!(spots(&s, &outdoors).is_empty());
    clean(&s);
}

#[test]
fn a_camps_spots_are_set_added_moved_and_taken_away() {
    let mut s = session();
    let camp = "camp c-4586@77B8";
    let at = |x, y| yalain_world_db::rows::At {
        x,
        y,
        z: Some(10.0),
    };
    let room = 0x77B8_0105;
    s.set_spots(camp, &[(room, at(20.0, 30.0)), (room, at(22.0, 31.5))])
        .unwrap();
    clean(&s);
    assert_eq!(
        spots(&s, camp),
        [(room, [20.0, 30.0]), (room, [22.0, 31.5])]
    );
    // A base camp: the world's own row of its key, its provenance kept and the spots a person's.
    let e = s.entry(camp).unwrap();
    assert_eq!(e.layer, Layer::Target);
    let prov = e.prov().unwrap();
    assert_eq!(prov.source, Source::Capture);
    assert_eq!(
        prov.fields.get("spots").map(|p| p.source),
        Some(Source::Authored)
    );
    s.set_spots(camp, &[(room, at(25.0, 30.0))]).unwrap();
    assert_eq!(spots(&s, camp), [(room, [25.0, 30.0])]);
    s.set_spots(camp, &[]).unwrap();
    assert!(spots(&s, camp).is_empty());
    assert!(!text(&s, FILE).contains("spots = ["), "{}", text(&s, FILE));
    assert!(
        s.set_spots("object n-334c@77B8", &[]).is_err(),
        "a camp's alone"
    );
    clean(&s);
}

/// A session over a new world whose base has a kind of pet and a skill's limit.
fn with_pets() -> Session {
    let mut files = crate::base_files();
    files.insert(
        "seed/server/pets.toml".into(),
        r#"[[pet]]
effect = 0x7000092A
template = 0x47000862
flags = 1096
prov = { from = "capture", evidence = "TEST-PETS", ref = "pet 1", confidence = "high" }

[[pet_limit]]
skill = 704
limits = [{ most = 1, classes = 0x1000 }]
prov = { from = "authored" }
"#
        .into(),
    );
    let base = Base::new(&files).expect("the base reads");
    let world = crate::world_files();
    Session::new(crate::dir(), Box::new(store(&world)), world, base, None).expect("a session")
}

const PETS: &str = "fill-test/server/pets.toml";

fn pet_items(s: &Session) -> Vec<String> {
    s.pets()
        .iter()
        .filter_map(yalain_world_db::merge::item_key)
        .collect()
}

/// A kind of pet and a skill's limit are added as a person's rows of the world, changed, and
/// taken away again, leaving nothing behind; one the world has already is refused.
#[test]
fn a_kind_of_pet_is_added_changed_and_taken_away() {
    let mut s = with_pets();
    let pet = s.add_pet(0x7000_1234, 0x4700_0863).unwrap();
    assert_eq!(pet, "pet 0x70001234");
    let limit = s.add_pet_limit(705, 2, 0x2000).unwrap();
    assert_eq!(limit, "pet_limit 0x000002c1");
    assert!(s.problems().is_empty(), "{:?}", s.problems());
    let added = s
        .pets()
        .iter()
        .find(|l| yalain_world_db::merge::item_key(l).as_deref() == Some(pet.as_str()))
        .unwrap()
        .clone();
    assert_eq!(s.pet_layer(&added), Layer::Target);
    assert_eq!(added.file, PETS);
    match &added.row {
        Row::Pet(r) => {
            assert_eq!((r.template, r.count), (0x4700_0863, 1));
            assert_eq!(r.prov.source, Source::Authored);
        }
        other => panic!("{other:?}"),
    }
    s.set(&pet, "count", "2").unwrap();
    assert!(
        s.add_pet(0x7000_1234, 0x4700_0001).is_err(),
        "one of its effect"
    );
    assert!(s.add_pet(0x7000_092A, 0x4700_0001).is_err(), "the base's");
    assert!(s.add_pet_limit(704, 1, 1).is_err(), "the base's");
    s.remove_pet(&pet).unwrap();
    s.remove_pet(&limit).unwrap();
    assert!(!pet_items(&s).contains(&pet));
    assert!(!s.files().contains_key(PETS), "nothing is left in the file");
    assert!(s.removed().is_empty(), "nothing of the base is taken away");
    assert!(s.problems().is_empty(), "{:?}", s.problems());
}

/// A kind of pet of the base, changed or not, taken away is removed by its item; put back, it is
/// the base's again and the world writes nothing of it.
#[test]
fn a_base_pet_taken_away_is_removed_by_item_and_put_back() {
    let mut s = with_pets();
    let item = "pet 0x7000092a";
    let row = s
        .pets()
        .iter()
        .find(|l| matches!(l.row, Row::Pet(_)))
        .unwrap()
        .clone();
    assert_eq!(s.pet_layer(&row), Layer::Base);
    for changed in [false, true] {
        if changed {
            s.set(item, "flags", "3128").unwrap();
            assert!(s.pet_was(item).is_some());
        }
        s.remove_pet(item).unwrap();
        assert!(!pet_items(&s).contains(&item.to_string()), "taken away");
        assert!(text(&s, PETS).contains("[[remove]]"), "{}", text(&s, PETS));
        assert!(!text(&s, PETS).contains("[[pet]]"), "{}", text(&s, PETS));
        assert!(s.removed().iter().any(|(i, _)| i == item));
        s.put_back(item).unwrap();
        assert!(pet_items(&s).contains(&item.to_string()), "back");
        assert!(!s.files().contains_key(PETS), "the base's again");
        let back = s
            .pets()
            .iter()
            .find_map(|l| match &l.row {
                Row::Pet(r) => Some(r.flags),
                _ => None,
            })
            .unwrap();
        assert_eq!(back, 1096, "as the base has it");
    }
    assert!(s.problems().is_empty(), "{:?}", s.problems());
}
