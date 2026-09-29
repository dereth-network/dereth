//! Fixtures shared by the cpu tier's modules.

use dereth_primitives::{AssetError, AssetSource, DataId, DataType};

/// An asset source that holds nothing: every tree in this tier is built from a `LayoutDesc`
/// written in the test, so no read ever has to succeed.
#[derive(Debug)]
pub(crate) struct NoAssets;
impl AssetSource for NoAssets {
    fn read(&self, id: DataId) -> Result<Vec<u8>, AssetError> {
        Err(AssetError::NotFound(id))
    }
    fn exists(&self, _: DataId) -> bool {
        false
    }
    fn iter_type(&self, _: DataType) -> Box<dyn Iterator<Item = DataId> + '_> {
        Box::new(std::iter::empty())
    }
}
