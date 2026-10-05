//! Packs pictures into a client layer: the container of Dereth's own records the client reads
//! over the portal files.
//!
//! The container is [`dereth_dat::client_layer`]'s.
//!
//! **Depends on** `dereth-dat` (the container and its writer), `dereth-primitives` (data ids) and
//! `png`. **Used by** its own command line, `dereth-pack`, and by nothing else yet: it is meant to
//! grow into the editor of Dereth's own records.
//!
//! **Must never** read or write a player's data files: it reads pictures and writes one container.
//!
//! A pack is described by a manifest, a text file of tab- or space-separated lines; `#` starts a
//! comment:
//!
//! ```text
//! over        portal.dat            # the files it lies over: portal.dat or client_portal.dat
//! out         layer.dat             # the container written, beside the manifest
//! background  0D1115                # what a picture's transparent pixels are laid on
//! 0x0600708F  rgb  cloak.png        # a record: its id, its format, its picture
//! 0x06006BEF  rgb  sigil.png  000000  # ... with a background of its own
//! ```
//!
//! The formats are `rgb`, the image record of the files from before Throne of Destiny (its id, a
//! width and a height, then three bytes a pixel in red, green, blue order), and `surface`, the
//! later files' image (`R8G8B8`, three bytes a pixel in blue, green, red order). Each must match
//! the files the pack lies over. A picture with transparency is flattened against its background,
//! and [`build`] says which were. The same manifest and pictures always give the same container.

use std::collections::BTreeMap;
use std::fmt;
use std::path::{Path, PathBuf};

use dereth_dat::ContainerEra;
use dereth_primitives::DataId;

/// What a pack could not do.
#[derive(Debug)]
pub enum PackError {
    /// A manifest line that does not read, with its line number.
    Manifest { line: usize, why: String },
    /// A picture that does not read.
    Picture { path: PathBuf, why: String },
    /// A file that could not be read or written.
    Io { path: PathBuf, why: String },
    /// The container could not be written.
    Container(String),
}

impl fmt::Display for PackError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Manifest { line, why } => write!(f, "manifest line {line}: {why}"),
            Self::Picture { path, why } => write!(f, "{}: {why}", path.display()),
            Self::Io { path, why } => write!(f, "{}: {why}", path.display()),
            Self::Container(why) => write!(f, "the container: {why}"),
        }
    }
}

impl std::error::Error for PackError {}

/// A record's format.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Format {
    /// The image record of the files from before Throne of Destiny.
    Rgb,
    /// The later files' image record, in `R8G8B8`.
    Surface,
}

impl Format {
    /// The files a record of this format belongs in.
    #[must_use]
    pub const fn era(self) -> ContainerEra {
        match self {
            Self::Rgb => ContainerEra::PreTod,
            Self::Surface => ContainerEra::Tod,
        }
    }
}

/// `R8G8B8`, the later files' pixel-format id for three bytes a pixel.
const PFID_R8G8B8: u32 = 20;
/// The category the later files' interface images carry.
const INTERFACE_CATEGORY: u32 = 6;

/// A picture as three bytes a pixel, red, green, blue, top row first.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Picture {
    pub width: u32,
    pub height: u32,
    pub rgb: Vec<u8>,
    /// Whether any pixel was transparent and was laid on the background.
    pub flattened: bool,
}

/// Read a PNG of any colour type and depth, laying transparent pixels on `background`.
///
/// # Errors
/// [`PackError::Picture`] for bytes that are not a PNG (`path` names it).
pub fn read_png(path: &Path, bytes: &[u8], background: [u8; 3]) -> Result<Picture, PackError> {
    let bad = |why: String| PackError::Picture {
        path: path.to_path_buf(),
        why,
    };
    let mut decoder = png::Decoder::new(bytes);
    decoder.set_transformations(png::Transformations::normalize_to_color8());
    let mut reader = decoder.read_info().map_err(|e| bad(e.to_string()))?;
    let mut buf = vec![0; reader.output_buffer_size()];
    let info = reader
        .next_frame(&mut buf)
        .map_err(|e| bad(e.to_string()))?;
    let (width, height) = (info.width, info.height);
    let channels = match info.color_type {
        png::ColorType::Grayscale => 1,
        png::ColorType::GrayscaleAlpha => 2,
        png::ColorType::Rgb => 3,
        png::ColorType::Rgba => 4,
        png::ColorType::Indexed => return Err(bad("an indexed picture was not expanded".into())),
    };
    let mut rgb = Vec::with_capacity(width as usize * height as usize * 3);
    let mut flattened = false;
    for row in buf[..info.buffer_size()].chunks_exact(info.line_size) {
        for px in row[..width as usize * channels].chunks_exact(channels) {
            let (colour, alpha) = match channels {
                1 => ([px[0]; 3], 255),
                2 => ([px[0]; 3], px[1]),
                3 => ([px[0], px[1], px[2]], 255),
                _ => ([px[0], px[1], px[2]], px[3]),
            };
            if alpha < 255 {
                flattened = true;
            }
            for (c, b) in colour.iter().zip(background) {
                let a = u32::from(alpha);
                let v = (u32::from(*c) * a + u32::from(b) * (255 - a) + 127) / 255;
                rgb.push(u8::try_from(v).unwrap_or(u8::MAX));
            }
        }
    }
    Ok(Picture {
        width,
        height,
        rgb,
        flattened,
    })
}

