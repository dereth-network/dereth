//! The author's tool over temporary folders: the template makes a plugin that checks, packs,
//! releases and is listed; the change log, the version and the list are edited as the
//! documentation says.

use std::path::{Path, PathBuf};
use std::process::Command;

use crate::{list, release, template};

/// A folder of its own for each test, removed when it ends.
struct Scratch(PathBuf);

impl Scratch {
    fn new(tag: &str) -> Self {
        let d =
            std::env::temp_dir().join(format!("dereth-plugin-tool-{tag}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&d);
        std::fs::create_dir_all(&d).expect("a scratch folder");
        Self(d)
    }
}

impl Drop for Scratch {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.0);
    }
}

fn git(dir: &Path, args: &[&str]) -> String {
    let out = Command::new("git")
        .current_dir(dir)
        .args(args)
        .output()
        .expect("git runs");
    assert!(
        out.status.success(),
        "git {args:?}: {}",
        String::from_utf8_lossy(&out.stderr)
    );
    String::from_utf8_lossy(&out.stdout).trim().to_owned()
}

/// A git repository in `dir` with an identity of its own, and no signing or hooks from the
/// machine's settings.
fn git_init(dir: &Path) {
    git(dir, &["init", "--quiet"]);
    for (k, v) in [
        ("user.name", "Author"),
        ("user.email", "author@example.org"),
        ("commit.gpgsign", "false"),
        ("tag.gpgsign", "false"),
        ("core.autocrlf", "false"),
        ("core.hooksPath", ".git/no-hooks"),
    ] {
        git(dir, &["config", k, v]);
    }
}

fn options<'a>(github: Option<&'a str>) -> template::Options<'a> {
    template::Options {
        id: None,
        name: Some("XP Tracker"),
        github,
    }
}

#[test]
fn a_new_repository_holds_a_plugin_that_checks_packs_releases_and_is_listed() {
    let dir = Scratch::new("new");
    let repo = dir.0.join("xp-tracker");
    let said = template::new(&repo, &options(Some("Someone/xp-tracker"))).expect("made");
    assert!(said.contains("xp-tracker/"), "{said}");
    let folder = repo.join("xp-tracker");
    let manifest = std::fs::read_to_string(folder.join("plugin.toml")).expect("plugin.toml");
    assert!(
        manifest.contains("id = \"io.github.someone.xp-tracker\""),
        "{manifest}"
    );
    assert!(manifest.contains("homepage = \"https://github.com/Someone/xp-tracker\""));
    let readme = std::fs::read_to_string(repo.join("README.md")).expect("README");
    assert!(
        readme.contains("https://someone.github.io/xp-tracker/plugins.json"),
        "{readme}"
    );
    // Every placeholder is filled.
    for (path, text) in
        template::render(&repo, &options(Some("Someone/xp-tracker"))).expect("rendered")
    {
        let left = text
            .match_indices("{{")
            .map(|(i, _)| &text[i..])
            .find(|rest| rest[2..].starts_with(|c: char| c.is_ascii_alphabetic()));
        assert!(left.is_none(), "{path} keeps a placeholder: {left:?}");
    }
    assert!(
        template::new(&repo, &options(None)).is_err(),
        "not into a folder that holds files"
    );

    // It checks, from the repository's root too, and packs with its SHA-256 beside the zip.
    assert!(crate::check::check(&folder)
        .expect("checks")
        .contains("io.github.someone.xp-tracker 0.1.0: ok"));
    assert!(crate::check::check(&repo).is_ok());
    let zip = dir.0.join("x.zip");
    crate::pack(&folder, &zip).expect("packed");
    let bytes = std::fs::read(&zip).expect("the zip");
    let sum = std::fs::read_to_string(dir.0.join("x.zip.sha256")).expect("the sum");
    assert_eq!(sum, format!("{}  x.zip\n", crate::sha256_hex(&bytes)));
    // Packing is the same bytes every time, so a release can be checked against its source.
    crate::pack(&folder, &dir.0.join("y.zip")).expect("packed");
    assert_eq!(std::fs::read(dir.0.join("y.zip")).expect("the zip"), bytes);

    // Released: the version set, the notes moved, a commit and the tag of a one-plugin repository.
    git_init(&repo);
    git(&repo, &["add", "--all"]);
    git(&repo, &["commit", "--quiet", "-m", "start"]);
    let said = release::release(&folder, "0.1.0", "2026-10-26", false).expect("released");
    assert!(
        said.contains("tagged v0.1.0") && said.contains("git push origin v0.1.0"),
        "{said}"
    );
    assert_eq!(
        git(&repo, &["log", "-1", "--format=%s"]),
        "XP Tracker 0.1.0"
    );
    assert_eq!(git(&repo, &["tag", "--list"]), "v0.1.0");
    assert_eq!(
        release::notes(&folder, "0.1.0").expect("notes"),
        "- First release."
    );
    assert!(
        release::release(&folder, "0.1.0", "2026-10-26", false).is_err(),
        "tagged already"
    );
    assert!(
        release::release(&folder, "0.0.9", "2026-10-26", false).is_err(),
        "not newer"
    );
    std::fs::write(folder.join("main.lua"), "-- changed\n").expect("edit");
    let dirty = release::release(&folder, "0.2.0", "2026-10-26", false).unwrap_err();
    assert!(dirty.contains("uncommitted"), "{dirty}");

    // The release workflow finds the plugin by its tag, and lists the packed zip.
    let found = release::find(&repo, "v0.1.0").expect("found");
    let fields: Vec<&str> = found.split('\t').collect();
    assert_eq!(
        fields[1..],
        ["io.github.someone.xp-tracker", "0.1.0", "XP Tracker"]
    );
    assert!(fields[0].ends_with("xp-tracker/xp-tracker"), "{found}");
    assert!(
        release::find(&repo, "v0.2.0").is_err(),
        "the tag must name the version it has"
    );
    git(&repo, &["checkout", "--quiet", "--", "."]);
    crate::pack(&folder, &zip).expect("packed");
    let site = dir.0.join("plugins.json");
    let said = list::add(
        &site,
        &zip,
        "io.github.someone.xp-tracker/x.zip",
        "- First release.",
        Some("Someone's plugins"),
        "2026-10-26",
    )
    .expect("listed");
    assert_eq!(said, "listed io.github.someone.xp-tracker 0.1.0");
    let read = dereth_plugin_package::catalog::Catalog::parse(&std::fs::read(&site).expect("list"))
        .expect("reads");
    assert_eq!(read.catalog.name, "Someone's plugins");
    let e = &read.catalog.plugins[0];
    assert_eq!(
        (e.zip.as_str(), e.notes.as_str()),
        ("io.github.someone.xp-tracker/x.zip", "- First release.")
    );
    assert!(e.check(&std::fs::read(&zip).expect("zip")).is_ok());
}

