//! How each World-DB model is stored in the pack ([`Codec`](crate::pack::Codec)) and read from a
//! dump row ([`FromRow`]), plus the one derived record, [`WeenieIndex`].

mod generated;

use crate::error::ImportError;
use crate::import::mysqldump::Row;

/// A model built from one dump row. Child collections are left empty; the importer fills them.
pub trait FromRow: Sized {
    /// The dump table the model is a row of.
    const TABLE: &'static str;
    /// The columns the model reads, in model order.
    const COLUMNS: &'static [&'static str];
    fn from_row(row: &Row<'_>) -> Result<Self, ImportError>;
}

/// Implements [`FromRow`] from a `field: "column"` list; unlisted fields take their default.
#[macro_export]
#[doc(hidden)]
macro_rules! impl_from_row {
    ($ty:ty, $table:literal { $($field:ident: $col:literal),* $(,)? }) => {
        impl $crate::records::FromRow for $ty {
            const TABLE: &'static str = $table;
            const COLUMNS: &'static [&'static str] = &[$($col),*];
            #[allow(clippy::needless_update)]
            fn from_row(row: &$crate::import::mysqldump::Row<'_>) -> Result<Self, $crate::error::ImportError> {
                Ok(Self { $( $field: row.get($col)?, )* ..Default::default() })
            }
        }
    };
}

/// Derived at import, one per weenie (table [`crate::pack::TableId::WEENIE_INDEX`]): what the
/// queries that scan every weenie need, without decoding every weenie's property rows.
#[derive(Debug, Clone, PartialEq, Default)]
pub struct WeenieIndex {
    /// `weenie.class_Name`.
    pub class_name: String,
    /// `weenie.type`.
    pub r#type: i32,
    /// The first `PropertyString.Name` row, if any.
    pub name: Option<String>,
    /// The first `PropertyDataId.Spell` row, if any.
    pub spell_did: Option<u32>,
}

crate::impl_codec!(WeenieIndex {
    class_name,
    r#type,
    name,
    spell_did
});
