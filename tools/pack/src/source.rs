//! A pack: a folder of text and pictures, read.
//!
//! ```text
//! mypack/
//!   pack.toml                    name, version, authors, license, description
//!   art/*.png                    pictures for new textures
//!   records/*.toml               new records: render surfaces, surface textures, surfaces
//!   edits/landblocks/XXYY.toml   operations on one landblock: statics and terrain vertices
//!   server/sql/**/*.sql          ACE's SQL, with {{kind:symbol}} where an id goes
//! ```
//!
//! Only `pack.toml` is required. Every file is read in sorted order, so a pack reads the same on
//! every machine. A record or an operation that does not read stops the read with its file and
//! line.
//!
//! **New records** (`records/*.toml`) each carry `name` (a symbol: a new id, numbered per world)
//! or `replace` (an id the base holds, written over):
//!
//! ```toml
//! [[render-surface]]
//! name = "stripes"
//! picture = "art/stripes.png"   # a PNG, its sides powers of two
//! format = "a8r8g8b8"           # or "r8g8b8"
//! category = 4                  # the header's category, as the world's textures carry
//!
//! [[surface-texture]]
//! name = "stripes"
//! levels = ["stripes"]          # render surfaces, largest first: symbols or ids
//!
//! [[surface]]
//! name = "stripes"
//! type = 2                      # the surface type bits; 2 or 4 lay a texture
//! texture = "stripes"           # a surface texture: a symbol or an id
//! translucency = 0.0
//! luminosity = 0.0
//! diffuse = 1.0
//! ```
//!
//! **Operations** (`edits/landblocks/XXYY.toml`, one file per landblock):
//!
//! ```toml
//! [[static]]                     # place a model
//! name = "stone-1"               # optional: a symbol another operation can move it by
//! model = 0x0200040B             # a GfxObj or a Setup
//! at = [186.0, 30.0, "ground"]   # landblock meters; z a height, or "ground"
//! heading = 90.0                 # degrees clockwise from north (or rotation = [w, x, y, z])
//!
//! [[move-static]]                # a base static by its index in the base record, or a symbol
//! index = 2                      # or: static = "stone-1"
//! at = [190.0, 30.0, "ground"]
//!
//! [[remove-static]]
//! index = 3
//! model = 0x02000BCD             # optional: what the base holds there, checked
//!
//! [[vertex]]                     # one vertex, or every vertex of the rectangle at..=to
//! at = [4, 3]
//! to = [5, 4]
//! height = 80                    # the height table's index, 0 to 255
//! terrain = 1                    # 0 to 31
//! scene = 0                      # 0 to 31
//! road = 0                       # 0 to 3
//! ```

use std::path::{Path, PathBuf};

use dereth_primitives::DataId;
use serde::Deserialize;
use toml::Spanned;

use crate::error::{Error, Loc};
use crate::symbol::{self, Kind};

/// A pack, read.
#[derive(Clone, Debug)]
pub struct Pack {
    /// The pack's folder.
    pub dir: PathBuf,
    pub name: String,
    pub version: String,
    pub authors: Vec<String>,
    pub license: Option<String>,
    pub description: Option<String>,
    /// The new records, in file order.
    pub records: Vec<Located<NewRecord>>,
    /// The operations, one entry per landblock file, ascending by landblock.
    pub edits: Vec<LandblockEdits>,
    /// The SQL files, in path order.
    pub sql: Vec<SqlFile>,
}

/// Something read from a source, and where.
#[derive(Clone, Debug)]
pub struct Located<T> {
    pub at: Loc,
    pub item: T,
}

/// Where a new record goes.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Target {
    /// A new id, numbered for the world: the record's full symbol.
    New(String),
    /// An id the base holds, written over.
    Replace(DataId),
}

/// A record a record names: an id, or a symbol (in full).
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum RecordRef {
    Id(DataId),
    Symbol(String),
}

/// A picture record's pixel format.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PixelFormat {
    /// Four bytes a pixel, blue, green, red, alpha (`D3DFMT_A8R8G8B8`).
    A8R8G8B8,
    /// Three bytes a pixel, blue, green, red (`D3DFMT_R8G8B8`).
    R8G8B8,
}

impl PixelFormat {
    /// The format's id in the record.
    #[must_use]
    pub const fn id(self) -> u32 {
        match self {
            Self::A8R8G8B8 => 21,
            Self::R8G8B8 => 20,
        }
    }
}

