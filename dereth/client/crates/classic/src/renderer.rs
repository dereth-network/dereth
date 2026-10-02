//! The classic interface's integer blits, as the shared overlay draws them: each command becomes
//! a batch of triangles over a texture this module uploads once, in drawing order, with the 3D
//! previews at their places in the list.
use crate::art::ClassicArt;
use crate::int::i32_from;
use crate::{Command, Screen, TextAlign};
use dereth_client_contract::overlay::{
    OverlayItem, OverlayMaterial, OverlaySampler, OverlaySpace, OverlayTexture, OverlayVertex,
    PreviewSpace,
};
use dereth_client_runtime::present::Presentation;
use dereth_primitives::{TextureData, TextureFormat};
use dereth_ui::{region::Box2D, UiSystem};
use std::{collections::BTreeMap, error::Error, sync::Arc};

/// A texture this canvas uploaded.
type TextureSlot = OverlayTexture;

type Result<T> = std::result::Result<T, Box<dyn Error>>;

fn keyed_layer(destination: &mut [u8], source: &[u8], width: u32, height: u32) {
    for y in 0..height.min(32) as usize {
        for x in 0..width.min(32) as usize {
            let s = (y * width as usize + x) * 4;
            let d = (y * 32 + x) * 4;
            let p = &source[s..s + 4];
            // Classic art keys out black; newer icons carry their transparency in alpha.
            if p[3] >= 128 && (p[0] >= 8 || p[1] >= 4 || p[2] >= 8) {
                destination[d..d + 4].copy_from_slice(p);
            }
        }
    }
}

fn expand_indices(indices: &[u8], palette: &[[u8; 4]]) -> Vec<u8> {
    let mut pixels = Vec::with_capacity(indices.len() * 4);
    for index in indices {
        let p = palette[*index as usize];
        let packed =
            (((p[0] as u16 >> 3) << 11) | ((p[1] as u16 >> 2) << 5) | (p[2] as u16 >> 3)).max(1);
        pixels.extend_from_slice(&[
            // Each channel is at most 252 after the shift.
            u8::try_from((packed & 31) << 3).unwrap_or(u8::MAX),
            u8::try_from(((packed >> 5) & 63) << 2).unwrap_or(u8::MAX),
            u8::try_from(((packed >> 11) & 31) << 3).unwrap_or(u8::MAX),
            255,
        ]);
    }
    pixels
}

#[cfg(test)]
mod indexed_tests {
    use super::*;
    #[test]
    fn item_layers_use_quantized_black_key_and_native_badge_extent() {
        let mut pixels = vec![20; 32 * 32 * 4];
        keyed_layer(&mut pixels, &[7, 3, 7, 255, 8, 0, 0, 255], 2, 1);
        assert_eq!(&pixels[..4], &[20; 4]);
        assert_eq!(&pixels[4..8], &[8, 0, 0, 255]);
        assert!(pixels[8..].iter().all(|v| *v == 20));
    }
    #[test]
    fn an_icon_pixel_transparent_in_alpha_leaves_the_background_showing() {
        let mut pixels = vec![20; 32 * 32 * 4];
        keyed_layer(&mut pixels, &[60, 60, 60, 0, 60, 60, 60, 255], 2, 1);
        assert_eq!(&pixels[..4], &[20; 4]);
        assert_eq!(&pixels[4..8], &[60, 60, 60, 255]);
    }
    #[test]
    fn palette_expansion_preserves_classic_565_black_sentinel() {
        let palette = [[0, 0, 0, 255], [255, 255, 255, 255], [17, 33, 65, 255]];
        assert_eq!(
            expand_indices(&[2, 0, 1], &palette),
            [64, 32, 16, 255, 8, 0, 0, 255, 248, 252, 248, 255]
        );
    }
}

