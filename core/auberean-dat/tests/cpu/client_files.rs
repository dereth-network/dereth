//! The client's data files opened as the client opens them: the portal and the high-resolution
//! pictures found and opened, a region's cell file made when missing, a language's file required.

use std::collections::BTreeMap;
use std::sync::Arc;

use auberean_dat::disk::client_files::{NEW_CELL_FILE, NO_LANGUAGE_NOTICE};
use auberean_dat::disk::controller::OPEN_CREATE;
use auberean_dat::disk::{ClientFiles, Files, LookHost, OpenMode, Opened, SharedStore, Slot};
use dereth_dat::{DatStorageMut, MemoryFile};

use crate::disk::support::spec;

/// A folder of files in memory, found under `D:\data` (the project variable's folder). The files
/// are made up: a portal and pictures under names of their own, a language called `Testish`.
#[derive(Default)]
struct Folder(BTreeMap<String, MemoryFile>);

impl Files for Folder {
    fn open(&mut self, name: &str, mode: OpenMode) -> Option<Box<dyn DatStorageMut>> {
        if mode == OpenMode::Create {
            self.0.insert(name.into(), MemoryFile::default());
        }
        self.0
            .get(name)
            .map(|f| Box::new(f.clone()) as Box<dyn DatStorageMut>)
    }
}

impl LookHost for Folder {
    fn exists(&self, name: &str) -> bool {
        self.0.contains_key(name)
    }
    fn working_folder(&self) -> Option<String> {
        None
    }
    fn variable(&self, name: &str) -> Option<String> {
        (name == "PROJECT_DAT").then(|| "D:\\data".into())
    }
    fn registry(&self, _: &str, _: &str) -> Option<String> {
        None
    }
}

/// A new file named `name` in the folder: data set and subset, game version `game`.
fn make(folder: &mut Folder, name: &str, set: u32, subset: u32, game: i32) {
    let s = SharedStore::new(Box::new(|| 0));
    let mut sp = spec(OPEN_CREATE, 0x400 + 64 * 0x100, 0x100);
    sp.file = name.into();
    sp.path = "D:\\data/".into();
    (sp.data_set, sp.data_subset, sp.game_pack_vnum) = (set, subset, game);
    assert_eq!(s.init_file(folder, &mut sp), Opened::Ready);
    s.lock().close();
}

fn client() -> ClientFiles {
    ClientFiles::new(false, false, Arc::new(|| 0x5000_0000))
}

/// Behaviour: data.client-files.opened-as-the-client-opens-them
#[test]
fn the_portal_and_its_pictures_open_from_where_they_are_found() {
    let mut folder = Folder::default();
    make(&mut folder, "world.dat", 1, 0, 3);
    make(&mut folder, "pictures.dat", 1, 0x6946_6948, 3);
    let mut c = client();
    let look = Folder(folder.0.clone());
    assert!(c.open_portal(&mut folder, &look, "world.dat", "pictures.dat"));
    assert!(c.set.by_slot[Slot::Portal as usize].is_some());
    assert!(c.set.is_dat_file_loaded(0x1_0000_0000));
    assert!(c.set.is_dat_file_loaded(0x1_6946_6948));
    // A portal of another game version does not open.
    let mut folder = Folder::default();
    make(&mut folder, "world.dat", 1, 0, 2);
    let look = Folder(folder.0.clone());
    assert!(!client().open_portal(&mut folder, &look, "world.dat", "pictures.dat"));
}

/// Behaviour: data.client-files.opened-as-the-client-opens-them
#[test]
fn a_regions_cell_file_is_made_when_missing_and_replaced_by_another_regions() {
    let mut folder = Folder::default();
    let mut c = client();
    let look = Folder::default();
    assert!(c.set_region(&mut folder, &look, 3));
    let cell = c.set.by_slot[Slot::Cell as usize].clone().unwrap();
    {
        let s = cell.lock();
        assert_eq!((s.header().data_set, s.header().data_subset), (2, 3));
        assert_eq!(s.header().block_size, NEW_CELL_FILE.1);
        assert_eq!(s.get_filename(), "", "a file made is not named");
    }
    assert!(folder.0.contains_key("cell_3.dat"));
    assert!(
        c.set_region(&mut folder, &look, 3),
        "the same region: nothing to do"
    );
    assert!(Arc::ptr_eq(
        &cell,
        c.set.by_slot[Slot::Cell as usize].as_ref().unwrap()
    ));
    assert!(c.set_region(&mut folder, &look, 4));
    assert!(!cell.is_initialized(), "the old region's file is closed");
    assert!(c.set.is_dat_file_loaded(0x2_0000_0004));
    assert!(!c.set.is_dat_file_loaded(0x2_0000_0003));
}

/// Behaviour: data.client-files.opened-as-the-client-opens-them
#[test]
fn a_languages_file_must_be_there_and_named() {
    let mut folder = Folder::default();
    make(&mut folder, "local_Testish.dat", 3, 0x8000_0001, 3);
    let look = Folder(folder.0.clone());
    let mut c = client();
    assert_eq!(
        c.set_language_internal(&mut folder, &look, 0x8000_0001, None, false),
        Err(NO_LANGUAGE_NOTICE)
    );
    assert_eq!(
        c.set_language_internal(&mut folder, &look, 0x8100_0003, Some("Deutsch"), false),
        Err(NO_LANGUAGE_NOTICE)
    );
    assert_eq!(
        c.set_language_internal(&mut folder, &look, 0x8000_0001, Some("Testish"), false),
        Ok(())
    );
    assert_eq!(c.language, Some(0x8000_0001));
    assert!(c.set.is_dat_file_loaded(0x3_8000_0001));
}
