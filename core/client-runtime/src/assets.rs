//! Asset store bring-up.
//!
//! This module opens the data files and hands back a
//! [`dereth_primitives::AssetSource`]. **Nothing here decodes
//! anything**: `dereth_dat::RetailDatStore` lives in `dereth-dat`, implements the seam trait
//! already, and is the role corresponding to the client's data cache.
//!
//! Two differences from the client's own bring-up, both deliberate and both recorded here rather
//! than in code that pretends otherwise:
//!
//! * The client opens `client_portal.dat` at startup and the **cell** and **high-res** dats later,
//!   when the server's region and product id are known. `RetailDatStore::open_dir` opens all four
//!   at once. Nothing tells this layer the server's region before the dats are needed, and the
//!   alternative is to reimplement the deferral against nothing.
//! * `read_only_dat_files` selects the open flags (`2 | (read_only ? 4 : 0) | ...`). The
//!   `dereth-dat` reader is read-only unconditionally, so the flag is carried in
//!   [`crate::config::Config`] and is not yet acted on. Dat *writing* and the DDD patch protocol
//!   are out of scope here.

use std::path::Path;

use dereth_dat::RetailDatStore;
use dereth_primitives::DataId;

use crate::corestrings::{display_string, ID_CANT_OPEN_DATA_FILES};

/// The retail surface the first-pixel check draws: a 256x256 DXT1 landscape texture from
/// `client_portal.dat` (`DB_TYPE_RENDERSURFACE`, `0x06` id space).
///
/// Chosen because it needs no palette — pixel format `DXT1` is copied to the GPU verbatim by
/// source-surface creation and by `decode_surface` — so the path from dat
/// bytes to a lit pixel crosses only the dat container, its `RenderSurface` decoder and the
/// renderer's block-compressed arm, with nothing of this crate's own in between.
pub const FIRST_PIXEL_SURFACE: u32 = 0x0600_378B;

/// A fatal startup failure, carrying the text the retail client would have shown.
///
/// The original startup path does not report *why* the open failed: it displays string 201 with
/// the data-store error and returns false. The `cause` is this
/// rebuild's addition, kept out of [`std::fmt::Display`] so the message stays verbatim.
#[derive(Debug, thiserror::Error)]
#[error("{}", display_string(ID_CANT_OPEN_DATA_FILES, &[]))]
pub struct DataFilesError {
    /// What actually went wrong, for a log line rather than for the dialog.
    pub cause: String,
}

/// The dat half of client database initialisation.
///
/// # Errors
/// [`DataFilesError`] — carrying corestrings 201 verbatim — when any of the three required files is
/// missing or unreadable. `client_highres.dat` is optional and is "silently skipped if the file is
/// absent", which is `RetailDatStore::open_dir`'s behaviour too.
pub fn open_data_files(dat_dir: &Path) -> Result<RetailDatStore, DataFilesError> {
    open_data_files_with(dat_dir, None)
}

/// [`open_data_files`], with the world drawn from an older dat set when `world_dat_dir` names one:
/// its `portal.dat` and `cell.dat` answer the world, and `dat_dir`'s later files the interface
/// and every record the older ones lack ([`RetailDatStore::open_pre_tod_with_later`]).
///
/// # Errors
/// [`DataFilesError`] as [`open_data_files`].
pub fn open_data_files_with(
    dat_dir: &Path,
    world_dat_dir: Option<&Path>,
) -> Result<RetailDatStore, DataFilesError> {
    if let Some(world) = world_dat_dir {
        return RetailDatStore::open_pre_tod_with_later(world, dat_dir).map_err(|e| {
            DataFilesError {
                cause: format!("{} with {}: {e}", world.display(), dat_dir.display()),
            }
        });
    }
    // A folder holding the dat set from before Throne of Destiny (`portal.dat`, `cell.dat`) and
    // no `client_portal.dat` opens as that set.
    let opened =
        if !dereth_dat::holds_retail_dats(dat_dir) && dereth_dat::holds_pre_tod_dats(dat_dir) {
            RetailDatStore::open_pre_tod_dir(dat_dir)
        } else {
            RetailDatStore::open_dir(dat_dir)
        };
    opened.map_err(|e| DataFilesError {
        cause: format!("{}: {e}", dat_dir.display()),
    })
}

/// [`open_data_files_with`], with a folder of older files beside a later world for presentation
/// alone (`--legacy-dat-dir`): its `portal.dat` from before Throne of Destiny answers the older
/// grounds and skies and nothing else ([`RetailDatStore::with_legacy_portal`]). Beside an older
/// world it is not needed, since the world's own files are the older ones, and it is not opened.
/// A folder that will not open is reported and left out: the world still opens, and the older
/// styles are refused as they are with no folder.
///
/// # Errors
/// [`DataFilesError`] as [`open_data_files`].
pub fn open_data_files_for(
    dat_dir: &Path,
    world_dat_dir: Option<&Path>,
    legacy_dat_dir: Option<&Path>,
) -> Result<RetailDatStore, DataFilesError> {
    let store = open_data_files_with(dat_dir, world_dat_dir)?;
    let Some(legacy) = legacy_dat_dir else {
        return Ok(store);
    };
    if store.era() != dereth_dat::ContainerEra::Tod {
        return Ok(store);
    }
    match store.clone().with_legacy_portal(legacy) {
        Ok(with) => {
            tracing::info!("older grounds and skies are read from {}", legacy.display());
            Ok(with)
        }
        Err(e) => {
            tracing::warn!(
                "the legacy dat folder {} will not open ({e}); the older grounds and skies \
                 are unavailable",
                legacy.display()
            );
            Ok(store)
        }
    }
}

