//! What a package tells its launcher before the player has: the realm it opens in on a first run,
//! and the releases it updates from.
//!
//! Every package carries the same launcher. Auberean's has a small file beside it, [`FILE`] (in
//! `Dereth.app`, in `Contents/Resources`; in the AppImage, in `usr/bin` beside the launcher), that
//! names Auberean for both:
//!
//! ```json
//! {"realm": "auberean", "updates": "auberean"}
//! ```
//!
//! Dereth's package has none, which reads as Dereth for both, so a Dereth install is as it always
//! was. A file that does not read, or a word this launcher does not know, is Dereth too.
//!
//! The realm counts on a first run only, before the launcher has a state file: after that, the
//! realm the player last showed wins. The game counts on every start: the launcher keeps that game's
//! client up to date from its releases (`dereth-v*` or `auberean-v*`). The launcher itself updates
//! from its own releases whichever the game ([`crate::releases`]).

use std::path::{Path, PathBuf};

use serde::Deserialize;

use crate::state::Realm;
use crate::world::Game;

/// The file's name.
pub const FILE: &str = "launcher-defaults.json";

/// What the file says.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct Defaults {
    /// The realm a first run opens in.
    pub realm: Realm,
    /// The game whose client the launcher keeps up to date.
    pub updates: Game,
}

#[derive(Deserialize)]
struct File {
    #[serde(default)]
    realm: Option<String>,
    #[serde(default)]
    updates: Option<String>,
}

impl Defaults {
    /// Read the file's text leniently: each word this launcher does not know, and a text that does
    /// not read at all, is Dereth.
    #[must_use]
    pub fn parse(text: &str) -> Self {
        let Ok(file) = serde_json::from_str::<File>(text) else {
            return Self::default();
        };
        let game =
            |word: Option<String>| word.and_then(|w| Game::parse(w.trim())).unwrap_or_default();
        Self {
            realm: match game(file.realm) {
                Game::Dereth => Realm::Dereth,
                Game::Auberean => Realm::Auberean,
            },
            updates: game(file.updates),
        }
    }

    /// The file a launcher in the folder `launcher_dir` reads: beside it, else in the bundle's
    /// `Resources` folder above it (macOS).
    #[must_use]
    pub fn candidates(launcher_dir: &Path) -> [PathBuf; 2] {
        [
            launcher_dir.join(FILE),
            launcher_dir.join("..").join("Resources").join(FILE),
        ]
    }

    /// The file a launcher keeps in its data folder `data`, since an update replaces the bundle the
    /// packaged one is in.
    #[must_use]
    pub fn kept_file(data: &Path) -> PathBuf {
        data.join(FILE)
    }

    /// Keep what the launcher in `launcher_dir` was packaged with in its data folder `data`. A
    /// package with a client (`packaged_client`) and no such file is Dereth's, so it takes away what
    /// an earlier package of Auberean's kept; a launcher with neither has only what was kept.
    pub fn keep(launcher_dir: &Path, data: &Path, packaged_client: bool) {
        let kept = Self::kept_file(data);
        match Self::candidates(launcher_dir)
            .iter()
            .find_map(|p| std::fs::read(p).ok())
        {
            Some(bytes) => {
                let _ = std::fs::create_dir_all(data);
                let _ = std::fs::write(kept, bytes);
            }
            None if packaged_client => {
                let _ = std::fs::remove_file(kept);
            }
            None => {}
        }
    }

    /// What the launcher in `launcher_dir` was packaged with: the first file of
    /// [`Self::candidates`] there is, else what it kept in `data`; Dereth's when there is neither.
    #[must_use]
    pub fn read(launcher_dir: &Path, data: &Path) -> Self {
        Self::candidates(launcher_dir)
            .into_iter()
            .chain([Self::kept_file(data)])
            .find_map(|p| std::fs::read_to_string(p).ok())
            .map_or_else(Self::default, |text| Self::parse(&text))
    }

