//! A new plugin repository from the template, or a new plugin added to one: its files, filled in
//! for a plugin's name, id and GitHub repository and for the Dereth release whose author's tool
//! its workflows install.
//!
//! The template has its one source here: the files under `tools/plugin-template/template/`. The
//! author's tool writes it (`dereth-plugin new`), the launcher writes it from its Plugins page, and
//! the template repository on GitHub is a copy made with it.
//!
//! **Depends on** `dereth-plugin-api` for the plugin API version the template's manifest asks for
//! and the rules a plugin's id keeps, and `toml` for the ids of the plugins a repository holds.
//! **Used by** `dereth-plugin` (`new`) and the launcher app,
//! a workspace of its own.
//!
//! **Must never** write anywhere but the folder it is given.
//!
//! ```text
//! README.md                       what it is, how to install it from its source, how to work on it
//!                                 and release it
//! .gitattributes, .gitignore      .gitignore keeps out the launcher's copy of the author's tool
//! .github/workflows/check.yml     every push: each plugin checked
//! .github/workflows/release.yml   a release tag: packed, released on GitHub, listed in plugins.json
//!                                 on the gh-pages branch, which GitHub Pages publishes
//! .github/workflows/unlist.yml    a push to the default branch: a plugin whose folder is gone is
//!                                 dropped from plugins.json on the gh-pages branch
//! <folder>/plugin.toml            the plugin: a window that says hello, and a command
//! <folder>/main.lua
//! <folder>/CHANGES.md
//! ```
//!
//! [`new`] writes all of it into an empty folder. A folder that is already a plugin repository (a
//! folder directly in it holds a plugin, as [`packages_in`] finds them) gets only the new plugin's
//! folder, beside the others; its releases are then tagged `<id>-v<version>`, as in any repository
//! of several. [`packages_in`] is what a plugin repository is for the author's tool and the
//! launcher alike.

use std::fmt;
use std::path::{Path, PathBuf};

use dereth_plugin_api::{ApiVersion, PluginId};

#[cfg(test)]
mod tests;

/// The template's files: where each goes in the new repository (`{folder}` is the plugin's
/// folder), and its text.
pub const FILES: &[(&str, &str)] = &[
    ("README.md", include_str!("../template/README.md")),
    (".gitattributes", include_str!("../template/gitattributes")),
    (".gitignore", include_str!("../template/gitignore")),
    (
        ".github/workflows/check.yml",
        include_str!("../template/check.yml"),
    ),
    (
        ".github/workflows/release.yml",
        include_str!("../template/release.yml"),
    ),
    (
        ".github/workflows/unlist.yml",
        include_str!("../template/unlist.yml"),
    ),
    (
        "{folder}/plugin.toml",
        include_str!("../template/plugin.toml"),
    ),
    ("{folder}/main.lua", include_str!("../template/main.lua")),
    (
        "{folder}/CHANGES.md",
        include_str!("../template/CHANGES.md"),
    ),
];

/// The lines of a plugin repository's `.gitignore` that keep the author's tool out of git: the
/// copy the launcher keeps at the repository's root on Windows, and on the other systems. The
/// template's `.gitignore` has them.
pub const TOOL_IGNORED: [&str; 2] = ["/dereth-plugin", "/dereth-plugin.exe"];

/// The repository Dereth's releases are published in: the one a release build names, else
/// Dereth's own. The template's workflows download the tool from there.
pub const DERETH_REPO: &str = match option_env!("DERETH_BUILD_SOURCE_URL") {
    Some(url) => url,
    None => "https://github.com/dereth-network/dereth",
};

/// What a new repository is made from.
#[derive(Debug)]
pub struct Options<'a> {
    pub id: Option<&'a str>,
    pub name: Option<&'a str>,
    /// `<username>/<repo>`: the repository on GitHub the plugin will live in.
    pub github: Option<&'a str>,
    /// The Dereth release whose author's tool the workflows install: the version of the program
    /// that writes the repository, which comes with that release.
    pub dereth: &'a str,
}

/// Why a repository could not be made. Its words are the author's tool's; another front end
/// words each case its own way.
#[derive(Debug)]
pub enum Error {
    /// The name is empty, or has a quote, a backslash or a line break.
    Name(String),
    /// The name has no letter to start a folder's name and an id with.
    NoLetter(String),
    /// The GitHub repository is not `<username>/<repo>`.
    Github(String),
    /// The id (given, or made from the name and the repository's owner) is not a plugin id.
    NotAnId { id: String, folder: String },
    /// The id is in a namespace kept for Dereth itself.
    Reserved(String),
    /// The folder holds something already, and no plugin in a folder directly in it.
    NotEmpty(PathBuf),
    /// The folder is a plugin's own (it holds a `plugin.toml`), not a repository of plugins.
    InAPlugin(PathBuf),
    /// The repository has a folder of the new plugin's name already.
    FolderTaken(PathBuf),
    /// A plugin in the repository has the new plugin's id.
    IdTaken { id: String, at: PathBuf },
    /// A plugin's `plugin.toml` in the repository does not say its id.
    NoId(PathBuf),
    /// Reading or writing a path failed.
    Io(PathBuf, std::io::Error),
}

