//! The interface art: its own pieces and fonts, which the client carries ([`crate::pieces`]),
//! and the game's own icons, read from the game's data.
//!
//! Every picture the interface draws has a [`TexId`] handed out here, and the drawing asks for
//! its pixels by that id the first time it draws it.

use std::collections::HashMap;
use std::sync::{Arc, Mutex};

use crate::draw::Rect;
use crate::font::Font;

/// A picture, four bytes a pixel in blue, green, red, alpha order.
#[derive(Clone, PartialEq, Eq)]
pub struct Image {
    pub width: u32,
    pub height: u32,
    pub bgra: Vec<u8>,
}

impl std::fmt::Debug for Image {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Image")
            .field("width", &self.width)
            .field("height", &self.height)
            .finish_non_exhaustive()
    }
}

impl Image {
    /// A picture of one colour.
    #[must_use]
    pub fn solid(width: u32, height: u32, bgra: [u8; 4]) -> Self {
        Self {
            width,
            height,
            bgra: bgra.repeat((width * height) as usize),
        }
    }

    /// The pixel at `(x, y)` as `[b, g, r, a]`, or transparent black outside the picture.
    #[must_use]
    pub fn pixel(&self, x: u32, y: u32) -> [u8; 4] {
        if x >= self.width || y >= self.height {
            return [0; 4];
        }
        let i = ((y * self.width + x) * 4) as usize;
        [
            self.bgra[i],
            self.bgra[i + 1],
            self.bgra[i + 2],
            self.bgra[i + 3],
        ]
    }
}

/// A picture the interface can draw.
pub type TexId = u32;

/// A picture as the interface uses it: its id, how many of its pixels make one layout unit, and
/// its size.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct TexRef {
    pub id: TexId,
    pub scale: f32,
    pub width: u32,
    pub height: u32,
}

/// A sprite: part of a texture, with its size in layout units.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Sprite {
    pub tex: TexId,
    /// Where in the texture, in its own pixels.
    pub src: Rect,
    /// Its size in layout units.
    pub w: f32,
    pub h: f32,
    /// Texture pixels per layout unit.
    pub scale: f32,
}

impl Sprite {
    /// The sub-rectangle `(x, y, w, h)` of this sprite, in layout units.
    #[must_use]
    pub fn sub(&self, x: f32, y: f32, w: f32, h: f32) -> Self {
        Self {
            tex: self.tex,
            src: Rect::new(
                self.src.x + x * self.scale,
                self.src.y + y * self.scale,
                w * self.scale,
                h * self.scale,
            ),
            w,
            h,
            scale: self.scale,
        }
    }
}

/// The font families the interface uses.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Family {
    /// Body text.
    Body,
    /// Wide numerals and small caps.
    Numerals,
    /// Bar numbers.
    BarNumbers,
    /// Titles and headings.
    Heading,
    /// Banners and big numbers.
    Banner,
}

impl Family {
    /// Every family.
    pub const ALL: [Self; 5] = [
        Self::Body,
        Self::Numerals,
        Self::BarNumbers,
        Self::Heading,
        Self::Banner,
    ];

    /// Its name in the font table.
    #[must_use]
    pub fn file_stem(self) -> &'static str {
        match self {
            Self::Body => "Body",
            Self::Numerals => "Numerals",
            Self::BarNumbers => "BarNumbers",
            Self::Heading => "Heading",
            Self::Banner => "Banner",
        }
    }
}

/// A loaded font: its table and the picture of each glyph page it draws from.
#[derive(Debug)]
pub struct Face {
    pub font: Font,
    /// Each page's picture.
    pages: Mutex<HashMap<u16, Option<TexId>>>,
}

#[derive(Debug, Default)]
struct Inner {
    by_path: HashMap<String, Option<TexRef>>,
    images: Vec<Option<Arc<Image>>>,
    fonts: HashMap<(Family, u32), Option<Arc<Face>>>,
    /// The game's composed icons, by what they are composed of.
    composites: HashMap<dereth_ui::region::IconRecipe, Option<TexRef>>,
}

