//! Behaviour: none (checks formats, geometry, numeric contracts or repository tools)
//! A set of data files by role: the names each known set gives its four roles, the file that
//! holds a role a set has none for, and a set of a game's own names in a layout of its own.
//! Fixture: empty files in a folder of the test's own under the system's temporary folder.

use std::path::PathBuf;

use dereth_dat::{ContainerLayout, FileSet, ModernDat};

/// A folder of the test's own, removed when dropped.
struct Scratch(PathBuf);

impl Scratch {
    fn new(tag: &str) -> Self {
        let dir = std::env::temp_dir().join(format!("dereth-dat-{tag}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        Self(dir)
    }

    fn path(&self) -> &std::path::Path {
        &self.0
    }
}

impl Drop for Scratch {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.0);
    }
}

#[test]
fn the_known_sets_name_each_role_and_the_older_one_keeps_language_and_high_res_in_its_portal_file()
{
    let m = FileSet::MODERN;
    for t in ModernDat::ALL {
        assert_eq!(m.name(t), Some(t.file_name()));
        assert_eq!(m.holder_name(t), t.file_name());
    }
    assert_eq!(m.roles().count(), 4);
    assert_eq!(m.layout(), &ContainerLayout::MODERN);
    let c = FileSet::CLASSIC;
    assert_eq!(c.name(ModernDat::Cell), Some("cell.dat"));
    assert_eq!(c.name(ModernDat::Local), None);
    assert_eq!(c.holder_name(ModernDat::Local), "portal.dat");
    assert_eq!(c.holder_name(ModernDat::HighRes), "portal.dat");
    assert_eq!(
        c.roles().collect::<Vec<_>>(),
        [ModernDat::Portal, ModernDat::Cell]
    );
    assert_eq!(c.role_of("CELL.DAT"), Some(ModernDat::Cell));
    assert_eq!(c.role_of("client_cell_1.dat"), None);
}

#[test]
fn a_set_of_its_own_names_holds_a_folder_with_its_portal_and_cell_files() {
    const OWN: FileSet = FileSet::new(
        [
            Some("world.dat"),
            Some("land_1.dat"),
            Some("text.dat"),
            None,
        ],
        ContainerLayout::MODERN,
    );
    let dir = Scratch::new("file-set");
    assert!(!OWN.holds(dir.path()));
    std::fs::write(OWN.in_dir(ModernDat::Portal, dir.path()).unwrap(), b"").unwrap();
    assert!(!OWN.holds(dir.path()), "the cell file is wanted too");
    std::fs::write(OWN.in_dir(ModernDat::Cell, dir.path()).unwrap(), b"").unwrap();
    assert!(OWN.holds(dir.path()));
    assert!(!FileSet::MODERN.holds(dir.path()));
    assert_eq!(OWN.in_dir(ModernDat::HighRes, dir.path()), None);
    assert_eq!(
        OWN.holder_in_dir(ModernDat::HighRes, dir.path()),
        dir.path().join("world.dat")
    );
    assert_eq!(OWN.role_of("land_1.dat"), Some(ModernDat::Cell));
}
