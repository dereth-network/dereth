//! Behaviour: none (the launcher's own plumbing: plugin sources, installing and updating from
//! them, checked over bytes served by the test).

use super::{
    AutoUpdate, Availability, Keep, PluginError, Refresh, Shelf, FIRST_PARTY, INSTALLED_FILE,
    SOURCES_FILE,
};
use crate::testing::{plugin_list as list, plugin_zip as zip};
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
