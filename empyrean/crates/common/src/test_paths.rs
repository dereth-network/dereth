//! Not ACE: where the tests and tools find inputs that are not committed fixtures. Every path is
//! its documented environment variable, else a place inside the Cargo workspace, so no host path
//! is hard-coded and the same defaults work on every host.
//!
//! | input | variable | default (workspace-relative) |
//! |---|---|---|
//! | `world.pack` | `EMPYREAN_TEST_WORLD_PACK` | `world.pack` |
//! | an Infiltration `world.pack` | `EMPYREAN_TEST_INFILTRATION_PACK` | `world.pack.infiltration` |
//! | ACE world-database dump | `EMPYREAN_WORLD_SQL` | `world-database/ACE-World-Database-<pin>.sql`, else the fetch cache |
//! | reassembled messages | `DERETH_TEST_NETBLOBS` | `fixtures/message-corpus` |
//! | recorded sessions | `DERETH_TEST_CAPTURES` | `fixtures/packet-captures` |
//!
//! `<pin>` is [`world_release::PINNED_TAG`]. The fetch cache is the per-user folder
//! `empyrean-import fetch` downloads the pinned release into ([`world_release::cache_dir`]), so a
//! machine that has fetched once runs the real-dump tests with no variable set.
//!
//! The retail dats are not here: every test, client and server alike, finds them through
//! `dereth_dat::testing` (`DERETH_TEST_DAT_DIR`).
//!
//! `world.pack` and `world-database/` are ignored by git, so a linked git worktree usually lacks
//! them: an ignored default that is absent here is taken from the main checkout
//! when it exists there. The real-content tier fails, naming the variable, when an input is absent.

use std::path::{Path, PathBuf};

use crate::world_release;

/// The Cargo workspace root (three levels above this crate's manifest: `empyrean/crates/common`).
#[must_use]
pub fn repo_root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../..")
}

/// The checkout holding the workspace: the nearest directory at or above [`repo_root`] with a
/// `.git` (a folder in a main checkout, a file in a linked worktree).
fn checkout_root() -> Option<PathBuf> {
    let root = repo_root().canonicalize().ok()?;
    root.ancestors()
        .find(|d| d.join(".git").exists())
        .map(Path::to_path_buf)
}

/// The workspace in the main checkout when this checkout is a linked git worktree (its `.git` is
/// a file naming the worktree's git directory, whose `commondir` is the main `.git`), else `None`.
#[must_use]
pub fn main_checkout() -> Option<PathBuf> {
    let checkout = checkout_root()?;
    let text = std::fs::read_to_string(checkout.join(".git")).ok()?;
    let gitdir = PathBuf::from(text.trim().strip_prefix("gitdir:")?.trim());
    let gitdir = if gitdir.is_absolute() {
        gitdir
    } else {
        checkout.join(gitdir)
    };
    let common = std::fs::read_to_string(gitdir.join("commondir")).ok()?;
    let common = PathBuf::from(common.trim());
    let common = if common.is_absolute() {
        common
    } else {
        gitdir.join(common)
    };
    // `commondir` is usually `../..`: resolve it before taking the parent, or the parent of the
    // unresolved path is a folder inside `.git`.
    let common = common.components().fold(PathBuf::new(), |mut out, c| {
        match c {
            std::path::Component::CurDir => {}
            std::path::Component::ParentDir => {
                out.pop();
            }
            other => out.push(other),
        }
        out
    });
    // The workspace sits at the same place below the main checkout as below this one.
    let below = repo_root()
        .canonicalize()
        .ok()?
        .strip_prefix(checkout.canonicalize().ok()?)
        .ok()?
        .to_path_buf();
    common.parent().map(|main| main.join(below))
}

/// `$var` if set and non-empty, else `default` under the workspace root (or under the main
/// checkout's workspace, when only that one has it).
#[must_use]
pub fn env_or_repo(var: &str, default: &str) -> PathBuf {
    if let Some(v) = std::env::var_os(var).filter(|v| !v.is_empty()) {
        return PathBuf::from(v);
    }
    let here = repo_root().join(default);
    if present(&here) {
        return here;
    }
    main_checkout()
        .map(|m| m.join(default))
        .filter(|p| present(p))
        .unwrap_or(here)
}

/// A file, or a directory with something in it (a worktree's ignored folders exist but are empty).
fn present(p: &Path) -> bool {
    p.is_file() || std::fs::read_dir(p).is_ok_and(|mut d| d.next().is_some())
}

/// The content pack: `EMPYREAN_TEST_WORLD_PACK`, else `world.pack`.
#[must_use]
pub fn world_pack() -> PathBuf {
    env_or_repo("EMPYREAN_TEST_WORLD_PACK", "world.pack")
}

/// A content pack built for the Infiltration era (ACE-World-16PY: `empyrean-import fetch --world
/// 16py --pack --out world.pack.infiltration`): `EMPYREAN_TEST_INFILTRATION_PACK`, else
/// `world.pack.infiltration`. The tests that play the February 2005 dat set read it.
#[must_use]
pub fn infiltration_pack() -> PathBuf {
    env_or_repo("EMPYREAN_TEST_INFILTRATION_PACK", "world.pack.infiltration")
}

/// ACE's world-database dump: `EMPYREAN_WORLD_SQL`, else `world-database/<the pinned dump>`,
/// else the pinned dump in the fetch cache when `empyrean-import fetch` has put it there.
#[must_use]
pub fn world_sql() -> PathBuf {
    const VAR: &str = "EMPYREAN_WORLD_SQL";
    let default = format!(
        "world-database/{}",
        world_release::sql_name(world_release::PINNED_TAG)
    );
    let found = env_or_repo(VAR, &default);
    if found.is_file() || std::env::var_os(VAR).is_some_and(|v| !v.is_empty()) {
        return found;
    }
    world_release::cached_sql(world_release::PINNED_TAG).unwrap_or(found)
}

/// The reassembled messages: `DERETH_TEST_NETBLOBS`, else `fixtures/message-corpus`.
#[must_use]
pub fn netblobs() -> PathBuf {
    env_or_repo("DERETH_TEST_NETBLOBS", "fixtures/message-corpus")
}

/// The recorded sessions: `DERETH_TEST_CAPTURES`, else `fixtures/packet-captures`.
#[must_use]
pub fn captures() -> PathBuf {
    env_or_repo("DERETH_TEST_CAPTURES", "fixtures/packet-captures")
}
