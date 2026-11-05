//! Behaviour: none (the launcher's own plumbing: plugin sources, installing and updating from
//! them, checked over bytes served by the test).

use super::{
    AutoUpdate, Availability, Keep, PluginError, Refresh, Shelf, FIRST_PARTY, INSTALLED_FILE,
    SOURCES_FILE,
};
use crate::testing::{plugin_list as list, plugin_zip as zip, write_plugin_folder};
use dereth_plugin_api::ApiVersion;

/// A folder of its own for each test, removed when it ends.
struct Scratch(std::path::PathBuf);

impl Scratch {
    fn new(tag: &str) -> Self {
        let d = std::env::temp_dir().join(format!(
            "dereth-launch-sources-{tag}-{}",
            std::process::id()
        ));
        let _ = std::fs::remove_dir_all(&d);
        Self(d)
    }
}

impl Drop for Scratch {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.0);
    }
}

const OTHER: &str = "https://example.github.io/plugins/plugins.json";

const CHAT: &str = "[permissions]\n\"chat.send\" = \"Says hello.\"\n";
const CHAT_AND_LOOT: &str =
    "[permissions]\n\"chat.send\" = \"Says hello.\"\n\"auto.loot\" = \"Loots.\"\n";

fn with_other_source(dir: &Scratch) -> Shelf {
    let mut shelf = Shelf::open(&dir.0);
    shelf.add_source(OTHER).expect("added");
    shelf
}

#[test]
fn the_launcher_starts_with_dereths_own_source_and_nothing_installed() {
    let dir = Scratch::new("start");
    let shelf = Shelf::open(&dir.0);
    let urls: Vec<&str> = shelf.sources.iter().map(|s| s.url.as_str()).collect();
    assert_eq!(urls, [FIRST_PARTY]);
    assert!(shelf.plugins.is_empty());
    assert!(shelf.offered_packages().is_empty());
    assert_eq!(shelf.auto_update, AutoUpdate::FirstParty);
    let view = shelf.view();
    assert!(view.sources[0].first_party);
    assert_eq!(view.sources[0].name, "Dereth");
}

#[test]
fn another_source_is_added_by_its_https_address() {
    let dir = Scratch::new("add");
    let mut shelf = Shelf::open(&dir.0);
    for bad in [
        "http://example.org/plugins.json",
        "example.org/plugins.json",
        "https://",
        "https://a b/c",
    ] {
        assert!(
            matches!(shelf.add_source(bad), Err(PluginError::NotHttps(_))),
            "{bad}"
        );
    }
    assert_eq!(shelf.sources.len(), 1, "a refused address adds nothing");
    assert_eq!(
        shelf.add_source(&format!("  {OTHER} ")).as_deref(),
        Ok(OTHER)
    );
    let second = "https://plugins.example.net/list.json";
    assert!(shelf.add_source(second).is_ok());
    assert_eq!(
        shelf.add_source(OTHER),
        Err(PluginError::AlreadyAdded(OTHER.into()))
    );
    assert_eq!(
        shelf.add_source(FIRST_PARTY),
        Err(PluginError::AlreadyAdded(FIRST_PARTY.into()))
    );
    assert_eq!(
        shelf.remove_source(FIRST_PARTY),
        Err(PluginError::FirstPartyStays)
    );
    shelf.remove_source(second).expect("removed");
    // Kept across a reopen.
    let again = Shelf::open(&dir.0);
    let urls: Vec<&str> = again.sources.iter().map(|s| s.url.as_str()).collect();
    assert_eq!(urls, [FIRST_PARTY, OTHER]);
    assert!(dir.0.join(SOURCES_FILE).is_file());
}

#[test]
fn a_sources_file_an_earlier_launcher_wrote_with_its_warning_flag_keeps_its_sources() {
    let dir = Scratch::new("earlier-flag");
    std::fs::create_dir_all(&dir.0).expect("folder");
    let path = dir.0.join(SOURCES_FILE);
    std::fs::write(
        &path,
        format!(
            r#"{{"sources": [{{"url": "{OTHER}"}}], "auto_update": "all", "others_allowed": true}}"#
        ),
    )
    .expect("an earlier file");
    let mut shelf = Shelf::open(&dir.0);
    let urls: Vec<&str> = shelf.sources.iter().map(|s| s.url.as_str()).collect();
    assert_eq!(urls, [FIRST_PARTY, OTHER], "the sources stay");
    assert_eq!(shelf.auto_update, AutoUpdate::All, "the setting stays");
    // Written again, the flag is gone and the rest kept.
    shelf
        .add_source("https://plugins.example.net/list.json")
        .expect("added");
    let kept = std::fs::read_to_string(&path).expect("the file");
    assert!(!kept.contains("others_allowed"), "{kept}");
    assert_eq!(Shelf::open(&dir.0).sources.len(), 3);
}

#[test]
fn a_sources_list_is_kept_and_a_failed_read_keeps_the_last_one_and_says_why() {
    let dir = Scratch::new("read");
    let mut shelf = with_other_source(&dir);
    let meter = zip("org.example.meter", "1.0.0", "0.7", "");
    shelf
        .record_read(OTHER, Ok(list(&[&meter])), 100)
        .expect("recorded");
    shelf
        .record_read(OTHER, Err("not found (404)".into()), 200)
        .expect("recorded");
    let s = shelf.source(OTHER).expect("the source");
    assert_eq!(s.catalog.plugins.len(), 1, "the last list stays in use");
    assert_eq!(
        (s.read_at, s.error.as_deref()),
        (Some(100), Some("not found (404)"))
    );
    shelf
        .record_read(OTHER, Ok(b"<html>a page, not a list</html>".to_vec()), 300)
        .expect("recorded");
    let s = shelf.source(OTHER).expect("the source");
    assert!(s
        .error
        .as_deref()
        .is_some_and(|e| e.starts_with("not a plugin list")));
    assert_eq!(s.catalog.plugins.len(), 1);
    // Reopened, the last list read is there without a read.
    let again = Shelf::open(&dir.0);
    let s = again.source(OTHER).expect("the source");
    assert_eq!(s.catalog.plugins[0].id.as_str(), "org.example.meter");
    assert_eq!(again.view().available.len(), 1);
    assert_eq!(again.view().available[0].state, Availability::Install);
}

