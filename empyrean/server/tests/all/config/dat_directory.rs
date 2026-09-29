//! Divergence: V375
//! Dat_files_directory default empty is platform-neutral; empty searches config folder then exe;
//! relative/home/absolute resolve as configured paths.
//! Fixture: isolated configuration paths and synthetic server state.

use std::collections::BTreeSet;
use std::path::PathBuf;

use empyrean_common::config_paths::PathBase;
use empyrean_common::master_configuration::MasterConfiguration;
use empyrean_server::dat_directory::dat_directory_candidates;

fn root() -> PathBuf {
    std::path::absolute("/").unwrap()
}

fn cwd() -> PathBuf {
    root().join("work")
}

fn config_dir() -> PathBuf {
    root().join("etc").join("empyrean")
}

fn exe() -> PathBuf {
    root().join("opt").join("empyrean")
}

fn base(with_config: bool) -> PathBase {
    let file = config_dir().join("empyrean.toml");
    PathBase::new(
        with_config.then_some(file.as_path()),
        &cwd(),
        Some(root().join("home")),
        Some(exe()),
    )
}

/// The folder the server would use if exactly `holding` held the dats: the first candidate that
/// does (what `dereth_dat::locate_retail_dats` answers on disk), else the first candidate.
fn find(configured: &str, with_config: bool, holding: &[PathBuf]) -> PathBuf {
    let holding: BTreeSet<PathBuf> = holding.iter().cloned().collect();
    let candidates = dat_directory_candidates(configured, &base(with_config));
    candidates
        .iter()
        .find(|dir: &&PathBuf| holding.contains(dir.as_path()))
        .unwrap_or(&candidates[0])
        .clone()
}

#[test]
fn the_default_is_empty_and_platform_neutral() {
    assert_eq!(
        MasterConfiguration::default().server.dat_files_directory,
        ""
    );
}

#[test]
fn an_empty_directory_is_the_config_folder_then_beside_the_executable() {
    let (c, e) = (config_dir().join(""), exe().join(""));
    assert_eq!(find("", true, &[c.clone(), e.clone()]), c);
    assert_eq!(
        find("", true, &[e.clone(), cwd().join("")]),
        e,
        "the working directory is not searched"
    );
    // found nowhere: the config folder, so the error names it
    assert_eq!(find("", true, &[]), c);
    // no configuration file: the working directory, then beside the executable
    assert_eq!(
        find("", false, &[cwd().join(""), e.clone()]),
        cwd().join("")
    );
    assert_eq!(find("", false, std::slice::from_ref(&e)), e);
}

#[test]
fn a_relative_directory_is_relative_to_the_config_folder() {
    // no search: the one folder, whether or not it holds the dats
    assert_eq!(
        find("dats", true, &[exe().join("dats"), cwd().join("dats")]),
        config_dir().join("dats")
    );
    assert_eq!(find("./dats", true, &[]), config_dir().join("dats"));
    assert_eq!(find("dats", false, &[]), cwd().join("dats"));
}

#[test]
fn a_home_directory_is_expanded() {
    assert_eq!(find("~/ac", true, &[]), root().join("home").join("ac"));
}

#[test]
fn an_absolute_directory_is_used_as_given() {
    let abs = if cfg!(windows) {
        "D:\\Games\\AC\\"
    } else {
        "/srv/ac/"
    };
    assert_eq!(find(abs, true, &[cwd().join("")]), PathBuf::from(abs));
    assert_eq!(find(abs, false, &[]), PathBuf::from(abs));
}
