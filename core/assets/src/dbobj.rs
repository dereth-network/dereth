//! The `DBObj` header: the DataID echo and the categorized extra dword.
//!
//! The header the client reads at the head of a payload:
//!
//! ```text
//! align to 4; u32 id
//! if the payload type is categorized:
//!     align to 4; u32 data_category
//! ```
//!
//! Both 4-byte alignment steps are inert — the dat archive is not word-aligned —
//! so this is simply one or two little-endian dwords at the head of the payload.
//!
//! Exactly `SURFACETEXTURE` (`0x05`), `RENDERSURFACE` (`0x06`/`0x07`) and
//! `RENDERTEXTURE` (`0x15`) are categorized. The id must echo.
//!
//! Decoding preserves each record's type dispatch and echoed id.

use dereth_dat::{Cursor, DbType};
use dereth_primitives::DataId;

use crate::error::AssetError;

/// The `DBObj` header of one payload.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct DbObjHeader {
    pub id: DataId,
    /// The data category, present for the three categorized types only.
    pub data_category: Option<u32>,
}

/// Read the header for a type.
pub fn read_dbobj_header(c: &mut Cursor<'_>, kind: DbType) -> Result<DbObjHeader, AssetError> {
    let id = c.data_id()?;
    let data_category = if kind.is_categorized() {
        Some(c.u32()?)
    } else {
        None
    };
    Ok(DbObjHeader { id, data_category })
}

/// The payload's leading DataID must equal the id it was fetched under.
///
/// The client fails the load when the payload's leading id is not the id it was fetched under.
pub fn check_id_echo(expected: DataId, found: DataId) -> Result<(), AssetError> {
    if expected == found {
        Ok(())
    } else {
        Err(AssetError::IdEchoMismatch { expected, found })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Oracle: the first eight bytes of a representative `0xA9B4FFFF` payload.
    #[test]
    fn an_uncategorized_header_is_one_dword() {
        let buf = [0xFF, 0xFF, 0xB4, 0xA9, 0x01, 0x00, 0x00, 0x00];
        let mut c = Cursor::new(&buf);
        let h = read_dbobj_header(&mut c, DbType::LandBlock).unwrap();
        assert_eq!(h.id, DataId(0xA9B4_FFFF));
        assert_eq!(h.data_category, None);
        assert_eq!(c.position(), 4);
    }

    /// Contract 9.7: getting this wrong shifts every texture header by four bytes.
    #[test]
    fn a_categorized_header_is_two_dwords() {
        let mut buf = Vec::new();
        buf.extend_from_slice(&0x0600_0133u32.to_le_bytes());
        buf.extend_from_slice(&6u32.to_le_bytes());
        let mut c = Cursor::new(&buf);
        let h = read_dbobj_header(&mut c, DbType::RenderSurface).unwrap();
        assert_eq!(h.id, DataId(0x0600_0133));
        assert_eq!(h.data_category, Some(6));
        assert_eq!(c.position(), 8);
    }

    #[test]
    fn the_id_echo_check_fires_on_a_mismatch() {
        assert!(check_id_echo(DataId(1), DataId(1)).is_ok());
        assert!(matches!(
            check_id_echo(DataId(1), DataId(2)),
            Err(AssetError::IdEchoMismatch { .. })
        ));
    }
}