#[derive(Clone)]
struct Image {
    width: u32,
    height: u32,
    rgba_file: String,
}
#[derive(Clone)]
struct Glyph {
    x: u32,
    y: u32,
    width: u32,
    height: u32,
    advance: i32,
    bearing_x: i32,
    bearing_y: i32,
}
#[derive(Clone)]
struct Font {
    width: u32,
    height: u32,
    rgba_file: String,
    baseline: i32,
    line_height: i32,
    glyphs: BTreeMap<String, Glyph>,
}
impl Font {
    fn from_atlas(name: &str, atlas: &dereth_classic_dat::fonts::FontAtlas) -> Self {
        Self {
            width: atlas.width,
            height: atlas.height,
            rgba_file: format!("font:{name}"),
            baseline: atlas.baseline,
            line_height: atlas.line_height,
            glyphs: atlas
                .glyphs
                .iter()
                .map(|(c, g)| {
                    (
                        c.to_string(),
                        Glyph {
                            x: g.x.unsigned_abs(),
                            y: g.y.unsigned_abs(),
                            width: g.width.unsigned_abs(),
                            height: g.height.unsigned_abs(),
                            advance: g.advance,
                            bearing_x: g.bearing_x,
                            bearing_y: g.bearing_y,
                        },
                    )
                })
                .collect(),
        }
    }
}
/// The images and fonts a screen can name: the classic portal's, and the icons decoded from the
/// active game data at run time.
struct Manifest {
    art: Arc<ClassicArt>,
    assets: BTreeMap<String, Image>,
    fonts: BTreeMap<String, Font>,
}
impl Manifest {
    fn new(art: Arc<ClassicArt>) -> Self {
        let fonts = art
            .fonts()
            .iter()
            .map(|(name, atlas)| (name.clone(), Font::from_atlas(name, atlas)))
            .collect();
        Self {
            art,
            assets: BTreeMap::new(),
            fonts,
        }
    }
    fn image(&self, did: &str) -> Option<Image> {
        if let Some(image) = self.assets.get(did) {
            return Some(image.clone());
        }
        // "BASE+COLOUR": the base image with its pure white pixels taken from the colour image.
        if let Some((base, _)) = did.split_once('+') {
            let image = self.image(base)?;
            return Some(Image {
                rgba_file: did.to_owned(),
                ..image
            });
        }
        let id = u32::from_str_radix(did, 16).ok()?;
        let image = self.art.image(id)?;
        Some(Image {
            width: image.width,
            height: image.height,
            rgba_file: format!("{id:08X}"),
        })
    }
    fn contains(&self, did: &str) -> bool {
        self.image(did).is_some()
    }
}
/// Replace every pure white pixel of `base` (compared at 16-bit colour depth) with the pixel at
/// the same place in `colour`, the way the selection marks take their colour.
fn recolour_white(base: &mut [u8], colour: &[u8]) {
    for (p, c) in base
        .as_chunks_mut::<4>()
        .0
        .iter_mut()
        .zip(colour.as_chunks::<4>().0)
    {
        if p[0] >= 248 && p[1] >= 252 && p[2] >= 248 {
            p[..3].copy_from_slice(&c[..3]);
        }
    }
}
static PALETTES: std::sync::RwLock<Option<BTreeMap<String, Vec<[u8; 4]>>>> =
    std::sync::RwLock::new(None);
pub fn classic_palette(id: u32) -> Option<Vec<[u8; 4]>> {
    PALETTES
        .read()
        .ok()?
        .as_ref()?
        .get(&format!("{id:08X}"))
        .cloned()
}
static FONT_METRICS: std::sync::RwLock<Option<BTreeMap<String, Font>>> =
    std::sync::RwLock::new(None);
pub fn font_line_height(font: &str) -> Option<i32> {
    FONT_METRICS
        .read()
        .ok()?
        .as_ref()?
        .get(font)
        .map(|f| f.line_height)
}
pub fn measure_text_width(font: &str, text: &str) -> Option<i32> {
    FONT_METRICS
        .read()
        .ok()?
        .as_ref()?
        .get(font)
        .map(|f| measure(f, text))
}
pub fn measure_text_height(font: &str, text: &str, width: i32) -> Option<i32> {
    FONT_METRICS
        .read()
        .ok()?
        .as_ref()?
        .get(font)
        .map(|f| i32_from(text_lines(f, text, width, true).len()) * f.line_height)
}
pub fn measure_rich_text_height(font: &str, runs: &[crate::TextRun], width: i32) -> Option<i32> {
    FONT_METRICS
        .read()
        .ok()?
        .as_ref()?
        .get(font)
        .map(|f| i32_from(rich_lines(f, runs, width, true).len()) * f.line_height)
}

pub struct Canvas {
    runtime_pixels: BTreeMap<String, Vec<u8>>,
    manifest: Manifest,
    textures: BTreeMap<String, TextureSlot>,
    size: (u32, u32),
    white: Option<TextureSlot>,
    /// The next texture key this canvas hands out.
    next_key: u64,
    /// This frame's overlay, built outside the frame bracket and drawn inside it.
    items: Vec<OverlayItem>,
}
impl std::fmt::Debug for Canvas {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Canvas")
            .field("size", &self.size)
            .field("textures", &self.textures.len())
            .finish_non_exhaustive()
    }
}

