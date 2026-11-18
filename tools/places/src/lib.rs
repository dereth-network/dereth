//! Builds a world's portal table and places for an era from the era's public world database,
//! fetching and caching the database first.
//!
//! The library is what the program does and what a launcher does for a world it joins.
//!
//! **Depends on** `dereth-world-places` (the documents, the dump reader and where each era's
//! database is published) and `dereth-primitives`. **Used by** the `dereth-places` program and the
//! launcher's knowledge (`dereth-launch`), which builds the places of a world's era.
//!
//! The network arrives through [`Fetch`]: the program fetches with its own HTTP client, and the
//! launcher with its own. With the `cli` feature off, nothing here brings an HTTP client.
//!
//! **Must never** write anywhere but the cache folder and the folder it is given.

use std::io::{Read, Write};
use std::path::{Path, PathBuf};

use dereth_primitives::EraId;
use dereth_world_places::eras::{EraDatabase, Form};
use dereth_world_places::{sql, Source, WorldPlaces};
use sha2::{Digest, Sha256};

/// The largest zip fetched (the releases are about 20 MB).
pub const ZIP_LIMIT: u64 = 512 * 1024 * 1024;

/// What the network is to this crate: the text at an address, or its bytes.
pub trait Fetch {
    /// The text at `url`, asked for as `accept`.
    ///
    /// # Errors
    /// The address did not answer, or answered with an error; the text says which.
    fn text(&self, url: &str, accept: &str) -> Result<String, String>;

    /// The bytes at `url`, at most `limit` of them.
    ///
    /// # Errors
    /// As [`Self::text`], or the answer was longer than `limit`.
    fn bytes(&self, url: &str, limit: u64) -> Result<Vec<u8>, String>;
}

/// The per-user folder Empyrean's import tool keeps the world databases in, from the environment:
/// `%LOCALAPPDATA%\Empyrean\world-database` on Windows, `~/Library/Caches/Empyrean/world-database`
/// on macOS, and `$XDG_CACHE_HOME/empyrean/world-database` (else `~/.cache/...`) elsewhere.
#[must_use]
pub fn cache_dir() -> Option<PathBuf> {
    let var = |name: &str| {
        std::env::var_os(name)
            .map(PathBuf::from)
            .filter(|p| p.is_absolute())
    };
    if cfg!(windows) {
        var("LOCALAPPDATA").map(|d| d.join("Empyrean").join("world-database"))
    } else if cfg!(target_os = "macos") {
        var("HOME").map(|h| h.join("Library/Caches/Empyrean/world-database"))
    } else {
        var("XDG_CACHE_HOME")
            .or_else(|| var("HOME").map(|h| h.join(".cache")))
            .map(|d| d.join("empyrean").join("world-database"))
    }
}

/// A release to fetch: its tag, its zip's address and the SHA-256 the zip must have.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Release {
    pub tag: String,
    pub url: String,
    pub sha256: String,
}

/// The release of `db` to fetch: the pinned one, or the newest its release list names.
///
/// # Errors
/// The release list did not answer, or not as expected.
pub fn release(db: &EraDatabase, fetch: &dyn Fetch) -> Result<Release, String> {
    if db.pinned() {
        return Ok(Release {
            tag: db.tag.to_owned(),
            url: db.download_url(db.tag),
            sha256: db.zip_sha256.to_owned(),
        });
    }
    let url = db.latest_url();
    let text = fetch.text(&url, "application/vnd.github+json")?;
    let v: serde_json::Value =
        serde_json::from_str(&text).map_err(|e| format!("{url}: not the answer expected: {e}"))?;
    let tag = v["tag_name"]
        .as_str()
        .ok_or_else(|| format!("{url}: the release has no tag"))?
        .to_owned();
    let asset = v["assets"]
        .as_array()
        .into_iter()
        .flatten()
        .find(|a| {
            a["name"]
                .as_str()
                .is_some_and(|n| db.is_world_asset(n, &tag))
        })
        .ok_or_else(|| format!("release {tag} of {} has no world zip", db.repository))?;
    let sha256 = asset["digest"]
        .as_str()
        .and_then(|d| d.strip_prefix("sha256:"))
        .ok_or_else(|| format!("release {tag} lists no SHA-256 for its world zip"))?
        .to_ascii_lowercase();
    let url = asset["browser_download_url"]
        .as_str()
        .ok_or_else(|| format!("release {tag}'s world zip has no address"))?
        .to_owned();
    Ok(Release { tag, url, sha256 })
}

/// The SQL of `db`'s release `tag` in `dir`, when a fetch has kept it there.
#[must_use]
pub fn cached(db: &EraDatabase, dir: &Path, tag: &str) -> Option<PathBuf> {
    Some(dir.join(db.sql_name(tag))).filter(|p| p.is_file())
}

