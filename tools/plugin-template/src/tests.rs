//! The template over temporary folders: what a new repository is named, where it goes and what is
//! written in it.
//!
//! Behaviour: none (the plugin author's template, written by the author's tool and the launcher)

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

use crate::{new, render, values, Error, Made, Options};

/// A folder of its own for each test, removed when it ends.
struct Scratch(PathBuf);

impl Scratch {
    fn new(tag: &str) -> Self {
        let d = std::env::temp_dir().join(format!(
            "dereth-plugin-template-{tag}-{}",
            std::process::id()
        ));
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

fn options<'a>(id: Option<&'a str>, name: Option<&'a str>) -> Options<'a> {
    Options {
        id,
        name,
        github: None,
        dereth: "9.8.7",
    }
}

#[test]
fn a_new_plugins_name_and_id_are_checked() {
    let dir = Scratch::new("names");
    let at = |name: &str| dir.0.join(name);
    let v = values(&at("Loot Helper"), &options(None, None)).expect("values");
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
    assert!(matches!(
        values(&at("x"), &options(Some("network.dereth.mine"), None)),
        Err(Error::Reserved(_))
    ));
    assert!(values(&at("x"), &options(Some("dereth.mine"), None)).is_err());
    assert!(matches!(
        values(&at("x"), &options(Some("Not An Id"), None)),
        Err(Error::NotAnId { .. })
    ));
    assert!(matches!(
        values(&at("x"), &options(None, Some("Say \"hi\""))),
        Err(Error::Name(_))
    ));
    assert!(matches!(
        values(&at("x"), &options(None, Some("123"))),
        Err(Error::NoLetter(_))
    ));
    let github = |g| Options {
        github: Some(g),
        ..options(None, Some("Loot Helper"))
    };
    assert!(matches!(
        values(&at("x"), &github("someone")),
        Err(Error::Github(_))
    ));
    assert!(values(&at("x"), &github("Someone/loot"))
        .expect("values")
        .contains(&("id", "io.github.someone.loot-helper".to_owned())));
    let quoted = values(&at("x"), &options(None, Some("Virindi's Tank"))).expect("values");
    assert!(quoted.contains(&("source_name", "Virindi''s Tank".to_owned())));
}

/// A new repository is the template's files, each filled in, with the Dereth release it was told
/// in its workflows; it goes into a missing or empty folder and never into one that holds anything.
#[test]
fn a_new_repository_is_the_filled_template_in_an_empty_folder_only() {
    let dir = Scratch::new("new");
    let repo = dir.0.join("made");
    let o = options(None, Some("XP Tracker"));
    assert_eq!(new(&repo, &o).expect("made").plugin, "xp-tracker");
    let files = render(&repo, &o).expect("rendered");
    assert_eq!(files.len(), crate::FILES.len());
    for (path, text) in &files {
        assert_eq!(
            &std::fs::read_to_string(repo.join(path)).expect("written"),
            text,
            "{path}"
        );
    }
    for workflow in ["check.yml", "release.yml", "unlist.yml"] {
        let text = std::fs::read_to_string(repo.join(".github/workflows").join(workflow))
            .expect("the workflow");
        assert!(text.contains("DERETH: '9.8.7'"), "{workflow}: {text}");
    }
    // The README is this plugin's, by its name, folder and id, and with no repository yet, the
    // source's address is a placeholder for one; the plugin says hi.
    let readme = std::fs::read_to_string(repo.join("README.md")).expect("README");
    for said in [
        "# XP Tracker\n",
        "turn **XP Tracker** on",
        "`/plugin reload example.xp-tracker`",
        "dereth-plugin check xp-tracker\n",
        "`xp-tracker/CHANGES.md`",
        "https://<username>.github.io/<repository>/plugins.json",
    ] {
        assert!(readme.contains(said), "{said}");
    }
    let manifest = std::fs::read_to_string(repo.join("xp-tracker/plugin.toml")).expect("manifest");
    assert!(
        manifest.contains(
            "
description = \"Says hi\"
"
        ),
        "{manifest}"
    );

    // An empty folder that is already there takes it too.
    let empty = dir.0.join("empty");
    std::fs::create_dir_all(&empty).expect("a folder");
    new(&empty, &o).expect("made in an empty folder");

    // A folder that holds anything is left as it is.
    let held = dir.0.join("held");
    std::fs::create_dir_all(&held).expect("a folder");
    std::fs::write(held.join("notes.txt"), "mine").expect("a file");
    assert!(matches!(new(&held, &o), Err(Error::NotEmpty(_))));
    let left: Vec<_> = std::fs::read_dir(&held)
        .expect("read")
        .map(|e| e.expect("entry").file_name())
        .collect();
    assert_eq!(left, ["notes.txt"]);
}

/// Every file under `dir`, by its `/`-separated path there, and its bytes.
fn files_under(dir: &Path) -> BTreeMap<String, Vec<u8>> {
    let mut out = BTreeMap::new();
    let mut stack = vec![PathBuf::new()];
    while let Some(rel) = stack.pop() {
        for entry in std::fs::read_dir(dir.join(&rel)).expect("read") {
            let entry = entry.expect("entry");
            let path = rel.join(entry.file_name());
            if entry.file_type().expect("type").is_dir() {
                stack.push(path);
            } else {
                let key = path
                    .components()
                    .map(|c| c.as_os_str().to_string_lossy().into_owned())
                    .collect::<Vec<_>>()
                    .join("/");
                out.insert(key, std::fs::read(dir.join(&path)).expect("bytes"));
            }
        }
    }
    out
}

/// A folder that is a plugin repository already gets only the new plugin's own folder, with
/// exactly the template's plugin files in it, and everything else there is left byte for byte. A
/// folder name or an id the repository has is refused with nothing written, and so is a plugin's
/// own folder. The line it says names the folder, and once there are two plugins, the tags.
#[test]
fn a_plugin_repository_gets_only_the_new_plugins_folder() {
    let dir = Scratch::new("add");
    let repo = dir.0.join("repo");
    new(&repo, &options(None, Some("XP Tracker"))).expect("made");
    // The author's own work in it, which nothing touches.
    std::fs::write(repo.join("xp-tracker/main.lua"), "-- mine\n").expect("an edit");
    std::fs::write(repo.join("notes.txt"), "mine").expect("a file");
    let before = files_under(&repo);

    let loot = options(None, Some("Loot Helper"));
    let made = new(&repo, &loot).expect("added");
    assert_eq!(
        made,
        Made {
            plugin: "loot-helper".into(),
            id: "example.loot-helper".into(),
            beside: 1,
        }
    );
    let mut wanted = before.clone();
    for (path, text) in render(&repo, &loot).expect("rendered") {
        if path.starts_with("loot-helper/") {
            wanted.insert(path, text.into_bytes());
        }
    }
    assert_eq!(wanted.len(), before.len() + 3);
    assert_eq!(files_under(&repo), wanted);
    // The line is the same for a plugin added and for a repository made new.
    let said = format!(
        "Added loot-helper/ to plugin repository at {}.",
        repo.display()
    );
    assert_eq!(made.said(&repo), said);
    assert_eq!(
        Made {
            beside: 0,
            ..made.clone()
        }
        .said(&repo),
        said
    );

    // A folder of the name, or an id, the repository has already: refused, nothing written.
    let now = files_under(&repo);
    assert!(matches!(new(&repo, &loot), Err(Error::FolderTaken(_))));
    std::fs::write(repo.join("bank-notes"), "a file of the name").expect("a file");
    let now_with_file = files_under(&repo);
    assert!(matches!(
        new(&repo, &options(None, Some("Bank Notes"))),
        Err(Error::FolderTaken(_))
    ));
    assert_eq!(files_under(&repo), now_with_file);
    std::fs::remove_file(repo.join("bank-notes")).expect("removed");
    let taken = Options {
        id: Some("example.xp-tracker"),
        ..options(None, Some("Another"))
    };
    assert!(
        matches!(new(&repo, &taken), Err(Error::IdTaken { ref id, .. }) if id == "example.xp-tracker")
    );
    assert_eq!(files_under(&repo), now);
    // A plugin's own folder is not a repository to add to.
    let inside = repo.join("xp-tracker");
    assert!(matches!(
        new(&inside, &options(None, Some("Nested"))),
        Err(Error::InAPlugin(_))
    ));
    assert_eq!(files_under(&repo), now);

    // A third, beside two.
    let third = new(&repo, &options(None, Some("Radar Plus"))).expect("added");
    assert_eq!(third.beside, 2);
    assert_eq!(
        third.said(&repo),
        format!(
            "Added radar-plus/ to plugin repository at {}.",
            repo.display()
        )
    );
}

/// What a plugin repository is, for the author's tool and the launcher alike: a plugin's own folder
/// is that one plugin, and a repository's plugins are the folders directly in it that hold a
/// `plugin.toml`, hidden folders and build output left out. A plugin further down is not found, so
/// a folder whose plugins are all deeper is no repository and gets no plugin added. One whose
/// `plugin.toml` does not say its id stops a new one being added.
#[test]
fn a_repositorys_plugins_are_the_folders_directly_in_it() {
    let dir = Scratch::new("found");
    let write = |folder: &Path, id: &str| {
        std::fs::create_dir_all(folder).expect("a folder");
        std::fs::write(folder.join("plugin.toml"), format!("id = \"{id}\"\n")).expect("manifest");
    };
    let repo = dir.0.join("repo");
    let two = repo.join("two");
    write(&two, "example.two");
    write(&repo.join("plugins/deep"), "example.deep");
    write(&repo.join("target"), "example.built");
    write(&repo.join(".hidden"), "example.hidden");
    assert_eq!(
        crate::packages_in(&repo).expect("found"),
        std::slice::from_ref(&two)
    );
    // A plugin's own folder is that plugin, whatever is under it.
    write(&two.join("inner"), "example.inner");
    assert_eq!(
        crate::packages_in(&two).expect("found"),
        std::slice::from_ref(&two)
    );
    // Plugins only deeper: none, and no plugin is added there.
    let deeper = dir.0.join("deeper");
    write(&deeper.join("plugins/one"), "example.one");
    assert!(crate::packages_in(&deeper).expect("read").is_empty());
    assert!(matches!(
        new(&deeper, &options(None, Some("Three"))),
        Err(Error::NotEmpty(_))
    ));
    assert!(!deeper.join("three").exists());

    let o = Options {
        id: Some("example.two"),
        ..options(None, Some("Two Again"))
    };
    assert!(matches!(new(&repo, &o), Err(Error::IdTaken { .. })));
    new(&repo, &options(None, Some("Three"))).expect("added beside it");
    assert!(repo.join("three/plugin.toml").is_file());

    std::fs::write(two.join("plugin.toml"), "name = \"no id\"\n").expect("manifest");
    assert!(matches!(
        new(&repo, &options(None, Some("Four"))),
        Err(Error::NoId(_))
    ));
    assert!(!repo.join("four").exists());
}

/// The README as the owner wrote it for the template's own repository, `plugin-template` on
/// GitHub at `dereth-network/plugin-template`: what a repository made for that name and repository
/// says, word for word, with the plugin docs' link in Dereth's repository.
const OWNERS_README: &str = r#"# plugin-template

A plugin for [Dereth](https://dereth.network).

## Install it

In the Dereth Network launcher, open **Plugins**, paste this address under **Sources** and press
**Add Source**:

```text
https://dereth-network.github.io/plugin-template/plugins.json
```

Then install **plugin-template** from the list, and turn it on in the client's plugin panel (`/plugin`).

## Work on it

1. In the Dereth Network launcher, open **Plugins**. Under **Installed**, press **Add Plugin Folder** and choose this folder. (If you used the launcher to create this plugin then this part is automatically done for you.)
2. Play, and turn **plugin-template** on in the client's plugin panel (`/plugin`).
3. Edit, save, and reload with `/plugin reload io.github.dereth-network.plugin-template` to check your work.

The plugin interface is [documented here](<docs>).

## Release it

You can release plugins however you like, so long as the plugins.json is correctly written, however, this template is designed around a common GitHub-driven release pattern, assisted by `dereth-plugin`, which comes with Dereth.

Track your changelog under `plugin-template/CHANGES.md`; anything under `## Unreleased` is promoted automatically on the next release.

```text
# verify the plugin will load correctly in-game
dereth-plugin check plugin-template

# release the plugin at <version>
dereth-plugin release plugin-template <version> --push
```

`release` updates the version in `plugin.toml`, commits, tags the
release and pushes the tag to GitHub. The GitHub workflows here will pack the plugin, make a release with the zip, and update the listing in `plugins.json`.

Once, after your first release, you must navigate to your repository **Settings → Pages** and publish from the `gh-pages` branch.

## One folder/repository, many plugins

A single repository can hold several plugins, each a folder beside `plugin-template`, with a `plugin.toml` and `CHANGES.md`. To add one, run `dereth-plugin new . --name "<name>"` in this directory, or choose this directory in the launcher.

To remove one, delete its folder and push: the workflows drop it from `plugins.json`, and its GitHub releases stay.
"#;

/// A repository's README is the owner's text for its plugin: its name, its folder, its id and its
/// source's address in their places, and the plugin docs on Dereth's repository linked.
#[test]
fn the_readme_is_the_owners_text_for_the_plugins_name_folder_id_and_source() {
    let dir = Scratch::new("readme");
    let o = Options {
        github: Some("dereth-network/plugin-template"),
        ..options(None, Some("plugin-template"))
    };
    let files = render(&dir.0.join("plugin-template"), &o).expect("rendered");
    let readme = &files
        .iter()
        .find(|(path, _)| path == "README.md")
        .expect("a README")
        .1;
    let docs = format!("{}/tree/main/docs/plugins", crate::DERETH_REPO);
    assert_eq!(readme, &OWNERS_README.replace("<docs>", &docs));

    // Another plugin's README is the same text with that plugin's name, folder, id and source.
    let o = Options {
        github: Some("Someone/loot"),
        ..options(None, Some("Loot Helper"))
    };
    let other = render(&dir.0.join("x"), &o).expect("rendered");
    let other = &other
        .iter()
        .find(|(p, _)| p == "README.md")
        .expect("a README")
        .1;
    let swapped = OWNERS_README
        .replace("<docs>", &docs)
        .replace(
            "https://dereth-network.github.io/plugin-template/plugins.json",
            "https://someone.github.io/loot/plugins.json",
        )
        .replace(
            "io.github.dereth-network.plugin-template",
            "io.github.someone.loot-helper",
        )
        .replace("# plugin-template\n", "# Loot Helper\n")
        .replace("**plugin-template**", "**Loot Helper**")
        .replace("plugin-template", "loot-helper");
    assert_eq!(other, &swapped);
}

/// The template's `.gitignore` keeps out the copy of the author's tool the launcher puts at a
/// repository's root, on every system, and the build output and site the workflows make.
#[test]
fn the_template_ignores_the_authors_tool_at_the_repositorys_root() {
    let gitignore = crate::FILES
        .iter()
        .find(|(path, _)| *path == ".gitignore")
        .expect("a .gitignore")
        .1;
    let lines: Vec<&str> = gitignore.lines().collect();
    for line in crate::TOOL_IGNORED.iter().chain(&["/dist/", "/site/"]) {
        assert!(lines.contains(line), "{line}: {gitignore}");
    }
}