impl Canvas {
    fn read_pixels(&self, file: &str) -> Result<Vec<u8>> {
        if let Some(pixels) = self.runtime_pixels.get(file) {
            return Ok(pixels.clone());
        }
        if let Some(name) = file.strip_prefix("font:") {
            return Ok(self
                .manifest
                .art
                .fonts()
                .get(name)
                .ok_or_else(|| format!("missing font {name}"))?
                .rgba
                .clone());
        }
        if let Some((base, colour)) = file.split_once('+') {
            let mut pixels = self.read_pixels(base)?;
            let colour = self.read_pixels(colour)?;
            recolour_white(&mut pixels, &colour);
            return Ok(pixels);
        }
        let id = u32::from_str_radix(file, 16)?;
        Ok(self
            .manifest
            .art
            .image(id)
            .ok_or_else(|| format!("missing image {file}"))?
            .rgba
            .clone())
    }
    /// Static theme art comes from the classic bundle. Dynamic model icons may
    /// belong to the active dat set and are decoded without changing its tables.
    pub fn load_runtime_images(
        &mut self,
        screen: &Screen,
        store: &dereth_dat::RetailDatStore,
    ) -> Result<()> {
        let mut ids = std::collections::BTreeSet::new();
        for command in &screen.commands {
            match command {
                Command::Image { did, .. } if !self.manifest.contains(did) => {
                    ids.insert(u32::from_str_radix(did, 16)?);
                }
                Command::ItemIcon { recipe, .. } => {
                    ids.extend(
                        [
                            recipe.background,
                            recipe.underlay,
                            recipe.icon,
                            recipe.overlay,
                            Some(recipe.effects),
                            recipe.badge,
                        ]
                        .into_iter()
                        .flatten(),
                    );
                }
                Command::SpellIcon { icon, .. } => {
                    ids.insert(*icon);
                }
                _ => {}
            }
        }
        let textures = dereth_scene::textures::TextureStore::new(store);
        for id in ids {
            let key = format!("{id:08X}");
            if id == 0 || self.manifest.contains(&key) {
                continue;
            }
            let decoded = textures.bgra8(dereth_primitives::DataId(id))?;
            let pixels = decoded
                .pixels
                .into_iter()
                .flat_map(|[b, g, r, a]| [r, g, b, a])
                .collect();
            let file = format!("active-dat:{key}");
            self.runtime_pixels.insert(file.clone(), pixels);
            self.manifest.assets.insert(
                key,
                Image {
                    width: decoded.width,
                    height: decoded.height,
                    rgba_file: file,
                },
            );
        }
        Ok(())
    }
    fn item_texture<P: Presentation + ?Sized>(
        &mut self,
        gpu: &mut P,
        recipe: &crate::item_art::Recipe,
    ) -> Result<TextureSlot> {
        let key = format!("item:{recipe:?}");
        if let Some(slot) = self.textures.get(&key) {
            return Ok(*slot);
        }
        let read = |id: u32| -> Result<(u32, u32, Vec<u8>)> {
            let asset = self
                .manifest
                .image(&format!("{id:08X}"))
                .ok_or_else(|| format!("missing item art {id:08X}"))?;
            let pixels = self.read_pixels(&asset.rgba_file)?;
            if pixels.len() != (asset.width * asset.height * 4) as usize {
                return Err(format!("wrong item art size {id:08X}").into());
            }
            Ok((asset.width, asset.height, pixels))
        };
        let mut pixels = vec![0; 32 * 32 * 4];
        for id in [
            recipe.background,
            recipe.underlay,
            recipe.icon,
            recipe.overlay,
        ]
        .into_iter()
        .flatten()
        {
            let (w, h, source) = read(id)?;
            keyed_layer(&mut pixels, &source, w, h);
        }
        if recipe.icon.is_some() {
            let (w, h, effects) = read(recipe.effects)?;
            if w != 32 || h != 32 {
                return Err("item effects must be 32x32".into());
            }
            for (p, e) in pixels
                .as_chunks_mut::<4>()
                .0
                .iter_mut()
                .zip(effects.as_chunks::<4>().0)
            {
                if p[0] >= 248 && p[1] >= 252 && p[2] >= 248 {
                    *p = *e;
                }
            }
        }
        if let Some(id) = recipe.badge {
            let (w, h, source) = read(id)?;
            keyed_layer(&mut pixels, &source, w, h);
        }
        for p in pixels.as_chunks_mut::<4>().0 {
            p.swap(0, 2);
        }
        let slot = self.upload(
            gpu,
            &TextureData {
                width: 32,
                height: 32,
                format: TextureFormat::Bgra8,
                levels: vec![pixels],
            },
        )?;
        self.textures.insert(key, slot);
        Ok(slot)
    }
    fn indexed_texture<P: Presentation + ?Sized>(
        &mut self,
        gpu: &mut P,
        did: &str,
        palette: &[[u8; 4]],
    ) -> Result<TextureSlot> {
        if palette.len() != 256 {
            return Err("indexed image needs 256 palette entries".into());
        }
        let key = format!("indexed:{did}:{palette:?}");
        if let Some(slot) = self.textures.get(&key) {
            return Ok(*slot);
        }
        let asset = self
            .manifest
            .art
            .indexed_texture(did)
            .ok_or_else(|| format!("missing indexed image {did}"))?;
        let indices = &asset.indices;
        if indices.len() != asset.width as usize * asset.height as usize {
            return Err(format!("wrong indexed image size: {did}").into());
        }
        let pixels = expand_indices(indices, palette);
        let slot = self.upload(
            gpu,
            &TextureData {
                width: asset.width,
                height: asset.height,
                format: TextureFormat::Bgra8,
                levels: vec![pixels],
            },
        )?;
        self.textures.insert(key, slot);
        Ok(slot)
    }
    fn spell_texture<P: Presentation + ?Sized>(
        &mut self,
        gpu: &mut P,
        icon: u32,
        level: u32,
        bits: u32,
    ) -> Result<TextureSlot> {
        let key = format!("spell:{icon:08X}:{level}:{bits:08X}");
        if let Some(slot) = self.textures.get(&key) {
            return Ok(*slot);
        }
        let read = |id: u32| -> Result<Vec<u8>> {
            let asset = self
                .manifest
                .image(&format!("{id:08X}"))
                .ok_or_else(|| format!("missing spell art {id:08X}"))?;
            if asset.width != 32 || asset.height != 32 {
                return Err(format!("spell art {id:08X} is not32x32").into());
            }
            self.read_pixels(&asset.rgba_file)
        };
        let background = match level {
            1 => 0x060013f4,
            2 => 0x060013f5,
            3 => 0x060013f6,
            4 => 0x060013f7,
            5 => 0x060013f8,
            6 => 0x060013f9,
            7 => 0x06001f63,
            _ => 0,
        };
        let mut pixels = if background == 0 {
            vec![0; 32 * 32 * 4]
        } else {
            read(background)?
        };
        let raw = read(icon)?;
        let reverse = if bits & 0x10 != 0 {
            Some(read(0x060013f2)?)
        } else {
            None
        };
        for (i, p) in pixels.as_chunks_mut::<4>().0.iter_mut().enumerate() {
            let source = &raw[i * 4..i * 4 + 4];
            if source[0] >= 248 && source[1] >= 252 && source[2] >= 248 {
                if let Some(reverse) = &reverse {
                    p.copy_from_slice(&reverse[i * 4..i * 4 + 4]);
                } else {
                    *p = [10, 10, 10, 255];
                }
            } else if source[0] >= 8 || source[1] >= 4 || source[2] >= 8 {
                p.copy_from_slice(source);
            }
        }
        let badge = if bits & 0x2000 != 0 {
            Some(0x060030d7)
        } else if bits & 8 != 0 {
            Some(0x060013f3)
        } else {
            None
        };
        if let Some(badge) = badge {
            let badge = read(badge)?;
            for (p, b) in pixels
                .as_chunks_mut::<4>()
                .0
                .iter_mut()
                .zip(badge.as_chunks::<4>().0)
            {
                if b[0] >= 8 || b[1] >= 4 || b[2] >= 8 {
                    *p = *b;
                }
            }
        }
        for p in pixels.as_chunks_mut::<4>().0 {
            p.swap(0, 2);
        }
        let slot = self.upload(
            gpu,
            &TextureData {
                width: 32,
                height: 32,
                format: TextureFormat::Bgra8,
                levels: vec![pixels],
            },
        )?;
        self.textures.insert(key, slot);
        Ok(slot)
    }
    pub fn new(art: Arc<ClassicArt>, size: (u32, u32)) -> Result<Self> {
        if size.0 == 0 || size.1 == 0 || size.0 > 8192 || size.1 > 8192 {
            return Err("invalid screen dimensions".into());
        }
        let manifest = Manifest::new(art);
        let palettes = manifest
            .art
            .creation()
            .map(|c| c.appearance.palettes.clone())
            .unwrap_or_default();
        *PALETTES.write().map_err(|_| "palette lock poisoned")? = Some(palettes);
        *FONT_METRICS
            .write()
            .map_err(|_| "font metrics lock poisoned")? = Some(manifest.fonts.clone());
        Ok(Self {
            runtime_pixels: BTreeMap::new(),
            manifest,
            textures: BTreeMap::new(),
            size,
            white: None,
            next_key: 1,
            items: Vec::new(),
        })
    }
    pub fn resize(&mut self, size: (u32, u32)) {
        self.size = size;
    }
    /// Upload `data` under a new key of this canvas's own.
    fn upload<P: Presentation + ?Sized>(
        &mut self,
        present: &mut P,
        data: &TextureData,
    ) -> Result<TextureSlot> {
        let texture = OverlayTexture {
            space: OverlaySpace::Local,
            key: self.next_key,
        };
        self.next_key += 1;
        present.overlay_upload(texture, data)?;
        Ok(texture)
    }
    /// The one white texel the fills and highlights are drawn with.
    fn white<P: Presentation + ?Sized>(&mut self, present: &mut P) -> Result<TextureSlot> {
        if let Some(white) = self.white {
            return Ok(white);
        }
        let white = self.upload(
            present,
            &TextureData {
                width: 1,
                height: 1,
                format: TextureFormat::Bgra8,
                levels: vec![vec![255; 4]],
            },
        )?;
        self.white = Some(white);
        Ok(white)
    }
    /// Build this frame's overlay from `screen`, uploading whatever it newly needs; `previews`
    /// names the space and rectangle each preview marker draws. Run outside the frame bracket.
    ///
    /// # Errors
    /// A missing image or font, or the device refusing an upload.
    pub fn compose<P: Presentation + ?Sized>(
        &mut self,
        present: &mut P,
        screen: &Screen,
        previews: &dyn Fn(usize) -> Option<(PreviewSpace, [i32; 4])>,
    ) -> Result<()> {
        self.items.clear();
        let mut layer = Screen {
            width: screen.width,
            height: screen.height,
            commands: vec![],
        };
        for command in &screen.commands {
            if let Command::Preview { index } = command {
                self.draw_prepared(present, &layer)?;
                layer.commands.clear();
                if let Some((space, [x, y, w, h])) = previews(*index) {
                    self.items.push(OverlayItem::Preview {
                        space,
                        rect: dereth_primitives::Viewport {
                            x: x.max(0).unsigned_abs(),
                            y: y.max(0).unsigned_abs(),
                            width: w.max(0).unsigned_abs(),
                            height: h.max(0).unsigned_abs(),
                        },
                    });
                }
            } else {
                layer.commands.push(command.clone());
            }
        }
        self.draw_prepared(present, &layer)
    }
    /// Draw preview space `space` into `rect` first, under everything else this frame.
    pub fn prepend_preview(&mut self, space: PreviewSpace, [x, y, w, h]: [i32; 4]) {
        self.items.insert(
            0,
            OverlayItem::Preview {
                space,
                rect: dereth_primitives::Viewport {
                    x: x.max(0).unsigned_abs(),
                    y: y.max(0).unsigned_abs(),
                    width: w.max(0).unsigned_abs(),
                    height: h.max(0).unsigned_abs(),
                },
            },
        );
    }
    /// This frame's overlay, as [`Self::compose`] built it.
    #[must_use]
    pub fn items(&self) -> &[OverlayItem] {
        &self.items
    }
    fn texture<P: Presentation + ?Sized>(
        &mut self,
        gpu: &mut P,
        file: &str,
        width: u32,
        height: u32,
        key: Option<[u8; 3]>,
        key_bits: Option<[u8; 3]>,
    ) -> Result<TextureSlot> {
        let bits = key_bits.unwrap_or([8; 3]);
        if bits.iter().any(|b| *b == 0 || *b > 8) {
            return Err("invalid key precision".into());
        }
        let cache_key = format!("{file}:{key:?}:{bits:?}");
        if let Some(slot) = self.textures.get(&cache_key) {
            return Ok(*slot);
        }
        let mut rgba = self.read_pixels(file)?;
        if rgba.len() != width as usize * height as usize * 4 {
            return Err(format!("wrong RGBA size: {file}").into());
        }
        for p in rgba.as_chunks_mut::<4>().0 {
            if key.is_some_and(|key| {
                (0..3).all(|i| (p[i] >> (8 - bits[i])) == (key[i] >> (8 - bits[i])))
            }) {
                p[3] = 0;
            }
            p.swap(0, 2);
        }
        let slot = self.upload(
            gpu,
            &TextureData {
                width,
                height,
                format: TextureFormat::Bgra8,
                levels: vec![rgba],
            },
        )?;
        self.textures.insert(cache_key, slot);
        Ok(slot)
    }

