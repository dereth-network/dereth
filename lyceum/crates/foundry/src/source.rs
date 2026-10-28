//! A pack: a folder of text and pictures, read.
//!
//! ```text
//! mypack/
//!   pack.toml                    name, version, game, authors, license, description
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
//! Any row of `records/` or `edits/` may say where it came from with `prov`, an inline table:
//! `prov = { from = "dat", record = 0x0200040B }`, or `from` `capture`, `derived` or `authored`
//! with what each names (`dereth_pack::provenance`). A row that says nothing a person wrote. It
//! changes nothing a build makes; [`Pack::provenance`] answers it for a row.
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
//! [[replace-cell]]               # one of the landblock's own cells reshaped: another piece in
//! cell = 0x0105                  # its place, keeping its number, its statics and its ways
//! environment = 0x0D000105
//! piece = 2
//! surfaces = [0x080000A1, 0x080000A2]
//! at = [12.0, -40.0, 0.0]        # landblock meters
//! rotation = [1.0, 0.0, 0.0, 0.0]
//! portals = ["keep", ""]         # per portal of the piece: "keep" (the way the cell had there),
//!                                # "" (shut, free to build on), "outside", "<new cell>:<portal>"
//!
//! [[remove-cell]]                # one of the landblock's own cells taken away, its statics with
//! cell = 0x017D                  # it; it keeps its number, closed far under the landblock
//!
//! [dungeon]                      # the landblock's dungeon: what it is called, and the look its
//! name = "Drudge Warren"         # pieces are drawn in (a dungeon whose look it shares, by its
//! look = 0x019E                  # landblock)
//!
//! [[move-cell-static]]           # a static standing in an indoor cell (a building's room or a
//! cell = 0x0105                  # dungeon's), by the cell's number or a pack cell's symbol,
//! index = 2                      # and its place among the cell's own statics
//! at = [61.5, 40.2, 94.0]        # landblock meters (the height is a height here, not "ground")
//! heading = 90.0
//!
//! [[remove-cell-static]]
//! cell = 0x0105
//! index = 3
//! model = 0x02000BCD             # optional: what the cell holds there, checked
//!
//! [[add-cell-static]]            # a static added to one of the world's own cells, after the
//! cell = 0x0105                  # statics it holds
//! model = 0x0200040B
//! at = [61.5, 40.2, 94.0]        # landblock meters
//! heading = 90.0
//!
//! [[add-building]]               # a copy of a retail building, its rooms with it
//! name = "cottage-1"             # its symbol: its rooms are cells cottage-1-room-000 and on
//! model = 0x01002228             # a set (a mansion's four) lists its models, and a template each
//! template = "3B9C/3"            # the retail copy: its landblock and its place in its buildings
//! at = [100.0, 60.0, 82.0]       # its origin, landblock meters; z a height or "ground"
//! heading = 90.0
//! furnished = "bare"             # or another copy's furniture, "3DC7/1"; left out, the template's
//! house = "cottage-1-house"      # optional, a house players can buy: its rooms and its land
//!                                # cell belong to this spawn of the landblock (a set lists one
//!                                # for each building)
//!
//! [[add-dungeon]]                # a copy of a retail dungeon: every cell of its landblock, in
//! name = "villa-1-basement"      # the same place; its cells are villa-1-basement-room-000 on
//! template = 0x0398              # the retail dungeon's landblock
//! house = "villa-1-basement-house"  # optional: its cells belong to this spawn of the landblock
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

use dereth_pack::provenance::{Provenance, Rows};

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
    /// The new records' rows that say where they came from (a landblock's are its edits').
    pub provenance: Rows,
}