#[test]
fn a_plugin_is_installed_only_when_its_zip_is_the_release_its_source_lists() {
    let dir = Scratch::new("install");
    let mut shelf = with_other_source(&dir);
    let meter = zip(
        "org.example.meter",
        "1.0.0",
        "0.7",
        &format!("description = \"Counts.\\nPer hour too.\"\nauthors = [\"Someone\"]\n{CHAT}"),
    );
    shelf
        .record_read(OTHER, Ok(list(&[&meter])), 1)
        .expect("recorded");
    let plan = shelf
        .plan_install(OTHER, "org.example.meter")
        .expect("a plan");
    assert_eq!(
        plan.url,
        "https://example.github.io/plugins/org.example.meter.zip"
    );
    assert_eq!(plan.limit, meter.len(), "no more than the source says");
    // Another file at that address: refused, and nothing kept.
    let other = zip("org.example.meter", "1.0.0", "0.7", CHAT_AND_LOOT);
    assert!(matches!(
        shelf.install(OTHER, "org.example.meter", &other),
        Err(PluginError::Mismatch { .. })
    ));
    assert!(shelf.plugins.is_empty() && shelf.offered_packages().is_empty());
    let done = shelf
        .install(OTHER, "org.example.meter", &meter)
        .expect("installed");
    assert_eq!((done.version.as_str(), done.updated), ("1.0.0", false));
    let offered = shelf.offered_packages();
    assert_eq!(offered.len(), 1);
    assert_eq!(std::fs::read(&offered[0]).expect("the zip"), meter);
    assert_eq!(shelf.plugins[0].release.permissions, ["chat.send"]);
    // Listed under Installed, with what its source said of it, and no longer as available.
    let view = shelf.view();
    assert!(view.available.is_empty());
    let p = &view.installed[0];
    assert_eq!(
        (p.description.as_str(), p.authors.as_slice()),
        ("Counts.\nPer hour too.", ["Someone".to_owned()].as_slice())
    );
    assert_eq!(
        (p.released.as_str(), p.notes.as_str(), p.size),
        ("2026-10-26", "Notes.", Some(meter.len() as u64))
    );
    // Installed again: nothing newer.
    assert!(matches!(
        shelf.plan_install(OTHER, "org.example.meter"),
        Err(PluginError::UpToDate { .. })
    ));
    // Kept across a reopen; removed, the client is offered nothing.
    let mut again = Shelf::open(&dir.0);
    assert_eq!(again.plugins, shelf.plugins);
    again.remove("org.example.meter").expect("removed");
    assert!(again.offered_packages().is_empty());
    assert!(Shelf::open(&dir.0).plugins.is_empty());
    assert!(dir.0.join(INSTALLED_FILE).is_file());
}

#[test]
fn a_release_needing_a_newer_dereth_is_listed_but_not_installed() {
    let dir = Scratch::new("api");
    let mut shelf = with_other_source(&dir);
    shelf.host = ApiVersion { major: 0, minor: 7 };
    let later = zip("org.example.later", "1.0.0", "0.9", "");
    shelf
        .record_read(OTHER, Ok(list(&[&later])), 1)
        .expect("recorded");
    assert_eq!(
        shelf.view().available[0].state,
        Availability::NeedsNewerDereth
    );
    assert!(matches!(
        shelf.plan_install(OTHER, "org.example.later"),
        Err(PluginError::NeedsNewerDereth { .. })
    ));
    assert!(matches!(
        shelf.install(OTHER, "org.example.later", &later),
        Err(PluginError::NeedsNewerDereth { .. })
    ));
    assert_eq!(
        shelf
            .install(OTHER, "org.example.later", &later)
            .unwrap_err()
            .to_string(),
        "Meter needs a newer Dereth (plugin API 0.9)"
    );
}

#[test]
fn dereths_own_ids_install_only_from_dereths_source() {
    let dir = Scratch::new("namespace");
    let mut shelf = with_other_source(&dir);
    let own = zip("network.dereth.xpmeter", "9.0.0", "0.7", "");
    shelf
        .record_read(OTHER, Ok(list(&[&own])), 1)
        .expect("recorded");
    shelf
        .record_read(FIRST_PARTY, Ok(list(&[&own])), 1)
        .expect("recorded");
    let states: Vec<Availability> = shelf.view().available.iter().map(|a| a.state).collect();
    assert_eq!(
        states,
        [Availability::Install, Availability::FirstPartyOnly]
    );
    assert_eq!(
        shelf.install(OTHER, "network.dereth.xpmeter", &own),
        Err(PluginError::FirstPartyOnly("network.dereth.xpmeter".into()))
    );
    shelf
        .install(FIRST_PARTY, "network.dereth.xpmeter", &own)
        .expect("from Dereth's own source");
}

#[test]
fn an_installed_plugin_updates_only_from_the_source_it_came_from() {
    let dir = Scratch::new("one-source");
    let mut shelf = with_other_source(&dir);
    let second = "https://second.example.org/plugins.json";
    shelf.add_source(second).expect("added");
    let v1 = zip("org.example.meter", "1.0.0", "0.7", "");
    let v2 = zip("org.example.meter", "2.0.0", "0.7", "");
    shelf
        .record_read(OTHER, Ok(list(&[&v1])), 1)
        .expect("recorded");
    shelf
        .record_read(second, Ok(list(&[&v2])), 1)
        .expect("recorded");
    shelf
        .install(OTHER, "org.example.meter", &v1)
        .expect("installed");
    assert!(matches!(
        shelf.install(second, "org.example.meter", &v2),
        Err(PluginError::FromAnotherSource { .. })
    ));
    assert!(
        shelf.pending().is_empty(),
        "the newer release is another source's"
    );
    assert!(
        shelf.view().available.is_empty(),
        "neither source offers it again"
    );
    // Its source removed, it stays, with no updates.
    shelf.remove_source(OTHER).expect("removed");
    assert_eq!(shelf.offered_packages().len(), 1);
    let view = shelf.view();
    assert!(view.installed[0].source_removed);
    assert!(matches!(
        shelf.plan_update("org.example.meter"),
        Err(PluginError::NoSource(_))
    ));
}

