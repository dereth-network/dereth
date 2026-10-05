//! Resolving a dat texture id to pixels.
//!
//! **Nothing here decodes anything.** The container is `dereth_dat`, the record layouts are
//! `dereth_assets`, and the pixel expansion is `dereth_render::texture` / `dereth_render::dxt` /
//! `dereth_render::palette`. This module owns the three-hop *lookup* from a surface id to the
//! render-surface payload:
//!
//! ```text
//! Surface record (0x08) --orig_texture_id--> SurfaceTexture (0x05) --source_levels[0]--> RenderSurface (0x06)
//! ```
//!
//! `TexMerge`'s `TerrainDesc::tex_gid` and `CodeTexture::tex_gid` name the middle link directly, and
//! a `GfxObj`'s surface list names the first, so all three entry points are accepted and the hop is
//! chosen by `dereth_dat::divine_type`.
//!
//! This module carries texture records from dat ids through decoding into GPU textures.

use dereth_assets::texture_lookup::{LookupError, TextureLookup};
use dereth_assets::RenderSurface;
use dereth_dat::RetailDatStore;
use dereth_primitives::{DataId, TextureData, TextureFormat};
use dereth_render::palette::ExpandedPalette;
use dereth_render::pixel_format::PixelFormatId;
use dereth_render::texture::{decode_surface, SourcePixels};
use dereth_terrain::land::merge::Bgra8;

/// Anything the lookup can refuse to do. A missing texture is **not** an error at the call sites:
/// the terrain compositor renders a missing tile as the client's `00 FF 00 00` debug colour
/// (`copy_and_tile`), and an object surface that will not resolve is drawn untextured.
#[derive(Debug, thiserror::Error)]
pub enum TextureError {
    #[error("{0}: {1}")]
    Dat(DataId, dereth_dat::DatError),
    #[error("{0}: {1}")]
    Asset(DataId, dereth_assets::AssetError),
    #[error("{0}: {1}")]
    Decode(DataId, dereth_render::RenderError),
    #[error("{0} is not a texture id, or names an empty level chain")]
    NotATexture(DataId),
}

impl From<LookupError> for TextureError {
    fn from(error: LookupError) -> Self {
        match error {
            LookupError::Dat(id, error) => Self::Dat(id, error),
            LookupError::Asset(id, error) => Self::Asset(id, error),
            LookupError::NotATexture(id) => Self::NotATexture(id),
        }
    }
}

/// The decoded pixels of one texture id, plus the palette it needed.
///
/// A block-compressed source stays compressed for [`TextureData`] (that is what the client does —
/// its texture creation copies DXT blocks verbatim) and is expanded for [`Bgra8`], because the
/// terrain compositor blends texels on the CPU.
#[derive(Debug)]
pub struct TextureStore<'a> {
    lookup: TextureLookup<'a>,
    /// The other era's files, where a part drawn from these records reads some of its colour
    /// ranges ([`Self::with_colours_from`]).
    colours: Option<(TextureLookup<'a>, Vec<bool>)>,
}

/// Three colour planes of `width` by `height` bytes, red, green and blue one after another, as
/// one interleaved blue-green-red image. `None` when the planes are short.
#[must_use]
pub fn interleave_rgb_planes(planes: &[u8], width: u32, height: u32) -> Option<Vec<u8>> {
    let n = width as usize * height as usize;
    let (r, rest) = planes.split_at_checked(n)?;
    let (g, rest) = rest.split_at_checked(n)?;
    let b = rest.get(..n)?;
    let mut out = Vec::with_capacity(n * 3);
    for i in 0..n {
        out.extend_from_slice(&[b[i], g[i], r[i]]);
    }
    Some(out)
}

impl<'a> TextureStore<'a> {
    /// A store at the registered `Render.EnvironmentTextureDetail` default, which is not the
    /// highest setting, so a two-level `SurfaceTexture` resolves to its original art.
    #[must_use]
    pub fn new(store: &'a RetailDatStore) -> Self {
        Self::with_environment_texture_detail(
            store,
            dereth_client_runtime::render_prefs::RenderPreferences::default()
                .environment_texture_detail,
        )
    }