/// The era's database in the cache `dir`, fetched first unless it is there: its path and tag.
/// `say` hears what is being done (a download, and how long it took).
///
/// # Errors
/// The release could not be found or fetched, its SHA-256 is not the one expected, or the cache
/// could not be written.
pub fn fetch_database(
    era: EraId,
    dir: &Path,
    fetch: &dyn Fetch,
    say: &mut dyn FnMut(&str),
) -> Result<(PathBuf, String), String> {
    let db = EraDatabase::of(era);
    if db.pinned() {
        if let Some(path) = cached(db, dir, db.tag) {
            return Ok((path, db.tag.to_owned()));
        }
    }
    let r = release(db, fetch)?;
    if let Some(path) = cached(db, dir, &r.tag) {
        return Ok((path, r.tag));
    }
    say(&format!("downloading {}", r.url));
    let zip = fetch.bytes(&r.url, ZIP_LIMIT)?;
    let actual: String = Sha256::digest(&zip)
        .iter()
        .map(|b| format!("{b:02x}"))
        .collect();
    if actual != r.sha256 {
        return Err(format!(
            "{} has SHA-256 {actual}, not {}: nothing is kept",
            r.url, r.sha256
        ));
    }
    let path = unzip(db, &r.tag, &zip, dir)?;
    say(&format!("kept {}", path.display()));
    Ok((path, r.tag))
}

/// The SQL member of the release zip `zip`, written into `dir` under its cache name.
///
/// # Errors
/// The bytes are not a zip holding a world database, or the cache could not be written.
pub fn unzip(db: &EraDatabase, tag: &str, zip: &[u8], dir: &Path) -> Result<PathBuf, String> {
    let mut archive = zip::ZipArchive::new(std::io::Cursor::new(zip))
        .map_err(|e| format!("the download is not a zip: {e}"))?;
    let index = (0..archive.len())
        .find(|&i| {
            archive
                .by_index(i)
                .is_ok_and(|e| e.is_file() && db.is_sql_member(e.name(), tag))
        })
        .or_else(|| {
            // A dump zip holding one SQL file under another name is that dump.
            let sql: Vec<usize> = (0..archive.len())
                .filter(|&i| {
                    archive
                        .by_index(i)
                        .is_ok_and(|e| e.is_file() && e.name().ends_with(".sql"))
                })
                .collect();
            (db.form == Form::SqlZip && sql.len() == 1).then(|| sql[0])
        })
        .ok_or_else(|| "the zip holds no world database".to_owned())?;
    std::fs::create_dir_all(dir).map_err(|e| format!("{}: {e}", dir.display()))?;
    let name = db.sql_name(tag);
    let dest = dir.join(&name);
    let partial = dir.join(format!("{name}.partial"));
    let mut entry = archive
        .by_index(index)
        .map_err(|e| format!("the zip: {e}"))?;
    // The file is closed at the end of this block, before it's renamed.
    {
        let mut out =
            std::fs::File::create(&partial).map_err(|e| format!("{}: {e}", partial.display()))?;
        std::io::copy(&mut entry, &mut out).map_err(|e| format!("{}: {e}", partial.display()))?;
        out.flush()
            .map_err(|e| format!("{}: {e}", partial.display()))?;
    }
    std::fs::rename(&partial, &dest).map_err(|e| format!("{}: {e}", dest.display()))?;
    Ok(dest)
}

/// Reads the world database at `path`: a dump, or a release zip holding one.
///
/// # Errors
/// The file could not be read, or holds no world database.
pub fn read_database(
    db: &EraDatabase,
    path: &Path,
) -> Result<dereth_world_places::WorldContent, String> {
    let is_zip = path
        .extension()
        .is_some_and(|e| e.eq_ignore_ascii_case("zip"));
    let at = |e: &dyn std::fmt::Display| format!("{}: {e}", path.display());
    if is_zip {
        let file = std::fs::File::open(path).map_err(|e| at(&e))?;
        let mut archive = zip::ZipArchive::new(file).map_err(|e| at(&e))?;
        let index = (0..archive.len())
            .find(|&i| {
                archive.by_index(i).is_ok_and(|e| {
                    e.is_file()
                        && (e.name().ends_with("/world.sql")
                            || e.name() == "world.sql"
                            || (db.form == Form::SqlZip && e.name().ends_with(".sql")))
                })
            })
            .ok_or_else(|| at(&"the zip holds no world database"))?;
        let mut text = String::new();
        archive
            .by_index(index)
            .map_err(|e| at(&e))?
            .read_to_string(&mut text)
            .map_err(|e| at(&e))?;
        return sql::read_str(&text).map_err(|e| at(&e));
    }
    let file = std::fs::File::open(path).map_err(|e| at(&e))?;
    sql::read(std::io::BufReader::with_capacity(1 << 20, file)).map_err(|e| at(&e))
}

/// The names of a place package's two documents in its folder.
pub const PORTALS_FILE: &str = "portals.tsv";
pub const PLACES_FILE: &str = "places.tsv";

/// What [`build`] wrote.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Built {
    pub portals: usize,
    /// Of them, the portals spawned rather than placed.
    pub spawned: usize,
    pub places: usize,
    pub towns: usize,
    pub landmarks: usize,
}