#[test]
fn an_update_is_a_newer_release_and_the_setting_decides_which_apply_on_their_own() {
    let dir = Scratch::new("updates");
    let mut shelf = with_other_source(&dir);
    let mine = zip("network.dereth.xpmeter", "1.0.0", "0.7", "");
    let theirs = zip("org.example.meter", "1.0.0", "0.7", CHAT);
    shelf
        .record_read(FIRST_PARTY, Ok(list(&[&mine])), 1)
        .expect("recorded");
    shelf
        .record_read(OTHER, Ok(list(&[&theirs])), 1)
        .expect("recorded");
    shelf
        .install(FIRST_PARTY, "network.dereth.xpmeter", &mine)
        .expect("installed");
    shelf
        .install(OTHER, "org.example.meter", &theirs)
        .expect("installed");
    assert!(shelf.pending().is_empty());

    // Both sources move on; an older or equal release is never an update.
    let mine2 = zip("network.dereth.xpmeter", "1.1.0", "0.7", "");
    let theirs2 = zip("org.example.meter", "1.0.1", "0.7", CHAT);
    shelf
        .record_read(FIRST_PARTY, Ok(list(&[&mine2])), 2)
        .expect("recorded");
    shelf
        .record_read(OTHER, Ok(list(&[&theirs2])), 2)
        .expect("recorded");
    let pending = shelf.pending();
    assert_eq!(pending.len(), 2);
    assert_eq!(
        (pending[0].from.as_str(), pending[0].to.as_str()),
        ("1.0.0", "1.1.0")
    );

    let automatic = |shelf: &mut Shelf, setting| {
        shelf.set_auto_update(setting).expect("set");
        shelf.automatic_updates()
    };
    assert!(automatic(&mut shelf, AutoUpdate::None).is_empty());
    assert_eq!(
        automatic(&mut shelf, AutoUpdate::FirstParty),
        ["network.dereth.xpmeter"]
    );
    assert_eq!(
        automatic(&mut shelf, AutoUpdate::All),
        ["network.dereth.xpmeter", "org.example.meter"]
    );
    // The updates that wait are told, whatever the setting.
    shelf.set_auto_update(AutoUpdate::None).expect("set");
    assert_eq!(
        shelf.said_waiting().as_deref(),
        Some("2 plugin updates are waiting.")
    );
    let waits = shelf.view().installed[1].update.clone().expect("waiting");
    assert_eq!(
        (waits.released.as_str(), waits.size),
        ("2026-10-26", Some(theirs2.len() as u64))
    );

    // A release that asks for more always waits for the player, and says what it asks for.
    let theirs3 = zip("org.example.meter", "1.1.0", "0.7", CHAT_AND_LOOT);
    shelf
        .record_read(OTHER, Ok(list(&[&theirs3])), 3)
        .expect("recorded");
    shelf.set_auto_update(AutoUpdate::All).expect("set");
    let waits = shelf
        .pending()
        .into_iter()
        .find(|p| p.id == "org.example.meter")
        .expect("pending");
    assert_eq!(waits.new_permissions, ["auto.loot"]);
    assert!(!waits.automatic);
    assert_eq!(shelf.automatic_updates(), ["network.dereth.xpmeter"]);
    let domains = zip(
        "org.example.meter",
        "1.2.0",
        "0.7",
        &format!("{CHAT}[net]\ndomains = [\"example.org\"]\n"),
    );
    shelf
        .record_read(OTHER, Ok(list(&[&domains])), 4)
        .expect("recorded");
    let waits = shelf
        .pending()
        .into_iter()
        .find(|p| p.id == "org.example.meter")
        .expect("pending");
    assert_eq!(waits.new_domains, ["example.org"]);
    assert!(!waits.automatic);

    // Applied by hand from its own source: the update replaces the installed release.
    let plan = shelf.plan_update("org.example.meter").expect("a plan");
    assert!(plan.url.ends_with("/org.example.meter.zip"));
    let done = shelf
        .install(OTHER, "org.example.meter", &domains)
        .expect("updated");
    assert!(done.updated);
    assert_eq!(
        shelf
            .get("org.example.meter")
            .map(|p| p.release.version.as_str()),
        Some("1.2.0")
    );
    assert_eq!(
        std::fs::read(&shelf.offered_packages()[1]).expect("zip"),
        domains
    );
    // An update needing a newer Dereth is listed, never applied.
    let later = zip("network.dereth.xpmeter", "2.0.0", "0.99", "");
    shelf
        .record_read(FIRST_PARTY, Ok(list(&[&later])), 5)
        .expect("recorded");
    let p = shelf
        .pending()
        .into_iter()
        .find(|p| p.id == "network.dereth.xpmeter")
        .expect("pending");
    assert!(p.needs_newer_dereth && !p.automatic);
}

#[test]
fn the_update_setting_is_none_all_or_only_dereth_and_starts_at_only_dereth() {
    assert_eq!(AutoUpdate::default(), AutoUpdate::FirstParty);
    for (name, setting) in [
        ("none", AutoUpdate::None),
        ("all", AutoUpdate::All),
        ("first_party", AutoUpdate::FirstParty),
    ] {
        assert_eq!(AutoUpdate::parse(name), Some(setting));
        let dir = Scratch::new(&format!("setting-{name}"));
        let mut shelf = Shelf::open(&dir.0);
        shelf.set_auto_update(setting).expect("set");
        assert_eq!(Shelf::open(&dir.0).auto_update, setting, "kept: {name}");
    }
    // An earlier launcher's choices are gone.
    for gone in ["off", "notify", "Tell me", ""] {
        assert_eq!(AutoUpdate::parse(gone), None, "{gone}");
    }
}

