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

use std::path::{Path, PathBuf};

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

/// The dat half of client database initialisation, for a world whose era the data files decide
/// and whose older set, if any, is beside the later one ([`open_world_files`] with neither).
///
/// # Errors
/// [`DataFilesError`] -- carrying corestrings 201 verbatim -- when any of the three required files is
/// missing or unreadable. `client_highres.dat` is optional and is "silently skipped if the file is
/// absent", which is `RetailDatStore::open_dir`'s behaviour too.
pub fn open_data_files(dat_dir: &Path) -> Result<RetailDatStore, DataFilesError> {
    open_world_files(dat_dir, None, None)
}

/// Where the set from before Throne of Destiny (`portal.dat`, with `cell.dat` beside it) is read
/// from: `classic_dat_dir` when it holds one, else `dat_dir` when it does, else nowhere. Naming a
/// folder only says where to look; what the client does follows from what is found.
#[must_use]
pub fn classic_set_dir(dat_dir: &Path, classic_dat_dir: Option<&Path>) -> Option<PathBuf> {
    let holds = |dir: &Path| dereth_dat::PreTodDat::Portal.in_dir(dir).is_file();
    let beside = holds(dat_dir);
    match classic_dat_dir {
        Some(classic) if holds(classic) => {
            if beside && classic != dat_dir {
                tracing::info!(
                    "the files from before Throne of Destiny are read from {} (--classic-dat-dir), \
                     not the ones in {}",
                    classic.display(),
                    dat_dir.display()
                );
            }
            Some(classic.to_path_buf())
        }
        Some(classic) => {
            tracing::warn!(
                "--classic-dat-dir {} holds no portal.dat from before Throne of Destiny{}",
                classic.display(),
                if beside {
                    format!("; the one in {} is read instead", dat_dir.display())
                } else {
                    String::new()
                }
            );
            beside.then(|| dat_dir.to_path_buf())
        }
        None => beside.then(|| dat_dir.to_path_buf()),
    }
}

/// The world's files, chosen by the dat set the world is drawn from (`world_set`) from what the
/// folders hold.
///
/// `dat_dir` holds the later set (`client_portal.dat`, `client_cell_1.dat`,
/// `client_local_English.dat`, and `client_highres.dat` when there) and may hold the set from before
/// Throne of Destiny (`portal.dat`, `cell.dat`) too: the names never collide. `classic_dat_dir`, when
/// given, is where to look for the older set instead ([`classic_set_dir`]).
///
/// - A world drawn from the files before Throne of Destiny draws from the older set, with the later files
///   beside it answering the later interface and every record the older ones lack
///   ([`RetailDatStore::open_pre_tod_with_later`]). With no later files, the older set alone.
/// - A world drawn from the later files, and one with no set named, draws the later world, with the older `portal.dat` beside
///   it for the classic interface and the older grounds, skies and object looks when one is found
///   ([`RetailDatStore::with_legacy_portal`]). An older portal that will not open is reported and
///   left out: the world still opens, and what needs it is refused as it is with none.
/// - With no set named, a `dat_dir` holding only the older set opens that set.
///
/// # Errors
/// [`DataFilesError`] as [`open_data_files`].
pub fn open_world_files(
    dat_dir: &Path,
    classic_dat_dir: Option<&Path>,
    world_set: Option<dereth_dat::ContainerEra>,
) -> Result<RetailDatStore, DataFilesError> {
    let later = dereth_dat::holds_retail_dats(dat_dir);
    let classic = classic_set_dir(dat_dir, classic_dat_dir);
    let pre_tod_world = match world_set {
        Some(set) => set == dereth_dat::ContainerEra::PreTod,
        None => !later && classic.is_some(),
    };
    if pre_tod_world {
        let Some(older) = classic.as_deref() else {
            return Err(DataFilesError {
                cause: format!(
                    "the world's era is before Throne of Destiny and no portal.dat and cell.dat \
                     were found in {}{}",
                    dat_dir.display(),
                    classic_dat_dir.map_or_else(String::new, |c| format!(" or {}", c.display()))
                ),
            });
        };
        return if later {
            RetailDatStore::open_pre_tod_with_later(older, dat_dir)
        } else {
            RetailDatStore::open_pre_tod_dir(older)
        }
        .map_err(|e| DataFilesError {
            cause: format!("{}: {e}", older.display()),
        });
    }
    let store = RetailDatStore::open_dir(dat_dir).map_err(|e| DataFilesError {
        cause: format!("{}: {e}", dat_dir.display()),
    })?;
    let Some(classic) = classic else {
        return Ok(store);
    };
    match store.clone().with_legacy_portal(&classic) {
        Ok(with) => Ok(with),
        Err(e) => {
            tracing::warn!(
                "the older portal.dat in {} will not open ({e}); the classic interface and the \
                 older grounds, skies and object looks are unavailable",
                classic.display()
            );
            Ok(store)
        }
    }
}