/// The interface art.
pub struct Art {
    /// Whether the interface's own pieces are behind the art.
    own: bool,
    /// The Asheron's Call data, for the game's own icons (spells, items, effects).
    ac: Mutex<Option<Arc<dereth_dat::RetailDatStore>>>,
    inner: Mutex<Inner>,
    /// The interface's own pieces' manifest, read the first time a piece is asked for.
    pieces: std::sync::OnceLock<Option<Arc<crate::pieces::Manifest>>>,
    /// The interface's own fonts' table, read the first time text is drawn.
    own_fonts: std::sync::OnceLock<Option<Arc<crate::pieces::FontTable>>>,
}

impl std::fmt::Debug for Art {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Art")
            .field("own", &self.own)
            .finish_non_exhaustive()
    }
}

impl Default for Art {
    fn default() -> Self {
        Self::new()
    }
}

impl Art {
    /// The art of the interface's own pieces, which the client carries.
    #[must_use]
    pub fn new() -> Self {
        Self {
            own: true,
            ac: Mutex::new(None),
            inner: Mutex::new(Inner::default()),
            pieces: std::sync::OnceLock::new(),
            own_fonts: std::sync::OnceLock::new(),
        }
    }

    /// The same art, with nothing decoded yet: what reading it again starts from.
    #[must_use]
    pub fn reopened(&self) -> Self {
        let art = Self {
            own: self.own,
            ac: Mutex::new(None),
            inner: Mutex::new(Inner::default()),
            pieces: std::sync::OnceLock::new(),
            own_fonts: std::sync::OnceLock::new(),
        };
        if let Some(store) = self.ac.lock().ok().and_then(|ac| ac.clone()) {
            art.set_ac_store(store);
        }
        art
    }

    /// Art with no pieces behind it: every lookup of them misses. For tests.
    #[must_use]
    pub fn empty() -> Self {
        Self {
            own: false,
            ac: Mutex::new(None),
            inner: Mutex::new(Inner::default()),
            pieces: std::sync::OnceLock::new(),
            own_fonts: std::sync::OnceLock::new(),
        }
    }