/// A new record, as its source describes it.
#[derive(Clone, Debug, PartialEq)]
pub enum NewRecord {
    RenderSurface {
        target: Target,
        /// The picture's path.
        picture: PathBuf,
        format: PixelFormat,
        category: u32,
    },
    SurfaceTexture {
        target: Target,
        levels: Vec<RecordRef>,
        category: u32,
    },
    Surface {
        target: Target,
        surface_type: u32,
        texture: Option<RecordRef>,
        palette: Option<RecordRef>,
        color: Option<u32>,
        translucency: f32,
        luminosity: f32,
        diffuse: f32,
    },
}

impl NewRecord {
    #[must_use]
    pub fn kind(&self) -> Kind {
        match self {
            Self::RenderSurface { .. } => Kind::RenderSurface,
            Self::SurfaceTexture { .. } => Kind::SurfaceTexture,
            Self::Surface { .. } => Kind::Surface,
        }
    }

    #[must_use]
    pub fn target(&self) -> &Target {
        match self {
            Self::RenderSurface { target, .. }
            | Self::SurfaceTexture { target, .. }
            | Self::Surface { target, .. } => target,
        }
    }

    /// The records this one names, with the kind each must be.
    #[must_use]
    pub fn references(&self) -> Vec<(Kind, &RecordRef)> {
        match self {
            Self::RenderSurface { .. } => Vec::new(),
            Self::SurfaceTexture { levels, .. } => {
                levels.iter().map(|l| (Kind::RenderSurface, l)).collect()
            }
            Self::Surface { texture, .. } => {
                texture.iter().map(|t| (Kind::SurfaceTexture, t)).collect()
            }
        }
    }
}

/// A static's height: given, or the ground's under it.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Height {
    At(f32),
    Ground,
}

/// Where a static stands, in landblock meters.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Place {
    pub x: f32,
    pub y: f32,
    pub z: Height,
}

/// Which way a static faces.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Rotation {
    /// Degrees clockwise from north, about up.
    Heading(f32),
    /// The quaternion itself, `w, x, y, z`.
    Quat([f32; 4]),
}

/// A static an operation names: the base's, by its index in the base record, or a pack's, by
/// its symbol (in full).
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Which {
    Index(usize),
    Symbol(String),
}

/// Place a model.
#[derive(Clone, Debug, PartialEq)]
pub struct AddStatic {
    /// Its symbol, in full.
    pub name: Option<String>,
    pub model: DataId,
    pub at: Place,
    pub rotation: Rotation,
}

/// Move a static, turn it, or both.
#[derive(Clone, Debug, PartialEq)]
pub struct MoveStatic {
    pub which: Which,
    pub at: Option<Place>,
    pub rotation: Option<Rotation>,
}

/// Take a static away.
#[derive(Clone, Debug, PartialEq)]
pub struct RemoveStatic {
    pub which: Which,
    /// What the base must hold there.
    pub model: Option<DataId>,
}

/// Set fields of every vertex of a rectangle, landblock-local, `from..=to`.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct VertexEdit {
    pub from: (u8, u8),
    pub to: (u8, u8),
    pub height: Option<u8>,
    pub terrain: Option<u16>,
    pub scene: Option<u16>,
    pub road: Option<u16>,
}

/// The operations of one landblock's file.
#[derive(Clone, Debug)]
pub struct LandblockEdits {
    /// The landblock, `0xXXYY`.
    pub landblock: u16,
    pub file: PathBuf,
    pub statics: Vec<Located<AddStatic>>,
    pub moves: Vec<Located<MoveStatic>>,
    pub removes: Vec<Located<RemoveStatic>>,
    pub vertices: Vec<Located<VertexEdit>>,
}

