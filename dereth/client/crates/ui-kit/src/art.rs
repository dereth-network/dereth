//! A skin's art: its own pieces and fonts, as the host hands them over ([`crate::pieces`]), the
//! pictures made at run time, and the game's own icons through the source an interface puts
//! behind it ([`ArtSource`]).
//!
//! Every picture the kit draws has a [`TexId`] handed out here, and the drawing asks for its
//! pixels by that id the first time it draws it.

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

/// A picture the kit can draw.
pub type TexId = u32;

/// A picture as the kit uses it: its id, how many of its pixels make one layout unit, and its
/// size.
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
    /// The sub-rectangle `(x, y, w, h)` of this sprite, in layout units: each axis by the
    /// sprite's own texels per unit along it, so a piece whose texels are not as many per unit
    /// down as across is cut from its own rows.
    #[must_use]
    pub fn sub(&self, x: f32, y: f32, w: f32, h: f32) -> Self {
        let across = if self.w > 0.0 {
            self.src.w / self.w
        } else {
            self.scale
        };
        let down = if self.h > 0.0 {
            self.src.h / self.h
        } else {
            self.scale
        };
        Self {
            tex: self.tex,
            src: Rect::new(
                self.src.x + x * across,
                self.src.y + y * down,
                w * across,
                h * down,
            ),
            w,
            h,
            scale: self.scale,
        }
    }

    /// The whole of picture `tex`, a pixel to a layout unit.
    #[must_use]
    pub fn whole(tex: &TexRef) -> Self {
        #[allow(clippy::cast_precision_loss)]
        let (w, h) = (tex.width as f32, tex.height as f32);
        Self {
            tex: tex.id,
            src: Rect::new(0.0, 0.0, w, h),
            w,
            h,
            scale: 1.0,
        }
    }
}

/// The font families the kit draws text in.
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

impl Face {
    /// A face of `font`, its pages not yet looked up.
    #[must_use]
    pub fn new(font: Font) -> Self {
        Self {
            font,
            pages: Mutex::new(HashMap::new()),
        }
    }
}

/// The files a skin's art is read from, each by its name ([`crate::pieces`] names them).
pub trait ArtFiles: Send + Sync + std::fmt::Debug {
    /// File `name`'s bytes, its case aside.
    fn get(&self, name: &str) -> Option<&[u8]>;
}

/// What an interface puts behind the art for what only it can read: the game's own icons, from
/// the game's data. A source may keep pictures it made through the art (by their ids), so another
/// set of art takes a [`ArtSource::fresh`] copy that keeps none of them.
pub trait ArtSource: Send + Sync + std::fmt::Debug {
    /// The game's icon `did`, decoded, or `None` when the data has no such picture.
    fn icon(&self, did: u32) -> Option<Image>;
    /// The same source, keeping nothing it made for another set of art.
    fn fresh(&self) -> Arc<dyn ArtSource>;
    /// The source as itself, for the interface that put it there.
    fn as_any(&self) -> &dyn std::any::Any;
}

/// A font handed over whole, rather than read from the pieces' font table: one table, drawn for
/// every family at its own size, and its glyph pages' pictures (white, with the glyphs' coverage
/// as alpha).
#[derive(Debug, Clone)]
pub struct GivenFont {
    pub font: Font,
    pub pages: Vec<Arc<Image>>,
}

/// Pieces handed over whole at run time rather than read from the skin's files: each a picture
/// drawn a pixel to a layout unit, by its name, and the values the kit lays them out by (as a
/// manifest's, [`crate::pieces`]). The art packs them into one picture of its own.
#[derive(Debug, Clone, Default)]
pub struct GivenPieces {
    pub pieces: std::collections::BTreeMap<String, Arc<Image>>,
    pub values: std::collections::BTreeMap<String, Vec<f32>>,
}

impl GivenPieces {
    /// Whether piece `name` is here.
    #[must_use]
    pub fn has(&self, name: &str) -> bool {
        self.pieces.contains_key(name)
    }
}

/// Given pieces packed into one picture: each piece's rectangle in it.
type Packed = HashMap<String, Rect>;

/// How far apart packed pieces stand: each is ringed by a copy of its own edge pixels, so a piece
/// drawn larger than itself takes nothing of its neighbours into its edges.
const PACK_GAP: u32 = 1;

/// The widest a picture of packed pieces is made.
const PACK_WIDTH: u32 = 1024;

