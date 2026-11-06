//! Fetching one release's files by its tag: how a game's package gets the launcher it names. The
//! release may still be a draft its workflow is uploading, so it can be waited for; a release that
//! isn't there in time is a failure, and nothing else is ever taken in its place.

use std::path::Path;
use std::time::Duration;

use super::drafts::unfinished;
use super::files::parse_sums;
use super::github::{GitHub, Release};
use crate::package::sha256_hex;

/// How long between looks, waiting.
pub(super) const POLL: Duration = Duration::from_secs(60);

/// How many looks before giving up: longer than the launcher's release workflow can take.
pub(super) const MAX_POLLS: u32 = 200;

/// Check `bytes` against `file`'s line in the checksums `sums`.
pub(crate) fn check_sum(sums: &str, file: &str, bytes: &[u8]) -> Result<(), String> {
    let listed = parse_sums(sums)?;
    let (_, want) = listed
        .iter()
        .find(|(name, _)| name == file)
        .ok_or_else(|| format!("SHA256SUMS doesn't list {file}"))?;
    let have = sha256_hex(bytes);
    if *want == have {
        Ok(())
    } else {
        Err(format!(
            "{file} doesn't match SHA256SUMS: it lists {want}, the file is {have}"
        ))
    }
}

/// The release `tag` when it's whole, or why not.
fn look(gh: &mut GitHub<'_>, tag: &str) -> Result<Result<Release, String>, String> {
    let Some(release) = gh.releases()?.into_iter().find(|r| r.tag_name == tag) else {
        return Ok(Err("there is no such release".to_owned()));
    };
    Ok(match unfinished(gh, &release)? {
        Some(why) => Err(why),
        None => Ok(release),
    })
}

/// Write the files of the release `tag` to `dir`, each checked against its `SHA256SUMS`. With
/// `pause`, stay until the release is whole, up to [`MAX_POLLS`] looks; without it, look once.
pub(super) fn fetch(
    gh: &mut GitHub<'_>,
    tag: &str,
    dir: &Path,
    mut pause: Option<&mut dyn FnMut()>,
) -> Result<(), String> {
    let looks = if pause.is_some() { MAX_POLLS } else { 1 };
    let mut last = String::new();
    let mut found = None;
    for n in 0..looks {
        match look(gh, tag)? {
            Ok(release) => {
                found = Some(release);
                break;
            }
            Err(why) => {
                println!("{tag}: not yet: {why}");
                last = why;
            }
        }
        if n + 1 < looks {
            if let Some(p) = pause.as_mut() {
                p();
            }
        }
    }
    let Some(release) = found else {
        return Err(format!(
            "this package needs the release {tag}, and it isn't ready: {last}.\n\
             That's the launcher version in the tree, and no other is used in its place. Cut it \
             with `cargo xtask release launcher <version>`, or look at its release workflow."
        ));
    };
    let assets = gh.assets(release.id)?;
    let sums_asset = assets
        .iter()
        .find(|a| a.name == "SHA256SUMS")
        .ok_or_else(|| format!("{tag} has no SHA256SUMS"))?;
    let sums_bytes = gh.download(sums_asset)?;
    let sums = String::from_utf8_lossy(&sums_bytes).into_owned();
    std::fs::create_dir_all(dir).map_err(|e| format!("{}: {e}", dir.display()))?;
    for (name, _) in parse_sums(&sums)? {
        // `unfinished` said each is there; a name with a folder in it never is one of ours.
        if name.contains(['/', '\\']) {
            return Err(format!(
                "{tag}'s SHA256SUMS names {name}, which isn't a file name"
            ));
        }
        let asset = assets
            .iter()
            .find(|a| a.name == name)
            .ok_or_else(|| format!("{tag} lacks {name}"))?;
        let bytes = gh.download(asset)?;
        check_sum(&sums, &name, &bytes).map_err(|e| format!("{tag}: {e}"))?;
        let path = dir.join(&name);
        std::fs::write(&path, &bytes).map_err(|e| format!("{}: {e}", path.display()))?;
        println!("  {name}: {} bytes", bytes.len());
    }
    let path = dir.join("SHA256SUMS");
    std::fs::write(&path, &sums_bytes).map_err(|e| format!("{}: {e}", path.display()))?;
    println!("{tag}: the release files are in {}", dir.display());
    Ok(())
}
