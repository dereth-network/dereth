//! The data files a client plays from, whatever opens them: the world's (portal, cell, high
//! resolution), the client's (those and the strings of the language to run in, in the order the
//! client opens them, with the script library linked) and the interface's records. A desktop opens
//! them from a folder; a browser from what the page reads for it.

use std::sync::Arc;

use auberean_client_sdk::dat::disk::client_files::CLIENT_VERSIONS;
use auberean_client_sdk::dat::locate::DatName;
use auberean_client_sdk::dat::DatFile;
use auberean_client_sdk::runtime::data::{DataFiles, ENGLISH};
use auberean_client_sdk::ClientData;
use auberean_scene::WorldData;
use auberean_shell::preferences::{EnumNames, Names, Preferences, LANGUAGES};
use auberean_shell::profile::Profile;

/// What is said of an install with no portal file of this game's version.
pub const NO_PORTAL: &str = "no portal file of this game's version";
/// What is said of an install with no cell file of this game's version.
pub const NO_CELL: &str = "no cell file of this game's version";

/// An install, as its host reaches it.
pub struct Install<'a> {
    /// The data files it has.
    pub found: Vec<DatName>,
    /// Open one of them; none when it will not open.
    pub open: &'a dyn Fn(&DatName) -> Option<DatFile>,
}

impl std::fmt::Debug for Install<'_> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Install")
            .field("found", &self.found)
            .finish_non_exhaustive()
    }
}

impl Install<'_> {
    /// The file `name`, when the install has it, it opens and it holds the game version this client
    /// reads.
    // D2-05172, D2-17654
    fn file(&self, name: &DatName) -> Option<DatFile> {
        let file = self
            .found
            .contains(name)
            .then(|| (self.open)(name))
            .flatten()?;
        let wanted = match name {
            DatName::Cell(_) => CLIENT_VERSIONS.game_cell,
            _ => CLIENT_VERSIONS.game_data,
        };
        let version = file.header().game_pack_vnum;
        if version != wanted {
            tracing::warn!(
                file = %name.file_name(),
                version,
                wanted,
                "a data file of another game version"
            );
            return None;
        }
        Some(file)
    }
}

/// The world's data and the client's, and the name of the language they run in: the one named,
/// else the preferences', else English (English too when the install has no strings for it).
///
/// # Errors
/// No portal or cell file of this game's version, or one whose records do not read.
// D2-00120
pub fn open_data(
    install: &Install<'_>,
    language: Option<&str>,
    preferences: &str,
) -> Result<(WorldData, ClientData, String), String> {
    let portal = install.file(&DatName::Portal).ok_or(NO_PORTAL)?;
    let cell = install.file(&DatName::Cell(1)).ok_or(NO_CELL)?;
    let ctx = auberean_assets::Context::from_portal(&portal).map_err(|e| format!("{e:?}"))?;
    let records = auberean_world::records::WorldRecords::new(
        portal,
        cell,
        install.file(&DatName::HighRes),
        ctx,
    );
    let world = WorldData::new(records)?;
    let languages = EnumNames::read(&world.records, LANGUAGES);
    let prefs = Preferences::startup(Profile::new(preferences), Names::read(&world.records));
    let mut id = auberean_shell::preferences::language(language, &prefs, &languages);
    // D2-05140, D2-12611
    let mut name = languages.name(id).unwrap_or("English").to_owned();
    if install.file(&DatName::Local(name.clone())).is_none() {
        tracing::warn!(language = %name, "no strings for the language: English");
        id = ENGLISH;
        name = "English".to_owned();
    }
    if let Some(file) = install.file(&DatName::Local(name.clone())) {
        world.records.use_language_file(file);
    }
    let portal = Arc::new(install.file(&DatName::Portal).ok_or(NO_PORTAL)?);
    let mut files = vec![Arc::clone(&portal)];
    files.extend(install.file(&DatName::Local(name.clone())).map(Arc::new));
    files.extend(install.file(&DatName::Cell(1)).map(Arc::new));
    files.extend(install.file(&DatName::HighRes).map(Arc::new));
    let program = auberean_client_sdk::runtime::link_scripts_noting(&portal, |r| {
        tracing::warn!("the script library: {r}");
    })?;
    let data = ClientData {
        files: DataFiles {
            files,
            language: id,
        },
        scripts: Some((portal, Arc::new(program))),
    };
    Ok((world, data, name))
}

/// The interface's records: the language's strings first, then the portal and high-resolution
/// files.
///
/// # Errors
/// No portal file, or one whose records do not read.
pub fn ui_assets(
    install: &Install<'_>,
    language: &str,
) -> Result<Arc<auberean_ui::DatAssets>, String> {
    let portal = install.file(&DatName::Portal).ok_or("no portal file")?;
    let ctx = auberean_assets::Context::from_portal(&portal).map_err(|e| format!("{e:?}"))?;
    let mut dats = Vec::new();
    dats.extend(install.file(&DatName::Local(language.to_owned())));
    dats.push(portal);
    dats.extend(install.file(&DatName::HighRes));
    Ok(Arc::new(auberean_ui::DatAssets::new(dats, ctx)))
}