    fn submit(&mut self, texture: TextureSlot, vertices: &[OverlayVertex], invert: bool) {
        if vertices.is_empty() {
            return;
        }
        self.items.push(OverlayItem::Triangles {
            material: if invert {
                OverlayMaterial::Invert
            } else {
                OverlayMaterial::Image
            },
            texture,
            sampler: OverlaySampler::POINT_CLAMP,
            vertices: vertices.to_vec(),
        });
    }

    fn draw_prepared<P: Presentation + ?Sized>(
        &mut self,
        gpu: &mut P,
        screen: &Screen,
    ) -> Result<()> {
        let screen = self.layout(screen)?;
        if (screen.width, screen.height) != self.size {
            return Err("renderer/screen size mismatch".into());
        }
        // Hollow current-UI regions provide clipping and coordinate conversion. Old event
        // semantics live in the independent widget layer, not in ToD control classes.
        let mut ui = UiSystem::new((screen.width as i32, screen.height as i32));
        let root = ui.root();
        for command in &screen.commands {
            let (x, y, w, h, clip) = match command {
                Command::Invert {
                    rect: [x, y, w, h],
                    clip,
                } => (*x, *y, (*w).max(0) as u32, (*h).max(0) as u32, *clip),
                Command::ItemIcon {
                    x,
                    y,
                    width,
                    height,
                    clip,
                    ..
                } => (*x, *y, *width, *height, *clip),
                Command::Preview { .. } => continue,
                Command::IndexedImage {
                    x,
                    y,
                    width,
                    height,
                    clip,
                    ..
                } => (*x, *y, *width, *height, *clip),
                Command::SpellIcon {
                    x,
                    y,
                    width,
                    height,
                    clip,
                    ..
                } => (*x, *y, *width, *height, *clip),
                Command::TextBox { .. } | Command::RichTextBox { .. } => {
                    unreachable!("text boxes expanded")
                }
                Command::Image {
                    x,
                    y,
                    width,
                    height,
                    clip,
                    ..
                } => (*x, *y, *width, *height, *clip),
                Command::Fill {
                    x,
                    y,
                    width,
                    height,
                    ..
                } => (*x, *y, *width, *height, None),
                Command::Text { x, y, clip, .. } => (*x, *y, screen.width, screen.height, *clip),
            };
            let clip = clip.unwrap_or([0, 0, screen.width as i32, screen.height as i32]);
            let parent = ui.create_hollow(Some(root));
            ui.move_to(parent, clip[0], clip[1]);
            ui.resize_to(
                parent,
                (clip[2] - clip[0]).max(0),
                (clip[3] - clip[1]).max(0),
            );
            let element = ui.create_hollow(Some(parent));
            ui.move_to(element, x - clip[0], y - clip[1]);
            ui.resize_to(element, w as i32, h as i32);
            let bounds = if matches!(command, Command::Text { .. }) {
                // A text pen is not a clip edge: italic/serif glyphs may extend
                // left of it, above the line, or beyond their advance width.
                ui.screen_clip_box(parent)
            } else {
                ui.screen_clip_box(element)
            };
            let mut vertices = Vec::new();
            let texture = match command {
                Command::Invert { .. } => {
                    quad(
                        &mut vertices,
                        [x, y, w as i32, h as i32],
                        [0., 0., 1., 1.],
                        bounds,
                        0xffffffff,
                        self.size,
                    );
                    self.white(gpu)?
                }
                Command::ItemIcon { recipe, .. } => {
                    quad(
                        &mut vertices,
                        [x, y, w as i32, h as i32],
                        [0., 0., 1., 1.],
                        bounds,
                        0xffffffff,
                        self.size,
                    );
                    self.item_texture(gpu, recipe)?
                }
                Command::Preview { .. } => unreachable!(),
                Command::IndexedImage {
                    did,
                    palette,
                    flip_x,
                    ..
                } => {
                    quad(
                        &mut vertices,
                        [x, y, w as i32, h as i32],
                        if *flip_x {
                            [1., 0., 0., 1.]
                        } else {
                            [0., 0., 1., 1.]
                        },
                        bounds,
                        0xffffffff,
                        self.size,
                    );
                    self.indexed_texture(gpu, did, palette)?
                }
                Command::SpellIcon {
                    icon,
                    level,
                    bitfield,
                    ..
                } => {
                    quad(
                        &mut vertices,
                        [x, y, w as i32, h as i32],
                        [0., 0., 1., 1.],
                        bounds,
                        0xffffffff,
                        self.size,
                    );
                    self.spell_texture(gpu, *icon, *level, *bitfield)?
                }
                Command::TextBox { .. } | Command::RichTextBox { .. } => {
                    unreachable!("text boxes expanded")
                }
                Command::Image {
                    did,
                    color_key,
                    key_bits,
                    tile,
                    ..
                } => {
                    let image = self
                        .manifest
                        .image(did)
                        .ok_or_else(|| format!("missing image {did}"))?;
                    if *tile {
                        if image.width == 0 || image.height == 0 {
                            return Err("empty tile".into());
                        }
                        for dy in (0..h).step_by(image.height as usize) {
                            for dx in (0..w).step_by(image.width as usize) {
                                quad(
                                    &mut vertices,
                                    [
                                        x + dx as i32,
                                        y + dy as i32,
                                        image.width as i32,
                                        image.height as i32,
                                    ],
                                    [0., 0., 1., 1.],
                                    bounds,
                                    0xffffffff,
                                    self.size,
                                );
                            }
                        }
                    } else {
                        quad(
                            &mut vertices,
                            [x, y, w as i32, h as i32],
                            [0., 0., 1., 1.],
                            bounds,
                            0xffffffff,
                            self.size,
                        );
                    }
                    self.texture(
                        gpu,
                        &image.rgba_file,
                        image.width,
                        image.height,
                        *color_key,
                        *key_bits,
                    )?
                }
                Command::Fill { color, .. } => {
                    quad(
                        &mut vertices,
                        [x, y, w as i32, h as i32],
                        [0., 0., 1., 1.],
                        bounds,
                        *color,
                        self.size,
                    );
                    self.white(gpu)?
                }
                Command::Text {
                    text, font, color, ..
                } => {
                    let font = self
                        .manifest
                        .fonts
                        .get(font)
                        .ok_or_else(|| format!("missing font {font}"))?
                        .clone();
                    let mut pen = x;
                    for ch in text.chars() {
                        let glyph = font
                            .glyphs
                            .get(&(ch as u32).to_string())
                            .or_else(|| font.glyphs.get("63"))
                            .ok_or_else(|| format!("missing glyph U+{:04X}", ch as u32))?;
                        quad(
                            &mut vertices,
                            [
                                pen + glyph.bearing_x,
                                y + font.baseline + glyph.bearing_y,
                                glyph.width as i32,
                                glyph.height as i32,
                            ],
                            [
                                glyph.x as f32 / font.width as f32,
                                glyph.y as f32 / font.height as f32,
                                (glyph.x + glyph.width) as f32 / font.width as f32,
                                (glyph.y + glyph.height) as f32 / font.height as f32,
                            ],
                            bounds,
                            *color,
                            self.size,
                        );
                        pen += glyph.advance;
                    }
                    self.texture(gpu, &font.rgba_file, font.width, font.height, None, None)?
                }
            };
            self.submit(
                texture,
                &vertices,
                matches!(command, Command::Invert { .. }),
            );
        }
        Ok(())
    }