/// The record `id` holding `picture` in `format`.
#[must_use]
pub fn encode(id: DataId, picture: &Picture, format: Format) -> Vec<u8> {
    let mut out = Vec::with_capacity(24 + picture.rgb.len());
    out.extend_from_slice(&id.raw().to_le_bytes());
    match format {
        Format::Rgb => {
            out.extend_from_slice(&picture.width.to_le_bytes());
            out.extend_from_slice(&picture.height.to_le_bytes());
            out.extend_from_slice(&picture.rgb);
        }
        Format::Surface => {
            let size = u32::try_from(picture.rgb.len()).unwrap_or(u32::MAX);
            for v in [
                INTERFACE_CATEGORY,
                picture.width,
                picture.height,
                PFID_R8G8B8,
                size,
            ] {
                out.extend_from_slice(&v.to_le_bytes());
            }
            for px in picture.rgb.as_chunks::<3>().0 {
                out.extend_from_slice(&[px[2], px[1], px[0]]);
            }
        }
    }
    out
}

/// A record [`encode`] made, read back: its id and its picture. `None` for bytes that are not a
/// whole record of `format`.
#[must_use]
pub fn decode(record: &[u8], format: Format) -> Option<(DataId, Picture)> {
    let word = |i: usize| -> Option<u32> {
        Some(u32::from_le_bytes(record.get(i..i + 4)?.try_into().ok()?))
    };
    let id = DataId(word(0)?);
    let (width, height, header, bgr) = match format {
        Format::Rgb => (word(4)?, word(8)?, 12, false),
        Format::Surface => {
            if word(16)? != PFID_R8G8B8 || word(20)? as usize + 24 != record.len() {
                return None;
            }
            (word(8)?, word(12)?, 24, true)
        }
    };
    let pixels = record.get(header..)?;
    if pixels.len() != width as usize * height as usize * 3 {
        return None;
    }
    let rgb = if bgr {
        pixels
            .as_chunks::<3>()
            .0
            .iter()
            .flat_map(|p| [p[2], p[1], p[0]])
            .collect()
    } else {
        pixels.to_vec()
    };
    Some((
        id,
        Picture {
            width,
            height,
            rgb,
            flattened: false,
        },
    ))
}

/// One record of a manifest.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Entry {
    pub id: DataId,
    pub format: Format,
    /// The picture, as the manifest names it (relative to the manifest's folder).
    pub picture: PathBuf,
    /// Its own background, when it names one.
    pub background: Option<[u8; 3]>,
}

/// A pack's manifest, read.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Manifest {
    /// The files the pack lies over.
    pub over: ContainerEra,
    /// The container it writes (relative to the manifest's folder).
    pub out: PathBuf,
    /// What transparent pixels are laid on, unless an entry names its own.
    pub background: [u8; 3],
    /// The records, ascending by id.
    pub entries: Vec<Entry>,
}

fn colour(s: &str) -> Option<[u8; 3]> {
    let v = u32::from_str_radix(s.trim_start_matches('#'), 16).ok()?;
    (s.trim_start_matches('#').len() == 6).then(|| {
        let [_, r, g, b] = v.to_be_bytes();
        [r, g, b]
    })
}

impl Manifest {
    /// Read a manifest's text.
    ///
    /// # Errors
    /// [`PackError::Manifest`] for a line that does not read, a record whose format is not the
    /// files' it lies over, an id given twice, or no `over` or `out` line.
    pub fn parse(text: &str) -> Result<Self, PackError> {
        let mut over = None;
        let mut out = None;
        let mut background = [0; 3];
        let mut entries: BTreeMap<DataId, Entry> = BTreeMap::new();
        for (n, raw) in text.lines().enumerate() {
            let line = n + 1;
            let bad = |why: &str| PackError::Manifest {
                line,
                why: why.to_owned(),
            };
            let words: Vec<&str> = raw
                .split('#')
                .next()
                .unwrap_or("")
                .split_whitespace()
                .collect();
            match words.as_slice() {
                [] => {}
                ["over", name] => {
                    over = Some(if name.eq_ignore_ascii_case("portal.dat") {
                        ContainerEra::PreTod
                    } else if name.eq_ignore_ascii_case("client_portal.dat") {
                        ContainerEra::Tod
                    } else {
                        return Err(bad("over names portal.dat or client_portal.dat"));
                    });
                }
                ["out", path] => out = Some(PathBuf::from(path)),
                ["background", c] => {
                    background = colour(c).ok_or_else(|| bad("a background is RRGGBB"))?;
                }
                [id, format, picture, rest @ ..] if rest.len() <= 1 => {
                    let id = id
                        .strip_prefix("0x")
                        .or_else(|| id.strip_prefix("0X"))
                        .and_then(|h| u32::from_str_radix(h, 16).ok())
                        .ok_or_else(|| bad("a record's id is 0x and eight hex digits"))?;
                    if id >> 24 != 6 {
                        return Err(bad("a picture's id is an image id (0x06......)"));
                    }
                    let format = match *format {
                        "rgb" => Format::Rgb,
                        "surface" => Format::Surface,
                        _ => return Err(bad("a record's format is rgb or surface")),
                    };
                    let background = match rest.first() {
                        Some(c) => Some(colour(c).ok_or_else(|| bad("a background is RRGGBB"))?),
                        None => None,
                    };
                    let entry = Entry {
                        id: DataId(id),
                        format,
                        picture: PathBuf::from(picture),
                        background,
                    };
                    if entries.insert(DataId(id), entry).is_some() {
                        return Err(bad("this id is given twice"));
                    }
                }
                _ => return Err(bad("not a line a manifest has")),
            }
        }
        let missing = |what: &str| PackError::Manifest {
            line: 0,
            why: format!("the manifest has no {what} line"),
        };
        let over = over.ok_or_else(|| missing("over"))?;
        if let Some(e) = entries.values().find(|e| e.format.era() != over) {
            return Err(PackError::Manifest {
                line: 0,
                why: format!(
                    "{:#010X} is in the other files' format from the files the pack lies over",
                    e.id.raw()
                ),
            });
        }
        Ok(Self {
            over,
            out: out.ok_or_else(|| missing("out"))?,
            background,
            entries: entries.into_values().collect(),
        })
    }
}