/// Pack `pieces` into one picture, tallest first along shelves.
fn pack(pieces: &GivenPieces) -> (Image, Packed) {
    let mut order: Vec<(&String, &Arc<Image>)> = pieces
        .pieces
        .iter()
        .filter(|(_, im)| im.width > 0 && im.height > 0)
        .collect();
    order.sort_by(|a, b| b.1.height.cmp(&a.1.height).then(a.0.cmp(b.0)));
    let width = order
        .iter()
        .map(|(_, im)| im.width + 2 * PACK_GAP)
        .max()
        .unwrap_or(1)
        .max(PACK_WIDTH);
    let mut places = Vec::with_capacity(order.len());
    let (mut x, mut y, mut shelf) = (0, 0, 0);
    for (name, im) in &order {
        let (w, h) = (im.width + 2 * PACK_GAP, im.height + 2 * PACK_GAP);
        if x + w > width {
            x = 0;
            y += shelf;
            shelf = 0;
        }
        places.push((*name, *im, x + PACK_GAP, y + PACK_GAP));
        x += w;
        shelf = shelf.max(h);
    }
    let height = (y + shelf).max(1);
    let mut atlas = Image::solid(width, height, [0; 4]);
    let mut at = HashMap::with_capacity(places.len());
    for (name, im, px, py) in places {
        // The piece, and its edge pixels once more all round it.
        for dy in -1..=i64::from(im.height) {
            for dx in -1..=i64::from(im.width) {
                let sx = u32::try_from(dx.clamp(0, i64::from(im.width) - 1)).unwrap_or(0);
                let sy = u32::try_from(dy.clamp(0, i64::from(im.height) - 1)).unwrap_or(0);
                let tx = u32::try_from(i64::from(px) + dx).unwrap_or(0);
                let ty = u32::try_from(i64::from(py) + dy).unwrap_or(0);
                let from = ((sy * im.width + sx) * 4) as usize;
                let to = ((ty * width + tx) * 4) as usize;
                atlas.bgra[to..to + 4].copy_from_slice(&im.bgra[from..from + 4]);
            }
        }
        #[allow(clippy::cast_precision_loss)]
        at.insert(
            name.clone(),
            Rect::new(px as f32, py as f32, im.width as f32, im.height as f32),
        );
    }
    (atlas, at)
}

#[derive(Debug, Default)]
struct Inner {
    by_path: HashMap<String, Option<TexRef>>,
    images: Vec<Option<Arc<Image>>>,
    fonts: HashMap<(Family, u32), Option<Arc<Face>>>,
    /// Pictures let go since the overlay last asked: their uploads go with them.
    forgotten: Vec<TexId>,
}

/// A skin's art.
pub struct Art {
    /// The skin's own pieces, when there are any behind the art.
    own: Option<Arc<dyn ArtFiles>>,
    /// What only the interface can read: the game's icons.
    source: Mutex<Option<Arc<dyn ArtSource>>>,
    inner: Mutex<Inner>,
    /// The skin's own pieces' manifest, read the first time a piece is asked for.
    pieces: std::sync::OnceLock<Option<Arc<crate::pieces::Manifest>>>,
    /// The skin's own fonts' table, read the first time text is drawn.
    own_fonts: std::sync::OnceLock<Option<Arc<crate::pieces::FontTable>>>,
    /// A font handed over whole, drawn for every family where the pieces have no font table.
    given_font: Option<Arc<GivenFont>>,
    /// Pieces handed over whole, where the skin has no files of its own.
    given_pieces: Option<Arc<GivenPieces>>,
    /// The given pieces packed, the first time one is drawn.
    packed: std::sync::OnceLock<Option<(TexId, Arc<Packed>)>>,
    /// How the widgets draw where the skin has no piece.
    palette: crate::palette::Palette,
}

impl std::fmt::Debug for Art {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Art")
            .field("own", &self.own.is_some())
            .finish_non_exhaustive()
    }
}

impl Art {
    /// The art of a skin's own pieces, as the host handed them over.
    #[must_use]
    pub fn new(pieces: Arc<dyn ArtFiles>) -> Self {
        Self {
            own: Some(pieces),
            ..Self::empty()
        }
    }

    /// Art with no pieces, drawn plainly in `palette` with its text in `font`: a skin of a font
    /// handed over and plain drawing.
    #[must_use]
    pub fn plain(font: Option<GivenFont>, palette: crate::palette::Palette) -> Self {
        Self {
            given_font: font.map(Arc::new),
            palette,
            ..Self::empty()
        }
    }

