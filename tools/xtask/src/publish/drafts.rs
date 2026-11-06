//! `cargo xtask publish drafts`: publishes the drafts the release workflows leave, instead of each
//! being published by hand on GitHub.
//!
//! A draft is published only when it is whole: its `SHA256SUMS` is there and names only files it
//! holds, none of them still uploading. Only Dereth's final release becomes the repository's
//! "Latest"; every other product, and every pre-release, never does. Without `--yes` it says what
//! it would publish and changes nothing.
//!
//! `--wait` stays until the release tags (those named, else those on `main`'s head) are published,
//! asking again every minute, and gives up on a tag whose release workflow failed, naming it, so it
//! never waits on one forever.

use std::time::Duration;

use serde_json::json;

use super::files;
use super::github::{GitHub, Https, Release, Token};
use crate::package::version;

/// The tags of our products, in the order they are published.
const TAG_PREFIXES: &[&str] = &[
    "launcher-v",
    "dereth-v",
    "dereth-web-v",
    "empyrean-v",
    "lyceum-v",
    "auberean-v",
    "yalain-v",
    "athenaeum-v",
    "network-hub-v",
    "network-ace-v",
];

/// How long `--wait` pauses between looks.
const POLL: Duration = Duration::from_secs(60);

/// How many times `--wait` looks before it stops: six hours, far longer than any release takes.
pub(super) const MAX_POLLS: u32 = 360;

/// What `--wait` waits for.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(super) struct Options {
    pub tags: Vec<String>,
    pub wait: bool,
    pub yes: bool,
    pub repo: Option<String>,
}

pub(super) fn parse(args: &[String]) -> Result<Options, String> {
    let mut o = Options {
        tags: Vec::new(),
        wait: false,
        yes: false,
        repo: None,
    };
    let mut it = args.iter();
    while let Some(a) = it.next() {
        match a.as_str() {
            "--wait" => o.wait = true,
            "--yes" => o.yes = true,
            "--repo" => {
                let repo = it
                    .next()
                    .ok_or_else(|| format!("--repo needs a value\n{}", super::USAGE))?;
                super::check_repo_name(repo)?;
                o.repo = Some(repo.clone());
            }
            tag if product(tag).is_some() => o.tags.push(tag.to_owned()),
            other => {
                return Err(format!(
                    "`{other}` is not a release tag or an option\n{}",
                    super::USAGE
                ))
            }
        }
    }
    Ok(o)
}

/// The place of `tag`'s product in the publishing order, and its version; `None` for a tag that is
/// not one of our products' releases.
pub(super) fn product(tag: &str) -> Option<(usize, &str)> {
    TAG_PREFIXES.iter().enumerate().find_map(|(i, prefix)| {
        let v = tag.strip_prefix(prefix)?;
        version::parse_release_version(v).ok()?;
        Some((i, v))
    })
}

/// How `tag` is published: as a pre-release or not, and whether it becomes "Latest". Only Dereth's
/// final release does.
pub(super) fn flags(tag: &str) -> (bool, &'static str) {
    let version = product(tag).map_or("", |(_, v)| v);
    let pre = version::is_prerelease(version);
    let latest = if !pre && tag.starts_with("dereth-v") {
        "true"
    } else {
        "false"
    };
    (pre, latest)
}

/// Why the draft `release` isn't ready to publish; `None` when it is whole.
pub(super) fn unfinished(gh: &mut GitHub<'_>, release: &Release) -> Result<Option<String>, String> {
    let assets = gh.assets(release.id)?;
    if let Some(a) = assets.iter().find(|a| !a.complete()) {
        return Ok(Some(format!("{} is still uploading", a.name)));
    }
    let Some(sums) = assets.iter().find(|a| a.name == "SHA256SUMS") else {
        return Ok(Some("it has no SHA256SUMS yet".to_owned()));
    };
    let text = String::from_utf8_lossy(&gh.download(sums)?).into_owned();
    let listed = files::parse_sums(&text).map_err(|e| format!("{}: {e}", release.tag_name))?;
    Ok(listed
        .iter()
        .find(|(name, _)| !assets.iter().any(|a| a.name == *name))
        .map(|(name, _)| format!("it lacks {name}, which its SHA256SUMS lists")))
}

