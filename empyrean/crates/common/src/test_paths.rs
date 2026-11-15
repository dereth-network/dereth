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
//!
//! A pack that is present must be built from its world's pinned dump: [`world_pack`] and
//! [`infiltration_pack`] panic on a pack whose dataset id is another, naming the command that
//! rebuilds it. A pack built from an older release passes tests written against its row ids and
//! fails those written against the pinned release's, so it is refused before either runs.

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
///
/// # Panics
/// When the pack is present but not built from the pinned release's dump (see the module docs).
#[must_use]
pub fn world_pack() -> PathBuf {
    let path = env_or_repo("EMPYREAN_TEST_WORLD_PACK", "world.pack");
    refuse_stale(
        &path,
        &world_release::PATCHES,
        "EMPYREAN_TEST_WORLD_PACK",
        "",
    );
    path
}

/// A content pack built for the Infiltration era (ACE-World-16PY: `empyrean-import fetch --world
/// 16py --pack --out world.pack.infiltration`): `EMPYREAN_TEST_INFILTRATION_PACK`, else
/// `world.pack.infiltration`. The tests that play the February 2005 dat set read it.
///
/// # Panics
/// When the pack is present but not built from ACE-World-16PY's pinned dump.
#[must_use]
pub fn infiltration_pack() -> PathBuf {
    let path = env_or_repo("EMPYREAN_TEST_INFILTRATION_PACK", "world.pack.infiltration");
    refuse_stale(
        &path,
        &world_release::SIXTEEN_PY,
        "EMPYREAN_TEST_INFILTRATION_PACK",
        " --world 16py",
    );
    path
}

/// Where a pack's header holds its dataset id: the 16 bytes after the content hash
/// (`empyrean_content::pack::format::PackHeader`, whose own tests check the two agree).
pub const PACK_DATASET_ID_AT: usize = 0x60;

/// The dataset id in the header of the pack at `path`, lowercase hex; `None` when the file cannot
/// be read that far.
#[must_use]
pub fn pack_dataset_id(path: &Path) -> Option<String> {
    use std::io::Read;
    let mut head = [0u8; PACK_DATASET_ID_AT + 16];
    std::fs::File::open(path)
        .and_then(|mut f| f.read_exact(&mut head))
        .ok()?;
    Some(
        head[PACK_DATASET_ID_AT..]
            .iter()
            .map(|b| format!("{b:02x}"))
            .collect(),
    )
}

/// Panics when the pack at `path` is present but records a dataset id other than `world`'s pin.
/// An absent or unreadable file is left to the test that opens it.
fn refuse_stale(path: &Path, world: &world_release::WorldLine, var: &str, fetch_world: &str) {
    if world.pinned_dataset_id.is_empty() || !path.is_file() {
        return;
    }
    let Some(found) = pack_dataset_id(path) else {
        return;
    };
    if let Err(why) = stale_pack_message(path, world, var, fetch_world, &found) {
        panic!("{why}");
    }
}

/// `Ok` when `found` is `world`'s pinned dataset id; otherwise the refusal, with the rebuild
/// command.
fn stale_pack_message(
    path: &Path,
    world: &world_release::WorldLine,
    var: &str,
    fetch_world: &str,
    found: &str,
) -> Result<(), String> {
    if found == world.pinned_dataset_id {
        return Ok(());
    }
    let path = path.canonicalize().unwrap_or_else(|_| path.to_path_buf());
    Err(format!(
        "{} ({var}) was built from another dump than {} {} (dataset id {found}, not {}). Rebuild \
         it, from the workspace: cargo run --profile test-release -p empyrean-import -- fetch{fetch_world} \
         --pack --out \"{}\"",
        path.display(),
        world.repository,
        world.pinned_tag,
        world.pinned_dataset_id,
        path.display()
    ))
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

#[cfg(test)]
mod tests {
    //! Behaviour: none (checks repository tools: the stale-pack refusal)
    use super::*;

    /// A file holding a pack header's first bytes, with `id` where the dataset id goes.
    fn header_with(dir: &Path, id: [u8; 16]) -> PathBuf {
        let mut bytes = vec![0u8; PACK_DATASET_ID_AT];
        bytes.extend_from_slice(&id);
        bytes.extend_from_slice(&[0u8; 16]);
        let path = dir.join("world.pack");
        std::fs::write(&path, bytes).unwrap();
        path
    }

    fn scratch(name: &str) -> PathBuf {
        let dir =
            std::env::temp_dir().join(format!("empyrean-stale-pack-{name}-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        dir
    }

    fn pinned() -> [u8; 16] {
        let hex = world_release::PATCHES.pinned_dataset_id;
        std::array::from_fn(|i| u8::from_str_radix(&hex[2 * i..2 * i + 2], 16).unwrap())
    }

    #[test]
    fn a_pack_from_the_pinned_dump_is_taken() {
        let dir = scratch("pinned");
        let path = header_with(&dir, pinned());
        assert_eq!(
            pack_dataset_id(&path).as_deref(),
            Some(world_release::PINNED_DATASET_ID)
        );
        refuse_stale(&path, &world_release::PATCHES, "VAR", "");
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn a_pack_from_another_dump_is_refused_with_the_rebuild_command() {
        let dir = scratch("stale");
        let mut other = pinned();
        other[0] ^= 1;
        let path = header_with(&dir, other);
        let found = pack_dataset_id(&path).unwrap();
        let why = stale_pack_message(
            &path,
            &world_release::SIXTEEN_PY,
            "VAR",
            " --world 16py",
            &found,
        )
        .unwrap_err();
        let caught = std::panic::catch_unwind(|| {
            refuse_stale(&path, &world_release::PATCHES, "VAR", "");
        });
        let _ = std::fs::remove_dir_all(&dir);
        assert!(caught.is_err(), "refused");
        assert!(why.contains(&found), "{why}");
        assert!(
            why.contains(world_release::SIXTEEN_PY.pinned_dataset_id),
            "{why}"
        );
        assert!(
            why.contains("-p empyrean-import -- fetch --world 16py --pack --out"),
            "{why}"
        );
    }

    #[test]
    fn an_absent_pack_is_left_to_the_test_that_opens_it() {
        let dir = scratch("absent");
        refuse_stale(&dir.join("world.pack"), &world_release::PATCHES, "VAR", "");
        let _ = std::fs::remove_dir_all(&dir);
    }
}
