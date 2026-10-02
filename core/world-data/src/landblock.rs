//! The landblock and region identities the whole landscape path is indexed by, and the error the
//! scene refuses with. They live here because the land source, the interior cells and the
//! character all read them, on the client and the server alike.
//!
//! `dereth_client_runtime::landblock` re-exports this module, and keeps `block_shift` itself
//! because that is the renderer's viewer-relative space and reads the renderer's block length.

use dereth_assets::{decode_any_in, DecodedAsset, Region};
use dereth_dat::{DbType, RetailDatStore};
use dereth_primitives::DataId;

/// The retail region record. The client loads exactly one, and Dereth is it.
pub const DERETH_REGION: DataId = DataId(0x1300_0000);

/// Holtburg — the worked example throughout the knowledge base.
pub const DEFAULT_LANDBLOCK: u16 = 0xA9B4;

/// Anything the scene can refuse to build.
#[derive(Debug, thiserror::Error)]
pub enum WorldError {
    #[error("region {0}: {1}")]
    Region(DataId, String),
    #[error("landblock {0:#06X} is not in the cell dat")]
    NoSuchLandblock(u16),
    #[error(
        "the shipped region has no terrain texture merge, so there is no terrain texture to build"
    )]
    MissingTerrainTexture,
    #[error("graphics: {0}")]
    Render(String),
}

/// The decoded region description for Dereth.
///
/// # Errors
/// [`WorldError::Region`] when the record is missing or will not decode.
pub fn load_region(store: &RetailDatStore) -> Result<Region, WorldError> {
    let bytes = store
        .read_typed(DbType::Region, DERETH_REGION)
        .map_err(|e| WorldError::Region(DERETH_REGION, e.to_string()))?;
    match decode_any_in(
        store.era_of(DERETH_REGION),
        DbType::Region,
        DERETH_REGION,
        &bytes,
    )
    .map_err(|e| WorldError::Region(DERETH_REGION, e.to_string()))?
    {
        DecodedAsset::Region(r) => Ok(*Box::new(r)),
        other => Err(WorldError::Region(
            DERETH_REGION,
            format!("decoded as {other:?}"),
        )),
    }
}

/// Convert a landblock id into the `(blockX, blockY)` coordinates used throughout the landscape path.
#[must_use]
pub fn block_xy(landblock: u16) -> (i32, i32) {
    (i32::from(landblock >> 8), i32::from(landblock & 0xFF))
}

/// The cell-dat id of a landblock record: `blockId | 0xFFFF`.
#[must_use]
pub fn landblock_did(landblock: u16) -> DataId {
    DataId((u32::from(landblock) << 16) | 0xFFFF)
}

/// The cell-dat id of a landblock-information record: `blockId | 0xFFFE`.
#[must_use]
pub fn lbi_did(landblock: u16) -> DataId {
    DataId((u32::from(landblock) << 16) | 0xFFFE)
}