    fn read(&self, path: &str) -> Option<&'static [u8]> {
        self.own.then(|| crate::pieces::builtin(path)).flatten()
    }

    /// Hand out an id for a picture made here rather than read (a font plane, a generated mask).
    pub fn add_image(&self, image: Image) -> TexId {
        let mut inner = self.inner.lock().expect("art lock");
        inner.images.push(Some(Arc::new(image)));
        u32::try_from(inner.images.len() - 1).unwrap_or(u32::MAX)
    }

    /// The pixels of picture `id`.
    #[must_use]
    pub fn image(&self, id: TexId) -> Option<Arc<Image>> {
        self.inner
            .lock()
            .ok()?
            .images
            .get(id as usize)
            .cloned()
            .flatten()
    }

    /// Hand the art the Asheron's Call data, for the game's own icons.
    pub fn set_ac_store(&self, store: Arc<dereth_dat::RetailDatStore>) {
        if let Ok(mut ac) = self.ac.lock() {
            *ac = Some(store);
        }
    }

    /// The Asheron's Call picture `did` (a spell, item or effect icon), decoded.
    #[must_use]
    pub fn ac_icon(&self, did: u32) -> Option<Sprite> {
        if did == 0 {
            return None;
        }
        let key = format!("ac:{did:08x}");
        let hit = self.inner.lock().ok()?.by_path.get(&key).copied();
        let tex = match hit {
            Some(t) => t,
            None => {
                let store = self.ac.lock().ok()?.clone()?;
                let image = dereth_scene::textures::TextureStore::new(&store)
                    .bgra8(dereth_primitives::DataId(did))
                    .ok()
                    .map(|d| Image {
                        width: d.width,
                        height: d.height,
                        bgra: d.pixels.concat(),
                    });
                let tex = image.map(|image| {
                    let (width, height) = (image.width, image.height);
                    TexRef {
                        id: self.add_image(image),
                        scale: 1.0,
                        width,
                        height,
                    }
                });
                self.inner.lock().ok()?.by_path.insert(key, tex);
                tex
            }
        }?;
        #[allow(clippy::cast_precision_loss)]
        let (w, h) = (tex.width as f32, tex.height as f32);
        Some(Sprite {
            tex: tex.id,
            src: Rect::new(0.0, 0.0, w, h),
            w,
            h,
            scale: 1.0,
        })
    }

    /// An item's picture as the game's item lists compose it: the item type's tile, the item's
    /// underlay, then its icon and overlay with the white of the icon's outline replaced by the
    /// item's effects picture. The composition and the choice of pictures are the game's shared
    /// ones ([`dereth_ui::region::composite`], [`dereth_ui_screens::items::widget::object_recipe_by`]).
    #[must_use]
    pub fn ac_item(&self, item: &crate::ui::game::Item) -> Option<Sprite> {
        let (store, recipe) = self.item_recipe(item)?;
        self.composite(&store, recipe)
    }

    /// What follows the pointer while `item` is dragged: the game's drag picture, its icon with
    /// the overlay and the effects picture in its outline, on no tile.
    #[must_use]
    pub fn ac_item_drag(&self, item: &crate::ui::game::Item) -> Option<Sprite> {
        let (store, recipe) = self.item_recipe(item)?;
        self.composite(&store, recipe.drag_surface())
    }

    /// A spell's icon as the game's spellbook composes it: the power level's background under
    /// the icon, the wash its bitfield picks, and its badge.
    #[must_use]
    pub fn ac_spell(&self, icon: u32, power: u32, bitfield: u32) -> Option<Sprite> {
        if icon == 0 {
            return None;
        }
        let store = self.ac.lock().ok()?.clone()?;
        let lookup =
            |group: u32, value: u32| dereth_client_runtime::assets::enum_did(&*store, group, value);
        let recipe = dereth_ui_screens::items::widget::spell_recipe_by(
            &lookup,
            power,
            Some(dereth_primitives::DataId(icon)),
            bitfield,
        );
        self.composite(&store, recipe)
    }

    /// A spell component's icon as the game's magic window draws it: its opaque white outline
    /// turned opaque black.
    #[must_use]
    pub fn ac_component(&self, icon: u32) -> Option<Sprite> {
        use dereth_ui::region::SurfaceOp;
        self.ac_icon_replacing(icon, SurfaceOp::OPAQUE_WHITE, SurfaceOp::OPAQUE_BLACK)
    }

    /// A skill's icon with its opaque black ground cleared, so the window shows through it.
    #[must_use]
    pub fn ac_skill(&self, icon: u32) -> Option<Sprite> {
        use dereth_ui::region::SurfaceOp;
        self.ac_icon_replacing(icon, SurfaceOp::OPAQUE_BLACK, 0)
    }

    /// The game's icon `icon` as its icon lists draw it, alone, on no tile.
    #[must_use]
    pub fn ac_plain_icon(&self, icon: u32) -> Option<Sprite> {
        // A colour no icon holds, so nothing is replaced.
        self.ac_icon_replacing(icon, 0x0001_0203, 0x0001_0203)
    }

    /// Icon `icon` with every texel of exactly `from` made `to`, made once.
    fn ac_icon_replacing(&self, icon: u32, from: u32, to: u32) -> Option<Sprite> {
        let op = dereth_ui::region::SurfaceOp::ReplaceColor { from, to };
        let key = format!("replaced:{icon:08x}:{from:08x}:{to:08x}");
        let hit = self.inner.lock().ok()?.by_path.get(&key).copied();
        let tex = match hit {
            Some(t) => t,
            None => {
                let store = self.ac.lock().ok()?.clone()?;
                let textures = dereth_scene::textures::TextureStore::new(&store);
                let tex = textures
                    .icon_data(dereth_primitives::DataId(icon))
                    .ok()
                    .filter(|d| d.format == dereth_primitives::TextureFormat::Bgra8)
                    .and_then(|d| {
                        let mut bgra = d.levels.into_iter().next()?;
                        for px in bgra.as_chunks_mut::<4>().0 {
                            *px = op.apply(u32::from_le_bytes(*px)).to_le_bytes();
                        }
                        let (width, height) = (d.width, d.height);
                        Some(TexRef {
                            id: self.add_image(Image {
                                width,
                                height,
                                bgra,
                            }),
                            scale: 1.0,
                            width,
                            height,
                        })
                    });
                self.inner.lock().ok()?.by_path.insert(key, tex);
                tex
            }
        }?;
        #[allow(clippy::cast_precision_loss)]
        let (w, h) = (tex.width as f32, tex.height as f32);
        Some(Sprite {
            tex: tex.id,
            src: Rect::new(0.0, 0.0, w, h),
            w,
            h,
            scale: 1.0,
        })
    }

    /// An item's icon recipe, with the store its layers are read from.
    fn item_recipe(
        &self,
        item: &crate::ui::game::Item,
    ) -> Option<(
        Arc<dereth_dat::RetailDatStore>,
        dereth_ui::region::IconRecipe,
    )> {
        let store = self.ac.lock().ok()?.clone()?;
        let decoration = item
            .decoration
            .unwrap_or(dereth_client_contract::view::SlotDecoration {
                icon_id: item.ac_icon.unwrap_or(0),
                icon_underlay_id: item.underlay.map(dereth_primitives::DataId),
                icon_overlay_id: item.overlay.map(dereth_primitives::DataId),
                ..dereth_client_contract::view::SlotDecoration::default()
            });
        let lookup =
            |group: u32, value: u32| dereth_client_runtime::assets::enum_did(&*store, group, value);
        let recipe = dereth_ui_screens::items::widget::object_recipe_by(&lookup, &decoration);
        Some((store, recipe))
    }

    /// A composed icon, made once per recipe.
    fn composite(
        &self,
        store: &dereth_dat::RetailDatStore,
        recipe: dereth_ui::region::IconRecipe,
    ) -> Option<Sprite> {
        let hit = self.inner.lock().ok()?.composites.get(&recipe).copied();
        let tex = match hit {
            Some(t) => t,
            None => {
                let textures = dereth_scene::textures::TextureStore::new(store);
                let data = dereth_ui::region::composite(recipe, &|id| textures.icon_data(id).ok());
                let tex = data.and_then(|d| {
                    let image = Image {
                        width: d.width,
                        height: d.height,
                        bgra: d.levels.into_iter().next()?,
                    };
                    let (width, height) = (image.width, image.height);
                    Some(TexRef {
                        id: self.add_image(image),
                        scale: 1.0,
                        width,
                        height,
                    })
                });
                self.inner.lock().ok()?.composites.insert(recipe, tex);
                tex
            }
        }?;
        #[allow(clippy::cast_precision_loss)]
        let (w, h) = (tex.width as f32, tex.height as f32);
        Some(Sprite {
            tex: tex.id,
            src: Rect::new(0.0, 0.0, w, h),
            w,
            h,
            scale: 1.0,
        })
    }

    /// The interface's own pieces' manifest, when the pieces are behind the art.
    fn manifest(&self) -> Option<Arc<crate::pieces::Manifest>> {
        self.pieces
            .get_or_init(|| {
                let bytes = self.read(crate::pieces::MANIFEST)?;
                match crate::pieces::Manifest::parse(bytes) {
                    Ok(m) => Some(Arc::new(m)),
                    Err(e) => {
                        tracing::warn!("{e}");
                        None
                    }
                }
            })
            .clone()
    }

    /// Whether the interface's own piece `name` is there to draw.
    #[must_use]
    pub fn has_piece(&self, name: &str) -> bool {
        self.manifest().is_some_and(|m| m.pieces.contains_key(name))
    }

    /// The interface's own piece `name`, from the atlas sharp enough for `px_per_unit` screen
    /// pixels a layout unit. Its size is in layout units.
    #[must_use]
    pub fn piece(&self, name: &str, px_per_unit: f32) -> Option<Sprite> {
        let manifest = self.manifest()?;
        let (scale, atlas, rect, ratio) = manifest.place(name, px_per_unit)?;
        let size = manifest.pieces.get(name)?.size;
        let tex = self.atlas(scale, atlas)?;
        #[allow(clippy::cast_precision_loss)]
        let src = Rect::new(
            rect[0] as f32,
            rect[1] as f32,
            rect[2] as f32,
            rect[3] as f32,
        );
        Some(Sprite {
            tex,
            src,
            w: size[0],
            h: size[1],
            scale: ratio,
        })
    }

    /// The pixels of piece `name` at its largest size, cut out of its atlas, for a picture the
    /// interface hands the world rather than draws itself. `None` when the art has no such piece.
    #[must_use]
    pub fn piece_picture(&self, name: &str) -> Option<dereth_primitives::TextureData> {
        let sprite = self.piece(name, 4.0)?;
        let image = self.image(sprite.tex)?;
        #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
        let (x0, y0, w, h) = (
            sprite.src.x as u32,
            sprite.src.y as u32,
            sprite.src.w as u32,
            sprite.src.h as u32,
        );
        if w == 0 || h == 0 || x0 + w > image.width || y0 + h > image.height {
            return None;
        }
        let mut bgra = Vec::with_capacity((w * h * 4) as usize);
        for y in y0..y0 + h {
            let row = ((y * image.width + x0) * 4) as usize;
            bgra.extend_from_slice(&image.bgra[row..row + (w * 4) as usize]);
        }
        Some(dereth_primitives::TextureData {
            width: w,
            height: h,
            format: dereth_primitives::TextureFormat::Bgra8,
            levels: vec![bgra],
        })
    }

    /// The value `name` the interface lays its own pieces out by (in layout units).
    #[must_use]
    pub fn piece_value(&self, name: &str) -> Option<Vec<f32>> {
        self.manifest()?.values.get(name).cloned()
    }

    /// Atlas `index` at `scale`, decoded once.
    fn atlas(&self, scale: u32, index: u32) -> Option<TexId> {
        let key = format!("pieces:{scale}:{index}");
        if let Some(hit) = self.inner.lock().ok()?.by_path.get(&key) {
            return hit.map(|t| t.id);
        }
        let path = crate::pieces::atlas_path(scale, index);
        let image = self.read(&path).and_then(|b| {
            crate::pieces::decode_png(b)
                .map_err(|e| tracing::warn!("{path}: {e}"))
                .ok()
        });
        let tex = image.map(|image| {
            let (width, height) = (image.width, image.height);
            TexRef {
                id: self.add_image(image),
                scale: 1.0,
                width,
                height,
            }
        });
        self.inner.lock().ok()?.by_path.insert(key, tex);
        tex.map(|t| t.id)
    }

    /// The interface's own fonts' table, when the pieces are behind the art.
    fn own_fonts(&self) -> Option<Arc<crate::pieces::FontTable>> {
        self.own_fonts
            .get_or_init(|| {
                let bytes = self.read(crate::pieces::FONTS)?;
                match crate::pieces::FontTable::parse(bytes) {
                    Ok(t) => Some(Arc::new(t)),
                    Err(e) => {
                        tracing::warn!("{e}");
                        None
                    }
                }
            })
            .clone()
    }

    /// The font of `family` nearest to `points`.
    #[must_use]
    pub fn font(&self, family: Family, points: f32) -> Option<Arc<Face>> {
        let table = self.own_fonts()?;
        let (size, factor) = table.nearest(family.file_stem(), points)?;
        #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
        let key = (family, size.size as u32);
        if let Some(hit) = self.inner.lock().ok()?.fonts.get(&key) {
            return hit.clone();
        }
        let face = Some(Arc::new(Face {
            font: size.font(factor),
            pages: Mutex::new(HashMap::new()),
        }));
        self.inner.lock().ok()?.fonts.insert(key, face.clone());
        face
    }

    /// The picture of a glyph page: white, with the page's grey as its alpha. The page is shared
    /// by every face, so it is made once.
    #[must_use]
    pub fn font_page(&self, face: &Face, page: u16) -> Option<TexId> {
        if let Some(hit) = face.pages.lock().ok()?.get(&page) {
            return *hit;
        }
        let key = format!("pieces-font:{page}");
        let cached = self.inner.lock().ok()?.by_path.get(&key).copied();
        let id = match cached {
            Some(t) => t.map(|t| t.id),
            None => {
                let image = self
                    .read(&crate::pieces::font_page_path(page))
                    .and_then(|b| crate::pieces::decode_png(b).ok())
                    .map(|im| {
                        let bgra = im
                            .bgra
                            .as_chunks::<4>()
                            .0
                            .iter()
                            .flat_map(|p| [255, 255, 255, p[0]])
                            .collect();
                        Image {
                            width: im.width,
                            height: im.height,
                            bgra,
                        }
                    });
                let tex = image.map(|image| {
                    let (width, height) = (image.width, image.height);
                    TexRef {
                        id: self.add_image(image),
                        scale: 1.0,
                        width,
                        height,
                    }
                });
                self.inner.lock().ok()?.by_path.insert(key, tex);
                tex.map(|t| t.id)
            }
        };
        face.pages.lock().ok()?.insert(page, id);
        id
    }
}