/// The store a run reads: the world's files from its folders ([`open_world_files`]) with the
/// world's overlay over them when there is one ([`crate::world_overlay::lay_over`]).
///
/// # Errors
/// [`DataFilesError`] as [`open_data_files`].
pub fn open_store(cfg: &crate::config::Config) -> Result<RetailDatStore, DataFilesError> {
    let set = world_set(cfg);
    let store = open_world_files(&cfg.dat_dir, cfg.classic_dat_dir.as_deref(), set)?;
    Ok(crate::world_overlay::lay_over(store, cfg))
}

/// The dat set the world is drawn from: the one its overlay was made against when the overlay
/// folder holds one, else `--world-base`, else the era's ([`dereth_primitives::EraId::container_era`]);
/// `None` when nothing names one. The era is only the default: a world made outside Dereth may
/// play an early era over the later files.
#[must_use]
pub fn world_set(cfg: &crate::config::Config) -> Option<dereth_dat::ContainerEra> {
    let from_overlay = crate::world_overlay::folder(cfg).and_then(|d| d.base_era());
    if let (Some(o), Some(b)) = (from_overlay, cfg.world_base) {
        if o != b {
            tracing::warn!(
                "the world's overlay was made against the {} files, not the {} ones --world-base \
                 names; the overlay's are read",
                set_name(o),
                set_name(b)
            );
        }
    }
    from_overlay
        .or(cfg.world_base)
        .or_else(|| cfg.era.map(dereth_primitives::EraId::container_era))
}

