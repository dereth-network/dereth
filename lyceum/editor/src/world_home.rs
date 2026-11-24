//! Where the editor keeps its own files of a world: the world's folder, or for a world repository,
//! which holds nothing of the editor's but its edits and its ids, a folder of the user's cache; and
//! the land each world repository was last opened over, in the editor's settings.

use std::path::{Path, PathBuf};

use dereth_desktop::Product as _;

/// The folder the editor keeps its own files of world file `world` in: the world's own folder,
/// but for a world repository, which holds nothing of the editor's but its edits and its ids,
/// a folder of the user's cache for it (`lyceum-worlds/<folder>-<a hash of its path>`), with its
/// build in `build`.
#[must_use]
pub fn world_home(world: &Path) -> PathBuf {
    if lyceum_foundry::repo_world::is_repository(world) {
        let whole = std::path::absolute(world).unwrap_or_else(|_| world.to_path_buf());
        let folder = whole
            .parent()
            .and_then(|p| p.file_name())
            .map_or_else(|| "world".to_owned(), |n| n.to_string_lossy().into_owned());
        // FNV-1a of the path: the same folder for the same repository, run after run.
        let hash = whole
            .to_string_lossy()
            .bytes()
            .fold(0xcbf2_9ce4_8422_2325_u64, |h, b| {
                (h ^ u64::from(b)).wrapping_mul(0x0100_0000_01b3)
            });
        return dereth_client_runtime::platform::files::cache_dir()
            .unwrap_or_else(std::env::temp_dir)
            .join("lyceum-worlds")
            .join(format!("{folder}-{:08x}", hash >> 32));
    }
    world
        .parent()
        .filter(|p| !p.as_os_str().is_empty())
        .unwrap_or(Path::new("."))
        .to_path_buf()
}

/// The land a world repository was last opened over, as the editor's settings remember it.
#[must_use]
pub fn remembered_land(world: &Path) -> Option<PathBuf> {
    let file = settings_dir()?.join(LANDS_FILE);
    let t: toml::Table = toml::from_str(&std::fs::read_to_string(file).ok()?).ok()?;
    let whole = std::path::absolute(world).ok()?;
    t.get(&whole.to_string_lossy().into_owned())
        .and_then(toml::Value::as_str)
        .map(PathBuf::from)
}

/// Remember `land` as world repository `world`'s, in the editor's settings: never in the
/// repository, whose land is the server owner's own.
pub fn remember_land(world: &Path, land: &Path) {
    let Some(dir) = settings_dir() else {
        return;
    };
    let file = dir.join(LANDS_FILE);
    let mut t: toml::Table = std::fs::read_to_string(&file)
        .ok()
        .and_then(|s| toml::from_str(&s).ok())
        .unwrap_or_default();
    let (Ok(w), Ok(l)) = (std::path::absolute(world), std::path::absolute(land)) else {
        return;
    };
    t.insert(
        w.to_string_lossy().into_owned(),
        toml::Value::String(l.to_string_lossy().into_owned()),
    );
    let _ = std::fs::create_dir_all(&dir);
    let _ = std::fs::write(&file, toml::to_string(&t).unwrap_or_default());
}

/// The editor's settings file of the lands world repositories were opened over.
const LANDS_FILE: &str = "repository-lands.toml";

/// The editor's settings folder.
fn settings_dir() -> Option<PathBuf> {
    dereth_desktop::folders::default_settings_dir(crate::Lyceum::SETTINGS_DIR_NAME)
}

#[cfg(test)]
mod tests {
    //! Behaviour: none (tooling: where the editor keeps its own files of a world)
    use super::*;

    /// A world repository's files of the editor's are kept in the user's cache, in a folder of its
    /// own, the same each time; any other world's beside its world file.
    #[test]
    fn a_world_repositorys_editor_files_are_kept_out_of_it() {
        let dir = std::env::temp_dir().join(format!("lyceum-home-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(dir.join("repo")).unwrap();
        std::fs::create_dir_all(dir.join("plain")).unwrap();
        let repo = dir.join("repo/world.toml");
        std::fs::write(&repo, "format_version = 1\npacks = []\nlineage = []\n").unwrap();
        let plain = dir.join("plain/world.toml");
        std::fs::write(&plain, "key = \"plain\"\npacks = []\n").unwrap();
        let home = world_home(&repo);
        assert!(!home.starts_with(&dir), "{}", home.display());
        assert!(home.parent().is_some_and(|p| p.ends_with("lyceum-worlds")));
        assert!(home
            .file_name()
            .is_some_and(|n| n.to_string_lossy().starts_with("repo-")));
        assert_eq!(world_home(&repo), home);
        assert_eq!(world_home(&plain), dir.join("plain"));
        let _ = std::fs::remove_dir_all(&dir);
    }
}