/// Resolve the two-level enum-id map lookup for any group.
///
/// An enum lookup resolves `(enumValue, group, dbType)` in two hops: the master
/// `DidMapper` at (`0x25000000`) maps the **group** to a second mapper,
/// and that mapper maps the **enum value** to the object's `DataID`. `dereth_ui`'s
/// `DidMapperResolver` does exactly this for group 5 (`UILAYOUT`) and hard-codes the group;
/// everything else that needs it — the `ActionMap` (group 8), the two key maps (group 10), the UI
/// sound table (group 7) — needs the general form, and nobody owns it.
///
/// **Never hard-code a `DataID` at a call site.** A DDD patch can move any of these objects, and
/// hard-coding works against this dat build and no other.
///
/// | group | name | used by |
/// |------:|------|---------|
/// | 5 | `UILAYOUT` | every screen, through `dereth_ui::framework::DidMapperResolver` |
/// | 7 | `UIASSET` | the UI sound table, enum `0x10000003` |
/// | 8 | `ACTIONMAP` | input-manager startup, enum 1 |
/// | 10 | `KEYMAP` | the two key-map loads, enums 1 and `0x10000001` |
///
/// Returns `None` when either hop is missing, which the caller must treat as a failure rather than
/// as an empty result: the client's own `require` on the returned pointer is fatal.
///
/// **The body lives in `dereth_assets::did_by_enum`** and this is a forward: the
/// icon backgrounds an item slot paints need the same two hops from `dereth-ui-screens`, which
/// sits below this crate. Nothing about the lookup changed; the call sites here are unchanged.
#[must_use]
pub fn enum_did(
    assets: &dyn dereth_primitives::AssetSource,
    group: u32,
    value: u32,
) -> Option<DataId> {
    dereth_assets::did_by_enum(assets, group, value)
}

/// The master `DidMapper` where every enum lookup starts.
pub const MASTER_DID_MAPPER: DataId = dereth_assets::MASTER_DID_MAPPER;

#[cfg(test)]
mod tests {
    use super::*;
    use dereth_primitives::AssetSource;

    // Oracle: entry 201 of the corestrings.dll string table, used for the portal dat's fatal failure.
    #[test]
    fn starting_without_dats_fails_with_the_documented_text_rather_than_a_panic() {
        let err = open_data_files(Path::new("no-such-directory-anywhere"))
            .expect_err("there are no dats there");
        assert_eq!(
            err.to_string(),
            "Can't open the data files. Check that they exist and that you have permission to \
             write to them. The program will now exit."
        );
        assert!(
            err.cause.contains("no-such-directory-anywhere"),
            "{}",
            err.cause
        );
    }

    #[test]
    #[cfg_attr(
        not(feature = "retail-dats"),
        ignore = "reads the retail dats: --features retail-dats"
    )]
    fn the_four_retail_files_open_and_expose_the_seam() {
        let dir = dereth_dat::testing::dat_dir();
        assert!(
            dereth_dat::testing::have_dats(),
            "the retail dats are this test's oracle and there are none at {} -- \
             set DERETH_TEST_DAT_DIR",
            dir.display()
        );
        let store = open_data_files(&dir).expect("the retail dats open");
        assert!(
            store.headers_match_retail(),
            "the four headers are the documented ones"
        );
        assert!(store.exists(DataId(FIRST_PIXEL_SURFACE)));
    }

    // Oracle: the retail `DidMapper 0x25000000` and the four mappers it names. These are the ids the client
    // reaches through, and every one of them is resolved rather than written
    // down at a call site.
    #[test]
    #[cfg_attr(
        not(feature = "retail-dats"),
        ignore = "reads the retail dats: --features retail-dats"
    )]
    fn the_two_level_enum_lookup_finds_the_documented_objects() {
        let dir = dereth_dat::testing::dat_dir();
        assert!(
            dereth_dat::testing::have_dats(),
            "the retail dats are this test's oracle and there are none at {} -- \
             set DERETH_TEST_DAT_DIR",
            dir.display()
        );
        let store = open_data_files(&dir).expect("the retail dats open");
        let s: &dyn AssetSource = &store;
        // Input startup asks for enum value 1 in action-map group 8 and database type 0x27.
        assert_eq!(
            enum_did(s, 8, 1),
            Some(DataId(0x2600_0000)),
            "ACTIONMAP/Default"
        );
        // Keymap init adds key map 1 and key map 0x10000001.
        assert_eq!(
            enum_did(s, 10, 1),
            Some(DataId(0x1400_0002)),
            "KEYMAP/Default"
        );
        assert_eq!(
            enum_did(s, 10, 0x1000_0001),
            Some(DataId(0x1400_0000)),
            "KEYMAP/0x10000001, the default game key map"
        );
        // The UI sound table is enum value 0x10000003 in group 7 and database type 0x22.
        assert_eq!(
            enum_did(s, 7, 0x1000_0003),
            Some(DataId(0x2000_004B)),
            "UIASSET/UISoundTable"
        );
        assert_eq!(
            enum_did(s, 5, 0x1000_0001),
            Some(DataId(0x2100_0000)),
            "UILAYOUT/patch"
        );
        // A group and an entry that do not exist are `None`, not a panic and not zero.
        assert_eq!(enum_did(s, 9999, 1), None);
        assert_eq!(enum_did(s, 8, 9999), None);
        // Entry 0 of every mapper is `Undef` -> 0, which must read as absent.
        assert_eq!(enum_did(s, 10, 0), None, "an id of 0 is not a resolution");
    }
}