    /// A store at a given `Render.EnvironmentTextureDetail`. The high-detail drop predicate keeps
    /// the high-res level only when the database has the HiFi file (`0x69466948`) and
    /// `EnvironmentTextureDetail` is 0, and drops it otherwise, so the high-res level survives
    /// only with the dat granted by the server ([`RetailDatStore::grant_highres`]) and the
    /// preference at its highest, 0. At the common `Medium` (2) setting, or against a server
    /// that grants nothing (ACE's default), the low-res level is used.
    #[must_use]
    pub fn with_environment_texture_detail(store: &'a RetailDatStore, detail: u32) -> Self {
        Self {
            lookup: TextureLookup::new(store, detail),
            colours: None,
        }
    }

    /// This store with `look`, the other era's files, as where [`Self::look_palette`] reads, for
    /// a part drawn from these records with the look's colours: `ranges` says, colour range by
    /// colour range of the part's description, which are read there
    /// ([`dereth_client_runtime::models::colours_for_look`]).
    #[must_use]
    pub fn with_colours_from(mut self, look: &'a RetailDatStore, ranges: Vec<bool>) -> Self {
        // Only palettes are read there, which no detail setting changes.
        let detail = u32::from(!self.lookup.keeps_high_detail());
        self.colours = Some((TextureLookup::new(look, detail), ranges));
        self
    }

    /// Which colour ranges [`Self::look_palette`] stands for ([`Self::with_colours_from`]);
    /// empty when every range reads this store.
    #[must_use]
    pub fn look_ranges(&self) -> &[bool] {
        self.colours.as_ref().map_or(&[], |(_, r)| r.as_slice())
    }

    /// One `Palette` of the other era's files ([`Self::with_colours_from`]), expanded as
    /// [`Self::palette`] does. `None` without them or when they lack it.
    #[must_use]
    pub fn look_palette(&self, id: DataId) -> Option<ExpandedPalette> {
        let p = self.colours.as_ref()?.0.palette(id).ok()?;
        ExpandedPalette::from_dat(&p.colors_argb)
    }

    /// Whether this store resolves a two-level `SurfaceTexture` to its high-res level.
    #[must_use]
    pub fn keeps_high_detail(&self) -> bool {
        self.lookup.keeps_high_detail()
    }

    /// Follow the shared record lookup to the render-surface payload.
    ///
    /// # Errors
    /// A missing, malformed or non-texture record in the chain.
    pub fn resolve(&self, id: DataId) -> Result<(DataId, RenderSurface, Vec<u8>), TextureError> {
        self.lookup.resolve(id).map_err(Into::into)
    }

    /// The GPU-ready form: DXT stays compressed, everything else lands as BGRA8.
    ///
    /// # Errors
    /// [`TextureError`] when the chain is broken or the payload will not decode.
    pub fn texture_data(&self, id: DataId) -> Result<TextureData, TextureError> {
        self.texture_data_clipped(id, false)
    }

    /// [`Self::texture_data`] for one layer of an item icon.
    ///
    /// The icon images of the dat set before Throne of Destiny carry no alpha: their pure black is
    /// the transparent colour around the picture, where the later files store alpha instead. Only
    /// an icon's layers are keyed so; every other image of those files, the interface art among
    /// them, draws its black opaque.
    ///
    /// # Errors
    /// [`TextureError`] when the chain is broken or the payload will not decode.
    pub fn icon_data(&self, id: DataId) -> Result<TextureData, TextureError> {
        let (rsid, rs, bytes) = self.resolve(id)?;
        let payload = rs.payload(&bytes).ok_or(TextureError::NotATexture(rsid))?;
        let mut data = self.decode_render_surface(rsid, &rs, payload, false, None)?;
        if PixelFormatId::from_raw(rs.format) == PixelFormatId::CustomB8G8R8
            && self.lookup.era_of(rsid) == dereth_dat::ContainerEra::Classic
            && data.format == TextureFormat::Bgra8
        {
            for level in &mut data.levels {
                for px in level.as_chunks_mut::<4>().0 {
                    if px[..3] == [0, 0, 0] {
                        px[3] = 0;
                    }
                }
            }
        }
        Ok(data)
    }