impl fmt::Display for Error {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Name(name) => write!(
                f,
                "{name:?} cannot be a plugin's name here: give one with --name"
            ),
            Self::NoLetter(name) => write!(
                f,
                "{name:?} needs a letter first to make a folder and an id of: give --name"
            ),
            Self::Github(g) => write!(f, "--github {g}: give <username>/<repo>"),
            Self::NotAnId { id, folder } => write!(
                f,
                "{id:?} is not a plugin id (lowercase reverse-DNS, e.g. io.github.you.{folder})"
            ),
            Self::Reserved(id) => write!(f, "{id} is in a namespace kept for Dereth itself"),
            Self::NotEmpty(dir) => write!(
                f,
                "{} is not empty, and holds no plugin to add another beside",
                dir.display()
            ),
            Self::InAPlugin(dir) => write!(
                f,
                "{} is a plugin's own folder: give the repository that holds it",
                dir.display()
            ),
            Self::FolderTaken(dir) => {
                write!(f, "{} is there already: give another --name", dir.display())
            }
            Self::IdTaken { id, at } => write!(f, "{} has the id {id} already", at.display()),
            Self::NoId(path) => write!(f, "{} does not say its id", path.display()),
            Self::Io(path, e) => write!(f, "{}: {e}", path.display()),
        }
    }
}

impl std::error::Error for Error {}

/// The words of a name as a folder's name: lowercase letters and digits, joined by `-`.
fn slug(name: &str) -> String {
    let mut out = String::new();
    for c in name.chars() {
        let c = c.to_ascii_lowercase();
        if c.is_ascii_lowercase() || c.is_ascii_digit() {
            out.push(c);
        } else if !out.is_empty() && !out.ends_with('-') {
            out.push('-');
        }
    }
    out.trim_end_matches('-').to_owned()
}

/// The values the template's placeholders take. The name is the folder's own when none is given.
pub fn values(folder: &Path, o: &Options) -> Result<Vec<(&'static str, String)>, Error> {
    let fallback = folder
        .file_name()
        .map(|n| n.to_string_lossy().into_owned())
        .unwrap_or_default();
    let name = o.name.unwrap_or(&fallback).trim().to_owned();
    if name.is_empty() || name.contains(['"', '\\', '\n', '\r']) {
        return Err(Error::Name(name));
    }
    let package = slug(&name);
    if package.is_empty() || !package.starts_with(|c: char| c.is_ascii_lowercase()) {
        return Err(Error::NoLetter(name));
    }
    let github = match o.github {
        Some(g) => {
            let (owner, repo) = g
                .split_once('/')
                .filter(|(o, r)| !o.is_empty() && !r.is_empty() && !r.contains('/'))
                .ok_or_else(|| Error::Github(g.to_owned()))?;
            Some((owner.to_owned(), repo.to_owned()))
        }
        None => None,
    };
    let id = match (o.id, &github) {
        (Some(id), _) => id.to_owned(),
        (None, Some((owner, _))) => format!("io.github.{}.{package}", slug(owner)),
        (None, None) => format!("example.{package}"),
    };
    let Some(parsed) = PluginId::parse(&id) else {
        return Err(Error::NotAnId {
            id,
            folder: package,
        });
    };
    if parsed.is_reserved() || id.starts_with("network.dereth.") {
        return Err(Error::Reserved(id));
    }
    let (source_url, homepage) = match &github {
        Some((owner, repo)) => (
            format!(
                "https://{}.github.io/{repo}/plugins.json",
                owner.to_ascii_lowercase()
            ),
            format!("homepage = \"https://github.com/{owner}/{repo}\"\n"),
        ),
        None => (
            "https://<username>.github.io/<repository>/plugins.json".to_owned(),
            String::new(),
        ),
    };
    Ok(vec![
        ("name", name.clone()),
        ("id", id),
        ("folder", package.clone()),
        ("command", package.replace('-', "")),
        ("api", ApiVersion::CURRENT.to_string()),
        ("dereth", o.dereth.to_owned()),
        ("dereth_repo", DERETH_REPO.trim_end_matches('/').to_owned()),
        ("source_url", source_url),
        ("homepage", homepage),
        // In a single-quoted YAML string a quote is written twice.
        ("source_name", name.replace('\'', "''")),
    ])
}

/// `text` with each `{{key}}` replaced by its value.
pub fn fill(text: &str, values: &[(&str, String)]) -> String {
    let mut out = text.to_owned();
    for (k, v) in values {
        out = out.replace(&format!("{{{{{k}}}}}"), v);
    }
    out
}