#[test]
fn the_templates_workflows_run_only_this_tools_commands_and_pin_their_actions() {
    const COMMANDS: &[&str] = &[
        "new", "check", "pack", "hash", "build", "release", "notes", "find", "add",
    ];
    for (path, text) in template::FILES {
        for (at, _) in text.match_indices("dereth-plugin ") {
            let word: String = text[at + 14..]
                .chars()
                .take_while(|c| c.is_ascii_lowercase())
                .collect();
            if !word.is_empty() {
                assert!(
                    COMMANDS.contains(&word.as_str()),
                    "{path} runs dereth-plugin {word}"
                );
                assert!(
                    crate::USAGE.contains(&format!("dereth-plugin {word}")),
                    "{word}"
                );
            }
        }
        for line in text.lines().filter(|l| {
            l.trim_start().starts_with("- uses:") || l.trim_start().starts_with("uses:")
        }) {
            let pin = line.split('@').nth(1).unwrap_or_default();
            let sha: String = pin.chars().take_while(char::is_ascii_hexdigit).collect();
            assert_eq!(
                sha.len(),
                40,
                "{path}: an action is pinned by its commit: {line}"
            );
        }
    }
    // Each workflow installs the tool of its pinned Dereth release from that release, checked
    // against the release's SHA256SUMS; none builds it.
    for (path, text) in template::FILES.iter().filter(|(p, _)| p.ends_with(".yml")) {
        let filled = template::fill(
            text,
            &template::values(Path::new("x"), &options(None)).expect("values"),
        );
        assert!(
            filled.contains("/releases/download/dereth-v$DERETH")
                && filled.contains("dereth-plugin-$DERETH-linux-x86_64.tar.gz")
                && filled.contains("SHA256SUMS | sha256sum -c -"),
            "{path}"
        );
        assert!(
            !filled.contains("cargo install") && !filled.contains("rustup"),
            "{path}"
        );
    }
    let release = template::FILES
        .iter()
        .find(|(p, _)| p.ends_with("release.yml"))
        .expect("release.yml")
        .1;
    // The workflow names the zip and its list entry the same way the documentation does.
    assert!(release.contains("dereth-plugin find . \"$GITHUB_REF_NAME\""));
    assert!(release.contains("dereth-plugin add site/plugins.json"));
    assert!(release.contains("tags: ['v*', '*-v*']"));
}

