//! Behaviour: none (tooling: a world repository's rows edited in the world editor's session)

use lyceum_foundry::edit::{Op, Session};

use crate::common::Fixture;

/// A row file of the layer being edited is written as a step of the history with the world's
/// own: held unsaved until the save, taken back and done again; one outside the layer's pack, or
/// with no repository being edited, is refused.
#[test]
fn a_repositorys_row_file_is_written_undone_and_saved_with_the_worlds_edits() {
    let f = Fixture::new("rows");
    let world = f.world();
    let below = f.write(
        "repo/base/library/weenie/00509.toml",
        "[[weenie]]\nclass = 509\n",
    );
    let repo = below.ancestors().nth(4).unwrap().to_path_buf();
    let pack = repo.join("patches/2027-01-first/packs/main");
    let file = pack.join("weenie/00509.toml");
    let mut s = Session::open(&world, &f.dats(), None).unwrap();
    let write = |text: Option<&str>| Op::WriteRows {
        file: file.clone(),
        text: text.map(str::to_owned),
    };
    assert!(
        s.apply(&write(Some("[[weenie]]\nclass = 509\n"))).is_err(),
        "no repository"
    );

    s.set_rows(Some((repo.clone(), pack.clone())));
    let elsewhere = Op::WriteRows {
        file: below.clone(),
        text: Some(String::new()),
    };
    assert!(s.apply(&elsewhere).is_err(), "another layer's pack");
    let row = "[[weenie]]\nclass = 509\n\n[weenie.int]\nLevel = 6\n";
    let applied = s.apply(&write(Some(row))).unwrap();
    assert!(applied.touched.rows && applied.touched.landblocks.is_empty());
    assert_eq!(s.rows_text(&file).as_deref(), Some(row));
    assert!(s.rows_unsaved() && !file.exists(), "unsaved until the save");

    s.undo().unwrap();
    assert_eq!(s.rows_text(&file), None);
    s.redo().unwrap();
    assert_eq!(s.rows_text(&file).as_deref(), Some(row));
    s.save().unwrap();
    assert_eq!(std::fs::read_to_string(&file).unwrap(), row);
    assert!(!s.rows_unsaved());
}
