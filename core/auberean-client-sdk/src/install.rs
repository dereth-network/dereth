//! An install's data files opened for the client: the files it reads, in the order it opens them,
//! and the script library the server's calls run in; and the world's records and region, which the
//! player's body walks through ([`world_data`]).

use std::path::Path;
use std::sync::Arc;

use auberean_client_runtime::data::{DataFiles, ENGLISH};
use auberean_client_runtime::ClientData;
use auberean_client_sim::WorldData;
use auberean_dat::locate::{install_dats, DatName};
use auberean_dat::DatFile;

/// The install in `folder` as the client's data: the portal, English, first cell and
/// high-resolution files, in that order (those missing left out, but the portal), English its
/// language, and the portal's script library linked, `note` told when the library read right only
/// the second time.
///
/// # Errors
/// The folder holds no install, it has no portal file, or the script library could not be
/// linked.
pub fn client_data(folder: &Path, note: impl FnOnce(String)) -> Result<ClientData, String> {
    let found = install_dats(folder).map_err(|e| e.to_string())?;
    let open = |want: &dyn Fn(&DatName) -> bool| -> Option<Arc<DatFile>> {
        let (_, p) = found.iter().find(|(n, _)| want(n))?;
        DatFile::open(p).ok().map(Arc::new)
    };
    let portal = open(&|n| *n == DatName::Portal).ok_or("no portal file")?;
    let mut files = vec![Arc::clone(&portal)];
    files.extend(open(&|n| *n == DatName::Local("English".into())));
    files.extend(open(&|n| *n == DatName::Cell(1)));
    files.extend(open(&|n| *n == DatName::HighRes));
    let program = auberean_client_runtime::link_scripts_noting(&portal, |r| note(r.to_string()))?;
    Ok(ClientData {
        files: DataFiles {
            files,
            language: ENGLISH,
        },
        scripts: Some((portal, Arc::new(program))),
    })
}

/// The world of the install in `folder`: its records read from the portal, first cell and
/// high-resolution files (the last when it has one), English its language, and its region.
///
/// # Errors
/// The folder holds no install, it has no portal or cell file, or the records or the region do
/// not read.
pub fn world_data(folder: &Path) -> Result<WorldData, String> {
    let found = install_dats(folder).map_err(|e| e.to_string())?;
    let open = |want: &dyn Fn(&DatName) -> bool| -> Option<DatFile> {
        let (_, p) = found.iter().find(|(n, _)| want(n))?;
        DatFile::open(p).ok()
    };
    let portal = open(&|n| *n == DatName::Portal).ok_or("no portal file")?;
    let cell = open(&|n| *n == DatName::Cell(1)).ok_or("no cell file")?;
    let ctx = auberean_assets::Context::from_portal(&portal).map_err(|e| format!("{e:?}"))?;
    let records = auberean_world::records::WorldRecords::new(
        portal,
        cell,
        open(&|n| *n == DatName::HighRes),
        ctx,
    );
    if let Some(english) = open(&|n| *n == DatName::Local("English".into())) {
        records.use_language_file(english);
    }
    WorldData::new(records)
}
