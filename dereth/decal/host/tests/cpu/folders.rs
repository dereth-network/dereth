//! Behaviour: none (a host inside another program: where it finds plugins)
//!
//! Besides its plugins folders, the host finds the plugins the launcher installed, as the launcher
//! offers them to the Dereth client: each plugin installed from a plugin source, in the launcher's
//! data folder. Like every found plugin, each is off until the player ticks it.

use dereth_launch::folders::Folders;
use dereth_launch::plugins::Shelf;
use dereth_launch::testing::plugin_list;
use dereth_testkit::plugins::read_package;

use crate::abi::{plugin_dir, settings, Replay};

/// A source of the launcher's, listing both test plugins.
const SOURCE: &str = "https://example.invalid/plugins.json";

/// A launcher's data folder whose source lists `test.window` and `test.probe`, with only
/// `test.window` installed, through the launcher's own install.
fn launcher_with_one_of_two_plugins(tag: &str) -> std::path::PathBuf {
    let data = std::env::temp_dir().join(format!("dereth-decal-host-{tag}-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&data);
    let mut shelf = Shelf::open(Folders::single(&data).plugins());
    shelf.add_source(SOURCE).expect("the source is added");
    let window = read_package(&plugin_dir("window")).to_zip();
    let probe = read_package(&plugin_dir("probe")).to_zip();
    shelf
        .record_read(SOURCE, Ok(plugin_list(&[&window, &probe])), 1)
        .expect("the list is kept");
    shelf
        .install(SOURCE, "test.window", &window)
        .expect("the launcher installs it");
    data
}

#[test]
fn the_plugins_the_launcher_installed_are_found_and_load_once_ticked() {
    let data = launcher_with_one_of_two_plugins("launcher-on");
    let mut r = Replay::frames_only(
        &settings(&[])
            .with("launcher", true)
            .with("launcher_data", data.to_string_lossy().into_owned()),
    );
    r.frame();
    assert_eq!(
        r.status("test.window"),
        "absent",
        "found, and off until ticked"
    );
    assert!(r.type_line("/plugin on test.window"));
    r.frame();
    r.frame();
    assert_ne!(r.status("test.window"), "absent", "ticked");
    assert!(r.type_line("/plugin on test.probe"));
    r.frame();
    r.frame();
    assert_eq!(r.status("test.probe"), "absent", "listed, not installed");
    let _ = std::fs::remove_dir_all(&data);
}

#[test]
fn without_the_launcher_setting_its_plugins_do_not_load() {
    let data = launcher_with_one_of_two_plugins("launcher-off");
    let mut r = Replay::frames_only(
        &settings(&[]).with("launcher_data", data.to_string_lossy().into_owned()),
    );
    r.frame();
    assert_eq!(r.status("test.window"), "absent");
    let _ = std::fs::remove_dir_all(&data);
}