    pub fn text_width(&self, font: &str, text: &str) -> i32 {
        self.manifest
            .fonts
            .get(font)
            .map_or(0, |font| measure(font, text))
    }

    fn layout(&self, source: &Screen) -> Result<Screen> {
        let mut screen = Screen {
            width: source.width,
            height: source.height,
            commands: vec![],
        };
        for command in &source.commands {
            if let Command::RichTextBox {
                runs,
                rect: [x, y, w, h],
                font,
                align,
                wrap,
                clip,
            } = command
            {
                let metrics = self
                    .manifest
                    .fonts
                    .get(font)
                    .ok_or_else(|| format!("missing font {font}"))?;
                let mut bounds = [*x, *y, x + w, y + h];
                if let Some(c) = clip {
                    bounds = [
                        bounds[0].max(c[0]),
                        bounds[1].max(c[1]),
                        bounds[2].min(c[2]),
                        bounds[3].min(c[3]),
                    ];
                }
                for (row, line) in rich_lines(metrics, runs, *w, *wrap).into_iter().enumerate() {
                    let top = y + i32_from(row) * metrics.line_height;
                    if top >= y + h {
                        break;
                    }
                    let width: i32 = line
                        .iter()
                        .map(|(c, _)| measure(metrics, &c.to_string()))
                        .sum();
                    let mut pen = x + match align {
                        TextAlign::Left => 0,
                        TextAlign::Center => (w - width) / 2,
                        TextAlign::Right => w - width,
                    };
                    let mut start = 0;
                    while start < line.len() {
                        let color = line[start].1;
                        let mut end = start + 1;
                        while end < line.len() && line[end].1 == color {
                            end += 1;
                        }
                        let text: String = line[start..end].iter().map(|(c, _)| *c).collect();
                        let advance = measure(metrics, &text);
                        screen.commands.push(Command::Text {
                            text,
                            x: pen,
                            y: top,
                            font: font.clone(),
                            color,
                            clip: Some(bounds),
                        });
                        pen += advance;
                        start = end;
                    }
                }
                continue;
            }
            if let Command::TextBox {
                text,
                rect: [x, y, w, h],
                font,
                color,
                align,
                wrap,
                clip,
            } = command
            {
                let metrics = self
                    .manifest
                    .fonts
                    .get(font)
                    .ok_or_else(|| format!("missing font {font}"))?;
                let mut bounds = [*x, *y, x + w, y + h];
                if let Some(c) = clip {
                    bounds = [
                        bounds[0].max(c[0]),
                        bounds[1].max(c[1]),
                        bounds[2].min(c[2]),
                        bounds[3].min(c[3]),
                    ];
                }
                for (row, line) in text_lines(metrics, text, *w, *wrap).into_iter().enumerate() {
                    let top = y + i32_from(row) * metrics.line_height;
                    if top >= y + h {
                        break;
                    }
                    let width = measure(metrics, &line);
                    let left = x + match align {
                        TextAlign::Left => 0,
                        TextAlign::Center => (w - width) / 2,
                        TextAlign::Right => w - width,
                    };
                    screen.commands.push(Command::Text {
                        text: line,
                        x: left,
                        y: top,
                        font: font.clone(),
                        color: *color,
                        clip: Some(bounds),
                    });
                }
            } else if let Command::Image {
                did,
                width: 0,
                height: 0,
                ..
            } = command
            {
                // An image with no size is drawn at its own size.
                let mut command = command.clone();
                if let (
                    Some(image),
                    Command::Image {
                        width: w,
                        height: h,
                        ..
                    },
                ) = (self.manifest.image(did), &mut command)
                {
                    *w = image.width;
                    *h = image.height;
                }
                screen.commands.push(command);
            } else {
                screen.commands.push(command.clone());
            }
        }
        Ok(screen)
    }
}

