//! CPU preparation performed after a backend's texture-cache lookup.

use std::borrow::Cow;

use crate::RenderError;
use dereth_primitives::TextureData;

/// The source chain and upload-owner policy, before allocating device resources.
#[derive(Debug)]
pub(crate) struct PreparedUpload<'a> {
    texture: Cow<'a, TextureData>,
    imgtex: bool,
}

impl<'a> PreparedUpload<'a> {
    /// Some backends reject dimensions before converting a compressed chain; others first
    /// select or decode the device format and validate dimensions at that boundary.
    pub(crate) fn new(
        texture: &'a TextureData,
        imgtex: bool,
        dimensions_first: bool,
    ) -> Result<Self, RenderError> {
        let mut prepared = Self {
            texture: Cow::Borrowed(texture),
            imgtex,
        };
        if dimensions_first {
            prepared.validate_dimensions()?;
        }
        if imgtex {
            if let Some(chain) = crate::mip::compressed_system_chain(texture)? {
                prepared.texture = Cow::Owned(chain);
            }
        }
        Ok(prepared)
    }

    pub(crate) fn texture(&self) -> &TextureData {
        &self.texture
    }

    #[cfg(feature = "vulkan")]
    pub(crate) fn decode_blocks(&mut self) -> Result<(), RenderError> {
        self.texture = Cow::Owned(crate::texture::decode_block_chain(&self.texture)?);
        Ok(())
    }

    pub(crate) fn validate_dimensions(&self) -> Result<(), RenderError> {
        let t = self.texture();
        if t.width == 0 || t.height == 0 || t.levels.is_empty() {
            return Err(RenderError::BadDimensions {
                width: t.width,
                height: t.height,
                reason: "a texture needs a non-zero extent and at least one level",
            });
        }
        Ok(())
    }

    pub(crate) fn wanted_levels(&self, autogen: bool) -> usize {
        if self.imgtex {
            crate::mip::runtime_level_count(self.texture(), autogen)
        } else {
            self.texture.levels.len()
        }
    }

    #[cfg(any(feature = "vulkan", all(windows, feature = "d3d12")))]
    pub(crate) fn native_levels(&self, autogen: bool) -> Result<(u16, bool), RenderError> {
        let levels = u16::try_from(self.wanted_levels(autogen))
            .map_err(|_| RenderError::Unsupported("too many provided texture levels"))?;
        Ok((levels, usize::from(levels) > self.texture.levels.len()))
    }

    #[cfg(feature = "wgpu")]
    pub(crate) fn web_levels(wanted: usize, available: usize, rgba: bool) -> (usize, u32, bool) {
        let provided = available.min(wanted.max(1));
        let count = u32::try_from(wanted.max(provided).max(1)).unwrap_or(1);
        let generate = rgba && count as usize > provided;
        let count = if generate {
            count
        } else {
            u32::try_from(provided).unwrap_or(1)
        };
        (provided, count, generate)
    }
}

#[cfg(test)]
mod tests {
    //! Behaviour: none (backend preparation ordering and integer conversion policies).
    use super::*;
    use dereth_primitives::TextureFormat;

    #[test]
    fn dimension_validation_can_precede_conversion_or_follow_format_selection() {
        let source = TextureData {
            width: 0,
            height: 4,
            format: TextureFormat::Bc1,
            levels: vec![vec![]],
        };
        assert!(matches!(
            PreparedUpload::new(&source, true, true),
            Err(RenderError::BadDimensions {
                reason: "a texture needs a non-zero extent and at least one level",
                ..
            })
        ));
        let early = PreparedUpload::new(&source, true, false).unwrap_err();
        assert!(!matches!(
            early,
            RenderError::BadDimensions {
                reason: "a texture needs a non-zero extent and at least one level",
                ..
            }
        ));
        let borrowed = PreparedUpload::new(&source, false, false).unwrap();
        assert!(std::ptr::eq(borrowed.texture(), &source));
        assert!(matches!(
            borrowed.validate_dimensions(),
            Err(RenderError::BadDimensions { .. })
        ));
    }

    #[cfg(any(feature = "vulkan", all(windows, feature = "d3d12")))]
    #[test]
    fn native_levels_keep_autogen_and_checked_provided_chain_limits() {
        let mut source = TextureData {
            width: 16,
            height: 8,
            format: TextureFormat::Bgra8,
            levels: vec![vec![0; 512]],
        };
        let upload = PreparedUpload::new(&source, true, false).unwrap();
        assert_eq!(upload.native_levels(true).unwrap(), (5, true));
        assert_eq!(upload.native_levels(false).unwrap(), (1, false));
        assert_eq!(
            PreparedUpload::new(&source, false, false)
                .unwrap()
                .native_levels(true)
                .unwrap(),
            (1, false)
        );
        source.levels = vec![Vec::new(); usize::from(u16::MAX) + 1];
        assert!(matches!(
            PreparedUpload::new(&source, false, false)
                .unwrap()
                .native_levels(true),
            Err(RenderError::Unsupported("too many provided texture levels"))
        ));
    }

    #[cfg(feature = "wgpu")]
    #[test]
    fn web_levels_keep_minimum_count_and_rgba_only_generation() {
        assert_eq!(PreparedUpload::web_levels(5, 1, true), (1, 5, true));
        assert_eq!(PreparedUpload::web_levels(5, 1, false), (1, 1, false));
        assert_eq!(PreparedUpload::web_levels(2, 4, true), (2, 2, false));
        assert_eq!(PreparedUpload::web_levels(0, 0, true), (0, 1, true));
        if usize::BITS > 32 {
            assert_eq!(
                PreparedUpload::web_levels(usize::MAX, 2, true),
                (2, 2, false)
            );
        }
    }
}
