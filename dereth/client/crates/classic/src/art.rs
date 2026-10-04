//! The Classic interface's chrome and fonts, independent of the world's creation resources.
use dereth_classic_dat::fonts::{FontAtlas, FontSource, FontSpec};
use dereth_classic_dat::ClassicPortal;
use std::collections::BTreeMap;
use std::path::PathBuf;
use std::sync::{Arc, Mutex, OnceLock};

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
pub const FONTS: [(&str, i32, i32, i32, &str); 17] = [
    ("10-4", 10, 4, 500, "Times New Roman"),
    ("14-5", 14, 5, 500, "Times New Roman"),
    ("14-6", 14, 6, 500, "Times New Roman"),
    ("15-5", 15, 5, 500, "Times New Roman"),
    ("15-6", 15, 6, 500, "Times New Roman"),
    ("16-6", 16, 6, 500, "Times New Roman"),
    ("16-7", 16, 7, 500, "Times New Roman"),
    ("20-8", 20, 8, 500, "Times New Roman"),
    ("25-10", 25, 10, 500, "Times New Roman"),
    ("35-16", 35, 16, 500, "Times New Roman"),
    ("courier-14-7", 14, 7, 700, "Courier New"),
    ("arial-14-6", 14, 6, 700, "Arial"),
    ("times-18-7-bold", 18, 7, 700, "Times New Roman"),
    ("times-35-16-heavy", 35, 16, 900, "Times New Roman"),
    ("times-35-13-bold", 35, 13, 700, "Times New Roman"),
    ("times-25-11", 25, 11, 500, "Times New Roman"),
    ("italic-15-6", 15, 6, 500, "Times New Roman Italic"),
];

/// Where the classic interface finds its inputs and keeps its own files.
#[derive(Debug, Clone)]
pub struct ClassicPaths {
    /// The folder holding the early-2005 `portal.dat`, where an installation's own `Default.map`
    /// key scheme sits beside it; `None` when that is not known.
    pub portal_dir: Option<PathBuf>,
    /// Where the classic interface's own settings and key schemes live.
    pub state: PathBuf,
}

/// The character screen's left panel and its Enter Game button (normal, pressed, disabled) in
/// the game's first years. Later portals carry seasonal art under other ids (the early 2005 portal
/// shows spring flowers) or repaint these ids.
pub const FIRST_YEARS_CHARACTER_ART: [u32; 4] =
    [0x0600_1239, 0x0600_122e, 0x0600_122f, 0x0600_1230];
/// The early 2005 portal's own left panel for the character screen.
pub const CHARACTER_PANEL: u32 = 0x0600_1924;

/// The classic portal's art and tables.
pub struct ClassicArt {
    portal: ClassicPortal,
    /// An older portal whose character-screen art is used instead of the classic portal's.
    character_art: Option<ClassicPortal>,
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
            character_art: None,
            images: Mutex::default(),
            fonts,
        })
    }

    /// Take the character screen's art (its left panel and Enter Game button) from `older`, a
    /// portal from the game's first years, instead of the classic portal's seasonal art.
    #[must_use]
    pub fn with_character_art(mut self, older: ClassicPortal) -> Self {
        self.character_art = Some(older);
        self
    }

    /// The character screen's left panel: the first years' panel when an older portal gives it,
    /// else the classic portal's own.
    #[must_use]
    pub fn character_panel(&self) -> u32 {
        if self.character_art.is_some() {
            FIRST_YEARS_CHARACTER_ART[0]
        } else {
            CHARACTER_PANEL
        }
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

    /// The interface image `id` (a `0x06…` record), decoded once and kept.
    #[must_use]
    pub fn image(&self, id: u32) -> Option<Arc<Image>> {
        if id >> 24 != 6 {
            return None;
        }
        if crate::composed::is_composed(id) {
            return self.composed(id);
        }
        let mut images = self.images.lock().ok()?;
        images
            .entry(id)
            .or_insert_with(|| {
                let older = self
                    .character_art
                    .as_ref()
                    .filter(|_| FIRST_YEARS_CHARACTER_ART.contains(&id))
                    .and_then(|p| p.get(id));
                let payload = older.or_else(|| self.portal.get(id))?;
                let decoded = dereth_classic_dat::image::decode_rgb(&payload, Some(id)).ok()?;
                Some(Arc::new(Image {
                    width: decoded.width,
                    height: decoded.height,
                    rgba: decoded.rgba,
                }))
            })
            .clone()
    }

    /// An image composed from the portal's own pieces ([`crate::composed`]), made once and kept.
    fn composed(&self, id: u32) -> Option<Arc<Image>> {
        if let Some(made) = self.images.lock().ok()?.get(&id) {
            return made.clone();
        }
        let made = crate::composed::compose(id, &|piece| self.image(piece).map(|i| (*i).clone()))
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

static INSTALLED: OnceLock<Arc<ClassicArt>> = OnceLock::new();

/// Make `art` the process's classic art. The first call wins; later calls return the art already
/// installed.
pub fn install(art: Arc<ClassicArt>) -> Arc<ClassicArt> {
    Arc::clone(INSTALLED.get_or_init(|| art))
}

/// The process's classic art, once [`install`] has run.
#[must_use]
pub fn installed() -> Option<Arc<ClassicArt>> {
    INSTALLED.get().cloned()
}