#[test]
fn a_new_plugins_name_and_id_are_checked() {
    let dir = Scratch::new("names");
    let at = |name: &str| dir.0.join(name);
    let o = |id: Option<&'static str>, name: Option<&'static str>| template::Options {
        id,
        name,
        github: None,
    };
    let v = template::values(&at("Loot Helper"), &o(None, None)).expect("values");
    let get = |k: &str| {
        v.iter()
            .find(|(key, _)| *key == k)
            .map(|(_, v)| v.clone())
            .expect(k)
    };
    assert_eq!(
        (get("name"), get("id"), get("folder"), get("command")),
        (
            "Loot Helper".into(),
            "example.loot-helper".into(),
            "loot-helper".into(),
            "loothelper".into()
        )
    );
    assert!(template::values(&at("x"), &o(Some("network.dereth.mine"), None)).is_err());
    assert!(template::values(&at("x"), &o(Some("dereth.mine"), None)).is_err());
    assert!(template::values(&at("x"), &o(Some("Not An Id"), None)).is_err());
    assert!(template::values(&at("x"), &o(None, Some("Say \"hi\""))).is_err());
    assert!(template::values(&at("x"), &o(None, Some("123"))).is_err());
    let quoted = template::values(&at("x"), &o(None, Some("Virindi's Tank"))).expect("values");
    assert!(quoted.contains(&("source_name", "Virindi''s Tank".to_owned())));
}

#[test]
fn a_script_that_does_not_compile_fails_the_check() {
    let dir = Scratch::new("check");
    std::fs::write(
        dir.0.join("plugin.toml"),
        "id = \"example.broken\"\nname = \"Broken\"\nversion = \"1.0.0\"\napi = \"0.1\"\n",
    )
    .expect("manifest");
    std::fs::write(dir.0.join("main.lua"), "print(1)\n").expect("main");
    std::fs::create_dir_all(dir.0.join("lib")).expect("lib");
    std::fs::write(dir.0.join("lib/util.lua"), "local x = \nend\n").expect("util");
    let e = crate::check::check(&dir.0).unwrap_err();
    assert!(e.contains("lib/util.lua:2"), "{e}");
    std::fs::write(dir.0.join("lib/util.lua"), "return 1\n").expect("util");
    assert!(crate::check::check(&dir.0).is_ok());
    // A plugin that needs a newer API than this tool's Dereth fails too.
    std::fs::write(
        dir.0.join("plugin.toml"),
        "id = \"example.broken\"\nname = \"Broken\"\nversion = \"1.0.0\"\napi = \"0.99\"\n",
    )
    .expect("manifest");
    assert!(crate::check::check(&dir.0)
        .unwrap_err()
        .contains("needs plugin API 0.99"));
}

#[test]
fn the_version_is_set_in_place_and_nothing_else_in_the_manifest_changes() {
    let text = "# A comment\nid = \"a.b\"\nversion   = \"0.1.0\" # kept\napi = \"0.7\"\n\n[permissions]\nversion = \"not this one\"\n";
    let set = release::set_version(text, "1.2.3-rc.1").expect("set");
    assert_eq!(
        set,
        text.replace("\"0.1.0\" # kept", "\"1.2.3-rc.1\" # kept")
    );
    assert!(release::set_version("id = \"a.b\"\n[x]\nversion = \"1.0.0\"\n", "2.0.0").is_err());
}

#[test]
fn a_release_moves_the_unreleased_notes_under_its_version() {
    let text = "# Changes\n\n## Unreleased\n\n- Counts deaths.\n- Faster.\n\n## 0.1.0 (2026-01-02)\n\n- First release.\n";
    let rolled = release::roll_changes(text, "0.2.0", "2026-10-26").expect("rolled");
    assert_eq!(rolled, "# Changes\n\n## Unreleased\n\n## 0.2.0 (2026-10-26)\n\n- Counts deaths.\n- Faster.\n\n## 0.1.0 (2026-01-02)\n\n- First release.\n");
    assert_eq!(
        release::section(&rolled, "0.2.0").as_deref(),
        Some("- Counts deaths.\n- Faster.")
    );
    assert_eq!(
        release::section(&rolled, "0.1.0").as_deref(),
        Some("- First release.")
    );
    assert_eq!(release::section(&rolled, "0.1").as_deref(), None);
    assert_eq!(release::section(&rolled, "Unreleased").as_deref(), Some(""));
    let empty = release::roll_changes(&rolled, "0.3.0", "2026-10-27").unwrap_err();
    assert!(empty.contains("nothing under ## Unreleased"), "{empty}");
    assert!(release::roll_changes("# Changes\n", "1.0.0", "2026-10-27").is_err());
}