/// A pack's records by id, and the ids whose pictures were flattened.
pub type Built = (BTreeMap<DataId, Vec<u8>>, Vec<DataId>);

/// The records a manifest makes, with the ids whose pictures were flattened. `dir` is the
/// manifest's folder.
///
/// # Errors
/// A picture that does not read.
pub fn build(manifest: &Manifest, dir: &Path) -> Result<Built, PackError> {
    let mut records = BTreeMap::new();
    let mut flattened = Vec::new();
    for e in &manifest.entries {
        let path = dir.join(&e.picture);
        let bytes = std::fs::read(&path).map_err(|err| PackError::Io {
            path: path.clone(),
            why: err.to_string(),
        })?;
        let picture = read_png(&path, &bytes, e.background.unwrap_or(manifest.background))?;
        if picture.flattened {
            flattened.push(e.id);
        }
        records.insert(e.id, encode(e.id, &picture, e.format));
    }
    Ok((records, flattened))
}

/// What a pack wrote.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Packed {
    /// The container.
    pub out: PathBuf,
    /// Every record, ascending.
    pub ids: Vec<DataId>,
    /// The ones whose pictures had transparency, laid on their background.
    pub flattened: Vec<DataId>,
}

/// Pack the manifest at `manifest_path`, writing the container it names to `out` (its own `out`
/// when `None`).
///
/// # Errors
/// The manifest or a picture does not read, or the container cannot be written.
pub fn pack(manifest_path: &Path, out: Option<&Path>) -> Result<Packed, PackError> {
    let text = std::fs::read_to_string(manifest_path).map_err(|e| PackError::Io {
        path: manifest_path.to_path_buf(),
        why: e.to_string(),
    })?;
    let manifest = Manifest::parse(&text)?;
    let dir = manifest_path.parent().unwrap_or(Path::new("."));
    let (records, flattened) = build(&manifest, dir)?;
    let out = out.map_or_else(|| dir.join(&manifest.out), Path::to_path_buf);
    dereth_dat::client_layer::write(&out, manifest.over, &records)
        .map_err(|e| PackError::Container(e.to_string()))?;
    Ok(Packed {
        out,
        ids: records.into_keys().collect(),
        flattened,
    })
}

/// Whether the container the manifest at `manifest_path` names is the one it makes now: packed
/// into a temporary file and compared byte for byte.
///
/// # Errors
/// As [`pack`], and the container named cannot be read.
pub fn is_current(manifest_path: &Path) -> Result<bool, PackError> {
    let text = std::fs::read_to_string(manifest_path).map_err(|e| PackError::Io {
        path: manifest_path.to_path_buf(),
        why: e.to_string(),
    })?;
    let manifest = Manifest::parse(&text)?;
    let dir = manifest_path.parent().unwrap_or(Path::new("."));
    let committed = dir.join(&manifest.out);
    let fresh = std::env::temp_dir().join(format!(
        "dereth-pack-{}-{}.dat",
        std::process::id(),
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map_or(0, |d| d.as_nanos())
    ));
    let packed = pack(manifest_path, Some(&fresh));
    let made = packed.and_then(|_| {
        std::fs::read(&fresh).map_err(|e| PackError::Io {
            path: fresh.clone(),
            why: e.to_string(),
        })
    });
    let _ = std::fs::remove_file(&fresh);
    let made = made?;
    let have = std::fs::read(&committed).map_err(|e| PackError::Io {
        path: committed.clone(),
        why: e.to_string(),
    })?;
    Ok(made == have)
}

#[cfg(test)]
mod tests;