    /// This art with `pieces` handed over whole: drawn where it has no files of its own, each
    /// widget drawn plainly from the palette where its pieces are not among them.
    #[must_use]
    pub fn with_pieces(mut self, pieces: GivenPieces) -> Self {
        self.given_pieces = Some(Arc::new(pieces));
        self.packed = std::sync::OnceLock::new();
        self
    }

    /// The pieces handed over whole, if any.
    #[must_use]
    pub fn given_pieces(&self) -> Option<&GivenPieces> {
        self.given_pieces.as_deref()
    }

    /// How the widgets draw where this skin has no piece.
    #[must_use]
    pub fn palette(&self) -> &crate::palette::Palette {
        &self.palette
    }

    /// The same art, with nothing decoded yet: what reading it again starts from. The source
    /// comes with it, keeping nothing it made for this art.
    #[must_use]
    pub fn reopened(&self) -> Self {
        let art = Self {
            own: self.own.clone(),
            given_font: self.given_font.clone(),
            given_pieces: self.given_pieces.clone(),
            palette: self.palette,
            ..Self::empty()
        };
        if let Some(source) = self.source() {
            art.set_source(source.fresh());
        }
        art
    }

    /// Art with no pieces behind it: every lookup of them misses. For tests.
    #[must_use]
    pub fn empty() -> Self {
        Self {
            own: None,
            source: Mutex::new(None),
            inner: Mutex::new(Inner::default()),
            pieces: std::sync::OnceLock::new(),
            own_fonts: std::sync::OnceLock::new(),
            given_font: None,
            given_pieces: None,
            packed: std::sync::OnceLock::new(),
            palette: crate::palette::Palette::default(),
        }
    }

    /// The given pieces packed into one picture, made the first time it is asked for.
    fn packed(&self) -> Option<(TexId, Arc<Packed>)> {
        self.packed
            .get_or_init(|| {
                let given = self.given_pieces.as_ref()?;
                let (atlas, at) = pack(given);
                let id = self.add_image(atlas);
                Some((id, Arc::new(at)))
            })
            .clone()
    }

    fn read(&self, name: &str) -> Option<&[u8]> {
        self.own.as_deref()?.get(name)
    }

    /// Hand out an id for a picture made here rather than read (a font plane, a generated mask).
    pub fn add_image(&self, image: Image) -> TexId {
        let mut inner = self.inner.lock().expect("art lock");
        inner.images.push(Some(Arc::new(image)));
        u32::try_from(inner.images.len() - 1).unwrap_or(u32::MAX)
    }

    /// Let picture `id` go: it is never drawn again, and the overlay lets its upload go.
    pub fn forget_image(&self, id: TexId) {
        if let Ok(mut inner) = self.inner.lock() {
            let gone = inner
                .images
                .get_mut(id as usize)
                .is_some_and(|slot| slot.take().is_some());
            if gone {
                inner.forgotten.push(id);
            }
        }
    }

