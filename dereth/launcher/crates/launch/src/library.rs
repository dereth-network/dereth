//! The library: the one retail client, and the dat sets.
//!
//! The player points the launcher at folders. A folder holding `acclient.exe` becomes the retail
//! client (there is only ever one: a second replaces the first), a folder holding the four later
//! dats becomes a Modern set, and one holding `portal.dat` and `cell.dat` a Classic set; the Dereth
//! client can be given either. A retail install is usually a client and a Modern set: its dats go
//! into the library too, so the Dereth client can use them. A folder may hold both kinds of set.
//!
//! The Dereth client is not added: it ships beside the launcher, and [`dereth_client`] reads it.

use std::path::Path;

use serde::{Deserialize, Serialize};

use crate::datset::{
    is_classic_pair, scan_classic_dir, scan_dir, DatOrigin, DatSet, SetKind, CLASSIC_FILES,
    SHARED_SET_ID,
};
use crate::install::{identify_dereth, identify_retail, Installation};
use crate::state::LauncherState;

/// What a folder the player pointed at holds.
#[derive(Debug, Clone, PartialEq, Eq, Default, Serialize, Deserialize)]
pub struct FolderFind {
    pub folder: String,
    /// A retail client, when the folder holds `acclient.exe`.
    pub retail: Option<Installation>,
    /// The Modern set, when the folder holds any of the four later dats.
    pub dats: Option<DatSet>,
    /// The Classic set, when the folder holds both `portal.dat` and `cell.dat`.
    #[serde(default)]
    pub classic: Option<DatSet>,
    pub error: Option<String>,
}

/// Read a folder: whether it holds the retail client, and which dats.
pub fn read_folder(path: &Path, state: &LauncherState) -> FolderFind {
    let retail = identify_retail(path, "retail").ok();
    let files = scan_dir(path);
    let dats = (!files.is_empty()).then(|| DatSet {
        id: state.new_id("d"),
        path: path.to_path_buf(),
        kind: SetKind::Modern,
        origin: DatOrigin::Unassigned,
        files,
        last_patched_by_server: None,
        created_by_launcher: false,
    });
    let classic_files = scan_classic_dir(path);
    let half_pair = !classic_files.is_empty() && !is_classic_pair(&classic_files);
    let classic = is_classic_pair(&classic_files).then(|| DatSet {
        // Its own prefix, so it never takes the Modern set's id from the same reading.
        id: state.new_id("c"),
        path: path.to_path_buf(),
        kind: SetKind::Classic,
        origin: DatOrigin::Unassigned,
        files: classic_files.clone(),
        last_patched_by_server: None,
        created_by_launcher: false,
    });
    let error = if half_pair {
        let (have, lacks) = if classic_files[0].file_name == CLASSIC_FILES[0].1 {
            (CLASSIC_FILES[0].1, CLASSIC_FILES[1].1)
        } else {
            (CLASSIC_FILES[1].1, CLASSIC_FILES[0].1)
        };
        (retail.is_none() && dats.is_none()).then(|| {
            format!("This folder has {have} but no {lacks}: Classic data files come as a pair.")
        })
    } else {
        (retail.is_none() && dats.is_none() && classic.is_none())
            .then(|| "No acclient.exe and no data files in this folder.".to_owned())
    };
    FolderFind {
        folder: path.display().to_string(),
        retail,
        dats,
        classic,
        error,
    }
}

/// Add what a folder held. Its retail client replaces the one before; its sets join the library,
/// or update the set of the same kind already there for the same folder. The first set of each
/// kind added is that kind's default; the player can make another one the default later
/// ([`LauncherState::set_default_set`]).
pub fn add_find(s: &mut LauncherState, find: FolderFind) {
    if let Some(retail) = find.retail {
        s.retail = Some(retail);
    }
    for mut set in find.dats.into_iter().chain(find.classic) {
        let same = |d: &DatSet| d.path == set.path && d.kind == set.kind;
        if let Some(existing) = s.dat_sets.iter().find(|d| same(d)) {
            set.id = existing.id.clone();
            set.origin = existing.origin.clone();
        } else {
            // Ids come from the folder's reading; one taken since (by the other kind's set from
            // the same reading, say) is replaced.
            if s.dat_set(&set.id).is_some() {
                set.id = s.new_id(if set.kind == SetKind::Classic {
                    "c"
                } else {
                    "d"
                });
            }
            if s.default_set(set.kind).is_none() {
                set.origin = DatOrigin::Shared;
                if set.kind == SetKind::Modern && s.dat_set(SHARED_SET_ID).is_none() {
                    set.id = SHARED_SET_ID.into();
                }
            }
        }
        s.dat_sets.retain(|d| !same(d));
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
        kind: SetKind::Modern,
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
    fn a_classic_pair_is_a_classic_set_with_its_own_default_beside_a_modern_one() {
        let d = tmp("lib-classic");
        fake_set(&d, Iterations::END_OF_RETAIL);
        std::fs::write(d.join("portal.dat"), b"p").unwrap();
        std::fs::write(d.join("cell.dat"), b"c").unwrap();
        let mut s = LauncherState::default();
        let find = read_folder(&d, &s);
        let (m, c) = (find.dats.clone().unwrap(), find.classic.clone().unwrap());
        assert_eq!((m.kind, c.kind), (SetKind::Modern, SetKind::Classic));
        assert_ne!(m.id, c.id);
        add_find(&mut s, find);
        assert_eq!(s.dat_sets.len(), 2, "one folder, both kinds");
        assert_eq!(s.shared_set().unwrap().id, SHARED_SET_ID);
        assert_eq!(s.default_set(SetKind::Classic).unwrap().path, d);

        // A second Classic folder joins without taking the default.
        let other = tmp("lib-classic-2");
        std::fs::create_dir_all(&other).unwrap();
        std::fs::write(other.join("portal.dat"), b"p").unwrap();
        std::fs::write(other.join("cell.dat"), b"c").unwrap();
        let find = read_folder(&other, &s);
        assert!(find.dats.is_none() && find.error.is_none());
        add_find(&mut s, find);
        assert_eq!(s.dat_sets.len(), 3);
        assert_eq!(s.default_set(SetKind::Classic).unwrap().path, d);
        assert!(
            s.dat_sets
                .iter()
                .map(|x| &x.id)
                .collect::<std::collections::HashSet<_>>()
                .len()
                == 3,
            "every id is its own"
        );

        // Adding the first folder again refreshes both of its sets.
        let again = read_folder(&d, &s);
        add_find(&mut s, again);
        assert_eq!(s.dat_sets.len(), 3);
        let _ = std::fs::remove_dir_all(&d);
        let _ = std::fs::remove_dir_all(&other);
    }

    #[test]
    fn half_a_classic_pair_is_named_as_half() {
        let d = tmp("lib-half");
        std::fs::create_dir_all(&d).unwrap();
        std::fs::write(d.join("cell.dat"), b"c").unwrap();
        let find = read_folder(&d, &LauncherState::default());
        assert!(find.classic.is_none());
        assert!(
            find.error
                .as_deref()
                .unwrap()
                .contains("cell.dat but no portal.dat"),
            "{:?}",
            find.error
        );
        let _ = std::fs::remove_dir_all(&d);
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
