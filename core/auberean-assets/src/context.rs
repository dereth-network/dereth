//! What decoding one record needs to know from the install's other records.

use std::sync::Arc;

use auberean_dat::{DatError, DatFile};

use crate::property::PropertyList;

/// The record id of the property list in the portal file.
pub const PROPERTY_LIST_ID: u32 = 0x3400_0000;

/// The install's records that other records' layouts depend on. Cheap to clone.
#[derive(Clone, Debug, Default)]
pub struct Context {
    properties: Option<Arc<PropertyList>>,
}

/// Why a context could not be made.
#[derive(Debug)]
pub enum ContextError {
    Dat(DatError),
    Decode(auberean_dat::DecodeError),
}

impl std::fmt::Display for ContextError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Dat(e) => write!(f, "{e}"),
            Self::Decode(e) => write!(f, "property list: {e}"),
        }
    }
}

impl std::error::Error for ContextError {}

impl Context {
    /// A context that knows nothing: enough for every record that carries no properties.
    #[must_use]
    pub fn empty() -> Self {
        Self::default()
    }

    /// A context with a given property list.
    #[must_use]
    pub fn with_properties(list: PropertyList) -> Self {
        Self {
            properties: Some(Arc::new(list)),
        }
    }

    /// The context of an install, read from its portal file.
    pub fn from_portal(portal: &DatFile) -> Result<Self, ContextError> {
        let bytes = portal.read(PROPERTY_LIST_ID).map_err(ContextError::Dat)?;
        let list = crate::decode::<PropertyList>(PROPERTY_LIST_ID, &bytes, &Self::empty())
            .map_err(ContextError::Decode)?;
        Ok(Self::with_properties(list))
    }

    /// The install's property list, when the context has one.
    #[must_use]
    pub fn properties(&self) -> Option<&PropertyList> {
        self.properties.as_deref()
    }
}
