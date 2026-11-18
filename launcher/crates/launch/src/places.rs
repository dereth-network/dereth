//! The place package of a world's era: the portal table and places `dereth-places` builds from the
//! era's public world database, which the Dereth client takes in as hints for its travel
//! (`--nav-hints`), under what the character learns in play and what the world's server
//! publishes.
//!
//! A world plays the era it says it plays ([`crate::eras`]); a world of Asheron's Call 2 has no
//! package. Each package is kept in the launcher's data folder, one folder for each era and
//! release of its database (`places/<era>-<release>/`), and built the first time a world of that
//! era is joined: the database is fetched into the per-user folder Empyrean's import tool keeps
//! world databases in (so a machine that hosts Empyrean downloads nothing twice), read, and
//! written out, in about a second once it is there.
//!
//! The pinned eras (the end of retail, Infiltration) need no network once built. The Launch
//! database follows its newest release: a package of it is used for [`LAUNCH_FRESH`] after it was
//! built before the release list is asked again, and kept on when the list does not answer.

use std::path::{Path, PathBuf};
use std::time::{Duration, SystemTime};

use dereth_places::{Fetch, PLACES_FILE, PORTALS_FILE};
use dereth_primitives::EraId;
use dereth_world_places::eras::EraDatabase;
use dereth_world_places::Source;

/// How long a package of the newest Launch release is used before the release list is asked
/// whether there is a newer one.
pub const LAUNCH_FRESH: Duration = Duration::from_secs(7 * 24 * 60 * 60);

/// The folder in the launcher's data folder that holds the packages.
#[must_use]
pub fn root(data: &Path) -> PathBuf {
    data.join("places")
}

/// The era a world's package is of: the one it plays, or none for a world of Asheron's Call 2.
#[must_use]
pub fn era_of(world: &crate::world::World) -> Option<EraId> {
    if world.game != crate::world::Game::Dereth {
        return None;
    }
    Some(
        world
            .era
            .as_deref()
            .and_then(EraId::parse)
            .unwrap_or(EraId::Eor),
    )
}

/// The folder of `era`'s package built from release `tag`.
#[must_use]
pub fn folder(root: &Path, era: EraId, tag: &str) -> PathBuf {
    root.join(format!("{}-{tag}", era.name()))
}

/// Whether `dir` holds a whole package.
fn whole(dir: &Path) -> bool {
    dir.join(PORTALS_FILE).is_file() && dir.join(PLACES_FILE).is_file()
}

/// When the package in `dir` was built.
fn built_at(dir: &Path) -> Option<SystemTime> {
    std::fs::metadata(dir.join(PLACES_FILE))
        .and_then(|m| m.modified())
        .ok()
}

/// The newest whole package of `era` in `root`, by when it was built.
fn newest(root: &Path, era: EraId) -> Option<PathBuf> {
    let prefix = format!("{}-", era.name());
    std::fs::read_dir(root)
        .ok()?
        .flatten()
        .map(|e| e.path())
        .filter(|p| {
            p.file_name()
                .and_then(|n| n.to_str())
                .is_some_and(|n| n.starts_with(&prefix))
                && whole(p)
        })
        .max_by_key(|p| built_at(p))
}

/// The package of `era` ready to use without the network: the pinned release's, or the newest
/// Launch one built within [`LAUNCH_FRESH`] of `now`.
#[must_use]
pub fn ready(root: &Path, era: EraId, now: SystemTime) -> Option<PathBuf> {
    let db = EraDatabase::of(era);
    if db.pinned() {
        return Some(folder(root, era, db.tag)).filter(|d| whole(d));
    }
    newest(root, era).filter(|d| {
        // Built after `now` (a clock set back): new.
        built_at(d)
            .map(|t| now.duration_since(t).unwrap_or(Duration::ZERO))
            .is_some_and(|age| age < LAUNCH_FRESH)
    })
}

