//! `new`: a plugin repository from the template, which has its one source here: the files under
//! `tools/plugin/template/`, with this tool's Dereth version, plugin API version and the repository
//! Dereth's releases are in written in. The template repository on GitHub is a copy made with it.
//!
//! ```text
//! README.md                       what it is, how to install it from its source, how to release it
//! .gitattributes, .gitignore
//! .github/workflows/check.yml     every push: each plugin checked
//! .github/workflows/release.yml   a release tag: packed, released on GitHub, listed in plugins.json
//!                                 on the gh-pages branch, which GitHub Pages publishes
//! <folder>/plugin.toml            the plugin: a window that says hello, and a command
//! <folder>/main.lua
//! <folder>/CHANGES.md
//! ```

use std::path::Path;

use dereth_plugin_api::{ApiVersion, PluginId};

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
        "{folder}/plugin.toml",
        include_str!("../template/plugin.toml"),
    ),
    ("{folder}/main.lua", include_str!("../template/main.lua")),
    (
        "{folder}/CHANGES.md",
        include_str!("../template/CHANGES.md"),
    ),
];

/// The Dereth release this tool comes with: the template's workflows install its tool.
pub const DERETH_VERSION: &str = env!("CARGO_PKG_VERSION");

/// The repository Dereth's releases are published in: the one a release build names, else
/// Dereth's own. The template's workflows download the tool from there.
pub const DERETH_REPO: &str = match option_env!("DERETH_BUILD_SOURCE_URL") {
    Some(url) => url,
    None => "https://github.com/dereth-network/dereth",
};

/// What `new` was told.
#[derive(Debug, Default)]
pub struct Options<'a> {
    pub id: Option<&'a str>,
    pub name: Option<&'a str>,
    /// `<owner>/<repo>`: the repository on GitHub the plugin will live in.
    pub github: Option<&'a str>,
}

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

/// The values the template's placeholders take.
pub fn values(folder: &Path, o: &Options) -> Result<Vec<(&'static str, String)>, String> {
    let fallback = folder
        .file_name()
        .map(|n| n.to_string_lossy().into_owned())
        .unwrap_or_default();
    let name = o.name.unwrap_or(&fallback).trim().to_owned();
    if name.is_empty() || name.contains(['"', '\\', '\n', '\r']) {
        return Err(format!(
            "{name:?} cannot be a plugin's name here: give one with --name"
        ));
    }
    let package = slug(&name);
    if package.is_empty() || !package.starts_with(|c: char| c.is_ascii_lowercase()) {
        return Err(format!(
            "{name:?} needs a letter first to make a folder and an id of: give --name"
        ));
    }
    let github = match o.github {
        Some(g) => {
            let (owner, repo) = g
                .split_once('/')
                .filter(|(o, r)| !o.is_empty() && !r.is_empty() && !r.contains('/'))
                .ok_or_else(|| format!("--github {g}: give <owner>/<repo>"))?;
            Some((owner.to_owned(), repo.to_owned()))
        }
        None => None,
    };
    let id = match (o.id, &github) {
        (Some(id), _) => id.to_owned(),
        (None, Some((owner, _))) => format!("io.github.{}.{package}", slug(owner)),
        (None, None) => format!("example.{package}"),
    };
    let parsed = PluginId::parse(&id).ok_or_else(|| {
        format!("{id:?} is not a plugin id (lowercase reverse-DNS, e.g. io.github.you.{package})")
    })?;
    if parsed.is_reserved() || id.starts_with("network.dereth.") {
        return Err(format!("{id} is in a namespace kept for Dereth itself"));
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
            "https://<owner>.github.io/<repository>/plugins.json".to_owned(),
            String::new(),
        ),
    };
    Ok(vec![
        ("name", name.clone()),
        ("id", id),
        ("folder", package.clone()),
        ("command", package.replace('-', "")),
        ("api", ApiVersion::CURRENT.to_string()),
        ("dereth", DERETH_VERSION.to_owned()),
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
pub fn render(folder: &Path, o: &Options) -> Result<Vec<(String, String)>, String> {
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

/// `new`: write the template into `folder`, which must be missing or empty.
pub fn new(folder: &Path, o: &Options) -> Result<String, String> {
    if folder.exists()
        && std::fs::read_dir(folder)
            .map_err(|e| format!("{}: {e}", folder.display()))?
            .next()
            .is_some()
    {
        return Err(format!("{} is not empty", folder.display()));
    }
    let files = render(folder, o)?;
    for (path, text) in &files {
        let to = folder.join(path);
        if let Some(parent) = to.parent() {
            std::fs::create_dir_all(parent).map_err(|e| format!("{}: {e}", parent.display()))?;
        }
        std::fs::write(&to, text).map_err(|e| format!("{}: {e}", to.display()))?;
    }
    let plugin = files
        .iter()
        .find(|(p, _)| p.ends_with("/plugin.toml"))
        .map_or("", |(p, _)| p.trim_end_matches("/plugin.toml"));
    Ok(format!(
        "Made a plugin repository in {}: the plugin is {plugin}/. Make it a git repository and \
         push it to GitHub; README.md says the rest.",
        folder.display()
    ))
}
