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
        dereth: crate::DERETH_VERSION,
    }
}

#[test]
fn a_new_repository_holds_a_plugin_that_checks_packs_releases_and_is_listed() {
    let dir = Scratch::new("new");
    let repo = dir.0.join("xp-tracker");
    let line = [
        "new",
        &repo.display().to_string(),
        "--name",
        "XP Tracker",
        "--github",
        "Someone/xp-tracker",
    ]
    .map(str::to_owned);
    let said = crate::run(&line).expect("made");
    assert_eq!(
        said,
        format!(
            "Added xp-tracker/ to plugin repository at {}.",
            repo.display()
        )
    );
    // What the command wrote is the template as the launcher writes it, for this tool's release.
    for (path, text) in
        template::render(&repo, &options(Some("Someone/xp-tracker"))).expect("rendered")
    {
        let written = std::fs::read_to_string(repo.join(&path)).expect("written");
        assert_eq!(written, text, "{path}");
    }
    let workflow =
        std::fs::read_to_string(repo.join(".github/workflows/check.yml")).expect("check.yml");
    assert!(
        workflow.contains(&format!("DERETH: '{}'", env!("CARGO_PKG_VERSION"))),
        "{workflow}"
    );
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
        crate::run(&line).is_err_and(|e| e.ends_with("is there already: give another --name")),
        "not over a plugin's folder"
    );
    let held = dir.0.join("held");
    std::fs::create_dir_all(&held).expect("a folder");
    std::fs::write(held.join("notes.txt"), "mine").expect("a file");
    let into_held = ["new", &held.display().to_string(), "--name", "XP"].map(str::to_owned);
    assert!(
        crate::run(&into_held).is_err_and(|e| e.ends_with("holds no plugin to add another beside")),
        "not into a folder that holds files and no plugin"
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
        "new", "check", "pack", "hash", "build", "release", "notes", "find", "add", "prune",
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
    for (folder, id) in [("one", "example.one"), ("two", "example.two")] {
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

/// The tool's commands agree with the launcher on what a plugin repository is: its plugins are
/// the folders directly in it. A plugin further down is not checked, found or released there, and
/// a folder whose plugins are all further down holds none.
#[test]
fn a_plugin_further_down_than_the_repositorys_own_folders_is_not_one_of_its_plugins() {
    let dir = Scratch::new("deep");
    let write = |folder: &Path, id: &str| {
        std::fs::create_dir_all(folder).expect("folder");
        std::fs::write(
            folder.join("plugin.toml"),
            format!("id = \"{id}\"\nname = \"N\"\nversion = \"1.0.0\"\napi = \"0.1\"\n"),
        )
        .expect("manifest");
        std::fs::write(folder.join("main.lua"), "print(1)\n").expect("main");
    };
    let one = dir.0.join("one");
    write(&one, "example.one");
    let deep = dir.0.join("more").join("plugins").join("deep");
    write(&deep, "example.deep");

    let said = crate::check::check(&dir.0).expect("checks");
    assert!(
        said.contains("example.one 1.0.0: ok") && !said.contains("example.deep"),
        "{said}"
    );
    let e = crate::check::check(&dir.0.join("more")).unwrap_err();
    assert!(
        e.ends_with("has no plugin.toml, and neither do the folders in it"),
        "{e}"
    );
    assert!(release::find(&dir.0, "v1.0.0")
        .expect("the one plugin")
        .contains("\texample.one\t"));
    assert!(release::find(&dir.0, "example.deep-v1.0.0").is_err());

    // Releasing the deeper one is refused, with nothing committed or tagged; the one directly in
    // the repository is its one plugin, tagged v<version>.
    git_init(&dir.0);
    git(&dir.0, &["add", "--all"]);
    git(&dir.0, &["commit", "--quiet", "-m", "start"]);
    let e = release::release(&deep, "1.1.0", "2026-10-27", false).unwrap_err();
    assert!(e.contains("is not a plugin of the repository at"), "{e}");
    assert_eq!(git(&dir.0, &["tag", "--list"]), "");
    assert_eq!(git(&dir.0, &["rev-list", "--count", "HEAD"]), "1");
    let said = release::release(&one, "1.1.0", "2026-10-27", false).expect("released");
    assert!(said.contains("tagged v1.1.0"), "{said}");
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

/// `new` on a plugin repository adds only the new plugin's folder, as the launcher's Make Your Own
/// does (both go through the template's own `new`), says so, and the repository's releases are
/// then tagged by id; an id the repository has is refused with nothing written.
#[test]
fn new_on_a_plugin_repository_adds_a_plugin_whose_releases_are_tagged_by_id() {
    let dir = Scratch::new("add");
    let repo = dir.0.join("plugins");
    let at = repo.display().to_string();
    let run =
        |words: &[&str]| crate::run(&words.iter().map(|w| (*w).to_owned()).collect::<Vec<_>>());
    run(&[
        "new",
        &at,
        "--name",
        "XP Tracker",
        "--github",
        "Someone/plugins",
    ])
    .expect("made");
    git_init(&repo);
    git(&repo, &["add", "--all"]);
    git(&repo, &["commit", "--quiet", "-m", "start"]);

    let said = run(&[
        "new",
        &at,
        "--name",
        "Loot Helper",
        "--github",
        "Someone/plugins",
    ])
    .expect("added");
    assert_eq!(
        said,
        format!("Added loot-helper/ to plugin repository at {at}.")
    );
    // Only the new plugin's three files, each as the template renders it.
    let status = git(&repo, &["status", "--porcelain", "--untracked-files=all"]);
    let mut changed: Vec<&str> = status.lines().collect();
    changed.sort_unstable();
    assert_eq!(
        changed,
        [
            "?? loot-helper/CHANGES.md",
            "?? loot-helper/main.lua",
            "?? loot-helper/plugin.toml"
        ]
    );
    let o = template::Options {
        name: Some("Loot Helper"),
        github: Some("Someone/plugins"),
        ..options(None)
    };
    for (path, text) in template::render(&repo, &o).expect("rendered") {
        if path.starts_with("loot-helper/") {
            let written = std::fs::read_to_string(repo.join(&path)).expect("written");
            assert_eq!(written, text, "{path}");
        }
    }
    let taken = run(&[
        "new",
        &at,
        "--name",
        "Other",
        "--id",
        "io.github.someone.xp-tracker",
    ]);
    assert!(
        taken.is_err_and(|e| e.ends_with("has the id io.github.someone.xp-tracker already")),
        "an id the repository has"
    );
    assert_eq!(
        git(&repo, &["status", "--porcelain", "--untracked-files=all"]),
        status,
        "nothing written"
    );

    // Two plugins: each released by its own tag, found by it.
    assert!(crate::check::check(&repo).is_ok());
    git(&repo, &["add", "--all"]);
    git(&repo, &["commit", "--quiet", "-m", "loot"]);
    let folder = repo.join("loot-helper");
    let released = release::release(&folder, "0.1.0", "2026-10-27", false).expect("released");
    assert!(
        released.contains("tagged io.github.someone.loot-helper-v0.1.0"),
        "{released}"
    );
    let found = release::find(&repo, "io.github.someone.loot-helper-v0.1.0").expect("found");
    assert!(
        found.ends_with("\tio.github.someone.loot-helper\t0.1.0\tLoot Helper"),
        "{found}"
    );
}

/// `prune` keeps every plugin a source lists that the repository still holds, found as every
/// command finds a repository's plugins, and drops the rest: their entries, and the zips the list
/// names beside itself (the folder the release workflow put each in, and a `.sha256` that `pack`
/// left), never a zip elsewhere. With nothing to drop it writes nothing; a repository holding no
/// plugin, or one whose manifest does not read, drops nothing at all.
#[test]
fn prune_drops_the_plugins_the_repository_no_longer_holds_with_their_zips() {
    let dir = Scratch::new("prune");
    let repo = dir.0.join("repo");
    let site = dir.0.join("site");
    let list_file = site.join("plugins.json");
    let write = |folder: &str, id: &str| {
        let f = repo.join(folder);
        std::fs::create_dir_all(&f).expect("folder");
        std::fs::write(
            f.join("plugin.toml"),
            format!("id = \"{id}\"\nname = \"N\"\nversion = \"1.0.0\"\napi = \"0.1\"\n"),
        )
        .expect("manifest");
        std::fs::write(f.join("main.lua"), "print(1)\n").expect("main");
    };
    // Three plugins released as the release workflow lists them, each zip in a folder of its own
    // beside the list; a fourth listed at an address of its own.
    let plugins = [
        ("loot", "example.loot"),
        ("meter", "example.meter"),
        ("radar", "example.radar"),
    ];
    for (folder, id) in plugins {
        write(folder, id);
        let zip = dir.0.join(format!("{id}-1.0.0.zip"));
        crate::pack(&repo.join(folder), &zip).expect("packed");
        std::fs::create_dir_all(site.join(id)).expect("its folder");
        let at = format!("{id}/{id}-1.0.0.zip");
        std::fs::copy(&zip, site.join(&at)).expect("put beside the list");
        list::add(&list_file, &zip, &at, "", Some("Mine"), "2026-10-27").expect("listed");
    }
    std::fs::write(
        site.join("example.meter/example.meter-1.0.0.zip.sha256"),
        "sum",
    )
    .expect("sum");
    write("far", "example.far");
    let far = dir.0.join("far.zip");
    crate::pack(&repo.join("far"), &far).expect("packed");
    let far_at = "https://example.org/far.zip";
    list::add(&list_file, &far, far_at, "", None, "2026-10-27").expect("listed");
    std::fs::remove_dir_all(repo.join("far")).expect("removed");
    let other = site.join("other.txt");
    std::fs::write(&other, "the site's own").expect("a file");
    let listed = |path: &Path| -> Vec<String> {
        let read =
            dereth_plugin_package::catalog::Catalog::parse(&std::fs::read(path).expect("list"))
                .expect("reads");
        read.catalog
            .plugins
            .iter()
            .map(|e| e.id.to_string())
            .collect()
    };
    let repo_at = repo.display().to_string();
    let list_at = list_file.display().to_string();
    let prune = || crate::run(&["prune", &list_at, &repo_at].map(str::to_owned));

    // Meter's folder is deleted: it and the plugin listed far away go, the others stay.
    std::fs::remove_dir_all(repo.join("meter")).expect("deleted");
    assert_eq!(
        prune().as_deref(),
        Ok("unlisted example.far 1.0.0\nunlisted example.meter 1.0.0")
    );
    assert_eq!(listed(&list_file), ["example.loot", "example.radar"]);
    assert!(
        !site.join("example.meter").exists(),
        "its zip, sum and folder"
    );
    for id in ["example.loot", "example.radar"] {
        assert!(site.join(format!("{id}/{id}-1.0.0.zip")).is_file(), "{id}");
    }
    assert!(other.is_file(), "what the list does not name stays");
    let read =
        dereth_plugin_package::catalog::Catalog::parse(&std::fs::read(&list_file).expect("list"))
            .expect("reads");
    assert_eq!(read.catalog.name, "Mine", "the rest of the list is kept");

    // Nothing more to drop: the list is not written (a list written again would lose the blank
    // line added to its end here).
    let mut before = std::fs::read(&list_file).expect("list");
    before.extend_from_slice(b"\n");
    std::fs::write(&list_file, &before).expect("a blank line more");
    assert!(prune().is_ok_and(|s| s.starts_with("every plugin")));
    assert_eq!(std::fs::read(&list_file).expect("list"), before);

    // A manifest that does not read, or a repository holding no plugin: nothing is dropped.
    std::fs::write(repo.join("radar/plugin.toml"), "id = 1\n").expect("broken");
    assert!(prune().is_err());
    std::fs::remove_dir_all(repo.join("radar")).expect("deleted");
    std::fs::remove_dir_all(repo.join("loot")).expect("deleted");
    assert!(
        prune().is_err_and(|e| e.ends_with("holds no plugin: give the repository's own folder"))
    );
    assert_eq!(std::fs::read(&list_file).expect("list"), before);
    assert_eq!(listed(&list_file), ["example.loot", "example.radar"]);

    // No list yet: nothing to do.
    write("loot", "example.loot");
    let none = dir.0.join("none").join("plugins.json");
    let said = list::prune(&none, &repo).expect("nothing to do");
    assert!(said.ends_with("is not there: nothing to unlist"), "{said}");
    assert!(!none.exists());
}

/// A dropped plugin's zip is removed only when the list names it by a plain path beside itself:
/// never one reached with `..`, an address, or a zip another plugin still listed names.
#[test]
fn prune_removes_only_zips_named_by_a_plain_path_beside_the_list() {
    let dir = Scratch::new("prune-paths");
    let repo = dir.0.join("repo");
    let site = dir.0.join("site");
    std::fs::create_dir_all(&site).expect("site");
    let kept = repo.join("kept");
    std::fs::create_dir_all(&kept).expect("folder");
    std::fs::write(
        kept.join("plugin.toml"),
        "id = \"example.kept\"\nname = \"N\"\nversion = \"1.0.0\"\napi = \"0.1\"\n",
    )
    .expect("manifest");
    std::fs::write(kept.join("main.lua"), "print(1)\n").expect("main");
    let zip_of = |id: &str| {
        let f = dir.0.join(format!("src-{id}"));
        std::fs::create_dir_all(&f).expect("folder");
        std::fs::write(
            f.join("plugin.toml"),
            format!("id = \"{id}\"\nname = \"N\"\nversion = \"1.0.0\"\napi = \"0.1\"\n"),
        )
        .expect("manifest");
        std::fs::write(f.join("main.lua"), "print(1)\n").expect("main");
        let zip = dir.0.join(format!("{id}.zip"));
        crate::pack(&f, &zip).expect("packed");
        zip
    };
    let list_file = site.join("plugins.json");
    let outside = dir.0.join("outside.zip");
    std::fs::write(&outside, "not the site's").expect("a file");
    let shared = site.join("shared.zip");
    std::fs::write(&shared, "shared").expect("a file");
    for (id, at) in [
        ("example.kept", "shared.zip"),
        ("example.gone", "shared.zip"),
        ("example.away", "../outside.zip"),
    ] {
        list::add(&list_file, &zip_of(id), at, "", None, "2026-10-27").expect("listed");
    }
    let said = list::prune(&list_file, &repo).expect("pruned");
    assert_eq!(
        said,
        "unlisted example.away 1.0.0\nunlisted example.gone 1.0.0"
    );
    assert!(outside.is_file(), "not beside the list");
    assert!(shared.is_file(), "still named by a plugin kept");
}

/// The unlist workflow runs `prune` as the tests above run it, on a copy of the gh-pages branch,
/// only on the default branch and only once that branch exists. It commits and pushes only from
/// the job that waits its turn with the releases, in their one concurrency group, and that job
/// runs only when the first found something to drop, so an ordinary push never takes the place
/// of a release waiting to run.
#[test]
fn the_unlist_workflow_prunes_gh_pages_on_the_default_branch_in_turn_with_the_releases() {
    let file = |name: &str| {
        template::FILES
            .iter()
            .find(|(p, _)| p.ends_with(name))
            .unwrap_or_else(|| panic!("{name}"))
            .1
    };
    let (unlist, release) = (file("unlist.yml"), file("release.yml"));
    let group = "concurrency:\n  group: release\n  cancel-in-progress: false\n";
    assert!(release.contains(group), "{release}");
    let (look, push) = unlist
        .split_once("\n  unlist:\n")
        .expect("the look job, then the unlist job");
    let prune = "dereth-plugin prune site/plugins.json .";
    for job in [look, push] {
        assert_eq!(job.matches(prune).count(), 1, "{job}");
        assert!(job.contains("git worktree add site gh-pages"), "{job}");
    }
    assert!(look.contains("if: github.ref_name == github.event.repository.default_branch"));
    assert!(look.contains("if git ls-remote --exit-code --heads origin gh-pages > /dev/null; then"));
    assert!(look.contains("echo \"dropped=true\" >> \"$GITHUB_OUTPUT\""));
    assert!(
        !look.contains("concurrency:") && !look.contains("git push") && !look.contains("commit")
    );
    assert!(push.contains("    needs: look\n    if: needs.look.outputs.dropped == 'true'\n"));
    assert!(
        push.contains("    concurrency:\n      group: release\n      cancel-in-progress: false\n")
    );
    let commit = push.find("git -C site commit").expect("a commit");
    let nothing = push
        .find("if git -C site diff --cached --quiet; then\n            exit 0")
        .expect("nothing to commit, nothing done");
    assert!(nothing < commit && commit < push.find("git -C site push").expect("a push"));
    assert!(unlist.contains("on:\n  push:\n    branches: ['**']\n"));
}