impl Pack {
    /// Where the row at `at`, a record or an edit of this pack, came from: a person, unless the
    /// row says otherwise.
    #[must_use]
    pub fn provenance(&self, at: &Loc) -> &Provenance {
        let edits = self.edits.iter().find(|e| e.file == at.file);
        edits.map_or_else(|| self.provenance.of(at), |e| e.provenance.of(at))
    }
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
    Geometry {
        target: Target,
        record: Box<crate::model_import::Record>,
    },
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
            Self::Geometry { record, .. } => record.kind(),
            Self::RenderSurface { .. } => Kind::RenderSurface,
            Self::SurfaceTexture { .. } => Kind::SurfaceTexture,
            Self::Surface { .. } => Kind::Surface,
        }
    }

    #[must_use]
    pub fn target(&self) -> &Target {
        match self {
            Self::Geometry { target, .. }
            | Self::RenderSurface { target, .. }
            | Self::SurfaceTexture { target, .. }
            | Self::Surface { target, .. } => target,
        }
    }

    /// The records this one names, with the kind each must be.
    #[must_use]
    pub fn references(&self) -> Vec<(Kind, &RecordRef)> {
        match self {
            Self::Geometry { record, .. } => record.references(),
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
    pub model: RecordRef,
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

/// An indoor cell an operation names: by its number in the landblock, or a pack's new cell by
/// its symbol (in full).
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum CellRef {
    Number(u16),
    Symbol(String),
}

/// Move or turn a static standing in an indoor cell, by its place among the cell's own statics.
#[derive(Clone, Debug, PartialEq)]
pub struct MoveCellStatic {
    pub cell: CellRef,
    pub index: usize,
    /// Landblock meters.
    pub at: Option<[f32; 3]>,
    pub rotation: Option<Rotation>,
}

/// Take a static standing in an indoor cell away.
#[derive(Clone, Debug, PartialEq)]
pub struct RemoveCellStatic {
    pub cell: CellRef,
    pub index: usize,
    /// What the cell must hold there.
    pub model: Option<DataId>,
}

/// A static added to an indoor cell not of the pack's own, after the statics it holds.
#[derive(Clone, Debug, PartialEq)]
pub struct AddCellStatic {
    pub cell: CellRef,
    pub model: DataId,
    /// Landblock meters.
    pub at: [f32; 3],
    pub rotation: Rotation,
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

/// Where a new cell's portal leads.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Link {
    /// Another new cell's portal: the cell's symbol (in full), and the portal's place in that
    /// cell's piece.
    Cell { cell: String, portal: usize },
    /// A portal of one of the landblock's own cells a pack reshaped: the cell's number, and the
    /// portal's place in its new piece. Written `"0x0105:1"`.
    Base { cell: u16, portal: usize },
    /// Of a reshaped cell: the way the cell had where this portal stands. Written `"keep"`.
    Keep,
    /// The outdoors.
    Outside,
    /// Nowhere: the opening stays shut (a check warns of it).
    Shut,
    /// Several cells, each through part of the opening: new cells' portals ([`Link::Cell`]) or
    /// reshaped cells' ([`Link::Base`]). Written as their links with commas between,
    /// `"cell-3:0, cell-4:0"`.
    Many(Vec<Link>),
}

/// A static standing in a cell.
#[derive(Clone, Debug, PartialEq)]
pub struct CellStatic {
    pub model: DataId,
    /// Landblock meters, as the files hold a cell's statics.
    pub at: [f32; 3],
    pub rotation: Rotation,
}

/// An indoor cell a pack adds: one piece of an environment, placed in the landblock.
#[derive(Clone, Debug, PartialEq)]
pub struct AddCell {
    /// Its symbol, in full.
    pub name: String,
    /// The environment and the piece of it (its cell struct).
    pub environment: DataId,
    pub piece: u16,
    /// The surfaces its polygons are drawn with, by the piece's own indices.
    pub surfaces: Vec<DataId>,
    /// Its origin in landblock meters, and its turn.
    pub at: [f32; 3],
    pub rotation: Rotation,
    /// Each of the piece's portals, in the piece's order.
    pub portals: Vec<Link>,
    pub statics: Vec<CellStatic>,
}

/// One of the landblock's own cells reshaped: another piece in its place, keeping its number, its
/// statics and the doorways matched by the new piece; dropped doorways produce warnings.
#[derive(Clone, Debug, PartialEq)]
pub struct ReplaceCell {
    pub cell: u16,
    pub environment: DataId,
    pub piece: u16,
    pub surfaces: Vec<DataId>,
    /// Its origin in landblock meters, and its turn.
    pub at: [f32; 3],
    pub rotation: Rotation,
    /// Each of the piece's portals, in the piece's order.
    pub portals: Vec<Link>,
}

/// One of the landblock's own cells taken away: it keeps its number, closed and empty far under
/// the landblock, and the ways into it are shut.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct RemoveCell {
    pub cell: u16,
}

/// Move or turn a building of the base, with everything inside it.
#[derive(Clone, Debug, PartialEq)]
pub struct MoveBuilding {
    /// Its place in the base's building list.
    pub index: usize,
    /// Its new origin in landblock meters (its height kept when none is given).
    pub at: Option<[f32; 3]>,
    pub rotation: Option<Rotation>,
}

/// The furniture a dropped building's rooms hold.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Furnished {
    /// The template's own.
    Template,
    /// None.
    Bare,
    /// Another copy's, one for each building of the drop: a copy of the same model laid out the
    /// same way.
    Like(Vec<crate::buildings::Site>),
}

/// A copy of a retail building, with its rooms, dropped in the landblock: one building, or a set
/// of buildings that stand together (a mansion's four), carried together.
#[derive(Clone, Debug, PartialEq)]
pub struct AddBuilding {
    /// Its symbol, in full. Its rooms are numbered as cells named `<name>-room-000` and on.
    pub name: String,
    /// Each building's model, and the retail copy it is taken from.
    pub models: Vec<DataId>,
    pub templates: Vec<crate::buildings::Site>,
    /// Where the first building's origin stands, in landblock meters (a height, or the ground's
    /// with the template's own height over its ground), and its turn. The others keep their
    /// places from it.
    pub at: Place,
    pub rotation: Rotation,
    pub furnished: Furnished,
    /// For a house players can buy, the spawn each building's rooms and land cell belong to: its
    /// symbol in full, without the landblock (the row's own). Empty for a plain building.
    pub house: Vec<String>,
}

/// A copy of a retail dungeon: every cell of its landblock, each where it stands there.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct AddDungeon {
    /// Its symbol, in full. Its cells are numbered as cells named `<name>-room-000` and on.
    pub name: String,
    /// The retail dungeon's landblock.
    pub template: u16,
    /// The spawn its cells belong to, when it is a house's: its symbol in full, without the
    /// landblock.
    pub house: Option<String>,
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
    pub cells: Vec<Located<AddCell>>,
    pub buildings: Vec<Located<MoveBuilding>>,
    pub cell_moves: Vec<Located<MoveCellStatic>>,
    pub cell_removes: Vec<Located<RemoveCellStatic>>,
    pub cell_adds: Vec<Located<AddCellStatic>>,
    pub replaces: Vec<Located<ReplaceCell>>,
    /// The landblock's own cells taken away.
    pub taken: Vec<Located<RemoveCell>>,
    pub drops: Vec<Located<AddBuilding>>,
    pub dungeon_copies: Vec<Located<AddDungeon>>,
    /// The landblock's dungeon, as the pack names it.
    pub dungeon: Option<Located<DungeonInfo>>,
    /// The rows that say where they came from.
    pub provenance: Rows,
}