#[test]
fn a_sources_file_holding_an_earlier_setting_reads_as_the_setting_at_first_and_keeps_its_sources() {
    for (tag, held) in [
        ("off", r#""off""#),
        ("notify", r#""notify""#),
        ("word", r#""sometimes""#),
        ("number", "3"),
        ("table", r#"{"kind": "all"}"#),
    ] {
        let dir = Scratch::new(&format!("earlier-{tag}"));
        std::fs::create_dir_all(&dir.0).expect("folder");
        std::fs::write(
            dir.0.join(SOURCES_FILE),
            format!(
                r#"{{"sources": [{{"url": "{OTHER}"}}], "auto_update": {held}, "others_allowed": true}}"#
            ),
        )
        .expect("an earlier file");
        let shelf = Shelf::open(&dir.0);
        assert_eq!(shelf.auto_update, AutoUpdate::FirstParty, "{held}");
        let urls: Vec<&str> = shelf.sources.iter().map(|s| s.url.as_str()).collect();
        assert_eq!(urls, [FIRST_PARTY, OTHER], "the sources stay: {held}");
    }
}

#[test]
fn a_plugin_updates_on_its_own_only_as_the_setting_says_whatever_an_earlier_launcher_kept_for_it() {
    let dir = Scratch::new("no-own-choice");
    let mut shelf = with_other_source(&dir);
    let mine = zip("network.dereth.xpmeter", "1.0.0", "0.7", "");
    let theirs = zip(
        "org.example.meter",
        "1.0.0",
        "0.7",
        "description = \"Counts.\"\n",
    );
    shelf
        .record_read(FIRST_PARTY, Ok(list(&[&mine])), 1)
        .expect("recorded");
    shelf
        .record_read(OTHER, Ok(list(&[&theirs])), 1)
        .expect("recorded");
    shelf
        .install(FIRST_PARTY, "network.dereth.xpmeter", &mine)
        .expect("installed");
    shelf
        .install(OTHER, "org.example.meter", &theirs)
        .expect("installed");
    // An earlier launcher kept a choice for each, never for Dereth's and always for the other,
    // and none of what the source said.
    let path = dir.0.join(INSTALLED_FILE);
    let mut record: serde_json::Value =
        serde_json::from_slice(&std::fs::read(&path).expect("the record")).expect("json");
    record[0]["auto_update"] = serde_json::json!(false);
    record[1]["auto_update"] = serde_json::json!(true);
    for p in record.as_array_mut().expect("a list") {
        let release = p["release"].as_object_mut().expect("a release");
        for said in [
            "description",
            "authors",
            "homepage",
            "released",
            "notes",
            "size",
        ] {
            release.remove(said);
        }
    }
    std::fs::write(&path, serde_json::to_vec(&record).expect("json")).expect("written");

    let mut shelf = Shelf::open(&dir.0);
    assert_eq!(shelf.plugins.len(), 2, "the record still reads");
    // What its source says of it stands in for what the record lacks.
    assert_eq!(shelf.view().installed[1].description, "Counts.");
    let mine2 = zip("network.dereth.xpmeter", "1.1.0", "0.7", "");
    let theirs2 = zip("org.example.meter", "1.1.0", "0.7", "");
    shelf
        .record_read(FIRST_PARTY, Ok(list(&[&mine2])), 2)
        .expect("recorded");
    shelf
        .record_read(OTHER, Ok(list(&[&theirs2])), 2)
        .expect("recorded");
    assert_eq!(shelf.auto_update, AutoUpdate::FirstParty);
    assert_eq!(shelf.automatic_updates(), ["network.dereth.xpmeter"]);
    shelf.set_auto_update(AutoUpdate::None).expect("set");
    assert!(shelf.automatic_updates().is_empty());
    // Written again, the record holds no choice of its own.
    shelf.remove("network.dereth.xpmeter").expect("removed");
    let kept = std::fs::read_to_string(&path).expect("the record");
    assert!(!kept.contains("auto_update"), "{kept}");
}

#[test]
fn available_leaves_out_every_installed_plugin_whichever_source_lists_it() {
    let dir = Scratch::new("available");
    let mut shelf = with_other_source(&dir);
    let meter = zip("org.example.meter", "1.0.0", "0.7", "");
    let newer = zip("org.example.meter", "2.0.0", "0.7", "");
    let other = zip("org.example.other", "1.0.0", "0.7", "");
    shelf
        .record_read(FIRST_PARTY, Ok(list(&[&newer])), 1)
        .expect("recorded");
    shelf
        .record_read(OTHER, Ok(list(&[&meter, &other])), 1)
        .expect("recorded");
    let listed = |shelf: &Shelf| -> Vec<(String, String)> {
        shelf
            .view()
            .available
            .iter()
            .map(|a| (a.source.clone(), a.id.clone()))
            .collect()
    };
    assert_eq!(listed(&shelf).len(), 3);
    shelf
        .install(OTHER, "org.example.meter", &meter)
        .expect("installed");
    assert_eq!(
        listed(&shelf),
        [(OTHER.to_owned(), "org.example.other".to_owned())],
        "neither its own source nor another offers it"
    );
    assert_eq!(shelf.view().installed[0].id, "org.example.meter");
    // Removed, both offer it again.
    shelf.remove("org.example.meter").expect("removed");
    assert_eq!(listed(&shelf).len(), 3);
}

#[test]
fn an_earlier_launchers_list_of_zip_installs_is_dropped() {
    let dir = Scratch::new("old");
    let old = dir.0.join("dev.example.old");
    std::fs::create_dir_all(&old).expect("folder");
    std::fs::write(old.join("current.zip"), b"zip").expect("zip");
    std::fs::write(
        dir.0.join("plugins.json"),
        br#"[{"id": "dev.example.old", "url": "https://example.org/old.zip", "current": null, "staged": null}]"#,
    )
    .expect("old list");
    let shelf = Shelf::open(&dir.0);
    assert!(shelf.plugins.is_empty());
    assert!(!dir.0.join("plugins.json").exists());
    assert!(!old.exists());
}

#[test]
fn another_program_reads_what_the_launcher_installed_and_changes_nothing_there() {
    let dir = Scratch::new("read-only");
    let mut shelf = with_other_source(&dir);
    let meter = zip("org.example.meter", "1.0.0", "0.7", CHAT);
    shelf
        .record_read(OTHER, Ok(list(&[&meter])), 1)
        .expect("recorded");
    shelf
        .install(OTHER, "org.example.meter", &meter)
        .expect("installed");
    let old = dir.0.join("dev.example.old");
    std::fs::create_dir_all(&old).expect("folder");
    std::fs::write(old.join("current.zip"), b"zip").expect("zip");
    std::fs::write(
        dir.0.join("plugins.json"),
        br#"[{"id": "dev.example.old"}]"#,
    )
    .expect("old list");
    let read = Shelf::read(&dir.0);
    assert_eq!(read.offered_packages().len(), 1);
    assert_eq!(read.offered_packages(), shelf.offered_packages());
    assert!(
        dir.0.join("plugins.json").is_file() && old.is_dir(),
        "an earlier launcher's list is the launcher's to drop"
    );
}

#[test]
fn a_read_names_the_plugins_its_source_lists_that_the_last_read_did_not() {
    let dir = Scratch::new("new-in-source");
    let mut shelf = with_other_source(&dir);
    let a = zip("org.example.alpha", "1.0.0", "0.7", "");
    let b = zip("org.example.beta", "1.0.0", "0.7", "");
    let a2 = zip("org.example.alpha", "1.1.0", "0.7", "");
    // The first read of a source names nothing new: everything is.
    assert_eq!(shelf.record_read(OTHER, Ok(list(&[&a])), 1), Ok(Vec::new()));
    // A newer release of a listed plugin is an update, not a new plugin.
    assert_eq!(
        shelf.record_read(OTHER, Ok(list(&[&a2, &b])), 2),
        Ok(vec!["Meter".to_owned()])
    );
    assert_eq!(
        shelf.record_read(OTHER, Ok(list(&[&a2, &b])), 3),
        Ok(Vec::new())
    );
    assert_eq!(
        shelf.record_read(OTHER, Err("offline".into()), 4),
        Ok(Vec::new())
    );
}

/// A keep in memory: the records and the zips by name, as another host than the launcher keeps
/// them.
#[derive(Debug, Default)]
struct Memory {
    records: std::collections::BTreeMap<String, Vec<u8>>,
    zips: std::collections::BTreeMap<String, Vec<u8>>,
}

impl Keep for Memory {
    fn read(&self, name: &str) -> Option<Vec<u8>> {
        self.records.get(name).cloned()
    }
    fn write(&mut self, name: &str, bytes: &[u8]) -> Result<(), PluginError> {
        self.records.insert(name.to_owned(), bytes.to_vec());
        Ok(())
    }
    fn put_zip(&mut self, id: &str, zip: &[u8]) -> Result<(), PluginError> {
        self.zips.insert(id.to_owned(), zip.to_vec());
        Ok(())
    }
    fn remove_zip(&mut self, id: &str) -> Result<(), PluginError> {
        self.zips.remove(id);
        Ok(())
    }
}

#[test]
fn a_shelf_kept_elsewhere_than_a_folder_keeps_and_checks_as_the_launchers() {
    let mut shelf = Shelf::load(Memory::default());
    shelf.add_source(OTHER).expect("added");
    let meter = zip("org.example.meter", "1.0.0", "0.7", CHAT);
    shelf
        .record_read(OTHER, Ok(list(&[&meter])), 1)
        .expect("recorded");
    let other = zip("org.example.meter", "1.0.0", "0.7", CHAT_AND_LOOT);
    assert!(matches!(
        shelf.install(OTHER, "org.example.meter", &other),
        Err(PluginError::Mismatch { .. })
    ));
    assert!(shelf.keep.zips.is_empty(), "nothing kept");
    shelf
        .install(OTHER, "org.example.meter", &meter)
        .expect("installed");
    assert_eq!(shelf.keep.zips.get("org.example.meter"), Some(&meter));
    // Loaded again from what it kept: the same sources, list and installs.
    let Memory { records, zips } = std::mem::take(&mut shelf.keep);
    let mut again = Shelf::load(Memory { records, zips });
    let urls: Vec<&str> = again.sources.iter().map(|s| s.url.as_str()).collect();
    assert_eq!(urls, [FIRST_PARTY, OTHER]);
    assert_eq!(again.plugins, shelf.plugins);
    assert_eq!(again.view().installed[0].id, "org.example.meter");
    again.remove("org.example.meter").expect("removed");
    assert!(again.keep.zips.is_empty());
    assert!(Shelf::load(std::mem::take(&mut again.keep))
        .plugins
        .is_empty());
}

#[test]
fn a_refresh_reads_every_source_then_applies_the_updates_that_apply_on_their_own() {
    let mut shelf = Shelf::load(Memory::default());
    shelf.add_source(OTHER).expect("added");
    let mine = zip("network.dereth.xpmeter", "1.0.0", "0.7", "");
    let theirs = zip("org.example.meter", "1.0.0", "0.7", CHAT);
    shelf
        .record_read(FIRST_PARTY, Ok(list(&[&mine])), 1)
        .expect("recorded");
    shelf
        .record_read(OTHER, Ok(list(&[&theirs])), 1)
        .expect("recorded");
    shelf
        .install(FIRST_PARTY, "network.dereth.xpmeter", &mine)
        .expect("installed");
    shelf
        .install(OTHER, "org.example.meter", &theirs)
        .expect("installed");
    shelf.set_auto_update(AutoUpdate::All).expect("set");

    // Dereth's source moves on; the other cannot be read this time, which holds nothing up.
    let mine2 = zip("network.dereth.xpmeter", "1.1.0", "0.7", "");
    let fresh = zip("network.dereth.fresh", "1.0.0", "0.7", "");
    let served = |url: &str| -> Result<Vec<u8>, String> {
        match url {
            FIRST_PARTY => Ok(list(&[&mine2, &fresh])),
            "https://plugins.dereth.network/network.dereth.xpmeter.zip" => Ok(mine2.clone()),
            _ => Err("offline".to_owned()),
        }
    };
    let mut walk = Refresh::new(&shelf);
    let mut asked = Vec::new();
    let mut fetched = None;
    while let Some(d) = walk.next(&mut shelf, fetched.take(), 2) {
        fetched = Some(served(&d.url));
        asked.push(d.url);
    }
    assert_eq!(
        asked,
        [
            FIRST_PARTY,
            OTHER,
            "https://plugins.dereth.network/network.dereth.xpmeter.zip"
        ]
    );
    assert_eq!(
        walk.said(&shelf),
        "New from Dereth: Meter.
example.github.io: offline
Updated Meter to 1.1.0."
    );
    assert_eq!(
        shelf
            .get("network.dereth.xpmeter")
            .map(|p| p.release.version.as_str()),
        Some("1.1.0")
    );
    // The source that could not be read keeps its last list, and its plugin stays installed.
    assert_eq!(
        shelf.source(OTHER).map(|s| s.catalog.plugins.len()),
        Some(1)
    );
    assert_eq!(shelf.plugins.len(), 2);

    // A release that asks for more waits, and the walk says so; nothing more is downloaded.
    let theirs2 = zip("org.example.meter", "1.1.0", "0.7", CHAT_AND_LOOT);
    let mut walk = Refresh::new(&shelf);
    let mut asked = Vec::new();
    let mut fetched = None;
    while let Some(d) = walk.next(&mut shelf, fetched.take(), 3) {
        fetched = Some(match d.url.as_str() {
            FIRST_PARTY => Ok(list(&[&mine2, &fresh])),
            OTHER => Ok(list(&[&theirs2])),
            _ => Err("not asked for".to_owned()),
        });
        asked.push(d.url);
    }
    assert_eq!(asked, [FIRST_PARTY, OTHER]);
    assert_eq!(walk.said(&shelf), "1 plugin update is waiting.");
}

// ----- folders on this computer -------------------------------------------------------------------

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

#[test]
fn the_folders_on_this_computer_are_kept_beside_the_sources_and_an_older_file_has_none() {
    let dir = Scratch::new("local-kept");
    let mine = dir.0.join("mine");
    write_plugin_folder(&mine.join("hello"), "org.example.hello", "Hello", "0.1.0");
    // An older launcher's file: sources and a setting, no folders.
    let path = dir.0.join(SOURCES_FILE);
    std::fs::write(
        &path,
        format!(r#"{{"sources": [{{"url": "{OTHER}"}}], "auto_update": "all"}}"#),
    )
    .expect("an older file");
    let mut shelf = Shelf::open(&dir.0);
    assert!(shelf.local.is_empty());
    assert!(shelf.view().local.is_empty());
    assert_eq!(shelf.add_local(&mine), Ok(1));
    // Read again: the sources, the setting and the folder.
    let mut again = Shelf::open(&dir.0);
    let urls: Vec<&str> = again.sources.iter().map(|s| s.url.as_str()).collect();
    assert_eq!(urls, [FIRST_PARTY, OTHER]);
    assert_eq!(again.auto_update, AutoUpdate::All);
    assert_eq!(again.local, std::slice::from_ref(&mine));
    // A source added later keeps the folder.
    again
        .add_source("https://plugins.example.net/list.json")
        .expect("added");
    assert_eq!(Shelf::read(&dir.0).local, [mine]);
}

#[test]
fn a_repository_on_the_list_offers_each_plugin_in_it_as_it_is_on_disk() {
    let dir = Scratch::new("local-repo");
    let repo = dir.0.join("repo");
    write_plugin_folder(&repo.join("hello"), "org.example.hello", "Hello", "0.1.0");
    let meter = repo.join("meter");
    write_plugin_folder(&meter, "org.example.meter", "Meter", "0.2.0");
    let deeper = repo.join("tools").join("deeper");
    write_plugin_folder(&deeper, "org.example.deeper", "Deeper", "0.1.0");
    let built = repo.join("target");
    write_plugin_folder(&built, "org.example.built", "Built", "0.1.0");
    let single = dir.0.join("single");
    write_plugin_folder(&single, "org.example.single", "Single", "1.0.0");
    let empty = dir.0.join("empty");
    std::fs::create_dir_all(&empty).expect("folder");

    let mut shelf = Shelf::open(dir.0.join("shelf"));
    assert_eq!(
        shelf.add_local(&repo),
        Ok(2),
        "the folders directly in it: not build output, nothing further down"
    );
    assert_eq!(shelf.add_local(&single), Ok(1), "one plugin's own folder");
    assert_eq!(
        shelf.add_local(&empty),
        Err(PluginError::NoPluginIn(empty.display().to_string()))
    );
    assert_eq!(
        shelf.add_local(&repo),
        Err(PluginError::FolderListed(repo.display().to_string()))
    );
    assert_eq!(
        shelf.offered_packages(),
        [repo.join("hello"), meter, single.clone()],
        "the folders themselves: no zip"
    );
    let view = shelf.view();
    let named: Vec<(&str, &str, &str)> = view
        .local
        .iter()
        .map(|l| (l.name.as_str(), l.version.as_str(), l.description.as_str()))
        .collect();
    assert_eq!(
        named,
        [
            ("Hello", "0.1.0", "Says hi"),
            ("Meter", "0.2.0", "Says hi"),
            ("Single", "1.0.0", "Says hi")
        ]
    );
    assert!(view
        .local
        .iter()
        .all(|l| l.problem.is_none() && !l.id_taken && !l.needs_newer_dereth));
    assert_eq!(view.local[1].listed, repo.display().to_string());
    assert!(shelf.plugins.is_empty(), "nothing is installed");
    // An edit on disk is what is offered next: nothing is copied.
    write_plugin_folder(&single, "org.example.single", "Single", "1.1.0");
    assert_eq!(shelf.view().local[2].version, "1.1.0");
    // A refresh reads the sources alone: these are never updated.
    let mut walk = Refresh::new(&shelf);
    let mut asked = Vec::new();
    while let Some(d) = walk.next(&mut shelf, None, 2) {
        asked.push(d.url);
    }
    assert_eq!(asked, [FIRST_PARTY]);
}

/// A folder is added only when it is a plugin's own folder or a plugin repository, as the author's
/// tool has a repository: a folder whose plugins are all further down than the folders directly
/// in it is refused and not listed. A folder on the list whose plugins move further down says it no
/// longer holds a plugin, and offers nothing.
#[test]
fn a_folder_whose_plugins_are_all_further_down_is_refused() {
    let dir = Scratch::new("local-deep");
    let code = dir.0.join("code");
    let plugins = code.join("plugins");
    let hello = plugins.join("hello");
    write_plugin_folder(&hello, "org.example.hello", "Hello", "0.1.0");
    let mut shelf = Shelf::open(dir.0.join("shelf"));
    let refused = shelf.add_local(&code);
    assert_eq!(
        refused,
        Err(PluginError::NoPluginIn(code.display().to_string()))
    );
    assert_eq!(
        refused.unwrap_err().to_string(),
        format!(
            "{} has no plugin.toml, and neither do the folders in it",
            code.display()
        )
    );
    assert!(shelf.local.is_empty(), "not listed");
    assert!(Shelf::read(dir.0.join("shelf")).local.is_empty());
    assert_eq!(shelf.add_local(&plugins), Ok(1), "a plugin repository");
    assert_eq!(shelf.add_local(&hello), Ok(1), "a plugin's own folder");

    // Moved a folder further down, the plugin is not the repository's any more.
    let more = plugins.join("more");
    std::fs::create_dir_all(&more).expect("a folder");
    std::fs::rename(&hello, more.join("hello")).expect("moved");
    let view = shelf.view();
    assert_eq!(
        view.local[0].problem.as_deref(),
        Some("This folder no longer holds a plugin.")
    );
    assert_eq!(
        view.local[1].problem.as_deref(),
        Some("This folder is not there any more.")
    );
    assert!(shelf.offered_packages().is_empty());
}

/// A plugin on this computer with an installed plugin's id is offered in its place, Dereth's own
/// ids too: the rule that they come only from Dereth's source is about downloads. The installed
/// one stays, and keeps updating from its source.
#[test]
fn a_plugin_on_this_computer_is_loaded_in_place_of_the_installed_one_which_still_updates() {
    let dir = Scratch::new("local-in-place");
    let id = "network.dereth.xpmeter";
    let mut shelf = Shelf::open(dir.0.join("shelf"));
    let v1 = zip(id, "1.0.0", "0.7", "");
    let other = zip("org.example.other", "1.0.0", "0.7", "");
    shelf
        .record_read(FIRST_PARTY, Ok(list(&[&v1, &other])), 1)
        .expect("recorded");
    shelf.install(FIRST_PARTY, id, &v1).expect("installed");
    shelf
        .install(FIRST_PARTY, "org.example.other", &other)
        .expect("installed");
    let mine = dir.0.join("xpmeter");
    write_plugin_folder(&mine, id, "XP Meter", "1.1.0-dev");
    shelf.add_local(&mine).expect("added");

    let installed_zip = shelf.plugin_dir(id).join("current.zip");
    let offered = shelf.offered_packages();
    assert_eq!(offered[0], mine, "the one on this computer first");
    assert!(!offered.contains(&installed_zip), "{offered:?}");
    assert_eq!(offered.len(), 2, "and the other installed plugin");
    let view = shelf.view();
    let in_place = |v: &super::PluginsView, id: &str| {
        v.installed
            .iter()
            .find(|p| p.id == id)
            .is_some_and(|p| p.local_in_its_place)
    };
    assert!(in_place(&view, id));
    assert!(!in_place(&view, "org.example.other"));
    assert!(view.local[0].in_place_of_installed);

    // The installed one updates as before, and is still not the one offered.
    let v2 = zip(id, "1.2.0", "0.7", "");
    shelf
        .record_read(FIRST_PARTY, Ok(list(&[&v2, &other])), 2)
        .expect("recorded");
    assert_eq!(shelf.automatic_updates(), [id]);
    shelf.update(id, &v2).expect("updated");
    assert_eq!(
        shelf.get(id).map(|p| p.release.version.as_str()),
        Some("1.2.0")
    );
    assert!(!shelf.offered_packages().contains(&installed_zip));

    // Off the list, the installed one is offered again.
    shelf.remove_local(&mine).expect("removed");
    assert!(shelf.offered_packages().contains(&installed_zip));
    assert!(!in_place(&shelf.view(), id));
}

#[test]
fn of_two_plugins_on_this_computer_with_one_id_the_first_on_the_list_is_loaded() {
    let dir = Scratch::new("local-twice");
    let (a, b) = (dir.0.join("a"), dir.0.join("b"));
    write_plugin_folder(&a, "org.example.hello", "Hello", "0.1.0");
    write_plugin_folder(&b, "org.example.hello", "Hello", "0.2.0");
    let mut shelf = Shelf::open(dir.0.join("shelf"));
    shelf.add_local(&a).expect("added");
    shelf.add_local(&b).expect("added");
    // A plugin under two folders on the list is listed once.
    shelf.add_local(&dir.0).expect("added");
    assert_eq!(shelf.offered_packages(), [a]);
    let taken: Vec<bool> = shelf.view().local.iter().map(|l| l.id_taken).collect();
    assert_eq!(taken, [false, true, false]);
    assert_eq!(
        shelf.view().local[2].problem.as_deref(),
        Some("Its plugins are listed under another folder on the list.")
    );
}

#[test]
fn taking_a_folder_off_the_list_deletes_nothing_and_a_folder_with_no_plugin_says_so() {
    let dir = Scratch::new("local-remove");
    let mine = dir.0.join("mine");
    write_plugin_folder(&mine, "org.example.hello", "Hello", "0.1.0");
    std::fs::write(mine.join("notes.txt"), "mine").expect("a file");
    let mut shelf = Shelf::open(dir.0.join("shelf"));
    shelf.add_local(&mine).expect("added");
    let manifest = std::fs::read(mine.join("plugin.toml")).expect("its manifest");

    // The plugin goes from the folder: its row says so, and nothing is offered.
    std::fs::remove_file(mine.join("plugin.toml")).expect("removed");
    let view = shelf.view();
    assert_eq!(view.local.len(), 1);
    assert_eq!(view.local[0].name, "mine");
    assert_eq!(
        view.local[0].problem.as_deref(),
        Some("This folder no longer holds a plugin.")
    );
    assert!(shelf.offered_packages().is_empty());
    let gone = dir.0.join("gone");
    write_plugin_folder(&gone, "org.example.gone", "Gone", "0.1.0");
    shelf.add_local(&gone).expect("added");
    std::fs::remove_dir_all(&gone).expect("removed");
    assert_eq!(
        shelf.view().local[1].problem.as_deref(),
        Some("This folder is not there any more.")
    );
    // A manifest that does not read says why, and is still offered: the client says so too.
    let broken = dir.0.join("broken");
    write_plugin_folder(&broken, "org.example.broken", "Broken", "0.1.0");
    shelf.add_local(&broken).expect("added");
    std::fs::write(broken.join("plugin.toml"), "id = 1\n").expect("broken");
    let view = shelf.view();
    assert!(
        view.local[2]
            .problem
            .as_deref()
            .is_some_and(|p| p.starts_with("Its plugin.toml does not read")),
        "{:?}",
        view.local[2]
    );
    assert_eq!(shelf.offered_packages(), std::slice::from_ref(&broken));

    std::fs::write(mine.join("plugin.toml"), &manifest).expect("put back");
    let before = files_under(&mine);
    let broken_before = files_under(&broken);
    for f in [&mine, &broken] {
        shelf.remove_local(f).expect("taken off");
    }
    assert_eq!(
        shelf.remove_local(&mine),
        Err(PluginError::FolderNotListed(mine.display().to_string()))
    );
    assert_eq!(files_under(&mine), before, "nothing in it is touched");
    assert_eq!(files_under(&broken), broken_before);
    assert_eq!(Shelf::read(dir.0.join("shelf")).local, [gone]);
}

/// At start a source that cannot be read says so on its row alone: what is said keeps what is new
/// and the updates applied and waiting. Refresh still says everything.
#[test]
fn at_start_a_source_that_cannot_be_read_is_left_out_of_what_is_said() {
    let mut shelf = Shelf::load(Memory::default());
    shelf.add_source(OTHER).expect("added");
    let mine = zip("network.dereth.xpmeter", "1.0.0", "0.7", "");
    shelf
        .record_read(FIRST_PARTY, Ok(list(&[&mine])), 1)
        .expect("recorded");
    shelf
        .install(FIRST_PARTY, "network.dereth.xpmeter", &mine)
        .expect("installed");
    let walk = |shelf: &mut Shelf<Memory>, served: &dyn Fn(&str) -> Result<Vec<u8>, String>| {
        let mut walk = Refresh::new(shelf);
        let mut fetched = None;
        while let Some(d) = walk.next(shelf, fetched.take(), 2) {
            fetched = Some(served(&d.url));
        }
        walk
    };
    let offline = |url: &str| Err(format!("{url} could not be reached"));
    let w = walk(&mut shelf, &offline);
    assert_eq!(w.clone().said_at_start(&shelf), "");
    let said = w.said(&shelf);
    assert!(
        said.contains("Dereth: ") && said.contains("example.github.io: "),
        "{said}"
    );
    assert!(
        shelf.view().sources.iter().all(|s| s.error.is_some()),
        "each source's row says so"
    );

    // Dereth's source moves on while the other stays out of reach: the update is told.
    let mine2 = zip("network.dereth.xpmeter", "1.1.0", "0.7", "");
    let half = |url: &str| match url {
        FIRST_PARTY => Ok(list(&[&mine2])),
        "https://plugins.dereth.network/network.dereth.xpmeter.zip" => Ok(mine2.clone()),
        _ => Err(format!("{url} could not be reached")),
    };
    let w = walk(&mut shelf, &half);
    assert_eq!(w.said_at_start(&shelf), "Updated Meter to 1.1.0.");
}

/// A folder Make Your Own wrote in is kept as one the launcher keeps the author's tool in, beside
/// the folders on this computer and read back with them; a folder added any other way is not,
/// an older file keeps it in none, and taking a folder off the list stops it.
#[test]
fn make_your_owns_folders_keep_the_tool_and_others_and_an_older_files_do_not() {
    let dir = Scratch::new("tool-kept");
    let (mine, given) = (dir.0.join("mine"), dir.0.join("given"));
    write_plugin_folder(&mine.join("hello"), "org.example.hello", "Hello", "0.1.0");
    write_plugin_folder(&given.join("meter"), "org.example.meter", "Meter", "0.1.0");
    // An older launcher's file: a folder, and nothing said of the tool.
    std::fs::create_dir_all(&dir.0).expect("a folder");
    let mine_json = serde_json::to_string(&mine).expect("a path");
    std::fs::write(
        dir.0.join(SOURCES_FILE),
        format!(r#"{{"sources": [], "local": [{mine_json}]}}"#),
    )
    .expect("an older file");
    let mut shelf = Shelf::open(&dir.0);
    assert_eq!(shelf.local, std::slice::from_ref(&mine));
    assert!(
        shelf.tool_folders.is_empty(),
        "an older file keeps it in none"
    );

    // Make Your Own's folder, on the list already: kept, and listed once.
    assert_eq!(shelf.add_own(&mine), Ok(1));
    assert_eq!(shelf.local, std::slice::from_ref(&mine));
    assert_eq!(shelf.tool_folders, std::slice::from_ref(&mine));
    shelf.add_own(&mine).expect("again");
    assert_eq!(shelf.tool_folders, std::slice::from_ref(&mine), "once");
    // A folder added with Add Plugin Folder is not.
    shelf.add_local(&given).expect("added");
    let again = Shelf::read(&dir.0);
    assert_eq!(again.local, [mine.clone(), given.clone()]);
    assert_eq!(again.tool_folders, std::slice::from_ref(&mine));
    // The file still names each folder on the list as a path, as an older launcher reads it.
    let text = std::fs::read_to_string(dir.0.join(SOURCES_FILE)).expect("the file");
    let file: serde_json::Value = serde_json::from_str(&text).expect("JSON");
    assert!(file["local"]
        .as_array()
        .expect("a list")
        .iter()
        .all(serde_json::Value::is_string));
    // A folder holding no plugin is refused, and not listed.
    let empty = dir.0.join("empty");
    std::fs::create_dir_all(&empty).expect("a folder");
    assert!(shelf.add_own(&empty).is_err());
    assert_eq!(shelf.local.len(), 2);

    // Off the list, it is no longer kept, there or in the file.
    shelf.tool_failed.push(mine.clone());
    shelf.remove_local(&mine).expect("removed");
    assert!(shelf.tool_folders.is_empty() && shelf.tool_failed.is_empty());
    assert!(Shelf::read(&dir.0).tool_folders.is_empty());
    let text = std::fs::read_to_string(dir.0.join(SOURCES_FILE)).expect("the file");
    assert!(!text.contains("tool_folders"), "{text}");
}

/// The tool's copy is written at the repository's root, executable, when it is not the tool,
/// and left as it is when it is; the `.gitignore` gets the lines that keep it out of git once,
/// and only those it lacks, in its own line endings.
#[test]
fn the_tools_copy_is_written_when_it_is_not_the_tool_and_ignored_by_git_once() {
    let dir = Scratch::new("tool-copy");
    let repo = dir.0.join("repo");
    std::fs::create_dir_all(&repo).expect("a folder");
    let copy = repo.join(super::TOOL_FILE);
    let gitignore = repo.join(".gitignore");

    // None there: written, and a .gitignore made with both lines.
    assert!(super::keep_tool(&repo, super::TOOL_FILE, b"tool 1").expect("kept"));
    assert_eq!(std::fs::read(&copy).expect("the copy"), b"tool 1");
    assert_eq!(
        std::fs::read_to_string(&gitignore).expect("made"),
        "/dereth-plugin\n/dereth-plugin.exe\n"
    );
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        let mode = std::fs::metadata(&copy)
            .expect("the copy")
            .permissions()
            .mode();
        assert_eq!(mode & 0o111, 0o111, "executable");
    }
    // The tool again: nothing written.
    std::fs::write(&gitignore, "/dist/\n").expect("the author's own");
    assert!(!super::keep_tool(&repo, super::TOOL_FILE, b"tool 1").expect("kept"));
    assert_eq!(
        std::fs::read_to_string(&gitignore).expect("left"),
        "/dist/\n"
    );
    // Another release's tool: the copy replaced, and only the lines it lacks added, once.
    std::fs::write(&gitignore, "/dist/\r\ndereth-plugin\r\n# mine").expect("the author's own");
    assert!(super::keep_tool(&repo, super::TOOL_FILE, b"tool 2").expect("kept"));
    assert_eq!(std::fs::read(&copy).expect("the copy"), b"tool 2");
    let lines = "/dist/\r\ndereth-plugin\r\n# mine\r\n/dereth-plugin.exe\r\n";
    assert_eq!(
        std::fs::read_to_string(&gitignore).expect("added to"),
        lines
    );
    assert!(super::keep_tool(&repo, super::TOOL_FILE, b"tool 3").expect("kept"));
    assert_eq!(std::fs::read_to_string(&gitignore).expect("once"), lines);
    let names: Vec<_> = std::fs::read_dir(&repo)
        .expect("read")
        .map(|e| e.expect("an entry").file_name())
        .collect();
    assert_eq!(names.len(), 2, "nothing else left there: {names:?}");

    // A copy that cannot be written is an error, with nothing left half written.
    let blocked = dir.0.join("blocked");
    std::fs::create_dir_all(blocked.join(super::TOOL_FILE)).expect("a folder in its place");
    assert!(super::keep_tool(&blocked, super::TOOL_FILE, b"tool 3").is_err());
    let names: Vec<_> = std::fs::read_dir(&blocked)
        .expect("read")
        .map(|e| e.expect("an entry").file_name())
        .collect();
    assert_eq!(names, [super::TOOL_FILE]);
}

/// A folder whose copy could not be brought up to date says so on its first row, once.
#[test]
fn a_folder_whose_copy_could_not_be_written_says_so_on_its_first_row() {
    let dir = Scratch::new("tool-row");
    let repo = dir.0.join("repo");
    write_plugin_folder(&repo.join("a"), "org.example.a", "A", "0.1.0");
    write_plugin_folder(&repo.join("b"), "org.example.b", "B", "0.1.0");
    let mut shelf = Shelf::open(dir.0.join("shelf"));
    shelf.add_own(&repo).expect("added");
    let said = |shelf: &Shelf| -> Vec<Option<String>> {
        shelf
            .view()
            .local
            .into_iter()
            .map(|l| l.tool_problem)
            .collect()
    };
    assert_eq!(said(&shelf), [None, None]);
    shelf.tool_failed.push(repo.clone());
    assert_eq!(
        said(&shelf),
        [
            Some("Could not update dereth-plugin in this folder.".to_owned()),
            None
        ]
    );
}