    /// The realm to open in: [`Self::realm`] on a first run (`first_run`), else `saved`, the one
    /// the player last showed.
    #[must_use]
    pub fn opening_realm(self, first_run: bool, saved: Realm) -> Realm {
        if first_run {
            self.realm
        } else {
            saved
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn temp(name: &str) -> PathBuf {
        let dir =
            std::env::temp_dir().join(format!("dereth-defaults-{name}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        dir
    }

    /// The file Auberean's package writes names Auberean for the realm and the updates; no file,
    /// as in Dereth's package, is Dereth for both.
    #[test]
    fn auberean_s_file_names_auberean_and_no_file_is_dereth() {
        let auberean = Defaults::parse(r#"{"realm": "auberean", "updates": "auberean"}"#);
        assert_eq!(auberean.realm, Realm::Auberean);
        assert_eq!(auberean.updates, Game::Auberean);
        let dir = temp("none");
        assert_eq!(Defaults::read(&dir, &dir.join("data")), Defaults::default());
        assert_eq!(Defaults::default().realm, Realm::Dereth);
        assert_eq!(Defaults::default().updates, Game::Dereth);
        let _ = std::fs::remove_dir_all(&dir);
    }

    /// A file that does not read, or a word this launcher does not know, falls back to Dereth
    /// rather than stopping the launcher.
    #[test]
    fn an_unreadable_file_or_an_unknown_word_is_dereth() {
        for text in ["", "not json", "[]", r#"{"realm": 3}"#] {
            assert_eq!(Defaults::parse(text), Defaults::default(), "{text}");
        }
        let odd = Defaults::parse(r#"{"realm": "ac3", "updates": "auberean", "more": true}"#);
        assert_eq!(odd.realm, Realm::Dereth);
        assert_eq!(odd.updates, Game::Auberean);
    }

    /// The file is read beside the launcher (Windows, and the AppImage's `usr/bin`), else from the
    /// bundle's `Resources` (macOS, where the launcher is in `Contents/MacOS`).
    #[test]
    fn the_file_is_read_beside_the_launcher_or_from_the_bundle_s_resources() {
        let dir = temp("beside");
        std::fs::write(
            dir.join(FILE),
            r#"{"realm":"auberean","updates":"auberean"}"#,
        )
        .unwrap();
        assert_eq!(
            Defaults::read(&dir, &dir.join("data")).updates,
            Game::Auberean
        );
        let _ = std::fs::remove_dir_all(&dir);

        let app = temp("bundle");
        let macos = app.join("Contents/MacOS");
        let resources = app.join("Contents/Resources");
        std::fs::create_dir_all(&macos).unwrap();
        std::fs::create_dir_all(&resources).unwrap();
        std::fs::write(
            resources.join(FILE),
            r#"{"realm":"auberean","updates":"auberean"}"#,
        )
        .unwrap();
        assert_eq!(
            Defaults::read(&macos, &macos.join("data")).realm,
            Realm::Auberean
        );
        let _ = std::fs::remove_dir_all(&app);
    }

    /// The packaged realm decides the first run only; after that the realm the player last showed
    /// wins, whichever package the launcher came from.
    #[test]
    fn the_packaged_realm_counts_on_a_first_run_only() {
        let auberean = Defaults {
            realm: Realm::Auberean,
            updates: Game::Auberean,
        };
        assert_eq!(auberean.opening_realm(true, Realm::Dereth), Realm::Auberean);
        assert_eq!(auberean.opening_realm(false, Realm::Dereth), Realm::Dereth);
        assert_eq!(
            Defaults::default().opening_realm(true, Realm::Auberean),
            Realm::Dereth
        );
        assert_eq!(
            Defaults::default().opening_realm(false, Realm::Auberean),
            Realm::Auberean
        );
    }

    /// A launcher release has no file of its own, and on macOS and Linux its update replaces the
    /// bundle that held it: the game the package named stays the player's game.
    #[test]
    fn what_a_package_named_outlasts_an_update_that_replaces_the_bundle() {
        let dir = temp("kept");
        let data = dir.join("data");
        let package = dir.join("package");
        std::fs::create_dir_all(&package).unwrap();
        std::fs::write(
            package.join(FILE),
            r#"{"realm":"auberean","updates":"auberean"}"#,
        )
        .unwrap();
        Defaults::keep(&package, &data, true);

        let updated = dir.join("updated");
        std::fs::create_dir_all(&updated).unwrap();
        Defaults::keep(&updated, &data, false);
        let read = Defaults::read(&updated, &data);
        assert_eq!(read.updates, Game::Auberean);
        assert_eq!(read.realm, Realm::Auberean);
    }

    /// Dereth's package has no file, so one kept from an earlier Auberean package goes; a launcher
    /// with no client either has nothing to say and takes nothing away.
    #[test]
    fn a_package_of_dereth_s_replaces_what_an_earlier_auberean_package_kept() {
        let dir = temp("replaced");
        let data = dir.join("data");
        std::fs::create_dir_all(&data).unwrap();
        std::fs::write(
            Defaults::kept_file(&data),
            r#"{"realm":"auberean","updates":"auberean"}"#,
        )
        .unwrap();
        let dereth_s = dir.join("dereth");
        std::fs::create_dir_all(&dereth_s).unwrap();
        Defaults::keep(&dereth_s, &data, false);
        assert_eq!(Defaults::read(&dereth_s, &data).updates, Game::Auberean);
        Defaults::keep(&dereth_s, &data, true);
        assert_eq!(Defaults::read(&dereth_s, &data), Defaults::default());
    }
}