#[cfg(test)]
mod tests {
    //! Behaviour: none (experimental Horizon interface)
    use super::*;

    /// An item's picture is the game's composite: where its icon's outline is white, the
    /// composite shows the item's effects picture and never the white.
    #[test]
    #[cfg_attr(
        not(feature = "retail-dats"),
        ignore = "reads the retail dats: --features retail-dats"
    )]
    fn an_item_s_icon_outline_takes_its_effects_picture_and_never_shows_white() {
        let store = Arc::new(dereth_dat::testing::open_store().unwrap_or_else(|| {
            panic!(
                "the retail dats are this test's oracle and they are not under {}",
                dereth_dat::testing::dat_dir().display()
            )
        }));
        let textures = dereth_scene::textures::TextureStore::new(&store);
        let white = |d: &dereth_primitives::TextureData| {
            d.levels[0]
                .as_chunks::<4>()
                .0
                .iter()
                .filter(|p| **p == [0xFF; 4])
                .count()
        };
        // The first icon in the files whose outline is white.
        let icon = (0x0600_1000..0x0600_4000)
            .map(dereth_primitives::DataId)
            .find(|id| {
                textures.icon_data(*id).ok().is_some_and(|d| {
                    d.width == 32
                        && d.height == 32
                        && d.format == dereth_primitives::TextureFormat::Bgra8
                        && white(&d) > 0
                })
            })
            .expect("an icon with a white outline");
        let art = Art::empty();
        art.set_ac_store(Arc::clone(&store));
        let item = crate::ui::game::Item {
            ac_icon: Some(icon.0),
            decoration: Some(dereth_client_contract::view::SlotDecoration {
                icon_id: icon.0,
                obj_type: 1,
                ..dereth_client_contract::view::SlotDecoration::default()
            }),
            ..crate::ui::game::Item::default()
        };
        let sprite = art.ac_item(&item).expect("the composite");
        let image = art.image(sprite.tex).expect("its pixels");
        assert_eq!((image.width, image.height), (32, 32));
        assert_eq!(
            image
                .bgra
                .as_chunks::<4>()
                .0
                .iter()
                .filter(|p| **p == [0xFF; 4])
                .count(),
            0,
            "icon {icon:?}: the outline's white was replaced"
        );
        assert!(
            image.bgra.as_chunks::<4>().0.iter().all(|p| p[3] == 0xFF),
            "the item type's tile is under the whole icon"
        );
        // The same item again is the same picture, made once.
        assert_eq!(art.ac_item(&item).map(|s| s.tex), Some(sprite.tex));
    }

    /// A component's icon is drawn with its white outline black, and a dragged item's picture is
    /// its drag picture: no white, and none of the type tile the slot shows under it.
    #[test]
    #[cfg_attr(
        not(feature = "retail-dats"),
        ignore = "reads the retail dats: --features retail-dats"
    )]
    fn a_component_s_outline_is_black_and_a_dragged_item_shows_no_white_and_no_tile() {
        let store = Arc::new(dereth_dat::testing::open_store().unwrap_or_else(|| {
            panic!(
                "the retail dats are this test's oracle and they are not under {}",
                dereth_dat::testing::dat_dir().display()
            )
        }));
        let textures = dereth_scene::textures::TextureStore::new(&store);
        let white = |bgra: &[u8]| {
            bgra.as_chunks::<4>()
                .0
                .iter()
                .filter(|p| **p == [0xFF; 4])
                .count()
        };
        let icon = (0x0600_1000..0x0600_4000)
            .map(dereth_primitives::DataId)
            .find(|id| {
                textures.icon_data(*id).ok().is_some_and(|d| {
                    d.width == 32
                        && d.height == 32
                        && d.format == dereth_primitives::TextureFormat::Bgra8
                        && white(&d.levels[0]) > 0
                })
            })
            .expect("an icon with a white outline");
        let art = Art::empty();
        art.set_ac_store(Arc::clone(&store));
        let component = art.ac_component(icon.0).expect("the component's icon");
        assert_eq!(white(&art.image(component.tex).unwrap().bgra), 0);
        let item = crate::ui::game::Item {
            ac_icon: Some(icon.0),
            decoration: Some(dereth_client_contract::view::SlotDecoration {
                icon_id: icon.0,
                obj_type: 1,
                ..dereth_client_contract::view::SlotDecoration::default()
            }),
            ..crate::ui::game::Item::default()
        };
        let slot = art.image(art.ac_item(&item).unwrap().tex).unwrap();
        let drag = art.image(art.ac_item_drag(&item).unwrap().tex).unwrap();
        assert_eq!(white(&drag.bgra), 0);
        assert_ne!(drag.bgra, slot.bgra, "the drag picture is not the tile's");
    }

    /// A skill's icon is drawn on the window, not on its black ground: none of it stays opaque
    /// black, and what was black is clear.
    #[test]
    #[cfg_attr(
        not(feature = "retail-dats"),
        ignore = "reads the retail dats: --features retail-dats"
    )]
    fn a_skill_s_icon_has_no_black_ground() {
        use dereth_assets::Decode as _;
        use dereth_primitives::asset::AssetSource as _;
        let store = Arc::new(dereth_dat::testing::open_store().unwrap_or_else(|| {
            panic!(
                "the retail dats are this test's oracle and they are not under {}",
                dereth_dat::testing::dat_dir().display()
            )
        }));
        let id = dereth_primitives::DataId(0x0E00_0004);
        let table = dereth_assets::tables::SkillTable::decode_payload_in(
            store.era_of(id),
            id,
            &store.read(id).expect("the skill table"),
        )
        .expect("the skill table decodes");
        let textures = dereth_scene::textures::TextureStore::new(&store);
        let black = |bgra: &[u8]| {
            bgra.as_chunks::<4>()
                .0
                .iter()
                .filter(|p| **p == [0, 0, 0, 0xFF])
                .count()
        };
        let art = Art::empty();
        art.set_ac_store(Arc::clone(&store));
        let mut checked = 0;
        for skill in table.skills.values().filter(|s| s.icon != 0) {
            let raw = textures
                .icon_data(dereth_primitives::DataId(skill.icon))
                .expect("the skill's icon");
            let was = black(&raw.levels[0]);
            let shown = art.image(art.ac_skill(skill.icon).unwrap().tex).unwrap();
            assert_eq!(black(&shown.bgra), 0, "{}", skill.name);
            let clear = shown
                .bgra
                .as_chunks::<4>()
                .0
                .iter()
                .filter(|p| p[3] == 0)
                .count();
            assert_eq!(clear, was, "{}: what was black is clear", skill.name);
            checked += usize::from(was > 0);
        }
        assert!(
            checked > 10,
            "the skills' icons have black grounds: {checked}"
        );
    }
}