    /// The pictures let go since the last call.
    pub fn take_forgotten(&self) -> Vec<TexId> {
        self.inner
            .lock()
            .map(|mut i| std::mem::take(&mut i.forgotten))
            .unwrap_or_default()
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

    /// Put `source` behind the art, for what only the interface can read.
    pub fn set_source(&self, source: Arc<dyn ArtSource>) {
        if let Ok(mut s) = self.source.lock() {
            *s = Some(source);
        }
    }

    /// What is behind the art, if anything.
    #[must_use]
    pub fn source(&self) -> Option<Arc<dyn ArtSource>> {
        self.source.lock().ok()?.clone()
    }

    /// What is kept under `key`: `None` when nothing is kept there yet, `Some(None)` when it is
    /// kept as missing.
    #[must_use]
    pub fn lookup(&self, key: &str) -> Option<Option<Sprite>> {
        let hit = *self.inner.lock().ok()?.by_path.get(key)?;
        Some(hit.as_ref().map(Sprite::whole))
    }

    /// The picture kept under `key`, made by `make` the first time it is asked for (and kept as
    /// missing when `make` has none), as a sprite of the whole picture.
    pub fn cached(&self, key: &str, make: impl FnOnce() -> Option<Image>) -> Option<Sprite> {
        let hit = self.inner.lock().ok()?.by_path.get(key).copied();
        let tex = match hit {
            Some(t) => t,
            None => {
                let tex = make().map(|image| {
                    let (width, height) = (image.width, image.height);
                    TexRef {
                        id: self.add_image(image),
                        scale: 1.0,
                        width,
                        height,
                    }
                });
                self.inner.lock().ok()?.by_path.insert(key.to_owned(), tex);
                tex
            }
        }?;
        Some(Sprite::whole(&tex))
    }

    /// The game's icon `did`, alone, as its icon lists draw it: from the source behind the art,
    /// made once. `None` with no source behind the art, or no such icon.
    #[must_use]
    pub fn icon(&self, did: u32) -> Option<Sprite> {
        if did == 0 {
            return None;
        }
        let source = self.source()?;
        self.cached(&format!("icon:{did:08x}"), || source.icon(did))
    }

    /// The skin's own pieces' manifest, when the pieces are behind the art.
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

    /// Whether the skin's own piece `name` is there to draw.
    #[must_use]
    pub fn has_piece(&self, name: &str) -> bool {
        self.manifest().is_some_and(|m| m.pieces.contains_key(name))
            || self.given_pieces.as_ref().is_some_and(|g| g.has(name))
    }

    /// The skin's own piece `name`, from the atlas sharp enough for `px_per_unit` screen pixels
    /// a layout unit. Its size is in layout units. A piece handed over whole is drawn a pixel to
    /// a layout unit, whatever the scale.
    #[must_use]
    pub fn piece(&self, name: &str, px_per_unit: f32) -> Option<Sprite> {
        let Some(manifest) = self.manifest() else {
            let (tex, at) = self.packed()?;
            let src = *at.get(name)?;
            return Some(Sprite {
                tex,
                src,
                w: src.w,
                h: src.h,
                scale: 1.0,
            });
        };
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

    /// The size of the skin's own piece `name`, in layout units, as its manifest gives it; `None`
    /// for a piece handed over whole, which has no size but its pixels.
    #[must_use]
    pub fn piece_size(&self, name: &str) -> Option<(f32, f32)> {
        let size = self.manifest()?.pieces.get(name)?.size;
        Some((size[0], size[1]))
    }

    /// The value `name` the skin lays its own pieces out by (in layout units).
    #[must_use]
    pub fn piece_value(&self, name: &str) -> Option<Vec<f32>> {
        match self.manifest() {
            Some(m) => m.values.get(name).cloned(),
            None => self.given_pieces.as_ref()?.values.get(name).cloned(),
        }
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

    /// The skin's own fonts' table, when the pieces are behind the art.
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

    /// The font of `family` nearest to `points`. A font handed over whole is drawn at its own
    /// size whatever is asked: its face answers as the size asked for, so it is never scaled.
    #[must_use]
    pub fn font(&self, family: Family, points: f32) -> Option<Arc<Face>> {
        let Some(table) = self.own_fonts() else {
            return self.given_face(family, points);
        };
        let (size, factor) = table.nearest(family.file_stem(), points)?;
        #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
        let key = (family, size.size as u32);
        if let Some(hit) = self.inner.lock().ok()?.fonts.get(&key) {
            return hit.clone();
        }
        let face = Some(Arc::new(Face::new(size.font(factor))));
        self.inner.lock().ok()?.fonts.insert(key, face.clone());
        face
    }

    /// The font handed over, as a face answering as `points`.
    fn given_face(&self, family: Family, points: f32) -> Option<Arc<Face>> {
        let given = self.given_font.as_ref()?;
        #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
        // LINT-OK: a text size in pixels, a few hundred at most, kept to a 64th.
        let key = (family, (points * 64.0).round().max(0.0) as u32);
        if let Some(hit) = self.inner.lock().ok()?.fonts.get(&key) {
            return hit.clone();
        }
        let mut font = given.font.clone();
        font.point_size = points;
        let face = Some(Arc::new(Face::new(font)));
        self.inner.lock().ok()?.fonts.insert(key, face.clone());
        face
    }

    /// The picture of a glyph page: white, with the page's coverage as its alpha. The page is
    /// shared by every face, so it is made once.
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
                let image = match (&self.own, &self.given_font) {
                    (None, Some(given)) => {
                        given.pages.get(usize::from(page)).map(|p| (**p).clone())
                    }
                    _ => self
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
                        }),
                };
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
    //! Behaviour: none (this client's own widget kit)
    use super::*;

    /// A font handed over is drawn unscaled at any size asked for, from its own pages.
    #[test]
    fn a_font_handed_over_answers_every_size_asked_and_draws_from_its_own_pages() {
        let mut font = Font {
            point_size: 9.0,
            line_height: 14,
            ascent: 11,
            ..Font::default()
        };
        font.glyphs.insert(
            'A',
            crate::font::Glyph {
                ch: 'A',
                page: 0,
                x: 0,
                y: 0,
                w: 6,
                h: 9,
                advance_adjust: 1,
                y_offset: 2,
                x_offset: 0,
            },
        );
        let page = Arc::new(Image::solid(16, 16, [255, 255, 255, 200]));
        let art = Art::plain(
            Some(GivenFont {
                font,
                pages: vec![Arc::clone(&page)],
            }),
            crate::palette::Palette::retail_plain(),
        );
        let face = art.font(Family::Heading, 23.0).expect("a face");
        assert!((face.font.point_size - 23.0).abs() < f32::EPSILON);
        assert_eq!(face.font.line_height, 14);
        let tex = art.font_page(&face, 0).expect("its page");
        assert_eq!(art.image(tex).as_deref(), Some(&*page));
        assert_eq!(art.font_page(&face, 1), None, "a page it has not");
        assert!(Art::empty().font(Family::Body, 13.0).is_none());
    }

    /// Pieces handed over whole are drawn from one picture they are packed into, each found by
    /// its name at its own size, its pixels where its rectangle says, apart from its neighbours
    /// and ringed by its own edge; their values are read as a manifest's; a name not handed over
    /// is no piece.
    #[test]
    fn pieces_handed_over_are_packed_into_one_picture_each_where_its_rectangle_says() {
        let mut given = GivenPieces::default();
        let red = Image::solid(5, 3, [0, 0, 255, 255]);
        let mut two = Image::solid(4, 6, [255, 0, 0, 255]);
        two.bgra[0..4].copy_from_slice(&[0, 255, 0, 255]);
        given.pieces.insert("a.red".to_owned(), Arc::new(red));
        given.pieces.insert("b.two".to_owned(), Arc::new(two));
        given.values.insert("a.inset".to_owned(), vec![1.0, 2.0]);
        let art = Art::plain(None, crate::palette::Palette::retail_plain()).with_pieces(given);
        assert!(art.has_piece("a.red") && art.has_piece("b.two"));
        assert!(!art.has_piece("c.none") && art.piece("c.none", 1.0).is_none());
        assert_eq!(art.piece_value("a.inset"), Some(vec![1.0, 2.0]));
        let a = art.piece("a.red", 3.0).expect("a.red");
        let b = art.piece("b.two", 1.0).expect("b.two");
        assert_eq!(a.tex, b.tex, "one picture");
        assert_eq!((a.w, a.h, a.scale), (5.0, 3.0, 1.0), "its own size");
        assert_eq!((b.w, b.h), (4.0, 6.0));
        let overlap = a.src.x < b.src.right() + 1.0
            && b.src.x < a.src.right() + 1.0
            && a.src.y < b.src.bottom() + 1.0
            && b.src.y < a.src.bottom() + 1.0;
        assert!(!overlap, "apart, a pixel between: {:?} {:?}", a.src, b.src);
        let atlas = art.image(a.tex).expect("the packed picture");
        #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
        let at = |s: &Sprite, dx: i32, dy: i32| {
            atlas.pixel((s.src.x as i32 + dx) as u32, (s.src.y as i32 + dy) as u32)
        };
        assert_eq!(at(&a, 0, 0), [0, 0, 255, 255]);
        assert_eq!(at(&a, 4, 2), [0, 0, 255, 255]);
        assert_eq!(at(&b, 0, 0), [0, 255, 0, 255], "its first pixel");
        assert_eq!(at(&b, 1, 0), [255, 0, 0, 255]);
        assert_eq!(at(&b, -1, -1), [0, 255, 0, 255], "ringed by its own corner");
        assert_eq!(at(&a, 5, 1), [0, 0, 255, 255], "and by its own edge");
    }

    /// A picture let go is drawn no more, and the overlay is told once.
    #[test]
    fn a_picture_let_go_is_gone_and_told_once() {
        let art = Art::empty();
        let id = art.add_image(Image::solid(2, 2, [1, 2, 3, 4]));
        art.forget_image(id);
        art.forget_image(id);
        assert!(art.image(id).is_none());
        assert_eq!(art.take_forgotten(), vec![id]);
        assert!(art.take_forgotten().is_empty());
    }
}