/// One SQL file.
#[derive(Clone, Debug)]
pub struct SqlFile {
    /// Its path under `server/sql`.
    pub rel: PathBuf,
    pub path: PathBuf,
    pub text: String,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct PackFile {
    name: String,
    version: String,
    #[serde(default)]
    authors: Vec<String>,
    license: Option<String>,
    description: Option<String>,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct RecordsFile {
    #[serde(default, rename = "render-surface")]
    render_surface: Vec<Spanned<RenderSurfaceRow>>,
    #[serde(default, rename = "surface-texture")]
    surface_texture: Vec<Spanned<SurfaceTextureRow>>,
    #[serde(default)]
    surface: Vec<Spanned<SurfaceRow>>,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct RenderSurfaceRow {
    name: Option<String>,
    replace: Option<i64>,
    picture: String,
    format: Option<String>,
    category: Option<u32>,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct SurfaceTextureRow {
    name: Option<String>,
    replace: Option<i64>,
    levels: Vec<toml::Value>,
    category: Option<u32>,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct SurfaceRow {
    name: Option<String>,
    replace: Option<i64>,
    #[serde(rename = "type")]
    surface_type: Option<u32>,
    texture: Option<toml::Value>,
    palette: Option<toml::Value>,
    color: Option<i64>,
    translucency: Option<f32>,
    luminosity: Option<f32>,
    diffuse: Option<f32>,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct EditsFile {
    #[serde(default, rename = "static")]
    statics: Vec<Spanned<StaticRow>>,
    #[serde(default, rename = "move-static")]
    moves: Vec<Spanned<MoveRow>>,
    #[serde(default, rename = "remove-static")]
    removes: Vec<Spanned<RemoveRow>>,
    #[serde(default)]
    vertex: Vec<Spanned<VertexRow>>,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct StaticRow {
    name: Option<String>,
    model: i64,
    at: Vec<toml::Value>,
    heading: Option<f32>,
    rotation: Option<Vec<f32>>,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct MoveRow {
    index: Option<usize>,
    #[serde(rename = "static")]
    symbol: Option<String>,
    at: Option<Vec<toml::Value>>,
    heading: Option<f32>,
    rotation: Option<Vec<f32>>,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct RemoveRow {
    index: Option<usize>,
    #[serde(rename = "static")]
    symbol: Option<String>,
    model: Option<i64>,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct VertexRow {
    at: Vec<i64>,
    to: Option<Vec<i64>>,
    height: Option<i64>,
    terrain: Option<i64>,
    scene: Option<i64>,
    road: Option<i64>,
}

fn read_text(path: &Path) -> Result<String, Error> {
    std::fs::read_to_string(path).map_err(|e| Error::io(path, e))
}

fn parse_toml<T: serde::de::DeserializeOwned>(path: &Path, text: &str) -> Result<T, Error> {
    toml::from_str(text).map_err(|e| {
        let at = e
            .span()
            .map_or_else(|| Loc::new(path, 0), |s| Loc::at(path, text, s.start));
        Error::source(&at, e.message().to_owned())
    })
}

/// The files directly in `dir` whose extension is `ext`, sorted by name; none when `dir` is not
/// there.
fn files_in(dir: &Path, ext: &str) -> Result<Vec<PathBuf>, Error> {
    if !dir.is_dir() {
        return Ok(Vec::new());
    }
    let mut out = Vec::new();
    for e in std::fs::read_dir(dir).map_err(|e| Error::io(dir, e))? {
        let p = e.map_err(|e| Error::io(dir, e))?.path();
        if p.is_file()
            && p.extension()
                .and_then(|x| x.to_str())
                .is_some_and(|x| x.eq_ignore_ascii_case(ext))
        {
            out.push(p);
        }
    }
    out.sort();
    Ok(out)
}

/// Every `.sql` file under `dir`, by its path under it compared component by component.
fn sql_files(dir: &Path) -> Result<Vec<(PathBuf, PathBuf)>, Error> {
    if !dir.is_dir() {
        return Ok(Vec::new());
    }
    let mut out: Vec<(Vec<String>, PathBuf, PathBuf)> = Vec::new();
    let mut stack = vec![dir.to_path_buf()];
    while let Some(d) = stack.pop() {
        for e in std::fs::read_dir(&d).map_err(|e| Error::io(&d, e))? {
            let p = e.map_err(|e| Error::io(&d, e))?.path();
            if p.is_dir() {
                stack.push(p);
            } else if p
                .extension()
                .and_then(|x| x.to_str())
                .is_some_and(|x| x.eq_ignore_ascii_case("sql"))
            {
                let rel = p.strip_prefix(dir).unwrap_or(&p).to_path_buf();
                let key = rel
                    .components()
                    .map(|c| c.as_os_str().to_string_lossy().into_owned())
                    .collect();
                out.push((key, rel, p));
            }
        }
    }
    out.sort();
    Ok(out.into_iter().map(|(_, rel, p)| (rel, p)).collect())
}

/// An id written as a TOML integer, which holds the whole `u32` range.
fn id_of(at: &Loc, v: i64, what: &str) -> Result<DataId, Error> {
    u32::try_from(v)
        .map(DataId)
        .map_err(|_| Error::source(at, format!("{what} {v:#x} is not a data id")))
}

fn record_ref(at: &Loc, pack: &str, v: &toml::Value) -> Result<RecordRef, Error> {
    match v {
        toml::Value::Integer(i) => Ok(RecordRef::Id(id_of(at, *i, "the record")?)),
        toml::Value::String(s) => symbol::qualify(pack, s)
            .map(RecordRef::Symbol)
            .map_err(|why| Error::source(at, why)),
        _ => Err(Error::source(at, "a record is named by a symbol or an id")),
    }
}

fn target(at: &Loc, pack: &str, name: Option<&str>, replace: Option<i64>) -> Result<Target, Error> {
    match (name, replace) {
        (Some(n), None) => symbol::qualify(pack, n)
            .map(Target::New)
            .map_err(|why| Error::source(at, why)),
        (None, Some(id)) => Ok(Target::Replace(id_of(at, id, "replace")?)),
        _ => Err(Error::source(
            at,
            "a new record has a name or a replace, and not both",
        )),
    }
}

fn read_records(pack: &Pack, path: &Path) -> Result<Vec<Located<NewRecord>>, Error> {
    let text = read_text(path)?;
    let file: RecordsFile = parse_toml(path, &text)?;
    let mut out: Vec<(usize, Located<NewRecord>)> = Vec::new();
    let loc = |start: usize| Loc::at(path, &text, start);
    for row in &file.render_surface {
        let at = loc(row.span().start);
        let r = row.get_ref();
        let format = match r.format.as_deref() {
            None | Some("a8r8g8b8") => PixelFormat::A8R8G8B8,
            Some("r8g8b8") => PixelFormat::R8G8B8,
            Some(other) => {
                return Err(Error::source(
                    &at,
                    format!("{other:?} is not a format: a8r8g8b8 or r8g8b8"),
                ))
            }
        };
        let item = NewRecord::RenderSurface {
            target: target(&at, &pack.name, r.name.as_deref(), r.replace)?,
            picture: pack.dir.join(&r.picture),
            format,
            category: r.category.unwrap_or(4),
        };
        out.push((row.span().start, Located { at, item }));
    }
    for row in &file.surface_texture {
        let at = loc(row.span().start);
        let r = row.get_ref();
        if r.levels.is_empty() {
            return Err(Error::source(
                &at,
                "a surface texture has at least one level",
            ));
        }
        let levels = r
            .levels
            .iter()
            .map(|v| record_ref(&at, &pack.name, v))
            .collect::<Result<_, _>>()?;
        let item = NewRecord::SurfaceTexture {
            target: target(&at, &pack.name, r.name.as_deref(), r.replace)?,
            levels,
            category: r.category.unwrap_or(0),
        };
        out.push((row.span().start, Located { at, item }));
    }
    for row in &file.surface {
        let at = loc(row.span().start);
        let r = row.get_ref();
        let surface_type = r.surface_type.unwrap_or(2);
        let textured = surface_type & 6 != 0;
        let texture = r
            .texture
            .as_ref()
            .map(|v| record_ref(&at, &pack.name, v))
            .transpose()?;
        let palette = r
            .palette
            .as_ref()
            .map(|v| record_ref(&at, &pack.name, v))
            .transpose()?;
        if textured != texture.is_some() {
            return Err(Error::source(
                &at,
                "a surface whose type lays a texture (bits 2 or 4) names its texture, and one that \
                 does not names none",
            ));
        }
        if !textured && palette.is_some() {
            return Err(Error::source(&at, "a colour surface has no palette"));
        }
        let color = r
            .color
            .map(|c| u32::try_from(c).map_err(|_| Error::source(&at, "a colour is 0xAARRGGBB")))
            .transpose()?;
        let item = NewRecord::Surface {
            target: target(&at, &pack.name, r.name.as_deref(), r.replace)?,
            surface_type,
            texture,
            palette,
            color: if textured {
                None
            } else {
                Some(color.unwrap_or(0))
            },
            translucency: r.translucency.unwrap_or(0.0),
            luminosity: r.luminosity.unwrap_or(0.0),
            diffuse: r.diffuse.unwrap_or(1.0),
        };
        out.push((row.span().start, Located { at, item }));
    }
    out.sort_by_key(|(start, _)| *start);
    Ok(out.into_iter().map(|(_, r)| r).collect())
}

fn number(at: &Loc, v: &toml::Value, what: &str) -> Result<f32, Error> {
    match v {
        #[allow(clippy::cast_precision_loss)]
        toml::Value::Integer(i) => Ok(*i as f32),
        #[allow(clippy::cast_possible_truncation)]
        toml::Value::Float(f) => Ok(*f as f32),
        _ => Err(Error::source(at, format!("{what} is a number"))),
    }
}

fn place(at: &Loc, v: &[toml::Value]) -> Result<Place, Error> {
    let [x, y, z] = v else {
        return Err(Error::source(
            at,
            "at is [x, y, z], z a height or \"ground\"",
        ));
    };
    let (x, y) = (number(at, x, "x")?, number(at, y, "y")?);
    if !(0.0..192.0).contains(&x) || !(0.0..192.0).contains(&y) {
        return Err(Error::source(
            at,
            "a static stands inside its landblock: x and y from 0 to under 192",
        ));
    }
    let z = match z {
        toml::Value::String(s) if s == "ground" => Height::Ground,
        other => Height::At(number(at, other, "z")?),
    };
    Ok(Place { x, y, z })
}

fn rotation(
    at: &Loc,
    heading: Option<f32>,
    quat: Option<&[f32]>,
) -> Result<Option<Rotation>, Error> {
    match (heading, quat) {
        (None, None) => Ok(None),
        (Some(h), None) => Ok(Some(Rotation::Heading(h))),
        (None, Some(&[w, x, y, z])) => {
            let m = w * w + x * x + y * y + z * z;
            if (m - 1.0).abs() > 0.001 {
                return Err(Error::source(at, "a rotation is a unit quaternion"));
            }
            Ok(Some(Rotation::Quat([w, x, y, z])))
        }
        (None, Some(_)) => Err(Error::source(at, "a rotation is [w, x, y, z]")),
        (Some(_), Some(_)) => Err(Error::source(at, "a heading or a rotation, not both")),
    }
}

fn which(at: &Loc, pack: &str, index: Option<usize>, symbol: Option<&str>) -> Result<Which, Error> {
    match (index, symbol) {
        (Some(i), None) => Ok(Which::Index(i)),
        (None, Some(s)) => symbol::qualify(pack, s)
            .map(Which::Symbol)
            .map_err(|why| Error::source(at, why)),
        _ => Err(Error::source(
            at,
            "a static is named by its index in the base record or by its symbol, and not both",
        )),
    }
}

fn small(at: &Loc, v: Option<i64>, max: i64, what: &str) -> Result<Option<u16>, Error> {
    v.map(|v| {
        if (0..=max).contains(&v) {
            u16::try_from(v).map_err(|_| Error::source(at, format!("{what} is 0 to {max}")))
        } else {
            Err(Error::source(at, format!("{what} is 0 to {max}, not {v}")))
        }
    })
    .transpose()
}

fn vertex_at(at: &Loc, v: &[i64]) -> Result<(u8, u8), Error> {
    match v {
        [x, y] if (0..=8).contains(x) && (0..=8).contains(y) => Ok((
            u8::try_from(*x).unwrap_or_default(),
            u8::try_from(*y).unwrap_or_default(),
        )),
        _ => Err(Error::source(at, "a vertex is [x, y], each 0 to 8")),
    }
}

fn read_edits(pack: &Pack, path: &Path) -> Result<LandblockEdits, Error> {
    let stem = path
        .file_stem()
        .and_then(|s| s.to_str())
        .unwrap_or_default();
    let landblock = (stem.len() == 4)
        .then(|| u16::from_str_radix(stem, 16).ok())
        .flatten()
        .filter(|lb| (lb >> 8) < 0xFF && (lb & 0xFF) < 0xFF)
        .ok_or_else(|| {
            Error::source(
                &Loc::new(path, 0),
                "a landblock's file is named for it, four hex digits as A9B4.toml",
            )
        })?;
    let text = read_text(path)?;
    let file: EditsFile = parse_toml(path, &text)?;
    let loc = |start: usize| Loc::at(path, &text, start);
    let mut edits = LandblockEdits {
        landblock,
        file: path.to_path_buf(),
        statics: Vec::new(),
        moves: Vec::new(),
        removes: Vec::new(),
        vertices: Vec::new(),
    };
    for row in &file.statics {
        let at = loc(row.span().start);
        let r = row.get_ref();
        let name = r
            .name
            .as_deref()
            .map(|n| symbol::qualify(&pack.name, n))
            .transpose()
            .map_err(|why| Error::source(&at, why))?;
        let model = id_of(&at, r.model, "model")?;
        if !matches!(model.raw() >> 24, 0x01 | 0x02) {
            return Err(Error::source(
                &at,
                format!(
                    "{:#010X} is not a model: a GfxObj (0x01) or a Setup (0x02)",
                    model.raw()
                ),
            ));
        }
        let item = AddStatic {
            name,
            model,
            at: place(&at, &r.at)?,
            rotation: rotation(&at, r.heading, r.rotation.as_deref())?
                .unwrap_or(Rotation::Heading(0.0)),
        };
        edits.statics.push(Located { at, item });
    }
    for row in &file.moves {
        let at = loc(row.span().start);
        let r = row.get_ref();
        let item = MoveStatic {
            which: which(&at, &pack.name, r.index, r.symbol.as_deref())?,
            at: r.at.as_deref().map(|v| place(&at, v)).transpose()?,
            rotation: rotation(&at, r.heading, r.rotation.as_deref())?,
        };
        if item.at.is_none() && item.rotation.is_none() {
            return Err(Error::source(
                &at,
                "a move gives at, a heading or a rotation",
            ));
        }
        edits.moves.push(Located { at, item });
    }
    for row in &file.removes {
        let at = loc(row.span().start);
        let r = row.get_ref();
        let item = RemoveStatic {
            which: which(&at, &pack.name, r.index, r.symbol.as_deref())?,
            model: r.model.map(|m| id_of(&at, m, "model")).transpose()?,
        };
        edits.removes.push(Located { at, item });
    }
    for row in &file.vertex {
        let at = loc(row.span().start);
        let r = row.get_ref();
        let from = vertex_at(&at, &r.at)?;
        let to =
            r.to.as_deref()
                .map(|v| vertex_at(&at, v))
                .transpose()?
                .unwrap_or(from);
        if to.0 < from.0 || to.1 < from.1 {
            return Err(Error::source(&at, "to is at or north-east of at"));
        }
        let item = VertexEdit {
            from,
            to,
            height: small(&at, r.height, 255, "height")?
                .map(|h| u8::try_from(h).unwrap_or(u8::MAX)),
            terrain: small(&at, r.terrain, 31, "terrain")?,
            scene: small(&at, r.scene, 31, "scene")?,
            road: small(&at, r.road, 3, "road")?,
        };
        if item.height.is_none()
            && item.terrain.is_none()
            && item.scene.is_none()
            && item.road.is_none()
        {
            return Err(Error::source(
                &at,
                "a vertex edit sets a height, a terrain, a scene or a road",
            ));
        }
        edits.vertices.push(Located { at, item });
    }
    Ok(edits)
}

/// Read the pack in `dir`.
///
/// # Errors
/// A file that does not read, or a record or operation the format does not allow.
pub fn read_pack(dir: &Path) -> Result<Pack, Error> {
    let meta_path = dir.join("pack.toml");
    let text = read_text(&meta_path)?;
    let meta: PackFile = parse_toml(&meta_path, &text)?;
    if !symbol::is_name(&meta.name) {
        return Err(Error::source(
            &Loc::new(&meta_path, 0),
            format!(
                "{:?} is not a pack name: lowercase letters, digits and hyphens",
                meta.name
            ),
        ));
    }
    let mut pack = Pack {
        dir: dir.to_path_buf(),
        name: meta.name,
        version: meta.version,
        authors: meta.authors,
        license: meta.license,
        description: meta.description,
        records: Vec::new(),
        edits: Vec::new(),
        sql: Vec::new(),
    };
    for path in files_in(&dir.join("records"), "toml")? {
        let records = read_records(&pack, &path)?;
        pack.records.extend(records);
    }
    for path in files_in(&dir.join("edits").join("landblocks"), "toml")? {
        let edits = read_edits(&pack, &path)?;
        if let Some(other) = pack.edits.iter().find(|e| e.landblock == edits.landblock) {
            return Err(Error::source(
                &Loc::new(&path, 0),
                format!(
                    "landblock {:04X} has a second file, {}",
                    edits.landblock,
                    other.file.display()
                ),
            ));
        }
        pack.edits.push(edits);
    }
    pack.edits.sort_by_key(|e| e.landblock);
    for (rel, path) in sql_files(&dir.join("server").join("sql"))? {
        let text = read_text(&path)?;
        pack.sql.push(SqlFile { rel, path, text });
    }
    Ok(pack)
}
