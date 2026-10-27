//! `release`, `notes` and `find`: a plugin's release made in its git repository, and found again
//! from its tag by the release workflow.
//!
//! A release sets `version` in the package's `plugin.toml`, moves the lines under `## Unreleased`
//! in the package's `CHANGES.md` under `## <version> (<date>)`, commits that as `<name> <version>`
//! and makes the annotated tag: `v<version>` in a repository of one plugin, `<id>-v<version>` in a
//! repository of several. Pushing the tag starts the template's release workflow, which finds the
//! plugin by the tag (`find`), packs it, and lists it in the repository's `plugins.json`.

use std::path::{Path, PathBuf};
use std::process::Command;

use dereth_plugin_api::manifest::PluginVersion;
use dereth_plugin_api::Manifest;
use dereth_plugin_package::MANIFEST_FILE;

/// The change log's file name in a package's folder.
pub const CHANGES_FILE: &str = "CHANGES.md";

/// The heading of the change log's section that is not released yet.
pub const UNRELEASED: &str = "## Unreleased";

/// The plugin folders of a repository, as `new` finds them before adding one and the launcher
/// finds a folder's ([`dereth_plugin_template::packages_in`]): the repository's own folder when it
/// is a plugin's, else the folders directly in it that hold one. Sorted.
pub fn packages_in(root: &Path) -> Result<Vec<PathBuf>, String> {
    dereth_plugin_template::packages_in(root).map_err(|e| e.to_string())
}

/// Whether `a` and `b` are one folder, however each is written.
fn same_folder(a: &Path, b: &Path) -> bool {
    matches!(
        (std::fs::canonicalize(a), std::fs::canonicalize(b)),
        (Ok(x), Ok(y)) if x == y
    )
}

/// The manifest of the package in `folder`.
pub fn manifest_of(folder: &Path) -> Result<Manifest, String> {
    crate::read_folder(folder)?
        .manifest()
        .map_err(|e| format!("{}: {e}", folder.display()))
}

/// `text`, a `plugin.toml`, with its top-level `version` set to `version` and nothing else
/// changed.
pub fn set_version(text: &str, version: &str) -> Result<String, String> {
    let mut out = String::with_capacity(text.len() + 8);
    let mut done = false;
    let mut in_table = false;
    for line in text.split_inclusive('\n') {
        let trimmed = line.trim_start();
        if trimmed.starts_with('[') {
            in_table = true;
        }
        let is_version = !in_table
            && !done
            && trimmed
                .strip_prefix("version")
                .is_some_and(|r| r.trim_start().starts_with('='));
        if is_version {
            let open = line
                .find('"')
                .ok_or("plugin.toml's version is not a quoted string")?;
            let close = line[open + 1..]
                .find('"')
                .map(|i| open + 1 + i)
                .ok_or("plugin.toml's version is not a quoted string")?;
            out.push_str(&line[..=open]);
            out.push_str(version);
            out.push_str(&line[close..]);
            done = true;
        } else {
            out.push_str(line);
        }
    }
    if done {
        Ok(out)
    } else {
        Err("plugin.toml has no version".into())
    }
}

/// The line index of each `## ` heading in `lines`.
fn headings(lines: &[&str]) -> Vec<usize> {
    lines
        .iter()
        .enumerate()
        .filter(|(_, l)| l.starts_with("## "))
        .map(|(i, _)| i)
        .collect()
}

/// `text`, a change log, with the lines under `## Unreleased` moved under a new
/// `## <version> (<date>)` below it, leaving `## Unreleased` empty.
pub fn roll_changes(text: &str, version: &str, date: &str) -> Result<String, String> {
    let lines: Vec<&str> = text.lines().collect();
    let heads = headings(&lines);
    let at = heads
        .iter()
        .copied()
        .find(|&i| lines[i].trim().eq_ignore_ascii_case(UNRELEASED))
        .ok_or_else(|| format!("{CHANGES_FILE} has no {UNRELEASED} section"))?;
    let end = heads
        .iter()
        .copied()
        .find(|&i| i > at)
        .unwrap_or(lines.len());
    let body: Vec<&str> = lines[at + 1..end].to_vec();
    let first = body.iter().position(|l| !l.trim().is_empty());
    let last = body.iter().rposition(|l| !l.trim().is_empty());
    let (Some(first), Some(last)) = (first, last) else {
        return Err(format!(
            "nothing under {UNRELEASED} in {CHANGES_FILE}: say what changed first"
        ));
    };
    let mut out: Vec<String> = lines[..=at].iter().map(|l| (*l).to_owned()).collect();
    out.push(String::new());
    out.push(format!("## {version} ({date})"));
    out.push(String::new());
    out.extend(body[first..=last].iter().map(|l| (*l).to_owned()));
    if end < lines.len() {
        out.push(String::new());
        out.extend(lines[end..].iter().map(|l| (*l).to_owned()));
    }
    Ok(out.join("\n") + "\n")
}

