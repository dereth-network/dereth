//! Where the interface's records come from: layouts, pictures, fonts and strings, by record id.
//!
//! The interface never opens a file. A host hands it an [`Assets`]; [`DatAssets`] is the one over
//! an install's data files, which the host opens.

use std::collections::HashMap;
use std::sync::{Arc, Mutex};

use auberean_assets::picture::{Picture, Texture};
use auberean_assets::strings::StringTable;
use auberean_assets::tables::IdMap;
use auberean_assets::ui::{Font, UiLayout};
use auberean_assets::Context;
use auberean_dat::DatFile;
use dereth_render_cpu::pixel_format::PixelFormatId;

/// A picture expanded to texels: `width * height` of blue, green, red, alpha.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Image {
    pub width: u32,
    pub height: u32,
    pub bgra: Vec<u8>,
    /// It holds alpha alone, its colors made up.
    pub alpha_only: bool,
}

impl Image {
    /// The texel at `(x, y)` as blue, green, red, alpha.
    #[must_use]
    pub fn texel(&self, x: u32, y: u32) -> [u8; 4] {
        let i = ((y * self.width + x) * 4) as usize;
        [
            self.bgra[i],
            self.bgra[i + 1],
            self.bgra[i + 2],
            self.bgra[i + 3],
        ]
    }
}

/// The records the interface draws from.
pub trait Assets {
    /// A layout record (`0x22...`).
    fn layout(&self, id: u32) -> Option<Arc<UiLayout>>;
    /// A picture (`0x41...`) or a picture list (`0x40...`, its largest level), expanded.
    fn image(&self, id: u32) -> Option<Arc<Image>>;
    /// A font (`0x42...`).
    fn font(&self, id: u32) -> Option<Arc<Font>>;
    /// The text a string table (`0x25...`) holds for a token.
    fn string(&self, table: u32, token: u32) -> Option<String>;
    /// The record an enumeration value names in one of the data's mappings (0 for none): the
    /// layout a screen is made from, by its layout number in the layouts' mapping.
    fn data_id_by_enum(&self, _value: u32, _mapping: u32) -> u32 {
        0
    }
    /// A string table's entry written as the client writes a string reference with no
    /// variables (its meta language rendered); by default the entry as stored.
    fn text(&self, table: u32, token: u32) -> Option<String> {
        self.string(table, token)
    }
    /// A string reference with its variables written as the client writes it; by default none.
    fn write(&self, _info: &auberean_text::StringInfo) -> Option<String> {
        None
    }
    /// A string reference's text only when it is valid (it, and every reference nested in it,
    /// names an entry its table holds, or is a literal): written as the client writes it, or its
    /// error message should the writing fail even so; by default none.
    fn write_valid(&self, _info: &auberean_text::StringInfo) -> Option<String> {
        None
    }
    /// A string reference as a text element shows it: written, or the client's error message
    /// when it cannot be; by default as [`Assets::write`] writes it, else nothing.
    fn element_text(&self, info: &auberean_text::StringInfo) -> Vec<u16> {
        self.write(info)
            .map(|s| s.encode_utf16().collect())
            .unwrap_or_default()
    }
    /// Ask for the string tables a reference given to a text element needs that the data lacks:
    /// the download's number while it waits, or `None` when the text can be written at once (by
    /// default, always).
    fn start_string_download(&self, _info: &auberean_text::StringInfo) -> Option<u32> {
        None
    }
    /// Whether a string download still waits for its tables.
    fn string_download_waiting(&self, _download: u32) -> bool {
        false
    }
    /// A string download dropped by its element before it was written.
    fn release_string_download(&self, _download: u32) {}
}