/// What a pack says of a landblock's dungeon: its name, and the look its pieces are drawn in, as a
/// dungeon's landblock whose look it shares.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct DungeonInfo {
    pub name: Option<String>,
    pub look: Option<u16>,
}

impl LandblockEdits {
    /// Whether the file changes the landblock's statics record: its statics, its cells or its
    /// buildings.
    #[must_use]
    pub fn touches_statics(&self) -> bool {
        !self.statics.is_empty()
            || !self.moves.is_empty()
            || !self.removes.is_empty()
            || !self.cells.is_empty()
            || !self.buildings.is_empty()
            || !self.replaces.is_empty()
            || !self.taken.is_empty()
            || !self.drops.is_empty()
            || !self.dungeon_copies.is_empty()
    }

    /// Whether the file moves or removes a static standing in an indoor cell.
    #[must_use]
    pub fn touches_cell_statics(&self) -> bool {
        !self.cell_moves.is_empty() || !self.cell_removes.is_empty() || !self.cell_adds.is_empty()
    }
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
struct RecordsFile {
    #[serde(default)]
    piece: Vec<Spanned<PieceRow>>,
    #[serde(default)]
    model: Vec<Spanned<ModelRow>>,
    #[serde(default, rename = "render-surface")]
    render_surface: Vec<Spanned<RenderSurfaceRow>>,
    #[serde(default, rename = "surface-texture")]
    surface_texture: Vec<Spanned<SurfaceTextureRow>>,
    #[serde(default)]
    surface: Vec<Spanned<SurfaceRow>>,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct PieceRow {
    name: String,
    file: String,
    /// Where the row came from; left out, a person wrote it.
    #[serde(default)]
    prov: Option<Provenance>,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct ModelRow {
    name: String,
    replace: Option<u32>,
    file: String,
    #[serde(default)]
    collision: crate::model_import::Collision,
    #[serde(default)]
    distances: Vec<f32>,
    /// Where the row came from; left out, a person wrote it.
    #[serde(default)]
    prov: Option<Provenance>,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct RenderSurfaceRow {
    name: Option<String>,
    replace: Option<i64>,
    picture: String,
    format: Option<String>,
    category: Option<u32>,
    /// Where the row came from; left out, a person wrote it.
    #[serde(default)]
    prov: Option<Provenance>,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct SurfaceTextureRow {
    name: Option<String>,
    replace: Option<i64>,
    levels: Vec<toml::Value>,
    category: Option<u32>,
    /// Where the row came from; left out, a person wrote it.
    #[serde(default)]
    prov: Option<Provenance>,
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
    /// Where the row came from; left out, a person wrote it.
    #[serde(default)]
    prov: Option<Provenance>,
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
    #[serde(default)]
    cell: Vec<Spanned<CellRow>>,
    #[serde(default, rename = "move-building")]
    move_building: Vec<Spanned<MoveBuildingRow>>,
    #[serde(default, rename = "move-cell-static")]
    move_cell_static: Vec<Spanned<MoveCellStaticRow>>,
    #[serde(default, rename = "remove-cell-static")]
    remove_cell_static: Vec<Spanned<RemoveCellStaticRow>>,
    #[serde(default, rename = "add-cell-static")]
    add_cell_static: Vec<Spanned<AddCellStaticRow>>,
    #[serde(default, rename = "replace-cell")]
    replace_cell: Vec<Spanned<ReplaceCellRow>>,
    #[serde(default, rename = "remove-cell")]
    remove_cell: Vec<Spanned<RemoveCellRow>>,
    dungeon: Option<Spanned<DungeonRow>>,
    #[serde(default, rename = "add-building")]
    add_building: Vec<Spanned<AddBuildingRow>>,
    #[serde(default, rename = "add-dungeon")]
    add_dungeon: Vec<Spanned<AddDungeonRow>>,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct AddDungeonRow {
    name: String,
    template: i64,
    house: Option<String>,
    /// Where the row came from; left out, a person wrote it.
    #[serde(default)]
    prov: Option<Provenance>,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct DungeonRow {
    name: Option<String>,
    look: Option<i64>,
    /// Where the row came from; left out, a person wrote it.
    #[serde(default)]
    prov: Option<Provenance>,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct AddBuildingRow {
    name: String,
    model: toml::Value,
    template: toml::Value,
    at: Vec<toml::Value>,
    heading: Option<f32>,
    rotation: Option<Vec<f32>>,
    furnished: Option<toml::Value>,
    house: Option<toml::Value>,
    /// Where the row came from; left out, a person wrote it.
    #[serde(default)]
    prov: Option<Provenance>,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct ReplaceCellRow {
    cell: i64,
    environment: i64,
    piece: Option<u16>,
    surfaces: Vec<i64>,
    at: Vec<toml::Value>,
    heading: Option<f32>,
    rotation: Option<Vec<f32>>,
    portals: Vec<String>,
    /// Where the row came from; left out, a person wrote it.
    #[serde(default)]
    prov: Option<Provenance>,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct RemoveCellRow {
    cell: i64,
    /// Where the row came from; left out, a person wrote it.
    #[serde(default)]
    prov: Option<Provenance>,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct AddCellStaticRow {
    cell: toml::Value,
    model: i64,
    at: Vec<toml::Value>,
    heading: Option<f32>,
    rotation: Option<Vec<f32>>,
    /// Where the row came from; left out, a person wrote it.
    #[serde(default)]
    prov: Option<Provenance>,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct MoveCellStaticRow {
    cell: toml::Value,
    index: usize,
    at: Option<Vec<toml::Value>>,
    heading: Option<f32>,
    rotation: Option<Vec<f32>>,
    /// Where the row came from; left out, a person wrote it.
    #[serde(default)]
    prov: Option<Provenance>,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct RemoveCellStaticRow {
    cell: toml::Value,
    index: usize,
    model: Option<i64>,
    /// Where the row came from; left out, a person wrote it.
    #[serde(default)]
    prov: Option<Provenance>,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct CellRow {
    name: String,
    environment: i64,
    piece: Option<u16>,
    surfaces: Vec<i64>,
    at: Vec<toml::Value>,
    heading: Option<f32>,
    rotation: Option<Vec<f32>>,
    portals: Vec<String>,
    #[serde(default, rename = "static")]
    statics: Vec<CellStaticRow>,
    /// Where the row came from; left out, a person wrote it.
    #[serde(default)]
    prov: Option<Provenance>,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct CellStaticRow {
    model: i64,
    at: Vec<toml::Value>,
    heading: Option<f32>,
    rotation: Option<Vec<f32>>,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct MoveBuildingRow {
    index: usize,
    at: Option<Vec<toml::Value>>,
    heading: Option<f32>,
    rotation: Option<Vec<f32>>,
    /// Where the row came from; left out, a person wrote it.
    #[serde(default)]
    prov: Option<Provenance>,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct StaticRow {
    name: Option<String>,
    model: toml::Value,
    at: Vec<toml::Value>,
    heading: Option<f32>,
    rotation: Option<Vec<f32>>,
    /// Where the row came from; left out, a person wrote it.
    #[serde(default)]
    prov: Option<Provenance>,
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
    /// Where the row came from; left out, a person wrote it.
    #[serde(default)]
    prov: Option<Provenance>,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct RemoveRow {
    index: Option<usize>,
    #[serde(rename = "static")]
    symbol: Option<String>,
    model: Option<i64>,
    /// Where the row came from; left out, a person wrote it.
    #[serde(default)]
    prov: Option<Provenance>,
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
    /// Where the row came from; left out, a person wrote it.
    #[serde(default)]
    prov: Option<Provenance>,
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

fn read_records(
    pack: &Pack,
    path: &Path,
    rows: &mut Rows,
) -> Result<Vec<Located<NewRecord>>, Error> {
    let text = read_text(path)?;
    let file: RecordsFile = parse_toml(path, &text)?;
    let mut out: Vec<(usize, Located<NewRecord>)> = Vec::new();
    let loc = |start: usize| Loc::at(path, &text, start);
    for row in &file.piece {
        let at = loc(row.span().start);
        rows.note(&at, row.get_ref().prov.as_ref());
        let r = row.get_ref();
        let name = symbol::qualify(&pack.name, &r.name).map_err(|s| Error::source(&at, s))?;
        for record in
            crate::model_import::piece::records(&pack.dir, &pack.dir.join(&r.file), &at, &name)?
        {
            out.push((row.span().start, record));
        }
    }
    for row in &file.model {
        let at = loc(row.span().start);
        rows.note(&at, row.get_ref().prov.as_ref());
        let r = row.get_ref();
        let name = symbol::qualify(&pack.name, &r.name).map_err(|s| Error::source(&at, s))?;
        for record in crate::model_import::records(
            &pack.dir,
            &pack.dir.join(&r.file),
            &at,
            &name,
            r.replace.map(DataId),
            r.collision,
            &r.distances,
        )? {
            out.push((row.span().start, record));
        }
    }
    for row in &file.render_surface {
        let at = loc(row.span().start);
        rows.note(&at, row.get_ref().prov.as_ref());
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
        rows.note(&at, row.get_ref().prov.as_ref());
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
        rows.note(&at, row.get_ref().prov.as_ref());
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

/// A point anywhere in landblock meters (a cell may stand under the ground, and past the
/// landblock's edge as retail's do).
fn point(at: &Loc, v: &[toml::Value]) -> Result<[f32; 3], Error> {
    let [x, y, z] = v else {
        return Err(Error::source(at, "at is [x, y, z]"));
    };
    Ok([
        number(at, x, "x")?,
        number(at, y, "y")?,
        number(at, z, "z")?,
    ])
}

/// A portal's link as written: `"<cell>:<portal>"`, `"outside"`, or `""` for none; several cells'
/// with commas between.
fn link(at: &Loc, pack: &str, v: &str) -> Result<Link, Error> {
    if v.contains(',') {
        let all = v
            .split(',')
            .map(|one| link(at, pack, one))
            .collect::<Result<Vec<Link>, Error>>()?;
        if all
            .iter()
            .any(|l| !matches!(l, Link::Cell { .. } | Link::Base { .. }))
        {
            return Err(Error::source(
                at,
                format!(
                    "{v:?}: a portal that joins several cells names each by \"<cell>:<portal>\""
                ),
            ));
        }
        return Ok(Link::Many(all));
    }
    match v.trim() {
        "" => Ok(Link::Shut),
        "outside" => Ok(Link::Outside),
        "keep" => Ok(Link::Keep),
        s => {
            let (cell, portal) = s.rsplit_once(':').ok_or_else(|| {
                Error::source(
                    at,
                    format!(
                        "{s:?} is not a portal's link: \"<cell>:<portal>\", \"outside\", \"keep\" or \"\""
                    ),
                )
            })?;
            let portal = portal
                .parse()
                .map_err(|_| Error::source(at, format!("{portal:?} is not a portal's place")))?;
            if let Some(n) = cell.strip_prefix("0x").or_else(|| cell.strip_prefix("0X")) {
                let n = u16::from_str_radix(n, 16)
                    .ok()
                    .filter(|n| *n >= crate::cells::FIRST_CELL)
                    .ok_or_else(|| Error::source(at, format!("{cell} is not a cell's number")))?;
                return Ok(Link::Base { cell: n, portal });
            }
            Ok(Link::Cell {
                cell: symbol::qualify(pack, cell).map_err(|why| Error::source(at, why))?,
                portal,
            })
        }
    }
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
    let text = read_text(path)?;
    parse_edits(pack, path, &text)
}

/// The landblock `path` names, by its file name (`A9B4.toml`).
///
/// # Errors
/// A name that is not four hex digits naming a landblock.
pub fn landblock_of_file(path: &Path) -> Result<u16, Error> {
    let stem = path
        .file_stem()
        .and_then(|s| s.to_str())
        .unwrap_or_default();
    (stem.len() == 4)
        .then(|| u16::from_str_radix(stem, 16).ok())
        .flatten()
        .filter(|lb| (lb >> 8) < 0xFF && (lb & 0xFF) < 0xFF)
        .ok_or_else(|| {
            Error::source(
                &Loc::new(path, 0),
                "a landblock's file is named for it, four hex digits as A9B4.toml",
            )
        })
}

/// One landblock's operations, read from `text`, the file `path` of `pack`.
///
/// # Errors
/// A file name that names no landblock, or an operation the format does not allow.
pub fn parse_edits(pack: &Pack, path: &Path, text: &str) -> Result<LandblockEdits, Error> {
    let landblock = landblock_of_file(path)?;
    let file: EditsFile = parse_toml(path, text)?;
    let loc = |start: usize| Loc::at(path, text, start);
    let mut edits = LandblockEdits {
        landblock,
        file: path.to_path_buf(),
        statics: Vec::new(),
        moves: Vec::new(),
        removes: Vec::new(),
        vertices: Vec::new(),
        cells: Vec::new(),
        buildings: Vec::new(),
        cell_moves: Vec::new(),
        cell_removes: Vec::new(),
        cell_adds: Vec::new(),
        replaces: Vec::new(),
        taken: Vec::new(),
        dungeon: None,
        drops: Vec::new(),
        dungeon_copies: Vec::new(),
        provenance: Rows::default(),
    };
    if let Some(row) = &file.dungeon {
        let at = loc(row.span().start);
        edits.provenance.note(&at, row.get_ref().prov.as_ref());
        let r = row.get_ref();
        let look = r
            .look
            .map(|l| {
                u16::try_from(l)
                    .map_err(|_| Error::source(&at, format!("look {l:#X} is not a landblock")))
            })
            .transpose()?;
        edits.dungeon = Some(Located {
            at,
            item: DungeonInfo {
                name: r.name.clone().filter(|n| !n.trim().is_empty()),
                look,
            },
        });
    }
    for row in &file.statics {
        let at = loc(row.span().start);
        edits.provenance.note(&at, row.get_ref().prov.as_ref());
        let r = row.get_ref();
        let name = r
            .name
            .as_deref()
            .map(|n| symbol::qualify(&pack.name, n))
            .transpose()
            .map_err(|why| Error::source(&at, why))?;
        let model = record_ref(&at, &pack.name, &r.model)?;
        if let RecordRef::Id(id) = &model {
            if !matches!(id.raw() >> 24, 0x01 | 0x02) {
                return Err(Error::source(
                    &at,
                    "a model is a GfxObj (0x01) or a Setup (0x02)",
                ));
            }
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
        edits.provenance.note(&at, row.get_ref().prov.as_ref());
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
        edits.provenance.note(&at, row.get_ref().prov.as_ref());
        let r = row.get_ref();
        let item = RemoveStatic {
            which: which(&at, &pack.name, r.index, r.symbol.as_deref())?,
            model: r.model.map(|m| id_of(&at, m, "model")).transpose()?,
        };
        edits.removes.push(Located { at, item });
    }
    for row in &file.vertex {
        let at = loc(row.span().start);
        edits.provenance.note(&at, row.get_ref().prov.as_ref());
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
    for row in &file.cell {
        let at = loc(row.span().start);
        edits.provenance.note(&at, row.get_ref().prov.as_ref());
        let r = row.get_ref();
        let environment = id_of(&at, r.environment, "environment")?;
        if environment.raw() >> 24 != 0x0D {
            return Err(Error::source(
                &at,
                format!("{:#010X} is not an environment (0x0D)", environment.raw()),
            ));
        }
        let surfaces = r
            .surfaces
            .iter()
            .map(|s| {
                let id = id_of(&at, *s, "surface")?;
                if id.raw() >> 24 == 0x08 {
                    Ok(id)
                } else {
                    Err(Error::source(
                        &at,
                        format!("{:#010X} is not a surface (0x08)", id.raw()),
                    ))
                }
            })
            .collect::<Result<_, _>>()?;
        let statics = r
            .statics
            .iter()
            .map(|s| {
                let model = id_of(&at, s.model, "model")?;
                if !matches!(model.raw() >> 24, 0x01 | 0x02) {
                    return Err(Error::source(
                        &at,
                        format!("{:#010X} is not a model", model.raw()),
                    ));
                }
                Ok(CellStatic {
                    model,
                    at: point(&at, &s.at)?,
                    rotation: rotation(&at, s.heading, s.rotation.as_deref())?
                        .unwrap_or(Rotation::Heading(0.0)),
                })
            })
            .collect::<Result<_, _>>()?;
        let item = AddCell {
            name: symbol::qualify(&pack.name, &r.name).map_err(|why| Error::source(&at, why))?,
            environment,
            piece: r.piece.unwrap_or(0),
            surfaces,
            at: point(&at, &r.at)?,
            rotation: rotation(&at, r.heading, r.rotation.as_deref())?
                .unwrap_or(Rotation::Heading(0.0)),
            portals: r
                .portals
                .iter()
                .map(|p| link(&at, &pack.name, p))
                .collect::<Result<_, _>>()?,
            statics,
        };
        if let Some(first) = edits.cells.iter().find(|c| c.item.name == item.name) {
            return Err(twice(
                &pack.name,
                format!("cell {}", item.name),
                &first.at,
                &at,
                "places it",
            ));
        }
        edits.cells.push(Located { at, item });
    }
    for row in &file.move_building {
        let at = loc(row.span().start);
        edits.provenance.note(&at, row.get_ref().prov.as_ref());
        let r = row.get_ref();
        let item = MoveBuilding {
            index: r.index,
            at: r.at.as_deref().map(|v| point(&at, v)).transpose()?,
            rotation: rotation(&at, r.heading, r.rotation.as_deref())?,
        };
        if item.at.is_none() && item.rotation.is_none() {
            return Err(Error::source(
                &at,
                "a building's move gives at, a heading or a rotation",
            ));
        }
        if let Some(first) = edits.buildings.iter().find(|b| b.item.index == item.index) {
            return Err(twice(
                &pack.name,
                format!("building {landblock:04X} {}", item.index),
                &first.at,
                &at,
                "moves it",
            ));
        }
        edits.buildings.push(Located { at, item });
    }
    for row in &file.replace_cell {
        let at = loc(row.span().start);
        edits.provenance.note(&at, row.get_ref().prov.as_ref());
        let r = row.get_ref();
        let CellRef::Number(cell) = cell_ref(&at, &pack.name, &toml::Value::Integer(r.cell))?
        else {
            return Err(Error::source(&at, "a reshaped cell is named by its number"));
        };
        let environment = id_of(&at, r.environment, "environment")?;
        if environment.raw() >> 24 != 0x0D {
            return Err(Error::source(
                &at,
                format!("{:#010X} is not an environment (0x0D)", environment.raw()),
            ));
        }
        let surfaces = r
            .surfaces
            .iter()
            .map(|s| id_of(&at, *s, "surface"))
            .collect::<Result<_, _>>()?;
        let item = ReplaceCell {
            cell,
            environment,
            piece: r.piece.unwrap_or(0),
            surfaces,
            at: point(&at, &r.at)?,
            rotation: rotation(&at, r.heading, r.rotation.as_deref())?
                .unwrap_or(Rotation::Heading(0.0)),
            portals: r
                .portals
                .iter()
                .map(|p| link(&at, &pack.name, p))
                .collect::<Result<_, _>>()?,
        };
        if let Some(first) = edits.replaces.iter().find(|x| x.item.cell == cell) {
            return Err(twice(
                &pack.name,
                format!("cell {landblock:04X} {cell:04X}"),
                &first.at,
                &at,
                "reshapes it",
            ));
        }
        edits.replaces.push(Located { at, item });
    }
    for row in &file.remove_cell {
        let at = loc(row.span().start);
        edits.provenance.note(&at, row.get_ref().prov.as_ref());
        let r = row.get_ref();
        let CellRef::Number(cell) = cell_ref(&at, &pack.name, &toml::Value::Integer(r.cell))?
        else {
            return Err(Error::source(
                &at,
                "a cell taken away is named by its number",
            ));
        };
        if let Some(first) = edits.taken.iter().find(|x| x.item.cell == cell) {
            return Err(twice(
                &pack.name,
                format!("cell {landblock:04X} {cell:04X}"),
                &first.at,
                &at,
                "takes it away",
            ));
        }
        if let Some(first) = edits.replaces.iter().find(|x| x.item.cell == cell) {
            return Err(twice(
                &pack.name,
                format!("cell {landblock:04X} {cell:04X}"),
                &first.at,
                &at,
                "reshapes it and takes it away",
            ));
        }
        edits.taken.push(Located {
            at,
            item: RemoveCell { cell },
        });
    }
    for row in &file.move_cell_static {
        let at = loc(row.span().start);
        edits.provenance.note(&at, row.get_ref().prov.as_ref());
        let r = row.get_ref();
        let item = MoveCellStatic {
            cell: cell_ref(&at, &pack.name, &r.cell)?,
            index: r.index,
            at: r.at.as_deref().map(|v| point(&at, v)).transpose()?,
            rotation: rotation(&at, r.heading, r.rotation.as_deref())?,
        };
        if item.at.is_none() && item.rotation.is_none() {
            return Err(Error::source(
                &at,
                "a move gives at, a heading or a rotation",
            ));
        }
        edits.cell_moves.push(Located { at, item });
    }
    for row in &file.remove_cell_static {
        let at = loc(row.span().start);
        edits.provenance.note(&at, row.get_ref().prov.as_ref());
        let r = row.get_ref();
        let item = RemoveCellStatic {
            cell: cell_ref(&at, &pack.name, &r.cell)?,
            index: r.index,
            model: r.model.map(|m| id_of(&at, m, "model")).transpose()?,
        };
        edits.cell_removes.push(Located { at, item });
    }
    for row in &file.add_cell_static {
        let at = loc(row.span().start);
        edits.provenance.note(&at, row.get_ref().prov.as_ref());
        let r = row.get_ref();
        let model = id_of(&at, r.model, "model")?;
        if !matches!(model.raw() >> 24, 0x01 | 0x02) {
            return Err(Error::source(
                &at,
                format!("{:#010X} is not a model", model.raw()),
            ));
        }
        let item = AddCellStatic {
            cell: cell_ref(&at, &pack.name, &r.cell)?,
            model,
            at: point(&at, &r.at)?,
            rotation: rotation(&at, r.heading, r.rotation.as_deref())?
                .unwrap_or(Rotation::Heading(0.0)),
        };
        edits.cell_adds.push(Located { at, item });
    }
    for row in &file.add_building {
        let at = loc(row.span().start);
        edits.provenance.note(&at, row.get_ref().prov.as_ref());
        let item = add_building(&at, &pack.name, row.get_ref())?;
        if let Some(first) = edits.drops.iter().find(|d| d.item.name == item.name) {
            return Err(twice(
                &pack.name,
                format!("building {}", item.name),
                &first.at,
                &at,
                "drops it",
            ));
        }
        // Its rooms are cells by name: no cell of the file may be named as one of them.
        let rooms = format!("{}-room-", item.name);
        if let Some(c) = edits.cells.iter().find(|c| c.item.name.starts_with(&rooms)) {
            return Err(Error::source(
                &c.at,
                format!(
                    "cell {} is named as a room of building {}",
                    c.item.name, item.name
                ),
            ));
        }
        edits.drops.push(Located { at, item });
    }
    for row in &file.add_dungeon {
        let at = loc(row.span().start);
        edits.provenance.note(&at, row.get_ref().prov.as_ref());
        let item = add_dungeon(&at, &pack.name, row.get_ref())?;
        let taken = edits
            .drops
            .iter()
            .map(|d| (&d.item.name, &d.at))
            .chain(edits.dungeon_copies.iter().map(|d| (&d.item.name, &d.at)))
            .find(|(n, _)| **n == item.name);
        if let Some((_, first)) = taken {
            return Err(twice(
                &pack.name,
                format!("{}'s rooms", item.name),
                first,
                &at,
                "names them",
            ));
        }
        let rooms = format!("{}-room-", item.name);
        if let Some(c) = edits.cells.iter().find(|c| c.item.name.starts_with(&rooms)) {
            return Err(Error::source(
                &c.at,
                format!(
                    "cell {} is named as a room of dungeon {}",
                    c.item.name, item.name
                ),
            ));
        }
        edits.dungeon_copies.push(Located { at, item });
    }
    Ok(edits)
}

/// A spawn of the row's landblock that a copy's cells belong to, as the row names it: a bare
/// symbol of the pack.
fn house_symbol(at: &Loc, pack: &str, s: &str) -> Result<String, Error> {
    if s.contains(':') || s.contains('@') {
        return Err(Error::source(
            at,
            "a house is named by a bare symbol: a spawn of the row's landblock",
        ));
    }
    symbol::qualify(pack, s).map_err(|why| Error::source(at, why))
}

fn add_dungeon(at: &Loc, pack: &str, r: &AddDungeonRow) -> Result<AddDungeon, Error> {
    if r.name.contains(':') || r.name.contains('@') {
        return Err(Error::source(
            at,
            "a copied dungeon is named by a bare symbol",
        ));
    }
    let name = symbol::qualify(pack, &r.name).map_err(|why| Error::source(at, why))?;
    let template = u16::try_from(r.template)
        .ok()
        .filter(|lb| (lb >> 8) < 0xFF && (lb & 0xFF) < 0xFF)
        .ok_or_else(|| {
            Error::source(
                at,
                format!(
                    "{:#X} is not a landblock: a dungeon's is 0x0398",
                    r.template
                ),
            )
        })?;
    Ok(AddDungeon {
        name,
        template,
        house: r
            .house
            .as_deref()
            .map(|h| house_symbol(at, pack, h))
            .transpose()?,
    })
}

/// One value or a list of them, as a list.
fn one_or_many(v: &toml::Value) -> Vec<&toml::Value> {
    match v {
        toml::Value::Array(a) => a.iter().collect(),
        other => vec![other],
    }
}

/// A site as a row writes it, `"3B9C/3"`.
fn site(at: &Loc, v: &toml::Value) -> Result<crate::buildings::Site, Error> {
    v.as_str()
        .and_then(crate::buildings::Site::parse)
        .ok_or_else(|| {
            Error::source(
                at,
                "a retail building is written \"3B9C/3\": its landblock and its place in that landblock's buildings",
            )
        })
}

fn add_building(at: &Loc, pack: &str, r: &AddBuildingRow) -> Result<AddBuilding, Error> {
    let name = symbol::qualify(pack, &r.name).map_err(|why| Error::source(at, why))?;
    if r.name.contains(':') || r.name.contains('@') {
        return Err(Error::source(
            at,
            "a dropped building is named by a bare symbol",
        ));
    }
    let models = one_or_many(&r.model)
        .into_iter()
        .map(|v| {
            let id = v
                .as_integer()
                .ok_or_else(|| Error::source(at, "a building's model is a GfxObj or Setup id"))?;
            let id = id_of(at, id, "model")?;
            if matches!(id.raw() >> 24, 0x01 | 0x02) {
                Ok(id)
            } else {
                Err(Error::source(
                    at,
                    format!("{:#010X} is not a model", id.raw()),
                ))
            }
        })
        .collect::<Result<Vec<_>, _>>()?;
    let templates = one_or_many(&r.template)
        .into_iter()
        .map(|v| site(at, v))
        .collect::<Result<Vec<_>, _>>()?;
    if models.is_empty() || models.len() != templates.len() {
        return Err(Error::source(
            at,
            format!(
                "a dropped building names one template for each model: {} model(s), {} template(s)",
                models.len(),
                templates.len()
            ),
        ));
    }
    let [x, y, z] = &r.at[..] else {
        return Err(Error::source(
            at,
            "at is [x, y, z], z a height or \"ground\"",
        ));
    };
    let z = match z {
        toml::Value::String(s) if s == "ground" => Height::Ground,
        other => Height::At(number(at, other, "z")?),
    };
    let furnished = match &r.furnished {
        None => Furnished::Template,
        Some(toml::Value::String(s)) if s == "bare" => Furnished::Bare,
        Some(v) => {
            let like = one_or_many(v)
                .into_iter()
                .map(|v| site(at, v))
                .collect::<Result<Vec<_>, _>>()?;
            if like.len() != templates.len() {
                return Err(Error::source(
                    at,
                    "furnished names \"bare\", or one retail building for each template",
                ));
            }
            Furnished::Like(like)
        }
    };
    let house = match &r.house {
        None => Vec::new(),
        Some(v) => {
            let list = one_or_many(v)
                .into_iter()
                .map(|v| {
                    v.as_str()
                        .ok_or_else(|| Error::source(at, "a house is a spawn's symbol"))
                        .and_then(|s| house_symbol(at, pack, s))
                })
                .collect::<Result<Vec<_>, _>>()?;
            if list.len() != models.len() {
                return Err(Error::source(
                    at,
                    format!(
                        "a house names the spawn of each building: {} building(s), {} house(s)",
                        models.len(),
                        list.len()
                    ),
                ));
            }
            list
        }
    };
    Ok(AddBuilding {
        name,
        models,
        templates,
        at: Place {
            x: number(at, x, "x")?,
            y: number(at, y, "y")?,
            z,
        },
        rotation: rotation(at, r.heading, r.rotation.as_deref())?.unwrap_or(Rotation::Heading(0.0)),
        furnished,
        house,
    })
}

/// A cell an operation names: a number of `0x0100` on, or a pack cell's symbol.
fn cell_ref(at: &Loc, pack: &str, v: &toml::Value) -> Result<CellRef, Error> {
    match v {
        toml::Value::Integer(n) => u16::try_from(*n)
            .ok()
            .filter(|n| *n >= crate::cells::FIRST_CELL)
            .map(CellRef::Number)
            .ok_or_else(|| Error::source(at, format!("{n:#X} is not an indoor cell's number"))),
        toml::Value::String(s) => symbol::qualify(pack, s)
            .map(CellRef::Symbol)
            .map_err(|why| Error::source(at, why)),
        _ => Err(Error::source(
            at,
            "a cell is its number or a pack cell's symbol",
        )),
    }
}

/// One file of `pack` that sets `what` twice, at `first` and `second`: a conflict only the pack can
/// settle.
fn twice(pack: &str, what: String, first: &Loc, second: &Loc, sets: &str) -> Error {
    Error::Conflicts(vec![crate::conflict::Conflict {
        what,
        landblocks: std::collections::BTreeSet::new(),
        place: None,
        claims: [first, second]
            .into_iter()
            .map(|at| crate::conflict::Claim::new(pack, at, sets, at.line.to_string()))
            .collect(),
    }])
}

/// Read the pack in `dir`.
///
/// # Errors
/// A file that does not read, or a record or operation the format does not allow.
pub fn read_pack(dir: &Path) -> Result<Pack, Error> {
    let meta_path = dir.join("pack.toml");
    let text = read_text(&meta_path)?;
    let meta = dereth_pack::pack::PackHeader::parse::<crate::dialect::Ac1>(&meta_path, &text)?;
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
        provenance: Rows::default(),
    };
    for path in files_in(&dir.join("records"), "toml")? {
        let mut rows = Rows::default();
        let records = read_records(&pack, &path, &mut rows)?;
        pack.provenance.extend(rows);
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