/// The section of `text`, a change log, for `version`: the lines under its `## <version>`
/// heading, trimmed. `None` when it has none.
pub fn section(text: &str, version: &str) -> Option<String> {
    let lines: Vec<&str> = text.lines().collect();
    let heads = headings(&lines);
    let at = heads.iter().copied().find(|&i| {
        lines[i][3..]
            .strip_prefix(version)
            .is_some_and(|rest| rest.is_empty() || rest.starts_with([' ', '(']))
    })?;
    let end = heads
        .iter()
        .copied()
        .find(|&i| i > at)
        .unwrap_or(lines.len());
    Some(lines[at + 1..end].join("\n").trim().to_owned())
}

/// `notes`: the change log's section for `version`, for a release's description. Empty when the
/// package has no change log.
pub fn notes(folder: &Path, version: &str) -> Result<String, String> {
    let path = folder.join(CHANGES_FILE);
    let Ok(text) = std::fs::read_to_string(&path) else {
        return Ok(String::new());
    };
    section(&text, version)
        .ok_or_else(|| format!("{} has no section for {version}", path.display()))
}

/// Run git in `dir`; its output, or what it said.
fn git(dir: &Path, args: &[&str]) -> Result<String, String> {
    let out = Command::new("git")
        .current_dir(dir)
        .args(args)
        .output()
        .map_err(|e| format!("git: {e}"))?;
    if out.status.success() {
        Ok(String::from_utf8_lossy(&out.stdout).trim().to_owned())
    } else {
        Err(format!(
            "git {}: {}",
            args.join(" "),
            String::from_utf8_lossy(&out.stderr).trim()
        ))
    }
}

/// The tag a release of `id` at `version` gets in a repository holding `plugins` plugins.
pub fn tag_for(plugins: usize, id: &str, version: &str) -> String {
    if plugins == 1 {
        format!("v{version}")
    } else {
        format!("{id}-v{version}")
    }
}