    /// [`Self::texture_data`], with the outer surface record's `BASE1_CLIPMAP` bit.
    ///
    /// The flag reaches the palettised decoders and nothing else, and it cannot be recovered from
    /// inside the id chain — the surface record that carries it sits at the *top*, and by the time a
    /// `RenderSurface` is in hand it is two hops out of reach. So the caller that read the surface
    /// passes it down.
    ///
    /// # Errors
    /// [`TextureError`] when the chain is broken or the payload will not decode.
    pub fn texture_data_clipped(
        &self,
        id: DataId,
        clip_map: bool,
    ) -> Result<TextureData, TextureError> {
        self.texture_data_shifted(id, clip_map, None)
    }

    /// [`Self::texture_data_clipped`], with an `ObjDesc`'s **shift palette** substituted for the
    /// texture's own default.
    ///
    /// This is surface setup's `SH_PALSHIFT` arm:
    /// Palette-shift restoration loads the indexed image texture, takes the base palette — which
    /// surface setup has just replaced with the part array's shift palette — and combines
    /// `(indexed texture, palette, clip-map flag)`.
    ///
    /// Palette expansion **requires** the source to be `PFID_P8` or `PFID_INDEX16`, so a
    /// shift palette on any other format is not consulted at all; that is what
    /// [`Self::is_palettised`] answers for a caller that has to key a cache.
    ///
    /// # Errors
    /// [`TextureError`] when the chain is broken or the payload will not decode.
    pub fn texture_data_shifted(
        &self,
        id: DataId,
        clip_map: bool,
        shift: Option<&ExpandedPalette>,
    ) -> Result<TextureData, TextureError> {
        let (rsid, rs, bytes) = self.resolve(id)?;
        let payload = rs.payload(&bytes).ok_or(TextureError::NotATexture(rsid))?;
        self.decode_render_surface(rsid, &rs, payload, clip_map, shift)
    }

    /// Whether the id chain ends at a palettised `RenderSurface`, i.e. whether a shift palette can
    /// change anything about it. `PFID_P8 (41)` and `PFID_INDEX16 (101)` are the two formats
    /// accepted by palette combination.
    ///
    /// # Errors
    /// [`TextureError`] when the chain is broken.
    pub fn is_palettised(&self, id: DataId) -> Result<bool, TextureError> {
        let (_, rs, _) = self.resolve(id)?;
        Ok(matches!(
            PixelFormatId::from_raw(rs.format),
            PixelFormatId::P8 | PixelFormatId::Index16
        ))
    }

    /// One dat `Palette` (`0x04`), expanded to the 2048 entries every palettised path assumes.
    #[must_use]
    pub fn palette(&self, id: DataId) -> Option<ExpandedPalette> {
        let p = self.lookup.palette(id).ok()?;
        ExpandedPalette::from_dat(&p.colors_argb)
    }

