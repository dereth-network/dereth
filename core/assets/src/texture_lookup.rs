//! Resolve texture record chains and their palette records without interpreting pixels.

use crate::{Decode, Palette, RenderSurface, RenderTexture, Surface, SurfaceTexture};
use dereth_dat::{divine_type, ContainerEra, DbType, RetailDatStore};
use dereth_primitives::DataId;

/// A missing, malformed or non-texture record in a texture chain.
#[derive(Debug, thiserror::Error)]
pub enum LookupError {
    #[error("{0}: {1}")]
    Dat(DataId, dereth_dat::DatError),
    #[error("{0}: {1}")]
    Asset(DataId, crate::AssetError),
    #[error("{0} is not a texture id, or names an empty level chain")]
    NotATexture(DataId),
}

/// The record lookup shared by products that display images from the data files.
#[derive(Debug)]
pub struct TextureLookup<'a> {
    store: &'a RetailDatStore,
    keep_high_detail: bool,
}

impl<'a> TextureLookup<'a> {
    /// Select the high-resolution level only when granted and detail is zero.
    #[must_use]
    pub fn new(store: &'a RetailDatStore, detail: u32) -> Self {
        Self {
            store,
            keep_high_detail: store.highres_granted() && detail == 0,
        }
    }

    /// Whether a two-level chain retains its high-resolution level.
    #[must_use]
    pub fn keeps_high_detail(&self) -> bool {
        self.keep_high_detail
    }

    /// Read one palette record without expanding its colours.
    ///
    /// # Errors
    /// A missing or malformed palette record.
    pub fn palette(&self, id: DataId) -> Result<Palette, LookupError> {
        self.decode(DbType::Palette, id)
    }

    /// Follow the id chain to the `RenderSurface` that actually carries pixels.
    ///
    /// # Errors
    /// [`LookupError`] when a hop is missing or will not decode.
    pub fn resolve(&self, id: DataId) -> Result<(DataId, RenderSurface, Vec<u8>), LookupError> {
        let kind = divine_type(id).ok_or(LookupError::NotATexture(id))?;
        match kind {
            DbType::Surface => {
                let s: Surface = self.decode(DbType::Surface, id)?;
                let next = s.orig_texture_id.ok_or(LookupError::NotATexture(id))?;
                self.resolve(next)
            }
            // Before Throne of Destiny an image texture carries its own pixels.
            DbType::SurfaceTexture if self.store.era() == ContainerEra::PreTod => {
                let bytes = self.read(DbType::SurfaceTexture, id)?;
                let rs = RenderSurface::from_pre_tod_texture(id, &bytes)
                    .map_err(|e| LookupError::Asset(id, e))?;
                Ok((id, rs, bytes))
            }
            DbType::SurfaceTexture => {
                let t: SurfaceTexture = self.decode(DbType::SurfaceTexture, id)?;
                // one level is that level; two levels are
                // `[1]` (the original) when high detail should be dropped and `[0]`
                // (the high-res partition) otherwise; any other count is the data error
                // "Cannot get surface DID, no source levels are listed!" and `INVALID_DID`.
                let next = match t.source_levels.as_slice() {
                    [one] => *one,
                    [high, original] => {
                        if self.keep_high_detail {
                            *high
                        } else {
                            *original
                        }
                    }
                    _ => return Err(LookupError::NotATexture(id)),
                };
                self.resolve(next)
            }
            DbType::RenderTexture => {
                let t: RenderTexture = self.decode(DbType::RenderTexture, id)?;
                let next = *t
                    .source_levels
                    .first()
                    .ok_or(LookupError::NotATexture(id))?;
                self.resolve(next)
            }
            DbType::RenderSurface => {
                let bytes = self.read(DbType::RenderSurface, id)?;
                let rs = RenderSurface::decode_payload_in(self.store.era(), id, &bytes)
                    .map_err(|e| LookupError::Asset(id, e))?;
                Ok((id, rs, bytes))
            }
            _ => Err(LookupError::NotATexture(id)),
        }
    }

    fn read(&self, kind: DbType, id: DataId) -> Result<Vec<u8>, LookupError> {
        self.store
            .read_typed(kind, id)
            .map_err(|e| LookupError::Dat(id, e))
    }

    fn decode<T: Decode>(&self, kind: DbType, id: DataId) -> Result<T, LookupError> {
        let bytes = self.read(kind, id)?;
        T::decode_payload_in(self.store.era(), id, &bytes).map_err(|e| LookupError::Asset(id, e))
    }
}
