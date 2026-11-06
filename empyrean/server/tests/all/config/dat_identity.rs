//! Divergence: V477
//! The data files a world starts on: each is named by the iteration it carries (and its hash when
//! it is small), and a world whose era expects other files refuses to start unless its operator
//! allows it. Fixture: synthetic container headers from before Throne of Destiny written to a
//! temporary folder; no retail bytes.

use std::path::Path;

use dereth_dat::testing::{classic_container, ScratchDir};
use dereth_dat::ContainerEra;
use empyrean_common::era::EraId;
use empyrean_server::dat_identity::{
    fingerprint, verdict, DatFingerprint, FileFingerprint, Verdict,
};

/// A folder holding a synthetic classic pair carrying these iterations.
fn classic_pair(dir: &Path, portal: u32, cell: u32) {
    std::fs::write(
        dir.join("portal.dat"),
        classic_container(0x400, portal, &[]),
    )
    .unwrap();
    std::fs::write(dir.join("cell.dat"), classic_container(0x100, cell, &[])).unwrap();
}

#[test]
fn a_launch_world_starts_on_a_1999_pair_and_names_its_files() {
    let dir = ScratchDir::new("dat-identity-1999").unwrap();
    classic_pair(dir.path(), 105, 67);
    let files = fingerprint(dir.path(), ContainerEra::Classic);
    assert_eq!(
        (files.portal.iteration, files.cell.iteration),
        (Some(105), Some(67))
    );
    assert!(files.portal.sha256.is_some(), "a small file is hashed");
    assert_eq!(verdict(EraId::Launch, &files, false), Verdict::Suits);
    let said = files.describe();
    assert!(
        said.starts_with("portal.dat iteration 105 (SHA-256 "),
        "{said}"
    );
    assert!(said.contains("cell.dat iteration 67"), "{said}");
}

#[test]
fn a_launch_world_refuses_the_2005_pair_unless_its_operator_allows_it() {
    let dir = ScratchDir::new("dat-identity-2005").unwrap();
    classic_pair(dir.path(), 2112, 1593);
    let files = fingerprint(dir.path(), ContainerEra::Classic);
    let Verdict::Refused(line) = verdict(EraId::Launch, &files, false) else {
        panic!("a Launch world on the February 2005 files starts");
    };
    assert!(
        line.starts_with("The launch era expects the files from before Throne of Destiny (portal.dat at iterations 102 to 123, cell.dat at iterations 67 to 107), and these are portal.dat iteration 2112"),
        "{line}"
    );
    assert!(
        line.ends_with("or set any_dat_files = true in [era] to start anyway."),
        "{line}"
    );
    let Verdict::Allowed(line) = verdict(EraId::Launch, &files, true) else {
        panic!("the operator's setting is not honored");
    };
    assert!(
        line.ends_with("Starting anyway: [era] any_dat_files is set."),
        "{line}"
    );
    assert_eq!(verdict(EraId::Infiltration, &files, false), Verdict::Suits);
}

#[test]
fn a_launch_world_refuses_the_later_files_and_an_unreadable_pair() {
    let later = DatFingerprint {
        container: ContainerEra::Modern,
        portal: FileFingerprint {
            file: "client_portal.dat",
            iteration: Some(2072),
            sha256: None,
        },
        cell: FileFingerprint {
            file: "client_cell_1.dat",
            iteration: Some(982),
            sha256: None,
        },
    };
    assert!(matches!(
        verdict(EraId::Launch, &later, false),
        Verdict::Refused(_)
    ));
    assert_eq!(verdict(EraId::Eor, &later, false), Verdict::Suits);

    let dir = ScratchDir::new("dat-identity-garbage").unwrap();
    std::fs::write(dir.path().join("portal.dat"), b"not a data file").unwrap();
    std::fs::write(dir.path().join("cell.dat"), b"not a data file").unwrap();
    let files = fingerprint(dir.path(), ContainerEra::Classic);
    assert_eq!(files.portal.iteration, None);
    assert!(files
        .describe()
        .starts_with("portal.dat no readable iteration"));
    assert!(matches!(
        verdict(EraId::Launch, &files, false),
        Verdict::Refused(_)
    ));
}

#[test]
fn an_infiltration_world_refuses_the_1999_pair() {
    let dir = ScratchDir::new("dat-identity-feb").unwrap();
    classic_pair(dir.path(), 105, 67);
    let files = fingerprint(dir.path(), ContainerEra::Classic);
    assert!(matches!(
        verdict(EraId::Infiltration, &files, false),
        Verdict::Refused(_)
    ));
}