fn measure(font: &Font, text: &str) -> i32 {
    text.chars()
        .map(|c| {
            font.glyphs
                .get(&(c as u32).to_string())
                .or_else(|| font.glyphs.get("63"))
                .map_or(0, |g| g.advance)
        })
        .sum()
}

fn rich_lines(
    font: &Font,
    runs: &[crate::TextRun],
    width: i32,
    wrap: bool,
) -> Vec<Vec<(char, u32)>> {
    let mut lines = vec![];
    let mut line: Vec<(char, u32)> = vec![];
    for run in runs {
        for c in run.text.chars() {
            if c == '\r' {
                continue;
            }
            if c == '\n' {
                lines.push(std::mem::take(&mut line));
                continue;
            }
            let used: i32 = line
                .iter()
                .map(|(c, _)| measure(font, &c.to_string()))
                .sum();
            if wrap && !line.is_empty() && used + measure(font, &c.to_string()) > width {
                if let Some(space) = line.iter().rposition(|(c, _)| c.is_whitespace()) {
                    let tail = line.split_off(space + 1);
                    line.truncate(space);
                    lines.push(std::mem::replace(&mut line, tail));
                } else {
                    lines.push(std::mem::take(&mut line));
                }
            }
            if !(wrap && line.is_empty() && c == ' ') {
                line.push((c, run.color));
            }
        }
    }
    lines.push(line);
    lines
}