/// Expand one picture's texels, for the formats the interface's pictures are stored in.
#[must_use]
pub fn expand(p: &Picture) -> Option<Image> {
    let (w, h) = (p.width, p.height);
    let n = usize::try_from(w).ok()? * usize::try_from(h).ok()?;
    let b = &p.bytes;
    let x5 = |v: u16| -> u8 { u8::try_from((u32::from(v) * 255 + 15) / 31).unwrap_or(255) };
    let x6 = |v: u16| -> u8 { u8::try_from((u32::from(v) * 255 + 31) / 63).unwrap_or(255) };
    let x4 = |v: u16| -> u8 { u8::try_from(u32::from(v) * 17).unwrap_or(255) };
    let words = || {
        b.get(..n * 2).map(|s| {
            s.as_chunks::<2>()
                .0
                .iter()
                .map(|c| u16::from_le_bytes(*c))
                .collect::<Vec<_>>()
        })
    };
    let bgra: Vec<u8> = match PixelFormatId::from_raw(p.format) {
        PixelFormatId::A8R8G8B8 => b.get(..n * 4)?.to_vec(),
        PixelFormatId::X8R8G8B8 => b
            .get(..n * 4)?
            .as_chunks::<4>()
            .0
            .iter()
            .flat_map(|c| [c[0], c[1], c[2], 0xFF])
            .collect(),
        PixelFormatId::R8G8B8 => b
            .get(..n * 3)?
            .as_chunks::<3>()
            .0
            .iter()
            .flat_map(|c| [c[0], c[1], c[2], 0xFF])
            .collect(),
        PixelFormatId::R5G6B5 => words()?
            .into_iter()
            .flat_map(|v| [x5(v & 31), x6((v >> 5) & 63), x5(v >> 11), 0xFF])
            .collect(),
        PixelFormatId::A1R5G5B5 => words()?
            .into_iter()
            .flat_map(|v| {
                let a = if v >> 15 == 1 { 0xFF } else { 0 };
                [x5(v & 31), x5((v >> 5) & 31), x5((v >> 10) & 31), a]
            })
            .collect(),
        PixelFormatId::X1R5G5B5 => words()?
            .into_iter()
            .flat_map(|v| [x5(v & 31), x5((v >> 5) & 31), x5((v >> 10) & 31), 0xFF])
            .collect(),
        PixelFormatId::A4R4G4B4 => words()?
            .into_iter()
            .flat_map(|v| {
                [
                    x4(v & 15),
                    x4((v >> 4) & 15),
                    x4((v >> 8) & 15),
                    x4(v >> 12),
                ]
            })
            .collect(),
        PixelFormatId::A8 => b
            .get(..n)?
            .iter()
            .flat_map(|&a| [0xFF, 0xFF, 0xFF, a])
            .collect(),
        f @ (PixelFormatId::Dxt1
        | PixelFormatId::Dxt2
        | PixelFormatId::Dxt3
        | PixelFormatId::Dxt4
        | PixelFormatId::Dxt5) => dereth_render_cpu::dxt::decode(f, b, w, h).ok()?,
        PixelFormatId::CustomRawJpeg => {
            let j = dereth_render_cpu::jpeg::decode(b).ok()?;
            return Some(Image {
                width: j.width,
                height: j.height,
                bgra: j
                    .bgr
                    .as_chunks::<3>()
                    .0
                    .iter()
                    .flat_map(|c| [c[0], c[1], c[2], 0xFF])
                    .collect(),
                alpha_only: false,
            });
        }
        _ if p.format == 0x32 => b.get(..n)?.iter().flat_map(|&l| [l, l, l, 0xFF]).collect(),
        _ => return None,
    };
    (bgra.len() == n * 4).then_some(Image {
        width: w,
        height: h,
        bgra,
        alpha_only: matches!(PixelFormatId::from_raw(p.format), PixelFormatId::A8),
    })
}

/// An install's interface records, read from its data files and kept once read.
pub struct DatAssets {
    /// Searched in order: the language file first, then the portal and high-resolution files.
    dats: Vec<Arc<DatFile>>,
    /// The game's text writer over the same files (the portal file first).
    writer: Mutex<auberean_text::DatSource>,
    ctx: Context,
    layouts: Mutex<HashMap<u32, Option<Arc<UiLayout>>>>,
    images: Mutex<HashMap<u32, Option<Arc<Image>>>>,
    fonts: Mutex<HashMap<u32, Option<Arc<Font>>>>,
    tables: Mutex<HashMap<u32, Option<Arc<StringTable>>>>,
    maps: Mutex<HashMap<u32, Option<Arc<IdMap>>>>,
}

impl std::fmt::Debug for DatAssets {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("DatAssets")
            .field("dats", &self.dats.len())
            .finish_non_exhaustive()
    }
}

impl DatAssets {
    /// Records from these data files, the language file first.
    #[must_use]
    pub fn new(dats: Vec<DatFile>, ctx: Context) -> Self {
        let dats: Vec<Arc<DatFile>> = dats.into_iter().map(Arc::new).collect();
        // The text writer reads the enumeration maps through the first file's master map: the
        // portal file's.
        let mut ordered = dats.clone();
        ordered.sort_by_key(|d| d.header().master_map_id == 0);
        Self {
            writer: Mutex::new(auberean_text::DatSource::new(ordered)),
            dats,
            ctx,
            layouts: Mutex::default(),
            images: Mutex::default(),
            fonts: Mutex::default(),
            tables: Mutex::default(),
            maps: Mutex::default(),
        }
    }