/// Builds the portal table and places of the world database `content` for the world `world`,
/// their knowledge from `source`, into the folder `out` ([`PORTALS_FILE`], [`PLACES_FILE`]). Each
/// document is written beside its name and then put in place.
///
/// # Errors
/// The folder or a document could not be written.
pub fn build(
    content: &dereth_world_places::WorldContent,
    world: &str,
    source: Source,
    out: &Path,
) -> Result<Built, String> {
    let places = WorldPlaces::of(content);
    let docs = places.documents(world, source);
    std::fs::create_dir_all(out).map_err(|e| format!("{}: {e}", out.display()))?;
    for (name, text) in [(PORTALS_FILE, &docs.portals), (PLACES_FILE, &docs.places)] {
        let p = out.join(name);
        let part = out.join(format!("{name}.partial"));
        std::fs::write(&part, text).map_err(|e| format!("{}: {e}", part.display()))?;
        std::fs::rename(&part, &p).map_err(|e| format!("{}: {e}", p.display()))?;
    }
    let spawned = places
        .portals
        .iter()
        .filter(|p| {
            matches!(
                p.standing,
                dereth_world_places::documents::Standing::Spawned(_)
            )
        })
        .count();
    let towns = places
        .towns
        .iter()
        .filter(|t| t.kind == dereth_world_places::towns::TownKind::Town)
        .count();
    Ok(Built {
        portals: places.portals.len(),
        spawned,
        places: places.places.len(),
        towns,
        landmarks: places.towns.len() - towns,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::cell::Cell;

    /// A release list naming one Launch world zip, and the zip; the downloads counted.
    struct Fake {
        list: String,
        zip: Vec<u8>,
        downloads: Cell<usize>,
    }

    impl Fetch for Fake {
        fn text(&self, _url: &str, _accept: &str) -> Result<String, String> {
            Ok(self.list.clone())
        }
        fn bytes(&self, _url: &str, _limit: u64) -> Result<Vec<u8>, String> {
            self.downloads.set(self.downloads.get() + 1);
            Ok(self.zip.clone())
        }
    }

    fn world_zip(sql: &str) -> Vec<u8> {
        let mut out = std::io::Cursor::new(Vec::new());
        let mut w = zip::ZipWriter::new(&mut out);
        w.start_file("world/world.sql", zip::write::SimpleFileOptions::default())
            .expect("a member");
        w.write_all(sql.as_bytes()).expect("written");
        w.finish().expect("finished");
        out.into_inner()
    }

    fn fake(zip: Vec<u8>, digest: &str) -> Fake {
        let list = serde_json::json!({
            "tag_name": "v9.9.9",
            "assets": [
                { "name": "empyrean-world-launch-v9.9.9-source.zip", "digest": "sha256:00",
                  "browser_download_url": "https://example.invalid/source.zip" },
                { "name": "empyrean-world-launch-v9.9.9.zip", "digest": format!("sha256:{digest}"),
                  "browser_download_url": "https://example.invalid/world.zip" },
            ],
        });
        Fake {
            list: list.to_string(),
            zip,
            downloads: Cell::new(0),
        }
    }

    fn scratch(name: &str) -> PathBuf {
        let d = std::env::temp_dir().join(format!("dereth-places-{name}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&d);
        d
    }

    /// Behaviour: none (tooling: the newest Launch release is downloaded once, checked against the
    /// SHA-256 its release lists, and its world database kept in the cache)
    #[test]
    fn the_newest_launch_release_is_checked_kept_and_not_fetched_again() {
        let dir = scratch("kept");
        let zip = world_zip("-- the world\n");
        let digest: String = Sha256::digest(&zip)
            .iter()
            .map(|b| format!("{b:02x}"))
            .collect();
        let net = fake(zip, &digest);
        let (path, tag) = fetch_database(EraId::Launch, &dir, &net, &mut |_| {}).expect("fetched");
        assert_eq!(tag, "v9.9.9");
        assert_eq!(path, dir.join("empyrean-world-launch-v9.9.9.sql"));
        assert_eq!(
            std::fs::read_to_string(&path).expect("kept"),
            "-- the world\n"
        );
        let again = fetch_database(EraId::Launch, &dir, &net, &mut |_| {}).expect("cached");
        assert_eq!(again.0, path);
        assert_eq!(net.downloads.get(), 1, "the kept copy is used");
        let _ = std::fs::remove_dir_all(&dir);
    }

    /// Behaviour: none (tooling: a download whose SHA-256 is not the release's is not kept)
    #[test]
    fn a_download_that_is_not_the_release_it_says_keeps_nothing() {
        let dir = scratch("refused");
        let net = fake(world_zip("-- the world\n"), &"0".repeat(64));
        let e = fetch_database(EraId::Launch, &dir, &net, &mut |_| {}).expect_err("refused");
        assert!(e.contains("nothing is kept"), "{e}");
        assert!(!dir.join("empyrean-world-launch-v9.9.9.sql").exists());
        let _ = std::fs::remove_dir_all(&dir);
    }
}