fn text_lines(font: &Font, text: &str, width: i32, wrap: bool) -> Vec<String> {
    let mut lines = vec![];
    for paragraph in text.split('\n') {
        if !wrap {
            lines.push(paragraph.trim_end_matches('\r').to_owned());
            continue;
        }
        let mut line = String::new();
        for word in paragraph.split_whitespace() {
            let candidate = if line.is_empty() {
                word.to_owned()
            } else {
                format!("{line} {word}")
            };
            if !line.is_empty() && measure(font, &candidate) > width {
                lines.push(std::mem::take(&mut line));
            }
            if !line.is_empty() {
                line.push(' ');
            }
            for ch in word.chars() {
                let mut candidate = line.clone();
                candidate.push(ch);
                if !line.is_empty() && measure(font, &candidate) > width {
                    lines.push(std::mem::take(&mut line));
                }
                line.push(ch);
            }
        }
        lines.push(line);
    }
    lines
}

fn quad(
    out: &mut Vec<OverlayVertex>,
    rect: [i32; 4],
    uv: [f32; 4],
    clip: Box2D,
    color: u32,
    size: (u32, u32),
) {
    let [x, y, w, h] = rect;
    if w <= 0 || h <= 0 {
        return;
    }
    let visible = Box2D::from_xywh(x, y, w, h).intersect(&clip);
    if !visible.is_valid() {
        return;
    }
    let u0 = uv[0] + (uv[2] - uv[0]) * (visible.x0 - x) as f32 / w as f32;
    let u1 = uv[0] + (uv[2] - uv[0]) * (visible.x1 + 1 - x) as f32 / w as f32;
    let v0 = uv[1] + (uv[3] - uv[1]) * (visible.y0 - y) as f32 / h as f32;
    let v1 = uv[1] + (uv[3] - uv[1]) * (visible.y1 + 1 - y) as f32 / h as f32;
    // These are integer DirectDraw/GDI destination edges. The ToD font helper's
    // D3D9 half-pixel adjustment does not apply to this modern blit submission.
    let left = 2.0 * visible.x0 as f32 / size.0 as f32 - 1.0;
    let right = 2.0 * (visible.x1 + 1) as f32 / size.0 as f32 - 1.0;
    let top = 1.0 - 2.0 * visible.y0 as f32 / size.1 as f32;
    let bottom = 1.0 - 2.0 * (visible.y1 + 1) as f32 / size.1 as f32;
    // The overlay draws its triangles counter-clockwise, at the depth the other interface uses.
    let vertex = |x, y, u, v| OverlayVertex {
        position: [x, y, 0.5],
        color,
        uv: [u, v],
    };
    let top_left = vertex(left, top, u0, v0);
    let top_right = vertex(right, top, u1, v0);
    let bottom_left = vertex(left, bottom, u0, v1);
    let bottom_right = vertex(right, bottom, u1, v1);
    out.extend_from_slice(&[
        top_left,
        bottom_left,
        bottom_right,
        bottom_right,
        top_right,
        top_left,
    ]);
}