#[test]
fn a_repository_of_several_plugins_tags_each_by_its_id() {
    let dir = Scratch::new("several");
    for (folder, id) in [("one", "example.one"), ("plugins/two", "example.two")] {
        let f = dir.0.join(folder);
        std::fs::create_dir_all(&f).expect("folder");
        std::fs::write(
            f.join("plugin.toml"),
            format!("id = \"{id}\"\nname = \"N\"\nversion = \"1.0.0-beta.1\"\napi = \"0.1\"\n"),
        )
        .expect("manifest");
        std::fs::write(f.join("main.lua"), "print(1)\n").expect("main");
    }
    std::fs::create_dir_all(dir.0.join("target/x")).expect("build output");
    std::fs::write(dir.0.join("target/x/plugin.toml"), "not a plugin").expect("ignored");
    assert_eq!(release::packages_in(&dir.0).expect("found").len(), 2);
    assert_eq!(
        release::tag_for(2, "example.two", "1.0.0"),
        "example.two-v1.0.0"
    );
    assert_eq!(release::tag_for(1, "example.two", "1.0.0"), "v1.0.0");
    let found = release::find(&dir.0, "example.two-v1.0.0-beta.1").expect("found");
    assert!(found.ends_with("\texample.two\t1.0.0-beta.1\tN"), "{found}");
    assert!(release::find(&dir.0, "v1.0.0-beta.1")
        .unwrap_err()
        .contains("2 plugins"));
    assert!(release::find(&dir.0, "example.three-v1.0.0").is_err());
}

#[test]
fn a_list_lists_each_plugin_at_its_latest_release_only() {
    let dir = Scratch::new("list");
    let write = |version: &str, uses: &str| {
        let f = dir.0.join(format!("p-{version}"));
        std::fs::create_dir_all(&f).expect("folder");
        std::fs::write(f.join("plugin.toml"), format!("id = \"example.meter\"\nname = \"Meter\"\nversion = \"{version}\"\napi = \"0.1\"\n{uses}")).expect("manifest");
        std::fs::write(f.join("main.lua"), "print(1)\n").expect("main");
        let zip = dir.0.join(format!("meter-{version}.zip"));
        crate::pack(&f, &zip).expect("packed");
        zip
    };
    let list_file = dir.0.join("plugins.json");
    let v1 = write("1.0.0", "");
    let v2 = write("1.1.0", "[permissions]\n\"chat.send\" = \"Says hi.\"\n");
    let v0 = write("0.9.0", "");
    assert!(
        list::add(
            &list_file,
            &v1,
            "http://example.org/x.zip",
            "",
            None,
            "2026-10-26"
        )
        .is_err(),
        "plain http"
    );
    list::add(
        &list_file,
        &v1,
        "https://github.com/o/r/releases/download/v1.0.0/meter-1.0.0.zip",
        "",
        None,
        "2026-10-26",
    )
    .expect("listed");
    assert_eq!(
        list::add(
            &list_file,
            &v1,
            "https://github.com/o/r/releases/download/v1.0.0/meter-1.0.0.zip",
            "",
            None,
            "2026-10-26"
        )
        .as_deref(),
        Ok("example.meter 1.0.0 is listed already")
    );
    let said = list::add(
        &list_file,
        &v2,
        "meter-1.1.0.zip",
        "Says hi.",
        None,
        "2026-10-27",
    )
    .expect("listed");
    assert_eq!(said, "listed example.meter 1.1.0 in place of 1.0.0");
    assert!(
        list::add(&list_file, &v0, "meter-0.9.0.zip", "", None, "2026-10-28")
            .unwrap_err()
            .contains("must be newer")
    );
    let read =
        dereth_plugin_package::catalog::Catalog::parse(&std::fs::read(&list_file).expect("list"))
            .expect("reads");
    assert_eq!(read.catalog.plugins.len(), 1);
    let e = &read.catalog.plugins[0];
    assert_eq!(
        (
            e.version.to_string(),
            e.permissions.clone(),
            e.released.as_str()
        ),
        (
            "1.1.0".to_owned(),
            vec!["chat.send".to_owned()],
            "2026-10-27"
        )
    );
}

#[test]
fn dates_are_counted_from_1970_in_the_gregorian_calendar() {
    assert_eq!(crate::date_of_day(0), "1970-01-01");
    assert_eq!(crate::date_of_day(11_016), "2000-02-29");
    assert_eq!(crate::date_of_day(20_752), "2026-10-26");
    assert_eq!(crate::date_of_day(-1), "1969-12-31");
}

#[test]
fn the_version_names_the_tool_and_dereths_version_first() {
    let text = crate::run(&["--version".to_owned()]).expect("a version");
    let mut lines = text.lines();
    assert_eq!(
        lines.next(),
        Some(format!("dereth-plugin {}", env!("CARGO_PKG_VERSION")).as_str())
    );
    assert!(lines.any(|l| l.starts_with("target: ")), "{text}");
}