/// The package of `era`, built into `root` first when there is none ready: its database fetched
/// into `databases` (the per-user world-database folder) through `fetch` unless it is there,
/// read, and written out. `say` hears what is being done. A Launch package the network cannot
/// renew is used as it is.
///
/// # Errors
/// There was no package and none could be built: the database could not be fetched or read, or
/// the package could not be written.
pub fn prepare(
    root: &Path,
    databases: &Path,
    era: EraId,
    fetch: &dyn Fetch,
    now: SystemTime,
    say: &mut dyn FnMut(&str),
) -> Result<PathBuf, String> {
    if let Some(dir) = ready(root, era, now) {
        return Ok(dir);
    }
    let db = EraDatabase::of(era);
    let (sql, tag) = match dereth_places::fetch_database(era, databases, fetch, say) {
        Ok(found) => found,
        Err(e) => {
            return newest(root, era).ok_or(e);
        }
    };
    let dir = folder(root, era, &tag);
    if whole(&dir) {
        // Built before from this release: used again from now.
        let _ = std::fs::File::options()
            .append(true)
            .open(dir.join(PLACES_FILE))
            .and_then(|f| f.set_modified(now));
        return Ok(dir);
    }
    say(&format!("building the places of {}", era.name()));
    let content = dereth_places::read_database(db, &sql)?;
    dereth_places::build(&content, "Dereth", Source::Hint, &dir)?;
    Ok(dir)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::cell::Cell;
    use std::io::Write;

    /// The newest Launch release, its zip holding `sql`; the release list's answers and the
    /// downloads counted, and nothing answered once `down`.
    struct Net {
        zip: Vec<u8>,
        asked: Cell<usize>,
        downloads: Cell<usize>,
        down: Cell<bool>,
    }

    impl Net {
        fn new(sql: &str) -> Self {
            let mut out = std::io::Cursor::new(Vec::new());
            let mut w = zip::ZipWriter::new(&mut out);
            w.start_file("world/world.sql", zip::write::SimpleFileOptions::default())
                .expect("a member");
            w.write_all(sql.as_bytes()).expect("written");
            w.finish().expect("finished");
            Self {
                zip: out.into_inner(),
                asked: Cell::new(0),
                downloads: Cell::new(0),
                down: Cell::new(false),
            }
        }
    }

    impl Fetch for Net {
        fn text(&self, _url: &str, _accept: &str) -> Result<String, String> {
            if self.down.get() {
                return Err("no network".into());
            }
            self.asked.set(self.asked.get() + 1);
            use sha2::Digest;
            let digest: String = sha2::Sha256::digest(&self.zip)
                .iter()
                .map(|b| format!("{b:02x}"))
                .collect();
            Ok(serde_json::json!({
                "tag_name": "v9.9.9",
                "assets": [{ "name": "empyrean-world-launch-v9.9.9.zip",
                             "digest": format!("sha256:{digest}"),
                             "browser_download_url": "https://example.invalid/world.zip" }],
            })
            .to_string())
        }
        fn bytes(&self, _url: &str, _limit: u64) -> Result<Vec<u8>, String> {
            if self.down.get() {
                return Err("no network".into());
            }
            self.downloads.set(self.downloads.get() + 1);
            Ok(self.zip.clone())
        }
    }

    fn scratch(name: &str) -> PathBuf {
        let d = std::env::temp_dir().join(format!(
            "dereth-launch-places-{name}-{}",
            std::process::id()
        ));
        let _ = std::fs::remove_dir_all(&d);
        d
    }

    /// A world database of one lifestone: enough for a package.
    const SQL: &str = "CREATE TABLE `weenie` (`class_Id` int, `class_Name` varchar(100), `type` int, `last_Modified` datetime);\n\
CREATE TABLE `landblock_instance` (`guid` int, `weenie_Class_Id` int, `obj_Cell_Id` int, `origin_X` float, `origin_Y` float, `origin_Z` float, `angles_W` float, `angles_X` float, `angles_Y` float, `angles_Z` float, `is_Link_Child` bit(1), `last_Modified` datetime);\n\
CREATE TABLE `weenie_properties_string` (`id` int, `object_Id` int, `type` smallint, `value` text);\n\
INSERT INTO `weenie` VALUES (1001,'lifestone',25,'2005-02-09 10:00:00');\n\
INSERT INTO `weenie_properties_string` VALUES (1,1001,1,'Life Stone');\n\
INSERT INTO `landblock_instance` VALUES (2,1001,2847146017,84,7.5,94.005,1,0,0,0,'\\0','2005-02-09');\n";

    /// Behaviour: none (tooling: the launcher builds a world's era's place package the first time
    /// the era is joined, uses it again without the network, and keeps a Launch package on when
    /// the release list cannot be asked)
    #[test]
    fn an_eras_package_is_built_once_used_again_and_kept_when_the_network_is_down() {
        let dir = scratch("built");
        let (root, dbs) = (dir.join("places"), dir.join("databases"));
        let net = Net::new(SQL);
        let t0 = SystemTime::now();
        let built = prepare(&root, &dbs, EraId::Launch, &net, t0, &mut |_| {}).expect("built");
        assert_eq!(built, folder(&root, EraId::Launch, "v9.9.9"));
        assert!(whole(&built), "both documents written");
        assert!(
            std::fs::read_to_string(built.join(PORTALS_FILE))
                .expect("the portal table")
                .starts_with("dereth-navigation-store"),
            "a portal table"
        );
        // Fresh: used again without asking.
        let again = prepare(&root, &dbs, EraId::Launch, &net, t0, &mut |_| {}).expect("ready");
        assert_eq!(again, built);
        assert_eq!((net.asked.get(), net.downloads.get()), (1, 1));
        // A fortnight on, the list is asked again; with no answer, the package is kept on.
        let later = t0 + Duration::from_secs(14 * 24 * 60 * 60);
        net.down.set(true);
        let kept = prepare(&root, &dbs, EraId::Launch, &net, later, &mut |_| {}).expect("kept");
        assert_eq!(kept, built);
        // With an answer, the same release: no download, and fresh again.
        net.down.set(false);
        prepare(&root, &dbs, EraId::Launch, &net, later, &mut |_| {}).expect("renewed");
        assert_eq!((net.asked.get(), net.downloads.get()), (2, 1));
        assert_eq!(ready(&root, EraId::Launch, later), Some(built));
        let _ = std::fs::remove_dir_all(&dir);
    }

    /// Behaviour: none (tooling: a world's package is of the era it plays, the end of retail when
    /// it names none, and an Asheron's Call 2 world has none)
    #[test]
    fn a_worlds_package_is_of_the_era_it_plays() {
        let mut w = crate::world::World::new("w", "W");
        assert_eq!(era_of(&w), Some(EraId::Eor));
        w.era = Some("infiltration".into());
        assert_eq!(era_of(&w), Some(EraId::Infiltration));
        w.game = crate::world::Game::Auberean;
        assert_eq!(era_of(&w), None);
    }
}