    /// The decode itself, shared by [`Self::texture_data_clipped`] and [`Self::bgra8`] so neither
    /// resolves the id chain twice.
    fn decode_render_surface(
        &self,
        rsid: DataId,
        rs: &RenderSurface,
        payload: &[u8],
        clip_map: bool,
        shift: Option<&ExpandedPalette>,
    ) -> Result<TextureData, TextureError> {
        let format = PixelFormatId::from_raw(rs.format);
        let own = self.palette_for(rs);
        // Retail's palette-shift surface restore hands `base1pal` — the part's shift palette — to
        // the combined-texture creation **in place of** the texture's own default, and only for the
        // two palettised formats accepted by palette combination.
        let palettised = matches!(format, PixelFormatId::P8 | PixelFormatId::Index16);
        let palette: Option<&ExpandedPalette> = match shift {
            Some(p) if palettised => Some(p),
            _ => own.as_ref(),
        };
        // `PFID_INDEX16` cannot go through `source_pixels`: `SourcePixels::Palettised16` borrows a
        // `&[u16]`, and the payload is bytes, so the widened buffer has to be owned by whoever
        // calls `decode_surface`. That is very likely why the arm was missing — and the failure was
        // silent, because 101 fell through to `Direct`, which is for formats D3D accepts natively,
        // and produced an all-black image with no error. Every part of a human body is INDEX16.
        // A 256-colour image of the dat set before Throne of Destiny indexes that era's 256-entry
        // palette, which the expanded palette holds eight times over: colour `i` is entry `8i`.
        let pre_tod_p8 = format == PixelFormatId::P8
            && self.lookup.era_of(rsid) == dereth_dat::ContainerEra::Classic;
        if format == PixelFormatId::Index16 || pre_tod_p8 {
            let table = palette.ok_or(TextureError::NotATexture(rsid))?;
            let indices: Vec<u16> = if pre_tod_p8 {
                payload.iter().map(|&i| u16::from(i) << 3).collect()
            } else {
                payload
                    .as_chunks::<2>()
                    .0
                    .iter()
                    .map(|c| u16::from_le_bytes(*c))
                    .collect()
            };
            let src = SourcePixels::Palettised16 {
                indices: &indices,
                palette: table,
                clip_map,
            };
            return decode_surface(src, rs.width, rs.height)
                .map_err(|e| TextureError::Decode(rsid, e));
        }
        // A 24-bit landscape image of the dat set before Throne of Destiny stores its colour as
        // three planes one after another, red, green, then blue; the later files interleave each
        // pixel's blue, green and red. The planes are interleaved here into the later order.
        if format == PixelFormatId::CustomLscapeR8G8B8
            && self.lookup.era_of(rsid) == dereth_dat::ContainerEra::Classic
        {
            let bgr = interleave_rgb_planes(payload, rs.width, rs.height)
                .ok_or(TextureError::NotATexture(rsid))?;
            return decode_surface(SourcePixels::LandscapeRgb(&bgr), rs.width, rs.height)
                .map_err(|e| TextureError::Decode(rsid, e));
        }
        let src = self.source_pixels(format, payload, palette, clip_map)?;
        decode_surface(src, rs.width, rs.height).map_err(|e| TextureError::Decode(rsid, e))
    }

    /// The CPU-readable form the terrain compositor needs: always BGRA8.
    ///
    /// # Errors
    /// [`TextureError`] when the chain is broken or the payload will not decode.
    pub fn bgra8(&self, id: DataId) -> Result<Bgra8, TextureError> {
        let (rsid, rs, bytes) = self.resolve(id)?;
        let payload = rs.payload(&bytes).ok_or(TextureError::NotATexture(rsid))?;
        let format = PixelFormatId::from_raw(rs.format);
        let pixels = match format {
            PixelFormatId::Dxt1
            | PixelFormatId::Dxt2
            | PixelFormatId::Dxt3
            | PixelFormatId::Dxt4
            | PixelFormatId::Dxt5 => {
                dereth_render::dxt::decode(format, payload, rs.width, rs.height)
                    .map_err(|e| TextureError::Decode(rsid, e))?
            }
            _ => {
                // No terrain texture is clip-mapped: the compositor blends through an alpha map
                // rather than testing against a key colour.
                let data = self.decode_render_surface(rsid, &rs, payload, false, None)?;
                if data.format != TextureFormat::Bgra8 {
                    return Err(TextureError::NotATexture(rsid));
                }
                data.levels.into_iter().next().unwrap_or_default()
            }
        };
        let n = rs.width as usize * rs.height as usize;
        if pixels.len() < n * 4 {
            return Err(TextureError::NotATexture(rsid));
        }
        Ok(Bgra8 {
            width: rs.width,
            height: rs.height,
            pixels: pixels.as_chunks::<4>().0[..n].to_vec(),
        })
    }

