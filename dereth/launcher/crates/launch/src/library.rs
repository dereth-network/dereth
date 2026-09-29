//! The library: the one retail client, and the dat sets.
//!
//! The player points the launcher at folders. A folder holding `acclient.exe` becomes the retail
//! client (there is only ever one: a second replaces the first), and a folder holding the four dats
//! becomes a dat set, which the Dereth client can be given. A retail install is usually both: its
//! dats go into the library too, so the Dereth client can use them.
//!
//! The Dereth client is not added: it ships beside the launcher, and [`dereth_client`] reads it.

use std::path::Path;

use serde::{Deserialize, Serialize};

use crate::datset::{scan_dir, DatOrigin, DatSet, SHARED_SET_ID};
use crate::install::{identify_dereth, identify_retail, Installation};
use crate::state::LauncherState;

/// What a folder the player pointed at holds.
#[derive(Debug, Clone, PartialEq, Eq, Default, Serialize, Deserialize)]
pub struct FolderFind {
    pub folder: String,
    /// A retail client, when the folder holds `acclient.exe`.
    pub retail: Option<Installation>,
    pub dats: Option<DatSet>,
    pub error: Option<String>,
}

/// Read a folder: whether it holds the retail client, and which dats.
pub fn read_folder(path: &Path, state: &LauncherState) -> FolderFind {
    let retail = identify_retail(path, "retail").ok();
    let files = scan_dir(path);
    let dats = (!files.is_empty()).then(|| DatSet {
        id: state.new_id("d"),
        path: path.to_path_buf(),
        origin: DatOrigin::Unassigned,
        files,
        last_patched_by_server: None,
        created_by_launcher: false,
    });
    let error = (retail.is_none() && dats.is_none())
        .then(|| "No acclient.exe and no data files in this folder.".to_owned());
    FolderFind {
        folder: path.display().to_string(),
        retail,
        dats,
        error,
    }
}

/// Add what a folder held. Its retail client replaces the one before; its dats join the library,
/// or update the set already there for the same folder. The first set added is the default (the
/// shared set); the player can make another one the default later ([`LauncherState::set_default_set`]).
pub fn add_find(s: &mut LauncherState, find: FolderFind) {
    if let Some(retail) = find.retail {
        s.retail = Some(retail);
    }
    if let Some(mut set) = find.dats {
        if let Some(existing) = s.dat_sets.iter().find(|d| d.path == set.path) {
            set.id = existing.id.clone();
            set.origin = existing.origin.clone();
        } else if s.shared_set().is_none() {
            set.origin = DatOrigin::Shared;
            if s.dat_set(SHARED_SET_ID).is_none() {
                set.id = SHARED_SET_ID.into();
            }
        }
        s.dat_sets.retain(|d| d.path != set.path);
        s.dat_sets.push(set);
    }
}

/// The Dereth client at `exe`, if it is there.
pub fn dereth_client(exe: &Path) -> Option<Installation> {
    identify_dereth(exe).ok()
}

/// The retail client's own dats, as a set, for the check: a retail client plays with nothing else.
pub fn retail_dats(retail: &Installation) -> DatSet {
    DatSet {
        id: "retail".into(),
        path: retail.path.clone(),
        origin: DatOrigin::Unassigned,
        files: scan_dir(&retail.path),
        last_patched_by_server: None,
        created_by_launcher: false,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::datset::tests::{fake_set, tmp};
    use crate::datset::Iterations;
    use crate::install::ClientKind;

    #[test]
    fn a_retail_folder_becomes_the_retail_client_and_its_dats_the_shared_set() {
        let d = tmp("lib-retail");
        fake_set(&d, Iterations::END_OF_RETAIL);
        std::fs::write(d.join("acclient.exe"), b"MZ").unwrap();
        let mut s = LauncherState::default();
        let find = read_folder(&d, &s);
        assert_eq!(
            find.retail.as_ref().map(|r| r.kind),
            Some(ClientKind::Retail)
        );
        add_find(&mut s, find);
        assert_eq!(s.retail.as_ref().unwrap().path, d);
        assert_eq!(s.shared_set().unwrap().path, d);

        // Adding the same folder again changes nothing but refreshes it.
        let again = read_folder(&d, &s);
        add_find(&mut s, again);
        assert_eq!(s.dat_sets.len(), 1);
        let _ = std::fs::remove_dir_all(&d);
    }

    #[test]
    fn there_is_only_ever_one_retail_client() {
        let (a, b) = (tmp("lib-a"), tmp("lib-b"));
        let mut s = LauncherState::default();
        for d in [&a, &b] {
            std::fs::create_dir_all(d).unwrap();
            std::fs::write(d.join("acclient.exe"), b"MZ").unwrap();
            let find = read_folder(d, &s);
            add_find(&mut s, find);
        }
        assert_eq!(s.retail.as_ref().unwrap().path, b);
        let _ = std::fs::remove_dir_all(&a);
        let _ = std::fs::remove_dir_all(&b);
    }

    #[test]
    fn a_folder_of_dats_alone_is_a_set_and_not_a_client() {
        let d = tmp("lib-dats");
        fake_set(&d, Iterations::END_OF_RETAIL);
        let mut s = LauncherState::default();
        let find = read_folder(&d, &s);
        assert!(find.retail.is_none() && find.dats.is_some());
        add_find(&mut s, find);
        assert!(s.retail.is_none());
        assert_eq!(s.dat_sets.len(), 1);
        assert!(
            s.shared_set().is_some(),
            "the first set added is the default"
        );

        // A second set is not the default until the player says so.
        let other = tmp("lib-dats-2");
        fake_set(&other, Iterations::END_OF_RETAIL);
        let find = read_folder(&other, &s);
        let id = find.dats.as_ref().unwrap().id.clone();
        add_find(&mut s, find);
        assert_eq!(s.shared_set().unwrap().path, d);
        assert!(s.set_default_set(&id));
        assert_eq!(s.shared_set().unwrap().path, other);
        assert_eq!(
            s.dat_sets
                .iter()
                .filter(|x| x.origin == DatOrigin::Shared)
                .count(),
            1
        );
        let _ = std::fs::remove_dir_all(&other);
        let empty = tmp("lib-empty");
        std::fs::create_dir_all(&empty).unwrap();
        assert!(read_folder(&empty, &s).error.is_some());
        let _ = std::fs::remove_dir_all(&d);
        let _ = std::fs::remove_dir_all(&empty);
    }
}