/// The template's files for `folder` as told: each one's path in the repository and its text.
pub fn render(folder: &Path, o: &Options) -> Result<Vec<(String, String)>, Error> {
    let values = values(folder, o)?;
    let package = &values
        .iter()
        .find(|(k, _)| *k == "folder")
        .expect("the folder's value")
        .1;
    Ok(FILES
        .iter()
        .map(|(path, text)| (path.replace("{folder}", package), fill(text, &values)))
        .collect())
}

/// The manifest's file name in a plugin's folder.
const MANIFEST_FILE: &str = "plugin.toml";

/// Folders a repository's plugins are not looked for in.
const NOT_SEARCHED: &[&str] = &["target", "dist", "site", "node_modules"];

/// The plugin folders of a repository: the folder itself when it holds a `plugin.toml` (a
/// plugin's own folder), else each folder directly in it that holds one, hidden folders and build
/// output left out. Nothing deeper is looked at: a folder whose plugins are further down is not a
/// plugin repository. Sorted.
pub fn packages_in(root: &Path) -> Result<Vec<PathBuf>, Error> {
    if root.join(MANIFEST_FILE).is_file() {
        return Ok(vec![root.to_path_buf()]);
    }
    let io = |e| Error::Io(root.to_path_buf(), e);
    let mut out = Vec::new();
    for entry in std::fs::read_dir(root).map_err(io)? {
        let entry = entry.map_err(io)?;
        let name = entry.file_name().to_string_lossy().into_owned();
        if name.starts_with('.') || NOT_SEARCHED.contains(&name.as_str()) {
            continue;
        }
        let dir = entry.path();
        if entry.file_type().map_err(io)?.is_dir() && dir.join(MANIFEST_FILE).is_file() {
            out.push(dir);
        }
    }
    out.sort();
    Ok(out)
}

/// The id the `plugin.toml` in `folder` says.
fn id_of(folder: &Path) -> Result<String, Error> {
    let path = folder.join(MANIFEST_FILE);
    let text = std::fs::read_to_string(&path).map_err(|e| Error::Io(path.clone(), e))?;
    text.parse::<toml::Table>()
        .ok()
        .and_then(|t| t.get("id")?.as_str().map(str::to_owned))
        .ok_or(Error::NoId(path))
}

/// What [`new`] made.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Made {
    /// The plugin's own folder in the repository, by its name there.
    pub plugin: String,
    pub id: String,
    /// How many plugins the repository held before: none when it was made new.
    pub beside: usize,
}

impl Made {
    /// What was made in `folder`, in a line for the author, the same whether the repository was
    /// made new or held plugins already. The author's tool and the launcher both say this.
    pub fn said(&self, folder: &Path) -> String {
        format!(
            "Added {}/ to plugin repository at {}.",
            self.plugin,
            folder.display()
        )
    }
}

/// Write the template into `folder`: all of it into a missing or empty folder, or only the new
/// plugin's own folder into one that is a plugin repository already, refusing a folder name or an
/// id the repository has. Any other folder is left as it is.
pub fn new(folder: &Path, o: &Options) -> Result<Made, Error> {
    let held = folder.exists()
        && std::fs::read_dir(folder)
            .map_err(|e| Error::Io(folder.to_owned(), e))?
            .next()
            .is_some();
    let others = if held {
        packages_in(folder)?
    } else {
        Vec::new()
    };
    if held && others.is_empty() {
        return Err(Error::NotEmpty(folder.to_owned()));
    }
    if others.iter().any(|p| p == folder) {
        return Err(Error::InAPlugin(folder.to_owned()));
    }
    let values = values(folder, o)?;
    let value = |key: &str| {
        values
            .iter()
            .find(|(k, _)| *k == key)
            .map(|(_, v)| v.clone())
            .expect("every value is made")
    };
    let (plugin, id) = (value("folder"), value("id"));
    let mut files: Vec<(String, String)> = FILES
        .iter()
        .map(|(path, text)| (path.replace("{folder}", &plugin), fill(text, &values)))
        .collect();
    if !others.is_empty() {
        let own = folder.join(&plugin);
        if own.exists() {
            return Err(Error::FolderTaken(own));
        }
        for other in &others {
            if id_of(other)? == id {
                return Err(Error::IdTaken {
                    id,
                    at: other.join(MANIFEST_FILE),
                });
            }
        }
        files.retain(|(path, _)| path.starts_with(&format!("{plugin}/")));
        // Made by itself, so that nothing that appeared there meanwhile is written into.
        std::fs::create_dir(&own).map_err(|e| Error::Io(own.clone(), e))?;
    }
    for (path, text) in &files {
        let to = folder.join(path);
        if let Some(parent) = to.parent() {
            std::fs::create_dir_all(parent).map_err(|e| Error::Io(parent.to_owned(), e))?;
        }
        std::fs::write(&to, text).map_err(|e| Error::Io(to.clone(), e))?;
    }
    Ok(Made {
        plugin,
        id,
        beside: others.len(),
    })
}