    /// The palettised decode arms read the surface's default palette id. Only formats 41 (`P8`) and
    /// 101 (`INDEX16`) carry one, and both are decoded here —
    /// `INDEX16` in [`Self::decode_render_surface`], because it needs an owned index buffer.
    fn palette_for(&self, rs: &RenderSurface) -> Option<ExpandedPalette> {
        let id = rs.default_palette_id?;
        let p = self.lookup.palette(id).ok()?;
        ExpandedPalette::from_dat(&p.colors_argb)
    }

    fn source_pixels<'p>(
        &self,
        format: PixelFormatId,
        payload: &'p [u8],
        palette: Option<&'p ExpandedPalette>,
        clip_map: bool,
    ) -> Result<SourcePixels<'p>, TextureError> {
        Ok(match format {
            PixelFormatId::Dxt1
            | PixelFormatId::Dxt2
            | PixelFormatId::Dxt3
            | PixelFormatId::Dxt4
            | PixelFormatId::Dxt5 => SourcePixels::Dxt {
                format,
                blocks: payload,
            },
            PixelFormatId::CustomRawJpeg => SourcePixels::Jpeg(payload),
            PixelFormatId::CustomLscapeR8G8B8 => SourcePixels::LandscapeRgb(payload),
            PixelFormatId::CustomLscapeAlpha => SourcePixels::LandscapeAlpha(payload),
            PixelFormatId::P8 => {
                let palette = palette.ok_or(TextureError::NotATexture(DataId(0)))?;
                SourcePixels::Palettised8 {
                    indices: payload,
                    palette,
                    clip_map,
                }
            }
            other => SourcePixels::Direct {
                format: other,
                bits: payload,
                pitch: 0,
            },
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Three planes become one image whose pixels are blue, green, red, each taken from the same
    /// place in the three planes; short planes are refused.
    #[test]
    fn three_colour_planes_interleave_into_blue_green_red_pixels() {
        let planes = [1, 2, 10, 20, 100, 200];
        assert_eq!(
            interleave_rgb_planes(&planes, 2, 1),
            Some(vec![100, 10, 1, 200, 20, 2])
        );
        assert_eq!(interleave_rgb_planes(&planes[..5], 2, 1), None);
    }

    /// A February 2005 landscape image reads as its three planes: the grass is green over brown,
    /// the barren rock brown, as their names say. Read as interleaved pixels they would be grey
    /// noise.
    #[test]
    #[cfg_attr(
        not(feature = "retail-dats"),
        ignore = "reads the February 2005 and end-of-retail dats: --features retail-dats"
    )]
    fn a_february_2005_landscape_image_reads_as_three_colour_planes() {
        let old = dereth_dat::testing::classic_dat_dir().unwrap_or_else(|| {
            panic!(
                "{}",
                dereth_dat::testing::classic_shortfall().unwrap_or_default()
            )
        });
        let hybrid =
            RetailDatStore::open_classic_with_modern(&old, &dereth_dat::testing::dat_dir())
                .expect("the February 2005 dats beside the end-of-retail ones");
        let textures = TextureStore::new(&hybrid);
        let mean = |id: u32| {
            let img = textures.bgra8(DataId(id)).expect("decodes");
            assert_eq!((img.width, img.height), (128, 128));
            let n = img.pixels.len() as u64;
            let sum = |c: usize| img.pixels.iter().map(|p| u64::from(p[c])).sum::<u64>() / n;
            (sum(2), sum(1), sum(0))
        };
        let (r, g, b) = mean(0x0500_1459);
        assert!(g > r && r > b, "grassland {r} {g} {b}");
        let (r, g, b) = mean(0x0500_145C);
        assert!(r > g && g > b, "barren rock {r} {g} {b}");
    }

    /// A February 2005 icon image carries no alpha, and its pure black is its transparent colour:
    /// drawn as an icon, its surround is transparent where the end-of-retail file's copy of the
    /// same icon is, and an item-type background tile, which has no black, stays opaque. The same
    /// image drawn as plain interface art keeps its black opaque.
    #[test]
    #[cfg_attr(
        not(feature = "retail-dats"),
        ignore = "reads the February 2005 and end-of-retail dats: --features retail-dats"
    )]
    fn a_february_2005_image_is_transparent_where_it_is_black() {
        let old = dereth_dat::testing::classic_dat_dir().unwrap_or_else(|| {
            panic!(
                "{}",
                dereth_dat::testing::classic_shortfall().unwrap_or_default()
            )
        });
        let hybrid =
            RetailDatStore::open_classic_with_modern(&old, &dereth_dat::testing::dat_dir())
                .expect("the February 2005 dats beside the end-of-retail ones");
        let later =
            RetailDatStore::open_dir(&dereth_dat::testing::dat_dir()).expect("the retail dats");
        let clear = |store: &RetailDatStore, id: u32| -> Vec<bool> {
            let d = TextureStore::new(store)
                .icon_data(DataId(id))
                .expect("the image decodes");
            assert_eq!(d.format, TextureFormat::Bgra8);
            d.levels[0]
                .as_chunks::<4>()
                .0
                .iter()
                .map(|p| p[3] == 0)
                .collect()
        };
        let icon = 0x0600_1375;
        let old_clear = clear(&hybrid, icon);
        assert!(old_clear.iter().filter(|&&c| c).count() > 100);
        assert_eq!(old_clear, clear(&later, icon));
        assert!(clear(&hybrid, 0x0600_11CB).iter().all(|&c| !c));
        let plain = TextureStore::new(&hybrid)
            .texture_data(DataId(icon))
            .expect("the image decodes");
        assert!(plain.levels[0]
            .as_chunks::<4>()
            .0
            .iter()
            .all(|p| p[3] == 0xFF));
    }

    /// A 256-colour image of the February 2005 files takes colour `i` of its 256-entry palette
    /// for index `i` (an Aluvian body part: indices 64-79, skin tones), not colour `i / 8`.
    #[test]
    #[cfg_attr(
        not(feature = "retail-dats"),
        ignore = "reads the February 2005 dats: --features retail-dats"
    )]
    fn a_february_2005_palette_image_takes_its_colours_by_index() {
        let store = dereth_dat::testing::open_classic_store_or_fail();
        let t = TextureStore::new(&store);
        let id = DataId(0x0500_0BB0);
        let (rsid, rs, bytes) = t.resolve(id).expect("the image resolves to itself");
        assert_eq!((rsid, rs.format), (id, 41));
        let indices = rs.payload(&bytes).expect("pixels").to_vec();
        let palette = t
            .lookup
            .palette(rs.default_palette_id.expect("a palette"))
            .expect("the palette decodes");
        assert_eq!(palette.colors_argb.len(), 256);
        let data = t.texture_data(id).expect("the image decodes");
        assert_eq!(data.format, dereth_primitives::TextureFormat::Bgra8);
        for (k, &i) in indices.iter().enumerate().take(64) {
            let [b, g, r, _] = palette.colors_argb[usize::from(i)].to_le_bytes();
            assert_eq!(&data.levels[0][k * 4..k * 4 + 3], &[b, g, r], "pixel {k}");
        }
    }

    #[test]
    #[cfg_attr(
        not(feature = "retail-dats"),
        ignore = "reads the retail dats: --features retail-dats"
    )]
    fn the_retail_jpeg_surface_the_character_screen_uses_decodes() {
        let store = dereth_dat::testing::open_store().unwrap_or_else(|| {
            panic!(
                "the retail dats are this test's oracle and they are not under {} -- \
                 set DERETH_TEST_DAT_DIR",
                dereth_dat::testing::dat_dir().display()
            )
        });
        let t = TextureStore::new(&store);
        let id = DataId(0x0600_7576);
        let (_, rs, _) = t.resolve(id).expect("the record decodes");
        assert_eq!(rs.format, 500, "PFID_CUSTOM_RAW_JPEG");
        assert_eq!(
            (rs.width, rs.height),
            (0, 0),
            "the header carries no extent"
        );

        let data = t.texture_data(id).expect("the JPEG decodes");
        assert!(
            data.width > 0 && data.height > 0,
            "{}x{}",
            data.width,
            data.height
        );
        assert_eq!(data.format, dereth_primitives::TextureFormat::Bgra8);
        assert_eq!(
            data.levels[0].len(),
            data.width as usize * data.height as usize * 4,
            "one BGRA texel per pixel"
        );
        // ...and it is a picture, not a flat fill: a background that decoded to one colour would
        // pass every size assertion above and still be wrong.
        let distinct: std::collections::BTreeSet<[u8; 4]> =
            data.levels[0].as_chunks::<4>().0.iter().copied().collect();
        assert!(
            distinct.len() > 16,
            "only {} distinct colours",
            distinct.len()
        );
    }

    #[test]
    #[cfg_attr(
        not(feature = "retail-dats"),
        ignore = "reads the retail dats: --features retail-dats"
    )]
    fn a_render_surface_id_resolves_to_itself_and_decodes_both_ways() {
        let store = dereth_dat::testing::open_store().unwrap_or_else(|| {
            panic!(
                "the retail dats are this test's oracle and they are not under {} -- \
                 set DERETH_TEST_DAT_DIR",
                dereth_dat::testing::dat_dir().display()
            )
        });
        let t = TextureStore::new(&store);
        let id = DataId(crate::gpu::FIRST_PIXEL_SURFACE);
        let (rsid, rs, _) = t.resolve(id).expect("resolves");
        assert_eq!(rsid, id, "a 0x06 id is already the pixel record");
        assert_eq!((rs.width, rs.height), (256, 256));

        // The GPU form keeps the DXT blocks, exactly as retail's texture creation does.
        let data = t.texture_data(id).expect("decodes for the GPU");
        assert_eq!(data.format, TextureFormat::Bc1);
        assert_eq!(
            data.levels[0].len(),
            256 * 256 / 2,
            "DXT1 is 4 bits per texel"
        );

        // The CPU form expands them, which is what the terrain compositor blends.
        let img = t.bgra8(id).expect("decodes to BGRA");
        assert_eq!((img.width, img.height), (256, 256));
        assert_eq!(img.pixels.len(), 256 * 256);
    }

    // Oracle: the retail region record 0x13000000. Every `TerrainDesc::tex_gid` in the shipped
    // TexMerge must resolve to real pixels -- if any did not, the terrain would composite the
    // client's `00 FF 00 00` debug colour and the landscape would be visibly green.
    #[test]
    #[cfg_attr(
        not(feature = "retail-dats"),
        ignore = "reads the retail dats: --features retail-dats"
    )]
    fn every_retail_terrain_texture_resolves_to_pixels() {
        let store = dereth_dat::testing::open_store().unwrap_or_else(|| {
            panic!(
                "the retail dats are this test's oracle and they are not under {} -- \
                 set DERETH_TEST_DAT_DIR",
                dereth_dat::testing::dat_dir().display()
            )
        });
        let region = dereth_world_data::landblock::load_region(&store).expect("the region decodes");
        let tm = region
            .land_surf
            .tex_merge
            .as_ref()
            .expect("retail ships a texture compositor");
        let t = TextureStore::new(&store);
        for d in &tm.terrain_desc {
            let img = t.bgra8(d.tex_gid).unwrap_or_else(|e| {
                panic!("terrain type {} texture {}: {e}", d.terrain_type, d.tex_gid)
            });
            assert!(img.width > 0 && img.height > 0);
        }
        for maps in [
            &tm.corner_terrain_maps,
            &tm.side_terrain_maps,
            &tm.road_maps,
        ] {
            for m in maps {
                let img = t
                    .bgra8(m.tex_gid)
                    .unwrap_or_else(|e| panic!("alpha map {} (code {}): {e}", m.tex_gid, m.code));
                assert!(img.width > 0 && img.height > 0);
            }
        }
    }
}