/// Publish the whole drafts among `only` (every product's when empty), in the publishing order;
/// with `yes` false, say what would be published and change nothing.
pub(super) fn pass(gh: &mut GitHub<'_>, only: &[String], yes: bool) -> Result<(), String> {
    let mut drafts: Vec<(usize, Release)> = gh
        .releases()?
        .into_iter()
        .filter(|r| r.draft)
        .filter(|r| only.is_empty() || only.contains(&r.tag_name))
        .filter_map(|r| {
            let place = product(&r.tag_name)?.0;
            Some((place, r))
        })
        .collect();
    drafts.sort_by(|a, b| (a.0, &a.1.tag_name).cmp(&(b.0, &b.1.tag_name)));
    for (_, release) in drafts {
        let tag = release.tag_name.clone();
        if let Some(why) = unfinished(gh, &release)? {
            println!("  {tag}: not yet: {why}");
            continue;
        }
        let (pre, latest) = flags(&tag);
        let how = match (pre, latest) {
            (true, _) => "a pre-release",
            (false, "true") => "the repository's \"Latest\" release",
            _ => "not \"Latest\"",
        };
        if yes {
            gh.update_release(
                release.id,
                &json!({ "draft": false, "prerelease": pre, "make_latest": latest }),
            )?;
            println!("  {tag}: published, {how}");
        } else {
            println!("  {tag}: would publish, {how}");
        }
    }
    Ok(())
}

/// Stay until every one of `tags` is published, or its release workflow failed: `pause` between
/// looks. Fails naming the workflows that failed, or what was still awaited after [`MAX_POLLS`].
pub(super) fn watch(
    gh: &mut GitHub<'_>,
    tags: &[String],
    pause: &mut dyn FnMut(),
) -> Result<(), String> {
    let mut waiting = tags.to_vec();
    let mut failed = Vec::new();
    for _ in 0..MAX_POLLS {
        pass(gh, &waiting, true)?;
        let releases = gh.releases()?;
        waiting.retain(|t| !releases.iter().any(|r| r.tag_name == *t && !r.draft));
        let mut still = Vec::new();
        for tag in waiting.drain(..) {
            let runs = gh.failed_runs(&tag)?;
            if runs.is_empty() {
                still.push(tag);
            } else {
                failed.extend(runs.into_iter().map(|r| format!("{tag}: {r}")));
            }
        }
        waiting = still;
        if waiting.is_empty() {
            return if failed.is_empty() {
                Ok(())
            } else {
                Err(format!(
                    "these release workflows failed, so their tags were not published:\n  {}",
                    failed.join("\n  ")
                ))
            };
        }
        println!("  waiting for {}", waiting.join(", "));
        pause();
    }
    failed.extend(waiting.iter().map(|t| format!("{t}: still not published")));
    Err(format!(
        "stopped waiting after {MAX_POLLS} looks:\n  {}",
        failed.join("\n  ")
    ))
}

/// The tags `--wait` waits for: those named, else our products' on `main`'s head.
pub(super) fn tags_to_wait_for(
    gh: &mut GitHub<'_>,
    named: &[String],
) -> Result<Vec<String>, String> {
    if !named.is_empty() {
        for tag in named {
            if gh.tag_commit(tag)?.is_none() {
                return Err(format!("there is no tag {tag} on GitHub yet"));
            }
        }
        return Ok(named.to_vec());
    }
    let head = gh.head_commit("main")?;
    let mut tags: Vec<String> = gh
        .tags_at(&head)?
        .into_iter()
        .filter(|t| product(t).is_some())
        .collect();
    tags.sort();
    if tags.is_empty() {
        return Err(format!(
            "main's head {head} has no release tag: name the tags to wait for"
        ));
    }
    Ok(tags)
}

/// `cargo xtask publish drafts ...`.
pub(super) fn run(o: &Options) -> Result<(), String> {
    let repo = super::repository(files::Kind::Dereth, o.repo.as_deref())?;
    let (token, var) = Token::from_env().ok_or_else(super::no_token)?;
    println!(
        "publish drafts on {repo}{}; the token is read from {var}",
        if o.yes { "" } else { ", a dry run" }
    );
    let mut https = Https::new(token);
    let mut gh = GitHub::new(&mut https, &repo, o.yes);
    if !o.wait {
        return pass(&mut gh, &o.tags, o.yes);
    }
    let tags = tags_to_wait_for(&mut gh, &o.tags)?;
    println!("waiting for {}", tags.join(", "));
    if !o.yes {
        println!("a dry run looks once and doesn't wait:");
        return pass(&mut gh, &tags, false);
    }
    watch(&mut gh, &tags, &mut || std::thread::sleep(POLL))
}