/// `release`: see the module documentation. `date` is the release's day, `YYYY-MM-DD`.
pub fn release(folder: &Path, version: &str, date: &str, push: bool) -> Result<String, String> {
    // Git runs in the plugin's folder, so every path given to it is whole: a folder named
    // relative to where the tool was started would otherwise be looked for inside itself.
    let folder = std::path::absolute(folder).map_err(|e| format!("{}: {e}", folder.display()))?;
    let folder = folder.as_path();
    let wanted = PluginVersion::parse(version)
        .ok_or_else(|| format!("{version} is not a SemVer version"))?;
    if !folder.join(MANIFEST_FILE).is_file() {
        return Err(format!("{} has no {MANIFEST_FILE}", folder.display()));
    }
    let m = manifest_of(folder)?;
    if wanted != m.version && !wanted.is_newer_than(&m.version) {
        return Err(format!(
            "{version} is not newer than the version plugin.toml has, {}",
            m.version
        ));
    }
    crate::check::check(folder)?;
    let root = PathBuf::from(git(folder, &["rev-parse", "--show-toplevel"])?);
    let plugins = packages_in(&root)?;
    if !plugins.iter().any(|p| same_folder(p, folder)) {
        return Err(format!(
            "{} is not a plugin of the repository at {} (a plugin's folder is the repository's \
             own, or a folder directly in it)",
            folder.display(),
            root.display()
        ));
    }
    if !git(folder, &["status", "--porcelain"])?.is_empty() {
        return Err("the repository has uncommitted changes: commit them first".into());
    }
    let tag = tag_for(plugins.len(), m.id.as_str(), version);
    if !git(folder, &["tag", "--list", &tag])?.is_empty() {
        return Err(format!("the tag {tag} already exists"));
    }
    let manifest_path = folder.join(MANIFEST_FILE);
    let changes_path = folder.join(CHANGES_FILE);
    let manifest_text = std::fs::read_to_string(&manifest_path)
        .map_err(|e| format!("{}: {e}", manifest_path.display()))?;
    let changes_text = std::fs::read_to_string(&changes_path).ok();
    let changes = match &changes_text {
        Some(text) => Some(roll_changes(text, version, date)?),
        None => None,
    };
    std::fs::write(&manifest_path, set_version(&manifest_text, version)?)
        .map_err(|e| format!("{}: {e}", manifest_path.display()))?;
    let mut paths = vec![manifest_path.display().to_string()];
    if let Some(text) = changes {
        std::fs::write(&changes_path, text)
            .map_err(|e| format!("{}: {e}", changes_path.display()))?;
        paths.push(changes_path.display().to_string());
    }
    let title = format!("{} {version}", m.name);
    let mut add = vec!["add", "--"];
    add.extend(paths.iter().map(String::as_str));
    let committed =
        git(folder, &add).and_then(|_| git(folder, &["commit", "--quiet", "-m", &title]));
    if let Err(e) = committed {
        // A release that could not be committed leaves the files as they were, so it can be
        // run again as soon as what stopped it is fixed.
        let mut reset = vec!["reset", "--quiet", "--"];
        reset.extend(paths.iter().map(String::as_str));
        let _ = git(folder, &reset);
        let _ = std::fs::write(&manifest_path, &manifest_text);
        if let Some(text) = &changes_text {
            let _ = std::fs::write(&changes_path, text);
        }
        return Err(e);
    }
    git(folder, &["tag", "-a", &tag, "-m", &title])?;
    let pushes = [
        vec!["push", "origin", "HEAD"],
        vec!["push", "origin", tag.as_str()],
    ];
    if push {
        for p in &pushes {
            git(folder, p)?;
        }
        return Ok(format!(
            "Released {title}: committed, tagged {tag} and pushed; the release workflow takes it \
             from there."
        ));
    }
    Ok(format!(
        "Released {title}: committed and tagged {tag}. To publish it, push both:\n  git {}\n  git {}",
        pushes[0].join(" "),
        pushes[1].join(" ")
    ))
}

/// `find`: the plugin of the repository at `repo` that the release tag `tag` names, as
/// `<folder>\t<id>\t<version>\t<name>`. The tag is `v<version>` in a repository of one plugin and
/// `<id>-v<version>` in one of several; either way the version must be the one its `plugin.toml`
/// has.
pub fn find(repo: &Path, tag: &str) -> Result<String, String> {
    let packages = packages_in(repo)?;
    let mut found = Vec::new();
    for folder in &packages {
        found.push((folder, manifest_of(folder)?));
    }
    let matched = if let Some(v) = tag
        .strip_prefix('v')
        .filter(|v| PluginVersion::parse(v).is_some())
    {
        if found.len() != 1 {
            return Err(format!(
                "{} holds {} plugins: tag each release <id>-v<version>",
                repo.display(),
                found.len()
            ));
        }
        found.pop().map(|(f, m)| (f, m, v.to_owned()))
    } else {
        tag.match_indices("-v").find_map(|(i, _)| {
            let (id, v) = (&tag[..i], &tag[i + 2..]);
            PluginVersion::parse(v)?;
            let (f, m) = found.iter().find(|(_, m)| m.id.as_str() == id)?;
            Some((*f, m.clone(), v.to_owned()))
        })
    };
    let (folder, m, version) = matched.ok_or_else(|| {
        format!(
            "no plugin in {} is released by the tag {tag}",
            repo.display()
        )
    })?;
    if m.version.to_string() != version {
        return Err(format!(
            "the tag {tag} names {version}, but {} says {}",
            folder.join(MANIFEST_FILE).display(),
            m.version
        ));
    }
    let shown = folder.display().to_string().replace('\\', "/");
    Ok(format!("{shown}\t{}\t{}\t{}", m.id, m.version, m.name))
}
