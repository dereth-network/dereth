//! The Classic interface's chrome and fonts, independent of the world's creation resources.
use dereth_classic_dat::fonts::{FontAtlas, FontSource, FontSpec};
use dereth_classic_dat::ClassicPortal;
use std::collections::BTreeMap;
use std::path::PathBuf;
use std::sync::{Arc, Mutex};

/// One decoded interface image.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Image {
    pub width: u32,
    pub height: u32,
    /// Rows top first, alpha opaque (transparency is a colour key chosen when drawing).
    pub rgba: Vec<u8>,
}

/// The fonts the classic interface asks for, by the name its screens use:
/// `(name, height, average width, weight, face)`.
pub const FONTS: [(&str, i32, i32, i32, &str); 17] = dereth_classic_dat::fonts::REQUESTS;

/// Where the classic interface finds its inputs and keeps its own files.
#[derive(Debug, Clone)]
pub struct ClassicPaths {
    /// Where the classic interface's own settings and key schemes live.
    pub state: PathBuf,
}

/// The early 2005 portal's own left panel for the character screen.
pub const CHARACTER_PANEL: u32 = 0x0600_1924;

/// The classic portal's art and tables.
pub struct ClassicArt {
    portal: ClassicPortal,
    images: Mutex<BTreeMap<u32, Option<Arc<Image>>>>,
    fonts: BTreeMap<String, Arc<FontAtlas>>,
}

impl std::fmt::Debug for ClassicArt {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("ClassicArt")
            .field("portal", &self.portal)
            .field("fonts", &self.fonts.len())
            .finish_non_exhaustive()
    }
}

impl ClassicArt {
    /// The art of `portal`, its text drawn by `source`.
    ///
    /// # Errors
    /// A font cannot be drawn.
    pub fn new(portal: ClassicPortal, source: &dyn FontSource) -> Result<Self, String> {
        let mut fonts = BTreeMap::new();
        for (name, height, width, weight, face) in FONTS {
            let atlas = source.rasterize(&FontSpec {
                height,
                width,
                weight,
                italic: false,
                face: face.into(),
            })?;
            fonts.insert(name.to_owned(), Arc::new(atlas));
        }
        Ok(Self {
            portal,
            images: Mutex::default(),
            fonts,
        })
    }

    /// The classic portal itself.
    #[must_use]
    pub const fn portal(&self) -> &ClassicPortal {
        &self.portal
    }

    /// A palette (a `0x04` record) of the classic portal, as 256 opaque colours.
    #[must_use]
    pub fn palette(&self, id: u32) -> Option<Vec<[u8; 4]>> {
        dereth_classic_dat::appearance::palette(&self.portal.get(id)?).ok()
    }

    /// An indexed texture (a `0x05` record) of the classic portal: the creation strips' own when
    /// it is one of them, else read from the portal; `-mirror` after the id names the mirrored
    /// form.
    #[must_use]
    pub fn indexed_texture(
        &self,
        key: &str,
    ) -> Option<dereth_classic_dat::appearance::IndexedTexture> {
        let (id, mirror) = match key.strip_suffix("-mirror") {
            Some(id) => (id, true),
            None => (key, false),
        };
        let id = u32::from_str_radix(id, 16).ok()?;
        let t = dereth_classic_dat::appearance::indexed(&self.portal.get(id)?).ok()?;
        Some(if mirror {
            dereth_classic_dat::appearance::mirrored(&t)
        } else {
            t
        })
    }

    /// The interface image `id` (a `0x06…` record), decoded once and kept: the portal's record,
    /// with the client's own records over it, else one composed from the portal's own pieces
    /// ([`crate::composed`]) when it is one of those.
    #[must_use]
    pub fn image(&self, id: u32) -> Option<Arc<Image>> {
        if id >> 24 != 6 {
            return None;
        }
        if let Some(made) = self.images.lock().ok()?.get(&id) {
            return made.clone();
        }
        // Composing reads other images, so no lock is held while the image is made.
        let made = match self.portal.get(id) {
            Some(payload) => dereth_classic_dat::image::decode_rgb(&payload, Some(id))
                .ok()
                .map(|decoded| Image {
                    width: decoded.width,
                    height: decoded.height,
                    rgba: decoded.rgba,
                }),
            None if crate::composed::is_composed(id) => {
                crate::composed::compose(id, &|piece| self.image(piece).map(|i| (*i).clone()))
            }
            None => None,
        }
        .map(Arc::new);
        self.images.lock().ok()?.insert(id, made.clone());
        made
    }

    /// Every font, by name.
    #[must_use]
    pub const fn fonts(&self) -> &BTreeMap<String, Arc<FontAtlas>> {
        &self.fonts
    }
}
