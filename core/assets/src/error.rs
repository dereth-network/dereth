//! The one error enum for this crate.

use dereth_dat::DatError;
use dereth_primitives::DataId;

/// Why a dat object could not be decoded.
#[derive(Debug, thiserror::Error)]
#[non_exhaustive]
pub enum AssetError {
    /// A cursor failure: an overrun, a shortfall, or a container problem.
    #[error(transparent)]
    Dat(#[from] DatError),
    /// Every payload starts with its own id.
    #[error("payload declares id {found}, directory says {expected}")]
    IdEchoMismatch { expected: DataId, found: DataId },
    /// A tagged union carried a tag the client's own switch does not have a case for, in a place
    /// where the client would have failed too.
    #[error("{what}: unknown tag {tag:#X}")]
    UnknownTag { what: &'static str, tag: u32 },
    /// A field whose only shipped value is known, seen with another value.
    #[error("{what}: unsupported value {value}")]
    Unsupported { what: &'static str, value: u32 },
    /// `divine_type` returned nothing, or nothing in this crate decodes that type yet.
    #[error("no decoder for {0:?}")]
    NoDecoder(dereth_dat::DbType),
    #[error("{0} does not resolve to any of the four dats")]
    NotFound(DataId),
}

impl From<AssetError> for dereth_primitives::AssetError {
    fn from(e: AssetError) -> Self {
        match e {
            AssetError::Dat(d) => d.into(),
            AssetError::NotFound(id) => dereth_primitives::AssetError::NotFound(id),
            AssetError::IdEchoMismatch { expected, found } => {
                dereth_primitives::AssetError::Malformed {
                    id: expected,
                    reason: format!("payload declares id {found}"),
                }
            }
            other => dereth_primitives::AssetError::Malformed {
                id: DataId(0),
                reason: other.to_string(),
            },
        }
    }
}
