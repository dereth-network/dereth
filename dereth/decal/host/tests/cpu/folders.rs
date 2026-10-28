//! Behaviour: none (a host inside another program: where it finds plugins)
//!
//! Besides its plugins folders, the host finds the plugins the launcher installed, as the launcher
//! offers them to the Dereth client: each plugin installed from a plugin source, in the launcher's
//! data folder, and each plugin in the folders on this computer the launcher lists. As in the
//! Dereth client, each is ticked the first time the host sees it, and an untick is kept.

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
fn the_plugins_the_launcher_installed_load_at_first_and_an_untick_is_kept() {
    let data = launcher_with_one_of_two_plugins("launcher-on");
    let stores = std::env::temp_dir().join(format!(
        "dereth-decal-host-launcher-on-stores-{}",
        std::process::id()
    ));
    let _ = std::fs::remove_dir_all(&stores);
    let run = || {
        Replay::frames_only(
            &settings(&[])
                .with("launcher", true)
                .with("launcher_data", data.to_string_lossy().into_owned())
                .with("data", stores.to_string_lossy().into_owned()),
        )
    };
    let mut r = run();
    r.frame();
    r.frame();
    assert_ne!(
        r.status("test.window"),
        "absent",
        "ticked the first time it is found"
    );
    assert!(r.type_line("/plugin enable test.probe"));
    r.frame();
    r.frame();
    assert_eq!(r.status("test.probe"), "absent", "listed, not installed");
    assert!(r.type_line("/plugin disable test.window"));
    r.frame();
    r.frame();
    assert_eq!(r.status("test.window"), "absent", "unticked");
    drop(r);
    let mut again = run();
    again.frame();
    again.frame();
    assert_eq!(
        again.status("test.window"),
        "absent",
        "the untick is kept, though the launcher offers it still"
    );
    let _ = std::fs::remove_dir_all(&data);
    let _ = std::fs::remove_dir_all(&stores);
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

/// Every file under `dir`, by its path, and its bytes.
fn files_under(dir: &std::path::Path) -> std::collections::BTreeMap<std::path::PathBuf, Vec<u8>> {
    let mut out = std::collections::BTreeMap::new();
    let mut stack = vec![dir.to_path_buf()];
    while let Some(d) = stack.pop() {
        for entry in std::fs::read_dir(&d).expect("a folder") {
            let path = entry.expect("an entry").path();
            if path.is_dir() {
                stack.push(path);
            } else {
                let bytes = std::fs::read(&path).expect("a file");
                out.insert(path, bytes);
            }
        }
    }
    out
}

/// The plugins in the folders on this computer the launcher lists are found as it offers them to
/// the Dereth client, from the folders themselves, and the host writes nothing in the launcher's
/// folder or in theirs.
#[test]
fn the_plugins_in_the_launchers_folders_on_this_computer_are_found_and_nothing_is_written() {
    let data = launcher_with_one_of_two_plugins("launcher-local");
    let mine = data.join("mine");
    let probe = mine.join("probe");
    std::fs::create_dir_all(&probe).expect("a folder");
    for f in std::fs::read_dir(plugin_dir("probe")).expect("the test plugin") {
        let f = f.expect("a file").path();
        std::fs::copy(&f, probe.join(f.file_name().expect("a name"))).expect("copied");
    }
    Shelf::open(Folders::single(&data).plugins())
        .add_local(&mine)
        .expect("listed");
    let before = files_under(&data);
    let mut r = Replay::frames_only(
        &settings(&[])
            .with("launcher", true)
            .with("launcher_data", data.to_string_lossy().into_owned()),
    );
    r.frame();
    r.frame();
    assert_ne!(
        r.status("test.probe"),
        "absent",
        "ticked the first time it is found, from its folder"
    );
    drop(r);
    assert_eq!(files_under(&data), before, "nothing written");
    let _ = std::fs::remove_dir_all(&data);
}