/// A dat set's name as `--world-base` spells it.
fn set_name(set: dereth_dat::ContainerEra) -> &'static str {
    match set {
        dereth_dat::ContainerEra::Tod => "modern",
        dereth_dat::ContainerEra::PreTod => "classic",
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

    /// What one opened store makes available, as the client decides it from the files found.
    #[derive(Debug, PartialEq, Eq)]
    struct Available {
        /// The world's set.
        world: dereth_dat::ContainerEra,
        /// The classic interface's portal (the older set, as the world or beside it).
        classic: bool,
        /// The later interface's files (the later set, as the world or beside it).
        modern: bool,
        /// The other era's object look.
        other_objects: bool,
    }

    fn available(store: &RetailDatStore) -> Available {
        let world = store.era();
        let other = match world {
            dereth_dat::ContainerEra::PreTod => dereth_dat::ContainerEra::Tod,
            dereth_dat::ContainerEra::Tod => dereth_dat::ContainerEra::PreTod,
        };
        Available {
            world,
            classic: world == dereth_dat::ContainerEra::PreTod || store.legacy_files().is_some(),
            modern: store.modern_files().is_some(),
            other_objects: store.object_files(other).is_some(),
        }
    }

    const EOR: Option<dereth_dat::ContainerEra> = Some(dereth_dat::ContainerEra::Tod);
    const INFILTRATION: Option<dereth_dat::ContainerEra> = Some(dereth_dat::ContainerEra::PreTod);

    fn both(world: dereth_dat::ContainerEra) -> Available {
        Available {
            world,
            classic: true,
            modern: true,
            other_objects: true,
        }
    }

    /// Behaviour: none (tooling: which data files open from the folders given)
    #[test]
    #[cfg_attr(
        not(feature = "retail-dats"),
        ignore = "reads the retail and February 2005 dats: --features retail-dats"
    )]
    fn both_sets_in_one_folder_make_both_interfaces_and_looks_available_and_the_era_picks_the_world(
    ) {
        let dir = dereth_dat::testing::both_sets_dir();
        for era in [None, EOR] {
            let store = open_world_files(&dir, None, era).expect("both sets open");
            assert_eq!(
                available(&store),
                both(dereth_dat::ContainerEra::Tod),
                "{era:?}"
            );
        }
        let store = open_world_files(&dir, None, INFILTRATION).expect("both sets open");
        assert_eq!(available(&store), both(dereth_dat::ContainerEra::PreTod));
    }

    /// Behaviour: none (tooling: which data files open from the folders given)
    #[test]
    #[cfg_attr(
        not(feature = "retail-dats"),
        ignore = "reads the retail and February 2005 dats: --features retail-dats"
    )]
    fn the_sets_in_two_folders_make_the_same_things_available_as_one_folder() {
        let later = dereth_dat::testing::dat_dir();
        let older = dereth_dat::testing::pre_tod_dat_dir().unwrap_or_else(|| {
            panic!(
                "{}",
                dereth_dat::testing::pre_tod_shortfall().unwrap_or_default()
            )
        });
        assert_eq!(classic_set_dir(&later, Some(&older)), Some(older.clone()));
        for era in [None, EOR] {
            let store = open_world_files(&later, Some(&older), era).expect("both sets open");
            assert_eq!(
                available(&store),
                both(dereth_dat::ContainerEra::Tod),
                "{era:?}"
            );
        }
        let store = open_world_files(&later, Some(&older), INFILTRATION).expect("both sets open");
        assert_eq!(available(&store), both(dereth_dat::ContainerEra::PreTod));
        // The named folder wins over a set beside the later one.
        let one = dereth_dat::testing::both_sets_dir();
        assert_eq!(classic_set_dir(&one, Some(&older)), Some(older.clone()));
        assert_eq!(classic_set_dir(&one, None), Some(one.clone()));
    }

    /// Behaviour: none (tooling: which data files open from the folders given)
    #[test]
    #[cfg_attr(
        not(feature = "retail-dats"),
        ignore = "reads the retail dats: --features retail-dats"
    )]
    fn the_later_set_alone_draws_the_later_world_with_no_classic_interface_or_older_looks() {
        let later = dereth_dat::testing::dat_dir();
        assert_eq!(classic_set_dir(&later, None), None);
        let alone = Available {
            world: dereth_dat::ContainerEra::Tod,
            classic: false,
            modern: true,
            other_objects: false,
        };
        for era in [None, EOR] {
            let store = open_world_files(&later, None, era).expect("the later set opens");
            assert_eq!(available(&store), alone, "{era:?}");
        }
        // An older world needs the older set, and says where it looked.
        let err = open_world_files(&later, None, INFILTRATION).expect_err("no older set");
        assert!(err.cause.contains("portal.dat"), "{}", err.cause);
    }

    /// Behaviour: none (tooling: which data files open from the folders given)
    #[test]
    #[cfg_attr(
        not(feature = "retail-dats"),
        ignore = "reads the retail dats: --features retail-dats"
    )]
    fn a_classic_folder_holding_no_older_set_changes_nothing_the_later_folder_makes_available() {
        let empty = dereth_dat::testing::ScratchDir::new("classic-dat-dir-empty").expect("scratch");
        let later = dereth_dat::testing::dat_dir();
        assert_eq!(classic_set_dir(&later, Some(empty.path())), None);
        let store = open_world_files(&later, Some(empty.path()), None).expect("the later set");
        assert_eq!(
            available(&store),
            available(&open_world_files(&later, None, None).expect("the later set"))
        );
        // Beside a later folder that holds the older set, that set is still found.
        let one = dereth_dat::testing::both_sets_dir();
        assert_eq!(classic_set_dir(&one, Some(empty.path())), Some(one.clone()));
        let store = open_world_files(&one, Some(empty.path()), None).expect("both sets");
        assert_eq!(available(&store), both(dereth_dat::ContainerEra::Tod));
    }

    /// Behaviour: none (tooling: which data files open from the folders given)
    #[test]
    fn the_overlays_base_names_the_worlds_set_over_the_switch_and_the_era() {
        use dereth_dat::overlay::{OverlayDir, OverlayWriter};
        let scratch = dereth_dat::testing::ScratchDir::new("world-set").expect("scratch");
        let mut cfg = crate::config::Config {
            connect: false,
            era: Some(dereth_primitives::EraId::Infiltration),
            ..crate::config::Config::default()
        };
        assert_eq!(
            world_set(&cfg),
            Some(dereth_dat::ContainerEra::PreTod),
            "the era's"
        );
        cfg.world_base = Some(dereth_dat::ContainerEra::Tod);
        assert_eq!(
            world_set(&cfg),
            Some(dereth_dat::ContainerEra::Tod),
            "the switch's"
        );
        // An overlay made against `portal.dat` names the older set, whatever the switch says.
        let base_path = scratch.path().join("portal.dat");
        {
            let mut w = dereth_dat::write::DatWriter::create(&base_path, 0x400, 1, 0, 0x400 * 17)
                .expect("a base");
            w.save(DataId(0x0600_0001), b"x", 1, 1, 1)
                .expect("a record");
        }
        let base = dereth_dat::DatFile::open(&base_path).expect("the base");
        let dir = OverlayDir::new(&scratch.path().join("overlay")).expect("an overlay folder");
        let mut w = OverlayWriter::open_or_create(
            &dir.container(dereth_dat::RetailDat::Portal),
            &base,
            "portal.dat",
            "a world",
            1,
        )
        .expect("the overlay");
        w.flush(1).expect("flushed");
        drop(w);
        cfg.overlay_dat_dir = Some(dir.path().to_path_buf());
        assert_eq!(
            world_set(&cfg),
            Some(dereth_dat::ContainerEra::PreTod),
            "the overlay's"
        );
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