    fn bytes(&self, id: u32) -> Option<Vec<u8>> {
        self.dats
            .iter()
            .find_map(|d| d.entry(id).and_then(|e| d.read_entry(e).ok()))
    }

    fn decoded<T: auberean_assets::Decode>(&self, id: u32) -> Option<T> {
        auberean_assets::decode(id, &self.bytes(id)?, &self.ctx).ok()
    }

    fn cached<T>(
        map: &Mutex<HashMap<u32, Option<Arc<T>>>>,
        id: u32,
        make: impl FnOnce() -> Option<T>,
    ) -> Option<Arc<T>> {
        if let Some(v) = map.lock().ok()?.get(&id) {
            return v.clone();
        }
        let v = make().map(Arc::new);
        map.lock().ok()?.insert(id, v.clone());
        v
    }
}

impl Assets for DatAssets {
    fn layout(&self, id: u32) -> Option<Arc<UiLayout>> {
        Self::cached(&self.layouts, id, || self.decoded(id))
    }

    fn image(&self, id: u32) -> Option<Arc<Image>> {
        Self::cached(&self.images, id, || {
            if id >> 24 == 0x40 {
                let list: Texture = self.decoded(id)?;
                return list
                    .levels
                    .iter()
                    .find_map(|l| self.decoded::<Picture>(l.0).and_then(|p| expand(&p)));
            }
            expand(&self.decoded::<Picture>(id)?)
        })
    }

    /// A font by its id, kept once read: only an id of a font's kind (the shared files' or the
    /// language file's) is read as one.
    // D2-10053, D2-10050
    fn font(&self, id: u32) -> Option<Arc<Font>> {
        auberean_assets::ui::font_kind(id)?;
        Self::cached(&self.fonts, id, || self.decoded(id))
    }

    fn string(&self, table: u32, token: u32) -> Option<String> {
        let t = Self::cached(&self.tables, table, || self.decoded(table))?;
        let e = t.entries.iter().find(|e| e.token == token)?;
        e.strings.iter().flatten().next().cloned()
    }

    fn text(&self, table: u32, token: u32) -> Option<String> {
        let mut w = self.writer.lock().ok()?;
        let (ok, units) = auberean_text::StringInfo::entry(table, token).inq(&mut *w, false);
        ok.then(|| String::from_utf16_lossy(&units))
    }

    fn write(&self, info: &auberean_text::StringInfo) -> Option<String> {
        let mut w = self.writer.lock().ok()?;
        let (ok, units) = info.clone().inq(&mut *w, false);
        ok.then(|| String::from_utf16_lossy(&units))
    }

    fn write_valid(&self, info: &auberean_text::StringInfo) -> Option<String> {
        let mut w = self.writer.lock().ok()?;
        if !info.is_valid(&mut *w, true) {
            return None;
        }
        Some(String::from_utf16_lossy(
            &info.clone().get_string(&mut *w, false),
        ))
    }

    fn element_text(&self, info: &auberean_text::StringInfo) -> Vec<u16> {
        self.writer
            .lock()
            .map(|mut w| info.clone().get_string(&mut *w, false))
            .unwrap_or_default()
    }

    /// Each file's own id map names the mapping's map (its game half first, then its tools'
    /// half); the first file whose maps name the value gives the record.
    fn data_id_by_enum(&self, value: u32, mapping: u32) -> u32 {
        let lookup = |m: &IdMap, v: u32| {
            m.ids
                .iter()
                .chain(m.tool_ids.iter())
                .find(|(k, _)| *k == v)
                .map_or(0, |(_, d)| d.0)
        };
        for d in &self.dats {
            let master_id = d.header().master_map_id;
            if master_id == 0 {
                continue;
            }
            let map = |id: u32| Self::cached(&self.maps, id, || self.decoded::<IdMap>(id));
            let Some(master) = map(master_id) else {
                continue;
            };
            let sub = lookup(&master, mapping);
            if sub == 0 {
                continue;
            }
            let found = map(sub).map_or(0, |m| lookup(&m, value));
            if found != 0 {
                return found;
            }
        }
        0
    }
}
